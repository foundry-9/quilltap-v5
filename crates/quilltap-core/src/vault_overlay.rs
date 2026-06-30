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

    // (markdownToNullable is defined just below the physical-prompts parser.)
    Some(CharacterVaultPhysicalPrompts {
        head_and_shoulders,
        short: nullable_string(obj.get("short")?)?,
        medium: nullable_string(obj.get("medium")?)?,
        long: nullable_string(obj.get("long")?)?,
        complete: nullable_string(obj.get("complete")?)?,
    })
}

/// Convert markdown file content into an overlay value (v4 `markdownToNullable`):
/// an empty string collapses to `null` (the "unset" state nullable schema fields
/// expect); any other content passes through as a string.
pub fn markdown_to_nullable(content: &str) -> Value {
    if content.is_empty() {
        Value::Null
    } else {
        Value::String(content.to_string())
    }
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

/// Resolve every item's raw `componentItemIds` (slug or UUID refs, as written in
/// the file) to canonical component ids, then clear any item whose resolved
/// components would form a cycle — v4 `resolveAndCheckComponentItems`
/// (`vault-overlay/parsers.ts:397`). Mutates `items` in place.
///
/// `item_by_slug` / `item_by_id` map a slug / item-id to the **index** of its
/// item in `items` (the caller builds them — slug is first-claimer-wins, every
/// item is addressable by id). A ref is resolved slug-first then UUID; an unknown
/// ref is dropped (read-tolerant). The cycle pass reads the **live** (already
/// mutated) component lists, so clearing one item mid-pass affects later items'
/// walks, exactly mirroring v4's mutable `itemById`.
pub fn resolve_and_check_component_items(
    items: &mut [WardrobeItemFromFile],
    item_by_slug: &HashMap<String, usize>,
    item_by_id: &HashMap<String, usize>,
) {
    // Pass 1 — slug/UUID → canonical id, dropping unknown refs. Compute all
    // resolved lists first (immutable borrow), then apply.
    let resolved: Vec<Option<Vec<String>>> = items
        .iter()
        .map(|item| {
            if item.component_item_ids.is_empty() {
                return None;
            }
            let mut out = Vec::new();
            for r in &item.component_item_ids {
                if let Some(&j) = item_by_slug.get(r) {
                    out.push(items[j].id.clone());
                } else if let Some(&j) = item_by_id.get(r) {
                    out.push(items[j].id.clone());
                }
                // unknown ref → dropped
            }
            Some(out)
        })
        .collect();
    for (i, r) in resolved.into_iter().enumerate() {
        if let Some(r) = r {
            items[i].component_item_ids = r;
        }
    }

    // Pass 2 — cycle check over the now-resolved ids. `by_id` maps each id to its
    // current component list and is updated as items are cleared, so the walk
    // sees prior-this-pass clears (matching v4's live `itemById`).
    let mut by_id: HashMap<String, Vec<String>> = items
        .iter()
        .map(|it| (it.id.clone(), it.component_item_ids.clone()))
        .collect();
    for item in items.iter_mut() {
        if item.component_item_ids.is_empty() {
            continue;
        }
        let cycles = detect_component_cycles(&item.id, &item.component_item_ids, &by_id);
        if !cycles.is_empty() {
            item.component_item_ids = Vec::new();
            by_id.insert(item.id.clone(), Vec::new());
        }
    }
}

// ── The wardrobe YAML emitter (Decision A — the ONLY eemeli/yaml site) ─────────
//
// `build_wardrobe_item_file` (v4 `buildWardrobeItemFile`,
// `mount-index/character-vault.ts`) projects a `WardrobeItem` to a `Wardrobe/*.md`
// file: a YAML frontmatter block + the description body. v4's frontmatter goes
// through eemeli/yaml's `YAML.stringify` (via `serializeFrontmatter`). Per locked
// Decision A we hand-roll a scoped emitter rather than depend on a YAML crate —
// the emitted bytes feed the content-dedup SHA, so a quoting mismatch is a
// *correctness* bug (silent mis-dedup), not just a test gap.
//
// This is a faithful port of eemeli/yaml 2.9.0's `stringifyString` +
// `foldFlowLines` (with default options: lineWidth 80, minContentWidth 20,
// doubleQuotedMinMultiLineLength 40, blockQuote true, singleQuote null,
// indent 2) for the bounded wardrobe value space: a top-level block map whose
// values are string scalars, the boolean `true`, or block sequences of string
// scalars. Two simplifications hold because every wardrobe frontmatter value is
// emitted at a NON-EMPTY indent (`  ` for a map value, `    ` for a seq element):
// the `containsDocumentMarker`/`indent === ''` branches never fire, and the
// indent fallbacks are always `ctx.indent`. Operates on UTF-16 code units
// throughout (as JS does) so fold offsets, the control-char/surrogate force-quote
// check, and `JSON.stringify` escaping match byte-for-byte.

const YAML_LINE_WIDTH: i64 = 80;
const YAML_MIN_CONTENT_WIDTH: i64 = 20;
const YAML_DQ_MIN_MULTILINE: i64 = 40;

// Frequently-used UTF-16 code units.
const U_SP: u16 = b' ' as u16;
const U_TAB: u16 = b'\t' as u16;
const U_NL: u16 = b'\n' as u16;
const U_BS: u16 = b'\\' as u16;
const U_DQ: u16 = b'"' as u16;
const U_SQ: u16 = b'\'' as u16;
const U_N: u16 = b'n' as u16;
const U_HASH: u16 = b'#' as u16;
const U_COLON: u16 = b':' as u16;

/// The core-schema implicit-type tests (all `default: true`): a plain scalar that
/// would reparse as one of these in YAML 1.2 core must be quoted. Regex sources
/// lifted verbatim from eemeli/yaml's `schema/core` + `schema/common/null`.
static YAML_REPARSE_RES: LazyLock<Vec<Regex>> = LazyLock::new(|| {
    [
        r"^(?:~|[Nn]ull|NULL)?$",                                  // null
        r"^(?:[Tt]rue|TRUE|[Ff]alse|FALSE)$",                      // bool
        r"^0o[0-7]+$",                                             // int OCT
        r"^[-+]?[0-9]+$",                                          // int
        r"^0x[0-9a-fA-F]+$",                                       // int HEX
        r"^(?:[-+]?\.(?:inf|Inf|INF)|\.nan|\.NaN|\.NAN)$",         // float special
        r"^[-+]?(?:\.[0-9]+|[0-9]+(?:\.[0-9]*)?)[eE][-+]?[0-9]+$", // float EXP
        r"^[-+]?(?:\.[0-9]+|[0-9]+\.[0-9]*)$",                     // float
    ]
    .iter()
    .map(|p| Regex::new(p).expect("valid reparse regex"))
    .collect()
});

/// A wardrobe frontmatter value the emitter handles.
enum FmVal {
    Str(String),
    True,
    Seq(Vec<String>),
}

fn s16(s: &str) -> Vec<u16> {
    s.encode_utf16().collect()
}
fn from16(v: &[u16]) -> String {
    String::from_utf16(v).expect("emitter output is valid UTF-16")
}

/// True iff `v` would reparse as a non-string under the core schema (so a plain
/// scalar of this value must be quoted). Applied to single-line values only.
fn yaml_reparse_unsafe(v: &str) -> bool {
    YAML_REPARSE_RES.iter().any(|re| re.is_match(v))
}

/// eemeli's force-double-quote set: `[\x00-\x08\x0b-\x1f\x7f-\x9f\u{D800}-\u{DFFF}]`
/// with the `/u` flag — so it matches CODE POINTS, not UTF-16 units. A valid astral
/// character is a single code point above `\u{FFFF}` (NOT in `D800-DFFF`), so it is
/// not forced to double quotes; only the control ranges and *lone* surrogates match.
/// Rust `&str` can never hold a lone surrogate, so that clause is unreachable here
/// and omitted — we test code points (`chars`), never the surrogate halves.
fn yaml_force_double(s: &str) -> bool {
    s.chars().any(|ch| {
        let c = ch as u32;
        c <= 0x08 || (0x0b..=0x1f).contains(&c) || (0x7f..=0x9f).contains(&c)
    })
}

/// The `plainString` indicator test — eemeli's
/// `/^[\n\t ,[\]{}#&*!|>'"%@`]|^[?-]$|^[?-][ \t]|[\n:][ \t]|[ \t]\n|[\n\t ]#|[\n\t :]$/`.
fn yaml_indicator_unsafe(v: &[u16]) -> bool {
    if v.is_empty() {
        return false;
    }
    let first_indicators: &[u8] = b",[]{}#&*!|>'\"%@`";
    let first = v[0];
    // ^[\n\t ,[]{}#&*!|>'"%@`]
    if first == U_NL
        || first == U_TAB
        || first == U_SP
        || (first < 0x80 && first_indicators.contains(&(first as u8)))
    {
        return true;
    }
    let q = b'?' as u16;
    let dash = b'-' as u16;
    // ^[?-]$
    if v.len() == 1 && (first == q || first == dash) {
        return true;
    }
    // ^[?-][ \t]
    if (first == q || first == dash) && (v[1] == U_SP || v[1] == U_TAB) {
        return true;
    }
    // [\n:][ \t]  |  [ \t]\n  |  [\n\t ]#  anywhere
    for i in 0..v.len() {
        let c = v[i];
        let next = v.get(i + 1).copied();
        if (c == U_NL || c == U_COLON) && (next == Some(U_SP) || next == Some(U_TAB)) {
            return true;
        }
        if (c == U_SP || c == U_TAB) && next == Some(U_NL) {
            return true;
        }
        if (c == U_NL || c == U_TAB || c == U_SP) && next == Some(U_HASH) {
            return true;
        }
    }
    // [\n\t :]$
    let last = v[v.len() - 1];
    last == U_NL || last == U_TAB || last == U_SP || last == U_COLON
}

#[derive(PartialEq, Clone, Copy)]
enum FoldMode {
    Flow,
    Block,
    Quoted,
}

/// Port of eemeli's `consumeMoreIndentedLines`. `i+1` is presumed at a line start;
/// returns the index of the last newline of a more-indented block.
fn yaml_consume_more_indented(text: &[u16], mut i: i64, indent: usize) -> i64 {
    let get = |idx: i64| -> Option<u16> {
        if idx < 0 {
            None
        } else {
            text.get(idx as usize).copied()
        }
    };
    let mut end = i;
    let mut start = i + 1;
    let mut ch = get(start);
    while ch == Some(U_SP) || ch == Some(U_TAB) {
        if i < start + indent as i64 {
            i += 1;
            ch = get(i);
        } else {
            loop {
                i += 1;
                ch = get(i);
                if ch.is_none() || ch == Some(U_NL) {
                    break;
                }
            }
            end = i;
            start = i + 1;
            ch = get(start);
        }
    }
    end
}

/// Port of eemeli's `foldFlowLines` (default lineWidth 80, minContentWidth 20).
/// Sets `*overflow` if a line overflowed without a fold point (block-folded uses
/// this to fall back to literal).
fn yaml_fold(
    text: &[u16],
    indent: &[u16],
    mode: FoldMode,
    indent_at_start: Option<usize>,
    overflow: &mut bool,
) -> Vec<u16> {
    let line_width = YAML_LINE_WIDTH;
    let mut min_content_width = YAML_MIN_CONTENT_WIDTH;
    if line_width < min_content_width {
        min_content_width = 0;
    }
    let end_step = std::cmp::max(1 + min_content_width, 1 + line_width - indent.len() as i64);
    if (text.len() as i64) <= end_step {
        return text.to_vec();
    }
    let mut folds: Vec<i64> = Vec::new();
    let mut escaped_folds: HashSet<i64> = HashSet::new();
    let mut end = line_width - indent.len() as i64;
    if let Some(ias) = indent_at_start {
        let ias = ias as i64;
        if ias > line_width - std::cmp::max(2, min_content_width) {
            folds.push(0);
        } else {
            end = line_width - ias;
        }
    }
    let get = |idx: i64| -> Option<u16> {
        if idx < 0 {
            None
        } else {
            text.get(idx as usize).copied()
        }
    };
    let mut split: Option<i64> = None;
    let mut prev: Option<u16> = None;
    let mut i: i64 = -1;
    let mut esc_start: i64 = -1;
    let mut esc_end: i64 = -1;
    if mode == FoldMode::Block {
        i = yaml_consume_more_indented(text, i, indent.len());
        if i != -1 {
            end = i + end_step;
        }
    }
    loop {
        i += 1;
        if i as usize >= text.len() {
            break;
        }
        let mut ch = text[i as usize];
        if mode == FoldMode::Quoted && ch == U_BS {
            esc_start = i;
            match get(i + 1) {
                Some(c) if c == b'x' as u16 => i += 3,
                Some(c) if c == b'u' as u16 => i += 5,
                Some(c) if c == b'U' as u16 => i += 9,
                _ => i += 1,
            }
            esc_end = i;
        }
        if ch == U_NL {
            if mode == FoldMode::Block {
                i = yaml_consume_more_indented(text, i, indent.len());
            }
            end = i + indent.len() as i64 + end_step;
            split = None;
        } else {
            if ch == U_SP
                && prev.is_some()
                && prev != Some(U_SP)
                && prev != Some(U_NL)
                && prev != Some(U_TAB)
            {
                let next = get(i + 1);
                if let Some(n) = next {
                    if n != U_SP && n != U_NL && n != U_TAB {
                        split = Some(i);
                    }
                }
            }
            if i >= end {
                if let Some(s) = split {
                    folds.push(s);
                    end = s + end_step;
                    split = None;
                } else if mode == FoldMode::Quoted {
                    while prev == Some(U_SP) || prev == Some(U_TAB) {
                        prev = Some(ch);
                        i += 1;
                        ch = match get(i) {
                            Some(c) => c,
                            None => break,
                        };
                        *overflow = true;
                    }
                    let j = if i > esc_end + 1 {
                        i - 2
                    } else {
                        esc_start - 1
                    };
                    if escaped_folds.contains(&j) {
                        return text.to_vec();
                    }
                    folds.push(j);
                    escaped_folds.insert(j);
                    end = j + end_step;
                    split = None;
                } else {
                    *overflow = true;
                }
            }
        }
        prev = Some(ch);
    }
    if folds.is_empty() {
        return text.to_vec();
    }
    let mut res: Vec<u16> = text[0..folds[0] as usize].to_vec();
    for k in 0..folds.len() {
        let fold = folds[k];
        let end_idx = folds.get(k + 1).copied().unwrap_or(text.len() as i64);
        if fold == 0 {
            res = Vec::new();
            res.push(U_NL);
            res.extend_from_slice(indent);
            res.extend_from_slice(&text[0..end_idx as usize]);
        } else {
            if mode == FoldMode::Quoted && escaped_folds.contains(&fold) {
                res.push(text[fold as usize]);
                res.push(U_BS);
            }
            res.push(U_NL);
            res.extend_from_slice(indent);
            res.extend_from_slice(&text[(fold + 1) as usize..end_idx as usize]);
        }
    }
    res
}

/// Port of eemeli's `doubleQuotedString` (default options; `implicitKey` false).
fn yaml_double_quoted(value: &[u16], indent: &[u16], indent_at_start: usize) -> Vec<u16> {
    let json = s16(&serde_json::to_string(&from16(value)).expect("json-encode scalar"));
    let mut out: Vec<u16> = Vec::new();
    let mut i: usize = 0;
    let mut start: usize = 0;
    let len = json.len();
    let at = |idx: usize| -> Option<u16> { json.get(idx).copied() };
    while i < len {
        let mut ch = json[i];
        // space before an escaped newline must itself be escaped (else folded away)
        if ch == U_SP && at(i + 1) == Some(U_BS) && at(i + 2) == Some(U_N) {
            out.extend_from_slice(&json[start..i]);
            out.push(U_BS);
            out.push(U_SP);
            i += 1;
            start = i;
            ch = U_BS;
        }
        if ch == U_BS {
            match at(i + 1) {
                Some(c) if c == b'u' as u16 => {
                    out.extend_from_slice(&json[start..i]);
                    let code = from16(&json[i + 2..i + 6]);
                    match code.as_str() {
                        "0000" => out.extend_from_slice(&s16("\\0")),
                        "0007" => out.extend_from_slice(&s16("\\a")),
                        "000b" => out.extend_from_slice(&s16("\\v")),
                        "001b" => out.extend_from_slice(&s16("\\e")),
                        "0085" => out.extend_from_slice(&s16("\\N")),
                        "00a0" => out.extend_from_slice(&s16("\\_")),
                        "2028" => out.extend_from_slice(&s16("\\L")),
                        "2029" => out.extend_from_slice(&s16("\\P")),
                        _ => {
                            if &code[0..2] == "00" {
                                out.extend_from_slice(&s16("\\x"));
                                out.extend_from_slice(&s16(&code[2..4]));
                            } else {
                                out.extend_from_slice(&json[i..i + 6]);
                            }
                        }
                    }
                    i += 5;
                    start = i + 1;
                }
                Some(c) if c == U_N => {
                    if at(i + 2) == Some(U_DQ) || (json.len() as i64) < YAML_DQ_MIN_MULTILINE {
                        i += 1;
                    } else {
                        out.extend_from_slice(&json[start..i]);
                        out.push(U_NL);
                        out.push(U_NL);
                        while at(i + 2) == Some(U_BS)
                            && at(i + 3) == Some(U_N)
                            && at(i + 4) != Some(U_DQ)
                        {
                            out.push(U_NL);
                            i += 2;
                        }
                        out.extend_from_slice(indent);
                        if at(i + 2) == Some(U_SP) {
                            out.push(U_BS);
                        }
                        i += 1;
                        start = i + 1;
                    }
                }
                _ => {
                    i += 1;
                }
            }
        }
        i += 1;
    }
    let str16: Vec<u16> = if start != 0 {
        let mut s = out;
        s.extend_from_slice(&json[start..]);
        s
    } else {
        json
    };
    let mut overflow = false;
    yaml_fold(
        &str16,
        indent,
        FoldMode::Quoted,
        Some(indent_at_start),
        &mut overflow,
    )
}

/// Port of eemeli's `singleQuotedString` (default options; `implicitKey` false).
fn yaml_single_quoted(value: &[u16], indent: &[u16], indent_at_start: usize) -> Vec<u16> {
    // single-quoted can't carry leading/trailing whitespace around a newline
    let mut ws_around_nl = false;
    for i in 0..value.len() {
        let c = value[i];
        let next = value.get(i + 1).copied();
        if (c == U_SP || c == U_TAB) && next == Some(U_NL) {
            ws_around_nl = true;
            break;
        }
        if c == U_NL && (next == Some(U_SP) || next == Some(U_TAB)) {
            ws_around_nl = true;
            break;
        }
    }
    if ws_around_nl {
        return yaml_double_quoted(value, indent, indent_at_start);
    }
    // '...' with ' → '' and \n+ → $&\n<indent>
    let mut body: Vec<u16> = Vec::new();
    let mut i = 0;
    while i < value.len() {
        let c = value[i];
        if c == U_SQ {
            body.push(U_SQ);
            body.push(U_SQ);
            i += 1;
        } else if c == U_NL {
            // emit the whole run of newlines, then one extra \n + indent
            let run_start = i;
            while i < value.len() && value[i] == U_NL {
                i += 1;
            }
            body.extend_from_slice(&value[run_start..i]);
            body.push(U_NL);
            body.extend_from_slice(indent);
        } else {
            body.push(c);
            i += 1;
        }
    }
    let mut res: Vec<u16> = vec![U_SQ];
    res.extend_from_slice(&body);
    res.push(U_SQ);
    let mut overflow = false;
    yaml_fold(
        &res,
        indent,
        FoldMode::Flow,
        Some(indent_at_start),
        &mut overflow,
    )
}

/// Port of eemeli's `quotedString` single-vs-double selection (singleQuote null).
fn yaml_quoted(value: &[u16], indent: &[u16], indent_at_start: usize) -> Vec<u16> {
    let has_double = value.contains(&U_DQ);
    let has_single = value.contains(&U_SQ);
    if has_double && !has_single {
        yaml_single_quoted(value, indent, indent_at_start)
    } else {
        yaml_double_quoted(value, indent, indent_at_start)
    }
}

/// `lineLengthOverLimit` — does any line of `value` exceed `lineWidth - indent`?
fn yaml_line_over_limit(value: &[u16], indent_len: usize) -> bool {
    let limit = YAML_LINE_WIDTH - indent_len as i64;
    let str_len = value.len() as i64;
    if str_len <= limit {
        return false;
    }
    let mut start: i64 = 0;
    let mut i: i64 = 0;
    while i < str_len {
        if value[i as usize] == U_NL {
            if i - start > limit {
                return true;
            }
            start = i + 1;
            if str_len - start <= limit {
                return false;
            }
        }
        i += 1;
    }
    true
}

/// Replace every run of `\n` with itself plus `<rep>` (eemeli's
/// `.replace(/\n+/g, "$&" + rep)`), operating on UTF-16 units.
fn yaml_replace_nl_runs(value: &[u16], rep: &[u16]) -> Vec<u16> {
    let mut out: Vec<u16> = Vec::new();
    let mut i = 0;
    while i < value.len() {
        if value[i] == U_NL {
            let run_start = i;
            while i < value.len() && value[i] == U_NL {
                i += 1;
            }
            out.extend_from_slice(&value[run_start..i]);
            out.extend_from_slice(rep);
        } else {
            out.push(value[i]);
            i += 1;
        }
    }
    out
}

/// Port of eemeli's `blockString` for the literal (`|`) path; folds to `>` when a
/// line overruns. `comment` is always absent here; node `type` is unset (so the
/// literal-vs-folded choice is by `lineLengthOverLimit`).
fn yaml_block_string(value: &[u16], indent: &[u16], indent_at_start: usize) -> Vec<u16> {
    // blockQuote default true; a string that can't end a block in whitespace
    // (a `\n` followed by trailing spaces/tabs to EOS) is quoted instead.
    let mut trailing_ws_after_nl = false;
    {
        // /\n[\t ]+$/
        let mut j = value.len();
        let mut saw_ws = false;
        while j > 0 {
            let c = value[j - 1];
            if c == U_SP || c == U_TAB {
                saw_ws = true;
                j -= 1;
            } else {
                break;
            }
        }
        if saw_ws && j > 0 && value[j - 1] == U_NL {
            trailing_ws_after_nl = true;
        }
    }
    if trailing_ws_after_nl {
        return yaml_quoted(value, indent, indent_at_start);
    }
    let literal = !yaml_line_over_limit(value, indent.len());
    if value.is_empty() {
        return s16(if literal { "|\n" } else { ">\n" });
    }
    let mut value = value.to_vec();
    // chomping from trailing whitespace
    let mut end_start = value.len();
    while end_start > 0 {
        let ch = value[end_start - 1];
        if ch != U_NL && ch != U_TAB && ch != U_SP {
            break;
        }
        end_start -= 1;
    }
    let mut end: Vec<u16> = value[end_start..].to_vec();
    let end_nl_pos = end.iter().position(|&c| c == U_NL);
    let chomp: &str = match end_nl_pos {
        None => "-",
        Some(p) => {
            // value === end  ||  endNlPos !== end.length - 1  → keep '+'
            if value.len() == end.len() || p != end.len() - 1 {
                "+"
            } else {
                ""
            }
        }
    };
    if !end.is_empty() {
        let cut = value.len() - end.len();
        value = value[..cut].to_vec();
        if *end.last().unwrap() == U_NL {
            end.pop();
        }
        // end.replace(blockEndNewlines, "$&" + indent): insert indent after each
        // run of newlines that is neither at start nor end-of-string.
        end = yaml_block_end_newlines(&end, indent);
    }
    // indent indicator from leading whitespace
    let mut start_with_space = false;
    let mut start_nl_pos: i64 = -1;
    let mut start_end = 0usize;
    while start_end < value.len() {
        let ch = value[start_end];
        if ch == U_SP {
            start_with_space = true;
        } else if ch == U_NL {
            start_nl_pos = start_end as i64;
        } else {
            break;
        }
        start_end += 1;
    }
    let start_slice_end = if start_nl_pos < start_end as i64 {
        (start_nl_pos + 1) as usize
    } else {
        start_end
    };
    let mut start: Vec<u16> = value[..start_slice_end].to_vec();
    if !start.is_empty() {
        value = value[start.len()..].to_vec();
        start = yaml_replace_nl_runs(&start, indent);
    }
    let indent_size = if indent.is_empty() { "1" } else { "2" };
    let mut header = String::new();
    if start_with_space {
        header.push_str(indent_size);
    }
    header.push_str(chomp);

    if !literal {
        // folded body
        let folded = yaml_block_folded_body(&value, indent);
        let mut overflow = false;
        let mut seq: Vec<u16> = Vec::new();
        seq.extend_from_slice(&start);
        seq.extend_from_slice(&folded);
        seq.extend_from_slice(&end);
        let body = yaml_fold(
            &seq,
            indent,
            FoldMode::Block,
            Some(indent.len()),
            &mut overflow,
        );
        if !overflow {
            // `>${header}\n${indent}${body}`
            let mut out: Vec<u16> = Vec::new();
            out.push(b'>' as u16);
            out.extend_from_slice(&s16(&header));
            out.push(U_NL);
            out.extend_from_slice(indent);
            out.extend_from_slice(&body);
            return out;
        }
    }
    // literal: `|${header}\n${indent}${start}${value}${end}`
    let value = yaml_replace_nl_runs(&value, indent);
    let mut out: Vec<u16> = Vec::new();
    out.push(b'|' as u16);
    out.extend_from_slice(&s16(&header));
    out.push(U_NL);
    out.extend_from_slice(indent);
    out.extend_from_slice(&start);
    out.extend_from_slice(&value);
    out.extend_from_slice(&end);
    out
}

/// eemeli's `blockEndNewlines` replace: `(^|(?<!\n))\n+(?!\n|$)` → `$&<indent>`.
/// Inserts `indent` after a run of newlines that is preceded by a non-newline (or
/// is at string start) and not at end-of-string / followed by another newline.
fn yaml_block_end_newlines(end: &[u16], indent: &[u16]) -> Vec<u16> {
    let mut out: Vec<u16> = Vec::new();
    let mut i = 0;
    while i < end.len() {
        if end[i] == U_NL {
            let run_start = i;
            while i < end.len() && end[i] == U_NL {
                i += 1;
            }
            out.extend_from_slice(&end[run_start..i]);
            // Lookbehind `(^|(?<!\n))`: the char before the run is not a newline.
            // Since a maximal run consumed all leading newlines, the preceding
            // char is non-`\n` (or start) by construction → always satisfied.
            // Negative lookahead `(?!\n|$)`: not end-of-string (run is maximal, so
            // the next char, if any, is non-`\n`).
            if i < end.len() {
                out.extend_from_slice(indent);
            }
        } else {
            out.push(end[i]);
            i += 1;
        }
    }
    out
}

/// eemeli's folded-body transform (block `>`): the three chained `.replace` calls
/// from `blockString`'s `!literal` branch, on UTF-16 units.
fn yaml_block_folded_body(value: &[u16], indent: &[u16]) -> Vec<u16> {
    // 1. /\n+/g → "\n$&"  (prefix every newline run with one extra newline)
    let step1 = {
        let mut out: Vec<u16> = Vec::new();
        let mut i = 0;
        while i < value.len() {
            if value[i] == U_NL {
                let s = i;
                while i < value.len() && value[i] == U_NL {
                    i += 1;
                }
                out.push(U_NL);
                out.extend_from_slice(&value[s..i]);
            } else {
                out.push(value[i]);
                i += 1;
            }
        }
        out
    };
    // 2. /(?:^|\n)([\t ].*)(?:([\n\t ]*)\n(?![\n\t ]))?/g → "$1$2"
    //    (more-indented lines aren't folded). Implemented as a direct scan in
    //    `yaml_more_indented_fold` rather than a backtracking regex.
    let step2 = yaml_more_indented_fold(&step1);
    // 3. /\n+/g → "$&<indent>"
    yaml_replace_nl_runs(&step2, indent)
}

/// The more-indented-lines collapse of eemeli's folded body (step 2 above),
/// implemented as a direct scan rather than a backtracking regex.
fn yaml_more_indented_fold(value: &[u16]) -> Vec<u16> {
    // Match `(?:^|\n)([\t ].*)(?:([\n\t ]*)\n(?![\n\t ]))?` globally, replacing
    // with `$1$2` — i.e. drop the leading `\n` (or BOS) and the single trailing
    // `\n` (not followed by indent) of a more-indented line group.
    let mut out: Vec<u16> = Vec::new();
    let n = value.len();
    let mut i = 0;
    let mut at_line_start = true; // BOS counts as `^`
    while i < n {
        let c = value[i];
        if c == U_NL {
            // The regex's leading `(?:^|\n)` consumes this `\n` only when the next
            // char begins a more-indented line; otherwise the `\n` is literal.
            if i + 1 < n && (value[i + 1] == U_TAB || value[i + 1] == U_SP) {
                // consumed as the group's leading `\n` (dropped)
                i += 1;
                at_line_start = true;
                continue;
            }
            out.push(c);
            i += 1;
            at_line_start = true;
            continue;
        }
        if at_line_start && (c == U_TAB || c == U_SP) {
            // `([\t ].*)` — capture to end of line
            let g1_start = i;
            while i < n && value[i] != U_NL {
                i += 1;
            }
            let g1_end = i;
            // optional `([\n\t ]*)\n(?![\n\t ])`
            let mut g2: Vec<u16> = Vec::new();
            if i < n {
                // collect [\n\t ]* then require a \n not followed by [\n\t ]
                let save = i;
                let mut j = i;
                let cap_start = j;
                while j < n && (value[j] == U_NL || value[j] == U_TAB || value[j] == U_SP) {
                    j += 1;
                }
                // backtrack to a position where value[k-1]=='\n' and (k==n or value[k] not in [\n\t ])
                let mut matched = false;
                let mut k = j;
                while k > cap_start {
                    if value[k - 1] == U_NL
                        && (k >= n || (value[k] != U_NL && value[k] != U_TAB && value[k] != U_SP))
                    {
                        // g2 = value[cap_start..k-1], then the \n at k-1 is consumed (dropped)
                        g2 = value[cap_start..k - 1].to_vec();
                        i = k;
                        matched = true;
                        break;
                    }
                    k -= 1;
                }
                if !matched {
                    i = save;
                }
            }
            out.extend_from_slice(&value[g1_start..g1_end]);
            out.extend_from_slice(&g2);
            at_line_start = false;
            continue;
        }
        out.push(c);
        i += 1;
        at_line_start = false;
    }
    out
}

/// Port of eemeli's `plainString` for our context (`implicitKey`/`inFlow` false,
/// node `type` unset). `value_str` is the original (single-line) value for the
/// reparse test.
fn yaml_plain_string(
    value: &[u16],
    value_str: &str,
    indent: &[u16],
    indent_at_start: usize,
) -> Vec<u16> {
    let has_nl = value.contains(&U_NL);
    if yaml_indicator_unsafe(value) {
        return if !has_nl {
            yaml_quoted(value, indent, indent_at_start)
        } else {
            yaml_block_string(value, indent, indent_at_start)
        };
    }
    // type !== PLAIN (unset) && value has newline → prefer block
    if has_nl {
        return yaml_block_string(value, indent, indent_at_start);
    }
    // single-line: reparse-safety, then fold
    if yaml_reparse_unsafe(value_str) {
        return yaml_quoted(value, indent, indent_at_start);
    }
    let mut overflow = false;
    yaml_fold(
        value,
        indent,
        FoldMode::Flow,
        Some(indent_at_start),
        &mut overflow,
    )
}

/// Port of eemeli's `stringifyString` for a string scalar at the given indent /
/// start column.
fn yaml_stringify_scalar(value: &str, indent: &str, indent_at_start: usize) -> String {
    let v16 = s16(value);
    let ind16 = s16(indent);
    if yaml_force_double(value) {
        return from16(&yaml_double_quoted(&v16, &ind16, indent_at_start));
    }
    from16(&yaml_plain_string(&v16, value, &ind16, indent_at_start))
}

/// Port of `YAML.stringify` for the wardrobe frontmatter value model: a top-level
/// block map of string scalars, the boolean `true`, and block sequences of string
/// scalars (indent step 2). Always ends with a trailing newline.
fn yaml_stringify_frontmatter(entries: &[(&str, FmVal)]) -> String {
    let mut parts: Vec<String> = Vec::new();
    for (key, val) in entries {
        match val {
            FmVal::True => parts.push(format!("{key}: true")),
            FmVal::Str(s) => {
                let ias = key.encode_utf16().count() + 2; // "<key>: "
                parts.push(format!("{key}: {}", yaml_stringify_scalar(s, "  ", ias)));
            }
            FmVal::Seq(items) => {
                let mut e = format!("{key}:");
                for it in items {
                    e.push_str("\n  - ");
                    e.push_str(&yaml_stringify_scalar(it, "    ", 4));
                }
                parts.push(e);
            }
        }
    }
    let mut out = parts.join("\n");
    out.push('\n');
    out
}

/// v4 `serializeFrontmatter`: wrap the YAML body in `---` delimiters.
fn yaml_serialize_frontmatter(entries: &[(&str, FmVal)]) -> String {
    format!("---\n{}---\n", yaml_stringify_frontmatter(entries))
}

/// Project a `WardrobeItem` to its `Wardrobe/*.md` file content — v4
/// `buildWardrobeItemFile` (`mount-index/character-vault.ts`). The frontmatter
/// keys are emitted in v4's exact insertion order; `componentItemIds` are
/// translated to slugs via `slug_by_item_id` (UUID fallback for collisions /
/// unknowns). The body is the item description (or empty). Decision-A YAML emitter.
pub fn build_wardrobe_item_file(
    item: &WardrobeItem,
    slug_by_item_id: &HashMap<String, String>,
) -> String {
    let comp: Vec<String> = item
        .component_item_ids
        .iter()
        .map(|id| {
            slug_by_item_id
                .get(id)
                .cloned()
                .unwrap_or_else(|| id.clone())
        })
        .collect();

    let mut data: Vec<(&str, FmVal)> = Vec::new();
    data.push(("id", FmVal::Str(item.id.clone())));
    data.push(("title", FmVal::Str(item.title.clone())));
    data.push(("types", FmVal::Seq(item.types.clone())));
    if !item.component_item_ids.is_empty() {
        data.push(("componentItems", FmVal::Seq(comp)));
    }
    if let Some(Some(a)) = &item.appropriateness {
        if !a.is_empty() {
            data.push(("appropriateness", FmVal::Str(a.clone())));
        }
    }
    if let Some(Some(ip)) = &item.image_prompt {
        if !ip.is_empty() {
            data.push(("imagePrompt", FmVal::Str(ip.clone())));
        }
    }
    if item.is_default {
        data.push(("default", FmVal::True));
    }
    if item.replace {
        data.push(("replace", FmVal::True));
    }
    if let Some(Some(at)) = &item.archived_at {
        if !at.is_empty() {
            data.push(("archived", FmVal::True));
            data.push(("archivedAt", FmVal::Str(at.clone())));
        }
    }
    if let Some(Some(m)) = &item.migrated_from_clothing_record_id {
        if !m.is_empty() {
            data.push(("migratedFromClothingRecordId", FmVal::Str(m.clone())));
        }
    }
    data.push(("createdAt", FmVal::Str(item.created_at.clone())));
    data.push(("updatedAt", FmVal::Str(item.updated_at.clone())));

    let body = match &item.description {
        Some(Some(d)) => d.clone(),
        _ => String::new(),
    };
    format!("{}\n{}", yaml_serialize_frontmatter(&data), body)
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

    /// A `WardrobeItemFromFile` with only the fields the resolver reads.
    fn wif(id: &str, title: &str, refs: &[&str]) -> WardrobeItemFromFile {
        WardrobeItemFromFile {
            id: id.to_string(),
            character_id: "c".to_string(),
            title: title.to_string(),
            description: None,
            image_prompt: None,
            types: vec!["top".to_string()],
            appropriateness: None,
            is_default: false,
            replace: false,
            component_item_ids: refs.iter().map(|s| s.to_string()).collect(),
            migrated_from_clothing_record_id: None,
            archived_at: None,
            created_at: "t".to_string(),
            updated_at: "t".to_string(),
        }
    }

    fn build_maps(
        items: &[WardrobeItemFromFile],
    ) -> (HashMap<String, usize>, HashMap<String, usize>) {
        let mut by_id = HashMap::new();
        let mut by_slug = HashMap::new();
        let mut claimed: HashSet<String> = HashSet::new();
        for (i, it) in items.iter().enumerate() {
            by_id.insert(it.id.clone(), i);
            let slug = slugify_wardrobe_title(&it.title);
            if slug.is_empty() || claimed.contains(&slug) {
                continue;
            }
            claimed.insert(slug.clone());
            by_slug.insert(slug, i);
        }
        (by_slug, by_id)
    }

    #[test]
    fn resolves_slug_uuid_and_drops_unknown() {
        // Outfit refs a slug ("shirt"), a UUID, and an unknown ref ("ghost").
        let mut items = vec![
            wif("id-shirt", "Shirt", &[]),
            wif("id-pants", "Pants", &[]),
            wif("id-outfit", "Outfit", &["shirt", "id-pants", "ghost"]),
        ];
        let (by_slug, by_id) = build_maps(&items);
        resolve_and_check_component_items(&mut items, &by_slug, &by_id);
        assert_eq!(
            items[2].component_item_ids,
            vec!["id-shirt".to_string(), "id-pants".to_string()],
            "slug→id, uuid→id, unknown dropped"
        );
    }

    #[test]
    fn mutual_cycle_clears_first_then_second_survives() {
        // a → b, b → a. Pass 2 processes `a` first, clearing it; `b`'s walk then
        // sees a's now-empty list and survives (the live-mutation asymmetry).
        let mut items = vec![
            wif("id-a", "Cycle A", &["cycle-b"]),
            wif("id-b", "Cycle B", &["cycle-a"]),
        ];
        let (by_slug, by_id) = build_maps(&items);
        resolve_and_check_component_items(&mut items, &by_slug, &by_id);
        assert_eq!(
            items[0].component_item_ids,
            Vec::<String>::new(),
            "a cleared"
        );
        assert_eq!(
            items[1].component_item_ids,
            vec!["id-a".to_string()],
            "b survives — a was already emptied when b's walk ran"
        );
    }

    #[test]
    fn self_cycle_clears() {
        let mut items = vec![wif("id-self", "Self Ref", &["self-ref"])];
        let (by_slug, by_id) = build_maps(&items);
        resolve_and_check_component_items(&mut items, &by_slug, &by_id);
        assert_eq!(items[0].component_item_ids, Vec::<String>::new());
    }

    #[test]
    fn collided_slug_resolves_to_first_claimer() {
        // Two "Hat" items share the slug; only the first claims it.
        let mut items = vec![
            wif("id-hat-1", "Hat", &[]),
            wif("id-hat-2", "Hat", &[]),
            wif("id-box", "Box", &["hat"]),
        ];
        let (by_slug, by_id) = build_maps(&items);
        resolve_and_check_component_items(&mut items, &by_slug, &by_id);
        assert_eq!(items[2].component_item_ids, vec!["id-hat-1".to_string()]);
    }

    /// A minimal item with the given overrides applied through a builder closure.
    fn warde(id: &str, title: &str) -> WardrobeItem {
        WardrobeItem {
            id: id.to_string(),
            character_id: None,
            title: title.to_string(),
            description: None,
            image_prompt: None,
            types: vec!["top".to_string()],
            component_item_ids: vec![],
            appropriateness: None,
            is_default: false,
            replace: false,
            migrated_from_clothing_record_id: None,
            archived_at: None,
            created_at: "2026-01-01T00:00:00.000Z".to_string(),
            updated_at: "2026-01-01T00:00:00.000Z".to_string(),
        }
    }

    #[test]
    fn build_file_minimal() {
        let item = warde("00000000-0000-4000-8000-000000000001", "Blue Shirt");
        let got = build_wardrobe_item_file(&item, &HashMap::new());
        assert_eq!(
            got,
            "---\nid: 00000000-0000-4000-8000-000000000001\ntitle: Blue Shirt\ntypes:\n  - top\ncreatedAt: 2026-01-01T00:00:00.000Z\nupdatedAt: 2026-01-01T00:00:00.000Z\n---\n\n"
        );
    }

    #[test]
    fn build_file_quotes_numeric_title_and_body() {
        let mut item = warde("00000000-0000-4000-8000-000000000002", "123");
        item.description = Some(Some("A short body.".to_string()));
        let got = build_wardrobe_item_file(&item, &HashMap::new());
        // numeric-looking title is double-quoted; body trails after the block
        assert!(got.contains("title: \"123\"\n"), "got: {got}");
        assert!(got.ends_with("---\n\nA short body."), "got: {got}");
    }

    #[test]
    fn build_file_component_items_slug_then_uuid_fallback() {
        let mut outfit = warde("00000000-0000-4000-8000-000000000003", "Outfit");
        outfit.component_item_ids = vec![
            "id-shirt".to_string(),
            "00000000-0000-4000-8000-0000000000ff".to_string(),
        ];
        let mut slug = HashMap::new();
        slug.insert("id-shirt".to_string(), "blue-shirt".to_string());
        let got = build_wardrobe_item_file(&outfit, &slug);
        // first ref maps to its slug; the unmapped UUID falls through verbatim
        assert!(
            got.contains(
                "componentItems:\n  - blue-shirt\n  - 00000000-0000-4000-8000-0000000000ff\n"
            ),
            "got: {got}"
        );
    }
}
