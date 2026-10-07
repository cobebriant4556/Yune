local runtime = require("@yune/runtime")
local render = require("@yune/render")
assert(Instance.new("Motor6D").Transform == CFrame.identity, "Motor6D has no identity Transform default")
assert(Instance.new("Bone").Transform == CFrame.identity, "Bone has no identity Transform default")
local world = Instance.new("Folder")
local camera = Instance.new("Camera")
local gui = Instance.new("ScreenGui")
local holder = Instance.new("Frame")
holder.Size = UDim2.fromOffset(0, 0)
holder.Position = UDim2.fromOffset(8, 8)
holder.BackgroundTransparency = 1
holder.Parent = gui
local child = Instance.new("Frame")
child.Size = UDim2.fromOffset(4, 4)
child.Position = UDim2.fromOffset(2, 2)
child.BackgroundColor3 = Color3.fromRGB(255, 0, 0)
child.BorderSizePixel = 0
child.Parent = holder
local function capture()
    return render.capture({world = world, camera = camera, gui = gui, width = 32, height = 32, clearColor = Color3.new(0, 0, 0), includePixels = true})
end
local visible = capture()
assert(buffer.readu8(visible.pixels, (11 * 32 + 11) * 4) == 255, "zero-size container hid offset-based children")
holder.ClipsDescendants = true
local clipped = capture()
assert(buffer.readu8(clipped.pixels, (11 * 32 + 11) * 4) == 0, "zero-size clipping container did not clip children")
print("PASS pose property defaults and zero-size GUI container behavior")
