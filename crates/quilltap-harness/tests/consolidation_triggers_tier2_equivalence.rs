//! P4.D271 tier-2 differential: the consolidation triggers, the enqueue, the
//! scheduled housekeeping pass and the job handler (v4
//! `lib/memory/consolidation-triggers.ts`, `queue-service.ts`
//! `enqueueMemoryConsolidation`, `scheduled-housekeeping.ts`
//! `runScheduledHousekeeping`, `handlers/memory-consolidation.ts` — all
//! byte-identical between `d58548051` and the `01a83539d` pin) against
//! `quilltap_core::services::{consolidation_triggers, queue_service,
//! memory_consolidation_job}`, over a COPY of the consolidation fixture pair.
//!
//! The steps (`harness/oracle/fixtures/consolidation-triggers-steps.json`) run
//! IN ORDER on ONE copy on each side after the same plants. Comparands per step:
//! the returned value (minted job ids masked) and every captured line of the
//! three child loggers (`memory:consolidation-triggers`,
//! `jobs:memory-consolidation`, `scheduled-housekeeping`) plus the enqueue's two
//! `[Consolidation]` lines — v4's array/object fields rendered under the
//! `…Json` key convention (`digests` → `digestsJson`). At the end: every
//! `background_jobs` row (type, status, `maxAttempts`, payload bytes, and the
//! `scheduledAt − createdAt` delay classified `now` / `+30m`).
//!
//! **`WATERMARK_ROWID_WINDOW` (P4.D271 R-B, a ruled both-ways divergence).** v4
//! reads the characters a job wrote from the child's write list
//! (`characterIdsFromMemoryWrites`); v5 reads the `memories` rows INSERTED past
//! the claim-time `rowid` mark. Per scenario (on a fresh copy): the same
//! inserts planted, v4's list vs v5's window. `one_job`, `updates_only` and
//! `backdated_extraction` must AGREE; `concurrent_over_include` must DIFFER in
//! exactly the recorded direction (v5 = v4 + the concurrent writer's
//! character). A v4 convergence (or a v5 regression) trips it. The falsifier
//! for R-B's rowid choice is measured here too: on `backdated_extraction` (the
//! row's `createdAt` backdated to its source message, as every extraction path
//! does) a `createdAt >= startedAt` window is EMPTY while the rowid window
//! finds the character. And R-B's full path (a planted claim-time mark → the
//! region call → a `watermark` job) is exercised end to end, v5-only.
//!
//! **As of `spec.now` (the 2026-10-13 time bomb).** The fixture dates every
//! memory relative to `consolidation-tier3.json`'s FIXED `now`; the handler
//! steps run the consolidation "as of" that instant on both sides (v4's real
//! `runConsolidation` wrapped to fill an omitted `now`; [`CannedRunner`] fills
//! an omitted `now_ms`). Before that seam the runs read the wall clock, so the
//! corpus's age categories drifted (D1–D2 crossed the maturity floor and the
//! family went red with `no canned answer for topic D`). Everything else stays
//! wall-clock and self-consistent: the planted jobs, debounce offsets and
//! throttle / housekeeping windows all read the run's own `now`.
//!
//! Generate the fixture + oracle output (Node 24, from the v4 checkout or the
//! round's pin — a /tmp mirror):
//!   N=~/.nvm/versions/node/v24.13.1/bin ; V5W=${V5W:-$HOME/source/quilltap-v5} ; M=/tmp/qt-consolidation-oracle
//!   rm -rf $M && mkdir -p $M && cp -R $V5W/harness/oracle/* $M/
//!   cd ~/source/quilltap-server
//!   QT_FIXTURE_OUT=/tmp/qt-consolidation-main.db \
//!   QT_FIXTURE_MOUNT_OUT=/tmp/qt-consolidation-mount.db \
//!     $N/npx tsx $V5W/harness/oracle/fixtures/build-consolidation-fixture.ts
//!   rm -f /tmp/qt-consolidation-triggers.ndjson
//!   QT_FIXTURE_CONSOLIDATION_MAIN=/tmp/qt-consolidation-main.db \
//!   QT_FIXTURE_CONSOLIDATION_MOUNT=/tmp/qt-consolidation-mount.db \
//!   QT_ORACLE_OUT=/tmp/qt-consolidation-triggers.ndjson \
//!     $N/npx jest --silent --watchman=false --testTimeout=180000 \
//!       --roots "$PWD" --roots "$M/cases" -- consolidation-triggers-tier2
//!   cd $V5W
//!   QT_ORACLE_CONSOLIDATION_TRIGGERS=/tmp/qt-consolidation-triggers.ndjson \
//!   QT_FIXTURE_CONSOLIDATION_MAIN=/tmp/qt-consolidation-main.db \
//!   QT_FIXTURE_CONSOLIDATION_MOUNT=/tmp/qt-consolidation-mount.db \
//!     cargo test -p quilltap-harness --test consolidation_triggers_tier2_equivalence -- --nocapture

