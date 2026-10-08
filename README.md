# Yune

Yune is a headless Luau runtime for Roblox-style scene execution, rendering, and offline audio testing. It embeds public Lune, uses its Roblox datatypes and Instance DOM, and adds independent runtime, rendering, and audio implementations.

Yune's native backend does not launch Roblox Studio or require a display server or sound card. It is an experimental compatibility implementation, not Roblox's proprietary engine and not a claim of pixel-perfect or sample-perfect parity. The optional Studio reference commands are separate from native execution.

## Runtime and rendering

The implementation includes a shared DataModel, script discovery and ModuleScript requiring, frame stepping, baseline physics and Motor6D/animation evaluation, offscreen RGBA/depth rendering, GUI composition, local mesh/image assets, and EditableMesh/EditableImage operations. Subsystem coverage remains partial; see [engine coverage](docs/ENGINE_COVERAGE.md) and regression scenes under `examples/`.

```luau
local runtime = require("@yune/runtime")
local render = require("@yune/render")
local assetService = game:GetService("AssetService")

local camera = workspace.CurrentCamera
camera.CFrame = CFrame.lookAt(Vector3.new(8, 6, 10), Vector3.zero)
camera.FieldOfView = 70

local part = Instance.new("Part")
part.Anchored = true
part.Color = Color3.fromRGB(245, 205, 48)
part.Parent = workspace

local mesh = assetService:CreateEditableMesh()
local a = mesh:AddVertex(Vector3.new(-1, 0, 0))
local b = mesh:AddVertex(Vector3.new(1, 0, 0))
local c = mesh:AddVertex(Vector3.new(0, 2, 0))
mesh:AddTriangle(a, b, c)
render.bindEditableMesh(part, mesh)

runtime.step(1 / 60)
render.capture({world = workspace, camera = camera, width = 1280, height = 720, path = "frame.png"})
```

## Headless AudioPlayer, spatial audio and effects

The native audio path supports local AudioPlayer assets, playback/stop/seek/speed/volume, playback and loop regions, scheduled sample-clock commands, cancellation, playback events, Wire routing, an approximate AudioAnalyzer, and offline PCM/WAV capture.

AudioEmitter/AudioListener implement direct-path stereo spatialization, parent/explicit positioning, attachment transforms, interaction groups, custom distance/angle curves and distance presets. Effects can be placed before emitters, after listeners or in non-spatial chains. The thirteen supported processors are Fader, Chorus, Flanger, Distortion, Echo, Equalizer, Filter, Compressor, Limiter, Gate, PitchShifter, Reverb and Tremolo. Their DSP state persists between steps, and compressor sidechains remain separate from the audible mix.

```text
cargo run -p yune -- run examples/audio_capture.luau input.wav actual.wav
cargo run -p yune -- run examples/audio_spatial_capture.luau input.wav spatial.wav 3 AudioReverb
python scripts/compare_audio.py reference.wav actual.wav --tolerance 0.00004 --json comparison.json
```

Output is 48 kHz stereo. Captures can tap individual players, effects, emitter inputs, listener outputs or the master mix. The spatial example also writes a JSON trajectory, exact effect settings, events and latency metadata beside its WAV. The comparator reports sample differences without silently normalizing or aligning away mistakes. Assets can be registered under the original `rbxassetid://` URIs used by a game.

These are independent DSP approximations, not sample-identical Roblox implementations. Automatic acoustic occlusion/diffraction/room simulation, HRTF, Doppler, device input and network replication are not implemented. Unsupported connected nodes produce explicit errors; requested acoustic simulation produces a direct-path-only diagnostic. `audio.getCapabilities()` reports the boundaries and `robloxDspParity = false`.

[Audio usage, API coverage, algorithm details, limitations and file formats](docs/AUDIO.md)

## Build and tests

A recent Rust toolchain with Edition 2024 support is required. Lune is vendored under `upstream/lune`; Yune includes explicit modifications to that local dependency.

```text
cargo build -p yune --release
cargo run -p yune --release -- run examples/render_smoke.luau
cargo test -p yune -p yune-runtime -p yune-render -p yune-reference -p yune-physics
cargo build -p yune
python scripts/run_smoke.py
python scripts/test_audio_compare.py
```

GitHub Actions tests Windows, macOS, and Linux and uploads logs, generated images, WAV captures, and comparison reports. Native DSP tests check filter responses, delay timing, pitch frequency/latency, dynamics, and block-partition invariance. End-to-end Luau scenes test spatial transforms, curves, groups, graph feedback, all effect nodes and sidechain isolation. The spatial CLI test renders procedural input twice and compares the WAV bytes and metadata.

The test fixtures are not original SM64 recordings. Validate real project assets and reference captures before making fidelity claims.

Lune and Yune use MPL-2.0. Third-party dependency notices are in [NOTICE.md](NOTICE.md).
