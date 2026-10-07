//! P4.160 — a v5 instance provisioned BEFORE P4.153 gains v4's whole
//! migration-created index family at its next boot (P4.153's OPEN item).
//!
//! P4.153 made `provision_fresh_instance` replay `migration_indexes.json`, so a
//! freshly `setup` instance carries the family (`host_boot_fresh_indexes`,
//! P4.164's file — mirrored here, never edited). An instance set up before
//! that round lacks 56 of the 60 statements after a boot (main 47 /
//! mount-index 4 / llm-logs 5 — the boot's own ensures already make three) and
//! holds `idx_doc_mount_folders_mp_path` PLAIN, which `builtin_mounts`'
//! `CREATE UNIQUE INDEX IF NOT EXISTS` cannot fix (a silent no-op behind the
//! plain copy of the same name). The backfill (`db::migration_index_family_
//! repair`, the last step of `seed_built_ins`) closes both.
//!
//! No committed pre-round instance exists, so every arm DERIVES one: provision
//! a fresh instance, `DROP` every name `migration_indexes.json` carries (the
//! three boot-ensure names too — the boot recreates them), and re-run
//! `fresh_schema.json`'s PLAIN `mp_path` statement. Duplicates are planted with
//! raw INSERTs after the drop, where no UNIQUE index can refuse them.
//!
//! The arms (the order's item 4): (a) one boot → `sqlite_master` byte-equal to
//! a fresh provision's, partition by partition; (b) the second boot creates
//! nothing (the INFO line's silence leg); (c) the per-chat plan; (d) the five
//! UNIQUE refusals of the fresh test, over the backfilled instance; (e) the
//! plain `mp_path` converted UNIQUE; (f) the connection-profile clash renamed
//! as v4's `add-connection-profile-unique-name-index-v1` renames it; (g)/(h) a
//! duplicate under a no-dedupe UNIQUE skips that ONE index with the ruled
//! v5-only WARN (R-A) and the boot succeeds; (i) NULL `mountPoint`s are not
//! duplicates; (j) a structural table the boot itself recreates is indexed the
//! same boot.
//!
//! Every boot logs from the writer and seed threads, which a thread-scoped
//! capture cannot see, so the shared `CaptureLayer` is the process-GLOBAL
//! default and the arms run one at a time behind `SERIAL`
//! (`host_boot_hardness`'s idiom).
//!
//! Red-first (P4.160's lane record): on unported `main` every arm but the
//! silence legs failed — see the record for the counts.
//!
//! Run:
//!   cargo test -p quilltap-host --test host_boot_backfilled_indexes

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};

use quilltap_core::db::Writer;
use quilltap_core::services::provisioning::provision_fresh_instance;
use quilltap_core::test_support::CaptureLayer;
use quilltap_host::{Host, HostConfig};
use serde_json::Value;
use tracing_subscriber::layer::SubscriberExt;

const PEPPER: &str = "dGVzdHBlcHBlcnRlc3RwZXBwZXJ0ZXN0cGVwcGVyMDE=";
const MAIN: &str = "quilltap.db";
const MOUNT: &str = "quilltap-mount-index.db";
const LLM: &str = "quilltap-llm-logs.db";
const PARTITIONS: [(&str, &str); 3] = [("main", MAIN), ("mountIndex", MOUNT), ("llmLogs", LLM)];
const MP_PATH: &str = "idx_doc_mount_folders_mp_path";

/// Arms boot one at a time: the capture is process-global.
static SERIAL: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

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
    config
}

fn provisioning_artifact(name: &str) -> Value {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../quilltap-core/src/services/provisioning")
        .join(name);
    serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap()
}

