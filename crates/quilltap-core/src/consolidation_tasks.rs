//! Consolidation task — one LLM call per memory cluster. v4
//! `lib/memory/cheap-llm-tasks/consolidation-tasks.ts` (`d58548051`,
//! byte-identical at the `01a83539d` pin).
//!
//! The consolidation job ([`crate::services::consolidation`]) hands this task a
//! cluster of a character's memories about one subject; the model binds them
//! into a few digest entries and says which notes are too distinct to fold
//! (memory-consolidation-and-tiers.md §C4).
//!
//! v4's `ConsolidationOutputSchema` is the single source of truth for the
//! answer's shape; [`validate_consolidation_output`] reproduces its Zod 4 parse
//! over a `serde_json::Value` (the `.trim()` transforms, the `.default()`s, the
//! FIRST issue's path and message bytes) and then enforces the membership rules
//! the schema cannot express:
//!
//!   - every `digests[].memberIds` entry must be one of the input handles;
//!   - every input appears in exactly one of `digests[].memberIds` or
//!     `keepStandalone` — an input listed twice is a failure, an input listed
//!     nowhere is treated as `keepStandalone`;
//!   - contradictions naming an unknown handle are dropped.
//!
//! A schema or rule failure fails the whole cluster: the caller skips it and
//! writes nothing for it — never a partial write.
//!
//! Members are presented to the model as short handles (`m1`, `m2`, …) rather
//! than 36-character UUIDs: echoing a handle is far more reliable than echoing a
//! UUID, and the caller maps handles back to ids.
//!
//! Tier-1 exact against v4's REAL exports (`consolidation_tasks_equivalence`):
//! the system prompt (GENERATED — [`prompt_text`]), the user message, and
//! `parseConsolidationResponse` over every reason string and Zod first-issue
//! shape.
//!
//! **Recorded lossy class.** `oneLine`'s 1,200-unit cut through a surrogate
//! pair leaves v4 a lone surrogate; a Rust `String` cannot carry one, so v5's
//! [`crate::jsstr::utf16_truncate`] decodes the lone unit as U+FFFD (the
//! `commonplace_file` precedent).

pub mod prompt_text;

use serde_json::{json, Map, Value};

use crate::api::zod_issues::{join_path, key, ZodIssue};
use crate::cheap_llm::{CheapLlmSelection, UncensoredFallbackOptions};
use crate::generators::llm_json::{parse_llm_json_object, parse_llm_json_object_failure_message};
use crate::jsstr::{is_js_ws, js_trim, utf16_len, utf16_truncate};
use crate::model::completion::{CompletionMessage, CompletionProvider};
use crate::services::cheap_llm_exec::{
    CheapLlmTaskExecutor, CheapLlmTaskOptions, CheapLlmTaskResult,
};

pub use prompt_text::CONSOLIDATION_SYSTEM_PROMPT;

/// Task type recorded on the LLM log and the activity registry.
pub const CONSOLIDATION_TASK_TYPE: &str = "memory-consolidation";

/// Output-token ceiling for one cluster's answer (a 30-member cluster fits
/// comfortably).
pub const CONSOLIDATION_MAX_TOKENS: f64 = 4000.0;

/// Longest member text shown to the model, in characters (UTF-16 units).
pub const MEMBER_CONTENT_CHAR_CAP: usize = 1200;

// ============================================================================
// Schema — v4's single source of truth for the model's answer
// ============================================================================

/// v4 `ConsolidationDigestOutput` (the parsed, trimmed, defaulted digest).
#[derive(Debug, Clone, PartialEq)]
pub struct ConsolidationDigestOutput {
    /// Third person about the subject; first person ("I") for the self bucket.
    pub content: String,
    /// ≤ 12 words, lowercase, extractor style.
    pub summary: String,
    /// Free keywords plus the three targeting tags.
    pub keywords: Vec<String>,
    /// 0.2–1.0 by the prompt; clamped by the caller.
    pub importance: f64,
    /// `"semantic"` | `"episodic"`.
    pub kind: String,
    /// Episodic only: earliest member event time. The caller recomputes it
    /// from the members. `None` = the key was absent (Zod omits it);
    /// `Some(None)` = JSON `null`.
    pub occurred_at: Option<Option<String>>,
    /// Handles of the inputs this digest replaces (ids once resolved).
    pub member_ids: Vec<String>,
}

