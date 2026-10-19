//! Wardrobe pictures for a character's Aurora gallery — every generated image
//! of every wardrobe item the character can reach (v4
//! `lib/wardrobe/gallery-images.ts`, `1cfc9b504`; P4.D321).
//!
//! Carrying v4's *why*s:
//!
//! - **"Can reach" is the character's wearable pool** ([`load_wearable_pool`]):
//!   their own vault, the stores of every group they belong to, and Quilltap
//!   General. "There is no chat here, so the project tier — which is a property
//!   of a chat, not a character — is not part of it": the project tier is
//!   passed EXPLICITLY EMPTY (`Some(&[])`), never resolved, and the options are
//!   the default (non-operator) ones — v4 passes no fourth argument.
//! - **Archived items are included** (their pictures still exist) **and
//!   flagged** — the pool's `byId` holds every reachable item, archived
//!   included (v4 `pool.ts:57`) — "and a shadowed copy is reported from the
//!   tier that wins, as everywhere else".
//! - **An archived character answers empty without reading the pool**: "A
//!   tombstone's vault may be pruned; there is nothing to show and no reason to
//!   go looking."
//! - **A picture is a `files` row linked to its item**, read for every item at
//!   once ([`FilesRepository::find_by_linked_to_any`], in pool order) and kept
//!   only when it is a GENERATED `IMAGE` — uploaded, imported and document rows
//!   are not wardrobe pictures in the gallery's sense. A file linked to several
//!   items is captioned by the FIRST `linkedTo` id the pool holds.
//! - **Newest first by `createdAt` as a STRING**, descending; ties keep the
//!   file read's order (V8's sort is stable, as `slice::sort_by` is).
//!
//! Read-only: managing an item's pictures stays with the item editor.

use std::collections::HashMap;

use rusqlite::Connection;
use serde_json::{Map, Value};

use crate::db::archetype_wardrobe::GENERAL_WARDROBE_NAME;
use crate::db::files::{FileRow, FilesRepository};
use crate::db::vault_read_overlay::VaultUnavailable;
use crate::db::{characters_read, DbError};
use crate::services::wardrobe_item_images::service::{
    wardrobe_image_thumbnail_url, wardrobe_image_url,
};
use crate::wearable_pool::{is_archived_truthy, load_wearable_pool, LoadWearablePoolOptions};

/// v4 `LOG_CONTEXT`.
const LOG_CONTEXT: &str = "wardrobe.gallery-images";

/// v4 `OWN_WARDROBE_LABEL` — the tier label for the character's own items
/// (v4's `wardrobeOriginLabel` answers `null` there, and the service's `??`
/// supplies this).
pub const OWN_WARDROBE_LABEL: &str = "Own wardrobe";

/// Why the listing failed.
#[derive(Debug)]
pub enum WardrobeGalleryError {
    /// v4's `throw new Error(\`Character not found: ${characterId}\`)`.
    CharacterNotFound(String),
    /// The overlaid character read refused a vault with no usable keystone —
    /// v4's `CharacterVaultUnavailableError`, whose MESSAGE the route answers
    /// in its 500 (the route catches before the middleware's 503 mapping can).
    VaultUnavailable(VaultUnavailable),
    /// Any other read failure (v4: the character read's `findById` throwing;
    /// `findByLinkedToAny` cannot — its reads are fallbacks).
    Db(DbError),
}

impl std::fmt::Display for WardrobeGalleryError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            WardrobeGalleryError::CharacterNotFound(id) => write!(f, "Character not found: {id}"),
            WardrobeGalleryError::VaultUnavailable(u) => write!(f, "{}", u.message()),
            WardrobeGalleryError::Db(e) => write!(f, "{e}"),
        }
    }
}

impl std::error::Error for WardrobeGalleryError {}

