//! CLI for the real-snapshot fixture sanitizer.
//!
//! Usage:
//!   quilltap-fixture-sanitizer --source <copy-data-dir> --dest <out-dir> \
//!       [--passphrase <p>] [--verify]
//!
//! `--source` is a **COPY** of an instance's `data/` directory (the one that
//! holds `quilltap.dbkey` + the `.db` files). The tool recovers the real pepper
//! from the copy's `.dbkey` (in memory only), sanitizes each database into
//! `--dest`, and re-keys the output under the committed TEST pepper. It refuses
//! a source that looks like a live instance. It NEVER writes the real pepper and
//! NEVER copies the `.dbkey` into `--dest`.
//!
//! Per the project decision (2026-07-01), no Friday-derived output is committed;
//! this binary is run locally to produce (or refresh) a sanitized fixture, and
//! `--verify` reads it back through the ported repos to prove real-shaped rows
//! marshal cleanly.

use std::path::{Path, PathBuf};
use std::process::ExitCode;

use quilltap_fixture_sanitizer::{
    open_read, open_write_fresh, sanitize_db, INSTANCE_DB_FILES, TEST_PEPPER_B64,
};

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("error: {e}");
            ExitCode::FAILURE
        }
    }
}

struct Args {
    source: PathBuf,
    dest: PathBuf,
    passphrase: Option<String>,
    verify: bool,
}

fn parse_args() -> std::result::Result<Args, String> {
    let mut source = None;
    let mut dest = None;
    let mut passphrase = None;
    let mut verify = false;
    let mut it = std::env::args().skip(1);
    while let Some(a) = it.next() {
        match a.as_str() {
            "--source" => source = Some(PathBuf::from(it.next().ok_or("--source needs a value")?)),
            "--dest" => dest = Some(PathBuf::from(it.next().ok_or("--dest needs a value")?)),
            "--passphrase" => passphrase = Some(it.next().ok_or("--passphrase needs a value")?),
            "--verify" => verify = true,
            other => return Err(format!("unknown argument: {other}")),
        }
    }
    Ok(Args {
        source: source.ok_or("--source <copy-data-dir> is required")?,
        dest: dest.ok_or("--dest <out-dir> is required")?,
        passphrase,
        verify,
    })
}

/// Refuse a source path that looks like a live instance. The real pepper is the
/// master key to everything — we only ever operate on a COPY.
fn guard_not_live(source: &Path) -> std::result::Result<(), String> {
    let canon = source
        .canonicalize()
        .map_err(|e| format!("bad --source: {e}"))?;
    let s = canon.to_string_lossy();
    let looks_live = s.contains("Mobile Documents") // iCloud Drive backing dir
        || s.contains("com~apple~CloudDocs")
        || s.contains("/iCloud")
        || s.replace('\\', "/").contains("Quilltap/Friday");
    if looks_live {
        return Err(format!(
            "refusing to run against what looks like a LIVE instance ({s}). \
             Copy the data dir to scratch first and point --source at the copy."
        ));
    }
    Ok(())
}

fn run() -> std::result::Result<(), String> {
    let args = parse_args()?;
    guard_not_live(&args.source)?;

    // Recover the real pepper from the COPY's .dbkey — in memory only. Never
    // printed, logged, or written anywhere.
    let real_pepper =
        quilltap_core::dbkey::load_pepper(&args.source, args.passphrase.as_deref())
            .map_err(|e| format!("could not load pepper from {}: {e}", args.source.display()))?;

    std::fs::create_dir_all(&args.dest).map_err(|e| format!("could not create --dest: {e}"))?;

    let mut any = false;
    for name in INSTANCE_DB_FILES {
        let src_path = args.source.join(name);
        if !src_path.exists() {
            continue;
        }
        any = true;
        let dst_path = args.dest.join(name);
        for suffix in ["", "-journal", "-wal", "-shm"] {
            let p = dst_path.with_file_name(format!("{name}{suffix}"));
            let _ = std::fs::remove_file(&p);
        }

        let src = open_read(&src_path, &real_pepper).map_err(|e| format!("{name}: open: {e}"))?;
        let dst = open_write_fresh(&dst_path, TEST_PEPPER_B64)
            .map_err(|e| format!("{name}: create: {e}"))?;
        let report = sanitize_db(&src, &dst).map_err(|e| format!("{name}: sanitize: {e}"))?;
        eprintln!(
            "{name}: {} tables, {} rows sanitized ({} document-store files re-hashed)",
            report.tables, report.rows, report.files_rekeyed
        );
    }
    if !any {
        return Err(format!(
            "no instance .db files found under {}",
            args.source.display()
        ));
    }

    eprintln!(
        "done → {} (keyed with the committed TEST pepper; no .dbkey written, real pepper never persisted)",
        args.dest.display()
    );

    if args.verify {
        verify(&args.dest)?;
    }
    Ok(())
}

/// The payoff: read the sanitized output back through the ported repos to prove
/// real-shaped rows marshal without error. Plain-marshaling reads (no vault
/// overlay) plus the overlaying `characters` read.
fn verify(dest: &Path) -> std::result::Result<(), String> {
    use quilltap_core::db::{characters_read, chats_read, memories_read};

    let main_path = dest.join("quilltap.db");
    let main =
        open_read(&main_path, TEST_PEPPER_B64).map_err(|e| format!("verify open main: {e}"))?;

    let mem = memories_read::find_all(&main).map_err(|e| format!("verify memories_read: {e}"))?;
    eprintln!("verify: memories_read::find_all → {} rows OK", mem.len());
    let chats = chats_read::find_all(&main).map_err(|e| format!("verify chats_read: {e}"))?;
    eprintln!("verify: chats_read::find_all → {} rows OK", chats.len());

    let mount_path = dest.join("quilltap-mount-index.db");
    if mount_path.exists() {
        let mount = open_read(&mount_path, TEST_PEPPER_B64)
            .map_err(|e| format!("verify open mount: {e}"))?;
        let chars = characters_read::find_all(&main, &mount)
            .map_err(|e| format!("verify characters_read: {e}"))?;
        eprintln!(
            "verify: characters_read::find_all (with vault overlay) → {} rows OK",
            chars.len()
        );
    }
    Ok(())
}
