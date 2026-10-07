use std::sync::Arc;

use glam::{Mat4, Vec2, Vec3, Vec4};
use lune_roblox::{
    datatypes::types::CFrame,
    instance::Instance,
};
use rbx_dom_weak::types::Variant;

use crate::{
    ImageBinding, RenderState,
    framebuffer::Framebuffer,
    gui::{GuiStats, Rect, render_gui_in_rect},
    props::{
        bool_prop, cframe_prop, color_prop, content_prop, enum_prop, f32_prop, ref_prop,
        udim2_prop, vec2_prop, vec3_prop,
    },
};

#[derive(Clone, Copy)]
struct Projected {
    x: f32,
    y: f32,
    depth: f32,
    inv_w: f32,
}

pub fn render_world_guis(
    framebuffer: &mut Framebuffer,
    world: Instance,
    camera: Instance,
    state: &RenderState,
) -> GuiStats {
    let mut total = GuiStats::default();

    for surface in world.get_descendants_preorder() {
        if matches!(surface.get_class_name(), "Decal" | "Texture") {
            render_part_image(framebuffer, surface, camera, state);
        }
    }

    for gui in world.get_descendants_preorder() {
        let stats = match gui.get_class_name() {
            "BillboardGui" => render_billboard(framebuffer, gui, camera, state),
            "SurfaceGui" => render_surface(framebuffer, gui, camera, state),
            _ => continue,
        };
        total.objects += stats.objects;
        total.viewport_parts += stats.viewport_parts;
        total.viewport_triangles += stats.viewport_triangles;
    }

    total
}

#[derive(Clone)]
struct ImageSurface {
    width: u32,
    height: u32,
    pixels: Arc<Vec<u8>>,
}

fn render_part_image(
    framebuffer: &mut Framebuffer,
    image_instance: Instance,
    camera: Instance,
    state: &RenderState,
) {
    let Some(parent) = image_instance.get_parent() else {
        return;
    };
    if !parent.is_a("BasePart") {
        return;
    }

    let Some(content_id) = content_prop(&image_instance, "Texture") else {
        return;
    };
    let Some(binding) = state.image_asset(&content_id) else {
        return;
    };
    let image = image_surface(binding);
    let tint = color_prop(&image_instance, "Color3", Vec3::ONE);
    let transparency = f32_prop(&image_instance, "Transparency", 0.0).clamp(0.0, 1.0);
    let face = enum_prop(&image_instance, "Face", 5);
    let part_size = vec3_prop(&parent, "Size", Vec3::ONE);
    let face_size = surface_face_size(part_size, face);

    let (uv0, uv1) = if image_instance.get_class_name() == "Texture" {
        let studs_u = f32_prop(&image_instance, "StudsPerTileU", 2.0).abs().max(0.001);
        let studs_v = f32_prop(&image_instance, "StudsPerTileV", 2.0).abs().max(0.001);
        let offset_u = f32_prop(&image_instance, "OffsetStudsU", 0.0) / studs_u;
        let offset_v = f32_prop(&image_instance, "OffsetStudsV", 0.0) / studs_v;
        (
            Vec2::new(offset_u, offset_v),
            Vec2::new(
                offset_u + face_size.x / studs_u,
                offset_v + face_size.y / studs_v,
            ),
        )
    } else {
        (Vec2::ZERO, Vec2::ONE)
    };

    let transform = cframe_prop(&parent, "CFrame", Mat4::IDENTITY);
    let corners =
        surface_face_corners(part_size, face).map(|point| transform.transform_point3(point));

    draw_image_quad(
        framebuffer,
        camera,
        corners,
        &image,
        [
            Vec2::new(uv0.x, uv0.y),
            Vec2::new(uv1.x, uv0.y),
            Vec2::new(uv1.x, uv1.y),
            Vec2::new(uv0.x, uv1.y),
        ],
        tint,
        transparency,
        image_instance.get_class_name() == "Texture",
    );
}

fn image_surface(binding: ImageBinding) -> ImageSurface {
    match binding {
        ImageBinding::Pixels {
            width,
            height,
            pixels,
        } => ImageSurface {
            width,
            height,
            pixels,
        },
        ImageBinding::Editable(image) => {
            let image = image.snapshot();
            ImageSurface {
                width: image.width,
                height: image.height,
                pixels: Arc::new(image.pixels),
            }
        }
    }
}

