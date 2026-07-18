//! P4.9c USER-PROFILE differential: `api::user_profile`'s three handlers vs
//! v4's REAL `app/api/v1/user/profile/route.ts` GET / PUT / PATCH handlers.
//! Both sides read a FRESH copy of the committed `profile-{main,mount}.db`
//! family per case.
//!
//! Tier 2 (structural): the response body AND the persisted `users` row are
//! both diffed — a handler can answer correctly while storing the wrong thing,
//! and the avatar pointer write is exactly that kind of claim. The single
//! normalized field is `updatedAt`, which is a live clock on v4's side; the
//! normalizer additionally ASSERTS that v4's value moved off the seed timestamp
//! for every mutation case, so "normalized" can never quietly mean "neither
//! side wrote".
//!
//! The route's `theme-preference` arms are NOT covered: already ported in v5
//! via `theme.service` over chatSettings, and out of this lane's scope.
//!
//! Generate the oracle (Node 24, from the PINNED v4 worktree — see the .ts):
//!   … QT_ORACLE_OUT=/tmp/oracle-profile.ndjson npx jest -- profile-routes
//! Run:
//!   QT_ORACLE_PROFILE=/tmp/oracle-profile.ndjson \
//!     cargo test -p quilltap-harness --test profile_routes_equivalence -- --nocapture

use std::path::PathBuf;

use quilltap_core::api::types::{ErrorKind, Response};
use quilltap_core::api::user_profile;
use quilltap_core::db::runtime::{Db, DbPaths};
use serde_json::{json, Value};

const TEST_PEPPER: &str = "dGVzdHBlcHBlcnRlc3RwZXBwZXJ0ZXN0cGVwcGVyMDE=";
/// The fixture's pinned `createdAt`/`updatedAt` — a mutation must move off it.
const SEED_TS: &str = "2026-06-01T00:00:00.000Z";
/// The clock this side injects; every mutated `updatedAt` must equal it.
const FIXED_NOW: &str = "2026-07-18T12:00:00.000Z";
/// What both sides' `updatedAt` collapses to once each has been checked.
const NORMALIZED: &str = "<updatedAt>";

fn fixtures_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../quilltap-web/tests/fixtures")
}

fn spec_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../harness/oracle/fixtures/profile-web.json")
}

fn fresh_db(tag: &str) -> Db {
    let scratch = std::env::temp_dir().join(format!("qt-profile-{}-{}", tag, std::process::id()));
    let _ = std::fs::remove_dir_all(&scratch);
    std::fs::create_dir_all(&scratch).unwrap();
    let main = scratch.join("main.db");
    let mount = scratch.join("mount.db");
    std::fs::copy(fixtures_dir().join("profile-main.db"), &main).unwrap();
    std::fs::copy(fixtures_dir().join("profile-mount.db"), &mount).unwrap();
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
        Response::UserProfile(v) => Some(v.clone()),
        _ => None,
    }
}

/// The `users` row this side persisted, in the oracle's column order.
fn persisted_row(db: &Db, user_id: &str) -> Value {
    let id = user_id.to_string();
    db.read_main(move |conn| {
        conn.query_row(
            "SELECT id, username, email, name, image, createdAt, updatedAt \
             FROM users WHERE id = ?1",
            rusqlite::params![id],
            |row| {
                Ok(json!({
                    "id": row.get::<_, String>(0)?,
                    "username": row.get::<_, String>(1)?,
                    "email": row.get::<_, Option<String>>(2)?,
                    "name": row.get::<_, Option<String>>(3)?,
                    "image": row.get::<_, Option<String>>(4)?,
                    "createdAt": row.get::<_, String>(5)?,
                    "updatedAt": row.get::<_, String>(6)?,
                }))
            },
        )
        .map_err(quilltap_core::db::DbError::from)
    })
    .expect("read back the users row")
}

/// Sort object keys so a mismatch reports VALUES, not ordering (key order is
/// pinned separately).
fn sorted(v: &Value) -> Value {
    match v {
        Value::Object(o) => {
            let mut m = serde_json::Map::new();
            let mut keys: Vec<&String> = o.keys().collect();
            keys.sort();
            for k in keys {
                m.insert(k.clone(), sorted(&o[k]));
            }
            Value::Object(m)
        }
        Value::Array(a) => Value::Array(a.iter().map(sorted).collect()),
        other => other.clone(),
    }
}
fn norm(v: &Value) -> String {
    serde_json::to_string_pretty(&sorted(v)).unwrap()
}

/// Replace every `updatedAt` with the sentinel, after checking it against
/// `expect`: for a mutation the value must have MOVED off the seed (v4) or be
/// exactly the injected clock (v5). Returns the number of fields rewritten.
fn normalize_updated_at(v: &mut Value, expect: &dyn Fn(&str) -> bool, ctx: &str) -> usize {
    let mut n = 0;
    match v {
        Value::Object(o) => {
            for (k, x) in o.iter_mut() {
                if k == "updatedAt" {
                    if let Some(s) = x.as_str() {
                        assert!(expect(s), "[{ctx}] unexpected updatedAt {s:?}");
                        n += 1;
                    }
                    *x = Value::String(NORMALIZED.to_string());
                } else {
                    n += normalize_updated_at(x, expect, ctx);
                }
            }
        }
        Value::Array(a) => {
            for x in a.iter_mut() {
                n += normalize_updated_at(x, expect, ctx);
            }
        }
        _ => {}
    }
    n
}

