use std::{collections::BTreeMap, sync::Arc};
use super::pcm::{Clip, Frame, SAMPLE_RATE};

#[derive(Clone, Copy, Debug)]
pub struct Config {
    pub volume: f32,
    pub speed: f64,
    pub looping: bool,
    pub playback: (f64, f64),
    pub loop_region: (f64, f64),
}
impl Default for Config {
    fn default() -> Self { Self { volume: 1.0, speed: 1.0, looping: false, playback: (0.0, 0.0), loop_region: (0.0, 0.0) } }
}

#[derive(Default)]
pub struct Player {
    pub asset: String,
    pub clip: Option<Arc<Clip>>,
    pub position: f64,
    pub playing: bool,
    pub entered: bool,
    pub config: Config,
    pub actions: BTreeMap<(u64, u64), bool>,
}

impl Player {
    pub fn is_playing(&self) -> bool { self.playing || self.actions.values().any(|play| *play) }
    pub fn cancel(&mut self, id: Option<u64>) -> bool {
        let Some(id) = id else { return false };
        let key = self.actions.keys().find(|(_, action)| *action == id).copied();
        key.is_some_and(|key| self.actions.remove(&key).is_some())
    }
    pub fn duration(&self) -> f64 { self.clip.as_ref().map_or(0.0, |clip| clip.duration()) }
    fn bounds(&self) -> (f64, f64, f64, f64) {
        let length = self.duration();
        let (min, max) = self.config.playback;
        let (start, end) = if min == max { (0.0, length) } else { (min.clamp(0.0, length), max.clamp(0.0, length)) };
        let (min, max) = self.config.loop_region;
        let (loop_start, loop_end) = if min == max { (start, end) } else { (min.max(start), max.min(end)) };
        if loop_end <= loop_start { (start, end, start, end) }
        else { (start, end, loop_start, loop_end) }
    }
    pub fn command(&mut self, play: bool) {
        if play {
            let (start, end, _, _) = self.bounds();
            if self.position < start || self.position >= end { self.position = start; }
            self.playing = self.clip.is_some() && end > start;
        } else { self.playing = false; }
    }
    pub fn render(&mut self, first_sample: u64, count: usize, events: &mut Vec<(&'static str, u64)>) -> Vec<Frame> {
        let mut output = vec![[0.0; 2]; count];
        for (index, frame) in output.iter_mut().enumerate() {
            let clock = first_sample + index as u64;
            while let Some((&key, &play)) = self.actions.first_key_value() {
                if key.0 > clock { break; }
                self.actions.remove(&key);
                let before = self.is_playing();
                self.command(play);
                if before != self.is_playing() { events.push(("IsPlaying", clock)); }
            }
            if !self.playing || self.config.speed == 0.0 { continue; }
            let Some(clip) = self.clip.as_ref() else { continue };
            let (start, end, loop_start, loop_end) = self.bounds();
            let boundary = if self.config.looping { loop_end } else { end };
            if boundary <= start {
                self.playing = false;
                events.push(("IsPlaying", clock));
                events.push(("Ended", clock));
                continue;
            }
            if self.position < start { self.position = start; }
            if self.position >= boundary {
                if self.config.looping { self.position = loop_start + (self.position - loop_start).rem_euclid(loop_end - loop_start); }
                else {
                    self.position = end;
                    self.playing = false;
                    events.push(("IsPlaying", clock));
                    events.push(("Ended", clock));
                    continue;
                }
            }
            let sample = clip.sample(self.position);
            *frame = [sample[0] * self.config.volume, sample[1] * self.config.volume];
            self.position += self.config.speed / SAMPLE_RATE as f64;
            if self.position + 1e-12 >= boundary {
                if self.config.looping {
                    let span = loop_end - loop_start;
                    let crossings = ((self.position - loop_end).max(0.0) / span).floor() as usize + 1;
                    self.position = loop_start + (self.position - loop_end).max(0.0).rem_euclid(span);
                    // Degenerate sub-sample regions are bounded to one notification per output sample.
                    for _ in 0..crossings.min(1) { events.push(("Looped", clock + 1)); }
                } else {
                    self.position = end;
                    self.playing = false;
                    events.push(("IsPlaying", clock + 1));
                    events.push(("Ended", clock + 1));
                }
            }
        }
        output
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn player() -> Player {
        let clip = Arc::new(Clip::new(48_000, 1, vec![0.5; 480]).unwrap());
        Player { clip: Some(clip), ..Default::default() }
    }
    #[test]
    fn stop_preserves_position_and_resume_continues() {
        let mut player = player();
        player.command(true);
        player.render(0, 100, &mut Vec::new());
        player.command(false);
        let position = player.position;
        assert!(player.render(100, 20, &mut Vec::new()).iter().all(|frame| *frame == [0.0; 2]));
        assert_eq!(player.position, position);
        player.command(true);
        player.render(120, 10, &mut Vec::new());
        assert!(player.position > position);
    }
    #[test]
    fn scheduled_actions_are_sample_accurate() {
        let mut player = player();
        player.actions.insert((10, 1), true);
        player.actions.insert((30, 2), false);
        let frames = player.render(0, 50, &mut Vec::new());
        assert!(frames[..10].iter().all(|frame| *frame == [0.0; 2]));
        assert!(frames[10..30].iter().all(|frame| *frame == [0.5; 2]));
        assert!(frames[30..].iter().all(|frame| *frame == [0.0; 2]));
        assert!((player.position - 20.0 / 48_000.0).abs() < 1e-12);
    }
    #[test]
    fn cancelling_scheduled_play_removes_intention() {
        let mut player = player();
        player.actions.insert((100, 9), true);
        assert!(player.is_playing());
        assert!(player.cancel(Some(9)));
        assert!(!player.cancel(Some(9)));
        assert!(!player.is_playing());
    }
    #[test]
    fn ended_once_and_never_on_stop() {
        let mut player = player();
        player.command(true);
        let mut events = Vec::new();
        player.render(0, 600, &mut events);
        assert_eq!(events.iter().filter(|(event, _)| *event == "Ended").count(), 1);
        assert_eq!(events.last(), Some(&("Ended", 480)));
        events.clear();
        player.command(false);
        player.render(600, 20, &mut events);
        assert!(events.is_empty());
    }
    #[test]
    fn speed_and_loop_regions() {
        let mut player = player();
        player.config.playback = (0.002, 0.008);
        player.config.loop_region = (0.004, 0.006);
        player.config.looping = true;
        player.config.speed = 2.0;
        player.command(true);
        let mut events = Vec::new();
        player.render(0, 96, &mut events);
        assert!((player.position - 0.004).abs() < 1e-9);
        assert_eq!(events.iter().filter(|(event, _)| *event == "Looped").count(), 1);
        assert!(player.playing);
    }
    #[test]
    fn rendering_is_independent_of_block_partition() {
        let mut a = player();
        let mut b = player();
        for player in [&mut a, &mut b] {
            player.config.looping = true;
            player.config.speed = 1.5;
            player.actions.insert((17, 1), true);
        }
        let expected = a.render(0, 1024, &mut Vec::new());
        let mut actual = Vec::new();
        for index in 0..8 { actual.extend(b.render(index * 128, 128, &mut Vec::new())); }
        assert_eq!(actual, expected);
        assert_eq!(a.position, b.position);
    }
}
