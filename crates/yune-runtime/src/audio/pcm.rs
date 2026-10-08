use std::{fs::File, io::ErrorKind, path::Path};
use symphonia::core::{audio::SampleBuffer, codecs::DecoderOptions, errors::Error, formats::FormatOptions, io::MediaSourceStream, meta::MetadataOptions, probe::Hint};

pub type Frame = [f32; 2];
pub const SAMPLE_RATE: u32 = 48_000;
pub const MAX_FRAMES: usize = 28_800_000;

#[derive(Debug)]
pub struct Clip {
    pub sample_rate: u32,
    pub channels: usize,
    pub samples: Vec<f32>,
}

impl Clip {
    pub fn new(sample_rate: u32, channels: usize, samples: Vec<f32>) -> Result<Self, String> {
        if !(8_000..=192_000).contains(&sample_rate) { return Err("sample rate must be between 8000 and 192000 Hz".into()); }
        if !(1..=2).contains(&channels) { return Err("headless audio currently accepts mono and stereo assets only".into()); }
        if samples.is_empty() || samples.len() % channels != 0 { return Err("PCM must contain a whole, nonempty number of frames".into()); }
        if samples.len() / channels > MAX_FRAMES { return Err("decoded audio exceeds the 28.8 million frame limit".into()); }
        if samples.iter().any(|value| !value.is_finite()) { return Err("PCM contains non-finite samples".into()); }
        Ok(Self { sample_rate, channels, samples })
    }

    pub fn frames(&self) -> usize { self.samples.len() / self.channels }
    pub fn duration(&self) -> f64 { self.frames() as f64 / self.sample_rate as f64 }
    pub fn frame(&self, index: usize) -> Frame {
        let index = index.min(self.frames() - 1) * self.channels;
        [self.samples[index], self.samples[index + self.channels - 1]]
    }
    pub fn sample(&self, time: f64) -> Frame {
        let position = (time * self.sample_rate as f64).clamp(0.0, (self.frames() - 1) as f64);
        let index = position.floor() as usize;
        let alpha = (position - index as f64) as f32;
        let a = self.frame(index);
        let b = self.frame(index + 1);
        [a[0] + (b[0] - a[0]) * alpha, a[1] + (b[1] - a[1]) * alpha]
    }

    pub fn waveform(&self, start: f64, end: f64, count: usize) -> Vec<f32> {
        if count == 0 || start >= self.duration() || end <= start { return Vec::new(); }
        let start = start.max(0.0);
        let end = end.min(self.duration());
        (0..count).map(|i| {
            let frame = self.sample(start + (i as f64 + 0.5) * (end - start) / count as f64);
            ((frame[0] + frame[1]) * 0.5).clamp(-1.0, 1.0)
        }).collect()
    }
}

pub fn decode(path: &Path) -> Result<Clip, String> {
    let file = File::open(path).map_err(|error| format!("{}: {error}", path.display()))?;
    if file.metadata().map_err(|error| error.to_string())?.len() > 256 * 1024 * 1024 {
        return Err("encoded audio file exceeds 256 MiB".into());
    }
    let stream = MediaSourceStream::new(Box::new(file), Default::default());
    let mut hint = Hint::new();
    if let Some(extension) = path.extension().and_then(|value| value.to_str()) { hint.with_extension(extension); }
    let options = FormatOptions { enable_gapless: true, ..Default::default() };
    let probed = symphonia::default::get_probe().format(&hint, stream, &options, &MetadataOptions::default()).map_err(|error| error.to_string())?;
    let mut format = probed.format;
    let track = format.default_track().ok_or("audio file has no default track")?;
    let track_id = track.id;
    let mut decoder = symphonia::default::get_codecs().make(&track.codec_params, &DecoderOptions::default()).map_err(|error| error.to_string())?;
    let mut output = Vec::new();
    let mut sample_rate = 0;
    let mut channels = 0;
    loop {
        let packet = match format.next_packet() {
            Ok(packet) => packet,
            Err(Error::IoError(error)) if error.kind() == ErrorKind::UnexpectedEof => break,
            Err(error) => return Err(error.to_string()),
        };
        if packet.track_id() != track_id { continue; }
        let decoded = decoder.decode(&packet).map_err(|error| error.to_string())?;
        let spec = *decoded.spec();
        let count = spec.channels.count();
        if count == 0 || count > 2 { return Err("multichannel decoding requires an explicit downmix; only mono/stereo are supported".into()); }
        if sample_rate != 0 && (sample_rate != spec.rate || channels != count) { return Err("audio stream changes sample format mid-file".into()); }
        sample_rate = spec.rate;
        channels = count;
        let mut buffer = SampleBuffer::<f32>::new(decoded.capacity() as u64, spec);
        buffer.copy_interleaved_ref(decoded);
        if output.len() + buffer.len() > MAX_FRAMES * channels { return Err("decoded audio exceeds frame budget".into()); }
        output.extend_from_slice(buffer.samples());
    }
    Clip::new(sample_rate, channels, output)
}

