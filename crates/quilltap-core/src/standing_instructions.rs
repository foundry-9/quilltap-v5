//! Standing Instructions — v4 `lib/chat/context/standing-instructions.ts`
//! (`8f868109`).
//!
//! Projects and groups each carry an optional `instructions` field ("the
//! prompt") that is injected into the system prompt of every character speaking
//! in scope of that entity:
//!
//!   - **Project instructions** apply to every chat with `chat.projectId` set.
//!   - **Group instructions** apply per *responding character* — a character in
//!     scope of a turn receives the instructions of every group they are a
//!     member of, regardless of which chat the turn is in. (Same doctrine as
//!     the group document-store tier: membership follows the character, never
//!     the chat.)
//!
//! The rendered section sits inside system block 1 — the cacheable prefix —
//! between the Taboo section and the tool instructions. Like Taboo, it is
//! stable across turns (it changes only when the user edits a project/group or
//! changes a membership) and emits nothing at all when there is nothing to say,
//! so chats without standing instructions build byte-identical prompts to the
//! pre-feature layout.
//!
//! Help and Brahma chats never receive the section: they have their own prompt
//! builders that do not call `buildSystemPrompt`. Carina one-off queries DO
//! receive it (mirrored insertion in [`crate::services::carina_query`]).
//!
//! Unlike Taboo phrases, instructions ARE template-processed by the consumer
//! (`{{char}}`/`{{user}}` etc.), matching the character system-prompt and
//! roleplay-template precedent — a group prompt legitimately wants to address
//! "{{char}}" when several member characters share it.

use crate::collation::locale_compare;
use crate::db::runtime::Db;
use crate::db::DbError;
use crate::jsstr::js_trim;

/// The kind of entity a standing-instruction source came from — v4's
/// `StandingInstructionsSource['kind']`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StandingInstructionsKind {
    Project,
    Group,
}

/// One entity's contribution to the standing-instructions section — v4
/// `StandingInstructionsSource`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StandingInstructionsSource {
    pub kind: StandingInstructionsKind,
    pub name: String,
    /// Already trimmed by the resolver (v4 pushes `project.instructions.trim()`).
    pub instructions: String,
}

/// Preamble of the standing-instructions section (v4
/// `STANDING_INSTRUCTIONS_PREAMBLE`). Follows the universal-section precedent
/// ([`crate::system_prompt::render_taboo_section`]'s preamble, the math note):
/// bracketed all-caps tag, imperative, addressed to the speaking character.
/// The "refine, never replace" clause keeps the character's own identity stack
/// primary when a project or group prompt brushes against it.
const STANDING_INSTRUCTIONS_PREAMBLE: &str = "[STANDING INSTRUCTIONS]\nThe sections below are standing instructions attached to this chat's project and to groups you belong to. They hold for the entire conversation. They refine how you conduct yourself here; they never replace who you are.";

/// Resolve the standing-instruction sources for a turn: the chat's project
/// (when `project_id` is set) followed by every group the responding character
/// belongs to, sorted by group name (then instructions) for cache determinism.
///
/// Every lookup fails soft: a missing project, a broken official store, or a
/// degraded mount-index DB drops that source rather than losing the turn.
/// Entities whose `instructions` are empty or whitespace contribute nothing.
///
/// NB v4's doc comment says the tie-break is "(then id)", but the code
/// tie-breaks on `instructions` — the CODE is what is ported here.
pub fn resolve_standing_instructions(
    db: &Db,
    project_id: Option<&str>,
    character_id: Option<&str>,
) -> Vec<StandingInstructionsSource> {
    let mut sources: Vec<StandingInstructionsSource> = Vec::new();

    // `if (projectId)` — JS truthiness, so an empty string is skipped too.
    if let Some(project_id) = project_id.filter(|p| !p.is_empty()) {
        match read_project(db, project_id) {
            Ok(Some(source)) => sources.push(source),
            Ok(None) => {}
            Err(e) => {
                tracing::warn!(
                    project_id = %project_id,
                    error = %e,
                    "[StandingInstructions] Failed to load project instructions — continuing without them"
                );
            }
        }
    }

    if let Some(character_id) = character_id.filter(|c| !c.is_empty()) {
        match db.read_mount_index(|mount| {
            crate::db::group_character_members::GroupCharacterMembersRepository::new(mount)
                .find_group_ids_by_character_id(character_id)
        }) {
            Ok(memberships) => {
                let mut group_sources: Vec<StandingInstructionsSource> = Vec::new();
                for group_id in memberships {
                    match read_group(db, &group_id) {
                        Ok(Some(source)) => group_sources.push(source),
                        Ok(None) => {}
                        Err(e) => {
                            tracing::warn!(
                                group_id = %group_id,
                                character_id = %character_id,
                                error = %e,
                                "[StandingInstructions] Failed to load group instructions — skipping that group"
                            );
                        }
                    }
                }
                // Deterministic order: membership rows carry no meaningful order,
                // and a Map/array-order wobble here would bisect the provider
                // cache prefix. `localeCompare` in both legs — v5's ICU4X en-US
                // twin, never byte order.
                group_sources.sort_by(|a, b| {
                    locale_compare(&a.name, &b.name)
                        .then_with(|| locale_compare(&a.instructions, &b.instructions))
                });
                sources.extend(group_sources);
            }
            Err(e) => {
                tracing::warn!(
                    character_id = %character_id,
                    error = %e,
                    "[StandingInstructions] Failed to load group memberships — continuing without group instructions"
                );
            }
        }
    }

    sources
}

