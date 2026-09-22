//! The `chats` **search & replace** ops (the conversation capstone, sub-unit 6).
//! Ports v4's `ChatSearchReplaceOps`
//! (`lib/database/repositories/chats-search.ops.ts`): `countMessagesWithText`,
//! `findMessagesWithText`, `searchMessagesGlobal`, and `replaceInMessages`.
//!
//! ## The three shapes
//!
//! - **`count`/`find`** walk a single chat's events ([`chats_messages_read::get_messages`]),
//!   filter `type === 'message'`, and substring-match `content.includes(searchText)`.
//!   Both bail to `0`/`[]` (with a warn) when the search text exceeds
//!   [`MAX_SEARCH_QUERY_LENGTH`] (v4's ReDoS-style guard).
//! - **`searchMessagesGlobal`** runs the SQLite branch only (`isSQLiteBackend()` is
//!   always true for us): an FTS5 index probe with an exact-scan fallback, sorted
//!   `createdAt DESC`, capped in SQL. See below.
//! - **`replaceInMessages`** does a literal `split(search).join(replace)` (replace
//!   ALL occurrences) per matching message, `UPDATE`-ing each changed row **through
//!   the compressed-text codec**, so a replacement that pushes the row over the
//!   512-byte floor is stored as v4 stores it. It **never** touches a chat
//!   timestamp (v4 comment: a message edit is not a "new message" for sorting).
//!
//! ## Global search is an INDEX now (v4 `f45a517a9`), and that changed the answers
//!
//! Until `f45a517a9` this was `content LIKE '%…%'` reached through the query
//! translator's `$regex` filter, and v5 reproduced that path byte-for-byte —
//! including its defect. The translator lowered `$regex` to `LIKE` by mangling
//! the regex SOURCE: `source.replace(/\.\*/g,'%').replace(/\./g,'_')`, wrapped
//! `%…%`, with NO `ESCAPE` clause. So `escapeRegex`'s backslashes survived into
//! the pattern as literals and a user's `.` became `_`. **A v5 user searching
//! `Mr. Smith`, `C++`, `foo(bar)` or `$500` got NOTHING, confidently** — found
//! by `/driftcheck` on the dogfood copy, closed here, and the two pins that
//! used to freeze the mangling (`like_pattern_reproduces_v4_mangling`) are
//! retired rather than repaired.
//!
//! What replaces it, from v4:
//!
//! 1. [`fts_query::build_fts_match_expression`] turns the query into a plan: an
//!    FTS5 phrase-with-prefix-star for anything carrying a token of two or more
//!    UTF-16 units, else a `LIKE … ESCAPE '\'` fallback.
//! 2. The FTS plan runs [`FTS_SEARCH_SQL`]: probe `chat_messages_fts MATCH ?`,
//!    join back through the id map, filter by `chatId IN (…)`, order
//!    `createdAt DESC`, `LIMIT ?`.
//! 3. A fallback plan — or an FTS query that THROWS (an instance whose index the
//!    boot reconciler has not reached yet, so `no such table`) — runs
//!    [`LIKE_SEARCH_SQL`]: the eligibility filter plus
//!    `qt_text(mm."content") LIKE ? ESCAPE '\'`. The throw arm warns first.
//! 4. Both shapes are wrapped by [`defer_text_decode`], which is NOT cosmetic:
//!    SQLite puts a query's output columns into the sorter record, so a
//!    `qt_text("content")` in the OUTER select list would be evaluated for every
//!    match before the limit applied. v4 measured 1.2 s vs 86 ms on 142,000 rows
//!    for a common word, identical results, because only 100 rows are ever
//!    decompressed. The inner query therefore carries nothing but the id and the
//!    sort key.
//!
//! Ordering stays `createdAt DESC` — **not** FTS rank — and the cap stays 100
//! (bound as `LIMIT ?`, which v5 already did before v4 caught up).
//!
//! ## Tracked seams
//!
//! - **`.includes` is UTF-16 substring, `str::contains` is byte substring.** For the
//!   ASCII corpus the two agree; on multi-byte data they can diverge at code-unit
//!   boundaries. This affects `count`/`find`/`replace` only — global search no
//!   longer uses substring semantics at all.
//! - **The length gate is `string.length` (UTF-16 code units).** [`utf16_len`]
//!   reproduces it.
//! - **`LIKE` is ASCII-case-insensitive only**, on the fallback path. v4 has
//!   exactly the same fallback and the same `LIKE`, so this is parity rather
//!   than a deferral now; the INDEXED path folds case and diacritics through
//!   `unicode61 remove_diacritics 2` on both sides.

