use std::f64::consts::{PI, TAU};
use super::pcm::{Frame, SAMPLE_RATE};

pub const RATE: f64 = SAMPLE_RATE as f64;
pub fn db(value: f64) -> f64 { 10.0_f64.powf(value / 20.0) }
pub fn smoothing(seconds: f64) -> f64 { (-1.0 / (seconds.max(1.0 / RATE) * RATE)).exp() }

#[derive(Clone, Copy, Debug)]
pub struct Coefficients { b: [f64; 3], a: [f64; 2] }
impl Default for Coefficients {
    fn default() -> Self { Self { b: [1.0, 0.0, 0.0], a: [0.0; 2] } }
}
impl Coefficients {
    fn normalized(b: [f64; 3], a: [f64; 3]) -> Self {
        Self { b: b.map(|v| v / a[0]), a: [a[1] / a[0], a[2] / a[0]] }
    }
    pub fn power(self, frequency: f64) -> f64 {
        let w = TAU * frequency.clamp(0.0, RATE / 2.0) / RATE;
        let numerator = (self.b[0] + self.b[1] * w.cos() + self.b[2] * (2.0 * w).cos()).powi(2)
            + (self.b[1] * w.sin() + self.b[2] * (2.0 * w).sin()).powi(2);
        let denominator = (1.0 + self.a[0] * w.cos() + self.a[1] * (2.0 * w).cos()).powi(2)
            + (self.a[0] * w.sin() + self.a[1] * (2.0 * w).sin()).powi(2);
        numerator / denominator.max(1e-30)
    }
}

#[derive(Clone, Default)]
pub struct Biquad { z: [[f64; 2]; 2] }
impl Biquad {
    pub fn sample(&mut self, x: f64, channel: usize, c: Coefficients) -> f64 {
        let y = c.b[0] * x + self.z[channel][0];
        self.z[channel][0] = c.b[1] * x - c.a[0] * y + self.z[channel][1];
        self.z[channel][1] = c.b[2] * x - c.a[1] * y;
        y
    }
}

pub fn filter(kind: u32, frequency: f64, q: f64, gain: f64) -> Vec<Coefficients> {
    let sections = match kind { 4 | 7 => 2, 5 | 8 => 4, _ => 1 };
    let base = match kind { 4 | 5 => 3, 7 | 8 => 6, _ => kind };
    (0..sections).map(|index| {
        let section_q = if sections == 1 { q } else {
            let butterworth = 1.0 / (2.0 * ((2 * index + 1) as f64 * PI / (4 * sections) as f64).cos());
            butterworth * q / std::f64::consts::FRAC_1_SQRT_2
        };
        biquad(base, frequency, section_q, gain)
    }).collect()
}

pub fn biquad(kind: u32, frequency: f64, q: f64, gain: f64) -> Coefficients {
    let w = TAU * frequency.clamp(1.0, RATE * 0.499) / RATE;
    let (s, c) = w.sin_cos();
    let alpha = s / (2.0 * q.clamp(0.01, 100.0));
    let a = 10.0_f64.powf(gain / 40.0);
    let shelf = s * std::f64::consts::SQRT_2 * a.sqrt();
    let (b, den) = match kind {
        0 => ([1.0 + alpha * a, -2.0 * c, 1.0 - alpha * a], [1.0 + alpha / a, -2.0 * c, 1.0 - alpha / a]),
        1 => ([a * ((a + 1.0) - (a - 1.0) * c + shelf), 2.0 * a * ((a - 1.0) - (a + 1.0) * c), a * ((a + 1.0) - (a - 1.0) * c - shelf)],
            [(a + 1.0) + (a - 1.0) * c + shelf, -2.0 * ((a - 1.0) + (a + 1.0) * c), (a + 1.0) + (a - 1.0) * c - shelf]),
        2 => ([a * ((a + 1.0) + (a - 1.0) * c + shelf), -2.0 * a * ((a - 1.0) + (a + 1.0) * c), a * ((a + 1.0) + (a - 1.0) * c - shelf)],
            [(a + 1.0) - (a - 1.0) * c + shelf, 2.0 * ((a - 1.0) - (a + 1.0) * c), (a + 1.0) - (a - 1.0) * c - shelf]),
        3 => ([(1.0 - c) / 2.0, 1.0 - c, (1.0 - c) / 2.0], [1.0 + alpha, -2.0 * c, 1.0 - alpha]),
        6 => ([(1.0 + c) / 2.0, -(1.0 + c), (1.0 + c) / 2.0], [1.0 + alpha, -2.0 * c, 1.0 - alpha]),
        9 => ([alpha, 0.0, -alpha], [1.0 + alpha, -2.0 * c, 1.0 - alpha]),
        10 => ([1.0, -2.0 * c, 1.0], [1.0 + alpha, -2.0 * c, 1.0 - alpha]),
        11 => {
            let k = (w / 2.0).tan();
            ([k, k, 0.0], [1.0 + k, k - 1.0, 0.0])
        }
        _ => return Coefficients::default(),
    };
    Coefficients::normalized(b, den)
}