fn statements(artifact: &str, partition: &str) -> Vec<String> {
    provisioning_artifact(artifact)[partition]
        .as_array()
        .unwrap()
        .iter()
        .map(|s| s.as_str().unwrap().to_string())
        .collect()
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
    ["fresh_schema.json", "migration_indexes.json"]
        .iter()
        .flat_map(|a| statements(a, partition))
        .filter_map(|sql| index_name(&sql))
        .collect()
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

fn index_sql(data: &Path, file: &str, name: &str) -> Option<String> {
    master(data, file)
        .into_iter()
        .find(|(t, n, _)| t == "index" && n == name)
        .and_then(|(_, _, sql)| sql)
}

fn exec(data: &Path, file: &str, sql: &str) {
    let w = Writer::open_writable(&data.join(file), PEPPER).unwrap();
    w.connection()
        .execute_batch(sql)
        .unwrap_or_else(|e| panic!("plant {sql:?}: {e}"));
}

/// A fresh provision with the migration family taken away again — the shape
/// every instance `setup` before P4.153 has on disk.
fn derive_pre_round(data: &Path) {
    provision_fresh_instance(data, PEPPER).expect("provision");
    for (partition, file) in PARTITIONS {
        let drops: String = statements("migration_indexes.json", partition)
            .iter()
            .map(|sql| format!("DROP INDEX \"{}\";", index_name(sql).unwrap()))
            .collect();
        exec(data, file, &drops);
    }
    // `fresh_schema.json`'s PLAIN copy — what the provisioner made before the
    // 2026-10-06 ruling skipped it in favour of the artifact's UNIQUE one.
    let plain = statements("fresh_schema.json", "mountIndex")
        .into_iter()
        .find(|sql| index_name(sql).as_deref() == Some(MP_PATH))
        .unwrap();
    assert!(plain.starts_with("CREATE INDEX "), "{plain}");
    exec(data, MOUNT, &plain);
}

struct Booted {
    _dir: tempfile::TempDir,
    data: PathBuf,
    result: Result<Host, String>,
    lines: Vec<String>,
}

impl Booted {
    fn host(&self) -> &Host {
        match &self.result {
            Ok(host) => host,
            Err(e) => panic!(
                "the boot FAILED — the backfill must never kill it: {e}\n{}",
                self.lines.join("\n")
            ),
        }
    }

    /// Exactly one captured line equal to `line`.
    fn assert_line(&self, line: &str) {
        let hits = self.lines.iter().filter(|l| l.as_str() == line).count();
        assert_eq!(
            hits,
            1,
            "expected exactly one line {line:?}; captured:\n{}",
            self.lines.join("\n")
        );
    }

    fn lines_containing(&self, needle: &str) -> Vec<String> {
        self.lines
            .iter()
            .filter(|l| l.contains(needle))
            .cloned()
            .collect()
    }

    /// Drop this boot's host and boot the SAME instance again (the
    /// `host_boot_hardness` reboot idiom).
    fn reboot(self) -> Booted {
        let Booted {
            _dir, data, result, ..
        } = self;
        drop(result);
        boot_dir(_dir, data)
    }
}

fn boot_dir(dir: tempfile::TempDir, data: PathBuf) -> Booted {
    capture().lock().unwrap().clear();
    let result = Host::start(config(dir.path())).map_err(|e| e.to_string());
    let lines = capture().lock().unwrap().clone();
    Booted {
        _dir: dir,
        data,
        result,
        lines,
    }
}

/// Derive a pre-round instance, run `plants` on it, and boot it once.
fn boot_pre_round(plants: &[(&str, &str)]) -> Booted {
    let dir = tempfile::tempdir().unwrap();
    let data = dir.path().join("data");
    std::fs::create_dir_all(&data).unwrap();
    derive_pre_round(&data);
    for (file, sql) in plants {
        exec(&data, file, sql);
    }
    boot_dir(dir, data)
}

/// A freshly provisioned instance, booted once — the comparand.
fn boot_fresh() -> Booted {
    let dir = tempfile::tempdir().unwrap();
    let data = dir.path().join("data");
    std::fs::create_dir_all(&data).unwrap();
    provision_fresh_instance(&data, PEPPER).expect("provision");
    boot_dir(dir, data)
}

/// The boot's `Backfilled migration-created indexes` lines (v5-only, R-A).
const BACKFILLED: &str = "Backfilled migration-created indexes";
const SKIPPED_UNIQUE: &str = "Skipped a unique index backfill: duplicate rows present";

/// `EXPLAIN QUERY PLAN` details, joined.
fn plan(data: &Path, sql: &str, args: &[&str]) -> String {
    let w = Writer::open_writable(&data.join(MAIN), PEPPER).unwrap();
    let mut stmt = w
        .connection()
        .prepare(&format!("EXPLAIN QUERY PLAN {sql}"))
        .unwrap();
    let details: Vec<String> = stmt
        .query_map(rusqlite::params_from_iter(args.iter()), |r| {
            r.get::<_, String>(3)
        })
        .unwrap()
        .map(Result::unwrap)
        .collect();
    details.join(" | ")
}

fn assert_master_equals_fresh(booted: &Booted, fresh: &Booted, skip: &[&str]) {
    for (partition, file) in PARTITIONS {
        let ours: Vec<_> = master(&booted.data, file)
            .into_iter()
            .filter(|(_, n, _)| !skip.contains(&n.as_str()))
            .collect();
        let theirs: Vec<_> = master(&fresh.data, file)
            .into_iter()
            .filter(|(_, n, _)| !skip.contains(&n.as_str()))
            .collect();
        let only_ours: Vec<_> = ours.iter().filter(|r| !theirs.contains(r)).collect();
        let only_theirs: Vec<_> = theirs.iter().filter(|r| !ours.contains(r)).collect();
        assert!(
            only_ours.is_empty() && only_theirs.is_empty(),
            "{partition}: sqlite_master differs from a fresh provision's after one boot\n  \
             only on the backfilled instance: {only_ours:#?}\n  only on the fresh one: \
             {only_theirs:#?}\n{}",
            booted.lines_containing("quilltap::boot").join("\n")
        );
    }
}

// ───────────── (a) + (b): the family, byte-equal; the second boot silent ─────────────

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn one_boot_gives_a_pre_round_instance_a_fresh_instances_sqlite_master() {
    let _serial = SERIAL.lock().await;
    let fresh = boot_fresh();
    fresh.host();
    let booted = boot_pre_round(&[]);
    booted.host();
    let mut missing = Vec::new();
    for (partition, file) in PARTITIONS {
        let have: BTreeSet<String> = master(&booted.data, file)
            .into_iter()
            .filter(|(t, _, _)| t == "index")
            .map(|(_, n, _)| n)
            .collect();
        missing.extend(
            promised(partition)
                .into_iter()
                .filter(|n| !have.contains(n))
                .map(|n| format!("{partition}:{n}")),
        );
    }
    assert!(
        missing.is_empty(),
        "{} promised index(es) absent after one boot:\n{}",
        missing.len(),
        missing.join("\n")
    );
    assert_master_equals_fresh(&booted, &fresh, &[]);
    // The INFO leg: main makes 47 (the three boot ensures already made
    // `idx_files_generationKey`, `idx_folders_userId_projectId_path` and
    // `idx_help_doc_chunks_docId`), the mount index 4 + the converted
    // `mp_path`, the LLM logs 5.
    booted.assert_line(
        "INFO quilltap::boot Backfilled migration-created indexes partition=main created=47 skipped=0",
    );
    booted.assert_line(
        "INFO quilltap::boot Backfilled migration-created indexes partition=mountIndex created=5 skipped=0",
    );
    booted.assert_line(
        "INFO quilltap::boot Backfilled migration-created indexes partition=llmLogs created=5 skipped=0",
    );
}

/// §S.2 of the `94fbb1ae3` boot-hardness round — the UNION of P4.159 and
/// P4.160: a pre-round instance whose mount index is GARBAGE boots DEGRADED
/// (P4.159) and the backfill still runs on the partitions it HAS — main makes
/// its 47 and the LLM logs their 5 — while the degraded mount index (no writer)
/// is skipped with no line and no error.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_degraded_mount_index_skips_its_statements_and_the_others_still_backfill() {
    let _serial = SERIAL.lock().await;
    let dir = tempfile::tempdir().unwrap();
    let data = dir.path().join("data");
    std::fs::create_dir_all(&data).unwrap();
    derive_pre_round(&data);
    std::fs::write(data.join(MOUNT), vec![0x5a_u8; 8192]).expect("plant a garbage mount index");
    let booted = boot_dir(dir, data);
    booted.host();
    assert!(
        !booted.lines_containing("entering degraded mode").is_empty(),
        "the mount index degraded (P4.159's open); captured:\n{}",
        booted.lines.join("\n")
    );
    booted.assert_line(
        "INFO quilltap::boot Backfilled migration-created indexes partition=main created=47 skipped=0",
    );
    booted.assert_line(
        "INFO quilltap::boot Backfilled migration-created indexes partition=llmLogs created=5 skipped=0",
    );
    for needle in [
        "partition=mountIndex",
        "Failed to backfill a migration-created index",
        "Migration index backfill failed",
    ] {
        assert!(
            booted.lines_containing(needle).is_empty(),
            "no {needle:?} line over a degraded mount index; captured:\n{}",
            booted.lines.join("\n")
        );
    }
    let have: BTreeSet<String> = master(&booted.data, MAIN)
        .into_iter()
        .filter(|(t, _, _)| t == "index")
        .map(|(_, n, _)| n)
        .collect();
    let missing: Vec<String> = promised("main").into_iter().filter(|n| !have.contains(n)).collect();
    assert!(missing.is_empty(), "main still lacks {missing:?}");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn the_second_boot_creates_nothing_and_logs_nothing() {
    let _serial = SERIAL.lock().await;
    let first = boot_pre_round(&[]);
    first.host();
    let after_first: Vec<_> = PARTITIONS
        .iter()
        .map(|(_, f)| master(&first.data, f))
        .collect();
    let second = first.reboot();
    second.host();
    let after_second: Vec<_> = PARTITIONS
        .iter()
        .map(|(_, f)| master(&second.data, f))
        .collect();
    assert_eq!(
        after_first, after_second,
        "a second boot changed sqlite_master"
    );
    let infos = second.lines_containing(BACKFILLED);
    assert!(infos.is_empty(), "the second boot logged {infos:?}");
}

/// The silence leg on a FRESH instance: the provisioner already made the
/// family, so the backfill creates nothing and logs nothing.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_fresh_instance_gives_the_backfill_nothing_to_do() {
    let _serial = SERIAL.lock().await;
    let fresh = boot_fresh();
    fresh.host();
    let infos = fresh.lines_containing(BACKFILLED);
    assert!(infos.is_empty(), "a fresh boot logged {infos:?}");
}

