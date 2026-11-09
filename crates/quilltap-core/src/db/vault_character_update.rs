//! Character vault UPDATE integration (characters sub-unit 4a). Ports v4's
//! `applyDocumentStoreWriteOverlay` (`vault-overlay/managed-fields.ts`) + the
//! `CharactersRepository.update` orchestration.
//!
//! ## The write overlay vs. the create-time writer
//!
//! Sub-unit 1's [`super::vault_character_write::write_character_vault_managed_fields`]
//! projects **every** managed field unconditionally (the create path, onto a fresh
//! vault). The UPDATE path is different: it routes only the fields **present in the
//! patch**, and `properties.json` is a **read-modify-write** — a patch touching
//! only `title` must preserve the other four property keys. So this is a distinct
//! port, composing the same verified write leaves with per-field presence checks.
//!
//! ## `apply_document_store_write_overlay`
//!
//! Reads the character's vault FK, then for each managed descriptor present in the
//! patch writes the vault file and strips that key from the DB-bound patch:
//!
//!   - **markdown** (`identity`/`description`/`manifesto`/`personality`/
//!     `exampleDialogues`): `None`/absent value → `""`.
//!   - **properties** (`pronouns`/`aliases`/`title`/`firstMessage`/`talkativeness`/
//!     `canChooseOutfit`/`canDrawDiagrams`/`diagramStyle`/`kind`):
//!     read the current `properties.json` (parse; fall back to the empty-managed
//!     default when GENUINELY ABSENT), overlay only the touched keys, rewrite. The
//!     untouched keys are preserved — the RMW invariant. A `properties.json` that
//!     is PRESENT but unparseable REFUSES the patch (`OverlayError::Unavailable`)
//!     instead of seeding defaults — see [`read_current_properties`]. This was
//!     dogfood finding #47, a fix v5 made FIRST: v4's
//!     `readCharacterVaultProperties` used to return `null` on a parse failure and
//!     its `??`-seed then projected the empty defaults over all six property
//!     fields, permanently — the exact clobber v4's own `dcd9440a` hardened the
//!     group/project bags against but missed for the character vault (the third
//!     bag of the same shape, and post-cutover the ONLY home those six fields
//!     have). **v4 has since CONVERGED** (`13ddc5ee`, bug 8): its new
//!     `readCharacterVaultPropertiesForWrite` throws `CharacterVaultUnavailableError`
//!     ("… has no usable vault …: properties.json unparseable: …") on the same
//!     condition, so both sides now refuse and preserve the corrupt bytes. The
//!     characters-update corpus is a PLAIN equality on this arm (the refusals are
//!     byte-compared with the parse-detail tail elided).
//!   - **physical** (`physicalDescription`): a non-null value writes
//!     `physical-description.md` + `physical-prompts.json`; a **null** value leaves
//!     the vault files alone (clearing is a DB-side concern) — matching v4, which
//!     does NOT strip it from the patch in that case (the slim update ignores it
//!     anyway, since `physicalDescription` is not a slim column).
//!   - **prompts / scenarios** (`systemPrompts`/`scenarios`): reproject the
//!     `Prompts/` / `Scenarios/` folder from the incoming array (sweep + write).
//!
//! Returns the DB-bound remainder (the unmanaged patch keys). `update_character`
//! then runs the slim `_update` for that remainder (skipped when empty — so a
//! managed-only update does NOT bump the slim row's `updatedAt`), mirroring v4.
//!
//! **Provision-on-the-fly.** When a character has no vault FK but the patch
//! carries managed fields, v4 provisions a vault mid-write (a loud-logged bug path
//! — every character is supposed to have one), re-reads to pick up the new FK,
//! verifies it linked, then continues normal routing. This port does the same via
//! [`super::character_vault::ensure_character_vault`] over the raw slim character
//! (the pre-patch managed values, all at their post-cutover empty defaults — the
//! patch's managed fields are then routed on top).

use rusqlite::{params, Connection};
use serde_json::{Map, Value};

use super::characters::{CharacterUpdate, CharactersRepository, MANAGED_FIELDS};
use super::doc_mount_documents::DocMountDocumentsRepository;
use super::doc_mount_file_links::DocMountFileLinksRepository;
use super::document_store_overlay::OverlayError;
use super::vault_character_write::{render_physical_prompts_json, render_properties_json};
use super::DbError;
use crate::characters::diagrams::{diagram_style_or_default, DiagramStyle};
use crate::characters::kind::{kind_or_default, CharacterKind};
use crate::vault_overlay::{
    build_scenario_file, build_system_prompt_file, parse_vault_physical_prompts,
    parse_vault_properties, sanitize_file_name, CharacterVaultPhysicalPrompts,
    CharacterVaultProperties,
};

use super::vault_character_write::{
    CharacterVaultWriteInput, PhysicalDescriptionWrite, Pronouns, ScenarioWrite, SystemPromptWrite,
};
use super::vault_wardrobe_write::{project_array_into_vault_folder, ProjectArrayOptions};

const IDENTITY_MD_PATH: &str = "identity.md";
const DESCRIPTION_MD_PATH: &str = "description.md";
const MANIFESTO_MD_PATH: &str = "manifesto.md";
const PERSONALITY_MD_PATH: &str = "personality.md";
const EXAMPLE_DIALOGUES_MD_PATH: &str = "example-dialogues.md";
const PHYSICAL_DESCRIPTION_MD_PATH: &str = "physical-description.md";
const PHYSICAL_PROMPTS_JSON_PATH: &str = "physical-prompts.json";
const PROPERTIES_JSON_PATH: &str = "properties.json";
const METADATA_JSON_PATH: &str = "metadata.json";
const PROMPTS_FOLDER: &str = "Prompts";
const SCENARIOS_FOLDER: &str = "Scenarios";

/// The markdown managed fields and their vault paths, in descriptor order.
const MARKDOWN_FIELDS: &[(&str, &str)] = &[
    ("identity", IDENTITY_MD_PATH),
    ("description", DESCRIPTION_MD_PATH),
    ("manifesto", MANIFESTO_MD_PATH),
    ("personality", PERSONALITY_MD_PATH),
    ("exampleDialogues", EXAMPLE_DIALOGUES_MD_PATH),
];

