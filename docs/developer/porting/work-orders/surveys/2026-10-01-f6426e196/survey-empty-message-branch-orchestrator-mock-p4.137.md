# Survey — P4.137: the empty new-user-message branch (P4.D243-F1) + lifting the orchestrator oracle's cheap-key mock

**Date:** 2026-10-01 · **v4:** `f6426e196` (clean) · **v5 `main`:** `6d44cfae2`
**Kind:** read-only measurement (no repo file touched but this one; no cargo /
npm / node / jest run).

Two items, ONE lane, because both land in the SAME family — `orchestrator_tier3`
(its oracle `harness/oracle/cases/orchestrator-tier3.test.ts`, its Rust half
`crates/quilltap-harness/tests/orchestrator_tier3_equivalence.rs`, its spec
`harness/oracle/fixtures/orchestrator-tier3.json`) — and two lanes regenerating
that family in one round is the collision every round record warns about.

**The finding in one line:** v4 routes an empty `newUserMessage` (`''` — admitted
by the send schema whenever files or pending tool results ride) to buildContext's
CHAINED branch by JS truthiness, persisting NO user row; v5 admits the same send
(same gate, byte-identical message) but `build_context`'s `if let Some(..)` takes
the FIRST-RESPONDER branch on `Some("")`, pushing an empty user message, flagging
it the attachment anchor, counting it (+1 message, +4 tokens) and moving
`historyTailHash` — three consumers in `build_context.rs` must become v4's
truthiness, every other `new_user_message` reader already is, and the proof is
one new op in `build_context_tier3` (an op may say `newUserMessage: ""` TODAY on
both sides) plus one new `orchestrator_tier3` case (`content: ""` +
`pendingToolResults` — the cheapest reachable shape; a second with an image on
the vision seat if the anchor fallback is wanted). The cheap-key mock lift is
mechanical and predicted ALL-GREEN: every cheap call in the corpus resolves to
`CheapDefault` (`f0000006…`, bound to `key-cheap-default`), the three refusing
P4.133 arms throw at the inline requires-gate (`:433`) BEFORE the cheap
selection is even computed (`:515`), and the mocked `getApiKeyForProfile` is a
key the real module never exports. ⚠ A **v5 SPA divergence** found on the way:
v4's client substitutes `'Please look at the attached file(s).'` for an
attachment-only send (`useSSEStreaming.ts:845`); the v5 composer sends `''`
through (`salon-conversation.ts:3654`) — so on v5 an attachment-only Salon send
reaches the server in the very shape this order is about, persists NO user row
and links NO file, where v4's Salon persists a sentence. Not this lane's file —
escalated as an SPA follow-up (§D7).

---

## §A — v4 (`f6426e196`)

### A1. The send schema admits empty content with files or tool results

`lib/services/chat-message/orchestrator.service.ts:148-171`:

```ts
/**
 * Validation schema for send message
 * Content can be empty if there are pending tool results or file attachments
 */
export const sendMessageSchema = z.object({
  content: z.string().default(''),
  fileIds: z.array(z.string()).optional(),
  /** Pending tool results to be saved as TOOL messages before the user message */
  pendingToolResults: z.array(pendingToolResultSchema).optional(),
  …
}).superRefine((data, ctx) => {
  if (data.content.trim().length === 0 &&
      (!data.fileIds || data.fileIds.length === 0) &&
      (!data.pendingToolResults || data.pendingToolResults.length === 0)) {
    ctx.addIssue({
      code: z.ZodIssueCode.custom,
      message: 'Message must have content, attached files, or tool results',
    })
  }
})
```

Two things the refinement does NOT do: it tests `trim()` (so `'   '` with no
files is refused) but the orchestrator's branch tests the RAW string (so `'   '`
WITH a file is admitted and is TRUTHY downstream — a whitespace user message is
persisted and takes the first-responder branch). The route parses the body
(`app/api/v1/chats/[id]/messages/route.ts:48` `buildSendMessageOptions(
sendMessageSchema.parse(body), …)`) and `request-helpers.ts:37` hands
`content: parsed.content` through unchanged — an absent `content` arrives as `''`.
The oracle bypasses the route and calls `handleSendMessage` (`:190`) directly
with `content: call.continueMode ? undefined : call.content` (`orchestrator-
tier3.test.ts:833`), i.e. the same `''` the route would produce.

### A2. Every `options.content` / `content` gate in `processMessage` is truthiness

| line | gate | effect on `''` |
|---|---|---|
| `:313` | `hasContent: !!options.content` (the paused-chat INFO line) | `false` |
| `:339-348` | the two-user-seat fairness pause: `!isContinueMode && !holdForPausedChat && … && !!options.content && …` | never pauses |
| `:708` | `if (!isContinueMode && options.pendingToolResults && options.pendingToolResults.length > 0)` — TOOL rows persisted + pushed into `existingMessages` | runs (content-independent) |
| `:739` | RNG auto-detect `if (autoDetectRng && !isContinueMode && options.content)` | skipped |
| `:785-837` | **the user-message persist** `if (!isContinueMode && options.content) { content = options.content; userMessageId = …; addMessage(…); for (file of fileProcessing.attachedFiles) addLink(file.id, userMessageId); postLibrarianUploadAnnouncement(…); dangerFlags attach }` | **ENTIRE block skipped: no USER row, no `files.addLink`, no Librarian upload announcement; `content` stays `''`, `userMessageId` stays `null`** |
| `:850` | Carina markup `if (!isContinueMode && content)` | skipped |
| `:896-898` | `const finalUserMessageContent = isContinueMode ? undefined : (fileProcessing.messageContentPrefix ? fileProcessing.messageContentPrefix + content : content)` | `''` when no prefix; `prefix + ''` (TRUTHY) when a fallback prefix exists |
| `:1196` | `newUserMessage: finalUserMessageContent` into `buildMessageContext` | `''` |
| `:1486` | `originalMessage: options.content` into the streaming state | `''` |
| `pre-compute.service.ts:220-225` | `if (!isContinueMode && content) messagesSinceLastSpoke.push({ role: 'USER', content })` | not pushed |

