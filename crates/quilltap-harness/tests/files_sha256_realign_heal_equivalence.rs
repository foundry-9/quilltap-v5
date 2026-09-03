//! P4.D152 — `db::files_sha256_realign_heal` vs v4's REAL
//! `realign-file-entry-sha256-v1` migration (`0b0617fee`, bug 117).
//!
//! Both sides build the same two partitions from
//! `harness/oracle/fixtures/files-sha256-realign-heal.json` — a migration-vintage
//! `files` table in MAIN and a `doc_mount_blobs` table in MOUNT — run the pass,
//! run it a second time, and dump the whole `files` table, the four counts, the
//! summary sentence, the warn lines and the migration ledger.
//!
//! ## The comparands, and the two that are not equalities
//!
//! * `updatedAt` is `new Date().toISOString()` on v4 and the caller's `now_iso`
//!   on v5, so it is NORMALIZED to the token `<now>` whenever it differs from the
//!   fixture's `createdAt`. That keeps the actual discriminator — v4 rewrites
//!   `updatedAt` **only on rows it changed** — and drops the wall clock.
//! * ⚠ **The ledger is a RECORDED DIVERGENCE, asserted in both directions.**
//!   v4's `shouldRun()` for this migration tests PRESENCE of mount-blob rows, not
//!   drift, so its runner writes a `migrations_state` row even when the pass
//!   realigned zero. v5 writes a row only on a pass that realigned ≥ 1 (work
//!   order P4.D152 §D — a stamp on a no-op pass would tell a later v4 boot to
//!   skip a migration that never ran, while a pre-4.9.0 v4 sharing the instance
//!   is still writing drifted rows). The `agreeing-row-untouched` scenario is
//!   where the two differ, and it is asserted as a divergence rather than
//!   masked; when v4 changes its own guard this trips by design.
//!
//! v5 has no `shouldRun()` seam — the pass folds the gate into the walk. The
//! harness computes v4's exact predicate (`COUNT(*) FROM files WHERE storageKey
//! LIKE 'mount-blob:%' > 0`) itself and compares it, then mirrors the runner:
//! only a true predicate emits a `result`/`warns`/`secondRun`.
//!
//! Generate the oracle (Node 24, from a PINNED v4 worktree — see the .ts header):
//!   … QT_ORACLE_OUT=/tmp/oracle-files-sha256-realign-heal.ndjson \
//!     npx jest -- "files-sha256-realign-heal\.test\.ts$"
//! Run:
//!   QT_ORACLE_FILES_SHA256_REALIGN=/tmp/oracle-files-sha256-realign-heal.ndjson \
//!     cargo test -p quilltap-harness --test files_sha256_realign_heal_equivalence

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use quilltap_core::db::files_sha256_realign_heal::{realign_file_entry_sha256, RealignOutcome};
use rusqlite::Connection;
use serde::Deserialize;
use serde_json::{json, Value};
use tracing_subscriber::layer::SubscriberExt;

const MIGRATION_ID: &str = "realign-file-entry-sha256-v1";

// ── the capturing tracing layer (thread-scoped: the standing note) ───────────

#[derive(Clone, Debug)]
struct CapturedEvent {
    message: String,
    fields: BTreeMap<String, String>,
}

struct CaptureLayer(Arc<Mutex<Vec<CapturedEvent>>>);

#[derive(Default)]
struct FieldVisitor {
    message: String,
    fields: BTreeMap<String, String>,
}
impl tracing::field::Visit for FieldVisitor {
    fn record_str(&mut self, field: &tracing::field::Field, value: &str) {
        self.fields
            .insert(field.name().to_string(), value.to_string());
    }
    fn record_debug(&mut self, field: &tracing::field::Field, value: &dyn std::fmt::Debug) {
        if field.name() == "message" {
            // A static message renders through `record_debug` on `format_args!`.
            self.message = format!("{value:?}").trim_matches('"').to_string();
        } else {
            self.fields
                .insert(field.name().to_string(), format!("{value:?}"));
        }
    }
}
impl<S: tracing::Subscriber> tracing_subscriber::Layer<S> for CaptureLayer {
    fn on_event(&self, event: &tracing::Event<'_>, _c: tracing_subscriber::layer::Context<'_, S>) {
        if *event.metadata().level() != tracing::Level::WARN {
            return;
        }
        let mut v = FieldVisitor::default();
        event.record(&mut v);
        self.0.lock().unwrap().push(CapturedEvent {
            message: v.message,
            fields: v.fields,
        });
    }
}

