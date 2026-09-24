//! Small vector icons and shapes drawn with `canvas`, tinted from the theme.
//!
//! Geometry is cached per widget and only rebuilt when the size, icon or
//! color changes.

use std::cell::Cell;

use iced::mouse;
use iced::widget::canvas::{self, Frame, Path, Stroke};
use iced::widget::{Canvas, canvas as canvas_widget};
use iced::{Color, Length, Point, Rectangle, Renderer, Size, Theme};

use crate::theme::{Palette, palette};

/// Which palette color to draw with.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tint {
    Text,
    Muted,
    OnAccent,
    OnAccentMuted,
    Danger,
    LineStrong,
    SurfaceAlt,
    Accent,
}

impl Tint {
    pub fn color(self, p: &Palette) -> Color {
        match self {
            Tint::Text => p.text,
            Tint::Muted => p.muted,
            Tint::OnAccent => p.on_accent,
            Tint::OnAccentMuted => p.on_accent_muted,
            Tint::Danger => p.danger,
            Tint::LineStrong => p.line_strong,
            Tint::SurfaceAlt => p.surface_alt,
            Tint::Accent => p.accent,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Icon {
    Pin,
    Muted,
    Check,
    Clock,
    Alert,
    Send,
    Reply,
    Close,
    File,
    Download,
    Play,
    Theme,
    Logout,
    Archive,
}

/// An icon of `size` logical pixels.
pub fn icon(icon: Icon, size: f32, tint: Tint) -> Canvas<Shape, crate::app::Message> {
    canvas_widget(Shape::Icon(icon, tint))
        .width(size)
        .height(size)
}

/// A dashed pill outline filling its parent (layer it over content in a stack).
pub fn dashed_pill(tint: Tint) -> Canvas<Shape, crate::app::Message> {
    canvas_widget(Shape::DashedPill(tint))
        .width(Length::Fill)
        .height(Length::Fill)
}

/// Paints the corners outside a rounded rectangle, to round images on
/// renderers that ignore image border radii.
pub fn corner_mask(radius: f32, tint: Tint) -> Canvas<Shape, crate::app::Message> {
    canvas_widget(Shape::CornerMask(radius, tint))
        .width(Length::Fill)
        .height(Length::Fill)
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Shape {
    Icon(Icon, Tint),
    DashedPill(Tint),
    CornerMask(f32, Tint),
}

#[derive(Default)]
pub struct ShapeState {
    cache: canvas::Cache,
    key: Cell<Option<(Shape, Color)>>,
}

impl<Message> canvas::Program<Message> for Shape {
    type State = ShapeState;

    fn draw(
        &self,
        state: &ShapeState,
        renderer: &Renderer,
        theme: &Theme,
        bounds: Rectangle,
        _cursor: mouse::Cursor,
    ) -> Vec<canvas::Geometry> {
        let p = palette(theme);
        let color = match self {
            Shape::Icon(_, tint) | Shape::DashedPill(tint) | Shape::CornerMask(_, tint) => {
                tint.color(p)
            }
        };
        let key = Some((*self, color));
        if state.key.get() != key {
            state.cache.clear();
            state.key.set(key);
        }
        let geometry = state
            .cache
            .draw(renderer, bounds.size(), |frame| match *self {
                Shape::Icon(icon, _) => draw_icon(frame, icon, color),
                Shape::DashedPill(_) => draw_dashed_pill(frame, color),
                Shape::CornerMask(radius, _) => draw_corner_mask(frame, radius, color),
            });
        vec![geometry]
    }
}

fn draw_dashed_pill(frame: &mut Frame, color: Color) {
    let size = frame.size();
    let radius = (size.height / 2.0 - 0.5).max(0.0);
    let path = Path::rounded_rectangle(
        Point::new(0.5, 0.5),
        Size::new(size.width - 1.0, size.height - 1.0),
        radius.into(),
    );
    frame.stroke(
        &path,
        Stroke {
            style: canvas::Style::Solid(color),
            width: 1.0,
            line_dash: canvas::LineDash {
                segments: &[4.0, 3.0],
                offset: 0,
            },
            ..Stroke::default()
        },
    );
}

fn draw_corner_mask(frame: &mut Frame, radius: f32, color: Color) {
    let size = frame.size();
    // The image is laid out 1px inside the mask and snapped to whole pixels
    // while the mask is not: a 2px frame hides that seam on every renderer.
    let inset = 2.0;
    let path = Path::new(|b| {
        b.rectangle(Point::ORIGIN, size);
        b.rounded_rectangle(
            Point::new(inset, inset),
            Size::new(size.width - 2.0 * inset, size.height - 2.0 * inset),
            (radius - inset).max(0.0).into(),
        );
    });
    frame.fill(
        &path,
        canvas::Fill {
            style: canvas::Style::Solid(color),
            rule: canvas::fill::Rule::EvenOdd,
        },
    );
}

/// Draws `icon` on a 16x16 grid scaled to the frame.
fn draw_icon(frame: &mut Frame, icon: Icon, color: Color) {
    let scale = frame.width().min(frame.height()) / 16.0;
    frame.scale(scale);
    let pt = Point::new;
    let line = |width: f32| Stroke {
        style: canvas::Style::Solid(color),
        width,
        line_cap: canvas::LineCap::Round,
        line_join: canvas::LineJoin::Round,
        ..Stroke::default()
    };
    let polyline = |points: &[(f32, f32)]| {
        Path::new(|b| {
            if let Some((first, rest)) = points.split_first() {
                b.move_to(pt(first.0, first.1));
                for (x, y) in rest {
                    b.line_to(pt(*x, *y));
                }
            }
        })
    };
    let polygon = |points: &[(f32, f32)]| {
        Path::new(|b| {
            if let Some((first, rest)) = points.split_first() {
                b.move_to(pt(first.0, first.1));
                for (x, y) in rest {
                    b.line_to(pt(*x, *y));
                }
                b.close();
            }
        })
    };

    match icon {
        Icon::Pin => {
            frame.translate(iced::Vector::new(8.0, 8.0));
            frame.rotate(std::f32::consts::FRAC_PI_4);
            frame.translate(iced::Vector::new(-8.0, -8.0));
            frame.fill(
                &polygon(&[
                    (5.5, 2.0),
                    (10.5, 2.0),
                    (10.0, 7.0),
                    (12.0, 9.5),
                    (4.0, 9.5),
                    (6.0, 7.0),
                ]),
                color,
            );
            frame.stroke(&polyline(&[(8.0, 9.5), (8.0, 14.5)]), line(1.5));
        }
        Icon::Muted => {
            frame.fill(
                &polygon(&[
                    (2.0, 6.0),
                    (4.8, 6.0),
                    (8.5, 3.0),
                    (8.5, 13.0),
                    (4.8, 10.0),
                    (2.0, 10.0),
                ]),
                color,
            );
            frame.stroke(&polyline(&[(11.0, 6.0), (15.0, 10.0)]), line(1.4));
            frame.stroke(&polyline(&[(15.0, 6.0), (11.0, 10.0)]), line(1.4));
        }
        Icon::Check => {
            frame.stroke(
                &polyline(&[(3.0, 8.5), (6.5, 12.0), (13.0, 4.5)]),
                line(1.6),
            );
        }
        Icon::Clock => {
            frame.stroke(&Path::circle(pt(8.0, 8.0), 6.0), line(1.4));
            frame.stroke(&polyline(&[(8.0, 4.8), (8.0, 8.0), (10.4, 9.6)]), line(1.4));
        }
        Icon::Alert => {
            frame.fill(&Path::circle(pt(8.0, 8.0), 7.0), color);
            let cut = Stroke {
                style: canvas::Style::Solid(Color::WHITE),
                ..line(1.8)
            };
            frame.stroke(&polyline(&[(8.0, 4.2), (8.0, 8.8)]), cut);
            frame.fill(&Path::circle(pt(8.0, 11.6), 1.1), Color::WHITE);
        }
        Icon::Send => {
            frame.stroke(&polyline(&[(8.0, 13.0), (8.0, 3.5)]), line(1.8));
            frame.stroke(&polyline(&[(3.8, 7.5), (8.0, 3.3), (12.2, 7.5)]), line(1.8));
        }
        Icon::Reply => {
            frame.stroke(&polyline(&[(6.0, 3.0), (2.0, 7.0), (6.0, 11.0)]), line(1.5));
            let curve = Path::new(|b| {
                b.move_to(pt(2.5, 7.0));
                b.line_to(pt(9.0, 7.0));
                b.quadratic_curve_to(pt(14.0, 7.0), pt(14.0, 13.0));
            });
            frame.stroke(&curve, line(1.5));
        }
        Icon::Close => {
            frame.stroke(&polyline(&[(4.0, 4.0), (12.0, 12.0)]), line(1.6));
            frame.stroke(&polyline(&[(12.0, 4.0), (4.0, 12.0)]), line(1.6));
        }
        Icon::File => {
            frame.stroke(
                &polygon(&[
                    (4.0, 1.5),
                    (9.5, 1.5),
                    (13.0, 5.0),
                    (13.0, 14.5),
                    (4.0, 14.5),
                ]),
                line(1.3),
            );
            frame.stroke(&polyline(&[(9.5, 1.5), (9.5, 5.0), (13.0, 5.0)]), line(1.3));
        }
        Icon::Download => {
            frame.stroke(&polyline(&[(8.0, 2.0), (8.0, 10.5)]), line(1.6));
            frame.stroke(
                &polyline(&[(4.5, 7.0), (8.0, 10.5), (11.5, 7.0)]),
                line(1.6),
            );
            frame.stroke(&polyline(&[(3.0, 13.5), (13.0, 13.5)]), line(1.6));
        }
        Icon::Play => {
            frame.fill(&polygon(&[(5.0, 3.0), (13.0, 8.0), (5.0, 13.0)]), color);
        }
        Icon::Theme => {
            frame.stroke(&Path::circle(pt(8.0, 8.0), 6.0), line(1.4));
            let half = Path::new(|b| {
                b.move_to(pt(8.0, 2.0));
                b.arc(canvas::path::Arc {
                    center: pt(8.0, 8.0),
                    radius: 6.0,
                    start_angle: iced::Radians(-std::f32::consts::FRAC_PI_2),
                    end_angle: iced::Radians(std::f32::consts::FRAC_PI_2),
                });
                b.close();
            });
            frame.fill(&half, color);
        }
        Icon::Logout => {
            frame.stroke(
                &polyline(&[(7.0, 2.5), (3.0, 2.5), (3.0, 13.5), (7.0, 13.5)]),
                line(1.4),
            );
            frame.stroke(&polyline(&[(6.5, 8.0), (14.0, 8.0)]), line(1.4));
            frame.stroke(
                &polyline(&[(11.0, 5.0), (14.0, 8.0), (11.0, 11.0)]),
                line(1.4),
            );
        }
        Icon::Archive => {
            frame.stroke(
                &polygon(&[(2.0, 3.0), (14.0, 3.0), (14.0, 6.0), (2.0, 6.0)]),
                line(1.3),
            );
            frame.stroke(
                &polyline(&[(3.0, 6.0), (3.0, 13.5), (13.0, 13.5), (13.0, 6.0)]),
                line(1.3),
            );
            frame.stroke(&polyline(&[(6.5, 9.0), (9.5, 9.0)]), line(1.3));
        }
    }
}
