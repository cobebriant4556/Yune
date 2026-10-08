# Lighting implementation

## Active renderer-state reconstruction

The software rasterizer now receives the recovered ambient/outdoor split from `RenderView::presetLighting` (`0x1051e828f..0x1051e8306`): `Ambient * Ambient` and the component-wise nonnegative difference `OutdoorAmbient * OutdoorAmbient - Ambient * Ambient`. The former `max(Ambient, OutdoorAmbient * 0.35)` heuristic is removed.

The property mapping is checked against the target build's `LightingProp` accessors, not old-layout field-name annotations: the property subobject starts at +192, global ambient is at +16, and global outdoor ambient is at +64. These resolve to the +208 and +256 offsets read by `presetLighting`.

`SceneManager::setFog` (`0x10520f1e8`) is reconstructed as four float32 shader constants, the fog color, fog end and the equal-endpoint-masked reciprocal span. The rasterizer uses those constants. Its finite, zero-width step fallback is a Yune adapter, not a recovered GPU instruction sequence.

`lighting_state_regression.luau` verifies rendered pixels, including live property changes. Native tests check per-channel ambient clamping, equal ambient, fog constants and zero-width boundaries.

## Remaining renderer work

The ambient contribution currently assumes unoccluded outdoor visibility. Occupancy/skylight propagation, local lights, shadows, time-dependent sun color/direction, color shifting, environment convolution and final GPU shading still need integration. The existing software material shader and display conversion are not Studio's shader implementation. This change ports the identified CPU state calculations, not the complete lighting engine; captures are not yet Studio-pixel references.

The source locations are `rbx/RenderView.c`, `rbx/SceneManager.c`, and `rbx/LightingProp.c` in the supplied 0.735 listings. `lighting_state.rs` records the reconstruction boundaries alongside the code.
