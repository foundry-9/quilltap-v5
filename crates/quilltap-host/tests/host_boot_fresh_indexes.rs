//! P4.153 (dogfood #149) — a freshly `setup` v5 instance, booted ONCE, carries
//! every secondary index a real v4 first boot carries.
//!
//! v4 has two index families: the generateDDL one its repositories make
//! (`fresh_schema.json`) and the one its MIGRATIONS make in PHASE 1 of every
//! first boot, before any repository runs (`migration_indexes.json`, dumped from
//! v4's REAL `MigrationRunner` by `harness/oracle/provision/
//! dump-migration-indexes.ts`). v5 provisioned the first family only, so a
//! restore into a fresh v5 instance scanned `chat_messages` once per message
//! (2 h 26 m against 9 m into a migrated copy).
//!
//! The acceptance set is `setup` + ONE host boot (the order's R-C): a handful
//! of migration indexes v5 already re-creates at boot (`idx_files_generationKey`,
//! `idx_folders_userId_projectId_path`, `idx_help_doc_chunks_docId` — measured;
//! the three `chat_informs` ones the order expected there are NOT made on a
//! fresh instance, since `ensure_chat_informs_table` fires only when the table
//! is absent), and the provisioner now makes them too, so the boot's
//! `IF NOT EXISTS` ensures become no-ops — pinned here by the first boot
//! creating no index and `sqlite_master` staying byte-identical across a second.
//!
//! The query-plan arms are the order's R-G, the restore timing's in-lane proxy:
//! the per-message `get_messages` read the restore's `add_message` makes and
//! the per-chat last-played read, both `USING INDEX idx_chat_messages_chatId`,
//! and `find_pending_for_participant`, `USING INDEX idx_chat_informs_pending`.
//! Since P4.164 each is driven through its PRODUCTION function and the arm
//! plans the statement that function actually ran — captured with SQLite's
//! statement trace (`traced`) — never a hand-copied string; the mutation arm
//! (`the_plan_arm_reddens_without_its_index`) drops both indexes and pins that
//! every captured plan then leaves them.
//!
//! Nine arms: the index family, the provisioner-vs-boot pin, the plan arm and
//! its mutation twin (P4.164), and the five UNIQUE-by-behaviour arms.
//!
//! Red-first (P4.153's lane record): on `main` before the provisioner replayed
//! `migration_indexes.json`, every arm then present failed (the record said
//! "seven"; there were eight — the four UNIQUE arms it named plus
//! `idx_doc_mount_folders_mp_path`'s) — the family arm missed 56
//! names after the boot (main 47 / mount-index 4 / llm-logs 5), the first boot
//! created three indexes the provisioner had not, the `get_messages` plan was
//! `SCAN chat_messages USING INDEX idx_chat_messages_createdAt`, and each of the
//! UNIQUE duplicates was ACCEPTED.
//!
//! Run:
//!   cargo test -p quilltap-host --test host_boot_fresh_indexes

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use quilltap_core::db::Writer;
use quilltap_core::services::provisioning::provision_fresh_instance;
use quilltap_host::{Host, HostConfig};
use serde_json::Value;

const PEPPER: &str = "dGVzdHBlcHBlcnRlc3RwZXBwZXJ0ZXN0cGVwcGVyMDE=";
const PARTITIONS: [(&str, &str); 3] = [
    ("main", "quilltap.db"),
    ("mountIndex", "quilltap-mount-index.db"),
    ("llmLogs", "quilltap-llm-logs.db"),
];

/// Boots run one at a time (each `Host` starts its own drivers).
static SERIAL: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

fn config(base: &Path) -> HostConfig {
    let mut config = HostConfig::new(base);
    config.instances_path = Some(base.join("instances.json"));
    config.env_pepper = Some(PEPPER.to_string());
    config.autonomous_tick_ms = 3_600_000;
    config.stuck_check_ms = 3_600_000;
    config.terminal = false;
    config.seed_sample_content = false;
    config
}

fn provisioning_artifact(name: &str) -> Value {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../quilltap-core/src/services/provisioning")
        .join(name);
    serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap()
}

