//! P4.80 CHAT-DELETE route-surface differential (dogfood finding #117):
//! `api::chat_delete::chat_delete_dispatch` vs v4's REAL `handleDelete`, driven
//! through v4's own route module so the whole dispatch is v4's — the `?action=`
//! classification, the pre-body 404, the Zod parse, the unknown-action refusal
//! — and underneath them the REAL `ChatsRepository.delete` cascade +
//! `removeConversationSummariesFromVaults`.
//!
//! Both sides read a FRESH copy of the committed
//! `chat-delete-{main,mount,llmlogs}.db` family per case (baked ids identical →
//! no remap), then diff the response body AND a whole-DB TABLE CENSUS across
//! all three partitions.
//!
//! **The census is the discriminator, not the body.** `{success: true}` says
//! nothing about what the cascade reached, and the whole question of this
//! family is what v4 DELETES (the `chats` row, its `chat_messages`, its
//! `conversation_annotations`, and the conversation's summary file in every
//! participant vault) versus what it deliberately LEAVES: `memories` — the
//! client's separate `DELETE /api/v1/memories?chatId=` is the re-extract flow —
//! plus `conversation_chunks`, `chat_documents`, `files`, `background_jobs`,
//! `characters.avatarOverrides`, the Scriptorium render's mount-index rows, and
//! the whole `llm_logs` partition. The census SQL lives in
//! `chat-delete-web.json` and is executed VERBATIM on both sides, so neither
//! side can quietly select a different projection.
//!
//! A fourth section, `vaults`, lists each character's `Conversation Summaries/`
//! folder through the STORE READER on both sides — the arm the raw table dump
//! cannot make, since a reader that stopped resolving a folder would leave
//! every row standing. (MOTE's listing is empty on every case: her pointer is
//! dangling by construction, so the reader has nothing to resolve. The orphaned
//! rows her broken vault keeps live in the mount census above.)
//!
//! `folders` is censused as a flat control: v4's chats carry no `folderId`, so
//! that table must never move on any case.
//!
//! Coverage is asserted by SHAPE, not by a hand-written count: every case name
//! the oracle emitted must have been driven here, and vice versa
//! (`harness-corpus-shape-constants-rot`).
//!
//! ## Nothing is normalized
//!
//! Every id and timestamp in the fixture is pinned by the builder (including
//! the mount-index rows' minted uuids, which are BAKED into the committed file
//! and therefore identical on both sides), and no case mints anything: the
//! delete only ever removes rows. So the census diffs BYTE-for-byte with no
//! normalizer at all — which is what makes "nothing was written" provable for
//! the refusal cases.
//!
//! ## The ONE minted value
//!
//! `stop_impersonate_with_profile` is the only case that WRITES a fresh
//! timestamp: v4's `updateParticipant` stamps the reassigned seat's
//! `updatedAt` (v4 from the oracle's frozen clock, v5 from the real one). It is
//! asserted-then-stripped by [`assert_and_strip_participant_clock`] — v4's must
//! be at-or-after the frozen instant and v5's must have moved off the fixture
//! seed, i.e. BOTH sides really wrote — and ONLY for that case name. Every
//! other case keeps the column fully diffed, which is precisely how "the
//! refusal wrote nothing" and "`removeImpersonation` does not restamp the seat"
//! are proven.
//!
//! ## Recorded, direction-asserted differences
//!
//! None. Where v4 and v5 disagreed the port was fixed.
//!
//! The `chat-delete-{main,mount,llmlogs}.db` trio is COMMITTED, so rebuilding
//! it is a lane decision and never a sweep's — its invocation lives in
//! `harness/oracle/fixtures/build-chat-delete-fixture.ts`'s own header, in
//! prose here on purpose (a regen stage that rewrote a committed artifact would
//! be a `repo_write` refusal, and rightly).
//!
//! Generate the oracle + run the diff:
//!   V5W=${V5W:-$HOME/source/quilltap-v5}
//!   N=~/.nvm/versions/node/v24.13.1/bin
//!   TMPO=/tmp/qt-cd-oracle
//!   rm -rf "$TMPO"
//!   mkdir -p "$TMPO/cases" "$TMPO/fixtures"
//!   cp "$V5W/harness/oracle/cases/chat-delete-routes.test.ts" "$TMPO/cases/"
//!   cp "$V5W/harness/oracle/fixtures/chat-delete-web.json" "$TMPO/fixtures/"
//!   cd ~/source/quilltap-server
//!   QT_FIXTURE_CD_MAIN=$V5W/crates/quilltap-web/tests/fixtures/chat-delete-main.db \
//!     QT_FIXTURE_CD_MOUNT=$V5W/crates/quilltap-web/tests/fixtures/chat-delete-mount.db \
//!     QT_FIXTURE_CD_LLMLOGS=$V5W/crates/quilltap-web/tests/fixtures/chat-delete-llmlogs.db \
//!     QT_ORACLE_OUT=/tmp/oracle-chat-delete.ndjson TZ=UTC \
//!     $N/npx jest --silent --watchman=false --testTimeout=180000 \
//!     --roots "$PWD" --roots "$TMPO/cases" -- "chat-delete-routes\.test\.ts$"
//!   cd $V5W
//!   QT_ORACLE_CHAT_DELETE=/tmp/oracle-chat-delete.ndjson \
//!     cargo test -p quilltap-harness --test chat_delete_equivalence -- --nocapture
//!
//! (While v4's checkout sits off the baseline, every regen above must run from
//! a worktree pinned at it — the drift ledger's §5.1 recipe; the sweep driver's
//! `--v4 <pin>` does that rewrite for you.)

