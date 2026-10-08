use std::f64::consts::{FRAC_1_SQRT_2, TAU};
use lune_roblox::instance::Instance;
use rbx_dom_weak::types::Variant;
use super::{boolean, enum_value, number, pcm::Frame};
use super::dsp::{self, Biquad, Delay, Ramp, RATE};
use super::pitch::Pitch;

pub const CLASSES: &[&str] = &["AudioFader", "AudioChorus", "AudioFlanger", "AudioDistortion", "AudioEcho", "AudioEqualizer", "AudioFilter", "AudioCompressor", "AudioLimiter", "AudioGate", "AudioPitchShifter", "AudioReverb", "AudioTremolo"];

pub const PARAMETERS: &[(&str, &str, f64, f64, f64)] = &[
    ("AudioFader", "Volume", 1.0, 0.0, 3.0),
    ("AudioChorus", "Depth", 0.15, 0.0, 1.0), ("AudioChorus", "Mix", 0.5, 0.0, 1.0), ("AudioChorus", "Rate", 0.5, 0.0, 20.0),
    ("AudioFlanger", "Depth", 0.45, 0.0, 1.0), ("AudioFlanger", "Mix", 0.85, 0.0, 1.0), ("AudioFlanger", "Rate", 5.0, 0.0, 20.0),
    ("AudioDistortion", "Level", 0.5, 0.0, 1.0),
    ("AudioEcho", "DelayTime", 0.25, 0.001, 5.0), ("AudioEcho", "Feedback", 0.5, 0.0, 1.0),
    ("AudioEcho", "DryLevel", 0.0, -80.0, 10.0), ("AudioEcho", "WetLevel", 0.0, -80.0, 10.0), ("AudioEcho", "RampTime", 0.0, 0.0, 60000.0),
    ("AudioEqualizer", "LowGain", 0.0, -80.0, 10.0), ("AudioEqualizer", "MidGain", 0.0, -80.0, 10.0), ("AudioEqualizer", "HighGain", 0.0, -80.0, 10.0),
    ("AudioFilter", "Frequency", 2000.0, 20.0, 22000.0), ("AudioFilter", "Q", FRAC_1_SQRT_2, 0.1, 10.0), ("AudioFilter", "Gain", 0.0, -30.0, 30.0),
    ("AudioCompressor", "Attack", 0.1, 0.0001, 0.5), ("AudioCompressor", "Release", 0.1, 0.01, 5.0),
    ("AudioCompressor", "Threshold", -40.0, -60.0, 0.0), ("AudioCompressor", "Ratio", 40.0, 1.0, 50.0), ("AudioCompressor", "MakeupGain", 0.0, -30.0, 30.0),
    ("AudioLimiter", "MaxLevel", 0.0, -12.0, 0.0), ("AudioLimiter", "Release", 0.01, 0.001, 1.0),
    ("AudioGate", "Attack", 0.01, 0.001, 5.0), ("AudioGate", "Release", 0.1, 0.001, 5.0),
    ("AudioPitchShifter", "Pitch", 1.0, 0.5, 2.0),
    ("AudioReverb", "DecayTime", 1.49, 0.1, 20.0), ("AudioReverb", "DecayRatio", 0.83, 0.1, 1.0),
    ("AudioReverb", "Density", 1.0, 0.1, 1.0), ("AudioReverb", "Diffusion", 1.0, 0.1, 1.0),
    ("AudioReverb", "DryLevel", 0.0, -80.0, 20.0), ("AudioReverb", "WetLevel", 0.0, -80.0, 20.0),
    ("AudioReverb", "EarlyDelayTime", 0.02, 0.0, 0.3), ("AudioReverb", "LateDelayTime", 0.03, 0.0, 0.1),
    ("AudioReverb", "HighCutFrequency", 20000.0, 20.0, 20000.0), ("AudioReverb", "LowShelfFrequency", 250.0, 20.0, 20000.0),
    ("AudioReverb", "LowShelfGain", 0.0, -36.0, 12.0), ("AudioReverb", "ReferenceFrequency", 5000.0, 20.0, 20000.0),
    ("AudioTremolo", "Depth", 1.0, 0.0, 1.0), ("AudioTremolo", "Duty", 0.5, 0.0, 1.0),
    ("AudioTremolo", "Frequency", 5.0, 0.1, 20.0), ("AudioTremolo", "Shape", 0.0, 0.0, 1.0),
    ("AudioTremolo", "Skew", 0.0, -1.0, 1.0), ("AudioTremolo", "Square", 0.0, 0.0, 1.0),
];

