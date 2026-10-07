use std::{cell::RefCell, collections::{BTreeMap, HashMap}, rc::Rc};
use glam::{Mat4, Quat};
use lune_roblox::{datatypes::types::CFrame, instance::{Instance, registry::InstanceRegistry}};
use mlua::prelude::*;
use rbx_dom_weak::types::{CFrame as DomCFrame, Variant};

#[derive(Clone, Copy)]
struct Keyframe {
    time: f32,
    transform: Mat4,
    weight: f32,
}

struct Clip {
    channels: BTreeMap<(Option<String>, String), Vec<Keyframe>>,
    length: f32,
}

struct TrackData {
    animator: Instance,
    rig: Instance,
    clip: Clip,
    playing: bool,
    fading_out: bool,
    destroyed: bool,
    looped: bool,
    speed: f32,
    weight: f32,
    target_weight: f32,
    fade_start: f32,
    fade_elapsed: f32,
    fade_duration: f32,
    time: f32,
}

impl TrackData {
    fn fade_to(&mut self, target: f32, duration: f32) {
        self.fade_start = self.weight;
        self.target_weight = target;
        self.fade_elapsed = 0.0;
        self.fade_duration = duration;
        if duration == 0.0 { self.weight = target; }
    }

    fn advance(&mut self, dt: f32) {
        if self.playing {
            self.time += dt * self.speed;
            if self.clip.length > 0.0 && (self.time >= self.clip.length || self.time < 0.0) {
                if self.looped { self.time = self.time.rem_euclid(self.clip.length); }
                else {
                    self.time = self.time.clamp(0.0, self.clip.length);
                    self.playing = false;
                    self.fading_out = true;
                    self.fade_to(0.0, 0.1);
                }
            }
        }
        if self.fade_duration > 0.0 {
            self.fade_elapsed = (self.fade_elapsed + dt).min(self.fade_duration);
            let alpha = self.fade_elapsed / self.fade_duration;
            self.weight = self.fade_start + (self.target_weight - self.fade_start) * alpha;
        }
        if self.fading_out && self.weight <= f32::EPSILON { self.fading_out = false; }
    }
}

#[derive(Clone)]
struct AnimationTrack { data: Rc<RefCell<TrackData>> }

#[derive(Default)]
pub struct AnimationSystem {
    tracks: Vec<Rc<RefCell<TrackData>>>,
    previous: HashMap<String, Mat4>,
}

impl AnimationSystem {
    pub fn install(lua: &Lua, system: Rc<RefCell<Self>>) -> LuaResult<()> {
        let loader = system.clone();
        let load = lua.create_function(move |_, (animator, source): (LuaUserDataRef<Instance>, LuaUserDataRef<Instance>)| {
            let owner = animator.get_parent().ok_or_else(|| LuaError::runtime("Animator requires a Humanoid or AnimationController parent"))?;
            if !matches!(owner.get_class_name(), "Humanoid" | "AnimationController") {
                return Err(LuaError::runtime("Animator requires a Humanoid or AnimationController parent"));
            }
            let rig = owner.get_parent().filter(|instance| instance.is_a("Model"))
                .ok_or_else(|| LuaError::runtime("Animator controller must belong to a Model"))?;
            let sequence = if source.get_class_name() == "KeyframeSequence" { Some(*source) } else {
                source.get_descendants_preorder().into_iter().find(|instance| instance.get_class_name() == "KeyframeSequence")
            }.ok_or_else(|| LuaError::runtime("Yune currently loads local KeyframeSequence data; AnimationId asset loading is not implemented"))?;
            let data = Rc::new(RefCell::new(TrackData {
                animator: *animator, rig, clip: compile_clip(sequence)?, playing: false,
                fading_out: false, destroyed: false, looped: bool_prop(&sequence, "Loop", false),
                speed: 1.0, weight: 0.0, target_weight: 0.0, fade_start: 0.0,
                fade_elapsed: 0.0, fade_duration: 0.0, time: 0.0,
            }));
            loader.borrow_mut().tracks.push(data.clone());
            Ok(AnimationTrack { data })
        })?;
        InstanceRegistry::insert_method(lua, "Animator", "LoadAnimation", load).map_err(LuaError::external)?;
        let playing = lua.create_function(move |lua, animator: LuaUserDataRef<Instance>| {
            let output = lua.create_table()?;
            let animator_key = instance_key(&animator);
            for track in &system.borrow().tracks {
                let data = track.borrow();
                if !data.destroyed && instance_key(&data.animator) == animator_key && (data.playing || data.fading_out) {
                    output.push(AnimationTrack { data: track.clone() })?;
                }
            }
            Ok(output)
        })?;
        InstanceRegistry::insert_method(lua, "Animator", "GetPlayingAnimationTracks", playing).map_err(LuaError::external)
    }

