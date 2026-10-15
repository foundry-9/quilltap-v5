//! Idle catch-ups for both fold passes — port of v4
//! `lib/memory/extraction/catchup.ts` (`ac6348c5f`; the episode catch-up of
//! bug 232 in its PIN shape — `runFoldCatchup(ctx, 'episode-fold')`, never
//! Phase A's `continue-episode:<wm>` job; pin `83334b9a5`).
//!
//! v4's why, carried: the fold covers a chat's backlog while the chat is
//! live; a chat that went quiet with a pass's cursor behind would otherwise
//! keep its tail unread until it spoke again. The daily sweep selects idle
//! chats whose cursor is behind ([`find_fold_catchup_candidates`]) and
//! enqueues one catch-up job each; the job ([`run_fold_catchup`]) reads the
//! backlog through the fold runner. The OTHER selection reads messages; the
//! episode selection reads only the chat row's fold anchors.

use serde_json::Value;

use crate::chat_predicates::is_help_like_chat_type;
use crate::db::fallback::error_text;
use crate::db::memory_extraction_cursors::{
    ExtractionCursorMap, MemoryExtractionCursorsRepository,
};
use crate::db::runtime::Db;
use crate::db::{chats_read, instance_settings, DbError};
use crate::memory_extraction::window::{message_id, messages_past_cursor};
use crate::model::completion::CompletionProvider;
use crate::model::embedding::EmbeddingProvider;
use crate::services::extraction::context::{
    pick_extraction_connection_profile_id, ExtractionContext,
};
use crate::services::extraction::fold::{
    fold_result, run_fold_pass, FoldResult, FoldSkipReason, RunFoldInput, OTHER_FOLD_PASS,
};
use crate::services::extraction::run::ExtractionDeps;
use crate::services::turn_transcript::load_chat_message_events;

/// A chat must have been quiet this long before a sweep treats its backlog as
/// stalled.
pub const FOLD_CATCHUP_IDLE_MS: f64 = 2.0 * 60.0 * 60.0 * 1000.0;

/// Chats whose last message is older than this are not swept (an old backlog
/// is a backfill, not a catch-up).
pub const FOLD_CATCHUP_LOOKBACK_MS: f64 = 30.0 * 24.0 * 60.0 * 60.0 * 1000.0;

/// Most catch-up jobs of one pass a sweep will enqueue; the rest wait for
/// tomorrow.
pub const FOLD_CATCHUP_MAX_CHATS: usize = 50;

fn str_of<'a>(v: &'a Value, k: &str) -> Option<&'a str> {
    v.get(k).and_then(Value::as_str)
}

/// `chat.lastMessageAt ? Date.parse(chat.lastMessageAt) : NaN`.
fn last_message_ms(chat: &Value) -> Option<f64> {
    str_of(chat, "lastMessageAt")
        .filter(|s| !s.is_empty())
        .and_then(crate::episodic::js_date_parse_ms)
        .map(|ms| ms as f64)
}

