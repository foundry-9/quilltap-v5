//! The CLI's ONE encrypted opener (v4 `db-helpers.js` `openEncryptedDb`).
//!
//! Every direct-mode verb that opens an instance database comes through here:
//! the `db` verb's raw SQL / `--tables` / `--count` / `--write`, `db
//! characters`, the `docs` verbs, `sync`'s store lookup, and `instances
//! restore-key`'s pepper proof. v4 `a2db63da7` (bug 162) retired the bin's
//! private opener onto this one "because two openers is how this happened": the
//! private one never registered `qt_text()`, so raw SQL could not read inside a
//! compressed column and every `--write` to `chat_messages` failed on the FTS
//! triggers. (⚠ That commit's TITLE says the raw-SQL path "opens without
//! qt_text()" — v5's second opener had registered it since P4.D203; what the
//! hunk moves on this side is the opener count and the failure STRINGS.) A
//! function registered here reaches every verb for free; do not add a second
//! opener.
//!
//! The read-path rule: `PRAGMA key = "x'<hex>'"` is the first and only pragma,
//! then `SELECT 1` verifies before use.

use rusqlite::{Connection, OpenFlags};

/// The message better-sqlite3 would surface as `err.message`: the engine text
/// for a SQLite failure, the display form otherwise.
pub fn sqlite_msg(e: &rusqlite::Error) -> String {
    match e {
        rusqlite::Error::SqliteFailure(_, Some(m)) => m.clone(),
        other => other.to_string(),
    }
}

/// v4 `openEncryptedDb`'s `{ readonly, friendlyName }`.
pub struct OpenOptions<'a> {
    pub readonly: bool,
    /// Names the database in the refusals: `main database`, `LLM logs
    /// database`, `mount index database` (v4 `openMainDb` & co.).
    pub friendly_name: &'a str,
}

/// A failed open, carrying v4's `err.message` exactly as thrown — the probe
/// arm's is ONE string with an embedded newline (the hint on its second line);
/// a caller that prints only the first line splits it at its own print site.
#[derive(Debug)]
pub struct OpenFailure {
    pub message: String,
}

impl std::fmt::Display for OpenFailure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message)
    }
}

impl OpenFailure {
    fn bare(message: String) -> Self {
        OpenFailure { message }
    }
}

/// Open `db_path` as v4 `openEncryptedDb(dbPath, pepper, opts)` does, with
/// v4's error SHAPE arm for arm:
///
/// - the file is missing → `{friendly} not found: {path}`;
/// - the constructor throws (e.g. an unreadable file) → the BARE engine
///   message (v4 does not wrap `new Database(...)`);
/// - the key pragma throws → BARE, likewise (unreached on either side: the
///   pragma only installs the codec and never reads the file);
/// - the `SELECT 1` probe throws → `Cannot open {friendly}: {e}` + the hint;
/// - `qt_text()` registration fails → reported (see below).
pub fn open_encrypted(
    db_path: &str,
    pepper: Option<&str>,
    opts: OpenOptions<'_>,
) -> Result<Connection, OpenFailure> {
    if !std::path::Path::new(db_path).exists() {
        return Err(OpenFailure::bare(format!(
            "{} not found: {db_path}",
            opts.friendly_name
        )));
    }
    let flags = if opts.readonly {
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX
    } else {
        OpenFlags::SQLITE_OPEN_READ_WRITE | OpenFlags::SQLITE_OPEN_NO_MUTEX
    };
    let conn = Connection::open_with_flags(db_path, flags).map_err(|e| {
        // rusqlite appends `: <path>` to a failed open's engine text — and ONLY
        // on `CannotOpen` (`inner_connection.rs`, the `open_with_flags` arm);
        // better-sqlite3's constructor throws `sqlite3_errmsg` alone. Strip
        // exactly that suffix on exactly that code — measured by the `db
        // unreadable database` Tier R case (`unable to open database file`).
        // Any other engine text is v4's bytes already and is left whole.
        let msg = sqlite_msg(&e);
        let cannot_open = matches!(
            &e,
            rusqlite::Error::SqliteFailure(
                rusqlite::ffi::Error {
                    code: rusqlite::ErrorCode::CannotOpen,
                    ..
                },
                _
            )
        );
        let suffix = format!(": {db_path}");
        let stripped = if cannot_open {
            msg.strip_suffix(&suffix).map(str::to_string)
        } else {
            None
        };
        OpenFailure::bare(stripped.unwrap_or(msg))
    })?;
    if let Some(pepper) = pepper {
        // v4 `Buffer.from(pepper, 'base64')` never throws; a pepper that
        // `loadDbKey` handed back is always decodable, so this arm is v5-only
        // and unreachable from a real `.dbkey`.
        let key_hex = quilltap_core::dbkey::pepper_b64_to_key_hex(pepper)
            .map_err(|e| OpenFailure::bare(e.to_string()))?;
        conn.pragma_update(None, "key", format!("x'{key_hex}'"))
            .map_err(|e| OpenFailure::bare(sqlite_msg(&e)))?;
    }
    if let Err(e) = conn.query_row("SELECT 1", [], |_| Ok(())) {
        return Err(OpenFailure::bare(format!(
            "Cannot open {}: {}\nThe database may be encrypted with a different key, or the .dbkey file may be missing.",
            opts.friendly_name,
            sqlite_msg(&e)
        )));
    }
    // v4 registers `qt_text()` here (`db-helpers.js:211`) so raw SQL and the
    // repl can read inside a compressed column — and it is REQUIRED for any
    // `--write` that touches `chat_messages`: the message search triggers call
    // it. v4 wraps the call in a `try` ("an old better-sqlite3 without
    // db.function must not block a read"); Rust has no such ambient-version
    // worry — `create_scalar_function` is a compile-time capability of the
    // pinned rusqlite — so a failure here is reported rather than swallowed.
    // A recorded divergence that no Tier R case can reach on either side.
    quilltap_core::db::text_compression::register_qt_text(&conn)
        .map_err(|e| OpenFailure::bare(sqlite_msg(&e)))?;
    Ok(conn)
}
