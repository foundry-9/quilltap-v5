//! The resolution chain + pepper unlock — v4 `packages/quilltap/lib/
//! db-helpers.js` (`resolveDataDir`, `resolveDataDirAndPassphrase`,
//! `printDefaultInstanceHint`, `promptPassphrase`, `loadDbKey`), composed
//! over `quilltap-host::instances` (the registry) and
//! `quilltap-core::dbkey` (the crypto — never re-implemented here). Plus
//! v4 `lock-helpers.js`'s `resolveServerPort` (bug 239, `93c5af6ad`): the
//! port of a RUNNING server, read from its instance lock.

use std::io::{BufRead, Write};
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};

use quilltap_host::instances::InstanceRegistry;

use crate::nodefmt::{node_join, node_normalize};
use crate::out;

/// v4 `resolveDataDirAndPassphrase`'s return.
#[derive(Debug)]
pub struct Resolved {
    pub data_dir: String,
    pub passphrase: String,
    /// v4 returns it for the verbs that surface the instance name — the
    /// server-port resolver's label ([`resolve_server_port`]).
    pub instance_name: Option<String>,
    pub used_platform_default: bool,
}

fn home_dir_string() -> String {
    std::env::var("HOME").unwrap_or_else(|_| ".".to_string())
}

/// v4 `resolveDataDir(overrideDir)` — note the `~` expansion does NOT
/// `path.resolve` (a relative override stays relative), and every branch
/// appends `/data` via `path.join`.
pub fn resolve_data_dir(override_dir: &str) -> String {
    if !override_dir.is_empty() {
        let resolved = if let Some(rest) = override_dir.strip_prefix('~') {
            node_join(&home_dir_string(), rest.trim_start_matches('/'))
        } else {
            node_normalize(override_dir)
        };
        return node_join(&resolved, "data");
    }
    if let Ok(env) = std::env::var("QUILLTAP_DATA_DIR") {
        if !env.is_empty() {
            return node_join(&env, "data");
        }
    }
    let home = home_dir_string();
    #[cfg(target_os = "macos")]
    {
        node_join(
            &format!("{home}/Library/Application Support/Quilltap"),
            "data",
        )
    }
    #[cfg(windows)]
    {
        let base = std::env::var("APPDATA").unwrap_or_else(|_| format!("{home}/AppData/Roaming"));
        node_join(&format!("{base}/Quilltap"), "data")
    }
    #[cfg(not(any(target_os = "macos", windows)))]
    {
        node_join(&format!("{home}/.quilltap"), "data")
    }
}

/// v4 `resolveDataDirAndPassphrase({ dataDir, instance, passphrase })`.
/// Precedence: `--data-dir` → `--instance` → registered default instance →
/// `QUILLTAP_DATA_DIR` → platform default. Both flags together is an error.
pub fn resolve_data_dir_and_passphrase(
    data_dir: &str,
    instance: &str,
    passphrase: &str,
    registry: &InstanceRegistry,
) -> Result<Resolved, String> {
    if !data_dir.is_empty() && !instance.is_empty() {
        return Err("Specify either --instance or --data-dir, not both.".to_string());
    }
    if !instance.is_empty() {
        let inst = registry.resolve_instance(instance)?;
        return Ok(Resolved {
            data_dir: node_join(&inst.path, "data"),
            passphrase: if !passphrase.is_empty() {
                passphrase.to_string()
            } else {
                inst.passphrase
            },
            instance_name: Some(inst.name),
            used_platform_default: false,
        });
    }
    let env_set = std::env::var("QUILLTAP_DATA_DIR").is_ok_and(|v| !v.is_empty());
    if data_dir.is_empty() && !env_set {
        // v4: getDefaultInstance() throws propagate; the resolve itself is
        // try/caught (fall through to env/platform default on failure).
        if let Some(default_name) = registry.get_default_instance()? {
            if let Ok(inst) = registry.resolve_instance(&default_name) {
                return Ok(Resolved {
                    data_dir: node_join(&inst.path, "data"),
                    passphrase: if !passphrase.is_empty() {
                        passphrase.to_string()
                    } else {
                        inst.passphrase
                    },
                    instance_name: Some(inst.name),
                    used_platform_default: false,
                });
            }
        }
    }
    let used_platform_default = data_dir.is_empty() && !env_set;
    Ok(Resolved {
        data_dir: resolve_data_dir(data_dir),
        passphrase: passphrase.to_string(),
        instance_name: None,
        used_platform_default,
    })
}

