//! The conversation-chunks repository — a Phase-2 repo port, after `folders`,
//! `tags`, `text_replacement_rules`, `prompt_templates`,
//! `conversation_annotations`, `provider_models`, `help_docs`, `image_profiles`,
//! and the rest. Ports v4's
//! `lib/database/repositories/conversation-chunks.repository.ts` (+ the
//! `_create`/`_update`/`_delete` internals of `base.repository.ts`).
//!
//! Scope: `create`, `update`, and `delete` (the three abstract methods, each a
//! straight delegate to `_create`/`_update`/`_delete`). The custom helpers —
//! `countByChatIds`, `findByChatId`, `findByInterchangeIndex`, `upsert`,
//! `findAllWithEmbeddings`, `deleteAllForChat`, `updateEmbedding` — are out of
//! scope (the `upsert`/`updateEmbedding` paths need the remap-normalization form
//! for their internal `now`/`generateId()`, deferred like the other repos'
//! `upsert*`).
//!
//! ## A BLOB column, after `help_docs`
//!
//! `conversation_chunks.embedding` is the second tier-2 **BLOB column**, modeled
//! exactly like `help_docs.embedding`: a raw little-endian Float32 byte buffer
//! via [`crate::embedding_blob::float32_to_blob`]. v4's `documentToRow` blob path
//! (`embeddingToBlob`, wired by the repo auto-registering the `embedding` blob
//! column on first `getCollection()`) stores an **empty array or null as SQL
//! NULL**, never a zero-length blob; here `None` *or* an empty `Vec<f32>` binds
//! SQL NULL, only a non-empty vector is serialized. The canonical dump emits
//! BLOBs as lowercase hex on both sides, so a deterministic Float32 buffer
//! compares byte-for-byte (`[0.5,-0.25,0.75,0.125]` →
//! `0000003f000080be0000403f0000003e`).
//!
//! A text-only patch leaves the BLOB intact: v4's `_update` whole-row rewrite
//! re-persists the existing embedding unchanged, and this port models the patch
//! as a partial `UPDATE SET` that simply never names the `embedding` column when
//! `CcUpdate::embedding` is `None`. The corpus exercises this directly (a
//! content/interchangeIndex/participantNames update on the embedded seed row,
//! asserted to still show the original embedding hex).
//!
//! **Bug 17 (`62ab1bc8`) made the BLOB reachable through `upsert`.** v4's
//! `upsert` update arm now NULLs the stored vector when the content at a
//! `(chatId, interchangeIndex)` changed (so the render handler re-embeds — its
//! enqueue gate is `!chunk.embedding`), and a supplied `input.embedding` wins
//! over that. `CcUpdate::embedding` is the tri-state that carries it (`None`
//! untouched / `Some(None)` NULL / `Some(v)` quantized), driven from `upsert`;
//! the three-way behavior is pinned by the conversation-chunks corpus's `upsert`
//! ops (overwrite / NULL-on-content-change / preserve-on-identical-content).
//!
//! ## The rest of the marshaling surface
//!
//!   - `interchangeIndex` is `z.number().int().min(0)` — a min but NO max — so
//!     v4's `mapToSQLiteType` lowers it to **REAL** (INTEGER affinity needs an
//!     integer min AND max). It binds `f64`; an integer-valued REAL (e.g. `0.0`)
//!     renders back as `0` in the canonical dump via [`super::js_number_to_json`],
//!     matching v4 byte-for-byte.
//!   - `participantNames` and `messageIds` are `z.array(z.string()).default([])`
//!     JSON **array** columns — `Vec<String>` → compact JSON text
//!     (`["a","b"]`, `[]`), the order-preserving array shape (no key-order
//!     subtlety, unlike JSON-object columns).
//!   - `chatId` is a UUID → TEXT; `content` is TEXT; timestamps TEXT.
//!
//! Determinism: the tier-2 case pins the id and timestamps (CreateOptions on
//! create; an explicit `updatedAt` in each update patch), so the persisted rows
//! match v4's byte-for-byte with no normalization — the pinned form
//! `folders`/`tags`/`help_docs`/`provider_models` use.

