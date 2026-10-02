# Survey — P4.145: the SPA's attachment-only send sentence + v4's client handling of the `fileProcessing` frame

**Date:** 2026-10-02 · **v4:** f6426e196 (tree dirty by the three recorded
docs paths) · **v5 main:** cb9ecf256 · **Kind:** read-only measurement —
`grep`/`sed`/`git grep` over both trees (v4 `app/salon/[id]/**`,
`components/{help-chat,brahma-console}/**`, `lib/services/chat-message/**`,
`lib/chat/file-attachment-fallback.ts`; v5 `apps/web/src/**`,
`apps/web/e2e/**`, `crates/quilltap-core/src/services/orchestrator.rs`,
`crates/quilltap-harness/tests/orchestrator_tier3_equivalence.rs`), plus the
round's recorded prose (`phase-4.md`, `status-log.md`, the P4.137 order and
survey) and five SPA memory notes. Nothing was built, run, or edited.

## The finding in one line

Item (1) stands and is slightly LARGER than recorded: v5's Salon sends
`content: ""` on an attachment-only post where v4's client sends the literal
`'Please look at the attached file(s).'` (`useSSEStreaming.ts:845`), so v5's
server persists no USER row and links no file — AND v5's optimistic bubble is
an EMPTY bubble with `attachments: []` where v4's reads `[Attached: <names>]`
and carries the attachment objects; the existing v5 unit spec that "covers"
this shape posits a server row the v5 client can never cause. Item (2)'s
recorded premise is **REFUTED**: v4's client has **no handler at all** for the
`fileProcessing` frame (`git grep fileProcessing` hits only the server encoder,
its type, and one server unit test) — v4 shows nothing, so the SPA needs NO
behavioural change when P4.140 starts emitting it; v5's reducer already
ignores unknown keys, and the lane's work there is a neutrality pin (plus an
optional mirror type).

---

## §A Item (1) — the attachment-only send

### A1 v4's send path (the oracle)

**Can a files-only send be submitted at all? Yes, at three gates:**

| gate | v4 site | predicate |
|---|---|---|
| Send button | `app/salon/[id]/components/ChatComposer.tsx:545` | `disabled={composerLocked \|\| (streaming \|\| waitingForResponse) \|\| (!hasContent && attachedFiles.length === 0 && pendingToolResults.length === 0) \|\| !hasActiveCharacters}` |
| Editor Enter | `ChatComposer.tsx:221` | `if (markdown.trim() \|\| attachedFiles.length > 0 \|\| pendingToolResults.length > 0)` |
| `sendMessage` door | `app/salon/[id]/hooks/useSSEStreaming.ts:770` | `if (!input.trim() && attachedFiles.length === 0 && pendingToolResults.length === 0) return` |

The editor placeholder with files attached is `"Add a message (optional)..."`
(three ASCII dots; `ChatComposer.tsx:523`) — and `""` with no files.

**What the client sends** (`useSSEStreaming.ts:776`, `:844-856`):

```ts
const userMessage = input.trim()
...
const requestPayload = {
  content: userMessage || (attachedFiles.length > 0 ? 'Please look at the attached file(s).' : ''),
  fileIds,
  speakingAsParticipantId: activeTypingParticipantIdRef.current ?? undefined,
  pendingToolResults: ...,
}
```

The exact bytes: `Please look at the attached file(s).` — 36 characters,
the parenthesised plural `(s)`, a terminal full stop, ASCII only. The
substitution keys on `attachedFiles.length` ONLY — a pending-tool-results-only
send still sends `''` (v4's server then persists the TOOL rows and no USER
row). It is applied to the REQUEST only; the optimistic bubble never sees it
(A3).

The rehearsal path (In Their Own Words) re-enters the same `sendMessage`
(`useImpersonationVoice.ts:278-285`) and is skipped outright for an empty text
with files (`useImpersonationVoice.ts:221`), so the substitution has ONE site.

**What the server does with it.** `sendMessageSchema`
(`lib/services/chat-message/orchestrator.service.ts:152-170`) defaults
`content` to `''` and refuses only when content, `fileIds` AND
`pendingToolResults` are all empty (`'Message must have content, attached
files, or tool results'`). The USER row is saved only under
`if (!isContinueMode && options.content)` (`:783`) — content the sentence,
`attachments: options.fileIds || []` (`:796`) — and each file linked
(`repos.files.addLink(file.id, userMessageId)`, `:804-806`), then the Librarian
upload announcement for image uploads (`:812-825`). The LLM sees
`messageContentPrefix + content` (`:896-898` — a text file's inlined body then
the sentence). So on v4 an attachment-only post persists ONE `chat_messages`
USER row whose `content` is the sentence, files linked to it, and the
transcript shows the sentence with the attachments under it.

