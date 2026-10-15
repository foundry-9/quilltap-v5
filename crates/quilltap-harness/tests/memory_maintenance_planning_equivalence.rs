//! P4.D297 tier-1 differential: the memory maintenance job's pure planners and
//! the ledger's decisions (`quilltap_core::services::memory_maintenance`)
//! against v4's REAL exports at the `83334b9a5` pin (`lib/memory/maintenance/`,
//! `settings-defaults.ts`) — `resolvePhases` + `exemptPhasesFor` over the whole
//! reason × phases × exemptions × switches × toggles × `coldRetentionDays`
//! grid, `withImpliedPhases`, `summarizeMaintenance` (BYTE compare — the key
//! order is the comparand), `planDemotion`, `planRetention`,
//! `planNearDuplicateFolds` (+ `isFoldEligible`), `bucketCandidates`,
//! `resolveClusterThreshold`, and scripted ledger scenarios (`shouldEnqueue`,
//! `recordMaintenanceOutcome`, `takeWatermarkCheck`, `noteEnqueued`,
//! `getMaintenanceLedgerStatus`, the seed and its failure) with the ledger's
//! `module: 'memory:maintenance:ledger'` lines per step.
//!
//! Numbers compare at 1e-12 (protection scores, cosines); every JSON object
//! compares in KEY ORDER. The ledger's seed rows ride a real `background_jobs`
//! table on the v5 side (v4's `findRecentByType` is mocked to answer the same
//! rows in the real query's `updatedAt DESC` order); a failed seed read is a
//! missing table on v5's side and the fallback `safeQuery`'s `[]` on v4's —
//! the ledger is then seeded with nothing (the seed's WARN is unreachable).
//!
//! Generate (Node 24, from the v4 checkout or the round's pin — a /tmp mirror):
//!   N=~/.nvm/versions/node/v24.13.1/bin ; V5W=${V5W:-$HOME/source/quilltap-v5} ; M=/tmp/qt-maintenance-planning-oracle
//!   rm -rf $M && mkdir -p $M && cp -R $V5W/harness/oracle/* $M/
//!   cd ~/source/quilltap-server
//!   rm -f /tmp/qt-maintenance-planning.ndjson
//!   QT_ORACLE_OUT=/tmp/qt-maintenance-planning.ndjson \
//!     $N/npx jest --silent --watchman=false --testTimeout=180000 \
//!       --roots "$PWD" --roots "$M/cases" -- memory-maintenance-planning
//!   cd $V5W
//!   QT_ORACLE_MAINTENANCE_PLANNING=/tmp/qt-maintenance-planning.ndjson \
//!     cargo test -p quilltap-harness --test memory_maintenance_planning_equivalence -- --nocapture

use std::path::PathBuf;

use quilltap_core::admission::thresholds::admission_thresholds_for_provider;
use quilltap_core::db::runtime::{Db, DbPaths};
use quilltap_core::db::Writer;
use quilltap_core::services::consolidation::{
    resolve_cluster_threshold, ConsolidationReport, ConsolidationStats,
};
use quilltap_core::services::memory_maintenance::consolidate::bucket_candidates;
use quilltap_core::services::memory_maintenance::demote::{plan_demotion, plan_retention};
use quilltap_core::services::memory_maintenance::fold_near_duplicates::{
    is_fold_eligible, plan_near_duplicate_folds,
};
use quilltap_core::services::memory_maintenance::ledger::{
    get_maintenance_ledger_status, note_enqueued, record_maintenance_outcome,
    reset_maintenance_ledger_for_tests, should_enqueue, take_watermark_check, WatermarkReading,
};
use quilltap_core::services::memory_maintenance::run::{
    exempt_phases_for, resolve_phases, summarize_maintenance, PhaseSettings,
};
use quilltap_core::services::memory_maintenance::types::{
    with_implied_phases, DemotePhaseReport, FoldPhaseReport, MaintenancePhase, MaintenanceReason,
    MaintenanceReport, MaintenanceSkip, MaintenanceSummary, MirrorPhaseReport, NearDuplicateGroup,
    PhaseOutcome, PhaseSkipReason, PhaseStatus, RetirePhaseReport,
};
use quilltap_core::test_support::global_capture;
use serde_json::{json, Value};

