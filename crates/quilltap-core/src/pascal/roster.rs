//! Custom Tools — discovery, tier shadowing, and roster resolution (v4
//! `lib/pascal/custom-tools.ts`, the `Discovery` half).
//!
//! Pascal deals from a roster of user-authored pseudo-tools: `Tools/*.tool.json`
//! documents at the root of any document store. This module finds them, resolves
//! which definition wins when several stores name the same tool, and reports the
//! rest as load errors rather than hiding them.
//!
//! # Freshness — no caching, EVER
//!
//! The roster is re-resolved at every assembly point and never cached across
//! turns. A `.tool.json` added, edited, or deleted mid-chat takes effect on the
//! next LLM call and the next popup open, with no "reload tools" step. This is
//! affordable — one folder listing per mount, against the local index — and it
//! is the only correct choice: edits to a document inside a mount don't reliably
//! touch the mount index, so an index-invalidated cache would miss exactly the
//! changes users most expect to see.
//!
//! # The shadowing core is a seam
//!
//! [`resolve_roster_from_pool`] takes an already-resolved pool and a `load`
//! closure, so the tier-shadowing + cap logic (the part with the most ways to be
//! subtly wrong) is driven directly by the differential — exactly as v4's own
//! `custom-tools-discovery.test.ts` mocks the pool and the store reads. The real
//! [`resolve_custom_tool_roster`] wires the same core to the live DB.

use std::collections::HashSet;

use rusqlite::Connection;
use serde_json::{Map, Value};

use crate::db::database_store::{list_database_files, DbStoreErrorCode};
use crate::db::doc_mount_points::{DocMountPointsRepository, MountServiceInfo};
use crate::db::tiered_mount_pool::{
    resolve_tiered_mount_pool, MountTier, TierContext, TierResolveOptions, TieredMountPool,
};
use crate::services::mount_index::file_op_error::FileOpErrorCode;
use crate::services::mount_index::path_utils::normalise_relative_path;
use crate::services::mount_index::read_file::{read_mount_file_bytes_conn, MountFileError};

use super::custom_tool_types::{
    collect_unknown_keys, format_definition_issues, safe_parse, QtapCustomTool, MAX_ROSTER_SIZE,
    TOOLS_FOLDER, TOOL_FILE_SUFFIX,
};
use super::tool_gate::{evaluate_tool_gate, has_tool_gate};

/// Tier precedence, nearest first. A nearer tier shadows a farther one.
const TIER_ORDER: [MountTier; 5] = [
    MountTier::Character,
    MountTier::Participant,
    MountTier::Group,
    MountTier::Project,
    MountTier::Global,
];

/// A definition found in a store, with the provenance shadowing and `pascalMeta`
/// both depend on.
#[derive(Debug, Clone, PartialEq)]
pub struct DiscoveredCustomTool {
    pub definition: QtapCustomTool,
    pub tier: MountTier,
    pub mount_point_id: String,
    pub mount_name: String,
    /// Vault-relative path, so the UI can link to the file the user wrote.
    pub definition_path: String,
}

/// A `.tool.json` that failed to load — surfaced as a badge rather than hidden.
#[derive(Debug, Clone, PartialEq)]
pub struct CustomToolLoadError {
    pub definition_path: String,
    pub mount_point_id: String,
    pub mount_name: String,
    pub tier: MountTier,
    pub reason: String,
}

/// The resolved roster for one invoker's perspective.
#[derive(Debug, Clone, Default)]
pub struct CustomToolRoster {
    /// Runnable tools, keyed by name, in insertion order — after shadowing and
    /// `disabled` suppression. (v4's `Map<string, DiscoveredCustomTool>`.)
    pub tools: Vec<(String, DiscoveredCustomTool)>,
    /// Definitions that could not be loaded.
    pub errors: Vec<CustomToolLoadError>,
    /// Names dropped because the roster hit [`MAX_ROSTER_SIZE`].
    pub dropped_for_cap: Vec<String>,
}

