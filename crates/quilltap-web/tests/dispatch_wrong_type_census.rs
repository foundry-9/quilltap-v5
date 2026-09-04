//! **P4.72 — the dispatch-level WRONG-TYPE census.**
//!
//! `POST /api/dispatch` decodes the `Request` enum with serde
//! (`quilltap-web/src/dispatch.rs:70-78`). When a variant carries a TYPED field
//! — `String`, `bool`, `Option<i64>`, `Vec<String>` — and a client sends a
//! present-but-wrong-typed value, serde rejects the whole body and v5 answers
//! `400 {"kind":"badRequest","message":"Invalid request: <serde text>"}`.
//!
//! v4 has no such single decode. Each route reads its body its own way, and
//! **five different things** can happen to a wrong-typed value there: a Zod
//! `.parse` throws and the context middleware answers
//! `{"error":"Validation error","details":[…]}`; a `.parse` caught locally
//! answers `{"error":"Invalid request body: …"}`; a `safeParse` answers the
//! route's own sentence — or, at seven sites, **no error at all**, the value
//! being silently coerced or dropped and the request answered 200. Where the
//! field comes from the query string or the URL path instead, a JSON type
//! cannot be wrong in v4 at all, because v4 never receives one.
//!
//! Nothing here is a fix. This is the **census**, in the `db_error_key_guard`
//! idiom: the class is enumerated mechanically from `api/types.rs` so it cannot
//! grow unnoticed, every row carries where v4 gets the value and what v4
//! answers, and the body-sourced rows are DRIVEN — decoded for real — so a
//! later round's fix reddens its row by name instead of quietly passing.
//!
//! ## Why the fix is not here
//!
//! The Tauri IPC transport dispatches the same `Request` enum, so a web-edge
//! mapping would leave the desktop app answering the serde envelope. The
//! transport-agnostic shape is the P4.D57 idiom — the field carries
//! `Option<Value>` / `Option<Option<Value>>` and the HANDLER refuses with v4's
//! sentence — which puts the change in `api/types.rs` and the handler files.
//! Those are P4.73's this round (§E of the shared contract), and beyond it the
//! remaining rows cross three lanes' ownership.
//!
//! ## The `ChatCreate` trio — and a correction to the order
//!
//! P4.72's order predicted that a non-string `conciergeState` /
//! `roleplayTemplateId` / non-object `timestampConfig` on `chatCreate` answers
//! `dispatch.rs:72`'s `Invalid request: <serde text>`. **Measured, it does
//! not.** `Request::ChatCreate` carries a raw
//! `serde_json::Map<String, Value>`, so serde accepts any object at the
//! dispatch boundary; the typed decode happens one level down, in the HOST
//! driver — `quilltap-host/src/spine.rs:1868-1878`, which composes
//! `ErrorKind::BadRequest` `invalid chatCreate request: <serde text>`. The
//! shape of the divergence is the same, but its home is not: the fields live on
//! `quilltap_core::services::chat_create::ChatCreateRequest`, and the sentence
//! belongs to the host. The trio's rows were marked `ORDERED(P4.73)` and driven
//! against `ChatCreateRequest` itself, so that P4.73's widening would redden
//! them here — and it did, at the follow-ups-round-2 unification (2026-09-04):
//! P4.73 landed the widening in the same round (`concierge_state` and
//! `roleplay_template_id` now carry `Option<Option<Value>>`, refused in the
//! handler with v4's flat `Validation error`, pinned by the capstone family's
//! `cs_wrong_type_400` / `rt_wrong_type_400` cases). The rows are therefore
//! `FIXED(P4.73)` and the probe below asserts the POST-fix shape: the decode
//! ACCEPTS a wrong type so the handler can refuse it.
//!
//! ## Nested request structs beyond the trio — a named deferral
//!
//! Several variants hand a second-level struct to serde (`PendingToolResult`,
//! `ChatCreateParticipant`, `AnnouncerSenderWire`, …). Their fields are NOT
//! enumerated: the mechanical half walks `Request`'s own fields only, which is
//! what makes it total and cheap. The `ChatCreate` trio is carried explicitly
//! because the order names it; every other nested struct is recorded here as
//! out of this census's scope, to be enumerated the same way when a round
//! needs it.
//!
//! Run standalone:
//!   cargo test -p quilltap-web --test dispatch_wrong_type_census

use std::path::{Path, PathBuf};

use serde_json::{json, Value};

/// Where v4 gets the value that v5 carries as a typed dispatch field — and
/// therefore what v4 answers when the JSON type is wrong.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum V4 {
    /// A URL path segment. Always a string in v4; a JSON type cannot be wrong.
    Path,
    /// `request.nextUrl.searchParams` — always a string. **No v4 counterpart**
    /// for a wrong JSON type; v5's typed field is the parse v4 does by hand.
    Query,
    /// A body key under an UNCAUGHT Zod `.parse` → the context middleware's
    /// `400 {"error":"Validation error","details":[…]}`.
    BodyParse,
    /// A body key under a `.parse` the route catches itself →
    /// `400 {"error":"Invalid request body: <messages>"}`, no `details`.
    BodyParseLocallyCaught,
    /// A body key under `safeParse` → the route's OWN sentence (they are not
    /// uniform; the note carries each one), or silence.
    BodySafeParse,
    /// A body key v4 reads raw and coerces or guards by hand.
    BodyHandRolled,
    /// No v4 counterpart at all: a multipart-only route, a key v4's schema does
    /// not declare, or a v5-only field.
    NoCounterpart,
}

impl V4 {
    /// Is this a body key — i.e. a shape v4 can actually receive as JSON, and
    /// therefore a row where v5's serde envelope is a real divergence?
    fn is_body(self) -> bool {
        matches!(
            self,
            V4::BodyParse | V4::BodyParseLocallyCaught | V4::BodySafeParse | V4::BodyHandRolled
        )
    }
}

struct Row {
    variant: &'static str,
    field: &'static str,
    rust_type: &'static str,
    v4: V4,
    /// The evidence: the v4 file:line and what it does with a wrong type.
    note: &'static str,
}

