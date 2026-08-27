//! `quilltap instances restore-key` — rebuild a lost `.dbkey`, or re-wrap an
//! existing one under a different passphrase, from the pepper itself (v4
//! `packages/quilltap/lib/dbkey-restore.js`, `b121ac77f`).
//!
//! The pepper IS the database key; the `.dbkey` file is only a wrapper around
//! it. So an operator who kept the pepper printed at first-run setup (or who
//! runs with `ENCRYPTION_MASTER_PEPPER` in the environment) can always rebuild
//! the wrapper — including under a new passphrase, when the old one is gone.
//!
//! The load-bearing safety property, carried verbatim from v4: a `.dbkey`
//! holding the WRONG pepper is worse than no `.dbkey` at all — the server
//! unwraps it, hands the cipher a key that decrypts nothing, and reports what
//! looks like a corrupt database. So the candidate pepper is proved against
//! the encrypted databases on disk BEFORE anything is written, and that proof
//! cannot be waived while an encrypted database exists to check (`--force`
//! only covers a fresh or still-plaintext instance).

use std::io::Read;
use std::path::Path;

use quilltap_core::dbkey;
use quilltap_host::instances::{expand_path, InstanceRegistry};
use quilltap_host::lock::{acquire_write_lock, release_write_lock};

use crate::nodefmt::node_join;
use crate::out;
use crate::resolve::{prompt_line, prompt_passphrase};

/// Mirror of v4 `SQLITE_MAGIC` (lib/startup/db-encryption-state.ts).
const SQLITE_MAGIC: &[u8; 16] = b"SQLite format 3\0";

/// The three databases the one pepper opens, main first — it is the authority.
const DATABASES: [(&str, &str); 3] = [
    ("quilltap.db", "main database"),
    ("quilltap-llm-logs.db", "LLM logs database"),
    ("quilltap-mount-index.db", "mount index database"),
];

#[derive(Default)]
struct RestoreOpts {
    name: String,
    data_dir: String,
    /// v4 `opts.passphrase` — `None` is JS `undefined` (flag absent or its
    /// value missing); `Some("")` is an explicit empty value, which v4's
    /// `!== ''` gate ALSO sends down the prompt path.
    passphrase: Option<String>,
    no_passphrase: bool,
    force: bool,
    yes: bool,
}

/// v4 `cmdRestoreKey` (instances-commands.js:302-325): flag parse + the
/// no-target usage exit, then the restore flow. Verb arms `restore-key` and
/// `rebuild-key` both land here.
pub fn cmd_restore_key(args: &[String]) -> Result<(), String> {
    let mut opts = RestoreOpts::default();
    let mut i = 0usize;
    while i < args.len() {
        let a = args[i].as_str();
        match a {
            "-d" | "--data-dir" => {
                i += 1;
                // v4 `args[++i]` past the end is `undefined` → falsy.
                opts.data_dir = args.get(i).cloned().unwrap_or_default();
            }
            "--passphrase" => {
                i += 1;
                opts.passphrase = args.get(i).cloned();
            }
            "--no-passphrase" => opts.no_passphrase = true,
            "--force" => opts.force = true,
            "-y" | "--yes" => opts.yes = true,
            _ => {
                if a.starts_with('-') {
                    return Err(format!("Unknown flag: {a}"));
                }
                if !opts.name.is_empty() {
                    return Err("Specify one instance.".to_string());
                }
                opts.name = a.to_string();
            }
        }
        i += 1;
    }
    if opts.name.is_empty() && opts.data_dir.is_empty() {
        out::elog("Usage: quilltap instances restore-key <name> | --data-dir <instance-root>");
        out::exit(1);
    }
    restore_key(&opts)
}

struct Target {
    data_dir: String,
    /// The registry key when the target is (or resolves to) a registered
    /// instance — v4 `target.instanceName`, `null` when unregistered.
    instance_name: Option<String>,
}

