//! Subprompts — smaller, optional instructions kept in a character's vault
//! (v4 `2f4254b42`, `lib/subprompts/subprompts.ts` — P4.D163).
//!
//! A subprompt is one Markdown file in the root-level `Subprompts/` folder of a
//! character's database-backed vault: a `title` in the frontmatter, the
//! instruction itself as the body, written in the second person like every
//! other prompt the character receives. A chat picks which of them are in play
//! per participant (`selectedSubpromptIds` on the participant record —
//! [`crate::db::chats::ChatParticipant::selected_subprompt_ids`]), and the
//! chosen ones ride into the compiled identity stack directly after the
//! character's system prompt (P4.D164) — and into the green-room dressing
//! call when the character chooses their own opening outfit (P4.D164).
//!
//! The folder is a lazy convention like Suparṇā's `Mail/` and Pascal's
//! `Tools/`: never scaffolded, ensured on EVERY create (v4's
//! `ensureFolderPath` runs on each `createCharacterSubprompt`, not only the
//! first — the hunk, not the commit message), and a missing folder lists as
//! empty rather than erroring. The file name (sans `.md`) is the subprompt's
//! id, so a selection survives edits to the title.
//!
//! This file carries the PURE half — the constants, the record types, the two
//! error shapes, and the five helpers v4 exports — pinned tier-1 exact by
//! `subprompts_helpers_equivalence` over a committed corpus driven through
//! v4's REAL exports. The vault reads/writes, the selection resolver and the
//! chat fan-out live in [`storage`] and [`fanout`].

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use unicode_normalization::UnicodeNormalization;

use crate::doc_edit::markdown_parser::serialize_frontmatter;
use crate::jsstr::{js_trim, js_trim_end, utf16_len, utf16_slice_from};
use crate::markdown::parse_frontmatter;

pub mod fanout;
pub mod storage;

pub use fanout::{
    fan_out_subprompt_change, FanoutOptions, FanoutSeams, ProductionFanoutSeams,
    SubpromptFanoutResult,
};
pub use storage::{
    create_character_subprompt, delete_character_subprompt, list_character_subprompts,
    list_subprompts_in_vault, read_character_subprompt, resolve_selected_subprompts,
    update_character_subprompt, SubpromptCreateInput, SubpromptError, SubpromptPatch,
};

/// Root-level vault folder holding a character's subprompts.
pub const SUBPROMPTS_FOLDER: &str = "Subprompts";

/// Longest title accepted; also bounds the derived file name.
pub const SUBPROMPT_TITLE_MAX_LENGTH: usize = 100;

/// Longest id accepted — a file name sans extension.
pub const SUBPROMPT_ID_MAX_LENGTH: usize = 120;

/// The slug cap inside [`slugify_subprompt_title`] (v4 `.slice(0, 60)`).
const SLUG_MAX_LENGTH: usize = 60;

/// A subprompt as read from the vault — v4 `Subprompt`, keys in THIS order on
/// the wire (the round's §C.1 record).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Subprompt {
    /// File name without `.md` — stable across title edits.
    pub id: String,
    /// Vault-relative path, `Subprompts/<id>.md`.
    pub path: String,
    /// From frontmatter `title`; falls back to the id when absent.
    pub title: String,
    /// The instruction body (no frontmatter).
    pub content: String,
    /// ISO 8601 timestamp of the underlying file's last modification.
    pub updated_at: String,
}

/// The slice of a subprompt a prompt builder needs (v4 `SubpromptForPrompt`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SubpromptForPrompt {
    pub title: String,
    pub content: String,
}

/// v4 `SubpromptNotFoundError` — `Subprompt "<id>" not found for character
/// <cid>`. The routes never surface the sentence (they answer
/// `notFound('Subprompt')`); it is kept for the log lines and for parity.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SubpromptNotFoundError {
    pub character_id: String,
    pub subprompt_id: String,
}

impl std::fmt::Display for SubpromptNotFoundError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "Subprompt \"{}\" not found for character {}",
            self.subprompt_id, self.character_id
        )
    }
}

