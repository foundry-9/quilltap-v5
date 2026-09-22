//! Shapes shared across the document-store sync.
//!
//! Port of v4 `lib/mount-index/sync/types.ts` (`23da0b322`).
//!
//! The sync keeps a database-backed store and a directory on disk in step by
//! comparing SHA-256 first and timestamps second. Both sides are reduced to the
//! same [`SyncEntry`] shape before the planner sees them, which is what lets the
//! planner be pure and table-driven — it knows nothing about SQLite or about the
//! filesystem, only about entries, a base (the manifest), and the options.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

/// serde double-option: on an `Option<Option<T>>` field this decodes an ABSENT
/// key to `None`, an explicit `null` to `Some(None)`, and a value to
/// `Some(Some(v))` — the null-vs-absent distinction serde's default `Option`
/// collapses.
///
/// v4 writes `createdAt: null` into a manifest entry (a file neither side can
/// date) and OMITS the key for a `touch` on a platform that cannot set a
/// birthtime, and the two mean different things to the next run. Without this
/// the round trip silently turns the first into the second.
/// (`api::types` keeps a private twin for the dispatch tri-states; this is the
/// sync family's, since that one is not exported.)
pub fn double_option<'de, T, D>(de: D) -> Result<Option<Option<T>>, D::Error>
where
    T: Deserialize<'de>,
    D: serde::Deserializer<'de>,
{
    Deserialize::deserialize(de).map(Some)
}

/// Hex-encoded SHA-256 of raw bytes — v4 `sha256OfBuffer` (`lib/utils/sha256.ts`).
///
/// The tree keeps one of these per consumer rather than one shared helper (five
/// private copies today); this is the sync family's.
pub fn sha256_hex(bytes: &[u8]) -> String {
    hex::encode(Sha256::digest(bytes))
}

// ============================================================================
// Ordered map
// ============================================================================

/// A `Map<string, V>` with JS insertion-order iteration.
///
/// v4's walks hand the planner `Map`s and the planner builds its key set with
/// `new Set([...store.keys(), ...disk.keys(), ...base.keys()])`; both JS types
/// iterate in insertion order, and the planner's final sort is STABLE, so two
/// actions that tie on the whole comparator (a `create disk` and the `describe
/// disk` pushed right after it, same path, same side, same kind) keep the order
/// they were pushed in. A `HashMap`/`BTreeMap` here would silently reorder the
/// plan, so the insertion order is carried explicitly.
#[derive(Debug, Clone, Default)]
pub struct OrderedMap<V> {
    keys: Vec<String>,
    /// `None` where [`OrderedMap::remove`] has taken the entry out — JS `Map`
    /// delete leaves every other entry's position untouched.
    values: Vec<Option<V>>,
    index: HashMap<String, usize>,
    len: usize,
}

impl<V> OrderedMap<V> {
    pub fn new() -> Self {
        Self {
            keys: Vec::new(),
            values: Vec::new(),
            index: HashMap::new(),
            len: 0,
        }
    }

    pub fn insert(&mut self, key: impl Into<String>, value: V) {
        let key = key.into();
        match self.index.get(&key) {
            // JS `Map.set` on an existing key replaces the value and KEEPS the
            // original position.
            Some(&slot) => {
                if self.values[slot].is_none() {
                    self.len += 1;
                }
                self.values[slot] = Some(value);
            }
            None => {
                self.index.insert(key.clone(), self.keys.len());
                self.keys.push(key);
                self.values.push(Some(value));
                self.len += 1;
            }
        }
    }

    pub fn get(&self, key: &str) -> Option<&V> {
        let slot = *self.index.get(key)?;
        self.values[slot].as_ref()
    }

    pub fn get_mut(&mut self, key: &str) -> Option<&mut V> {
        let slot = *self.index.get(key)?;
        self.values[slot].as_mut()
    }

    pub fn remove(&mut self, key: &str) {
        if let Some(&slot) = self.index.get(key) {
            if self.values[slot].take().is_some() {
                self.len -= 1;
            }
        }
    }

    pub fn contains_key(&self, key: &str) -> bool {
        self.get(key).is_some()
    }

    pub fn len(&self) -> usize {
        self.len
    }

    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    pub fn keys(&self) -> impl Iterator<Item = &str> {
        self.iter().map(|(k, _)| k)
    }

