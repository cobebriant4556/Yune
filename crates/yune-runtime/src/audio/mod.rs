mod graph;
mod pcm;
mod player;
mod signals;

use std::{cell::RefCell, collections::{BTreeMap, BTreeSet}, path::{Component, Path, PathBuf}, rc::Rc, sync::Arc};
use lune_roblox::{datatypes::types::{Content, NumberRange}, instance::{Instance, instance_to_lua, registry::InstanceRegistry}};
use mlua::prelude::*;
use rbx_dom_weak::types::{Content as DomContent, ContentType, NumberRange as DomRange, Variant};
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
        self.assets.insert(id.into(), clip.clone());
        Ok(clip)
    }
    fn refresh_player(&mut self, instance: Instance, force_load: bool) {
        let id = asset_id(&instance);
        let instance_key = key(&instance);
        let entry = self.players.entry(instance_key.clone()).or_insert_with(|| (instance, Player { position: number(&instance, "TimePosition", 0.0), ..Default::default() }));
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
        if let Some(at) = at { finite(at, "atTime")?; if at < 0.0 { return Err(LuaError::runtime("atTime cannot be negative")); } }
        self.refresh_player(instance, play);
        let instance_key = key(&instance);
        if play && self.players[&instance_key].1.clip.is_none() {
            return Err(LuaError::runtime(format!("AudioPlayer asset is not ready: {:?}; register a local asset first", self.players[&instance_key].1.asset)));
        }
        let before = self.players[&instance_key].1.is_playing();
        let mut action_id = None;
        if let Some(at) = at {
            self.next_action += 1;
            action_id = Some(self.next_action);
            if at > self.clock as f64 / SAMPLE_RATE as f64 {
                if at * SAMPLE_RATE as f64 > u64::MAX as f64 { return Err(LuaError::runtime("scheduled audio time exceeds clock range")); }
                let sample = (at * SAMPLE_RATE as f64 - 1e-7).ceil() as u64;
                self.players.get_mut(&instance_key).unwrap().1.actions.insert((sample, self.next_action), play);
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
        self.fractional_samples = (debt - frames as f64).max(0.0);
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

pub fn install(lua: &Lua, world: Rc<RefCell<AudioWorld>>) -> LuaResult<LuaTable> {
    for name in ["Asset", "AssetId", "AudioContent"] {
        let state = world.clone();
        let getter = lua.create_function(move |lua, instance: LuaUserDataRef<Instance>| {
            let id = asset_id(&instance);
            if name == "AudioContent" { Content::from(DomContent::from_uri(id)).into_lua(lua) }
            else { id.into_lua(lua) }
        })?;
        let setter = lua.create_function(move |lua, (instance, value): (LuaUserDataRef<Instance>, LuaValue)| {
            let id = if name == "AudioContent" {
                let LuaValue::UserData(value) = value else { return Err(LuaError::runtime("AudioContent requires a Content value")) };
                let content: DomContent = value.borrow::<Content>()?.clone().into();
                match content.value() { ContentType::None => String::new(), ContentType::Uri(uri) => uri.clone(), _ => return Err(LuaError::runtime("headless audio supports URI Content only")) }
            } else { String::from_lua(value, lua)? };
            instance.set_property("Asset", Variant::String(id.clone()));
            instance.set_property("AssetId", Variant::String(id.clone()));
            instance.set_property("AudioContent", Variant::Content(DomContent::from_uri(id)));
            { let mut state = state.borrow_mut(); state.refresh_player(*instance, false); state.changed(*instance, name); }
            dispatch(lua, &state)
        })?;
        property(lua, "AudioPlayer", name, getter, Some(setter))?;
    }
    for (name, default, min, max) in [("Volume", 1.0, 0.0, 10.0), ("PlaybackSpeed", 1.0, 0.0, 20.0), ("TimePosition", 0.0, 0.0, f64::MAX)] {
        let state = world.clone();
        let getter_state = world.clone();
        let getter = lua.create_function(move |_, instance: LuaUserDataRef<Instance>| {
            if name == "TimePosition" {
                let mut state = getter_state.borrow_mut(); state.refresh_player(*instance, false);
                Ok(state.players[&key(&instance)].1.position)
            } else { Ok(number(&instance, name, default)) }
        })?;
        let setter = lua.create_function(move |lua, (instance, value): (LuaUserDataRef<Instance>, f64)| {
            let value = finite(value, name)?.clamp(min, max);
            instance.set_property(name, Variant::Float64(value));
            { let mut state = state.borrow_mut(); state.refresh_player(*instance, false);
                if name == "TimePosition" { state.players.get_mut(&key(&instance)).unwrap().1.position = value; }
                state.changed(*instance, name);
            }
            dispatch(lua, &state)
        })?;
        property(lua, "AudioPlayer", name, getter, Some(setter))?;
    }
    for (name, default) in [("AutoLoad", true), ("AutoPlay", false), ("Looping", false)] {
        let state = world.clone();
        let getter = lua.create_function(move |_, instance: LuaUserDataRef<Instance>| Ok(boolean(&instance, name, default)))?;
        let setter = lua.create_function(move |lua, (instance, value): (LuaUserDataRef<Instance>, bool)| {
            instance.set_property(name, Variant::Bool(value));
            { let mut state = state.borrow_mut(); state.refresh_player(*instance, false); state.changed(*instance, name); }
            dispatch(lua, &state)
        })?;
        property(lua, "AudioPlayer", name, getter, Some(setter))?;
    }
    for name in ["PlaybackRegion", "LoopRegion"] {
        let state = world.clone();
        let getter = lua.create_function(move |_, instance: LuaUserDataRef<Instance>| {
            let (min, max) = range(&instance, name);
            Ok(NumberRange::from(DomRange { min: min as f32, max: max as f32 }))
        })?;
        let setter = lua.create_function(move |lua, (instance, value): (LuaUserDataRef<Instance>, LuaUserDataRef<NumberRange>)| {
            let value: DomRange = (*value).into();
            finite(value.min as f64, name)?; finite(value.max as f64, name)?;
            instance.set_property(name, Variant::NumberRange(value));
            { let mut state = state.borrow_mut(); state.refresh_player(*instance, false); state.changed(*instance, name); }
            dispatch(lua, &state)
        })?;
        property(lua, "AudioPlayer", name, getter, Some(setter))?;
    }
    for name in ["IsReady", "IsPlaying", "TimeLength"] {
        let state = world.clone();
        let getter = lua.create_function(move |lua, instance: LuaUserDataRef<Instance>| {
            let mut state = state.borrow_mut(); state.refresh_player(*instance, false);
            let player = &state.players[&key(&instance)].1;
            match name { "IsReady" => player.clip.is_some().into_lua(lua), "IsPlaying" => player.is_playing().into_lua(lua), _ => player.duration().into_lua(lua) }
        })?;
        property(lua, "AudioPlayer", name, getter, None)?;
    }
    for (name, play) in [("Play", true), ("Stop", false)] {
        let state = world.clone();
        method(lua, "AudioPlayer", name, lua.create_function(move |lua, (instance, at): (LuaUserDataRef<Instance>, Option<f64>)| {
            let result = state.borrow_mut().command(*instance, play, at)?;
            dispatch(lua, &state)?;
            Ok(result)
        })?)?;
    }
    let state = world.clone();
    method(lua, "AudioPlayer", "Cancel", lua.create_function(move |lua, (instance, id): (LuaUserDataRef<Instance>, Option<u64>)| {
        let result = {
            let mut state = state.borrow_mut(); state.refresh_player(*instance, false);
            let player = &mut state.players.get_mut(&key(&instance)).unwrap().1;
            let before = player.is_playing();
            let result = player.cancel(id);
            if before != player.is_playing() { state.changed(*instance, "IsPlaying"); }
            result
        };
        dispatch(lua, &state)?;
        Ok(result)
    })?)?;
    let state = world.clone();
    method(lua, "AudioPlayer", "GetWaveformAsync", lua.create_async_function(move |lua, (instance, requested, count): (LuaUserDataRef<Instance>, LuaUserDataRef<NumberRange>, usize)| {
        let state = state.clone();
        async move {
            if count > 65_536 { return Err(LuaError::runtime("waveform samples must not exceed 65536")); }
            let requested: DomRange = (*requested).into();
            finite(requested.min as f64, "waveform start")?; finite(requested.max as f64, "waveform end")?;
            let samples = { let mut state = state.borrow_mut(); state.refresh_player(*instance, true);
                state.players[&key(&instance)].1.clip.as_ref().map_or_else(Vec::new, |clip| clip.waveform(requested.min as f64, requested.max as f64, count))
            };
            lua.create_sequence_from(samples)
        }
    })?)?;
    let state = world.clone();
    method(lua, "SoundService", "GetMixerTime", lua.create_function(move |_, _: LuaUserDataRef<Instance>| Ok(state.borrow().clock as f64 / SAMPLE_RATE as f64))?)?;
    for class in graph::CLASSES {
        for (name, inputs) in [("GetInputPins", true), ("GetOutputPins", false)] {
            method(lua, class, name, lua.create_function(move |lua, instance: LuaUserDataRef<Instance>| {
                let pins = if inputs { graph::input_pins(instance.get_class_name()) } else { graph::output_pins(instance.get_class_name()) };
                lua.create_sequence_from(pins.iter().copied())
            })?)?;
        }
        let state = world.clone();
        method(lua, class, "GetConnectedWires", lua.create_function(move |lua, (instance, pin): (LuaUserDataRef<Instance>, String)| {
            let graph = state.borrow_mut().refresh_graph();
            let id = key(&instance);
            let wires = graph.edges.iter().filter(|edge| (edge.source_key == id && edge.source_pin == pin) || (edge.target_key == id && edge.target_pin == pin))
                .map(|edge| instance_to_lua(lua, edge.wire)).collect::<LuaResult<Vec<_>>>()?;
            dispatch(lua, &state)?;
            lua.create_sequence_from(wires)
        })?)?;
        for name in ["WiringChanged", "Changed"] {
            let state = world.clone();
            property(lua, class, name, lua.create_function(move |_, instance: LuaUserDataRef<Instance>| Ok(state.borrow_mut().signal(*instance, name)))?, None)?;
        }
        let state = world.clone();
        method(lua, class, "GetPropertyChangedSignal", lua.create_function(move |_, (instance, name): (LuaUserDataRef<Instance>, String)| {
            Ok(state.borrow_mut().signal(*instance, &format!("property:{name}")))
        })?)?;
    }
    for name in ["Ended", "Looped"] {
        let state = world.clone();
        property(lua, "AudioPlayer", name, lua.create_function(move |_, instance: LuaUserDataRef<Instance>| Ok(state.borrow_mut().signal(*instance, name)))?, None)?;
    }
    let state = world.clone();
    property(lua, "Wire", "Connected", lua.create_function(move |lua, instance: LuaUserDataRef<Instance>| {
        state.borrow_mut().refresh_graph(); dispatch(lua, &state)?;
        Ok(boolean(&instance, "Connected", false))
    })?, None)?;
    for name in ["PeakLevel", "RmsLevel"] {
        let state = world.clone();
        property(lua, "AudioAnalyzer", name, lua.create_function(move |_, instance: LuaUserDataRef<Instance>| {
            Ok(state.borrow().analyzers.get(&key(&instance)).map_or(0.0, |analyzer| if name == "PeakLevel" { analyzer.peak } else { analyzer.rms }))
        })?, None)?;
    }
    let state = world.clone();
    method(lua, "AudioAnalyzer", "GetSpectrum", lua.create_function(move |lua, instance: LuaUserDataRef<Instance>| {
        let values = if boolean(&instance, "SpectrumEnabled", true) {
            state.borrow().analyzers.get(&key(&instance)).map_or_else(Vec::new, |analyzer| analyzer.spectrum(enum_value(&instance, "WindowSize", 1)))
        } else { Vec::new() };
        lua.create_sequence_from(values)
    })?)?;
    install_module(lua, world)
}

fn install_module(lua: &Lua, world: Rc<RefCell<AudioWorld>>) -> LuaResult<LuaTable> {
    let module = lua.create_table()?;
    module.set("sampleRate", SAMPLE_RATE)?;
    module.set("channels", 2)?;
    let state = world.clone();
    module.set("registerAsset", lua.create_function(move |_, (id, path): (String, String)| {
        let clip = Arc::new(pcm::decode(Path::new(&path)).map_err(LuaError::runtime)?);
        let duration = clip.duration();
        state.borrow_mut().assets.insert(id, clip);
        Ok(duration)
    })?)?;
    let state = world.clone();
    module.set("registerPCM", lua.create_function(move |_, (id, rate, channels, buffer): (String, u32, usize, LuaBuffer)| {
        let bytes = buffer.as_slice();
        if bytes.len() % 4 != 0 || bytes.len() > pcm::MAX_FRAMES * 2 * 4 { return Err(LuaError::runtime("PCM buffer length is invalid")); }
        let samples = bytes.chunks_exact(4).map(|bytes| f32::from_le_bytes(bytes.try_into().unwrap())).collect();
        let clip = Arc::new(Clip::new(rate, channels, samples).map_err(LuaError::runtime)?);
        let duration = clip.duration();
        state.borrow_mut().assets.insert(id, clip);
        Ok(duration)
    })?)?;
    let state = world.clone();
    module.set("setAssetRoot", lua.create_function(move |_, path: String| {
        let path = Path::new(&path).canonicalize().map_err(LuaError::external)?;
        if !path.is_dir() { return Err(LuaError::runtime("audio asset root is not a directory")); }
        state.borrow_mut().root = Some(path); Ok(())
    })?)?;
    let state = world.clone();
    module.set("beginCapture", lua.create_function(move |_, options: Option<LuaTable>| {
        let mut max_seconds = 120.0;
        let mut source = None;
        if let Some(options) = options {
            max_seconds = options.get::<Option<f64>>("maxSeconds")?.unwrap_or(max_seconds);
            if let Some(value) = options.get::<Option<LuaAnyUserData>>("source")? { source = Some(key(&value.borrow::<Instance>()?)); }
        }
        finite(max_seconds, "maxSeconds")?;
        if !(0.0..=600.0).contains(&max_seconds) { return Err(LuaError::runtime("capture limit must be between 0 and 600 seconds")); }
        let mut state = state.borrow_mut();
        if state.capture.is_some() { return Err(LuaError::runtime("an audio capture is already active")); }
        state.capture = Some(Capture { source, samples: Vec::new(), max_frames: (max_seconds * SAMPLE_RATE as f64).round() as usize, start: state.clock });
        Ok(())
    })?)?;
    let state = world.clone();
    module.set("getCaptureBuffer", lua.create_function(move |lua, (): ()| {
        let state = state.borrow();
        let capture = state.capture.as_ref().ok_or_else(|| LuaError::runtime("no audio capture is active"))?;
        let bytes = capture.samples.iter().flat_map(|frame| frame.iter().flat_map(|sample| sample.to_le_bytes())).collect::<Vec<_>>();
        lua.create_buffer(bytes)
    })?)?;
    let state = world.clone();
    module.set("endCapture", lua.create_function(move |lua, (path, float): (String, Option<bool>)| {
        let mut state = state.borrow_mut();
        let capture = state.capture.as_ref().ok_or_else(|| LuaError::runtime("no audio capture is active"))?;
        let metrics = pcm::write_wav(Path::new(&path), &capture.samples, float.unwrap_or(false)).map_err(LuaError::runtime)?;
        let result = lua.create_table()?;
        result.set("path", path)?; result.set("sampleRate", SAMPLE_RATE)?; result.set("channels", 2)?;
        result.set("frames", capture.samples.len())?; result.set("duration", capture.samples.len() as f64 / SAMPLE_RATE as f64)?;
        result.set("startTime", capture.start as f64 / SAMPLE_RATE as f64)?; result.set("endTime", state.clock as f64 / SAMPLE_RATE as f64)?;
        result.set("peak", metrics.peak)?; result.set("rms", metrics.rms)?; result.set("clippedSamples", metrics.clipped_samples)?;
        state.capture = None;
        Ok(result)
    })?)?;
    let state = world.clone();
    module.set("step", lua.create_function(move |lua, seconds: f64| {
        state.borrow_mut().advance(seconds)?; dispatch(lua, &state)
    })?)?;
    let state = world.clone();
    module.set("getTime", lua.create_function(move |_, (): ()| Ok(state.borrow().clock as f64 / SAMPLE_RATE as f64))?)?;
    let state = world.clone();
    module.set("takeEvents", lua.create_function(move |lua, (): ()| {
        let traces = std::mem::take(&mut state.borrow_mut().traces);
        let output = lua.create_table()?;
        for trace in traces {
            let item = lua.create_table()?;
            item.set("instance", trace.instance)?; item.set("event", trace.event)?;
            item.set("sample", trace.sample)?; item.set("time", trace.sample as f64 / SAMPLE_RATE as f64)?;
            output.push(item)?;
        }
        Ok(output)
    })?)?;
    let state = world.clone();
    module.set("getDiagnostics", lua.create_function(move |lua, (): ()| lua.create_sequence_from(state.borrow().diagnostics.iter().cloned()))?)?;
    module.set("getCapabilities", lua.create_function(|lua, (): ()| {
        let result = lua.create_table()?;
        result.set("audioPlayer", true)?; result.set("scheduledPlayback", true)?; result.set("wireGraph", true)?;
        result.set("audioFader", true)?; result.set("audioAnalyzer", "approximate Hann-window spectrum")?;
        result.set("spatialAudio", false)?; result.set("robloxDspParity", false)?;
        result.set("unsupported", lua.create_sequence_from(["AudioEmitter", "AudioListener", "AudioReverb", "AudioEcho", "AudioFilter", "AudioEqualizer", "AudioCompressor", "AudioPitchShifter", "AudioDeviceInput", "AudioRecorder", "AudioTextToSpeech", "AudioSpeechToText", "multichannel output", "replication"])?;
        Ok(result)
    })?)?;
    Ok(module)
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
fn key(instance: &Instance) -> String { format!("{}:{}", instance.dom_id, instance.dom_ref) }
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
