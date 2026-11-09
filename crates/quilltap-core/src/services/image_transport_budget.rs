//! One image transport budget on the way to a provider — v4
//! `lib/chat/image-transport-budget.ts` (`29c1e2690`, at the `624d0d25c` pin),
//! with `d439f6dc9`'s per-request image cap spent as its count.
//!
//! A Salon turn can carry images from three sources — this turn's uploads,
//! uploads the character has not yet been shown (bug 121's re-hydration), and
//! Lantern images posted since the character last spoke — and every one of
//! them costs the request three things: a slot under the profile's per-request
//! image cap ([`resolve_max_images_per_request`]), base64 bytes under the
//! provider's request-body limit, and a model that can read it at all. Those
//! three questions used to be answered in five places, each locally sensible and
//! together uneven: the byte budget covered Lantern images only (bug 151's 413
//! was still reachable with four uploads beside two portraits), the cap was
//! enforced twice with opposite tie-breaks, an image past the byte budget
//! vanished without a word, and a failover understudy had its count re-checked
//! but never its bytes.
//!
//! [`apply_image_transport_budget`] answers all three once, on the final
//! message array, for whichever profile is about to be called — the primary
//! from the context builder, and each substitute from the failover service — so
//! an understudy gets the full policy for free.
//!
//!  1. **Capability.** An attachment the profile cannot receive is put through
//!     the describe-fallback whatever its rank: an image becomes its
//!     description, a text file is inlined. This is the only place the
//!     capability rule is written for images; the loaders only tag.
//!  2. **Rank.** Receivable images are ranked by provenance and age, never by
//!     array position: this turn's uploads newest first, then re-hydrated
//!     uploads newest first, then Lantern images newest first. An attachment
//!     without a `transport` tag ranks after all of them, in array order — a
//!     caller that forgets to tag loses priority, never correctness.
//!  3. **Count, then bytes.** The cap and then [`LLM_TRANSPORT_TOTAL_BASE64`]
//!     are spent down the ranked list. Over the byte budget the walk continues,
//!     so a smaller image further down may still fit.
//!  4. **Overflow is described or dropped with a note — never silently.** A
//!     `describable` image (an upload, or a generated image whose prompt is on
//!     file) becomes its description; anything else, or a description that
//!     fails, is replaced by [`IMAGE_OMITTED_NOTE`] so the model and the
//!     LLM-log reader both see that something was there.
//!
//! Returns `None` — v4's **same array reference** — when nothing changes, which
//! is the overwhelmingly common case. Never fails: a describer that fails
//! leaves a note where the bytes were.
//!
//! The per-image shrink is not here: it needs the raw buffer, and stays at load
//! time in `chat_files::load_chat_files_for_llm`. Nothing here touches a stored
//! file.
//!
//! ## v5 shapes
//!
//! * v4's verdict `Map` is keyed by attachment OBJECT identity. Every
//!   attachment on a final array is a distinct object (each loader builds
//!   fresh objects, the Lantern ids are de-duplicated by the walk, and uploads
//!   and re-hydrations are disjoint by construction — the unseen walk stops at
//!   this turn), so [`plan_image_transport`]'s POSITIONAL
//!   `[message][attachment]` grid answers exactly what the `Map` would.
//! * The pass is generic over [`AttachmentBearingMessage`] — v4's
//!   "deliberately loose" message shape — so the primary path keeps
//!   [`FormattedMsg`]'s exact absent-vs-empty `attachments`, and the failover
//!   keeps [`StreamMessage`] (one recorded narrowing, on its impl).
//! * v4's two describers can throw; v5's never fail. [`TransportDescriber`]
//!   answers `Result` so v4's two WARN arms are ported and reachable (see its
//!   doc for why production never reaches them).
//!
//! Design of record: v4 `docs/developer/features/refactor-4-12-image-transport-budget.md`.

use std::borrow::Cow;
use std::future::Future;

use serde::Serialize;
use serde_json::{Map, Value};

use crate::db::runtime::Db;
use crate::files::image_transport::{profile_can_receive_attachment, AttachmentProfileView};
use crate::files::llm_image_budget::LLM_TRANSPORT_TOTAL_BASE64;
use crate::model::completion::CompletionProvider;
use crate::model::stream::StreamMessage;
use crate::services::connection_profiles::resolve_max_images_per_request;
use crate::services::file_fallback::{
    self, format_fallback_as_message_prefix, FallbackDeps, FallbackFile, FallbackResult,
    FallbackType,
};
use crate::services::message_context::FormattedMsg;

// ===========================================================================
// Provenance
// ===========================================================================

/// Where an attachment on the final array came from — v4 `ImageTransportOrigin`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ImageTransportOrigin {
    Upload,
    Rehydrated,
    Lantern,
}

impl ImageTransportOrigin {
    /// v4's string literal.
    pub fn as_str(self) -> &'static str {
        match self {
            ImageTransportOrigin::Upload => "upload",
            ImageTransportOrigin::Rehydrated => "rehydrated",
            ImageTransportOrigin::Lantern => "lantern",
        }
    }

    /// v4 `ORIGIN_RANK` — upload 0, rehydrated 1, lantern 2.
    pub fn rank(self) -> u8 {
        match self {
            ImageTransportOrigin::Upload => 0,
            ImageTransportOrigin::Rehydrated => 1,
            ImageTransportOrigin::Lantern => 2,
        }
    }

    fn from_str(s: &str) -> Option<Self> {
        match s {
            "upload" => Some(ImageTransportOrigin::Upload),
            "rehydrated" => Some(ImageTransportOrigin::Rehydrated),
            "lantern" => Some(ImageTransportOrigin::Lantern),
            _ => None,
        }
    }
}

/// v4 `UNTAGGED_RANK` — an attachment with no `transport` tag ranks after every
/// tagged one.
pub const UNTAGGED_RANK: u8 = 3;

