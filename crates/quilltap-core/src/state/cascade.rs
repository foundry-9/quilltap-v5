//! Shared cascade resolver for the four-tier persistent state system — v4
//! `lib/state/state-cascade.ts` (NEW at `f48f34dc`):
//! **chat → project → group → general**.
//!
//! This is the single merge implementation, replacing the duplicated
//! `mergeState` helpers that previously lived in the state tool handler and the
//! chat get-state API route. It is consumed by:
//!   - the `state` LLM tool handler (`tools::state`)
//!   - the chat get-state dispatch verb
//!   - both Pascal `$state` entrances (LLM run + manual popup)
//!
//! ## Precedence (shallow, top-level keys only)
//!
//!   merged = { ...general, ...group, ...project, ...chat }   // chat wins
//!
//! ## Group tier — the exactly-one rule
//!
//! A chat can surface more than one applicable group (a character in several
//! groups; or, in the participants-union scope, several characters each in
//! different groups). Merging an arbitrary one would be a silent lie, so the
//! group tier only contributes to `merged` when **exactly one** group applies.
//! With two or more the tier reports `status: 'ambiguous'` and merges nothing —
//! that group state is then reachable only by declaring the group explicitly
//! (see [`resolve_group_for_context`]).

use rusqlite::Connection;
use serde::Serialize;
use serde_json::{Map, Value};

use crate::db::group_character_members::GroupCharacterMembersRepository;
use crate::db::groups::GroupsRepository;
use crate::db::projects::ProjectsRepository;
use crate::services::mount_index::general_state::read_general_state;

/// How to determine which group(s) apply to a state read — v4 `GroupScope`.
///
/// - `Character`: the responding character's own memberships (Knowledge's rule —
///   a character only sees its own groups). Used by the LLM tool and Pascal.
/// - `ParticipantsUnion`: the union across the chat's active character
///   participants (`type === 'CHARACTER' && status !== 'removed'`; deliberately
///   NOT filtered by `controlledBy`). Used by the API/UI merged view.
/// - `None`: no group tier at all.
#[derive(Debug, Clone)]
pub enum GroupScope {
    Character { character_id: String },
    ParticipantsUnion,
    None,
}

/// A resolvable group, trimmed to what the UI/errors need (v4 `GroupCandidate`).
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct GroupCandidate {
    pub id: String,
    pub name: String,
}

/// The outcome of applying the exactly-one rule (v4 `GroupTier`).
/// - `none`: no candidate groups.
/// - `single`: exactly one; its state merged; `appliedGroupId` set.
/// - `ambiguous`: two or more; nothing merged.
#[derive(Debug, Clone, Serialize)]
pub struct GroupTier {
    /// `'none' | 'single' | 'ambiguous'`.
    pub status: &'static str,
    pub candidates: Vec<GroupCandidate>,
    #[serde(rename = "appliedGroupId", skip_serializing_if = "Option::is_none")]
    pub applied_group_id: Option<String>,
}

/// v4 `StateCascadeResult`. All tier objects are JSON objects (`{}` when empty).
#[derive(Debug, Clone)]
pub struct StateCascadeResult {
    pub chat_state: Value,
    pub project_state: Value,
    pub group_state: Value,
    pub general_state: Value,
    pub merged: Value,
    pub group_tier: GroupTier,
    /// `chat.projectId || undefined`.
    pub project_id: Option<String>,
}

/// Error codes for [`resolve_group_for_context`] (v4 `StateGroupResolutionCode`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StateGroupResolutionCode {
    GroupNotFound,
    GroupAmbiguous,
    NoGroups,
    GroupRefRequired,
}

impl StateGroupResolutionCode {
    /// v4's literal code strings.
    pub fn as_str(&self) -> &'static str {
        match self {
            StateGroupResolutionCode::GroupNotFound => "GROUP_NOT_FOUND",
            StateGroupResolutionCode::GroupAmbiguous => "GROUP_AMBIGUOUS",
            StateGroupResolutionCode::NoGroups => "NO_GROUPS",
            StateGroupResolutionCode::GroupRefRequired => "GROUP_REF_REQUIRED",
        }
    }
}

/// Thrown when an explicit group-context op cannot pin down exactly one group
/// (v4 `StateGroupResolutionError`). Carries the candidate list so the caller
/// can surface a helpful message; `Display` is v4's message verbatim.
#[derive(Debug, Clone)]
pub struct StateGroupResolutionError {
    pub message: String,
    pub code: StateGroupResolutionCode,
    pub candidates: Vec<GroupCandidate>,
}

