# Yune engine coverage

Yune is an independent runtime recreation built on the open-source Lune/Luau stack. The goal is behavioral compatibility for local execution, rendering, visual testing, and automation.

The public repository does not contain proprietary Roblox engine source. Reference builds are used only to identify subsystem boundaries and expected externally observable behavior.

## Runtime layers

| Reference subsystem | Yune subsystem | Status |
| --- | --- | --- |
| ScriptContext | `yune-runtime` + Lune VM | In progress |
| TaskScheduler | vendored Lune scheduler + Yune frame loop | In progress |
| DataModel | `yune-runtime` | Working |
| RunService | `yune-runtime` signals/frame stepping | Working baseline |
| Workspace | Lune Instance DOM + Yune runtime services | Working baseline |
| RenderingEngine / RenderView | `yune-render` | Working baseline |
| SceneUpdater / RenderEntity | DOM traversal + render bindings | Early |
| GfxGui / GfxGuiRenderer | `yune-render/src/gui.rs` | Working baseline |
| Lighting | CPU lighting/fog/exposure pipeline | Working baseline |
| ViewportFrame | nested offscreen world render | Working baseline |
| EditableMesh | stable-ID editable topology | Working baseline |
| EditableImage | mutable RGBA image + drawing | Working baseline |
| AssetService | EditableMesh/EditableImage creation | Working baseline |
| CoreGui / PlayerGui | runtime-owned GUI roots | Working baseline |
| place loading | Lune RBXL/RBXLX document decoder | In progress |
| physics / assemblies / contacts | not implemented | Missing |
| Humanoid controller/state machine | not implemented | Missing |
| animation evaluation | not implemented | Missing |
| MeshPart asset decoding | not implemented | Missing |
| materials / textures / decals | not implemented | Missing |
| SurfaceAppearance / PBR | not implemented | Missing |
| terrain rendering | not implemented | Missing |
| shadow maps | not implemented | Missing |
| sky / atmosphere / clouds | not implemented | Missing |
| post effects | not implemented | Missing |
| UI layout constraints | partial | Missing coverage |
| text shaping / Roblox fonts | bitmap fallback only | Missing |
| automatic Script/LocalScript lifecycle | not implemented | Missing |
| ModuleScript require by Instance | not implemented | Missing |
| signals beyond RunService | not implemented | Missing |
| replication/network ownership | not implemented | Missing |

## Near-term order

1. Persistent ScriptContext-like script lifecycle and ModuleScript requiring.
2. RBXL service merge validation and automatic script discovery.
3. MeshPart, SpecialMesh, Decal, Texture, SurfaceAppearance, and local asset cache.
4. GUI layout pipeline: UIScale, UIPadding, UIListLayout, UIGridLayout, constraints, clipping, ZIndexBehavior, ImageRect, ScaleType, and filtering.
5. Lighting: sun direction from ClockTime/geography, shadow maps, Sky, Atmosphere, ColorCorrection, Bloom, SunRays, DepthOfField.
6. Physics stepping and raycasts.
7. Animation tracks, Motor6D transforms, Animator, and Humanoid state evaluation.
8. Deterministic render outputs: color, depth, normals, object ID, material ID, and scene metadata.