use std::collections::{BTreeSet, HashMap};
use std::path::PathBuf;

use quilltap_core::api::chat_delete::{
    chat_delete_dispatch, classify_delete_action, unknown_delete_action_message, DeleteAction,
    CHAT_DELETE_ACTIONS,
};
use quilltap_core::api::types::{ErrorKind, Response};
use quilltap_core::db::runtime::{Db, DbPaths};
use serde::Deserialize;
use serde_json::{json, Value};

const CHAT_FULL: &str = "c2000000-0000-4000-8000-000000000001";
const CHAT_SHARED: &str = "c2000000-0000-4000-8000-000000000002";
const CHAT_BROKEN: &str = "c2000000-0000-4000-8000-000000000003";
const CHAT_STATE: &str = "c2000000-0000-4000-8000-000000000004";
const CHAT_IMP: &str = "c2000000-0000-4000-8000-000000000005";
const MISSING_ID: &str = "99999999-9999-4999-8999-999999999999";

const P_IMP_CLIO: &str = "e2000000-0000-4000-8000-000000000042";
const P_UNKNOWN: &str = "e2000000-0000-4000-8000-0000000000de";
const CONN_PROFILE: &str = "93000000-0000-4000-8000-000000000001";

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Spec {
    test_pepper_base64: String,
    // Plain maps: the census output's key order is normalized away by
    // [`sorted`] before the compare, so only the (table, SQL) pairs matter.
    census_main: HashMap<String, String>,
    census_mount: HashMap<String, String>,
    census_llm_logs: HashMap<String, String>,
    /// Character ids whose `Conversation Summaries/` folder is listed per case.
    vault_characters: Vec<String>,
}

fn spec_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../harness/oracle/fixtures/chat-delete-web.json")
}
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

fn fresh_db(spec: &Spec, tag: &str) -> Db {
    let scratch = std::env::temp_dir().join(format!("qt-cd-{}-{}", tag, std::process::id()));
    let _ = std::fs::remove_dir_all(&scratch);
    std::fs::create_dir_all(&scratch).unwrap();
    let main = scratch.join("main.db");
    let mount = scratch.join("mount.db");
    let llm = scratch.join("llmlogs.db");
    std::fs::copy(fixtures_dir().join("chat-delete-main.db"), &main).unwrap();
    std::fs::copy(fixtures_dir().join("chat-delete-mount.db"), &mount).unwrap();
    std::fs::copy(fixtures_dir().join("chat-delete-llmlogs.db"), &llm).unwrap();
    Db::open(
        DbPaths {
            main,
            mount_index: Some(mount),
            llm_logs: Some(llm),
        },
        &spec.test_pepper_base64,
    )
    .expect("open db")
}

// ---------------------------------------------------------------------------
// The census — the SAME SQL both sides run
// ---------------------------------------------------------------------------

