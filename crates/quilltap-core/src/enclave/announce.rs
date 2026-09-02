//! Autonomous-room announcements + the run-start row contract (U4.3) — v4
//! `lib/background-jobs/handlers/autonomous-room-announce.ts` (whole file).
//!
//! Shared between the run-start entry points (manual start, the scheduled tick,
//! and the turn handler's defensive idle→running fallback — U4.4) and the turn
//! handler's lifecycle announcements. Centralising the run-start row patch and
//! the "run begun" banner here keeps the contract single-sourced: a run flips
//! straight to `running` the instant it is requested, so the Salon header badges
//! reflect the live status immediately instead of waiting for the first turn job
//! to flip `idle → running`.
//!
//! The announcement writer follows the [`crate::services::carina_runner`] writer
//! / W4.6b persona-writer idiom: build the exact v4 `MessageEvent` object literal
//! as JSON, validate it into a [`ChatEventInput`], and post it through the ported
//! [`crate::db::chats_messages`] `add_message` (so the stored row is byte-for-byte
//! the verified insert marshaling). The write is **error-swallowed** (v4 logs +
//! continues; a missing banner must never wedge a run that is already `running`).
//!
//! Clock + UUID minting are **injected** (`now_ms` / `mint_uuid` closures — the
//! orchestrator precedent) so the differential can drive a deterministic clock;
//! production callers pass [`system_now_ms`] / [`system_mint_uuid`].

use serde_json::{json, Value};

use crate::clock::iso_from_unix_ms;
use crate::db::chats::ChatUpdate;
use crate::db::chats_messages::ChatEventInput;
use crate::db::runtime::Db;
use crate::db::{characters_read, DbError};
use crate::participant_filters::{get_active_character_participants, ParticipantView};
// v4's `Number.prototype.toLocaleString()` grouping — the canonical port lives
// in `services::primary_stream` (its recovery text is persisted, so it is
// byte-exact-proven there); the banner reuses it.
use crate::services::primary_stream::to_locale_string;

/// An injected millisecond clock (v4 `Date.now()` / `new Date()`).
pub type NowMsFn<'a> = &'a (dyn Fn() -> i64 + Send + Sync);
/// An injected UUID mint (v4 `randomUUID()`).
pub type MintUuidFn<'a> = &'a (dyn Fn() -> String + Send + Sync);

/// Production clock: the real wall clock ([`crate::clock::now_unix_ms`]).
pub fn system_now_ms() -> i64 {
    crate::clock::now_unix_ms()
}

/// Production UUID mint: a random v4 UUID (v4 `randomUUID()`).
pub fn system_mint_uuid() -> String {
    uuid::Uuid::new_v4().to_string()
}

/// v4 `AutonomousRoomKind` — the `systemKind` vocabulary of the Host-authored
/// autonomous-room announcements.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AutonomousRoomKind {
    Start,
    End,
    Paused,
    Halfway,
    NearingEnd,
    Grace,
}

impl AutonomousRoomKind {
    pub fn as_str(self) -> &'static str {
        match self {
            AutonomousRoomKind::Start => "autonomous-room-start",
            AutonomousRoomKind::End => "autonomous-room-end",
            AutonomousRoomKind::Paused => "autonomous-room-paused",
            AutonomousRoomKind::Halfway => "autonomous-room-halfway",
            AutonomousRoomKind::NearingEnd => "autonomous-room-nearing-end",
            AutonomousRoomKind::Grace => "autonomous-room-grace",
        }
    }
}

