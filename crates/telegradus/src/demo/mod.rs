//! An in-app fake core for screenshots and UI work without a Telegram account.
//!
//! It speaks the real [`Command`]/[`Event`] API from its own thread, so the UI
//! code paths are exactly the ones used with TDLib.

mod data;
mod images;

use std::path::PathBuf;
use std::sync::Arc;
use std::sync::mpsc::{self, RecvTimeoutError};
use std::time::{Duration, Instant};

use telegradus_core::{
    AuthState, ChatId, ChatListEntry, ChatListId, CodeInfo, CodeKind, Command, ConnectionState,
    CoreError, ErrorContext, Event, FileId, FormattedText, Message, MessageContent, MessageId,
    Sender, SendingState,
};
use tokio::sync::mpsc::UnboundedReceiver;

pub use data::SHOWCASE_CHAT;
use data::{DemoFile, World};

/// Which screen the demo starts on.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Screen {
    Setup,
    Phone,
    Qr,
    Code,
    Password,
    Main,
    Chat,
}

impl Screen {
    pub fn parse(name: &str) -> Option<Self> {
        Some(match name {
            "setup" => Self::Setup,
            "phone" => Self::Phone,
            "qr" => Self::Qr,
            "code" => Self::Code,
            "password" => Self::Password,
            "main" | "" => Self::Main,
            "chat" => Self::Chat,
            _ => return None,
        })
    }
}

/// Command sender of the demo core; the counterpart of `telegradus_core::Handle`.
#[derive(Debug, Clone)]
pub struct DemoHandle(mpsc::Sender<Command>);

impl DemoHandle {
    pub fn send(&self, command: Command) {
        let _ = self.0.send(command);
    }
}

/// Starts the demo core on its own thread.
pub fn start(screen: Screen) -> (DemoHandle, UnboundedReceiver<Event>) {
    let (command_tx, command_rx) = mpsc::channel();
    let (event_tx, event_rx) = tokio::sync::mpsc::unbounded_channel();
    let spawned = std::thread::Builder::new()
        .name("telegradus-demo".into())
        .spawn(move || Demo::new(event_tx).run(screen, command_rx));
    if let Err(error) = spawned {
        tracing::error!(%error, "cannot start the demo core");
    }
    (DemoHandle(command_tx), event_rx)
}

struct Demo {
    tx: tokio::sync::mpsc::UnboundedSender<Event>,
    world: World,
    scheduled: Vec<(Instant, Event)>,
    main_page_sent: usize,
    next_pending: MessageId,
    files_dir: PathBuf,
}

const PENDING_BASE: MessageId = 9_000_000_000;

fn code_info(phone: &str, kind: CodeKind, timeout: i32) -> CodeInfo {
    CodeInfo {
        phone_number: phone.to_owned(),
        kind,
        length: 5,
        can_resend: true,
        timeout_secs: timeout,
    }
}

fn auth_error(message: &str) -> Event {
    Event::Error(CoreError {
        context: ErrorContext::Auth,
        code: 400,
        message: message.to_owned(),
    })
}

impl Demo {
    fn new(tx: tokio::sync::mpsc::UnboundedSender<Event>) -> Self {
        Self {
            tx,
            world: data::world(),
            scheduled: Vec::new(),
            main_page_sent: 0,
            next_pending: PENDING_BASE,
            files_dir: std::env::temp_dir().join("telegradus-demo"),
        }
    }

    fn run(mut self, screen: Screen, commands: mpsc::Receiver<Command>) {
        self.boot(screen);
        loop {
            self.flush_due();
            let timeout = self
                .scheduled
                .iter()
                .map(|(at, _)| at.saturating_duration_since(Instant::now()))
                .min()
                .unwrap_or(Duration::from_secs(3600));
            match commands.recv_timeout(timeout) {
                Ok(Command::Shutdown) => {
                    self.emit(Event::Auth(AuthState::Closing));
                    std::thread::sleep(Duration::from_millis(150));
                    self.emit(Event::Auth(AuthState::Closed));
                    self.emit(Event::Closed);
                    return;
                }
                Ok(command) => self.handle(command),
                Err(RecvTimeoutError::Timeout) => {}
                Err(RecvTimeoutError::Disconnected) => return,
            }
        }
    }

    fn emit(&self, event: Event) {
        let _ = self.tx.send(event);
    }

    fn later(&mut self, millis: u64, event: Event) {
        self.scheduled
            .push((Instant::now() + Duration::from_millis(millis), event));
    }

    fn flush_due(&mut self) {
        let now = Instant::now();
        let mut due = Vec::new();
        self.scheduled.retain(|(at, event)| {
            if *at <= now {
                due.push((*at, event.clone()));
                false
            } else {
                true
            }
        });
        due.sort_by_key(|(at, _)| *at);
        for (_, event) in due {
            self.emit(event);
        }
    }