const PEPPER: &str = "dGVzdHBlcHBlcnRlc3RwZXBwZXJ0ZXN0cGVwcGVyMDE=";

/// Structural equality, numbers at 1e-12, objects in KEY ORDER.
fn approx_eq(a: &Value, b: &Value) -> bool {
    match (a, b) {
        (Value::Number(x), Value::Number(y)) => {
            let (x, y) = (x.as_f64().unwrap(), y.as_f64().unwrap());
            x == y || (x - y).abs() <= 1e-12
        }
        (Value::Array(x), Value::Array(y)) => {
            x.len() == y.len() && x.iter().zip(y).all(|(p, q)| approx_eq(p, q))
        }
        (Value::Object(x), Value::Object(y)) => {
            x.len() == y.len()
                && x.iter()
                    .zip(y.iter())
                    .all(|((k1, v1), (k2, v2))| k1 == k2 && approx_eq(v1, v2))
        }
        _ => a == b,
    }
}

fn strings(v: &Value) -> Option<Vec<String>> {
    v.as_array()
        .map(|a| a.iter().map(|x| x.as_str().unwrap().to_string()).collect())
}

fn reason_of(s: &str) -> MaintenanceReason {
    MaintenanceReason::parse(s).unwrap()
}

fn phase_of(s: &str) -> MaintenancePhase {
    MaintenancePhase::parse(s).unwrap()
}

fn skip_of(s: &str) -> PhaseSkipReason {
    match s {
        "not-requested" => PhaseSkipReason::NotRequested,
        "maintenance-disabled" => PhaseSkipReason::MaintenanceDisabled,
        "consolidation-disabled" => PhaseSkipReason::ConsolidationDisabled,
        "phase-disabled" => PhaseSkipReason::PhaseDisabled,
        "no-retention-window" => PhaseSkipReason::NoRetentionWindow,
        "nothing-touched" => PhaseSkipReason::NothingTouched,
        other => panic!("skip reason {other}"),
    }
}

fn outcomes(v: &Value) -> Vec<PhaseOutcome> {
    v.as_array()
        .unwrap()
        .iter()
        .map(|p| PhaseOutcome {
            phase: phase_of(p["phase"].as_str().unwrap()),
            status: if p["status"] == "ran" {
                PhaseStatus::Ran
            } else {
                PhaseStatus::Skipped
            },
            skipped_because: p.get("skippedBecause").and_then(Value::as_str).map(skip_of),
            duration_ms: p["durationMs"].as_i64().unwrap(),
        })
        .collect()
}

fn n(v: &Value, k: &str) -> usize {
    v[k].as_u64().unwrap() as usize
}

