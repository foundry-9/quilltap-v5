//! **P4.D183 — the transcript route differential** (v4 `5029075bb`).
//!
//! Drives v4's REAL `GET` export from `app/api/v1/messages/route.ts` on the
//! oracle side (the whole dispatcher: the `chatId` gate, the ownership 404,
//! the `Number()` coercion, the response envelopes) and v5's
//! `api::chat_transcript` verbs on this side, over a fresh copy of the
//! committed SALON fixture per case, with the same per-case plants applied to
//! both.
//!
//! **Why the salon fixture and not a new committed pair.** The same rows feed
//! `salon_reads`, so "the transcript verb's `messages`/`offSceneCharacters`
//! are byte-identical to the chat GET's" — the whole reason v4 extracted
//! `projectChatTranscript` — is a property the two corpora prove together
//! against ONE set of bytes, rather than a claim each makes about its own.
//!
//! **What this family does NOT drive: the REST edge.** v4's `chatId` 400, its
//! unknown-action envelope and its `Number()` coercion all live above the
//! dispatch boundary in v5, so this family pins v4's exact BYTES for those
//! arms and `crates/quilltap-web/tests/messages_route.rs` proves the edge
//! emits them — the `has_dangerous_unknown_action` precedent in `salon_reads`.
//!
//! Key ORDER is a comparand throughout (serde_json is built with
//! `preserve_order` workspace-wide, and the oracle round-trips through
//! `JSON.stringify` to take the wire's order rather than a live reference —
//! `nextresponse-json-hands-back-the-object-by-reference`).
//!
//! Generate the oracle (Node 24, from the v4 checkout — see the .ts header):
//!   … QT_ORACLE_OUT=/tmp/oracle-transcript-route.ndjson TZ=UTC npx jest \
//!       -- "qt-tr-oracle/cases/transcript-route\.test\.ts$"
//! Run:
//!   QT_ORACLE_TRANSCRIPT_ROUTE=/tmp/oracle-transcript-route.ndjson \
//!     cargo test -p quilltap-harness --test transcript_route_equivalence

use std::path::PathBuf;

use quilltap_core::api::chat_transcript;
use quilltap_core::api::types::{ErrorKind, Response};
use quilltap_core::db::runtime::{Db, DbPaths};
use serde::Deserialize;
use serde_json::{json, Value};

const SOLO: &str = "c1000000-0000-4000-8000-000000000001";
const GROUP: &str = "c1000000-0000-4000-8000-000000000002";
const MISSING: &str = "99999999-9999-4999-8999-999999999999";
const FOREIGN_USER: &str = "aaaaaaaa-bbbb-4ccc-8ddd-eeeeeeeeeeee";

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Spec {
    test_pepper_base64: String,
    user_id: String,
}

fn fixtures_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../quilltap-web/tests/fixtures")
}