/// Run one `SELECT` over a connection and shape each row as a JSON object, the
/// way `better-sqlite3`'s `.all()` does: column name → value, in the SELECT's
/// own column order.
fn select_rows(conn: &rusqlite::Connection, sql: &str) -> Value {
    let mut stmt = conn.prepare(sql).unwrap_or_else(|e| panic!("{sql}: {e}"));
    let names: Vec<String> = stmt.column_names().iter().map(|s| s.to_string()).collect();
    let mut rows = Vec::new();
    let mut q = stmt.query([]).unwrap();
    while let Some(row) = q.next().unwrap() {
        let mut obj = serde_json::Map::new();
        for (i, name) in names.iter().enumerate() {
            let v = match row.get_ref(i).unwrap() {
                rusqlite::types::ValueRef::Null => Value::Null,
                rusqlite::types::ValueRef::Integer(n) => json!(n),
                rusqlite::types::ValueRef::Real(f) => json!(f),
                rusqlite::types::ValueRef::Text(t) => {
                    Value::String(String::from_utf8_lossy(t).into_owned())
                }
                // No census column is a BLOB today; render one as its length so
                // a schema change surfaces here rather than panicking.
                rusqlite::types::ValueRef::Blob(b) => json!({ "blobLen": b.len() }),
            };
            obj.insert(name.clone(), v);
        }
        rows.push(Value::Object(obj));
    }
    Value::Array(rows)
}

/// The vault half of the census, read through the STORE READER on both sides
/// (`list_database_files` here, `listDatabaseFiles` in the oracle) rather than
/// through the raw tables — a reader that stopped resolving a folder would
/// leave every row in place, so this is the arm the table dump cannot make.
///
/// The mount POINTER comes off the raw `characters` row, never through the
/// hydrated read: v4's own `getCharacterVaultStore` and v5's
/// `resolve_vault_mount_id` both read it raw, and the hydrated read THROWS for
/// the broken-vault character — the one this section most needs to look at.
fn vault_census(db: &Db, spec: &Spec) -> Value {
    let mut out = serde_json::Map::new();
    for character_id in &spec.vault_characters {
        let id = character_id.clone();
        let mount_point_id: Option<String> = db
            .read_main(move |conn| {
                conn.query_row(
                    "SELECT \"characterDocumentMountPointId\" FROM \"characters\" WHERE \"id\" = ?1",
                    [&id],
                    |r| r.get::<_, Option<String>>(0),
                )
                .or_else(|e| match e {
                    rusqlite::Error::QueryReturnedNoRows => Ok(None),
                    other => Err(other),
                })
                .map_err(quilltap_core::db::DbError::from)
            })
            .expect("vault pointer read");
        let Some(mount_point_id) = mount_point_id.filter(|s| !s.is_empty()) else {
            out.insert(character_id.clone(), Value::Null);
            continue;
        };
        let listed = db.read_mount_index(move |conn| {
            quilltap_core::db::database_store::list_database_files(
                conn,
                &mount_point_id,
                Some("Conversation Summaries"),
            )
        });
        let value = match listed {
            Ok(entries) => {
                let mut paths: Vec<String> = entries
                    .into_iter()
                    .filter(|e| e.kind != "folder")
                    .map(|e| e.relative_path)
                    .collect();
                paths.sort();
                Value::Array(paths.into_iter().map(Value::String).collect())
            }
            Err(_) => Value::String("unreadable".to_string()),
        };
        out.insert(character_id.clone(), value);
    }
    Value::Object(out)
}

fn census(db: &Db, spec: &Spec) -> Value {
    let main = db
        .read_main(|conn| {
            let mut out = serde_json::Map::new();
            for (table, sql) in &spec.census_main {
                out.insert(table.clone(), select_rows(conn, sql));
            }
            Ok(Value::Object(out))
        })
        .expect("main census");
    let mount = db
        .read_mount_index(|conn| {
            let mut out = serde_json::Map::new();
            for (table, sql) in &spec.census_mount {
                out.insert(table.clone(), select_rows(conn, sql));
            }
            Ok(Value::Object(out))
        })
        .expect("mount census");
    let llm_logs = db
        .read_llm_logs(|conn| {
            let mut out = serde_json::Map::new();
            for (table, sql) in &spec.census_llm_logs {
                out.insert(table.clone(), select_rows(conn, sql));
            }
            Ok(Value::Object(out))
        })
        .expect("llm-logs census");
    json!({
        "main": main,
        "mount": mount,
        "llmLogs": llm_logs,
        "vaults": vault_census(db, spec),
    })
}

