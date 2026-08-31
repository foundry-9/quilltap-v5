//! Host environment probes + the production `SelfInventoryEnv` (P4.1d).
//!
//! Two distinct v4 probe families live here — do not merge them:
//!
//! - [`detect_environment_type`] — the LOCK's probe
//!   (`instance-lock.ts detectEnvironmentType()`): electron → docker
//!   (`DOCKER_CONTAINER` / `/.dockerenv`) → local. v5 emits no `electron` (no
//!   Electron shell in this host); the variant exists so a v4-written lock
//!   file still parses/classifies.
//! - [`is_docker_environment`] — `lib/paths.ts`'s probe, consumed by
//!   `resolveRuntimeMode()`. NOTE the paths.ts docker probe additionally
//!   treats an `/app` DIRECTORY as Docker (the Quilltap Docker convention) —
//!   the lock's probe does not.
//!
//! v4 `1560bd43b` retired the managed Lima (macOS) and WSL2 (Windows) VM
//! modes: `isLimaEnvironment()` is gone, `EnvironmentType` is down to three,
//! and the `lima`/`wsl2` detection branches were deleted rather than
//! reassigned. A lock file written by a pre-4.9 v4 running in one of those VMs
//! can still be on disk, so [`EnvironmentType::Other`] keeps v4's own
//! tolerance: v4's type is a bare TS union over an UNVALIDATED JSON field
//! (`readLockFile` shape-checks only pid/hostname/history), so `"lima"` still
//! parses there and simply fails every `=== 'docker'` / `=== 'electron'`
//! test.
//!
//! [`SelfInventoryEnv::production`] fills the core's injected
//! `self_inventory` environment seam from host reality: the host version, the
//! runtime-mode probes, the browser client shell, the release-notes/changelog
//! file reads (v4 `findReleaseNotesFile` over `docs/releases/*.md` +
//! `docs/CHANGELOG.md`), the mount-index degraded state derived from the
//! assembled `Db`, and the model-context registry data.
//!
//! ## Documented seams (the flat model-context env)
//!
//! The core's `SelfInventoryEnv` carries ONE flat `model_info` list, ONE
//! fallback-pricing list, and ONE registry default — v4's
//! `getModelContextLimit` consults all three PER PROVIDER. The production
//! values here:
//! - `model_info = []` — the provider manifests carry no per-model static
//!   lists (v4's plugin `getModelInfo()` data is unported W4.7 surface);
//! - `fallback_pricing` = every provider's `LEGACY_FALLBACK_PRICING` rows
//!   flattened (model ids are provider-distinctive strings, so cross-provider
//!   substring collisions do not occur over the real rows);
//! - `registry_default_context = 8192` — the lookup's "no registry answer"
//!   sentinel, which falls through to the ported per-provider
//!   `DEFAULT_CONTEXT_BY_PROVIDER` table. Seam: DEEPSEEK / Z_AI (manifest
//!   default 131072, absent from that table) resolve 8192 here until the env
//!   becomes provider-aware (a tools/self_inventory follow-up).

use std::path::Path;

use serde::{Deserialize, Serialize};

use quilltap_core::db::runtime::Db;
use quilltap_core::model_context::PricingRow;
use quilltap_core::provider_manifest::Registry;
use quilltap_core::services::pricing_fetcher::fallback_pricing;
use quilltap_core::tools::self_inventory::{ClientShell, SelfInventoryEnv};

// ============================================================================
// Environment probes
// ============================================================================

/// v4 `EnvironmentType` (`'local' | 'electron' | 'docker'` since `1560bd43b`).
/// v5 never emits `Electron`; it parses (v4-written lock files).
///
/// [`EnvironmentType::Other`] is the retired-vocabulary tolerance described in
/// the module header: a pre-4.9 `"lima"` / `"wsl2"` lock round-trips as its own
/// string and, like every unrecognized value in v4, matches neither `docker`
/// nor `electron`. It is a READ shape only — no probe ever produces it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EnvironmentType {
    Local,
    Electron,
    Docker,
    Other(String),
}

impl EnvironmentType {
    pub fn as_str(&self) -> &str {
        match self {
            EnvironmentType::Local => "local",
            EnvironmentType::Electron => "electron",
            EnvironmentType::Docker => "docker",
            EnvironmentType::Other(raw) => raw.as_str(),
        }
    }