/// What a case plants on its private copy — mirrors the oracle's `Plant`.
#[derive(Clone, Copy)]
enum Plant {
    None,
    /// `chats.transcriptVersion` for a chat (`None` = SQL NULL).
    Version(&'static str, Option<i64>),
    /// `chats.userId` for a chat — poses v4's ownership arm on the one user
    /// v5 has.
    Owner(&'static str, &'static str),
}

/// A fresh Db over a copy of the committed fixture, healed and planted exactly
/// as the oracle heals and plants its own copy.
fn venue(spec: &Spec, tag: &str, plant: Plant) -> (PathBuf, Db) {
    let scratch =
        std::env::temp_dir().join(format!("qt-transcript-route-{}-{tag}", std::process::id()));
    let _ = std::fs::remove_dir_all(&scratch);
    std::fs::create_dir_all(&scratch).unwrap();
    let main = scratch.join("main.db");
    let mount = scratch.join("mount.db");
    std::fs::copy(fixtures_dir().join("salon-main.db"), &main).unwrap();
    std::fs::copy(fixtures_dir().join("salon-mount.db"), &mount).unwrap();
    {
        let w = quilltap_core::db::Writer::open_writable(&main, &spec.test_pepper_base64).unwrap();
        // The vintage heals the committed pair needs, matching the oracle's
        // `plantOnCopy`: the route-trail and cycle-order columns from earlier
        // rounds, then the two `31436bae4` columns.
        quilltap_core::db::chat_messages_route_trail_repair::
            ensure_chat_messages_route_trail_column(w.connection())
            .unwrap();
        quilltap_core::db::chats_cycle_order_repair::ensure_chats_cycle_order_column(
            w.connection(),
        )
        .unwrap();
        quilltap_core::test_support::ensure_p4d182_columns(w.connection());
        match plant {
            Plant::None => {}
            Plant::Version(chat, v) => {
                w.connection()
                    .execute(
                        "UPDATE \"chats\" SET \"transcriptVersion\" = ?1 WHERE \"id\" = ?2",
                        rusqlite::params![v, chat],
                    )
                    .unwrap();
            }
            Plant::Owner(chat, user) => {
                w.connection()
                    .execute(
                        "UPDATE \"chats\" SET \"userId\" = ?1 WHERE \"id\" = ?2",
                        rusqlite::params![user, chat],
                    )
                    .unwrap();
            }
        }
    }
    let db = Db::open(
        DbPaths {
            main,
            mount_index: Some(mount),
            llm_logs: None,
        },
        &spec.test_pepper_base64,
    )
    .expect("open db");
    (scratch, db)
}

/// The dispatch response, rendered as the REST edge renders it: `{status,
/// body}`, so it compares directly with v4's `NextResponse`.
fn as_http(r: &Response) -> (i64, Value) {
    match r {
        Response::ChatTranscript(v) | Response::ChatMessageEvents(v) => (200, v.clone()),
        Response::Error(e) => {
            let status = match e.kind {
                ErrorKind::NotFound => 404,
                ErrorKind::BadRequest => 400,
                _ => 500,
            };
            (status, json!({ "error": e.message }))
        }
        other => panic!("unexpected core response: {other:?}"),
    }
}

/// Strip `renderedHtml` from the ORACLE's message rows — the locked
/// markdown-render divergence: the port omits the key entirely, v4 emits it
/// (as `null` here, since the oracle mocks `canPreRenderMessage` false).
///
/// `shift_remove`, NOT `remove`: serde_json is built with `preserve_order`, so
/// `Map::remove` is indexmap's SWAP-remove and would move the LAST key into
/// this slot — silently rewriting the very key order the comparand below
/// checks. (`salon_reads` had exactly that bug until this round.)
fn strip_rendered_html(body: &mut Value) {
    if let Some(msgs) = body.get_mut("messages").and_then(Value::as_array_mut) {
        for m in msgs {
            if let Some(o) = m.as_object_mut() {
                o.shift_remove("renderedHtml");
            }
        }
    }
}

/// Every key path, in order — the wire-order comparand (the `salon_reads`
/// `check_key_order` precedent).
fn key_paths(v: &Value, path: &str, out: &mut Vec<String>) {
    match v {
        Value::Object(m) => {
            out.push(format!(
                "{path}: {}",
                m.keys().cloned().collect::<Vec<_>>().join(",")
            ));
            for (k, val) in m {
                key_paths(val, &format!("{path}/{k}"), out);
            }
        }
        Value::Array(a) => {
            for (i, val) in a.iter().enumerate() {
                key_paths(val, &format!("{path}[{i}]"), out);
            }
        }
        _ => {}
    }
}

#[test]
fn transcript_route_matches_oracle() {
    let oracle_path = match std::env::var("QT_ORACLE_TRANSCRIPT_ROUTE") {
        Ok(p) => p,
        Err(_) => {
            eprintln!("SKIP: set QT_ORACLE_TRANSCRIPT_ROUTE to the oracle NDJSON (see header).");
            return;
        }
    };
    let spec: Spec = serde_json::from_str(
        &std::fs::read_to_string(
            PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("../../harness/oracle/fixtures/salon.json"),
        )
        .unwrap_or_else(|e| panic!("read spec: {e}")),
    )
    .expect("parse spec");

    let oracle: std::collections::HashMap<String, Value> = std::fs::read_to_string(&oracle_path)
        .unwrap_or_else(|e| panic!("read oracle: {e}"))
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| {
            let v: Value = serde_json::from_str(l).expect("parse oracle line");
            (v["name"].as_str().unwrap().to_string(), v)
        })
        .collect();
    assert!(
        oracle.len() >= 16,
        "the oracle carries only {} cases — regenerate it (the corpus is 16)",
        oracle.len()
    );

    let uid = &spec.user_id;
    // (name, plant, the v5 call)
    type Call = Box<dyn Fn(&Db) -> Response>;
    let tv = |known: Option<Value>| -> Call {
        Box::new(move |db: &Db| chat_transcript::chat_transcript(db, "", SOLO, known.as_ref()))
    };
    let _ = &tv;

    let mut failed: Vec<String> = Vec::new();
    let mut ran = 0usize;