// ---------------------------------------------------------------------------
// Normalization (key order only — no value is normalized; see the header)
// ---------------------------------------------------------------------------

fn canon_numbers(v: &mut Value) {
    match v {
        Value::Number(n) => {
            if let Some(f) = n.as_f64() {
                if f.is_finite() && f.fract() == 0.0 && f.abs() < 9.007_199_254_740_992e15 {
                    *v = Value::Number((f as i64).into());
                }
            }
        }
        Value::Array(a) => a.iter_mut().for_each(canon_numbers),
        Value::Object(o) => o.iter_mut().for_each(|(_, x)| canon_numbers(x)),
        _ => {}
    }
}
fn sorted(v: &Value) -> Value {
    match v {
        Value::Array(a) => Value::Array(a.iter().map(sorted).collect()),
        Value::Object(o) => {
            let mut keys: Vec<&String> = o.keys().collect();
            keys.sort();
            let mut m = serde_json::Map::new();
            for k in keys {
                m.insert(k.clone(), sorted(&o[k]));
            }
            Value::Object(m)
        }
        _ => v.clone(),
    }
}
fn norm(v: &Value) -> String {
    let mut v = v.clone();
    canon_numbers(&mut v);
    serde_json::to_string_pretty(&sorted(&v)).unwrap()
}
fn first_diff(got: &str, want: &str) -> String {
    let g: Vec<&str> = got.lines().collect();
    let w: Vec<&str> = want.lines().collect();
    for i in 0..g.len().max(w.len()) {
        let gi = g.get(i).copied().unwrap_or("<none>");
        let wi = w.get(i).copied().unwrap_or("<none>");
        if gi != wi {
            let mut ctx = String::new();
            for j in i.saturating_sub(3)..i {
                ctx.push_str(&format!("   = {}\n", g.get(j).copied().unwrap_or("")));
            }
            ctx.push_str(&format!("  GOT : {gi}\n  WANT: {wi}\n"));
            return ctx;
        }
    }
    "(identical line-by-line)".to_string()
}

/// The web-edge (status, body) for a `Response` — the mapping
/// `chats_routes::chat_delete` performs, so the differential measures the bytes
/// a client actually receives.
fn status_body(r: &Response) -> (u16, Value) {
    match r {
        Response::ChatAdmin(v) | Response::State(v) | Response::ChatImpersonation(v) => {
            (200, v.clone())
        }
        Response::Error(e) => {
            let status = match e.kind {
                ErrorKind::BadRequest => 400,
                ErrorKind::Unauthorized => 401,
                ErrorKind::Forbidden => 403,
                ErrorKind::NotFound => 404,
                ErrorKind::Conflict => 409,
                ErrorKind::Unprocessable => 422,
                ErrorKind::Locked | ErrorKind::Unavailable => 503,
                ErrorKind::Internal => 500,
            };
            // v4's `validationError` body carries `details` beside `error`.
            let body = e
                .validation_wire_body()
                .unwrap_or_else(|| json!({ "error": e.message }));
            (status, body)
        }
        other => (500, serde_json::to_value(other).unwrap()),
    }
}

/// The fixture's seed timestamp — what an untouched `updatedAt` still reads.
const SEED_ISO: &str = "2026-05-01T00:00:00.000Z";
/// The instant the oracle's TICKING clock starts from (`chat-delete-web.json`).
const FROZEN_NOW_ISO: &str = "2026-05-05T00:00:00.000Z";
/// The ONE case that mints a timestamp (see the header).
const PARTICIPANT_CLOCK_CASES: &[&str] = &["stop_impersonate_with_profile"];
/// Whose seat that case reassigns.
const CLOCK_PARTICIPANT: &str = P_IMP_CLIO;

