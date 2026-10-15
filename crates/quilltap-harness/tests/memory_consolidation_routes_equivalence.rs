//! P4.D271 route differential, reshaped by P4.D297 (bug 244, `b045dd986`):
//! `api::memory_consolidation::*` vs v4's REAL memories route handlers
//! (`app/api/v1/memories/route.ts` at the `83334b9a5` pin) — GET/POST
//! `?action=consolidation-config`, GET/POST `?action=extraction-mode-config`,
//! and the RETIRED POST `?action=consolidate` (v4 `b045dd986` — `maintain`
//! covers it; v4 answers its `Unknown action` 400 and v5's dispatch the serde
//! `unknown variant` 400, R-G: the rows compare the 400 class and the `error`
//! text the route answers) — each case over a FRESH copy of the committed
//! memories-{main,mount}.db pair. Comparands: success bodies whole, errors as
//! status + `error` message (v5's house class answers no `details` — R-K),
//! the stored settings after each config call, and the captured `[Memories
//! API]` lines (v4's `fields` array under v5's `fieldsJson` key).
//!
//! **`PATCH_PRESERVES_STORED` (both ways — P4.D297 retired P4.D271 R-A's
//! `PARTIAL_DEFAULTS_CLOBBER`).** Bug 244's `patchSchemaOf` keeps a POST to
//! the keys it SENT: a tuned stored value survives a write of another key on
//! BOTH sides, and the INFO's `fields` names only the sent keys. Measured at
//! the pin by P4.D297's red-first (lane record): `clobber_cold_retention`
//! stores `{enabled: true, clusterThreshold: 0.5, …, watermark: 300,
//! coldRetentionDays: 90}` with `fields ["coldRetentionDays"]`;
//! `clobber_null_cold_retention` keeps `enabled: true` with
//! `coldRetentionDays: null`; `clobber_fold_candidates` keeps `otherPass:
//! 'fold'`, `perTurnOtherFloor: 0.5`. A v4 regression (the clobber back) turns
//! the explicit rows below red on the v4 side.
//!
//! Generate (Node 24, from the v4 checkout or the round's pin — a /tmp mirror):
//!   N=~/.nvm/versions/node/v24.13.1/bin ; V5W=${V5W:-$HOME/source/quilltap-v5} ; M=/tmp/qt-consolidation-oracle
//!   rm -rf $M && mkdir -p $M && cp -R $V5W/harness/oracle/* $M/
//!   cd ~/source/quilltap-server
//!   rm -f /tmp/qt-consolidation-routes.ndjson
//!   QT_FIXTURE_MEM_MAIN=$V5W/crates/quilltap-web/tests/fixtures/memories-main.db \
//!   QT_FIXTURE_MEM_MOUNT=$V5W/crates/quilltap-web/tests/fixtures/memories-mount.db \
//!   QT_ORACLE_OUT=/tmp/qt-consolidation-routes.ndjson \
//!     $N/npx jest --silent --watchman=false --testTimeout=180000 \
//!       --roots "$PWD" --roots "$M/cases" -- memory-consolidation-routes
//!   cd $V5W
//!   QT_ORACLE_CONSOLIDATION_ROUTES=/tmp/qt-consolidation-routes.ndjson \
//!     cargo test -p quilltap-harness --test memory_consolidation_routes_equivalence -- --nocapture

use std::path::PathBuf;

use quilltap_core::api::memory_consolidation as api;
use quilltap_core::api::types::{ErrorKind, Response};
use quilltap_core::db::instance_settings::{
    get_memory_consolidation_settings, get_memory_extraction_mode_settings,
    set_memory_consolidation_settings, set_memory_extraction_mode_settings,
};
use quilltap_core::db::runtime::{Db, DbPaths};
use quilltap_core::test_support::global_capture;
use serde_json::{json, Value};

