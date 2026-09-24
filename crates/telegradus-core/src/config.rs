use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use tracing::warn;

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

const ENV_API_ID: &str = "TELEGRADUS_API_ID";
const ENV_API_HASH: &str = "TELEGRADUS_API_HASH";
const ENV_DATA_DIR: &str = "TELEGRADUS_DATA_DIR";
const ENV_TEST_DC: &str = "TELEGRADUS_TEST_DC";

/// Name of the settings file inside the data directory.
pub(crate) const SETTINGS_FILE: &str = "settings.json";

impl Config {
    /// Build the configuration from, in order of priority:
    /// 1. `TELEGRADUS_API_ID` / `TELEGRADUS_API_HASH` environment variables at runtime,
    /// 2. the same variables at compile time (`option_env!`),
    /// 3. `settings.json` in the data directory (written by `SetApiCredentials`).
    ///
    /// `TELEGRADUS_DATA_DIR` overrides the platform data directory and
    /// `TELEGRADUS_TEST_DC=1` enables the test data centers.
    pub fn load() -> Self {
        let compile_time = Sources {
            api_id: option_env!("TELEGRADUS_API_ID"),
            api_hash: option_env!("TELEGRADUS_API_HASH"),
        };
        resolve(
            |key| std::env::var(key).ok(),
            compile_time,
            default_data_dir,
            read_settings,
        )
    }
}

/// A pair of raw `api_id`/`api_hash` values from one configuration source.
#[derive(Debug, Clone, Copy, Default)]
struct Sources<'a> {
    api_id: Option<&'a str>,
    api_hash: Option<&'a str>,
}

/// Pure configuration resolution; every side effect is injected so it can be unit-tested.
fn resolve(
    env: impl Fn(&str) -> Option<String>,
    compile_time: Sources<'_>,
    default_dir: impl FnOnce() -> PathBuf,
    settings: impl FnOnce(&Path) -> Option<ApiCredentials>,
) -> Config {
    let data_dir = match env(ENV_DATA_DIR) {
        Some(dir) if !dir.trim().is_empty() => absolute(PathBuf::from(dir)),
        Some(_) => {
            warn!("{ENV_DATA_DIR} is empty; using the default data directory");
            default_dir()
        }
        None => default_dir(),
    };

    let use_test_dc = env(ENV_TEST_DC).is_some_and(|value| parse_flag(ENV_TEST_DC, &value));

    let runtime_id = env(ENV_API_ID);
    let runtime_hash = env(ENV_API_HASH);
    let runtime = Sources {
        api_id: runtime_id.as_deref(),
        api_hash: runtime_hash.as_deref(),
    };
    let api = credentials_from(runtime, "environment")
        .or_else(|| credentials_from(compile_time, "build-time environment"))
        .or_else(|| settings(&data_dir));

    Config {
        api,
        data_dir,
        use_test_dc,
    }
}

fn absolute(path: PathBuf) -> PathBuf {
    std::path::absolute(&path).unwrap_or(path)
}

/// Platform data directory: `~/.local/share/telegradus` on Linux,
/// `%APPDATA%\Telegradus\data` on Windows.
fn default_data_dir() -> PathBuf {
    match directories::ProjectDirs::from("", "", "Telegradus") {
        Some(dirs) => dirs.data_dir().to_path_buf(),
        None => {
            warn!("no home directory found; storing data in ./telegradus-data");
            absolute(PathBuf::from("telegradus-data"))
        }
    }
}

fn parse_flag(name: &str, value: &str) -> bool {
    match value.trim().to_ascii_lowercase().as_str() {
        "1" | "true" | "yes" | "on" => true,
        "" | "0" | "false" | "no" | "off" => false,
        _ => {
            warn!("ignoring invalid {name}={value:?}; expected 1 or 0");
            false
        }
    }
}

