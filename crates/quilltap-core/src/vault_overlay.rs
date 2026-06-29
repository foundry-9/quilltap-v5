//! Character/wardrobe **vault-overlay** pure leaves — ported from v4's
//! `lib/database/repositories/vault-overlay/parsers.ts`. The vault overlay is the
//! heavier store-backed family (Family B of the document-store slice): a
//! character's identity/manifesto/prompts/scenarios/wardrobe live as files in its
//! official store, projected on read and routed on write. This module collects
//! the *pure* helpers that family needs, ported leaf-first so the stateful
//! overlay (a later slice) can build on verified primitives.
//!
//! So far (leaf-first, all tier-1 differentially verified):
//!   - [`stable_uuid_from_string`] — the deterministic id every folder-enumerated
//!     vault entity derives from `(kind, mountPointId, relativePath)`.
//!   - the wardrobe-component leaves: [`parse_component_items_field`],
//!     [`parse_wardrobe_types_field`], [`detect_component_cycles`].
//!   - the write-projection string leaves: [`slugify_wardrobe_title`],
//!     [`build_slug_by_item_id_map`], [`sanitize_file_name`], [`escape_yaml`],
//!     [`build_system_prompt_file`], [`build_scenario_file`].
//!   - the JSON projection parsers: [`parse_vault_properties`],
//!     [`parse_vault_physical_prompts`] (Zod `safeParse` → fall-back-to-null).
//!   - the legacy migration parser: [`parse_legacy_wardrobe_json`] — validates an
//!     array of full `WardrobeItemSchema` items, reproducing Zod's `z.uuid()` /
//!     `z.iso.datetime()` string formats verbatim.
//!   - the frontmatter READ parsers: [`parse_prompt_file`], [`parse_scenario_file`],
//!     [`parse_wardrobe_item_file`] — built on [`crate::markdown::parse_frontmatter`]
//!     (the hand-rolled YAML reader), with the `# heading` / filename title
//!     fallbacks. The wardrobe parser keeps raw `componentItemIds` for the
//!     overlay's later resolution pass.
//!
//! Two vault decisions are locked (2026-06-29): the wardrobe YAML emitter is
//! hand-rolled (build step 7, the only eemeli/yaml site), and `localeCompare`
//! folder sorts use the code-unit seam + a pinned corpus (no ICU crate).

use std::collections::{HashMap, HashSet};
use std::sync::LazyLock;

use regex::Regex;
use serde::Serialize;
use serde_json::{Map, Value};
use sha2::{Digest, Sha256};

/// Build a stable, RFC-4122 **v8**-style UUID from a SHA-256 digest of `source`
/// — v4 `stableUuidFromString` (`vault-overlay/parsers.ts:154`). v8 is the
/// "custom" version: the exact version byte is not load-bearing, only that the
/// string parses as a UUID and round-trips to itself for the same input. The
/// vault derives every folder-enumerated entity's id this way
/// (`prompt:<mp>:<path>`, `scenario:<mp>:<path>`, `wardrobe-item:<mp>:<path>`),
/// so chat references to those ids are stable across rescans.
///
/// Exactly v4's bytes: SHA-256 over the UTF-8 bytes of `source` → first 16 bytes
/// → set the version nibble of byte 6 to 8 (`(b & 0x0f) | 0x80`) and the variant
/// of byte 8 to RFC-4122 (`(b & 0x3f) | 0x80`) → lowercase hex, hyphenated
/// 8-4-4-4-12. (Node's `hash.update(source)` defaults to UTF-8, matching
/// `source.as_bytes()` — so non-ASCII sources agree too; there is no case
/// mapping here, unlike the `toLowerCase`/`localeCompare` seams.)
pub fn stable_uuid_from_string(source: &str) -> String {
    let hash = Sha256::digest(source.as_bytes());
    let mut bytes = [0u8; 16];
    bytes.copy_from_slice(&hash[0..16]);
    // Version 8 (custom) in byte 6; RFC-4122 variant in byte 8.
    bytes[6] = (bytes[6] & 0x0f) | 0x80;
    bytes[8] = (bytes[8] & 0x3f) | 0x80;
    let hex = hex::encode(bytes);
    format!(
        "{}-{}-{}-{}-{}",
        &hex[0..8],
        &hex[8..12],
        &hex[12..16],
        &hex[16..20],
        &hex[20..32]
    )
}

/// The four wardrobe item types (v4 `WardrobeItemTypeEnum`), in declaration order.
pub const WARDROBE_ITEM_TYPES: [&str; 4] = ["top", "bottom", "footwear", "accessories"];

// ── JSON projection parsers (`properties.json` / `physical-prompts.json`) ─────
//
// These reproduce v4's `safeParse`-then-fall-back-to-null semantics
// (`vault-overlay/parsers.ts`): parse the file's JSON, validate against the
// vault schema, and return the typed value — or `None` on a JSON parse error OR
// any schema violation (the read overlay then falls back to the DB values). Zod
// `z.object` STRIPS unknown keys (default, not strict), so extras are dropped,
// not rejected. A `.nullable()` field is REQUIRED present (its key must exist;
// the value may be `null`); a `.nullable().optional()` field may be absent.

/// The `pronouns` sub-object (v4 `PronounsSchema`): three required strings, each
/// 1–20 UTF-16 code units.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Pronouns {
    pub subject: String,
    pub object: String,
    pub possessive: String,
}

