//! Views. Everything here only reads state; no work beyond building widgets.

pub mod auth;
mod chat;
pub mod icons;
mod message;
mod sidebar;
pub mod widgets;

use iced::widget::{Space, column, container, row, stack, text};
use iced::{Element, Length, Padding};
use telegradus_core::AuthState;

use crate::app::{App, Message};
use crate::theme;

/// Width of the chat list sidebar.
pub const SIDEBAR_WIDTH: f32 = 320.0;

pub fn view(app: &App) -> Element<'_, Message> {
    let content = match app.auth {
        AuthState::Ready => main(app),
        _ => auth::view(&app.auth_form, &app.auth),
    };
    // Always a stack, so showing a toast does not rebuild (and reset) the
    // widget state of the content underneath.
    let toast = app.toast.as_ref().map(|(_, message)| toast(message));
    stack![content, toast].into()
}

fn main(app: &App) -> Element<'_, Message> {
    let divider = container(Space::new())
        .width(1)
        .height(Length::Fill)
        .style(theme::line);
    row![
        container(sidebar::view(app))
            .width(SIDEBAR_WIDTH)
            .height(Length::Fill)
            .style(theme::sidebar),
        divider,
        container(chat::view(app))
            .width(Length::Fill)
            .height(Length::Fill)
            .style(theme::pane),
    ]
    .into()
}

fn toast(message: &str) -> Element<'_, Message> {
    let pill = container(text(message).size(13))
        .padding(Padding::from([9.0, 16.0]))
        .style(|theme| {
            let p = theme::palette(theme);
            container::Style {
                background: Some(p.accent.into()),
                text_color: Some(p.on_accent),
                border: iced::border::rounded(999),
                ..container::Style::default()
            }
        });
    container(column![Space::new().height(Length::Fill), pill])
        .center_x(Length::Fill)
        .height(Length::Fill)
        .padding(Padding::default().bottom(90))
        .into()
}
