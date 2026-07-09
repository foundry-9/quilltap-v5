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
//! The write verbs (`add`/`remove`/`set-passphrase`/`default`/`rename`) —
//! P4.3 — operate on the registry as an insertion-ordered `serde_json::Value`
//! so unknown fields and entry key order survive a round-trip, and write
//! atomically (tmp file at mode 0600, then rename — v4 `writeInstances`).
//! Every error string is v4's, message-for-message.

use std::path::{Path, PathBuf};

use quilltap_core::api::{InstanceDirectory, InstanceDto, InstancesDto};
use serde_json::{Map, Value};

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

// ============================================================================
// P4.3 additions: the v4 registry surface (read + write verbs), verbatim from
// `packages/quilltap/lib/instances.js`.
// ============================================================================

/// v4 `resolveInstance`'s return: the stored key (case-preserved), the
/// EXPANDED absolute path, and the stored passphrase ('' when none).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedInstance {
    pub name: String,
    pub path: String,
    pub passphrase: String,
}

/// v4 `listInstances()` row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InstanceEntry {
    pub name: String,
    pub path: String,
    pub expanded_path: String,
    pub has_passphrase: bool,
    pub is_default: bool,
}

/// v4 `verifyPassphrase`'s four outcomes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PassphraseCheck {
    /// passphrase unlocks the encrypted pepper
    Valid,
    /// dbkey requires a user passphrase but this one doesn't decrypt
    Wrong,
    /// no .dbkey on disk yet (first-run instance)
    NoDbkey,
    /// dbkey is unlocked by the internal passphrase, no user one needed
    NoEncryption,
}

impl PassphraseCheck {
    /// The v4 string form (`'valid' | 'wrong' | 'no-dbkey' | 'no-encryption'`).
    pub fn as_str(self) -> &'static str {
        match self {
            PassphraseCheck::Valid => "valid",
            PassphraseCheck::Wrong => "wrong",
            PassphraseCheck::NoDbkey => "no-dbkey",
            PassphraseCheck::NoEncryption => "no-encryption",
        }
    }
}

fn home_dir() -> PathBuf {
    #[cfg(windows)]
    let var = std::env::var_os("USERPROFILE");
    #[cfg(not(windows))]
    let var = std::env::var_os("HOME");
    var.map(PathBuf::from).unwrap_or_else(|| PathBuf::from("."))
}

/// Node `path.normalize` for an absolute POSIX path: collapse `//`, resolve
/// `.` and `..` textually, strip the trailing separator (except root).
fn normalize_absolute(p: &str) -> String {
    let mut parts: Vec<&str> = Vec::new();
    for seg in p.split('/') {
        match seg {
            "" | "." => {}
            ".." => {
                parts.pop();
            }
            s => parts.push(s),
        }
    }
    let joined = parts.join("/");
    format!("/{joined}")
}

/// v4 `expandPath`: `~` → homedir + rest, then `path.resolve` (cwd-anchored
/// for relative input, normalized).
pub fn expand_path(input: &str) -> String {
    if input.is_empty() {
        return input.to_string();
    }
    let mut p = input.to_string();
    if p.starts_with('~') {
        // v4: path.join(os.homedir(), p.slice(1))
        let home = home_dir().to_string_lossy().into_owned();
        let rest = &p[1..];
        p = if rest.is_empty() {
            home
        } else {
            format!(
                "{}/{}",
                home.trim_end_matches('/'),
                rest.trim_start_matches('/')
            )
        };
    }
    if !p.starts_with('/') {
        let cwd = std::env::current_dir()
            .map(|c| c.to_string_lossy().into_owned())
            .unwrap_or_else(|_| "/".to_string());
        p = format!("{cwd}/{p}");
    }
    normalize_absolute(&p)
}

impl InstanceRegistry {
    /// The registry file path (v4 `getInstancesPath()` for the default
    /// location; whatever the constructor was given otherwise).
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// v4 `emptyRegistry()`.
    fn empty_registry() -> Map<String, Value> {
        let mut m = Map::new();
        m.insert("version".into(), Value::from(SCHEMA_VERSION));
        m.insert("instances".into(), Value::Object(Map::new()));
        m.insert("defaultInstance".into(), Value::Null);
        m
    }