/// v4 `runStartPatch(nowIso, runId)` — the run-state row subset written when a
/// run begins. A run goes straight to `running` with `runStartedAt` stamped and
/// every per-run counter zeroed, so the status is correct the moment the run is
/// requested rather than after the first turn job executes. Callers extend the
/// returned [`ChatUpdate`] with any schedule bookkeeping
/// (`scheduleLastRunAt` / `scheduleNextRunAt`) before writing.
///
/// `runStartedAt` anchors both the wall-clock budget and the per-run token
/// window; stamping it here means wall-clock counts from the button press
/// (including any brief job-queue wait before the first turn).
pub fn run_start_patch(now_iso: &str, run_id: &str) -> ChatUpdate {
    ChatUpdate {
        current_run_id: Some(run_id.to_string()),
        run_state: Some("running".to_string()),
        run_state_message: Some(None),
        run_started_at: Some(now_iso.to_string()),
        run_ended_at: Some(None),
        // Fresh run — drop any pause state left over from a prior run so it
        // can't bleed into this run's wall-clock accounting.
        run_paused_at: Some(None),
        run_paused_accum_ms: Some(0.0),
        run_turns_consumed: Some(0.0),
        run_tokens_consumed: Some(0.0),
        run_milestones_announced: Some(0.0),
        ..Default::default()
    }
}

/// v4 `formatNameList(names)` — `[]` → `""`, `[a]` → `a`, `[a,b]` → `a and b`,
/// `[a,b,c]` → `a, b, and c` (Oxford comma).
pub fn format_name_list(names: &[String]) -> String {
    match names {
        [] => String::new(),
        [one] => one.clone(),
        [a, b] => format!("{a} and {b}"),
        _ => {
            let head = names[..names.len() - 1].join(", ");
            format!("{head}, and {}", names[names.len() - 1])
        }
    }
}

/// JS `Math.round(x)` = `floor(x + 0.5)` (half toward +∞ — differs from Rust's
/// `f64::round` half-away-from-zero on negative `.5`s; the wall-clock cap is
/// always positive, but the port keeps the exact JS formula anyway).
fn js_math_round(x: f64) -> f64 {
    (x + 0.5).floor()
}

/// Render a JS number for template-literal interpolation (`${n}`): an
/// integer-valued double renders bare (`40.0` → `"40"`), matching JS `String(n)`
/// over this domain (the budget columns are schema-int / small reals).
fn js_number_string(n: f64) -> String {
    crate::db::js_number_to_json(n).to_string()
}

/// v4 `postAutonomousRoomAnnouncement(chatId, systemKind, content, opaqueContent)`.
///
/// Post a Host-authored `autonomous-room-*` system message. The Host owns the
/// announcement surface for autonomous rooms (start / end / paused / pacing).
///
/// `opaque_content` is the persona-free body swapped into every character's LLM
/// context when the room is opaque-anywhere. Pass it for messages the characters
/// should read and act on (the pacing nudges in particular); the human always
/// sees the Host-voiced `content` in the Salon.
///
/// Best-effort: the `addMessage` write failure is swallowed (v4 logs + returns),
/// so this never fails the caller.
pub async fn post_autonomous_room_announcement(
    db: &Db,
    now_ms: NowMsFn<'_>,
    mint_uuid: MintUuidFn<'_>,
    chat_id: &str,
    system_kind: AutonomousRoomKind,
    content: &str,
    opaque_content: Option<&str>,
) {
    // v4 object-literal evaluation order: `id: randomUUID()` before
    // `createdAt: new Date().toISOString()`.
    let message_id = mint_uuid();
    let created_at = iso_from_unix_ms(now_ms());

    let message = json!({
        "type": "message",
        "id": message_id,
        "role": "ASSISTANT",
        "content": content,
        "opaqueContent": opaque_content,
        "attachments": [],
        "createdAt": created_at,
        "participantId": Value::Null,
        "systemSender": "host",
        "systemKind": system_kind.as_str(),
    });

    let Ok(event) = serde_json::from_value::<ChatEventInput>(message) else {
        // Structurally impossible for the literal above; mirrors v4's swallow.
        return;
    };
    let chat_id = chat_id.to_string();
    // v4 try/catch around `repos.chats.addMessage` — a failure is logged and
    // swallowed (host-side logging is out of scope for the port).
    let _ = db
        .write(move |writers| writers.main().chat_messages().add_message(&chat_id, &event))
        .await;
}

