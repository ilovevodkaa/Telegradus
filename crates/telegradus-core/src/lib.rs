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

mod command;
mod config;
mod event;
pub mod model;

pub use command::Command;
pub use config::{ApiCredentials, Config};
pub use event::Event;
pub use model::*;

use tokio::sync::mpsc;

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

/// Start the core on its own threads.
///
/// Must be called at most once per process: TDLib's receive loop is global.
/// Returns the command handle and the stream of events for the UI.
pub fn start(config: Config) -> (Handle, mpsc::UnboundedReceiver<Event>) {
    let _ = config;
    todo!("implemented by the core actor")
}