/// **The census.** One row per (`Request` variant, non-`Value` field) that the
/// route-identifier rule does not exclude — NOT one per typed field: the rule
/// drops 403 of 643 today, a large part of them real v4 body keys (see
/// [`EXCLUDED_BY_THE_ROUTE_IDENTIFIER_RULE`]). Held against a mechanical walk of
/// `api/types.rs` by [`census_covers_every_typed_request_field`], so a new
/// typed field outside the exclusion fails this test until it is classified.
/// The v4 half of every row (`v4`, `note`) is a TRANSCRIPTION read at the
/// `0b0617fee` pin, not a mechanical read of the v4 tree — v4 can move a
/// `.parse` site without this file noticing; the drift check is the ledger's.
const CENSUS: &[Row] = &[
    Row {
        variant: "Unlock",
        field: "passphrase",
        rust_type: "String",
        v4: V4::BodyHandRolled,
        note: concat!(
            "`system/unlock/route.ts:122` `getPassphrase`: `typeof body.passphrase ",
            "=== 'string' ? … : ''` — a wrong type COERCES to `''` and then fails the ",
            "business check (400 `Passphrase is required to unlock`), never a type ",
            "error"
        ),
    },
    Row {
        variant: "Setup",
        field: "passphrase",
        rust_type: "String",
        v4: V4::BodyHandRolled,
        note: concat!(
            "`system/unlock/route.ts:122,151` — coerces to `''`; `handleSetup('')` ",
            "has no empty guard, so v4 answers 200"
        ),
    },
    Row {
        variant: "StorePepper",
        field: "passphrase",
        rust_type: "String",
        v4: V4::BodyHandRolled,
        note: "`system/unlock/route.ts:122,349` — coerces to `''`; v4 answers 200",
    },
    Row {
        variant: "ChangePassphrase",
        field: "old_passphrase",
        rust_type: "String",
        v4: V4::BodyHandRolled,
        note: concat!(
            "`system/unlock/route.ts:388-389` — coerces to `''` (the documented ",
            "no-passphrase sentinel); no type error"
        ),
    },
    Row {
        variant: "ChangePassphrase",
        field: "new_passphrase",
        rust_type: "String",
        v4: V4::BodyHandRolled,
        note: concat!(
            "`system/unlock/route.ts:388-389` — coerces to `''` (the documented ",
            "no-passphrase sentinel); no type error"
        ),
    },
    Row {
        variant: "ListChats",
        field: "limit",
        rust_type: "Option<i64>",
        v4: V4::Query,
        note: "`chats/route.ts:933,935` `searchParams`",
    },
    Row {
        variant: "ListChats",
        field: "include_autonomous",
        rust_type: "bool",
        v4: V4::Query,
        note: "`chats/route.ts:933,935` `searchParams`",
    },
    Row {
        variant: "ConnectionProfileList",
        field: "image_capable",
        rust_type: "bool",
        v4: V4::Query,
        note: "`connection-profiles/route.ts:65` `searchParams`",
    },
    Row {
        variant: "ApiKeyCreate",
        field: "label",
        rust_type: "String",
        v4: V4::BodyHandRolled,
        note: concat!(
            "`api-keys/route.ts:112,122,126` hand-rolled `typeof` guards → 400 ",
            "`Invalid provider` / `Label is required` / `API key is required`"
        ),
    },
    Row {
        variant: "ApiKeyCreate",
        field: "provider",
        rust_type: "String",
        v4: V4::BodyHandRolled,
        note: concat!(
            "`api-keys/route.ts:112,122,126` hand-rolled `typeof` guards → 400 ",
            "`Invalid provider` / `Label is required` / `API key is required`"
        ),
    },
    Row {
        variant: "ApiKeyCreate",
        field: "api_key",
        rust_type: "String",
        v4: V4::BodyHandRolled,
        note: concat!(
            "`api-keys/route.ts:112,122,126` hand-rolled `typeof` guards → 400 ",
            "`Invalid provider` / `Label is required` / `API key is required`"
        ),
    },
    Row {
        variant: "ApiKeyUpdate",
        field: "label",
        rust_type: "Option<String>",
        v4: V4::BodyHandRolled,
        note: concat!(
            "`api-keys/[id]/route.ts:106-127` hand-rolled `typeof` guards → 400 ",
            "`<field> must be a …`"
        ),
    },
    Row {
        variant: "ApiKeyUpdate",
        field: "is_active",
        rust_type: "Option<bool>",
        v4: V4::BodyHandRolled,
        note: concat!(
            "`api-keys/[id]/route.ts:106-127` hand-rolled `typeof` guards → 400 ",
            "`<field> must be a …`"
        ),
    },
    Row {
        variant: "ApiKeyUpdate",
        field: "api_key",
        rust_type: "Option<String>",
        v4: V4::BodyHandRolled,
        note: concat!(
            "`api-keys/[id]/route.ts:106-127` hand-rolled `typeof` guards → 400 ",
            "`<field> must be a …`"
        ),
    },
    Row {
        variant: "ApiKeyTest",
        field: "base_url",
        rust_type: "Option<String>",
        v4: V4::BodyHandRolled,
        note: concat!(
            "`api-keys/[id]/route.ts:210` — NO type check at all; the raw value ",
            "reaches `testProviderApiKey`, so only a downstream throw (500) can ",
            "refuse it"
        ),
    },
    Row {
        variant: "ModelList",
        field: "provider",
        rust_type: "Option<String>",
        v4: V4::Query,
        note: "`models/route.ts:28` `searchParams`",
    },
    Row {
        variant: "ModelFetch",
        field: "provider",
        rust_type: "String",
        v4: V4::BodyParse,
        note: "`models/route.ts:60,72` `getModelsSchema.parse`",
    },
    Row {
        variant: "ModelFetch",
        field: "base_url",
        rust_type: "Option<String>",
        v4: V4::BodyParse,
        note: "`models/route.ts:60,72` `getModelsSchema.parse`",
    },
    Row {
        variant: "ChatTurnAction",
        field: "action",
        rust_type: "String",
        v4: V4::BodyParse,
        note: concat!(
            "`chats/[id]/schemas.ts:149` `turnActionSchema.parse` (a discriminated ",
            "union on `action`)"
        ),
    },
    Row {
        variant: "MessageEdit",
        field: "content",
        rust_type: "String",
        v4: V4::BodyParse,
        note: "`messages/[id]/route.ts:23,108` `editMessageSchema.parse`",
    },
    Row {
        variant: "MessageDelete",
        field: "memory_action",
        rust_type: "Option<String>",
        v4: V4::Query,
        note: "`messages/[id]/route.ts:142-143` `searchParams`",
    },
    Row {
        variant: "MessageDelete",
        field: "skip_confirmation",
        rust_type: "bool",
        v4: V4::Query,
        note: "`messages/[id]/route.ts:142-143` `searchParams`",
    },
    Row {
        variant: "MessageSwipe",
        field: "swipe_index",
        rust_type: "Option<i64>",
        v4: V4::BodySafeParse,
        note: concat!(
            "`messages/[id]/route.ts:27,247` `swipeActionSchema.safeParse` — and the ",
            "guard is `if (parsed.success && …)`, so a wrong type is NOT refused: it ",
            "silently reroutes to `handleGenerateSwipe`"
        ),
    },
    Row {
        variant: "ChatSend",
        field: "content",
        rust_type: "String",
        v4: V4::BodyParse,
        note: concat!(
            "`chats/[id]/messages/route.ts:37`; `orchestrator.service.ts:149` ",
            "`sendMessageSchema.parse` — but only in NON-continue mode; in continue ",
            "mode Zod strips the key silently"
        ),
    },
    Row {
        variant: "ChatSend",
        field: "continue_mode",
        rust_type: "bool",
        v4: V4::BodyHandRolled,
        note: concat!(
            "`chats/[id]/messages/route.ts:38` `body.continueMode === true` — a bare ",
            "strict compare, no schema and no error ever"
        ),
    },
    Row {
        variant: "ChatSend",
        field: "nudge",
        rust_type: "Option<bool>",
        v4: V4::BodyParse,
        note: concat!(
            "`orchestrator.service.ts:181` `continueMessageSchema.nudge` — validated ",
            "only in CONTINUE mode; silently stripped otherwise"
        ),
    },
    Row {
        variant: "ChatSend",
        field: "pending_tool_results",
        rust_type: "Vec<PendingToolResult>",
        v4: V4::BodyParse,
        note: concat!(
            "`chats/[id]/messages/route.ts:37`; `orchestrator.service.ts:149` ",
            "`sendMessageSchema.parse` — but only in NON-continue mode; in continue ",
            "mode Zod strips the key silently"
        ),
    },
    Row {
        variant: "CharacterList",
        field: "archived",
        rust_type: "Option<String>",
        v4: V4::Query,
        note: "`characters/handlers/get.ts:30-42` `searchParams`",
    },
    Row {
        variant: "CharacterList",
        field: "npc",
        rust_type: "Option<String>",
        v4: V4::Query,
        note: "`characters/handlers/get.ts:30-42` `searchParams`",
    },
    Row {
        variant: "CharacterList",
        field: "controlled_by",
        rust_type: "Option<String>",
        v4: V4::Query,
        note: "`characters/handlers/get.ts:30-42` `searchParams`",
    },
    Row {
        variant: "CharacterQuickCreate",
        field: "name",
        rust_type: "String",
        v4: V4::BodyParse,
        note: "`characters/handlers/post.ts:84,366` `quickCreateSchema.parse`",
    },
    Row {
        variant: "CharacterDelete",
        field: "cascade_chats",
        rust_type: "bool",
        v4: V4::Query,
        note: "`characters/[id]/handlers/delete.ts:34-35` `searchParams`",
    },
    Row {
        variant: "CharacterDelete",
        field: "cascade_images",
        rust_type: "bool",
        v4: V4::Query,
        note: "`characters/[id]/handlers/delete.ts:34-35` `searchParams`",
    },
    Row {
        variant: "CharacterChats",
        field: "search",
        rust_type: "Option<String>",
        v4: V4::Query,
        note: "`characters/[id]/handlers/get.ts:116-118` `searchParams`",
    },
    Row {
        variant: "CharacterChats",
        field: "limit",
        rust_type: "Option<i64>",
        v4: V4::Query,
        note: "`characters/[id]/handlers/get.ts:116-118` `searchParams`",
    },
    Row {
        variant: "CharacterChats",
        field: "offset",
        rust_type: "Option<i64>",
        v4: V4::Query,
        note: "`characters/[id]/handlers/get.ts:116-118` `searchParams`",
    },
    Row {
        variant: "CharacterDepictionGuidelinesUpdate",
        field: "content",
        rust_type: "String",
        v4: V4::BodyHandRolled,
        note: concat!(
            "`characters/[id]/handlers/put.ts:124` `typeof … === 'string' ? … : ''` — ",
            "a wrong type silently CLEARS the file and answers 200"
        ),
    },
    Row {
        variant: "CharacterPromptCreate",
        field: "name",
        rust_type: "String",
        v4: V4::BodyParse,
        note: "`characters/[id]/prompts/route.ts:13,41` `createPromptSchema.parse`",
    },
    Row {
        variant: "CharacterPromptCreate",
        field: "content",
        rust_type: "String",
        v4: V4::BodyParse,
        note: "`characters/[id]/prompts/route.ts:13,41` `createPromptSchema.parse`",
    },
    Row {
        variant: "CharacterPromptCreate",
        field: "is_default",
        rust_type: "bool",
        v4: V4::BodyParse,
        note: "`characters/[id]/prompts/route.ts:13,41` `createPromptSchema.parse`",
    },
    Row {
        variant: "CharacterPromptUpdate",
        field: "name",
        rust_type: "Option<String>",
        v4: V4::BodyParse,
        note: concat!(
            "`characters/[id]/prompts/[promptId]/route.ts:14,50` ",
            "`updatePromptSchema.parse`"
        ),
    },
    Row {
        variant: "CharacterPromptUpdate",
        field: "content",
        rust_type: "Option<String>",
        v4: V4::BodyParse,
        note: concat!(
            "`characters/[id]/prompts/[promptId]/route.ts:14,50` ",
            "`updatePromptSchema.parse`"
        ),
    },
    Row {
        variant: "CharacterPromptUpdate",
        field: "is_default",
        rust_type: "Option<bool>",
        v4: V4::BodyParse,
        note: concat!(
            "`characters/[id]/prompts/[promptId]/route.ts:14,50` ",
            "`updatePromptSchema.parse`"
        ),
    },
    Row {
        variant: "CharacterScenarioList",
        field: "include_archived",
        rust_type: "bool",
        v4: V4::Query,
        note: "`lib/api/query-params.ts:18` `readIncludeArchived`",
    },
    Row {
        variant: "CharacterScenarioCreate",
        field: "title",
        rust_type: "String",
        v4: V4::BodyParse,
        note: concat!(
            "`characters/[id]/scenarios/route.ts:22,60` `createScenarioSchema.parse` ",
            "(`archived` is `.optional()` but NOT `.nullable()`, so an explicit null ",
            "refuses on both sides)"
        ),
    },
    Row {
        variant: "CharacterScenarioCreate",
        field: "content",
        rust_type: "String",
        v4: V4::BodyParse,
        note: concat!(
            "`characters/[id]/scenarios/route.ts:22,60` `createScenarioSchema.parse` ",
            "(`archived` is `.optional()` but NOT `.nullable()`, so an explicit null ",
            "refuses on both sides)"
        ),
    },
    Row {
        variant: "CharacterScenarioCreate",
        field: "archived",
        rust_type: "Option<Option<bool>>",
        v4: V4::BodyParse,
        note: concat!(
            "`characters/[id]/scenarios/route.ts:22,60` `createScenarioSchema.parse` ",
            "(`archived` is `.optional()` but NOT `.nullable()`, so an explicit null ",
            "refuses on both sides)"
        ),
    },
    Row {
        variant: "CharacterScenarioUpdate",
        field: "title",
        rust_type: "Option<String>",
        v4: V4::BodyParse,
        note: "`characters/[id]/scenarios/route.ts:28,104` `updateScenarioSchema.parse`",
    },
    Row {
        variant: "CharacterScenarioUpdate",
        field: "content",
        rust_type: "Option<String>",
        v4: V4::BodyParse,
        note: "`characters/[id]/scenarios/route.ts:28,104` `updateScenarioSchema.parse`",
    },
    Row {
        variant: "CharacterScenarioUpdate",
        field: "archived",
        rust_type: "Option<Option<bool>>",
        v4: V4::BodyParse,
        note: "`characters/[id]/scenarios/route.ts:28,104` `updateScenarioSchema.parse`",
    },
    Row {
        variant: "CharacterPluginDataUpsert",
        field: "plugin_name",
        rust_type: "String",
        v4: V4::BodySafeParse,
        note: concat!(
            "`characters/[id]/plugin-data/route.ts:14,54` `safeParse` → 400 `Invalid ",
            "request: <joined messages>` — NOT the `Validation error` envelope, and ",
            "coincidentally close to v5's own serde wording"
        ),
    },
    Row {
        variant: "CharacterPluginDataGet",
        field: "plugin_name",
        rust_type: "String",
        v4: V4::Path,
        note: concat!(
            "`characters/[id]/plugin-data/[pluginName]/route.ts:15` — a URL path ",
            "segment, always a string in v4"
        ),
    },
    Row {
        variant: "CharacterPluginDataDelete",
        field: "plugin_name",
        rust_type: "String",
        v4: V4::Path,
        note: concat!(
            "`characters/[id]/plugin-data/[pluginName]/route.ts:83` — a URL path ",
            "segment"
        ),
    },
    Row {
        variant: "CharacterWardrobeList",
        field: "scope",
        rust_type: "Option<String>",
        v4: V4::Query,
        note: "`characters/[id]/wardrobe/route.ts:99` + `readIncludeArchived`",
    },
    Row {
        variant: "CharacterWardrobeList",
        field: "include_archived",
        rust_type: "bool",
        v4: V4::Query,
        note: "`characters/[id]/wardrobe/route.ts:99` + `readIncludeArchived`",
    },
    Row {
        variant: "CharacterExport",
        field: "format",
        rust_type: "Option<String>",
        v4: V4::Query,
        note: concat!(
            "`characters/[id]/handlers/get.ts:74` `searchParams.get('format') || ",
            "'json'`"
        ),
    },
    Row {
        variant: "CharacterPhotoList",
        field: "limit",
        rust_type: "Option<i64>",
        v4: V4::Query,
        note: concat!(
            "`characters/[id]/photos/route.ts:25` `searchParams`, then `Number()` ",
            "into `listQuerySchema.safeParse`"
        ),
    },
    Row {
        variant: "CharacterPhotoList",
        field: "offset",
        rust_type: "Option<i64>",
        v4: V4::Query,
        note: concat!(
            "`characters/[id]/photos/route.ts:25` `searchParams`, then `Number()` ",
            "into `listQuerySchema.safeParse`"
        ),
    },
    Row {
        variant: "TagList",
        field: "search",
        rust_type: "Option<String>",
        v4: V4::Query,
        note: "`tags/route.ts:29` `searchParams`",
    },
    Row {
        variant: "TagCreate",
        field: "name",
        rust_type: "String",
        v4: V4::BodyParse,
        note: "`tags/route.ts:20,90` `createTagSchema.parse`",
    },
    Row {
        variant: "GroupCreate",
        field: "name",
        rust_type: "String",
        v4: V4::BodyParse,
        note: "`groups/route.ts:19,58` `createGroupSchema.parse`",
    },
    Row {
        variant: "GroupCreate",
        field: "description",
        rust_type: "Option<String>",
        v4: V4::BodyParse,
        note: "`groups/route.ts:19,58` `createGroupSchema.parse`",
    },
    Row {
        variant: "GroupCreate",
        field: "instructions",
        rust_type: "Option<String>",
        v4: V4::BodyParse,
        note: "`groups/route.ts:19,58` `createGroupSchema.parse`",
    },
    Row {
        variant: "GroupCreate",
        field: "color",
        rust_type: "Option<String>",
        v4: V4::BodyParse,
        note: "`groups/route.ts:19,58` `createGroupSchema.parse`",
    },
    Row {
        variant: "GroupCreate",
        field: "icon",
        rust_type: "Option<String>",
        v4: V4::BodyParse,
        note: "`groups/route.ts:19,58` `createGroupSchema.parse`",
    },
    Row {
        variant: "GroupScenarioList",
        field: "include_archived",
        rust_type: "bool",
        v4: V4::Query,
        note: "`groups/[id]/scenarios/route.ts:47` `readIncludeArchived`",
    },
    Row {
        variant: "GroupScenarioUpdate",
        field: "include_archived",
        rust_type: "bool",
        v4: V4::Query,
        note: concat!(
            "`lib/mount-index/scenario-item-route-factory.ts:171` ",
            "`readIncludeArchived`"
        ),
    },
    Row {
        variant: "GroupScenarioRename",
        field: "new_filename",
        rust_type: "String",
        v4: V4::BodyParseLocallyCaught,
        note: concat!(
            "`lib/mount-index/scenario-item-route-factory.ts:229` — ",
            "`renameScenarioSchema.parse` inside a LOCAL try/catch → 400 `Invalid ",
            "request body: <joined messages>`, no `details` array"
        ),
    },
    Row {
        variant: "GroupScenarioRename",
        field: "include_archived",
        rust_type: "bool",
        v4: V4::Query,
        note: "`scenario-item-route-factory.ts:225` `readIncludeArchived`",
    },
    Row {
        variant: "GroupScenarioDelete",
        field: "include_archived",
        rust_type: "bool",
        v4: V4::Query,
        note: "`scenario-item-route-factory.ts:274` `readIncludeArchived`",
    },
    Row {
        variant: "GroupWardrobeList",
        field: "include_archived",
        rust_type: "bool",
        v4: V4::Query,
        note: concat!(
            "`lib/mount-index/mount-wardrobe-route-factory.ts:174` ",
            "`readIncludeArchived`"
        ),
    },
    Row {
        variant: "GroupScenariosUnion",
        field: "include_archived",
        rust_type: "bool",
        v4: V4::Query,
        note: "`groups/scenarios/route.ts:34` `readIncludeArchived`",
    },
    Row {
        variant: "ScenarioList",
        field: "include_archived",
        rust_type: "bool",
        v4: V4::Query,
        note: "`scenarios/route.ts:44` `readIncludeArchived`",
    },
    Row {
        variant: "ScenarioUpdate",
        field: "include_archived",
        rust_type: "bool",
        v4: V4::Query,
        note: "`scenarios/[scenarioPath]/route.ts:106` `readIncludeArchived`",
    },
    Row {
        variant: "ScenarioRename",
        field: "new_filename",
        rust_type: "String",
        v4: V4::BodyParseLocallyCaught,
        note: concat!(
            "`scenarios/[scenarioPath]/route.ts:175,213` — `.parse` in a LOCAL catch ",
            "→ 400 `Invalid request body: <messages>`"
        ),
    },
    Row {
        variant: "ScenarioRename",
        field: "include_archived",
        rust_type: "bool",
        v4: V4::Query,
        note: "`scenarios/[scenarioPath]/route.ts:161` `readIncludeArchived`",
    },
    Row {
        variant: "ScenarioDelete",
        field: "include_archived",
        rust_type: "bool",
        v4: V4::Query,
        note: "`scenarios/[scenarioPath]/route.ts:233` `readIncludeArchived`",
    },
    Row {
        variant: "ImageProfileList",
        field: "sort_by_character",
        rust_type: "Option<String>",
        v4: V4::Query,
        note: "`image-profiles/route.ts:50` `searchParams`",
    },
    Row {
        variant: "ImageProfileGenerate",
        field: "prompt",
        rust_type: "String",
        v4: V4::BodyParse,
        note: concat!(
            "`image-profiles/[id]/route.ts:21,242` `generateImageSchema.parse` ",
            "(`count`'s `.prefault(1)` substitutes only on `undefined`, so a wrong ",
            "type still fails)"
        ),
    },
    Row {
        variant: "ImageProfileGenerate",
        field: "count",
        rust_type: "Option<i64>",
        v4: V4::BodyParse,
        note: concat!(
            "`image-profiles/[id]/route.ts:21,242` `generateImageSchema.parse` ",
            "(`count`'s `.prefault(1)` substitutes only on `undefined`, so a wrong ",
            "type still fails)"
        ),
    },
    Row {
        variant: "ImageProfileListModels",
        field: "provider",
        rust_type: "Option<String>",
        v4: V4::Query,
        note: "`image-profiles/route.ts:128` `searchParams`",
    },
    Row {
        variant: "ImageProfileOptionsSchema",
        field: "provider",
        rust_type: "Option<String>",
        v4: V4::Query,
        note: "`image-profiles/route.ts:248-249` `searchParams`",
    },
    Row {
        variant: "ImageProfileOptionsSchema",
        field: "model",
        rust_type: "Option<String>",
        v4: V4::Query,
        note: "`image-profiles/route.ts:248-249` `searchParams`",
    },
    Row {
        variant: "EmbeddingProfileListModels",
        field: "provider",
        rust_type: "Option<String>",
        v4: V4::Query,
        note: "`embedding-profiles/route.ts:201` `searchParams`",
    },
    Row {
        variant: "EmbeddingProfileFetchModels",
        field: "provider",
        rust_type: "String",
        v4: V4::Query,
        note: "`embedding-profiles/route.ts:135-136` `searchParams`",
    },
    Row {
        variant: "EmbeddingProfileFetchModels",
        field: "base_url",
        rust_type: "Option<String>",
        v4: V4::Query,
        note: "`embedding-profiles/route.ts:135-136` `searchParams`",
    },
    Row {
        variant: "MemoryDedupPreview",
        field: "threshold",
        rust_type: "Option<f64>",
        v4: V4::Query,
        note: "`system/tools/route.ts:1183` `searchParams` + `parseFloat`",
    },
    Row {
        variant: "MemoryDedupRun",
        field: "threshold",
        rust_type: "Option<f64>",
        v4: V4::BodyHandRolled,
        note: concat!(
            "`system/tools/route.ts:1217` `typeof x === 'number' ? x : 0.80` — a ",
            "wrong type SILENTLY defaults and answers 200"
        ),
    },
    Row {
        variant: "ProjectChatList",
        field: "limit",
        rust_type: "Option<i64>",
        v4: V4::Query,
        note: "`projects/[id]/actions/chats.ts:34-35` `searchParams`",
    },
    Row {
        variant: "ProjectChatList",
        field: "offset",
        rust_type: "Option<i64>",
        v4: V4::Query,
        note: "`projects/[id]/actions/chats.ts:34-35` `searchParams`",
    },
    Row {
        variant: "ProjectAestheticGet",
        field: "kind",
        rust_type: "String",
        v4: V4::Query,
        note: "`projects/[id]/actions/aesthetic.ts:35` `searchParams`",
    },
    Row {
        variant: "ProjectAestheticSet",
        field: "kind",
        rust_type: "String",
        v4: V4::Query,
        note: "`projects/[id]/actions/aesthetic.ts:55` `searchParams`",
    },
    Row {
        variant: "ProjectAestheticSet",
        field: "content",
        rust_type: "Option<String>",
        v4: V4::BodySafeParse,
        note: concat!(
            "`projects/[id]/actions/aesthetic.ts:62` ",
            "`aestheticContentSchema.safeParse(body).data?.content ?? ''` — a wrong ",
            "type silently becomes `''`, which DELETES the file, and answers 200"
        ),
    },
    Row {
        variant: "ProjectToolSettingsUpdate",
        field: "default_disabled_tools",
        rust_type: "Vec<String>",
        v4: V4::BodyParse,
        note: concat!(
            "`projects/[id]/schemas.ts:51-52`, `actions/tools.ts:29` ",
            "`updateToolSettingsSchema.parse`"
        ),
    },
    Row {
        variant: "ProjectToolSettingsUpdate",
        field: "default_disabled_tool_groups",
        rust_type: "Vec<String>",
        v4: V4::BodyParse,
        note: concat!(
            "`projects/[id]/schemas.ts:51-52`, `actions/tools.ts:29` ",
            "`updateToolSettingsSchema.parse`"
        ),
    },
    Row {
        variant: "ProjectScenarioList",
        field: "include_archived",
        rust_type: "bool",
        v4: V4::Query,
        note: "`projects/[id]/scenarios/route.ts:47` `readIncludeArchived`",
    },
    Row {
        variant: "ProjectScenarioUpdate",
        field: "include_archived",
        rust_type: "bool",
        v4: V4::Query,
        note: "`scenario-item-route-factory.ts:160` `readIncludeArchived`",
    },
    Row {
        variant: "ProjectScenarioRename",
        field: "new_filename",
        rust_type: "String",
        v4: V4::BodyParseLocallyCaught,
        note: concat!(
            "`scenario-item-route-factory.ts:231,271` — `.parse` in a LOCAL catch → ",
            "400 `Invalid request body: <messages>`"
        ),
    },
    Row {
        variant: "ProjectScenarioRename",
        field: "include_archived",
        rust_type: "bool",
        v4: V4::Query,
        note: "`scenario-item-route-factory.ts:217` `readIncludeArchived`",
    },
    Row {
        variant: "ProjectScenarioDelete",
        field: "include_archived",
        rust_type: "bool",
        v4: V4::Query,
        note: "`scenario-item-route-factory.ts:291` `readIncludeArchived`",
    },
    Row {
        variant: "ProjectWardrobeList",
        field: "include_archived",
        rust_type: "bool",
        v4: V4::Query,
        note: "`mount-wardrobe-route-factory.ts:174` `readIncludeArchived`",
    },
    Row {
        variant: "MemoryList",
        field: "search",
        rust_type: "Option<String>",
        v4: V4::Query,
        note: "`memories/route.ts:312-320` `searchParams`",
    },
    Row {
        variant: "MemoryList",
        field: "min_importance",
        rust_type: "Option<f64>",
        v4: V4::Query,
        note: "`memories/route.ts:312-320` `searchParams`",
    },
    Row {
        variant: "MemoryList",
        field: "source",
        rust_type: "Option<String>",
        v4: V4::Query,
        note: "`memories/route.ts:312-320` `searchParams`",
    },
    Row {
        variant: "MemoryList",
        field: "sort_by",
        rust_type: "Option<String>",
        v4: V4::Query,
        note: "`memories/route.ts:312-320` `searchParams`",
    },
    Row {
        variant: "MemoryList",
        field: "sort_order",
        rust_type: "Option<String>",
        v4: V4::Query,
        note: "`memories/route.ts:312-320` `searchParams`",
    },
    Row {
        variant: "MemoryList",
        field: "limit",
        rust_type: "Option<i64>",
        v4: V4::Query,
        note: "`memories/route.ts:312-320` `searchParams`",
    },
    Row {
        variant: "MemoryList",
        field: "offset",
        rust_type: "Option<i64>",
        v4: V4::Query,
        note: "`memories/route.ts:312-320` `searchParams`",
    },
    Row {
        variant: "MemoryHousekeepPreview",
        field: "max_memories",
        rust_type: "Option<i64>",
        v4: V4::Query,
        note: "`memories/route.ts:675-698` `searchParams`",
    },
    Row {
        variant: "MemoryHousekeepPreview",
        field: "max_age_months",
        rust_type: "Option<i64>",
        v4: V4::Query,
        note: "`memories/route.ts:675-698` `searchParams`",
    },
    Row {
        variant: "MemoryHousekeepPreview",
        field: "min_importance",
        rust_type: "Option<f64>",
        v4: V4::Query,
        note: "`memories/route.ts:675-698` `searchParams`",
    },
    Row {
        variant: "MemoryHousekeepPreview",
        field: "merge_similar",
        rust_type: "Option<bool>",
        v4: V4::Query,
        note: "`memories/route.ts:675-698` `searchParams`",
    },
    Row {
        variant: "MemoryExtractionConcurrencySet",
        field: "concurrency",
        rust_type: "i64",
        v4: V4::BodySafeParse,
        note: concat!(
            "`memories/route.ts:135,1226` `safeParse` → an EXPLICIT ",
            "`validationError(parsed.error)`, i.e. the same `{error:'Validation ",
            "error',details:[…]}` envelope an uncaught `.parse` produces"
        ),
    },
    Row {
        variant: "MemoryBackfillStart",
        field: "batch_size",
        rust_type: "Option<i64>",
        v4: V4::BodySafeParse,
        note: concat!(
            "`memories/route.ts:128,895` `safeParse` → explicit ",
            "`validationError(...)`"
        ),
    },
    Row {
        variant: "MemoryGenerateEmbeddings",
        field: "batch_size",
        rust_type: "Option<i64>",
        v4: V4::BodyParse,
        note: "`memories/route.ts:95,972` `generateEmbeddingsSchema.parse`",
    },
    Row {
        variant: "MemoryRebuildIndex",
        field: "confirm",
        rust_type: "bool",
        v4: V4::BodyParse,
        note: concat!(
            "`memories/route.ts:100,1056` `rebuildIndexSchema.parse` ",
            "(`z.literal(true)` with a custom message)"
        ),
    },
    Row {
        variant: "MountFileRead",
        field: "encoding",
        rust_type: "Option<String>",
        v4: V4::Query,
        note: concat!(
            "`mount-points/[id]/files/[...path]/route.ts:62,77-85` `searchParams` ",
            "through `readQuerySchema.safeParse`"
        ),
    },
    Row {
        variant: "MountFileRead",
        field: "offset",
        rust_type: "Option<i64>",
        v4: V4::Query,
        note: concat!(
            "`mount-points/[id]/files/[...path]/route.ts:62,77-85` `searchParams` ",
            "through `readQuerySchema.safeParse`"
        ),
    },
    Row {
        variant: "MountFileRead",
        field: "limit",
        rust_type: "Option<i64>",
        v4: V4::Query,
        note: concat!(
            "`mount-points/[id]/files/[...path]/route.ts:62,77-85` `searchParams` ",
            "through `readQuerySchema.safeParse`"
        ),
    },
    Row {
        variant: "MountReindex",
        field: "force",
        rust_type: "Option<bool>",
        v4: V4::BodyHandRolled,
        note: concat!(
            "`mount-points/[id]/route.ts:704`; `lib/mount-index/reindex.ts:104` ",
            "`!!options.force` — pure JS truthiness, never refused"
        ),
    },
    Row {
        variant: "MountEmbed",
        field: "force",
        rust_type: "Option<bool>",
        v4: V4::BodyHandRolled,
        note: concat!(
            "`mount-points/[id]/route.ts:750`; `lib/mount-index/reindex.ts:211` ",
            "`!!options.force` — pure JS truthiness, never refused"
        ),
    },
    Row {
        variant: "MountFileMove",
        field: "source_path",
        rust_type: "String",
        v4: V4::BodySafeParse,
        note: concat!(
            "`mount-points/[id]/route.ts:433,458` `moveFileSchema.safeParse` → one ",
            "FIXED sentence: 400 `Move requires sourcePath, destMountPointId, and ",
            "destPath`"
        ),
    },
    Row {
        variant: "MountFileMove",
        field: "dest_path",
        rust_type: "String",
        v4: V4::BodySafeParse,
        note: concat!(
            "`mount-points/[id]/route.ts:433,458` `moveFileSchema.safeParse` → one ",
            "FIXED sentence: 400 `Move requires sourcePath, destMountPointId, and ",
            "destPath`"
        ),
    },
    Row {
        variant: "MountFileCopy",
        field: "source_path",
        rust_type: "String",
        v4: V4::BodySafeParse,
        note: concat!(
            "`mount-points/[id]/route.ts:439,496` → 400 `Copy requires sourcePath, ",
            "destMountPointId, and destPath`"
        ),
    },
    Row {
        variant: "MountFileCopy",
        field: "dest_path",
        rust_type: "String",
        v4: V4::BodySafeParse,
        note: concat!(
            "`mount-points/[id]/route.ts:439,496` → 400 `Copy requires sourcePath, ",
            "destMountPointId, and destPath`"
        ),
    },
    Row {
        variant: "MountFileCopy",
        field: "force",
        rust_type: "Option<bool>",
        v4: V4::BodySafeParse,
        note: concat!(
            "`mount-points/[id]/route.ts:439,496` → 400 `Copy requires sourcePath, ",
            "destMountPointId, and destPath`"
        ),
    },
    Row {
        variant: "MountFileLink",
        field: "source_path",
        rust_type: "String",
        v4: V4::BodySafeParse,
        note: concat!(
            "`mount-points/[id]/route.ts:535` (reuses `moveFileSchema`) → 400 `Link ",
            "requires sourcePath, destMountPointId, and destPath`"
        ),
    },
    Row {
        variant: "MountFileLink",
        field: "dest_path",
        rust_type: "String",
        v4: V4::BodySafeParse,
        note: concat!(
            "`mount-points/[id]/route.ts:535` (reuses `moveFileSchema`) → 400 `Link ",
            "requires sourcePath, destMountPointId, and destPath`"
        ),
    },
    Row {
        variant: "MountFolderMove",
        field: "from_path",
        rust_type: "String",
        v4: V4::BodySafeParse,
        note: concat!(
            "`mount-points/[id]/route.ts:447,586` → 400 `Move folder requires ",
            "fromPath and toPath`"
        ),
    },
    Row {
        variant: "MountFolderMove",
        field: "to_path",
        rust_type: "String",
        v4: V4::BodySafeParse,
        note: concat!(
            "`mount-points/[id]/route.ts:447,586` → 400 `Move folder requires ",
            "fromPath and toPath`"
        ),
    },
    Row {
        variant: "MountFileUpdate",
        field: "description",
        rust_type: "Option<String>",
        v4: V4::BodySafeParse,
        note: concat!(
            "`mount-points/[id]/files/[...path]/route.ts:240,257` ",
            "`patchBodySchema.safeParse` → 400 `<zod messages joined by ', '>`"
        ),
    },
    Row {
        variant: "MountFileUpdate",
        field: "rename",
        rust_type: "Option<String>",
        v4: V4::BodySafeParse,
        note: concat!(
            "`mount-points/[id]/files/[...path]/route.ts:240,257` ",
            "`patchBodySchema.safeParse` → 400 `<zod messages joined by ', '>`"
        ),
    },
    Row {
        variant: "MountFileWrite",
        field: "content",
        rust_type: "String",
        v4: V4::BodySafeParse,
        note: concat!(
            "`mount-points/[id]/files/[...path]/route.ts:126,166` ",
            "`writeBodySchema.safeParse` → 400 `Invalid body: <messages>`"
        ),
    },
    Row {
        variant: "MountFileWrite",
        field: "encoding",
        rust_type: "Option<String>",
        v4: V4::BodySafeParse,
        note: concat!(
            "`mount-points/[id]/files/[...path]/route.ts:126,166` ",
            "`writeBodySchema.safeParse` → 400 `Invalid body: <messages>`"
        ),
    },
    Row {
        variant: "MountFileWrite",
        field: "expected_mtime",
        rust_type: "Option<i64>",
        v4: V4::BodySafeParse,
        note: concat!(
            "`mount-points/[id]/files/[...path]/route.ts:126,166` ",
            "`writeBodySchema.safeParse` → 400 `Invalid body: <messages>`"
        ),
    },
    Row {
        variant: "MountFileWrite",
        field: "force",
        rust_type: "Option<bool>",
        v4: V4::BodySafeParse,
        note: concat!(
            "`mount-points/[id]/files/[...path]/route.ts:126,166` ",
            "`writeBodySchema.safeParse` → 400 `Invalid body: <messages>`"
        ),
    },
    Row {
        variant: "MountFileWrite",
        field: "original_mime_type",
        rust_type: "Option<String>",
        v4: V4::NoCounterpart,
        note: concat!(
            "`writeBodySchema` has no such key; v4 reads it only off ",
            "`multipart/form-data` (`:153`), so a JSON body carrying it is silently ",
            "stripped"
        ),
    },
    Row {
        variant: "MountFileWrite",
        field: "original_file_name",
        rust_type: "Option<String>",
        v4: V4::NoCounterpart,
        note: "`writeBodySchema` has no such key; multipart-only (`:154`)",
    },
    Row {
        variant: "MountFileWriteRaw",
        field: "data",
        rust_type: "String",
        v4: V4::NoCounterpart,
        note: concat!(
            "`mount-points/[id]/route.ts:610` `?action=write-file` is multipart-ONLY: ",
            "v4 answers 400 `Expected multipart/form-data` before any field is read"
        ),
    },
    Row {
        variant: "MountFileWriteRaw",
        field: "force",
        rust_type: "Option<bool>",
        v4: V4::NoCounterpart,
        note: concat!(
            "`mount-points/[id]/route.ts:610` `?action=write-file` is multipart-ONLY: ",
            "v4 answers 400 `Expected multipart/form-data` before any field is read"
        ),
    },
    Row {
        variant: "MountBlobUpload",
        field: "description",
        rust_type: "Option<String>",
        v4: V4::NoCounterpart,
        note: concat!(
            "`mount-points/[id]/blobs/route.ts:56` is multipart-ONLY: 400 `Expected ",
            "multipart/form-data` before any field is read"
        ),
    },
    Row {
        variant: "MountBlobUpload",
        field: "data",
        rust_type: "String",
        v4: V4::NoCounterpart,
        note: concat!(
            "`mount-points/[id]/blobs/route.ts:56` is multipart-ONLY: 400 `Expected ",
            "multipart/form-data` before any field is read"
        ),
    },
    Row {
        variant: "MountBlobUpload",
        field: "original_mime_type",
        rust_type: "Option<String>",
        v4: V4::NoCounterpart,
        note: concat!(
            "`mount-points/[id]/blobs/route.ts:56` is multipart-ONLY: 400 `Expected ",
            "multipart/form-data` before any field is read"
        ),
    },
    Row {
        variant: "MountBlobUpload",
        field: "original_file_name",
        rust_type: "Option<String>",
        v4: V4::NoCounterpart,
        note: concat!(
            "`mount-points/[id]/blobs/route.ts:56` is multipart-ONLY: 400 `Expected ",
            "multipart/form-data` before any field is read"
        ),
    },
    Row {
        variant: "MountBlobsList",
        field: "folder",
        rust_type: "Option<String>",
        v4: V4::Query,
        note: "`mount-points/[id]/blobs/route.ts:36` `searchParams`",
    },
    Row {
        variant: "MountBlobUpdate",
        field: "description",
        rust_type: "Option<String>",
        v4: V4::BodyHandRolled,
        note: concat!(
            "`mount-points/[id]/blobs/[...path]/route.ts:150` `typeof … === 'string' ",
            "? … : ''` — silently becomes `''`, 200"
        ),
    },
    Row {
        variant: "MountDeconvert",
        field: "target_path",
        rust_type: "String",
        v4: V4::BodySafeParse,
        note: concat!(
            "`mount-points/[id]/route.ts:338,350` → 400 `Deconvert requires a ",
            "non-empty targetPath`"
        ),
    },
    Row {
        variant: "ChatAccessibleStores",
        field: "all",
        rust_type: "bool",
        v4: V4::Query,
        note: "`chats/[id]/handlers/get.ts:175` `searchParams`",
    },
    Row {
        variant: "MessageResolveExternalTurn",
        field: "reply_content",
        rust_type: "String",
        v4: V4::BodySafeParse,
        note: concat!(
            "`chats/[id]/messages/[messageId]/route.ts:86,103` `safeParse` → 400 ",
            "`<messages joined by '; '>`"
        ),
    },
    Row {
        variant: "ChatFileUpload",
        field: "filename",
        rust_type: "String",
        v4: V4::NoCounterpart,
        note: concat!(
            "`chats/[id]/files/route.ts:56-63` — the default leg is multipart; ",
            "`filename`/`contentType`/`data` are the decoded `File`, and `resolution` ",
            "is a form field (always a string). v5's variant is the ALREADY-DECODED ",
            "form, so there is no v4 JSON shape to be wrong"
        ),
    },
    Row {
        variant: "ChatFileUpload",
        field: "content_type",
        rust_type: "String",
        v4: V4::NoCounterpart,
        note: concat!(
            "`chats/[id]/files/route.ts:56-63` — the default leg is multipart; ",
            "`filename`/`contentType`/`data` are the decoded `File`, and `resolution` ",
            "is a form field (always a string). v5's variant is the ALREADY-DECODED ",
            "form, so there is no v4 JSON shape to be wrong"
        ),
    },
    Row {
        variant: "ChatFileUpload",
        field: "data",
        rust_type: "String",
        v4: V4::NoCounterpart,
        note: concat!(
            "`chats/[id]/files/route.ts:56-63` — the default leg is multipart; ",
            "`filename`/`contentType`/`data` are the decoded `File`, and `resolution` ",
            "is a form field (always a string). v5's variant is the ALREADY-DECODED ",
            "form, so there is no v4 JSON shape to be wrong"
        ),
    },
    Row {
        variant: "ChatFileUpload",
        field: "resolution",
        rust_type: "Option<String>",
        v4: V4::NoCounterpart,
        note: concat!(
            "`chats/[id]/files/route.ts:56-63` — the default leg is multipart; ",
            "`filename`/`contentType`/`data` are the decoded `File`, and `resolution` ",
            "is a form field (always a string). v5's variant is the ALREADY-DECODED ",
            "form, so there is no v4 JSON shape to be wrong"
        ),
    },
    Row {
        variant: "FilesList",
        field: "folder_path",
        rust_type: "Option<String>",
        v4: V4::Query,
        note: "`files/handlers/get.ts:15-17` `searchParams`",
    },
    Row {
        variant: "FilesList",
        field: "filter",
        rust_type: "Option<String>",
        v4: V4::Query,
        note: "`files/handlers/get.ts:15-17` `searchParams`",
    },
    Row {
        variant: "FilesList",
        field: "category",
        rust_type: "Option<String>",
        v4: V4::Query,
        note: "`files/handlers/get.ts:15-17` `searchParams`",
    },
    Row {
        variant: "FileMove",
        field: "folder_path",
        rust_type: "Option<String>",
        v4: V4::BodySafeParse,
        note: concat!(
            "`files/[id]/shared.ts:5`, `actions/move.ts:14` `safeParse` → 400 ",
            "`Invalid request: <zod message>`"
        ),
    },
    Row {
        variant: "FileMove",
        field: "filename",
        rust_type: "Option<String>",
        v4: V4::BodySafeParse,
        note: concat!(
            "`files/[id]/shared.ts:5`, `actions/move.ts:14` `safeParse` → 400 ",
            "`Invalid request: <zod message>`"
        ),
    },
    Row {
        variant: "FilePromote",
        field: "folder_path",
        rust_type: "Option<String>",
        v4: V4::BodySafeParse,
        note: concat!(
            "`files/[id]/shared.ts:12`, `actions/promote.ts:14` → 400 `Invalid ",
            "request: <zod message>`"
        ),
    },
    Row {
        variant: "FileDelete",
        field: "force",
        rust_type: "bool",
        v4: V4::Query,
        note: "`files/[id]/actions/delete.ts:29-30` `searchParams` (`=== 'true'`)",
    },
    Row {
        variant: "FileDelete",
        field: "dissociate",
        rust_type: "bool",
        v4: V4::Query,
        note: "`files/[id]/actions/delete.ts:29-30` `searchParams` (`=== 'true'`)",
    },
    Row {
        variant: "FilesFolderRename",
        field: "new_name",
        rust_type: "String",
        v4: V4::BodySafeParse,
        note: concat!(
            "`files/folders/route.ts:33,273` `safeParse` → an EXPLICIT ",
            "`validationError(...)` — the `{error:'Validation error',details:[…]}` ",
            "envelope"
        ),
    },
    // P4.73's images COLLECTION upload (added at the follow-ups-round-2
    // unification: the mechanical walk found the three transport fields the
    // moment the variant landed). Same shape as `FileUpload`'s trio — v4
    // `app/api/v1/images/route.ts:452-458` reads the multipart `File`
    // (`formData.get('file')`), so `filename`/`contentType`/`data` are the
    // decoded upload and a wrong JSON type has no v4 counterpart; only the
    // dispatch transport carries them as JSON.
    Row {
        variant: "ImageUpload",
        field: "filename",
        rust_type: "String",
        v4: V4::NoCounterpart,
        note: "`app/api/v1/images/route.ts:452-458` — multipart; the decoded `File`",
    },
    Row {
        variant: "ImageUpload",
        field: "content_type",
        rust_type: "String",
        v4: V4::NoCounterpart,
        note: "`app/api/v1/images/route.ts:452-458` — multipart; the decoded `File`",
    },
    Row {
        variant: "ImageUpload",
        field: "data",
        rust_type: "String",
        v4: V4::NoCounterpart,
        note: "`app/api/v1/images/route.ts:452-458` — multipart; the decoded `File`",
    },
    Row {
        variant: "FileUpload",
        field: "filename",
        rust_type: "String",
        v4: V4::NoCounterpart,
        note: concat!(
            "`files/actions/upload.ts:52-61` — multipart; ",
            "`filename`/`contentType`/`data` are the decoded `File`, `folderPath` a ",
            "form field"
        ),
    },
    Row {
        variant: "FileUpload",
        field: "content_type",
        rust_type: "String",
        v4: V4::NoCounterpart,
        note: concat!(
            "`files/actions/upload.ts:52-61` — multipart; ",
            "`filename`/`contentType`/`data` are the decoded `File`, `folderPath` a ",
            "form field"
        ),
    },
    Row {
        variant: "FileUpload",
        field: "data",
        rust_type: "String",
        v4: V4::NoCounterpart,
        note: concat!(
            "`files/actions/upload.ts:52-61` — multipart; ",
            "`filename`/`contentType`/`data` are the decoded `File`, `folderPath` a ",
            "form field"
        ),
    },
    Row {
        variant: "FileUpload",
        field: "tags",
        rust_type: "Option<Vec<String>>",
        v4: V4::BodyHandRolled,
        note: concat!(
            "`files/actions/upload.ts:38-44` — a form field holding a JSON STRING of ",
            "`{tagType,tagId}` objects; only malformed JSON is refused (400 `Invalid ",
            "tags JSON`), the element shape is never validated. v5's field is the ",
            "already-RESOLVED id list, so the shapes differ as well as the types ",
            "(P4.62(a) → P4.73)"
        ),
    },
    Row {
        variant: "FileUpload",
        field: "folder_path",
        rust_type: "Option<String>",
        v4: V4::NoCounterpart,
        note: concat!(
            "`files/actions/upload.ts:52-61` — multipart; ",
            "`filename`/`contentType`/`data` are the decoded `File`, `folderPath` a ",
            "form field"
        ),
    },
    Row {
        variant: "FilesGenerateThumbnails",
        field: "size",
        rust_type: "Option<i64>",
        v4: V4::BodySafeParse,
        note: concat!(
            "`files/shared.ts:26`, `actions/generate-thumbnails.ts:21` → 400 `Invalid ",
            "request: <zod message>`"
        ),
    },
    Row {
        variant: "FilesCleanupStale",
        field: "dry_run",
        rust_type: "Option<bool>",
        v4: V4::BodySafeParse,
        note: concat!(
            "`files/shared.ts:31`, `actions/cleanup-stale.ts:14` — `safeParse` with ",
            "NO failure branch: a wrong type is silently treated as absent and the ",
            "route proceeds on its default, 200"
        ),
    },
    Row {
        variant: "FilesCleanupOrphans",
        field: "mode",
        rust_type: "String",
        v4: V4::BodySafeParse,
        note: concat!(
            "`files/shared.ts:35`, `actions/cleanup-orphans.ts:81` — same ",
            "silent-swallow: wrong types fall back to `mode='delete'` / ",
            "`dryRun=true`, 200"
        ),
    },
    Row {
        variant: "FilesCleanupOrphans",
        field: "dry_run",
        rust_type: "Option<bool>",
        v4: V4::BodySafeParse,
        note: concat!(
            "`files/shared.ts:35`, `actions/cleanup-orphans.ts:81` — same ",
            "silent-swallow: wrong types fall back to `mode='delete'` / ",
            "`dryRun=true`, 200"
        ),
    },
    Row {
        variant: "ChatGetCost",
        field: "detailed",
        rust_type: "Option<bool>",
        v4: V4::Query,
        note: "`chats/[id]/handlers/get.ts:228` `searchParams` (`=== 'true'`)",
    },
    Row {
        variant: "ChatCustomToolRun",
        field: "tool",
        rust_type: "String",
        v4: V4::BodyParse,
        note: "`chats/[id]/custom-tools/route.ts:371,385` `runSchema.parse`",
    },
    Row {
        variant: "ChatCustomToolRun",
        field: "private",
        rust_type: "Option<bool>",
        v4: V4::BodyParse,
        note: "`chats/[id]/custom-tools/route.ts:371,385` `runSchema.parse`",
    },
    Row {
        variant: "CustomToolPreview",
        field: "private",
        rust_type: "Option<bool>",
        v4: V4::BodySafeParse,
        note: concat!(
            "`custom-tools/route.ts:93,118,213` `safeParse` → 400 `{error:'Invalid ",
            "request body', details:<zod issues>}`"
        ),
    },
    Row {
        variant: "LlmLogsList",
        field: "log_type",
        rust_type: "Option<String>",
        v4: V4::Query,
        note: concat!(
            "`llm-logs/route.ts:30-37` `searchParams` (limit/offset stay RAW strings ",
            "on both sides)"
        ),
    },
    Row {
        variant: "LlmLogsList",
        field: "standalone",
        rust_type: "bool",
        v4: V4::Query,
        note: concat!(
            "`llm-logs/route.ts:30-37` `searchParams` (limit/offset stay RAW strings ",
            "on both sides)"
        ),
    },
    Row {
        variant: "LlmLogsList",
        field: "include_messages",
        rust_type: "bool",
        v4: V4::Query,
        note: concat!(
            "`llm-logs/route.ts:30-37` `searchParams` (limit/offset stay RAW strings ",
            "on both sides)"
        ),
    },
    Row {
        variant: "LlmLogsList",
        field: "limit",
        rust_type: "Option<String>",
        v4: V4::Query,
        note: concat!(
            "`llm-logs/route.ts:30-37` `searchParams` (limit/offset stay RAW strings ",
            "on both sides)"
        ),
    },
    Row {
        variant: "LlmLogsList",
        field: "offset",
        rust_type: "Option<String>",
        v4: V4::Query,
        note: concat!(
            "`llm-logs/route.ts:30-37` `searchParams` (limit/offset stay RAW strings ",
            "on both sides)"
        ),
    },
    Row {
        variant: "SystemImageAestheticsGet",
        field: "kind",
        rust_type: "String",
        v4: V4::Query,
        note: "`system/image-aesthetics/route.ts:28` `searchParams`",
    },
    Row {
        variant: "SystemImageAestheticsSet",
        field: "kind",
        rust_type: "String",
        v4: V4::Query,
        note: "`system/image-aesthetics/route.ts:42` `searchParams`",
    },
    Row {
        variant: "SystemImageAestheticsSet",
        field: "content",
        rust_type: "Option<String>",
        v4: V4::BodySafeParse,
        note: concat!(
            "`system/image-aesthetics/route.ts:50` `safeParse(...).data?.content ?? ",
            "''` — a wrong type silently becomes `''`, which DELETES the aesthetic ",
            "file, and answers 200"
        ),
    },
    Row {
        variant: "PhotoGalleryList",
        field: "q",
        rust_type: "Option<String>",
        v4: V4::Query,
        note: concat!(
            "`photos/route.ts:29-45` `searchParams` (`limit`/`offset` are ",
            "`Number()`-coerced BEFORE Zod, which is why v5 carries them as `f64`)"
        ),
    },
    Row {
        variant: "PhotoGalleryList",
        field: "tag",
        rust_type: "Option<Vec<String>>",
        v4: V4::Query,
        note: concat!(
            "`photos/route.ts:29-45` `searchParams` (`limit`/`offset` are ",
            "`Number()`-coerced BEFORE Zod, which is why v5 carries them as `f64`)"
        ),
    },
    Row {
        variant: "PhotoGalleryList",
        field: "limit",
        rust_type: "Option<f64>",
        v4: V4::Query,
        note: concat!(
            "`photos/route.ts:29-45` `searchParams` (`limit`/`offset` are ",
            "`Number()`-coerced BEFORE Zod, which is why v5 carries them as `f64`)"
        ),
    },
    Row {
        variant: "PhotoGalleryList",
        field: "offset",
        rust_type: "Option<f64>",
        v4: V4::Query,
        note: concat!(
            "`photos/route.ts:29-45` `searchParams` (`limit`/`offset` are ",
            "`Number()`-coerced BEFORE Zod, which is why v5 carries them as `f64`)"
        ),
    },
    Row {
        variant: "PhotoGallerySave",
        field: "caption",
        rust_type: "Option<String>",
        v4: V4::BodySafeParse,
        note: "`photos/route.ts:22,70` `safeParse` → 400 `<messages joined by '; '>`",
    },
    Row {
        variant: "PhotoGallerySave",
        field: "tags",
        rust_type: "Option<Vec<String>>",
        v4: V4::BodySafeParse,
        note: "`photos/route.ts:22,70` `safeParse` → 400 `<messages joined by '; '>`",
    },
    Row {
        variant: "WardrobeList",
        field: "include_archived",
        rust_type: "bool",
        v4: V4::Query,
        note: "`wardrobe/route.ts:60` `readIncludeArchived`",
    },
    Row {
        variant: "SystemBackupCreate",
        field: "compact",
        rust_type: "bool",
        v4: V4::BodyHandRolled,
        note: concat!(
            "`system/backup/route.ts:27` `body.compact === true` — any other value ",
            "(wrong type included) is simply `false`; never a 400"
        ),
    },
    Row {
        variant: "SystemExportEntities",
        field: "entity_type",
        rust_type: "String",
        v4: V4::Query,
        note: "`system/tools/route.ts:470` `searchParams`",
    },
    Row {
        variant: "SystemExportPreview",
        field: "entity_type",
        rust_type: "String",
        v4: V4::Query,
        note: "`system/tools/route.ts:647-650` `searchParams`",
    },
    Row {
        variant: "SystemExportPreview",
        field: "scope",
        rust_type: "Option<String>",
        v4: V4::Query,
        note: "`system/tools/route.ts:647-650` `searchParams`",
    },
    Row {
        variant: "SystemExportPreview",
        field: "include_memories",
        rust_type: "bool",
        v4: V4::Query,
        note: "`system/tools/route.ts:647-650` `searchParams`",
    },
    Row {
        variant: "SystemTasksQueueControl",
        field: "action",
        rust_type: "String",
        v4: V4::BodyHandRolled,
        note: concat!(
            "`system/tools/route.ts:340` `!action || ",
            "!['start','stop'].includes(action)` → 400 `Invalid action. Must be ",
            "\\\"start\\\" or \\\"stop\\\"` for every wrong type"
        ),
    },
    Row {
        variant: "SystemJobConcurrencySet",
        field: "max_concurrent_jobs",
        rust_type: "i64",
        v4: V4::BodySafeParse,
        note: concat!(
            "`system/tools/route.ts:373,392` `safeParse` → explicit ",
            "`validationError(...)`. ⚠ v4's wire key is `concurrency`; v5's field is ",
            "`maxConcurrentJobs`"
        ),
    },
    Row {
        variant: "SystemJobControl",
        field: "action",
        rust_type: "String",
        v4: V4::Query,
        note: concat!(
            "`system/jobs/[id]/route.ts:78` `getActionParam` — v4 reads this from ",
            "`?action=`, never from a body"
        ),
    },
    Row {
        variant: "SystemDeleteData",
        field: "confirm",
        rust_type: "String",
        v4: V4::BodyHandRolled,
        note: concat!(
            "`system/tools/route.ts:173,182` — `confirm !== 'DELETE_ALL_MY_DATA'` → ",
            "the fixed 400; `keepArchivedCharacterBundles !== false` → any ",
            "non-`false` value means `true`, never a 400"
        ),
    },
    Row {
        variant: "SystemDeleteData",
        field: "keep_archived_character_bundles",
        rust_type: "Option<bool>",
        v4: V4::BodyHandRolled,
        note: concat!(
            "`system/tools/route.ts:173,182` — `confirm !== 'DELETE_ALL_MY_DATA'` → ",
            "the fixed 400; `keepArchivedCharacterBundles !== false` → any ",
            "non-`false` value means `true`, never a 400"
        ),
    },
    Row {
        variant: "ChatAnnouncementPost",
        field: "content_markdown",
        rust_type: "String",
        v4: V4::BodyParse,
        note: concat!(
            "`chats/[id]/schemas.ts:211`, `actions/announcement.ts:24` ",
            "`announcementSchema.parse`"
        ),
    },
    Row {
        variant: "ChatAnnouncementPost",
        field: "sender",
        rust_type: "AnnouncerSenderWire",
        v4: V4::BodyParse,
        note: concat!(
            "`chats/[id]/schemas.ts:211`, `actions/announcement.ts:24` ",
            "`announcementSchema.parse`"
        ),
    },
    Row {
        variant: "ChatAnnouncementPreview",
        field: "seed_markdown",
        rust_type: "String",
        v4: V4::BodyParse,
        note: concat!(
            "`chats/[id]/schemas.ts:226`, `actions/announcement-preview.ts:27` ",
            "`.parse`"
        ),
    },
    Row {
        variant: "ChatSendMail",
        field: "body_markdown",
        rust_type: "String",
        v4: V4::BodyParse,
        note: "`chats/[id]/schemas.ts:245`, `actions/send-mail.ts:28` `.parse`",
    },
    Row {
        variant: "ChatSendMail",
        field: "in_reply_to_path",
        rust_type: "Option<String>",
        v4: V4::BodyParse,
        note: "`chats/[id]/schemas.ts:245`, `actions/send-mail.ts:28` `.parse`",
    },
    Row {
        variant: "ChatAddParticipant",
        field: "display_order",
        rust_type: "Option<i64>",
        v4: V4::BodyParse,
        note: "`chats/[id]/schemas.ts:70`, `actions/participants.ts:250` `.parse`",
    },
    Row {
        variant: "ChatAddParticipant",
        field: "has_history_access",
        rust_type: "Option<bool>",
        v4: V4::BodyParse,
        note: "`chats/[id]/schemas.ts:70`, `actions/participants.ts:250` `.parse`",
    },
    Row {
        variant: "ChatAddParticipant",
        field: "join_scenario",
        rust_type: "Option<Option<String>>",
        v4: V4::BodyParse,
        note: "`chats/[id]/schemas.ts:70`, `actions/participants.ts:250` `.parse`",
    },
    Row {
        variant: "ChatAddParticipant",
        field: "controlled_by",
        rust_type: "Option<String>",
        v4: V4::BodyParse,
        note: "`chats/[id]/schemas.ts:70`, `actions/participants.ts:250` `.parse`",
    },
    Row {
        variant: "ChatUpdateParticipant",
        field: "display_order",
        rust_type: "Option<i64>",
        v4: V4::BodyParse,
        note: "`chats/[id]/schemas.ts:52`, `actions/participants.ts:468` `.parse`",
    },
    Row {
        variant: "ChatUpdateParticipant",
        field: "is_active",
        rust_type: "Option<bool>",
        v4: V4::BodyParse,
        note: "`chats/[id]/schemas.ts:52`, `actions/participants.ts:468` `.parse`",
    },
    Row {
        variant: "ChatUpdateParticipant",
        field: "status",
        rust_type: "Option<String>",
        v4: V4::BodyParse,
        note: "`chats/[id]/schemas.ts:52`, `actions/participants.ts:468` `.parse`",
    },
    Row {
        variant: "ChatUpdateParticipant",
        field: "controlled_by",
        rust_type: "Option<String>",
        v4: V4::BodyParse,
        note: "`chats/[id]/schemas.ts:52`, `actions/participants.ts:468` `.parse`",
    },
    Row {
        variant: "ChatUpdateParticipant",
        field: "has_history_access",
        rust_type: "Option<bool>",
        v4: V4::BodyParse,
        note: "`chats/[id]/schemas.ts:52`, `actions/participants.ts:468` `.parse`",
    },
    Row {
        variant: "ChatUpdateParticipant",
        field: "join_scenario",
        rust_type: "Option<Option<String>>",
        v4: V4::BodyParse,
        note: "`chats/[id]/schemas.ts:52`, `actions/participants.ts:468` `.parse`",
    },
    Row {
        variant: "ChatUpdateParticipant",
        field: "talkativeness",
        rust_type: "Option<Option<f64>>",
        v4: V4::BodyParse,
        note: "`chats/[id]/schemas.ts:52`, `actions/participants.ts:468` `.parse`",
    },
    Row {
        variant: "ChatBulkReattribute",
        field: "role_filter",
        rust_type: "Option<String>",
        v4: V4::BodyParse,
        note: "`chats/[id]/schemas.ts:161`, `actions/bulk.ts:25` `.parse`",
    },
    Row {
        variant: "ChatUpdateToolSettings",
        field: "disabled_tools",
        rust_type: "Vec<String>",
        v4: V4::BodyParse,
        note: concat!(
            "`chats/[id]/actions/tools.ts:18,77` `.parse`. ⚠ v4's fields are ",
            "`.default([])`, v5's are REQUIRED `Vec<String>` with no ",
            "`#[serde(default)]` — so an ABSENT key also diverges"
        ),
    },
    Row {
        variant: "ChatUpdateToolSettings",
        field: "disabled_tool_groups",
        rust_type: "Vec<String>",
        v4: V4::BodyParse,
        note: concat!(
            "`chats/[id]/actions/tools.ts:18,77` `.parse`. ⚠ v4's fields are ",
            "`.default([])`, v5's are REQUIRED `Vec<String>` with no ",
            "`#[serde(default)]` — so an ABSENT key also diverges"
        ),
    },
    Row {
        variant: "ChatRunTool",
        field: "tool_name",
        rust_type: "String",
        v4: V4::BodyParse,
        note: concat!(
            "`chats/[id]/actions/run-tool.ts:28,47` `.parse` (v4's wire key is ",
            "`toolName`)"
        ),
    },
    Row {
        variant: "ChatRunTool",
        field: "private",
        rust_type: "Option<bool>",
        v4: V4::BodyParse,
        note: concat!(
            "`chats/[id]/actions/run-tool.ts:28,47` `.parse` (v4's wire key is ",
            "`toolName`)"
        ),
    },
    Row {
        variant: "ChatRng",
        field: "rolls",
        rust_type: "Option<u32>",
        v4: V4::BodyParse,
        note: concat!(
            "`chats/[id]/actions/rng.ts:19,38` `.parse`. ⚠ v4's discriminating wire ",
            "key is `type`; v5 renamed it `kind` to dodge the serde tag"
        ),
    },
    Row {
        variant: "ChatRng",
        field: "preview",
        rust_type: "Option<bool>",
        v4: V4::BodyParse,
        note: concat!(
            "`chats/[id]/actions/rng.ts:19,38` `.parse`. ⚠ v4's discriminating wire ",
            "key is `type`; v5 renamed it `kind` to dodge the serde tag"
        ),
    },
    Row {
        variant: "ChatToggleAgentMode",
        field: "enabled",
        rust_type: "Option<Option<bool>>",
        v4: V4::BodyParse,
        note: "`chats/[id]/actions/agent-mode.ts:17,34` `.parse`",
    },
    Row {
        variant: "ToolsList",
        field: "include_schemas",
        rust_type: "Option<bool>",
        v4: V4::Query,
        note: "`tools/route.ts:439` `searchParams` (`=== 'true'`)",
    },
    Row {
        variant: "SearchReplacePreview",
        field: "search_text",
        rust_type: "String",
        v4: V4::BodySafeParse,
        note: concat!(
            "`search-replace/route.ts:18,72` `safeParse` — the issues are LOGGED and ",
            "discarded; the client gets the flat 400 `Invalid request`"
        ),
    },
    Row {
        variant: "SearchReplacePreview",
        field: "replace_text",
        rust_type: "String",
        v4: V4::BodySafeParse,
        note: concat!(
            "`search-replace/route.ts:18,72` `safeParse` — the issues are LOGGED and ",
            "discarded; the client gets the flat 400 `Invalid request`"
        ),
    },
    Row {
        variant: "SearchReplacePreview",
        field: "include_messages",
        rust_type: "Option<bool>",
        v4: V4::BodySafeParse,
        note: concat!(
            "`search-replace/route.ts:18,72` `safeParse` — the issues are LOGGED and ",
            "discarded; the client gets the flat 400 `Invalid request`"
        ),
    },
    Row {
        variant: "SearchReplacePreview",
        field: "include_memories",
        rust_type: "Option<bool>",
        v4: V4::BodySafeParse,
        note: concat!(
            "`search-replace/route.ts:18,72` `safeParse` — the issues are LOGGED and ",
            "discarded; the client gets the flat 400 `Invalid request`"
        ),
    },
    Row {
        variant: "SearchReplaceExecute",
        field: "search_text",
        rust_type: "String",
        v4: V4::BodySafeParse,
        note: "`search-replace/route.ts:18,40` — same flat 400 `Invalid request`",
    },
    Row {
        variant: "SearchReplaceExecute",
        field: "replace_text",
        rust_type: "String",
        v4: V4::BodySafeParse,
        note: "`search-replace/route.ts:18,40` — same flat 400 `Invalid request`",
    },
    Row {
        variant: "SearchReplaceExecute",
        field: "include_messages",
        rust_type: "Option<bool>",
        v4: V4::BodySafeParse,
        note: "`search-replace/route.ts:18,40` — same flat 400 `Invalid request`",
    },
    Row {
        variant: "SearchReplaceExecute",
        field: "include_memories",
        rust_type: "Option<bool>",
        v4: V4::BodySafeParse,
        note: "`search-replace/route.ts:18,40` — same flat 400 `Invalid request`",
    },
    Row {
        variant: "UiSearch",
        field: "q",
        rust_type: "Option<String>",
        v4: V4::Query,
        note: "`ui/search/route.ts:87-93` `searchParams`",
    },
    Row {
        variant: "UiSearch",
        field: "types",
        rust_type: "Option<String>",
        v4: V4::Query,
        note: "`ui/search/route.ts:87-93` `searchParams`",
    },
    Row {
        variant: "UiSearch",
        field: "limit",
        rust_type: "Option<String>",
        v4: V4::Query,
        note: "`ui/search/route.ts:87-93` `searchParams`",
    },
    Row {
        variant: "UiSearch",
        field: "offset",
        rust_type: "Option<String>",
        v4: V4::Query,
        note: "`ui/search/route.ts:87-93` `searchParams`",
    },
];

