//! The instance registry — the **read path** of the v4 launcher's
//! `packages/quilltap/lib/instances.js`: a per-user `instances.json` mapping
//! friendly names → (instance root path, optional database passphrase).
//!
//! Because the file may contain plaintext passphrases, the read path refuses
//! to load it when the POSIX safety invariants are broken (wrong owner, or
//! group/other permission bits set) — v4 `assertSafePermissions`, ported
//! message-for-message in spirit. Listing never exposes a stored passphrase,
//! only whether one exists.
//!
//! The write verbs (`add`/`remove`/`set-passphrase`/`default`/`rename`) are
//! P4.3 CLI work.

use std::path::PathBuf;

use quilltap_core::api::{InstanceDirectory, InstanceDto, InstancesDto};

/// Registry schema version this reader accepts (v4 `SCHEMA_VERSION`).
const SCHEMA_VERSION: u64 = 1;

/// Reads `instances.json`. Construct with [`InstanceRegistry::at_default_location`]
/// for the launcher's per-user path, or [`InstanceRegistry::at`] for tests.
pub struct InstanceRegistry {
    path: PathBuf,
}

impl InstanceRegistry {
    pub fn at_default_location() -> Self {
        Self {
            path: crate::paths::app_dir().join("instances.json"),
        }
    }

    pub fn at(path: impl Into<PathBuf>) -> Self {
        Self { path: path.into() }
    }

    /// v4 `assertSafePermissions`: refuse a passphrase-bearing file that other
    /// users could read or that we don't own. Windows accepts (no POSIX bits;
    /// the user-profile location limits access).
    fn assert_safe_permissions(&self) -> Result<(), String> {
        #[cfg(unix)]
        {
            use std::os::unix::fs::MetadataExt;
            let meta = std::fs::metadata(&self.path)
                .map_err(|e| format!("Failed to stat {}: {e}", self.path.display()))?;
            // SAFETY: geteuid has no failure modes.
            let euid = unsafe { libc_geteuid() };
            if meta.uid() != euid {
                return Err(format!(
                    "Refusing to read {}: file is owned by uid {}, but current process is uid {}. \
                     Either delete the file or run: sudo chown {} \"{}\"",
                    self.path.display(),
                    meta.uid(),
                    euid,
                    euid,
                    self.path.display()
                ));
            }
            let perms = meta.mode() & 0o777;
            if perms & 0o077 != 0 {
                return Err(format!(
                    "Refusing to read {}: permissions are {:03o} (group/other can access). \
                     Quilltap stores passphrases in this file. Restrict it with: chmod 600 \"{}\"",
                    self.path.display(),
                    perms,
                    self.path.display()
                ));
            }
        }
        Ok(())
    }
}

// Minimal libc-crate-free geteuid: `std::os::unix::fs::MetadataExt` gives the
// file uid, but the process euid needs a syscall. Rather than pull the `libc`
// crate for one call, declare the POSIX symbol directly.
#[cfg(unix)]
unsafe fn libc_geteuid() -> u32 {
    extern "C" {
        fn geteuid() -> u32;
    }
    geteuid()
}

impl InstanceDirectory for InstanceRegistry {
    fn list(&self) -> Result<InstancesDto, String> {
        // Missing file → the empty registry (v4 `emptyRegistry()`).
        if !self.path.exists() {
            return Ok(InstancesDto {
                instances: vec![],
                default_instance: None,
            });
        }
        self.assert_safe_permissions()?;

        let raw = std::fs::read_to_string(&self.path)
            .map_err(|e| format!("Failed to read {}: {e}", self.path.display()))?;
        let parsed: serde_json::Value = serde_json::from_str(&raw)
            .map_err(|e| format!("Failed to parse {}: {e}", self.path.display()))?;
        let obj = parsed.as_object().ok_or_else(|| {
            format!(
                "Invalid contents in {}: expected a JSON object.",
                self.path.display()
            )
        })?;

        // v4's reader tolerates a missing version (defaults it); an explicit
        // FUTURE version is refused rather than misread.
        if let Some(v) = obj.get("version").and_then(|v| v.as_u64()) {
            if v > SCHEMA_VERSION {
                return Err(format!(
                    "Unsupported instances.json schema version {v} (this build understands {SCHEMA_VERSION})."
                ));
            }
        }

        let default_instance = obj
            .get("defaultInstance")
            .and_then(|v| v.as_str())
            .map(str::to_string);

        // Insertion order preserved (serde_json preserve_order), matching
        // v4's Object.entries listing.
        let mut instances = Vec::new();
        if let Some(map) = obj.get("instances").and_then(|v| v.as_object()) {
            for (name, entry) in map {
                let path = entry
                    .get("path")
                    .and_then(|v| v.as_str())
                    .unwrap_or_default()
                    .to_string();
                let has_passphrase = entry
                    .get("passphrase")
                    .and_then(|v| v.as_str())
                    .is_some_and(|p| !p.is_empty());
                instances.push(InstanceDto {
                    name: name.clone(),
                    path,
                    is_default: Some(name.as_str()) == default_instance.as_deref(),
                    has_passphrase,
                });
            }
        }

        Ok(InstancesDto {
            instances,
            default_instance,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write_registry(dir: &tempfile::TempDir, contents: &str, mode: u32) -> PathBuf {
        let path = dir.path().join("instances.json");
        std::fs::write(&path, contents).unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(mode)).unwrap();
        }
        #[cfg(not(unix))]
        let _ = mode;
        path
    }

    #[test]
    fn missing_file_is_empty_registry() {
        let dir = tempfile::tempdir().unwrap();
        let reg = InstanceRegistry::at(dir.path().join("instances.json"));
        let dto = reg.list().unwrap();
        assert!(dto.instances.is_empty());
        assert_eq!(dto.default_instance, None);
    }

    #[test]
    fn lists_instances_with_default_and_passphrase_flags() {
        let dir = tempfile::tempdir().unwrap();
        let path = write_registry(
            &dir,
            r#"{
              "version": 1,
              "instances": {
                "Friday": { "path": "~/iCloud/Quilltap/Friday", "passphrase": "s3cret" },
                "Ignite": { "path": "/tmp/ignite" }
              },
              "defaultInstance": "Friday"
            }"#,
            0o600,
        );
        let dto = InstanceRegistry::at(path).list().unwrap();
        assert_eq!(dto.default_instance.as_deref(), Some("Friday"));
        assert_eq!(dto.instances.len(), 2);
        assert_eq!(dto.instances[0].name, "Friday");
        assert!(dto.instances[0].is_default);
        assert!(dto.instances[0].has_passphrase);
        assert_eq!(dto.instances[1].name, "Ignite");
        assert!(!dto.instances[1].is_default);
        assert!(!dto.instances[1].has_passphrase);
    }

    #[cfg(unix)]
    #[test]
    fn group_readable_registry_is_refused() {
        let dir = tempfile::tempdir().unwrap();
        let path = write_registry(&dir, r#"{"version":1,"instances":{}}"#, 0o644);
        let err = InstanceRegistry::at(path).list().unwrap_err();
        assert!(err.contains("group/other can access"), "{err}");
    }

    #[test]
    fn future_schema_version_is_refused() {
        let dir = tempfile::tempdir().unwrap();
        let path = write_registry(&dir, r#"{"version":2,"instances":{}}"#, 0o600);
        let err = InstanceRegistry::at(path).list().unwrap_err();
        assert!(err.contains("Unsupported"), "{err}");
    }
}
