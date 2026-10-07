use std::sync::Arc;

use glam::{Mat4, Vec2, Vec3, Vec4};
use lune_roblox::instance::Instance;

use crate::{
    ImageBinding, RenderState,
    framebuffer::Framebuffer,
    props::{
        cframe_prop, color_prop, content_prop, enum_prop, f32_prop, instance_key, vec3_prop,
    },
};

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
    inv_w: f32,
}

#[derive(Clone)]
struct TexturePixels {
    width: u32,
    height: u32,
    pixels: Arc<Vec<u8>>,
}

#[derive(Clone, Default)]
struct SurfaceMaps {
    color: Option<TexturePixels>,
    roughness: Option<TexturePixels>,
    metalness: Option<TexturePixels>,
}

#[derive(Clone, Copy)]
struct MaterialResponse {
    roughness: f32,
    metalness: f32,
    emission: f32,
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
        let material = enum_prop(&instance, "Material", 256);
        let response = material_response(material);
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
                    if draw_triangle(
                        framebuffer,
                        view_projection,
                        [aw, bw, cw],
                        None,
                        None,
                        color,
                        alpha,
                        response,
                        SurfaceMaps::default(),
                        lighting,
                    ) {
                        stats.triangles += 1;
                    }
                }
            }
        } else if instance.get_class_name() == "MeshPart"
            && let Some(mesh_id) = content_prop(&instance, "MeshId")
            && let Some(mesh) = state.mesh_asset(&mesh_id)
        {
            let size = vec3_prop(&instance, "Size", mesh.mesh_size());
            let scale = size / mesh.mesh_size();
            let center = mesh.center();
            let maps = mesh_surface_maps(&instance, state);

            for triangle in &mesh.triangles {
                let ids = [
                    triangle[0] as usize,
                    triangle[1] as usize,
                    triangle[2] as usize,
                ];
                if ids.iter().any(|id| *id >= mesh.vertices.len()) {
                    continue;
                }

                let local = [
                    (mesh.vertices[ids[0]] - center) * scale,
                    (mesh.vertices[ids[1]] - center) * scale,
                    (mesh.vertices[ids[2]] - center) * scale,
                ];
                let world = local.map(|point| transform.transform_point3(point));

                let uvs = if ids.iter().all(|id| *id < mesh.uvs.len()) {
                    Some([mesh.uvs[ids[0]], mesh.uvs[ids[1]], mesh.uvs[ids[2]]])
                } else {
                    None
                };
                let normals = if ids.iter().all(|id| *id < mesh.normals.len()) {
                    Some([
                        transform.transform_vector3(mesh.normals[ids[0]]).normalize_or_zero(),
                        transform.transform_vector3(mesh.normals[ids[1]]).normalize_or_zero(),
                        transform.transform_vector3(mesh.normals[ids[2]]).normalize_or_zero(),
                    ])
                } else {
                    None
                };

                if draw_triangle(
                    framebuffer,
                    view_projection,
                    world,
                    uvs,
                    normals,
                    color,
                    alpha,
                    response,
                    maps.clone(),
                    lighting,
                ) {
                    stats.triangles += 1;
                }
            }
        } else {
            let size = vec3_prop(&instance, "Size", Vec3::new(4.0, 1.0, 2.0));
            stats.triangles += render_block(
                framebuffer,
                view_projection,
                transform,
                size,
                color,
                alpha,
                response,
                lighting,
            ) as u64;
        }
    }

    stats
}

fn mesh_surface_maps(instance: &Instance, state: &RenderState) -> SurfaceMaps {
    let mut maps = SurfaceMaps::default();

    let texture_id = content_prop(instance, "TextureID")
        .or_else(|| content_prop(instance, "TextureId"));
    if let Some(texture_id) = texture_id {
        maps.color = state.image_asset(&texture_id).map(texture_pixels);
    }

    if let Some(surface) = instance
        .get_children()
        .into_iter()
        .find(|child| child.get_class_name() == "SurfaceAppearance")
    {
        if let Some(id) = content_prop(&surface, "ColorMap") {
            maps.color = state.image_asset(&id).map(texture_pixels);
        }
        if let Some(id) = content_prop(&surface, "RoughnessMap") {
            maps.roughness = state.image_asset(&id).map(texture_pixels);
        }
        if let Some(id) = content_prop(&surface, "MetalnessMap") {
            maps.metalness = state.image_asset(&id).map(texture_pixels);
        }
    }

    maps
}

fn texture_pixels(binding: ImageBinding) -> TexturePixels {
    match binding {
        ImageBinding::Pixels {
            width,
            height,
            pixels,
        } => TexturePixels {
            width,
            height,
            pixels,
        },
        ImageBinding::Editable(image) => {
            let image = image.snapshot();
            TexturePixels {
                width: image.width,
                height: image.height,
                pixels: Arc::new(image.pixels),
            }
        }
    }
}

