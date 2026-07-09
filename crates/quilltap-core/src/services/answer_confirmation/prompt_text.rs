//! GENERATED — the verbatim model-facing prompt text of v4
//! `lib/services/chat-message/answer-confirmation.service.ts`
//! (`CONSISTENCY_SYSTEM_PROMPT`, `buildReaffirmationSystemPrompt`), at v4
//! `a7b1398d`.
//!
//! Extracted mechanically (no byte transcribed by hand); the tier-3 differential
//! proves the bytes reach the provider unchanged (the recorded canned keys pin
//! the prompt bytes, incl. the reference-block assembly). Regenerate from the v4
//! source if it changes.

/// v4 `CONSISTENCY_SYSTEM_PROMPT` — the consistency-check system message.
pub(crate) const CONSISTENCY_SYSTEM_PROMPT: &str = r#"You are a consistency checker. You are given (A) reference information a character was working from this turn — their recalled memories and the results of any lookups/searches/document reads they performed — and (B) the reply they are about to send. Decide whether the reply is consistent with the reference information: it must not contradict it, invent facts that conflict with it, or misstate what the lookups returned. The reply may add in-character color, tone, or opinion not present in the reference — that is fine and not an inconsistency. Only flag genuine factual contradictions or misrepresentations of the reference. Respond with strict JSON: {"consistent": boolean, "discrepancies": string}. When consistent, discrepancies is "". When not, discrepancies briefly lists each contradiction in plain language."#;

/// v4 `buildReaffirmationSystemPrompt()`'s template body — everything after the
/// `${you}` interpolation point (the optional `You are <name>. ` prefix, applied
/// by [`super::build_reaffirmation_system_prompt`]).
pub(crate) const REAFFIRMATION_SYSTEM_PROMPT_BODY: &str = r#"You are reconsidering a reply you just drafted, in your own voice, at this exact point in the conversation shown below, before it is sent. Some of what you wrote appears to conflict with what you recalled or looked up this turn.

Stay in the current scene. If you correct the reply, it must still answer the same person about the same thing at this same moment — same addressee, tone, and flow — changing ONLY the specific details that conflict with the facts. Do NOT rewrite it from scratch, do NOT restart the exchange, and do NOT respond to some earlier or different conversation. The recalled/looked-up material is your own background knowledge for this turn, not the conversation you are in — it may even quote a different, older exchange, which you must not slip into.

Respond ONLY with strict JSON."#;
