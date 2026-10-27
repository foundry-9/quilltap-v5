//! The title lint at the document-write layer (v4
//! `lib/inventory/title-lint-documents.ts`, `7263105c6` at `322a5488e`;
//! P4.D340).
//!
//! The household's hands are a rename path: a title changed by
//! `doc_write_file` or `doc_str_replace` on an `Inventory/*.json` file, a file
//! deleted or moved with the doc tools, an edit saved in the Scriptorium —
//! none of these call an Inventory mutation. The database store
//! (`db::database_store`) therefore asks [`lint_inventory_document_change`]
//! before every write, delete and move whose path `is_title_lint_path`
//! claims, and calls [`PendingTitleLintRows::write`] once the change has
//! landed, to write the rows.
//!
//! For an `Inventory/` object file it reads the file's prior state first — the
//! lint cannot judge a rename it never saw the before of — parses both sides
//! with the Inventory's own reader, and hands them to `lint_inventory_title`,
//! keyed by the object's id: a file rewritten with a different object inside
//! is that object leaving and another entering. For a `Tools/*.tool.json`
//! file it records the census line and nothing else.
//!
//! v4's two "could not read" refusals here sit behind FALLBACK reads (v4
//! `withRawDb`): a failed read logs the repository's ERROR and answers
//! nothing, so through v4's real code they are unreachable (P4.D340 R-7 —
//! v4's own test reaches them only by mocking the repository to reject).
//! `its prior state could not be read` (with v4's `labelOf`) is therefore NOT
//! ported: the prior read is v5's fallback home too, and a failed one judges
//! the change with no prior state, as v4's real code does. `its contents
//! could not be listed` IS ported, for the strict-repository scope, where
//! v5's listing propagates.
//!
//! Writes from the Inventory mutations carry their own lint and are skipped
//! (`TitleLintScope::Linted`). Only database-backed stores hold an Inventory,
//! so a filesystem write never reaches here.

use rusqlite::Connection;
use serde_json::Value;

use super::errors::{InventoryRefusal, InventoryTitleLintUnread};
use super::folders::is_inventory_item_document_path;
use super::lint_scope::{is_title_lint_folder, is_tool_definition_path, TitleLintScope};
use super::location::{general_inventory_origin, inventory_location_for_mount, InventoryLocation};
use super::title_lint::{
    lint_inventory_title, write_title_lint_rows, TitleLintDoor, TitleLintOptions, TitleLintRequest,
    TitleLintVerdict,
};
use super::types::{InventoryFields, InventoryOrigin, InventoryRecord, InventoryScope};
use crate::db::doc_mount_documents::DocMountDocumentsRepository;
use crate::db::doc_mount_points::DocMountPointsRepository;
use crate::db::fallback;
use crate::db::inventory_readers::{parse_inventory_item_file, InventoryDoc};
use crate::db::item_folder_ops::{VaultFolderMount, VaultFolderMountScope};
use crate::jsstr::utf16_len;
use crate::pascal::tool_gate_keys::collect_tool_gate_keys;

/// v4 `InventoryDocumentChange`.
#[derive(Debug, Clone, Copy)]
pub enum InventoryDocumentChange<'a> {
    Write {
        mount_point_id: &'a str,
        relative_path: &'a str,
        content: &'a str,
    },
    Delete {
        mount_point_id: &'a str,
        relative_path: &'a str,
    },
    Move {
        mount_point_id: &'a str,
        from_path: &'a str,
        to_path: &'a str,
    },
    MoveFolder {
        mount_point_id: &'a str,
        from_path: &'a str,
        to_path: &'a str,
    },
}

