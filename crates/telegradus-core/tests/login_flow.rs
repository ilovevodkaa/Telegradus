//! End-to-end check of the credentials prompt and of logging out against the
//! real TDLib (offline): missing credentials are asked for and persisted, and
//! after `LogOut` a fresh TDLib instance brings the user back to the phone
//! number step.
//!
//! `start` may run once per process, so this file holds a single test.
//! Run with `cargo test -p telegradus-core --test login_flow -- --ignored`.

use std::time::Duration;

use telegradus_core::{AuthState, Command, Config, ErrorContext, Event};
use tokio::sync::mpsc::UnboundedReceiver;

const STEP_TIMEOUT: Duration = Duration::from_secs(30);
const API_HASH: &str = "0123456789abcdef0123456789abcdef";

/// Waits for the first event matching `wanted`, returning every event seen.
async fn wait_for(
    events: &mut UnboundedReceiver<Event>,
    what: &str,
    wanted: impl Fn(&Event) -> bool,
) -> Vec<Event> {
    let mut seen = Vec::new();
    let deadline = tokio::time::Instant::now() + STEP_TIMEOUT;
    loop {
        match tokio::time::timeout_at(deadline, events.recv()).await {
            Ok(Some(event)) => {
                let done = wanted(&event);
                seen.push(event);
                if done {
                    return seen;
                }
            }
            Ok(None) => panic!("event stream ended before {what}; saw {seen:#?}"),
            Err(_) => panic!("timed out waiting for {what}; saw {seen:#?}"),
        }
    }
}

fn is_auth(event: &Event, state: &AuthState) -> bool {
    matches!(event, Event::Auth(s) if s == state)
}

#[test]
#[ignore = "starts the real TDLib; run with --ignored"]
fn asks_for_credentials_and_restarts_after_logout() {
    let data_dir = std::env::temp_dir().join(format!("telegradus-login-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&data_dir);
    let config = Config {
        api: None,
        data_dir: data_dir.clone(),
        use_test_dc: false,
    };
    let (handle, mut events) = telegradus_core::start(config);

    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_time()
        .build()
        .unwrap();
    runtime.block_on(async {
        wait_for(&mut events, "NeedApiCredentials", |e| {
            is_auth(e, &AuthState::NeedApiCredentials)
        })
        .await;

        // Invalid values are rejected locally.
        handle.send(Command::SetApiCredentials {
            api_id: 0,
            api_hash: API_HASH.into(),
        });
        let seen = wait_for(
            &mut events,
            "a validation error",
            |e| matches!(e, Event::Error(err) if err.context == ErrorContext::Auth),
        )
        .await;
        println!("validation: {seen:?}");

        handle.send(Command::SetApiCredentials {
            api_id: 1,
            api_hash: API_HASH.into(),
        });
        wait_for(&mut events, "WaitPhoneNumber", |e| {
            is_auth(e, &AuthState::WaitPhoneNumber)
        })
        .await;
        let settings = std::fs::read_to_string(data_dir.join("settings.json")).unwrap();
        let settings: serde_json::Value = serde_json::from_str(&settings).unwrap();
        assert_eq!(settings["api_id"], 1);
        assert_eq!(settings["api_hash"], API_HASH);

        // Logging out closes TDLib; a new instance must come up by itself.
        handle.send(Command::LogOut);
        let seen = wait_for(&mut events, "Closed after LogOut", |e| {
            is_auth(e, &AuthState::Closed)
        })
        .await;
        println!("log out: {seen:?}");
        let seen = wait_for(&mut events, "WaitPhoneNumber on a new client", |e| {
            is_auth(e, &AuthState::WaitPhoneNumber)
        })
        .await;
        println!("restart: {seen:?}");
        assert!(
            seen.iter().any(|e| is_auth(e, &AuthState::Initializing)),
            "no new TDLib instance was started"
        );

        handle.send(Command::Shutdown);
        wait_for(&mut events, "Closed", |e| matches!(e, Event::Closed)).await;
    });
    let _ = std::fs::remove_dir_all(&data_dir);
}
