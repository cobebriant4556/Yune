use font8x8::{BASIC_FONTS, UnicodeFonts};
use fontdue::layout::{CoordinateSystem, HorizontalAlign, Layout, LayoutSettings, TextStyle, VerticalAlign};
use glam::{Vec2, Vec3, Vec4};
use lune_roblox::instance::Instance;
use rbx_dom_weak::types::{UDim, UDim2, Variant, Vector2 as DomVector2};
use crate::{ImageBinding, RenderState, framebuffer::Framebuffer,
    props::{bool_prop, color_prop, color_to_rgba, content_prop, enum_prop, f32_prop, i32_prop, instance_key, ref_prop, string_prop, udim2_prop, vec2_prop, vec3_prop},
    raster::{Lighting, render_world}};

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

impl Rect {
    fn intersect(self, other: Self) -> Self {
        let x = self.x.max(other.x);
        let y = self.y.max(other.y);
        Self { x, y, w: (self.x + self.w).min(other.x + other.w).sub(x).max(0.0), h: (self.y + self.h).min(other.y + other.h).sub(y).max(0.0) }
    }
    fn valid(self) -> bool { [self.x, self.y, self.w, self.h].iter().all(|value| value.is_finite()) && self.w > 0.0 && self.h > 0.0 }
    fn contains(self, x: i32, y: i32) -> bool {
        let x = x as f32 + 0.5;
        let y = y as f32 + 0.5;
        x >= self.x && y >= self.y && x < self.x + self.w && y < self.y + self.h
    }
    fn bounds(self, framebuffer: &Framebuffer) -> (i32, i32, i32, i32) {
        (self.x.floor().max(0.0) as i32, self.y.floor().max(0.0) as i32,
            (self.x + self.w).ceil().min(framebuffer.width as f32) as i32,
            (self.y + self.h).ceil().min(framebuffer.height as f32) as i32)
    }
}

use std::ops::Sub;

#[derive(Clone, Copy)]
struct DrawItem { instance: Instance, rect: Rect, clip: Rect, scale: f32, z: i32, order: usize }

fn screen_rect(framebuffer: &Framebuffer) -> Rect {
    Rect { x: 0.0, y: 0.0, w: framebuffer.width as f32, h: framebuffer.height as f32 }
}

pub fn render_gui(framebuffer: &mut Framebuffer, root: Instance, state: &RenderState) -> GuiStats {
    let screen = screen_rect(framebuffer);
    if root.is_a("LayerCollector") || root.is_a("GuiObject") {
        return render_gui_in_rect(framebuffer, root, state, screen);
    }
    let mut roots = root.get_descendants_preorder().into_iter()
        .filter(|instance| matches!(instance.get_class_name(), "ScreenGui" | "GuiMain")).collect::<Vec<_>>();
    roots.sort_by_key(|root| i32_prop(root, "DisplayOrder", 0));
    let mut total = GuiStats::default();
    for root in roots {
        let stats = render_gui_in_rect(framebuffer, root, state, screen);
        total.objects += stats.objects;
        total.viewport_parts += stats.viewport_parts;
        total.viewport_triangles += stats.viewport_triangles;
    }
    total
}

pub(crate) fn render_gui_in_rect(framebuffer: &mut Framebuffer, root: Instance, state: &RenderState, root_rect: Rect) -> GuiStats {
    if !root_rect.valid() || (root.is_a("LayerCollector") && !bool_prop(&root, "Enabled", true)) { return GuiStats::default(); }
    let mut clip = screen_rect(framebuffer);
    if bool_prop(&root, "ClipsDescendants", false) { clip = clip.intersect(root_rect); }
    let sibling = enum_prop(&root, "ZIndexBehavior", 1) == 1;
    let mut items = Vec::new();
    collect_items(root, root_rect, 1.0, clip, sibling, 0, &mut items);
    if !sibling { items.sort_by_key(|item| (item.z, item.order)); }
    let mut stats = GuiStats::default();
    for item in items {
        stats.objects += 1;
        draw_item(framebuffer, item, state, &mut stats);
    }
    stats
}

