//! Colors, fonts and widget styles.
//!
//! The look follows the author's site: monochrome surfaces, an ink accent,
//! JetBrains Mono for small meta text and pill-shaped controls. Every style
//! function reads the named colors of the active [`Palette`], which is picked
//! from the iced [`Theme`] (light or dark).

use iced::border::{self, Border};
use iced::font::{Style as FontStyle, Weight};
use iced::theme::Mode;
use iced::widget::{button, container, scrollable, text, text_editor, text_input};
use iced::{Background, Color, Font, Theme};

/// UI font family, bundled with the app.
pub const SANS: Font = Font::with_name("Inter Tight");
pub const SANS_MEDIUM: Font = Font {
    weight: Weight::Medium,
    ..SANS
};
pub const SANS_SEMIBOLD: Font = Font {
    weight: Weight::Semibold,
    ..SANS
};
pub const SANS_ITALIC: Font = Font {
    style: FontStyle::Italic,
    ..SANS
};
/// Small meta text (times, sizes, status lines) and code.
pub const MONO: Font = Font::with_name("JetBrains Mono");
/// The platform's color emoji font. Naming it avoids monochrome emoji from
/// general-purpose fallback fonts; if it is missing, normal fallback applies.
pub const EMOJI: Font = Font::with_name(if cfg!(windows) {
    "Segoe UI Emoji"
} else if cfg!(target_os = "macos") {
    "Apple Color Emoji"
} else {
    "Noto Color Emoji"
});

/// Named colors of one theme variant.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Palette {
    /// Chat pane and cards.
    pub bg: Color,
    /// Sidebar and the page behind auth cards.
    pub surface: Color,
    /// Incoming bubbles, inputs, placeholders.
    pub surface_alt: Color,
    pub hover: Color,
    pub line: Color,
    /// Dashed chip outlines and focused borders.
    pub line_strong: Color,
    pub text: Color,
    pub muted: Color,
    /// The ink accent: active pills, outgoing bubbles, primary buttons.
    pub accent: Color,
    pub on_accent: Color,
    /// Muted text drawn on the accent.
    pub on_accent_muted: Color,
    pub success: Color,
    pub warning: Color,
    pub danger: Color,
}

const fn hex(rgb: u32) -> Color {
    Color::from_rgb8((rgb >> 16) as u8, (rgb >> 8) as u8, rgb as u8)
}

pub const LIGHT: Palette = Palette {
    bg: hex(0xffffff),
    surface: hex(0xf7f7f9),
    surface_alt: hex(0xf0f0f3),
    hover: hex(0xececef),
    line: hex(0xe6e6ea),
    line_strong: hex(0xbdbdc5),
    text: hex(0x0b0b0c),
    muted: hex(0x74747c),
    accent: hex(0x0b0b0c),
    on_accent: hex(0xffffff),
    on_accent_muted: hex(0xa4a4ac),
    success: hex(0x1f9d55),
    warning: hex(0xd4940f),
    danger: hex(0xd93b3b),
};

pub const DARK: Palette = Palette {
    bg: hex(0x0b0b0c),
    surface: hex(0x18181c),
    surface_alt: hex(0x232328),
    hover: hex(0x26262c),
    line: hex(0x2a2a30),
    line_strong: hex(0x4a4a53),
    text: hex(0xe6e6ec),
    muted: hex(0x83838d),
    accent: hex(0xe6e6ec),
    on_accent: hex(0x0b0b0c),
    on_accent_muted: hex(0x5c5c64),
    success: hex(0x3ecf7a),
    warning: hex(0xf0b232),
    danger: hex(0xf06363),
};

/// Builds the iced theme for a mode. `Mode::None` means light.
pub fn theme_for(mode: Mode) -> Theme {
    let (name, p) = match mode {
        Mode::Dark => ("Telegradus Dark", DARK),
        Mode::Light | Mode::None => ("Telegradus Light", LIGHT),
    };
    Theme::custom(
        name,
        iced::theme::Palette {
            background: p.bg,
            text: p.text,
            primary: p.accent,
            success: p.success,
            warning: p.warning,
            danger: p.danger,
        },
    )
}