    /// v4 `readInstances()`: missing → empty registry; permission-checked;
    /// shape-normalized (instances object ensured, version defaulted,
    /// defaultInstance key ensured). Errors are v4's messages.
    pub fn read_registry(&self) -> Result<Map<String, Value>, String> {
        if !self.path.exists() {
            return Ok(Self::empty_registry());
        }
        self.assert_safe_permissions()?;
        let raw = std::fs::read_to_string(&self.path)
            .map_err(|e| format!("Failed to read {}: {e}", self.path.display()))?;
        let parsed: Value = serde_json::from_str(&raw)
            .map_err(|e| format!("Failed to parse {}: {e}", self.path.display()))?;
        let Value::Object(mut obj) = parsed else {
            return Err(format!(
                "Invalid contents in {}: expected a JSON object.",
                self.path.display()
            ));
        };
        if !obj.get("instances").is_some_and(Value::is_object) {
            obj.insert("instances".into(), Value::Object(Map::new()));
        }
        // v4: `if (!parsed.version) parsed.version = SCHEMA_VERSION` (falsy).
        let version_falsy = match obj.get("version") {
            None | Some(Value::Null) => true,
            Some(Value::Number(n)) => n.as_f64() == Some(0.0),
            Some(Value::String(s)) => s.is_empty(),
            Some(Value::Bool(b)) => !b,
            _ => false,
        };
        if version_falsy {
            obj.insert("version".into(), Value::from(SCHEMA_VERSION));
        }
        if !obj.contains_key("defaultInstance") {
            obj.insert("defaultInstance".into(), Value::Null);
        }
        Ok(obj)
    }