/// Node `path.basename` for the absolute normalized paths `expand_path`
/// produces (no trailing separator except root).
fn node_basename(p: &str) -> &str {
    p.rsplit('/').next().unwrap_or(p)
}

/// Node `path.dirname`, same input contract.
fn node_dirname(p: &str) -> String {
    match p.rfind('/') {
        Some(0) => "/".to_string(),
        Some(i) => p[..i].to_string(),
        None => ".".to_string(),
    }
}

/// v4 `resolveTarget` (dbkey-restore.js:149-168): a registered instance name
/// XOR an explicit instance root; `--data-dir` accepts the instance root or
/// the `data/` directory itself, and scans the registry to recover the
/// instance name so a stored passphrase can be kept honest afterwards.
fn resolve_target(name: &str, data_dir_flag: &str) -> Result<Target, String> {
    if !name.is_empty() && !data_dir_flag.is_empty() {
        return Err("Specify either an instance name or --data-dir, not both.".to_string());
    }
    if !name.is_empty() {
        let inst = InstanceRegistry::at_default_location().resolve_instance(name)?;
        return Ok(Target {
            data_dir: node_join(&inst.path, "data"),
            instance_name: Some(inst.name),
        });
    }
    if data_dir_flag.is_empty() {
        // Unreachable from the CLI (cmd_restore_key exits on no target first),
        // kept for v4 parity — resolveTarget carries its own usage throw.
        return Err(
            "Usage: quilltap instances restore-key <name> | --data-dir <instance-root>".to_string(),
        );
    }
    let root = expand_path(data_dir_flag);
    // Accept either the instance root or the data directory itself.
    let data_dir = if node_basename(&root) == "data" {
        root
    } else {
        node_join(&root, "data")
    };
    let registry = InstanceRegistry::at_default_location().read_registry()?;
    let parent = expand_path(&node_dirname(&data_dir));
    let mut instance_name = None;
    if let Some(serde_json::Value::Object(instances)) = registry.get("instances") {
        for (key, entry) in instances {
            let stored = entry.get("path").and_then(|v| v.as_str()).unwrap_or("");
            if !stored.is_empty() && expand_path(stored) == parent {
                instance_name = Some(key.clone());
                break;
            }
        }
    }
    Ok(Target {
        data_dir,
        instance_name,
    })
}

/// 'encrypted' | 'plaintext' | 'absent' — by file header, exactly as the
/// server decides whether a database still needs converting (v4
/// `databaseState`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FileState {
    Absent,
    Plaintext,
    Encrypted,
}

fn database_state(db_path: &Path) -> Result<FileState, String> {
    if !db_path.exists() {
        return Ok(FileState::Absent);
    }
    let mut f = std::fs::File::open(db_path).map_err(|e| e.to_string())?;
    let mut header = [0u8; 16];
    let mut n = 0usize;
    while n < 16 {
        match f.read(&mut header[n..]) {
            Ok(0) => break,
            Ok(k) => n += k,
            Err(e) if e.kind() == std::io::ErrorKind::Interrupted => {}
            Err(e) => return Err(e.to_string()),
        }
    }
    if n < 16 {
        return Ok(FileState::Plaintext);
    }
    Ok(if &header == SQLITE_MAGIC {
        FileState::Plaintext
    } else {
        FileState::Encrypted
    })
}

struct ProofRow {
    filename: &'static str,
    state: FileState,
    /// `None` for a file there was nothing to check (absent / plaintext).
    ok: Option<bool>,
    error: Option<String>,
}

