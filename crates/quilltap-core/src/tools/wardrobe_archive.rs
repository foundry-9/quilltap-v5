//! `wardrobe_archive` handler (v4 `lib/tools/handlers/wardrobe-archive-handler.ts`
//! `executeWardrobeArchiveTool` + `formatWardrobeArchiveResults`).
//!
//! Soft-retires the character's OWN item (sets `archivedAt` via the public update
//! path). Shared archetypes are read-only. When the archived item was equipped it
//! records a pending wardrobe announcement (returned to the executor to fold into
//! the per-turn set) and would trigger avatar generation — the latter an image
//! subsystem seam, out of scope (see the module note in [`super::wardrobe_create`]).

use rusqlite::Connection;
use serde::Serialize;
use serde_json::Value;

use crate::clock;
use crate::db::doc_mount_documents::DocMountDocumentsRepository;
use crate::db::doc_mount_file_links::DocMountFileLinksRepository;
use crate::db::vault_wardrobe_public::{update_vault_wardrobe_item, WardrobePatch};
use crate::wardrobe::{normalize_no_item_sentinel, WARDROBE_SLOT_TYPES};

use super::wardrobe_read::{not_found_message, validate_locate};
use super::wardrobe_shared::{is_own_wardrobe_item, resolve_wardrobe_item_across_tiers};
use crate::wardrobe_tiers::resolve_shared_wardrobe_tiers_for_chat;

/// v4 `WardrobeArchiveToolOutput`.
#[derive(Debug, Serialize)]
pub struct WardrobeArchiveToolOutput {
    pub success: bool,
    pub item_id: String,
    pub title: String,
    /// Always `"archived"`.
    pub action: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

fn failure(error: impl Into<String>) -> WardrobeArchiveToolOutput {
    WardrobeArchiveToolOutput {
        success: false,
        item_id: String::new(),
        title: String::new(),
        action: "archived".to_string(),
        error: Some(error.into()),
    }
}

/// Execute `wardrobe_archive` (v4 `executeWardrobeArchiveTool`). Returns the output
/// plus the character ids to announce (v4's `recordPendingWardrobeAnnouncement` —
/// non-empty only when the archived item was equipped).
pub fn execute(
    main: &Connection,
    mount: &Connection,
    user_id: &str,
    chat_id: &str,
    character_id: &str,
    args: &Value,
) -> (WardrobeArchiveToolOutput, Vec<String>) {
    let _ = user_id;
    let Some(input) = validate_locate(args) else {
        return (
            failure("Invalid input: item_id or item_title is required."),
            Vec::new(),
        );
    };
    match run(main, mount, chat_id, character_id, &input) {
        Ok(pair) => pair,
        Err(e) => (failure(e.to_string()), Vec::new()),
    }
}

fn run(
    main: &Connection,
    mount: &Connection,
    chat_id: &str,
    character_id: &str,
    input: &super::wardrobe_read::LocateInput,
) -> Result<(WardrobeArchiveToolOutput, Vec<String>), crate::db::DbError> {
    let docs = DocMountDocumentsRepository::new(mount);
    let links = DocMountFileLinksRepository::new(mount);
    let tiers = resolve_shared_wardrobe_tiers_for_chat(main, mount, chat_id, character_id);

    let item = resolve_wardrobe_item_across_tiers(
        main,
        &docs,
        character_id,
        normalize_no_item_sentinel(input.item_id.as_deref()).as_deref(),
        normalize_no_item_sentinel(input.item_title.as_deref()).as_deref(),
        &tiers,
    )?;
    let Some(item) = item else {
        return Ok((
            failure(not_found_message(&input.item_id, &input.item_title)),
            Vec::new(),
        ));
    };

    let title = item
        .get("title")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();

    if !is_own_wardrobe_item(&item, character_id) {
        return Ok((
            failure(format!(
                "\"{title}\" is a shared wardrobe item — you can wear it but not edit or retire it. \
Only items in your own wardrobe can be archived."
            )),
            Vec::new(),
        ));
    }

    let id = item
        .get("id")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();

    // Was the item equipped? (Archive doesn't clear slots, but an equipped-then-
    // archived item warrants a visible refresh.)
    let equipped = crate::db::chats_outfits::ChatOutfitsRepository::new(main)
        .get_equipped_outfit_for_character(chat_id, character_id)?;
    let was_equipped = equipped.as_ref().is_some_and(|eq| {
        WARDROBE_SLOT_TYPES.iter().any(|s| {
            eq.get(*s)
                .and_then(Value::as_array)
                .is_some_and(|arr| arr.iter().any(|v| v.as_str() == Some(id.as_str())))
        })
    });

    // v4 `repos.wardrobe.archive(id, item.characterId)` = update {archivedAt: now}.
    let patch = WardrobePatch {
        archived_at: Some(Some(clock::now_iso())),
        ..Default::default()
    };
    let updated =
        match update_vault_wardrobe_item(main, &links, &docs, &id, &patch, Some(character_id)) {
            Ok(u) => u,
            // v4's outer try/catch maps a thrown NoMount to a failure result.
            Err(e) => {
                return Ok((
                    failure(super::wardrobe_shared::public_error_message(e, "update")),
                    Vec::new(),
                ))
            }
        };
    if updated.is_none() {
        return Ok((
            failure(format!("Failed to archive wardrobe item \"{title}\"")),
            Vec::new(),
        ));
    }

    // Equipped-then-archived: avatar generation (image subsystem seam, out of
    // scope) + a pending wardrobe announcement (the character id, folded by the
    // executor into the per-turn set).
    let announce = if was_equipped {
        vec![character_id.to_string()]
    } else {
        Vec::new()
    };

    Ok((
        WardrobeArchiveToolOutput {
            success: true,
            item_id: id,
            title,
            action: "archived".to_string(),
            error: None,
        },
        announce,
    ))
}

/// v4 `formatWardrobeArchiveResults`.
pub fn format(output: &WardrobeArchiveToolOutput) -> String {
    if !output.success {
        return format!(
            "Wardrobe Error: {}",
            output.error.as_deref().unwrap_or("Unknown error")
        );
    }
    format!(
        "Archived \"{}\" ({}). It's hidden from listings and can't be worn; a human can restore it.",
        output.title, output.item_id
    )
}