/// `properties.json`'s validated shape (v4 `CharacterVaultPropertiesSchema`). All
/// five keys are required; `pronouns`/`title`/`firstMessage` are nullable
/// (serialized as `null` when unset, matching Zod's required-nullable output).
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct CharacterVaultProperties {
    pub pronouns: Option<Pronouns>,
    pub aliases: Vec<String>,
    pub title: Option<String>,
    #[serde(rename = "firstMessage")]
    pub first_message: Option<String>,
    pub talkativeness: f64,
}

/// `physical-prompts.json`'s validated shape (v4 `CharacterVaultPhysicalPromptsSchema`).
/// `headAndShoulders` is optional (absent → omitted); the four tiers are required
/// and nullable (serialized as `null` when unset).
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct CharacterVaultPhysicalPrompts {
    #[serde(rename = "headAndShoulders", skip_serializing_if = "Option::is_none")]
    pub head_and_shoulders: Option<String>,
    pub short: Option<String>,
    pub medium: Option<String>,
    pub long: Option<String>,
    pub complete: Option<String>,
}

/// A required key whose value must be a string or `null` (`z.string().nullable()`).
/// `Some(inner)` = valid (`inner` is the `null`/string value); `None` = the value
/// was neither a string nor `null` (schema violation).
fn nullable_string(v: &Value) -> Option<Option<String>> {
    match v {
        Value::Null => Some(None),
        Value::String(s) => Some(Some(s.clone())),
        _ => None,
    }
}

/// Validate the `pronouns` object (all three fields required strings, 1–20 UTF-16
/// code units; unknown keys stripped). `None` on any violation.
fn parse_pronouns(p: &Map<String, Value>) -> Option<Pronouns> {
    let field = |key: &str| -> Option<String> {
        let s = p.get(key)?.as_str()?;
        let len = crate::jsstr::utf16_len(s);
        if (1..=20).contains(&len) {
            Some(s.to_string())
        } else {
            None
        }
    };
    Some(Pronouns {
        subject: field("subject")?,
        object: field("object")?,
        possessive: field("possessive")?,
    })
}

/// Parse + validate `properties.json` (v4 `parseVaultProperties`). `None` on a
/// JSON parse error or any schema violation (the read overlay falls back to DB
/// values). `characterId` is v4's logging-only arg — not needed here.
pub fn parse_vault_properties(raw: &str) -> Option<CharacterVaultProperties> {
    let json: Value = serde_json::from_str(raw).ok()?;
    let obj = json.as_object()?;

    // pronouns: required key, null or a valid Pronouns object.
    let pronouns = match obj.get("pronouns")? {
        Value::Null => None,
        Value::Object(p) => Some(parse_pronouns(p)?),
        _ => return None,
    };

    // aliases: required array of strings.
    let mut aliases = Vec::new();
    for a in obj.get("aliases")?.as_array()? {
        aliases.push(a.as_str()?.to_string());
    }

    let title = nullable_string(obj.get("title")?)?;
    let first_message = nullable_string(obj.get("firstMessage")?)?;

    // talkativeness: required number, 0.1 ≤ t ≤ 1.0 (inclusive).
    let talkativeness = obj.get("talkativeness")?.as_f64()?;
    if !(0.1..=1.0).contains(&talkativeness) {
        return None;
    }

    Some(CharacterVaultProperties {
        pronouns,
        aliases,
        title,
        first_message,
        talkativeness,
    })
}

/// Parse + validate `physical-prompts.json` (v4 `parseVaultPhysicalPrompts`).
/// `None` on a JSON parse error or any schema violation.
///
/// NB `headAndShoulders` present-as-`null` (vs absent) is the null-vs-absent
/// open-JSON seam — the corpus keeps it absent or a string, and present-`null`
/// is mapped to omitted here (the one unexercised divergence). The four required
/// tiers serialize `null` when unset, matching Zod.
pub fn parse_vault_physical_prompts(raw: &str) -> Option<CharacterVaultPhysicalPrompts> {
    let json: Value = serde_json::from_str(raw).ok()?;
    let obj = json.as_object()?;

    let head_and_shoulders = match obj.get("headAndShoulders") {
        None => None,              // optional → absent is fine
        Some(Value::Null) => None, // present-null → omitted (tracked seam)
        Some(Value::String(s)) => Some(s.clone()),
        Some(_) => return None, // wrong type → violation
    };

    Some(CharacterVaultPhysicalPrompts {
        head_and_shoulders,
        short: nullable_string(obj.get("short")?)?,
        medium: nullable_string(obj.get("medium")?)?,
        long: nullable_string(obj.get("long")?)?,
        complete: nullable_string(obj.get("complete")?)?,
    })
}

// ── Legacy `wardrobe.json` parser (`parseLegacyWardrobeJson`) ─────────────────
//
// The migration path that still reads the old single-file wardrobe payload onto
// the folder layout. Unlike the two JSON projection parsers above (which validate
// hand-written `z.object` shapes), this one validates an array of full
// `WardrobeItemSchema` items, so it has to reproduce Zod's `z.uuid()` and
// `z.iso.datetime()` string formats exactly — any single bad item fails the whole
// array (`z.array(WardrobeItemSchema)`) and the parser returns `None` (the read
// overlay then falls back to DB values).

