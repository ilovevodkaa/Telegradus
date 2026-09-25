//! Procedurally generated pictures for the demo: photos and avatars.

use std::path::{Path, PathBuf};

use image::{ImageBuffer, Rgb, RgbImage};

/// What a demo file looks like once "downloaded".
#[derive(Debug, Clone, Copy)]
pub enum Picture {
    /// Landscape: sky gradient, sun and hills.
    Landscape { seed: u32 },
    /// Portrait: night city skyline.
    City { seed: u32 },
    /// Square avatar with a gradient and a disc.
    Avatar { seed: u32 },
}

impl Picture {
    pub fn size(self) -> (u32, u32) {
        match self {
            Picture::Landscape { .. } => (800, 533),
            Picture::City { .. } => (540, 720),
            Picture::Avatar { .. } => (160, 160),
        }
    }
}

/// Renders `picture` to a PNG under `dir` (once) and returns its path.
pub fn render(dir: &Path, name: &str, picture: Picture) -> std::io::Result<PathBuf> {
    let path = dir.join(format!("{name}.png"));
    if path.exists() {
        return Ok(path);
    }
    std::fs::create_dir_all(dir)?;
    let (w, h) = picture.size();
    let image = match picture {
        Picture::Landscape { seed } => landscape(w, h, seed),
        Picture::City { seed } => city(w, h, seed),
        Picture::Avatar { seed } => avatar(w, h, seed),
    };
    image.save(&path).map_err(std::io::Error::other)?;
    Ok(path)
}

type Color = [f32; 3];

const PALETTES: [(Color, Color); 6] = [
    ([255.0, 176.0, 102.0], [120.0, 76.0, 170.0]),
    ([126.0, 206.0, 244.0], [36.0, 72.0, 140.0]),
    ([250.0, 214.0, 130.0], [214.0, 94.0, 88.0]),
    ([168.0, 230.0, 190.0], [40.0, 110.0, 120.0]),
    ([240.0, 170.0, 200.0], [90.0, 60.0, 130.0]),
    ([200.0, 200.0, 210.0], [40.0, 40.0, 48.0]),
];

fn lerp(a: Color, b: Color, t: f32) -> Color {
    let t = t.clamp(0.0, 1.0);
    [
        a[0] + (b[0] - a[0]) * t,
        a[1] + (b[1] - a[1]) * t,
        a[2] + (b[2] - a[2]) * t,
    ]
}

fn rgb(c: Color) -> Rgb<u8> {
    Rgb([c[0] as u8, c[1] as u8, c[2] as u8])
}

fn landscape(w: u32, h: u32, seed: u32) -> RgbImage {
    let (top, bottom) = PALETTES[seed as usize % PALETTES.len()];
    let sun = (w as f32 * 0.68, h as f32 * 0.46, h as f32 * 0.11);
    let phase = seed as f32 * 1.7;
    ImageBuffer::from_fn(w, h, |x, y| {
        let (fx, fy) = (x as f32, y as f32);
        let sky = lerp(bottom, top, fy / h as f32 * 1.25);
        let far = h as f32 * (0.62 + 0.05 * (fx / 97.0 + phase).sin() + 0.02 * (fx / 31.0).sin());
        let near = h as f32 * (0.76 + 0.04 * (fx / 150.0 + phase * 2.0).cos());
        let color = if fy > near {
            lerp(
                [30.0, 28.0, 40.0],
                [12.0, 12.0, 16.0],
                (fy - near) / (h as f32 - near),
            )
        } else if fy > far {
            lerp(bottom, [30.0, 28.0, 40.0], 0.55)
        } else {
            let d = ((fx - sun.0).powi(2) + (fy - sun.1).powi(2)).sqrt();
            if d < sun.2 {
                [255.0, 244.0, 214.0]
            } else {
                let glow = (1.0 - (d - sun.2) / (sun.2 * 3.0)).clamp(0.0, 1.0) * 0.35;
                lerp(sky, [255.0, 236.0, 200.0], glow)
            }
        };
        rgb(color)
    })
}

fn city(w: u32, h: u32, seed: u32) -> RgbImage {
    let sky_top = [18.0, 22.0, 48.0];
    let sky_bottom = [92.0, 64.0, 128.0];
    let columns = 14;
    let heights: Vec<f32> = (0..columns)
        .map(|i| {
            let n = ((i as u32).wrapping_mul(2_654_435_761) ^ seed.wrapping_mul(40_503)) % 1000;
            0.35 + n as f32 / 1000.0 * 0.35
        })
        .collect();
    ImageBuffer::from_fn(w, h, |x, y| {
        let (fx, fy) = (x as f32, y as f32);
        let column = ((fx / w as f32) * columns as f32) as usize;
        let top = h as f32 * (1.0 - heights[column.min(columns - 1)]);
        let color = if fy > top {
            let wx = (fx as u32 / 9) % 3 == 1;
            let wy = (fy as u32 / 13) % 3 == 1;
            let lit = ((fx as u32 / 27) * 7 + (fy as u32 / 39) * 13 + seed) % 5 < 2;
            if wx && wy && lit {
                [255.0, 214.0, 130.0]
            } else {
                [14.0, 14.0, 20.0]
            }
        } else {
            let star = (x.wrapping_mul(73) ^ y.wrapping_mul(151)) % 997 == 0;
            if star {
                [240.0, 240.0, 255.0]
            } else {
                lerp(sky_top, sky_bottom, fy / h as f32)
            }
        };
        rgb(color)
    })
}

fn avatar(w: u32, h: u32, seed: u32) -> RgbImage {
    let (a, b) = PALETTES[seed as usize % PALETTES.len()];
    let center = (w as f32 * 0.62, h as f32 * 0.4);
    ImageBuffer::from_fn(w, h, |x, y| {
        let (fx, fy) = (x as f32, y as f32);
        let t = (fx + fy) / (w + h) as f32;
        let base = lerp(a, b, t);
        let d = ((fx - center.0).powi(2) + (fy - center.1).powi(2)).sqrt();
        let color = if d < w as f32 * 0.22 {
            lerp(base, [255.0, 255.0, 255.0], 0.45)
        } else {
            base
        };
        rgb(color)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn renders_png_once() {
        let dir = std::env::temp_dir().join(format!("telegradus-demo-test-{}", std::process::id()));
        let path = render(&dir, "a", Picture::Avatar { seed: 3 }).unwrap();
        assert!(path.exists());
        let again = render(&dir, "a", Picture::Avatar { seed: 3 }).unwrap();
        assert_eq!(path, again);
        let decoded = image::open(&path).unwrap();
        assert_eq!((decoded.width(), decoded.height()), (160, 160));
        std::fs::remove_dir_all(dir).unwrap();
    }
}
