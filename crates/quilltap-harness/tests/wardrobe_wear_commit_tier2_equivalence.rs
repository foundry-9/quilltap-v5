//! Tier-2 differential (P4.D262 item 15): the wear ledger's WRITE chokepoint —
//! v4 `3ee3b1342` "Wardrobe wear ledger (#81)" — and every path that writes
//! equipped slots through it.
//!
//! Both sides run the SAME op sequence (`fixtures/wardrobe-wear-commit-
//! tier2.json`) on ONE copy of a wardrobe-tools fixture pair built with that
//! spec as its `QT_WT_AVATAR_SPEC` companion (nine extra chats, each seeded
//! with the caller's Blue Blouse; the `Casual Outfit` bundle is `isDefault`):
//!
//! - `commit` → v4's REAL `WardrobeWearRepository.commitEquippedOutfit` vs
//!   `services::wardrobe_wear_commit::commit_equipped_outfit` (v4's spec
//!   `wardrobe-wear.repository.test.ts:237-330`: a newly worn leaf, a removal
//!   that credits nothing and a re-wear that is a second wear, two wearers, a
//!   bundle credited once, a bundle whose leaves were all on, a merge, equal
//!   slots still written, a legacy whole-composite id, an earlier wear landing
//!   late, and a write to a MISSING chat — which v4 does NOT treat as a lost
//!   write: its `update` is a no-op and `setEquippedOutfit` still returns the
//!   slots, so the chokepoint credits);
//! - `apply` → v4's REAL `applyOutfitSelections` vs
//!   `services::outfit_selections::apply_outfit_selections` (default /
//!   manual with and without `wornBundleIds` / none / previous_chat under
//!   `merge` and `chat-start`; `llm_choose`'s credit rides
//!   `outfit_llm_choose_tier3`);
//! - `route` → v4's REAL `handleEquipSlot` vs `api::chat_outfits::chat_equip`
//!   (`set_all`'s `wornBundleIds` claim incl. ids that do not resolve, the
//!   Zod refusals, `clear_slot` as `take-off`, `wear` / `add_to_slot` of a
//!   bundle, a missing chat).
//!
//! Comparands per op: the result (the commit's `{slots, newlyWornLeafIds,
//! creditedBundleIds, changed}` or its thrown message; the route's status +
//! body), the op chat's `equippedOutfit`, EVERY `wardrobe_wear_stats` row
//! minus the minted `id` / `createdAt` / `updatedAt` (a wear stamped `now` by
//! either side collapses to `<now>`; the spec's pinned `at`s compare exactly),
//! and the ledger's log lines (message + every field). The route's chokepoint
//! lines run inside the writer closure, where the capture rig cannot see them
//! (memory note `capture-rig-writer-thread`), so for `route` ops only the
//! route's own lines (logged on the caller thread) are compared; `commit` and
//! `apply` ops run on the test thread and compare every line.
//!
//! Generate (Node 24, from the v4 checkout or the round's pin):
//!   N=~/.nvm/versions/node/v24.13.1/bin ; V5=~/source/quilltap-v5
//!   cd ~/source/quilltap-server
//!   QT_WT_AVATAR_SPEC=$V5/harness/oracle/fixtures/wardrobe-wear-commit-tier2.json \
//!   QT_FIXTURE_WT_MAIN=/tmp/qt-wwc-main.db QT_FIXTURE_WT_MOUNT=/tmp/qt-wwc-mount.db \
//!     $N/node --import tsx $V5/harness/oracle/fixtures/build-wardrobe-tools-fixture.ts
//!   QT_FIXTURE_WWC_MAIN=/tmp/qt-wwc-main.db QT_FIXTURE_WWC_MOUNT=/tmp/qt-wwc-mount.db \
//!     $N/node --import tsx $V5/harness/oracle/cases/wardrobe-wear-commit-tier2.ts \
//!     > /tmp/oracle-wardrobe-wear-commit.ndjson
//! Run:
//!   QT_ORACLE_WWC=/tmp/oracle-wardrobe-wear-commit.ndjson \
//!   QT_FIXTURE_WWC_MAIN=/tmp/qt-wwc-main.db QT_FIXTURE_WWC_MOUNT=/tmp/qt-wwc-mount.db \
//!     cargo test -p quilltap-harness --test wardrobe_wear_commit_tier2_equivalence -- --nocapture

