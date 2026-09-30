//! The Post Office — mailbox storage layer (v4 `lib/post-office/mailbox.ts`).
//!
//! A character's mailbox is the root-level `Mail/` folder in that character's
//! database-backed vault; one Markdown file per letter, delivery metadata in the
//! frontmatter. These helpers call the document-store service functions directly
//! (not the `doc_*` tool handlers) — every operation but [`discard_letter`] is a
//! content read/write or a folder ensure. `discard_letter` goes through
//! [`delete_database_document`], the same chokepoint `doc_delete_file` uses, whose
//! `delete_with_gc` handles hard-link groups and file-row collection. (v4's module
//! doc adds that the delete is "buffered whole in the forked child and replayed on
//! the parent" — v4 `12c336fad`; that is v4's Node IPC path, not a hunk: here the
//! delete simply runs on the mount writer.)
//!
//! All functions operate on a **mount-index** `&Connection` (the vault store).
//!
//! Scope: the four mail tool handlers (`send_mail` / `list_mail` / `read_mail` /
//! `discard_mail`) use `resolve_mail_path` / `letter_file_name` /
//! `slugify_sender_name` / `compose_letter_content` / `parse_letter` /
//! `build_reply_preface` / `deliver_letter` / `read_letter` / `list_mailbox` /
//! `mark_alerted` / `discard_letter`. The Suparṇā mail-check helpers
//! (`collect_unalerted_mail` / `mark_alerted`) drive the Commonplace-time mail
//! whisper.

use rusqlite::Connection;
use serde_json::{json, Map, Value};

use crate::clock::iso_to_ms;
use crate::collation::locale_compare;
use crate::db::database_store::{
    delete_database_document, read_database_document, write_database_document, DbStoreErrorCode,
    StoreError,
};
use crate::db::doc_mount_file_links::{normalise_relative_path, DocMountFileLinksRepository};
use crate::db::doc_mount_folders::DocMountFoldersRepository;
use crate::db::DbError;
use crate::doc_edit::markdown_parser::serialize_frontmatter;
use crate::format_time::{format_date_time, MonthStyle};
use crate::markdown::{body_after, parse_frontmatter};
use jiff::tz::TimeZone;

/// Root-level folder, in every character's vault, where letters are delivered.
pub const MAIL_FOLDER: &str = "Mail";

/// The tracing target for the module's own lines (v4
/// `createServiceLogger('PostOffice:Mailbox')`, which renders
/// `{ service: 'PostOffice:Mailbox', module: 'service' }`).
pub const LOG_TARGET: &str = "quilltap::post_office::mailbox";

/// Whether `s` starts with `prefix`, ASCII case-insensitively. v4 compares
/// `s.toLowerCase().startsWith(prefix.toLowerCase())` against an ASCII prefix;
/// no non-ASCII character lowercases into an ASCII one here (`İ` → `i̇`, two
/// code points), so the byte compare is exact — and `prefix.len()` is then a
/// char boundary in `s`.
fn starts_with_ascii_ci(s: &str, prefix: &str) -> bool {
    s.len() >= prefix.len() && s.as_bytes()[..prefix.len()].eq_ignore_ascii_case(prefix.as_bytes())
}

/// Whether `s` ends with `suffix`, ASCII case-insensitively (the mirror of
/// [`starts_with_ascii_ci`] for v4's `toLowerCase().endsWith('.md')`).
fn ends_with_ascii_ci(s: &str, suffix: &str) -> bool {
    s.len() >= suffix.len()
        && s.as_bytes()[s.len() - suffix.len()..].eq_ignore_ascii_case(suffix.as_bytes())
}

/// v4 `letterFileName`: a letter's bare file name (`1718370000000-from-ariadne.md`)
/// from its vault-relative `Mail/…` path — the handle characters are given. Strips
/// ONE leading `Mail/` (case-insensitively); anything else comes back unchanged.
pub fn letter_file_name(path: &str) -> &str {
    let prefix = "Mail/";
    if starts_with_ascii_ci(path, prefix) {
        &path[prefix.len()..]
    } else {
        path
    }
}