impl ConsolidationDigestOutput {
    /// The Zod output object in shape order (`occurredAt` only when present).
    pub fn to_json(&self) -> Value {
        let mut m = Map::new();
        m.insert("content".into(), json!(self.content));
        m.insert("summary".into(), json!(self.summary));
        m.insert("keywords".into(), json!(self.keywords));
        m.insert(
            "importance".into(),
            crate::db::js_number_to_json(self.importance),
        );
        m.insert("kind".into(), json!(self.kind));
        if let Some(o) = &self.occurred_at {
            m.insert("occurredAt".into(), json!(o));
        }
        m.insert("memberIds".into(), json!(self.member_ids));
        Value::Object(m)
    }
}

/// v4 `ConsolidationContradiction`.
#[derive(Debug, Clone, PartialEq)]
pub struct ConsolidationContradiction {
    pub older_id: String,
    pub newer_id: String,
    pub note: String,
}

impl ConsolidationContradiction {
    pub fn to_json(&self) -> Value {
        json!({ "olderId": self.older_id, "newerId": self.newer_id, "note": self.note })
    }
}

/// v4 `ValidatedConsolidation` — a validated answer, still in handle space.
#[derive(Debug, Clone, PartialEq)]
pub struct ValidatedConsolidation {
    pub digests: Vec<ConsolidationDigestOutput>,
    /// Explicit `keepStandalone` plus every input listed nowhere.
    pub keep_standalone: Vec<String>,
    /// Contradictions whose handles are both inputs.
    pub contradictions: Vec<ConsolidationContradiction>,
    /// Inputs the model listed nowhere (already folded into `keep_standalone`).
    pub unlisted: Vec<String>,
}

/// v4 `ConsolidationValidation`.
#[derive(Debug, Clone, PartialEq)]
pub enum ConsolidationValidation {
    Ok(ValidatedConsolidation),
    Invalid { reason: String },
}

impl ConsolidationValidation {
    /// v4's `{ ok: true, value }` | `{ ok: false, reason }`.
    pub fn to_json(&self) -> Value {
        match self {
            ConsolidationValidation::Ok(v) => json!({
                "ok": true,
                "value": {
                    "digests": v.digests.iter().map(ConsolidationDigestOutput::to_json).collect::<Vec<_>>(),
                    "keepStandalone": v.keep_standalone,
                    "contradictions": v.contradictions.iter().map(ConsolidationContradiction::to_json).collect::<Vec<_>>(),
                    "unlisted": v.unlisted,
                }
            }),
            ConsolidationValidation::Invalid { reason } => json!({ "ok": false, "reason": reason }),
        }
    }
}

/// The parsed output before the membership rules.
struct ParsedOutput {
    digests: Vec<ConsolidationDigestOutput>,
    keep_standalone: Vec<String>,
    contradictions: Vec<ConsolidationContradiction>,
}

type Path = Vec<Value>;

fn at(path: &Path, k: Value) -> Path {
    let mut p = path.clone();
    p.push(k);
    p
}

/// `z.string().trim().min(1)` — JS trim, then a UTF-16 length check.
fn trimmed_nonempty(v: Option<&Value>, path: Path) -> Result<String, Box<ZodIssue>> {
    match v {
        Some(Value::String(s)) => {
            let t = js_trim(s);
            if utf16_len(t) < 1 {
                Err(ZodIssue::too_small_string(json!(1), path).into())
            } else {
                Ok(t.to_string())
            }
        }
        other => Err(ZodIssue::invalid_type("string", path, other).into()),
    }
}

/// `z.array(z.string().trim().min(1))` (+ `.min(1)` when `min_one`); `None`
/// input answers the caller's default.
fn handle_array(v: &Value, path: &Path, min_one: bool) -> Result<Vec<String>, Box<ZodIssue>> {
    let Value::Array(items) = v else {
        return Err(ZodIssue::invalid_type("array", path.clone(), Some(v)).into());
    };
    let mut out = Vec::with_capacity(items.len());
    for (i, item) in items.iter().enumerate() {
        out.push(trimmed_nonempty(Some(item), at(path, json!(i)))?);
    }
    if min_one && items.is_empty() {
        // Zod 4's array `.min(1)` sentence (measured at the pin).
        return Err(
            ZodIssue::custom(path.clone(), "Too small: expected array to have >=1 items").into(),
        );
    }
    Ok(out)
}

