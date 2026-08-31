//! Re-deciding the attachment question after the model changes underneath a
//! message array — v4 `lib/chat/message-attachment-adapter.ts` (`a1d88aa3a`,
//! bug 106).
//!
//! A formatted message array is built once, against one connection profile.
//! `process_file_attachment_fallback` runs at that moment and answers, for that
//! profile: send the raw bytes, or replace them with a description? Whichever
//! it answers is baked into the array — raw attachments on the anchor message,
//! or description text prepended to its content.
//!
//! Every mid-turn model swap inherits that array. The uncensored reroute is the
//! one that bit v4: a vision profile's array, bytes and all, handed to a
//! text-only substitute, which the gateway refuses with a 400 before the remedy
//! has a chance to run. The failure is structural rather than unlucky — the
//! answer in the array was computed for a model that is no longer the one being
//! called.
//!
//! [`adapt_messages_for_profile`] asks the question again, for the profile
//! actually about to be called. An image a text-only stand-in cannot read
//! becomes its description, exactly as it would have if that profile had been
//! the primary, and the retry proceeds instead of dying at the gateway. A
//! profile that *can* take the bytes gets the array back untouched.
//!
//! ## The same-reference contract, in Rust
//!
//! v4 returns the **same array reference** when nothing needs changing, and
//! says so in its doc comment because callers compare. v5 has no reference
//! identity to hand back, so the contract is expressed in the type instead:
//! [`adapt_messages_for_profile`] returns `Option<Vec<StreamMessage>>` and
//! `None` IS "unchanged — keep what you had". A caller cannot accidentally
//! spend a clone on the common case, and the contract is checkable rather than
//! documented.
//!
//! ## One recorded narrowing: absent vs empty `attachments`
//!
//! v4 ends a rewritten message with `delete next.attachments` when nothing is
//! kept, so its arrays distinguish "the key is gone" from "the key is an empty
//! list". `StreamMessage::User.attachments` is a `Vec` and structurally cannot.
//! Nothing observes the difference — every request builder reads the list with
//! JS truthiness, so `[]` and absent reach the wire identically — and the
//! `file_attachment_tier3` differential's (E) family collapses the two on BOTH
//! sides so the rest of its projection stays discriminating.

use serde_json::Value;

use crate::model::completion::CompletionProvider;
use crate::model::stream::StreamMessage;
use crate::services::file_fallback::{
    self, format_fallback_as_message_prefix, needs_fallback_processing, FallbackDeps, FallbackFile,
};

/// One attachment as the file loader produces it — v4's `LoadedAttachment`.
/// Only `id` and `mimeType` are structurally required (v4's `isLoadedAttachment`
/// type guard tests exactly those two, both `typeof === 'string'`).
struct LoadedAttachment<'a> {
    id: &'a str,
    filename: &'a str,
    mime_type: &'a str,
    data: Option<&'a str>,
}

/// v4 `isLoadedAttachment(value)` — an object with string `id` AND string
/// `mimeType`. Anything else is "a caller's own shape" and is carried through
/// untouched.
fn as_loaded_attachment(value: &Value) -> Option<LoadedAttachment<'_>> {
    let id = value.get("id").and_then(Value::as_str)?;
    let mime_type = value.get("mimeType").and_then(Value::as_str)?;
    Some(LoadedAttachment {
        id,
        // v4's interface types `filename` as a required string but the guard
        // does not check it; a bag without one reaches `processFileAttachment-
        // Fallback` as `undefined` and lands in the metadata as such.
        filename: value.get("filename").and_then(Value::as_str).unwrap_or(""),
        mime_type,
        data: value.get("data").and_then(Value::as_str),
    })
}

/// Every MIME type riding in a message array's attachments, de-duplicated —
/// v4 `collectAttachmentMimeTypes`.
///
/// The routing question ("which substitute should we even offer?") is asked
/// before a profile is in hand, so it needs the payload's shape rather than a
/// per-profile verdict. Feed this to
/// [`crate::services::dangerous_content::provider_routing::resolve_provider_for_dangerous_content`].
///
/// Insertion-ordered like v4's `Array.from(new Set(…))`: only `StreamMessage::
/// User` ever carries attachments in v5, which is the same set v4 walks (its
/// loop reads `message.attachments ?? []` on every role, and nothing but the
/// user message ever has one).
pub fn collect_attachment_mime_types(messages: &[StreamMessage]) -> Vec<String> {
    let mut seen: Vec<String> = Vec::new();
    for message in messages {
        for attachment in message.attachments() {
            if let Some(a) = as_loaded_attachment(attachment) {
                if !seen.iter().any(|s| s == a.mime_type) {
                    seen.push(a.mime_type.to_string());
                }
            }
        }
    }
    seen
}

