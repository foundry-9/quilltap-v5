//! The Inventory title lint — a title may not shadow a kind name or a live
//! gate key, and a title that stops answering rings a bell (v4
//! `lib/inventory/title-lint.ts`, `7263105c6` at `322a5488e`; P4.D340).
//!
//! `resolve_inventory_ref` matches id → title → name and stops at the first
//! round that matches anything. A title equal to a string some gate or
//! reference means by NAME wins the title round, the name round never runs,
//! and the reference quietly lands on the wrong object — or a gated tool goes
//! dark — with no error anywhere.
//!
//! ## The event, not the field
//!
//! The lint is handed the object as it was before a write and as it will be
//! after (either may be `None`), and asks one question: **does the set of
//! reachable titled objects change?** A title enters when the object becomes
//! reachable under a title it did not answer to before — a create, a rename, a
//! restore from the archive, a move into `Inventory/` — or when its name
//! changes under a standing title (its own name is a kind name). A title
//! leaves when the object stops answering to it — a rename, an archive, a
//! delete, a move out. Anything else (a description, a folder) is neither.
//!
//! ## Entering: two protected sets, fail closed
//!
//! After [`normalize_title`], an entering title is checked against:
//!
//!   1. **Kind names** — the resolved `name` of every object the writing
//!      location's pool reaches, the object's own included. Flat reject
//!      (`kind-name shadow`).
//!   2. **Gate keys in use** — every literal `inventory` / `inventoryCount`
//!      key in any tool's `availableWhen` / `withheldWhen`
//!      (`pascal::tool_gate_keys`), resolved against the pool with the object
//!      left out: landing on some other object is `live gate shadow`
//!      (reject); landing on nothing means the object is the gate's target
//!      (`gate key naming`, pass, one row).
//!
//! If the pool or any tool file cannot be read, an entering title is refused,
//! naming what could not be read: "could not look" is not "nothing to collide
//! with".
//!
//! ## Leaving: a bell, never a refusal
//!
//! A title that leaves while a gate keys on it gets a row, alert on: `gate
//! dark` (the key now resolves to nothing), `gate redirected: <owner>` (to some
//! other object), or `gate unverifiable: <what>` (a mount or tool file could
//! not be read, so which emergency it is cannot be told). `gate key moved`
//! (still lands on this object by name) is not an alert.
//!
//! ## Evidence
//!
//! Every invocation writes one `debug` census line naming the pools and tool
//! stores the protected set was built from and their sizes — so "the lint
//! looked and passed" is distinguishable from "the lint was never called".
//! Rows are `info`, written only after the write they describe succeeds
//! ([`write_title_lint_rows`]); a clean pass writes no row.
//!
//! Two layers call it: the Inventory mutations (`item_mutations`) and the
//! document-write layer (`title_lint_documents`), which catches edits made
//! with ordinary document tools.
//!
//! **On this lane's branch the gate-key resolution is a fenced refusal arm**
//! (P4.D340 R-3): `resolve_inventory_ref` is P4.D328's (S10). Until the
//! unifier wires it, a key the census holds for an entering title fails
//! CLOSED (`TITLE_CHECK_UNAVAILABLE`) and one for a leaving title rings `gate
//! unverifiable` — v4's own posture toward "could not look". The production
//! census counts no key on this branch (`tool_gate_keys`, R-4), so only an
//! injected census reaches the arm.

use std::sync::LazyLock;

use regex::Regex;
use rusqlite::Connection;
use serde_json::{json, Map, Value};
use unicode_normalization::UnicodeNormalization;

use super::errors::{InventoryRefusal, InventoryTitleCollision, InventoryTitleLintUnread};
use super::location::InventoryLocation;
use super::pool::{load_pool_for_location, InventoryPool, LoadPoolContext};
use super::resolve::resolve_item;
use super::types::{InventoryFields, InventoryRecord, InventoryScope};
use crate::db::doc_mount_points::DocMountPointsRepository;
use crate::db::fallback;
use crate::jsstr::{is_js_ws, js_trim};
use crate::pascal::tool_gate_keys::{collect_tool_gate_keys, ToolGateKeyCensus, ToolGateKeyUse};

