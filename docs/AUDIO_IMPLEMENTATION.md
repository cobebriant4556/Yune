# Audio implementation changes

## Playback corrections

`AudioWorld::command` now retains Play intention when an asset is not ready. Rendering stays silent and the cursor remains stationary until local PCM becomes available; missing assets still produce diagnostics. Immediate Play/Stop return action ID `0`. Scheduled actions keep individual IDs.

`Player::bounds` uses float32 effective-region calculations from `FMODPlaybackChannel::getEffectivePlaybackRegion` (`0x1034dbee2`) and `getEffectiveLoopRegion` (`0x1034dce64`): clamp to the valid interval, then fall back to the whole interval when endpoints are equal or within the relative tolerance. Natural completion resets TimePosition to zero. Explicit Stop preserves the cursor. The offline scheduler, decoder and interpolator remain Yune implementations.

Tests cover scheduled sample boundaries, region fallback, delayed asset readiness, natural completion versus explicit Stop, and block partitioning. Native scalar DSP kernels are included with unit tests in `source_kernels.rs`; graph integration of those kernels is a separate step.

## Source reference map

| Native component | Recovered implementation | Scope |
| --- | --- | --- |
| Distortion kernel | `_external/fmod/DSPDistortion.c`, `readInternal`, `0x1035829e6` | Float32 rational transfer and coefficient at Level=1 |
| Limiter kernel | `_external/fmod/DSPLimiter.c`, `readInternal` / `setParameterFloatInternal`, `0x10358cb8e` / `0x10358cc7c` | Independent channel envelopes and RC release |
| Compressor kernel | `_external/fmod/DSPCompressor.c`, `readInternal`, `0x10357659c`; stereo scalar tail `0x1035fc9e0..0x1035fca7e` | Summed stereo power, two-stage envelope and scalar powf gain |

Scalar reconstruction is not a claim of identical SIMD approximations or complete Studio audio parity. Device output, backend scheduling, source-channel adaptation and codec behavior still differ. `robloxDspParity` remains false.
