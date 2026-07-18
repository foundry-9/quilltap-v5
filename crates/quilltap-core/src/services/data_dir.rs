//! The data-directory resolver (P4.9c) — a differential port of the
//! `lib/paths.ts` functions v4's `GET /api/v1/system/data-dir` route composes:
//! `isLimaEnvironment`, `isDockerEnvironment`, `isElectronShell`,
//! `getElectronShellVersion`, `getShellCapabilities`, `getPlatform`,
//! `getPlatformDefaultBaseDir`, `getBaseDataDirWithSource`, and
//! `getHostDataDir`.
//!
//! ## Why an env SNAPSHOT rather than reading `std::env` inline
//!
//! Every one of v4's functions reads `process.env` (plus `os.homedir()`,
//! `process.platform`, and two filesystem probes) at call time. Ported that
//! way the behavior would be untestable and undifferentiable — the oracle sets
//! an env matrix per case and Rust cannot mutate its own environment safely
//! (`std::env::set_var` is `unsafe` in the 2024 edition, and the harness runs
//! tests in threads). So the whole family is a **pure function over
//! [`DataDirEnv`]**, and [`DataDirEnv::from_process`] is the one impure edge.
//! The oracle emits the env inputs it set per case; the differential feeds the
//! identical snapshot in.
//!
//! ## The JS-truthiness rules this port must keep
//!
//! - `process.env.X` is `undefined` when unset and `""` when set empty; v4's
//!   `!!raw` / `if (envOverride)` guards treat BOTH as absent. [`truthy`]
//!   collapses them, so `Option<String>` alone is not enough.
//! - `isLimaEnvironment` / the `DOCKER_CONTAINER` probe compare `=== 'true'`
//!   STRICTLY — `"1"`, `"TRUE"`, `"yes"` are all false.
//! - `getShellCapabilities` returns a `Set`, so duplicates collapse while
//!   INSERTION order survives; `[...set]` then reaches the wire in that order.
//!
//! ## The one uncovered arm
//!
//! `getPlatformDefaultBaseDir`'s `win32` branch is exercised by the
//! differential only as v4 renders it **on a posix host** (forward slashes —
//! `path.join` uses the running platform's separator, and both sides of the
//! diff run on macOS). A real Windows host is a named uncovered divergence,
//! consistent with the standing "Windows one-origin re-checks" deferral.

use std::path::Path;

/// Everything v4's paths family reads from the ambient world, captured once.
/// Field names mirror the env vars / Node globals they stand in for.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct DataDirEnv {
    /// `process.env.LIMA_CONTAINER`.
    pub lima_container: Option<String>,
    /// `process.env.DOCKER_CONTAINER`.
    pub docker_container: Option<String>,
    /// `process.env.QUILLTAP_DATA_DIR`.
    pub quilltap_data_dir: Option<String>,
    /// `process.env.QUILLTAP_HOST_DATA_DIR`.
    pub quilltap_host_data_dir: Option<String>,
    /// `process.env.QUILLTAP_SHELL` (the Electron shell's own version string).
    pub quilltap_shell: Option<String>,
    /// `process.env.QUILLTAP_SHELL_CAPABILITIES` (comma-delimited flags).
    pub quilltap_shell_capabilities: Option<String>,
    /// `process.env.APPDATA` (the win32 default-dir base).
    pub appdata: Option<String>,
    /// `os.homedir()`.
    pub home_dir: String,
    /// `process.platform` — `"darwin" | "win32" | "linux" | …`.
    pub os_platform: String,
    /// `fs.existsSync('/.dockerenv')`.
    pub dockerenv_exists: bool,
    /// `fs.statSync('/app').isDirectory()` (a throw counts as false).
    pub app_is_dir: bool,
}

impl DataDirEnv {
    /// The one impure edge: snapshot the real process environment + the two
    /// filesystem probes, exactly as v4 reads them at call time.
    pub fn from_process() -> Self {
        Self {
            lima_container: std::env::var("LIMA_CONTAINER").ok(),
            docker_container: std::env::var("DOCKER_CONTAINER").ok(),
            quilltap_data_dir: std::env::var("QUILLTAP_DATA_DIR").ok(),
            quilltap_host_data_dir: std::env::var("QUILLTAP_HOST_DATA_DIR").ok(),
            quilltap_shell: std::env::var("QUILLTAP_SHELL").ok(),
            quilltap_shell_capabilities: std::env::var("QUILLTAP_SHELL_CAPABILITIES").ok(),
            appdata: std::env::var("APPDATA").ok(),
            home_dir: home_dir(),
            os_platform: os_platform().to_string(),
            dockerenv_exists: Path::new("/.dockerenv").exists(),
            app_is_dir: Path::new("/app").is_dir(),
        }
    }
}