    // Each case: (oracle name, plant, chat id, leg, knownVersion as it arrives
    // over dispatch — i.e. AFTER the edge's `Number()` has run).
    enum Leg {
        Transcript(Option<Value>),
        Listing,
    }
    let cases: Vec<(&str, Plant, &str, Leg)> = vec![
        (
            "unchanged_without_projecting",
            Plant::Version(SOLO, Some(7)),
            SOLO,
            Leg::Transcript(Some(json!(7))),
        ),
        (
            "whole_transcript_when_the_version_moved",
            Plant::Version(SOLO, Some(7)),
            SOLO,
            Leg::Transcript(Some(json!(6))),
        ),
        (
            "whole_transcript_when_no_version_is_offered",
            Plant::Version(SOLO, Some(7)),
            SOLO,
            Leg::Transcript(None),
        ),
        (
            "not_an_integer_is_not_trusted",
            Plant::Version(SOLO, Some(7)),
            SOLO,
            Leg::Transcript(Some(json!(7.5))),
        ),
        (
            // `Number('abc')` is NaN, which has no JSON spelling, so the edge
            // hands the verb nothing at all — same answer as absent.
            "not_a_number_at_all_is_not_trusted",
            Plant::Version(SOLO, Some(7)),
            SOLO,
            Leg::Transcript(None),
        ),
        (
            // `Number('')` is 0 — a real integer, and it MATCHES a chat whose
            // counter has never moved.
            "an_empty_known_version_is_zero",
            Plant::Version(SOLO, Some(0)),
            SOLO,
            Leg::Transcript(Some(json!(0))),
        ),
        (
            "a_row_with_no_counter_yet_is_version_zero",
            Plant::Version(SOLO, None),
            SOLO,
            Leg::Transcript(Some(json!(0))),
        ),
        (
            "off_scene_author_cards_travel_with_the_transcript",
            Plant::None,
            GROUP,
            Leg::Transcript(None),
        ),
        (
            "transcript_404s_a_chat_owned_by_someone_else",
            Plant::Owner(SOLO, FOREIGN_USER),
            SOLO,
            Leg::Transcript(None),
        ),
        (
            "transcript_404s_a_chat_this_user_cannot_see",
            Plant::None,
            MISSING,
            Leg::Transcript(None),
        ),
        (
            "listing_returns_stored_message_events",
            Plant::None,
            SOLO,
            Leg::Listing,
        ),
        (
            "listing_404s_a_chat_owned_by_someone_else",
            Plant::Owner(SOLO, FOREIGN_USER),
            SOLO,
            Leg::Listing,
        ),
        (
            // v4's present-but-empty `?action=` is JS-falsy, so the dispatcher
            // takes the DEFAULT leg — the same listing, byte for byte.
            "an_empty_action_lists_like_an_absent_one",
            Plant::None,
            SOLO,
            Leg::Listing,
        ),
    ];

    for (i, (name, plant, chat_id, leg)) in cases.iter().enumerate() {
        let (scratch, db) = venue(&spec, &format!("c{i}"), *plant);
        let got = match leg {
            Leg::Transcript(known) => {
                chat_transcript::chat_transcript(&db, uid, chat_id, known.as_ref())
            }
            Leg::Listing => chat_transcript::chat_message_events(&db, uid, chat_id),
        };
        drop(db);
        let _ = std::fs::remove_dir_all(&scratch);

        let (status, body) = as_http(&got);
        let want = oracle
            .get(*name)
            .unwrap_or_else(|| panic!("the oracle has no case named {name}"));
        let want_status = want["status"].as_i64().unwrap();
        let mut want_body = want["body"].clone();
        strip_rendered_html(&mut want_body);
        let want_body = &want_body;
        if status != want_status || &body != want_body {
            eprintln!(
                "[{name}] MISMATCH:\n  rust:   {status} {body}\n  oracle: {want_status} {want_body}"
            );
            failed.push((*name).to_string());
            continue;
        }
        // Key order, as the wire would carry it.
        let (mut gp, mut wp) = (Vec::new(), Vec::new());
        key_paths(&body, "", &mut gp);
        key_paths(want_body, "", &mut wp);
        if gp != wp {
            eprintln!("[{name}] KEY ORDER MISMATCH:\n got {gp:#?}\n want {wp:#?}");
            failed.push(format!("{name}:keyOrder"));
            continue;
        }
        eprintln!("[{name}] OK.");
        ran += 1;
    }

    // ── the three EDGE-only arms: v4's exact bytes, pinned here and emitted by
    //    `quilltap-web`'s `messages_route.rs` wire test.
    for (name, want_status, want_body) in [
        (
            "transcript_requires_a_chat_id",
            400,
            json!({"error": "Query parameter required: chatId"}),
        ),
        (
            "listing_requires_a_chat_id",
            400,
            json!({"error": "Query parameter required: chatId"}),
        ),
        (
            "an_unknown_action_is_refused_not_listed",
            400,
            json!({"error": "Unknown action: no-such-action", "availableActions": ["transcript"]}),
        ),
    ] {
        let want = oracle
            .get(name)
            .unwrap_or_else(|| panic!("the oracle has no case named {name}"));
        if want["status"].as_i64() != Some(want_status) || want["body"] != want_body {
            eprintln!(
                "[{name}] the edge's recorded bytes have MOVED:\n  expected: {want_status} {want_body}\n  oracle:   {} {}",
                want["status"], want["body"]
            );
            failed.push(name.to_string());
        } else {
            eprintln!("[{name}] OK (edge bytes pinned; the wire test proves the edge emits them).");
            ran += 1;
        }
    }

    assert_eq!(ran, 16, "every case must have run");
    assert!(
        failed.is_empty(),
        "transcript-route differences: {failed:?}"
    );
    eprintln!("OK: transcript route matched oracle ({ran} cases).");
}
