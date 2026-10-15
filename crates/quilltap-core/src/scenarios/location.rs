//! Scenario Location — v4 `lib/scenarios/location.ts` (`545e299bb`, pinned at
//! `83334b9a5`): the one answer to "where does this scenario live, and how do
//! I read and write there".
//!
//! A location is one owner's `Scenarios/` folder: a character's vault, a
//! project's official store, a group's official store, or Quilltap General.
//! Routes, the chat-side resolver, the instance-wide lister and the startup
//! heal all address a scenario through this; nobody switches on scope to pick
//! a reader or a writer, and a [`ScenarioRef`] (`{ mountPointId, path }`) is
//! the one pointer that names a scenario anywhere.
//!
//! Provisioning is opt-in: `ensure: true` creates a project/group store and its
//! `Scenarios/` folder when missing, and is for callers about to write. A read
//! probe passes nothing and gets `None` for a store that doesn't exist yet.
//!
//! The character tier is different on disk (a `# Heading` title, ids hashed
//! from the file path, the default in `characters.defaultScenarioId`) and the
//! vault overlay re-projects the whole `character.scenarios` array over the
//! folder on every write, deleting any file it did not write. So the character
//! location reads the folder directly and is READ-ONLY: character scenarios are
//! written through the characters repository, which keeps the vault projection
//! the single writer. (The squash's fourth commit deleted the character
//! location's writes — port nothing for them; §R.4(k).)
//!
//! Archived characters are tombstones: they resolve and classify as `None`, so
//! a pruned vault is never browsed, plucked from, or written through here.
//!
//! ## Connections
//!
//! Every function takes the main and mount-index connections v4's `repos`
//! stand for. A read probe only reads. `ensure: true` and the store writes
//! WRITE both databases, so their callers run them inside the writer closure
//! (`with_both_conns`) — which is also why their lines are pinned through the
//! global capture (the writer thread is invisible to the thread-scoped rig).

use rusqlite::Connection;
use serde_json::{Map, Value};

use crate::db::chats::ScenarioRef;
use crate::db::database_store::{
    delete_database_document, move_database_document, write_database_document, StoreError,
};
use crate::db::doc_mount_documents::{DocMountDocumentsRepository, VaultFolderDoc};
use crate::db::doc_mount_file_links::DocMountFileLinksRepository;
use crate::db::ensure_official_store::{ensure_owner_official_store, StoreOwnerKind};
use crate::db::instance_settings::get_general_mount_point_id;
use crate::db::scenarios::{
    build_scenario_file_content, build_scenario_list, is_scenario_content_archived,
    parse_scenario_doc, resolve_scenario_path, set_scenario_default_in_folder, ListScenariosResult,
    ParsedScenario, ScenarioListLogContext, ScenarioPathResolution, ScenarioWriteError,
    SCENARIOS_FOLDER,
};
use crate::db::{characters_read, groups, projects, DbError};
use crate::scenarios::decode_uri::{js_decode_uri_component, UriMalformed};
use crate::vault_overlay::{
    parse_scenario_file, sanitize_file_name, stable_uuid_from_string, VaultDoc,
};

/// v4 `ScenarioScopeSchema` (`location.ts:62`) — the four kinds of owner.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScenarioScope {
    Character,
    Project,
    Group,
    General,
}

impl ScenarioScope {
    /// v4's wire string.
    pub fn as_str(self) -> &'static str {
        match self {
            ScenarioScope::Character => "character",
            ScenarioScope::Project => "project",
            ScenarioScope::Group => "group",
            ScenarioScope::General => "general",
        }
    }
}

/// v4 `GENERAL_OWNER_NAME` (`location.ts:73`) — General's display name as an
/// owner.
pub const GENERAL_OWNER_NAME: &str = "Quilltap General";

/// The location's failures, `Display` = v4's messages.
#[derive(Debug)]
pub enum ScenarioLocationError {
    /// v4 `ScenarioNotFoundError(path)` — `Scenario not found: <path>`.
    NotFound(String),
    /// v4 `ScenarioConflictError(filename)` — `A scenario named "<f>" already exists`.
    Conflict(String),
    /// v4 `ScenarioFilenameError(message)` — a filename that sanitises to nothing.
    Filename(String),
    /// V8's `URIError` from `normalizeScenarioPath`'s decode — v4 THROWS (R-Z).
    Uri(UriMalformed),
    /// Anything else v4 would throw as a plain `Error` (a store write, a DB
    /// read, `setScenarioDefaultInFolder`'s not-found).
    Other(String),
}

impl std::fmt::Display for ScenarioLocationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ScenarioLocationError::NotFound(p) => write!(f, "Scenario not found: {p}"),
            ScenarioLocationError::Conflict(n) => {
                write!(f, "A scenario named \"{n}\" already exists")
            }
            ScenarioLocationError::Filename(m) | ScenarioLocationError::Other(m) => f.write_str(m),
            ScenarioLocationError::Uri(e) => write!(f, "{e}"),
        }
    }
}