fn render_block(
    framebuffer: &mut Framebuffer,
    view_projection: Mat4,
    transform: Mat4,
    size: Vec3,
    color: Vec3,
    alpha: f32,
    response: MaterialResponse,
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
    let world = local.map(|point| transform.transform_point3(point));
    let mut count = 0;

    for [a, b, c] in indices {
        if draw_triangle(
            framebuffer,
            view_projection,
            [world[a], world[b], world[c]],
            None,
            None,
            color,
            alpha,
            response,
            SurfaceMaps::default(),
            lighting,
        ) {
            count += 1;
        }
    }

    count
}

fn project(
    framebuffer: &Framebuffer,
    view_projection: Mat4,
    point: Vec3,
) -> Option<ScreenVertex> {
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
        inv_w: 1.0 / clip.w,
    })
}

fn draw_triangle(
    framebuffer: &mut Framebuffer,
    view_projection: Mat4,
    world: [Vec3; 3],
    uvs: Option<[Vec2; 3]>,
    normals: Option<[Vec3; 3]>,
    base_color: Vec3,
    alpha: f32,
    material: MaterialResponse,
    maps: SurfaceMaps,
    lighting: Lighting,
) -> bool {
    let Some(a) = project(framebuffer, view_projection, world[0]) else {
        return false;
    };
    let Some(b) = project(framebuffer, view_projection, world[1]) else {
        return false;
    };
    let Some(c) = project(framebuffer, view_projection, world[2]) else {
        return false;
    };

    let geometric_normal = (world[1] - world[0])
        .cross(world[2] - world[0])
        .normalize_or_zero();

    let area = edge(a, b, c.x, c.y);
    if area.abs() < 0.0001 {
        return false;
    }

    let min_x = a.x.min(b.x).min(c.x).floor().max(0.0) as i32;
    let max_x = a
        .x
        .max(b.x)
        .max(c.x)
        .ceil()
        .min(framebuffer.width as f32 - 1.0) as i32;
    let min_y = a.y.min(b.y).min(c.y).floor().max(0.0) as i32;
    let max_y = a
        .y
        .max(b.y)
        .max(c.y)
        .ceil()
        .min(framebuffer.height as f32 - 1.0) as i32;

    if min_x > max_x || min_y > max_y {
        return false;
    }

    let screen = [a, b, c];
    let ambient = lighting.ambient.max(lighting.outdoor_ambient * 0.35);
    let light_direction = -lighting.light_direction.normalize_or_zero();
    let exposure = 2.0_f32.powf(lighting.exposure);

    for y in min_y..=max_y {
        for x in min_x..=max_x {
            let px = x as f32 + 0.5;
            let py = y as f32 + 0.5;
            let weights = [
                edge(b, c, px, py) / area,
                edge(c, a, px, py) / area,
                edge(a, b, px, py) / area,
            ];
            if weights.iter().any(|weight| *weight < 0.0) {
                continue;
            }

            let depth =
                weights[0] * a.z + weights[1] * b.z + weights[2] * c.z;
            let depth_index = y as usize * framebuffer.width as usize + x as usize;
            if depth >= framebuffer.depth[depth_index] {
                continue;
            }

            let world_position =
                world[0] * weights[0] + world[1] * weights[1] + world[2] * weights[2];
            let normal = if let Some(normals) = normals {
                (normals[0] * weights[0]
                    + normals[1] * weights[1]
                    + normals[2] * weights[2])
                    .normalize_or_zero()
            } else {
                geometric_normal
            };

            let uv = uvs.map(|uvs| perspective_uv(screen, uvs, weights));
            let texture = uv
                .and_then(|uv| maps.color.as_ref().map(|texture| sample_texture(texture, uv)))
                .unwrap_or(Vec4::ONE);

            let mut roughness = material.roughness;
            if let (Some(uv), Some(map)) = (uv, maps.roughness.as_ref()) {
                roughness = sample_texture(map, uv).x.clamp(0.02, 1.0);
            }
            let mut metalness = material.metalness;
            if let (Some(uv), Some(map)) = (uv, maps.metalness.as_ref()) {
                metalness = sample_texture(map, uv).x.clamp(0.0, 1.0);
            }

            let albedo = base_color * texture.truncate();
            let diffuse_amount = normal.dot(light_direction).max(0.0) * lighting.brightness;
            let view_direction = (lighting.camera_position - world_position).normalize_or_zero();
            let half_vector = (light_direction + view_direction).normalize_or_zero();
            let specular_power = 2.0 + (1.0 - roughness).powi(2) * 126.0;
            let specular_amount = normal
                .dot(half_vector)
                .max(0.0)
                .powf(specular_power)
                * lighting.brightness;
            let dielectric = 0.04;
            let specular_color =
                Vec3::splat(dielectric).lerp(albedo, metalness.clamp(0.0, 1.0));

            let mut lit = albedo * ambient;
            lit += albedo
                * lighting.light_color
                * diffuse_amount
                * (1.0 - metalness);
            lit += specular_color * lighting.light_color * specular_amount;
            lit += albedo * material.emission;
            lit *= exposure;

            let distance = world_position.distance(lighting.camera_position);
            let fog_range = (lighting.fog_end - lighting.fog_start).max(0.0001);
            let fog =
                ((distance - lighting.fog_start) / fog_range).clamp(0.0, 1.0);
            lit = lit.lerp(lighting.fog_color, fog);

            let output_alpha = (alpha * texture.w).clamp(0.0, 1.0);
            let rgba = [
                (lit.x.clamp(0.0, 1.0) * 255.0).round() as u8,
                (lit.y.clamp(0.0, 1.0) * 255.0).round() as u8,
                (lit.z.clamp(0.0, 1.0) * 255.0).round() as u8,
                (output_alpha * 255.0).round() as u8,
            ];

            framebuffer.blend_pixel(x, y, rgba);
            if output_alpha >= 0.98 {
                framebuffer.depth[depth_index] = depth;
            }
        }
    }

    true
}

