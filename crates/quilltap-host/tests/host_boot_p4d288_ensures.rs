//! P4.D288 (C1-2) — the span's boot ensures through the REAL `Host::start`:
//! their slot order in `seed_built_ins` and v5's ONE boot INFO each, capture-
//! pinned, with the silence leg on a no-op boot.
//!
//! (a) A PRE-ROUND instance — a fresh provision with this round's surfaces taken
//!     back (no `memories.supersededAt` / its index, no `chats` scenario pair,
//!     the cursor table gone and the OTHER watermark column back with a value,
//!     the three dead `chat_settings` columns + the dead instance key, retired
//!     job rows, no `tfidf_vocabularies.textShapeVersion`): ONE boot logs every
//!     ensure's line, in v4's registration order, with its outcome — and none of
//!     v4's migration lines; the episode and cursor ensures STAMP their ledger
//!     rows (R-D) and no other ensure writes one.
//! (b) The SECOND boot is silent (every ensure a no-op).
//! (c) A fresh `setup` instance runs only the two ledgered rewrites (0 rows,
//!     STAMPED) on its first boot, then boots silent.

use std::path::Path;
use std::sync::{Arc, Mutex, OnceLock};

use quilltap_core::db::Writer;
use quilltap_core::services::provisioning::provision_fresh_instance;
use quilltap_core::test_support::CaptureLayer;
use quilltap_host::{Host, HostConfig};
use tracing_subscriber::layer::SubscriberExt;

const PEPPER: &str = "dGVzdHBlcHBlcnRlc3RwZXBwZXJ0ZXN0cGVwcGVyMDE=";
const MAIN: &str = "quilltap.db";
const SINGLE_USER: &str = "ffffffff-ffff-ffff-ffff-ffffffffffff";

static SERIAL: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

/// v5's lines, in the host's (v4's registration) order.
const EXPECTED: [&str; 11] = [
    "INFO quilltap::boot Fitted the memory supersession clock migrationId=add-memory-superseded-at-v1 columnAdded=true backfilledRows=1",
    "INFO quilltap::boot Added the chat scenario source columns migrationId=add-chat-scenario-source-v1 columnsAdded=2",
    "INFO quilltap::boot Backfilled the episode watermarks from summary anchors migrationId=add-episode-extraction-watermark-v1 columnAdded=true candidates=1 backfilled=1",
    "INFO quilltap::boot Recomputed reinforced importance migrationId=recompute-reinforced-importance-v1 rewritten=1 total=1",
    "INFO quilltap::boot Re-stamped legacy UTC-midnight memory dates migrationId=rezone-legacy-memory-occurred-at-v1 timeZone=UTC rewritten=1 candidates=1",
    "INFO quilltap::boot Created idx_memories_supersededById migrationId=add-memories-superseded-by-index-v1",
    "INFO quilltap::boot Moved the memory maintenance settings migrationId=move-memory-maintenance-settings-v1 copied=true droppedColumns=3 droppedInstanceKey=true",
    "INFO quilltap::boot Moved the fold-pass watermarks into extraction cursors migrationId=add-memory-extraction-cursors-v1 tableCreated=true backfilled=2 droppedColumns=2",
    "INFO quilltap::boot Deleted retired INTER_CHARACTER_MEMORY jobs migrationId=drop-inter-character-memory-jobs-v1 deleted=1",
    "INFO quilltap::boot Added tfidf_vocabularies.textShapeVersion migrationId=add-tfidf-text-shape-version-v1",
    "INFO quilltap::boot Deleted retired housekeeping and consolidation jobs migrationId=drop-housekeeping-consolidation-jobs-v1 deleted=2",
];

/// The span's migration ids — the needle that finds ANY of v5's lines.
const IDS: [&str; 11] = [
    "add-memory-superseded-at-v1",
    "add-chat-scenario-source-v1",
    "add-episode-extraction-watermark-v1",
    "recompute-reinforced-importance-v1",
    "rezone-legacy-memory-occurred-at-v1",
    "add-memories-superseded-by-index-v1",
    "move-memory-maintenance-settings-v1",
    "add-memory-extraction-cursors-v1",
    "drop-inter-character-memory-jobs-v1",
    "add-tfidf-text-shape-version-v1",
    "drop-housekeeping-consolidation-jobs-v1",
];

