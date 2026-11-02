//! P4.D341 — the round's five boot ensures (the 17th–21st D23 moves) through
//! the REAL `Host::start`, capture-pinned with their silence legs.
//!
//! (a) A PRE-ROUND instance — a fresh provision with the seven columns
//!     DROPPED (`memories.keystone`, `chats.turnMode` / `storytellerInviteFilter`,
//!     `chat_messages.storytellerBid`, `chat_settings.customToolIntervention`,
//!     `roleplay_templates.unmarkedStyle` / `markdownAsterisks`), the
//!     `storyteller_rounds` table and the three new indexes DROPPED, the
//!     llm-logs `storytellerRoundId` column + index DROPPED, and the built-ins
//!     put back in their pre-round storage (no Novel row; `Standard`'s narration
//!     a bare `*`): ONE boot appends every column at the END with v4's clauses,
//!     creates the table in the MIGRATION's text (R-2's FK line) with both
//!     indexes, the partial index and the llm-logs index; the five INFOs land in
//!     v4's registration order (intervention — inside the 4.11 chain — then
//!     keystone, Storyteller, the llm-logs round id, unmarked); none of v4's
//!     migration lines appear; NOTHING is stamped; the PHASE 3.1 structural
//!     pass reports no problem (no `MISSING storytellerRoundId` — the llm-logs
//!     ensure runs inside `seed_built_ins`, before the pass); and
//!     `TEMPLATE_SEED_AFTER_ENSURES` holds — the built-in seed, now AFTER the
//!     unmarked ensure, inserts Novel and rewrites `Standard` to `"*"` and keeps
//!     `Quilltap RP`'s `["[","]"]` (without the move the seeder answered `no
//!     such column` on this instance and seeded nothing).
//! (b) The SECOND boot logs none of the five and moves no built-in cell; v4's
//!     seeder `$set`s `updatedAt` on every boot, and so does v5's (pinned).
//! (c) A fresh `setup` instance logs none on its first boot.
//! (d) The llm-logs partition DEGRADED (P4.159's garbage plant): the round-id
//!     step logs its DEBUG skip (R-17), the boot continues, the other four run.
//!
//! The chain runs inside `Db::write` (the writer thread), so the capture is
//! the process-wide subscriber, as `host_boot_p4d304_chat_settings_4_11`.

use std::path::Path;
use std::sync::{Arc, Mutex, OnceLock};

use quilltap_core::db::Writer;
use quilltap_core::services::provisioning::provision_fresh_instance;
use quilltap_core::test_support::CaptureLayer;
use quilltap_host::{Host, HostConfig};
use tracing_subscriber::layer::SubscriberExt;

const PEPPER: &str = "dGVzdHBlcHBlcnRlc3RwZXBwZXJ0ZXN0cGVwcGVyMDE=";
const MAIN: &str = "quilltap.db";
const LLM: &str = "quilltap-llm-logs.db";

static SERIAL: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

const INTERVENTION: &str = "INFO quilltap::boot Fitted the custom-tool intervention toggle migrationId=add-custom-tool-intervention-field-v1 columnAdded=true";
const KEYSTONE: &str = "INFO quilltap::boot Set the keystones in the Commonplace Book migrationId=add-memory-keystone-v1 columnAdded=true indexCreated=true";
const STORYTELLER: &str = "INFO quilltap::boot Cleared a dais in the Salon for the Storyteller migrationId=add-storyteller-mode-v1 columnsAdded=3 tableCreated=true";
const ROUND_ID: &str = "INFO quilltap::boot Taught the wire ledger to bundle each storyteller round under one ribbon migrationId=add-llm-logs-storyteller-round-id-column-v1 indexCreated=true";
const UNMARKED: &str = "INFO quilltap::boot Set aside a shelf for the unadorned novel migrationId=add-roleplay-template-unmarked-fields-v1 columnsAdded=2";
const SKIP: &str =
    "DEBUG quilltap::boot Storyteller round-id ensure skipped: llm-logs partition unavailable";

const IDS: [&str; 5] = [
    "add-custom-tool-intervention-field-v1",
    "add-memory-keystone-v1",
    "add-storyteller-mode-v1",
    "add-llm-logs-storyteller-round-id-column-v1",
    "add-roleplay-template-unmarked-fields-v1",
];

/// v4's migration lines (NO-PORT) — none may appear.
const V4_LINES: [&str; 9] = [
    "Added memories.keystone column",
    "Created idx_memories_keystone",
    "Added chats.turnMode column",
    "Added chats.storytellerInviteFilter column",
    "Added chat_messages.storytellerBid column",
    "Created storyteller_rounds table",
    "Added storytellerRoundId column to llm_logs",
    "Added unmarked-text columns to roleplay_templates table",
    "Added customToolIntervention column to chat_settings table",
];

