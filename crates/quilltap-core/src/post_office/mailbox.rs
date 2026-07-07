//! The Post Office — mailbox storage layer (v4 `lib/post-office/mailbox.ts`).
//!
//! A character's mailbox is the root-level `Mail/` folder in that character's
//! database-backed vault; one Markdown file per letter, delivery metadata in the
//! frontmatter. These helpers call the document-store service functions directly
//! (not the `doc_*` tool handlers) — every operation is a content read/write or a
//! folder ensure.
//!
//! All functions operate on a **mount-index** `&Connection` (the vault store).
//!
//! Scope: the two mail tool handlers (`send_mail` / `list_email`) use
//! `slugify_sender_name` / `compose_letter_content` / `parse_letter` /
//! `build_reply_preface` / `deliver_letter` / `read_letter` / `list_mailbox`. The
//! Suparṇā mail-check helpers (`collect_unalerted_mail` / `mark_alerted`) drive the
//! Commonplace-time mail whisper — the READ half of the Suparṇā feeder (W4.6a);
//! the whisper POST is W4.6b.

use rusqlite::Connection;
use serde_json::{json, Map, Value};

use crate::clock::iso_to_ms;
use crate::collation::locale_compare;
use crate::db::database_store::{
    list_database_files, read_database_document, write_database_document, DbStoreErrorCode,
    StoreError,
};
use crate::db::doc_mount_file_links::DocMountFileLinksRepository;
use crate::db::DbError;
use crate::doc_edit::markdown_parser::serialize_frontmatter;
use crate::format_time::{format_date_time, MonthStyle};
use crate::markdown::{body_after, parse_frontmatter};

/// Root-level folder, in every character's vault, where letters are delivered.
pub const MAIL_FOLDER: &str = "Mail";

/// The frontmatter stamped on every delivered letter (v4 `MailFrontmatter`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MailFrontmatter {
    pub from: String,
    pub from_character_id: String,
    pub sent_at: String,
    pub alerted: bool,
    pub in_reply_to: Option<String>,
}

/// A parsed letter: its metadata plus the body the recipient should read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedLetter {
    pub frontmatter: MailFrontmatter,
    pub body: String,
}

/// Summary of a delivered letter, used by listings (v4 `DeliveredLetterSummary`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeliveredLetterSummary {
    /// The vault-relative path — also the agent-facing message id.
    pub path: String,
    pub from: String,
    pub sent_at: String,
    pub body: String,
    pub alerted: bool,
    pub in_reply_to: Option<String>,
}

/// Params for [`deliver_letter`] (v4 `DeliverLetterParams`).
pub struct DeliverLetterParams<'a> {
    pub recipient_vault_id: &'a str,
    pub from_name: &'a str,
    pub from_character_id: &'a str,
    pub sent_at: &'a str,
    pub body: &'a str,
    pub in_reply_to: Option<&'a str>,
}

/// Map a [`StoreError`] to a [`DbError`] (NotFound handled by the caller before
/// this is reached; other store errors surface as the message).
fn store_to_db(e: StoreError) -> DbError {
    match e {
        StoreError::Db(db) => db,
        StoreError::Store(s) => DbError::Key(s.message),
    }
}

/// v4 `slugifySenderName`: lowercase, runs of non-alphanumerics → a single hyphen,
/// no leading/trailing hyphen; empty → `"someone"`. JS `\W`-style class is ASCII
/// here (`[^a-z0-9]`), and `.toLowerCase()` matches `str::to_lowercase` (the
/// resolved case-mapping seam).
pub fn slugify_sender_name(name: &str) -> String {
    let lower = name.to_lowercase();
    let lower = lower.trim();
    // Replace runs of non-[a-z0-9] with a single '-'.
    let mut slug = String::with_capacity(lower.len());
    let mut in_run = false;
    for ch in lower.chars() {
        if ch.is_ascii_lowercase() || ch.is_ascii_digit() {
            slug.push(ch);
            in_run = false;
        } else if !in_run {
            slug.push('-');
            in_run = true;
        }
    }
    let slug = slug.trim_matches('-');
    if slug.is_empty() {
        "someone".to_string()
    } else {
        slug.to_string()
    }
}

/// Build the on-disk letter content: frontmatter + a blank line + body (v4
/// `composeLetterContent`).
pub fn compose_letter_content(frontmatter: &MailFrontmatter, body: &str) -> String {
    let mut fm = Map::new();
    fm.insert("from".into(), json!(frontmatter.from));
    fm.insert(
        "fromCharacterId".into(),
        json!(frontmatter.from_character_id),
    );
    fm.insert("sentAt".into(), json!(frontmatter.sent_at));
    fm.insert("alerted".into(), json!(frontmatter.alerted));
    fm.insert(
        "inReplyTo".into(),
        match &frontmatter.in_reply_to {
            Some(s) => json!(s),
            None => Value::Null,
        },
    );
    format!("{}\n{}", serialize_frontmatter(&fm), body)
}

