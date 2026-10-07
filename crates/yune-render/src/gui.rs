use font8x8::{BASIC_FONTS, UnicodeFonts};
use glam::{Vec2, Vec3};
use lune_roblox::instance::Instance;
use rbx_dom_weak::types::{UDim, UDim2};

use crate::{
    ImageBinding, RenderState,
    framebuffer::Framebuffer,
    props::{
        bool_prop, color_prop, color_to_rgba, f32_prop, i32_prop, instance_key, ref_prop,
        string_prop, udim2_prop, vec2_prop, vec3_prop,
    },
    raster::{Lighting, RenderStats, render_world},
};

#[derive(Debug, Clone, Copy, Default)]
pub struct GuiStats {
    pub objects: u64,
    pub viewport_parts: u64,
    pub viewport_triangles: u64,
}

#[derive(Debug, Clone, Copy)]
struct Rect {
    x: f32,
    y: f32,
    w: f32,
    h: f32,
}

#[derive(Debug, Clone)]
struct DrawItem {
    instance: Instance,
    rect: Rect,
    z: i32,
    order: usize,
}

pub fn render_gui(framebuffer: &mut Framebuffer, root: Instance, state: &RenderState) -> GuiStats {
    if root.is_a("LayerCollector") && !bool_prop(&root, "Enabled", true) {
        return GuiStats::default();
    }
    let screen = Rect {
        x: 0.0,
        y: 0.0,
        w: framebuffer.width as f32,
        h: framebuffer.height as f32,
    };
    let mut items = Vec::new();
    let mut order = 0;
    collect_gui_items(root, screen, &mut items, &mut order);
    items.sort_by_key(|item| (item.z, item.order));

    let mut stats = GuiStats::default();
    for item in items {
        stats.objects += 1;
        draw_item(framebuffer, &item, state, &mut stats);
    }
    stats
}

fn collect_gui_items(root: Instance, parent_rect: Rect, items: &mut Vec<DrawItem>, order: &mut usize) {
    for child in root.get_children() {
        if child.is_a("GuiObject") {
            if !bool_prop(&child, "Visible", true) {
                continue;
            }
            let position = udim2_prop(&child, "Position", udim2(0.0, 0, 0.0, 0));
            let size = udim2_prop(&child, "Size", udim2(0.0, 100, 0.0, 100));
            let anchor = vec2_prop(&child, "AnchorPoint", Vec2::ZERO);
            let w = resolve_udim(size.x, parent_rect.w).max(0.0);
            let h = resolve_udim(size.y, parent_rect.h).max(0.0);
            let x = parent_rect.x + resolve_udim(position.x, parent_rect.w) - anchor.x * w;
            let y = parent_rect.y + resolve_udim(position.y, parent_rect.h) - anchor.y * h;
            let rect = Rect { x, y, w, h };
            let z = i32_prop(&child, "ZIndex", 1);
            items.push(DrawItem {
                instance: child,
                rect,
                z,
                order: *order,
            });
            *order += 1;
            collect_gui_items(child, rect, items, order);
        } else if child.is_a("LayerCollector") {
            collect_gui_items(child, parent_rect, items, order);
        }
    }
}

fn draw_item(framebuffer: &mut Framebuffer, item: &DrawItem, state: &RenderState, stats: &mut GuiStats) {
    let class = item.instance.get_class_name();
    let background = color_prop(&item.instance, "BackgroundColor3", Vec3::ONE);
    let background_transparency = f32_prop(&item.instance, "BackgroundTransparency", 0.0);
    if background_transparency < 1.0 {
        fill_rect(framebuffer, item.rect, color_to_rgba(background, background_transparency));
    }

    let border_size = i32_prop(&item.instance, "BorderSizePixel", 0).max(0);
    if border_size > 0 {
        let border = color_prop(&item.instance, "BorderColor3", Vec3::ZERO);
        stroke_rect(framebuffer, item.rect, border_size, color_to_rgba(border, 0.0));
    }

    if class == "TextLabel" || class == "TextButton" || class == "TextBox" {
        draw_text(framebuffer, item);
    } else if class == "ImageLabel" || class == "ImageButton" {
        draw_image(framebuffer, item, state);
    } else if class == "ViewportFrame" {
        draw_viewport(framebuffer, item, state, stats);
    }
}