    pub fn iter(&self) -> impl Iterator<Item = (&str, &V)> {
        self.keys
            .iter()
            .zip(self.values.iter())
            .filter_map(|(k, v)| v.as_ref().map(|v| (k.as_str(), v)))
    }

    pub fn iter_mut(&mut self) -> impl Iterator<Item = (&str, &mut V)> {
        self.keys
            .iter()
            .zip(self.values.iter_mut())
            .filter_map(|(k, v)| v.as_mut().map(|v| (k.as_str(), v)))
    }
}

impl<V: PartialEq> PartialEq for OrderedMap<V> {
    /// Two maps are equal when they hold the same keys, in the same order, with
    /// equal values — the comparison a report diff wants.
    fn eq(&self, other: &Self) -> bool {
        self.len() == other.len() && self.iter().zip(other.iter()).all(|(a, b)| a == b)
    }
}

impl<V: Eq> Eq for OrderedMap<V> {}

impl<V: Serialize> Serialize for OrderedMap<V> {
    /// A JS object: the keys come out in the order they went in.
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        use serde::ser::SerializeMap;
        let mut map = serializer.serialize_map(Some(self.len()))?;
        for (key, value) in self.iter() {
            map.serialize_entry(key, value)?;
        }
        map.end()
    }
}

impl<'de, V: Deserialize<'de>> Deserialize<'de> for OrderedMap<V> {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct Visitor<V>(std::marker::PhantomData<V>);
        impl<'de, V: Deserialize<'de>> serde::de::Visitor<'de> for Visitor<V> {
            type Value = OrderedMap<V>;
            fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
                f.write_str("a map")
            }
            fn visit_map<M: serde::de::MapAccess<'de>>(
                self,
                mut access: M,
            ) -> Result<Self::Value, M::Error> {
                let mut out = OrderedMap::new();
                while let Some((key, value)) = access.next_entry::<String, V>()? {
                    out.insert(key, value);
                }
                Ok(out)
            }
        }
        deserializer.deserialize_map(Visitor(std::marker::PhantomData))
    }
}

// ============================================================================
// Options
// ============================================================================

/// v4 `SyncDirection` — which side(s) the run is allowed to change.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum SyncDirection {
    Both,
    ToDisk,
    ToStore,
}

impl SyncDirection {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Both => "both",
            Self::ToDisk => "to-disk",
            Self::ToStore => "to-store",
        }
    }

    pub fn parse(raw: &str) -> Option<Self> {
        match raw {
            "both" => Some(Self::Both),
            "to-disk" => Some(Self::ToDisk),
            "to-store" => Some(Self::ToStore),
            _ => None,
        }
    }
}

/// v4 `SyncPreference` — how a content difference is resolved.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SyncPreference {
    Newer,
    Store,
    Disk,
}

impl SyncPreference {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Newer => "newer",
            Self::Store => "store",
            Self::Disk => "disk",
        }
    }

    pub fn parse(raw: &str) -> Option<Self> {
        match raw {
            "newer" => Some(Self::Newer),
            "store" => Some(Self::Store),
            "disk" => Some(Self::Disk),
            _ => None,
        }
    }
}

/// v4 `SyncOptionsSchema`'s inferred type. The route's `syncSchema` is the same
/// shape with `.optional()` on the five defaulted keys; the defaults here ARE
/// v4's.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SyncOptions {
    /// Absolute, server-local directory. Created when absent.
    pub target_path: String,
    /// Plan and report; change nothing on either side, and write no manifest.
    pub dry_run: bool,
    pub direction: SyncDirection,
    pub prefer: SyncPreference,
    /// False suppresses every `delete` / `rmdir`, on both sides.
    pub propagate_deletes: bool,
    /// False ignores (and does not write) `.quilltap-sync.json` — first-run
    /// rules every time.
    pub use_manifest: bool,
}

impl SyncOptions {
    /// The five defaults v4's schema applies, for a caller that has only a path.
    pub fn with_defaults(target_path: impl Into<String>) -> Self {
        Self {
            target_path: target_path.into(),
            dry_run: false,
            direction: SyncDirection::Both,
            prefer: SyncPreference::Newer,
            propagate_deletes: true,
            use_manifest: true,
        }
    }
}