impl std::error::Error for SubpromptNotFoundError {}

/// v4 `SubpromptValidationError` — the message IS the wire body (`badRequest(
/// error.message)`), so it is byte-exact: the three sentences below.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SubpromptValidationError(pub String);

impl std::fmt::Display for SubpromptValidationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for SubpromptValidationError {}

// ── Pure helpers ──────────────────────────────────────────────────────────────

/// v4 `isValidSubpromptId(id: unknown)` over a JSON value: `typeof id !==
/// 'string'` is `false`; a string goes through [`is_valid_subprompt_id`].
pub fn is_valid_subprompt_id_value(id: &Value) -> bool {
    match id {
        Value::String(s) => is_valid_subprompt_id(s),
        _ => false,
    }
}

/// A subprompt id is one path segment: no slashes, no `.`/`..`, no control or
/// filesystem-reserved characters, and short enough to be a file name.
///
/// v4's `id.length` is UTF-16 code units (`jsstr::utf16_len` — an astral
/// character counts two, so 61 emoji are 122 and refused), `charCodeAt(i) <
/// 32` per unit (a surrogate is never below 32, so the per-`char` test is the
/// same predicate), and `id.trim() === id` is the JS whitespace set
/// (`jsstr::js_trim` — U+00A0, U+3000 and U+FEFF are all whitespace to JS,
/// while Rust's `str::trim` keeps U+FEFF; the corpus's `trailing-bom-feff`
/// row is that distinction).
pub fn is_valid_subprompt_id(id: &str) -> bool {
    let len = utf16_len(id);
    if len == 0 || len > SUBPROMPT_ID_MAX_LENGTH {
        return false;
    }
    if id == "." || id == ".." {
        return false;
    }
    if id
        .chars()
        .any(|c| matches!(c, '\\' | '/' | '<' | '>' | ':' | '"' | '|' | '?' | '*'))
    {
        return false;
    }
    if id.chars().any(|c| (c as u32) < 32) {
        return false;
    }
    js_trim(id) == id
}

/// Vault-relative path for a subprompt id.
pub fn subprompt_path_for_id(id: &str) -> String {
    format!("{SUBPROMPTS_FOLDER}/{id}.md")
}

/// Derive a file-name slug from a title: lower-case, letters/digits/hyphens
/// only, collapsed, trimmed, capped. A title that yields nothing becomes
/// `subprompt`.
///
/// v4, step for step: `.normalize('NFKD')` → `.replace(/[̀-ͯ]/g,
/// '')` (the combining-diacritics block only — a base letter that has no
/// decomposition, the Turkish dotless ı or ß, survives and is then swept by
/// the class replace, so `ısı` → `s` and `Straße` → `stra-e`) →
/// `.toLowerCase()` (JS full-Unicode; `str::to_lowercase` is byte-identical,
/// Phase 1) → `.replace(/[^a-z0-9]+/g, '-')` → `.replace(/^-+|-+$/g, '')` →
/// `.slice(0, 60)` (after the class replace only ASCII survives, so 60 UTF-16
/// units are 60 bytes) → `.replace(/-+$/, '')` (the cut can land on a hyphen).
pub fn slugify_subprompt_title(title: &str) -> String {
    let folded: String = title
        .nfkd()
        .filter(|c| !('\u{0300}'..='\u{036f}').contains(c))
        .collect::<String>()
        .to_lowercase();
    let mut collapsed = String::with_capacity(folded.len());
    let mut in_run = false;
    for c in folded.chars() {
        if c.is_ascii_lowercase() || c.is_ascii_digit() {
            collapsed.push(c);
            in_run = false;
        } else if !in_run {
            collapsed.push('-');
            in_run = true;
        }
    }
    let trimmed = collapsed.trim_matches('-');
    let cut: String = trimmed.chars().take(SLUG_MAX_LENGTH).collect();
    let slug = cut.trim_end_matches('-');
    if slug.is_empty() {
        "subprompt".to_string()
    } else {
        slug.to_string()
    }
}

