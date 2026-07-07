//! GENERATED — the verbatim model-facing prompt text of v4
//! `lib/services/chat-message/answer-confirmation.service.ts`
//! (`CONSISTENCY_SYSTEM_PROMPT`, `buildReaffirmationSystemPrompt`).
//!
//! Extracted mechanically (no byte transcribed by hand); the tier-3 differential
//! proves the bytes reach the provider unchanged (the recorded canned keys pin
//! the prompt bytes, incl. the reference-block assembly). Regenerate from the v4
//! source if it changes.

/// v4 `CONSISTENCY_SYSTEM_PROMPT` — the consistency-check system message.
pub(crate) const CONSISTENCY_SYSTEM_PROMPT: &str = r#"You are a consistency checker. You are given (A) reference information a character was working from this turn — their recalled memories and the results of any lookups/searches/document reads they performed — and (B) the reply they are about to send. Decide whether the reply is consistent with the reference information: it must not contradict it, invent facts that conflict with it, or misstate what the lookups returned. The reply may add in-character color, tone, or opinion not present in the reference — that is fine and not an inconsistency. Only flag genuine factual contradictions or misrepresentations of the reference. Respond with strict JSON: {"consistent": boolean, "discrepancies": string}. When consistent, discrepancies is "". When not, discrepancies briefly lists each contradiction in plain language."#;

/// v4 `buildReaffirmationSystemPrompt()` — the re-affirmation system message.
pub(crate) const REAFFIRMATION_SYSTEM_PROMPT: &str = r#"You are reconsidering a reply you just drafted, in your own voice, before it is sent. Some of what you wrote appears to conflict with what you actually know or looked up this turn. Respond ONLY with strict JSON."#;