/// `chat.summaryAnchorMessageIds ?? []`.
pub fn summary_anchor_message_ids(chat: &Value) -> Vec<String> {
    chat.get("summaryAnchorMessageIds")
        .and_then(Value::as_array)
        .map(|a| {
            a.iter()
                .filter_map(Value::as_str)
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default()
}

/// Chats a catch-up sweep may consider (v4 `idleChatsForCatchup`): not
/// help / brahma, at least two messages, idle more than
/// [`FOLD_CATCHUP_IDLE_MS`] and active within [`FOLD_CATCHUP_LOOKBACK_MS`].
/// Most recently active first (a stable sort, as V8's).
pub fn idle_chats_for_catchup(chats: &[Value], now: f64) -> Vec<&Value> {
    let mut idle: Vec<(&Value, f64)> = chats
        .iter()
        .filter(|c| !is_help_like_chat_type(str_of(c, "chatType")))
        .filter(|c| c.get("messageCount").and_then(Value::as_f64).unwrap_or(0.0) >= 2.0)
        .filter_map(|c| last_message_ms(c).map(|last| (c, last)))
        .filter(|(_, last)| {
            now - last > FOLD_CATCHUP_IDLE_MS && now - last < FOLD_CATCHUP_LOOKBACK_MS
        })
        .collect();
    idle.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
    idle.into_iter().map(|(c, _)| c).collect()
}

/// True when the chat has spoken within the idle window, so the fold, not a
/// catch-up, owns its backlog (v4 `isChatActiveForCatchup`).
pub fn is_chat_active_for_catchup(chat: &Value, now: f64) -> bool {
    last_message_ms(chat).is_some_and(|last| now - last <= FOLD_CATCHUP_IDLE_MS)
}

/// Index (in `messages`) of the last message the chat's summary has folded,
/// or -1 when the chat has never folded or none of its anchors survive (v4
/// `foldEndIndex`).
pub fn fold_end_index(messages: &[Value], summary_anchor_message_ids: &[String]) -> i64 {
    if summary_anchor_message_ids.is_empty() {
        return -1;
    }
    messages
        .iter()
        .rposition(|m| {
            summary_anchor_message_ids
                .iter()
                .any(|a| a == message_id(m))
        })
        .map(|i| i as i64)
        .unwrap_or(-1)
}

/// Whether the episode cursor is behind the fold boundary (v4
/// `isEpisodeBacklogBehind`). A cursor that names a message the chat no
/// longer has counts as behind: the pass treats it as absent and re-reads.
pub fn is_episode_backlog_behind(
    messages: &[Value],
    summary_anchor_message_ids: &[String],
    cursor: Option<&str>,
) -> (bool, i64) {
    let end_index = fold_end_index(messages, summary_anchor_message_ids);
    if end_index == -1 {
        return (false, end_index);
    }
    let cursor_index = cursor
        .filter(|c| !c.is_empty())
        .and_then(|c| messages.iter().position(|m| message_id(m) == c))
        .map(|i| i as i64)
        .unwrap_or(-1);
    (cursor_index < end_index, end_index)
}

/// The sweep's transcript-free test (v4 `isEpisodeBacklogBehindByAnchors`):
/// whether the episode cursor is behind the fold boundary as the chat row
/// records it. `summaryAnchorMessageIds` lists every folded message in turn
/// order, so the boundary is its last entry and the cursor's position in it
/// says how far the pass has read. Can only err towards "behind"; the job's
/// own [`is_episode_backlog_behind`] then skips.
pub fn is_episode_backlog_behind_by_anchors(
    summary_anchor_message_ids: &[String],
    cursor: Option<&str>,
) -> (bool, Option<String>) {
    let Some(fold_end) = summary_anchor_message_ids.last() else {
        return (false, None);
    };
    let cursor_index = cursor
        .filter(|c| !c.is_empty())
        .and_then(|c| summary_anchor_message_ids.iter().rposition(|a| a == c))
        .map(|i| i as i64)
        .unwrap_or(-1);
    (
        cursor_index < summary_anchor_message_ids.len() as i64 - 1,
        Some(fold_end.clone()),
    )
}

/// v4 `FoldCatchupCandidate`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FoldCatchupCandidate {
    pub chat_id: String,
    pub user_id: String,
    /// The job's dedupe anchor: the chat's last message (`other-fold`) or fold
    /// boundary (`episode-fold`).
    pub anchor_message_id: String,
    pub connection_profile_id: String,
}

/// v4 `FoldCatchupSelection`.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct FoldCatchupSelection {
    pub candidates: Vec<FoldCatchupCandidate>,
    /// Eligible chats beyond the per-sweep cap, left for the next sweep.
    pub deferred: usize,
}

/// v4 `findFoldCatchupCandidates`'s options. `chats` and `cursors` let the
/// sweep share one chat list and one cursor read between the two passes.
#[derive(Default)]
pub struct FoldCatchupOptions<'a> {
    pub now: Option<f64>,
    pub limit: Option<usize>,
    pub chats: Option<&'a [Value]>,
    pub cursors: Option<&'a [(String, ExtractionCursorMap)]>,
}

