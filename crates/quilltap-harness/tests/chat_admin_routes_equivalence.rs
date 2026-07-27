//! P4.9E3A CHAT-ADMIN + TOOLS route-surface differential:
//! `services::chat_admin::*` (and its siblings) vs v4's REAL chat action
//! handlers. Both sides read a FRESH copy of the committed
//! `chat-admin-{main,mount}.db` fixture per case (baked ids identical → no
//! remap), then diff the response body AND the post-mutation table dumps.
//!
//! Coverage is asserted by SHAPE, not by a hand-written count: every case name
//! the oracle emitted must have been driven here, and vice versa
//! (`harness-corpus-shape-constants-rot`).
//!
//! ## Recorded, direction-asserted differences
//!
//! 1. **[`V4_CREATED_CASES`]** — v4 answers **201** on `add-tag`. v5's dispatch
//!    boundary carries no per-verb success status (the standing `ChatCreate`
//!    precedent: every success is 200 at the HTTP edge), so those cases assert
//!    v4 == 201 ∧ v5 == 200 rather than equality. The BODY is compared
//!    byte-for-byte.
//!
//! ## Minted values
//!
//! Background-job rows mint a UUID + three timestamps on both sides, so the job
//! dump carries only the stable columns (type/status/payload/priority/attempts)
//! and the body's `jobId` is blanked. The DEDUPE cases prove job identity
//! in-band instead: each side computes `sameJobId` from its OWN two calls, and
//! that boolean is compared.
//!
//! Generate the oracle (Node 24, from the v4 checkout — see the .ts header):
//!   … QT_ORACLE_OUT=/tmp/oracle-chat-admin.ndjson npx jest -- chat-admin-routes
//! Run:
//!   QT_ORACLE_CHAT_ADMIN=/tmp/oracle-chat-admin.ndjson \
//!     cargo test -p quilltap-harness --test chat_admin_routes_equivalence -- --nocapture

use std::collections::{BTreeSet, HashMap};
use std::path::PathBuf;

use quilltap_core::api::types::{ErrorKind, Response};
use quilltap_core::db::runtime::{Db, DbPaths};
use quilltap_core::services::chat_run_tool::ErasedToolRunner;
use quilltap_core::services::{chat_admin, chat_merge, chat_rng, chat_run_tool};
use quilltap_core::tools::executor::BuiltInToolRunner;
use quilltap_core::tools::rng::FixedBytes;
use quilltap_core::tools::self_inventory::{ClientShell, SelfInventoryEnv};
use serde::Deserialize;
use serde_json::{json, Value};

const TAG_A: &str = "b1000000-0000-4000-8000-000000000001";
const TAG_B: &str = "b1000000-0000-4000-8000-000000000002";
const CHAT: &str = "c1000000-0000-4000-8000-000000000001";
const EMPTY_CHAT: &str = "c1000000-0000-4000-8000-000000000003";
const MISSING_ID: &str = "99999999-9999-4999-8999-999999999999";
const P_EVE: &str = "e1000000-0000-4000-8000-000000000001";
const P_ARIA: &str = "e1000000-0000-4000-8000-000000000002";
const P_CLIO: &str = "e1000000-0000-4000-8000-000000000003";
/// A participant of the SOURCE chat — "not found in THIS chat" with a
/// well-formed uuid.
const P_STRANGER: &str = "e1000000-0000-4000-8000-000000000011";
const CLIO: &str = "a1000000-0000-4000-8000-000000000003";
const BEA: &str = "a1000000-0000-4000-8000-000000000002";
const DORIAN: &str = "a1000000-0000-4000-8000-000000000004";
const SOURCE_CHAT: &str = "c1000000-0000-4000-8000-000000000002";
/// The instant the oracle's TICKING clock starts from (`chat-admin-web.json`).
/// Each `new Date()` there advances 1 ms, so a v4 stamp is at-or-after this.
const FROZEN_NOW_ISO: &str = "2026-05-05T00:00:00.000Z";
/// The fixture's seed timestamp — what an untouched `updatedAt` still reads.
const SEED_ISO: &str = "2026-05-01T00:00:00.000Z";

/// Cases where v4 answers 201 and the dispatch boundary answers 200.
const V4_CREATED_CASES: &[&str] = &["add_tag_new", "add_tag_already_present"];