fn draw_image_quad(
    framebuffer: &mut Framebuffer,
    camera: Instance,
    corners: [Vec3; 4],
    image: &ImageSurface,
    uvs: [Vec2; 4],
    tint: Vec3,
    transparency: f32,
    repeat: bool,
) {
    let Some(a) = project(framebuffer, camera, corners[0]) else {
        return;
    };
    let Some(b) = project(framebuffer, camera, corners[1]) else {
        return;
    };
    let Some(c) = project(framebuffer, camera, corners[2]) else {
        return;
    };
    let Some(d) = project(framebuffer, camera, corners[3]) else {
        return;
    };

    draw_image_triangle(
        framebuffer,
        [a, b, c],
        [uvs[0], uvs[1], uvs[2]],
        image,
        tint,
        transparency,
        repeat,
    );
    draw_image_triangle(
        framebuffer,
        [a, c, d],
        [uvs[0], uvs[2], uvs[3]],
        image,
        tint,
        transparency,
        repeat,
    );
}

fn draw_image_triangle(
    framebuffer: &mut Framebuffer,
    vertices: [Projected; 3],
    uvs: [Vec2; 3],
    image: &ImageSurface,
    tint: Vec3,
    transparency: f32,
    repeat: bool,
) {
    let area = edge(vertices[0], vertices[1], vertices[2].x, vertices[2].y);
    if area.abs() <= 0.0001 {
        return;
    }

    let min_x = vertices
        .iter()
        .map(|vertex| vertex.x)
        .fold(f32::INFINITY, f32::min)
        .floor()
        .max(0.0) as i32;
    let max_x = vertices
        .iter()
        .map(|vertex| vertex.x)
        .fold(f32::NEG_INFINITY, f32::max)
        .ceil()
        .min(framebuffer.width as f32 - 1.0) as i32;
    let min_y = vertices
        .iter()
        .map(|vertex| vertex.y)
        .fold(f32::INFINITY, f32::min)
        .floor()
        .max(0.0) as i32;
    let max_y = vertices
        .iter()
        .map(|vertex| vertex.y)
        .fold(f32::NEG_INFINITY, f32::max)
        .ceil()
        .min(framebuffer.height as f32 - 1.0) as i32;

    for y in min_y..=max_y {
        for x in min_x..=max_x {
            let px = x as f32 + 0.5;
            let py = y as f32 + 0.5;
            let weights = [
                edge(vertices[1], vertices[2], px, py) / area,
                edge(vertices[2], vertices[0], px, py) / area,
                edge(vertices[0], vertices[1], px, py) / area,
            ];
            if weights.iter().any(|weight| *weight < 0.0) {
                continue;
            }

            let depth = weights[0] * vertices[0].depth
                + weights[1] * vertices[1].depth
                + weights[2] * vertices[2].depth;
            let depth_index = y as usize * framebuffer.width as usize + x as usize;
            if depth > framebuffer.depth[depth_index] + 0.001 {
                continue;
            }

            let reciprocal = [
                weights[0] * vertices[0].inv_w,
                weights[1] * vertices[1].inv_w,
                weights[2] * vertices[2].inv_w,
            ];
            let denominator = reciprocal[0] + reciprocal[1] + reciprocal[2];
            if denominator.abs() <= f32::EPSILON {
                continue;
            }

            let uv = (uvs[0] * reciprocal[0]
                + uvs[1] * reciprocal[1]
                + uvs[2] * reciprocal[2])
                / denominator;
            let sample = sample_image_surface(image, uv, repeat);
            framebuffer.blend_pixel(
                x,
                y,
                [
                    (sample.x * tint.x.clamp(0.0, 1.0) * 255.0).round() as u8,
                    (sample.y * tint.y.clamp(0.0, 1.0) * 255.0).round() as u8,
                    (sample.z * tint.z.clamp(0.0, 1.0) * 255.0).round() as u8,
                    (sample.w * (1.0 - transparency) * 255.0).round() as u8,
                ],
            );
        }
    }
}

fn sample_image_surface(image: &ImageSurface, uv: Vec2, repeat: bool) -> Vec4 {
    if image.width == 0 || image.height == 0 || image.pixels.is_empty() {
        return Vec4::ONE;
    }

    let u = if repeat {
        uv.x.rem_euclid(1.0)
    } else {
        uv.x.clamp(0.0, 1.0)
    };
    let v = if repeat {
        uv.y.rem_euclid(1.0)
    } else {
        uv.y.clamp(0.0, 1.0)
    };
    let x = (u * image.width.saturating_sub(1) as f32).round() as usize;
    let y = (v * image.height.saturating_sub(1) as f32).round() as usize;
    let index = (y * image.width as usize + x) * 4;
    if index + 3 >= image.pixels.len() {
        return Vec4::ONE;
    }

    Vec4::new(
        image.pixels[index] as f32 / 255.0,
        image.pixels[index + 1] as f32 / 255.0,
        image.pixels[index + 2] as f32 / 255.0,
        image.pixels[index + 3] as f32 / 255.0,
    )
}


