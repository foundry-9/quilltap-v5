//! The `chats` **search & replace** ops (the conversation capstone, sub-unit 6).
//! Ports v4's `ChatSearchReplaceOps`
//! (`lib/database/repositories/chats-search.ops.ts`): `countMessagesWithText`,
//! `findMessagesWithText`, `searchMessagesGlobal`, and `replaceInMessages`.
//!
//! ## The three shapes
//!
//! - **`count`/`find`** walk a single chat's events ([`chats_messages_read::get_messages`]),
//!   filter `type === 'message'`, and substring-match `content.includes(searchText)`.
//!   Both bail to `0`/`[]` when the search text exceeds [`MAX_SEARCH_QUERY_LENGTH`]
//!   (v4's ReDoS-style guard).
//! - **`searchMessagesGlobal`** runs the SQLite branch only (`isSQLiteBackend()` is
//!   always true for us): a direct `chat_messages` query over a set of chat ids,
//!   `type='message'`, `role IN ('USER','ASSISTANT')`, and a `content` `$regex`
//!   filter, sorted `createdAt DESC`, truncated to `limit`.
//! - **`replaceInMessages`** does a literal `split(search).join(replace)` (replace
//!   ALL occurrences) per matching message, `UPDATE`-ing each changed row. It
//!   **never** touches a chat timestamp (v4 comment: a message edit is not a "new
//!   message" for sorting), so the resulting `chat_messages` dump has ZERO minted
//!   values.
//!
//! ## The `$regex` → SQL `LIKE` seam (reused from `memories`)
//!
//! `searchMessagesGlobal` builds `regex = new RegExp(escapeRegex(searchText), 'i')`
//! and hands it to the SQLite query translator as a `{ $regex }` filter. The
//! translator lowers `$regex` to `LIKE` by mangling the regex **source** (NOT the
//! original text): `source.replace(/\.\*/g,'%').replace(/\./g,'_')`, wrapped
//! `%…%`, with NO `ESCAPE` clause (so the `\(`/`\.` backslashes from `escapeRegex`
//! are matched literally by SQLite `LIKE`). [`like_pattern`] reproduces that exact
//! transformation byte-for-byte — identical to `memories_read`'s private helper —
//! so SQLite (the same engine v4 runs) matches identically. ASCII `LIKE` is
//! case-insensitive by default, matching the regex `'i'` flag for ASCII corpora.
//!
//! ## Tracked seams
//!
//! - **`.includes` is UTF-16 substring, `str::contains` is byte substring.** For the
//!   ASCII corpus the two agree; on multi-byte data they can diverge at code-unit
//!   boundaries. The corpus stays ASCII (see also the length gate below).
//! - **The length gate is `string.length` (UTF-16 code units).** [`utf16_len`]
//!   reproduces it; the corpus uses ASCII so it equals the byte length too.
//! - **`LIKE` is ASCII-case-insensitive only.** The regex `'i'` flag is full
//!   Unicode case folding; a non-ASCII corpus would expose the difference. Kept
//!   ASCII (the same deferral the `memories` `$regex` seam carries).

use rusqlite::types::ToSql;
use rusqlite::Connection;
use serde_json::{json, Value};

use super::chats_messages_read::get_messages;
use super::DbError;

/// Maximum allowed search query length before v4 bails (ReDoS guard) —
/// v4 `MAX_SEARCH_QUERY_LENGTH`.
const MAX_SEARCH_QUERY_LENGTH: usize = 1000;

/// Roles `searchMessagesGlobal` keeps (`role: { $in: ['USER','ASSISTANT'] }`).
const GLOBAL_ROLES: [&str; 2] = ["USER", "ASSISTANT"];

/// UTF-16 code-unit length (v4's `string.length`), for the search-length gate.
fn utf16_len(s: &str) -> usize {
    s.chars().map(char::len_utf16).sum()
}