// ── the shared spec ──────────────────────────────────────────────────────────

#[derive(Deserialize)]
struct SpecFile {
    id: String,
    sha256: String,
    #[serde(rename = "storageKey")]
    storage_key: Option<String>,
}
#[derive(Deserialize)]
struct SpecBlob {
    id: String,
    sha256: String,
}
#[derive(Deserialize)]
struct Generate {
    count: usize,
}
#[derive(Deserialize)]
struct Scenario {
    name: String,
    files: Vec<SpecFile>,
    blobs: Vec<SpecBlob>,
    #[serde(default)]
    generate: Option<Generate>,
    /// Plant the ledger row first — the cross-app "v4 already ran it" shape.
    #[serde(rename = "preCompleted", default)]
    pre_completed: bool,
}
#[derive(Deserialize)]
struct Spec {
    #[serde(rename = "userId")]
    user_id: String,
    #[serde(rename = "createdAt")]
    created_at: String,
    #[serde(rename = "nowIso")]
    now_iso: String,
    scenarios: Vec<Scenario>,
}

fn spec_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../harness/oracle/fixtures/files-sha256-realign-heal.json")
}

/// One `files` row as the builder wants it: `(id, sha256, storageKey)`.
type FileRowSeed = (String, String, Option<String>);
/// One `doc_mount_blobs` row: `(id, sha256)`.
type BlobRowSeed = (String, String);

/// The generated past-the-first-batch rows — the .ts twin, character for
/// character (zero-padded ids so `ORDER BY id` is the same total order).
fn expand(s: &Scenario) -> (Vec<FileRowSeed>, Vec<BlobRowSeed>) {
    let mut files: Vec<FileRowSeed> = s
        .files
        .iter()
        .map(|f| (f.id.clone(), f.sha256.clone(), f.storage_key.clone()))
        .collect();
    let mut blobs: Vec<BlobRowSeed> = s
        .blobs
        .iter()
        .map(|b| (b.id.clone(), b.sha256.clone()))
        .collect();
    if let Some(g) = &s.generate {
        for i in 0..g.count {
            let n = format!("{i:04}");
            blobs.push((format!("gb-{n}"), format!("stored-{n}")));
            files.push((
                format!("gf-{n}"),
                format!("input-{n}"),
                Some(format!("mount-blob:mp-1:gb-{n}")),
            ));
        }
    }
    (files, blobs)
}

/// v4 `migrations/state.ts`'s own tables + the completed row it writes — the .ts
/// twin.
fn plant_ledger_row(db: &Connection, spec: &Spec) {
    db.execute_batch(
        "CREATE TABLE IF NOT EXISTS \"migrations_state\" (\n           \"id\" TEXT PRIMARY KEY,\n           \"completedAt\" TEXT NOT NULL,\n           \"quilltapVersion\" TEXT NOT NULL,\n           \"itemsAffected\" INTEGER NOT NULL DEFAULT 0,\n           \"message\" TEXT\n         );\n         CREATE TABLE IF NOT EXISTS \"migrations_metadata\" (\n           \"key\" TEXT PRIMARY KEY,\n           \"value\" TEXT NOT NULL\n         );",
    )
    .expect("ledger ddl");
    db.execute(
        "INSERT INTO migrations_state (id, completedAt, quilltapVersion, itemsAffected, message) VALUES (?1, ?2, ?3, ?4, ?5)",
        rusqlite::params![
            MIGRATION_ID,
            spec.now_iso,
            "4.9.0",
            0_i64,
            "planted by the other app"
        ],
    )
    .expect("ledger row");
}

fn build_main(files: &[FileRowSeed], spec: &Spec) -> Connection {
    let db = Connection::open_in_memory().expect("open main");
    db.execute_batch(
        "CREATE TABLE \"files\" (\n           \"id\" TEXT PRIMARY KEY,\n           \"userId\" TEXT NOT NULL,\n           \"sha256\" TEXT NOT NULL,\n           \"storageKey\" TEXT,\n           \"createdAt\" TEXT NOT NULL,\n           \"updatedAt\" TEXT NOT NULL\n         );",
    )
    .expect("main ddl");
    for (id, sha, key) in files {
        db.execute(
            "INSERT INTO files (id, userId, sha256, storageKey, createdAt, updatedAt) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            rusqlite::params![id, spec.user_id, sha, key, spec.created_at, spec.created_at],
        )
        .expect("file row");
    }
    db
}

