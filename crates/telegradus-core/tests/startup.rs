//! End-to-end check against the real TDLib: the core must reach the phone
//! number step (TDLib gets there offline), then shut down cleanly.
//!
//! `start` may run once per process, so this file holds a single test.
//! Run with `cargo test -p telegradus-core --test startup -- --ignored`.

use std::path::PathBuf;
use std::time::{Duration, Instant};

use telegradus_core::{ApiCredentials, AuthState, Command, Config, Event};
use tokio::sync::mpsc::UnboundedReceiver;

const STEP_TIMEOUT: Duration = Duration::from_secs(30);

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

/// Names of this process's threads (Linux only).
fn thread_names() -> Vec<String> {
    let Ok(tasks) = std::fs::read_dir("/proc/self/task") else {
        return Vec::new();
    };
    tasks
        .filter_map(|task| std::fs::read_to_string(task.ok()?.path().join("comm")).ok())
        .map(|name| name.trim().to_owned())
        .collect()
}

fn core_threads_running() -> bool {
    thread_names()
        .iter()
        .any(|name| name == "tdlib-receive" || name == "telegradus-core")
}

#[test]
#[ignore = "starts the real TDLib; run with --ignored"]
fn reaches_phone_number_step_and_shuts_down() {
    let data_dir: PathBuf =
        std::env::temp_dir().join(format!("telegradus-startup-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&data_dir);

    let config = Config {
        api: Some(ApiCredentials {
            api_id: 1,
            api_hash: "0123456789abcdef0123456789abcdef".into(),
        }),
        data_dir: data_dir.clone(),
        use_test_dc: false,
    };
    let (handle, mut events) = telegradus_core::start(config);

    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_time()
        .build()
        .unwrap();
    runtime.block_on(async {
        let started = Instant::now();
        let seen = wait_for(&mut events, "WaitPhoneNumber", |e| {
            matches!(e, Event::Auth(AuthState::WaitPhoneNumber))
        })
        .await;
        println!("WaitPhoneNumber after {:?}: {seen:?}", started.elapsed());
        assert!(matches!(seen[0], Event::Auth(AuthState::Initializing)));

        handle.send(Command::Shutdown);
        let seen = wait_for(&mut events, "Closed", |e| matches!(e, Event::Closed)).await;
        println!("closed: {seen:?}");
        assert!(
            seen.iter()
                .any(|e| matches!(e, Event::Auth(AuthState::Closed))),
            "TDLib did not report authorizationStateClosed"
        );
    });

    // The actor thread ends right after `Closed`; the receive loop notices
    // within one `td_receive` timeout (2 s).
    let deadline = Instant::now() + Duration::from_secs(10);
    while core_threads_running() && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(100));
    }
    assert!(
        !core_threads_running(),
        "core threads still running: {:?}",
        thread_names()
    );

    // Commands after shutdown are ignored without panicking.
    handle.send(Command::Shutdown);

    assert!(
        data_dir.join("tdlib.log").exists(),
        "TDLib log file missing"
    );
    assert!(data_dir.join("tdlib").is_dir(), "TDLib database missing");
    let _ = std::fs::remove_dir_all(&data_dir);
}