static MARKS: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\p{M}+").expect("\\p{M}+"));

/// Fold a title for comparison: the resolver's own fold (`trim` +
/// `toLowerCase`) first, then NFKD, diacritics stripped, whitespace collapsed
/// and case folded again (v4 `normalizeTitle`). Starting from the resolver's
/// fold guarantees the lint is never looser than the door it guards: any two
/// strings the resolver calls equal, this calls equal. Punctuation,
/// apostrophes included, is kept. The whitespace is JS `\s` (U+FEFF in,
/// U+0085 out), never Rust's.
pub fn normalize_title(value: Option<&str>) -> String {
    let folded = js_trim(value.unwrap_or("")).to_lowercase();
    let decomposed: String = folded.nfkd().collect();
    let stripped = MARKS.replace_all(&decomposed, "");
    let mut collapsed = String::with_capacity(stripped.len());
    let mut in_ws = false;
    for c in stripped.chars() {
        if is_js_ws(c) {
            if !in_ws {
                collapsed.push(' ');
            }
            in_ws = true;
        } else {
            collapsed.push(c);
            in_ws = false;
        }
    }
    js_trim(&collapsed).to_lowercase()
}

/// Which door the write came through; for logs and the census line (v4
/// `TitleLintDoor`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TitleLintDoor {
    Create,
    Update,
    Archive,
    Delete,
    DocumentWrite,
    DocumentDelete,
    DocumentMove,
}

impl TitleLintDoor {
    pub fn as_str(self) -> &'static str {
        match self {
            TitleLintDoor::Create => "create",
            TitleLintDoor::Update => "update",
            TitleLintDoor::Archive => "archive",
            TitleLintDoor::Delete => "delete",
            TitleLintDoor::DocumentWrite => "document-write",
            TitleLintDoor::DocumentDelete => "document-delete",
            TitleLintDoor::DocumentMove => "document-move",
        }
    }
}

/// v4 `TitleLintRequest`.
#[derive(Debug, Clone, Copy)]
pub struct TitleLintRequest<'a> {
    pub door: TitleLintDoor,
    /// The object as stored before the write; `None` on a create.
    pub before: Option<&'a InventoryRecord>,
    /// The object as it will be stored after the write; `None` on a delete or
    /// a move out.
    pub after: Option<&'a InventoryRecord>,
}

/// A row to write once the write succeeds (v4 `TitleLintRow` — the variant's
/// fields in v4's key order, which is the row INFO's field order).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TitleLintRow {
    GateKeyNaming {
        title: String,
        tools: Vec<String>,
    },
    GateDark {
        gate_key: String,
        tools: Vec<String>,
    },
    GateRedirected {
        gate_key: String,
        tools: Vec<String>,
        new_owners: Vec<String>,
    },
    GateUnverifiable {
        gate_key: String,
        tools: Vec<String>,
        unread: Vec<String>,
    },
    GateKeyMoved {
        gate_key: String,
        tools: Vec<String>,
    },
}

impl TitleLintRow {
    /// v4's `reason` literal.
    pub fn reason(&self) -> &'static str {
        match self {
            TitleLintRow::GateKeyNaming { .. } => "gate key naming",
            TitleLintRow::GateDark { .. } => "gate dark",
            TitleLintRow::GateRedirected { .. } => "gate redirected",
            TitleLintRow::GateUnverifiable { .. } => "gate unverifiable",
            TitleLintRow::GateKeyMoved { .. } => "gate key moved",
        }
    }

    /// v4's object literal, in its key order.
    pub fn to_value(&self) -> Value {
        let mut o = Map::new();
        o.insert("reason".into(), json!(self.reason()));
        match self {
            TitleLintRow::GateKeyNaming { title, tools } => {
                o.insert("title".into(), json!(title));
                o.insert("tools".into(), json!(tools));
            }
            TitleLintRow::GateDark { gate_key, tools }
            | TitleLintRow::GateKeyMoved { gate_key, tools } => {
                o.insert("gateKey".into(), json!(gate_key));
                o.insert("tools".into(), json!(tools));
            }
            TitleLintRow::GateRedirected {
                gate_key,
                tools,
                new_owners,
            } => {
                o.insert("gateKey".into(), json!(gate_key));
                o.insert("tools".into(), json!(tools));
                o.insert("newOwners".into(), json!(new_owners));
            }
            TitleLintRow::GateUnverifiable {
                gate_key,
                tools,
                unread,
            } => {
                o.insert("gateKey".into(), json!(gate_key));
                o.insert("tools".into(), json!(tools));
                o.insert("unread".into(), json!(unread));
            }
        }
        Value::Object(o)
    }
}

