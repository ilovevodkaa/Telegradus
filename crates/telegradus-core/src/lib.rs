//! Telegradus core: owns the TDLib client and exposes a small, UI-agnostic
//! command/event API.
//!
//! ```text
//!   UI  --Command-->  Handle  -->  actor (tokio, own thread)  -->  TDLib
//!   UI  <--Event----  receiver <-- actor <-- receive loop (own thread) <-- TDLib
//! ```
//!
//! The UI never blocks on the core: commands are fire-and-forget and every
//! result comes back as an [`Event`].

mod actor;
mod chat_list;
mod command;
mod config;
mod convert;
mod errors;
mod event;
mod history;
mod message;
pub mod model;
mod ru;
mod service;
mod store;
mod tdlog;
mod text;

pub use command::Command;
pub use config::{ApiCredentials, Config};
pub use event::Event;
pub use model::*;

use std::io;
use std::panic::{self, AssertUnwindSafe};
use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;

use tokio::sync::mpsc;
use tracing::{debug, error};

use crate::actor::{Actor, IncomingUpdate};

/// Sends commands to the running core. Cheap to clone.
#[derive(Debug, Clone)]
pub struct Handle {
    tx: mpsc::UnboundedSender<Command>,
}

impl Handle {
    /// Queue a command. Never blocks; silently ignored after the core stopped.
    pub fn send(&self, command: Command) {
        let _ = self.tx.send(command);
    }
}

/// Set by the first [`start`] call.
static STARTED: AtomicBool = AtomicBool::new(false);

/// Start the core on its own threads.
///
/// Must be called at most once per process: TDLib's receive loop is global.
/// Returns the command handle and the stream of events for the UI.
pub fn start(config: Config) -> (Handle, mpsc::UnboundedReceiver<Event>) {
    let (command_tx, command_rx) = mpsc::unbounded_channel();
    let (event_tx, event_rx) = mpsc::unbounded_channel();
    let handle = Handle { tx: command_tx };

    if STARTED.swap(true, Ordering::SeqCst) {
        error!("telegradus_core::start called more than once");
        report_failure(&event_tx, "Ядро Telegradus уже запущено.".to_owned());
    } else if let Err(err) = spawn_threads(config, command_rx, event_tx.clone()) {
        error!("cannot start the core threads: {err}");
        report_failure(&event_tx, format!("Не удалось запустить ядро: {err}"));
    }
    (handle, event_rx)
}

fn report_failure(events: &mpsc::UnboundedSender<Event>, message: String) {
    let _ = events.send(Event::Error(errors::local_error(
        ErrorContext::Other,
        message,
    )));
    let _ = events.send(Event::Closed);
}

/// Spawns the core thread, which prepares the data directory and TDLib
/// logging, then starts the receive thread and runs the actor.
fn spawn_threads(
    config: Config,
    commands: mpsc::UnboundedReceiver<Command>,
    events: mpsc::UnboundedSender<Event>,
) -> io::Result<()> {
    thread::Builder::new()
        .name("telegradus-core".into())
        .spawn(move || {
            prepare_data_dir(&config.data_dir, &events);
            // Before the first `td_receive`, so TDLib never logs to stderr.
            tdlog::configure(&config.data_dir);

            let (update_tx, update_rx) = mpsc::unbounded_channel();
            let running = Arc::new(AtomicBool::new(true));
            // Stops the receive loop however this thread ends.
            let _stop = StopOnDrop(Arc::clone(&running));
            let spawned = thread::Builder::new()
                .name("tdlib-receive".into())
                .spawn(move || receive_loop(&update_tx, &running));
            if let Err(err) = spawned {
                error!("cannot start the TDLib receive thread: {err}");
                report_failure(&events, format!("Не удалось запустить ядро: {err}"));
                return;
            }
            run_actor(config, commands, update_rx, events);
        })
        .map(drop)
}

fn prepare_data_dir(data_dir: &Path, events: &mpsc::UnboundedSender<Event>) {
    if let Err(err) = std::fs::create_dir_all(data_dir) {
        error!("cannot create {}: {err}", data_dir.display());
        let message = format!(
            "Не удалось создать папку данных {}: {err}",
            data_dir.display()
        );
        let _ = events.send(Event::Error(errors::local_error(
            ErrorContext::Other,
            message,
        )));
    }
}

/// Runs the actor on a single-threaded tokio runtime until it stops.
fn run_actor(
    config: Config,
    commands: mpsc::UnboundedReceiver<Command>,
    updates: mpsc::UnboundedReceiver<IncomingUpdate>,
    events: mpsc::UnboundedSender<Event>,
) {
    let runtime = match tokio::runtime::Builder::new_current_thread()
        .enable_time()
        .build()
    {
        Ok(runtime) => runtime,
        Err(err) => {
            error!("cannot build the core runtime: {err}");
            report_failure(&events, format!("Не удалось запустить ядро: {err}"));
            return;
        }
    };
    let actor = Actor::new(config, commands, updates, events.clone());
    let outcome = panic::catch_unwind(AssertUnwindSafe(|| runtime.block_on(actor.run())));
    if outcome.is_err() {
        error!("the core actor panicked");
        report_failure(&events, "Внутренняя ошибка ядра.".to_owned());
    }
}

/// Forwards TDLib updates to the actor until the core stops.
///
/// `tdlib_rs::receive` blocks for up to two seconds and also routes responses
/// to the futures of pending requests, so it must run on exactly one thread.
fn receive_loop(updates: &mpsc::UnboundedSender<IncomingUpdate>, running: &AtomicBool) {
    while running.load(Ordering::Acquire) {
        match panic::catch_unwind(tdlib_rs::receive) {
            Ok(Some((update, client_id))) => {
                if updates.send((Box::new(update), client_id)).is_err() {
                    break;
                }
            }
            Ok(None) => {}
            Err(_) => error!("tdlib_rs::receive panicked; continuing"),
        }
    }
    debug!("TDLib receive loop stopped");
}

/// Clears the shared "running" flag when dropped.
struct StopOnDrop(Arc<AtomicBool>);

impl Drop for StopOnDrop {
    fn drop(&mut self) {
        self.0.store(false, Ordering::Release);
    }
}
