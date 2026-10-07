# Yune engine coverage

Yune is an independent Lune/Luau-based runtime and software renderer. It does not embed Roblox Studio's engine. API names and a passing small test suite do not establish full Roblox compatibility.

## Implemented paths and their limits

| Subsystem | Implemented path | Important limits |
| --- | --- | --- |
| Luau / DataModel | Vendored Lune VM, Roblox datatypes and Instance DOM, local services | Services existing in the tree do not imply their methods or platform behavior are implemented. |
| Script lifecycle | Frame-boundary discovery, individual script environments, top-level coroutine cancellation/restart on disable/re-enable/removal | No complete server/client RunContext eligibility, owned child-task cancellation, or automatic event-connection cleanup. |
| ModuleScript require | Tracked yielding module threads, concurrent callers share one execution and cached result; exact one-value validation; cached errors | Yune deliberately reports a dependency cycle rather than reproducing an indefinite cyclic-require hang. String require delegates to Lune. |
| Frame scheduling | Explicit step/runFrames, RunService signals, physics/animation stages | runFrames is still synchronous; task.wait is tied to Lune's real-time scheduler, not a virtual simulation clock. Signal callbacks cannot all yield safely yet. |
| Place loading | RBXL/RBXLX parser and service merge | Cross-service reference remapping and replacement/reset lifecycle require more testing. Do not assume arbitrary saved games boot unchanged. |
| MeshPart / SpecialMesh | Local Roblox mesh v1-v5 decoding; positions, UVs, normals; content-ID cache; automatic local lookup | No v6/v7 mesh decoding, bone skinning, LOD selection, collision-mesh fidelity or live Roblox downloads. |
| Asset IO | setAssetRoot, registerAsset, normalized numeric and legacy URL IDs, rbxasset paths, signature-based PNG decoding | Local files only. Current image decoder build enables PNG, not all formats suggested by filename extensions. Missing/invalid referenced assets fail strict capture. |
| Materials / textures | Simple material response; MeshPart color, roughness and metalness maps; SpecialMesh texture; Decal and tiled Texture surfaces | Approximate shading; no real Roblox material shaders, normal-map tangents, full alpha/material modes, terrain, atmosphere or shadows. |
| GUI | Basic frames/text/images, inherited UIScale, axis-aligned ClipsDescendants, ScreenGui Enabled/DisplayOrder, sibling/global ZIndex, image crop/filter/fit/tile/nine-slice | No full layout constraints, UIListLayout/UIGridLayout, automatic sizing, rich text, rotations, device-safe-area/inset emulation or input dispatch. AbsolutePosition/Size update at capture time. |
| World GUI | BillboardGui and SurfaceGui offscreen composition | Depth bias, back-face visibility, billboard offset semantics and ordering still need conformance work. |
| ViewportFrame | Separate camera and offscreen part/mesh rendering | Not a complete WorldModel runtime; effects, transparency and nested world-GUI parity remain incomplete. |
| Fonts | Local TTF/OTF registration through fontdue, FontFace family selection, rasterized vector text; readable bitmap fallback | No font files are distributed. Weight/style selection, full shaping, per-line alignment, hinting and exact Roblox metrics are incomplete. |
| EditableMesh / EditableImage | Mutable geometry topology and RGBA image drawing/binding | API subset only; no claim of Roblox permission/memory-budget or complete Content.fromObject semantics. |
| Physics | Rapier cuboid bodies, gravity, collision baseline, velocity writeback, impulses, basic raycasts, stale-body cleanup | NOT Roblox's solver. Motor/weld children are excluded from independent bodies rather than forming complete compound collision/mass assemblies. RaycastParams, touch events, collision groups and most constraints remain missing. |
| Joints | Rooted graph propagation for Motor6D/Weld/WeldConstraint, reverse traversal from anchored roots, enabled checks and long chains | Kinematic pose propagation, not a physical joint/constraint solver. Cyclic and multi-anchor assemblies use a spanning traversal, not full constraint resolution. Motor DesiredAngle behavior remains missing. |
| Animation | Rig-scoped local KeyframeSequence clips, hierarchical pose-name routing, interpolation, persistent playing tracks, weight fades, stop cleanup, GetPlayingAnimationTracks | No AnimationId asset loading, priorities, markers/events, complete easing, bone deformation or Humanoid state machine. |
| Capture | Offscreen PNG plus optional RGBA buffer and structured missing-asset diagnostics | Depth exists internally, but public depth/normal/object-ID passes are not implemented. No GPU renderer or full cross-platform pixel-equivalence claim. |

## Regression suite

Run `cargo build -p yune` followed by `python scripts/run_smoke.py` from the repository root. The harness runs every scenario independently with a 60-second timeout, writes logs and a summary to `test-results`, and returns nonzero if any scenario fails. CI runs the same harness on Linux, Windows and macOS; read the result for the exact commit rather than treating a queued build as a pass.

- `require_regression`: eight concurrent yielding requires, result identity, return validation, cycle diagnostic, script disable/re-enable.
- `runtime_smoke`: ScriptContext bootstrap, gravity, impulse, basic raycast, Motor6D and local Animator evaluation.
- `cframe_regression`: lookAt/lookAlong/legacy look constructor, camera-facing direction, orthogonal basis.
- `render_smoke` / `visual_smoke`: PNG output and baseline EditableMesh/MeshPart/world GUI scenes. These are smoke scenes, not complete visual fidelity tests.
- `asset_regression`: automatic mesh/image lookup, numeric ID aliases, extensionless PNG decode, RGBA pixel assertion and strict missing-asset error.
- `rig_regression`: isolated rigs with identical part names, fades, stopped pose cleanup, long reverse-ordered Motor6D chains and disabled joints.
- `gui_regression`: pixel checks for inherited UIScale, clipping, disabled GUI, bitmap orientation, atlas filtering, TileSize and nine-slice borders.

The native tests use procedural scenes and a tiny mesh fixture. They do not prove that a full Mario rig, SM64 level or arbitrary Roblox place works. Compare those real project fixtures with a pinned official Studio build before making fidelity claims.

## Public behavioral references

- https://create.roblox.com/docs/reference/engine/classes/ModuleScript
- https://create.roblox.com/docs/reference/engine/classes/BaseScript
- https://create.roblox.com/docs/reference/engine/classes/Motor6D
- https://create.roblox.com/docs/reference/engine/classes/Animator
- https://create.roblox.com/docs/reference/engine/classes/AnimationTrack
- https://create.roblox.com/docs/reference/engine/datatypes/CFrame
- https://create.roblox.com/docs/reference/engine/classes/UIScale
- https://create.roblox.com/docs/reference/engine/classes/GuiObject
- https://create.roblox.com/docs/reference/engine/classes/ImageLabel