use std::collections::BTreeMap;

use quilltap_core::api::chat_outfits::chat_equip;
use quilltap_core::api::types::{ErrorKind, Response};
use quilltap_core::db::chats_outfits::ChatOutfitsRepository;
use quilltap_core::db::runtime::{Db, DbPaths};
use quilltap_core::db::wardrobe_wear_stats::{EquipSource, WornBundle};
use quilltap_core::db::Writer;
use quilltap_core::model::completion::CannedCompletionProvider;
use quilltap_core::services::cheap_llm_exec::CheapLlmTaskExecutor;
use quilltap_core::services::creation_progress::CreationProgressEmitter;
use quilltap_core::services::outfit_selections::{
    apply_outfit_selections, OutfitContext, OutfitSelection,
};
use quilltap_core::services::wardrobe_wear_commit::{
    commit_equipped_outfit, CommitEquippedOutfitInput,
};
use quilltap_core::test_support::captured_with;
use quilltap_core::wardrobe::Slots;
use serde::Deserialize;
use serde_json::{json, Value};

#[derive(Deserialize)]
struct Base {
    #[serde(rename = "testPepperBase64")]
    test_pepper_base64: String,
    #[serde(rename = "userId")]
    user_id: String,
    #[serde(rename = "callerCharacterId")]
    caller_character_id: String,
    #[serde(rename = "recipientCharacterId")]
    recipient_character_id: String,
}

#[derive(Deserialize)]
struct Spec {
    ops: Vec<Op>,
}

#[derive(Deserialize)]
struct WireBundle {
    id: String,
    #[serde(rename = "leafIds")]
    leaf_ids: Vec<String>,
}

#[derive(Deserialize)]
struct Op {
    name: String,
    kind: String,
    chat: String,
    #[serde(default)]
    who: Option<String>,
    #[serde(default, rename = "nextSlots")]
    next_slots: Option<Value>,
    #[serde(default, rename = "wornBundles")]
    worn_bundles: Vec<WireBundle>,
    #[serde(default)]
    source: Option<String>,
    #[serde(default)]
    at: Option<String>,
    #[serde(default, rename = "sourceChat")]
    source_chat: Option<String>,
    #[serde(default)]
    selections: Vec<Value>,
    #[serde(default)]
    body: Option<Value>,
}

/// The messages this family compares (the oracle captures the same set).
const CAPTURED: [&str; 6] = [
    "Committed equipped outfit",
    "Equipped outfit write failed; no wears credited",
    "[applyOutfitSelections] Failed to persist equipped outfit",
    "[Chats v1] Equipped outfit replaced (set_all)",
    "[Chats v1] Some claimed worn bundles were not credited",
    "[Chats v1] Error equipping wardrobe slot",
];

/// The lines a `route` op can compare: the ones the route logs on the
/// caller thread (the chokepoint's run inside the writer closure).
const ROUTE_LEVEL: [&str; 3] = [
    "[Chats v1] Equipped outfit replaced (set_all)",
    "[Chats v1] Some claimed worn bundles were not credited",
    "[Chats v1] Error equipping wardrobe slot",
];

fn source_of(s: &str) -> EquipSource {
    match s {
        "ui" => EquipSource::Ui,
        "tool" => EquipSource::Tool,
        "chat-start" => EquipSource::ChatStart,
        "participant-added" => EquipSource::ParticipantAdded,
        "merge" => EquipSource::Merge,
        "take-off" => EquipSource::TakeOff,
        other => panic!("unknown source {other}"),
    }
}

/// A captured line → `(level, message, fields)`. The rig renders
/// `LEVEL target message k=v k=v…`; the message is one of [`CAPTURED`].
fn parse_line(line: &str) -> Option<(String, String, BTreeMap<String, String>)> {
    let (level, rest) = line.split_once(' ')?;
    let (_target, rest) = rest.split_once(' ')?;
    let message = CAPTURED.iter().find(|m| rest.starts_with(**m))?;
    let tail = rest[message.len()..].trim_start();
    let mut fields = BTreeMap::new();
    // Split on ` <ident>=` boundaries; a value may carry spaces (an error).
    let mut keys: Vec<(usize, usize)> = Vec::new();
    let bytes = tail.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        let at_start = i == 0 || bytes[i - 1] == b' ';
        if at_start {
            let mut j = i;
            while j < bytes.len() && (bytes[j].is_ascii_alphanumeric() || bytes[j] == b'_') {
                j += 1;
            }
            if j > i && j < bytes.len() && bytes[j] == b'=' {
                keys.push((i, j));
                i = j + 1;
                continue;
            }
        }
        i += 1;
    }
    for (n, (start, eq)) in keys.iter().enumerate() {
        let end = keys
            .get(n + 1)
            .map(|(next, _)| next - 1)
            .unwrap_or(tail.len());
        fields.insert(tail[*start..*eq].to_string(), tail[eq + 1..end].to_string());
    }
    Some((level.to_lowercase(), message.to_string(), fields))
}

