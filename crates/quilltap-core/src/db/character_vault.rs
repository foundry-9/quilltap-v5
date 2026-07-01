//! Character vault provisioning — the store-backed capstone's stateful glue
//! (characters sub-unit 3). Ports v4's `scaffoldCharacterMount`
//! (`lib/mount-index/character-scaffold.ts`), `ensureCharacterVault`
//! (`lib/mount-index/character-vault.ts` — including the ADOPT branch), and the
//! `CharactersRepository.create` integration.
//!
//! ## `scaffold_character_mount`
//!
//! Populates a freshly-created database-backed character store with the
//! conventional preset structure: **seven** empty top-level folders, **six** blank
//! Markdown files, and **two** seeded JSON files. Idempotent — a path that already
//! has a link is left untouched, so it is safe to re-run when a store's `storeType`
//! is flipped to `'character'` on an already-populated store.
//!
//! All writes go through the verified storage primitive
//! ([`super::doc_mount_file_links`]): folders via
//! [`DocMountFileLinksRepository::ensure_folder_path`], files via
//! `write_database_document` (which dedups by content sha — the six blank `.md`
//! files share one `doc_mount_files`/`doc_mount_documents` row but get six distinct
//! links). The two seeded JSON files carry FIXED default content (matching v4's
//! `JSON.stringify(…, null, 2)` byte-for-byte — see the unit tests); the bytes feed
//! the dedup sha so they must be exact.
//!
//! In the full `create` flow, `writeCharacterVaultManagedFields` (sub-unit 1) runs
//! AFTER the scaffold and OVERWRITES the five identity markdown files +
//! `properties.json` (and, when a physical description exists,
//! `physical-description.md` + `physical-prompts.json`). So the scaffold's default
//! `properties.json` / `physical-prompts.json` content only survives on the no-physical
//! / pre-managed-write path — which is exactly why scaffold is verified on its own
//! here (the create differential would mask the defaults).
//!
//! Non-database / non-character mount points are a no-op (the guard), mirroring v4.

use rusqlite::{params, Connection};

use super::characters::CreateOptions as SlimCreateOptions;
use super::characters::{CharacterCreate, CharacterUpdate, CharactersRepository};
use super::doc_mount_documents::DocMountDocumentsRepository;
use super::doc_mount_file_links::DocMountFileLinksRepository;
use super::doc_mount_points::{
    CreateOptions as DmpCreateOptions, DmpCreate, DocMountPointsRepository,
};
use super::vault_character_write::{
    write_character_vault_managed_fields, CharacterVaultWriteInput,
};
use super::DbError;

/// The six files a successfully-populated vault always carries — v4
/// `REQUIRED_VAULT_FILES`. `writeCharacterVaultManagedFields` writes each
/// unconditionally (empty string when blank), so their presence is the "safe to
/// adopt" test. The physical-* pair is intentionally ABSENT (the writer skips it
/// when there is no physical description, so requiring it would wrongly reject
/// sparse characters). Compared lowercased.
const REQUIRED_VAULT_FILES: &[&str] = &[
    "properties.json",
    "identity.md",
    "description.md",
    "manifesto.md",
    "personality.md",
    "example-dialogues.md",
];

/// The six blank Markdown files the scaffold seeds (content `""`), in v4's order.
/// All share the empty-string content sha, so they dedup to ONE
/// `doc_mount_files` / `doc_mount_documents` row with six distinct links.
const BLANK_MARKDOWN_FILES: &[&str] = &[
    "identity.md",
    "description.md",
    "manifesto.md",
    "personality.md",
    "physical-description.md",
    "example-dialogues.md",
];

/// The seven empty top-level folders the scaffold creates, in v4's order. Most
/// hold no files at scaffold time (the markdown/JSON files are all at the mount
/// root), so they exist purely as structure.
const TOP_LEVEL_FOLDERS: &[&str] = &[
    "Prompts",
    "Scenarios",
    "Wardrobe",
    "Outfits",
    "lore",
    "images",
    "files",
];

