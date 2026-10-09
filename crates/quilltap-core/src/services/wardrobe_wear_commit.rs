//! The wear ledger's WRITE chokepoint — v4 `WardrobeWearRepository.
//! commitEquippedOutfit` (`lib/database/repositories/wardrobe-wear.
//! repository.ts:200-262`, `3ee3b1342` "Wardrobe wear ledger (#81)",
//! unchanged through `f5e953a3f`; P4.D262).
//!
//! **The one place a character's equipped slots are written.** Every slot
//! writer — the displacement primitives (`tools::wardrobe_shared`), the
//! dialog's `set_all` (`api::chat_outfits`), the selection paths
//! (`services::outfit_selections`, the added-participant arm in
//! `services::chat_participants`, the merge) — passes through
//! [`commit_equipped_outfit`], which reads the prior slots, writes the next
//! ones, and credits the garments the write newly put on. v4 fences this
//! with a jest test sanctioning two files; v5's fence is the
//! `set_equipped_outfit_census_guard` source census (the definition in
//! `db/chats_outfits.rs` and this module, nothing else). Never credit a wear
//! by hand from a handler (v4's warning): the handler's idea of "already
//! worn" is stale.
//!
//! **What a v4 child process needed and v5 does not.** v4 buffers this call
//! whole from the job child (`METHOD_OVERRIDES`, a synthetic return value)
//! and `039f7017c` (bug 179) overlays buffered writes for read-your-writes.
//! v5 has no child process: every wardrobe write runs on the single writer,
//! so the prior read here is always the TRUE state and two ops in one turn
//! compound (`tools/executor.rs`'s `run_wardrobe_wear` / `wardrobe_write`).
//! Nothing of that machinery is ported.

use rusqlite::Connection;

use crate::db::chats_outfits::ChatOutfitsRepository;
use crate::db::wardrobe_wear_stats::{
    diff_equipped_outfit, EquipSource, WardrobeWearIncrement, WardrobeWearStatsRepository,
    WornBundle,
};
use crate::db::DbError;
use crate::wardrobe::Slots;

/// v4 `CommitEquippedOutfitInput` (`:69-78`).
#[derive(Debug, Clone)]
pub struct CommitEquippedOutfitInput<'a> {
    pub chat_id: &'a str,
    pub character_id: &'a str,
    /// The slots to write — stored as given.
    pub next_slots: &'a Slots,
    /// Bundles the caller dissolved into `next_slots`, each with the leaves it
    /// contributed; one earns a wear when any of its leaves is newly worn.
    pub worn_bundles: &'a [WornBundle],
    pub source: EquipSource,
    /// When the wears happened (ISO); now when `None`.
    pub at: Option<String>,
}

/// v4 `CommitEquippedOutfitResult` (`:80-85`).
#[derive(Debug, Clone, PartialEq)]
pub struct CommitEquippedOutfitResult {
    /// The slots as written (v4's `written` — the `next_slots` passed in).
    pub slots: Slots,
    pub newly_worn_leaf_ids: Vec<String>,
    pub credited_bundle_ids: Vec<String>,
    pub changed: bool,
}

/// Why the chokepoint failed.
#[derive(Debug)]
pub enum WearCommitError {
    /// The slot write FAILED (a database error). v4's `setEquippedOutfit`
    /// is a `safeQuery` that resolves `null` on a thrown error, and the
    /// chokepoint throws this sentence; no wear is credited. NOT a missing
    /// chat: v4's `update` is a silent no-op on a missing row and
    /// `setEquippedOutfit` still returns the slots, so that write "lands" (and
    /// credits) — measured against v4's real chokepoint at `f5e953a3f`.
    Unsaved {
        chat_id: String,
        character_id: String,
    },
    /// The slots were written but crediting the wears failed (v4's
    /// `incrementWears` throws through the chokepoint — the whole batch rolled
    /// back, so no partial credit).
    Credit(DbError),
}

