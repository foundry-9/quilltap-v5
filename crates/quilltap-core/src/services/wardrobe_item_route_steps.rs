//! The steps every wardrobe item endpoint (character, General, project, group)
//! performs identically around its tier-specific write — v4
//! `lib/wardrobe/item-route-steps.ts` (`3ee3b1342` + `7c8572869`):
//!
//! - DELETE: scrub equipped references to the item from every chat, and drop
//!   its wear-ledger rows, BEFORE the item goes — logging (never failing) when
//!   either clean-up hiccups ([`cleanup_equipped_refs`]); its pictures go once
//!   it is gone (P4.D255's `cleanup_item_images`, called by the routes).
//! - PUT: refuse an `imageFileId` that is not one of the item's own pictures
//!   ([`image_choice_error`]).
//!
//! Composite items that still reference the id in `componentItemIds` are left
//! alone on purpose (the read path tolerates unknown ids); likewise a
//! composite's deletion drops only its OWN ledger rows, never its components'.
//!
//! **Threads.** The routes run their writes on the writer thread, where a log
//! line is invisible to the thread-scoped capture rig (memory note
//! `capture-rig-writer-thread`) — so the clean-up is split: the routes run
//! [`run_cleanup_equipped_refs`] inside the write and hand its outcome to
//! [`log_cleanup_equipped_refs`] on the calling thread. [`cleanup_equipped_refs`]
//! is the two composed (v4's one function).

use rusqlite::Connection;

use crate::db::chats_outfits::ChatOutfitsRepository;
use crate::db::fallback::error_text;
use crate::db::wardrobe_wear_stats::WardrobeWearStatsRepository;
use crate::services::wardrobe_item_images::primitives::{
    assert_item_image_choice, ItemImageChoiceError,
};