impl From<DbError> for WardrobeGalleryError {
    fn from(e: DbError) -> Self {
        match e {
            DbError::StoreUnavailable {
                entity_label: "character",
                id,
                mount_point_id: Some(mount_id),
                ..
            } => WardrobeGalleryError::VaultUnavailable(VaultUnavailable {
                character_id: id,
                mount_id,
            }),
            other => WardrobeGalleryError::Db(other),
        }
    }
}

/// The server twin of v4's `wardrobeOriginLabel(origin) ?? OWN_WARDROBE_LABEL`
/// (`wardrobe-container.ts:66-78` + `gallery-images.ts:108`) over a pool item's
/// `origin` JSON (`with_origin`'s `{ scope, id, name }`): `general` →
/// `Shared · Quilltap General`; `project` → `Project · <name>` (unreachable
/// here — the gallery has no project tier — but the twin is whole); `group` →
/// `Group · <name>`; `character`, an absent `origin` or an unknown scope →
/// `Own wardrobe`. The separator is U+00B7 with one ASCII space each side;
/// `<name>` is interpolated whatever it holds (an empty name gives `Group · `).
pub fn wardrobe_gallery_location_label(origin: Option<&Value>) -> String {
    let scope = origin.and_then(|o| o.get("scope")).and_then(Value::as_str);
    let name = || js_string(origin.and_then(|o| o.get("name")).unwrap_or(&Value::Null));
    match scope {
        Some("general") => format!("Shared \u{b7} {GENERAL_WARDROBE_NAME}"),
        Some("project") => format!("Project \u{b7} {}", name()),
        Some("group") => format!("Group \u{b7} {}", name()),
        _ => OWN_WARDROBE_LABEL.to_string(),
    }
}

/// JS template-literal interpolation of a JSON value (`${v}`): a string as
/// itself, `null` / absent as `null`, anything else through its JSON text.
fn js_string(v: &Value) -> String {
    match v {
        Value::String(s) => s.clone(),
        other => other.to_string(),
    }
}

/// v4 `listWardrobeGalleryImages(repos, characterId)` → the BARE `{ entries,
/// total }`.
///
/// Lines: DEBUG `[WardrobeGallery] Archived character — no wardrobe pictures`
/// `{context, characterId}` on the archived short-circuit; DEBUG
/// `[WardrobeGallery] Listed wardrobe pictures` `{context, characterId,
/// itemCount, linkedFileCount, returned}` once the files are read (NOT on the
/// empty-pool return — v4 returns before it).
pub fn list_wardrobe_gallery_images(
    main: &Connection,
    mount: &Connection,
    character_id: &str,
) -> Result<Value, WardrobeGalleryError> {
    list_with(
        character_id,
        || characters_read::find_by_id(main, mount, character_id),
        || {
            load_wearable_pool(
                main,
                mount,
                character_id,
                Some(&[]),
                LoadWearablePoolOptions::default(),
            )
            // v4 `Array.from(pool.byId.values())` — the Map's insertion order.
            .by_id_ordered()
            .cloned()
            .collect()
        },
        |ids| FilesRepository::new(main).find_by_linked_to_any(ids),
    )
}