/// Build a [`ParticipantView`] list from a hydrated chat's `participants` array
/// (the same shape [`crate::services::turn_orchestrator`] builds).
fn participant_views(chat: &Value) -> Vec<ParticipantView> {
    let empty = Vec::new();
    let arr = chat
        .get("participants")
        .and_then(Value::as_array)
        .unwrap_or(&empty);
    arr.iter()
        .map(|p| ParticipantView {
            id: p
                .get("id")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string(),
            status: crate::chat_predicates::participant_status_from_str(
                p.get("status").and_then(Value::as_str),
            ),
            controlled_by: p
                .get("controlledBy")
                .and_then(Value::as_str)
                .unwrap_or("llm")
                .to_string(),
            character_id: p
                .get("characterId")
                .and_then(Value::as_str)
                .filter(|c| !c.is_empty())
                .map(String::from),
        })
        .collect()
}

/// The pure content assembly of the run-start banner (v4
/// `postRunStartAnnouncement`'s cap summary + name-list prefix), split out so it
/// unit-tests without a database. `chat` is the hydrated pre-update snapshot.
fn build_run_start_content(chat: &Value, names: &[String], run_id: &str) -> String {
    let mut caps: Vec<String> = Vec::new();
    // v4 `chat.budgetMaxTurns != null` — the hydrated read omits NULL columns,
    // so key-presence is the `!= null` test.
    if let Some(n) = chat.get("budgetMaxTurns").and_then(Value::as_f64) {
        caps.push(format!("{} turn(s)", js_number_string(n)));
    }
    if let Some(n) = chat.get("budgetMaxTokens").and_then(Value::as_f64) {
        // v4 `chat.budgetMaxTokens.toLocaleString()` — the column is
        // `z.number().int().positive()`, so the integer grouper is exact; a
        // (schema-impossible) fractional value falls back to the bare JS
        // rendering.
        let grouped = if n.fract() == 0.0 {
            to_locale_string(n as i64)
        } else {
            js_number_string(n)
        };
        caps.push(format!("{grouped} token(s)"));
    }
    if let Some(n) = chat.get("budgetMaxWallClockMs").and_then(Value::as_f64) {
        caps.push(format!(
            "{} min",
            js_number_string(js_math_round(n / 60000.0))
        ));
    }
    let cap_summary = if caps.is_empty() {
        "No caps configured.".to_string()
    } else {
        format!("Caps: {}.", caps.join(", "))
    };

    let participants_summary = format_name_list(names);
    let prefix = if participants_summary.is_empty() {
        "Autonomous room run begun.".to_string()
    } else {
        format!("Autonomous room run begun with {participants_summary}.")
    };

    format!("{prefix} {cap_summary} Run id: {run_id}.")
}

