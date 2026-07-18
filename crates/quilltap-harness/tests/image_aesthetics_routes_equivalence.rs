//! P4.6ar SYSTEM IMAGE-AESTHETICS route-surface differential:
//! `api::system::system_image_aesthetics_{get,set}` vs v4's REAL
//! `app/api/v1/system/image-aesthetics` handlers. Both sides read a FRESH copy of
//! the committed `inspector-*` family per case — `inspector-main.db` for the
//! provisioned-store arms, `inspector-nostore-main.db` (the same DB minus the
//! singleton `generalMountPointId` row) for the unprovisioned ones.
//!
//! The mutating cases re-GET afterwards and diff `extra.after` against the
//! oracle's, so the store EFFECT is compared and not just the `{success: true}`
//! ack — which would pass vacuously against a handler that wrote nothing.
//!
//! Generate the oracle (Node 24, from the v4 checkout — see the .ts header):
//!   … QT_ORACLE_OUT=/tmp/oracle-image-aesthetics.ndjson npx jest -- image-aesthetics-routes
//! Run:
//!   QT_ORACLE_IMAGE_AESTHETICS=/tmp/oracle-image-aesthetics.ndjson \
//!     cargo test -p quilltap-harness --test image_aesthetics_routes_equivalence -- --nocapture

use std::collections::HashMap;
use std::path::PathBuf;

use quilltap_core::api::system;
use quilltap_core::api::types::{ErrorKind, Response};
use quilltap_core::db::runtime::{Db, DbPaths};
use serde_json::Value;

const TEST_PEPPER: &str = "dGVzdHBlcHBlcnRlc3RwZXBwZXJ0ZXN0cGVwcGVyMDE=";

fn fixtures_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../quilltap-web/tests/fixtures")
}

fn env_or_skip(key: &str) -> Option<String> {
    match std::env::var(key) {
        Ok(v) => Some(v),
        Err(_) => {
            eprintln!("SKIP: set {key} (see test header).");
            None
        }
    }
}

/// A fresh Db over a private copy of the fixture. `store` picks the main file:
/// the provisioned one, or the pointer-less `nostore` sibling.
fn fresh_db(tag: &str, store: &str) -> Db {
    let scratch = std::env::temp_dir().join(format!("qt-aes-{}-{}", tag, std::process::id()));
    let _ = std::fs::remove_dir_all(&scratch);
    std::fs::create_dir_all(&scratch).unwrap();
    let main = scratch.join("main.db");
    let mount = scratch.join("mount.db");
    let main_fixture = if store == "nostore" {
        "inspector-nostore-main.db"
    } else {
        "inspector-main.db"
    };
    std::fs::copy(fixtures_dir().join(main_fixture), &main).unwrap();
    std::fs::copy(fixtures_dir().join("inspector-mount.db"), &mount).unwrap();
    Db::open(
        DbPaths {
            main,
            mount_index: Some(mount),
            llm_logs: None,
        },
        TEST_PEPPER,
    )
    .expect("open db")
}

fn status_of(kind: ErrorKind) -> u16 {
    match kind {
        ErrorKind::BadRequest => 400,
        ErrorKind::Unauthorized => 401,
        ErrorKind::Forbidden => 403,
        ErrorKind::NotFound => 404,
        ErrorKind::Conflict => 409,
        ErrorKind::Unprocessable => 422,
        ErrorKind::Locked => 503,
        ErrorKind::Internal => 500,
    }
}
fn success_body(r: &Response) -> Option<Value> {
    match r {
        Response::SystemAesthetic(v) => Some(v.clone()),
        _ => None,
    }
}

fn check(oracle: &HashMap<String, Value>, name: &str, resp: &Response, failed: &mut Vec<String>) {
    let want = &oracle[name];
    let want_status = want["status"].as_u64().unwrap() as u16;
    if (200..300).contains(&want_status) {
        match success_body(resp) {
            Some(body) => {
                if body != want["body"] {
                    eprintln!(
                        "[{name}] BODY MISMATCH:\n got {body}\n want {}",
                        want["body"]
                    );
                    failed.push(name.to_string());
                } else {
                    eprintln!("[{name}] OK.");
                }
            }
            None => {
                eprintln!("[{name}] expected success, got {resp:?}");
                failed.push(name.to_string());
            }
        }
    } else {
        match resp {
            Response::Error(e) => {
                let want_msg = want["body"]["error"].as_str().unwrap_or("");
                if status_of(e.kind) == want_status && e.message == want_msg {
                    eprintln!("[{name}] OK ({want_status}).");
                } else {
                    eprintln!(
                        "[{name}] MISMATCH: got {} {:?} / want {want_status} {want_msg:?}",
                        status_of(e.kind),
                        e.message
                    );
                    failed.push(name.to_string());
                }
            }
            other => {
                eprintln!("[{name}] expected error, got {other:?}");
                failed.push(name.to_string());
            }
        }
    }
}

