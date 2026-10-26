//! Inventory schemas — the objects a character carries, one `.json` file per
//! object under a tier's `Inventory/` folder (v4 `lib/schemas/
//! inventory.types.ts`, `4fbdad186` at `322a5488e`; P4.D324).
//!
//! Three shapes of one object:
//!
//!   - **the full object** (v4 `InventoryItemSchema`) — every field present.
//!     What a standalone, an archetype or a kit file holds (minus `folder`),
//!     and what every reader outside the writer sees after resolution
//!     ([`super::resolve`]) — [`ResolvedInventoryItem`] here.
//!   - **the file as stored** (v4 `InventoryItemFileSchema`) — for an
//!     instance only `id`, `parentId`, the timestamps and the fields it
//!     overrides; an omitted field inherits from the archetype. No defaults
//!     are applied (`patchSchemaOf`), so "omitted" stays omitted.
//!   - **[`InventoryRecord`]** — a parsed file with its `folder` (derived from
//!     the path): what a tier's folder holds and the writer projects.
//!
//! ## Why the record and the resolved item are ORDERED MAPS
//!
//! v4 builds both with object spreads (`{...before, ...patch}`,
//! `{...DEFAULTS, ...record}`) and answers them on the wire as-is, so their
//! KEY ORDER is observable (survey T1): V8 keeps a re-set key at its FIRST
//! insertion position and appends new keys. A typed struct cannot carry that
//! order across a spread, so both are newtypes over a `preserve_order`
//! `serde_json::Map` — `insert` on an existing key keeps its position,
//! `shift_remove` + `insert` appends — with typed accessors over the JS
//! values. A key ABSENT from the map is JS `undefined`; `Value::Null` is an
//! explicit `null` (the three states of a sparse instance file). Every family
//! that pins one compares `serde_json::to_string` bytes, never `==`
//! (`serde_json::Map`'s equality ignores order).
//!
//! The request-body parsers (v4's ten body schemas) close the module: each a
//! hand-written Zod twin answering the SAME issue list v4's `.parse` throws
//! (through [`crate::api::zod_issues`]).

use serde_json::{Map, Value};

/// `item` is one object; a `kit` names a list of objects equipped as one.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InventoryKind {
    Item,
    Kit,
}

impl InventoryKind {
    pub fn as_str(self) -> &'static str {
        match self {
            InventoryKind::Item => "item",
            InventoryKind::Kit => "kit",
        }
    }
}

/// The inline SVG glyph cap, in bytes.
pub const INVENTORY_SVG_MAX_BYTES: usize = 16 * 1024;

/// The most members a kit may name.
pub const INVENTORY_KIT_MAX_MEMBERS: usize = 200;

/// Every key a file may carry (all of the object's but `folder`), in v4's
/// `InventoryItemSchema.shape` order — the serialised file's key order.
pub const INVENTORY_FILE_KEYS: [&str; 19] = [
    "id",
    "kind",
    "name",
    "title",
    "description",
    "imageDescription",
    "equippedByDefault",
    "maxEquipped",
    "canCopy",
    "isArchetype",
    "parentId",
    "itemIds",
    "emoji",
    "svg",
    "imageFileId",
    "metadata",
    "archivedAt",
    "createdAt",
    "updatedAt",
];

/// The fields an instance inherits from its archetype when its file omits
/// them. Identity, shape (`kind`, `isArchetype`, `parentId`, `itemIds`),
/// filing and timestamps are always the instance's own.
pub const INVENTORY_INHERITABLE_FIELDS: [&str; 11] = [
    "name",
    "title",
    "description",
    "imageDescription",
    "equippedByDefault",
    "maxEquipped",
    "canCopy",
    "emoji",
    "svg",
    "imageFileId",
    "metadata",
];

/// The four tiers an object can live in (v4 `InventoryScopeSchema`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum InventoryScope {
    Character,
    Group,
    Project,
    General,
}

impl InventoryScope {
    pub const ALL: [InventoryScope; 4] = [
        InventoryScope::Character,
        InventoryScope::Group,
        InventoryScope::Project,
        InventoryScope::General,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            InventoryScope::Character => "character",
            InventoryScope::Group => "group",
            InventoryScope::Project => "project",
            InventoryScope::General => "general",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|scope| scope.as_str() == s)
    }
}

/// Where an object lives, as a collection read tags it (v4 `InventoryOrigin`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InventoryOrigin {
    pub scope: InventoryScope,
    pub id: Option<String>,
    pub name: String,
}

impl InventoryOrigin {
    /// v4's literal `{scope, id, name}`.
    pub fn to_value(&self) -> Value {
        let mut m = Map::new();
        m.insert("scope".into(), Value::String(self.scope.as_str().into()));
        m.insert(
            "id".into(),
            self.id.clone().map(Value::String).unwrap_or(Value::Null),
        );
        m.insert("name".into(), Value::String(self.name.clone()));
        Value::Object(m)
    }

    /// The origin back from a resolved item's `origin` value.
    pub fn from_value(v: &Value) -> Option<Self> {
        let o = v.as_object()?;
        Some(InventoryOrigin {
            scope: InventoryScope::parse(o.get("scope")?.as_str()?)?,
            id: o.get("id").and_then(Value::as_str).map(str::to_string),
            name: o
                .get("name")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string(),
        })
    }
}

