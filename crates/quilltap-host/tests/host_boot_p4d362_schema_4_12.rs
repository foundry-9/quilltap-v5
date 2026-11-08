//! P4.D362 — the `624d0d25c` round's ONE chain (`run_4_12_ensures`: the
//! STAMPED Anthropic sampling strip, then the 22nd / 23rd D23 moves' column
//! ensures) through the REAL `Host::start`, capture-pinned with silence legs.
//!
//! (a) A PRE-ROUND instance — a fresh provision with `chat_messages.
//!     isEstablishedFact` and `connection_profiles.maxImagesPerRequest`
//!     DROPPED, two Anthropic profiles carrying sampling keys (and one OpenAI
//!     profile that must stay), no strip stamp: ONE boot appends both columns
//!     at the END with v4's clauses, rewrites both Anthropic blobs, stamps the
//!     strip with v4's message, logs v4's two DEBUGs + INFO then v5's two INFOs
//!     in registration order, and NONE of v4's column-migration lines;
//! (b) the SECOND boot logs nothing of the chain and rewrites nothing;
//! (c) a fresh `setup` instance's first boot logs ONLY the strip's INFO
//!     (`examined 0, cleaned 0`) and stamps;
//! (d) an instance v4 already stamped → `AlreadyCompleted`: an Anthropic blob
//!     carrying `temperature` planted AFTER v4's stamp stays (v5 honours v4's
//!     row; the repository strip, not the heal, cleans a later write).
//!
//! The chain runs inside `Db::write` (the writer thread), so the capture is
//! the process-wide subscriber.

use std::path::Path;
use std::sync::{Arc, Mutex, OnceLock};

use quilltap_core::db::Writer;
use quilltap_core::services::provisioning::provision_fresh_instance;
use quilltap_core::test_support::CaptureLayer;
use quilltap_host::{Host, HostConfig};
use tracing_subscriber::layer::SubscriberExt;

const PEPPER: &str = "dGVzdHBlcHBlcnRlc3RwZXBwZXJ0ZXN0cGVwcGVyMDE=";
const MAIN: &str = "quilltap.db";
const STRIP_ID: &str = "strip-anthropic-sampling-parameters-v1";

static SERIAL: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

const CTX: &str = "context=migration.strip-anthropic-sampling-parameters";
const ESTABLISHED: &str = "INFO quilltap::boot Made room in the transcript for established fact migrationId=add-established-fact-field-v1 columnAdded=true";
const MAX_IMAGES: &str = "INFO quilltap::boot Set each connection profile's image allowance migrationId=add-profile-max-images-per-request-v1 columnAdded=true backfilled=0";

/// v4's column-migration lines (NO-PORT) — none may appear.
const V4_LINES: [&str; 4] = [
    "Added chat_messages.isEstablishedFact column",
    "Added connection_profiles.maxImagesPerRequest column",
    "Failed to add isEstablishedFact column",
    "Failed to add maxImagesPerRequest column",
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
    config.set_display_zone(quilltap_core::host_zone::TimeZone::UTC);
    config
}

fn boot(base: &Path) -> Vec<String> {
    capture().lock().unwrap().clear();
    let host = Host::start(config(base)).expect("the boot");
    let lines = capture().lock().unwrap().clone();
    drop(host);
    lines
}

/// The chain's lines, in order.
fn ours(lines: &[String]) -> Vec<String> {
    lines
        .iter()
        .filter(|l| {
            l.contains(CTX)
                || l.contains("migrationId=add-established-fact-field-v1")
                || l.contains("migrationId=add-profile-max-images-per-request-v1")
        })
        .cloned()
        .collect()
}

fn open(data: &Path) -> Writer {
    Writer::open_writable(&data.join(MAIN), PEPPER).unwrap()
}

fn table_sql(w: &Writer, name: &str) -> String {
    w.connection()
        .query_row(
            "SELECT sql FROM sqlite_master WHERE name = ?1",
            [name],
            |r| r.get(0),
        )
        .unwrap()
}

