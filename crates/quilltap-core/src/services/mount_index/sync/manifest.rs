//! `<target>/.quilltap-sync.json` — what the last run left on both sides.
//!
//! Port of v4 `lib/mount-index/sync/manifest.ts` (`23da0b322`).
//!
//! Without it "absent on one side" is ambiguous: a file may be new here or
//! deleted there, and the sync would either resurrect everything the operator
//! deleted or delete everything they added. The manifest is what turns that into
//! a decidable question, and the only thing that makes a genuine conflict — both
//! sides edited since the last agreement — detectable rather than silently
//! resolved by whichever clock happens to be ahead.
//!
//! It is also the authoritative carrier of `createdAt` on the disk side, because
//! birthtime cannot be set from user space on Linux and only obliquely on macOS.
//! The comparison stays right even where `stat` cannot be made to agree.
//!
//! It lives in the target directory, is the one dot-entry the walk does not
//! ignore, and never enters the store.

use std::fmt;
use std::io::Write;
use std::path::{Path, PathBuf};

use serde_json::Value;

use super::types::{EntryKind, ManifestEntry, OrderedMap, SyncManifest, SYNC_MANIFEST_FILENAME};

/// v4 `ManifestMismatchError` — the manifest in this directory belongs to a
/// different store. The route answers it with a 409.
#[derive(Debug, Clone)]
pub struct ManifestMismatchError {
    pub found_store_id: String,
    pub expected_store_id: String,
}

impl fmt::Display for ManifestMismatchError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{SYNC_MANIFEST_FILENAME} in this directory belongs to store {}, not {}. \
             Sync to a different directory, or pass --no-manifest to ignore it.",
            self.found_store_id, self.expected_store_id
        )
    }
}

impl std::error::Error for ManifestMismatchError {}

pub fn manifest_path_for(target_path: &Path) -> PathBuf {
    target_path.join(SYNC_MANIFEST_FILENAME)
}

/// What [`read_manifest`] can fail with. A read error other than "not there" is
/// v4's re-thrown `fs` error and reaches the route's 500 arm.
#[derive(Debug)]
pub enum ReadManifestError {
    Mismatch(ManifestMismatchError),
    Io(std::io::Error),
}

impl fmt::Display for ReadManifestError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Mismatch(e) => e.fmt(f),
            Self::Io(e) => e.fmt(f),
        }
    }
}

impl std::error::Error for ReadManifestError {}

/// Read the manifest, or return `None` when there is none (a first run).
///
/// A manifest that does not parse is treated as absent and reported as a
/// warning: first-run rules create rather than delete, so the worst a corrupt
/// manifest can cost is a conflict the operator resolves by hand — never a
/// deletion nobody asked for.
pub fn read_manifest(
    target_path: &Path,
    store_id: &str,
    warnings: &mut Vec<String>,
) -> Result<Option<SyncManifest>, ReadManifestError> {
    let raw = match std::fs::read_to_string(manifest_path_for(target_path)) {
        Ok(raw) => raw,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(ReadManifestError::Io(e)),
    };

    let parsed: Value = match serde_json::from_str(&raw) {
        Ok(v) => v,
        Err(_) => {
            warnings.push(format!(
                "{SYNC_MANIFEST_FILENAME} is not valid JSON; treating this as a first run"
            ));
            return Ok(None);
        }
    };

    // The store check comes before schema validation so a manifest from another
    // store is named as such rather than reported as malformed.
    if let Some(claimed) = parsed.get("storeId").and_then(Value::as_str) {
        if claimed != store_id {
            return Err(ReadManifestError::Mismatch(ManifestMismatchError {
                found_store_id: claimed.to_string(),
                expected_store_id: store_id.to_string(),
            }));
        }
    }

    match parse_manifest(&parsed) {
        Ok(manifest) => Ok(Some(manifest)),
        Err(message) => {
            warnings.push(format!(
                "{SYNC_MANIFEST_FILENAME} did not validate ({message}); treating this as a first run"
            ));
            Ok(None)
        }
    }
}

/// v4 `SyncManifestSchema.safeParse`, plus the FIRST issue's MESSAGE — which is
/// what the warning quotes.
///
/// Zod's issue wording reaches the operator, so it is reproduced rather than
/// paraphrased; every sentence below was RECORDED from v4's real zod at
/// `f45a517a9` through `harness/oracle/cases/sync-manifest-sidecar.ts`, not
/// guessed. (The first draft of this function guessed, and the corpus caught
/// three of the four shapes wrong — `z.literal` prints no "received" clause, an
/// absent key is `undefined` rather than `null`, and a bad enum is
/// `Invalid option`, not `Invalid input`.)
///
/// Zod reports issues in the schema's own key order and `readManifest` quotes
/// `issues[0]`, so the checks run in `version, storeId, storeName, lastSyncAt,
/// entries` order, and each entry's in `kind, sha256, lastModified, createdAt,
/// descriptionSha256, descriptionUpdatedAt` order.
fn parse_manifest(value: &Value) -> Result<SyncManifest, String> {
    let object = value.as_object().ok_or_else(|| {
        format!(
            "Invalid input: expected object, received {}",
            zod_type_word(Some(value))
        )
    })?;

    // `z.literal(1)` prints the expected value and NO received clause, and it
    // fires for an absent key just as for a wrong one.
    if object.get("version").and_then(Value::as_i64) != Some(1) {
        return Err("Invalid input: expected 1".to_string());
    }
    let store_id = zod_string(object.get("storeId"))?;
    if store_id.is_empty() {
        return Err("Too small: expected string to have >=1 characters".to_string());
    }
    let store_name = zod_string(object.get("storeName"))?;
    let last_sync_at = zod_string(object.get("lastSyncAt"))?;

    let entries_value = object.get("entries");
    let entries_object = entries_value.and_then(Value::as_object).ok_or_else(|| {
        format!(
            "Invalid input: expected record, received {}",
            zod_type_word(entries_value)
        )
    })?;

    let mut entries: OrderedMap<ManifestEntry> = OrderedMap::new();
    for (key, raw) in entries_object {
        entries.insert(key.clone(), parse_manifest_entry(raw)?);
    }

    Ok(SyncManifest {
        version: 1,
        store_id,
        store_name,
        last_sync_at,
        entries,
    })
}

