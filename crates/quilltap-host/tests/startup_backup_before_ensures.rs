//! P4.D288 (R-AD / O-9) — `STARTUP_BACKUP_BEFORE_ENSURES`, a RULED protective
//! divergence pinned both ways.
//!
//! v5's startup physical backup (P4.D259 — v4's PHASE 2 connect-time backup)
//! runs in `assemble` BEFORE `seed_built_ins`, where every boot ensure lives —
//! including this round's DESTRUCTIVE ones (the settings move's and the cursor
//! move's `DROP COLUMN`s, the vector-mirror `DROP TABLE`s). v4 runs its
//! migrations FIRST (`instrumentation.ts` PHASE 1, on the migration utilities'
//! own connection) and takes the startup backup inside the backend's
//! `connect()` (`backend.ts:561`), whose first caller is PHASE 1.1's
//! `repairTextEmbeddings` (`getDatabaseAsync`) — so a v4 startup backup taken on
//! the migrating boot already holds the migrated instance. (v4's DAILY backup,
//! PHASE 0.75, precedes the migrations on both sides — P4.D259.) v5 keeps the backup
//! ahead: the one boot that drops columns leaves a copy from before it. The
//! human may align it; until then this table records both sides and the test
//! fails if EITHER side moves (retire the row then).
//!
//! The v5 side reads `crates/quilltap-host/src/host.rs`; the v4 side reads
//! `instrumentation.ts` from `QT_V4_ROOT` (the round's pin) when set — without
//! it the v4 leg SKIPs loudly.
//!
//! Run:
//!   QT_V4_ROOT=/tmp/qt-v4-pin-p4d288-83334b9a5 \
//!     cargo test -p quilltap-host --test startup_backup_before_ensures -- --nocapture

/// `(name, v5 first, v5 second, v4 first, v4 second)` — each side's two
/// markers, in the order that side runs them.
const STARTUP_BACKUP_BEFORE_ENSURES: &[(&str, &str, &str, &str, &str)] = &[(
    "startup backup vs the boot ensures",
    "run_startup_backups_at_boot(db, data_dir, &self.display_zone);",
    "let ensure_failures = seed_built_ins(db)?;",
    "const migrationResult = await migrationRunner.runMigrations();",
    "// PHASE 1.1: Auto-repair TEXT embeddings",
)];

fn position(haystack: &str, needle: &str, side: &str) -> usize {
    assert_eq!(
        haystack.matches(needle).count(),
        1,
        "{side}: the marker {needle:?} must appear exactly once"
    );
    haystack.find(needle).unwrap()
}

#[test]
fn v5_backs_up_before_the_boot_ensures() {
    let host = std::fs::read_to_string(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/host.rs"),
    )
    .expect("read host.rs");
    for (name, first, second, _, _) in STARTUP_BACKUP_BEFORE_ENSURES {
        assert!(
            position(&host, first, "v5") < position(&host, second, "v5"),
            "{name}: v5 no longer backs up before its ensures — the divergence closed \
             (retire the STARTUP_BACKUP_BEFORE_ENSURES row)"
        );
    }
    eprintln!("OK: v5 backs up before its boot ensures.");
}

#[test]
fn v4_migrates_before_its_startup_backup() {
    let Ok(root) = std::env::var("QT_V4_ROOT") else {
        eprintln!("SKIP: set QT_V4_ROOT to the round's v4 pin (see header).");
        return;
    };
    let instrumentation =
        std::fs::read_to_string(std::path::Path::new(&root).join("instrumentation.ts"))
            .expect("read v4 instrumentation.ts");
    for (name, _, _, first, second) in STARTUP_BACKUP_BEFORE_ENSURES {
        assert!(
            position(&instrumentation, first, "v4") < position(&instrumentation, second, "v4"),
            "{name}: v4 no longer migrates before its connect-time backup — the divergence \
             moved (retire or re-rule the STARTUP_BACKUP_BEFORE_ENSURES row)"
        );
    }
    eprintln!("OK: v4 migrates before its startup backup (the recorded other side).");
}