/// The service over its three reads — v4's own seams, the ones its
/// `gallery-images.test.ts` mocks (`repos.characters.findById`,
/// `loadWearablePool`, `repos.files.findByLinkedToAny`), so the transcribed
/// cases can prove the pool is never read for an archived character and the
/// files never for an empty pool.
fn list_with(
    character_id: &str,
    find_character: impl FnOnce() -> Result<Option<Value>, DbError>,
    load_pool: impl FnOnce() -> Vec<Value>,
    find_files: impl FnOnce(&[&str]) -> Result<Vec<FileRow>, DbError>,
) -> Result<Value, WardrobeGalleryError> {
    let Some(character) = find_character()? else {
        return Err(WardrobeGalleryError::CharacterNotFound(
            character_id.to_string(),
        ));
    };
    // JS truthiness of `character.archivedAt` (the character schema only ever
    // carries a string or null there).
    if is_archived_truthy(&character) {
        tracing::debug!(
            context = LOG_CONTEXT,
            characterId = %character_id,
            "[WardrobeGallery] Archived character — no wardrobe pictures"
        );
        return Ok(empty());
    }

    // "No chat, so no project tier: pass it explicitly empty."
    let items = load_pool();
    if items.is_empty() {
        return Ok(empty());
    }

    let ids = pool_item_ids(&items);
    let id_refs: Vec<&str> = ids.iter().map(String::as_str).collect();
    let files = find_files(&id_refs)?;
    let entries = gallery_entries(&items, &files);

    tracing::debug!(
        context = LOG_CONTEXT,
        characterId = %character_id,
        itemCount = items.len(),
        linkedFileCount = files.len(),
        returned = entries.len(),
        "[WardrobeGallery] Listed wardrobe pictures"
    );

    let total = entries.len();
    let mut body = Map::new();
    body.insert("entries".to_string(), Value::Array(entries));
    body.insert("total".to_string(), Value::from(total));
    Ok(Value::Object(body))
}

fn empty() -> Value {
    let mut body = Map::new();
    body.insert("entries".to_string(), Value::Array(Vec::new()));
    body.insert("total".to_string(), Value::from(0));
    Value::Object(body)
}

/// The pool items' ids, in pool order — the `findByLinkedToAny` argument
/// (v4's test pins `['i-own', 'i-grp', 'i-gen']`).
fn pool_item_ids(items: &[Value]) -> Vec<String> {
    items
        .iter()
        .map(|item| {
            item.get("id")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string()
        })
        .collect()
}

/// v4 `:89-115` — the GENERATED `IMAGE` rows, each captioned by its first
/// in-pool `linkedTo` item, newest first. `items` in pool order; `files` in
/// the read's order.
fn gallery_entries(items: &[Value], files: &[FileRow]) -> Vec<Value> {
    // v4 `new Map(items.map((item) => [item.id, item]))`.
    let items_by_id: HashMap<&str, &Value> = items
        .iter()
        .map(|item| {
            (
                item.get("id").and_then(Value::as_str).unwrap_or_default(),
                item,
            )
        })
        .collect();
    let by_id = |id: &str| items_by_id.get(id).copied();
    let mut entries: Vec<Value> = Vec::new();
    for file in files {
        if file.category != "IMAGE" || file.source != "GENERATED" {
            continue;
        }
        // `file.linkedTo.map((id) => itemsById.get(id)).find(Boolean)` — a
        // non-string element is a Map miss (`undefined`), skipped.
        let Some(item) = file
            .linked_to
            .iter()
            .filter_map(Value::as_str)
            .find_map(by_id)
        else {
            continue;
        };
        entries.push(entry(file, item));
    }
    // `a.createdAt < b.createdAt ? 1 : a.createdAt > b.createdAt ? -1 : 0` —
    // a JS string compare runs on UTF-16 code units; a stored ISO timestamp is
    // ASCII (a non-ASCII `createdAt` cannot pass `FileEntrySchema`), so bytes
    // order the same. `sort_by` is stable, as V8's is: ties keep the read order.
    entries.sort_by(|a, b| created_at(b).cmp(created_at(a)));
    entries
}

fn created_at(entry: &Value) -> &str {
    entry
        .get("createdAt")
        .and_then(Value::as_str)
        .unwrap_or_default()
}

fn opt(v: &Option<String>) -> Value {
    v.as_ref().map_or(Value::Null, |s| Value::String(s.clone()))
}

