//! P4.D175 — `db::generated_image_placeholder_heal` vs v4's REAL
//! `clear-generated-image-placeholder-descriptions-v1` migration
//! (`78b381a96`, bug 132).
//!
//! Both sides build the same two partitions from
//! `harness/oracle/fixtures/generated-image-placeholder-heal.json` — a
//! migration-vintage `files` table in MAIN and a `doc_mount_file_links` table
//! in MOUNT — run the pass, run it a second time, and dump both whole tables,
//! the result, the info line's three counts, the warns and the ledger.
//!
//! ## The comparands, and the one that is normalized
//!
//! * `files.updatedAt` is `new Date().toISOString()` on v4 and the caller's
//!   `now_iso` on v5, so it is NORMALIZED to `<now>` whenever it differs from
//!   the fixture's `createdAt`. That keeps the actual discriminator — **only a
//!   row the pass CLEARED has its `updatedAt` moved** — and drops the wall
//!   clock. A surviving row proving it was untouched is half this family's job.
//! * Everything else is a plain equality, **including the ledger**. Unlike the
//!   P4.D152 realign (whose v4 `shouldRun()` tests PRESENCE, so v4 stamps a
//!   zero-affected pass and the two ledgers are pinned as a divergence), THIS
//!   migration's `shouldRun()` counts placeholders on both sides. A pass that
//!   clears nothing means v4 never ran and recorded nothing either.
//!
//! ## ⚠ The convergence tripwire, by design
//!
//! `outfit-preview-survives-the-narrow-predicate` asserts that a THIRD v4
//! writer's label (`<Name> — outfit preview`,
//! `app/api/v1/wardrobe/preview-avatar/route.ts:160,:185`, v5's
//! `api/wardrobe.rs:1112`) SURVIVES on both sides. v4's own fix does not touch
//! that writer and its predicate does not match the label — measured, not
//! assumed (`git show --stat 78b381a96 -- <that file>` is empty). This port
//! reproduces v4 exactly and files the gap upstream rather than widening the
//! predicate unilaterally, so the arm goes red the day v4 widens its own —
//! which is when this port should follow.
//!
//! v5 has no `shouldRun()` seam — the pass folds the gate into the walk — so
//! the harness computes v4's exact two-stage predicate itself (the files count,
//! short-circuiting before the mount is touched) and compares it, then mirrors
//! the runner: only a true predicate emits a `result`/`warns`/`info`.
//!
//! Generate the oracle (Node 24, from a PINNED v4 worktree — see the .ts
//! header for the /tmp-mirror recipe jest's `.claude/` ignore forces):
//!   … QT_ORACLE_OUT=/tmp/oracle-generated-image-placeholder-heal.ndjson \
//!     npx jest -- "generated-image-placeholder-heal\.test\.ts$"
//! Run:
//!   QT_ORACLE_PLACEHOLDER_HEAL=/tmp/oracle-generated-image-placeholder-heal.ndjson \
//!     cargo test -p quilltap-harness --test generated_image_placeholder_heal_equivalence

use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use quilltap_core::db::generated_image_placeholder_heal::{
    clear_generated_image_placeholder_descriptions, PlaceholderHealOutcome,
};
use rusqlite::Connection;
use serde::Deserialize;
use serde_json::{json, Value};
use tracing_subscriber::layer::SubscriberExt;

const MIGRATION_ID: &str = "clear-generated-image-placeholder-descriptions-v1";

// ── the capturing tracing layer (thread-scoped: the standing note) ───────────
//
// The P4.D152 rig in shape, narrowed to the MESSAGE: WARN-level only, and this
// migration's two warn arms carry nothing but a `context` and the driver's own
// error string, so there are no id fields to address by key. Kept local rather
// than moved to `test_support::captured()` for the same reason the realign
// family keeps its own — the shared helper cannot express the level filter.
//
// ⚠ **Both warn arms are UNREACHABLE by this corpus, on BOTH sides**, and that
// is deliberate rather than an oversight: v4 warns only from its `catch`,
// which needs a driver-level I/O failure neither side has a seam to inject.
// What the comparand therefore asserts is SILENCE — that the plain
// absent/unusable `else` branch, which two scenarios do reach, adds no warn.
// That is the load-bearing half; a v5 that warned there would redden
// `no-mount-index-skips-the-links` and `links-table-missing-a-column`.