/// A `MaintenanceReport` from v4's report JSON (the fields `summarize` reads).
fn report_from(v: &Value) -> MaintenanceReport {
    let consolidation = v.get("consolidation").map(|c| {
        let s = &c["stats"];
        ConsolidationReport {
            character_id: c["characterId"].as_str().unwrap_or_default().to_string(),
            character_name: None,
            dry_run: false,
            trigger: MaintenanceReason::Manual,
            started_at: String::new(),
            finished_at: String::new(),
            settings: Value::Null,
            clusters: Vec::new(),
            stats: ConsolidationStats {
                digests_created: n(s, "digestsCreated"),
                digests_updated: n(s, "digestsUpdated"),
                members_superseded: n(s, "membersSuperseded"),
                clusters_failed: n(s, "clustersFailed"),
                clusters_deferred: n(s, "clustersDeferred"),
                rows_marked_considered: n(s, "rowsMarkedConsidered"),
                ..Default::default()
            },
            skipped_reason: c
                .get("skippedReason")
                .and_then(Value::as_str)
                .map(|r| match r {
                    "no-llm" => "no-llm",
                    other => panic!("skippedReason {other}"),
                }),
        }
    });
    MaintenanceReport {
        character_id: v["characterId"].as_str().unwrap().to_string(),
        character_name: v["characterName"].as_str().map(str::to_string),
        reason: reason_of(v["reason"].as_str().unwrap()),
        dry_run: v["dryRun"].as_bool().unwrap(),
        started_at: v["startedAt"].as_str().unwrap().to_string(),
        finished_at: v["finishedAt"].as_str().unwrap().to_string(),
        duration_ms: v["durationMs"].as_i64().unwrap(),
        skipped_reason: v
            .get("skippedReason")
            .and_then(Value::as_str)
            .map(|s| match s {
                "archived" => MaintenanceSkip::Archived,
                _ => MaintenanceSkip::CharacterNotFound,
            }),
        phases: outcomes(&v["phases"]),
        fold: v.get("fold").map(|f| FoldPhaseReport {
            eligible: n(f, "eligible"),
            deferred: n(f, "deferred"),
            groups: f["groups"]
                .as_array()
                .unwrap()
                .iter()
                .map(|g| NearDuplicateGroup {
                    survivor_id: g["survivorId"].as_str().unwrap().to_string(),
                    absorbed_ids: strings(&g["absorbedIds"]).unwrap(),
                    min_cosine: g["minCosine"].as_f64().unwrap(),
                })
                .collect(),
            folded: n(f, "folded"),
            groups_failed: n(f, "groupsFailed"),
            groups_deferred: n(f, "groupsDeferred"),
        }),
        consolidation,
        demote: v.get("demote").map(|d| DemotePhaseReport {
            cap: d["cap"].as_f64().unwrap(),
            hot_total: n(d, "hotTotal"),
            demotable: n(d, "demotable"),
            protected: n(d, "protected"),
            excess: d["excess"].as_f64().unwrap(),
            demoted: n(d, "demoted"),
            demoted_ids: strings(&d["demotedIds"]).unwrap(),
            highest_score_demoted: d["highestScoreDemoted"].as_f64(),
        }),
        retire: v.get("retire").map(|r| RetirePhaseReport {
            retention_days: r["retentionDays"].as_f64().unwrap(),
            retired: n(r, "retired"),
            retired_ids: strings(&r["retiredIds"]).unwrap(),
        }),
        mirror: v.get("mirror").map(|m| MirrorPhaseReport {
            subjects: strings(&m["subjects"]).unwrap(),
            files_written: n(m, "filesWritten"),
            deferred_to_commit: m["deferredToCommit"].as_bool().unwrap(),
        }),
    }
}

/// A `MaintenanceSummary` from v4's summary JSON (the ledger's input).
fn summary_from(v: &Value) -> MaintenanceSummary {
    MaintenanceSummary {
        character_id: v["characterId"].as_str().unwrap().to_string(),
        reason: reason_of(v["reason"].as_str().unwrap()),
        dry_run: v["dryRun"].as_bool().unwrap(),
        finished_at: v["finishedAt"].as_str().unwrap().to_string(),
        skipped: v.get("skippedReason").is_some_and(|s| !s.is_null()),
        phases: outcomes(&v["phases"]),
        json: v.clone(),
    }
}

/// `LEVEL message k=v …` for a v4 line — arrays / objects as `kJson=…`.
fn render_oracle_line(line: &Value) -> String {
    let mut out = format!(
        "{} {}",
        line["level"].as_str().unwrap().to_uppercase(),
        line["message"].as_str().unwrap()
    );
    for (k, v) in line["context"].as_object().unwrap() {
        match v {
            Value::String(s) => out.push_str(&format!(" {k}={s}")),
            Value::Array(_) | Value::Object(_) => {
                out.push_str(&format!(" {k}Json={}", serde_json::to_string(v).unwrap()))
            }
            other => out.push_str(&format!(" {k}={other}")),
        }
    }
    out
}

