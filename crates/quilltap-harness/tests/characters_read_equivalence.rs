//! Read-differential test: v4's `CharactersRepository` findBy* queries (Phase-2,
//! the store-backed capstone sub-unit 4c — the read path).
//!
//! Both sides READ the SAME baked fixture (four characters + vaults created by v4's
//! real `repos.characters.create`), run the SAME query list, and the hydrated
//! result lists are compared per query. Because both read a COPY of the one baked
//! fixture, the minted ids/timestamps are IDENTICAL on both sides (no remap) — the
//! lists compare exactly, except `physicalDescription.createdAt`/`updatedAt` (minted
//! at hydration on each side, placeholdered here). Each result list is sorted by
//! character id before comparison (the queries carry no ORDER BY).
//!
//! The Rust port drives [`characters_read`]'s query functions over two writers; v4
//! drives the real repository methods (see the oracle). Id-taking queries
//! (findById / findByIdRaw / findByIds) carry a `targetName` resolved to the minted
//! id via a name->id lookup on each side.
//!
//! Banks: the slim-row read marshaling in isolation (findByIdRaw — managed fields at
//! their Zod defaults; nullable cells omitted, JSON-object/array columns parsed,
//! booleans coerced), the overlaid single read (findById), findAll, the userId /
//! controlledBy filters (findByUserId / findUserControlled / findLLMControlled), the
//! id-set query (findByIds), and the JSON-array `json_each` filters
//! (findByDefaultImageId / findByAvatarOverrideImageId / findByTag).
//!
//! Generate the oracle output + fixtures (Node 24, from the v4 checkout):
//!   N=~/.nvm/versions/node/v24.13.1/bin
//!   cd ~/source/quilltap-server
//!   QT_FIXTURE_CHARREAD_MAIN=/tmp/qt-charread-main.db \
//!   QT_FIXTURE_CHARREAD_MOUNT=/tmp/qt-charread-mount.db \
//!     $N/npx tsx ~/source/quilltap-v5/harness/oracle/fixtures/build-characters-read-fixture.ts
//!   QT_FIXTURE_CHARREAD_MAIN=/tmp/qt-charread-main.db \
//!   QT_FIXTURE_CHARREAD_MOUNT=/tmp/qt-charread-mount.db \
//!     $N/npx tsx ~/source/quilltap-v5/harness/oracle/cases/characters-read.ts > /tmp/oracle-charread.ndjson
//! Run:
//!   QT_ORACLE_CHARREAD=/tmp/oracle-charread.ndjson \
//!   QT_FIXTURE_CHARREAD_MAIN=/tmp/qt-charread-main.db \
//!   QT_FIXTURE_CHARREAD_MOUNT=/tmp/qt-charread-mount.db \
//!     cargo test -p quilltap-harness --test characters_read_equivalence

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use quilltap_core::db::characters_read as cr;
use quilltap_core::db::Writer;
use serde::Deserialize;
use serde_json::Value;

#[derive(Deserialize)]
struct Spec {
    #[serde(rename = "testPepperBase64")]
    test_pepper_base64: String,
    queries: Vec<Query>,
}

#[derive(Deserialize)]
struct Query {
    kind: String,
    #[serde(default, rename = "targetName")]
    target_name: Option<String>,
    #[serde(default, rename = "targetNames")]
    target_names: Option<Vec<String>>,
    #[serde(default, rename = "userId")]
    user_id: Option<String>,
    #[serde(default, rename = "imageId")]
    image_id: Option<String>,
    #[serde(default, rename = "tagId")]
    tag_id: Option<String>,
}

#[derive(Deserialize)]
struct OracleQuery {
    kind: String,
    result: Vec<Value>,
}
#[derive(Deserialize)]
struct Oracle {
    queries: Vec<OracleQuery>,
}

fn spec_path() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../harness/oracle/fixtures/characters-read-tier2.json")
}

/// Placeholder the read-minted `physicalDescription.createdAt`/`updatedAt` (the read
/// overlay's only nondeterminism) and sort the list by character id, so two
/// independent reads of the same fixture compare exactly.
fn normalize(list: &mut [Value]) {
    for c in list.iter_mut() {
        if let Some(phys) = c
            .get_mut("physicalDescription")
            .and_then(Value::as_object_mut)
        {
            for k in ["createdAt", "updatedAt"] {
                if phys.contains_key(k) {
                    phys.insert(k.to_string(), Value::String("<ts>".to_string()));
                }
            }
        }
    }
    list.sort_by(|a, b| {
        let ka = a.get("id").and_then(Value::as_str).unwrap_or("");
        let kb = b.get("id").and_then(Value::as_str).unwrap_or("");
        ka.cmp(kb)
    });
}

