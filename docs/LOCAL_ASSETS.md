# Running with local assets

Yune never downloads assets from a Roblox account. Use files you are authorized to read locally.

A place containing `rbxassetid://123` can find its mesh automatically under an asset root as `123`, `123.mesh` or `meshes/123.mesh`. Images can be stored as numeric filenames with no extension; decoding uses the file signature. The current decoder build supports PNG. A URI such as `rbxasset://textures/example.png` resolves below the configured root or its `content` subdirectory.

```luau
local runtime = require("@yune/runtime")
local render = require("@yune/render")
render.setAssetRoot("assets")
render.registerAsset("rbxassetid://123", "assets/mario.mesh")
render.registerAsset("rbxassetid://456", "assets/mario.png")
```

`registerAsset` records a path and loads the file lazily when rendering needs it. `registerMesh` and `registerImage` remain available for eager parsing. Numeric IDs and legacy `http://www.roblox.com/asset/?id=...` IDs share a cache key. Files discovered relative to the asset root cannot escape it through `..` or symlinks. Explicit registrations select the exact file supplied by the caller.

`setAssetRoot` clears previously decoded mesh/image caches. Register explicit decoded assets after choosing the root. Explicit path registrations persist until the process exits or are replaced for the same ID.

```luau
local result = render.capture({
    world = workspace,
    camera = workspace.CurrentCamera,
    width = 960,
    height = 540,
    strictAssets = true,
    includePixels = true,
    path = "frame.png",
})
assert(buffer.len(result.pixels) == 960 * 540 * 4)
```

RGBA bytes are top-to-bottom, left-to-right, with four bytes per pixel. `gui = {}` disables screen GUI capture; omitting `gui` uses CoreGui and the local PlayerGui.

Missing or malformed referenced assets cause an error by default, before a PNG is saved. `strictAssets = false` allows an exploratory capture and returns `result.assetErrors`; an unresolved mesh may still show a box placeholder in that mode. Do not use permissive captures as fidelity references.

For vector text, call `render.registerFont(familyId, localFontPath)` with a locally available TTF/OTF before capture. No font files are included or uploaded by Yune's tests. The first registered face supplies a deterministic fallback when a requested family is not registered; this is not exact Roblox font matching.
