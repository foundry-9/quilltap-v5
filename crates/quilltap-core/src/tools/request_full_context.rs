//! Port of v4's `request_full_context` tool
//! (`lib/tools/handlers/request-full-context-handler.ts` +
//! `lib/tools/request-full-context-tool.ts`).
//!
//! Sets the `requestFullContextOnNextMessage` flag on the chat so the next turn
//! bypasses context compression and rebuilds the full, uncompressed context. Takes
//! no parameters — it only signals intent.
//!
//! ## The write, without touching `db/chats.rs`
//!
//! v4 does `repos.chats.update(chatId, { requestFullContextOnNextMessage: true })`
//! — a single-column partial `SET` that (per the chats port) does **not** mint
//! `updatedAt`. That is byte-identical to a standalone `UPDATE chats SET
//! "requestFullContextOnNextMessage" = 1 WHERE id = ?`, so this handler issues that
//! write directly instead of adding a `ChatUpdate` setter (which lives in
//! `db/chats.rs`, owned by a parallel unit this round). The differential dumps the
//! full chats row on both sides to prove the equivalence (the flag flips to 1, every
//! other column — `updatedAt` included — is preserved).

use serde::Serialize;
use serde_json::Value;

use crate::db::runtime::Db;

/// The success message v4 returns verbatim (Roaring-20s register kept as-is).
const SUCCESS_MESSAGE: &str = "Full context will be provided on your next response. The next message will include the complete, uncompressed conversation history and system prompt.";

/// v4 `RequestFullContextToolOutput` — `{ success, message }`.
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct RequestFullContextOutput {
    pub success: bool,
    pub message: String,
}

/// v4 `validateRequestFullContextInput` — `z.object({})` `safeParse`: any plain
/// object is valid (extra keys allowed); a non-object (string/number/null/array)
/// fails. serde arrays are `Value::Array`, not `Value::Object`, so `is_object()`
/// matches Zod's `!Array.isArray` guard.
fn validate_input(args: &Value) -> bool {
    args.is_object()
}

/// Execute the `request_full_context` tool (v4 `executeRequestFullContextTool`).
pub async fn execute_request_full_context(
    db: &Db,
    chat_id: &str,
    args: &Value,
) -> RequestFullContextOutput {
    if !validate_input(args) {
        return RequestFullContextOutput {
            success: false,
            message: "Invalid input format".to_string(),
        };
    }

    let chat_id = chat_id.to_string();
    let write = db
        .write(move |writers| {
            writers.main().connection().execute(
                "UPDATE chats SET \"requestFullContextOnNextMessage\" = 1 WHERE id = ?1",
                rusqlite::params![chat_id],
            )?;
            Ok(())
        })
        .await;

    match write {
        Ok(()) => RequestFullContextOutput {
            success: true,
            message: SUCCESS_MESSAGE.to_string(),
        },
        // v4's catch: `error.message` (the DbError Display string here).
        Err(e) => RequestFullContextOutput {
            success: false,
            message: e.to_string(),
        },
    }
}

/// v4 `formatRequestFullContextResults` — the message verbatim.
pub fn format_request_full_context(out: &RequestFullContextOutput) -> String {
    out.message.clone()
}
