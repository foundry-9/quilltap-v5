//! The character **read overlay** — ports v4's
//! `lib/database/repositories/vault-overlay/read-overlay.ts`. Folds a character's
//! vault files (the single-file overlay paths + the `Prompts/` / `Scenarios/`
//! folders) onto the character so every read path sees vault values transparently.
//!
//! Unlike the generic store-backed overlay (groups/projects), this is the
//! **bespoke character hydration**: it patches a fixed set of managed fields and
//! has its own inlined fetch path. Because the overlay is a plain JSON merge
//! (`out = { ...out, field: value }`), the port operates on the character as a
//! `serde_json::Value` object rather than a fully-typed `Character`, patching the
//! managed keys with values produced by the already-ported pure parsers
//! ([`crate::vault_overlay`]).
//!
//! ## Failure is asymmetric and store-only (no DB-column fallback post-cutover)
//!
//!   - [`apply_document_store_overlay_one`] (single, behind `findById`) returns the
//!     [`VaultUnavailable`] error — the caller asked for that one character.
//!   - [`apply_document_store_overlay`] (batched, behind `findAll`) DROPS the
//!     offending character so one corrupt vault can't take down the whole roster.
//!
//! The keystone is `properties.json`: a linked vault that lacks it is broken
//! (`VaultUnavailable`); every other file is optional and simply not overlaid when
//! absent.

use std::collections::{HashMap, HashSet};

use serde_json::{Map, Value};

use super::doc_mount_documents::{DocMountDocumentsRepository, VaultFolderDoc};
use super::DbError;
use crate::vault_overlay::{
    markdown_to_nullable, parse_legacy_wardrobe_json, parse_prompt_file, parse_scenario_file,
    parse_vault_physical_prompts, parse_vault_properties, parse_wardrobe_item_file,
    resolve_and_check_component_items, slugify_wardrobe_title, stable_uuid_from_string,
    CharacterScenario, CharacterSystemPrompt, VaultDoc, WardrobeItemFromFile,
};

/// The eight single-file overlay paths (v4 `SINGLE_FILE_OVERLAY_PATHS`), in order.
pub const SINGLE_FILE_OVERLAY_PATHS: [&str; 8] = [
    "properties.json",
    "identity.md",
    "description.md",
    "manifesto.md",
    "personality.md",
    "example-dialogues.md",
    "physical-description.md",
    "physical-prompts.json",
];

const PROMPTS_FOLDER: &str = "Prompts";
const SCENARIOS_FOLDER: &str = "Scenarios";
const WARDROBE_FOLDER: &str = "Wardrobe";
const WARDROBE_JSON_PATH: &str = "wardrobe.json";

/// Returned when a character with a linked vault has no usable vault — a missing
/// `properties.json` keystone (v4 `CharacterVaultUnavailableError`).
#[derive(Debug, Clone)]
pub struct VaultUnavailable {
    pub character_id: String,
    pub mount_id: String,
}

/// The loaded vault files for a set of mount points, keyed for hydration
/// (v4 `VaultFileMaps`).
pub struct VaultFileMaps {
    /// path → (mountPointId → file content) for each single-file overlay path.
    content_by_mount_by_path: HashMap<&'static str, HashMap<String, String>>,
    /// mountPointId → `Prompts/*.md` docs.
    prompts_by_mount: HashMap<String, Vec<VaultFolderDoc>>,
    /// mountPointId → `Scenarios/*.md` docs.
    scenarios_by_mount: HashMap<String, Vec<VaultFolderDoc>>,
}

impl VaultFileMaps {
    fn content(&self, path: &str, mount_id: &str) -> Option<&String> {
        self.content_by_mount_by_path
            .get(path)
            .and_then(|m| m.get(mount_id))
    }
    fn prompts(&self, mount_id: &str) -> Vec<&VaultFolderDoc> {
        self.prompts_by_mount
            .get(mount_id)
            .map(|v| v.iter().collect())
            .unwrap_or_default()
    }
    fn scenarios(&self, mount_id: &str) -> Vec<&VaultFolderDoc> {
        self.scenarios_by_mount
            .get(mount_id)
            .map(|v| v.iter().collect())
            .unwrap_or_default()
    }
}

