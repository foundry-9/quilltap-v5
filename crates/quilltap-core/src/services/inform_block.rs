//! The inform block — the one reader of `chat_informs` on the prompt path
//! (P4.D205, porting v4 `lib/chat/context/inform-block.ts` from `e7d77bb60`).
//!
//! An **inform** is an out-of-character passage the operator hands to a seat:
//! something the character now knows or notices, delivered verbatim as its own
//! system block on that seat's next generation, and consumed once the turn
//! produces a persisted assistant message.
//!
//! Two rules give this module its whole shape, and both are v4's, carried over
//! with its reasoning intact.
//!
//! **It never frames the text.** The block is exactly what the operator typed —
//! no preamble, no "do not mention this", no Host voice, for transparent and
//! opaque characters alike. Several pending passages join with a `---` rule and
//! nothing else. Anything more would be the House speaking over the operator.
//!
//! **It never writes.** Selection and consumption are deliberately separate:
//! building a context is not evidence that anything was delivered, and a
//! provider failure that saves no message must leave the rows pending for the
//! seat's next attempt. `mark_consumed` is called by the finalizer, against a
//! *persisted* assistant message id — never from here.
//!
//! A **standing** inform (`permanent: true`, P4.D249 / v4 `52d6e7ecd`) is the
//! exception to "consumed once": it rides every generation the seat makes in
//! this chat until the operator withdraws it. Standing passages lead the block
//! (they are the same turn after turn, so the block's front stays stable for
//! prefix caches), and only a standing row that has never been delivered is
//! handed back for consumption — which stamps its first delivery without
//! retiring it.
//!
//! A swipe is the one case that reads consumed rows. Re-rolling a past line has
//! to see exactly the informs that line's generation saw, so the caller passes
//! `regeneration_of_message_ids` (the target message plus every id in its swipe
//! group) and gets those rows back. Pending rows are deliberately NOT delivered
//! to a swipe — a swipe re-rolls a past line, and it would be surprising for a
//! brand-new inform to land there and vanish. Standing rows ARE delivered to a
//! swipe: they are in force for every prompt from now on, and a swipe is one.
//!
//! Nothing else reads `chat_informs` on the prompt path.

use crate::db::runtime::Db;

/// The separator between stacked passages. Nothing else joins them.
pub const INFORM_BLOCK_SEPARATOR: &str = "\n\n---\n\n";

/// The assembled block plus the rows it carried.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct InformBlock {
    /// The assembled system block, or `None` when there is nothing to deliver.
    pub content: Option<String>,
    /// The rows this block carried that the finalizer should consume: every
    /// one-shot row, and any standing row not yet delivered. **Empty on a
    /// swipe.**
    pub row_ids: Vec<String>,
    /// How many non-empty passages the block joined — v4's `passages` debug
    /// field. Zero whenever `content` is `None`.
    pub passage_count: usize,
}

/// Assemble the inform block for one generation (v4 `buildInformBlock`).
///
/// Returns `{ content: None, row_ids: [] }` when there is nothing to deliver —
/// and the caller must then push *nothing*, so a turn with no informs is
/// byte-for-byte identical to one built before this feature existed. That is
/// what keeps the cache-determinism golden and the provider prompt caches
/// intact.
///
/// The seat's in-force rows are read on EVERY call (a swipe too — two seat
/// reads on a swipe, as v4). `regeneration_of_message_ids` non-empty selects
/// the swipe arm: every standing row now in force, then the rows those
/// messages consumed (de-duplicated), and **no row ids at all**. v4 returns the empty list
/// rather than relying on the caller to ignore them, "makes that impossible to
/// get wrong by accident" — kept.
///
/// A read failure answers EMPTY rather than propagating: v4 wraps every read in
/// `safeQuery(…, [])`, so a broken table costs the passage, not the turn.
pub fn build_inform_block(
    db: &Db,
    chat_id: &str,
    participant_id: &str,
    regeneration_of_message_ids: Option<&[String]>,
) -> InformBlock {
    let is_swipe = is_swipe_request(regeneration_of_message_ids);

    let chat = chat_id.to_string();
    let participant = participant_id.to_string();
    let ids: Vec<String> = regeneration_of_message_ids.unwrap_or(&[]).to_vec();
    // Each read is its own v4 fallback: a failed in-force read still lets a
    // swipe re-apply what it consumed, and vice versa. Both read through
    // `findByFilter`, whose own `safeQuery` logs `Error finding entities by
    // filter {collection, error}` and answers `[]` before the method's outer
    // `safeQuery` can (`base.repository.ts:283-297`) — `db::fallback`'s line.
    use crate::db::fallback::find_by_filter_or_empty;
    let read =
        |f: &dyn Fn(
            &crate::db::chat_informs::ChatInformsRepository<'_>,
        )
            -> Result<Vec<crate::db::chat_informs::ChatInformRow>, crate::db::DbError>| {
            find_by_filter_or_empty("chat_informs", || {
                db.read_main(|conn| f(&crate::db::chat_informs::ChatInformsRepository::new(conn)))
            })
        };
    let in_force = read(&|repo| repo.find_pending_for_participant(&chat, &participant));
    let rows = if is_swipe {
        merge_for_swipe(
            read(&|repo| repo.find_consumed_by_messages(&chat, &participant, &ids)),
            in_force.into_iter().filter(|r| r.permanent).collect(),
        )
    } else {
        in_force
    };

    if rows.is_empty() {
        tracing::debug!(
            target: "quilltap::inform",
            chat_id,
            participant_id,
            pending = 0,
            reapplied = 0,
            standing = 0,
            "[Inform] No inform block for this turn",
        );
        return InformBlock::default();
    }

    let block = assemble_inform_block(&rows, is_swipe);
    if block.content.is_some() {
        let counts = inform_counts(&rows, is_swipe);
        tracing::debug!(
            target: "quilltap::inform",
            chat_id,
            participant_id,
            pending = counts.pending,
            reapplied = counts.reapplied,
            standing = counts.standing,
            passages = block.passage_count,
            "[Inform] Built inform block",
        );
    }
    block
}