/// Validated credentials from one source; `None` (with a warning) when the
/// source is incomplete or invalid.
fn credentials_from(source: Sources<'_>, origin: &str) -> Option<ApiCredentials> {
    match (source.api_id, source.api_hash) {
        (None, None) => None,
        (Some(id), Some(hash)) => {
            let credentials = parse_api_id(id).zip(valid_api_hash(hash));
            if credentials.is_none() {
                warn!("ignoring invalid api_id/api_hash from the {origin}");
            }
            credentials.map(|(api_id, api_hash)| ApiCredentials { api_id, api_hash })
        }
        _ => {
            warn!(
                "ignoring api credentials from the {origin}: both {ENV_API_ID} and {ENV_API_HASH} must be set"
            );
            None
        }
    }
}

fn parse_api_id(value: &str) -> Option<i32> {
    value.trim().parse::<i32>().ok().filter(|id| *id > 0)
}

/// An `api_hash` is 32 hexadecimal characters.
fn valid_api_hash(value: &str) -> Option<String> {
    let value = value.trim();
    (value.len() == 32 && value.bytes().all(|b| b.is_ascii_hexdigit())).then(|| value.to_owned())
}

/// Checks user-provided credentials before they are stored.
pub(crate) fn validate_credentials(api_id: i32, api_hash: &str) -> Option<ApiCredentials> {
    let api_hash = valid_api_hash(api_hash)?;
    (api_id > 0).then_some(ApiCredentials { api_id, api_hash })
}

fn read_settings(data_dir: &Path) -> Option<ApiCredentials> {
    let path = data_dir.join(SETTINGS_FILE);
    match fs::read_to_string(&path) {
        Ok(json) => {
            let credentials = parse_settings(&json);
            if credentials.is_none() {
                warn!("ignoring invalid credentials in {}", path.display());
            }
            credentials
        }
        Err(err) if err.kind() == io::ErrorKind::NotFound => None,
        Err(err) => {
            warn!("cannot read {}: {err}", path.display());
            None
        }
    }
}

/// Extracts credentials from the contents of `settings.json`.
fn parse_settings(json: &str) -> Option<ApiCredentials> {
    let value: serde_json::Value = serde_json::from_str(json).ok()?;
    let api_id = match value.get("api_id")? {
        serde_json::Value::Number(n) => i32::try_from(n.as_i64()?).ok()?,
        serde_json::Value::String(s) => parse_api_id(s)?,
        _ => return None,
    };
    validate_credentials(api_id, value.get("api_hash")?.as_str()?)
}

/// Stores credentials in `settings.json`, keeping any other keys in the file.
pub(crate) fn save_credentials(data_dir: &Path, credentials: &ApiCredentials) -> io::Result<()> {
    fs::create_dir_all(data_dir)?;
    let path = data_dir.join(SETTINGS_FILE);
    let existing = fs::read_to_string(&path).ok();
    let json = merge_settings(existing.as_deref(), credentials);
    // Write to a temporary file first so a crash never leaves a truncated file.
    let tmp = path.with_extension("json.tmp");
    fs::write(&tmp, json)?;
    fs::rename(&tmp, &path)
}

