//! The `EMBEDDING_REFIT` job handler — a port of v4's
//! `lib/background-jobs/handlers/embedding-refit.ts`. Rebuilds the TF-IDF
//! vocabulary for a BUILTIN embedding profile from every character's memories
//! (plus the help docs for coverage), persists it via the ported
//! `tfidf_vocabulary.upsertByProfileId`, then enqueues `EMBEDDING_REINDEX_ALL`.
//!
//! ## The corpus is every memory row (`4ae0c4ca6`)
//!
//! "Every memory, by the characters that hold one: the same corpus the refit
//! policy counts (`assessBuiltinRefit`). Enumerating characters instead would
//! miss memories held by a character whose vault cannot be read, and a refit
//! the policy keeps asking for would never clear its reason." So the corpus is
//! [`crate::db::memories_read::find_distinct_character_ids`] then each id's
//! rows IN THAT ORDER (the vocabulary's index order follows document order);
//! the pre-span characters read — and with it v5's mount-index requirement —
//! is gone. Each memory's document is
//! [`crate::memory_embedding_text::lexical_embedding_text_for`] — "exactly the
//! text the BUILTIN path embeds for a memory" — and the fitted row is stamped
//! `textShapeVersion = LEXICAL_TEXT_SHAPE_VERSION`, the ONLY writer of a
//! non-zero value, which is what clears the refit policy's `text-shape` reason.
//!
//! The debounce that schedules refits is
//! [`crate::services::embedding_job_scheduler`] (P4.D292, R-Q). v4's
//! `getTfIdfVectorizer` dynamic plugin import (its ERROR `[EmbeddingRefit]
//! Failed to load TF-IDF vectorizer` and throw `Built-in embedding plugin not
//! available`) is NO-PORT: v5 links [`crate::tfidf`] directly.

use serde_json::Value;

use crate::db::runtime::Db;
use crate::db::tfidf_vocabulary::TvCreate;
use crate::memory_embedding_text::{
    lexical_embedding_text_for, EmbeddingTextSource, LEXICAL_TEXT_SHAPE_VERSION,
};
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

/// v4's refit handler's gate (`profile.provider !== 'BUILTIN'` — provider
/// ROUTING, not the lexical-scale predicate): only BUILTIN profiles are refit.
pub fn is_builtin_profile(provider: &str) -> bool {
    provider == "BUILTIN"
}

