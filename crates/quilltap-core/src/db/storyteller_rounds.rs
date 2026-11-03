//! The `storyteller_rounds` repository (v4
//! `lib/database/repositories/storyteller-rounds.repository.ts`, P4.D343).
//!
//! One row per storyteller round: the operator post that opened it, the bids
//! gathered while it is `open`, and the selection once the operator chooses.
//! When a round closes its `bids` are cleared — the full bids stay in the LLM
//! log — and only ids, timings and the selection are kept. Not exported or
//! backed up: bids are not part of the record.
//!
//! A bid lands with [`StorytellerRoundsRepository::record_bid`], which
//! re-reads the row and replaces that one seat's card. v4 serialises those
//! read-modify-writes per round with a promise chain (`withRoundLock`); in v5
//! the caller runs it as ONE `Db::write` closure, and the single writer task
//! is the lock.
//!
//! Reads follow v4's base repository: `findByFilter` is a fallback (a failed
//! SELECT logs `Error finding entities by filter` and answers `[]`, so the
//! methods' own `safeQuery` lines — `Error finding active storyteller round` /
//! `… latest …` — never fire), and a row that fails the round schema is
//! DROPPED silently; `findById` validates THROWING inside its `safeQuery`, so
//! a corrupt row logs `Error finding entity by ID` and reads as absent. Every
//! write stores the Zod-parsed object: keys in schema order, `null`s kept.

use rusqlite::{params, Connection, OptionalExtension};

use super::DbError;
use crate::storyteller::types::{
    StorytellerBid, StorytellerClosedReason, StorytellerRound, StorytellerRoundStatus,
    StorytellerSelection,
};

const COLLECTION: &str = "storyteller_rounds";

const SELECT_COLUMNS: &str = r#"SELECT "id", "chatId", "triggerMessageId", "status", "bids", "selection", "closedReason", "createdAt", "updatedAt", "closedAt" FROM "storyteller_rounds""#;

/// The columns of one raw row, before the round schema reads them.
struct RawRow {
    id: Option<String>,
    chat_id: Option<String>,
    trigger_message_id: Option<String>,
    status: Option<String>,
    bids: Option<String>,
    selection: Option<String>,
    closed_reason: Option<String>,
    created_at: Option<String>,
    updated_at: Option<String>,
    closed_at: Option<String>,
}

fn raw_from(r: &rusqlite::Row<'_>) -> rusqlite::Result<RawRow> {
    Ok(RawRow {
        id: r.get(0)?,
        chat_id: r.get(1)?,
        trigger_message_id: r.get(2)?,
        status: r.get(3)?,
        bids: r.get(4)?,
        selection: r.get(5)?,
        closed_reason: r.get(6)?,
        created_at: r.get(7)?,
        updated_at: r.get(8)?,
        closed_at: r.get(9)?,
    })
}

/// v4's row → `StorytellerRoundSchema.safeParse`, through v4's backend
/// deserializer first (`backends/sqlite/backend.ts`), measured by
/// `storyteller_rounds_tier2`:
///
/// - EVERY NULL cell reads as ABSENT (`undefined`), not `null` — so a read
///   round carries no `selection` / `closedReason` / `closedAt` key when the
///   cell is NULL, and a NULL `triggerMessageId` (`.nullable()`, NOT
///   `.optional()`) makes the row fail its schema: `findById` logs and
///   answers absent, `findByFilter` drops it.
/// - A JSON column (`bids`, `selection`) goes through `fromJsonSafe`: an
///   empty, unparseable or `'null'` cell reads as ABSENT (v4 also logs its
///   backend `Corrupted JSON in column, using default` WARN — unported by
///   standing convention), so an unreadable `bids` falls to its `[]` default
///   and the row SURVIVES; a parseable value the schema refuses drops it.
///
/// `None` when the row does not parse.
fn parse_row(raw: RawRow) -> Option<StorytellerRound> {
    use serde_json::{Map, Value};
    // v4 `fromJsonSafe` + the null→undefined rule.
    let json_cell = |cell: Option<String>| -> Option<Value> {
        let text = cell?;
        if text.is_empty() {
            return None;
        }
        match serde_json::from_str::<Value>(&text) {
            Ok(Value::Null) | Err(_) => None,
            Ok(v) => Some(v),
        }
    };
    // `.nullable()` without `.optional()`: the `undefined` a NULL cell reads
    // as fails Zod (serde would otherwise default a missing `Option` to None).
    raw.trigger_message_id.as_ref()?;
    let mut m = Map::new();
    let mut put = |key: &str, v: Option<Value>| {
        if let Some(v) = v {
            m.insert(key.into(), v);
        }
    };
    put("id", raw.id.map(Value::String));
    put("chatId", raw.chat_id.map(Value::String));
    put(
        "triggerMessageId",
        raw.trigger_message_id.map(Value::String),
    );
    put("status", raw.status.map(Value::String));
    put("bids", json_cell(raw.bids));
    put("selection", json_cell(raw.selection));
    put("closedReason", raw.closed_reason.map(Value::String));
    put("createdAt", raw.created_at.map(Value::String));
    put("updatedAt", raw.updated_at.map(Value::String));
    put("closedAt", raw.closed_at.map(Value::String));
    serde_json::from_value(Value::Object(m)).ok()
}

