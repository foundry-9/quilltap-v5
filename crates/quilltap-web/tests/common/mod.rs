//! Shared test scaffolding: boot a server over either a bare instance (the
//! M0 hand-rolled pattern) or the committed v4-baked chat-send fixture
//! (test-pepper-keyed; built by `harness/oracle/fixtures/
//! build-orchestrator-fixture.ts` — regenerate + re-copy when the fixture
//! spec moves), and serve the router on an ephemeral port.

use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use quilltap_core::api::SINGLE_USER_ID;
use quilltap_core::db::Writer;
use quilltap_host::HostConfig;
use quilltap_web::{boot_startup_status, build_router, web_state, SharedState};

/// The committed fixture's test pepper (harness corpus pepper — synthetic).
pub const TEST_PEPPER: &str = "dGVzdHBlcHBlcnRlc3RwZXBwZXJ0ZXN0cGVwcGVyMDE=";
/// The fixture builder's user id (rewritten to [`SINGLE_USER_ID`] at setup).
#[allow(dead_code)]
pub const FIXTURE_USER: &str = "e18e05bc-63e8-4539-8a85-719b7a508850";

pub fn fixtures_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures")
}

/// The harness `llm_logs` DDL (a fresh encrypted sibling partition).
const LLM_LOGS_DDL: &str = "CREATE TABLE llm_logs (\
    id TEXT PRIMARY KEY, userId TEXT, type TEXT, messageId TEXT, \
    chatId TEXT, characterId TEXT, autonomousRunId TEXT, provider TEXT, \
    modelName TEXT, request TEXT, response TEXT, usage TEXT, \
    cacheUsage TEXT, rawProviderUsage TEXT, requestHashes TEXT, \
    durationMs REAL, createdAt TEXT, updatedAt TEXT);";

/// Rewrite the fixture builder's user id to the engine's `SINGLE_USER_ID`
/// (a real v4 instance's rows all belong to it; the oracle fixture minted its
/// own). Generic: every table with a `userId` column, plus `users.id`.
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

/// Materialize an instance dir from the committed fixture (main + mount +
/// a fresh llm-logs partition), user ids rewritten. Returns the base dir.
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
        // The committed fixture predates v4 `b90cd1f5`; bring its `chats`
        // schema up to HEAD the way v4's `add-turn-skipping-field-v1`
        // migration does on an old instance (idempotent — a regenerated
        // fixture already carries the column).
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
        // The oracle fixture carries no terminal_sessions table (its corpus
        // never spawns); the terminal routes need it (the P4.1c DDL).
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

/// Materialize an instance dir from the committed CHARACTERS fixture (Aria +
/// her vault; the P4.6f/i corpus), user ids rewritten. Used by the characters
/// multipart/binary route tests.
#[allow(dead_code)]
pub fn materialize_characters_instance() -> tempfile::TempDir {
    let base = tempfile::tempdir().expect("tempdir");
    let data = base.path().join("data");
    std::fs::create_dir_all(&data).unwrap();
    std::fs::copy(
        fixtures_dir().join("characters-main.db"),
        data.join("quilltap.db"),
    )
    .unwrap();
    std::fs::copy(
        fixtures_dir().join("characters-mount.db"),
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
        // Idempotent schema top-ups (a regenerated fixture already carries them).
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

/// Materialize an instance dir from the committed P4.6ak text-replacements
/// fixture (three rules + three chats + a background file), user ids rewritten.
/// Used by the text-replacements + get-background web-edge tests.
#[allow(dead_code)]
pub fn materialize_text_replacements_instance() -> tempfile::TempDir {
    let base = tempfile::tempdir().expect("tempdir");
    let data = base.path().join("data");
    std::fs::create_dir_all(&data).unwrap();
    std::fs::copy(
        fixtures_dir().join("text-replacements-main.db"),
        data.join("quilltap.db"),
    )
    .unwrap();
    std::fs::copy(
        fixtures_dir().join("text-replacements-mount.db"),
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

/// A bare instance (empty encrypted main DB) — the M0 pattern.
#[allow(dead_code)]
pub fn materialize_bare_instance() -> tempfile::TempDir {
    let base = tempfile::tempdir().expect("tempdir");
    let data = base.path().join("data");
    std::fs::create_dir_all(&data).unwrap();
    let _ = Writer::open_writable(&data.join("quilltap.db"), TEST_PEPPER).unwrap();
    base
}

/// Boot + serve on an ephemeral port; returns the bound address + state.
/// `configure` tweaks the `HostConfig` (spine factory, terminal toggle, …).
pub async fn serve_instance(
    base_dir: &Path,
    configure: impl FnOnce(HostConfig) -> HostConfig,
) -> (SocketAddr, SharedState) {
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
    let state = web_state(startup, version, base_dir.to_path_buf(), None);
    let router = build_router(Arc::clone(&state));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        let _ = axum::serve(listener, router).await;
    });
    (addr, state)
}