/// Cases whose 400 body carries v4's Zod `details` array, which v5's error
/// envelope does not model (the standing, named P4.6bb deferral). Asserted in
/// both directions, then dropped before the body compare.
const VALIDATION_DETAILS_GAP: &[&str] = &[
    "bulk_reattribute_bad_role_filter",
    "rng_bad_kind",
    "rng_bad_rolls",
    "run_tool_empty_name",
];

/// Cases that persist a freshly MINTED message: both sides mint a UUID and a
/// `createdAt`, so `id` / `createdAt` are blanked inside `body.message` and
/// throughout the message dump. Everything that matters — the content JSON's
/// bytes, the role, the ordering, and the fact that PREVIEW writes nothing at
/// all — still diffs.
const MINTED_MESSAGE_CASES: &[&str] = &[
    "rng_d20_single",
    "rng_d6_three",
    "rng_flip_coin",
    "rng_flip_coin_multi",
    "rng_spin_the_bottle",
    "rng_preview_d20",
    "rng_preview_coin_multi",
    "run_tool_unknown_tool",
    "run_tool_read_conversation",
    "run_tool_private",
    "run_tool_named_character",
    "run_tool_unmatched_character",
    "run_tool_default_character",
    // The merge mints participant ids, Host bubble ids and their `createdAt`s
    // on both sides.
    "merge_all",
    "merge_allowlist",
    "merge_outfit_selections",
    "merge_all_already_present",
];

/// Cases whose chat row carries a timestamp MINTED from the wall clock —
/// `addMessage` stamps `updatedAt` + `lastMessageAt` with `now` on every
/// message-typed event, and v5's clock is the real one while v4's is frozen.
/// The two keys are asserted (v4 == the frozen instant, v5 != the fixture seed —
/// i.e. BOTH sides did write) and only then dropped from the compare, so the
/// difference is proven rather than normalized away.
const CLOCK_MINTED_CASES: &[&str] = &[
    "bulk_reattribute_both",
    "bulk_reattribute_assistant_only",
    "bulk_reattribute_user_only",
    "bulk_reattribute_null_source",
    // The merge's Host bubbles are messages, so the target chat's `updatedAt` /
    // `lastMessageAt` are stamped from the wall clock too.
    "merge_all",
    "merge_allowlist",
    "merge_outfit_selections",
];

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Spec {
    test_pepper_base64: String,
    user_id: String,
    frozen_now_ms: i64,
    rng_byte_stream: Vec<u8>,
}

/// A fixed `SelfInventoryEnv`: no tool driven by the run-tool cases reads it,
/// so its values only need to be stable.
fn fixture_env() -> SelfInventoryEnv {
    SelfInventoryEnv {
        version: "0.0.0-fixture".to_string(),
        runtime_mode: "desktop".to_string(),
        client_shell: ClientShell::Unknown,
        mount_index_degraded: false,
        release_notes: None,
        changelog: None,
        model_info: Vec::new(),
        fallback_pricing: Vec::new(),
        registry_default_context: 8192,
    }
}

fn spec_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../harness/oracle/fixtures/chat-admin-web.json")
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
    let scratch = std::env::temp_dir().join(format!("qt-ca-{}-{}", tag, std::process::id()));
    let _ = std::fs::remove_dir_all(&scratch);
    std::fs::create_dir_all(&scratch).unwrap();
    let main = scratch.join("main.db");
    let mount = scratch.join("mount.db");
    std::fs::copy(fixtures_dir().join("chat-admin-main.db"), &main).unwrap();
    std::fs::copy(fixtures_dir().join("chat-admin-mount.db"), &mount).unwrap();
    Db::open(
        DbPaths {
            main,
            mount_index: Some(mount),
            llm_logs: None,
        },
        &spec.test_pepper_base64,
    )
    .expect("open db")
}