/// Assert-then-strip the wall-clock `updatedAt` `updateParticipant` stamps on
/// ONE seat inside the `chats.participants` JSON column: v4's must be
/// at-or-after the frozen instant and v5's must differ from the fixture seed —
/// i.e. BOTH sides really wrote — before the key is blanked. Scoped to the one
/// participant id, so every OTHER seat's timestamp stays fully diffed.
fn assert_and_strip_participant_clock(name: &str, tables: &mut Value, v4: bool) -> Vec<String> {
    let mut failed = Vec::new();
    let label = if v4 { "v4" } else { "v5" };
    let Some(rows) = tables
        .get_mut("main")
        .and_then(|m| m.get_mut("chats"))
        .and_then(Value::as_array_mut)
    else {
        failed.push(format!("{name}_{label}_clock_shape"));
        return failed;
    };
    let mut seen = false;
    for row in rows.iter_mut() {
        let Some(raw) = row.get("participants").and_then(Value::as_str) else {
            continue;
        };
        let Ok(mut parsed) = serde_json::from_str::<Value>(raw) else {
            continue;
        };
        let mut touched = false;
        for p in parsed.as_array_mut().into_iter().flatten() {
            if p.get("id").and_then(Value::as_str) != Some(CLOCK_PARTICIPANT) {
                continue;
            }
            seen = true;
            let stamped = p
                .get("updatedAt")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string();
            let ok = if v4 {
                stamped.as_str() >= FROZEN_NOW_ISO
            } else {
                stamped != SEED_ISO && !stamped.is_empty()
            };
            if !ok {
                eprintln!(
                    "[{name}] {label} seat updatedAt = {stamped:?} — the reassignment did not \
                     stamp it (expected {})",
                    if v4 {
                        FROZEN_NOW_ISO
                    } else {
                        "anything but the seed"
                    }
                );
                failed.push(format!("{name}_{label}_seat_updatedAt"));
            }
            p.as_object_mut()
                .unwrap()
                .insert("updatedAt".into(), Value::String("<updatedAt>".into()));
            touched = true;
        }
        if touched {
            row.as_object_mut()
                .unwrap()
                .insert("participants".into(), Value::String(parsed.to_string()));
        }
    }
    if !seen {
        eprintln!("[{name}] {label}: the clocked seat is not in the census at all");
        failed.push(format!("{name}_{label}_clock_seat_missing"));
    }
    failed
}

struct Case {
    name: &'static str,
    action: Option<&'static str>,
    chat_id: &'static str,
    /// `None` = an unreadable (empty / non-JSON) body — the oracle's mock rejects
    /// `req.json()` with a `SyntaxError` for these rows.
    body: Option<Value>,
}

