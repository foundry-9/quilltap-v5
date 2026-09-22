//! The compressed-column WRITE-SITE census (P4.D203 — v4 `186eb09cb` +
//! `f45a517a9`).
//!
//! **v4's write chokepoint is ONE function.** `documentToRow` compresses every
//! registered column on the way to SQLite, so v4 cannot grow a write that
//! bypasses the codec: there is nowhere to put one. v5 has no collection
//! abstraction — its writes are independent hand-written `INSERT`/`UPDATE`
//! statements across a dozen files — so **a write that bypasses the codec is
//! the DEFAULT**, and the next person to add a `chat_messages` insert will bind
//! a `String` without noticing.
//!
//! Nothing else in the repo can see that. A tier-2 differential over a fresh
//! fixture compares v5's dump against v4's, and both sides hold plain text for
//! any row under 512 bytes — which most corpus messages are. The divergence
//! only shows on a long row, in hex, in a family nobody thought to grow. Hence
//! this census: per-file exact counts over the production zone, plus the
//! assertion that the census's file list IS the set of production files
//! writing one of the seven registered columns.
//!
//! The seven columns (v4 `manager.ts:125,134-139` and
//! `llm-logs.repository.ts:42`):
//! `chat_messages.{content,opaqueContent,description,context}`,
//! `conversation_chunks.content`, `llm_logs.{request,response}`.
//!
//! Run standalone:
//!   cargo test -p quilltap-harness --test compressed_column_write_sites_census

use std::path::{Path, PathBuf};

/// `(path under `crates/quilltap-core/src`, production `text_to_blob(` calls,
/// why — naming every statement the count covers)`.
///
/// **The arithmetic: 4 + 2 + 2 + 2 + 1 + 1 = 12 production `text_to_blob`
/// calls across six files** (the sixth being the definition itself). The
/// `db/chats_search.rs` row is P4.D204's: P4.D203 left that file EXEMPT with a
/// note saying P4.D204 owned the rewrite and would land the codec there, which
/// it has — so the file moves from EXEMPT to CENSUS and the count goes 11 → 12.
const CENSUS: &[(&str, usize, &str)] = &[
    (
        "db/chats_messages.rs",
        4,
        "the message insert's `content` + `opaqueContent`, the context-summary \
         insert's `context`, and the system insert's `description`. \
         `update_message` is a DELETE + re-INSERT through `insert_event`, so it \
         is covered by these four and adds none of its own.",
    ),
    (
        "db/conversation_chunks.rs",
        2,
        "`create`'s `content` and the dynamic `update` patch's `content` arm — \
         the patch arm matters on its own, or an upsert over an existing chunk \
         would launder a compressed cell back to plaintext.",
    ),
    (
        "db/llm_logs.rs",
        2,
        "`create_inner`'s `request` and `response`. JSON FIRST, then the codec \
         — v4's `documentToRow` runs its compressed branch before its JSON \
         branch for exactly these two columns, which are both.",
    ),
    (
        "db/avatar_rolls_collapse_heal.rs",
        2,
        "the boot heal's write-back of `content` + `opaqueContent`, each under \
         v4's explicit NULL guard (`content === null ? null : \
         textToBlob(content)`) so a NULL cell stays NULL. Its SELECT reads \
         through `qt_text()`, matching v4's own migration.",
    ),
    // P4.D204 OUT-OF-MANDATE — P4.D203 owns this census file. This row and the
    // EXEMPT row below are the two halves of the handoff P4.D203 WROTE INTO the
    // exemption it is replacing ("P4.D204 owns the FTS5 rewrite of this file
    // and lands them there"). Landed here rather than reported, because leaving
    // the guard red over a file this lane has now fixed would hand the unifier
    // a failure with no owner. Named in the P4.D204 lane record.
    (
        "db/chats_search.rs",
        1,
        "`replace_in_messages`'s `UPDATE chat_messages SET content = ?1` — the \
         search-and-replace write, which v4 makes through its repository layer \
         and so compresses whenever the replacement crosses the 512-byte floor. \
         The file's READS need no codec call: both SQL shapes project \
         `qt_text(m.\"content\")` in the outer select, so the text arrives \
         decoded and binds as a plain `Option<String>`.",
    ),
    (
        "db/text_compression.rs",
        1,
        "the codec's own home — the ONE occurrence is `pub fn text_to_blob(` \
         itself; its production body calls nothing else. Its `#[cfg(test)]` \
         module calls it a dozen times, so this row is also the proof that the \
         test-stripping works: without it the count would run away.",
    ),
];

