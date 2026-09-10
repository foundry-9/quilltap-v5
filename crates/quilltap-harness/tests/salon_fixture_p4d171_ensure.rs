//! P4.D173 — emit a P4.D171-ensured copy of the committed `salon-{main,mount}.db`
//! pair for the ORACLE side of the salon mutation families.
//!
//! ## Why this exists
//!
//! P4.D171 widened `chats_read`'s SELECT and `chats_messages`'s INSERT with the
//! two `78b381a96`-round columns, and the committed `salon-*.db` pair predates
//! both. On a real instance the boot ensures add them before any Salon read or
//! write runs, so the v5-side tests copy the fixture and run
//! `test_support::ensure_p4d171_columns` on the copy (the `salon_reads`
//! precedent). That is enough for a READ family.
//!
//! It is NOT enough for `salon_mutations` / `salon_swipe_generate`, which
//! compare whole-table DUMPS: v4 would be dumping the vintage column set while
//! v5 dumps the healed one, and every row diverges on the column list alone. So
//! the oracle needs a fixture with the columns too — and the least invasive way
//! to get one is to heal a COPY with the very same ensures, rather than
//! regenerate or rewrite a committed pair that three families and the Playwright
//! seeder read.
//!
//! ## How to use it (this is a recipe step, not a test)
//!
//! ```text
//! cargo test -p quilltap-harness --test salon_fixture_p4d171_ensure -- --ignored
//! # → /tmp/qt-salon-p4d171-main.db + /tmp/qt-salon-p4d171-mount.db
//! ```
//!
//! then point the oracle's `QT_FIXTURE_SALON_MAIN` / `QT_FIXTURE_SALON_MOUNT` at
//! those two paths. `#[ignore]` because it writes files and asserts nothing about
//! the port — it is a fixture step that happens to live in a test binary, the
//! only place `test_support` is reachable.
//!
//! **Spotted, not mine (for the unifier):** the same vintage gap makes
//! `salon_skip_equivalence` fail the moment its oracle is supplied — its seed
//! write names `routeTrail` on a table that has not got it. That file is
//! P4.D172's this round; the one-line fix is the same
//! `ensure_p4d171_columns` call on its fixture copy. P4.D171's own gate never
//! saw any of this because all three families SKIP without their oracle vars.

use std::path::PathBuf;

const PEPPER: &str = "dGVzdHBlcHBlcnRlc3RwZXBwZXJ0ZXN0cGVwcGVyMDE=";

fn fixtures_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../quilltap-web/tests/fixtures")
}

#[test]
#[ignore = "a fixture step, not a check — see the module docs"]
fn emit_a_p4d171_ensured_salon_fixture_pair() {
    for (src, dst) in [
        ("salon-main.db", "/tmp/qt-salon-p4d171-main.db"),
        ("salon-mount.db", "/tmp/qt-salon-p4d171-mount.db"),
    ] {
        let _ = std::fs::remove_file(dst);
        std::fs::copy(fixtures_dir().join(src), dst).unwrap_or_else(|e| panic!("copy {src}: {e}"));
    }
    // Only the main partition carries `chats` / `chat_messages`; the ensures
    // no-op on a table-less partition, so running them on both is harmless and
    // keeps the two paths symmetrical.
    for dst in [
        "/tmp/qt-salon-p4d171-main.db",
        "/tmp/qt-salon-p4d171-mount.db",
    ] {
        let w = quilltap_core::db::Writer::open_writable(std::path::Path::new(dst), PEPPER)
            .unwrap_or_else(|e| panic!("open {dst}: {e}"));
        quilltap_core::test_support::ensure_p4d171_columns(w.connection());
    }
    // Prove the heal landed, so a silent no-op cannot masquerade as success.
    let w = quilltap_core::db::Writer::open_writable(
        std::path::Path::new("/tmp/qt-salon-p4d171-main.db"),
        PEPPER,
    )
    .unwrap();
    for (table, column) in [
        ("chat_messages", "routeTrail"),
        ("chats", "cycleOrderParticipantIds"),
    ] {
        let found: i64 = w
            .connection()
            .query_row(
                &format!(
                    "SELECT COUNT(*) FROM pragma_table_info('{table}') WHERE name = '{column}'"
                ),
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(found, 1, "{table}.{column} must exist after the ensure");
    }
    println!("wrote /tmp/qt-salon-p4d171-{{main,mount}}.db");
}
