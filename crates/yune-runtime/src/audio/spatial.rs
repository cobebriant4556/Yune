use std::collections::BTreeMap;
use glam::{Mat4, Vec3};
use lune_roblox::{datatypes::types::CFrame, instance::Instance};
use rbx_dom_weak::types::Variant;
use super::{effects, enum_value, key, reference, text};

#[derive(Clone, Default)]
pub struct Curves { pub distance: Vec<(f64, f64)>, pub angle: Vec<(f64, f64)> }
pub type CurveMap = BTreeMap<String, (Instance, Curves)>;

pub fn interpolate(curve: &[(f64, f64)], at: f64, default: f64) -> f64 {
    if curve.is_empty() { return default; }
    let next = curve.partition_point(|(position, _)| *position <= at);
    if next == 0 { return curve[0].1; }
    if next == curve.len() { return curve[next - 1].1; }
    let (a, x) = curve[next - 1];
    let (b, y) = curve[next];
    x + (y - x) * ((at - a) / (b - a))
}

fn cframe(node: &Instance, property: &str) -> Mat4 {
    match node.get_property(property) { Some(Variant::CFrame(value)) => CFrame::from(value).0, _ => Mat4::IDENTITY }
}
fn world_transform(node: Instance, depth: usize) -> Option<Mat4> {
    if depth > 64 { return None; }
    let transform = if node.is_a("Attachment") {
        let parent = world_transform(node.get_parent()?, depth + 1)?;
        let local = cframe(&node, "CFrame");
        parent * local * if node.is_a("Bone") { cframe(&node, "Transform") } else { Mat4::IDENTITY }
    } else if node.is_a("BasePart") || node.get_class_name() == "Camera" {
        cframe(&node, "CFrame")
    } else if node.is_a("Model") {
        if let Some(primary) = reference(&node, "PrimaryPart") {
            world_transform(primary, depth + 1)? * cframe(&primary, "PivotOffset")
        } else { cframe(&node, "WorldPivot") }
    } else { return None; };
    if transform.is_finite() && transform.determinant().abs() > 1e-8 { Some(transform) } else { None }
}
pub fn transform(node: &Instance) -> Option<Mat4> {
    let positioned = match enum_value(node, "PositionType", 0) {
        0 => node.get_parent(), 1 => reference(node, "PositionInstance"), _ => None,
    }?;
    world_transform(positioned, 0)
}
pub fn interacts(emitter: &Instance, listener: &Instance) -> bool {
    text(emitter, "AudioInteractionGroup", "") == text(listener, "AudioInteractionGroup", "")
}
fn angle(forward: Vec3, direction: Vec3) -> f64 {
    if direction.length_squared() < 1e-12 { return 0.0; }
    f64::from(forward.normalize_or_zero().dot(direction.normalize_or_zero()).clamp(-1.0, 1.0).acos().to_degrees())
}
pub fn preset(mode: u32, distance: f64, bounds: (f64, f64)) -> f64 {
    let min = bounds.0.max(0.0);
    let max = bounds.1.max(min);
    if distance >= max { return 0.0; }
    if distance <= min { return 1.0; }
    let linear = ((max - distance) / (max - min).max(1e-12)).clamp(0.0, 1.0);
    let inverse = (min / distance.max(1e-12)).clamp(0.0, 1.0);
    match mode { 1 => inverse.min(linear * linear), 2 => linear, 3 => linear * linear, 4 => inverse, _ => 0.0 }
}

pub fn attenuation(emitter: &Instance, listener: &Instance, curves: &CurveMap) -> (f64, f64) {
    if !interacts(emitter, listener) { return (0.0, 0.0); }
    let (Some(e), Some(l)) = (transform(emitter), transform(listener)) else { return (0.0, 0.0) };
    let direction = e.w_axis.truncate() - l.w_axis.truncate();
    let distance = f64::from(direction.length());
    if !distance.is_finite() { return (0.0, 0.0); }
    let empty = Curves::default();
    let ec = curves.get(&key(emitter)).map_or(&empty, |(_, curves)| curves);
    let lc = curves.get(&key(listener)).map_or(&empty, |(_, curves)| curves);
    let mode = enum_value(emitter, "DistanceAttenuationMode", 0);
    let emitter_distance = if mode == 0 {
        // This default is a Yune inverse-square profile, not calibrated Roblox DSP.
        interpolate(&ec.distance, distance, (4.0 / distance.max(4.0)).powi(2))
    } else { preset(mode, distance, effects::range(emitter, "DistanceAttenuationBounds", (4.0, 10000.0))) };
    let listener_distance = interpolate(&lc.distance, distance, 1.0);
    let emitter_angle = interpolate(&ec.angle, angle(-e.z_axis.truncate(), -direction), 1.0);
    let listener_angle = interpolate(&lc.angle, angle(-l.z_axis.truncate(), direction), 1.0);
    let gain = (emitter_distance * listener_distance * emitter_angle * listener_angle).clamp(0.0, 1.0);
    let pan = f64::from(l.x_axis.truncate().normalize_or_zero().dot(direction.normalize_or_zero())).clamp(-1.0, 1.0);
    (gain, pan)
}
pub fn channel_gains(gain: f64, pan: f64) -> [f32; 2] {
    [(gain * ((1.0 - pan.clamp(-1.0, 1.0)) * 0.5).sqrt()) as f32,
        (gain * ((1.0 + pan.clamp(-1.0, 1.0)) * 0.5).sqrt()) as f32]
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn curve_interpolation_clamps_endpoints() {
        let curve = [(2.0, 1.0), (10.0, 0.5), (20.0, 0.0)];
        assert_eq!(interpolate(&curve, 0.0, 0.0), 1.0);
        assert_eq!(interpolate(&curve, 6.0, 0.0), 0.75);
        assert_eq!(interpolate(&curve, 30.0, 1.0), 0.0);
        assert_eq!(interpolate(&[], 10.0, 0.4), 0.4);
    }
    #[test]
    fn panner_has_constant_power_and_correct_handedness() {
        assert_eq!(channel_gains(1.0, -1.0), [1.0, 0.0]);
        assert_eq!(channel_gains(1.0, 1.0), [0.0, 1.0]);
        for i in 0..101 {
            let gain = channel_gains(0.6, i as f64 / 50.0 - 1.0);
            assert!((gain[0] * gain[0] + gain[1] * gain[1] - 0.36).abs() < 1e-6);
        }
    }
    #[test]
    fn attenuation_presets_follow_public_formulas() {
        assert_eq!(preset(2, 6.0, (2.0, 10.0)), 0.5);
        assert_eq!(preset(3, 6.0, (2.0, 10.0)), 0.25);
        assert_eq!(preset(4, 4.0, (2.0, 10.0)), 0.5);
        assert_eq!(preset(1, 6.0, (2.0, 10.0)), 0.25);
        for mode in 1..=4 { assert_eq!(preset(mode, 10.0, (2.0, 10.0)), 0.0); }
    }
}
