//! Port of v4's `lib/llm/message-formatter.ts` — provider-aware multi-character
//! message formatting plus the finalizer's anti-hijack cleanups.
//!
//! The centrepiece is the anti-hijack trio the finalizer runs on a model's
//! response: `strip_character_name_prefix` (drop the model's own leading
//! `[Name]`/`Name:` echo), `truncate_at_foreign_speaker` (cut where the model
//! begins writing ANOTHER participant's turn), and `normalize_content_block_format`
//! (unwrap Anthropic-style content-block arrays). These guard against a model
//! speaking as someone else — subtle string/regex logic reproduced byte-for-byte.
//!
//! JS-regex fidelity notes (the trap):
//!   * v4's `RegExp` literals here use NO `u` flag except the one general
//!     name-prefix fallback (`/…/u`). A non-`u` regex matches on UTF-16 code
//!     units; a `/u` regex matches on code points. We reproduce the character
//!     classes with the `regex` crate (Unicode on by default) and pin exact
//!     anchoring / case-insensitivity per pattern.
//!   * `.replace(pattern, '')` with a non-global `RegExp` replaces only the
//!     FIRST match; `.replace(/\s+$/, '')` strips one trailing-whitespace run.
//!     JS `\s` is the full Unicode whitespace set (`is_js_ws` in `jsstr`).
//!   * `String.prototype.slice(0, n)` and `.length` are UTF-16 based
//!     (`jsstr::utf16_truncate` / `utf16_len`).

use crate::jsstr;
use regex::Regex;
use std::sync::LazyLock;

/// Provider capabilities for the message `name` field.
///
/// Mirrors v4's `ProviderNameSupport` / the legacy `LegacyProviderNameSupport`.
/// In v4 this comes from a plugin's `messageFormat` (the provider-registry seam)
/// falling back to the hardcoded `LEGACY_PROVIDER_NAME_SUPPORT` table; no bundled
/// plugin registers a `messageFormat`, so the fallback table is authoritative.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProviderNameSupport {
    /// Whether the provider supports a `name` field on messages.
    pub supports_name_field: bool,
    /// Which roles support the `name` field.
    pub supported_roles: Vec<MessageRole>,
    /// Maximum length for the `name` field (if limited).
    pub max_name_length: Option<usize>,
}

/// The two attributable message roles (`system` never gets name attribution).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MessageRole {
    User,
    Assistant,
}

/// Legacy provider name-support table (`LEGACY_PROVIDER_NAME_SUPPORT` in
/// `lib/llm/fallback-data.ts`). Keyed by the UPPERCASED provider string.
///
/// Faithful quirk: the OpenAI-compatible entry is keyed `"OPENAI-COMPATIBLE"`
/// (a hyphen), while `get_provider_name_support` looks up by `to_uppercase()`.
/// So a provider `"openai_compatible"` → `"OPENAI_COMPATIBLE"` MISSES this row
/// (falls through to no name support), whereas `"openai-compatible"` matches.
fn legacy_provider_name_support(upper: &str) -> Option<ProviderNameSupport> {
    match upper {
        "OPENAI" => Some(ProviderNameSupport {
            supports_name_field: true,
            supported_roles: vec![MessageRole::User, MessageRole::Assistant],
            max_name_length: Some(64),
        }),
        "ANTHROPIC" => Some(ProviderNameSupport {
            supports_name_field: false,
            supported_roles: vec![],
            max_name_length: None,
        }),
        "GOOGLE" => Some(ProviderNameSupport {
            supports_name_field: false,
            supported_roles: vec![],
            max_name_length: None,
        }),
        "OPENROUTER" => Some(ProviderNameSupport {
            supports_name_field: false,
            supported_roles: vec![],
            max_name_length: None,
        }),
        "GROK" => Some(ProviderNameSupport {
            supports_name_field: true,
            supported_roles: vec![MessageRole::User, MessageRole::Assistant],
            max_name_length: Some(64),
        }),
        "OLLAMA" => Some(ProviderNameSupport {
            supports_name_field: false,
            supported_roles: vec![],
            max_name_length: None,
        }),
        "OPENAI-COMPATIBLE" => Some(ProviderNameSupport {
            supports_name_field: true,
            supported_roles: vec![MessageRole::User, MessageRole::Assistant],
            max_name_length: Some(64),
        }),
        _ => None,
    }
}

