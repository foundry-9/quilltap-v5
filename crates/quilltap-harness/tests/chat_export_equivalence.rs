//! P4.9E3B chat-export + outfit-summary differential:
//! `services::chat_export::chat_export` + `api::chat_outfits::chat_outfit_summary`
//! vs v4's REAL chat GET handler (`handlers/get.ts` `?action=export` /
//! `?action=outfit-summary`). Both sides read a FRESH copy of the committed
//! `chat-dialogs-{main,mount}.db` fixture per case.
//!
//! The export cases compare the JSONL **body text and download headers
//! byte-for-byte** (no normalization — the bytes are the deliverable). The
//! outfit-summary / error cases compare status + JSON body; the JSON cases'
//! `Content-Type` is transport furniture (NextResponse.json vs axum) and is
//! not part of the ported surface.
//!
//! Coverage is asserted by SHAPE (`harness-corpus-shape-constants-rot`): the
//! driven case set must equal the oracle's exactly.
//!
//! Generate the oracle (Node 24, from the v4 checkout — see the .ts header):
//!   … QT_ORACLE_OUT=/tmp/oracle-chat-export.ndjson npx jest -- chat-dialogs-export
//! Run:
//!   QT_ORACLE_CHAT_EXPORT=/tmp/oracle-chat-export.ndjson \
//!     cargo test -p quilltap-harness --test chat_export_equivalence -- --nocapture

use std::collections::{BTreeSet, HashMap};
use std::path::PathBuf;

use quilltap_core::api::types::{ErrorKind, Response};
use quilltap_core::content_disposition::{build_content_disposition, Disposition};
use quilltap_core::db::runtime::{Db, DbPaths};
use serde::Deserialize;
use serde_json::{json, Value};

const EXPORT_CHAT: &str = "c2000000-0000-4000-8000-000000000001";
const NOCHAR_CHAT: &str = "c2000000-0000-4000-8000-000000000002";
const SR_CHAT: &str = "c2000000-0000-4000-8000-000000000003";
const TOOLS_CHAT_BARE: &str = "c2000000-0000-4000-8000-000000000006";
const MERGE_SOURCE: &str = "c2000000-0000-4000-8000-000000000008";
const MERGE_TARGET: &str = "c2000000-0000-4000-8000-000000000007";
const MARKDOWN_CHAT: &str = "c2000000-0000-4000-8000-000000000009";
const MISSING_ID: &str = "99999999-9999-4999-8999-999999999999";

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Spec {
    test_pepper_base64: String,
    user_id: String,
}

fn spec_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../harness/oracle/fixtures/chat-dialogs-web.json")
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
    let scratch = std::env::temp_dir().join(format!("qt-cdx-{}-{}", tag, std::process::id()));
    let _ = std::fs::remove_dir_all(&scratch);
    std::fs::create_dir_all(&scratch).unwrap();
    let main = scratch.join("main.db");
    let mount = scratch.join("mount.db");
    std::fs::copy(fixtures_dir().join("chat-dialogs-main.db"), &main).unwrap();
    std::fs::copy(fixtures_dir().join("chat-dialogs-mount.db"), &mount).unwrap();
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

