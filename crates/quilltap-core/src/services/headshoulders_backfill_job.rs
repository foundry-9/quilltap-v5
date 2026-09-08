//! The `CHARACTER_HEADSHOULDERS_BACKFILL` job handler (v4
//! `lib/background-jobs/handlers/character-headshoulders-backfill.ts`, P4.82) —
//! the last job type that still fell through [`super::job_runner`]'s
//! "recognized but not yet available" loud fallback.
//!
//! Generates the `headAndShouldersPrompt` physical-description variant for one
//! character that lacks one. The avatar generator PREFERS that variant (an
//! avatar is a head-and-shoulders crop), so backfilling it stops full-body
//! anatomy from leaking into avatar prompts and tripping image-provider
//! moderation. v4's startup scan
//! ([`super::headshoulders_backfill_enqueue`]) enqueues one of these per
//! existing character; new characters get the field at creation, from the
//! wizard or the AI import.
//!
//! ## The write is the WHOLE merged object, deliberately
//!
//! v4's own header says why, and the reason survives the port unchanged: the
//! vault's `renderPhysicalPromptsJson` re-renders EVERY key of
//! `physical-prompts.json`, so a partial `{headAndShouldersPrompt}` write would
//! null the other five tiers. This handler reads `physicalDescription` once,
//! merges the new key and a fresh `updatedAt` into it, and writes the complete
//! object back. The differential's happy-path case seeds a character with all
//! five other tiers populated precisely so dropping the spread reddens it.
//!
//! ## The one call goes through the wizard's own `generateField`
//!
//! v4 imports `buildContextPrompt`, `generateField` and
//! `HEAD_AND_SHOULDERS_PHYSICAL_PROMPT` from `character-wizard.service.ts`, so
//! this handler's completion is the wizard's completion — same temperature,
//! same `No response from model` refusal, same `CHARACTER_WIZARD` `llm_logs`
//! row. v5 reaches it through
//! [`crate::generators::wizard::generate_field_for_caller`], a call-shape
//! adapter over the one private implementation (the wizard's own families are
//! the proof that its path did not move). The seed text rides
//! `buildContextPrompt`'s `imageDescription` slot, which frames it as
//! physical-appearance-only grounding — v4's comment, carried.
//!
//! v4 does NOT pass `generateField`'s tenth argument (`profileParameters`), so
//! the wire carries none on this path; [`ProfileParametersNotPassed`] records
//! that as a named constant rather than a bare `None`.
//!
//! ## Two whitespace asymmetries, both v4's and both reproduced
//!
//! * `headAndShouldersPrompt` is "already filled" only when it TRIMS non-empty,
//!   so a whitespace-only value is treated as absent by this handler AND by the
//!   scan.
//! * the seed is trimmed here (`(a || b || …).trim()`) but NOT by the scan
//!   (`Boolean(a || b || …)`), so a whitespace-only `mediumPrompt` gets a job
//!   enqueued that then returns silently. Both sides of that asymmetry are
//!   pinned by fixture characters (`Blankseed Bartholomew`).
//!
//! ## Measured: the "Failed to select cheap LLM" arm is UNREACHABLE
//!
//! v4 wraps `selectCheapLLMFromProfiles(allProfiles, buildCheapLLMConfig(
//! chatSettings))` in a try/catch with its own warn sentence. Measured at the
//! `2f4254b42` baseline: `lib/llm/cheap-llm.ts` contains **no `throw` at all**,
//! `lib/llm/cheap-llm-user-selection.ts` contains none either, and neither
//! argument expression can throw (`chatSettings?.cheapLLMSettings` is
//! optional-chained; `allProfiles.find` is an array method). The catch is dead
//! code in v4. [`SELECT_FAILED_UNREACHABLE`] keeps the sentence for the record;
//! no v5 code path can reach it, and none is invented to.
//!
//! ## Measured: v4's per-call-site log-failure warn needs no port
//!
//! v4's `generateField` ends its fire-and-forget `logLLMCall` with a `.catch`
//! that warns `Failed to log character wizard LLM call` — because v4's
//! `logLLMCall` REJECTS. v5's [`crate::services::llm_logging::log_llm_call`]
//! never propagates: its own `Err` arm already narrates the failure at `error`
//! level (louder than v4's `warn`) with the error message, once, for every
//! caller. So the shared `generate_field`'s `let _ =` drops nothing, and a
//! second warn here would double-log. Recorded rather than ported.
//!
//! ## Model boundary (tier-3 seam)
//!
//! [`CompletionProvider`], through the wizard adapter. `now_ms` is injected
//! (the `TitleUpdateHandler` precedent) so the `updatedAt` this handler writes
//! into the vault is diffable.

