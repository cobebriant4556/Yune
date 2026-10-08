use super::*;
use mlua::Buffer as LuaBuffer;
use lune_roblox::datatypes::types::{Content, NumberRange};
use rbx_dom_weak::types::{Content as DomContent, NumberRange as DomRange};

pub fn install(lua: &Lua, world: Rc<RefCell<AudioWorld>>) -> LuaResult<LuaTable> {
    for name in ["Asset", "AssetId", "AudioContent"] {
        let state = world.clone();
        let getter = lua.create_function(move |lua, instance: LuaUserDataRef<Instance>| {
            let id = asset_id(&instance);
            if name == "AudioContent" { Content::from(if id.is_empty() { DomContent::none() } else { DomContent::from_uri(id) }).into_lua(lua) }
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
            instance.set_property("AudioContent", Variant::Content(if id.is_empty() { DomContent::none() } else { DomContent::from_uri(id) }));
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
            dispatch(&lua, &state)?;
            if samples.is_empty() { Ok(LuaValue::Nil) } else { Ok(LuaValue::Table(lua.create_sequence_from(samples)?)) }
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
        state.borrow_mut().register(id, clip).map_err(LuaError::runtime)?;
        Ok(duration)
    })?)?;
    let state = world.clone();
    module.set("registerPCM", lua.create_function(move |_, (id, rate, channels, buffer): (String, u32, usize, LuaBuffer)| {
        if buffer.len() % 4 != 0 || buffer.len() > pcm::MAX_FRAMES * 2 * 4 { return Err(LuaError::runtime("PCM buffer length is invalid")); }
        let bytes = buffer.to_vec();
        let samples = bytes.chunks_exact(4).map(|bytes| f32::from_le_bytes(bytes.try_into().unwrap())).collect();
        let clip = Arc::new(Clip::new(rate, channels, samples).map_err(LuaError::runtime)?);
        let duration = clip.duration();
        state.borrow_mut().register(id, clip).map_err(LuaError::runtime)?;
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
            if let Some(value) = options.get::<Option<LuaAnyUserData>>("source")? {
                let instance = *value.borrow::<Instance>()?;
                if !graph::CLASSES.contains(&instance.get_class_name()) { return Err(LuaError::runtime("capture source must be a supported audio node")); }
                source = Some(key(&instance));
            }
        }
        finite(max_seconds, "maxSeconds")?;
        if !(0.0..=600.0).contains(&max_seconds) { return Err(LuaError::runtime("capture limit must be between 0 and 600 seconds")); }
        let mut state = state.borrow_mut();
        if state.capture.is_some() { return Err(LuaError::runtime("an audio capture is already active")); }
        let start = state.clock;
        state.capture = Some(Capture { source, samples: Vec::new(), max_frames: (max_seconds * SAMPLE_RATE as f64).round() as usize, start });
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
        let unsupported = ["AudioEmitter", "AudioListener", "AudioReverb", "AudioEcho", "AudioFilter", "AudioEqualizer", "AudioCompressor", "AudioPitchShifter", "AudioDeviceInput", "AudioRecorder", "AudioTextToSpeech", "AudioSpeechToText", "multichannel output", "replication"];
        result.set("unsupported", lua.create_sequence_from(unsupported)?)?;
        Ok(result)
    })?)?;
    Ok(module)
}