/// Production files that write a registered column through raw SQL but are NOT
/// expected to call `text_to_blob` — each with its reason, so a future reader
/// cannot mistake an omission for a decision.
const EXEMPT: &[(&str, &str)] = &[(
    // P4.D204 OUT-OF-MANDATE — see the CENSUS note above.
    "db/chat_message_fts.rs",
    "A FALSE POSITIVE the scanner cannot avoid: this file's only `content` \
     write is `INSERT INTO \"chat_messages_fts\"(rowid, content)`, and \
     `chat_messages_fts` is the FTS5 INDEX, not `chat_messages`. Its `content` \
     column is a different column that must hold DECODED text — the value bound \
     there is read through `qt_text(\"content\")` precisely so the index \
     tokenizes words rather than brotli bytes. Routing it through \
     `text_to_blob` would index the compressed bytes and break search, which \
     is the exact failure v4's contentless design exists to prevent.",
)];

/// The codec entry point a write must reach.
///
/// Counted as the identifier followed by `(` **or** `)`, because both call
/// shapes are load-bearing: `text_to_blob(&m.content)` for a required column
/// and `opt.as_deref().map(text_to_blob)` for a nullable one. A naive
/// `text_to_blob(` pattern silently scores the `.map` form as ZERO — which it
/// did on first run, reporting `db/avatar_rolls_collapse_heal.rs` as having no
/// codec call at all when both of its writes go through it.
///
/// An `use …::text_to_blob;` or `{text_to_blob, …}` import is followed by `;`
/// or `,`, so imports are not counted.
const WRITE: &str = "text_to_blob";

/// Occurrences of [`WRITE`] used as a call or passed as a function value.
fn codec_calls(zone: &str) -> usize {
    let mut n = 0usize;
    let mut from = 0usize;
    while let Some(at) = zone[from..].find(WRITE) {
        let abs = from + at;
        let after = abs + WRITE.len();
        if matches!(zone.as_bytes().get(after), Some(b'(') | Some(b')')) {
            n += 1;
        }
        from = after;
    }
    n
}

fn core_src_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|p| p.parent())
        .expect("the harness crate sits two levels under the repo root")
        .join("crates/quilltap-core/src")
}

