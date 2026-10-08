use super::pcm::{Frame, SAMPLE_RATE};

// FMOD::DSPDistortion::readInternal, 0x1035829e6.
// Keep the f32 evaluation order and the limiting coefficient from the listing.
pub fn distortion(stream: &mut [Frame], level: f32) {
    let k = if level < 1.0 { (level + level) / (1.0 - level) } else { f32::from_bits(0x469c355d) };
    let scale = k + 1.0;
    for sample in stream.iter_mut().flatten() {
        *sample = (scale * *sample) / (sample.abs() * k + 1.0);
    }
}

// FMOD::DSPLimiter::{readInternal,setParameterFloatInternal},
// 0x10358cb8e / 0x10358cc7c. AudioLimiter leaves the DSP's default linked=false.
pub struct Limiter { envelope: [f32; 2] }
impl Default for Limiter {
    fn default() -> Self { Self { envelope: [1.0; 2] } }
}
impl Limiter {
    pub fn process(&mut self, stream: &mut [Frame], release_seconds: f32, max_level: f32) {
        let milliseconds = release_seconds * 1000.0;
        let seconds = milliseconds / 1000.0;
        let decay = seconds / (1.0 / SAMPLE_RATE as f32 + seconds);
        let ceiling = if max_level <= -80.0 { 0.0 } else { 10.0_f32.powf(max_level / 20.0) };
        for frame in stream {
            for (sample, envelope) in frame.iter_mut().zip(&mut self.envelope) {
                *envelope *= decay;
                let peak = sample.abs();
                if peak > *envelope { *envelope = peak; }
                *sample *= (ceiling / *envelope).min(1.0);
            }
        }
    }
}

// DSPCompressor::readInternal parameter preparation (0x10357659c),
// FMOD_DSP_Compressor_2channel_SSE scalar tail (0x1035fc9e0..0x1035fca7e).
// The scalar powf path is used for every sample, not the SIMD am_pow_eps approximation.
#[derive(Default)]
pub struct Compressor { first: f32, second: f32 }
impl Compressor {
    pub fn process(&mut self, stream: &mut [Frame], sidechain: Option<&[Frame]>, threshold_db: f32, ratio: f32, attack: f32, release: f32, makeup_db: f32) {
        let threshold = 10.0_f32.powf(threshold_db / 10.0);
        let inverse = 1.0 / threshold;
        let exponent = (1.0 / ratio - 1.0) * 0.5;
        let coefficient = |seconds: f32| 1.0 - (-3.11127_f32 / (SAMPLE_RATE as f32 * ((seconds * 1000.0) / 1000.0))).exp();
        let attack = coefficient(attack);
        let release = coefficient(release);
        let makeup = if makeup_db <= -80.0 { 0.0 } else { 10.0_f32.powf(makeup_db / 20.0) };
        for (index, frame) in stream.iter_mut().enumerate() {
            let detector = sidechain.and_then(|samples| samples.get(index)).copied().unwrap_or(*frame);
            let power = detector[0] * detector[0] + detector[1] * detector[1];
            let k = if power > self.second { attack } else { release };
            self.first = (power - self.first) * k + self.first;
            self.second = (self.first - self.second) * k + self.second;
            let gain = if self.second > threshold { (self.second * inverse).powf(exponent) * makeup } else { makeup };
            for sample in frame { *sample *= gain; }
        }
    }
}