impl CustomToolRoster {
    /// Look up a runnable tool by its `name`.
    pub fn get(&self, name: &str) -> Option<&DiscoveredCustomTool> {
        self.tools.iter().find(|(k, _)| k == name).map(|(_, v)| v)
    }

    fn has(&self, name: &str) -> bool {
        self.tools.iter().any(|(k, _)| k == name)
    }
}

/// Who is asking, and from where. (v4 `RosterContext`.)
#[derive(Debug, Clone, Default)]
pub struct RosterContext {
    pub user_id: String,
    pub chat_id: String,
    /// The invoking character — their vault is the `character` tier.
    pub character_id: Option<String>,
    pub character_mount_point_id: Option<String>,
    /// Every character in the chat — their vaults form the `participant` tier.
    pub character_ids: Option<Vec<String>>,
    pub project_id: Option<String>,
    /// The invoking character's hydrated fact sheet, which availability gates
    /// are answered against (v4 `6864bf0e`). Optional: a caller that already
    /// holds it (every one of them does — the popup hydrates perspectives, the
    /// turn resolves a responding character) passes it to spare a read, and one
    /// that doesn't leaves the roster to fetch it, but only if some definition
    /// turns out to be gated.
    ///
    /// `Some(map)` — including `Some({})` — is v4's truthy `ctx.metadata` and
    /// short-circuits the read; `None` is v4's `null`/`undefined` and does not.
    pub metadata: Option<Map<String, Value>>,
}

/// The content of one candidate tool file, whichever kind of store held it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ToolFileContent {
    /// The file's bytes, read successfully.
    Content(String),
    /// Deleted between list and read — a race, not a defect. Skipped quietly.
    Skip,
    /// A non-race read failure — becomes a `could not be read` error entry.
    ReadError(String),
}

/// True for a root-level `*.tool.json`.
///
/// `list_database_files({ folder })` filters by PREFIX, so it happily returns
/// `Tools/nested/deep/x.tool.json`. Definitions are a flat root-level
/// convention; a nested file is somebody's archive, not a live tool.
pub fn is_root_tool_file(relative_path: &str) -> bool {
    if !relative_path.to_lowercase().ends_with(TOOL_FILE_SUFFIX) {
        return false;
    }
    // v4 `relativePath.slice(TOOLS_FOLDER.length + 1)` — the part after `Tools/`.
    let rest = relative_path.get(TOOLS_FOLDER.len() + 1..).unwrap_or("");
    !rest.is_empty() && !rest.contains('/')
}

