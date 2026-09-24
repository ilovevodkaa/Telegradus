use std::path::PathBuf;

/// Telegram application credentials from <https://my.telegram.org/apps>.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ApiCredentials {
    pub api_id: i32,
    pub api_hash: String,
}

/// Startup configuration of the core.
#[derive(Debug, Clone)]
pub struct Config {
    /// `None` until the user provides credentials (see [`crate::AuthState::NeedApiCredentials`]).
    pub api: Option<ApiCredentials>,
    /// Root of all persistent data: TDLib database, downloaded files, settings.
    pub data_dir: PathBuf,
    /// Connect to Telegram's test data centers instead of production.
    pub use_test_dc: bool,
}

impl Config {
    /// Build the configuration from, in order of priority:
    /// 1. `TELEGRADUS_API_ID` / `TELEGRADUS_API_HASH` environment variables at runtime,
    /// 2. the same variables at compile time (`option_env!`),
    /// 3. `settings.json` in the data directory (written by `SetApiCredentials`).
    ///
    /// `TELEGRADUS_DATA_DIR` overrides the platform data directory and
    /// `TELEGRADUS_TEST_DC=1` enables the test data centers.
    pub fn load() -> Self {
        todo!("implemented by the core actor")
    }
}
