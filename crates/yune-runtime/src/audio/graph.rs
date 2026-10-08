use std::collections::{BTreeMap, BTreeSet, VecDeque};
use lune_roblox::instance::Instance;
use rbx_dom_weak::types::Variant;
use super::{key, number, boolean, reference, text};
use super::pcm::{self, Frame};

pub const CLASSES: &[&str] = &["AudioPlayer", "AudioFader", "AudioAnalyzer", "AudioDeviceOutput"];

pub fn input_pins(class: &str) -> &'static [&'static str] {
    match class { "AudioFader" | "AudioAnalyzer" | "AudioDeviceOutput" => &["Input"], _ => &[] }
}
pub fn output_pins(class: &str) -> &'static [&'static str] {
    match class { "AudioPlayer" | "AudioFader" => &["Output"], _ => &[] }
}

#[derive(Clone)]
pub struct Edge {
    pub wire: Instance,
    pub source: Instance,
    pub target: Instance,
    pub source_key: String,
    pub target_key: String,
    pub source_pin: String,
    pub target_pin: String,
}
impl Edge {
    pub fn identity(&self) -> String { format!("{}:{}:{}:{}:{}", key(&self.wire), self.source_key, self.target_key, self.source_pin, self.target_pin) }
}

pub struct Graph {
    pub nodes: BTreeMap<String, Instance>,
    pub edges: Vec<Edge>,
    pub order: Vec<String>,
    pub errors: Vec<String>,
}

impl Graph {
    pub fn build(game: Instance) -> Self {
        let descendants = game.get_descendants_preorder();
        let nodes = descendants.iter().filter(|node| CLASSES.contains(&node.get_class_name()))
            .map(|node| (key(node), *node)).collect::<BTreeMap<_, _>>();
        let live = descendants.iter().map(key).collect::<BTreeSet<_>>();
        let mut graph = Self { nodes, edges: Vec::new(), order: Vec::new(), errors: Vec::new() };
        for wire in descendants.iter().filter(|node| node.get_class_name() == "Wire") {
            wire.set_property("Connected", Variant::Bool(false));
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
            if source_key == target_key || reachable(&graph.edges, &target_key, &source_key) { continue; }
            graph.edges.push(Edge { wire: *wire, source, target, source_key, target_key, source_pin, target_pin });
            wire.set_property("Connected", Variant::Bool(true));
        }
        let mut pending = graph.nodes.keys().cloned().collect::<BTreeSet<_>>();
        while !pending.is_empty() {
            let ready = pending.iter().filter(|node| !graph.edges.iter().any(|edge| edge.target_key == **node && pending.contains(&edge.source_key)))
                .cloned().collect::<Vec<_>>();
            if ready.is_empty() { graph.errors.push("audio graph cycle detected".into()); break; }
            for node in ready { pending.remove(&node); graph.order.push(node); }
        }
        graph.errors.sort();
        graph.errors.dedup();
        graph
    }

    pub fn mix(&self, sources: BTreeMap<String, Vec<Frame>>, count: usize, analyzers: &mut BTreeMap<String, Analyzer>, local_player: Option<Instance>) -> (Vec<Frame>, BTreeMap<String, Vec<Frame>>) {
        let mut output = sources;
        let mut master = vec![[0.0; 2]; count];
        for node_key in &self.order {
            let node = self.nodes[node_key];
            if node.get_class_name() == "AudioPlayer" { continue; }
            let mut stream = vec![[0.0; 2]; count];
            for edge in self.edges.iter().filter(|edge| &edge.target_key == node_key) {
                if let Some(source) = output.get(&edge.source_key) {
                    for (target, sample) in stream.iter_mut().zip(source) { target[0] += sample[0]; target[1] += sample[1]; }
                }
            }
            match node.get_class_name() {
                "AudioFader" => {
                    let gain = if boolean(&node, "Bypass", false) { 1.0 } else { number(&node, "Volume", 1.0).clamp(0.0, 3.0) as f32 };
                    for sample in &mut stream { sample[0] *= gain; sample[1] *= gain; }
                }
                "AudioAnalyzer" => {
                    analyzers.entry(node_key.clone()).or_default().process(&stream, boolean(&node, "SpectrumEnabled", true));
                }
                "AudioDeviceOutput" => {
                    let selected = reference(&node, "Player");
                    let audible = selected.is_none() || selected.zip(local_player).is_some_and(|(a, b)| key(&a) == key(&b));
                    if audible {
                        for (target, sample) in master.iter_mut().zip(&stream) { target[0] += sample[0]; target[1] += sample[1]; }
                    }
                }
                _ => {}
            }
            output.insert(node_key.clone(), stream);
        }
        (master, output)
    }
}

fn reachable(edges: &[Edge], start: &str, target: &str) -> bool {
    let mut pending = vec![start];
    let mut seen = BTreeSet::new();
    while let Some(node) = pending.pop() {
        if node == target { return true; }
        if !seen.insert(node) { continue; }
        for edge in edges.iter().filter(|edge| edge.source_key == node) { pending.push(&edge.target_key); }
    }
    false
}

#[derive(Default)]
pub struct Analyzer {
    pub peak: f64,
    pub rms: f64,
    history: VecDeque<f32>,
}
impl Analyzer {
    pub fn process(&mut self, frames: &[Frame], spectrum: bool) {
        let metrics = pcm::metrics(frames);
        self.peak = metrics.peak;
        self.rms = metrics.rms;
        if spectrum {
            self.history.extend(frames.iter().map(|frame| (frame[0] + frame[1]) * 0.5));
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
