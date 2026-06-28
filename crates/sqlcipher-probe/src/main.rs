//! Phase-0 P4 probe.
//!
//! 1. Proves `rusqlite` + bundled SQLCipher compiles/links.
//! 2. Reports the bundled SQLCipher + SQLite versions and cipher defaults.
//! 3. Round-trips an encrypted DB using the raw-hex key form (meta.ts style).
//! 4. If given an instance data dir, reads `quilltap.dbkey` itself (via
//!    quilltap-core::dbkey — NO env var, NO saved pepper), derives the key,
//!    opens a COPY of the real DB, and lists tables.
//!
//! Run (versions + round trip only):
//!     cargo run -p sqlcipher-probe
//!
//! Run against a real instance (reads + decrypts the .dbkey for you):
//!     QT_DATA_DIR="/Users/csebold/iCloud/Quilltap/Friday/data" \
//!     QT_DB_COPY="/tmp/friday-copy.db" \
//!     cargo run -p sqlcipher-probe
//!   (QT_DB_COPY must be a COPY of quilltap.db. The .dbkey is read in place,
//!    read-only. If a user passphrase was set, also pass QT_DB_PASSPHRASE.)

use std::path::Path;

use quilltap_core::dbkey;
use rusqlite::Connection;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("== Phase-0 SQLCipher probe ==\n");

    // ---- 1 & 2: versions / cipher defaults ----
    let info = Connection::open_in_memory()?;
    info.pragma_update(None, "key", "probe-dummy-key")?;
    let cipher_version: Option<String> = info
        .query_row("PRAGMA cipher_version;", [], |r| r.get(0))
        .ok();
    let sqlite_version: String = info.query_row("SELECT sqlite_version();", [], |r| r.get(0))?;
    println!(
        "SQLCipher version : {}",
        cipher_version
            .as_deref()
            .unwrap_or("<none — NOT sqlcipher!>")
    );
    println!("SQLite version    : {sqlite_version}");
    for pragma in [
        "cipher_page_size",
        "kdf_iter",
        "cipher_hmac_algorithm",
        "cipher_kdf_algorithm",
    ] {
        let v: Option<String> = info
            .query_row(&format!("PRAGMA {pragma};"), [], |r| r.get(0))
            .ok();
        println!("  {pragma:<22}: {}", v.unwrap_or_else(|| "<n/a>".into()));
    }
    println!();

    // ---- 3: raw-hex-key round trip ----
    let tmp = std::env::temp_dir().join("qt-probe-rawkey.db");
    let _ = std::fs::remove_file(&tmp);
    let key_hex = "2b7e151628aed2a6abf7158809cf4f3c";
    {
        let c = Connection::open(&tmp)?;
        c.execute_batch(&format!(
            r#"PRAGMA key = "x'{key_hex}'";
               PRAGMA foreign_keys = ON;
               PRAGMA journal_mode = TRUNCATE;
               CREATE TABLE t (id TEXT PRIMARY KEY, v TEXT);
               INSERT INTO t VALUES ('a','hello-from-rust');"#
        ))?;
    }
    {
        let c = Connection::open(&tmp)?;
        c.pragma_update(None, "key", format!("x'{key_hex}'"))?;
        let v: String = c.query_row("SELECT v FROM t WHERE id='a';", [], |r| r.get(0))?;
        println!("raw-hex-key round trip: read back {v:?}  ✓");
    }
    let _ = std::fs::remove_file(&tmp);
    println!();

    // ---- 4: real instance via .dbkey self-decrypt ----
    match (std::env::var("QT_DATA_DIR"), std::env::var("QT_DB_COPY")) {
        (Ok(data_dir), Ok(db_copy)) => {
            println!("Reading .dbkey from: {data_dir}");
            let pass = std::env::var("QT_DB_PASSPHRASE").ok();
            let pepper_b64 = dbkey::load_pepper(Path::new(&data_dir), pass.as_deref())?;
            println!(
                "  pepper decrypted from .dbkey  ✓  (len {} b64 chars)",
                pepper_b64.len()
            );

            let key_hex = dbkey::pepper_b64_to_key_hex(&pepper_b64)?;
            println!("Opening DB copy:     {db_copy}");
            // Mirror db-helpers.js `openEncryptedDb` EXACTLY: open read-only,
            // set ONLY the key pragma (key must be the first and only op before
            // the first read), then verify with a bare SELECT 1. Do NOT touch
            // journal_mode / foreign_keys here — mutating journal_mode on an
            // existing encrypted file forces header writes that race the cipher
            // context and surface as NotADatabase.
            let c =
                Connection::open_with_flags(&db_copy, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)?;
            c.pragma_update(None, "key", format!("x'{key_hex}'"))?;
            // better-sqlite3-multiple-ciphers may default to a different cipher
            // scheme/compat than rusqlite's bundled SQLCipher. Pin SQLCipher v4
            // compatibility right after the key, before the first read.
            if let Ok(compat) = std::env::var("QT_CIPHER_COMPAT") {
                println!("  applying cipher_compatibility = {compat}");
                c.pragma_update(None, "cipher_compatibility", &compat)?;
            }
            c.query_row("SELECT 1;", [], |_| Ok(()))?; // proves the key engaged
            let mut stmt = c.prepare(
                "SELECT name FROM sqlite_master WHERE type='table' ORDER BY name LIMIT 12;",
            )?;
            let names: Vec<String> = stmt
                .query_map([], |r| r.get(0))?
                .collect::<Result<_, _>>()?;
            let count: i64 = c.query_row(
                "SELECT count(*) FROM sqlite_master WHERE type='table';",
                [],
                |r| r.get(0),
            )?;
            println!("  opened real instance: ✓  ({count} tables total)");
            println!("  first tables: {names:?}");
        }
        _ => println!("(skip) set QT_DATA_DIR + QT_DB_COPY to open a real instance COPY."),
    }

    println!("\n== probe complete ==");
    Ok(())
}