#[test]
fn chat_delete_matches_oracle() {
    let Some(oracle_path) = env_or_skip("QT_ORACLE_CHAT_DELETE") else {
        return;
    };
    let spec: Spec = serde_json::from_str(&std::fs::read_to_string(spec_path()).unwrap()).unwrap();

    let mut oracle: HashMap<String, Value> = HashMap::new();
    for line in std::fs::read_to_string(&oracle_path)
        .unwrap()
        .lines()
        .filter(|l| !l.trim().is_empty())
    {
        let v: Value = serde_json::from_str(line).unwrap();
        oracle.insert(v["name"].as_str().unwrap().to_string(), v);
    }
    assert!(
        !oracle.is_empty(),
        "the oracle NDJSON is empty — regenerate it (an erroring builder leaves a stale file)"
    );

    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let mut failed: Vec<String> = Vec::new();
    let mut driven: BTreeSet<String> = BTreeSet::new();

    let cases = vec![
        Case {
            name: "delete_full",
            action: None,
            chat_id: CHAT_FULL,
            body: Some(json!({})),
        },
        Case {
            name: "delete_missing",
            action: None,
            chat_id: MISSING_ID,
            body: Some(json!({})),
        },
        Case {
            name: "delete_broken_vault",
            action: None,
            chat_id: CHAT_BROKEN,
            body: Some(json!({})),
        },
        Case {
            name: "delete_shared_summary",
            action: None,
            chat_id: CHAT_SHARED,
            body: Some(json!({})),
        },
        Case {
            name: "reset_state",
            action: Some("reset-state"),
            chat_id: CHAT_STATE,
            body: Some(json!({})),
        },
        Case {
            name: "reset_state_missing",
            action: Some("reset-state"),
            chat_id: MISSING_ID,
            body: Some(json!({})),
        },
        Case {
            name: "stop_impersonate",
            action: Some("stop-impersonate"),
            chat_id: CHAT_IMP,
            body: Some(json!({ "participantId": P_IMP_CLIO })),
        },
        Case {
            name: "stop_impersonate_with_profile",
            action: Some("stop-impersonate"),
            chat_id: CHAT_IMP,
            body: Some(
                json!({ "participantId": P_IMP_CLIO, "newConnectionProfileId": CONN_PROFILE }),
            ),
        },
        Case {
            name: "stop_impersonate_unknown_profile",
            action: Some("stop-impersonate"),
            chat_id: CHAT_IMP,
            body: Some(
                json!({ "participantId": P_IMP_CLIO, "newConnectionProfileId": MISSING_ID }),
            ),
        },
        Case {
            name: "stop_impersonate_unknown_participant",
            action: Some("stop-impersonate"),
            chat_id: CHAT_IMP,
            body: Some(json!({ "participantId": P_UNKNOWN })),
        },
        Case {
            name: "stop_impersonate_missing_chat",
            action: Some("stop-impersonate"),
            chat_id: MISSING_ID,
            body: Some(json!({ "participantId": P_IMP_CLIO })),
        },
        Case {
            name: "stop_impersonate_missing_chat_bad_body",
            action: Some("stop-impersonate"),
            chat_id: MISSING_ID,
            body: Some(json!({})),
        },
        Case {
            name: "stop_impersonate_no_participant",
            action: Some("stop-impersonate"),
            chat_id: CHAT_IMP,
            body: Some(json!({})),
        },
        Case {
            name: "stop_impersonate_bad_uuid",
            action: Some("stop-impersonate"),
            chat_id: CHAT_IMP,
            body: Some(json!({ "participantId": "not-a-uuid" })),
        },
        Case {
            name: "stop_impersonate_both_fields_bad",
            action: Some("stop-impersonate"),
            chat_id: CHAT_IMP,
            body: Some(json!({ "participantId": 7, "newConnectionProfileId": null })),
        },
        // Zod 4.5.4's `z.object` refuses a NON-object body before any field
        // walk, with ONE root-path issue (the §3 unification review's catch —
        // the per-field walk used to invent `participantId … received
        // undefined`).
        Case {
            name: "stop_impersonate_null_body",
            action: Some("stop-impersonate"),
            chat_id: CHAT_IMP,
            body: Some(Value::Null),
        },
        Case {
            name: "stop_impersonate_array_body",
            action: Some("stop-impersonate"),
            chat_id: CHAT_IMP,
            body: Some(json!([])),
        },
        // An UNREADABLE body (empty / not JSON): v4's `await req.json()` throws a
        // SyntaxError → the middleware's 500 `Internal server error` — but only
        // on the leg that reads the body, and only AFTER its chat gate.
        Case {
            name: "stop_impersonate_empty_body",
            action: Some("stop-impersonate"),
            chat_id: CHAT_IMP,
            body: None,
        },
        Case {
            name: "stop_impersonate_missing_chat_empty_body",
            action: Some("stop-impersonate"),
            chat_id: MISSING_ID,
            body: None,
        },
        Case {
            name: "action_bogus",
            action: Some("zzz"),
            chat_id: CHAT_FULL,
            body: Some(json!({})),
        },
        // `?action=` present but EMPTY — JS-falsy, so it DELETES.
        Case {
            name: "action_empty",
            action: Some(""),
            chat_id: CHAT_FULL,
            body: Some(json!({})),
        },
    ];

    for c in &cases {
        driven.insert(c.name.to_string());
        let db = fresh_db(&spec, c.name);
        let r = rt.block_on(chat_delete_dispatch(
            &db,
            c.chat_id,
            c.action,
            c.body.as_ref(),
        ));
        let (status, body) = status_body(&r);
        let mut tables = census(&db, &spec);

        let Some(want) = oracle.get(c.name) else {
            failed.push(format!("{}_MISSING_FROM_ORACLE", c.name));
            continue;
        };
        let want_status = want["status"].as_u64().unwrap() as u16;
        if status != want_status {
            eprintln!("[{}] STATUS {status} != {want_status}", c.name);
            failed.push(format!("{}_status", c.name));
        }
        if norm(&body) != norm(&want["body"]) {
            eprintln!(
                "[{}] BODY MISMATCH:\n{}",
                c.name,
                first_diff(&norm(&body), &norm(&want["body"]))
            );
            failed.push(c.name.to_string());
        } else {
            eprintln!("[{}] body OK.", c.name);
        }
        let mut want_tables = want["tables"].clone();
        if PARTICIPANT_CLOCK_CASES.contains(&c.name) {
            failed.extend(assert_and_strip_participant_clock(
                c.name,
                &mut want_tables,
                true,
            ));
            failed.extend(assert_and_strip_participant_clock(
                c.name,
                &mut tables,
                false,
            ));
        }
        if norm(&tables) != norm(&want_tables) {
            eprintln!(
                "[{} tables] MISMATCH:\n{}",
                c.name,
                first_diff(&norm(&tables), &norm(&want_tables))
            );
            failed.push(format!("{}_tables", c.name));
        } else {
            eprintln!("[{} tables] OK.", c.name);
        }
    }

    // Coverage by SHAPE: the oracle's names and the driven names must agree.
    let oracle_names: BTreeSet<String> = oracle.keys().cloned().collect();
    let missing: Vec<&String> = oracle_names.difference(&driven).collect();
    assert!(
        missing.is_empty(),
        "oracle cases never driven by the Rust side: {missing:?}"
    );

    assert!(
        failed.is_empty(),
        "{} case(s) failed: {failed:?}",
        failed.len()
    );
}

