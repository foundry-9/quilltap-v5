//! The nine `wardrobe_*` tools as ONE table (v4 `lib/tools/handlers/
//! wardrobe-tool-table.ts` + the new halves of `wardrobe-handler-shared.ts`,
//! `9ecebf986`; `wardrobe_rollback` / `wardrobe_history`, `ebb480edd`).
//!
//! v4 builds ONE `WardrobeToolContext` for all nine tools, so the turn's
//! announcement set reaches every tool that changes an outfit — none can be
//! the one that forgets it (bug 193: `wardrobe_create` + `equip_now` had fired
//! the avatar alone). Every tool reads ONE wearable pool per call
//! ([`load_tool_pool`]).

use rusqlite::Connection;

use crate::wearable_pool::{load_wearable_pool, LoadWearablePoolOptions, WearablePool};

/// v4 `loadToolPool(repos, chatId, characterId)` — the wearable pool a
/// character's tool call sees: their own vault, their groups' stores, the
/// chat's project stores (behind the project roster — a character tool call
/// is never the operator), and Quilltap General. One read per tier for the
/// whole tool call.
pub fn load_tool_pool(
    main: &Connection,
    mount: &Connection,
    chat_id: &str,
    character_id: &str,
) -> WearablePool {
    load_wearable_pool(
        main,
        mount,
        character_id,
        None,
        LoadWearablePoolOptions {
            chat_id: Some(chat_id),
            ..Default::default()
        },
    )
}

/// v4 `sharedWardrobeItemReadOnlyMessage(title, verb)` — the refusal when the
/// model tries to mutate a shared (group / project / Quilltap General) item.
/// Phrased identically across `wardrobe_update` (`changed`) and
/// `wardrobe_archive` (`archived`).
pub fn shared_wardrobe_item_read_only_message(title: &str, verb: &str) -> String {
    format!(
        "\"{title}\" is a shared wardrobe item — you can wear it but not edit or retire it. \
Only items in your own wardrobe can be {verb}."
    )
}

// ── The table (v4 `WARDROBE_TOOL_TABLE`) ────────────────────────────────────
//
// v5's handlers are synchronous and run inside ONE `Db::write` closure that
// holds both writer connections; the outfit-change effects and the picture
// are async enqueues that cannot run in there. So each table entry is two
// halves, and every caller composes them the same way (the executor, and the
// differential through [`run_wardrobe_tool_handler`] +
// [`finish_wardrobe_tool`]):
//
//  1. [`run_wardrobe_tool_handler`] — the handler, inside the write. It
//     answers the tool's output plus the characters whose outfit it changed.
//  2. [`finish_wardrobe_tool`] — once the write has committed: v4's
//     `notifyWardrobeChanged` for every changed character (the avatar
//     refresh, then the turn's announcement set — the SAME context for all
//     seven tools, so `wardrobe_create` + `equip_now` can no longer forget
//     the set, bug 193), then the picture (`wardrobe_create` /
//     `wardrobe_update`), then their `… completed` INFO, which carries the
//     picture's status.
//  3. [`WardrobeToolOutput::run`] — v4's per-tool payload projection.

use std::collections::HashSet;
use std::sync::{Arc, Mutex};

use serde_json::{json, Map, Value};

use crate::db::chat_wardrobe_history::{WardrobeHistoryIntent, WardrobeHistoryProvenance};
use crate::db::runtime::Db;
use crate::services::outfit_change_effects::{
    notify_wardrobe_changed, OutfitChangeContext, PendingWardrobeHistory,
};
use crate::services::tool_image_generation::{
    maybe_queue_wardrobe_tool_image, QueueWardrobeToolImageArgs,
};

use super::wardrobe_read::WardrobeReadToolOutput;
use super::{
    wardrobe_archive, wardrobe_create, wardrobe_history, wardrobe_list, wardrobe_read,
    wardrobe_rollback, wardrobe_take_off, wardrobe_update, wardrobe_wear,
};

/// The nine `wardrobe_*` tools, the table's keys in v4's order.
pub const WARDROBE_TOOL_NAMES: [&str; 9] = [
    "wardrobe_list",
    "wardrobe_read",
    "wardrobe_create",
    "wardrobe_update",
    "wardrobe_archive",
    "wardrobe_wear",
    "wardrobe_take_off",
    "wardrobe_rollback",
    "wardrobe_history",
];

