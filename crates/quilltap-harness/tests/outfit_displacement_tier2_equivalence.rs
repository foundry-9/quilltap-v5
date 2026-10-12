//! Tier-2 differential (P4.D279 keystone, C1 §6): v4's REAL `applyDisplacement`
//! (`lib/wardrobe/outfit-displacement.ts`, `9ecebf986`) vs
//! [`quilltap_core::services::outfit_displacement::apply_displacement`] — the
//! ONE persisted outfit-change primitive: load the character's slots, the pure
//! mode → primitive table (`slot_ops`), the wear ledger's chokepoint, the
//! `[applyDisplacement]` log lines.
//!
//! Both sides run the SAME op sequence (`fixtures/outfit-displacement-
//! tier2.json`) on ONE copy of the seed fixture: a leaf worn / re-worn /
//! layered, `add_to_slot` with a composite that covers the slot and one that
//! does not (the composite's own id lands and is credited as a leaf),
//! `replace`, the replacing composite through `wear` (`item.replace`), a
//! composite with no lookup (stored whole), the missing-item / missing-slot
//! fallbacks (a committed no-op copy logging `itemId=null` / `slot=null`),
//! `remove_from_slot` with and without an id and `clear_slot` (always
//! `take-off`), a re-wear after a take-off (a second wear), an unresolvable
//! composite (the `Composite resolved to no wearable parts` WARN), a cyclic
//! lookup and a depth-truncated chain (the `Malformed composite graph` WARN
//! with `cycles` / `truncated`), and a wear into a MISSING chat (v4's no-op
//! `update` still credits).
//!
//! Comparands per op: the returned slots (or the thrown message), the op
//! chat's `equippedOutfit`, EVERY `wardrobe_wear_stats` row minus the minted
//! `id` / `createdAt` / `updatedAt` (every stamp is `now` on both sides —
//! `applyDisplacement` pins no `at` — so each collapses to `<now>`), and the
//! log lines (message + every field, v4's camelCase keys). Every line runs on
//! the test thread, so all of them compare — the chokepoint's `Committed
//! equipped outfit` included.
//!
//! Generate (Node 24, from the v4 checkout):
//!   N=~/.nvm/versions/node/v24.13.1/bin ; V5W=${V5W:-$HOME/source/quilltap-v5}
//!   cd ~/source/quilltap-server
//!   QT_FIXTURE_OUT=/tmp/qt-outfit-displacement-main.db \
//!     $N/node --import tsx $V5W/harness/oracle/fixtures/build-outfit-displacement-fixture.ts
//!   QT_FIXTURE_OUTFIT_DISPLACEMENT=/tmp/qt-outfit-displacement-main.db \
//!     $N/node --import tsx $V5W/harness/oracle/cases/outfit-displacement-tier2.ts \
//!     > /tmp/oracle-outfit-displacement.ndjson
//! Run:
//!   QT_ORACLE_OUTFIT_DISPLACEMENT=/tmp/oracle-outfit-displacement.ndjson \
//!   QT_FIXTURE_OUTFIT_DISPLACEMENT=/tmp/qt-outfit-displacement-main.db \
//!     cargo test -p quilltap-harness --test outfit_displacement_tier2_equivalence -- --nocapture

use std::collections::BTreeMap;

use quilltap_core::db::chats_outfits::ChatOutfitsRepository;
use quilltap_core::db::wardrobe_wear_stats::EquipSource;
use quilltap_core::db::Writer;
use quilltap_core::services::outfit_displacement::apply_displacement;
use quilltap_core::slot_ops::{
    ComputeDisplacedOptions, DisplacementMode, WearableLookup, WearableNode,
};
use quilltap_core::test_support::captured_with;
use serde::Deserialize;
use serde_json::{json, Value};

#[derive(Deserialize)]
struct Spec {
    #[serde(rename = "testPepperBase64")]
    test_pepper_base64: String,
    #[serde(rename = "characterId")]
    character_id: String,
    items: Vec<Value>,
    ops: Vec<Op>,
}

#[derive(Deserialize)]
struct Op {
    name: String,
    chat: String,
    mode: String,
    #[serde(default)]
    item: Option<String>,
    #[serde(default)]
    slot: Option<String>,
    #[serde(default, rename = "itemId")]
    item_id: Option<String>,
    /// `"all"` / a list / null.
    lookup: Value,
    source: String,
}

/// The messages this family compares (the oracle captures the same set).
const CAPTURED: [&str; 5] = [
    "[applyDisplacement] Malformed composite graph; expansion truncated",
    "[applyDisplacement] Composite resolved to no wearable parts; wearing it whole",
    "[applyDisplacement] Outfit change committed",
    "Committed equipped outfit",
    "Equipped outfit write failed; no wears credited",
];

