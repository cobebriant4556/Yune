use glam::{Mat4, Vec3, Vec4};
use lune_roblox::instance::Instance;

use crate::{RenderState, framebuffer::Framebuffer, props::{cframe_prop, color_prop, f32_prop, instance_key, vec3_prop}};

#[derive(Debug, Clone, Copy)]
pub struct Lighting {
    pub ambient: Vec3,
    pub outdoor_ambient: Vec3,
    pub light_color: Vec3,
    pub light_direction: Vec3,
    pub brightness: f32,
    pub exposure: f32,
    pub fog_color: Vec3,
    pub fog_start: f32,
    pub fog_end: f32,
    pub camera_position: Vec3,
}

impl Default for Lighting {
    fn default() -> Self {
        Self {
            ambient: Vec3::splat(0.35),
            outdoor_ambient: Vec3::splat(0.5),
            light_color: Vec3::ONE,
            light_direction: Vec3::new(-0.6, -1.0, -0.45).normalize(),
            brightness: 0.85,
            exposure: 0.0,
            fog_color: Vec3::new(0.75, 0.82, 0.9),
            fog_start: 0.0,
            fog_end: 100000.0,
            camera_position: Vec3::ZERO,
        }
    }
}

#[derive(Debug, Clone, Copy, Default)]
pub struct RenderStats {
    pub parts: u64,
    pub triangles: u64,
}

#[derive(Debug, Clone, Copy)]
struct ScreenVertex {
    x: f32,
    y: f32,
    z: f32,
}

pub fn render_world(
    framebuffer: &mut Framebuffer,
    root: Instance,
    camera: Instance,
    state: &RenderState,
    lighting: Lighting,
) -> RenderStats {
    let camera_cf = cframe_prop(&camera, "CFrame", Mat4::IDENTITY);
    let mut lighting = lighting;
    lighting.camera_position = camera_cf.w_axis.truncate();
    let fov = f32_prop(&camera, "FieldOfView", 70.0).clamp(1.0, 120.0);
    let aspect = framebuffer.width as f32 / framebuffer.height as f32;
    let projection = Mat4::perspective_rh(fov.to_radians(), aspect, 0.05, 10000.0);
    let view_projection = projection * camera_cf.inverse();

    let mut stats = RenderStats::default();
    for instance in root.get_descendants_preorder() {
        if !instance.is_a("BasePart") {
            continue;
        }
        let transparency = f32_prop(&instance, "Transparency", 0.0).clamp(0.0, 1.0);
        if transparency >= 1.0 {
            continue;
        }
        stats.parts += 1;
        let transform = cframe_prop(&instance, "CFrame", Mat4::IDENTITY);
        let color = color_prop(&instance, "Color", Vec3::new(0.64, 0.64, 0.67));
        let alpha = 1.0 - transparency;
        let key = instance_key(&instance);
        if let Some(mesh) = state.mesh_binding(&key) {
            let mesh = mesh.snapshot();
            for face in mesh.faces.values() {
                if face.len() < 3 {
                    continue;
                }
                for i in 1..face.len() - 1 {
                    let Some(a) = mesh.vertices.get(&face[0]) else { continue };
                    let Some(b) = mesh.vertices.get(&face[i]) else { continue };
                    let Some(c) = mesh.vertices.get(&face[i + 1]) else { continue };
                    let aw = transform.transform_point3(*a);
                    let bw = transform.transform_point3(*b);
                    let cw = transform.transform_point3(*c);
                    if draw_triangle(framebuffer, view_projection, aw, bw, cw, color, alpha, lighting) {
                        stats.triangles += 1;
                    }
                }
            }
        } else {
            let size = vec3_prop(&instance, "Size", Vec3::new(4.0, 1.0, 2.0));
            stats.triangles += render_block(framebuffer, view_projection, transform, size, color, alpha, lighting) as u64;
        }
    }
    stats
}

fn render_block(
    framebuffer: &mut Framebuffer,
    view_projection: Mat4,
    transform: Mat4,
    size: Vec3,
    color: Vec3,
    alpha: f32,
    lighting: Lighting,
) -> usize {
    let h = size * 0.5;
    let local = [
        Vec3::new(-h.x, -h.y, -h.z),
        Vec3::new(h.x, -h.y, -h.z),
        Vec3::new(h.x, h.y, -h.z),
        Vec3::new(-h.x, h.y, -h.z),
        Vec3::new(-h.x, -h.y, h.z),
        Vec3::new(h.x, -h.y, h.z),
        Vec3::new(h.x, h.y, h.z),
        Vec3::new(-h.x, h.y, h.z),
    ];
    let indices = [
        [0, 2, 1], [0, 3, 2],
        [4, 5, 6], [4, 6, 7],
        [0, 1, 5], [0, 5, 4],
        [3, 7, 6], [3, 6, 2],
        [0, 4, 7], [0, 7, 3],
        [1, 2, 6], [1, 6, 5],
    ];
    let world = local.map(|p| transform.transform_point3(p));
    let mut count = 0;
    for [a, b, c] in indices {
        if draw_triangle(framebuffer, view_projection, world[a], world[b], world[c], color, alpha, lighting) {
            count += 1;
        }
    }
    count
}