use rusqlite::types::ToSql;
use rusqlite::Connection;
use serde_json::{json, Value};

use super::chat_message_fts::chat_message_fts_eligibility_sql;
use super::chats_messages_read::get_messages;
use super::fts_query::{build_fts_match_expression, escape_like_pattern, FtsQueryPlan};
use super::text_compression::text_to_blob;
use super::DbError;
use crate::jsstr::utf16_len;

/// Maximum allowed search query length before v4 bails (ReDoS guard) —
/// v4 `MAX_SEARCH_QUERY_LENGTH`.
const MAX_SEARCH_QUERY_LENGTH: usize = 1000;

/// The `context` field on every line these ops log.
const LOG_CONTEXT: &str = "db.chats-search";

/// Wrap a matching-ids query so the message text is decoded ONLY for the rows
/// that survive `ORDER BY … LIMIT` (v4 `deferTextDecode`). See the module doc
/// for the measurement that makes this load-bearing rather than tidy.
fn defer_text_decode(matching_ids_sql: &str) -> String {
    format!(
        r#"
    SELECT m."id" AS id, m."chatId" AS chatId, m."role" AS role,
           s."ca" AS createdAt, qt_text(m."content") AS content
      FROM ({matching_ids_sql}) s
      JOIN "chat_messages" m ON m."id" = s."mid"
     ORDER BY s."ca" DESC
  "#
    )
}

/// The indexed path: probe the FTS5 index, then join back for the columns the
/// caller wants (v4 `buildFtsSearchSql`).
///
/// The `chatId IN (…)` list is kept for PARITY with the pre-index query rather
/// than being replaced with a join on `chats.userId`. It binds the same
/// parameters the `$in` filter always did, so nothing about the caller's
/// contract changes; in a single-user instance it is every chat anyway.
fn build_fts_search_sql(chat_id_count: usize) -> String {
    let placeholders = (0..chat_id_count)
        .map(|_| "?")
        .collect::<Vec<_>>()
        .join(", ");
    defer_text_decode(&format!(
        r#"
      SELECT x."messageId" AS mid, mm."createdAt" AS ca
        FROM "chat_messages_fts" f
        JOIN "chat_messages_fts_map" x ON x."ftsId" = f.rowid
        JOIN "chat_messages" mm        ON mm."id" = x."messageId"
       WHERE "chat_messages_fts" MATCH ?
         AND mm."chatId" IN ({placeholders})
       ORDER BY mm."createdAt" DESC
       LIMIT ?
  "#
    ))
}

