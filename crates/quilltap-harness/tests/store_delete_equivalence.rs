//! P4.31 STORE-DELETE cascade differential. `api::mount_points::mount_point_delete`
//! plus the boot orphan reaper, against v4's REAL `DELETE /api/v1/mount-points/[id]`,
//! over a FRESH copy of the committed `store-delete-*` fixture per case, diffed with
//! a WHOLE-TABLE census of the mount-index partition.
//!
//! **Why a whole-table census.** The existing route family
//! (`mount_points_routes_equivalence`'s `dump_cascade`) counts the deleted store's
//! children through subqueries scoped to that store's own `doc_mount_file_links`
//! rows. The cascade has just emptied that table, so every such count reads 0 on
//! both sides no matter how much either app leaked — which is precisely why the
//! bug this lane closes survived a differential-verified port. Nothing there
//! needed changing; it is structurally blind to this class, and this family
//! carries the census instead.
//!
//! **The divergences — CONVERGED (bug 9).** v5 fixed the leaky delete path first
//! (P4.31): a single-transaction cascade that leaks no orphaned
//! `doc_mount_documents` or `group_doc_mount_links`. v4 has since adopted it
//! (`3bb664f0`, `delete-store-cascade.ts`), so those tables are now a PLAIN
//! equality. The `reap_orphans` arm stays — it exercises v5's boot orphan reaper
//! (`sweep_orphaned_store_children`), a v5-only pass that clears the fixture's
//! planted orphans; v4 has no equivalent in the DELETE route.
//!
//! Generate the oracle (Node 24, from the v4 checkout — see the .test.ts header):
//!   … QT_ORACLE_OUT=/tmp/oracle-store-delete.ndjson npx jest -- store-delete
//! Run:
//!   QT_ORACLE_STORE_DELETE=/tmp/oracle-store-delete.ndjson \
//!     cargo test -p quilltap-harness --test store_delete_equivalence -- --nocapture

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::path::PathBuf;

use quilltap_core::api::mount_points as mp;
use quilltap_core::api::types::Response;
use quilltap_core::db::doc_mount_file_links::sweep_orphaned_store_children;
use quilltap_core::db::runtime::{Db, DbPaths};
use serde::Deserialize;
use serde_json::{json, Map, Value};

const MP_TARGET: &str = "b9000000-0000-4000-8000-000000000001";
const MP_KEEPER: &str = "b9000000-0000-4000-8000-000000000002";
/// The planted orphans' vanished parent — no `doc_mount_points` row.
const MP_GHOST: &str = "b9000000-0000-4000-8000-0000000000ee";
/// Never existed at all.
const MP_BOGUS: &str = "b9000000-0000-4000-8000-0000000000ff";

/// The planted-orphan shape the fixture builder pins (`build-store-delete-fixture.ts`
/// refuses to write a fixture that drifts from it). Asserted on the ORACLE side
/// too, so a rebuilt fixture that quietly lost its orphans cannot make the reaper
/// arm pass by having nothing to reap.
const PLANTED_ORPHANS: (usize, usize, usize) = (2, 4, 3); // links, folders, chunks

// ── The two former divergences — CONVERGED (bug 9, v4 `3bb664f0`) ────────────
//
// Both were *dangling references* v4 used to leak on a store delete and v5 never
// did (P4.31). v4's new `delete-store-cascade.ts` runs the whole cascade in one
// transaction and leaks neither, so both are now PLAIN equalities:
//
// 1. `doc_mount_documents` with a `fileId` matching no `doc_mount_files` row —
//    v4 used to delete files before selecting their document ids, so the bodies
//    leaked. Fixed.
// 2. `group_doc_mount_links` with a `mountPointId` matching no
//    `doc_mount_points` row — v4's delete route used to never touch that table.
//    Fixed (`deleteByMountPointId` added).
//
// The retirement's tripwire: the oracle census is asserted to carry NO orphan of
// either shape (in `store_delete_matches_oracle`); if v4 regresses, it reddens.
//
// NOT a divergence, though the survey predicted one: `doc_mount_blobs`. v4's
// blobs step is genuinely dead (a copy-paste of the files step that never names
// the table), but `doc_mount_blobs` is the one mount table whose hand-written
// DDL carries `fileId … ON DELETE CASCADE` on BOTH vintages, and v4 enables
// `foreign_keys = ON` on its mount-index client exactly as v5 does on its
// writers — so the blob dies on both sides anyway. Measured, not assumed: the
// `delete_target` oracle census drops `doc_mount_blobs` 2 → 1. v5 collects it
// explicitly regardless, because the cascade should not depend on a pragma it
// does not set (see `api::mount_points::cascade_delete`); the unit test
// `cascade_collects_blobs_and_documents_without_any_foreign_key` proves that half, which no fixture
// built by v4 can.

