//! `wardrobe_wear` handler (v4 `lib/tools/handlers/wardrobe-wear-handler.ts`
//! `executeWardrobeWearTool` + `formatWardrobeWearResults`).
//!
//! Applies an ordered array of put-on operations in sequence (each builds on the
//! last), over the tool call's ONE wearable pool (v4 `9ecebf986`): each op
//! resolves through `wear_ops::resolve_wearable` (not found / archived / the
//! slot sentence) and lands through `wear_ops::wear_item` — one displacement,
//! committed through the wear ledger's chokepoint. Fails fast on the first bad
//! op. The outfit-change effects (v4 `notifyWardrobeChanged`) fire ONCE when at
//! least one op landed: the returned ids are the characters whose outfit
//! changed, fired by the executor once the write commits.

use rusqlite::Connection;
use serde::Serialize;
use serde_json::Value;

use crate::db::wardrobe_wear_stats::EquipSource;
use crate::db::DbError;
use crate::wardrobe::{describe_wardrobe_effect, normalize_no_item_sentinel, WardrobeEffect};
use crate::wear_ops::{
    resolve_wearable, wear_item, PutOnMode, WearEffect, WearableRef, WearableResolution,
};

use super::wardrobe_shared::{empty_equipped_state, load_current_wardrobe_state};
use super::wardrobe_tool_table::load_tool_pool;

/// The item acted on in an op result (`{ item_id, title } | null`).
#[derive(Debug, Serialize)]
pub struct OpItem {
    pub item_id: String,
    pub title: String,
}

