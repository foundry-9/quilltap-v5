//! The help-docs disk sync (v4 `lib/help/help-doc-sync.ts`) — read the help
//! Markdown tree and upsert into `help_docs`, clearing a changed doc's stored
//! embedding so it re-embeds.
//!
//! **Design decision (documented per the P4.1b work order):** the DIRECTORY
//! WALK is host-side — the core sync takes the already-read file list
//! ([`HelpSourceFile`], in walk order) so the engine stays fs-free. The host
//! walker (`quilltap-host::files_store::load_help_source_files`) reproduces v4
//! `findMarkdownFiles`' inline-recursive `readdirSync` order (files and
//! directories interleaved in raw readdir order — both Node and Rust issue the
//! same syscall over the same directory, so the order matches for a shared
//! fixture tree). The order only affects `changed_ids` sequencing; the
//! differential compares path-keyed forms.
//!
//! v4's LOCAL `parseFrontmatter` here is deliberately DISTINCT from the shared
//! `lib/markdown/frontmatter` parser (a loose regex, not the structural
//! parser) — ported as its own private helper, never unified with
//! [`crate::markdown::parse_frontmatter`].
//!
//! v4's private `generateDocumentId` was computed-but-unused here (dead code
//! carried from `build-help-index.ts`), and this port skipped it on that
//! ground. **That judgment expired with v4 `d6e74145`**, which DELETED it from
//! `help-doc-sync.ts` and promoted it to the shared module
//! `lib/help/help-doc-slug.ts` as the live `helpDocSlug` — the path-derived
//! identifier everything outside the database uses, since the DB primary key is
//! a UUID that changes whenever a doc is re-created. It is ported at
//! [`crate::help_doc_slug::help_doc_slug`]; the sync itself is not a consumer.
//!
//! The `ensureHelpDocsSynced` module-promise concurrency guard does not port
//! (the single-writer runtime already serializes callers).

use rusqlite::{params, Connection};

use crate::db::help_docs::{HdUpsert, HelpDocsRepository};
use crate::db::DbError;
use crate::jsstr::{is_js_ws, js_trim};

/// One on-disk help file (the host walker's output): `rel_path` is v4's
/// `relative(process.cwd(), filePath)` (e.g. `help/aurora.md`), `raw_content`
/// the UTF-8 file text.
#[derive(Clone, Debug)]
pub struct HelpSourceFile {
    pub rel_path: String,
    pub raw_content: String,
}

/// v4 `HelpDocSyncResult`.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct HelpDocSyncResult {
    pub total_on_disk: usize,
    pub created: usize,
    pub updated: usize,
    pub unchanged: usize,
    pub failed: usize,
    /// Ids of docs created or updated (need re-embedding), in walk order.
    pub changed_ids: Vec<String>,
}

/// Split `s` into JS-multiline "lines" — boundaries at `\r\n` (as one), `\n`,
/// `\r`, U+2028, U+2029 (the JS `/m` LineTerminator set).
fn js_lines(s: &str) -> Vec<&str> {
    let mut lines = Vec::new();
    let mut start = 0;
    let bytes = s.as_bytes();
    let mut i = 0;
    while i < s.len() {
        let c = s[i..].chars().next().unwrap();
        match c {
            '\r' => {
                lines.push(&s[start..i]);
                // \r\n counts as one boundary.
                if i + 1 < s.len() && bytes[i + 1] == b'\n' {
                    i += 2;
                } else {
                    i += 1;
                }
                start = i;
            }
            '\n' | '\u{2028}' | '\u{2029}' => {
                lines.push(&s[start..i]);
                i += c.len_utf8();
                start = i;
            }
            _ => i += c.len_utf8(),
        }
    }
    lines.push(&s[start..]);
    lines
}