/// Zod 4.4 `z.uuid()` — version nibble `[1-8]`, RFC-4122 variant `[89abAB]`, plus
/// the two well-known all-zero / all-`f` sentinels. (`stableUuidFromString`'s v8 /
/// `0x8.` variant bytes satisfy this, so vault-minted ids round-trip.) Sourced
/// verbatim from the live Zod schema's compiled `pattern`.
static UUID_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"^([0-9a-fA-F]{8}-[0-9a-fA-F]{4}-[1-8][0-9a-fA-F]{3}-[89abAB][0-9a-fA-F]{3}-[0-9a-fA-F]{12}|00000000-0000-0000-0000-000000000000|ffffffff-ffff-ffff-ffff-ffffffffffff)$",
    )
    .unwrap()
});

/// Zod 4.4 `z.iso.datetime()` (default: no offset, no `local`, unbounded
/// fractional precision) — a real ISO date validator with leap-year arithmetic
/// and a `Z`-only zone. Sourced verbatim from the live Zod schema's compiled
/// `pattern`, with JS `\d` rewritten to ASCII `[0-9]` (the Rust `regex` `\d` is
/// Unicode-aware; JS's is ASCII). The `$` anchor rejects a trailing newline in
/// both engines.
static ISO_DATETIME_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"^(?:(?:[0-9][0-9][2468][048]|[0-9][0-9][13579][26]|[0-9][0-9]0[48]|[02468][048]00|[13579][26]00)-02-29|[0-9]{4}-(?:(?:0[13578]|1[02])-(?:0[1-9]|[12][0-9]|3[01])|(?:0[469]|11)-(?:0[1-9]|[12][0-9]|30)|(?:02)-(?:0[1-9]|1[0-9]|2[0-8])))T(?:(?:[01][0-9]|2[0-3]):[0-5][0-9](?::[0-5][0-9](?:\.[0-9]+)?)?(?:Z))$",
    )
    .unwrap()
});

fn is_uuid(s: &str) -> bool {
    UUID_RE.is_match(s)
}

fn is_iso_datetime(s: &str) -> bool {
    ISO_DATETIME_RE.is_match(s)
}

/// A `z.string().nullable().optional()` field. Outer `None` = schema violation
/// (a present non-string, non-null value); `Some(inner)` is the field value where
/// `inner` is `None` (absent → omitted) / `Some(None)` (`null`) / `Some(Some(s))`
/// (string).
#[allow(clippy::type_complexity)]
fn opt_nullable_string(v: Option<&Value>) -> Option<Option<Option<String>>> {
    match v {
        None => Some(None),                                    // absent → field omitted
        Some(Value::Null) => Some(Some(None)),                 // null
        Some(Value::String(s)) => Some(Some(Some(s.clone()))), // string
        Some(_) => None,                                       // wrong type → violation
    }
}

/// A `z.uuid().nullable().optional()` field. Like [`opt_nullable_string`] but a
/// present non-null value must also pass the UUID format.
#[allow(clippy::type_complexity)]
fn opt_nullable_uuid(v: Option<&Value>) -> Option<Option<Option<String>>> {
    match v {
        None => Some(None),
        Some(Value::Null) => Some(Some(None)),
        Some(Value::String(s)) if is_uuid(s) => Some(Some(Some(s.clone()))),
        Some(_) => None,
    }
}

/// A validated `WardrobeItem` (v4 `WardrobeItemSchema` z.infer output). Fields in
/// schema-declaration order — Zod emits its output object in shape order
/// regardless of input key order — with the `.default()` keys
/// (`componentItemIds`/`isDefault`/`replace`) always materialized and the
/// `.nullable().optional()` keys omitted when absent (`skip_serializing_if`).
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct WardrobeItem {
    pub id: String,
    #[serde(rename = "characterId", skip_serializing_if = "Option::is_none")]
    pub character_id: Option<Option<String>>,
    pub title: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<Option<String>>,
    #[serde(rename = "imagePrompt", skip_serializing_if = "Option::is_none")]
    pub image_prompt: Option<Option<String>>,
    pub types: Vec<String>,
    #[serde(rename = "componentItemIds")]
    pub component_item_ids: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub appropriateness: Option<Option<String>>,
    #[serde(rename = "isDefault")]
    pub is_default: bool,
    pub replace: bool,
    #[serde(
        rename = "migratedFromClothingRecordId",
        skip_serializing_if = "Option::is_none"
    )]
    pub migrated_from_clothing_record_id: Option<Option<String>>,
    #[serde(rename = "archivedAt", skip_serializing_if = "Option::is_none")]
    pub archived_at: Option<Option<String>>,
    #[serde(rename = "createdAt")]
    pub created_at: String,
    #[serde(rename = "updatedAt")]
    pub updated_at: String,
}

/// The result of [`parse_legacy_wardrobe_json`] — `{ items }` only. v4 deliberately
/// drops the legacy `outfit`/`presets` from the output (the DB-side migration owns
/// presets; `outfit` was never consumed), but still *validates* a present `outfit`
/// so a malformed one fails the whole parse.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct LegacyVaultWardrobe {
    pub items: Vec<WardrobeItem>,
}

