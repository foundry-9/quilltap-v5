//! Character-voiced announcement rewriter — v4
//! `lib/services/announcer/character-voiced.ts`
//! (`generateCharacterVoicedAnnouncement`).
//!
//! Given a seed text the operator typed for an off-scene character and a chosen
//! connection profile, this rewrites the seed in the character's own voice. The
//! character is told they are NOT in the chat — they are speaking in character to
//! the people who are. The system gives them their normal identity stack (no
//! scenario, no tools, no roleplay template), runs a Commonplace Book recall
//! against the seed so they bring in relevant memories, and lists the currently
//! active or silent participants so the character has an audience to address. The
//! result is returned to the caller for the operator to review, edit, regenerate,
//! or post. **Nothing is persisted by this service.**
//!
//! When the operator has chosen a whisper audience, the roster is replaced by that
//! audience and the character is told the remark is private. A line pitched to a
//! full room reads wrong when only one person hears it, so the audience has to
//! reach the rewrite rather than being applied after the fact.
//!
//! Every dependency was already ported; this is composition, not new subsystem
//! work:
//!
//! | v4 import | v5 |
//! | --- | --- |
//! | `executeCheapLLMTask` | [`CheapLlmTaskExecutor::execute`] |
//! | `buildSystemPrompt` | [`crate::system_prompt::build_system_prompt`] |
//! | `searchMemoriesSemantic` | [`crate::services::memory_service::search_memories_semantic`] |
//! | `formatDynamicMemoryHead` | [`crate::memory_injector::format_dynamic_memory_head`] |
//! | `buildCommonplaceLLMContext` | [`crate::services::commonplace_notifications::build_commonplace_llm_context`] |
//!
//! **No uncensored fallback and no `cheapLLMSettings`.** v4 calls
//! `executeCheapLLMTask(…, undefined /* uncensoredFallback */, 2048,
//! character.id)` and builds its selection straight off the CHOSEN connection
//! profile — the P4.15 all-profiles/danger threading does not apply here, because
//! the operator picked the profile in the dialog.
//!
//! Pinned by `announcer_tier3_equivalence`.

use serde_json::Value;

use crate::cheap_llm::CheapLlmSelection;
use crate::db::runtime::Db;
use crate::jsstr::js_trim;
use crate::memory_injector::{format_dynamic_memory_head, InjectorResult, RecallAdjustment};
use crate::model::completion::{CompletionMessage, CompletionProvider, CompletionRole};
use crate::model::embedding::EmbeddingProvider;
use crate::services::cheap_llm_exec::CheapLlmTaskExecutor;
use crate::services::cheap_llm_exec::CheapLlmTaskOptions;
use crate::services::commonplace_notifications::{build_commonplace_llm_context, CommonplaceParts};
use crate::services::memory_service::{
    search_memories_semantic, SemanticSearchOptions, SemanticSearchResult,
};
use crate::system_prompt::{
    build_system_prompt, BuildSystemPromptOptions, Character, PhysicalDescription, Pronouns,
    ScenarioEntry, SystemPromptEntry,
};

/// v4 `TASK_TYPE`.
const TASK_TYPE: &str = "announcement-rewrite";

/// v4 `CharacterVoicedAnnouncementParams` — `character` and `profile` arrive as
/// the route's already-resolved rows (vault-overlaid character, raw profile).
pub struct CharacterVoicedAnnouncementParams<'a> {
    pub chat_id: &'a str,
    pub character: &'a Value,
    pub profile: &'a Value,
    pub seed_markdown: &'a str,
    pub system_prompt_id: Option<&'a str>,
    pub user_id: &'a str,
    /// v4 `audienceNames` — display names of the whisper audience the operator
    /// has chosen, already resolved and verified
    /// ([`super::audience::resolve_announcement_audience`]). Empty means the
    /// announcement is public and the character addresses the whole room.
    pub audience_names: &'a [String],
    /// The injected `Date.now()` (ms) the memory blend's time-decay reads.
    pub now_ms: f64,
}

/// v4 `CharacterVoicedAnnouncementResult`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CharacterVoicedAnnouncementResult {
    pub success: bool,
    pub proposed_markdown: String,
    pub error: Option<String>,
}

// ---------------------------------------------------------------------------
// Small JSON readers (the `to_carina_character` convention — each service keeps
// its own projection of the character row)
// ---------------------------------------------------------------------------