The one v4 branch that persists a row WITHOUT the guard is the turn-fairness
pause (another user-driven seat holds the floor): `content: options.content ??
''` (`orchestrator.service.ts:1962-1983`) — so with v4's client that row also
carries the sentence.

### A2 v5 today

**Can a files-only send be submitted? Yes — the same three-way predicate:**
`apps/web/src/app/chat/chat-composer.ts:714-718` `hasContentToSend =
text().trim().length > 0 || attachedFiles().length > 0 ||
pendingToolResults().length > 0`; `:736-737` `canSend = !composerLocked() &&
hasContentToSend() && hasActiveCharacters()`; the Send button
`[disabled]="!canSend()"` (`:513`); `submit()` (`:941-992`) emits
`{ content: content.trim(), fileIds: this.attachedFiles().map((f) => f.id) }`
(`:988-991`). `ComposerSend` (`:74-78`) carries `content` + `fileIds` only —
**no filenames**.

The Salon: `send()` (`salon-conversation.ts:3493-3515`) →
`postComposedMessage()` (`:3527-3545`) → `runTurn({ content, fileIds, pending
})` → the dispatch at `:3650-3683`:

```ts
type: 'chatSend',
chatId,
content: opts.content,          // :3654 — '' on an attachment-only send
fileIds: opts.fileIds?.length ? opts.fileIds : undefined,
```

