//! P4.D321 item 9 — the gallery's Wardrobe Pictures EDGE
//! (`GET /api/v1/characters/{id}/wardrobe-images`,
//! `quilltap-web::characters_routes::wardrobe_images_get` over
//! `quilltap-core::api::characters::character_wardrobe_images` →
//! `services::wardrobe_gallery_images` → `FilesRepository::find_by_linked_to_any`)
//! vs v4's REAL `app/api/v1/characters/[id]/wardrobe-images/route.ts` +
//! `FilesRepository.findByLinkedToAny` (`1cfc9b504`), op by op, over the
//! item-images tier-2 builder's fixture (the corpus:
//! `harness/oracle/fixtures/wardrobe-images-routes.json`).
//!
//! Two op kinds over ONE corpus (the order's R-8):
//!   - `repo` — `find_by_linked_to_any(ids)` on a planted copy: the ids it
//!     answers, IN ORDER (the row-order + dedupe proof the route leans on —
//!     measured: v4's `$in` over `json_each("linkedTo")` answers the TABLE's
//!     scan order, whatever the argument order), and its lines.
//!   - `route` — status + body BYTES through the REAL router of a host booted
//!     on a planted copy (`common::serve_instance`; key order included —
//!     `serde_json`'s `preserve_order` map equality is order-blind), and the
//!     lines of the SAME request driven straight through the core API fn on an
//!     unbooted planted copy (a served host logs off the test thread).
//!
//! Lines: every v4 line whose message starts with one of `LINE_PREFIXES` is
//! matched IN ORDER by a v5 line of the same level and message, carrying every
//! v4 context key (`key=`) and every scalar value (`key=value`); the ERROR's
//! third-argument `Error` is v5's `error=` field. NEGATIVE pin: `Error finding
//! files linked to entities` appears on NEITHER side (v4's `safeQuery` wrap
//! around `findByFilter` — itself a fallback read — never fires; v5's home is
//! `find_by_filter_or_empty`'s `Error finding entities by filter`).
//!
//! No normalization: every id the bodies carry is either pinned in the spec or
//! baked by the fixture build both sides read, and every `createdAt` is pinned
//! by the builder — so the sweep's one build feeds both sides (a rebuild between
//! the regen and the run would re-mint the picture ids and fail every row).
//!
//! Regenerate (see the oracle header — the fixture is the item-images tier-2
//! builder's, built into this family's OWN `/tmp/qt-wgi-*` paths):
//!   QT_ORACLE_WGI=/tmp/oracle-wardrobe-images-routes.ndjson \
//!   QT_FIXTURE_WGI_MAIN=/tmp/qt-wgi-main.db QT_FIXTURE_WGI_MOUNT=/tmp/qt-wgi-mount.db \
//!     cargo test -p quilltap-web --test wardrobe_images_routes_equivalence

mod common;

use std::path::{Path, PathBuf};
use std::sync::Arc;

use quilltap_core::api::characters::character_wardrobe_images;
use quilltap_core::api::engine::SINGLE_USER_ID;
use quilltap_core::api::types::Response;
use quilltap_core::db::files::FilesRepository;
use quilltap_core::db::runtime::{Db, DbPaths};
use quilltap_core::db::Writer;
use quilltap_core::test_support::captured_with;
use serde_json::Value;

/// The v4 message prefixes this family compares (the oracle's own list).
const LINE_PREFIXES: [&str; 6] = [
    "[Characters/WardrobeImages v1]",
    "[WardrobeGallery]",
    "[WearablePool]",
    "Data validation failed",
    "Safe validation failed",
    "Error finding",
];

/// v4's `safeQuery` wrap — unreachable through v4's real code (see the header).
const UNREACHABLE_WRAP: &str = "Error finding files linked to entities";

fn harness_fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../harness/oracle/fixtures")
        .join(name)
}

fn character_id(spec: &Value, v: &str) -> String {
    let key = match v {
        "@character" => "characterId",
        "@archived" => "archivedCharacterId",
        "@stranger" => "strangerCharacterId",
        other => return other.to_string(),
    };
    spec[key].as_str().unwrap().to_string()
}