/// Idle chats whose `pass` cursor is behind (v4 `findFoldCatchupCandidates`).
/// Most recently active first, bounded by `limit`. The chat list and cursor
/// reads propagate their failures (v4's repository reads throw).
pub fn find_fold_catchup_candidates(
    db: &Db,
    pass: &str,
    options: FoldCatchupOptions<'_>,
) -> Result<FoldCatchupSelection, DbError> {
    let now = options
        .now
        .unwrap_or_else(|| crate::clock::now_unix_ms() as f64);
    let limit = options.limit.unwrap_or(FOLD_CATCHUP_MAX_CHATS);

    if pass == OTHER_FOLD_PASS {
        let mode =
            db.read_main(|c| Ok(instance_settings::get_memory_extraction_mode_settings(c)))?;
        if mode.other_pass == "turn" {
            tracing::debug!("[FoldCatchup] otherPass=turn; no OTHER catch-up needed");
            return Ok(FoldCatchupSelection::default());
        }
    }

    let owned_chats;
    let chats: &[Value] = match options.chats {
        Some(c) => c,
        None => {
            // v4 `repos.chats.findAll()` is `_findAll` — a FALLBACK `safeQuery`
            // (`Error finding all entities`, `[]`), so a failed list reads as no
            // chats, never a failed selection.
            owned_chats = crate::db::fallback::find_all_or_empty("chats", || {
                db.read_main(chats_read::find_all)
            });
            &owned_chats
        }
    };
    let owned_cursors;
    let cursors: &[(String, ExtractionCursorMap)] = match options.cursors {
        Some(c) => c,
        None => {
            owned_cursors =
                db.read_main(|c| MemoryExtractionCursorsRepository::new(c).find_all_by_chat())?;
            &owned_cursors
        }
    };
    let mut prefiltered = idle_chats_for_catchup(chats, now);
    // A chat that has never folded has no episode backlog.
    if pass == crate::services::extraction::fold::EPISODE_FOLD_PASS {
        prefiltered.retain(|c| !summary_anchor_message_ids(c).is_empty());
    }

    let mut out = FoldCatchupSelection::default();
    for chat in &prefiltered {
        let chat_id = str_of(chat, "id").unwrap_or_default();
        let cursor: Option<String> = cursors
            .iter()
            .find(|(id, _)| id == chat_id)
            .and_then(|(_, map)| cursor_of(map, pass));
        let anchor_message_id = if pass == crate::services::extraction::fold::EPISODE_FOLD_PASS {
            let (behind, fold_end) = is_episode_backlog_behind_by_anchors(
                &summary_anchor_message_ids(chat),
                cursor.as_deref(),
            );
            let Some(fold_end) = fold_end.filter(|_| behind) else {
                continue;
            };
            if out.candidates.len() >= limit {
                out.deferred += 1;
                continue;
            }
            fold_end
        } else {
            if out.candidates.len() >= limit {
                // Counted without reading their messages; some may turn out
                // covered.
                out.deferred += 1;
                continue;
            }
            match load_chat_message_events(db, chat_id) {
                Ok(messages) => {
                    let Some(last) = messages.last() else {
                        continue;
                    };
                    if cursor.as_deref() == Some(message_id(last)) {
                        continue;
                    }
                    message_id(last).to_string()
                }
                Err(error) => {
                    tracing::warn!(
                        chatId = %chat_id,
                        error = %error_text(&error),
                        "[FoldCatchup] Could not inspect chat; skipping"
                    );
                    continue;
                }
            }
        };
        let Some(connection_profile_id) = pick_extraction_connection_profile_id(chat) else {
            tracing::debug!(
                chatId = %chat_id,
                pass = %pass,
                "[FoldCatchup] No connection profile on any seat; skipping chat"
            );
            continue;
        };
        out.candidates.push(FoldCatchupCandidate {
            chat_id: chat_id.to_string(),
            user_id: str_of(chat, "userId").unwrap_or_default().to_string(),
            anchor_message_id,
            connection_profile_id,
        });
    }
    tracing::debug!(
        pass = %pass,
        scanned = chats.len(),
        idle = prefiltered.len(),
        selected = out.candidates.len(),
        deferred = out.deferred,
        "[FoldCatchup] Selection complete"
    );
    Ok(out)
}