fn value(node: &Instance, name: &str) -> f64 {
    let (_, _, default, min, max) = PARAMETERS.iter().find(|(class, property, _, _, _)| *class == node.get_class_name() && *property == name).expect("effect parameter definition");
    number(node, name, *default).clamp(*min, *max)
}

pub fn range(node: &Instance, name: &str, default: (f64, f64)) -> (f64, f64) {
    match node.get_property(name) {
        Some(Variant::NumberRange(value)) if value.min.is_finite() && value.max.is_finite() => (f64::from(value.min), f64::from(value.max)),
        _ => default,
    }
}

pub fn filter_response(node: &Instance, frequency: f64) -> f64 {
    if boolean(node, "Bypass", false) { return 0.0; }
    let coefficients = dsp::filter(enum_value(node, "FilterType", 0), value(node, "Frequency"), value(node, "Q"), value(node, "Gain"));
    10.0 * coefficients.iter().map(|coefficient| coefficient.power(frequency)).product::<f64>().max(1e-30).log10()
}

pub fn memory_bound(class: &str) -> usize {
    match class {
        "AudioEcho" => 2_000_000, "AudioReverb" => 400_000, "AudioPitchShifter" => 1_000_000,
        "AudioChorus" | "AudioFlanger" => 32_000, _ => 2048,
    }
}