/// The three row counts of v4's `[Inform] Built inform block` debug line.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct InformCounts {
    pub pending: usize,
    pub reapplied: usize,
    pub standing: usize,
}

/// v4's debug arithmetic over the selected rows: standing rows are counted on
/// their own and subtracted from whichever of `pending` / `reapplied` the turn
/// is (`isSwipe ? 0 : rows.length - standing`, and its mirror).
pub fn inform_counts(
    rows: &[crate::db::chat_informs::ChatInformRow],
    is_swipe: bool,
) -> InformCounts {
    let standing = rows.iter().filter(|r| r.permanent).count();
    let rest = rows.len() - standing;
    InformCounts {
        pending: if is_swipe { 0 } else { rest },
        reapplied: if is_swipe { rest } else { 0 },
        standing,
    }
}

/// v4 `mergeForSwipe` — a swipe's rows: what the re-rolled generation consumed,
/// plus every standing row now in force, standing first and without duplicates
/// (a standing row's first delivery may have been the very message being
/// swiped).
pub fn merge_for_swipe(
    reapplied: Vec<crate::db::chat_informs::ChatInformRow>,
    standing: Vec<crate::db::chat_informs::ChatInformRow>,
) -> Vec<crate::db::chat_informs::ChatInformRow> {
    let seen: std::collections::HashSet<String> = standing.iter().map(|r| r.id.clone()).collect();
    let mut out = standing;
    out.extend(reapplied.into_iter().filter(|r| !seen.contains(&r.id)));
    out
}

/// Which set the block reads (v4 `isSwipe`): a regeneration list that is present
/// AND non-empty selects the consumed set; anything else selects the pending
/// set. v4 spells it `Array.isArray(ids) && ids.length > 0`, so an EMPTY list
/// falls back to pending — one of v4's own eight cases.
pub fn is_swipe_request(regeneration_of_message_ids: Option<&[String]>) -> bool {
    regeneration_of_message_ids.is_some_and(|ids| !ids.is_empty())
}

/// The assembly half of v4's `buildInformBlock`, split out so the tier-1
/// differential can drive it directly against v4's real module (v4's own tests
/// inject a fake `repos`, so the module's behaviour is separable from the query
/// by construction).
///
/// v4 trims each body and drops the empty ones; the join is over the TRIMMED
/// bodies, so a passage stored with trailing whitespace is delivered without it.
/// All-whitespace rows leave nothing to deliver and the block is absent — and
/// the row ids are NOT returned in that case either, so those rows stay pending
/// rather than being silently spent by a turn that carried nothing.
pub fn assemble_inform_block(
    rows: &[crate::db::chat_informs::ChatInformRow],
    is_swipe: bool,
) -> InformBlock {
    if rows.is_empty() {
        return InformBlock::default();
    }
    let bodies: Vec<&str> = rows
        .iter()
        .map(|r| r.content_markdown.trim())
        .filter(|b| !b.is_empty())
        .collect();
    if bodies.is_empty() {
        return InformBlock::default();
    }
    InformBlock {
        content: Some(bodies.join(INFORM_BLOCK_SEPARATOR)),
        // A swipe never consumes: the caller ignores these, but returning an
        // empty list makes that impossible to get wrong by accident. Off a
        // swipe, a standing row already delivered is left out so its
        // first-delivery stamp never moves.
        row_ids: if is_swipe {
            Vec::new()
        } else {
            rows.iter()
                .filter(|r| r.consumed_at.is_none())
                .map(|r| r.id.clone())
                .collect()
        },
        passage_count: bodies.len(),
    }
}