use serde_json::{Map, Value};

use crate::cheap_llm::CheapLlmSelection;
use crate::clock::iso_from_unix_ms;
use crate::db::background_jobs::BackgroundJob;
use crate::db::runtime::Db;
use crate::db::{characters_read, connection_profiles, DbError};
use crate::generators::wizard::{build_context_prompt, generate_field_for_caller};
use crate::generators::wizard_prompts::HEAD_AND_SHOULDERS_PHYSICAL_PROMPT;
use crate::jsstr::js_trim;
use crate::model::completion::CompletionProvider;
use crate::services::api_key_service::get_api_key_for_cheap_llm_selection;
use crate::services::image_job_common::{build_cheap_llm_selection, with_both_conns};
use crate::services::job_runner::{JobFuture, JobHandler, JobOutcome};

/// v4's `CONTEXT` (`character-headshoulders-backfill.ts:38`) — byte-exact,
/// because it is what an operator greps `combined.log` for.
pub(crate) const CONTEXT: &str = "background-jobs.headshoulders-backfill";

/// v4's `maxTokens` for the one call.
const MAX_TOKENS: i64 = 350;

/// v4's `content.substring(0, 500)` cap, in UTF-16 units.
const PROMPT_CAP_UTF16: usize = 500;

/// v4 passes NO tenth argument to `generateField`, so `profileParameters` is
/// `undefined` and the wire carries none. Named so the omission reads as the
/// deliberate transcription it is rather than a forgotten field.
const PROFILE_PARAMETERS_NOT_PASSED: Option<Value> = None;

/// v4's `[HeadShouldersBackfill] Failed to select cheap LLM, skipping` — kept
/// for the record only. See the module header: the arm is dead code in v4, so
/// nothing here can emit it.
#[allow(dead_code)]
pub(crate) const SELECT_FAILED_UNREACHABLE: &str =
    "[HeadShouldersBackfill] Failed to select cheap LLM, skipping";

/// JS `String.prototype.substring(0, n)` — UTF-16 units, not chars or bytes.
/// (The sixth copy of this three-line helper in the tree; each sits with the
/// module whose cap it implements, and the consolidation is a standing
/// recorded candidate rather than this lane's business.)
fn utf16_prefix(s: &str, n: usize) -> String {
    String::from_utf16_lossy(&s.encode_utf16().take(n).collect::<Vec<u16>>())
}

/// The decoded `CHARACTER_HEADSHOULDERS_BACKFILL` payload (v4
/// `CharacterHeadShouldersBackfillPayload`).
#[derive(Clone, Debug, Default)]
pub struct HeadShouldersBackfillPayload {
    pub character_id: String,
}

impl HeadShouldersBackfillPayload {
    pub fn from_json(payload: &Value) -> Self {
        Self {
            character_id: payload
                .get("characterId")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string(),
        }
    }
}

/// v4's seed fall-through: `(pd.mediumPrompt || pd.shortPrompt || pd.longPrompt
/// || pd.completePrompt || pd.fullDescription || '').trim()`.
///
/// JS `||` on strings, so an EMPTY string falls through to the next tier — and
/// the trim happens ONCE, on the winner, never per-candidate. A whitespace-only
/// `mediumPrompt` therefore WINS the fall-through and then trims to nothing,
/// which is the arm the scan disagrees with.
fn seed_text(pd: &Value) -> String {
    const TIERS: [&str; 5] = [
        "mediumPrompt",
        "shortPrompt",
        "longPrompt",
        "completePrompt",
        "fullDescription",
    ];
    for key in TIERS {
        if let Some(s) = pd.get(key).and_then(Value::as_str) {
            if !s.is_empty() {
                return js_trim(s).to_string();
            }
        }
    }
    String::new()
}

