use std::collections::{HashMap, HashSet};

use glam::{Mat4, Quat, Vec3};
use lune_roblox::{datatypes::types::CFrame, instance::Instance};
use rapier3d::prelude::*;
use rbx_dom_weak::types::{CFrame as DomCFrame, Variant, Vector3 as DomVector3};

#[derive(Debug, Clone, Copy)]
struct BodyRecord {
    body: RigidBodyHandle,
    collider: ColliderHandle,
    size: Vec3,
}

#[derive(Debug, Clone, Copy)]
pub struct RayHit {
    pub instance: Instance,
    pub position: Vec3,
    pub normal: Vec3,
    pub distance: f32,
}

pub struct PhysicsWorld {
    pipeline: PhysicsPipeline,
    islands: IslandManager,
    broad_phase: DefaultBroadPhase,
    narrow_phase: NarrowPhase,
    bodies: RigidBodySet,
    colliders: ColliderSet,
    impulse_joints: ImpulseJointSet,
    multibody_joints: MultibodyJointSet,
    ccd: CCDSolver,
    records: HashMap<String, BodyRecord>,
    collider_instances: HashMap<ColliderHandle, Instance>,
}

impl Default for PhysicsWorld {
    fn default() -> Self {
        Self {
            pipeline: PhysicsPipeline::new(),
            islands: IslandManager::new(),
            broad_phase: DefaultBroadPhase::new(),
            narrow_phase: NarrowPhase::new(),
            bodies: RigidBodySet::new(),
            colliders: ColliderSet::new(),
            impulse_joints: ImpulseJointSet::new(),
            multibody_joints: MultibodyJointSet::new(),
            ccd: CCDSolver::new(),
            records: HashMap::new(),
            collider_instances: HashMap::new(),
        }
    }
}

impl PhysicsWorld {
    pub fn step(&mut self, workspace: Instance, dt: f64) {
        if !dt.is_finite() || dt <= 0.0 { return; }
        let joint_children = collect_joint_children(workspace);
        let parts = workspace.get_descendants_preorder().into_iter()
            .filter(|instance| instance.is_a("BasePart") && instance.get_class_name() != "Terrain")
            .filter(|instance| !joint_children.contains(&instance_key(instance)))
            .collect::<Vec<_>>();
        let live = parts.iter().map(instance_key).collect::<HashSet<_>>();
        let stale = self.records.keys().filter(|key| !live.contains(*key)).cloned().collect::<Vec<_>>();
        for key in stale {
            if let Some(record) = self.records.remove(&key) {
                self.collider_instances.remove(&record.collider);
                self.bodies.remove(record.body, &mut self.islands, &mut self.colliders,
                    &mut self.impulse_joints, &mut self.multibody_joints, true);
            }
        }
        for part in &parts { self.sync_part(*part); }

        let gravity = Vec3::new(0.0, -f32_prop(&workspace, "Gravity", 196.2), 0.0);
        let substeps = (dt / (1.0 / 60.0)).ceil().clamp(1.0, 240.0) as u32;
        let parameters = IntegrationParameters { dt: (dt / substeps as f64) as f32, ..IntegrationParameters::default() };
        for _ in 0..substeps {
            self.pipeline.step(gravity, &parameters, &mut self.islands, &mut self.broad_phase,
                &mut self.narrow_phase, &mut self.bodies, &mut self.colliders,
                &mut self.impulse_joints, &mut self.multibody_joints, &mut self.ccd, &(), &());
        }
        for part in parts { self.write_back_part(part); }
    }

    pub fn apply_impulse(&mut self, instance: Instance, impulse: Vec3) {
        if !impulse.is_finite() { return; }
        self.sync_part(instance);
        if let Some(record) = self.records.get(&instance_key(&instance)).copied() {
            if let Some(body) = self.bodies.get_mut(record.body) { body.apply_impulse(impulse, true); }
        }
        self.write_back_part(instance);
    }

    pub fn apply_angular_impulse(&mut self, instance: Instance, impulse: Vec3) {
        if !impulse.is_finite() { return; }
        self.sync_part(instance);
        if let Some(record) = self.records.get(&instance_key(&instance)).copied() {
            if let Some(body) = self.bodies.get_mut(record.body) { body.apply_torque_impulse(impulse, true); }
        }
        self.write_back_part(instance);
    }

    pub fn mass(&mut self, instance: Instance) -> f32 {
        self.sync_part(instance);
        self.records.get(&instance_key(&instance))
            .and_then(|record| self.bodies.get(record.body)).map_or(0.0, RigidBody::mass)
    }

    pub fn raycast(&self, origin: Vec3, direction: Vec3, max_distance: f32) -> Option<RayHit> {
        let length = direction.length();
        if !origin.is_finite() || !direction.is_finite() || !max_distance.is_finite()
            || length <= f32::EPSILON || max_distance <= 0.0 { return None; }
        let ray = Ray::new(origin, direction / length);
        let predicate = |handle: ColliderHandle, _collider: &Collider| {
            self.collider_instances.get(&handle).is_some_and(|part| {
                bool_prop(part, "CanCollide", true) || bool_prop(part, "CanQuery", true)
            })
        };
        let filter = QueryFilter::default().predicate(&predicate);
        let query = self.broad_phase.as_query_pipeline(self.narrow_phase.query_dispatcher(),
            &self.bodies, &self.colliders, filter);
        let (handle, hit) = query.cast_ray_and_get_normal(&ray, max_distance, true)?;
        Some(RayHit {
            instance: *self.collider_instances.get(&handle)?,
            position: ray.origin + ray.dir * hit.time_of_impact,
            normal: hit.normal,
            distance: hit.time_of_impact,
        })
    }

