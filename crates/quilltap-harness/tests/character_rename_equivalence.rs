//! P4.9K1 tier-2 differential: `api::generators_detail::character_rename` +
//! `character_refresh_archive` (v4 `characters/[id]/handlers/post.ts` `rename`
//! / `refresh-archive`, over `lib/services/character-rename.service.ts`) vs
//! v4's REAL route. Both sides run every corpus case on a FRESH copy of the
//! committed `characters-{main,mount}.db` pair, apply the case's seeds through
//! the ported repository twins, then diff the response (status + body, Zod
//! `details` included) AND a post-state census: the overlaid character, the
//! character's memories, every chat, every chat message, every background job.
//!
//! The TABLES are the discriminator: a dry run and an execute answer the same
//! preview body, and only the rows say whether anything was written — which is
//! how "dry-run writes nothing" and "no-match execute writes nothing" are
//! proven rather than asserted.
//!
//! ## Normalized (both sides, identically)
//!
//! Every `createdAt` / `updatedAt` / `lastMessageAt` / `lastReinforcedAt` /
//! `lastAccessedAt` / `scheduledAt` / `startedAt` / `completedAt` string (the
//! execute leg re-stamps sub-object `updatedAt`s and the render enqueue mints
//! its own), `background_jobs.id` (minted per side, referenced by nothing), and
//! the broken-vault refusal's error TEXT (`broken_vault_echo`: v4's
//! `CharacterVaultUnavailableError` sentence vs v5's `DbError` Display — the
//! `ok:false` arm is what is compared, and the route answer above it is the
//! byte-compared contextful 503).
//!
//! Coverage is asserted by SHAPE: the oracle's case names must equal the
//! corpus's, both ways.
//!
//! Regenerate the oracle (Node 24, from the v4 checkout — see the .ts header):
//!   N=~/.nvm/versions/node/v24.13.1/bin
//!   V5W=${V5W:-$HOME/source/quilltap-v5}
//!   TMPO=/tmp/qt-character-rename-oracle
//!   rm -rf "$TMPO"; mkdir -p "$TMPO/cases" "$TMPO/fixtures"
//!   cp "$V5W/harness/oracle/cases/character-rename.test.ts" "$TMPO/cases/"
//!   cp "$V5W/harness/oracle/fixtures/characters.json" "$TMPO/fixtures/"
//!   cp "$V5W/harness/oracle/fixtures/character-rename-tier2.json" "$TMPO/fixtures/"
//!   cd ~/source/quilltap-server
//!   QT_FIXTURE_CHARACTERS_MAIN=$V5W/crates/quilltap-web/tests/fixtures/characters-main.db \
//!   QT_FIXTURE_CHARACTERS_MOUNT=$V5W/crates/quilltap-web/tests/fixtures/characters-mount.db \
//!   QT_ORACLE_OUT=/tmp/oracle-character-rename.ndjson \
//!     $N/npx jest --silent --watchman=false --testTimeout=300000 \
//!       --roots "$PWD" --roots "$TMPO/cases" -- character-rename
//! Run:
//!   QT_ORACLE_CHARACTER_RENAME=/tmp/oracle-character-rename.ndjson \
//!     cargo test -p quilltap-harness --test character_rename_equivalence -- --nocapture
//!
//! While v4 HEAD is past the baseline the regen needs a worktree pinned at
//! the baseline — the sweep driver's job (`recipe_sweep.py --run
//! character_rename_equivalence --v4 <pin>`), never a path baked in here.

use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

use quilltap_core::api::generators_detail::{character_refresh_archive, character_rename};
use quilltap_core::api::types::{ErrorKind, Request, Response};
use quilltap_core::db::chats_messages::{ChatEventInput, ChatMessagesRepository};
use quilltap_core::db::memories::{MemUpdate, MemoriesRepository};
use quilltap_core::db::runtime::{Db, DbPaths};
use serde::Deserialize;
use serde_json::{json, Value};

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Spec {
    test_pepper_base64: String,
    user_id: String,
}