/// Compose the on-disk content: frontmatter + a blank line + the body
/// (`serializeFrontmatter({ title }) + '\n' + content.trim() + '\n'`).
pub fn compose_subprompt_content(title: &str, content: &str) -> String {
    let mut data = Map::new();
    data.insert("title".to_string(), Value::String(title.to_string()));
    format!("{}\n{}\n", serialize_frontmatter(&data), js_trim(content))
}

/// Parse a subprompt file. The title falls back to the id when the frontmatter
/// carries none, so a file dropped straight into the folder still lists.
///
/// `parseFrontmatter(raw)` is [`crate::markdown::parse_frontmatter`] (its
/// `bodyStartOffset` is UTF-16 units — the slice goes through
/// `utf16_slice_from`); the body is `raw.slice(offset).replace(/^\n+/, '')
/// .trimEnd()` — ONLY leading `\n`s are stripped (a leading NBSP or `\r`
/// survives; the corpus's `nbsp-body-edges` and `cr-only-body` rows), then the
/// JS `trimEnd`; the title is `data.title` when it is a string with a
/// non-blank JS `trim`, trimmed, else the id.
pub fn parse_subprompt_content(id: &str, raw: &str, updated_at: &str) -> Subprompt {
    let parsed = parse_frontmatter(raw);
    let data = parsed.data.unwrap_or(Value::Object(Map::new()));
    let body_full = utf16_slice_from(raw, parsed.body_start_offset);
    let body = js_trim_end(body_full.trim_start_matches('\n'));
    let title = match data.get("title") {
        Some(Value::String(t)) if !js_trim(t).is_empty() => js_trim(t).to_string(),
        _ => id.to_string(),
    };
    Subprompt {
        id: id.to_string(),
        path: subprompt_path_for_id(id),
        title,
        content: body.to_string(),
        updated_at: updated_at.to_string(),
    }
}

/// Only files directly inside `Subprompts/` count — `listDatabaseFiles`
/// filters by prefix, so a nested `Subprompts/drafts/x.md` would otherwise
/// sneak in. Case-INSENSITIVE on the prefix and the `.md` suffix, as v4
/// spells it (`toLowerCase().startsWith` / `endsWith`).
pub fn is_root_subprompt_file(relative_path: &str) -> bool {
    let prefix = format!("{SUBPROMPTS_FOLDER}/");
    let lower = relative_path.to_lowercase();
    if !lower.starts_with(&prefix.to_lowercase()) {
        return false;
    }
    // `relativePath.slice(prefix.length)` — the prefix is ASCII, so the
    // UTF-16 length is the byte length.
    let rest = &relative_path[prefix.len().min(relative_path.len())..];
    !rest.contains('/') && rest.to_lowercase().ends_with(".md")
}

/// v4 `idFromRelativePath` — the last segment with a case-insensitive `.md`
/// stripped.
pub fn id_from_relative_path(relative_path: &str) -> String {
    let file_name = match relative_path.rfind('/') {
        Some(i) => &relative_path[i + 1..],
        None => relative_path,
    };
    let lower = file_name.to_lowercase();
    if lower.ends_with(".md") {
        file_name[..file_name.len() - 3].to_string()
    } else {
        file_name.to_string()
    }
}

/// v4 `validateTitle`: JS `trim`; empty → `A subprompt needs a title`; more
/// than 100 UTF-16 units → `A subprompt title is at most 100 characters`
/// (UTF-16 `.length`, AFTER the route's Zod `.max(100)` which counts code
/// points — a 100-code-point astral title passes Zod and fails here).
pub fn validate_title(title: &str) -> Result<String, SubpromptValidationError> {
    let trimmed = js_trim(title);
    if trimmed.is_empty() {
        return Err(SubpromptValidationError(
            "A subprompt needs a title".to_string(),
        ));
    }
    if utf16_len(trimmed) > SUBPROMPT_TITLE_MAX_LENGTH {
        return Err(SubpromptValidationError(format!(
            "A subprompt title is at most {SUBPROMPT_TITLE_MAX_LENGTH} characters"
        )));
    }
    Ok(trimmed.to_string())
}