/// v4 `TitleLintVerdict`: what the write did to the reachable titled set,
/// and the rows to write once it lands.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct TitleLintVerdict {
    pub entering: bool,
    pub leaving: bool,
    pub rows: Vec<TitleLintRow>,
}

impl TitleLintVerdict {
    /// v4's `{event: {entering, leaving}, rows}`.
    pub fn to_value(&self) -> Value {
        json!({
            "event": { "entering": self.entering, "leaving": self.leaving },
            "rows": self.rows.iter().map(TitleLintRow::to_value).collect::<Vec<_>>(),
        })
    }
}

/// A census source (v4 `TitleLintOptions.census` — "Injected census
/// (tests)"; an `Err` is a thrown census).
pub type CensusFn<'a> = &'a dyn Fn() -> Result<ToolGateKeyCensus, String>;

/// v4 `TitleLintOptions` (P4.D340 R-12).
#[derive(Clone, Copy, Default)]
pub struct TitleLintOptions<'a> {
    /// The location's pool, when the caller already has it.
    pub pool: Option<&'a InventoryPool>,
    /// The project context a character's pool is read in.
    pub context: LoadPoolContext<'a>,
    /// Injected census; otherwise `collect_tool_gate_keys`.
    pub census: Option<CensusFn<'a>>,
}

/// The resolved view of one side of the write.
#[derive(Debug, Clone)]
struct Side {
    id: String,
    title: String,
    name: String,
    reachable: bool,
}

fn mount_name(mount: &Connection, mount_point_id: &str) -> Option<String> {
    fallback::find_by_id_or_none("doc_mount_points", mount_point_id, || {
        DocMountPointsRepository::new(mount).find_id_and_name_by_id(mount_point_id)
    })
    .map(|(_, name)| name)
}

/// `name ? \`store "${name}"\` : \`store ${id}\`` — a quoted name, an UNQUOTED
/// id.
fn store_label(id: &str, name: Option<&str>) -> String {
    match name.filter(|n| !n.is_empty()) {
        Some(n) => format!("store \"{n}\""),
        None => format!("store {id}"),
    }
}

/// How an object is named in a message or row.
fn describe_item(item: &impl InventoryFields, origin_name: Option<&str>) -> String {
    let label = match item.title().filter(|t| !t.is_empty()) {
        Some(t) => t.to_string(),
        None => item.name().to_string(),
    };
    match origin_name.filter(|n| !n.is_empty()) {
        Some(o) => format!("{label} ({o})"),
        None => label,
    }
}

/// What one lint call read: the pool and census, and everything it could not
/// read.
struct Reading<'p> {
    pool: Option<PoolRef<'p>>,
    census: Option<ToolGateKeyCensus>,
    /// Unread pools and tool files, by name; non-empty means the protected
    /// set is incomplete.
    unread: Vec<InventoryTitleLintUnread>,
    /// Pools read, by name.
    pools: Vec<String>,
}

/// The caller's pool, or the one the lint loaded itself.
enum PoolRef<'p> {
    Borrowed(&'p InventoryPool),
    Owned(Box<InventoryPool>),
}

impl PoolRef<'_> {
    fn get(&self) -> &InventoryPool {
        match self {
            PoolRef::Borrowed(p) => p,
            PoolRef::Owned(p) => p,
        }
    }
}

