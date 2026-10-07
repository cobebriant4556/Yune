local runtime = require("@yune/runtime")
local render = require("@yune/render")
render.setAssetRoot("test-results/assets")

local world = Instance.new("Folder")
local camera = Instance.new("Camera")
camera.CFrame = CFrame.new(0, 0, 8)
local mesh = Instance.new("MeshPart")
mesh.MeshId = "http://www.roblox.com/asset/?id=123"
mesh.Size = Vector3.new(4, 4, 1)
mesh.CFrame = CFrame.new()
mesh.Parent = world
local gui = Instance.new("ScreenGui")
local image = Instance.new("ImageLabel")
image.Size = UDim2.fromScale(1, 1)
image.BackgroundTransparency = 1
image.Image = "rbxassetid://456"
image.Parent = gui

local result = render.capture({world = world, camera = camera, gui = gui, width = 64, height = 64, includePixels = true})
assert(result.triangles == 1, "MeshPart was not decoded as one triangle")
assert(#result.assetErrors == 0)
assert(buffer.len(result.pixels) == 64 * 64 * 4)
local center = (32 * 64 + 32) * 4
assert(buffer.readu8(result.pixels, center) == 255 and buffer.readu8(result.pixels, center + 1) == 0, "numeric image asset was not decoded by its file signature")

image.Image = "http://www.roblox.com/asset/?id=456"
assert(#render.capture({world = world, camera = camera, gui = gui, width = 32, height = 32}).assetErrors == 0)
mesh.MeshId = "rbxassetid://999999"
local ok, message = pcall(render.capture, {world = world, camera = camera, gui = {}, width = 32, height = 32})
assert(not ok and string.find(tostring(message), "999999"), "missing mesh silently passed strict capture")
local diagnostic = render.capture({world = world, camera = camera, gui = {}, width = 32, height = 32, strictAssets = false})
assert(#diagnostic.assetErrors > 0)
print("PASS automatic mesh/image lookup, ID aliases, signature decoding, RGBA readback, strict missing-asset errors")