impl std::error::Error for ScenarioLocationError {}

impl From<DbError> for ScenarioLocationError {
    fn from(e: DbError) -> Self {
        ScenarioLocationError::Other(crate::db::fallback::error_text(&e))
    }
}
impl From<StoreError> for ScenarioLocationError {
    fn from(e: StoreError) -> Self {
        ScenarioLocationError::Other(crate::db::scenarios::store_error_text(&e))
    }
}
impl From<ScenarioWriteError> for ScenarioLocationError {
    fn from(e: ScenarioWriteError) -> Self {
        ScenarioLocationError::Other(e.to_string())
    }
}
impl From<UriMalformed> for ScenarioLocationError {
    fn from(e: UriMalformed) -> Self {
        ScenarioLocationError::Uri(e)
    }
}

/// v4 `characterScenarioIdForPath(mountPointId, path)` (`location.ts:157-159`):
/// ref → character-scenario id, a pure function of the ref —
/// `stableUuidFromString('scenario:<mountPointId>:<path>')`, byte-identical to
/// the id the vault parser gives the same file.
pub fn character_scenario_id_for_path(mount_point_id: &str, path: &str) -> String {
    stable_uuid_from_string(&format!("scenario:{mount_point_id}:{path}"))
}

/// v4 `normalizeScenarioPath(path)` (`location.ts:192-196`) =
/// `resolveScenarioPath(path, 'Scenarios')`: `decodeURIComponent` FIRST, then
/// trim, then the `Scenarios/<file>.md` rules. The outer `Err` is V8's
/// `URIError` — v4 THROWS on a malformed escape rather than refusing, so every
/// caller propagates it (R-Z); the inner resolution is ok-or-reason.
///
/// This is the decode the item routes' boundary seam
/// ([`crate::db::scenarios::resolve_scenario_path`], no decode) relies on
/// (ruling L-6): v4 decodes an item route's `[scenarioPath]` twice — once for
/// the HTTP encoding the factory undoes, once here — and v5's dispatch never
/// HTTP-encodes, so this single decode equals v4's pair on the logical path.
pub fn normalize_scenario_path(path: &str) -> Result<ScenarioPathResolution, UriMalformed> {
    let decoded = js_decode_uri_component(path)?;
    Ok(resolve_scenario_path(&decoded, SCENARIOS_FOLDER))
}

/// v4's private `requirePath`: a path that does not normalise is
/// `ScenarioNotFoundError(<the raw path>)`; a malformed escape throws through.
fn require_path(path: &str) -> Result<String, ScenarioLocationError> {
    match normalize_scenario_path(path)? {
        ScenarioPathResolution::Ok(p) => Ok(p),
        ScenarioPathResolution::Err(_) => Err(ScenarioLocationError::NotFound(path.to_string())),
    }
}

/// v4's private `cleanFilename(filename, emptyMessage)`:
/// `sanitizeFileName(f).replace(/\.md$/i, '')`, empty → `ScenarioFilenameError`.
fn clean_filename(filename: &str, empty_message: &str) -> Result<String, ScenarioLocationError> {
    let sanitized = sanitize_file_name(filename);
    let cleaned = strip_md_suffix(&sanitized);
    if cleaned.is_empty() {
        return Err(ScenarioLocationError::Filename(empty_message.to_string()));
    }
    Ok(cleaned)
}

/// `/\.md$/i` stripped once.
fn strip_md_suffix(name: &str) -> String {
    let b = name.as_bytes();
    let n = b.len();
    if n >= 3
        && b[n - 3] == b'.'
        && b[n - 2].eq_ignore_ascii_case(&b'm')
        && b[n - 1].eq_ignore_ascii_case(&b'd')
    {
        name[..n - 3].to_string()
    } else {
        name.to_string()
    }
}

/// v4 `parseScenarioForScope(scope, doc, { characterId, defaultScenarioId })`
/// (`location.ts:166-189`): a store scope parses with `parseScenarioDoc`; a
/// character file with the vault parser, mapped to a [`ParsedScenario`] in
/// v4's key order, `isDefault` / `rawIsDefault` both
/// `!!defaultScenarioId && parsed.id === defaultScenarioId` (the default is a
/// COLUMN, not frontmatter).
pub fn parse_scenario_for_scope(
    scope: ScenarioScope,
    doc: &VaultFolderDoc,
    default_scenario_id: Option<&str>,
) -> Option<ParsedScenario> {
    if scope != ScenarioScope::Character {
        return parse_scenario_doc(doc);
    }
    let parsed = parse_scenario_file(&VaultDoc {
        content: &doc.content,
        mount_point_id: &doc.mount_point_id,
        relative_path: &doc.relative_path,
        file_name: &doc.file_name,
        created_at: &doc.created_at,
        updated_at: &doc.updated_at,
    })?;
    let is_default = default_scenario_id.is_some_and(|d| !d.is_empty() && parsed.id == d);
    Some(ParsedScenario {
        path: doc.relative_path.clone(),
        filename: strip_md_suffix(&doc.file_name),
        name: parsed.title,
        description: parsed.description,
        is_default,
        raw_is_default: is_default,
        archived: parsed.archived == Some(true),
        body: parsed.content,
        last_modified: doc.last_modified.clone(),
        created_at: doc.created_at.clone(),
        updated_at: doc.updated_at.clone(),
    })
}