fn parse_digest(v: &Value, path: Path) -> Result<ConsolidationDigestOutput, Box<ZodIssue>> {
    let Value::Object(o) = v else {
        return Err(ZodIssue::invalid_type("object", path, Some(v)).into());
    };
    let content = trimmed_nonempty(o.get("content"), at(&path, key("content")))?;
    let summary = trimmed_nonempty(o.get("summary"), at(&path, key("summary")))?;
    let keywords = match o.get("keywords") {
        None => Vec::new(),
        Some(Value::Array(items)) => {
            let mut out = Vec::with_capacity(items.len());
            for (i, item) in items.iter().enumerate() {
                match item {
                    Value::String(s) => out.push(s.clone()),
                    other => {
                        return Err(ZodIssue::invalid_type(
                            "string",
                            at(&at(&path, key("keywords")), json!(i)),
                            Some(other),
                        )
                        .into())
                    }
                }
            }
            out
        }
        Some(other) => {
            return Err(
                ZodIssue::invalid_type("array", at(&path, key("keywords")), Some(other)).into(),
            )
        }
    };
    let importance = match o.get("importance") {
        Some(Value::Number(n)) => {
            let x = n.as_f64().unwrap_or(f64::NAN);
            if x < 0.0 {
                return Err(
                    ZodIssue::too_small_number(json!(0), at(&path, key("importance"))).into(),
                );
            }
            if x > 1.0 {
                return Err(
                    ZodIssue::too_big_number(json!(1), at(&path, key("importance"))).into(),
                );
            }
            x
        }
        other => {
            return Err(
                ZodIssue::invalid_type("number", at(&path, key("importance")), other).into(),
            )
        }
    };
    let kind = match o.get("kind") {
        None => "semantic".to_string(),
        Some(Value::String(s)) if s == "semantic" || s == "episodic" => s.clone(),
        Some(_) => {
            return Err(
                ZodIssue::invalid_value(&["semantic", "episodic"], at(&path, key("kind"))).into(),
            )
        }
    };
    let occurred_at = match o.get("occurredAt") {
        None => None,
        Some(Value::Null) => Some(None),
        Some(Value::String(s)) => Some(Some(s.clone())),
        Some(other) => {
            return Err(
                ZodIssue::invalid_type("string", at(&path, key("occurredAt")), Some(other)).into(),
            )
        }
    };
    let member_ids = match o.get("memberIds") {
        None => {
            return Err(ZodIssue::invalid_type("array", at(&path, key("memberIds")), None).into());
        }
        Some(v) => handle_array(v, &at(&path, key("memberIds")), true)?,
    };
    Ok(ConsolidationDigestOutput {
        content,
        summary,
        keywords,
        importance,
        kind,
        occurred_at,
        member_ids,
    })
}

fn parse_contradiction(v: &Value, path: Path) -> Result<ConsolidationContradiction, Box<ZodIssue>> {
    let Value::Object(o) = v else {
        return Err(ZodIssue::invalid_type("object", path, Some(v)).into());
    };
    let older_id = trimmed_nonempty(o.get("olderId"), at(&path, key("olderId")))?;
    let newer_id = trimmed_nonempty(o.get("newerId"), at(&path, key("newerId")))?;
    let note = match o.get("note") {
        None => String::new(),
        Some(Value::String(s)) => s.clone(),
        Some(other) => {
            return Err(
                ZodIssue::invalid_type("string", at(&path, key("note")), Some(other)).into(),
            )
        }
    };
    Ok(ConsolidationContradiction {
        older_id,
        newer_id,
        note,
    })
}