#[derive(Deserialize)]
#[serde(tag = "kind")]
enum Seed {
    #[serde(rename = "messages", rename_all = "camelCase")]
    Messages { chat_id: String, events: Vec<Value> },
    #[serde(rename = "memoryContent", rename_all = "camelCase")]
    MemoryContent {
        memory_id: String,
        content: String,
        #[serde(default)]
        keywords: Option<Vec<String>>,
    },
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Case {
    name: String,
    character_id: String,
    action: String,
    body: Value,
    #[serde(default)]
    seeds: Vec<Seed>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Corpus {
    seed_timestamp: String,
    cases: Vec<Case>,
}

/// One oracle line — the same shape the Rust side builds.
#[derive(Deserialize)]
struct OracleRow {
    name: String,
    status: i64,
    body: Value,
    character: Value,
    memories: Value,
    chats: Value,
    messages: Value,
    jobs: Value,
}

fn oracle_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../harness/oracle/fixtures")
}
fn fixtures_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../quilltap-web/tests/fixtures")
}

const MEMORIES_SQL: &str = "SELECT id, characterId, aboutCharacterId, content, summary, keywords, updatedAt FROM memories WHERE characterId = ?1 ORDER BY id";
const CHATS_SQL: &str =
    "SELECT id, title, messageCount, lastMessageAt, updatedAt FROM chats ORDER BY id";
const MESSAGES_SQL: &str = "SELECT id, chatId, type, role, content, systemSender, participantId, createdAt FROM chat_messages ORDER BY chatId, createdAt, id";
const JOBS_SQL: &str = "SELECT id, userId, type, status, payload, priority, attempts, maxAttempts FROM background_jobs ORDER BY rowid";

/// A raw SQL census as JSON rows (the oracle's `rawQuery` shape: column names
/// as keys, SQLite scalars as JSON scalars).
fn rows_to_json(conn: &rusqlite::Connection, sql: &str, params: &[&dyn rusqlite::ToSql]) -> Value {
    let mut stmt = conn.prepare(sql).unwrap();
    let names: Vec<String> = stmt.column_names().iter().map(|s| s.to_string()).collect();
    let rows = stmt
        .query_map(params, |r| {
            let mut o = serde_json::Map::new();
            for (i, name) in names.iter().enumerate() {
                let v: rusqlite::types::Value = r.get(i)?;
                let j = match v {
                    rusqlite::types::Value::Null => Value::Null,
                    rusqlite::types::Value::Integer(n) => json!(n),
                    rusqlite::types::Value::Real(f) => json!(f),
                    rusqlite::types::Value::Text(s) => Value::String(s),
                    rusqlite::types::Value::Blob(b) => Value::String(format!("<blob {}>", b.len())),
                };
                o.insert(name.clone(), j);
            }
            Ok(Value::Object(o))
        })
        .unwrap();
    Value::Array(rows.map(|r| r.unwrap()).collect())
}

const TS_KEYS: &[&str] = &[
    "createdAt",
    "updatedAt",
    "lastMessageAt",
    "lastReinforcedAt",
    "lastAccessedAt",
    "scheduledAt",
    "startedAt",
    "completedAt",
    "descriptionUpdatedAt",
];

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

fn scrub(v: &mut Value) {
    match v {
        Value::Object(o) => {
            for (k, val) in o.iter_mut() {
                if TS_KEYS.contains(&k.as_str()) && val.is_string() {
                    *val = Value::String("<ts>".into());
                } else {
                    scrub(val);
                }
            }
        }
        Value::Array(a) => a.iter_mut().for_each(scrub),
        _ => {}
    }
}

fn sorted(v: &Value) -> Value {
    match v {
        Value::Array(a) => Value::Array(a.iter().map(sorted).collect()),
        Value::Object(o) => {
            let mut keys: Vec<&String> = o.keys().collect();
            keys.sort();
            let mut m = serde_json::Map::new();
            for k in keys {
                m.insert(k.clone(), sorted(&o[k]));
            }
            Value::Object(m)
        }
        _ => v.clone(),
    }
}

/// The one per-side normalization: timestamps, minted job ids, the broken-vault
/// error text, then canonical numbers + sorted keys.
fn normalize(mut v: Value) -> String {
    canon_numbers(&mut v);
    scrub(&mut v);
    if let Some(jobs) = v.get_mut("jobs").and_then(Value::as_array_mut) {
        for job in jobs {
            if job.get("id").is_some() {
                job["id"] = Value::String("<jobid>".into());
            }
        }
    }
    if let Some(ch) = v.get_mut("character") {
        if ch.get("ok") == Some(&Value::Bool(false)) {
            ch["error"] = Value::String("<unavailable>".into());
        }
    }
    serde_json::to_string_pretty(&sorted(&v)).unwrap()
}

