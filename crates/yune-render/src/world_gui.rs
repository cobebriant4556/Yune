use glam::{Mat4, Vec2, Vec3, Vec4};
use lune_roblox::{
    datatypes::types::CFrame,
    instance::Instance,
};
use rbx_dom_weak::types::Variant;

use crate::{
    RenderState,
    framebuffer::Framebuffer,
    gui::{GuiStats, Rect, render_gui_in_rect},
    props::{
        bool_prop, cframe_prop, enum_prop, f32_prop, ref_prop, udim2_prop, vec2_prop, vec3_prop,
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
