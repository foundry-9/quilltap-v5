//! Persisted outfit changes — load, compute, commit (v4
//! `lib/wardrobe/outfit-displacement.ts`, `9ecebf986`).
//!
//! Every gesture that changes what a character has on in a chat comes through
//! [`apply_displacement`]: it loads the character's current slots, runs the
//! pure gesture from [`crate::slot_ops`] ([`compute_displaced_slots`], the one
//! mode → primitive table), and commits through
//! [`commit_equipped_outfit`] — the wear ledger's chokepoint — telling it
//! which composite (if any) the gesture dissolved, since the stored leaves
//! alone cannot say an outfit was worn.
//!
//! The item lookup that lets a composite dissolve comes from the caller's
//! wearable pool (`pool.byId`); without one a composite is stored whole, which
//! still renders correctly via read-time expansion.
//!
//! Server-only. The pure math lives in `slot_ops` for the client. The four
//! persisted primitives this replaced (`equip_item` / `replace_item` /
//! `add_to_slot` / `remove_from_slot` in `tools::wardrobe_shared`) survive as
//! LANE-REMOVE shims over this one function.
//!
//! Every function takes a raw `main: &Connection` so the callers run inside a
//! single writer closure (the `wardrobe_transfers` precedent), or, in the
//! differential, against a directly-opened `Writer`. Both log lines are logged
//! on the CALLER's thread.

use rusqlite::Connection;

use crate::db::chats_outfits::ChatOutfitsRepository;
use crate::db::wardrobe_wear_stats::EquipSource;
use crate::db::DbError;
use crate::services::wardrobe_wear_commit::{commit_equipped_outfit, CommitEquippedOutfitInput};
use crate::slot_ops::{
    analyze_composite, compute_displaced_slots, worn_bundles_for_mode, ComputeDisplacedOptions,
};
use crate::wardrobe::Slots;

/// v4 `loadSlots` — the character's current slots in the chat, or a fresh
/// empty outfit when they have none (`current ? cloneEquippedSlots(current) :
/// makeEmptyEquippedSlots()`; [`Slots::from_value`] IS that pair).
fn load_slots(main: &Connection, chat_id: &str, character_id: &str) -> Slots {
    let stored =
        ChatOutfitsRepository::new(main).get_equipped_outfit_for_character(chat_id, character_id);
    Slots::from_value(stored.as_ref())
}

/// v4 `logCompositeIssues` — log a composite that could not dissolve cleanly;
/// the client-safe math can't.
fn log_composite_issues(options: &ComputeDisplacedOptions<'_>, character_id: &str) {
    let (Some(item), Some(items_by_id)) = (options.item, options.items_by_id) else {
        return;
    };
    let analysis = analyze_composite(item, Some(items_by_id));
    if analysis.cycles > 0 || analysis.truncated {
        tracing::warn!(
            context = "wardrobe",
            characterId = character_id,
            itemId = item.id.as_str(),
            cycles = analysis.cycles,
            truncated = analysis.truncated,
            "[applyDisplacement] Malformed composite graph; expansion truncated"
        );
    }
    if analysis.unresolved {
        tracing::warn!(
            context = "wardrobe",
            characterId = character_id,
            itemId = item.id.as_str(),
            componentCount = item.component_item_ids.len(),
            "[applyDisplacement] Composite resolved to no wearable parts; wearing it whole"
        );
    }
}