fn project(framebuffer: &Framebuffer, view_projection: Mat4, point: Vec3) -> Option<ScreenVertex> {
    let clip: Vec4 = view_projection * point.extend(1.0);
    if clip.w <= 0.0001 {
        return None;
    }
    let ndc = clip.truncate() / clip.w;
    if ndc.z < 0.0 || ndc.z > 1.0 {
        return None;
    }
    Some(ScreenVertex {
        x: (ndc.x * 0.5 + 0.5) * framebuffer.width as f32,
        y: (0.5 - ndc.y * 0.5) * framebuffer.height as f32,
        z: ndc.z,
    })
}

fn draw_triangle(
    framebuffer: &mut Framebuffer,
    view_projection: Mat4,
    a_world: Vec3,
    b_world: Vec3,
    c_world: Vec3,
    base_color: Vec3,
    alpha: f32,
    lighting: Lighting,
) -> bool {
    let Some(a) = project(framebuffer, view_projection, a_world) else { return false };
    let Some(b) = project(framebuffer, view_projection, b_world) else { return false };
    let Some(c) = project(framebuffer, view_projection, c_world) else { return false };

    let normal = (b_world - a_world).cross(c_world - a_world).normalize_or_zero();
    let diffuse = normal
        .dot(-lighting.light_direction.normalize_or_zero())
        .max(0.0)
        * lighting.brightness;
    let ambient = lighting.ambient.max(lighting.outdoor_ambient * 0.35);
    let exposure = 2.0_f32.powf(lighting.exposure);
    let mut lit =
        (base_color * ambient + base_color * lighting.light_color * diffuse) * exposure;

    let center = (a_world + b_world + c_world) / 3.0;
    let distance = center.distance(lighting.camera_position);
    let fog_range = (lighting.fog_end - lighting.fog_start).max(0.0001);
    let fog = ((distance - lighting.fog_start) / fog_range).clamp(0.0, 1.0);
    lit = lit.lerp(lighting.fog_color, fog);

    let rgba = [
        (lit.x.clamp(0.0, 1.0) * 255.0).round() as u8,
        (lit.y.clamp(0.0, 1.0) * 255.0).round() as u8,
        (lit.z.clamp(0.0, 1.0) * 255.0).round() as u8,
        (alpha.clamp(0.0, 1.0) * 255.0).round() as u8,
    ];

    let area = edge(a, b, c.x, c.y);
    if area.abs() < 0.0001 {
        return false;
    }

    let min_x = a.x.min(b.x).min(c.x).floor().max(0.0) as i32;
    let max_x = a.x.max(b.x).max(c.x).ceil().min(framebuffer.width as f32 - 1.0) as i32;
    let min_y = a.y.min(b.y).min(c.y).floor().max(0.0) as i32;
    let max_y = a.y.max(b.y).max(c.y).ceil().min(framebuffer.height as f32 - 1.0) as i32;

    if min_x > max_x || min_y > max_y {
        return false;
    }

    for y in min_y..=max_y {
        for x in min_x..=max_x {
            let px = x as f32 + 0.5;
            let py = y as f32 + 0.5;
            let w0 = edge(b, c, px, py) / area;
            let w1 = edge(c, a, px, py) / area;
            let w2 = edge(a, b, px, py) / area;
            if w0 < 0.0 || w1 < 0.0 || w2 < 0.0 {
                continue;
            }
            let depth = w0 * a.z + w1 * b.z + w2 * c.z;
            let depth_index = y as usize * framebuffer.width as usize + x as usize;
            if depth >= framebuffer.depth[depth_index] {
                continue;
            }
            framebuffer.blend_pixel(x, y, rgba);
            if rgba[3] >= 250 {
                framebuffer.depth[depth_index] = depth;
            }
        }
    }
    true
}

fn edge(a: ScreenVertex, b: ScreenVertex, x: f32, y: f32) -> f32 {
    (x - a.x) * (b.y - a.y) - (y - a.y) * (b.x - a.x)
}