/// The parse → validate → dedup core, over a mount's already-listed root tool
/// files. Never fails: a broken file becomes an error entry, because one
/// malformed `.tool.json` must not take the whole roster down with it.
///
/// This is the sequence v4's `loadToolsFromMount` runs after listing, and the
/// point Pascal's Workbench library reuses rather than copying.
pub fn load_definitions(
    files: &[(String, ToolFileContent)],
    mount_point_id: &str,
    mount_name: &str,
    tier: MountTier,
) -> (Vec<DiscoveredCustomTool>, Vec<CustomToolLoadError>) {
    let mut found = Vec::new();
    let mut errors = Vec::new();
    let err = |path: &str, reason: String| CustomToolLoadError {
        definition_path: path.to_string(),
        mount_point_id: mount_point_id.to_string(),
        mount_name: mount_name.to_string(),
        tier,
        reason,
    };

    // Same-mount duplicate `name` is a rejection (the spec's rule 5): within one
    // store there is no tier to break the tie, so neither file can be trusted to
    // be the one the author meant.
    let mut name_to_path: Vec<(String, String)> = Vec::new();

    for (relative_path, content) in files {
        let raw = match content {
            ToolFileContent::Content(s) => s,
            ToolFileContent::Skip => continue,
            ToolFileContent::ReadError(msg) => {
                errors.push(err(relative_path, format!("could not be read: {msg}")));
                continue;
            }
        };

        let parsed_json: serde_json::Value = match serde_json::from_str(raw) {
            Ok(v) => v,
            Err(e) => {
                errors.push(err(relative_path, format!("is not valid JSON: {e}")));
                continue;
            }
        };

        // Unknown TOP-LEVEL keys are tolerated, not rejected — `persist` is a
        // planned v2 key and an older build must not choke on a newer file. v4
        // logs them at debug; the roster is unaffected.
        let _unknown_keys = collect_unknown_keys(&parsed_json);

        let definition = match safe_parse(&parsed_json) {
            Ok(d) => d,
            Err(issues) => {
                errors.push(err(relative_path, format_definition_issues(&issues)));
                continue;
            }
        };

        if let Some((_, prior_path)) = name_to_path.iter().find(|(n, _)| *n == definition.name) {
            errors.push(err(
                relative_path,
                format!(
                    "defines \"{}\", which {prior_path} in this same store already defines",
                    definition.name
                ),
            ));
            continue;
        }
        name_to_path.push((definition.name.clone(), relative_path.clone()));

        found.push(DiscoveredCustomTool {
            definition,
            tier,
            mount_point_id: mount_point_id.to_string(),
            mount_name: mount_name.to_string(),
            definition_path: relative_path.clone(),
        });
    }

    (found, errors)
}

/// Bucket the pool into (tier, mount id) pairs in precedence order.
fn ordered_mounts(pool: &TieredMountPool) -> Vec<(MountTier, String)> {
    let by_tier = |tier: MountTier| -> Vec<String> {
        let mut ids = match tier {
            MountTier::Character => pool.character_mount_point_id.iter().cloned().collect(),
            MountTier::Participant => pool.participant_mount_point_ids.clone(),
            MountTier::Group => pool.group_mount_point_ids.clone(),
            MountTier::Project => pool.project_mount_point_ids.clone(),
            MountTier::Global => pool.global_mount_point_id.iter().cloned().collect(),
        };
        // Same-tier collisions resolve by mount id, lexicographically — arbitrary
        // but stable, so a roster never flickers between two equally-close stores.
        ids.sort();
        ids
    };

    let mut ordered = Vec::new();
    for tier in TIER_ORDER {
        for mount_point_id in by_tier(tier) {
            ordered.push((tier, mount_point_id));
        }
    }
    ordered
}

/// The invoking character's fact sheet, for the availability gates (v4
/// `loadInvokerMetadata`).
///
/// Fail-soft to `{}`: a vault that cannot be read is not a reason to abandon the
/// whole roster, and an empty sheet has a defined meaning at the gate (nothing
/// qualifies, nothing is disqualified — see [`super::tool_gate`]).
fn load_invoker_metadata(
    ctx: &RosterContext,
    main: &Connection,
    mount: &Connection,
) -> Map<String, Value> {
    if let Some(sheet) = ctx.metadata.as_ref() {
        return sheet.clone();
    }
    let Some(character_id) = ctx.character_id.as_deref() else {
        return Map::new();
    };

    match crate::db::characters_read::find_by_id(main, mount, character_id) {
        Ok(Some(character)) => character
            .get("metadata")
            .and_then(Value::as_object)
            .cloned()
            .unwrap_or_default(),
        Ok(None) => Map::new(),
        Err(error) => {
            tracing::debug!(
                target: "quilltap::pascal",
                chat_id = %ctx.chat_id,
                character_id,
                error = %error,
                "Custom-tool gates: the invoker's fact sheet could not be read, treating it as empty",
            );
            Map::new()
        }
    }
}