/// Transport-only provenance, set by the loader that built the attachment and
/// read by nothing but this pass — v4 `ImageTransportTag`. Stripped before the
/// attachment reaches a plugin or a log ([`strip_transport_tag`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ImageTransportTag {
    pub origin: ImageTransportOrigin,
    /// Epoch ms of the message (or upload) that carried it; newer ranks higher.
    pub created_at: i64,
    /// True when describing this image past the budget is worth a describer
    /// call: the operator's own uploads always, and a generated image only when
    /// its prompt or a description is on file (free to reuse). A vault-stored
    /// avatar comes back from the loader with a mount-link id that the
    /// persisted-description lookup cannot find, so it is not describable until
    /// a lookup by link id exists, and past the budget it is dropped with a
    /// note rather than spend a vision call on every turn.
    pub describable: bool,
}

impl ImageTransportTag {
    /// The tag's JSON — `{origin, createdAt, describable}`, in THAT order (all
    /// three of v4's callers write the literal in that order).
    pub fn to_json(&self) -> Value {
        let mut m = Map::new();
        m.insert("origin".into(), Value::String(self.origin.as_str().into()));
        m.insert("createdAt".into(), Value::from(self.created_at));
        m.insert("describable".into(), Value::Bool(self.describable));
        Value::Object(m)
    }
}

/// Read an attachment's `transport` tag. Absent, or not an object with a known
/// `origin`, is `None` — untagged.
///
/// Tolerant by design and with no v4 counterpart for the malformed case: only
/// [`tag_for_transport`] ever writes a tag and the plugin boundary strips it, so
/// a malformed tag cannot exist. (v4 would rank one `ORIGIN_RANK[undefined]` —
/// a `NaN` comparator; that input is unreachable on both sides.) Within a
/// well-formed tag, a missing `describable` reads v4's `?? true`.
pub fn read_tag(attachment: &Value) -> Option<ImageTransportTag> {
    let t = attachment.get("transport")?.as_object()?;
    let origin = ImageTransportOrigin::from_str(t.get("origin")?.as_str()?)?;
    let created_at = t
        .get("createdAt")
        .and_then(|v| v.as_i64().or_else(|| v.as_f64().map(|f| f as i64)))
        .unwrap_or(0);
    let describable = t
        .get("describable")
        .and_then(Value::as_bool)
        .unwrap_or(true);
    Some(ImageTransportTag {
        origin,
        created_at,
        describable,
    })
}

/// Attach provenance to a loaded attachment — v4 `tagForTransport`, `{
/// ...attachment, transport: tag }`.
///
/// On an untagged attachment `transport` is APPENDED LAST. On one that already
/// carries a `transport` key, v4's spread copies the old key first and the
/// literal then overwrites it IN PLACE, so the key keeps its original position
/// with the new value — exactly what `Map::insert` does under `preserve_order`.
///
/// Only loaded attachment objects are ever tagged; a non-object (unreachable)
/// is treated as `{}`, so it comes back as `{transport}`.
pub fn tag_for_transport(attachment: Value, tag: &ImageTransportTag) -> Value {
    let mut m = match attachment {
        Value::Object(m) => m,
        _ => Map::new(),
    };
    m.insert("transport".into(), tag.to_json());
    Value::Object(m)
}

/// The attachment a plugin may see: `transport` removed — v4
/// `stripTransportTag`. Anything that is not an object comes back unchanged,
/// and so does an object with no tag (`Cow::Borrowed` — v4's same reference).
/// Every other key keeps its order (`shift_remove`, never `swap_remove`).
pub fn strip_transport_tag(attachment: &Value) -> Cow<'_, Value> {
    match attachment {
        Value::Object(m) if m.contains_key("transport") => {
            let mut out = m.clone();
            out.shift_remove("transport");
            Cow::Owned(Value::Object(out))
        }
        _ => Cow::Borrowed(attachment),
    }
}

/// [`strip_transport_tag`] over an attachment list — v4 `stripTransportTags`.
/// `Cow::Borrowed` when no attachment carries a tag (no clone).
pub fn strip_transport_tags(attachments: &[Value]) -> Cow<'_, [Value]> {
    if !attachments.iter().any(|a| a.get("transport").is_some()) {
        return Cow::Borrowed(attachments);
    }
    Cow::Owned(
        attachments
            .iter()
            .map(|a| strip_transport_tag(a).into_owned())
            .collect(),
    )
}

// ===========================================================================
// Shapes
// ===========================================================================

/// The subset of a formatted message this module reads — v4
/// `AttachmentBearingMessage`. Deliberately loose: callers hold richer shapes
/// and every extra field is carried through untouched by [`Self::rebuilt`].
pub trait AttachmentBearingMessage: Clone {
    /// v4 `message.attachments ?? []`.
    fn attachments(&self) -> &[Value];

    /// v4's `{ ...message, content: prefix + message.content }` plus
    /// `keep.length > 0 ? next.attachments = keep : delete next.attachments`.
    fn rebuilt(&self, prefix: &str, keep: Vec<Value>) -> Self;
}

/// The failover's shape.
///
/// RECORDED NARROWING — absent vs empty `attachments`: v4 ends a rewritten
/// message with `delete next.attachments` when nothing is kept, so its arrays
/// distinguish "the key is gone" from "the key is an empty list".
/// `StreamMessage::User.attachments` is a `Vec` and structurally cannot.
/// Nothing observes the difference — every request builder reads the list with
/// JS truthiness, so `[]` and absent reach the wire identically — and the
/// differentials collapse the two on BOTH sides for this shape. The primary
/// path's [`FormattedMsg`] keeps the distinction exactly.
impl AttachmentBearingMessage for StreamMessage {
    fn attachments(&self) -> &[Value] {
        StreamMessage::attachments(self)
    }

    fn rebuilt(&self, prefix: &str, keep: Vec<Value>) -> Self {
        match self {
            StreamMessage::User {
                content,
                cache_control,
                name,
                ..
            } => StreamMessage::User {
                content: format!("{prefix}{content}"),
                cache_control: cache_control.clone(),
                attachments: keep,
                // Carried, never reset: the participant name is part of the
                // request-prefix hash (P4.128).
                name: name.clone(),
            },
            // No other variant can carry attachments (`attachments()` answers
            // `&[]`), and the pass rebuilds only a message with a non-empty
            // list, so this arm is never reached.
            other => other.clone(),
        }
    }
}