/// Load every vault file the overlay needs for the given mount points (v4
/// `loadVaultFileMaps`): one batched single-file query per overlay path plus the
/// two directory listings. Read failures propagate (no swallow — there is no DB
/// fallback post-cutover).
pub fn load_vault_file_maps(
    repo: &DocMountDocumentsRepository,
    mount_point_ids: &[String],
) -> Result<VaultFileMaps, DbError> {
    let mut content_by_mount_by_path: HashMap<&'static str, HashMap<String, String>> =
        HashMap::new();
    for path in SINGLE_FILE_OVERLAY_PATHS {
        let pairs = repo.find_many_by_mount_points_and_path(mount_point_ids, path)?;
        let mut by_mount = HashMap::new();
        for (mount_id, content) in pairs {
            by_mount.insert(mount_id, content);
        }
        content_by_mount_by_path.insert(path, by_mount);
    }

    let mut prompts_by_mount: HashMap<String, Vec<VaultFolderDoc>> = HashMap::new();
    for doc in repo.find_many_by_mount_points_in_folder(mount_point_ids, PROMPTS_FOLDER, ".md")? {
        prompts_by_mount
            .entry(doc.mount_point_id.clone())
            .or_default()
            .push(doc);
    }
    let mut scenarios_by_mount: HashMap<String, Vec<VaultFolderDoc>> = HashMap::new();
    for doc in repo.find_many_by_mount_points_in_folder(mount_point_ids, SCENARIOS_FOLDER, ".md")? {
        scenarios_by_mount
            .entry(doc.mount_point_id.clone())
            .or_default()
            .push(doc);
    }

    Ok(VaultFileMaps {
        content_by_mount_by_path,
        prompts_by_mount,
        scenarios_by_mount,
    })
}

/// A character is subject to overlay iff `characterDocumentMountPointId` is truthy
/// (v4 `hasLinkedVault` = `!!id` — a non-null, non-empty string).
fn linked_mount_id(obj: &Map<String, Value>) -> Option<&str> {
    obj.get("characterDocumentMountPointId")
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty())
}

/// `Option<String>` → JSON value (`Some(s)` → string, `None` → `null`).
fn opt_to_value(o: Option<String>) -> Value {
    o.map(Value::String).unwrap_or(Value::Null)
}

/// A `VaultDoc` borrowing a folder-listed doc, for the per-file parsers.
fn vault_doc(d: &VaultFolderDoc) -> VaultDoc<'_> {
    VaultDoc {
        content: &d.content,
        mount_point_id: &d.mount_point_id,
        relative_path: &d.relative_path,
        file_name: &d.file_name,
        created_at: &d.created_at,
        updated_at: &d.updated_at,
    }
}

/// The fresh `PhysicalDescription` base v4 mints when a character has none but the
/// vault carries physical files. `createdAt`/`updatedAt` are minted via the
/// system clock (the read overlay's only nondeterminism — the differential
/// placeholders them for this branch).
fn default_physical(mount_id: &str) -> Map<String, Value> {
    let now = crate::clock::now_iso();
    let mut m = Map::new();
    m.insert(
        "id".into(),
        Value::String(stable_uuid_from_string(&format!("physical:{mount_id}"))),
    );
    m.insert("name".into(), Value::String("default".into()));
    m.insert("usageContext".into(), Value::Null);
    m.insert("headAndShouldersPrompt".into(), Value::Null);
    m.insert("shortPrompt".into(), Value::Null);
    m.insert("mediumPrompt".into(), Value::Null);
    m.insert("longPrompt".into(), Value::Null);
    m.insert("completePrompt".into(), Value::Null);
    m.insert("fullDescription".into(), Value::Null);
    m.insert("createdAt".into(), Value::String(now.clone()));
    m.insert("updatedAt".into(), Value::String(now));
    m
}

/// Ensure exactly one `isDefault` among the parsed prompts (v4 `hydrateOne`'s
/// normalization): keep the first declared default and demote the rest; if none
/// is marked default, promote the first (already in sorted order). No-op on empty.
fn normalize_prompt_defaults(prompts: &mut [CharacterSystemPrompt]) {
    if prompts.is_empty() {
        return;
    }
    let mut seen_default = false;
    for p in prompts.iter_mut() {
        if p.is_default {
            if seen_default {
                p.is_default = false;
            } else {
                seen_default = true;
            }
        }
    }
    if !seen_default {
        prompts[0].is_default = true;
    }
}

