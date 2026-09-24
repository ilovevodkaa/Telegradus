//! TDLib log configuration.
//!
//! Logging is global in TDLib. It is configured with synchronous requests
//! before the first client exists, so TDLib never writes its startup chatter
//! to stderr. `tdlib-rs` does not expose `td_execute`, so it is declared here;
//! the symbol comes from the tdjson library that `tdlib-rs` links.

use std::ffi::{CStr, CString, c_char};
use std::path::Path;

use serde_json::{Value, json};
use tracing::warn;

/// TDLib log file inside the data directory.
pub(crate) const LOG_FILE: &str = "tdlib.log";
/// Errors and warnings only.
pub(crate) const LOG_VERBOSITY: i32 = 1;
const LOG_FILE_MAX_SIZE: i64 = 10 * 1024 * 1024;

unsafe extern "C" {
    fn td_execute(request: *const c_char) -> *const c_char;
}

/// Sends TDLib's log to a size-limited file in `data_dir` at low verbosity.
pub(crate) fn configure(data_dir: &Path) {
    let path = data_dir.join(LOG_FILE);
    let stream = json!({
        "@type": "setLogStream",
        "log_stream": {
            "@type": "logStreamFile",
            "path": path.to_string_lossy(),
            "max_file_size": LOG_FILE_MAX_SIZE,
            "redirect_stderr": false,
        },
    });
    if let Err(err) = execute(&stream) {
        warn!("cannot redirect the TDLib log to {}: {err}", path.display());
    }
    let verbosity = json!({
        "@type": "setLogVerbosityLevel",
        "new_verbosity_level": LOG_VERBOSITY,
    });
    if let Err(err) = execute(&verbosity) {
        warn!("cannot set the TDLib log verbosity: {err}");
    }
}

/// Runs a request documented as "can be called synchronously".
fn execute(request: &Value) -> Result<(), String> {
    let request = CString::new(request.to_string()).map_err(|err| err.to_string())?;
    // SAFETY: `request` is a valid NUL-terminated string that outlives the call.
    // TDLib returns null or a NUL-terminated string kept in thread-local storage
    // until this thread's next TDLib call; it is copied out immediately.
    let response = unsafe {
        let response = td_execute(request.as_ptr());
        if response.is_null() {
            return Err("no response".to_owned());
        }
        CStr::from_ptr(response).to_string_lossy().into_owned()
    };
    let response: Value = serde_json::from_str(&response).map_err(|err| err.to_string())?;
    if response["@type"] == "error" {
        let message = response["message"].as_str().unwrap_or("unknown error");
        return Err(message.to_owned());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn synchronous_requests_report_errors() {
        assert!(execute(&json!({"@type": "getOption", "name": "version"})).is_ok());
        assert!(execute(&json!({"@type": "getLogVerbosityLevel"})).is_ok());
        assert!(execute(&json!({"@type": "noSuchRequest"})).is_err());
    }
}
