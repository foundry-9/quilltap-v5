//! Automatic chat titling — the one place a cheap-LLM title reaches a chat (v4
//! `lib/chat/auto-title.ts`, NEW at `00c290c9a`, bugs 163 + 164).
//!
//! Three paths put a cheap-LLM title on a chat: the checkpoint title check (the
//! `TITLE_UPDATE` job, [`super::title_update_job`]), the context-summary fold
//! ([`super::context_summary`]), and the Salon's "Use automatic naming"
//! (regenerate-title) action ([`super::chat_admin::chat_regenerate_title`]). All
//! go through [`apply_auto_title`], which owns the two rules every automatic
//! rename must obey:
//!
//!   1. A manually-renamed chat keeps its title (bug 164 — the fold used to
//!      overwrite it). Only an explicit regenerate (`clear_manual_rename`)
//!      overrules it.
//!   2. A title that actually changed is the Lantern's cue that the scene has
//!      moved, so it queues a story background (bug 163 — the fold used to
//!      rename without one).
//!
//! [`queue_story_background_if_enabled`] lives here too (moved from
//! `image_profile_resolution`, as v4 moved it out of the title-update handler):
//! it is the Lantern's auto-trigger, and a rename is the only thing that pulls
//! it.
//!
//! ## What the commit message does not say (port from the hunks, `00c290c9a`)
//!
//! - It says the chokepoint "writes only a changed title". The hunk ALSO writes
//!   `extraPatch + updatedAt` on the two refusing arms (`writeExtraOnly`,
//!   `auto-title.ts:65-69`): the title check's cursor, regenerate's
//!   `isManuallyRenamed: false`. The fold passes no patch, so it writes nothing
//!   there.
//! - The title check's UNCHANGED title now writes only the cursor and queues NO
//!   background — before the commit it wrote the title and queued. A behaviour
//!   change the message never names.
//! - The two enqueue lines are re-prefixed `[Title Update]` → `[Auto Title]`
//!   and lose their `context: 'background-jobs.title-update'` field.
//!
//! ## The extra patch is typed to what v4's callers pass
//!
//! v4's `extraPatch` is a `Partial<ChatMetadata>`, but its three callers pass
//! exactly two shapes: the title check's `{ lastRenameCheckInterchange }` and
//! (through `clearManualRename`) regenerate's `{ isManuallyRenamed: false }`.
//! [`AutoTitleExtraPatch`] carries the first; `clear_manual_rename` adds the
//! second exactly as v4's `:52-55` spread does. "Has at least one key" is then
//! "either is present".
//!
//! ## No internal catch
//!
//! v4's `applyAutoTitle` has no try/catch: a `findById`/`update` throw
//! propagates, and each caller decides — the fold catches (`context-summary.ts`
//! `:630-632`), the job fails, the route answers 500 `Failed to regenerate
//! title`. So [`apply_auto_title`] returns the [`DbError`] and leaves the policy
//! to its callers.

use serde_json::Value;

use crate::chat_predicates::is_help_like_chat_type;
use crate::db::chats::ChatUpdate;
use crate::db::chats_read;
use crate::db::runtime::Db;
use crate::db::DbError;
use crate::services::image_profile_resolution::resolve_image_profile_for_chat;

/// v4 `AutoTitleSource` — which producer handed the chokepoint its title.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AutoTitleSource {
    TitleCheck,
    SummaryFold,
    Regenerate,
}

impl AutoTitleSource {
    /// v4's string, as it appears in every `[Auto Title]` line's `source` field.
    pub fn as_str(self) -> &'static str {
        match self {
            AutoTitleSource::TitleCheck => "title-check",
            AutoTitleSource::SummaryFold => "summary-fold",
            AutoTitleSource::Regenerate => "regenerate",
        }
    }
}

/// v4 `AutoTitleOutcome`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AutoTitleOutcome {
    Applied,
    Unchanged,
    ManuallyRenamed,
    Missing,
}

impl AutoTitleOutcome {
    /// v4's string (the `outcome` field of the fold's and regenerate's info lines).
    pub fn as_str(self) -> &'static str {
        match self {
            AutoTitleOutcome::Applied => "applied",
            AutoTitleOutcome::Unchanged => "unchanged",
            AutoTitleOutcome::ManuallyRenamed => "manually-renamed",
            AutoTitleOutcome::Missing => "missing",
        }
    }
}

/// v4 `ApplyAutoTitleOptions.extraPatch`, typed to the one key a caller passes
/// (see the module docs). Written alongside the title, or alone when the title
/// is refused.
#[derive(Clone, Debug, Default)]
pub struct AutoTitleExtraPatch {
    pub last_rename_check_interchange: Option<f64>,
}

