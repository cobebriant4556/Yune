local runtime = require("@yune/runtime")
local render = require("@yune/render")
local assets = game:GetService("AssetService")
local world = Instance.new("Folder")
local camera = Instance.new("Camera")
camera.CFrame = CFrame.new(0, 0, 8)

local function capture(gui)
    return render.capture({world = world, camera = camera, gui = gui, width = 64, height = 64, clearColor = Color3.new(0, 0, 0), includePixels = true})
end

local function pixel(result, x, y)
    local offset = (y * result.width + x) * 4
    return buffer.readu8(result.pixels, offset), buffer.readu8(result.pixels, offset + 1), buffer.readu8(result.pixels, offset + 2)
end

local function expect(result, x, y, r, g, b, label)
    local ar, ag, ab = pixel(result, x, y)
    assert(ar == r and ag == g and ab == b, label .. ": " .. ar .. "," .. ag .. "," .. ab)
end

local gui = Instance.new("ScreenGui")
local parent = Instance.new("Frame")
parent.Position = UDim2.fromOffset(4, 4)
parent.Size = UDim2.fromOffset(8, 8)
parent.BackgroundColor3 = Color3.fromRGB(0, 0, 255)
parent.BorderSizePixel = 0
parent.ClipsDescendants = true
parent.Parent = gui
local scale = Instance.new("UIScale")
scale.Scale = 2
scale.Parent = parent
local child = Instance.new("Frame")
child.Position = UDim2.fromOffset(3, 3)
child.Size = UDim2.fromOffset(8, 8)
child.BackgroundColor3 = Color3.fromRGB(255, 0, 0)
child.BorderSizePixel = 0
child.Parent = parent
local result = capture(gui)
expect(result, 9, 9, 0, 0, 255, "ancestor UIScale offset")
expect(result, 10, 10, 255, 0, 0, "scaled child position")
expect(result, 19, 19, 255, 0, 0, "clip inside")
expect(result, 20, 20, 0, 0, 0, "clip outside")
assert(child.AbsoluteSize == Vector2.new(16, 16))
assert(child.AbsolutePosition == Vector2.new(10, 10))
gui.Enabled = false
expect(capture(gui), 10, 10, 0, 0, 0, "disabled ScreenGui")

local textGui = Instance.new("ScreenGui")
local text = Instance.new("TextLabel")
text.Size = UDim2.fromOffset(8, 8)
text.BackgroundTransparency = 1
text.Text = "F"
text.TextSize = 8
text.TextColor3 = Color3.new(1, 1, 1)
text.TextXAlignment = Enum.TextXAlignment.Left
text.TextYAlignment = Enum.TextYAlignment.Top
text.Parent = textGui
local glyph = capture(textGui)
expect(glyph, 1, 6, 255, 255, 255, "bitmap glyph stem")
expect(glyph, 6, 6, 0, 0, 0, "bitmap glyph must not be mirrored")

local imageGui = Instance.new("ScreenGui")
local image = Instance.new("ImageLabel")
image.Size = UDim2.fromOffset(16, 8)
image.BackgroundTransparency = 1
image.Parent = imageGui
local bitmap = assets:CreateEditableImage({Size = Vector2.new(2, 1)})
bitmap:DrawRectangle(Vector2.new(0, 0), Vector2.new(1, 1), Color3.fromRGB(255, 0, 0), 0)
bitmap:DrawRectangle(Vector2.new(1, 0), Vector2.new(1, 1), Color3.fromRGB(0, 0, 255), 0)
render.bindEditableImage(image, bitmap)
image.ImageRectOffset = Vector2.zero
image.ImageRectSize = Vector2.new(1, 1)
image.ResampleMode = Enum.ResamplerMode.Default
local cropped = capture(imageGui)
expect(cropped, 0, 4, 255, 0, 0, "atlas crop first edge")
expect(cropped, 15, 4, 255, 0, 0, "atlas crop last edge")

image.ImageRectSize = Vector2.zero
image.ResampleMode = Enum.ResamplerMode.Pixelated
image.ScaleType = Enum.ScaleType.Tile
image.TileSize = UDim2.fromOffset(4, 4)
local tiled = capture(imageGui)
expect(tiled, 0, 2, 255, 0, 0, "tile first color")
expect(tiled, 2, 2, 0, 0, 255, "tile second color")
expect(tiled, 4, 2, 255, 0, 0, "TileSize repeat")
expect(tiled, 6, 2, 0, 0, 255, "TileSize second repeat")

local slicedImage = assets:CreateEditableImage({Size = Vector2.new(3, 3)})
slicedImage:DrawRectangle(Vector2.zero, Vector2.new(3, 3), Color3.fromRGB(0, 255, 0), 0)
slicedImage:DrawRectangle(Vector2.zero, Vector2.new(1, 1), Color3.fromRGB(255, 0, 0), 0)
slicedImage:DrawRectangle(Vector2.new(2, 0), Vector2.new(1, 1), Color3.fromRGB(0, 0, 255), 0)
render.bindEditableImage(image, slicedImage)
image.Size = UDim2.fromOffset(8, 8)
image.ScaleType = Enum.ScaleType.Slice
image.SliceCenter = Rect.new(1, 1, 2, 2)
image.SliceScale = 2
local sliced = capture(imageGui)
expect(sliced, 1, 1, 255, 0, 0, "nine-slice left corner")
expect(sliced, 6, 1, 0, 0, 255, "nine-slice right corner")
expect(sliced, 4, 4, 0, 255, 0, "nine-slice stretched center")
print("PASS inherited UIScale, clipping, disabled GUI, glyph orientation, atlas filtering, TileSize and nine-slice pixels")