    /// v4 `writeInstances()`: `{version, instances, defaultInstance}` in that
    /// key order, pretty-2-space + trailing newline, written to a tmp file at
    /// mode 0600 then renamed into place (and re-chmodded defensively).
    pub fn write_registry(&self, registry: &Map<String, Value>) -> Result<PathBuf, String> {
        if let Some(dir) = self.path.parent() {
            if !dir.exists() {
                std::fs::create_dir_all(dir)
                    .map_err(|e| format!("Failed to create {}: {e}", dir.display()))?;
            }
        }
        let mut payload = Map::new();
        payload.insert("version".into(), Value::from(SCHEMA_VERSION));
        payload.insert(
            "instances".into(),
            registry
                .get("instances")
                .cloned()
                .unwrap_or(Value::Object(Map::new())),
        );
        // v4: `defaultInstance: registry.defaultInstance || null` (falsy → null).
        let default = match registry.get("defaultInstance") {
            Some(Value::String(s)) if !s.is_empty() => Value::from(s.clone()),
            _ => Value::Null,
        };
        payload.insert("defaultInstance".into(), default);

        let json = serde_json::to_string_pretty(&Value::Object(payload))
            .map_err(|e| format!("serialize instances.json: {e}"))?;
        let tmp = self.path.with_file_name(format!(
            "{}.tmp-{}-{}",
            self.path
                .file_name()
                .map(|s| s.to_string_lossy().into_owned())
                .unwrap_or_default(),
            std::process::id(),
            quilltap_core::clock::now_unix_ms()
        ));
        std::fs::write(&tmp, format!("{json}\n"))
            .map_err(|e| format!("Failed to write {}: {e}", tmp.display()))?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&tmp, std::fs::Permissions::from_mode(0o600))
                .map_err(|e| format!("Failed to chmod {}: {e}", tmp.display()))?;
        }
        std::fs::rename(&tmp, &self.path)
            .map_err(|e| format!("Failed to rename {}: {e}", tmp.display()))?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&self.path, std::fs::Permissions::from_mode(0o600))
                .map_err(|e| format!("Failed to chmod {}: {e}", self.path.display()))?;
        }
        Ok(self.path.clone())
    }

    /// v4 `findInstanceKey`: case-insensitive lookup returning the stored key.
    fn find_instance_key(registry: &Map<String, Value>, name: &str) -> Option<String> {
        if name.is_empty() {
            return None;
        }
        let lower = name.to_lowercase();
        registry
            .get("instances")
            .and_then(Value::as_object)
            .and_then(|m| m.keys().find(|k| k.to_lowercase() == lower).cloned())
    }

    fn unknown_instance_error(registry: &Map<String, Value>, name: &str) -> String {
        let known: Vec<&String> = registry
            .get("instances")
            .and_then(Value::as_object)
            .map(|m| m.keys().collect())
            .unwrap_or_default();
        let hint = if known.is_empty() {
            " No instances are registered yet — use `quilltap instances add <name>`.".to_string()
        } else {
            format!(
                " Known instances: {}.",
                known
                    .iter()
                    .map(|s| s.as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            )
        };
        format!("Unknown instance \"{name}\".{hint}")
    }

    /// v4 `resolveInstance(name)`.
    pub fn resolve_instance(&self, name: &str) -> Result<ResolvedInstance, String> {
        let registry = self.read_registry()?;
        let Some(key) = Self::find_instance_key(&registry, name) else {
            return Err(Self::unknown_instance_error(&registry, name));
        };
        let entry = registry
            .get("instances")
            .and_then(Value::as_object)
            .and_then(|m| m.get(&key))
            .cloned()
            .unwrap_or(Value::Null);
        let raw_path = entry
            .get("path")
            .and_then(Value::as_str)
            .unwrap_or_default();
        let passphrase = entry
            .get("passphrase")
            .and_then(Value::as_str)
            .unwrap_or_default();
        Ok(ResolvedInstance {
            name: key,
            path: expand_path(raw_path),
            passphrase: passphrase.to_string(),
        })
    }

    /// v4 `listInstances()` (insertion order, passphrase presence only).
    pub fn list_instances(&self) -> Result<Vec<InstanceEntry>, String> {
        let registry = self.read_registry()?;
        let default_name = registry
            .get("defaultInstance")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string();
        let mut out = Vec::new();
        if let Some(map) = registry.get("instances").and_then(Value::as_object) {
            for (name, entry) in map {
                let path = entry
                    .get("path")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_string();
                let has_passphrase = entry
                    .get("passphrase")
                    .and_then(Value::as_str)
                    .is_some_and(|p| !p.is_empty());
                out.push(InstanceEntry {
                    name: name.clone(),
                    expanded_path: expand_path(&path),
                    path,
                    has_passphrase,
                    is_default: *name == default_name,
                });
            }
        }
        Ok(out)
    }

    /// v4 `upsertInstance(name, { instancePath, passphrase })`. A re-add
    /// without a passphrase drops any stored one (v4's net effect — the
    /// preserve-spread in the source is dead code).
    pub fn upsert_instance(
        &self,
        name: &str,
        instance_path: &str,
        passphrase: &str,
    ) -> Result<String, String> {
        if name.trim().is_empty() {
            return Err("Instance name is required.".to_string());
        }
        if instance_path.trim().is_empty() {
            return Err("Instance path is required.".to_string());
        }
        let mut registry = self.read_registry()?;
        let key =
            Self::find_instance_key(&registry, name).unwrap_or_else(|| name.trim().to_string());
        let mut entry = Map::new();
        entry.insert("path".into(), Value::from(instance_path.trim()));
        if !passphrase.is_empty() {
            entry.insert("passphrase".into(), Value::from(passphrase));
        }
        registry
            .entry("instances")
            .or_insert_with(|| Value::Object(Map::new()))
            .as_object_mut()
            .expect("instances object ensured by read_registry")
            .insert(key.clone(), Value::Object(entry));
        self.write_registry(&registry)?;
        Ok(key)
    }

    /// v4 `removeInstance(name)`.
    pub fn remove_instance(&self, name: &str) -> Result<String, String> {
        let mut registry = self.read_registry()?;
        let Some(key) = Self::find_instance_key(&registry, name) else {
            return Err(format!("Unknown instance \"{name}\"."));
        };
        if let Some(m) = registry.get_mut("instances").and_then(Value::as_object_mut) {
            m.shift_remove(&key);
        }
        self.write_registry(&registry)?;
        Ok(key)
    }

    /// v4 `setInstancePassphrase(name, passphrase)` — empty clears.
    pub fn set_instance_passphrase(&self, name: &str, passphrase: &str) -> Result<String, String> {
        let mut registry = self.read_registry()?;
        let Some(key) = Self::find_instance_key(&registry, name) else {
            return Err(format!("Unknown instance \"{name}\"."));
        };
        if let Some(entry) = registry
            .get_mut("instances")
            .and_then(Value::as_object_mut)
            .and_then(|m| m.get_mut(&key))
            .and_then(Value::as_object_mut)
        {
            if passphrase.is_empty() {
                entry.shift_remove("passphrase");
            } else {
                entry.insert("passphrase".into(), Value::from(passphrase));
            }
        }
        self.write_registry(&registry)?;
        Ok(key)
    }

    /// v4 `setDefaultInstance(name)` (unknown → the with-known-list error).
    pub fn set_default_instance(&self, name: &str) -> Result<String, String> {
        let mut registry = self.read_registry()?;
        let Some(key) = Self::find_instance_key(&registry, name) else {
            return Err(Self::unknown_instance_error(&registry, name));
        };
        registry.insert("defaultInstance".into(), Value::from(key.clone()));
        self.write_registry(&registry)?;
        Ok(key)
    }

    /// v4 `clearDefaultInstance()`.
    pub fn clear_default_instance(&self) -> Result<(), String> {
        let mut registry = self.read_registry()?;
        registry.insert("defaultInstance".into(), Value::Null);
        self.write_registry(&registry)?;
        Ok(())
    }

    /// v4 `getDefaultInstance()`.
    pub fn get_default_instance(&self) -> Result<Option<String>, String> {
        let registry = self.read_registry()?;
        Ok(registry
            .get("defaultInstance")
            .and_then(Value::as_str)
            .filter(|s| !s.is_empty())
            .map(str::to_string))
    }

    /// v4 `renameInstance(oldName, newName)` — preserves the entry (incl. the
    /// passphrase) but appends it at the end of the map, exactly as v4's
    /// delete-then-assign does.
    pub fn rename_instance(
        &self,
        old_name: &str,
        new_name: &str,
    ) -> Result<(String, String), String> {
        if old_name.trim().is_empty() {
            return Err("Old instance name is required.".to_string());
        }
        if new_name.trim().is_empty() {
            return Err("New instance name is required.".to_string());
        }
        let mut registry = self.read_registry()?;
        let Some(old_key) = Self::find_instance_key(&registry, old_name) else {
            return Err(format!("Unknown instance \"{old_name}\"."));
        };
        let new_trimmed = new_name.trim().to_string();
        if Self::find_instance_key(&registry, &new_trimmed).is_some() {
            return Err(format!("Instance \"{new_trimmed}\" already exists."));
        }
        let entry = registry
            .get_mut("instances")
            .and_then(Value::as_object_mut)
            .and_then(|m| m.shift_remove(&old_key))
            .unwrap_or(Value::Null);
        registry
            .get_mut("instances")
            .and_then(Value::as_object_mut)
            .expect("instances object ensured by read_registry")
            .insert(new_trimmed.clone(), entry);
        if registry.get("defaultInstance").and_then(Value::as_str) == Some(old_key.as_str()) {
            registry.insert("defaultInstance".into(), Value::from(new_trimmed.clone()));
        }
        self.write_registry(&registry)?;
        Ok((old_key, new_trimmed))
    }
}