/// v4 `postRunStartAnnouncement(chatId, runId, chat)` — post the "Autonomous
/// room run begun" banner: the configured caps + the participating characters'
/// names (via the ported `getActiveCharacterParticipants` filter and the
/// vault-overlaid `characters.findById`).
///
/// Failure semantics match v4 exactly: the character-name lookups have **no**
/// try/catch (a DB error propagates to the caller — `beginAutonomousRun` does
/// not guard this call either), while the final `addMessage` write is swallowed
/// inside [`post_autonomous_room_announcement`]. A missing character or an empty
/// name is simply skipped (v4's `c?.name` truthy check).
///
/// `chat` is the hydrated **pre-update** snapshot (participants + budget caps
/// are unchanged by the run-start flip).
pub async fn post_run_start_announcement(
    db: &Db,
    now_ms: NowMsFn<'_>,
    mint_uuid: MintUuidFn<'_>,
    chat_id: &str,
    run_id: &str,
    chat: &Value,
) -> Result<(), DbError> {
    // Resolve the active LLM participants' character names. The overlaid
    // `characters_read::find_by_id` needs both the main and mount-index
    // connections, so the reads run on the writer (the wardrobe precedent).
    let views = participant_views(chat);
    let character_ids: Vec<String> = get_active_character_participants(&views)
        .into_iter()
        // v4 `if (!p.characterId) continue` — already guaranteed by the filter,
        // kept for shape fidelity.
        .filter_map(|p| p.character_id.clone())
        .collect();

    let names: Vec<String> = db
        .write(move |writers| {
            let mount = writers
                .mount_index()
                .ok_or(DbError::PartitionUnavailable(
                    crate::write_partition::WriteDbTarget::MountIndex,
                ))?
                .connection();
            let main = writers.main().connection();
            let mut names = Vec::new();
            for cid in &character_ids {
                let c = characters_read::find_by_id(main, mount, cid)?;
                if let Some(name) = c
                    .as_ref()
                    .and_then(|c| c.get("name"))
                    .and_then(Value::as_str)
                    .filter(|n| !n.is_empty())
                {
                    names.push(name.to_string());
                }
            }
            Ok(names)
        })
        .await?;

    let content = build_run_start_content(chat, &names, run_id);
    post_autonomous_room_announcement(
        db,
        now_ms,
        mint_uuid,
        chat_id,
        AutonomousRoomKind::Start,
        &content,
        None,
    )
    .await;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn format_name_list_variants() {
        let n = |v: &[&str]| v.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        assert_eq!(format_name_list(&n(&[])), "");
        assert_eq!(format_name_list(&n(&["Aria"])), "Aria");
        assert_eq!(format_name_list(&n(&["Aria", "Beau"])), "Aria and Beau");
        assert_eq!(
            format_name_list(&n(&["Aria", "Beau", "Cleo"])),
            "Aria, Beau, and Cleo"
        );
        assert_eq!(
            format_name_list(&n(&["A", "B", "C", "D"])),
            "A, B, C, and D"
        );
    }

    #[test]
    fn locale_string_groups_thousands() {
        assert_eq!(to_locale_string(0), "0");
        assert_eq!(to_locale_string(999), "999");
        assert_eq!(to_locale_string(1500000), "1,500,000");
        assert_eq!(to_locale_string(1234567), "1,234,567");
    }

    #[test]
    fn js_round_half_up() {
        // JS Math.round(1.5) = 2, Math.round(2.5) = 3 (half toward +inf).
        assert_eq!(js_math_round(1.5), 2.0);
        assert_eq!(js_math_round(2.5), 3.0);
        assert_eq!(js_math_round(89.4), 89.0);
        // 90000/60000 = 1.5 → "2 min" (banked in the differential too).
        assert_eq!(js_math_round(90000.0 / 60000.0), 2.0);
    }

    #[test]
    fn run_start_content_all_caps_and_names() {
        let chat = json!({
            "budgetMaxTurns": 40,
            "budgetMaxTokens": 1500000,
            "budgetMaxWallClockMs": 5400000,
        });
        let names = vec!["Aria".to_string(), "Beau".to_string(), "Cleo".to_string()];
        assert_eq!(
            build_run_start_content(&chat, &names, "RUN-1"),
            "Autonomous room run begun with Aria, Beau, and Cleo. \
             Caps: 40 turn(s), 1,500,000 token(s), 90 min. Run id: RUN-1."
        );
    }

    #[test]
    fn run_start_content_no_caps_no_names() {
        let chat = json!({});
        assert_eq!(
            build_run_start_content(&chat, &[], "RUN-2"),
            "Autonomous room run begun. No caps configured. Run id: RUN-2."
        );
    }

    #[test]
    fn run_start_patch_field_set() {
        let p = run_start_patch("2031-01-01T00:00:00.000Z", "RUN-3");
        assert_eq!(p.current_run_id.as_deref(), Some("RUN-3"));
        assert_eq!(p.run_state.as_deref(), Some("running"));
        assert_eq!(p.run_state_message, Some(None));
        assert_eq!(
            p.run_started_at.as_deref(),
            Some("2031-01-01T00:00:00.000Z")
        );
        assert_eq!(p.run_ended_at, Some(None));
        assert_eq!(p.run_paused_at, Some(None));
        assert_eq!(p.run_paused_accum_ms, Some(0.0));
        assert_eq!(p.run_turns_consumed, Some(0.0));
        assert_eq!(p.run_tokens_consumed, Some(0.0));
        assert_eq!(p.run_milestones_announced, Some(0.0));
        // Nothing else set — the caller adds schedule bookkeeping explicitly.
        assert!(p.schedule_last_run_at.is_none());
        assert!(p.schedule_next_run_at.is_none());
        assert!(p.updated_at.is_none());
    }
}