/// v4 `WardrobeWearOpResult`.
#[derive(Debug, Serialize)]
pub struct WardrobeWearOpResult {
    pub mode: String,
    pub effect: String,
    pub effect_summary: String,
    pub item: Option<OpItem>,
    pub slots_affected: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

/// v4 `WardrobeWearToolOutput`.
#[derive(Debug, Serialize)]
pub struct WardrobeWearToolOutput {
    pub success: bool,
    pub operations: Vec<WardrobeWearOpResult>,
    pub current_state: Value,
    pub coverage_summary: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

fn failure(error: impl Into<String>) -> WardrobeWearToolOutput {
    WardrobeWearToolOutput {
        success: false,
        operations: Vec::new(),
        current_state: empty_equipped_state().to_value(),
        coverage_summary: String::new(),
        error: Some(error.into()),
    }
}

const VALIDATION_ERROR: &str = "Invalid input: provide a non-empty \"operations\" array. Each operation needs an item_id or item_title; mode=add_to_slot also needs a slot.";
const MODE_ENUM: [&str; 3] = ["wear", "replace", "add_to_slot"];
const SLOT_ENUM: [&str; 5] = crate::wardrobe::WARDROBE_SLOT_TYPES;

/// One validated wear op.
struct WearOp {
    item_id: Option<String>,
    item_title: Option<String>,
    mode: Option<String>,
    slot: Option<String>,
}

/// v4 `validateWardrobeWearInput` — a non-empty `operations` array whose elements
/// are typed, with the superRefine: `add_to_slot` requires a slot; every op
/// requires `item_id` or `item_title`.
fn validate(args: &Value) -> Option<Vec<WearOp>> {
    let obj = args.as_object()?;
    let ops_val = obj.get("operations")?.as_array()?;
    if ops_val.is_empty() {
        return None; // `.min(1)`
    }
    let mut ops = Vec::with_capacity(ops_val.len());
    for op in ops_val {
        let o = op.as_object()?;
        let item_id = opt_str_field(o, "item_id")?;
        let item_title = opt_str_field(o, "item_title")?;
        let mode = opt_enum_field(o, "mode", &MODE_ENUM)?;
        let slot = opt_enum_field(o, "slot", &SLOT_ENUM)?;
        // superRefine.
        let effective_mode = mode.as_deref().unwrap_or("wear");
        if effective_mode == "add_to_slot" && slot.is_none() {
            return None;
        }
        if item_id.is_none() && item_title.is_none() {
            return None;
        }
        ops.push(WearOp {
            item_id,
            item_title,
            mode,
            slot,
        });
    }
    Some(ops)
}

/// `Ok(None)` = absent; `Ok(Some)` = a valid string; `None` (outer) = wrong type.
fn opt_str_field(o: &serde_json::Map<String, Value>, key: &str) -> Option<Option<String>> {
    match o.get(key) {
        None | Some(Value::Null) => Some(None),
        Some(Value::String(s)) => Some(Some(s.clone())),
        Some(_) => None,
    }
}

fn opt_enum_field(
    o: &serde_json::Map<String, Value>,
    key: &str,
    allowed: &[&str],
) -> Option<Option<String>> {
    match o.get(key) {
        None | Some(Value::Null) => Some(None),
        Some(Value::String(s)) if allowed.contains(&s.as_str()) => Some(Some(s.clone())),
        Some(_) => None,
    }
}

/// v4's `context` on every line the handler logs.
const LOG_CONTEXT: &str = "wardrobe-wear-handler";

/// Execute `wardrobe_wear` (v4 `executeWardrobeWearTool`). Returns the output plus
/// the characters whose outfit changed (non-empty when at least one op landed).
pub fn execute(
    main: &Connection,
    mount: &Connection,
    user_id: &str,
    chat_id: &str,
    character_id: &str,
    args: &Value,
) -> (WardrobeWearToolOutput, Vec<String>) {
    let Some(ops) = validate(args) else {
        tracing::warn!(
            target: "quilltap::tools",
            context = LOG_CONTEXT,
            userId = user_id,
            chatId = chat_id,
            characterId = character_id,
            inputJson = %args,
            "Wardrobe wear tool validation failed",
        );
        return (failure(VALIDATION_ERROR), Vec::new());
    };
    // v4 has no outer catch here: the per-op catch takes every op failure, and
    // only the closing state read can throw past it.
    match run(main, mount, user_id, chat_id, character_id, &ops) {
        Ok(pair) => pair,
        Err(e) => (failure(crate::db::fallback::error_text(&e)), Vec::new()),
    }
}

fn run(
    main: &Connection,
    mount: &Connection,
    user_id: &str,
    chat_id: &str,
    character_id: &str,
    ops: &[WearOp],
) -> Result<(WardrobeWearToolOutput, Vec<String>), DbError> {
    let pool = load_tool_pool(main, mount, chat_id, character_id);

    let mut results: Vec<WardrobeWearOpResult> = Vec::new();
    let mut applied_count = 0usize;
    let mut failed_error: Option<String> = None;

    for op in ops {
        let mode_str = op.mode.clone().unwrap_or_else(|| "wear".to_string());
        // The validator admits only the three put-on modes.
        let mode = PutOnMode::parse(&mode_str).unwrap_or(PutOnMode::Wear);
        let item_id = normalize_no_item_sentinel(op.item_id.as_deref());
        let item_title = normalize_no_item_sentinel(op.item_title.as_deref());

        // v4's per-op `try` — a refusal or a failed write is this op's error.
        let attempt: Result<WardrobeWearOpResult, String> = (|| {
            let item = match resolve_wearable(
                &pool,
                WearableRef {
                    item_id: item_id.as_deref(),
                    item_title: item_title.as_deref(),
                },
                mode,
                op.slot.as_deref(),
            ) {
                WearableResolution::Ok(item) => item,
                WearableResolution::Refused { message, .. } => return Err(message),
            };
            let outcome = wear_item(
                main,
                chat_id,
                &pool,
                item,
                mode,
                op.slot.as_deref(),
                EquipSource::Tool,
            )
            .map_err(|e| crate::db::fallback::error_text(&e))?;
            let effect = match outcome.effect {
                WearEffect::Replaced => WardrobeEffect::Replaced,
                WearEffect::Layered => WardrobeEffect::Layered,
            };
            let id = item
                .get("id")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string();
            let title = item
                .get("title")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string();
            tracing::info!(
                target: "quilltap::tools",
                context = LOG_CONTEXT,
                userId = user_id,
                chatId = chat_id,
                characterId = character_id,
                itemId = %id,
                itemTitle = %title,
                mode = %mode_str,
                effect = effect.as_str(),
                slotsAffectedJson = %serde_json::to_string(&outcome.slots_affected).unwrap_or_default(),
                "Wardrobe item worn",
            );
            Ok(WardrobeWearOpResult {
                mode: mode_str.clone(),
                effect: effect.as_str().to_string(),
                effect_summary: describe_wardrobe_effect(
                    effect,
                    &outcome.slots_affected,
                    Some(title.as_str()),
                ),
                item: Some(OpItem { item_id: id, title }),
                slots_affected: outcome.slots_affected,
                error: None,
            })
        })();

        match attempt {
            Ok(res) => {
                results.push(res);
                applied_count += 1;
            }
            Err(message) => {
                tracing::warn!(
                    target: "quilltap::tools",
                    context = LOG_CONTEXT,
                    userId = user_id,
                    chatId = chat_id,
                    characterId = character_id,
                    mode = %mode_str,
                    // v4's `message` key, renamed (a tracing `message` field
                    // replaces the file record's envelope text — recorded).
                    errorMessage = %message,
                    "Wardrobe wear operation failed",
                );
                results.push(WardrobeWearOpResult {
                    mode: mode_str,
                    effect: "layered".to_string(),
                    effect_summary: String::new(),
                    item: None,
                    slots_affected: Vec::new(),
                    error: Some(message.clone()),
                });
                failed_error = Some(message);
                break; // fail-fast
            }
        }
    }

    // v4 `finalizeWardrobeMutation`: the effects fire ONCE, only if at least
    // one op landed (the executor fires them once the write commits).
    let changed = if applied_count > 0 {
        vec![character_id.to_string()]
    } else {
        Vec::new()
    };

    let current_state = load_current_wardrobe_state(main, chat_id, character_id)?;
    // v4 `buildWardrobeCoverageSummaryFromState(pool, slots)` =
    // `describeOutfit(resolveEquippedOutfitForCharacter(pool, slots).outfitValues)`
    // — pure over the pool the tool already loaded (no second read).
    let coverage_summary = crate::wardrobe::describe_outfit(
        &crate::services::resolve_equipped::resolve_equipped_outfit_for_character(
            &pool,
            &current_state,
        )
        .outfit_values,
    );

    Ok((
        WardrobeWearToolOutput {
            success: failed_error.is_none(),
            operations: results,
            current_state: current_state.to_value(),
            coverage_summary,
            error: failed_error,
        },
        changed,
    ))
}

/// v4 `formatWardrobeWearResults`.
pub fn format(output: &WardrobeWearToolOutput) -> String {
    if !output.success && output.operations.is_empty() {
        return format!(
            "Wardrobe Error: {}",
            output.error.as_deref().unwrap_or("Unknown error")
        );
    }
    let mut lines: Vec<String> = Vec::new();
    for op in &output.operations {
        if let Some(err) = &op.error {
            lines.push(format!("Failed: {err}"));
        } else if !op.effect_summary.is_empty() {
            lines.push(op.effect_summary.clone());
        }
    }
    lines.push(String::new());
    lines.push("Current outfit:".to_string());
    for slot in crate::wardrobe::WARDROBE_SLOT_TYPES {
        let ids = output
            .current_state
            .get(slot)
            .and_then(Value::as_array)
            .map(|a| {
                a.iter()
                    .filter_map(|e| e.as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            })
            .unwrap_or_default();
        // An unreported-if-blank slot (hair) is omitted entirely when empty
        // rather than listed as "(empty)" — the model must never read that as
        // baldness.
        if ids.is_empty() && !crate::wardrobe::is_slot_reported_when_empty(slot) {
            continue;
        }
        let label = if ids.is_empty() {
            "(empty)".to_string()
        } else {
            ids
        };
        lines.push(format!("  {slot}: {label}"));
    }
    lines.push(String::new());
    lines.push(format!("Summary: {}", output.coverage_summary));
    lines.join("\n")
}

/// P4.D258 (R-D, R-F) — **v4 bug 179 cannot arise in v5**, measured.
///
/// v4 `039f7017c` fixed a forked-child artifact: a background job's wardrobe
/// tools wrote equipped slots through the child's BUFFERED proxy
/// (`commitEquippedOutfit` replayed by the parent after the job), while the
/// child's own `getEquippedOutfitForCharacter` still read the parent's
/// pre-job row — so a second outfit change in one job was built on the stale
/// slots and overwrote the first. The fix is a per-job overlay of buffered
/// slots inside `child-repositories-proxy.ts`. v5 has no child process, no
/// buffered-write proxy and no overlay (`services/job_runner.rs`'s header):
/// the wardrobe tools read-modify-write the equipped slots on the WRITER's
/// connections inside one `Db::write` (`tools/executor.rs`'s
/// `wardrobe_write`), each op reading what the previous op committed.
///
/// These pins are the shape of v4's jest (`child-proxy-wardrobe-wear.test.ts
/// :94-189`): two garments in one call both stay worn; a later op in the same
/// call builds on the earlier one; and a take-off after a put-on, on the same
/// connections, sees the put-on.
#[cfg(test)]
mod bug_179_no_port {
    use super::execute;
    use crate::api::types::Response;
    use crate::db::chats_outfits::ChatOutfitsRepository;
    use crate::db::runtime::{Db, DbPaths};
    use crate::tools::wardrobe_take_off;
    use serde_json::{json, Value};

    const PEPPER: &str = "cXVpbGx0YXAtdGVzdC1wZXBwZXItMzItYnl0ZXMhIQ==";
    const USER: &str = "u-179";
    const CHAT: &str = "c1790000-0000-4000-8000-000000000001";
    const CHARACTER: &str = "a1790000-0000-4000-8000-000000000001";

    /// The plant: a provisioned temp instance, four Quilltap General garments
    /// created through the General wardrobe's own create route (two tops, a
    /// bottom, footwear) and one chat. Returns the `Db` and the four item ids
    /// (shirt, trousers, boots, waistcoat).
    async fn instance(dir: &std::path::Path) -> (Db, [String; 4]) {
        let path = dir.to_path_buf();
        let db = tokio::task::spawn_blocking(move || {
            crate::services::provisioning::provision_fresh_instance(&path, PEPPER).unwrap();
            Db::open(
                DbPaths {
                    main: path.join("quilltap.db"),
                    mount_index: Some(path.join("quilltap-mount-index.db")),
                    llm_logs: None,
                },
                PEPPER,
            )
            .unwrap()
        })
        .await
        .unwrap();
        let mut ids = Vec::new();
        for (title, slot) in [
            ("Linen Shirt", "top"),
            ("Wool Trousers", "bottom"),
            ("Riding Boots", "footwear"),
            ("Brocade Waistcoat", "top"),
        ] {
            let created = crate::api::wardrobe::wardrobe_create(
                &db,
                json!({ "title": title, "types": [slot] }),
            )
            .await;
            let Response::Wardrobe(body) = created else {
                panic!("the General create failed: {created:?}");
            };
            ids.push(body["wardrobeItem"]["id"].as_str().unwrap().to_string());
        }
        db.write(|w| {
            w.main()
                .connection()
                .execute(
                    "INSERT INTO chats (id, userId, title, createdAt, updatedAt) \
                     VALUES (?1, ?2, 'A Change of Clothes', ?3, ?3)",
                    rusqlite::params![CHAT, USER, "2026-10-08T00:00:00.000Z"],
                )
                .map(|_| ())
                .map_err(Into::into)
        })
        .await
        .unwrap();
        (db, ids.try_into().unwrap())
    }

    async fn worn(db: &Db) -> Value {
        db.write(|w| {
            Ok(ChatOutfitsRepository::new(w.main().connection())
                .get_equipped_outfit_for_character(CHAT, CHARACTER)
                .unwrap_or(Value::Null))
        })
        .await
        .unwrap()
    }

    fn slot(slots: &Value, name: &str) -> Vec<String> {
        slots[name]
            .as_array()
            .map(|a| {
                a.iter()
                    .filter_map(|v| v.as_str().map(str::to_string))
                    .collect()
            })
            .unwrap_or_default()
    }

    /// ONE `wardrobe_wear` call, inside ONE `Db::write` on the writer's main
    /// and mount connections — the executor's `wardrobe_write` shape.
    async fn wear(db: &Db, ops: Value) {
        let (out, announce) = db
            .write(move |w| {
                let mount = w.mount_index().expect("mount index").connection();
                Ok(execute(
                    w.main().connection(),
                    mount,
                    USER,
                    CHAT,
                    CHARACTER,
                    &json!({ "operations": ops }),
                ))
            })
            .await
            .unwrap();
        assert!(out.success, "{:?}", out.error);
        assert!(out.operations.iter().all(|op| op.error.is_none()));
        assert_eq!(announce, vec![CHARACTER.to_string()]);
    }

    #[tokio::test]
    async fn two_outfit_changes_in_one_call_compound() {
        let dir = tempfile::tempdir().unwrap();
        let (db, [shirt, trousers, boots, waistcoat]) = instance(dir.path()).await;
        // v4's bug: the job's second op read the pre-job slots and dropped
        // the first garment. Here the second op reads the first op's write.
        wear(&db, json!([{ "item_id": shirt }, { "item_id": trousers }])).await;
        let slots = worn(&db).await;
        assert_eq!(slot(&slots, "top"), vec![shirt.clone()]);
        assert_eq!(slot(&slots, "bottom"), vec![trousers.clone()]);

        // A later call builds on the stored slots, and its second op on its
        // first: the boots go on, then the waistcoat REPLACES the top — the
        // trousers and the boots must both survive it.
        wear(
            &db,
            json!([
                { "item_id": boots },
                { "item_id": waistcoat, "mode": "replace" },
            ]),
        )
        .await;
        let slots = worn(&db).await;
        assert_eq!(slot(&slots, "top"), vec![waistcoat]);
        assert_eq!(slot(&slots, "bottom"), vec![trousers]);
        assert_eq!(slot(&slots, "footwear"), vec![boots]);
    }

    #[tokio::test]
    async fn a_take_off_after_a_put_on_sees_the_put_on() {
        let dir = tempfile::tempdir().unwrap();
        let (db, [shirt, ..]) = instance(dir.path()).await;
        // One turn's two tool calls, each its own `Db::write` — as the
        // executor runs them.
        wear(&db, json!([{ "item_id": shirt }])).await;
        assert_eq!(slot(&worn(&db).await, "top"), vec![shirt.clone()]);
        let out = db
            .write(move |w| {
                let mount = w.mount_index().expect("mount index").connection();
                Ok(wardrobe_take_off::execute(
                    w.main().connection(),
                    mount,
                    USER,
                    CHAT,
                    CHARACTER,
                    &json!({ "operations": [{ "item_id": shirt }] }),
                )
                .0)
            })
            .await
            .unwrap();
        assert!(out.success, "{:?}", out.error);
        assert!(
            slot(&worn(&db).await, "top").is_empty(),
            "the take-off missed the put-on"
        );
    }
}
