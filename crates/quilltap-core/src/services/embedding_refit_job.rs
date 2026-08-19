//! The `EMBEDDING_REFIT` job handler — a port of v4's
//! `lib/background-jobs/handlers/embedding-refit.ts`. Rebuilds the TF-IDF
//! vocabulary for a BUILTIN embedding profile from every character's memories
//! (plus the help docs for coverage), persists it via the ported
//! `tfidf_vocabulary.upsertByProfileId`, then enqueues `EMBEDDING_REINDEX_ALL`.
//!
//! ## The debounce scheduler is host-timing (not ported)
//!
//! v4's `embedding-job-scheduler.ts` debounces refit scheduling behind a 5 s
//! `setTimeout` map. Timers do not live in the core (the W4.8 rule) — the host
//! driver owns that cadence. The only pure decision in that file is the
//! BUILTIN-profile gate ([`is_builtin_profile`]), which the refit handler already
//! reproduces; the rest is the host driver's cadence seam.

use serde_json::Value;

use crate::db::runtime::Db;
use crate::db::tfidf_vocabulary::TvCreate;
use crate::tfidf::{state_idf_json, state_vocabulary_json, TfIdfVectorizer};

/// The decoded `EMBEDDING_REFIT` payload (v4 `EmbeddingRefitPayload`).
#[derive(Debug, Clone)]
pub struct EmbeddingRefitPayload {
    pub profile_id: String,
    /// v4 `payload.triggerReindex !== false` — so an absent field defaults to
    /// `true`.
    pub trigger_reindex: bool,
}

impl EmbeddingRefitPayload {
    /// Decode from the raw job payload JSON. Missing `profileId` is an error.
    pub fn from_json(payload: &Value) -> Result<Self, String> {
        let profile_id = payload
            .get("profileId")
            .and_then(Value::as_str)
            .ok_or("payload missing profileId")?
            .to_string();
        // `!== false`: absent (None) or any non-false value → true.
        let trigger_reindex = payload.get("triggerReindex").and_then(Value::as_bool) != Some(false);
        Ok(Self {
            profile_id,
            trigger_reindex,
        })
    }
}

/// v4's scheduler `scheduleRefit` gate: only BUILTIN profiles get a refit. The
/// debounce timer that follows is host-side; this pure predicate is the ported
/// half.
pub fn is_builtin_profile(provider: &str) -> bool {
    provider == "BUILTIN"
}

