//! What every `inventory_*` tool handler shares (v4 `lib/tools/handlers/
//! inventory-handler-shared.ts`, `e1872c94f` + `102a23648`; P4.D328): the
//! object summary every mutation returns, the tier / kind / state words, the
//! refusals for an object that resolved to nothing, a shared object, and a
//! closed project shelf, and the ONE `[InventoryTools]` line per call.
//!
//! Every reference resolves through `inventory::refs::resolve_inventory_ref`;
//! nothing here re-derives id-or-title-or-name.

use serde::Serialize;
use serde_json::{Map, Value};

use crate::inventory::context::LoadedInventoryContext;
use crate::inventory::errors::{InventoryError, InventoryRefusal};
use crate::inventory::refs::{
    resolve_inventory_ref, InventoryRefMatch, InventoryState, ResolveInventoryRefOptions,
    ResolvedInventoryPool,
};
use crate::inventory::resolve::{inventory_display_name, inventory_label};
use crate::inventory::types::{InventoryFields, InventoryScope, ResolvedInventoryItem};
use crate::jsstr::js_trim;

/// Where an object lives, from the caller's side (v4 `InventoryToolTier`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum InventoryToolTier {
    Own,
    Group,
    Project,
    General,
}

impl InventoryToolTier {
    pub fn as_str(self) -> &'static str {
        match self {
            InventoryToolTier::Own => "own",
            InventoryToolTier::Group => "group",
            InventoryToolTier::Project => "project",
            InventoryToolTier::General => "general",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        Some(match s {
            "own" => InventoryToolTier::Own,
            "group" => InventoryToolTier::Group,
            "project" => InventoryToolTier::Project,
            "general" => InventoryToolTier::General,
            _ => return None,
        })
    }
}

/// An object's kind as the tools name it (archetype > kit > item).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum InventoryObjectKind {
    Item,
    Kit,
    Archetype,
}

impl InventoryObjectKind {
    pub fn as_str(self) -> &'static str {
        match self {
            InventoryObjectKind::Item => "item",
            InventoryObjectKind::Kit => "kit",
            InventoryObjectKind::Archetype => "archetype",
        }
    }
}

/// An object as every mutation reports it, so the model can chain calls
/// without a list (v4 `InventoryObjectSummary` — the six keys in this order;
/// `title` / `state` serialise `null`).
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct InventoryObjectSummary {
    pub id: String,
    pub name: String,
    pub title: Option<String>,
    /// `None` for an archetype.
    pub state: Option<InventoryState>,
    pub tier: InventoryToolTier,
    pub kind: InventoryObjectKind,
}

/// v4 `tierOf`: own → `own`; an origin scope of `group` / `general` as is;
/// ANYTHING else — a project origin, or no origin at all — reads `project`.
pub fn tier_of(item: &ResolvedInventoryItem, own: bool) -> InventoryToolTier {
    if own {
        return InventoryToolTier::Own;
    }
    match item.origin().map(|o| o.scope) {
        Some(InventoryScope::Group) => InventoryToolTier::Group,
        Some(InventoryScope::General) => InventoryToolTier::General,
        _ => InventoryToolTier::Project,
    }
}

/// v4 `kindOf`.
pub fn kind_of(item: &ResolvedInventoryItem) -> InventoryObjectKind {
    if item.is_archetype() {
        InventoryObjectKind::Archetype
    } else if item.is_kit() {
        InventoryObjectKind::Kit
    } else {
        InventoryObjectKind::Item
    }
}

/// An object's state from what the caller knows (v4 `stateFor`): an
/// archetype has none; equipped; own → carried; else reachable.
pub fn state_for(
    item: &ResolvedInventoryItem,
    own: bool,
    equipped: bool,
) -> Option<InventoryState> {
    if item.is_archetype() {
        return None;
    }
    Some(if equipped {
        InventoryState::Equipped
    } else if own {
        InventoryState::Carried
    } else {
        InventoryState::Reachable
    })
}

/// v4 `item.title?.trim() || null`.
pub fn trimmed_title(item: &impl InventoryFields) -> Option<String> {
    item.title()
        .map(js_trim)
        .filter(|t| !t.is_empty())
        .map(str::to_string)
}