/// The named colors of the given theme.
pub fn palette(theme: &Theme) -> &'static Palette {
    if theme.extended_palette().is_dark {
        &DARK
    } else {
        &LIGHT
    }
}

// --- Text -------------------------------------------------------------------

pub fn text_muted(theme: &Theme) -> text::Style {
    text::Style {
        color: Some(palette(theme).muted),
    }
}

pub fn text_danger(theme: &Theme) -> text::Style {
    text::Style {
        color: Some(palette(theme).danger),
    }
}

// --- Containers ---------------------------------------------------------------

pub fn page(theme: &Theme) -> container::Style {
    let p = palette(theme);
    container::Style {
        background: Some(p.surface.into()),
        text_color: Some(p.text),
        ..container::Style::default()
    }
}

pub fn pane(theme: &Theme) -> container::Style {
    let p = palette(theme);
    container::Style {
        background: Some(p.bg.into()),
        text_color: Some(p.text),
        ..container::Style::default()
    }
}

/// The sidebar: surface with a 1px right border (drawn by [`vertical_line`]).
pub fn sidebar(theme: &Theme) -> container::Style {
    page(theme)
}

pub fn line(theme: &Theme) -> container::Style {
    container::Style {
        background: Some(palette(theme).line.into()),
        ..container::Style::default()
    }
}

pub fn card(theme: &Theme) -> container::Style {
    let p = palette(theme);
    container::Style {
        background: Some(p.bg.into()),
        text_color: Some(p.text),
        border: Border {
            color: p.line,
            width: 1.0,
            radius: 20.0.into(),
        },
        ..container::Style::default()
    }
}

pub fn bubble_in(theme: &Theme) -> container::Style {
    let p = palette(theme);
    container::Style {
        background: Some(p.surface_alt.into()),
        text_color: Some(p.text),
        border: border::rounded(16),
        ..container::Style::default()
    }
}

pub fn bubble_out(theme: &Theme) -> container::Style {
    let p = palette(theme);
    container::Style {
        background: Some(p.accent.into()),
        text_color: Some(p.on_accent),
        border: border::rounded(16),
        ..container::Style::default()
    }
}

/// Small centered pill for day separators and service messages.
pub fn service_pill(theme: &Theme) -> container::Style {
    let p = palette(theme);
    container::Style {
        background: Some(p.surface_alt.into()),
        text_color: Some(p.muted),
        border: border::rounded(999),
        ..container::Style::default()
    }
}

/// Quoted reply header inside a bubble.
pub fn reply_quote(outgoing: bool) -> impl Fn(&Theme) -> container::Style {
    move |theme| {
        let p = palette(theme);
        let bar = if outgoing {
            p.on_accent_muted
        } else {
            p.line_strong
        };
        container::Style {
            background: Some(bar.scale_alpha(0.18).into()),
            border: Border {
                color: bar,
                width: 0.0,
                radius: 8.0.into(),
            },
            ..container::Style::default()
        }
    }
}

/// The accent bar at the left of a reply quote.
pub fn quote_bar(outgoing: bool) -> impl Fn(&Theme) -> container::Style {
    move |theme| {
        let p = palette(theme);
        container::Style {
            background: Some(if outgoing { p.on_accent } else { p.text }.into()),
            border: border::rounded(2),
            ..container::Style::default()
        }
    }
}

/// Code blocks inside bubbles.
pub fn code_block(outgoing: bool) -> impl Fn(&Theme) -> container::Style {
    move |theme| {
        let p = palette(theme);
        let base = if outgoing { p.on_accent } else { p.text };
        container::Style {
            background: Some(base.scale_alpha(0.08).into()),
            border: border::rounded(8),
            ..container::Style::default()
        }
    }
}