/// v4 `provePepper`: prove the candidate pepper against every encrypted
/// database in `data_dir`. `proved` is true only when at least one encrypted
/// database opened AND none that was encrypted failed. Opening is read-only
/// and reads the schema page, which is what actually exercises the key.
fn prove_pepper(data_dir: &str, pepper: &str) -> Result<(bool, Vec<ProofRow>), String> {
    let mut results = Vec::new();
    for (filename, label) in DATABASES {
        let db_path = node_join(data_dir, filename);
        let state = database_state(Path::new(&db_path))?;
        if state != FileState::Encrypted {
            results.push(ProofRow {
                filename,
                state,
                ok: None,
                error: None,
            });
            continue;
        }
        // v4 `openEncryptedDb(..., { readonly: true, friendlyName: label })` +
        // the sqlite_master count. The open's failure message is v4's composed
        // two-liner (only its first line is ever printed); the count probe
        // after a successful open throws the raw engine text, as in v4.
        let opened = crate::dbopen::open_readonly(&db_path, Some(pepper)).map_err(|e| {
            format!(
                "Cannot open {label}: {e}\nThe database may be encrypted with a different key, or the .dbkey file may be missing."
            )
        });
        let probe = opened.and_then(|conn| {
            conn.query_row("SELECT count(*) AS n FROM sqlite_master", [], |_| Ok(()))
                .map_err(|e| crate::dbopen::sqlite_msg(&e))
        });
        match probe {
            Ok(()) => results.push(ProofRow {
                filename,
                state,
                ok: Some(true),
                error: None,
            }),
            Err(err) => results.push(ProofRow {
                filename,
                state,
                ok: Some(false),
                error: Some(err),
            }),
        }
    }
    let checked: Vec<&ProofRow> = results.iter().filter(|r| r.ok.is_some()).collect();
    let proved = !checked.is_empty() && checked.iter().all(|r| r.ok == Some(true));
    Ok((proved, results))
}

/// v4 `confirm(question, assumeYes)` — the restore-key promptLine variant:
/// closed stdin (piped, /dev/null, CI) reads as an empty answer, so the
/// default (no) still applies. `prompt_line` already yields `""` at EOF.
fn confirm(question: &str, assume_yes: bool) -> bool {
    if assume_yes {
        return true;
    }
    let answer = prompt_line(&format!("{question} [y/N] "))
        .trim()
        .to_lowercase();
    answer == "y" || answer == "yes"
}

/// The `.bak-` timestamp: v4 `new Date().toISOString().replace(/[:.]/g, '-')`.
fn backup_stamp() -> String {
    quilltap_core::clock::iso_from_unix_ms(quilltap_core::clock::now_unix_ms())
        .replace([':', '.'], "-")
}

/// v4's archive-note predicate (dbkey-restore.js:331-334): the one thing a
/// re-wrap does NOT carry is the passphrase the ARCHIVE bundles were sealed
/// under, so the note fires when the effective passphrase changed — including
/// the both-set case, where the new one may or may not equal the old.
fn passphrase_changed(
    replacing_different_pepper: bool,
    had_user_passphrase: bool,
    new_passphrase_set: bool,
) -> bool {
    replacing_different_pepper
        || had_user_passphrase != new_passphrase_set
        || (had_user_passphrase && new_passphrase_set)
}

/// Releases the write lock on scope exit — v4's `finally` block
/// (`release_write_lock` is idempotent).
struct LockGuard<'a>(&'a Path);
impl Drop for LockGuard<'_> {
    fn drop(&mut self) {
        release_write_lock(self.0);
    }
}

/// v4 `restoreKey(opts)` (dbkey-restore.js:181-345), in order.
fn restore_key(opts: &RestoreOpts) -> Result<(), String> {
    let target = resolve_target(&opts.name, &opts.data_dir)?;
    let data_dir = target.data_dir.clone();

    if !Path::new(&data_dir).exists() {
        return Err(format!("Data directory does not exist: {data_dir}"));
    }

    out::log(&format!(
        "Instance:    {}",
        target.instance_name.as_deref().unwrap_or("(unregistered)")
    ));
    out::log(&format!("Data dir:    {data_dir}"));

    // The server keeps the pepper and the effective passphrase in memory; a
    // file rewritten underneath it would leave both stale (archive encryption
    // reads the cached passphrase). Recovery happens with the instance down.
    // Unlike `db --write` (prints the lock message bare), this verb's error
    // goes through the instances handler's `Error: <msg>` + exit 1 — v4's
    // instancesCommand catch.
    let dir = Path::new(&data_dir);
    acquire_write_lock(dir).map_err(|e| e.to_string())?;
    let _guard = LockGuard(dir);
    restore_key_locked(opts, &target, &data_dir)
}