use rusqlite::types::ToSql;
use rusqlite::{params, Connection};

use super::DbError;
use crate::embedding_blob::{blob_to_float32, float32_to_blob};

/// Fields for creating a conversation chunk (the `Omit<ConversationChunk,'id'|
/// timestamps>` shape). `embedding` is the BLOB column (`None`/empty → SQL NULL,
/// non-empty → little-endian Float32 bytes); `participant_names`/`message_ids`
/// are the JSON array columns; `interchange_index` is the REAL number column.
pub struct CcCreate {
    pub chat_id: String,
    /// `z.number().int().min(0)` (min only, no max) → REAL → bound `f64`.
    pub interchange_index: f64,
    pub content: String,
    /// Stored as compact JSON array text (`["a","b"]`, `[]` when empty).
    pub participant_names: Vec<String>,
    /// Stored as compact JSON array text (`["id"]`, `[]` when empty).
    pub message_ids: Vec<String>,
    /// `None` or empty → SQL NULL; non-empty → little-endian Float32 bytes.
    pub embedding: Option<Vec<f32>>,
}

/// Pinned id + timestamps (v4's `CreateOptions`).
pub struct CreateOptions {
    pub id: String,
    pub created_at: String,
    pub updated_at: String,
}

/// A conversation-chunk update patch. Mirrors v4 `update` over `_update`:
/// provided fields overwrite, id and createdAt are preserved, `updatedAt` is set
/// explicitly. Each `Some` text/number field sets that column.
///
/// The `embedding` field is **tri-state** (v4 Bug 17, `62ab1bc8`): `None` leaves
/// the BLOB untouched — the text-only patch's whole prior behavior, still the
/// only shape the render `upsert` produces on a preserve; the partial `UPDATE
/// SET` never names the column so the stored BLOB survives. `Some(None)` NULLs it
/// (v4's `updateData.embedding = null` — a content change re-render). `Some(v)`
/// sets it to the quantized Float32 blob (v4's supplied `input.embedding`), an
/// empty vector storing as SQL NULL per the codec convention.
#[derive(Default)]
pub struct CcUpdate {
    pub chat_id: Option<String>,
    /// REAL number column; `Some(n)` sets it (bound `f64`).
    pub interchange_index: Option<f64>,
    pub content: Option<String>,
    /// Re-serialized to compact JSON array text when provided.
    pub participant_names: Option<Vec<String>>,
    /// Re-serialized to compact JSON array text when provided.
    pub message_ids: Option<Vec<String>>,
    /// Bug 17: tri-state embedding — see the struct doc. `None` = untouched.
    pub embedding: Option<Option<Vec<f32>>>,
    pub updated_at: String,
}

/// Repository over a borrowed connection (held by the [`super::Writer`]).
pub struct ConversationChunksRepository<'c> {
    conn: &'c Connection,
}

/// A conversation chunk with its decoded embedding — the read shape
/// [`ConversationChunksRepository::find_all_with_embeddings`] returns (v4
/// `ConversationChunk` with a non-null embedding). Consumed by the Scriptorium
/// `search` tool's conversation source ([`super::conversation_search`]).
#[derive(Debug, Clone)]
pub struct EmbeddedChunk {
    pub id: String,
    pub chat_id: String,
    /// `interchangeIndex` — a JS number (REAL affinity); carried as `f64`.
    pub interchange_index: f64,
    pub content: String,
    pub participant_names: Vec<String>,
    pub embedding: Vec<f32>,
}

/// A chunk row as the EMBEDDING_GENERATE handler reads it (P4.6BL) — v4
/// `findById` narrowed to the fields the handler consumes (`content` to embed,
/// `interchangeIndex` for its log line).
#[derive(Debug, Clone)]
pub struct CcChunkRow {
    pub id: String,
    pub chat_id: String,
    /// `interchangeIndex` — a JS number (REAL affinity); carried as `f64`.
    pub interchange_index: f64,
    pub content: String,
}

