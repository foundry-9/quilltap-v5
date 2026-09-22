//! Mixed differential test: v4's `ChatSearchReplaceOps` (Phase-2, the
//! conversation capstone, sub-unit 6 — `countMessagesWithText` /
//! `findMessagesWithText` / `searchMessagesGlobal` / `replaceInMessages`),
//! rewritten for `f45a517a9`'s FTS5 index (P4.D204).
//!
//! Both sides run the SAME reads + replaces (`chats-search.json`) on a fresh
//! copy of the seed fixture, in **TWO VENUES**:
//!
//! - **`plain`** — a fixture with NO FTS objects. Every query goes down v4's
//!   `LIKE` fallback: the `fallback` plans by decision, and the `fts` plans by
//!   the RUNTIME fallback v4 takes when the FTS query throws `no such table`.
//!   This is the state a v4 instance is in before its migration runs and a v5
//!   instance is in before its boot reconciler runs, and it is the only way to
//!   test the throw arm without faking a failure.
//! - **`fts`** — the same spec with the index built by v4's own
//!   `ensureChatMessageFtsSchema` + `rebuildChatMessageFtsIndex`. Here the
//!   token semantics actually bite.
//!
//! The contrast between the two venues IS the port's proof, and the corpus is
//! built to make it visible: `walk` finds the *sidewalk* row on `plain` and
//! does NOT on `fts`; `cafe` finds only the unaccented row on `plain` and both
//! on `fts`; `café` the mirror image.
//!
//! ## What the two venues compare
//!
//! Per venue: every read result exactly; every replace count; the reads that
//! can only answer AFTER the replace (the `_au` trigger's own proof on the
//! `fts` venue); the post-replace `chat_messages` dump; and, on `fts`, the
//! whole `chat_messages_fts_map`. No normalization — no op touches a
//! timestamp and every id is pinned.
//!
//! ## What this rewrite CLOSED, and what it retired
//!
//! Before `f45a517a9` this path was the query translator's `$regex` → `LIKE`
//! conversion, and v5 reproduced it byte-for-byte including its defect: a
//! user's `.` became `_` and `escapeRegex`'s backslashes survived into the
//! pattern with no `ESCAPE` clause, so **`Mr. Smith`, `C++`, `foo(bar)` and
//! `$500` returned NOTHING**. The pin that froze that mangling
//! (`like_pattern_reproduces_v4_mangling`) is RETIRED with the helper it
//! pinned; `the_retired_regex_mangling_is_gone_from_both_shapes` replaces it.
//!
//! Red-first, measured at both pins BEFORE the port (the same corpus, the same
//! builder, one regen per pin): exactly TWO reads moved between `baa85e19b`
//! and `f45a517a9` — `searchMessagesGlobal` for `1.5` and for `f(x)`, both
//! `[]` → the row. The replace counts and the whole `chat_messages` dump were
//! byte-identical. The grown corpus below is what turns those two rows into
//! coverage of the whole seam.
//!
//! ## Regenerating (two fixtures, two invocations, ONE NDJSON)
//!
//! ```bash
//!   N=~/.nvm/versions/node/v24.13.1/bin
//!   V5W=/Users/csebold/source/quilltap-v5
//!   cd ~/source/quilltap-server
//!   QT_FIXTURE_OUT=/tmp/qt-chsearch-fixture.db \
//!     $N/npx tsx "$V5W/harness/oracle/fixtures/build-chats-search-fixture.ts"
//!   QT_FIXTURE_FTS=1 QT_FIXTURE_OUT=/tmp/qt-chsearch-fixture-fts.db \
//!     $N/npx tsx "$V5W/harness/oracle/fixtures/build-chats-search-fixture.ts"
//!   QT_VENUE=plain QT_FIXTURE_CHSEARCH=/tmp/qt-chsearch-fixture.db \
//!     $N/npx tsx "$V5W/harness/oracle/cases/chats-search.ts" > /tmp/oracle-chsearch.ndjson
//!   QT_VENUE=fts QT_FIXTURE_CHSEARCH=/tmp/qt-chsearch-fixture-fts.db \
//!     $N/npx tsx "$V5W/harness/oracle/cases/chats-search.ts" >> /tmp/oracle-chsearch.ndjson
//!   QT_ORACLE_CHSEARCH=/tmp/oracle-chsearch.ndjson \
//!   QT_FIXTURE_CHSEARCH=/tmp/qt-chsearch-fixture.db \
//!   QT_FIXTURE_CHSEARCH_FTS=/tmp/qt-chsearch-fixture-fts.db \
//!     cargo test -p quilltap-harness --test chats_search_equivalence -- --nocapture
//! ```