fn merge_settings(existing: Option<&str>, credentials: &ApiCredentials) -> String {
    let mut root = existing
        .and_then(|json| serde_json::from_str::<serde_json::Value>(json).ok())
        .filter(serde_json::Value::is_object)
        .unwrap_or_else(|| serde_json::Value::Object(Default::default()));
    if let Some(map) = root.as_object_mut() {
        map.insert("api_id".into(), credentials.api_id.into());
        map.insert("api_hash".into(), credentials.api_hash.clone().into());
    }
    serde_json::to_string_pretty(&root).unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    const HASH: &str = "0123456789abcdef0123456789ABCDEF";

    fn env_of(pairs: &[(&str, &str)]) -> impl Fn(&str) -> Option<String> {
        let map: HashMap<String, String> = pairs
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect();
        move |key| map.get(key).cloned()
    }

    fn creds(api_id: i32) -> ApiCredentials {
        ApiCredentials {
            api_id,
            api_hash: HASH.into(),
        }
    }

    fn resolve_with(
        env: &[(&str, &str)],
        compile_time: Sources<'_>,
        settings: Option<ApiCredentials>,
    ) -> Config {
        resolve(
            env_of(env),
            compile_time,
            || PathBuf::from("/default"),
            |_| settings,
        )
    }

    #[test]
    fn runtime_env_has_priority() {
        let config = resolve_with(
            &[(ENV_API_ID, "11"), (ENV_API_HASH, HASH)],
            Sources {
                api_id: Some("22"),
                api_hash: Some(HASH),
            },
            Some(creds(33)),
        );
        assert_eq!(config.api, Some(creds(11)));
    }

    #[test]
    fn compile_time_beats_settings() {
        let compile_time = Sources {
            api_id: Some("22"),
            api_hash: Some(HASH),
        };
        let config = resolve_with(&[], compile_time, Some(creds(33)));
        assert_eq!(config.api, Some(creds(22)));
    }

    #[test]
    fn invalid_sources_fall_through() {
        let config = resolve_with(
            &[(ENV_API_ID, "abc"), (ENV_API_HASH, HASH)],
            Sources {
                api_id: Some("22"),
                api_hash: None,
            },
            Some(creds(33)),
        );
        assert_eq!(config.api, Some(creds(33)));

        let config = resolve_with(
            &[(ENV_API_ID, "5"), (ENV_API_HASH, "short")],
            Sources::default(),
            None,
        );
        assert_eq!(config.api, None);
    }

    #[test]
    fn data_dir_and_test_dc() {
        let config = resolve_with(&[], Sources::default(), None);
        assert_eq!(config.data_dir, PathBuf::from("/default"));
        assert!(!config.use_test_dc);

        let config = resolve_with(
            &[(ENV_DATA_DIR, "/tmp/tg"), (ENV_TEST_DC, "1")],
            Sources::default(),
            None,
        );
        assert_eq!(config.data_dir, PathBuf::from("/tmp/tg"));
        assert!(config.use_test_dc);

        let config = resolve_with(
            &[(ENV_DATA_DIR, "  "), (ENV_TEST_DC, "maybe")],
            Sources::default(),
            None,
        );
        assert_eq!(config.data_dir, PathBuf::from("/default"));
        assert!(!config.use_test_dc);
    }

    #[test]
    fn settings_are_parsed_and_validated() {
        let json = format!(r#"{{"api_id": 42, "api_hash": "{HASH}"}}"#);
        assert_eq!(parse_settings(&json), Some(creds(42)));
        let json = format!(r#"{{"api_id": "42", "api_hash": "{HASH}"}}"#);
        assert_eq!(parse_settings(&json), Some(creds(42)));
        assert_eq!(parse_settings(r#"{"api_id": 42}"#), None);
        assert_eq!(parse_settings(r#"{"api_id": -1, "api_hash": "x"}"#), None);
        assert_eq!(parse_settings("not json"), None);
    }

    #[test]
    fn merge_keeps_unknown_keys() {
        let merged = merge_settings(Some(r#"{"theme": "light", "api_id": 1}"#), &creds(7));
        let value: serde_json::Value = serde_json::from_str(&merged).unwrap();
        assert_eq!(value["theme"], "light");
        assert_eq!(value["api_id"], 7);
        assert_eq!(parse_settings(&merged), Some(creds(7)));
        assert_eq!(
            parse_settings(&merge_settings(Some("[1]"), &creds(8))),
            Some(creds(8))
        );
    }

    #[test]
    fn save_and_read_round_trip() {
        let dir = std::env::temp_dir().join(format!("telegradus-config-{}", std::process::id()));
        let nested = dir.join("nested");
        save_credentials(&nested, &creds(9)).unwrap();
        assert_eq!(read_settings(&nested), Some(creds(9)));
        fs::remove_dir_all(&dir).unwrap();
    }
}
