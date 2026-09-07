//! The vault reads/writes + the selection resolver (v4
//! `lib/subprompts/subprompts.ts` `:146-395` — P4.D163 unit 3).
//!
//! Every function here takes the two borrowed connections the vault overlay
//! needs (`main` for the slim character row, `mount` for the store) and is
//! synchronous, so the API handlers run one op inside one `db.write` closure
//! exactly as the `characterPrompt*` family does. Pinned tier-2 by
//! `subprompts_storage_tier2_equivalence` over the committed
//! `subprompts-{main,mount}.db` pair, driving v4's REAL module at the pin.
//!
//! Vault resolution is SPLIT on purpose (v4's comments, carried): reads use
//! `findByIdRaw` overlay-free — "so a broken vault does not turn a listing
//! into a throw" — and **archived characters still resolve** ("a chat that
//! still carries the seat should keep compiling the same prompt"); writes
//! refuse an archived character ("the vault is a tombstone") and provision a
//! vault for a live character that somehow lacks one, "the same posture as
//! the wardrobe writers".

use rusqlite::Connection;
use serde_json::Value;

use super::{
    compose_subprompt_content, id_from_relative_path, is_root_subprompt_file,
    is_valid_subprompt_id, parse_subprompt_content, slugify_subprompt_title, subprompt_path_for_id,
    validate_content, validate_title, Subprompt, SubpromptForPrompt, SubpromptNotFoundError,
    SubpromptValidationError, SUBPROMPTS_FOLDER,
};
use crate::clock::iso_from_unix_ms;
use crate::collation::locale_compare;
use crate::db::character_vault::ensure_character_vault;
use crate::db::database_store::{
    delete_database_document, list_database_files, read_database_document, write_database_document,
    DbStoreErrorCode, StoreError,
};
use crate::db::doc_mount_file_links::DocMountFileLinksRepository;
use crate::db::vault_character_write::CharacterVaultWriteInput;
use crate::db::{characters_read, DbError};

/// The tracing target for the module's own lines (v4
/// `createServiceLogger('Subprompts')`).
pub const LOG_TARGET: &str = "quilltap::subprompts";

/// The four ways a storage op fails, kept as v4's four error classes so the
/// route layer can map each to its status (`SubpromptNotFoundError` → 404,
/// `SubpromptValidationError` → 400, `CharacterArchivedError` → 409, anything
/// else → 500).
#[derive(Debug)]
pub enum SubpromptError {
    NotFound(SubpromptNotFoundError),
    Validation(SubpromptValidationError),
    /// v4 `CharacterArchivedError(characterId)` — the vault is a tombstone.
    Archived {
        character_id: String,
    },
    /// A `DatabaseStoreError` other than the NOT_FOUND arms the module
    /// handles itself (e.g. an unsupported extension), or a DB failure.
    Store(StoreError),
    Db(DbError),
}

impl std::fmt::Display for SubpromptError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SubpromptError::NotFound(e) => write!(f, "{e}"),
            SubpromptError::Validation(e) => write!(f, "{e}"),
            // v4 `CharacterArchivedError(characterId)` with its default message.
            SubpromptError::Archived { character_id } => write!(
                f,
                "Character {character_id} is archived: this character is archived; rehydrate it to continue"
            ),
            SubpromptError::Store(e) => write!(f, "{e}"),
            SubpromptError::Db(e) => write!(f, "{e}"),
        }
    }
}

impl std::error::Error for SubpromptError {}

impl From<DbError> for SubpromptError {
    fn from(e: DbError) -> Self {
        SubpromptError::Db(e)
    }
}

impl From<StoreError> for SubpromptError {
    fn from(e: StoreError) -> Self {
        match e {
            StoreError::Db(d) => SubpromptError::Db(d),
            other => SubpromptError::Store(other),
        }
    }
}

impl From<SubpromptValidationError> for SubpromptError {
    fn from(e: SubpromptValidationError) -> Self {
        SubpromptError::Validation(e)
    }
}

/// v4 `createCharacterSubprompt`'s `input`.
#[derive(Debug, Clone)]
pub struct SubpromptCreateInput {
    pub title: String,
    pub content: String,
}