fn restore_key_locked(opts: &RestoreOpts, target: &Target, data_dir: &str) -> Result<(), String> {
    // ---- 1. The pepper. Never a flag: a command line lands in shell history
    // and in `ps`. Environment or hidden prompt only.
    let mut pepper = std::env::var("ENCRYPTION_MASTER_PEPPER")
        .unwrap_or_default()
        .trim()
        .to_string();
    if !pepper.is_empty() {
        out::log("Pepper:      from ENCRYPTION_MASTER_PEPPER");
    } else {
        pepper = prompt_passphrase("Encryption master pepper (hidden): ")?
            .trim()
            .to_string();
        if pepper.is_empty() {
            return Err(
                "No pepper provided. Set ENCRYPTION_MASTER_PEPPER or paste it at the prompt."
                    .to_string(),
            );
        }
    }
    // v4 `Buffer.from(pepper, 'base64').length !== 32`. Node's decoder is
    // lenient (skips non-alphabet bytes) where v5's refuses them; either way a
    // garbage pepper is not 32 bytes, so the warning fires the same.
    let decoded_len = dbkey::pepper_b64_to_key_hex(&pepper)
        .map(|hex| hex.len() / 2)
        .unwrap_or(0);
    if decoded_len != 32 {
        out::log(
            "Warning:     that does not look like a Quilltap pepper (44-char base64 of 32 bytes).",
        );
    }

    // ---- 2. Prove it against the databases before writing anything.
    let (proved, results) = prove_pepper(data_dir, &pepper)?;
    out::log("");
    for r in &results {
        match (r.state, r.ok) {
            (FileState::Absent, _) => out::log(&format!("  {:<28} absent", r.filename)),
            (FileState::Plaintext, _) => out::log(&format!(
                "  {:<28} unencrypted (nothing to check against)",
                r.filename
            )),
            (_, Some(true)) => out::log(&format!(
                "  {:<28} opens with this pepper \u{2713}",
                r.filename
            )),
            _ => out::log(&format!(
                "  {:<28} DOES NOT OPEN — {}",
                r.filename,
                r.error
                    .as_deref()
                    .unwrap_or("")
                    .split('\n')
                    .next()
                    .unwrap_or("")
            )),
        }
    }
    out::log("");

    if results.iter().any(|r| r.ok == Some(false)) {
        return Err(
            "This pepper does not open the databases on disk. Refusing to write a .dbkey that\n\
             would make an intact instance look corrupt. If the files are cloud-evicted rather\n\
             than wrongly keyed, run `quilltap file-verify` first and try again."
                .to_string(),
        );
    }
    if !proved {
        out::log("No encrypted database exists here, so the pepper cannot be proved.");
        if results.iter().any(|r| r.state == FileState::Plaintext) {
            out::log("The databases are still unencrypted — the server will encrypt them with");
            out::log("this pepper on its next start, whether or not it is the original one.");
        }
        if !opts.force && !confirm("Write the .dbkey anyway?", opts.yes) {
            return Err("Aborted.".to_string());
        }
    }

    // ---- 3. What is already on disk.
    let existing = dbkey::read_dbkey_raw(Path::new(data_dir)).map_err(|e| e.to_string())?;
    let mut replacing_different_pepper = false;
    if let Some(raw) = &existing {
        let existing_hash = serde_json::from_str::<serde_json::Value>(raw)
            .ok()
            .and_then(|v| {
                v.get("pepperHash")
                    .and_then(|h| h.as_str().map(str::to_string))
            });
        if existing_hash.as_deref() == Some(dbkey::hash_pepper(&pepper).as_str()) {
            out::log("An existing .dbkey already holds this pepper — rewrapping it.");
        } else {
            replacing_different_pepper = true;
            out::log("WARNING: the existing .dbkey holds a DIFFERENT pepper than the one given.");
            if proved {
                out::log(
                    "The databases opened with the new one, so the file on disk is the stale part.",
                );
            }
            if !confirm("Replace it? (a timestamped backup is kept)", opts.yes) {
                return Err("Aborted.".to_string());
            }
        }
    }

    // ---- 4. The passphrase for the rebuilt file.
    let new_passphrase: String = if opts.no_passphrase {
        String::new()
    } else if let Some(p) = opts.passphrase.as_ref().filter(|p| !p.is_empty()) {
        p.clone()
    } else if opts.yes {
        String::new()
    } else {
        let first = prompt_passphrase("Passphrase for the rebuilt .dbkey (blank for none): ")?;
        if !first.is_empty() {
            let again = prompt_passphrase("Confirm passphrase: ")?;
            if again != first {
                return Err("Passphrases did not match.".to_string());
            }
        }
        first
    };

    let had_user_passphrase = existing
        .as_deref()
        .map(|raw| dbkey::try_decrypt_pepper(raw, dbkey::INTERNAL_PASSPHRASE).is_none())
        .unwrap_or(false);
    let effective: &str = if new_passphrase.is_empty() {
        dbkey::INTERNAL_PASSPHRASE
    } else {
        &new_passphrase
    };

    // ---- 5. Write, then read back and prove the round trip.
    let dbkey_path = node_join(data_dir, "quilltap.dbkey");
    let mut backup_path: Option<String> = None;
    if existing.is_some() {
        let bp = format!("{dbkey_path}.bak-{}", backup_stamp());
        std::fs::copy(&dbkey_path, &bp).map_err(|e| e.to_string())?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            // Best effort, as v4's try/catch around chmodSync.
            let _ = std::fs::set_permissions(&bp, std::fs::Permissions::from_mode(0o600));
        }
        backup_path = Some(bp);
    }

    // `minServerVersion` (the Electron shell's version floor) rides along in
    // the same file without belonging to the wrapping — carry it, and anything
    // like it, across the rebuild.
    dbkey::save_dbkey_preserving(
        Path::new(data_dir),
        existing.as_deref(),
        &pepper,
        &new_passphrase,
    )
    .map_err(|e| e.to_string())?;

    let round_trip = dbkey::read_dbkey_raw(Path::new(data_dir))
        .ok()
        .flatten()
        .and_then(|written| dbkey::try_decrypt_pepper(&written, effective));
    if round_trip.as_deref() != Some(pepper.as_str()) {
        if let Some(bp) = &backup_path {
            std::fs::copy(bp, &dbkey_path).map_err(|e| e.to_string())?;
        }
        return Err(
            "Wrote the .dbkey but could not read the pepper back — restored the previous file."
                .to_string(),
        );
    }

    out::log("");
    out::log(&format!("Wrote {dbkey_path} (mode 0600)."));
    if let Some(bp) = &backup_path {
        out::log(&format!("Previous file kept at {}.", node_basename(bp)));
    }
    out::log(if new_passphrase.is_empty() {
        "The instance now opens with no passphrase."
    } else {
        "The instance now unlocks with the passphrase you just set."
    });

    // ---- 6. Keep the registry's stored passphrase honest.
    if let Some(name) = &target.instance_name {
        InstanceRegistry::at_default_location().set_instance_passphrase(name, &new_passphrase)?;
        out::log(&if new_passphrase.is_empty() {
            format!("Cleared the stored passphrase for \"{name}\".")
        } else {
            format!("Updated the stored passphrase for \"{name}\".")
        });
    }

    // ---- 7. The one thing a re-wrap does NOT carry with it.
    if existing.is_some()
        && passphrase_changed(
            replacing_different_pepper,
            had_user_passphrase,
            !new_passphrase.is_empty(),
        )
    {
        out::log("");
        out::log("Note: character ARCHIVE bundles in files/ are encrypted with the passphrase,");
        out::log("not the pepper. This command does not rewrite them — bundles made under the");
        out::log("old passphrase still want the old one. The server's Change Passphrase card");
        out::log("re-encrypts them; this offline path cannot.");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// v4 `databaseState` by header: absent / short / SQLite magic → plaintext
    /// / anything else 16-byte → encrypted.
    #[test]
    fn database_state_classifies_by_header() {
        let dir = tempfile::tempdir().unwrap();
        let p = |name: &str| dir.path().join(name);

        assert_eq!(database_state(&p("missing.db")).unwrap(), FileState::Absent);

        std::fs::write(p("short.db"), b"tiny").unwrap();
        assert_eq!(
            database_state(&p("short.db")).unwrap(),
            FileState::Plaintext
        );

        let mut plain = SQLITE_MAGIC.to_vec();
        plain.extend_from_slice(&[0u8; 48]);
        std::fs::write(p("plain.db"), &plain).unwrap();
        assert_eq!(
            database_state(&p("plain.db")).unwrap(),
            FileState::Plaintext
        );

        std::fs::write(p("enc.db"), [0xAAu8; 64]).unwrap();
        assert_eq!(database_state(&p("enc.db")).unwrap(), FileState::Encrypted);

        // Exactly 16 non-magic bytes is encrypted; exactly 15 is plaintext.
        std::fs::write(p("edge16.db"), [0x01u8; 16]).unwrap();
        assert_eq!(
            database_state(&p("edge16.db")).unwrap(),
            FileState::Encrypted
        );
        std::fs::write(p("edge15.db"), [0x01u8; 15]).unwrap();
        assert_eq!(
            database_state(&p("edge15.db")).unwrap(),
            FileState::Plaintext
        );
    }

    /// The backup stamp is the millisecond ISO instant with `:` and `.`
    /// replaced by `-` (v4 `new Date().toISOString().replace(/[:.]/g, '-')`).
    #[test]
    fn backup_stamp_shape() {
        let stamp = backup_stamp();
        // e.g. 2026-08-27T21-49-05-123Z
        assert_eq!(stamp.len(), 24, "{stamp}");
        assert!(stamp.ends_with('Z'), "{stamp}");
        assert!(!stamp.contains(':') && !stamp.contains('.'), "{stamp}");
        let bytes = stamp.as_bytes();
        for (i, b) in bytes.iter().enumerate() {
            match i {
                4 | 7 | 13 | 16 | 19 => assert_eq!(*b, b'-', "{stamp}"),
                10 => assert_eq!(*b, b'T', "{stamp}"),
                23 => assert_eq!(*b, b'Z', "{stamp}"),
                _ => assert!(b.is_ascii_digit(), "{stamp}"),
            }
        }
    }

    /// v4's predicate, truth-tabled: `replacingDifferentPepper ||
    /// hadUserPassphrase !== newSet || (hadUserPassphrase && newSet)`.
    #[test]
    fn passphrase_changed_truth_table() {
        // (replacing, had, new_set) → expected
        let table = [
            ((false, false, false), false), // no-passphrase rewrap, same pepper
            ((false, false, true), true),   // none → set
            ((false, true, false), true),   // set → cleared
            ((false, true, true), true),    // both set: may differ, note fires
            ((true, false, false), true),   // different pepper always notes
            ((true, false, true), true),
            ((true, true, false), true),
            ((true, true, true), true),
        ];
        for ((replacing, had, new_set), expected) in table {
            assert_eq!(
                passphrase_changed(replacing, had, new_set),
                expected,
                "({replacing}, {had}, {new_set})"
            );
        }
    }

    #[test]
    fn node_path_helpers() {
        assert_eq!(node_basename("/a/b/data"), "data");
        assert_eq!(node_basename("/data"), "data");
        assert_eq!(node_dirname("/a/b/data"), "/a/b");
        assert_eq!(node_dirname("/a"), "/");
    }
}
