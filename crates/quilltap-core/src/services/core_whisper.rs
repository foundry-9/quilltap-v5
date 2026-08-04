//! Aurora's Core whisper — READ + ASSEMBLY half (v4
//! `lib/services/aurora-notifications/core-whisper.ts`).
//!
//! The Core whisper is a periodic, character-private re-offering of each
//! character's own `Core/` vault folder (plus the shared `Core/` of every group
//! they belong to). It grounds *who the character is* before they next take the
//! floor.
//!
//! Closes the READ/ASSEMBLY half of `BuildContextSeams::resolve_core_whisper_context`
//! (W4.6a): `resolveCoreWhisperConfig`, `assembleCorePacket`, and the three
//! content builders (persona / opaque / LLM-context). build_context consumes
//! `build_core_whisper_llm_context(packet)`. The whisper POST (`postCoreWhisper`)
//! plus the stale-whisper sweep are W4.6b — NOT ported here. The
//! `shouldFireCoreWhisper` trigger is the Phase-1 [`crate::core_whisper`] leaf.

use std::collections::BTreeMap;

use crate::collation::locale_compare;
use crate::db::runtime::Db;
use crate::db::DbError;
use crate::markdown::{body_after, parse_frontmatter};
use crate::token_estimation::estimate_tokens;

use rusqlite::Connection;

/// The default chars-per-token the zero-arg v4 `estimateTokens` uses
/// (`CHARS_PER_TOKEN.default`). Only feeds the soft-budget log warning (a no-op
/// in Rust), so it never reaches a diffed output.
const DEFAULT_CHARS_PER_TOKEN: f64 = 3.5;

/// v4 `CORE_WHISPER_PREAMBLE` — the exact two-paragraph preamble (snapshot-tested).
pub const CORE_WHISPER_PREAMBLE: &str = "This is who you are. Carry it forward. Are you being faithful to what you know yourself to be, to want, and to choose?\n\nYou may have grown since this was written. If something no longer fits, that is not failure. Name the change.";

/// v4 `CORE_WHISPER_ADVISORY` — closes the LLM-context form.
const CORE_WHISPER_ADVISORY: &str = "This material is offered, not imposed. If the scene honestly calls for silence, grief, confusion, experiment, contradiction, or change, do not perform a recognised self-shape. Ask whether this still comes from you.";

const AURORA_NARRATIVE_OPENER: &str =
    "Aurora pauses beside the workbench and sets your own plumb line into your hand:";

const LLM_CONTEXT_OPENER: &str = "Your own center of gravity, as you have written it for yourself:";

/// v4 `CoreFile`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CoreFile {
    /// Relative path inside the source store, e.g. `Core/manifesto.md`.
    pub path: String,
    /// File body with YAML frontmatter stripped.
    pub body: String,
    /// Group name when this Core file came from a group store (shared grounding).
    pub source_label: Option<String>,
}

/// v4 `CorePacket`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CorePacket {
    pub files: Vec<CoreFile>,
    /// Total token estimate over all file bodies + headers + preamble.
    pub approx_tokens: i64,
}

/// v4 `CoreWhisperSettings`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CoreWhisperSettings {
    pub enabled: bool,
    pub interval: i64,
    pub silence_threshold: i64,
    pub packet_token_budget: i64,
    pub fire_on_context_transition: bool,
}

/// v4 `resolveCoreWhisperConfig`: precedence chat → character → global default.
/// Only `enabled` + `interval` have per-entity overrides today. Each override is
/// a nullable option (v4's `??` coalescing).
pub fn resolve_core_whisper_config(
    chat_enabled: Option<bool>,
    chat_interval: Option<i64>,
    character_enabled: Option<bool>,
    global: &GlobalCoreWhisperSettings,
) -> CoreWhisperSettings {
    let default_enabled = global.enabled.unwrap_or(true);
    let default_interval = global.interval.unwrap_or(12);
    let silence_threshold = global.silence_threshold.unwrap_or(3);
    let packet_token_budget = global.packet_token_budget.unwrap_or(4096);
    let fire_on_context_transition = global.fire_on_context_transition.unwrap_or(true);

    // `chat?.coreWhisperEnabled ?? character?.coreWhisperEnabled ?? defaults.enabled`.
    let enabled = chat_enabled
        .or(character_enabled)
        .unwrap_or(default_enabled);
    // `chat?.coreWhisperInterval ?? defaults.interval`.
    let interval = chat_interval.unwrap_or(default_interval);

    CoreWhisperSettings {
        enabled,
        interval,
        silence_threshold,
        packet_token_budget,
        fire_on_context_transition,
    }
}

