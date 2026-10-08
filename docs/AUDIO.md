# Headless audio

Yune has a native offline audio mixer. It does not launch Studio, initialize a sound card, record the desktop, or contact Roblox servers. Playback timing, graph routing, effects and direct-path spatial audio can be captured to PCM/WAV for automated comparison.

This is an independent compatibility implementation, not Roblox's audio engine. The supported nodes produce real audio, but their DSP algorithms and spatial rendering are not calibrated to Roblox. `audio.getCapabilities().robloxDspParity` remains false. Passing procedural tests does not establish SM64 or Roblox waveform parity.

## Capture a local asset

```text
cargo build -p yune
cargo run -p yune -- run examples/audio_capture.luau input.wav actual.wav
python scripts/compare_audio.py reference.wav actual.wav --tolerance 0.00004 --json comparison.json
```

The basic capture accepts optional duration and playback speed arguments after the two paths. Its output is 48 kHz stereo float32. The comparator accepts integer PCM and float WAV, reports length and amplitude errors, and exits nonzero for mismatches. Sample rates and channel counts must match. It never automatically normalizes, aligns, or resamples away errors. An explicitly requested `--offset-frames` is recorded in its JSON report.

For a moving emitter, with an optional effect:

```text
cargo run -p yune -- run examples/audio_spatial_capture.luau jump.wav spatial.wav 3 AudioReverb
cargo run -p yune -- run examples/audio_spatial_capture.luau jump.wav dry.wav 3 none
```

This plays the source once, moves it on a fixed 60 Hz trajectory, renders any remaining effect tail, and saves `spatial.wav.json` beside the WAV. The JSON contains effect settings, pitch-shifter latency, each step's source position and left/right attenuation gains, sample-clock events, output metrics and diagnostics. No SM64 assets are included. Use the original project files as inputs; the sample scene is a test trajectory, not a recreation of SM64's audio positioning or reverb.

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

Loading is synchronous rather than network-buffered. Numeric inputs must be finite. Missing assets produce diagnostics, and Play rejects an unready asset instead of pretending it is playing.

## Routing and state

A direct graph is `AudioPlayer -> AudioDeviceOutput`, joined by Wire instances. A spatial graph is `AudioPlayer -> AudioEmitter`, then an implicit world-space connection to `AudioListener -> AudioDeviceOutput`. Effects can be wired before emitters, after listeners, or in ordinary non-spatial chains. There is no Wire between an emitter and listener.

Sources are evaluated once per block and can fan out. Incoming wires sum their streams. The compressor's Sidechain stream is kept separate from Input and never added to the audible mix. Output.Player selects the local player in this single-client environment. Invalid pins, incomplete wires and cycles stay disconnected; cycle checks include the implicit spatial connections. Connected unsupported audio classes raise errors.

Delay histories, oscillators, filter histories, envelopes and pitch-shifter overlap buffers persist across mixer blocks. Stopping a player does not discard an effect's tail. `audio.resetEffects(effect)` clears one effect's state; `audio.resetEffects()` clears all effects. Bypass is an exact pass-through and currently freezes that effect's DSP state. Removing an effect from the DataModel clears its state on the next audio step. These lifecycle choices are Yune behavior, not verified Roblox bypass/reparenting behavior.

Graph ordering and summation follow DataModel traversal order rather than random referent IDs. Tests check same-platform byte-identical repeat renders and partition invariance; identical floating-point bits across different CPUs or operating systems are not guaranteed.

## Effect nodes

All thirteen listed processors accept Input and expose Output. AudioCompressor also accepts Sidechain. They have Bypass, wiring methods and property-change signals. Values are checked for finiteness and constrained to the implemented parameter ranges.