// FMODPlaybackChannel::getEffectivePlaybackRegion / getEffectiveLoopRegion,
// 0x1034dbee2 / 0x1034dce64. Region comparisons and clamps use float32.
fn same_endpoint(start: f32, end: f32) -> bool {
    start == end || (start - end).abs() <= (start.abs() + 1.0) * 0.00001_f32
}
fn clamped_region(region: (f64, f64), low: f32, high: f32) -> (f32, f32) {
    let start = (region.0 as f32).clamp(low, high);
    let end = (region.1 as f32).clamp(low, high);
    if same_endpoint(start, end) { (low, high) } else { (start, end) }
}
pub fn regions(duration: f64, playback: (f64, f64), looping: (f64, f64)) -> (f64, f64, f64, f64) {
    let length = duration as f32;
    let (start, end) = clamped_region(playback, 0.0, length);
    let (loop_start, loop_end) = clamped_region(looping, 0.0, length);
    let (loop_start, loop_end) = clamped_region((loop_start as f64, loop_end as f64), start, end);
    (start as f64, end as f64, loop_start as f64, loop_end as f64)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn distortion_uses_rational_transfer_not_tanh() {
        let mut input = [[0.25, -0.25], [0.5, -0.5], [1.0, -1.0], [2.0, -2.0]];
        distortion(&mut input, 0.5);
        assert_eq!(input, [[0.5, -0.5], [0.75, -0.75], [1.0, -1.0], [1.2, -1.2]]);
        distortion(&mut input, 1.0);
        assert!(input.iter().flatten().all(|v| v.is_finite()));
    }
    #[test]
    fn distortion_zero_level_preserves_finite_samples() {
        let original = [[-0.0, 0.0], [0.1, -3.0]];
        let mut actual = original;
        distortion(&mut actual, 0.0);
        for (a, b) in actual.iter().flatten().zip(original.iter().flatten()) { assert_eq!(a.to_bits(), b.to_bits()); }
    }
    #[test]
    fn limiter_channels_have_independent_peak_histories() {
        let mut limiter = Limiter::default();
        let mut frames = [[2.0, 0.25], [0.5, 0.25]];
        limiter.process(&mut frames, 0.01, 0.0);
        assert_eq!(frames[0], [1.0, 0.25]);
        let seconds = (0.01_f32 * 1000.0) / 1000.0;
        let decay = seconds / (1.0 / 48000.0 + seconds);
        assert_eq!(frames[1][0], 0.5 * (1.0 / (2.0 * decay)));
        assert_eq!(frames[1][1], 0.25);
    }
    #[test]
    fn limiter_startup_envelopes_and_release_match_rc_recurrence() {
        let mut limiter = Limiter::default();
        let mut actual = [[0.1, -0.2]; 97];
        limiter.process(&mut actual, 0.01, -6.0);
        let seconds = (0.01_f32 * 1000.0) / 1000.0;
        let coefficient = seconds / (1.0 / 48000.0 + seconds);
        let ceiling = 10.0_f32.powf(-6.0 / 20.0);
        let mut envelope = 1.0_f32;
        for frame in actual {
            envelope *= coefficient;
            assert_eq!(frame, [0.1 * (ceiling / envelope), -0.2 * (ceiling / envelope)]);
        }
    }
    #[test]
    fn clamped_equal_and_fuzzy_regions_fall_back_to_full_span() {
        assert_eq!(regions(2.0, (9.0, 10.0), (9.0, 10.0)), (0.0, 2.0, 0.0, 2.0));
        assert_eq!(regions(2.0, (-9.0, -2.0), (1.0, 1.000001)), (0.0, 2.0, 0.0, 2.0));
        assert_eq!(regions(2.0, (0.5, 1.5), (1.7, 1.9)), (0.5, 1.5, 0.5, 1.5));
        assert_eq!(regions(2.0, (0.5, 1.5), (0.0, 1.0)), (0.5, 1.5, 0.5, 1.0));
    }
    #[test]
    fn compressor_uses_stereo_power_and_two_stage_detector() {
        let mut c = Compressor::default();
        let mut frames = [[0.5, 0.25]; 128];
        c.process(&mut frames, None, -20.0, 4.0, 0.001, 0.1, 0.0);
        let a = 1.0 - (-3.11127_f32 / (48000.0 * ((0.001_f32 * 1000.0) / 1000.0))).exp();
        let mut first = 0.0_f32;
        let mut second = 0.0_f32;
        for frame in frames {
            first = (0.3125 - first) * a + first;
            second = (first - second) * a + second;
            let threshold = 10.0_f32.powf(-20.0 / 10.0);
            let gain = if second > threshold { (second * (1.0 / threshold)).powf(-0.375) } else { 1.0 };
            assert_eq!(frame, [0.5 * gain, 0.25 * gain]);
        }
    }
    #[test]
    fn compressor_silent_sidechain_passes_input_and_partition_is_invariant() {
        let input = vec![[0.75, -0.5]; 1024];
        let mut dry = input.clone();
        Compressor::default().process(&mut dry, Some(&vec![[0.0; 2]; 1024]), -30.0, 10.0, 0.001, 0.1, 0.0);
        assert_eq!(dry, input);
        let mut whole = input.clone();
        Compressor::default().process(&mut whole, None, -30.0, 10.0, 0.001, 0.1, 0.0);
        let mut split = input;
        let mut effect = Compressor::default();
        for block in split.chunks_mut(17) { effect.process(block, None, -30.0, 10.0, 0.001, 0.1, 0.0); }
        assert_eq!(whole, split);
    }

}