/// Every object's key sequence, depth-first — what the sorted compare discards.
fn key_paths(v: &Value, path: &str, out: &mut Vec<String>) {
    match v {
        Value::Object(o) => {
            out.push(format!(
                "{path}: {}",
                o.keys().cloned().collect::<Vec<_>>().join(",")
            ));
            for (k, x) in o.iter() {
                key_paths(x, &format!("{path}/{k}"), out);
            }
        }
        Value::Array(a) => {
            for (i, x) in a.iter().enumerate() {
                key_paths(x, &format!("{path}[{i}]"), out);
            }
        }
        _ => {}
    }
}

fn field(payload: &Value, key: &str) -> Option<Option<Value>> {
    match payload.get(key) {
        None => None,
        Some(Value::Null) => Some(None),
        Some(v) => Some(Some(v.clone())),
    }
}

#[test]
fn profile_routes_match_oracle() {
    let Ok(oracle_path) = std::env::var("QT_ORACLE_PROFILE") else {
        eprintln!("SKIP: set QT_ORACLE_PROFILE (see test header).");
        return;
    };

    let spec: Value = serde_json::from_str(&std::fs::read_to_string(spec_path()).unwrap()).unwrap();
    let user_id = spec["userId"].as_str().unwrap().to_string();
    let missing_user_id = spec["missingUserId"].as_str().unwrap().to_string();

    let raw = std::fs::read_to_string(&oracle_path).expect("read oracle NDJSON");
    let rows: Vec<Value> = raw
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| serde_json::from_str(l).expect("parse oracle row"))
        .collect();
    assert!(!rows.is_empty(), "oracle NDJSON is empty");

    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();

    let mut failed: Vec<String> = Vec::new();
    let mut key_order_checked = false;

    for row in &rows {
        let name = row["name"].as_str().unwrap().to_string();
        let verb = row["verb"].as_str().unwrap();
        let payload = row["payload"].clone();
        let want_status = row["status"].as_u64().unwrap() as u16;

        // The GET's missing-user case runs against an id with no row; every
        // other case is the primary user.
        let acting_id: &str = if name == "get_missing_user" {
            &missing_user_id
        } else {
            &user_id
        };

        let db = fresh_db(&name);
        let resp = match verb {
            "get" => user_profile::user_profile_get(&db, acting_id),
            "update" => rt.block_on(user_profile::user_profile_update(
                &db,
                acting_id,
                field(&payload, "name"),
                field(&payload, "email"),
                field(&payload, "image"),
                FIXED_NOW,
            )),
            "setAvatar" => rt.block_on(user_profile::user_profile_set_avatar(
                &db,
                acting_id,
                field(&payload, "imageId"),
                FIXED_NOW,
            )),
            other => panic!("[{name}] unknown verb {other:?} in the oracle row"),
        };

        // ── the response ──
        if (200..300).contains(&want_status) {
            let Some(mut got_body) = success_body(&resp) else {
                eprintln!("[{name}] expected success, got {resp:?}");
                failed.push(name);
                continue;
            };
            let mut want_body = row["body"].clone();

            // The wire-order claim, on the richest success body.
            if !key_order_checked && name == "get_primary" {
                let mut want_keys = Vec::new();
                key_paths(&want_body, "", &mut want_keys);
                let mut got_keys = Vec::new();
                key_paths(&got_body, "", &mut got_keys);
                if got_keys != want_keys {
                    eprintln!(
                        "[{name}] KEY ORDER MISMATCH:\n got {got_keys:#?}\n want {want_keys:#?}"
                    );
                    failed.push(format!("{name}:keyOrder"));
                } else {
                    eprintln!("[{name}] key order OK.");
                }
                key_order_checked = true;
            }

            let mutating = verb != "get";
            let want_check: Box<dyn Fn(&str) -> bool> = if mutating {
                Box::new(|s: &str| s != SEED_TS)
            } else {
                Box::new(|s: &str| s == SEED_TS)
            };
            let got_check: Box<dyn Fn(&str) -> bool> = if mutating {
                Box::new(|s: &str| s == FIXED_NOW)
            } else {
                Box::new(|s: &str| s == SEED_TS)
            };
            normalize_updated_at(&mut want_body, &want_check, &format!("{name}/v4"));
            normalize_updated_at(&mut got_body, &got_check, &format!("{name}/v5"));

            if norm(&got_body) != norm(&want_body) {
                eprintln!(
                    "[{name}] BODY MISMATCH:\n got {}\n want {}",
                    norm(&got_body),
                    norm(&want_body)
                );
                failed.push(name.clone());
            } else {
                eprintln!("[{name}] OK.");
            }
        } else {
            match &resp {
                Response::Error(e) => {
                    let want_msg = row["body"]["error"].as_str().unwrap_or("");
                    if status_of(e.kind) == want_status && e.message == want_msg {
                        eprintln!("[{name}] OK ({want_status}).");
                    } else {
                        eprintln!(
                            "[{name}] MISMATCH: got {} {:?} / want {want_status} {want_msg:?}",
                            status_of(e.kind),
                            e.message
                        );
                        failed.push(name.clone());
                    }
                }
                other => {
                    eprintln!("[{name}] expected error, got {other:?}");
                    failed.push(name.clone());
                }
            }
        }

        // ── the persisted row (every case, including the rejections — a 400
        //    that still wrote would be a silent data bug) ──
        let mut got_row = persisted_row(&db, &user_id);
        let mut want_row = row["row"].clone();
        // Whether the WRITE happened is decided by v4's row, not by the status:
        // a rejected PUT leaves the seed timestamp in place.
        let v4_wrote = want_row["updatedAt"].as_str() != Some(SEED_TS);
        let want_check: Box<dyn Fn(&str) -> bool> = if v4_wrote {
            Box::new(|s: &str| s != SEED_TS)
        } else {
            Box::new(|s: &str| s == SEED_TS)
        };
        let got_check: Box<dyn Fn(&str) -> bool> = if v4_wrote {
            Box::new(|s: &str| s == FIXED_NOW)
        } else {
            Box::new(|s: &str| s == SEED_TS)
        };
        normalize_updated_at(&mut want_row, &want_check, &format!("{name}/row-v4"));
        normalize_updated_at(&mut got_row, &got_check, &format!("{name}/row-v5"));

        if norm(&got_row) != norm(&want_row) {
            eprintln!(
                "[{name}] ROW MISMATCH:\n got {}\n want {}",
                norm(&got_row),
                norm(&want_row)
            );
            failed.push(format!("{name}:row"));
        }
    }

    eprintln!("profile differential: {} cases.", rows.len());
    assert!(failed.is_empty(), "cases diverged from v4: {failed:?}");
    assert!(
        key_order_checked,
        "the get_primary key-order case never ran"
    );

    // Corpus-coverage guard — a thinned oracle must fail loudly.
    let names: Vec<&str> = rows.iter().map(|r| r["name"].as_str().unwrap()).collect();
    for required in [
        "get_primary",
        "get_missing_user",
        "put_name_only",
        "put_my_own_email",
        "put_email_already_in_use",
        "put_explicit_nulls",
        "put_image_empty_string",
        "patch_avatar_image_category",
        "patch_avatar_avatar_category",
        "patch_avatar_null_clears",
        "patch_avatar_wrong_category",
        "patch_avatar_missing_file",
    ] {
        assert!(
            names.contains(&required),
            "oracle is missing the {required:?} case"
        );
    }
}