/// Handle an `EMBEDDING_REFIT` job (v4 `handleEmbeddingRefit`). `now_iso` is the
/// injected fit clock (v4's internal `new Date().toISOString()` inside
/// `fitCorpus`). Skips (returns `Ok`) when the profile is not BUILTIN, has no
/// characters, or has no memories; errors only when the profile is missing or the
/// fit / persist fails.
pub async fn handle_embedding_refit(
    db: &Db,
    user_id: &str,
    payload: &EmbeddingRefitPayload,
    now_iso: &str,
) -> Result<(), String> {
    // Resolve the embedding profile.
    let profile_id = payload.profile_id.clone();
    let profile = db
        .read_main(move |conn| crate::db::embedding_profiles::find_by_id(conn, &profile_id))
        .map_err(|e| format!("{e}"))?
        .ok_or_else(|| format!("Embedding profile not found: {}", payload.profile_id))?;

    // Verify this is a BUILTIN profile (v4 warns + skips otherwise).
    if !is_builtin_profile(&profile.provider) {
        return Ok(());
    }

    // Get all characters for this user (the overlaid read spans both DBs).
    let uid = user_id.to_string();
    let characters = db
        .write(move |writers| {
            let mount = writers
                .mount_index()
                .ok_or_else(|| {
                    crate::db::DbError::Internal(
                        "embedding refit requires the mount-index database".to_string(),
                    )
                })?
                .connection();
            let main = writers.main().connection();
            crate::db::characters_read::find_by_user_id(main, mount, &uid)
        })
        .await
        .map_err(|e| format!("{e}"))?;

    // No characters → skip (v4's explicit branch).
    if characters.is_empty() {
        return Ok(());
    }

    // Gather every character's memories into `${summary}\n\n${content}` documents.
    let char_ids: Vec<String> = characters
        .iter()
        .filter_map(|c| c.get("id").and_then(Value::as_str).map(str::to_string))
        .collect();
    let mut documents = db
        .read_main(move |conn| {
            let mut docs: Vec<String> = Vec::new();
            for cid in &char_ids {
                for m in crate::db::memories_read::find_by_character_id(conn, cid)? {
                    let summary = m.get("summary").and_then(Value::as_str).unwrap_or_default();
                    let content = m.get("content").and_then(Value::as_str).unwrap_or_default();
                    docs.push(format!("{summary}\n\n{content}"));
                }
            }
            Ok(docs)
        })
        .map_err(|e| format!("{e}"))?;

    // No memories → skip (v4 checks BEFORE adding help docs).
    if documents.is_empty() {
        return Ok(());
    }

    // Include help docs for vocabulary coverage (v4 swallows a read failure).
    if let Ok(help_docs) =
        db.read_main(|conn| crate::db::help_docs::HelpDocsRepository::new(conn).find_all())
    {
        for doc in help_docs {
            documents.push(format!("{}\n\n{}", doc.title, doc.content));
        }
    }

    // Fit the vocabulary (include bigrams). The memories guard guarantees a
    // non-empty corpus, so `EmptyCorpus` cannot fire here.
    let mut vectorizer = TfIdfVectorizer::new(true);
    vectorizer
        .fit_corpus(&documents, now_iso.to_string())
        .map_err(|e| e.message().to_string())?;

    let state = vectorizer
        .get_state()
        .ok_or("Failed to get TF-IDF state after fitting")?;

    // Persist via upsertByProfileId (mints id + createdAt + updatedAt itself).
    let tv = TvCreate {
        profile_id: payload.profile_id.clone(),
        user_id: user_id.to_string(),
        vocabulary: state_vocabulary_json(&state),
        idf: state_idf_json(&state),
        avg_doc_length: state.avg_doc_length,
        vocabulary_size: state.vocabulary_size as f64,
        include_bigrams: state.include_bigrams,
        fitted_at: state.fitted_at.clone(),
    };
    db.write(move |writers| writers.main().tfidf_vocabulary().upsert_by_profile_id(&tv))
        .await
        .map_err(|e| format!("{e}"))?;

    // Trigger reindex if requested (v4 `payload.triggerReindex !== false`).
    if payload.trigger_reindex {
        crate::services::queue_service::enqueue_embedding_reindex_all(
            db,
            user_id,
            &payload.profile_id,
            // v4's refit sends no `scope` — the handler defaults it to 'all'.
            None,
        )
        .await
        .map_err(|e| format!("{e}"))?;
    }

    Ok(())
}

/// An `EMBEDDING_REFIT` [`crate::services::job_runner::JobHandler`]. The host wires
/// it (removing the type from the loud fallback); the differential's runner E2E
/// wires it with a frozen clock. `now_iso` is the fit clock: `None` reads the real
/// clock at handle time, `Some(s)` pins it (the test).
pub struct EmbeddingRefitHandler {
    pub now_iso: Option<String>,
}

impl crate::services::job_runner::JobHandler for EmbeddingRefitHandler {
    fn handle<'a>(
        &'a self,
        db: &'a Db,
        job: &'a crate::db::background_jobs::BackgroundJob,
    ) -> crate::services::job_runner::JobFuture<'a> {
        Box::pin(async move {
            let payload_json: Value = serde_json::from_str(&job.payload).unwrap_or(Value::Null);
            let payload = match EmbeddingRefitPayload::from_json(&payload_json) {
                Ok(p) => p,
                Err(e) => return crate::services::job_runner::JobOutcome::Failed(e),
            };
            let now_iso = self.now_iso.clone().unwrap_or_else(crate::clock::now_iso);
            match handle_embedding_refit(db, &job.user_id, &payload, &now_iso).await {
                Ok(()) => crate::services::job_runner::JobOutcome::Completed(None),
                Err(e) => crate::services::job_runner::JobOutcome::Failed(e),
            }
        })
    }
}