/// `os.homedir()`. v4 reads `HOME` on posix and `USERPROFILE` on Windows.
fn home_dir() -> String {
    #[cfg(windows)]
    let var = std::env::var("USERPROFILE");
    #[cfg(not(windows))]
    let var = std::env::var("HOME");
    var.unwrap_or_default()
}

/// `process.platform` for the platforms v4 names.
fn os_platform() -> &'static str {
    if cfg!(target_os = "macos") {
        "darwin"
    } else if cfg!(windows) {
        "win32"
    } else {
        "linux"
    }
}

/// v4's `Platform` union (`lib/paths.ts:46`), which is NOT `process.platform` —
/// `docker` displaces the real OS, and Lima reports as plain `linux`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Platform {
    Docker,
    Linux,
    Darwin,
    Win32,
}

impl Platform {
    pub fn as_str(self) -> &'static str {
        match self {
            Platform::Docker => "docker",
            Platform::Linux => "linux",
            Platform::Darwin => "darwin",
            Platform::Win32 => "win32",
        }
    }

    /// v4's `platformDescriptions` map (`getBaseDataDirWithSource`), verbatim.
    fn description(self) -> &'static str {
        match self {
            Platform::Docker => "Docker container default (/app/quilltap)",
            Platform::Darwin => "macOS default (~/Library/Application Support/Quilltap)",
            Platform::Win32 => "Windows default (%APPDATA%\\Quilltap)",
            Platform::Linux => "Linux default (~/.quilltap)",
        }
    }
}

/// v4's `BaseDataDirSource`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BaseDataDirSource {
    Environment,
    PlatformDefault,
}

impl BaseDataDirSource {
    pub fn as_str(self) -> &'static str {
        match self {
            BaseDataDirSource::Environment => "environment",
            BaseDataDirSource::PlatformDefault => "platform-default",
        }
    }
}

/// v4's `BaseDataDirInfo`.
#[derive(Debug, Clone, PartialEq)]
pub struct BaseDataDirInfo {
    pub path: String,
    pub source: BaseDataDirSource,
    pub source_description: String,
}

/// The route's `DataDirInfo` response shape (`data-dir/route.ts:32-55`), in
/// v4's declaration order — the wire emits it in exactly this sequence.
#[derive(Debug, Clone, PartialEq)]
pub struct DataDirInfo {
    pub path: String,
    pub source: BaseDataDirSource,
    pub source_description: String,
    pub platform: Platform,
    pub is_docker: bool,
    pub is_vm: bool,
    pub is_electron_shell: bool,
    pub shell_version: Option<String>,
    pub shell_capabilities: Vec<String>,
    pub can_open: bool,
    pub host_path: String,
}

/// JS truthiness for an env var: unset AND empty-string both read as absent.
fn truthy(v: &Option<String>) -> Option<&str> {
    match v {
        Some(s) if !s.is_empty() => Some(s.as_str()),
        _ => None,
    }
}

/// v4 `isLimaEnvironment()` — a STRICT `=== 'true'` compare.
pub fn is_lima_environment(env: &DataDirEnv) -> bool {
    env.lima_container.as_deref() == Some("true")
}

/// v4 `isDockerEnvironment()` — the env flag, then `/.dockerenv`, then `/app`
/// as a DIRECTORY (the Quilltap Docker convention).
pub fn is_docker_environment(env: &DataDirEnv) -> bool {
    env.docker_container.as_deref() == Some("true") || env.dockerenv_exists || env.app_is_dir
}

/// v4 `isElectronShell()` — `!!process.env.QUILLTAP_SHELL`.
pub fn is_electron_shell(env: &DataDirEnv) -> bool {
    truthy(&env.quilltap_shell).is_some()
}

/// v4 `getElectronShellVersion()` — `process.env.QUILLTAP_SHELL || null`.
pub fn electron_shell_version(env: &DataDirEnv) -> Option<String> {
    truthy(&env.quilltap_shell).map(str::to_string)
}

/// v4 `getShellCapabilities()` — split on `,`, trim, drop empties. The `Set`
/// dedupes while KEEPING first-insertion order, which is what `[...set]`
/// serializes.
pub fn shell_capabilities(env: &DataDirEnv) -> Vec<String> {
    let Some(raw) = truthy(&env.quilltap_shell_capabilities) else {
        return Vec::new();
    };
    let mut out: Vec<String> = Vec::new();
    for part in raw.split(',') {
        let flag = part.trim();
        if flag.is_empty() {
            continue;
        }
        if !out.iter().any(|seen| seen == flag) {
            out.push(flag.to_string());
        }
    }
    out
}