/// v4's migration lines (NO-PORT) — none may appear.
const V4_LINES: [&str; 6] = [
    "Added memories.supersededAt column",
    "Backfilled supersededAt from updatedAt",
    "Backfilled episode watermarks from summary anchors",
    "memory_extraction_cursors table in place",
    "Copied watermark column into cursors",
    "Moved memory maintenance settings and dropped the dead memory knobs",
];

fn capture() -> &'static Arc<Mutex<Vec<String>>> {
    static LOGS: OnceLock<Arc<Mutex<Vec<String>>>> = OnceLock::new();
    LOGS.get_or_init(|| {
        let logs = Arc::new(Mutex::new(Vec::new()));
        tracing::subscriber::set_global_default(
            tracing_subscriber::registry().with(CaptureLayer(logs.clone())),
        )
        .expect("this binary owns the global subscriber");
        logs
    })
}

fn config(base: &Path) -> HostConfig {
    let mut config = HostConfig::new(base);
    config.instances_path = Some(base.join("instances.json"));
    config.env_pepper = Some(PEPPER.to_string());
    config.autonomous_tick_ms = 3_600_000;
    config.stuck_check_ms = 3_600_000;
    config.terminal = false;
    config.seed_sample_content = false;
    // The rezone's host rung (O-8) is the host's calendar zone: pinned, so the
    // expected `timeZone=UTC` holds on any machine.
    config.set_display_zone(quilltap_core::host_zone::TimeZone::UTC);
    config
}

fn exec(data: &Path, sql: &str) {
    let w = Writer::open_writable(&data.join(MAIN), PEPPER).unwrap();
    w.connection()
        .execute_batch(sql)
        .unwrap_or_else(|e| panic!("plant {sql:?}: {e}"));
}

fn boot(base: &Path) -> Vec<String> {
    capture().lock().unwrap().clear();
    let host = Host::start(config(base)).expect("the boot");
    let lines = capture().lock().unwrap().clone();
    drop(host);
    lines
}

fn ours(lines: &[String]) -> Vec<String> {
    lines
        .iter()
        .filter(|l| {
            IDS.iter()
                .any(|id| l.contains(&format!("migrationId={id}")))
        })
        .cloned()
        .collect()
}

fn ledger_ids(data: &Path) -> Vec<String> {
    let w = Writer::open_writable(&data.join(MAIN), PEPPER).unwrap();
    let c = w.connection();
    if c.query_row(
        "SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = 'migrations_state'",
        [],
        |_| Ok(()),
    )
    .is_err()
    {
        return Vec::new();
    }
    let mut stmt = c
        .prepare("SELECT id FROM migrations_state ORDER BY id")
        .unwrap();
    // The span's ids only — other boot heals stamp their own rows.
    let ids = stmt
        .query_map([], |r| r.get::<_, String>(0))
        .unwrap()
        .map(Result::unwrap)
        .filter(|id| IDS.contains(&id.as_str()))
        .collect();
    ids
}