/// v4 `applyDisplacement(repos, chatId, characterId, options, source = 'ui')`
/// — apply one gesture to a character's slots in a chat and commit it.
/// Returns the slots it computed (v4: from the job child the chokepoint's
/// write is buffered, so its echo would be synthetic; v5 keeps the shape).
///
/// The take-off modes (`remove_from_slot`, `clear_slot`) always commit as
/// [`EquipSource::TakeOff`]; the put-on modes use `source` — v4 types it
/// `Extract<EquipSource, 'ui' | 'tool'>`, which is what every caller passes.
///
/// A failed commit (a lost slot write, or a credit that could not be written)
/// is the chokepoint's error as a bare-message `DbError::Internal`, the
/// sentence v4's callers see.
pub fn apply_displacement(
    main: &Connection,
    chat_id: &str,
    character_id: &str,
    options: &ComputeDisplacedOptions<'_>,
    source: EquipSource,
) -> Result<Slots, DbError> {
    let current = load_slots(main, chat_id, character_id);
    log_composite_issues(options, character_id);
    let next_slots = compute_displaced_slots(&current, options);
    let effective = if options.mode.is_take_off() {
        EquipSource::TakeOff
    } else {
        source
    };
    let worn_bundles = worn_bundles_for_mode(options);
    commit_equipped_outfit(
        main,
        CommitEquippedOutfitInput {
            chat_id,
            character_id,
            next_slots: &next_slots,
            worn_bundles: &worn_bundles,
            source: effective,
            at: None,
        },
    )?;
    // v4's `options.item?.id ?? options.itemId ?? null` / `options.slot ??
    // null` — the absent value renders as the literal `null`, as the JSON
    // logger prints it.
    let item_id = options
        .item
        .map(|item| item.id.as_str())
        .or(options.item_id)
        .unwrap_or("null");
    tracing::debug!(
        context = "wardrobe",
        chatId = chat_id,
        characterId = character_id,
        mode = options.mode.as_str(),
        itemId = item_id,
        slot = options.slot.unwrap_or("null"),
        source = effective.as_str(),
        "[applyDisplacement] Outfit change committed"
    );
    Ok(next_slots)
}

#[cfg(test)]
mod tests {
    //! v4's `slot-ops.test.ts` `applyDisplacement (persisted)` block, over a
    //! real in-memory `chats` + `wardrobe_wear_stats` pair, plus the two WARN
    //! lines capture-pinned with their silence legs.
    use super::*;
    use crate::db::chats_outfits::ChatOutfitsRepository;
    use crate::slot_ops::{DisplacementMode, WearableLookup, WearableNode};
    use crate::test_support::captured_with;
    use serde_json::{json, Value};

    const CHAT: &str = "c0000001-0000-4000-8000-000000000001";
    const CHAR: &str = "c1c1c1c1-0000-0000-0000-000000000001";

    fn make_item(id: &str, types: &[&str], component_item_ids: &[&str]) -> Value {
        json!({ "id": id, "title": id, "types": types, "componentItemIds": component_item_ids })
    }

    fn wardrobe() -> (WearableNode, WearableNode, WearableLookup) {
        let items = [
            make_item("shirt", &["top"], &[]),
            make_item("trousers", &["bottom"], &[]),
            make_item("boots", &["footwear"], &[]),
            make_item("gloves", &["accessories"], &[]),
            make_item(
                "man-in-black",
                &["top", "bottom", "footwear", "accessories"],
                &["shirt", "trousers", "boots", "gloves"],
            ),
        ];
        let map: WearableLookup = items
            .iter()
            .map(|i| (i["id"].as_str().unwrap().to_string(), i.clone()))
            .collect();
        (
            WearableNode::from_value(&items[0]),
            WearableNode::from_value(&items[4]),
            map,
        )
    }

    /// A main partition carrying the CURRENT `chats` DDL (from
    /// `fresh_schema.json`), the wear ledger in v4's migration shape, and one
    /// chat with `equippedOutfit` as given (the `wardrobe_wear_commit` rig).
    fn rig(initial: Option<Value>) -> Connection {
        let schema: Value =
            serde_json::from_str(include_str!("provisioning/fresh_schema.json")).unwrap();
        let conn = Connection::open_in_memory().unwrap();
        let ddl = schema["main"]
            .as_array()
            .unwrap()
            .iter()
            .filter_map(Value::as_str)
            .find(|s| s.starts_with("CREATE TABLE \"chats\" ("))
            .unwrap();
        conn.execute_batch(ddl).unwrap();
        for sql in crate::db::wardrobe_wear_stats::WARDROBE_WEAR_STATS_DDL {
            conn.execute_batch(sql).unwrap();
        }
        let outfit = initial.map(|slots| json!({ CHAR: slots }).to_string());
        conn.execute(
            "INSERT INTO chats (id, userId, title, equippedOutfit, createdAt, updatedAt) \
             VALUES (?1, 'u', 't', ?2, '2026-01-01T00:00:00.000Z', '2026-01-01T00:00:00.000Z')",
            rusqlite::params![CHAT, outfit],
        )
        .unwrap();
        conn
    }

    fn stored(conn: &Connection) -> Slots {
        Slots::from_value(
            ChatOutfitsRepository::new(conn)
                .get_equipped_outfit_for_character(CHAT, CHAR)
                .as_ref(),
        )
    }