/// Re-run the attachment decision for `profile` and return an array it can
/// actually accept — v4 `adaptMessagesForProfile`.
///
/// Returns `None` when nothing needs changing, which is the overwhelmingly
/// common case: no attachments at all, or a substitute that reads the same
/// things the original did. Only a genuine mismatch costs a describer call.
///
/// Never fails. A describer that fails leaves the bytes dropped and a
/// `⚠️ Attachment Processing Failed` note in their place — a degraded turn the
/// model can still answer, which is strictly better than the 400 that dropping
/// this step guarantees. (v5's `process_file_attachment_fallback` returns that
/// note as a `FallbackResult` rather than throwing, so v4's try/catch arm has
/// no separate v5 shape; the prefix bytes are identical either way.)
pub async fn adapt_messages_for_profile<CMP: CompletionProvider>(
    messages: &[StreamMessage],
    profile: &Value,
    deps: &FallbackDeps<'_, CMP>,
    chat_id: &str,
) -> Option<Vec<StreamMessage>> {
    let needs_work = messages.iter().any(|m| {
        m.attachments().iter().any(|a| {
            as_loaded_attachment(a).is_some_and(|a| needs_fallback_processing(profile, a.mime_type))
        })
    });
    if !needs_work {
        return None;
    }

    // Hoisted: inside `tracing::info!` the macro brings `tracing::field::Value`
    // into scope, so a `Value::as_str` path in an argument no longer names
    // `serde_json::Value`.
    let str_field = |k: &str| profile.get(k).and_then(Value::as_str).unwrap_or("");
    let (profile_id, profile_name, provider, model_name) = (
        str_field("id"),
        str_field("name"),
        str_field("provider"),
        str_field("modelName"),
    );
    tracing::info!(
        chat_id,
        profile_id,
        profile_name,
        provider,
        model_name,
        "[Attachment] Re-deciding attachments for a substituted profile"
    );

    let mut adapted: Vec<StreamMessage> = Vec::with_capacity(messages.len());
    for message in messages {
        let attachments = message.attachments();
        if attachments.is_empty() {
            adapted.push(message.clone());
            continue;
        }

        let mut keep: Vec<Value> = Vec::new();
        let mut prefix = String::new();

        for attachment in attachments {
            let Some(a) = as_loaded_attachment(attachment) else {
                // Not ours to reason about — a caller's own shape. Leave it
                // alone.
                keep.push(attachment.clone());
                continue;
            };
            if !needs_fallback_processing(profile, a.mime_type) {
                keep.push(attachment.clone());
                continue;
            }

            // v4 also rebuilds `filepath` (`?? \`/api/v1/files/${id}\``) and
            // `size` into the metadata it hands the fallback. v5's
            // [`FallbackFile`] carries neither: `process_file_attachment_fallback`
            // reads id / filename / mimeType / data and nothing else, and the
            // `fb` family of this same differential proves the results are
            // identical without them. Recorded rather than carried, so the
            // narrowing is a measurement instead of an omission.
            let file = FallbackFile {
                id: a.id.to_string(),
                filename: a.filename.to_string(),
                mime_type: a.mime_type.to_string(),
                data: a.data.map(str::to_string),
            };
            let result =
                file_fallback::process_file_attachment_fallback(deps, &file, profile).await;
            prefix.push_str(&format_fallback_as_message_prefix(&result));
            // Mirror `load_and_process_files`: the bytes ride along only when
            // the profile natively supports them, which `needs_fallback_
            // processing` has already said it does not. Anything else here — a
            // description, inlined text, or a failed describe — replaces them.
        }

        adapted.push(with_content_and_attachments(message, &prefix, keep));
    }

    Some(adapted)
}

/// v4's `{ ...message, content: prefix + message.content }` plus the
/// `keep.length > 0 ? next.attachments = keep : delete next.attachments`
/// pair. Every other field rides through untouched, which is why this
/// reconstructs the variant rather than building a fresh `user`.
fn with_content_and_attachments(
    message: &StreamMessage,
    prefix: &str,
    keep: Vec<Value>,
) -> StreamMessage {
    match message {
        StreamMessage::User {
            content,
            cache_control,
            ..
        } => StreamMessage::User {
            content: format!("{prefix}{content}"),
            cache_control: cache_control.clone(),
            attachments: keep,
        },
        // No other variant can carry attachments (`StreamMessage::attachments`
        // answers `&[]`), so the loop above never reaches here with a non-empty
        // list — but the prefix still belongs on the content if it ever did.
        other => other.clone(),
    }
}
