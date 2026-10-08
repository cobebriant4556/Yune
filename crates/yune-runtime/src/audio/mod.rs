mod graph;
mod pcm;
mod player;
mod signals;
mod bindings;
pub use bindings::install;

use std::{cell::RefCell, collections::{BTreeMap, BTreeSet}, path::{Component, Path, PathBuf}, rc::Rc, sync::Arc};
use lune_roblox::instance::{Instance, instance_to_lua, registry::InstanceRegistry};
use mlua::prelude::*;
use rbx_dom_weak::types::{ContentType, Variant};
use graph::{Analyzer, Edge, Graph};
use pcm::{Clip, Frame, SAMPLE_RATE};
use player::{Config, Player};
use signals::AudioSignal;

struct Capture { source: Option<String>, samples: Vec<Frame>, max_frames: usize, start: u64 }
struct Trace { instance: String, event: String, sample: u64 }
enum Event { Signal(Instance, String), Property(Instance, String), Wiring(bool, Edge) }

pub struct AudioWorld {
    game: Instance,
    players: BTreeMap<String, (Instance, Player)>,
    assets: BTreeMap<String, Arc<Clip>>,
    root: Option<PathBuf>,
    clock: u64,
    fractional_samples: f64,
    next_action: u64,
    capture: Option<Capture>,
    signals: BTreeMap<(String, String), AudioSignal>,
    pending: Vec<Event>,
    traces: Vec<Trace>,
    wires: BTreeMap<String, Edge>,
    analyzers: BTreeMap<String, Analyzer>,
    diagnostics: BTreeSet<String>,
}
impl AudioWorld {
    pub fn new(game: Instance) -> Self {
        Self {
            game, players: BTreeMap::new(), assets: BTreeMap::new(), root: None, clock: 0,
            fractional_samples: 0.0, next_action: 0, capture: None, signals: BTreeMap::new(),
            pending: Vec::new(), traces: Vec::new(), wires: BTreeMap::new(),
            analyzers: BTreeMap::new(), diagnostics: BTreeSet::new(),
        }
    }
    fn signal(&mut self, instance: Instance, name: &str) -> AudioSignal {
        self.signals.entry((key(&instance), name.to_string())).or_default().clone()
    }
    fn trace(&mut self, instance: Instance, event: &str, sample: u64) {
        if self.traces.len() < 100_000 { self.traces.push(Trace { instance: key(&instance), event: event.into(), sample }); }
        else { self.diagnostics.insert("event trace budget exhausted; call audio.takeEvents() periodically".into()); }
    }
    fn changed(&mut self, instance: Instance, property: &str) {
        self.pending.push(Event::Property(instance, property.into()));
    }
    fn register(&mut self, id: String, clip: Arc<Clip>) -> Result<(), String> {
        let used = self.assets.iter().filter(|(key, _)| *key != &id).map(|(_, clip)| clip.samples.len() * 4).sum::<usize>();
        if used + clip.samples.len() * 4 > 512 * 1024 * 1024 { return Err("decoded audio cache exceeds 512 MiB".into()); }
        self.assets.insert(id, clip);
        Ok(())
    }
    fn resolve(&mut self, id: &str) -> Result<Arc<Clip>, String> {
        if let Some(clip) = self.assets.get(id) { return Ok(clip.clone()); }
        let root = self.root.as_ref().ok_or_else(|| format!("unmapped audio asset {id:?}; use audio.registerAsset or audio.setAssetRoot"))?;
        let suffix = id.strip_prefix("rbxasset://").unwrap_or(id);
        let candidates = if let Some(id) = id.strip_prefix("rbxassetid://") {
            if id.is_empty() || !id.bytes().all(|value| value.is_ascii_digit()) { return Err("invalid rbxassetid audio URI".into()); }
            ["", ".wav", ".mp3", ".ogg", ".flac", ".m4a"].iter().map(|extension| root.join(format!("{id}{extension}"))).collect::<Vec<_>>()
        } else {
            let path = Path::new(suffix);
            if path.components().any(|part| !matches!(part, Component::Normal(_))) { return Err("audio asset paths must stay inside the asset root".into()); }
            vec![root.join(path)]
        };
        let path = candidates.into_iter().find_map(|path| path.canonicalize().ok().filter(|path| path.starts_with(root) && path.is_file()))
            .ok_or_else(|| format!("audio asset not found inside asset root: {id}"))?;
        let clip = Arc::new(pcm::decode(&path)?);
        self.register(id.into(), clip.clone())?;
        Ok(clip)
    }
    fn refresh_player(&mut self, instance: Instance, force_load: bool) {
        let id = asset_id(&instance);
        let instance_key = key(&instance);
        let entry = self.players.entry(instance_key.clone()).or_insert_with(|| (instance, Player { position: number(&instance, "TimePosition", 0.0), ..Default::default() }));
        entry.0 = instance;
        if entry.1.asset != id {
            entry.1.asset = id.clone();
            entry.1.clip = None;
            entry.1.position = 0.0;
            instance.set_property("TimePosition", Variant::Float64(0.0));
        }
        let load = !id.is_empty() && entry.1.clip.is_none() && (force_load || boolean(&instance, "AutoLoad", true) || entry.1.is_playing());
        if load {
            match self.resolve(&id) {
                Ok(clip) => {
                    self.players.get_mut(&instance_key).unwrap().1.clip = Some(clip);
                    self.changed(instance, "IsReady");
                    self.changed(instance, "TimeLength");
                }
                Err(error) => { self.diagnostics.insert(error); }
            }
        }
        let entry = &mut self.players.get_mut(&instance_key).unwrap().1;
        entry.config = Config {
            volume: number(&instance, "Volume", 1.0).clamp(0.0, 10.0) as f32,
            speed: number(&instance, "PlaybackSpeed", 1.0).clamp(0.0, 20.0),
            looping: boolean(&instance, "Looping", false),
            playback: range(&instance, "PlaybackRegion"), loop_region: range(&instance, "LoopRegion"),
        };
    }
    fn refresh_graph(&mut self) -> Graph {
        let graph = Graph::build(self.game);
        let current = graph.edges.iter().map(|edge| (edge.identity(), edge.clone())).collect::<BTreeMap<_, _>>();
        for (id, edge) in &self.wires {
            if !current.contains_key(id) { self.pending.push(Event::Wiring(false, edge.clone())); }
        }
        for (id, edge) in &current {
            if !self.wires.contains_key(id) { self.pending.push(Event::Wiring(true, edge.clone())); }
        }
        self.wires = current;
        for error in &graph.errors { self.diagnostics.insert(error.clone()); }
        graph
    }
    fn command(&mut self, instance: Instance, play: bool, at: Option<f64>) -> LuaResult<Option<u64>> {
        if let Some(at) = at {
            finite(at, "atTime")?;
            if at < 0.0 || at * SAMPLE_RATE as f64 > u64::MAX as f64 { return Err(LuaError::runtime("atTime is outside the supported mixer clock range")); }
        }
        self.refresh_player(instance, play);
        let instance_key = key(&instance);
        if play && self.players[&instance_key].1.clip.is_none() {
            return Err(LuaError::runtime(format!("AudioPlayer asset is not ready: {:?}; register a local asset first", self.players[&instance_key].1.asset)));
        }
        let before = self.players[&instance_key].1.is_playing();
        let mut action_id = None;
        if let Some(at) = at {
            self.next_action = self.next_action.checked_add(1).ok_or_else(|| LuaError::runtime("audio action ID range exhausted"))?;
            action_id = Some(self.next_action);
            if at > self.clock as f64 / SAMPLE_RATE as f64 {
                let sample = (at * SAMPLE_RATE as f64 - 1e-7).ceil() as u64;
                let player = &mut self.players.get_mut(&instance_key).unwrap().1;
                if player.actions.len() >= 65_536 { return Err(LuaError::runtime("scheduled audio action budget exceeded")); }
                player.actions.insert((sample, self.next_action), play);
            } else { self.players.get_mut(&instance_key).unwrap().1.command(play); }
        } else { self.players.get_mut(&instance_key).unwrap().1.command(play); }
        if before != self.players[&instance_key].1.is_playing() { self.changed(instance, "IsPlaying"); }
        self.trace(instance, if play { "Play" } else { "Stop" }, self.clock);
        Ok(action_id)
    }
    pub fn advance(&mut self, dt: f64) -> LuaResult<()> {
        finite(dt, "audio delta time")?;
        if !(0.0..=60.0).contains(&dt) { return Err(LuaError::runtime("audio step must be between 0 and 60 seconds")); }
        let graph = self.refresh_graph();
        if !graph.errors.is_empty() { return Err(LuaError::runtime(graph.errors.join("; "))); }
        if graph.nodes.len() > 512 { return Err(LuaError::runtime("headless audio graph exceeds 512 nodes")); }
        let debt = self.fractional_samples + dt * SAMPLE_RATE as f64;
        let frames = (debt + 1e-7).floor() as usize;
        if let Some(capture) = &self.capture {
            if capture.samples.len() + frames > capture.max_frames { return Err(LuaError::runtime("audio capture duration budget exceeded")); }
        }
        self.fractional_samples = debt - frames as f64;
        for node in graph.nodes.values().filter(|node| node.get_class_name() == "AudioPlayer") {
            self.refresh_player(*node, false);
            let entry = &mut self.players.get_mut(&key(node)).unwrap().1;
            if !entry.entered {
                entry.entered = true;
                if boolean(node, "AutoPlay", false) { self.command(*node, true, None)?; }
            }
        }
        self.players.retain(|_, (instance, _)| Instance::new_opt(instance.dom_id, instance.dom_ref).is_some());
        self.analyzers.retain(|id, _| graph.nodes.contains_key(id));
        let local_player = self.game.get_children().into_iter().find(|child| child.get_class_name() == "Players")
            .and_then(|players| reference(&players, "LocalPlayer"));
        let mut remaining = frames;
        while remaining > 0 {
            let count = remaining.min(256);
            let mut sources = BTreeMap::new();
            for node in graph.nodes.values().filter(|node| node.get_class_name() == "AudioPlayer") {
                let mut events = Vec::new();
                let player = &mut self.players.get_mut(&key(node)).unwrap().1;
                let samples = player.render(self.clock, count, &mut events);
                node.set_property("TimePosition", Variant::Float64(player.position));
                for (event, sample) in events {
                    if event == "IsPlaying" { self.changed(*node, event); }
                    else { self.pending.push(Event::Signal(*node, event.into())); self.trace(*node, event, sample); }
                }
                sources.insert(key(node), samples);
            }
            let (master, mut streams) = graph.mix(sources, count, &mut self.analyzers, local_player);
            if let Some(capture) = &mut self.capture {
                let stream = match &capture.source {
                    Some(source) => streams.remove(source).unwrap_or_else(|| vec![[0.0; 2]; count]),
                    None => master,
                };
                capture.samples.extend(stream);
            }
            self.clock += count as u64;
            remaining -= count;
        }
        Ok(())
    }
}