/// The index name a `CREATE [UNIQUE] INDEX [IF NOT EXISTS] "name"` statement
/// makes; `None` for a table.
fn index_name(sql: &str) -> Option<String> {
    let rest = sql.strip_prefix("CREATE ")?;
    let rest = rest.strip_prefix("UNIQUE ").unwrap_or(rest);
    let rest = rest.strip_prefix("INDEX ")?;
    let rest = rest.strip_prefix("IF NOT EXISTS ").unwrap_or(rest);
    Some(
        rest.split([' ', '('])
            .next()
            .unwrap()
            .trim_matches('"')
            .to_string(),
    )
}

/// Every index name both artifacts promise for one partition.
fn promised(partition: &str) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    for artifact in ["fresh_schema.json", "migration_indexes.json"] {
        for sql in provisioning_artifact(artifact)[partition]
            .as_array()
            .unwrap()
        {
            if let Some(name) = index_name(sql.as_str().unwrap()) {
                out.insert(name);
            }
        }
    }
    out
}

/// One partition's `sqlite_master` as `(type, name, sql)`, by name.
fn master(data: &Path, file: &str) -> Vec<(String, String, Option<String>)> {
    let w = Writer::open_writable(&data.join(file), PEPPER).unwrap();
    let mut stmt = w
        .connection()
        .prepare("SELECT type, name, sql FROM sqlite_master ORDER BY type, name")
        .unwrap();
    let rows = stmt
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))
        .unwrap()
        .map(Result::unwrap)
        .collect();
    rows
}

fn index_names(data: &Path, file: &str) -> BTreeSet<String> {
    master(data, file)
        .into_iter()
        .filter(|(t, n, _)| t == "index" && !n.starts_with("sqlite_"))
        .map(|(_, n, _)| n)
        .collect()
}

/// Provision a fresh instance and boot the real `Host` over it once.
fn setup_and_boot() -> (tempfile::TempDir, PathBuf, Host) {
    let dir = tempfile::tempdir().unwrap();
    let data = dir.path().join("data");
    std::fs::create_dir_all(&data).unwrap();
    provision_fresh_instance(&data, PEPPER).expect("provision");
    let host = Host::start(config(dir.path())).expect("first boot");
    (dir, data, host)
}

/// `EXPLAIN QUERY PLAN` details of `sql` on `conn`, joined. The args bind
/// the statement's own `?N` parameters (only as many as it declares).
fn plan(conn: &rusqlite::Connection, sql: &str, args: &[&str]) -> String {
    let mut stmt = conn.prepare(&format!("EXPLAIN QUERY PLAN {sql}")).unwrap();
    let wanted = stmt.parameter_count();
    let details: Vec<String> = stmt
        .query_map(rusqlite::params_from_iter(args.iter().take(wanted)), |r| {
            r.get::<_, String>(3)
        })
        .unwrap()
        .map(Result::unwrap)
        .collect();
    details.join(" | ")
}

/// P4.164 — the SQL a PRODUCTION read actually runs, captured off the
/// connection with SQLite's own statement trace (`sqlite3_trace_v2`,
/// `SQLITE_TRACE_STMT`, through `rusqlite::ffi` — the workspace builds
/// rusqlite without its `trace` feature, and a `pub const` per statement
/// would be a production hunk). Each entry is `sqlite3_sql` — the prepared
/// text with its `?N` placeholders, not the expanded one — in the order the
/// statements first stepped. The plan arms plan THESE texts, so a production
/// query that drifts off its index reddens the arm (the hand-copied SQL the
/// arms planned before could not).
fn traced<T>(conn: &rusqlite::Connection, read: impl FnOnce() -> T) -> (T, Vec<String>) {
    use rusqlite::ffi;
    use std::ffi::{c_int, c_uint, c_void, CStr};

    unsafe extern "C" fn on_stmt(
        _mask: c_uint,
        ctx: *mut c_void,
        stmt: *mut c_void,
        _x: *mut c_void,
    ) -> c_int {
        // SAFETY: `ctx` is the `Vec<String>` `traced` registered and keeps
        // alive until it unregisters; `stmt` is the stepping statement.
        let seen = unsafe { &mut *(ctx as *mut Vec<String>) };
        let sql = unsafe { ffi::sqlite3_sql(stmt as *mut ffi::sqlite3_stmt) };
        if !sql.is_null() {
            let sql = unsafe { CStr::from_ptr(sql) }
                .to_string_lossy()
                .into_owned();
            if !seen.contains(&sql) {
                seen.push(sql);
            }
        }
        0
    }

    let mut seen: Box<Vec<String>> = Box::default();
    // SAFETY: the handle outlives both calls (borrowed from `conn`); the
    // callback is unregistered before `seen` is read or dropped.
    unsafe {
        let db = conn.handle();
        let rc = ffi::sqlite3_trace_v2(
            db,
            ffi::SQLITE_TRACE_STMT as c_uint,
            Some(on_stmt),
            &mut *seen as *mut Vec<String> as *mut c_void,
        );
        assert_eq!(rc, ffi::SQLITE_OK, "sqlite3_trace_v2 register");
    }
    let out = read();
    unsafe {
        let rc = ffi::sqlite3_trace_v2(conn.handle(), 0, None, std::ptr::null_mut());
        assert_eq!(rc, ffi::SQLITE_OK, "sqlite3_trace_v2 unregister");
    }
    (out, *seen)
}

