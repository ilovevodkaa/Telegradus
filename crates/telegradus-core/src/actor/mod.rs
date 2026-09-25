//! The core actor: owns all state, drives TDLib and emits coalesced events.
//!
//! Inputs are UI commands, TDLib updates (from the receive thread) and results
//! of TDLib requests, which run as tasks so a slow request never blocks update
//! processing. After each input the actor drains everything already queued and
//! then flushes the changed chats and chat lists as a few batched events.

mod auth;
mod chats;
mod messages;

use std::collections::{BTreeSet, HashMap, HashSet};
use std::future::Future;
use std::sync::Arc;
use std::time::Duration;

use tdlib_rs::enums::Update;
use tdlib_rs::{functions, types};
use tokio::sync::mpsc::{self, error::TryRecvError};
use tokio::task::{JoinError, JoinSet};
use tokio::time::Instant;
use tracing::{debug, error, warn};

use crate::chat_list::ChatLists;
use crate::errors::{core_error, local_error};
use crate::history::{OpenChat, Page};
use crate::model::{ChatId, ChatListEntry, ChatListId, ErrorContext, FileId, FileRef};
use crate::store::Store;
use crate::tdlog::LOG_VERBOSITY;
use crate::{Command, Config, Event};

/// A TDLib update and the id of the client it belongs to.
pub(crate) type IncomingUpdate = (Box<Update>, i32);

/// Minimum time between two flushes of batched chat events (about one frame).
const MIN_FLUSH_INTERVAL: Duration = Duration::from_millis(16);
/// Inputs handled per drain before the actor gives the flush timer a chance.
const MAX_DRAIN: usize = 4096;
/// How long to wait for TDLib to report `Closed` after `close()`.
const SHUTDOWN_TIMEOUT: Duration = Duration::from_secs(15);

/// Result of a TDLib request, fed back into the actor.
enum Done {
    /// Nothing left to do.
    Nothing,
    /// A request whose only interesting outcome is an error.
    Unit(ErrorContext, Result<(), types::Error>),
    TdlibParameters(Result<(), types::Error>),
    Auth(Result<(), types::Error>),
    Me(Result<Box<types::User>, types::Error>),
    ChatsLoaded(ChatListId, Result<(), types::Error>),
    History {
        chat_id: ChatId,
        initial: bool,
        result: Result<Page, types::Error>,
    },
    Refetched(Result<Box<types::Message>, types::Error>),
    File(Result<types::File, types::Error>),
}

/// Where the current TDLib instance is in its lifecycle.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Phase {
    /// Created; no authorization state received yet.
    Starting,
    /// Waiting for `setTdlibParameters` (possibly for credentials from the user).
    WaitParameters,
    WaitCode,
    WaitEmailCode,
    /// Any other login step.
    LoggingIn,
    Ready,
    /// Logging out or closing.
    Closing,
}

/// Changes waiting to be flushed to the UI.
#[derive(Debug, Default)]
struct Dirty {
    chats: HashSet<ChatId>,
    lists: BTreeSet<ChatListId>,
    /// Lists to send even if their entries did not change (e.g. after a load).
    forced_lists: BTreeSet<ChatListId>,
    files: HashMap<FileId, FileRef>,
}

impl Dirty {
    fn is_empty(&self) -> bool {
        self.chats.is_empty()
            && self.lists.is_empty()
            && self.forced_lists.is_empty()
            && self.files.is_empty()
    }
}

pub(crate) struct Actor {
    config: Config,
    client_id: i32,
    phase: Phase,
    commands: mpsc::UnboundedReceiver<Command>,
    commands_open: bool,
    updates: mpsc::UnboundedReceiver<IncomingUpdate>,
    events: mpsc::UnboundedSender<Event>,
    /// In-flight TDLib requests, tagged with the client they were sent to.
    tasks: JoinSet<(i32, Done)>,
    store: Store,
    lists: ChatLists,
    loading_lists: HashSet<ChatListId>,
    /// Entries last sent per list, to skip unchanged lists.
    sent_lists: HashMap<ChatListId, Arc<[ChatListEntry]>>,
    open_chats: HashMap<ChatId, OpenChat>,
    dirty: Dirty,
    last_flush: Instant,
    flush_at: Option<Instant>,
    shutting_down: bool,
    shutdown_deadline: Option<Instant>,
    stopped: bool,
}

impl Actor {
    pub(crate) fn new(
        config: Config,
        commands: mpsc::UnboundedReceiver<Command>,
        updates: mpsc::UnboundedReceiver<IncomingUpdate>,
        events: mpsc::UnboundedSender<Event>,
    ) -> Self {
        Self {
            config,
            client_id: 0,
            phase: Phase::Starting,
            commands,
            commands_open: true,
            updates,
            events,
            tasks: JoinSet::new(),
            store: Store::default(),
            lists: ChatLists::default(),
            loading_lists: HashSet::new(),
            sent_lists: HashMap::new(),
            open_chats: HashMap::new(),
            dirty: Dirty::default(),
            last_flush: Instant::now(),
            flush_at: None,
            shutting_down: false,
            shutdown_deadline: None,
            stopped: false,
        }
    }