fn status_of(kind: ErrorKind) -> u16 {
    match kind {
        ErrorKind::BadRequest => 400,
        ErrorKind::Unauthorized => 401,
        ErrorKind::Forbidden => 403,
        ErrorKind::NotFound => 404,
        ErrorKind::Conflict => 409,
        ErrorKind::Unprocessable => 422,
        ErrorKind::Locked => 423,
        ErrorKind::Unavailable => 503,
        ErrorKind::Internal => 500,
    }
}

fn status_and_body(r: &Response) -> (u16, Value) {
    match r {
        Response::Error(e) => (status_of(e.kind), json!({ "error": e.message })),
        other => {
            let v = serde_json::to_value(other).unwrap();
            (200, v.get("data").cloned().unwrap_or(Value::Null))
        }
    }
}

fn mask_uuids(s: &str, keep: &[&str]) -> String {
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
                out.push_str(if keep.contains(&cand) { cand } else { "<id>" });
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

/// Numbers compared as f64; a dry run's clocks blanked.
fn normalize(v: &Value) -> Value {
    match v {
        Value::Number(n) => json!(n.as_f64().unwrap()),
        Value::Array(a) => Value::Array(a.iter().map(normalize).collect()),
        Value::Object(o) => Value::Object(
            o.iter()
                .map(|(k, x)| {
                    let x = match k.as_str() {
                        "startedAt" | "finishedAt" | "durationMs" => json!("<clock>"),
                        _ => normalize(x),
                    };
                    (k.clone(), x)
                })
                .collect(),
        ),
        other => other.clone(),
    }
}

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
    out
}

fn strip_target(line: &str) -> String {
    let mut parts = line.splitn(3, ' ');
    let level = parts.next().unwrap_or_default();
    let _ = parts.next();
    format!("{level} {}", parts.next().unwrap_or_default())
}

fn open_copy(dir: &std::path::Path, pepper: &str) -> Db {
    let fx = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../quilltap-web/tests/fixtures");
    let main = dir.join("main.db");
    let mount = dir.join("mount.db");
    std::fs::copy(fx.join("memories-main.db"), &main).unwrap();
    std::fs::copy(fx.join("memories-mount.db"), &mount).unwrap();
    Db::open(
        DbPaths {
            main,
            mount_index: Some(mount),
            llm_logs: None,
        },
        pepper,
    )
    .expect("open the memories-web copy")
}

fn jobs(db: &Db) -> Value {
    db.read_main(|c| {
        let mut stmt = c.prepare(
            "SELECT userId, type, status, payload, maxAttempts FROM background_jobs \
             WHERE type = 'MEMORY_CONSOLIDATION' ORDER BY createdAt",
        )?;
        let rows = stmt.query_map([], |r| {
            Ok(json!({
                "userId": r.get::<_, String>(0)?,
                "type": r.get::<_, String>(1)?,
                "status": r.get::<_, String>(2)?,
                "payload": r.get::<_, String>(3)?,
                "maxAttempts": r.get::<_, f64>(4)?,
            }))
        })?;
        Ok(Value::Array(rows.collect::<Result<Vec<_>, _>>()?))
    })
    .unwrap()
}