/// v4's local `parseFrontmatter` (`help-doc-sync.ts:68`) — the loose
/// `/^---\r?\n([\s\S]*?)\r?\n---\r?\n?/` opener/closer scan + the first
/// `/^url:\s*(.+)$/m` line. Returns `(url, body)`; no frontmatter →
/// `("", content)`.
fn parse_frontmatter(content: &str) -> (String, String) {
    use std::sync::LazyLock;
    // `[\s\S]` = any char in both JS and Rust regex; the lazy group + the
    // optional trailing `\r?\n?` reproduce v4's quirks (a close fence NOT
    // followed by a newline still matches, leaving the rest as the body).
    static FM: LazyLock<regex::Regex> = LazyLock::new(|| {
        regex::Regex::new(r"^---\r?\n([\s\S]*?)\r?\n---\r?\n?").expect("frontmatter regex")
    });
    let Some(m) = FM.captures(content) else {
        return (String::new(), content.to_string());
    };
    let frontmatter = m.get(1).map(|g| g.as_str()).unwrap_or("");
    let body = &content[m.get(0).unwrap().end()..];

    // `/^url:\s*(.+)$/m` — the first line starting `url:` whose remainder is
    // non-empty (the `\s*(.+)` backtrack makes an all-whitespace remainder
    // still match, trimming to ""), then `.trim()`.
    let mut url = String::new();
    for line in js_lines(frontmatter) {
        if let Some(rest) = line.strip_prefix("url:") {
            if !rest.is_empty() {
                url = js_trim(rest).to_string();
                break;
            }
        }
    }
    (url, body.to_string())
}