/// v4 `WardrobeToolContext` (= `Omit<OutfitChangeContext, 'historyIntent'>`)
/// — the ONE context every wardrobe tool runs in. The executor builds it once
/// from the tool-execution context: the caller's `character` provenance and
/// BOTH turn containers, so every change any of the tools makes is recorded
/// as the calling character's.
#[derive(Clone, Copy, Debug)]
pub struct WardrobeToolContext<'a> {
    pub user_id: &'a str,
    pub chat_id: &'a str,
    pub character_id: &'a str,
    /// Who is changing the outfit — `character`, naming the caller.
    pub provenance: &'a WardrobeHistoryProvenance,
    /// The per-turn announcement queue the orchestrator threads through.
    pub pending_wardrobe_announcements: Option<&'a Arc<Mutex<HashSet<String>>>>,
    /// The per-turn outfit-history intents, created beside the set.
    pub pending_wardrobe_history: Option<&'a PendingWardrobeHistory>,
}

/// One tool's handler output (v4's `execute*` return value).
#[derive(Debug)]
pub enum WardrobeToolOutput {
    List(wardrobe_list::WardrobeListToolOutput),
    Read(WardrobeReadToolOutput),
    Create(wardrobe_create::WardrobeCreateToolOutput),
    Update(
        WardrobeReadToolOutput,
        Option<wardrobe_update::UpdatePictureClaim>,
    ),
    Archive(wardrobe_archive::WardrobeArchiveToolOutput),
    Wear(wardrobe_wear::WardrobeWearToolOutput),
    TakeOff(wardrobe_take_off::WardrobeTakeOffToolOutput),
    Rollback(wardrobe_rollback::WardrobeRollbackToolOutput),
    History(wardrobe_history::WardrobeHistoryToolOutput),
}

/// What [`run_wardrobe_tool_handler`] hands out of the write.
#[derive(Debug)]
pub struct WardrobeToolStage {
    pub output: WardrobeToolOutput,
    /// The characters whose outfit the tool changed (v4's
    /// `notifyWardrobeChanged` targets) — at most one per call — each with its
    /// history intent: `None` is the context provenance's `append` (every
    /// tool but the rollback), `Some(Cursor)` the rollback's cursor move. A
    /// refusal or a no-op pushes nothing, so the effects fire once per
    /// gesture and only when something landed.
    pub changed: Vec<(String, Option<WardrobeHistoryIntent>)>,
}

/// v4 `WardrobeToolRun` — the executor's row for one call.
#[derive(Debug, Clone, PartialEq)]
pub struct WardrobeToolRun {
    pub success: bool,
    /// `null` on a failure, except wear / take-off, which always answer one
    /// ("a partial run still changed slots").
    pub result: Value,
    pub error: Option<String>,
}

/// The handler's `context` — v4's `sourceContext` for the effects and the
/// picture's `callerContext`.
pub fn source_context(name: &str) -> Option<&'static str> {
    Some(match name {
        "wardrobe_list" => "wardrobe-list-handler",
        "wardrobe_read" => "wardrobe-read-handler",
        "wardrobe_create" => "wardrobe-create-handler",
        "wardrobe_update" => "wardrobe-update-handler",
        "wardrobe_archive" => "wardrobe-archive-handler",
        "wardrobe_wear" => "wardrobe-wear-handler",
        "wardrobe_take_off" => "wardrobe-take-off-handler",
        "wardrobe_rollback" => wardrobe_rollback::CONTEXT,
        "wardrobe_history" => wardrobe_history::CONTEXT,
        _ => return None,
    })
}