/// The nine `properties.json` keys (in literal order — v4 `propsKeys`,
/// `managed-fields.ts:426-429`, `b09ca46bb`).
const PROPERTY_KEYS: &[&str] = &[
    "pronouns",
    "aliases",
    "title",
    "firstMessage",
    "talkativeness",
    "canChooseOutfit",
    "canDrawDiagrams",
    "diagramStyle",
    "kind",
];

/// Route the managed fields in `patch` to the character's vault and return the
/// DB-bound remainder (v4 `applyDocumentStoreWriteOverlay`). `main` holds the slim
/// row (the FK lookup); `mount` holds the store.
///
/// Errors: [`OverlayError::Db`] wraps every real DB failure;
/// [`OverlayError::Unavailable`] (entity label `character`) is the
/// present-but-unparseable `properties.json` write refusal — dogfood finding
/// #47, a fix v5 made first and v4 has since converged to (see the module doc).
pub fn apply_document_store_write_overlay(
    main: &Connection,
    mount: &Connection,
    character_id: &str,
    patch: &Map<String, Value>,
) -> Result<Map<String, Value>, OverlayError> {
    let mount_point_id = match read_vault_fk(main, character_id)? {
        Some(id) => id,
        None => {
            // No linked vault. With no managed fields there's nothing to route —
            // let the unmanaged remainder flow to the DB.
            if !MANAGED_FIELDS.iter().any(|f| patch.contains_key(*f)) {
                return Ok(patch.clone());
            }
            // The post-4.6 cutover dropped the DB columns for managed fields, so a
            // character without a vault would silently lose them on the way through
            // `_update`. Every character is supposed to have a vault — the startup
            // backfill provisions one for any that don't — so reaching this branch
            // is a bug elsewhere. Provision a vault now (v4 logs loudly) so the
            // write doesn't get dropped, re-read the FK it set, and continue.
            provision_vault_on_the_fly(main, mount, character_id)?
        }
    };

    let links = DocMountFileLinksRepository::new(mount);
    let docs = DocMountDocumentsRepository::new(mount);
    let mut db_patch = patch.clone();

    // 1. Markdown fields (None/absent value → "").
    for (key, path) in MARKDOWN_FIELDS {
        if let Some(value) = patch.get(*key) {
            links.write_database_document(&mount_point_id, path, value.as_str().unwrap_or(""))?;
            db_patch.remove(*key);
        }
    }

    // 2. properties.json — read-modify-write over only the touched keys.
    let touched: Vec<&str> = PROPERTY_KEYS
        .iter()
        .copied()
        .filter(|k| patch.contains_key(*k))
        .collect();
    if !touched.is_empty() {
        // Seed from the current properties.json; fall back to the empty-managed
        // default ONLY when the file is genuinely absent (the raw slim row
        // carries no managed values post-cutover). A present-but-unparseable
        // file refuses instead — `read_current_properties` errors rather than
        // returning `None`, so this fallback can never fire on a corrupt bag.
        let current = read_current_properties(mount, &mount_point_id, character_id)?
            .unwrap_or_else(empty_properties_default);

        let pronouns: Option<Pronouns> = if patch.contains_key("pronouns") {
            patch_pronouns(patch.get("pronouns").unwrap())
        } else {
            // Convert the read-side `Pronouns` (vault_overlay) to the write-side one.
            current.pronouns.as_ref().map(|p| Pronouns {
                subject: p.subject.clone(),
                object: p.object.clone(),
                possessive: p.possessive.clone(),
            })
        };
        let aliases = if patch.contains_key("aliases") {
            patch_string_array(patch.get("aliases").unwrap())
        } else {
            current.aliases
        };
        let title = if patch.contains_key("title") {
            patch_nullable_string(patch.get("title").unwrap())
        } else {
            current.title
        };
        let first_message = if patch.contains_key("firstMessage") {
            patch_nullable_string(patch.get("firstMessage").unwrap())
        } else {
            current.first_message
        };
        let talkativeness = if patch.contains_key("talkativeness") {
            patch.get("talkativeness").unwrap().as_f64().unwrap_or(0.5)
        } else {
            current.talkativeness
        };
        // v4: `'canChooseOutfit' in patch ? (patch.canChooseOutfit ?? false)
        // : current.canChooseOutfit` — a present-but-null/non-bool patch value
        // coalesces to `false`, matching `?? false`.
        let can_choose_outfit = if patch.contains_key("canChooseOutfit") {
            patch
                .get("canChooseOutfit")
                .unwrap()
                .as_bool()
                .unwrap_or(false)
        } else {
            current.can_choose_outfit
        };
        // v4: `'canDrawDiagrams' in patch ? (patch.canDrawDiagrams ?? false) :
        // current.canDrawDiagrams` — the `canChooseOutfit` arm's coalescing.
        let can_draw_diagrams = if patch.contains_key("canDrawDiagrams") {
            patch
                .get("canDrawDiagrams")
                .unwrap()
                .as_bool()
                .unwrap_or(false)
        } else {
            current.can_draw_diagrams
        };
        // v4: `'diagramStyle' in patch ? diagramStyleOrDefault(patch.diagramStyle)
        // : current.diagramStyle`.
        let diagram_style = if patch.contains_key("diagramStyle") {
            diagram_style_or_default(patch.get("diagramStyle"))
        } else {
            current.diagram_style
        };
        if touched.contains(&"canDrawDiagrams") || touched.contains(&"diagramStyle") {
            tracing::debug!(
                characterId = %character_id,
                canDrawDiagrams = can_draw_diagrams,
                diagramStyle = diagram_style.as_str(),
                "[VaultOverlay] Writing diagram settings to properties.json"
            );
        }
        // v4 `b09ca46bb`: `'kind' in patch ? characterKindOrDefault(patch.kind) :
        // current.kind` — a present `null` reads roleplay; an unrelated patch over
        // a legacy (kind-less) file WRITES `roleplay` (the lenient read's
        // default), and over an assistant file KEEPS `assistant`.
        let kind = if patch.contains_key("kind") {
            kind_or_default(patch.get("kind"))
        } else {
            current.kind
        };
        if touched.contains(&"kind") {
            tracing::debug!(
                characterId = %character_id,
                kind = kind.as_str(),
                "[VaultOverlay] Writing character kind to properties.json"
            );
        }

        let json = render_properties_json(
            pronouns.as_ref(),
            &aliases,
            title.as_deref(),
            first_message.as_deref(),
            talkativeness,
            can_choose_outfit,
            can_draw_diagrams,
            diagram_style,
            kind,
        );
        links.write_database_document(&mount_point_id, PROPERTIES_JSON_PATH, &json)?;
        for k in &touched {
            db_patch.remove(*k);
        }
    }

    // 2b. metadata.json — a whole-object REPLACE, NOT a key-merge (v4
    // `managed-fields.ts:436`). `metadata` is one field owning one file, so the
    // patch's value simply becomes the file: `patch.metadata ?? {}` (an explicit
    // null clears the sheet to `{}`). properties.json read-modify-writes because
    // FIVE Character fields share it — that dance buys nothing here and would make
    // it impossible to DELETE a key. Key-level edits are the caller's
    // read-modify-write to do. There is no DB column, so the key is stripped from
    // the DB-bound patch.
    if let Some(metadata) = patch.get("metadata") {
        let next = if metadata.is_null() {
            Value::Object(Map::new())
        } else {
            metadata.clone()
        };
        let json =
            serde_json::to_string_pretty(&next).expect("metadata.json serialization is infallible");
        links.write_database_document(&mount_point_id, METADATA_JSON_PATH, &json)?;
        db_patch.remove("metadata");
    }

    // 3. physicalDescription — non-null writes the two files; null leaves them.
    if let Some(incoming) = patch.get("physicalDescription") {
        if !incoming.is_null() {
            let mut phys: PhysicalDescriptionWrite = serde_json::from_value(incoming.clone())
                .map_err(|e| DbError::Internal(format!("physicalDescription parse: {e}")))?;
            // `humanoid` is a setting, not generated text: a writer that doesn't
            // name it (the AI Wizard, the optimizer, the head-and-shoulders
            // backfill) keeps whatever the vault already says rather than
            // resetting a non-humanoid character to humanoid (v4 `b27e5089a`).
            if phys.humanoid.is_none() {
                let current = read_current_physical_prompts(mount, &mount_point_id, character_id);
                phys.humanoid = current.and_then(|c| c.humanoid);
                tracing::debug!(
                    characterId = %character_id,
                    mountPointId = %mount_point_id,
                    humanoid = phys.humanoid.unwrap_or(true),
                    "[VaultOverlay] Physical write carries no humanoid flag; keeping the vault's"
                );
            }
            links.write_database_document(
                &mount_point_id,
                PHYSICAL_DESCRIPTION_MD_PATH,
                phys.full_description.as_deref().unwrap_or(""),
            )?;
            links.write_database_document(
                &mount_point_id,
                PHYSICAL_PROMPTS_JSON_PATH,
                &render_physical_prompts_json(Some(&phys)),
            )?;
            db_patch.remove("physicalDescription");
        }
        // Null case: leave the vault files; v4 keeps the key in the DB patch (the
        // slim update ignores it — not a slim column).
    }

    // 4. systemPrompts / scenarios — reproject the folder from the incoming array.
    if let Some(value) = patch.get("systemPrompts") {
        let prompts: Vec<SystemPromptWrite> = parse_array(value, "systemPrompts")?;
        project_array_into_vault_folder(
            &links,
            &docs,
            &mount_point_id,
            PROMPTS_FOLDER,
            &prompts,
            |p| {
                (
                    format!("{}.md", sanitize_file_name(&p.name)),
                    build_system_prompt_file(&p.name, p.is_default, &p.content),
                )
            },
            character_id,
            // v4 passes no options here: the managed-fields projections are FLAT
            // and sweep normally (`4a38222e6` left the prompts / scenarios
            // callers on exactly the top-level behaviour they always had).
            ProjectArrayOptions::FLAT,
        )?;
        db_patch.remove("systemPrompts");
    }
    if let Some(value) = patch.get("scenarios") {
        let scenarios: Vec<ScenarioWrite> = parse_array(value, "scenarios")?;
        project_array_into_vault_folder(
            &links,
            &docs,
            &mount_point_id,
            SCENARIOS_FOLDER,
            &scenarios,
            |s| {
                (
                    format!("{}.md", sanitize_file_name(&s.title)),
                    build_scenario_file(&s.title, &s.content, s.description.as_deref(), s.archived),
                )
            },
            character_id,
            // v4 passes no options here: the managed-fields projections are FLAT
            // and sweep normally (`4a38222e6` left the prompts / scenarios
            // callers on exactly the top-level behaviour they always had).
            ProjectArrayOptions::FLAT,
        )?;
        db_patch.remove("scenarios");
    }

    Ok(db_patch)
}

