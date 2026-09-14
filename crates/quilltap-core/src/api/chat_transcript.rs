//! **The Salon's conditional transcript read** — v4 `app/api/v1/messages/
//! route.ts`'s `handleTranscript`, plus the plain listing beside it
//! (`handleListMessages`). Both from `5029075bb`.
//!
//! The whole point of the conditional is that it can answer *nothing changed*
//! without serializing a line of the conversation: the tab hands back the
//! `transcriptVersion` it last saw and gets `{unchanged: true}` when the
//! counter still agrees. v4's own words for why that is the feature rather
//! than an optimisation — one busy turn fires wardrobe, backdrop, whisper and
//! memory hints at the same `chats` topic, and a single Commonplace whisper
//! can run to 17 KB, so a hint storm must cost round trips, not payloads.
//!
//! The full body is byte-identical to the transcript embedded in the chat GET
//! because both come from [`super::transcript_projection::
//! project_chat_transcript`] — the drift that module exists to prevent.
//!
//! **v4's ownership arm on the single user v5 has.** v4 refuses with
//! `notFound('Chat')` when `!chat || chat.userId !== user.id` — "`notFound`
//! rather than `forbidden`, matching the per-message endpoints: a chat this
//! account does not own should not be distinguishable from one that does not
//! exist." v5 has exactly one user, so the second disjunct is not dead code to
//! be dropped but a real arm reachable by a row whose `userId` is not
//! `SINGLE_USER_ID` (an imported archive, a hand-edited row) — kept, and
//! pinned on both sides of the differential.
//!
//! `POST /api/v1/messages` (v4's SSE send) has NO COUNTERPART here, by the
//! locked boundary: sends are `chatSend` over dispatch and tokens arrive on
//! the `Event` channel. No 405 is invented as a "port" of it.

use serde_json::{json, Value};

use super::types::{ErrorKind, Response};
use crate::db::runtime::Db;
use crate::db::{chats_messages_read, chats_read, DbError};

/// v4's `knownVersion` gate, whole.
///
/// `knownVersionParam === null ? null : Number(param)` then
/// `knownVersion !== null && Number.isInteger(knownVersion) && knownVersion
/// === version`. Three things follow, and each is a case in the differential:
///
/// - a **float** (`1.5`) is a number but not an integer → full read;
/// - a **string** survives `Number()` only if it parses cleanly — `"3"` is 3,
///   `"abc"` is NaN, and `Number.isInteger(NaN)` is false → full read. (The
///   string→number conversion happens at the REST edge, which is where v4's
///   `Number()` lives; by the time a value reaches here over dispatch it is
///   already JSON-typed.)
/// - `null` and absent are the same answer → full read.
///
/// Never an error. v4 shrugs at every one of these and hands back everything,
/// which is the safe direction: a redundant full read costs a round trip, a
/// wrongly-trusted version costs the tab a message it never sees.
fn known_integer_version(known: Option<&Value>) -> Option<i64> {
    let n = known?.as_f64()?;
    // `Number.isInteger` — finite and with no fractional part. `as_f64` on a
    // JSON number cannot be NaN or infinite (JSON has no literal for either),
    // but the check is spelled out because it is v4's, and a future caller
    // could hand us one.
    if !n.is_finite() || n.fract() != 0.0 {
        return None;
    }
    Some(n as i64)
}