/// `JSON.stringify(PROPERTIES_JSON, null, 2)` — the default `properties.json`
/// content (pronouns/aliases/title/firstMessage/talkativeness). FIXED bytes (they
/// feed the dedup sha); verified against the JS output in the unit tests.
const PROPERTIES_JSON_DEFAULT: &str = "{\n  \"pronouns\": null,\n  \"aliases\": [],\n  \"title\": \"\",\n  \"firstMessage\": \"\",\n  \"talkativeness\": 0.5\n}";

/// `JSON.stringify(PHYSICAL_PROMPTS_JSON, null, 2)` — the default
/// `physical-prompts.json` content. Note this is the **four-key** scaffold default
/// (`short`/`medium`/`long`/`complete`); the managed-fields writer's
/// `renderPhysicalPromptsJson` emits a **five-key** variant (adds
/// `headAndShoulders`) and only when a physical description exists.
const PHYSICAL_PROMPTS_JSON_DEFAULT: &str =
    "{\n  \"short\": null,\n  \"medium\": null,\n  \"long\": null,\n  \"complete\": null\n}";

/// Scaffold the preset structure for a database-backed character store (v4
/// `scaffoldCharacterMount`). No-op when the mount point is not found, not
/// database-backed, or not a character store. Existing files (by path) are never
/// overwritten. Operates on the mount-index `conn`.
pub fn scaffold_character_mount(conn: &Connection, mount_point_id: &str) -> Result<(), DbError> {
    // Guard: only database-backed character stores are scaffolded (v4 reads the
    // mount point and returns early otherwise).
    let kind: Option<(String, String)> = conn
        .query_row(
            "SELECT mountType, storeType FROM doc_mount_points WHERE id = ?1",
            params![mount_point_id],
            |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)),
        )
        .map(Some)
        .or_else(|e| match e {
            rusqlite::Error::QueryReturnedNoRows => Ok(None),
            other => Err(other),
        })?;
    match kind {
        Some((mount_type, store_type)) if mount_type == "database" && store_type == "character" => {
        }
        _ => return Ok(()),
    }

    let links = DocMountFileLinksRepository::new(conn);

    // The seven top-level folders (idempotent find-or-create).
    for folder in TOP_LEVEL_FOLDERS {
        links.ensure_folder_path(mount_point_id, folder)?;
    }

    // The eight seeded files: six blank markdown + two default JSON. Skip any path
    // that already has a link (the idempotent re-scaffold case).
    let mut specs: Vec<(&str, &str)> = BLANK_MARKDOWN_FILES.iter().map(|p| (*p, "")).collect();
    specs.push(("properties.json", PROPERTIES_JSON_DEFAULT));
    specs.push(("physical-prompts.json", PHYSICAL_PROMPTS_JSON_DEFAULT));

    for (rel_path, content) in specs {
        if links.link_exists_at_path(mount_point_id, rel_path)? {
            continue;
        }
        links.write_database_document(mount_point_id, rel_path, content)?;
    }

    Ok(())
}

/// What [`ensure_character_vault`] resolved/created.
pub struct EnsureResult {
    pub mount_point_id: String,
    /// True if this call created the vault; false if the character already had one.
    pub created: bool,
}