/// v4 `verifyPassphrase(instanceRoot, passphrase)`: check a candidate
/// passphrase against `<root>/data/quilltap.dbkey` via the ported
/// `quilltap-core::dbkey` reader. Read/parse faults surface as `Err` (v4
/// throws there too).
pub fn verify_passphrase(instance_root: &str, passphrase: &str) -> Result<PassphraseCheck, String> {
    use quilltap_core::dbkey::{self, DbKeyError};
    let data_dir = PathBuf::from(expand_path(instance_root)).join("data");
    if !data_dir.join("quilltap.dbkey").exists() {
        return Ok(PassphraseCheck::NoDbkey);
    }
    // Internal passphrase first — success means no user passphrase is needed.
    match dbkey::load_pepper(&data_dir, None) {
        Ok(_) => Ok(PassphraseCheck::NoEncryption),
        Err(DbKeyError::PassphraseRequired) => {
            match dbkey::load_pepper(&data_dir, Some(passphrase)) {
                Ok(_) => Ok(PassphraseCheck::Valid),
                Err(DbKeyError::DecryptFailed) => Ok(PassphraseCheck::Wrong),
                Err(e) => Err(e.to_string()),
            }
        }
        Err(e) => Err(e.to_string()),
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

    #[test]
    fn crud_round_trip_matches_v4_shapes() {
        let dir = tempfile::tempdir().unwrap();
        let reg = InstanceRegistry::at(dir.path().join("instances.json"));

        // Upsert two, set a default, list.
        assert_eq!(
            reg.upsert_instance("Friday", "/tmp/friday", "").unwrap(),
            "Friday"
        );
        assert_eq!(
            reg.upsert_instance("Ignite", "/tmp/ignite", "s3cret")
                .unwrap(),
            "Ignite"
        );
        assert_eq!(reg.set_default_instance("friday").unwrap(), "Friday");
        let entries = reg.list_instances().unwrap();
        assert_eq!(entries.len(), 2);
        assert!(entries[0].is_default && !entries[0].has_passphrase);
        assert!(entries[1].has_passphrase);

        // The file is v4's exact payload shape (key order + trailing newline).
        let raw = std::fs::read_to_string(reg.path()).unwrap();
        assert!(raw.starts_with("{\n  \"version\": 1,\n  \"instances\": {"));
        assert!(raw.ends_with("\n"));
        assert!(raw.contains("\"defaultInstance\": \"Friday\""));
        #[cfg(unix)]
        {
            use std::os::unix::fs::MetadataExt;
            assert_eq!(std::fs::metadata(reg.path()).unwrap().mode() & 0o777, 0o600);
        }

        // Case-insensitive resolve returns the stored key + expanded path.
        let resolved = reg.resolve_instance("IGNITE").unwrap();
        assert_eq!(resolved.name, "Ignite");
        assert_eq!(resolved.path, "/tmp/ignite");
        assert_eq!(resolved.passphrase, "s3cret");

        // Unknown name lists the known set.
        let err = reg.resolve_instance("Nope").unwrap_err();
        assert_eq!(
            err,
            "Unknown instance \"Nope\". Known instances: Friday, Ignite."
        );

        // set-passphrase: set then clear.
        reg.set_instance_passphrase("friday", "pw").unwrap();
        assert!(reg.list_instances().unwrap()[0].has_passphrase);
        reg.set_instance_passphrase("friday", "").unwrap();
        assert!(!reg.list_instances().unwrap()[0].has_passphrase);

        // Rename preserves the entry + retargets the default; appends at end.
        let (old, new) = reg.rename_instance("Friday", "FridayDev").unwrap();
        assert_eq!((old.as_str(), new.as_str()), ("Friday", "FridayDev"));
        assert_eq!(
            reg.get_default_instance().unwrap().as_deref(),
            Some("FridayDev")
        );
        let entries = reg.list_instances().unwrap();
        assert_eq!(entries[0].name, "Ignite");
        assert_eq!(entries[1].name, "FridayDev");

        // Rename onto an existing name refuses.
        let err = reg.rename_instance("FridayDev", "ignite").unwrap_err();
        assert_eq!(err, "Instance \"ignite\" already exists.");

        // Remove + clear default.
        assert_eq!(reg.remove_instance("ignite").unwrap(), "Ignite");
        reg.clear_default_instance().unwrap();
        assert_eq!(reg.get_default_instance().unwrap(), None);
        assert_eq!(
            reg.remove_instance("ghost").unwrap_err(),
            "Unknown instance \"ghost\"."
        );
    }

    #[test]
    fn upsert_without_passphrase_drops_stored_one() {
        // v4's net effect: re-adding an instance without a passphrase drops it.
        let dir = tempfile::tempdir().unwrap();
        let reg = InstanceRegistry::at(dir.path().join("instances.json"));
        reg.upsert_instance("A", "/tmp/a", "pw").unwrap();
        assert!(reg.list_instances().unwrap()[0].has_passphrase);
        reg.upsert_instance("a", "/tmp/a2", "").unwrap();
        let entries = reg.list_instances().unwrap();
        assert_eq!(entries[0].name, "A"); // stored key preserved on case-insensitive hit
        assert_eq!(entries[0].path, "/tmp/a2");
        assert!(!entries[0].has_passphrase);
    }

    #[test]
    fn verify_passphrase_outcomes() {
        use quilltap_core::dbkey;
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().to_string_lossy().into_owned();

        // no-dbkey
        assert_eq!(
            verify_passphrase(&root, "anything").unwrap(),
            PassphraseCheck::NoDbkey
        );

        // no-encryption (internal-sentinel dbkey)
        let data_dir = dir.path().join("data");
        let pepper = dbkey::generate_pepper().unwrap();
        dbkey::save_dbkey(&data_dir, &pepper, "").unwrap();
        assert_eq!(
            verify_passphrase(&root, "anything").unwrap(),
            PassphraseCheck::NoEncryption
        );

        // valid / wrong (user-passphrase dbkey)
        dbkey::save_dbkey(&data_dir, &pepper, "swordfish").unwrap();
        assert_eq!(
            verify_passphrase(&root, "swordfish").unwrap(),
            PassphraseCheck::Valid
        );
        assert_eq!(
            verify_passphrase(&root, "wrong").unwrap(),
            PassphraseCheck::Wrong
        );
    }

    #[test]
    fn expand_path_matches_node_semantics() {
        assert_eq!(expand_path("/a/b/../c//d/"), "/a/c/d");
        assert_eq!(expand_path("/a/./b"), "/a/b");
        let home = std::env::var("HOME").unwrap_or_default();
        assert_eq!(expand_path("~/x"), format!("{home}/x"));
        assert_eq!(expand_path("~"), normalize_absolute(&home));
    }
}
