//! Application state, messages and the update loop.

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use chrono::NaiveDate;
use iced::keyboard::{self, key};
use iced::theme::Mode as ThemeMode;
use iced::widget::{image, operation, scrollable, text_editor};
use iced::{Element, Subscription, Task, Theme, window};
use telegradus_core::{
    AuthState, ChatId, ChatListId, Command, ConnectionState, CoreError, ErrorContext, Event,
    FileId, Me, MessageId,
};

use crate::backend::{self, Backend, Mode};
use crate::format;
use crate::rich::Link;
use crate::state::avatars::{self, Avatars};
use crate::state::chats::{Chat, Chats};
use crate::state::files::Files;
use crate::state::history::OpenChat;
use crate::theme;
use crate::ui;
use crate::virtual_list;

/// Fixed height of a chat list row (virtualization depends on it).
pub const CHAT_ROW_HEIGHT: f32 = 66.0;
/// Extra rows built above and below the viewport.
const CHAT_OVERSCAN: usize = 4;
/// Load the next page of chats when this close to the end of the list.
const LOAD_MORE_THRESHOLD: f32 = CHAT_ROW_HEIGHT * 6.0;
/// Load older history when this close to the top.
const LOAD_OLDER_THRESHOLD: f32 = 600.0;
/// How long to wait for the core to close before exiting anyway.
const SHUTDOWN_TIMEOUT: Duration = Duration::from_secs(3);

pub const CHAT_LIST_ID: &str = "chat-list";
pub const HISTORY_ID: &str = "history";
pub const COMPOSER_ID: &str = "composer";

/// Command-line options.
#[derive(Debug, Clone)]
pub struct Options {
    pub mode: Mode,
    /// Overrides the system light/dark preference.
    pub theme: Option<ThemeMode>,
}

#[derive(Debug, Clone)]
pub enum Message {
    // Core connection
    CoreReady(Backend),
    Core(Vec<Event>),

    // Window and system
    CloseRequested,
    ForceExit,
    SystemTheme(ThemeMode),
    ToggleTheme,
    Tick,
    Escape,
    /// Tab (`true`) or Shift+Tab (`false`) moves the keyboard focus.
    FocusNext(bool),
    DismissToast(u64),

    // Auth forms
    Auth(ui::auth::Msg),

    // Chat list
    SelectList(ChatListId),
    ChatListScrolled(scrollable::Viewport),
    OpenChat(ChatId),
    AvatarsReady(Vec<(FileId, Result<image::Handle, String>)>),
    AskLogOut,
    CancelLogOut,
    LogOut,

    // Chat pane
    CloseChat,
    HistoryScrolled(scrollable::Viewport),
    Seen(MessageId),
    FlushViews,
    MediaShown(FileId),
    Hover(MessageId),
    Unhover(MessageId),
    ReplyTo(MessageId),
    CancelReply,
    Composer(text_editor::Action),
    Send,
    Link(MessageId, Link),
    OpenUrl(String),
    OpenFile(FileId),
    Download(FileId),
}

/// Scroll position of the chat list, used for virtualization.
#[derive(Debug, Clone, Copy)]
pub struct ListViewport {
    pub offset: f32,
    pub height: f32,
}

pub struct App {
    pub mode: Mode,
    backend: Option<Backend>,
    /// Commands issued before the core connected.
    queued: Vec<Command>,

    pub auth: AuthState,
    pub auth_form: ui::auth::Form,
    pub connection: ConnectionState,
    pub me: Option<Arc<Me>>,

    pub chats: Chats,
    pub files: Files,
    avatars: Avatars,
    pub selected_list: ChatListId,
    pub list_viewport: ListViewport,

    pub open: Option<OpenChat>,
    pub composer: text_editor::Content,
    pub reply_to: Option<MessageId>,
    pending_views: Vec<MessageId>,

    pub toast: Option<(u64, String)>,
    toast_seq: u64,
    pub logout_armed: bool,

    theme_override: Option<ThemeMode>,
    system_theme: ThemeMode,
    themes: (Theme, Theme),

    pub today: NaiveDate,
    closing: bool,
    #[cfg_attr(not(feature = "demo"), allow(dead_code))]
    auto_open: Option<ChatId>,
}