/// The `meta` object a route hands v4's `cleanupEquippedRefs` — one variant per
/// caller shape (tracing field names are static, so the shape is the match).
#[derive(Debug, Clone, Copy)]
pub enum ItemRouteMeta<'a> {
    /// The character route: `{ characterId: id, itemId }`.
    Character { character_id: &'a str },
    /// The General (archetype) route: `{ itemId }`.
    Archetype,
    /// The project store factory: `{ projectId: id, itemId, context: 'wardrobe' }`.
    Project { project_id: &'a str },
    /// The group store factory: `{ groupId: id, itemId, context: 'wardrobe' }`.
    Group { group_id: &'a str },
}

/// One clean-up line in each of the four meta shapes; `$rest` are the keys v4
/// spreads AFTER the meta (`itemId` is already in every meta, so `{ ...meta,
/// itemId }` adds nothing). Each line is emitted under its ROUTE's target —
/// in v4 the route's own logger call owns it (the `[Projects v1]` lines the
/// projects family captures sit at `quilltap_core::api::projects`).
macro_rules! step_line {
    ($level:ident, $meta:expr, $item_id:expr, $message:expr; $($rest:tt)*) => {
        match $meta {
            ItemRouteMeta::Character { character_id } => tracing::$level!(
                target: "quilltap_core::api::characters",
                characterId = %character_id, itemId = %$item_id, $($rest)* "{}", $message
            ),
            ItemRouteMeta::Archetype => tracing::$level!(
                target: "quilltap_core::api::wardrobe",
                itemId = %$item_id, $($rest)* "{}", $message
            ),
            ItemRouteMeta::Project { project_id } => tracing::$level!(
                target: "quilltap_core::api::projects",
                projectId = %project_id, itemId = %$item_id, context = "wardrobe",
                $($rest)* "{}", $message
            ),
            ItemRouteMeta::Group { group_id } => tracing::$level!(
                target: "quilltap_core::api::groups",
                groupId = %group_id, itemId = %$item_id, context = "wardrobe",
                $($rest)* "{}", $message
            ),
        }
    };
}

/// What the two clean-up steps answered — each step's error text, or `Ok`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EquippedRefsCleanup {
    /// `removeEquippedItemFromAllChats` — `Err(message)` on a scrub failure.
    pub equipped: Result<(), String>,
    /// `wardrobeWear.deleteByItemIds([itemId])` — `Err(message)` on failure
    /// (a pre-round instance with no ledger table: `no such table:
    /// wardrobe_wear_stats`, v4's throw too).
    pub ledger: Result<(), String>,
}

/// The two steps of v4 `cleanupEquippedRefs`, without its logging: scrub the
/// item from every chat's equipped slots, then drop its wear-ledger rows. Never
/// fails — each step's error is answered, and the delete proceeds regardless.
pub fn run_cleanup_equipped_refs(main: &Connection, item_id: &str) -> EquippedRefsCleanup {
    let equipped = ChatOutfitsRepository::new(main)
        .remove_equipped_item_from_all_chats(item_id)
        .map(|_| ())
        .map_err(|e| error_text(&e));
    let ledger = WardrobeWearStatsRepository::new(main)
        .delete_by_item_ids(&[item_id.to_string()])
        .map_err(|e| error_text(&e));
    EquippedRefsCleanup { equipped, ledger }
}

/// v4 `cleanupEquippedRefs`'s three lines, from [`run_cleanup_equipped_refs`]'s
/// outcome: a scrub failure → WARN `` `${logTag} Cleanup of equipped references
/// had issues, proceeding with delete` `` `{ ...meta, cleanupError }`; the
/// ledger drop → DEBUG `` `${logTag} Dropped wear-ledger rows for deleted item`
/// `` `{ ...meta, itemId }`, or on failure WARN `` `${logTag} Cleanup of
/// wear-ledger rows had issues, proceeding with delete` `` `{ ...meta,
/// ledgerError }`.
pub fn log_cleanup_equipped_refs(
    outcome: &EquippedRefsCleanup,
    item_id: &str,
    log_tag: &str,
    meta: ItemRouteMeta<'_>,
) {
    if let Err(cleanup_error) = &outcome.equipped {
        step_line!(
            warn,
            meta,
            item_id,
            format!("{log_tag} Cleanup of equipped references had issues, proceeding with delete");
            cleanupError = %cleanup_error,
        );
    }
    match &outcome.ledger {
        Ok(()) => step_line!(
            debug,
            meta,
            item_id,
            format!("{log_tag} Dropped wear-ledger rows for deleted item");
        ),
        Err(ledger_error) => step_line!(
            warn,
            meta,
            item_id,
            format!("{log_tag} Cleanup of wear-ledger rows had issues, proceeding with delete");
            ledgerError = %ledger_error,
        ),
    }
}

/// v4 `cleanupEquippedRefs(repos, itemId, logTag, meta)` — both steps and their
/// lines on the calling thread.
pub fn cleanup_equipped_refs(
    main: &Connection,
    item_id: &str,
    log_tag: &str,
    meta: ItemRouteMeta<'_>,
) -> EquippedRefsCleanup {
    let outcome = run_cleanup_equipped_refs(main, item_id);
    log_cleanup_equipped_refs(&outcome, item_id, log_tag, meta);
    outcome
}

/// v4 `imageChoiceError(repos, itemId, imageFileId)` — the 400 message for a
/// PUT whose `imageFileId` names a file that is not one of the item's own
/// pictures, or `Ok(None)` when the choice is fine (or absent / `null` —
/// clearing the picture always passes). A refusal logs INFO
/// `[WardrobeItem] Refused an imageFileId that is not the item's own
/// { itemId, imageFileId }`. Any other failure (the history read) is v4's
/// rethrow — `Err(message)`, which the route answers through its own error arm.
pub fn image_choice_error(
    main: &Connection,
    item_id: &str,
    image_file_id: Option<&str>,
) -> Result<Option<String>, String> {
    match assert_item_image_choice(main, item_id, image_file_id) {
        Ok(()) => Ok(None),
        Err(e @ ItemImageChoiceError::Foreign { .. }) => {
            tracing::info!(
                itemId = %item_id,
                imageFileId = %image_file_id.unwrap_or_default(),
                "[WardrobeItem] Refused an imageFileId that is not the item's own"
            );
            Ok(Some(e.to_string()))
        }
        Err(ItemImageChoiceError::Read(message)) => Err(message),
    }
}

/// v4 `updateWardrobeSchema`'s `imageFileId: UUIDSchema.nullable().optional()`
/// (`7c8572869`) on an item PUT body: absent → `Ok(None)` (leave the pointer);
/// `null` → `Ok(Some(None))` (clear it); a `z.uuid()`-valid string →
/// `Ok(Some(Some(id)))`; anything else fails the parse (`Err` — the routes
/// answer v4's middleware `Validation error`). Create bodies never carry it
/// (v4's create schema strips it).
pub(crate) fn parse_image_file_id(body: &serde_json::Value) -> Result<Option<Option<String>>, ()> {
    match body.get("imageFileId") {
        None => Ok(None),
        Some(serde_json::Value::Null) => Ok(Some(None)),
        Some(serde_json::Value::String(s)) if crate::api::zod_issues::zod_uuid_ok(s) => {
            Ok(Some(Some(s.clone())))
        }
        Some(_) => Err(()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::captured_with;

    const ITEM: &str = "e9000000-0000-4000-8000-000000000001";

    /// `chats` (the scrub's table) + `files` (the picture read) in memory; the
    /// ledger table only when `ledger`.
    fn main_db(ledger: bool) -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        // The real `chats` + `files` tables, from the D23 dump.
        let fresh: serde_json::Value =
            serde_json::from_str(include_str!("provisioning/fresh_schema.json")).unwrap();
        for ddl in fresh["main"]
            .as_array()
            .unwrap()
            .iter()
            .filter_map(|d| d.as_str())
        {
            if ddl.starts_with("CREATE TABLE \"chats\" (")
                || ddl.starts_with("CREATE TABLE \"files\" (")
            {
                conn.execute_batch(ddl).unwrap();
            }
        }
        if ledger {
            crate::test_support::ensure_wear_ledger_on(&conn);
            conn.execute(
                r#"INSERT INTO "wardrobe_wear_stats" ("id", "itemId", "wearerCharacterId",
                     "wearCount", "firstWornAt", "lastWornAt", "lastWornChatId", "createdAt",
                     "updatedAt") VALUES ('w1', ?1, NULL, 2, 't', 't', NULL, 't', 't')"#,
                [ITEM],
            )
            .unwrap();
        }
        conn
    }

    fn tagged(lines: &[String], needle: &str) -> Vec<String> {
        lines
            .iter()
            .filter(|l| l.contains(needle))
            .map(|l| l.splitn(3, ' ').nth(2).unwrap_or_default().to_string())
            .collect()
    }

    #[test]
    fn the_ledger_rows_go_with_the_item_and_the_debug_line_carries_the_meta() {
        let main = main_db(true);
        for (meta, want) in [
            (
                ItemRouteMeta::Character { character_id: "c1" },
                format!("[Wardrobe v1] Dropped wear-ledger rows for deleted item characterId=c1 itemId={ITEM}"),
            ),
            (
                ItemRouteMeta::Archetype,
                format!("[Wardrobe Archetypes v1] Dropped wear-ledger rows for deleted item itemId={ITEM}"),
            ),
            (
                ItemRouteMeta::Project { project_id: "p1" },
                format!("[Projects v1] Dropped wear-ledger rows for deleted item projectId=p1 itemId={ITEM} context=wardrobe"),
            ),
            (
                ItemRouteMeta::Group { group_id: "g1" },
                format!("[Groups v1] Dropped wear-ledger rows for deleted item groupId=g1 itemId={ITEM} context=wardrobe"),
            ),
        ] {
            let tag = want.split(" Dropped").next().unwrap().to_string();
            let (outcome, lines) =
                captured_with(|| cleanup_equipped_refs(&main, ITEM, &tag, meta));
            assert_eq!(outcome.ledger, Ok(()));
            assert_eq!(tagged(&lines, "Dropped wear-ledger rows"), vec![want], "{lines:#?}");
            // Silence legs: neither WARN fires on success.
            assert!(!lines.iter().any(|l| l.contains("had issues")), "{lines:#?}");
        }
        let left: i64 = main
            .query_row(r#"SELECT COUNT(*) FROM "wardrobe_wear_stats""#, [], |r| {
                r.get(0)
            })
            .unwrap();
        assert_eq!(left, 0, "the item's ledger rows are gone");
    }

    #[test]
    fn a_pre_round_instance_warns_and_proceeds() {
        // No ledger table: v4's `deleteByItemIds` throws `no such table` → the
        // WARN with `ledgerError`, and the delete proceeds.
        let main = main_db(false);
        let (outcome, lines) = captured_with(|| {
            cleanup_equipped_refs(
                &main,
                ITEM,
                "[Groups v1]",
                ItemRouteMeta::Group { group_id: "g1" },
            )
        });
        assert_eq!(outcome.equipped, Ok(()));
        assert_eq!(
            tagged(&lines, "Cleanup of wear-ledger rows"),
            vec![format!(
                "[Groups v1] Cleanup of wear-ledger rows had issues, proceeding with delete groupId=g1 itemId={ITEM} context=wardrobe ledgerError=no such table: wardrobe_wear_stats"
            )],
            "{lines:#?}"
        );
        assert!(!lines.iter().any(|l| l.contains("Dropped wear-ledger rows")));
    }

    #[test]
    fn a_failed_scrub_warns_and_proceeds() {
        // R-A: the character route's scrub failure is warn-and-proceed too.
        let main = main_db(true);
        main.execute_batch(r#"DROP TABLE "chats""#).unwrap();
        let (outcome, lines) = captured_with(|| {
            cleanup_equipped_refs(
                &main,
                ITEM,
                "[Wardrobe v1]",
                ItemRouteMeta::Character { character_id: "c1" },
            )
        });
        assert!(outcome.equipped.is_err());
        assert_eq!(outcome.ledger, Ok(()), "the ledger step still runs");
        let warn = tagged(&lines, "Cleanup of equipped references");
        assert_eq!(warn.len(), 1, "{lines:#?}");
        assert!(
            warn[0].starts_with(&format!(
                "[Wardrobe v1] Cleanup of equipped references had issues, proceeding with delete characterId=c1 itemId={ITEM} cleanupError="
            )),
            "{warn:?}"
        );
    }

    #[test]
    fn image_choice_refuses_a_foreign_file_and_passes_null() {
        let main = main_db(false);
        let (r, lines) = captured_with(|| image_choice_error(&main, ITEM, None));
        assert_eq!(r, Ok(None));
        assert!(lines.is_empty());
        let foreign = "f9000000-0000-4000-8000-000000000009";
        let (r, lines) = captured_with(|| image_choice_error(&main, ITEM, Some(foreign)));
        assert_eq!(
            r,
            Ok(Some(
                "imageFileId must name one of this item's own pictures".to_string()
            ))
        );
        assert_eq!(
            tagged(&lines, "Refused an imageFileId"),
            vec![format!(
                "[WardrobeItem] Refused an imageFileId that is not the item's own itemId={ITEM} imageFileId={foreign}"
            )]
        );
    }

    #[test]
    fn image_file_id_parses_as_v4s_nullable_optional_uuid() {
        use serde_json::json;
        assert_eq!(parse_image_file_id(&json!({})), Ok(None));
        assert_eq!(
            parse_image_file_id(&json!({ "imageFileId": null })),
            Ok(Some(None))
        );
        let id = "f9000000-0000-4000-8000-000000000009";
        assert_eq!(
            parse_image_file_id(&json!({ "imageFileId": id })),
            Ok(Some(Some(id.to_string())))
        );
        assert_eq!(
            parse_image_file_id(&json!({ "imageFileId": "not-a-uuid" })),
            Err(())
        );
        assert_eq!(parse_image_file_id(&json!({ "imageFileId": 7 })), Err(()));
        assert_eq!(parse_image_file_id(&json!({ "imageFileId": "" })), Err(()));
    }
}
