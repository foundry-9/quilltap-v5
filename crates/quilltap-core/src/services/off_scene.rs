//! Off-scene character introduction SCAN + content builders (v4
//! `lib/services/host-notifications/writer.ts` + the scan block in
//! `lib/chat/context-manager.ts`).
//!
//! When characters or the user mention (by name) a workspace character not
//! present in the Salon, the Host introduces that off-scene character — for
//! reference only. `buildContext` scans this turn's corpus for newly-mentioned
//! off-scene characters and surfaces the announcement to THIS turn's LLM context.
//!
//! Closes the SCAN half of `BuildContextSeams::resolve_off_scene_announcement`
//! (W4.6a): everything up to and including the announcement CONTENT is ported;
//! the Host POST (`postHostOffSceneCharactersAnnouncement`'s `addMessage`) is
//! W4.6b — the caller invokes the recorded seam separately.

use serde_json::Value;
use std::collections::BTreeSet;

use crate::collation::locale_compare;
use crate::db::runtime::Db;
use crate::db::DbError;
use crate::mentioned_characters::{find_mentioned_character_ids, MentionCandidate};

/// v4 `applyHostTemplates`: replace `{{char}}` / `{{user}}` with the supplied
/// names, leaving *unbound* tokens untouched (partial-binding contexts). No-op
/// when the text has no `{{`.
pub fn apply_host_templates(
    text: &str,
    char_name: Option<&str>,
    user_name: Option<&str>,
) -> String {
    if text.is_empty() || !text.contains("{{") {
        return text.to_string();
    }
    let mut result = text.to_string();
    if let Some(cn) = char_name {
        if !cn.is_empty() {
            result = result.replace("{{char}}", cn);
        }
    }
    if let Some(un) = user_name {
        if !un.is_empty() {
            result = result.replace("{{user}}", un);
        }
    }
    result
}

/// An off-scene character card (v4 `OffSceneCharacterCard`), built from a
/// user-character read.
#[derive(Clone, Debug)]
pub struct OffSceneCharacterCard {
    pub id: String,
    pub name: String,
    pub aliases: Vec<String>,
    /// `(subject, object, possessive)`.
    pub pronouns: Option<(String, String, String)>,
    pub identity: Option<String>,
    pub description: Option<String>,
}

impl OffSceneCharacterCard {
    /// Build a card from an overlaid character `Value` (v4
    /// `characters.findByUserId(...)` rows). Absent optional fields → `None`.
    fn from_value(v: &Value) -> Option<Self> {
        let id = v.get("id").and_then(Value::as_str)?.to_string();
        let name = v
            .get("name")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string();
        let aliases = v
            .get("aliases")
            .and_then(Value::as_array)
            .map(|a| {
                a.iter()
                    .filter_map(Value::as_str)
                    .map(str::to_string)
                    .collect()
            })
            .unwrap_or_default();
        let pronouns = v.get("pronouns").and_then(Value::as_object).and_then(|p| {
            let s = p.get("subject").and_then(Value::as_str)?.to_string();
            let o = p.get("object").and_then(Value::as_str)?.to_string();
            let poss = p.get("possessive").and_then(Value::as_str)?.to_string();
            Some((s, o, poss))
        });
        let identity = v
            .get("identity")
            .and_then(Value::as_str)
            .map(str::to_string);
        let description = v
            .get("description")
            .and_then(Value::as_str)
            .map(str::to_string);
        Some(OffSceneCharacterCard {
            id,
            name,
            aliases,
            pronouns,
            identity,
            description,
        })
    }
}

/// v4 `renderOffSceneCard`: `### <name>` + optional aliases/pronouns lines +
/// (identity else description) body with `{{char}}`/`{{user}}` substitution.
fn render_off_scene_card(c: &OffSceneCharacterCard, user_character_name: Option<&str>) -> String {
    let mut lines: Vec<String> = vec![format!("### {}", c.name)];
    if !c.aliases.is_empty() {
        lines.push(format!("Aliases: {}", c.aliases.join(", ")));
    }
    if let Some((s, o, p)) = &c.pronouns {
        lines.push(format!("Pronouns: {s}/{o}/{p}"));
    }
    // `(c.identity ?? '').trim() || (c.description ?? '').trim()`.
    let identity_trim = c
        .identity
        .as_deref()
        .map(crate::jsstr::js_trim)
        .unwrap_or("");
    let body = if !identity_trim.is_empty() {
        identity_trim.to_string()
    } else {
        c.description
            .as_deref()
            .map(|s| crate::jsstr::js_trim(s).to_string())
            .unwrap_or_default()
    };
    if !body.is_empty() {
        lines.push(apply_host_templates(
            &body,
            Some(&c.name),
            user_character_name,
        ));
    }
    lines.join("\n")
}

