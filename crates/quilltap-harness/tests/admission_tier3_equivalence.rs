//! Tier-3 → tier-2 differential: ADMISSION (`quilltap_core::services::
//! admission` — v4 `lib/memory/admission/` at the `83334b9a5` pin; P4.D290,
//! replacing the retired `memory_gate_tier3` family, whose subject v4 deleted
//! in `8af764514`).
//!
//! Both sides run the same corpus (`harness/oracle/fixtures/admission-tier3.
//! json`) over a copy of the same seed pair (`build-admission-fixture.ts`):
//! the oracle drives v4's REAL `admitRow` / `admitCandidate` / `admitPrepared`
//! with only `generateEmbeddingForUser` pinned, and EMITS the exact texts its
//! real `embeddingTextFor` produced with the vector it answered; this test
//! builds a `CannedEmbeddingProvider` from those lines (never re-derived) and
//! drives `admit_row` / `admit_candidate` step for step. Per step it compares:
//!
//!  - every `AdmissionOutcome` (the action BYTES via `to_string`, the rest —
//!    the memory row without its vector, `novelDetails`, `linkedIds`, the
//!    trace incl. `notes` / `searchedK` / `orphansIgnored` /
//!    `embeddingAttempts` — structurally, numbers within 1e-12);
//!  - the embedding calls, in order (`text`, `lexicalText`, `priority`);
//!  - the lines admission owns (its three `module` bindings and the root
//!    `[Admission]` lines — §R.5), each rendered `LEVEL message k=v …` with
//!    v4's camelCase keys, an array as `<key>Json` (the file-layer
//!    convention), `null` as the bare word, minted ids tokenised;
//!  - the watermark calls against the PURE invariant (LR-5: one call with
//!    `[characterId]` per inserting outcome that wrote — not a dry run, not
//!    a repeat the ledger answered; the call itself lands at unification,
//!    §R.10(h));
//!  - the rate counts (the AUTO-only, non-backfill, non-dry-run rule).
//!
//! and after each leg the `memories` table (`SELECT *` ordered by content,
//! id): a minted id is tokenised by its row's content, a wall-clock stamp
//! (`createdAt` / `updatedAt` / `lastReinforcedAt` / `lastAccessedAt` not
//! planted by the corpus) placeholdered, the embedding BLOB compared as hex.
//!
//! Two LEGS: `utc` (the instance has no zone; the host fallback is `UTC`) and
//! `chicago` (`chat_settings.timezone` set first — the anchor line and the
//! fallback `when` phrase move). Tier-1 rows ride the utc leg:
//! `admission_content_key` over a corpus (LR-7 — control characters, a
//! non-BMP character, U+2028) and the key scope's minted keys (LR-1).
//!
//! Ruled / measured divergences: none (`FIND_BY_IDS_UNVALIDATED` retired by
//! P4.168 — the `invalid_row_neighbour` step compares like every other).
//!
//! Recipe (canonical; outputs under `/tmp/p4d290/` while the lane runs):
//!   N=$HOME/.nvm/versions/node/v24.13.1/bin
//!   V5W=${V5W:-$HOME/source/quilltap-v5}
//!   TMPO=/tmp/qt-admission-tier3-oracle
//!   rm -rf "$TMPO"; mkdir -p "$TMPO/harness/oracle/cases" "$TMPO/harness/oracle/fixtures"
//!   cp "$V5W/harness/oracle/cases/admission-tier3.test.ts" "$TMPO/harness/oracle/cases/"
//!   cp "$V5W/harness/oracle/fixtures/admission-tier3.json" "$TMPO/harness/oracle/fixtures/"
//!   cd ~/source/quilltap-server
//!   rm -f /tmp/qt-admission-main.db /tmp/qt-admission-mount.db
//!   QT_FIXTURE_OUT=/tmp/qt-admission-main.db QT_FIXTURE_MOUNT_OUT=/tmp/qt-admission-mount.db \
//!     PATH=$N:$PATH npx tsx "$V5W/harness/oracle/fixtures/build-admission-fixture.ts"
//!   for LEG in utc chicago; do rm -f /tmp/oracle-admission-$LEG.ndjson; \
//!     TZ=UTC QT_ADMISSION_LEG=$LEG QT_FIXTURE_ADMISSION_MAIN=/tmp/qt-admission-main.db \
//!     QT_FIXTURE_ADMISSION_MOUNT=/tmp/qt-admission-mount.db QT_ORACLE_OUT=/tmp/oracle-admission-$LEG.ndjson \
//!     PATH=$N:$PATH npx jest --silent --testTimeout=180000 --roots "$PWD" --roots "$TMPO/harness/oracle/cases" -- admission-tier3; done
//! Run:
//!   QT_ORACLE_ADMISSION_UTC=/tmp/oracle-admission-utc.ndjson \
//!   QT_ORACLE_ADMISSION_CHICAGO=/tmp/oracle-admission-chicago.ndjson \
//!   QT_FIXTURE_ADMISSION_MAIN=/tmp/qt-admission-main.db \
//!   QT_FIXTURE_ADMISSION_MOUNT=/tmp/qt-admission-mount.db \
//!     cargo test -p quilltap-harness --test admission_tier3_equivalence

