use super::*;
use mlua::ObjectLike;
use lune_roblox::datatypes::types::{EnumItem, NumberRange};
use rbx_dom_weak::types::{EnumItem as DomEnumItem, NumberRange as DomRange};

pub fn install(lua: &Lua, world: Rc<RefCell<AudioWorld>>, module: &LuaTable) -> LuaResult<()> {
    for &(class, name, default, min, max) in effects::PARAMETERS {
        let state = world.clone();
        let getter = lua.create_function(move |_, instance: LuaUserDataRef<Instance>| Ok(number(&instance, name, default)))?;
        let setter = lua.create_function(move |lua, (instance, value): (LuaUserDataRef<Instance>, f64)| {
            let value = finite(value, name)?.clamp(min, max);
            if number(&instance, name, default) != value {
                instance.set_property(name, Variant::Float64(value));
                state.borrow_mut().changed(*instance, name);
                dispatch(lua, &state)?;
            }
            Ok(())
        })?;
        property(lua, class, name, getter, Some(setter))?;
    }
    for &class in effects::CLASSES { install_bool(lua, world.clone(), class, "Bypass", false)?; }
    for (class, name, default, min, max) in [
        ("AudioEqualizer", "MidRange", (400.0, 4000.0), 200.0, 20000.0),
        ("AudioGate", "Threshold", (-36.0, -24.0), -80.0, 30.0),
        ("AudioEmitter", "DistanceAttenuationBounds", (4.0, 10000.0), 0.0, f32::MAX as f64),
    ] {
        let state = world.clone();
        let getter = lua.create_function(move |_, instance: LuaUserDataRef<Instance>| {
            let (a, b) = effects::range(&instance, name, default);
            Ok(NumberRange::from(DomRange { min: a as f32, max: b as f32 }))
        })?;
        let setter = lua.create_function(move |lua, (instance, value): (LuaUserDataRef<Instance>, LuaUserDataRef<NumberRange>)| {
            let value: DomRange = (*value).into();
            let a = finite(f64::from(value.min), name)?.clamp(min, max);
            let b = finite(f64::from(value.max), name)?.clamp(min, max);
            if a > b { return Err(LuaError::runtime(format!("{name} must have ordered endpoints"))); }
            instance.set_property(name, Variant::NumberRange(DomRange { min: a as f32, max: b as f32 }));
            state.borrow_mut().changed(*instance, name);
            dispatch(lua, &state)
        })?;
        property(lua, class, name, getter, Some(setter))?;
    }
    for (class, name, ty, default) in [
        ("AudioFilter", "FilterType", "AudioFilterType", 0),
        ("AudioPitchShifter", "WindowSize", "AudioWindowSize", 1),
        ("AudioEmitter", "PositionType", "EmitterPositionType", 0),
        ("AudioListener", "PositionType", "ListenerPositionType", 0),
        ("AudioEmitter", "DistanceAttenuationMode", "DistanceAttenuationMode", 0),
    ] {
        let state = world.clone();
        let getter = lua.create_function(move |lua, instance: LuaUserDataRef<Instance>| {
            let value = enum_value(&instance, name, default);
            let root: LuaAnyUserData = lua.globals().get("Enum")?;
            let enumeration: LuaAnyUserData = root.get(ty)?;
            let items: LuaTable = enumeration.call_method("GetEnumItems", ())?;
            for item in items.sequence_values::<LuaAnyUserData>() {
                let item = item?;
                if item.get::<u32>("Value")? == value { return Ok(item); }
            }
            Err(LuaError::runtime(format!("{name} contains an unsupported {ty} value {value}")))
        })?;
        let setter = lua.create_function(move |lua, (instance, value): (LuaUserDataRef<Instance>, LuaUserDataRef<EnumItem>)| {
            let value: DomEnumItem = (*value).clone().into();
            if value.ty != ty { return Err(LuaError::runtime(format!("{name} requires Enum.{ty}"))); }
            instance.set_property(name, Variant::EnumItem(value));
            state.borrow_mut().changed(*instance, name);
            dispatch(lua, &state)
        })?;
        property(lua, class, name, getter, Some(setter))?;
    }
    method(lua, "AudioFilter", "GetGainAt", lua.create_function(|_, (instance, frequency): (LuaUserDataRef<Instance>, f64)| {
        let frequency = finite(frequency, "frequency")?;
        if !(0.0..=SAMPLE_RATE as f64 / 2.0).contains(&frequency) { return Err(LuaError::runtime("frequency must be between zero and the output Nyquist frequency")); }
        Ok(effects::filter_response(&instance, frequency))
    })?)?;
    for class in ["AudioEmitter", "AudioListener"] {
        install_bool(lua, world.clone(), class, "AcousticSimulationEnabled", false)?;
        let state = world.clone();
        let getter = lua.create_function(|_, instance: LuaUserDataRef<Instance>| Ok(text(&instance, "AudioInteractionGroup", "")))?;
        let setter = lua.create_function(move |lua, (instance, value): (LuaUserDataRef<Instance>, String)| {
            instance.set_property("AudioInteractionGroup", Variant::String(value));
            state.borrow_mut().changed(*instance, "AudioInteractionGroup");
            dispatch(lua, &state)
        })?;
        property(lua, class, "AudioInteractionGroup", getter, Some(setter))?;
        let state = world.clone();
        let getter = lua.create_function(|lua, instance: LuaUserDataRef<Instance>| {
            reference(&instance, "PositionInstance").map(|value| instance_to_lua(lua, value)).transpose()
        })?;
        let setter = lua.create_function(move |lua, (instance, value): (LuaUserDataRef<Instance>, Option<LuaUserDataRef<Instance>>)| {
            instance.set_property("PositionInstance", Variant::Ref(value.map_or_else(rbx_dom_weak::types::Ref::none, |value| value.dom_ref)));
            state.borrow_mut().changed(*instance, "PositionInstance");
            dispatch(lua, &state)
        })?;
        property(lua, class, "PositionInstance", getter, Some(setter))?;
        for (kind, angle) in [("AngleAttenuation", true), ("DistanceAttenuation", false)] {
            let state = world.clone();
            method(lua, class, &format!("Set{kind}"), lua.create_function(move |lua, (instance, curve): (LuaUserDataRef<Instance>, Option<LuaTable>)| {
                let mut points = Vec::new();
                if let Some(curve) = curve {
                    for pair in curve.pairs::<LuaValue, LuaValue>() {
                        let (position, volume) = pair?;
                        let position = numeric(position, "attenuation curve key")?;
                        let volume = numeric(volume, "attenuation curve volume")?;
                        if position < 0.0 || (angle && position > 180.0) || !(0.0..=1.0).contains(&volume) {
                            return Err(LuaError::runtime("attenuation keys must be nonnegative (angles at most 180); volumes must be between 0 and 1"));
                        }
                        if points.len() == 400 { return Err(LuaError::runtime("attenuation curves support at most 400 points")); }
                        points.push((position, volume));
                    }
                }
                points.sort_by(|a, b| a.0.total_cmp(&b.0));
                {
                    let mut state = state.borrow_mut();
                    let entry = state.curves.entry(key(&instance)).or_insert_with(|| (*instance, spatial::Curves::default()));
                    entry.0 = *instance;
                    if angle { entry.1.angle = points; } else { entry.1.distance = points; }
                    state.changed(*instance, kind);
                }
                dispatch(lua, &state)
            })?)?;
            let state = world.clone();
            method(lua, class, &format!("Get{kind}"), lua.create_function(move |lua, instance: LuaUserDataRef<Instance>| {
                let table = lua.create_table()?;
                let mut state = state.borrow_mut();
                if let Some((owner, curves)) = state.curves.get_mut(&key(&instance)) {
                    *owner = *instance;
                    for &(position, volume) in if angle { &curves.angle } else { &curves.distance } { table.set(position, volume)?; }
                }
                Ok(table)
            })?)?;
        }
        let state = world.clone();
        let other_class = if class == "AudioEmitter" { "AudioListener" } else { "AudioEmitter" };
        let name = if class == "AudioEmitter" { "GetInteractingListeners" } else { "GetInteractingEmitters" };
        method(lua, class, name, lua.create_function(move |lua, instance: LuaUserDataRef<Instance>| {
            let state = state.borrow();
            let nodes = state.game.get_descendants_preorder().into_iter()
                .filter(|other| other.get_class_name() == other_class && spatial::interacts(&instance, other))
                .map(|other| instance_to_lua(lua, other)).collect::<LuaResult<Vec<_>>>()?;
            lua.create_sequence_from(nodes)
        })?)?;
        let state = world.clone();
        method(lua, class, "GetAudibilityFor", lua.create_function(move |_, (instance, other): (LuaUserDataRef<Instance>, LuaUserDataRef<Instance>)| {
            if other.get_class_name() != other_class { return Err(LuaError::runtime(format!("GetAudibilityFor requires an {other_class}"))); }
            let state = state.borrow();
            let (emitter, listener) = if class == "AudioEmitter" { (&*instance, &*other) } else { (&*other, &*instance) };
            Ok(spatial::attenuation(emitter, listener, &state.curves).0)
        })?)?;
    }
    module.set("getCapabilities", lua.create_function(|lua, (): ()| {
        let result = lua.create_table()?;
        for name in ["audioPlayer", "scheduledPlayback", "wireGraph", "audioFader", "spatialAudio", "sidechain", "effectStateAcrossBlocks"] { result.set(name, true)?; }
        for name in ["robloxDspParity", "acousticSimulation", "hrtf", "doppler", "serializedAttenuationCurves"] { result.set(name, false)?; }
        result.set("audioAnalyzer", "approximate Hann-window spectrum")?;
        result.set("spatialModel", "mono point sources with equal-power stereo panning; custom and preset attenuation")?;
        result.set("effectModel", "mixed: source-derived distortion, limiter and scalar stereo compressor; remaining effects are independent implementations")?;
        result.set("sourceDerivedEffects", lua.create_sequence_from(["AudioDistortion", "AudioLimiter", "AudioCompressor"])?)?;
        result.set("sourceDerivedPlayback", "float32 effective regions, deferred readiness, immediate action ID 0, natural-end cursor reset")?;
        result.set("supportedEffects", lua.create_sequence_from(effects::CLASSES.iter().copied())?)?;
        result.set("unsupported", lua.create_sequence_from(["AudioDeviceInput", "AudioRecorder", "AudioTextToSpeech", "AudioSpeechToText", "AudioChannelMixer", "AudioChannelSplitter", "HRTF", "Doppler", "automatic occlusion/diffraction/room acoustics", "multichannel output", "replication"])?)?;
        Ok(result)
    })?)?;
    module.set("getNodeLatency", lua.create_function(|lua, instance: LuaUserDataRef<Instance>| {
        let samples = if instance.get_class_name() == "AudioPitchShifter" && !boolean(&instance, "Bypass", false) {
            match enum_value(&instance, "WindowSize", 1) { 0 => 512, 2 => 2048, _ => 1024 }
        } else { 0 };
        let result = lua.create_table()?;
        result.set("samples", samples)?;
        result.set("seconds", samples as f64 / SAMPLE_RATE as f64)?;
        result.set("includesCreativeDelays", false)?;
        Ok(result)
    })?)?;
    let state = world.clone();
    module.set("resetEffects", lua.create_function(move |_, instance: Option<LuaUserDataRef<Instance>>| {
        let mut state = state.borrow_mut();
        if let Some(instance) = instance {
            if !effects::CLASSES.contains(&instance.get_class_name()) { return Err(LuaError::runtime("resetEffects requires a supported effect instance")); }
            state.effects.remove(&key(&instance));
        } else { state.effects.clear(); }
        Ok(())
    })?)?;
    let state = world.clone();
    module.set("getSpatialInfo", lua.create_function(move |lua, (emitter, listener): (LuaUserDataRef<Instance>, LuaUserDataRef<Instance>)| {
        if emitter.get_class_name() != "AudioEmitter" || listener.get_class_name() != "AudioListener" { return Err(LuaError::runtime("getSpatialInfo requires an AudioEmitter and AudioListener")); }
        let (gain, pan) = spatial::attenuation(&emitter, &listener, &state.borrow().curves);
        let channels = spatial::channel_gains(gain, pan);
        let result = lua.create_table()?;
        result.set("audibility", gain)?; result.set("pan", pan)?;
        result.set("leftGain", channels[0])?; result.set("rightGain", channels[1])?;
        result.set("calibratedAgainstRoblox", false)?;
        Ok(result)
    })?)?;
    Ok(())
}

fn numeric(value: LuaValue, name: &str) -> LuaResult<f64> {
    let value = match value { LuaValue::Integer(value) => value as f64, LuaValue::Number(value) => value, _ => return Err(LuaError::runtime(format!("{name} must be a number"))) };
    finite(value, name)
}
fn install_bool(lua: &Lua, world: Rc<RefCell<AudioWorld>>, class: &'static str, name: &'static str, default: bool) -> LuaResult<()> {
    let getter = lua.create_function(move |_, instance: LuaUserDataRef<Instance>| Ok(boolean(&instance, name, default)))?;
    let setter = lua.create_function(move |lua, (instance, value): (LuaUserDataRef<Instance>, bool)| {
        if boolean(&instance, name, default) != value {
            instance.set_property(name, Variant::Bool(value));
            world.borrow_mut().changed(*instance, name);
            dispatch(lua, &world)?;
        }
        Ok(())
    })?;
    property(lua, class, name, getter, Some(setter))
}