/// The primary path's shape — EXACT: an empty `keep` deletes the key (`None`).
impl AttachmentBearingMessage for FormattedMsg {
    fn attachments(&self) -> &[Value] {
        self.attachments.as_deref().unwrap_or(&[])
    }

    fn rebuilt(&self, prefix: &str, keep: Vec<Value>) -> Self {
        FormattedMsg {
            role: self.role.clone(),
            content: format!("{prefix}{}", self.content),
            name: self.name.clone(),
            thought_signature: self.thought_signature.clone(),
            attachments: if keep.is_empty() { None } else { Some(keep) },
        }
    }
}

/// v4 `isLoadedAttachment(value)` — an object with a STRING `id` and a STRING
/// `mimeType`; returns the mime. Anything else is "a caller's own shape" and is
/// carried through untouched. (`data`'s type is not checked — see
/// [`base64_bytes`].)
fn loaded_mime(value: &Value) -> Option<&str> {
    let o = value.as_object()?;
    o.get("id")?.as_str()?;
    o.get("mimeType")?.as_str()
}

/// Every MIME type riding in a message array's `attachments`, de-duplicated —
/// v4 `collectAttachmentMimeTypes` (moved here from the deleted
/// attachment adapter, `adaptMessagesForProfile`'s module, body identical).
///
/// The routing question ("which substitute should we even offer?") is asked
/// before a profile is in hand, so it needs the payload's shape rather than a
/// per-profile verdict. Feed this to
/// [`crate::services::dangerous_content::provider_routing::resolve_provider_for_dangerous_content`].
/// Insertion-ordered, like v4's `Array.from(new Set(…))`.
pub fn collect_attachment_mime_types<M: AttachmentBearingMessage>(messages: &[M]) -> Vec<String> {
    let mut seen: Vec<String> = Vec::new();
    for message in messages {
        for attachment in message.attachments() {
            if let Some(mime) = loaded_mime(attachment) {
                if !seen.iter().any(|s| s == mime) {
                    seen.push(mime.to_string());
                }
            }
        }
    }
    seen
}

// ===========================================================================
// Policy
// ===========================================================================

/// v4 `ImageTransportPolicy`.
pub struct ImageTransportPolicy {
    /// Most images one request may carry ([`resolve_max_images_per_request`]).
    pub max_images: i64,
    /// Most base64 the kept images may sum to ([`LLM_TRANSPORT_TOTAL_BASE64`]).
    pub max_total_base64: usize,
    can_receive: Box<dyn Fn(&str) -> bool + Send + Sync>,
}

impl ImageTransportPolicy {
    /// May this profile be sent these bytes raw?
    pub fn can_receive(&self, mime_type: &str) -> bool {
        (self.can_receive)(mime_type)
    }

    /// An explicit policy — the differential's constructor (v4's tier-1 corpus
    /// hands `planImageTransport` a literal policy with a `canReceive` list).
    /// Production builds its policy with [`image_transport_policy_for`].
    pub fn explicit(
        max_images: i64,
        max_total_base64: usize,
        can_receive: impl Fn(&str) -> bool + Send + Sync + 'static,
    ) -> Self {
        Self {
            max_images,
            max_total_base64,
            can_receive: Box::new(can_receive),
        }
    }
}

/// The policy for a profile about to be called — v4 `imageTransportPolicyFor`.
/// This is [`resolve_max_images_per_request`]'s only production caller, as in
/// v4.
pub fn image_transport_policy_for(profile: &Value) -> ImageTransportPolicy {
    let provider = profile
        .get("provider")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();
    let supports_image_upload = profile.get("supportsImageUpload").and_then(Value::as_bool);
    ImageTransportPolicy {
        max_images: resolve_max_images_per_request(profile),
        max_total_base64: LLM_TRANSPORT_TOTAL_BASE64,
        can_receive: Box::new(move |mime| {
            profile_can_receive_attachment(
                AttachmentProfileView {
                    provider: &provider,
                    supports_image_upload,
                },
                mime,
            )
        }),
    }
}

/// What stands in for an image that was over the request budget and could not
/// be described — v4 `IMAGE_OMITTED_NOTE`. Plain and short: it reaches the
/// model.
pub const IMAGE_OMITTED_NOTE: &str = "[image omitted: over the request budget]";

// ===========================================================================
// Report
// ===========================================================================

/// v4 `TransportOutcome`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum TransportOutcome {
    Kept,
    Described,
    Dropped,
}

/// v4 `TransportReason`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum TransportReason {
    WithinBudget,
    Capability,
    Count,
    Bytes,
}

/// v4 `TransportReportEntry` — keys in v4's literal order.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TransportReportEntry {
    pub attachment_id: String,
    /// v4 `attachment.filename` VERBATIM — an `undefined` one is omitted, as
    /// `JSON.stringify` omits it.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub filename: Option<Value>,
    pub mime_type: String,
    /// v4 `transport?.origin ?? null`.
    pub origin: Option<&'static str>,
    pub base64_bytes: usize,
    pub outcome: TransportOutcome,
    pub reason: TransportReason,
}

/// v4 `TransportReport`.
#[derive(Clone, Debug, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TransportReport {
    pub entries: Vec<TransportReportEntry>,
    pub kept: usize,
    pub described: usize,
    pub dropped: usize,
    /// Base64 bytes of the images kept on the wire.
    pub base64_kept: usize,
}

// ===========================================================================
// The describer seam
// ===========================================================================