#[derive(Clone, Debug)]
struct CapturedEvent {
    message: String,
}

struct CaptureLayer(Arc<Mutex<Vec<CapturedEvent>>>);

#[derive(Default)]
struct FieldVisitor {
    message: String,
}
impl tracing::field::Visit for FieldVisitor {
    fn record_debug(&mut self, field: &tracing::field::Field, value: &dyn std::fmt::Debug) {
        if field.name() == "message" {
            self.message = format!("{value:?}").trim_matches('"').to_string();
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
        self.0
            .lock()
            .unwrap()
            .push(CapturedEvent { message: v.message });
    }
}

// ── the shared spec ──────────────────────────────────────────────────────────

#[derive(Deserialize)]
struct SpecFile {
    id: String,
    source: String,
    description: Option<String>,
}
#[derive(Deserialize)]
struct SpecLink {
    id: String,
    #[serde(rename = "originalMimeType")]
    original_mime_type: Option<String>,
    description: String,
}
#[derive(Deserialize)]
struct Scenario {
    name: String,
    files: Vec<SpecFile>,
    links: Vec<SpecLink>,
    #[serde(rename = "noMount", default)]
    no_mount: bool,
    #[serde(rename = "linksTableMissingColumn", default)]
    links_table_missing_column: bool,
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
        .join("../../harness/oracle/fixtures/generated-image-placeholder-heal.json")
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
            "4.10.0",
            0_i64,
            "planted by the other app"
        ],
    )
    .expect("ledger row");
}

fn build_main(files: &[SpecFile], spec: &Spec) -> Connection {
    let db = Connection::open_in_memory().expect("open main");
    db.execute_batch(
        "CREATE TABLE \"files\" (\n           \"id\" TEXT PRIMARY KEY,\n           \"userId\" TEXT NOT NULL,\n           \"source\" TEXT NOT NULL,\n           \"description\" TEXT,\n           \"createdAt\" TEXT NOT NULL,\n           \"updatedAt\" TEXT NOT NULL\n         );",
    )
    .expect("main ddl");
    for f in files {
        db.execute(
            "INSERT INTO files (id, userId, source, description, createdAt, updatedAt) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            rusqlite::params![
                f.id,
                spec.user_id,
                f.source,
                f.description,
                spec.created_at,
                spec.created_at
            ],
        )
        .expect("file row");
    }
    db
}

fn build_mount(scenario: &Scenario) -> Option<Connection> {
    if scenario.no_mount {
        return None;
    }
    let db = Connection::open_in_memory().expect("open mount");
    if scenario.links_table_missing_column {
        db.execute_batch(
            "CREATE TABLE \"doc_mount_file_links\" (\"id\" TEXT PRIMARY KEY, \"description\" TEXT NOT NULL DEFAULT '');",
        )
        .expect("mount ddl");
        return Some(db);
    }
    db.execute_batch(
        "CREATE TABLE \"doc_mount_file_links\" (\n           \"id\" TEXT PRIMARY KEY,\n           \"originalMimeType\" TEXT,\n           \"description\" TEXT NOT NULL DEFAULT ''\n         );",
    )
    .expect("mount ddl");
    for l in &scenario.links {
        db.execute(
            "INSERT INTO doc_mount_file_links (id, originalMimeType, description) VALUES (?1, ?2, ?3)",
            rusqlite::params![l.id, l.original_mime_type, l.description],
        )
        .expect("link row");
    }
    Some(db)
}

