# Studio 0.735 lighting and shading: full-archive search

This follow-up expands [STUDIO_PORT_AUDIT.md](STUDIO_PORT_AUDIT.md). The earlier targeted audit did not adequately distinguish the substantial recovered lighting implementation already available from the separate question of recovering complete GPU shader payloads. An unresolved shader payload is not a reason to postpone reconstruction of all CPU-side lighting and rendering components.

This is a source investigation, not a completed renderer port. No runtime or renderer behavior is changed by this document. The inspected archive identifies itself as Studio 0.735 with Mac symbols; that attribution has not been verified against a matching executable. The files are decompiler-generated listings with unresolved addresses and some visibly unreliable recovered types/calls, not an original buildable project.

## Scope

Archive: `0.735-full(4).rar`.

SHA-256: `899b78bd9dd981a20126bd87aec545b017b8409876ef37824a0c91d4da8d908d`.

All **22,547 regular files** were extracted and included in full-content searches: **1,462,056,243 bytes**, including **9,234 files under `_external`**. The archive also has 157 directory entries. Every regular entry has a `.c` filename, so searches inspected file contents for shader text, shader signatures, C arrays and resource registrations rather than assuming the extension described all embedded content.

Each extracted size matched the inventory, and all eleven hashes from the earlier selected-file audit matched. Original encrypted-file CRCs were not independently verified. No uploaded code was executed. The search combined automated whole-content scanning with targeted manual examination; it was not manual review of every line.

A separate downloadable search bundle was prepared with 144 full, unmodified selected listings, their SHA-256 hashes, and 5,168 indexed recovered function entries. That index includes wrappers and initializers, not 5,168 completed lighting ports. It also records 285 distinct literal shader-identifier candidates, not 285 recovered shader programs. The source bundle and full TSV/CSV indices are separate deliverables, not files added to this repository by this documentation commit.

## Actual lighting implementation found

All line numbers below refer to the original extracted files, relative to `0.735-full/`.

| Component | Locations | Recovered implementation |
| --- | --- | --- |
| Renderer lighting handoff | `rbx/RenderView.c:7175` (`updateLighting`), `:7488` (`presetLighting`), `:7592–7612` | Computation and transfer of lighting colors/direction and environment values into SceneManager, rather than property declarations alone. |
| Shader-state preparation | `rbx/SceneManager.c:3369` (`setLighting`), `:3407` (`setIBLData`), `:3456` (`setFog`); `rbx/GlobalShaderData.c` | Lighting, environment and fog state supplied to rendering. Exact recovered field labels still require verification. |
| Time and quality | `rbx/LightingParameters.c:23–235`; `rbx/LightingQualityManager.c:76,181,226,323,357,374,410`; `rbx/FrameRateManager.c:1513,2913,3708` | Time-dependent lighting calculations, quality selection, shadow distances/softness and light-grid budgets. Some constant addresses remain unresolved. |
| CPU light grid | `rbx/LightGridCPU.c` | 13,027 lines and 115 recovered function entries covering occupancy, global/skylight propagation, local lights, shadow masks, filtering/encoding and upload. |
| Local-light calculations | `rbx/LightGridCPU.c:5475,5608,5732,5929,6107` | Actual SIMD method bodies for point, spot and surface-light contributions. |
| Frustum light culling | `rbx/FroxelGrid.c:94,226,913,1123,1726,2106` | Shader selection, GPU-light-data upload, CPU culling, GPU dispatch and updates. CPU culling is present separately from the GPU-dispatch path. |
| Combined light-grid path | `rbx/LightGridUnified.c:64–144`; `rbx/SceneUpdater.c:5379,5725` | Underlying grid, optional FroxelGrid, quality decisions and shadow-system integration. |
| Shadows | `rbx/ShadowMapSystemImpl.c:723,891,5817,5823`; `rbx/ShadowMapSystemImplNew.c:5,608`; directional/point/spot ShadowObject variants | Recovered preparation and rendering passes. Both old/new branches exist; that does not establish which branch was active in a given reference capture. |

Especially useful entry points:

- `LightGridCPU::lightingUpdateChunkGlobal`: line 870, `0x10511ad50`.
- `LightGridCPU::lightingUpdateSkylight`: line 997, `0x10511b1c8`.
- `LightGridCPU::lightingUpdatePointLightSIMD<true>`: line 5475, `0x1051209c6`.
- `LightGridCPU::lightingUpdateSpotLightSIMD<true>`: line 5732, `0x105120e5a`.
- `LightGridCPU::lightingUpdateSurfaceLightSIMD<false>`: line 6107, `0x10512150e`.
- `LightGridCPU::lightingBlurAndEncode`: line 7674, `0x105122bf0`.
- `FroxelGrid::cullLightsCPU`: line 1123, `0x1050a95f8`.
- `FroxelGrid::cullLightsGPU`: line 1726, `0x1050aa386`.

These are meaningful source-derived reconstruction targets. They do not establish complete dependency recovery or Studio-pixel parity.

## Materials, shading pipeline, image processing and geometry