    fn boot(&mut self, screen: Screen) {
        let phone = "+7 900 123-45-67";
        match screen {
            Screen::Setup => self.emit(Event::Auth(AuthState::NeedApiCredentials)),
            Screen::Phone => {
                self.emit(Event::Auth(AuthState::Initializing));
                self.later(300, Event::Auth(AuthState::WaitPhoneNumber));
            }
            Screen::Qr => self.emit(Event::Auth(AuthState::WaitQrConfirmation {
                link: "tg://login?token=AQJ4c2VjcmV0LXRva2VuLWZvci10ZWxlZ3JhZHVzLWRlbW8".into(),
            })),
            Screen::Code => self.emit(Event::Auth(AuthState::WaitCode(code_info(
                phone,
                CodeKind::Sms,
                45,
            )))),
            Screen::Password => self.emit(Event::Auth(AuthState::WaitPassword {
                hint: "кличка первой собаки".into(),
                has_recovery_email: true,
            })),
            Screen::Main | Screen::Chat => self.log_in(),
        }
    }

    fn log_in(&mut self) {
        self.emit(Event::Connection(ConnectionState::Updating));
        self.emit(Event::Auth(AuthState::Ready));
        self.emit(Event::Me(Arc::new(self.world.me.clone())));
        self.emit(Event::FoldersChanged(data::folders(&self.world)));
        let chats = self.world.chats.values().cloned().map(Arc::new).collect();
        self.emit(Event::ChatsUpdated(chats));
        self.send_main(data::FIRST_PAGE);
        self.later(900, Event::Connection(ConnectionState::Ready));
    }

    fn send_main(&mut self, count: usize) {
        let count = count.min(self.world.main.len());
        self.main_page_sent = count;
        let entries: Arc<[ChatListEntry]> = self.world.main[..count].into();
        self.emit(Event::ChatListUpdated {
            list: ChatListId::Main,
            entries,
            has_more: count < self.world.main.len(),
        });
    }

    fn handle(&mut self, command: Command) {
        match command {
            Command::SetApiCredentials { .. } => {
                self.emit(Event::Auth(AuthState::Initializing));
                self.later(700, Event::Auth(AuthState::WaitPhoneNumber));
            }
            Command::SubmitPhoneNumber(phone) => {
                let digits = phone.chars().filter(char::is_ascii_digit).count();
                if digits < 10 {
                    self.later(
                        300,
                        auth_error("Неверный номер телефона. Проверьте код страны и цифры."),
                    );
                } else {
                    let info = code_info(&phone, CodeKind::TelegramMessage, 30);
                    self.later(600, Event::Auth(AuthState::WaitCode(info)));
                }
            }
            Command::RequestQrCode => self.later(
                400,
                Event::Auth(AuthState::WaitQrConfirmation {
                    link: "tg://login?token=AQJ4c2VjcmV0LXRva2VuLWZvci10ZWxlZ3JhZHVzLWRlbW8".into(),
                }),
            ),
            Command::SubmitCode(code) => {
                if code.len() != 5 || code.starts_with('0') {
                    self.later(400, auth_error("Неверный код. Попробуйте ещё раз."));
                } else {
                    self.later(
                        500,
                        Event::Auth(AuthState::WaitPassword {
                            hint: "кличка первой собаки".into(),
                            has_recovery_email: true,
                        }),
                    );
                }
            }
            Command::ResendCode => {
                let info = code_info("+7 900 123-45-67", CodeKind::Sms, 60);
                self.later(400, Event::Auth(AuthState::WaitCode(info)));
            }
            Command::SubmitPassword(password) => {
                if password == "error" || password.is_empty() {
                    self.later(500, auth_error("Неверный пароль."));
                } else {
                    std::thread::sleep(Duration::from_millis(400));
                    self.log_in();
                }
            }
            Command::LogOut => {
                self.emit(Event::Auth(AuthState::LoggingOut));
                self.later(600, Event::Auth(AuthState::WaitPhoneNumber));
            }
            Command::LoadChats(list) => self.load_chats(list),
            Command::OpenChat(chat_id) => self.open_chat(chat_id),
            Command::CloseChat(_) => {}
            Command::LoadOlder { chat_id, .. } => {
                let page = self.world.older.get_mut(&chat_id).and_then(|pages| {
                    if pages.is_empty() {
                        None
                    } else {
                        Some((pages.remove(0), !pages.is_empty()))
                    }
                });
                let (messages, has_more_older) = page.unwrap_or_default();
                self.later(
                    350,
                    Event::History {
                        chat_id,
                        messages,
                        is_initial: false,
                        has_more_older,
                    },
                );
            }
            Command::ViewMessages {
                chat_id,
                message_ids,
            } => self.view(chat_id, &message_ids),
            Command::SendText {
                chat_id,
                text,
                reply_to,
            } => self.send_text(chat_id, text, reply_to),
            Command::DownloadFile(id) => self.download(id),
            Command::Shutdown => {}
        }
    }

