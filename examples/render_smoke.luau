local render = require("@yune/render")
local assetService = game:GetService("AssetService")

local camera = Instance.new("Camera")
camera.CFrame = CFrame.lookAt(Vector3.new(8, 6, 10), Vector3.new(0, 0.5, 0))
camera.FieldOfView = 70
camera.Parent = workspace

local floor = Instance.new("Part")
floor.Size = Vector3.new(12, 1, 12)
floor.Color = Color3.fromRGB(245, 205, 48)
floor.CFrame = CFrame.new(0, -1, 0)
floor.Parent = workspace

local meshPart = Instance.new("Part")
meshPart.Color = Color3.fromRGB(80, 170, 255)
meshPart.CFrame = CFrame.new(0, 0, 0)
meshPart.Parent = workspace

local mesh = assetService:CreateEditableMesh()
local v1 = mesh:AddVertex(Vector3.new(-2, 0, -1))
local v2 = mesh:AddVertex(Vector3.new(2, 0, -1))
local v3 = mesh:AddVertex(Vector3.new(0, 3, -1))
local v4 = mesh:AddVertex(Vector3.new(0, 1, 2))
mesh:AddTriangle(v1, v2, v3)
mesh:AddTriangle(v1, v4, v2)
mesh:AddTriangle(v2, v4, v3)
mesh:AddTriangle(v3, v4, v1)
render.bindEditableMesh(meshPart, mesh)

local gui = Instance.new("ScreenGui")
local frame = Instance.new("Frame")
frame.Position = UDim2.fromOffset(24, 24)
frame.Size = UDim2.fromOffset(330, 80)
frame.BackgroundColor3 = Color3.fromRGB(30, 30, 34)
frame.BackgroundTransparency = 0.08
frame.Parent = gui

local label = Instance.new("TextLabel")
label.Size = UDim2.fromScale(1, 1)
label.BackgroundTransparency = 1
label.Text = "Yune headless render"
label.TextColor3 = Color3.fromRGB(255, 255, 255)
label.TextSize = 24
label.Parent = frame

render.capture({
    world = workspace,
    camera = camera,
    gui = gui,
    width = 960,
    height = 540,
    clearColor = Color3.fromRGB(120, 180, 235),
    path = "yune-smoke.png",
})