/// v4's `ConversationChunkInput` as the render handler builds it — the fields it
/// passes to `upsert`. `embedding` models v4's optional `input.embedding`: the
/// render handler leaves it `None` (v4's key is absent — `!== undefined` is
/// false), which is what makes the update arm preserve or NULL rather than
/// overwrite; a caller that DOES supply one wins over the content-change NULL.
pub struct CcUpsert {
    pub chat_id: String,
    pub interchange_index: f64,
    pub content: String,
    pub participant_names: Vec<String>,
    pub message_ids: Vec<String>,
    /// `None` = not supplied (v4 `undefined`); `Some(v)` supplied (empty → NULL).
    pub embedding: Option<Vec<f32>>,
}

/// A chunk row as the CONVERSATION_RENDER handler and EMBEDDING_REINDEX_ALL read
/// it (P4.6BM) — the fields those two consume plus `has_embedding`, the
/// `chunk.embedding` truthiness test each branches on. The vector itself is never
/// needed there, so the BLOB is probed rather than decoded.
#[derive(Debug, Clone)]
pub struct CcRow {
    pub id: String,
    pub chat_id: String,
    /// `interchangeIndex` — a JS number (REAL affinity); carried as `f64`.
    pub interchange_index: f64,
    pub content: String,
    /// v4 `!chunk.embedding` — an absent or EMPTY vector is falsy. v5 stores an
    /// empty vector as SQL NULL, so `embedding IS NOT NULL` captures both.
    pub has_embedding: bool,
    /// The stored vector's component count — all `embeddingMatchesDim`
    /// (EMBEDDING_REINDEX_ALL's partial scope) reads. `0` for a NULL/empty BLOB,
    /// which can never equal a positive target dim.
    ///
    /// ⚠ This is the DECODED length, never `LENGTH(embedding) / 4`. Current
    /// writes are int8-quantized (`11 + dim` bytes, see
    /// [`crate::embedding_blob`]), so byte arithmetic gives a plausible, wrong
    /// answer — an 8-component vector measures 19 bytes, and `19 / 4` is 4.
    pub embedding_dim: usize,
}

fn marshal_cc_row(row: &rusqlite::Row) -> rusqlite::Result<CcRow> {
    let blob: Option<Vec<u8>> = row.get(4)?;
    let dim = blob
        .as_deref()
        .map(|b| blob_to_float32(b).len())
        .unwrap_or(0);
    Ok(CcRow {
        id: row.get(0)?,
        chat_id: row.get(1)?,
        interchange_index: row.get(2)?,
        content: row.get(3)?,
        has_embedding: dim > 0,
        embedding_dim: dim,
    })
}

impl<'c> ConversationChunksRepository<'c> {
    pub fn new(conn: &'c Connection) -> Self {
        Self { conn }
    }

    /// The `(total, embedded)` chunk counts for one chat — the no-preloaded
    /// scriptorium-status source (v4 chat-list enrichment reads
    /// `conversationChunks.findByChatId(chatId)` then `chunks.length` /
    /// `chunks.filter(c => c.embedding != null).length`). One `COUNT` for the total
    /// plus a `COUNT WHERE embedding IS NOT NULL` for the embedded count — the same
    /// numbers the batched `countByChatIds` GROUP BY yields. Read-only.
    pub fn count_stats_by_chat_id(&self, chat_id: &str) -> Result<(i64, i64), DbError> {
        let total: i64 = self.conn.query_row(
            "SELECT COUNT(*) FROM conversation_chunks WHERE chatId = ?1",
            rusqlite::params![chat_id],
            |r| r.get(0),
        )?;
        let embedded: i64 = self.conn.query_row(
            "SELECT COUNT(*) FROM conversation_chunks WHERE chatId = ?1 AND embedding IS NOT NULL",
            rusqlite::params![chat_id],
            |r| r.get(0),
        )?;
        Ok((total, embedded))
    }

