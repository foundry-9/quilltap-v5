//! Port of v4's `lib/chat/context/announcement-attribution.ts` — speaker
//! attribution for ad-hoc announcements (the Insert Announcement composer).
//!
//! `customAnnouncer` names who is speaking — an off-scene workspace character, or
//! a free-text display name — but it is a *rendering* field: the Salon paints that
//! name and avatar on the bubble, and nothing carried it into the model's context.
//! So an announcement posted as a named character arrived at every character as an
//! anonymous block of prose, and the model had to guess who was talking. It
//! guesses badly, and confidently: a whispered announcement written in one
//! character's voice was read as a different character entirely, and the mistake
//! then became part of the scene.
//!
//! Every other authored line in a context carries a speaker — participant messages
//! are tagged `[Name]` by [`crate::message_attribution::attribute_messages_for_character`],
//! and Staff name themselves in their own prose ("Prospero opens his ledger…").
//! The ad-hoc announcement was the sole anonymous line, and it is the one place
//! the operator explicitly *chose* a speaker. The Courier transport already
//! resolved the name for its own transcript
//! ([`crate::services::courier_transport`]), so the same message was attributed on
//! one path and anonymous on the other — the model's reading of a scene changed
//! with the transport carrying it.
//!
//! The tag uses the same `[Name] ` form the multi-character path already emits, so
//! it reads as one convention rather than a second dialect.
//!
//! **Pure** — no repository access, so the name map is the caller's problem
//! (`services::message_context` builds it). Pinned by
//! `announcement_attribution_equivalence` (tier 1, exact).

use std::collections::HashMap;

use serde_json::Value;

use crate::jsstr::js_trim;
use crate::staff_display_names::staff_display_name;

/// The `customAnnouncer` column's shape, structural so callers stay decoupled
/// (v4 `CustomAnnouncer`). `kind` is carried as the raw string: v4 types it
/// `'character' | 'custom'` but tests only `=== 'character'` at runtime, so any
/// other stored value takes the display-name branch.
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct CustomAnnouncer {
    pub kind: String,
    pub character_id: Option<String>,
    pub display_name: Option<String>,
}

impl CustomAnnouncer {
    /// Read the stored `customAnnouncer` JSON. A non-object (absent, null, or a
    /// scalar) is `None` — v4's optional chaining reaches the same place.
    pub fn from_value(v: Option<&Value>) -> Option<CustomAnnouncer> {
        let o = v?.as_object()?;
        Some(CustomAnnouncer {
            kind: o
                .get("kind")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string(),
            character_id: o
                .get("characterId")
                .and_then(Value::as_str)
                .map(str::to_string),
            display_name: o
                .get("displayName")
                .and_then(Value::as_str)
                .map(str::to_string),
        })
    }
}

/// v4's structural `AnnouncerAttributable` — the fields the pass reads, plus the
/// `{...m, content}` / `{...m, opaqueContent}` spreads it uses to rewrite them.
pub trait AnnouncerAttributable: Clone {
    fn content(&self) -> Option<&str>;
    /// Returned by value so a JSON-shaped carrier can read the column on demand
    /// (the announcer bag is three short strings and only announcement rows carry
    /// one, so the copy is not worth a lifetime).
    fn custom_announcer(&self) -> Option<CustomAnnouncer>;
    /// `typeof m.opaqueContent === 'string'` — `Some` only when the column is a
    /// string (an absent or null `opaqueContent` is `None`, matching v4's guard).
    fn opaque_content(&self) -> Option<&str>;
    /// The `systemSender` column (the Staff signer) — `None` when absent/null/"".
    fn system_sender(&self) -> Option<&str>;
    /// The `systemKind` column; the staff fallback fires only for `"announcement"`.
    fn system_kind(&self) -> Option<&str>;
    /// v4 `{ ...m, content }` — every other field survives untouched.
    fn with_content(&self, content: String) -> Self;
    /// v4 `next.opaqueContent = …` — rewrites only `opaqueContent`. Separate from
    /// [`Self::with_content`] because the pass writes both, independently.
    fn with_opaque_content(&self, opaque_content: String) -> Self;
}