/// The global `CoreWhisperSettings` sub-object (from user chat settings), each
/// field a nullable option (absent → the built-in default).
#[derive(Clone, Debug, Default)]
pub struct GlobalCoreWhisperSettings {
    pub enabled: Option<bool>,
    pub interval: Option<i64>,
    pub silence_threshold: Option<i64>,
    pub packet_token_budget: Option<i64>,
    pub fire_on_context_transition: Option<bool>,
}

/// v4 `stripFrontmatterBody`: strip YAML frontmatter, then `replace(/^\n+/, '')`
/// then `replace(/\s+$/, '')`. `data === null || bodyStartOffset === 0` → operate
/// on the whole content.
fn strip_frontmatter_body(content: &str) -> String {
    if content.is_empty() {
        return String::new();
    }
    let parsed = parse_frontmatter(content);
    let slice: &str = if parsed.data.is_none() || parsed.body_start_offset == 0 {
        content
    } else {
        body_after(content, &parsed)
    };
    // `replace(/^\n+/, '')` (leading newlines only), then `replace(/\s+$/, '')`
    // (trailing JS-whitespace).
    let no_leading = slice.trim_start_matches('\n');
    crate::jsstr::js_trim_end(no_leading).to_string()
}

/// v4 `renderPacketBodies`.
fn render_packet_bodies(files: &[CoreFile]) -> String {
    files
        .iter()
        .map(|f| {
            let heading = match &f.source_label {
                Some(label) => format!("### [Shared — {label}] {}", f.path),
                None => format!("### {}", f.path),
            };
            format!("{heading}\n\n{}", f.body)
        })
        .collect::<Vec<_>>()
        .join("\n\n")
}

/// v4 `buildCoreWhisperContent` — persona-voiced transcript form.
pub fn build_core_whisper_content(packet: &CorePacket) -> String {
    let bodies = render_packet_bodies(&packet.files);
    format!("{AURORA_NARRATIVE_OPENER}\n\n{CORE_WHISPER_PREAMBLE}\n\n{bodies}")
}

/// v4 `buildCoreWhisperOpaqueContent` — persona-stripped form.
pub fn build_core_whisper_opaque_content(packet: &CorePacket) -> String {
    let bodies = render_packet_bodies(&packet.files);
    format!("{CORE_WHISPER_PREAMBLE}\n\n{bodies}")
}

/// v4 `buildCoreWhisperLLMContext` — plain second-person form for the LLM context.
pub fn build_core_whisper_llm_context(packet: &CorePacket) -> String {
    let bodies = render_packet_bodies(&packet.files);
    format!(
        "{LLM_CONTEXT_OPENER}\n\n{CORE_WHISPER_PREAMBLE}\n\n{bodies}\n\n{CORE_WHISPER_ADVISORY}"
    )
}

/// v4 `readVaultCoreFiles`: read every `Core/**.md` from one store, case-folded
/// dedup (first-stored-path wins on collision via `localeCompare`), sorted by the
/// lowercased key.
fn read_vault_core_files(
    mount: &Connection,
    mount_point_id: &str,
) -> Result<Vec<CoreFile>, DbError> {
    let docs = crate::db::doc_mount_documents::DocMountDocumentsRepository::new(mount)
        .find_many_by_mount_points_in_folder_opts(
            std::slice::from_ref(&mount_point_id.to_string()),
            "Core",
            ".md",
            true,
        )?;
    if docs.is_empty() {
        return Ok(Vec::new());
    }

    // byKey: lowercased relativePath → (path, content); collision keeps the
    // lexicographically-first STORED path (`existing.path.localeCompare(doc.path)
    // <= 0`).
    let mut by_key: BTreeMap<String, (String, String)> = BTreeMap::new();
    for doc in &docs {
        let key = doc.relative_path.to_lowercase();
        if let Some(existing) = by_key.get(&key) {
            let winner =
                if locale_compare(&existing.0, &doc.relative_path) != std::cmp::Ordering::Greater {
                    existing.0.clone()
                } else {
                    doc.relative_path.clone()
                };
            if winner == existing.0 {
                continue; // keep existing.
            }
        }
        by_key.insert(key, (doc.relative_path.clone(), doc.content.clone()));
    }

    // `Array.from(byKey.keys()).sort()` = ascending on the lowercased keys.
    // BTreeMap iterates keys in ascending byte order == JS default array sort on
    // ASCII slug/folder names (the tracked code-unit-vs-localeCompare seam; the
    // Core filenames stay ASCII).
    Ok(by_key
        .into_values()
        .map(|(path, content)| CoreFile {
            path,
            body: strip_frontmatter_body(&content),
            source_label: None,
        })
        .collect())
}