    fn load_chats(&mut self, list: ChatListId) {
        let (entries, has_more): (Arc<[ChatListEntry]>, bool) = match list {
            ChatListId::Main => {
                let count = self.main_page_sent + 30;
                self.later(250, Event::Connection(ConnectionState::Ready));
                let count = count.min(self.world.main.len());
                self.main_page_sent = count;
                (
                    self.world.main[..count].into(),
                    count < self.world.main.len(),
                )
            }
            ChatListId::Archive => (self.world.archive.as_slice().into(), false),
            ChatListId::Folder(id) => {
                let entries = self
                    .world
                    .folders
                    .iter()
                    .find(|(folder, _)| folder.id == id)
                    .map(|(_, entries)| entries.as_slice().into())
                    .unwrap_or_else(|| Arc::from([]));
                (entries, false)
            }
        };
        self.later(
            300,
            Event::ChatListUpdated {
                list,
                entries,
                has_more,
            },
        );
    }

    fn open_chat(&mut self, chat_id: ChatId) {
        let messages = self
            .world
            .histories
            .get(&chat_id)
            .cloned()
            .unwrap_or_default();
        let has_more_older = self
            .world
            .older
            .get(&chat_id)
            .is_some_and(|pages| !pages.is_empty());
        self.later(
            120,
            Event::History {
                chat_id,
                messages,
                is_initial: true,
                has_more_older,
            },
        );
    }

    fn view(&mut self, chat_id: ChatId, ids: &[MessageId]) {
        let Some(chat) = self.world.chats.get_mut(&chat_id) else {
            return;
        };
        let newest = ids.iter().copied().max().unwrap_or(0);
        if newest <= chat.last_read_inbox_message_id && chat.unread_count == 0 {
            return;
        }
        chat.last_read_inbox_message_id = chat.last_read_inbox_message_id.max(newest);
        chat.unread_count = 0;
        chat.unread_mention_count = 0;
        let summary = Arc::new(chat.clone());
        self.emit(Event::ChatsUpdated(vec![summary]));
    }

    fn send_text(&mut self, chat_id: ChatId, text: String, reply_to: Option<MessageId>) {
        self.next_pending += 1;
        let pending_id = self.next_pending;
        let final_id = self
            .world
            .histories
            .get(&chat_id)
            .map_or(100, |history| data::last_id(history).max(100))
            + 1;
        let mut message = Message {
            id: pending_id,
            chat_id,
            sender: Sender::User(data::ME),
            sender_name: "Даниил Орлов".into(),
            date: chrono::Local::now().timestamp(),
            edit_date: 0,
            is_outgoing: true,
            sending_state: SendingState::Pending,
            reply_to,
            content: MessageContent::Text(FormattedText {
                text: text.clone(),
                entities: Vec::new(),
            }),
        };
        self.emit(Event::MessageAdded(message.clone()));

        let lower = text.to_lowercase();
        let fails = lower.contains("ошибка") || lower.contains("error");
        if fails {
            message.sending_state = SendingState::Failed {
                error: "Нет соединения".into(),
            };
            self.later(900, Event::MessageUpdated(message));
            return;
        }
        let old_id = message.id;
        message.id = final_id;
        message.sending_state = SendingState::Sent;
        self.world
            .histories
            .entry(chat_id)
            .or_default()
            .push(message.clone());
        self.later(
            650,
            Event::MessageSent {
                old_id,
                message: message.clone(),
            },
        );

        // Move the chat to the top of the unpinned part of the main list.
        if let Some(chat) = self.world.chats.get_mut(&chat_id) {
            let is_group = matches!(
                chat.kind,
                telegradus_core::ChatKind::Supergroup | telegradus_core::ChatKind::BasicGroup
            );
            chat.last_message = Some(data::preview(&message, is_group));
            let summary = Arc::new(chat.clone());
            self.later(650, Event::ChatsUpdated(vec![summary]));
        }
        let main = &mut self.world.main;
        if let Some(position) = main
            .iter()
            .position(|e| e.chat_id == chat_id && !e.is_pinned)
        {
            let entry = main.remove(position);
            let first_unpinned = main.iter().position(|e| !e.is_pinned).unwrap_or(main.len());
            main.insert(first_unpinned, entry);
            let count = self.main_page_sent.max(position + 1).min(main.len());
            let entries: Arc<[ChatListEntry]> = main[..count].into();
            let has_more = count < main.len();
            self.later(
                660,
                Event::ChatListUpdated {
                    list: ChatListId::Main,
                    entries,
                    has_more,
                },
            );
        }
    }