fn run_query(
    main: &Writer,
    mount: &Writer,
    q: &Query,
    id_by_name: &HashMap<String, String>,
) -> Vec<Value> {
    let m = main.connection();
    let mo = mount.connection();
    let id_for = |name: &str| -> String {
        id_by_name
            .get(name)
            .unwrap_or_else(|| panic!("no character named {name}"))
            .clone()
    };
    let opt_to_vec = |v: Option<Value>| v.map(|x| vec![x]).unwrap_or_default();

    match q.kind.as_str() {
        "findByIdRaw" => opt_to_vec(
            cr::find_by_id_raw(m, &id_for(q.target_name.as_deref().unwrap())).expect("findByIdRaw"),
        ),
        "findById" => opt_to_vec(
            cr::find_by_id(m, mo, &id_for(q.target_name.as_deref().unwrap())).expect("findById"),
        ),
        "findAll" => cr::find_all(m, mo).expect("findAll"),
        "findByUserId" => {
            cr::find_by_user_id(m, mo, q.user_id.as_deref().unwrap()).expect("findByUserId")
        }
        "findUserControlled" => cr::find_user_controlled(m, mo, q.user_id.as_deref().unwrap())
            .expect("findUserControlled"),
        "findLLMControlled" => cr::find_llm_controlled(m, mo, q.user_id.as_deref().unwrap())
            .expect("findLLMControlled"),
        "findByIds" => {
            let ids: Vec<String> = q
                .target_names
                .as_ref()
                .unwrap()
                .iter()
                .map(|n| id_for(n))
                .collect();
            cr::find_by_ids(m, mo, &ids).expect("findByIds")
        }
        "findByDefaultImageId" => {
            cr::find_by_default_image_id(m, mo, q.image_id.as_deref().unwrap())
                .expect("findByDefaultImageId")
        }
        "findByAvatarOverrideImageId" => {
            cr::find_by_avatar_override_image_id(m, mo, q.image_id.as_deref().unwrap())
                .expect("findByAvatarOverrideImageId")
        }
        "findByTag" => cr::find_by_tag(m, mo, q.tag_id.as_deref().unwrap()).expect("findByTag"),
        other => panic!("unknown query kind: {other}"),
    }
}

#[test]
fn characters_read_matches_oracle() {
    let oracle_path = match std::env::var("QT_ORACLE_CHARREAD") {
        Ok(p) => p,
        Err(_) => {
            eprintln!("SKIP: set QT_ORACLE_CHARREAD to the oracle NDJSON (see header).");
            return;
        }
    };
    let main_fixture = match std::env::var("QT_FIXTURE_CHARREAD_MAIN") {
        Ok(p) => p,
        Err(_) => {
            eprintln!("SKIP: set QT_FIXTURE_CHARREAD_MAIN to the main fixture .db (header).");
            return;
        }
    };
    let mount_fixture = match std::env::var("QT_FIXTURE_CHARREAD_MOUNT") {
        Ok(p) => p,
        Err(_) => {
            eprintln!("SKIP: set QT_FIXTURE_CHARREAD_MOUNT to the mount fixture .db (header).");
            return;
        }
    };

    let spec: Spec = serde_json::from_str(
        &std::fs::read_to_string(spec_path()).unwrap_or_else(|e| panic!("read spec: {e}")),
    )
    .expect("parse spec");

    let oracle: Oracle = serde_json::from_str(
        std::fs::read_to_string(&oracle_path)
            .unwrap_or_else(|e| panic!("read oracle: {e}"))
            .trim(),
    )
    .expect("parse oracle dump");

    // Fresh copies so the shared baked fixtures stay pristine.
    let pid = std::process::id();
    let main_work = std::env::temp_dir().join(format!("qt-charread-main-rust-{pid}.db"));
    let mount_work = std::env::temp_dir().join(format!("qt-charread-mount-rust-{pid}.db"));
    let _ = std::fs::remove_file(&main_work);
    let _ = std::fs::remove_file(&mount_work);
    std::fs::copy(&main_fixture, &main_work).unwrap_or_else(|e| panic!("copy main: {e}"));
    std::fs::copy(&mount_fixture, &mount_work).unwrap_or_else(|e| panic!("copy mount: {e}"));

    let main = Writer::open_writable(&main_work, &spec.test_pepper_base64)
        .unwrap_or_else(|e| panic!("open main: {e}"));
    let mount = Writer::open_writable(&mount_work, &spec.test_pepper_base64)
        .unwrap_or_else(|e| panic!("open mount: {e}"));

    // name -> minted id map (both sides read the same fixture).
    let mut id_by_name: HashMap<String, String> = HashMap::new();
    {
        let conn = main.connection();
        let mut stmt = conn
            .prepare("SELECT id, name FROM characters")
            .expect("prepare name map");
        let rows = stmt
            .query_map([], |row| {
                Ok((row.get::<_, String>(1)?, row.get::<_, String>(0)?))
            })
            .expect("query name map");
        for r in rows {
            let (name, id) = r.expect("name map row");
            id_by_name.insert(name, id);
        }
    }

    assert_eq!(
        spec.queries.len(),
        oracle.queries.len(),
        "query count: spec vs oracle"
    );

    for (i, q) in spec.queries.iter().enumerate() {
        let mut got = run_query(&main, &mount, q, &id_by_name);
        let oq = &oracle.queries[i];
        assert_eq!(oq.kind, q.kind, "query {i}: kind mismatch");
        let mut want = oq.result.clone();
        normalize(&mut got);
        normalize(&mut want);
        assert_eq!(
            got,
            want,
            "query {i} ({}): result diverged\n  rust:   {}\n  oracle: {}",
            q.kind,
            serde_json::to_string(&got).unwrap(),
            serde_json::to_string(&want).unwrap()
        );
    }

    let _ = std::fs::remove_file(&main_work);
    let _ = std::fs::remove_file(&mount_work);

    eprintln!(
        "OK: characters read matched oracle ({} queries).",
        spec.queries.len()
    );
}
