use std::{f64::consts::{PI, TAU}, sync::Arc};
use rustfft::{Fft, FftPlanner, num_complex::Complex32};
use super::pcm::Frame;

pub struct Pitch {
    pub size: usize,
    input: Vec<Frame>,
    output: Vec<Frame>,
    input_at: usize,
    output_at: usize,
    hop_count: usize,
    window: Vec<f32>,
    last_phase: [Vec<f64>; 2],
    phase: [Vec<f64>; 2],
    magnitude: Vec<f64>,
    frequency: Vec<f64>,
    work: Vec<Complex32>,
    forward: Arc<dyn Fft<f32>>,
    inverse: Arc<dyn Fft<f32>>,
    scratch: Vec<Complex32>,
}
impl Pitch {
    pub fn new(size: usize) -> Self {
        let mut planner = FftPlanner::new();
        let forward = planner.plan_fft_forward(size);
        let inverse = planner.plan_fft_inverse(size);
        let scratch_len = forward.get_inplace_scratch_len().max(inverse.get_inplace_scratch_len());
        Self {
            size, input: vec![[0.0; 2]; size], output: vec![[0.0; 2]; size * 2],
            input_at: 0, output_at: 0, hop_count: 0,
            window: (0..size).map(|i| (0.5 - 0.5 * (TAU * i as f64 / size as f64).cos()) as f32).collect(),
            last_phase: std::array::from_fn(|_| vec![0.0; size / 2 + 1]),
            phase: std::array::from_fn(|_| vec![0.0; size / 2 + 1]),
            magnitude: vec![0.0; size / 2 + 1], frequency: vec![0.0; size / 2 + 1],
            work: vec![Complex32::default(); size], forward, inverse,
            scratch: vec![Complex32::default(); scratch_len],
        }
    }
    pub fn sample(&mut self, input: Frame, ratio: f64) -> Frame {
        let output = self.output[self.output_at];
        self.output[self.output_at] = [0.0; 2];
        self.input[self.input_at] = input;
        self.input_at = (self.input_at + 1) % self.size;
        self.hop_count += 1;
        if self.hop_count == self.size / 4 {
            self.transform(ratio.clamp(0.5, 2.0));
            self.hop_count = 0;
        }
        self.output_at = (self.output_at + 1) % self.output.len();
        output
    }
    fn transform(&mut self, ratio: f64) {
        let phase_step = TAU / 4.0;
        for channel in 0..2 {
            for i in 0..self.size {
                self.work[i] = Complex32::new(self.input[(self.input_at + i) % self.size][channel] * self.window[i], 0.0);
            }
            self.forward.process_with_scratch(&mut self.work, &mut self.scratch);
            self.magnitude.fill(0.0);
            self.frequency.fill(0.0);
            for bin in 0..=self.size / 2 {
                let value = self.work[bin];
                let phase = f64::from(value.arg());
                let delta = (phase - self.last_phase[channel][bin] - phase_step * bin as f64 + PI).rem_euclid(TAU) - PI;
                self.last_phase[channel][bin] = phase;
                let true_bin = bin as f64 + delta / phase_step;
                let destination = (bin as f64 * ratio).floor() as usize;
                if destination <= self.size / 2 {
                    let magnitude = f64::from(value.norm());
                    self.magnitude[destination] += magnitude;
                    self.frequency[destination] += magnitude * true_bin * ratio;
                }
            }
            self.work.fill(Complex32::default());
            for bin in 0..=self.size / 2 {
                let magnitude = self.magnitude[bin];
                let frequency = if magnitude > 1e-20 { self.frequency[bin] / magnitude } else { bin as f64 };
                self.phase[channel][bin] = (self.phase[channel][bin] + phase_step * frequency).rem_euclid(TAU);
                let value = Complex32::from_polar(magnitude as f32, self.phase[channel][bin] as f32);
                self.work[bin] = if bin == 0 || bin == self.size / 2 { Complex32::new(value.re, 0.0) } else { value };
                if bin > 0 && bin < self.size / 2 { self.work[self.size - bin] = value.conj(); }
            }
            self.inverse.process_with_scratch(&mut self.work, &mut self.scratch);
            for i in 0..self.size {
                let target = (self.output_at + 1 + i) % self.output.len();
                self.output[target][channel] += self.work[i].re * self.window[i] / (self.size as f32 * 1.5);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use super::super::dsp::RATE;
    #[test]
    fn unity_pitch_reconstructs_with_one_window_latency() {
        for size in [512, 1024, 2048] {
            let mut pitch = Pitch::new(size);
            let source = (0..12000).map(|i| ((TAU * 437.0 * i as f64 / RATE).sin() * 0.3) as f32).collect::<Vec<_>>();
            for i in 0..source.len() {
                let output = pitch.sample([source[i], -source[i]], 1.0);
                let expected = if i >= size { source[i - size] } else { 0.0 };
                assert!((output[0] - expected).abs() < 0.0002, "size={size} sample={i}: {} versus {expected}", output[0]);
                assert!((output[0] + output[1]).abs() < 0.0002);
            }
        }
    }
    #[test]
    fn pitch_changes_frequency_without_changing_stream_length() {
        let mut pitch = Pitch::new(2048);
        let mut previous = 0.0;
        let mut crossings = 0;
        for i in 0..96000 {
            let input = ((TAU * 440.0 * i as f64 / RATE).sin() * 0.2) as f32;
            let output = pitch.sample([input; 2], 1.5)[0];
            if i >= 48000 && previous <= 0.0 && output > 0.0 { crossings += 1; }
            previous = output;
            assert!(output.is_finite());
        }
        assert!((crossings as i32 - 660).abs() <= 4, "measured {crossings} Hz");
    }
}
