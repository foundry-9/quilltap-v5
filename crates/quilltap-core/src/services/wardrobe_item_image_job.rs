//! `WARDROBE_ITEM_IMAGE_GENERATION` — draw a picture of one wardrobe item,
//! queued by `wardrobe_create` / `wardrobe_update` when the operator has let
//! the wardrobe tools commission pictures (`chatSettings.wardrobeImageSettings.
//! generateFromTools`). v4 `lib/background-jobs/handlers/wardrobe-item-image.
//! ts` (`b3f937076`).
//!
//! The work is [`generate_wardrobe_item_image`], the same path the editor's
//! Generate button takes: designated profile, worn-by-owner prompt, the
//! Concierge's image failover, then the item-image write. v4 ran it in the job
//! child with the bridge write routed to the parent over host-RPC; v5's runner
//! is in-process, so the write is the ordinary `Db::write` (P4.D263 R-A).
//!
//! Nothing here retries a spend: a missing profile, a refusal or a provider
//! failure is logged and the job ENDS (the enqueue sets `maxAttempts: 1`, and
//! the expected failures complete rather than fail). Anything else is
//! rethrown — the runner marks the job failed (one attempt, no retry).
//!
//! P4.D263 R-D: the generation passes `containerId: payload.characterId`
//! (which the project-aesthetic lookup ignores for a character's item) and NO
//! profile override; the payload's `chatId` is never forwarded, so no
//! Concierge bubble or ledger row can result.

use serde_json::Value;

use crate::api::types::WardrobeContainerScope;
use crate::db::background_jobs::BackgroundJob;
use crate::db::runtime::Db;
use crate::db::DbError;
use crate::services::job_runner::{JobFuture, JobHandler, JobOutcome};
use crate::services::wardrobe_container::resolve_wardrobe_item_home;
use crate::services::wardrobe_item_image_generation::{
    generate_wardrobe_item_image, GenerateWardrobeItemImageArgs, WardrobeImageGenerationFailure,
    WardrobeItemImageSeams,
};
use crate::services::wardrobe_item_images::service::ItemImageError;

const LOG_CONTEXT: &str = "background-jobs.wardrobe-item-image";

/// The job's handler — the host registers it under
/// `WARDROBE_ITEM_IMAGE_GENERATION` with the seams `wardrobe_item_image_seams`
/// builds.
pub struct WardrobeItemImageJobHandler {
    pub seams: WardrobeItemImageSeams,
}

impl JobHandler for WardrobeItemImageJobHandler {
    fn handle<'a>(&'a self, db: &'a Db, job: &'a BackgroundJob) -> JobFuture<'a> {
        Box::pin(handle_wardrobe_item_image_generation(db, &self.seams, job))
    }
}