/// One entry, in v4's literal key order (`gallery-images.ts:94-112`).
fn entry(file: &FileRow, item: &Value) -> Value {
    let item_id = item.get("id").and_then(Value::as_str).unwrap_or_default();
    let origin = item.get("origin");
    let mut e = Map::new();
    e.insert("fileId".into(), Value::String(file.id.clone()));
    e.insert(
        "fileName".into(),
        Value::String(file.original_filename.clone()),
    );
    e.insert("url".into(), Value::String(wardrobe_image_url(&file.id)));
    e.insert(
        "thumbnailUrl".into(),
        Value::String(wardrobe_image_thumbnail_url(&file.id)),
    );
    // `file.mimeType ?? null` — never null in practice: `FileRow.mime_type`
    // is required, as `FileEntrySchema`'s `mimeType` is.
    e.insert("mimeType".into(), Value::String(file.mime_type.clone()));
    e.insert("fileSizeBytes".into(), Value::from(file.size));
    e.insert("width".into(), file.width.map_or(Value::Null, Value::from));
    e.insert(
        "height".into(),
        file.height.map_or(Value::Null, Value::from),
    );
    // `String(file.createdAt)` of the stored ISO string — the same bytes.
    e.insert("createdAt".into(), Value::String(file.created_at.clone()));
    e.insert("generationPrompt".into(), opt(&file.generation_prompt));
    e.insert("generationModel".into(), opt(&file.generation_model));
    e.insert("itemId".into(), Value::String(item_id.to_string()));
    e.insert(
        "itemTitle".into(),
        item.get("title").cloned().unwrap_or(Value::Null),
    );
    e.insert(
        "scope".into(),
        origin
            .and_then(|o| o.get("scope"))
            .cloned()
            .unwrap_or(Value::Null),
    );
    e.insert(
        "locationLabel".into(),
        Value::String(wardrobe_gallery_location_label(origin)),
    );
    e.insert("itemArchived".into(), Value::Bool(is_archived_truthy(item)));
    e.insert(
        "isCurrentItemImage".into(),
        Value::Bool(item.get("imageFileId").and_then(Value::as_str) == Some(file.id.as_str())),
    );
    Value::Object(e)
}

#[cfg(test)]
mod tests {
    use std::cell::{Cell, RefCell};

    use serde_json::json;

    use super::*;
    use crate::test_support::captured_with;

    const CHAR: &str = "char-1";

    fn item(id: &str, title: &str, origin: Value, extra: Value) -> Value {
        let mut v = json!({ "id": id, "title": title, "types": ["top"], "origin": origin });
        for (k, val) in extra.as_object().unwrap() {
            v.as_object_mut().unwrap().insert(k.clone(), val.clone());
        }
        v
    }

    /// v4's test `file(id, linkedTo, extra)` — GENERATED IMAGE defaults.
    fn file(id: &str, linked_to: &[&str], f: impl FnOnce(&mut FileRow)) -> FileRow {
        let mut row = FileRow {
            id: id.to_string(),
            user_id: "u".into(),
            sha256: None,
            original_filename: format!("{id}.webp"),
            mime_type: "image/webp".into(),
            size: 100,
            width: None,
            height: None,
            is_plain_text: None,
            linked_to: linked_to.iter().map(|s| json!(s)).collect(),
            source: "GENERATED".into(),
            category: "IMAGE".into(),
            generation_prompt: None,
            generation_model: None,
            generation_revised_prompt: None,
            generation_key: None,
            description: None,
            tags: vec![],
            project_id: None,
            folder_path: None,
            storage_key: None,
            file_status: None,
            created_at: "2026-10-01T00:00:00.000Z".into(),
            updated_at: "2026-10-01T00:00:00.000Z".into(),
        };
        f(&mut row);
        row
    }

    fn ids(v: &Value) -> Vec<&str> {
        v["entries"]
            .as_array()
            .unwrap()
            .iter()
            .map(|e| e["fileId"].as_str().unwrap())
            .collect()
    }