// ───────────── (c): the plan the restore depends on ─────────────

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn the_per_chat_message_read_uses_the_chat_id_index_after_one_boot() {
    let _serial = SERIAL.lock().await;
    let booted = boot_pre_round(&[]);
    booted.host();
    let chat = "00000000-0000-0000-0000-000000000001";
    let messages = plan(
        &booted.data,
        "SELECT id FROM chat_messages WHERE chatId = ?1 ORDER BY createdAt ASC",
        &[chat],
    );
    assert!(
        messages.contains("USING INDEX idx_chat_messages_chatId"),
        "get_messages plan: {messages}"
    );
    let pending = plan(
        &booted.data,
        "SELECT id FROM chat_informs WHERE chatId = ?1 AND participantId = ?2",
        &[chat, "p"],
    );
    assert!(
        pending.contains("USING INDEX idx_chat_informs_pending"),
        "find_pending_for_participant plan: {pending}"
    );
}

// ───────────── (d) + (e): the UNIQUE refusals over the backfilled instance ─────────────
//
// The fresh test's five arms (`host_boot_fresh_indexes`), re-run after ONE
// boot of a derived pre-round instance: each repository's RAW `create` twice,
// the second refused with the bytes v4's SQLite gives the same duplicate.

fn opts_ts(n: u8) -> (String, String) {
    (
        format!("00000000-0000-4000-8000-0000000000{n:02}"),
        "2026-10-07T00:00:00.000Z".to_string(),
    )
}