/// v4's `pd.headAndShouldersPrompt && pd.headAndShouldersPrompt.trim()` — the
/// same "present and not all whitespace" test the scan uses.
pub(crate) fn head_prompt_is_filled(pd: &Value) -> bool {
    pd.get("headAndShouldersPrompt")
        .and_then(Value::as_str)
        .is_some_and(|s| !js_trim(s).is_empty())
}

/// v4 `handleCharacterHeadShouldersBackfill`.
///
/// `Ok(())` is a job SUCCESS — including every skip arm (v4 returns, it does not
/// throw). `Err` fails the job, which is what a thrown `generateField` does:
/// the runner's backoff retries it, and the enqueuer asks for `maxAttempts: 3`
/// precisely so a cold provider gets more than one shot.
pub async fn handle_headshoulders_backfill<CMP: CompletionProvider>(
    db: &Db,
    completion: &CMP,
    job_id: &str,
    user_id: &str,
    payload: &HeadShouldersBackfillPayload,
    now_ms: i64,
) -> Result<(), String> {
    let cid = payload.character_id.clone();
    let character = with_both_conns(db, move |main, mount| {
        characters_read::find_by_id(main, mount, &cid)
    })
    .await
    .map_err(|e| e.to_string())?;

    let Some(character) = character else {
        tracing::info!(
            context = CONTEXT,
            job_id = %job_id,
            character_id = %payload.character_id,
            "[HeadShouldersBackfill] Character not found, skipping"
        );
        return Ok(());
    };

    // v4 `if (!pd) return;` — SILENT. Reachable only for a vault-less
    // character: a linked vault always scaffolds `physical-prompts.json`, so
    // the overlay hands back an object (see the fixture builder's measurement).
    let Some(pd) = character
        .get("physicalDescription")
        .filter(|v| !v.is_null())
    else {
        return Ok(());
    };

    // Idempotent: another path (or a prior attempt) may have filled it.
    if head_prompt_is_filled(pd) {
        return Ok(());
    }

    let seed = seed_text(pd);
    if seed.is_empty() {
        return Ok(());
    }

    // The profile read stays OUTSIDE the guard on purpose (v4's comment at
    // `:83-85`): a failing read is the job's failure to REPORT, and only a
    // failing selection would be worth skipping over — except that, as the
    // module header measures, v4's selection cannot fail.
    // v4's `repos.chatSettings.findByUserId` is FALLBACK-mode `safeQuery(…,
    // null)` (`chat-settings.repository.ts:38-45`): a failing read logs
    // `Error finding chat settings by user ID` and yields `null`, so
    // `buildCheapLLMConfig(undefined)` is the default config and the job
    // PROCEEDS. Only the connections read below throws (base `findByFilter`).
    // The §3 unification review caught this arm failing the job instead —
    // the P4.48 `safeQuery` class.
    let uid = user_id.to_string();
    let chat_settings = match db
        .read_main(move |conn| crate::db::chat_settings::find_by_user_id(conn, &uid))
    {
        Ok(v) => v,
        Err(e) => {
            tracing::error!(
                user_id = %user_id,
                error = %e,
                "Error finding chat settings by user ID"
            );
            None
        }
    };
    let uid = user_id.to_string();
    let all_profiles = db
        .read_main(move |conn| connection_profiles::find_by_user_id(conn, &uid))
        .map_err(|e| e.to_string())?;

    let cheap_settings = chat_settings
        .as_ref()
        .and_then(|cs| cs.get("cheapLLMSettings"));
    let Some(selection) = build_cheap_llm_selection(&all_profiles, cheap_settings) else {
        tracing::warn!(
            context = CONTEXT,
            job_id = %job_id,
            character_id = %payload.character_id,
            "[HeadShouldersBackfill] No connection profile configured, skipping"
        );
        return Ok(());
    };

    let uid = user_id.to_string();
    let sel = selection.clone();
    let api_key = db
        .read_main(move |conn| get_api_key_for_cheap_llm_selection(conn, &sel, &uid))
        .map_err(|e| e.to_string())?;
    let Some(api_key) = api_key else {
        tracing::warn!(
            context = CONTEXT,
            job_id = %job_id,
            "[HeadShouldersBackfill] No API key for cheap LLM selection, skipping"
        );
        return Ok(());
    };
    // v4 resolves the key and hands it to `generateField`; v5's completion
    // boundary resolves the key itself from the profile, so the value is only
    // consulted for the null gate above. Bound so the shape stays visible.
    let _ = &api_key;

    let content = generate_one(db, completion, &selection, &character, &seed, user_id).await?;

    let head_and_shoulders_prompt = js_trim(&utf16_prefix(&content, PROMPT_CAP_UTF16)).to_string();
    if head_and_shoulders_prompt.is_empty() {
        tracing::warn!(
            context = CONTEXT,
            job_id = %job_id,
            character_id = %payload.character_id,
            "[HeadShouldersBackfill] Model returned empty text, skipping write"
        );
        return Ok(());
    }

    // Write the COMPLETE merged object so the JSON re-render keeps the other
    // tiers (v4's own warning — see the module header).
    let mut merged: Map<String, Value> = match pd {
        Value::Object(o) => o.clone(),
        _ => Map::new(),
    };
    merged.insert(
        "headAndShouldersPrompt".into(),
        Value::String(head_and_shoulders_prompt.clone()),
    );
    merged.insert("updatedAt".into(), Value::String(iso_from_unix_ms(now_ms)));

    let mut patch = Map::new();
    patch.insert("physicalDescription".into(), Value::Object(merged));
    let cid = payload.character_id.clone();
    with_both_conns(db, move |main, mount| {
        crate::db::vault_character_update::update_character(main, mount, &cid, &patch)
            .map_err(crate::db::document_store_overlay::OverlayError::into_db)
            .map(|_| ())
    })
    .await
    .map_err(|e: DbError| e.to_string())?;

    tracing::info!(
        context = CONTEXT,
        job_id = %job_id,
        character_id = %payload.character_id,
        length = crate::jsstr::utf16_len(&head_and_shoulders_prompt),
        "[HeadShouldersBackfill] Populated head-and-shoulders prompt"
    );
    Ok(())
}