/// `escapeRegex(input)` — `lib/utils/regex.ts` (inlined in chats-search.ops.ts as
/// `searchText.replace(/[.*+?^${}()|[\]\\]/g, '\\$&')`): prefix each
/// `[.*+?^${}()|[\]\\]` char with a backslash.
fn regex_source_escaped(query: &str) -> String {
    let mut out = String::with_capacity(query.len());
    for c in query.chars() {
        if matches!(
            c,
            '.' | '*' | '+' | '?' | '^' | '$' | '{' | '}' | '(' | ')' | '|' | '[' | ']' | '\\'
        ) {
            out.push('\\');
        }
        out.push(c);
    }
    out
}

/// Reproduce the query translator's `$regex` → `LIKE` conversion on the escaped
/// regex source: `source.replace(/\.\*/g,'%').replace(/\./g,'_')`, wrapped `%…%`.
/// Byte-identical to `memories_read::like_pattern`. No `ESCAPE` clause is used (v4
/// emits a bare `LIKE ?`), so the `\` chars are matched literally by SQLite.
fn like_pattern(query: &str) -> String {
    let src = regex_source_escaped(query);
    let p = src.replace(".*", "%").replace('.', "_");
    format!("%{p}%")
}

/// Repository over a borrowed MAIN-db connection (held by the [`super::Writer`]).
pub struct ChatSearchRepository<'c> {
    conn: &'c Connection,
}

impl<'c> ChatSearchRepository<'c> {
    pub fn new(conn: &'c Connection) -> Self {
        Self { conn }
    }

    /// `countMessagesWithText` — count `type==='message'` events in a chat whose
    /// `content` contains `search_text` (substring). Over-long search → `0`.
    pub fn count_messages_with_text(
        &self,
        chat_id: &str,
        search_text: &str,
    ) -> Result<i64, DbError> {
        if utf16_len(search_text) > MAX_SEARCH_QUERY_LENGTH {
            return Ok(0);
        }
        let messages = get_messages(self.conn, chat_id)?;
        let mut count = 0i64;
        for msg in &messages {
            if is_message_match(msg, search_text) {
                count += 1;
            }
        }
        Ok(count)
    }

    /// `findMessagesWithText` — `{ messageId, content, chatId }` for each
    /// `type==='message'` event in a chat whose `content` contains `search_text`.
    /// Over-long search → `[]`.
    pub fn find_messages_with_text(
        &self,
        chat_id: &str,
        search_text: &str,
    ) -> Result<Vec<Value>, DbError> {
        if utf16_len(search_text) > MAX_SEARCH_QUERY_LENGTH {
            return Ok(Vec::new());
        }
        let messages = get_messages(self.conn, chat_id)?;
        let mut matches = Vec::new();
        for msg in &messages {
            if is_message_match(msg, search_text) {
                let content = msg.get("content").and_then(Value::as_str).unwrap_or("");
                let id = msg.get("id").and_then(Value::as_str).unwrap_or("");
                matches.push(json!({
                    "messageId": id,
                    "content": content,
                    "chatId": chat_id,
                }));
            }
        }
        Ok(matches)
    }