/// v4 `summarize`.
pub fn summarize(
    item: &ResolvedInventoryItem,
    own: bool,
    equipped: bool,
) -> InventoryObjectSummary {
    InventoryObjectSummary {
        id: item.id().to_string(),
        name: item.name().to_string(),
        title: trimmed_title(item),
        state: state_for(item, own, equipped),
        tier: tier_of(item, own),
        kind: kind_of(item),
    }
}

/// v4 `summarizeMatch`.
pub fn summarize_match(m: &InventoryRefMatch) -> InventoryObjectSummary {
    summarize(&m.item, m.own, m.equipped)
}

/// A bare `{ name, title }` for the display-name rule.
struct NameTitle(Map<String, Value>);

impl InventoryFields for NameTitle {
    fn fields(&self) -> &Map<String, Value> {
        &self.0
    }
}

/// v4 `inventoryDisplayName({ name, title })` over two strings.
pub fn display_name_of(name: &str, title: Option<&str>) -> String {
    let mut m = Map::new();
    m.insert("name".into(), Value::String(name.to_string()));
    m.insert(
        "title".into(),
        title.map_or(Value::Null, |t| Value::String(t.to_string())),
    );
    inventory_display_name(&NameTitle(m))
}

/// One line naming an object for the model (v4 `formatSummary`):
/// `Excalibur (sword) [id] — equipped, own`.
pub fn format_summary(summary: &InventoryObjectSummary) -> String {
    let label = display_name_of(&summary.name, summary.title.as_deref());
    let state = summary.state.map_or("archetype", InventoryState::as_str);
    format!(
        "{label} [{}] — {state}, {}",
        summary.id,
        summary.tier.as_str()
    )
}

/// Resolve a ref over the call's context (v4 `resolveToolRef`).
pub fn resolve_tool_ref(
    ctx: &LoadedInventoryContext,
    reference: &str,
    opts: ResolveInventoryRefOptions,
) -> Vec<InventoryRefMatch> {
    ctx.resolve(reference, opts)
}

/// When a ref names an object on a project shelf the roster has closed to
/// this character, Pascal's refusal saying so (C1's `OFF_ROSTER` sentence);
/// `None` otherwise (v4 `closedShelfMessage`, ⟨102a⟩). `archetypes_only` asks
/// only after archetypes (a pick-up). ONLY `inventory_read`, `inventory_equip`
/// and `inventory_pick_up` consult it (R-16).
pub fn closed_shelf_message(
    ctx: &LoadedInventoryContext,
    reference: &str,
    archetypes_only: bool,
) -> Option<String> {
    if ctx.closed_shelf.is_empty() {
        return None;
    }
    let shelf = ResolvedInventoryPool {
        items: ctx.closed_shelf.clone(),
        owns: Box::new(|_| false),
    };
    let first = resolve_inventory_ref(
        reference,
        &shelf,
        &ctx.equipped_ids,
        ResolveInventoryRefOptions {
            include_archetypes: true,
            include_archived: false,
        },
    )
    .into_iter()
    .find(|m| !archetypes_only || m.item.is_archetype())?;
    Some(InventoryRefusal::off_roster(&[inventory_label(&first.item)]).message)
}

/// The refusal for an object that resolved to nothing (v4
/// `inventoryNotFoundMessage`).
pub fn inventory_not_found_message(reference: &str) -> String {
    format!(
        "No object \"{reference}\" is in your inventory or within reach. Use inventory_list to see what there is."
    )
}

/// The refusal for a change to an object in a shared store (v4
/// `sharedInventoryItemReadOnlyMessage`); `verb` ∈ `changed` / `archived` /
/// `given`.
pub fn shared_inventory_item_read_only_message(item: &ResolvedInventoryItem, verb: &str) -> String {
    format!(
        "\"{}\" lives in a shared store — you can carry it but not edit, archive or give it. Only objects in your own vault can be {verb}.",
        inventory_label(item)
    )
}

/// A refusal's own words, or the error's text (v4 `errorMessageOf` — every v5
/// error carries a message, so the fallback never shows; a storage error
/// renders its BARE text, §R.5).
pub fn error_message_of(error: &InventoryError) -> String {
    match error {
        InventoryError::Refusal(r) => r.message.clone(),
        InventoryError::Db(e) => crate::db::fallback::error_text(e),
        other => other.to_string(),
    }
}

// ============================================================================
// The one `[InventoryTools]` line per call
// ============================================================================