fn draw_text(framebuffer: &mut Framebuffer, item: &DrawItem) {
    let text = string_prop(&item.instance, "Text", "");
    if text.is_empty() {
        return;
    }
    let color = color_prop(&item.instance, "TextColor3", Vec3::ONE);
    let transparency = f32_prop(&item.instance, "TextTransparency", 0.0);
    let rgba = color_to_rgba(color, transparency);
    let text_size = f32_prop(&item.instance, "TextSize", 14.0).max(1.0);
    let text_scaled = bool_prop(&item.instance, "TextScaled", false);
    let lines = text.lines().collect::<Vec<_>>();
    let longest = lines.iter().map(|line| line.chars().count()).max().unwrap_or(1).max(1);
    let line_count = lines.len().max(1);
    let scale = if text_scaled {
        let sx = (item.rect.w / (longest as f32 * 8.0)).floor();
        let sy = (item.rect.h / (line_count as f32 * 8.0)).floor();
        sx.min(sy).max(1.0) as i32
    } else {
        (text_size / 8.0).round().max(1.0) as i32
    };
    let line_height = 8 * scale;
    let total_h = line_height * line_count as i32;
    let start_y = item.rect.y.round() as i32 + ((item.rect.h.round() as i32 - total_h) / 2);

    for (line_index, line) in lines.iter().enumerate() {
        let width = line.chars().count() as i32 * 8 * scale;
        let mut x = item.rect.x.round() as i32 + ((item.rect.w.round() as i32 - width) / 2);
        let y = start_y + line_index as i32 * line_height;
        for ch in line.chars() {
            if let Some(glyph) = BASIC_FONTS.get(ch) {
                for (gy, row) in glyph.iter().enumerate() {
                    for gx in 0..8 {
                        if row & (1 << gx) == 0 {
                            continue;
                        }
                        for sy in 0..scale {
                            for sx in 0..scale {
                                framebuffer.blend_pixel(
                                    x + (7 - gx) * scale + sx,
                                    y + gy as i32 * scale + sy,
                                    rgba,
                                );
                            }
                        }
                    }
                }
            }
            x += 8 * scale;
        }
    }
}

fn draw_image(framebuffer: &mut Framebuffer, item: &DrawItem, state: &RenderState) {
    let key = instance_key(&item.instance);
    let Some(binding) = state.image_binding(&key) else { return };
    let tint = color_prop(&item.instance, "ImageColor3", Vec3::ONE);
    let transparency = f32_prop(&item.instance, "ImageTransparency", 0.0).clamp(0.0, 1.0);
    match binding {
        ImageBinding::Pixels { width, height, pixels } => {
            blit_scaled(framebuffer, item.rect, width, height, &pixels, tint, transparency);
        }
        ImageBinding::Editable(image) => {
            let image = image.snapshot();
            blit_scaled(framebuffer, item.rect, image.width, image.height, &image.pixels, tint, transparency);
        }
    }
}