/// Guard every write to an archived character — v4
/// `validateCharacterArchivePatch` (`characters.repository.ts:42`, v4
/// `d553f72a`).
///
/// Archiving prunes the vault in place (§4.2a): an archived character keeps a
/// live, writable vault, and this guard — running before the write overlay —
/// is what stands between that vault and an edit arriving through the
/// repository.
///
/// A tombstone is read-only with exactly one exception: **unarchive**, the
/// single-key patch that clears `archivedAt`. Nothing else is sanctioned — in
/// particular, nulling `characterDocumentMountPointId` (the old
/// archive-finalization patch) is refused, because nothing legitimately nulls
/// the pointer any more: the vault survives the archive.
///
/// `existing` is the RAW read (no overlay), matching v4's `findByIdRaw`: an
/// unparseable vault must not be able to hide a tombstone.
pub fn validate_character_archive_patch(
    existing: Option<&Value>,
    patch: &Map<String, Value>,
) -> Result<(), DbError> {
    let Some(archived_at) = existing
        .and_then(|e| e.get("archivedAt"))
        .filter(|v| !v.is_null())
    else {
        return Ok(());
    };
    // v4's `!existing?.archivedAt` is JS-falsy, so an empty-string tombstone
    // is no tombstone at all. Reproduce that rather than "key present".
    if archived_at.as_str() == Some("") {
        return Ok(());
    }

    let is_sanctioned_unarchive =
        patch.len() == 1 && matches!(patch.get("archivedAt"), Some(Value::Null));
    if is_sanctioned_unarchive {
        return Ok(());
    }

    Err(DbError::CharacterArchived {
        // v4 falls back to the literal `'unknown'` when the raw row somehow
        // carries no id.
        character_id: existing
            .and_then(|e| e.get("id"))
            .and_then(Value::as_str)
            .unwrap_or("unknown")
            .to_string(),
    })
}