| Component | Locations | What is present |
| --- | --- | --- |
| Material/shader selection | `rbx/MaterialGenerator.c:9744` (`generateShaderSettingsUnified`), `:10356` (`setupTechniquesWithShaderGroup`) | Settings and variants including DefaultUnifiedPlastic, DefaultUnifiedSurfaceAppearance, tiled/detiled, depth/shadow and transparency paths. Shader identifiers are not shader bodies. |
| SurfaceAppearance | `rbx/MaterialGenerator.c:5274` (`createSurfaceAppearanceMaterialImpl`) | Surface-map/material-state setup. |
| Environment lighting/reflections | `rbx/EnvMapPBR.c:1202,2364,2689` | Irradiance-state updates and specular prefilter/convolution orchestration, with indoor/outdoor and ViewportFrame paths elsewhere in the file. |
| Ambient occlusion | `rbx/SSAO.c:615,1155,1321,1508,1678` | SSAO/HBAO compute/apply, blur/upsampling and shader selection. |
| Sky | `rbx/AdvSky.c:922,1136,1367`; `rbx/IndoorSkybox.c`; `rbx/SkyUtils.c` | Skybox loading and rendering setup. |
| Final image processing | `rbx/ScreenSpaceEffect.c:435–1280`; `rbx/SceneManager.c:16068–16266` | Image-processing selection and state for exposure, color correction, bloom and related effects. This is not a recovered standalone GPU tonemapper body. |
| Render-vertex encoding | `rbx/Graphics.part001.c:6428`, `encodeMVertex`, `0x10504df3c` | Position, normal, UV, tangent and color inputs packed into a render vertex. |
| Geometry production | `rbx/GeometryGenerator.c`; `rbx/FastClusterMeshGenerator.c`; `rbx/EditableMeshData.c` | Geometry/attribute paths that must connect to materials instead of Yune dropping UVs, normals and colors. |

The generic namespace bundles matter: `Graphics.part001.c` contains 400 recovered function entries and `Graphics.part002.c` contains 112. They hide helpers such as vertex encoding, color shifts, sampler handling, shader compilation and pack reading under broad filenames.

Generic initializers also matter. `_free/_free_g.part009.c:11872` contains the LightGridCPU.cpp initializer; `:12148` contains the MaterialGeneratorShaders.cpp initializer. The latter initializes rendering flags, not a complete inline shader program despite its suggestive name.

## What `_external` actually contains

The directory is present locally in the archive. It does not mean a remote renderer or unavailable online code.

A useful material/texture-processing group is `_external/pbrsynth/` (20 files), plus `_external/_p/PBRSynth.c` (171 recovered function entries). `PBRSynth.c:5` is `convertPixelsLinearToSrgb`. `pbrsynth/TexturePackProcessed.c:5,61,118` contains `setColor`, `setNormal`, and `setSpec`; KTX/KTX2 reader/writer paths are also present. This is relevant texture-processing implementation, not evidence that the entire scene-lighting shader is implemented in PBRSynth.

Qt embedded-resource registrations appear in `_external/_global__n_1/initializer.c` and `_free/_free_q.c`, referencing `qt_resource_data`, `qt_resource_name`, and `qt_resource_struct`. Inspected registrations include themes, icons, translations, debugger UI and editor resources. The searches found references, not complete recovered array definitions. A resource pointer alone is not its payload.

`rbx/RenderExternal.c` is a separate name: its two functions export a texture to an RGBA image and convert an image to a PNG buffer. It is not an external lighting engine.

## Shader-resource trace

### Main packs

1. `rbx/VisualEngine.c:4898–4950`, `reloadShaders`, gets the asset directory and appends `../shaders`.
2. `rbx/ShaderManager.c:429–497` builds `shaders_<backend>.pack`, joins the directory, and calls `AssetReader::getFilePointerFromPath`.
3. `rbx/AssetReaderImpl.c:39–43` calls `FileSystem::openFileReadUtf8`.
4. `rbx/ShaderManager.c:502–537` reads a 20-byte header, checks integer magic 1398293074 (little-endian ASCII `RBXS`), and requires version **11**.
5. Following code reads shader/variant/capability/file metadata and payloads. `rbx/Graphics.part001.c:23066`, `readData`, calls `fread`; `DeferredShaderHandle.c` and backend shader code handle loading/reloading.

### Additional Metal source resource

`rbx/DeviceMtl.c:881–910` requests bundle resource `builtins` with extension `shaders`, then reads its text. At line 959 it invokes `newLibraryWithSource:options:error:` on that text. This identifies the separate **`builtins.shaders`** resource path. The inspected listing does not include its source body inline at the call.

### Metal compiled-payload path

`rbx/Graphics.part001.c:1776–1855`, `compileShaderImpl`, checks input bytes. The comparison at line 1804 uses 1112298573 (little-endian `MTLB`) and the branch creates a dispatch-data object from the supplied buffer. That is a format check, not an embedded metallib array. Several recovered calls/types in this function are visibly unreliable and need resolution before treating the listing as buildable implementation.

## Search boundary and next reconstruction work

Whole-content searches covered shader-language signatures, source-extension strings, literal entry points, pack/metallib/numeric signatures, C-style arrays, compiler calls and Qt resource registrations, including `_external` and `_free` shards.

**No recognizable complete world-rendering GPU shader program was recovered as inline source or an embedded byte array by these searches.** This is a bounded search result, not proof that no payload could be reconstructed from another encoding or from incompletely recovered data. In particular, static-address references and unresolved constants remain.

The corrected conclusion is not "the archive has no lighting." It contains substantial light-grid calculations, culling, renderer-state preparation, material selection, geometry packing and pass orchestration suitable for source-traced reconstruction. Those available components should not be replaced with another invented simple directional-light model. Conversely, the discovery does not mean Yune already renders them or has complete shader resources.