fn strings(v: &Value, key: &str) -> Vec<String> {
    v.get(key)
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .map(|s| s.as_str().unwrap().to_string())
        .collect()
}

/// Copy the fixture pair to `dir` under `main_name` / `mount_name` and run the
/// op's plants on writable opens.
fn planted_copy(
    op: &Value,
    pepper: &str,
    main_fixture: &str,
    mount_fixture: &str,
    main_path: &Path,
    mount_path: &Path,
) {
    std::fs::copy(main_fixture, main_path).unwrap();
    std::fs::copy(mount_fixture, mount_path).unwrap();
    let main = Writer::open_writable(main_path, pepper).unwrap();
    let mount = Writer::open_writable(mount_path, pepper).unwrap();
    for sql in strings(op, "sql") {
        main.connection().execute_batch(&sql).unwrap();
    }
    for sql in strings(op, "mountSql") {
        mount.connection().execute_batch(&sql).unwrap();
    }
}

/// The captured lines this family compares (the same prefixes, after the
/// `<LEVEL> <target> ` head).
fn family_lines(lines: &[String]) -> Vec<String> {
    lines
        .iter()
        .filter(|l| {
            let rest = l.splitn(3, ' ').nth(2).unwrap_or_default();
            LINE_PREFIXES.iter().any(|p| rest.starts_with(p))
        })
        .cloned()
        .collect()
}

fn compare_lines(name: &str, want: &Value, got: &[String], failures: &mut Vec<String>) {
    for l in got {
        assert!(
            !l.contains(UNREACHABLE_WRAP),
            "{name}: v5 logged the unreachable wrap: {l}"
        );
    }
    let want = want.as_array().cloned().unwrap_or_default();
    let got = family_lines(got);
    if want.len() != got.len() {
        failures.push(format!(
            "{name}: {} v4 line(s), {} v5 line(s)\n  v4: {}\n  v5: {got:#?}",
            want.len(),
            got.len(),
            Value::Array(want.clone())
        ));
        return;
    }
    for (w, g) in want.iter().zip(&got) {
        let level = w["level"].as_str().unwrap().to_uppercase();
        let message = w["message"].as_str().unwrap();
        assert!(
            !message.contains(UNREACHABLE_WRAP),
            "{name}: v4 logged the unreachable wrap"
        );
        let head_ok = g.starts_with(&format!("{level} "))
            && g.splitn(3, ' ')
                .nth(2)
                .unwrap_or_default()
                .starts_with(message);
        let mut missing: Vec<String> = Vec::new();
        for k in w["keys"].as_array().unwrap() {
            let k = k.as_str().unwrap();
            if !g.contains(&format!(" {k}=")) {
                missing.push(format!("{k}="));
            }
        }
        for (k, v) in w["values"].as_object().unwrap() {
            let text = match v {
                Value::String(s) => s.clone(),
                Value::Null => continue,
                other => other.to_string(),
            };
            if !g.contains(&format!(" {k}={text}")) {
                missing.push(format!("{k}={text}"));
            }
        }
        if let Some(e) = w.get("error").and_then(Value::as_str) {
            if !g.contains(&format!(" error={e}")) {
                missing.push(format!("error={e}"));
            }
        }
        if !head_ok || !missing.is_empty() {
            failures.push(format!(
                "{name}: line `{message}` diverged (head ok: {head_ok}; missing {missing:?})\n  v5: {g}"
            ));
        }
    }
}