/// `repos.projects.findById(projectId)` → the source, when the project exists
/// and its trimmed `instructions` are non-empty.
fn read_project(db: &Db, project_id: &str) -> Result<Option<StandingInstructionsSource>, DbError> {
    let project = db.read_main(|main| {
        db.read_mount_index(|mount| {
            crate::db::projects::ProjectsRepository::new(main, mount)
                .find_by_id(project_id)
                .map_err(overlay_to_db_err)
        })
    })?;
    Ok(project.and_then(|p| to_source(StandingInstructionsKind::Project, &p)))
}

/// `repos.groups.findById(membership.groupId)` — same shape as the project leg.
fn read_group(db: &Db, group_id: &str) -> Result<Option<StandingInstructionsSource>, DbError> {
    let group = db.read_main(|main| {
        db.read_mount_index(|mount| {
            crate::db::groups::GroupsRepository::new(main, mount)
                .find_by_id(group_id)
                .map_err(overlay_to_db_err)
        })
    })?;
    Ok(group.and_then(|g| to_source(StandingInstructionsKind::Group, &g)))
}

fn overlay_to_db_err(e: crate::db::document_store_overlay::OverlayError) -> DbError {
    match e {
        crate::db::document_store_overlay::OverlayError::Db(d) => d,
        other => DbError::Internal(other.to_string()),
    }
}

/// `const instructions = entity?.instructions?.trim(); if (entity && instructions)`
/// — a non-string, absent, or all-whitespace value contributes nothing.
fn to_source(
    kind: StandingInstructionsKind,
    entity: &serde_json::Value,
) -> Option<StandingInstructionsSource> {
    let instructions = js_trim(entity.get("instructions")?.as_str()?);
    if instructions.is_empty() {
        return None;
    }
    let name = entity.get("name")?.as_str()?.to_string();
    Some(StandingInstructionsSource {
        kind,
        name,
        instructions: instructions.to_string(),
    })
}

/// Render the standing-instructions section, or `None` when there is nothing
/// to say — no header, no blank block, byte-identical to the pre-feature
/// prompt (the Taboo contract). v4 `renderStandingInstructionsSection`.
pub fn render_standing_instructions_section(
    sources: &[StandingInstructionsSource],
) -> Option<String> {
    if sources.is_empty() {
        return None;
    }
    let mut blocks: Vec<String> = Vec::new();
    for source in sources {
        // v4 re-trims here: the render half is exported and callable with
        // untrimmed sources, so the empty guard fires at BOTH layers.
        let instructions = js_trim(&source.instructions);
        if instructions.is_empty() {
            continue;
        }
        let heading = match source.kind {
            StandingInstructionsKind::Project => {
                format!("## Project Instructions — {}", source.name)
            }
            StandingInstructionsKind::Group => format!("## Group Instructions — {}", source.name),
        };
        blocks.push(format!("{heading}\n{instructions}"));
    }
    if blocks.is_empty() {
        return None;
    }
    Some(format!(
        "{STANDING_INSTRUCTIONS_PREAMBLE}\n\n{}",
        blocks.join("\n\n")
    ))
}

/// Convenience wrapper: resolve + render in one call — v4
/// `resolveStandingInstructionsSection`. Used by the call sites
/// (`build_context`, Carina, `self_inventory`) that hand the finished string to
/// the synchronous [`crate::system_prompt::build_system_prompt`].
pub fn resolve_standing_instructions_section(
    db: &Db,
    project_id: Option<&str>,
    character_id: Option<&str>,
) -> Option<String> {
    render_standing_instructions_section(&resolve_standing_instructions(
        db,
        project_id,
        character_id,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn src(
        kind: StandingInstructionsKind,
        name: &str,
        instructions: &str,
    ) -> StandingInstructionsSource {
        StandingInstructionsSource {
            kind,
            name: name.to_string(),
            instructions: instructions.to_string(),
        }
    }

    #[test]
    fn empty_sources_render_nothing() {
        assert_eq!(render_standing_instructions_section(&[]), None);
    }

    #[test]
    fn all_whitespace_sources_render_nothing() {
        let sources = vec![src(StandingInstructionsKind::Group, "Gamma", "   \n\t ")];
        assert_eq!(render_standing_instructions_section(&sources), None);
    }

    #[test]
    fn project_then_group_headings_and_joins() {
        let sources = vec![
            src(StandingInstructionsKind::Project, "Iota", "Be thorough."),
            src(StandingInstructionsKind::Group, "Gamma", "Keep it brief."),
        ];
        let got = render_standing_instructions_section(&sources).unwrap();
        assert_eq!(
            got,
            "[STANDING INSTRUCTIONS]\nThe sections below are standing instructions attached to this chat's project and to groups you belong to. They hold for the entire conversation. They refine how you conduct yourself here; they never replace who you are.\n\n## Project Instructions — Iota\nBe thorough.\n\n## Group Instructions — Gamma\nKeep it brief."
        );
    }
}