use quilltap_core::db::Writer;
use serde::Deserialize;
use serde_json::Value;

/// Sentinel for the >MAX_SEARCH_QUERY_LENGTH guard (expanded identically both
/// sides — see the oracle case).
const TOO_LONG_SENTINEL: &str = "TOOLONGSEARCHTEXT_REPLACE_AT_RUNTIME";

/// 1001 chars — one over v4's MAX_SEARCH_QUERY_LENGTH (1000).
fn expand_search_text(s: &str) -> String {
    if s == TOO_LONG_SENTINEL {
        "x".repeat(1001)
    } else {
        s.to_string()
    }
}

#[derive(Deserialize)]
struct Spec {
    #[serde(rename = "testPepperBase64")]
    test_pepper_base64: String,
    reads: Vec<ReadOp>,
    replace: Vec<ReplaceOp>,
    #[serde(rename = "postReplaceReads")]
    post_replace_reads: Vec<ReadOp>,
}

#[derive(Deserialize)]
struct ReadOp {
    kind: String,
    #[serde(default, rename = "chatId")]
    chat_id: Option<String>,
    #[serde(default, rename = "chatIds")]
    chat_ids: Option<Vec<String>>,
    #[serde(rename = "searchText")]
    search_text: String,
    #[serde(default)]
    limit: Option<i64>,
}

#[derive(Deserialize)]
struct ReplaceOp {
    #[serde(rename = "chatId")]
    chat_id: String,
    #[serde(rename = "searchText")]
    search_text: String,
    #[serde(rename = "replaceText")]
    replace_text: String,
}

#[derive(Deserialize)]
struct OracleRead {
    kind: String,
    result: Value,
}
#[derive(Deserialize)]
struct OracleReplace {
    #[allow(dead_code)]
    kind: String,
    count: i64,
}
#[derive(Deserialize)]
struct Oracle {
    venue: String,
    reads: Vec<OracleRead>,
    replace: Vec<OracleReplace>,
    #[serde(rename = "postReplaceReads")]
    post_replace_reads: Vec<OracleRead>,
    messages: Value,
    #[serde(rename = "ftsMap")]
    fts_map: Value,
}

fn run_read(writer: &Writer, op: &ReadOp) -> Value {
    let repo = writer.chat_search();
    let search = expand_search_text(&op.search_text);
    match op.kind.as_str() {
        "countMessagesWithText" => {
            let n = repo
                .count_messages_with_text(op.chat_id.as_deref().unwrap(), &search)
                .expect("count_messages_with_text");
            Value::from(n)
        }
        "findMessagesWithText" => Value::Array(
            repo.find_messages_with_text(op.chat_id.as_deref().unwrap(), &search)
                .expect("find_messages_with_text"),
        ),
        "searchMessagesGlobal" => Value::Array(
            repo.search_messages_global(
                op.chat_ids.as_deref().unwrap(),
                &search,
                op.limit.unwrap(),
            )
            .expect("search_messages_global"),
        ),
        other => panic!("unknown read kind: {other}"),
    }
}

fn assert_dump_eq(got: &Value, oracle: &Value, label: &str) {
    assert_eq!(got["table"], oracle["table"], "{label}: table name");
    assert_eq!(
        got["columns"], oracle["columns"],
        "{label}: column set / order"
    );
    assert_eq!(
        got["rows"], oracle["rows"],
        "{label}: row state diverged\n  rust:   {}\n  oracle: {}",
        got["rows"], oracle["rows"]
    );
}

fn spec() -> Spec {
    serde_json::from_str(
        &std::fs::read_to_string(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../harness/oracle/fixtures/chats-search.json"),
        )
        .unwrap_or_else(|e| panic!("read spec: {e}")),
    )
    .expect("parse spec")
}

