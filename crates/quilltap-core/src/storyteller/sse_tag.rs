//! Tag every frame one concurrent storyteller turn writes (v4
//! `lib/storyteller/sse-tag.ts`, P4.D343).
//!
//! When a round runs "all at once", several turns share one stream. Most
//! frames name their participant already, but content and status chunks do
//! not, so each concurrent turn writes through a wrapper that stamps
//! `participantId` and `concurrent: true` onto every frame. The Salon keeps one
//! streaming bubble per tagged participant.
//!
//! v4 re-encodes SSE text (`data: {json}` lines); v5's frames are typed
//! [`ChatEvent`](crate::services::chat_events::ChatEvent)s serialised at the
//! edge, so the merge is applied to the frame's JSON value — the same key
//! order: the original keys, then `participantId` only when absent, then
//! `concurrent`.

use serde_json::Value;

/// v4 `tagSseChunk`'s per-frame merge: a non-array object becomes `{...parsed,
/// participantId: parsed.participantId ?? participant_id, concurrent: true}`
/// (a key the frame already has keeps its position); anything else is
/// returned untouched.
pub fn tag_frame(frame: Value, participant_id: &str) -> Value {
    let Value::Object(mut map) = frame else {
        return frame;
    };
    let pid = match map.get("participantId") {
        Some(v) if !v.is_null() => v.clone(),
        _ => Value::String(participant_id.to_string()),
    };
    map.insert("participantId".to_string(), pid);
    map.insert("concurrent".to_string(), Value::Bool(true));
    Value::Object(map)
}