/// The JSON text v4 stores for a JSON column (`JSON.stringify` of the
/// Zod-parsed value).
fn bids_text(bids: &[StorytellerBid]) -> String {
    serde_json::to_string(bids).expect("bids serialise")
}

fn selection_text(selection: &Option<Option<StorytellerSelection>>) -> Option<String> {
    selection
        .as_ref()
        .and_then(|s| s.as_ref())
        .map(|s| serde_json::to_string(s).expect("selection serialises"))
}

/// The `storyteller_rounds` repository over a borrowed connection.
pub struct StorytellerRoundsRepository<'c> {
    conn: &'c Connection,
}

impl<'c> StorytellerRoundsRepository<'c> {
    pub fn new(conn: &'c Connection) -> Self {
        Self { conn }
    }

    /// v4 `_findById`: a corrupt row logs `Error finding entity by ID` and
    /// reads as absent.
    pub fn find_by_id(&self, id: &str) -> Option<StorytellerRound> {
        super::fallback::find_by_id_or_none(COLLECTION, id, || {
            let raw = self
                .conn
                .query_row(
                    &format!(r#"{SELECT_COLUMNS} WHERE "id" = ?1"#),
                    [id],
                    raw_from,
                )
                .optional()?;
            match raw {
                None => Ok(None),
                Some(raw) => parse_row(raw).map(Some).ok_or_else(|| {
                    DbError::Internal(format!("storyteller round {id} fails its schema"))
                }),
            }
        })
    }

    /// v4 `findByFilter({chatId})`: a failed SELECT logs and answers `[]`; a
    /// row that fails the schema is dropped silently.
    fn find_by_chat(&self, chat_id: &str) -> Vec<StorytellerRound> {
        super::fallback::find_by_filter_or_empty(COLLECTION, || {
            let mut stmt = self
                .conn
                .prepare(&format!(r#"{SELECT_COLUMNS} WHERE "chatId" = ?1"#))?;
            let rows = stmt.query_map([chat_id], raw_from)?;
            let mut out = Vec::new();
            for r in rows {
                if let Some(round) = parse_row(r?) {
                    out.push(round);
                }
            }
            Ok(out)
        })
    }

    /// v4 `findActiveForChat`: the chat's latest round that is not closed
    /// (open or running), `createdAt` descending by `localeCompare`.
    pub fn find_active_for_chat(&self, chat_id: &str) -> Option<StorytellerRound> {
        let mut active: Vec<StorytellerRound> = self
            .find_by_chat(chat_id)
            .into_iter()
            .filter(|r| r.status != StorytellerRoundStatus::Closed)
            .collect();
        active.sort_by(|a, b| crate::collation::locale_compare(&b.created_at, &a.created_at));
        active.into_iter().next()
    }

    /// v4 `findLatestForChat`: the chat's most recent round, closed or not.
    pub fn find_latest_for_chat(&self, chat_id: &str) -> Option<StorytellerRound> {
        let mut rows = self.find_by_chat(chat_id);
        rows.sort_by(|a, b| crate::collation::locale_compare(&b.created_at, &a.created_at));
        rows.into_iter().next()
    }

    /// v4 `create`: mints `id` / `createdAt` / `updatedAt` (one clock reading)
    /// and stores the row.
    #[allow(clippy::too_many_arguments)]
    pub fn create(
        &self,
        chat_id: &str,
        trigger_message_id: Option<&str>,
        status: StorytellerRoundStatus,
        bids: Vec<StorytellerBid>,
        id: &str,
        now: &str,
    ) -> Result<StorytellerRound, DbError> {
        let round = StorytellerRound {
            id: id.to_string(),
            chat_id: chat_id.to_string(),
            trigger_message_id: trigger_message_id.map(str::to_string),
            status,
            bids,
            selection: Some(None),
            closed_reason: Some(None),
            created_at: now.to_string(),
            updated_at: now.to_string(),
            closed_at: Some(None),
        };
        self.conn.execute(
            r#"INSERT INTO "storyteller_rounds" ("id", "chatId", "triggerMessageId", "status", "bids", "selection", "closedReason", "createdAt", "updatedAt", "closedAt") VALUES (?1, ?2, ?3, ?4, ?5, NULL, NULL, ?6, ?7, NULL)"#,
            params![
                round.id,
                round.chat_id,
                round.trigger_message_id,
                round.status.as_str(),
                bids_text(&round.bids),
                round.created_at,
                round.updated_at,
            ],
        )?;
        Ok(round)
    }

    /// v4 `_update`: re-read, merge, stamp `updatedAt`, write the whole row.
    /// `None` when the row is gone (v4's `Entity not found for update` WARN).
    fn update(
        &self,
        id: &str,
        now: &str,
        merge: impl FnOnce(&mut StorytellerRound),
    ) -> Result<Option<StorytellerRound>, DbError> {
        let Some(mut round) = self.find_by_id(id) else {
            tracing::warn!(
                target: "quilltap::db",
                collection = COLLECTION,
                id = %id,
                "Entity not found for update"
            );
            return Ok(None);
        };
        merge(&mut round);
        round.updated_at = now.to_string();
        self.conn.execute(
            r#"UPDATE "storyteller_rounds" SET "chatId" = ?2, "triggerMessageId" = ?3, "status" = ?4, "bids" = ?5, "selection" = ?6, "closedReason" = ?7, "createdAt" = ?8, "updatedAt" = ?9, "closedAt" = ?10 WHERE "id" = ?1"#,
            params![
                round.id,
                round.chat_id,
                round.trigger_message_id,
                round.status.as_str(),
                bids_text(&round.bids),
                selection_text(&round.selection),
                round.closed_reason.flatten().map(|r| r.as_str()),
                round.created_at,
                round.updated_at,
                round.closed_at.clone().flatten(),
            ],
        )?;
        Ok(Some(round))
    }

    /// v4 `recordBid`: replace one seat's card on an OPEN round (or append it).
    /// A round that is gone or no longer open drops the bid with DEBUG
    /// `Storyteller bid dropped — round not open` and answers `None`.
    pub fn record_bid(
        &self,
        round_id: &str,
        bid: StorytellerBid,
        now: &str,
    ) -> Result<Option<StorytellerRound>, DbError> {
        let round = self.find_by_id(round_id);
        let open = round
            .as_ref()
            .is_some_and(|r| r.status == StorytellerRoundStatus::Open);
        if !open {
            tracing::debug!(
                target: "quilltap::storyteller",
                context = "storyteller-rounds.repository",
                roundId = %round_id,
                participantId = %bid.participant_id,
                status = round.as_ref().map_or("missing", |r| r.status.as_str()),
                "Storyteller bid dropped — round not open"
            );
            return Ok(None);
        }
        self.update(round_id, now, |r| {
            match r
                .bids
                .iter_mut()
                .find(|existing| existing.participant_id == bid.participant_id)
            {
                Some(slot) => *slot = bid,
                None => r.bids.push(bid),
            }
        })
    }

    /// v4 `markRunning`: `{status: 'running', selection}`.
    pub fn mark_running(
        &self,
        round_id: &str,
        selection: StorytellerSelection,
        now: &str,
    ) -> Result<Option<StorytellerRound>, DbError> {
        self.update(round_id, now, |r| {
            r.status = StorytellerRoundStatus::Running;
            r.selection = Some(Some(selection));
        })
    }

    /// v4 `markClosed`: `{status: 'closed', closedReason, closedAt: now, bids:
    /// []}`, plus the selection when one is given (`Some(None)` writes a
    /// `null`; `None` leaves the stored one).
    pub fn mark_closed(
        &self,
        round_id: &str,
        reason: StorytellerClosedReason,
        selection: Option<Option<StorytellerSelection>>,
        now: &str,
    ) -> Result<Option<StorytellerRound>, DbError> {
        self.update(round_id, now, |r| {
            r.status = StorytellerRoundStatus::Closed;
            r.closed_reason = Some(Some(reason));
            r.closed_at = Some(Some(now.to_string()));
            r.bids = Vec::new();
            if let Some(selection) = selection {
                r.selection = Some(selection);
            }
        })
    }

    /// Every round of one chat, removed (contract C-2: a chat delete removes
    /// its rounds EXPLICITLY — a v5-provisioned instance has no FK cascade).
    pub fn delete_by_chat_id(&self, chat_id: &str) -> Result<usize, DbError> {
        Ok(self.conn.execute(
            r#"DELETE FROM "storyteller_rounds" WHERE "chatId" = ?1"#,
            [chat_id],
        )?)
    }
}