/// Run one production read under [`traced`], keep every captured statement
/// that reads `table`, and answer each one's plan. Panics if the read ran
/// no statement over `table` (a capture that saw nothing proves nothing).
fn production_plans(
    conn: &rusqlite::Connection,
    table: &str,
    args: &[&str],
    read: impl FnOnce(),
) -> Vec<(String, String)> {
    let ((), statements) = traced(conn, read);
    let reads: Vec<(String, String)> = statements
        .into_iter()
        .filter(|sql| {
            sql.trim_start().to_ascii_uppercase().starts_with("SELECT")
                && sql.contains(&format!("FROM {table}"))
        })
        .map(|sql| {
            let p = plan(conn, &sql, args);
            (sql, p)
        })
        .collect();
    assert!(
        !reads.is_empty(),
        "the production read ran no SELECT over {table}"
    );
    reads
}

/// Every captured read's plan uses `index`.
fn assert_all_use(label: &str, plans: &[(String, String)], index: &str) {
    for (sql, p) in plans {
        assert!(
            p.contains(&format!("USING INDEX {index}")),
            "{label}: the production statement does not use {index}\n  sql: {sql}\n  plan: {p}"
        );
    }
}

#[tokio::test]
async fn setup_plus_one_boot_carries_both_index_families() {
    let _serial = SERIAL.lock().await;
    let (_dir, data, _host) = setup_and_boot();
    let mut missing = Vec::new();
    for (partition, file) in PARTITIONS {
        let have = index_names(&data, file);
        for name in promised(partition) {
            if !have.contains(&name) {
                missing.push(format!("{partition}:{name}"));
            }
        }
    }
    assert!(
        missing.is_empty(),
        "{} promised index(es) absent after setup + one boot:\n{}",
        missing.len(),
        missing.join("\n")
    );
}

#[tokio::test]
async fn the_provisioner_leaves_the_boot_nothing_to_create() {
    let _serial = SERIAL.lock().await;
    let dir = tempfile::tempdir().unwrap();
    let data = dir.path().join("data");
    std::fs::create_dir_all(&data).unwrap();
    provision_fresh_instance(&data, PEPPER).expect("provision");
    let provisioned: Vec<_> = PARTITIONS
        .iter()
        .map(|(_, f)| index_names(&data, f))
        .collect();

    let first = Host::start(config(dir.path())).expect("first boot");
    let after_first: Vec<_> = PARTITIONS.iter().map(|(_, f)| master(&data, f)).collect();
    let first_indexes: Vec<_> = PARTITIONS
        .iter()
        .map(|(_, f)| index_names(&data, f))
        .collect();
    // Every index the first boot ensures, the provisioner already made.
    for (i, (partition, _)) in PARTITIONS.iter().enumerate() {
        let created: Vec<_> = first_indexes[i].difference(&provisioned[i]).collect();
        assert!(
            created.is_empty(),
            "{partition}: the first boot created index(es) the provisioner did not: {created:?}"
        );
    }

    drop(first);
    let _second = Host::start(config(dir.path())).expect("second boot");
    let after_second: Vec<_> = PARTITIONS.iter().map(|(_, f)| master(&data, f)).collect();
    assert_eq!(
        after_first, after_second,
        "a second boot changed sqlite_master"
    );
}