/// Resolve the roster from an already-resolved pool, given a per-mount loader.
///
/// The tier-shadowing + cap core, separated from the DB so the differential can
/// drive it exactly as v4's discovery test does. Nearest tier wins; a `disabled`
/// tombstone suppresses a name at its tier AND every farther one.
pub fn resolve_roster_from_pool(
    pool: &TieredMountPool,
    mut load: impl FnMut(&str, MountTier) -> (Vec<DiscoveredCustomTool>, Vec<CustomToolLoadError>),
    mut invoker_metadata: impl FnMut() -> Map<String, Value>,
) -> CustomToolRoster {
    let mut roster = CustomToolRoster::default();
    // Names switched off by a nearer tier. They must stay off further out.
    let mut suppressed: HashSet<String> = HashSet::new();

    // Fetched at most once, and only if a gated definition actually turns up:
    // most rosters carry none, and none of them should pay for a vault read.
    // (v4 memoises a promise; synchronous Rust memoises the value.)
    let mut sheet: Option<Map<String, Value>> = None;

    for (tier, mount_point_id) in ordered_mounts(pool) {
        let (found, mount_errors) = load(&mount_point_id, tier);
        roster.errors.extend(mount_errors);

        for entry in found {
            let name = entry.definition.name.clone();

            // Nearest tier wins. Once a name is decided — by a live definition or
            // by a `disabled` tombstone — farther tiers cannot revive it.
            if roster.has(&name) || suppressed.contains(&name) {
                continue;
            }

            // The availability gate, and it runs BEFORE `disabled` on purpose. A
            // gated-out definition makes no claim on the name at all — not even a
            // tombstone — so a farther tier may still deal one. That is the useful
            // arrangement: a character's own vault holds the variant written for
            // characters like them, and the General store holds the plain one
            // everybody else gets. `disabled`, by contrast, stays absolute; a
            // gated tombstone ("suppress this name for novices") is simply both
            // keys at once, and reads exactly as it says.
            if has_tool_gate(&entry.definition) {
                let verdict = evaluate_tool_gate(
                    &entry.definition,
                    Some(sheet.get_or_insert_with(&mut invoker_metadata)),
                );
                if !verdict.available {
                    // v4's `logger.debug('Custom tool withheld by its
                    // availability gate', {…})`, same point, same fields.
                    tracing::debug!(
                        target: "quilltap::pascal",
                        name = %name,
                        tier = ?tier,
                        mount_point_id = %mount_point_id,
                        withheld_by = verdict.withheld_by.map(|c| c.as_str()).unwrap_or("null"),
                        "Custom tool withheld by its availability gate",
                    );
                    continue;
                }
            }

            if entry.definition.disabled == Some(true) {
                suppressed.insert(name);
                continue;
            }

            if roster.tools.len() >= MAX_ROSTER_SIZE {
                roster.dropped_for_cap.push(name);
                continue;
            }

            roster.tools.push((name, entry));
        }
    }

    roster
}