    /// `searchMessagesGlobal` (SQLite branch) — query `chat_messages` across a set
    /// of chat ids for `type='message'`, `role IN ('USER','ASSISTANT')`, and the
    /// `$regex`→`LIKE` content filter; sorted `createdAt DESC`, truncated to
    /// `limit`. Returns `{ messageId, content, chatId, role, createdAt }` per row.
    /// Over-long search OR empty `chat_ids` → `[]`.
    pub fn search_messages_global(
        &self,
        chat_ids: &[String],
        search_text: &str,
        limit: i64,
    ) -> Result<Vec<Value>, DbError> {
        if utf16_len(search_text) > MAX_SEARCH_QUERY_LENGTH {
            return Ok(Vec::new());
        }
        if chat_ids.is_empty() {
            return Ok(Vec::new());
        }

        let pat = like_pattern(search_text);
        let chat_placeholders = (0..chat_ids.len())
            .map(|_| "?")
            .collect::<Vec<_>>()
            .join(", ");
        // `role IN ('USER','ASSISTANT')` — bound as params for parity with the
        // translator (and to keep the SQL injection-free).
        let role_placeholders = (0..GLOBAL_ROLES.len())
            .map(|_| "?")
            .collect::<Vec<_>>()
            .join(", ");
        let sql = format!(
            "SELECT id, content, chatId, role, createdAt FROM chat_messages \
             WHERE chatId IN ({chat_placeholders}) AND type = 'message' \
             AND role IN ({role_placeholders}) AND content LIKE ? \
             ORDER BY createdAt DESC LIMIT ?"
        );

        let mut params: Vec<&dyn ToSql> = Vec::new();
        for id in chat_ids {
            params.push(id as &dyn ToSql);
        }
        for role in &GLOBAL_ROLES {
            params.push(role as &dyn ToSql);
        }
        params.push(&pat as &dyn ToSql);
        // v4 takes `up to limit` rows from the already-sorted result; a SQL `LIMIT`
        // is equivalent (the sort is total once createdAt values are distinct).
        params.push(&limit as &dyn ToSql);

        let mut stmt = self.conn.prepare(&sql)?;
        let rows = stmt.query_map(params.as_slice(), |row| {
            let id: String = row.get(0)?;
            let content: String = row.get(1)?;
            let chat_id: String = row.get(2)?;
            let role: String = row.get(3)?;
            let created_at: String = row.get(4)?;
            Ok(json!({
                "messageId": id,
                "content": content,
                "chatId": chat_id,
                "role": role,
                "createdAt": created_at,
            }))
        })?;
        let mut out = Vec::new();
        for r in rows {
            out.push(r?);
        }
        Ok(out)
    }

    /// `replaceInMessages` (SQLite branch) — for each `type==='message'` event whose
    /// `content` contains `search_text`, compute `content.split(search).join(replace)`
    /// (replace ALL occurrences) and `UPDATE` the row when it changed. Returns the
    /// updated count. **Does not touch any chat/message timestamp.**
    pub fn replace_in_messages(
        &self,
        chat_id: &str,
        search_text: &str,
        replace_text: &str,
    ) -> Result<i64, DbError> {
        let messages = get_messages(self.conn, chat_id)?;
        let mut updated_count = 0i64;
        for msg in &messages {
            if !is_message_match(msg, search_text) {
                continue;
            }
            let content = msg.get("content").and_then(Value::as_str).unwrap_or("");
            // `split(search).join(replace)` = replace ALL occurrences. For a
            // non-empty `search_text` (the only case reaching here — `includes`
            // is true) Rust `str::replace` is the literal-substring replace-all.
            let new_content = content.replace(search_text, replace_text);
            if new_content == content {
                continue;
            }
            let id = msg.get("id").and_then(Value::as_str).unwrap_or("");
            self.conn.execute(
                "UPDATE chat_messages SET content = ?1 WHERE id = ?2",
                rusqlite::params![new_content, id],
            )?;
            updated_count += 1;
        }
        Ok(updated_count)
    }
}

/// True when `msg` is a `type:'message'` event whose `content` contains
/// `search_text` (v4: `msg.type === 'message' && msg.content.includes(searchText)`).
fn is_message_match(msg: &Value, search_text: &str) -> bool {
    if msg.get("type").and_then(Value::as_str) != Some("message") {
        return false;
    }
    msg.get("content")
        .and_then(Value::as_str)
        .map(|c| c.contains(search_text))
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn like_pattern_reproduces_v4_mangling() {
        // Plain text wraps in %…%.
        assert_eq!(like_pattern("hello"), "%hello%");
        // A literal `.` is escaped to `\.`, then `.replace(/\./g,'_')` turns the
        // unescaped `.` into `_` — but the backslash from escapeRegex stays, so
        // the source `\.` becomes `\_` (matched literally by SQLite, no ESCAPE).
        assert_eq!(like_pattern("a.b"), "%a\\_b%");
        // A literal `(` is escaped to `\(` and passes through unchanged.
        assert_eq!(like_pattern("f("), "%f\\(%");
        // `%` is not a regex special, so it survives as a LIKE wildcard.
        assert_eq!(like_pattern("50%"), "%50%%");
    }

    #[test]
    fn utf16_len_counts_code_units() {
        assert_eq!(utf16_len("abc"), 3);
        // An astral char is two UTF-16 code units (matches JS `string.length`).
        assert_eq!(utf16_len("\u{1F600}"), 2);
    }
}