fn render_billboard(
    framebuffer: &mut Framebuffer,
    gui: Instance,
    camera: Instance,
    state: &RenderState,
) -> GuiStats {
    if !bool_prop(&gui, "Enabled", true) {
        return GuiStats::default();
    }

    let Some(adornee) = adornee_or_parent(gui) else {
        return GuiStats::default();
    };

    let camera_cf = cframe_prop(&camera, "CFrame", Mat4::IDENTITY);
    let mut position = world_position(adornee);
    position += vec3_prop(&gui, "StudsOffsetWorldSpace", Vec3::ZERO);
    position += vec3_prop(&gui, "StudsOffset", Vec3::ZERO);

    let distance = position.distance(camera_cf.w_axis.truncate());
    let max_distance = f32_prop(&gui, "MaxDistance", 0.0);
    if max_distance > 0.0 && distance > max_distance {
        return GuiStats::default();
    }

    let Some(projected) = project(framebuffer, camera, position) else {
        return GuiStats::default();
    };

    if !bool_prop(&gui, "AlwaysOnTop", false) && is_occluded(framebuffer, projected) {
        return GuiStats::default();
    }

    let fov = f32_prop(&camera, "FieldOfView", 70.0).to_radians();
    let pixels_per_stud =
        framebuffer.height as f32 / (2.0 * (fov * 0.5).tan() * distance.max(0.01));
    let size = udim2_prop(
        &gui,
        "Size",
        rbx_dom_weak::types::UDim2 {
            x: rbx_dom_weak::types::UDim {
                scale: 0.0,
                offset: 100,
            },
            y: rbx_dom_weak::types::UDim {
                scale: 0.0,
                offset: 100,
            },
        },
    );

    let width = (size.x.scale * pixels_per_stud + size.x.offset as f32).max(1.0);
    let height = (size.y.scale * pixels_per_stud + size.y.offset as f32).max(1.0);
    let anchor = vec2_prop(&gui, "AnchorPoint", Vec2::new(0.5, 0.5));

    render_gui_in_rect(
        framebuffer,
        gui,
        state,
        Rect {
            x: projected.x - width * anchor.x,
            y: projected.y - height * anchor.y,
            w: width,
            h: height,
        },
    )
}

fn render_surface(
    framebuffer: &mut Framebuffer,
    gui: Instance,
    camera: Instance,
    state: &RenderState,
) -> GuiStats {
    if !bool_prop(&gui, "Enabled", true) {
        return GuiStats::default();
    }

    let Some(adornee) = adornee_or_parent(gui) else {
        return GuiStats::default();
    };
    if !adornee.is_a("BasePart") {
        return GuiStats::default();
    }

    let part_size = vec3_prop(&adornee, "Size", Vec3::ONE);
    let face = enum_prop(&gui, "Face", 5);
    let canvas_size = vec2_prop(&gui, "CanvasSize", Vec2::new(800.0, 600.0));
    let pixels_per_stud = f32_prop(&gui, "PixelsPerStud", 50.0).max(1.0);
    let sizing_mode = enum_prop(&gui, "SizingMode", 0);
    let face_size = surface_face_size(part_size, face);

    let (width, height) = if sizing_mode == 1 {
        (
            (face_size.x * pixels_per_stud).round().clamp(1.0, 4096.0) as u32,
            (face_size.y * pixels_per_stud).round().clamp(1.0, 4096.0) as u32,
        )
    } else {
        (
            canvas_size.x.round().clamp(1.0, 4096.0) as u32,
            canvas_size.y.round().clamp(1.0, 4096.0) as u32,
        )
    };

    let mut canvas = Framebuffer::new(width, height);
    canvas.clear([0, 0, 0, 0]);
    let stats = render_gui_in_rect(
        &mut canvas,
        gui,
        state,
        Rect {
            x: 0.0,
            y: 0.0,
            w: width as f32,
            h: height as f32,
        },
    );

    let cframe = cframe_prop(&adornee, "CFrame", Mat4::IDENTITY);
    let corners = surface_face_corners(part_size, face)
        .map(|point| cframe.transform_point3(point));

    draw_surface_canvas(
        framebuffer,
        camera,
        corners,
        &canvas,
        bool_prop(&gui, "AlwaysOnTop", false),
    );

    stats
}