/// The one completion: v4's `buildContextPrompt(character.name, '', undefined,
/// seedText)` then `generateField(..., HEAD_AND_SHOULDERS_PHYSICAL_PROMPT,
/// 350, userId, characterId, selection.provider)`.
async fn generate_one<CMP: CompletionProvider>(
    db: &Db,
    completion: &CMP,
    selection: &CheapLlmSelection,
    character: &Value,
    seed: &str,
    user_id: &str,
) -> Result<String, String> {
    let character_name = character
        .get("name")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let character_id = character.get("id").and_then(Value::as_str);
    let context_prompt = build_context_prompt(character_name, "", None, Some(seed), None);
    generate_field_for_caller(
        db,
        completion,
        &selection.provider,
        selection.base_url.as_deref(),
        &selection.model_name,
        &context_prompt,
        HEAD_AND_SHOULDERS_PHYSICAL_PROMPT,
        MAX_TOKENS,
        user_id,
        character_id,
        PROFILE_PARAMETERS_NOT_PASSED,
    )
    .await
}

/// The `CHARACTER_HEADSHOULDERS_BACKFILL` [`JobHandler`] — a payload decode
/// around [`handle_headshoulders_backfill`]. The host builds one per job so
/// `now_ms` is the wall clock at job time (the `TitleUpdateHandler`
/// precedent).
pub struct CharacterHeadShouldersBackfillHandler<CMP> {
    pub completion: CMP,
    pub now_ms: i64,
}

impl<CMP> JobHandler for CharacterHeadShouldersBackfillHandler<CMP>
where
    CMP: CompletionProvider + Send + Sync,
{
    fn handle<'a>(&'a self, db: &'a Db, job: &'a BackgroundJob) -> JobFuture<'a> {
        Box::pin(async move {
            let payload: Value = serde_json::from_str(&job.payload).unwrap_or(Value::Null);
            let decoded = HeadShouldersBackfillPayload::from_json(&payload);
            match handle_headshoulders_backfill(
                db,
                &self.completion,
                &job.id,
                &job.user_id,
                &decoded,
                self.now_ms,
            )
            .await
            {
                Ok(()) => JobOutcome::Completed(None),
                Err(message) => JobOutcome::Failed(message),
            }
        })
    }
}