/// The fallback path: an exact substring scan, for queries FTS cannot answer
/// (all tokens under two characters, or no tokens at all) — and for an FTS
/// query that throws (v4 `buildLikeSearchSql`).
///
/// Slow, correct and rare — and slower still now that the column is
/// compressed, since every row must be decompressed to be compared. That cost
/// is the reason the fallback is reserved for queries the index genuinely
/// cannot serve.
fn build_like_search_sql(chat_id_count: usize) -> String {
    let placeholders = (0..chat_id_count)
        .map(|_| "?")
        .collect::<Vec<_>>()
        .join(", ");
    defer_text_decode(&format!(
        r#"
      SELECT mm."id" AS mid, mm."createdAt" AS ca
        FROM "chat_messages" mm
       WHERE {eligibility}
         AND mm."chatId" IN ({placeholders})
         AND qt_text(mm."content") LIKE ? ESCAPE '\'
       ORDER BY mm."createdAt" DESC
       LIMIT ?
  "#,
        eligibility = chat_message_fts_eligibility_sql("mm")
    ))
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
            tracing::warn!(
                target: "quilltap::db",
                context = LOG_CONTEXT,
                chatId = chat_id,
                queryLength = utf16_len(search_text),
                maxLength = MAX_SEARCH_QUERY_LENGTH,
                "Search text exceeds maximum length",
            );
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
            tracing::warn!(
                target: "quilltap::db",
                context = LOG_CONTEXT,
                chatId = chat_id,
                queryLength = utf16_len(search_text),
                maxLength = MAX_SEARCH_QUERY_LENGTH,
                "Search text exceeds maximum length",
            );
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

    /// `searchMessagesGlobal` (SQLite branch) — the FTS5 index with an exact-scan
    /// fallback, ordered `createdAt DESC` and capped in SQL. Returns
    /// `{ messageId, content, chatId, role, createdAt }` per row. Over-long
    /// search OR empty `chat_ids` → `[]`.
    pub fn search_messages_global(
        &self,
        chat_ids: &[String],
        search_text: &str,
        limit: i64,
    ) -> Result<Vec<Value>, DbError> {
        if utf16_len(search_text) > MAX_SEARCH_QUERY_LENGTH {
            tracing::warn!(
                target: "quilltap::db",
                context = LOG_CONTEXT,
                queryLength = utf16_len(search_text),
                maxLength = MAX_SEARCH_QUERY_LENGTH,
                "Global search text exceeds maximum length",
            );
            return Ok(Vec::new());
        }
        if chat_ids.is_empty() {
            return Ok(Vec::new());
        }

        let plan = build_fts_match_expression(search_text);
        match &plan {
            FtsQueryPlan::Fallback { reason, .. } => tracing::debug!(
                target: "quilltap::db",
                context = LOG_CONTEXT,
                path = plan.kind(),
                tokens = plan.tokens().len(),
                chatCount = chat_ids.len(),
                reason = reason.as_str(),
                "Global message search plan",
            ),
            FtsQueryPlan::Fts { .. } => tracing::debug!(
                target: "quilltap::db",
                context = LOG_CONTEXT,
                path = plan.kind(),
                tokens = plan.tokens().len(),
                chatCount = chat_ids.len(),
                "Global message search plan",
            ),
        }

        let mut rows: Option<Vec<Value>> = None;

        if let FtsQueryPlan::Fts { match_expr, .. } = &plan {
            match self.run_search(&build_fts_search_sql(chat_ids.len()), |params| {
                params.push(match_expr as &dyn ToSql);
                for id in chat_ids {
                    params.push(id as &dyn ToSql);
                }
                params.push(&limit as &dyn ToSql);
            }) {
                Ok(found) => rows = Some(found),
                Err(err) => {
                    // The index is created by the boot reconciler and healed at
                    // every boot, so this should not happen — but a search bar
                    // that returns nothing is a worse failure than a slow one.
                    tracing::warn!(
                        target: "quilltap::db",
                        context = LOG_CONTEXT,
                        error = %err,
                        "FTS message search failed; falling back to an exact scan",
                    );
                    rows = None;
                }
            }
        }

        let rows = match rows {
            Some(found) => found,
            None => {
                let like_pattern = match &plan {
                    FtsQueryPlan::Fallback { like_pattern, .. } => like_pattern.clone(),
                    FtsQueryPlan::Fts { .. } => {
                        format!("%{}%", escape_like_pattern(search_text))
                    }
                };
                self.run_search(&build_like_search_sql(chat_ids.len()), |params| {
                    for id in chat_ids {
                        params.push(id as &dyn ToSql);
                    }
                    params.push(&like_pattern as &dyn ToSql);
                    params.push(&limit as &dyn ToSql);
                })?
            }
        };

        Ok(rows)
    }

    /// Run one of the two search shapes and map its rows the way v4's
    /// `rows.map(...)` does (`content ?? ''`).
    fn run_search<'p>(
        &self,
        sql: &str,
        bind: impl FnOnce(&mut Vec<&'p dyn ToSql>),
    ) -> Result<Vec<Value>, DbError> {
        let mut params: Vec<&dyn ToSql> = Vec::new();
        bind(&mut params);
        let mut stmt = self.conn.prepare(sql)?;
        let rows = stmt.query_map(params.as_slice(), |row| {
            let id: String = row.get(0)?;
            let chat_id: String = row.get(1)?;
            let role: String = row.get(2)?;
            let created_at: String = row.get(3)?;
            // Already decoded by `qt_text()` in the outer select.
            let content: Option<String> = row.get(4)?;
            Ok(json!({
                "messageId": id,
                "content": content.unwrap_or_default(),
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
    /// (replace ALL occurrences) and `UPDATE` the row when it changed. Returns
    /// the updated count. **Does not touch any chat/message timestamp.**
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
            // Through the codec: `chat_messages.content` is a registered
            // compressed column at v4's repository layer (`manager.ts:134-139`),
            // so a replacement that pushes the row over the 512-byte floor is
            // stored as a brotli BLOB on v4's side and must be here too. On the
            // FTS venue this UPDATE also fires the `_au` trigger, which compares
            // DECODED text and so re-tokenizes exactly when the words changed.
            self.conn.execute(
                "UPDATE chat_messages SET content = ?1 WHERE id = ?2",
                rusqlite::params![text_to_blob(&new_content), id],
            )?;
            updated_count += 1;
        }
        // v4 `chats-search.ops.ts:234-248` (`5029075bb`) — a transcript change,
        // and the one message-writing path that does NOT go through the
        // add/update/delete funnel. Without it an open Salon tab would go on
        // being told "unchanged" while every line it is displaying had its text
        // rewritten underneath it.
        //
        // MEASURED at `31436bae4`, because the hunk and v4's own test title
        // disagree at a glance: the announce statement is unconditional *in the
        // diff*, but an `if (updatedCount === 0) return 0;` sits directly above
        // it, so v4's test "says nothing when no message matched" is the true
        // description. The guard is reproduced here rather than the hunk.
        if updated_count == 0 {
            return Ok(0);
        }
        super::chats_messages::ChatMessagesRepository::new(self.conn)
            .announce_transcript_change(chat_id);
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
    use crate::db::chat_message_fts::ensure_chat_message_fts_schema;
    use crate::db::text_compression::register_qt_text;
    use crate::test_support::captured;

    /// The reduced `chat_messages` the two SQL shapes actually name — the same
    /// columns `chat-message-fts.json` carries, so the log pins need no
    /// fixture.
    const REDUCED_DDL: &str = r#"CREATE TABLE "chat_messages" (
        "id" TEXT PRIMARY KEY, "chatId" TEXT NOT NULL, "type" TEXT DEFAULT 'message',
        "role" TEXT, "content" TEXT, "createdAt" TEXT NOT NULL)"#;

    /// An in-memory main db with `qt_text` registered, two eligible messages,
    /// and the FTS objects present only when `indexed`.
    fn db(indexed: bool) -> Connection {
        let conn = Connection::open_in_memory().expect("open");
        register_qt_text(&conn).expect("register qt_text");
        conn.execute_batch(REDUCED_DDL).expect("ddl");
        if indexed {
            ensure_chat_message_fts_schema(&conn).expect("ensure fts");
        }
        for (id, content) in [
            ("m1", "she went for a walk along the promenade"),
            ("m2", "the sidewalk was crowded"),
            ("m3", "a literal 50%_off sign"),
        ] {
            conn.execute(
                r#"INSERT INTO "chat_messages" ("id","chatId","type","role","content","createdAt")
                   VALUES (?,?,'message','USER',?,?)"#,
                rusqlite::params![
                    id,
                    "c1",
                    content,
                    format!("2026-01-01T00:00:0{}.000Z", &id[1..])
                ],
            )
            .expect("seed");
        }
        conn
    }

    fn chats() -> Vec<String> {
        vec!["c1".to_string()]
    }

    /// v4's `Global message search plan` debug on the INDEXED path: four
    /// fields, no `reason`.
    #[test]
    fn the_plan_line_names_the_indexed_path() {
        let conn = db(true);
        let lines = captured(|| {
            ChatSearchRepository::new(&conn)
                .search_messages_global(&chats(), "walk", 100)
                .expect("search");
        });
        let plan: Vec<&String> = lines
            .iter()
            .filter(|l| l.contains("Global message search plan"))
            .collect();
        assert_eq!(plan.len(), 1, "one plan line per search: {lines:?}");
        let line = plan[0];
        assert!(
            line.starts_with("DEBUG quilltap::db"),
            "level/target: {line}"
        );
        assert!(line.contains("path=fts"), "{line}");
        assert!(line.contains("tokens=1"), "{line}");
        assert!(line.contains("chatCount=1"), "{line}");
        assert!(
            !line.contains("reason="),
            "v4 spreads `reason` only on a fallback plan: {line}"
        );
        // The silence leg for the failure warn: an index that IS there says
        // nothing about falling back.
        assert!(
            !lines
                .iter()
                .any(|l| l.contains("falling back to an exact scan")),
            "a healthy index must not warn: {lines:?}"
        );
    }

    /// The same line on a FALLBACK plan, which v4 spreads `reason` onto.
    #[test]
    fn the_plan_line_names_the_fallback_and_its_reason() {
        let conn = db(true);
        let lines = captured(|| {
            ChatSearchRepository::new(&conn)
                .search_messages_global(&chats(), "C++", 100)
                .expect("search");
        });
        let line = lines
            .iter()
            .find(|l| l.contains("Global message search plan"))
            .expect("a plan line");
        assert!(line.contains("path=fallback"), "{line}");
        assert!(line.contains("reason=tokens-too-short"), "{line}");
        // A decided fallback is not a FAILURE, so the warn stays silent.
        assert!(
            !lines
                .iter()
                .any(|l| l.contains("falling back to an exact scan")),
            "a decided fallback must not warn: {lines:?}"
        );
    }

    /// The runtime fallback: an FTS plan against a database with no index at
    /// all warns ONCE with v4's sentence, and still answers.
    #[test]
    fn an_index_that_is_not_there_warns_once_and_still_answers() {
        let conn = db(false);
        let (rows, lines) = crate::test_support::captured_with(|| {
            ChatSearchRepository::new(&conn)
                .search_messages_global(&chats(), "walk", 100)
                .expect("search")
        });
        let warns: Vec<&String> = lines
            .iter()
            .filter(|l| l.contains("FTS message search failed; falling back to an exact scan"))
            .collect();
        assert_eq!(warns.len(), 1, "one warn, not one per row: {lines:?}");
        assert!(warns[0].starts_with("WARN quilltap::db"), "{}", warns[0]);
        assert!(
            warns[0].contains("no such table: chat_messages_fts"),
            "v4 carries the error message: {}",
            warns[0]
        );
        // And the fallback answered — substring semantics, so *sidewalk* too.
        let ids: Vec<&str> = rows
            .iter()
            .map(|r| r["messageId"].as_str().unwrap())
            .collect();
        assert_eq!(ids, ["m2", "m1"], "createdAt DESC over both rows");
    }

    /// The over-length guard on all three ops, with v4's three sentences, and
    /// its silence leg.
    #[test]
    fn the_over_length_guard_warns_and_refuses_on_all_three_ops() {
        let conn = db(true);
        let long = "x".repeat(MAX_SEARCH_QUERY_LENGTH + 1);
        let repo = ChatSearchRepository::new(&conn);

        let lines = captured(|| {
            assert_eq!(repo.count_messages_with_text("c1", &long).unwrap(), 0);
        });
        assert_eq!(lines.len(), 1, "{lines:?}");
        assert!(
            lines[0].contains("Search text exceeds maximum length"),
            "{}",
            lines[0]
        );
        assert!(lines[0].contains("chatId=c1"), "{}", lines[0]);
        assert!(lines[0].contains("queryLength=1001"), "{}", lines[0]);
        assert!(lines[0].contains("maxLength=1000"), "{}", lines[0]);

        let lines = captured(|| {
            assert!(repo
                .find_messages_with_text("c1", &long)
                .unwrap()
                .is_empty());
        });
        assert_eq!(lines.len(), 1, "{lines:?}");
        assert!(
            lines[0].contains("Search text exceeds maximum length"),
            "{}",
            lines[0]
        );

        let lines = captured(|| {
            assert!(repo
                .search_messages_global(&chats(), &long, 100)
                .unwrap()
                .is_empty());
        });
        assert_eq!(lines.len(), 1, "{lines:?}");
        assert!(
            lines[0].contains("Global search text exceeds maximum length"),
            "the global op has its OWN sentence: {}",
            lines[0]
        );
        assert!(
            !lines[0].contains("chatId="),
            "v4's global warn carries no chatId: {}",
            lines[0]
        );
        // The refusal happens BEFORE the plan, so nothing else is said.
        assert!(
            !lines[0].contains("Global message search plan"),
            "{}",
            lines[0]
        );

        // Silence leg: a query within the limit says nothing about length.
        let lines = captured(|| {
            repo.search_messages_global(&chats(), "walk", 100).unwrap();
        });
        assert!(
            !lines.iter().any(|l| l.contains("exceeds maximum length")),
            "a short query must not mention the limit: {lines:?}"
        );
    }

    /// The fallback's `ESCAPE '\'` doing its job end to end: a user-typed `%_`
    /// is a literal, not two wildcards. Under the retired `$regex` path this
    /// query matched every row in the table.
    #[test]
    fn a_user_typed_wildcard_stays_literal_on_the_fallback_path() {
        let conn = db(true);
        let rows = ChatSearchRepository::new(&conn)
            .search_messages_global(&chats(), "%_", 100)
            .expect("search");
        let ids: Vec<&str> = rows
            .iter()
            .map(|r| r["messageId"].as_str().unwrap())
            .collect();
        assert_eq!(ids, ["m3"], "only the row that literally contains `%_`");
    }

    #[test]
    fn utf16_len_counts_code_units() {
        assert_eq!(utf16_len("abc"), 3);
        // An astral char is two UTF-16 code units (matches JS `string.length`).
        assert_eq!(utf16_len("\u{1F600}"), 2);
    }

    /// The pin that REPLACES `like_pattern_reproduces_v4_mangling`, retired with
    /// the `$regex` → `LIKE` seam it froze. The old helper turned `Mr. Smith`
    /// into `%Mr\_ Smith%` with no `ESCAPE` clause and matched nothing; the
    /// fallback pattern now escapes only `\ % _` and declares the escape
    /// character, so a punctuated query is a literal substring search and a
    /// user-typed wildcard stays a literal.
    #[test]
    fn the_retired_regex_mangling_is_gone_from_both_shapes() {
        // What the query translator used to produce, for the record:
        //   like_pattern("a.b") == "%a\\_b%"   (the `.` became `_`)
        //   like_pattern("f(")  == "%f\\(%"    (a stray regex escape)
        //   like_pattern("50%") == "%50%%"     (an unescaped wildcard)
        // What v4 and v5 now produce instead:
        assert_eq!(format!("%{}%", escape_like_pattern("a.b")), "%a.b%");
        assert_eq!(format!("%{}%", escape_like_pattern("f(")), "%f(%");
        assert_eq!(format!("%{}%", escape_like_pattern("50%")), "%50\\%%");

        // And the SQL that consumes it declares the escape character, which the
        // old `LIKE ?` did not — the other half of the same defect.
        let like = build_like_search_sql(2);
        assert!(
            like.contains(r#"qt_text(mm."content") LIKE ? ESCAPE '\'"#),
            "the fallback must declare ESCAPE '\\': {like}"
        );
        // The eligibility filter is single-sourced from the FTS module, so the
        // fallback can never drift from what the index holds.
        assert!(like.contains(&chat_message_fts_eligibility_sql("mm")));
        // Both shapes bind the cap rather than truncating after the fact.
        assert!(like.contains("LIMIT ?"));
        assert!(build_fts_search_sql(1).contains("LIMIT ?"));
    }

    /// The deferred decode is the reason both shapes are built the way they
    /// are: the OUTER select carries `qt_text`, the INNER one carries only the
    /// id and the sort key. A `qt_text` projected inside the matching-ids
    /// subquery would put a decompression into the sorter record for every
    /// match, which is the 1.2 s the module doc records.
    #[test]
    fn the_matching_ids_subquery_never_decodes() {
        for sql in [build_fts_search_sql(3), build_like_search_sql(3)] {
            let (outer, rest) = sql
                .split_once("FROM (")
                .expect("the wrapper opens the subquery with `FROM (`");
            let (inner, _) = rest
                .rsplit_once(") s")
                .expect("the wrapper closes the subquery with `) s`");

            // The outer select is the ONE place the text is decoded.
            assert_eq!(
                outer.matches("qt_text(").count(),
                1,
                "the outer select decodes exactly once: {outer}"
            );
            assert!(
                outer.contains(r#"qt_text(m."content") AS content"#),
                "{outer}"
            );

            // The subquery projects the id and the sort key and nothing else —
            // no decoded column, under any alias.
            assert!(
                !inner.contains("AS content") && !inner.contains(r#"qt_text(m."content")"#),
                "the subquery must not project decoded text: {inner}"
            );
            // The LIKE shape's own PREDICATE decodes (it has to compare text);
            // the FTS shape's subquery decodes nothing at all.
            assert!(
                inner.matches("qt_text(").count() <= 1,
                "at most the fallback predicate may decode inside the subquery: {inner}"
            );
        }
    }
}
