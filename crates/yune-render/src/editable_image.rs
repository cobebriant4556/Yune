use std::sync::{Arc, Mutex};

use glam::Vec2;
use lune_roblox::datatypes::types::{Color3, Vector2};
use mlua::prelude::*;
use rbx_dom_weak::types::Color3 as DomColor3;

#[derive(Debug, Clone)]
pub struct ImageSnapshot {
    pub width: u32,
    pub height: u32,
    pub pixels: Vec<u8>,
}

#[derive(Debug)]
struct ImageData {
    width: u32,
    height: u32,
    pixels: Vec<u8>,
    destroyed: bool,
}

#[derive(Debug, Clone)]
pub struct EditableImage {
    inner: Arc<Mutex<ImageData>>,
}

impl EditableImage {
    pub fn new(width: u32, height: u32) -> Self {
        let width = width.clamp(1, 4096);
        let height = height.clamp(1, 4096);
        Self {
            inner: Arc::new(Mutex::new(ImageData {
                width,
                height,
                pixels: vec![0; width as usize * height as usize * 4],
                destroyed: false,
            })),
        }
    }

    pub fn snapshot(&self) -> ImageSnapshot {
        let data = self.inner.lock().expect("editable image lock poisoned");
        ImageSnapshot {
            width: data.width,
            height: data.height,
            pixels: data.pixels.clone(),
        }
    }

    fn ensure_alive(data: &ImageData) -> LuaResult<()> {
        if data.destroyed {
            Err(LuaError::runtime("EditableImage has been destroyed"))
        } else {
            Ok(())
        }
    }

    fn rgba(color: Color3, transparency: f32) -> [u8; 4] {
        let c: DomColor3 = color.into();
        [
            (c.r.clamp(0.0, 1.0) * 255.0).round() as u8,
            (c.g.clamp(0.0, 1.0) * 255.0).round() as u8,
            (c.b.clamp(0.0, 1.0) * 255.0).round() as u8,
            ((1.0 - transparency.clamp(0.0, 1.0)) * 255.0).round() as u8,
        ]
    }

    fn blend(data: &mut ImageData, x: i32, y: i32, src: [u8; 4]) {
        if x < 0 || y < 0 || x >= data.width as i32 || y >= data.height as i32 {
            return;
        }
        let i = (y as usize * data.width as usize + x as usize) * 4;
        let sa = src[3] as f32 / 255.0;
        let da = data.pixels[i + 3] as f32 / 255.0;
        let out_a = sa + da * (1.0 - sa);
        if out_a <= f32::EPSILON {
            data.pixels[i..i + 4].fill(0);
            return;
        }
        for c in 0..3 {
            let s = src[c] as f32 / 255.0;
            let d = data.pixels[i + c] as f32 / 255.0;
            let out = (s * sa + d * da * (1.0 - sa)) / out_a;
            data.pixels[i + c] = (out.clamp(0.0, 1.0) * 255.0).round() as u8;
        }
        data.pixels[i + 3] = (out_a.clamp(0.0, 1.0) * 255.0).round() as u8;
    }
}

impl Default for EditableImage {
    fn default() -> Self {
        Self::new(512, 512)
    }
}

impl LuaUserData for EditableImage {
    fn add_fields<F: LuaUserDataFields<Self>>(fields: &mut F) {
        fields.add_field_method_get("Size", |_, this| {
            let data = this.inner.lock().expect("editable image lock poisoned");
            Self::ensure_alive(&data)?;
            Ok(Vector2(Vec2::new(data.width as f32, data.height as f32)))
        });
    }

    fn add_methods<M: LuaUserDataMethods<Self>>(methods: &mut M) {
        methods.add_method_mut(
            "DrawRectangle",
            |_, this, (position, size, color, transparency, _combine): (LuaUserDataRef<Vector2>, LuaUserDataRef<Vector2>, LuaUserDataRef<Color3>, f32, Option<LuaValue>)| {
                let mut data = this.inner.lock().expect("editable image lock poisoned");
                Self::ensure_alive(&data)?;
                let rgba = Self::rgba(*color, transparency);
                let x0 = position.0.x.floor() as i32;
                let y0 = position.0.y.floor() as i32;
                let x1 = (position.0.x + size.0.x).ceil() as i32;
                let y1 = (position.0.y + size.0.y).ceil() as i32;
                for y in y0..y1 {
                    for x in x0..x1 {
                        Self::blend(&mut data, x, y, rgba);
                    }
                }
                Ok(())
            },
        );

        methods.add_method_mut(
            "DrawLine",
            |_, this, (p1, p2, color, transparency, _combine, _aa): (LuaUserDataRef<Vector2>, LuaUserDataRef<Vector2>, LuaUserDataRef<Color3>, f32, Option<LuaValue>, Option<LuaValue>)| {
                let mut data = this.inner.lock().expect("editable image lock poisoned");
                Self::ensure_alive(&data)?;
                let rgba = Self::rgba(*color, transparency);
                let mut x0 = p1.0.x.round() as i32;
                let mut y0 = p1.0.y.round() as i32;
                let x1 = p2.0.x.round() as i32;
                let y1 = p2.0.y.round() as i32;
                let dx = (x1 - x0).abs();
                let sx = if x0 < x1 { 1 } else { -1 };
                let dy = -(y1 - y0).abs();
                let sy = if y0 < y1 { 1 } else { -1 };
                let mut err = dx + dy;
                loop {
                    Self::blend(&mut data, x0, y0, rgba);
                    if x0 == x1 && y0 == y1 {
                        break;
                    }
                    let e2 = err * 2;
                    if e2 >= dy {
                        err += dy;
                        x0 += sx;
                    }
                    if e2 <= dx {
                        err += dx;
                        y0 += sy;
                    }
                }
                Ok(())
            },
        );

        methods.add_method_mut(
            "DrawCircle",
            |_, this, (center, radius, color, transparency, _combine, _aa): (LuaUserDataRef<Vector2>, f32, LuaUserDataRef<Color3>, f32, Option<LuaValue>, Option<LuaValue>)| {
                let mut data = this.inner.lock().expect("editable image lock poisoned");
                Self::ensure_alive(&data)?;
                let rgba = Self::rgba(*color, transparency);
                let r = radius.max(0.0);
                let x0 = (center.0.x - r).floor() as i32;
                let y0 = (center.0.y - r).floor() as i32;
                let x1 = (center.0.x + r).ceil() as i32;
                let y1 = (center.0.y + r).ceil() as i32;
                let r2 = r * r;
                for y in y0..=y1 {
                    for x in x0..=x1 {
                        let dx = x as f32 + 0.5 - center.0.x;
                        let dy = y as f32 + 0.5 - center.0.y;
                        if dx * dx + dy * dy <= r2 {
                            Self::blend(&mut data, x, y, rgba);
                        }
                    }
                }
                Ok(())
            },
        );

        methods.add_method_mut("Clear", |_, this, ()| {
            let mut data = this.inner.lock().expect("editable image lock poisoned");
            Self::ensure_alive(&data)?;
            data.pixels.fill(0);
            Ok(())
        });

        methods.add_method_mut("Destroy", |_, this, ()| {
            let mut data = this.inner.lock().expect("editable image lock poisoned");
            data.pixels.clear();
            data.destroyed = true;
            Ok(())
        });
    }
}