fn first_diff(got: &str, want: &str) -> String {
    let g: Vec<&str> = got.lines().collect();
    let w: Vec<&str> = want.lines().collect();
    for i in 0..g.len().max(w.len()) {
        let gi = g.get(i).copied().unwrap_or("<none>");
        let wi = w.get(i).copied().unwrap_or("<none>");
        if gi != wi {
            let mut ctx = String::new();
            for j in i.saturating_sub(4)..i {
                ctx.push_str(&format!("   = {}\n", g.get(j).copied().unwrap_or("")));
            }
            ctx.push_str(&format!("  GOT : {gi}\n  WANT: {wi}\n"));
            return ctx;
        }
    }
    "(identical line-by-line)".to_string()
}

fn status_of(kind: &ErrorKind) -> i64 {
    match kind {
        ErrorKind::BadRequest => 400,
        ErrorKind::Unauthorized => 401,
        ErrorKind::Forbidden => 403,
        ErrorKind::NotFound => 404,
        ErrorKind::Conflict => 409,
        ErrorKind::Unprocessable => 422,
        ErrorKind::Locked | ErrorKind::Unavailable => 503,
        ErrorKind::Internal => 500,
    }
}

/// v4's route answer from the dispatch `Response`: a success body verbatim, an
/// error as `{error[, details]}` — or the contextful store-unavailable body.
fn to_status_body(r: Response) -> (i64, Value) {
    match r {
        Response::Character(v) => (200, v),
        Response::Error(e) => {
            let status = status_of(&e.kind);
            if let Some(body) = e.unavailable_wire_body() {
                return (status, body);
            }
            let mut body = json!({ "error": e.message });
            if let Some(d) = e.details {
                body["details"] = *d;
            }
            (status, body)
        }
        other => panic!("unexpected response variant: {other:?}"),
    }
}

fn fresh_pair(tag: &str) -> (Db, PathBuf, String) {
    let spec: Spec = serde_json::from_str(
        &std::fs::read_to_string(oracle_dir().join("characters.json")).unwrap(),
    )
    .unwrap();
    let scratch = std::env::temp_dir().join(format!("qt-rename-{}-{}", tag, std::process::id()));
    let _ = std::fs::remove_dir_all(&scratch);
    std::fs::create_dir_all(&scratch).unwrap();
    let main = scratch.join("main.db");
    let mount = scratch.join("mount.db");
    std::fs::copy(fixtures_dir().join("characters-main.db"), &main).unwrap();
    std::fs::copy(fixtures_dir().join("characters-mount.db"), &mount).unwrap();
    let db = Db::open(
        DbPaths {
            main,
            mount_index: Some(mount),
            llm_logs: None,
        },
        &spec.test_pepper_base64,
    )
    .expect("open fixture pair");
    (db, scratch, spec.user_id)
}

async fn apply_seeds(db: &Db, seeds: &[Seed], seed_ts: &str) {
    for seed in seeds {
        match seed {
            Seed::Messages { chat_id, events } => {
                let chat_id = chat_id.clone();
                let events: Vec<ChatEventInput> = events
                    .iter()
                    .map(|e| serde_json::from_value(e.clone()).expect("seed event decodes"))
                    .collect();
                db.write(move |w| {
                    ChatMessagesRepository::new(w.main().connection())
                        .add_messages(&chat_id, &events)
                })
                .await
                .expect("seed messages");
            }
            Seed::MemoryContent {
                memory_id,
                content,
                keywords,
            } => {
                let id = memory_id.clone();
                let patch = MemUpdate {
                    content: Some(content.clone()),
                    keywords: keywords.clone(),
                    updated_at: Some(seed_ts.to_string()),
                    ..Default::default()
                };
                db.write(move |w| {
                    MemoriesRepository::new(w.main().connection()).update(&id, &patch)
                })
                .await
                .expect("seed memory");
            }
        }
    }
}

/// The post-state census, the oracle's shape.
fn census(db: &Db, character_id: &str) -> (Value, Value, Value, Value, Value) {
    let cid = character_id.to_string();
    let character = db.read_main(|main| {
        db.read_mount_index(|mount| {
            Ok(
                match quilltap_core::db::characters_read::find_by_id(main, mount, &cid) {
                    Ok(c) => json!({ "ok": true, "value": c }),
                    Err(e) => json!({ "ok": false, "error": e.to_string() }),
                },
            )
        })
    });
    let character = character.unwrap();
    let cid2 = character_id.to_string();
    let (memories, chats, messages, jobs) = db
        .read_main(|conn| {
            Ok((
                rows_to_json(conn, MEMORIES_SQL, &[&cid2]),
                rows_to_json(conn, CHATS_SQL, &[]),
                rows_to_json(conn, MESSAGES_SQL, &[]),
                rows_to_json(conn, JOBS_SQL, &[]),
            ))
        })
        .unwrap();
    (character, memories, chats, messages, jobs)
}