/// A kit's state in a chat: how many of its resolvable members are equipped.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KitState {
    All,
    Some,
    None,
}

impl KitState {
    pub fn as_str(self) -> &'static str {
        match self {
            KitState::All => "all",
            KitState::Some => "some",
            KitState::None => "none",
        }
    }
}

/// JS truthiness of an optional JSON value (`undefined` is `None`).
pub fn js_truthy(v: Option<&Value>) -> bool {
    match v {
        None | Some(Value::Null) => false,
        Some(Value::Bool(b)) => *b,
        Some(Value::Number(n)) => n.as_f64().is_some_and(|f| f != 0.0 && !f.is_nan()),
        Some(Value::String(s)) => !s.is_empty(),
        Some(Value::Array(_)) | Some(Value::Object(_)) => true,
    }
}

/// The accessors shared by [`InventoryRecord`] and [`ResolvedInventoryItem`]
/// — both are V8-ordered objects of the same keys.
pub trait InventoryFields {
    fn fields(&self) -> &Map<String, Value>;

    /// The raw value (`None` = JS `undefined`).
    fn get(&self, key: &str) -> Option<&Value> {
        self.fields().get(key)
    }
    /// True when the key is present (even as `null`).
    fn has(&self, key: &str) -> bool {
        self.fields().contains_key(key)
    }
    /// A string field, or `None` (absent, null, or not a string).
    fn str_of(&self, key: &str) -> Option<&str> {
        self.get(key).and_then(Value::as_str)
    }
    fn id(&self) -> &str {
        self.str_of("id").unwrap_or("")
    }
    /// `kind ?? 'item'`.
    fn kind(&self) -> InventoryKind {
        match self.str_of("kind") {
            Some("kit") => InventoryKind::Kit,
            _ => InventoryKind::Item,
        }
    }
    fn is_kit(&self) -> bool {
        self.kind() == InventoryKind::Kit
    }
    /// `name` (an instance file may omit it — `""` then).
    fn name(&self) -> &str {
        self.str_of("name").unwrap_or("")
    }
    fn title(&self) -> Option<&str> {
        self.str_of("title")
    }
    fn description(&self) -> Option<&str> {
        self.str_of("description")
    }
    fn image_description(&self) -> Option<&str> {
        self.str_of("imageDescription")
    }
    /// `parentId` when a (truthy) string.
    fn parent_id(&self) -> Option<&str> {
        self.str_of("parentId").filter(|s| !s.is_empty())
    }
    /// `!!isArchetype`.
    fn is_archetype(&self) -> bool {
        js_truthy(self.get("isArchetype"))
    }
    /// `!!equippedByDefault`.
    fn equipped_by_default(&self) -> bool {
        js_truthy(self.get("equippedByDefault"))
    }
    /// `!!canCopy`.
    fn can_copy(&self) -> bool {
        js_truthy(self.get("canCopy"))
    }
    /// `maxEquipped ?? 1`.
    fn max_equipped(&self) -> i64 {
        self.get("maxEquipped")
            .and_then(Value::as_f64)
            .map(|f| f as i64)
            .unwrap_or(1)
    }
    /// `itemIds ?? []` (the string entries).
    fn item_ids(&self) -> Vec<String> {
        self.get("itemIds")
            .and_then(Value::as_array)
            .map(|a| {
                a.iter()
                    .filter_map(|v| v.as_str().map(str::to_string))
                    .collect()
            })
            .unwrap_or_default()
    }
    fn image_file_id(&self) -> Option<&str> {
        self.str_of("imageFileId")
    }
    /// `archivedAt` when truthy.
    fn archived_at(&self) -> Option<&str> {
        self.str_of("archivedAt").filter(|s| !s.is_empty())
    }
    fn is_archived(&self) -> bool {
        js_truthy(self.get("archivedAt"))
    }
    /// `folder ?? ''`.
    fn folder(&self) -> &str {
        self.str_of("folder").unwrap_or("")
    }
    fn created_at(&self) -> &str {
        self.str_of("createdAt").unwrap_or("")
    }
    fn updated_at(&self) -> &str {
        self.str_of("updatedAt").unwrap_or("")
    }
    /// `title || name` — v4's display name for messages.
    fn title_or_name(&self) -> &str {
        match self.title() {
            Some(t) if !t.is_empty() => t,
            _ => self.name(),
        }
    }
}

/// A parsed file with its folder: what a tier's `Inventory/` holds (v4
/// `InventoryRecord = InventoryItemFile & { folder, fileStem? }`). `fileStem`
/// is the file's current name (without `.json`): the projection names a file
/// `title ?? name`, falling back to the stem for an instance that inherits
/// both, so its file keeps its name across writes. Never serialised.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct InventoryRecord(pub Map<String, Value>);

impl InventoryFields for InventoryRecord {
    fn fields(&self) -> &Map<String, Value> {
        &self.0
    }
}