// ============================================================================
// Entries
// ============================================================================

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SyncSide {
    Store,
    Disk,
}

impl SyncSide {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Store => "store",
            Self::Disk => "disk",
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum EntryKind {
    #[default]
    File,
    Folder,
}

impl EntryKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::File => "file",
            Self::Folder => "folder",
        }
    }
}

/// One `(relativePath)` as one side sees it. Files and folders share the shape;
/// a folder has no sha, size, or description.
///
/// Keys in the maps handed to the planner are lower-cased (the store's index is
/// NOCASE and macOS is case-insensitive); `relative_path` keeps the casing the
/// side actually stores, so an applier writes what that side expects.
///
/// The serde shape is v4's object literal: every `Option` is an OPTIONAL key
/// (absent, not `null`), which is what lets a differential corpus recorded from
/// v4's own walks deserialize straight into it. `created_at` is the one field
/// v4 types `string | null` rather than `?`, so `null` is its absent form.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SyncEntry {
    pub relative_path: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub kind: Option<EntryKind>,
    /// Files only: SHA-256 of the bytes as they would sit on disk.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sha256: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub size_bytes: Option<i64>,
    /// ISO-8601 with milliseconds.
    pub last_modified: String,
    /// ISO-8601; `None` when the side cannot say (Linux birthtime, a fresh
    /// folder).
    pub created_at: Option<String>,
    /// Binaries only: the store's `description`, or the sidecar's body. `None`
    /// is v4's `undefined` — on the disk side that is "no sidecar here", which
    /// the planner distinguishes from an empty one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// v4 `descriptionUpdatedAt?: string | null`. Both of v4's absent forms
    /// (`undefined` and `null`) are read only through `?? lastModified` and
    /// `timeOf`, which treat them identically, so the two collapse here.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description_updated_at: Option<String>,

    // ---- store-side extras the appliers need and the planner ignores ----
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub link_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub file_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub link_group_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub file_type: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub folder_id: Option<String>,
}

impl SyncEntry {
    /// v4 `entry.kind`, which is never absent on a walked entry.
    pub fn kind(&self) -> EntryKind {
        self.kind.unwrap_or(EntryKind::File)
    }
}

/// A side's walk, keyed by lower-cased relative path.
pub type SyncEntryMap = OrderedMap<SyncEntry>;

// ============================================================================
// Manifest
// ============================================================================

/// v4 `ManifestEntrySchema`. Field order is the order v4's `buildManifest`
/// inserts the keys, which is what `JSON.stringify` writes.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ManifestEntry {
    pub kind: EntryKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sha256: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_modified: Option<String>,
    /// `.nullable().optional()`: absent and `null` are distinct on the wire, so
    /// the outer `Option` is "key present" and the inner is `null`.
    #[serde(
        default,
        deserialize_with = "double_option",
        skip_serializing_if = "Option::is_none"
    )]
    pub created_at: Option<Option<String>>,
    /// SHA-256 of the description text, so a sidecar edit is detectable.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description_sha256: Option<String>,
    #[serde(
        default,
        deserialize_with = "double_option",
        skip_serializing_if = "Option::is_none"
    )]
    pub description_updated_at: Option<Option<String>>,
}

impl ManifestEntry {
    /// v4 `b.createdAt` read through `timeOf`, where `undefined` and `null`
    /// answer the same.
    pub fn created_at_value(&self) -> Option<&str> {
        self.created_at.as_ref().and_then(|v| v.as_deref())
    }
}

/// v4 `SyncManifestSchema`. The key order here is the order `writeManifest`
/// serializes, byte for byte.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SyncManifest {
    /// v4 `z.literal(1)`.
    pub version: i64,
    pub store_id: String,
    pub store_name: String,
    pub last_sync_at: String,
    /// A JS object: insertion-ordered, which is the order `buildManifest`
    /// walked the store.
    pub entries: OrderedMap<ManifestEntry>,
}

// ============================================================================
// Actions
// ============================================================================

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SyncActionKind {
    Create,
    Modify,
    Delete,
    Touch,
    Describe,
    Mkdir,
    Rmdir,
    Conflict,
    Skip,
}

