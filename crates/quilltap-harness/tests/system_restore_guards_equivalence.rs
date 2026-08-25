//! P4.60 — the restore route's BODY GUARDS vs v4's REAL route handlers.
//!
//! `app/api/v1/system/restore/route.ts` uses no Zod at all: it destructures
//! `{ uploadId, mode }` and guards them by hand. That makes the questions here
//! JS-semantic rather than schema-shaped, and it is exactly where v5's
//! `and_then(Value::as_str)` read went wrong:
//!
//! - `if (!uploadId)` is JS **falsiness**, so `0`, `false` and `''` answer
//!   `uploadId is required` just as an absent key does;
//! - a truthy WRONG-TYPED value passes that guard and reaches
//!   `UUID_REGEX.test(uploadId)`, which `String()`-coerces it — so it answers
//!   `Upload not found or expired`, a different sentence entirely;
//! - the guards run in the order uploadId → mode → the upload lookup. v5 used
//!   to check `uploadId` first at the REST edge and `mode` first at the dispatch
//!   entrance, so the two entrances disagreed with each other and one of them
//!   with v4.
//!
//! Every arm stops inside those guards, so the oracle needs no provisioned
//! instance (v4's `pendingUploads` map is module-local and empty on a fresh
//! import) and this side needs no populated database — the `Db` below is a
//! read-only copy of a committed fixture that `restore_execute` never reaches.
//!
//! Generate the oracle (Node 24, from the v4 checkout — see the .ts header):
//!   N=~/.nvm/versions/node/v24.13.1/bin ; V5W=${V5W:-$HOME/source/quilltap-v5}
//!   TMPO=/tmp/qt-restore-guards-oracle
//!   rm -rf "$TMPO"; mkdir -p "$TMPO/cases"
//!   cp "$V5W/harness/oracle/cases/system-restore-guards.test.ts" "$TMPO/cases/"
//!   cd ~/source/quilltap-server
//!   QT_ORACLE_OUT=/tmp/oracle-system-restore-guards.ndjson \
//!     $N/npx jest --silent --watchman=false --testTimeout=120000 \
//!       --roots "$PWD" --roots "$TMPO/cases" -- system-restore-guards
//! Run:
//!   QT_ORACLE_RESTORE_GUARDS=/tmp/oracle-system-restore-guards.ndjson \
//!     cargo test -p quilltap-harness --test system_restore_guards_equivalence

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use quilltap_core::api::system_backup::{restore_execute, restore_preview};
use quilltap_core::api::types::{ErrorKind, Response};
use quilltap_core::db::runtime::{Db, DbPaths};
use quilltap_core::services::backup::{BackupHost, HostDirs};
use quilltap_core::services::file_storage::{PixelCodec, StorageBackend};
use serde_json::{json, Value};

/// The committed characters fixture's pepper (this family only needs a `Db`
/// handle that opens — no arm reaches a query).
const PEPPER: &str = "dGVzdHBlcHBlcnRlc3RwZXBwZXJ0ZXN0cGVwcGVyMDE=";

/// A host whose pending-upload store is EMPTY — v4's freshly imported route
/// module, exactly.
struct EmptyUploadHost {
    root: PathBuf,
}

impl BackupHost for EmptyUploadHost {
    fn storage(&self) -> Arc<dyn StorageBackend> {
        Arc::new(quilltap_core::services::file_storage::NotConfiguredStorageBackend)
    }
    fn pixel_codec(&self) -> Arc<dyn PixelCodec> {
        Arc::new(quilltap_core::services::file_storage::NotConfiguredPixelCodec)
    }
    fn temp_dir(&self) -> PathBuf {
        self.root.clone()
    }
    fn host_dirs(&self) -> HostDirs {
        HostDirs::default()
    }
    fn app_version(&self) -> String {
        "<normalized>".to_string()
    }
    fn now_ms(&self) -> i64 {
        0
    }
    fn store_backup(&self, _id: &str, _p: &Path) {}
    fn take_backup(&self, _id: &str) -> Option<PathBuf> {
        None
    }
    fn store_upload(&self, _id: &str, _p: &Path) {}
    fn get_upload(&self, _id: &str) -> Option<PathBuf> {
        None
    }
    fn remove_upload(&self, _id: &str) {}
}

fn status_body(resp: &Response) -> (u64, Value) {
    match resp {
        Response::System(v) => (200, v.clone()),
        Response::Error(e) => {
            let status = match e.kind {
                ErrorKind::BadRequest => 400,
                ErrorKind::NotFound => 404,
                ErrorKind::Unavailable => 503,
                _ => 500,
            };
            (status, json!({ "error": e.message }))
        }
        other => panic!("unexpected response: {other:?}"),
    }
}

