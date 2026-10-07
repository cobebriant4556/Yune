use glam::{Mat4, Vec2, Vec3};
use lune_roblox::{datatypes::types::CFrame, instance::Instance};
use rbx_dom_weak::types::{Color3 as DomColor3, UDim2 as DomUDim2, Variant};

pub fn instance_key(instance: &Instance) -> String {
    format!("{}:{}", instance.dom_id, instance.dom_ref)
}

pub fn bool_prop(instance: &Instance, name: &str, default: bool) -> bool {
    match instance.get_property(name) {
        Some(Variant::Bool(value)) => value,
        _ => default,
    }
}

pub fn f32_prop(instance: &Instance, name: &str, default: f32) -> f32 {
    match instance.get_property(name) {
        Some(Variant::Float32(value)) => value,
        Some(Variant::Float64(value)) => value as f32,
        Some(Variant::Int32(value)) => value as f32,
        Some(Variant::Int64(value)) => value as f32,
        _ => default,
    }
}

pub fn i32_prop(instance: &Instance, name: &str, default: i32) -> i32 {
    match instance.get_property(name) {
        Some(Variant::Int32(value)) => value,
        Some(Variant::Int64(value)) => value as i32,
        Some(Variant::Float32(value)) => value as i32,
        Some(Variant::Float64(value)) => value as i32,
        _ => default,
    }
}

pub fn string_prop(instance: &Instance, name: &str, default: &str) -> String {
    match instance.get_property(name) {
        Some(Variant::String(value)) => value,
        _ => default.to_string(),
    }
}

pub fn vec2_prop(instance: &Instance, name: &str, default: Vec2) -> Vec2 {
    match instance.get_property(name) {
        Some(Variant::Vector2(value)) => Vec2::new(value.x, value.y),
        _ => default,
    }
}

pub fn vec3_prop(instance: &Instance, name: &str, default: Vec3) -> Vec3 {
    match instance.get_property(name) {
        Some(Variant::Vector3(value)) => Vec3::new(value.x, value.y, value.z),
        _ => default,
    }
}

pub fn cframe_prop(instance: &Instance, name: &str, default: Mat4) -> Mat4 {
    match instance.get_property(name) {
        Some(Variant::CFrame(value)) => CFrame::from(value).0,
        _ => default,
    }
}

pub fn color_prop(instance: &Instance, name: &str, default: Vec3) -> Vec3 {
    match instance.get_property(name) {
        Some(Variant::Color3(value)) => Vec3::new(value.r, value.g, value.b),
        Some(Variant::Color3uint8(value)) => {
            let value = DomColor3::from(value);
            Vec3::new(value.r, value.g, value.b)
        }
        _ => default,
    }
}

pub fn udim2_prop(instance: &Instance, name: &str, default: DomUDim2) -> DomUDim2 {
    match instance.get_property(name) {
        Some(Variant::UDim2(value)) => value,
        _ => default,
    }
}

pub fn ref_prop(instance: &Instance, name: &str) -> Option<Instance> {
    match instance.get_property(name) {
        Some(Variant::Ref(referent)) => Instance::new_opt(instance.dom_id, referent),
        _ => None,
    }
}

pub fn color_to_rgba(color: Vec3, transparency: f32) -> [u8; 4] {
    [
        (color.x.clamp(0.0, 1.0) * 255.0).round() as u8,
        (color.y.clamp(0.0, 1.0) * 255.0).round() as u8,
        (color.z.clamp(0.0, 1.0) * 255.0).round() as u8,
        ((1.0 - transparency.clamp(0.0, 1.0)) * 255.0).round() as u8,
    ]
}
