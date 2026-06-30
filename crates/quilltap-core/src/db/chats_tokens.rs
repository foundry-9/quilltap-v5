//! The `chats` **token-tracking** ops (the conversation capstone, sub-unit 6).
//! Ports v4's `ChatTokenTrackingOps`
//! (`lib/database/repositories/chats-tokens.ops.ts`): the prompt/completion
//! token aggregate counters + the estimated-cost accumulator.
//!
//! ## `incrementTokenAggregates`
//!
//! v4 builds a Mongo-style `{ $inc: { totalPromptTokens, totalCompletionTokens },
//! $set: { updatedAt: now } }` and, when `estimatedCost > 0` AND the chat exists,
//! also `$set.estimatedCostUSD = (existing.estimatedCostUSD || 0) + estimatedCost`
//! (plus `$set.priceSource` when a `priceSource` was passed). On SQLite the
//! `$inc` collapses to a single self-referential `UPDATE … SET col = col + ?`, so
//! the whole op is ONE statement with the increment + the always-minted
//! `updatedAt` and, conditionally, the cost/priceSource SET clauses. A missing
//! chat matches zero rows → no-op (v4 just warns). `updatedAt` is minted
//! UNconditionally, which is why the differential needs the sentinel-aware
//! normalization (a reset, below, does NOT mint it).
//!
//! The cost accumulation reads the current `estimatedCostUSD` via the marshaled
//! chat ([`chats_read::find_by_id`]); that marshaler DROPS a NULL
//! `estimatedCostUSD` cell (v4's `undefined`-dropping read), so an absent key
//! means "no cost yet" → treat as `0` (matching v4's `existing.estimatedCostUSD
//! || 0`). Both v4 and the port compute `current + cost` in IEEE f64, so the
//! stored bytes are identical (the dump renders via [`js_number_to_json`]).
//!
//! ## `resetTokenAggregates`
//!
//! `update(chatId, { totalPromptTokens: 0, totalCompletionTokens: 0,
//! estimatedCostUSD: null })` — counters back to zero, the cost cleared to SQL
//! NULL. It rides [`ChatsRepository::update`], which PRESERVES `updatedAt`
//! (v4's `_update` override), so a reset does not bump the chat's timestamp.

use rusqlite::Connection;

use super::chats::{ChatUpdate, ChatsRepository};
use super::{chats_read, DbError};
use crate::clock::now_iso;

/// Repository over a borrowed MAIN-db connection (held by the [`super::Writer`]).
pub struct ChatTokensRepository<'c> {
    conn: &'c Connection,
}

impl<'c> ChatTokensRepository<'c> {
    pub fn new(conn: &'c Connection) -> Self {
        Self { conn }
    }

    /// `incrementTokenAggregates` — atomically add `prompt_tokens` /
    /// `completion_tokens` to the running counters and bump `updatedAt` to a
    /// minted `now`. When `estimated_cost` is `Some(c)` with `c > 0` AND the chat
    /// exists, also accumulate `estimatedCostUSD = current + c` (current defaults
    /// to `0` when the cell is NULL) and, when `price_source` is `Some`, set
    /// `priceSource`. A missing chat matches zero rows → silent no-op (v4 warns).
    pub fn increment_token_aggregates(
        &self,
        chat_id: &str,
        prompt_tokens: f64,
        completion_tokens: f64,
        estimated_cost: Option<f64>,
        price_source: Option<&str>,
    ) -> Result<(), DbError> {
        let now = now_iso();

        // The cost/priceSource SET clauses are added only when the cost is
        // positive AND the chat exists (v4 reads it via findById first).
        let cost_update: Option<(f64, Option<&str>)> = match estimated_cost {
            Some(c) if c > 0.0 => match chats_read::find_by_id(self.conn, chat_id)? {
                Some(existing) => {
                    // The marshaler DROPS a NULL estimatedCostUSD → absent = 0
                    // (v4's `existing.estimatedCostUSD || 0`).
                    let current = existing
                        .get("estimatedCostUSD")
                        .and_then(serde_json::Value::as_f64)
                        .unwrap_or(0.0);
                    Some((current + c, price_source))
                }
                None => None,
            },
            _ => None,
        };

        // Build the single self-referential UPDATE. `?1`/`?2` are the increments,
        // `?3` the minted updatedAt; the optional cost/priceSource follow.
        let mut sql = String::from(
            "UPDATE chats SET \
               totalPromptTokens = totalPromptTokens + ?1, \
               totalCompletionTokens = totalCompletionTokens + ?2, \
               updatedAt = ?3",
        );
        let mut params: Vec<Box<dyn rusqlite::ToSql>> = vec![
            Box::new(prompt_tokens),
            Box::new(completion_tokens),
            Box::new(now),
        ];
        if let Some((new_cost, ps)) = &cost_update {
            params.push(Box::new(*new_cost));
            sql.push_str(&format!(", estimatedCostUSD = ?{}", params.len()));
            if let Some(ps) = ps {
                params.push(Box::new(ps.to_string()));
                sql.push_str(&format!(", priceSource = ?{}", params.len()));
            }
        }
        params.push(Box::new(chat_id.to_string()));
        sql.push_str(&format!(" WHERE id = ?{}", params.len()));

        let param_refs: Vec<&dyn rusqlite::ToSql> = params.iter().map(|b| b.as_ref()).collect();
        // A missing chat matches zero rows — no error, matching v4's warn-and-return.
        self.conn.execute(&sql, param_refs.as_slice())?;
        Ok(())
    }

    /// `resetTokenAggregates` — counters back to `0` and `estimatedCostUSD` to SQL
    /// NULL, via [`ChatsRepository::update`] (which PRESERVES `updatedAt`).
    /// Returns whether a row was updated (v4's non-null `ChatMetadata`).
    pub fn reset_token_aggregates(&self, chat_id: &str) -> Result<bool, DbError> {
        let update = ChatUpdate {
            total_prompt_tokens: Some(0.0),
            total_completion_tokens: Some(0.0),
            estimated_cost_usd: Some(None),
            ..Default::default()
        };
        ChatsRepository::new(self.conn).update(chat_id, &update)
    }
}