/// The line's level: refusals and invalid input at `info`, successes at
/// `debug`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InventoryToolLogLevel {
    Debug,
    Info,
}

/// The data of v4's `logInventoryTool(tool, context, data, level)` — every
/// key a handler spells, in ONE order that is a superset of every handler's
/// own (measured: each call's keys appear in this order), so one tracing call
/// renders them; an absent key is v4's absent (or `undefined`) key. Arrays
/// and objects ride the `…Json` file-layer convention.
#[derive(Clone, Debug, Default)]
pub struct InventoryToolLog {
    pub refs: Option<Vec<String>>,
    pub resolved: Option<Vec<String>>,
    pub to: Option<String>,
    pub outcome: &'static str,
    pub count: Option<usize>,
    pub filters: Option<Value>,
    pub skipped: Option<Value>,
    pub equipped_ids: Option<Vec<String>>,
    pub created_ids: Option<Vec<String>>,
    pub unequipped: Option<Vec<String>>,
    pub fields: Option<Vec<String>>,
    /// `inventory_pick_up`'s `created: created?.id`.
    pub created: Option<String>,
    /// `inventory_pick_up` / `inventory_create`'s boolean `equipped`.
    pub equipped: Option<bool>,
    pub archived: Option<bool>,
    pub error: Option<String>,
}

impl InventoryToolLog {
    pub fn outcome(outcome: &'static str) -> Self {
        InventoryToolLog {
            outcome,
            ..Default::default()
        }
    }
}

fn json_of<T: Serialize>(v: &Option<T>) -> Option<String> {
    v.as_ref()
        .map(|v| serde_json::to_string(v).unwrap_or_default())
}

/// Emit v4's `[InventoryTools] <tool>` line: `context = "inventory-tools",
/// chatId, characterId, tool`, then the data keys.
pub fn log_inventory_tool(
    tool: &str,
    chat_id: &str,
    character_id: &str,
    data: &InventoryToolLog,
    level: InventoryToolLogLevel,
) {
    let refs = json_of(&data.refs);
    let resolved = json_of(&data.resolved);
    let filters = json_of(&data.filters);
    let skipped = json_of(&data.skipped);
    let equipped_ids = json_of(&data.equipped_ids);
    let created_ids = json_of(&data.created_ids);
    let unequipped = json_of(&data.unequipped);
    let fields = json_of(&data.fields);
    macro_rules! line {
        ($mac:ident) => {
            tracing::$mac!(
                target: "quilltap::inventory_tools",
                context = "inventory-tools",
                chatId = chat_id,
                characterId = character_id,
                tool = tool,
                refsJson = refs.as_deref(),
                resolvedJson = resolved.as_deref(),
                to = data.to.as_deref(),
                outcome = data.outcome,
                count = data.count,
                filtersJson = filters.as_deref(),
                skippedJson = skipped.as_deref(),
                equippedJson = equipped_ids.as_deref(),
                createdJson = created_ids.as_deref(),
                unequippedJson = unequipped.as_deref(),
                fieldsJson = fields.as_deref(),
                created = data.created.as_deref(),
                equipped = data.equipped,
                archived = data.archived,
                error = data.error.as_deref(),
                "[InventoryTools] {tool}"
            )
        };
    }
    match level {
        InventoryToolLogLevel::Debug => line!(debug),
        InventoryToolLogLevel::Info => line!(info),
    }
}

// ============================================================================
// The inputs (v4's `validate*Input` — `schema.safeParse(input)` → data or
// null). Zod's object strips unknown keys; an `.optional()` key admits
// `undefined` only (a `null` fails); string lengths are Zod ≥ 4.5's
// code-point-aware checks (`jsstr::zod_len_*`).
// ============================================================================

/// The input failed v4's schema (`validate*Input` answered null).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct InvalidInput;

/// One optional input key: absent → `Ok(None)`, a value the schema refuses →
/// `Err(InvalidInput)`.
pub type Parsed<T> = Result<Option<T>, InvalidInput>;

/// The input object (v4 `z.object` — a non-object fails).
pub fn input_object(input: &Value) -> Option<&Map<String, Value>> {
    input.as_object()
}