/// Get name-field support info for a provider (`getProviderNameSupport`).
///
/// v4 first consults the plugin registry (`getMessageFormat`) and returns it iff
/// it advertises support; no bundled plugin does, so the registry always yields
/// the empty `{ supportsNameField: false, supportedRoles: [] }` and we fall
/// straight through to the legacy table. Unknown providers → no support.
pub fn get_provider_name_support(provider: &str) -> ProviderNameSupport {
    let normalized = provider.to_uppercase();
    legacy_provider_name_support(&normalized).unwrap_or(ProviderNameSupport {
        supports_name_field: false,
        supported_roles: vec![],
        max_name_length: None,
    })
}

/// Whether a provider supports the `name` field for a given role
/// (`supportsNameField`).
pub fn supports_name_field(provider: &str, role: MessageRole) -> bool {
    let support = get_provider_name_support(provider);
    support.supports_name_field && support.supported_roles.contains(&role)
}

/// Format a participant name for the OpenAI-style `name` field or a content
/// prefix (`formatParticipantName`). Sanitizes to `[A-Za-z0-9_-]`, spaces →
/// underscores, truncates to `max_length` UTF-16 units, falls back to
/// `"Unknown"` when nothing survives.
pub fn format_participant_name(name: &str, max_length: usize) -> String {
    // 1. trim (JS String.prototype.trim — Unicode whitespace)
    let trimmed = jsstr::js_trim(name);
    // 2. replace(/\s+/g, '_') — runs of JS whitespace → single underscore
    let mut spaced = String::new();
    let mut in_ws = false;
    for ch in trimmed.chars() {
        if jsstr::is_js_ws(ch) {
            if !in_ws {
                spaced.push('_');
                in_ws = true;
            }
        } else {
            spaced.push(ch);
            in_ws = false;
        }
    }
    // 3. replace(/[^a-zA-Z0-9_-]/g, '') — strip everything else
    let filtered: String = spaced
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || *c == '_' || *c == '-')
        .collect();
    // 4. slice(0, maxLength) — UTF-16 truncation
    let sanitized = jsstr::utf16_truncate(&filtered, max_length);
    // 5. `sanitized || 'Unknown'`
    if sanitized.is_empty() {
        "Unknown".to_string()
    } else {
        sanitized
    }
}

/// Format a display name for a content prefix (`formatDisplayName`): trimmed, or
/// `"Unknown"` when empty.
pub fn format_display_name(name: &str) -> String {
    let trimmed = jsstr::js_trim(name);
    if trimmed.is_empty() {
        "Unknown".to_string()
    } else {
        trimmed.to_string()
    }
}

/// A message with optional name/participant info for multi-character context
/// (`MultiCharacterMessage`).
#[derive(Clone, Debug)]
pub struct MultiCharacterMessage {
    pub role: WireRole,
    pub content: String,
    pub id: Option<String>,
    pub name: Option<String>,
    pub participant_id: Option<String>,
    pub thought_signature: Option<String>,
}

/// The three input roles (`system` | `user` | `assistant`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WireRole {
    System,
    User,
    Assistant,
}

/// A provider-formatted message (the output shape of `formatMessagesForProvider`).
///
/// `thought_signature` is always present in the JS object (set to `undefined`
/// when absent, which `JSON.stringify` drops); modelled here as an `Option`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FormattedMessage {
    pub role: WireRole,
    pub content: String,
    pub name: Option<String>,
    pub thought_signature: Option<String>,
}

/// Build the `^\[<escaped-display-name>\]\s*` (case-insensitive) existing-prefix
/// probe used by both the assistant and user branches of
/// `formatMessagesForProvider`. `\s*` is JS's Unicode whitespace star.
fn build_existing_prefix_pattern(display_name: &str) -> Regex {
    let escaped = escape_regexp(display_name);
    // (?i) case-insensitive, (?s not needed) — `[…]` literal, `\s*` whitespace.
    Regex::new(&format!(r"(?i)^\[{escaped}\]\s*")).expect("valid prefix regex")
}

/// Prefix `content` with `[displayName] ` unless it already opens with that
/// bracketed name (the shared body of the assistant/user prefix logic).
fn apply_existing_prefix(display_name: &str, content: &str) -> String {
    let pat = build_existing_prefix_pattern(display_name);
    if pat.is_match(content) {
        content.to_string()
    } else {
        format!("[{display_name}] {content}")
    }
}