/// v4 `extractTitle` (`help-doc-sync.ts:84`) — the first `/^#\s+(.+)$/m` H1 in
/// the BODY (trimmed), else the filename title-cased (`split('-')`,
/// first-unit uppercase per word).
fn extract_title(body: &str, rel_path: &str) -> String {
    for line in js_lines(body) {
        if let Some(rest) = line.strip_prefix('#') {
            // `#\s+` needs ≥1 JS-whitespace char, then `(.+)` ≥1 char (the
            // backtrack semantics collapse to: remainder starts with JS-ws and
            // has ≥2 chars total; the capture trims to js_trim(remainder)).
            let mut chars = rest.chars();
            if let Some(first) = chars.next() {
                if is_js_ws(first) && chars.next().is_some() {
                    return js_trim(rest).to_string();
                }
            }
        }
    }

    // Fallback: filename without the FIRST '.md' occurrence, '-'-split,
    // per-word first-char uppercase (JS `charAt(0).toUpperCase() + slice(1)`;
    // ASCII-faithful — help filenames are ASCII; a non-BMP first char is a
    // documented seam, v4 leaves such a word unchanged via lone surrogates).
    let filename = rel_path.rsplit('/').next().unwrap_or("");
    let filename = if filename.is_empty() {
        "Unknown"
    } else {
        filename
    };
    let without_md = match filename.find(".md") {
        Some(i) => format!("{}{}", &filename[..i], &filename[i + 3..]),
        None => filename.to_string(),
    };
    without_md
        .split('-')
        .map(|word| {
            let mut chars = word.chars();
            match chars.next() {
                Some(c) => format!("{}{}", c.to_uppercase(), chars.as_str()),
                None => String::new(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// Lowercase-hex SHA-256 of the (trimmed) content (v4 `hashContent`).
fn hash_content(content: &str) -> String {
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    hasher.update(content.as_bytes());
    hasher
        .finalize()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

/// v4 `findByPath` reduced to the two cells the sync consumes.
fn find_by_path(main: &Connection, path: &str) -> Result<Option<(String, String)>, DbError> {
    let row = main
        .query_row(
            "SELECT id, contentHash FROM help_docs WHERE path = ?1",
            params![path],
            |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)),
        )
        .map(Some)
        .or_else(|e| match e {
            rusqlite::Error::QueryReturnedNoRows => Ok(None),
            other => Err(other),
        })?;
    Ok(row)
}

/// v4 `clearAllEmbeddingsForDoc` — `_update(id, { embedding: null })`, which
/// NULLs the BLOB and mints `updatedAt` (the base `_update` clock).
fn clear_embedding(main: &Connection, id: &str) -> Result<(), DbError> {
    main.execute(
        "UPDATE help_docs SET embedding = NULL, updatedAt = ?1 WHERE id = ?2",
        params![crate::clock::now_iso(), id],
    )?;
    Ok(())
}

/// v4 `syncHelpDocs` (`help-doc-sync.ts:124`) over an already-walked file list
/// (see the module docs for the host-walk decision). Conn-level: the upserts
/// run on the caller's (writer-held) main connection. Per-file failures are
/// swallowed into `failed` (v4's catch).
pub fn sync_help_docs(main: &Connection, files: &[HelpSourceFile]) -> HelpDocSyncResult {
    let mut result = HelpDocSyncResult {
        total_on_disk: files.len(),
        ..Default::default()
    };
    if files.is_empty() {
        return result;
    }

    let repo = HelpDocsRepository::new(main);
    for file in files {
        let outcome = (|| -> Result<(), DbError> {
            let raw = js_trim(&file.raw_content);
            if raw.is_empty() {
                // v4 `continue` — counted in totalOnDisk only.
                return Ok(());
            }
            let content_hash = hash_content(raw);
            let (url, body) = parse_frontmatter(raw);
            let title = extract_title(&body, &file.rel_path);

            let existing = find_by_path(main, &file.rel_path)?;
            if let Some((_, existing_hash)) = &existing {
                if *existing_hash == content_hash {
                    result.unchanged += 1;
                    return Ok(());
                }
            }

            let doc_id = repo.upsert_by_path(&HdUpsert {
                title,
                path: file.rel_path.clone(),
                url,
                content: body,
                content_hash,
            })?;

            if existing.is_some() {
                clear_embedding(main, &doc_id)?;
                result.updated += 1;
            } else {
                result.created += 1;
            }
            result.changed_ids.push(doc_id);
            Ok(())
        })();
        if outcome.is_err() {
            result.failed += 1;
        }
    }
    result
}

/// v4 `ensureHelpDocsSynced` — lazy: sync only when `help_docs` is empty.
/// Returns `Some(result)` when a sync ran. (The module-promise concurrency
/// guard does not port — the single-writer runtime serializes callers.)
pub fn ensure_help_docs_synced(
    main: &Connection,
    files: &[HelpSourceFile],
) -> Result<Option<HelpDocSyncResult>, DbError> {
    let existing = HelpDocsRepository::new(main).find_all()?;
    if !existing.is_empty() {
        return Ok(None);
    }
    Ok(Some(sync_help_docs(main, files)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frontmatter_basic() {
        let (url, body) = parse_frontmatter("---\nurl: /help/x\n---\nBody here");
        assert_eq!(url, "/help/x");
        assert_eq!(body, "Body here");
    }

    #[test]
    fn frontmatter_crlf() {
        let (url, body) = parse_frontmatter("---\r\nurl: /a\r\n---\r\nB");
        assert_eq!(url, "/a");
        assert_eq!(body, "B");
    }

    #[test]
    fn frontmatter_absent_or_unclosed() {
        let (url, body) = parse_frontmatter("# Title\ntext");
        assert_eq!(url, "");
        assert_eq!(body, "# Title\ntext");
        // Unclosed fence → no match, whole content is the body.
        let (url, body) = parse_frontmatter("---\nurl: /a\nno close");
        assert_eq!(url, "");
        assert_eq!(body, "---\nurl: /a\nno close");
    }

    #[test]
    fn frontmatter_close_without_trailing_newline() {
        // v4's optional `\r?\n?` after the close fence: "---\nurl: /a\n---"
        // (EOF right after) matches with an empty body.
        let (url, body) = parse_frontmatter("---\nurl: /a\n---");
        assert_eq!(url, "/a");
        assert_eq!(body, "");
        // And the quirk: a close fence with trailing garbage on the SAME line
        // still closes, the garbage becoming the body ("---MORE").
        let (url, body) = parse_frontmatter("---\nurl: /a\n---MORE");
        assert_eq!(url, "/a");
        assert_eq!(body, "MORE");
    }

    #[test]
    fn url_line_edge_cases() {
        // `url:` alone (nothing after the colon) never matches; a later line can.
        let (url, _) = parse_frontmatter("---\nurl:\nurl: real\n---\nB");
        assert_eq!(url, "real");
        // Whitespace-only remainder matches and trims to "".
        let (url, _) = parse_frontmatter("---\nurl:   \nurl: later\n---\nB");
        assert_eq!(url, "");
    }

    #[test]
    fn title_extraction() {
        assert_eq!(extract_title("# My Title  \nrest", "help/x.md"), "My Title");
        // '#x' (no whitespace) is not an H1.
        assert_eq!(extract_title("#x\n", "help/some-doc.md"), "Some Doc");
        // Fallback casing: existing caps preserved (charAt(0).toUpperCase()).
        assert_eq!(
            extract_title("", "help/weird-CASE-Name.md"),
            "Weird CASE Name"
        );
        // `.replace('.md','')` removes the FIRST occurrence.
        assert_eq!(extract_title("", "help/a.mdx.md"), "Ax.md");
    }

    #[test]
    fn hash_is_sha256_hex() {
        assert_eq!(
            hash_content("abc"),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }
}
