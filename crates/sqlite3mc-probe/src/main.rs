//! Phase-0 probe #2 — open a REAL Quilltap database with the correct cipher.
//!
//! Probe #1 (sqlcipher-probe) proved the .dbkey decrypts and the toolchain
//! works, but stock SQLCipher (AES) returned NotADatabase on the real file
//! because Quilltap databases are ChaCha20/sqleet (confirmed: PRAGMA cipher →
//! "chacha20"). This probe links SQLite3MultipleCiphers (sqleet default) via
//! build.rs and confirms it reads real rows.
//!
//! Run:
//!     QT_DATA_DIR="/Users/csebold/iCloud/Quilltap/Friday/data" \
//!     QT_DB_COPY="/tmp/friday-copy.db" \
//!     cargo run -p sqlite3mc-probe
//!   (QT_DB_COPY must be a COPY. Add QT_DB_PASSPHRASE if a user passphrase was set.)

use std::path::Path;

use quilltap_core::dbkey;
use rusqlite::Connection;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("== Phase-0 SQLite3MC probe (ChaCha20/sqleet) ==\n");

    // Confirm we linked SQLite3MultipleCiphers, not stock/SQLCipher.
    let mem = Connection::open_in_memory()?;
    let cipher_default: Option<String> = mem.query_row("PRAGMA cipher;", [], |r| r.get(0)).ok();
    let sqlite_version: String = mem.query_row("SELECT sqlite_version();", [], |r| r.get(0))?;
    println!("SQLite version       : {sqlite_version}");
    println!(
        "default PRAGMA cipher : {}",
        cipher_default
            .as_deref()
            .unwrap_or("<none — NOT SQLite3MC!>")
    );
    println!();

    let (data_dir, db_copy) = match (std::env::var("QT_DATA_DIR"), std::env::var("QT_DB_COPY")) {
        (Ok(a), Ok(b)) => (a, b),
        _ => {
            println!("(set QT_DATA_DIR + QT_DB_COPY to open a real instance COPY.)");
            return Ok(());
        }
    };

    // 1) Decrypt the pepper from .dbkey (proven path from probe #1).
    let pass = std::env::var("QT_DB_PASSPHRASE").ok();
    let pepper_b64 = dbkey::load_pepper(Path::new(&data_dir), pass.as_deref())?;
    let key_hex = dbkey::pepper_b64_to_key_hex(&pepper_b64)?;
    println!("pepper decrypted from .dbkey  ✓");

    // 2) Open the copy read-only, key first and ONLY (mirror openEncryptedDb).
    //    SQLite3MC's default cipher is sqleet/chacha20 — same as the app — so no
    //    `cipher=` pragma is needed. If this ever needs to be explicit:
    //        c.pragma_update(None, "cipher", "chacha20")?;  // BEFORE the key
    let c = Connection::open_with_flags(
        Path::new(&db_copy),
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
    )?;
    c.pragma_update(None, "key", format!("x'{key_hex}'"))?;

    // 3) The real test: read actual schema + a real row count.
    let cipher_now: Option<String> = c.query_row("PRAGMA cipher;", [], |r| r.get(0)).ok();
    let table_count: i64 = c.query_row(
        "SELECT count(*) FROM sqlite_master WHERE type='table';",
        [],
        |r| r.get(0),
    )?;
    println!(
        "opened real instance: ✓  (cipher = {}, {table_count} tables)",
        cipher_now.as_deref().unwrap_or("?")
    );

    // Prove we can read genuine application data, not just open the file.
    if let Ok(n) = c.query_row(
        "SELECT count(*) FROM characters;",
        [],
        |r: &rusqlite::Row| r.get::<_, i64>(0),
    ) {
        println!("  characters table: {n} rows");
    }
    if let Ok(n) = c.query_row("SELECT count(*) FROM memories;", [], |r: &rusqlite::Row| {
        r.get::<_, i64>(0)
    }) {
        println!("  memories table:   {n} rows");
    }
    let mut stmt =
        c.prepare("SELECT name FROM sqlite_master WHERE type='table' ORDER BY name LIMIT 8;")?;
    let names: Vec<String> = stmt
        .query_map([], |r| r.get(0))?
        .collect::<Result<_, _>>()?;
    println!("  first tables: {names:?}");

    println!("\n== probe complete — correct cipher confirmed ==");
    Ok(())
}