// ---------------------------------------------------------------------------
// Normalization (the post-office mold)
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
/// Blank the freshly minted job id (both sides mint a UUID). Scoped to the
/// `jobId` key — chat / participant / message ids are NOT blanked, so the
/// bulk-reattribution and merge dumps stay fully diffed.
fn blank_job_id(v: &mut Value) {
    match v {
        Value::Object(o) => {
            if o.contains_key("jobId") {
                o.insert("jobId".to_string(), Value::String("<jobId>".into()));
            }
            o.iter_mut().for_each(|(_, x)| blank_job_id(x));
        }
        Value::Array(a) => a.iter_mut().for_each(blank_job_id),
        _ => {}
    }
}
/// Blank the minted message keys (`id` / `createdAt`) everywhere — used only
/// for [`MINTED_MESSAGE_CASES`].
fn blank_minted(v: &mut Value) {
    match v {
        Value::Object(o) => {
            // `updatedAt` joins the minted set here because `addParticipant`
            // stamps a fresh one on every participant it inserts.
            for k in ["id", "createdAt", "updatedAt"] {
                if o.contains_key(k) {
                    o.insert(k.to_string(), Value::String(format!("<{k}>")));
                }
            }
            // `compiledIdentityStacks` is KEYED by the minted participant id, so
            // the keys cannot be compared. The VALUES are the whole point (each
            // names its character), so the map becomes a sorted array of them —
            // a stack that failed to compile, or compiled for the wrong
            // character, still shows.
            // A Host bubble's `hostEvent.participantId` is the minted id of the
            // participant that just joined. Blanked HERE (rather than by key
            // name anywhere) so ordinary messages' `participantId` — which the
            // bulk-reattribution cases live on — stays fully diffed.
            if let Some(he) = o.get_mut("hostEvent").and_then(Value::as_object_mut) {
                if he.contains_key("participantId") {
                    he.insert(
                        "participantId".to_string(),
                        Value::String("<participantId>".into()),
                    );
                }
            }
            if let Some(stacks) = o.get("compiledIdentityStacks").and_then(Value::as_object) {
                let mut vals: Vec<Value> = stacks.values().cloned().collect();
                vals.sort_by_key(|v| v.as_str().unwrap_or("").to_string());
                o.insert("compiledIdentityStacks".to_string(), Value::Array(vals));
            }
            o.iter_mut().for_each(|(_, x)| blank_minted(x));
        }
        Value::Array(a) => a.iter_mut().for_each(blank_minted),
        _ => {}
    }
}
fn norm(v: &Value) -> String {
    let mut v = v.clone();
    canon_numbers(&mut v);
    blank_job_id(&mut v);
    serde_json::to_string_pretty(&sorted(&v)).unwrap()
}
/// [`norm`] plus the minted-message blanking.
fn norm_minted(v: &Value) -> String {
    let mut v = v.clone();
    blank_minted(&mut v);
    norm(&v)
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

/// Assert-then-strip the two wall-clock-minted chat timestamps on ONE chat
/// object: v4's must equal the frozen instant and v5's must differ from the
/// fixture seed — i.e. both sides really wrote — before the keys are dropped
/// from the compare. Returns the failure labels (empty when both held).
fn assert_and_strip_clock(
    name: &str,
    chat: &mut serde_json::Map<String, Value>,
    v4: bool,
) -> Vec<String> {
    let mut failed = Vec::new();
    let label = if v4 { "v4" } else { "v5" };
    for key in ["updatedAt", "lastMessageAt"] {
        let got = chat
            .get(key)
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string();
        let ok = if v4 {
            // The oracle's clock ticks, so a v4 stamp is at-or-after the base
            // (ISO-8601 compares lexicographically).
            got.as_str() >= FROZEN_NOW_ISO
        } else {
            got != SEED_ISO && !got.is_empty()
        };
        if !ok {
            eprintln!(
                "[{name}] {label} {key} = {got:?} — the write did not stamp it (expected {})",
                if v4 {
                    FROZEN_NOW_ISO
                } else {
                    "anything but the seed"
                }
            );
            failed.push(format!("{name}_{label}_{key}"));
        }
        chat.remove(key);
    }
    failed
}

/// The web-edge (status, body) for a Response (the HTTP transport mapping).
fn status_body(r: &Response) -> (u16, Value) {
    match r {
        Response::ChatAdmin(v) => (200, v.clone()),
        Response::Error(e) => {
            let status = match e.kind {
                ErrorKind::BadRequest => 400,
                ErrorKind::Unauthorized => 401,
                ErrorKind::Forbidden => 403,
                ErrorKind::NotFound => 404,
                ErrorKind::Conflict => 409,
                ErrorKind::Unprocessable => 422,
                ErrorKind::Locked => 423,
                ErrorKind::Internal => 500,
            };
            (status, json!({ "error": e.message }))
        }
        other => (500, serde_json::to_value(other).unwrap()),
    }
}

// ---------------------------------------------------------------------------
// Table dumps (mirror the oracle's readers exactly)
// ---------------------------------------------------------------------------

fn dump_chat(db: &Db, chat_id: &str) -> Value {
    let cid = chat_id.to_string();
    db.read_main(move |c| quilltap_core::db::chats_read::find_by_id(c, &cid))
        .unwrap()
        .unwrap_or(Value::Null)
}

/// Every chat event, in order (the oracle's `readMessages`).
fn dump_messages(db: &Db, chat_id: &str) -> Value {
    let cid = chat_id.to_string();
    Value::Array(
        db.read_main(move |c| quilltap_core::db::chats_messages_read::get_messages(c, &cid))
            .unwrap(),
    )
}

/// Every memory row, id-sorted, reduced to the three columns the bulk delete
/// moves (the oracle's `readMemories`).
fn dump_memories(db: &Db) -> Value {
    let rows = db
        .read_main(quilltap_core::db::memories_read::find_all)
        .unwrap();
    let mut out: Vec<Value> = rows
        .into_iter()
        .map(|m| {
            json!({
                "id": m.get("id").cloned().unwrap_or(Value::Null),
                "characterId": m.get("characterId").cloned().unwrap_or(Value::Null),
                "sourceMessageId": m.get("sourceMessageId").cloned().unwrap_or(Value::Null),
            })
        })
        .collect();
    out.sort_by_key(|v| {
        v.get("id")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string()
    });
    Value::Array(out)
}

/// The user's PENDING + PROCESSING jobs, minus the minted id/timestamps, sorted
/// by their own JSON text (the oracle sorts identically).
fn dump_jobs(db: &Db, user_id: &str) -> Value {
    let uid = user_id.to_string();
    let rows = db
        .read_main(move |c| {
            let repo = quilltap_core::db::background_jobs::BackgroundJobsRepository::new(c);
            let mut all = repo.find_by_user_id(&uid, Some("PENDING"))?;
            all.extend(repo.find_by_user_id(&uid, Some("PROCESSING"))?);
            Ok(all)
        })
        .unwrap();
    let mut out: Vec<Value> = rows
        .into_iter()
        .map(|j| {
            json!({
                "type": j.job_type,
                "status": j.status,
                "payload": serde_json::from_str::<Value>(&j.payload).unwrap_or(Value::Null),
                "priority": j.priority,
                "attempts": j.attempts,
                "maxAttempts": j.max_attempts,
                "lastError": j.last_error,
            })
        })
        .collect();
    out.sort_by_key(|v| serde_json::to_string(v).unwrap());
    Value::Array(out)
}

#[test]
fn chat_admin_routes_match_oracle() {
    let Some(oracle_path) = env_or_skip("QT_ORACLE_CHAT_ADMIN") else {
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

    let mut check = |name: &str, resp: &Response, tables: Option<Value>| {
        driven.insert(name.to_string());
        let Some(want) = oracle.get(name) else {
            failed.push(format!("{name}_MISSING_FROM_ORACLE"));
            return;
        };
        let (status, body) = status_body(resp);
        let want_status = want["status"].as_u64().unwrap() as u16;
        let mut want_body = want["body"].clone();

        if V4_CREATED_CASES.contains(&name) {
            if want_status != 201 || status != 200 {
                eprintln!(
                    "[{name}] the recorded 201-vs-200 difference no longer holds \
                     (v4={want_status}, v5={status}) — re-rule it or drop the entry"
                );
                failed.push(format!("{name}_status"));
            }
        } else if status != want_status {
            eprintln!("[{name}] STATUS {status} != {want_status}");
            failed.push(format!("{name}_status"));
        }

        // The Zod `details` gap, asserted in both directions.
        if VALIDATION_DETAILS_GAP.contains(&name) {
            let v4_details_ok = want_body
                .get("details")
                .and_then(Value::as_array)
                .is_some_and(|a| !a.is_empty());
            let v5_has_details = body.get("details").is_some();
            if !v4_details_ok || v5_has_details {
                eprintln!(
                    "[{name}] the recorded validation-details gap no longer holds \
                     (v4 details present={v4_details_ok}, v5 details present={v5_has_details}) \
                     — re-rule it or drop the entry"
                );
                failed.push(format!("{name}_details_gap"));
            }
            if let Some(o) = want_body.as_object_mut() {
                o.remove("details");
            }
        }

        // The merge's body carries the refreshed chat, whose `updatedAt` /
        // `lastMessageAt` the Host bubbles stamped from the wall clock.
        let mut body = body;
        if CLOCK_MINTED_CASES.contains(&name) {
            for (is_v4, v) in [(true, &mut want_body), (false, &mut body)] {
                if let Some(chat) = v.get_mut("chat").and_then(Value::as_object_mut) {
                    failed.extend(assert_and_strip_clock(name, chat, is_v4));
                }
            }
        }

        let n: fn(&Value) -> String = if MINTED_MESSAGE_CASES.contains(&name) {
            norm_minted
        } else {
            norm
        };
        if n(&body) != n(&want_body) {
            eprintln!(
                "[{name}] BODY MISMATCH:\n{}",
                first_diff(&n(&body), &n(&want_body))
            );
            failed.push(name.to_string());
        } else {
            eprintln!("[{name}] body OK.");
        }

        if let Some(mut got_tables) = tables {
            let mut want_tables = want["tables"].clone();
            // Wall-clock-minted chat timestamps: prove BOTH sides wrote, then
            // drop the two keys (see CLOCK_MINTED_CASES).
            if CLOCK_MINTED_CASES.contains(&name) {
                for (is_v4, tbl) in [(true, &mut want_tables), (false, &mut got_tables)] {
                    match tbl.get_mut("chat").and_then(Value::as_object_mut) {
                        Some(chat) => failed.extend(assert_and_strip_clock(name, chat, is_v4)),
                        None => failed.push(format!("{name}_clock_shape")),
                    }
                }
            }
            if n(&got_tables) != n(&want_tables) {
                eprintln!(
                    "[{name} tables] MISMATCH:\n{}",
                    first_diff(&n(&got_tables), &n(&want_tables))
                );
                failed.push(format!("{name}_tables"));
            } else {
                eprintln!("[{name} tables] OK.");
            }
        }
    };

    // ── ?action=add-tag ─────────────────────────────────────────────────────
    for (name, chat, tag, dump) in [
        ("add_tag_new", CHAT, TAG_B, true),
        ("add_tag_already_present", CHAT, TAG_A, true),
        ("add_tag_unknown_tag", CHAT, MISSING_ID, false),
        ("add_tag_chat_missing", MISSING_ID, TAG_B, false),
    ] {
        let db = fresh_db(&spec, name);
        let r = rt.block_on(chat_admin::chat_add_tag(&db, chat, tag));
        let tables = dump.then(|| json!({ "chat": dump_chat(&db, CHAT) }));
        check(name, &r, tables);
    }

    // ── ?action=remove-tag ──────────────────────────────────────────────────
    for (name, tag) in [
        ("remove_tag_present", TAG_A),
        ("remove_tag_absent", TAG_B),
        ("remove_tag_unknown_id", MISSING_ID),
    ] {
        let db = fresh_db(&spec, name);
        let r = rt.block_on(chat_admin::chat_remove_tag(&db, CHAT, tag));
        check(name, &r, Some(json!({ "chat": dump_chat(&db, CHAT) })));
    }

    // ── ?action=update-tool-settings ────────────────────────────────────────
    for (name, tools, groups) in [
        (
            "update_tool_settings",
            vec!["generate_image".to_string(), "web_search".to_string()],
            vec!["scriptorium".to_string()],
        ),
        ("update_tool_settings_empty", vec![], vec![]),
    ] {
        let db = fresh_db(&spec, name);
        let r = rt.block_on(chat_admin::chat_update_tool_settings(
            &db, CHAT, tools, groups,
        ));
        check(name, &r, Some(json!({ "chat": dump_chat(&db, CHAT) })));
    }

    // ── ?action=toggle-agent-mode ───────────────────────────────────────────
    // All four v4 arms are driven, though the frozen §1 wire can only express
    // the first (see `services/chat_admin.rs`'s header).
    for (name, chat, enabled, dump) in [
        ("toggle_agent_mode_absent", CHAT, None, true),
        ("toggle_agent_mode_true", CHAT, Some(Some(true)), true),
        ("toggle_agent_mode_false", CHAT, Some(Some(false)), true),
        ("toggle_agent_mode_null", CHAT, Some(None), true),
        ("toggle_agent_mode_chat_missing", MISSING_ID, None, false),
    ] {
        let db = fresh_db(&spec, name);
        let r = rt.block_on(chat_admin::chat_toggle_agent_mode(
            &db,
            &spec.user_id,
            chat,
            enabled,
        ));
        let tables = dump.then(|| json!({ "chat": dump_chat(&db, CHAT) }));
        check(name, &r, tables);
    }

    // ── ?action=reclassify-danger ───────────────────────────────────────────
    {
        let db = fresh_db(&spec, "reclassify_danger");
        let r = rt.block_on(chat_admin::chat_reclassify_danger(&db, &spec.user_id, CHAT));
        check(
            "reclassify_danger",
            &r,
            Some(json!({
                "chat": dump_chat(&db, CHAT),
                "jobs": dump_jobs(&db, &spec.user_id),
            })),
        );
    }
    {
        let db = fresh_db(&spec, "reclassify_danger_dedupe");
        let first = rt.block_on(chat_admin::chat_reclassify_danger(&db, &spec.user_id, CHAT));
        let second = rt.block_on(chat_admin::chat_reclassify_danger(&db, &spec.user_id, CHAT));
        let same = job_id(&first) == job_id(&second);
        let r = with_extra(&second, "sameJobId", json!(same));
        check(
            "reclassify_danger_dedupe",
            &r,
            Some(json!({
                "chat": dump_chat(&db, CHAT),
                "jobs": dump_jobs(&db, &spec.user_id),
            })),
        );
    }
    {
        let db = fresh_db(&spec, "reclassify_danger_no_profile");
        let r = rt.block_on(chat_admin::chat_reclassify_danger(
            &db,
            &spec.user_id,
            EMPTY_CHAT,
        ));
        check(
            "reclassify_danger_no_profile",
            &r,
            Some(json!({
                "chat": dump_chat(&db, EMPTY_CHAT),
                "jobs": dump_jobs(&db, &spec.user_id),
            })),
        );
    }

    // ── ?action=render-conversation ─────────────────────────────────────────
    {
        let db = fresh_db(&spec, "render_conversation");
        let r = rt.block_on(chat_admin::chat_render_conversation(
            &db,
            &spec.user_id,
            CHAT,
        ));
        check(
            "render_conversation",
            &r,
            Some(json!({
                "chat": dump_chat(&db, CHAT),
                "jobs": dump_jobs(&db, &spec.user_id),
            })),
        );
    }
    {
        let db = fresh_db(&spec, "render_conversation_dedupe");
        let first = rt.block_on(chat_admin::chat_render_conversation(
            &db,
            &spec.user_id,
            CHAT,
        ));
        let second = rt.block_on(chat_admin::chat_render_conversation(
            &db,
            &spec.user_id,
            CHAT,
        ));
        let same = job_id(&first) == job_id(&second);
        let r = with_extra(&second, "sameJobId", json!(same));
        check(
            "render_conversation_dedupe",
            &r,
            Some(json!({
                "chat": dump_chat(&db, CHAT),
                "jobs": dump_jobs(&db, &spec.user_id),
            })),
        );
    }
    {
        let db = fresh_db(&spec, "render_conversation_chat_missing");
        let r = rt.block_on(chat_admin::chat_render_conversation(
            &db,
            &spec.user_id,
            MISSING_ID,
        ));
        check("render_conversation_chat_missing", &r, None);
    }

    // ── ?action=bulk-reattribute ────────────────────────────────────────────
    for (name, src, target, role, dump) in [
        ("bulk_reattribute_both", Some(P_ARIA), P_EVE, None, true),
        (
            "bulk_reattribute_assistant_only",
            Some(P_ARIA),
            P_EVE,
            Some("ASSISTANT"),
            true,
        ),
        (
            "bulk_reattribute_user_only",
            Some(P_ARIA),
            P_EVE,
            Some("USER"),
            true,
        ),
        ("bulk_reattribute_null_source", None, P_CLIO, None, true),
        (
            "bulk_reattribute_no_matches",
            Some(P_CLIO),
            P_EVE,
            Some("USER"),
            true,
        ),
        (
            "bulk_reattribute_same_participant",
            Some(P_ARIA),
            P_ARIA,
            None,
            false,
        ),
        (
            "bulk_reattribute_source_not_in_chat",
            Some(P_STRANGER),
            P_EVE,
            None,
            false,
        ),
        (
            "bulk_reattribute_target_not_in_chat",
            Some(P_ARIA),
            P_STRANGER,
            None,
            false,
        ),
        (
            "bulk_reattribute_bad_role_filter",
            Some(P_ARIA),
            P_EVE,
            Some("TOOL"),
            false,
        ),
    ] {
        let db = fresh_db(&spec, name);
        let r = rt.block_on(chat_admin::chat_bulk_reattribute(
            &db, CHAT, src, target, role,
        ));
        let tables = dump.then(|| {
            json!({
                "chat": dump_chat(&db, CHAT),
                "messages": dump_messages(&db, CHAT),
                "memories": dump_memories(&db),
            })
        });
        check(name, &r, tables);
    }

    // ── ?action=rng ─────────────────────────────────────────────────────────
    // Every case replays the SAME committed byte stream from cursor 0, exactly
    // as the oracle's mocked `crypto.randomBytes` does. Byte CONSUMPTION is not
    // re-asserted here — `rng_executor_equivalence` already pins it.
    let frozen_iso = quilltap_core::clock::iso_from_unix_ms(spec.frozen_now_ms);
    for (name, kind, rolls, preview, dump) in [
        ("rng_d20_single", json!(20), None, None, true),
        ("rng_d6_three", json!(6), Some(3u32), None, true),
        ("rng_flip_coin", json!("flip_coin"), None, None, true),
        (
            "rng_flip_coin_multi",
            json!("flip_coin"),
            Some(3),
            None,
            true,
        ),
        (
            "rng_spin_the_bottle",
            json!("spin_the_bottle"),
            None,
            None,
            true,
        ),
        ("rng_preview_d20", json!(20), None, Some(true), true),
        (
            "rng_preview_coin_multi",
            json!("flip_coin"),
            Some(4),
            Some(true),
            true,
        ),
        ("rng_bad_kind", json!(1), None, None, false),
        ("rng_bad_rolls", json!(20), Some(0), None, false),
    ] {
        let db = fresh_db(&spec, name);
        let mut rng = FixedBytes::new(spec.rng_byte_stream.clone());
        let r = rt.block_on(chat_rng::chat_rng(
            &db,
            &spec.user_id,
            CHAT,
            &kind,
            rolls,
            preview,
            &mut rng,
            // The two minted values, injected: the differential blanks them, but
            // injecting keeps the service free of a hidden clock/uuid call.
            "00000000-0000-4000-8000-0000000000ff".to_string(),
            frozen_iso.clone(),
        ));
        let tables = dump.then(|| json!({ "messages": dump_messages(&db, CHAT) }));
        check(name, &r, tables);
    }
    {
        let db = fresh_db(&spec, "rng_chat_missing");
        let mut rng = FixedBytes::new(spec.rng_byte_stream.clone());
        let r = rt.block_on(chat_rng::chat_rng(
            &db,
            &spec.user_id,
            MISSING_ID,
            &json!(20),
            None,
            None,
            &mut rng,
            "00000000-0000-4000-8000-0000000000ff".to_string(),
            frozen_iso.clone(),
        ));
        check("rng_chat_missing", &r, None);
    }

    // ── ?action=run-tool ────────────────────────────────────────────────────
    // The runner is the REAL `BuiltInToolRunner` over the fixture DB, with a
    // fixed `SelfInventoryEnv` no driven tool reads. Every tool here is DB-only
    // and deterministic: an unknown name lands on v4's `Unknown tool: …`,
    // `read_conversation` on its un-rendered-conversation failure, and
    // `wardrobe_list` on the CALLING character's own wardrobe — which is what
    // makes the three participant-picking arms distinguishable.
    let operator_name = Some("Friday".to_string());
    for (name, tool, args, character, private, dump) in [
        (
            "run_tool_denied_submit_final_response",
            "submit_final_response",
            json!({}),
            None,
            None,
            false,
        ),
        (
            "run_tool_denied_request_full_context",
            "request_full_context",
            json!({}),
            None,
            None,
            false,
        ),
        (
            "run_tool_unknown_tool",
            "no_such_tool",
            json!({ "a": 1 }),
            None,
            None,
            true,
        ),
        (
            "run_tool_read_conversation",
            "read_conversation",
            json!({ "interchanges": 2 }),
            None,
            None,
            true,
        ),
        (
            "run_tool_private",
            "read_conversation",
            json!({ "interchanges": 1 }),
            None,
            Some(true),
            true,
        ),
        (
            "run_tool_named_character",
            "wardrobe_list",
            json!({}),
            Some(CLIO),
            None,
            true,
        ),
        (
            "run_tool_unmatched_character",
            "wardrobe_list",
            json!({}),
            Some(MISSING_ID),
            None,
            true,
        ),
        (
            "run_tool_default_character",
            "wardrobe_list",
            json!({}),
            None,
            None,
            true,
        ),
        ("run_tool_empty_name", "", json!({}), None, None, false),
    ] {
        let db = fresh_db(&spec, name);
        let runner = ErasedToolRunner(BuiltInToolRunner::new(db.clone(), fixture_env()));
        let r = rt.block_on(chat_run_tool::chat_run_tool(
            &db,
            &spec.user_id,
            CHAT,
            tool,
            Some(&args),
            character,
            private,
            Some(&runner),
            operator_name.clone(),
            "00000000-0000-4000-8000-0000000000ff".to_string(),
            frozen_iso.clone(),
        ));
        let tables = dump.then(|| json!({ "messages": dump_messages(&db, CHAT) }));
        check(name, &r, tables);
    }
    {
        let db = fresh_db(&spec, "run_tool_chat_missing");
        let runner = ErasedToolRunner(BuiltInToolRunner::new(db.clone(), fixture_env()));
        let r = rt.block_on(chat_run_tool::chat_run_tool(
            &db,
            &spec.user_id,
            MISSING_ID,
            "read_conversation",
            Some(&json!({})),
            None,
            None,
            Some(&runner),
            operator_name.clone(),
            "00000000-0000-4000-8000-0000000000ff".to_string(),
            frozen_iso.clone(),
        ));
        check("run_tool_chat_missing", &r, None);
    }

    // ── ?action=merge-conversation ──────────────────────────────────────────
    let all_ids: Vec<String> = vec![];
    for (name, chat, source, include, selections, dump) in [
        ("merge_all", CHAT, SOURCE_CHAT, None, None, true),
        (
            "merge_allowlist",
            CHAT,
            SOURCE_CHAT,
            Some(vec![DORIAN.to_string()]),
            None,
            true,
        ),
        (
            "merge_outfit_selections",
            CHAT,
            SOURCE_CHAT,
            None,
            Some(vec![
                json!({ "characterId": BEA, "mode": "default" }),
                json!({ "characterId": DORIAN, "mode": "none" }),
            ]),
            true,
        ),
        (
            "merge_all_already_present",
            CHAT,
            SOURCE_CHAT,
            Some(vec![CLIO.to_string()]),
            None,
            true,
        ),
        ("merge_into_itself", CHAT, CHAT, None, None, false),
        (
            "merge_empty_allowlist",
            CHAT,
            SOURCE_CHAT,
            Some(all_ids.clone()),
            None,
            false,
        ),
        ("merge_source_missing", CHAT, MISSING_ID, None, None, false),
        (
            "merge_chat_missing",
            MISSING_ID,
            SOURCE_CHAT,
            None,
            None,
            false,
        ),
    ] {
        let db = fresh_db(&spec, name);
        let r = rt.block_on(chat_merge::chat_merge_conversation(
            &db,
            &spec.user_id,
            chat,
            source,
            include.as_deref(),
            selections.as_deref(),
            None,
        ));
        let tables = dump.then(|| {
            json!({
                "chat": dump_chat(&db, CHAT),
                "messages": dump_messages(&db, CHAT),
                "sourceMessages": dump_messages(&db, SOURCE_CHAT),
            })
        });
        check(name, &r, tables);
    }

    // Shape, not a hand-written count: the two case sets must agree exactly.
    let expected: BTreeSet<String> = oracle.keys().cloned().collect();
    let missing: Vec<&String> = expected.difference(&driven).collect();
    let extra: Vec<&String> = driven.difference(&expected).collect();
    assert!(
        missing.is_empty() && extra.is_empty(),
        "case-set drift — oracle-only: {missing:?}; driven-only: {extra:?}"
    );
    assert!(failed.is_empty(), "chat-admin route mismatches: {failed:?}");
}

/// The `jobId` a chat-admin body carries, if any.
fn job_id(r: &Response) -> Option<String> {
    match r {
        Response::ChatAdmin(v) => v.get("jobId").and_then(Value::as_str).map(str::to_string),
        _ => None,
    }
}

/// A copy of a chat-admin response with one extra key (the dedupe cases' own
/// `sameJobId` witness, computed per side).
fn with_extra(r: &Response, key: &str, value: Value) -> Response {
    match r {
        Response::ChatAdmin(v) => {
            let mut m = v.as_object().cloned().unwrap_or_default();
            m.insert(key.to_string(), value);
            Response::ChatAdmin(Value::Object(m))
        }
        other => Response::ChatAdmin(serde_json::to_value(other).unwrap()),
    }
}