No substitution anywhere in `apps/web/src` (`grep -rn "attached file"` finds
only the doc comment at `chat/transcript-reconcile.ts:178-182` and the unit
spec at `screens/salon/salon-conversation.spec.ts:3449`/`:3466`, both
describing v4's row).

**The second `chatSend` at `:1991`** is the Whisper dialog's send
(`onWhisperSend`, `:1983-2003`, v4 `WhisperDialog.handleSend`): content +
`targetParticipantIds` + `speakingAsParticipantId`, never `fileIds` — out of
scope (v4's whisper has no attachments either).

**What v5's server does with `content: ""` + files.** The main path's guard is
v4's: `if !is_continue_mode && !input.options.content.is_empty()`
(`crates/quilltap-core/src/services/orchestrator.rs:1796`) — so NO USER row and
NO `add_link` (`:1822-1828`); the files still go to the model via the file
pipeline (and P4.137's empty-new-message branch, which the orchestrator
family now pins). The turn-fairness pause twin persists `content:
options.content` = `""` with the attachments and links them
(`orchestrator.rs:913-945`) — v4-faithful for the INPUT it gets; with the SPA
fix it will carry the sentence as v4's does. **The server needs no change** —
the gap is purely the client's missing substitution (the dispatch contract
`ChatSendRequest.content?: string`, `core/core-contract.ts:42-55`, already
carries it).

### A3 The optimistic bubble

**v4** (`useSSEStreaming.ts:799-802`, `:830-842`):

```ts
const displayContent = messageAttachments.length > 0
  ? `${userMessage}${userMessage ? '\n' : ''}[Attached: ${messageAttachments.map(f => f.filename).join(', ')}]`
  : userMessage
...
content: displayContent,
attachments: messageAttachments.length > 0 ? messageAttachments : undefined,
```

So an attachment-only post shows `[Attached: note.txt]` (no leading newline),
a text+file post shows `prose\n[Attached: a.txt, b.png]`, and the bubble
carries the `{id, filename, filepath, mimeType}` objects. The sentence never
appears in the bubble; the reconcile retires it by pass 2 (role + newly-arrived
+ 60 s clock slack, `transcript-reconcile.ts:158-219`), whose doc comment
(`:170-180`) names exactly these two non-matching shapes.

**v5** (`salon-conversation.ts:3627-3633`, `makeTempUserMessage` `:4173-4210`):
`if (opts.content || hasAttachments || pending.length > 0)` → bubble content
`opts.content ?? ''` (`''` for attachment-only), `attachments: []`. So v5
raises an **empty** USER bubble; since no row ever lands, the post-turn sweep
(`settleTranscriptAfterTurn`, pass-2 reconcile in
`chat/transcript-reconcile.ts:193-226`) finds nothing to claim and removes it —
the human's post vanishes from the transcript. v5 cannot build v4's
`[Attached: …]` today: `runTurn` sees only `fileIds`. The reconcile machinery
itself is already v4's (pass 2 present at `:215-224`), so once the sentence
row lands the bubble retires correctly whatever it shows.

### A4 Every other v4 client that sends files

| v4 client | sends files? | v5 twin | gap? |
|---|---|---|---|
| Salon `useSSEStreaming.sendMessage` | yes — the ONLY substitution site | `salon-conversation.ts` `runTurn` | **yes** |
| Salon continue (`triggerContinueMode`) | no `fileIds` | `runTurn({continueMode})` | no |
| Help chat `useHelpChatStreaming.ts:85-108` | signature takes `fileIds`, but both callers pass none (`HelpChatDialog.tsx:220`, `:240`); route requires `content: z.string().min(1, 'Message content is required')` (`app/api/v1/help-chats/[id]/messages/route.ts:22`) | `help/help-streaming.service.ts:58-82` → `help-wire.ts:278`; sole caller `help-dialog.ts:464` passes no `fileIds` | no |
| Brahma console `useBrahmaConsoleStreaming.ts:65-88` | same — callers `BrahmaConsoleDialog.tsx:157`/`:178` pass none; route `min(1)` (`brahma-console/[id]/messages/route.ts:20`) | `brahma/brahma-wire.ts:190`; caller `brahma-console-dialog.ts:362` passes none | no |
| Whisper dialog | no files | `onWhisperSend` `:1983` | no |

`useSSEStreaming` is confirmed the only Salon path, and the only v4 site of
the string (`git grep "Please look at the attached"` → `useSSEStreaming.ts:845`,
the `transcript-reconcile.ts:179` comment, and its unit test
`__tests__/unit/app/salon/hooks/transcript-reconcile.test.ts:184-194`).

### A5 How to prove it

There is no tier-1/2/3 harness family for an SPA string — v4's source is the
oracle (the bytes copied from `useSSEStreaming.ts:845`). Proof is a vitest
red-first + a live Playwright beat.

**Unit (red-first, `salon-conversation.spec.ts`).** The existing
`it('retires an attachment-only send by pass 2 …')` (`:3446-3477`) drives
`inst.send({ content: '', fileIds: ['file-1'] })` but never reads the dispatch.
Add sibling arms reading the `chatSend` request off `client.dispatch.mock.calls`
(the idiom at `:1423-1427`):
1. attachment-only → `content === 'Please look at the attached file(s).'`,
   `fileIds === ['file-1']` (RED on main: `''`);
2. text + file → `content` is the trimmed prose (green both sides — pins that
   the substitution is `||`, not always);
3. pending-tool-results only, no file → `content === ''` (pins v4's
   `attachedFiles.length` conjunct; a mutation that keys on `pending` too goes
   red);
4. a continue (`continueTurn()`/nudge) → no `content` substitution;
5. the bubble arm: the optimistic bubble does NOT carry the sentence (and, if
   Tier 2 lands, reads `[Attached: file-1.txt]` with the attachment objects).

Mutation proofs: M1 revert the substitution → arm 1 red; M2 key it on
`hasAttachments || pending.length` → arm 3 red; M3 put the sentence into
`makeTempUserMessage` → arm 5 red; M4 drop the `opts.content ||` → arm 2 red.

**Live beat (Playwright).** Closest specs:
- `e2e/salon-courier-images-flow.spec.ts:127-173` — the only beat that
  uploads through the composer (`input[type=file][aria-label="Choose a file to
  attach"]` `.setInputFiles({name, mimeType: 'text/plain', buffer})`, chip
  `.qt-chat-attachment-chip`), and it discovers a GENERAL chat via
  `POST /api/dispatch {type:'listChats'}` → `!c.project` because the fixture's
  project chats have no linked store and their upload fails (`:133-150`). It
  stops short of sending.
- `e2e/salon-attachment-ledger-flow.spec.ts` / `salon-optimistic-bubble-
  reconcile.spec.ts` — the mock-LLM send idiom (`startMockLlm(MOCK_LLM_REPLY,
  MOCK_LLM_PORT)` in `beforeAll`, `.qt-chat-composer-input .qt-rich-editor-
  content` + `Enter`, `MOCK_LLM_REPLY` visible), both in "Group Expedition".

The beat: open a general chat, upload a `text/plain` file (NOT an image — an
image upload arms P4.120's background describe and the Librarian announcement,
both noise here), press Send with an empty editor (the button, since Enter on
an empty editor is the composer's own gate), wait for `MOCK_LLM_REPLY`, then
(a) `POST /api/dispatch {type:'chatGet', chatId}` and assert a USER message
whose `content === 'Please look at the attached file(s).'` and whose
`attachments` names the uploaded file; (b) the transcript shows exactly one
`qt-message-row` with that text. RED on main: no row, and the bubble is swept.
Fixture: the shared `global-setup.ts` instance (`salon-main.db` copy); the
mock profile is `OPENAI_COMPATIBLE` (`global-setup.ts:502-505`), which inlines
a text file as the prefix — so the mock is called and answers. The lane must
measure which general chat's participants reach the mock profile (only Solo
Voyage's are explicitly re-pointed, `global-setup.ts:556-561`) and must NOT
send into Solo Voyage (its totals beat asserts a hardcoded baseline —
`e2e-playwright-traps.md` §5b; the ledger spec's ORDERING note). Mutation-
prove the beat (break the substitution, rebuild the dist, confirm red —
traps §1, "a mutation check proves nothing against a stale dist").

---

## §B Item (2) — the `fileProcessing` frame

### B1 v4 server (for the contract; P4.140 owns the port)

`encodeFallbackInfo` (`lib/services/chat-message/streaming.service.ts:563-576`):

```ts
const fallbackInfo = fallbackResults.map((result) => ({
  filename: result.processingMetadata?.originalFilename || 'Unknown',
  type: result.type,
  usedImageDescriptionLLM: result.processingMetadata?.usedImageDescriptionLLM || false,
  error: result.error,
}))
return encoder.encode(`data: ${JSON.stringify({ fileProcessing: fallbackInfo })}\n\n`)
```

Emitted once per send, from `orchestrator.service.ts:1386-1389` (`if
(fileProcessing.fallbackResults.length > 0)`), after the `debugLLMRequest`
frame (`:1372`ff) and before pre-send context validation. `fallbackResults`
gets ONE entry per loaded attachment (`context-builder.service.ts:192-215`) —
including a natively-supported file, which yields `{type: 'unsupported'}` with
no error (`file-attachment-fallback.ts:910-920`) — so in practice the frame
fires on EVERY send that carries files. The harness has already recorded v4
emitting it on a vision seat:
`{"fileProcessing":[{"filename":"lb_fit_a.webp","type":"unsupported","usedImageDescriptionLLM":false}]}`
(`crates/quilltap-harness/tests/orchestrator_tier3_equivalence.rs:514-530`,
`EXPECTED_EVENT_DIVERGENCES`).

`type` values — `FallbackResult.type` (`lib/chat/file-attachment-fallback.ts:267`):
`'text' | 'image_description' | 'unsupported'` (the `'IMAGE_DESCRIPTION'` at
`:381` is an llm-logs `type`, not this enum). `usedImageDescriptionLLM` is
`true` only on a fresh describe (`:632-640`); a reused persisted description
is `image_description` with `false` (`:705-712`). The declared stream type
`StreamChunkData.fileProcessing` (`lib/services/chat-message/types.ts:471-476`)
is `Array<{ filename: string; type: string; usedImageDescriptionLLM: boolean;
error?: string }>`.

### B2 v4 client — NOTHING

`git grep -n fileProcessing` over the whole v4 tree (excluding the orchestrator's
local variable) returns exactly: `streaming.service.ts:575`, `types.ts:471`,
`__tests__/unit/lib/services/chat-message/streaming.service.test.ts:134`, and
two bug write-ups that use `fileProcessing.` as the orchestrator's local. The
Salon's `readSSEStream` (`useSSEStreaming.ts:550-735`) is a chain of
independent `if (data.<key>)` arms — `status`, `content`, `error`,
`toolsDetected`, `toolResult`, `confirmationResult`, `done`,
`pendingExternalTurn`, `turnStart`, `turnComplete`, `chainComplete`
(`:600-720`) — with no `fileProcessing` arm and no catch-all; the client's
`SSEEvent` type does not even declare the key. **No toast, no transcript note,
no state flag, no user-facing string exists.** The frame is server-side
telemetry the v4 UI drops on the floor.

### B3 v5 client today — already ignores it

- Parse: `core/core-transport.ts:214-224` `parseEventData` is a bare
  `JSON.parse(trimmed) as ScopedEvent` — no schema, unknown keys survive into
  the object untouched.
- Fan-out: `core/core-client.ts:96` pushes every frame to `events$`;
  `core/realtime.service.ts:108-116` `acceptFrame` drops anything that is not a
  realtime hint.
- Fold: `core/chat-stream.reducer.ts:211-334` `reduceChatFrame` is the same
  `if (frame.<key>)` chain as v4's; with no matching arm it returns `s`, which
  is `prev` **by reference** (`:334`). A frame of `{chatId, fileProcessing:
  [...]}` changes nothing.
- The one hazard: each entry may carry a NESTED `error` string. v5's error arm
  reads only the TOP-LEVEL `frame.error` (`:236-239`), so a nested item error
  does not halt the stream — correct, but unpinned. P4.140 must keep `error`
  nested (never flatten it beside `chatId`), or the SPA would treat a failed
  text decode as a fatal stream error.
- Other `events$` consumers (`help-streaming.service.ts`, `brahma-wire.ts`,
  `regeneration.state.ts`, the green-room/scenario-builder/generator states)
  either filter on a different scope id or only ever see sends without files;
  none reads unknown keys.

**So the SPA MUST NOT change behaviour when P4.140 lands the frame**, and v4
gives it no UI effect to port.

### B4 Contract

`core/core-contract.ts` is "hand-written TS … mirroring the Rust source of
truth (`api/types.rs` + `services/chat_events.rs`)" (`:1-14`); `ChatStreamFrame`
(`:4623-4704`) flattens into `ScopedEvent` (`:4756-4761`). The FROZEN banners
(`:207`, `:605`) cover the Post Office and the chat-dialog §1 surfaces, not
`ChatStreamFrame`, so a mirror field is an ordinary addition — but it must
mirror whatever variant P4.140 adds to `chat_events.rs`. Not required: v4's own
client type omits the key. Recommended only so the neutrality spec can be
written without a cast:

```ts
/** v4 `encodeFallbackInfo` (`streaming.service.ts:563-576`) — one entry per
 *  attached file. v4's client reads nothing from it; neither does v5's. */
export interface FileProcessingEntry {
  filename: string;
  type: 'text' | 'image_description' | 'unsupported';
  usedImageDescriptionLLM: boolean;
  error?: string;
}
// in ChatStreamFrame:
  fileProcessing?: FileProcessingEntry[];
```

### B5 Proof

- Unit (`core/chat-stream.reducer.spec.ts`, 306 lines; frames are plain
  `ScopedEvent` literals): `reduceChatFrame(prev, {fileProcessing: [{filename:
  'a.txt', type: 'unsupported', usedImageDescriptionLLM: false, error: 'File
  type … no fallback is available'}]})` `toBe(prev)` (reference equality), and
  the same frame mid-stream leaves `content`/`streaming`/`error` untouched.
  Mutation M5: add an arm that copies the nested error to `s.error` → red.
- e2e: optional. The ledger spec's `EventSource.prototype.onmessage` accessor
  wrap (`salon-attachment-ledger-flow.spec.ts:41-94`; memory
  `e2e-inject-wire-bytes-via-eventsource.md`) can splice a `fileProcessing`
  frame carrying an item `error` into the §A beat and assert the turn still
  completes with no error toast — but with no v4 UI effect there is nothing
  positive to see, so the unit pin is the proof and the beat adds little. Once
  P4.140 lands, the §A beat itself receives the REAL frame (a text file on the
  OPENAI_COMPATIBLE mock → `{type:'text'}`) and its green run is the live
  neutrality proof for free.

---

## What the recorded description got wrong

1. **"SPA shows no fallback notice where v4 would"** (`status-log.md:159709`,
   P4.137's 💸 list) — FALSE. v4 shows no notice either; no v4 client reads
   `fileProcessing` (B2). The "whatever client handling v4 has" half of this
   order is: none.
2. **The P4.137 deferral's "persists NO user row and links NO file"**
   (`phase-4.md:7283-7286`, P4.137 order Tier 3 item 11) — true for the main
   path; the turn-fairness pause branch persists an EMPTY-content row with the
   files linked on both sides (`orchestrator.rs:913-945`; v4 `:1962-1983`),
   which with the fix will carry the sentence. And it UNDERSTATES the
   divergence: v5's optimistic bubble is empty with `attachments: []`, then
   swept (A3), where v4's reads `[Attached: …]` — neither record mentions the
   bubble.
3. **The v5 unit spec `retires an attachment-only send by pass 2`**
   (`salon-conversation.spec.ts:3446-3477`) and **the reconcile doc comment**
   (`chat/transcript-reconcile.ts:178-182`) present v4's shapes as v5's: the
   spec's comment says "the composer's bubble shows what the operator typed
   (nothing), while the server stores 'Please look at the attached file(s).'"
   — v4's bubble shows `[Attached: …]`, not nothing, and v5's server never
   receives the sentence. The spec stays green only because it hand-plants the
   server row. (The reconcile logic itself is correct.)
4. **The brief's v4 line for the frame shape** (`types.ts:471-480`) and the
   producer path (`lib/chat-files-v2.ts`) — the type is `types.ts:471-476`
   and the producer is `lib/chat/file-attachment-fallback.ts`
   (`processFileAttachmentFallback`, `:903-950`) via
   `context-builder.service.ts:192-215`; `chat-files-v2.ts` is not involved.
   The frame also carries a fourth key, `error`, which the order's text omits.

Re-verified as stated: the string bytes and site (`useSSEStreaming.ts:845`);
the v5 site (`salon-conversation.ts:3654`); `useSSEStreaming` is the only
Salon sender; the help twin (`help-streaming.service.ts:58-82`) has no gap.

## Proposed tiered deliverables

### Tier 1 — must land

1. **The substitution** in `salon-conversation.ts` `runTurn`'s dispatch
   (`:3654`): `content: opts.content || (opts.fileIds?.length ? 'Please look
   at the attached file(s).' : '')` — semantically v4's `userMessage ||
   (attachedFiles.length > 0 ? … : '')`. Request only; the bubble is untouched;
   continue/nudge unaffected (they carry no files). Keep the v4 cite and the
   *why* in a comment. Note the `speakingAsParticipantId` gate (`:3675-3678`)
   already treats attachment-only as a user send.
2. **Unit arms** 1–4 of §A5 in `salon-conversation.spec.ts`, RED-first on
   arm 1; mutation M1/M2/M4.
3. **Correct the two stale descriptions** (`salon-conversation.spec.ts:3447-3450`
   comment; `transcript-reconcile.ts:178-182` stays — it describes v4 and,
   after this lane, v5 too — but say so).
4. **The live beat** (§A5): attachment-only send → `chatGet` shows the USER row
   with the sentence and the file in `attachments`; the transcript shows it
   once. Mutation-proven against a rebuilt dist.
5. **The `fileProcessing` neutrality pin** in `chat-stream.reducer.spec.ts`
   (B5), mutation M5.

### Tier 2 — should land

6. **v4's optimistic bubble** (A3): widen `ComposerSend` (`chat-composer.ts:74-78`)
   with the attachment objects (`UploadedChatFile` → `MessageAttachment`
   `{id, filename, filepath, mimeType}`), thread them through `send` /
   `postComposedMessage` / `runTurn` (and the impersonation stash
   `PendingSend`, `chat/impersonation-voice/impersonation-voice.state.ts`, which
   today stashes `fileIds` only), and build `displayContent` +
   `attachments` exactly as `useSSEStreaming.ts:799-802`/`:836`. Unit arm 5;
   the beat asserts the bubble text `[Attached: <name>]` before the reply.
   Pass-1/pass-2 reconcile behaviour is unchanged (pass 2 already retires it).
7. **The `FileProcessingEntry` mirror type** (B4) — only if P4.140's Rust
   variant name is fixed at order time; otherwise defer to unification.

### Tier 3 — loud deferrals

8. **The composer placeholder bytes**: v5 `chat-composer.ts:749-752` returns
   `'Add a character to start chatting…'` / `'Add a message (optional)…'`
   (U+2026) / `'Type a message…'`; v4 `ChatComposer.tsx:523` uses three ASCII
   dots and an EMPTY placeholder when nothing is attached. Unrecorded; "Type a
   message…" is referenced by dogfood finding #75's prose. A one-line
   recorded-divergence for the human to rule on, not this lane's to flip.
9. **No UI for `fileProcessing`** — v4 has none; v5 adds none (recorded so a
   future reader does not "port" a notice).

## Files the lane would edit

- `apps/web/src/app/screens/salon/salon-conversation.ts` (Tier 1 item 1; Tier 2
  item 6 `runTurn`/`postComposedMessage`/`send`/`makeTempUserMessage`)
- `apps/web/src/app/screens/salon/salon-conversation.spec.ts` (items 2, 3, 6)
- `apps/web/src/app/core/chat-stream.reducer.spec.ts` (item 5)
- `apps/web/e2e/salon-attachment-only-send-flow.spec.ts` (NEW, item 4; name
  must sort after `aa-foundation` and before the `zz…` destructives)
- Tier 2 only: `apps/web/src/app/chat/chat-composer.ts` (`ComposerSend` +
  emit), `apps/web/src/app/chat/chat-composer.spec.ts` (if it asserts the emit
  shape), `apps/web/src/app/chat/impersonation-voice/impersonation-voice.state.ts`
  (+ its `.spec.ts`), `apps/web/src/app/screens/salon/salon-impersonation-voice.spec.ts`
  (if the stash shape is asserted), `apps/web/src/app/core/core-contract.ts`
  (item 7)
- Possibly `apps/web/src/app/chat/transcript-reconcile.ts` (comment only)
- `apps/web/package.json` + `apps/web/package-lock.json` (version, both
  `"version"` lines `:3`/`:9`)
- `docs/CHANGELOG.md`, `docs/developer/porting/status-log.md` (lane record),
  `docs/developer/porting/phase-4.md` (the deferred-by-name item closes)

No cargo, no harness family, no oracle case, no committed fixture.

## Files the lane must READ but not edit

- v4: `app/salon/[id]/hooks/useSSEStreaming.ts:550-735`, `:750-860`;
  `app/salon/[id]/components/ChatComposer.tsx:186-230`, `:515-545`;
  `app/salon/[id]/hooks/transcript-reconcile.ts:150-220`;
  `app/salon/[id]/hooks/useImpersonationVoice.ts:205-290`;
  `lib/services/chat-message/orchestrator.service.ts:148-170`, `:686-830`,
  `:1380-1390`, `:1955-1985`; `lib/services/chat-message/streaming.service.ts:560-576`;
  `lib/services/chat-message/types.ts:440-480`; `lib/chat/file-attachment-fallback.ts:263-290`, `:900-950`.
- v5: `crates/quilltap-core/src/services/orchestrator.rs:913-945`, `:1793-1830`
  (the server needs no change); `crates/quilltap-core/src/services/chat_events.rs`
  (P4.140's variant, for item 7); `apps/web/src/app/core/core-transport.ts:207-224`;
  `apps/web/src/app/core/realtime.service.ts:100-116`;
  `apps/web/src/app/chat/transcript-reconcile.ts`; `apps/web/e2e/global-setup.ts`;
  `apps/web/e2e/support/{env,mock-llm,fixtures}.ts`;
  `apps/web/e2e/salon-courier-images-flow.spec.ts:127-173`;
  `apps/web/e2e/salon-attachment-ledger-flow.spec.ts`;
  `apps/web/e2e/salon-optimistic-bubble-reconcile.spec.ts`.
- Memory notes the order should cite: `ng-test-defaults-to-watch-mode.md` (use
  `npm test` / `npm run build`, never bare `ng`), `spa-tsc-does-not-typecheck-
  app-sources.md` (only `npm run build` type-checks), `ng-test-filter-matches-
  test-names.md`, `e2e-port-4319-is-repo-wide.md` (one Playwright run at a time),
  `e2e-playwright-traps.md` §1 (stale dist), §3 (isolate by file), §5b (a beat
  that SENDS moves other files' numbers), §8 (`webBinary()` prefers RELEASE),
  §9 (an SPA-only lane builds DEBUG `quilltap-web` in its worktree),
  `e2e-inject-wire-bytes-via-eventsource.md` (if the optional injected beat is
  written), `e2e-sends-into-a-shared-chat-trip-a-title-checkpoint.md`,
  `e2e-assertion-can-read-the-pre-click-state.md`, `spa-worktree-node-modules.md`
  (`npm ci`, never cp). `forcing-a-deterministic-turn-in-a-salon-e2e-beat.md`
  is NOT needed (a single-LLM general chat; any responder answers the mock).

**SPA gates** (`apps/web/package.json:5-17`): `npm test` (= qt-class
self-test + census + `node tools/ng-run.mjs test --watch=false`), `npm run
build` (`tools/ng-run.mjs build` — the real type gate), `npm run lint`
(qt-class checks), `npm run e2e` (`playwright test`; needs a built
`quilltap-web` + `quilltap` binary in the worktree's `target/`).

## Cross-lane adjacencies / risks

- **P4.140 (the Salon spine) owns the `fileProcessing` frame's Rust emission**
  and the `EXPECTED_EVENT_DIVERGENCES` retirement in
  `orchestrator_tier3_equivalence.rs:514-530`. The shared contract below must
  appear byte-identical in both orders. Constraints from this side: keep
  `error` NESTED per entry (B3); omit `error` when absent (v4's
  `JSON.stringify` drops `undefined`); key order `filename, type,
  usedImageDescriptionLLM, error`; the frame flattens beside `chatId` like
  every other chat frame. If P4.140 names a `ChatEvent` variant, P4.145's
  optional mirror (item 7) follows it — a unifier handoff if the name is not
  fixed at order time.
- **P4.140's Option V** threads a `TimeZone` through the Salon spine; no SPA
  overlap. Its orchestrator family regen will now see the `fileProcessing`
  frame on the `empty_content_image_on_vision_seat` arm — not this lane's.
- **The live beat runs on the round's union** with P4.140's frame present: the
  §A beat then receives a real `{"fileProcessing":[{"filename":"<name>.txt",
  "type":"text",…}]}` — its staying green IS the live neutrality proof; if it
  goes red at unification, the frame was flattened or mis-keyed.
- **P4.139** (API-key reads) / **P4.141** (model layer) / **P4.142**
  (fallbacks) / **P4.143** (data/zod) / **P4.144** (memory + harness): no
  `apps/web/**` overlap. A P4.142 change to `chatGet` projection under a
  renamed column does not affect the beat's healthy-fixture read.
- **Playwright port 4319** is repo-wide: the unifier's full suite and this
  lane's beat must not overlap with any other lane's e2e run.

### Shared contract candidate (verbatim for P4.140's order)

> **The `fileProcessing` stream frame** (v4 `encodeFallbackInfo`,
> `lib/services/chat-message/streaming.service.ts:563-576`; emitted once from
> `orchestrator.service.ts:1386-1389` when `fallbackResults.length > 0`, after
> the `debugLLMRequest` frame and before pre-send context validation). Wire
> bytes, as one SSE `data:` payload:
>
> `{"fileProcessing":[{"filename":<string>,"type":<"text"|"image_description"|"unsupported">,"usedImageDescriptionLLM":<boolean>,"error":<string, ABSENT when undefined>}, …]}`
>
> - One entry per attached file, in the order `loadAndProcessFiles` processed
>   them (`context-builder.service.ts:192-215`).
> - `filename` = `processingMetadata.originalFilename`, else `'Unknown'`.
> - `type` = `FallbackResult.type` (`lib/chat/file-attachment-fallback.ts:267`):
>   `'text'` (text inlined), `'image_description'` (described — fresh or
>   reused), `'unsupported'` (natively supported with NO `error`, or a failed
>   fallback WITH `error`).
> - `usedImageDescriptionLLM` = `processingMetadata.usedImageDescriptionLLM ||
>   false` — `true` only on a fresh describe call.
> - `error` = `FallbackResult.error`, omitted when undefined; NESTED in the
>   entry, never a top-level frame key.
> - In v5 the frame flattens into the scope envelope like every chat frame:
>   `{"chatId":"…","fileProcessing":[…]}`.
> - **Client handling: none.** v4's Salon (`useSSEStreaming.ts` `readSSEStream`)
>   has no arm for it; v5's `reduceChatFrame` returns the previous state
>   unchanged. No UI string exists.
> - Recorded v4 instance (`orchestrator_tier3_equivalence.rs:529`):
>   `{"fileProcessing":[{"filename":"lb_fit_a.webp","type":"unsupported","usedImageDescriptionLLM":false}]}`.

## Versions / bumps

- SPA `apps/web/package.json` + `package-lock.json`: `0.5.792` → `0.5.793`
  (one bump per lane commit series, per the repo's habit of a version bump
  per SPA commit — the lane decides the count).
- No crate bumps (no Rust edits).