    /// v4 `captions generated pictures across tiers, newest first`.
    #[test]
    fn captions_generated_pictures_across_tiers_newest_first() {
        let own = item(
            "i-own",
            "Velvet Coat",
            json!({ "scope": "character", "id": CHAR, "name": "" }),
            json!({ "imageFileId": "f-own" }),
        );
        let grp = item(
            "i-grp",
            "Guild Sash",
            json!({ "scope": "group", "id": "g1", "name": "The Guild" }),
            json!({ "archivedAt": "2026-09-01T00:00:00.000Z" }),
        );
        let gen = item(
            "i-gen",
            "Plain Boots",
            json!({ "scope": "general", "id": null, "name": "Quilltap General" }),
            json!({}),
        );
        let asked: RefCell<Vec<String>> = RefCell::new(Vec::new());
        let result = list_with(
            CHAR,
            || Ok(Some(json!({ "id": CHAR, "name": "Ada" }))),
            || vec![own, grp, gen],
            |ids| {
                *asked.borrow_mut() = ids.iter().map(|s| s.to_string()).collect();
                Ok(vec![
                    file("f-own", &["i-own"], |f| {
                        f.created_at = "2026-10-01T00:00:00.000Z".into()
                    }),
                    file("f-grp", &["i-grp"], |f| {
                        f.created_at = "2026-10-03T00:00:00.000Z".into()
                    }),
                    file("f-gen", &["i-gen"], |f| {
                        f.created_at = "2026-10-02T00:00:00.000Z".into()
                    }),
                    file("f-upload", &["i-own"], |f| f.source = "UPLOADED".into()),
                    file("f-doc", &["i-own"], |f| f.category = "DOCUMENT".into()),
                    file("f-stray", &["not-reachable"], |_| {}),
                ])
            },
        )
        .unwrap();

        // The files read is asked in POOL order.
        assert_eq!(*asked.borrow(), vec!["i-own", "i-grp", "i-gen"]);
        assert_eq!(result["total"], 3);
        assert_eq!(ids(&result), vec!["f-grp", "f-gen", "f-own"]);
        let e = result["entries"].as_array().unwrap();
        assert_eq!(e[0]["itemTitle"], "Guild Sash");
        assert_eq!(e[0]["scope"], "group");
        assert_eq!(e[0]["locationLabel"], "Group · The Guild");
        assert_eq!(e[0]["itemArchived"], true);
        assert_eq!(e[0]["isCurrentItemImage"], false);
        assert_eq!(e[1]["locationLabel"], "Shared · Quilltap General");
        assert_eq!(e[2]["itemTitle"], "Velvet Coat");
        assert_eq!(e[2]["locationLabel"], OWN_WARDROBE_LABEL);
        assert_eq!(e[2]["isCurrentItemImage"], true);
        assert_eq!(e[2]["url"], "/api/v1/files/f-own");
        assert_eq!(e[2]["thumbnailUrl"], "/api/v1/files/f-own?action=thumbnail");
        // C5's key order, and `{ entries, total }` in that order.
        assert_eq!(
            serde_json::to_string(&e[2]).unwrap(),
            "{\"fileId\":\"f-own\",\"fileName\":\"f-own.webp\",\"url\":\"/api/v1/files/f-own\",\
             \"thumbnailUrl\":\"/api/v1/files/f-own?action=thumbnail\",\"mimeType\":\"image/webp\",\
             \"fileSizeBytes\":100,\"width\":null,\"height\":null,\
             \"createdAt\":\"2026-10-01T00:00:00.000Z\",\"generationPrompt\":null,\
             \"generationModel\":null,\"itemId\":\"i-own\",\"itemTitle\":\"Velvet Coat\",\
             \"scope\":\"character\",\"locationLabel\":\"Own wardrobe\",\"itemArchived\":false,\
             \"isCurrentItemImage\":true}"
        );
        assert!(serde_json::to_string(&result)
            .unwrap()
            .starts_with("{\"entries\":["));
    }