/// Load every definition in one mount from the live DB / disk. Never fails.
///
/// Exported for Pascal's Workbench, whose library view lists every store's
/// definitions — valid and broken alike — through this same read → parse →
/// validate sequence rather than a second copy of it.
pub fn load_tools_from_mount(
    mount: &Connection,
    mount_point_id: &str,
    tier: MountTier,
) -> (Vec<DiscoveredCustomTool>, Vec<CustomToolLoadError>) {
    let empty = (Vec::new(), Vec::new());

    // The service-info row is the one the canonical reader wants, and it carries
    // everything the listing needs too (`name`/`enabled`/`mountType`/`basePath`),
    // so one fetch serves both halves. v4 reads the row twice — once here, once
    // inside `readMountFileBytes` — which is the same row either way.
    let row = match DocMountPointsRepository::new(mount).find_service_info_by_id(mount_point_id) {
        Ok(Some(r)) if r.enabled => r,
        _ => return empty,
    };

    // List the candidate root tool files. A listing failure yields an empty
    // mount, matching v4's warn-and-continue.
    //
    // The listing side is deliberately NOT routed through the mount index for
    // on-disk stores: `Tools/` is enumerated live off the disk, because edits
    // inside a mount don't reliably touch the index.
    let paths: Vec<String> = if row.mount_type == "database" {
        match list_database_files(mount, mount_point_id, Some(TOOLS_FOLDER)) {
            Ok(entries) => entries
                .into_iter()
                .filter(|e| e.kind != "folder" && is_root_tool_file(&e.relative_path))
                .map(|e| e.relative_path)
                .collect(),
            Err(_) => return empty,
        }
    } else {
        // v4 calls `listToolFilesFromDisk(mount.basePath)`; a null basePath makes
        // its `path.join` throw, which the same try/catch turns into an empty
        // mount. Model that directly rather than joining onto "".
        match row.base_path.as_deref() {
            Some(base) => match list_tool_files_from_disk(base) {
                Ok(p) => p,
                Err(_) => return empty,
            },
            None => return empty,
        }
    };

    let files: Vec<(String, ToolFileContent)> = paths
        .into_iter()
        .map(|rel| {
            let content = read_tool_file(mount, &row, &rel);
            (rel, content)
        })
        .collect();

    load_definitions(&files, mount_point_id, &row.name, tier)
}

/// Read one definition's bytes, whichever kind of store holds it (v4
/// `readToolFile`).
///
/// Delegates to the canonical mount reader rather than reaching for the disk or
/// the documents table itself: that one helper already knows every storage shape
/// (filesystem, Obsidian, database documents, database blobs) and enforces the
/// mount-boundary check, so a definition path can never wander outside its own
/// store — and a definition stored as a database BLOB, which the old direct
/// `read_database_document` call missed entirely, is found.
///
/// v4's catch chain has three skip arms: `FileOpError SOURCE_NOT_FOUND` (added
/// by this drift), `DatabaseStoreError NOT_FOUND`, and a raw `ENOENT`. Only the
/// first is reachable now — the canonical reader converts every missing source,
/// on disk or in the tables, into `SOURCE_NOT_FOUND`. The typed
/// `DatabaseStoreError` arm is carried anyway, exactly as v4 carries it; v4's
/// raw-`ENOENT` arm has no faithful analog here (v5's reader erases the io error
/// kind into a message, and matching on message text would be a different rule
/// from v4's `.code === 'ENOENT'`, not a port of it), and it is dead on both
/// sides for the same reason the second arm is.
fn read_tool_file(
    mount: &Connection,
    row: &MountServiceInfo,
    relative_path: &str,
) -> ToolFileContent {
    // v4's `readMountFileBytes` normalises internally; v5's takes an
    // already-normalised path, so the normalisation (and its traversal refusal)
    // happens here.
    let rel = match normalise_relative_path(relative_path) {
        Ok(r) => r,
        Err(e) => return ToolFileContent::ReadError(e.to_string()),
    };

    match read_mount_file_bytes_conn(mount, row, &rel) {
        // v4 does `bytes.toString('utf-8')` — Node's decoder never throws, it
        // substitutes U+FFFD. `from_utf8_lossy` is the byte-faithful analog;
        // `read_to_string` (what this path used to do) would have failed instead.
        Ok(file) => ToolFileContent::Content(String::from_utf8_lossy(&file.bytes).into_owned()),
        // Deleted between list and read — a race, not a defect. Skip quietly.
        Err(MountFileError::FileOp(e)) if e.code == FileOpErrorCode::SourceNotFound => {
            ToolFileContent::Skip
        }
        Err(MountFileError::Store(e)) if e.code == DbStoreErrorCode::NotFound => {
            ToolFileContent::Skip
        }
        Err(e) => ToolFileContent::ReadError(e.to_string()),
    }
}