/// `ConsolidationOutputSchema.safeParse(raw)` → the FIRST issue on failure
/// (Zod 4 walks the shape keys in order, array elements in index order).
fn parse_output(raw: &Value) -> Result<ParsedOutput, Box<ZodIssue>> {
    let Value::Object(o) = raw else {
        return Err(ZodIssue::invalid_type("object", Vec::new(), Some(raw)).into());
    };
    let digests = match o.get("digests") {
        Some(Value::Array(items)) => {
            let mut out = Vec::with_capacity(items.len());
            for (i, item) in items.iter().enumerate() {
                out.push(parse_digest(item, vec![key("digests"), json!(i)])?);
            }
            out
        }
        other => return Err(ZodIssue::invalid_type("array", vec![key("digests")], other).into()),
    };
    let keep_standalone = match o.get("keepStandalone") {
        None => Vec::new(),
        Some(v) => handle_array(v, &vec![key("keepStandalone")], false)?,
    };
    let contradictions = match o.get("contradictions") {
        None => Vec::new(),
        Some(Value::Array(items)) => {
            let mut out = Vec::with_capacity(items.len());
            for (i, item) in items.iter().enumerate() {
                out.push(parse_contradiction(
                    item,
                    vec![key("contradictions"), json!(i)],
                )?);
            }
            out
        }
        Some(other) => {
            return Err(
                ZodIssue::invalid_type("array", vec![key("contradictions")], Some(other)).into(),
            )
        }
    };
    Ok(ParsedOutput {
        digests,
        keep_standalone,
        contradictions,
    })
}

/// Validate a parsed answer against the schema and the membership rules.
/// Pure — v4 exports it for tests and the dry-run report.
pub fn validate_consolidation_output(
    raw: &Value,
    input_handles: &[String],
) -> ConsolidationValidation {
    let output = match parse_output(raw) {
        Ok(o) => o,
        Err(issue) => {
            let joined = join_path(issue.path());
            let path = if joined.is_empty() {
                "(root)".to_string()
            } else {
                joined
            };
            return ConsolidationValidation::Invalid {
                reason: format!("schema: {path} {}", issue.message()),
            };
        }
    };
    let inputs: std::collections::HashSet<&str> =
        input_handles.iter().map(String::as_str).collect();
    let mut seen: std::collections::HashMap<String, String> = std::collections::HashMap::new();

    for (d, digest) in output.digests.iter().enumerate() {
        for handle in &digest.member_ids {
            if !inputs.contains(handle.as_str()) {
                return ConsolidationValidation::Invalid {
                    reason: format!("digest {} names unknown member \"{handle}\"", d + 1),
                };
            }
            if let Some(prior) = seen.get(handle) {
                return ConsolidationValidation::Invalid {
                    reason: format!(
                        "member \"{handle}\" listed twice ({prior} and digest {})",
                        d + 1
                    ),
                };
            }
            seen.insert(handle.clone(), format!("digest {}", d + 1));
        }
    }

    let mut standalone: Vec<String> = Vec::new();
    for handle in &output.keep_standalone {
        if !inputs.contains(handle.as_str()) {
            return ConsolidationValidation::Invalid {
                reason: format!("keepStandalone names unknown member \"{handle}\""),
            };
        }
        if let Some(prior) = seen.get(handle) {
            if prior == "keepStandalone" {
                continue; // a repeat inside the same list is harmless
            }
            return ConsolidationValidation::Invalid {
                reason: format!("member \"{handle}\" listed twice ({prior} and keepStandalone)"),
            };
        }
        seen.insert(handle.clone(), "keepStandalone".to_string());
        standalone.push(handle.clone());
    }

    let unlisted: Vec<String> = input_handles
        .iter()
        .filter(|h| !seen.contains_key(h.as_str()))
        .cloned()
        .collect();

    let contradictions = output
        .contradictions
        .into_iter()
        .filter(|c| {
            inputs.contains(c.older_id.as_str())
                && inputs.contains(c.newer_id.as_str())
                && c.older_id != c.newer_id
        })
        .collect();

    let mut keep_standalone = standalone;
    keep_standalone.extend(unlisted.iter().cloned());
    ConsolidationValidation::Ok(ValidatedConsolidation {
        digests: output.digests,
        keep_standalone,
        contradictions,
        unlisted,
    })
}

// ============================================================================
// Prompt
// ============================================================================

/// v4 `ConsolidationMemberInput` — one member note as the model sees it.
#[derive(Debug, Clone, PartialEq)]
pub struct ConsolidationMemberInput {
    pub handle: String,
    /// Event time (occurredAt, else createdAt), ISO.
    pub when: Option<String>,
    pub importance: f64,
    pub reinforcement_count: f64,
    pub content: String,
}