/// Handle an `EMBEDDING_REFIT` job (v4 `handleEmbeddingRefit`). `job_id` is
/// v4's `job.id` (every line carries it); `now_iso` is the injected fit clock
/// (v4's internal `new Date().toISOString()` inside `fitCorpus`). Skips
/// (returns `Ok`) when the profile is not BUILTIN (WARN) or there are no
/// memories (INFO); errors when the profile is missing or the fit / persist
/// fails. v4's nine lines, every one on the caller's thread.
pub async fn handle_embedding_refit(
    db: &Db,
    job_id: &str,
    user_id: &str,
    payload: &EmbeddingRefitPayload,
    now_iso: &str,
) -> Result<(), String> {
    tracing::info!(
        target: "quilltap::jobs",
        context = "handleEmbeddingRefit",
        jobId = %job_id,
        profileId = %payload.profile_id,
        "[EmbeddingRefit] Starting vocabulary refit"
    );

    // Resolve the embedding profile.
    let profile_id = payload.profile_id.clone();
    let profile = db
        .read_main(move |conn| crate::db::embedding_profiles::find_by_id(conn, &profile_id))
        .map_err(|e| format!("{e}"))?
        .ok_or_else(|| format!("Embedding profile not found: {}", payload.profile_id))?;

    // Verify this is a BUILTIN profile.
    if !is_builtin_profile(&profile.provider) {
        tracing::warn!(
            target: "quilltap::jobs",
            context = "handleEmbeddingRefit",
            jobId = %job_id,
            profileId = %payload.profile_id,
            provider = %profile.provider,
            "[EmbeddingRefit] Profile is not BUILTIN, skipping refit"
        );
        return Ok(());
    }

    // Every memory, by the characters that hold one (see the module doc).
    let (characters, mut documents) = db
        .read_main(|conn| {
            let character_ids = crate::db::memories_read::find_distinct_character_ids(conn);
            let mut docs: Vec<String> = Vec::new();
            for cid in &character_ids {
                for m in crate::db::memories_read::find_by_character_id(conn, cid)? {
                    let str_of = |k: &str| m.get(k).and_then(Value::as_str).unwrap_or_default();
                    docs.push(lexical_embedding_text_for(&EmbeddingTextSource {
                        summary: str_of("summary"),
                        content: str_of("content"),
                        ..Default::default()
                    }));
                }
            }
            Ok((character_ids.len(), docs))
        })
        .map_err(|e| format!("{e}"))?;
    tracing::debug!(
        target: "quilltap::jobs",
        context = "handleEmbeddingRefit",
        jobId = %job_id,
        characters = characters,
        memories = documents.len(),
        "[EmbeddingRefit] Corpus gathered"
    );

    if documents.is_empty() {
        tracing::info!(
            target: "quilltap::jobs",
            context = "handleEmbeddingRefit",
            jobId = %job_id,
            profileId = %payload.profile_id,
            "[EmbeddingRefit] No memories found, skipping refit"
        );
        return Ok(());
    }

    // Include help docs in the corpus for better vocabulary coverage. v4's
    // `helpDocs.findAll()` is the base `_findAll` FALLBACK (ERROR `Error
    // finding all entities`, `[]`), so its catch — the WARN below — is reached
    // only inside the strict-repository scope, here as there.
    match db.read_main(|conn| {
        crate::db::fallback::find_all_strict_aware("help_docs", || {
            crate::db::help_docs::HelpDocsRepository::new(conn).find_all()
        })
    }) {
        Ok(help_docs) => {
            for doc in help_docs {
                documents.push(format!("{}\n\n{}", doc.title, doc.content));
            }
        }
        Err(e) => tracing::warn!(
            target: "quilltap::jobs",
            context = "handleEmbeddingRefit",
            error = %crate::db::fallback::error_text(&e),
            "[EmbeddingRefit] Failed to load help docs for corpus, continuing without them"
        ),
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

    // Persist via upsertByProfileId (mints id + createdAt + updatedAt itself),
    // stamped with the text shape the fit read.
    let tv = TvCreate {
        profile_id: payload.profile_id.clone(),
        user_id: user_id.to_string(),
        vocabulary: state_vocabulary_json(&state),
        idf: state_idf_json(&state),
        avg_doc_length: state.avg_doc_length,
        vocabulary_size: state.vocabulary_size as f64,
        include_bigrams: state.include_bigrams,
        fitted_at: state.fitted_at.clone(),
        text_shape_version: LEXICAL_TEXT_SHAPE_VERSION as f64,
    };
    db.write(move |writers| writers.main().tfidf_vocabulary().upsert_by_profile_id(&tv))
        .await
        .map_err(|e| format!("{e}"))?;

    tracing::info!(
        target: "quilltap::jobs",
        context = "handleEmbeddingRefit",
        jobId = %job_id,
        profileId = %payload.profile_id,
        vocabularySize = state.vocabulary_size,
        documentCount = documents.len(),
        avgDocLength = %crate::jsnum::to_fixed(state.avg_doc_length, 2),
        "[EmbeddingRefit] Vocabulary fitted and saved"
    );

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
        tracing::info!(
            target: "quilltap::jobs",
            context = "handleEmbeddingRefit",
            jobId = %job_id,
            profileId = %payload.profile_id,
            "[EmbeddingRefit] Enqueued reindex job"
        );
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
            match handle_embedding_refit(db, &job.id, &job.user_id, &payload, &now_iso).await {
                Ok(()) => crate::services::job_runner::JobOutcome::Completed(None),
                Err(e) => crate::services::job_runner::JobOutcome::Failed(e),
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::global_capture;

    const PEPPER: &str = "dGVzdHBlcHBlcnRlc3RwZXBwZXJ0ZXN0cGVwcGVyMDE=";

    /// A fresh instance (its default profile is BUILTIN) with one memory; the
    /// help-docs table renamed away when `break_help_docs`.
    fn instance(break_help_docs: bool) -> (tempfile::TempDir, Db, String, String) {
        let dir = tempfile::tempdir().unwrap();
        let data = dir.path().join("data");
        std::fs::create_dir_all(&data).unwrap();
        crate::services::provisioning::provision_fresh_instance(&data, PEPPER).unwrap();
        let db = Db::open_main(data.join("quilltap.db"), PEPPER).unwrap();
        let (profile, user) = db
            .write_blocking(move |ws| {
                let c = ws.main().connection();
                c.execute_batch(
                    r#"INSERT INTO "memories" ("id","characterId","content","summary","createdAt","updatedAt") VALUES ('e2920000-0000-4000-8000-000000000a01','e2920000-0000-4000-8000-0000000000c1','the lantern burns','a lantern','2026-01-01T00:00:00.000Z','2026-01-01T00:00:00.000Z')"#,
                )?;
                if break_help_docs {
                    c.execute_batch(r#"ALTER TABLE "help_docs" RENAME TO "help_docs_gone""#)?;
                }
                Ok(c.query_row(
                    "SELECT id, userId FROM embedding_profiles WHERE isDefault = 1",
                    [],
                    |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)),
                )?)
            })
            .unwrap();
        (dir, db, profile, user)
    }

    /// v4's help-docs WARN is reached only inside the strict scope: a failing
    /// read logs the base `_findAll` ERROR and the refit fits WITHOUT the help
    /// docs, the WARN silent — and the fitted row is stamped 1.
    #[test]
    fn a_failing_help_docs_read_is_the_base_fallback_not_the_warn() {
        global_capture::install();
        let (_dir, db, profile, user) = instance(true);
        let payload = EmbeddingRefitPayload {
            profile_id: profile.clone(),
            trigger_reindex: false,
        };
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        let (outcome, lines) = global_capture::capture(|| {
            rt.block_on(handle_embedding_refit(
                &db,
                "job-1",
                &user,
                &payload,
                "2026-07-07T12:00:00.000Z",
            ))
        });
        outcome.unwrap();
        assert!(
            lines
                .iter()
                .any(|l| l.contains("Error finding all entities")
                    && l.contains("collection=help_docs")),
            "{lines:?}"
        );
        assert!(
            !lines.iter().any(|l| l.contains("Failed to load help docs")),
            "{lines:?}"
        );
        assert!(
            lines.iter().any(
                |l| l.contains("[EmbeddingRefit] Vocabulary fitted and saved")
                    && l.contains("documentCount=1")
            ),
            "{lines:?}"
        );
        assert!(
            !lines.iter().any(|l| l.contains("Enqueued reindex job")),
            "triggerReindex false: {lines:?}"
        );
        let stamped = db
            .read_main(move |c| crate::db::tfidf_vocabulary::find_by_profile_id(c, &profile))
            .unwrap()
            .unwrap()
            .text_shape_version;
        assert_eq!(stamped, LEXICAL_TEXT_SHAPE_VERSION as f64);
    }

    /// Inside the strict scope the base read hands its error back and the
    /// handler's own WARN fires (v4's catch), the fit going on without them.
    #[test]
    fn inside_the_strict_scope_the_help_docs_warn_fires() {
        global_capture::install();
        let (_dir, db, profile, user) = instance(true);
        let payload = EmbeddingRefitPayload {
            profile_id: profile,
            trigger_reindex: false,
        };
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        let (outcome, lines) = global_capture::capture(|| {
            crate::db::fallback::with_strict_repository_failures(|| {
                rt.block_on(handle_embedding_refit(
                    &db,
                    "job-2",
                    &user,
                    &payload,
                    "2026-07-07T12:00:00.000Z",
                ))
            })
        });
        outcome.unwrap();
        assert!(
            lines.iter().any(|l| l.starts_with("WARN")
                && l.contains(
                    "[EmbeddingRefit] Failed to load help docs for corpus, continuing without them"
                )
                && l.contains("context=handleEmbeddingRefit")
                && l.contains("error=no such table: help_docs")),
            "{lines:?}"
        );
    }
}