/// v4 `handleTranscript`.
pub fn chat_transcript(
    db: &Db,
    user_id: &str,
    chat_id: &str,
    known_version: Option<&Value>,
) -> Response {
    let chat_id_owned = chat_id.to_string();
    let chat = match db.read_main(move |conn| chats_read::find_by_id(conn, &chat_id_owned)) {
        Ok(Some(c)) => c,
        Ok(None) => return Response::error(ErrorKind::NotFound, "Chat not found"),
        Err(e) => {
            tracing::error!(chat_id, error = %e, "[Messages API v1] Error reading transcript");
            return Response::error(ErrorKind::Internal, "Failed to read transcript");
        }
    };
    // v4's `chat.userId !== user.id` half — see the module header for why this
    // is reachable on a single-user instance.
    if chat.get("userId").and_then(Value::as_str) != Some(user_id) {
        return Response::error(ErrorKind::NotFound, "Chat not found");
    }

    // Read the counter BEFORE projecting. The pairing the caller stores must
    // never claim a version newer than the rows beside it: a version read
    // after the projection could have moved on, and the tab would then be
    // answered "unchanged" for a write it has not seen. Reading first can only
    // cost an extra round trip later, which is the harmless direction.
    let id_for_version = chat_id.to_string();
    let version = match db.read_main(move |conn| {
        Ok(crate::db::chats::ChatsRepository::new(conn).get_transcript_version(&id_for_version))
    }) {
        Ok(v) => v,
        Err(e) => {
            tracing::error!(chat_id, error = %e, "[Messages API v1] Error reading transcript");
            return Response::error(ErrorKind::Internal, "Failed to read transcript");
        }
    };

    if let Some(known) = known_integer_version(known_version) {
        if known == version {
            tracing::debug!(chat_id, version, "[Messages API v1] Transcript unchanged");
            return Response::ChatTranscript(json!({
                "unchanged": true,
                "version": version,
            }));
        }
    }

    let participants = chat
        .get("participants")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let chat_id_s = chat_id.to_string();
    let projected = db.read_main(|main| {
        db.read_mount_index(|mount| {
            super::transcript_projection::project_chat_transcript(
                main,
                mount,
                &participants,
                &chat_id_s,
            )
        })
    });
    let projection = match projected {
        Ok(p) => p,
        Err(e) => {
            tracing::error!(chat_id, error = %e, "[Messages API v1] Error reading transcript");
            return Response::error(ErrorKind::Internal, "Failed to read transcript");
        }
    };

    let count = projection.messages.len();
    tracing::debug!(
        chat_id,
        version,
        known_version = ?known_version,
        count,
        "[Messages API v1] Transcript read"
    );
    Response::ChatTranscript(json!({
        "unchanged": false,
        "version": version,
        "messages": projection.messages,
        "offSceneCharacters": projection.off_scene_characters,
        "count": count,
    }))
}

/// v4 `handleListMessages` — the stored `type === 'message'` events,
/// UNPROJECTED (no attachments resolved, no off-scene cards). The Salon does
/// not read this; it is the lightweight listing the collection route serves
/// when no action is given.
pub fn chat_message_events(db: &Db, user_id: &str, chat_id: &str) -> Response {
    let chat_id_owned = chat_id.to_string();
    let chat = match db.read_main(move |conn| chats_read::find_by_id(conn, &chat_id_owned)) {
        Ok(Some(c)) => c,
        Ok(None) => return Response::error(ErrorKind::NotFound, "Chat not found"),
        Err(e) => {
            // v4's catch logs with an EMPTY context bag here — `logger.error(
            // '[Messages API v1] Error listing messages', {}, error)` — unlike
            // the transcript arm, which carries `{chatId}`. Reproduced.
            tracing::error!(error = %e, "[Messages API v1] Error listing messages");
            return Response::error(ErrorKind::Internal, "Failed to list messages");
        }
    };
    if chat.get("userId").and_then(Value::as_str) != Some(user_id) {
        return Response::error(ErrorKind::NotFound, "Chat not found");
    }

    let chat_id_owned = chat_id.to_string();
    let events: Result<Vec<Value>, DbError> =
        db.read_main(move |conn| chats_messages_read::get_messages(conn, &chat_id_owned));
    let events = match events {
        Ok(e) => e,
        Err(e) => {
            tracing::error!(error = %e, "[Messages API v1] Error listing messages");
            return Response::error(ErrorKind::Internal, "Failed to list messages");
        }
    };
    let message_events: Vec<Value> = events
        .into_iter()
        .filter(|e| e.get("type").and_then(Value::as_str) == Some("message"))
        .collect();
    let count = message_events.len();
    Response::ChatMessageEvents(json!({
        "messages": message_events,
        "count": count,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn v(n: f64) -> Value {
        json!(n)
    }

    /// v4's `Number.isInteger` gate, arm by arm. Everything that is not an
    /// integer reads as "no known version" — never as an error.
    #[test]
    fn only_an_integer_counts_as_a_known_version() {
        assert_eq!(known_integer_version(Some(&v(3.0))), Some(3));
        assert_eq!(known_integer_version(Some(&json!(0))), Some(0));
        assert_eq!(known_integer_version(Some(&v(-2.0))), Some(-2));
        // A float is a number but not an integer.
        assert_eq!(known_integer_version(Some(&v(1.5))), None);
        // Neither absent nor null is a version.
        assert_eq!(known_integer_version(None), None);
        assert_eq!(known_integer_version(Some(&Value::Null)), None);
        // A STRING never counts here: v4's `Number()` runs at the REST edge,
        // so by dispatch a version is already JSON-typed. `"3"` arriving as a
        // string is therefore a full read, not 3.
        assert_eq!(known_integer_version(Some(&json!("3"))), None);
        assert_eq!(known_integer_version(Some(&json!("abc"))), None);
        assert_eq!(known_integer_version(Some(&json!(true))), None);
        assert_eq!(known_integer_version(Some(&json!([3]))), None);
    }
}