    fn ledger(conn: &Connection) -> Vec<(String, i64)> {
        let mut st = conn
            .prepare(r#"SELECT "itemId", "wearCount" FROM "wardrobe_wear_stats" ORDER BY "itemId""#)
            .unwrap();
        st.query_map([], |r| Ok((r.get(0)?, r.get(1)?)))
            .unwrap()
            .map(Result::unwrap)
            .collect()
    }

    fn opts<'a>(
        mode: DisplacementMode,
        item: Option<&'a WearableNode>,
        slot: Option<&'a str>,
        item_id: Option<&'a str>,
        items_by_id: Option<&'a WearableLookup>,
    ) -> ComputeDisplacedOptions<'a> {
        ComputeDisplacedOptions {
            mode,
            item,
            slot,
            item_id,
            items_by_id,
        }
    }

    fn ids(list: &[&str]) -> Vec<String> {
        list.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn dissolves_a_composite_through_the_pool_lookup_and_credits_it_as_worn() {
        let (_, mib, map) = wardrobe();
        let conn = rig(None);
        let (result, lines) = captured_with(|| {
            apply_displacement(
                &conn,
                CHAT,
                CHAR,
                &opts(DisplacementMode::Wear, Some(&mib), None, None, Some(&map)),
                EquipSource::Tool,
            )
        });
        let result = result.unwrap();
        assert_eq!(result.top, ids(&["shirt"]));
        assert_eq!(result.bottom, ids(&["trousers"]));
        assert_eq!(result.footwear, ids(&["boots"]));
        assert_eq!(result.accessories, ids(&["gloves"]));
        assert!(result.hair.is_empty());
        assert_eq!(stored(&conn), result);
        // The four leaves AND the composite each earn one wear.
        assert_eq!(
            ledger(&conn),
            vec![
                ("boots".to_string(), 1),
                ("gloves".to_string(), 1),
                ("man-in-black".to_string(), 1),
                ("shirt".to_string(), 1),
                ("trousers".to_string(), 1),
            ]
        );
        let committed = lines
            .iter()
            .find(|l| l.contains("[applyDisplacement] Outfit change committed"))
            .expect("the committed line");
        assert_eq!(
            committed,
            &format!(
                "DEBUG quilltap_core::services::outfit_displacement [applyDisplacement] Outfit change committed \
                 context=wardrobe chatId={CHAT} characterId={CHAR} mode=wear itemId=man-in-black slot=null source=tool"
            )
        );
        // Silence legs: a clean graph logs neither WARN.
        assert!(!lines
            .iter()
            .any(|l| l.contains("Malformed composite graph")));
        assert!(!lines
            .iter()
            .any(|l| l.contains("Composite resolved to no wearable parts")));
    }

    #[test]
    fn claims_no_composite_credit_for_a_plain_garment_and_defaults_to_ui() {
        let (shirt, _, map) = wardrobe();
        let conn = rig(None);
        let (_, lines) = captured_with(|| {
            apply_displacement(
                &conn,
                CHAT,
                CHAR,
                &opts(DisplacementMode::Wear, Some(&shirt), None, None, Some(&map)),
                EquipSource::Ui,
            )
            .unwrap()
        });
        assert_eq!(ledger(&conn), vec![("shirt".to_string(), 1)]);
        assert!(lines
            .iter()
            .any(|l| l.ends_with("mode=wear itemId=shirt slot=null source=ui")));
    }

    #[test]
    fn stores_the_composite_whole_when_no_lookup_is_supplied() {
        let (_, mib, _) = wardrobe();
        let conn = rig(None);
        let (result, lines) = captured_with(|| {
            apply_displacement(
                &conn,
                CHAT,
                CHAR,
                &opts(DisplacementMode::Wear, Some(&mib), None, None, None),
                EquipSource::Ui,
            )
            .unwrap()
        });
        assert_eq!(result.top, ids(&["man-in-black"]));
        // No lookup ⇒ `logCompositeIssues` returns before analyzing: silent.
        assert!(!lines.iter().any(|l| l.contains("WARN")));
    }

    #[test]
    fn commits_the_take_off_modes_as_take_off_whatever_source_is_passed() {
        for mode in [
            DisplacementMode::RemoveFromSlot,
            DisplacementMode::ClearSlot,
        ] {
            let conn = rig(Some(json!({ "top": ["vest", "shirt"] })));
            let (_, lines) = captured_with(|| {
                apply_displacement(
                    &conn,
                    CHAT,
                    CHAR,
                    &opts(mode, None, Some("top"), Some("shirt"), None),
                    EquipSource::Tool,
                )
                .unwrap()
            });
            let want_top = if mode == DisplacementMode::ClearSlot {
                Vec::new()
            } else {
                ids(&["vest"])
            };
            assert_eq!(stored(&conn).top, want_top);
            assert!(ledger(&conn).is_empty(), "a take-off credits nothing");
            let line = lines
                .iter()
                .find(|l| l.contains("Outfit change committed"))
                .unwrap();
            assert!(
                line.ends_with(&format!(
                    "mode={} itemId=shirt slot=top source=take-off",
                    mode.as_str()
                )),
                "{line}"
            );
        }
    }

    #[test]
    fn starts_from_empty_slots_when_the_character_has_none_in_the_chat() {
        let (shirt, _, _) = wardrobe();
        let conn = rig(None);
        let result = apply_displacement(
            &conn,
            CHAT,
            CHAR,
            &opts(
                DisplacementMode::AddToSlot,
                Some(&shirt),
                Some("top"),
                None,
                None,
            ),
            EquipSource::Ui,
        )
        .unwrap();
        assert_eq!(result.top, ids(&["shirt"]));
        assert!(result.bottom.is_empty() && result.footwear.is_empty());
    }

    #[test]
    fn a_missing_item_logs_null_and_commits_the_unchanged_slots() {
        let conn = rig(Some(json!({ "top": ["vest"] })));
        let (result, lines) = captured_with(|| {
            apply_displacement(
                &conn,
                CHAT,
                CHAR,
                &opts(DisplacementMode::Wear, None, None, None, None),
                EquipSource::Ui,
            )
            .unwrap()
        });
        assert_eq!(result.top, ids(&["vest"]));
        assert!(lines
            .iter()
            .any(|l| l.ends_with("mode=wear itemId=null slot=null source=ui")));
    }

    #[test]
    fn logs_a_malformed_graph_and_an_unresolved_composite() {
        let (_, _, mut map) = wardrobe();
        let yin = make_item("yin", &["top"], &["yang", "shirt"]);
        let yang = make_item("yang", &["bottom"], &["yin", "trousers"]);
        map.insert("yin".into(), yin.clone());
        map.insert("yang".into(), yang);
        let orphan = make_item("orphan", &["top"], &["nowhere-1", "nowhere-2"]);
        map.insert("orphan".into(), orphan.clone());

        let conn = rig(None);
        let yin = WearableNode::from_value(&yin);
        let (_, lines) = captured_with(|| {
            apply_displacement(
                &conn,
                CHAT,
                CHAR,
                &opts(DisplacementMode::Wear, Some(&yin), None, None, Some(&map)),
                EquipSource::Ui,
            )
            .unwrap()
        });
        assert!(
            lines.iter().any(|l| l
                == &format!(
                    "WARN quilltap_core::services::outfit_displacement [applyDisplacement] Malformed composite graph; expansion truncated \
                     context=wardrobe characterId={CHAR} itemId=yin cycles=1 truncated=false"
                )),
            "{lines:#?}"
        );
        assert!(!lines
            .iter()
            .any(|l| l.contains("Composite resolved to no wearable parts")));

        let orphan = WearableNode::from_value(&orphan);
        let (result, lines) = captured_with(|| {
            apply_displacement(
                &conn,
                CHAT,
                CHAR,
                &opts(
                    DisplacementMode::Wear,
                    Some(&orphan),
                    None,
                    None,
                    Some(&map),
                ),
                EquipSource::Ui,
            )
            .unwrap()
        });
        assert!(result.top.contains(&"orphan".to_string()), "worn whole");
        assert!(
            lines.iter().any(|l| l
                == &format!(
                    "WARN quilltap_core::services::outfit_displacement [applyDisplacement] Composite resolved to no wearable parts; wearing it whole \
                     context=wardrobe characterId={CHAR} itemId=orphan componentCount=2"
                )),
            "{lines:#?}"
        );
        assert!(!lines
            .iter()
            .any(|l| l.contains("Malformed composite graph")));
    }
}