pub fn dispatch(lua: &Lua, world: &Rc<RefCell<AudioWorld>>) -> LuaResult<()> {
    let events = std::mem::take(&mut world.borrow_mut().pending);
    for event in events {
        match event {
            Event::Signal(instance, name) => {
                let signal = world.borrow_mut().signal(instance, &name);
                signal.fire(lua, LuaMultiValue::new())?;
            }
            Event::Property(instance, name) => {
                let signal = world.borrow_mut().signal(instance, &format!("property:{name}"));
                signal.fire(lua, LuaMultiValue::new())?;
                let changed = world.borrow_mut().signal(instance, "Changed");
                changed.fire(lua, (name,).into_lua_multi(lua)?)?;
            }
            Event::Wiring(connected, edge) => {
                for (node, pin, other) in [(edge.source, edge.source_pin, edge.target), (edge.target, edge.target_pin, edge.source)] {
                    let signal = world.borrow_mut().signal(node, "WiringChanged");
                    let args = (connected, pin, instance_to_lua(lua, edge.wire)?, instance_to_lua(lua, other)?).into_lua_multi(lua)?;
                    signal.fire(lua, args)?;
                }
            }
        }
    }
    Ok(())
}

fn property(lua: &Lua, class: &str, name: &str, getter: LuaFunction, setter: Option<LuaFunction>) -> LuaResult<()> {
    InstanceRegistry::insert_property_getter(lua, class, name, getter).map_err(LuaError::external)?;
    let setter = match setter { Some(setter) => setter, None => {
        let name = name.to_string();
        lua.create_function(move |_, _: LuaMultiValue| -> LuaResult<()> { Err(LuaError::runtime(format!("{name} is read-only"))) })?
    }};
    InstanceRegistry::insert_property_setter(lua, class, name, setter).map_err(LuaError::external)
}
fn method(lua: &Lua, class: &str, name: &str, function: LuaFunction) -> LuaResult<()> {
    InstanceRegistry::insert_method(lua, class, name, function).map_err(LuaError::external)
}
fn finite(value: f64, name: &str) -> LuaResult<f64> {
    if value.is_finite() { Ok(value) } else { Err(LuaError::runtime(format!("{name} must be finite"))) }
}
fn key(instance: &Instance) -> String { instance.dom_ref.to_string() }
fn number(instance: &Instance, name: &str, default: f64) -> f64 {
    let value = match instance.get_property(name) { Some(Variant::Float64(value)) => value, Some(Variant::Float32(value)) => value as f64, Some(Variant::Int32(value)) => value as f64, _ => default };
    if value.is_finite() { value } else { default }
}
fn boolean(instance: &Instance, name: &str, default: bool) -> bool { match instance.get_property(name) { Some(Variant::Bool(value)) => value, _ => default } }
fn reference(instance: &Instance, name: &str) -> Option<Instance> { match instance.get_property(name) { Some(Variant::Ref(value)) => Instance::new_opt(instance.dom_id, value), _ => None } }
fn text(instance: &Instance, name: &str, default: &str) -> String { match instance.get_property(name) { Some(Variant::String(value)) => value, Some(Variant::ContentId(value)) => AsRef::<str>::as_ref(&value).to_string(), _ => default.into() } }
fn asset_id(instance: &Instance) -> String {
    if let Some(Variant::Content(content)) = instance.get_property("AudioContent") {
        if let ContentType::Uri(uri) = content.value() { if !uri.is_empty() { return uri.clone(); } }
    }
    let asset = text(instance, "Asset", "");
    if asset.is_empty() { text(instance, "AssetId", "") } else { asset }
}
fn range(instance: &Instance, name: &str) -> (f64, f64) {
    match instance.get_property(name) { Some(Variant::NumberRange(value)) => (value.min as f64, value.max as f64), _ => (0.0, 0.0) }
}
fn enum_value(instance: &Instance, name: &str, default: u32) -> u32 {
    match instance.get_property(name) { Some(Variant::Enum(value)) => value.to_u32(), Some(Variant::EnumItem(value)) => value.value, _ => default }
}