fn rust_sources(dir: &Path, out: &mut Vec<PathBuf>) {
    let entries = std::fs::read_dir(dir).unwrap_or_else(|e| panic!("read {}: {e}", dir.display()));
    for entry in entries {
        let path = entry.expect("dir entry").path();
        if path.is_dir() {
            rust_sources(&path, out);
        } else if path.extension().and_then(|e| e.to_str()) == Some("rs") {
            out.push(path);
        }
    }
}
/// The file with every `#[cfg(test)]` item removed — and **everything else
/// kept verbatim**, string literals included.
///
/// Two properties that pull in opposite directions, and getting them mixed up
/// cost this census its most important arm:
///
/// 1. **Finding an item's boundary needs a LEXER.** The sibling census
///    (`stream_watchdog_wrap_census.rs`) balances braces by counting `{`/`}`
///    characters, which works because it only ever scans a curated file list.
///    Run over all of `crates/quilltap-core/src` the same counter panics on
///    **ten files** — `cycle_order.rs`, `select_speaker.rs`,
///    `db/chats_read.rs`, `db/fictional_clock_anchor_repair.rs`,
///    `api/generators_wizard.rs`, `generators/llm_json.rs`,
///    `services/chat_events.rs`, `services/agent_mode.rs`,
///    `services/off_scene.rs`, `services/avatar_cache.rs` — every one a test
///    module holding JSON fixture text whose braces sit inside a STRING
///    literal. So [`next_token`] skips strings, raw strings, char literals and
///    both comment forms when hunting for the closing brace.
///
/// 2. **The OUTPUT must keep string literals.** The thing this census searches
///    for — `INSERT INTO chat_messages (… content …)` — *is* a string literal.
///    An earlier draft emitted a space in place of every literal, and its
///    mutation proof duly survived: a brand-new file with an unconverted
///    `chat_messages` insert was not caught, because the insert had been
///    elided before the search ever ran.
///
/// Stripping test items is not optional the other way either: a test module's
/// seeds INSERT into `chat_messages` freely against trigger-less in-memory
/// DDL, and counting those would drown the production signal.
fn production_zone(src: &str) -> String {
    let mut out = String::with_capacity(src.len());
    let mut i = 0usize;
    let mut kept_from = 0usize;
    while i < src.len() {
        if src[i..].starts_with("#[cfg(test)]") {
            out.push_str(&src[kept_from..i]);
            match test_item_end(src, i) {
                Some(end) => {
                    i = end;
                    kept_from = end;
                    continue;
                }
                // Unterminated: keep the rest rather than silently dropping it.
                None => {
                    kept_from = i;
                    break;
                }
            }
        }
        // Step by TOKEN, not by char, so a `#[cfg(test)]` inside a string or a
        // comment is not mistaken for a real attribute.
        let (tok_end, _is_code) = next_token(src, i);
        i = tok_end.max(i + 1);
    }
    out.push_str(&src[kept_from.min(src.len())..]);
    out
}

/// One lexical step from `at`: `(end, is_code)`. A string, raw string, char
/// literal or comment is NOT code; everything else is.
fn next_token(src: &str, at: usize) -> (usize, bool) {
    let rest = &src[at..];
    if rest.starts_with("//") {
        let end = rest.find('\n').map(|n| at + n).unwrap_or(src.len());
        return (end, false);
    }
    if rest.starts_with("/*") {
        let mut depth = 0usize;
        let mut idx = 0usize;
        while idx < rest.len() {
            if rest[idx..].starts_with("/*") {
                depth += 1;
                idx += 2;
            } else if rest[idx..].starts_with("*/") {
                depth -= 1;
                idx += 2;
                if depth == 0 {
                    return (at + idx, false);
                }
            } else {
                idx += next_char_len(rest, idx);
            }
        }
        return (src.len(), false);
    }
    // Raw string: r"…", r#"…"#, br"…", br#"…"#
    let raw_prefix = if rest.starts_with("r\"") || rest.starts_with("r#") {
        Some(1usize)
    } else if rest.starts_with("br\"") || rest.starts_with("br#") {
        Some(2usize)
    } else {
        None
    };
    if let Some(prefix) = raw_prefix {
        let hashes = rest[prefix..].chars().take_while(|c| *c == '#').count();
        let open = prefix + hashes;
        if rest[open..].starts_with('"') {
            let mut close = String::from("\"");
            for _ in 0..hashes {
                close.push('#');
            }
            let body = &rest[open + 1..];
            let end = body
                .find(&close)
                .map(|n| at + open + 1 + n + close.len())
                .unwrap_or(src.len());
            return (end, false);
        }
    }
    if rest.starts_with('"') {
        let mut idx = 1usize;
        while idx < rest.len() {
            let b = rest.as_bytes()[idx];
            if b == b'\\' {
                idx += 1 + next_char_len(rest, idx + 1);
                continue;
            }
            if b == b'"' {
                return (at + idx + 1, false);
            }
            idx += next_char_len(rest, idx);
        }
        return (src.len(), false);
    }
    // A char literal, distinguished from a LIFETIME by its closing quote.
    if rest.starts_with('\'') {
        let idx = if rest.as_bytes().get(1) == Some(&b'\\') {
            // `'\n'`, `'\\'`, `'\u{1f600}'` — scan to the closing quote.
            rest[2..]
                .find('\'')
                .map(|n| 2 + n)
                .unwrap_or(rest.len().min(2))
        } else {
            1 + next_char_len(rest, 1)
        };
        if rest.as_bytes().get(idx) == Some(&b'\'') {
            return (at + idx + 1, false);
        }
        // Otherwise it is a lifetime; fall through as code.
    }
    (at + next_char_len(src, at), true)
}