    pub(crate) async fn run(mut self) {
        self.boot_client();
        while !self.stopped {
            let wake_at = self.wake_at();
            tokio::select! {
                Some(joined) = self.tasks.join_next(), if !self.tasks.is_empty() => {
                    self.on_joined(joined);
                }
                update = self.updates.recv() => match update {
                    Some((update, client_id)) => self.on_update(*update, client_id),
                    None => self.on_receive_stopped(),
                },
                command = self.commands.recv(), if self.commands_open => match command {
                    Some(command) => self.on_command(command),
                    None => self.on_commands_closed(),
                },
                () = tokio::time::sleep_until(wake_at.unwrap_or_else(Instant::now)),
                    if wake_at.is_some() => self.on_timer(),
            }
            self.drain();
            self.schedule_flush();
        }
        debug!("core actor stopped");
    }

    /// Handles everything that is already queued, up to [`MAX_DRAIN`] inputs.
    fn drain(&mut self) {
        for _ in 0..MAX_DRAIN {
            if self.stopped {
                return;
            }
            if let Some(joined) = self.tasks.try_join_next() {
                self.on_joined(joined);
                continue;
            }
            match self.updates.try_recv() {
                Ok((update, client_id)) => {
                    self.on_update(*update, client_id);
                    continue;
                }
                Err(TryRecvError::Disconnected) => {
                    self.on_receive_stopped();
                    return;
                }
                Err(TryRecvError::Empty) => {}
            }
            if self.commands_open {
                match self.commands.try_recv() {
                    Ok(command) => {
                        self.on_command(command);
                        continue;
                    }
                    Err(TryRecvError::Disconnected) => {
                        self.on_commands_closed();
                        continue;
                    }
                    Err(TryRecvError::Empty) => {}
                }
            }
            return;
        }
    }

    /// Creates a new TDLib instance. TDLib starts working on its first request,
    /// which repeats the (already configured) log verbosity.
    fn boot_client(&mut self) {
        self.client_id = tdlib_rs::create_client();
        self.phase = Phase::Starting;
        debug!(client_id = self.client_id, "created TDLib client");
        self.emit(Event::Auth(crate::AuthState::Initializing));
        let client_id = self.client_id;
        self.spawn(async move {
            if let Err(err) = functions::set_log_verbosity_level(LOG_VERBOSITY, client_id).await {
                warn!("cannot set the TDLib log verbosity: {}", err.message);
            }
            Done::Nothing
        });
    }

    /// Runs a TDLib request as a task; its result comes back through [`Self::on_joined`].
    fn spawn(&mut self, request: impl Future<Output = Done> + Send + 'static) {
        let client_id = self.client_id;
        self.tasks.spawn(async move { (client_id, request.await) });
    }

    fn on_joined(&mut self, joined: Result<(i32, Done), JoinError>) {
        match joined {
            Ok((client_id, done)) if client_id == self.client_id => self.on_done(done),
            // A result for a client that no longer exists.
            Ok(_) => {}
            Err(err) if err.is_panic() => {
                error!("a TDLib request task panicked: {err}");
                self.emit_local_error(ErrorContext::Other, "Внутренняя ошибка ядра.");
            }
            Err(_) => {}
        }
    }

    fn on_done(&mut self, done: Done) {
        match done {
            Done::Nothing => {}
            Done::Unit(context, result) => {
                if let Err(err) = result {
                    self.emit_error(context, &err);
                }
            }
            Done::TdlibParameters(result) => self.on_parameters_set(result),
            Done::Auth(result) => self.on_auth_result(result),
            Done::Me(result) => self.on_me(result),
            Done::ChatsLoaded(list, result) => self.on_chats_loaded(list, result),
            Done::History {
                chat_id,
                initial,
                result,
            } => self.on_history(chat_id, initial, result),
            Done::Refetched(result) => self.on_refetched(result),
            Done::File(result) => self.on_file_result(result),
        }
    }

    fn on_update(&mut self, update: Update, client_id: i32) {
        if client_id != self.client_id {
            return;
        }
        match update {
            Update::AuthorizationState(u) => self.on_authorization_state(u.authorization_state),
            Update::ConnectionState(u) => {
                self.emit(Event::Connection(crate::convert::connection_state(
                    &u.state,
                )));
            }
            Update::NewMessage(u) => self.on_new_message(u.message),
            Update::MessageSendSucceeded(u) => self.on_send_succeeded(u),
            Update::MessageSendFailed(u) => self.on_send_failed(u),
            Update::MessageContent(u) => self.on_message_content(u),
            Update::MessageEdited(u) => self.on_message_edited(u),
            Update::DeleteMessages(u) => self.on_messages_deleted(u),
            Update::File(u) => self.on_file(&u.file),
            other => self.on_chat_update(other),
        }
    }

