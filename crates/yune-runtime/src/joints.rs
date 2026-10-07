use std::collections::HashMap;

use glam::Mat4;
use lune_roblox::{datatypes::types::CFrame, instance::Instance};
use rbx_dom_weak::types::{CFrame as DomCFrame, Variant};

#[derive(Default)]
pub struct JointSystem {
    weld_offsets: HashMap<String, Mat4>,
}

impl JointSystem {
    pub fn solve(&mut self, workspace: Instance) {
        let joints = workspace
            .get_descendants_preorder()
            .into_iter()
            .filter(|instance| {
                matches!(
                    instance.get_class_name(),
                    "Motor6D" | "Motor" | "Weld" | "WeldConstraint"
                )
            })
            .collect::<Vec<_>>();

        for _ in 0..4 {
            for joint in &joints {
                match joint.get_class_name() {
                    "Motor6D" | "Motor" | "Weld" => solve_cframe_joint(*joint),
                    "WeldConstraint" => self.solve_weld_constraint(*joint),
                    _ => {}
                }
            }
        }
    }

    fn solve_weld_constraint(&mut self, joint: Instance) {
        let Some(part0) = ref_prop(&joint, "Part0") else {
            return;
        };
        let Some(part1) = ref_prop(&joint, "Part1") else {
            return;
        };
        let part0_cf = cframe_prop(&part0, "CFrame", Mat4::IDENTITY);
        let part1_cf = cframe_prop(&part1, "CFrame", Mat4::IDENTITY);
        let key = instance_key(&joint);
        let offset = *self
            .weld_offsets
            .entry(key)
            .or_insert_with(|| part0_cf.inverse() * part1_cf);
        set_cframe(part1, part0_cf * offset);
    }
}

fn solve_cframe_joint(joint: Instance) {
    let Some(part0) = ref_prop(&joint, "Part0") else {
        return;
    };
    let Some(part1) = ref_prop(&joint, "Part1") else {
        return;
    };

    let part0_cf = cframe_prop(&part0, "CFrame", Mat4::IDENTITY);
    let c0 = cframe_prop(&joint, "C0", Mat4::IDENTITY);
    let c1 = cframe_prop(&joint, "C1", Mat4::IDENTITY);
    let transform = cframe_prop(&joint, "Transform", Mat4::IDENTITY);

    set_cframe(part1, part0_cf * c0 * transform * c1.inverse());
}

fn set_cframe(instance: Instance, value: Mat4) {
    instance.set_property("CFrame", Variant::CFrame(DomCFrame::from(CFrame(value))));
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

fn instance_key(instance: &Instance) -> String {
    format!("{}:{}", instance.dom_id, instance.dom_ref)
}