mod admission_support;

use std::collections::HashSet;
use std::sync::{Arc, Mutex};

use admission_support::*;
use quilltap_core::admission::{admission_inserted, AdmissionOutcome};
use quilltap_core::db::runtime::{Db, DbPaths};
use quilltap_core::memory_candidate::{parse_memory_candidate, ResolvedCandidateTime};
use quilltap_core::services::admission::{
    admission_content_key, admit_candidate, admit_row, count_recent_admissions,
    create_dry_run_book, reset_admission_state_for_tests, reset_rate_ledger_for_tests,
    AdmissionKeyScope, AdmitOptions,
};
use quilltap_core::services::memory_time_zone::reset_memory_time_zone_cache_for_tests;
use serde_json::{json, Value};

const HOST_ZONE: &str = "UTC";
const OWNED_MODULES: &[&str] = &[
    "memory:admission",
    "memory:admission:neighbours",
    "memory:admission:apply",
];

// `FIND_BY_IDS_UNVALIDATED` RETIRED (P4.168, survey C-1): v5's
// `memories_read::find_by_ids` now runs every row through `MemorySchema` as
// v4's `findByFilter` `validateSafe` does, so the `invalid_row_neighbour`
// step's schema-invalid row (a non-UUID id the index still loads) is an
// ORPHAN on both sides and the step compares like every other.

/// The note a ledger-answered repeat carries (v4 `admitPrepared`).
const LEDGER_NOTE: &str = "already admitted by an earlier attempt of this job";

// ---------------------------------------------------------------------------
// One leg
// ---------------------------------------------------------------------------

struct LegResult {
    steps: Vec<(String, Value)>,
    dump: Value,
}