    fn sync_part(&mut self, part: Instance) {
        let key = instance_key(&part);
        let anchored = bool_prop(&part, "Anchored", false);
        let can_collide = bool_prop(&part, "CanCollide", true);
        let cframe = cframe_prop(&part, "CFrame", Mat4::IDENTITY);
        let translation = cframe.w_axis.truncate();
        let rotation = Quat::from_mat4(&cframe).normalize();
        let linear_velocity = vec3_prop(&part, "AssemblyLinearVelocity", Vec3::ZERO);
        let angular_velocity = vec3_prop(&part, "AssemblyAngularVelocity", Vec3::ZERO);
        let size = vec3_prop(&part, "Size", Vec3::ONE).abs().max(Vec3::splat(0.001));
        if !translation.is_finite() || !rotation.is_finite() || !size.is_finite()
            || !linear_velocity.is_finite() || !angular_velocity.is_finite() { return; }

        if !self.records.contains_key(&key) {
            let (axis, angle) = rotation.to_axis_angle();
            let builder = if anchored { RigidBodyBuilder::fixed() } else { RigidBodyBuilder::dynamic() };
            let body = builder.translation(translation).rotation(axis * angle)
                .linvel(linear_velocity).angvel(angular_velocity).ccd_enabled(true).build();
            let body_handle = self.bodies.insert(body);
            let collider = ColliderBuilder::cuboid(size.x * 0.5, size.y * 0.5, size.z * 0.5)
                .friction(0.3).restitution(0.0).sensor(!can_collide).build();
            let collider_handle = self.colliders.insert_with_parent(collider, body_handle, &mut self.bodies);
            if let Some(body) = self.bodies.get_mut(body_handle) {
                body.recompute_mass_properties_from_colliders(&self.colliders);
            }
            self.collider_instances.insert(collider_handle, part);
            self.records.insert(key, BodyRecord { body: body_handle, collider: collider_handle, size });
            return;
        }

        let mut record = self.records[&key];
        if let Some(body) = self.bodies.get_mut(record.body) {
            body.set_body_type(if anchored { RigidBodyType::Fixed } else { RigidBodyType::Dynamic }, true);
            body.set_translation(translation, true);
            body.set_rotation(rotation, true);
            if !anchored {
                body.set_linvel(linear_velocity, true);
                body.set_angvel(angular_velocity, true);
            }
        }
        if let Some(collider) = self.colliders.get_mut(record.collider) {
            collider.set_sensor(!can_collide);
            if record.size != size {
                collider.set_shape(SharedShape::cuboid(size.x * 0.5, size.y * 0.5, size.z * 0.5));
                record.size = size;
                self.records.insert(key, record);
            }
        }
        if let Some(body) = self.bodies.get_mut(record.body) {
            body.recompute_mass_properties_from_colliders(&self.colliders);
        }
    }

    fn write_back_part(&self, part: Instance) {
        let Some(record) = self.records.get(&instance_key(&part)) else { return };
        let Some(body) = self.bodies.get(record.body) else { return };
        if bool_prop(&part, "Anchored", false) { return; }
        let cframe = Mat4::from_rotation_translation(*body.rotation(), body.translation());
        part.set_property("CFrame", Variant::CFrame(DomCFrame::from(CFrame(cframe))));
        part.set_property("AssemblyLinearVelocity", Variant::Vector3(DomVector3 {
            x: body.linvel().x, y: body.linvel().y, z: body.linvel().z,
        }));
        part.set_property("AssemblyAngularVelocity", Variant::Vector3(DomVector3 {
            x: body.angvel().x, y: body.angvel().y, z: body.angvel().z,
        }));
    }
}

fn collect_joint_children(workspace: Instance) -> HashSet<String> {
    workspace.get_descendants_preorder().into_iter()
        .filter(|instance| matches!(instance.get_class_name(), "Motor6D" | "Motor" | "Weld" | "WeldConstraint"))
        .filter(|joint| bool_prop(joint, "Enabled", true) && ref_prop(joint, "Part0").is_some())
        .filter_map(|joint| ref_prop(&joint, "Part1")).map(|part| instance_key(&part)).collect()
}

fn instance_key(instance: &Instance) -> String { format!("{}:{}", instance.dom_id, instance.dom_ref) }
fn bool_prop(instance: &Instance, name: &str, default: bool) -> bool {
    match instance.get_property(name) { Some(Variant::Bool(value)) => value, _ => default }
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
fn vec3_prop(instance: &Instance, name: &str, default: Vec3) -> Vec3 {
    match instance.get_property(name) { Some(Variant::Vector3(v)) => Vec3::new(v.x, v.y, v.z), _ => default }
}
fn cframe_prop(instance: &Instance, name: &str, default: Mat4) -> Mat4 {
    match instance.get_property(name) { Some(Variant::CFrame(value)) => CFrame::from(value).0, _ => default }
}
fn ref_prop(instance: &Instance, name: &str) -> Option<Instance> {
    match instance.get_property(name) { Some(Variant::Ref(r)) => Instance::new_opt(instance.dom_id, r), _ => None }
}