/// The two describers the pass calls — v4's `processFileAttachmentFallback`
/// (the capability arm) and `generateImageDescription` (the overflow arm).
///
/// `Err` is v4's THROW. v5's two describer functions return a
/// [`FallbackResult`] and never fail, so the production wrapper
/// [`FallbackDepsDescriber`] always answers `Ok`, and the pass's two WARN arms
/// (`[ImageTransport] Fallback failed …` with its `⚠️ Attachment Processing
/// Failed` prefix, `[ImageTransport] Describing an over-budget image failed …`
/// with the note) are unreachable from production — v4 reaches them only on an
/// unexpected throw (e.g. its describer's profile read). They are ported,
/// reachable through this seam, and pinned by the differential's scripted
/// describer, which answers `Err` exactly where v4's mock throws.
pub trait TransportDescriber {
    /// v4 `processFileAttachmentFallback(fileMetadataOf(a), stripTransportTag(a),
    /// profile, …)`.
    fn fallback(
        &self,
        file: &FallbackFile,
        profile: &Value,
    ) -> impl Future<Output = Result<FallbackResult, String>>;

    /// v4 `generateImageDescription(stripTransportTag(a), …)`.
    fn describe(&self, file: &FallbackFile)
        -> impl Future<Output = Result<FallbackResult, String>>;
}

/// The production describer: the two infallible v5 functions over the turn's
/// [`FallbackDeps`] (which carry v4's `describeOptions.chatId`).
pub struct FallbackDepsDescriber<'a, CMP: CompletionProvider>(pub &'a FallbackDeps<'a, CMP>);

impl<CMP: CompletionProvider> TransportDescriber for FallbackDepsDescriber<'_, CMP> {
    async fn fallback(
        &self,
        file: &FallbackFile,
        profile: &Value,
    ) -> Result<FallbackResult, String> {
        Ok(file_fallback::process_file_attachment_fallback(self.0, file, profile).await)
    }

    async fn describe(&self, file: &FallbackFile) -> Result<FallbackResult, String> {
        Ok(file_fallback::generate_image_description(self.0, file).await)
    }
}

/// The describer for a caller that holds no describer deps — test doubles only
/// (every production `walk_fallback_chain` caller passes its deps). v4's
/// `walkFallbackChain` runs the pass for every understudy unconditionally, so
/// v5 runs it too rather than skipping a re-decide; this describer then
/// answers every call `Err`, which lands in the pass's WARN `error` key (a
/// v5-only text that never reaches a user).
pub struct NoTransportDescriber;

impl TransportDescriber for NoTransportDescriber {
    async fn fallback(
        &self,
        _file: &FallbackFile,
        _profile: &Value,
    ) -> Result<FallbackResult, String> {
        Err("No transport describer is configured".to_string())
    }

    async fn describe(&self, _file: &FallbackFile) -> Result<FallbackResult, String> {
        Err("No transport describer is configured".to_string())
    }
}

/// The caller's extra log keys — v4's `deps.logContext`, spread FIRST into
/// every line: `{chatId, characterParticipantId}` (the primary pass),
/// `{chatId}` (the uncensored reroute), `{chatId, kind}` (each fallback
/// understudy). An absent key is omitted, as v4's spread omits it.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct TransportLogContext {
    pub chat_id: Option<String>,
    pub character_participant_id: Option<String>,
    pub kind: Option<String>,
}

impl TransportLogContext {
    /// `{chatId, characterParticipantId}` — the primary pass.
    pub fn primary(chat_id: &str, character_participant_id: &str) -> Self {
        Self {
            chat_id: Some(chat_id.to_string()),
            character_participant_id: Some(character_participant_id.to_string()),
            kind: None,
        }
    }

    /// `{chatId}` — the uncensored reroute.
    pub fn chat(chat_id: &str) -> Self {
        Self {
            chat_id: Some(chat_id.to_string()),
            ..Self::default()
        }
    }

    /// `{chatId, kind}` — a fallback-chain understudy.
    pub fn chain(chat_id: &str, kind: &str) -> Self {
        Self {
            chat_id: Some(chat_id.to_string()),
            character_participant_id: None,
            kind: Some(kind.to_string()),
        }
    }
}

// ===========================================================================
// The plan
// ===========================================================================

/// One attachment's fate — v4's `Verdict`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TransportVerdict {
    Keep,
    Capability,
    Overflow(OverflowReason),
}

/// Why an image overflowed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OverflowReason {
    Count,
    Bytes,
}

impl OverflowReason {
    fn report_reason(self) -> TransportReason {
        match self {
            OverflowReason::Count => TransportReason::Count,
            OverflowReason::Bytes => TransportReason::Bytes,
        }
    }
}

/// v4 `base64Bytes(a)` = `a.data?.length ?? 0` — JS `.length` exactly: a
/// string is its UTF-16 unit count (NOT `str::len`), an array its element
/// count, anything else (absent, `null`, a number, a boolean) `0`. v4's guard
/// does not check `data`'s type, so neither does this; an object-valued `data`
/// cannot come out of any loader and is counted `0`.
pub fn base64_bytes(attachment: &Value) -> usize {
    match attachment.get("data") {
        Some(Value::String(s)) => s.encode_utf16().count(),
        Some(Value::Array(a)) => a.len(),
        _ => 0,
    }
}