/// v4 `updateCharacterSubprompt`'s `patch` — each field validated ONLY when
/// present (`!== undefined`), else the current value stays.
#[derive(Debug, Clone, Default)]
pub struct SubpromptPatch {
    pub title: Option<String>,
    pub content: Option<String>,
}

fn is_not_found(e: &StoreError) -> bool {
    matches!(e, StoreError::Store(s) if s.code == DbStoreErrorCode::NotFound)
}

/// A JS-truthy string read of a raw row's field (`character?.<key> ?? null`
/// followed by a `!vaultId` test — an empty string is falsy to both).
fn truthy_str(row: &Value, key: &str) -> Option<String> {
    row.get(key)
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
}

// ── Vault resolution ─────────────────────────────────────────────────────────

/// The vault id for reads: overlay-free lookup so a broken vault does not turn
/// a listing into a throw. `None` when the character is missing or has no
/// vault. Archived characters still resolve.
fn resolve_vault_for_read(
    main: &Connection,
    character_id: &str,
) -> Result<Option<String>, DbError> {
    Ok(characters_read::find_by_id_raw(main, character_id)?
        .and_then(|c| truthy_str(&c, "characterDocumentMountPointId")))
}

/// The vault id for writes. Refuses an archived character and provisions a
/// vault for a live character that lacks one (v4 `ensureCharacterVault(
/// character)` over the RAW row — the `CharacterVaultWriteInput` is the row
/// through serde with every managed field at its default, the
/// `post_office::deliver` / `tools::list_email` idiom), warning as v4 does.
fn resolve_vault_for_write(
    main: &Connection,
    mount: &Connection,
    character_id: &str,
) -> Result<String, SubpromptError> {
    let Some(character) = characters_read::find_by_id_raw(main, character_id)? else {
        return Err(SubpromptError::NotFound(SubpromptNotFoundError {
            character_id: character_id.to_string(),
            subprompt_id: "(character)".to_string(),
        }));
    };
    if crate::api::characters::is_archived(&character) {
        return Err(SubpromptError::Archived {
            character_id: character_id.to_string(),
        });
    }
    if let Some(vault) = truthy_str(&character, "characterDocumentMountPointId") {
        return Ok(vault);
    }
    let name = character
        .get("name")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let input: CharacterVaultWriteInput =
        serde_json::from_value(character.clone()).unwrap_or_default();
    let ensured = ensure_character_vault(main, mount, character_id, name, &input, None)?;
    tracing::warn!(
        target: LOG_TARGET,
        character_id = %character_id,
        mount_point_id = %ensured.mount_point_id,
        "Provisioned a vault for a character with none before writing a subprompt"
    );
    Ok(ensured.mount_point_id)
}

// ── Reads ────────────────────────────────────────────────────────────────────

/// List a vault's subprompts, sorted by title (then id). A missing or empty
/// folder is `[]`, never an error; a file that fails to read is skipped with a
/// warning so one bad file cannot hide the rest.
pub fn list_subprompts_in_vault(
    mount: &Connection,
    vault_id: &str,
) -> Result<Vec<Subprompt>, DbError> {
    let entries = list_database_files(mount, vault_id, Some(SUBPROMPTS_FOLDER))?;
    let mut out: Vec<Subprompt> = Vec::new();
    for entry in entries
        .iter()
        .filter(|e| e.kind != "folder" && is_root_subprompt_file(&e.relative_path))
    {
        match read_database_document(mount, vault_id, &entry.relative_path) {
            Ok(doc) => {
                let updated_at = iso_from_unix_ms(doc.mtime_ms);
                out.push(parse_subprompt_content(
                    &id_from_relative_path(&entry.relative_path),
                    &doc.content,
                    &updated_at,
                ));
            }
            Err(error) => {
                tracing::warn!(
                    target: LOG_TARGET,
                    vault_id = %vault_id,
                    relative_path = %entry.relative_path,
                    error = %error,
                    "Skipping unreadable subprompt file"
                );
            }
        }
    }
    // `a.title.localeCompare(b.title) || a.id.localeCompare(b.id)` — ICU
    // en-US, the collator every v5 `localeCompare` sort shares.
    out.sort_by(|a, b| {
        locale_compare(&a.title, &b.title).then_with(|| locale_compare(&a.id, &b.id))
    });
    tracing::debug!(
        target: LOG_TARGET,
        vault_id = %vault_id,
        count = out.len(),
        "Listed subprompts"
    );
    Ok(out)
}