impl App {
    pub fn new(options: Options) -> (Self, Task<Message>) {
        #[cfg(feature = "demo")]
        let auto_open = match options.mode {
            Mode::Demo(crate::demo::Screen::Chat) => Some(crate::demo::SHOWCASE_CHAT),
            _ => None,
        };
        #[cfg(not(feature = "demo"))]
        let auto_open = None;

        let app = Self {
            mode: options.mode,
            backend: None,
            queued: Vec::new(),
            auth: AuthState::Initializing,
            auth_form: ui::auth::Form::default(),
            connection: ConnectionState::Connecting,
            me: None,
            chats: Chats::default(),
            files: Files::default(),
            avatars: Avatars::default(),
            selected_list: ChatListId::Main,
            list_viewport: ListViewport {
                offset: 0.0,
                height: 900.0,
            },
            open: None,
            composer: text_editor::Content::new(),
            reply_to: None,
            pending_views: Vec::new(),
            toast: None,
            toast_seq: 0,
            logout_armed: false,
            theme_override: options.theme,
            system_theme: ThemeMode::Light,
            themes: (
                theme::theme_for(ThemeMode::Light),
                theme::theme_for(ThemeMode::Dark),
            ),
            today: format::today(),
            closing: false,
            auto_open,
        };
        (app, iced::system::theme().map(Message::SystemTheme))
    }

    pub fn title(&self) -> String {
        match (&self.open, self.auth == AuthState::Ready) {
            (Some(open), true) => match self.chats.get(open.id) {
                Some(chat) => format!("{} — Telegradus", chat.summary.title),
                None => "Telegradus".to_owned(),
            },
            _ => "Telegradus".to_owned(),
        }
    }

    pub fn theme(&self) -> Theme {
        match self.theme_override.unwrap_or(self.system_theme) {
            ThemeMode::Dark => self.themes.1.clone(),
            ThemeMode::Light | ThemeMode::None => self.themes.0.clone(),
        }
    }

    pub fn is_dark(&self) -> bool {
        self.theme_override.unwrap_or(self.system_theme) == ThemeMode::Dark
    }