/// Validate one `WardrobeItemSchema` element. `None` on any violation.
fn validate_wardrobe_item(v: &Value) -> Option<WardrobeItem> {
    let obj = v.as_object()?; // non-object → violation

    // id: required UUID string.
    let id = obj.get("id")?.as_str()?;
    if !is_uuid(id) {
        return None;
    }

    // characterId: uuid | null | absent.
    let character_id = opt_nullable_uuid(obj.get("characterId"))?;

    // title: required string, `.min(1)`.
    let title = obj.get("title")?.as_str()?;
    if title.is_empty() {
        return None;
    }

    let description = opt_nullable_string(obj.get("description"))?;
    let image_prompt = opt_nullable_string(obj.get("imagePrompt"))?;

    // types: required non-empty array of enum strings.
    let types_v = obj.get("types")?.as_array()?;
    if types_v.is_empty() {
        return None;
    }
    let mut types = Vec::with_capacity(types_v.len());
    for t in types_v {
        let s = t.as_str()?;
        if !WARDROBE_ITEM_TYPES.contains(&s) {
            return None;
        }
        types.push(s.to_string());
    }

    // componentItemIds: `.default([])`; else array of UUID strings.
    let component_item_ids = match obj.get("componentItemIds") {
        None => Vec::new(),
        Some(arr) => {
            let arr = arr.as_array()?;
            let mut out = Vec::with_capacity(arr.len());
            for c in arr {
                let s = c.as_str()?;
                if !is_uuid(s) {
                    return None;
                }
                out.push(s.to_string());
            }
            out
        }
    };

    let appropriateness = opt_nullable_string(obj.get("appropriateness"))?;

    // isDefault / replace: `.default(false)`; else a boolean (null/other → fail).
    let is_default = match obj.get("isDefault") {
        None => false,
        Some(b) => b.as_bool()?,
    };
    let replace = match obj.get("replace") {
        None => false,
        Some(b) => b.as_bool()?,
    };

    let migrated_from_clothing_record_id =
        opt_nullable_uuid(obj.get("migratedFromClothingRecordId"))?;

    // archivedAt: ISO datetime | null | absent.
    let archived_at = match obj.get("archivedAt") {
        None => None,
        Some(Value::Null) => Some(None),
        Some(Value::String(s)) if is_iso_datetime(s) => Some(Some(s.clone())),
        Some(_) => return None,
    };

    // createdAt / updatedAt: required ISO datetime strings (the transform passes a
    // string through unchanged, so we store as-is).
    let created_at = obj.get("createdAt")?.as_str()?;
    if !is_iso_datetime(created_at) {
        return None;
    }
    let updated_at = obj.get("updatedAt")?.as_str()?;
    if !is_iso_datetime(updated_at) {
        return None;
    }

    Some(WardrobeItem {
        id: id.to_string(),
        character_id,
        title: title.to_string(),
        description,
        image_prompt,
        types,
        component_item_ids,
        appropriateness,
        is_default,
        replace,
        migrated_from_clothing_record_id,
        archived_at,
        created_at: created_at.to_string(),
        updated_at: updated_at.to_string(),
    })
}

/// Validate a present `outfit` block (each of top/bottom/footwear/accessories is
/// `z.string().nullable().optional()`; unknown keys stripped; `outfit` itself is
/// not nullable, so `null` fails). Returns `Some(())` if valid (the value is then
/// discarded), `None` on any violation.
fn validate_outfit(v: &Value) -> Option<()> {
    let obj = v.as_object()?; // non-object (incl. null) → violation
    for key in ["top", "bottom", "footwear", "accessories"] {
        // Each key is optional: absent is fine; present must be string|null.
        if let Some(field) = obj.get(key) {
            opt_nullable_string(Some(field))?;
        }
    }
    Some(())
}

/// Parse the legacy `wardrobe.json` payload — v4 `parseLegacyWardrobeJson`
/// (`vault-overlay/parsers.ts:110`). `None` on a JSON parse error or any schema
/// violation in `LegacyVaultWardrobeJsonSchema = { items: WardrobeItem[],
/// outfit? }`. The legacy `presets` array (and any other unknown root key) is
/// stripped; `outfit` is validated then dropped; only `{ items }` is returned.
/// `characterId` is v4's logging-only arg.
pub fn parse_legacy_wardrobe_json(raw: &str) -> Option<LegacyVaultWardrobe> {
    let json: Value = serde_json::from_str(raw).ok()?;
    let obj = json.as_object()?; // non-object root → violation

    // items: required array; every element a valid WardrobeItem.
    let items_v = obj.get("items")?.as_array()?;
    let mut items = Vec::with_capacity(items_v.len());
    for item in items_v {
        items.push(validate_wardrobe_item(item)?);
    }

    // outfit: optional, but validated when present (a malformed one fails all).
    if let Some(outfit) = obj.get("outfit") {
        validate_outfit(outfit)?;
    }

    Some(LegacyVaultWardrobe { items })
}

/// Kebab-case slug from a wardrobe item title — v4 `slugifyWardrobeTitle`
/// (`character-vault.ts:226`): `toLowerCase` → JS-`trim` → collapse every run of
/// non-`[a-z0-9]` to a single `-` → strip a leading/trailing `-`. The
/// `[^a-z0-9]` filter neutralizes JS-vs-Rust case-mapping divergence (a non-ASCII
/// char never lowercases INTO `[a-z]` in one engine but not the other), so this
/// is collation/case-safe per the locked vault decision — no ICU needed.
pub fn slugify_wardrobe_title(title: &str) -> String {
    let lowered = title.to_lowercase();
    let trimmed = crate::jsstr::js_trim(&lowered);
    let mut out = String::with_capacity(trimmed.len());
    let mut prev_dash = false;
    for c in trimmed.chars() {
        if c.is_ascii_lowercase() || c.is_ascii_digit() {
            out.push(c);
            prev_dash = false;
        } else if !prev_dash {
            out.push('-');
            prev_dash = true;
        }
    }
    out.trim_matches('-').to_string()
}