#[derive(Clone, Copy)]
enum Op {
    /// No write — the census is the fixture as committed.
    None,
    Delete(&'static str),
    DeleteTwice(&'static str),
    /// The v5-only boot repair pass.
    Reap,
}

struct Arm {
    case: &'static str,
    op: Op,
    /// This arm runs the reaper, so v5 additionally clears the planted orphans.
    reaps: bool,
}

const ARMS: &[Arm] = &[
    Arm {
        case: "pristine",
        op: Op::None,
        reaps: false,
    },
    Arm {
        case: "delete_target",
        op: Op::Delete(MP_TARGET),
        reaps: false,
    },
    Arm {
        case: "delete_keeper",
        op: Op::Delete(MP_KEEPER),
        reaps: false,
    },
    Arm {
        case: "delete_twice",
        op: Op::DeleteTwice(MP_TARGET),
        reaps: false,
    },
    Arm {
        case: "delete_404",
        op: Op::Delete(MP_BOGUS),
        reaps: false,
    },
    Arm {
        case: "delete_ghost_404",
        op: Op::Delete(MP_GHOST),
        reaps: false,
    },
    Arm {
        case: "reap_orphans",
        op: Op::Reap,
        reaps: true,
    },
];

/// Table → the column list dumped, mirroring the oracle's `TABLES` verbatim.
const TABLES: &[(&str, &str)] = &[
    (
        "doc_mount_points",
        "id, name, mountType, storeType, enabled, fileCount, chunkCount, totalSizeBytes",
    ),
    (
        "doc_mount_files",
        "id, sha256, fileSizeBytes, fileType, source",
    ),
    (
        "doc_mount_file_links",
        "id, fileId, linkGroupId, mountPointId, relativePath, fileName, folderId, chunkCount",
    ),
    (
        "doc_mount_documents",
        "id, fileId, contentSha256, plainTextLength",
    ),
    (
        "doc_mount_blobs",
        "id, fileId, sha256, sizeBytes, storedMimeType",
    ),
    (
        "doc_mount_folders",
        "id, mountPointId, parentId, name, path",
    ),
    (
        "doc_mount_chunks",
        "id, linkId, mountPointId, chunkIndex, content, tokenCount",
    ),
    ("group_doc_mount_links", "id, groupId, mountPointId"),
    ("project_doc_mount_links", "id, projectId, mountPointId"),
];

const ORPHANABLE: &[&str] = &[
    "doc_mount_file_links",
    "doc_mount_folders",
    "doc_mount_chunks",
];

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Spec {
    test_pepper_base64: String,
    #[allow(dead_code)]
    user_id: String,
}

fn spec() -> Spec {
    let p = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../harness/oracle/fixtures/store-delete.json");
    serde_json::from_str(&std::fs::read_to_string(p).unwrap()).expect("spec")
}

fn fixtures_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../quilltap-web/tests/fixtures")
}

fn fresh_db(spec: &Spec, tag: &str) -> Db {
    let scratch = std::env::temp_dir().join(format!("qt-sd-{}-{}", tag, std::process::id()));
    let _ = std::fs::remove_dir_all(&scratch);
    std::fs::create_dir_all(&scratch).unwrap();
    let main = scratch.join("main.db");
    let mount = scratch.join("mount.db");
    std::fs::copy(fixtures_dir().join("store-delete-main.db"), &main).unwrap();
    std::fs::copy(fixtures_dir().join("store-delete-mount.db"), &mount).unwrap();
    Db::open(
        DbPaths {
            main,
            mount_index: Some(mount),
            llm_logs: None,
        },
        &spec.test_pepper_base64,
    )
    .expect("open db")
}

/// SQLite value → JSON, matching what better-sqlite3 hands the oracle. Integer
/// REALs are canonicalised on both sides by [`canon_numbers`].
fn cell(v: rusqlite::types::ValueRef<'_>) -> Value {
    use rusqlite::types::ValueRef as V;
    match v {
        V::Null => Value::Null,
        V::Integer(i) => json!(i),
        V::Real(f) => json!(f),
        V::Text(t) => Value::String(String::from_utf8_lossy(t).into_owned()),
        V::Blob(b) => json!(format!("<blob {} bytes>", b.len())),
    }
}

fn canon_numbers(v: &mut Value) {
    match v {
        Value::Number(n) => {
            if let Some(f) = n.as_f64() {
                if f.is_finite() && f.fract() == 0.0 && f.abs() < 9.007_199_254_740_992e15 {
                    *v = Value::Number((f as i64).into());
                }
            }
        }
        Value::Array(a) => a.iter_mut().for_each(canon_numbers),
        Value::Object(o) => o.iter_mut().for_each(|(_, x)| canon_numbers(x)),
        _ => {}
    }
}

fn census(db: &Db) -> Value {
    db.read_mount_index(|conn| {
        let mut counts = Map::new();
        let mut rows = Map::new();
        for (table, cols) in TABLES {
            let n: i64 = conn.query_row(&format!("SELECT COUNT(*) FROM \"{table}\""), [], |r| {
                r.get(0)
            })?;
            counts.insert((*table).to_string(), json!(n));
            let mut stmt = conn.prepare(&format!("SELECT {cols} FROM \"{table}\" ORDER BY id"))?;
            let names: Vec<String> = stmt
                .column_names()
                .into_iter()
                .map(|s| s.to_string())
                .collect();
            let out = stmt
                .query_map([], |r| {
                    let mut o = Map::new();
                    for (i, name) in names.iter().enumerate() {
                        o.insert(name.clone(), cell(r.get_ref(i)?));
                    }
                    Ok(Value::Object(o))
                })?
                .collect::<Result<Vec<_>, _>>()?;
            rows.insert((*table).to_string(), Value::Array(out));
        }
        let mut orphans = Map::new();
        for table in ORPHANABLE {
            let n: i64 = conn.query_row(
                &format!(
                    "SELECT COUNT(*) FROM \"{table}\" t WHERE NOT EXISTS \
                     (SELECT 1 FROM doc_mount_points p WHERE p.id = t.mountPointId)"
                ),
                [],
                |r| r.get(0),
            )?;
            orphans.insert((*table).to_string(), json!(n));
        }
        let mut v = json!({ "counts": counts, "orphans": orphans, "rows": rows });
        canon_numbers(&mut v);
        Ok(v)
    })
    .expect("census")
}

// ── Census helpers, run over EITHER side's dump ──────────────────────────────

fn table_rows<'a>(c: &'a Value, table: &str) -> &'a Vec<Value> {
    c["rows"][table]
        .as_array()
        .unwrap_or_else(|| panic!("census missing rows.{table}"))
}