/// Parse a delivered letter's content into structured metadata + body (v4
/// `parseLetter`). Body = the post-frontmatter slice with leading `\n`s stripped.
pub fn parse_letter(content: &str) -> ParsedLetter {
    let parsed = parse_frontmatter(content);
    let data = match &parsed.data {
        Some(Value::Object(m)) => m.clone(),
        _ => Map::new(),
    };
    let str_field = |k: &str| data.get(k).and_then(Value::as_str).map(str::to_string);
    let body = body_after(content, &parsed)
        .trim_start_matches('\n')
        .to_string();
    ParsedLetter {
        frontmatter: MailFrontmatter {
            from: str_field("from").unwrap_or_else(|| "Someone".to_string()),
            from_character_id: str_field("fromCharacterId").unwrap_or_default(),
            sent_at: str_field("sentAt").unwrap_or_default(),
            alerted: data.get("alerted") == Some(&Value::Bool(true)),
            in_reply_to: str_field("inReplyTo"),
        },
        body,
    }
}

/// v4 `buildReplyPreface`: the quoted reply preface from an original letter's body
/// (body only). Each line prefixed `> ` (empty lines → `>`).
pub fn build_reply_preface(original_body: &str, original_sent_at: &str) -> String {
    let when = {
        let formatted = format_date_time(Some(original_sent_at), MonthStyle::Long);
        if formatted.is_empty() {
            "an earlier date".to_string()
        } else {
            formatted
        }
    };
    let quoted = original_body
        .split('\n')
        .map(|line| {
            if line.is_empty() {
                ">".to_string()
            } else {
                format!("> {line}")
            }
        })
        .collect::<Vec<_>>()
        .join("\n");
    format!("> In reply to your letter of {when}:\n>\n{quoted}")
}

/// List the letters (files only, `.md`) in a vault's `Mail/` folder (v4
/// `listMailEntries`). Missing/empty → empty vec.
fn list_mail_entries(mount: &Connection, vault_id: &str) -> Result<Vec<String>, DbError> {
    let entries = list_database_files(mount, vault_id, Some(MAIL_FOLDER))?;
    Ok(entries
        .into_iter()
        .filter(|e| e.kind != "folder" && e.relative_path.to_lowercase().ends_with(".md"))
        .map(|e| e.relative_path)
        .collect())
}

/// Pick a non-colliding `Mail/<epoch>-from-<slug>.md` path (v4 `pickFreeMailPath`).
fn pick_free_mail_path(
    mount: &Connection,
    vault_id: &str,
    epoch_millis: i64,
    from_name: &str,
) -> Result<String, DbError> {
    let slug = slugify_sender_name(from_name);
    let existing: std::collections::HashSet<String> = list_mail_entries(mount, vault_id)?
        .into_iter()
        .map(|p| p.to_lowercase())
        .collect();
    let base = format!("{MAIL_FOLDER}/{epoch_millis}-from-{slug}");
    let mut candidate = format!("{base}.md");
    let mut n = 2;
    while existing.contains(&candidate.to_lowercase()) {
        candidate = format!("{base}-{n}.md");
        n += 1;
    }
    Ok(candidate)
}

/// Deliver a letter into the recipient's `Mail/` folder (v4 `deliverLetter`).
/// Stamps the frontmatter, picks a collision-free path, ensures the folder, writes
/// the file, and returns the delivered vault-relative path.
pub fn deliver_letter(mount: &Connection, params: &DeliverLetterParams) -> Result<String, DbError> {
    // `Number.isFinite(Date.parse(sentAt)) ? Date.parse(sentAt) : 0`.
    let epoch_millis = iso_to_ms(params.sent_at).unwrap_or(0);
    let frontmatter = MailFrontmatter {
        from: params.from_name.to_string(),
        from_character_id: params.from_character_id.to_string(),
        sent_at: params.sent_at.to_string(),
        alerted: false,
        in_reply_to: params.in_reply_to.map(str::to_string),
    };
    let path = pick_free_mail_path(
        mount,
        params.recipient_vault_id,
        epoch_millis,
        params.from_name,
    )?;

    // ensureFolderPath is idempotent; writeDatabaseDocument also creates parents,
    // but v4 calls it explicitly so the folder row exists even before a listing.
    DocMountFileLinksRepository::new(mount)
        .ensure_folder_path(params.recipient_vault_id, MAIL_FOLDER)?;
    write_database_document(
        mount,
        params.recipient_vault_id,
        &path,
        &compose_letter_content(&frontmatter, params.body),
    )
    .map_err(store_to_db)?;
    Ok(path)
}

