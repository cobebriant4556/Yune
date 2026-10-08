use std::collections::{BTreeMap, BTreeSet, VecDeque};
use lune_roblox::instance::Instance;
use rbx_dom_weak::types::Variant;
use super::{key, boolean, reference, text};
use super::pcm::{self, Frame};
use super::effects::{self, Effect};
use super::spatial::{self, CurveMap};

pub const CLASSES: &[&str] = &["AudioPlayer", "AudioFader", "AudioAnalyzer", "AudioDeviceOutput", "AudioEmitter", "AudioListener", "AudioChorus", "AudioFlanger", "AudioDistortion", "AudioEcho", "AudioEqualizer", "AudioFilter", "AudioCompressor", "AudioLimiter", "AudioGate", "AudioPitchShifter", "AudioReverb", "AudioTremolo"];

pub fn input_pins(class: &str) -> &'static [&'static str] {
    match class {
        "AudioCompressor" => &["Input", "Sidechain"],
        "AudioAnalyzer" | "AudioDeviceOutput" | "AudioEmitter" => &["Input"],
        class if effects::CLASSES.contains(&class) => &["Input"],
        _ => &[],
    }
}
pub fn output_pins(class: &str) -> &'static [&'static str] {
    match class {
        "AudioPlayer" | "AudioListener" => &["Output"],
        class if effects::CLASSES.contains(&class) => &["Output"],
        _ => &[],
    }
}

#[derive(Clone)]
pub struct Edge {
    pub wire: Instance, pub source: Instance, pub target: Instance,
    pub source_key: String, pub target_key: String, pub source_pin: String, pub target_pin: String,
}
impl Edge {
    pub fn identity(&self) -> String { format!("{}:{}:{}:{}:{}", key(&self.wire), self.source_key, self.target_key, self.source_pin, self.target_pin) }
}
struct SpatialEdge { source_key: String, target_key: String, gains: Frame }

pub struct Graph {
    pub nodes: BTreeMap<String, Instance>,
    pub edges: Vec<Edge>,
    pub order: Vec<String>,
    pub errors: Vec<String>,
    pub warnings: Vec<String>,
    spatial: Vec<SpatialEdge>,
}

