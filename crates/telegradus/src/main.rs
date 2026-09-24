//! Telegradus: a fast, low-memory native Telegram client.
//!
//! Command line:
//! - `--theme=light|dark` overrides the system color scheme;
//! - `--demo[=setup|phone|qr|code|password|main|chat]` (builds with the `demo`
//!   feature) runs against an in-app fake core with canned data.
//!
//! `RUST_LOG` controls logging (default `warn,telegradus=info`).
#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

mod app;
mod backend;
#[cfg(feature = "demo")]
mod demo;
mod format;
mod rich;
mod state;
mod theme;
mod ui;
mod virtual_list;

use iced::theme::Mode as ThemeMode;
use iced::{Size, window};

use crate::app::{App, Options};
use crate::backend::Mode;

fn main() -> iced::Result {
    init_tracing();
    let options = parse_args(std::env::args().skip(1));

    iced::application(move || App::new(options.clone()), App::update, App::view)
        .title(App::title)
        .subscription(App::subscription)
        .theme(App::theme)
        .font(include_bytes!("../assets/fonts/InterTight-Regular.ttf").as_slice())
        .font(include_bytes!("../assets/fonts/InterTight-Medium.ttf").as_slice())
        .font(include_bytes!("../assets/fonts/InterTight-SemiBold.ttf").as_slice())
        .font(include_bytes!("../assets/fonts/InterTight-Italic.ttf").as_slice())
        .font(include_bytes!("../assets/fonts/JetBrainsMono-Regular.ttf").as_slice())
        .default_font(theme::SANS)
        .window(window_settings())
        .run()?;
    tracing::info!("event loop finished");
    Ok(())
}

fn window_settings() -> window::Settings {
    window::Settings {
        size: Size::new(1100.0, 720.0),
        min_size: Some(Size::new(720.0, 480.0)),
        position: window::Position::Centered,
        exit_on_close_request: false,
        #[cfg(target_os = "linux")]
        platform_specific: window::settings::PlatformSpecific {
            application_id: "telegradus".to_owned(),
            ..Default::default()
        },
        ..window::Settings::default()
    }
}

fn init_tracing() {
    use tracing_subscriber::EnvFilter;

    let filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new("warn,telegradus=info"));
    tracing_subscriber::fmt().with_env_filter(filter).init();
}

/// `--demo[=<screen>]` (with the `demo` feature) and `--theme=light|dark`.
fn parse_args(args: impl Iterator<Item = String>) -> Options {
    let mut options = Options {
        mode: Mode::Real,
        theme: None,
    };
    for arg in args {
        if let Some(value) = arg.strip_prefix("--theme=") {
            options.theme = match value {
                "light" => Some(ThemeMode::Light),
                "dark" => Some(ThemeMode::Dark),
                _ => {
                    tracing::warn!(value, "unknown theme, expected light or dark");
                    None
                }
            };
        } else if arg == "--demo" || arg.starts_with("--demo=") {
            options.mode = demo_mode(arg.strip_prefix("--demo=").unwrap_or(""));
        } else {
            tracing::warn!(arg, "unknown argument");
        }
    }
    options
}

#[cfg(feature = "demo")]
fn demo_mode(screen: &str) -> Mode {
    match demo::Screen::parse(screen) {
        Some(screen) => Mode::Demo(screen),
        None => {
            tracing::warn!(
                screen,
                "unknown demo screen; use setup, phone, qr, code, password, main or chat"
            );
            Mode::Demo(demo::Screen::Main)
        }
    }
}

#[cfg(not(feature = "demo"))]
fn demo_mode(_screen: &str) -> Mode {
    tracing::warn!("this build has no demo mode; rebuild with `--features demo`");
    Mode::Real
}