/// Read the pool and the census, never failing: every failure lands in
/// `unread`, named. A consulted store that does not exist reads as an empty
/// folder, so it is checked for and counted as unread too.
fn read<'p>(
    main: &Connection,
    mount: &Connection,
    location: &InventoryLocation,
    opts: &TitleLintOptions<'p>,
) -> Reading<'p> {
    let mut unread = Vec::new();
    let mut pools = Vec::new();

    let pool = match opts.pool {
        Some(p) => Some(PoolRef::Borrowed(p)),
        None => match load_pool_for_location(main, mount, location, opts.context) {
            Ok(p) => Some(PoolRef::Owned(Box::new(p))),
            Err(error) => {
                let owner = if location.origin.name.is_empty() {
                    location.mount_point_id.as_str()
                } else {
                    location.origin.name.as_str()
                };
                unread.push(InventoryTitleLintUnread {
                    pool: format!("the {} inventory \"{owner}\"", location.scope.as_str()),
                    reason: fallback::error_text(&error),
                });
                None
            }
        },
    };
    if let Some(p) = &pool {
        let p = p.get();
        let failed: Vec<&str> = p
            .unread_mounts
            .iter()
            .map(|u| u.mount_point_id.as_str())
            .collect();
        for u in &p.unread_mounts {
            let name = mount_name(mount, &u.mount_point_id);
            unread.push(InventoryTitleLintUnread {
                pool: format!(
                    "the inventory in {}",
                    store_label(&u.mount_point_id, name.as_deref())
                ),
                reason: u.error.clone(),
            });
        }
        let mut seen: Vec<&str> = Vec::new();
        for mp in &p.consulted_mounts {
            if seen.contains(&mp.as_str()) {
                continue;
            }
            seen.push(mp);
            if failed.contains(&mp.as_str()) {
                continue;
            }
            match mount_name(mount, mp) {
                None => unread.push(InventoryTitleLintUnread {
                    pool: format!("the inventory in store {mp}"),
                    reason: "no such store; an empty read from it is not an answer".to_string(),
                }),
                Some(name) => pools.push(store_label(mp, Some(&name))),
            }
        }
    }

    let census_result = match opts.census {
        Some(census) => census(),
        None => Ok(collect_tool_gate_keys(mount)),
    };
    let census = match census_result {
        Ok(c) => {
            for u in &c.unread {
                unread.push(InventoryTitleLintUnread {
                    pool: format!("{} in store \"{}\"", u.definition_path, u.mount_name),
                    reason: u.reason.clone(),
                });
            }
            Some(c)
        }
        Err(reason) => {
            unread.push(InventoryTitleLintUnread {
                pool: "the custom tools' Tools/ folders".to_string(),
                reason,
            });
            None
        }
    };

    Reading {
        pool,
        census,
        unread,
        pools,
    }
}

/// One side of the write, resolved against the pool (raw when the pool could
/// not be read).
fn side_of(record: Option<&InventoryRecord>, pool: Option<&InventoryPool>) -> Option<Side> {
    let record = record?;
    let resolved = match pool {
        Some(p) => p.resolve(record),
        None => resolve_item(record, |_| None),
    };
    Some(Side {
        id: record.id().to_string(),
        title: resolved
            .title()
            .map(|t| js_trim(t).to_string())
            .unwrap_or_default(),
        name: resolved.name().to_string(),
        // An archetype counts: its title is inherited by every instance that
        // does not override it.
        reachable: !super::types::js_truthy(resolved.get("archivedAt")),
    })
}

/// Gate uses for one normalized key, grouped by the ref as written (first-seen
/// order).
fn uses_of<'c>(
    census: &'c ToolGateKeyCensus,
    key: &str,
) -> Vec<(&'c str, Vec<&'c ToolGateKeyUse>)> {
    let mut out: Vec<(&str, Vec<&ToolGateKeyUse>)> = Vec::new();
    for u in &census.keys {
        if normalize_title(Some(&u.reference)) != key {
            continue;
        }
        match out.iter_mut().find(|(r, _)| *r == u.reference) {
            Some((_, uses)) => uses.push(u),
            None => out.push((&u.reference, vec![u])),
        }
    }
    out
}