/// Decide every attachment's fate for `policy` — v4 `planImageTransport`.
/// Pure. Positional: `grid[message][attachment]`, `None` for an entry that is
/// not a loaded attachment (see the module note for why positions are v4's
/// object identities).
///
/// Ranking is v4's `rankImages` comparator — tag rank, then `createdAt`
/// DESCENDING, then array order — over a STABLE sort, so array order needs no
/// key of its own. The comparator is total for every tag [`tag_for_transport`]
/// can write, so V8's NaN-comparator behaviour has no path here.
pub fn plan_image_transport<M: AttachmentBearingMessage>(
    messages: &[M],
    policy: &ImageTransportPolicy,
) -> Vec<Vec<Option<TransportVerdict>>> {
    struct Located {
        mi: usize,
        ai: usize,
        rank: u8,
        created_at: i64,
        cost: usize,
    }

    let mut grid: Vec<Vec<Option<TransportVerdict>>> = messages
        .iter()
        .map(|m| vec![None; m.attachments().len()])
        .collect();
    let mut receivable_images: Vec<Located> = Vec::new();

    for (mi, message) in messages.iter().enumerate() {
        for (ai, attachment) in message.attachments().iter().enumerate() {
            let Some(mime) = loaded_mime(attachment) else {
                continue;
            };
            if !policy.can_receive(mime) {
                grid[mi][ai] = Some(TransportVerdict::Capability);
            } else if mime.starts_with("image/") {
                let tag = read_tag(attachment);
                receivable_images.push(Located {
                    mi,
                    ai,
                    rank: tag.map_or(UNTAGGED_RANK, |t| t.origin.rank()),
                    created_at: tag.map_or(0, |t| t.created_at),
                    cost: base64_bytes(attachment),
                });
            } else {
                grid[mi][ai] = Some(TransportVerdict::Keep);
            }
        }
    }

    receivable_images.sort_by(|a, b| {
        a.rank
            .cmp(&b.rank)
            .then_with(|| b.created_at.cmp(&a.created_at))
    });

    let mut kept: i64 = 0;
    let mut bytes_left = policy.max_total_base64;
    for img in &receivable_images {
        if kept >= policy.max_images {
            grid[img.mi][img.ai] = Some(TransportVerdict::Overflow(OverflowReason::Count));
            continue;
        }
        // STRICT `>`: an image that exactly spends what is left is kept. Over
        // the budget the walk CONTINUES — a smaller image further down may
        // still fit — and an over-count image above spent no bytes.
        if img.cost > bytes_left {
            grid[img.mi][img.ai] = Some(TransportVerdict::Overflow(OverflowReason::Bytes));
            continue;
        }
        bytes_left -= img.cost;
        kept += 1;
        grid[img.mi][img.ai] = Some(TransportVerdict::Keep);
    }

    grid
}

/// The order of `plan_image_transport`'s ranked walk, as `(message, attachment)`
/// positions — v4 `rankImages` projected for the tier-1 differential. Only
/// receivable images are ranked.
pub fn ranked_image_positions<M: AttachmentBearingMessage>(
    messages: &[M],
    policy: &ImageTransportPolicy,
) -> Vec<(usize, usize)> {
    let mut located: Vec<(usize, usize, u8, i64)> = Vec::new();
    for (mi, message) in messages.iter().enumerate() {
        for (ai, attachment) in message.attachments().iter().enumerate() {
            let Some(mime) = loaded_mime(attachment) else {
                continue;
            };
            if policy.can_receive(mime) && mime.starts_with("image/") {
                let tag = read_tag(attachment);
                located.push((
                    mi,
                    ai,
                    tag.map_or(UNTAGGED_RANK, |t| t.origin.rank()),
                    tag.map_or(0, |t| t.created_at),
                ));
            }
        }
    }
    located.sort_by(|a, b| a.2.cmp(&b.2).then_with(|| b.3.cmp(&a.3)));
    located.into_iter().map(|(mi, ai, _, _)| (mi, ai)).collect()
}

fn entry_for(
    attachment: &Value,
    outcome: TransportOutcome,
    reason: TransportReason,
) -> TransportReportEntry {
    TransportReportEntry {
        attachment_id: attachment
            .get("id")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string(),
        filename: attachment.get("filename").cloned(),
        mime_type: loaded_mime(attachment).unwrap_or("").to_string(),
        origin: read_tag(attachment).map(|t| t.origin.as_str()),
        base64_bytes: base64_bytes(attachment),
        outcome,
        reason,
    }
}

/// The [`FallbackFile`] handed to either describer, built from the STRIPPED
/// attachment (v4 hands `stripTransportTag(attachment)`, so no describer ever
/// sees a tag — v5's `FallbackFile` has no slot for one at all).
///
/// RECORDED NARROWING (carried from the deleted adapter): v4 also rebuilds
/// `filepath` (`?? \`/api/v1/files/${id}\``) and `size` into the metadata it
/// hands the capability fallback. v5's `FallbackFile` carries neither:
/// `process_file_attachment_fallback` reads id / filename / mimeType / data and
/// nothing else, and the `fb` family of `file_attachment_tier3` proves the
/// results identical without them.
fn fallback_file_of(attachment: &Value) -> FallbackFile {
    let stripped = strip_transport_tag(attachment);
    let s = |k: &str| {
        stripped
            .get(k)
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string()
    };
    FallbackFile {
        id: s("id"),
        filename: s("filename"),
        mime_type: s("mimeType"),
        data: stripped
            .get("data")
            .and_then(Value::as_str)
            .map(str::to_string),
    }
}

fn is_describable(attachment: &Value) -> bool {
    // v4 `transport?.describable ?? true` — an untagged image is put to the
    // describer, as before the budget existed.
    read_tag(attachment).is_none_or(|t| t.describable)
}

fn profile_str<'a>(profile: &'a Value, key: &str) -> Option<&'a str> {
    profile.get(key).and_then(Value::as_str)
}