fn assert_refused(result: Result<(), quilltap_core::db::DbError>, v4_bytes: &str) {
    match result {
        Ok(()) => panic!("the duplicate was ACCEPTED; v4 refuses it with {v4_bytes:?}"),
        Err(e) => assert_eq!(quilltap_core::db::fallback::error_text(&e), v4_bytes),
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn the_backfilled_unique_indexes_refuse_v4s_duplicates() {
    use quilltap_core::db::{
        chat_documents, connection_profiles, doc_mount_folders, group_character_members,
        group_doc_mount_links,
    };
    let _serial = SERIAL.lock().await;
    let booted = boot_pre_round(&[]);
    booted.host();

    let main = Writer::open_writable(&booted.data.join(MAIN), PEPPER).unwrap();
    let mount = Writer::open_writable(&booted.data.join(MOUNT), PEPPER).unwrap();

    let docs = chat_documents::ChatDocumentsRepository::new(main.connection());
    let doc = || chat_documents::CdCreate {
        chat_id: "c".into(),
        file_path: "f.md".into(),
        scope: "project".into(),
        mount_point: Some("mp".into()),
        display_title: None,
        is_active: false,
    };
    let doc_opts = |n| {
        let (id, ts) = opts_ts(n);
        chat_documents::CreateOptions {
            id,
            created_at: ts.clone(),
            updated_at: ts,
        }
    };
    docs.create(&doc(), &doc_opts(1)).unwrap();
    assert_refused(
        docs.create(&doc(), &doc_opts(2)),
        "UNIQUE constraint failed: chat_documents.chatId, chat_documents.filePath, \
         chat_documents.scope, chat_documents.mountPoint",
    );

    let profiles = connection_profiles::ConnectionProfilesRepository::new(main.connection());
    let profile = |name: &str| connection_profiles::CpCreate {
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
    let profile_opts = |n| {
        let (id, ts) = opts_ts(n);
        connection_profiles::CreateOptions {
            id,
            created_at: ts.clone(),
            updated_at: ts,
        }
    };
    profiles
        .create(&profile("Alpha"), &profile_opts(3))
        .unwrap();
    assert_refused(
        profiles.create(&profile("  alpha "), &profile_opts(4)),
        "UNIQUE constraint failed: index 'idx_connection_profiles_userId_name'",
    );

    let members = group_character_members::GroupCharacterMembersRepository::new(mount.connection());
    let member = || group_character_members::GcmCreate {
        group_id: "g".into(),
        character_id: "c".into(),
    };
    let member_opts = |n| {
        let (id, ts) = opts_ts(n);
        group_character_members::CreateOptions {
            id,
            created_at: ts.clone(),
            updated_at: ts,
        }
    };
    members.create(&member(), &member_opts(5)).unwrap();
    assert_refused(
        members.create(&member(), &member_opts(6)),
        "UNIQUE constraint failed: group_character_members.groupId, \
         group_character_members.characterId",
    );

    let links = group_doc_mount_links::GroupDocMountLinksRepository::new(mount.connection());
    let link = || group_doc_mount_links::GdmlCreate {
        group_id: "g".into(),
        mount_point_id: "m".into(),
    };
    let link_opts = |n| {
        let (id, ts) = opts_ts(n);
        group_doc_mount_links::CreateOptions {
            id,
            created_at: ts.clone(),
            updated_at: ts,
        }
    };
    links.create(&link(), &link_opts(7)).unwrap();
    assert_refused(
        links.create(&link(), &link_opts(8)),
        "UNIQUE constraint failed: group_doc_mount_links.groupId, \
         group_doc_mount_links.mountPointId",
    );

    // (e) — the plain `mp_path` the derived instance carried is UNIQUE now.
    let folders = doc_mount_folders::DocMountFoldersRepository::new(mount.connection());
    let folder = |name: &str| doc_mount_folders::DmfCreate {
        mount_point_id: "m".into(),
        parent_id: None,
        name: name.into(),
        path: "/a/".into(),
    };
    let folder_opts = |n| {
        let (id, ts) = opts_ts(n);
        doc_mount_folders::CreateOptions {
            id,
            created_at: ts.clone(),
            updated_at: ts,
        }
    };
    folders.create(&folder("a"), &folder_opts(9)).unwrap();
    assert_refused(
        folders.create(&folder("b"), &folder_opts(10)),
        "UNIQUE constraint failed: doc_mount_folders.mountPointId, doc_mount_folders.path",
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn the_plain_mp_path_index_is_converted_unique_in_one_boot() {
    let _serial = SERIAL.lock().await;
    let dir = tempfile::tempdir().unwrap();
    let data = dir.path().join("data");
    std::fs::create_dir_all(&data).unwrap();
    derive_pre_round(&data);
    assert_eq!(
        index_sql(&data, MOUNT, MP_PATH).as_deref(),
        Some("CREATE INDEX \"idx_doc_mount_folders_mp_path\" ON \"doc_mount_folders\" (\"mountPointId\", \"path\")"),
        "the derived instance carries the PLAIN copy"
    );
    let booted = boot_dir(dir, data);
    booted.host();
    let want = statements("migration_indexes.json", "mountIndex")
        .into_iter()
        .find(|sql| index_name(sql).as_deref() == Some(MP_PATH))
        .unwrap();
    assert_eq!(
        index_sql(&booted.data, MOUNT, MP_PATH),
        Some(want),
        "the UNIQUE copy, byte-equal to the artifact"
    );
}

// ───────────── (f): the connection-profile clash, renamed as v4 renames it ─────────────

const USER: &str = "ffffffff-ffff-ffff-ffff-ffffffffffff";

fn profile_insert(id: &str, user: &str, name: &str, created_at: &str) -> String {
    format!(
        "INSERT INTO connection_profiles (id, userId, name, provider, modelName, createdAt, \
         updatedAt) VALUES ('{id}', '{user}', '{}', 'OPENAI', 'm', '{created_at}', '{created_at}');",
        name.replace('\'', "''")
    )
}

fn profile_names(data: &Path) -> Vec<(String, String, String)> {
    let w = Writer::open_writable(&data.join(MAIN), PEPPER).unwrap();
    let mut stmt = w
        .connection()
        .prepare("SELECT id, name, updatedAt FROM connection_profiles ORDER BY id")
        .unwrap();
    let rows = stmt
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))
        .unwrap()
        .map(Result::unwrap)
        .collect();
    rows
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_connection_profile_name_clash_is_renamed_as_v4_renames_it() {
    let _serial = SERIAL.lock().await;
    const SEEDED: &str = "2026-01-01T00:00:00.000Z";
    let plants = [
        profile_insert("p1", USER, "Alpha", "2026-01-01T00:00:00.000Z"),
        profile_insert("p2", USER, " alpha ", "2026-01-02T00:00:00.000Z"),
        profile_insert("p3", USER, "ALPHA ", "2026-01-03T00:00:00.000Z"),
        // Another user's "Alpha" is not a clash (the index is per user).
        profile_insert("p4", "u2", "Alpha", "2026-01-04T00:00:00.000Z"),
    ];
    let plants: Vec<(&str, &str)> = plants.iter().map(|s| (MAIN, s.as_str())).collect();
    let booted = boot_pre_round(&plants);
    booted.host();
    let names = profile_names(&booted.data);
    let by_id = |id: &str| names.iter().find(|(i, _, _)| i == id).unwrap().clone();
    // The oldest keeps its name; the later collisions gain v4's suffix, trimmed.
    assert_eq!(by_id("p1").1, "Alpha");
    assert_eq!(by_id("p2").1, "alpha (2)");
    assert_eq!(by_id("p3").1, "ALPHA (3)");
    assert_eq!(by_id("p4").1, "Alpha");
    // Only the renamed rows are re-stamped.
    assert_eq!(by_id("p1").2, SEEDED);
    assert_eq!(by_id("p4").2, "2026-01-04T00:00:00.000Z");
    assert_ne!(by_id("p2").2, "2026-01-02T00:00:00.000Z");
    assert_ne!(by_id("p3").2, "2026-01-03T00:00:00.000Z");
    booted.assert_line(
        "DEBUG quilltap::boot Renamed duplicate connection-profile name \
         context=migration.add-connection-profile-unique-name-index profileId=p2 \
         from= alpha  to=alpha (2)",
    );
    booted.assert_line(
        "DEBUG quilltap::boot Renamed duplicate connection-profile name \
         context=migration.add-connection-profile-unique-name-index profileId=p3 \
         from=ALPHA  to=ALPHA (3)",
    );
    booted.assert_line(
        "INFO quilltap::boot Enforced unique connection-profile names \
         context=migration.add-connection-profile-unique-name-index profileCount=4 renamed=2",
    );
    assert_eq!(
        index_sql(&booted.data, MAIN, "idx_connection_profiles_userId_name").as_deref(),
        Some(
            "CREATE UNIQUE INDEX \"idx_connection_profiles_userId_name\" ON \
             \"connection_profiles\" (\"userId\", lower(trim(\"name\")))"
        )
    );
    assert!(booted.lines_containing(SKIPPED_UNIQUE).is_empty());
}

/// v4's trim quirk: a name with surrounding whitespace is renamed even with no
/// clash at all (`uniqueName !== profile.name`), and a clean set is untouched.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_lone_padded_profile_name_is_trimmed_and_a_clean_set_untouched() {
    let _serial = SERIAL.lock().await;
    let plants = [
        profile_insert("p1", USER, " Solo ", "2026-01-01T00:00:00.000Z"),
        profile_insert("p2", USER, "Clean", "2026-01-02T00:00:00.000Z"),
    ];
    let plants: Vec<(&str, &str)> = plants.iter().map(|s| (MAIN, s.as_str())).collect();
    let booted = boot_pre_round(&plants);
    booted.host();
    let names: Vec<String> = profile_names(&booted.data)
        .into_iter()
        .map(|(_, n, _)| n)
        .collect();
    assert_eq!(names, ["Solo", "Clean"]);
    booted.assert_line(
        "INFO quilltap::boot Enforced unique connection-profile names \
         context=migration.add-connection-profile-unique-name-index profileCount=2 renamed=1",
    );
}

// ───────────── (g) – (i): the no-dedupe UNIQUEs (R-A) ─────────────

fn chat_document_insert(id: &str, mount_point: Option<&str>) -> String {
    format!(
        "INSERT INTO chat_documents (id, chatId, filePath, scope, mountPoint, createdAt, \
         updatedAt) VALUES ('{id}', 'c', 'f.md', 'project', {}, 'x', 'x');",
        mount_point.map_or("NULL".to_string(), |m| format!("'{m}'"))
    )
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_chat_documents_duplicate_skips_that_one_index_and_the_boot_succeeds() {
    let _serial = SERIAL.lock().await;
    let fresh = boot_fresh();
    fresh.host();
    let plants = [
        chat_document_insert("d1", Some("mp")),
        chat_document_insert("d2", Some("mp")),
        chat_document_insert("d3", Some("mp")),
    ];
    let plants: Vec<(&str, &str)> = plants.iter().map(|s| (MAIN, s.as_str())).collect();
    let booted = boot_pre_round(&plants);
    booted.host();
    booted.assert_line(
        "WARN quilltap::boot Skipped a unique index backfill: duplicate rows present \
         index=idx_chat_documents_unique table=chat_documents duplicates=2",
    );
    booted.assert_line(
        "INFO quilltap::boot Backfilled migration-created indexes partition=main created=46 skipped=1",
    );
    assert_eq!(
        index_sql(&booted.data, MAIN, "idx_chat_documents_unique"),
        None
    );
    // Every OTHER index is there, byte-equal.
    assert_master_equals_fresh(&booted, &fresh, &["idx_chat_documents_unique"]);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn group_and_folder_duplicates_skip_their_indexes_and_leave_mp_path_plain() {
    let _serial = SERIAL.lock().await;
    let fresh = boot_fresh();
    fresh.host();
    let plants = [
        (
            MOUNT,
            "INSERT INTO group_character_members (id, groupId, characterId, createdAt, updatedAt) \
             VALUES ('m1', 'g', 'c', 'x', 'x'), ('m2', 'g', 'c', 'x', 'x');",
        ),
        // Same `(mountPointId, path)`, different names — so the nocase
        // `(mountPointId, parentId, name)` index cannot be what refuses it —
        // under a REAL built-in store: the boot's orphan reaper deletes
        // folders whose mount point is gone, before the backfill runs.
        (
            MOUNT,
            "INSERT INTO doc_mount_folders (id, mountPointId, parentId, name, path, createdAt, \
             updatedAt) SELECT 'f1', id, NULL, 'qt-a', 'qt-dup', 'x', 'x' FROM \
             (SELECT id FROM doc_mount_points ORDER BY id LIMIT 1) \
             UNION ALL SELECT 'f2', id, NULL, 'qt-b', 'qt-dup', 'x', 'x' FROM \
             (SELECT id FROM doc_mount_points ORDER BY id LIMIT 1);",
        ),
    ];
    let booted = boot_pre_round(&plants);
    booted.host();
    booted.assert_line(
        "WARN quilltap::boot Skipped a unique index backfill: duplicate rows present \
         index=idx_doc_mount_folders_mp_path table=doc_mount_folders duplicates=1",
    );
    booted.assert_line(
        "WARN quilltap::boot Skipped a unique index backfill: duplicate rows present \
         index=idx_group_character_members_group_char table=group_character_members duplicates=1",
    );
    booted.assert_line(
        "INFO quilltap::boot Backfilled migration-created indexes partition=mountIndex created=3 skipped=2",
    );
    assert_eq!(
        index_sql(&booted.data, MOUNT, MP_PATH).as_deref(),
        Some("CREATE INDEX \"idx_doc_mount_folders_mp_path\" ON \"doc_mount_folders\" (\"mountPointId\", \"path\")"),
        "a duplicate path leaves the PLAIN copy in place — never dropped"
    );
    assert_eq!(
        index_sql(
            &booted.data,
            MOUNT,
            "idx_group_character_members_group_char"
        ),
        None
    );
    assert_master_equals_fresh(
        &booted,
        &fresh,
        &[MP_PATH, "idx_group_character_members_group_char"],
    );
}

/// SQLite treats NULLs as DISTINCT inside a UNIQUE index (a `GROUP BY` would
/// call them equal), so two rows differing only in a NULL `mountPoint` are no
/// duplicate and the index is made.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn null_mount_points_are_not_duplicates() {
    let _serial = SERIAL.lock().await;
    let plants = [
        chat_document_insert("d1", None),
        chat_document_insert("d2", None),
    ];
    let plants: Vec<(&str, &str)> = plants.iter().map(|s| (MAIN, s.as_str())).collect();
    let booted = boot_pre_round(&plants);
    booted.host();
    assert!(booted.lines_containing(SKIPPED_UNIQUE).is_empty());
    assert!(index_sql(&booted.data, MAIN, "idx_chat_documents_unique").is_some());
}

// ───────────── (j): a structural table the same boot recreates ─────────────

/// `create_missing_structural_tables` recreates an absent link table from
/// `fresh_schema.json` alone (no migration-family index); the backfill runs
/// AFTER it in `seed_built_ins`, so the table is indexed the same boot.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_structural_table_the_boot_recreates_is_indexed_the_same_boot() {
    let _serial = SERIAL.lock().await;
    let fresh = boot_fresh();
    fresh.host();
    let booted = boot_pre_round(&[(MOUNT, "DROP TABLE group_doc_mount_links;")]);
    booted.host();
    assert_master_equals_fresh(&booted, &fresh, &[]);
    for name in [
        "idx_group_doc_mount_links_group_mount",
        "idx_group_doc_mount_links_mountPointId",
    ] {
        assert!(
            index_sql(&booted.data, MOUNT, name).is_some(),
            "{name} absent"
        );
    }
}
