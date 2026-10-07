local runtime = require("@yune/runtime")
local storage = game:GetService("ReplicatedStorage")

local function module(name, source)
    local item = Instance.new("ModuleScript")
    item.Name = name
    item.Source = source
    item.Parent = storage
    return item
end

local slow = module("Concurrent", [[
_G.YUNE_REQUIRE_COUNT = (_G.YUNE_REQUIRE_COUNT or 0) + 1
task.wait(0.01)
return { value = 42, owner = script }
]])
local results = {}
for index = 1, 8 do
    task.spawn(function()
        results[index] = require(slow)
    end)
end
for _ = 1, 200 do
    if #results == 8 then break end
    task.wait(0.005)
end
assert(#results == 8, "concurrent require did not complete")
assert(_G.YUNE_REQUIRE_COUNT == 1, "module executed more than once")
for index = 1, 8 do
    assert(results[index] == results[1], "module result identity was not preserved")
    assert(results[index].owner == slow, "module script environment was incorrect")
end
assert(require(slow) == results[1])
assert(require(module("FalseResult", "return false")) == false)
local bad = module("BadReturn", "return 1, 2")
local ok, message = pcall(require, bad)
assert(not ok and string.find(tostring(message), "exactly one value"))

local a = module("CycleA", "return require(script.Parent.CycleB)")
module("CycleB", "return require(script.Parent.CycleA)")
local cycleOk, cycleMessage = pcall(require, a)
assert(not cycleOk and string.find(tostring(cycleMessage), "cyclic"), "cycle diagnostic was missing")

local scriptObject = Instance.new("LocalScript")
scriptObject.Name = "Lifecycle"
scriptObject.Source = [[
_G.YUNE_LIFECYCLE_STARTS = (_G.YUNE_LIFECYCLE_STARTS or 0) + 1
task.wait(0.05)
_G.YUNE_LIFECYCLE_FINISHES = (_G.YUNE_LIFECYCLE_FINISHES or 0) + 1
]]
scriptObject.Parent = game:GetService("ReplicatedFirst")
runtime.step(1 / 60)
task.wait(0.01)
assert(_G.YUNE_LIFECYCLE_STARTS == 1)
scriptObject.Enabled = false
runtime.step(1 / 60)
task.wait(0.07)
assert(_G.YUNE_LIFECYCLE_FINISHES == nil, "disabled script resumed")
scriptObject.Enabled = true
runtime.step(1 / 60)
task.wait(0.07)
assert(_G.YUNE_LIFECYCLE_STARTS == 2 and _G.YUNE_LIFECYCLE_FINISHES == 1, "reenabled script did not restart")
scriptObject:Destroy()
runtime.step(1 / 60)
print("PASS require concurrency, identity, return validation, cycles, lifecycle restart")