/// Apply the one image transport policy to the final message array for the
/// profile about to be called — v4 `applyImageTransportBudget`. See the module
/// note for the rules.
///
/// `None` back is v4's `result === messages`: nothing needed changing, keep
/// what you had — and on that path NOTHING is logged (v4's early return sits
/// before both lines). Otherwise every message is rebuilt and the INFO
/// (overflow only) and DEBUG lines fire, on the CALLER's thread (no line here
/// runs inside a `Db::write` closure). Never panics, never fails.
///
/// `policy` `None` is v4's default parameter, [`image_transport_policy_for`].
pub async fn apply_image_transport_budget<M, D>(
    messages: &[M],
    profile: &Value,
    describer: &D,
    log_context: &TransportLogContext,
    policy: Option<ImageTransportPolicy>,
) -> (Option<Vec<M>>, TransportReport)
where
    M: AttachmentBearingMessage,
    D: TransportDescriber,
{
    let policy = policy.unwrap_or_else(|| image_transport_policy_for(profile));
    let verdicts = plan_image_transport(messages, &policy);
    let mut report = TransportReport::default();

    let needs_work = verdicts
        .iter()
        .flatten()
        .any(|v| matches!(v, Some(v) if *v != TransportVerdict::Keep));
    if !needs_work {
        for message in messages {
            for attachment in message.attachments() {
                if loaded_mime(attachment).is_some_and(|m| m.starts_with("image/")) {
                    report.entries.push(entry_for(
                        attachment,
                        TransportOutcome::Kept,
                        TransportReason::WithinBudget,
                    ));
                    report.kept += 1;
                    report.base64_kept += base64_bytes(attachment);
                }
            }
        }
        return (None, report);
    }

    // v4's `logContext` — the caller's keys, then the profile's four.
    let chat_id = log_context.chat_id.as_deref();
    let character_participant_id = log_context.character_participant_id.as_deref();
    let kind = log_context.kind.as_deref();
    let profile_id = profile_str(profile, "id");
    let profile_name = profile_str(profile, "name");
    let provider = profile_str(profile, "provider");
    let model_name = profile_str(profile, "modelName");

    let mut adapted: Vec<M> = Vec::with_capacity(messages.len());
    for (mi, message) in messages.iter().enumerate() {
        let attachments = message.attachments();
        if attachments.is_empty() {
            adapted.push(message.clone());
            continue;
        }

        let mut keep: Vec<Value> = Vec::new();
        let mut prefix = String::new();

        for (ai, attachment) in attachments.iter().enumerate() {
            let Some(verdict) = verdicts[mi][ai] else {
                // Not ours to reason about — a caller's own shape. Leave it
                // alone.
                keep.push(attachment.clone());
                continue;
            };
            let is_image = loaded_mime(attachment).is_some_and(|m| m.starts_with("image/"));
            let attachment_id = attachment.get("id").and_then(Value::as_str).unwrap_or("");

            match verdict {
                TransportVerdict::Keep => {
                    keep.push(attachment.clone());
                    if is_image {
                        report.entries.push(entry_for(
                            attachment,
                            TransportOutcome::Kept,
                            TransportReason::WithinBudget,
                        ));
                        report.kept += 1;
                        report.base64_kept += base64_bytes(attachment);
                    }
                }
                TransportVerdict::Capability => {
                    // The profile cannot take these bytes at all: describe or
                    // inline, exactly as if it had been the primary all along
                    // (bug 106). The bytes never ride along here, described or
                    // not.
                    let file = fallback_file_of(attachment);
                    match describer.fallback(&file, profile).await {
                        Ok(result) => {
                            prefix.push_str(&format_fallback_as_message_prefix(&result));
                            let described = result.type_ != FallbackType::Unsupported;
                            let outcome = if described {
                                TransportOutcome::Described
                            } else {
                                TransportOutcome::Dropped
                            };
                            report.entries.push(entry_for(
                                attachment,
                                outcome,
                                TransportReason::Capability,
                            ));
                            if described {
                                report.described += 1;
                            } else {
                                report.dropped += 1;
                            }
                        }
                        Err(error) => {
                            let filename = attachment
                                .get("filename")
                                .and_then(Value::as_str)
                                .unwrap_or("undefined");
                            prefix.push_str(&format!(
                                "⚠️ Attachment Processing Failed: {filename}\n{error}\n\n"
                            ));
                            report.entries.push(entry_for(
                                attachment,
                                TransportOutcome::Dropped,
                                TransportReason::Capability,
                            ));
                            report.dropped += 1;
                            let mime_type = loaded_mime(attachment).unwrap_or("");
                            tracing::warn!(
                                chatId = chat_id,
                                characterParticipantId = character_participant_id,
                                kind = kind,
                                profileId = profile_id,
                                profileName = profile_name,
                                provider = provider,
                                modelName = model_name,
                                attachmentId = attachment_id,
                                mimeType = mime_type,
                                error = error.as_str(),
                                "[ImageTransport] Fallback failed for an attachment the profile cannot receive; dropping it"
                            );
                        }
                    }
                }
                TransportVerdict::Overflow(reason) => {
                    // Over the count cap or the byte budget: the profile could
                    // read it but may not be sent it, so the describer is asked
                    // directly — the capability fallback would wave a readable
                    // image through.
                    let mut result: Option<FallbackResult> = None;
                    if is_describable(attachment) {
                        match describer.describe(&fallback_file_of(attachment)).await {
                            Ok(r) => result = Some(r),
                            Err(error) => {
                                tracing::warn!(
                                    chatId = chat_id,
                                    characterParticipantId = character_participant_id,
                                    kind = kind,
                                    profileId = profile_id,
                                    profileName = profile_name,
                                    provider = provider,
                                    modelName = model_name,
                                    attachmentId = attachment_id,
                                    error = error.as_str(),
                                    "[ImageTransport] Describing an over-budget image failed; omitting it with a note"
                                );
                            }
                        }
                    }
                    match result {
                        Some(r) if r.type_ == FallbackType::ImageDescription => {
                            prefix.push_str(&format_fallback_as_message_prefix(&r));
                            report.entries.push(entry_for(
                                attachment,
                                TransportOutcome::Described,
                                reason.report_reason(),
                            ));
                            report.described += 1;
                        }
                        _ => {
                            prefix.push_str(IMAGE_OMITTED_NOTE);
                            prefix.push_str("\n\n");
                            report.entries.push(entry_for(
                                attachment,
                                TransportOutcome::Dropped,
                                reason.report_reason(),
                            ));
                            report.dropped += 1;
                        }
                    }
                }
            }
        }

        adapted.push(message.rebuilt(&prefix, keep));
    }

    let overflow: Vec<&TransportReportEntry> = report
        .entries
        .iter()
        .filter(|e| matches!(e.reason, TransportReason::Count | TransportReason::Bytes))
        .collect();
    if !overflow.is_empty() {
        let over_count = overflow
            .iter()
            .filter(|e| e.reason == TransportReason::Count)
            .count();
        let over_bytes = overflow
            .iter()
            .filter(|e| e.reason == TransportReason::Bytes)
            .count();
        let described = overflow
            .iter()
            .filter(|e| e.outcome == TransportOutcome::Described)
            .count();
        let omitted = overflow
            .iter()
            .filter(|e| e.outcome == TransportOutcome::Dropped)
            .count();
        tracing::info!(
            chatId = chat_id,
            characterParticipantId = character_participant_id,
            kind = kind,
            profileId = profile_id,
            profileName = profile_name,
            provider = provider,
            modelName = model_name,
            maxImages = policy.max_images,
            maxTotalBase64 = policy.max_total_base64,
            base64Kept = report.base64_kept,
            kept = report.kept,
            overCount = over_count,
            overBytes = over_bytes,
            described = described,
            omitted = omitted,
            "[ImageTransport] Images over the request budget were described or omitted"
        );
    }
    let report_json = serde_json::to_string(&report).unwrap_or_default();
    tracing::debug!(
        chatId = chat_id,
        characterParticipantId = character_participant_id,
        kind = kind,
        profileId = profile_id,
        profileName = profile_name,
        provider = provider,
        modelName = model_name,
        reportJson = report_json.as_str(),
        "[ImageTransport] Transport report"
    );

    (Some(adapted), report)
}