/// Grey box shown in place of a photo that is not downloaded yet.
pub fn media_placeholder(theme: &Theme) -> container::Style {
    let p = palette(theme);
    container::Style {
        background: Some(p.hover.into()),
        text_color: Some(p.muted),
        border: border::rounded(12),
        ..container::Style::default()
    }
}

/// Circle with initials for chats without a photo.
pub fn avatar(shade: Color, text: Color) -> impl Fn(&Theme) -> container::Style {
    move |_| container::Style {
        background: Some(shade.into()),
        text_color: Some(text),
        border: border::rounded(999),
        ..container::Style::default()
    }
}

/// Unread counter; grey when the chat is muted.
pub fn badge(muted: bool, selected: bool) -> impl Fn(&Theme) -> container::Style {
    move |theme| {
        let p = palette(theme);
        let (bg, fg) = match (selected, muted) {
            (true, _) => (p.on_accent, p.accent),
            (false, true) => (p.line_strong, p.bg),
            (false, false) => (p.accent, p.on_accent),
        };
        container::Style {
            background: Some(bg.into()),
            text_color: Some(fg),
            border: border::rounded(999),
            ..container::Style::default()
        }
    }
}

/// The reply chip above the composer.
pub fn composer_chip(theme: &Theme) -> container::Style {
    let p = palette(theme);
    container::Style {
        background: Some(p.surface.into()),
        text_color: Some(p.text),
        border: Border {
            color: p.line,
            width: 1.0,
            radius: 12.0.into(),
        },
        ..container::Style::default()
    }
}

pub fn notice(theme: &Theme) -> container::Style {
    let p = palette(theme);
    container::Style {
        background: Some(p.surface.into()),
        text_color: Some(p.muted),
        border: Border {
            color: p.line,
            width: 1.0,
            radius: 12.0.into(),
        },
        ..container::Style::default()
    }
}

// --- Buttons ------------------------------------------------------------------

/// Filled ink pill: primary actions and the active folder tab.
pub fn button_primary(theme: &Theme, status: button::Status) -> button::Style {
    let p = palette(theme);
    let background = match status {
        button::Status::Active => p.accent,
        button::Status::Hovered => p.accent.scale_alpha(0.86),
        button::Status::Pressed => p.accent.scale_alpha(0.76),
        button::Status::Disabled => p.accent.scale_alpha(0.28),
    };
    button::Style {
        background: Some(background.into()),
        text_color: p.on_accent,
        border: border::rounded(999),
        ..button::Style::default()
    }
}

/// Outlined pill for secondary actions.
pub fn button_secondary(theme: &Theme, status: button::Status) -> button::Style {
    let p = palette(theme);
    let (background, border_color) = match status {
        button::Status::Active => (p.bg, p.line),
        button::Status::Hovered | button::Status::Pressed => (p.surface_alt, p.line_strong),
        button::Status::Disabled => (p.bg, p.line),
    };
    button::Style {
        background: Some(background.into()),
        text_color: if status == button::Status::Disabled {
            p.muted
        } else {
            p.text
        },
        border: Border {
            color: border_color,
            width: 1.0,
            radius: 999.0.into(),
        },
        ..button::Style::default()
    }
}

/// Borderless text button (links, small actions).
pub fn button_ghost(theme: &Theme, status: button::Status) -> button::Style {
    let p = palette(theme);
    let background = match status {
        button::Status::Hovered | button::Status::Pressed => Some(p.hover.into()),
        _ => None,
    };
    button::Style {
        background,
        text_color: if status == button::Status::Disabled {
            p.muted
        } else {
            p.text
        },
        border: border::rounded(999),
        ..button::Style::default()
    }
}

/// Inactive folder tab: transparent pill; the dashed outline is drawn by
/// [`crate::ui::widgets::dashed_pill`].
pub fn button_chip(theme: &Theme, status: button::Status) -> button::Style {
    let p = palette(theme);
    let background = match status {
        button::Status::Hovered | button::Status::Pressed => Some(p.hover.into()),
        _ => None,
    };
    button::Style {
        background,
        text_color: p.text,
        border: border::rounded(999),
        ..button::Style::default()
    }
}