fn next_char_len(s: &str, at: usize) -> usize {
    s[at..].chars().next().map(char::len_utf8).unwrap_or(1)
}

/// The end byte of the `#[cfg(test)]` item starting at `at` — its balanced
/// body, or the statement's `;` for a brace-less item (a `use`).
fn test_item_end(src: &str, at: usize) -> Option<usize> {
    let mut i = at + "#[cfg(test)]".len();
    let mut brace_start = None;
    while i < src.len() {
        let (end, is_code) = next_token(src, i);
        if is_code {
            match src.as_bytes()[i] {
                b'{' => {
                    brace_start = Some(i);
                    break;
                }
                b';' => return Some(end),
                _ => {}
            }
        }
        i = end;
    }
    let mut i = brace_start?;
    let mut depth = 0usize;
    while i < src.len() {
        let (end, is_code) = next_token(src, i);
        if is_code {
            match src.as_bytes()[i] {
                b'{' => depth += 1,
                b'}' => {
                    depth -= 1;
                    if depth == 0 {
                        return Some(end);
                    }
                }
                _ => {}
            }
        }
        i = end;
    }
    None
}

/// The nearest char boundary at or below `at` — a byte window over Rust source
/// full of em dashes cannot be sliced naively.
fn floor_boundary(s: &str, mut at: usize) -> usize {
    while at > 0 && !s.is_char_boundary(at) {
        at -= 1;
    }
    at
}

/// The production zone with COMMENTS and STRING LITERALS removed — the view
/// [`codec_calls`] counts over.
///
/// Needed because the zone is verbatim (it must be, so arm (b) can read the
/// SQL), which means a prose mention of `text_to_blob()` in a why-comment
/// counts as a call. That inflated `db/avatar_rolls_collapse_heal.rs` from 2 to
/// 3 on first run, the extra being this port's own comment explaining that the
/// heal writes back through `text_to_blob()`.
fn code_only(zone: &str) -> String {
    let mut out = String::with_capacity(zone.len());
    let mut i = 0usize;
    while i < zone.len() {
        let (end, is_code) = next_token(zone, i);
        if is_code {
            out.push_str(&zone[i..end]);
        } else {
            out.push(' ');
        }
        i = end.max(i + 1);
    }
    out
}

/// Every string literal in the zone, body only.
///
/// This is the unit arm (b) tests, and it must be: a byte window around the
/// table name is not a statement. Two false positives proved it on first run —
/// `services/backup/collect.rs`, where `createdAt, updatedAt FROM
/// conversation_chunks` put the substring `UPDATE` in the backward window, and
/// `services/collapse_stale_chat_caches.rs`, whose genuinely-clean `UPDATE
/// chat_messages SET rawResponse = NULL …` sat 900 bytes from a comment reading
/// "(keep content)". A Rust multi-line SQL statement is ONE literal with `\`
/// continuations, so the literal is exactly the right granularity.
fn string_literals(zone: &str) -> Vec<&str> {
    let mut out = Vec::new();
    let mut i = 0usize;
    while i < zone.len() {
        let starts_literal = zone[i..].starts_with('"')
            || zone[i..].starts_with("r\"")
            || zone[i..].starts_with("r#")
            || zone[i..].starts_with("br\"")
            || zone[i..].starts_with("br#");
        let (end, is_code) = next_token(zone, i);
        if !is_code && starts_literal && end > i {
            out.push(&zone[i..end]);
        }
        i = end.max(i + 1);
    }
    out
}