fn draw_viewport(framebuffer: &mut Framebuffer, item: &DrawItem, state: &RenderState, stats: &mut GuiStats) {
    let width = item.rect.w.round().max(1.0) as u32;
    let height = item.rect.h.round().max(1.0) as u32;
    let Some(camera) = ref_prop(&item.instance, "CurrentCamera") else { return };
    let mut target = Framebuffer::new(width, height);
    target.clear([0, 0, 0, 0]);
    let lighting = Lighting {
        ambient: color_prop(&item.instance, "Ambient", Vec3::new(0.5, 0.5, 0.5)),
        outdoor_ambient: color_prop(
            &item.instance,
            "Ambient",
            Vec3::new(0.5, 0.5, 0.5),
        ),
        light_color: color_prop(&item.instance, "LightColor", Vec3::ONE),
        light_direction: vec3_prop(
            &item.instance,
            "LightDirection",
            Vec3::new(-1.0, -1.0, -1.0),
        )
        .normalize_or_zero(),
        brightness: 1.0,
        ..Lighting::default()
    };
    let RenderStats { parts, triangles } = render_world(&mut target, item.instance, camera, state, lighting);
    stats.viewport_parts += parts;
    stats.viewport_triangles += triangles;
    framebuffer.composite(&target, item.rect.x.round() as i32, item.rect.y.round() as i32);
}

fn blit_scaled(
    framebuffer: &mut Framebuffer,
    rect: Rect,
    src_width: u32,
    src_height: u32,
    pixels: &[u8],
    tint: Vec3,
    transparency: f32,
) {
    if src_width == 0 || src_height == 0 || rect.w <= 0.0 || rect.h <= 0.0 {
        return;
    }
    let dst_w = rect.w.round().max(1.0) as i32;
    let dst_h = rect.h.round().max(1.0) as i32;
    for dy in 0..dst_h {
        let sy = ((dy as f32 / dst_h as f32) * src_height as f32).floor().min(src_height as f32 - 1.0) as u32;
        for dx in 0..dst_w {
            let sx = ((dx as f32 / dst_w as f32) * src_width as f32).floor().min(src_width as f32 - 1.0) as u32;
            let i = (sy as usize * src_width as usize + sx as usize) * 4;
            if i + 3 >= pixels.len() {
                continue;
            }
            let rgba = [
                ((pixels[i] as f32 * tint.x.clamp(0.0, 1.0)).round()).clamp(0.0, 255.0) as u8,
                ((pixels[i + 1] as f32 * tint.y.clamp(0.0, 1.0)).round()).clamp(0.0, 255.0) as u8,
                ((pixels[i + 2] as f32 * tint.z.clamp(0.0, 1.0)).round()).clamp(0.0, 255.0) as u8,
                ((pixels[i + 3] as f32 * (1.0 - transparency)).round()).clamp(0.0, 255.0) as u8,
            ];
            framebuffer.blend_pixel(rect.x.round() as i32 + dx, rect.y.round() as i32 + dy, rgba);
        }
    }
}

fn fill_rect(framebuffer: &mut Framebuffer, rect: Rect, color: [u8; 4]) {
    let x0 = rect.x.floor() as i32;
    let y0 = rect.y.floor() as i32;
    let x1 = (rect.x + rect.w).ceil() as i32;
    let y1 = (rect.y + rect.h).ceil() as i32;
    for y in y0..y1 {
        for x in x0..x1 {
            framebuffer.blend_pixel(x, y, color);
        }
    }
}

fn stroke_rect(framebuffer: &mut Framebuffer, rect: Rect, thickness: i32, color: [u8; 4]) {
    let t = thickness.max(1) as f32;
    fill_rect(framebuffer, Rect { x: rect.x, y: rect.y, w: rect.w, h: t }, color);
    fill_rect(framebuffer, Rect { x: rect.x, y: rect.y + rect.h - t, w: rect.w, h: t }, color);
    fill_rect(framebuffer, Rect { x: rect.x, y: rect.y, w: t, h: rect.h }, color);
    fill_rect(framebuffer, Rect { x: rect.x + rect.w - t, y: rect.y, w: t, h: rect.h }, color);
}

fn resolve_udim(value: UDim, parent: f32) -> f32 {
    value.scale * parent + value.offset as f32
}

fn udim2(xs: f32, xo: i32, ys: f32, yo: i32) -> UDim2 {
    UDim2 {
        x: UDim { scale: xs, offset: xo },
        y: UDim { scale: ys, offset: yo },
    }
}
