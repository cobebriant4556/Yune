# Headless audio

Yune has a native offline audio mixer. It does not launch Studio, initialize a sound card, record the desktop, or contact Roblox servers. Its purpose is to make playback timing, routing, and waveform differences inspectable by automated tools.

This is an independent compatibility implementation, not Roblox's audio engine. The public AudioPlayer playback API is the target; proprietary DSP, spatial acoustics, server replication, and all audio effects are not implemented.

## Capture a local asset

```text
cargo build -p yune
cargo run -p yune -- run examples/audio_capture.luau input.wav actual.wav
python scripts/compare_audio.py reference.wav actual.wav --tolerance 0.00004 --json comparison.json
```

The capture script accepts optional duration and playback speed arguments after the two paths. Its WAV output is 48 kHz stereo float32. The comparator accepts integer PCM and float WAV, reports length and amplitude errors, and exits nonzero for mismatches. Sample rates and channel counts must match. It never automatically normalizes, aligns, or resamples away errors. An explicitly requested `--offset-frames` is recorded in its JSON report.

## AudioPlayer coverage

| Surface | Implementation |
| --- | --- |
| Asset / AssetId / AudioContent | Local URI aliases; URI and empty Content supported; object Content rejected |
| AutoLoad / AutoPlay | Local decode on demand or automatically; AutoPlay is evaluated on first graph entry |
| IsReady / IsPlaying / TimeLength | Playback state and decoded duration; pending Play counts as playback intention |
| Play / Stop / Cancel | Immediate and mixer-time scheduled commands; scheduled commands use individual sample boundaries |
| TimePosition | Seeking; Stop preserves the cursor and Play resumes it |
| Volume / PlaybackSpeed | Gain and variable-rate sample playback, including speed zero |
| PlaybackRegion / LoopRegion / Looping | Region restriction, intro followed by custom loop, and looping notifications |
| Ended / Looped | Queued Lua signals; Ended is not emitted for Stop |
| GetWaveformAsync | Signed mono waveform samples without modifying the playback cursor |
| Pin and wire methods / WiringChanged | Validated live Wire graph and connection events |
| Signals | Connect, Once, Disconnect, Connected and yieldable Wait |

Local loading is synchronous rather than Roblox's network-buffering behavior. Numeric inputs must be finite. Unsupported or missing local assets produce diagnostics; Play rejects an unready asset rather than pretending to play it.

## Routing and analysis

Connect `AudioPlayer -> Wire -> AudioDeviceOutput` for an audible master mix. `AudioFader` supports Volume and Bypass. Sources are evaluated once per block and can fan out to several targets; multiple incoming wires sum their streams. `AudioDeviceOutput.Player` selects the local player in this single-client environment. Invalid pins, incomplete wires and cycles stay disconnected.

`AudioAnalyzer` reports block peak and RMS and offers a Hann-window FFT spectrum. Its window sizes and normalization are Yune's approximation, not a verified match for Roblox's analyzer. Equalizer, filter, echo, reverb, compressor, pitch shifter, device input, recorder, speech, emitter/listener spatialization, and replication are not present. Connecting an unsupported audio node raises an error rather than silently bypassing the node. Call `audio.getCapabilities()` and `audio.getDiagnostics()` to inspect the current boundary.

## Automation interface

```luau
local runtime = require("@yune/runtime")
local audio = require("@yune/audio")

audio.registerAsset("rbxassetid://123", "assets/jump.wav")

audio.beginCapture({maxSeconds = 10})
runtime.runFrames(120, 1 / 60)
local report = audio.endCapture("mix.wav", true)
local events = audio.takeEvents()
```

This example captures the graph already present in the DataModel. Register the original audio under its existing asset URI; the game's AudioPlayer properties do not need replacement. `audio.setAssetRoot(path)` also resolves contained relative paths, `rbxasset://` paths, and numeric asset IDs with common audio extensions. It rejects traversal and symlink escapes.

`audio.beginCapture({source = player})` taps a single player or supported audio node instead of the master bus. `audio.getCaptureBuffer()` returns interleaved little-endian float32 stereo. `audio.registerPCM(id, sampleRate, channels, buffer)` supplies mono or stereo source PCM directly. `endCapture(path)` exports PCM16; its optional second argument enables float32. Reports include frame count, duration, peak, RMS, clipping count, and start/end mixer times. `takeEvents()` drains a sample-indexed event trace.

`runtime.step(dt)` advances physics, animation, and audio together. `audio.step(dt)` advances only audio and should not be used in addition to runtime.step for the same time interval. Scheduled audio uses `SoundService:GetMixerTime()`, whose epoch is the offline audio clock. Lua callbacks run when the scheduler regains control; sample timestamps remain available in the event trace. A long synchronous runFrames call does not turn arbitrary task.wait coroutines into sample-level callbacks.

## Fidelity and resource limits

The mixer uses a 48 kHz stereo output clock, mono duplication, linear source resampling, and floating-point summation without loudness normalization. These choices are useful for deterministic regression testing but are not proof of sample-for-sample Roblox DSP parity. Captured references are still required to validate the implementation against Roblox itself.

Symphonia provides local WAV, MP3, Vorbis/Ogg, FLAC and other enabled decoders. Codec enablement is distinct from conformance testing; the generated smoke fixture initially exercises PCM/WAV. Multichannel input is rejected rather than silently downmixed. Encoded files are limited to 256 MiB; each decoded clip is limited to 28.8 million frames; the cached-asset budget is 512 MiB. Captures default to 120 seconds, with an explicit maximum of 600 seconds. Individual steps are limited to 60 seconds and graphs to 512 supported nodes. Sub-sample loop regions emit at most one loop notification per output sample.

## Reference contracts

- https://create.roblox.com/docs/reference/engine/classes/AudioPlayer
- https://create.roblox.com/docs/reference/engine/classes/Wire
- https://create.roblox.com/docs/reference/engine/classes/AudioFader
- https://create.roblox.com/docs/reference/engine/classes/AudioAnalyzer
- https://create.roblox.com/docs/reference/engine/classes/AudioDeviceOutput
- https://create.roblox.com/docs/reference/engine/classes/SoundService

Tests: `cargo test -p yune-runtime`, `python scripts/run_smoke.py`, and `python scripts/test_audio_compare.py`. Generated WAVs and logs live in `test-results/audio/` and `test-results/`.