    fn on_command(&mut self, command: Command) {
        if self.shutting_down {
            debug!(?command, "ignoring a command during shutdown");
            return;
        }
        match command {
            Command::SetApiCredentials { api_id, api_hash } => {
                self.set_api_credentials(api_id, &api_hash);
            }
            Command::SubmitPhoneNumber(phone) => self.submit_phone_number(phone),
            Command::RequestQrCode => self.request_qr_code(),
            Command::SubmitCode(code) => self.submit_code(code),
            Command::ResendCode => self.resend_code(),
            Command::SubmitPassword(password) => self.submit_password(password),
            Command::LogOut => self.log_out(),
            Command::LoadChats(list) => self.load_chats(list),
            Command::OpenChat(chat_id) => self.open_chat(chat_id),
            Command::CloseChat(chat_id) => self.close_chat(chat_id),
            Command::LoadOlder {
                chat_id,
                from_message_id,
            } => self.load_older(chat_id, from_message_id),
            Command::ViewMessages {
                chat_id,
                message_ids,
            } => self.view_messages(chat_id, message_ids),
            Command::SendText {
                chat_id,
                text,
                reply_to,
            } => self.send_text(chat_id, text, reply_to),
            Command::DownloadFile(file_id) => self.download_file(file_id),
            Command::Shutdown => self.shutdown(),
        }
    }

    /// Every [`crate::Handle`] was dropped: close TDLib cleanly anyway.
    fn on_commands_closed(&mut self) {
        self.commands_open = false;
        self.shutdown();
    }

    /// The receive thread is gone, so no TDLib response can arrive any more.
    fn on_receive_stopped(&mut self) {
        error!("the TDLib receive loop stopped unexpectedly");
        self.emit_local_error(ErrorContext::Other, "Связь с TDLib потеряна.");
        self.finish();
    }

    fn shutdown(&mut self) {
        if self.shutting_down {
            return;
        }
        self.shutting_down = true;
        self.shutdown_deadline = Some(Instant::now() + SHUTDOWN_TIMEOUT);
        let client_id = self.client_id;
        self.spawn(async move {
            if let Err(err) = functions::close(client_id).await {
                debug!("close failed: {}", err.message);
            }
            Done::Nothing
        });
    }

    /// Flushes pending events, reports [`Event::Closed`] and stops the actor.
    fn finish(&mut self) {
        self.flush();
        self.emit(Event::Closed);
        self.stopped = true;
    }

    fn wake_at(&self) -> Option<Instant> {
        match (self.flush_at, self.shutdown_deadline) {
            (Some(a), Some(b)) => Some(a.min(b)),
            (a, b) => a.or(b),
        }
    }

    fn on_timer(&mut self) {
        let now = Instant::now();
        if self.flush_at.is_some_and(|at| at <= now) {
            self.flush();
        }
        if self.shutdown_deadline.is_some_and(|at| at <= now) {
            warn!("TDLib did not close in time; stopping anyway");
            self.finish();
        }
    }

    /// Flushes now, or arms the flush timer if the last flush was too recent.
    fn schedule_flush(&mut self) {
        if self.stopped || self.dirty.is_empty() {
            return;
        }
        let due = self.last_flush + MIN_FLUSH_INTERVAL;
        if Instant::now() >= due {
            self.flush();
        } else {
            self.flush_at.get_or_insert(due);
        }
    }

    /// Sends one `ChatsUpdated`, one `ChatListUpdated` per changed list and the
    /// latest state of every changed file.
    fn flush(&mut self) {
        self.flush_at = None;
        self.last_flush = Instant::now();
        let dirty = std::mem::take(&mut self.dirty);

        let chats: Vec<_> = dirty
            .chats
            .into_iter()
            .filter_map(|id| self.store.take_changed_summary(id))
            .collect();
        if !chats.is_empty() {
            self.emit(Event::ChatsUpdated(chats));
        }

        for list in dirty.lists.union(&dirty.forced_lists) {
            self.emit_list(*list, dirty.forced_lists.contains(list));
        }

        for file in dirty.files.into_values() {
            self.emit(Event::FileUpdated(file));
        }
    }

    fn emit_list(&mut self, list: ChatListId, force: bool) {
        let entries = self.lists.entries(list);
        let unchanged = self
            .sent_lists
            .get(&list)
            .is_some_and(|sent| sent[..] == entries[..]);
        if unchanged && !force {
            return;
        }
        self.sent_lists.insert(list, Arc::clone(&entries));
        self.emit(Event::ChatListUpdated {
            list,
            entries,
            has_more: self.lists.has_more(list),
        });
    }

    fn emit(&self, event: Event) {
        // The UI may already be gone during shutdown; nothing to do then.
        let _ = self.events.send(event);
    }

    fn emit_error(&self, context: ErrorContext, error: &types::Error) {
        debug!(
            ?context,
            code = error.code,
            "TDLib error: {}",
            error.message
        );
        self.emit(Event::Error(core_error(context, error)));
    }

    fn emit_local_error(&self, context: ErrorContext, message: impl Into<String>) {
        self.emit(Event::Error(local_error(context, message)));
    }
}

#[cfg(test)]
mod tests;