/// Hydrate one character from the loaded vault files (v4 `hydrateOne`). A
/// character without a linked vault is returned unchanged; one whose
/// `properties.json` keystone is absent yields [`VaultUnavailable`].
pub fn hydrate_one(character: &Value, maps: &VaultFileMaps) -> Result<Value, VaultUnavailable> {
    let obj = match character.as_object() {
        Some(o) => o,
        None => return Ok(character.clone()),
    };
    let mount_id = match linked_mount_id(obj) {
        Some(m) => m.to_string(),
        None => return Ok(character.clone()),
    };

    // Keystone: a provisioned vault always carries properties.json. Its absence
    // means the vault is missing/unpopulated — fail rather than hollow the char.
    let props_raw = match maps.content("properties.json", &mount_id) {
        Some(r) => r,
        None => {
            return Err(VaultUnavailable {
                character_id: obj
                    .get("id")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_string(),
                mount_id,
            })
        }
    };

    let mut out = obj.clone();

    // properties.json → pronouns, aliases, title, firstMessage, talkativeness.
    if let Some(parsed) = parse_vault_properties(props_raw) {
        let pv = serde_json::to_value(parsed).expect("serialize vault properties");
        for key in [
            "pronouns",
            "aliases",
            "title",
            "firstMessage",
            "talkativeness",
        ] {
            if let Some(v) = pv.get(key) {
                out.insert(key.to_string(), v.clone());
            }
        }
    }

    // The plain markdown fields (empty → null).
    for (path, field) in [
        ("identity.md", "identity"),
        ("description.md", "description"),
        ("manifesto.md", "manifesto"),
        ("personality.md", "personality"),
        ("example-dialogues.md", "exampleDialogues"),
    ] {
        if let Some(raw) = maps.content(path, &mount_id) {
            out.insert(field.to_string(), markdown_to_nullable(raw));
        }
    }

    // physical-description.md + physical-prompts.json → physicalDescription.
    let phys_desc = maps.content("physical-description.md", &mount_id);
    let phys_prompts = maps.content("physical-prompts.json", &mount_id);
    if phys_desc.is_some() || phys_prompts.is_some() {
        let mut patched: Map<String, Value> = match out.get("physicalDescription") {
            Some(Value::Object(b)) => b.clone(),
            _ => default_physical(&mount_id),
        };
        if let Some(raw) = phys_desc {
            patched.insert("fullDescription".into(), markdown_to_nullable(raw));
        }
        if let Some(raw) = phys_prompts {
            if let Some(pp) = parse_vault_physical_prompts(raw) {
                patched.insert(
                    "headAndShouldersPrompt".into(),
                    opt_to_value(pp.head_and_shoulders),
                );
                patched.insert("shortPrompt".into(), opt_to_value(pp.short));
                patched.insert("mediumPrompt".into(), opt_to_value(pp.medium));
                patched.insert("longPrompt".into(), opt_to_value(pp.long));
                patched.insert("completePrompt".into(), opt_to_value(pp.complete));
            }
        }
        out.insert("physicalDescription".into(), Value::Object(patched));
    }

    // Prompts/*.md → systemPrompts (sorted, parsed, default-normalized).
    let mut prompt_docs = maps.prompts(&mount_id);
    prompt_docs.sort_by(|a, b| a.relative_path.cmp(&b.relative_path));
    let mut prompts: Vec<CharacterSystemPrompt> = prompt_docs
        .iter()
        .filter_map(|d| parse_prompt_file(&vault_doc(d)))
        .collect();
    normalize_prompt_defaults(&mut prompts);
    out.insert(
        "systemPrompts".into(),
        serde_json::to_value(&prompts).expect("serialize systemPrompts"),
    );

    // Scenarios/*.md → scenarios (sorted, parsed).
    let mut scenario_docs = maps.scenarios(&mount_id);
    scenario_docs.sort_by(|a, b| a.relative_path.cmp(&b.relative_path));
    let scenarios: Vec<CharacterScenario> = scenario_docs
        .iter()
        .filter_map(|d| parse_scenario_file(&vault_doc(d)))
        .collect();
    out.insert(
        "scenarios".into(),
        serde_json::to_value(&scenarios).expect("serialize scenarios"),
    );

    Ok(Value::Object(out))
}

/// Apply the vault overlay to a list of characters (v4 `applyDocumentStoreOverlay`).
/// Non-linked characters pass through unchanged; a character whose vault is
/// unavailable is DROPPED (order otherwise preserved). One batched load covers
/// every candidate mount point.
pub fn apply_document_store_overlay(
    repo: &DocMountDocumentsRepository,
    characters: Vec<Value>,
) -> Result<Vec<Value>, DbError> {
    if characters.is_empty() {
        return Ok(characters);
    }
    let mut mount_ids: Vec<String> = Vec::new();
    let mut seen: HashSet<String> = HashSet::new();
    for c in &characters {
        if let Some(m) = c.as_object().and_then(linked_mount_id) {
            if seen.insert(m.to_string()) {
                mount_ids.push(m.to_string());
            }
        }
    }
    if mount_ids.is_empty() {
        return Ok(characters);
    }

    let maps = load_vault_file_maps(repo, &mount_ids)?;
    let mut out = Vec::with_capacity(characters.len());
    for c in &characters {
        match hydrate_one(c, &maps) {
            Ok(h) => out.push(h),
            Err(_) => { /* vault unavailable → drop */ }
        }
    }
    Ok(out)
}