impl InventoryRecord {
    pub fn new(map: Map<String, Value>) -> Self {
        InventoryRecord(map)
    }
    /// v4 `record.fileStem`.
    pub fn file_stem(&self) -> Option<&str> {
        self.str_of("fileStem")
    }
    /// V8 `obj[key] = v`: an existing key keeps its position.
    pub fn set(&mut self, key: &str, v: Value) {
        self.0.insert(key.to_string(), v);
    }
    /// V8 `delete obj[key]` (the order of the rest kept).
    pub fn remove(&mut self, key: &str) {
        self.0.shift_remove(key);
    }
    pub fn into_map(self) -> Map<String, Value> {
        self.0
    }
    pub fn to_value(&self) -> Value {
        Value::Object(self.0.clone())
    }
}

/// An object as every reader outside the writer sees it (v4
/// `ResolvedInventoryItem = InventoryItem & { overriddenFields?,
/// parentMissing?, parentName?, origin? }`) — every full-object field
/// present, plus (for an instance) `overriddenFields` / `parentMissing` /
/// `parentName`, and the collection read's `origin` / `mountPointId`. It
/// serialises in v4's KEY ORDER (the map's).
#[derive(Debug, Clone, PartialEq, Default)]
pub struct ResolvedInventoryItem(pub Map<String, Value>);

impl InventoryFields for ResolvedInventoryItem {
    fn fields(&self) -> &Map<String, Value> {
        &self.0
    }
}

impl serde::Serialize for ResolvedInventoryItem {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        self.0.serialize(s)
    }
}

impl ResolvedInventoryItem {
    /// `metadata ?? {}`.
    pub fn metadata(&self) -> Map<String, Value> {
        self.get("metadata")
            .and_then(Value::as_object)
            .cloned()
            .unwrap_or_default()
    }
    pub fn emoji(&self) -> Option<&str> {
        self.str_of("emoji")
    }
    pub fn svg(&self) -> Option<&str> {
        self.str_of("svg")
    }
    /// `origin`, when tagged.
    pub fn origin(&self) -> Option<InventoryOrigin> {
        self.get("origin").and_then(InventoryOrigin::from_value)
    }
    pub fn mount_point_id(&self) -> Option<&str> {
        self.str_of("mountPointId")
    }
    /// `overriddenFields`, when present.
    pub fn overridden_fields(&self) -> Option<Vec<String>> {
        self.get("overriddenFields")
            .and_then(Value::as_array)
            .map(|a| {
                a.iter()
                    .filter_map(|v| v.as_str().map(str::to_string))
                    .collect()
            })
    }
    /// `parentMissing`, when present.
    pub fn parent_missing(&self) -> Option<bool> {
        self.get("parentMissing").and_then(Value::as_bool)
    }
    /// `parentName`, when present (`Some(None)` for an explicit null).
    pub fn parent_name(&self) -> Option<Option<&str>> {
        self.get("parentName").map(Value::as_str)
    }
    /// V8 `obj[key] = v`: an existing key keeps its position.
    pub fn set(&mut self, key: &str, v: Value) {
        self.0.insert(key.to_string(), v);
    }
    pub fn to_value(&self) -> Value {
        Value::Object(self.0.clone())
    }
}

/// The shape rules every object obeys (v4 `inventoryShapeIssues`): an
/// archetype is a parentless item; a kit is a parentless non-archetype with
/// members; an item names no members; an instance is never an archetype.
/// Returns the problems found, in v4's ORDER (archetype checks first —
/// parentId, then kind — then kit checks — parentId, then empty members —
/// else the item-with-members check). The input is any object carrying the
/// four keys (a body, a record, a merge of both).
pub fn inventory_shape_issues(item: &Map<String, Value>) -> Vec<String> {
    let kind = match item.get("kind").and_then(Value::as_str) {
        Some("kit") => InventoryKind::Kit,
        _ => InventoryKind::Item,
    };
    let parent = js_truthy(item.get("parentId"));
    let members = item
        .get("itemIds")
        .and_then(Value::as_array)
        .map_or(0, Vec::len);
    let mut issues = Vec::new();
    if js_truthy(item.get("isArchetype")) {
        if parent {
            issues.push("An archetype cannot derive from another archetype".to_string());
        }
        if kind != InventoryKind::Item {
            issues.push("A kit cannot be an archetype".to_string());
        }
    }
    if kind == InventoryKind::Kit {
        if parent {
            issues.push("A kit cannot be an instance".to_string());
        }
        if members == 0 {
            issues.push("A kit needs at least one member".to_string());
        }
    } else if members > 0 {
        issues.push("Only a kit names members".to_string());
    }
    issues
}

// ============================================================================
// THE ZOD TWINS — the full object, the files, and the ten request bodies
// ============================================================================
//
// One small interpreter over a field list, measured against v4's REAL schemas
// at the pin (`inventory_types_equivalence`, `cases/inventory-types.ts`):
//
//   - an object's parsed data comes back in SHAPE order, defaults filled
//     (`.default(v)` on an absent key), absent optionals omitted;
//   - a field's TYPE gate (`invalid_type`, `invalid_value`, a union) ABORTS;
//     its CHECKS (`too_small` / `too_big`, a uuid / datetime format, a
//     `.refine`) continue — so a `.refine` on a string runs after its `.max`
//     failed, and the body's `superRefine` runs only when no field issue
//     aborted ([`zod_issue_aborts`]);
//   - `.strict()` answers `unrecognized_keys` (input order) after the shape,
//     `.strip()` drops them;
//   - an array past its `.max` answers `too_big` ALONE (zod 4.6.7 `capSize`);
//   - string bounds count code points (`zod_len_*_ok`, zod ≥ 4.5.4).