/// Format messages for a provider, applying the `name` field or a content prefix
/// as needed (`formatMessagesForProvider`).
///
/// `responding_character_name` is accepted to mirror v4's signature; v4 does not
/// actually consult it in the body (attribution is per-message via `name`).
pub fn format_messages_for_provider(
    messages: &[MultiCharacterMessage],
    provider: &str,
    _responding_character_name: &str,
) -> Vec<FormattedMessage> {
    let name_support = get_provider_name_support(provider);

    messages
        .iter()
        .map(|msg| {
            // System messages don't get name attribution.
            if msg.role == WireRole::System {
                return FormattedMessage {
                    role: msg.role,
                    content: msg.content.clone(),
                    name: None,
                    thought_signature: msg.thought_signature.clone(),
                };
            }

            // No name to attribute — return as is.
            let name = match &msg.name {
                Some(n) if !n.is_empty() => n,
                // In JS `!msg.name` is true for `undefined`, `null`, and `""`.
                _ => {
                    return FormattedMessage {
                        role: msg.role,
                        content: msg.content.clone(),
                        name: None,
                        thought_signature: msg.thought_signature.clone(),
                    };
                }
            };

            let role_for_provider = if msg.role == WireRole::Assistant {
                MessageRole::Assistant
            } else {
                MessageRole::User
            };
            let supports_native_name = name_support.supports_name_field
                && name_support.supported_roles.contains(&role_for_provider);

            let max_name_length = name_support.max_name_length.unwrap_or(64);

            if role_for_provider == MessageRole::Assistant {
                // Assistant turns belong to the responding character themselves —
                // prefer the native name field, only fall back to prefixing.
                if supports_native_name {
                    return FormattedMessage {
                        role: WireRole::Assistant,
                        content: msg.content.clone(),
                        name: Some(format_participant_name(name, max_name_length)),
                        thought_signature: msg.thought_signature.clone(),
                    };
                }
                let display_name = format_display_name(name);
                let prefixed = apply_existing_prefix(&display_name, &msg.content);
                return FormattedMessage {
                    role: WireRole::Assistant,
                    content: prefixed,
                    name: None,
                    thought_signature: msg.thought_signature.clone(),
                };
            }

            // User-role messages mix the actual user, other characters'
            // downgraded assistant turns, and system narration. The OpenAI-style
            // `name` field is a weak cross-speaker signal, so always inline the
            // `[Name]` prefix; if the provider also supports the name field, send
            // both.
            let display_name = format_display_name(name);
            let prefixed = apply_existing_prefix(&display_name, &msg.content);
            FormattedMessage {
                role: WireRole::User,
                content: prefixed,
                name: if supports_native_name {
                    Some(format_participant_name(name, max_name_length))
                } else {
                    None
                },
                thought_signature: msg.thought_signature.clone(),
            }
        })
        .collect()
}

/// One "other participant" for the multi-character context section.
#[derive(Clone, Debug)]
pub struct OtherParticipant {
    pub name: String,
    pub aliases: Option<Vec<String>>,
    pub pronouns: Option<ParticipantPronouns>,
    pub description: Option<String>,
    /// The `type: 'CHARACTER'` discriminator (only value v4 ever passes).
    pub status: Option<String>,
}

/// Subject/object/possessive pronoun triple for the context section.
#[derive(Clone, Debug)]
pub struct ParticipantPronouns {
    pub subject: String,
    pub object: String,
    pub possessive: String,
}