fn collect_items(root: Instance, parent: Rect, inherited_scale: f32, clip: Rect, sibling: bool, depth: u32, items: &mut Vec<DrawItem>) {
    if depth > 256 || !clip.valid() { return; }
    let mut children = root.get_children().into_iter().filter(|child| child.is_a("GuiObject")).collect::<Vec<_>>();
    if sibling { children.sort_by_key(|child| i32_prop(child, "ZIndex", 1)); }
    for child in children {
        if !bool_prop(&child, "Visible", true) { continue; }
        let own_scale = child.get_children().into_iter().find(|item| item.get_class_name() == "UIScale")
            .map_or(1.0, |item| f32_prop(&item, "Scale", 1.0));
        let scale = inherited_scale * own_scale;
        if !scale.is_finite() || scale <= 0.0 { continue; }
        let position = udim2_prop(&child, "Position", udim2(0.0, 0, 0.0, 0));
        let size = udim2_prop(&child, "Size", udim2(0.0, 100, 0.0, 100));
        let anchor = vec2_prop(&child, "AnchorPoint", Vec2::ZERO);
        let w = (resolve(size.x, parent.w, inherited_scale) * own_scale).round().max(0.0);
        let h = (resolve(size.y, parent.h, inherited_scale) * own_scale).round().max(0.0);
        let x = (parent.x + resolve(position.x, parent.w, inherited_scale) - anchor.x * w).round();
        let y = (parent.y + resolve(position.y, parent.h, inherited_scale) - anchor.y * h).round();
        let rect = Rect { x, y, w, h };
        if !rect.valid() { continue; }
        child.set_property("AbsolutePosition", Variant::Vector2(DomVector2 { x, y }));
        child.set_property("AbsoluteSize", Variant::Vector2(DomVector2 { x: w, y: h }));
        items.push(DrawItem { instance: child, rect, clip, scale, z: i32_prop(&child, "ZIndex", 1), order: items.len() });
        let child_clip = if bool_prop(&child, "ClipsDescendants", false) { clip.intersect(rect) } else { clip };
        collect_items(child, rect, scale, child_clip, sibling, depth + 1, items);
    }
}

fn put(framebuffer: &mut Framebuffer, clip: Rect, x: i32, y: i32, color: [u8; 4]) {
    if clip.contains(x, y) { framebuffer.blend_pixel(x, y, color); }
}

fn fill(framebuffer: &mut Framebuffer, rect: Rect, clip: Rect, color: [u8; 4]) {
    let rect = rect.intersect(clip);
    if !rect.valid() || color[3] == 0 { return; }
    let (x0, y0, x1, y1) = rect.bounds(framebuffer);
    for y in y0..y1 { for x in x0..x1 { if rect.contains(x, y) { framebuffer.blend_pixel(x, y, color); } } }
}

fn draw_item(framebuffer: &mut Framebuffer, item: DrawItem, state: &RenderState, stats: &mut GuiStats) {
    let background = color_prop(&item.instance, "BackgroundColor3", Vec3::ONE);
    fill(framebuffer, item.rect, item.clip, color_to_rgba(background, f32_prop(&item.instance, "BackgroundTransparency", 0.0)));
    let border = (i32_prop(&item.instance, "BorderSizePixel", 0).max(0) as f32 * item.scale).round();
    if border > 0.0 {
        let color = color_to_rgba(color_prop(&item.instance, "BorderColor3", Vec3::ZERO), 0.0);
        let r = item.rect;
        for edge in [Rect { h: border, ..r }, Rect { y: r.y + r.h - border, h: border, ..r },
            Rect { w: border, ..r }, Rect { x: r.x + r.w - border, w: border, ..r }] {
            fill(framebuffer, edge, item.clip, color);
        }
    }
    match item.instance.get_class_name() {
        "TextLabel" | "TextButton" | "TextBox" => draw_text(framebuffer, item, state),
        "ImageLabel" | "ImageButton" => draw_image(framebuffer, item, state),
        "ViewportFrame" => draw_viewport(framebuffer, item, state, stats),
        _ => {}
    }
}