impl std::fmt::Display for StateGroupResolutionError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for StateGroupResolutionError {}

/// v4 `asStateObject`: coerce a non-object / array / null value to `{}`.
pub fn as_state_object(value: Option<&Value>) -> Value {
    match value {
        Some(v) if v.is_object() => v.clone(),
        _ => Value::Object(Map::new()),
    }
}

/// Format a candidate list as `"Name" (id)` joined by `, ` for error messages.
fn format_candidates(candidates: &[GroupCandidate]) -> String {
    candidates
        .iter()
        .map(|c| format!("\"{}\" ({})", c.name, c.id))
        .collect::<Vec<_>>()
        .join(", ")
}

/// Collect the group IDs implied by a scope, deduplicated (v4 `collectGroupIds`
/// — a JS `Set` preserves insertion order). Reads memberships via the hot-path
/// `findByCharacterId` index; v4's repo method rides `safeQuery` (returns `[]`
/// on error, never throws), mirrored here — a missing mount-index (where the
/// membership table lives) reads as no memberships.
fn collect_group_ids(mount: Option<&Connection>, chat: &Value, scope: &GroupScope) -> Vec<String> {
    let mut ids: Vec<String> = Vec::new();
    let add_for_character = |character_id: &str, ids: &mut Vec<String>| {
        let memberships = mount
            .map(|mount| {
                GroupCharacterMembersRepository::new(mount)
                    .find_group_ids_by_character_id(character_id)
                    .unwrap_or_default()
            })
            .unwrap_or_default();
        for group_id in memberships {
            if !ids.contains(&group_id) {
                ids.push(group_id);
            }
        }
    };

    match scope {
        GroupScope::Character { character_id } => add_for_character(character_id, &mut ids),
        GroupScope::ParticipantsUnion => {
            let participants = chat
                .get("participants")
                .and_then(Value::as_array)
                .cloned()
                .unwrap_or_default();
            for p in &participants {
                // Union across active character participants. Deliberately NOT filtered
                // by `controlledBy` — the merged view spans every character at the table.
                if p.get("type").and_then(Value::as_str) != Some("CHARACTER") {
                    continue;
                }
                let Some(character_id) = p
                    .get("characterId")
                    .and_then(Value::as_str)
                    .filter(|s| !s.is_empty())
                else {
                    continue;
                };
                if p.get("status").and_then(Value::as_str) == Some("removed") {
                    continue;
                }
                add_for_character(character_id, &mut ids);
            }
        }
        // scope 'none' → no ids.
        GroupScope::None => {}
    }
    ids
}

/// v4 `resolveGroupCandidates`: hydrate the candidate groups for a scope,
/// fail-soft per group: a group whose store is unavailable is logged and
/// dropped rather than failing the read. Returns hydrated group rows.
///
/// `mount` is optional: with no mount-index every hydration fails (dropped),
/// mirroring the degraded-open behaviour.
pub fn resolve_group_candidates(
    main: &Connection,
    mount: Option<&Connection>,
    chat: &Value,
    scope: &GroupScope,
) -> Vec<Value> {
    let ids = collect_group_ids(mount, chat, scope);
    let mut groups: Vec<Value> = Vec::new();
    for id in ids {
        let hydrated = mount.and_then(|mount| {
            match GroupsRepository::new(main, mount).find_by_id(&id) {
                Ok(g) => g,
                Err(e) => {
                    eprintln!(
                        "[StateCascade] Could not hydrate a candidate group; skipping (group {id}): {e}"
                    );
                    None
                }
            }
        });
        if let Some(group) = hydrated {
            groups.push(group);
        }
    }
    groups
}

fn candidate_brief(groups: &[Value]) -> Vec<GroupCandidate> {
    groups
        .iter()
        .map(|g| GroupCandidate {
            id: g
                .get("id")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string(),
            name: g
                .get("name")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string(),
        })
        .collect()
}