/// v4's `'self' | 'other' | 'none'` bucket kind.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConsolidationBucketKind {
    SelfBucket,
    Other,
    None,
}

impl ConsolidationBucketKind {
    pub fn as_str(self) -> &'static str {
        match self {
            ConsolidationBucketKind::SelfBucket => "self",
            ConsolidationBucketKind::Other => "other",
            ConsolidationBucketKind::None => "none",
        }
    }
}

/// v4 `ConsolidationCallInput` — everything one consolidation call needs.
#[derive(Debug, Clone, PartialEq)]
pub struct ConsolidationCallInput {
    pub holder_name: String,
    /// Display name of the subject; the holder's own name for the self bucket.
    pub subject_name: String,
    /// self → first person; other → third person about the subject; none →
    /// general notes.
    pub bucket: ConsolidationBucketKind,
    /// `"semantic"` | `"episodic"`.
    pub cluster_kind: String,
    /// Rendered ALREADY ESTABLISHED block for the subject, when any.
    pub canon_block: Option<String>,
    /// The existing digest's content, when the cluster formed around one.
    pub existing_digest: Option<String>,
    /// Members, oldest first.
    pub members: Vec<ConsolidationMemberInput>,
}

fn voice_line(input: &ConsolidationCallInput) -> String {
    match input.bucket {
        ConsolidationBucketKind::SelfBucket => format!(
            "VOICE: first person — the subject is the holder, {}; write \"I …\".",
            input.holder_name
        ),
        ConsolidationBucketKind::Other => {
            format!("VOICE: third person — write about {} by name.", input.subject_name)
        }
        ConsolidationBucketKind::None => {
            "VOICE: third person — these notes have no single subject; name whoever each fact concerns."
                .to_string()
        }
    }
}

/// JS `text.replace(/\s*\n\s*/g, ' ')` — every maximal run of JS whitespace
/// that contains a `\n` collapses to one space; a run without one is kept.
fn collapse_newline_runs(text: &str) -> String {
    let chars: Vec<char> = text.chars().collect();
    let mut out = String::with_capacity(text.len());
    let mut i = 0;
    while i < chars.len() {
        if is_js_ws(chars[i]) {
            let start = i;
            while i < chars.len() && is_js_ws(chars[i]) {
                i += 1;
            }
            let run = &chars[start..i];
            match run.iter().position(|&c| c == '\n') {
                // The regex's leftmost match starts at the run's first
                // whitespace (`\s*` is greedy and backtracks to reach `\n`),
                // and its trailing `\s*` eats the rest of the run.
                Some(_) => out.push(' '),
                None => out.extend(run.iter()),
            }
        } else {
            out.push(chars[i]);
            i += 1;
        }
    }
    out
}

/// v4 `oneLine`: flatten to one line, JS-trim, cap at 1,200 UTF-16 units + `…`.
pub fn one_line(text: &str) -> String {
    let collapsed = collapse_newline_runs(text);
    let flat = js_trim(&collapsed);
    if utf16_len(flat) > MEMBER_CONTENT_CHAR_CAP {
        format!("{}…", utf16_truncate(flat, MEMBER_CONTENT_CHAR_CAP))
    } else {
        flat.to_string()
    }
}

/// `x?.trim()` truthiness — a non-empty JS-trimmed string.
fn has_text(v: &Option<String>) -> Option<&str> {
    v.as_deref().map(js_trim).filter(|t| !t.is_empty())
}