impl SyncActionKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Create => "create",
            Self::Modify => "modify",
            Self::Delete => "delete",
            Self::Touch => "touch",
            Self::Describe => "describe",
            Self::Mkdir => "mkdir",
            Self::Rmdir => "rmdir",
            Self::Conflict => "conflict",
            Self::Skip => "skip",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SyncOutcome {
    Applied,
    Skipped,
    Failed,
    Planned,
}

/// One unit of work. `side` is the side that CHANGES — `modify store` means the
/// store is rewritten from disk. `conflict` and `skip` change nothing and carry
/// `side: null`.
///
/// Every `Option` here is v4's `?`, and serializes by OMITTING the key; the two
/// fields v4 types `?: T | null` carry a nested `Option` so an explicit `null`
/// stays distinguishable from an absent key on the wire (the CLI reads this
/// JSON).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SyncAction {
    pub kind: SyncActionKind,
    /// Serialized as `null` for `conflict` / `skip`, exactly as v4 does.
    pub side: Option<SyncSide>,
    /// The path as the changing side should spell it.
    pub relative_path: String,
    pub entry_kind: EntryKind,
    /// Human-readable why, printed in parentheses by the CLI.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
    /// Timestamps to stamp on the changing side after the content lands.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_modified: Option<String>,
    #[serde(
        default,
        deserialize_with = "double_option",
        skip_serializing_if = "Option::is_none"
    )]
    pub created_at: Option<Option<String>>,
    /// `describe` only: the text to write (empty string clears).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// Files only, for the report.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sha256: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub size_bytes: Option<i64>,
    /// The store-side link this action acts on (delete / touch / describe /
    /// modify).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub link_id: Option<String>,
    /// The sha the planner saw on the store, for the applier's
    /// compare-and-swap.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expected_store_sha256: Option<String>,
    /// Set by the applier when the action did not go through as planned.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub outcome: Option<SyncOutcome>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

impl SyncAction {
    /// The bare skeleton every constructor in the planner starts from. v4 builds
    /// object literals with only the keys it wants; an omitted key here is a
    /// `None`, which serializes the same way.
    pub fn bare(
        kind: SyncActionKind,
        relative_path: impl Into<String>,
        entry_kind: EntryKind,
    ) -> Self {
        Self {
            kind,
            side: None,
            relative_path: relative_path.into(),
            entry_kind,
            reason: None,
            last_modified: None,
            created_at: None,
            description: None,
            sha256: None,
            size_bytes: None,
            link_id: None,
            expected_store_sha256: None,
            outcome: None,
            error: None,
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SyncSummary {
    pub created: i64,
    pub modified: i64,
    pub deleted: i64,
    pub touched: i64,
    pub described: i64,
    pub conflicts: i64,
    pub skipped: i64,
    pub failed: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SyncReport {
    pub store_id: String,
    pub store_name: String,
    pub target_path: String,
    pub dry_run: bool,
    pub actions: Vec<SyncAction>,
    pub summary: SyncSummary,
    pub warnings: Vec<String>,
    pub elapsed_ms: i64,
}

// ============================================================================
// Constants
// ============================================================================

/// The verb's own dotfile, and the one dot-entry it does not ignore.
pub const SYNC_MANIFEST_FILENAME: &str = ".quilltap-sync.json";

/// A binary's description lives beside it under this suffix.
pub const SIDECAR_SUFFIX: &str = ".description.md";

/// Scratch suffix for the write-then-rename on the disk side.
pub const DISK_TEMP_SUFFIX: &str = ".quilltap-tmp";

/// Timestamps closer together than this compare equal. Filesystems with one- or
/// two-second resolution (FAT, some network mounts) would otherwise make every
/// run find work to do.
pub const MTIME_TOLERANCE_MS: i64 = 1000;

/// Only these carry a description, and so only these get a sidecar.
pub const SIDECAR_FILE_TYPES: &[&str] = &["blob", "pdf", "docx"];

/// v4 `hasSidecar` — `undefined` is not a sidecar-bearing type.
pub fn has_sidecar(file_type: Option<&str>) -> bool {
    match file_type {
        Some(t) => SIDECAR_FILE_TYPES.contains(&t),
        None => false,
    }
}