/// Decode the case body through the `Request` enum (the edge's exact decode),
/// then run the handler it names.
async fn drive(db: &Db, user_id: &str, case: &Case) -> Response {
    match case.action.as_str() {
        "rename" => {
            let mut wire = json!({ "type": "characterRename", "characterId": case.character_id });
            if let Some(obj) = case.body.as_object() {
                for (k, v) in obj {
                    wire[k] = v.clone();
                }
            }
            let req: Request = serde_json::from_value(wire).expect("decodes through Request");
            match req {
                Request::CharacterRename {
                    character_id,
                    primary_rename,
                    additional_replacements,
                    dry_run,
                } => {
                    let primary = primary_rename.map(|o| o.unwrap_or(Value::Null));
                    let additional = additional_replacements.map(|o| o.unwrap_or(Value::Null));
                    let dry = dry_run.map(|o| o.unwrap_or(Value::Null));
                    character_rename(
                        db,
                        user_id,
                        &character_id,
                        primary.as_ref(),
                        additional.as_ref(),
                        dry.as_ref(),
                    )
                    .await
                }
                other => panic!("decoded the wrong variant: {other:?}"),
            }
        }
        "refresh-archive" => {
            let req: Request = serde_json::from_value(
                json!({ "type": "characterRefreshArchive", "characterId": case.character_id }),
            )
            .unwrap();
            match req {
                Request::CharacterRefreshArchive { character_id } => {
                    character_refresh_archive(db, user_id, &character_id).await
                }
                other => panic!("decoded the wrong variant: {other:?}"),
            }
        }
        other => panic!("unknown corpus action {other}"),
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn character_rename_matches_oracle() {
    let Ok(oracle_path) = std::env::var("QT_ORACLE_CHARACTER_RENAME") else {
        eprintln!("SKIP: set QT_ORACLE_CHARACTER_RENAME (see the test header).");
        return;
    };
    let text = std::fs::read_to_string(&oracle_path).unwrap();
    assert!(
        !text.trim().is_empty(),
        "{oracle_path} is EMPTY — the regen truncated it before failing (ledger §5.1)"
    );
    let corpus: Corpus = serde_json::from_str(
        &std::fs::read_to_string(oracle_dir().join("character-rename-tier2.json")).unwrap(),
    )
    .unwrap();
    let mut oracle: BTreeMap<String, OracleRow> = BTreeMap::new();
    for line in text.lines().filter(|l| !l.trim().is_empty()) {
        let row: OracleRow = serde_json::from_str(line).unwrap();
        oracle.insert(row.name.clone(), row);
    }

    // Coverage by shape, both directions.
    let corpus_names: BTreeSet<&str> = corpus.cases.iter().map(|c| c.name.as_str()).collect();
    let oracle_names: BTreeSet<&str> = oracle.keys().map(String::as_str).collect();
    assert_eq!(
        corpus_names, oracle_names,
        "the oracle's case set must equal the corpus's"
    );

    let mut failed: Vec<String> = Vec::new();
    let mut ran = 0usize;
    for case in &corpus.cases {
        let (db, scratch, user_id) = fresh_pair(&case.name);
        apply_seeds(&db, &case.seeds, &corpus.seed_timestamp).await;
        let (status, body) = to_status_body(drive(&db, &user_id, case).await);
        let (character, memories, chats, messages, jobs) = census(&db, &case.character_id);
        drop(db);
        let _ = std::fs::remove_dir_all(&scratch);

        let got = normalize(json!({
            "name": case.name,
            "status": status,
            "body": body,
            "character": character,
            "memories": memories,
            "chats": chats,
            "messages": messages,
            "jobs": jobs,
        }));
        let o = &oracle[&case.name];
        let want = normalize(json!({
            "name": o.name,
            "status": o.status,
            "body": o.body,
            "character": o.character,
            "memories": o.memories,
            "chats": o.chats,
            "messages": o.messages,
            "jobs": o.jobs,
        }));
        ran += 1;
        if got != want {
            failed.push(format!("{}:\n{}", case.name, first_diff(&got, &want)));
        }
    }
    eprintln!("character_rename_equivalence: {ran} cases driven");
    assert!(
        failed.is_empty(),
        "{} case(s) DIFFER:\n{}",
        failed.len(),
        failed.join("\n")
    );
}