/// v4 `applyAutoTitle(opts)`. Re-reads the chat so a rename the user made while
/// the caller's LLM call was in flight still wins.
///
/// `chat_settings` is the user's settings row — `None` skips the story
/// background (v4's `chatSettings: null`). `now_iso` is the `updatedAt` every
/// write stamps, injected so the job's frozen clock stays diffable (the fold and
/// regenerate pass their own `now_iso()`/request stamp).
#[allow(clippy::too_many_arguments)]
pub async fn apply_auto_title(
    db: &Db,
    user_id: &str,
    chat_id: &str,
    title: &str,
    chat_settings: Option<&Value>,
    extra_patch: AutoTitleExtraPatch,
    clear_manual_rename: bool,
    source: AutoTitleSource,
    now_iso: &str,
) -> Result<AutoTitleOutcome, DbError> {
    // `:52-55` — clearManualRename spreads `isManuallyRenamed: false` into the
    // patch; `:56` — every producer already ran `cleanTitle` (which trims), so
    // this trim is neutral, but it is v4's.
    let title = title.trim();

    // `:59` — the re-read, AFTER the caller's LLM call.
    let cid = chat_id.to_string();
    let Some(chat) = db.read_main(move |c| chats_read::find_by_id(c, &cid))? else {
        tracing::debug!(
            chatId = chat_id,
            source = source.as_str(),
            "[Auto Title] Chat vanished before title could be applied"
        );
        return Ok(AutoTitleOutcome::Missing);
    };

    // `:65-69` — `writeExtraOnly`: the patch + `updatedAt`, only when the patch
    // has at least one key.
    let has_extra = extra_patch.last_rename_check_interchange.is_some() || clear_manual_rename;
    let patch_with = |title: Option<String>| ChatUpdate {
        title,
        last_rename_check_interchange: extra_patch.last_rename_check_interchange,
        is_manually_renamed: clear_manual_rename.then_some(false),
        updated_at: Some(now_iso.to_string()),
        ..Default::default()
    };

    // `:71-75` — a hand rename wins unless the user asked for a fresh title.
    let manually_renamed = chat.get("isManuallyRenamed").and_then(Value::as_bool) == Some(true);
    if manually_renamed && !clear_manual_rename {
        tracing::debug!(
            chatId = chat_id,
            source = source.as_str(),
            "[Auto Title] Chat was renamed by hand; keeping its title"
        );
        if has_extra {
            write_chat(db, chat_id, patch_with(None)).await?;
        }
        return Ok(AutoTitleOutcome::ManuallyRenamed);
    }

    // `:77-81` — compared against the RE-READ title.
    let current_title = chat
        .get("title")
        .and_then(Value::as_str)
        .unwrap_or_default();
    if title.is_empty() || title == current_title {
        tracing::debug!(
            chatId = chat_id,
            source = source.as_str(),
            "[Auto Title] Title unchanged"
        );
        if has_extra {
            write_chat(db, chat_id, patch_with(None)).await?;
        }
        return Ok(AutoTitleOutcome::Unchanged);
    }

    // `:83-88`.
    write_chat(db, chat_id, patch_with(Some(title.to_string()))).await?;
    tracing::info!(
        chatId = chat_id,
        source = source.as_str(),
        from = current_title,
        to = title,
        "[Auto Title] Chat retitled"
    );

    // `:92-94` — story backgrounds are for normal chats only: help chats are
    // skipped here (on the RE-READ row's type), autonomous rooms inside the
    // gate. The helper sees the re-read pre-write row with the new title
    // spliced in (`{ ...chat, title }`) — before the commit the job re-fetched
    // after its write instead.
    if let Some(settings) = chat_settings {
        if !is_help_like_chat_type(chat.get("chatType").and_then(Value::as_str)) {
            let mut retitled = chat.clone();
            if let Some(obj) = retitled.as_object_mut() {
                obj.insert("title".to_string(), Value::String(title.to_string()));
            }
            queue_story_background_if_enabled(db, user_id, &retitled, Some(settings), title).await;
        }
    }

    Ok(AutoTitleOutcome::Applied)
}

/// `repos.chats.update(chatId, patch)` through the single writer. Errors
/// propagate (v4 has no catch here).
async fn write_chat(db: &Db, chat_id: &str, patch: ChatUpdate) -> Result<(), DbError> {
    let cid = chat_id.to_string();
    db.write(move |writers| {
        writers.main().chats().update(&cid, &patch)?;
        Ok(())
    })
    .await
}