/// Drive one venue: its own fixture copy, its own reads, replaces and dumps.
fn run_venue(venue: &str, fixture: &str, spec: &Spec, oracle: &Oracle) {
    assert_eq!(
        oracle.venue, venue,
        "oracle line out of step with the venue"
    );

    let work = std::env::temp_dir().join(format!(
        "qt-chsearch-rust-{venue}-{}.db",
        std::process::id()
    ));
    let _ = std::fs::remove_file(&work);
    std::fs::copy(fixture, &work).unwrap_or_else(|e| panic!("copy {venue} fixture: {e}"));

    let writer = Writer::open_writable(&work, &spec.test_pepper_base64)
        .unwrap_or_else(|e| panic!("open {venue} fixture copy: {e}"));

    // 1) Read methods (before any mutation).
    assert_eq!(
        spec.reads.len(),
        oracle.reads.len(),
        "{venue}: read count: spec vs oracle"
    );
    for (i, op) in spec.reads.iter().enumerate() {
        let got = run_read(&writer, op);
        let oracle_read = &oracle.reads[i];
        assert_eq!(
            oracle_read.kind, op.kind,
            "{venue}: read {i}: kind mismatch"
        );
        assert_eq!(
            got,
            oracle_read.result,
            "{venue}: read {i} ({} {:?}): result diverged\n  rust:   {}\n  oracle: {}",
            op.kind,
            op.search_text,
            serde_json::to_string(&got).unwrap(),
            serde_json::to_string(&oracle_read.result).unwrap()
        );
    }

    // 2) Replace ops (mutate chat_messages; no timestamp touched).
    assert_eq!(
        spec.replace.len(),
        oracle.replace.len(),
        "{venue}: replace count: spec vs oracle"
    );
    {
        let repo = writer.chat_search();
        for (i, op) in spec.replace.iter().enumerate() {
            let count = repo
                .replace_in_messages(&op.chat_id, &op.search_text, &op.replace_text)
                .expect("replace_in_messages");
            assert_eq!(
                count, oracle.replace[i].count,
                "{venue}: replace {i}: count diverged"
            );
        }
    }

    // 3) Reads that can only answer AFTER the replace. On the `fts` venue this
    //    is the `_au` trigger firing on v5's OWN update — and one of the
    //    replacements crosses the 512-byte floor, so the cell it re-indexes is
    //    a compressed BLOB v5 wrote through the codec.
    assert_eq!(
        spec.post_replace_reads.len(),
        oracle.post_replace_reads.len(),
        "{venue}: post-replace read count: spec vs oracle"
    );
    for (i, op) in spec.post_replace_reads.iter().enumerate() {
        let got = run_read(&writer, op);
        assert_eq!(
            got,
            oracle.post_replace_reads[i].result,
            "{venue}: post-replace read {i} ({:?}): result diverged\n  rust:   {}\n  oracle: {}",
            op.search_text,
            serde_json::to_string(&got).unwrap(),
            serde_json::to_string(&oracle.post_replace_reads[i].result).unwrap()
        );
    }

    // 4) The dumps.
    let got_messages = writer
        .dump_table_json("chat_messages", "id")
        .expect("dump chat_messages");
    assert_dump_eq(
        &got_messages,
        &oracle.messages,
        &format!("{venue}/chat_messages"),
    );

    if venue == "fts" {
        let got_map = writer
            .dump_table_json("chat_messages_fts_map", "ftsId")
            .expect("dump chat_messages_fts_map");
        assert_dump_eq(
            &got_map,
            &oracle.fts_map,
            &format!("{venue}/chat_messages_fts_map"),
        );
    } else {
        assert!(
            oracle.fts_map.is_null(),
            "the plain venue must carry no map dump"
        );
    }

    let _ = std::fs::remove_file(&work);
}

