//! The data-directory dispatch surface (P4.9c) — the glue over
//! [`crate::services::data_dir`], which holds the differentialed port of v4's
//! `lib/paths.ts` family. Ports v4
//! `app/api/v1/system/data-dir/route.ts`'s `GET` (`createContextHandler`, no
//! unlock requirement — the handler reads the environment, never the vault).
//!
//! ## The one deliberate divergence: the `--data-dir` flag
//!
//! v4 resolves the data dir from the environment alone, so the route can simply
//! answer `getBaseDataDirWithSource()`. v5's host also accepts an explicit
//! `--data-dir` flag, and the engine is already holding the base dir it is
//! REALLY serving (`CoreConfig::base_dir`). Reporting the env-derived guess
//! when the two disagree would mean the Profile screen tells the user their data
//! lives somewhere it does not — the exact question the screen exists to answer.
//!
//! So: the faithful resolver runs first, and when its answer disagrees with the
//! engine's actual base dir (which can only happen on the explicit-flag path),
//! `path` and `hostPath` are corrected and the pair is reported as
//! `source: "environment"` — v4's own value for "something outside the platform
//! default chose this" — with a `sourceDescription` naming the flag.
//! `sourceDescription` is free-form display text in v4 too, so the wire TYPE is
//! unchanged; only the sentence is v5's.
//!
//! ## The refusal
//!
//! v4's `POST ?action=open` shells out to `open`/`explorer`/`xdg-open`. v5
//! refuses it loudly ([`not_available`]) — a Tauri shell-open is a named future
//! native nicety, and the HTTP deployment has no business opening a file
//! browser on the server's desktop anyway. `canOpen` on the payload stays
//! FAITHFUL (`!isDocker`); it describes the environment, and the SPA gates the
//! button on its own knowledge that v5 has no opener.

use std::path::Path;

use serde_json::json;

use crate::services::data_dir::{data_dir_info, BaseDataDirSource, DataDirEnv};

use super::types::{ErrorKind, Response};

/// v4 `GET /api/v1/system/data-dir` — `successResponse(response)`, the payload
/// RAW, in v4's `DataDirInfo` declaration order.
pub fn system_data_dir(actual_base_dir: &Path) -> Response {
    let env = DataDirEnv::from_process();
    let mut info = data_dir_info(&env);

    let actual = actual_base_dir.to_string_lossy().to_string();
    if !actual.is_empty() && actual != info.path {
        // The explicit-flag path — see the header.
        if info.host_path == info.path {
            info.host_path = actual.clone();
        }
        info.path = actual.clone();
        info.source = BaseDataDirSource::Environment;
        info.source_description = format!("--data-dir command-line flag ({actual})");
    }

    Response::SystemDataDir(json!({
        "path": info.path,
        "source": info.source.as_str(),
        "sourceDescription": info.source_description,
        "platform": info.platform.as_str(),
        "isDocker": info.is_docker,
        "isElectronShell": info.is_electron_shell,
        "shellVersion": info.shell_version,
        "shellCapabilities": info.shell_capabilities,
        "canOpen": info.can_open,
        "hostPath": info.host_path,
    }))
}

/// The loud "recognized but not yet available" refusal for the data-dir
/// family's deferred action (`POST ?action=open`).
pub fn not_available(action: &str) -> Response {
    Response::error(
        ErrorKind::Internal,
        format!("The '{action}' data-directory action is recognized but not yet available."),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::Value;

    fn body(resp: Response) -> Value {
        match resp {
            Response::SystemDataDir(v) => v,
            other => panic!("expected a data-dir body, got {other:?}"),
        }
    }

    #[test]
    fn the_explicit_flag_overrides_the_env_derived_path() {
        let v = body(system_data_dir(Path::new("/tmp/some-instance")));
        assert_eq!(v["path"], "/tmp/some-instance");
        assert_eq!(v["hostPath"], "/tmp/some-instance");
        assert_eq!(v["source"], "environment");
        assert_eq!(
            v["sourceDescription"],
            "--data-dir command-line flag (/tmp/some-instance)"
        );
    }

    #[test]
    fn the_payload_keeps_v4s_field_order() {
        let v = body(system_data_dir(Path::new("/tmp/some-instance")));
        let keys: Vec<&str> = v.as_object().unwrap().keys().map(String::as_str).collect();
        assert_eq!(
            keys,
            vec![
                "path",
                "source",
                "sourceDescription",
                "platform",
                "isDocker",
                "isElectronShell",
                "shellVersion",
                "shellCapabilities",
                "canOpen",
                "hostPath",
            ]
        );
    }

    #[test]
    fn the_open_action_refuses_loudly_by_name() {
        let Response::Error(e) = not_available("open") else {
            panic!("expected an error envelope");
        };
        assert!(e.message.contains("'open'"), "{}", e.message);
        assert!(e.message.contains("not yet available"), "{}", e.message);
    }
}
