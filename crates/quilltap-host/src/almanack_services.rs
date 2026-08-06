//! The host half of the Almanack (P4.37 resume item 3) — the [`AlmanackHost`]
//! seam: paths, runtime facts, the passphrase flag, version, clock and the
//! disk storage backend. Until this wire landed, all four `SystemAlmanack*`
//! verbs answered the loud not-assembled refusal and the feature was
//! unreachable in production.
//!
//! ## The runtime block is HONEST host facts (the recorded phase-1 divergence)
//!
//! v4 reports `process.version`, `os.type()`, `os.freemem()` and so on. The
//! section's purpose is to describe the process that produced the report, so
//! this host reports what a Rust process can honestly say about itself:
//!
//!  - `nodeVersion` carries the host runtime string (`quilltap-host/<ver>`),
//!  - `osType`/`osRelease` come from `uname -s` / `uname -r` on unix (the same
//!    values Node's `os.type()`/`os.release()` surface) and fall back to the
//!    compile-time platform name elsewhere,
//!  - total memory is read from the platform (macOS `sysctl hw.memsize`,
//!    Linux `/proc/meminfo`); FREE memory is reported as 0 — there is no
//!    dependency-free portable read, and inventing one number would be less
//!    honest than a visible zero,
//!  - `uptimeSeconds` is the host process's own uptime,
//!  - `runtimeType` keeps v4's vocabulary: `docker` in a container, else
//!    `node` (v5 has no lima/electron shell detection).

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;

use quilltap_core::almanack::{AlmanackPaths, RuntimeFacts};
use quilltap_core::api::almanack::AlmanackHost;
use quilltap_core::api::provision::provision;
use quilltap_core::services::file_storage::StorageBackend;

use crate::backup_services::Clock;
use crate::files_store::LocalStorageBackend;

/// The live [`AlmanackHost`], one per assembly.
pub struct HostAlmanackServices {
    base_dir: PathBuf,
    version: String,
    env_pepper: Option<String>,
    tz: String,
    started: Instant,
    clock: Arc<dyn Clock>,
}

impl HostAlmanackServices {
    pub fn new(
        base_dir: PathBuf,
        version: String,
        env_pepper: Option<String>,
        tz: String,
        started: Instant,
        clock: Arc<dyn Clock>,
    ) -> Self {
        HostAlmanackServices {
            base_dir,
            version,
            env_pepper,
            tz,
            started,
            clock,
        }
    }

    fn data_dir(&self) -> PathBuf {
        self.base_dir.join("data")
    }
}

/// `uname -s` / `uname -r` — what Node's `os.type()` / `os.release()` report
/// on unix. Empty on failure (a blank cell over an invented one).
#[cfg(unix)]
fn uname(flag: &str) -> String {
    std::process::Command::new("uname")
        .arg(flag)
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .unwrap_or_default()
}

fn os_type() -> String {
    #[cfg(unix)]
    {
        let t = uname("-s");
        if !t.is_empty() {
            return t;
        }
    }
    std::env::consts::OS.to_string()
}

fn os_release() -> String {
    #[cfg(unix)]
    {
        return uname("-r");
    }
    #[allow(unreachable_code)]
    String::new()
}

/// Total physical memory, best-effort and dependency-free.
fn total_memory_bytes() -> f64 {
    #[cfg(target_os = "macos")]
    {
        if let Ok(out) = std::process::Command::new("sysctl")
            .args(["-n", "hw.memsize"])
            .output()
        {
            if let Ok(n) = String::from_utf8_lossy(&out.stdout).trim().parse::<f64>() {
                return n;
            }
        }
    }
    #[cfg(target_os = "linux")]
    {
        if let Ok(meminfo) = std::fs::read_to_string("/proc/meminfo") {
            for line in meminfo.lines() {
                if let Some(rest) = line.strip_prefix("MemTotal:") {
                    let kb: f64 = rest
                        .trim()
                        .trim_end_matches("kB")
                        .trim()
                        .parse()
                        .unwrap_or(0.0);
                    return kb * 1024.0;
                }
            }
        }
    }
    0.0
}

impl AlmanackHost for HostAlmanackServices {
    fn storage(&self) -> Arc<dyn StorageBackend> {
        // The disk backend over `<base>/files` — the same construction the
        // backup host and the spine use. A report's own bytes live in the
        // Uploads mount; this only matters for a legacy disk-keyed row.
        Arc::new(LocalStorageBackend::new(self.base_dir.join("files")))
    }

    fn paths(&self) -> AlmanackPaths {
        let data = self.data_dir();
        AlmanackPaths {
            main_db: data.join("quilltap.db"),
            llm_logs_db: Some(data.join("quilltap-llm-logs.db")),
            mount_index_db: Some(data.join("quilltap-mount-index.db")),
            backups_dir: data.join("backups"),
            data_dir: data,
        }
    }

    fn runtime_facts(&self) -> RuntimeFacts {
        RuntimeFacts {
            node_version: format!("quilltap-host/{}", self.version),
            platform: std::env::consts::OS.to_string(),
            arch: std::env::consts::ARCH.to_string(),
            os_type: os_type(),
            os_release: os_release(),
            total_memory_bytes: total_memory_bytes(),
            free_memory_bytes: 0.0,
            runtime_type: if crate::env::is_docker_environment() {
                "docker".to_string()
            } else {
                "node".to_string()
            },
            uptime_seconds: self.started.elapsed().as_secs_f64(),
            timezone: self.tz.clone(),
        }
    }

    fn passphrase_protected(&self) -> bool {
        // v4 `getHasUserPassphrase()` — captured at startup provisioning
        // there; recomputed with the SAME pure logic here (`provision` is a
        // read: env-pepper installs report `false` exactly as v4's case 1
        // does, and a passphrase-wrapped `.dbkey` without an env pepper
        // reports `true`). A hash-mismatch fatal cannot happen on a booted
        // instance; report the un-protected default if it somehow does.
        provision(&self.data_dir(), self.env_pepper.as_deref())
            .map(|p| p.has_user_passphrase)
            .unwrap_or(false)
    }

    fn app_version(&self) -> String {
        self.version.clone()
    }

    fn node_env(&self) -> String {
        // v4 `process.env.NODE_ENV || 'development'`. A running v5 host binary
        // is a production build unless told otherwise.
        std::env::var("NODE_ENV").unwrap_or_else(|_| "production".to_string())
    }

    fn now_ms(&self) -> i64 {
        self.clock.now_ms()
    }
}