/// `cursors.get(chat.id)?.[pass] ?? null` — the map holds the three known
/// passes only (an unknown pass string reads none, as v4's validated map
/// does).
fn cursor_of(map: &ExtractionCursorMap, pass: &str) -> Option<String> {
    crate::db::memory_extraction_cursors::ExtractionCursorPass::parse(pass)
        .and_then(|p| map.get(p))
        .map(str::to_string)
}

/// v4 `runFoldCatchup`'s options.
#[derive(Clone, Copy, Debug, Default)]
pub struct RunFoldCatchupOptions {
    pub ignore_idle: bool,
    pub now: Option<f64>,
    pub max_slices: Option<f64>,
}

/// Catch up one chat's backlog for `pass` on a built context (v4
/// `runFoldCatchup`): the messages past the cursor (`other-fold`), or the
/// folded history past it (any other pass string — the episode branch, R-X).
pub async fn run_fold_catchup<C, E>(
    deps: &ExtractionDeps<'_, C, E>,
    ctx: &ExtractionContext,
    pass: &str,
    options: RunFoldCatchupOptions,
) -> Result<FoldResult, DbError>
where
    C: CompletionProvider,
    E: EmbeddingProvider + Sync,
{
    let now = options
        .now
        .unwrap_or_else(|| crate::clock::now_unix_ms() as f64);
    if !options.ignore_idle && is_chat_active_for_catchup(&ctx.chat, now) {
        tracing::debug!(
            chatId = %ctx.chat_id,
            pass = %pass,
            "[FoldCatchup] Chat active again since the sweep; leaving it to the fold"
        );
        return Ok(fold_result(pass, Some(FoldSkipReason::NotIdle)));
    }
    let messages = ctx.messages()?.to_vec();
    let cursor = ctx.cursors.read(&ctx.db, pass)?.after_message_id;

    if pass == OTHER_FOLD_PASS {
        let window = messages_past_cursor(&messages, cursor.as_deref());
        if window.is_empty() {
            return Ok(fold_result(pass, Some(FoldSkipReason::BehindCursor)));
        }
        let cursor_text = cursor.as_deref().unwrap_or("null");
        tracing::debug!(
            chatId = %ctx.chat_id,
            windowMessages = window.len(),
            cursor = %cursor_text,
            "[FoldCatchup] Running OTHER catch-up"
        );
        return Ok(run_fold_pass(
            deps,
            ctx,
            pass,
            RunFoldInput {
                window_messages: window,
                cover_through_message_id: messages.last().map(|m| message_id(m).to_string()),
                max_slices: options.max_slices,
            },
        )
        .await);
    }

    let (behind, end_index) = is_episode_backlog_behind(
        &messages,
        &summary_anchor_message_ids(&ctx.chat),
        cursor.as_deref(),
    );
    if !behind {
        return Ok(fold_result(
            pass,
            Some(if end_index == -1 {
                FoldSkipReason::NeverFolded
            } else {
                FoldSkipReason::BehindCursor
            }),
        ));
    }
    let folded: Vec<Value> = messages[..=(end_index as usize)].to_vec();
    let cursor_text = cursor.as_deref().unwrap_or("null");
    let fold_end = folded.last().map(message_id).unwrap_or("null").to_string();
    tracing::debug!(
        chatId = %ctx.chat_id,
        foldedMessages = folded.len(),
        cursor = %cursor_text,
        foldEnd = %fold_end,
        "[FoldCatchup] Running episode catch-up"
    );
    Ok(run_fold_pass(
        deps,
        ctx,
        pass,
        RunFoldInput {
            window_messages: folded,
            cover_through_message_id: None,
            max_slices: options.max_slices,
        },
    )
    .await)
}