use std::path::{Path, PathBuf};

use quilltap_core::db::background_jobs::BackgroundJob;
use quilltap_core::db::runtime::{Db, DbPaths};
use quilltap_core::model::completion::{
    CannedCompletionProvider, CompletionMessage, CompletionRole, CompletionUsage,
};
use quilltap_core::model::embedding::CannedEmbeddingProvider;
use quilltap_core::services::cheap_llm_exec::CheapLlmTaskExecutor;
use quilltap_core::services::consolidation::{
    run_consolidation, ConsolidationRunFuture, ConsolidationRunner, RunConsolidationOptions,
};
use quilltap_core::services::consolidation_triggers::{
    characters_inserted_since, check_consolidation_watermark_after_job_at, max_memory_rowid,
    maybe_enqueue_consolidation_for_characters, record_mark_for_tests,
    reset_consolidation_trigger_state_for_tests,
};
use quilltap_core::services::memory_consolidation_job::handle_memory_consolidation;
use quilltap_core::services::queue_service::{
    enqueue_memory_consolidation, run_scheduled_housekeeping, MemoryConsolidationPayload,
};
use quilltap_core::test_support::global_capture;
use serde_json::{json, Value};

const MODULES: &[&str] = &[
    "memory:consolidation-triggers",
    "jobs:memory-consolidation",
    "scheduled-housekeeping",
];
const MESSAGES: &[&str] = &[
    "[Consolidation] Job already in flight for character; not enqueuing another",
    "[Consolidation] Failed to check for existing jobs during enqueue",
];

fn fixtures_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../harness/oracle/fixtures")
}

struct CannedRunner {
    completion: CannedCompletionProvider,
    embedding: CannedEmbeddingProvider,
    /// The fixture's `spec.now` — see [`CannedRunner::run`].
    now_ms: i64,
}

impl ConsolidationRunner for CannedRunner {
    /// THE TIME BOMB (red 2026-10-13): the fixture dates every memory relative
    /// to the FIXED `spec.now` (2026-10-08), but the job handler runs with no
    /// `now_ms`, so the run clustered on the WALL CLOCK — the corpus's "fresh
    /// pair below the floor" (D1–D2, 9 days old) aged past the maturity floor
    /// and formed a topic-D cluster no answer set covers (the immature, 2×-floor
    /// and already-considered rows cross their thresholds on later dates too).
    /// An omitted `now_ms` is filled from `spec.now` here, as the v4 case's
    /// `runConsolidation` wrapper fills an omitted `now` — an explicit one is
    /// kept, so a handler that passed its own clock still diverges.
    fn run<'a>(
        &'a self,
        db: &'a Db,
        character_id: &'a str,
        mut opts: RunConsolidationOptions,
    ) -> ConsolidationRunFuture<'a> {
        opts.now_ms.get_or_insert(self.now_ms);
        Box::pin(async move {
            let executor = CheapLlmTaskExecutor::new();
            run_consolidation(
                db,
                &self.completion,
                &self.embedding,
                &executor,
                character_id,
                opts,
            )
            .await
        })
    }
}

/// Every 36-char UUID not in `known` → `<id>`.
fn mask_minted(s: &str, known: &str) -> String {
    let mut out = String::new();
    let mut i = 0;
    while i < s.len() {
        if i + 36 <= s.len() && s.is_char_boundary(i) && s.is_char_boundary(i + 36) {
            let cand = &s[i..i + 36];
            let is_uuid = cand.bytes().enumerate().all(|(j, c)| match j {
                8 | 13 | 18 | 23 => c == b'-',
                _ => c.is_ascii_hexdigit(),
            });
            if is_uuid {
                out.push_str(
                    if known.contains(cand)
                        || (cand.starts_with("0de") || cand.starts_with("000000"))
                    {
                        cand
                    } else {
                        "<id>"
                    },
                );
                i += 36;
                continue;
            }
        }
        let ch = s[i..].chars().next().unwrap();
        out.push(ch);
        i += ch.len_utf8();
    }
    out
}