#[cfg(test)]
mod tests {
    //! The fallback line (P4.D249 Tier 2 item 18): v4's seat reads go through
    //! `findByFilter`, so a broken `chat_informs` logs `Error finding entities
    //! by filter {collection, error}` — once per read — and the turn proceeds
    //! with no block.

    use super::*;
    use crate::db::runtime::{Db, DbPaths};
    use crate::test_support::captured_with;

    const PEPPER: &str = "dGVzdHBlcHBlcnRlc3RwZXBwZXJ0ZXN0cGVwcGVyMDE=";

    fn db_without_the_table(dir: &tempfile::TempDir) -> Db {
        let data = dir.path().join("data");
        std::fs::create_dir_all(&data).unwrap();
        crate::services::provisioning::provision_fresh_instance(&data, PEPPER).unwrap();
        let db = Db::open(
            DbPaths {
                main: data.join("quilltap.db"),
                mount_index: None,
                llm_logs: None,
            },
            PEPPER,
        )
        .unwrap();
        db.write_blocking(|w| {
            w.main()
                .connection()
                .execute_batch("DROP TABLE chat_informs")?;
            Ok::<_, crate::db::DbError>(())
        })
        .unwrap();
        db
    }

    fn provisioned_db(dir: &tempfile::TempDir) -> Db {
        let data = dir.path().join("data");
        std::fs::create_dir_all(&data).unwrap();
        crate::services::provisioning::provision_fresh_instance(&data, PEPPER).unwrap();
        Db::open(
            DbPaths {
                main: data.join("quilltap.db"),
                mount_index: None,
                llm_logs: None,
            },
            PEPPER,
        )
        .unwrap()
    }

    /// Install `global_capture`'s process-global subscriber once per binary
    /// (the `db/memories.rs` idiom): sibling tests hit the same `debug!`
    /// callsites with NO subscriber on their thread, and tracing's `Interest`
    /// cache can leave a callsite `never` for the thread that IS capturing —
    /// the first run of this pin captured the line, the second captured
    /// nothing (memory: a-second-global-default-silences-the-first). One
    /// permanently-live `always` dispatcher keeps every callsite interesting.
    fn arm_global_callsites() {
        static INIT: std::sync::Once = std::sync::Once::new();
        INIT.call_once(|| {
            tokio::runtime::Builder::new_current_thread()
                .build()
                .expect("a current-thread runtime to arm the global capture layer")
                .block_on(crate::test_support::global_capture::capture_events(async {}));
        });
    }

    /// The ONE line about the block, if any, and the fields it carries after
    /// the level + target prefix, in emission order.
    fn inform_line(lines: &[String], needle: &str) -> (String, Vec<String>) {
        let l = lines
            .iter()
            .find(|l| l.contains(needle))
            .unwrap_or_else(|| panic!("no {needle:?} in {lines:#?}"))
            .clone();
        let fields = l
            .split_whitespace()
            .filter_map(|tok| tok.split_once('=').map(|(k, _)| k.to_string()))
            .collect();
        (l, fields)
    }