// ============================================================================
// Server port resolution (v4 `lock-helpers.js:113-176`, bug 239)
//
// CLI tools that talk to a RUNNING server over HTTP (recall-replay,
// anchor-probe) need its port. The server records the port it listens on in
// the instance lock, so naming an instance (or a data dir) is enough to find
// it. An explicit --port always wins.
// ============================================================================

/// v4 `DEFAULT_SERVER_PORT`.
pub const DEFAULT_SERVER_PORT: i64 = 3000;

/// v4's `source`: where the port came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PortSource {
    Flag,
    Lock,
    Default,
}

/// v4 `resolveServerPort`'s return.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServerTarget {
    pub port: i64,
    pub source: PortSource,
    /// The one-line message to print (DIM, stderr) when the lock supplied the
    /// port.
    pub note: Option<String>,
    /// The connection-failure advice (may carry a second, `\n`-joined line).
    pub hint: String,
}

/// v4 `resolveServerPort({ port, instance, dataDir })`, arm for arm.
///
/// An explicit `--port` short-circuits before ANY resolution, so `--port N -i a
/// -d b` is not an error (v4's own "explicit --port wins over a live lock"
/// row). Otherwise the data dir resolves through
/// [`resolve_data_dir_and_passphrase`] (v4's lazy `require('./db-helpers')`),
/// so the both-flags sentence and the unknown-instance error are THAT fn's
/// bytes. The lock's port counts only when the probed classification is
/// ACTIVE (a live non-Quilltap PID is SUSPECT — no port), by the lock's ONE
/// lenient predicate ([`quilltap_host::lock::lock_port_from_json`] behind
/// `LockFileContent::port`). v4 reads the lock once; this reads it twice — the
/// classifier, then the typed read for the port — over the same file in the
/// same call (P4.D299 R-L10).
pub fn resolve_server_port(
    port: Option<i64>,
    instance: &str,
    data_dir: &str,
    registry: &InstanceRegistry,
) -> Result<ServerTarget, String> {
    use quilltap_host::lock::{classify_lock_status_probed, read_lock_file, LockStatus};

    let base_hint =
        "Start the server (npm run dev) or pass --port to match a non-default port.".to_string();
    if let Some(port) = port {
        return Ok(ServerTarget {
            port,
            source: PortSource::Flag,
            note: None,
            hint: base_hint,
        });
    }
    if instance.is_empty() && data_dir.is_empty() {
        return Ok(ServerTarget {
            port: DEFAULT_SERVER_PORT,
            source: PortSource::Default,
            note: None,
            hint: base_hint,
        });
    }

    let resolved = resolve_data_dir_and_passphrase(data_dir, instance, "", registry)?;
    let label = resolved
        .instance_name
        .clone()
        .unwrap_or_else(|| resolved.data_dir.clone());
    let lock_path = Path::new(&resolved.data_dir).join("quilltap.lock");
    let status = classify_lock_status_probed(&lock_path, quilltap_core::clock::now_unix_ms());
    let lock_port = read_lock_file(&lock_path).and_then(|lock| lock.port);

    if let (LockStatus::Active { .. }, Some(lock_port)) = (&status, lock_port) {
        return Ok(ServerTarget {
            port: i64::from(lock_port),
            source: PortSource::Lock,
            note: Some(format!(
                "Using port {lock_port} from the {label} instance lock"
            )),
            hint: base_hint,
        });
    }

    let why = match &status {
        LockStatus::Active { .. } => {
            "its lock records no port (server predates port recording)".to_string()
        }
        LockStatus::Absent => {
            "it has no instance lock, so no server appears to be running".to_string()
        }
        LockStatus::Stale { reason } => format!("its lock is stale ({reason})"),
        LockStatus::Suspect { reason } => format!("its lock looks suspect ({reason})"),
        LockStatus::Corrupt => "its lock file is unreadable".to_string(),
    };
    Ok(ServerTarget {
        port: DEFAULT_SERVER_PORT,
        source: PortSource::Default,
        note: None,
        hint: format!(
            "{base_hint}\nNo port found for the {label} instance: {why}; fell back to {DEFAULT_SERVER_PORT}."
        ),
    })
}