impl Graph {
    pub fn build(game: Instance, curves: &CurveMap) -> Self {
        let descendants = game.get_descendants_preorder();
        let nodes = descendants.iter().filter(|node| CLASSES.contains(&node.get_class_name()))
            .map(|node| (key(node), *node)).collect::<BTreeMap<_, _>>();
        let live = descendants.iter().map(key).collect::<BTreeSet<_>>();
        let ranks = descendants.iter().enumerate().map(|(rank, node)| (key(node), rank)).collect::<BTreeMap<_, _>>();
        let mut graph = Self { nodes, edges: Vec::new(), order: Vec::new(), errors: Vec::new(), warnings: Vec::new(), spatial: Vec::new() };
        for wire in descendants.iter().filter(|node| node.get_class_name() == "Wire") { wire.set_property("Connected", Variant::Bool(false)); }
        if graph.nodes.len() > 512 { graph.errors.push("headless audio graph exceeds 512 nodes".into()); return graph; }
        let emitters = descendants.iter().filter(|node| node.get_class_name() == "AudioEmitter").collect::<Vec<_>>();
        let listeners = descendants.iter().filter(|node| node.get_class_name() == "AudioListener").collect::<Vec<_>>();
        let mut dependencies = BTreeMap::<String, Vec<String>>::new();
        for emitter in &emitters {
            for listener in &listeners {
                if !spatial::interacts(emitter, listener) { continue; }
                let (gain, pan) = spatial::attenuation(emitter, listener, curves);
                let source_key = key(emitter);
                let target_key = key(listener);
                dependencies.entry(source_key.clone()).or_default().push(target_key.clone());
                graph.spatial.push(SpatialEdge { source_key, target_key, gains: spatial::channel_gains(gain, pan) });
                if boolean(emitter, "AcousticSimulationEnabled", false) && boolean(listener, "AcousticSimulationEnabled", false) {
                    graph.warnings.push("AcousticSimulationEnabled requested: Yune currently renders direct-path spatial audio only; automatic occlusion, diffraction and room acoustics are not implemented".into());
                }
            }
        }
        for wire in descendants.iter().filter(|node| node.get_class_name() == "Wire") {
            let (Some(source), Some(target)) = (reference(wire, "SourceInstance"), reference(wire, "TargetInstance")) else { continue };
            let source_key = key(&source);
            let target_key = key(&target);
            if !live.contains(&source_key) || !live.contains(&target_key) { continue; }
            for node in [source, target] {
                if node.get_class_name().starts_with("Audio") && !CLASSES.contains(&node.get_class_name()) {
                    graph.errors.push(format!("unsupported audio node {} ({})", node.get_full_name(), node.get_class_name()));
                }
            }
            let source_pin = text(wire, "SourceName", "Output");
            let target_pin = text(wire, "TargetName", "Input");
            if !graph.nodes.contains_key(&source_key) || !graph.nodes.contains_key(&target_key)
                || !output_pins(source.get_class_name()).contains(&source_pin.as_str())
                || !input_pins(target.get_class_name()).contains(&target_pin.as_str()) { continue; }
            if source_key == target_key || reachable(&dependencies, &target_key, &source_key) {
                graph.warnings.push(format!("disconnected cyclic audio wire {} (including implicit emitter/listener feedback)", wire.get_full_name()));
                continue;
            }
            if graph.edges.len() >= 4096 { graph.errors.push("headless audio graph exceeds 4096 connected wires".into()); break; }
            dependencies.entry(source_key.clone()).or_default().push(target_key.clone());
            graph.edges.push(Edge { wire: *wire, source, target, source_key, target_key, source_pin, target_pin });
            wire.set_property("Connected", Variant::Bool(true));
        }
        let mut indegrees = graph.nodes.keys().map(|id| (id.clone(), 0_usize)).collect::<BTreeMap<_, _>>();
        for targets in dependencies.values() { for target in targets { *indegrees.get_mut(target).unwrap() += 1; } }
        let mut ready = indegrees.iter().filter(|(_, degree)| **degree == 0).map(|(id, _)| (ranks[id], id.clone())).collect::<BTreeSet<_>>();
        while let Some((_, id)) = ready.pop_first() {
            if let Some(targets) = dependencies.get(&id) {
                for target in targets {
                    let degree = indegrees.get_mut(target).unwrap();
                    *degree -= 1;
                    if *degree == 0 { ready.insert((ranks[target], target.clone())); }
                }
            }
            graph.order.push(id);
        }
        if graph.order.len() != graph.nodes.len() { graph.errors.push("audio graph cycle detected".into()); }
        graph.errors.sort(); graph.errors.dedup();
        graph.warnings.sort(); graph.warnings.dedup();
        graph
    }