    fn download(&mut self, id: FileId) {
        let Some((file, kind)) = self.world.files.get(&id).cloned() else {
            self.emit(Event::Error(CoreError {
                context: ErrorContext::File,
                code: 404,
                message: "Файл не найден".into(),
            }));
            return;
        };
        if file.is_downloaded() {
            return;
        }
        let path = match kind {
            DemoFile::Picture(picture) => {
                images::render(&self.files_dir, &format!("file-{id}"), picture)
            }
            DemoFile::Document => {
                let path = self.files_dir.join(format!("document-{id}.txt"));
                std::fs::create_dir_all(&self.files_dir)
                    .and_then(|()| std::fs::write(&path, "Демо-документ Telegradus\n"))
                    .map(|()| path)
            }
        };
        let path = match path {
            Ok(path) => path,
            Err(error) => {
                self.emit(Event::Error(CoreError {
                    context: ErrorContext::File,
                    code: 0,
                    message: format!("Не удалось сохранить файл: {error}"),
                }));
                return;
            }
        };
        // Avatars appear quickly; message media shows progress first.
        let is_avatar = (5000..7000).contains(&id);
        let steps: &[(u64, i64)] = if is_avatar {
            &[(150, 100)]
        } else {
            &[(200, 35), (600, 80), (1000, 100)]
        };
        for &(delay, percent) in steps {
            let mut update = file.clone();
            update.downloaded_size = file.size * percent / 100;
            if percent == 100 {
                update.local_path = Some(path.clone());
                update.is_downloading = false;
            } else {
                update.is_downloading = true;
            }
            self.later(delay, Event::FileUpdated(update));
        }
        if let Some((stored, _)) = self.world.files.get_mut(&id) {
            stored.local_path = Some(path);
            stored.downloaded_size = stored.size;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn drain(rx: &mut UnboundedReceiver<Event>, wait: Duration) -> Vec<Event> {
        let deadline = Instant::now() + wait;
        let mut events = Vec::new();
        while Instant::now() < deadline {
            match rx.try_recv() {
                Ok(event) => events.push(event),
                Err(_) => std::thread::sleep(Duration::from_millis(20)),
            }
        }
        events
    }

    #[test]
    fn main_screen_boots_logged_in() {
        let (handle, mut rx) = start(Screen::Main);
        let events = drain(&mut rx, Duration::from_millis(200));
        assert!(
            events
                .iter()
                .any(|e| matches!(e, Event::Auth(AuthState::Ready)))
        );
        let list = events.iter().find_map(|e| match e {
            Event::ChatListUpdated {
                list: ChatListId::Main,
                entries,
                has_more,
            } => Some((entries.len(), *has_more)),
            _ => None,
        });
        assert_eq!(list, Some((data::FIRST_PAGE, true)));
        handle.send(Command::Shutdown);
        let events = drain(&mut rx, Duration::from_millis(400));
        assert!(events.iter().any(|e| matches!(e, Event::Closed)));
    }

    #[test]
    fn send_text_echoes_pending_then_sent() {
        let (handle, mut rx) = start(Screen::Main);
        drain(&mut rx, Duration::from_millis(100));
        handle.send(Command::SendText {
            chat_id: 8,
            text: "привет".into(),
            reply_to: None,
        });
        let events = drain(&mut rx, Duration::from_millis(900));
        let pending = events.iter().find_map(|e| match e {
            Event::MessageAdded(m) => Some(m.clone()),
            _ => None,
        });
        let pending = pending.expect("pending message");
        assert_eq!(pending.sending_state, SendingState::Pending);
        assert!(events.iter().any(|e| matches!(
            e,
            Event::MessageSent { old_id, message } if *old_id == pending.id && message.sending_state == SendingState::Sent
        )));
        handle.send(Command::Shutdown);
    }

    #[test]
    fn world_has_rich_content() {
        let world = data::world();
        assert!(world.chats.len() >= 40);
        assert_eq!(world.folders.len(), 3);
        let showcase = &world.histories[&SHOWCASE_CHAT];
        assert!(
            showcase
                .iter()
                .any(|m| matches!(m.content, MessageContent::Photo { .. }))
        );
        assert!(
            showcase
                .iter()
                .any(|m| matches!(m.content, MessageContent::Service(_)))
        );
        assert!(showcase.iter().any(|m| m.reply_to.is_some()));
        assert!(showcase.iter().any(|m| match &m.content {
            MessageContent::Text(t) => !t.entities.is_empty(),
            _ => false,
        }));
    }
}