    fn from_str(raw: &str) -> Self {
        match raw {
            "local" => EnvironmentType::Local,
            "electron" => EnvironmentType::Electron,
            "docker" => EnvironmentType::Docker,
            other => EnvironmentType::Other(other.to_string()),
        }
    }

    /// v4's post-`1560bd43b` container test: `environment === 'docker'`, the
    /// one value whose cross-host heartbeat is trusted.
    pub fn is_container(&self) -> bool {
        matches!(self, EnvironmentType::Docker)
    }
}

impl Serialize for EnvironmentType {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(self.as_str())
    }
}

impl<'de> Deserialize<'de> for EnvironmentType {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        Ok(EnvironmentType::from_str(&String::deserialize(d)?))
    }
}

fn env_is(name: &str, want: &str) -> bool {
    std::env::var(name).map(|v| v == want).unwrap_or(false)
}

/// v4 `isDockerEnvironment()` (`lib/paths.ts:92`): `DOCKER_CONTAINER === 'true'`,
/// `/.dockerenv`, or an `/app` DIRECTORY (the Quilltap Docker convention).
pub fn is_docker_environment() -> bool {
    if env_is("DOCKER_CONTAINER", "true") {
        return true;
    }
    if Path::new("/.dockerenv").exists() {
        return true;
    }
    std::fs::metadata("/app")
        .map(|m| m.is_dir())
        .unwrap_or(false)
}

/// The LOCK's environment probe (v4 `instance-lock.ts detectEnvironmentType()`,
/// minus the Electron branches — v5 has no Electron shell): docker
/// (`DOCKER_CONTAINER` / `/.dockerenv` — NO `/app` check in this probe) →
/// local. v4 `1560bd43b` deleted the `LIMA_CONTAINER` and `WSL_DISTRO_NAME`
/// branches that used to run ahead of the Docker probe.
pub fn detect_environment_type() -> EnvironmentType {
    if env_is("DOCKER_CONTAINER", "true") || Path::new("/.dockerenv").exists() {
        return EnvironmentType::Docker;
    }
    EnvironmentType::Local
}

/// v4 `resolveRuntimeMode()` (`self-inventory/builders.ts:689`), for a host
/// with no Electron shell: `docker` → `local-dev` / `local-production`. v4
/// `1560bd43b` deleted the `vm` and `electron-vm` modes with the Lima probe.
/// `dev` is the caller's development flag (v4 `isDevelopment` =
/// `NODE_ENV === 'development'`; the host passes `cfg!(debug_assertions)`).
pub fn resolve_runtime_mode(dev: bool) -> String {
    if is_docker_environment() {
        return "docker".to_string();
    }
    if dev {
        "local-dev".to_string()
    } else {
        "local-production".to_string()
    }
}

// ============================================================================
// Release notes / changelog (v4 `findReleaseNotesFile` + the changelog read)
// ============================================================================

/// v4 `parseSemanticVersion`: strip a `-suffix`, split on `.`, parts as
/// numbers with missing → 0. `None` when a part is non-numeric (v4's `NaN`).
fn parse_semver(version: &str) -> Option<(i64, i64, i64)> {
    let base = version.split('-').next().unwrap_or("");
    let mut parts = [0i64; 3];
    for (i, p) in base.split('.').enumerate().take(3) {
        if p.is_empty() {
            continue; // JS Number('') === 0 — an empty part reads as 0.
        }
        parts[i] = p.parse::<i64>().ok()?;
    }
    Some((parts[0], parts[1], parts[2]))
}

/// v4 `findReleaseNotesFile(version)` over `<docs_dir>/releases/*.md`: parse
/// each stem as `major.minor.patch`, keep releases ≤ the current version,
/// pick the newest, and read it. Returns `(body, release_version)`.
pub fn find_release_notes(docs_dir: &Path, version: &str) -> Option<(String, String)> {
    let releases_dir = docs_dir.join("releases");
    let entries = std::fs::read_dir(&releases_dir).ok()?;
    let (t_major, t_minor, t_patch) = parse_semver(version)?;

    let mut candidates: Vec<(i64, i64, i64, String, std::path::PathBuf)> = Vec::new();
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        let Some(stem) = name.strip_suffix(".md") else {
            continue;
        };
        // v4: parts = stem.split('.').map(Number); skip if any NaN.
        let Some((major, minor, patch)) = parse_semver(stem) else {
            continue;
        };
        let le_target = major < t_major
            || (major == t_major && minor < t_minor)
            || (major == t_major && minor == t_minor && patch <= t_patch);
        if le_target {
            candidates.push((major, minor, patch, stem.to_string(), entry.path()));
        }
    }
    if candidates.is_empty() {
        return None;
    }
    candidates.sort_by_key(|c| std::cmp::Reverse((c.0, c.1, c.2)));
    let (_, _, _, stem, path) = &candidates[0];
    let body = std::fs::read_to_string(path).ok()?;
    Some((body, stem.clone()))
}