use crate::api::zod_issues::{key, zod_iso_datetime_ok, zod_issue_aborts, zod_uuid_ok, ZodIssue};
use crate::jsstr::{zod_len_max_ok, zod_len_min_ok};
use serde_json::json;

#[derive(Debug, Clone)]
enum ZKind {
    Str {
        min: Option<(usize, Option<&'static str>)>,
        max: Option<usize>,
    },
    Uuid,
    Bool,
    Int {
        min: i64,
        max: i64,
    },
    Enum(&'static [&'static str]),
    Record,
    Timestamp,
    /// `InventoryFolderPathSchema` — `z.string().max(900).refine(valid)`.
    FolderPath,
    Array {
        elem: Box<ZKind>,
        min: Option<(usize, Option<&'static str>)>,
        max: Option<usize>,
    },
    /// A nested strip object.
    Object(Vec<ZField>),
}

#[derive(Debug, Clone)]
struct ZField {
    key: &'static str,
    kind: ZKind,
    optional: bool,
    nullable: bool,
    default: Option<Value>,
}

fn f(key: &'static str, kind: ZKind) -> ZField {
    ZField {
        key,
        kind,
        optional: false,
        nullable: false,
        default: None,
    }
}

impl ZField {
    fn opt(mut self) -> Self {
        self.optional = true;
        self
    }
    fn null(mut self) -> Self {
        self.nullable = true;
        self
    }
    fn def(mut self, v: Value) -> Self {
        self.default = Some(v);
        self
    }
    /// `patchSchemaOf`'s per-field rewrite: the default stripped, `.optional()`.
    fn patched(mut self) -> Self {
        self.default = None;
        self.optional = true;
        self
    }
}

fn s_max(max: usize) -> ZKind {
    ZKind::Str {
        min: None,
        max: Some(max),
    }
}

fn s_min_max(min: usize, msg: Option<&'static str>, max: Option<usize>) -> ZKind {
    ZKind::Str {
        min: Some((min, msg)),
        max,
    }
}

fn path_with(path: &[Value], k: Value) -> Vec<Value> {
    let mut p = path.to_vec();
    p.push(k);
    p
}

fn too_small_array(min: usize, msg: Option<&'static str>, path: Vec<Value>) -> ZodIssue {
    ZodIssue::TooSmall {
        origin: "array",
        code: "too_small",
        minimum: json!(min),
        inclusive: true,
        path,
        message: msg
            .map(str::to_string)
            .unwrap_or_else(|| format!("Too small: expected array to have >={min} items")),
    }
}

/// Run one kind over a present-or-required value; answers the output value
/// (`None` when the type gate failed) and pushes the issues.
fn run_kind(
    kind: &ZKind,
    v: Option<&Value>,
    path: &[Value],
    issues: &mut Vec<ZodIssue>,
) -> Option<Value> {
    match kind {
        ZKind::Str { min, max } => {
            let Some(Value::String(s)) = v else {
                issues.push(ZodIssue::invalid_type("string", path.to_vec(), v));
                return None;
            };
            if let Some((n, msg)) = min {
                if !zod_len_min_ok(s, *n) {
                    issues.push(match msg {
                        Some(m) => ZodIssue::too_small_string_message(json!(n), path.to_vec(), *m),
                        None => ZodIssue::too_small_string(json!(n), path.to_vec()),
                    });
                }
            }
            if let Some(n) = max {
                if !zod_len_max_ok(s, *n) {
                    issues.push(ZodIssue::too_big_string(json!(n), path.to_vec()));
                }
            }
            Some(Value::String(s.clone()))
        }
        ZKind::Uuid => {
            let Some(Value::String(s)) = v else {
                issues.push(ZodIssue::invalid_type("string", path.to_vec(), v));
                return None;
            };
            if !zod_uuid_ok(s) {
                issues.push(ZodIssue::invalid_uuid(path.to_vec()));
            }
            Some(Value::String(s.clone()))
        }
        ZKind::Bool => match v {
            Some(Value::Bool(b)) => Some(Value::Bool(*b)),
            _ => {
                issues.push(ZodIssue::invalid_type("boolean", path.to_vec(), v));
                None
            }
        },
        ZKind::Int { min, max } => {
            let Some(Value::Number(n)) = v else {
                issues.push(ZodIssue::invalid_type("number", path.to_vec(), v));
                return None;
            };
            let x = n.as_f64().unwrap_or(f64::NAN);
            if x.fract() != 0.0 {
                issues.push(ZodIssue::invalid_int_type(path.to_vec(), v));
                return None;
            }
            // The safe-integer bounds are CHECKS (they continue): zod reports
            // the bound and then the schema's own `.min` / `.max` (measured).
            if x > crate::api::zod_issues::MAX_SAFE_INTEGER {
                issues.push(ZodIssue::too_big_int(path.to_vec()));
            } else if x < -crate::api::zod_issues::MAX_SAFE_INTEGER {
                issues.push(ZodIssue::too_small_int(path.to_vec()));
            }
            if x < *min as f64 {
                issues.push(ZodIssue::too_small_number(json!(min), path.to_vec()));
            }
            if x > *max as f64 {
                issues.push(ZodIssue::too_big_number(json!(max), path.to_vec()));
            }
            Some(Value::Number(n.clone()))
        }
        ZKind::Enum(values) => match v {
            Some(Value::String(s)) if values.contains(&s.as_str()) => {
                Some(Value::String(s.clone()))
            }
            _ => {
                issues.push(ZodIssue::invalid_value(values, path.to_vec()));
                None
            }
        },
        ZKind::Record => match v {
            Some(Value::Object(o)) => Some(Value::Object(o.clone())),
            _ => {
                issues.push(ZodIssue::invalid_type("record", path.to_vec(), v));
                None
            }
        },
        ZKind::Timestamp => match v {
            Some(Value::String(s)) => {
                if !zod_iso_datetime_ok(s) {
                    issues.push(ZodIssue::invalid_datetime(path.to_vec()));
                }
                Some(Value::String(s.clone()))
            }
            got => {
                issues.push(ZodIssue::invalid_union(
                    vec![
                        vec![ZodIssue::invalid_type("string", vec![], got)],
                        vec![ZodIssue::invalid_type("date", vec![], got)],
                    ],
                    path.to_vec(),
                ));
                None
            }
        },
        ZKind::FolderPath => {
            let Some(Value::String(s)) = v else {
                issues.push(ZodIssue::invalid_type("string", path.to_vec(), v));
                return None;
            };
            if !zod_len_max_ok(s, 900) {
                issues.push(ZodIssue::too_big_string(json!(900), path.to_vec()));
            }
            if !super::folders::is_valid_inventory_folder_path(s) {
                issues.push(ZodIssue::custom(
                    path.to_vec(),
                    super::folders::INVENTORY_FOLDER_SCHEMA_MESSAGE,
                ));
            }
            Some(Value::String(s.clone()))
        }
        ZKind::Array { elem, min, max } => {
            let Some(Value::Array(a)) = v else {
                issues.push(ZodIssue::invalid_type("array", path.to_vec(), v));
                return None;
            };
            if let Some(n) = max {
                if a.len() > *n {
                    issues.push(ZodIssue::too_big_array(json!(n), path.to_vec()));
                    return None;
                }
            }
            let mut out = Vec::with_capacity(a.len());
            for (i, e) in a.iter().enumerate() {
                if let Some(x) = run_kind(elem, Some(e), &path_with(path, json!(i)), issues) {
                    out.push(x);
                }
            }
            if let Some((n, msg)) = min {
                if a.len() < *n {
                    issues.push(too_small_array(*n, *msg, path.to_vec()));
                }
            }
            Some(Value::Array(out))
        }
        ZKind::Object(fields) => match v {
            Some(Value::Object(o)) => {
                Some(Value::Object(parse_fields(fields, false, o, path, issues)))
            }
            _ => {
                issues.push(ZodIssue::invalid_type("object", path.to_vec(), v));
                None
            }
        },
    }
}

/// Parse an object's fields in shape order (+ `.strict()`'s tail).
fn parse_fields(
    fields: &[ZField],
    strict: bool,
    input: &Map<String, Value>,
    path: &[Value],
    issues: &mut Vec<ZodIssue>,
) -> Map<String, Value> {
    let mut out = Map::new();
    for field in fields {
        let v = input.get(field.key);
        let fpath = path_with(path, key(field.key));
        match v {
            None if field.default.is_some() => {
                out.insert(field.key.to_string(), field.default.clone().unwrap());
            }
            None if field.optional => {}
            Some(Value::Null) if field.nullable => {
                out.insert(field.key.to_string(), Value::Null);
            }
            _ => {
                if let Some(x) = run_kind(&field.kind, v, &fpath, issues) {
                    out.insert(field.key.to_string(), x);
                }
            }
        }
    }
    if strict {
        let unknown: Vec<String> = input
            .keys()
            .filter(|k| !fields.iter().any(|f| f.key == k.as_str()))
            .cloned()
            .collect();
        if !unknown.is_empty() {
            issues.push(ZodIssue::unrecognized_keys(unknown, path.to_vec()));
        }
    }
    out
}

/// A top-level object parse: `Ok(data)` or v4's issue list.
fn parse_object(
    fields: &[ZField],
    strict: bool,
    input: &Value,
) -> Result<Map<String, Value>, Vec<ZodIssue>> {
    let mut issues = Vec::new();
    let Value::Object(o) = input else {
        return Err(vec![ZodIssue::invalid_type("object", vec![], Some(input))]);
    };
    let out = parse_fields(fields, strict, o, &[], &mut issues);
    if issues.is_empty() {
        Ok(out)
    } else {
        Err(issues)
    }
}

const KIND_VALUES: &[&str] = &["item", "kit"];
const SCOPE_VALUES: &[&str] = &["character", "group", "project", "general"];

/// v4 `InventoryItemSchema.shape` in order.
fn item_fields() -> Vec<ZField> {
    vec![
        f("id", ZKind::Uuid),
        f("kind", ZKind::Enum(KIND_VALUES)).def(json!("item")),
        f("name", s_min_max(1, None, Some(200))),
        f("title", s_max(200)).null().opt(),
        f("description", s_max(4000)).null().opt(),
        f("imageDescription", s_max(2000)).null().opt(),
        f("equippedByDefault", ZKind::Bool).def(json!(false)),
        f("maxEquipped", ZKind::Int { min: 1, max: 999 }).def(json!(1)),
        f("canCopy", ZKind::Bool).def(json!(false)),
        f("isArchetype", ZKind::Bool).def(json!(false)),
        f("parentId", ZKind::Uuid).null().def(Value::Null),
        f(
            "itemIds",
            ZKind::Array {
                elem: Box::new(ZKind::Uuid),
                min: None,
                max: Some(INVENTORY_KIT_MAX_MEMBERS),
            },
        )
        .def(json!([])),
        f("emoji", s_max(16)).null().opt(),
        f("svg", s_max(INVENTORY_SVG_MAX_BYTES)).null().opt(),
        f("imageFileId", ZKind::Uuid).null().opt(),
        f("metadata", ZKind::Record).def(json!({})),
        f("folder", ZKind::FolderPath).def(json!("")),
        f("archivedAt", ZKind::Timestamp).null().opt(),
        f("createdAt", ZKind::Timestamp),
        f("updatedAt", ZKind::Timestamp),
    ]
}

fn without(fields: Vec<ZField>, drop: &[&str]) -> Vec<ZField> {
    fields
        .into_iter()
        .filter(|f| !drop.contains(&f.key))
        .collect()
}

/// v4 `inventoryItemFieldsSchema` — the editable field set.
fn editable_fields() -> Vec<ZField> {
    let mut fields = without(
        item_fields(),
        &[
            "id",
            "createdAt",
            "updatedAt",
            "archivedAt",
            "imageFileId",
            "folder",
        ],
    );
    fields.push(f("folder", s_max(900)).opt());
    fields
}

/// v4 `InventoryItemSchema.safeParse` — the full object (strict).
pub fn zod_inventory_item(input: &Value) -> Result<Map<String, Value>, Vec<ZodIssue>> {
    parse_object(&item_fields(), true, input)
}

/// v4 `InventoryItemSchema.omit({folder})` — a standalone / archetype / kit
/// FILE (the reader's non-instance parse; strict, defaults filled).
pub fn zod_inventory_full_file(input: &Value) -> Result<Map<String, Value>, Vec<ZodIssue>> {
    parse_object(&without(item_fields(), &["folder"]), true, input)
}

/// v4 `InventoryItemFileSchema` — the stored file, sparse (every field
/// optional, nothing defaulted, `id` and the stamps required; strip).
pub fn zod_inventory_item_file(input: &Value) -> Result<Map<String, Value>, Vec<ZodIssue>> {
    let fields: Vec<ZField> = without(item_fields(), &["folder"])
        .into_iter()
        .map(|field| match field.key {
            "id" | "createdAt" | "updatedAt" => field,
            _ => field.patched(),
        })
        .collect();
    parse_object(&fields, false, input)
}

/// v4 `createInventorySchema` — `POST …/inventory`.
pub fn parse_create_inventory_body(input: &Value) -> Result<Map<String, Value>, Vec<ZodIssue>> {
    let mut issues = Vec::new();
    let Value::Object(o) = input else {
        return Err(vec![ZodIssue::invalid_type("object", vec![], Some(input))]);
    };
    let body = parse_fields(&editable_fields(), false, o, &[], &mut issues);
    if !issues.iter().any(zod_issue_aborts) {
        for message in inventory_shape_issues(&body) {
            issues.push(ZodIssue::custom_added(message, vec![]));
        }
        if js_truthy(body.get("parentId")) {
            issues.push(ZodIssue::custom_added(
                "Create an instance with ?action=instantiate",
                vec![key("parentId")],
            ));
        }
    }
    if issues.is_empty() {
        Ok(body)
    } else {
        Err(issues)
    }
}

/// v4 `updateInventorySchema` — `PUT …/inventory/[itemId]`: any editable
/// field, nothing defaulted, `parentId` never, plus `archived` and
/// `imageFileId`.
pub fn parse_update_inventory_body(input: &Value) -> Result<Map<String, Value>, Vec<ZodIssue>> {
    let mut fields: Vec<ZField> = without(editable_fields(), &["parentId"])
        .into_iter()
        .map(ZField::patched)
        .collect();
    fields.push(f("archived", ZKind::Bool).opt());
    fields.push(f("imageFileId", ZKind::Uuid).null().opt());
    parse_object(&fields, false, input)
}

/// v4 `clearInventoryOverrideSchema` — `?action=clear-override`.
pub fn parse_clear_inventory_override_body(
    input: &Value,
) -> Result<Map<String, Value>, Vec<ZodIssue>> {
    parse_object(
        &[f(
            "fields",
            ZKind::Array {
                elem: Box::new(ZKind::Enum(&INVENTORY_INHERITABLE_FIELDS)),
                min: Some((1, None)),
                max: None,
            },
        )],
        false,
        input,
    )
}

/// v4 `instantiateInventorySchema` — `?action=instantiate`. `overrides`
/// picks ten inheritable keys (NO `imageFileId`).
pub fn parse_instantiate_inventory_body(
    input: &Value,
) -> Result<Map<String, Value>, Vec<ZodIssue>> {
    let pick = [
        "name",
        "title",
        "description",
        "imageDescription",
        "equippedByDefault",
        "maxEquipped",
        "canCopy",
        "emoji",
        "svg",
        "metadata",
    ];
    let overrides: Vec<ZField> = item_fields()
        .into_iter()
        .filter(|field| pick.contains(&field.key))
        .map(ZField::patched)
        .collect();
    parse_object(
        &[
            f("archetypeId", ZKind::Uuid),
            f("folder", s_max(900)).opt(),
            f(
                "projectId",
                ZKind::Str {
                    min: None,
                    max: None,
                },
            )
            .null()
            .opt(),
            f(
                "chatId",
                ZKind::Str {
                    min: None,
                    max: None,
                },
            )
            .null()
            .opt(),
            f("overrides", ZKind::Object(overrides)).opt(),
        ],
        false,
        input,
    )
}

/// v4 `inventoryFolderPathBodySchema` — `?action=create-folder` / `delete-folder`.
pub fn parse_inventory_folder_path_body(
    input: &Value,
) -> Result<Map<String, Value>, Vec<ZodIssue>> {
    parse_object(
        &[f(
            "path",
            s_min_max(1, Some("A folder path is required"), Some(900)),
        )],
        false,
        input,
    )
}

/// v4 `renameInventoryFolderSchema` — `?action=rename-folder`.
pub fn parse_rename_inventory_folder_body(
    input: &Value,
) -> Result<Map<String, Value>, Vec<ZodIssue>> {
    parse_object(
        &[
            f(
                "path",
                s_min_max(1, Some("A folder path is required"), Some(900)),
            ),
            f(
                "name",
                s_min_max(1, Some("A folder name is required"), Some(200)),
            ),
        ],
        false,
        input,
    )
}

/// v4 `fileInventoryItemsSchema` — `?action=move-items` / `copy-items`.
pub fn parse_file_inventory_items_body(input: &Value) -> Result<Map<String, Value>, Vec<ZodIssue>> {
    parse_object(
        &[
            f(
                "itemIds",
                ZKind::Array {
                    elem: Box::new(s_min_max(1, None, None)),
                    min: Some((1, Some("At least one item is required"))),
                    max: Some(500),
                },
            ),
            f("folder", s_max(900)),
        ],
        false,
        input,
    )
}

/// v4 `chatInventoryEquipSchema` — `?action=inventory-equip` / `-unequip`.
pub fn parse_chat_inventory_equip_body(input: &Value) -> Result<Map<String, Value>, Vec<ZodIssue>> {
    parse_object(
        &[
            f("characterId", s_min_max(1, None, None)),
            f(
                "ids",
                ZKind::Array {
                    elem: Box::new(ZKind::Uuid),
                    min: Some((1, None)),
                    max: Some(INVENTORY_KIT_MAX_MEMBERS),
                },
            ),
        ],
        false,
        input,
    )
}

/// v4 `chatInventoryResetSchema` — `?action=inventory-reset-defaults`.
pub fn parse_chat_inventory_reset_body(input: &Value) -> Result<Map<String, Value>, Vec<ZodIssue>> {
    parse_object(&[f("characterId", s_min_max(1, None, None))], false, input)
}

/// v4 `inventoryTransferSchema` — `POST /api/v1/inventory/transfers`.
pub fn parse_inventory_transfer_body(input: &Value) -> Result<Map<String, Value>, Vec<ZodIssue>> {
    let side = || {
        ZKind::Object(vec![
            f("scope", ZKind::Enum(SCOPE_VALUES)),
            f(
                "id",
                ZKind::Str {
                    min: None,
                    max: None,
                },
            )
            .null()
            .opt(),
        ])
    };
    parse_object(
        &[
            f("action", ZKind::Enum(&["move", "copy"])),
            f("itemId", ZKind::Uuid),
            f("from", side()),
            f("to", side()),
        ],
        false,
        input,
    )
}

// ============================================================================
// V8's JSON — the parse an operator-written file goes through, and the
// `JSON.stringify(out, null, 2)` a file is written with
// ============================================================================

/// A canonical array index (`0` … `2^32 - 2`, no leading zero) — the keys V8
/// orders FIRST, ascending, in every object (ES `OrdinaryOwnPropertyKeys`).
fn is_array_index(key: &str) -> bool {
    if key.is_empty() || (key.len() > 1 && key.starts_with('0')) {
        return false;
    }
    key.bytes().all(|b| b.is_ascii_digit()) && key.parse::<u64>().is_ok_and(|n| n < u32::MAX as u64)
}

/// Re-key every object of a parsed value in V8's own-property order: array
/// indices first (ascending), then the rest in insertion order. serde keeps
/// pure insertion order; `JSON.parse` does not.
pub fn v8_key_order(v: Value) -> Value {
    match v {
        Value::Object(o) => {
            let mut indices: Vec<(u64, String, Value)> = Vec::new();
            let mut rest: Vec<(String, Value)> = Vec::new();
            for (k, val) in o {
                let val = v8_key_order(val);
                if is_array_index(&k) {
                    indices.push((k.parse().unwrap_or(0), k, val));
                } else {
                    rest.push((k, val));
                }
            }
            indices.sort_by_key(|(n, _, _)| *n);
            let mut out = Map::new();
            for (_, k, val) in indices {
                out.insert(k, val);
            }
            for (k, val) in rest {
                out.insert(k, val);
            }
            Value::Object(out)
        }
        Value::Array(a) => Value::Array(a.into_iter().map(v8_key_order).collect()),
        other => other,
    }
}

/// V8 `JSON.parse(text)`: serde's parse (a duplicate key keeps the LAST value
/// at its FIRST position, as V8 does; `__proto__` is a plain key), every
/// object re-keyed in V8's order, and on failure V8's own message
/// ([`crate::jsstr::v8_json_parse_message`]; serde's where V8 would accept —
/// a recorded divergence).
pub fn js_json_parse(text: &str) -> Result<Value, String> {
    match serde_json::from_str::<Value>(text) {
        Ok(v) => Ok(v8_key_order(v)),
        Err(e) => Err(crate::jsstr::v8_json_parse_message(text).unwrap_or_else(|| e.to_string())),
    }
}

fn push_indent(out: &mut String, depth: usize) {
    out.push('\n');
    for _ in 0..depth {
        out.push_str("  ");
    }
}

fn write_pretty(v: &Value, depth: usize, out: &mut String) {
    match v {
        Value::Null => out.push_str("null"),
        Value::Bool(b) => out.push_str(if *b { "true" } else { "false" }),
        Value::Number(n) => match n.as_f64() {
            Some(f) if f.is_finite() => out.push_str(&crate::pascal::js_value::number_to_string(f)),
            Some(_) => out.push_str("null"),
            None => out.push_str(&n.to_string()),
        },
        Value::String(s) => out.push_str(&Value::String(s.clone()).to_string()),
        Value::Array(a) => {
            if a.is_empty() {
                out.push_str("[]");
                return;
            }
            out.push('[');
            for (i, e) in a.iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                push_indent(out, depth + 1);
                write_pretty(e, depth + 1, out);
            }
            push_indent(out, depth);
            out.push(']');
        }
        Value::Object(o) => {
            if o.is_empty() {
                out.push_str("{}");
                return;
            }
            // A JS object enumerates its array-index keys first, ascending,
            // whatever order they were set in.
            let mut entries: Vec<(&String, &Value)> = o.iter().collect();
            entries.sort_by_key(|(k, _)| {
                if is_array_index(k) {
                    (0u8, k.parse::<u64>().unwrap_or(0))
                } else {
                    (1u8, 0)
                }
            });
            out.push('{');
            for (i, (k, e)) in entries.into_iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                push_indent(out, depth + 1);
                out.push_str(&Value::String(k.clone()).to_string());
                out.push_str(": ");
                write_pretty(e, depth + 1, out);
            }
            push_indent(out, depth);
            out.push('}');
        }
    }
}