/// v4's captured line → the same shape (field values as the rig renders them).
fn oracle_line(v: &Value) -> (String, String, BTreeMap<String, String>) {
    let fields = v["fields"]
        .as_object()
        .unwrap()
        .iter()
        .map(|(k, val)| {
            let rendered = match val {
                Value::String(s) => s.clone(),
                other => other.to_string(),
            };
            (k.clone(), rendered)
        })
        .collect();
    (
        v["level"].as_str().unwrap().to_string(),
        v["message"].as_str().unwrap().to_string(),
        fields,
    )
}

/// Collapse every ISO stamp the spec did not pin (a wear stamped `now`).
fn normalize_ledger(rows: Value, pinned: &[String]) -> Value {
    let Value::Array(rows) = rows else {
        return rows;
    };
    Value::Array(
        rows.into_iter()
            .map(|mut row| {
                for k in ["firstWornAt", "lastWornAt"] {
                    if let Some(Value::String(s)) = row.get(k) {
                        if !pinned.contains(s) {
                            row[k] = json!("<now>");
                        }
                    }
                }
                row
            })
            .collect(),
    )
}

fn ledger(main: &rusqlite::Connection) -> Value {
    let mut st = main
        .prepare(
            "SELECT \"itemId\", \"wearerCharacterId\", \"wearCount\", \"firstWornAt\", \"lastWornAt\", \"lastWornChatId\"
               FROM \"wardrobe_wear_stats\" ORDER BY \"itemId\", COALESCE(\"wearerCharacterId\", '')",
        )
        .unwrap();
    let rows: Vec<Value> = st
        .query_map([], |r| {
            Ok(json!({
                "itemId": r.get::<_, String>(0)?,
                "wearerCharacterId": r.get::<_, Option<String>>(1)?,
                "wearCount": r.get::<_, i64>(2)?,
                "firstWornAt": r.get::<_, String>(3)?,
                "lastWornAt": r.get::<_, String>(4)?,
                "lastWornChatId": r.get::<_, Option<String>>(5)?,
            }))
        })
        .unwrap()
        .map(Result::unwrap)
        .collect();
    Value::Array(rows)
}

fn status_body(r: &Response) -> Value {
    match r {
        Response::ChatOutfit(v) => json!({ "status": 200, "body": v }),
        Response::Error(e) => {
            let status = match e.kind {
                ErrorKind::BadRequest => 400,
                ErrorKind::NotFound => 404,
                ErrorKind::Internal => 500,
                ref other => panic!("unexpected error kind {other:?}"),
            };
            json!({ "status": status, "body": { "error": e.message } })
        }
        other => panic!("unexpected response {other:?}"),
    }
}