fn ids(c: &Value, table: &str) -> BTreeSet<String> {
    table_rows(c, table)
        .iter()
        .map(|r| r["id"].as_str().unwrap().to_string())
        .collect()
}

fn refs(c: &Value, table: &str, column: &str) -> BTreeSet<String> {
    table_rows(c, table)
        .iter()
        .filter_map(|r| r[column].as_str().map(|s| s.to_string()))
        .collect()
}

/// Row ids in `table` whose `column` references a row id absent from `parent`.
fn dangling(c: &Value, table: &str, column: &str, parent: &str) -> BTreeSet<String> {
    let live = ids(c, parent);
    table_rows(c, table)
        .iter()
        .filter(|r| r[column].as_str().is_none_or(|v| !live.contains(v)))
        .map(|r| r["id"].as_str().unwrap().to_string())
        .collect()
}

/// Row ids in `table` whose `mountPointId` names no surviving store.
fn orphaned(c: &Value, table: &str) -> BTreeSet<String> {
    dangling(c, table, "mountPointId", "doc_mount_points")
}

/// Build the census v5 is expected to produce, by removing from the ORACLE's
/// census exactly the rows the ruled divergences say v5 does not keep. Everything
/// else must then match row for row — no per-table carve-outs, no normalizer that
/// could hide an unrelated difference.
fn expected_from_oracle(oracle: &Value, arm: &Arm) -> Value {
    // [bug 9] The delete-cascade leak divergences (orphaned doc_mount_documents /
    // group_doc_mount_links) are RETIRED — v4 converged (`3bb664f0`), so nothing
    // is dropped on the delete arms and v5 must equal v4's census row for row. The
    // reaper arm still drops the rows v5's boot reaper collects (a v5-only pass).
    let mut drop: BTreeMap<&str, BTreeSet<String>> = BTreeMap::new();

    if arm.reaps {
        // The reaper: the parentless links / folders / chunks, plus the content
        // whose ONLY references were those links.
        let doomed_links = orphaned(oracle, "doc_mount_file_links");
        drop.insert("doc_mount_file_links", doomed_links.clone());
        drop.insert("doc_mount_folders", orphaned(oracle, "doc_mount_folders"));
        drop.insert("doc_mount_chunks", orphaned(oracle, "doc_mount_chunks"));

        let mut surviving_file_refs: BTreeSet<String> = BTreeSet::new();
        for link in table_rows(oracle, "doc_mount_file_links") {
            let id = link["id"].as_str().unwrap();
            if !doomed_links.contains(id) {
                if let Some(f) = link["fileId"].as_str() {
                    surviving_file_refs.insert(f.to_string());
                }
            }
        }
        let doomed_files: BTreeSet<String> = table_rows(oracle, "doc_mount_file_links")
            .iter()
            .filter(|l| doomed_links.contains(l["id"].as_str().unwrap()))
            .filter_map(|l| l["fileId"].as_str().map(|s| s.to_string()))
            .filter(|f| !surviving_file_refs.contains(f))
            .collect();
        drop.insert("doc_mount_files", doomed_files.clone());
        for payload in ["doc_mount_documents", "doc_mount_blobs"] {
            let mut set = drop.remove(payload).unwrap_or_default();
            for row in table_rows(oracle, payload) {
                if row["fileId"]
                    .as_str()
                    .is_some_and(|f| doomed_files.contains(f))
                {
                    set.insert(row["id"].as_str().unwrap().to_string());
                }
            }
            drop.insert(payload, set);
        }
    }

    let mut out = oracle.clone();
    for (table, _) in TABLES {
        let dropped = drop.get(table).cloned().unwrap_or_default();
        let kept: Vec<Value> = table_rows(oracle, table)
            .iter()
            .filter(|r| !dropped.contains(r["id"].as_str().unwrap()))
            .cloned()
            .collect();
        out["counts"][table] = json!(kept.len() as i64);
        out["rows"][table] = Value::Array(kept);
    }
    for table in ORPHANABLE {
        let n = orphaned(&out, table).len() as i64;
        out["orphans"][table] = json!(n);
    }
    out
}