static HINT_PRINTED: AtomicBool = AtomicBool::new(false);

/// v4 `printDefaultInstanceHint` — a one-shot stderr hint when the CLI fell
/// back to the platform default but instances are registered. Silenced by
/// `QUILLTAP_QUIET_HINTS`.
pub fn print_default_instance_hint(resolved: &Resolved, registry: &InstanceRegistry) {
    if HINT_PRINTED.load(Ordering::Relaxed) {
        return;
    }
    if !resolved.used_platform_default {
        return;
    }
    if std::env::var_os("QUILLTAP_QUIET_HINTS").is_some_and(|v| !v.is_empty()) {
        return;
    }
    let Ok(registered) = registry.list_instances() else {
        return;
    };
    if registered.is_empty() {
        return;
    }
    HINT_PRINTED.store(true, Ordering::Relaxed);
    let names = registered
        .iter()
        .map(|r| r.name.as_str())
        .collect::<Vec<_>>()
        .join(", ");
    out::write_stderr(&format!(
        "Hint: using the default instance ({}). Registered: {}. Pass --instance <name> to target one. (set QUILLTAP_QUIET_HINTS=1 to silence)\n",
        resolved.data_dir, names
    ));
}

/// v4 `promptLine` (readline question): prompt to stdout, read one line.
pub fn prompt_line(prompt: &str) -> String {
    out::write_stdout(prompt.as_bytes());
    let _ = std::io::stdout().flush();
    let mut line = String::new();
    let _ = std::io::stdin().lock().read_line(&mut line);
    while line.ends_with('\n') || line.ends_with('\r') {
        line.pop();
    }
    line
}

/// v4 `promptPassphrase`: hidden TTY input (raw mode, no echo). Enter /
/// Ctrl-D finish; Ctrl-C restores the terminal and exits 130; DEL/backspace
/// deletes; no TTY → v4's exact error.
pub fn prompt_passphrase(prompt: &str) -> Result<String, String> {
    if !out::stdin_is_tty() {
        return Err(
            "This database requires a passphrase. Use --passphrase <pass> or set QUILLTAP_DB_PASSPHRASE"
                .to_string(),
        );
    }
    out::write_stdout(prompt.as_bytes());
    let _ = std::io::stdout().flush();

    #[cfg(unix)]
    {
        use std::io::Read;
        // Raw-ish mode: no canonical buffering, no echo.
        let mut orig: libc::termios = unsafe { std::mem::zeroed() };
        // SAFETY: fd 0 is a TTY (checked above); termios is a plain C struct.
        unsafe {
            libc::tcgetattr(0, &mut orig);
        }
        let mut raw = orig;
        raw.c_lflag &= !(libc::ICANON | libc::ECHO | libc::ISIG);
        raw.c_cc[libc::VMIN] = 1;
        raw.c_cc[libc::VTIME] = 0;
        unsafe {
            libc::tcsetattr(0, libc::TCSANOW, &raw);
        }
        let restore = || unsafe {
            libc::tcsetattr(0, libc::TCSANOW, &orig);
        };

        let mut passphrase = String::new();
        let mut stdin = std::io::stdin();
        let mut buf = [0u8; 64];
        let mut pending: Vec<u8> = Vec::new();
        loop {
            let n = match stdin.read(&mut buf) {
                Ok(0) => 0,
                Ok(n) => n,
                Err(_) => 0,
            };
            if n == 0 {
                restore();
                out::write_stdout(b"\n");
                return Ok(passphrase);
            }
            for &b in &buf[..n] {
                match b {
                    b'\n' | b'\r' | 0x04 => {
                        restore();
                        out::write_stdout(b"\n");
                        pending.clear();
                        return Ok(passphrase);
                    }
                    0x03 => {
                        restore();
                        out::write_stdout(b"\n");
                        out::exit(130);
                    }
                    0x7f | 0x08 => {
                        passphrase.pop();
                    }
                    _ => {
                        pending.push(b);
                        if let Ok(s) = std::str::from_utf8(&pending) {
                            passphrase.push_str(s);
                            pending.clear();
                        }
                    }
                }
            }
        }
    }
    #[cfg(not(unix))]
    {
        // Non-unix fallback: visible line input (no raw mode plumbing).
        Ok(prompt_line(""))
    }
}