    pub fn mix(&self, sources: BTreeMap<String, Vec<Frame>>, count: usize, analyzers: &mut BTreeMap<String, Analyzer>, effect_states: &mut BTreeMap<String, Effect>, local_player: Option<Instance>) -> Result<(Vec<Frame>, BTreeMap<String, Vec<Frame>>), String> {
        let mut output = sources;
        let mut master = vec![[0.0; 2]; count];
        for node_key in &self.order {
            let node = self.nodes[node_key];
            if node.get_class_name() == "AudioPlayer" { continue; }
            let mut stream = vec![[0.0; 2]; count];
            let mut sidechain = None;
            for edge in self.edges.iter().filter(|edge| &edge.target_key == node_key) {
                let target = if edge.target_pin == "Sidechain" { sidechain.get_or_insert_with(|| vec![[0.0; 2]; count]) } else { &mut stream };
                if let Some(source) = output.get(&edge.source_key) { add(target, source); }
            }
            match node.get_class_name() {
                "AudioListener" => {
                    for edge in self.spatial.iter().filter(|edge| &edge.target_key == node_key) {
                        if let Some(source) = output.get(&edge.source_key) {
                            for (target, sample) in stream.iter_mut().zip(source) {
                                let mono = sample[0] * 0.5 + sample[1] * 0.5;
                                target[0] += mono * edge.gains[0]; target[1] += mono * edge.gains[1];
                            }
                        }
                    }
                }
                class if effects::CLASSES.contains(&class) => {
                    effect_states.entry(node_key.clone()).or_default().process(&node, &mut stream, sidechain.as_deref());
                }
                "AudioAnalyzer" => {
                    analyzers.entry(node_key.clone()).or_default().process(&stream, boolean(&node, "SpectrumEnabled", true));
                }
                "AudioDeviceOutput" => {
                    let selected = reference(&node, "Player");
                    let audible = selected.is_none() || selected.zip(local_player).is_some_and(|(a, b)| key(&a) == key(&b));
                    if audible { add(&mut master, &stream); }
                }
                _ => {}
            }
            if stream.iter().flatten().any(|sample| !sample.is_finite()) {
                return Err(format!("non-finite audio at {} ({}); reduce graph gains and reset effect state", node.get_full_name(), node.get_class_name()));
            }
            output.insert(node_key.clone(), stream);
        }
        if master.iter().flatten().any(|sample| !sample.is_finite()) { return Err("non-finite master mix; reduce graph gains".into()); }
        Ok((master, output))
    }
}

fn add(target: &mut [Frame], source: &[Frame]) {
    for (target, sample) in target.iter_mut().zip(source) { target[0] += sample[0]; target[1] += sample[1]; }
}
fn reachable(edges: &BTreeMap<String, Vec<String>>, start: &str, target: &str) -> bool {
    let mut pending = vec![start];
    let mut seen = BTreeSet::new();
    while let Some(node) = pending.pop() {
        if node == target { return true; }
        if !seen.insert(node) { continue; }
        if let Some(targets) = edges.get(node) { pending.extend(targets.iter().map(String::as_str)); }
    }
    false
}

#[derive(Default)]
pub struct Analyzer {
    pub peak: f64, pub rms: f64, history: VecDeque<f32>,
}
impl Analyzer {
    pub fn process(&mut self, frames: &[Frame], spectrum: bool) {
        let metrics = pcm::metrics(frames);
        self.peak = metrics.peak; self.rms = metrics.rms;
        if spectrum {
            self.history.extend(frames.iter().map(|frame| frame[0] * 0.5 + frame[1] * 0.5));
            while self.history.len() > 2048 { self.history.pop_front(); }
        } else { self.history.clear(); }
    }
    pub fn spectrum(&self, window: u32) -> Vec<f32> {
        use rustfft::{num_complex::Complex32, FftPlanner};
        let size = match window { 0 => 512, 2 => 2048, _ => 1024 };
        let mut values = vec![Complex32::new(0.0, 0.0); size];
        let samples = self.history.iter().rev().take(size).copied().collect::<Vec<_>>();
        let offset = size - samples.len();
        let mut normalization = 0.0;
        for index in 0..size {
            let window = 0.5 - 0.5 * (std::f32::consts::TAU * index as f32 / size as f32).cos();
            normalization += window;
            if index >= offset { values[index].re = samples[size - index - 1] * window; }
        }
        FftPlanner::<f32>::new().plan_fft_forward(size).process(&mut values);
        values[..=size / 2].iter().enumerate().map(|(index, value)| {
            let scale = if index == 0 || index == size / 2 { 1.0 } else { std::f32::consts::SQRT_2 };
            value.norm() * scale / normalization
        }).collect()
    }
}