/// Update a character — v4 `CharactersRepository.update`. Routes managed fields to
/// the vault (the write overlay), then runs the slim `_update` for the unmanaged
/// remainder (skipped when empty, so a managed-only update does NOT bump the slim
/// row's `updatedAt`). The closing overlay re-read is a READ concern (no DB
/// mutation) and is left to the read overlay. Returns `true` if the slim row was
/// updated, `false` if the update was managed-only (or the row was absent).
///
/// Errors as [`apply_document_store_write_overlay`] — the `Unavailable` arm is
/// reachable only when the patch touches a `properties.json` key and the file is
/// present but unparseable.
pub fn update_character(
    main: &Connection,
    mount: &Connection,
    character_id: &str,
    patch: &Map<String, Value>,
) -> Result<bool, OverlayError> {
    // The archive write guard (v4 `d553f72a`), at v4's own placement: the raw
    // read comes FIRST, before the write overlay, so this one check covers
    // every sub-array mutator and every managed-field route as well as the
    // slim columns. `delete` stays unguarded (v4's escape hatch).
    let existing =
        crate::db::characters_read::find_by_id_raw(main, character_id).map_err(OverlayError::Db)?;
    validate_character_archive_patch(existing.as_ref(), patch).map_err(OverlayError::Db)?;

    let db_patch = apply_document_store_write_overlay(main, mount, character_id, patch)?;
    if db_patch.is_empty() {
        return Ok(false);
    }
    let update = slim_update_from_patch(&db_patch)?;
    let mut changed = CharactersRepository::new(main).update(character_id, &update)?;
    // Explicit-null clears: `slim_update_from_patch`'s `Option<T>` fields collapse
    // an ABSENT key and an explicit JSON `null` to the same `None` (= "skip"), so
    // the slim `_update` above can never NULL a column. v4's `_update` DOES null a
    // column set to `null` (e.g. the avatar / default-partner "clear" verbs). Issue
    // a supplementary NULL update for the nullable slim columns present-as-null.
    changed |= clear_null_slim_columns(main, character_id, &db_patch)?;
    // v4 `characters.repository.ts:413-415` (`ed8b15b50`, bug 186): the vault
    // is named after the character, so a rename renames it. v4's condition:
    // the update answered a row (`existing` read it), the patch carries `name`,
    // and it differs from the stored one (a JSON compare — `null` differs).
    // `name` is a slim column, so a patch carrying it never reaches the
    // managed-only early return above. Never fails the update.
    if let (Some(existing), Some(name)) = (existing.as_ref(), patch.get("name")) {
        if existing.get("name") != Some(name) {
            crate::db::store_name_reconcile::reconcile_vault_names_or_warn(
                main,
                Some(mount),
                crate::db::store_name_reconcile::TRIGGER_CHARACTER_RENAMED,
                character_id,
            );
        }
    }
    Ok(changed)
}

/// The nullable slim `characters` columns a patch may explicitly `null` (JSON key
/// == column name). MANAGED_FIELDS are already routed to the vault and stripped
/// from `db_patch`, so the remaining null-valued keys are exactly these.
const NULLABLE_SLIM_COLUMNS: &[&str] = &[
    "defaultImageId",
    "defaultConnectionProfileId",
    "defaultPartnerId",
    "defaultRoleplayTemplateId",
    "defaultImageProfileId",
    "defaultScenarioId",
    "defaultSystemPromptId",
    "characterDocumentMountPointId",
    "sillyTavernData",
    "defaultTimestampConfig",
    "defaultAgentModeEnabled",
    "defaultHelpToolsEnabled",
    "canDressThemselves",
    "canCreateOutfits",
    // P4.D324 (v4 `e1872c94f`): a vaulted character's inventory-tools flag is
    // a COLUMN write, like `canCreateOutfits`.
    "canManageInventory",
    "systemTransparency",
    "coreWhisperEnabled",
    "canBeCarina",
    // The three archive columns (v4 `d553f72a`). `archivedAt: null` is the
    // sanctioned unarchive patch (round 2's rehydrate) and reaches the column
    // through here; the other two are cleared on the same path.
    "archivedAt",
    "archiveFileId",
    "archivedAvatarFileId",
];

/// `SET <col> = NULL, …, updatedAt = now WHERE id = ?` for every nullable slim
/// column present-as-`null` in `db_patch`. Returns `true` if a row was updated.
fn clear_null_slim_columns(
    main: &Connection,
    character_id: &str,
    db_patch: &Map<String, Value>,
) -> Result<bool, DbError> {
    let cols: Vec<&str> = NULLABLE_SLIM_COLUMNS
        .iter()
        .copied()
        .filter(|col| matches!(db_patch.get(*col), Some(Value::Null)))
        .collect();
    if cols.is_empty() {
        return Ok(false);
    }
    let now = crate::clock::now_iso();
    let mut sets: Vec<String> = cols.iter().map(|c| format!("{c} = NULL")).collect();
    sets.push("updatedAt = ?1".to_string());
    let sql = format!("UPDATE characters SET {} WHERE id = ?2", sets.join(", "));
    let affected = main.execute(&sql, params![now, character_id])?;
    Ok(affected > 0)
}

/// Read the character's `characterDocumentMountPointId` (the vault FK), `None` when
/// absent / unset.
fn read_vault_fk(main: &Connection, character_id: &str) -> Result<Option<String>, DbError> {
    main.query_row(
        "SELECT characterDocumentMountPointId FROM characters WHERE id = ?1",
        params![character_id],
        |row| row.get::<_, Option<String>>(0),
    )
    .or_else(|e| match e {
        rusqlite::Error::QueryReturnedNoRows => Ok(None),
        other => Err(other),
    })
    .map_err(DbError::from)
}