const CHAT: &str = "00000000-0000-0000-0000-000000000001";
const PARTICIPANT: &str = "00000000-0000-0000-0000-000000000002";

/// One production read's label, the index it must use, and its captured
/// `(statement, plan)` pairs.
type ReadPlans = (&'static str, &'static str, Vec<(String, String)>);

/// The three per-chat reads, each driven through its PRODUCTION function and
/// planned on the statement it ran ([`production_plans`]).
fn per_chat_read_plans(conn: &rusqlite::Connection) -> [ReadPlans; 3] {
    use quilltap_core::db::chat_informs::ChatInformsRepository;
    use quilltap_core::db::chats_messages_read::{get_last_played_message_at, get_messages};
    [
        // The restore's `add_message` re-reads the chat's messages after every
        // insert (`update_chat_metadata` → `get_messages`).
        (
            "get_messages",
            "idx_chat_messages_chatId",
            production_plans(conn, "chat_messages", &[CHAT], || {
                get_messages(conn, CHAT).unwrap();
            }),
        ),
        // The per-chat last-played read the restore makes after each chat
        // (`orchestrator.rs`, `get_last_played_message_at`).
        (
            "get_last_played_message_at",
            "idx_chat_messages_chatId",
            production_plans(conn, "chat_messages", &[CHAT], || {
                get_last_played_message_at(conn, CHAT).unwrap();
            }),
        ),
        // `find_pending_for_participant` — `idx_chat_informs_pending`, which a
        // fresh instance lacked even after a boot before P4.153 (the boot's
        // `ensure_chat_informs_table` only fires when the TABLE is absent, and
        // the provisioner made the table).
        (
            "find_pending_for_participant",
            "idx_chat_informs_pending",
            production_plans(conn, "chat_informs", &[CHAT, PARTICIPANT], || {
                ChatInformsRepository::new(conn)
                    .find_pending_for_participant(CHAT, PARTICIPANT)
                    .unwrap();
            }),
        ),
    ]
}

#[tokio::test]
async fn the_per_chat_message_reads_use_the_chat_id_index() {
    let _serial = SERIAL.lock().await;
    let (_dir, data, _host) = setup_and_boot();
    let w = Writer::open_writable(&data.join("quilltap.db"), PEPPER).unwrap();
    for (label, index, plans) in per_chat_read_plans(w.connection()) {
        assert_all_use(label, &plans, index);
    }
}

/// P4.164 — the arm above can fail: with each index DROPPED on the test
/// instance, every production read's plan leaves it (a `SCAN`, or another
/// index), so `assert_all_use` would redden. Measured at P4.164 (the lane
/// record carries the plans).
#[tokio::test]
async fn the_plan_arm_reddens_without_its_index() {
    let _serial = SERIAL.lock().await;
    let (_dir, data, _host) = setup_and_boot();
    let w = Writer::open_writable(&data.join("quilltap.db"), PEPPER).unwrap();
    let conn = w.connection();
    conn.execute_batch("DROP INDEX idx_chat_messages_chatId; DROP INDEX idx_chat_informs_pending;")
        .unwrap();
    for (label, index, plans) in per_chat_read_plans(conn) {
        for (sql, p) in &plans {
            assert!(
                !p.contains(index),
                "{label}: still planned on the dropped {index}\n  sql: {sql}\n  plan: {p}"
            );
            eprintln!("{label} without {index}: {p}");
        }
    }
}