/// The §3 Shared-contract wire shapes (always on — a serialization contract,
/// not a behavior diff). Pins the verb names and the payload keys the SPA
/// sends, so a later rename cannot slip past.
#[test]
fn p4_9c_wire_contract() {
    use quilltap_core::api::types::Request;

    assert_eq!(
        serde_json::from_str::<Request>(r#"{"type":"userProfileGet"}"#).expect("userProfileGet"),
        Request::UserProfileGet
    );
    assert_eq!(
        serde_json::from_str::<Request>(r#"{"type":"systemDataDir"}"#).expect("systemDataDir"),
        Request::SystemDataDir
    );

    // The double-option decode: absent / explicit null / value are three
    // distinct states, and collapsing them would erase v4's validation arms.
    let absent: Request = serde_json::from_str(r#"{"type":"userProfileUpdate"}"#).unwrap();
    assert_eq!(
        absent,
        Request::UserProfileUpdate {
            name: None,
            email: None,
            image: None
        }
    );
    let explicit_null: Request =
        serde_json::from_str(r#"{"type":"userProfileUpdate","name":null}"#).unwrap();
    assert_eq!(
        explicit_null,
        Request::UserProfileUpdate {
            name: Some(None),
            email: None,
            image: None
        }
    );
    let valued: Request =
        serde_json::from_str(r#"{"type":"userProfileUpdate","name":"Friday","email":"f@x.com"}"#)
            .unwrap();
    assert_eq!(
        valued,
        Request::UserProfileUpdate {
            name: Some(Some(json!("Friday"))),
            email: Some(Some(json!("f@x.com"))),
            image: None
        }
    );

    let avatar: Request =
        serde_json::from_str(r#"{"type":"userProfileSetAvatar","imageId":null}"#).unwrap();
    assert_eq!(
        avatar,
        Request::UserProfileSetAvatar {
            image_id: Some(None)
        }
    );

    // The success envelopes.
    assert_eq!(
        serde_json::to_string(&Response::UserProfile(json!({ "profile": { "id": "u1" } })))
            .unwrap(),
        r#"{"type":"userProfile","data":{"profile":{"id":"u1"}}}"#
    );
    assert_eq!(
        serde_json::to_string(&Response::SystemDataDir(json!({ "path": "/tmp" }))).unwrap(),
        r#"{"type":"systemDataDir","data":{"path":"/tmp"}}"#
    );
}