    /// v4 `returns nothing for an archived character without reading the pool`.
    #[test]
    fn returns_nothing_for_an_archived_character_without_reading_the_pool() {
        let pool_read = Cell::new(false);
        let (result, lines) = captured_with(|| {
            list_with(
                CHAR,
                || {
                    Ok(Some(
                        json!({ "id": CHAR, "archivedAt": "2026-09-01T00:00:00.000Z" }),
                    ))
                },
                || {
                    pool_read.set(true);
                    vec![]
                },
                |_| panic!("no files read for an archived character"),
            )
        });
        assert_eq!(
            serde_json::to_string(&result.unwrap()).unwrap(),
            "{\"entries\":[],\"total\":0}"
        );
        assert!(!pool_read.get(), "the pool was read");
        let archived: Vec<&String> = lines
            .iter()
            .filter(|l| l.contains("[WardrobeGallery] Archived character — no wardrobe pictures"))
            .collect();
        assert_eq!(archived.len(), 1, "{lines:#?}");
        assert!(archived[0].starts_with("DEBUG "));
        assert!(archived[0].contains(" context=wardrobe.gallery-images"));
        assert!(archived[0].contains(" characterId=char-1"));
        assert!(!lines.iter().any(|l| l.contains("Listed wardrobe pictures")));
    }

    /// v4 `throws for a missing character`.
    #[test]
    fn throws_for_a_missing_character() {
        let err = list_with(
            "missing-id",
            || Ok(None),
            || panic!("no pool read"),
            |_| panic!("no files read"),
        )
        .unwrap_err();
        assert!(matches!(err, WardrobeGalleryError::CharacterNotFound(_)));
        assert_eq!(err.to_string(), "Character not found: missing-id");
    }

    /// v4 `skips the file lookup when the pool is empty` — and logs no Listed
    /// line (v4 returns before it).
    #[test]
    fn skips_the_file_lookup_when_the_pool_is_empty() {
        let (result, lines) = captured_with(|| {
            list_with(
                CHAR,
                || Ok(Some(json!({ "id": CHAR }))),
                Vec::new,
                |_| panic!("no files read for an empty pool"),
            )
        });
        assert_eq!(
            serde_json::to_string(&result.unwrap()).unwrap(),
            "{\"entries\":[],\"total\":0}"
        );
        assert!(
            !lines.iter().any(|l| l.contains("[WardrobeGallery]")),
            "{lines:#?}"
        );
    }

    #[test]
    fn the_listed_line_counts_the_pool_the_validated_files_and_the_entries() {
        let (_, lines) = captured_with(|| {
            list_with(
                CHAR,
                || Ok(Some(json!({ "id": CHAR }))),
                || {
                    vec![item(
                        "i-own",
                        "Coat",
                        json!({ "scope": "character", "id": CHAR, "name": "" }),
                        json!({}),
                    )]
                },
                |_| {
                    Ok(vec![
                        file("a", &["i-own"], |_| {}),
                        file("b", &["i-own"], |f| f.source = "UPLOADED".into()),
                    ])
                },
            )
        });
        let listed: Vec<&String> = lines
            .iter()
            .filter(|l| l.contains("[WardrobeGallery] Listed wardrobe pictures"))
            .collect();
        assert_eq!(listed.len(), 1, "{lines:#?}");
        for field in [
            " context=wardrobe.gallery-images",
            " characterId=char-1",
            " itemCount=1",
            " linkedFileCount=2",
            " returned=1",
        ] {
            assert!(listed[0].contains(field), "{field} in {}", listed[0]);
        }
    }