    /// v4 `findAllWithEmbeddings` = `_findAll()` filtered to a non-null, non-empty
    /// embedding. Reads every chunk (no scoping, rowid/insertion order) and keeps
    /// only those carrying a decoded embedding. Read-only.
    pub fn find_all_with_embeddings(&self) -> Result<Vec<EmbeddedChunk>, DbError> {
        let mut stmt = self.conn.prepare(
            "SELECT id, chatId, interchangeIndex, content, participantNames, embedding \
             FROM conversation_chunks WHERE embedding IS NOT NULL",
        )?;
        let rows = stmt.query_map([], |r| {
            let blob: Vec<u8> = r.get(5)?;
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, f64>(2)?,
                r.get::<_, String>(3)?,
                r.get::<_, String>(4)?,
                blob,
            ))
        })?;
        let mut out = Vec::new();
        for row in rows {
            let (id, chat_id, interchange_index, content, participant_names_json, blob) = row?;
            let embedding = blob_to_float32(&blob);
            // v4's `.length > 0` guard.
            if embedding.is_empty() {
                continue;
            }
            let participant_names: Vec<String> =
                serde_json::from_str(&participant_names_json).unwrap_or_default();
            out.push(EmbeddedChunk {
                id,
                chat_id,
                interchange_index,
                content,
                participant_names,
                embedding,
            });
        }
        Ok(out)
    }

    /// Insert a conversation chunk with the given pinned id + timestamps. The
    /// embedding serializes to a little-endian Float32 BLOB (`None`/empty → SQL
    /// NULL); `participantNames`/`messageIds` → compact JSON array text;
    /// `interchangeIndex` binds `f64`.
    pub fn create(&self, data: &CcCreate, opts: &CreateOptions) -> Result<(), DbError> {
        // empty / null embedding -> SQL NULL; non-empty -> Float32 LE bytes.
        let embedding_blob: Option<Vec<u8>> = match &data.embedding {
            Some(v) if !v.is_empty() => Some(float32_to_blob(v)),
            _ => None,
        };
        let participant_names_json = serde_json::to_string(&data.participant_names)
            .map_err(|e| DbError::Key(format!("participantNames serialize: {e}")))?;
        let message_ids_json = serde_json::to_string(&data.message_ids)
            .map_err(|e| DbError::Key(format!("messageIds serialize: {e}")))?;

        self.conn.execute(
            "INSERT INTO conversation_chunks \
               (id, chatId, interchangeIndex, content, participantNames, messageIds, \
                embedding, createdAt, updatedAt) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            params![
                opts.id,
                data.chat_id,
                data.interchange_index,
                data.content,
                participant_names_json,
                message_ids_json,
                embedding_blob,
                opts.created_at,
                opts.updated_at,
            ],
        )?;
        Ok(())
    }

    /// Apply an update patch to the chunk `id`. Returns `Ok(false)` when no row
    /// matched (v4's "not found -> null"). id, createdAt, and the `embedding`
    /// BLOB are never touched. Each `Some` field sets that column; `updatedAt` is
    /// always set.
    pub fn update(&self, id: &str, patch: &CcUpdate) -> Result<bool, DbError> {
        // v4 `_update` first `findById`: the row must exist or the update is a
        // no-op (-> null). Mirror that so a missing target yields Ok(false)
        // rather than relying on the UPDATE affecting zero rows.
        if !self.row_exists(id)? {
            return Ok(false);
        }

        let mut assignments: Vec<String> = Vec::new();
        let mut values: Vec<Box<dyn ToSql>> = Vec::new();

        if let Some(chat_id) = &patch.chat_id {
            assignments.push(format!("chatId = ?{}", values.len() + 1));
            values.push(Box::new(chat_id.clone()));
        }
        if let Some(interchange_index) = patch.interchange_index {
            assignments.push(format!("interchangeIndex = ?{}", values.len() + 1));
            values.push(Box::new(interchange_index));
        }
        if let Some(content) = &patch.content {
            assignments.push(format!("content = ?{}", values.len() + 1));
            values.push(Box::new(content.clone()));
        }
        if let Some(participant_names) = &patch.participant_names {
            let participant_names_json = serde_json::to_string(participant_names)
                .map_err(|e| DbError::Key(format!("participantNames serialize: {e}")))?;
            assignments.push(format!("participantNames = ?{}", values.len() + 1));
            values.push(Box::new(participant_names_json));
        }
        if let Some(message_ids) = &patch.message_ids {
            let message_ids_json = serde_json::to_string(message_ids)
                .map_err(|e| DbError::Key(format!("messageIds serialize: {e}")))?;
            assignments.push(format!("messageIds = ?{}", values.len() + 1));
            values.push(Box::new(message_ids_json));
        }
        // Bug 17: the embedding column is named only when the patch carries it
        // (tri-state — `None` leaves the BLOB untouched). An empty vector, like
        // `None`-inner, binds SQL NULL (the codec convention).
        if let Some(embedding_opt) = &patch.embedding {
            let blob: Option<Vec<u8>> = match embedding_opt {
                Some(v) if !v.is_empty() => Some(float32_to_blob(v)),
                _ => None,
            };
            assignments.push(format!("embedding = ?{}", values.len() + 1));
            values.push(Box::new(blob));
        }
        assignments.push(format!("updatedAt = ?{}", values.len() + 1));
        values.push(Box::new(patch.updated_at.clone()));

        let id_idx = values.len() + 1;
        values.push(Box::new(id.to_string()));

        let sql = format!(
            "UPDATE conversation_chunks SET {} WHERE id = ?{}",
            assignments.join(", "),
            id_idx
        );

        let params_refs: Vec<&dyn ToSql> = values.iter().map(|b| b.as_ref()).collect();
        let affected = self.conn.execute(&sql, params_refs.as_slice())?;
        Ok(affected > 0)
    }

    /// v4 `findById` (P4.6BL), narrowed to the handler-read fields (see
    /// [`CcChunkRow`]). `None` when no row matches.
    pub fn find_row_by_id(&self, id: &str) -> Result<Option<CcChunkRow>, DbError> {
        self.conn
            .query_row(
                "SELECT id, chatId, interchangeIndex, content \
                 FROM conversation_chunks WHERE id = ?1",
                params![id],
                |row| {
                    Ok(CcChunkRow {
                        id: row.get(0)?,
                        chat_id: row.get(1)?,
                        interchange_index: row.get(2)?,
                        content: row.get(3)?,
                    })
                },
            )
            .map(Some)
            .or_else(|e| match e {
                rusqlite::Error::QueryReturnedNoRows => Ok(None),
                other => Err(other.into()),
            })
    }

    /// v4 `updateEmbedding` (P4.6BL) — set just the `embedding` BLOB on a chunk
    /// (+ the minted `updatedAt`, injected here as `now_iso` per this repo's
    /// convention). The vector serializes exactly as `create` does (`empty →
    /// SQL NULL`, else Float32 LE bytes). Returns `Ok(false)` when no row
    /// matched — v4 THROWS `` `Chunk not found for embedding update: ${id}` ``
    /// there; the EMBEDDING_GENERATE handler (its only caller) formats that
    /// exact string so the persisted error text matches byte-for-byte.
    pub fn update_embedding(
        &self,
        id: &str,
        embedding: &[f32],
        now_iso: &str,
    ) -> Result<bool, DbError> {
        let embedding_blob: Option<Vec<u8>> = if embedding.is_empty() {
            None
        } else {
            Some(float32_to_blob(embedding))
        };
        let affected = self.conn.execute(
            "UPDATE conversation_chunks SET embedding = ?1, updatedAt = ?2 WHERE id = ?3",
            params![embedding_blob, now_iso, id],
        )?;
        Ok(affected > 0)
    }

    /// Cold-tier a chat's chunks (v4 `clearEmbeddingsForChat`): NULL every
    /// non-null `embedding` while keeping `content` / `messageIds` /
    /// `interchangeIndex` intact, so the chat stays keyword-searchable and can be
    /// re-embedded (embed-only, no re-chunk) when reopened. Idempotent — the
    /// `embedding IS NOT NULL` guard makes a second pass a no-op. Returns the
    /// number of rows cleared.
    ///
    /// `older_than` (ISO-8601) restricts clearing to embeddings last written
    /// before that moment (v4 `f7cc887b`). The maintenance sweep passes its
    /// staleness cutoff here so an embedding minted by the reopen path
    /// (`cold-chunk-reembed`) survives for a full retention window from the
    /// reopen: a chat the user READS — without playing a message, so it stays
    /// "stale" — keeps its warmth instead of being re-embedded (paid) on every
    /// open and cleared again by every sweep. The embedding row's own
    /// `updatedAt` is the reopen signal; nothing else writes chunk rows on a
    /// stale chat, because renders only fire on played messages and the boot
    /// reconcile skips stale chats.
    ///
    /// Unlike [`Self::update`], this DELIBERATELY bumps `updatedAt` (the cold-tier
    /// is a real change the chunk carries) — v4 stamps `new Date().toISOString()`;
    /// the sweep injects `now_iso` here for a deterministic differential.
    pub fn clear_embeddings_for_chat(
        &self,
        chat_id: &str,
        now_iso: &str,
        older_than: Option<&str>,
    ) -> Result<usize, DbError> {
        // v4 builds the SQL and the param list together: the guard clause is
        // appended only when `olderThan` is present, and the params go
        // `[now, chatId, olderThan?]` — match both exactly.
        let age_guard = if older_than.is_some() {
            " AND updatedAt < ?3"
        } else {
            ""
        };
        let sql = format!(
            "UPDATE conversation_chunks SET embedding = NULL, updatedAt = ?1 \
             WHERE chatId = ?2 AND embedding IS NOT NULL{age_guard}"
        );
        let affected = match older_than {
            Some(cutoff) => self.conn.execute(&sql, params![now_iso, chat_id, cutoff])?,
            None => self.conn.execute(&sql, params![now_iso, chat_id])?,
        };
        Ok(affected)
    }

    /// The ids of a chat's COLD chunks — content present, embedding absent (v4
    /// cold-chunk-reembed's per-chunk filter: `chunk.embedding == null ||
    /// length 0` AND `chunk.content?.trim()` non-empty). Since v5 stores an
    /// empty embedding as SQL NULL, `embedding IS NULL` captures both the
    /// null and empty-vector cases. Returned in rowid (insertion) order to
    /// mirror v4's `findByChatId` iteration. Read-only.
    pub fn find_cold_chunk_ids_by_chat_id(&self, chat_id: &str) -> Result<Vec<String>, DbError> {
        let mut stmt = self.conn.prepare(
            "SELECT id FROM conversation_chunks \
             WHERE chatId = ?1 AND embedding IS NULL AND trim(content) != '' \
             ORDER BY rowid ASC",
        )?;
        let rows = stmt.query_map(params![chat_id], |r| r.get::<_, String>(0))?;
        rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
    }

    // === P4.6BM: the CONVERSATION_RENDER handler's reads + upsert ===

    /// v4 `findByChatId` — every chunk for a chat, **ordered by
    /// `interchangeIndex` ascending** (v4's `{ sort: { interchangeIndex: 1 } }`).
    /// The column is REAL (see the module doc), so the sort is numeric, not
    /// lexical. Read-only.
    pub fn find_by_chat_id(&self, chat_id: &str) -> Result<Vec<CcRow>, DbError> {
        let mut stmt = self.conn.prepare(
            "SELECT id, chatId, interchangeIndex, content, embedding \
             FROM conversation_chunks WHERE chatId = ?1 ORDER BY interchangeIndex ASC",
        )?;
        let rows = stmt.query_map(params![chat_id], marshal_cc_row)?;
        rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
    }

    /// v4 `findByInterchangeIndex` — `findOneByFilter({chatId, interchangeIndex})`.
    /// `findOne` takes the FIRST match in storage order, so this reads in rowid
    /// order and stops at the first row (the pair is not unique-constrained;
    /// duplicates are legal and v4 silently prefers the earliest). Read-only.
    pub fn find_by_interchange_index(
        &self,
        chat_id: &str,
        interchange_index: f64,
    ) -> Result<Option<CcRow>, DbError> {
        let mut stmt = self.conn.prepare(
            "SELECT id, chatId, interchangeIndex, content, embedding \
             FROM conversation_chunks WHERE chatId = ?1 AND interchangeIndex = ?2 \
             ORDER BY rowid ASC LIMIT 1",
        )?;
        let mut rows = stmt.query_map(params![chat_id, interchange_index], marshal_cc_row)?;
        rows.next().transpose().map_err(Into::into)
    }

    /// v4 `upsert(input)` — update the chunk matching `(chatId,
    /// interchangeIndex)` if one exists, else create it.
    ///
    /// The update arm writes `content` / `participantNames` / `messageIds`
    /// (v4's `updateData`) plus, per v4 Bug 17 (`62ab1bc8`), the embedding: a
    /// supplied `input.embedding` wins; else if `existing.content !=
    /// input.content` the stale vector is NULLed so the render handler re-embeds
    /// (its enqueue gate is `!chunk.embedding`); else the BLOB is left untouched
    /// (a re-render of unchanged text preserves the vector already computed).
    /// `updatedAt` is stamped from the injected `now_iso`, matching `_update`'s
    /// minted `now`.
    ///
    /// The create arm mints the row with `new_id` + `now_iso` for all three of
    /// id / createdAt / updatedAt and the supplied-or-NULL embedding (v4's
    /// `_create` with the schema's `embedding` default). Returns `(chunk_id,
    /// created)`.
    pub fn upsert(
        &self,
        input: &CcUpsert,
        new_id: &str,
        now_iso: &str,
    ) -> Result<(String, bool), DbError> {
        if let Some(existing) =
            self.find_by_interchange_index(&input.chat_id, input.interchange_index)?
        {
            // v4: `input.embedding !== undefined` wins; else a content change
            // NULLs the now-stale vector; else leave the BLOB alone.
            let embedding = match &input.embedding {
                Some(v) => Some(Some(v.clone())),
                None if existing.content != input.content => Some(None),
                None => None,
            };
            let patch = CcUpdate {
                content: Some(input.content.clone()),
                participant_names: Some(input.participant_names.clone()),
                message_ids: Some(input.message_ids.clone()),
                embedding,
                updated_at: now_iso.to_string(),
                ..Default::default()
            };
            self.update(&existing.id, &patch)?;
            return Ok((existing.id, false));
        }
        let create = CcCreate {
            chat_id: input.chat_id.clone(),
            interchange_index: input.interchange_index,
            content: input.content.clone(),
            participant_names: input.participant_names.clone(),
            message_ids: input.message_ids.clone(),
            // v4 `_create(input)` carries the supplied embedding (or NULL).
            embedding: input.embedding.clone(),
        };
        let opts = CreateOptions {
            id: new_id.to_string(),
            created_at: now_iso.to_string(),
            updated_at: now_iso.to_string(),
        };
        self.create(&create, &opts)?;
        Ok((new_id.to_string(), true))
    }

    // === end P4.6BM ===

    /// Delete the chunk `id`. Returns `Ok(false)` when no row matched (v4's
    /// `_delete` "deletedCount === 0 -> false").
    pub fn delete(&self, id: &str) -> Result<bool, DbError> {
        let affected = self
            .conn
            .execute("DELETE FROM conversation_chunks WHERE id = ?1", params![id])?;
        Ok(affected > 0)
    }

    /// True iff a row with this id exists — v4's `_update` `findById` precondition
    /// (a missing target makes the update a no-op returning `null`).
    fn row_exists(&self, id: &str) -> Result<bool, DbError> {
        let found: Option<i64> = self
            .conn
            .query_row(
                "SELECT 1 FROM conversation_chunks WHERE id = ?1",
                params![id],
                |row| row.get::<_, i64>(0),
            )
            .map(Some)
            .or_else(|e| match e {
                rusqlite::Error::QueryReturnedNoRows => Ok(None),
                other => Err(other),
            })?;
        Ok(found.is_some())
    }
}