/// v4 `docMountDocuments.findByMountPointAndPath(mp, path)` as the location
/// reads it: a fallback `withRawDb` — a failed read logs `Error finding
/// document by mount point and path` and answers `null`.
fn find_doc(mount: &Connection, mount_point_id: &str, path: &str) -> Option<VaultFolderDoc> {
    crate::db::fallback::document_by_mount_point_and_path_or_none(mount_point_id, path, || {
        DocMountDocumentsRepository::new(mount)
            .find_with_link_by_mount_point_and_path(mount_point_id, path)
    })
}

/// v4 `findManyByMountPointsInFolder([mp], 'Scenarios', '.md')` (fallback `[]`).
fn folder_docs(
    mount: &Connection,
    mount_point_id: &str,
) -> Result<Vec<VaultFolderDoc>, ScenarioLocationError> {
    Ok(
        DocMountDocumentsRepository::new(mount).find_many_by_mount_points_in_folder_or_empty(
            &[mount_point_id.to_string()],
            SCENARIOS_FOLDER,
            ".md",
        )?,
    )
}

/// v4 `CreateScenarioInput` — `createScenarioSchema`'s validated shape.
#[derive(Debug, Clone, Default)]
pub struct CreateScenarioInput {
    pub filename: String,
    pub name: Option<String>,
    pub description: Option<String>,
    pub is_default: Option<bool>,
    pub archived: Option<bool>,
    pub body: String,
}

/// v4 `UpdateScenarioInput` — `updateScenarioSchema`'s validated shape.
#[derive(Debug, Clone, Default)]
pub struct UpdateScenarioInput {
    pub name: Option<String>,
    pub description: Option<String>,
    pub is_default: Option<bool>,
    pub archived: Option<bool>,
    pub body: String,
}

/// v4 `storeLocation(...)` (`location.ts:214-318`) — a file-backed owner's
/// folder (General, a project, a group) with its writes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoreScenarioLocation {
    scope: ScenarioScope,
    id: Option<String>,
    mount_point_id: String,
    owner_name: String,
}

/// v4 `characterLocation(...)` (`location.ts:324-357`) — READ-ONLY.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CharacterScenarioLocation {
    id: String,
    mount_point_id: String,
    owner_name: String,
    /// The character's default is a column on the row this location was built
    /// from; nothing writes through a character location, so that copy holds.
    default_scenario_id: Option<String>,
}

/// v4 `ScenarioLocation` — one owner's `Scenarios/` folder, for reading.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ScenarioLocation {
    Store(StoreScenarioLocation),
    Character(CharacterScenarioLocation),
}

impl ScenarioLocation {
    pub fn scope(&self) -> ScenarioScope {
        match self {
            ScenarioLocation::Store(s) => s.scope,
            ScenarioLocation::Character(_) => ScenarioScope::Character,
        }
    }

    /// Owner id; `None` for General.
    pub fn id(&self) -> Option<&str> {
        match self {
            ScenarioLocation::Store(s) => s.id.as_deref(),
            ScenarioLocation::Character(c) => Some(&c.id),
        }
    }

    pub fn mount_point_id(&self) -> &str {
        match self {
            ScenarioLocation::Store(s) => &s.mount_point_id,
            ScenarioLocation::Character(c) => &c.mount_point_id,
        }
    }

    /// Display name of the owner: character name, project name, group name,
    /// `Quilltap General`.
    pub fn owner_name(&self) -> &str {
        match self {
            ScenarioLocation::Store(s) => &s.owner_name,
            ScenarioLocation::Character(c) => &c.owner_name,
        }
    }

    /// The writable half, when this is a store location.
    pub fn as_store(&self) -> Option<&StoreScenarioLocation> {
        match self {
            ScenarioLocation::Store(s) => Some(s),
            ScenarioLocation::Character(_) => None,
        }
    }

    /// v4 `idForPath(path)`: the id a character-tier scenario carries; `None`
    /// for the other scopes.
    pub fn id_for_path(&self, path: &str) -> Option<String> {
        match self {
            ScenarioLocation::Store(_) => None,
            ScenarioLocation::Character(c) => {
                Some(character_scenario_id_for_path(&c.mount_point_id, path))
            }
        }
    }