/// The handler half — runs inside the write with both connections. `None` for
/// a name the table does not hold.
pub fn run_wardrobe_tool_handler(
    name: &str,
    main: &Connection,
    mount: &Connection,
    user_id: &str,
    chat_id: &str,
    character_id: &str,
    args: &Value,
) -> Option<WardrobeToolStage> {
    let (output, changed) = match name {
        "wardrobe_list" => (
            WardrobeToolOutput::List(wardrobe_list::execute(
                main,
                mount,
                user_id,
                chat_id,
                character_id,
                args,
            )),
            Vec::new(),
        ),
        "wardrobe_read" => (
            WardrobeToolOutput::Read(wardrobe_read::execute(
                main,
                mount,
                user_id,
                chat_id,
                character_id,
                args,
            )),
            Vec::new(),
        ),
        "wardrobe_create" => {
            let out = wardrobe_create::execute(main, mount, user_id, chat_id, character_id, args);
            // `equip_now` changed the TARGET's outfit (bug 193).
            let changed = out
                .target_character_id
                .clone()
                .filter(|_| out.success)
                .into_iter()
                .collect();
            (WardrobeToolOutput::Create(out), changed)
        }
        "wardrobe_update" => {
            let (out, claim) = wardrobe_update::execute_with_picture_claim(
                main,
                mount,
                user_id,
                chat_id,
                character_id,
                args,
            );
            (WardrobeToolOutput::Update(out, claim), Vec::new())
        }
        "wardrobe_archive" => {
            let (out, changed) =
                wardrobe_archive::execute(main, mount, user_id, chat_id, character_id, args);
            (WardrobeToolOutput::Archive(out), changed)
        }
        "wardrobe_wear" => {
            let (out, changed) =
                wardrobe_wear::execute(main, mount, user_id, chat_id, character_id, args);
            (WardrobeToolOutput::Wear(out), changed)
        }
        "wardrobe_take_off" => {
            let (out, changed) =
                wardrobe_take_off::execute(main, mount, user_id, chat_id, character_id, args);
            (WardrobeToolOutput::TakeOff(out), changed)
        }
        // The rollback's ONE change carries its CURSOR intent; a refusal or a
        // failure changes nothing.
        "wardrobe_rollback" => {
            let (out, stage) = wardrobe_rollback::execute(main, mount, chat_id, character_id, args);
            let changed = stage
                .map(|stage| vec![(character_id.to_string(), Some(stage.intent))])
                .unwrap_or_default();
            return Some(WardrobeToolStage {
                output: WardrobeToolOutput::Rollback(out),
                changed,
            });
        }
        // A history call changes nothing.
        "wardrobe_history" => (
            WardrobeToolOutput::History(wardrobe_history::execute(
                main,
                mount,
                chat_id,
                character_id,
                args,
            )),
            Vec::new(),
        ),
        _ => return None,
    };
    let changed = changed.into_iter().map(|id: String| (id, None)).collect();
    Some(WardrobeToolStage { output, changed })
}