/// Resolve the display name for an announcer, or `None` when it can't be named
/// (v4 `resolveAnnouncerName`).
///
/// A `character` announcer whose id resolves to nothing — deleted since the
/// announcement was posted — returns `None` rather than a placeholder: a wrong or
/// invented name is worse than no name, because the model treats a name as fact.
///
/// When no `customAnnouncer` is present the announcement was signed as Staff (the
/// Insert Announcement dialog's `staff` mode writes a `systemSender` and no
/// `customAnnouncer`). Those still need a speaker: an operator-authored line signed
/// as the Host is not prose the Host wrote, so it carries no self-naming, and
/// without a fallback it reaches the model as an anonymous `user` turn. Fall back
/// to the `systemSender`, resolved through the single staff-name table.
pub fn resolve_announcer_name(
    announcer: Option<CustomAnnouncer>,
    character_names_by_id: &HashMap<String, String>,
    system_sender: Option<&str>,
) -> Option<String> {
    if let Some(announcer) = announcer {
        if announcer.kind == "character" {
            // `if (!announcer.characterId) return null` — the empty string is falsy.
            let id = announcer
                .character_id
                .as_deref()
                .filter(|s| !s.is_empty())?;
            return character_names_by_id
                .get(id)
                .map(|n| js_trim(n).to_string())
                .filter(|n| !n.is_empty());
        }

        return announcer
            .display_name
            .as_deref()
            .map(|n| js_trim(n).to_string())
            .filter(|n| !n.is_empty());
    }

    // v4 `if (systemSender) return staffDisplayName(systemSender).trim() || null`.
    // `if (systemSender)` is JS-truthy, so "" is excluded but a whitespace-only
    // sender is NOT — it reaches `staffDisplayName` (returns the raw tag) and the
    // `.trim() || null` then nulls it out.
    let sender = system_sender.filter(|s| !s.is_empty())?;
    let name = js_trim(&staff_display_name(Some(sender))).to_string();
    (!name.is_empty()).then_some(name)
}

/// Character ids an announcement references, for a single up-front name lookup
/// (v4 `collectAnnouncerCharacterIds`). First-appearance order, deduped.
pub fn collect_announcer_character_ids<T: AnnouncerAttributable>(messages: &[T]) -> Vec<String> {
    let mut seen = std::collections::HashSet::new();
    let mut ids = Vec::new();
    for m in messages {
        if let Some(a) = m.custom_announcer() {
            if a.kind == "character" {
                if let Some(id) = a.character_id.as_deref().filter(|s| !s.is_empty()) {
                    if seen.insert(id.to_string()) {
                        ids.push(id.to_string());
                    }
                }
            }
        }
    }
    ids
}