    /// Named colors of the active theme, for widgets that take plain colors.
    pub fn palette(&self) -> &'static theme::Palette {
        if self.is_dark() {
            &theme::DARK
        } else {
            &theme::LIGHT
        }
    }

    pub fn subscription(&self) -> Subscription<Message> {
        let mut subscriptions = vec![
            backend::subscription(self.mode),
            window::close_requests().map(|_| Message::CloseRequested),
            iced::system::theme_changes().map(Message::SystemTheme),
            keyboard::listen().filter_map(|event| match event {
                keyboard::Event::KeyPressed {
                    key: keyboard::Key::Named(key::Named::Escape),
                    ..
                } => Some(Message::Escape),
                keyboard::Event::KeyPressed {
                    key: keyboard::Key::Named(key::Named::Tab),
                    modifiers,
                    ..
                } => Some(Message::FocusNext(!modifiers.shift())),
                _ => None,
            }),
        ];
        // Tick once a second only while a resend countdown is visible.
        if self.auth_form.resend_countdown(&self.auth).is_some() {
            subscriptions.push(iced::time::every(Duration::from_secs(1)).map(|_| Message::Tick));
        }
        Subscription::batch(subscriptions)
    }

    pub fn view(&self) -> Element<'_, Message> {
        ui::view(self)
    }

    /// Sends a command, or queues it until the core is connected.
    pub fn send(&mut self, command: Command) {
        match &self.backend {
            Some(backend) => backend.send(command),
            None => self.queued.push(command),
        }
    }

    pub fn update(&mut self, message: Message) -> Task<Message> {
        if !matches!(message, Message::Core(_)) {
            tracing::trace!(?message, "update");
        }
        match message {
            Message::CoreReady(backend) => {
                for command in self.queued.drain(..) {
                    backend.send(command);
                }
                self.backend = Some(backend);
                Task::none()
            }
            Message::Core(events) => {
                self.today = format::today();
                let tasks: Vec<Task<Message>> = events
                    .into_iter()
                    .map(|event| self.handle_event(event))
                    .collect();
                self.chats.refresh_unread();
                let after = self.after_list_change();
                Task::batch(tasks).chain(after)
            }
            Message::CloseRequested => self.close(),
            Message::ForceExit => {
                tracing::warn!("core did not close in time; exiting anyway");
                iced::exit()
            }
            Message::SystemTheme(mode) => {
                self.system_theme = mode;
                Task::none()
            }
            Message::ToggleTheme => {
                self.theme_override = Some(if self.is_dark() {
                    ThemeMode::Light
                } else {
                    ThemeMode::Dark
                });
                Task::none()
            }
            Message::Tick => Task::none(),
            Message::Escape => self.escape(),
            Message::FocusNext(true) => operation::focus_next(),
            Message::FocusNext(false) => operation::focus_previous(),
            Message::DismissToast(seq) => {
                if self.toast.as_ref().is_some_and(|(id, _)| *id == seq) {
                    self.toast = None;
                }
                Task::none()
            }
            Message::Auth(msg) => {
                let (command, task) = self.auth_form.update(msg, &self.auth);
                if let Some(command) = command {
                    self.send(command);
                }
                task
            }
            Message::SelectList(list) => self.select_list(list),
            Message::ChatListScrolled(viewport) => {
                self.list_viewport = ListViewport {
                    offset: viewport.absolute_offset().y,
                    height: viewport.bounds().height,
                };
                self.after_list_change()
            }
            Message::OpenChat(chat_id) => self.open_chat(chat_id),
            Message::AvatarsReady(results) => {
                for (file_id, result) in results {
                    self.avatars.finish(file_id, result);
                }
                Task::none()
            }
            Message::AskLogOut => {
                self.logout_armed = true;
                Task::none()
            }
            Message::CancelLogOut => {
                self.logout_armed = false;
                Task::none()
            }
            Message::LogOut => {
                self.logout_armed = false;
                self.close_chat();
                self.send(Command::LogOut);
                Task::none()
            }
            Message::CloseChat => {
                self.close_chat();
                Task::none()
            }
            Message::HistoryScrolled(viewport) => self.history_scrolled(viewport),
            Message::Seen(id) => self.message_seen(id),
            Message::FlushViews => {
                if let Some(open) = &self.open
                    && !self.pending_views.is_empty()
                {
                    let chat_id = open.id;
                    let message_ids = std::mem::take(&mut self.pending_views);
                    self.send(Command::ViewMessages {
                        chat_id,
                        message_ids,
                    });
                }
                Task::none()
            }
            Message::MediaShown(file_id) | Message::Download(file_id) => {
                if self.files.request(file_id) {
                    self.send(Command::DownloadFile(file_id));
                }
                Task::none()
            }
            Message::Hover(id) => {
                if let Some(open) = &mut self.open {
                    open.hovered = Some(id);
                }
                Task::none()
            }
            Message::Unhover(id) => {
                if let Some(open) = &mut self.open
                    && open.hovered == Some(id)
                {
                    open.hovered = None;
                }
                Task::none()
            }
            Message::ReplyTo(id) => {
                self.reply_to = Some(id);
                operation::focus(COMPOSER_ID)
            }
            Message::CancelReply => {
                self.reply_to = None;
                Task::none()
            }
            Message::Composer(action) => {
                self.composer.perform(action);
                Task::none()
            }
            Message::Send => self.send_composer(),
            Message::Link(message_id, link) => match link {
                Link::Url(url) => {
                    open_external(url.to_string());
                    Task::none()
                }
                Link::Spoiler => {
                    if let Some(open) = &mut self.open {
                        open.toggle_spoiler(message_id);
                    }
                    Task::none()
                }
            },
            Message::OpenUrl(url) => {
                open_external(url);
                Task::none()
            }
            Message::OpenFile(file_id) => {
                if let Some(path) = self.files.get(file_id).and_then(|f| f.local_path.clone()) {
                    open_external(path.to_string_lossy().into_owned());
                }
                Task::none()
            }
        }
    }

    fn handle_event(&mut self, event: Event) -> Task<Message> {
        match event {
            Event::Auth(state) => {
                if state == self.auth {
                    return Task::none();
                }
                if self.auth == AuthState::Ready {
                    // Logged out (or TDLib restarted): forget the session's data.
                    self.reset_session();
                }
                let task = self.auth_form.on_state_change(&state);
                self.auth = state;
                task
            }
            Event::Connection(state) => {
                self.connection = state;
                Task::none()
            }
            Event::Me(me) => {
                self.me = Some(me);
                Task::none()
            }
            Event::ChatsUpdated(chats) => {
                for chat in chats {
                    if let Some(photo) = &chat.photo {
                        self.files.register(photo, false);
                    }
                    self.chats.upsert(chat);
                }
                Task::none()
            }
            Event::ChatListUpdated {
                list,
                entries,
                has_more,
            } => {
                self.chats.set_list(list, entries, has_more);
                self.auto_open_chat()
            }
            Event::FoldersChanged(folders) => {
                let still_exists = match self.selected_list {
                    ChatListId::Folder(id) => folders.iter().any(|f| f.id == id),
                    _ => true,
                };
                self.chats.set_folders(folders);
                if still_exists {
                    Task::none()
                } else {
                    self.select_list(ChatListId::Main)
                }
            }
            Event::History {
                chat_id,
                messages,
                is_initial,
                has_more_older,
            } => {
                let Some(open) = self.open.as_mut().filter(|open| open.id == chat_id) else {
                    return Task::none();
                };
                for message in &messages {
                    register_media(&mut self.files, &message.content);
                }
                open.apply_page(messages, is_initial, has_more_older);
                // Short histories never scroll, so ask for more right away.
                if !open.viewport_seen && open.has_more_older && open.items.len() < 20 {
                    return self.load_older();
                }
                if is_initial {
                    operation::snap_to(HISTORY_ID, scrollable::RelativeOffset::START)
                } else {
                    Task::none()
                }
            }
            Event::MessageAdded(message) => {
                let Some(open) = self.open.as_mut().filter(|open| open.id == message.chat_id)
                else {
                    return Task::none();
                };
                register_media(&mut self.files, &message.content);
                let outgoing = message.is_outgoing;
                open.push(message);
                if outgoing {
                    operation::snap_to(HISTORY_ID, scrollable::RelativeOffset::START)
                } else {
                    Task::none()
                }
            }
            Event::MessageUpdated(message) => {
                if let Some(open) = self.open.as_mut().filter(|open| open.id == message.chat_id) {
                    register_media(&mut self.files, &message.content);
                    open.replace(message);
                }
                Task::none()
            }
            Event::MessageSent { old_id, message } => {
                if let Some(open) = self.open.as_mut().filter(|open| open.id == message.chat_id) {
                    register_media(&mut self.files, &message.content);
                    open.sent(old_id, message);
                }
                Task::none()
            }
            Event::MessagesDeleted {
                chat_id,
                message_ids,
            } => {
                if let Some(open) = self.open.as_mut().filter(|open| open.id == chat_id) {
                    open.delete(&message_ids);
                }
                if self.reply_to.is_some_and(|id| message_ids.contains(&id)) {
                    self.reply_to = None;
                }
                Task::none()
            }
            Event::FileUpdated(file) => {
                self.files.update(file);
                Task::none()
            }
            Event::Error(error) => self.handle_error(error),
            Event::Closed => {
                if self.closing {
                    tracing::info!("core closed, exiting");
                    iced::exit()
                } else {
                    tracing::warn!("core closed unexpectedly");
                    Task::none()
                }
            }
        }
    }

    fn handle_error(&mut self, error: CoreError) -> Task<Message> {
        tracing::warn!(context = ?error.context, code = error.code, message = %error.message, "core error");
        match error.context {
            ErrorContext::Auth => {
                self.auth_form.on_error(error.message);
                Task::none()
            }
            ErrorContext::ChatList => {
                self.chats.loading_failed();
                Task::none()
            }
            ErrorContext::History => {
                if let Some(open) = &mut self.open {
                    open.loading_older = false;
                }
                self.show_toast(error.message)
            }
            ErrorContext::File => {
                self.files.clear_requests();
                self.show_toast(error.message)
            }
            ErrorContext::Send | ErrorContext::Other => self.show_toast(error.message),
        }
    }

    /// Drops everything that belongs to the logged-in account.
    fn reset_session(&mut self) {
        self.close_chat();
        self.pending_views.clear();
        self.me = None;
        self.chats = Chats::default();
        self.files = Files::default();
        self.avatars = Avatars::default();
        self.selected_list = ChatListId::Main;
        self.list_viewport.offset = 0.0;
        self.logout_armed = false;
    }

    fn show_toast(&mut self, text: String) -> Task<Message> {
        self.toast_seq += 1;
        let seq = self.toast_seq;
        self.toast = Some((seq, text));
        delay(Duration::from_secs(5), Message::DismissToast(seq))
    }

    fn close(&mut self) -> Task<Message> {
        if self.closing {
            return Task::none();
        }
        match &self.backend {
            Some(backend) => {
                self.closing = true;
                backend.send(Command::Shutdown);
                delay(SHUTDOWN_TIMEOUT, Message::ForceExit)
            }
            None => iced::exit(),
        }
    }

    fn escape(&mut self) -> Task<Message> {
        if self.reply_to.take().is_none() {
            self.close_chat();
        }
        Task::none()
    }

    fn select_list(&mut self, list: ChatListId) -> Task<Message> {
        if self.selected_list == list {
            return operation::snap_to(CHAT_LIST_ID, scrollable::RelativeOffset::START);
        }
        self.selected_list = list;
        self.list_viewport.offset = 0.0;
        if self.chats.is_untouched(list) && self.chats.begin_load(list) {
            self.send(Command::LoadChats(list));
        }
        let after = self.after_list_change();
        operation::snap_to(CHAT_LIST_ID, scrollable::RelativeOffset::START).chain(after)
    }

    /// Requests avatars of the visible rows and the next page when near the end.
    fn after_list_change(&mut self) -> Task<Message> {
        let list_id = self.selected_list;
        let Some(list) = self.chats.list(list_id) else {
            return Task::none();
        };
        let entries = list.entries.clone();
        let has_more = list.has_more;
        let viewport = self.list_viewport;
        let window = virtual_list::window(
            entries.len(),
            CHAT_ROW_HEIGHT,
            viewport.offset,
            viewport.height,
            CHAT_OVERSCAN,
        );
        let open_chat = self.open.as_ref().map(|open| open.id);
        let visible = entries[window.rows]
            .iter()
            .map(|e| e.chat_id)
            .chain(open_chat);
        let photos: Vec<_> = visible
            .filter_map(|id| self.chats.get(id).and_then(|c| c.summary.photo.clone()))
            .collect();
        let jobs: Vec<_> = photos
            .into_iter()
            .filter_map(|photo| self.ensure_avatar(photo))
            .collect();
        let near_end = virtual_list::near_end(
            entries.len(),
            CHAT_ROW_HEIGHT,
            viewport.offset,
            viewport.height,
            LOAD_MORE_THRESHOLD,
        );
        if has_more && near_end && self.chats.begin_load(list_id) {
            self.send(Command::LoadChats(list_id));
        }
        if jobs.is_empty() {
            return Task::none();
        }
        // One blocking task per batch keeps the thread count low.
        Task::perform(
            async move {
                let ids: Vec<FileId> = jobs.iter().map(|(id, _)| *id).collect();
                tokio::task::spawn_blocking(move || {
                    jobs.into_iter()
                        .map(|(id, path)| (id, avatars::make_thumbnail(&path)))
                        .collect()
                })
                .await
                .unwrap_or_else(|error| {
                    ids.into_iter()
                        .map(|id| (id, Err(error.to_string())))
                        .collect()
                })
            },
            Message::AvatarsReady,
        )
    }

    /// Requests the download of an avatar, or returns the thumbnail job for a
    /// downloaded one that has no thumbnail yet.
    fn ensure_avatar(&mut self, photo: telegradus_core::FileRef) -> Option<(FileId, PathBuf)> {
        let file = self.files.get(photo.id).cloned().unwrap_or(photo);
        let Some(path) = file.local_path else {
            if self.files.request(file.id) {
                self.send(Command::DownloadFile(file.id));
            }
            return None;
        };
        self.avatars.touch(file.id).then_some((file.id, path))
    }

    /// The round thumbnail of a chat photo, once ready.
    pub fn avatar(&self, chat: &Chat) -> Option<&image::Handle> {
        chat.summary
            .photo
            .as_ref()
            .and_then(|photo| self.avatars.get(photo.id))
    }

    fn auto_open_chat(&mut self) -> Task<Message> {
        match self.auto_open {
            Some(chat_id) if self.chats.get(chat_id).is_some() => {
                self.auto_open = None;
                self.open_chat(chat_id)
            }
            _ => Task::none(),
        }
    }

    fn open_chat(&mut self, chat_id: ChatId) -> Task<Message> {
        if self.open.as_ref().is_some_and(|open| open.id == chat_id) {
            return operation::focus(COMPOSER_ID);
        }
        self.close_chat();
        self.open = Some(OpenChat::new(chat_id));
        self.send(Command::OpenChat(chat_id));
        Task::batch([
            operation::snap_to(HISTORY_ID, scrollable::RelativeOffset::START),
            operation::focus(COMPOSER_ID),
            self.after_list_change(),
        ])
    }

    fn close_chat(&mut self) {
        if let Some(open) = self.open.take() {
            if !self.pending_views.is_empty() {
                let message_ids = std::mem::take(&mut self.pending_views);
                self.send(Command::ViewMessages {
                    chat_id: open.id,
                    message_ids,
                });
            }
            self.send(Command::CloseChat(open.id));
        }
        self.reply_to = None;
        self.composer = text_editor::Content::new();
    }

    fn history_scrolled(&mut self, viewport: scrollable::Viewport) -> Task<Message> {
        let Some(open) = &mut self.open else {
            return Task::none();
        };
        open.viewport_seen = true;
        // The history is anchored to the bottom: the reversed offset is the
        // distance from the top of the content.
        let from_top = viewport.absolute_offset_reversed().y;
        if from_top < LOAD_OLDER_THRESHOLD {
            self.load_older()
        } else {
            Task::none()
        }
    }

    fn load_older(&mut self) -> Task<Message> {
        let Some(open) = &mut self.open else {
            return Task::none();
        };
        if open.loading_older || !open.has_more_older || !open.loaded {
            return Task::none();
        }
        let Some(from_message_id) = open.oldest_id() else {
            return Task::none();
        };
        open.loading_older = true;
        let chat_id = open.id;
        self.send(Command::LoadOlder {
            chat_id,
            from_message_id,
        });
        Task::none()
    }

    fn message_seen(&mut self, id: MessageId) -> Task<Message> {
        let Some(open) = &mut self.open else {
            return Task::none();
        };
        if !open.mark_viewed(id) {
            return Task::none();
        }
        self.pending_views.push(id);
        if self.pending_views.len() == 1 {
            // Coalesce the burst of messages that appear together.
            delay(Duration::from_millis(250), Message::FlushViews)
        } else {
            Task::none()
        }
    }

    fn send_composer(&mut self) -> Task<Message> {
        let Some(open) = &self.open else {
            return Task::none();
        };
        let chat_id = open.id;
        if !self
            .chats
            .get(chat_id)
            .is_some_and(|c| c.summary.can_send_messages)
        {
            return Task::none();
        }
        let text = self.composer.text();
        if text.trim().is_empty() {
            return Task::none();
        }
        let command = Command::SendText {
            chat_id,
            text: text.trim_end().to_owned(),
            reply_to: self.reply_to.take(),
        };
        self.send(command);
        self.composer = text_editor::Content::new();
        Task::batch([
            operation::snap_to(HISTORY_ID, scrollable::RelativeOffset::START),
            operation::focus(COMPOSER_ID),
        ])
    }
}

/// Registers the files of a message so their download state is tracked.
fn register_media(files: &mut Files, content: &telegradus_core::MessageContent) {
    match content {
        telegradus_core::MessageContent::Photo { file, .. } => files.register(file, true),
        telegradus_core::MessageContent::Document { file, .. } => files.register(file, false),
        _ => {}
    }
}

/// A message delivered after `duration`, without blocking anything.
fn delay(duration: Duration, message: Message) -> Task<Message> {
    Task::perform(tokio::time::sleep(duration), move |()| message)
}

/// Opens a URL or file with the system handler, off the UI thread.
fn open_external(target: String) {
    let spawned = std::thread::Builder::new()
        .name("open-external".into())
        .spawn(move || {
            let result = if target.contains("://")
                || target.starts_with("mailto:")
                || target.starts_with("tel:")
            {
                opener::open_browser(&target)
            } else {
                opener::open(&target)
            };
            if let Err(error) = result {
                tracing::warn!(%error, %target, "cannot open");
            }
        });
    if let Err(error) = spawned {
        tracing::warn!(%error, "cannot spawn opener thread");
    }
}