/// Read a single letter by its `Mail/…` path (v4 `readLetter`). NOT_FOUND → `None`.
pub fn read_letter(
    mount: &Connection,
    vault_id: &str,
    path: &str,
) -> Result<Option<ParsedLetter>, DbError> {
    match read_database_document(mount, vault_id, path) {
        Ok(doc) => Ok(Some(parse_letter(&doc.content))),
        Err(StoreError::Store(e)) if e.code == DbStoreErrorCode::NotFound => Ok(None),
        Err(e) => Err(store_to_db(e)),
    }
}

/// Collect every letter in `vault_id`'s mailbox, newest-first (v4 `listMailbox`).
pub fn list_mailbox(
    mount: &Connection,
    vault_id: &str,
) -> Result<Vec<DeliveredLetterSummary>, DbError> {
    let paths = list_mail_entries(mount, vault_id)?;
    let mut summaries: Vec<DeliveredLetterSummary> = Vec::new();
    for path in paths {
        let Some(letter) = read_letter(mount, vault_id, &path)? else {
            continue;
        };
        summaries.push(DeliveredLetterSummary {
            path,
            from: letter.frontmatter.from,
            sent_at: letter.frontmatter.sent_at,
            body: letter.body,
            alerted: letter.frontmatter.alerted,
            in_reply_to: letter.frontmatter.in_reply_to,
        });
    }
    sort_newest_first(&mut summaries);
    Ok(summaries)
}

/// v4 `collectUnalertedMail`: collect the letters NOT yet announced by Suparṇā,
/// newest-first. Missing/empty mailbox → `[]`, never an error (v4's `listMailbox`
/// tolerates an absent folder). Read on a **mount-index** connection.
pub fn collect_unalerted_mail(
    mount: &Connection,
    vault_id: &str,
) -> Result<Vec<DeliveredLetterSummary>, DbError> {
    let all = list_mailbox(mount, vault_id)?;
    Ok(all.into_iter().filter(|l| !l.alerted).collect())
}

/// v4 `markAlerted`: flip a letter's `alerted` flag to true (a content update; no
/// link/folder GC). A missing letter is a no-op (v4 warns + returns). Runs on a
/// **mount-index writer** connection (it writes).
pub fn mark_alerted(mount: &Connection, vault_id: &str, path: &str) -> Result<(), DbError> {
    // v4 reads then updates; a NOT_FOUND read is a no-op.
    let content = match read_database_document(mount, vault_id, path) {
        Ok(doc) => doc.content,
        Err(StoreError::Store(e)) if e.code == DbStoreErrorCode::NotFound => return Ok(()),
        Err(e) => return Err(store_to_db(e)),
    };
    let mut updates = Map::new();
    updates.insert("alerted".to_string(), Value::Bool(true));
    let updated =
        crate::doc_edit::markdown_parser::update_frontmatter_in_content(&content, &updates, false);
    write_database_document(mount, vault_id, path, &updated).map_err(store_to_db)?;
    Ok(())
}

/// v4 `sortNewestFirst`: newest `sentAt` first; ties (or unparseable dates) fall
/// back to `b.path.localeCompare(a.path)` (descending by path).
fn sort_newest_first(letters: &mut [DeliveredLetterSummary]) {
    letters.sort_by(|a, b| {
        match (iso_to_ms(&a.sent_at), iso_to_ms(&b.sent_at)) {
            (Some(ta), Some(tb)) if ta != tb => tb.cmp(&ta), // tb - ta → newest first
            // v4: `b.path.localeCompare(a.path)`.
            _ => locale_compare(&b.path, &a.path),
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slugify_matches_v4() {
        assert_eq!(slugify_sender_name("Friday"), "friday");
        assert_eq!(slugify_sender_name("Jeeves McTavish"), "jeeves-mctavish");
        assert_eq!(slugify_sender_name("  --Ada!! "), "ada");
        assert_eq!(slugify_sender_name("!!!"), "someone");
        assert_eq!(slugify_sender_name(""), "someone");
    }

    #[test]
    fn compose_and_parse_round_trip() {
        let fm = MailFrontmatter {
            from: "Friday".into(),
            from_character_id: "char-a".into(),
            sent_at: "2026-02-01T09:05:00.000Z".into(),
            alerted: false,
            in_reply_to: None,
        };
        let content = compose_letter_content(&fm, "Dear friend,\n\nHello.");
        assert!(content.starts_with("---\nfrom: Friday\n"));
        let parsed = parse_letter(&content);
        assert_eq!(parsed.frontmatter, fm);
        assert_eq!(parsed.body, "Dear friend,\n\nHello.");
    }

    #[test]
    fn reply_preface_quotes_body() {
        let preface = build_reply_preface("Line one\n\nLine two", "2026-02-01T09:05:00.000Z");
        assert_eq!(
            preface,
            "> In reply to your letter of February 1, 2026 at 09:05 AM:\n>\n> Line one\n>\n> Line two"
        );
    }
}