/// Build the `(itemId → slug)` list a wardrobe write uses to translate
/// `componentItemIds` UUIDs to slugs — v4 `buildSlugByItemIdMap`
/// (`character-vault.ts:240`). First item that slugifies to a given slug claims
/// it; empty slugs and later collisions are skipped. `items` is `(id, title)` in
/// the caller's order; the result preserves first-seen order (the analogue of v4's
/// insertion-ordered `Map`).
pub fn build_slug_by_item_id_map(items: &[(String, String)]) -> Vec<(String, String)> {
    let mut out = Vec::new();
    let mut claimed: HashSet<String> = HashSet::new();
    for (id, title) in items {
        let slug = slugify_wardrobe_title(title);
        if slug.is_empty() || claimed.contains(&slug) {
            continue;
        }
        claimed.insert(slug.clone());
        out.push((id.clone(), slug));
    }
    out
}

/// Sanitize a string into a safe file name — v4 `sanitizeFileName`
/// (`character-vault.ts:203`): replace each of `\ / : * ? " < > |` with `_`,
/// collapse every JS-whitespace run to a single space, JS-`trim`, take the first
/// 100 UTF-16 code units, falling back to `"untitled"` when the result is empty.
pub fn sanitize_file_name(name: &str) -> String {
    let replaced: String = name
        .chars()
        .map(|c| match c {
            '\\' | '/' | ':' | '*' | '?' | '"' | '<' | '>' | '|' => '_',
            other => other,
        })
        .collect();
    // Collapse runs of JS-whitespace to a single ' '.
    let mut collapsed = String::with_capacity(replaced.len());
    let mut prev_ws = false;
    for c in replaced.chars() {
        if crate::jsstr::is_js_ws(c) {
            if !prev_ws {
                collapsed.push(' ');
                prev_ws = true;
            }
        } else {
            collapsed.push(c);
            prev_ws = false;
        }
    }
    let trimmed = crate::jsstr::js_trim(&collapsed);
    let sliced = crate::jsstr::utf16_truncate(trimmed, 100);
    if sliced.is_empty() {
        "untitled".to_string()
    } else {
        sliced
    }
}

/// Escape a value for hand-built YAML frontmatter — v4's private `escapeYaml`
/// (`character-vault.ts:211`): if the value contains any of `: # " ' \n`, emit it
/// as a `JSON.stringify`'d (double-quoted, JSON-escaped) string; otherwise emit it
/// plain. `serde_json::to_string` of a string reproduces `JSON.stringify` exactly
/// (same escape set, no `/` or non-ASCII escaping). Used by
/// [`build_system_prompt_file`]; NOT the eemeli/yaml path (that is the wardrobe
/// write slice).
pub fn escape_yaml(value: &str) -> String {
    if value.contains([':', '#', '"', '\'', '\n']) {
        serde_json::to_string(value).expect("string always serializes")
    } else {
        value.to_string()
    }
}

/// Build a `Prompts/*.md` system-prompt file — v4 `buildSystemPromptFile`
/// (`character-vault.ts:192`): YAML frontmatter (`name`, plus `isDefault: true`
/// only when set) over the prompt body. The `name` value goes through
/// [`escape_yaml`]; this is a hand-built frontmatter string, NOT eemeli/yaml.
pub fn build_system_prompt_file(name: &str, is_default: bool, content: &str) -> String {
    let frontmatter = if is_default {
        format!("---\nname: {}\nisDefault: true\n---\n\n", escape_yaml(name))
    } else {
        format!("---\nname: {}\n---\n\n", escape_yaml(name))
    };
    format!("{frontmatter}{content}")
}

/// Build a `Scenarios/*.md` file — v4 `buildScenarioFile` (`character-vault.ts:199`):
/// a plain `# title` heading + the body, NO frontmatter at all.
pub fn build_scenario_file(title: &str, content: &str) -> String {
    format!("# {title}\n\n{content}")
}

/// Coerce a raw `componentItems:` value into a clean `Vec<String>` — v4
/// `parseComponentItemsField` (`vault-overlay/parsers.ts:358`). Non-arrays (incl.
/// `null`) yield `[]`; within an array, non-string and empty/whitespace-only
/// entries are dropped and the survivors are trimmed. Order is preserved; there
/// is no dedup here (that happens later, during id resolution). `raw` is the
/// parsed frontmatter value (`serde_json::Value`, the analogue of v4's `unknown`).
pub fn parse_component_items_field(raw: &Value) -> Vec<String> {
    let Some(arr) = raw.as_array() else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for v in arr {
        let Some(s) = v.as_str() else { continue };
        let trimmed = s.trim();
        if trimmed.is_empty() {
            continue;
        }
        out.push(trimmed.to_string());
    }
    out
}

/// Validate a raw `types:` value into the wardrobe-type list — v4
/// `parseWardrobeTypesField` (`vault-overlay/parsers.ts:371`). Returns `None`
/// (the "fall back to default" signal) for a non-array, an empty array, or ANY
/// element that is not a string or not one of [`WARDROBE_ITEM_TYPES`] — the
/// validation is all-or-nothing. Valid input is de-duplicated preserving
/// first-seen order. Returns the type strings (the enum members).
pub fn parse_wardrobe_types_field(raw: &Value) -> Option<Vec<String>> {
    let arr = raw.as_array()?;
    if arr.is_empty() {
        return None;
    }
    let allowed: HashSet<&str> = WARDROBE_ITEM_TYPES.into_iter().collect();
    let mut out = Vec::new();
    let mut seen = HashSet::new();
    for v in arr {
        let s = v.as_str()?; // non-string → whole field invalid
        if !allowed.contains(s) {
            return None; // unknown type → whole field invalid
        }
        if seen.insert(s.to_string()) {
            out.push(s.to_string());
        }
    }
    if out.is_empty() {
        None
    } else {
        Some(out)
    }
}