impl<'a> InventoryDocumentChange<'a> {
    pub fn kind(&self) -> &'static str {
        match self {
            InventoryDocumentChange::Write { .. } => "write",
            InventoryDocumentChange::Delete { .. } => "delete",
            InventoryDocumentChange::Move { .. } => "move",
            InventoryDocumentChange::MoveFolder { .. } => "move-folder",
        }
    }

    pub fn mount_point_id(&self) -> &'a str {
        match *self {
            InventoryDocumentChange::Write { mount_point_id, .. }
            | InventoryDocumentChange::Delete { mount_point_id, .. }
            | InventoryDocumentChange::Move { mount_point_id, .. }
            | InventoryDocumentChange::MoveFolder { mount_point_id, .. } => mount_point_id,
        }
    }

    /// v4's `paths`: write / delete → `[relativePath]`, else `[fromPath, toPath]`.
    pub fn paths(&self) -> Vec<&'a str> {
        match *self {
            InventoryDocumentChange::Write { relative_path, .. }
            | InventoryDocumentChange::Delete { relative_path, .. } => vec![relative_path],
            InventoryDocumentChange::Move {
                from_path, to_path, ..
            }
            | InventoryDocumentChange::MoveFolder {
                from_path, to_path, ..
            } => vec![from_path, to_path],
        }
    }
}

/// The rows a linted change writes once it has landed (v4's returned
/// function; [`PendingTitleLintRows::nothing`] is `NOTHING_TO_WRITE`).
#[derive(Debug, Default)]
#[must_use = "call write() once the change has landed"]
pub struct PendingTitleLintRows {
    verdicts: Vec<(TitleLintVerdict, String)>,
    scope: Option<(InventoryScope, String)>,
}

impl PendingTitleLintRows {
    /// v4 `NOTHING_TO_WRITE`.
    pub fn nothing() -> Self {
        Self::default()
    }

    /// Write the rows (v4 calls the returned function after the change).
    pub fn write(self) {
        let Some((scope, mount_point_id)) = self.scope else {
            return;
        };
        for (verdict, item_id) in &self.verdicts {
            write_title_lint_rows(verdict, item_id, scope, &mount_point_id);
        }
    }
}

/// The location an `Inventory/` folder in this mount belongs to (v4
/// `locationForMount`), in v4's order: a character's vault, Quilltap General,
/// else any existing store as a PROJECT-scoped location ("A project or group
/// store, or any store linked to a project: its pool is its own folder plus
/// General").
fn location_for_mount(
    main: &Connection,
    mount: &Connection,
    mount_point_id: &str,
) -> Option<InventoryLocation> {
    if let Some(character) =
        crate::db::characters_read::find_by_document_mount_point_id_raw(main, mount_point_id)
    {
        let id = character
            .get("id")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string();
        let name = character
            .get("name")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string();
        return Some(inventory_location_for_mount(
            VaultFolderMount {
                mount_point_id: mount_point_id.to_string(),
                scope: VaultFolderMountScope::Character,
                character_id: Some(id.clone()),
            },
            Some(&id),
            InventoryOrigin {
                scope: InventoryScope::Character,
                id: Some(id.clone()),
                name,
            },
        ));
    }
    let general = crate::db::instance_settings::get_general_mount_point_id(main)
        .ok()
        .flatten();
    if general.as_deref() == Some(mount_point_id) {
        return Some(inventory_location_for_mount(
            VaultFolderMount {
                mount_point_id: mount_point_id.to_string(),
                scope: VaultFolderMountScope::General,
                character_id: None,
            },
            None,
            general_inventory_origin(),
        ));
    }
    let (_, name) = fallback::find_by_id_or_none("doc_mount_points", mount_point_id, || {
        DocMountPointsRepository::new(mount).find_id_and_name_by_id(mount_point_id)
    })?;
    Some(inventory_location_for_mount(
        VaultFolderMount {
            mount_point_id: mount_point_id.to_string(),
            scope: VaultFolderMountScope::Project,
            character_id: None,
        },
        None,
        InventoryOrigin {
            scope: InventoryScope::Project,
            id: None,
            name,
        },
    ))
}