fn draw_text(framebuffer: &mut Framebuffer, item: DrawItem, state: &RenderState) {
    let text = string_prop(&item.instance, "Text", "");
    if text.is_empty() { return; }
    let family = match item.instance.get_property("FontFace") { Some(Variant::Font(font)) => font.family, _ => String::new() };
    let rgba = color_to_rgba(color_prop(&item.instance, "TextColor3", Vec3::ONE), f32_prop(&item.instance, "TextTransparency", 0.0));
    let x_align = enum_prop(&item.instance, "TextXAlignment", 2);
    let y_align = enum_prop(&item.instance, "TextYAlignment", 1);
    if let Some(font) = state.font_asset(&family) {
        let wrapped = bool_prop(&item.instance, "TextWrapped", false);
        let mut px = (f32_prop(&item.instance, "TextSize", 14.0) * item.scale).clamp(1.0, 1024.0);
        if bool_prop(&item.instance, "TextScaled", false) {
            let mut low = 1.0;
            let mut high = item.rect.h.min(1024.0).max(1.0);
            for _ in 0..10 {
                let size = (low + high) * 0.5;
                let mut probe = Layout::new(CoordinateSystem::PositiveYDown);
                probe.reset(&LayoutSettings { max_width: wrapped.then_some(item.rect.w), ..LayoutSettings::default() });
                probe.append(&[font.as_ref()], &TextStyle::new(&text, size, 0));
                let width = probe.glyphs().iter().map(|glyph| glyph.x + glyph.width as f32).fold(0.0, f32::max);
                if probe.height() <= item.rect.h && width <= item.rect.w { low = size; } else { high = size; }
            }
            px = low;
        }
        let mut layout = Layout::new(CoordinateSystem::PositiveYDown);
        layout.reset(&LayoutSettings {
            x: item.rect.x, y: item.rect.y, max_width: wrapped.then_some(item.rect.w), max_height: Some(item.rect.h),
            horizontal_align: match x_align { 0 => HorizontalAlign::Left, 1 => HorizontalAlign::Right, _ => HorizontalAlign::Center },
            vertical_align: match y_align { 0 => VerticalAlign::Top, 2 => VerticalAlign::Bottom, _ => VerticalAlign::Middle },
            line_height: f32_prop(&item.instance, "LineHeight", 1.0).clamp(0.1, 10.0),
            ..LayoutSettings::default()
        });
        layout.append(&[font.as_ref()], &TextStyle::new(&text, px, 0));
        let min_x = layout.glyphs().iter().map(|glyph| glyph.x).fold(item.rect.x, f32::min);
        let max_x = layout.glyphs().iter().map(|glyph| glyph.x + glyph.width as f32).fold(item.rect.x, f32::max);
        let shift = if wrapped { 0.0 } else { align_offset(item.rect.w, max_x - min_x, x_align, 2) + item.rect.x - min_x };
        for glyph in layout.glyphs() {
            let (metrics, bitmap) = font.rasterize_config(glyph.key);
            let x0 = (glyph.x + shift).round() as i32;
            let y0 = glyph.y.round() as i32;
            for y in 0..metrics.height { for x in 0..metrics.width {
                let mut color = rgba;
                color[3] = ((rgba[3] as u16 * bitmap[y * metrics.width + x] as u16 + 127) / 255) as u8;
                if color[3] != 0 { put(framebuffer, item.clip, x0 + x as i32, y0 + y as i32, color); }
            } }
        }
    } else {
        let lines = text.split('\n').collect::<Vec<_>>();
        let longest = lines.iter().map(|line| line.chars().count()).max().unwrap_or(1).max(1);
        let scale = if bool_prop(&item.instance, "TextScaled", false) {
            (item.rect.w / (longest as f32 * 8.0)).min(item.rect.h / (lines.len().max(1) as f32 * 8.0)).floor().max(1.0) as i32
        } else { (f32_prop(&item.instance, "TextSize", 14.0) * item.scale / 8.0).round().clamp(1.0, 128.0) as i32 };
        let y0 = item.rect.y + align_offset(item.rect.h, (lines.len() as i32 * 8 * scale) as f32, y_align, 1);
        for (index, line) in lines.iter().enumerate() {
            let mut x0 = item.rect.x + align_offset(item.rect.w, (line.chars().count() as i32 * 8 * scale) as f32, x_align, 2);
            for ch in line.chars() {
                if let Some(glyph) = BASIC_FONTS.get(ch) {
                    for (y, row) in glyph.iter().enumerate() { for x in 0..8 {
                        if row & (1 << x) != 0 {
                            fill(framebuffer, Rect { x: x0 + (x * scale) as f32, y: y0 + ((index as i32 * 8 + y as i32) * scale) as f32, w: scale as f32, h: scale as f32 }, item.clip, rgba);
                        }
                    } }
                }
                x0 += (8 * scale) as f32;
            }
        }
    }
}