    /// The order's items 9–10 asked for capture pins on the two debug lines;
    /// the P4.D249 lane pinned v4's field order against LITERALS inside the
    /// differential and never captured v5's own lines (the `52d6e7ecd`
    /// unification's review catch). Off a swipe: `pending` counts the one-shot
    /// rows, `standing` the standing ones, in v4's field order at v4's level.
    #[test]
    fn the_built_line_carries_v4s_fields_in_order_off_a_swipe() {
        arm_global_callsites();
        let dir = tempfile::tempdir().unwrap();
        let db = provisioned_db(&dir);
        let seats = vec!["p1".to_string()];
        db.write_blocking(move |w| {
            let repo = crate::db::chat_informs::ChatInformsRepository::new(w.main().connection());
            repo.create_batch("c1", "a standing note", &seats, None, true)?;
            repo.create_batch("c1", "a one-shot", &seats, None, false)?;
            Ok::<_, crate::db::DbError>(())
        })
        .unwrap();
        let (block, lines) = captured_with(|| build_inform_block(&db, "c1", "p1", None));
        assert_eq!(
            block.row_ids.len(),
            2,
            "both undelivered rows are handed back"
        );
        let (l, fields) = inform_line(&lines, "[Inform] Built inform block");
        assert!(l.starts_with("DEBUG quilltap::inform"), "{l}");
        assert_eq!(
            fields,
            [
                "chat_id",
                "participant_id",
                "pending",
                "reapplied",
                "standing",
                "passages"
            ],
            "{l}"
        );
        for want in ["pending=1", "reapplied=0", "standing=1", "passages=2"] {
            assert!(l.contains(want), "{want}: {l}");
        }
        assert!(
            !lines
                .iter()
                .any(|x| x.contains("[Inform] No inform block for this turn")),
            "{lines:#?}"
        );
    }

    /// On a swipe the standing row (delivered or not) is merged in and counted
    /// as `standing`; the swiped message's consumed one-shot is `reapplied`;
    /// `pending` is 0 and no row id is handed back.
    #[test]
    fn the_built_line_counts_standing_and_reapplied_on_a_swipe() {
        arm_global_callsites();
        let dir = tempfile::tempdir().unwrap();
        let db = provisioned_db(&dir);
        let seats = vec!["p1".to_string()];
        db.write_blocking(move |w| {
            let repo = crate::db::chat_informs::ChatInformsRepository::new(w.main().connection());
            let standing = repo.create_batch("c1", "a standing note", &seats, None, true)?;
            let one_shot = repo.create_batch("c1", "a one-shot", &seats, None, false)?;
            repo.mark_consumed(&[one_shot[0].id.clone()], "m-swiped")?;
            repo.mark_consumed(&[standing[0].id.clone()], "m-earlier")?;
            Ok::<_, crate::db::DbError>(())
        })
        .unwrap();
        let ids = vec!["m-swiped".to_string()];
        let (block, lines) =
            captured_with(|| build_inform_block(&db, "c1", "p1", Some(ids.as_slice())));
        assert!(block.row_ids.is_empty(), "a swipe never consumes");
        let (l, fields) = inform_line(&lines, "[Inform] Built inform block");
        assert!(l.starts_with("DEBUG quilltap::inform"), "{l}");
        assert_eq!(
            fields,
            [
                "chat_id",
                "participant_id",
                "pending",
                "reapplied",
                "standing",
                "passages"
            ],
            "{l}"
        );
        for want in ["pending=0", "reapplied=1", "standing=1", "passages=2"] {
            assert!(l.contains(want), "{want}: {l}");
        }
    }

    /// Nothing owed: v4's no-block line, with the `standing: 0` field
    /// `52d6e7ecd` added, LAST.
    #[test]
    fn the_no_block_line_gains_standing_zero_last() {
        arm_global_callsites();
        let dir = tempfile::tempdir().unwrap();
        let db = provisioned_db(&dir);
        let (block, lines) = captured_with(|| build_inform_block(&db, "c1", "p1", None));
        assert_eq!(block, InformBlock::default());
        let (l, fields) = inform_line(&lines, "[Inform] No inform block for this turn");
        assert!(l.starts_with("DEBUG quilltap::inform"), "{l}");
        assert_eq!(
            fields,
            [
                "chat_id",
                "participant_id",
                "pending",
                "reapplied",
                "standing"
            ],
            "{l}"
        );
        assert!(l.contains("standing=0"), "{l}");
    }

    #[test]
    fn a_broken_table_logs_v4s_filter_line_per_read_and_delivers_nothing() {
        let dir = tempfile::tempdir().unwrap();
        let db = db_without_the_table(&dir);
        for (ids, want_lines) in [(None, 1usize), (Some(vec!["m1".to_string()]), 2usize)] {
            let (block, lines) =
                captured_with(|| build_inform_block(&db, "c1", "p1", ids.as_deref()));
            assert_eq!(block, InformBlock::default());
            let hits: Vec<&String> = lines
                .iter()
                .filter(|l| l.contains("Error finding entities by filter"))
                .collect();
            assert_eq!(hits.len(), want_lines, "one line per seat read: {lines:#?}");
            assert!(hits[0].starts_with("ERROR"), "{}", hits[0]);
            assert!(hits[0].contains("collection=chat_informs"), "{}", hits[0]);
            assert!(
                hits[0].contains("no such table: chat_informs"),
                "{}",
                hits[0]
            );
        }
    }
}