/// Save-time component-cycle check — v4 `detectComponentCycles`
/// (`wardrobe/expand-composites.ts:110`). Returns the cycle paths that would
/// result if `component_item_ids` were saved as the components of `self_id`; an
/// empty result means safe. `items_by_id` maps each known item id to its declared
/// `componentItemIds` (the only field the walk reads). A direct self-reference
/// yields `[self_id, self_id]`; an indirect cycle yields the path back to a
/// repeated node (the offending id appended).
pub fn detect_component_cycles(
    self_id: &str,
    component_item_ids: &[String],
    items_by_id: &HashMap<String, Vec<String>>,
) -> Vec<Vec<String>> {
    let mut cycles: Vec<Vec<String>> = Vec::new();
    for child_id in component_item_ids {
        if child_id == self_id {
            cycles.push(vec![self_id.to_string(), self_id.to_string()]);
            continue;
        }
        walk_cycles(
            self_id,
            child_id,
            &[self_id.to_string(), child_id.clone()],
            items_by_id,
            &mut cycles,
        );
    }
    cycles
}

/// The recursive half of [`detect_component_cycles`], mirroring v4's `walk`: for
/// each grandchild, a back-edge to `self_id` or to a node already on `path` is a
/// cycle (recorded as `path + grand`); otherwise descend.
fn walk_cycles(
    self_id: &str,
    id: &str,
    path: &[String],
    items_by_id: &HashMap<String, Vec<String>>,
    cycles: &mut Vec<Vec<String>>,
) {
    let Some(children) = items_by_id.get(id) else {
        return;
    };
    for grand in children {
        if grand == self_id || path.iter().any(|p| p == grand) {
            let mut cycle = path.to_vec();
            cycle.push(grand.clone());
            cycles.push(cycle);
            continue;
        }
        let mut next = path.to_vec();
        next.push(grand.clone());
        walk_cycles(self_id, grand, &next, items_by_id, cycles);
    }
}

// ── Frontmatter READ parsers (`parsePromptFile` / `parseScenarioFile`) ────────
//
// The vault read overlay's per-file parsers, built on the hand-rolled frontmatter
// reader ([`crate::markdown::parse_frontmatter`]). Each turns a vault markdown
// file (a [`VaultDoc`]) into a `CharacterSystemPrompt` / `CharacterScenario`, or
// `None` when the file can't yield a valid value — the overlay then falls back to
// the DB value for that one file rather than failing the whole list. The objects
// are built directly (not via Zod), so the JS `.trim()` / `.slice(0, n)` are
// reproduced with the `jsstr` UTF-16 primitives.

/// The subset of v4's `DocMountDocumentWithLink` the per-file parsers read.
#[derive(Debug, Clone, Copy)]
pub struct VaultDoc<'a> {
    pub content: &'a str,
    pub mount_point_id: &'a str,
    pub relative_path: &'a str,
    pub file_name: &'a str,
    pub created_at: &'a str,
    pub updated_at: &'a str,
}

/// A prompt-variant entry (v4 `CharacterSystemPrompt`).
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct CharacterSystemPrompt {
    pub id: String,
    pub name: String,
    pub content: String,
    #[serde(rename = "isDefault")]
    pub is_default: bool,
    #[serde(rename = "createdAt")]
    pub created_at: String,
    #[serde(rename = "updatedAt")]
    pub updated_at: String,
}

/// A scenario entry (v4 `CharacterScenario`). `description` is present only when
/// the frontmatter supplied a non-empty one (matching v4's conditional spread).
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct CharacterScenario {
    pub id: String,
    pub title: String,
    pub content: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(rename = "createdAt")]
    pub created_at: String,
    #[serde(rename = "updatedAt")]
    pub updated_at: String,
}

/// The leading-`# heading` matcher (v4 `/^#\s+(.+)$/` per line). `\s` is the JS
/// whitespace set; `.+` requires ≥1 trailing char (captured, then JS-trimmed).
static HEADING_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(&format!(r"^#{}+(.+)$", crate::jsstr::JS_WS_CLASS)).unwrap());

/// First `# heading` in `lines` → `(line_index, trimmed_title)`.
fn first_heading(lines: &[&str]) -> Option<(usize, String)> {
    for (i, line) in lines.iter().enumerate() {
        if let Some(caps) = HEADING_RE.captures(line) {
            let title = crate::jsstr::js_trim(caps.get(1).unwrap().as_str()).to_string();
            return Some((i, title));
        }
    }
    None
}

/// Strip a trailing `.md` (case-insensitive) — v4 `fileName.replace(/\.md$/i, '')`.
fn strip_md_ext(name: &str) -> String {
    if name.to_ascii_lowercase().ends_with(".md") {
        name[..name.len() - 3].to_string()
    } else {
        name.to_string()
    }
}