fn repo_op(
    spec: &Value,
    op: &Value,
    main_fixture: &str,
    mount_fixture: &str,
) -> (Vec<String>, Vec<String>) {
    let pepper = spec["testPepperBase64"].as_str().unwrap();
    let scratch = tempfile::tempdir().unwrap();
    let (mw, nw) = (
        scratch.path().join("main.db"),
        scratch.path().join("mount.db"),
    );
    planted_copy(op, pepper, main_fixture, mount_fixture, &mw, &nw);
    let main = Writer::open_writable(&mw, pepper).unwrap();
    let ids: Vec<String> = strings(op, "ids")
        .into_iter()
        .map(|k| {
            spec["items"]
                .get(&k)
                .map(|i| i["id"].as_str().unwrap().to_string())
                .unwrap_or(k)
        })
        .collect();
    let refs: Vec<&str> = ids.iter().map(String::as_str).collect();
    let (rows, lines) = captured_with(|| {
        FilesRepository::new(main.connection())
            .find_by_linked_to_any(&refs)
            .unwrap()
    });
    (rows.into_iter().map(|r| r.id).collect(), lines)
}

/// The core API fn on an unbooted planted copy, on THIS thread — its lines.
fn route_lines(spec: &Value, op: &Value, main_fixture: &str, mount_fixture: &str) -> Vec<String> {
    let pepper = spec["testPepperBase64"].as_str().unwrap();
    let scratch = tempfile::tempdir().unwrap();
    let (mw, nw) = (
        scratch.path().join("main.db"),
        scratch.path().join("mount.db"),
    );
    planted_copy(op, pepper, main_fixture, mount_fixture, &mw, &nw);
    let db = Db::open(
        DbPaths {
            main: mw,
            mount_index: Some(nw),
            llm_logs: None,
        },
        pepper,
    )
    .unwrap();
    let id = character_id(spec, op["character"].as_str().unwrap());
    let (resp, lines) = captured_with(|| character_wardrobe_images(&db, SINGLE_USER_ID, &id));
    // Keep the borrow honest: the response is the one the edge renders.
    let _ = matches!(resp, Response::CharacterWardrobeImages(_));
    lines
}

async fn route_http(
    spec: &Value,
    op: &Value,
    main_fixture: &str,
    mount_fixture: &str,
) -> (u16, String) {
    let pepper = spec["testPepperBase64"].as_str().unwrap().to_string();
    let base = tempfile::tempdir().unwrap();
    let data = base.path().join("data");
    std::fs::create_dir_all(&data).unwrap();
    planted_copy(
        op,
        &pepper,
        main_fixture,
        mount_fixture,
        &data.join("quilltap.db"),
        &data.join("quilltap-mount-index.db"),
    );
    let (addr, _state) = common::serve_instance(base.path(), |mut c| {
        c.terminal = false;
        c.env_pepper = Some(pepper.clone());
        // The host outlives its temp dir (never stopped), so its heartbeat finds
        // the lock gone; a lost lock here is the test's own teardown.
        c.on_lock_lost = Some(Arc::new(|| {}));
        c
    })
    .await;
    let id = character_id(spec, op["character"].as_str().unwrap());
    let resp = reqwest::Client::new()
        .get(format!(
            "http://{addr}/api/v1/characters/{id}/wardrobe-images"
        ))
        .send()
        .await
        .unwrap();
    let status = resp.status().as_u16();
    let text = resp.text().await.unwrap();
    // Re-serialize through `serde_json` (preserve_order) so both sides render
    // the same whitespace; the key ORDER survives.
    let body: Value = serde_json::from_str(&text).unwrap_or(Value::String(text));
    (status, serde_json::to_string(&body).unwrap())
}