impl std::fmt::Display for WearCommitError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            WearCommitError::Unsaved {
                chat_id,
                character_id,
            } => write!(
                f,
                "Failed to save the equipped outfit for character {character_id} in chat {chat_id}"
            ),
            // v4 throws the SQLite error itself: its BARE message.
            WearCommitError::Credit(e) => write!(f, "{}", crate::db::fallback::error_text(e)),
        }
    }
}

impl std::error::Error for WearCommitError {}

/// The primitives and the selection paths are `DbError`-typed; v4's callers
/// see the thrown `Error`'s message, which this keeps byte-for-byte: both arms
/// cross as a bare-message `Internal` (the unsaved sentence; a credit failure
/// as the SQLite error's own text — handing the inner `DbError::Sqlite` back
/// displayed `sqlite error: …`, which v4 never says; the `f5e953a3f`
/// unification's §3 finding).
impl From<WearCommitError> for DbError {
    fn from(e: WearCommitError) -> Self {
        DbError::Internal(e.to_string())
    }
}

/// v4 `commitEquippedOutfit(input)` — write a character's equipped slots in a
/// chat and credit the wears the write represents:
///
/// 1. read the prior slots (`getEquippedOutfitForCharacter`);
/// 2. write `next_slots` — ALWAYS, even when equal to the prior state, so the
///    callers' announcement and avatar hooks behave exactly as before;
/// 3. a FAILED write (the slot writer's `Err` — v4's `safeQuery` fallback
///    `null`) logs v4's WARN `Equipped outfit write failed; no wears
///    credited` `{chatId, characterId, source}` and answers
///    [`WearCommitError::Unsaved`] — so no caller reports a change that was
///    not saved, and nothing is credited. A MISSING chat is not a failed
///    write: the slot writer's `Ok(false)` is v4's no-op `update` whose
///    `setEquippedOutfit` still returns the slots, so the call goes on and
///    credits (v4-faithful; measured);
/// 4. diff the prior slots against `next_slots` with `worn_bundles`
///    ([`diff_equipped_outfit`]);
/// 5. credit the newly worn leaves and the credited bundles — nothing for
///    [`EquipSource::Merge`] (a merge changes nobody's clothes) — in ONE
///    savepoint ([`WardrobeWearStatsRepository::increment_wears`]) at ONE
///    `at`;
/// 6. log v4's DEBUG `Committed equipped outfit` with the counts.
///
/// v4's recorded NON-atomicity, carried: the credits for one call are written
/// in a single transaction, so a call records all of its wears or none. The
/// prior-slot read and the slot write are NOT inside it — the slot write goes
/// through the chats repository's update exactly as the bare
/// `setEquippedOutfit` did before the ledger, and two simultaneous equips of
/// the same character in one chat race on the slots the same way they always
/// have. The ledger cost of that race is at most a credit for a garment the
/// other request then took off. (On v5 every caller is on the single writer,
/// so the race cannot open; the shape is kept as v4's.)
///
/// Both lines are logged on the CALLER's thread (a caller running inside a
/// `Db::write` closure logs on the writer thread, as every writer-side line
/// does).
pub fn commit_equipped_outfit(
    main: &Connection,
    input: CommitEquippedOutfitInput<'_>,
) -> Result<CommitEquippedOutfitResult, WearCommitError> {
    let CommitEquippedOutfitInput {
        chat_id,
        character_id,
        next_slots,
        worn_bundles,
        source,
        at,
    } = input;
    let chats = ChatOutfitsRepository::new(main);

    let prior = chats
        .get_equipped_outfit_for_character(chat_id, character_id)
        .map(|v| Slots::from_value(Some(&v)));
    // `Ok(false)` (no chat row) is v4's silent no-op `update`: still
    // "written". Only a thrown error is v4's `null`.
    let written = chats
        .set_equipped_outfit(chat_id, character_id, &next_slots.to_value())
        .is_ok();
    if !written {
        tracing::warn!(
            target: "quilltap::wardrobe_wear",
            module = "wardrobe-wear",
            chatId = chat_id,
            characterId = character_id,
            source = source.as_str(),
            "Equipped outfit write failed; no wears credited"
        );
        return Err(WearCommitError::Unsaved {
            chat_id: chat_id.to_string(),
            character_id: character_id.to_string(),
        });
    }

    let diff = diff_equipped_outfit(prior.as_ref(), next_slots, worn_bundles);

    let credit: Vec<&String> = if source == EquipSource::Merge {
        Vec::new()
    } else {
        diff.newly_worn_leaf_ids
            .iter()
            .chain(diff.credited_bundle_ids.iter())
            .collect()
    };
    if !credit.is_empty() {
        let at = at.unwrap_or_else(crate::clock::now_iso);
        let entries: Vec<WardrobeWearIncrement> = credit
            .iter()
            .map(|item_id| WardrobeWearIncrement {
                item_id: (*item_id).clone(),
                wearer_character_id: Some(character_id.to_string()),
                chat_id: Some(chat_id.to_string()),
                at: at.clone(),
            })
            .collect();
        WardrobeWearStatsRepository::new(main)
            .increment_wears(&entries)
            .map_err(WearCommitError::Credit)?;
    }

    tracing::debug!(
        target: "quilltap::wardrobe_wear",
        module = "wardrobe-wear",
        chatId = chat_id,
        characterId = character_id,
        source = source.as_str(),
        newlyWorn = diff.newly_worn_leaf_ids.len(),
        creditedBundles = diff.credited_bundle_ids.len(),
        credited = credit.len(),
        changed = diff.changed,
        "Committed equipped outfit"
    );

    Ok(CommitEquippedOutfitResult {
        slots: next_slots.clone(),
        newly_worn_leaf_ids: diff.newly_worn_leaf_ids,
        credited_bundle_ids: diff.credited_bundle_ids,
        changed: diff.changed,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::wardrobe_wear_stats::WARDROBE_WEAR_STATS_DDL;
    use crate::test_support::captured_with;
    use serde_json::Value;

    const CHAT: &str = "c0000000-0000-4000-8000-000000000001";
    const CHAT_2: &str = "c0000000-0000-4000-8000-000000000002";
    const MISSING_CHAT: &str = "c0000000-0000-4000-8000-0000000000ff";
    const ALICE: &str = "a0000000-0000-4000-8000-000000000001";
    const BOB: &str = "a0000000-0000-4000-8000-000000000002";

    /// A main partition carrying the CURRENT `chats` DDL (from
    /// `fresh_schema.json`), the wear ledger in v4's migration shape, and two
    /// chats.
    fn main_db() -> Connection {
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
        for sql in WARDROBE_WEAR_STATS_DDL {
            conn.execute_batch(sql).unwrap();
        }
        for chat in [CHAT, CHAT_2] {
            conn.execute(
                "INSERT INTO chats (id, userId, title, createdAt, updatedAt) \
                 VALUES (?1, 'u', 't', '2026-01-01T00:00:00.000Z', '2026-01-01T00:00:00.000Z')",
                [chat],
            )
            .unwrap();
        }
        conn
    }

    fn top(ids: &[&str]) -> Slots {
        Slots {
            top: ids.iter().map(|s| s.to_string()).collect(),
            ..Slots::default()
        }
    }

    fn commit(
        conn: &Connection,
        chat: &str,
        character: &str,
        next: &Slots,
        bundles: &[WornBundle],
        source: EquipSource,
        at: &str,
    ) -> Result<CommitEquippedOutfitResult, WearCommitError> {
        commit_equipped_outfit(
            conn,
            CommitEquippedOutfitInput {
                chat_id: chat,
                character_id: character,
                next_slots: next,
                worn_bundles: bundles,
                source,
                at: Some(at.to_string()),
            },
        )
    }

    /// `(itemId, wearerCharacterId, wearCount, lastWornChatId)` per row.
    fn ledger(conn: &Connection) -> Vec<(String, Option<String>, i64, Option<String>)> {
        let mut st = conn
            .prepare(
                "SELECT itemId, wearerCharacterId, wearCount, lastWornChatId \
                 FROM wardrobe_wear_stats ORDER BY itemId, wearerCharacterId",
            )
            .unwrap();
        st.query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)))
            .unwrap()
            .map(Result::unwrap)
            .collect()
    }

    fn stored(conn: &Connection, chat: &str, character: &str) -> Option<Slots> {
        ChatOutfitsRepository::new(conn)
            .get_equipped_outfit_for_character(chat, character)
            .map(|v| Slots::from_value(Some(&v)))
    }

    /// v4 spec `:241-248`: a newly worn leaf earns one wear; the slots are
    /// written; the result names it.
    #[test]
    fn a_newly_worn_leaf_is_credited_once() {
        let conn = main_db();
        let out = commit(
            &conn,
            CHAT,
            ALICE,
            &top(&["coat"]),
            &[],
            EquipSource::Ui,
            "2026-02-01T00:00:00.000Z",
        )
        .unwrap();
        assert_eq!(out.newly_worn_leaf_ids, vec!["coat"]);
        assert!(out.credited_bundle_ids.is_empty());
        assert!(out.changed);
        assert_eq!(out.slots, top(&["coat"]));
        assert_eq!(stored(&conn, CHAT, ALICE), Some(top(&["coat"])));
        assert_eq!(
            ledger(&conn),
            vec![("coat".into(), Some(ALICE.into()), 1, Some(CHAT.into()))]
        );
    }

    /// v4 spec `:250-256`: a removal credits nothing; putting it back on later
    /// is a second wear.
    #[test]
    fn a_removal_credits_nothing_and_rewearing_is_a_second_wear() {
        let conn = main_db();
        commit(
            &conn,
            CHAT,
            ALICE,
            &top(&["coat"]),
            &[],
            EquipSource::Ui,
            "2026-02-01T00:00:00.000Z",
        )
        .unwrap();
        let off = commit(
            &conn,
            CHAT,
            ALICE,
            &top(&[]),
            &[],
            EquipSource::TakeOff,
            "2026-02-02T00:00:00.000Z",
        )
        .unwrap();
        assert!(off.newly_worn_leaf_ids.is_empty() && off.changed);
        assert_eq!(ledger(&conn)[0].2, 1);
        commit(
            &conn,
            CHAT,
            ALICE,
            &top(&["coat"]),
            &[],
            EquipSource::Tool,
            "2026-02-03T00:00:00.000Z",
        )
        .unwrap();
        assert_eq!(ledger(&conn)[0].2, 2);
    }

    /// v4 spec `:258-262`: two characters each earn a wear of a shared garment.
    #[test]
    fn two_wearers_each_earn_a_wear() {
        let conn = main_db();
        commit(
            &conn,
            CHAT,
            ALICE,
            &top(&["scarf"]),
            &[],
            EquipSource::Ui,
            "2026-02-01T00:00:00.000Z",
        )
        .unwrap();
        commit(
            &conn,
            CHAT,
            BOB,
            &top(&["scarf"]),
            &[],
            EquipSource::Ui,
            "2026-02-01T00:00:00.000Z",
        )
        .unwrap();
        let rows = ledger(&conn);
        assert_eq!(rows.len(), 2);
        assert!(rows.iter().all(|r| r.2 == 1));
    }

    /// v4 spec `:264-276`: a bundle is credited ONCE when one of its leaves
    /// went on; spec `:278-289`: a bundle whose leaves were all already on
    /// earns nothing, and `changed` is false.
    #[test]
    fn a_bundle_is_credited_once_and_only_on_a_transition() {
        let conn = main_db();
        let suit = [WornBundle {
            id: "suit".into(),
            leaf_ids: vec!["shirt".into(), "slacks".into()],
        }];
        let next = Slots {
            top: vec!["shirt".into()],
            bottom: vec!["slacks".into()],
            ..Slots::default()
        };
        let out = commit(
            &conn,
            CHAT,
            ALICE,
            &next,
            &suit,
            EquipSource::Ui,
            "2026-02-01T00:00:00.000Z",
        )
        .unwrap();
        assert_eq!(out.credited_bundle_ids, vec!["suit"]);
        assert_eq!(ledger(&conn).len(), 3);
        let again = commit(
            &conn,
            CHAT,
            ALICE,
            &next,
            &suit,
            EquipSource::Ui,
            "2026-02-02T00:00:00.000Z",
        )
        .unwrap();
        assert!(again.credited_bundle_ids.is_empty() && again.newly_worn_leaf_ids.is_empty());
        assert!(!again.changed);
        assert!(ledger(&conn).iter().all(|r| r.2 == 1));
    }

    /// v4 spec `:292-298`: a merge writes the slots and credits nothing.
    #[test]
    fn a_merge_writes_slots_and_credits_nothing() {
        let conn = main_db();
        let out = commit(
            &conn,
            CHAT,
            ALICE,
            &top(&["coat"]),
            &[],
            EquipSource::Merge,
            "2026-02-01T00:00:00.000Z",
        )
        .unwrap();
        assert_eq!(out.newly_worn_leaf_ids, vec!["coat"]);
        assert_eq!(stored(&conn, CHAT, ALICE), Some(top(&["coat"])));
        assert!(ledger(&conn).is_empty());
    }

    /// v4 spec `:301-308`: equal slots are STILL written (`changed: false`).
    #[test]
    fn equal_slots_are_still_written() {
        let conn = main_db();
        commit(
            &conn,
            CHAT,
            ALICE,
            &top(&["coat"]),
            &[],
            EquipSource::Ui,
            "2026-02-01T00:00:00.000Z",
        )
        .unwrap();
        // Scribble the stored bag so a skipped write would show.
        conn.execute(
            "UPDATE chats SET equippedOutfit = '{}' WHERE id = ?1",
            [CHAT],
        )
        .unwrap();
        // With nothing stored the prior is None, so this one is a change; the
        // equal-slots arm is the NEXT call.
        commit(
            &conn,
            CHAT,
            ALICE,
            &top(&["coat"]),
            &[],
            EquipSource::Ui,
            "2026-02-02T00:00:00.000Z",
        )
        .unwrap();
        let (out, lines) = captured_with(|| {
            commit(
                &conn,
                CHAT,
                ALICE,
                &top(&["coat"]),
                &[],
                EquipSource::Ui,
                "2026-02-03T00:00:00.000Z",
            )
            .unwrap()
        });
        assert!(!out.changed);
        assert_eq!(stored(&conn, CHAT, ALICE), Some(top(&["coat"])));
        assert_eq!(lines.len(), 1, "{lines:#?}");
        assert_eq!(
            lines[0],
            format!(
                "DEBUG quilltap::wardrobe_wear Committed equipped outfit module=wardrobe-wear \
                 chatId={CHAT} characterId={ALICE} source=ui newlyWorn=0 creditedBundles=0 \
                 credited=0 changed=false"
            )
        );
    }

    /// v4 spec `:310-316`: a FAILED slot write throws v4's sentence, logs the
    /// WARN, and credits nothing (no DEBUG either). The failure is posed by a
    /// partition with no `chats` table (v4's `safeQuery` turns the thrown
    /// write into `null`).
    #[test]
    fn a_failed_write_is_an_error_and_credits_nothing() {
        let conn = main_db();
        conn.execute_batch("DROP TABLE chats").unwrap();
        let (out, lines) = captured_with(|| {
            commit(
                &conn,
                MISSING_CHAT,
                ALICE,
                &top(&["coat"]),
                &[],
                EquipSource::Tool,
                "2026-02-01T00:00:00.000Z",
            )
        });
        let err = out.unwrap_err();
        assert_eq!(
            err.to_string(),
            format!(
                "Failed to save the equipped outfit for character {ALICE} in chat {MISSING_CHAT}"
            )
        );
        assert!(matches!(err, WearCommitError::Unsaved { .. }));
        // The primitives see the same sentence through `DbError`.
        assert_eq!(
            DbError::from(err).to_string(),
            format!(
                "Failed to save the equipped outfit for character {ALICE} in chat {MISSING_CHAT}"
            )
        );
        assert!(ledger(&conn).is_empty());
        let warns: Vec<&String> = lines.iter().filter(|l| l.starts_with("WARN ")).collect();
        assert_eq!(
            warns,
            vec![&format!(
                "WARN quilltap::wardrobe_wear Equipped outfit write failed; no wears credited \
                 module=wardrobe-wear chatId={MISSING_CHAT} characterId={ALICE} source=tool"
            )]
        );
        assert!(
            !lines
                .iter()
                .any(|l| l.contains("Committed equipped outfit")),
            "{lines:#?}"
        );
    }

    /// A MISSING chat is NOT a lost write in v4: `update` is a silent no-op
    /// and `setEquippedOutfit` returns the slots, so the chokepoint credits
    /// (measured at `f5e953a3f` — the tier-2 `commit_lost_write` row). Nothing
    /// is stored (there is no row to store it on).
    #[test]
    fn a_missing_chat_is_still_credited_as_v4_does() {
        let conn = main_db();
        let out = commit(
            &conn,
            MISSING_CHAT,
            ALICE,
            &top(&["coat"]),
            &[],
            EquipSource::Tool,
            "2026-02-01T00:00:00.000Z",
        )
        .unwrap();
        assert_eq!(out.newly_worn_leaf_ids, vec!["coat"]);
        assert_eq!(stored(&conn, MISSING_CHAT, ALICE), None);
        assert_eq!(
            ledger(&conn),
            vec![(
                "coat".into(),
                Some(ALICE.into()),
                1,
                Some(MISSING_CHAT.into())
            )]
        );
    }

    /// A credit that cannot land (no ledger table — a pre-round instance the
    /// boot ensure never reached) fails the call AFTER the slots were written,
    /// as v4's `incrementWears` throws through the chokepoint.
    #[test]
    fn a_failed_credit_is_an_error_after_the_write() {
        let conn = main_db();
        conn.execute_batch("DROP TABLE wardrobe_wear_stats")
            .unwrap();
        let err = commit(
            &conn,
            CHAT,
            ALICE,
            &top(&["coat"]),
            &[],
            EquipSource::Ui,
            "2026-02-01T00:00:00.000Z",
        )
        .unwrap_err();
        assert!(matches!(err, WearCommitError::Credit(_)), "{err}");
        assert!(err.to_string().contains("no such table"), "{err}");
        // As the callers see it: v4's BARE message, never `sqlite error: …`.
        let as_db = DbError::from(err).to_string();
        assert!(as_db.starts_with("no such table"), "{as_db}");
        assert_eq!(stored(&conn, CHAT, ALICE), Some(top(&["coat"])));
    }

    /// The credit's `at` is ONE timestamp for the whole call; the chat is the
    /// call's chat.
    #[test]
    fn one_call_credits_every_wear_at_one_instant() {
        let conn = main_db();
        commit(
            &conn,
            CHAT_2,
            BOB,
            &Slots {
                top: vec!["coat".into()],
                footwear: vec!["boots".into()],
                ..Slots::default()
            },
            &[],
            EquipSource::ChatStart,
            "2026-03-14T09:00:00.000Z",
        )
        .unwrap();
        let mut st = conn
            .prepare(
                "SELECT DISTINCT firstWornAt, lastWornAt, lastWornChatId FROM wardrobe_wear_stats",
            )
            .unwrap();
        let rows: Vec<(String, String, String)> = st
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))
            .unwrap()
            .map(Result::unwrap)
            .collect();
        assert_eq!(
            rows,
            vec![(
                "2026-03-14T09:00:00.000Z".to_string(),
                "2026-03-14T09:00:00.000Z".to_string(),
                CHAT_2.to_string()
            )]
        );
    }
}