/// List `Tools/*.tool.json` in an on-disk store (filesystem or obsidian).
///
/// A missing `Tools/` folder is the normal case — it is a lazy convention, never
/// scaffolded — so ENOENT/ENOTDIR yield `[]` rather than an error.
fn list_tool_files_from_disk(base_path: &str) -> std::io::Result<Vec<String>> {
    let folder = std::path::Path::new(base_path).join(TOOLS_FOLDER);
    let entries = match std::fs::read_dir(&folder) {
        Ok(e) => e,
        Err(e) if matches!(e.kind(), std::io::ErrorKind::NotFound) => return Ok(Vec::new()),
        // ENOTDIR surfaces as `Other`/`NotADirectory` depending on platform; v4
        // treats both as "no folder".
        Err(e) if e.raw_os_error() == Some(20) => return Ok(Vec::new()),
        Err(e) => return Err(e),
    };

    let mut out = Vec::new();
    for entry in entries.flatten() {
        if entry.file_type().map(|t| t.is_file()).unwrap_or(false) {
            let name = entry.file_name().to_string_lossy().into_owned();
            if name.to_lowercase().ends_with(TOOL_FILE_SUFFIX) {
                out.push(format!("{TOOLS_FOLDER}/{name}"));
            }
        }
    }
    Ok(out)
}

/// Every definition in every enabled store — the Workbench library (v4
/// `CustomToolLibrary`).
#[derive(Debug, Clone, Default)]
pub struct CustomToolLibrary {
    pub entries: Vec<DiscoveredCustomTool>,
    pub errors: Vec<CustomToolLoadError>,
}

/// Enumerate every definition in every enabled store — the Workbench library
/// (v4 `listAllCustomTools`).
///
/// Deliberately NOT the tiered roster: no shadowing, no `disabled` suppression,
/// no per-invoker perspective, no cap. The library is the authoring surface, so
/// it shows the whole table face up; which definition would win a given chat
/// depends on the invoker and is not this function's question to answer. Tier
/// attribution is meaningless without an invoker, so every entry carries
/// `'global'` — callers should not read anything into it.
pub fn list_all_custom_tools(mount: &Connection) -> CustomToolLibrary {
    let mut mounts = match DocMountPointsRepository::new(mount).find_enabled_for_docedit() {
        Ok(m) => m,
        Err(_) => return CustomToolLibrary::default(),
    };
    // v4 sorts by `id.localeCompare` — code-unit order for the ASCII mount ids.
    mounts.sort_by(|a, b| a.id.cmp(&b.id));

    let mut library = CustomToolLibrary::default();
    for m in mounts {
        let (found, errors) = load_tools_from_mount(mount, &m.id, MountTier::Global);
        library.entries.extend(found);
        library.errors.extend(errors);
    }
    library
}

/// Resolve the roster for one invoker, against the live DB.
///
/// Iterates tier buckets explicitly rather than flattening the pool: flattening
/// dedups into a set and throws away the tier attribution that shadowing and
/// `pascalMeta` both depend on.
pub fn resolve_custom_tool_roster(
    ctx: &RosterContext,
    main: &Connection,
    mount: &Connection,
) -> CustomToolRoster {
    let pool = resolve_tiered_mount_pool(
        main,
        mount,
        &TierContext {
            user_id: Some(ctx.user_id.clone()),
            character_id: ctx.character_id.clone(),
            character_mount_point_id: ctx.character_mount_point_id.clone(),
            character_ids: ctx.character_ids.clone(),
            project_id: ctx.project_id.clone(),
        },
        &TierResolveOptions {
            include_participants: true,
            ..Default::default()
        },
    );

    resolve_roster_from_pool(
        &pool,
        |mount_point_id, tier| load_tools_from_mount(mount, mount_point_id, tier),
        || load_invoker_metadata(ctx, main, mount),
    )
}