/// Build the multi-character context section for the system prompt
/// (`buildMultiCharacterContextSection`). Empty roster → empty string.
pub fn build_multi_character_context_section(
    other_participants: &[OtherParticipant],
    responding_character_name: &str,
) -> String {
    if other_participants.is_empty() {
        return String::new();
    }

    let mut lines: Vec<String> = vec![
        String::new(),
        "## Other Participants in This Conversation".to_string(),
        String::new(),
    ];

    // Status guide — always present so the LLM understands the participation model.
    lines.push("**Participant Status Guide:**".to_string());
    lines.push("- **active**: Present and participating normally in the conversation".to_string());
    lines.push(
        "- **silent**: Present but observing silently — may think and act physically, but does not speak aloud"
            .to_string(),
    );
    lines.push("- **absent**: Away from the scene — cannot perceive what is happening".to_string());
    lines.push(String::new());

    for participant in other_participants {
        // type is always 'CHARACTER'; label the user when the name includes "User".
        let type_label = if participant.name.contains("User") {
            "(the user)"
        } else {
            ""
        };
        let alias_note = match &participant.aliases {
            Some(a) if !a.is_empty() => format!(" (also known as: {})", a.join(", ")),
            _ => String::new(),
        };
        let pronoun_note = match &participant.pronouns {
            Some(p) => format!(" (pronouns: {}/{}/{})", p.subject, p.object, p.possessive),
            None => String::new(),
        };
        let status = participant.status.as_deref().filter(|s| !s.is_empty());
        let status_note = format!(" [{}]", status.unwrap_or("active"));
        let description = match &participant.description {
            Some(d) if !d.is_empty() => format!(" - {d}"),
            _ => String::new(),
        };
        lines.push(format!(
            "- **{}**{alias_note}{pronoun_note}{status_note} {type_label}{description}",
            participant.name
        ));
    }

    lines.push(String::new());
    lines.push(format!(
        "You are {responding_character_name}. Stay in character when responding to the other participants. \
Messages from other characters and the user will be marked with their names. \
Your responses will be attributed to you ({responding_character_name})."
    ));

    lines.join("\n")
}

/// Normalize LLM response content that may be wrapped in content-block array
/// format (`normalizeContentBlockFormat`).
///
/// Some providers return content as Anthropic's content-block array, either as a
/// Python repr `[{'type': 'text', 'text': "…"}]` or as JSON
/// `[{"type": "text", "text": "…"}]`. Extract the inner text if detected.
pub fn normalize_content_block_format(content: &str) -> String {
    if content.is_empty() {
        return content.to_string();
    }

    let trimmed = jsstr::js_trim(content);

    // Quick checks to avoid expensive regex on normal content.
    if !trimmed.starts_with('[') {
        return content.to_string();
    }
    if !trimmed.contains("'type'") && !trimmed.contains("\"type\"") {
        return content.to_string();
    }

    // Python-style single quotes: `[{'type': 'text', 'text': "…"}]`. The inner
    // text value uses double quotes; `[\s\S]*` is greedy over any char.
    if let Some(caps) = PYTHON_CONTENT_BLOCK.captures(trimmed) {
        return caps.get(1).map(|m| m.as_str()).unwrap_or("").to_string();
    }

    // Try parsing as JSON (double-quoted format):
    // `[{"type": "text", "text": "…"}]`.
    if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(trimmed) {
        if let Some(arr) = parsed.as_array() {
            if !arr.is_empty() {
                let first = &arr[0];
                if first.get("type").and_then(|t| t.as_str()) == Some("text") {
                    if let Some(text) = first.get("text").and_then(|t| t.as_str()) {
                        return text.to_string();
                    }
                }
            }
        }
    }

    content.to_string()
}

