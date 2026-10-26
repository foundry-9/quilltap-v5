//! The nine `inventory_*` tools as ONE table (v4 `lib/tools/handlers/
//! inventory-tool-table.ts`, `e1872c94f`; P4.D328).
//!
//! v4 builds ONE `InventoryToolContext` for all nine tools, so the turn's
//! pending Inventory announcements reach every tool that changes what a
//! character carries. Every tool loads ONE context per call
//! (`inventory::context::load_inventory_context` — the pool, the equipped set
//! and the roster-closed shelf).
//!
//! ## Two halves (the `wardrobe_tool_table` shape, R-3)
//!
//! v5's handlers are synchronous and run inside ONE `Db::write` closure that
//! holds both writer connections (the keystone's chokepoints are in-write:
//! equip / unequip / pick-up / drop / transfer, and `inventory_update`'s second
//! context load is a second read in the SAME closure). Pascal's line, the
//! avatar refresh and the realtime publishes cannot run in there, so each
//! entry is two halves, composed the same way by every caller (the executor,
//! and the differential):
//!
//!  1. [`run_inventory_tool_handler`] — the handler, inside the write. It
//!     answers the tool's output, the changes it made (one per gesture, each
//!     with its chat id), the publishes it owes, and its ONE
//!     `[InventoryTools]` line.
//!  2. [`finish_inventory_tool`] — once the write has committed: the
//!     publishes in v4's order, then `notify_inventory_changed` per change in
//!     v4's order (joining the turn's map, or posting now), then the handler's
//!     line — which v4 writes AFTER its notifies (each chokepoint notifies
//!     inside the handler, before the handler's own log call).
//!  3. [`InventoryToolRun::run`] — v4's `run(execute, format)` projection.

use rusqlite::Connection;
use serde::Serialize;
use serde_json::{Map, Value};

use crate::db::runtime::Db;
use crate::realtime::bus::publish_realtime;
use crate::realtime::types::RealtimeTopic;
use crate::services::inventory_change_effects::{
    notify_inventory_changed, InventoryChange, InventoryChangeContext, InventoryChangeProvenance,
    PendingInventoryAnnouncements,
};
use crate::services::mount_index::blob_transcode::WebpTranscoder;

use super::inventory_shared::{log_inventory_tool, InventoryToolLog, InventoryToolLogLevel};
use super::{
    inventory_archive, inventory_create, inventory_equip, inventory_give, inventory_list,
    inventory_pick_up, inventory_read, inventory_unequip, inventory_update,
};

/// The nine tools, the table's keys in v4's order (v4 `INVENTORY_TOOL_NAMES =
/// Object.keys(INVENTORY_TOOL_TABLE)`).
pub const INVENTORY_TOOL_NAMES: [&str; 9] = [
    "inventory_list",
    "inventory_read",
    "inventory_equip",
    "inventory_unequip",
    "inventory_pick_up",
    "inventory_create",
    "inventory_update",
    "inventory_archive",
    "inventory_give",
];

/// v4 `InventoryToolContext` — the ONE context every inventory tool runs in.
/// The executor builds it once from the tool-execution context, so the turn's
/// pending map reaches every tool that changes what a character carries.
#[derive(Clone, Copy, Debug)]
pub struct InventoryToolContext<'a> {
    pub user_id: &'a str,
    pub chat_id: &'a str,
    /// The calling character — the one whose bag every read and write uses.
    pub character_id: &'a str,
    pub pending_inventory_announcements: Option<&'a PendingInventoryAnnouncements>,
}

/// What a handler half runs over: the two writer connections, the caller,
/// and the bridge's WebP encoder (a give's transfer carries the pictures).
#[derive(Clone, Copy)]
pub struct InventoryHandlerCx<'a> {
    pub main: &'a Connection,
    pub mount: &'a Connection,
    pub user_id: &'a str,
    pub chat_id: &'a str,
    pub character_id: &'a str,
    pub blob_webp: &'a dyn WebpTranscoder,
}

/// What one handler answers from inside the write.
#[derive(Debug)]
pub struct InventoryHandled<O> {
    pub output: O,
    /// The changes to notify, each with its chat id, in v4's order.
    pub changes: Vec<(String, InventoryChange)>,
    /// The realtime publishes the HANDLER owes (the chokepoints publish their
    /// own), in v4's order.
    pub publishes: Vec<(RealtimeTopic, Option<String>)>,
    /// The handler's one `[InventoryTools]` line (`None` on v4's few silent
    /// returns — a missing chat, a missing vault, an update / archive whose
    /// ref resolves to nothing).
    pub log: Option<(InventoryToolLog, InventoryToolLogLevel)>,
}

impl<O> InventoryHandled<O> {
    /// A return with no effects.
    pub fn quiet(output: O) -> Self {
        InventoryHandled {
            output,
            changes: Vec::new(),
            publishes: Vec::new(),
            log: None,
        }
    }

