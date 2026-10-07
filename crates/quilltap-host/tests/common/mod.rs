//! Shared helpers for the host boot tests that plan PRODUCTION SQL
//! (P4.164's statement trace, moved here at the `94fbb1ae3` boot-hardness
//! unification so `host_boot_fresh_indexes` and `host_boot_backfilled_indexes`
//! plan the same captured statements — the latter had kept hand-copied SQL).
#![allow(dead_code)]

/// `EXPLAIN QUERY PLAN` details of `sql` on `conn`, joined. The args bind
/// the statement's own `?N` parameters (only as many as it declares).
pub fn plan(conn: &rusqlite::Connection, sql: &str, args: &[&str]) -> String {
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
pub fn traced<T>(conn: &rusqlite::Connection, read: impl FnOnce() -> T) -> (T, Vec<String>) {
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
    // Unregisters on the way out — on a panicking `read` too, before `seen`
    // drops (locals drop in reverse order), so the callback never outlives it.
    struct Unregister<'c>(&'c rusqlite::Connection);
    impl Drop for Unregister<'_> {
        fn drop(&mut self) {
            // SAFETY: clearing the trace on a live handle.
            let rc =
                unsafe { ffi::sqlite3_trace_v2(self.0.handle(), 0, None, std::ptr::null_mut()) };
            if rc != ffi::SQLITE_OK && !std::thread::panicking() {
                panic!("sqlite3_trace_v2 unregister: {rc}");
            }
        }
    }
    let unregister = Unregister(conn);
    let out = read();
    drop(unregister);
    (out, *seen)
}

/// Run one production read under [`traced`], keep every captured statement
/// that reads `table`, and answer each one's plan. Panics if the read ran
/// no statement over `table` (a capture that saw nothing proves nothing).
pub fn production_plans(
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
pub fn assert_all_use(label: &str, plans: &[(String, String)], index: &str) {
    for (sql, p) in plans {
        assert!(
            p.contains(&format!("USING INDEX {index}")),
            "{label}: the production statement does not use {index}\n  sql: {sql}\n  plan: {p}"
        );
    }
}

pub const CHAT: &str = "00000000-0000-0000-0000-000000000001";
pub const PARTICIPANT: &str = "00000000-0000-0000-0000-000000000002";

/// One production read's label, the index it must use, and its captured
/// `(statement, plan)` pairs.
pub type ReadPlans = (&'static str, &'static str, Vec<(String, String)>);

/// The three per-chat reads, each driven through its PRODUCTION function and
/// planned on the statement it ran ([`production_plans`]).
pub fn per_chat_read_plans(conn: &rusqlite::Connection) -> [ReadPlans; 3] {
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