/// v4 `getPlatform()`. The Lima check runs FIRST on purpose: the rootfs
/// exported from Docker still carries `/.dockerenv` and `/app`, which would
/// otherwise make a Lima VM report as Docker.
pub fn get_platform(env: &DataDirEnv) -> Platform {
    if is_lima_environment(env) {
        return Platform::Linux;
    }
    if is_docker_environment(env) {
        return Platform::Docker;
    }
    match env.os_platform.as_str() {
        "darwin" => Platform::Darwin,
        "win32" => Platform::Win32,
        // v4 falls back to linux for every other Unix-like platform.
        _ => Platform::Linux,
    }
}

/// v4 `getPlatformDefaultBaseDir()`.
pub fn platform_default_base_dir(env: &DataDirEnv) -> String {
    match get_platform(env) {
        Platform::Docker => "/app/quilltap".to_string(),
        Platform::Darwin => {
            js_path_join(&[&env.home_dir, "Library", "Application Support", "Quilltap"])
        }
        Platform::Win32 => {
            let app_data = truthy(&env.appdata)
                .map(str::to_string)
                .unwrap_or_else(|| js_path_join(&[&env.home_dir, "AppData", "Roaming"]));
            js_path_join(&[&app_data, "Quilltap"])
        }
        Platform::Linux => js_path_join(&[&env.home_dir, ".quilltap"]),
    }
}

/// v4 `getBaseDataDirWithSource()`. In Docker the env override is IGNORED —
/// the container must use `/app/quilltap` to match the configured volume mount.
pub fn base_data_dir_with_source(env: &DataDirEnv) -> BaseDataDirInfo {
    let platform = get_platform(env);

    if platform == Platform::Docker {
        return BaseDataDirInfo {
            path: platform_default_base_dir(env),
            source: BaseDataDirSource::PlatformDefault,
            source_description: platform.description().to_string(),
        };
    }

    if let Some(raw) = truthy(&env.quilltap_data_dir) {
        // v4 expands a leading `~` via `path.join(os.homedir(), raw.slice(1))`.
        // `path.join` does NOT treat an absolute-looking second segment as a
        // reset (Rust's `Path::join` DOES) — hence [`js_path_join`].
        let resolved = if let Some(rest) = raw.strip_prefix('~') {
            js_path_join(&[&env.home_dir, rest])
        } else {
            raw.to_string()
        };
        return BaseDataDirInfo {
            path: resolved,
            source: BaseDataDirSource::Environment,
            source_description: format!("QUILLTAP_DATA_DIR environment variable ({raw})"),
        };
    }

    BaseDataDirInfo {
        path: platform_default_base_dir(env),
        source: BaseDataDirSource::PlatformDefault,
        source_description: platform.description().to_string(),
    }
}

/// v4 `getHostDataDir()` — `QUILLTAP_HOST_DATA_DIR` is only meaningful inside
/// a VM/container, where the path the app sees differs from the host's.
pub fn host_data_dir(env: &DataDirEnv) -> String {
    if is_lima_environment(env) || is_docker_environment(env) {
        if let Some(override_path) = truthy(&env.quilltap_host_data_dir) {
            return override_path.to_string();
        }
    }
    base_data_dir_with_source(env).path
}

/// The whole `GET /api/v1/system/data-dir` payload (`route.ts:61-83`).
pub fn data_dir_info(env: &DataDirEnv) -> DataDirInfo {
    let dir_info = base_data_dir_with_source(env);
    let is_docker = is_docker_environment(env);
    DataDirInfo {
        path: dir_info.path,
        source: dir_info.source,
        source_description: dir_info.source_description,
        platform: get_platform(env),
        is_docker,
        // v4's `isVM` is the LIMA probe only (WSL2 rides the same flag).
        is_vm: is_lima_environment(env),
        is_electron_shell: is_electron_shell(env),
        shell_version: electron_shell_version(env),
        shell_capabilities: shell_capabilities(env),
        can_open: !is_docker,
        host_path: host_data_dir(env),
    }
}