/// The store effect: what a follow-up GET reports after a mutating case.
fn check_after(
    oracle: &HashMap<String, Value>,
    name: &str,
    db: &Db,
    kind: &str,
    failed: &mut Vec<String>,
) {
    let want = &oracle[name]["extra"]["after"];
    let got = success_body(&system::system_image_aesthetics_get(db, kind))
        .map(|b| b["content"].clone())
        .unwrap_or(Value::Null);
    if &got != want {
        eprintln!("[{name}] after MISMATCH:\n got {got}\n want {want}");
        failed.push(format!("{name}:after"));
    } else {
        eprintln!("[{name}] after OK.");
    }
}

#[test]
fn image_aesthetics_routes_match_oracle() {
    let Some(oracle_path) = env_or_skip("QT_ORACLE_IMAGE_AESTHETICS") else {
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

    let mut failed: Vec<String> = Vec::new();
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();

    // ── The kind guard ──
    check(
        &oracle,
        "get_invalid_kind",
        &system::system_image_aesthetics_get(&fresh_db("get_invalid", "main"), "bogus"),
        &mut failed,
    );
    // An absent `kind` reaches the handler as '' (what the REST edge resolves it
    // to) and lands on the same 400.
    check(
        &oracle,
        "get_absent_kind",
        &system::system_image_aesthetics_get(&fresh_db("get_absent", "main"), ""),
        &mut failed,
    );
    check(
        &oracle,
        "put_invalid_kind",
        &rt.block_on(system::system_image_aesthetics_set(
            &fresh_db("put_invalid", "main"),
            "bogus",
            Some("x".to_string()),
        )),
        &mut failed,
    );

    // ── GET ──
    check(
        &oracle,
        "get_lantern_existing",
        &system::system_image_aesthetics_get(&fresh_db("get_lantern", "main"), "lantern"),
        &mut failed,
    );
    // No aurora file was ever seeded → ''.
    check(
        &oracle,
        "get_aurora_absent",
        &system::system_image_aesthetics_get(&fresh_db("get_aurora", "main"), "aurora"),
        &mut failed,
    );

    // ── PUT (each re-read afterwards, so the store effect is the real diff) ──
    // The last four all land on '' — empty, whitespace-only, a malformed body,
    // and a non-string `content` all resolve to '' and DELETE the file. The two
    // non-string arms reach the boundary as `None` (the REST edge's parse maps
    // every non-string path there).
    for (name, kind, content) in [
        (
            "put_aurora_then_get",
            "aurora",
            Some("# Aurora\n\nSilk, and too much of it.\n".to_string()),
        ),
        (
            "put_lantern_overwrite_then_get",
            "lantern",
            Some("Replaced.".to_string()),
        ),
        ("put_lantern_empty_deletes", "lantern", Some(String::new())),
        (
            "put_lantern_whitespace_deletes",
            "lantern",
            Some("   \n\t ".to_string()),
        ),
        ("put_lantern_malformed_body_deletes", "lantern", None),
        ("put_lantern_non_string_content_deletes", "lantern", None),
    ] {
        let db = fresh_db(name, "main");
        let resp = rt.block_on(system::system_image_aesthetics_set(&db, kind, content));
        check(&oracle, name, &resp, &mut failed);
        check_after(&oracle, name, &db, kind, &mut failed);
    }

    // ── The unprovisioned Quilltap General store ──
    // The GET succeeds with ''; the PUT refuses.
    check(
        &oracle,
        "get_no_store",
        &system::system_image_aesthetics_get(&fresh_db("get_no_store", "nostore"), "lantern"),
        &mut failed,
    );
    check(
        &oracle,
        "put_no_store",
        &rt.block_on(system::system_image_aesthetics_set(
            &fresh_db("put_no_store", "nostore"),
            "lantern",
            Some("x".to_string()),
        )),
        &mut failed,
    );

    assert!(failed.is_empty(), "cases failed: {failed:?}");
}
