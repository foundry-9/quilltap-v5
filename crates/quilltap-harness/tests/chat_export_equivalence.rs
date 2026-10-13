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
// P4.D282 — the outfit-summary plants' ids (`build-chat-dialogs-fixture.ts`).
const NORA: &str = "a2000000-0000-4000-8000-000000000001";
const PIP: &str = "a2000000-0000-4000-8000-000000000002";
const VERA: &str = "a2000000-0000-4000-8000-000000000003";
const WREN: &str = "a2000000-0000-4000-8000-000000000004";
const N_DEF: &str = "00000000-0000-4000-8000-0000000000a1";
const P_BOOTS: &str = "00000000-0000-4000-8000-0000000000b2";
const P_SCARF: &str = "00000000-0000-4000-8000-0000000000b3";
const P_SET: &str = "00000000-0000-4000-8000-0000000000b4";
const G_CAPE: &str = "00000000-0000-4000-8000-0000000000c1";
const GRP_LANTERN: &str = "00000000-0000-4000-8000-0000000000d1";
/// The planted two-slot LEAF (`plantDress`).
const P_SMOCK: &str = "00000000-0000-4000-8000-0000000000b9";

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

fn fresh_db(spec: &Spec, tag: &str) -> (Db, tempfile::TempDir) {
    let scratch_dir = tempfile::Builder::new()
        .prefix(&format!("qt-cdx-{tag}-"))
        .tempdir()
        .expect("tempdir");
    let scratch = scratch_dir.path();
    let main = scratch.join("main.db");
    let mount = scratch.join("mount.db");
    std::fs::copy(fixtures_dir().join("chat-dialogs-main.db"), &main).unwrap();
    std::fs::copy(fixtures_dir().join("chat-dialogs-mount.db"), &mount).unwrap();
    // P4.D171: the committed `chat-dialogs-{main,mount}.db` predates the two
    // `78b381a96`-round schema moves — the same repaired-at-boot idiom
    // `web_search_runner_wire.rs` uses for the connection-profiles pair.
    {
        let w = quilltap_core::db::Writer::open_writable(&main, &spec.test_pepper_base64).unwrap();
        quilltap_core::db::chat_messages_route_trail_repair::ensure_chat_messages_route_trail_column(
            w.connection(),
        )
        .expect("ensure the route-trail column on the vintage fixture");
        quilltap_core::db::chats_cycle_order_repair::ensure_chats_cycle_order_column(
            w.connection(),
        )
        .expect("ensure the cycle-order column on the vintage fixture");
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
    (db, scratch_dir)
}

/// P4.106 item 6 — the oracle's `plantInform`, cell for cell, on the per-run
/// COPY: two `chat_informs` rows (pending + consumed) through
/// `ChatInformsRepository::create`, and the inform's Host record message
/// through the production `add_message` (how `?action=inform` writes it).
fn plant_inform(db: &Db, chat_id: &str) {
    use quilltap_core::db::chat_informs::{ChatInformCreate, ChatInformsRepository};
    const RECORD: &str = "1f100000-0000-4000-8000-0000000000d1";
    let chat = chat_id.to_string();
    db.write_blocking(move |w| {
        let conn = w.main().connection();
        quilltap_core::db::chat_informs::ensure_chat_informs_table(conn)?;
        let repo = ChatInformsRepository::new(conn);
        for consumed in [false, true] {
            let k = if consumed { 2 } else { 1 };
            repo.create(&ChatInformCreate {
                id: format!("1f10000{k}-0000-4000-8000-00000000000{k}"),
                chat_id: chat.clone(),
                batch_id: format!("1f10000{k}-0000-4000-8000-0000000000b{k}"),
                participant_id: format!("1f10000{k}-0000-4000-8000-0000000000a{k}"),
                content_markdown: "The clock in the hall has stopped.".into(),
                record_message_id: Some(RECORD.into()),
                // P4.D249: the consumed row is a delivered STANDING inform
                // (the oracle's `permanent: consumed`) — neutrality only.
                permanent: consumed,
                created_at: "2026-01-02T00:00:00.000Z".into(),
                updated_at: if consumed {
                    "2026-01-03T00:00:00.000Z".into()
                } else {
                    "2026-01-02T00:00:00.000Z".into()
                },
                consumed_at: consumed.then(|| "2026-01-03T00:00:00.000Z".to_string()),
                consumed_by_message_id: consumed
                    .then(|| format!("1f10000{k}-0000-4000-8000-0000000000e{k}")),
            })?;
        }
        let event: quilltap_core::db::chats_messages::ChatEventInput =
            serde_json::from_value(json!({
                "type": "message",
                "id": RECORD,
                "role": "ASSISTANT",
                "content": "The clock in the hall has stopped.",
                "opaqueContent": null,
                "attachments": [],
                "createdAt": "2026-01-02T00:00:00.000Z",
                "participantId": null,
                "systemSender": "host",
                "systemKind": "inform",
                "targetParticipantIds": null,
            }))
            .expect("inform record event");
        w.main().chat_messages().add_message(&chat, &event)
    })
    .expect("plant the inform on the fixture copy");
}

/// P4.D282 — the oracle's `plantDress`: Pip's two-slot smock (a LEAF covering
/// top + bottom) filed through the REAL character wardrobe location `create`,
/// on the per-run COPY.
fn plant_dress(db: &Db, user_id: &str) {
    use quilltap_core::db::vault_wardrobe_public::WardrobeScope;
    use quilltap_core::db::wardrobe_location::{
        resolve_wardrobe_location, ResolveWardrobeLocationOptions,
    };
    let user = user_id.to_string();
    db.write_blocking(move |w| {
        let main = w.main().connection();
        let mount = w.mount_index().expect("mount-index writer").connection();
        let location = resolve_wardrobe_location(
            main,
            mount,
            WardrobeScope::Character,
            Some(PIP),
            &user,
            ResolveWardrobeLocationOptions::default(),
        )?
        .expect("Pip has a wardrobe location");
        let item = quilltap_core::vault_overlay::WardrobeItem::from_read_value(&json!({
            "id": P_SMOCK,
            "characterId": PIP,
            "title": "Pip’s oilcloth smock",
            "description": null,
            "imagePrompt": null,
            "types": ["top", "bottom"],
            "componentItemIds": [],
            "appropriateness": null,
            "isDefault": false,
            "replace": false,
            "migratedFromClothingRecordId": null,
            "archivedAt": null,
            "folder": "",
            "createdAt": "2026-01-02T00:00:00.000Z",
            "updatedAt": "2026-01-02T00:00:00.000Z",
        }));
        location
            .create(main, mount, &item)
            .map_err(|e| quilltap_core::db::DbError::Internal(e.message()))?;
        Ok(())
    })
    .expect("plant the smock on the fixture copy");
}

/// P4.D282 — the oracle's `plantOutfit`: the chat's stored `equippedOutfit`
/// overwritten with a raw UPDATE on the per-run COPY.
fn plant_outfit(db: &Db, chat_id: &str, outfit: &Value) {
    let (chat, outfit) = (chat_id.to_string(), outfit.to_string());
    db.write_blocking(move |w| {
        w.main().connection().execute(
            "UPDATE chats SET \"equippedOutfit\" = ?1 WHERE id = ?2",
            rusqlite::params![outfit, chat],
        )?;
        Ok(())
    })
    .expect("plant the outfit on the fixture copy");
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
                // The store-unavailable refusal (P4.23) — v4's deliberate
                // contextful 503 (context.ts:176-205).
                ErrorKind::Unavailable => 503,
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
        let (db, _scratch) = fresh_db(&spec, name);
        let r = quilltap_core::services::chat_export::chat_export(&db, &spec.user_id, chat);
        check(name, &r);
    }

    // ── P4.106 item 6: both export surfaces over a planted Inform ──────────
    for (name, markdown) in [
        ("export_with_inform", false),
        ("export_markdown_with_inform", true),
    ] {
        let (db, _scratch) = fresh_db(&spec, name);
        plant_inform(&db, EXPORT_CHAT);
        let r = if markdown {
            quilltap_core::services::markdown_transcript::chat_export_markdown(
                &db,
                &spec.user_id,
                EXPORT_CHAT,
                &quilltap_core::host_zone::TimeZone::UTC,
            )
        } else {
            quilltap_core::services::chat_export::chat_export(&db, &spec.user_id, EXPORT_CHAT)
        };
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
        let (db, _scratch) = fresh_db(&spec, name);
        let r = quilltap_core::services::markdown_transcript::chat_export_markdown(
            &db,
            &spec.user_id,
            chat,
            &quilltap_core::host_zone::TimeZone::UTC,
        );
        check(name, &r);
    }

    // ── ?action=outfit-summary ──────────────────────────────────────────────
    for (name, chat) in [
        ("outfit_summary_rich", MERGE_SOURCE),
        ("outfit_summary_empty", EXPORT_CHAT),
        ("outfit_summary_chat_missing", MISSING_ID),
    ] {
        let (db, _scratch) = fresh_db(&spec, name);
        let r = quilltap_core::api::chat_outfits::chat_outfit_summary(&db, chat);
        check(name, &r);
    }
    // P4.D282 (`9ecebf986`, pin `4a38222e6`) — the per-character pools and the
    // leaf routing, each over a plant on the copy (the oracle's case list, the
    // same outfits).
    let cast = json!({
        (PIP): { "top": [G_CAPE, P_SET], "footwear": [P_BOOTS] },
        (NORA): { "top": [N_DEF], "accessories": [P_SCARF] },
        (WREN): { "top": [GRP_LANTERN] },
        (VERA): null,
    });
    for (name, dress, outfit) in [
        (
            "outfit_summary_leaf_spread",
            true,
            json!({ (PIP): { "top": [P_SMOCK] } }),
        ),
        (
            "outfit_summary_composite_spread",
            false,
            json!({ (PIP): { "bottom": [P_SET], "accessories": [P_SCARF] } }),
        ),
        ("outfit_summary_cast_pools", false, cast),
        ("outfit_summary_empty_map", false, json!({})),
    ] {
        let (db, _scratch) = fresh_db(&spec, name);
        if dress {
            plant_dress(&db, &spec.user_id);
        }
        plant_outfit(&db, MERGE_SOURCE, &outfit);
        let r = quilltap_core::api::chat_outfits::chat_outfit_summary(&db, MERGE_SOURCE);
        check(name, &r);
    }

    // P4.D282 — the summary's own line: DEBUG `[Chats v1] Built outfit summary`
    // `{chatId, characterCount, context}` once per answered summary (logged on
    // the caller thread — the summary is a read), the resolver's DEBUG for
    // Pip's borrowed hat beside it; a missing chat answers 404 before either.
    {
        let (db, _scratch) = fresh_db(&spec, "summary-lines");
        let (_r, lines) = quilltap_core::test_support::captured_with(|| {
            quilltap_core::api::chat_outfits::chat_outfit_summary(&db, MERGE_SOURCE)
        });
        let built: Vec<&String> = lines
            .iter()
            .filter(|l| l.contains("[Chats v1] Built outfit summary"))
            .collect();
        assert_eq!(built.len(), 1, "{lines:#?}");
        for field in [
            &format!("chatId={MERGE_SOURCE}"),
            "characterCount=2",
            "context=wardrobe",
        ] {
            assert!(built[0].contains(field), "{field} in {}", built[0]);
        }
        assert!(built[0].starts_with("DEBUG "), "{}", built[0]);
        let unresolved: Vec<&String> = lines
            .iter()
            .filter(|l| l.contains("Equipped ids not in the wearable pool"))
            .collect();
        assert_eq!(unresolved.len(), 1, "{lines:#?}");
        assert!(
            unresolved[0].contains(&format!("characterId={PIP}"))
                && unresolved[0].contains("unresolvedCount=1"),
            "{}",
            unresolved[0]
        );
        let (_r, lines) = quilltap_core::test_support::captured_with(|| {
            quilltap_core::api::chat_outfits::chat_outfit_summary(&db, MISSING_ID)
        });
        assert!(
            !lines.iter().any(|l| l.contains("Built outfit summary")
                || l.contains("Error fetching equipped outfit summary")),
            "{lines:#?}"
        );
    }
    // An unreadable `chats` table: v4's `findById` is a fallback read (the
    // repository's ERROR, then `null`), so the summary answers the 404 — its
    // own catch (`Error fetching equipped outfit summary`) is not reached.
    {
        let (db, _scratch) = fresh_db(&spec, "summary-unreadable");
        db.write_blocking(|w| {
            w.main()
                .connection()
                .execute_batch("DROP TABLE chats;")
                .map_err(quilltap_core::db::DbError::from)
        })
        .expect("drop chats on the copy");
        let (r, lines) = quilltap_core::test_support::captured_with(|| {
            quilltap_core::api::chat_outfits::chat_outfit_summary(&db, MERGE_SOURCE)
        });
        assert_eq!(project(&r)["status"], json!(404), "{r:?}");
        assert!(
            lines.iter().any(|l| l.starts_with("ERROR ")
                && l.contains("Error finding entity by ID")
                && l.contains("collection=chats")),
            "{lines:#?}"
        );
        assert!(
            !lines
                .iter()
                .any(|l| l.contains("Error fetching equipped outfit summary")),
            "{lines:#?}"
        );
    }

    // ── ?action=group-stores (the P4.9E3B tier-2 audit port) ───────────────
    for (name, chat) in [
        ("group_stores_watch", SR_CHAT),
        ("group_stores_empty", TOOLS_CHAT_BARE),
        ("group_stores_chat_missing", MISSING_ID),
    ] {
        let (db, _scratch) = fresh_db(&spec, name);
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
