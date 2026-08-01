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

/// v4's structural `AnnouncerAttributable` — the two fields the pass reads, plus
/// the `{...m, content}` spread it uses to rewrite one of them.
pub trait AnnouncerAttributable: Clone {
    fn content(&self) -> Option<&str>;
    /// Returned by value so a JSON-shaped carrier can read the column on demand
    /// (the announcer bag is three short strings and only announcement rows carry
    /// one, so the copy is not worth a lifetime).
    fn custom_announcer(&self) -> Option<CustomAnnouncer>;
    /// v4 `{ ...m, content }` — every other field survives untouched.
    fn with_content(&self, content: String) -> Self;
}

/// Resolve the display name for an announcer, or `None` when it can't be named
/// (v4 `resolveAnnouncerName`).
///
/// A `character` announcer whose id resolves to nothing — deleted since the
/// announcement was posted — returns `None` rather than a placeholder: a wrong or
/// invented name is worse than no name, because the model treats a name as fact.
pub fn resolve_announcer_name(
    announcer: Option<CustomAnnouncer>,
    character_names_by_id: &HashMap<String, String>,
) -> Option<String> {
    let announcer = announcer?;

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

    announcer
        .display_name
        .as_deref()
        .map(|n| js_trim(n).to_string())
        .filter(|n| !n.is_empty())
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
/// Messages without a `customAnnouncer` pass through untouched — Staff
/// announcements carry their identity in their prose already, and participant
/// messages are attributed by the multi-character path. An announcer that can't be
/// named also passes through unchanged.
pub fn attribute_adhoc_announcements<T: AnnouncerAttributable>(
    messages: &[T],
    character_names_by_id: &HashMap<String, String>,
) -> Vec<T> {
    messages
        .iter()
        .map(|m| {
            let Some(name) = resolve_announcer_name(m.custom_announcer(), character_names_by_id)
            else {
                return m.clone();
            };
            // v4 `m.content ?? ''`.
            let body = m.content().unwrap_or("");
            // Idempotent: re-running (a retry, a regenerate) must not stack tags.
            if body.starts_with(&format!("[{name}]")) {
                return m.clone();
            }
            m.with_content(format!("[{name}] {body}"))
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
    fn with_content(&self, content: String) -> Value {
        let mut v = self.clone();
        if let Some(o) = v.as_object_mut() {
            o.insert("content".to_string(), Value::String(content));
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
        assert_eq!(resolve_announcer_name(Some(gone), &names()), None);
        assert_eq!(resolve_announcer_name(None, &names()), None);
    }
}