/// v4 `resolveStateCascade`: resolve the full four-tier cascade for a chat,
/// applying the exactly-one rule to the group tier. Project and general tiers
/// degrade to `{}` on any failure (state is enrichment, never load-bearing for
/// the chat's own read).
pub fn resolve_state_cascade(
    main: &Connection,
    mount: Option<&Connection>,
    chat: &Value,
    group_scope: &GroupScope,
) -> StateCascadeResult {
    let chat_state = as_state_object(chat.get("state"));

    // --- Project tier (graceful degradation) ---
    let mut project_state = Value::Object(Map::new());
    // `chat.projectId || undefined` — empty string is falsy.
    let project_id = chat
        .get("projectId")
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty())
        .map(str::to_string);
    if let Some(pid) = &project_id {
        // Any throw → warn + {} (a missing mount-index counts as unavailable).
        let loaded = mount.and_then(|mount| {
            match ProjectsRepository::new(main, mount).find_by_id(pid) {
                Ok(p) => p,
                Err(e) => {
                    eprintln!(
                        "[StateCascade] Could not load project state for merge; using {{}} (project {pid}): {e}"
                    );
                    None
                }
            }
        });
        if let Some(project) = loaded {
            project_state = as_state_object(project.get("state"));
        }
    }

    // --- Group tier (exactly-one rule) ---
    let candidate_groups = resolve_group_candidates(main, mount, chat, group_scope);
    let candidates = candidate_brief(&candidate_groups);

    let mut group_state = Value::Object(Map::new());
    let group_tier = if candidate_groups.is_empty() {
        GroupTier {
            status: "none",
            candidates,
            applied_group_id: None,
        }
    } else if candidate_groups.len() == 1 {
        group_state = as_state_object(candidate_groups[0].get("state"));
        GroupTier {
            status: "single",
            applied_group_id: candidate_groups[0]
                .get("id")
                .and_then(Value::as_str)
                .map(str::to_string),
            candidates,
        }
    } else {
        // Two or more → skip the tier in the merged view.
        GroupTier {
            status: "ambiguous",
            candidates,
            applied_group_id: None,
        }
    };

    // --- General tier (already fail-soft) ---
    let general_state = read_general_state(main, mount);

    // --- Merge (chat wins) --- JS spread: later spreads override, overridden
    // keys keep their FIRST insertion position.
    let mut merged = Map::new();
    for tier in [&general_state, &group_state, &project_state, &chat_state] {
        if let Value::Object(m) = tier {
            for (k, v) in m {
                merged.insert(k.clone(), v.clone());
            }
        }
    }

    StateCascadeResult {
        chat_state,
        project_state,
        group_state,
        general_state,
        merged: Value::Object(merged),
        group_tier,
        project_id,
    }
}