/// v4 `loadDbKey(dataDir, passphrase)`: `None` when the instance is
/// unencrypted (no `.dbkey`); otherwise the base64 pepper. Handles the legacy
/// `hasPassphrase` strip-and-rewrite, the internal-sentinel-first order, and
/// the flag → env → interactive-prompt passphrase chain.
pub fn load_db_key(data_dir: &str, passphrase: &str) -> Result<Option<String>, String> {
    use quilltap_core::dbkey::{self, DbKeyError};

    let dir = Path::new(data_dir);
    let dbkey_path = dir.join("quilltap.dbkey");
    if !dbkey_path.exists() {
        return Ok(None);
    }

    // Legacy migration (v4 loadDbKey): a `hasPassphrase` key is stripped and
    // the file rewritten (pretty JSON, mode 0600, no trailing newline).
    if let Ok(raw) = std::fs::read_to_string(&dbkey_path) {
        if let Ok(serde_json::Value::Object(mut obj)) =
            serde_json::from_str::<serde_json::Value>(&raw)
        {
            if obj.contains_key("hasPassphrase") {
                obj.shift_remove("hasPassphrase");
                let pretty = serde_json::to_string_pretty(&serde_json::Value::Object(obj))
                    .map_err(|e| e.to_string())?;
                std::fs::write(&dbkey_path, pretty).map_err(|e| e.to_string())?;
                #[cfg(unix)]
                {
                    use std::os::unix::fs::PermissionsExt;
                    let _ = std::fs::set_permissions(
                        &dbkey_path,
                        std::fs::Permissions::from_mode(0o600),
                    );
                }
            }
        }
    }

    // 1) Internal sentinel first.
    match dbkey::load_pepper(dir, None) {
        Ok(pepper) => return Ok(Some(pepper)),
        Err(DbKeyError::PassphraseRequired) => {}
        Err(e) => return Err(e.to_string()),
    }

    // 2) Flag → env → hidden prompt.
    let mut pass = passphrase.to_string();
    if pass.is_empty() {
        if let Ok(env) = std::env::var("QUILLTAP_DB_PASSPHRASE") {
            pass = env;
        }
    }
    if pass.is_empty() {
        pass = prompt_passphrase("Database passphrase: ")?;
        if pass.is_empty() {
            return Err("No passphrase provided".to_string());
        }
    }

    match dbkey::load_pepper(dir, Some(&pass)) {
        Ok(pepper) => Ok(Some(pepper)),
        // Node's AES-GCM auth-failure message, verbatim (what v4's tryDecrypt
        // throws on a wrong passphrase).
        Err(DbKeyError::DecryptFailed) => {
            Err("Unsupported state or unable to authenticate data".to_string())
        }
        Err(e) => Err(e.to_string()),
    }
}