/// v4 `handleDelete`'s action classification in isolation — the arms the
/// four-case corpus above exercises only one at a time, plus the two the corpus
/// cannot express (a repeated key is the transport's business, and `None` and
/// `Some("")` reach the same leg by DIFFERENT routes).
///
/// Needs no oracle: v4's `delete.ts:25-45` is four `if`s and the table below is
/// their transcription. The bytes of the refusal sentence are pinned against v4
/// by the `action_bogus` corpus row.
#[test]
fn delete_action_classification() {
    assert_eq!(classify_delete_action(None), DeleteAction::Delete);
    // JS truthiness: `?action=` is falsy, so it DELETES rather than refusing.
    assert_eq!(classify_delete_action(Some("")), DeleteAction::Delete);
    assert_eq!(
        classify_delete_action(Some("reset-state")),
        DeleteAction::ResetState
    );
    assert_eq!(
        classify_delete_action(Some("stop-impersonate")),
        DeleteAction::StopImpersonate
    );
    // Case matters — v4 compares with `===`.
    assert_eq!(
        classify_delete_action(Some("Reset-State")),
        DeleteAction::Unknown("Reset-State".into())
    );
    assert_eq!(
        classify_delete_action(Some("delete")),
        DeleteAction::Unknown("delete".into())
    );
    // The sentence's tail is JOINED from the one list, never transcribed twice.
    assert_eq!(CHAT_DELETE_ACTIONS, &["reset-state", "stop-impersonate"]);
    assert_eq!(
        unknown_delete_action_message("zzz"),
        "Unknown DELETE action: zzz. Available DELETE actions: reset-state, stop-impersonate"
    );
}