/// V8 `JSON.stringify(value, null, 2)`: two-space indent, `": "`, empty
/// containers as `[]` / `{}`, JS number formatting (`1.0` is `1`, `1e21` is
/// `1e+21`), keys in the map's order.
pub fn js_json_stringify_pretty(v: &Value) -> String {
    let mut out = String::new();
    write_pretty(v, 0, &mut out);
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn shape(v: Value) -> Vec<String> {
        inventory_shape_issues(v.as_object().unwrap())
    }

    #[test]
    fn shape_issue_order() {
        assert_eq!(
            shape(json!({"isArchetype": true, "parentId": "p", "kind": "kit"})),
            vec![
                "An archetype cannot derive from another archetype",
                "A kit cannot be an archetype",
                "A kit cannot be an instance",
                "A kit needs at least one member",
            ]
        );
        assert_eq!(
            shape(json!({"itemIds": ["a"]})),
            vec!["Only a kit names members"]
        );
        assert!(shape(json!({"kind": "kit", "itemIds": ["a"]})).is_empty());
        assert!(shape(json!({})).is_empty());
    }

    #[test]
    fn record_set_keeps_position_and_remove_reappends() {
        let mut r = InventoryRecord::new(
            json!({"id": "i", "title": "t", "name": "n"})
                .as_object()
                .unwrap()
                .clone(),
        );
        r.set("title", json!("u"));
        r.remove("id");
        r.set("id", json!("i"));
        assert_eq!(
            serde_json::to_string(&r.to_value()).unwrap(),
            r#"{"title":"u","name":"n","id":"i"}"#
        );
    }
}