/// List a character's subprompts. No vault → `[]`.
pub fn list_character_subprompts(
    main: &Connection,
    mount: &Connection,
    character_id: &str,
) -> Result<Vec<Subprompt>, DbError> {
    let Some(vault_id) = resolve_vault_for_read(main, character_id)? else {
        tracing::debug!(
            target: LOG_TARGET,
            character_id = %character_id,
            "Character has no vault; no subprompts"
        );
        return Ok(vec![]);
    };
    list_subprompts_in_vault(mount, &vault_id)
}

/// Read one subprompt by id. `None` when absent (or the character has no
/// vault); any store error other than NOT_FOUND propagates.
pub fn read_character_subprompt(
    main: &Connection,
    mount: &Connection,
    character_id: &str,
    subprompt_id: &str,
) -> Result<Option<Subprompt>, SubpromptError> {
    if !is_valid_subprompt_id(subprompt_id) {
        return Ok(None);
    }
    let Some(vault_id) = resolve_vault_for_read(main, character_id)? else {
        return Ok(None);
    };
    match read_database_document(mount, &vault_id, &subprompt_path_for_id(subprompt_id)) {
        Ok(doc) => Ok(Some(parse_subprompt_content(
            subprompt_id,
            &doc.content,
            &iso_from_unix_ms(doc.mtime_ms),
        ))),
        Err(e) if is_not_found(&e) => Ok(None),
        Err(e) => Err(e.into()),
    }
}

/// Resolve a participant's selection to the subprompts that still exist, in
/// the order they appear in the vault listing (by title) so the prompt is
/// stable regardless of the order boxes were ticked. Ids that no longer match
/// a file are dropped with a debug log — a deleted subprompt must never sink a
/// turn. Fails soft to `[]` on any read error for the same reason.
///
/// `found.length !== wanted.size` compares against the SET of lowercased ids
/// (a duplicated selection is one wanted id).
pub fn resolve_selected_subprompts(
    main: &Connection,
    mount: &Connection,
    character_id: &str,
    selected_ids: &[String],
) -> Vec<SubpromptForPrompt> {
    if selected_ids.is_empty() {
        return vec![];
    }
    match list_character_subprompts(main, mount, character_id) {
        Ok(all) => {
            let wanted: std::collections::HashSet<String> =
                selected_ids.iter().map(|id| id.to_lowercase()).collect();
            let found: Vec<&Subprompt> = all
                .iter()
                .filter(|s| wanted.contains(&s.id.to_lowercase()))
                .collect();
            if found.len() != wanted.len() {
                tracing::debug!(
                    target: LOG_TARGET,
                    character_id = %character_id,
                    selected = selected_ids.len(),
                    found = found.len(),
                    "Some selected subprompts no longer exist"
                );
            }
            found
                .into_iter()
                .map(|s| SubpromptForPrompt {
                    title: s.title.clone(),
                    content: s.content.clone(),
                })
                .collect()
        }
        Err(error) => {
            tracing::warn!(
                target: LOG_TARGET,
                character_id = %character_id,
                error = %error,
                "Failed to resolve selected subprompts — continuing without them"
            );
            vec![]
        }
    }
}

// ── Writes ───────────────────────────────────────────────────────────────────