pub struct Delay { samples: Vec<Frame>, cursor: usize }
impl Delay {
    pub fn new(capacity: usize) -> Self { Self { samples: vec![[0.0; 2]; capacity.max(2)], cursor: 0 } }
    pub fn read(&self, frames: f64, channel: usize) -> f64 {
        let frames = frames.clamp(1.0, (self.samples.len() - 1) as f64);
        let whole = frames.floor() as usize;
        let fraction = frames - whole as f64;
        let a = (self.cursor + self.samples.len() - whole) % self.samples.len();
        let b = (a + self.samples.len() - 1) % self.samples.len();
        f64::from(self.samples[a][channel]) * (1.0 - fraction) + f64::from(self.samples[b][channel]) * fraction
    }
    pub fn push(&mut self, frame: Frame) {
        self.samples[self.cursor] = frame;
        self.cursor = (self.cursor + 1) % self.samples.len();
    }
}

#[derive(Default)]
pub struct Ramp { value: f64, target: Option<f64>, increment: f64, remaining: u64 }
impl Ramp {
    pub fn set(&mut self, target: f64, seconds: f64) {
        if self.target == Some(target) { return; }
        let first = self.target.is_none();
        self.target = Some(target);
        self.remaining = (seconds * RATE).round() as u64;
        if first || self.remaining == 0 { self.value = target; self.remaining = 0; }
        else { self.increment = (target - self.value) / self.remaining as f64; }
    }
    pub fn next(&mut self) -> f64 {
        if self.remaining > 0 {
            self.value += self.increment;
            self.remaining -= 1;
            if self.remaining == 0 { self.value = self.target.unwrap_or(self.value); }
        }
        self.value
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn delay_is_sample_exact_and_fractional() {
        let mut delay = Delay::new(100);
        for i in 0..20 { delay.push([i as f32, -i as f32]); }
        assert_eq!(delay.read(1.0, 0), 19.0);
        assert_eq!(delay.read(4.5, 0), 15.5);
        assert_eq!(delay.read(4.5, 1), -15.5);
    }
    #[test]
    fn filter_response_matches_design() {
        let low = biquad(3, 1000.0, std::f64::consts::FRAC_1_SQRT_2, 0.0);
        assert!((low.power(0.0) - 1.0).abs() < 1e-10);
        assert!((low.power(1000.0) - 0.5).abs() < 1e-10);
        assert!(low.power(12000.0) < 0.0001);
        let peak = biquad(0, 1000.0, 1.0, 6.0);
        assert!((10.0 * peak.power(1000.0).log10() - 6.0).abs() < 1e-9);
        for kind in [4, 5, 7, 8] {
            let power = filter(kind, 1000.0, std::f64::consts::FRAC_1_SQRT_2, 0.0).iter().map(|c| c.power(1000.0)).product::<f64>();
            assert!((power - 0.5).abs() < 1e-9);
        }
    }
    #[test]
    fn all_filter_types_are_stable_at_parameter_extremes() {
        for kind in 0..12 {
            for frequency in [20.0, 1000.0, 22000.0] {
                for q in [0.1, 0.707, 10.0] {
                    for gain in [-30.0, 0.0, 30.0] {
                        let coefficients = filter(kind, frequency, q, gain);
                        let mut states = vec![Biquad::default(); coefficients.len()];
                        for i in 0..8192 {
                            let mut y = if i == 0 { 1.0 } else { 0.0 };
                            for (state, coefficient) in states.iter_mut().zip(&coefficients) { y = state.sample(y, 0, *coefficient); }
                            assert!(y.is_finite() && y.abs() < 1e10);
                        }
                    }
                }
            }
        }
    }
    #[test]
    fn ramp_reaches_target_without_block_size_dependency() {
        let mut ramp = Ramp::default();
        ramp.set(10.0, 1.0);
        assert_eq!(ramp.next(), 10.0);
        ramp.set(20.0, 4.0 / RATE);
        assert_eq!([ramp.next(), ramp.next(), ramp.next(), ramp.next()], [12.5, 15.0, 17.5, 20.0]);
        assert_eq!(ramp.next(), 20.0);
    }
}