/// Parse a `Prompts/*.md` file (v4 `parsePromptFile`). Requires frontmatter with
/// a non-empty `name` and a non-empty body; `isDefault` is `=== true`. `None`
/// (skip) otherwise. The body is the content after the frontmatter, `trimStart`ed.
pub fn parse_prompt_file(doc: &VaultDoc) -> Option<CharacterSystemPrompt> {
    let fm = crate::markdown::parse_frontmatter(doc.content);
    let data = fm.data.as_ref()?; // no parseable frontmatter → skip
    let name = data.get("name").and_then(Value::as_str)?; // missing / non-string → skip
    if crate::jsstr::js_trim(name).is_empty() {
        return None; // empty after trim → skip
    }
    let is_default = data.get("isDefault") == Some(&Value::Bool(true));
    let body = crate::jsstr::js_trim_start(crate::markdown::body_after(doc.content, &fm));
    if body.is_empty() {
        return None; // empty body → skip
    }
    Some(CharacterSystemPrompt {
        id: stable_uuid_from_string(&format!(
            "prompt:{}:{}",
            doc.mount_point_id, doc.relative_path
        )),
        name: crate::jsstr::utf16_truncate(crate::jsstr::js_trim(name), 100),
        content: body.to_string(),
        is_default,
        created_at: doc.created_at.to_string(),
        updated_at: doc.updated_at.to_string(),
    })
}

/// Parse a `Scenarios/*.md` file (v4 `parseScenarioFile`). Title resolution:
/// frontmatter `name` → first `# heading` → filename-without-`.md`. A non-empty
/// frontmatter `description` (capped at 500) is carried when present. `None`
/// (skip) when there is no usable title or the body is empty.
pub fn parse_scenario_file(doc: &VaultDoc) -> Option<CharacterScenario> {
    let content = doc.content;
    let fm = crate::markdown::parse_frontmatter(content);

    // Title: frontmatter `name` wins; description (if non-empty) is carried.
    let mut title: Option<String> = None;
    let mut frontmatter_description: Option<String> = None;
    if let Some(data) = fm.data.as_ref() {
        if let Some(name) = data.get("name").and_then(Value::as_str) {
            let t = crate::jsstr::js_trim(name);
            if !t.is_empty() {
                title = Some(t.to_string());
            }
        }
        if let Some(desc) = data.get("description").and_then(Value::as_str) {
            let d = crate::jsstr::js_trim(desc);
            if !d.is_empty() {
                frontmatter_description = Some(crate::jsstr::utf16_truncate(d, 500));
            }
        }
    }

    let after = crate::markdown::body_after(content, &fm);
    let lines: Vec<&str> = after.split('\n').collect();
    let mut title_line_index: Option<usize> = None;

    if title.is_none() {
        if let Some((idx, t)) = first_heading(&lines) {
            title_line_index = Some(idx);
            title = Some(t);
        }
    }

    let title = match title {
        Some(t) => t,
        None => {
            // Filename without extension, trimmed, capped at 200.
            let fname = strip_md_ext(doc.file_name);
            let t = crate::jsstr::utf16_truncate(crate::jsstr::js_trim(&fname), 200);
            if t.is_empty() {
                return None; // no usable title → skip
            }
            t
        }
    };

    // Body: drop the heading line when one was used as the title.
    let body = match title_line_index {
        Some(idx) => crate::jsstr::js_trim(&lines[idx + 1..].join("\n")).to_string(),
        None => crate::jsstr::js_trim(after).to_string(),
    };
    if body.is_empty() {
        return None; // empty body → skip
    }

    Some(CharacterScenario {
        id: stable_uuid_from_string(&format!(
            "scenario:{}:{}",
            doc.mount_point_id, doc.relative_path
        )),
        title: crate::jsstr::utf16_truncate(&title, 200),
        content: body,
        description: frontmatter_description,
        created_at: doc.created_at.to_string(),
        updated_at: doc.updated_at.to_string(),
    })
}

/// A wardrobe item parsed from a `Wardrobe/*.md` file (v4 `parseWardrobeItemFile`
/// output). Built directly — distinct from the Zod-validated [`WardrobeItem`]: the
/// nullable fields are ALWAYS present (`null` or value), `characterId` is the
/// passed id (never null), and `componentItemIds` holds the **raw** refs
/// (slug/UUID strings) the overlay resolves in a later pass. Fields are in v4's
/// object-literal order.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct WardrobeItemFromFile {
    pub id: String,
    #[serde(rename = "characterId")]
    pub character_id: String,
    pub title: String,
    pub description: Option<String>,
    #[serde(rename = "imagePrompt")]
    pub image_prompt: Option<String>,
    pub types: Vec<String>,
    pub appropriateness: Option<String>,
    #[serde(rename = "isDefault")]
    pub is_default: bool,
    pub replace: bool,
    #[serde(rename = "componentItemIds")]
    pub component_item_ids: Vec<String>,
    #[serde(rename = "migratedFromClothingRecordId")]
    pub migrated_from_clothing_record_id: Option<String>,
    #[serde(rename = "archivedAt")]
    pub archived_at: Option<String>,
    #[serde(rename = "createdAt")]
    pub created_at: String,
    #[serde(rename = "updatedAt")]
    pub updated_at: String,
}

/// A frontmatter field accessor — `None` when there's no object data or the key
/// is absent.
fn fm_get<'a>(fm: &'a crate::markdown::ParsedFrontmatter, key: &str) -> Option<&'a Value> {
    fm.data.as_ref().and_then(|o| o.get(key))
}

/// v4's id sanity check `/^[0-9a-f-]{36}$/i` — exactly 36 chars, each a hex digit
/// (either case) or `-`.
fn is_wardrobe_id_shaped(s: &str) -> bool {
    s.chars().count() == 36 && s.chars().all(|c| c.is_ascii_hexdigit() || c == '-')
}