fn perspective_uv(
    screen: [ScreenVertex; 3],
    uvs: [Vec2; 3],
    weights: [f32; 3],
) -> Vec2 {
    let weighted = [
        weights[0] * screen[0].inv_w,
        weights[1] * screen[1].inv_w,
        weights[2] * screen[2].inv_w,
    ];
    let denominator = weighted[0] + weighted[1] + weighted[2];
    if denominator.abs() <= f32::EPSILON {
        return uvs[0];
    }

    (uvs[0] * weighted[0] + uvs[1] * weighted[1] + uvs[2] * weighted[2])
        / denominator
}

fn sample_texture(texture: &TexturePixels, uv: Vec2) -> Vec4 {
    if texture.width == 0 || texture.height == 0 || texture.pixels.is_empty() {
        return Vec4::ONE;
    }

    let u = uv.x.rem_euclid(1.0);
    let v = uv.y.rem_euclid(1.0);
    let x = u * texture.width.saturating_sub(1) as f32;
    let y = v * texture.height.saturating_sub(1) as f32;
    let x0 = x.floor() as i32;
    let y0 = y.floor() as i32;
    let tx = x - x0 as f32;
    let ty = y - y0 as f32;

    let a = texture_pixel(texture, x0, y0);
    let b = texture_pixel(texture, x0 + 1, y0);
    let c = texture_pixel(texture, x0, y0 + 1);
    let d = texture_pixel(texture, x0 + 1, y0 + 1);

    a.lerp(b, tx).lerp(c.lerp(d, tx), ty)
}

fn texture_pixel(texture: &TexturePixels, x: i32, y: i32) -> Vec4 {
    let x = x.clamp(0, texture.width as i32 - 1) as usize;
    let y = y.clamp(0, texture.height as i32 - 1) as usize;
    let index = (y * texture.width as usize + x) * 4;
    if index + 3 >= texture.pixels.len() {
        return Vec4::ONE;
    }

    Vec4::new(
        texture.pixels[index] as f32 / 255.0,
        texture.pixels[index + 1] as f32 / 255.0,
        texture.pixels[index + 2] as f32 / 255.0,
        texture.pixels[index + 3] as f32 / 255.0,
    )
}

fn material_response(material: u32) -> MaterialResponse {
    match material {
        272 => MaterialResponse { roughness: 0.2, metalness: 0.0, emission: 0.0 },
        288 => MaterialResponse { roughness: 0.3, metalness: 0.0, emission: 1.0 },
        512 | 528 => MaterialResponse { roughness: 0.48, metalness: 0.0, emission: 0.0 },
        784 => MaterialResponse { roughness: 0.2, metalness: 0.0, emission: 0.0 },
        788 | 800 | 816 | 820 | 832 | 836 | 848 | 864 | 880 | 896 | 912 => {
            MaterialResponse { roughness: 0.5, metalness: 0.0, emission: 0.0 }
        }
        1040 => MaterialResponse { roughness: 0.55, metalness: 0.65, emission: 0.0 },
        1056 | 1072 | 1088 => MaterialResponse { roughness: 0.35, metalness: 1.0, emission: 0.0 },
        1280 | 1284 | 1296 | 1344 | 1360 | 1376 | 1392 => {
            MaterialResponse { roughness: 0.65, metalness: 0.0, emission: 0.0 }
        }
        1312 => MaterialResponse { roughness: 0.7, metalness: 0.0, emission: 0.0 },
        1328 => MaterialResponse { roughness: 0.3, metalness: 0.0, emission: 0.0 },
        1536 | 1552 => MaterialResponse { roughness: 0.05, metalness: 0.0, emission: 0.0 },
        1568 => MaterialResponse { roughness: 0.08, metalness: 0.0, emission: 0.0 },
        1584 => MaterialResponse { roughness: 0.25, metalness: 0.0, emission: 0.45 },
        2304 | 2305 | 2306 | 2307 | 2308 | 2309 | 2310 | 2311 => {
            MaterialResponse { roughness: 0.65, metalness: 0.0, emission: 0.0 }
        }
        _ => MaterialResponse { roughness: 0.3, metalness: 0.0, emission: 0.0 },
    }
}

fn edge(a: ScreenVertex, b: ScreenVertex, x: f32, y: f32) -> f32 {
    (x - a.x) * (b.y - a.y) - (y - a.y) * (b.x - a.x)
}
