use std::collections::{HashMap, HashSet, VecDeque};
use glam::Mat4;
use lune_roblox::{datatypes::types::CFrame, instance::Instance};
use rbx_dom_weak::types::{CFrame as DomCFrame, Variant};

struct WeldOffset {
    part0: String,
    part1: String,
    relative: Mat4,
}

#[derive(Default)]
pub struct JointSystem {
    weld_offsets: HashMap<String, WeldOffset>,
}

impl JointSystem {
    pub fn solve(&mut self, workspace: Instance) {
        let descendants = workspace.get_descendants_preorder();
        let parts = descendants.iter().copied().filter(|part| part.is_a("BasePart") && part.get_class_name() != "Terrain").collect::<Vec<_>>();
        let by_key = parts.iter().map(|part| (instance_key(part), *part)).collect::<HashMap<_, _>>();
        let mut adjacency: HashMap<String, Vec<(String, Mat4)>> = HashMap::new();
        let mut children = HashSet::new();
        let mut active_welds = HashSet::new();
        for joint in descendants {
            let class = joint.get_class_name();
            if !matches!(class, "Motor6D" | "Motor" | "Weld" | "WeldConstraint") || !bool_prop(&joint, "Enabled", true) { continue; }
            let (Some(part0), Some(part1)) = (ref_prop(&joint, "Part0"), ref_prop(&joint, "Part1")) else { continue };
            let key0 = instance_key(&part0);
            let key1 = instance_key(&part1);
            if key0 == key1 || !by_key.contains_key(&key0) || !by_key.contains_key(&key1) { continue; }
            let relative = if class == "WeldConstraint" {
                let key = instance_key(&joint);
                active_welds.insert(key.clone());
                let recapture = self.weld_offsets.get(&key).is_none_or(|offset| offset.part0 != key0 || offset.part1 != key1);
                if recapture {
                    self.weld_offsets.insert(key.clone(), WeldOffset {
                        part0: key0.clone(), part1: key1.clone(),
                        relative: cframe_prop(&part0, "CFrame").inverse() * cframe_prop(&part1, "CFrame"),
                    });
                }
                self.weld_offsets[&key].relative
            } else {
                let transform = if class == "Motor6D" { cframe_prop(&joint, "Transform") } else { Mat4::IDENTITY };
                cframe_prop(&joint, "C0") * transform * cframe_prop(&joint, "C1").inverse()
            };
            if !relative.is_finite() || relative.determinant().abs() < 1e-8 { continue; }
            adjacency.entry(key0.clone()).or_default().push((key1.clone(), relative));
            adjacency.entry(key1.clone()).or_default().push((key0, relative.inverse()));
            children.insert(key1);
        }
        self.weld_offsets.retain(|key, _| active_welds.contains(key));

        let mut roots = Vec::new();
        roots.extend(parts.iter().filter(|part| bool_prop(part, "Anchored", false)).map(instance_key));
        roots.extend(parts.iter().filter(|part| !children.contains(&instance_key(part))).map(instance_key));
        roots.extend(parts.iter().map(instance_key));
        let mut visited = HashSet::new();
        for root in roots {
            if !visited.insert(root.clone()) { continue; }
            let mut queue = VecDeque::from([root]);
            while let Some(key) = queue.pop_front() {
                let Some(neighbors) = adjacency.get(&key) else { continue };
                let parent_world = cframe_prop(&by_key[&key], "CFrame");
                for (next, relative) in neighbors {
                    if !visited.insert(next.clone()) { continue; }
                    let child = by_key[next];
                    if !bool_prop(&child, "Anchored", false) {
                        let value = parent_world * *relative;
                        if value.is_finite() {
                            child.set_property("CFrame", Variant::CFrame(DomCFrame::from(CFrame(value))));
                        }
                    }
                    queue.push_back(next.clone());
                }
            }
        }
    }
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