/// The web-edge projection of a Response for this family: status + headers +
/// body/bodyText, mirroring what the oracle records.
fn project(r: &Response) -> Value {
    match r {
        Response::ChatExportPayload { filename, jsonl } => json!({
            "status": 200,
            "contentType": "application/x-ndjson",
            "contentDisposition": format!("attachment; filename=\"{filename}\""),
            "bodyText": jsonl,
        }),
        // P4.d28: the transcript's edge — the RFC 5987 disposition (built by the
        // PRODUCTION helper, not a copy of it) and the `no-store` header v4
        // sends here and nowhere else in this fan-out.
        Response::ChatMarkdownTranscriptPayload { filename, markdown } => json!({
            "status": 200,
            "contentType": "text/markdown; charset=utf-8",
            "contentDisposition": build_content_disposition(filename, Disposition::Attachment),
            "cacheControl": "no-store",
            "bodyText": markdown,
        }),
        Response::ChatDialog(v) => json!({ "status": 200, "body": v }),
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
            json!({ "status": status, "body": { "error": e.message } })
        }
        other => json!({ "status": 500, "body": serde_json::to_value(other).unwrap() }),
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

fn first_diff(got: &str, want: &str) -> String {
    let g: Vec<&str> = got.lines().collect();
    let w: Vec<&str> = want.lines().collect();
    for i in 0..g.len().max(w.len()) {
        let gi = g.get(i).copied().unwrap_or("<none>");
        let wi = w.get(i).copied().unwrap_or("<none>");
        if gi != wi {
            return format!("  line {i}\n  GOT : {gi}\n  WANT: {wi}");
        }
    }
    "(identical line-by-line)".to_string()
}

#[test]
fn chat_export_matches_oracle() {
    let Some(oracle_path) = env_or_skip("QT_ORACLE_CHAT_EXPORT") else {
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

    let mut failed: Vec<String> = Vec::new();
    let mut driven: BTreeSet<String> = BTreeSet::new();

    let mut check = |name: &str, resp: &Response| {
        driven.insert(name.to_string());
        let Some(want) = oracle.get(name) else {
            failed.push(format!("{name}_MISSING_FROM_ORACLE"));
            return;
        };
        let got = project(resp);

        // Status: exact.
        if got["status"] != want["status"] {
            eprintln!("[{name}] STATUS {} != {}", got["status"], want["status"]);
            failed.push(format!("{name}_status"));
        }

        if let Some(want_text) = want.get("bodyText").and_then(Value::as_str) {
            // The byte legs: headers + JSONL text, verbatim.
            for key in ["contentType", "contentDisposition", "cacheControl"] {
                // `cacheControl` is recorded only for the transcript cases.
                if want.get(key).is_none() && got.get(key).is_none() {
                    continue;
                }
                if got.get(key) != want.get(key) {
                    eprintln!(
                        "[{name}] {key} mismatch: {:?} != {:?}",
                        got.get(key),
                        want.get(key)
                    );
                    failed.push(format!("{name}_{key}"));
                }
            }
            let got_text = got.get("bodyText").and_then(Value::as_str).unwrap_or("");
            if got_text != want_text {
                eprintln!(
                    "[{name}] JSONL MISMATCH:\n{}",
                    first_diff(got_text, want_text)
                );
                failed.push(name.to_string());
            } else {
                eprintln!("[{name}] JSONL bytes OK ({} bytes).", got_text.len());
            }
        } else {
            // JSON cases: status + body (content type is transport furniture).
            let g = serde_json::to_string_pretty(&sorted(&got["body"])).unwrap();
            let w = serde_json::to_string_pretty(&sorted(&want["body"])).unwrap();
            if g != w {
                eprintln!("[{name}] BODY MISMATCH:\n{}", first_diff(&g, &w));
                failed.push(name.to_string());
            } else {
                eprintln!("[{name}] body OK.");
            }
            // The summary's key order is part of the surface (the equipped
            // object's own entry order) — compare the raw serialization too.
            if want["status"] == 200 {
                let graw = got["body"].to_string();
                // Reserialize the oracle's body with preserve_order intact.
                let wraw = want["body"].to_string();
                if graw != wraw {
                    eprintln!("[{name}] BODY KEY-ORDER MISMATCH:\n  GOT : {graw}\n  WANT: {wraw}");
                    failed.push(format!("{name}_key_order"));
                }
            }
        }
    };

    // ── ?action=export ──────────────────────────────────────────────────────
    for (name, chat) in [
        ("export_multi", EXPORT_CHAT),
        ("export_no_character", NOCHAR_CHAT),
        ("export_chat_missing", MISSING_ID),
    ] {
        let db = fresh_db(&spec, name);
        let r = quilltap_core::services::chat_export::chat_export(&db, &spec.user_id, chat);
        check(name, &r);
    }

    // ── ?action=export-markdown (P4.d28) ────────────────────────────────────
    for (name, chat) in [
        ("export_markdown_rich", MARKDOWN_CHAT),
        ("export_markdown_defaults", EXPORT_CHAT),
        ("export_markdown_scenario", MERGE_TARGET),
        ("export_markdown_empty", NOCHAR_CHAT),
        ("export_markdown_chat_missing", MISSING_ID),
    ] {
        let db = fresh_db(&spec, name);
        let r = quilltap_core::services::markdown_transcript::chat_export_markdown(
            &db,
            &spec.user_id,
            chat,
        );
        check(name, &r);
    }

    // ── ?action=outfit-summary ──────────────────────────────────────────────
    for (name, chat) in [
        ("outfit_summary_rich", MERGE_SOURCE),
        ("outfit_summary_empty", EXPORT_CHAT),
        ("outfit_summary_chat_missing", MISSING_ID),
    ] {
        let db = fresh_db(&spec, name);
        let r = quilltap_core::api::chat_outfits::chat_outfit_summary(&db, chat);
        check(name, &r);
    }

    // ── ?action=group-stores (the P4.9E3B tier-2 audit port) ───────────────
    for (name, chat) in [
        ("group_stores_watch", SR_CHAT),
        ("group_stores_empty", TOOLS_CHAT_BARE),
        ("group_stores_chat_missing", MISSING_ID),
    ] {
        let db = fresh_db(&spec, name);
        let r = quilltap_core::api::chat_media::chat_group_stores(&db, chat);
        check(name, &r);
    }

    let expected: BTreeSet<String> = oracle.keys().cloned().collect();
    let missing: Vec<&String> = expected.difference(&driven).collect();
    let extra: Vec<&String> = driven.difference(&expected).collect();
    assert!(
        missing.is_empty() && extra.is_empty(),
        "case-set drift — oracle-only: {missing:?}; driven-only: {extra:?}"
    );
    assert!(failed.is_empty(), "chat-export mismatches: {failed:?}");
}