/// v4 `queueStoryBackgroundIfEnabled` (`auto-title.ts:102-167`, moved there
/// from `title-update.ts` at `00c290c9a`): enqueue a story-background job only
/// when story backgrounds are enabled and the chat is not an autonomous room,
/// an image profile resolves, and the chat has present participants. Errors are
/// swallowed (automatic path). `chat` is the chats row, `chat_settings` the
/// user's settings.
pub async fn queue_story_background_if_enabled(
    db: &Db,
    user_id: &str,
    chat: &Value,
    chat_settings: Option<&Value>,
    new_title: &str,
) {
    // 1. `storyBackgroundsSettings?.enabled` gate.
    let enabled = chat_settings
        .and_then(|cs| cs.get("storyBackgroundsSettings"))
        .and_then(|s| s.get("enabled"))
        .and_then(Value::as_bool)
        == Some(true);
    if !enabled {
        return;
    }
    // 2. Autonomous rooms (4.6 Private Character Rooms): the Lantern's
    //    auto-trigger is disabled. Backgrounds are token-budget-conscious; the
    //    user is not in the room to see them, and a character can still
    //    deliberately invoke image-generation tools when desired.
    if chat.get("chatType").and_then(Value::as_str) == Some("autonomous") {
        return;
    }
    // 3. Resolve the image profile (dual-connection read).
    let chat_owned = chat.clone();
    let settings_owned = chat_settings.cloned();
    let user_owned = user_id.to_string();
    let image_profile_id = db
        .write(move |writers| {
            let main = writers.main().connection();
            let mount = writers.mount_index().map(|w| w.connection());
            Ok(resolve_image_profile_for_chat(
                main,
                mount,
                &user_owned,
                &chat_owned,
                settings_owned.as_ref(),
            ))
        })
        .await
        // ⚠ A recorded divergence (the `00c290c9a` unification's review): in
        // v4 this call sits OUTSIDE the enqueue's try, so a throw here would
        // propagate out of `applyAutoTitle` (the fold's catch logs it, the job
        // fails, regenerate answers 500). v5's resolver answers `Option` — its
        // reads fold failure to "no profile", as v4's fallback repository reads
        // mostly do — and only a failed writer round-trip can land here, where
        // it queues nothing, silently. Pre-existing (moved verbatim from
        // `image_profile_resolution.rs`); not widened here.
        .ok()
        .flatten();
    let Some(image_profile_id) = image_profile_id else {
        return;
    };
    // 4. Participant characterIds — of participants who are actually in the
    //    scene. [70505745a] Absent and (soft-)removed participants must never be
    //    painted into the background; the crafter is told to place every
    //    enumerated character as a figure in the frame, so a stale enumeration
    //    puts someone in the room who walked out of it. 'silent' counts as
    //    present: they are standing there, just not speaking.
    //
    //    `.filter(p => isParticipantPresent(p.status) && p.characterId)` — the
    //    second conjunct is v4's pre-existing JS truthiness, so an empty-string
    //    characterId drops out here exactly as it does at the manual-regenerate
    //    twin (`api::chat_media::chat_regenerate_background`).
    let character_ids: Vec<String> = chat
        .get("participants")
        .and_then(Value::as_array)
        .map(|arr| {
            arr.iter()
                .filter(|p| crate::chat_predicates::json_participant_is_present(p))
                .filter_map(|p| p.get("characterId").and_then(Value::as_str))
                .filter(|s| !s.is_empty())
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default();
    if character_ids.is_empty() {
        return;
    }

    let chat_id = chat.get("id").and_then(Value::as_str).unwrap_or_default();
    let project_id = chat
        .get("projectId")
        .and_then(Value::as_str)
        .map(str::to_string);
    // Errors swallowed — the automatic path must never affect the caller. v4
    // says so out loud, though: the enqueue's two outcomes are the last two log
    // sites of `auto-title.ts` (`:154` info / `:162` warn), carried so a
    // background that never got queued is visible in `combined.log`. At
    // `00c290c9a` both lost their `context` field and took the `[Auto Title]`
    // prefix; their fields are v4's camelCase names (the P4.D212 precedent).
    //
    // The `isNew` gate is v4's: a dedupe hit (a story-background job already
    // pending for this chat) returns the EXISTING id with `isNew: false` and
    // says nothing.
    //
    // Recorded shape divergence: v4 carries the failure text inside the context
    // bag (`{chatId, error}` — `logger.warn` has no error parameter), while v5's
    // `error = %e` is the house idiom at 130-odd sites and the P4.49 file layer
    // hoists a field named `error` into the record's own `error` key. The text
    // is present and greppable either way; the placement differs.
    match crate::services::queue_service::enqueue_story_background_generation(
        db,
        user_id,
        chat_id,
        &image_profile_id,
        &character_ids,
        Some(new_title),
        project_id,
    )
    .await
    {
        Ok((job_id, is_new)) => {
            if is_new {
                tracing::info!(
                    chatId = chat_id,
                    jobId = job_id.as_str(),
                    imageProfileId = image_profile_id.as_str(),
                    characterCount = character_ids.len(),
                    "[Auto Title] Queued story background generation"
                );
            }
        }
        Err(e) => {
            tracing::warn!(
                chatId = chat_id,
                error = %e,
                "[Auto Title] Failed to queue story background generation"
            );
        }
    }
}
