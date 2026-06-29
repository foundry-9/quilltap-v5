//! Character/wardrobe **vault-overlay** pure leaves — ported from v4's
//! `lib/database/repositories/vault-overlay/parsers.ts`. The vault overlay is the
//! heavier store-backed family (Family B of the document-store slice): a
//! character's identity/manifesto/prompts/scenarios/wardrobe live as files in its
//! official store, projected on read and routed on write. This module collects
//! the *pure* helpers that family needs, ported leaf-first so the stateful
//! overlay (a later slice) can build on verified primitives.
//!
//! So far: [`stable_uuid_from_string`] — the deterministic id every
//! folder-enumerated vault entity (system prompts, scenarios, wardrobe items)
//! derives from `(kind, mountPointId, relativePath)`, the id chat references
//! depend on.

use std::collections::{HashMap, HashSet};

use serde_json::Value;
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