/// v4's own two-stage `shouldRun()`, spelled here because v5 folds the gate
/// into the pass. The files count SHORT-CIRCUITS: a positive count returns true
/// without the mount being opened at all.
fn v4_should_run(main: &Connection, mount: Option<&Connection>) -> bool {
    const PRED: &str = "(\"description\" LIKE 'Story background for: %' OR \"description\" LIKE '% — wardrobe portrait')";
    let files: i64 = main
        .query_row(
            &format!("SELECT COUNT(*) FROM \"files\" WHERE \"source\" = 'GENERATED' AND {PRED}"),
            [],
            |r| r.get(0),
        )
        .expect("files count");
    if files > 0 {
        return true;
    }
    let Some(mount) = mount else { return false };
    let usable: bool = {
        let exists = mount
            .prepare(
                "SELECT 1 FROM sqlite_master WHERE type='table' AND name='doc_mount_file_links'",
            )
            .and_then(|mut s| s.exists([]))
            .unwrap_or(false);
        if !exists {
            false
        } else {
            let mut stmt = mount
                .prepare("PRAGMA table_info(\"doc_mount_file_links\")")
                .expect("pragma");
            let cols: Vec<String> = stmt
                .query_map([], |r| r.get::<_, String>(1))
                .expect("cols")
                .collect::<Result<Vec<_>, _>>()
                .expect("cols rows");
            cols.iter().any(|c| c == "description") && cols.iter().any(|c| c == "originalMimeType")
        }
    };
    if !usable {
        return false;
    }
    let links: i64 = mount
        .query_row(
            &format!(
                "SELECT COUNT(*) FROM \"doc_mount_file_links\" WHERE \"originalMimeType\" LIKE 'image/%' AND {PRED}"
            ),
            [],
            |r| r.get(0),
        )
        .expect("links count");
    links > 0
}