// ───────────── the UNIQUE indexes by behaviour (the order's R-E) ─────────────
//
// A real v4 first boot makes five UNIQUE indexes from its migrations (measured
// at `94fbb1ae3`; `idx_project_doc_mount_links_proj_mp`, the walk's fourth, is
// NOT among them — `convert-project-files-to-document-stores`'s `shouldRun` is
// false on an empty instance, so only an instance that once had project files
// carries it). `idx_folders_userId_projectId_path` was already made by v5's
// boot (`folders_unique_path_repair`), so it pins nothing new here. The other
// four each have an above-SQLite guard on v5's ordinary path, as on v4's (the
// connection-profiles create's 409 `A connection profile named "…" already
// exists`; `add_member`, `link` and `open_document` find the pair first) — so
// each arm drives the repository's RAW `create`, the writer the restore and
// the importer reach, twice on a freshly `setup` + booted instance. The second
// write must fail with the bytes v4's SQLite gives the same duplicate
// (measured on a copy of the real-boot instance through
// better-sqlite3-multiple-ciphers). Red-first: on `main` before P4.153 every
// second write SUCCEEDED.

fn opts_ts(n: u8) -> (String, String) {
    (
        format!("00000000-0000-4000-8000-0000000000{n:02}"),
        "2026-10-06T00:00:00.000Z".to_string(),
    )
}

fn assert_refused(result: Result<(), quilltap_core::db::DbError>, v4_bytes: &str) {
    match result {
        Ok(()) => panic!("the duplicate was ACCEPTED; v4 refuses it with {v4_bytes:?}"),
        Err(e) => assert_eq!(quilltap_core::db::fallback::error_text(&e), v4_bytes),
    }
}

#[tokio::test]
async fn idx_chat_documents_unique_refuses_a_duplicate_document_row() {
    use quilltap_core::db::chat_documents::{CdCreate, ChatDocumentsRepository, CreateOptions};
    let _serial = SERIAL.lock().await;
    let (_dir, data, _host) = setup_and_boot();
    let w = Writer::open_writable(&data.join("quilltap.db"), PEPPER).unwrap();
    let repo = ChatDocumentsRepository::new(w.connection());
    let row = || CdCreate {
        chat_id: "c".into(),
        file_path: "f.md".into(),
        scope: "project".into(),
        mount_point: Some("mp".into()),
        display_title: None,
        is_active: false,
    };
    let opts = |n| {
        let (id, ts) = opts_ts(n);
        CreateOptions {
            id,
            created_at: ts.clone(),
            updated_at: ts,
        }
    };
    repo.create(&row(), &opts(1)).unwrap();
    assert_refused(
        repo.create(&row(), &opts(2)),
        "UNIQUE constraint failed: chat_documents.chatId, chat_documents.filePath, \
         chat_documents.scope, chat_documents.mountPoint",
    );
}

#[tokio::test]
async fn idx_connection_profiles_user_id_name_refuses_a_trimmed_case_folded_duplicate() {
    use quilltap_core::db::connection_profiles::{
        ConnectionProfilesRepository, CpCreate, CreateOptions,
    };
    let _serial = SERIAL.lock().await;
    let (_dir, data, _host) = setup_and_boot();
    let w = Writer::open_writable(&data.join("quilltap.db"), PEPPER).unwrap();
    let repo = ConnectionProfilesRepository::new(w.connection());
    let profile = |name: &str| CpCreate {
        user_id: "ffffffff-ffff-ffff-ffff-ffffffffffff".into(),
        name: name.into(),
        provider: "OPENAI".into(),
        transport: "direct".into(),
        courier_delta_mode: false,
        api_key_id: None,
        base_url: None,
        model_name: "m".into(),
        parameters: serde_json::json!({}),
        is_default: false,
        is_cheap: false,
        allow_web_search: false,
        use_native_web_search: false,
        allow_tool_use: true,
        pseudo_tool_mode: "auto".into(),
        multi_character_prefill: None,
        model_class: None,
        fallback_profile_id: None,
        allow_tier_fallback: false,
        max_context: None,
        max_tokens: None,
        is_dangerous_compatible: false,
        supports_image_upload: false,
        tags: Vec::new(),
        sort_index: 0.0,
        total_tokens: 0.0,
        total_prompt_tokens: 0.0,
        total_completion_tokens: 0.0,
        message_count: 0.0,
    };
    let opts = |n| {
        let (id, ts) = opts_ts(n);
        CreateOptions {
            id,
            created_at: ts.clone(),
            updated_at: ts,
        }
    };
    repo.create(&profile("Alpha"), &opts(1)).unwrap();
    // The EXPRESSION index — `lower(trim("name"))` — so the case- and
    // space-different name is the duplicate.
    assert_refused(
        repo.create(&profile("  alpha "), &opts(2)),
        "UNIQUE constraint failed: index 'idx_connection_profiles_userId_name'",
    );
}