    /// v4 `file.linkedTo.map((id) => itemsById.get(id)).find(Boolean)` — the
    /// FIRST in-pool id captions the plate; a non-pool id (or a non-string
    /// element) is skipped; no in-pool id drops the plate.
    #[test]
    fn the_first_in_pool_linked_id_captions_the_plate() {
        let pool = || {
            vec![
                item("coat", "Coat", json!({ "scope": "character" }), json!({})),
                item("scarf", "Scarf", json!({ "scope": "character" }), json!({})),
            ]
        };
        let result = list_with(
            CHAR,
            || Ok(Some(json!({ "id": CHAR }))),
            pool,
            |_| {
                Ok(vec![
                    file("two", &["coat", "scarf"], |f| {
                        f.created_at = "2026-10-04T00:00:00.000Z".into()
                    }),
                    file("skip", &["smock", "scarf"], |f| {
                        f.created_at = "2026-10-03T00:00:00.000Z".into()
                    }),
                    file("odd", &[], |f| {
                        f.linked_to = vec![json!(7), json!("coat")];
                        f.created_at = "2026-10-02T00:00:00.000Z".into();
                    }),
                    file("none", &["ghost"], |_| {}),
                ])
            },
        )
        .unwrap();
        let titles: Vec<&str> = result["entries"]
            .as_array()
            .unwrap()
            .iter()
            .map(|e| e["itemTitle"].as_str().unwrap())
            .collect();
        assert_eq!(titles, vec!["Coat", "Scarf", "Coat"]);
        assert_eq!(result["total"], 3);
    }

    /// v4's comparator returns 0 on a tie and V8's sort is stable: the read
    /// order survives.
    #[test]
    fn a_created_at_tie_keeps_the_read_order() {
        let result = list_with(
            CHAR,
            || Ok(Some(json!({ "id": CHAR }))),
            || vec![item("i", "I", json!({ "scope": "character" }), json!({}))],
            |_| {
                Ok(vec![
                    file("first", &["i"], |_| {}),
                    file("newer", &["i"], |f| {
                        f.created_at = "2026-10-09T00:00:00.000Z".into()
                    }),
                    file("second", &["i"], |_| {}),
                ])
            },
        )
        .unwrap();
        assert_eq!(ids(&result), vec!["newer", "first", "second"]);
    }

    #[test]
    fn a_vault_unavailable_character_read_answers_v4s_message() {
        let err = list_with(
            "c9",
            || {
                Err(DbError::StoreUnavailable {
                    entity_label: "character",
                    id: "c9".into(),
                    mount_point_id: Some("mp9".into()),
                    message: "applyDocumentStoreOverlayOne: vault unavailable for character c9 (mount mp9)".into(),
                })
            },
            || panic!("no pool read"),
            |_| panic!("no files read"),
        )
        .unwrap_err();
        assert_eq!(
            err.to_string(),
            "Character c9 has no usable vault (characterDocumentMountPointId=mp9): properties.json missing"
        );
    }

    // --- the tier label (R-2) ---------------------------------------------

    #[test]
    fn the_tier_label_twins_v4s_wardrobe_origin_label() {
        let label = |v: Value| wardrobe_gallery_location_label(Some(&v));
        assert_eq!(
            label(json!({ "scope": "general", "id": null, "name": "Quilltap General" })),
            "Shared · Quilltap General"
        );
        assert_eq!(
            label(json!({ "scope": "group", "id": "g", "name": "The Guild" })),
            "Group · The Guild"
        );
        assert_eq!(
            label(json!({ "scope": "project", "id": "p", "name": "Lighthouse" })),
            "Project · Lighthouse"
        );
        assert_eq!(
            label(json!({ "scope": "character", "id": "c", "name": "" })),
            "Own wardrobe"
        );
        assert_eq!(wardrobe_gallery_location_label(None), "Own wardrobe");
        assert_eq!(
            label(json!({ "scope": "attic", "name": "x" })),
            "Own wardrobe"
        );
        // v4 interpolates whatever `name` holds.
        assert_eq!(label(json!({ "scope": "group", "name": "" })), "Group · ");
        // The separator: ASCII space, U+00B7 (C2 B7), ASCII space.
        assert_eq!(
            &label(json!({ "scope": "group", "name": "G" })).as_bytes()[5..9],
            &[0x20, 0xC2, 0xB7, 0x20]
        );
    }

    #[test]
    fn the_general_label_names_the_general_wardrobe() {
        // The ONE source of the name the general label carries.
        assert_eq!(GENERAL_WARDROBE_NAME, "Quilltap General");
    }
}