/// Word-bounded `find`: `UPDATE` must not match inside `updatedAt`.
fn contains_word(haystack: &str, needle: &str) -> bool {
    let mut from = 0usize;
    while let Some(at) = haystack[from..].find(needle) {
        let abs = from + at;
        let before_ok = abs == 0
            || !haystack.as_bytes()[abs - 1].is_ascii_alphanumeric()
                && haystack.as_bytes()[abs - 1] != b'_';
        let after = abs + needle.len();
        let after_ok = match haystack.as_bytes().get(after) {
            None => true,
            Some(b) => !b.is_ascii_alphanumeric() && *b != b'_',
        };
        if before_ok && after_ok {
            return true;
        }
        from = after;
    }
    false
}

/// The three tables whose registered columns a write must route through the
/// codec.
const TABLES: &[&str] = &["chat_messages", "conversation_chunks", "llm_logs"];

/// The seven registered column names, as they appear in SQL.
const COLUMNS: &[&str] = &[
    "content",
    "opaqueContent",
    "description",
    "context",
    "request",
    "response",
];

/// Does this production zone hold a SQL statement that writes one of the seven
/// registered columns?
///
/// Three precision rules, each bought with a false positive on first run:
///
/// 1. **Per STRING LITERAL, not per byte window** — see [`string_literals`].
///    A window around the table name swept up neighbouring comments and
///    unrelated statements.
/// 2. **The verb must be ADJACENT to the table name** (within 40 characters,
///    `INSERT INTO "chat_messages"` / `UPDATE chat_messages`). Without this,
///    `tools/definitions/data.rs` flagged: the `run_sql` tool's JSON
///    description mentions `INSERT/UPDATE/DELETE` in one sentence and
///    `chat_messages` in another, inside one literal, and is not SQL at all.
/// 3. **Word-bounded matching** — `updatedAt` must not read as `UPDATE`, which
///    is how `services/backup/collect.rs`'s `createdAt, updatedAt FROM
///    conversation_chunks` flagged.
fn writes_a_registered_column(zone: &str) -> bool {
    for literal in string_literals(zone) {
        if !sql_writes_a_table(literal) {
            continue;
        }
        let names_a_column = COLUMNS.iter().any(|c| {
            literal
                .split(|ch: char| !ch.is_ascii_alphanumeric() && ch != '_')
                .any(|w| w == *c)
        });
        if names_a_column {
            return true;
        }
    }
    false
}

/// Is there an `INSERT INTO <table>` / `UPDATE <table>` in this literal, with
/// the table name within 40 characters of the verb?
fn sql_writes_a_table(literal: &str) -> bool {
    let upper = literal.to_ascii_uppercase();
    for verb in ["INSERT", "UPDATE"] {
        let mut from = 0usize;
        while let Some(at) = upper[from..].find(verb) {
            let abs = from + at;
            let after = abs + verb.len();
            // BOTH sides, or `updatedAt` reads as `UPDATE` — which is exactly
            // how `services/backup/collect.rs` flagged on the run before this
            // one: `… createdAt, updatedAt FROM conversation_chunks …` put a
            // real table name 40 characters past a verb that was not a verb.
            let word_start = abs == 0 || {
                let b = upper.as_bytes()[abs - 1];
                !b.is_ascii_alphanumeric() && b != b'_'
            };
            let word_end = match upper.as_bytes().get(after) {
                None => true,
                Some(b) => !b.is_ascii_alphanumeric() && *b != b'_',
            };
            if word_start && word_end {
                let to = floor_boundary(&upper, (after + 40).min(upper.len()));
                let window = &upper[floor_boundary(&upper, after)..to];
                if TABLES
                    .iter()
                    .any(|t| contains_word(window, &t.to_ascii_uppercase()))
                {
                    return true;
                }
            }
            from = after;
        }
    }
    false
}