/// v4 `validateContent`: JS `trim`; empty → `A subprompt needs some
/// instruction text`.
pub fn validate_content(content: &str) -> Result<String, SubpromptValidationError> {
    let trimmed = js_trim(content);
    if trimmed.is_empty() {
        return Err(SubpromptValidationError(
            "A subprompt needs some instruction text".to_string(),
        ));
    }
    Ok(trimmed.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// v4's own `subprompts.test.ts` pure-helper cases, transcribed (the corpus
    /// family is the byte-level proof; these keep the module's tests local).
    #[test]
    fn v4_unit_cases_for_the_pure_helpers() {
        assert!(is_valid_subprompt_id("be-terse"));
        assert!(is_valid_subprompt_id("My Prompt"));
        for bad in ["", ".", "..", "a/b", "a\\b", "a:b", " padded"] {
            assert!(!is_valid_subprompt_id(bad), "{bad:?}");
        }
        assert!(!is_valid_subprompt_id(&"x".repeat(121)));
        assert!(!is_valid_subprompt_id_value(&Value::from(42)));
        assert_eq!(slugify_subprompt_title("Be Terse!"), "be-terse");
        assert_eq!(slugify_subprompt_title("  Café   au lait "), "cafe-au-lait");
        assert_eq!(slugify_subprompt_title("!!!"), "subprompt");
        let raw = compose_subprompt_content("Be terse", "You answer in one line.\n");
        let parsed = parse_subprompt_content("be-terse", &raw, "2026-01-01T00:00:00.000Z");
        assert_eq!(
            parsed,
            Subprompt {
                id: "be-terse".into(),
                path: "Subprompts/be-terse.md".into(),
                title: "Be terse".into(),
                content: "You answer in one line.".into(),
                updated_at: "2026-01-01T00:00:00.000Z".into(),
            }
        );
        let dropped = parse_subprompt_content("dropped-in", "Just a body.", "x");
        assert_eq!(dropped.title, "dropped-in");
        assert_eq!(dropped.content, "Just a body.");
    }

    /// The three validator sentences are the wire (`badRequest(error.message)`)
    /// and are not exported by v4, so the corpus family cannot drive them; they
    /// are transcribed here and measured end-to-end by the routes family.
    #[test]
    fn validator_sentences_are_v4s_bytes() {
        assert_eq!(
            validate_title("   ").unwrap_err().0,
            "A subprompt needs a title"
        );
        assert_eq!(
            validate_title(&"x".repeat(101)).unwrap_err().0,
            "A subprompt title is at most 100 characters"
        );
        // 100 astral code points are 200 UTF-16 units — Zod passes, v4's
        // `.length` refuses.
        assert!(validate_title(&"😀".repeat(100)).is_err());
        assert_eq!(validate_title("  T  ").unwrap(), "T");
        assert_eq!(
            validate_content(" \n\t ").unwrap_err().0,
            "A subprompt needs some instruction text"
        );
        assert_eq!(validate_content("  x  ").unwrap(), "x");
    }

    #[test]
    fn root_file_filter_and_id_extraction() {
        assert!(is_root_subprompt_file("Subprompts/a.md"));
        assert!(is_root_subprompt_file("subprompts/A.MD"));
        assert!(!is_root_subprompt_file("Subprompts/drafts/x.md"));
        assert!(!is_root_subprompt_file("Subprompts/notes.txt"));
        assert!(!is_root_subprompt_file("manifesto.md"));
        assert_eq!(id_from_relative_path("Subprompts/Be-Terse.MD"), "Be-Terse");
        assert_eq!(id_from_relative_path("x.md"), "x");
    }

    #[test]
    fn not_found_error_renders_v4s_sentence() {
        let e = SubpromptNotFoundError {
            character_id: "c1".into(),
            subprompt_id: "terse".into(),
        };
        assert_eq!(
            e.to_string(),
            "Subprompt \"terse\" not found for character c1"
        );
    }
}