    /// v4 `parse(doc)` with the scope's parser (a character's own default).
    pub fn parse(&self, doc: &VaultFolderDoc) -> Option<ParsedScenario> {
        match self {
            ScenarioLocation::Store(_) => parse_scenario_doc(doc),
            ScenarioLocation::Character(c) => parse_scenario_for_scope(
                ScenarioScope::Character,
                doc,
                c.default_scenario_id.as_deref(),
            ),
        }
    }

    fn log_context(&self) -> ScenarioListLogContext {
        ScenarioListLogContext {
            scope: self.scope().as_str().to_string(),
            id: self.id().map(str::to_string),
            mount_point_id: self.mount_point_id().to_string(),
        }
    }

    /// v4 `list({ includeArchived })`: ONE folder query → `buildScenarioList`.
    pub fn list(
        &self,
        mount: &Connection,
        include_archived: bool,
    ) -> Result<ListScenariosResult, ScenarioLocationError> {
        let docs = folder_docs(mount, self.mount_point_id())?;
        Ok(build_scenario_list(
            docs,
            |d| self.parse(d),
            include_archived,
            &self.log_context(),
        ))
    }

    /// v4 `read(path)`: one scenario, archived included; `isDefault` reflects
    /// only the file (or column) itself. A path that does not normalise is
    /// `NotFound`; a malformed escape throws (`Uri`).
    pub fn read(
        &self,
        mount: &Connection,
        path: &str,
    ) -> Result<Option<ParsedScenario>, ScenarioLocationError> {
        let resolved = require_path(path)?;
        Ok(find_doc(mount, self.mount_point_id(), &resolved).and_then(|d| self.parse(&d)))
    }
}

impl StoreScenarioLocation {
    pub fn scope(&self) -> ScenarioScope {
        self.scope
    }
    pub fn id(&self) -> Option<&str> {
        self.id.as_deref()
    }
    pub fn mount_point_id(&self) -> &str {
        &self.mount_point_id
    }
    pub fn owner_name(&self) -> &str {
        &self.owner_name
    }

    fn as_location(&self) -> ScenarioLocation {
        ScenarioLocation::Store(self.clone())
    }

    /// v4 `list` (the store parser).
    pub fn list(
        &self,
        mount: &Connection,
        include_archived: bool,
    ) -> Result<ListScenariosResult, ScenarioLocationError> {
        self.as_location().list(mount, include_archived)
    }

    /// v4 `read` (the store parser).
    pub fn read(
        &self,
        mount: &Connection,
        path: &str,
    ) -> Result<Option<ParsedScenario>, ScenarioLocationError> {
        self.as_location().read(mount, path)
    }

    fn log_write(&self, message: &'static str, path: &str) {
        tracing::debug!(
            scope = self.scope.as_str(),
            id = self.id.as_deref().unwrap_or("null"),
            mountPointId = %self.mount_point_id,
            path = %path,
            "{message}"
        );
    }

    /// v4 `create(input)` (`location.ts:244-264`): clean the filename, refuse a
    /// conflict, write, set the default when asked, then re-read — a file that
    /// parses to nothing (a whitespace-only body passes `body.min(1)`) is
    /// `NotFound` AFTER the write (Δ12).
    pub fn create(
        &self,
        mount: &Connection,
        input: &CreateScenarioInput,
    ) -> Result<ParsedScenario, ScenarioLocationError> {
        let cleaned = clean_filename(
            &input.filename,
            "Filename cannot be empty after sanitisation",
        )?;
        let path = format!("{SCENARIOS_FOLDER}/{cleaned}.md");
        if find_doc(mount, &self.mount_point_id, &path).is_some() {
            return Err(ScenarioLocationError::Conflict(cleaned));
        }
        let content = build_scenario_file_content(
            input.name.as_deref(),
            input.description.as_deref(),
            input.is_default.unwrap_or(false),
            input.archived.unwrap_or(false),
            &input.body,
        );
        write_database_document(mount, &self.mount_point_id, &path, &content)?;
        if input.is_default == Some(true) {
            set_scenario_default_in_folder(
                mount,
                &self.mount_point_id,
                Some(&path),
                SCENARIOS_FOLDER,
            )?;
        }
        self.log_write("[ScenarioLocation] Created scenario", &path);
        self.read(mount, &path)?
            .ok_or(ScenarioLocationError::NotFound(path))
    }

    /// v4 `update(path, patch)` (`location.ts:265-287`): the serializer
    /// rewrites the whole file, so an unmentioned `archived` keeps the file's
    /// current flag; re-read (null → `NotFound`).
    pub fn update(
        &self,
        mount: &Connection,
        path: &str,
        patch: &UpdateScenarioInput,
    ) -> Result<ParsedScenario, ScenarioLocationError> {
        let resolved = require_path(path)?;
        let Some(existing) = find_doc(mount, &self.mount_point_id, &resolved) else {
            return Err(ScenarioLocationError::NotFound(resolved));
        };
        let content = build_scenario_file_content(
            patch.name.as_deref(),
            patch.description.as_deref(),
            patch.is_default.unwrap_or(false),
            patch
                .archived
                .unwrap_or_else(|| is_scenario_content_archived(&existing.content)),
            &patch.body,
        );
        write_database_document(mount, &self.mount_point_id, &resolved, &content)?;
        if patch.is_default == Some(true) {
            set_scenario_default_in_folder(
                mount,
                &self.mount_point_id,
                Some(&resolved),
                SCENARIOS_FOLDER,
            )?;
        }
        self.log_write("[ScenarioLocation] Updated scenario", &resolved);
        self.read(mount, &resolved)?
            .ok_or(ScenarioLocationError::NotFound(resolved))
    }