/// The `ChatCreate` trio (the order's anchor). These are NOT `Request` fields —
/// they live on `ChatCreateRequest`, one level below the raw
/// `serde_json::Map` the variant carries — so they are listed separately and
/// driven against that struct. See the header's correction.
const CHAT_CREATE_TRIO: &[Row] = &[
    Row {
        variant: "ChatCreateRequest",
        field: "concierge_state",
        rust_type: "Option<Option<Value>>",
        v4: V4::BodyParse,
        note: "FIXED(P4.73) — v4 `chats/route.ts:146,993` \
               `createChatSchema.conciergeState: z.enum([...]).optional()` under an \
               uncaught `.parse` → `{\"error\":\"Validation error\",\"details\":[…]}`. \
               v5 used to answer `invalid chatCreate request: …` from \
               `quilltap-host/src/spine.rs:1871`; since P4.73 the field is a raw \
               `Value` carrier and the handler answers the flat sentence \
               (capstone `cs_wrong_type_400`).",
    },
    Row {
        variant: "ChatCreateRequest",
        field: "roleplay_template_id",
        rust_type: "Option<Option<Value>>",
        v4: V4::BodyParse,
        note: "FIXED(P4.73) — v4 `roleplayTemplateId: z.uuid().nullable().optional()`; \
               same envelope, same fix (capstone `rt_wrong_type_400`). (`.nullable()` \
               here, so an explicit null is a deliberate choice on BOTH sides — only \
               a wrong TYPE diverged.)",
    },
];