/// Strip character name prefixes from the START of a response
/// (`stripCharacterNamePrefix`).
///
/// LLMs sometimes mimic the `[Name]` prefix format from the input. This removes
/// any such leading prefixes — but ONLY those carrying the actual character name
/// or an alias (as `[Name]` or `Name:`), never arbitrary bracketed content
/// (roleplay action tags `[*sighs*]`, status lines `[Whisper sent.]`, …). With
/// no name at all it falls back to a conservative name-like bracket pattern.
/// Iterates so `"[Name]\n[Name]\n*content*"` fully collapses (≤10 passes).
pub fn strip_character_name_prefix(
    content: &str,
    character_name: Option<&str>,
    aliases: Option<&[String]>,
) -> String {
    if content.is_empty() {
        return content.to_string();
    }

    let mut result = content.to_string();

    // Build targeted patterns from the known name and aliases. We ONLY strip
    // brackets containing the actual name/alias — anything else would eat
    // roleplay action tags / status messages.
    let mut name_patterns: Vec<Regex> = Vec::new();

    // v4 guards with `if (characterName)` — an empty string is falsy, so no
    // pattern is built for `""` (only a non-empty name gets targeted patterns).
    if let Some(name) = character_name.filter(|n| !n.is_empty()) {
        let escaped = escape_regexp(name);
        name_patterns.push(Regex::new(&format!(r"(?i)^\s*\[{escaped}\]\s*")).unwrap());
        // Also match "Name:" without brackets.
        name_patterns.push(Regex::new(&format!(r"(?i)^\s*{escaped}\s*:\s*")).unwrap());
    }

    if let Some(aliases) = aliases {
        for alias in aliases {
            let escaped = escape_regexp(alias);
            name_patterns.push(Regex::new(&format!(r"(?i)^\s*\[{escaped}\]\s*")).unwrap());
            name_patterns.push(Regex::new(&format!(r"(?i)^\s*{escaped}\s*:\s*")).unwrap());
        }
    }

    // No character name at all → a conservative general pattern that only
    // matches short name-like content in brackets (1–3 words worth of
    // letters/marks/spaces/hyphens/apostrophes, 1–40 code points — the `/u`
    // flag makes this count code points, `\p{L}\p{M}` Unicode categories).
    if name_patterns.is_empty() {
        name_patterns.push(GENERAL_NAME_PREFIX.clone());
    }

    // Keep stripping until nothing more matches (or the safety cap).
    let mut previous_len: i64 = -1;
    let mut iterations = 0;
    const MAX_ITERATIONS: i32 = 10;

    // JS compares `result.length` (UTF-16 units); reproduce with utf16_len so a
    // strip that removes an astral char still registers as a length change.
    while jsstr::utf16_len(&result) as i64 != previous_len && iterations < MAX_ITERATIONS {
        previous_len = jsstr::utf16_len(&result) as i64;
        iterations += 1;

        let mut matched = false;
        for pattern in &name_patterns {
            if pattern.is_match(&result) {
                // `.replace(pattern, '')` with a non-global RegExp replaces the
                // FIRST match only — an `^`-anchored pattern has at most one.
                result = pattern.replace(&result, "").into_owned();
                matched = true;
                break;
            }
        }

        if !matched {
            break;
        }
    }

    result
}

/// Result of [`truncate_at_foreign_speaker`] (`ForeignSpeakerTruncation`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ForeignSpeakerTruncation {
    /// The response up to (not including) the first foreign speaker tag,
    /// right-trimmed.
    pub text: String,
    /// UTF-16 offset into the INPUT where truncation occurred (start of the
    /// foreign tag's line), or `None` if none found. `Some(0)` = the input began
    /// with a foreign tag.
    pub truncated_at: Option<usize>,
}

/// Multi-character anti-hijack safeguard (`truncateAtForeignSpeaker`): detect
/// where a model has begun writing ANOTHER participant's turn — a line opening
/// with a known other-participant name as a speaker tag, `[Name]` or `Name:` —
/// and truncate there. Returns the text before that tag (right-trimmed) plus the
/// cut offset.
///
/// Model-agnostic structural backstop to the system-prompt anti-impersonation
/// guidance: even if an LLM ignores "only write your own turn", its output can
/// never carry another character's lines into the transcript. Matches ONLY the
/// supplied `foreign_names` (trimmed, non-empty) as line-anchored speaker tags —
/// never arbitrary bracketed content, so action tags (`[*sighs*]`), status lines
/// (`[Whisper sent.]`), prose mentions ("I told Charlie:"), and the speaker's
/// OWN name are left intact. Strip the speaker's own prefix with
/// [`strip_character_name_prefix`] first; pass only the OTHER participants here.
pub fn truncate_at_foreign_speaker(
    content: &str,
    foreign_names: &[String],
) -> ForeignSpeakerTruncation {
    if content.is_empty() || foreign_names.is_empty() {
        return ForeignSpeakerTruncation {
            text: content.to_string(),
            truncated_at: None,
        };
    }

    let escaped: Vec<String> = foreign_names
        .iter()
        .map(|n| jsstr::js_trim(n))
        .filter(|n| !n.is_empty())
        .map(escape_regexp)
        .collect();
    if escaped.is_empty() {
        return ForeignSpeakerTruncation {
            text: content.to_string(),
            truncated_at: None,
        };
    }

    let names = escaped.join("|");
    // Line-anchored (start of string or after a newline), optional leading
    // space/tab, then "[Name]" or "Name:" — the two screenplay speaker formats.
    // Case-insensitive; NOT multiline (`^` anchors the whole string), matching
    // v4's `(?:^|\n)` explicit newline alternation.
    let re = Regex::new(&format!(
        r"(?i)(?:^|\n)[ \t]*(?:\[(?:{names})\]|(?:{names})[ \t]*:)"
    ))
    .unwrap();

    let m = match re.find(content) {
        Some(m) => m,
        None => {
            return ForeignSpeakerTruncation {
                text: content.to_string(),
                truncated_at: None,
            };
        }
    };

    // m.start() is the byte position of the matched newline (or 0 at string
    // start). v4 reports `m.index` as a UTF-16 offset; convert. Slicing there
    // drops the newline and everything after it (the char-boundary byte slice is
    // exact because `[ \t\n[]` are all ASCII at the cut point).
    let cut_byte = m.start();
    let cut_utf16 = jsstr::utf16_len(&content[..cut_byte]);
    let sliced = &content[..cut_byte];
    let text = trim_trailing_js_ws(sliced);
    ForeignSpeakerTruncation {
        text,
        truncated_at: Some(cut_utf16),
    }
}