fn strip_row(data: &Path) -> Option<(i64, String)> {
    open(data)
        .connection()
        .query_row(
            "SELECT itemsAffected, message FROM migrations_state WHERE id = ?1",
            [STRIP_ID],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .ok()
}

fn params(data: &Path, id: &str) -> String {
    open(data)
        .connection()
        .query_row(
            "SELECT parameters FROM connection_profiles WHERE id = ?1",
            [id],
            |r| r.get(0),
        )
        .unwrap()
}

fn plant_profile(w: &Writer, id: &str, provider: &str, parameters: &str) {
    w.connection()
        .execute(
            "INSERT INTO connection_profiles (id, userId, name, provider, modelName, parameters, createdAt, updatedAt) \
             VALUES (?1, 'u1', ?1, ?2, 'm', ?3, 't', 't')",
            rusqlite::params![id, provider, parameters],
        )
        .unwrap();
}

fn assert_no_v4_lines(lines: &[String]) {
    for v4 in V4_LINES {
        assert!(
            !lines.iter().any(|l| l.contains(v4)),
            "v4's NO-PORT line {v4:?}: {lines:#?}"
        );
    }
}

fn provisioned() -> (tempfile::TempDir, std::path::PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let data = dir.path().join("data");
    std::fs::create_dir_all(&data).unwrap();
    provision_fresh_instance(&data, PEPPER).expect("provision");
    (dir, data)
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_pre_round_instance_gains_both_columns_and_the_strip_in_v4s_order() {
    let _serial = SERIAL.lock().await;
    let (dir, data) = provisioned();
    {
        let w = open(&data);
        w.connection()
            .execute_batch(
                r#"ALTER TABLE "chat_messages" DROP COLUMN "isEstablishedFact";
                   ALTER TABLE "connection_profiles" DROP COLUMN "maxImagesPerRequest";"#,
            )
            .expect("the pre-round plant");
        plant_profile(
            &w,
            "pa",
            "ANTHROPIC",
            r#"{"temperature":0.7,"max_tokens":512}"#,
        );
        plant_profile(&w, "pb", "ANTHROPIC", r#"{"top_k":5,"topP":0.9}"#);
        plant_profile(&w, "po", "OPENAI", r#"{"temperature":0.7}"#);
    }

    // (a) the first boot.
    let lines = boot(dir.path());
    assert_eq!(
        ours(&lines),
        [
            format!("DEBUG quilltap::boot Stripped sampling parameters from Anthropic profile {CTX} profileId=pa removedJson=[\"temperature\"]"),
            format!("DEBUG quilltap::boot Stripped sampling parameters from Anthropic profile {CTX} profileId=pb removedJson=[\"topP\",\"top_k\"]"),
            format!("INFO quilltap::boot Stripped sampling parameters from Anthropic connection profiles {CTX} examined=2 cleaned=2"),
            ESTABLISHED.to_string(),
            MAX_IMAGES.to_string(),
        ],
        "{lines:#?}"
    );
    assert_no_v4_lines(&lines);
    {
        let w = open(&data);
        assert!(table_sql(&w, "chat_messages")
            .ends_with(", \"isEstablishedFact\" INTEGER DEFAULT NULL)"));
        assert!(table_sql(&w, "connection_profiles")
            .ends_with(", \"maxImagesPerRequest\" INTEGER DEFAULT 4)"));
    }
    assert_eq!(params(&data, "pa"), r#"{"max_tokens":512}"#);
    assert_eq!(params(&data, "pb"), "{}");
    assert_eq!(params(&data, "po"), r#"{"temperature":0.7}"#);
    assert_eq!(
        strip_row(&data),
        Some((
            2,
            "Examined 2 Anthropic profile(s); removed sampling parameters from 2".to_string()
        ))
    );

    // (b) the second boot: nothing of the chain, nothing rewritten.
    open(&data)
        .connection()
        .execute(
            "UPDATE connection_profiles SET parameters = '{\"temperature\":1}' WHERE id = 'pa'",
            [],
        )
        .unwrap();
    let lines = boot(dir.path());
    assert!(ours(&lines).is_empty(), "{lines:#?}");
    assert_eq!(
        params(&data, "pa"),
        r#"{"temperature":1}"#,
        "the stamp stops a re-run"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_fresh_instance_logs_only_the_strips_info_and_stamps() {
    let _serial = SERIAL.lock().await;
    let (dir, data) = provisioned();
    let lines = boot(dir.path());
    assert_eq!(
        ours(&lines),
        [format!(
            "INFO quilltap::boot Stripped sampling parameters from Anthropic connection profiles {CTX} examined=0 cleaned=0"
        )],
        "{lines:#?}"
    );
    assert_no_v4_lines(&lines);
    assert_eq!(
        strip_row(&data),
        Some((
            0,
            "Examined 0 Anthropic profile(s); removed sampling parameters from 0".to_string()
        ))
    );
    let lines = boot(dir.path());
    assert!(ours(&lines).is_empty(), "{lines:#?}");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_v4_stamped_instance_is_honoured() {
    let _serial = SERIAL.lock().await;
    let (dir, data) = provisioned();
    {
        let w = open(&data);
        w.connection()
            .execute_batch(
                "CREATE TABLE IF NOT EXISTS migrations_state (id TEXT PRIMARY KEY, completedAt TEXT NOT NULL, \
                 quilltapVersion TEXT NOT NULL, itemsAffected INTEGER NOT NULL DEFAULT 0, message TEXT); \
                 INSERT INTO migrations_state VALUES ('strip-anthropic-sampling-parameters-v1', \
                 '2026-11-04T00:00:00.000Z', '4.13.0-dev.8', 0, 'Examined 0 Anthropic profile(s); removed sampling parameters from 0');",
            )
            .unwrap();
        plant_profile(&w, "pa", "ANTHROPIC", r#"{"temperature":0.7}"#);
    }
    let lines = boot(dir.path());
    assert!(ours(&lines).is_empty(), "{lines:#?}");
    assert_eq!(
        params(&data, "pa"),
        r#"{"temperature":0.7}"#,
        "v4's row is honoured"
    );
    assert_eq!(
        strip_row(&data).unwrap().1,
        "Examined 0 Anthropic profile(s); removed sampling parameters from 0"
    );
}
