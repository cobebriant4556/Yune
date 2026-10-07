# Yune

Yune is a headless Luau runtime aimed at deterministic Roblox-style scene execution and rendering for automated testing, visual comparison, and tooling.

It embeds the public Lune runtime, exposes Lune's Roblox datatypes and Instance DOM as Roblox-style globals, and adds a renderer that can run without Studio or a display server.

## Current milestone

The first milestone is deliberately small enough to stay testable:

- Lune Luau runtime and task scheduler
- `game`, `workspace`, `Instance`, `Vector2`, `Vector3`, `CFrame`, `Color3`, `UDim`, `UDim2`, and `Enum` globals
- deterministic offscreen RGBA + depth framebuffer
- `Part`/`BasePart` block rendering
- `Camera` projection
- simple ambient + directional lighting
- `ScreenGui`, `Frame`, `TextLabel`, `TextButton`, `ImageLabel`, and `ImageButton`
- `ViewportFrame` with its own camera and child 3D scene
- mutable `EditableMesh` with stable vertex/face IDs
- mutable `EditableImage` with rectangle/line/circle drawing
- `AssetService:CreateEditableMesh()` and `AssetService:CreateEditableImage()`
- local image binding for GUI testing
- PNG capture from Luau

Yune is not Roblox Studio and is not intended to contact Roblox services. The goal is a deterministic, scriptable recreation of the runtime/rendering behaviors that matter for testing.

## Example

```luau
local assetService = game:GetService("AssetService")
local render = require("@yune/render")

local camera = Instance.new("Camera")
camera.CFrame = CFrame.lookAt(Vector3.new(8, 6, 10), Vector3.zero)
camera.FieldOfView = 70
camera.Parent = workspace

local part = Instance.new("Part")
part.Size = Vector3.new(5, 1, 5)
part.Color = Color3.fromRGB(245, 205, 48)
part.CFrame = CFrame.new(0, 0, 0)
part.Parent = workspace

local mesh = assetService:CreateEditableMesh()
local a = mesh:AddVertex(Vector3.new(-1, 0, 0))
local b = mesh:AddVertex(Vector3.new(1, 0, 0))
local c = mesh:AddVertex(Vector3.new(0, 2, 0))
mesh:AddTriangle(a, b, c)
render.bindEditableMesh(part, mesh)

render.capture({
    world = workspace,
    camera = camera,
    width = 1280,
    height = 720,
    path = "frame.png",
})
```

## Build

A recent Rust toolchain with Edition 2024 support is required.

```text
cargo build --release
cargo run --release -- run examples/render_smoke.luau
```

The Lune dependency is pinned to a specific upstream commit so Yune does not silently change underneath its renderer.
