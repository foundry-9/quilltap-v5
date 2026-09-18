//! **P4.102 — the shared decoder for tri-state REST edges.** Generalizes
//! P4.98's `images_generate_request` (`images_routes.rs`) into one helper
//! every REST edge whose `Request` variant carries `Option<Option<Value>>`
//! fields can use, so the absent / explicit-`null` / value tri-state has
//! exactly one implementation and a transport-specific edge cannot drift from
//! `POST /api/dispatch`'s own decode (`dispatch.rs:69`,
//! `serde_json::from_slice::<Request>`).
//!
//! Before this helper existed, an edge that needed v4's raw body (to answer
//! v4's `{error: 'Validation error', details}` envelope for a non-object
//! body, which the flat dispatch variant cannot express) hand-built its
//! `Request` variant from `map.get(key).cloned()` — a SECOND spelling of the
//! tri-state (`tri()` in `subprompts_routes.rs` / `prompt_templates_routes.rs`
//! before this port) or a bespoke `.cloned()` walk (`images_routes.rs` before
//! P4.98). The two spellings drifted: from P4.76 until P4.98 the images edge
//! preserved an explicit `null` correctly while the dispatch decode collapsed
//! it to absent, so `{"chatId": null}` answered two different things on the
//! two transports. Lifting the same keys into a dispatch-shaped envelope and
//! running the SAME `serde_json::from_value::<Request>` decode both transports
//! run closes the class at the root: there is nothing left to reimplement.
//!
//! See the memory note `a-rest-edge-that-shares-the-dispatch-decoder-cannot-
//! drift` and P4.98's `images_generate_decoder_tests`, whose pin
//! (`images_edge_and_dispatch_decode_the_five_keys_identically`) is the proof
//! this generalization moved nothing: `images_generate_request` is now a
//! one-line call onto this helper and stays green.

use serde_json::Value;

use quilltap_core::api::Request as CoreRequest;

/// Lift `body_keys` out of a parsed request body (PRESENT-ness carries,
/// `null` included) and `path_fields` (the URL-sourced ids, always present —
/// `characterId`, `id`, …) into a dispatch-shaped `{"type": kind, ...}`
/// envelope, then decode it through the exact same
/// `serde_json::from_value::<Request>` call `POST /api/dispatch` runs.
///
/// A body that PARSES but is not a JSON object still reaches v4's Zod parse,
/// which refuses it — so a non-object `parsed` folds to all-absent here (no
/// body keys are lifted) and the caller's HANDLER answers v4's refusal, not
/// this function. Unknown keys are dropped, which is v4-faithful: its route
/// schemas are `z.object`, and a `z.object` STRIPS undeclared keys rather
/// than refusing them.
///
/// The result is `Option<CoreRequest>`; `None` is unreachable while every
/// lifted field on the target variant stays a raw `Option<Option<Value>>` —
/// every JSON shape decodes. A `None` here means a variant's field was
/// re-typed to something serde can reject, at which point the refusal
/// belongs to the DECODE again and this edge would be answering serde's
/// sentence where v4 answers Zod's. `tri_state_edges_share_the_decoder.rs`'s
/// census guard is what keeps that from happening silently: no `*_routes.rs`
/// may hand-build a `Request` variant carrying `Option<Option<…>>` fields
/// except through this function.
pub fn request_envelope(
    kind: &str,
    parsed: &Value,
    body_keys: &[&str],
    path_fields: &[(&str, Value)],
) -> Option<CoreRequest> {
    let mut envelope = serde_json::Map::new();
    envelope.insert("type".into(), Value::String(kind.to_string()));
    if let Value::Object(map) = parsed {
        for key in body_keys {
            // PRESENT-ness is what carries: a key holding `null` must be
            // inserted as `null`, not skipped (v4 bug 130's `chatId` — the
            // `IMAGES_GENERATE_RAW_FIVE` lesson, generalized).
            if let Some(v) = map.get(*key) {
                envelope.insert(key.to_string(), v.clone());
            }
        }
    }
    // The URL-sourced ids go in LAST, so they win: v4's route schemas are
    // `z.object`s over the BODY keys alone and the path param is read from the
    // URL, so a body that also spells `characterId` can never redirect the
    // write. Inserting them first would let a same-named body key overwrite
    // them (the `baa85e19b` round's §3 review; pinned by
    // `a_body_key_spelling_a_path_id_cannot_overwrite_the_url`).
    for (key, value) in path_fields {
        envelope.insert((*key).to_string(), value.clone());
    }
    serde_json::from_value::<CoreRequest>(Value::Object(envelope)).ok()
}