/// v4 `ManifestEntrySchema`, field by field in its declared order.
fn parse_manifest_entry(raw: &Value) -> Result<ManifestEntry, String> {
    let object = raw.as_object().ok_or_else(|| {
        format!(
            "Invalid input: expected object, received {}",
            zod_type_word(Some(raw))
        )
    })?;

    // `z.enum(['file', 'folder'])` — an absent key and a wrong one share the
    // message.
    let kind = match object.get("kind").and_then(Value::as_str) {
        Some("file") => EntryKind::File,
        Some("folder") => EntryKind::Folder,
        _ => return Err("Invalid option: expected one of \"file\"|\"folder\"".to_string()),
    };

    Ok(ManifestEntry {
        kind,
        sha256: zod_optional_string(object.get("sha256"))?,
        last_modified: zod_optional_string(object.get("lastModified"))?,
        created_at: zod_nullable_optional_string(object.get("createdAt"))?,
        description_sha256: zod_optional_string(object.get("descriptionSha256"))?,
        description_updated_at: zod_nullable_optional_string(object.get("descriptionUpdatedAt"))?,
    })
}

fn zod_string(value: Option<&Value>) -> Result<String, String> {
    match value {
        Some(Value::String(s)) => Ok(s.clone()),
        other => Err(format!(
            "Invalid input: expected string, received {}",
            zod_type_word(other)
        )),
    }
}

/// `z.string().optional()` — absent is fine, present must be a string.
fn zod_optional_string(value: Option<&Value>) -> Result<Option<String>, String> {
    match value {
        None => Ok(None),
        Some(Value::String(s)) => Ok(Some(s.clone())),
        other => Err(format!(
            "Invalid input: expected string, received {}",
            zod_type_word(other)
        )),
    }
}

/// `z.string().nullable().optional()` — absent, `null` and a string are all
/// valid, and the three stay distinguishable.
fn zod_nullable_optional_string(value: Option<&Value>) -> Result<Option<Option<String>>, String> {
    match value {
        None => Ok(None),
        Some(Value::Null) => Ok(Some(None)),
        Some(Value::String(s)) => Ok(Some(Some(s.clone()))),
        other => Err(format!(
            "Invalid input: expected string, received {}",
            zod_type_word(other)
        )),
    }
}

/// Zod 4's `parsedType` word for a JSON value, where an ABSENT key is
/// `undefined` rather than `null`.
fn zod_type_word(value: Option<&Value>) -> &'static str {
    match value {
        None => "undefined",
        Some(Value::Null) => "null",
        Some(Value::Bool(_)) => "boolean",
        Some(Value::Number(_)) => "number",
        Some(Value::String(_)) => "string",
        Some(Value::Array(_)) => "array",
        Some(Value::Object(_)) => "object",
    }
}

/// Write the manifest atomically — temp file, fsync, rename — so an interrupted
/// run leaves either the previous manifest or the new one, never half of one.
///
/// A half-written manifest is worse than none: it would read as "these entries
/// existed at the last run" for a prefix of the tree and drive deletions on the
/// rest.
pub fn write_manifest(target_path: &Path, manifest: &SyncManifest) -> std::io::Result<()> {
    let final_path = manifest_path_for(target_path);
    let temp = {
        let mut p = final_path.clone().into_os_string();
        p.push(".tmp");
        PathBuf::from(p)
    };
    let body = render_manifest(manifest);

    {
        let mut handle = std::fs::File::create(&temp)?;
        handle.write_all(body.as_bytes())?;
        // v4 `handle.sync()` — `fsync(2)`, so the rename cannot be ordered ahead
        // of the bytes.
        handle.sync_all()?;
    }
    std::fs::rename(&temp, &final_path)?;
    tracing::debug!(
        target: "quilltap::mount_index",
        target_path = %target_path.display(),
        entries = manifest.entries.len(),
        "[Sync] Manifest written",
    );
    Ok(())
}

/// The manifest's bytes: `JSON.stringify(manifest, null, 2)` plus a trailing
/// newline.
///
/// `serde_json::to_string_pretty` is two-space-indented like `JSON.stringify`'s
/// `null, 2`, and both render an empty object as `{}` on one line — the shape
/// `entries` takes on a store with nothing agreed.
pub fn render_manifest(manifest: &SyncManifest) -> String {
    format!(
        "{}\n",
        serde_json::to_string_pretty(manifest).expect("a manifest always serializes")
    )
}

/// An empty base, used under `--no-manifest` and on a first run.
pub fn empty_base() -> OrderedMap<ManifestEntry> {
    OrderedMap::new()
}

/// The manifest's entries, keyed by lower-cased path like the two walks.
pub fn base_from_manifest(manifest: Option<&SyncManifest>) -> OrderedMap<ManifestEntry> {
    let mut base = OrderedMap::new();
    let Some(manifest) = manifest else {
        return base;
    };
    for (key, entry) in manifest.entries.iter() {
        base.insert(key.to_lowercase(), entry.clone());
    }
    base
}