/// `Array.from(new Set(names)).sort()` — the default sort is UTF-16 order.
fn unique_tools(uses: &[&ToolGateKeyUse]) -> Vec<String> {
    let mut names: Vec<String> = Vec::new();
    for u in uses {
        if !names.contains(&u.tool_name) {
            names.push(u.tool_name.clone());
        }
    }
    names.sort_by(|a, b| a.encode_utf16().cmp(b.encode_utf16()));
    names
}

// === LANE-REMOVE(P4.D340 → §S.5, v4 7263105c6): S10 resolve_inventory_ref is P4.D328's (C2) ===
// The unifier replaces `gate_key_hits`' body with `resolve_inventory_ref(
// reference, &ResolvedInventoryPool { items, owns }, &HashSet::new(),
// ResolveInventoryRefOptions::default())` and deletes `PoolView` / `GateKeyHit`
// for C2's `ResolvedInventoryPool` / `InventoryRefMatch` (`hit.item`).

/// The pool as the resolver sees it, the object replaced (or left out) — v4
/// `resolvedWith`.
#[allow(dead_code)]
struct PoolView<'p> {
    items: Vec<super::types::ResolvedInventoryItem>,
    pool: &'p InventoryPool,
}

/// One resolver hit (C2's `InventoryRefMatch`).
#[allow(dead_code)]
struct GateKeyHit {
    item: super::types::ResolvedInventoryItem,
}

/// v4 `resolvedWith(pool, itemId, replacement)`.
fn resolved_with<'p>(
    pool: &'p InventoryPool,
    item_id: &str,
    replacement: Option<&Side>,
) -> PoolView<'p> {
    let items = pool
        .resolved()
        .into_iter()
        .filter_map(|mut item| {
            if item.id() != item_id {
                return Some(item);
            }
            let r = replacement.filter(|r| r.reachable)?;
            item.set(
                "title",
                if r.title.is_empty() {
                    Value::Null
                } else {
                    json!(r.title)
                },
            );
            item.set("name", json!(r.name));
            Some(item)
        })
        .collect();
    PoolView { items, pool }
}

