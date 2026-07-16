//! Shared test scaffolding for the IPC contract suite.
//!
//! The materializers are DUPLICATED from `quilltap-web/tests/common/mod.rs`
//! (the order left extraction-vs-duplication to the lane; duplication keeps
//! quilltap-web's published surface clean — noted in the lane record). The
//! fixture files themselves are read from quilltap-web's committed
//! `tests/fixtures/` tree, so a fixture regeneration updates both suites.
//! Keep the schema top-ups in step with the source module.

use std::path::{Path, PathBuf};

use quilltap_core::api::SINGLE_USER_ID;
use quilltap_core::db::Writer;
use quilltap_host::HostConfig;
use quilltap_web::{boot_startup_status, web_state, SharedState};

/// The committed fixture's test pepper (harness corpus pepper — synthetic).
pub const TEST_PEPPER: &str = "dGVzdHBlcHBlcnRlc3RwZXBwZXJ0ZXN0cGVwcGVyMDE=";
/// The fixture builder's user id (rewritten to [`SINGLE_USER_ID`] at setup).
#[allow(dead_code)]
pub const FIXTURE_USER: &str = "e18e05bc-63e8-4539-8a85-719b7a508850";

/// quilltap-web's committed fixture tree (one copy for both transports).
#[allow(dead_code)]
pub fn fixtures_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../quilltap-web/tests/fixtures")
}

/// The harness `llm_logs` DDL (a fresh encrypted sibling partition).
#[allow(dead_code)]
const LLM_LOGS_DDL: &str = "CREATE TABLE llm_logs (\
    id TEXT PRIMARY KEY, userId TEXT, type TEXT, messageId TEXT, \
    chatId TEXT, characterId TEXT, autonomousRunId TEXT, provider TEXT, \
    modelName TEXT, request TEXT, response TEXT, usage TEXT, \
    cacheUsage TEXT, rawProviderUsage TEXT, requestHashes TEXT, \
    durationMs REAL, createdAt TEXT, updatedAt TEXT);";

/// Rewrite the fixture builder's user id to the engine's `SINGLE_USER_ID`
/// (a real v4 instance's rows all belong to it; the oracle fixture minted
/// its own). Generic: every table with a `userId` column, plus `users.id`.
#[allow(dead_code)]
fn rewrite_user_ids(conn: &rusqlite::Connection) {
    let tables: Vec<String> = {
        let mut stmt = conn
            .prepare("SELECT name FROM sqlite_master WHERE type='table'")
            .unwrap();
        stmt.query_map([], |r| r.get::<_, String>(0))
            .unwrap()
            .filter_map(Result::ok)
            .collect()
    };
    for table in tables {
        let has_user_id: bool = {
            let mut stmt = conn
                .prepare(&format!("PRAGMA table_info({table})"))
                .unwrap();
            let found = stmt
                .query_map([], |r| r.get::<_, String>(1))
                .unwrap()
                .filter_map(Result::ok)
                .any(|c| c == "userId");
            found
        };
        if has_user_id {
            conn.execute(
                &format!("UPDATE {table} SET userId = ?1 WHERE userId = ?2"),
                rusqlite::params![SINGLE_USER_ID, FIXTURE_USER],
            )
            .unwrap();
        }
        if table == "users" {
            conn.execute(
                "UPDATE users SET id = ?1 WHERE id = ?2",
                rusqlite::params![SINGLE_USER_ID, FIXTURE_USER],
            )
            .unwrap();
        }
    }
}

/// A bare instance (empty encrypted main DB) — the M0 pattern.
#[allow(dead_code)]
pub fn materialize_bare_instance() -> tempfile::TempDir {
    let base = tempfile::tempdir().expect("tempdir");
    let data = base.path().join("data");
    std::fs::create_dir_all(&data).unwrap();
    let _ = Writer::open_writable(&data.join("quilltap.db"), TEST_PEPPER).unwrap();
    base
}

/// Materialize an instance dir from the committed chat-send fixture (main +
/// mount + a fresh llm-logs partition), user ids rewritten — the terminal
/// contract case's lineage (the same one `terminal_ws.rs` uses).
#[allow(dead_code)]
pub fn materialize_fixture_instance() -> tempfile::TempDir {
    let base = tempfile::tempdir().expect("tempdir");
    let data = base.path().join("data");
    std::fs::create_dir_all(&data).unwrap();
    std::fs::copy(
        fixtures_dir().join("chat-send-main.db"),
        data.join("quilltap.db"),
    )
    .unwrap();
    std::fs::copy(
        fixtures_dir().join("chat-send-mount.db"),
        data.join("quilltap-mount-index.db"),
    )
    .unwrap();
    {
        let w = Writer::open_writable(&data.join("quilltap-llm-logs.db"), TEST_PEPPER).unwrap();
        w.connection().execute_batch(LLM_LOGS_DDL).unwrap();
    }
    {
        let w = Writer::open_writable(&data.join("quilltap.db"), TEST_PEPPER).unwrap();
        rewrite_user_ids(w.connection());
        // Idempotent schema top-ups (a regenerated fixture already carries
        // them) — same as the source module.
        let has_col: i64 = w
            .connection()
            .query_row(
                "SELECT COUNT(*) FROM pragma_table_info('chats') WHERE name = 'turnSkippingEnabled'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        if has_col == 0 {
            w.connection()
                .execute_batch("ALTER TABLE chats ADD COLUMN turnSkippingEnabled INTEGER;")
                .unwrap();
        }
        w.connection()
            .execute_batch(
                "CREATE TABLE IF NOT EXISTS terminal_sessions (\
                   id TEXT PRIMARY KEY, chatId TEXT, label TEXT, shell TEXT, \
                   cwd TEXT, startedAt TEXT, exitedAt TEXT, exitCode REAL, \
                   transcriptPath TEXT, createdAt TEXT, updatedAt TEXT);",
            )
            .unwrap();
    }
    base
}

/// Boot a host over an instance dir — the IPC analogue of quilltap-web's
/// `serve_instance` (same config quieting), minus the HTTP listener: the
/// IPC surface drives the booted `SharedState` directly.
pub fn boot_instance(
    base_dir: &Path,
    configure: impl FnOnce(HostConfig) -> HostConfig,
) -> SharedState {
    let mut config = HostConfig::new(base_dir);
    config.env_pepper = Some(TEST_PEPPER.to_string());
    config.tz = "UTC".to_string();
    // Keep the daily sweeps quiet in tests (long grace = never fires).
    config.startup_grace_ms = 3_600_000;
    config.danger_scan_interval_ms = 3_600_000;
    config.autonomous_tick_ms = 3_600_000;
    let config = configure(config);
    let version = config.version.clone();
    let startup = boot_startup_status(config);
    web_state(startup, version, base_dir.to_path_buf(), None)
}