    /// v4 `rename(path, filename)` (`location.ts:288-301`): the move (and its
    /// line) only when the path changes; then ALWAYS re-read the new path, so
    /// a same-name rename of a MISSING file is `NotFound` (Δ11).
    pub fn rename(
        &self,
        mount: &Connection,
        path: &str,
        filename: &str,
    ) -> Result<ParsedScenario, ScenarioLocationError> {
        let resolved = require_path(path)?;
        let cleaned = clean_filename(filename, "newFilename cannot be empty after sanitisation")?;
        let new_path = format!("{SCENARIOS_FOLDER}/{cleaned}.md");
        if new_path != resolved {
            if find_doc(mount, &self.mount_point_id, &resolved).is_none() {
                return Err(ScenarioLocationError::NotFound(resolved));
            }
            if find_doc(mount, &self.mount_point_id, &new_path).is_some() {
                return Err(ScenarioLocationError::Conflict(cleaned));
            }
            move_database_document(mount, &self.mount_point_id, &resolved, &new_path)?;
            tracing::debug!(
                scope = self.scope.as_str(),
                id = self.id.as_deref().unwrap_or("null"),
                mountPointId = %self.mount_point_id,
                from = %resolved,
                to = %new_path,
                "[ScenarioLocation] Renamed scenario"
            );
        }
        self.read(mount, &new_path)?
            .ok_or(ScenarioLocationError::NotFound(new_path))
    }

    /// v4 `delete(path)` (`location.ts:302-307`): nothing deleted → `NotFound`.
    pub fn delete(&self, mount: &Connection, path: &str) -> Result<(), ScenarioLocationError> {
        let resolved = require_path(path)?;
        if !delete_database_document(mount, &self.mount_point_id, &resolved)? {
            return Err(ScenarioLocationError::NotFound(resolved));
        }
        self.log_write("[ScenarioLocation] Deleted scenario", &resolved);
        Ok(())
    }

    /// v4 `setDefault(path | null)` (`location.ts:308-310`): `None` CLEARS the
    /// default (Δ17).
    pub fn set_default(
        &self,
        mount: &Connection,
        path: Option<&str>,
    ) -> Result<(), ScenarioLocationError> {
        let resolved = match path {
            None => None,
            Some(p) => Some(require_path(p)?),
        };
        set_scenario_default_in_folder(
            mount,
            &self.mount_point_id,
            resolved.as_deref(),
            SCENARIOS_FOLDER,
        )?;
        Ok(())
    }
}

/// A raw character row's cells the location reads.
struct CharacterRow {
    id: String,
    name: String,
    user_id: String,
    archived: bool,
    vault: Option<String>,
    default_scenario_id: Option<String>,
}

impl CharacterRow {
    fn from_raw(row: &Value) -> Self {
        let s = |k: &str| row.get(k).and_then(Value::as_str).map(str::to_string);
        // v4 tests `character.archivedAt` for TRUTHINESS: '' is live.
        let archived = row.get("archivedAt").is_some_and(|v| match v {
            Value::Null => false,
            Value::String(s) => !s.is_empty(),
            Value::Bool(b) => *b,
            _ => true,
        });
        Self {
            id: s("id").unwrap_or_default(),
            name: s("name").unwrap_or_default(),
            user_id: s("userId").unwrap_or_default(),
            archived,
            vault: s("characterDocumentMountPointId").filter(|m| !m.is_empty()),
            default_scenario_id: s("defaultScenarioId"),
        }
    }

    /// v4 `usableCharacter(row, userId)` (`location.ts:150-154`): the caller's,
    /// live, with a vault.
    fn usable(self, user_id: &str) -> Option<CharacterScenarioLocation> {
        if self.user_id != user_id || self.archived {
            return None;
        }
        let mount_point_id = self.vault?;
        Some(CharacterScenarioLocation {
            id: self.id,
            mount_point_id,
            owner_name: self.name,
            default_scenario_id: self.default_scenario_id,
        })
    }
}