/// v4 `assembleGroupCoreFiles`: read the shared `Core/**.md` of every group the
/// character belongs to, labeled by group name, deduped across mounts (seeded with
/// the character's own vault mount). Groups emitted alphabetically by name. Fails
/// soft (a lookup failure never blocks the character's own Core).
fn assemble_group_core_files(
    main: &Connection,
    mount: &Connection,
    character_id: &str,
    character_mount_point_id: Option<&str>,
) -> Vec<CoreFile> {
    let group_ids =
        match crate::db::group_character_members::GroupCharacterMembersRepository::new(mount)
            .find_group_ids_by_character_id(character_id)
        {
            Ok(ids) => ids,
            Err(_) => return Vec::new(),
        };
    if group_ids.is_empty() {
        return Vec::new();
    }

    // seenMounts seeded with the character's own vault.
    let mut seen_mounts: std::collections::HashSet<String> = std::collections::HashSet::new();
    if let Some(mp) = character_mount_point_id {
        seen_mounts.insert(mp.to_string());
    }

    // groups: Vec<(name, mountIds)>.
    let mut groups: Vec<(String, Vec<String>)> = Vec::new();
    for group_id in &group_ids {
        let group =
            match crate::db::groups::find_name_and_official_mount_point_id_raw(main, group_id) {
                Ok(Some(g)) => g,
                _ => continue,
            };
        let (name, official) = group;
        let mut mount_ids: Vec<String> = Vec::new();
        if let Some(off) = official {
            if !seen_mounts.contains(&off) {
                seen_mounts.insert(off.clone());
                mount_ids.push(off);
            }
        }
        let links = crate::db::group_doc_mount_links::GroupDocMountLinksRepository::new(mount)
            .find_by_group_id(group_id)
            .unwrap_or_default();
        for link in links {
            if seen_mounts.contains(&link) {
                continue;
            }
            seen_mounts.insert(link.clone());
            mount_ids.push(link);
        }
        if !mount_ids.is_empty() {
            groups.push((name, mount_ids));
        }
    }

    // Emit groups alphabetically by name (`localeCompare`).
    groups.sort_by(|a, b| locale_compare(&a.0, &b.0));

    let mut result: Vec<CoreFile> = Vec::new();
    for (name, mount_ids) in groups {
        let docs = match crate::db::doc_mount_documents::DocMountDocumentsRepository::new(mount)
            .find_many_by_mount_points_in_folder_opts(&mount_ids, "Core", ".md", true)
        {
            Ok(d) => d,
            Err(_) => continue,
        };
        if docs.is_empty() {
            continue;
        }
        // FIRST-WINS dedup on case-folded path within the group, sorted by key.
        let mut by_key: BTreeMap<String, (String, String)> = BTreeMap::new();
        for doc in &docs {
            let key = doc.relative_path.to_lowercase();
            by_key
                .entry(key)
                .or_insert_with(|| (doc.relative_path.clone(), doc.content.clone()));
        }
        for (path, content) in by_key.into_values() {
            result.push(CoreFile {
                path,
                body: strip_frontmatter_body(&content),
                source_label: Some(name.clone()),
            });
        }
    }
    result
}

/// v4 `assembleCorePacket`: read the character's own `Core/**.md` + every group's
/// shared `Core/`, render, estimate tokens. Returns `None` when the character row
/// is missing or there are no Core files. Any error → `None` (v4's catch-all).
pub fn assemble_core_packet(
    db: &Db,
    character_id: &str,
    _packet_token_budget: i64,
) -> Option<CorePacket> {
    assemble_core_packet_inner(db, character_id).ok().flatten()
}