/// Provision a vault mid-write for a character that has none (v4's
/// `ensureCharacterVault(character)` + reload branch). Reads the raw slim
/// character (its managed values sit at their post-cutover empty defaults), then
/// mints/scaffolds/projects/links the vault via
/// [`super::character_vault::ensure_character_vault`], re-reads the FK it set, and
/// confirms it linked. Returns the new mount-point id so the caller can route the
/// patch's managed fields onto the freshly-provisioned vault.
fn provision_vault_on_the_fly(
    main: &Connection,
    mount: &Connection,
    character_id: &str,
) -> Result<String, DbError> {
    let raw = super::characters_read::find_by_id_raw(main, character_id)?.ok_or_else(|| {
        DbError::Internal(format!(
            "applyDocumentStoreWriteOverlay: character {character_id} not found while \
             provisioning a vault on the fly"
        ))
    })?;
    let name = raw
        .get("name")
        .and_then(Value::as_str)
        .ok_or_else(|| {
            DbError::Internal(format!(
                "applyDocumentStoreWriteOverlay: character {character_id} row has no name"
            ))
        })?
        .to_string();
    // The raw slim row's managed keys are absent post-cutover, so every managed
    // field deserializes to its default (`None` markdown → `""`, talkativeness →
    // 0.5, …), matching v4's `?? <default>` coalescing off `findByIdRaw`.
    let vault: CharacterVaultWriteInput = serde_json::from_value(raw.clone())
        .map_err(|e| DbError::Internal(format!("raw character → vault input: {e}")))?;

    super::character_vault::ensure_character_vault(main, mount, character_id, &name, &vault, None)?;

    // Reload — ensureCharacterVault set characterDocumentMountPointId. Confirm.
    read_vault_fk(main, character_id)?.ok_or_else(|| {
        DbError::Internal(format!(
            "applyDocumentStoreWriteOverlay: failed to provision vault for {character_id}"
        ))
    })
}

/// Read + parse the current `properties.json` for the RMW seed.
///
/// Returns `None` ONLY for a genuinely absent `properties.json` — the caller
/// seeds [`empty_properties_default`], which is legitimate there (a vault whose
/// bag was never written, or was deliberately deleted, has nothing to preserve;
/// provisioning depends on this arm). A file that is PRESENT but unreadable or
/// unparseable ERRORS ([`OverlayError::Unavailable`], entity label `character`)
/// instead: post-cutover the six property fields have no DB columns, so seeding
/// defaults over a *failed* read silently and permanently resets all six
/// (dogfood finding #47 — the same clobber v4's `dcd9440a` closed for the
/// group/project bags; mirrors [`super::document_store_overlay::read_properties`]
/// including the `properties.json unparseable: <detail>` message form).
///
/// v5 made this refusal first; v4 has since CONVERGED (`13ddc5ee`, bug 8) via
/// `readCharacterVaultPropertiesForWrite`, so the write path is now a plain
/// equality. The fail-soft READ overlay (`vault_read_overlay.rs`) is v4-faithful
/// and deliberately untouched — a corrupt bag costs a read nothing, it is only
/// the write that loses data.
fn read_current_properties(
    mount: &Connection,
    mount_point_id: &str,
    character_id: &str,
) -> Result<Option<CharacterVaultProperties>, OverlayError> {
    let docs = DocMountDocumentsRepository::new(mount);
    let content = match docs.find_by_mount_point_and_path(mount_point_id, PROPERTIES_JSON_PATH)? {
        Some(content) => content,
        None => {
            tracing::debug!(
                entity = "character",
                entity_id = character_id,
                mount_point_id,
                "{PROPERTIES_JSON_PATH} absent — caller may seed defaults"
            );
            return Ok(None);
        }
    };
    let refuse = |detail: String| {
        tracing::error!(
            entity = "character",
            entity_id = character_id,
            mount_point_id,
            reason = %detail,
            "{PROPERTIES_JSON_PATH} unparseable — refusing to treat as absent"
        );
        OverlayError::Unavailable {
            entity_label: "character",
            id: character_id.to_string(),
            mount_point_id: Some(mount_point_id.to_string()),
            detail: format!("{PROPERTIES_JSON_PATH} unparseable: {detail}"),
        }
    };
    // Two-stage, mirroring `read_properties::<E>`: a JSON syntax error carries
    // serde's message; valid JSON that violates the schema carries the schema
    // arm. `parse_vault_properties` is the read overlay's shared validator and
    // collapses its reason (v4's Zod `safeParse` does the same) — the detail
    // tail is a stated seam, elided by the corpus normalization either way.
    if let Err(e) = serde_json::from_str::<Value>(&content) {
        return Err(refuse(e.to_string()));
    }
    match parse_vault_properties(&content) {
        Some(parsed) => Ok(Some(parsed)),
        None => Err(refuse("schema validation failed".to_string())),
    }
}

/// The RMW fallback when `properties.json` is missing — the empty-managed default
/// (v4 seeds from the raw slim row, which carries no managed values post-cutover).
fn empty_properties_default() -> CharacterVaultProperties {
    CharacterVaultProperties {
        pronouns: None,
        aliases: Vec::new(),
        title: None,
        first_message: None,
        talkativeness: 0.5,
        can_choose_outfit: false,
        can_draw_diagrams: false,
        diagram_style: DiagramStyle::HandDrawn,
        // v4 seeds `characterKindOrDefault(character.kind)` from the RAW row, whose
        // kind is always the default (every write strips it — MANAGED_FIELDS).
        kind: CharacterKind::Roleplay,
    }
}

/// v4 `readCharacterVaultPhysicalPrompts` (`vault-readers.ts:235-246`) for the
/// physical write's humanoid read-back. LENIENT, unlike
/// [`read_current_properties`]: v4's `readVaultTextFile` turns any read failure
/// into `null` (WARN `Failed to read vault file`) and `parseVaultPhysicalPrompts`
/// turns a malformed file into `null` — so an absent, unreadable or unparseable
/// file all keep the renderer's `?? true`.
pub(crate) fn read_current_physical_prompts(
    mount: &Connection,
    mount_point_id: &str,
    character_id: &str,
) -> Option<CharacterVaultPhysicalPrompts> {
    use super::database_store::{read_database_document, DbStoreErrorCode, StoreError};
    let content = match read_database_document(mount, mount_point_id, PHYSICAL_PROMPTS_JSON_PATH) {
        Ok(doc) => doc.content,
        Err(StoreError::Store(e)) if e.code == DbStoreErrorCode::NotFound => {
            // v4 `readDatabaseDocumentIfExists`'s child logger (`module: 'service'`).
            tracing::debug!(
                module = "service",
                mountPointId = %mount_point_id,
                relativePath = PHYSICAL_PROMPTS_JSON_PATH,
                "Document absent from database-backed store"
            );
            return None;
        }
        Err(error) => {
            tracing::warn!(
                mountPointId = %mount_point_id,
                path = PHYSICAL_PROMPTS_JSON_PATH,
                characterId = %character_id,
                error = %error,
                "Failed to read vault file"
            );
            return None;
        }
    };
    parse_vault_physical_prompts(&content)
}