fn dump_files(main: &Connection, created_at: &str) -> Value {
    let mut stmt = main
        .prepare("SELECT id, source, description, updatedAt FROM files ORDER BY id")
        .expect("prepare dump");
    let rows = stmt
        .query_map([], |r| {
            let updated: String = r.get(3)?;
            Ok(json!({
                "id": r.get::<_, String>(0)?,
                "source": r.get::<_, String>(1)?,
                "description": r.get::<_, Option<String>>(2)?,
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

fn dump_links(mount: Option<&Connection>, missing_column: bool) -> Value {
    let Some(mount) = mount else { return json!([]) };
    let sql = if missing_column {
        "SELECT id, description FROM doc_mount_file_links ORDER BY id"
    } else {
        "SELECT id, originalMimeType, description FROM doc_mount_file_links ORDER BY id"
    };
    let mut stmt = mount.prepare(sql).expect("prepare links dump");
    let rows = stmt
        .query_map([], |r| {
            if missing_column {
                Ok(json!({
                    "id": r.get::<_, String>(0)?,
                    "description": r.get::<_, String>(1)?,
                }))
            } else {
                Ok(json!({
                    "id": r.get::<_, String>(0)?,
                    "originalMimeType": r.get::<_, Option<String>>(1)?,
                    "description": r.get::<_, String>(2)?,
                }))
            }
        })
        .expect("links dump")
        .collect::<Result<Vec<_>, _>>()
        .expect("links dump rows");
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

/// v4's warn shape. Both arms carry only a message and an `error`, so the id
/// fields the realign family compares have no counterpart here — an EMPTY array
/// is the assertion that both skip arms stayed silent (v4 warns from its catch,
/// never from the plain else).
fn shape_warns(events: &[CapturedEvent]) -> Value {
    Value::Array(
        events
            .iter()
            .map(|e| json!({ "message": e.message }))
            .collect(),
    )
}

/// v4's `migrations_metadata` rows, `[{key}]` in key order. The oracle dumps
/// them and this family used to ignore them — the §3 unification review of the
/// `78b381a96` round: deleting v5's metadata upsert left every arm green.
fn metadata(main: &Connection) -> Value {
    let exists = main
        .prepare("SELECT 1 FROM sqlite_master WHERE type='table' AND name='migrations_metadata'")
        .and_then(|mut s| s.exists([]))
        .unwrap_or(false);
    if !exists {
        return json!([]);
    }
    let mut stmt = main
        .prepare("SELECT key FROM migrations_metadata ORDER BY key")
        .expect("prepare metadata");
    let rows = stmt
        .query_map([], |r| Ok(json!({ "key": r.get::<_, String>(0)? })))
        .expect("metadata")
        .collect::<Result<Vec<_>, _>>()
        .expect("metadata rows");
    Value::Array(rows)
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
    outcome: PlaceholderHealOutcome,
    warns: Value,
}

fn run_pass(main: &Connection, mount: Option<&Connection>, now_iso: &str) -> PassResult {
    let logs = Arc::new(Mutex::new(Vec::<CapturedEvent>::new()));
    let subscriber = tracing_subscriber::registry().with(CaptureLayer(logs.clone()));
    let outcome = {
        let _guard = tracing::subscriber::set_default(subscriber);
        clear_generated_image_placeholder_descriptions(main, mount, now_iso).expect("heal")
    };
    let events = logs.lock().unwrap().clone();
    PassResult {
        outcome,
        warns: shape_warns(&events),
    }
}

fn items_affected(o: &PlaceholderHealOutcome) -> i64 {
    match o {
        PlaceholderHealOutcome::Ran {
            files_cleared,
            links_cleared,
            ..
        } => (*files_cleared + *links_cleared) as i64,
        _ => 0,
    }
}

/// v4's `logger.info` context, as the oracle shapes it.
fn info_row(o: &PlaceholderHealOutcome) -> Value {
    match o {
        PlaceholderHealOutcome::Ran {
            files_cleared,
            links_cleared,
            links_skipped,
        } => json!([{
            "message": "Cleared placeholder descriptions from generated images",
            "filesCleared": files_cleared,
            "linksCleared": links_cleared,
            "linksSkipped": links_skipped,
        }]),
        _ => json!([]),
    }
}

#[test]
fn generated_image_placeholder_heal_equivalence() {
    let Ok(path) = std::env::var("QT_ORACLE_PLACEHOLDER_HEAL") else {
        eprintln!("SKIP: QT_ORACLE_PLACEHOLDER_HEAL unset");
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
    let mut cleared_files = 0usize;
    let mut cleared_links = 0usize;
    let mut skipped_links = 0usize;
    let mut singular_sentences = 0usize;
    let mut honoured_ledger = 0usize;
    let mut survivors = 0usize;

    for (i, scenario) in spec.scenarios.iter().enumerate() {
        let o = &oracle[i];
        assert_eq!(
            o["scenario"].as_str().unwrap(),
            scenario.name,
            "scenario order"
        );
        let main = build_main(&scenario.files, &spec);
        let mount = build_mount(scenario);

        if scenario.pre_completed {
            plant_ledger_row(&main, &spec);
        }
        // v4's runner checks the ledger FIRST and never reaches `shouldRun()`.
        let should_run = !scenario.pre_completed && v4_should_run(&main, mount.as_ref());
        if should_run != o["shouldRun"].as_bool().unwrap() {
            failed.push(format!(
                "{}: shouldRun v5={should_run} v4={}",
                scenario.name, o["shouldRun"]
            ));
        }

        let mut result = Value::Null;
        let mut info = json!([]);
        let mut second_run = Value::Null;
        let mut files_after_second = Value::Null;
        let mut links_after_second = Value::Null;

        let pass = run_pass(&main, mount.as_ref(), &spec.now_iso);
        let warns = pass.warns.clone();

        if should_run {
            let affected = items_affected(&pass.outcome);
            let message = pass
                .outcome
                .message()
                .expect("a pass that ran renders a message");
            if let PlaceholderHealOutcome::Ran {
                files_cleared,
                links_cleared,
                links_skipped,
            } = &pass.outcome
            {
                if *files_cleared > 0 {
                    cleared_files += 1;
                }
                if *links_cleared > 0 {
                    cleared_links += 1;
                }
                if *links_skipped {
                    skipped_links += 1;
                }
                if *files_cleared == 1 && *links_cleared == 1 {
                    singular_sentences += 1;
                }
            }
            result = json!({
                "id": MIGRATION_ID,
                "success": true,
                "itemsAffected": affected,
                "message": message,
            });
            info = info_row(&pass.outcome);

            let second = run_pass(&main, mount.as_ref(), &spec.now_iso);
            // v5 honours its own ledger row, so the second pass is
            // `AlreadyCompleted` — and it renders v4's own `itemsAffected: 0`
            // answer, because by then there is nothing left to clear either way.
            second_run = json!({ "itemsAffected": items_affected(&second.outcome) });
            files_after_second = dump_files(&main, &spec.created_at);
            links_after_second = dump_links(mount.as_ref(), scenario.links_table_missing_column);
        } else if scenario.pre_completed {
            // The ledger row from the other app is what stops the pass — and it
            // MUST, because the rows underneath it genuinely carry labels. A
            // heal that ignored the ledger would clear them and the `files` /
            // `links` compares below would redden.
            assert_eq!(
                pass.outcome,
                PlaceholderHealOutcome::AlreadyCompleted,
                "{}: an existing ledger row must stop the pass",
                scenario.name
            );
            honoured_ledger += 1;
        } else {
            assert_eq!(
                pass.outcome,
                PlaceholderHealOutcome::NotApplicable,
                "{}: v4 skips, so v5 must too",
                scenario.name
            );
        }

        let v5_files = dump_files(&main, &spec.created_at);
        let v4_files = normalize_oracle_files(&o["files"], &spec.created_at);
        // Every scenario that keeps a label after the pass is a survivor arm.
        if v5_files.as_array().is_some_and(|rows| {
            rows.iter()
                .any(|r| r["description"].as_str().is_some_and(|d| !d.is_empty()))
        }) {
            survivors += 1;
        }
        cmp(&mut failed, &scenario.name, "files", &v5_files, &v4_files);
        cmp(
            &mut failed,
            &scenario.name,
            "links",
            &dump_links(mount.as_ref(), scenario.links_table_missing_column),
            &o["links"],
        );
        cmp(&mut failed, &scenario.name, "result", &result, &o["result"]);
        cmp(&mut failed, &scenario.name, "warns", &warns, &o["warns"]);
        cmp(&mut failed, &scenario.name, "info", &info, &o["info"]);
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
            cmp(
                &mut failed,
                &scenario.name,
                "linksAfterSecondRun",
                &links_after_second,
                &o["linksAfterSecondRun"],
            );
        }
        // The ledger is a PLAIN EQUALITY on every arm — see the header for why
        // this family has no divergence where the realign family has one.
        cmp(
            &mut failed,
            &scenario.name,
            "ledger",
            &ledger(&main),
            &o["ledger"],
        );
        cmp(
            &mut failed,
            &scenario.name,
            "metadata",
            &metadata(&main),
            &o["metadata"],
        );
    }

    assert!(
        cleared_files >= 4 && cleared_links >= 3,
        "coverage floor: both sides of the sweep must clear rows in several \
         scenarios (files={cleared_files}, links={cleared_links})"
    );
    assert!(
        skipped_links >= 2,
        "coverage floor: the linksSkipped arm must fire on BOTH its routes — no \
         mount at all, and an unusable links table (got {skipped_links})"
    );
    assert_eq!(
        singular_sentences, 1,
        "the message's two singular forms must be rendered exactly once"
    );
    assert_eq!(
        honoured_ledger, 1,
        "the cross-app ledger-honour arm must be exercised exactly once"
    );
    assert!(
        survivors >= 5,
        "coverage floor: the SURVIVING-row arms (uploaded source, non-image \
         link, the — outfit preview tripwire, the anchored near-misses, the \
         honoured ledger) must all still be measured (got {survivors})"
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