/// `content === null || !isInventoryItemDocumentPath(rel) ? null :
/// parseInventoryItemFile({relativePath, content}, {quiet: true})`.
fn parse(relative_path: &str, content: Option<&str>) -> Option<InventoryRecord> {
    let content = content?;
    if !is_inventory_item_document_path(relative_path) {
        return None;
    }
    parse_inventory_item_file(
        &InventoryDoc {
            relative_path,
            content,
            mount_point_id: None,
            created_at: None,
            updated_at: None,
        },
        true,
    )
}

/// Split a file change into per-object pairs: a different object in the same
/// file is a leave and an enter.
fn pairs_of(
    before: Option<InventoryRecord>,
    after: Option<InventoryRecord>,
) -> Vec<(Option<InventoryRecord>, Option<InventoryRecord>)> {
    match (before, after) {
        (Some(b), Some(a)) if b.id() != a.id() => vec![(Some(b), None), (None, Some(a))],
        (b, a) => vec![(b, a)],
    }
}

fn json_text(v: &impl serde::Serialize) -> String {
    serde_json::to_string(v).unwrap_or_default()
}

/// Each file the change touches: its path before, its path after, and the
/// content it carries.
struct FileChange {
    from: Option<String>,
    to: Option<String>,
    content: Option<String>,
    door: TitleLintDoor,
}