#[cfg(test)]
mod tests {
    //! Rust twins of v4's `resolveServerPort` describe
    //! (`packages/quilltap/lib/__tests__/server-port-resolution.test.js`, all
    //! seven cases), plus the arms v4's suite does not reach — suspect,
    //! corrupt, a cross-host container lock, and the lenient port predicate.
    use super::*;

    struct Scratch {
        _dir: tempfile::TempDir,
        root: String,
        lock_path: std::path::PathBuf,
        registry: InstanceRegistry,
    }

    fn scratch() -> Scratch {
        let dir = tempfile::Builder::new()
            .prefix("qtap-port-test-")
            .tempdir()
            .unwrap();
        std::fs::create_dir_all(dir.path().join("data")).unwrap();
        let root = dir.path().to_string_lossy().into_owned();
        let lock_path = dir.path().join("data/quilltap.lock");
        let registry = InstanceRegistry::at(dir.path().join("instances.json"));
        Scratch {
            _dir: dir,
            root,
            lock_path,
            registry,
        }
    }

    /// v4's `writeLock(overrides)`: this process, this host, a fresh
    /// heartbeat, then the overrides' keys (raw JSON text) laid over it.
    fn write_lock(s: &Scratch, overrides: &[(&str, &str)]) {
        let now = quilltap_core::clock::now_iso();
        let mut map: serde_json::Map<String, serde_json::Value> =
            serde_json::from_value(serde_json::json!({
                "pid": std::process::id(),
                "hostname": quilltap_host::lock::hostname(),
                "startedAt": now,
                "lastHeartbeat": now,
                "environment": "local",
                "processTitle": "node",
                "processArgv0": "/usr/bin/node",
                "history": [],
            }))
            .unwrap();
        for (k, v) in overrides {
            map.insert(k.to_string(), serde_json::from_str(v).unwrap());
        }
        std::fs::write(&s.lock_path, serde_json::to_string(&map).unwrap()).unwrap();
    }

    fn resolve(s: &Scratch, port: Option<i64>, instance: &str) -> ServerTarget {
        resolve_server_port(port, instance, &s.root, &s.registry).unwrap()
    }

    #[test]
    fn defaults_to_3000_with_no_flags_at_all() {
        let s = scratch();
        let t = resolve_server_port(None, "", "", &s.registry).unwrap();
        assert_eq!(t.port, 3000);
        assert_eq!(t.source, PortSource::Default);
        assert_eq!(t.note, None);
        assert_eq!(
            t.hint,
            "Start the server (npm run dev) or pass --port to match a non-default port."
        );
    }

    #[test]
    fn an_explicit_port_wins_over_a_live_lock() {
        let s = scratch();
        write_lock(&s, &[("port", "3005")]);
        let t = resolve(&s, Some(4000), "");
        assert_eq!((t.port, t.source), (4000, PortSource::Flag));
        // It short-circuits before resolution: both flags are NOT an error.
        let t = resolve_server_port(Some(4000), "x", &s.root, &s.registry).unwrap();
        assert_eq!((t.port, t.source), (4000, PortSource::Flag));
    }

    #[test]
    fn uses_the_port_from_a_live_lock_and_says_so() {
        let s = scratch();
        write_lock(&s, &[("port", "3005")]);
        let t = resolve(&s, None, "");
        assert_eq!((t.port, t.source), (3005, PortSource::Lock));
        assert_eq!(
            t.note.as_deref(),
            Some(format!("Using port 3005 from the {}/data instance lock", s.root).as_str())
        );
    }

    #[test]
    fn falls_back_when_the_live_lock_records_no_port() {
        let s = scratch();
        write_lock(&s, &[]);
        let t = resolve(&s, None, "");
        assert_eq!(
            (t.port, t.source, t.note.clone()),
            (3000, PortSource::Default, None)
        );
        assert_eq!(
            t.hint,
            format!(
                "Start the server (npm run dev) or pass --port to match a non-default port.\n\
                 No port found for the {}/data instance: its lock records no port (server \
                 predates port recording); fell back to 3000.",
                s.root
            )
        );
    }