/// Build the user message for one cluster. v4 exports it for tests and the
/// dry-run report.
pub fn build_consolidation_user_message(input: &ConsolidationCallInput) -> String {
    let subject_line = match input.bucket {
        ConsolidationBucketKind::SelfBucket => {
            format!("SUBJECT: {} (the holder)", input.holder_name)
        }
        ConsolidationBucketKind::Other => format!("SUBJECT: {}", input.subject_name),
        ConsolidationBucketKind::None => "SUBJECT: (none — general notes)".to_string(),
    };
    let mut lines: Vec<String> = vec![
        "CONTEXT".to_string(),
        format!("HOLDER: {}", input.holder_name),
        subject_line,
        voice_line(input),
        if input.cluster_kind == "episodic" {
            "CLUSTER: EPISODE — dated notes of something that happened within one day.".to_string()
        } else {
            "CLUSTER: standing facts, preferences, and threads.".to_string()
        },
        String::new(),
    ];
    if let Some(canon) = has_text(&input.canon_block) {
        lines.push(canon.to_string());
        lines.push(String::new());
    }
    if let (Some(_), Some(existing)) = (
        has_text(&input.existing_digest),
        input.existing_digest.as_deref(),
    ) {
        lines.push("EXISTING DIGEST".to_string());
        lines.push(one_line(existing));
        lines.push(String::new());
    }
    lines.push("NOTES (oldest first)".to_string());
    for m in &input.members {
        let when = match m.when.as_deref() {
            Some(w) if !w.is_empty() => utf16_truncate(w, 10),
            _ => "undated".to_string(),
        };
        lines.push(format!(
            "{} | {} | {} | {} | {}",
            m.handle,
            when,
            crate::jsnum::to_fixed(m.importance, 2),
            crate::db::js_number_to_json(m.reinforcement_count),
            one_line(&m.content)
        ));
    }
    lines.join("\n")
}

/// Parse a raw answer into a validation verdict. Never fails: a malformed
/// answer is a *finished* call with a bad result (skip the cluster), not a
/// provider failure that should walk the fallback chain.
pub fn parse_consolidation_response(
    content: &str,
    input_handles: &[String],
) -> ConsolidationValidation {
    match parse_llm_json_object(content) {
        Ok(raw) => validate_consolidation_output(&raw, input_handles),
        Err(err) => ConsolidationValidation::Invalid {
            reason: format!(
                "unparseable JSON: {}",
                parse_llm_json_object_failure_message(content, &err)
            ),
        },
    }
}

/// Run the consolidation call for one cluster. The task result is successful
/// whenever the provider answered; the result's verdict says whether the answer
/// passed validation.
#[allow(clippy::too_many_arguments)]
pub async fn consolidate_memory_cluster<C: CompletionProvider>(
    executor: &CheapLlmTaskExecutor,
    completion: &C,
    input: &ConsolidationCallInput,
    selection: &CheapLlmSelection,
    character_id: &str,
    uncensored_fallback: Option<&UncensoredFallbackOptions<'_>>,
) -> CheapLlmTaskResult<ConsolidationValidation> {
    let handles: Vec<String> = input.members.iter().map(|m| m.handle.clone()).collect();
    let messages = vec![
        CompletionMessage::system(CONSOLIDATION_SYSTEM_PROMPT),
        CompletionMessage::user(build_consolidation_user_message(input)),
    ];
    // v4's ROOT logger (no module child).
    tracing::debug!(
        characterId = %character_id,
        bucket = %input.bucket.as_str(),
        clusterKind = %input.cluster_kind,
        members = handles.len(),
        hasExistingDigest = input.existing_digest.as_deref().is_some_and(|s| !s.is_empty()),
        provider = %selection.provider,
        model = %selection.model_name,
        "[Consolidation] Sending cluster to model"
    );
    executor
        .execute(
            completion,
            selection,
            messages,
            |content: &str| parse_consolidation_response(content, &handles),
            uncensored_fallback,
            Some(CONSOLIDATION_MAX_TOKENS),
            Some(character_id),
            Some(CONSOLIDATION_TASK_TYPE),
            CheapLlmTaskOptions::default(),
        )
        .await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn collapse_keeps_runs_without_newlines() {
        assert_eq!(one_line("a  b\n  c\t\n d"), "a  b c d");
        assert_eq!(one_line("  \n x \n "), "x");
    }

    #[test]
    fn unlisted_folds_into_standalone() {
        let handles: Vec<String> = ["m1", "m2", "m3"].iter().map(|s| s.to_string()).collect();
        let raw = json!({"digests": [{"content": " c ", "summary": "s", "importance": 0.5, "memberIds": ["m1"]}], "keepStandalone": ["m2"]});
        let ConsolidationValidation::Ok(v) = validate_consolidation_output(&raw, &handles) else {
            panic!("ok expected");
        };
        assert_eq!(v.digests[0].content, "c");
        assert_eq!(v.keep_standalone, vec!["m2", "m3"]);
        assert_eq!(v.unlisted, vec!["m3"]);
    }
}