/// The after-commit half: the outfit-change effects for every changed
/// character (the ONE context's announcement set), then the picture and the
/// `… completed` INFO for `wardrobe_create` / `wardrobe_update`.
pub async fn finish_wardrobe_tool(
    db: &Db,
    ctx: &WardrobeToolContext<'_>,
    stage: WardrobeToolStage,
) -> WardrobeToolOutput {
    let WardrobeToolStage {
        mut output,
        changed,
    } = stage;
    let source = match &output {
        WardrobeToolOutput::List(_) => "wardrobe-list-handler",
        WardrobeToolOutput::Read(_) => "wardrobe-read-handler",
        WardrobeToolOutput::Create(_) => "wardrobe-create-handler",
        WardrobeToolOutput::Update(..) => "wardrobe-update-handler",
        WardrobeToolOutput::Archive(_) => "wardrobe-archive-handler",
        WardrobeToolOutput::Wear(_) => "wardrobe-wear-handler",
        WardrobeToolOutput::TakeOff(_) => "wardrobe-take-off-handler",
        WardrobeToolOutput::Rollback(_) => wardrobe_rollback::CONTEXT,
        WardrobeToolOutput::History(_) => wardrobe_history::CONTEXT,
    };
    for (character_id, intent) in &changed {
        notify_wardrobe_changed(
            db,
            &OutfitChangeContext {
                user_id: ctx.user_id,
                chat_id: ctx.chat_id,
                character_id,
                provenance: ctx.provenance,
                history_intent: intent.as_ref(),
                pending_wardrobe_announcements: ctx.pending_wardrobe_announcements,
                pending_wardrobe_history: ctx.pending_wardrobe_history,
            },
            source,
        )
        .await;
    }
    // v4 logs `Rolled a wardrobe back` AFTER `notifyWardrobeChanged` — here,
    // once the effects above ran (the rollback module's one line).
    if let (WardrobeToolOutput::Rollback(out), Some((character_id, _))) = (&output, changed.first())
    {
        crate::wardrobe::rollback::log_rolled_back(
            source,
            &OutfitChangeContext {
                user_id: ctx.user_id,
                chat_id: ctx.chat_id,
                character_id,
                provenance: ctx.provenance,
                history_intent: None,
                pending_wardrobe_announcements: ctx.pending_wardrobe_announcements,
                pending_wardrobe_history: ctx.pending_wardrobe_history,
            },
            out.previous_index.unwrap_or_default(),
            out.restored_index.unwrap_or_default(),
            out.skipped.as_ref().map_or(0, Vec::len),
        );
    }
    match &mut output {
        // v4 `b3f937076`: a new garment is drawn by default when the operator
        // allows tool pictures — for the RECIPIENT of a gift, equipped or not.
        WardrobeToolOutput::Create(out) => {
            if out.success {
                if let Some(recipient) = out.picture_character_id.clone() {
                    out.image_generation = maybe_queue_wardrobe_tool_image(
                        db,
                        QueueWardrobeToolImageArgs {
                            user_id: ctx.user_id.to_string(),
                            chat_id: ctx.chat_id.to_string(),
                            character_id: recipient,
                            item_id: out.item_id.clone(),
                            requested: out.generate_image,
                            default_when_enabled: true,
                            caller_context: source,
                        },
                    )
                    .await;
                }
            }
            wardrobe_create::log_create_completed(ctx.user_id, ctx.chat_id, ctx.character_id, out);
        }
        // An edit redraws by default only when it changes how the item looks.
        WardrobeToolOutput::Update(out, Some(claim)) => {
            out.image_generation = maybe_queue_wardrobe_tool_image(
                db,
                QueueWardrobeToolImageArgs {
                    user_id: ctx.user_id.to_string(),
                    chat_id: ctx.chat_id.to_string(),
                    character_id: ctx.character_id.to_string(),
                    item_id: claim.item_id.clone(),
                    requested: claim.requested,
                    default_when_enabled: claim.changes_look,
                    caller_context: source,
                },
            )
            .await;
            wardrobe_update::log_update_completed(
                ctx.user_id,
                ctx.chat_id,
                ctx.character_id,
                claim,
                out.image_generation.as_ref(),
            );
        }
        _ => {}
    }
    output
}

/// `{ formattedText, ...out }` (v4's spread — `formattedText` first).
fn spread(formatted: String, out: &impl serde::Serialize) -> Value {
    let mut result = Map::new();
    result.insert("formattedText".into(), Value::String(formatted));
    if let Ok(Value::Object(obj)) = serde_json::to_value(out) {
        result.extend(obj);
    }
    Value::Object(result)
}

impl WardrobeToolOutput {
    /// The handler's output as v4's `execute*` returns it.
    pub fn to_value(&self) -> Value {
        let v = match self {
            WardrobeToolOutput::List(o) => serde_json::to_value(o),
            WardrobeToolOutput::Read(o) | WardrobeToolOutput::Update(o, _) => {
                serde_json::to_value(o)
            }
            WardrobeToolOutput::Create(o) => serde_json::to_value(o),
            WardrobeToolOutput::Archive(o) => serde_json::to_value(o),
            WardrobeToolOutput::Wear(o) => serde_json::to_value(o),
            WardrobeToolOutput::TakeOff(o) => serde_json::to_value(o),
            WardrobeToolOutput::Rollback(o) => serde_json::to_value(o),
            WardrobeToolOutput::History(o) => serde_json::to_value(o),
        };
        v.unwrap_or(Value::Null)
    }