    pub fn step(&mut self, workspace: Instance, dt: f64) {
        if !dt.is_finite() || dt < 0.0 { return; }
        let live = workspace.get_descendants_preorder().into_iter().map(|instance| (instance_key(&instance), instance)).collect::<HashMap<_, _>>();
        for (key, expected) in self.previous.drain() {
            if let Some(motor) = live.get(&key) {
                if cframe_prop(motor, "Transform").abs_diff_eq(expected, 1e-6) {
                    motor.set_property("Transform", Variant::CFrame(DomCFrame::from(CFrame(Mat4::IDENTITY))));
                }
            }
        }
        self.tracks.retain(|track| {
            let data = track.borrow();
            !data.destroyed && live.contains_key(&instance_key(&data.animator)) && live.contains_key(&instance_key(&data.rig))
                && (data.playing || data.fading_out || Rc::strong_count(track) > 1)
        });
        let mut mixed: BTreeMap<String, (Instance, Mat4, f32)> = BTreeMap::new();
        for track in &self.tracks {
            let mut data = track.borrow_mut();
            if !data.playing && !data.fading_out { continue; }
            data.advance(dt as f32);
            if data.weight <= f32::EPSILON { continue; }
            for motor in data.rig.get_descendants_preorder().into_iter().filter(|instance| instance.get_class_name() == "Motor6D") {
                if !bool_prop(&motor, "Enabled", true) { continue; }
                let (Some(part0), Some(part1)) = (ref_prop(&motor, "Part0"), ref_prop(&motor, "Part1")) else { continue };
                let parent = part0.get_name();
                let child = part1.get_name();
                let keys = data.clip.channels.get(&(Some(parent), child.clone()))
                    .or_else(|| data.clip.channels.get(&(None, child)));
                let Some(keys) = keys else { continue };
                let sample = sample_channel(keys, data.time);
                let weight = (sample.weight * data.weight).clamp(0.0, 1.0);
                if weight <= f32::EPSILON { continue; }
                mixed.entry(instance_key(&motor)).and_modify(|(_, transform, total)| {
                    *transform = lerp_cframe(*transform, sample.transform, weight / (*total + weight));
                    *total += weight;
                }).or_insert((motor, sample.transform, weight));
            }
        }
        for (key, (motor, transform, total)) in mixed {
            let transform = lerp_cframe(Mat4::IDENTITY, transform, total.min(1.0));
            motor.set_property("Transform", Variant::CFrame(DomCFrame::from(CFrame(transform))));
            self.previous.insert(key, transform);
        }
    }
}

impl LuaUserData for AnimationTrack {
    fn add_fields<F: LuaUserDataFields<Self>>(fields: &mut F) {
        fields.add_field_method_get("IsPlaying", |_, this| Ok(this.data.borrow().playing));
        fields.add_field_method_get("Length", |_, this| Ok(this.data.borrow().clip.length));
        fields.add_field_method_get("Looped", |_, this| Ok(this.data.borrow().looped));
        fields.add_field_method_set("Looped", |_, this, value: bool| { this.data.borrow_mut().looped = value; Ok(()) });
        fields.add_field_method_get("TimePosition", |_, this| Ok(this.data.borrow().time));
        fields.add_field_method_set("TimePosition", |_, this, value: f32| {
            finite(value, "TimePosition")?;
            let mut data = this.data.borrow_mut();
            data.time = value.clamp(0.0, data.clip.length);
            Ok(())
        });
        fields.add_field_method_get("Speed", |_, this| Ok(this.data.borrow().speed));
        fields.add_field_method_get("WeightCurrent", |_, this| Ok(this.data.borrow().weight));
        fields.add_field_method_get("WeightTarget", |_, this| Ok(this.data.borrow().target_weight));
    }
    fn add_methods<M: LuaUserDataMethods<Self>>(methods: &mut M) {
        methods.add_method("Play", |_, this, (fade, weight, speed): (Option<f32>, Option<f32>, Option<f32>)| {
            let fade = nonnegative(fade.unwrap_or(0.1), "fadeTime")?;
            let weight = nonnegative(weight.unwrap_or(1.0), "weight")?;
            let speed = finite(speed.unwrap_or(1.0), "speed")?;
            let mut data = this.data.borrow_mut();
            if data.destroyed { return Err(LuaError::runtime("AnimationTrack has been destroyed")); }
            data.time = if speed < 0.0 { data.clip.length } else { 0.0 };
            data.playing = true;
            data.fading_out = false;
            data.speed = speed;
            data.weight = 0.0;
            data.fade_to(weight, fade);
            Ok(())
        });
        methods.add_method("Stop", |_, this, fade: Option<f32>| {
            let fade = nonnegative(fade.unwrap_or(0.1), "fadeTime")?;
            let mut data = this.data.borrow_mut();
            if data.playing {
                data.playing = false;
                data.fading_out = true;
                data.fade_to(0.0, fade);
            }
            Ok(())
        });
        methods.add_method("AdjustSpeed", |_, this, speed: Option<f32>| {
            this.data.borrow_mut().speed = finite(speed.unwrap_or(1.0), "speed")?; Ok(())
        });
        methods.add_method("AdjustWeight", |_, this, (weight, fade): (Option<f32>, Option<f32>)| {
            let weight = nonnegative(weight.unwrap_or(1.0), "weight")?;
            let fade = nonnegative(fade.unwrap_or(0.1), "fadeTime")?;
            let mut data = this.data.borrow_mut();
            if data.playing { data.fade_to(weight, fade); }
            Ok(())
        });
        methods.add_method("Destroy", |_, this, ()| {
            let mut data = this.data.borrow_mut();
            data.destroyed = true;
            data.playing = false;
            data.fading_out = false;
            data.weight = 0.0;
            Ok(())
        });
    }
}