/// Create a subprompt. Validates the title THEN the content, ensures the
/// `Subprompts/` folder exists (EVERY time — "ensureFolderPath is idempotent
/// (mkdir -p); writeDatabaseDocument also creates parents, but the explicit
/// ensure makes the folder row exist even before the first listing"), picks a
/// collision-free id from the title (`base`, `base-2`, `base-3`, … against
/// the lowercased existing ids), and writes the file.
pub fn create_character_subprompt(
    main: &Connection,
    mount: &Connection,
    character_id: &str,
    input: &SubpromptCreateInput,
) -> Result<Subprompt, SubpromptError> {
    let title = validate_title(&input.title)?;
    let content = validate_content(&input.content)?;
    let vault_id = resolve_vault_for_write(main, mount, character_id)?;

    DocMountFileLinksRepository::new(mount).ensure_folder_path(&vault_id, SUBPROMPTS_FOLDER)?;

    let existing: std::collections::HashSet<String> = list_subprompts_in_vault(mount, &vault_id)?
        .into_iter()
        .map(|s| s.id.to_lowercase())
        .collect();
    let base = slugify_subprompt_title(&title);
    let mut id = base.clone();
    let mut n = 2;
    while existing.contains(&id.to_lowercase()) {
        id = format!("{base}-{n}");
        n += 1;
    }

    let mtime = write_database_document(
        mount,
        &vault_id,
        &subprompt_path_for_id(&id),
        &compose_subprompt_content(&title, &content),
    )?;
    tracing::info!(
        target: LOG_TARGET,
        character_id = %character_id,
        vault_id = %vault_id,
        subprompt_id = %id,
        "Created subprompt"
    );
    Ok(Subprompt {
        path: subprompt_path_for_id(&id),
        id,
        title,
        content,
        updated_at: iso_from_unix_ms(mtime),
    })
}

/// Update a subprompt's title and/or content in place. The id never changes.
pub fn update_character_subprompt(
    main: &Connection,
    mount: &Connection,
    character_id: &str,
    subprompt_id: &str,
    patch: &SubpromptPatch,
) -> Result<Subprompt, SubpromptError> {
    let not_found = || {
        SubpromptError::NotFound(SubpromptNotFoundError {
            character_id: character_id.to_string(),
            subprompt_id: subprompt_id.to_string(),
        })
    };
    if !is_valid_subprompt_id(subprompt_id) {
        return Err(not_found());
    }
    let vault_id = resolve_vault_for_write(main, mount, character_id)?;
    let path = subprompt_path_for_id(subprompt_id);

    let current = match read_database_document(mount, &vault_id, &path) {
        Ok(doc) => {
            parse_subprompt_content(subprompt_id, &doc.content, &iso_from_unix_ms(doc.mtime_ms))
        }
        Err(e) if is_not_found(&e) => return Err(not_found()),
        Err(e) => return Err(e.into()),
    };

    let title = match &patch.title {
        Some(t) => validate_title(t)?,
        None => current.title,
    };
    let content = match &patch.content {
        Some(c) => validate_content(c)?,
        None => current.content,
    };

    let mtime = write_database_document(
        mount,
        &vault_id,
        &path,
        &compose_subprompt_content(&title, &content),
    )?;
    // `Object.keys(patch).filter((k) => patch[k] !== undefined)` — the PRESENT
    // keys, in v4's declaration order.
    let mut changed: Vec<&str> = Vec::new();
    if patch.title.is_some() {
        changed.push("title");
    }
    if patch.content.is_some() {
        changed.push("content");
    }
    tracing::info!(
        target: LOG_TARGET,
        character_id = %character_id,
        vault_id = %vault_id,
        subprompt_id = %subprompt_id,
        changed = ?changed,
        "Updated subprompt"
    );
    Ok(Subprompt {
        id: subprompt_id.to_string(),
        path,
        title,
        content,
        updated_at: iso_from_unix_ms(mtime),
    })
}

/// Delete a subprompt. Returns `false` when there was nothing to delete. An
/// invalid id is `false` BEFORE the vault resolves (so it never 409s), but a
/// valid one resolves the vault in the WRITE posture — an archived character
/// refuses even a delete.
pub fn delete_character_subprompt(
    main: &Connection,
    mount: &Connection,
    character_id: &str,
    subprompt_id: &str,
) -> Result<bool, SubpromptError> {
    if !is_valid_subprompt_id(subprompt_id) {
        return Ok(false);
    }
    let vault_id = resolve_vault_for_write(main, mount, character_id)?;
    let deleted = delete_database_document(mount, &vault_id, &subprompt_path_for_id(subprompt_id))?;
    tracing::info!(
        target: LOG_TARGET,
        character_id = %character_id,
        vault_id = %vault_id,
        subprompt_id = %subprompt_id,
        deleted = deleted,
        "Deleted subprompt"
    );
    Ok(deleted)
}

#[cfg(test)]
mod log_tests {
    //! The module's log lines, capture-pinned at v4's levels with v4's bags
    //! (the #103/#110/#116 class — this round's own surface). Every test runs
    //! on a fresh copy of the committed `subprompts-{main,mount}.db` pair.
    use super::*;
    use crate::db::Writer;
    use crate::test_support::captured_with;