fn build_mount(blobs: &[BlobRowSeed]) -> Connection {
    let db = Connection::open_in_memory().expect("open mount");
    db.execute_batch(
        "CREATE TABLE \"doc_mount_blobs\" (\n           \"id\" TEXT PRIMARY KEY,\n           \"sha256\" TEXT NOT NULL\n         );",
    )
    .expect("mount ddl");
    for (id, sha) in blobs {
        db.execute(
            "INSERT INTO doc_mount_blobs (id, sha256) VALUES (?1, ?2)",
            rusqlite::params![id, sha],
        )
        .expect("blob row");
    }
    db
}

/// v4's own `shouldRun()` predicate, spelled here because v5 folds the gate into
/// the walk (the module header).
fn v4_should_run(main: &Connection) -> bool {
    main.query_row(
        "SELECT COUNT(*) FROM \"files\" WHERE storageKey LIKE 'mount-blob:%'",
        [],
        |r| r.get::<_, i64>(0),
    )
    .expect("count")
        > 0
}

fn dump_files(main: &Connection, created_at: &str) -> Value {
    let mut stmt = main
        .prepare("SELECT id, sha256, storageKey, updatedAt FROM files ORDER BY id")
        .expect("prepare dump");
    let rows = stmt
        .query_map([], |r| {
            let id: String = r.get(0)?;
            let sha: String = r.get(1)?;
            let key: Option<String> = r.get(2)?;
            let updated: String = r.get(3)?;
            Ok(json!({
                "id": id,
                "sha256": sha,
                "storageKey": key,
                // The wall clock is normalized; "did this row's updatedAt move"
                // is the whole discriminator.
                "updatedAt": if updated == created_at { updated } else { "<now>".to_string() },
            }))
        })
        .expect("dump")
        .collect::<Result<Vec<_>, _>>()
        .expect("dump rows");
    Value::Array(rows)
}

fn normalize_oracle_files(files: &Value, created_at: &str) -> Value {
    Value::Array(
        files
            .as_array()
            .expect("files array")
            .iter()
            .map(|row| {
                let mut m = row.as_object().expect("file row").clone();
                let updated = m
                    .get("updatedAt")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_string();
                m.insert(
                    "updatedAt".to_string(),
                    Value::String(if updated == created_at {
                        updated
                    } else {
                        "<now>".to_string()
                    }),
                );
                Value::Object(m)
            })
            .collect(),
    )
}

/// v4's warn shape: `{message, fileId, blobId, storageKey}` with the absent ids
/// rendered as JSON null.
fn shape_warns(events: &[CapturedEvent]) -> Value {
    Value::Array(
        events
            .iter()
            .map(|e| {
                let g = |k: &str| match e.fields.get(k) {
                    Some(v) => Value::String(v.clone()),
                    None => Value::Null,
                };
                json!({
                    "message": e.message,
                    "fileId": g("file_id"),
                    "blobId": g("blob_id"),
                    "storageKey": g("storage_key"),
                })
            })
            .collect(),
    )
}

fn ledger(main: &Connection) -> Value {
    let exists = main
        .prepare("SELECT 1 FROM sqlite_master WHERE type='table' AND name='migrations_state'")
        .and_then(|mut s| s.exists([]))
        .unwrap_or(false);
    if !exists {
        return json!([]);
    }
    let mut stmt = main
        .prepare("SELECT id, itemsAffected, message FROM migrations_state ORDER BY id")
        .expect("prepare ledger");
    let rows = stmt
        .query_map([], |r| {
            Ok(json!({
                "id": r.get::<_, String>(0)?,
                "itemsAffected": r.get::<_, i64>(1)?,
                "message": r.get::<_, Option<String>>(2)?,
            }))
        })
        .expect("ledger")
        .collect::<Result<Vec<_>, _>>()
        .expect("ledger rows");
    Value::Array(rows)
}