/// A patch `pronouns` value → `Option<Pronouns>` (null → None; an object → parsed
/// into the write-side `Pronouns`).
fn patch_pronouns(v: &Value) -> Option<Pronouns> {
    if v.is_null() {
        return None;
    }
    serde_json::from_value(v.clone()).ok()
}

/// A patch string-array value → `Vec<String>` (null → empty).
fn patch_string_array(v: &Value) -> Vec<String> {
    v.as_array()
        .map(|a| {
            a.iter()
                .filter_map(|x| x.as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default()
}

/// A patch nullable-string value → `Option<String>` (null/non-string → None).
fn patch_nullable_string(v: &Value) -> Option<String> {
    v.as_str().map(str::to_string)
}

/// Parse a patch array (`systemPrompts` / `scenarios`) into the typed write inputs.
fn parse_array<T: serde::de::DeserializeOwned>(v: &Value, label: &str) -> Result<Vec<T>, DbError> {
    if v.is_null() {
        return Ok(Vec::new());
    }
    serde_json::from_value(v.clone()).map_err(|e| DbError::Internal(format!("{label} parse: {e}")))
}

/// Build the slim-row update patch from the DB-bound remainder. Only the slim
/// (non-managed) columns are recognized; any leftover keys are ignored (v4's
/// `_update` strips `MANAGED_FIELDS` defensively, and a managed key never survives
/// the overlay anyway). `updatedAt` is minted (v4 `_update` mints `now` when the
/// patch carries none).
fn slim_update_from_patch(db_patch: &Map<String, Value>) -> Result<CharacterUpdate, DbError> {
    // Deserialize the recognized slim columns; unknown/managed keys are ignored.
    #[derive(serde::Deserialize, Default)]
    #[serde(rename_all = "camelCase")]
    struct SlimPatch {
        #[serde(default)]
        user_id: Option<String>,
        #[serde(default)]
        name: Option<String>,
        #[serde(default)]
        default_image_id: Option<String>,
        #[serde(default)]
        default_connection_profile_id: Option<String>,
        #[serde(default)]
        default_partner_id: Option<String>,
        #[serde(default)]
        default_roleplay_template_id: Option<String>,
        #[serde(default)]
        default_image_profile_id: Option<String>,
        #[serde(default)]
        silly_tavern_data: Option<Value>,
        #[serde(default)]
        is_favorite: Option<bool>,
        #[serde(default)]
        npc: Option<bool>,
        #[serde(default)]
        controlled_by: Option<String>,
        #[serde(default)]
        default_agent_mode_enabled: Option<bool>,
        #[serde(default)]
        default_help_tools_enabled: Option<bool>,
        #[serde(default)]
        default_timestamp_config: Option<super::characters::TimestampConfig>,
        #[serde(default)]
        default_scenario_id: Option<String>,
        #[serde(default)]
        default_system_prompt_id: Option<String>,
        #[serde(default)]
        character_document_mount_point_id: Option<String>,
        #[serde(default)]
        can_dress_themselves: Option<bool>,
        #[serde(default)]
        can_create_outfits: Option<bool>,
        #[serde(default)]
        can_manage_inventory: Option<bool>,
        #[serde(default)]
        system_transparency: Option<bool>,
        #[serde(default)]
        core_whisper_enabled: Option<bool>,
        #[serde(default)]
        can_be_carina: Option<bool>,
        #[serde(default)]
        partner_links: Option<Vec<super::characters::PartnerLink>>,
        #[serde(default)]
        tags: Option<Vec<String>>,
        #[serde(default)]
        avatar_overrides: Option<Vec<super::characters::AvatarOverride>>,
        // The three archive columns (v4 `d553f72a`).
        #[serde(default)]
        archived_at: Option<String>,
        #[serde(default)]
        archive_file_id: Option<String>,
        #[serde(default)]
        archived_avatar_file_id: Option<String>,
    }

    let p: SlimPatch = serde_json::from_value(Value::Object(db_patch.clone()))
        .map_err(|e| DbError::Internal(format!("slim patch parse: {e}")))?;

    Ok(CharacterUpdate {
        user_id: p.user_id,
        name: p.name,
        default_image_id: p.default_image_id,
        default_connection_profile_id: p.default_connection_profile_id,
        default_partner_id: p.default_partner_id,
        default_roleplay_template_id: p.default_roleplay_template_id,
        default_image_profile_id: p.default_image_profile_id,
        silly_tavern_data: p.silly_tavern_data,
        is_favorite: p.is_favorite,
        npc: p.npc,
        controlled_by: p.controlled_by,
        default_agent_mode_enabled: p.default_agent_mode_enabled,
        default_help_tools_enabled: p.default_help_tools_enabled,
        default_timestamp_config: p.default_timestamp_config,
        default_scenario_id: p.default_scenario_id,
        default_system_prompt_id: p.default_system_prompt_id,
        character_document_mount_point_id: p.character_document_mount_point_id,
        can_dress_themselves: p.can_dress_themselves,
        can_create_outfits: p.can_create_outfits,
        can_manage_inventory: p.can_manage_inventory.map(Some),
        system_transparency: p.system_transparency,
        core_whisper_enabled: p.core_whisper_enabled,
        can_be_carina: p.can_be_carina,
        partner_links: p.partner_links,
        tags: p.tags,
        avatar_overrides: p.avatar_overrides,
        archived_at: p.archived_at,
        archive_file_id: p.archive_file_id,
        archived_avatar_file_id: p.archived_avatar_file_id,
        updated_at: crate::clock::now_iso(),
    })
}

#[cfg(test)]
mod p4d349_log_tests {
    //! The two P4.D349 write-overlay lines, capture-pinned with their silence
    //! legs: `[VaultOverlay] Physical write carries no humanoid flag; keeping
    //! the vault's` (`managed-fields.ts:518-522`, `{characterId, mountPointId,
    //! humanoid: humanoid ?? true}`) and `[VaultOverlay] Writing diagram
    //! settings to properties.json` (`:467-473`, `{characterId,
    //! canDrawDiagrams, diagramStyle}`). Both fire on the caller thread.
    use super::*;
    use crate::db::Writer;
    use crate::test_support::captured_with;
    use serde_json::json;

    const TEST_PEPPER: &str = "dGVzdHBlcHBlcnRlc3RwZXBwZXJ0ZXN0cGVwcGVyMDE=";
    const CHAR_A: &str = "a1000000-0000-4000-8000-0000000000a1";

    fn open_pair(dir: &tempfile::TempDir) -> (Writer, Writer) {
        let fx = |n: &str| {
            std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("../quilltap-web/tests/fixtures")
                .join(n)
        };
        let main = dir.path().join("main.db");
        let mount = dir.path().join("mount.db");
        std::fs::copy(fx("subprompts-main.db"), &main).unwrap();
        std::fs::copy(fx("subprompts-mount.db"), &mount).unwrap();
        (
            Writer::open_writable(&main, TEST_PEPPER).unwrap(),
            Writer::open_writable(&mount, TEST_PEPPER).unwrap(),
        )
    }

    fn patch(v: Value) -> Map<String, Value> {
        v.as_object().unwrap().clone()
    }

    fn physical_prompts(mount: &Connection, mp: &str) -> Value {
        let raw = DocMountDocumentsRepository::new(mount)
            .find_by_mount_point_and_path(mp, PHYSICAL_PROMPTS_JSON_PATH)
            .unwrap()
            .unwrap();
        serde_json::from_str(&raw).unwrap()
    }

    const PHYS_LINE: &str =
        "[VaultOverlay] Physical write carries no humanoid flag; keeping the vault's";
    const DIAGRAM_LINE: &str = "[VaultOverlay] Writing diagram settings to properties.json";

    #[test]
    fn an_omitted_humanoid_flag_keeps_the_vaults_and_says_so() {
        let dir = tempfile::tempdir().unwrap();
        let (main, mount) = open_pair(&dir);
        let mp = read_vault_fk(main.connection(), CHAR_A).unwrap().unwrap();

        // Explicit `false`: written as given, no read-back, no line.
        let (_, lines) = captured_with(|| {
            update_character(
                main.connection(),
                mount.connection(),
                CHAR_A,
                &patch(
                    json!({"physicalDescription": {"shortPrompt": "a lantern", "humanoid": false}}),
                ),
            )
            .unwrap()
        });
        assert!(
            !lines.iter().any(|l| l.contains(PHYS_LINE)),
            "an explicit flag is silent: {lines:?}"
        );
        assert_eq!(
            physical_prompts(mount.connection(), &mp)["humanoid"],
            json!(false)
        );

        // Omitted: the vault's `false` is kept, and the DEBUG names it.
        let (_, lines) = captured_with(|| {
            update_character(
                main.connection(),
                mount.connection(),
                CHAR_A,
                &patch(json!({"physicalDescription": {"shortPrompt": "a brighter lantern"}})),
            )
            .unwrap()
        });
        let l = lines
            .iter()
            .find(|l| l.contains(PHYS_LINE))
            .unwrap_or_else(|| panic!("no physical line in {lines:?}"));
        assert!(l.contains("DEBUG"), "{l}");
        assert!(l.contains(&format!("characterId={CHAR_A}")), "{l}");
        assert!(l.contains(&format!("mountPointId={mp}")), "{l}");
        assert!(l.contains("humanoid=false"), "{l}");
        let prompts = physical_prompts(mount.connection(), &mp);
        assert_eq!(prompts["humanoid"], json!(false));
        assert_eq!(prompts["short"], json!("a brighter lantern"));
    }

    #[test]
    fn the_diagram_line_fires_only_when_a_diagram_key_is_touched() {
        let dir = tempfile::tempdir().unwrap();
        let (main, mount) = open_pair(&dir);

        let (_, lines) = captured_with(|| {
            update_character(
                main.connection(),
                mount.connection(),
                CHAR_A,
                &patch(json!({"title": "The Lamplighter"})),
            )
            .unwrap()
        });
        assert!(
            !lines.iter().any(|l| l.contains(DIAGRAM_LINE)),
            "a title-only patch is silent: {lines:?}"
        );

        let (_, lines) = captured_with(|| {
            update_character(
                main.connection(),
                mount.connection(),
                CHAR_A,
                &patch(json!({"canDrawDiagrams": true, "diagramStyle": "crayon"})),
            )
            .unwrap()
        });
        let l = lines
            .iter()
            .find(|l| l.contains(DIAGRAM_LINE))
            .unwrap_or_else(|| panic!("no diagram line in {lines:?}"));
        assert!(l.contains("DEBUG"), "{l}");
        assert!(l.contains(&format!("characterId={CHAR_A}")), "{l}");
        assert!(l.contains("canDrawDiagrams=true"), "{l}");
        assert!(l.contains("diagramStyle=handDrawn"), "{l}");
    }
}

#[cfg(test)]
mod p4d369_kind_tests {
    //! P4.D369 (v4 `b09ca46bb`): the `kind` arm of the properties.json
    //! read-modify-write — v4's overlay test's `properties.json — character
    //! kind` describe — and the NEW `[VaultOverlay] Writing character kind to
    //! properties.json` DEBUG (`managed-fields.ts:481-486`, `{characterId,
    //! kind}`), capture-pinned with its silence leg. Fires on the caller thread.
    use super::*;
    use crate::db::Writer;
    use crate::test_support::captured_with;
    use serde_json::json;

    const TEST_PEPPER: &str = "dGVzdHBlcHBlcnRlc3RwZXBwZXJ0ZXN0cGVwcGVyMDE=";
    const CHAR_A: &str = "a1000000-0000-4000-8000-0000000000a1";
    const KIND_LINE: &str = "[VaultOverlay] Writing character kind to properties.json";
    const DIAGRAM_LINE: &str = "[VaultOverlay] Writing diagram settings to properties.json";
    /// A properties.json written before kinds existed (v4 `6588972d5`'s eight keys).
    const LEGACY: &str = "{\n  \"pronouns\": null,\n  \"aliases\": [\n    \"Lamps\"\n  ],\n  \"title\": \"The Lamplighter\",\n  \"firstMessage\": null,\n  \"talkativeness\": 0.7,\n  \"canChooseOutfit\": false,\n  \"canDrawDiagrams\": false,\n  \"diagramStyle\": \"handDrawn\"\n}";

    fn open_pair(dir: &tempfile::TempDir) -> (Writer, Writer) {
        let fx = |n: &str| {
            std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("../quilltap-web/tests/fixtures")
                .join(n)
        };
        let main = dir.path().join("main.db");
        let mount = dir.path().join("mount.db");
        std::fs::copy(fx("subprompts-main.db"), &main).unwrap();
        std::fs::copy(fx("subprompts-mount.db"), &mount).unwrap();
        (
            Writer::open_writable(&main, TEST_PEPPER).unwrap(),
            Writer::open_writable(&mount, TEST_PEPPER).unwrap(),
        )
    }

    fn patch(v: Value) -> Map<String, Value> {
        v.as_object().unwrap().clone()
    }

    fn plant(mount: &Connection, mp: &str, content: &str) {
        DocMountFileLinksRepository::new(mount)
            .write_database_document(mp, PROPERTIES_JSON_PATH, content)
            .unwrap();
    }

    fn props(mount: &Connection, mp: &str) -> String {
        DocMountDocumentsRepository::new(mount)
            .find_by_mount_point_and_path(mp, PROPERTIES_JSON_PATH)
            .unwrap()
            .unwrap()
    }

    fn update(main: &Writer, mount: &Writer, p: Value) -> Vec<String> {
        captured_with(|| {
            update_character(main.connection(), mount.connection(), CHAR_A, &patch(p)).unwrap()
        })
        .1
    }

    #[test]
    fn an_unrelated_patch_over_a_legacy_file_writes_the_default_kind() {
        let dir = tempfile::tempdir().unwrap();
        let (main, mount) = open_pair(&dir);
        let mp = read_vault_fk(main.connection(), CHAR_A).unwrap().unwrap();
        plant(mount.connection(), &mp, LEGACY);

        let lines = update(&main, &mount, json!({"talkativeness": 0.6}));
        let want = LEGACY.replace("0.7", "0.6").replace(
            "\"handDrawn\"\n}",
            "\"handDrawn\",\n  \"kind\": \"roleplay\"\n}",
        );
        assert_eq!(props(mount.connection(), &mp), want, "nine keys, kind LAST");
        assert!(
            !lines.iter().any(|l| l.contains(KIND_LINE)),
            "an untouched kind is silent: {lines:?}"
        );
    }

    #[test]
    fn an_unrelated_patch_keeps_an_assistant_kind() {
        let dir = tempfile::tempdir().unwrap();
        let (main, mount) = open_pair(&dir);
        let mp = read_vault_fk(main.connection(), CHAR_A).unwrap().unwrap();
        let assistant = LEGACY.replace(
            "\"handDrawn\"\n}",
            "\"handDrawn\",\n  \"kind\": \"assistant\"\n}",
        );
        plant(mount.connection(), &mp, &assistant);

        // The erasure hazard: before b09ca46bb every v5 write dropped the key.
        for p in [
            json!({"title": "The Steward"}),
            json!({"talkativeness": 0.9}),
        ] {
            let lines = update(&main, &mount, p.clone());
            assert!(
                props(mount.connection(), &mp).ends_with("\"kind\": \"assistant\"\n}"),
                "{p}: {}",
                props(mount.connection(), &mp)
            );
            assert!(
                !lines.iter().any(|l| l.contains(KIND_LINE)),
                "{p} leaves kind untouched and is silent: {lines:?}"
            );
        }
    }

    #[test]
    fn a_kind_patch_writes_only_that_key_and_keeps_every_other() {
        let dir = tempfile::tempdir().unwrap();
        let (main, mount) = open_pair(&dir);
        let mp = read_vault_fk(main.connection(), CHAR_A).unwrap().unwrap();
        plant(mount.connection(), &mp, LEGACY);
        let slim_before: String = main
            .connection()
            .query_row(
                "SELECT updatedAt FROM characters WHERE id = ?1",
                [CHAR_A],
                |r| r.get(0),
            )
            .unwrap();

        let lines = update(&main, &mount, json!({"kind": "assistant"}));
        let want = LEGACY.replace(
            "\"handDrawn\"\n}",
            "\"handDrawn\",\n  \"kind\": \"assistant\"\n}",
        );
        assert_eq!(props(mount.connection(), &mp), want);
        let l = lines
            .iter()
            .find(|l| l.contains(KIND_LINE))
            .unwrap_or_else(|| panic!("no kind line in {lines:?}"));
        assert!(l.contains("DEBUG"), "{l}");
        assert!(l.contains(&format!("characterId={CHAR_A}")), "{l}");
        assert!(l.contains("kind=assistant"), "{l}");

        // MANAGED_FIELDS strips it: a kind-only patch never reaches the slim row.
        let slim_after: String = main
            .connection()
            .query_row(
                "SELECT updatedAt FROM characters WHERE id = ?1",
                [CHAR_A],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(slim_before, slim_after, "the slim row was not touched");

        // A bad value through the repository reads roleplay (v4's
        // `characterKindOrDefault(patch.kind)`), and so does a present null.
        for bad in [json!("robot"), json!(null), json!("Assistant")] {
            plant(mount.connection(), &mp, &want);
            let lines = update(&main, &mount, json!({ "kind": bad }));
            assert!(
                props(mount.connection(), &mp).ends_with("\"kind\": \"roleplay\"\n}"),
                "{bad}"
            );
            assert!(
                lines
                    .iter()
                    .any(|l| l.contains(KIND_LINE) && l.contains("kind=roleplay")),
                "{bad}: {lines:?}"
            );
        }
    }

    #[test]
    fn the_kind_line_follows_the_diagram_line() {
        let dir = tempfile::tempdir().unwrap();
        let (main, mount) = open_pair(&dir);
        let lines = update(
            &main,
            &mount,
            json!({"diagramStyle": "clean", "kind": "assistant"}),
        );
        let d = lines
            .iter()
            .position(|l| l.contains(DIAGRAM_LINE))
            .unwrap_or_else(|| panic!("no diagram line in {lines:?}"));
        let k = lines
            .iter()
            .position(|l| l.contains(KIND_LINE))
            .unwrap_or_else(|| panic!("no kind line in {lines:?}"));
        assert!(d < k, "v4 logs the diagram line first: {lines:?}");
        assert_eq!(
            lines.iter().filter(|l| l.contains(KIND_LINE)).count(),
            1,
            "{lines:?}"
        );
    }

    #[test]
    fn an_absent_file_seeds_the_default_kind() {
        let dir = tempfile::tempdir().unwrap();
        let (main, mount) = open_pair(&dir);
        let mp = read_vault_fk(main.connection(), CHAR_A).unwrap().unwrap();
        DocMountFileLinksRepository::new(mount.connection())
            .delete_database_document(&mp, PROPERTIES_JSON_PATH)
            .unwrap();
        update(&main, &mount, json!({"title": "Seeded"}));
        assert!(props(mount.connection(), &mp).ends_with("\"kind\": \"roleplay\"\n}"));
    }
}