/// Error from [`apply_document_store_overlay_one`].
#[derive(Debug)]
pub enum OverlayOneError {
    /// A read failure loading the vault files.
    Db(DbError),
    /// The character's vault is unavailable (missing keystone) — v4 throws
    /// `CharacterVaultUnavailableError`, mapped to a 503.
    Unavailable(VaultUnavailable),
}

/// Single-character overlay (v4 `applyDocumentStoreOverlayOne`). Returns
/// [`OverlayOneError::Unavailable`] when the vault is unavailable — the caller
/// asked for this specific character, so fail loudly rather than drop.
pub fn apply_document_store_overlay_one(
    repo: &DocMountDocumentsRepository,
    character: Option<Value>,
) -> Result<Option<Value>, OverlayOneError> {
    let character = match character {
        Some(c) => c,
        None => return Ok(None),
    };
    let mount_id = match character.as_object().and_then(linked_mount_id) {
        Some(m) => m.to_string(),
        None => return Ok(Some(character)),
    };
    let maps = load_vault_file_maps(repo, &[mount_id]).map_err(OverlayOneError::Db)?;
    match hydrate_one(&character, &maps) {
        Ok(h) => Ok(Some(h)),
        Err(u) => Err(OverlayOneError::Unavailable(u)),
    }
}

/// Read a character's vault wardrobe (v4 `readCharacterVaultWardrobe`,
/// `vault-overlay/vault-readers.ts:234`). Returns `Some({ items })` — from the
/// `Wardrobe/*.md` folder layout when present, else the legacy `wardrobe.json`
/// — or `None` when neither yields a usable list.
///
/// The `Wardrobe/*.md` path parses each file, builds the in-vault slug/id lookup
/// maps (first-claimer wins a slug; every item is addressable by id), and runs
/// [`resolve_and_check_component_items`] to canonicalize and cycle-check the
/// `componentItems:` refs. Files are sorted by `relativePath` under the
/// Decision-B code-unit order (Rust `str::cmp`) before parsing.
///
/// **Tracked deferral — archetype seeding.** v4 additionally seeds the lookup
/// maps with shared archetypes (`repos.wardrobe.findArchetypes(true)` → the
/// General/project `Wardrobe` stores) so composites can reference shared items.
/// That pulls in the General-Wardrobe subsystem and is not ported here; the
/// differential keeps no General store provisioned, so v4's `findArchetypes`
/// returns `[]` and the seed is a verified no-op (component refs resolve within
/// the character's own vault). Close this before reading vaults that reference
/// shared archetypes.
pub fn read_character_vault_wardrobe(
    repo: &DocMountDocumentsRepository,
    mount_point_id: &str,
    character_id: &str,
) -> Result<Option<Value>, DbError> {
    let mount = [mount_point_id.to_string()];
    let mut item_docs = repo.find_many_by_mount_points_in_folder(&mount, WARDROBE_FOLDER, ".md")?;

    if !item_docs.is_empty() {
        // Decision-B code-unit sort by relativePath, then parse, then drop the
        // files that can't yield a valid item.
        item_docs.sort_by(|a, b| a.relative_path.cmp(&b.relative_path));
        let mut items: Vec<WardrobeItemFromFile> = item_docs
            .iter()
            .filter_map(|d| parse_wardrobe_item_file(&vault_doc(d), character_id))
            .collect();

        // Build the id/slug lookup maps (index-valued). Every item is addressable
        // by id; a slug goes to its first claimer (empty/duplicate slugs skipped).
        let mut item_by_id: HashMap<String, usize> = HashMap::new();
        let mut item_by_slug: HashMap<String, usize> = HashMap::new();
        let mut claimed: HashSet<String> = HashSet::new();
        for (i, item) in items.iter().enumerate() {
            item_by_id.insert(item.id.clone(), i);
            let slug = slugify_wardrobe_title(&item.title);
            if slug.is_empty() || claimed.contains(&slug) {
                continue;
            }
            claimed.insert(slug.clone());
            item_by_slug.insert(slug, i);
        }

        resolve_and_check_component_items(&mut items, &item_by_slug, &item_by_id);

        return Ok(Some(serde_json::json!({
            "items": serde_json::to_value(&items).expect("serialize wardrobe items"),
        })));
    }

    // Folder empty/missing — fall through to legacy wardrobe.json so
    // pre-migration vaults still surface their items.
    let legacy = repo.find_many_by_mount_points_and_path(&mount, WARDROBE_JSON_PATH)?;
    let Some((_, content)) = legacy.into_iter().next() else {
        return Ok(None);
    };
    Ok(parse_legacy_wardrobe_json(&content)
        .map(|lw| serde_json::to_value(&lw).expect("serialize legacy wardrobe")))
}
