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
//! A swipe is the one case that reads consumed rows. Re-rolling a past line has
//! to see exactly the informs that line's generation saw, so the caller passes
//! `regeneration_of_message_ids` (the target message plus every id in its swipe
//! group) and gets those rows back. Pending rows are deliberately NOT delivered
//! to a swipe — a swipe re-rolls a past line, and it would be surprising for a
//! brand-new inform to land there and vanish.
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
    /// The rows this block carried, for the finalizer to consume. **Empty on a
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
/// `regeneration_of_message_ids` non-empty selects the swipe arm: consumed rows
/// for those messages, and **no row ids at all**. v4 returns the empty list
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
    let rows = db
        .read_main(move |conn| {
            let repo = crate::db::chat_informs::ChatInformsRepository::new(conn);
            if is_swipe {
                repo.find_consumed_by_messages(&chat, &participant, &ids)
            } else {
                repo.find_pending_for_participant(&chat, &participant)
            }
        })
        .unwrap_or_default();

    if rows.is_empty() {
        tracing::debug!(
            target: "quilltap::inform",
            chat_id,
            participant_id,
            pending = 0,
            reapplied = 0,
            "[Inform] No inform block for this turn",
        );
        return InformBlock::default();
    }

    let block = assemble_inform_block(&rows, is_swipe);
    if block.content.is_some() {
        tracing::debug!(
            target: "quilltap::inform",
            chat_id,
            participant_id,
            pending = if is_swipe { 0 } else { rows.len() },
            reapplied = if is_swipe { rows.len() } else { 0 },
            passages = block.passage_count,
            "[Inform] Built inform block",
        );
    }
    block
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
        // empty list makes that impossible to get wrong by accident.
        row_ids: if is_swipe {
            Vec::new()
        } else {
            rows.iter().map(|r| r.id.clone()).collect()
        },
        passage_count: bodies.len(),
    }
}