/// Parse a `Wardrobe/<title>.md` file (v4 `parseWardrobeItemFile`). Title
/// resolution mirrors the scenario parser (frontmatter `title` → first
/// `# heading` → filename-without-`.md`); a valid `types` list is required (else
/// skip). `id` is taken from frontmatter when it is 36-char id-shaped, else
/// derived via `stableUuidFromString`. `componentItemIds` keeps the raw author
/// refs for the overlay's later resolution pass. `None` (skip) when there is no
/// usable title or no valid `types`.
pub fn parse_wardrobe_item_file(
    doc: &VaultDoc,
    character_id: &str,
) -> Option<WardrobeItemFromFile> {
    let content = doc.content;
    let fm = crate::markdown::parse_frontmatter(content);

    // Title: frontmatter `title` → first `# heading` → filename-without-`.md`.
    let mut title: Option<String> = None;
    if let Some(t) = fm_get(&fm, "title").and_then(Value::as_str) {
        let tt = crate::jsstr::js_trim(t);
        if !tt.is_empty() {
            title = Some(tt.to_string());
        }
    }
    let after = crate::markdown::body_after(content, &fm);
    let lines: Vec<&str> = after.split('\n').collect();
    let mut title_line_index: Option<usize> = None;
    if title.is_none() {
        if let Some((idx, t)) = first_heading(&lines) {
            title_line_index = Some(idx);
            title = Some(t);
        }
    }
    let title =
        title.unwrap_or_else(|| crate::jsstr::js_trim(&strip_md_ext(doc.file_name)).to_string());
    if title.is_empty() {
        return None; // no usable title → skip
    }

    // types: required valid enum list (parsed from frontmatter; absent → null → skip).
    let types = parse_wardrobe_types_field(fm_get(&fm, "types").unwrap_or(&Value::Null))?;

    let id = match fm_get(&fm, "id").and_then(Value::as_str) {
        Some(s) if is_wardrobe_id_shaped(s) => s.to_string(),
        _ => stable_uuid_from_string(&format!(
            "wardrobe-item:{}:{}",
            doc.mount_point_id, doc.relative_path
        )),
    };

    let appropriateness = fm_get(&fm, "appropriateness")
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty())
        .map(str::to_string);
    let image_prompt = fm_get(&fm, "imagePrompt")
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty())
        .map(str::to_string);

    let is_default = fm_get(&fm, "default") == Some(&Value::Bool(true))
        || fm_get(&fm, "isDefault") == Some(&Value::Bool(true));
    let replace = fm_get(&fm, "replace") == Some(&Value::Bool(true));

    // archivedAt: a non-empty string wins; else `archived: true` → doc.updatedAt.
    let archived_at = match fm_get(&fm, "archivedAt").and_then(Value::as_str) {
        Some(s) if !s.is_empty() => Some(s.to_string()),
        _ if fm_get(&fm, "archived") == Some(&Value::Bool(true)) => {
            Some(doc.updated_at.to_string())
        }
        _ => None,
    };

    // `typeof === 'string'` — any string (incl. empty) is kept.
    let migrated_from_clothing_record_id = fm_get(&fm, "migratedFromClothingRecordId")
        .and_then(Value::as_str)
        .map(str::to_string);

    let component_item_ids =
        parse_component_items_field(fm_get(&fm, "componentItems").unwrap_or(&Value::Null));

    // createdAt / updatedAt: a frontmatter string (incl. empty) wins, else the doc's.
    let created_at = fm_get(&fm, "createdAt")
        .and_then(Value::as_str)
        .map(str::to_string)
        .unwrap_or_else(|| doc.created_at.to_string());
    let updated_at = fm_get(&fm, "updatedAt")
        .and_then(Value::as_str)
        .map(str::to_string)
        .unwrap_or_else(|| doc.updated_at.to_string());

    // Body: drop the heading line when one was used as the title.
    let body = match title_line_index {
        Some(idx) => crate::jsstr::js_trim(&lines[idx + 1..].join("\n")).to_string(),
        None => crate::jsstr::js_trim(after).to_string(),
    };
    let description = if body.is_empty() { None } else { Some(body) };

    Some(WardrobeItemFromFile {
        id,
        character_id: character_id.to_string(),
        title: crate::jsstr::utf16_truncate(&title, 200),
        description,
        image_prompt,
        types,
        appropriateness,
        is_default,
        replace,
        component_item_ids,
        migrated_from_clothing_record_id,
        archived_at,
        created_at,
        updated_at,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shape_is_a_v8_uuid() {
        let id = stable_uuid_from_string("prompt:mp:Prompts/intro.md");
        // 8-4-4-4-12 with the version nibble 8 and an RFC-4122 variant nibble.
        let parts: Vec<&str> = id.split('-').collect();
        assert_eq!(
            parts.iter().map(|p| p.len()).collect::<Vec<_>>(),
            vec![8, 4, 4, 4, 12]
        );
        assert_eq!(&id[14..15], "8", "version nibble");
        assert!(
            matches!(&id[19..20], "8" | "9" | "a" | "b"),
            "variant nibble"
        );
    }

    #[test]
    fn is_deterministic() {
        assert_eq!(
            stable_uuid_from_string("scenario:x:y"),
            stable_uuid_from_string("scenario:x:y")
        );
        assert_ne!(
            stable_uuid_from_string("scenario:x:y"),
            stable_uuid_from_string("scenario:x:z")
        );
    }
}