fn draw_surface_canvas(
    framebuffer: &mut Framebuffer,
    camera: Instance,
    corners: [Vec3; 4],
    canvas: &Framebuffer,
    always_on_top: bool,
) {
    let Some(a) = project(framebuffer, camera, corners[0]) else {
        return;
    };
    let Some(b) = project(framebuffer, camera, corners[1]) else {
        return;
    };
    let Some(c) = project(framebuffer, camera, corners[2]) else {
        return;
    };
    let Some(d) = project(framebuffer, camera, corners[3]) else {
        return;
    };

    draw_textured_triangle(
        framebuffer,
        [a, b, c],
        [Vec2::new(0.0, 0.0), Vec2::new(1.0, 0.0), Vec2::new(1.0, 1.0)],
        canvas,
        always_on_top,
    );
    draw_textured_triangle(
        framebuffer,
        [a, c, d],
        [Vec2::new(0.0, 0.0), Vec2::new(1.0, 1.0), Vec2::new(0.0, 1.0)],
        canvas,
        always_on_top,
    );
}

fn draw_textured_triangle(
    framebuffer: &mut Framebuffer,
    vertices: [Projected; 3],
    uvs: [Vec2; 3],
    texture: &Framebuffer,
    always_on_top: bool,
) {
    let area = edge(vertices[0], vertices[1], vertices[2].x, vertices[2].y);
    if area.abs() <= 0.0001 {
        return;
    }

    let min_x = vertices
        .iter()
        .map(|v| v.x)
        .fold(f32::INFINITY, f32::min)
        .floor()
        .max(0.0) as i32;
    let max_x = vertices
        .iter()
        .map(|v| v.x)
        .fold(f32::NEG_INFINITY, f32::max)
        .ceil()
        .min(framebuffer.width as f32 - 1.0) as i32;
    let min_y = vertices
        .iter()
        .map(|v| v.y)
        .fold(f32::INFINITY, f32::min)
        .floor()
        .max(0.0) as i32;
    let max_y = vertices
        .iter()
        .map(|v| v.y)
        .fold(f32::NEG_INFINITY, f32::max)
        .ceil()
        .min(framebuffer.height as f32 - 1.0) as i32;

    for y in min_y..=max_y {
        for x in min_x..=max_x {
            let px = x as f32 + 0.5;
            let py = y as f32 + 0.5;
            let w0 = edge(vertices[1], vertices[2], px, py) / area;
            let w1 = edge(vertices[2], vertices[0], px, py) / area;
            let w2 = edge(vertices[0], vertices[1], px, py) / area;
            if w0 < 0.0 || w1 < 0.0 || w2 < 0.0 {
                continue;
            }

            let depth =
                w0 * vertices[0].depth + w1 * vertices[1].depth + w2 * vertices[2].depth;
            let depth_index = y as usize * framebuffer.width as usize + x as usize;
            if !always_on_top && depth >= framebuffer.depth[depth_index] + 0.0005 {
                continue;
            }

            let denom =
                w0 * vertices[0].inv_w + w1 * vertices[1].inv_w + w2 * vertices[2].inv_w;
            if denom.abs() <= f32::EPSILON {
                continue;
            }
            let uv = (
                uvs[0] * (w0 * vertices[0].inv_w)
                    + uvs[1] * (w1 * vertices[1].inv_w)
                    + uvs[2] * (w2 * vertices[2].inv_w)
            ) / denom;

            let tx = (uv.x.clamp(0.0, 1.0) * (texture.width.saturating_sub(1)) as f32).round()
                as usize;
            let ty = (uv.y.clamp(0.0, 1.0) * (texture.height.saturating_sub(1)) as f32).round()
                as usize;
            let index = (ty * texture.width as usize + tx) * 4;
            if index + 3 >= texture.pixels.len() {
                continue;
            }

            framebuffer.blend_pixel(
                x,
                y,
                [
                    texture.pixels[index],
                    texture.pixels[index + 1],
                    texture.pixels[index + 2],
                    texture.pixels[index + 3],
                ],
            );
        }
    }
}