fn align_offset(container: f32, content: f32, alignment: u32, center: u32) -> f32 {
    if alignment == 0 { 0.0 } else if alignment == center { (container - content) * 0.5 } else { container - content }
}

fn draw_image(framebuffer: &mut Framebuffer, item: DrawItem, state: &RenderState) {
    let binding = state.image_binding(&instance_key(&item.instance)).or_else(|| content_prop(&item.instance, "Image").and_then(|id| state.image_asset(&id)));
    let Some(binding) = binding else { return };
    match binding {
        ImageBinding::Pixels { width, height, pixels } => draw_image_pixels(framebuffer, item, width, height, &pixels),
        ImageBinding::Editable(image) => { let image = image.snapshot(); draw_image_pixels(framebuffer, item, image.width, image.height, &image.pixels); }
    }
}

fn draw_image_pixels(framebuffer: &mut Framebuffer, item: DrawItem, width: u32, height: u32, pixels: &[u8]) {
    if width == 0 || height == 0 || pixels.len() < width as usize * height as usize * 4 { return; }
    let offset = vec2_prop(&item.instance, "ImageRectOffset", Vec2::ZERO);
    let size = vec2_prop(&item.instance, "ImageRectSize", Vec2::ZERO);
    if !offset.is_finite() || !size.is_finite() { return; }
    let source = Rect { x: offset.x.floor(), y: offset.y.floor(), w: if size.x == 0.0 { width as f32 } else { size.x.floor() }, h: if size.y == 0.0 { height as f32 } else { size.y.floor() } }
        .intersect(Rect { x: 0.0, y: 0.0, w: width as f32, h: height as f32 });
    if !source.valid() { return; }
    let mode = enum_prop(&item.instance, "ScaleType", 0);
    let pixelated = enum_prop(&item.instance, "ResampleMode", 0) == 1;
    let tint = color_prop(&item.instance, "ImageColor3", Vec3::ONE).clamp(Vec3::ZERO, Vec3::ONE);
    let alpha = 1.0 - f32_prop(&item.instance, "ImageTransparency", 0.0).clamp(0.0, 1.0);
    let mut dest = item.rect;
    let mut sample_rect = source;
    let ratio = source.w / source.h;
    if mode == 3 {
        if dest.w / dest.h > ratio { dest.w = dest.h * ratio; dest.x += (item.rect.w - dest.w) * 0.5; }
        else { dest.h = dest.w / ratio; dest.y += (item.rect.h - dest.h) * 0.5; }
    } else if mode == 4 {
        if dest.w / dest.h > ratio { sample_rect.h = source.w * dest.h / dest.w; sample_rect.y += (source.h - sample_rect.h) * 0.5; }
        else { sample_rect.w = source.h * dest.w / dest.h; sample_rect.x += (source.w - sample_rect.w) * 0.5; }
    }
    let tile = udim2_prop(&item.instance, "TileSize", udim2(1.0, 0, 1.0, 0));
    let tile_w = resolve(tile.x, item.rect.w, item.scale).max(0.001);
    let tile_h = resolve(tile.y, item.rect.h, item.scale).max(0.001);
    let slice = match item.instance.get_property("SliceCenter") {
        Some(Variant::Rect(rect)) => Some((Vec2::new(rect.min.x, rect.min.y), Vec2::new(rect.max.x, rect.max.y))),
        _ => None,
    };
    let slice_scale = (f32_prop(&item.instance, "SliceScale", 1.0) * item.scale).max(0.0);
    let bounds = dest.intersect(item.clip);
    if !bounds.valid() { return; }
    let (x0, y0, x1, y1) = bounds.bounds(framebuffer);
    for y in y0..y1 { for x in x0..x1 {
        if !bounds.contains(x, y) { continue; }
        let dx = x as f32 + 0.5 - dest.x;
        let dy = y as f32 + 0.5 - dest.y;
        let (sx, sy) = if mode == 2 {
            (source.x + (dx / tile_w).rem_euclid(1.0) * source.w, source.y + (dy / tile_h).rem_euclid(1.0) * source.h)
        } else if mode == 1 && slice.is_some() {
            let (min, max) = slice.unwrap();
            (source.x + slice_coordinate(dx, dest.w, source.w, min.x - source.x, max.x - source.x, slice_scale),
             source.y + slice_coordinate(dy, dest.h, source.h, min.y - source.y, max.y - source.y, slice_scale))
        } else { (sample_rect.x + dx / dest.w * sample_rect.w, sample_rect.y + dy / dest.h * sample_rect.h) };
        let sample = sample_pixels(width, pixels, source, sx, sy, pixelated);
        framebuffer.blend_pixel(x, y, [
            (sample.x * tint.x * 255.0).round().clamp(0.0, 255.0) as u8,
            (sample.y * tint.y * 255.0).round().clamp(0.0, 255.0) as u8,
            (sample.z * tint.z * 255.0).round().clamp(0.0, 255.0) as u8,
            (sample.w * alpha * 255.0).round().clamp(0.0, 255.0) as u8,
        ]);
    } }
}

