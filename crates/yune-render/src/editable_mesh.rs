use std::{collections::{BTreeMap, BTreeSet}, sync::{Arc, Mutex}};

use glam::Vec3;
use lune_roblox::datatypes::types::Vector3;
use mlua::prelude::*;

#[derive(Debug, Clone)]
pub struct MeshSnapshot {
    pub vertices: BTreeMap<i64, Vec3>,
    pub faces: BTreeMap<i64, Vec<i64>>,
}

#[derive(Debug)]
struct MeshData {
    next_vertex: i64,
    next_face: i64,
    vertices: BTreeMap<i64, Vec3>,
    faces: BTreeMap<i64, Vec<i64>>,
    destroyed: bool,
}

impl Default for MeshData {
    fn default() -> Self {
        Self {
            next_vertex: 1,
            next_face: 1,
            vertices: BTreeMap::new(),
            faces: BTreeMap::new(),
            destroyed: false,
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct EditableMesh {
    inner: Arc<Mutex<MeshData>>,
}

impl EditableMesh {
    pub fn snapshot(&self) -> MeshSnapshot {
        let data = self.inner.lock().expect("editable mesh lock poisoned");
        MeshSnapshot {
            vertices: data.vertices.clone(),
            faces: data.faces.clone(),
        }
    }

    fn ensure_alive(data: &MeshData) -> LuaResult<()> {
        if data.destroyed {
            Err(LuaError::runtime("EditableMesh has been destroyed"))
        } else {
            Ok(())
        }
    }

    fn check_vertex(data: &MeshData, id: i64) -> LuaResult<()> {
        if data.vertices.contains_key(&id) {
            Ok(())
        } else {
            Err(LuaError::runtime(format!("invalid vertex id {id}")))
        }
    }

    fn check_face(data: &MeshData, id: i64) -> LuaResult<()> {
        if data.faces.contains_key(&id) {
            Ok(())
        } else {
            Err(LuaError::runtime(format!("invalid face id {id}")))
        }
    }
}

impl LuaUserData for EditableMesh {
    fn add_fields<F: LuaUserDataFields<Self>>(fields: &mut F) {
        fields.add_field_method_get("FixedSize", |_, _| Ok(false));
    }

    fn add_methods<M: LuaUserDataMethods<Self>>(methods: &mut M) {
        methods.add_method_mut("AddVertex", |_, this, position: LuaUserDataRef<Vector3>| {
            let mut data = this.inner.lock().expect("editable mesh lock poisoned");
            Self::ensure_alive(&data)?;
            if data.vertices.len() >= 60_000 {
                return Err(LuaError::runtime("EditableMesh vertex limit reached"));
            }
            let id = data.next_vertex;
            data.next_vertex += 1;
            data.vertices.insert(id, position.0);
            Ok(id)
        });

        methods.add_method_mut("AddTriangle", |_, this, (a, b, c): (i64, i64, i64)| {
            let mut data = this.inner.lock().expect("editable mesh lock poisoned");
            Self::ensure_alive(&data)?;
            Self::check_vertex(&data, a)?;
            Self::check_vertex(&data, b)?;
            Self::check_vertex(&data, c)?;
            let id = data.next_face;
            data.next_face += 1;
            data.faces.insert(id, vec![a, b, c]);
            Ok(id)
        });

        methods.add_method_mut("AddFace", |_, this, vertices: LuaTable| {
            let ids = vertices.sequence_values::<i64>().collect::<LuaResult<Vec<_>>>()?;
            if !(3..=4).contains(&ids.len()) {
                return Err(LuaError::runtime("EditableMesh:AddFace expects 3 or 4 vertex IDs"));
            }
            let mut data = this.inner.lock().expect("editable mesh lock poisoned");
            Self::ensure_alive(&data)?;
            for id in &ids {
                Self::check_vertex(&data, *id)?;
            }
            let id = data.next_face;
            data.next_face += 1;
            data.faces.insert(id, ids);
            Ok(id)
        });

        methods.add_method("GetVertices", |lua, this, ()| {
            let data = this.inner.lock().expect("editable mesh lock poisoned");
            Self::ensure_alive(&data)?;
            let out = lua.create_table_with_capacity(data.vertices.len(), 0)?;
            for id in data.vertices.keys() {
                out.push(*id)?;
            }
            Ok(out)
        });

        methods.add_method("GetFaces", |lua, this, ()| {
            let data = this.inner.lock().expect("editable mesh lock poisoned");
            Self::ensure_alive(&data)?;
            let out = lua.create_table_with_capacity(data.faces.len(), 0)?;
            for id in data.faces.keys() {
                out.push(*id)?;
            }
            Ok(out)
        });

        methods.add_method("GetFaceVertices", |lua, this, face_id: i64| {
            let data = this.inner.lock().expect("editable mesh lock poisoned");
            Self::ensure_alive(&data)?;
            Self::check_face(&data, face_id)?;
            let face = data.faces.get(&face_id).expect("face checked");
            let out = lua.create_table_with_capacity(face.len(), 0)?;
            for id in face {
                out.push(*id)?;
            }
            Ok(out)
        });

        methods.add_method("GetPosition", |_, this, vertex_id: i64| {
            let data = this.inner.lock().expect("editable mesh lock poisoned");
            Self::ensure_alive(&data)?;
            Self::check_vertex(&data, vertex_id)?;
            Ok(Vector3(*data.vertices.get(&vertex_id).expect("vertex checked")))
        });

        methods.add_method_mut("SetPosition", |_, this, (vertex_id, position): (i64, LuaUserDataRef<Vector3>)| {
            let mut data = this.inner.lock().expect("editable mesh lock poisoned");
            Self::ensure_alive(&data)?;
            Self::check_vertex(&data, vertex_id)?;
            data.vertices.insert(vertex_id, position.0);
            Ok(())
        });

        methods.add_method_mut("RemoveFace", |_, this, face_id: i64| {
            let mut data = this.inner.lock().expect("editable mesh lock poisoned");
            Self::ensure_alive(&data)?;
            if data.faces.remove(&face_id).is_none() {
                return Err(LuaError::runtime(format!("invalid face id {face_id}")));
            }
            Ok(())
        });

        methods.add_method_mut("Clear", |_, this, ()| {
            let mut data = this.inner.lock().expect("editable mesh lock poisoned");
            Self::ensure_alive(&data)?;
            data.vertices.clear();
            data.faces.clear();
            Ok(())
        });

        methods.add_method("GetSize", |_, this, ()| {
            let data = this.inner.lock().expect("editable mesh lock poisoned");
            Self::ensure_alive(&data)?;
            if data.vertices.is_empty() {
                return Ok(Vector3(Vec3::ZERO));
            }
            let mut min = Vec3::splat(f32::INFINITY);
            let mut max = Vec3::splat(f32::NEG_INFINITY);
            for p in data.vertices.values() {
                min = min.min(*p);
                max = max.max(*p);
            }
            Ok(Vector3(max - min))
        });

        methods.add_method("FindClosestVertex", |_, this, point: LuaUserDataRef<Vector3>| {
            let data = this.inner.lock().expect("editable mesh lock poisoned");
            Self::ensure_alive(&data)?;
            let mut best = None;
            let mut best_d2 = f32::INFINITY;
            for (id, p) in &data.vertices {
                let d2 = p.distance_squared(point.0);
                if d2 < best_d2 {
                    best_d2 = d2;
                    best = Some(*id);
                }
            }
            Ok(best.unwrap_or(0))
        });

        methods.add_method("GetVertexFaces", |lua, this, vertex_id: i64| {
            let data = this.inner.lock().expect("editable mesh lock poisoned");
            Self::ensure_alive(&data)?;
            Self::check_vertex(&data, vertex_id)?;
            let out = lua.create_table()?;
            for (face_id, face) in &data.faces {
                if face.contains(&vertex_id) {
                    out.push(*face_id)?;
                }
            }
            Ok(out)
        });

        methods.add_method("GetAdjacentVertices", |lua, this, vertex_id: i64| {
            let data = this.inner.lock().expect("editable mesh lock poisoned");
            Self::ensure_alive(&data)?;
            Self::check_vertex(&data, vertex_id)?;
            let mut adjacent = BTreeSet::new();
            for face in data.faces.values() {
                if face.contains(&vertex_id) {
                    for id in face {
                        if *id != vertex_id {
                            adjacent.insert(*id);
                        }
                    }
                }
            }
            let out = lua.create_table_with_capacity(adjacent.len(), 0)?;
            for id in adjacent {
                out.push(id)?;
            }
            Ok(out)
        });

        methods.add_method("GetAdjacentFaces", |lua, this, face_id: i64| {
            let data = this.inner.lock().expect("editable mesh lock poisoned");
            Self::ensure_alive(&data)?;
            Self::check_face(&data, face_id)?;
            let source = data.faces.get(&face_id).expect("face checked");
            let source_set = source.iter().copied().collect::<BTreeSet<_>>();
            let out = lua.create_table()?;
            for (other_id, other) in &data.faces {
                if *other_id == face_id {
                    continue;
                }
                let shared = other.iter().filter(|id| source_set.contains(id)).count();
                if shared >= 2 {
                    out.push(*other_id)?;
                }
            }
            Ok(out)
        });

        methods.add_method("IdDebugString", |_, this, id: i64| {
            let data = this.inner.lock().expect("editable mesh lock poisoned");
            Self::ensure_alive(&data)?;
            if data.vertices.contains_key(&id) {
                Ok(format!("v{id}"))
            } else if data.faces.contains_key(&id) {
                Ok(format!("f{id}"))
            } else {
                Ok(format!("?{id}"))
            }
        });

        methods.add_method_mut("Destroy", |_, this, ()| {
            let mut data = this.inner.lock().expect("editable mesh lock poisoned");
            data.vertices.clear();
            data.faces.clear();
            data.destroyed = true;
            Ok(())
        });
    }
}