/// v4 `buildOffSceneCharactersContent`: the persona-voiced Host introduction
/// (sorted alphabetically by name via `localeCompare`).
pub fn build_off_scene_characters_content(
    characters: &[OffSceneCharacterCard],
    user_character_name: Option<&str>,
) -> String {
    let mut sorted: Vec<&OffSceneCharacterCard> = characters.iter().collect();
    sorted.sort_by(|a, b| locale_compare(&a.name, &b.name));

    let intro = if sorted.len() == 1 {
        "The Host begs leave to introduce a person spoken of in this conversation but not presently in the Salon — for accurate reference only; not a summons to the scene."
    } else {
        "The Host begs leave to introduce certain persons spoken of in this conversation but not presently in the Salon — for accurate reference only; not a summons to the scene."
    };

    let mut parts: Vec<String> = vec![intro.to_string(), String::new()];
    parts.extend(
        sorted
            .iter()
            .map(|c| render_off_scene_card(c, user_character_name)),
    );
    parts.join("\n\n")
}

/// v4 `buildOffSceneCharactersOpaqueContent`: the persona-stripped form (used by
/// the persisted message's opaque swap — W4.6b). Ported for the differential.
pub fn build_off_scene_characters_opaque_content(
    characters: &[OffSceneCharacterCard],
    user_character_name: Option<&str>,
) -> String {
    let mut sorted: Vec<&OffSceneCharacterCard> = characters.iter().collect();
    sorted.sort_by(|a, b| locale_compare(&a.name, &b.name));

    let intro = if sorted.len() == 1 {
        "A person spoken of in this conversation but not presently in the scene — for accurate reference only; not a summons to the scene:"
    } else {
        "Persons spoken of in this conversation but not presently in the scene — for accurate reference only; not a summons to the scene:"
    };

    let mut parts: Vec<String> = vec![intro.to_string(), String::new()];
    parts.extend(
        sorted
            .iter()
            .map(|c| render_off_scene_card(c, user_character_name)),
    );
    parts.join("\n\n")
}

/// v4 `findIntroducedOffSceneCharacterIds`: scan chat-message rows for prior Host
/// off-scene introductions and collect the character ids already introduced.
pub fn find_introduced_off_scene_character_ids(messages: &[Value]) -> BTreeSet<String> {
    let mut introduced: BTreeSet<String> = BTreeSet::new();
    for m in messages {
        let Some(obj) = m.as_object() else { continue };
        let is_off_scene = obj.get("type").and_then(Value::as_str) == Some("message")
            && obj.get("systemSender").and_then(Value::as_str) == Some("host")
            && obj.get("systemKind").and_then(Value::as_str) == Some("off-scene-characters");
        if !is_off_scene {
            continue;
        }
        if let Some(ids) = obj
            .get("hostEvent")
            .and_then(|h| h.get("introducedCharacterIds"))
            .and_then(Value::as_array)
        {
            for id in ids {
                if let Some(s) = id.as_str() {
                    introduced.insert(s.to_string());
                }
            }
        }
    }
    introduced
}

/// A chat participant, the subset the off-scene exclusion + user-name resolution
/// needs (built from `build_context`'s `FullParticipant`).
#[derive(Clone, Debug)]
pub struct OffSceneParticipant {
    pub participant_type: String,
    pub character_id: Option<String>,
    pub controlled_by: String,
    pub status: String,
}