/// v4 `resolveMailPath`: resolve a character-supplied letter reference to its
/// vault-relative `Mail/…` path. Accepts the bare file name (the canonical
/// handle) and — so a model echoing an older instruction still lands — the
/// `Mail/…` path or its `qtap://self/Mail/…` URI. `.md` is optional. Anything
/// that would escape the `Mail/` folder (sub-paths, a backslash, `.`, `..`)
/// resolves to `None`.
///
/// The ORDER is v4's and load-bearing: JS `trim` → ONE anchored
/// case-insensitive `qtap://self/` strip → leading slashes stripped AFTER the
/// URI strip (so `/qtap://self/Mail/x` keeps its URI and is refused, while
/// `qtap://self//Mail/x` resolves) → ONE `Mail/` strip (so `Mail/Mail/x` still
/// holds a `/` and is refused) → the refusals → `.md` appended unless the name
/// already ends in it case-insensitively (`x.MD` stays).
pub fn resolve_mail_path(reference: &str) -> Option<String> {
    let mut name = crate::jsstr::js_trim(reference);
    // `.replace(/^qtap:\/\/self\//i, '')`
    if starts_with_ascii_ci(name, "qtap://self/") {
        name = &name["qtap://self/".len()..];
    }
    // `.replace(/^\/+/, '')`
    name = name.trim_start_matches('/');
    let prefix = format!("{MAIL_FOLDER}/");
    if starts_with_ascii_ci(name, &prefix) {
        name = &name[prefix.len()..];
    }
    if name.is_empty() || name.contains('/') || name.contains('\\') || name == "." || name == ".." {
        return None;
    }
    if ends_with_ascii_ci(name, ".md") {
        Some(format!("{MAIL_FOLDER}/{name}"))
    } else {
        Some(format!("{MAIL_FOLDER}/{name}.md"))
    }
}

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
        StoreError::Store(s) => DbError::Internal(s.message),
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
/// (body only). Each line prefixed `> ` (empty lines → `>`). The date is v4's
/// zone-less `formatDateTime` — the host's zone in production (P4.119), so a
/// v5-written preface persists the same local time v4 would write.
pub fn build_reply_preface(original_body: &str, original_sent_at: &str, zone: &TimeZone) -> String {
    let when = {
        let formatted = format_date_time(Some(original_sent_at), MonthStyle::Long, zone);
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
/// `listMailEntries` over `listDatabaseFiles(vaultId, { folder: 'Mail' })`).
/// Missing/empty → empty vec.
///
/// v4's two reads under `listDatabaseFiles` are FALLBACK repository reads —
/// `docMountFileLinks.findByMountPointId` and `docMountFolders.
/// findByMountPointId` are each `safeQuery(…, [])`
/// (`doc-mount-file-links.repository.ts:500-507`, `doc-mount-folders.
/// repository.ts:179-188`) — so a failed read logs the repository's ERROR and
/// lists nothing: a broken store reads as an EMPTY postbox, never a throw. v5's
/// shared `list_database_files` propagates, so the Post Office takes the two
/// reads here, each with v4's fallback and line (P4.126). Only the file links
/// can name a letter (`listMailEntries` drops every folder entry); the folder
/// read is kept for its failure line, in v4's order.
fn list_mail_entries(mount: &Connection, vault_id: &str) -> Result<Vec<String>, DbError> {
    let links = DocMountFileLinksRepository::new(mount)
        .find_by_mount_point_id(vault_id)
        .unwrap_or_else(|error| {
            tracing::error!(
                collection = "doc_mount_file_links",
                mountPointId = vault_id,
                error = %error,
                "Error finding file links by mount point ID"
            );
            Vec::new()
        });
    if let Err(error) = DocMountFoldersRepository::new(mount).find_by_mount_point_id(vault_id) {
        tracing::error!(
            collection = "doc_mount_folders",
            mountPointId = vault_id,
            error = %error,
            "Error finding folders by mount point ID"
        );
    }
    // `listDatabaseFiles`' folder filter (`relativePath.startsWith('Mail/')`)
    // then `listMailEntries`' `.md` filter.
    let folder_prefix = format!("{MAIL_FOLDER}/");
    Ok(links
        .into_iter()
        .filter(|l| {
            l.relative_path.starts_with(&folder_prefix)
                && l.relative_path.to_lowercase().ends_with(".md")
        })
        .map(|l| l.relative_path)
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
///
/// v4's document read under `readDatabaseDocument` is a FALLBACK repository
/// read — `docMountDocuments.findByMountPointAndPath` is `withRawDb(null, …)`
/// (`doc-mount-documents.repository.ts:110-134`) — so a failed read logs the
/// repository's ERROR, answers `null`, becomes `readDatabaseDocument`'s
/// NOT_FOUND and so `readLetter`'s `null`: the letter reads as ABSENT, never a
/// throw. v5's shared `read_database_document` propagates, so the repository
/// failure (the `Db` arm — a path refusal stays a throw, as in v4) takes v4's
/// fallback and line here (P4.126).
pub fn read_letter(
    mount: &Connection,
    vault_id: &str,
    path: &str,
) -> Result<Option<ParsedLetter>, DbError> {
    match read_database_document(mount, vault_id, path) {
        Ok(doc) => Ok(Some(parse_letter(&doc.content))),
        Err(StoreError::Store(e)) if e.code == DbStoreErrorCode::NotFound => Ok(None),
        Err(StoreError::Db(error)) => {
            tracing::error!(
                collection = "doc_mount_documents",
                mountPointId = vault_id,
                relativePath = normalise_relative_path(path)?.as_str(),
                error = %error,
                "Error finding document by mount point and path"
            );
            Ok(None)
        }
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
/// link/folder GC). A missing letter is a no-op that warns `markAlerted: letter no
/// longer present` (v4's NOT_FOUND arm). Runs on a **mount-index writer**
/// connection (it writes).
pub fn mark_alerted(mount: &Connection, vault_id: &str, path: &str) -> Result<(), DbError> {
    // v4 reads then updates; a NOT_FOUND read is a warned no-op.
    let content = match read_database_document(mount, vault_id, path) {
        Ok(doc) => doc.content,
        Err(StoreError::Store(e)) if e.code == DbStoreErrorCode::NotFound => {
            tracing::warn!(
                target: LOG_TARGET,
                vaultId = vault_id,
                path = path,
                "markAlerted: letter no longer present"
            );
            return Ok(());
        }
        Err(e) => return Err(store_to_db(e)),
    };
    let mut updates = Map::new();
    updates.insert("alerted".to_string(), Value::Bool(true));
    let updated =
        crate::doc_edit::markdown_parser::update_frontmatter_in_content(&content, &updates, false);
    write_database_document(mount, vault_id, path, &updated).map_err(store_to_db)?;
    Ok(())
}

/// v4 `discardLetter`: discard a letter from a vault. Deletes through
/// [`delete_database_document`] — never a raw link delete — so a hard-linked
/// letter loses only this link, a group of one is dissolved, and the file row
/// and its content are collected once no link remains. `false` when there was
/// no such letter.
///
/// v4 calls `deleteDatabaseDocumentIfExists`, whose NOT_FOUND catch is
/// unreachable from here: `deleteDatabaseDocument` never throws NOT_FOUND
/// (an absent link is its `false` return), and a path [`resolve_mail_path`]
/// produces never trips `normaliseRelativePath`'s `..` refusal. So the plain
/// chokepoint is the whole behaviour — bar its link lookup:
/// `docMountFileLinks.findByMountPointAndPath` is a FALLBACK repository read
/// (`safeQuery(…, null)`, `doc-mount-file-links.repository.ts:514-529`), so a
/// failed lookup logs the repository's ERROR and discards NOTHING (`false`),
/// never a throw. v5's chokepoint propagates, so the Post Office makes that
/// lookup first, with v4's fallback and line (P4.126).
pub fn discard_letter(mount: &Connection, vault_id: &str, path: &str) -> Result<bool, DbError> {
    let rel = normalise_relative_path(path)?;
    let deleted = match DocMountFileLinksRepository::new(mount)
        .find_by_mount_point_and_path(vault_id, &rel)
    {
        Err(error) => {
            tracing::error!(
                collection = "doc_mount_file_links",
                mountPointId = vault_id,
                relativePath = rel.as_str(),
                error = %error,
                "Error finding file link by mount point and path"
            );
            false
        }
        Ok(None) => false,
        Ok(Some(_)) => delete_database_document(mount, vault_id, path)?,
    };
    tracing::debug!(
        target: LOG_TARGET,
        vaultId = vault_id,
        path = path,
        deleted = deleted,
        "discardLetter"
    );
    Ok(deleted)
}

/// v4 `sortNewestFirst`: newest `sentAt` first; ties (or unparseable dates) fall
/// back to `b.path.localeCompare(a.path)` (descending by path).
///
/// ⚠ NOT a total order, and kept that way — it is v4's
/// (`lib/post-office/mailbox.ts:262`). The `Number.isFinite` guard means a
/// letter whose `sentAt` will not parse compares to EVERY other letter by path,
/// while two parseable ones compare by time, so `a < b` (by time), `b > c` and
/// `c > a` (both by path) can form a cycle: letters {t=10,"a"}, {t=5,"z"},
/// {t=None,"m"} do. It needs a malformed/missing `sentAt` to reach — the
/// injector's epsilon rule breaks on ordinary data, this one only on bad
/// frontmatter — but the failure mode is identical: `slice::sort_by` panics
/// (98/120 shuffled 24–128-letter slates in the P4.14 audit probe) where V8
/// silently returns. Hence [`crate::stable_sort::stable_sort_by_unchecked`].
fn sort_newest_first(letters: &mut [DeliveredLetterSummary]) {
    crate::stable_sort::stable_sort_by_unchecked(letters, |a, b| {
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
        let preface = build_reply_preface(
            "Line one\n\nLine two",
            "2026-02-01T09:05:00.000Z",
            &TimeZone::UTC,
        );
        assert_eq!(
            preface,
            "> In reply to your letter of February 1, 2026 at 09:05 AM:\n>\n> Line one\n>\n> Line two"
        );
    }

    /// P4.119 red-first — the 2026-09-29 walk's D2 bytes: a letter sent at
    /// 19:40Z is quoted as `02:40 PM` on a host in `America/Chicago` (CDT),
    /// which is what v4 persists there; a winter letter keeps ITS offset (CST).
    #[test]
    fn reply_preface_dates_in_the_host_zone() {
        let zone = TimeZone::get("America/Chicago").unwrap();
        let preface = build_reply_preface("Hi.", "2026-09-29T19:40:00.000Z", &zone);
        assert_eq!(
            preface,
            "> In reply to your letter of September 29, 2026 at 02:40 PM:\n>\n> Hi."
        );
        let preface = build_reply_preface("Hi.", "2026-01-29T19:40:00.000Z", &zone);
        assert!(
            preface.contains("January 29, 2026 at 01:40 PM"),
            "{preface}"
        );
    }

    fn summary(sent_at: &str, path: &str) -> DeliveredLetterSummary {
        DeliveredLetterSummary {
            path: path.to_string(),
            from: "Friday".to_string(),
            sent_at: sent_at.to_string(),
            body: String::new(),
            alerted: false,
            in_reply_to: None,
        }
    }

    /// The ordinary mailbox — every `sentAt` parses and all differ — is a plain
    /// key comparison. Pins that the P4.14 sort swap left it untouched.
    #[test]
    fn sorts_parseable_letters_newest_first() {
        let mut letters = vec![
            summary("2026-02-01T09:00:00.000Z", "inbox/a.md"),
            summary("2026-02-03T09:00:00.000Z", "inbox/b.md"),
            summary("2026-02-02T09:00:00.000Z", "inbox/c.md"),
        ];
        sort_newest_first(&mut letters);
        let paths: Vec<&str> = letters.iter().map(|l| l.path.as_str()).collect();
        assert_eq!(paths, ["inbox/b.md", "inbox/c.md", "inbox/a.md"]);
    }

    /// P4.14 audit: a letter with unparseable frontmatter `sentAt` makes the
    /// comparator intransitive (it compares to everything by path while the rest
    /// compare by time). `slice::sort_by` panics on a big enough shuffled slate;
    /// `stable_sort_by_unchecked` must not.
    #[test]
    fn survives_letters_with_unparseable_sent_at() {
        let mut letters: Vec<DeliveredLetterSummary> = (0..128)
            .map(|i| {
                let sent_at = if i % 4 == 0 {
                    "not a date".to_string()
                } else {
                    format!("2026-02-{:02}T{:02}:00:00.000Z", (i % 28) + 1, i % 24)
                };
                summary(&sent_at, &format!("inbox/{:03}.md", (i * 37) % 128))
            })
            .collect();

        let mut state: u64 = 0x51ED_2718_2818_2845;
        for i in (1..letters.len()).rev() {
            state = state
                .wrapping_mul(6_364_136_223_846_793_005)
                .wrapping_add(1_442_695_040_888_963_407);
            letters.swap(i, ((state >> 33) as usize) % (i + 1));
        }

        let before = letters.len();
        sort_newest_first(&mut letters);
        assert_eq!(letters.len(), before, "letters lost during the sort");
    }
}