    /// A return with no effects and the handler's line.
    pub fn logged(output: O, log: InventoryToolLog, level: InventoryToolLogLevel) -> Self {
        InventoryHandled {
            output,
            changes: Vec::new(),
            publishes: Vec::new(),
            log: Some((log, level)),
        }
    }
}

/// One tool's handler output (v4's `execute*` return value).
#[derive(Debug)]
pub enum InventoryToolOutput {
    List(inventory_list::InventoryListToolOutput),
    Read(Box<inventory_read::InventoryReadToolOutput>),
    Equip(inventory_equip::InventoryEquipToolOutput),
    Unequip(inventory_unequip::InventoryUnequipToolOutput),
    PickUp(inventory_pick_up::InventoryPickUpToolOutput),
    Create(inventory_create::InventoryCreateToolOutput),
    Update(inventory_update::InventoryUpdateToolOutput),
    Archive(inventory_archive::InventoryArchiveToolOutput),
    Give(inventory_give::InventoryGiveToolOutput),
}

/// What [`run_inventory_tool_handler`] hands out of the write.
#[derive(Debug)]
pub struct InventoryToolStage {
    pub tool: &'static str,
    pub output: InventoryToolOutput,
    pub changes: Vec<(String, InventoryChange)>,
    pub publishes: Vec<(RealtimeTopic, Option<String>)>,
    pub log: Option<(InventoryToolLog, InventoryToolLogLevel)>,
}

fn stage<O>(
    tool: &'static str,
    handled: InventoryHandled<O>,
    wrap: impl FnOnce(O) -> InventoryToolOutput,
) -> InventoryToolStage {
    InventoryToolStage {
        tool,
        output: wrap(handled.output),
        changes: handled.changes,
        publishes: handled.publishes,
        log: handled.log,
    }
}

/// The handler half — runs inside the write with both connections. `None` for
/// a name the table does not hold.
pub fn run_inventory_tool_handler(
    name: &str,
    cx: &InventoryHandlerCx<'_>,
    args: &Value,
) -> Option<InventoryToolStage> {
    Some(match name {
        "inventory_list" => stage(
            "inventory_list",
            inventory_list::execute(cx, args),
            InventoryToolOutput::List,
        ),
        "inventory_read" => stage("inventory_read", inventory_read::execute(cx, args), |o| {
            InventoryToolOutput::Read(Box::new(o))
        }),
        "inventory_equip" => stage(
            "inventory_equip",
            inventory_equip::execute(cx, args),
            InventoryToolOutput::Equip,
        ),
        "inventory_unequip" => stage(
            "inventory_unequip",
            inventory_unequip::execute(cx, args),
            InventoryToolOutput::Unequip,
        ),
        "inventory_pick_up" => stage(
            "inventory_pick_up",
            inventory_pick_up::execute(cx, args),
            InventoryToolOutput::PickUp,
        ),
        "inventory_create" => stage(
            "inventory_create",
            inventory_create::execute(cx, args),
            InventoryToolOutput::Create,
        ),
        "inventory_update" => stage(
            "inventory_update",
            inventory_update::execute(cx, args),
            InventoryToolOutput::Update,
        ),
        "inventory_archive" => stage(
            "inventory_archive",
            inventory_archive::execute(cx, args),
            InventoryToolOutput::Archive,
        ),
        "inventory_give" => stage(
            "inventory_give",
            inventory_give::execute(cx, args),
            InventoryToolOutput::Give,
        ),
        _ => return None,
    })
}

/// The after-commit half: the publishes, then every change's effects over
/// the ONE context's pending map (a character's own gesture — `character`
/// provenance, announced — v4 `characterChangeContext`), then the handler's
/// line.
pub async fn finish_inventory_tool(
    db: &Db,
    ctx: &InventoryToolContext<'_>,
    stage: InventoryToolStage,
) -> InventoryToolOutput {
    let InventoryToolStage {
        tool,
        output,
        changes,
        publishes,
        log,
    } = stage;
    for (topic, id) in &publishes {
        publish_realtime(*topic, id.as_deref());
    }
    let change_ctx = InventoryChangeContext {
        user_id: ctx.user_id,
        provenance: InventoryChangeProvenance::Character,
        announce: true,
        pending: None,
    };
    for (chat_id, change) in &changes {
        notify_inventory_changed(
            db,
            chat_id,
            ctx.character_id,
            change,
            &change_ctx,
            ctx.pending_inventory_announcements,
        )
        .await;
    }
    if let Some((data, level)) = &log {
        log_inventory_tool(tool, ctx.chat_id, ctx.character_id, data, *level);
    }
    output
}

/// v4 `InventoryToolRun` — the executor's row for one call.
#[derive(Debug, Clone, PartialEq)]
pub struct InventoryToolRun {
    pub success: bool,
    /// `{ formattedText, …payload }` on success, `null` on a failure.
    pub result: Value,
    pub error: Option<String>,
}