fn slice_coordinate(position: f32, dest: f32, source: f32, min: f32, max: f32, scale: f32) -> f32 {
    let min = min.clamp(0.0, source);
    let max = max.clamp(min, source);
    let border = (min + source - max) * scale;
    let shrink = if border > dest { dest / border } else { 1.0 };
    let left = min * scale * shrink;
    let right = (source - max) * scale * shrink;
    if position < left && left > 0.0 { position / left * min }
    else if position >= dest - right && right > 0.0 { max + (position - dest + right) / right * (source - max) }
    else { min + (position - left) / (dest - left - right).max(0.0001) * (max - min) }
}

fn sample_pixels(width: u32, pixels: &[u8], crop: Rect, x: f32, y: f32, nearest: bool) -> Vec4 {
    let read = |x: i32, y: i32| {
        let x = x.clamp(crop.x as i32, (crop.x + crop.w) as i32 - 1) as usize;
        let y = y.clamp(crop.y as i32, (crop.y + crop.h) as i32 - 1) as usize;
        let i = (y * width as usize + x) * 4;
        Vec4::new(pixels[i] as f32, pixels[i + 1] as f32, pixels[i + 2] as f32, pixels[i + 3] as f32) / 255.0
    };
    if nearest { return read(x.floor() as i32, y.floor() as i32); }
    let x = x - 0.5;
    let y = y - 0.5;
    let ix = x.floor() as i32;
    let iy = y.floor() as i32;
    let premultiply = |value: Vec4| Vec4::new(value.x * value.w, value.y * value.w, value.z * value.w, value.w);
    let a = premultiply(read(ix, iy));
    let b = premultiply(read(ix + 1, iy));
    let c = premultiply(read(ix, iy + 1));
    let d = premultiply(read(ix + 1, iy + 1));
    let result = a.lerp(b, x.fract()).lerp(c.lerp(d, x.fract()), y.fract());
    if result.w > f32::EPSILON { Vec4::new(result.x / result.w, result.y / result.w, result.z / result.w, result.w) } else { Vec4::ZERO }
}