/// v4 `ensureScenariosFolder(mp)` (`location.ts:408-418`): heal `Scenarios/`;
/// a failure is the writer's to see — WARN `[ScenarioLocation] Failed to
/// ensure Scenarios folder {mountPointId, error}`, then RETHROW (the pre-span
/// General helper swallowed it).
fn ensure_scenarios_folder(
    mount: &Connection,
    mount_point_id: &str,
) -> Result<(), ScenarioLocationError> {
    if let Err(error) =
        DocMountFileLinksRepository::new(mount).ensure_folder_path(mount_point_id, SCENARIOS_FOLDER)
    {
        let text = crate::db::fallback::error_text(&error);
        tracing::warn!(
            mountPointId = %mount_point_id,
            error = %text,
            "[ScenarioLocation] Failed to ensure Scenarios folder"
        );
        return Err(ScenarioLocationError::Other(text));
    }
    Ok(())
}

/// v4 `resolveScenarioLocation(scope, id, repos, userId, { ensure })`
/// (`location.ts:369-401`): the location, or `None` when the owner is missing
/// (or, for a character, not `user_id`'s, archived, or vaultless), when a
/// non-General scope arrives without an id, or when the backing store doesn't
/// exist and `ensure` is off. ALWAYS logs DEBUG `[ScenarioLocation] Resolved
/// scenario location {scope, id, ensure, found, mountPointId, context}` after
/// a resolve that returns (a throw escapes before it).
pub fn resolve_scenario_location(
    main: &Connection,
    mount: &Connection,
    scope: ScenarioScope,
    id: Option<&str>,
    user_id: &str,
    ensure: bool,
) -> Result<Option<ScenarioLocation>, ScenarioLocationError> {
    let location = resolve(main, mount, scope, id, user_id, ensure)?;
    tracing::debug!(
        scope = scope.as_str(),
        id = id.unwrap_or("null"),
        ensure,
        found = location.is_some(),
        mountPointId = location.as_ref().map_or("null", |l| l.mount_point_id()),
        context = "scenarios",
        "[ScenarioLocation] Resolved scenario location"
    );
    Ok(location)
}

/// [`resolve_scenario_location`] for a store scope, answering the writable
/// half (v4's overload `StoreScenarioScope → StoreScenarioLocation | null`).
pub fn resolve_store_scenario_location(
    main: &Connection,
    mount: &Connection,
    scope: ScenarioScope,
    id: Option<&str>,
    user_id: &str,
    ensure: bool,
) -> Result<Option<StoreScenarioLocation>, ScenarioLocationError> {
    debug_assert!(
        scope != ScenarioScope::Character,
        "the character location is read-only"
    );
    Ok(
        resolve_scenario_location(main, mount, scope, id, user_id, ensure)?.and_then(|l| match l {
            ScenarioLocation::Store(s) => Some(s),
            ScenarioLocation::Character(_) => None,
        }),
    )
}

/// v4's private `resolve` (`location.ts:420-462`).
fn resolve(
    main: &Connection,
    mount: &Connection,
    scope: ScenarioScope,
    id: Option<&str>,
    user_id: &str,
    ensure: bool,
) -> Result<Option<ScenarioLocation>, ScenarioLocationError> {
    if scope == ScenarioScope::General {
        let Some(mount_point_id) = get_general_mount_point_id(main)? else {
            return Ok(None);
        };
        if ensure {
            ensure_scenarios_folder(mount, &mount_point_id)?;
        }
        return Ok(Some(ScenarioLocation::Store(StoreScenarioLocation {
            scope,
            id: None,
            mount_point_id,
            owner_name: GENERAL_OWNER_NAME.to_string(),
        })));
    }

    // JS truthiness: '' is no id.
    let Some(id) = id.filter(|i| !i.is_empty()) else {
        return Ok(None);
    };

    if scope == ScenarioScope::Character {
        // Raw read: only the row's mount pointer and name are needed, and a
        // broken vault should cost a null rather than a throw. `ensure` is
        // ignored for characters.
        let Some(row) = characters_read::find_by_id_raw_or_none(main, id) else {
            return Ok(None);
        };
        return Ok(CharacterRow::from_raw(&row)
            .usable(user_id)
            .map(ScenarioLocation::Character));
    }

    // Raw read: only `name` and `officialMountPointId` are needed.
    let (owner, kind, fallback_name) = match scope {
        ScenarioScope::Project => (
            projects::find_by_id_raw_or_none(main, id),
            StoreOwnerKind::Project,
            "Project",
        ),
        _ => (
            groups::find_by_id_raw_or_none(main, id),
            StoreOwnerKind::Group,
            "Group",
        ),
    };
    let Some(owner) = owner else {
        return Ok(None);
    };
    let owner_id = str_field(&owner, "id").unwrap_or(id).to_string();
    let name = str_field(&owner, "name")
        .filter(|n| !n.is_empty())
        .unwrap_or(fallback_name)
        .to_string();

    let mount_point_id = if ensure {
        let Some(ensured) = ensure_owner_official_store(main, mount, kind, &owner_id, &name)?
        else {
            return Ok(None);
        };
        ensure_scenarios_folder(mount, &ensured.mount_point_id)?;
        Some(ensured.mount_point_id)
    } else {
        str_field(&owner, "officialMountPointId").map(str::to_string)
    };
    let Some(mount_point_id) = mount_point_id.filter(|m| !m.is_empty()) else {
        return Ok(None);
    };
    Ok(Some(ScenarioLocation::Store(StoreScenarioLocation {
        scope,
        id: Some(owner_id),
        mount_point_id,
        owner_name: name,
    })))
}

