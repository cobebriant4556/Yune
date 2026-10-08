# Studio 0.735 reconstruction audit

This is an evidence/status report, **not a completed renderer port**. Yune's current renderer and audio DSP are independent implementations. Passing their regression tests does not establish Studio pixel or sample parity. The requested target is source-traceable reconstruction of Studio behavior, not another approximate lighting model.

## What was actually supplied and inspected

The uploaded `0.735-full(4).rar` contains 22,704 entries: 22,547 regular files, all with `.c` extensions, and 157 directories. Its listings identify themselves as `0.735 Studio, Mac symbols, 2026-08`. That build attribution has not been verified against an executable. File sizes, SHA-256 digests and inspected-function counts are recorded in [studio-0735-reference.json](studio-0735-reference.json).

The inspected files are decompiler-generated C-like listings, not an original buildable source tree. They contain recovered function names and addresses, register temporaries, unresolved global addresses and incomplete recovered types. For example, `LightingParameters.c` explicitly says field names came from a 2022 PDB and distinguishes corroborated fields from `UNVERIFIED` and `SIZE CONFLICT` annotations. Those annotations must not be treated as established 0.735 layouts.

No separate headers, build project files, executable, or shader packs are present in the archive inventory. This inventory check does not establish whether every constant or embedded resource could eventually be recovered from the listings. Such recovery has not been completed.

## Lighting: the existing renderer is not the Studio pipeline

`crates/yune-render/src/raster.rs` uses Yune's own directional-light/material/fog calculations. It is a software preview renderer. A `Lighting` instance and similarly named properties do not make those calculations a port of Studio. Adding more approximate lights would not satisfy a full-port requirement.

Concrete evidence from the supplied dump:

| Source | Observed dependency / behavior |
| --- | --- |
| `rbx/ShaderManager.c`, instructions at `0x10524da77` and `0x10524da8a` | Constructs an external shader-pack filename from `shaders_`, a supplied suffix and `.pack`. Porting the loader does not supply the shader programs it loads. |
| `rbx/LightingParameters.c`, `setTime`, `0x103210916` | Recovered lighting/time calculations reference unresolved constant addresses, including `xmmword_*` data. The full required constant contents have not been recovered. |
| `rbx/LightingQualityManager.c`, `0x104471190` onward | Tunable initialization and quality-dependent selection for lighting, sun/local shadows, environment-map distances, light-grid radius and budgets. A single user-facing graphics setting cannot be substituted for all of this state. |
| `rbx/VisualEngine.c` | 99 recovered function listings, rather than a standalone shader or self-contained renderer. Its device/resources and rendering subsystems also need reconstruction. |

The archive inventory also contains `GeometryGenerator.c`, `FastClusterMeshGenerator.c`, `MaterialGenerator.c`, `LightGridCPU.c`, `LightGridUnified.c`, `ShadowMapSystemImpl.c`, `ShadowMapSystemImplNew.c`, `ShaderMtl.c`, `ShaderProgramMtl.c`, `ShaderGL.c` and `ShaderProgramGL.c`. Their presence identifies additional implementation work; it does not mean those subsystems are already ported to Yune.

Necessary next inputs are the **matching Studio executable**, to resolve binary data and verify recovered layouts/control flow, and its matching **shader packs/resources**. Original headers/build definitions, if available, would further reduce reconstruction work. A shader pack from another build must not be assumed equivalent. Acquiring these inputs is only a prerequisite, not a claim that the full GPU renderer can then be copied in unchanged.

The native/software preview remains available. It must not be used as a Studio graphics-level-9 reference. Lighting replacement is **not completed by this change**.

## EditableMesh: trace the actual data path before claiming a full port

The bound EditableMesh branch in Yune's current rasterizer supplies positions but no corner UVs, normals, colors or texture maps. This is a genuine implementation gap, not an asset-export-only issue.

Relevant functions located in the supplied listings:

| Recovered function | Address | Why it matters |
| --- | --- | --- |
| `EditableMeshData::attributeTypeForID` | `0x102832e78` | Attribute type is encoded in the ID; Yune's existing overlapping vertex/face counters are not this slot-map implementation. |
| `EditableMeshData::findOrCreateCornerAttrInds` | `0x10283451c` | Reuses or creates corner attribute indices associated with a position. Attributes cannot simply be attached to a position with no provision for seams. |
| `EditableMesh::setVector` | `0x10557747a` | Checks the number of supplied attributes against the face and updates corners through attribute-specific callbacks. |
| `EditableMesh::addNormal` / `setNormal` / `resetNormal` | `0x105575bb2` / `0x105576c3c` / `0x105576cdc` | Distinguishes manually provided normals from automatically generated normals. |
| `EditableMeshData::computeGeometricNormal` / `refreshNormals` | `0x102831e0e` / `0x10283b09e` | Geometry-dependent normal calculation and refreshing non-manual normals; replacing these with one face normal loses behavior. |
| `ColorAttr::setColor` | `0x105575e28` | Packs colors into bytes. Visible scalar lanes multiply by 256, floor and cap at 255. SIMD constants and out-of-range behavior still need verification; generic `round(value * 255)` is not a demonstrated port. |
| `EditableMeshData::getQuadSplit` | `0x102836ba4` | Calls `computeQuadSplit`; arbitrary fan triangulation is not established as equivalent. |

The UV/color/normal storage, typed/stable IDs, attribute sharing, mutation propagation, image binding, mesh generation and material/shader consumption need to be connected together. Implementing the public setters alone is not a full EditableMesh rendering port. No new replacement EditableMesh implementation is being claimed in this audit.

Public API contracts are useful cross-checks, but are not implementation source: https://create.roblox.com/docs/reference/engine/classes/EditableMesh

## Source-derived and verified are separate statuses

For future reconstructed functions, record the source path, source-file digest, symbol/address, recovered dependencies, unresolved assumptions, runtime integration and tests. Distinguish an original-source port, a decompilation reconstruction, a public-contract implementation, and a preview approximation. Never label the last two as the first.

Verification against the original executable needs controlled fixtures and recorded build/settings/assets. Current synthetic tests establish internal consistency, not equality with Studio. The same distinction applies to Yune's independently implemented AudioPlayer effects.

## Build-selection bug corrected separately

Both audio CLI regression scripts and the smoke harness now use `scripts/yune_binary.py`. They accept `--profile debug|release`, `--binary PATH`, `--target-dir PATH` and `--target TRIPLE`. Environment equivalents are `YUNE_PROFILE`, `YUNE_BIN`, `CARGO_TARGET_DIR` and `CARGO_BUILD_TARGET`.

An explicit binary wins. Explicit build-selection arguments take precedence over an inherited `YUNE_BIN`. With no selection, a release-only or debug-only checkout works; when both exist, the test refuses to guess which build is intended. An explicitly selected missing binary/profile never silently falls back to another build. Paths containing spaces are passed as individual subprocess arguments.

The smoke harness passes its selected absolute executable to child CLI tests and records it in `test-results/executable.json`. CI explicitly selects the debug build that it builds. Resolver unit tests exercise release paths, selection precedence, custom target directories, missing builds and ambiguous/stale-build prevention. These unit tests do not substitute for running an optimized release binary.

```text
cargo build -p yune --release
python scripts/run_smoke.py --profile release
python scripts/audio_cli_regression.py --profile release
python scripts/audio_spatial_cli_regression.py --profile release
```

This change does not export BoB/Mario assets, integrate SM64Sound, validate the user's existing Lune patches, or complete the Studio renderer. Those remain separate work, not implicit successes.