/// Ensure the given character has a linked database-backed character vault — v4
/// `ensureCharacterVault`. Idempotent: when `current_fk` is already set, returns
/// it unchanged.
///
/// When there is no link, v4 first looks for an existing populated same-name
/// `'character'` store and ADOPTS it (the startup-heal path for a vault whose
/// link write was lost mid cloud-materialization) — critical post-cutover, where
/// the legacy content columns are gone, so creating a fresh vault here would
/// populate it with empty files and orphan the good one. Exactly one populated
/// candidate → adopt; multiple → ambiguous, don't guess, fall through to create
/// fresh (v4 logs loudly); zero → create fresh.
///
/// The create path mints a `<name> Character Vault` mount point (MOUNT-INDEX db),
/// scaffolds it, projects the managed fields, then links it by setting
/// `characterDocumentMountPointId` on the slim row (MAIN db), confirming the write
/// stuck.
///
/// Spans both databases: the mount point + store live in `mount`, the slim row +
/// FK in `main`.
pub fn ensure_character_vault(
    main: &Connection,
    mount: &Connection,
    character_id: &str,
    character_name: &str,
    vault: &CharacterVaultWriteInput,
    current_fk: Option<&str>,
) -> Result<EnsureResult, DbError> {
    if let Some(fk) = current_fk {
        return Ok(EnsureResult {
            mount_point_id: fk.to_string(),
            created: false,
        });
    }

    let vault_name = format!("{character_name} Character Vault");

    // Adopt an existing populated same-name character vault before creating one.
    // v4 filters `findByName` matches to `storeType === 'character'`, keeps those
    // that hold every REQUIRED_VAULT_FILES entry, and adopts iff exactly one
    // qualifies.
    let same_name = DocMountPointsRepository::new(mount).find_by_name(&vault_name)?;
    let mut populated: Vec<String> = Vec::new();
    for (id, store_type) in same_name {
        if store_type == "character" && vault_has_required_files(mount, &id)? {
            populated.push(id);
        }
    }
    if populated.len() == 1 {
        let adopted = populated.into_iter().next().unwrap();
        link_character_to_vault(main, character_id, &adopted)?;
        return Ok(EnsureResult {
            mount_point_id: adopted,
            created: false,
        });
    }
    // populated.len() > 1: ambiguous — don't guess which holds the real content.
    // Fall through to create a fresh vault (v4 logs an error and leaves the
    // existing ones for an operator to reconcile).

    // 1. Provision a fresh character-vault mount point (minted id + now).
    let now = crate::clock::now_iso();
    let mount_point_id = uuid::Uuid::new_v4().to_string();
    DocMountPointsRepository::new(mount).create(
        &DmpCreate {
            name: format!("{character_name} Character Vault"),
            base_path: String::new(),
            mount_type: "database".into(),
            store_type: "character".into(),
            include_patterns: vec![
                "*.md".into(),
                "*.txt".into(),
                "*.pdf".into(),
                "*.docx".into(),
            ],
            exclude_patterns: vec![
                ".git".into(),
                "node_modules".into(),
                ".obsidian".into(),
                ".trash".into(),
            ],
            enabled: true,
            last_scanned_at: None,
            scan_status: "idle".into(),
            last_scan_error: None,
            conversion_status: "idle".into(),
            conversion_error: None,
            file_count: 0.0,
            chunk_count: 0.0,
            total_size_bytes: 0.0,
        },
        &DmpCreateOptions {
            id: mount_point_id.clone(),
            created_at: now.clone(),
            updated_at: now,
        },
    )?;

    // 2. Scaffold the preset structure, then project the managed fields onto it.
    scaffold_character_mount(mount, &mount_point_id)?;
    let links = DocMountFileLinksRepository::new(mount);
    let docs = DocMountDocumentsRepository::new(mount);
    write_character_vault_managed_fields(&links, &docs, &mount_point_id, vault)?;

    // 3. Link: set the FK on the slim row, then CONFIRM it stuck.
    link_character_to_vault(main, character_id, &mount_point_id)?;

    Ok(EnsureResult {
        mount_point_id,
        created: true,
    })
}

/// True iff `mount_point_id` holds every [`REQUIRED_VAULT_FILES`] entry — v4
/// `vaultHasRequiredFiles`. The check reads the links' lowercased `relativePath`
/// set and asserts all six required files are present (deciding whether an
/// existing same-name vault is safe to adopt).
fn vault_has_required_files(mount: &Connection, mount_point_id: &str) -> Result<bool, DbError> {
    let present = DocMountFileLinksRepository::new(mount).relative_paths_lower(mount_point_id)?;
    Ok(REQUIRED_VAULT_FILES
        .iter()
        .all(|f| present.iter().any(|p| p == f)))
}