/// Right-trim JS `\s` whitespace (`.replace(/\s+$/, '')`): drop the trailing run
/// of Unicode-whitespace code points. Equivalent to `js_trim_end` (which trims
/// the same set), reused via `jsstr`.
fn trim_trailing_js_ws(s: &str) -> String {
    jsstr::js_trim_end(s).to_string()
}

/// Port of JS `String.prototype.replace(/[.*+?^${}()|[\]\\]/g, '\\$&')` —
/// escape every regex metacharacter in a literal so it matches literally. This
/// is the exact escape set v4 uses inline throughout this module.
fn escape_regexp(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for ch in s.chars() {
        if matches!(
            ch,
            '.' | '*' | '+' | '?' | '^' | '$' | '{' | '}' | '(' | ')' | '|' | '[' | ']' | '\\'
        ) {
            out.push('\\');
        }
        out.push(ch);
    }
    out
}

/// `^\[\s*\{\s*'type'\s*:\s*'text'\s*,\s*'text'\s*:\s*"([\s\S]*)"\s*\}\s*\]$`
/// (Python-repr content block). `[\s\S]` = "any char"; enable `s` so `.`-style
/// dot-all isn't needed and `$` anchors the absolute end. v4's `\s` here is JS
/// whitespace; the `regex` crate `\s` is Unicode whitespace — the same set for
/// this ASCII-punctuation-delimited pattern.
static PYTHON_CONTENT_BLOCK: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"(?s)^\[\s*\{\s*'type'\s*:\s*'text'\s*,\s*'text'\s*:\s*"([\s\S]*)"\s*\}\s*\]$"#)
        .expect("valid python content-block regex")
});

/// `^\s*\[[\p{L}\p{M}'\- ]{1,40}\]\s*` with the `/u` flag — the general
/// name-prefix fallback used when no character name is known. `{1,40}` counts
/// code points (the `/u` flag); the `regex` crate is Unicode by default.
static GENERAL_NAME_PREFIX: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^\s*\[[\p{L}\p{M}'\- ]{1,40}\]\s*").expect("valid general name-prefix regex")
});

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn escape_regexp_matches_js_set() {
        assert_eq!(escape_regexp("a.b*c"), r"a\.b\*c");
        assert_eq!(escape_regexp("[x]"), r"\[x\]");
        assert_eq!(escape_regexp("plain"), "plain");
    }

    #[test]
    fn strip_leading_bracket_name() {
        assert_eq!(
            strip_character_name_prefix("[Alice] hello", Some("Alice"), None),
            "hello"
        );
        // Repeated prefixes collapse.
        assert_eq!(
            strip_character_name_prefix("[Alice]\n[Alice]\n*waves*", Some("Alice"), None),
            "*waves*"
        );
        // Action tag left intact.
        assert_eq!(
            strip_character_name_prefix("[*sighs*] hi", Some("Alice"), None),
            "[*sighs*] hi"
        );
    }

    #[test]
    fn truncate_foreign_speaker_basic() {
        let r = truncate_at_foreign_speaker("I nod.\n[Bob] Hey there", &["Bob".to_string()]);
        assert_eq!(r.text, "I nod.");
        assert_eq!(r.truncated_at, Some(6));
    }

    #[test]
    fn normalize_python_content_block() {
        assert_eq!(
            normalize_content_block_format(r#"[{'type': 'text', 'text': "hi there"}]"#),
            "hi there"
        );
        assert_eq!(
            normalize_content_block_format(r#"[{"type": "text", "text": "json path"}]"#),
            "json path"
        );
        assert_eq!(normalize_content_block_format("plain text"), "plain text");
    }
}