/// v4 `handleWardrobeItemImageGeneration(job)`.
pub async fn handle_wardrobe_item_image_generation(
    db: &Db,
    seams: &WardrobeItemImageSeams,
    job: &BackgroundJob,
) -> JobOutcome {
    let payload: Value = serde_json::from_str(&job.payload).unwrap_or(Value::Null);
    let field = |k: &str| {
        payload
            .get(k)
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string()
    };
    let (chat_id, character_id, item_id) = (field("chatId"), field("characterId"), field("itemId"));

    tracing::info!(
        context = LOG_CONTEXT,
        jobId = %job.id,
        chatId = %chat_id,
        characterId = %character_id,
        itemId = %item_id,
        "[WardrobeItemImage] Starting tool-queued wardrobe item image"
    );

    let home = {
        let (uid, cid, iid) = (job.user_id.clone(), character_id.clone(), item_id.clone());
        db.write(move |ws| {
            let mount = ws
                .mount_index()
                .ok_or(DbError::PartitionUnavailable(
                    crate::write_partition::WriteDbTarget::MountIndex,
                ))?
                .connection();
            resolve_wardrobe_item_home(
                ws.main().connection(),
                mount,
                &uid,
                WardrobeContainerScope::Character,
                Some(&cid),
                &iid,
            )
        })
        .await
    };
    let home = match home {
        Ok(Some(home)) => home,
        Ok(None) => {
            // Deleted, moved or given away between the tool call and now.
            tracing::info!(
                context = LOG_CONTEXT,
                jobId = %job.id,
                characterId = %character_id,
                itemId = %item_id,
                "[WardrobeItemImage] Item no longer in the character's wardrobe; nothing to draw"
            );
            return JobOutcome::Completed(None);
        }
        Err(e) => return JobOutcome::Failed(crate::db::fallback::error_text(&e)),
    };
    if home
        .item
        .get("archivedAt")
        .and_then(Value::as_str)
        .is_some_and(|s| !s.is_empty())
    {
        tracing::info!(
            context = LOG_CONTEXT,
            jobId = %job.id,
            itemId = %item_id,
            "[WardrobeItemImage] Item archived since the tool call; skipping"
        );
        return JobOutcome::Completed(None);
    }

    let result = generate_wardrobe_item_image(
        db,
        seams,
        &GenerateWardrobeItemImageArgs {
            user_id: &job.user_id,
            home: &home,
            container_id: Some(&character_id),
            image_profile_id: None,
        },
    )
    .await;

    match result {
        Ok(r) => {
            tracing::info!(
                context = LOG_CONTEXT,
                jobId = %job.id,
                itemId = %item_id,
                fileId = %r.file_id,
                subject = r.subject.as_str(),
                profileId = %r.profile_id,
                rerouted = r.rerouted,
                "[WardrobeItemImage] Tool-queued wardrobe item image complete"
            );
            JobOutcome::Completed(None)
        }
        Err(WardrobeImageGenerationFailure::NoProfile) => {
            tracing::warn!(
                context = LOG_CONTEXT,
                jobId = %job.id,
                itemId = %item_id,
                "[WardrobeItemImage] No usable image profile; skipping"
            );
            JobOutcome::Completed(None)
        }
        Err(WardrobeImageGenerationFailure::Item(ItemImageError::Archived { .. })) => {
            tracing::info!(
                context = LOG_CONTEXT,
                jobId = %job.id,
                characterId = %character_id,
                "[WardrobeItemImage] Owner archived since the tool call; skipping"
            );
            JobOutcome::Completed(None)
        }
        Err(WardrobeImageGenerationFailure::Generation {
            message,
            trail,
            refused,
        }) => {
            let trail_json = trail.as_ref().map(|t| {
                Value::Array(
                    t.iter()
                        .map(|r| {
                            serde_json::json!({
                                "profileName": r.profile_name,
                                "outcome": r.outcome.as_str(),
                            })
                        })
                        .collect(),
                )
                .to_string()
            });
            tracing::warn!(
                context = LOG_CONTEXT,
                jobId = %job.id,
                itemId = %item_id,
                refused = refused,
                error = %message,
                trailJson = trail_json.as_deref(),
                "[WardrobeItemImage] Provider would not draw the item"
            );
            JobOutcome::Completed(None)
        }
        // v4 rethrows anything else — the runner marks the job FAILED.
        Err(WardrobeImageGenerationFailure::Item(e)) => JobOutcome::Failed(e.message()),
    }
}

#[cfg(test)]
mod tests {
    use crate::realtime::job_topics::topics_for_completed_job;
    use crate::realtime::types::RealtimeTopic;

    /// C1 §10 (v4 `job-topics.ts:66-69`): a completed picture job refreshes the
    /// owner's `characters` topic and the `mountPoints` collection — and
    /// nothing for the chat (the payload's `chatId` is not a topic).
    #[test]
    fn completion_refreshes_the_owner_and_the_mounts() {
        let hints = topics_for_completed_job(
            Some("WARDROBE_ITEM_IMAGE_GENERATION"),
            Some(
                &serde_json::json!({ "chatId": "chat-1", "characterId": "char-1", "itemId": "i" }),
            ),
        );
        let got: Vec<(RealtimeTopic, Option<String>)> =
            hints.into_iter().map(|h| (h.topic, h.id)).collect();
        assert_eq!(
            got,
            vec![
                (RealtimeTopic::Characters, Some("char-1".to_string())),
                (RealtimeTopic::MountPoints, None),
            ]
        );
    }
}