/// v4 `run(execute, format)`: `{ success, error, ...payload } = r`; a success
/// answers `{ formattedText: format(r), ...payload }` (the payload in the
/// handler's literal key order, `success` / `error` stripped), a failure
/// `result: null` and the error.
pub fn run_projection<T: Serialize>(
    output: &T,
    success: bool,
    formatted: String,
) -> InventoryToolRun {
    let value = serde_json::to_value(output).unwrap_or(Value::Null);
    let mut map = match value {
        Value::Object(m) => m,
        _ => Map::new(),
    };
    let error = map
        .shift_remove("error")
        .and_then(|e| e.as_str().map(str::to_string));
    map.shift_remove("success");
    if !success {
        return InventoryToolRun {
            success,
            result: Value::Null,
            error,
        };
    }
    let mut result = Map::new();
    result.insert("formattedText".into(), Value::String(formatted));
    result.extend(map);
    InventoryToolRun {
        success,
        result: Value::Object(result),
        error: None,
    }
}

impl InventoryToolOutput {
    /// v4's per-tool `format*Results`.
    pub fn format(&self) -> String {
        match self {
            InventoryToolOutput::List(o) => inventory_list::format(o),
            InventoryToolOutput::Read(o) => inventory_read::format(o),
            InventoryToolOutput::Equip(o) => inventory_equip::format(o),
            InventoryToolOutput::Unequip(o) => inventory_unequip::format(o),
            InventoryToolOutput::PickUp(o) => inventory_pick_up::format(o),
            InventoryToolOutput::Create(o) => inventory_create::format(o),
            InventoryToolOutput::Update(o) => inventory_update::format(o),
            InventoryToolOutput::Archive(o) => inventory_archive::format(o),
            InventoryToolOutput::Give(o) => inventory_give::format(o),
        }
    }

    /// The executor's row (v4 `INVENTORY_TOOL_TABLE[name](input, context)`).
    pub fn run(&self) -> InventoryToolRun {
        let formatted = self.format();
        match self {
            InventoryToolOutput::List(o) => run_projection(o, o.success, formatted),
            InventoryToolOutput::Read(o) => run_projection(o.as_ref(), o.success, formatted),
            InventoryToolOutput::Equip(o) => run_projection(o, o.success, formatted),
            InventoryToolOutput::Unequip(o) => run_projection(o, o.success, formatted),
            InventoryToolOutput::PickUp(o) => run_projection(o, o.success, formatted),
            InventoryToolOutput::Create(o) => run_projection(o, o.success, formatted),
            InventoryToolOutput::Update(o) => run_projection(o, o.success, formatted),
            InventoryToolOutput::Archive(o) => run_projection(o, o.success, formatted),
            InventoryToolOutput::Give(o) => run_projection(o, o.success, formatted),
        }
    }
}

#[cfg(test)]
mod tests {
    /// P4.D328 Tier 2 item 1 — the `[InventoryTools]` line census as a
    /// source-text guard. A handler's one line is DATA on `InventoryHandled`,
    /// written once by `finish_inventory_tool`, so the way to drop a line is
    /// an exit arm answering `InventoryHandled::quiet` where v4 logs. Each
    /// handler's silent exits are pinned to v4's (`e1872c94f` — the returns
    /// with no `logInventoryTool` call: a missing chat, a missing vault, an
    /// update / archive whose ref resolves to nothing, an update that wrote
    /// nothing); an added or a lost one reds here. The behavioural half is
    /// `inventory_tools_equivalence`, which compares every line whole.
    const SILENT_EXITS: [(&str, &str, usize); 9] = [
        ("inventory_list", include_str!("inventory_list.rs"), 0),
        ("inventory_read", include_str!("inventory_read.rs"), 0),
        ("inventory_equip", include_str!("inventory_equip.rs"), 1),
        ("inventory_unequip", include_str!("inventory_unequip.rs"), 1),
        ("inventory_pick_up", include_str!("inventory_pick_up.rs"), 1),
        ("inventory_create", include_str!("inventory_create.rs"), 1),
        ("inventory_update", include_str!("inventory_update.rs"), 3),
        ("inventory_archive", include_str!("inventory_archive.rs"), 2),
        ("inventory_give", include_str!("inventory_give.rs"), 0),
    ];

    #[test]
    fn every_handler_keeps_v4s_silent_exits_and_no_more() {
        for (tool, src, want) in SILENT_EXITS {
            assert_eq!(
                src.matches("InventoryHandled::quiet(").count(),
                want,
                "{tool}: the silent exits (v4 logs on every other arm)"
            );
            assert!(
                !src.contains("log_inventory_tool("),
                "{tool}: a handler never writes its own line — it answers it as data"
            );
        }
        // The one writer of the line, in the table's after-commit half.
        let table = include_str!("inventory_tool_table.rs");
        assert_eq!(
            table
                .matches(concat!("log_inventory_tool", "(tool,"))
                .count(),
            1,
            "the table writes every handler's line from ONE call"
        );
    }
}