/// `LEVEL target message k=v` → `LEVEL message k=v`.
fn strip_target(line: &str) -> String {
    let mut parts = line.splitn(3, ' ');
    let level = parts.next().unwrap_or_default();
    let _ = parts.next();
    format!("{level} {}", parts.next().unwrap_or_default())
}

fn background_jobs_ddl() -> String {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../quilltap-core/src/services/provisioning/fresh_schema.json");
    let dump: Value = serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
    dump["main"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(Value::as_str)
        .find(|s| s.starts_with("CREATE TABLE \"background_jobs\""))
        .expect("background_jobs DDL")
        .to_string()
}

fn insert_jobs(db: &Db, jobs: &[Value]) {
    let rows: Vec<Value> = jobs.to_vec();
    let ddl = background_jobs_ddl();
    db.write_blocking(move |w| {
        let c = w.main().connection();
        c.execute_batch(&ddl)?;
        for j in &rows {
            c.execute(
                "INSERT INTO background_jobs (id, userId, type, status, payload, priority, attempts, \
                 maxAttempts, lastError, scheduledAt, startedAt, completedAt, createdAt, updatedAt) \
                 VALUES (?1, 'u1', 'MEMORY_MAINTENANCE', ?2, ?3, 0, 0, 1, NULL, ?4, NULL, NULL, ?4, ?4)",
                rusqlite::params![
                    j["id"].as_str().unwrap(),
                    j["status"].as_str().unwrap(),
                    j["payload"].to_string(),
                    j["updatedAt"].as_str().unwrap(),
                ],
            )?;
        }
        Ok(())
    })
    .unwrap();
}

fn fresh_db(dir: &std::path::Path) -> Db {
    let path = dir.join("main.db");
    {
        let w = Writer::open_writable(&path, PEPPER).unwrap();
        w.connection()
            .execute_batch("CREATE TABLE stand_in (id TEXT);")
            .unwrap();
    }
    Db::open(
        DbPaths {
            main: path,
            mount_index: None,
            llm_logs: None,
        },
        PEPPER,
    )
    .unwrap()
}

fn row_refs(rows: &Value) -> Vec<Value> {
    rows.as_array().unwrap().clone()
}

#[test]
fn memory_maintenance_planning_matches_oracle() {
    let Ok(path) = std::env::var("QT_ORACLE_MAINTENANCE_PLANNING") else {
        eprintln!("SKIP: set QT_ORACLE_MAINTENANCE_PLANNING (see header).");
        return;
    };
    global_capture::install();
    let rows: Vec<Value> = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("read oracle {path}: {e}"))
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| serde_json::from_str(l).unwrap())
        .collect();
    let mut failures: Vec<String> = Vec::new();
    let mut counts: std::collections::BTreeMap<String, usize> = Default::default();

    for row in &rows {
        let kind = row["kind"].as_str().unwrap();
        *counts.entry(kind.to_string()).or_default() += 1;
        match kind {
            "resolvePhases" => {
                let req = &row["request"];
                let s = &row["settings"];
                let phases = req.get("phases").and_then(strings);
                let exempt = req.get("exemptPhases").and_then(strings);
                let reason = reason_of(req["reason"].as_str().unwrap());
                let settings = PhaseSettings {
                    maintenance_enabled: s["maintenanceEnabled"].as_bool().unwrap(),
                    fold_near_duplicates: s["toggles"]["foldNearDuplicates"].as_bool().unwrap(),
                    demote: s["toggles"]["demote"].as_bool().unwrap(),
                    retire: s["toggles"]["retire"].as_bool().unwrap(),
                    consolidation_enabled: s["consolidationEnabled"].as_bool().unwrap(),
                    cold_retention_days: s["coldRetentionDays"].as_f64(),
                };
                let got: Vec<Value> =
                    resolve_phases(phases.as_deref(), reason, exempt.as_deref(), &settings)
                        .into_iter()
                        .map(|(p, s)| json!([p.as_str(), s.map(|r| r.as_str())]))
                        .collect();
                if Value::Array(got.clone()) != row["out"] {
                    failures.push(format!(
                        "resolvePhases {req} {s}:\n  v5 {got:?}\n  v4 {}",
                        row["out"]
                    ));
                }
                let got_exempt = exempt_phases_for(phases.as_deref(), reason, exempt.as_deref());
                if json!(got_exempt) != row["exempt"] {
                    failures.push(format!(
                        "exemptPhasesFor {req}: v5 {got_exempt:?} v4 {}",
                        row["exempt"]
                    ));
                }
            }
            "withImpliedPhases" => {
                let phases: Vec<MaintenancePhase> = strings(&row["phases"])
                    .unwrap()
                    .iter()
                    .map(|p| phase_of(p))
                    .collect();
                let got: Vec<&str> = with_implied_phases(&phases)
                    .iter()
                    .map(|p| p.as_str())
                    .collect();
                if json!(got) != row["out"] {
                    failures.push(format!(
                        "withImpliedPhases {}: v5 {got:?} v4 {}",
                        row["phases"], row["out"]
                    ));
                }
            }
            "summarize" => {
                let got = summarize_maintenance(&report_from(&row["report"])).to_json();
                let (g, w) = (
                    serde_json::to_string(&got).unwrap(),
                    serde_json::to_string(&row["out"]).unwrap(),
                );
                if g != w {
                    failures.push(format!("summarize {}:\n  v5 {g}\n  v4 {w}", row["name"]));
                }
            }
            "planDemotion" => {
                let rs = row_refs(&row["rows"]);
                let refs: Vec<&Value> = rs.iter().collect();
                let got = plan_demotion(
                    &refs,
                    row["cap"].as_f64().unwrap(),
                    row["nowMs"].as_f64().unwrap(),
                )
                .to_json();
                if !approx_eq(&got, &row["out"]) {
                    failures.push(format!(
                        "planDemotion cap {}:\n  v5 {got}\n  v4 {}",
                        row["cap"], row["out"]
                    ));
                }
            }
            "planRetention" => {
                let rs = row_refs(&row["rows"]);
                let refs: Vec<&Value> = rs.iter().collect();
                let got = plan_retention(
                    &refs,
                    row["retentionDays"].as_f64().unwrap(),
                    row["nowMs"].as_f64().unwrap(),
                );
                if json!(got) != row["out"] {
                    failures.push(format!(
                        "planRetention {}: v5 {got:?} v4 {}",
                        row["retentionDays"], row["out"]
                    ));
                }
            }
            "planFolds" => {
                let rs = row_refs(&row["rows"]);
                let refs: Vec<&Value> = rs.iter().collect();
                let thresholds = admission_thresholds_for_provider(row["provider"].as_str());
                let planned =
                    plan_near_duplicate_folds(&refs, thresholds, row["nowMs"].as_f64().unwrap());
                let got = json!({
                    "eligible": planned.eligible,
                    "deferred": planned.deferred,
                    "groups": planned.groups.iter().map(NearDuplicateGroup::to_json).collect::<Vec<_>>(),
                });
                if !approx_eq(&got, &row["out"]) {
                    failures.push(format!(
                        "planFolds {}:\n  v5 {got}\n  v4 {}",
                        row["name"], row["out"]
                    ));
                }
                if let Some(want) = row.get("eligible") {
                    let got: Vec<bool> = refs.iter().map(|m| is_fold_eligible(m)).collect();
                    if json!(got) != *want {
                        failures.push(format!(
                            "isFoldEligible {}: v5 {got:?} v4 {want}",
                            row["name"]
                        ));
                    }
                }
            }
            "bucketCandidates" => {
                let rs = row_refs(&row["rows"]);
                let refs: Vec<&Value> = rs.iter().collect();
                let b = bucket_candidates(
                    &refs,
                    row["holderId"].as_str().unwrap(),
                    row["matureCutoffMs"].as_f64().unwrap(),
                );
                let row_index: Vec<Value> = b
                    .row_index
                    .iter()
                    .map(
                        |(k, r)| json!([k, {"id": r.id, "relatedMemoryIds": r.related_memory_ids}]),
                    )
                    .collect();
                let buckets: Vec<Value> = b
                    .buckets
                    .iter()
                    .map(|k| {
                        let ids = |list: &[Value]| -> Vec<String> {
                            list.iter()
                                .map(|m| m["id"].as_str().unwrap().to_string())
                                .collect()
                        };
                        json!({
                            "key": k.key,
                            "ref": k.reference.to_json(),
                            "aboutCharacterId": k.about_character_id,
                            "rows": ids(&k.rows),
                            "digests": ids(&k.digests),
                        })
                    })
                    .collect();
                // v4's stats object carries every key; the loader only moves
                // the loader's counts, so compare those keys by name.
                let want_stats = &row["out"]["stats"];
                let got_stats = b.stats.to_json();
                let mut stats_ok = true;
                for k in [
                    "rowsLoaded",
                    "candidates",
                    "demotedCandidates",
                    "digestsLoaded",
                    "immatureSkipped",
                    "noEmbeddingSkipped",
                    "bucketRowsCapped",
                ] {
                    if got_stats[k] != want_stats[k] {
                        stats_ok = false;
                    }
                }
                let got_keys: Vec<&String> = got_stats.as_object().unwrap().keys().collect();
                let want_keys: Vec<&String> = want_stats.as_object().unwrap().keys().collect();
                if !stats_ok || got_keys != want_keys {
                    failures.push(format!(
                        "bucketCandidates {} stats:\n  v5 {got_stats}\n  v4 {want_stats}",
                        row["name"]
                    ));
                }
                if json!(row_index) != row["out"]["rowIndex"] {
                    failures.push(format!("bucketCandidates {} rowIndex", row["name"]));
                }
                if json!(buckets) != row["out"]["buckets"] {
                    failures.push(format!(
                        "bucketCandidates {} buckets:\n  v5 {}\n  v4 {}",
                        row["name"],
                        json!(buckets)
                            .to_string()
                            .chars()
                            .take(800)
                            .collect::<String>(),
                        row["out"]["buckets"]
                            .to_string()
                            .chars()
                            .take(800)
                            .collect::<String>()
                    ));
                }
            }
            "resolveClusterThreshold" => {
                let provider = if row["providerUndefined"] == json!(true) {
                    None
                } else {
                    row["provider"].as_str()
                };
                let got = resolve_cluster_threshold(0.72, 0.3, provider, row["override"].as_f64());
                if json!(got) != row["out"] && got != row["out"].as_f64().unwrap() {
                    failures.push(format!(
                        "resolveClusterThreshold {} {}: v5 {got} v4 {}",
                        row["provider"], row["override"], row["out"]
                    ));
                }
            }
            "ledger" => ledger_scenario(row, &mut failures),
            other => panic!("unknown oracle kind {other}"),
        }
    }

    assert!(
        failures.is_empty(),
        "{} failure(s):\n{}",
        failures.len(),
        failures
            .iter()
            .take(40)
            .cloned()
            .collect::<Vec<_>>()
            .join("\n\n")
    );
    assert!(
        counts.get("resolvePhases").copied().unwrap_or(0) >= 3000,
        "{counts:?}"
    );
    assert!(
        counts.get("ledger").copied().unwrap_or(0) >= 8,
        "{counts:?}"
    );
    eprintln!("OK: memory maintenance planning matched oracle {counts:?}.");
}

