local runtime = require("@yune/runtime")
local render = require("@yune/render")

local camera = workspace.CurrentCamera
camera.CFrame = CFrame.lookAt(Vector3.new(0, 5, 14), Vector3.new(0, 2, 0))
camera.FieldOfView = 70

render.registerMesh("yune://triangle", "examples/assets/triangle.mesh")

local meshPart = Instance.new("MeshPart")
meshPart.Name = "TriangleMesh"
meshPart.MeshId = "yune://triangle"
meshPart.Size = Vector3.new(4, 4, 1)
meshPart.Color = Color3.fromRGB(80, 170, 255)
meshPart.CFrame = CFrame.new(-3, 1, 0)
meshPart.Anchored = true
meshPart.Parent = workspace

local billboardAnchor = Instance.new("Part")
billboardAnchor.Name = "BillboardAnchor"
billboardAnchor.Size = Vector3.new(1, 1, 1)
billboardAnchor.Transparency = 1
billboardAnchor.Anchored = true
billboardAnchor.CanCollide = false
billboardAnchor.CFrame = CFrame.new(0, 3, 0)
billboardAnchor.Parent = workspace

local billboard = Instance.new("BillboardGui")
billboard.Adornee = billboardAnchor
billboard.Size = UDim2.fromOffset(220, 64)
billboard.AlwaysOnTop = false
billboard.Parent = billboardAnchor

local billboardFrame = Instance.new("Frame")
billboardFrame.Size = UDim2.fromScale(1, 1)
billboardFrame.BackgroundColor3 = Color3.fromRGB(30, 30, 36)
billboardFrame.BackgroundTransparency = 0.1
billboardFrame.Parent = billboard

local billboardText = Instance.new("TextLabel")
billboardText.Size = UDim2.fromScale(1, 1)
billboardText.BackgroundTransparency = 1
billboardText.Text = "BillboardGui"
billboardText.TextColor3 = Color3.fromRGB(255, 255, 255)
billboardText.TextSize = 20
billboardText.Parent = billboardFrame

local panel = Instance.new("Part")
panel.Name = "SurfacePanel"
panel.Size = Vector3.new(6, 4, 0.5)
panel.CFrame = CFrame.new(3, 1, 0)
panel.Anchored = true
panel.Color = Color3.fromRGB(60, 60, 70)
panel.Parent = workspace

local surface = Instance.new("SurfaceGui")
surface.Adornee = panel
surface.Face = Enum.NormalId.Front
surface.CanvasSize = Vector2.new(600, 400)
surface.AlwaysOnTop = false
surface.Parent = panel

local surfaceFrame = Instance.new("Frame")
surfaceFrame.Size = UDim2.fromScale(1, 1)
surfaceFrame.BackgroundColor3 = Color3.fromRGB(255, 220, 70)
surfaceFrame.Parent = surface

local surfaceText = Instance.new("TextLabel")
surfaceText.Size = UDim2.fromScale(1, 1)
surfaceText.BackgroundTransparency = 1
surfaceText.Text = "SurfaceGui"
surfaceText.TextColor3 = Color3.fromRGB(20, 20, 20)
surfaceText.TextSize = 30
surfaceText.Parent = surfaceFrame

runtime.step(1 / 60)

local result = render.capture({
    world = workspace,
    camera = camera,
    width = 960,
    height = 540,
    path = "yune-visual-smoke.png",
})

assert(result.triangles >= 1, "MeshPart geometry did not render")
assert(result.guiObjects >= 4, "BillboardGui/SurfaceGui children did not render")
print("Yune visual smoke passed")