fn compile_clip(sequence: Instance) -> LuaResult<Clip> {
    let mut clip = Clip { channels: BTreeMap::new(), length: 0.0 };
    for key in sequence.get_children().into_iter().filter(|instance| instance.get_class_name() == "Keyframe") {
        let time = nonnegative(f32_prop(&key, "Time", 0.0), "Keyframe.Time")?;
        clip.length = clip.length.max(time);
        for pose in key.get_descendants_preorder().into_iter().filter(|instance| instance.get_class_name() == "Pose") {
            let parent = pose.get_parent().filter(|instance| instance.get_class_name() == "Pose").map(|instance| instance.get_name());
            let transform = cframe_prop(&pose, "CFrame");
            if !transform.is_finite() || transform.determinant().abs() < 1e-8 {
                return Err(LuaError::runtime("Pose CFrame must be a finite invertible transform"));
            }
            let weight = nonnegative(f32_prop(&pose, "Weight", 1.0), "Pose.Weight")?;
            clip.channels.entry((parent, pose.get_name())).or_default().push(Keyframe { time, transform, weight });
        }
    }
    for keys in clip.channels.values_mut() { keys.sort_by(|a, b| a.time.total_cmp(&b.time)); }
    Ok(clip)
}

fn sample_channel(keys: &[Keyframe], time: f32) -> Keyframe {
    let next = keys.partition_point(|key| key.time <= time);
    if next == 0 { return keys[0]; }
    if next == keys.len() { return keys[keys.len() - 1]; }
    let a = keys[next - 1];
    let b = keys[next];
    let alpha = ((time - a.time) / (b.time - a.time).max(f32::EPSILON)).clamp(0.0, 1.0);
    Keyframe { time, transform: lerp_cframe(a.transform, b.transform, alpha), weight: a.weight + (b.weight - a.weight) * alpha }
}
fn lerp_cframe(a: Mat4, b: Mat4, alpha: f32) -> Mat4 {
    let translation = a.w_axis.truncate().lerp(b.w_axis.truncate(), alpha);
    let rotation = Quat::from_mat4(&a).normalize().slerp(Quat::from_mat4(&b).normalize(), alpha);
    Mat4::from_rotation_translation(rotation, translation)
}
fn finite(value: f32, name: &str) -> LuaResult<f32> {
    if value.is_finite() { Ok(value) } else { Err(LuaError::runtime(format!("{name} must be finite"))) }
}
fn nonnegative(value: f32, name: &str) -> LuaResult<f32> {
    let value = finite(value, name)?;
    if value >= 0.0 { Ok(value) } else { Err(LuaError::runtime(format!("{name} must not be negative"))) }
}
fn f32_prop(instance: &Instance, name: &str, default: f32) -> f32 {
    match instance.get_property(name) { Some(Variant::Float32(value)) => value, Some(Variant::Float64(value)) => value as f32, _ => default }
}
fn bool_prop(instance: &Instance, name: &str, default: bool) -> bool {
    match instance.get_property(name) { Some(Variant::Bool(value)) => value, _ => default }
}
fn cframe_prop(instance: &Instance, name: &str) -> Mat4 {
    match instance.get_property(name) { Some(Variant::CFrame(value)) => CFrame::from(value).0, _ => Mat4::IDENTITY }
}
fn ref_prop(instance: &Instance, name: &str) -> Option<Instance> {
    match instance.get_property(name) { Some(Variant::Ref(value)) => Instance::new_opt(instance.dom_id, value), _ => None }
}
fn instance_key(instance: &Instance) -> String { format!("{}:{}", instance.dom_id, instance.dom_ref) }
