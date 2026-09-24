//! Connection between the UI and the core (real TDLib or the demo).

use iced::Subscription;
use iced::futures::{SinkExt, Stream};
use telegradus_core::{Command, Config, Event};
use tokio::sync::mpsc::UnboundedReceiver;

use crate::app::Message;

/// Where commands go.
#[derive(Debug, Clone)]
pub enum Backend {
    Real(telegradus_core::Handle),
    #[cfg(feature = "demo")]
    Demo(crate::demo::DemoHandle),
}

impl Backend {
    pub fn send(&self, command: Command) {
        match self {
            Backend::Real(handle) => handle.send(command),
            #[cfg(feature = "demo")]
            Backend::Demo(handle) => handle.send(command),
        }
    }
}

/// Which core the app runs against.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Mode {
    Real,
    #[cfg(feature = "demo")]
    Demo(crate::demo::Screen),
}

/// Events drained from the core in one go are delivered as one message.
const MAX_BATCH: usize = 256;

/// Starts the core once and streams its events. The subscription identity
/// only depends on `mode`, so it lives as long as the app.
pub fn subscription(mode: Mode) -> Subscription<Message> {
    Subscription::run_with(mode, connect)
}

fn connect(mode: &Mode) -> impl Stream<Item = Message> + use<> {
    let mode = *mode;
    iced::stream::channel(32, async move |mut output| {
        let (backend, mut events) = start(mode);
        if output.send(Message::CoreReady(backend)).await.is_err() {
            return;
        }
        while let Some(first) = events.recv().await {
            let mut batch = Vec::with_capacity(8);
            batch.push(first);
            while batch.len() < MAX_BATCH {
                match events.try_recv() {
                    Ok(event) => batch.push(event),
                    Err(_) => break,
                }
            }
            if output.send(Message::Core(batch)).await.is_err() {
                return;
            }
        }
        // The core reports every stop with `Event::Closed` first.
        tracing::debug!("core event stream ended");
        // Never finish: a finished subscription would be restarted, and the
        // core must be started at most once per process.
        std::future::pending::<()>().await;
    })
}

fn start(mode: Mode) -> (Backend, UnboundedReceiver<Event>) {
    match mode {
        Mode::Real => {
            let (handle, events) = telegradus_core::start(Config::load());
            (Backend::Real(handle), events)
        }
        #[cfg(feature = "demo")]
        Mode::Demo(screen) => {
            let (handle, events) = crate::demo::start(screen);
            (Backend::Demo(handle), events)
        }
    }
}