    const TEST_PEPPER: &str = "dGVzdHBlcHBlcnRlc3RwZXBwZXJ0ZXN0cGVwcGVyMDE=";
    const CHAR_A: &str = "a1000000-0000-4000-8000-0000000000a1";
    const CHAR_C: &str = "a1000000-0000-4000-8000-0000000000c3";

    fn fixture(name: &str) -> std::path::PathBuf {
        std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../quilltap-web/tests/fixtures")
            .join(name)
    }

    fn open_pair(dir: &tempfile::TempDir) -> (Writer, Writer) {
        let main = dir.path().join("main.db");
        let mount = dir.path().join("mount.db");
        std::fs::copy(fixture("subprompts-main.db"), &main).unwrap();
        std::fs::copy(fixture("subprompts-mount.db"), &mount).unwrap();
        (
            Writer::open_writable(&main, TEST_PEPPER).unwrap(),
            Writer::open_writable(&mount, TEST_PEPPER).unwrap(),
        )
    }

    fn vault_of(main: &Connection, cid: &str) -> String {
        characters_read::find_by_id_raw(main, cid)
            .unwrap()
            .unwrap()
            .get("characterDocumentMountPointId")
            .and_then(Value::as_str)
            .unwrap()
            .to_string()
    }

    fn line<'a>(lines: &'a [String], needle: &str) -> &'a String {
        lines
            .iter()
            .find(|l| l.contains(needle))
            .unwrap_or_else(|| panic!("no `{needle}` line in {lines:?}"))
    }

    /// `Skipping unreadable subprompt file` (warn, `{vaultId, relativePath,
    /// error}`) for the link whose document row is gone, then `Listed
    /// subprompts` (debug, `{vaultId, count}`) — one bad file cannot hide the
    /// rest.
    #[test]
    fn listing_warns_on_the_unreadable_file_and_debugs_the_count() {
        let dir = tempfile::tempdir().unwrap();
        let (main, mount) = open_pair(&dir);
        let vault = vault_of(main.connection(), CHAR_A);
        let (out, lines) =
            captured_with(|| list_subprompts_in_vault(mount.connection(), &vault).unwrap());
        assert_eq!(out.len(), 5, "broken skipped, nested + txt excluded");
        let skip = line(&lines, "Skipping unreadable subprompt file");
        assert!(skip.starts_with("WARN quilltap::subprompts"), "{skip}");
        assert!(skip.contains(&format!("vault_id={vault}")), "{skip}");
        assert!(
            skip.contains("relative_path=Subprompts/broken.md"),
            "{skip}"
        );
        assert!(
            skip.contains(
                "error=Document not found in database-backed store: Subprompts/broken.md"
            ),
            "{skip}"
        );
        let listed = line(&lines, "Listed subprompts");
        assert!(listed.starts_with("DEBUG quilltap::subprompts"), "{listed}");
        assert!(listed.contains("count=5"), "{listed}");
    }

    /// `Character has no vault; no subprompts` (debug, `{characterId}`).
    #[test]
    fn listing_a_vaultless_character_debugs_and_returns_empty() {
        let dir = tempfile::tempdir().unwrap();
        let (main, mount) = open_pair(&dir);
        let (out, lines) = captured_with(|| {
            list_character_subprompts(main.connection(), mount.connection(), CHAR_C).unwrap()
        });
        assert!(out.is_empty());
        let l = line(&lines, "Character has no vault; no subprompts");
        assert!(l.starts_with("DEBUG quilltap::subprompts"), "{l}");
        assert!(l.contains(&format!("character_id={CHAR_C}")), "{l}");
    }

    /// The resolver: `Some selected subprompts no longer exist` (debug,
    /// `{characterId, selected, found}` — `selected` is the raw list length,
    /// `found` the matched count) when an id is dangling; silent when every
    /// wanted id resolves; `Failed to resolve selected subprompts — continuing
    /// without them` (warn) + `[]` when the listing itself throws.
    #[test]
    fn resolver_debugs_the_dropped_ids_and_warns_on_a_failed_listing() {
        let dir = tempfile::tempdir().unwrap();
        let (main, mount) = open_pair(&dir);
        let ids = |s: &[&str]| s.iter().map(|x| x.to_string()).collect::<Vec<_>>();
        let (out, lines) = captured_with(|| {
            resolve_selected_subprompts(
                main.connection(),
                mount.connection(),
                CHAR_A,
                &ids(&["terse", "VERSE", "gone"]),
            )
        });
        assert_eq!(out.len(), 2);
        let l = line(&lines, "Some selected subprompts no longer exist");
        assert!(l.starts_with("DEBUG quilltap::subprompts"), "{l}");
        assert!(l.contains("selected=3") && l.contains("found=2"), "{l}");

        let (_, lines) = captured_with(|| {
            resolve_selected_subprompts(
                main.connection(),
                mount.connection(),
                CHAR_A,
                &ids(&["terse", "VERSE"]),
            )
        });
        assert!(
            !lines.iter().any(|l| l.contains("no longer exist")),
            "{lines:?}"
        );

        // A mount partition with no store tables: the listing throws, the
        // resolver fails SOFT.
        let broken = tempfile::tempdir().unwrap();
        let empty = Writer::open_writable(&broken.path().join("empty.db"), TEST_PEPPER).unwrap();
        let (out, lines) = captured_with(|| {
            resolve_selected_subprompts(
                main.connection(),
                empty.connection(),
                CHAR_A,
                &ids(&["terse"]),
            )
        });
        assert!(out.is_empty());
        let l = line(
            &lines,
            "Failed to resolve selected subprompts — continuing without them",
        );
        assert!(l.starts_with("WARN quilltap::subprompts"), "{l}");
        assert!(
            l.contains(&format!("character_id={CHAR_A}")) && l.contains("error="),
            "{l}"
        );
    }

    /// The write lines: `Provisioned a vault for a character with none before
    /// writing a subprompt` (warn, `{characterId, mountPointId}`) on the
    /// vault-less character, then `Created subprompt` (info, `{characterId,
    /// vaultId, subpromptId}`); `Updated subprompt` carries `changed` (the
    /// PRESENT patch keys); `Deleted subprompt` carries `deleted`.
    #[test]
    fn write_lines_carry_v4s_bags() {
        let dir = tempfile::tempdir().unwrap();
        let (main, mount) = open_pair(&dir);
        let (created, lines) = captured_with(|| {
            create_character_subprompt(
                main.connection(),
                mount.connection(),
                CHAR_C,
                &SubpromptCreateInput {
                    title: "Fresh".into(),
                    content: "x".into(),
                },
            )
            .unwrap()
        });
        let vault = vault_of(main.connection(), CHAR_C);
        let prov = line(
            &lines,
            "Provisioned a vault for a character with none before writing a subprompt",
        );
        assert!(prov.starts_with("WARN quilltap::subprompts"), "{prov}");
        assert!(
            prov.contains(&format!("character_id={CHAR_C}"))
                && prov.contains(&format!("mount_point_id={vault}")),
            "{prov}"
        );
        let cr = line(&lines, "Created subprompt");
        assert!(cr.starts_with("INFO quilltap::subprompts"), "{cr}");
        assert!(
            cr.contains(&format!("vault_id={vault}")) && cr.contains("subprompt_id=fresh"),
            "{cr}"
        );
        assert_eq!(created.id, "fresh");

        let (_, lines) = captured_with(|| {
            update_character_subprompt(
                main.connection(),
                mount.connection(),
                CHAR_A,
                "terse",
                &SubpromptPatch {
                    title: Some("Brief".into()),
                    content: None,
                },
            )
            .unwrap()
        });
        let up = line(&lines, "Updated subprompt");
        assert!(up.starts_with("INFO quilltap::subprompts"), "{up}");
        assert!(up.contains("changed=[\"title\"]"), "{up}");

        let (_, lines) = captured_with(|| {
            delete_character_subprompt(main.connection(), mount.connection(), CHAR_A, "gone")
                .unwrap()
        });
        let del = line(&lines, "Deleted subprompt");
        assert!(del.starts_with("INFO quilltap::subprompts"), "{del}");
        assert!(
            del.contains("subprompt_id=gone") && del.contains("deleted=false"),
            "{del}"
        );
    }
}