#[cfg(test)]
mod read_tool_file_tests {
    //! The two `read_tool_file` arms the fixture differential cannot stage.
    //!
    //! v4 pins the same two with jest stubs, for the same reason: the race skip
    //! needs a file to vanish BETWEEN the listing and the read, and the traversal
    //! refusal needs a definition path neither listing can produce (`readdir`
    //! names carry no `/`, and `isRootToolFile` rejects any database link path
    //! with a segment after `Tools/`). Everything reachable through a real store
    //! is proven by `pascal_definition_reader_equivalence` against v4's real code.

    use super::*;

    fn database_mount() -> MountServiceInfo {
        MountServiceInfo {
            id: "mp-1".into(),
            mount_type: "database".into(),
            base_path: None,
            exclude_patterns: Vec::new(),
            include_patterns: Vec::new(),
            name: "store".into(),
            enabled: true,
            conversion_status: "idle".into(),
        }
    }

    /// A mount-index connection carrying only the tables the reader's database
    /// branch touches before it gives up.
    fn empty_links_db() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(
            "CREATE TABLE doc_mount_files (
               id TEXT PRIMARY KEY, sha256 TEXT, fileSizeBytes INTEGER,
               fileType TEXT, source TEXT, createdAt TEXT, updatedAt TEXT);
             CREATE TABLE doc_mount_file_links (
               id TEXT PRIMARY KEY, fileId TEXT, linkGroupId TEXT,
               mountPointId TEXT, relativePath TEXT,
               fileName TEXT, folderId TEXT, lastModified TEXT, createdAt TEXT,
               allowCharacterRead INTEGER, allowCharacterWrite INTEGER,
               extractedText TEXT, originalMimeType TEXT, conversionStatus TEXT,
               chunkCount INTEGER, description TEXT, extractionStatus TEXT,
               allowEmbed INTEGER);",
        )
        .unwrap();
        conn
    }

    /// v4: `if (error instanceof FileOpError && error.code === 'SOURCE_NOT_FOUND')
    /// continue;` — the definition listed a moment ago is gone. A race, not a
    /// defect: skipped quietly, never an error entry.
    #[test]
    fn vanished_definition_is_skipped_not_reported() {
        let conn = empty_links_db();
        let got = read_tool_file(&conn, &database_mount(), "Tools/gone.tool.json");
        assert_eq!(got, ToolFileContent::Skip);
    }

    /// The boundary check the canonical reader brought with it: a definition path
    /// can no longer resolve outside its own store. Unreachable from either
    /// listing today — this pins it so a future listing change cannot quietly
    /// reopen the door.
    #[test]
    fn escaping_definition_path_refuses() {
        let conn = empty_links_db();
        let got = read_tool_file(&conn, &database_mount(), "Tools/../../etc/passwd.tool.json");
        assert_eq!(
            got,
            ToolFileContent::ReadError(
                "Path traversal not allowed: Tools/../../etc/passwd.tool.json".into()
            )
        );
    }
}

#[cfg(test)]
mod preset_isolation_tests {
    //! Run presets (v4 `c988fbd2`) live in the very same `Tools/` folder as the
    //! definitions, as `Tools/{tool}.{preset}.settings.json`. The whole feature
    //! rests on one guarantee the spec doc states outright: roster discovery
    //! reads ONLY `Tools/*.tool.json`, so a preset file can never leak in as a
    //! tool. Nothing about a preset write is validated against the roster, so
    //! if this predicate ever widened, an operator's saved parameter values
    //! would start appearing on Pascal's Table as unloadable definitions.

    use super::*;

    #[test]
    fn a_preset_file_is_never_a_tool_definition() {
        // The shape the run dialog writes, and the two adjacent shapes.
        assert!(!is_root_tool_file(
            "Tools/coin_toss.hard-mode.settings.json"
        ));
        assert!(!is_root_tool_file("Tools/coin_toss.settings.json"));
        assert!(!is_root_tool_file("Tools/coin_toss.tool.settings.json"));
        // …while the definition itself still is.
        assert!(is_root_tool_file("Tools/coin_toss.tool.json"));
    }
}