fn draw_viewport(framebuffer: &mut Framebuffer, item: DrawItem, state: &RenderState, stats: &mut GuiStats) {
    let Some(camera) = ref_prop(&item.instance, "CurrentCamera") else { return };
    let width = item.rect.w.round().clamp(1.0, 4096.0) as u32;
    let height = item.rect.h.round().clamp(1.0, 4096.0) as u32;
    let mut target = Framebuffer::new(width, height);
    target.clear([0, 0, 0, 0]);
    let ambient = color_prop(&item.instance, "Ambient", Vec3::splat(0.5));
    let lighting = Lighting {
        ambient, outdoor_ambient: ambient, light_color: color_prop(&item.instance, "LightColor", Vec3::ONE),
        light_direction: vec3_prop(&item.instance, "LightDirection", Vec3::new(-1.0, -1.0, -1.0)).normalize_or_zero(),
        brightness: 1.0, ..Lighting::default()
    };
    let result = render_world(&mut target, item.instance, camera, state, lighting);
    stats.viewport_parts += result.parts;
    stats.viewport_triangles += result.triangles;
    let rect = item.rect.intersect(item.clip);
    let (x0, y0, x1, y1) = rect.bounds(framebuffer);
    for y in y0..y1 { for x in x0..x1 {
        if !rect.contains(x, y) { continue; }
        let sx = ((x as f32 + 0.5 - item.rect.x) / item.rect.w * width as f32).floor().clamp(0.0, width as f32 - 1.0) as usize;
        let sy = ((y as f32 + 0.5 - item.rect.y) / item.rect.h * height as f32).floor().clamp(0.0, height as f32 - 1.0) as usize;
        let i = (sy * width as usize + sx) * 4;
        framebuffer.blend_pixel(x, y, [target.pixels[i], target.pixels[i + 1], target.pixels[i + 2], target.pixels[i + 3]]);
    } }
}

fn resolve(value: UDim, parent: f32, scale: f32) -> f32 { value.scale * parent + value.offset as f32 * scale }
fn udim2(xs: f32, xo: i32, ys: f32, yo: i32) -> UDim2 {
    UDim2 { x: UDim { scale: xs, offset: xo }, y: UDim { scale: ys, offset: yo } }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn atlas_filter_does_not_sample_neighboring_cell() {
        let pixels = [255, 0, 0, 255, 0, 0, 255, 255];
        let source = Rect { x: 0.0, y: 0.0, w: 1.0, h: 1.0 };
        for x in [0.1, 0.5, 0.9] {
            assert!(sample_pixels(2, &pixels, source, x, 0.5, false).abs_diff_eq(Vec4::new(1.0, 0.0, 0.0, 1.0), 1e-6));
        }
    }
    #[test]
    fn transparent_filter_preserves_visible_color() {
        let pixels = [255, 0, 0, 255, 0, 0, 0, 0];
        let source = Rect { x: 0.0, y: 0.0, w: 2.0, h: 1.0 };
        let sample = sample_pixels(2, &pixels, source, 1.0, 0.5, false);
        assert!(sample.abs_diff_eq(Vec4::new(1.0, 0.0, 0.0, 0.5), 1e-6));
    }
    #[test]
    fn slices_preserve_border_widths() {
        assert!((slice_coordinate(1.0, 8.0, 3.0, 1.0, 2.0, 2.0) - 0.5).abs() < 1e-6);
        assert!((slice_coordinate(4.0, 8.0, 3.0, 1.0, 2.0, 2.0) - 1.5).abs() < 1e-6);
        assert!((slice_coordinate(7.0, 8.0, 3.0, 1.0, 2.0, 2.0) - 2.5).abs() < 1e-6);
    }
}
