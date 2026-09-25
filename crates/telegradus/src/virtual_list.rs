//! Range math for a virtualized list of fixed-height rows.
//!
//! Only the rows intersecting the viewport (plus a small overscan) are built;
//! two spacers stand in for the rows above and below, so the scrollable keeps
//! the full content height.

use std::ops::Range;

/// The slice of rows to build and the spacer heights around it.
#[derive(Debug, Clone, PartialEq)]
pub struct Window {
    pub rows: Range<usize>,
    pub space_before: f32,
    pub space_after: f32,
}

/// Rows of a list with `count` rows of `row_height` visible at `offset` in a
/// viewport of `viewport_height`, extended by `overscan` rows on both sides.
pub fn window(
    count: usize,
    row_height: f32,
    offset: f32,
    viewport_height: f32,
    overscan: usize,
) -> Window {
    if count == 0 || row_height <= 0.0 {
        return Window {
            rows: 0..0,
            space_before: 0.0,
            space_after: 0.0,
        };
    }
    let offset = if offset.is_finite() {
        offset.max(0.0)
    } else {
        0.0
    };
    let viewport_height = if viewport_height.is_finite() {
        viewport_height.max(0.0)
    } else {
        0.0
    };

    let first_visible = ((offset / row_height).floor() as usize).min(count - 1);
    let visible = (viewport_height / row_height).ceil() as usize + 1;
    let start = first_visible.saturating_sub(overscan);
    let end = (first_visible + visible + overscan).min(count);

    Window {
        rows: start..end,
        space_before: start as f32 * row_height,
        space_after: (count - end) as f32 * row_height,
    }
}

/// Whether the viewport bottom is within `threshold` pixels of the list end.
pub fn near_end(
    count: usize,
    row_height: f32,
    offset: f32,
    viewport_height: f32,
    threshold: f32,
) -> bool {
    let content = count as f32 * row_height;
    let bottom = offset.max(0.0) + viewport_height.max(0.0);
    content - bottom <= threshold
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_list() {
        let w = window(0, 64.0, 0.0, 600.0, 3);
        assert_eq!(w.rows, 0..0);
        assert_eq!(w.space_before, 0.0);
        assert_eq!(w.space_after, 0.0);
    }

    #[test]
    fn top_of_list() {
        // 600 / 64 = 9.4 -> 10 rows + 1 partial + 3 overscan below.
        let w = window(100, 64.0, 0.0, 600.0, 3);
        assert_eq!(w.rows, 0..14);
        assert_eq!(w.space_before, 0.0);
        assert_eq!(w.space_after, 86.0 * 64.0);
    }

    #[test]
    fn middle_of_list() {
        let w = window(100, 64.0, 64.0 * 40.5, 600.0, 3);
        assert_eq!(w.rows, 37..54);
        assert_eq!(w.space_before, 37.0 * 64.0);
        assert_eq!(w.space_after, 46.0 * 64.0);
        // Spacers plus rows always add up to the full height.
        let total = w.space_before + w.rows.len() as f32 * 64.0 + w.space_after;
        assert_eq!(total, 100.0 * 64.0);
    }

    #[test]
    fn end_of_list_is_clamped() {
        let w = window(20, 64.0, 10_000.0, 600.0, 3);
        assert_eq!(w.rows.end, 20);
        assert_eq!(w.space_after, 0.0);
        assert!(w.rows.start < 20);
    }

    #[test]
    fn short_list_renders_everything() {
        let w = window(5, 64.0, 0.0, 600.0, 3);
        assert_eq!(w.rows, 0..5);
        assert_eq!(w.space_after, 0.0);
    }

    #[test]
    fn bad_input_is_sanitized() {
        let w = window(10, 64.0, f32::NAN, -5.0, 2);
        assert_eq!(w.rows, 0..3);
        let w = window(10, 0.0, 0.0, 100.0, 2);
        assert_eq!(w.rows, 0..0);
    }

    #[test]
    fn near_end_threshold() {
        assert!(!near_end(100, 64.0, 0.0, 600.0, 300.0));
        assert!(near_end(100, 64.0, 6400.0 - 600.0 - 200.0, 600.0, 300.0));
        assert!(near_end(3, 64.0, 0.0, 600.0, 300.0));
    }
}