// ===========================================================================
// The persisted-description probe
// ===========================================================================

/// Would describing this image be free? — v4 `hasPersistedImageDescription`
/// (`context-builder.service.ts:429-447`): the file row holds a generated
/// image's (revised) prompt, or a stored description — the same three fields,
/// trimmed, that `generate_image_description` reuses before any vision call
/// (one predicate, [`file_fallback::first_non_empty_trimmed`]).
///
/// v4 keeps it in the context builder; v5 keeps it beside the rules it feeds
/// so the Lantern walk and the pass share one module.
///
/// R5 (v4-faithful, no divergence): a vault-stored image arrives with a
/// mount-link id that the `files` table does not hold, so the lookup answers
/// `Ok(None)` → `false` silently, and past the budget such an image is OMITTED
/// with [`IMAGE_OMITTED_NOTE`] rather than described — model-visible. A read
/// failure logs v4's DEBUG (`{fileId, error}`, the bare message) and also
/// answers `false`. Never fails. The line fires on the caller's thread (the
/// read closure returns the error; nothing logs inside it).
pub(crate) async fn has_persisted_image_description(db: &Db, file_id: &str) -> bool {
    let id = file_id.to_string();
    match db.read_main(move |c| crate::db::files::FilesRepository::new(c).find_by_id(&id)) {
        Ok(Some(entry)) => file_fallback::first_non_empty_trimmed(&[
            entry.generation_revised_prompt.as_deref(),
            entry.generation_prompt.as_deref(),
            entry.description.as_deref(),
        ])
        .is_some(),
        Ok(None) => false,
        Err(e) => {
            let error = e.to_string();
            tracing::debug!(
                fileId = file_id,
                error = error.as_str(),
                "Persisted-description probe failed; treating the image as not describable"
            );
            false
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn tag(origin: ImageTransportOrigin, created_at: i64) -> ImageTransportTag {
        ImageTransportTag {
            origin,
            created_at,
            describable: true,
        }
    }

    #[test]
    fn the_tag_is_appended_last_in_v4s_key_order() {
        let a = json!({"id": "a", "mimeType": "image/png", "data": "xx"});
        let tagged = tag_for_transport(a, &tag(ImageTransportOrigin::Upload, 7));
        assert_eq!(
            serde_json::to_string(&tagged).unwrap(),
            r#"{"id":"a","mimeType":"image/png","data":"xx","transport":{"origin":"upload","createdAt":7,"describable":true}}"#
        );
    }

    /// v4's `{...a, transport}` on an already-tagged attachment keeps the key
    /// where it was (the spread copies it first; the literal overwrites it in
    /// place).
    #[test]
    fn retagging_keeps_the_keys_original_position() {
        let a = json!({"id": "a", "transport": {"origin": "lantern"}, "mimeType": "image/png"});
        let tagged = tag_for_transport(a, &tag(ImageTransportOrigin::Rehydrated, 3));
        assert_eq!(
            serde_json::to_string(&tagged).unwrap(),
            r#"{"id":"a","transport":{"origin":"rehydrated","createdAt":3,"describable":true},"mimeType":"image/png"}"#
        );
    }

    #[test]
    fn strip_borrows_when_untagged_and_keeps_key_order_when_not() {
        let untagged = json!({"id": "a", "mimeType": "image/png"});
        assert!(matches!(strip_transport_tag(&untagged), Cow::Borrowed(_)));
        let scalar = json!("x");
        assert!(matches!(strip_transport_tag(&scalar), Cow::Borrowed(_)));
        let tagged = json!({"id": "a", "transport": {"origin": "upload"}, "mimeType": "image/png", "data": "d"});
        let stripped = strip_transport_tag(&tagged);
        assert!(matches!(stripped, Cow::Owned(_)));
        assert_eq!(
            serde_json::to_string(&*stripped).unwrap(),
            r#"{"id":"a","mimeType":"image/png","data":"d"}"#
        );
        let list = vec![untagged.clone()];
        assert!(matches!(strip_transport_tags(&list), Cow::Borrowed(_)));
    }

    /// The primary path's shape deletes the key on an empty keep (v4's
    /// `delete next.attachments`); the failover's narrows it to `[]`.
    #[test]
    fn formatted_msg_deletes_the_key_and_stream_message_narrows_it() {
        let f = FormattedMsg {
            role: "user".into(),
            content: "look".into(),
            name: Some("Bob".into()),
            thought_signature: None,
            attachments: Some(vec![json!({"id": "a"})]),
        };
        let r = f.rebuilt("[d] ", Vec::new());
        assert_eq!(r.attachments, None);
        assert_eq!(r.content, "[d] look");
        assert_eq!(r.name.as_deref(), Some("Bob"));
        let r2 = f.rebuilt("", vec![json!({"id": "b"})]);
        assert_eq!(r2.attachments, Some(vec![json!({"id": "b"})]));
    }

    /// P4.128 (moved from the deleted adapter): the rebuild CARRIES the
    /// participant name (it rides the request-prefix hash — v4
    /// `cache-prefix-hashes.ts:80`), and every other field it does not own.
    #[test]
    fn the_rebuild_carries_the_participant_name() {
        let cache = json!({ "type": "ephemeral" });
        let original = StreamMessage::User {
            content: "look".into(),
            cache_control: Some(cache.clone()),
            attachments: vec![json!({ "id": "a" })],
            name: Some("Bob".into()),
        };
        let rebuilt = original.rebuilt("[desc] ", Vec::new());
        assert_eq!(
            rebuilt,
            StreamMessage::User {
                content: "[desc] look".into(),
                cache_control: Some(cache),
                attachments: Vec::new(),
                name: Some("Bob".into()),
            }
        );
    }

    #[test]
    fn base64_bytes_is_js_length() {
        assert_eq!(base64_bytes(&json!({"data": "ééé"})), 3);
        assert_eq!(base64_bytes(&json!({"data": "abc"})), 3);
        assert_eq!(base64_bytes(&json!({"data": [1, 2]})), 2);
        assert_eq!(base64_bytes(&json!({"data": 7})), 0);
        assert_eq!(base64_bytes(&json!({"data": null})), 0);
        assert_eq!(base64_bytes(&json!({})), 0);
    }

    /// R-I10: every upload in one send carries the same `createdAt`, so the
    /// tie ranks by array order.
    #[test]
    fn an_upload_tie_ranks_by_array_order() {
        let img = |id: &str| {
            tag_for_transport(
                json!({"id": id, "mimeType": "image/png", "data": "x"}),
                &tag(ImageTransportOrigin::Upload, 100),
            )
        };
        let msgs = vec![FormattedMsg {
            role: "user".into(),
            content: String::new(),
            name: None,
            thought_signature: None,
            attachments: Some(vec![img("a"), img("b"), img("c")]),
        }];
        let policy = ImageTransportPolicy::explicit(2, 100, |_| true);
        let grid = plan_image_transport(&msgs, &policy);
        assert_eq!(
            grid[0],
            vec![
                Some(TransportVerdict::Keep),
                Some(TransportVerdict::Keep),
                Some(TransportVerdict::Overflow(OverflowReason::Count))
            ]
        );
    }

    #[test]
    fn the_report_serialises_in_v4s_key_order() {
        let report = TransportReport {
            entries: vec![entry_for(
                &json!({"id": "a", "filename": "a.png", "mimeType": "image/png", "data": "xy"}),
                TransportOutcome::Kept,
                TransportReason::WithinBudget,
            )],
            kept: 1,
            described: 0,
            dropped: 0,
            base64_kept: 2,
        };
        assert_eq!(
            serde_json::to_string(&report).unwrap(),
            r#"{"entries":[{"attachmentId":"a","filename":"a.png","mimeType":"image/png","origin":null,"base64Bytes":2,"outcome":"kept","reason":"within-budget"}],"kept":1,"described":0,"dropped":0,"base64Kept":2}"#
        );
    }

    // -----------------------------------------------------------------------
    // The persisted-description probe's two silent-or-logged legs
    // -----------------------------------------------------------------------

    /// A scratch main partition; the dir rides out with the handle.
    fn probe_db() -> (Db, tempfile::TempDir) {
        let dir = tempfile::Builder::new()
            .prefix("qt-image-transport-probe-")
            .tempdir()
            .expect("tempdir");
        let db = Db::open_main(
            dir.path().join("main.db"),
            "dGVzdHBlcHBlcnRlc3RwZXBwZXJ0ZXN0cGVwcGVyMDE=",
        )
        .unwrap();
        (db, dir)
    }

    fn probe(db: &Db, file_id: &str) -> (bool, Vec<String>) {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        crate::test_support::captured_with(|| {
            rt.block_on(has_persisted_image_description(db, file_id))
        })
    }

    /// v4 `context-builder.service.ts:440-445` — a `files` read that THROWS
    /// answers `false` and logs exactly one DEBUG, v4's sentence with
    /// `{fileId, error}` (the bare message). A main partition with no `files`
    /// table is the read failure.
    #[test]
    fn a_failed_probe_answers_false_with_v4s_debug() {
        let (db, _scratch) = probe_db();
        let (got, lines) = probe(&db, "file-err-1");
        assert!(!got);
        let hits: Vec<&String> = lines
            .iter()
            .filter(|l| {
                l.contains(
                    "Persisted-description probe failed; treating the image as not describable",
                )
            })
            .collect();
        assert_eq!(hits.len(), 1, "exactly one DEBUG: {lines:#?}");
        let line = hits[0];
        assert!(line.starts_with("DEBUG "), "{line}");
        assert!(line.contains(" fileId=file-err-1"), "{line}");
        assert!(line.contains(" error=") && line.contains("files"), "{line}");
        assert_eq!(lines.len(), 1, "nothing else logs: {lines:#?}");
    }

    /// R5 — a mount-link id the `files` table does not hold answers `Ok(None)`
    /// → `false`, SILENTLY (v4's `findById` answers null; no catch fires).
    #[test]
    fn a_mount_link_id_answers_false_silently() {
        let (db, _scratch) = probe_db();
        let schema: Value =
            serde_json::from_str(include_str!("provisioning/fresh_schema.json")).unwrap();
        let ddl = schema["main"]
            .as_array()
            .unwrap()
            .iter()
            .filter_map(Value::as_str)
            .find(|s| s.starts_with("CREATE TABLE \"files\" ("))
            .unwrap()
            .to_string();
        db.write_blocking(move |w| {
            w.main().connection().execute_batch(&ddl)?;
            Ok(())
        })
        .unwrap();
        let (got, lines) = probe(&db, "mount-link-0001");
        assert!(!got);
        assert!(lines.is_empty(), "silent: {lines:#?}");
    }
}