/// Node's `path.join` for posix, which is NOT `std::path::Path::join`: an
/// absolute-looking later segment is concatenated, not treated as a new root
/// (`path.join('/home/me', '/qtdata')` → `/home/me/qtdata`, while Rust's
/// `Path::join` answers `/qtdata`). Segments are joined with `/`, empties are
/// dropped, then `.`/`..` are resolved and duplicate separators collapse.
fn js_path_join(parts: &[&str]) -> String {
    let joined: Vec<&str> = parts.iter().copied().filter(|p| !p.is_empty()).collect();
    if joined.is_empty() {
        // Node's `path.join()` with nothing to join answers '.'.
        return ".".to_string();
    }
    let absolute = joined[0].starts_with('/');
    let combined = joined.join("/");

    let mut out: Vec<&str> = Vec::new();
    for seg in combined.split('/') {
        match seg {
            "" | "." => continue,
            ".." => {
                // A leading `..` on a RELATIVE path is preserved (Node keeps it);
                // on an absolute path it is swallowed at the root.
                if matches!(out.last(), Some(&last) if last != "..") {
                    out.pop();
                } else if !absolute {
                    out.push("..");
                }
            }
            other => out.push(other),
        }
    }

    let body = out.join("/");
    if absolute {
        format!("/{body}")
    } else if body.is_empty() {
        ".".to_string()
    } else {
        body
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn base_env() -> DataDirEnv {
        DataDirEnv {
            home_dir: "/Users/fixture".to_string(),
            os_platform: "darwin".to_string(),
            ..Default::default()
        }
    }

    #[test]
    fn js_join_concatenates_absolute_looking_segments() {
        // The trap Rust's Path::join falls into.
        assert_eq!(
            js_path_join(&["/Users/fixture", "/qtdata"]),
            "/Users/fixture/qtdata"
        );
        assert_eq!(js_path_join(&["/Users/fixture", ""]), "/Users/fixture");
        assert_eq!(js_path_join(&["/a/b", "../c"]), "/a/c");
        assert_eq!(js_path_join(&["a", "..", "..", "b"]), "../b");
    }

    #[test]
    fn empty_env_vars_read_as_absent() {
        let mut env = base_env();
        env.quilltap_data_dir = Some(String::new());
        env.quilltap_shell = Some(String::new());
        let info = data_dir_info(&env);
        assert_eq!(info.source, BaseDataDirSource::PlatformDefault);
        assert!(!info.is_electron_shell);
        assert_eq!(info.shell_version, None);
    }

    #[test]
    fn docker_ignores_the_env_override() {
        let mut env = base_env();
        env.docker_container = Some("true".to_string());
        env.quilltap_data_dir = Some("/somewhere/else".to_string());
        let info = data_dir_info(&env);
        assert_eq!(info.path, "/app/quilltap");
        assert_eq!(info.platform, Platform::Docker);
        assert!(!info.can_open);
    }

    #[test]
    fn lima_wins_over_docker_markers() {
        let mut env = base_env();
        env.lima_container = Some("true".to_string());
        env.dockerenv_exists = true;
        env.app_is_dir = true;
        let info = data_dir_info(&env);
        assert_eq!(info.platform, Platform::Linux);
        assert_eq!(info.path, "/Users/fixture/.quilltap");
        // isDocker is still true — only the PLATFORM defers to Lima.
        assert!(info.is_docker);
        assert!(info.is_vm);
    }

    #[test]
    fn capabilities_trim_dedupe_and_keep_order() {
        let mut env = base_env();
        env.quilltap_shell_capabilities =
            Some(" DOWNLOADS_FILE , OPENS_FS_PATH,,DOWNLOADS_FILE ".to_string());
        assert_eq!(
            shell_capabilities(&env),
            vec!["DOWNLOADS_FILE".to_string(), "OPENS_FS_PATH".to_string()]
        );
    }

    #[test]
    fn tilde_expands_against_home() {
        let mut env = base_env();
        env.quilltap_data_dir = Some("~/qtdata".to_string());
        let info = data_dir_info(&env);
        assert_eq!(info.path, "/Users/fixture/qtdata");
        assert_eq!(
            info.source_description,
            "QUILLTAP_DATA_DIR environment variable (~/qtdata)"
        );
    }

    #[test]
    fn host_path_override_only_inside_a_vm_or_container() {
        let mut env = base_env();
        env.quilltap_host_data_dir = Some("/host/side".to_string());
        // Not in a VM/container: the override is ignored.
        assert_eq!(
            host_data_dir(&env),
            "/Users/fixture/Library/Application Support/Quilltap"
        );
        env.docker_container = Some("true".to_string());
        assert_eq!(host_data_dir(&env), "/host/side");
    }
}