/// Drop every move the round's re-dump made (the pre-round instance).
const PRE_ROUND_MAIN: &str = r#"
DROP INDEX "idx_memories_keystone";
ALTER TABLE "memories" DROP COLUMN "keystone";
ALTER TABLE "chats" DROP COLUMN "turnMode";
ALTER TABLE "chats" DROP COLUMN "storytellerInviteFilter";
ALTER TABLE "chat_messages" DROP COLUMN "storytellerBid";
ALTER TABLE "chat_settings" DROP COLUMN "customToolIntervention";
DELETE FROM "roleplay_templates" WHERE "isBuiltIn" = 1 AND "name" = 'Novel';
UPDATE "roleplay_templates" SET "narrationDelimiters" = '*' WHERE "isBuiltIn" = 1 AND "name" = 'Standard';
ALTER TABLE "roleplay_templates" DROP COLUMN "unmarkedStyle";
ALTER TABLE "roleplay_templates" DROP COLUMN "markdownAsterisks";
DROP TABLE "storyteller_rounds";
"#;
const PRE_ROUND_LLM: &str = r#"
DROP INDEX "idx_llm_logs_storytellerRoundId";
ALTER TABLE "llm_logs" DROP COLUMN "storytellerRoundId";
"#;

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

/// Boot once; answer the captured lines and the PHASE 3.1 problems.
fn boot(base: &Path) -> (Vec<String>, Vec<String>) {
    capture().lock().unwrap().clear();
    let host = Host::start(config(base)).expect("the boot");
    let problems = host.structural_problems();
    let lines = capture().lock().unwrap().clone();
    drop(host);
    (lines, problems)
}

/// The lines of the five steps (their INFOs + the DEBUG skip), in order.
fn ours(lines: &[String]) -> Vec<String> {
    lines
        .iter()
        .filter(|l| {
            IDS.iter()
                .any(|id| l.contains(&format!("migrationId={id}")))
                || l.starts_with(SKIP)
        })
        .cloned()
        .collect()
}

fn open(data: &Path, file: &str) -> Writer {
    Writer::open_writable(&data.join(file), PEPPER).unwrap()
}

fn sql_of(w: &Writer, name: &str) -> Option<String> {
    w.connection()
        .query_row(
            "SELECT sql FROM sqlite_master WHERE name = ?1",
            [name],
            |r| r.get(0),
        )
        .ok()
}

fn stamped(data: &Path) -> Vec<String> {
    let w = open(data, MAIN);
    if sql_of(&w, "migrations_state").is_none() {
        return Vec::new();
    }
    let mut stmt = w
        .connection()
        .prepare("SELECT id FROM migrations_state")
        .unwrap();
    let ids = stmt
        .query_map([], |r| r.get::<_, String>(0))
        .unwrap()
        .map(Result::unwrap)
        .filter(|id| IDS.contains(&id.as_str()))
        .collect();
    ids
}

/// The built-ins' `(name, narration, unmarkedStyle IS NULL, markdownAsterisks, updatedAt)`.
fn built_ins(data: &Path) -> Vec<(String, String, bool, i64, String)> {
    let w = open(data, MAIN);
    let mut stmt = w
        .connection()
        .prepare(
            "SELECT name, narrationDelimiters, unmarkedStyle IS NULL, markdownAsterisks, updatedAt \
             FROM roleplay_templates WHERE isBuiltIn = 1 ORDER BY name",
        )
        .unwrap();
    let rows = stmt
        .query_map([], |r| {
            Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?))
        })
        .unwrap()
        .map(Result::unwrap)
        .collect();
    rows
}

fn pre_round_instance() -> (tempfile::TempDir, std::path::PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let data = dir.path().join("data");
    std::fs::create_dir_all(&data).unwrap();
    provision_fresh_instance(&data, PEPPER).expect("provision");
    open(&data, MAIN)
        .connection()
        .execute_batch(PRE_ROUND_MAIN)
        .expect("the pre-round plant (main)");
    (dir, data)
}