    #[test]
    fn ignores_the_port_of_a_stale_lock() {
        let s = scratch();
        write_lock(&s, &[("pid", "4206649"), ("port", "3005")]);
        let t = resolve(&s, None, "");
        assert_eq!(t.port, 3000);
        assert!(
            t.hint
                .contains("its lock is stale (PID 4206649 is no longer running)"),
            "{}",
            t.hint
        );
    }

    #[test]
    fn falls_back_when_there_is_no_lock() {
        let s = scratch();
        let t = resolve(&s, None, "");
        assert_eq!(t.port, 3000);
        assert!(
            t.hint
                .contains("it has no instance lock, so no server appears to be running"),
            "{}",
            t.hint
        );
    }

    #[test]
    fn rejects_instance_together_with_data_dir() {
        let s = scratch();
        let err = resolve_server_port(None, "x", &s.root, &s.registry).unwrap_err();
        assert_eq!(err, "Specify either --instance or --data-dir, not both.");
    }

    #[test]
    #[cfg(unix)]
    fn a_suspect_lock_names_its_reason() {
        let s = scratch();
        let mut sleeper = std::process::Command::new("sleep")
            .arg("30")
            .spawn()
            .unwrap();
        let pid = sleeper.id().to_string();
        write_lock(&s, &[("pid", &pid), ("port", "3005")]);
        let t = resolve(&s, None, "");
        let _ = sleeper.kill();
        let _ = sleeper.wait();
        assert_eq!(t.port, 3000);
        assert!(
            t.hint.contains(&format!(
                "its lock looks suspect (PID {pid} is alive but does not look like a Quilltap process)"
            )),
            "{}",
            t.hint
        );
    }

    #[test]
    fn a_corrupt_lock_is_unreadable() {
        let s = scratch();
        std::fs::write(&s.lock_path, "{ not json").unwrap();
        let t = resolve(&s, None, "");
        assert_eq!(t.port, 3000);
        assert!(t
            .hint
            .ends_with(": its lock file is unreadable; fell back to 3000."));
    }

    #[test]
    fn a_cross_host_container_lock_with_a_fresh_heartbeat_supplies_its_port() {
        let s = scratch();
        write_lock(
            &s,
            &[
                ("hostname", "\"elsewhere-host\""),
                ("environment", "\"docker\""),
                ("port", "3005"),
            ],
        );
        let t = resolve(&s, None, "");
        assert_eq!((t.port, t.source), (3005, PortSource::Lock));
    }

    #[test]
    fn the_lock_port_is_read_leniently() {
        for (raw, want) in [
            ("3005.0", Some(3005)),
            ("\"3005\"", None),
            ("70000", None),
            ("0", None),
            ("3000.5", None),
            ("null", None),
        ] {
            let s = scratch();
            write_lock(&s, &[("port", raw)]);
            let t = resolve(&s, None, "");
            match want {
                Some(port) => assert_eq!((t.port, t.source), (port, PortSource::Lock), "{raw}"),
                None => {
                    assert_eq!((t.port, t.source), (3000, PortSource::Default), "{raw}");
                    assert!(
                        t.hint.contains("its lock records no port"),
                        "{raw}: {}",
                        t.hint
                    );
                }
            }
        }
    }

    #[test]
    fn a_named_instance_labels_by_name() {
        let s = scratch();
        s.registry.upsert_instance("instA", &s.root, "").unwrap();
        write_lock(&s, &[("port", "3005")]);
        let t = resolve_server_port(None, "instA", "", &s.registry).unwrap();
        assert_eq!(
            t.note.as_deref(),
            Some("Using port 3005 from the instA instance lock")
        );
    }
}
