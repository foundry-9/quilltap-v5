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

mod source_census;

use source_census::{
    code_only, contains_word, core_src_root, floor_boundary, production_zone, rust_sources,
    string_literals,
};

/// `(path under `crates/quilltap-core/src`, production `text_to_blob(` calls,
/// why — naming every statement the count covers)`.
///
/// **The arithmetic: 4 + 2 + 4 + 2 + 1 + 1 = 14 production `text_to_blob`
/// calls across six files** (the sixth being the definition itself). P4.105
/// moved `db/chats_messages.rs`'s WRITES (an UPDATE joined the INSERT) but not
/// its count: both statements bind the one member marshaling, so the row stays
/// 4 and the total 14. The
/// `db/chats_search.rs` row is P4.D204's: P4.D203 left that file EXEMPT with a
/// note saying P4.D204 owned the rewrite and would land the codec there, which
/// it has — so the file moves from EXEMPT to CENSUS and the count goes 11 → 12.
/// The `db/llm_logs.rs` row then goes 2 → 4 (and the total 12 → 14) when the
/// dynamic `update` patch's `request`/`response` arms join `create_inner`'s.
const CENSUS: &[(&str, usize, &str)] = &[
    // P4.105 OUT-OF-MANDATE — P4.D203's file by history, nobody's this round
    // (§R.10(j)): the note moves with the port; the count does not.
    (
        "db/chats_messages.rs",
        4,
        "the four member marshalers' codec calls — `message_columns`'s `content` \
         + `opaqueContent`, `context_summary_columns`'s `context`, and \
         `system_columns`'s `description`. Since P4.105 BOTH writes bind \
         through that one marshaling: `insert_event`'s INSERT and \
         `update_message`'s UPDATE (v4's `$set: validated`, which replaced the \
         old DELETE + re-INSERT so the FTS triggers see an UPDATE). The UPDATE \
         is a real write site on all four columns, and it adds NO call of its \
         own — a second set of calls would be exactly the column-list drift \
         the shared marshaler exists to prevent. So 4 stays 4.",
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
        4,
        "`create_inner`'s `request` and `response`, PLUS the dynamic `update` \
         patch's two arms for the same columns. JSON FIRST, then the codec — \
         v4's `documentToRow` runs its compressed branch before its JSON \
         branch for exactly these two columns, which are both; and \
         `translateUpdate` runs `documentToRow` over the patch just as \
         `translateInsert` runs it over a new document, so an update that \
         crosses the floor stores a BLOB rather than laundering the cell back \
         to plaintext.",
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
    // P4.110: a FLOOR on the walk. Arm (a) reads its rows by path, so only arm
    // (b) depends on the walker — and an empty walk makes arm (b) pass
    // vacuously. Measured when the walker moved to `source_census`: a mutation
    // that stopped it matching `.rs` files reddened every other census on the
    // shared walkers and left this one GREEN. (715 files at the lift.)
    assert!(
        files.len() > 500,
        "the walk found only {} rust files under {} — arm (b) would be measuring \
         nothing",
        files.len(),
        root.display()
    );

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
    // this lane to land). Then 12 → 14 when `db/llm_logs.rs`'s dynamic
    // `update` patch routes its `request`/`response` arms through the codec
    // as v4's `translateUpdate` does.
    assert_eq!(
        total, 14,
        "the census totals 14 production `text_to_blob` occurrences \
         (13 call sites + the definition); got {total}"
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

/// The lexer is load-bearing in BOTH directions (see `source_census::production_zone`): it
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