fn mask_duration(s: &str) -> String {
    match s.find("durationMs=") {
        Some(i) => {
            let tail = &s[i + "durationMs=".len()..];
            let end = tail.find(' ').unwrap_or(tail.len());
            format!("{}durationMs=<ms>{}", &s[..i], &tail[end..])
        }
        None => s.to_string(),
    }
}

/// An oracle line as v5 renders it (target dropped): array/object fields under
/// the `…Json` key convention.
fn render_oracle_line(line: &Value) -> String {
    let mut out = format!(
        "{} {}",
        line["level"].as_str().unwrap().to_uppercase(),
        line["message"].as_str().unwrap()
    );
    if let Some(ctx) = line["context"].as_object() {
        for (k, v) in ctx {
            match v {
                Value::String(s) => out.push_str(&format!(" {k}={s}")),
                Value::Array(_) | Value::Object(_) => {
                    out.push_str(&format!(" {k}Json={}", serde_json::to_string(v).unwrap()))
                }
                other => out.push_str(&format!(" {k}={other}")),
            }
        }
    }
    mask_duration(&out)
}

fn strip_target(line: &str) -> String {
    let mut parts = line.splitn(3, ' ');
    let level = parts.next().unwrap_or_default();
    let _ = parts.next();
    mask_duration(&format!("{level} {}", parts.next().unwrap_or_default()))
}

fn wanted(line: &str) -> bool {
    let rest = line.splitn(3, ' ').nth(2).unwrap_or_default();
    MODULES
        .iter()
        .any(|m| rest.contains(&format!(" module={m}")))
        || MESSAGES.iter().any(|m| {
            rest.strip_prefix(m)
                .is_some_and(|t| t.is_empty() || t.starts_with(' '))
        })
}

fn open_copy(main_src: &str, mount_src: &str, pepper: &str, dir: &Path) -> Db {
    let main = dir.join("main.db");
    let mount = dir.join("mount.db");
    std::fs::copy(main_src, &main).unwrap();
    std::fs::copy(mount_src, &mount).unwrap();
    Db::open(
        DbPaths {
            main,
            mount_index: Some(mount),
            llm_logs: None,
        },
        pepper,
    )
    .expect("open the fixture copies")
}

async fn exec(db: &Db, sql: String, params: Vec<String>) {
    db.write(move |w| {
        w.main()
            .connection()
            .execute(&sql, rusqlite::params_from_iter(params.iter()))?;
        Ok(())
    })
    .await
    .expect("plant");
}

fn delay_class(ms: i64) -> &'static str {
    if ms.abs() < 2_000 {
        "now"
    } else if (ms - 1_800_000).abs() < 10_000 {
        "+30m"
    } else {
        "other"
    }
}

fn jobs_rows(db: &Db) -> Vec<Value> {
    db.read_main(|c| {
        let mut stmt = c.prepare(
            "SELECT id, userId, type, status, payload, priority, attempts, maxAttempts, lastError, scheduledAt, createdAt \
             FROM background_jobs ORDER BY type, payload, status",
        )?;
        let rows = stmt.query_map([], |r| {
            let scheduled: String = r.get(9)?;
            let created: String = r.get(10)?;
            let d = quilltap_core::clock::iso_to_ms(&scheduled).unwrap_or(0)
                - quilltap_core::clock::iso_to_ms(&created).unwrap_or(0);
            Ok(json!({
                "id": r.get::<_, String>(0)?,
                "userId": r.get::<_, String>(1)?,
                "type": r.get::<_, String>(2)?,
                "status": r.get::<_, String>(3)?,
                "payload": r.get::<_, String>(4)?,
                "priority": r.get::<_, f64>(5)?,
                "attempts": r.get::<_, f64>(6)?,
                "maxAttempts": r.get::<_, f64>(7)?,
                "lastError": r.get::<_, Option<String>>(8)?,
                "delay": delay_class(d),
            }))
        })?;
        Ok(rows.collect::<Result<Vec<_>, _>>()?)
    })
    .unwrap()
}