/// Set `characterDocumentMountPointId` on the slim row and CONFIRM it stuck by
/// re-reading — v4 `linkCharacterToVault`. The `characters.update` write can
/// return without persisting when the main DB is unwritable at that instant (most
/// notably while a cloud-synced file is still materializing). Verifying here turns
/// a silent "linked but not linked" state into a loud throw the caller can block
/// on, rather than destroying columns out from under good vault content.
fn link_character_to_vault(
    main: &Connection,
    character_id: &str,
    mount_point_id: &str,
) -> Result<(), DbError> {
    CharactersRepository::new(main).update(
        character_id,
        &CharacterUpdate {
            character_document_mount_point_id: Some(mount_point_id.to_string()),
            updated_at: crate::clock::now_iso(),
            ..Default::default()
        },
    )?;
    let stuck: Option<String> = main
        .query_row(
            "SELECT characterDocumentMountPointId FROM characters WHERE id = ?1",
            params![character_id],
            |row| row.get::<_, Option<String>>(0),
        )
        .or_else(|e| match e {
            rusqlite::Error::QueryReturnedNoRows => Ok(None),
            other => Err(other),
        })?;
    if stuck.as_deref() != Some(mount_point_id) {
        return Err(DbError::Key(format!(
            "Failed to persist characterDocumentMountPointId for {character_id}: wrote \
             {mount_point_id} but re-read {}. The character row write did not stick.",
            stuck.as_deref().unwrap_or("null")
        )));
    }
    Ok(())
}

/// Create a character end-to-end — v4 `CharactersRepository.create`. Inserts the
/// slim row (FK nulled — a fresh character always provisions a fresh vault), then
/// [`ensure_character_vault`] scaffolds + projects + links. Returns the minted
/// character id. The closing overlay re-read (`findById`) is a READ concern (it
/// mutates no DB state) and is left to the read overlay; this returns the id so
/// callers can re-read.
///
/// Mints id + timestamps internally (v4's `_create` with no `CreateOptions`).
pub fn create_character(
    main: &Connection,
    mount: &Connection,
    slim: &CharacterCreate,
    vault: &CharacterVaultWriteInput,
) -> Result<String, DbError> {
    let now = crate::clock::now_iso();
    let id = uuid::Uuid::new_v4().to_string();

    // The slim row always lands with a NULL vault FK (create provisions fresh and
    // sets the FK in step 3); drop any incoming pointer.
    let mut slim_row = slim.clone();
    slim_row.character_document_mount_point_id = None;
    let name = slim_row.name.clone();

    CharactersRepository::new(main).create(
        &slim_row,
        &SlimCreateOptions {
            id: id.clone(),
            created_at: now.clone(),
            updated_at: now,
        },
    )?;

    ensure_character_vault(main, mount, &id, &name, vault, None)?;

    Ok(id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::Serialize;

    // Typed mirrors of v4's PROPERTIES_JSON / PHYSICAL_PROMPTS_JSON objects, in
    // schema field order. serde_json pretty-print (2-space indent) matches
    // `JSON.stringify(obj, null, 2)` for these shapes; a typed struct (NOT
    // serde_json::Value, which sorts keys) preserves the insertion order v4 emits.
    #[derive(Serialize)]
    struct PropsDefault {
        pronouns: Option<()>,
        aliases: Vec<String>,
        title: String,
        #[serde(rename = "firstMessage")]
        first_message: String,
        talkativeness: f64,
    }

    #[derive(Serialize)]
    struct PhysicalPromptsDefault {
        short: Option<()>,
        medium: Option<()>,
        long: Option<()>,
        complete: Option<()>,
    }

    #[test]
    fn properties_json_default_matches_js_stringify() {
        let pretty = serde_json::to_string_pretty(&PropsDefault {
            pronouns: None,
            aliases: vec![],
            title: String::new(),
            first_message: String::new(),
            talkativeness: 0.5,
        })
        .unwrap();
        assert_eq!(pretty, PROPERTIES_JSON_DEFAULT);
    }

    #[test]
    fn physical_prompts_json_default_matches_js_stringify() {
        let pretty = serde_json::to_string_pretty(&PhysicalPromptsDefault {
            short: None,
            medium: None,
            long: None,
            complete: None,
        })
        .unwrap();
        assert_eq!(pretty, PHYSICAL_PROMPTS_JSON_DEFAULT);
    }
}
