use std::path::Path;

use image::{ImageError, RgbaImage};

#[derive(Debug, Clone)]
pub struct Framebuffer {
    pub width: u32,
    pub height: u32,
    pub pixels: Vec<u8>,
    pub depth: Vec<f32>,
}

impl Framebuffer {
    pub fn new(width: u32, height: u32) -> Self {
        let width = width.max(1);
        let height = height.max(1);
        Self {
            width,
            height,
            pixels: vec![0; width as usize * height as usize * 4],
            depth: vec![f32::INFINITY; width as usize * height as usize],
        }
    }

    pub fn clear(&mut self, color: [u8; 4]) {
        for px in self.pixels.chunks_exact_mut(4) {
            px.copy_from_slice(&color);
        }
        self.depth.fill(f32::INFINITY);
    }

    pub fn blend_pixel(&mut self, x: i32, y: i32, src: [u8; 4]) {
        if x < 0 || y < 0 || x >= self.width as i32 || y >= self.height as i32 {
            return;
        }
        let i = (y as usize * self.width as usize + x as usize) * 4;
        let sa = src[3] as f32 / 255.0;
        let da = self.pixels[i + 3] as f32 / 255.0;
        let out_a = sa + da * (1.0 - sa);
        if out_a <= f32::EPSILON {
            self.pixels[i..i + 4].fill(0);
            return;
        }
        for c in 0..3 {
            let s = src[c] as f32 / 255.0;
            let d = self.pixels[i + c] as f32 / 255.0;
            let out = (s * sa + d * da * (1.0 - sa)) / out_a;
            self.pixels[i + c] = (out.clamp(0.0, 1.0) * 255.0).round() as u8;
        }
        self.pixels[i + 3] = (out_a.clamp(0.0, 1.0) * 255.0).round() as u8;
    }

    pub fn composite(&mut self, src: &Framebuffer, dst_x: i32, dst_y: i32) {
        for y in 0..src.height as i32 {
            for x in 0..src.width as i32 {
                let i = (y as usize * src.width as usize + x as usize) * 4;
                self.blend_pixel(dst_x + x, dst_y + y, [
                    src.pixels[i],
                    src.pixels[i + 1],
                    src.pixels[i + 2],
                    src.pixels[i + 3],
                ]);
            }
        }
    }

    pub fn save_png(&self, path: impl AsRef<Path>) -> Result<(), ImageError> {
        let image = RgbaImage::from_raw(self.width, self.height, self.pixels.clone())
            .expect("framebuffer dimensions always match pixel storage");
        image.save(path)
    }
}