#[test]
fn wardrobe_wear_commit_matches_oracle() {
    let (Ok(oracle_path), Ok(fixture_main), Ok(fixture_mount)) = (
        std::env::var("QT_ORACLE_WWC"),
        std::env::var("QT_FIXTURE_WWC_MAIN"),
        std::env::var("QT_FIXTURE_WWC_MOUNT"),
    ) else {
        eprintln!(
            "SKIP: set QT_ORACLE_WWC + QT_FIXTURE_WWC_MAIN + QT_FIXTURE_WWC_MOUNT (see header)."
        );
        return;
    };
    let fixtures =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../harness/oracle/fixtures");
    let base: Base = serde_json::from_str(
        &std::fs::read_to_string(fixtures.join("wardrobe-tools.json")).unwrap(),
    )
    .unwrap();
    let spec: Spec = serde_json::from_str(
        &std::fs::read_to_string(fixtures.join("wardrobe-wear-commit-tier2.json")).unwrap(),
    )
    .unwrap();
    let oracle: Vec<Value> = std::fs::read_to_string(&oracle_path)
        .unwrap_or_else(|e| panic!("read {oracle_path}: {e}"))
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| serde_json::from_str(l).unwrap())
        .collect();
    assert_eq!(
        oracle.len(),
        spec.ops.len(),
        "the oracle has {} rows for {} ops — regenerate it",
        oracle.len(),
        spec.ops.len()
    );
    let pinned: Vec<String> = spec.ops.iter().filter_map(|o| o.at.clone()).collect();

    let scratch = tempfile::tempdir().unwrap();
    let work_main = scratch.path().join("wwc-main.db");
    let work_mount = scratch.path().join("wwc-mount.db");
    std::fs::copy(&fixture_main, &work_main).unwrap();
    std::fs::copy(&fixture_mount, &work_mount).unwrap();
    let pepper = &base.test_pepper_base64;
    let main = Writer::open_writable(&work_main, pepper).expect("open main");
    let mount = Writer::open_writable(&work_mount, pepper).expect("open mount");
    let db = Db::open(
        DbPaths {
            main: work_main.clone(),
            mount_index: Some(work_mount.clone()),
            llm_logs: None,
        },
        pepper,
    )
    .expect("open db");
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();

    let who = |w: Option<&str>| -> String {
        if w == Some("recipient") {
            base.recipient_character_id.clone()
        } else {
            base.caller_character_id.clone()
        }
    };

    let mut failed: Vec<String> = Vec::new();
    let (mut credited_ops, mut route_ops, mut apply_ops, mut lines_compared) = (0, 0, 0, 0);
    for (op, want) in spec.ops.iter().zip(&oracle) {
        assert_eq!(want["name"], json!(op.name), "oracle row order");
        let ledger_before = ledger(main.connection());

        let (result, lines) = match op.kind.as_str() {
            "commit" => {
                let next = Slots::from_value(op.next_slots.as_ref());
                let bundles: Vec<WornBundle> = op
                    .worn_bundles
                    .iter()
                    .map(|b| WornBundle {
                        id: b.id.clone(),
                        leaf_ids: b.leaf_ids.clone(),
                    })
                    .collect();
                let character = who(op.who.as_deref());
                let (out, lines) = captured_with(|| {
                    commit_equipped_outfit(
                        main.connection(),
                        CommitEquippedOutfitInput {
                            chat_id: &op.chat,
                            character_id: &character,
                            next_slots: &next,
                            worn_bundles: &bundles,
                            source: source_of(op.source.as_deref().unwrap()),
                            at: op.at.clone(),
                        },
                    )
                });
                let result = match out {
                    Ok(r) => json!({
                        "slots": r.slots.to_value(),
                        "newlyWornLeafIds": r.newly_worn_leaf_ids,
                        "creditedBundleIds": r.credited_bundle_ids,
                        "changed": r.changed,
                    }),
                    Err(e) => json!({ "error": e.to_string() }),
                };
                (result, lines)
            }
            "apply" => {
                apply_ops += 1;
                let selections: Vec<OutfitSelection> = op
                    .selections
                    .iter()
                    .map(|s| OutfitSelection {
                        character_id: who(s["who"].as_str()),
                        mode: s["mode"].as_str().unwrap().to_string(),
                        slots: s.get("slots").map(|v| Slots::from_value(Some(v))),
                        worn_bundle_ids: s.get("wornBundleIds").and_then(Value::as_array).map(
                            |ids| {
                                ids.iter()
                                    .filter_map(Value::as_str)
                                    .map(str::to_string)
                                    .collect()
                            },
                        ),
                    })
                    .collect();
                let ctx = OutfitContext {
                    user_id: &base.user_id,
                    project_mount_point_ids: &[],
                    scenario_text: None,
                    cheap_settings: None,
                    source_chat_id: op.source_chat.as_deref(),
                    // v4's `context?.source ?? 'chat-start'`.
                    source: source_of(op.source.as_deref().unwrap_or("chat-start")),
                };
                let provider = CannedCompletionProvider::new();
                let executor = CheapLlmTaskExecutor::new();
                let emitter = CreationProgressEmitter::inert();
                let (applied, lines) = captured_with(|| {
                    rt.block_on(apply_outfit_selections(
                        main.connection(),
                        mount.connection(),
                        &provider,
                        &executor,
                        &op.chat,
                        &selections,
                        &ctx,
                        &emitter,
                    ))
                });
                assert!(applied.is_ok(), "{}: apply never fails", op.name);
                (Value::Null, lines)
            }
            "route" => {
                route_ops += 1;
                let mut body = op.body.clone().unwrap();
                let w = body["who"].as_str().map(str::to_string);
                let obj = body.as_object_mut().unwrap();
                obj.remove("who");
                let mut with_character = serde_json::Map::new();
                with_character.insert("characterId".into(), json!(who(w.as_deref())));
                with_character.extend(obj.clone());
                let (resp, lines) = captured_with(|| {
                    rt.block_on(chat_equip(
                        &db,
                        &base.user_id,
                        &op.chat,
                        Value::Object(with_character),
                    ))
                });
                (status_body(&resp), lines)
            }
            other => panic!("unknown op kind {other}"),
        };

        // ── result ──
        let want_result = &want["result"];
        let result_ok = if op.kind == "commit" && want_result.get("slots").is_some() {
            // v4 returns the passed slots object as given (partial keys); v5's
            // typed `Slots` carries all five — compare through the reader.
            let mut w = want_result.clone();
            w["slots"] = Slots::from_value(Some(&want_result["slots"])).to_value();
            w == result
        } else {
            *want_result == result
        };
        if !result_ok {
            eprintln!(
                "[{}] RESULT\n   v5: {result}\n   v4: {want_result}",
                op.name
            );
            failed.push(format!("{}:result", op.name));
        }

        // ── the op chat's equippedOutfit ──
        let got_outfit = ChatOutfitsRepository::new(main.connection())
            .get_equipped_outfit(&op.chat)
            .unwrap_or(Value::Null);
        if got_outfit != want["equippedOutfit"] {
            eprintln!(
                "[{}] OUTFIT\n   v5: {got_outfit}\n   v4: {}",
                op.name, want["equippedOutfit"]
            );
            failed.push(format!("{}:equippedOutfit", op.name));
        }

        // ── the whole ledger ──
        let got_ledger = normalize_ledger(ledger(main.connection()), &pinned);
        let want_ledger = normalize_ledger(want["ledger"].clone(), &pinned);
        if got_ledger != want_ledger {
            eprintln!(
                "[{}] LEDGER\n   v5: {got_ledger}\n   v4: {want_ledger}",
                op.name
            );
            failed.push(format!("{}:ledger", op.name));
        }
        if normalize_ledger(ledger_before, &pinned) != got_ledger {
            credited_ops += 1;
        }

        // ── the log lines ──
        let comparable = |m: &str| op.kind != "route" || ROUTE_LEVEL.contains(&m);
        let mut got_lines: Vec<_> = lines.iter().filter_map(|l| parse_line(l)).collect();
        got_lines.retain(|(_, m, _)| comparable(m));
        let mut want_lines: Vec<_> = want["logs"]
            .as_array()
            .unwrap()
            .iter()
            .map(oracle_line)
            .filter(|(_, m, _)| comparable(m))
            .collect();
        got_lines.sort();
        want_lines.sort();
        lines_compared += want_lines.len();
        if got_lines != want_lines {
            eprintln!(
                "[{}] LOGS\n   v5: {got_lines:#?}\n   v4: {want_lines:#?}",
                op.name
            );
            failed.push(format!("{}:logs", op.name));
        }
        if failed
            .iter()
            .all(|f| !f.starts_with(&format!("{}:", op.name)))
        {
            eprintln!("[{}] OK.", op.name);
        }
    }

    assert!(failed.is_empty(), "wardrobe-wear-commit FAILED: {failed:?}");
    // Shape assertions — the corpus must exercise every kind and credit.
    assert!(apply_ops >= 7, "apply ops: {apply_ops}");
    assert!(route_ops >= 10, "route ops: {route_ops}");
    assert!(
        credited_ops >= 12,
        "ops that moved the ledger: {credited_ops}"
    );
    assert!(lines_compared >= 24, "log lines compared: {lines_compared}");
    eprintln!(
        "OK: wardrobe-wear-commit matched oracle ({} ops; {credited_ops} moved the ledger; \
         {lines_compared} log lines).",
        spec.ops.len()
    );
}