The hold path (`:872-887` → `finishHeldUserTurn`) and the fairness writer at
`:1960-1968` (`content: options.content ?? ''`) are both unreachable on `''`:
the hold runs the persist block above first (skipped on `''` the same way) and
the fairness guard is gated by `!!options.content` at `:348`.

**So an attachment-only send in v4 links nothing and persists nothing** — the
files stay linked to the CHAT (they were linked at upload; `loadAndProcessFiles`
reads `repos.files.findByLinkedTo(chatId)` and filters to `fileIds`,
`context-builder.service.ts:172-174`) and ride this ONE turn as attachments.

### A3. Where the prefix comes from (decides which branch a file-only send takes)

`context-builder.service.ts:156-234` `loadAndProcessFiles`: per matched file,
`processFileAttachmentFallback` (`lib/chat/file-attachment-fallback.ts`) answers
`{ type: 'unsupported' }` with NO error when `!needsFallbackProcessing(profile,
mimeType)` — i.e. the provider can receive the attachment natively
(`image-transport.ts:70-77` `profileCanReceiveAttachment`: the profile's
`supportsImageUpload` AND `providerCanTransportImages`); then
`formatFallbackAsMessagePrefix` adds nothing and the attachment is KEPT in
`attachmentsToSend` (`:223-226`). A text file (`isTextFile`: `text/*`,
`application/json`, `application/xml`) is inlined (`convertTextFileToInline`) →
a non-empty prefix. Consequence for the order's arms:

- **image on a vision seat** (fixture `LanternVision`, the only profile with
  `supportsImageUpload: true`): prefix `''` → `finalUserMessageContent = ''` →
  the CHAINED branch, attachment anchored by fallback (A5).
- **text/markdown file** (fixture `fe000001… transcript.md`): prefix non-empty →
  `finalUserMessageContent = prefix` (truthy) → the FIRST-RESPONDER branch with
  the prefix as the whole user message — and STILL no persisted row.
- **`pendingToolResults` only** (no files): prefix `''` → `''` → CHAINED branch;
  the TOOL rows persisted at `:708` and in `existingMessages`. The cheapest
  deterministic shape (no vision seat, no bytes).

### A4. Every `newUserMessage` consumer in `buildContext` (`lib/chat/context-manager.ts`)

| line | test | on `''` |
|---|---|---|
| `:703-705` `buildRecentWindowQuery`: `if (newUserMessage && newUserMessage.trim().length > 0) parts.push(newUserMessage.trim())` | truthy + non-blank | not pushed |
| `:1300-1305` `recapRelevanceQuery = newUserMessage \|\| (existingMessages.length > 0 ? existingMessages[last].content : '') \|\| chat.scenarioText \|\| chat.contextSummary \|\| ''` | `\|\|` chain | falls to the last message |
| `:1362` `buildRecentWindowQuery(existingMessages, newUserMessage)` | (above) | — |
| `:1436` `if (newUserMessage) recentForDistill.push({ role: 'user', content: newUserMessage })` | truthy | not pushed |
| `:2101` `const newUserMessageTokens = newUserMessage ? estimateTokens(newUserMessage, provider) + 4 : 0` | truthy | **0** |
| `:2714` `hasNewUserMessage: !!newUserMessage` (the scene-note input) | truthy | `false` → the note MAY apply |
| `:2726` **`if (newUserMessage) {` — the first-responder push** (name resolution, the five trailing sections joined under the message, `metadata: { isUserTurn: true }`) | truthy | **NOT taken** |
| `:2748` `} else if (userNarrationAnchor \|\| turnSkipInstruction \|\| progressionsLLMContext) {` — the trailing-only user message `[anchor, progressions, turnSkip].filter(Boolean).join('\n\n---\n\n')` | — | **taken when any is non-empty; otherwise NOTHING is pushed** |
| `:2766` `totalUsed = … + messagesTokens + newUserMessageTokens` | — | +0 |
| `:2776` `recentMessages: messagesTokens + newUserMessageTokens` | — | +0 |
| `:2782` `messagesIncluded: selectedMessages.length + (newUserMessage ? 1 : 0)` | truthy | **+0** |

`regenerate-swipe.service.ts:202` passes `newUserMessage: undefined` (a swipe);
`context-builder.service.ts:61` types it `newUserMessage?: string` and hands it
through at `:877` and `:1085` untouched.

### A5. The attachment anchor and the tail hash (the downstream consequences)

- `context-builder.service.ts:730-745` `selectAttachmentAnchorIndex`: preference
  1 = the message with `metadata.isUserTurn` (set ONLY by the `:2726` branch);
  2 = the last `role: user` whose `messageId` is in `userTurnMessageIds`; 3 = the
  last `role: user` of any kind; else `-1`. `:1272-1276`: with attachments and
  `-1`, the WARN `Image attachments could not be anchored — no user-role message
  in context; images will not reach the model` `{ attachmentCount,
  contextMessageCount }` and the attachments are dropped (`:1280-1290`). So an
  image-only send in v4 anchors on the human's PREVIOUS turn (preference 2) —
  or, in a chat with no prior human turn and no trailing-only note, is dropped
  with the WARN.
