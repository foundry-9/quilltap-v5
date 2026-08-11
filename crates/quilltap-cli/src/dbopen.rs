//! Read-only encrypted opens, shared by the direct-mode verbs (v4
//! `db-helpers.js` `openEncryptedDb`): `PRAGMA key = "x'<hex>'"` is the first
//! and only pragma, then `SELECT 1` verifies before use — the read-path rule.

/// The message better-sqlite3 would surface as `err.message`: the engine text
/// for a SQLite failure, the display form otherwise.
pub fn sqlite_msg(e: &rusqlite::Error) -> String {
    match e {
        rusqlite::Error::SqliteFailure(_, Some(m)) => m.clone(),
        other => other.to_string(),
    }
}

pub fn open_readonly(db_path: &str, pepper: Option<&str>) -> Result<rusqlite::Connection, String> {
    use rusqlite::OpenFlags;
    let conn = rusqlite::Connection::open_with_flags(
        db_path,
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )
    .map_err(|e| sqlite_msg(&e))?;
    if let Some(pepper) = pepper {
        let key_hex =
            quilltap_core::dbkey::pepper_b64_to_key_hex(pepper).map_err(|e| e.to_string())?;
        conn.pragma_update(None, "key", format!("x'{key_hex}'"))
            .map_err(|e| sqlite_msg(&e))?;
    }
    conn.query_row("SELECT 1", [], |_| Ok(()))
        .map_err(|e| sqlite_msg(&e))?;
    Ok(conn)
}
