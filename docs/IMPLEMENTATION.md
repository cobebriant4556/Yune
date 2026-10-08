# Native implementation progress

## Working tree and validation

Development changes land directly on `main` in separate commits. Read CI for the exact commit; a successful older run does not validate a newer change. Each implementation step should include a source reference, its runtime integration, regression tests and any remaining compatibility limits.

Build and run the complete suite:

```text
cargo build -p yune
python scripts/run_smoke.py --profile debug
```

Release builds use `--profile release`; individual executables can be selected with `--binary PATH`. The harness passes the same selected binary to its child audio tests.

## Current subsystem boundary

| Subsystem | Available | Work in progress |
| --- | --- | --- |
| Audio | Offline AudioPlayer playback, sample-clock scheduling, Wire graph, stereo spatialization, thirteen effects and WAV capture | Replacing independent kernels with recovered implementation, beginning with playback regions and dynamics/distortion |
| Lighting | Native software shading, ambient/directional light and fog | Recovered renderer-state preparation and CPU skylight propagation |
| Editable geometry | Position/topology editing and rendering | Face-corner UVs, normals, colors, texture binding and mutation propagation |
| Assets | Local mesh/image/audio registration and lookup | Real project asset exports and end-to-end SM64 fixtures |

This table distinguishes available behavior from pending changes. Detailed usage remains in [AUDIO.md](AUDIO.md), [ENGINE_COVERAGE.md](ENGINE_COVERAGE.md) and [LOCAL_ASSETS.md](LOCAL_ASSETS.md).

## Reconstruction entry points

Lighting: `RenderView::presetLighting`, `SceneManager::setFog`, `LightGridCPU::lightingUpdateSkylightRow`, `FroxelGrid::cullLightsCPU`, `MaterialGenerator`, and geometry vertex encoding.

Audio: `AudioPlayer`, `FMODPlaybackChannel`, and the effect implementations under `FMOD::DSPDistortion`, `FMOD::DSPLimiter` and `FMOD::DSPCompressor`. Source-derived kernels and independent replacements must remain separately identified. CPU scalar behavior alone does not prove matching SIMD output, codec behavior or complete engine parity.

The lengthy archive-investigation pages were removed. File identities remain in `studio-0735-reference.json`; implementation-specific symbols and limitations belong beside the code and in coverage docs. No claim of full Studio rendering or audio parity is made by this documentation change.