fn open_db(scratch: &Path) -> Db {
    let fixture = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../quilltap-web/tests/fixtures/characters-main.db");
    let main = scratch.join("main.db");
    std::fs::copy(&fixture, &main).expect("copy the fixture main db");
    Db::open(
        DbPaths {
            main,
            mount_index: None,
            llm_logs: None,
        },
        PEPPER,
    )
    .expect("open db")
}

#[test]
fn restore_route_guards_match_oracle() {
    let Ok(oracle_path) = std::env::var("QT_ORACLE_RESTORE_GUARDS") else {
        eprintln!("SKIP: set QT_ORACLE_RESTORE_GUARDS (see the test header).");
        return;
    };
    let mut oracle: HashMap<String, Value> = HashMap::new();
    for line in std::fs::read_to_string(&oracle_path)
        .unwrap()
        .lines()
        .filter(|l| !l.trim().is_empty())
    {
        let v: Value = serde_json::from_str(line).unwrap();
        oracle.insert(v["name"].as_str().unwrap().to_string(), v);
    }

    let scratch = std::env::temp_dir().join(format!("qt-restore-guards-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&scratch);
    std::fs::create_dir_all(&scratch).unwrap();
    let db = open_db(&scratch);
    let host = EmptyUploadHost {
        root: scratch.clone(),
    };
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();

    // `(case, body)` — the SAME bodies the oracle posts. `None` for the
    // `?action=preview` leg; the rest go through the default restore action.
    const UUID: &str = "11111111-1111-4111-8111-111111111111";
    let previews: Vec<(&str, Value)> = vec![
        ("preview_upload_id_absent", json!({})),
        ("preview_upload_id_null", json!({ "uploadId": null })),
        ("preview_upload_id_empty", json!({ "uploadId": "" })),
        ("preview_upload_id_zero", json!({ "uploadId": 0 })),
        ("preview_upload_id_false", json!({ "uploadId": false })),
        ("preview_upload_id_number", json!({ "uploadId": 123 })),
        ("preview_upload_id_true", json!({ "uploadId": true })),
        (
            "preview_upload_id_object",
            json!({ "uploadId": { "a": 1 } }),
        ),
        ("preview_upload_id_array", json!({ "uploadId": ["x"] })),
        (
            "preview_upload_id_unknown_uuid",
            json!({ "uploadId": UUID }),
        ),
    ];
    let restores: Vec<(&str, Value)> = vec![
        (
            "restore_upload_id_absent_bad_mode",
            json!({ "mode": "nope" }),
        ),
        (
            "restore_upload_id_number_bad_mode",
            json!({ "uploadId": 123, "mode": "nope" }),
        ),
        ("restore_mode_absent", json!({ "uploadId": UUID })),
        (
            "restore_mode_wrong_type",
            json!({ "uploadId": UUID, "mode": 7 }),
        ),
        (
            "restore_mode_unknown",
            json!({ "uploadId": UUID, "mode": "merge" }),
        ),
        (
            "restore_valid_shape_unknown_upload",
            json!({ "uploadId": UUID, "mode": "replace" }),
        ),
        (
            "restore_keep_bundles_wrong_type",
            json!({ "uploadId": UUID, "mode": "replace",
                    "keepArchivedCharacterBundles": "no" }),
        ),
    ];

    let mut failed: Vec<String> = Vec::new();
    let mut checked = 0usize;
    let check = |name: &str, got: (u64, Value), failed: &mut Vec<String>| {
        let want = oracle
            .get(name)
            .unwrap_or_else(|| panic!("oracle missing case '{name}'"));
        if got.0 != want["status"].as_u64().unwrap() {
            eprintln!("[{name}] STATUS {} != {}", got.0, want["status"]);
            failed.push(format!("{name}_status"));
        } else if got.1 != want["body"] {
            eprintln!("[{name}] BODY {} != {}", got.1, want["body"]);
            failed.push(name.to_string());
        } else {
            eprintln!("[{name}] OK ({}).", got.0);
        }
    };

    for (name, body) in &previews {
        let resp = restore_preview(&host, body.get("uploadId").unwrap_or(&Value::Null));
        check(name, status_body(&resp), &mut failed);
        checked += 1;
    }
    for (name, body) in &restores {
        let resp = rt.block_on(restore_execute(
            &db,
            &host,
            body.get("uploadId").unwrap_or(&Value::Null),
            body.get("mode").unwrap_or(&Value::Null),
            body.get("keepArchivedCharacterBundles"),
        ));
        check(name, status_body(&resp), &mut failed);
        checked += 1;
    }

    let _ = std::fs::remove_dir_all(&scratch);
    // Declared on BOTH sides, so a case added to the oracle and forgotten here
    // would pass silently on a smaller set.
    assert_eq!(
        checked,
        oracle.len(),
        "the Rust case list and the oracle disagree: {checked} vs {}",
        oracle.len()
    );
    assert!(failed.is_empty(), "restore-guards FAILED: {failed:?}");
    eprintln!("OK: restore route guards matched oracle ({checked} cases).");
}
