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
use super::scenarios::strip_md_suffix;
use super::DbError;
use crate::scenarios::sort::{sort_scenarios, SortableScenario};
use crate::vault_overlay::{
    is_wardrobe_item_document_path, markdown_to_nullable, parse_legacy_wardrobe_json,
    parse_prompt_file, parse_scenario_file, parse_scenario_file_unlogged, parse_vault_metadata,
    parse_vault_physical_prompts, parse_vault_properties, parse_wardrobe_item_file,
    resolve_and_check_component_items, slugify_wardrobe_title, stable_uuid_from_string,
    CharacterScenario, CharacterSystemPrompt, VaultDoc, WardrobeItemFromFile,
};

/// The nine single-file overlay paths (v4 `SINGLE_FILE_OVERLAY_PATHS`), in order.
/// `metadata.json` sits SECOND (v4 `schema.ts:109`), the fact sheet beside the
/// `properties.json` keystone. This constant also drives `character_stats`'
/// `characterFiles` / `characterFilesTotal` (v4's `get.ts` reads the same list),
/// so adding metadata.json lifts the health denominator 8 → 9 in lockstep with v4.
pub const SINGLE_FILE_OVERLAY_PATHS: [&str; 9] = [
    "properties.json",
    "metadata.json",
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

impl VaultUnavailable {
    /// v4 `CharacterVaultUnavailableError`'s `message`, byte-for-byte
    /// (`vault-overlay/schema.ts:155-159`). v5 raises this only for the
    /// missing-`properties.json` case, which is the `detail` v4 passes here.
    pub fn message(&self) -> String {
        format!(
            "Character {} has no usable vault (characterDocumentMountPointId={}): properties.json missing",
            self.character_id, self.mount_id
        )
    }
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
/// two directory listings.
///
/// **A read failure does NOT propagate — it empties the maps** (P4.142,
/// measured at `f6426e196`). v4's comment here (`read-overlay.ts:66-70`, "Does
/// NOT swallow read failures — a store-read exception propagates") is false
/// against v4's own code: both repository methods it calls are fallback
/// `withRawDb([])`s (`doc-mount-documents.repository.ts:142-220`), so a failed
/// read logs the repository's line and answers `[]`, every linked character
/// then lacks its `properties.json` keystone, and the batch overlay DROPS it
/// while the single overlay throws `CharacterVaultUnavailableError`. v5 had
/// ported the false comment AND written the propagation to match it; the reads
/// now go through the `_or_empty` twins. The one `Err` left is v4's strict
/// scope ([`super::fallback::with_strict_repository_failures`]), under which
/// the twins propagate as v4's `safeQuery` rethrows.
pub fn load_vault_file_maps(
    repo: &DocMountDocumentsRepository,
    mount_point_ids: &[String],
) -> Result<VaultFileMaps, DbError> {
    let mut content_by_mount_by_path: HashMap<&'static str, HashMap<String, String>> =
        HashMap::new();
    for path in SINGLE_FILE_OVERLAY_PATHS {
        let pairs = repo.find_many_by_mount_points_and_path_or_empty(mount_point_ids, path)?;
        let mut by_mount = HashMap::new();
        for (mount_id, content) in pairs {
            by_mount.insert(mount_id, content);
        }
        content_by_mount_by_path.insert(path, by_mount);
    }

    let mut prompts_by_mount: HashMap<String, Vec<VaultFolderDoc>> = HashMap::new();
    for doc in
        repo.find_many_by_mount_points_in_folder_or_empty(mount_point_ids, PROMPTS_FOLDER, ".md")?
    {
        prompts_by_mount
            .entry(doc.mount_point_id.clone())
            .or_default()
            .push(doc);
    }
    let mut scenarios_by_mount: HashMap<String, Vec<VaultFolderDoc>> = HashMap::new();
    for doc in
        repo.find_many_by_mount_points_in_folder_or_empty(mount_point_ids, SCENARIOS_FOLDER, ".md")?
    {
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

/// The prompt NAMES and scenario TITLES a vault's `Prompts/` / `Scenarios/`
/// folders yield, parsed exactly as [`hydrate_one`]'s read does
/// ([`parse_prompt_file`] / [`parse_scenario_file`] over the same folder
/// listing). The restore's preserve-arm backfill asks it which archived
/// entries the vault already carries: a vault file's name is NOT its
/// projected path (a renamed prompt, a hand-written scenario titled by its
/// `# heading`), so a path check alone would write a duplicate. Reads inside
/// the strict scope — a failed listing PROPAGATES (the backfill fails into its
/// one warning) rather than reading as an empty folder and duplicating every
/// entry.
pub fn vault_entry_names(
    repo: &DocMountDocumentsRepository,
    mount_point_id: &str,
) -> Result<(HashSet<String>, HashSet<String>), DbError> {
    let ids = [mount_point_id.to_string()];
    super::fallback::with_strict_repository_failures(|| {
        let prompts = repo
            .find_many_by_mount_points_in_folder_or_empty(&ids, PROMPTS_FOLDER, ".md")?
            .iter()
            .filter_map(|d| parse_prompt_file(&vault_doc(d)))
            .map(|p| p.name)
            .collect();
        let scenarios = repo
            .find_many_by_mount_points_in_folder_or_empty(&ids, SCENARIOS_FOLDER, ".md")?
            .iter()
            .filter_map(|d| parse_scenario_file_unlogged(&vault_doc(d)).ok())
            .map(|s| s.title)
            .collect();
        Ok((prompts, scenarios))
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

/// v4 `parseScenarioFilesSorted` (`vault-overlay/parsers.ts:675-697`): the one
/// way a vault's scenario array is built — every doc parsed (unparseable ones
/// dropped), then ordered by `sortScenarios` on the scenario's title, then the
/// FILE name without `.md` (NOT the relative path), then the relative path.
fn parse_scenario_files_sorted(
    docs: &[&VaultFolderDoc],
    character_id: &str,
) -> Vec<CharacterScenario> {
    let parsed: Vec<(&VaultFolderDoc, CharacterScenario)> = docs
        .iter()
        .filter_map(|&d| parse_scenario_file(&vault_doc(d), character_id).map(|s| (d, s)))
        .collect();
    sort_scenarios(parsed, |(d, s)| SortableScenario {
        title: s.title.clone(),
        filename: strip_md_suffix(&d.file_name),
        path: Some(d.relative_path.clone()),
    })
    .into_iter()
    .map(|(_, s)| s)
    .collect()
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
    // No archivedAt short-circuit here, deliberately (§4.2a, v4 `d553f72a`):
    // archiving prunes the vault but keeps every managed-field document, so an
    // archived character hydrates exactly like a live one. Skipping would hand
    // back a hollow row — the very outcome the prune-in-place revision exists
    // to avoid. A *broken* archived vault is a real fault and surfaces as one,
    // like any other.
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

    // properties.json → pronouns, aliases, title, firstMessage, talkativeness,
    // canChooseOutfit.
    if let Some(parsed) = parse_vault_properties(props_raw) {
        let pv = serde_json::to_value(parsed).expect("serialize vault properties");
        for key in [
            "pronouns",
            "aliases",
            "title",
            "firstMessage",
            "talkativeness",
            "canChooseOutfit",
        ] {
            if let Some(v) = pv.get(key) {
                out.insert(key.to_string(), v.clone());
            }
        }
    }

    // metadata.json → the user's freeform fact sheet. Explicitly NOT a keystone:
    // vaults predating the feature have no such file, and the absence is the normal
    // state rather than a broken vault, so it hydrates as `{}` instead of throwing.
    // An unparseable file lands on `{}` too (`parse_vault_metadata` returns None —
    // v4 warns). A stray comma in a fact sheet must not hollow the character.
    // Hydration therefore ALWAYS yields at least `{}` for a vault-linked character,
    // so readers need no null gymnastics.
    let metadata = match maps.content("metadata.json", &mount_id) {
        None => Value::Object(Map::new()),
        Some(raw) => parse_vault_metadata(raw).unwrap_or_else(|| Value::Object(Map::new())),
    };
    out.insert("metadata".to_string(), metadata);

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

    // Prompts/*.md → systemPrompts (sorted, parsed, default-normalized). v4
    // sorts `a.relativePath.localeCompare(b.relativePath)` (`read-overlay.ts:
    // 274-277`) — the ICU4X en-US port, NOT code units: a mixed-case folder
    // (`Prompts/a.md` vs `Prompts/B.md`) orders differently, and with no prompt
    // marked default the order decides which one `normalize_prompt_defaults`
    // promotes (P4.D279's OPEN item, closed by P4.D295 — Δ42).
    let mut prompt_docs = maps.prompts(&mount_id);
    prompt_docs
        .sort_by(|a, b| crate::collation::locale_compare(&a.relative_path, &b.relative_path));
    let mut prompts: Vec<CharacterSystemPrompt> = prompt_docs
        .iter()
        .filter_map(|d| parse_prompt_file(&vault_doc(d)))
        .collect();
    normalize_prompt_defaults(&mut prompts);
    out.insert(
        "systemPrompts".into(),
        serde_json::to_value(&prompts).expect("serialize systemPrompts"),
    );

    // Scenarios/*.md → scenarios, through the one sorted parse so
    // `character.scenarios` and therefore its "first scenario" agree with every
    // picker (`read-overlay.ts:300-305`).
    let scenario_docs = maps.scenarios(&mount_id);
    let scenarios = parse_scenario_files_sorted(
        &scenario_docs,
        obj.get("id").and_then(Value::as_str).unwrap_or(""),
    );
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
    let mut dropped = 0usize;
    for c in &characters {
        match hydrate_one(c, &maps) {
            Ok(h) => out.push(h),
            Err(u) => {
                // v4 `read-overlay.ts:346-353`. The DROP is deliberate — a list
                // read must not fail over one shelved vault — but it was SILENT
                // in v5, which is the divergence P4.D172 closes: bug 131 makes
                // this the hot turn path (every selection site's room map comes
                // through here), so a seat vanishing from the rotation because
                // its vault is on the shelf now says so.
                dropped += 1;
                tracing::error!(
                    characterId = u.character_id,
                    characterDocumentMountPointId = u.mount_id,
                    detail = u.message(),
                    "Dropping character from list — vault unavailable"
                );
            }
        }
    }
    if dropped > 0 {
        tracing::warn!(
            dropped,
            total = characters.len(),
            "applyDocumentStoreOverlay dropped characters with unavailable vaults"
        );
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
    // Archived characters are NOT short-circuited (§4.2a, v4 `d553f72a`):
    // their vault survives the prune and must be overlaid like any other.
    // Pre-revision tombstones (vault deleted, pointer nulled) fall through the
    // linked-vault check unhollowed.
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
/// `vault-overlay/vault-readers.ts:331-402` at `4a38222e6`). Returns
/// `Some({ items })` — from the `Wardrobe/**/*.md` folder layout when present,
/// else the legacy `wardrobe.json` — or `None` when neither yields a usable
/// list.
///
/// **Recursive** (`4a38222e6`): the operator's subfolders
/// (`Wardrobe/Formal/Evening/`) hold items too, and each item's `folder` is
/// derived from its path. `instructions.md` is dressing guidance, never a
/// garment, and `Wardrobe/images/` holds item pictures — the ONE predicate
/// [`is_wardrobe_item_document_path`] says which documents are items. The
/// filter runs BEFORE the length check so a folder holding only the
/// instructions file still falls through to the legacy `wardrobe.json` branch
/// below; anything filtered out is counted in the debug `Wardrobe read skipped
/// non-item documents`.
///
/// The `Wardrobe/**/*.md` path parses each file, builds the in-vault slug/id
/// lookup maps (first-claimer wins a slug; every item is addressable by id),
/// and runs [`resolve_and_check_component_items`] to canonicalize and
/// cycle-check the `componentItems:` refs. Component refs resolve against THIS
/// folder alone: a component in another tier is written as its UUID and
/// survives parsing unresolved (bug 187, `7a8d6dacc`); the per-request
/// wearable pool (`crate::wearable_pool`) resolves it. **The reader never
/// calls back into the repository** — the archetype seeding that lived here
/// before `9ecebf986` is gone with v4's. Files are sorted by `relativePath`
/// under v4's `localeCompare` ([`crate::collation::locale_compare`]) before
/// parsing — NOT the Decision-B code-unit order the `Prompts/` / `Scenarios/`
/// folds above still use (see the why-comment at the sort).
pub fn read_character_vault_wardrobe(
    repo: &DocMountDocumentsRepository,
    mount_point_id: &str,
    character_id: &str,
) -> Result<Option<Value>, DbError> {
    let mount = [mount_point_id.to_string()];
    let all_docs = repo.find_many_by_mount_points_in_folder_opts_or_empty(
        &mount,
        WARDROBE_FOLDER,
        ".md",
        true,
    )?;
    let total = all_docs.len();
    let mut item_docs: Vec<_> = all_docs
        .into_iter()
        .filter(|doc| is_wardrobe_item_document_path(&doc.relative_path))
        .collect();
    if item_docs.len() != total {
        tracing::debug!(
            characterId = %character_id,
            mountPointId = %mount_point_id,
            skipped = total - item_docs.len(),
            "Wardrobe read skipped non-item documents"
        );
    }

    if !item_docs.is_empty() {
        // v4's `a.relativePath.localeCompare(b.relativePath)` (en-US,
        // tertiary), then parse, then drop the files that can't yield a valid
        // item. Decision B (document-store-overlay.md) chose a code-unit sort
        // on the premise that vault filenames are ASCII slugs — false here:
        // wardrobe files are `Wardrobe/<folder>/<sanitized title>.md`, with
        // mixed-case operator folders, so the two orders disagree
        // (`Wardrobe/Coat.md` is FIRST by code unit, after `Wardrobe/attic/
        // Coat.md` by locale). The order is load-bearing: it is the item
        // list's order, and the slug maps below go to the FIRST claimer, so it
        // decides which of two same-titled items a `componentItems:` slug (and
        // a title lookup) lands on.
        item_docs
            .sort_by(|a, b| crate::collation::locale_compare(&a.relative_path, &b.relative_path));
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
                if !slug.is_empty() {
                    tracing::warn!(
                        characterId = %character_id,
                        mountPointId = %mount_point_id,
                        slug = %slug,
                        itemId = %item.id,
                        title = %item.title,
                        "Wardrobe items share a slug; later item only addressable by UUID"
                    );
                }
                continue;
            }
            claimed.insert(slug.clone());
            item_by_slug.insert(slug, i);
        }

        resolve_and_check_component_items(
            &mut items,
            &item_by_slug,
            &item_by_id,
            character_id,
            mount_point_id,
        );

        return Ok(Some(serde_json::json!({
            "items": serde_json::to_value(&items).expect("serialize wardrobe items"),
        })));
    }

    // Folder empty/missing — fall through to legacy wardrobe.json so
    // pre-migration vaults still surface their items.
    let legacy = repo.find_many_by_mount_points_and_path_or_empty(&mount, WARDROBE_JSON_PATH)?;
    let Some((_, content)) = legacy.into_iter().next() else {
        return Ok(None);
    };
    Ok(parse_legacy_wardrobe_json(&content)
        .map(|lw| serde_json::to_value(&lw).expect("serialize legacy wardrobe")))
}

#[cfg(test)]
mod wardrobe_read_tests {
    //! [`read_character_vault_wardrobe`] over a planted mount index: v4's
    //! `localeCompare` file order (and the first-claimer slug it decides), and
    //! the two lines the read logs — each with its silence leg.
    use super::*;
    use crate::test_support::captured_with;
    use rusqlite::Connection;

    const MP: &str = "mp-wardrobe";
    const CHAR: &str = "char-w";

    /// The three tables `find_many_by_mount_points_in_folder_opts` joins, in
    /// the shape its SELECT names, holding one document per `(path, content)`.
    fn mount(files: &[(&str, String)]) -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(
            "CREATE TABLE doc_mount_file_links (id TEXT, mountPointId TEXT, relativePath TEXT, \
               fileName TEXT, fileId TEXT, lastModified TEXT);
             CREATE TABLE doc_mount_documents (fileId TEXT, content TEXT, createdAt TEXT, updatedAt TEXT);
             CREATE TABLE doc_mount_files (id TEXT);",
        )
        .unwrap();
        for (i, (path, content)) in files.iter().enumerate() {
            let file_id = format!("f{i}");
            let file_name = path.rsplit('/').next().unwrap();
            conn.execute(
                "INSERT INTO doc_mount_file_links VALUES (?1, ?2, ?3, ?4, ?1, '2026-01-01T00:00:00.000Z')",
                rusqlite::params![file_id, MP, path, file_name],
            )
            .unwrap();
            conn.execute(
                "INSERT INTO doc_mount_documents VALUES (?1, ?2, '2026-01-01T00:00:00.000Z', '2026-01-01T00:00:00.000Z')",
                rusqlite::params![file_id, content],
            )
            .unwrap();
            conn.execute(
                "INSERT INTO doc_mount_files VALUES (?1)",
                rusqlite::params![file_id],
            )
            .unwrap();
        }
        conn
    }

    /// A `Wardrobe/*.md` item file with a fixed id (so the order is legible).
    fn item_file(n: u32, title: &str, components: &[&str]) -> String {
        let mut out =
            format!("---\nid: 00000000-0000-4000-8000-{n:012}\ntitle: {title}\ntypes:\n  - top\n");
        if !components.is_empty() {
            out.push_str("componentItems:\n");
            for c in components {
                out.push_str(&format!("  - {c}\n"));
            }
        }
        out.push_str("---\n\n");
        out
    }

    fn uuid(n: u32) -> String {
        format!("00000000-0000-4000-8000-{n:012}")
    }

    fn read(conn: &Connection) -> (Vec<Value>, Vec<String>) {
        let (out, lines) = captured_with(|| {
            read_character_vault_wardrobe(&DocMountDocumentsRepository::new(conn), MP, CHAR)
        });
        let items = out.unwrap().unwrap()["items"].as_array().unwrap().clone();
        (items, lines)
    }

    #[test]
    fn the_recursive_read_sorts_by_locale_compare_not_code_units() {
        // Mixed-case operator folders and sanitized-title filenames. v4's
        // order, from Node 24's `localeCompare` (en-US):
        //   attic/Coat, boots, coat tails, Coat, Formal/Coat, Formal/Evening/Gown
        // where a code-unit sort puts every capitalised path first:
        //   Coat, Formal/Coat, Formal/Evening/Gown, attic/Coat, boots, coat tails
        let conn = mount(&[
            ("Wardrobe/Coat.md", item_file(1, "Coat", &[])),
            ("Wardrobe/Formal/Coat.md", item_file(2, "Coat", &[])),
            ("Wardrobe/Formal/Evening/Gown.md", item_file(3, "Gown", &[])),
            ("Wardrobe/attic/Coat.md", item_file(4, "Coat", &[])),
            ("Wardrobe/boots.md", item_file(5, "boots", &[])),
            // The composite names its part by SLUG: the first claimer of
            // `coat` in v4's order is the attic's.
            (
                "Wardrobe/coat tails.md",
                item_file(6, "coat tails", &["coat"]),
            ),
        ]);
        let (items, lines) = read(&conn);
        let ids: Vec<&str> = items.iter().map(|i| i["id"].as_str().unwrap()).collect();
        assert_eq!(
            ids,
            vec![uuid(4), uuid(5), uuid(6), uuid(1), uuid(2), uuid(3)],
            "v4's localeCompare order"
        );
        let tails = items.iter().find(|i| i["id"] == uuid(6)).unwrap();
        assert_eq!(
            tails["componentItemIds"],
            serde_json::json!([uuid(4)]),
            "the slug lands on the first claimer in v4's order"
        );
        // v4's WARN for each later claimer of `coat`, in v4's order.
        let shared: Vec<&String> = lines
            .iter()
            .filter(|l| l.contains("Wardrobe items share a slug"))
            .collect();
        assert_eq!(
            shared,
            vec![
                &format!(
                    "WARN quilltap_core::db::vault_read_overlay Wardrobe items share a slug; later item only addressable by UUID \
                     characterId={CHAR} mountPointId={MP} slug=coat itemId={} title=Coat",
                    uuid(1)
                ),
                &format!(
                    "WARN quilltap_core::db::vault_read_overlay Wardrobe items share a slug; later item only addressable by UUID \
                     characterId={CHAR} mountPointId={MP} slug=coat itemId={} title=Coat",
                    uuid(2)
                ),
            ]
        );
    }

    #[test]
    fn distinct_slugs_log_no_shared_slug_warning() {
        // Two titles that slug to nothing (`§§`, `ßß`) skip the slug map but
        // are not a SHARED slug — v4's `if (slug.length > 0)` keeps them quiet.
        let conn = mount(&[
            ("Wardrobe/Coat.md", item_file(1, "Coat", &[])),
            ("Wardrobe/boots.md", item_file(2, "boots", &[])),
            ("Wardrobe/sect.md", item_file(3, "§§", &[])),
            ("Wardrobe/eszett.md", item_file(4, "ßß", &[])),
        ]);
        let (items, lines) = read(&conn);
        assert_eq!(items.len(), 4);
        assert!(
            !lines.iter().any(|l| l.contains("share a slug")),
            "{lines:#?}"
        );
    }

    #[test]
    fn the_skipped_non_item_documents_line_counts_what_the_predicate_dropped() {
        let conn = mount(&[
            ("Wardrobe/Coat.md", item_file(1, "Coat", &[])),
            ("Wardrobe/instructions.md", "Dress warmly.".to_string()),
            ("Wardrobe/images/Coat.md", item_file(2, "Picture note", &[])),
        ]);
        let (items, lines) = read(&conn);
        assert_eq!(items.len(), 1, "only the garment is an item");
        let skipped: Vec<&String> = lines
            .iter()
            .filter(|l| l.contains("Wardrobe read skipped non-item documents"))
            .collect();
        assert_eq!(
            skipped,
            vec![&format!(
                "DEBUG quilltap_core::db::vault_read_overlay Wardrobe read skipped non-item documents \
                 characterId={CHAR} mountPointId={MP} skipped=2"
            )]
        );

        // Silence: a folder of garments alone skips nothing and says nothing.
        let conn = mount(&[
            ("Wardrobe/Coat.md", item_file(1, "Coat", &[])),
            ("Wardrobe/Formal/Gown.md", item_file(2, "Gown", &[])),
        ]);
        let (items, lines) = read(&conn);
        assert_eq!(items.len(), 2);
        assert!(
            !lines.iter().any(|l| l.contains("skipped non-item")),
            "{lines:#?}"
        );
    }
}