#[tokio::test]
async fn idx_group_character_members_group_char_refuses_a_duplicate_membership() {
    use quilltap_core::db::group_character_members::{
        CreateOptions, GcmCreate, GroupCharacterMembersRepository,
    };
    let _serial = SERIAL.lock().await;
    let (_dir, data, _host) = setup_and_boot();
    let w = Writer::open_writable(&data.join("quilltap-mount-index.db"), PEPPER).unwrap();
    let repo = GroupCharacterMembersRepository::new(w.connection());
    let row = || GcmCreate {
        group_id: "g".into(),
        character_id: "c".into(),
    };
    let opts = |n| {
        let (id, ts) = opts_ts(n);
        CreateOptions {
            id,
            created_at: ts.clone(),
            updated_at: ts,
        }
    };
    repo.create(&row(), &opts(1)).unwrap();
    assert_refused(
        repo.create(&row(), &opts(2)),
        "UNIQUE constraint failed: group_character_members.groupId, \
         group_character_members.characterId",
    );
}

#[tokio::test]
async fn idx_group_doc_mount_links_group_mount_refuses_a_duplicate_link() {
    use quilltap_core::db::group_doc_mount_links::{
        CreateOptions, GdmlCreate, GroupDocMountLinksRepository,
    };
    let _serial = SERIAL.lock().await;
    let (_dir, data, _host) = setup_and_boot();
    let w = Writer::open_writable(&data.join("quilltap-mount-index.db"), PEPPER).unwrap();
    let repo = GroupDocMountLinksRepository::new(w.connection());
    let row = || GdmlCreate {
        group_id: "g".into(),
        mount_point_id: "m".into(),
    };
    let opts = |n| {
        let (id, ts) = opts_ts(n);
        CreateOptions {
            id,
            created_at: ts.clone(),
            updated_at: ts,
        }
    };
    repo.create(&row(), &opts(1)).unwrap();
    assert_refused(
        repo.create(&row(), &opts(2)),
        "UNIQUE constraint failed: group_doc_mount_links.groupId, \
         group_doc_mount_links.mountPointId",
    );
}

/// `idx_doc_mount_folders_mp_path` — the ONE name both v4 families make whose
/// texts differ in meaning: the migrations make it UNIQUE, the repository's
/// `onTableEnsured` plain, and a real v4 boot keeps the migration's. The
/// human's 2026-10-06 ruling: v5 provisions the UNIQUE one. Two folders under
/// the same parent with DIFFERENT names but the same `path` (so the nocase
/// `(mountPointId, parentId, name)` index cannot be what refuses it) — v4's
/// bytes measured on a copy of the real-boot instance. Red-first: before the
/// ruling the fresh instance carried the plain copy and ACCEPTED it.
#[tokio::test]
async fn idx_doc_mount_folders_mp_path_refuses_a_duplicate_path() {
    use quilltap_core::db::doc_mount_folders::{
        CreateOptions, DmfCreate, DocMountFoldersRepository,
    };
    let _serial = SERIAL.lock().await;
    let (_dir, data, _host) = setup_and_boot();
    let w = Writer::open_writable(&data.join("quilltap-mount-index.db"), PEPPER).unwrap();
    let repo = DocMountFoldersRepository::new(w.connection());
    let row = |name: &str| DmfCreate {
        mount_point_id: "m".into(),
        parent_id: None,
        name: name.into(),
        path: "/a/".into(),
    };
    let opts = |n| {
        let (id, ts) = opts_ts(n);
        CreateOptions {
            id,
            created_at: ts.clone(),
            updated_at: ts,
        }
    };
    repo.create(&row("a"), &opts(1)).unwrap();
    assert_refused(
        repo.create(&row("b"), &opts(2)),
        "UNIQUE constraint failed: doc_mount_folders.mountPointId, doc_mount_folders.path",
    );
}