/// A chat list row.
pub fn chat_row(selected: bool) -> impl Fn(&Theme, button::Status) -> button::Style {
    move |theme, status| {
        let p = palette(theme);
        let background = if selected {
            Some(p.accent.into())
        } else {
            match status {
                button::Status::Hovered | button::Status::Pressed => Some(p.hover.into()),
                _ => None,
            }
        };
        button::Style {
            background,
            text_color: if selected { p.on_accent } else { p.text },
            border: border::rounded(14),
            ..button::Style::default()
        }
    }
}

/// Small round icon button (send, close, reply).
pub fn button_icon(theme: &Theme, status: button::Status) -> button::Style {
    button_ghost(theme, status)
}

// --- Inputs ---------------------------------------------------------------------

pub fn input(theme: &Theme, status: text_input::Status) -> text_input::Style {
    let p = palette(theme);
    let border_color = match status {
        text_input::Status::Focused { .. } => p.text,
        text_input::Status::Hovered => p.line_strong,
        text_input::Status::Active | text_input::Status::Disabled => p.line,
    };
    text_input::Style {
        background: Background::Color(p.bg),
        border: Border {
            color: border_color,
            width: 1.0,
            radius: 12.0.into(),
        },
        icon: p.muted,
        placeholder: p.muted,
        value: if status == text_input::Status::Disabled {
            p.muted
        } else {
            p.text
        },
        selection: selection(p),
    }
}

pub fn editor(theme: &Theme, status: text_editor::Status) -> text_editor::Style {
    let p = palette(theme);
    let border_color = match status {
        text_editor::Status::Focused { .. } => p.line_strong,
        _ => p.line,
    };
    text_editor::Style {
        background: Background::Color(p.surface),
        border: Border {
            color: border_color,
            width: 1.0,
            radius: 20.0.into(),
        },
        placeholder: p.muted,
        value: p.text,
        selection: selection(p),
    }
}

fn selection(p: &Palette) -> Color {
    p.text.scale_alpha(0.18)
}

// --- Scrollables ----------------------------------------------------------------

/// Thin, unobtrusive scrollbars.
pub fn scrollbar(theme: &Theme, status: scrollable::Status) -> scrollable::Style {
    let p = palette(theme);
    let scroller_color = match status {
        scrollable::Status::Hovered {
            is_vertical_scrollbar_hovered: true,
            ..
        }
        | scrollable::Status::Dragged {
            is_vertical_scrollbar_dragged: true,
            ..
        } => p.muted,
        scrollable::Status::Hovered { .. } => p.line_strong,
        _ => p.line,
    };
    let rail = scrollable::Rail {
        background: None,
        border: Border::default(),
        scroller: scrollable::Scroller {
            background: scroller_color.into(),
            border: border::rounded(999),
        },
    };
    let default = scrollable::default(theme, status);
    scrollable::Style {
        container: container::Style::default(),
        vertical_rail: rail,
        horizontal_rail: rail,
        gap: None,
        auto_scroll: default.auto_scroll,
    }
}

/// Deterministic grey shade for an initials avatar, and a readable text color on it.
pub fn avatar_colors(shade_index: usize, dark: bool) -> (Color, Color) {
    const LIGHT_SHADES: [u32; 6] = [0xe4e4e8, 0xd6d6db, 0xc6c6cd, 0xa9a9b1, 0x8a8a93, 0x5e5e66];
    const DARK_SHADES: [u32; 6] = [0x2e2e34, 0x393940, 0x46464e, 0x5a5a63, 0x74747d, 0x9c9ca5];
    let shades = if dark { &DARK_SHADES } else { &LIGHT_SHADES };
    let index = shade_index % shades.len();
    let shade = hex(shades[index]);
    let text = if dark {
        if index >= 4 {
            hex(0x0b0b0c)
        } else {
            hex(0xe6e6ec)
        }
    } else if index >= 3 {
        hex(0xffffff)
    } else {
        hex(0x0b0b0c)
    };
    (shade, text)
}