/// v4's off-scene SCAN block (`context-manager.ts`) — compute the pending Host
/// announcement CONTENT for this turn, or `None`. The POST is W4.6b.
///
/// Reads all of the user's characters (overlaid), excludes the responding
/// character + every non-`removed` CHARACTER participant + the user persona by
/// name, scans the tool-stripped chat corpus for mentions, diffs against
/// already-introduced ids, and returns the genuine-newcomer cards (the Host POST
/// builds the announcement content + stamps `introducedCharacterIds` from these).
/// Any failure / no newcomers → `None` (v4 wraps the block warn-only).
pub fn scan_off_scene_newcomers(
    db: &Db,
    chat_id: &str,
    user_id: &str,
    responding_character_id: &str,
    user_character_name: Option<&str>,
    all_participants: &[OffSceneParticipant],
) -> Option<Vec<OffSceneCharacterCard>> {
    // v4: the whole block is warn-only — a DB failure yields no announcement.
    scan_off_scene_newcomers_inner(
        db,
        chat_id,
        user_id,
        responding_character_id,
        user_character_name,
        all_participants,
    )
    .unwrap_or_default()
}

fn scan_off_scene_newcomers_inner(
    db: &Db,
    chat_id: &str,
    user_id: &str,
    responding_character_id: &str,
    user_character_name: Option<&str>,
    all_participants: &[OffSceneParticipant],
) -> Result<Option<Vec<OffSceneCharacterCard>>, DbError> {
    // 1. All of the user's characters (overlaid — carries name/aliases/pronouns/
    //    identity/description). `find_by_user_id` spans both DBs (main slim rows +
    //    the mount-index vault overlay), read via the nested-pool pattern.
    let all_user_characters = db.read_main(|main| {
        db.read_mount_index(|mount| {
            crate::db::characters_read::find_by_user_id(main, mount, user_id)
        })
    })?;

    // 2. Excluded ids: responding character + every non-removed CHARACTER
    //    participant.
    let mut excluded: BTreeSet<String> = BTreeSet::new();
    excluded.insert(responding_character_id.to_string());
    for p in all_participants {
        if p.participant_type == "CHARACTER" && p.status != "removed" {
            if let Some(cid) = &p.character_id {
                excluded.insert(cid.clone());
            }
        }
    }

    // 3. The user persona, excluded by name (no id exposed via options).
    let user_name_lower = user_character_name.map(|n| crate::jsstr::js_trim(n).to_lowercase());

    // 4. Candidates = user characters minus excluded (by id) minus user-persona
    //    name-match.
    let candidates: Vec<OffSceneCharacterCard> = all_user_characters
        .iter()
        .filter_map(OffSceneCharacterCard::from_value)
        .filter(|c| {
            if excluded.contains(&c.id) {
                return false;
            }
            if let Some(unl) = &user_name_lower {
                if !unl.is_empty() && crate::jsstr::js_trim(&c.name).to_lowercase() == *unl {
                    return false;
                }
            }
            true
        })
        .collect();

    if candidates.is_empty() {
        return Ok(None);
    }

    // 5. Scan corpus from FULL chat messages (things actually SAID).
    let full_chat_messages = {
        let cid = chat_id.to_string();
        db.read_main(move |conn| crate::db::chats_messages_read::get_messages(conn, &cid))?
    };
    let mut corpus_parts: Vec<String> = Vec::new();
    for m in &full_chat_messages {
        let Some(obj) = m.as_object() else { continue };
        // `m.type !== undefined && m.type !== 'message'` → skip.
        if let Some(t) = obj.get("type").and_then(Value::as_str) {
            if t != "message" {
                continue;
            }
        }
        let content = obj.get("content").and_then(Value::as_str).unwrap_or("");
        if content.is_empty() {
            continue;
        }
        // Skip synthetic system whispers/announcements (`m.systemSender` truthy).
        if obj
            .get("systemSender")
            .and_then(Value::as_str)
            .is_some_and(|s| !s.is_empty())
        {
            continue;
        }
        let role = obj
            .get("role")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_uppercase();
        if role != "USER" && role != "ASSISTANT" {
            continue;
        }
        if role == "ASSISTANT" {
            match crate::chat_tasks::strip_tool_artifacts(content) {
                Some(cleaned) if !cleaned.is_empty() => corpus_parts.push(cleaned),
                _ => continue,
            }
        } else {
            corpus_parts.push(content.to_string());
        }
    }
    let scan_corpus = corpus_parts.join("\n");

    // 6. Mention scan (Phase-1 leaf).
    let mention_candidates: Vec<MentionCandidate> = candidates
        .iter()
        .map(|c| MentionCandidate {
            id: c.id.clone(),
            name: c.name.clone(),
            aliases: c.aliases.clone(),
        })
        .collect();
    let matched = find_mentioned_character_ids(&scan_corpus, &mention_candidates);
    if matched.is_empty() {
        return Ok(None);
    }

    // 7. Diff against already-introduced ids → newcomers.
    let introduced = find_introduced_off_scene_character_ids(&full_chat_messages);
    let newcomer_ids: BTreeSet<String> = matched
        .into_iter()
        .filter(|id| !introduced.contains(id))
        .collect();
    if newcomer_ids.is_empty() {
        return Ok(None);
    }

    // 8. Return the newcomer cards. The Host POST (W4.6b) builds the announcement
    //    content itself (resolving the user-character name from the chat) and stamps
    //    `hostEvent.introducedCharacterIds`, so we hand it the structured cards
    //    rather than a pre-rendered string.
    let newcomers: Vec<OffSceneCharacterCard> = candidates
        .into_iter()
        .filter(|c| newcomer_ids.contains(&c.id))
        .collect();
    Ok(Some(newcomers))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn card(id: &str, name: &str) -> OffSceneCharacterCard {
        OffSceneCharacterCard {
            id: id.into(),
            name: name.into(),
            aliases: vec![],
            pronouns: None,
            identity: None,
            description: None,
        }
    }

    #[test]
    fn apply_host_templates_substitutes_and_preserves_unbound() {
        assert_eq!(
            apply_host_templates("Hi {{char}}, from {{user}}.", Some("Ada"), Some("Bob")),
            "Hi Ada, from Bob."
        );
        // Unbound {{user}} preserved when user_name is None.
        assert_eq!(
            apply_host_templates("Hi {{char}} & {{user}}.", Some("Ada"), None),
            "Hi Ada & {{user}}."
        );
        // No `{{` → verbatim.
        assert_eq!(
            apply_host_templates("plain", Some("Ada"), Some("Bob")),
            "plain"
        );
    }

    #[test]
    fn render_card_identity_over_description() {
        let mut c = card("id1", "Ada");
        c.identity = Some("  {{char}} is a coder.  ".into());
        c.description = Some("ignored".into());
        assert_eq!(
            render_off_scene_card(&c, Some("Bob")),
            "### Ada\nAda is a coder."
        );
    }

    #[test]
    fn render_card_aliases_pronouns_description_fallback() {
        let mut c = card("id1", "Ada");
        c.aliases = vec!["A".into(), "Ace".into()];
        c.pronouns = Some(("she".into(), "her".into(), "hers".into()));
        c.description = Some("A brave soul.".into());
        assert_eq!(
            render_off_scene_card(&c, None),
            "### Ada\nAliases: A, Ace\nPronouns: she/her/hers\nA brave soul."
        );
    }

    #[test]
    fn build_content_single_vs_plural_and_sort() {
        let single =
            build_off_scene_characters_content(std::slice::from_ref(&card("i", "Zed")), None);
        assert!(single.starts_with("The Host begs leave to introduce a person"));
        assert!(single.ends_with("### Zed"));

        let plural =
            build_off_scene_characters_content(&[card("i2", "Zed"), card("i1", "Ada")], None);
        assert!(plural.starts_with("The Host begs leave to introduce certain persons"));
        // Alphabetical: Ada before Zed.
        let ada_pos = plural.find("### Ada").unwrap();
        let zed_pos = plural.find("### Zed").unwrap();
        assert!(ada_pos < zed_pos);
    }

    #[test]
    fn find_introduced_ids_from_host_rows() {
        let rows = vec![
            json!({"type":"message","systemSender":"host","systemKind":"off-scene-characters",
                   "hostEvent":{"introducedCharacterIds":["c1","c2"]}}),
            json!({"type":"message","systemSender":"aurora","systemKind":"core-whisper"}),
            json!({"type":"message","role":"USER","content":"hi"}),
            json!({"type":"message","systemSender":"host","systemKind":"off-scene-characters",
                   "hostEvent":{"introducedCharacterIds":["c2","c3"]}}),
        ];
        let ids = find_introduced_off_scene_character_ids(&rows);
        assert_eq!(
            ids,
            ["c1", "c2", "c3"]
                .iter()
                .map(|s| s.to_string())
                .collect::<BTreeSet<_>>()
        );
    }
}
