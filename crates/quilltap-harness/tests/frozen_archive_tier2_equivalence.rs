//! Tier-2 differential (P4.D268): the frozen memory archive
//! (`quilltap_core::services::frozen_archive`) vs v4's REAL
//! `lib/memory/frozen-archive-cache.ts` and the job dispatcher's REAL
//! `invalidateFrozenArchivesForCompletedJob`, at `01a83539d` (`3f7320138` F5 +
//! `d58548051`).
//!
//! Both sides run over a COPY of the committed `memories-{main,mount}.db` pair
//! with the memory-tiers columns ensured (v5 `ensure_memory_tiers_columns` / v4
//! `applyMemoryTiersMigration` — idempotent on the widened pair, §R.3), then
//! the spec's `plantSql` (explicit uuids + timestamps, the SAME statements on
//! both sides). The spec's `ops` run in order against ONE process-global
//! cache per side, reset at case start:
//!   * `get` — `get_or_compute_frozen_archive(character, chat, generation,
//!     size, present, now_ms)`: the returned ids in ORDER (digests-first
//!     selection, the SALIENCE fill (P4.D291), the final `localeCompare` sort —
//!     the UPPER-CASE id `…0000000C9999` sorts AFTER `…c0006` there and FIRST
//!     in code-unit order);
//!   * `invalidate` / `invalidateAll`;
//!   * `hook` — `invalidate_frozen_archives_for_completed_job(job_id, type,
//!     payload)`.
//!
//! Per op, every `[FrozenArchive] …` / `Invalidated frozen memory archives
//! after job` line is compared — message, key order, values (v4 renders a
//! number as its JS text, a string bare) — through `global_capture` here and a
//! transport spliced into v4's singleton logger there. The corpus exercises:
//! the 3-per-present and 5-self caps biting, a cold digest + a cold shard never
//! selected, the NULL-tier row v4's `hydrateRawRows` drops, the
//! `reinforcedImportance` tie (id ASC), an effective-weight order that differs
//! from `reinforcedImportance` order, a hit (no line), a generation bump, a
//! size change, a presence change (`[B, A, responder]` ≡ `[A, B]`), two chats
//! of one character then `invalidate` (another character's entry survives),
//! `invalidateAll` twice (the second silent), the LRU walk (64 chats, touch,
//! overflow, hit, miss) and the hook for `MEMORY_HOUSEKEEPING` {per character,
//! user-wide, `dryRun: true`, `characterId: ""`}, `MEMORY_CONSOLIDATION` per
//! character and `MEMORY_EXTRACTION` (no-op).
//!
//! Regenerate the oracle (Node 24, from the v4 checkout or a pinned worktree —
//! the case's header has the same recipe; jest ignores `.claude/` paths, so the
//! case + spec + helper are staged in a /tmp mirror):
//!   N=~/.nvm/versions/node/v24.13.1/bin ; V5W=${V5W:-$HOME/source/quilltap-v5}
//!   TMPO=/tmp/qt-frozen-archive-oracle
//!   rm -rf "$TMPO"; mkdir -p "$TMPO/cases" "$TMPO/fixtures"
//!   cp "$V5W/harness/oracle/cases/frozen-archive-tier2.test.ts" "$TMPO/cases/"
//!   cp "$V5W/harness/oracle/fixtures/frozen-archive-tier2.json" "$TMPO/fixtures/"
//!   cp "$V5W/harness/oracle/fixtures/apply-memory-tiers-migration.ts" "$TMPO/fixtures/"
//!   cd ~/source/quilltap-server
//!   TZ=UTC QT_FIXTURE_MEM_MAIN=$V5W/crates/quilltap-web/tests/fixtures/memories-main.db \
//!   QT_FIXTURE_MEM_MOUNT=$V5W/crates/quilltap-web/tests/fixtures/memories-mount.db \
//!   QT_ORACLE_OUT=/tmp/oracle-frozen-archive-tier2.ndjson \
//!     $N/npx jest --silent --watchman=false --testTimeout=180000 \
//!       --roots "$PWD" --roots "$TMPO/cases" -- frozen-archive-tier2
//! Run:
//!   QT_ORACLE_FROZEN_ARCHIVE=/tmp/oracle-frozen-archive-tier2.ndjson \
//!     cargo test -p quilltap-harness --test frozen_archive_tier2_equivalence

