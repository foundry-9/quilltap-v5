//! Character vault provisioning — the store-backed capstone's stateful glue
//! (characters sub-unit 3). Ports v4's `scaffoldCharacterMount`
//! (`lib/mount-index/character-scaffold.ts`) now; `ensureCharacterVault`
//! (`lib/mount-index/character-vault.ts`) + the `CharactersRepository.create`
//! integration land next (sub-unit 3b).
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

use super::doc_mount_file_links::DocMountFileLinksRepository;
use super::DbError;

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