- `lib/llm/cache-prefix-hashes.ts:73-82`: `historyTailHash` hashes every
  non-system message EXCEPT THE LAST. With no empty user message pushed, the
  frozen window ends one message earlier than v5's today — so the persisted
  `CHAT_MESSAGE` row's `historyTailHash` cell moves too (a second, independent
  red on the new arm, in `llm_logs`, not just in the message array).
- `isInitialMessage` (`context-builder.service.ts:1049`) counts USER rows in
  HISTORY, independent of `newUserMessage` — unchanged.

### A6. The v4 client never sends the empty-with-files shape from the Salon

`app/salon/[id]/hooks/useSSEStreaming.ts:771` refuses a send with no text, no
files and no pending results; `:845`:

```ts
content: userMessage || (attachedFiles.length > 0 ? 'Please look at the attached file(s).' : ''),
```

— an attachment-only send gets the sentence; a **pending-tool-results-only**
send (no text, no files) still sends `content: ''`. `ChatComposer.tsx:545`
enables Send on `hasContent || attachedFiles.length || pendingToolResults.length`.
So in v4 production the `''` branch is reached by (a) a tool-results-only post,
(b) any direct API caller (the CLI, curl). The v5 SPA reaches it on EVERY
attachment-only send (§B6).

---

## §B — v5 on `main` (`6d44cfae2`)

### B1. The send gate matches v4 and ADMITS the shape