fn assemble_core_packet_inner(db: &Db, character_id: &str) -> Result<Option<CorePacket>, DbError> {
    db.read_main(|main| {
        db.read_mount_index(|mount| {
            // findByIdRaw: only the vault pointer is needed (survives a broken
            // overlay). Missing row → None.
            let character = crate::db::characters_read::find_by_id_raw(main, character_id)?;
            let Some(character) = character else {
                return Ok(None);
            };
            let mount_point_id = character
                .get("characterDocumentMountPointId")
                .and_then(|v| v.as_str())
                .map(str::to_string);

            let personal_files = match &mount_point_id {
                Some(mp) => read_vault_core_files(mount, mp)?,
                None => Vec::new(),
            };
            let group_files =
                assemble_group_core_files(main, mount, character_id, mount_point_id.as_deref());

            let mut files = personal_files;
            files.extend(group_files);
            if files.is_empty() {
                return Ok(None);
            }

            let persona_text = render_packet_bodies(&files);
            // v4: `estimateTokens(`${CORE_WHISPER_PREAMBLE}\n\n${personaText}`)`.
            let approx_tokens = estimate_tokens(
                &format!("{CORE_WHISPER_PREAMBLE}\n\n{persona_text}"),
                DEFAULT_CHARS_PER_TOKEN,
            );
            Ok(Some(CorePacket {
                files,
                approx_tokens,
            }))
        })
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn packet(files: Vec<CoreFile>) -> CorePacket {
        CorePacket {
            files,
            approx_tokens: 0,
        }
    }

    #[test]
    fn resolve_config_precedence() {
        let global = GlobalCoreWhisperSettings::default();
        // All defaults.
        let cfg = resolve_core_whisper_config(None, None, None, &global);
        assert_eq!(
            cfg,
            CoreWhisperSettings {
                enabled: true,
                interval: 12,
                silence_threshold: 3,
                packet_token_budget: 4096,
                fire_on_context_transition: true
            }
        );
        // chat enabled overrides character + global.
        let cfg = resolve_core_whisper_config(Some(false), Some(7), Some(true), &global);
        assert!(!cfg.enabled);
        assert_eq!(cfg.interval, 7);
        // character enabled used when chat is None.
        let cfg = resolve_core_whisper_config(None, None, Some(false), &global);
        assert!(!cfg.enabled);
        assert_eq!(cfg.interval, 12);
    }

    #[test]
    fn resolve_config_global_overrides() {
        let global = GlobalCoreWhisperSettings {
            enabled: Some(false),
            interval: Some(20),
            silence_threshold: Some(5),
            packet_token_budget: Some(2048),
            fire_on_context_transition: Some(false),
        };
        let cfg = resolve_core_whisper_config(None, None, None, &global);
        assert_eq!(
            cfg,
            CoreWhisperSettings {
                enabled: false,
                interval: 20,
                silence_threshold: 5,
                packet_token_budget: 2048,
                fire_on_context_transition: false
            }
        );
    }

    #[test]
    fn builders_byte_exact() {
        let p = packet(vec![
            CoreFile {
                path: "Core/manifesto.md".into(),
                body: "I build.".into(),
                source_label: None,
            },
            CoreFile {
                path: "Core/values.md".into(),
                body: "Kindness.".into(),
                source_label: Some("Household".into()),
            },
        ]);
        let bodies = "### Core/manifesto.md\n\nI build.\n\n### [Shared — Household] Core/values.md\n\nKindness.";
        assert_eq!(
            build_core_whisper_content(&p),
            format!("{AURORA_NARRATIVE_OPENER}\n\n{CORE_WHISPER_PREAMBLE}\n\n{bodies}")
        );
        assert_eq!(
            build_core_whisper_opaque_content(&p),
            format!("{CORE_WHISPER_PREAMBLE}\n\n{bodies}")
        );
        assert_eq!(
            build_core_whisper_llm_context(&p),
            format!("{LLM_CONTEXT_OPENER}\n\n{CORE_WHISPER_PREAMBLE}\n\n{bodies}\n\n{CORE_WHISPER_ADVISORY}")
        );
    }

    #[test]
    fn strip_frontmatter_body_variants() {
        // No frontmatter → leading \n stripped, trailing whitespace stripped.
        assert_eq!(strip_frontmatter_body("\n\nHello world  \n"), "Hello world");
        // Frontmatter present → body after it.
        let with_fm = "---\ntitle: X\n---\n\nBody here.\n";
        assert_eq!(strip_frontmatter_body(with_fm), "Body here.");
        // Empty → empty.
        assert_eq!(strip_frontmatter_body(""), "");
    }
}
