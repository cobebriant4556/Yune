use std::{
    cell::RefCell,
    collections::HashMap,
    rc::{Rc, Weak},
};

use glam::{Mat4, Quat, Vec3};
use lune_roblox::{
    datatypes::types::CFrame,
    instance::{Instance, registry::InstanceRegistry},
};
use mlua::prelude::*;
use rbx_dom_weak::types::{CFrame as DomCFrame, Variant};

#[derive(Default)]
pub struct AnimationSystem {
    tracks: Vec<Weak<RefCell<TrackData>>>,
}

struct TrackData {
    sequence: Instance,
    playing: bool,
    looped: bool,
    speed: f32,
    weight: f32,
    time: f32,
    length: f32,
}

#[derive(Clone)]
struct AnimationTrack {
    data: Rc<RefCell<TrackData>>,
}

#[derive(Clone, Copy)]
struct Sample {
    transform: Mat4,
    weight: f32,
}

impl AnimationSystem {
    pub fn install(lua: &Lua, system: Rc<RefCell<Self>>) -> LuaResult<()> {
        let load_system = system;
        let load_animation = lua.create_function(
            move |_, (_animator, source): (LuaUserDataRef<Instance>, LuaUserDataRef<Instance>)| {
                let sequence = resolve_sequence(*source).ok_or_else(|| {
                    LuaError::runtime(
                        "Yune Animator:LoadAnimation currently requires a KeyframeSequence or an Animation containing one",
                    )
                })?;
                let length = sequence_length(sequence);
                let looped = bool_prop(&sequence, "Loop", false);
                let data = Rc::new(RefCell::new(TrackData {
                    sequence,
                    playing: false,
                    looped,
                    speed: 1.0,
                    weight: 1.0,
                    time: 0.0,
                    length,
                }));
                load_system.borrow_mut().tracks.push(Rc::downgrade(&data));
                Ok(AnimationTrack { data })
            },
        )?;
        InstanceRegistry::insert_method(lua, "Animator", "LoadAnimation", load_animation)
            .map_err(LuaError::external)
    }

    pub fn step(&mut self, workspace: Instance, dt: f64) {
        self.tracks.retain(|track| track.strong_count() > 0);

        let motors = workspace
            .get_descendants_preorder()
            .into_iter()
            .filter(|instance| instance.get_class_name() == "Motor6D")
            .filter_map(|motor| {
                let part1 = ref_prop(&motor, "Part1")?;
                Some((part1.get_name(), motor))
            })
            .collect::<HashMap<_, _>>();

        let mut mixed: HashMap<String, (Sample, f32)> = HashMap::new();

        for weak in &self.tracks {
            let Some(track) = weak.upgrade() else {
                continue;
            };
            let mut track = track.borrow_mut();
            if !track.playing {
                continue;
            }

            track.time += dt as f32 * track.speed;
            if track.length > 0.0 && track.time > track.length {
                if track.looped {
                    track.time %= track.length;
                } else {
                    track.time = track.length;
                    track.playing = false;
                }
            } else if track.time < 0.0 {
                if track.looped && track.length > 0.0 {
                    track.time = track.length + track.time % track.length;
                } else {
                    track.time = 0.0;
                    track.playing = false;
                }
            }

            for (name, sample) in sample_sequence(track.sequence, track.time) {
                let weight = (sample.weight * track.weight).clamp(0.0, 1.0);
                if weight <= f32::EPSILON {
                    continue;
                }
                mixed
                    .entry(name)
                    .and_modify(|(current, total)| {
                        let alpha = weight / (*total + weight);
                        current.transform =
                            lerp_cframe(current.transform, sample.transform, alpha);
                        current.weight = (*total + weight).clamp(0.0, 1.0);
                        *total += weight;
                    })
                    .or_insert((
                        Sample {
                            transform: sample.transform,
                            weight,
                        },
                        weight,
                    ));
            }
        }

        for (part_name, (sample, _)) in mixed {
            let Some(motor) = motors.get(&part_name).copied() else {
                continue;
            };
            let blended = lerp_cframe(Mat4::IDENTITY, sample.transform, sample.weight);
            motor.set_property(
                "Transform",
                Variant::CFrame(DomCFrame::from(CFrame(blended))),
            );
        }
    }
}

impl LuaUserData for AnimationTrack {
    fn add_fields<F: LuaUserDataFields<Self>>(fields: &mut F) {
        fields.add_field_method_get("IsPlaying", |_, this| Ok(this.data.borrow().playing));
        fields.add_field_method_get("Length", |_, this| Ok(this.data.borrow().length));
        fields.add_field_method_get("Looped", |_, this| Ok(this.data.borrow().looped));
        fields.add_field_method_set("Looped", |_, this, value: bool| {
            this.data.borrow_mut().looped = value;
            Ok(())
        });
        fields.add_field_method_get("TimePosition", |_, this| Ok(this.data.borrow().time));
        fields.add_field_method_set("TimePosition", |_, this, value: f32| {
            let length = this.data.borrow().length;
            this.data.borrow_mut().time = value.clamp(0.0, length.max(0.0));
            Ok(())
        });
        fields.add_field_method_get("Speed", |_, this| Ok(this.data.borrow().speed));
        fields.add_field_method_get("WeightCurrent", |_, this| Ok(this.data.borrow().weight));
    }