    /// v4's `format*Results(output, nowMs)` — list and read take the clock.
    pub fn format_at(&self, now_ms: f64) -> String {
        match self {
            WardrobeToolOutput::List(o) => wardrobe_list::format_at(o, now_ms),
            WardrobeToolOutput::Read(o) => wardrobe_read::format_at(o, now_ms),
            WardrobeToolOutput::Create(o) => wardrobe_create::format(o),
            WardrobeToolOutput::Update(o, _) => wardrobe_update::format(o),
            WardrobeToolOutput::Archive(o) => wardrobe_archive::format(o),
            WardrobeToolOutput::Wear(o) => wardrobe_wear::format(o),
            WardrobeToolOutput::TakeOff(o) => wardrobe_take_off::format(o),
            WardrobeToolOutput::Rollback(o) => wardrobe_rollback::format(o),
            WardrobeToolOutput::History(o) => wardrobe_history::format(o),
        }
    }

    /// [`Self::format_at`] on the production clock.
    pub fn format(&self) -> String {
        self.format_at(crate::clock::now_unix_ms() as f64)
    }

    /// v4's `WARDROBE_TOOL_TABLE` payload projection over `formatted`.
    /// `already_archived` rides the formatted text ONLY (R-C) — the archive
    /// row names `item_id`, `title`, `action`.
    pub fn run(&self, formatted: String) -> WardrobeToolRun {
        let row = |success: bool, result: Value, error: &Option<String>| WardrobeToolRun {
            success,
            result: if success { result } else { Value::Null },
            error: if success { None } else { error.clone() },
        };
        match self {
            WardrobeToolOutput::List(o) => row(
                o.success,
                json!({ "formattedText": formatted, "items": o.items, "total_count": o.total_count }),
                &o.error,
            ),
            WardrobeToolOutput::Read(o) | WardrobeToolOutput::Update(o, _) => {
                row(o.success, spread(formatted, o), &o.error)
            }
            WardrobeToolOutput::Create(o) => {
                // `undefined` values → keys absent.
                let mut result = Map::new();
                result.insert("formattedText".into(), json!(formatted));
                result.insert("item_id".into(), json!(o.item_id));
                result.insert("title".into(), json!(o.title));
                result.insert("equipped".into(), json!(o.equipped));
                if let Some(name) = &o.recipient_name {
                    result.insert("recipient_name".into(), json!(name));
                }
                if let Some(state) = &o.current_state {
                    result.insert("current_state".into(), state.clone());
                }
                row(o.success, Value::Object(result), &o.error)
            }
            WardrobeToolOutput::Archive(o) => row(
                o.success,
                json!({
                    "formattedText": formatted,
                    "item_id": o.item_id,
                    "title": o.title,
                    "action": o.action,
                }),
                &o.error,
            ),
            // Wear and take-off always return a result: a partial run still
            // changed slots.
            WardrobeToolOutput::Wear(o) => WardrobeToolRun {
                success: o.success,
                result: json!({
                    "formattedText": formatted,
                    "operations": o.operations,
                    "current_state": o.current_state,
                    "coverage_summary": o.coverage_summary,
                }),
                error: if o.success { None } else { o.error.clone() },
            },
            WardrobeToolOutput::TakeOff(o) => WardrobeToolRun {
                success: o.success,
                result: json!({
                    "formattedText": formatted,
                    "operations": o.operations,
                    "current_state": o.current_state,
                    "coverage_summary": o.coverage_summary,
                }),
                error: if o.success { None } else { o.error.clone() },
            },
            // v4: `{ formattedText, previous_index, restored_index,
            // current_state, coverage_summary, skipped }`, in that order.
            WardrobeToolOutput::Rollback(o) => row(
                o.success,
                json!({
                    "formattedText": formatted,
                    "previous_index": o.previous_index,
                    "restored_index": o.restored_index,
                    "current_state": o.current_state,
                    "coverage_summary": o.coverage_summary,
                    "skipped": o.skipped,
                }),
                &o.error,
            ),
            // v4's spread: `success` rides INSIDE the result, as `wardrobe_read`.
            WardrobeToolOutput::History(o) => row(o.success, spread(formatted, o), &o.error),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // The 32-byte all-`testpepper` base64 used across the core write tests.
    const TEST_PEPPER: &str = "dGVzdHBlcHBlcnRlc3RwZXBwZXJ0ZXN0cGVwcGVyMDE=";

    /// v4 `wardrobe-tool-table.test.ts` — "covers exactly the nine
    /// wardrobe_* tools": the table answers each of the nine and nothing
    /// else, each with its handler's `context`.
    #[test]
    fn covers_exactly_the_nine_wardrobe_tools() {
        let (main, mount) = (
            Connection::open_in_memory().unwrap(),
            Connection::open_in_memory().unwrap(),
        );
        for name in WARDROBE_TOOL_NAMES {
            assert!(
                run_wardrobe_tool_handler(name, &main, &mount, "u", "c", "ch", &json!(null))
                    .is_some(),
                "{name}"
            );
            assert_eq!(
                source_context(name),
                Some(format!("{}-handler", name.replace('_', "-")).as_str())
            );
        }
        for other in ["wardrobe_dance", "wardrobe", "list_wardrobe", ""] {
            assert!(
                run_wardrobe_tool_handler(other, &main, &mount, "u", "c", "ch", &json!({}))
                    .is_none()
            );
            assert_eq!(source_context(other), None);
        }
    }

    /// v4 `wardrobe-tool-table.test.ts` — `it.each` "%s forwards the turn's
    /// announcement set to its handler (bug 193)": whichever of the seven
    /// changed an outfit, the character lands in the ONE set the context
    /// carries (the avatar refresh finds no chat here and stands aside).
    #[tokio::test]
    async fn every_tool_forwards_the_turns_announcement_set() {
        let dir = tempfile::Builder::new()
            .prefix("qt-wardrobe-tool-table-")
            .tempdir()
            .unwrap();
        let db = Db::open_main(dir.path().join("main.db"), TEST_PEPPER).unwrap();
        let (main, mount) = (
            Connection::open_in_memory().unwrap(),
            Connection::open_in_memory().unwrap(),
        );
        let provenance = WardrobeHistoryProvenance::new(
            crate::db::chat_wardrobe_history::WardrobeHistorySource::Character,
        )
        .with_actor("ch");
        for name in WARDROBE_TOOL_NAMES {
            let (pending, history, _) =
                crate::services::outfit_change_effects::new_wardrobe_turn_containers();
            // A refused input — no write, no picture; the stage then reports
            // an outfit change for `char-x`, as a landed call would.
            let mut stage =
                run_wardrobe_tool_handler(name, &main, &mount, "u", "c", "ch", &json!(null))
                    .unwrap();
            stage.changed = vec![(format!("char-{name}"), None)];
            let ctx = WardrobeToolContext {
                user_id: "u",
                chat_id: "c",
                character_id: "ch",
                provenance: &provenance,
                pending_wardrobe_announcements: Some(&pending),
                pending_wardrobe_history: Some(&history),
            };
            finish_wardrobe_tool(&db, &ctx, stage).await;
            let set = pending.lock().unwrap();
            assert_eq!(
                set.iter().cloned().collect::<Vec<_>>(),
                vec![format!("char-{name}")],
                "{name} must forward the turn's set"
            );
            // v4's `it.each` asserts the Map's identity and the provenance:
            // the intent lands in the SAME map, as the caller's append.
            let map = history.lock().unwrap();
            assert_eq!(map.len(), 1, "{name} must forward the turn's map");
            assert_eq!(
                map.get(&format!("char-{name}")),
                Some(&WardrobeHistoryIntent::Append {
                    provenance: provenance.clone()
                }),
                "{name} records the caller's provenance"
            );
        }
    }

    /// R-C: `already_archived` rides the formatted text ONLY — the archive
    /// row's payload names `item_id`, `title`, `action` and nothing else.
    #[test]
    fn already_archived_never_reaches_the_payload() {
        let out = WardrobeToolOutput::Archive(wardrobe_archive::WardrobeArchiveToolOutput {
            success: true,
            item_id: "i1".into(),
            title: "Red Hat".into(),
            action: "archived".into(),
            already_archived: Some(true),
            error: None,
        });
        let formatted = out.format();
        assert_eq!(
            formatted,
            "\"Red Hat\" (i1) was already archived; nothing changed."
        );
        assert!(out.to_value().get("already_archived").is_some());
        let run = out.run(formatted);
        assert!(run.success);
        assert_eq!(
            serde_json::to_string(&run.result).unwrap(),
            r#"{"formattedText":"\"Red Hat\" (i1) was already archived; nothing changed.","item_id":"i1","title":"Red Hat","action":"archived"}"#
        );
    }
}