/// Read `<docs_dir>/CHANGELOG.md`, `None` when absent (v4's "not found").
pub fn read_changelog(docs_dir: &Path) -> Option<String> {
    std::fs::read_to_string(docs_dir.join("CHANGELOG.md")).ok()
}

// ============================================================================
// The production SelfInventoryEnv
// ============================================================================

/// v4 `isMountIndexDegraded()`: the mount-index DB failed to open. Derived
/// from the assembled `Db` — the mount-index partition being unavailable (or
/// unreadable) is degraded.
pub fn mount_index_degraded(db: &Db) -> bool {
    db.read_mount_index(|_conn| Ok(())).is_err()
}

/// Build the production `SelfInventoryEnv` for this host (the browser/web
/// shell). `docs_dir` is where the app bundle's `releases/` + `CHANGELOG.md`
/// live (absent files → `None`, v4's "not found"). See the module header for
/// the model-context seams.
pub fn production_self_inventory_env(
    version: &str,
    docs_dir: Option<&Path>,
    db: &Db,
) -> SelfInventoryEnv {
    let mut rows: Vec<PricingRow> = Vec::new();
    for provider in Registry::built_in().provider_names() {
        for p in fallback_pricing(provider) {
            rows.push(PricingRow {
                model_id: p.model_id,
                context_length: p.context_length,
            });
        }
    }
    SelfInventoryEnv {
        version: version.to_string(),
        runtime_mode: resolve_runtime_mode(cfg!(debug_assertions)),
        client_shell: ClientShell::Browser,
        mount_index_degraded: mount_index_degraded(db),
        release_notes: docs_dir.and_then(|d| find_release_notes(d, version)),
        changelog: docs_dir.and_then(read_changelog),
        model_info: Vec::new(),
        fallback_pricing: rows,
        registry_default_context: 8192,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn semver_parse_matches_v4() {
        assert_eq!(parse_semver("1.2.3"), Some((1, 2, 3)));
        assert_eq!(parse_semver("1.2"), Some((1, 2, 0)));
        assert_eq!(parse_semver("1.2.3-beta.1"), Some((1, 2, 3)));
        assert_eq!(parse_semver("abc"), None);
    }

    #[test]
    fn release_notes_picks_newest_at_or_below() {
        let dir = std::env::temp_dir().join(format!(
            "qt-env-test-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let releases = dir.join("releases");
        std::fs::create_dir_all(&releases).unwrap();
        std::fs::write(releases.join("1.0.0.md"), "one").unwrap();
        std::fs::write(releases.join("1.2.0.md"), "one-two").unwrap();
        std::fs::write(releases.join("2.0.0.md"), "two").unwrap();
        std::fs::write(releases.join("notes.md"), "not a version").unwrap();

        // Newest ≤ 1.5.0 is 1.2.0.
        assert_eq!(
            find_release_notes(&dir, "1.5.0"),
            Some(("one-two".to_string(), "1.2.0".to_string()))
        );
        // Exact match counts (patch <= patch).
        assert_eq!(
            find_release_notes(&dir, "2.0.0"),
            Some(("two".to_string(), "2.0.0".to_string()))
        );
        // Below every release → none.
        assert_eq!(find_release_notes(&dir, "0.1.0"), None);
        // No releases dir → none.
        assert_eq!(find_release_notes(&dir.join("nope"), "1.0.0"), None);
        // Changelog read.
        assert_eq!(read_changelog(&dir), None);
        std::fs::write(dir.join("CHANGELOG.md"), "log").unwrap();
        assert_eq!(read_changelog(&dir), Some("log".to_string()));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn fallback_rows_flatten_nonempty() {
        // Sanity over the generated static: the four populated providers'
        // rows all flatten in (ANTHROPIC/OPENAI/GOOGLE/GROK have rows; the
        // other three are intentionally empty).
        let mut n = 0;
        for provider in Registry::built_in().provider_names() {
            n += fallback_pricing(provider).len();
        }
        assert!(n >= 18, "expected the 18 legacy rows, got {n}");
    }
}
