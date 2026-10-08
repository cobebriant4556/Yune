local runtime = require("@yune/runtime")
local render = require("@yune/render")
local lighting = game:GetService("Lighting")
lighting.Ambient = Color3.new(0.5, 0.75, 0)
lighting.OutdoorAmbient = Color3.new(0.75, 0.5, 1)
lighting.Brightness = 0
lighting.FogStart = 4
lighting.FogEnd = 12
local camera = workspace.CurrentCamera
camera.CFrame = CFrame.new(0, 0, 5)
lighting.FogStart = 100
lighting.FogEnd = 200
local part = Instance.new("Part")
part.Anchored = true
part.Color = Color3.new(1, 1, 1)
part.Size = Vector3.new(4, 4, 1)
part.Parent = workspace
local function center()
    local image = render.capture({width = 32, height = 32, gui = {}, includePixels = true})
    local offset = (16 * 32 + 16) * 4
    return buffer.readu8(image.pixels, offset), buffer.readu8(image.pixels, offset + 1), buffer.readu8(image.pixels, offset + 2)
end
local r, g, b = center()
assert(r == 143 and g == 143 and b == 255, tostring(r) .. "," .. tostring(g) .. "," .. tostring(b))
lighting.Ambient = Color3.new(0.5, 0.5, 0.5)
lighting.OutdoorAmbient = Color3.new(0.5, 0.5, 0.5)
r, g, b = center()
assert(r == 64 and g == 64 and b == 64)
lighting.FogStart = 0
lighting.FogEnd = 0
lighting.FogColor = Color3.new(1, 0, 0)
r, g, b = center()
assert(r == 255 and g == 0 and b == 0)
print("PASS source-derived ambient/outdoor split and fog constants, live property updates and finite equal-boundary raster output")