`crates/quilltap-core/src/api/types.rs:378-381` `Request::ChatSend { chat_id,
#[serde(default)] content: String, … #[serde(default)] file_ids: Vec<String>,
#[serde(default)] pending_tool_results: Vec<PendingToolResult> }` — an absent
`content` decodes to `""` (v4's `.default('')`). `api/engine.rs:1981-1992`:

```rust
if !continue_mode
    && content.trim().is_empty()
    && file_ids.is_empty()
    && pending_tool_results.is_empty()
{
    return Response::error(ErrorKind::BadRequest,
        "Message must have content, attached files, or tool results");
}
```

(the whitespace arm is pinned at `:7395-7410`). The web transport has NO REST
`POST /api/v1/chats/{id}/messages` (grep of `crates/quilltap-web/src/lib.rs`:
only `/files`, `/qtap-target`, `/custom-tools`, the chat itself) — the Salon
sends through dispatch; `api/chat_send.rs:26-39` `ChatSendRequest { content:
String, file_ids, pending_tool_results, … }`; the host spine maps it 1:1 at
`crates/quilltap-host/src/spine.rs:1566-1575` (`content: req.content.clone()`).
So `process_message` receives `options.content == ""` with `file_ids` /
`pending_tool_results` populated — **`Some("")` is reachable from dispatch
today, and from the SPA (B6).**

### B2. The orchestrator's gates already agree with v4 (truthiness = non-empty)

`crates/quilltap-core/src/services/orchestrator.rs`: `:386-389`
`SendMessageOptions { content: String, … }`; `:1064` `has_content =
!input.options.content.is_empty()`; `:1119` the fairness pause's
`&& !input.options.content.is_empty()`; `:1460` the danger route's
`&& !input.options.content.is_empty()` (v4's gate is `!!options.content` at
`:348`/`:1986` — same); `:1742` RNG `&& !input.options.content.is_empty()`;
`:1795-1800` **the persist** `if !is_continue_mode &&
!input.options.content.is_empty() { content = …; … add_link … }` (skipped on
`""` exactly as v4's `:785`); `services/pre_compute.rs:182` `if
!is_continue_mode && !content.is_empty()`. Carina on the user path is the
standing unwired seam (`:1849-1856` comment). The one build that differs:

`:1869-1877`:

```rust
let final_user_message = if is_continue_mode {
    None
} else {
    Some(match &file_processing.message_content_prefix {
        Some(prefix) => format!("{prefix}{content}"),
        None => content.clone(),
    })
};
```

→ `Some("")` for a non-continue send with no prefix (`chat_files.rs:542-546`
makes the prefix `None` when empty, so `prefix + ''` is `Some(prefix)` exactly as
v4). Threaded at `:2436` `final_user_message: final_user_message.clone()` →
`build_context_input` `:4543` `new_user_message: args.final_user_message`.
`Some("")` is the faithful carrier of v4's `''`; the defect is in the readers.

### B3. Every `new_user_message` reader in `build_context.rs` — three disagree

| line | v5 today | v4 twin | agrees? |
|---|---|---|---|
| `:1062-1066` `build_recent_window_query`: `if let Some(n) { let t = js_trim(n); if !t.is_empty() { push } }` | trim + non-empty | `:703` | ✓ |
| `:2339-2342` recap query `.new_user_message.clone().filter(\|s\| !s.is_empty()).or_else(…)` | non-empty | `:1301` | ✓ |
| `:2414-2415` the window query call | — | `:1362` | ✓ |
| `:2504` distill `if let Some(num) = input.new_user_message.as_ref().filter(\|s\| !s.is_empty())` (comment: "v4 `if (newUserMessage)` — a truthy (non-empty) string only") | non-empty | `:1436` | ✓ |
| **`:3229-3233`** `new_user_message_tokens = input.new_user_message.as_deref().map(\|m\| estimate_tokens(m, cpt) + 4).unwrap_or(0)` | **`Some("")` → 4** | `:2101` → 0 | **✗** |
| `:3736-3739` the scene-note `has_new_user_message: … .as_deref().is_some_and(\|s\| !s.is_empty())` | non-empty | `:2714` | ✓ (P4.D243) |
| **`:3750`** `if let Some(new_user_message) = &input.new_user_message {` — the first-responder push (`:3806-3826`: `composed` = the empty string, or `"\n\n---\n\n" + trailing` under it; `is_user_turn: Some(true)`; `messages_included += 1`) | **`Some("")` → taken** | `:2726` | **✗ — THE defect** |
| `:3827-3830` `} else if !user_narration_anchor.is_empty() \|\| !turn_skip_instruction.is_empty() \|\| !progressions_llm_context.is_empty() {` | — | `:2748` | ✓ once reached |
| **`:3826` / `:3887`** `messages_included` (+1 inside the branch) | **+1** | `:2782` +0 | **✗ (fixed by the branch)** |
| `:3863`, `:3881` `total_used` / `recent_messages` + `new_user_message_tokens` | +4 | +0 | ✗ via `:3229` |

The unit pin `build_context.rs:5080-5085` `an_empty_new_user_message_is_no_new_
message_to_the_gate` (P4.D243) sets `t.new_user_message = Some(String::new())`
and asserts ONLY that the anchor's DEBUG line fires once — it does not look at
what was pushed, which is why the half-first-responder turn survived the lane.

**What v4's truthiness means in Rust, stated once:** a NEW user message exists
iff `input.new_user_message.as_deref().is_some_and(|s| !s.is_empty())` — the
exact predicate P4.D243 already wrote at `:3736-3739`. The `:3750` branch, the
`:3229` token count and (through the branch) `messages_included` must bind to
the SAME predicate, so one turn is never half-first-responder. The right shape
is one `let new_user_message: Option<&str> = input.new_user_message.as_deref()
.filter(|s| !s.is_empty());` computed once above `:3229` and consumed by the
anchor input, the token count and the branch (the three JS-truthiness sites
that today read the raw `Option`). `Some("   ")` stays a new message on both
sides (A1) — do NOT trim here; `build_recent_window_query` trims on its own as
v4 does.

### B4. What the first-responder branch pushes for `Some("")` today

`:3806-3826`: `content: ""` (or `"\n\n---\n\n<sections>"` when any trailing
section exists), `name` resolved for a multi-character chat, `metadata.is_user_
turn = Some(true)` — so `message_context.rs:1511-1519` picks THIS empty message
as the attachment anchor (preference 1), the images ride on an empty user line,
and v4's preference-2/3 fallback (and the `:1521-1526` WARN twin, byte-identical
to v4's `:1273`) is never exercised on this shape. The scene note is computed
(`:3733-3747`, the DEBUG line fires) but the `else if` is never reached, so the
note — and on a chained-with-skip shape the turn-skip note and progressions —
are NOT pushed on their own; they ride as trailing sections UNDER the empty
message instead (a different byte stream: `"\n\n---\n\n" + …` versus v4's bare
`trailingOnly.join(…)`).

### B5. The `BuildContextInput {` literals (five, all unaffected)

`build_context.rs:511` (the struct), `:4451` (`a_blocked_turn`, `Some(…)`),
`:4785` (`a_turn`, `Some(…)`; `:4959` `chained` reuses it and sets `None` at
`:4961`), `orchestrator.rs:4532` (production, from `final_user_message`), and
`crates/quilltap-harness/tests/build_context_tier3_equivalence.rs:807` (the
family, `new_user_message: op.new_user_message.clone()` at `:869` from `SpecOp
{ #[serde(default)] new_user_message: Option<String> }` `:103-104`). None needs
a field change: the fix is in the readers, not the input. `message_context.rs`
reads NO `new_user_message` at all (grep: only `user_turn_message_ids` and the
`is_user_turn` metadata, `:113`, `:1301`, `:1426`, `:1511-1519`) — it sees the
branch's EFFECT (the `is_user_turn` flag), which is why fixing the branch fixes
the anchor for free.

### B6. ⚠ The v5 SPA sends `''` where v4's client sends a sentence

`apps/web/src/app/chat/chat-composer.ts:714-718` `hasContentToSend = text().
trim().length > 0 || attachedFiles().length > 0 || pendingToolResults().length
> 0` (`:736-737` `canSend`, `:751` placeholder `'Add a message (optional)…'`
when files are attached); `screens/salon/salon-conversation.ts:3627` `if
(opts.content || hasAttachments || pending.length > 0)` → `:3654` `content:
opts.content` — no substitution anywhere (`ggrep -rn 'attached file' apps/web/
src` finds only the COMMENT at `chat/transcript-reconcile.ts:182`, which
describes v4's sentence as if v5 stored it). So an attachment-only Salon send on
v5 persists no row and links no file (B2), and the transcript shows nothing for
the human's post; on v4 it persists the sentence with the files linked. A real
divergence in the SPA, outside this lane's files — §D7.

---

## §C — the families

### C1. `build_context_tier3` — an op may already say `newUserMessage: ""`

- Oracle: `harness/oracle/cases/build-context-tier3.test.ts:79` `newUserMessage?:
  string;` → `:426` `newUserMessage: op.newUserMessage,` — passed through
  verbatim, so `""` reaches `buildContext` as `''`.
- Rust: `build_context_tier3_equivalence.rs:103-104` `#[serde(default)]
  new_user_message: Option<String>` → `:869` `op.new_user_message.clone()` —
  `""` deserializes to `Some("")`.
- Spec `harness/oracle/fixtures/build-context-tier3.json`: 58 ops; `newUserMessage`
  is ABSENT on 12 (`progressions_chained_no_user_message`,
  `progressions_chained_with_turn_skip`, `narration_anchor_chained_multi_
  applies`, …) and a non-empty string on 46; **no op says `""` today.** The
  chained ops prove the `else if` on `undefined`/`None`; nothing proves it on
  `''`/`Some("")`.
- New ops (red-first on v5 before the fix): (i) `empty_new_user_message_single`
  — `newUserMessage: ""` on the plain single-character shape (v4 pushes NO
  trailing message; v5 today pushes `content: ""` with `isUserTurn`), (ii)
  `empty_new_user_message_chained_multi_with_skip` — `newUserMessage: ""` on
  the `narration_anchor_chained_multi_applies` shape plus `turnSkip`, so the
  trailing-only message `[anchor, …, skip].join('\n\n---\n\n')` is what v4 pushes
  and v5 today pushes `"\n\n---\n\n" + …` under an empty line (distinct bytes —
  the arm cannot pass by accident), (iii) `whitespace_new_user_message` —
  `newUserMessage: "   "` (a NEW message on both sides; guards against an
  over-eager trim at the branch). The family's compared shape already includes
  `messages` and `tokenUsage`/`messagesIncluded` per the P4.D168/P4.D243 ops
  (confirm in the recipe: `python3 harness/tools/recipe_sweep.py --show
  build_context_tier3`).

### C2. `orchestrator_tier3` — the case shape, and the arm that reaches `process_message` with `Some("")`

- A case (`orchestrator-tier3.test.ts:97-121` `CallSpec`): `{ name, kind:
  'single'|'chain', chatId, content: string, continueMode: boolean, streamLabel,
  cheapLLMSettings: boolean, summaryCheck: boolean, respondingParticipant?,
  expectThrow?, rngBytes?, nudge?, pendingToolResults?, fileIds?,
  neverPauseForUser? }`. The oracle calls `handleSendMessage(repos, chatId,
  userId, { content: call.continueMode ? undefined : call.content, continueMode,
  respondingParticipantId, nudge, pendingToolResults, fileIds,
  neverPauseForUser })` (`:832-840`); the Rust side builds `SendMessageOptions {
  continue_mode, content: content.to_string(), … pending_tool_results: if
  continue_mode { vec![] } else { call_ptrs }, file_ids: if continue_mode {
  vec![] } else { call_file_ids } }` (`orchestrator_tier3_equivalence.rs:
  1739-1765`). **So a case `{ content: "", continueMode: false, pendingToolResults:
  [...] }` reaches v4's `processMessage` with `''` and v5's `process_message`
  with `content: ""` → `final_user_message = Some("")` — no harness change
  needed to express the arm.**
- Spec facts (`orchestrator-tier3.json`, 70 `calls`): `content: ""` appears
  ONLY with `continueMode: true` (every continue case); every `continueMode:
  false` case has non-empty content. ONE case carries `fileIds`
  (`paused_hold_attachment`: content `"The ledger, for the record."`, paused
  chat `ed000002…`, file `fe000001…` text/markdown). `rehydrate_user_attachments`
  is a CONTINUE case (`content: ""`, `continueMode: true`, chat `fe100001…` whose
  history holds a USER row with attachments) — attachments come from history,
  not from the send. **No case sends empty content with files or tool results.**
- The `files` seed (`build-orchestrator-fixture.ts:461-490`): every spec file is
  linked to EVERY corpus chat (`linkedTo: chatIds`), so `loadAndProcessFiles`'s
  `findByLinkedTo(chatId)` matches any spec file on any chat. The eleven files:
  `fe000001` text/markdown, `fe000002` application/pdf, `fe000003`
  application/zip, `1f151001-8` image/webp (the Lantern-budget set).
- Seats: the vision profile is `LanternVision` (`1a151000…`, ANTHROPIC
  `claude-sees`, `supportsImageUpload: true` — the only one); its LLM seats are
  Lucida's on chats `1e151001-4`. `single_basic`/`continue_mode`/
  `pending_tool_results` run Friday on `Primary` (ANTHROPIC `claude-sonnet`,
  `supportsImageUpload` absent → an image there takes the describe-fallback,
  which the corpus does not want to open).
- **Arms to add:** (1) `empty_content_pending_tool_results` — a fresh single
  chat (Friday/Primary, no history), `content: ""`, `continueMode: false`,
  `pendingToolResults` = the existing `roll_dice` chip; v4: the TOOL row
  persisted, NO user row, the stream's messages carry no empty user line, the
  `CHAT_MESSAGE` row's `historyTailHash` over the shorter frozen window; v5
  today: an empty `isUserTurn` message + `messages_included` + 4 tokens →
  RED-FIRST on the `messages` comparand AND the `llm_logs` cell. (2) OPTIONAL
  `empty_content_image_on_vision_seat` — a new single chat on `LanternVision`
  with ONE prior USER row in history, `content: ""`, `fileIds: ["1f151001…"]`:
  no prefix → `''` → chained branch; the attachment anchors on the prior human
  row (preference 2) on both sides; without the prior row it is the
  `-1` WARN + drop (a unit-test arm, not a corpus one — the corpus should not
  send bytes nowhere). The spec needs a `streams` entry per new `streamLabel`
  (`:97` `streamLabel`; the `streams` map keys are the labels). (3) NOT a corpus
  arm: `content: ""` + `fe000001…` (text) — the prefix makes it a first-responder
  turn with the inlined transcript as the user message; it is v4-faithful today
  and adds a second compressed-text comparand for no new branch.
- Regen: `orchestrator_tier3` is regenerated from the SPEC through the sweep
  driver (`python3 harness/tools/recipe_sweep.py --show orchestrator_tier3`),
  pinned at the round's v4 pin. Adding a case moves NO other case's bytes
  (each case is its own chat) — but item (e) below changes the same oracle
  file, so ONE regen covers both.

### C3. Item (e) — lifting the cheap-key mock

- The block, `orchestrator-tier3.test.ts:438-448`:

  ```ts
  jest.doMock('@/lib/services/api-key.service', () => {
    const actual = jest.requireActual('@/lib/services/api-key.service');
    return {
      __esModule: true,
      ...actual,
      getApiKeyForCheapLLMSelection: async () => 'test-key',
      getApiKeyForProfile: async () => 'test-key',
    };
  });
  ```

  The precedent at `:422-436` (`provider-validation` restored with
  `jest.doMock(…, () => jest.requireActual(…))` because `jest.setup.ts:220`
  mocks it GLOBALLY). `jest.setup.ts` does NOT mock `api-key.service` (grep:
  only `provider-validation` at `:220`), so the lift is simply DELETING the
  block (a `requireActual` doMock is equivalent and keeps the comment slot).
- **`getApiKeyForProfile` is a phantom:** `lib/services/api-key.service.ts`
  exports `getApiKeyForConnectionProfile` (`:23`), `getApiKeyForCheapLLMSelection`
  (`:39`), `describeProfileApiKeyFailure`, `resolveConnectionProfileApiKey`; its
  own header (`:8-11`) says the embedding service's `getApiKeyForProfile`
  "stay[s] specialized". That function is PRIVATE to `lib/embedding/
  embedding-service.ts:118` (`async function getApiKeyForProfile(`, called at
  `:150` inside `generateEmbeddingForUser`) — and the oracle cans
  `generateEmbeddingForUser` outright (`:408-419`). So the second mock entry
  mocks an export that does not exist and the embedding path never reaches
  the real one. Nothing to replace.
- **Who calls `getApiKeyForCheapLLMSelection` in the family:** `lib/memory/
  cheap-llm-tasks/core-execution.ts:309-312` `sendToProvider` (`const apiKey =
  await getApiKeyForCheapLLMSelection(selection, userId); if (apiKey === null)
  throw new Error('No API key available for cheap LLM provider')`) — the funnel
  for MEMORY_EXTRACTION (buildContext's `extractMemorySearchKeywords` distill,
  `orchestrator-tier3.test.ts:537-540`; the proactive pre-compute distill on
  `c860cf74…`, `:555-560`) and SUMMARIZATION (`summary_fold`'s fold + episode
  passes); `lib/services/dangerous-content/gatekeeper.service.ts:378-382` (the
  Concierge's cheap-LLM classification, behind the moderation-API attempt —
  `safeFallback` with the WARN `[Gatekeeper] No API key available for
  classification, failing safe` on `null`, NOT a throw); `lib/background-jobs/
  handlers/character-headshoulders-backfill.ts:107` (a job; the processor is
  no-op'd at `:678-681`, so never). Title update and memory extraction proper
  are ENQUEUED jobs — not run.
- **What each resolves to:** `orchestrator.service.ts:501-520` builds ONE
  selection per turn: `allProfiles = repos.connections.findByUserId(userId)`,
  `cheapLLMConfig = chatSettings?.cheapLLMSettings || DEFAULT_CHEAP_LLM_CONFIG`
  → `getCheapLLMProvider(connectionProfile, config, allProfiles, false)`;
  `cheap-llm.ts:216-223` priority 1: `config.defaultCheapProfileId` found in
  `availableProfiles` → `selectionFromProfile(defaultCheapProfile)` (`:124-132`:
  `connectionProfileId: profile.id`, `isLocal: provider === 'OLLAMA'`). The
  fixture's ONE `chatSettings` row (`build-orchestrator-fixture.ts:444-456`,
  shared by all 70 calls; the per-call `cheapLLMSettings: boolean` is read by
  NEITHER side — `:104` is its only mention in the oracle, and the Rust
  `:3318-3320` says "documentary") carries `cheapLLMSettings: { strategy:
  'PROVIDER_CHEAPEST', fallbackToLocal: false, defaultCheapProfileId:
  'f0000006-0000-4000-8000-000000000006' }` → **every cheap call in every case
  resolves `CheapDefault` (OPENAI `cheap-configured-model`, `apiKeyId:
  ae000004…` → `apiKeys` value `"key-cheap-default"`).** `api-key.service.ts:
  39-47`: not local, has a profile → `getApiKeyForConnectionProfile` → `profile.
  apiKeyId` → `findApiKeyByIdAndUserId` → `key_value`. Non-null. The key VALUE
  reaches no compared surface (the canned provider ignores it; `llm_logs` rows
  carry hashes of messages/tools, never the key).
- **The P4.133 arms:** `keyless_oac_sends_empty` (Ottoline on `KeylessOac`,
  `apiKeyId: null`, OPENAI_COMPATIBLE → sends `''`, runs the turn; its cheap
  selection is still `CheapDefault`), `keyless_requires_refuses` (`Unkeyed`,
  ANTHROPIC, `expectThrow`) and `dangling_key_refuses` (`DanglingKey`, `apiKeyId:
  b13300ff…` with no row, `expectThrow`) throw at the inline gate
  `orchestrator.service.ts:433-434` (`if (requiresApiKey(provider) && !rawApiKey)
  throw new Error('No API key configured for this connection profile')`) —
  BEFORE the cheap selection at `:515` and before any buildContext/pre-compute
  distill. `BoundSecondKey`/`InactiveBound` run normally. **No case's cheap
  resolver can answer `null`** → no case reaches the `No API key available for
  cheap LLM provider` throw on either side.
- The 17 profiles and their keys (`apiKeys` map id → value; `apiKeyRows` are
  P4.133's unreferenced extras): Primary `ae000001`→`key-primary`, Router
  `ae000002`, Uncensored `c0000004`→`unc-decrypted-key`, Courier (none —
  courier, by design), TextBlockModel `ae000003`, CheapDefault `ae000004`,
  PrefillOffRouter `ae000005`, PrefillOnAnthropic `ae000006`, FailoverPrimary
  `ae000007`, FailoverUnderstudy `fa110004`→`understudy-decrypted-key`,
  ChainPrimary `ae000008`, LanternVision `ae000009`, BoundSecondKey
  `b133000b`→`k-bound`, InactiveBound `b133000c` (deactivated — v4 follows
  `apiKeyId` with no `isActive` question), KeylessOac/Unkeyed (none),
  DanglingKey `b13300ff` (no row). Only CheapDefault's binding matters for the
  cheap path.
- **The v5 twin to remove:** `orchestrator_tier3_equivalence.rs:1365-1374` (the
  P4.133 comment + `let _canned_key = quilltap_core::test_support::
  CannedCheapLlmKey::install("test-key");`). The seam: `test_support.rs:356-372`
  (thread-local, `Drop`-restored); `api_key_service.rs:111-124`
  `get_api_key_for_cheap_llm_selection` returns the canned value FIRST, else
  `is_local → Some("")`, no profile → `None`, else
  `get_api_key_for_connection_profile`; `cheap_llm_exec.rs:704-730`
  `resolve_api_key` maps `None` (or a read error, `.ok().flatten()`) to
  `CompletionError::new("No API key available for cheap LLM provider")` — v4's
  throw, bytes identical. With the twin gone v5 resolves `CheapDefault` →
  `key-cheap-default` over the SAME fixture rows. **NINE other harness families
  install the same seam** (`pascal_run_custom_handler`, `appearance_sanitize_
  gate_tier3`, `courier_images_routes`, `pascal_workbench_route`,
  `context_summary_service_tier3`, `enclave_step_tier3`, `memory_processor_
  tier3`, `answer_confirmation_tier3`, `danger_gatekeeper_tier3`) — each
  twinning its OWN oracle's mock; remove ONLY the orchestrator line.
- **Prediction: all 70 (+ the new arm) stay green after the lift on BOTH sides,
  with zero bytes moving in the oracle NDJSON** — the lift changes a value
  nothing records. The proof that it bit anything is therefore the lane's
  mutation: bind `CheapDefault`'s `apiKeyId` to a missing row in a scratch
  copy of the spec and watch BOTH sides fail the distill with the same message
  (v4: the `No API key available for cheap LLM provider` throw inside
  `extractMemorySearchKeywords`'s own catch → the distill falls back; v5: the
  same arm) — the lane measures which arm the corpus shows, since the distill's
  catch may swallow it to a WARN rather than a thrown case. One `[Gatekeeper]`
  measurement too: grep the regenerated oracle NDJSON / the v5 capture for the
  classifier's lines to say whether any corpus case reaches `gatekeeper.service.
  ts:378` at all (the fixture's `concierge: { enabled: true }` names no
  pre-screen; the P4.133 note claimed the classifier's key was resolved for
  real — verify rather than carry).

---

## §D — traps and premises

1. **`jest-domock-survives-resetmodules`** (memory): the oracle calls
   `jest.resetModules()` ONCE at `:224`, registers every `doMock` after it, and
   runs all 70 calls inside ONE `test(` (`:930`) — there is no per-case reset,
   so deleting the block is the whole lift; a per-case `requireActual` is
   needed only if a future edit adds a per-case `resetModules`. Do not
   "restore per case": there is nothing to restore between cases.
2. **Both items regenerate `orchestrator_tier3` — once, from the union of the
   two edits**, at the round's pin. The round's other lanes (from the phase
   plan's NEXT paragraph, `phase-4.md:6978-6990`): bug 174 (`services/
   chat_files.rs` — the `load_and_process_files` home this survey READS at
   `:459-548` but this lane must NOT edit — plus the Z.AI/NanoGPT request
   builders); the D184/ledger-gate rulings (host boot); the five silent
   API-key reads (`dangerous_content/provider_routing.rs:141` shape — ⚠ an
   ADJACENCY, not a file collision: if that lane adds v4's `Error finding API
   key by ID and user ID` fallback line INSIDE `api_key_service.rs::
   get_api_key_for_connection_profile`, the un-mocked cheap path now runs
   through it; no corpus case trips it (every cheap read hits a present row),
   but the lane that lifts the twin should re-run the family on the union);
   `memory_pipeline_jobs_tier3` + five recipe headers (harness smalls — a
   different family and different headers). **No other lane names
   `orchestrator-tier3.test.ts`, `orchestrator_tier3_equivalence.rs`, or
   `orchestrator-tier3.json`.** bug 174's lane WILL regenerate request-builder
   families; if it also regenerates `orchestrator_tier3` (a Z.AI/NanoGPT seat
   is not in this corpus — it should not need to), the unifier runs the one
   regen on the union, as P4.D243/P4.133 did last round.
3. **`build_context.rs`'s other branch consumers** that must NOT move:
   `:2339` (recap), `:2504` (distill), `:1062` (window query) already use
   non-empty tests — leave them; the fix touches `:3229`, `:3736` (reuse) and
   `:3750` only. `messages_included` is incremented INSIDE the branch (`:3826`)
   so it follows automatically. The `is_user_turn` flag is set only inside the
   branch, so `message_context.rs:1511-1526` needs no edit — but its anchor
   fallback (preference 2/3 and the `-1` WARN) is exercised for the first time
   on a non-continue turn; a unit test in `message_context.rs` should pin the
   WARN on `Some("")` + attachments + no human row (today impossible to reach
   with a new message).
4. **The P4.129 render-aware normalizer** (`orchestrator_tier3_equivalence.rs:
   562-615`, `RENDER_MARKER = ". You are reading history, not in active
   conversation."`, `normalize_llm_log_rows`) decodes compressed TOOL rows and
   replaces the render's wall clock in the request text; the new arm's
   `pendingToolResults` TOOL row is such a row — the normalizer must keep
   seeing it (it keys on the marker, not the case name; no change expected,
   but the arm's first regen is where a blind spot would show).
5. **Whitespace is not empty at the branch** (A1): `content: "   "` with a file
   passes the schema, is persisted, and takes the first-responder branch on v4;
   a Rust fix written as `trim().is_empty()` would diverge. The C1 op (iii)
   guards it.
6. **The persisted-row claim:** v4 writes NO user row and NO file link on
   `''` — the SAME is true of v5 today (`orchestrator.rs:1797`). The order's
   Tier-2 DB diff on the new arm will show `chat_messages` identical before and
   after the fix; the reds are in the stream/`llm_logs` comparands, not the
   tables. Anyone expecting a `chat_messages` red has the wrong premise.
7. **The SPA divergence (B6) is real and outside this lane:** v4's Salon
   substitutes the sentence at `useSSEStreaming.ts:845`; v5's sends `''`. The
   two options are (a) port the substitution into `salon-conversation.ts`'s
   `runTurn` (v4-faithful: the sentence is persisted, the files linked, the
   transcript shows the human's post), or (b) rule that the server-side shape
   is the Salon's — NOT recommended, since v4 users never see an unlinked,
   unpersisted attachment post. A one-file SPA smalls item with a live beat
   (an attachment-only send persisting the sentence) — hand to the SPA smalls
   lane or the next round; `transcript-reconcile.ts:182`'s comment already
   assumes (a).
8. **`cheapLLMSettings: true/false` per call is inert on both sides** (C3); an
   order that reads it as "this case runs the cheap path" is wrong — the cheap
   path runs wherever the distill/fold fires, settings or no.
9. **Line drift:** the status-log's P4.D243 item 9 cites `orchestrator.rs:
   1836-1843`; on `6d44cfae2` the build is at `:1869-1877` (the P4.133 key
   hunks above it). Cite fresh.

---

## What the order should say

**Tier 1 (the port):**
1. `build_context.rs`: ONE `new_user_message: Option<&str>` computed as
   `input.new_user_message.as_deref().filter(|s| !s.is_empty())` once, above
   `:3229`; `:3229-3233` (tokens), `:3736-3739` (the anchor input, reuse), and
   `:3750` (the branch) all read it. `Some("   ")` stays a new message. No
   other reader moves. Carry a *why*-comment naming v4's `:2101`/`:2726`/`:2782`
   truthiness.
2. Grow `build_context.rs:5080` so it asserts the PUSHED trailing message (the
   note alone on the chained shape) and that no `is_user_turn` message exists.
3. A `message_context.rs` unit pin for the `-1` WARN on `Some("")` +
   attachments + no human row (and the preference-2 anchor with one).
4. `orchestrator-tier3.test.ts:438-448`: delete the `api-key.service` doMock
   (or `requireActual` it, with the comment saying the phantom
   `getApiKeyForProfile` was never an export); `orchestrator_tier3_equivalence.
   rs:1365-1374`: delete the P4.133 twin and its comment. Touch no other
   family's `CannedCheapLlmKey`.

**Tier 2 (the proofs):**
5. `build_context_tier3`: the three C1 ops (`""` single, `""` chained-multi
   with skip, `"   "`), regenerated at the pin; red-first on (i)/(ii) recorded
   with the exact byte difference.
6. `orchestrator_tier3`: the `empty_content_pending_tool_results` arm (a fresh
   single chat + the existing `roll_dice` chip; a `streams` entry for its
   label), regenerated ONCE at the pin on the union of items 4 + 6; red-first
   on the stream's messages AND the `CHAT_MESSAGE` `historyTailHash` cell;
   `chat_messages` unchanged before/after (D6). Optional: the image-on-vision-
   seat arm with one prior USER row (anchor preference 2).
7. After the lift: the family's full green at the pin with zero NDJSON bytes
   moved by item 4 (diff the oracle output before/after the lift — the lift is
   expected byte-neutral); the mutation (D-C3: `CheapDefault`'s `apiKeyId` →
   a missing row in a scratch spec) showing BOTH sides take the same no-key
   arm; the `[Gatekeeper]` grep recorded either way.
8. `orchestrator_tier3` re-run on the round's union if the silent-key-reads
   lane touches `get_api_key_for_connection_profile` (D2 adjacency).

**Tier 3 (named, loud):**
9. The SPA substitution divergence (B6/D7) — an SPA smalls item, not this
   lane: `salon-conversation.ts` `runTurn` + a live beat.
10. The text-file-prefix shape (`''` + a text file → first-responder with the
    inlined prefix, no persisted row) — v4-faithful today, recorded, no arm.
11. The whitespace-with-file Salon shape (`'   '` + file → a persisted
    whitespace row on both sides) — v4-faithful, recorded.