/// v4 `resolveGroupForContext`: pin down exactly one group for an explicit
/// group-context operation. `candidates` are hydrated group rows.
///
/// Policy:
///   - no candidates → `NO_GROUPS`
///   - omitted ref + exactly one candidate → that candidate
///   - omitted ref + 2+ candidates → `GROUP_REF_REQUIRED`
///   - ref matches a candidate id → that candidate
///   - else case-insensitive exact name match **among candidates only**:
///     one match → it; multiple → `GROUP_AMBIGUOUS`; none → `GROUP_NOT_FOUND`
pub fn resolve_group_for_context<'g>(
    group_ref: Option<&str>,
    candidates: &'g [Value],
) -> Result<&'g Value, StateGroupResolutionError> {
    let brief = candidate_brief(candidates);

    if candidates.is_empty() {
        return Err(StateGroupResolutionError {
            message:
                "This character does not belong to any group, so there is no group state to reach."
                    .to_string(),
            code: StateGroupResolutionCode::NoGroups,
            candidates: brief,
        });
    }

    // `groupRef?.trim()`; `if (!ref)` — empty after trim counts as omitted.
    let r = group_ref.map(str::trim).filter(|s| !s.is_empty());
    let Some(r) = r else {
        if candidates.len() == 1 {
            return Ok(&candidates[0]);
        }
        return Err(StateGroupResolutionError {
            message: format!(
                "More than one group applies; specify one by name or id: {}.",
                format_candidates(&brief)
            ),
            code: StateGroupResolutionCode::GroupRefRequired,
            candidates: brief,
        });
    };

    // Exact id match wins outright.
    if let Some(by_id) = candidates
        .iter()
        .find(|g| g.get("id").and_then(Value::as_str) == Some(r))
    {
        return Ok(by_id);
    }

    // Case-insensitive exact name match, among candidates only. JS
    // `toLowerCase()` — `str::to_lowercase` is byte-identical to JS (Phase 1).
    let lowered = r.to_lowercase();
    let by_name: Vec<&Value> = candidates
        .iter()
        .filter(|g| {
            g.get("name")
                .and_then(Value::as_str)
                .map(|n| n.to_lowercase() == lowered)
                .unwrap_or(false)
        })
        .collect();
    if by_name.len() == 1 {
        return Ok(by_name[0]);
    }
    if by_name.len() > 1 {
        let dup_brief =
            candidate_brief(&by_name.iter().map(|g| (*g).clone()).collect::<Vec<Value>>());
        return Err(StateGroupResolutionError {
            message: format!(
                "More than one group is named \"{r}\"; specify one by id: {}.",
                format_candidates(&dup_brief)
            ),
            code: StateGroupResolutionCode::GroupAmbiguous,
            candidates: brief,
        });
    }

    Err(StateGroupResolutionError {
        message: format!(
            "No group matching \"{r}\" among this character's groups: {}.",
            format_candidates(&brief)
        ),
        code: StateGroupResolutionCode::GroupNotFound,
        candidates: brief,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn group(id: &str, name: &str) -> Value {
        json!({ "id": id, "name": name, "state": {} })
    }

    #[test]
    fn group_for_context_policy() {
        // NO_GROUPS.
        let err = resolve_group_for_context(None, &[]).unwrap_err();
        assert_eq!(err.code, StateGroupResolutionCode::NoGroups);
        assert_eq!(
            err.message,
            "This character does not belong to any group, so there is no group state to reach."
        );

        let groups = vec![group("grp-1", "Alpha"), group("grp-2", "Beta")];
        // Sole candidate, ref omitted.
        let sole = vec![group("grp-1", "Alpha")];
        assert_eq!(
            resolve_group_for_context(None, &sole)
                .unwrap()
                .get("id")
                .unwrap(),
            "grp-1"
        );
        // Omitted + 2 → GROUP_REF_REQUIRED with the formatted candidate list.
        let err = resolve_group_for_context(None, &groups).unwrap_err();
        assert_eq!(err.code, StateGroupResolutionCode::GroupRefRequired);
        assert_eq!(
            err.message,
            "More than one group applies; specify one by name or id: \"Alpha\" (grp-1), \"Beta\" (grp-2)."
        );
        // By id.
        assert_eq!(
            resolve_group_for_context(Some("grp-2"), &groups)
                .unwrap()
                .get("id")
                .unwrap(),
            "grp-2"
        );
        // Case-insensitive name.
        assert_eq!(
            resolve_group_for_context(Some("alpha"), &groups)
                .unwrap()
                .get("id")
                .unwrap(),
            "grp-1"
        );
        // Not found.
        let err = resolve_group_for_context(Some("Zed"), &groups).unwrap_err();
        assert_eq!(err.code, StateGroupResolutionCode::GroupNotFound);
        assert_eq!(
            err.message,
            "No group matching \"Zed\" among this character's groups: \"Alpha\" (grp-1), \"Beta\" (grp-2)."
        );
        // Ambiguous name (case-insensitive collision).
        let dup = vec![group("grp-1", "Alpha"), group("grp-3", "alpha")];
        let err = resolve_group_for_context(Some("Alpha"), &dup).unwrap_err();
        assert_eq!(err.code, StateGroupResolutionCode::GroupAmbiguous);
        assert_eq!(
            err.message,
            "More than one group is named \"Alpha\"; specify one by id: \"Alpha\" (grp-1), \"alpha\" (grp-3)."
        );
        // Whitespace-only ref counts as omitted.
        let err = resolve_group_for_context(Some("   "), &groups).unwrap_err();
        assert_eq!(err.code, StateGroupResolutionCode::GroupRefRequired);
    }

    #[test]
    fn group_tier_serializes_camel_case() {
        let single = GroupTier {
            status: "single",
            candidates: vec![GroupCandidate {
                id: "g1".into(),
                name: "Alpha".into(),
            }],
            applied_group_id: Some("g1".into()),
        };
        assert_eq!(
            serde_json::to_string(&single).unwrap(),
            r#"{"status":"single","candidates":[{"id":"g1","name":"Alpha"}],"appliedGroupId":"g1"}"#
        );
        let none = GroupTier {
            status: "none",
            candidates: vec![],
            applied_group_id: None,
        };
        assert_eq!(
            serde_json::to_string(&none).unwrap(),
            r#"{"status":"none","candidates":[]}"#
        );
    }
}