fn response_data(r: &Response) -> Value {
    serde_json::to_value(r)
        .unwrap()
        .get("data")
        .cloned()
        .unwrap_or(Value::Null)
}

fn pretty(v: &Value) -> String {
    serde_json::to_string_pretty(v).unwrap()
}

fn first_diff(got: &str, want: &str) -> String {
    let g: Vec<&str> = got.lines().collect();
    let w: Vec<&str> = want.lines().collect();
    for i in 0..g.len().max(w.len()) {
        let gi = g.get(i).copied().unwrap_or("<none>");
        let wi = w.get(i).copied().unwrap_or("<none>");
        if gi != wi {
            return format!("  line {i}\n  GOT : {gi}\n  WANT: {wi}");
        }
    }
    "(identical)".to_string()
}

#[test]
fn store_delete_matches_oracle() {
    let Ok(oracle_path) = std::env::var("QT_ORACLE_STORE_DELETE") else {
        eprintln!("SKIP: set QT_ORACLE_STORE_DELETE (see test header).");
        return;
    };
    let spec = spec();
    let mut oracle: HashMap<String, Value> = HashMap::new();
    for line in std::fs::read_to_string(&oracle_path)
        .unwrap()
        .lines()
        .filter(|l| !l.trim().is_empty())
    {
        let v: Value = serde_json::from_str(line).unwrap();
        oracle.insert(v["name"].as_str().unwrap().to_string(), v);
    }
    assert_eq!(
        oracle.len(),
        ARMS.len(),
        "oracle case count {} != ARMS {} — regenerate the oracle, or add the arm",
        oracle.len(),
        ARMS.len()
    );

    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let mut failed: Vec<String> = Vec::new();

    for arm in ARMS {
        let name = arm.case;
        let want = oracle
            .get(name)
            .unwrap_or_else(|| panic!("oracle has no case {name} — regenerate it"));
        let db = fresh_db(&spec, name);

        // 1. Run the arm's operation and compare the wire, where there is one.
        let resp: Option<Response> = match arm.op {
            Op::None => None,
            Op::Delete(id) => Some(rt.block_on(mp::mount_point_delete(&db, id))),
            Op::DeleteTwice(id) => {
                let _ = rt.block_on(mp::mount_point_delete(&db, id));
                Some(rt.block_on(mp::mount_point_delete(&db, id)))
            }
            Op::Reap => {
                let reaped = db
                    .write_blocking(|ws| {
                        let conn = ws.mount_index().expect("mount-index writer").connection();
                        sweep_orphaned_store_children(conn)
                    })
                    .expect("reaper");
                // The reaper is idempotent: a second pass must find nothing.
                let again = db
                    .write_blocking(|ws| {
                        let conn = ws.mount_index().expect("mount-index writer").connection();
                        sweep_orphaned_store_children(conn)
                    })
                    .expect("reaper (second pass)");
                if !again.is_empty() {
                    eprintln!("[{name}] reaper is NOT idempotent: second pass {again:?}");
                    failed.push(name.to_string());
                }
                eprintln!("[{name}] reaped {reaped:?}");
                None
            }
        };
        if let Some(resp) = resp {
            let got_status = match &resp {
                Response::Error(e) => http_for(&resp, e),
                _ => 200,
            };
            let want_status = want["status"].as_i64().unwrap();
            if got_status != want_status {
                eprintln!("[{name}] status {got_status} != oracle {want_status}");
                failed.push(name.to_string());
            }
            let got_body = match &resp {
                Response::Error(e) => json!({ "error": e.message }),
                other => response_data(other),
            };
            if pretty(&got_body) != pretty(&want["body"]) {
                eprintln!(
                    "[{name}] BODY MISMATCH:\n{}",
                    first_diff(&pretty(&got_body), &pretty(&want["body"]))
                );
                failed.push(name.to_string());
            }
        }

        // 2. Both-direction divergence pins, over the ORACLE's own census.
        let oracle_census = &want["census"];
        // [bug 9] The delete-cascade leak (orphaned doc_mount_documents +
        // group_doc_mount_links) CONVERGED: v4 (`3bb664f0`,
        // `delete-store-cascade.ts`) now runs the whole cascade in one transaction
        // and leaks neither, exactly as v5 has since P4.31. So those tables are a
        // plain equality here (via `expected_from_oracle`, which no longer drops
        // them) and the direction-4 "v5 leaves no dangling row" check below is now
        // asserting agreement, not divergence. The oracle census is checked below
        // to carry NO such orphan (the retirement's own tripwire).
        for (table, column, parent) in [
            ("doc_mount_documents", "fileId", "doc_mount_files"),
            ("group_doc_mount_links", "mountPointId", "doc_mount_points"),
        ] {
            let leaked = dangling(oracle_census, table, column, parent);
            if !leaked.is_empty() {
                eprintln!(
                    "[{name}] v4 still leaks {} orphaned {table} row(s) — bug 9 was expected to \
                     have converged; if v4 has regressed, restore the both-directions divergence \
                     pin (see the git history for bug 9's leaked_* arms).",
                    leaked.len()
                );
                failed.push(name.to_string());
            }
        }
        // The planted orphans must still BE planted on v4's side, in every case
        // (nothing in v4 reaps them), or the reaper arm proves nothing.
        let planted = (
            orphaned(oracle_census, "doc_mount_file_links").len(),
            orphaned(oracle_census, "doc_mount_folders").len(),
            orphaned(oracle_census, "doc_mount_chunks").len(),
        );
        if planted != PLANTED_ORPHANS {
            eprintln!(
                "[{name}] oracle planted-orphan shape {planted:?} != {PLANTED_ORPHANS:?} — \
                 the fixture lost its orphans (or v4 grew a reaper)"
            );
            failed.push(name.to_string());
        }

        // 3. The state diff: v5 must equal v4 minus exactly the divergent rows.
        let got = census(&db);
        let expected = expected_from_oracle(oracle_census, arm);
        if pretty(&got) != pretty(&expected) {
            eprintln!(
                "[{name}] CENSUS MISMATCH (v5 vs v4-minus-the-ruled-divergences):\n{}",
                first_diff(&pretty(&got), &pretty(&expected))
            );
            failed.push(name.to_string());
        }

        // 4. …and v5 must carry no dangling reference of either shape, which is
        //    the other direction of the same pin.
        for (table, column, parent) in [
            ("doc_mount_documents", "fileId", "doc_mount_files"),
            ("doc_mount_blobs", "fileId", "doc_mount_files"),
            ("group_doc_mount_links", "mountPointId", "doc_mount_points"),
            (
                "project_doc_mount_links",
                "mountPointId",
                "doc_mount_points",
            ),
        ] {
            let bad = dangling(&got, table, column, parent);
            if !bad.is_empty() {
                eprintln!(
                    "[{name}] v5 left {} dangling {table}.{column} rows: {bad:?}",
                    bad.len()
                );
                failed.push(name.to_string());
            }
        }
        if arm.reaps {
            for table in ORPHANABLE {
                let n = got["orphans"][*table].as_i64().unwrap();
                if n != 0 {
                    eprintln!("[{name}] v5 left {n} orphaned {table} rows after the reaper");
                    failed.push(name.to_string());
                }
            }
        } else {
            // Outside the reaper arm v5 must leave the planted orphans exactly
            // as it found them — the cascade reaps its own store's children, not
            // the whole partition.
            let still = (
                orphaned(&got, "doc_mount_file_links").len(),
                orphaned(&got, "doc_mount_folders").len(),
                orphaned(&got, "doc_mount_chunks").len(),
            );
            if still != PLANTED_ORPHANS {
                eprintln!("[{name}] v5 planted-orphan shape {still:?} != {PLANTED_ORPHANS:?}");
                failed.push(name.to_string());
            }
        }

        if !failed.iter().any(|f| f == name) {
            eprintln!("[{name}] OK.");
        }
    }

    // A live cross-check that the appendix really is hard-linked: if the fixture
    // ever loses the shared file, `delete_keeper`'s refcount arm becomes vacuous.
    let db = fresh_db(&spec, "linkcheck");
    let base = census(&db);
    let shared: Vec<&Value> = table_rows(&base, "doc_mount_file_links")
        .iter()
        .filter(|l| l["linkGroupId"].as_str().is_some())
        .collect();
    assert_eq!(
        shared.len(),
        2,
        "the fixture must carry exactly one 2-member link group"
    );
    let group_files = refs(&base, "doc_mount_file_links", "fileId");
    assert!(!group_files.is_empty());
    let a = shared[0]["fileId"].as_str().unwrap();
    let b = shared[1]["fileId"].as_str().unwrap();
    assert_eq!(
        a, b,
        "the hard-linked pair must share one doc_mount_files row"
    );
    let mounts: BTreeSet<&str> = shared
        .iter()
        .map(|l| l["mountPointId"].as_str().unwrap())
        .collect();
    assert_eq!(
        mounts,
        BTreeSet::from([MP_TARGET, MP_KEEPER]),
        "the link group must straddle the target and the keeper"
    );

    assert!(failed.is_empty(), "store-delete divergences: {failed:?}");
}

/// The HTTP status v4 answers for this error kind, mirroring the route family's
/// table.
fn http_for(_r: &Response, e: &quilltap_core::api::types::CoreError) -> i64 {
    use quilltap_core::api::types::ErrorKind::*;
    match e.kind {
        BadRequest => 400,
        Unauthorized => 401,
        Forbidden => 403,
        NotFound => 404,
        Conflict => 409,
        Unprocessable => 422,
        Locked | Unavailable => 503,
        Internal => 500,
    }
}