fn derive_pre_round(data: &Path) {
    provision_fresh_instance(data, PEPPER).expect("provision");
    exec(
        data,
        &format!(
            "DROP INDEX \"idx_memories_supersededById\"; \
             ALTER TABLE memories DROP COLUMN supersededAt; \
             ALTER TABLE chats DROP COLUMN scenarioSource; \
             ALTER TABLE chats DROP COLUMN scenarioNotes; \
             DROP TABLE memory_extraction_cursors; \
             ALTER TABLE chats ADD COLUMN \"otherExtractionWatermarkMessageId\" TEXT DEFAULT NULL; \
             ALTER TABLE chat_settings ADD COLUMN \"autoHousekeepingSettings\" TEXT; \
             ALTER TABLE chat_settings ADD COLUMN \"memoryExtractionLimits\" TEXT; \
             ALTER TABLE chat_settings ADD COLUMN \"memoryExtractionConcurrency\" INTEGER DEFAULT 1; \
             UPDATE chat_settings SET autoHousekeepingSettings = '{{\"enabled\":true,\"perCharacterCap\":900}}' \
               WHERE userId = '{SINGLE_USER}'; \
             INSERT INTO instance_settings (key, value) VALUES ('memoryExtractionConcurrency', '3'); \
             ALTER TABLE tfidf_vocabularies DROP COLUMN textShapeVersion; \
             INSERT INTO memories (id, characterId, content, summary, supersededById, importance, reinforcementCount, \
               reinforcedImportance, occurredAt, createdAt, updatedAt) \
               VALUES ('m1', 'c1', 'x', 'y', 'd1', 0.5, 2, 0.5, '2026-07-14T00:00:00Z', \
               '2026-01-01T00:00:00.000Z', '2026-02-01T00:00:00.000Z'); \
             INSERT INTO chats (id, userId, title, otherExtractionWatermarkMessageId, summaryAnchorMessageIds, createdAt, updatedAt) \
               VALUES ('chat1', 'u1', 't', 'm9', '[\"a1\",\"a2\"]', 't', 't'); \
             INSERT INTO background_jobs (id, userId, type, status, payload, scheduledAt, createdAt, updatedAt) VALUES \
               ('j1', 'u1', 'INTER_CHARACTER_MEMORY', 'PENDING', '{{}}', 't', 't', 't'), \
               ('j2', 'u1', 'MEMORY_HOUSEKEEPING', 'COMPLETED', '{{}}', 't', 't', 't'), \
               ('j3', 'u1', 'MEMORY_CONSOLIDATION', 'FAILED', '{{}}', 't', 't', 't');"
        ),
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_pre_round_instance_runs_every_ensure_once_in_v4s_order() {
    let _serial = SERIAL.lock().await;
    let dir = tempfile::tempdir().unwrap();
    let data = dir.path().join("data");
    std::fs::create_dir_all(&data).unwrap();
    derive_pre_round(&data);

    let lines = boot(dir.path());
    assert_eq!(ours(&lines), EXPECTED, "{lines:#?}");
    for v4 in V4_LINES {
        assert!(
            !lines.iter().any(|l| l.contains(v4)),
            "v4's NO-PORT line {v4:?}: {lines:#?}"
        );
    }
    assert_eq!(
        ledger_ids(&data),
        [
            "add-episode-extraction-watermark-v1",
            "add-memory-extraction-cursors-v1",
            "recompute-reinforced-importance-v1",
            "rezone-legacy-memory-occurred-at-v1",
        ],
        "only the chain (R-D) and the two data rewrites (R-E) are STAMPED"
    );
    {
        let w = Writer::open_writable(&data.join(MAIN), PEPPER).unwrap();
        let mut stmt = w
            .connection()
            .prepare("SELECT id, messageId FROM memory_extraction_cursors ORDER BY id")
            .unwrap();
        let cursors: Vec<(String, String)> = stmt
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))
            .unwrap()
            .map(Result::unwrap)
            .collect();
        assert_eq!(
            cursors,
            [
                ("chat1:episode-fold".to_string(), "a2".to_string()),
                ("chat1:other-fold".to_string(), "m9".to_string()),
            ]
        );
    }

    // (b) the second boot: every ensure a no-op, silent.
    let lines = boot(dir.path());
    assert!(ours(&lines).is_empty(), "{lines:#?}");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_fresh_instance_runs_only_the_two_ledgered_rewrites_once() {
    // A fresh `setup` instance carries no v4 ledger, so — as on v4's first boot,
    // where both `shouldRun`s hold — the two data rewrites run once over zero
    // rows and STAMP (R-E: "a 0-row instance STAMPS"); every structure-gated
    // ensure is a no-op. The second boot is silent.
    let _serial = SERIAL.lock().await;
    let dir = tempfile::tempdir().unwrap();
    let data = dir.path().join("data");
    std::fs::create_dir_all(&data).unwrap();
    provision_fresh_instance(&data, PEPPER).expect("provision");
    let lines = boot(dir.path());
    assert_eq!(
        ours(&lines),
        [
            "INFO quilltap::boot Recomputed reinforced importance migrationId=recompute-reinforced-importance-v1 rewritten=0 total=0",
            "INFO quilltap::boot Re-stamped legacy UTC-midnight memory dates migrationId=rezone-legacy-memory-occurred-at-v1 timeZone=UTC rewritten=0 candidates=0",
        ],
        "{lines:#?}"
    );
    assert_eq!(
        ledger_ids(&data),
        [
            "recompute-reinforced-importance-v1",
            "rezone-legacy-memory-occurred-at-v1"
        ]
    );
    let lines = boot(dir.path());
    assert!(ours(&lines).is_empty(), "{lines:#?}");
}