use std::path::PathBuf;

use quilltap_core::db::runtime::{Db, DbPaths};
use quilltap_core::services::frozen_archive::{
    get_or_compute_frozen_archive, invalidate_all_frozen_archives, invalidate_frozen_archive,
    invalidate_frozen_archives_for_completed_job, reset_frozen_archive_cache_for_tests,
};
use quilltap_core::test_support::global_capture;
use serde::Deserialize;
use serde_json::Value;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Spec {
    test_pepper_base64: String,
    now_ms: f64,
    plant_sql: Vec<String>,
    ops: Vec<Op>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Op {
    kind: String,
    #[serde(default)]
    character_id: Option<String>,
    #[serde(default)]
    chat_id: Option<String>,
    #[serde(default)]
    generation: Option<i64>,
    #[serde(default)]
    size: Option<usize>,
    #[serde(default)]
    present: Option<Vec<String>>,
    /// P4.D291: rows to leave out — computed without touching the cache.
    #[serde(default)]
    exclude: Option<Vec<String>>,
    #[serde(default)]
    job_id: Option<String>,
    #[serde(default, rename = "type")]
    job_type: Option<String>,
    #[serde(default)]
    payload: Option<Value>,
}

/// v4's per-op row.
#[derive(Deserialize)]
struct OracleRow {
    i: usize,
    kind: String,
    returned: Option<Vec<String>>,
    lines: Vec<Value>,
}

/// The messages this family compares (the oracle captures the same set).
const MESSAGES: [&str; 5] = [
    "[FrozenArchive] Composed archive",
    "[FrozenArchive] Computed archive",
    "[FrozenArchive] Invalidated archives for character",
    "[FrozenArchive] Invalidated all archives",
    "Invalidated frozen memory archives after job",
];

fn spec_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../harness/oracle/fixtures/frozen-archive-tier2.json")
}

fn fixtures_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../quilltap-web/tests/fixtures")
}

/// `(level, message, [(key, value)])` out of one captured v5 line.
type Line = (String, String, Vec<(String, String)>);

fn v5_lines(lines: &[String]) -> Vec<Line> {
    lines
        .iter()
        .filter_map(|l| {
            let msg = MESSAGES.iter().find(|m| l.contains(*m))?;
            let (head, rest) = l.split_once(*msg)?;
            let level = head.split(' ').next().unwrap_or("").to_lowercase();
            let fields = rest
                .split(' ')
                .filter(|kv| !kv.is_empty())
                .map(|kv| {
                    let (k, v) = kv
                        .split_once('=')
                        .unwrap_or_else(|| panic!("bad field {kv:?} in {l}"));
                    (k.to_string(), v.to_string())
                })
                .collect();
            Some((level, msg.to_string(), fields))
        })
        .collect()
}

fn v4_lines(lines: &[Value]) -> Vec<Line> {
    lines
        .iter()
        .map(|w| {
            let fields = w["fields"]
                .as_object()
                .expect("fields object")
                .iter()
                .map(|(k, v)| {
                    (
                        k.clone(),
                        match v {
                            Value::String(s) => s.clone(),
                            other => other.to_string(),
                        },
                    )
                })
                .collect();
            (
                w["level"].as_str().unwrap().to_string(),
                w["message"].as_str().unwrap().to_string(),
                fields,
            )
        })
        .collect()
}

