//! Data-dir and app-dir resolution (v4 `lib/paths.ts` + the launcher's
//! platform conventions).
//!
//! P4.0 ships the minimal chain — explicit → `QUILLTAP_DATA_DIR` → platform
//! default. The docker default (`/app/quilltap`) lands with P4.2's
//! Dockerfile; the default-instance link in the chain lands with the CLI
//! (P4.3).

use std::path::PathBuf;

fn home_dir() -> PathBuf {
    // v4 uses os.homedir(); HOME/USERPROFILE cover the supported platforms.
    #[cfg(windows)]
    let var = std::env::var_os("USERPROFILE");
    #[cfg(not(windows))]
    let var = std::env::var_os("HOME");
    var.map(PathBuf::from).unwrap_or_else(|| PathBuf::from("."))
}

/// v4 `getPlatformDefaultBaseDir()` — where an instance lives when nothing
/// says otherwise.
pub fn platform_default_base_dir() -> PathBuf {
    #[cfg(target_os = "macos")]
    {
        home_dir()
            .join("Library")
            .join("Application Support")
            .join("Quilltap")
    }
    #[cfg(windows)]
    {
        std::env::var_os("APPDATA")
            .map(PathBuf::from)
            .unwrap_or_else(|| home_dir().join("AppData").join("Roaming"))
            .join("Quilltap")
    }
    #[cfg(not(any(target_os = "macos", windows)))]
    {
        home_dir().join(".quilltap")
    }
}

/// Resolve the instance base directory: explicit flag → `QUILLTAP_DATA_DIR`
/// → the platform default.
pub fn resolve_base_dir(explicit: Option<PathBuf>) -> PathBuf {
    if let Some(p) = explicit {
        return p;
    }
    if let Some(env) = std::env::var_os("QUILLTAP_DATA_DIR") {
        if !env.is_empty() {
            return PathBuf::from(env);
        }
    }
    platform_default_base_dir()
}

/// The launcher's per-user app directory (v4 `packages/quilltap/lib/
/// instances.js` `getAppDir()`): where `instances.json` lives. Note the Linux
/// location is `~/.quilltap` here (the launcher's convention), which happens
/// to coincide with the data default above.
pub fn app_dir() -> PathBuf {
    #[cfg(target_os = "macos")]
    {
        home_dir()
            .join("Library")
            .join("Application Support")
            .join("Quilltap")
    }
    #[cfg(windows)]
    {
        std::env::var_os("APPDATA")
            .map(PathBuf::from)
            .unwrap_or_else(|| home_dir().join("AppData").join("Roaming"))
            .join("Quilltap")
    }
    #[cfg(not(any(target_os = "macos", windows)))]
    {
        home_dir().join(".quilltap")
    }
}

/// The system IANA timezone for cron evaluation (v4 evaluates crons in the
/// process zone). Falls back to UTC when the platform zone is unnameable.
pub fn system_timezone() -> String {
    jiff::tz::TimeZone::system()
        .iana_name()
        .unwrap_or("UTC")
        .to_string()
}