    fn add_methods<M: LuaUserDataMethods<Self>>(methods: &mut M) {
        methods.add_method(
            "Play",
            |_, this, (_fade_time, weight, speed): (Option<f32>, Option<f32>, Option<f32>)| {
                let mut data = this.data.borrow_mut();
                data.weight = weight.unwrap_or(1.0).max(0.0);
                data.speed = speed.unwrap_or(1.0);
                if data.time >= data.length && data.speed >= 0.0 {
                    data.time = 0.0;
                }
                data.playing = true;
                Ok(())
            },
        );
        methods.add_method("Stop", |_, this, _fade_time: Option<f32>| {
            this.data.borrow_mut().playing = false;
            Ok(())
        });
        methods.add_method("AdjustSpeed", |_, this, speed: Option<f32>| {
            this.data.borrow_mut().speed = speed.unwrap_or(1.0);
            Ok(())
        });
        methods.add_method(
            "AdjustWeight",
            |_, this, (weight, _fade_time): (f32, Option<f32>)| {
                this.data.borrow_mut().weight = weight.max(0.0);
                Ok(())
            },
        );
    }
}

fn resolve_sequence(source: Instance) -> Option<Instance> {
    if source.get_class_name() == "KeyframeSequence" {
        return Some(source);
    }

    source
        .get_descendants_preorder()
        .into_iter()
        .find(|instance| instance.get_class_name() == "KeyframeSequence")
}

fn sequence_length(sequence: Instance) -> f32 {
    sequence
        .get_children()
        .into_iter()
        .filter(|instance| instance.get_class_name() == "Keyframe")
        .map(|keyframe| f32_prop(&keyframe, "Time", 0.0))
        .fold(0.0, f32::max)
}

fn sample_sequence(sequence: Instance, time: f32) -> HashMap<String, Sample> {
    let mut samples: HashMap<String, Vec<(f32, Mat4, f32)>> = HashMap::new();

    for keyframe in sequence
        .get_children()
        .into_iter()
        .filter(|instance| instance.get_class_name() == "Keyframe")
    {
        let key_time = f32_prop(&keyframe, "Time", 0.0);
        for pose in keyframe
            .get_descendants_preorder()
            .into_iter()
            .filter(|instance| instance.get_class_name() == "Pose")
        {
            samples.entry(pose.get_name()).or_default().push((
                key_time,
                cframe_prop(&pose, "CFrame", Mat4::IDENTITY),
                f32_prop(&pose, "Weight", 1.0),
            ));
        }
    }

    let mut output = HashMap::new();
    for (name, mut keys) in samples {
        keys.sort_by(|a, b| a.0.total_cmp(&b.0));
        let previous = keys
            .iter()
            .rev()
            .find(|entry| entry.0 <= time)
            .copied()
            .unwrap_or(keys[0]);
        let next = keys
            .iter()
            .find(|entry| entry.0 >= time)
            .copied()
            .unwrap_or(*keys.last().expect("pose sample has at least one key"));

        let alpha = if (next.0 - previous.0).abs() <= f32::EPSILON {
            0.0
        } else {
            ((time - previous.0) / (next.0 - previous.0)).clamp(0.0, 1.0)
        };
        output.insert(
            name,
            Sample {
                transform: lerp_cframe(previous.1, next.1, alpha),
                weight: previous.2 + (next.2 - previous.2) * alpha,
            },
        );
    }

    output
}

fn lerp_cframe(a: Mat4, b: Mat4, alpha: f32) -> Mat4 {
    let alpha = alpha.clamp(0.0, 1.0);
    let translation = a.w_axis.truncate().lerp(b.w_axis.truncate(), alpha);
    let rotation_a = Quat::from_mat4(&a).normalize();
    let rotation_b = Quat::from_mat4(&b).normalize();
    Mat4::from_rotation_translation(rotation_a.slerp(rotation_b, alpha), translation)
}

fn f32_prop(instance: &Instance, name: &str, default: f32) -> f32 {
    match instance.get_property(name) {
        Some(Variant::Float32(value)) => value,
        Some(Variant::Float64(value)) => value as f32,
        Some(Variant::Int32(value)) => value as f32,
        Some(Variant::Int64(value)) => value as f32,
        _ => default,
    }
}

fn bool_prop(instance: &Instance, name: &str, default: bool) -> bool {
    match instance.get_property(name) {
        Some(Variant::Bool(value)) => value,
        _ => default,
    }
}

fn cframe_prop(instance: &Instance, name: &str, default: Mat4) -> Mat4 {
    match instance.get_property(name) {
        Some(Variant::CFrame(value)) => CFrame::from(value).0,
        _ => default,
    }
}

fn ref_prop(instance: &Instance, name: &str) -> Option<Instance> {
    match instance.get_property(name) {
        Some(Variant::Ref(referent)) => Instance::new_opt(instance.dom_id, referent),
        _ => None,
    }
}
