//! The resolution chain + pepper unlock — v4 `packages/quilltap/lib/
//! db-helpers.js` (`resolveDataDir`, `resolveDataDirAndPassphrase`,
//! `printDefaultInstanceHint`, `promptPassphrase`, `loadDbKey`), composed
//! over `quilltap-host::instances` (the registry) and
//! `quilltap-core::dbkey` (the crypto — never re-implemented here).

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
    /// v4 returns it for the verbs that surface the instance name; the Tier-R
    /// verbs don't consume it yet.
    #[allow(dead_code)]
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
