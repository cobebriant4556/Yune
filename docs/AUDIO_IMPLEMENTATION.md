# Audio implementation changes

## Playback corrections

`AudioWorld::command` retains Play intention when an asset is not ready. Rendering stays silent and the cursor remains stationary until local PCM becomes available; missing assets still produce diagnostics. Immediate Play/Stop return action ID `0`. Scheduled actions keep individual IDs.

`Player::bounds` uses float32 effective-region calculations from `FMODPlaybackChannel::getEffectivePlaybackRegion` (`0x1034dbee2`) and `getEffectiveLoopRegion` (`0x1034dce64`): clamp to the valid interval, then fall back to the whole interval when endpoints are equal or within the relative tolerance. Natural completion resets TimePosition to zero. Explicit Stop preserves the cursor. The offline scheduler, decoder and interpolator remain Yune implementations.

## Active source-derived processors

`Effect::process` now calls the distortion, limiter and stereo scalar compressor kernels in `source_kernels.rs`. These are active in Wire chains, spatial chains and offline captures, not unused reference routines. State persists across calls and `audio.resetEffects` resets it.

| Processor | Recovered implementation | Changed behavior |
| --- | --- | --- |
| AudioDistortion | `_external/fmod/DSPDistortion.c`, `readInternal`, `0x1035829e6` | Float32 rational transfer replaces tanh; uses the recovered limiting coefficient at Level=1 |
| AudioLimiter | `_external/fmod/DSPLimiter.c`, `readInternal` / `setParameterFloatInternal`, `0x10358cb8e` / `0x10358cc7c` | Independent channel envelopes initialized to 1 and RC release replace linked stereo gain smoothing |
| AudioCompressor | `_external/fmod/DSPCompressor.c`, `readInternal`, `0x10357659c`; `_free/_free_f.part004.c`, stereo scalar tail `0x1035fc9e0..0x1035fca7e` | Summed stereo power, two-stage envelope, recovered time coefficients and scalar powf gain replace a peak-based detector |

Compressor sidechain streams drive the detector without entering the output. An absent sidechain uses the input's summed stereo power. A mono clip is duplicated by Yune before graph processing; this channel adaptation has not been matched to the original engine. All other processors retain their previous independent implementations.

## Validation and limits

Native tests check equations, histories, region fallback, source readiness, natural completion and partition invariance. `audio_source_regression.luau` checks recovered playback and distortion/limiter behavior through the live graph. `audio_effects_regression.luau` checks all processors, including updated stereo-power compression and sidechain isolation.

`audio.getCapabilities().sourceDerivedEffects` lists the three integrated processors. Scalar reconstruction is not a claim of identical SIMD approximations, platform math-library output, codec behavior, channel adaptation or complete Studio audio parity. `robloxDspParity` remains false. The synthetic fixtures are not SM64 reference recordings.
