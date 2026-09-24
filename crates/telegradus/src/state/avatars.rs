//! Round avatar thumbnails.
//!
//! Avatars are decoded, downscaled and masked to a circle once, off the UI
//! thread, so they look the same on every renderer and cost only a few KB.
//! The cache is bounded (least recently used entries are evicted).

use std::collections::{HashMap, HashSet};
use std::path::Path;

use iced::widget::image;
use telegradus_core::FileId;

/// Pixel size of a thumbnail: 2x the largest avatar for sharp HiDPI output.
pub const THUMB_SIZE: u32 = 96;
/// Enough for several screens of chat rows.
const CAPACITY: usize = 160;

#[derive(Debug, Default)]
pub struct Avatars {
    ready: HashMap<FileId, (image::Handle, u64)>,
    in_flight: HashSet<FileId>,
    failed: HashSet<FileId>,
    clock: u64,
}

impl Avatars {
    pub fn get(&self, id: FileId) -> Option<&image::Handle> {
        self.ready.get(&id).map(|(handle, _)| handle)
    }

    /// Marks an avatar as used; returns `true` if it must be generated.
    pub fn touch(&mut self, id: FileId) -> bool {
        self.clock += 1;
        if let Some((_, used)) = self.ready.get_mut(&id) {
            *used = self.clock;
            return false;
        }
        !self.failed.contains(&id) && self.in_flight.insert(id)
    }

    pub fn finish(&mut self, id: FileId, result: Result<image::Handle, String>) {
        self.in_flight.remove(&id);
        match result {
            Ok(handle) => {
                self.clock += 1;
                self.ready.insert(id, (handle, self.clock));
                self.evict();
            }
            Err(error) => {
                tracing::debug!(id, %error, "avatar thumbnail failed");
                self.failed.insert(id);
            }
        }
    }

    fn evict(&mut self) {
        while self.ready.len() > CAPACITY {
            let oldest = self
                .ready
                .iter()
                .min_by_key(|(_, (_, used))| *used)
                .map(|(id, _)| *id);
            match oldest {
                Some(id) => {
                    self.ready.remove(&id);
                }
                None => break,
            }
        }
    }
}

/// Decodes `path`, crops it to a centered square and masks it to a circle.
pub fn make_thumbnail(path: &Path) -> Result<image::Handle, String> {
    let decoded = ::image::open(path).map_err(|e| e.to_string())?;
    let square = decoded.resize_to_fill(
        THUMB_SIZE,
        THUMB_SIZE,
        ::image::imageops::FilterType::Triangle,
    );
    let mut rgba = square.to_rgba8();
    let radius = THUMB_SIZE as f32 / 2.0;
    for (x, y, pixel) in rgba.enumerate_pixels_mut() {
        let dx = x as f32 + 0.5 - radius;
        let dy = y as f32 + 0.5 - radius;
        let distance = (dx * dx + dy * dy).sqrt();
        // One pixel of anti-aliasing on the edge.
        let coverage = (radius - distance + 0.5).clamp(0.0, 1.0);
        pixel.0[3] = (f32::from(pixel.0[3]) * coverage) as u8;
    }
    Ok(image::Handle::from_rgba(
        THUMB_SIZE,
        THUMB_SIZE,
        rgba.into_raw(),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn handle() -> image::Handle {
        image::Handle::from_rgba(1, 1, vec![0, 0, 0, 255])
    }

    #[test]
    fn touch_requests_once() {
        let mut avatars = Avatars::default();
        assert!(avatars.touch(1));
        assert!(!avatars.touch(1));
        avatars.finish(1, Ok(handle()));
        assert!(avatars.get(1).is_some());
        assert!(!avatars.touch(1));
    }

    #[test]
    fn failures_are_not_retried() {
        let mut avatars = Avatars::default();
        assert!(avatars.touch(2));
        avatars.finish(2, Err("broken".into()));
        assert!(!avatars.touch(2));
        assert!(avatars.get(2).is_none());
    }

    #[test]
    fn cache_is_bounded_and_keeps_recent() {
        let mut avatars = Avatars::default();
        for id in 0..(CAPACITY as i32 + 20) {
            avatars.touch(id);
            avatars.finish(id, Ok(handle()));
            // Keep the first avatar hot.
            avatars.touch(0);
        }
        assert_eq!(avatars.ready.len(), CAPACITY);
        assert!(avatars.get(0).is_some());
        assert!(avatars.get(1).is_none());
    }

    #[test]
    fn thumbnail_is_round() {
        let dir = std::env::temp_dir().join(format!("telegradus-thumb-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("a.png");
        ::image::RgbImage::from_pixel(200, 120, ::image::Rgb([200, 10, 10]))
            .save(&path)
            .unwrap();
        let handle = make_thumbnail(&path).unwrap();
        let image::Handle::Rgba {
            width,
            height,
            pixels,
            ..
        } = handle
        else {
            panic!("expected rgba handle");
        };
        assert_eq!((width, height), (THUMB_SIZE, THUMB_SIZE));
        let alpha = |x: u32, y: u32| pixels[((y * THUMB_SIZE + x) * 4 + 3) as usize];
        assert_eq!(alpha(0, 0), 0);
        assert_eq!(alpha(THUMB_SIZE / 2, THUMB_SIZE / 2), 255);
        std::fs::remove_dir_all(dir).unwrap();
    }
}