async fn run_leg(
    spec: &Value,
    leg: &Value,
    oracle: &[Value],
    paths: DbPaths,
    logs: &Arc<Mutex<Vec<String>>>,
) -> LegResult {
    let pepper = spec["testPepperBase64"].as_str().unwrap();
    let user_id = spec["userId"].as_str().unwrap();
    let db = Db::open(paths, pepper).expect("open fixture copy");
    reset_admission_state_for_tests();
    reset_rate_ledger_for_tests();
    reset_memory_time_zone_cache_for_tests();
    if let Some(tz) = leg["timezone"].as_str() {
        let sql = format!("UPDATE chat_settings SET timezone = '{tz}'");
        db.write(move |w| {
            w.main().connection().execute(&sql, [])?;
            Ok(())
        })
        .await
        .expect("plant the instance timezone");
    }
    let provider = provider_from(oracle);
    let opts = |dry_run: bool, backfill: bool| AdmitOptions {
        user_id,
        dry_run,
        backfill,
        dry_run_book: None,
        key_scope: None,
        host_zone_fallback: HOST_ZONE,
    };

    let mut steps = Vec::new();
    for step in leg["steps"].as_array().unwrap() {
        provider.calls.lock().unwrap().clear();
        logs.lock().unwrap().clear();
        let name = step["name"].as_str().unwrap().to_string();
        let mut outcomes: Vec<AdmissionOutcome> = Vec::new();
        let mut dry_flags: Vec<bool> = Vec::new();
        let mut counts: Option<Vec<usize>> = None;
        match step["op"].as_str().unwrap() {
            "admit" => {
                let dry = step["dryRun"].as_bool().unwrap_or(false);
                let backfill = step["backfill"].as_bool().unwrap_or(false);
                let o = admit_row(&db, &provider, row_of(&step["row"]), opts(dry, backfill))
                    .await
                    .unwrap_or_else(|e| panic!("{name}: {e:?}"));
                outcomes.push(o);
                dry_flags.push(dry);
            }
            "candidate" => {
                let candidate = parse_memory_candidate(&step["candidate"])
                    .unwrap_or_else(|e| panic!("{name}: candidate refused: {e:?}"));
                let resolved = ResolvedCandidateTime {
                    occurred_at: opt_str(&step["resolved"], "occurredAt"),
                    narrative_time: opt_str(&step["resolved"], "narrativeTime"),
                    source_message_timestamp: opt_str(&step["resolved"], "sourceMessageTimestamp"),
                };
                let o = admit_candidate(&db, &provider, &candidate, &resolved, opts(false, false))
                    .await
                    .unwrap_or_else(|e| panic!("{name}: {e:?}"));
                outcomes.push(o);
                dry_flags.push(false);
            }
            "concurrent" => {
                let rows = step["rows"].as_array().unwrap();
                let (a, b) = tokio::join!(
                    admit_row(&db, &provider, row_of(&rows[0]), opts(false, false)),
                    admit_row(&db, &provider, row_of(&rows[1]), opts(false, false)),
                );
                outcomes.push(a.expect("concurrent a"));
                outcomes.push(b.expect("concurrent b"));
                dry_flags.extend([false, false]);
            }
            "attempt" => {
                let scope = AdmissionKeyScope::new(step["jobId"].as_str().unwrap());
                for r in step["rows"].as_array().unwrap() {
                    let o = admit_row(
                        &db,
                        &provider,
                        row_of(r),
                        AdmitOptions {
                            key_scope: Some(&scope),
                            ..opts(false, false)
                        },
                    )
                    .await
                    .unwrap_or_else(|e| panic!("{name}: {e:?}"));
                    outcomes.push(o);
                    dry_flags.push(false);
                }
            }
            "dryRunBook" => {
                let mut book = create_dry_run_book(
                    step["retiring"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .map(|x| x.as_str().unwrap().to_string()),
                );
                for r in step["rows"].as_array().unwrap() {
                    let o = admit_row(
                        &db,
                        &provider,
                        row_of(r),
                        AdmitOptions {
                            dry_run_book: Some(&mut book),
                            ..opts(true, false)
                        },
                    )
                    .await
                    .unwrap_or_else(|e| panic!("{name}: {e:?}"));
                    outcomes.push(o);
                    dry_flags.push(true);
                }
            }
            "rateCounts" => {
                let now = quilltap_core::clock::now_unix_ms() as f64;
                counts = Some(
                    step["characterIds"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .map(|id| count_recent_admissions(id.as_str().unwrap(), 0.0, now))
                        .collect(),
                );
            }
            other => panic!("unknown op {other}"),
        }
        // LR-5: the watermark call is the unifier's; pin the invariant it
        // must satisfy — one `[characterId]` per non-dry-run inserting outcome.
        let watermark: Vec<Value> = outcomes
            .iter()
            .zip(&dry_flags)
            // A repeat the ledger answers returns before the call (it wrote
            // nothing).
            .filter(|(o, dry)| {
                !**dry
                    && admission_inserted(&o.action)
                    && o.trace.notes.last().map(String::as_str) != Some(LEDGER_NOTE)
            })
            .map(|(o, _)| json!([o.memory.as_ref().unwrap()["characterId"].as_str().unwrap()]))
            .collect();
        let mut out = json!({
            "outcomes": outcomes.iter().map(|o| serde_json::to_value(o).unwrap()).collect::<Vec<_>>(),
            "actionBytes": outcomes.iter().map(|o| serde_json::to_string(&o.action).unwrap()).collect::<Vec<_>>(),
            "embedCalls": provider.calls.lock().unwrap().clone(),
            "lines": logs.lock().unwrap().clone(),
            "watermarkCalls": watermark,
        });
        if let Some(c) = counts {
            out["counts"] = json!(c);
        }
        steps.push((name, out));
    }
    let dump = db
        .read_main(|c| {
            let columns: Vec<String> = c
                .prepare("PRAGMA table_info(memories)")?
                .query_map([], |r| r.get::<_, String>(1))?
                .collect::<Result<_, _>>()?;
            let mut stmt = c.prepare("SELECT * FROM memories ORDER BY content, id")?;
            let n = columns.len();
            let rows: Vec<Value> = stmt
                .query_map([], |r| {
                    let mut m = serde_json::Map::new();
                    for (i, col) in columns.iter().enumerate().take(n) {
                        let v = match r.get_ref(i)? {
                            rusqlite::types::ValueRef::Null => Value::Null,
                            rusqlite::types::ValueRef::Integer(x) => json!(x),
                            rusqlite::types::ValueRef::Real(x) => json!(x),
                            rusqlite::types::ValueRef::Text(t) => {
                                Value::String(String::from_utf8_lossy(t).into_owned())
                            }
                            rusqlite::types::ValueRef::Blob(b) => Value::String(hex::encode(b)),
                        };
                        m.insert(col.clone(), v);
                    }
                    Ok(Value::Object(m))
                })?
                .collect::<Result<_, _>>()?;
            Ok(json!({ "columns": columns, "rows": rows }))
        })
        .expect("dump memories");
    drop(db);
    LegResult { steps, dump }
}

fn compare_leg(spec: &Value, leg: &Value, oracle: &[Value], got: &LegResult) {
    let leg_name = leg["name"].as_str().unwrap();
    let seeded: HashSet<String> = spec["seeds"]
        .as_array()
        .unwrap()
        .iter()
        .map(|s| s["id"].as_str().unwrap().to_string())
        .collect();
    let want_dump = oracle
        .iter()
        .find(|l| l["kind"] == "dump")
        .expect("oracle dump");
    let mut planted = HashSet::new();
    planted_stamps(spec, &mut planted);
    let norm_got = Norm {
        minted: minted_ids(&got.dump, &seeded),
        planted: planted.clone(),
    };
    let norm_want = Norm {
        minted: minted_ids(want_dump, &seeded),
        planted,
    };

    // --- the dump ---------------------------------------------------------
    assert_eq!(
        got.dump["columns"], want_dump["columns"],
        "{leg_name}: memories column set / order"
    );
    let rows_got: Vec<Value> = got.dump["rows"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| norm_got.row(r))
        .collect();
    let rows_want: Vec<Value> = want_dump["rows"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| norm_want.row(r))
        .collect();

    // --- the steps ----------------------------------------------------------
    let want_steps: Vec<&Value> = oracle.iter().filter(|l| l["kind"] == "step").collect();
    assert_eq!(want_steps.len(), got.steps.len(), "{leg_name}: step count");
    for ((name, g), w) in got.steps.iter().zip(want_steps) {
        assert_eq!(w["name"].as_str(), Some(name.as_str()), "step order");
        // Embedding calls, in order (v4's absent priority is interactive).
        let want_calls: Vec<Value> = w["embedCalls"]
            .as_array()
            .unwrap()
            .iter()
            .map(|c| {
                let mut c = c.clone();
                if c["priority"].is_null() {
                    c["priority"] = json!("interactive");
                }
                c
            })
            .collect();
        assert_eq!(
            g["embedCalls"].as_array().unwrap(),
            &want_calls,
            "{leg_name}/{name}: embedding calls"
        );
        // Rate counts.
        assert_eq!(
            g.get("counts"),
            w.get("counts"),
            "{leg_name}/{name}: counts"
        );
        let want_out = w["outcomes"].as_array().unwrap();
        let got_out = g["outcomes"].as_array().unwrap();
        assert_eq!(
            want_out.len(),
            got_out.len(),
            "{leg_name}/{name}: outcome count"
        );
        let action_bytes: Vec<String> = want_out
            .iter()
            .map(|o| norm_want.ids(&serde_json::to_string(&o["action"]).unwrap()))
            .collect();
        let got_bytes: Vec<String> = g["actionBytes"]
            .as_array()
            .unwrap()
            .iter()
            .map(|b| norm_got.ids(b.as_str().unwrap()))
            .collect();
        assert_eq!(got_bytes, action_bytes, "{leg_name}/{name}: action bytes");
        for (i, (go, wo)) in got_out.iter().zip(want_out).enumerate() {
            let (go, wo) = (norm_got.value(go, None), norm_want.value(wo, None));
            assert!(
                same(&go, &wo),
                "{leg_name}/{name}[{i}]: outcome diverged\n  rust:   {go}\n  oracle: {wo}"
            );
        }
        // The lines admission owns.
        let want_lines: Vec<String> = w["lines"]
            .as_array()
            .unwrap()
            .iter()
            .map(|l| norm_want.ids(&render_v4(l)))
            .collect();
        let got_lines: Vec<String> = g["lines"]
            .as_array()
            .unwrap()
            .iter()
            .map(|l| norm_got.ids(l.as_str().unwrap()))
            .collect();
        assert_eq!(got_lines, want_lines, "{leg_name}/{name}: lines");
        // LR-5: the watermark invariant (as a multiset — concurrent admissions
        // finish in either order).
        let mut wm_got: Vec<String> = g["watermarkCalls"]
            .as_array()
            .unwrap()
            .iter()
            .map(|c| c.to_string())
            .collect();
        let mut wm_want: Vec<String> = w["watermarkCalls"]
            .as_array()
            .unwrap()
            .iter()
            .map(|c| c.to_string())
            .collect();
        wm_got.sort();
        wm_want.sort();
        assert_eq!(wm_got, wm_want, "{leg_name}/{name}: watermark calls");
    }

    // The dump.
    let rows_got: Vec<&Value> = rows_got.iter().collect();
    let rows_want: Vec<&Value> = rows_want.iter().collect();
    assert_eq!(
        rows_got.len(),
        rows_want.len(),
        "{leg_name}: memories row count"
    );
    for (g, w) in rows_got.iter().zip(&rows_want) {
        assert!(
            same(g, w),
            "{leg_name}: memories row diverged\n  rust:   {g}\n  oracle: {w}"
        );
    }
}

#[tokio::test(flavor = "current_thread")]
async fn admission_tier3_matches_oracle() {
    let env = |k: &str| std::env::var(k).ok();
    let (Some(utc), Some(chicago), Some(main), Some(mount)) = (
        env("QT_ORACLE_ADMISSION_UTC"),
        env("QT_ORACLE_ADMISSION_CHICAGO"),
        env("QT_FIXTURE_ADMISSION_MAIN"),
        env("QT_FIXTURE_ADMISSION_MOUNT"),
    ) else {
        eprintln!("SKIP: set QT_ORACLE_ADMISSION_UTC / _CHICAGO and QT_FIXTURE_ADMISSION_MAIN / _MOUNT (see header).");
        return;
    };
    let spec: Value =
        serde_json::from_str(&std::fs::read_to_string(spec_path()).expect("read spec")).unwrap();

    use tracing_subscriber::layer::SubscriberExt;
    let logs = Arc::new(Mutex::new(Vec::new()));
    let subscriber = tracing_subscriber::registry().with(Capture {
        lines: logs.clone(),
        owned: OWNED_MODULES,
        prefixes: &["[Admission]"],
    });
    let _guard = tracing::subscriber::set_default(subscriber);

    let mut tier1_done = false;
    for (leg, path) in spec["legs"]
        .as_array()
        .unwrap()
        .iter()
        .zip([&utc, &chicago])
    {
        let oracle = oracle_lines(path);
        assert!(
            oracle.iter().any(|l| l["kind"] == "dump"),
            "{path}: no dump — a stale or partial oracle?"
        );
        let dir = tempfile::Builder::new()
            .prefix("qt-admission-rust-")
            .tempdir()
            .unwrap();
        let got = run_leg(
            &spec,
            leg,
            &oracle,
            copy_pair(&main, &mount, dir.path()),
            &logs,
        )
        .await;
        compare_leg(&spec, leg, &oracle, &got);

        // Tier-1 rows (the utc leg carries them).
        for line in oracle.iter().filter(|l| l["kind"] == "contentKey") {
            assert_eq!(
                admission_content_key(&row_of(&line["row"])),
                line["key"].as_str().unwrap(),
                "content key of {}",
                line["row"]
            );
            tier1_done = true;
        }
        if let Some(keys) = oracle.iter().find(|l| l["kind"] == "scopeKeys") {
            let scope = AdmissionKeyScope::new(spec["scopeKeys"]["jobId"].as_str().unwrap());
            let minted: Vec<String> = spec["scopeKeys"]["contentKeys"]
                .as_array()
                .unwrap()
                .iter()
                .map(|k| scope.next_key(k.as_str().unwrap()))
                .collect();
            assert_eq!(json!(minted), keys["keys"], "scope keys");
        }
    }
    assert!(tier1_done, "no tier-1 content-key rows ran");
    eprintln!("OK: admission tier-3 matched the oracle (utc + chicago legs).");
}