/// Prefix each ad-hoc announcement's body with its speaker (v4
/// `attributeAdhocAnnouncements`).
///
/// A `customAnnouncer` (character/custom mode) names the speaker directly. A
/// `staff`-mode announcement carries a `systemSender` instead — but only ad-hoc
/// announcements (`systemKind === 'announcement'`) take that fallback: ordinary
/// Staff whispers (image notices, tool bubbles, memory recalls) also carry a
/// `systemSender` and name themselves in their own prose, so prefixing them here
/// would double-tag every one. A message with neither field, and an announcer that
/// can't be named, both pass through unchanged.
///
/// The prefix lands on `opaqueContent` too when present: an opaque-anywhere chat
/// swaps that persona-free body into the LLM context in place of `content`
/// ([`crate::services::message_context::normalize_whisper_roles`]), so tagging only
/// `content` would leave the model's copy anonymous in exactly that mode.
pub fn attribute_adhoc_announcements<T: AnnouncerAttributable>(
    messages: &[T],
    character_names_by_id: &HashMap<String, String>,
) -> Vec<T> {
    messages
        .iter()
        .map(|m| {
            // v4 `const systemSender = m.systemKind === 'announcement' ? m.systemSender : undefined`.
            let system_sender = if m.system_kind() == Some("announcement") {
                m.system_sender()
            } else {
                None
            };
            let Some(name) =
                resolve_announcer_name(m.custom_announcer(), character_names_by_id, system_sender)
            else {
                return m.clone();
            };
            let tag = format!("[{name}]");
            // Idempotent: re-running (a retry, a regenerate) must not stack tags.
            let prefix = |text: &str| {
                if text.starts_with(&tag) {
                    text.to_string()
                } else {
                    format!("{tag} {text}")
                }
            };
            // v4 `{ ...m, content: prefix(m.content ?? '') }`.
            let mut next = m.with_content(prefix(m.content().unwrap_or("")));
            // v4 `if (typeof m.opaqueContent === 'string') next.opaqueContent = prefix(m.opaqueContent)`.
            if let Some(opaque) = m.opaque_content() {
                next = next.with_opaque_content(prefix(opaque));
            }
            next
        })
        .collect()
}

/// v4's interface is structural, so a raw message object satisfies it directly.
/// This impl is what the differential drives (and what any JSON-shaped caller
/// gets for free); the production caller uses
/// [`crate::services::message_context::WhisperMessage`].
impl AnnouncerAttributable for Value {
    fn content(&self) -> Option<&str> {
        self.get("content").and_then(Value::as_str)
    }
    fn custom_announcer(&self) -> Option<CustomAnnouncer> {
        CustomAnnouncer::from_value(self.get("customAnnouncer"))
    }
    fn opaque_content(&self) -> Option<&str> {
        // `typeof === 'string'`: a null/absent opaqueContent is not a string.
        self.get("opaqueContent").and_then(Value::as_str)
    }
    fn system_sender(&self) -> Option<&str> {
        self.get("systemSender").and_then(Value::as_str)
    }
    fn system_kind(&self) -> Option<&str> {
        self.get("systemKind").and_then(Value::as_str)
    }
    fn with_content(&self, content: String) -> Value {
        let mut v = self.clone();
        if let Some(o) = v.as_object_mut() {
            o.insert("content".to_string(), Value::String(content));
        }
        v
    }
    fn with_opaque_content(&self, opaque_content: String) -> Value {
        let mut v = self.clone();
        if let Some(o) = v.as_object_mut() {
            o.insert("opaqueContent".to_string(), Value::String(opaque_content));
        }
        v
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn names() -> HashMap<String, String> {
        HashMap::from([("ariel".to_string(), "Ariel".to_string())])
    }

    #[test]
    fn unresolvable_character_is_never_invented() {
        let gone = CustomAnnouncer {
            kind: "character".into(),
            character_id: Some("gone".into()),
            display_name: None,
        };
        assert_eq!(resolve_announcer_name(Some(gone), &names(), None), None);
        assert_eq!(resolve_announcer_name(None, &names(), None), None);
    }

    #[test]
    fn staff_sender_is_the_fallback_speaker() {
        // No announcer, a Staff signer → the staff display name (bug 28).
        assert_eq!(
            resolve_announcer_name(None, &names(), Some("host")),
            Some("The Host".to_string())
        );
        // A whitespace-only sender is JS-truthy but trims empty → None.
        assert_eq!(resolve_announcer_name(None, &names(), Some("   ")), None);
        // An announcer, when present, wins over the sender.
        let ariel = CustomAnnouncer {
            kind: "character".into(),
            character_id: Some("ariel".into()),
            display_name: None,
        };
        assert_eq!(
            resolve_announcer_name(Some(ariel), &names(), Some("host")),
            Some("Ariel".to_string())
        );
    }
}