fn s(v: &Value, k: &str) -> Option<String> {
    v.get(k).and_then(Value::as_str).map(str::to_string)
}
fn b(v: &Value, k: &str) -> Option<bool> {
    v.get(k).and_then(Value::as_bool)
}
fn str_array(v: &Value, k: &str) -> Vec<String> {
    v.get(k)
        .and_then(Value::as_array)
        .map(|a| {
            a.iter()
                .filter_map(Value::as_str)
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default()
}

/// Project the vault-overlaid character row into the prompt builder's subset.
fn to_sys_character(c: &Value) -> Character {
    let pronouns = c.get("pronouns").and_then(|p| {
        Some(Pronouns {
            subject: s(p, "subject")?,
            object: s(p, "object")?,
            possessive: s(p, "possessive")?,
        })
    });
    let physical_description = c.get("physicalDescription").and_then(|p| {
        Some(PhysicalDescription {
            name: s(p, "name")?,
            usage_context: s(p, "usageContext"),
            short_prompt: s(p, "shortPrompt"),
            medium_prompt: s(p, "mediumPrompt"),
            long_prompt: s(p, "longPrompt"),
            complete_prompt: s(p, "completePrompt"),
            full_description: s(p, "fullDescription"),
        })
    });
    Character {
        name: s(c, "name").unwrap_or_default(),
        title: s(c, "title"),
        identity: s(c, "identity"),
        description: s(c, "description"),
        manifesto: s(c, "manifesto"),
        personality: s(c, "personality"),
        aliases: str_array(c, "aliases"),
        pronouns,
        physical_description,
        example_dialogues: s(c, "exampleDialogues"),
        scenarios: c
            .get("scenarios")
            .and_then(Value::as_array)
            .map(|a| {
                a.iter()
                    .map(|sc| ScenarioEntry {
                        content: s(sc, "content").unwrap_or_default(),
                        archived: sc.get("archived") == Some(&Value::Bool(true)),
                    })
                    .collect()
            })
            .unwrap_or_default(),
        system_prompts: c
            .get("systemPrompts")
            .and_then(Value::as_array)
            .map(|a| {
                a.iter()
                    .map(|sp| SystemPromptEntry {
                        id: s(sp, "id").unwrap_or_default(),
                        content: s(sp, "content").unwrap_or_default(),
                        is_default: b(sp, "isDefault").unwrap_or(false),
                    })
                    .collect()
            })
            .unwrap_or_default(),
    }
}

/// Project a semantic-search result into the injector shape the memory formatter
/// reads (mirrors `build_context`'s private `injector_result_from_search`).
fn injector_result_from_search(r: &SemanticSearchResult) -> InjectorResult {
    InjectorResult {
        memory: crate::services::carina_query::injector_memory_from_json(&r.memory),
        score: r.score,
        effective_weight: Some(r.effective_weight),
        raw_weight: Some(r.raw_weight),
        recall_adjustment: r.recall_adjustment.as_ref().map(|a| RecallAdjustment {
            multiplier: Some(a.multiplier),
            fired: a.fired.clone(),
            blended_before: Some(a.blended_before),
            blended_after: Some(a.blended_after),
        }),
    }
}

/// v4 `buildSelection(profile)` — straight off the operator's chosen profile.
/// The profile's provider params (e.g. DeepSeek thinking mode) are forwarded so
/// per-model settings take effect for this utility call too.
fn build_selection(profile: &Value) -> CheapLlmSelection {
    let provider = s(profile, "provider").unwrap_or_default();
    CheapLlmSelection {
        is_local: provider == "OLLAMA",
        provider,
        model_name: s(profile, "modelName").unwrap_or_default(),
        // JS `profile.baseUrl || undefined` — the empty string is falsy.
        base_url: s(profile, "baseUrl").filter(|u| !u.is_empty()),
        connection_profile_id: s(profile, "id"),
        // v4 `d9c5a1c7` converted this inline construction to the shared
        // `profileParams()` helper, so the Ollama `num_ctx` injection applies
        // to this utility call too.
        profile_parameters: crate::cheap_llm::profile_params_value(profile),
    }
}

/// v4 `formatNameList` — `"Alice"`, `"Alice and Bob"`, `"Alice, Bob, and Carol"`.
/// (The zero-length case is unreachable: every call site guards on a non-empty
/// audience, and v4's own `names[names.length - 1]` would be `undefined` there.)
fn format_name_list(names: &[String]) -> String {
    match names {
        [] => String::new(),
        [one] => one.clone(),
        [a, b] => format!("{a} and {b}"),
        [head @ .., last] => format!("{}, and {last}", head.join(", ")),
    }
}

/// v4 `buildRoster` — the present-roster block from a chat's participants.
/// Includes only participants whose status is `active` or `silent` (not `absent`
/// or `removed`). The off-scene speaking character is filtered out in case they
/// also appear as a participant. An unknown chat, or an empty roster, is `""`.
fn build_roster(db: &Db, chat_id: &str, speaking_character_id: &str) -> String {
    let cid = chat_id.to_string();
    let Ok(Some(chat)) = db.read_main(move |c| crate::db::chats_read::find_by_id(c, &cid)) else {
        return String::new();
    };
    let roster: Vec<(String, String)> = chat
        .get("participants")
        .and_then(Value::as_array)
        .map(|ps| {
            ps.iter()
                .filter(|p| {
                    p.get("type").and_then(Value::as_str) == Some("CHARACTER")
                        && matches!(
                            p.get("status").and_then(Value::as_str),
                            Some("active") | Some("silent")
                        )
                        && p.get("characterId").and_then(Value::as_str)
                            != Some(speaking_character_id)
                })
                .map(|p| {
                    (
                        p.get("characterId")
                            .and_then(Value::as_str)
                            .unwrap_or_default()
                            .to_string(),
                        p.get("status")
                            .and_then(Value::as_str)
                            .unwrap_or_default()
                            .to_string(),
                    )
                })
                .collect()
        })
        .unwrap_or_default();

    if roster.is_empty() {
        return String::new();
    }

    let entries: Vec<String> = roster
        .into_iter()
        .map(|(character_id, status)| {
            // v4 `repos.characters.findById` (the overlay finder), then
            // `character?.name?.trim() || 'Someone'`.
            let name = db
                .read_main(|main| {
                    db.read_mount_index(|mount| {
                        crate::db::characters_read::find_by_id(main, mount, &character_id)
                    })
                })
                .ok()
                .flatten()
                .and_then(|c| s(&c, "name"))
                .map(|n| js_trim(&n).to_string())
                .filter(|n| !n.is_empty())
                .unwrap_or_else(|| "Someone".to_string());
            format!("- {name} ({status})")
        })
        .collect();
    entries.join("\n")
}

/// The rewrite instruction v4 appends after the presence line — one template
/// literal, carried byte-for-byte.
const REWRITE_INSTRUCTION: &str = "\n\nBelow is your own rough draft — the meaning and substance of what you want to convey. Rewrite it in your own voice, the way you would actually say it given your personality, manner of speech, and current circumstances. Keep the meaning, the addressees, and any specific facts; refine the voice and phrasing. Narration, action, and stage directions are welcome where they fit your voice. Do not respond to the draft — it is yours.\n\nDraft:";

/// v4's presence line when nobody is present.
const PRESENCE_ALONE: &str = "You want to say something to the people in the chat.";

/// v4 `generateCharacterVoicedAnnouncement`. Never throws — v4 wraps the whole
/// body in try/catch and returns `{success:false, proposedMarkdown:'', error}`.
pub async fn generate_character_voiced_announcement<C, E>(
    db: &Db,
    completion: &C,
    embedding: &E,
    executor: &CheapLlmTaskExecutor,
    params: &CharacterVoicedAnnouncementParams<'_>,
) -> CharacterVoicedAnnouncementResult
where
    C: CompletionProvider,
    E: EmbeddingProvider,
{
    let character_id = s(params.character, "id").unwrap_or_default();
    // v4 `audienceNames?.filter(n => n.trim().length > 0) ?? []`.
    let whisper_audience: Vec<String> = params
        .audience_names
        .iter()
        .filter(|n| !js_trim(n).is_empty())
        .cloned()
        .collect();
    let selection = build_selection(params.profile);

    // System prompt: identity stack only — no roleplay template, no tools.
    let sys_character = to_sys_character(params.character);
    // v4 passes ONLY `{ character, selectedSystemPromptId }` — every other option
    // is `undefined`, so no roleplay template, no tool instructions, no scenario,
    // and the `{{timestamp}}` path never fires. That includes
    // `standingInstructions` (v4 `8f868109`): verified per call site at the
    // P4.D103 port — `lib/services/announcer/character-voiced.ts:119` imports the
    // shared builder but does not pass the option, so the announcer is
    // structurally excluded from the section exactly as it is from Taboo.
    let system_prompt = match build_system_prompt(&BuildSystemPromptOptions {
        character: &sys_character,
        user_character: None,
        roleplay_template: None,
        tool_instructions: None,
        selected_system_prompt_id: params.system_prompt_id,
        timestamp_config: None,
        is_initial_message: false,
        timezone: None,
        scenario_text: None,
        precompiled_identity_stack: None,
        // v4 omits `tabooPhrases` here — the character-voiced announcer is not a
        // conversational turn, so it carries no Taboo section (`7df7de8e`).
        taboo_phrases: None,
        standing_instructions: None,
        now_ms: 0,
        local_offset_minutes: 0,
    }) {
        Ok(p) => p,
        // v4's `buildSystemPrompt` only throws on the `{{timestamp}}` zone path,
        // which this call never enables; a throw here would land in v4's outer
        // catch, so mirror that shape rather than panicking.
        Err(e) => {
            return CharacterVoicedAnnouncementResult {
                success: false,
                proposed_markdown: String::new(),
                error: Some(e.to_string()),
            }
        }
    };

    // Commonplace recall against the seed text. **Memory recall failure should
    // not block the rewrite** — v4 catches and proceeds without (it only warns).
    let mut recall_text = String::new();
    let memory_results = search_memories_semantic(
        db,
        embedding,
        &character_id,
        params.seed_markdown,
        &SemanticSearchOptions {
            limit: Some(20),
            min_importance: Some(0.3),
            now_ms: params.now_ms,
            ..Default::default()
        },
        None,
    )
    .await;
    match memory_results {
        Ok(results) if !results.is_empty() => {
            let injector: Vec<InjectorResult> =
                results.iter().map(injector_result_from_search).collect();
            // The recall spans the character's whole store, so it carries their
            // memories about other people too; attribute them or the rewrite
            // reads someone else's life as its own (v4 `d883a5ee1`, bug 122).
            let subject = crate::services::memory_subject::build_memory_subject_context(
                db,
                &character_id,
                injector.iter().map(|r| r.memory.about_character_id.clone()),
            );
            let formatted =
                format_dynamic_memory_head(&injector, None, Some(12), params.now_ms, &subject);
            if !formatted.content.is_empty() {
                recall_text = build_commonplace_llm_context(&CommonplaceParts {
                    relevant: Some(formatted.content),
                    ..Default::default()
                });
            }
        }
        Ok(_) => {}
        Err(_) => { /* v4 logs a warning and proceeds without recall. */ }
    }

    // Who's listening. A whisper's audience IS the audience — the room's wider
    // roster is not merely irrelevant to it, it would mislead the rewrite into
    // pitching a private aside at people who will never hear it. (v4 short-circuits
    // the `&&`, so `buildRoster` is not even called for a whisper — no chat read,
    // no character reads.)
    let roster = if whisper_audience.is_empty() {
        build_roster(db, params.chat_id, &character_id)
    } else {
        String::new()
    };

    // Compose the user-role message.
    let seed_trimmed = js_trim(params.seed_markdown).to_string();
    let mut user_parts: Vec<String> = Vec::new();
    if !recall_text.is_empty() {
        user_parts.push(recall_text);
    }
    let presence_line = if !whisper_audience.is_empty() {
        // v4's whisper arm, carried byte-for-byte (incl. the em-dash and the
        // singular/plural `them` / `those named` swap).
        format!(
            "You want to say something privately to {} — others are present in the chat, but this remark is for {} alone and no one else will hear it. Pitch it as a private aside, not a declaration to the room.",
            format_name_list(&whisper_audience),
            if whisper_audience.len() == 1 { "them" } else { "those named" },
        )
    } else if roster.is_empty() {
        PRESENCE_ALONE.to_string()
    } else {
        format!(
            "You want to say something to the people in the chat. The following people are present:\n{roster}"
        )
    };
    user_parts.push(format!("{presence_line}{REWRITE_INSTRUCTION}"));
    user_parts.push(seed_trimmed);

    let messages = vec![
        CompletionMessage {
            role: CompletionRole::System,
            content: system_prompt,
        },
        CompletionMessage {
            role: CompletionRole::User,
            content: user_parts.join("\n\n"),
        },
    ];

    let llm_result = executor
        .execute(
            completion,
            &selection,
            messages,
            |content: &str| js_trim(content).to_string(),
            // v4 passes `undefined` for the uncensored fallback here.
            None,
            Some(2048.0),
            Some(&character_id),
            Some(TASK_TYPE),
            CheapLlmTaskOptions::default(),
        )
        .await;

    // v4: `if (!llmResult.success || !llmResult.result)` — an empty-string result
    // is falsy in JS, so a whitespace-only completion takes the failure arm too.
    let result = llm_result.result.filter(|r| !r.is_empty());
    match (llm_result.success, result) {
        (true, Some(proposed)) => CharacterVoicedAnnouncementResult {
            success: true,
            proposed_markdown: proposed,
            error: None,
        },
        _ => CharacterVoicedAnnouncementResult {
            success: false,
            proposed_markdown: String::new(),
            error: Some(
                llm_result
                    .error
                    .filter(|e| !e.is_empty())
                    .unwrap_or_else(|| "The LLM returned no content.".to_string()),
            ),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// v4 `formatNameList` — the Oxford comma is v4's, not ours.
    #[test]
    fn name_list_is_oxford_comma_joined() {
        let n = |xs: &[&str]| -> Vec<String> { xs.iter().map(|s| s.to_string()).collect() };
        assert_eq!(format_name_list(&n(&["Alice"])), "Alice");
        assert_eq!(format_name_list(&n(&["Alice", "Bob"])), "Alice and Bob");
        assert_eq!(
            format_name_list(&n(&["Alice", "Bob", "Carol"])),
            "Alice, Bob, and Carol"
        );
        assert_eq!(
            format_name_list(&n(&["Alice", "Bob", "Carol", "Dan"])),
            "Alice, Bob, Carol, and Dan"
        );
    }
}