fn ledger_scenario(row: &Value, failures: &mut Vec<String>) {
    let name = row["name"].as_str().unwrap();
    reset_maintenance_ledger_for_tests();
    let dir = tempfile::tempdir().unwrap();
    let db = fresh_db(dir.path());
    if row["seedFails"] != json!(true) {
        insert_jobs(&db, row["seed"].as_array().unwrap());
    }
    for (i, step) in row["results"].as_array().unwrap().iter().enumerate() {
        let s = &step["step"];
        let op = s["op"].as_str().unwrap();
        let (result, lines) = global_capture::capture(|| -> Value {
            match op {
                "should" => {
                    let reading = s.get("reading").map(|r| WatermarkReading {
                        demotable: r["demotable"].as_i64().unwrap(),
                        cap: r["cap"].as_i64().unwrap(),
                        unconsidered: r["unconsidered"].as_i64().unwrap(),
                        consolidation_watermark: r["consolidationWatermark"].as_i64().unwrap(),
                        maintenance_enabled: r["maintenanceEnabled"].as_bool().unwrap(),
                        consolidation_enabled: r["consolidationEnabled"].as_bool().unwrap(),
                    });
                    match should_enqueue(
                        &db,
                        s["characterId"].as_str().unwrap(),
                        reason_of(s["reason"].as_str().unwrap()),
                        reading.as_ref(),
                        s["nowMs"].as_f64().unwrap(),
                    ) {
                        Some(phases) => json!(phases.iter().map(|p| p.as_str()).collect::<Vec<_>>()),
                        None => Value::Null,
                    }
                }
                "record" => {
                    record_maintenance_outcome(&summary_from(&s["summary"]));
                    Value::Null
                }
                "take" => json!(take_watermark_check(
                    s["characterId"].as_str().unwrap(),
                    s["nowMs"].as_f64().unwrap()
                )),
                "note" => {
                    let phases: Vec<MaintenancePhase> = strings(&s["phases"])
                        .unwrap()
                        .iter()
                        .map(|p| phase_of(p))
                        .collect();
                    note_enqueued(
                        s["characterId"].as_str().unwrap(),
                        &phases,
                        s["nowMs"].as_f64().unwrap(),
                    );
                    Value::Null
                }
                "status" => json!(get_maintenance_ledger_status(&db, s["characterId"].as_str())
                    .iter()
                    .map(|l| l.to_json())
                    .collect::<Vec<_>>()),
                "statusShape" => json!(get_maintenance_ledger_status(&db, None)
                    .iter()
                    .map(|l| {
                        let l = l.to_json();
                        let last_run = l["lastRun"].as_object().unwrap();
                        json!({
                            "characterId": l["characterId"],
                            "phases": last_run.keys().collect::<Vec<_>>(),
                            "reasons": last_run.values().map(|x| x["reason"].clone()).collect::<Vec<_>>(),
                            "hasSummary": !l["lastSummary"].is_null(),
                            "lastDemotion": if l["lastDemotion"].is_null() { Value::Null } else { json!({
                                "demoted": l["lastDemotion"]["demoted"],
                                "excess": l["lastDemotion"]["excess"],
                                "effective": l["lastDemotion"]["effective"],
                            }) },
                        })
                    })
                    .collect::<Vec<_>>()),
                "unthrow" => {
                    insert_jobs(&db, s["seed"].as_array().unwrap());
                    Value::Null
                }
                other => panic!("unknown ledger op {other}"),
            }
        });
        if !approx_eq(&result, &step["result"]) {
            failures.push(format!(
                "ledger {name} step {i} ({op}):\n  v5 {result}\n  v4 {}",
                step["result"]
            ));
        }
        let got_lines: Vec<String> = lines
            .iter()
            .map(|l| strip_target(l))
            .filter(|l| l.contains(" module=memory:maintenance:ledger"))
            .collect();
        let want_lines: Vec<String> = step["lines"]
            .as_array()
            .unwrap()
            .iter()
            .map(render_oracle_line)
            .collect();
        if got_lines != want_lines {
            failures.push(format!(
                "ledger {name} step {i} ({op}) lines:\n  v5 {got_lines:#?}\n  v4 {want_lines:#?}"
            ));
        }
    }
    reset_maintenance_ledger_for_tests();
}