fn plant_memory_sql(i: u64, character_id: &str, created_at: &str) -> String {
    format!(
        "INSERT INTO memories (id, characterId, content, summary, keywords, tags, importance, source, entities, kind, \
         reinforcementCount, relatedMemoryIds, reinforcedImportance, tier, consolidatedFrom, createdAt, updatedAt) VALUES \
         ('0dd00000-0000-4000-8000-{i:012x}', '{character_id}', 'window {i}', 'window {i}', '[]', '[]', 0.5, 'AUTO', '[]', \
         'semantic', 1, '[]', 0.5, 'hot', '[]', '{created_at}', '{created_at}')"
    )
}

#[tokio::test(flavor = "current_thread")]
async fn consolidation_triggers_match_oracle() {
    let (Ok(oracle_path), Ok(fixture_main), Ok(fixture_mount)) = (
        std::env::var("QT_ORACLE_CONSOLIDATION_TRIGGERS"),
        std::env::var("QT_FIXTURE_CONSOLIDATION_MAIN"),
        std::env::var("QT_FIXTURE_CONSOLIDATION_MOUNT"),
    ) else {
        eprintln!("SKIP: set QT_ORACLE_CONSOLIDATION_TRIGGERS + QT_FIXTURE_CONSOLIDATION_MAIN / _MOUNT (see header).");
        return;
    };
    global_capture::install();
    let spec_text =
        std::fs::read_to_string(fixtures_dir().join("consolidation-tier3.json")).unwrap();
    let spec: Value = serde_json::from_str(&spec_text).unwrap();
    let corpus: Value = serde_json::from_str(
        &std::fs::read_to_string(fixtures_dir().join("consolidation-triggers-steps.json")).unwrap(),
    )
    .unwrap();
    let oracle: Vec<Value> = std::fs::read_to_string(&oracle_path)
        .unwrap_or_else(|e| panic!("read oracle {oracle_path}: {e}"))
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| serde_json::from_str(l).unwrap())
        .collect();
    let pepper = spec["testPepperBase64"].as_str().unwrap();
    let user_id = corpus["userId"].as_str().unwrap().to_string();

    let mut completion = CannedCompletionProvider::new();
    for row in oracle.iter().filter(|o| o["kind"] == "canned") {
        let messages: Vec<CompletionMessage> = row["messages"]
            .as_array()
            .unwrap()
            .iter()
            .map(|m| CompletionMessage {
                role: CompletionRole::from_v4_wire(m["role"].as_str().unwrap()).unwrap(),
                content: m["content"].as_str().unwrap().to_string(),
            })
            .collect();
        let (p, m, t) = (
            row["provider"].as_str().unwrap(),
            row["model"].as_str().unwrap(),
            row["temperature"].as_f64(),
        );
        completion = match row.get("fail").and_then(Value::as_str) {
            Some(fail) => completion.with_failure(p, m, t, &messages, fail),
            None => completion.with_response(
                p,
                m,
                t,
                &messages,
                row["response"].as_str().unwrap().to_string(),
                Some(CompletionUsage {
                    prompt_tokens: 100,
                    completion_tokens: 40,
                    total_tokens: 140,
                }),
            ),
        };
    }
    let mut embedding = CannedEmbeddingProvider::new();
    for row in oracle.iter().filter(|o| o["kind"] == "embedding") {
        let text = row["text"].as_str().unwrap().to_string();
        embedding = match row["vector"].as_array() {
            Some(v) => {
                embedding.with_vector(text, v.iter().map(|x| x.as_f64().unwrap() as f32).collect())
            }
            None => embedding.with_failure(text),
        };
    }
    let runner = CannedRunner {
        completion,
        embedding,
        now_ms: quilltap_core::clock::iso_to_ms(spec["now"].as_str().unwrap()).unwrap(),
    };
    let _canned_key = quilltap_core::test_support::CannedCheapLlmKey::install("test-key");

    let scratch = tempfile::Builder::new()
        .prefix("qt-consolidation-triggers-rust-")
        .tempdir()
        .unwrap();
    let db = open_copy(&fixture_main, &fixture_mount, pepper, scratch.path());
    reset_consolidation_trigger_state_for_tests();
    let base = quilltap_core::clock::now_unix_ms();

    for sql in corpus["plants"].as_array().unwrap() {
        exec(&db, sql.as_str().unwrap().to_string(), vec![]).await;
    }
    for j in corpus["jobs"].as_array().unwrap() {
        let at = quilltap_core::clock::iso_from_unix_ms(
            base - j["hoursAgo"].as_i64().unwrap() * 3_600_000,
        );
        exec(
            &db,
            "INSERT INTO background_jobs (id, userId, type, status, payload, priority, attempts, maxAttempts, lastError, scheduledAt, startedAt, completedAt, createdAt, updatedAt) \
             VALUES (?1, ?2, 'MEMORY_CONSOLIDATION', ?3, ?4, 0, 0, 1, NULL, ?5, NULL, NULL, ?5, ?5)"
                .to_string(),
            vec![
                j["id"].as_str().unwrap().to_string(),
                user_id.clone(),
                j["status"].as_str().unwrap().to_string(),
                serde_json::to_string(&j["payload"]).unwrap(),
                at,
            ],
        )
        .await;
    }
    exec(
        &db,
        corpus["chatSettingsSql"].as_str().unwrap().to_string(),
        vec![],
    )
    .await;

    let oracle_steps: Vec<&Value> = oracle.iter().filter(|o| o["kind"] == "step").collect();
    let mut step_iter = oracle_steps.into_iter();
    let mut failures: Vec<String> = Vec::new();
    let mut lines_compared = 0usize;
    for step in corpus["steps"].as_array().unwrap() {
        let op = step["op"].as_str().unwrap();
        match op {
            "setSettings" => {
                let patch = step["patch"].clone();
                db.write(move |w| {
                    quilltap_core::db::instance_settings::set_memory_consolidation_settings(
                        w.main().connection(),
                        &patch,
                    )
                })
                .await
                .expect("set settings");
                continue;
            }
            "sql" => {
                exec(&db, step["sql"].as_str().unwrap().to_string(), vec![]).await;
                continue;
            }
            _ => {}
        }
        let want = step_iter.next().expect("an oracle step row");
        let name = step["name"].as_str().unwrap();
        assert_eq!(want["name"], name, "step order");
        let (value, captured) = global_capture::capture_async(async {
            match op {
                "maybeEnqueue" => {
                    let ids: Vec<String> = step["ids"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .map(|v| v.as_str().unwrap().to_string())
                        .collect();
                    json!(
                        maybe_enqueue_consolidation_for_characters(
                            &db,
                            &ids,
                            base + step["offsetMs"].as_i64().unwrap()
                        )
                        .await
                    )
                }
                "enqueue" => {
                    let payload = MemoryConsolidationPayload::decode(&step["payload"]).unwrap();
                    json!(enqueue_memory_consolidation(
                        &db,
                        step["userId"].as_str().unwrap(),
                        &payload
                    )
                    .await
                    .unwrap())
                }
                "housekeeping" => {
                    let (u, j) = run_scheduled_housekeeping(&db).await.unwrap();
                    json!({ "usersProcessed": u, "jobsEnqueued": j })
                }
                "handler" => {
                    if let Some(pre) = step.get("pre") {
                        let patch = pre.clone();
                        db.write(move |w| {
                            quilltap_core::db::instance_settings::set_memory_consolidation_settings(
                                w.main().connection(),
                                &patch,
                            )
                        })
                        .await
                        .expect("pre settings");
                    }
                    let now = quilltap_core::clock::now_iso();
                    let job = BackgroundJob {
                        id: step["job"]["id"].as_str().unwrap().to_string(),
                        user_id: user_id.clone(),
                        job_type: "MEMORY_CONSOLIDATION".to_string(),
                        status: "PROCESSING".to_string(),
                        payload: serde_json::to_string(&step["job"]["payload"]).unwrap(),
                        priority: 0.0,
                        attempts: 1.0,
                        max_attempts: 1.0,
                        last_error: None,
                        scheduled_at: now.clone(),
                        started_at: Some(now.clone()),
                        completed_at: None,
                        created_at: now.clone(),
                        updated_at: now,
                    };
                    match handle_memory_consolidation(&db, &runner, &job).await {
                        quilltap_core::services::job_runner::JobOutcome::Completed(_) => {
                            json!("completed")
                        }
                        quilltap_core::services::job_runner::JobOutcome::Failed(m) => {
                            json!({ "failed": m })
                        }
                    }
                }
                other => panic!("unknown op {other}"),
            }
        })
        .await;
        let got_value = mask_minted(&value.to_string(), &spec_text);
        let want_value = mask_minted(&want["value"].to_string(), &spec_text);
        if got_value != want_value {
            failures.push(format!("{name}: value v5 {got_value} v4 {want_value}"));
        }
        let got_lines: Vec<String> = captured
            .iter()
            .filter(|l| wanted(l))
            .map(|l| mask_minted(&strip_target(l), &spec_text))
            .collect();
        let want_lines: Vec<String> = want["lines"]
            .as_array()
            .unwrap()
            .iter()
            .map(|l| mask_minted(&render_oracle_line(l), &spec_text))
            .collect();
        lines_compared += want_lines.len();
        if got_lines != want_lines {
            failures.push(format!(
                "{name}: lines\n  v5 {got_lines:#?}\n  v4 {want_lines:#?}"
            ));
        }
    }

    let want_jobs = &oracle
        .iter()
        .find(|o| o["kind"] == "jobs")
        .expect("jobs row")["rows"];
    let want_jobs: Vec<Value> = want_jobs
        .as_array()
        .unwrap()
        .iter()
        .map(|r| {
            let mut r = r.clone();
            let d = r["scheduledDelayMs"].as_i64().unwrap();
            r.as_object_mut().unwrap().remove("scheduledDelayMs");
            r["delay"] = json!(delay_class(d));
            r
        })
        .collect();
    let got_jobs = jobs_rows(&db);
    let norm = |rows: &[Value]| -> Vec<String> {
        rows.iter()
            .map(|r| {
                let mut r = r.clone();
                for k in ["priority", "attempts", "maxAttempts"] {
                    r[k] = json!(r[k].as_f64().unwrap());
                }
                mask_minted(&r.to_string(), &spec_text)
            })
            .collect()
    };
    if norm(&got_jobs) != norm(&want_jobs) {
        failures.push(format!(
            "background_jobs:\n  v5 {:#?}\n  v4 {:#?}",
            norm(&got_jobs),
            norm(&want_jobs)
        ));
    }
    drop(db);

    // ── WATERMARK_ROWID_WINDOW (R-B) ────────────────────────────────────────
    let chars = &corpus["windowCharacters"];
    let mut window_rows = 0;
    for (n, w) in corpus["window"].as_array().unwrap().iter().enumerate() {
        let name = w["name"].as_str().unwrap();
        let v4_ids: Vec<String> = oracle
            .iter()
            .find(|o| o["kind"] == "window" && o["name"] == name)
            .unwrap_or_else(|| panic!("oracle window {name}"))["ids"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_str().unwrap().to_string())
            .collect();
        let dir = scratch.path().join(format!("window-{n}"));
        std::fs::create_dir_all(&dir).unwrap();
        let wdb = open_copy(&fixture_main, &fixture_mount, pepper, &dir);
        let mark = max_memory_rowid(&wdb).unwrap();
        let started_iso = quilltap_core::clock::now_iso();
        let backdated = w.get("backdated").and_then(Value::as_bool).unwrap_or(false);
        let created = if backdated {
            "2026-01-01T00:00:00.000Z".to_string()
        } else {
            quilltap_core::clock::now_iso()
        };
        let mut i = (n as u64 + 1) * 100;
        let mut concurrent: Vec<String> = Vec::new();
        for (list, is_job) in [(&w["job"], true), (&w["concurrent"], false)] {
            for k in list.as_array().unwrap() {
                let cid = chars[k.as_str().unwrap()].as_str().unwrap().to_string();
                exec(&wdb, plant_memory_sql(i, &cid, &created), vec![]).await;
                i += 1;
                if !is_job && !concurrent.contains(&cid) {
                    concurrent.push(cid);
                }
            }
        }
        if w["updatesOnly"].as_bool().unwrap_or(false) {
            exec(
                &wdb,
                "UPDATE memories SET content = content WHERE rowid = 1".to_string(),
                vec![],
            )
            .await;
        }
        let v5_ids = characters_inserted_since(&wdb, mark).unwrap();
        window_rows += 1;
        if concurrent.is_empty() {
            if v5_ids != v4_ids {
                failures.push(format!(
                    "WATERMARK_ROWID_WINDOW {name}: must agree — v5 {v5_ids:?} v4 {v4_ids:?}"
                ));
            }
        } else {
            let mut expected = v4_ids.clone();
            for c in &concurrent {
                if !expected.contains(c) {
                    expected.push(c.clone());
                }
            }
            if v5_ids == v4_ids || v5_ids != expected {
                failures.push(format!(
                    "WATERMARK_ROWID_WINDOW {name}: the recorded over-include moved — v5 {v5_ids:?} v4 {v4_ids:?} expected v5 {expected:?}"
                ));
            }
        }
        if backdated {
            // The falsifier: a `createdAt >= startedAt` window is EMPTY for a
            // backdated extraction row; the rowid window is not.
            let s = started_iso.clone();
            let by_created: Vec<String> = wdb
                .read_main(move |c| {
                    let mut stmt = c.prepare(
                        "SELECT DISTINCT characterId FROM memories WHERE createdAt >= ?1",
                    )?;
                    let r = stmt.query_map([s], |r| r.get::<_, String>(0))?;
                    Ok(r.collect::<Result<Vec<_>, _>>()?)
                })
                .unwrap();
            if !by_created.is_empty() || v5_ids.is_empty() {
                failures.push(format!(
                    "R-B falsifier: createdAt window {by_created:?} (must be empty), rowid window {v5_ids:?} (must not)"
                ));
            }
        }
    }

    // R-B end to end (v5-only): a claim-time mark → the region call → one
    // `watermark` job for the character the job inserted for.
    {
        let dir = scratch.path().join("window-e2e");
        std::fs::create_dir_all(&dir).unwrap();
        let edb = open_copy(&fixture_main, &fixture_mount, pepper, &dir);
        reset_consolidation_trigger_state_for_tests();
        edb.write(|w| {
            quilltap_core::db::instance_settings::set_memory_consolidation_settings(
                w.main().connection(),
                &json!({ "enabled": true, "watermark": 1 }),
            )
        })
        .await
        .unwrap();
        let claim = quilltap_core::clock::now_unix_ms();
        record_mark_for_tests(claim - 1, max_memory_rowid(&edb).unwrap());
        let cid = chars["A"].as_str().unwrap().to_string();
        exec(
            &edb,
            plant_memory_sql(9_000, &cid, "2026-01-01T00:00:00.000Z"),
            vec![],
        )
        .await;
        let job = BackgroundJob {
            id: "0cb00000-0000-4000-8000-00000000e2e0".to_string(),
            user_id: user_id.clone(),
            job_type: "MEMORY_EXTRACTION".to_string(),
            status: "COMPLETED".to_string(),
            payload: "{}".to_string(),
            priority: 0.0,
            attempts: 1.0,
            max_attempts: 3.0,
            last_error: None,
            scheduled_at: quilltap_core::clock::iso_from_unix_ms(claim),
            started_at: Some(quilltap_core::clock::iso_from_unix_ms(claim)),
            completed_at: None,
            created_at: quilltap_core::clock::iso_from_unix_ms(claim),
            updated_at: quilltap_core::clock::iso_from_unix_ms(claim),
        };
        check_consolidation_watermark_after_job_at(&edb, &job, claim + 5_000).await;
        let jobs = jobs_rows(&edb);
        let watermark_jobs: Vec<&Value> = jobs
            .iter()
            .filter(|r| {
                r["payload"]
                    == json!(format!(
                        "{{\"characterId\":\"{cid}\",\"trigger\":\"watermark\"}}"
                    ))
            })
            .collect();
        if watermark_jobs.len() != 1 {
            failures.push(format!(
                "R-B end to end: expected one watermark job for {cid}, got {jobs:#?}"
            ));
        }
        // A MEMORY_CONSOLIDATION completion never re-triggers.
        let mut own = job.clone();
        own.job_type = "MEMORY_CONSOLIDATION".to_string();
        check_consolidation_watermark_after_job_at(&edb, &own, claim + 700_000).await;
        if jobs_rows(&edb).len() != jobs.len() {
            failures.push("R-B: a MEMORY_CONSOLIDATION completion enqueued".to_string());
        }
    }

    assert!(
        failures.is_empty(),
        "{} failure(s):\n{}",
        failures.len(),
        failures.join("\n\n")
    );
    assert!(
        lines_compared >= 30 && window_rows == 4,
        "lines {lines_compared} window rows {window_rows}"
    );
    eprintln!(
        "OK: consolidation triggers matched oracle ({lines_compared} lines, background_jobs, WATERMARK_ROWID_WINDOW {window_rows} rows, the R-B falsifier and end-to-end)."
    );
}