#[test]
fn chats_search_matches_oracle() {
    let oracle_path = match std::env::var("QT_ORACLE_CHSEARCH") {
        Ok(p) => p,
        Err(_) => {
            eprintln!("SKIP: set QT_ORACLE_CHSEARCH to the oracle NDJSON (see header).");
            return;
        }
    };
    let plain = match std::env::var("QT_FIXTURE_CHSEARCH") {
        Ok(p) => p,
        Err(_) => {
            eprintln!("SKIP: set QT_FIXTURE_CHSEARCH to the plain seed fixture (see header).");
            return;
        }
    };
    let fts = match std::env::var("QT_FIXTURE_CHSEARCH_FTS") {
        Ok(p) => p,
        Err(_) => {
            eprintln!("SKIP: set QT_FIXTURE_CHSEARCH_FTS to the indexed fixture (see header).");
            return;
        }
    };

    let spec = spec();
    let text = std::fs::read_to_string(&oracle_path).unwrap_or_else(|e| panic!("read oracle: {e}"));
    let oracles: Vec<Oracle> = text
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| serde_json::from_str(l).unwrap_or_else(|e| panic!("parse oracle line: {e}")))
        .collect();
    assert_eq!(
        oracles.len(),
        2,
        "the oracle must carry one line per venue — regenerate both invocations"
    );

    for (venue, fixture) in [("plain", plain.as_str()), ("fts", fts.as_str())] {
        let oracle = oracles
            .iter()
            .find(|o| o.venue == venue)
            .unwrap_or_else(|| panic!("no oracle line for venue {venue}"));
        run_venue(venue, fixture, &spec, oracle);
    }

    eprintln!(
        "OK: chats search matched oracle in BOTH venues ({} reads, {} replaces, {} post-replace reads each).",
        spec.reads.len(),
        spec.replace.len(),
        spec.post_replace_reads.len()
    );
}

/// The two venues must actually DISAGREE, or a green differential would prove
/// only that v5 copies whichever path it happens to take. Reads the oracle
/// alone (no fixture, no DB) and asserts the contrasts the corpus was grown
/// for. SKIPs with the oracle unset.
#[test]
fn the_two_venues_disagree_where_token_semantics_bite() {
    let Ok(oracle_path) = std::env::var("QT_ORACLE_CHSEARCH") else {
        eprintln!("SKIP: set QT_ORACLE_CHSEARCH (see header).");
        return;
    };
    let spec = spec();
    let text = std::fs::read_to_string(&oracle_path).expect("read oracle");
    let oracles: Vec<Oracle> = text
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| serde_json::from_str(l).expect("parse oracle line"))
        .collect();
    let plain = oracles.iter().find(|o| o.venue == "plain").expect("plain");
    let fts = oracles.iter().find(|o| o.venue == "fts").expect("fts");

    let hits = |o: &Oracle, query: &str| -> Vec<String> {
        let i = spec
            .reads
            .iter()
            .position(|r| r.search_text == query && r.kind == "searchMessagesGlobal")
            .unwrap_or_else(|| panic!("no searchMessagesGlobal read for {query:?}"));
        o.reads[i]
            .result
            .as_array()
            .expect("array")
            .iter()
            .map(|r| r["messageId"].as_str().unwrap_or_default().to_string())
            .collect()
    };

    // Substring vs token-prefix: `walk` matched *sidewalk* under LIKE and must
    // NOT under the index.
    let walk_plain = hits(plain, "walk");
    let walk_fts = hits(fts, "walk");
    assert!(
        walk_plain.len() > walk_fts.len(),
        "the LIKE venue must find MORE for `walk` (it matches *sidewalk*): \
         plain={walk_plain:?} fts={walk_fts:?}"
    );
    // And the index must still find the word itself.
    assert!(!walk_fts.is_empty(), "the index must find `walk`");

    // Diacritic folding: each spelling finds only its own row under LIKE, and
    // both rows under the index.
    for query in ["cafe", "café"] {
        let p = hits(plain, query);
        let f = hits(fts, query);
        assert_eq!(
            p.len(),
            1,
            "LIKE must find exactly its own spelling for {query:?}: {p:?}"
        );
        assert_eq!(
            f.len(),
            2,
            "the index must fold {query:?} onto both spellings: {f:?}"
        );
    }

    // The punctuated queries the retired `$regex`→`LIKE` path answered with
    // nothing must now answer on BOTH paths — that is the closed defect.
    for query in ["Mr. Smith", "foo(bar)", "$500", "C++"] {
        assert!(
            !hits(plain, query).is_empty(),
            "the fallback must answer {query:?}"
        );
        assert!(
            !hits(fts, query).is_empty(),
            "the indexed path must answer {query:?}"
        );
    }

    // A user-typed wildcard stays literal on the fallback path.
    assert!(
        !hits(plain, "%_").is_empty(),
        "`%_` must match the literal `50%_off` row through ESCAPE"
    );
}