| Node | Processing and controls |
| --- | --- |
| AudioFader | Volume gain |
| AudioChorus | Three modulated delayed copies; Depth, Mix and Rate; maximum delay scales to 100 ms, and zero Depth is undelayed |
| AudioFlanger | Modulated delay with feedback; Depth, Mix and Rate; maximum delay scales to 10 ms |
| AudioDistortion | Soft saturation controlled by Level; zero Level is unchanged |
| AudioEcho | Fractional delay, Feedback, DryLevel/WetLevel in dB, DelayTime and sample-clock RampTime interpolation |
| AudioEqualizer | Three-band shaping with LowGain, MidGain, HighGain and MidRange crossover frequencies |
| AudioFilter | Peak, shelves, low/high-pass cascades, bandpass and notch; Frequency, Q, Gain and GetGainAt |
| AudioCompressor | Stereo-linked gain reduction, Input/Sidechain selection, Attack, Release, Threshold, Ratio and MakeupGain |
| AudioLimiter | Stereo-linked peak ceiling with MaxLevel and Release; not an oversampled true-peak limiter |
| AudioGate | Initially closed gate, NumberRange Threshold hysteresis, Attack and Release |
| AudioPitchShifter | Streaming FFT phase vocoder; Pitch and WindowSize; changes pitch without advancing the player's cursor faster |
| AudioReverb | Filtered early reflection, diffuse feedback tail and high-frequency damping; all documented reverb controls are used |
| AudioTremolo | Gain oscillator controlled by Depth, Frequency, Duty, Shape, Skew and Square |

Reverb's twelve numeric controls are DecayTime, DecayRatio, Density, Diffusion, DryLevel, WetLevel, EarlyDelayTime, LateDelayTime, HighCutFrequency, LowShelfFrequency, LowShelfGain and ReferenceFrequency; it also has Bypass. The implementation uses comb/all-pass delay networks, not Roblox's room renderer. Chorus/flanger oscillator profiles, distortion transfer function, equalizer crossover response, dynamics detector/time constants and tremolo waveform mappings are independent approximations. Reverb and pitch changes can have audible artifacts. Defaults in `effects::PARAMETERS` are Yune's explicit defaults and still require captured-engine validation.

The pitch shifter uses 512/1024/2048-sample windows and adds one window of latency, including at Pitch = 1. Window-size changes reset its analysis state. `audio.getNodeLatency(effect)` returns `{samples, seconds, includesCreativeDelays = false}`. Echo and reverb's intentional delays are not counted as processing latency. Do not silently remove latency when comparing; report any requested alignment.

## Spatial audio

Emitters/listeners use a parent BasePart, Camera, Attachment/Bone or Model transform. PositionType = Instance uses PositionInstance instead. Invalid positioning objects are silent. Attachment transforms compose through their parent chain; model positioning uses the primary part's pivot when available, otherwise WorldPivot. These transform paths still need complete saved-place conformance testing.

`SetDistanceAttenuation` and `SetAngleAttenuation` accept numeric-key tables of up to 400 points. Angles range from 0 to 180 degrees, distances are nonnegative, and volumes range from 0 to 1. Curves interpolate linearly between points and hold their endpoint values outside the range. Nil/empty tables restore the defaults; getters return independent tables. Emitters and listeners each contribute distance and angle gain, multiplied together.

Emitters also support the Custom, InverseTapered, Linear, LinearSquared and Inverse DistanceAttenuationMode presets with DistanceAttenuationBounds. Yune's uncalibrated custom-default emitter gain is `(4 / max(distance, 4))^2`; the listener's default distance gain and both default angle gains are 1. Use explicit custom curves for controlled comparisons rather than assuming this default matches Roblox.

Only matching AudioInteractionGroup strings interact. GetInteractingListeners, GetInteractingEmitters and GetAudibilityFor are implemented. `audio.getSpatialInfo(emitter, listener)` reports audibility, pan, leftGain/rightGain and `calibratedAgainstRoblox = false`.

Each emitter downmixes its incoming stereo stream to mono and uses equal-power left/right panning relative to the listener's orientation. Centered sound is approximately -3 dB per channel relative to the mono input. This is not HRTF/binaural audio and does not distinguish front/back through ear filtering. Keep stereo music wired directly to the output when it should remain non-spatial.

Transforms, curves and ordinary effect properties are sampled at the start of each audio/runtime step. They are not sample-accurate automation lanes, and a long step does not interpolate motion. Advance at the same frame intervals as the reference.

AcousticSimulationEnabled does not enable a room simulation yet. When both ends request it, Yune emits a direct-path-only diagnostic. Automatic wall occlusion, diffraction, geometric room reverberation, HRTF and Doppler are not implemented. Serialized binary attenuation-curve properties, default listener creation, replication, multichannel routing, device input, recording and speech nodes also remain unsupported. The Lua curve methods work, but loading/cloning an opaque engine-serialized curve does not recreate it. Reapply explicit curves in the test fixture.

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