pub struct Effect {
    delays: Vec<Delay>, filters: [Biquad; 4], phase: f64, gain: f64,
    envelope: f64, gate_open: bool, ramp: Ramp, damping: [[f64; 2]; 4], pitch: Option<Pitch>,
}
impl Default for Effect {
    fn default() -> Self {
        Self { delays: Vec::new(), filters: std::array::from_fn(|_| Biquad::default()), phase: 0.0,
            gain: 1.0, envelope: 0.0, gate_open: false, ramp: Ramp::default(), damping: [[0.0; 2]; 4], pitch: None }
    }
}
impl Effect {
    pub fn process(&mut self, node: &Instance, stream: &mut [Frame], sidechain: Option<&[Frame]>) {
        if boolean(node, "Bypass", false) { return; }
        match node.get_class_name() {
            "AudioFader" => {
                let gain = value(node, "Volume") as f32;
                for frame in stream { for sample in frame { *sample *= gain; } }
            }
            "AudioFilter" => {
                let coefficients = dsp::filter(enum_value(node, "FilterType", 0), value(node, "Frequency"), value(node, "Q"), value(node, "Gain"));
                for frame in stream {
                    for channel in 0..2 {
                        let mut sample = f64::from(frame[channel]);
                        for (state, coefficient) in self.filters.iter_mut().zip(&coefficients) { sample = state.sample(sample, channel, *coefficient); }
                        frame[channel] = sample as f32;
                    }
                }
            }
            "AudioEqualizer" => {
                let (low, high) = range(node, "MidRange", (400.0, 4000.0));
                let mid = value(node, "MidGain");
                let coefficients = [dsp::biquad(1, low.clamp(200.0, 20000.0), FRAC_1_SQRT_2, value(node, "LowGain") - mid),
                    dsp::biquad(2, high.clamp(200.0, 20000.0), FRAC_1_SQRT_2, value(node, "HighGain") - mid)];
                for frame in stream {
                    for channel in 0..2 {
                        let sample = self.filters[0].sample(f64::from(frame[channel]), channel, coefficients[0]);
                        frame[channel] = (self.filters[1].sample(sample, channel, coefficients[1]) * dsp::db(mid)) as f32;
                    }
                }
            }
            "AudioEcho" => self.echo(node, stream),
            "AudioChorus" | "AudioFlanger" => self.modulated_delay(node, stream),
            "AudioDistortion" => {
                let level = value(node, "Level");
                if level == 0.0 { return; }
                let drive = 1.0 + 30.0 * level;
                for frame in stream { for sample in frame { *sample = ((f64::from(*sample) * drive).tanh() / drive.tanh()) as f32; } }
            }
            "AudioCompressor" | "AudioLimiter" | "AudioGate" => self.dynamics(node, stream, sidechain),
            "AudioTremolo" => self.tremolo(node, stream),
            "AudioReverb" => self.reverb(node, stream),
            "AudioPitchShifter" => {
                let size = match enum_value(node, "WindowSize", 1) { 0 => 512, 2 => 2048, _ => 1024 };
                if self.pitch.as_ref().is_none_or(|pitch| pitch.size != size) { self.pitch = Some(Pitch::new(size)); }
                let pitch = self.pitch.as_mut().unwrap();
                let ratio = value(node, "Pitch");
                for frame in stream { *frame = pitch.sample(*frame, ratio); }
            }
            _ => {}
        }
    }
    fn echo(&mut self, node: &Instance, stream: &mut [Frame]) {
        if self.delays.is_empty() { self.delays.push(Delay::new((5.0 * RATE) as usize + 2)); }
        self.ramp.set(value(node, "DelayTime") * RATE, value(node, "RampTime"));
        let feedback = value(node, "Feedback");
        let dry = dsp::db(value(node, "DryLevel"));
        let wet = dsp::db(value(node, "WetLevel"));
        for frame in stream {
            let delay = self.ramp.next();
            let mut stored = [0.0; 2];
            for channel in 0..2 {
                let echo = self.delays[0].read(delay, channel);
                let input = f64::from(frame[channel]);
                stored[channel] = (input + echo * feedback) as f32;
                frame[channel] = (input * dry + echo * wet) as f32;
            }
            self.delays[0].push(stored);
        }
    }
    fn modulated_delay(&mut self, node: &Instance, stream: &mut [Frame]) {
        if self.delays.is_empty() { self.delays.push(Delay::new((0.06 * RATE) as usize + 2)); }
        let flanger = node.get_class_name() == "AudioFlanger";
        let depth = value(node, "Depth");
        let mix = value(node, "Mix");
        let rate = value(node, "Rate") / RATE;
        let voices = if flanger { 1 } else { 3 };
        for frame in stream {
            let mut stored = *frame;
            for channel in 0..2 {
                let mut delayed = 0.0;
                for voice in 0..voices {
                    let phase = TAU * (self.phase + voice as f64 / 3.0 + channel as f64 / 4.0);
                    let seconds = if flanger { 0.002 + 0.0019 * depth * phase.sin() } else { 0.018 + 0.008 * depth * phase.sin() };
                    delayed += self.delays[0].read(seconds * RATE, channel) / voices as f64;
                }
                if flanger { stored[channel] += (delayed * 0.5 * depth) as f32; }
                frame[channel] = (f64::from(frame[channel]) * (1.0 - mix) + delayed * mix) as f32;
            }
            self.delays[0].push(stored);
            self.phase = (self.phase + rate).fract();
        }
    }
    fn dynamics(&mut self, node: &Instance, stream: &mut [Frame], sidechain: Option<&[Frame]>) {
        let class = node.get_class_name();
        let release = dsp::smoothing(value(node, "Release"));
        if class == "AudioGate" {
            let attack = dsp::smoothing(value(node, "Attack"));
            let (low, high) = range(node, "Threshold", (-36.0, -24.0));
            let (low, high) = (dsp::db(low.clamp(-80.0, 30.0)), dsp::db(high.clamp(-80.0, 30.0)));
            let detector_release = dsp::smoothing(0.01);
            for frame in stream {
                let peak = f64::from(frame[0].abs().max(frame[1].abs()));
                self.envelope = peak.max(self.envelope * detector_release);
                if self.envelope > high { self.gate_open = true; }
                else if self.envelope < low { self.gate_open = false; }
                let target = if self.gate_open { 1.0 } else { 0.0 };
                if self.phase == 0.0 { self.gain = 0.0; self.phase = 1.0; }
                let coefficient = if self.gate_open { attack } else { release };
                self.gain = target + coefficient * (self.gain - target);
                for sample in frame { *sample *= self.gain as f32; }
            }
            return;
        }
        if class == "AudioLimiter" {
            let ceiling = dsp::db(value(node, "MaxLevel"));
            for frame in stream {
                let peak = f64::from(frame[0].abs().max(frame[1].abs()));
                let target = (ceiling / peak.max(1e-30)).min(1.0);
                self.gain = if target < self.gain { target } else { target + release * (self.gain - target) };
                for sample in frame { *sample = (f64::from(*sample) * self.gain) as f32; }
            }
            return;
        }
        let attack = dsp::smoothing(value(node, "Attack"));
        let threshold = value(node, "Threshold");
        let ratio = value(node, "Ratio");
        let makeup = dsp::db(value(node, "MakeupGain"));
        for (index, frame) in stream.iter_mut().enumerate() {
            let detector = sidechain.map_or(*frame, |samples| samples[index]);
            let peak = f64::from(detector[0].abs().max(detector[1].abs()));
            let excess = (20.0 * peak.max(1e-30).log10() - threshold).max(0.0);
            let target = dsp::db(-excess * (1.0 - 1.0 / ratio));
            let coefficient = if target < self.gain { attack } else { release };
            self.gain = target + coefficient * (self.gain - target);
            for sample in frame { *sample = (f64::from(*sample) * self.gain * makeup) as f32; }
        }
    }
    fn tremolo(&mut self, node: &Instance, stream: &mut [Frame]) {
        let depth = value(node, "Depth");
        let duty = value(node, "Duty");
        let shape = value(node, "Shape");
        let split = 0.5 + 0.499 * value(node, "Skew");
        let square = value(node, "Square");
        let increment = value(node, "Frequency") / RATE;
        for frame in stream {
            let p = if self.phase < split { 0.5 * self.phase / split } else { 0.5 + 0.5 * (self.phase - split) / (1.0 - split) };
            let triangle = 1.0 - (2.0 * p - 1.0).abs();
            let sine = 0.5 - 0.5 * (TAU * p).cos();
            let wave = triangle * (1.0 - shape) + sine * shape;
            let wave = ((wave - 0.5) / (1.0 - square).max(0.0001) + duty).clamp(0.0, 1.0);
            let gain = (1.0 - depth * (1.0 - wave)) as f32;
            for sample in frame { *sample *= gain; }
            self.phase = (self.phase + increment).fract();
        }
    }
    fn reverb(&mut self, node: &Instance, stream: &mut [Frame]) {
        if self.delays.is_empty() {
            self.delays = vec![Delay::new((RATE * 0.3) as usize + 2), Delay::new((RATE * 0.1) as usize + 2)];
            self.delays.extend((0..4).map(|_| Delay::new(2200)));
            self.delays.extend([Delay::new(300), Delay::new(150)]);
        }
        let early_delay = value(node, "EarlyDelayTime") * RATE;
        let late_delay = value(node, "LateDelayTime") * RATE;
        let dry = dsp::db(value(node, "DryLevel"));
        let wet = dsp::db(value(node, "WetLevel"));
        let decay = value(node, "DecayTime");
        let ratio = value(node, "DecayRatio");
        let density = 0.5 + 0.5 * value(node, "Density");
        let diffusion = 0.7 * value(node, "Diffusion");
        let damp = (-TAU * value(node, "ReferenceFrequency") / RATE).exp();
        let high_cut = dsp::biquad(3, value(node, "HighCutFrequency"), FRAC_1_SQRT_2, 0.0);
        let low_shelf = dsp::biquad(1, value(node, "LowShelfFrequency"), FRAC_1_SQRT_2, value(node, "LowShelfGain"));
        let combs: [[(f64, f64, f64); 2]; 4] = std::array::from_fn(|line| std::array::from_fn(|channel| {
            let length = [1423.0, 1559.0, 1613.0, 1789.0][line] * density + channel as f64 * 23.0;
            (length, 10.0_f64.powf(-3.0 * length / (RATE * decay)), 10.0_f64.powf(-3.0 * length / (RATE * decay * ratio)))
        }));
        for frame in stream {
            let input = *frame;
            let shaped: Frame = std::array::from_fn(|channel| {
                let x = self.filters[0].sample(f64::from(input[channel]), channel, high_cut);
                self.filters[1].sample(x, channel, low_shelf) as f32
            });
            let early: Frame = std::array::from_fn(|channel| if early_delay < 1.0 { shaped[channel] } else { self.delays[0].read(early_delay, channel) as f32 });
            self.delays[0].push(shaped);
            let late: Frame = std::array::from_fn(|channel| if late_delay < 1.0 { early[channel] } else { self.delays[1].read(late_delay, channel) as f32 });
            self.delays[1].push(early);
            let mut reflected = [0.0_f64; 2];
            for (line, parameters) in combs.iter().enumerate() {
                let mut stored = [0.0; 2];
                for channel in 0..2 {
                    let (length, low_gain, high_gain) = parameters[channel];
                    let delayed = self.delays[line + 2].read(length, channel);
                    self.damping[line][channel] = delayed * (1.0 - damp) + self.damping[line][channel] * damp;
                    let feedback = self.damping[line][channel] * low_gain + (delayed - self.damping[line][channel]) * high_gain;
                    stored[channel] = (f64::from(late[channel]) + feedback) as f32;
                    reflected[channel] += delayed * 0.25;
                }
                self.delays[line + 2].push(stored);
            }
            for line in 0..2 {
                let mut stored = [0.0; 2];
                for channel in 0..2 {
                    let delayed = self.delays[line + 6].read([211.0, 73.0][line] + channel as f64 * 17.0, channel);
                    let output = delayed - diffusion * reflected[channel];
                    stored[channel] = (reflected[channel] + diffusion * output) as f32;
                    reflected[channel] = output;
                }
                self.delays[line + 6].push(stored);
            }
            for channel in 0..2 { frame[channel] = (f64::from(input[channel]) * dry + (0.25 * f64::from(early[channel]) + 0.75 * reflected[channel]) * wet) as f32; }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn node(class: &str, values: &[(&str, f64)]) -> Instance {
        let instance = Instance::new_orphaned(class);
        for (name, value) in values { instance.set_property(*name, Variant::Float64(*value)); }
        instance
    }
    #[test]
    fn echo_retains_history_across_irregular_blocks() {
        let node = node("AudioEcho", &[("DelayTime", 0.01), ("Feedback", 0.5), ("DryLevel", 0.0), ("WetLevel", 0.0)]);
        let mut source = vec![[0.0; 2]; 2000]; source[0] = [1.0, -1.0];
        let mut whole = source.clone();
        Effect::default().process(&node, &mut whole, None);
        let mut effect = Effect::default();
        for block in source.chunks_mut(73) { effect.process(&node, block, None); }
        assert_eq!(whole, source);
        assert_eq!(source[0], [1.0, -1.0]);
        assert_eq!(source[480], [1.0, -1.0]);
        assert_eq!(source[960], [0.5, -0.5]);
        assert_eq!(source[1440], [0.25, -0.25]);
    }
    #[test]
    fn bypass_is_bit_exact_for_every_effect() {
        for class in CLASSES {
            let node = node(class, &[]); node.set_property("Bypass", Variant::Bool(true));
            let original = vec![[0.23, -0.48]; 300]; let mut actual = original.clone();
            Effect::default().process(&node, &mut actual, None);
            assert_eq!(actual, original, "{class}");
        }
    }
    #[test]
    fn limiter_enforces_stereo_linked_sample_ceiling() {
        let node = node("AudioLimiter", &[("MaxLevel", -6.0)]);
        let mut stream = vec![[2.0, -1.0]; 1000];
        Effect::default().process(&node, &mut stream, None);
        for sample in stream { assert!(sample[0] <= dsp::db(-6.0) as f32 + 1e-7); assert!((sample[0] + 2.0 * sample[1]).abs() < 1e-7); }
    }
    #[test]
    fn compressor_uses_sidechain_without_adding_it_to_output() {
        let node = node("AudioCompressor", &[("Attack", 0.0001), ("Threshold", -20.0), ("Ratio", 10.0)]);
        let mut stream = vec![[0.1; 2]; 2000]; let detector = vec![[1.0; 2]; 2000];
        Effect::default().process(&node, &mut stream, Some(&detector));
        assert!((stream[1999][0] - (0.1 * dsp::db(-18.0)) as f32).abs() < 1e-6);
        let mut silence = vec![[0.0; 2]; 2000];
        Effect::default().process(&node, &mut silence, Some(&detector));
        assert!(silence.iter().all(|frame| *frame == [0.0; 2]));
    }
    #[test]
    fn all_effects_produce_finite_output_and_reverb_has_a_tail() {
        for class in CLASSES {
            let node = node(class, &[]);
            let mut stream = vec![[0.0; 2]; 12000]; stream[0] = [0.5; 2];
            Effect::default().process(&node, &mut stream, None);
            assert!(stream.iter().flatten().all(|sample| sample.is_finite()), "{class}");
            if *class == "AudioReverb" { assert!(stream[4000..].iter().flatten().any(|sample| sample.abs() > 1e-5)); }
        }
    }
}