/// A string with Zod `.min(min)` / `.max(max)` (`None` = no bound).
pub fn opt_string(
    o: &Map<String, Value>,
    key: &str,
    min: Option<usize>,
    max: Option<usize>,
) -> Parsed<String> {
    match o.get(key) {
        None => Ok(None),
        Some(Value::String(s)) => {
            if min.is_some_and(|m| !crate::jsstr::zod_len_min_ok(s, m))
                || max.is_some_and(|m| !crate::jsstr::zod_len_max_ok(s, m))
            {
                Err(InvalidInput)
            } else {
                Ok(Some(s.clone()))
            }
        }
        Some(_) => Err(InvalidInput),
    }
}

/// A required string (`.min(min).max(max)`).
pub fn req_string(
    o: &Map<String, Value>,
    key: &str,
    min: Option<usize>,
    max: Option<usize>,
) -> Result<String, InvalidInput> {
    opt_string(o, key, min, max)?.ok_or(InvalidInput)
}

/// v4 `InventoryRefInputSchema` — `z.string().min(1).max(200)`.
pub fn req_ref(o: &Map<String, Value>, key: &str) -> Result<String, InvalidInput> {
    req_string(o, key, Some(1), Some(200))
}

/// v4 `InventoryRefsInputSchema` — one to eight refs.
pub fn req_refs(o: &Map<String, Value>, key: &str) -> Result<Vec<String>, InvalidInput> {
    let Some(Value::Array(items)) = o.get(key) else {
        return Err(InvalidInput);
    };
    if items.is_empty() || items.len() > 8 {
        return Err(InvalidInput);
    }
    items
        .iter()
        .map(|v| match v {
            Value::String(s)
                if crate::jsstr::zod_len_min_ok(s, 1) && crate::jsstr::zod_len_max_ok(s, 200) =>
            {
                Ok(s.clone())
            }
            _ => Err(InvalidInput),
        })
        .collect()
}

/// `z.boolean().optional()`.
pub fn opt_bool(o: &Map<String, Value>, key: &str) -> Parsed<bool> {
    match o.get(key) {
        None => Ok(None),
        Some(Value::Bool(b)) => Ok(Some(*b)),
        Some(_) => Err(InvalidInput),
    }
}

/// `z.number().int().min(1).max(999).optional()` (v4 `max_equipped`).
pub fn opt_max_equipped(o: &Map<String, Value>, key: &str) -> Parsed<i64> {
    match o.get(key) {
        None => Ok(None),
        Some(Value::Number(n)) => {
            let f = n.as_f64().ok_or(InvalidInput)?;
            if f.fract() == 0.0 && (1.0..=999.0).contains(&f) {
                Ok(Some(f as i64))
            } else {
                Err(InvalidInput)
            }
        }
        Some(_) => Err(InvalidInput),
    }
}

/// `z.record(z.string(), z.union([z.string(), z.number(), z.boolean(),
/// z.null()])).optional()` — a nested object or array fails the input.
pub fn opt_metadata(o: &Map<String, Value>, key: &str) -> Parsed<Map<String, Value>> {
    match o.get(key) {
        None => Ok(None),
        Some(Value::Object(m)) => {
            if m.values()
                .all(|v| !matches!(v, Value::Object(_) | Value::Array(_)))
            {
                Ok(Some(m.clone()))
            } else {
                Err(InvalidInput)
            }
        }
        Some(_) => Err(InvalidInput),
    }
}

/// v4 `inventoryObjectFieldsShape` — the five optional object fields
/// `inventory_create` and `inventory_update` share, in shape order.
#[derive(Clone, Debug, Default)]
pub struct InventoryObjectFields {
    pub title: Option<String>,
    pub description: Option<String>,
    pub image_description: Option<String>,
    pub emoji: Option<String>,
    pub max_equipped: Option<i64>,
    pub metadata: Option<Map<String, Value>>,
}

pub fn object_fields(o: &Map<String, Value>) -> Result<InventoryObjectFields, InvalidInput> {
    Ok(InventoryObjectFields {
        title: opt_string(o, "title", None, Some(200))?,
        description: opt_string(o, "description", None, Some(4000))?,
        image_description: opt_string(o, "image_description", None, Some(2000))?,
        emoji: opt_string(o, "emoji", None, Some(16))?,
        max_equipped: opt_max_equipped(o, "max_equipped")?,
        metadata: opt_metadata(o, "metadata")?,
    })
}