/// `timestampConfig` — the trio's third member, and NOT a wrong-type row at
/// all. **Measured:** v5 already carries it as
/// `Option<Option<serde_json::Value>>` (`services/chat_create.rs`), so serde
/// accepts any JSON and the field reaches the handler intact — the P4.D57
/// shape, already in place. The order listed it beside the other two; what is
/// actually open for it is a HANDLER question (does v5 refuse a non-object the
/// way v4's `TimestampConfigSchema` does?), not a decode one, and that is
/// outside a census of the serde boundary. Asserted below so the day it stops
/// being a `Value` carrier, this file says so.
const TIMESTAMP_CONFIG_IS_ALREADY_A_VALUE_CARRIER: &str =
    "quilltap_core::services::chat_create::ChatCreateRequest::timestamp_config";

// ===========================================================================
// The mechanical half — `Request`'s own typed fields, walked from the source
// ===========================================================================

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|p| p.parent())
        .expect("the web crate sits two levels under the repo root")
        .to_path_buf()
}

fn types_rs() -> String {
    let p: PathBuf = repo_root().join("crates/quilltap-core/src/api/types.rs");
    std::fs::read_to_string(&p).unwrap_or_else(|e| panic!("read {}: {e}", p.display()))
}

/// Strip doc comments, line comments and attributes, keeping the code shape.
/// The census only needs names and types, and comments contain braces.
fn strip_noise(src: &str) -> String {
    src.lines()
        .map(|l| {
            let t = l.trim_start();
            if t.starts_with("//") || t.starts_with("#[") {
                ""
            } else {
                l
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// Split a body on top-level commas, counting only the delimiters in `open` /
/// `close`.
///
/// The two call sites need DIFFERENT delimiter sets, and getting that wrong is
/// silent: the variant split counts braces alone, because a stray `]` from a
/// multi-line attribute (or a `[u8]` in a type) would otherwise unbalance it
/// and make one variant swallow its neighbours — which is exactly what a first
/// draft of this parser did, reporting fields under the wrong variant name.
/// The field split additionally counts `<>` and `()` so `Option<Vec<String>>`
/// stays one field. Neither counts `[]`.
fn split_top_level(body: &str, open: &str, close: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut depth = 0i32;
    let mut cur = String::new();
    for ch in body.chars() {
        if open.contains(ch) {
            depth += 1;
        } else if close.contains(ch) {
            depth -= 1;
        }
        if ch == ',' && depth == 0 {
            out.push(std::mem::take(&mut cur));
        } else {
            cur.push(ch);
        }
    }
    out.push(cur);
    out
}

const VARIANT_OPEN: &str = "{";
const VARIANT_CLOSE: &str = "}";
const FIELD_OPEN: &str = "{<(";
const FIELD_CLOSE: &str = "}>)";

/// Every (`variant`, `field`, `type`) of `Request` whose type is not a
/// `serde_json::Value` — i.e. every field serde will type-check.
fn typed_request_fields() -> Vec<(String, String, String)> {
    let src = strip_noise(&types_rs());
    let start = src.find("pub enum Request").expect("the Request enum");
    let open = src[start..].find('{').expect("enum body") + start;
    let mut depth = 0i32;
    let mut end = open;
    for (i, ch) in src[open..].char_indices() {
        match ch {
            '{' => depth += 1,
            '}' => {
                depth -= 1;
                if depth == 0 {
                    end = open + i;
                    break;
                }
            }
            _ => {}
        }
    }
    let body = &src[open + 1..end];
    let mut out = Vec::new();
    for variant in split_top_level(body, VARIANT_OPEN, VARIANT_CLOSE) {
        let v = variant.trim();
        if v.is_empty() {
            continue;
        }
        let head = v.split('{').next().unwrap_or("").trim();
        let Some(name) = head.trim_end_matches(',').split_whitespace().next_back() else {
            continue;
        };
        let Some(bo) = v.find('{') else { continue };
        let Some(bc) = v.rfind('}') else { continue };
        for field in split_top_level(&v[bo + 1..bc], FIELD_OPEN, FIELD_CLOSE) {
            let f = field.split_whitespace().collect::<Vec<_>>().join(" ");
            let Some((fname, ftype)) = f.split_once(':') else {
                continue;
            };
            let fname = fname.replace("pub", "");
            let fname = fname.trim();
            let ftype = ftype.trim();
            if fname.is_empty() || ftype.contains("Value") {
                continue;
            }
            out.push((name.to_string(), fname.to_string(), ftype.to_string()));
        }
    }
    out
}

/// Route identifiers — the fields v5 carries because the dispatch channel has
/// no URL to take them from. They are excluded from the census by name, not by
/// judgment, and the rule is spelled here so it can be argued with.
fn is_route_identifier(field: &str) -> bool {
    field == "id"
        || field.ends_with("_id")
        || field.ends_with("_ids")
        || matches!(
            field,
            "path" | "relative_path" | "file_path" | "scenario_path" | "mount_point_id"
        )
}

/// **The exclusion is NOT totality, and this pins its size (the §3 review of
/// the follow-ups round 2).** `is_route_identifier` drops every `id` / `*_id` /
/// `*_ids` field on the premise that v4 takes them from the URL — but a large
/// part of what it drops are v4 BODY keys under an uncaught `.parse`:
/// `ChatSend.{file_ids, target_participant_ids, speaking_as_participant_id,
/// responding_participant_id}` (`orchestrator.service.ts:149-181`, parsed at
/// `chats/[id]/messages/route.ts:47-48`), `ChatAnnouncementPost.
/// target_participant_ids` and `ChatAnnouncementPreview.{character_id,
/// connection_profile_id, system_prompt_id, target_participant_ids}`
/// (`chats/[id]/schemas.ts:223-236`), `ChatSendMail.{from_character_id,
/// to_character_id}` (`:245-247`), `ChatUpdateParticipant.{connection_profile_id,
/// image_profile_id, selected_system_prompt_id}`, `ChatSetAvatar.image_id`,
/// `CharacterAvatar.image_id`, `ChatMergeConversation.source_chat_id`,
/// `SystemExportPreview.selected_ids`, `ChatBulkReattribute.
/// target_participant_id`, `MessageReattribute.new_participant_id`, and the
/// tag-add pairs — textbook `V4::BodyParse` rows this census does not carry.
/// The honest fix is a per-variant allow-list of the fields v4 really reads
/// from the URL (the first `*_id` of a variant whose v4 twin is `/[id]/`) and
/// classification of the remainder — a round of its own, recorded in
/// `phase-4.md`'s candidates. Until then the excluded set's size is asserted
/// here so it cannot grow unnoticed, and the header no longer calls the
/// census total.
const EXCLUDED_BY_THE_ROUTE_IDENTIFIER_RULE: usize = 403;

#[test]
fn census_covers_every_typed_request_field() {
    let all = typed_request_fields();
    let excluded = all
        .iter()
        .filter(|(_, f, _)| is_route_identifier(f))
        .count();
    assert_eq!(
        excluded, EXCLUDED_BY_THE_ROUTE_IDENTIFIER_RULE,
        "the route-identifier exclusion now drops {excluded} typed fields — re-measure \
         (and remember it is a heuristic that also drops real v4 body keys; see the \
         constant's doc)"
    );
    let mut expected: Vec<(String, String)> = all
        .into_iter()
        .filter(|(_, f, _)| !is_route_identifier(f))
        .map(|(v, f, _)| (v, f))
        .collect();
    expected.sort();
    expected.dedup();

    let mut have: Vec<(String, String)> = CENSUS
        .iter()
        .map(|r| (r.variant.to_string(), r.field.to_string()))
        .collect();
    have.sort();

    let missing: Vec<_> = expected.iter().filter(|e| !have.contains(e)).collect();
    let extra: Vec<_> = have.iter().filter(|h| !expected.contains(h)).collect();
    assert!(
        missing.is_empty(),
        "{} typed `Request` field(s) are NOT in the census — classify each one \
         (where does v4 read it, and what does v4 answer for a wrong JSON type?): {missing:?}",
        missing.len()
    );
    assert!(
        extra.is_empty(),
        "{} census row(s) name a field `Request` no longer has (renamed, retyped to \
         `Value`, or removed) — retire the row or update it: {extra:?}",
        extra.len()
    );
    // A floor, so a parser that silently found nothing cannot pass.
    assert!(
        expected.len() > 200,
        "the mechanical walk found only {} typed fields — the parser broke",
        expected.len()
    );
}

// ===========================================================================
// The executable half — the body rows really are serde-type-rejected TODAY
// ===========================================================================

fn camel(s: &str) -> String {
    // Variant names are PascalCase, field names snake_case; the enum carries
    // `#[serde(tag = "type", rename_all = "camelCase")]`.
    let mut out = String::new();
    let mut upper_next = false;
    for (i, ch) in s.chars().enumerate() {
        if ch == '_' {
            upper_next = true;
        } else if i == 0 {
            out.extend(ch.to_lowercase());
        } else if upper_next {
            out.extend(ch.to_uppercase());
            upper_next = false;
        } else {
            out.push(ch);
        }
    }
    out
}

/// A value of the WRONG JSON type for this Rust type.
fn wrong_value(rust_type: &str) -> Value {
    if rust_type.contains("String") && !rust_type.contains("Vec") {
        json!(42)
    } else if rust_type.contains("bool") {
        json!("nope")
    } else if rust_type.contains("Vec") {
        json!(42)
    } else {
        // i64 / u32 / f64 and the wire-typed enums.
        json!("nope")
    }
}

/// Decode `{"type": <verb>, <key>: <wrong>}` and report whether serde rejected
/// it on the field's TYPE. Both spellings of the key are tried, so a variant
/// without `rename_all` is not mistaken for a passing row.
fn serde_rejects_type(verb: &str, field: &str, rust_type: &str) -> Result<String, String> {
    let wrong = wrong_value(rust_type);
    let mut last = String::new();
    for key in [camel(field), field.to_string()] {
        let body = format!(
            "{{\"type\":\"{verb}\",\"{key}\":{wrong}}}",
            wrong = serde_json::to_string(&wrong).unwrap()
        );
        match serde_json::from_str::<quilltap_core::api::Request>(&body) {
            Ok(_) => last = format!("ACCEPTED with key `{key}`"),
            Err(e) => {
                let msg = e.to_string();
                if msg.contains("invalid type") || msg.contains("invalid value") {
                    return Ok(msg);
                }
                last = format!("key `{key}`: {msg}");
            }
        }
    }
    Err(last)
}

#[test]
fn body_sourced_rows_are_serde_type_rejected_today() {
    let mut proven = 0usize;
    let mut failed: Vec<String> = Vec::new();
    for row in CENSUS.iter().filter(|r| r.v4.is_body()) {
        let verb = camel(row.variant);
        match serde_rejects_type(&verb, row.field, row.rust_type) {
            Ok(_) => proven += 1,
            Err(why) => failed.push(format!("{}.{} — {why}", row.variant, row.field)),
        }
    }
    eprintln!("body-sourced rows proven serde-type-rejected: {proven}");
    assert!(
        failed.is_empty(),
        "{} census row(s) did NOT reject a wrong JSON type at the dispatch decode. \
         Either the field stopped being typed (a fix landed — retire the row and \
         record v4's answer as reached) or the probe's verb/key is wrong: {failed:#?}",
        failed.len()
    );
    // The floor: the class is real and large. If this collapses, the probe
    // stopped reaching the decode.
    assert!(
        proven >= 100,
        "only {proven} body rows were proven — the probe broke"
    );
}

#[test]
fn chat_create_trio_fixed_by_p4_73_decodes_raw_so_the_handler_can_refuse() {
    use quilltap_core::services::chat_create::ChatCreateRequest;

    // The minimum a `ChatCreateRequest` needs, so what the probe measures is
    // the wrong TYPE and not a missing sibling. Post-fix, a wrong type must be
    // ACCEPTED by the decode (the host driver's `invalid chatCreate request`
    // envelope is no longer reachable for these two) — the refusal lives in the
    // handler and is pinned by the capstone family, not here.
    let base = json!({ "participants": [] });
    for row in CHAT_CREATE_TRIO {
        assert!(
            row.note.starts_with("FIXED(P4.73)"),
            "{}.{} must carry its fixing owner",
            row.variant,
            row.field
        );
        assert_eq!(
            row.rust_type, "Option<Option<Value>>",
            "{}.{} must be recorded as the raw carrier it now is",
            row.variant, row.field
        );
        let mut body = base.clone();
        body.as_object_mut()
            .unwrap()
            .insert(camel(row.field), json!(42));
        let decoded = serde_json::from_value::<ChatCreateRequest>(body);
        assert!(
            decoded.is_ok(),
            "`{}` REJECTED a wrong-typed value at the decode — P4.73's widening has \
             regressed and the host envelope is reachable again: {decoded:?}",
            row.field
        );
    }

    // `timestampConfig` is the trio's third member and is NOT a decode row: it
    // already carries a `Value`, so serde accepts any JSON type. The day that
    // changes, this assertion is where it shows.
    let ts = serde_json::from_value::<ChatCreateRequest>(json!({
        "participants": [],
        "timestampConfig": "not-an-object",
    }));
    assert!(
        ts.is_ok(),
        "{TIMESTAMP_CONFIG_IS_ALREADY_A_VALUE_CARRIER} stopped accepting any JSON — \
         it is now a decode row and belongs in the census: {ts:?}"
    );

    // The order predicted `dispatch.rs:72`; the decode is one level down, so
    // `Request` itself accepts the very body the trio rejects. That is the
    // correction the header records, and it is asserted rather than asserted-in-prose.
    let accepted = serde_json::from_str::<quilltap_core::api::Request>(
        r#"{"type":"chatCreate","participants":[],"conciergeState":42}"#,
    );
    assert!(
        accepted.is_ok(),
        "`Request::ChatCreate` carries a raw Map, so the dispatch decode must ACCEPT \
         a wrong-typed `conciergeState`: {accepted:?}"
    );
}

#[test]
fn the_census_reads_the_file_it_claims_to() {
    let p: PathBuf = repo_root().join("crates/quilltap-core/src/api/types.rs");
    assert!(Path::new(&p).is_file(), "{} is missing", p.display());
    assert!(
        types_rs().contains("pub enum Request"),
        "types.rs no longer declares `pub enum Request`"
    );
}