#[tokio::test(flavor = "current_thread")]
async fn memory_consolidation_routes_match_oracle() {
    let Ok(oracle_path) = std::env::var("QT_ORACLE_CONSOLIDATION_ROUTES") else {
        eprintln!("SKIP: set QT_ORACLE_CONSOLIDATION_ROUTES (see header).");
        return;
    };
    global_capture::install();
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../harness/oracle/fixtures");
    let spec: Value =
        serde_json::from_str(&std::fs::read_to_string(root.join("memories-web.json")).unwrap())
            .unwrap();
    let corpus: Value = serde_json::from_str(
        &std::fs::read_to_string(root.join("memory-consolidation-routes.json")).unwrap(),
    )
    .unwrap();
    let oracle: Vec<Value> = std::fs::read_to_string(&oracle_path)
        .unwrap_or_else(|e| panic!("read oracle {oracle_path}: {e}"))
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| serde_json::from_str(l).unwrap())
        .collect();
    let pepper = spec["testPepperBase64"].as_str().unwrap();
    let user_id = spec["userId"].as_str().unwrap();
    let keep = [
        "b1000000-0000-4000-8000-000000000001",
        "b1000000-0000-4000-8000-000000000002",
        user_id,
    ];
    let cases = corpus["cases"].as_array().unwrap();
    assert_eq!(
        cases.len(),
        oracle.len(),
        "case count — regenerate the oracle"
    );
    let mut failures: Vec<String> = Vec::new();
    let mut results: std::collections::HashMap<String, Value> = Default::default();
    for (c, want) in cases.iter().zip(oracle.iter()) {
        let name = c["name"].as_str().unwrap();
        assert_eq!(want["name"], name, "case order");
        let dir = tempfile::tempdir().unwrap();
        let db = open_copy(dir.path(), pepper);
        if let Some(stored) = c.get("stored") {
            if let Some(cs) = stored.get("consolidation").cloned() {
                db.write(move |w| set_memory_consolidation_settings(w.main().connection(), &cs))
                    .await
                    .unwrap();
            }
            if let Some(es) = stored.get("extraction").cloned() {
                db.write(move |w| set_memory_extraction_mode_settings(w.main().connection(), &es))
                    .await
                    .unwrap();
            }
        }
        let action = c["action"].as_str().unwrap();
        let method = c["method"].as_str().unwrap();
        let body = c.get("body").cloned().unwrap_or(json!({}));
        let call = || async {
            match (action, method) {
                ("consolidation-config", "GET") => api::memory_consolidation_config_get(&db),
                ("consolidation-config", _) => {
                    api::memory_consolidation_config_set(&db, body.clone()).await
                }
                ("extraction-mode-config", "GET") => api::memory_extraction_mode_config_get(&db),
                ("extraction-mode-config", _) => {
                    api::memory_extraction_mode_config_set(&db, body.clone()).await
                }
                // P4.D288 (compile-forced, R-G): the `consolidate` verb and its
                // handler are DELETED with v4's (`104e986c7` — v4 answers its
                // `Unknown action` envelope); P4.D297 re-points this family.
                ("consolidate", _) => {
                    Response::error(ErrorKind::BadRequest, "Unknown action: consolidate")
                }
                other => panic!("unknown route {other:?}"),
            }
        };
        let twice = c.get("twice").and_then(Value::as_bool).unwrap_or(false);
        let ((first, resp), captured) = global_capture::capture_async(async {
            let first = call().await;
            let resp = if twice { call().await } else { first.clone() };
            (first, resp)
        })
        .await;
        let (status, got_body) = status_and_body(&resp);
        if u64::from(status) != want["status"].as_u64().unwrap() {
            failures.push(format!("{name}: status v5 {status} v4 {}", want["status"]));
        }
        let want_body = if status == 200 {
            want["body"].clone()
        } else {
            json!({ "error": want["body"]["error"] })
        };
        let g = mask_uuids(&normalize(&got_body).to_string(), &keep);
        let w = mask_uuids(&normalize(&want_body).to_string(), &keep);
        if g != w {
            failures.push(format!("{name}: body\n  v5 {g}\n  v4 {w}"));
        }
        if twice {
            let (_, first_body) = status_and_body(&first);
            if first_body["jobId"] != got_body["jobId"] {
                failures.push(format!("{name}: the dedupe returned a different job id"));
            }
        }
        if let Some(stored) = want.get("storedAfter") {
            let got_stored = if action == "consolidation-config" {
                db.read_main(|cn| Ok(get_memory_consolidation_settings(cn)))
                    .unwrap()
                    .to_json()
            } else {
                db.read_main(|cn| Ok(get_memory_extraction_mode_settings(cn)))
                    .unwrap()
                    .to_json()
            };
            if normalize(&got_stored) != normalize(stored) {
                failures.push(format!("{name}: storedAfter v5 {got_stored} v4 {stored}"));
            }
            results.insert(name.to_string(), got_stored);
        }
        if let Some(want_jobs) = want.get("jobs") {
            let g = normalize(&jobs(&db));
            if g != normalize(want_jobs) {
                failures.push(format!("{name}: jobs v5 {g} v4 {want_jobs}"));
            }
        }
        let got_lines: Vec<String> = captured
            .iter()
            .map(|l| strip_target(l))
            .filter(|l| {
                l.split_once(' ').is_some_and(|(_, m)| {
                    m.starts_with("[Memories API] Consolidation")
                        || m.starts_with("[Memories API] Extraction-mode")
                        || m.starts_with("[Memories API] Failed to enqueue consolidation")
                })
            })
            .map(|l| mask_uuids(&l, &keep))
            .collect();
        let want_lines: Vec<String> = want["lines"]
            .as_array()
            .unwrap()
            .iter()
            .map(|l| mask_uuids(&render_oracle_line(l), &keep))
            .collect();
        if got_lines != want_lines {
            failures.push(format!(
                "{name}: lines\n  v5 {got_lines:#?}\n  v4 {want_lines:#?}"
            ));
        }
    }

    // ── PATCH_PRESERVES_STORED — the tuned values KEPT, on BOTH sides ───────
    let oracle_stored = |name: &str| -> Value {
        oracle
            .iter()
            .find(|o| o["name"] == name)
            .unwrap_or_else(|| panic!("oracle row {name}"))["storedAfter"]
            .clone()
    };
    for (side, stored) in [
        ("v4", oracle_stored("clobber_cold_retention")),
        ("v5", results["clobber_cold_retention"].clone()),
    ] {
        if stored["enabled"] != json!(true)
            || stored["clusterThreshold"].as_f64() != Some(0.5)
            || stored["watermark"].as_f64() != Some(300.0)
            || stored["coldRetentionDays"].as_f64() != Some(90.0)
        {
            failures.push(format!(
                "PATCH_PRESERVES_STORED clobber_cold_retention ({side}): the tuned values were expected KEPT — {stored} (the clobber back? re-measure)"
            ));
        }
    }
    for (side, stored) in [
        ("v4", oracle_stored("clobber_null_cold_retention")),
        ("v5", results["clobber_null_cold_retention"].clone()),
    ] {
        if stored["enabled"] != json!(true) || !stored["coldRetentionDays"].is_null() {
            failures.push(format!(
                "PATCH_PRESERVES_STORED clobber_null_cold_retention ({side}): {stored}"
            ));
        }
    }
    for (side, stored) in [
        ("v4", oracle_stored("clobber_fold_candidates")),
        ("v5", results["clobber_fold_candidates"].clone()),
    ] {
        if stored["otherPass"] != json!("fold")
            || stored["perTurnOtherFloor"].as_f64() != Some(0.5)
            || stored["foldCandidatesPerSubject"].as_f64() != Some(5.0)
        {
            failures.push(format!(
                "PATCH_PRESERVES_STORED clobber_fold_candidates ({side}): {stored}"
            ));
        }
    }
    // The INFO names only the sent keys (bug 244), both sides: the one-key
    // write lists one key, the empty write none.
    for (name, want_fields) in [
        ("clobber_cold_retention", r#"["coldRetentionDays"]"#),
        ("consolidation_set_empty", "[]"),
        ("clobber_fold_candidates", r#"["foldCandidatesPerSubject"]"#),
    ] {
        let row = oracle.iter().find(|o| o["name"] == name).unwrap();
        let fields = serde_json::to_string(&row["lines"][0]["context"]["fields"]).unwrap();
        if fields != want_fields {
            failures.push(format!(
                "PATCH_PRESERVES_STORED {name} (v4): fields {fields}, expected {want_fields}"
            ));
        }
    }

    assert!(
        failures.is_empty(),
        "{} failure(s):\n{}",
        failures.len(),
        failures.join("\n\n")
    );
    eprintln!(
        "OK: memory consolidation routes matched oracle ({} cases, PATCH_PRESERVES_STORED 3 rows both ways + 3 field rows).",
        cases.len()
    );
}