fn assert_no_v4_lines(lines: &[String]) {
    for v4 in V4_LINES {
        assert!(
            !lines.iter().any(|l| l.contains(v4)),
            "v4's NO-PORT line {v4:?}: {lines:#?}"
        );
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_pre_round_instance_gains_every_move_in_v4s_order() {
    let _serial = SERIAL.lock().await;
    let (dir, data) = pre_round_instance();
    open(&data, LLM)
        .connection()
        .execute_batch(PRE_ROUND_LLM)
        .expect("the pre-round plant (llm-logs)");
    let pre: i64 = open(&data, MAIN)
        .connection()
        .query_row(
            "SELECT count(*) FROM roleplay_templates WHERE isBuiltIn = 1",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(pre, 2, "the plant removed Novel");

    // (a) the first boot.
    let (lines, problems) = boot(dir.path());
    assert_eq!(
        ours(&lines),
        [INTERVENTION, KEYSTONE, STORYTELLER, ROUND_ID, UNMARKED],
        "{lines:#?}"
    );
    assert_no_v4_lines(&lines);
    assert!(problems.is_empty(), "PHASE 3.1 found {problems:?}");
    assert!(
        !lines
            .iter()
            .any(|l| l.contains("Error seeding built-in roleplay templates")),
        "TEMPLATE_SEED_AFTER_ENSURES: {lines:#?}"
    );
    {
        let w = open(&data, MAIN);
        assert!(sql_of(&w, "memories")
            .unwrap()
            .ends_with(", \"keystone\" INTEGER NOT NULL DEFAULT 0)"));
        assert!(sql_of(&w, "chats").unwrap().ends_with(
            ", \"turnMode\" TEXT DEFAULT NULL, \"storytellerInviteFilter\" INTEGER DEFAULT 0)"
        ));
        assert!(sql_of(&w, "chat_messages")
            .unwrap()
            .ends_with(", \"storytellerBid\" TEXT DEFAULT NULL)"));
        assert!(sql_of(&w, "chat_settings")
            .unwrap()
            .ends_with(", \"customToolIntervention\" INTEGER DEFAULT 1)"));
        assert!(sql_of(&w, "roleplay_templates")
            .unwrap()
            .ends_with(", \"unmarkedStyle\" TEXT, \"markdownAsterisks\" INTEGER DEFAULT 1)"));
        let rounds = sql_of(&w, "storyteller_rounds").unwrap();
        assert!(
            rounds.contains(
                "FOREIGN KEY (\"chatId\") REFERENCES \"chats\"(\"id\") ON DELETE CASCADE"
            ),
            "R-2: {rounds}"
        );
        for idx in [
            "idx_memories_keystone",
            "idx_storyteller_rounds_chat",
            "idx_storyteller_rounds_createdAt",
        ] {
            assert!(sql_of(&w, idx).is_some(), "{idx}");
        }
        let l = open(&data, LLM);
        assert!(sql_of(&l, "llm_logs")
            .unwrap()
            .ends_with(", \"storytellerRoundId\" TEXT DEFAULT NULL)"));
        assert!(sql_of(&l, "idx_llm_logs_storytellerRoundId").is_some());
    }
    assert!(stamped(&data).is_empty(), "UNSTAMPED (R-9)");
    let first = built_ins(&data);
    let cells: Vec<(String, String, bool, i64)> = first
        .iter()
        .map(|(n, d, u, m, _)| (n.clone(), d.clone(), *u, *m))
        .collect();
    assert_eq!(
        cells,
        [
            ("Novel".to_string(), "null".to_string(), false, 0),
            (
                "Quilltap RP".to_string(),
                "[\"[\",\"]\"]".to_string(),
                true,
                1
            ),
            ("Standard".to_string(), "\"*\"".to_string(), true, 1),
        ],
        "the first-boot rewrite under the ensure"
    );

    // (b) the second boot: silent, no cell moves; `updatedAt` re-stamped
    // (v4's seeder `$set`s it every boot).
    std::thread::sleep(std::time::Duration::from_millis(5));
    let (lines, problems) = boot(dir.path());
    assert!(ours(&lines).is_empty(), "{lines:#?}");
    assert!(problems.is_empty());
    assert!(stamped(&data).is_empty());
    let second = built_ins(&data);
    for (a, b) in first.iter().zip(&second) {
        assert_eq!(
            (&a.0, &a.1, a.2, a.3),
            (&b.0, &b.1, b.2, b.3),
            "no cell moves"
        );
        assert_ne!(
            a.4, b.4,
            "{}: the seeder re-stamps updatedAt every boot, as v4's",
            a.0
        );
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_fresh_instance_boots_silent() {
    let _serial = SERIAL.lock().await;
    let dir = tempfile::tempdir().unwrap();
    let data = dir.path().join("data");
    std::fs::create_dir_all(&data).unwrap();
    provision_fresh_instance(&data, PEPPER).expect("provision");
    let (lines, problems) = boot(dir.path());
    assert!(ours(&lines).is_empty(), "{lines:#?}");
    assert!(problems.is_empty(), "{problems:?}");
    assert!(stamped(&data).is_empty());
    assert_eq!(built_ins(&data).len(), 3);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_degraded_llm_logs_partition_skips_the_round_id_step_only() {
    let _serial = SERIAL.lock().await;
    let (dir, data) = pre_round_instance();
    // P4.159's garbage plant: a NEW inode the open ladder cannot key.
    let garbage: Vec<u8> = (0..4608usize)
        .map(|i| ((i * 131 + 17) & 255) as u8)
        .collect();
    let swap = data.join(LLM).with_extension("swap");
    std::fs::write(&swap, garbage).unwrap();
    std::fs::rename(&swap, data.join(LLM)).unwrap();

    let (lines, _problems) = boot(dir.path());
    assert_eq!(
        ours(&lines),
        [INTERVENTION, KEYSTONE, STORYTELLER, SKIP, UNMARKED],
        "{lines:#?}"
    );
    assert_no_v4_lines(&lines);
    assert!(stamped(&data).is_empty());
}