struct PassResult {
    outcome: RealignOutcome,
    warns: Value,
}

fn run_pass(main: &Connection, mount: &Connection, now_iso: &str) -> PassResult {
    let logs = Arc::new(Mutex::new(Vec::<CapturedEvent>::new()));
    let subscriber = tracing_subscriber::registry().with(CaptureLayer(logs.clone()));
    let outcome = {
        let _guard = tracing::subscriber::set_default(subscriber);
        realign_file_entry_sha256(main, Some(mount), now_iso).expect("heal")
    };
    let events = logs.lock().unwrap().clone();
    PassResult {
        outcome,
        warns: shape_warns(&events),
    }
}

fn items_affected(o: &RealignOutcome) -> i64 {
    match o {
        RealignOutcome::Ran { realigned, .. } => *realigned as i64,
        _ => 0,
    }
}

#[test]
fn files_sha256_realign_heal_equivalence() {
    let Ok(path) = std::env::var("QT_ORACLE_FILES_SHA256_REALIGN") else {
        eprintln!("SKIP: QT_ORACLE_FILES_SHA256_REALIGN unset");
        return;
    };
    let raw = std::fs::read_to_string(&path).expect("read oracle");
    let oracle: Vec<Value> = raw
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| serde_json::from_str::<Value>(l).expect("oracle line"))
        .collect();
    let spec: Spec =
        serde_json::from_str(&std::fs::read_to_string(spec_path()).expect("read spec"))
            .expect("parse spec");

    assert_eq!(
        oracle.len(),
        spec.scenarios.len(),
        "oracle rows and spec scenarios must line up"
    );

    let mut failed: Vec<String> = Vec::new();
    // Floors: the corpus must keep exercising every arm the fix touches.
    let mut realigning = 0usize;
    let mut orphan_warns = 0usize;
    let mut malformed_warns = 0usize;
    let mut divergent_ledger = 0usize;
    let mut honoured_ledger = 0usize;

    for (i, scenario) in spec.scenarios.iter().enumerate() {
        let o = &oracle[i];
        assert_eq!(
            o["scenario"].as_str().unwrap(),
            scenario.name,
            "scenario order"
        );
        let (files, blobs) = expand(scenario);
        let main = build_main(&files, &spec);
        let mount = build_mount(&blobs);

        if scenario.pre_completed {
            plant_ledger_row(&main, &spec);
        }
        // v4's runner checks the ledger FIRST and never reaches `shouldRun()`.
        let should_run = !scenario.pre_completed && v4_should_run(&main);
        if should_run != o["shouldRun"].as_bool().unwrap() {
            failed.push(format!(
                "{}: shouldRun v5={should_run} v4={}",
                scenario.name, o["shouldRun"]
            ));
        }

        let mut result = Value::Null;
        let mut warns = json!([]);
        let mut second_run = Value::Null;
        let mut files_after_second = Value::Null;
        let mut summary = Value::Null;

        // Mirror v4's runner: a false `shouldRun()` means the pass never runs.
        // v5's heal is a no-op there anyway (its `files` dump proves it), and the
        // dump below is taken either way.
        let pass = run_pass(&main, &mount, &spec.now_iso);
        if should_run {
            let affected = items_affected(&pass.outcome);
            if affected > 0 {
                realigning += 1;
            }
            let message = pass.outcome.message().expect("a scanned pass renders one");
            result = json!({
                "id": MIGRATION_ID,
                "success": true,
                "itemsAffected": affected,
                "message": message,
            });
            summary = json!({ "message": message });
            warns = pass.warns.clone();

            let second = run_pass(&main, &mount, &spec.now_iso);
            // v5 honours the ledger row, so a realigning first pass makes the
            // second `AlreadyCompleted` — the ONE place the two runners differ in
            // sequence, and it renders v4's own `itemsAffected: 0` answer because
            // there is by then nothing left to realign either way.
            second_run = json!({
                "itemsAffected": items_affected(&second.outcome),
                "message": second.outcome.message().unwrap_or_else(|| message.clone()),
            });
            files_after_second = dump_files(&main, &spec.created_at);
        } else if scenario.pre_completed {
            // The ledger row from the other app is what stops the pass — and it
            // MUST, because the row underneath it is genuinely drifted. A heal
            // that ignored the ledger would realign it and the `files` compare
            // below would redden.
            assert_eq!(
                pass.outcome,
                RealignOutcome::AlreadyCompleted,
                "{}: an existing ledger row must stop the pass",
                scenario.name
            );
            honoured_ledger += 1;
        } else {
            assert!(
                matches!(pass.outcome, RealignOutcome::NoDrift { scanned: 0, .. }),
                "{}: v4 skips, so v5 must scan nothing",
                scenario.name
            );
        }

        for w in warns.as_array().unwrap_or(&Vec::new()) {
            match w["message"].as_str().unwrap_or_default() {
                "Mount blob missing for FileEntry; sha256 left untouched" => orphan_warns += 1,
                "Malformed mount-blob storage key; sha256 left untouched" => malformed_warns += 1,
                _ => {}
            }
        }

        let v5_files = dump_files(&main, &spec.created_at);
        let v4_files = normalize_oracle_files(&o["files"], &spec.created_at);
        cmp(&mut failed, &scenario.name, "files", &v5_files, &v4_files);
        cmp(&mut failed, &scenario.name, "result", &result, &o["result"]);
        cmp(&mut failed, &scenario.name, "warns", &warns, &o["warns"]);
        if !o["summary"].is_null() {
            cmp(
                &mut failed,
                &scenario.name,
                "summary.message",
                &summary["message"],
                &o["summary"]["message"],
            );
        }
        cmp(
            &mut failed,
            &scenario.name,
            "secondRun.itemsAffected",
            &second_run["itemsAffected"],
            &o["secondRun"]["itemsAffected"],
        );
        if !files_after_second.is_null() {
            cmp(
                &mut failed,
                &scenario.name,
                "filesAfterSecondRun",
                &files_after_second,
                &normalize_oracle_files(&o["filesAfterSecondRun"], &spec.created_at),
            );
        }

        // ── the ledger, both directions ──
        let v5_ledger = ledger(&main);
        let v4_ledger = o["ledger"].clone();
        let v5_rows = v5_ledger.as_array().map(Vec::len).unwrap_or(0);
        let v4_rows = v4_ledger.as_array().map(Vec::len).unwrap_or(0);
        let realigned_any = items_affected(&pass.outcome) > 0;
        if should_run && !realigned_any {
            // The RECORDED DIVERGENCE: v4 stamps, v5 does not.
            divergent_ledger += 1;
            if !(v4_rows == 1 && v5_rows == 0) {
                failed.push(format!(
                    "{}: expected the recorded ledger divergence (v4 one row, v5 none), got v4={v4_rows} v5={v5_rows}",
                    scenario.name
                ));
            }
        } else if should_run {
            cmp(
                &mut failed,
                &scenario.name,
                "ledger",
                &v5_ledger,
                &v4_ledger,
            );
        } else {
            // A skipped migration writes nothing on either side: both ledgers are
            // whatever was planted (nothing, or the other app's row).
            cmp(
                &mut failed,
                &scenario.name,
                "ledger",
                &v5_ledger,
                &v4_ledger,
            );
        }
    }

    assert!(
        realigning >= 4,
        "coverage floor: at least four scenarios must realign a row (got {realigning})"
    );
    assert!(
        orphan_warns >= 1 && malformed_warns >= 2,
        "coverage floor: the orphan and malformed-key warn arms must both fire \
         (orphan={orphan_warns}, malformed={malformed_warns})"
    );
    assert_eq!(
        divergent_ledger, 1,
        "the recorded ledger divergence must be exercised exactly once"
    );
    assert_eq!(
        honoured_ledger, 1,
        "the cross-app ledger-honour arm must be exercised exactly once"
    );

    assert!(
        failed.is_empty(),
        "{} mismatch(es):\n{}",
        failed.len(),
        failed.join("\n")
    );
}

fn cmp(failed: &mut Vec<String>, scenario: &str, field: &str, v5: &Value, v4: &Value) {
    if v5 != v4 {
        let a = serde_json::to_string(v5).unwrap_or_default();
        let b = serde_json::to_string(v4).unwrap_or_default();
        failed.push(format!(
            "{scenario}/{field}:\n  v5: {}\n  v4: {}",
            &a[..a.len().min(1400)],
            &b[..b.len().min(1400)]
        ));
    }
}