#[test]
fn frozen_archive_tier2_matches_oracle() {
    let Ok(oracle_path) = std::env::var("QT_ORACLE_FROZEN_ARCHIVE") else {
        eprintln!("SKIP: set QT_ORACLE_FROZEN_ARCHIVE to the oracle NDJSON (see header).");
        return;
    };
    let spec: Spec = serde_json::from_str(&std::fs::read_to_string(spec_path()).unwrap()).unwrap();
    let oracle: Vec<OracleRow> = std::fs::read_to_string(&oracle_path)
        .unwrap_or_else(|e| panic!("read oracle: {e}"))
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| serde_json::from_str(l).expect("parse oracle row"))
        .collect();
    assert_eq!(
        oracle.len(),
        spec.ops.len(),
        "op-count mismatch — regenerate the oracle NDJSON (an erroring case leaves a stale file)"
    );

    let scratch = tempfile::Builder::new()
        .prefix("qt-frozen-archive-")
        .tempdir()
        .unwrap();
    let main = scratch.path().join("main.db");
    let mount = scratch.path().join("mount.db");
    std::fs::copy(fixtures_dir().join("memories-main.db"), &main).unwrap();
    std::fs::copy(fixtures_dir().join("memories-mount.db"), &mount).unwrap();
    {
        let w = quilltap_core::db::Writer::open_writable(&main, &spec.test_pepper_base64)
            .expect("open the fixture copy writable");
        quilltap_core::test_support::ensure_memory_tiers_columns(w.connection());
        for sql in &spec.plant_sql {
            w.connection()
                .execute_batch(sql)
                .unwrap_or_else(|e| panic!("plant failed: {e}\n{sql}"));
        }
    }
    let db = Db::open(
        DbPaths {
            main,
            mount_index: Some(mount),
            llm_logs: None,
        },
        &spec.test_pepper_base64,
    )
    .expect("open fixture copy");

    reset_frozen_archive_cache_for_tests();
    let mut failures: Vec<String> = Vec::new();
    let mut seen: std::collections::HashMap<&str, usize> = std::collections::HashMap::new();
    let mut hits = 0usize;
    for (i, (op, want)) in spec.ops.iter().zip(&oracle).enumerate() {
        assert_eq!(
            (want.i, want.kind.as_str()),
            (i, op.kind.as_str()),
            "op order mismatch"
        );
        let (returned, lines) = global_capture::capture(|| match op.kind.as_str() {
            "get" => {
                let exclude: Option<std::collections::HashSet<String>> =
                    op.exclude.as_ref().map(|e| e.iter().cloned().collect());
                let archive = get_or_compute_frozen_archive(
                    &db,
                    op.character_id.as_deref().unwrap(),
                    op.chat_id.as_deref().unwrap(),
                    op.generation.unwrap(),
                    op.size,
                    op.present.as_deref().unwrap_or(&[]),
                    spec.now_ms,
                    exclude.as_ref(),
                )
                .unwrap_or_else(|e| panic!("op {i}: {e}"));
                Some(
                    archive
                        .iter()
                        .map(|m| m["id"].as_str().unwrap().to_string())
                        .collect::<Vec<_>>(),
                )
            }
            "invalidate" => {
                invalidate_frozen_archive(op.character_id.as_deref().unwrap());
                None
            }
            "invalidateAll" => {
                invalidate_all_frozen_archives();
                None
            }
            "hook" => {
                invalidate_frozen_archives_for_completed_job(
                    op.job_id.as_deref().unwrap(),
                    op.job_type.as_deref().unwrap(),
                    op.payload.as_ref().unwrap_or(&Value::Null),
                );
                None
            }
            other => panic!("unknown op kind {other}"),
        });
        if returned != want.returned {
            failures.push(format!(
                "op {i} ({}) returned diverges\n  v5 {returned:?}\n  v4 {:?}",
                op.kind, want.returned
            ));
        }
        let (got, wl) = (v5_lines(&lines), v4_lines(&want.lines));
        if got != wl {
            failures.push(format!(
                "op {i} ({}) lines diverge\n  v5 {got:?}\n  v4 {wl:?}",
                op.kind
            ));
        }
        for (_, m, _) in &wl {
            let key = MESSAGES
                .iter()
                .find(|x| *x == m)
                .copied()
                .expect("known message");
            *seen.entry(key).or_default() += 1;
        }
        if op.kind == "get" && wl.is_empty() {
            hits += 1;
        }
    }
    assert!(
        failures.is_empty(),
        "{} op(s) diverge:\n{}",
        failures.len(),
        failures.join("\n")
    );
    // Non-vacuous: every line class appeared, and the cache really hit.
    for m in MESSAGES {
        assert!(
            seen.get(m).copied().unwrap_or(0) > 0,
            "no `{m}` line in the oracle"
        );
    }
    assert!(hits >= 5, "only {hits} cache hits in the corpus");
    eprintln!(
        "OK: frozen-archive tier-2 matched v4 over {} ops.",
        spec.ops.len()
    );
}