fn project(framebuffer: &Framebuffer, camera: Instance, point: Vec3) -> Option<Projected> {
    let camera_cf = cframe_prop(&camera, "CFrame", Mat4::IDENTITY);
    let fov = f32_prop(&camera, "FieldOfView", 70.0).clamp(1.0, 120.0);
    let aspect = framebuffer.width as f32 / framebuffer.height as f32;
    let projection = Mat4::perspective_rh(fov.to_radians(), aspect, 0.05, 10000.0);
    let clip: Vec4 = projection * camera_cf.inverse() * point.extend(1.0);
    if clip.w <= 0.0001 {
        return None;
    }

    let ndc = clip.truncate() / clip.w;
    if ndc.z < 0.0 || ndc.z > 1.0 {
        return None;
    }

    Some(Projected {
        x: (ndc.x * 0.5 + 0.5) * framebuffer.width as f32,
        y: (0.5 - ndc.y * 0.5) * framebuffer.height as f32,
        depth: ndc.z,
        inv_w: 1.0 / clip.w,
    })
}

fn is_occluded(framebuffer: &Framebuffer, projected: Projected) -> bool {
    let x = projected.x.round() as i32;
    let y = projected.y.round() as i32;
    if x < 0 || y < 0 || x >= framebuffer.width as i32 || y >= framebuffer.height as i32 {
        return true;
    }
    let index = y as usize * framebuffer.width as usize + x as usize;
    framebuffer.depth[index] + 0.0005 < projected.depth
}

fn adornee_or_parent(gui: Instance) -> Option<Instance> {
    ref_prop(&gui, "Adornee").or_else(|| {
        let parent = gui.get_parent()?;
        if parent.is_a("BasePart") || parent.get_class_name() == "Attachment" {
            Some(parent)
        } else {
            None
        }
    })
}

fn world_position(instance: Instance) -> Vec3 {
    if instance.get_class_name() == "Attachment" {
        if let Some(Variant::CFrame(value)) = instance.get_property("WorldCFrame") {
            return CFrame::from(value).0.w_axis.truncate();
        }
        if let Some(parent) = instance.get_parent()
            && parent.is_a("BasePart")
        {
            return (
                cframe_prop(&parent, "CFrame", Mat4::IDENTITY)
                    * cframe_prop(&instance, "CFrame", Mat4::IDENTITY)
            )
                .w_axis
                .truncate();
        }
    }
    cframe_prop(&instance, "CFrame", Mat4::IDENTITY)
        .w_axis
        .truncate()
}

fn surface_face_size(size: Vec3, face: u32) -> Vec2 {
    match face {
        0 | 3 => Vec2::new(size.z, size.y),
        1 | 4 => Vec2::new(size.x, size.z),
        _ => Vec2::new(size.x, size.y),
    }
}

fn surface_face_corners(size: Vec3, face: u32) -> [Vec3; 4] {
    let h = size * 0.5;
    match face {
        0 => [
            Vec3::new(h.x, h.y, -h.z),
            Vec3::new(h.x, h.y, h.z),
            Vec3::new(h.x, -h.y, h.z),
            Vec3::new(h.x, -h.y, -h.z),
        ],
        1 => [
            Vec3::new(-h.x, h.y, h.z),
            Vec3::new(h.x, h.y, h.z),
            Vec3::new(h.x, h.y, -h.z),
            Vec3::new(-h.x, h.y, -h.z),
        ],
        2 => [
            Vec3::new(h.x, h.y, h.z),
            Vec3::new(-h.x, h.y, h.z),
            Vec3::new(-h.x, -h.y, h.z),
            Vec3::new(h.x, -h.y, h.z),
        ],
        3 => [
            Vec3::new(-h.x, h.y, h.z),
            Vec3::new(-h.x, h.y, -h.z),
            Vec3::new(-h.x, -h.y, -h.z),
            Vec3::new(-h.x, -h.y, h.z),
        ],
        4 => [
            Vec3::new(-h.x, -h.y, -h.z),
            Vec3::new(h.x, -h.y, -h.z),
            Vec3::new(h.x, -h.y, h.z),
            Vec3::new(-h.x, -h.y, h.z),
        ],
        _ => [
            Vec3::new(-h.x, h.y, -h.z),
            Vec3::new(h.x, h.y, -h.z),
            Vec3::new(h.x, -h.y, -h.z),
            Vec3::new(-h.x, -h.y, -h.z),
        ],
    }
}

fn edge(a: Projected, b: Projected, x: f32, y: f32) -> f32 {
    (x - a.x) * (b.y - a.y) - (y - a.y) * (b.x - a.x)
}