This captures the graph already in the DataModel. Register an original asset under its existing URI; AudioPlayer properties do not need replacement. `audio.setAssetRoot(path)` resolves contained relative paths, rbxasset paths and numeric IDs with common extensions, rejecting traversal and symlink escapes.

`beginCapture({source = node})` taps a supported player, effect, emitter input, listener output or other supported node instead of the master bus. `getCaptureBuffer()` returns interleaved little-endian float32 stereo. `registerPCM(id, sampleRate, channels, buffer)` supplies source PCM. `endCapture(path)` exports PCM16; its optional second argument enables float32. Reports include frame count, duration, peak, RMS, clipping count and mixer times. `takeEvents()` drains a sample-indexed event trace. AudioAnalyzer provides block peak/RMS and an approximate Hann-window spectrum.

`runtime.step(dt)` advances physics, animation and audio together. `audio.step(dt)` advances only audio; do not use both for the same interval. Scheduled AudioPlayer commands use SoundService:GetMixerTime, whose epoch is the offline clock. Lua callbacks run when the scheduler regains control; their sample timestamps remain in the trace. Synchronous runFrames does not make arbitrary task.wait coroutines sample-accurate.

## Fidelity and resources

The mixer uses 48 kHz stereo output, linear source resampling, floating-point summation and no loudness normalization. Symphonia provides enabled local WAV, MP3, Vorbis/Ogg, FLAC and other decoders; enablement is not comprehensive codec conformance testing. Multichannel source files are rejected instead of silently downmixed.

Limits: encoded file 256 MiB; decoded clip 28.8 million frames; cached assets 512 MiB; conservative active effect-state estimate 256 MiB; capture default 120 seconds / explicit maximum 600 seconds; step maximum 60 seconds; 512 supported graph nodes and 4096 connected wires. Sub-sample loop regions generate at most one loop notification per output sample. Non-finite graph output raises a diagnostic error rather than exporting invalid samples.

For SM64 comparisons, preserve the original samples, gain, playback rate, looping points, spatial curves, effect parameters, source/listener transforms and timing. First compare the direct player tap, then each effect output, then the listener and master mix. A matching dry signal does not validate reverb, panning or pitch processing. Captured Roblox and/or vanilla SM64 references are still required to establish actual fidelity.

## Tests and public contracts

Run `cargo test -p yune-runtime`, `python scripts/run_smoke.py` and `python scripts/test_audio_compare.py` after building Yune. Native tests cover filter responses/extremes, exact delays and ramps, pitch reconstruction/frequency, limiter ceilings, sidechain isolation, bypass, effect tails, curve interpolation and partition invariance. Luau scenes exercise actual Instance/Wire routing, transforms, curves, groups, enum validation, lifecycle and capture. The spatial CLI test checks repeated WAVs byte-for-byte and validates JSON trajectory/latency metadata. CI retains captures and logs in its test artifacts.

Primary behavioral references:

- https://create.roblox.com/docs/reference/engine/classes/AudioPlayer
- https://create.roblox.com/docs/reference/engine/classes/Wire
- https://create.roblox.com/docs/reference/engine/classes/AudioEmitter
- https://create.roblox.com/docs/reference/engine/classes/AudioListener
- https://create.roblox.com/docs/reference/engine/enums/DistanceAttenuationMode
- https://create.roblox.com/docs/reference/engine/classes/AudioChorus
- https://create.roblox.com/docs/reference/engine/classes/AudioFlanger
- https://create.roblox.com/docs/reference/engine/classes/AudioEcho
- https://create.roblox.com/docs/reference/engine/classes/AudioEqualizer
- https://create.roblox.com/docs/reference/engine/classes/AudioFilter
- https://create.roblox.com/docs/reference/engine/classes/AudioCompressor
- https://create.roblox.com/docs/reference/engine/classes/AudioLimiter
- https://create.roblox.com/docs/reference/engine/classes/AudioGate
- https://create.roblox.com/docs/reference/engine/classes/AudioPitchShifter
- https://create.roblox.com/docs/reference/engine/classes/AudioReverb
- https://create.roblox.com/docs/reference/engine/classes/AudioTremolo
