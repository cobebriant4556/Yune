use font8x8::{BASIC_FONTS, UnicodeFonts};
use fontdue::layout::{
    CoordinateSystem, HorizontalAlign, Layout, LayoutSettings, TextStyle, VerticalAlign, WrapStyle,
};
use glam::{Vec2, Vec3};
use lune_roblox::instance::Instance;
use rbx_dom_weak::types::{UDim, UDim2, Variant};

use crate::{
    ImageBinding, RenderState,
    framebuffer::Framebuffer,
    props::{
        bool_prop, color_prop, color_to_rgba, content_prop, enum_prop, f32_prop, i32_prop,
        instance_key, ref_prop, string_prop, udim2_prop, vec2_prop, vec3_prop,
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
pub(crate) struct Rect {
    pub(crate) x: f32,
    pub(crate) y: f32,
    pub(crate) w: f32,
    pub(crate) h: f32,
}

#[derive(Debug, Clone)]
struct DrawItem {
    instance: Instance,
    rect: Rect,
    z: i32,
    order: usize,
}

pub fn render_gui(framebuffer: &mut Framebuffer, root: Instance, state: &RenderState) -> GuiStats {
    let screen = Rect {
        x: 0.0,
        y: 0.0,
        w: framebuffer.width as f32,
        h: framebuffer.height as f32,
    };
    render_gui_in_rect(framebuffer, root, state, screen)
}

pub(crate) fn render_gui_in_rect(
    framebuffer: &mut Framebuffer,
    root: Instance,
    state: &RenderState,
    root_rect: Rect,
) -> GuiStats {
    if root.is_a("LayerCollector") && !bool_prop(&root, "Enabled", true) {
        return GuiStats::default();
    }

    let mut items = Vec::new();
    let mut order = 0;
    collect_gui_items(root, root_rect, &mut items, &mut order);
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
        draw_text(framebuffer, item, state);
    } else if class == "ImageLabel" || class == "ImageButton" {
        draw_image(framebuffer, item, state);
    } else if class == "ViewportFrame" {
        draw_viewport(framebuffer, item, state, stats);
    }
}

fn draw_text(framebuffer: &mut Framebuffer, item: &DrawItem, state: &RenderState) {
    let family = match item.instance.get_property("FontFace") {
        Some(Variant::Font(font)) => font.family,
        _ => String::new(),
    };

    if let Some(font) = state.font_asset(&family) {
        draw_vector_text(framebuffer, item, font.as_ref());
    } else {
        draw_bitmap_text(framebuffer, item);
    }
}

fn draw_vector_text(framebuffer: &mut Framebuffer, item: &DrawItem, font: &fontdue::Font) {
    let text = string_prop(&item.instance, "Text", "");
    if text.is_empty() {
        return;
    }

    let color = color_prop(&item.instance, "TextColor3", Vec3::ONE);
    let transparency = f32_prop(&item.instance, "TextTransparency", 0.0).clamp(0.0, 1.0);
    let wrapped = bool_prop(&item.instance, "TextWrapped", false);
    let scaled = bool_prop(&item.instance, "TextScaled", false);
    let x_alignment = enum_prop(&item.instance, "TextXAlignment", 2);
    let y_alignment = enum_prop(&item.instance, "TextYAlignment", 1);

    let px = if scaled {
        fit_vector_text(font, &text, item.rect, wrapped)
    } else {
        f32_prop(&item.instance, "TextSize", 14.0).clamp(1.0, 200.0)
    };

    let horizontal_align = match x_alignment {
        0 => HorizontalAlign::Left,
        1 => HorizontalAlign::Right,
        _ => HorizontalAlign::Center,
    };
    let vertical_align = match y_alignment {
        0 => VerticalAlign::Top,
        2 => VerticalAlign::Bottom,
        _ => VerticalAlign::Middle,
    };

    let mut layout = Layout::new(CoordinateSystem::PositiveYDown);
    layout.reset(&LayoutSettings {
        x: item.rect.x,
        y: item.rect.y,
        max_width: if wrapped { Some(item.rect.w.max(1.0)) } else { None },
        max_height: Some(item.rect.h.max(1.0)),
        horizontal_align,
        vertical_align,
        line_height: 1.0,
        wrap_style: WrapStyle::Word,
        wrap_hard_breaks: true,
    });
    layout.append(&[font], &TextStyle::new(&text, px, 0));

    let x_shift = if wrapped {
        0.0
    } else {
        let bounds = layout
            .glyphs()
            .iter()
            .fold(None::<(f32, f32)>, |bounds, glyph| {
                let min = glyph.x;
                let max = glyph.x + glyph.width as f32;
                Some(match bounds {
                    Some((old_min, old_max)) => (old_min.min(min), old_max.max(max)),
                    None => (min, max),
                })
            });
        let width = bounds.map_or(0.0, |(min, max)| max - min);
        match x_alignment {
            0 => item.rect.x,
            1 => item.rect.x + item.rect.w - width,
            _ => item.rect.x + (item.rect.w - width) * 0.5,
        }
    };

    for glyph in layout.glyphs() {
        let (_, bitmap) = font.rasterize_config(glyph.key);
        if glyph.width == 0 || glyph.height == 0 {
            continue;
        }

        let origin_x = if wrapped {
            glyph.x.round() as i32
        } else {
            (glyph.x + x_shift).round() as i32
        };
        let origin_y = glyph.y.round() as i32;

        for gy in 0..glyph.height {
            for gx in 0..glyph.width {
                let coverage = bitmap[gy * glyph.width + gx] as f32 / 255.0;
                if coverage <= f32::EPSILON {
                    continue;
                }
                framebuffer.blend_pixel(
                    origin_x + gx as i32,
                    origin_y + gy as i32,
                    [
                        (color.x.clamp(0.0, 1.0) * 255.0).round() as u8,
                        (color.y.clamp(0.0, 1.0) * 255.0).round() as u8,
                        (color.z.clamp(0.0, 1.0) * 255.0).round() as u8,
                        (coverage * (1.0 - transparency) * 255.0).round() as u8,
                    ],
                );
            }
        }
    }
}

fn fit_vector_text(font: &fontdue::Font, text: &str, rect: Rect, wrapped: bool) -> f32 {
    let mut low = 1.0;
    let mut high = rect.h.max(1.0).min(200.0);

    for _ in 0..8 {
        let mid = (low + high) * 0.5;
        let mut layout = Layout::new(CoordinateSystem::PositiveYDown);
        layout.reset(&LayoutSettings {
            max_width: if wrapped { Some(rect.w.max(1.0)) } else { None },
            max_height: None,
            ..LayoutSettings::default()
        });
        layout.append(&[font], &TextStyle::new(text, mid, 0));

        let width = layout
            .glyphs()
            .iter()
            .map(|glyph| glyph.x + glyph.width as f32)
            .fold(0.0, f32::max);
        let fits_width = wrapped || width <= rect.w.max(1.0);
        let fits_height = layout.height() <= rect.h.max(1.0);
        if fits_width && fits_height {
            low = mid;
        } else {
            high = mid;
        }
    }

    low.max(1.0)
}

fn draw_bitmap_text(framebuffer: &mut Framebuffer, item: &DrawItem) {
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
    let binding = state.image_binding(&key).or_else(|| {
        content_prop(&item.instance, "Image")
            .and_then(|content_id| state.image_asset(&content_id))
    });
    let Some(binding) = binding else { return };

    let tint = color_prop(&item.instance, "ImageColor3", Vec3::ONE);
    let transparency = f32_prop(&item.instance, "ImageTransparency", 0.0).clamp(0.0, 1.0);
    let rect_offset = vec2_prop(&item.instance, "ImageRectOffset", Vec2::ZERO);
    let rect_size = vec2_prop(&item.instance, "ImageRectSize", Vec2::ZERO);
    let scale_type = enum_prop(&item.instance, "ScaleType", 0);
    let pixelated = enum_prop(&item.instance, "ResampleMode", 0) == 1;

    match binding {
        ImageBinding::Pixels { width, height, pixels } => draw_image_pixels(
            framebuffer,
            item.rect,
            width,
            height,
            &pixels,
            tint,
            transparency,
            rect_offset,
            rect_size,
            scale_type,
            pixelated,
        ),
        ImageBinding::Editable(image) => {
            let image = image.snapshot();
            draw_image_pixels(
                framebuffer,
                item.rect,
                image.width,
                image.height,
                &image.pixels,
                tint,
                transparency,
                rect_offset,
                rect_size,
                scale_type,
                pixelated,
            );
        }
    }
}

fn draw_image_pixels(
    framebuffer: &mut Framebuffer,
    rect: Rect,
    src_width: u32,
    src_height: u32,
    pixels: &[u8],
    tint: Vec3,
    transparency: f32,
    rect_offset: Vec2,
    rect_size: Vec2,
    scale_type: u32,
    pixelated: bool,
) {
    if src_width == 0 || src_height == 0 || rect.w <= 0.0 || rect.h <= 0.0 {
        return;
    }

    let sx0 = rect_offset.x.max(0.0).min(src_width as f32);
    let sy0 = rect_offset.y.max(0.0).min(src_height as f32);
    let mut sw = if rect_size.x > 0.0 { rect_size.x } else { src_width as f32 - sx0 };
    let mut sh = if rect_size.y > 0.0 { rect_size.y } else { src_height as f32 - sy0 };
    sw = sw.max(1.0).min(src_width as f32 - sx0);
    sh = sh.max(1.0).min(src_height as f32 - sy0);

    let mut draw_rect = rect;
    let source_aspect = sw / sh;
    let target_aspect = rect.w / rect.h.max(0.0001);

    if scale_type == 3 {
        if target_aspect > source_aspect {
            draw_rect.w = rect.h * source_aspect;
            draw_rect.x = rect.x + (rect.w - draw_rect.w) * 0.5;
        } else {
            draw_rect.h = rect.w / source_aspect;
            draw_rect.y = rect.y + (rect.h - draw_rect.h) * 0.5;
        }
    }

    let mut crop_x = sx0;
    let mut crop_y = sy0;
    let mut crop_w = sw;
    let mut crop_h = sh;
    if scale_type == 4 {
        if target_aspect > source_aspect {
            crop_h = sw / target_aspect;
            crop_y = sy0 + (sh - crop_h) * 0.5;
        } else {
            crop_w = sh * target_aspect;
            crop_x = sx0 + (sw - crop_w) * 0.5;
        }
    }

    let dst_w = draw_rect.w.round().max(1.0) as i32;
    let dst_h = draw_rect.h.round().max(1.0) as i32;
    for dy in 0..dst_h {
        for dx in 0..dst_w {
            let (u, v) = if scale_type == 2 {
                (
                    (dx as f32 / crop_w.max(1.0)).fract(),
                    (dy as f32 / crop_h.max(1.0)).fract(),
                )
            } else {
                (
                    (dx as f32 + 0.5) / dst_w as f32,
                    (dy as f32 + 0.5) / dst_h as f32,
                )
            };
            let sx = crop_x + u * crop_w;
            let sy = crop_y + v * crop_h;
            let sample = sample_image(src_width, src_height, pixels, sx, sy, pixelated);
            let rgba = [
                (sample[0] * tint.x.clamp(0.0, 1.0) * 255.0).round().clamp(0.0, 255.0) as u8,
                (sample[1] * tint.y.clamp(0.0, 1.0) * 255.0).round().clamp(0.0, 255.0) as u8,
                (sample[2] * tint.z.clamp(0.0, 1.0) * 255.0).round().clamp(0.0, 255.0) as u8,
                (sample[3] * (1.0 - transparency) * 255.0).round().clamp(0.0, 255.0) as u8,
            ];
            framebuffer.blend_pixel(
                draw_rect.x.round() as i32 + dx,
                draw_rect.y.round() as i32 + dy,
                rgba,
            );
        }
    }
}

fn sample_image(
    width: u32,
    height: u32,
    pixels: &[u8],
    x: f32,
    y: f32,
    pixelated: bool,
) -> [f32; 4] {
    if pixelated {
        return pixel_at(
            width,
            height,
            pixels,
            x.floor() as i32,
            y.floor() as i32,
        );
    }

    let x = x - 0.5;
    let y = y - 0.5;
    let x0 = x.floor() as i32;
    let y0 = y.floor() as i32;
    let tx = x - x0 as f32;
    let ty = y - y0 as f32;
    let a = pixel_at(width, height, pixels, x0, y0);
    let b = pixel_at(width, height, pixels, x0 + 1, y0);
    let c = pixel_at(width, height, pixels, x0, y0 + 1);
    let d = pixel_at(width, height, pixels, x0 + 1, y0 + 1);
    let mut out = [0.0; 4];
    for channel in 0..4 {
        let top = a[channel] + (b[channel] - a[channel]) * tx;
        let bottom = c[channel] + (d[channel] - c[channel]) * tx;
        out[channel] = top + (bottom - top) * ty;
    }
    out
}

fn pixel_at(width: u32, height: u32, pixels: &[u8], x: i32, y: i32) -> [f32; 4] {
    let x = x.clamp(0, width as i32 - 1) as usize;
    let y = y.clamp(0, height as i32 - 1) as usize;
    let index = (y * width as usize + x) * 4;
    if index + 3 >= pixels.len() {
        return [0.0; 4];
    }
    [
        pixels[index] as f32 / 255.0,
        pixels[index + 1] as f32 / 255.0,
        pixels[index + 2] as f32 / 255.0,
        pixels[index + 3] as f32 / 255.0,
    ]
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