#[test]
fn every_production_write_to_a_registered_column_goes_through_the_codec() {
    let root = core_src_root();
    let mut files = Vec::new();
    rust_sources(&root, &mut files);

    let mut problems: Vec<String> = Vec::new();
    let mut total = 0usize;

    // (a) Every census row's count is EXACT.
    for (rel, want, why) in CENSUS {
        let path = root.join(rel);
        let text = std::fs::read_to_string(&path)
            .unwrap_or_else(|e| panic!("the census names {rel}, which must exist: {e}"));
        let zone = production_zone(&text);
        let got = codec_calls(&code_only(&zone));
        total += got;
        if got != *want {
            problems.push(format!(
                "{rel}: {got} production `{WRITE}` call(s), census says {want}\n    ({why})"
            ));
        }
    }

    // (b) The census's file list IS the set of production files writing one.
    //     A new file that writes a registered column must land in CENSUS or
    //     EXEMPT — this is the arm that catches the write nobody thought about.
    let listed: Vec<&str> = CENSUS
        .iter()
        .map(|(r, _, _)| *r)
        .chain(EXEMPT.iter().map(|(r, _)| *r))
        .collect();
    for path in &files {
        let rel = path
            .strip_prefix(&root)
            .expect("every scanned file sits under the core src root")
            .to_string_lossy()
            .replace('\\', "/");
        if listed.contains(&rel.as_str()) {
            continue;
        }
        let text = std::fs::read_to_string(path).expect("readable source");
        let zone = production_zone(&text);
        if writes_a_registered_column(&zone) {
            problems.push(format!(
                "{rel} writes a REGISTERED COMPRESSED COLUMN but is in neither \
                 CENSUS nor EXEMPT — route it through `text_to_blob` and add \
                 the row, or exempt it with a reason"
            ));
        }
    }

    assert!(
        problems.is_empty(),
        "the compressed-column write census moved ({} problem(s)):\n  - {}",
        problems.len(),
        problems.join("\n  - ")
    );
    // P4.D204 OUT-OF-MANDATE — 11 → 12, the arithmetic being
    // 4 + 2 + 2 + 2 + 1 (P4.D203's five files) + 1 (`db/chats_search.rs`'s
    // search-and-replace UPDATE, which P4.D203's own EXEMPT note handed to
    // this lane to land).
    assert_eq!(
        total, 12,
        "the census totals 12 production `text_to_blob` occurrences \
         (11 call sites + the definition); got {total}"
    );
}

/// The EXEMPT list is not a place to hide a real site: every exempt file must
/// still EXIST, so a rename cannot silently retire an exemption.
#[test]
fn every_exempt_file_still_exists() {
    let root = core_src_root();
    for (rel, why) in EXEMPT {
        assert!(
            root.join(rel).is_file(),
            "EXEMPT names {rel}, which no longer exists — re-derive the \
             exemption rather than deleting it ({why})"
        );
    }
}

/// The lexer is load-bearing in BOTH directions (see [`production_zone`]): it
/// must find an item's closing brace past literal braces, and it must hand
/// back the surviving string literals intact. Pin both.
#[test]
fn the_scanner_balances_past_literal_braces_and_keeps_the_sql() {
    let src = concat!(
        "fn keep() {}\n",
        "const SQL: &str = \"INSERT INTO chat_messages (id, content) VALUES (?1, ?2)\";\n",
        "#[cfg(test)]\n",
        "mod tests {\n",
        "    const J: &str = \"{\\\"a\\\": {\\\"b\\\": 1}\";\n",
        "    const R: &str = r#\"{ \"unbalanced\": { \"#;\n",
        "    const C: char = '}';\n",
        "    // a stray } in a comment\n",
        "    /* and /* nested */ { */\n",
        "    fn t() { let _ = \"}\"; }\n",
        "}\n",
        "fn also_keep() {}\n",
    );
    let zone = production_zone(src);
    // (1) the test item is gone, braces inside its literals notwithstanding
    assert!(
        !zone.contains("mod tests"),
        "the test module survived: {zone}"
    );
    assert!(!zone.contains("fn t()"), "the test body survived: {zone}");
    // (2) the scanner did not stop early at an unbalanced literal brace
    assert!(zone.contains("fn keep()"), "zone: {zone}");
    assert!(
        zone.contains("fn also_keep()"),
        "the scanner stopped early: {zone}"
    );
    // (3) the SQL — a string literal — is STILL THERE, which is the whole
    //     point: arm (b) searches inside it.
    assert!(
        writes_a_registered_column(&zone),
        "the surviving SQL literal must still be findable: {zone}"
    );
}