fn str_field<'a>(row: &'a Map<String, Value>, key: &str) -> Option<&'a str> {
    row.get(key).and_then(Value::as_str)
}

/// What [`classify`] found: the owner, and (for a character) the row it read.
struct Classified {
    scope: ScenarioScope,
    id: Option<String>,
    character: Option<CharacterRow>,
}

/// v4 `classifyScenarioMount(mp, repos, userId)` (`location.ts:465-472`): which
/// owner a mount belongs to, or `None` when it is none of the four kinds.
pub fn classify_scenario_mount(
    main: &Connection,
    mount_point_id: &str,
    user_id: &str,
) -> Result<Option<(ScenarioScope, Option<String>)>, ScenarioLocationError> {
    Ok(classify(main, mount_point_id, user_id)?.map(|c| (c.scope, c.id)))
}

/// v4's private `classify` (`location.ts:475-504`), ORDER: General → a
/// character's vault (another user's or an archived one → DEBUG `Mount
/// belongs to an unavailable character` → `None`) → a project's official
/// store → a group's. A LINKED (non-official) store or an arbitrary
/// Scriptorium store classifies as nothing.
fn classify(
    main: &Connection,
    mount_point_id: &str,
    user_id: &str,
) -> Result<Option<Classified>, ScenarioLocationError> {
    if get_general_mount_point_id(main)?.as_deref() == Some(mount_point_id) {
        return Ok(Some(Classified {
            scope: ScenarioScope::General,
            id: None,
            character: None,
        }));
    }

    if let Some(raw) = characters_read::find_by_document_mount_point_id_raw(main, mount_point_id) {
        let row = CharacterRow::from_raw(&raw);
        if row.user_id != user_id || row.archived {
            tracing::debug!(
                mountPointId = %mount_point_id,
                characterId = %row.id,
                archived = row.archived,
                "[ScenarioLocation] Mount belongs to an unavailable character"
            );
            return Ok(None);
        }
        return Ok(Some(Classified {
            scope: ScenarioScope::Character,
            id: Some(row.id.clone()),
            character: Some(row),
        }));
    }

    if let Some(project) = projects::find_by_official_mount_point_id_raw(main, mount_point_id) {
        return Ok(Some(Classified {
            scope: ScenarioScope::Project,
            id: str_field(&project, "id").map(str::to_string),
            character: None,
        }));
    }

    if let Some(group) = groups::find_by_official_mount_point_id_raw(main, mount_point_id) {
        return Ok(Some(Classified {
            scope: ScenarioScope::Group,
            id: str_field(&group, "id").map(str::to_string),
            character: None,
        }));
    }

    Ok(None)
}

/// v4 `ScenarioRefLookup` (`location.ts:507-510`) — for callers that must tell
/// a refusal from a miss.
#[derive(Debug, Clone)]
pub enum ScenarioRefLookup {
    Ok {
        location: ScenarioLocation,
        scenario: ParsedScenario,
    },
    Missing {
        location: ScenarioLocation,
        path: String,
    },
    Refused(String),
}

/// v4 `lookupScenarioRef(ref, repos, userId)` (`location.ts:517-565`):
/// `Refused` when the path is outside `Scenarios/` (checked before any store
/// read) or the mount is not one of the four kinds of owner; `Missing` when
/// the owner is fine but the file is gone. A malformed `%` escape THROWS.
pub fn lookup_scenario_ref(
    main: &Connection,
    mount: &Connection,
    scenario_ref: &ScenarioRef,
    user_id: &str,
) -> Result<ScenarioRefLookup, ScenarioLocationError> {
    let normalized = match normalize_scenario_path(&scenario_ref.path)? {
        ScenarioPathResolution::Ok(p) => p,
        ScenarioPathResolution::Err(reason) => {
            tracing::warn!(
                mountPointId = %scenario_ref.mount_point_id,
                path = %scenario_ref.path,
                reason = %reason,
                "[ScenarioLocation] Refused scenario ref: bad path"
            );
            return Ok(ScenarioRefLookup::Refused(reason));
        }
    };

    let Some(owner) = classify(main, &scenario_ref.mount_point_id, user_id)? else {
        tracing::warn!(
            mountPointId = %scenario_ref.mount_point_id,
            path = %normalized,
            "[ScenarioLocation] Refused scenario ref: mount is not a scenario owner"
        );
        return Ok(ScenarioRefLookup::Refused(NOT_A_SCENARIO_STORE.to_string()));
    };

    // A character's row was read to classify the mount; build its location
    // from that row rather than reading it again.
    let scope = owner.scope;
    let owner_id = owner.id.clone();
    let location = match owner.character {
        Some(row) => row.usable(user_id).map(ScenarioLocation::Character),
        None => resolve_scenario_location(main, mount, scope, owner_id.as_deref(), user_id, false)?,
    };
    let Some(location) = location else {
        tracing::warn!(
            mountPointId = %scenario_ref.mount_point_id,
            scope = scope.as_str(),
            id = owner_id.as_deref().unwrap_or("null"),
            "[ScenarioLocation] Refused scenario ref: owner did not resolve"
        );
        return Ok(ScenarioRefLookup::Refused(NOT_A_SCENARIO_STORE.to_string()));
    };

    match location.read(mount, &normalized)? {
        Some(scenario) => Ok(ScenarioRefLookup::Ok { location, scenario }),
        None => {
            tracing::debug!(
                mountPointId = %scenario_ref.mount_point_id,
                path = %normalized,
                "[ScenarioLocation] Scenario ref points at a missing file"
            );
            Ok(ScenarioRefLookup::Missing {
                location,
                path: normalized,
            })
        }
    }
}