/// A captured line → `(level, message, fields)`. The rig renders
/// `LEVEL target message k=v k=v…`; the message is one of [`CAPTURED`].
fn parse_line(line: &str) -> Option<(String, String, BTreeMap<String, String>)> {
    let (level, rest) = line.split_once(' ')?;
    let (_target, rest) = rest.split_once(' ')?;
    let message = CAPTURED.iter().find(|m| rest.starts_with(**m))?;
    let tail = rest[message.len()..].trim_start();
    let mut fields = BTreeMap::new();
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

/// v4's captured line → the same shape (field values as the rig renders them;
/// a JSON `null` renders as the literal `null`, as v5 logs it).
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

/// Every wear is stamped `now` on both sides: collapse the stamps.
fn normalize_ledger(rows: Value) -> Value {
    let Value::Array(rows) = rows else {
        return rows;
    };
    Value::Array(
        rows.into_iter()
            .map(|mut row| {
                for k in ["firstWornAt", "lastWornAt"] {
                    if row.get(k).is_some_and(Value::is_string) {
                        row[k] = json!("<now>");
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

#[test]
fn outfit_displacement_matches_oracle() {
    let (Ok(oracle_path), Ok(fixture)) = (
        std::env::var("QT_ORACLE_OUTFIT_DISPLACEMENT"),
        std::env::var("QT_FIXTURE_OUTFIT_DISPLACEMENT"),
    ) else {
        eprintln!(
            "SKIP: set QT_ORACLE_OUTFIT_DISPLACEMENT + QT_FIXTURE_OUTFIT_DISPLACEMENT (see header)."
        );
        return;
    };
    let fixtures =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../harness/oracle/fixtures");
    let spec: Spec = serde_json::from_str(
        &std::fs::read_to_string(fixtures.join("outfit-displacement-tier2.json")).unwrap(),
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

    let scratch = tempfile::tempdir().unwrap();
    let work = scratch.path().join("outfit-displacement-main.db");
    std::fs::copy(&fixture, &work).unwrap();
    let main = Writer::open_writable(&work, &spec.test_pepper_base64).expect("open main");

    let universe: WearableLookup = spec
        .items
        .iter()
        .map(|i| (i["id"].as_str().unwrap().to_string(), i.clone()))
        .collect();
    let lookup_of = |lookup: &Value| -> Option<WearableLookup> {
        match lookup {
            Value::Null => None,
            Value::String(s) if s == "all" => Some(universe.clone()),
            Value::Array(ids) => Some(
                ids.iter()
                    .map(|id| {
                        let id = id.as_str().unwrap();
                        (id.to_string(), universe[id].clone())
                    })
                    .collect(),
            ),
            other => panic!("bad lookup {other}"),
        }
    };

    let mut failed: Vec<String> = Vec::new();
    let (mut credited_ops, mut lines_compared, mut warn_lines, mut take_offs, mut fallbacks) =
        (0, 0, 0, 0, 0);
    for (op, want) in spec.ops.iter().zip(&oracle) {
        assert_eq!(want["name"], json!(op.name), "oracle row order");
        let ledger_before = ledger(main.connection());

        let mode = DisplacementMode::parse(&op.mode)
            .unwrap_or_else(|| panic!("{}: unknown mode {}", op.name, op.mode));
        let map = lookup_of(&op.lookup);
        let node: Option<WearableNode> = op
            .item
            .as_deref()
            .map(|id| WearableNode::from_value(&universe[id]));
        let source = match op.source.as_str() {
            "ui" => EquipSource::Ui,
            "tool" => EquipSource::Tool,
            other => panic!("{}: unknown source {other}", op.name),
        };
        let options = ComputeDisplacedOptions {
            mode,
            item: node.as_ref(),
            slot: op.slot.as_deref(),
            item_id: op.item_id.as_deref(),
            items_by_id: map.as_ref(),
        };
        if mode.is_take_off() {
            take_offs += 1;
        }
        let needs_item = matches!(
            mode,
            DisplacementMode::Wear | DisplacementMode::Replace | DisplacementMode::AddToSlot
        );
        let needs_slot = matches!(
            mode,
            DisplacementMode::AddToSlot
                | DisplacementMode::RemoveFromSlot
                | DisplacementMode::ClearSlot
        );
        if (needs_item && node.is_none()) || (needs_slot && op.slot.is_none()) {
            fallbacks += 1;
        }

        let (out, lines) = captured_with(|| {
            apply_displacement(
                main.connection(),
                &op.chat,
                &spec.character_id,
                &options,
                source,
            )
        });
        let result = match out {
            Ok(slots) => json!({ "slots": slots.to_value() }),
            Err(e) => json!({ "error": e.to_string() }),
        };

        // ── result ──
        let want_result = &want["result"];
        if *want_result != result {
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
        let got_ledger = normalize_ledger(ledger(main.connection()));
        let want_ledger = normalize_ledger(want["ledger"].clone());
        if got_ledger != want_ledger {
            eprintln!(
                "[{}] LEDGER\n   v5: {got_ledger}\n   v4: {want_ledger}",
                op.name
            );
            failed.push(format!("{}:ledger", op.name));
        }
        if normalize_ledger(ledger_before) != got_ledger {
            credited_ops += 1;
        }

        // ── the log lines ──
        let mut got_lines: Vec<_> = lines.iter().filter_map(|l| parse_line(l)).collect();
        let mut want_lines: Vec<_> = want["logs"]
            .as_array()
            .unwrap()
            .iter()
            .map(oracle_line)
            .collect();
        got_lines.sort();
        want_lines.sort();
        lines_compared += want_lines.len();
        warn_lines += want_lines.iter().filter(|(l, _, _)| l == "warn").count();
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

    assert!(failed.is_empty(), "outfit-displacement FAILED: {failed:?}");
    // Shape assertions — the corpus must exercise every arm.
    assert!(take_offs >= 4, "take-off ops: {take_offs}");
    assert!(fallbacks >= 3, "missing item/slot fallbacks: {fallbacks}");
    assert!(
        credited_ops >= 9,
        "ops that moved the ledger: {credited_ops}"
    );
    assert!(warn_lines >= 4, "WARN lines compared: {warn_lines}");
    assert!(lines_compared >= 40, "log lines compared: {lines_compared}");
    // The missing-chat arm must be real: v4 answers `null` for the outfit and
    // still credits.
    let missing = oracle
        .iter()
        .find(|r| r["name"] == "wear_into_missing_chat")
        .expect("the missing-chat row");
    assert!(missing["equippedOutfit"].is_null(), "{missing}");
    eprintln!(
        "OK: outfit-displacement matched oracle ({} ops; {credited_ops} moved the ledger; \
         {lines_compared} log lines, {warn_lines} WARN).",
        spec.ops.len()
    );
}