#[tokio::test(flavor = "multi_thread")]
async fn wardrobe_images_routes_match_oracle() {
    let (Ok(oracle_path), Ok(main_fixture), Ok(mount_fixture)) = (
        std::env::var("QT_ORACLE_WGI"),
        std::env::var("QT_FIXTURE_WGI_MAIN"),
        std::env::var("QT_FIXTURE_WGI_MOUNT"),
    ) else {
        eprintln!(
            "SKIP: set QT_ORACLE_WGI + QT_FIXTURE_WGI_MAIN + QT_FIXTURE_WGI_MOUNT (see header)."
        );
        return;
    };
    let spec: Value = serde_json::from_str(
        &std::fs::read_to_string(harness_fixture("wardrobe-item-images-tier2.json")).unwrap(),
    )
    .unwrap();
    let corpus: Value = serde_json::from_str(
        &std::fs::read_to_string(harness_fixture("wardrobe-images-routes.json")).unwrap(),
    )
    .unwrap();
    let oracle: Vec<Value> = std::fs::read_to_string(&oracle_path)
        .unwrap()
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| serde_json::from_str(l).unwrap())
        .collect();
    let ops = corpus["ops"].as_array().unwrap();
    assert_eq!(oracle.len(), ops.len(), "oracle row count != corpus");

    let mut failures = Vec::new();
    let mut ran = 0;
    for (op, want) in ops.iter().zip(&oracle) {
        let name = op["name"].as_str().unwrap();
        assert_eq!(want["name"], op["name"], "oracle out of corpus order");
        if op["op"] == "repo" {
            let (ids, lines) = repo_op(&spec, op, &main_fixture, &mount_fixture);
            if ids != strings(want, "ids") {
                failures.push(format!(
                    "{name}: ids diverged\n  rust:   {ids:?}\n  oracle: {:?}",
                    strings(want, "ids")
                ));
            }
            compare_lines(name, &want["lines"], &lines, &mut failures);
        } else {
            let (status, body) = route_http(&spec, op, &main_fixture, &mount_fixture).await;
            if u64::from(status) != want["status"].as_u64().unwrap() {
                failures.push(format!(
                    "{name}: status {status} != oracle {}",
                    want["status"]
                ));
            }
            let want_body = serde_json::to_string(&want["body"]).unwrap();
            if body != want_body {
                failures.push(format!(
                    "{name}: body diverged\n  rust:   {body}\n  oracle: {want_body}"
                ));
            }
            let lines = route_lines(&spec, op, &main_fixture, &mount_fixture);
            compare_lines(name, &want["lines"], &lines, &mut failures);
        }
        ran += 1;
    }
    assert_eq!(ran, ops.len());
    assert!(
        failures.is_empty(),
        "{} divergence(s):\n{}",
        failures.len(),
        failures.join("\n\n")
    );
}

/// Item 10 — the edge over a booted instance with NO oracle needed: the GET is
/// v4's BARE `{ entries, total }` (no `data` wrapper, `entries` first), and a
/// POST is refused by the method router (v4's route file exports only `GET`,
/// so Next answers 405 — status compared only).
#[tokio::test(flavor = "multi_thread")]
async fn the_edge_answers_the_bare_body_and_refuses_other_methods() {
    let base = common::materialize_characters_instance();
    let (addr, _state) = common::serve_instance(base.path(), |mut c| {
        c.terminal = false;
        c.on_lock_lost = Some(Arc::new(|| {}));
        c
    })
    .await;
    let main =
        Writer::open_writable(&base.path().join("data/quilltap.db"), common::TEST_PEPPER).unwrap();
    let aria: String = main
        .connection()
        .query_row(
            "SELECT id FROM characters WHERE name = 'Aria' LIMIT 1",
            [],
            |r| r.get(0),
        )
        .unwrap();
    drop(main);
    let client = reqwest::Client::new();
    let url = format!("http://{addr}/api/v1/characters/{aria}/wardrobe-images");
    let resp = client.get(&url).send().await.unwrap();
    assert_eq!(resp.status().as_u16(), 200);
    let body: Value = serde_json::from_str(&resp.text().await.unwrap()).unwrap();
    assert_eq!(
        serde_json::to_string(&body).unwrap(),
        "{\"entries\":[],\"total\":0}",
        "the BARE body, entries before total"
    );
    let post = client.post(&url).send().await.unwrap();
    assert_eq!(post.status().as_u16(), 405);
    let absent = client
        .get(format!(
            "http://{addr}/api/v1/characters/99999999-1111-4000-8000-000000000000/wardrobe-images"
        ))
        .send()
        .await
        .unwrap();
    assert_eq!(absent.status().as_u16(), 404);
    assert_eq!(
        absent.text().await.unwrap(),
        "{\"error\":\"Character not found\"}"
    );
}