/// v4's refusal reason for a mount that holds no scenarios.
pub const NOT_A_SCENARIO_STORE: &str = "That store does not hold scenarios";

#[cfg(test)]
mod fallback_read_tests {
    use super::*;

    const PEPPER: &str = "dGVzdHBlcHBlcnRlc3RwZXBwZXJ0ZXN0cGVwcGVyMDE=";

    /// A provisioned instance's mount index with `Scenarios/a.md` written into a
    /// database store, then `doc_mount_documents.content` renamed (the column
    /// only the documents read names).
    fn planted(dir: &tempfile::TempDir) -> (crate::db::Writer, String) {
        crate::services::provisioning::provision_fresh_instance(dir.path(), PEPPER).unwrap();
        let w =
            crate::db::Writer::open_writable(&dir.path().join("quilltap-mount-index.db"), PEPPER)
                .unwrap();
        let mp: String = w
            .connection()
            .query_row(
                "SELECT id FROM doc_mount_points WHERE mountType = 'database' LIMIT 1",
                [],
                |r| r.get(0),
            )
            .unwrap();
        DocMountFileLinksRepository::new(w.connection())
            .write_database_document(&mp, "Scenarios/a.md", "---\nname: A\n---\nbody")
            .unwrap();
        w.connection()
            .execute_batch("ALTER TABLE doc_mount_documents RENAME COLUMN content TO content_x")
            .unwrap();
        (w, mp)
    }

    fn line(mp: &str, path: &str) -> String {
        format!("ERROR quilltap::db Error finding document by mount point and path collection=doc_mount_documents mountPointId={mp} relativePath={path} error=no such column: d.content")
    }

    /// P4.142 G2 (re-homed by P4.D295 onto the store location — the writes
    /// moved here from `api::scenarios`): the scenario writes' lookups are
    /// v4's FALLBACKS. A failed read is the not-found arm (update / rename →
    /// `NotFound`), and for the create's conflict check "no conflict", so the
    /// write itself is what fails next — after v4's repository line, never the
    /// read's own `Err`.
    #[test]
    fn the_scenario_lookups_fall_back_as_v4() {
        let dir = tempfile::tempdir().unwrap();
        let (w, mp) = planted(&dir);
        let conn = w.connection();
        let store = StoreScenarioLocation {
            scope: ScenarioScope::Project,
            id: Some("p".into()),
            mount_point_id: mp.clone(),
            owner_name: "P".into(),
        };

        let (got, lines) = crate::test_support::captured_with(|| {
            store.update(
                conn,
                "Scenarios/a.md",
                &UpdateScenarioInput {
                    name: Some("B".into()),
                    body: "b".into(),
                    ..Default::default()
                },
            )
        });
        assert!(
            matches!(got, Err(ScenarioLocationError::NotFound(_))),
            "{got:?} {lines:#?}"
        );
        assert_eq!(lines, vec![line(&mp, "Scenarios/a.md")]);

        let (got, lines) =
            crate::test_support::captured_with(|| store.rename(conn, "Scenarios/a.md", "b"));
        assert!(
            matches!(got, Err(ScenarioLocationError::NotFound(_))),
            "{got:?}"
        );
        assert_eq!(lines, vec![line(&mp, "Scenarios/a.md")]);

        let (got, lines) = crate::test_support::captured_with(|| {
            store.create(
                conn,
                &CreateScenarioInput {
                    filename: "c".into(),
                    name: Some("C".into()),
                    body: "c".into(),
                    ..Default::default()
                },
            )
        });
        assert!(
            !matches!(got, Err(ScenarioLocationError::Conflict(_))),
            "a failed conflict read must not read as a conflict: {got:?}"
        );
        assert_eq!(
            lines.first(),
            Some(&line(&mp, "Scenarios/c.md")),
            "{lines:#?}"
        );
    }
}