/// **P4.80 Tier 2, item 7 — the log lines.** A `logger.warn`/`logger.info` is
/// invisible to the differential above: the bodies and every censused row are
/// identical whether or not a line fires (`differential-blind-to-a-log-only-fix`).
/// These are the lines an operator greps `combined.log` for after a chat
/// vanishes, so each is pinned against v4's sentence, with the SILENCE half
/// asserted too — without it a line moved to the wrong branch still passes.
///
/// v4's five lines on this path, and where each lives:
///   `[Chats v1] Unknown DELETE action, rejecting to prevent data loss` (warn,
///       `delete.ts:42`)          → `api::chat_delete::chat_delete_dispatch`
///   `[Chats v1] Chat deleted`    (info, `delete.ts:56`)  → `api::chat_delete`
///   `Chat deleted`               (info, `chats.repository.ts:371`)
///   `Removed conversation summary from character vault` (debug,
///       `conversation-summary-vault-bridge.ts:280`)
///   `Failed to delete conversation annotations for chat` (warn,
///       `chats.repository.ts:349`) — the error arm, unreachable on a healthy
///       fixture and left to `db::chats`'s own module tests.
#[test]
fn chat_delete_log_lines() {
    let Ok(raw) = std::fs::read_to_string(spec_path()) else {
        eprintln!("SKIP: the chat-delete spec is missing.");
        return;
    };
    let spec: Spec = serde_json::from_str(&raw).unwrap();
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let has = |lines: &[String], sentence: &str| lines.iter().any(|l| l.contains(sentence));
    let line_with = |lines: &Vec<String>, sentence: &str| -> String {
        lines
            .iter()
            .find(|l| l.contains(sentence))
            .unwrap_or_else(|| panic!("no line carried {sentence:?}; got {lines:#?}"))
            .clone()
    };

    // --- the happy delete: both `Chat deleted` lines + the per-vault debug ---
    let db = fresh_db(&spec, "log_delete");
    let lines = quilltap_core::test_support::captured(|| {
        rt.block_on(chat_delete_dispatch(&db, CHAT_FULL, None, Some(&json!({}))));
    });
    let route_line = line_with(&lines, "[Chats v1] Chat deleted");
    assert!(
        route_line.starts_with("INFO ") && route_line.contains(&format!("chat_id={CHAT_FULL}")),
        "v4 logs the route line at info with the chat id: {route_line}"
    );
    // The repository's own `logger.info('Chat deleted', { chatId })` — a SECOND
    // line, which v4 emits from underneath the route's.
    assert_eq!(
        lines.iter().filter(|l| l.contains("Chat deleted")).count(),
        2,
        "v4 writes BOTH the route's `[Chats v1] Chat deleted` and the \
         repository's bare `Chat deleted`; got {lines:#?}"
    );
    // Two participant vaults each lose one summary file.
    assert_eq!(
        lines
            .iter()
            .filter(|l| l.contains("Removed conversation summary from character vault"))
            .count(),
        2,
        "one debug line per vault the sweep actually emptied; got {lines:#?}"
    );
    assert!(
        !has(&lines, "Unknown DELETE action"),
        "a no-action delete must not warn about an action: {lines:#?}"
    );

    // --- the unknown-action refusal: the warn, and NOTHING about a delete ---
    let db = fresh_db(&spec, "log_bogus");
    let lines = quilltap_core::test_support::captured(|| {
        rt.block_on(chat_delete_dispatch(
            &db,
            CHAT_FULL,
            Some("zzz"),
            Some(&json!({})),
        ));
    });
    let warn = line_with(
        &lines,
        "[Chats v1] Unknown DELETE action, rejecting to prevent data loss",
    );
    assert!(warn.starts_with("WARN "), "v4 logs this at warn: {warn}");
    for field in [format!("chat_id={CHAT_FULL}"), "action=zzz".to_string()] {
        assert!(warn.contains(&field), "missing {field} in {warn}");
    }
    assert!(
        !has(&lines, "Chat deleted"),
        "the refusal must not announce a delete: {lines:#?}"
    );

    // --- the SILENCE half: a 404 delete announces nothing at all ---
    let db = fresh_db(&spec, "log_missing");
    let lines = quilltap_core::test_support::captured(|| {
        rt.block_on(chat_delete_dispatch(
            &db,
            MISSING_ID,
            None,
            Some(&json!({})),
        ));
    });
    assert!(
        !has(&lines, "Chat deleted") && !has(&lines, "Unknown DELETE action"),
        "a missing chat writes neither line; got {lines:#?}"
    );

    // --- a chat whose sole participant has a DANGLING vault pointer: the
    //     sweep cannot resolve her store, SAYS SO (v4's resolver catch —
    //     `character-vault-bridge.ts:74-81` warns `Failed to resolve character
    //     vault store` when the hydrated read throws for a dangling pointer),
    //     and the delete still announces itself. The §3 unification review
    //     found this arm SILENT in v5; the warn is ported and pinned here.
    let db = fresh_db(&spec, "log_broken");
    let lines = quilltap_core::test_support::captured(|| {
        rt.block_on(chat_delete_dispatch(
            &db,
            CHAT_BROKEN,
            None,
            Some(&json!({})),
        ));
    });
    assert!(
        has(&lines, "[Chats v1] Chat deleted"),
        "the delete succeeds despite the broken vault: {lines:#?}"
    );
    assert!(
        !has(&lines, "Removed conversation summary from character vault"),
        "nothing was removed from a vault that cannot be read: {lines:#?}"
    );
    assert!(
        has(&lines, "Failed to resolve character vault store"),
        "v4 warns when a participant's vault cannot be resolved: {lines:#?}"
    );
}
