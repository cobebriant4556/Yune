use std::io::Cursor;

use glam::{Vec2, Vec3};
use rbx_mesh::{
    mesh::{Face2, Mesh, Vertices2},
    read_mesh_versioned,
};

#[derive(Debug, Clone)]
pub struct StaticMesh {
    pub vertices: Vec<Vec3>,
    pub normals: Vec<Vec3>,
    pub uvs: Vec<Vec2>,
    pub triangles: Vec<[u32; 3]>,
    pub bounds_min: Vec3,
    pub bounds_max: Vec3,
}

impl StaticMesh {
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, rbx_mesh::mesh::Error> {
        let mesh = read_mesh_versioned(Cursor::new(bytes))?;
        let (vertices, normals, uvs, triangles) = match mesh {
            Mesh::V1(mesh) => {
                let vertices = mesh
                    .vertices
                    .iter()
                    .map(|vertex| Vec3::from_array(vertex.pos))
                    .collect::<Vec<_>>();
                let normals = mesh
                    .vertices
                    .iter()
                    .map(|vertex| Vec3::from_array(vertex.norm))
                    .collect::<Vec<_>>();
                let uvs = mesh
                    .vertices
                    .iter()
                    .map(|vertex| Vec2::new(vertex.tex[0], vertex.tex[1]))
                    .collect::<Vec<_>>();
                let triangles = (0..vertices.len() / 3)
                    .map(|face| {
                        let base = (face * 3) as u32;
                        [base, base + 1, base + 2]
                    })
                    .collect();
                (vertices, normals, uvs, triangles)
            }
            Mesh::V2(mesh) => {
                let (vertices, normals, uvs) = vertices2(&mesh.vertices);
                (vertices, normals, uvs, faces2(&mesh.faces))
            }
            Mesh::V3(mesh) => {
                let (vertices, normals, uvs) = vertices2(&mesh.vertices);
                (vertices, normals, uvs, faces2(&mesh.faces))
            }
            Mesh::V4(mesh) => (
                mesh.vertices
                    .iter()
                    .map(|vertex| Vec3::from_array(vertex.pos))
                    .collect(),
                mesh.vertices
                    .iter()
                    .map(|vertex| Vec3::from_array(vertex.norm))
                    .collect(),
                mesh.vertices
                    .iter()
                    .map(|vertex| Vec2::new(vertex.tex[0], vertex.tex[1]))
                    .collect(),
                faces2(&mesh.faces),
            ),
            Mesh::V5(mesh) => (
                mesh.vertices
                    .iter()
                    .map(|vertex| Vec3::from_array(vertex.pos))
                    .collect(),
                mesh.vertices
                    .iter()
                    .map(|vertex| Vec3::from_array(vertex.norm))
                    .collect(),
                mesh.vertices
                    .iter()
                    .map(|vertex| Vec2::new(vertex.tex[0], vertex.tex[1]))
                    .collect(),
                faces2(&mesh.faces),
            ),
        };

        let mut bounds_min = Vec3::splat(f32::INFINITY);
        let mut bounds_max = Vec3::splat(f32::NEG_INFINITY);
        for vertex in &vertices {
            bounds_min = bounds_min.min(*vertex);
            bounds_max = bounds_max.max(*vertex);
        }
        if vertices.is_empty() {
            bounds_min = Vec3::ZERO;
            bounds_max = Vec3::ZERO;
        }

        Ok(Self {
            vertices,
            normals,
            uvs,
            triangles,
            bounds_min,
            bounds_max,
        })
    }

    pub fn mesh_size(&self) -> Vec3 {
        (self.bounds_max - self.bounds_min).max(Vec3::splat(0.0001))
    }

    pub fn center(&self) -> Vec3 {
        (self.bounds_min + self.bounds_max) * 0.5
    }
}

fn vertices2(vertices: &Vertices2) -> (Vec<Vec3>, Vec<Vec3>, Vec<Vec2>) {
    match vertices {
        Vertices2::Full(vertices) => (
            vertices
                .iter()
                .map(|vertex| Vec3::from_array(vertex.pos))
                .collect(),
            vertices
                .iter()
                .map(|vertex| Vec3::from_array(vertex.norm))
                .collect(),
            vertices
                .iter()
                .map(|vertex| Vec2::new(vertex.tex[0], vertex.tex[1]))
                .collect(),
        ),
        Vertices2::Truncated(vertices) => (
            vertices
                .iter()
                .map(|vertex| Vec3::from_array(vertex.pos))
                .collect(),
            vertices
                .iter()
                .map(|vertex| Vec3::from_array(vertex.norm))
                .collect(),
            vertices
                .iter()
                .map(|vertex| Vec2::new(vertex.tex[0], vertex.tex[1]))
                .collect(),
        ),
    }
}

fn faces2(faces: &[Face2]) -> Vec<[u32; 3]> {
    faces
        .iter()
        .map(|face| [face.0[0].0, face.0[1].0, face.0[2].0])
        .collect()
}