#[derive(Clone, Copy, Default, Debug)]
pub struct Metrics {
    pub peak: f64,
    pub rms: f64,
    pub clipped_samples: usize,
}

pub fn metrics(frames: &[Frame]) -> Metrics {
    let mut result = Metrics::default();
    let mut sum = 0.0;
    for frame in frames {
        for sample in frame {
            let sample = *sample as f64;
            result.peak = result.peak.max(sample.abs());
            sum += sample * sample;
            if sample.abs() > 1.0 { result.clipped_samples += 1; }
        }
    }
    if !frames.is_empty() { result.rms = (sum / (frames.len() * 2) as f64).sqrt(); }
    result
}

pub fn write_wav(path: &Path, frames: &[Frame], float: bool) -> Result<Metrics, String> {
    if let Some(parent) = path.parent().filter(|path| !path.as_os_str().is_empty()) {
        std::fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    }
    let spec = hound::WavSpec {
        channels: 2, sample_rate: SAMPLE_RATE, bits_per_sample: if float { 32 } else { 16 },
        sample_format: if float { hound::SampleFormat::Float } else { hound::SampleFormat::Int },
    };
    let mut writer = hound::WavWriter::create(path, spec).map_err(|error| error.to_string())?;
    for frame in frames {
        for sample in frame {
            if !sample.is_finite() { return Err("cannot export non-finite audio".into()); }
            if float { writer.write_sample(*sample).map_err(|error| error.to_string())?; }
            else { writer.write_sample((sample.clamp(-1.0, 1.0) * 32767.0).round() as i16).map_err(|error| error.to_string())?; }
        }
    }
    writer.finalize().map_err(|error| error.to_string())?;
    Ok(metrics(frames))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn stereo_order_and_linear_interpolation() {
        let clip = Clip::new(48_000, 2, vec![0.0, 1.0, 1.0, 0.0]).unwrap();
        assert_eq!(clip.sample(0.5 / 48_000.0), [0.5, 0.5]);
        assert_eq!(clip.sample(0.0), [0.0, 1.0]);
    }
    #[test]
    fn invalid_pcm_is_rejected() {
        assert!(Clip::new(48_000, 1, vec![f32::NAN]).is_err());
        assert!(Clip::new(48_000, 2, vec![0.0]).is_err());
        assert!(Clip::new(0, 1, vec![0.0]).is_err());
    }
    #[test]
    fn wav_float_roundtrip() {
        let path = std::env::temp_dir().join(format!("yune-audio-roundtrip-{}.wav", std::process::id()));
        let frames = [[0.25, -0.5], [0.75, -0.125]];
        write_wav(&path, &frames, true).unwrap();
        let clip = decode(&path).unwrap();
        assert_eq!(clip.frame(0), frames[0]);
        assert_eq!(clip.frame(1), frames[1]);
        let _ = std::fs::remove_file(path);
    }
}