/// Lint one document change before it is made (v4
/// `lintInventoryDocumentChange`). Refuses to stop it; otherwise answers the
/// rows to write once the change has landed.
pub fn lint_inventory_document_change(
    mount: &Connection,
    change: &InventoryDocumentChange<'_>,
    linted: &TitleLintScope<'_>,
) -> Result<PendingTitleLintRows, InventoryRefusal> {
    let main = match linted {
        TitleLintScope::Linted => return Ok(PendingTitleLintRows::nothing()),
        TitleLintScope::Unlinted { main } => *main,
        TitleLintScope::NeverClaimed => {
            debug_assert!(
                false,
                "a NeverClaimed writer reached the title lint with {:?}",
                change.paths()
            );
            return Err(never_claimed_refusal(change));
        }
    };
    let mount_point_id = change.mount_point_id();
    let paths = change.paths();
    let kind = change.kind();
    let paths_json = json_text(&paths);

    if !matches!(change, InventoryDocumentChange::MoveFolder { .. })
        && paths.iter().any(|p| is_tool_definition_path(p))
        && !paths.iter().any(|p| is_inventory_item_document_path(p))
    {
        // A gate rewrite changes no title; the census line is the evidence the
        // lint looked. (`door` is `document-write` whatever the change — v4's.)
        let census = collect_tool_gate_keys(mount);
        tracing::debug!(
            kind = kind,
            mountPointId = %mount_point_id,
            pathsJson = %paths_json,
            context = "inventory",
            door = "document-write",
            entering = false,
            leaving = false,
            toolStoresJson = %json_text(&census.stores_consulted),
            gateKeys = census.keys.len(),
            unreadJson = %json_text(&census.unread),
            "[InventoryTitleLint] Census"
        );
        return Ok(PendingTitleLintRows::nothing());
    }

    let Some(location) = location_for_mount(main, mount, mount_point_id) else {
        return Err(InventoryRefusal::title_check_unavailable(
            paths[0],
            &[InventoryTitleLintUnread {
                pool: format!("store {mount_point_id}"),
                reason: "no such store; an empty read from it is not an answer".to_string(),
            }],
        ));
    };

    let docs = DocMountDocumentsRepository::new(mount);
    let mut files: Vec<FileChange> = Vec::new();
    match *change {
        InventoryDocumentChange::Write {
            relative_path,
            content,
            ..
        } => files.push(FileChange {
            from: Some(relative_path.to_string()),
            to: Some(relative_path.to_string()),
            content: Some(content.to_string()),
            door: TitleLintDoor::DocumentWrite,
        }),
        InventoryDocumentChange::Delete { relative_path, .. } => files.push(FileChange {
            from: Some(relative_path.to_string()),
            to: None,
            content: None,
            door: TitleLintDoor::DocumentDelete,
        }),
        InventoryDocumentChange::Move {
            from_path, to_path, ..
        } => files.push(FileChange {
            from: Some(from_path.to_string()),
            to: Some(to_path.to_string()),
            content: None,
            door: TitleLintDoor::DocumentMove,
        }),
        InventoryDocumentChange::MoveFolder {
            from_path, to_path, ..
        } => {
            // A folder move matters only when it carries objects across the
            // `Inventory/` boundary.
            if is_title_lint_folder(from_path) == is_title_lint_folder(to_path) {
                return Ok(PendingTitleLintRows::nothing());
            }
            let listed = docs.find_many_by_mount_points_in_folder_opts_or_empty(
                &[mount_point_id.to_string()],
                from_path,
                ".json",
                true,
            );
            let listed = match listed {
                Ok(listed) => listed,
                Err(error) => {
                    return Err(InventoryRefusal::title_check_unavailable(
                        from_path,
                        &[InventoryTitleLintUnread {
                            pool: format!("the folder {from_path}"),
                            reason: format!(
                                "its contents could not be listed: {}",
                                fallback::error_text(&error)
                            ),
                        }],
                    ))
                }
            };
            // `relativePath.slice(fromPath.length)` — UTF-16 code units.
            let cut = utf16_len(from_path);
            for doc in listed {
                let units: Vec<u16> = doc.relative_path.encode_utf16().collect();
                let tail = String::from_utf16_lossy(units.get(cut..).unwrap_or(&[]));
                files.push(FileChange {
                    from: Some(doc.relative_path.clone()),
                    to: Some(format!("{to_path}{tail}")),
                    content: None,
                    door: TitleLintDoor::DocumentMove,
                });
            }
        }
    }

    let mut verdicts: Vec<(TitleLintVerdict, String)> = Vec::new();
    for file in &files {
        // v4's `readPrior` refuses on a THROWN read; its read is a fallback
        // (R-7), so a failed read logs and answers no prior state.
        let prior = file
            .from
            .as_deref()
            .and_then(|from| docs.find_by_mount_point_and_path_or_none(mount_point_id, from));
        let next = match &file.to {
            None => None,
            Some(_) => file.content.clone().or_else(|| prior.clone()),
        };
        let before = file
            .from
            .as_deref()
            .and_then(|f| parse(f, prior.as_deref()));
        let after = file.to.as_deref().and_then(|t| parse(t, next.as_deref()));
        for (b, a) in pairs_of(before, after) {
            let verdict = lint_inventory_title(
                main,
                mount,
                &location,
                TitleLintRequest {
                    door: file.door,
                    before: b.as_ref(),
                    after: a.as_ref(),
                },
                TitleLintOptions::default(),
            )?;
            let item_id = a.as_ref().or(b.as_ref()).map(|r| r.id().to_string());
            if let Some(item_id) = item_id.filter(|id| !id.is_empty()) {
                if !verdict.rows.is_empty() {
                    verdicts.push((verdict, item_id));
                }
            }
        }
    }

    tracing::debug!(
        kind = kind,
        mountPointId = %mount_point_id,
        pathsJson = %paths_json,
        context = "inventory",
        files = files.len(),
        rows = verdicts.len(),
        "[InventoryTitleLint] Document change linted"
    );
    Ok(PendingTitleLintRows {
        verdicts,
        scope: Some((location.scope, location.mount_point_id.clone())),
    })
}

/// R-2's release-mode answer for a claimed path under `NeverClaimed` — a
/// v5-only text on an arm the census test proves unreachable.
fn never_claimed_refusal(change: &InventoryDocumentChange<'_>) -> InventoryRefusal {
    InventoryRefusal::title_check_unavailable(
        change.paths()[0],
        &[InventoryTitleLintUnread {
            pool: format!("store {}", change.mount_point_id()),
            reason: "this writer carries no main connection for the title check".to_string(),
        }],
    )
}