fn gate_key_hits(_reference: &str, _pool: &PoolView<'_>) -> Result<Vec<GateKeyHit>, &'static str> {
    Err("not_available: resolve_inventory_ref (P4.D328, S10) is not on this branch")
}

/// The unread entry a failed resolution stands for (R-3).
fn unresolved_key(reference: &str, reason: &str) -> InventoryTitleLintUnread {
    InventoryTitleLintUnread {
        pool: format!("the gate key \"{reference}\""),
        reason: reason.to_string(),
    }
}
// === end LANE-REMOVE(P4.D340 → §S.5, v4 7263105c6) ===

fn describe_hit(hit: &GateKeyHit) -> String {
    let origin = hit.item.origin().map(|o| o.name);
    describe_item(&hit.item, origin.as_deref())
}

fn json_text(v: &impl serde::Serialize) -> String {
    serde_json::to_string(v).unwrap_or_default()
}

fn null_or<T: std::fmt::Display>(v: Option<T>) -> String {
    v.map_or_else(|| "null".to_string(), |n| n.to_string())
}

/// Lint one write (v4 `lintInventoryTitle`). Refuses `TITLE_SHADOW` on a
/// collision and `TITLE_CHECK_UNAVAILABLE` when an entering title cannot be
/// checked; otherwise answers the rows to write once the write succeeds.
pub fn lint_inventory_title(
    main: &Connection,
    mount: &Connection,
    location: &InventoryLocation,
    request: TitleLintRequest<'_>,
    opts: TitleLintOptions<'_>,
) -> Result<TitleLintVerdict, InventoryRefusal> {
    let mut reading = read(main, mount, location, &opts);
    let pool = reading.pool.as_ref().map(PoolRef::get);
    let before = side_of(request.before, pool);
    let after = side_of(request.after, pool);
    let item_id = after
        .as_ref()
        .or(before.as_ref())
        .map(|s| s.id.clone())
        .unwrap_or_default();

    let key_of = |s: &Option<Side>| match s {
        Some(s) if s.reachable => normalize_title(Some(&s.title)),
        _ => String::new(),
    };
    let before_key = key_of(&before);
    let after_key = key_of(&after);
    let name_changed = match (&before, &after) {
        (Some(b), Some(a)) => normalize_title(Some(&b.name)) != normalize_title(Some(&a.name)),
        _ => false,
    };
    let entering = !after_key.is_empty() && (after_key != before_key || name_changed);
    let leaving = !before_key.is_empty() && before_key != after_key;

    let door = request.door.as_str();
    let scope = location.scope.as_str();
    let mount_point_id = location.mount_point_id.as_str();
    tracing::debug!(
        door = door,
        itemId = %item_id,
        scope = scope,
        mountPointId = %mount_point_id,
        context = "inventory",
        entering = entering,
        leaving = leaving,
        poolsJson = %json_text(&reading.pools),
        reachableObjects = %null_or(pool.map(|p| p.by_id.len())),
        toolStoresJson = %json_text(
            &reading.census.as_ref().map(|c| c.stores_consulted.clone()).unwrap_or_default()
        ),
        gateKeys = %null_or(reading.census.as_ref().map(|c| c.keys.len())),
        unreadJson = %json_text(&reading.unread),
        "[InventoryTitleLint] Census"
    );

    let refused_incomplete = |title: &str, unread: &[InventoryTitleLintUnread]| {
        tracing::info!(
            door = door,
            itemId = %item_id,
            scope = scope,
            mountPointId = %mount_point_id,
            context = "inventory",
            title = %title,
            unreadJson = %json_text(&unread),
            "[InventoryTitleLint] Title refused: protected set incomplete"
        );
        InventoryRefusal::title_check_unavailable(title, unread)
    };

    let mut rows = Vec::new();

    if entering {
        let after_side = after.as_ref().expect("entering has an after");
        let title = after_side.title.clone();
        let (Some(pool), Some(census)) = (pool, reading.census.as_ref()) else {
            return Err(refused_incomplete(&title, &reading.unread));
        };
        if !reading.unread.is_empty() {
            return Err(refused_incomplete(&title, &reading.unread));
        }
        let mut collisions: Vec<InventoryTitleCollision> = Vec::new();

        // 1. Kind names: every reachable object's name, the object's own (new)
        // one included. FIRST spelling wins; `by_id` is the archived-included
        // shadow, archetypes counted.
        let mut kind_names: Vec<(String, String)> = vec![(
            normalize_title(Some(&after_side.name)),
            after_side.name.clone(),
        )];
        for record in pool.by_id.values() {
            if record.id() == item_id {
                continue;
            }
            let name = pool.resolve(record).name().to_string();
            let key = normalize_title(Some(&name));
            if !key.is_empty() && !kind_names.iter().any(|(k, _)| *k == key) {
                kind_names.push((key, name));
            }
        }
        if let Some((_, kind_name)) = kind_names.iter().find(|(k, _)| *k == after_key) {
            collisions.push(InventoryTitleCollision::KindNameShadow {
                kind_name: kind_name.clone(),
            });
        }

        // 2. Gate keys: shadowing when the key lands elsewhere, naming when it
        // lands nowhere.
        let mut naming: Vec<&ToolGateKeyUse> = Vec::new();
        let others = resolved_with(pool, &item_id, None);
        for (reference, ref_uses) in uses_of(census, &after_key) {
            let hits = match gate_key_hits(reference, &others) {
                Ok(hits) => hits,
                // === LANE-REMOVE(P4.D340 → §S.5, v4 7263105c6): S10 — an unresolvable key
                // is one more unread entry: the entering title fails closed. ===
                Err(reason) => {
                    reading.unread.push(unresolved_key(reference, reason));
                    return Err(refused_incomplete(&title, &reading.unread));
                } // === end LANE-REMOVE(P4.D340 → §S.5, v4 7263105c6) ===
            };
            if hits.is_empty() {
                naming.extend(ref_uses);
                continue;
            }
            for tool_name in unique_tools(&ref_uses) {
                collisions.push(InventoryTitleCollision::LiveGateShadow {
                    tool_name,
                    reference: reference.to_string(),
                    lands_on: hits.iter().map(describe_hit).collect(),
                });
            }
        }

        if !collisions.is_empty() {
            let collisions_json: Vec<Value> = collisions.iter().map(|c| c.to_value()).collect();
            tracing::info!(
                door = door,
                itemId = %item_id,
                scope = scope,
                mountPointId = %mount_point_id,
                context = "inventory",
                title = %title,
                collisionsJson = %json_text(&collisions_json),
                "[InventoryTitleLint] Title rejected"
            );
            return Err(InventoryRefusal::title_shadow(&title, &collisions));
        }
        if !naming.is_empty() {
            rows.push(TitleLintRow::GateKeyNaming {
                title,
                tools: unique_tools(&naming),
            });
        }
    }

    if leaving {
        let before_side = before.as_ref().expect("leaving has a before");
        rows.extend(leaving_rows(
            &reading,
            before_side,
            after.as_ref(),
            &item_id,
        ));
    }

    Ok(TitleLintVerdict {
        entering,
        leaving,
        rows,
    })
}

/// Where the gate keys a leaving title leaves behind now land (v4
/// `leavingRows`).
fn leaving_rows(
    reading: &Reading<'_>,
    before: &Side,
    after: Option<&Side>,
    item_id: &str,
) -> Vec<TitleLintRow> {
    let pool = reading.pool.as_ref().map(PoolRef::get);
    let unread_names: Vec<String> = reading.unread.iter().map(|u| u.pool.clone()).collect();
    let Some(census) = reading.census.as_ref() else {
        // The tool files themselves could not be listed: whether this title
        // was a gate key is unknown.
        return vec![TitleLintRow::GateUnverifiable {
            gate_key: before.title.clone(),
            tools: Vec::new(),
            unread: unread_names,
        }];
    };
    let mut rows = Vec::new();
    for (reference, ref_uses) in uses_of(census, &normalize_title(Some(&before.title))) {
        let tools = unique_tools(&ref_uses);
        let Some(pool) = pool else {
            rows.push(TitleLintRow::GateUnverifiable {
                gate_key: reference.to_string(),
                tools,
                unread: unread_names.clone(),
            });
            continue;
        };
        let hits = match gate_key_hits(reference, &resolved_with(pool, item_id, after)) {
            Ok(hits) => hits,
            // === LANE-REMOVE(P4.D340 → §S.5, v4 7263105c6): S10 — an unresolvable key
            // cannot tell dark from redirected: unverifiable. ===
            Err(reason) => {
                let mut unread = unread_names.clone();
                unread.push(unresolved_key(reference, reason).pool);
                rows.push(TitleLintRow::GateUnverifiable {
                    gate_key: reference.to_string(),
                    tools,
                    unread,
                });
                continue;
            } // === end LANE-REMOVE(P4.D340 → §S.5, v4 7263105c6) ===
        };
        let others: Vec<&GateKeyHit> = hits.iter().filter(|h| h.item.id() != item_id).collect();
        if !others.is_empty() {
            rows.push(TitleLintRow::GateRedirected {
                gate_key: reference.to_string(),
                tools,
                new_owners: others.iter().map(|h| describe_hit(h)).collect(),
            });
        } else if !hits.is_empty() {
            rows.push(TitleLintRow::GateKeyMoved {
                gate_key: reference.to_string(),
                tools,
            });
        } else if !reading.unread.is_empty() {
            // Dark here, but an unread mount may hold the title: which
            // emergency cannot be told.
            rows.push(TitleLintRow::GateUnverifiable {
                gate_key: reference.to_string(),
                tools,
                unread: unread_names.clone(),
            });
        } else {
            rows.push(TitleLintRow::GateDark {
                gate_key: reference.to_string(),
                tools,
            });
        }
    }
    if !census.unread.is_empty() && rows.is_empty() {
        // A tool file that could not be read may key on this title.
        rows.push(TitleLintRow::GateUnverifiable {
            gate_key: before.title.clone(),
            tools: Vec::new(),
            unread: unread_names,
        });
    }
    rows
}

/// Write a verdict's rows, once the write they describe has succeeded (v4
/// `writeTitleLintRows`). `reason` keeps the row's FIRST position, its value
/// overwritten with the redirect's owners / the unread names.
pub fn write_title_lint_rows(
    verdict: &TitleLintVerdict,
    item_id: &str,
    scope: InventoryScope,
    mount_point_id: &str,
) {
    let scope = scope.as_str();
    for row in &verdict.rows {
        match row {
            TitleLintRow::GateKeyNaming { title, tools } => tracing::info!(
                reason = "gate key naming",
                title = %title,
                toolsJson = %json_text(tools),
                itemId = %item_id,
                scope = scope,
                mountPointId = %mount_point_id,
                alert = false,
                context = "inventory",
                "[InventoryTitleLint] Title doubles as address and gate key"
            ),
            TitleLintRow::GateKeyMoved { gate_key, tools } => tracing::info!(
                reason = "gate key moved",
                gateKey = %gate_key,
                toolsJson = %json_text(tools),
                itemId = %item_id,
                scope = scope,
                mountPointId = %mount_point_id,
                alert = false,
                context = "inventory",
                "[InventoryTitleLint] A gate key was left behind; it still lands on this object by name"
            ),
            TitleLintRow::GateDark { gate_key, tools } => tracing::info!(
                reason = "gate dark",
                gateKey = %gate_key,
                toolsJson = %json_text(tools),
                itemId = %item_id,
                scope = scope,
                mountPointId = %mount_point_id,
                alert = true,
                context = "inventory",
                "[InventoryTitleLint] A gate key was left behind"
            ),
            TitleLintRow::GateRedirected {
                gate_key,
                tools,
                new_owners,
            } => tracing::info!(
                reason = %format!("gate redirected: {}", new_owners.join(", ")),
                gateKey = %gate_key,
                toolsJson = %json_text(tools),
                newOwnersJson = %json_text(new_owners),
                itemId = %item_id,
                scope = scope,
                mountPointId = %mount_point_id,
                alert = true,
                context = "inventory",
                "[InventoryTitleLint] A gate key was left behind"
            ),
            TitleLintRow::GateUnverifiable {
                gate_key,
                tools,
                unread,
            } => tracing::info!(
                reason = %format!("gate unverifiable: {}", unread.join("; ")),
                gateKey = %gate_key,
                toolsJson = %json_text(tools),
                unreadJson = %json_text(unread),
                itemId = %item_id,
                scope = scope,
                mountPointId = %mount_point_id,
                alert = true,
                context = "inventory",
                "[InventoryTitleLint] A gate key was left behind"
            ),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn v4_normalization_rows() {
        // title-lint.test.ts:74-88.
        assert_eq!(
            normalize_title(Some("Friday's Yellow Tape Measure")),
            "friday's yellow tape measure"
        );
        assert_eq!(normalize_title(Some("  Brass\t\tLamp  ")), "brass lamp");
        assert_eq!(
            normalize_title(Some("Ansíble Résonator")),
            "ansible resonator"
        );
        assert_eq!(normalize_title(None), "");
    }

    #[test]
    fn js_whitespace_not_rusts() {
        assert_eq!(normalize_title(Some("a\u{FEFF}b")), "a b");
        assert_eq!(normalize_title(Some("a\u{85}b")), "a\u{85}b");
        assert_eq!(normalize_title(Some("\u{FEFF}x\u{3000}")), "x");
    }

    #[test]
    fn marks_after_nfkd() {
        // Precomposed é decomposes, then loses its mark.
        assert_eq!(normalize_title(Some("\u{E9}")), "e");
        assert_eq!(normalize_title(Some("\u{FB01}")), "fi");
        assert_eq!(normalize_title(Some("\u{130}")), "i");
    }

    #[test]
    fn row_key_order() {
        let row = TitleLintRow::GateRedirected {
            gate_key: "TAPE".into(),
            tools: vec!["a".into()],
            new_owners: vec!["b".into()],
        };
        assert_eq!(
            serde_json::to_string(&row.to_value()).unwrap(),
            r#"{"reason":"gate redirected","gateKey":"TAPE","tools":["a"],"newOwners":["b"]}"#
        );
    }
}
