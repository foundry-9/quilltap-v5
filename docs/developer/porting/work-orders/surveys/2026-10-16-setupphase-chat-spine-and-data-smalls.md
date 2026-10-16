# Survey — the chat-spine smalls and the data / CLI / memory-time smalls across `83334b9a5` → `25b8fab6a`

Read-only planning survey, **2026-10-16**, for the first catch-up round past the
`83334b9a5` baseline. v4 = `~/source/quilltap-server`, read ONLY through
`git show 25b8fab6a:<path>` (the **PIN**, v4 main HEAD
`25b8fab6aa58636c6885ba63684c7f8fbfca9c3b`, `4.11.0-dev.10`, clean),
`git show <sha> -- <paths>` and `git diff 83334b9a5 25b8fab6a -- <paths>`.
**Every v4 line number is AT THE PIN** unless marked `(commit)`. v5 = `main` at
**`56554d98c`**. Nothing here is taken from commit prose (ledger §5.3): every
port fact comes from a shipped hunk. The drift ledger's §3 rows for
`01f508d4e` (`drift-ledger.md:299`), `49f8906c3` (`:302`), `75d094e63` (`:303`),
`b82037d75` (`:308`), `7458ff94e` (`:314`), `6069a7acd` (`:318`) and `25b8fab6a`
(`:323`) were read first and are built on, not restated;
where a hunk disagrees with a row it is said in **§9** (and flagged `⚠` inline).

Two parts, one per lane:

- **PART A — LANE S1, the chat spine:** (1) `6069a7acd` bug 249 — the lone
  character no longer chains to the depth cap; (2) `b82037d75` bug 248 — the
  conversation render queues embeddings from the pre-write read; (3)
  `01f508d4e` — the pruned / renamed debug lines.
- **PART B — LANE S2, data / CLI / memory-time:** (4) `49f8906c3` — the
  new-account restore remaps seat ids inside JSON columns; (5) `7458ff94e` —
  `describeOutfit` marks a styled hair slot; (6) `75d094e63` — the CLI help /
  completion bytes (+ the migrations fix, NO-PORT); (7) `25b8fab6a` bug 247 —
  the month-word prototype keys (a CONVERGENCE).

Nothing in this slice moves a schema, a migration, a cipher, a route, a wire
type, or the SPA. **No new D23 re-dump; no contract candidate (C1 style)** —
§A7 / §B7 say so and list the small rulings the lanes will want.

Survey-local unit labels (the order author renames them): `S1-U1` (bug 249),
`S1-U2` (bug 248), `S1-U3` (log lines); `S2-U1` (seat ids), `S2-U2` (hair note),
`S2-U3` (CLI bytes), `S2-U4` (bug 247 retire).

---

# PART A — LANE S1: the chat spine

## A0. The span in one paragraph

Three commits, none touching a schema. `01f508d4e` (2026-10-13, release
checklist 6) pruned two DEBUG traces and gave two mailbox DEBUGs descriptive
messages; it is the OLDEST of the three and the only one that changes lines v5
ported **at the `83334b9a5` round** (`[Sync] Reading disk file` landed with
P4.D274, `markMailAlertsDelivered` with P4.D291 — both at their PIN texts).
`b82037d75` (2026-10-14, bug 248) moves the conversation-render job's embedding
decision from a post-upsert re-read to a pre-upsert read — a fix for Node's
job-child BUFFERED writes, a mechanism v5's single writer never had, so the
behavioural delta is statement ORDER, one gate, and two DEBUG lines.
`6069a7acd` (2026-10-15, bug 249) adds ONE early return to `shouldChainNext`
(a persona-less one-character chat stops chaining after the character spoke) —
the only commit of the three with an OBSERVABLE decision change, and the one
v5 genuinely shares as a defect. Bugs 248 and 249 are both "Original to v4,
v5 status: Not assessed" — NOT convergences (this port filed neither).

---

## A1. v4 hunks, file by file

### A1.1 `6069a7acd` — `lib/services/chat-message/turn-orchestrator.service.ts`

`shouldChainNext(repos, chatId, userParticipantId, chainDepth, chainStartTime,
config = DEFAULT_CHAIN_CONFIG, guards: ChainGuards = {})` is `:63`; the
guards interface (`neverPauseForUser?: boolean`) is `:53-61`. The decision
sequence at the pin (line numbers):

| step | pin lines | what |
|---|---|---|
| re-read chat | 72-77 | not found → WARN `[TurnOrchestrator] Chat not found during chain` `{ chatId }` → `{chain:false, reason:'error'}` |
| paused | 80-83 | INFO `Chat paused, stopping chain` `{ chatId, chainDepth }` |
| depth / time | 86-96 | INFO `Max chain depth reached` / `Max chain time reached` |
| turn state | 99-110 | `calculateTurnStateFromHistory({ messages: messageEvents, participants, userParticipantId, spokenThisCycleParticipantIds, cycleOrderParticipantIds })` |
| presence | 112-118 | `isAllLLM`; `hasUserPresence = userParticipantId !== null \|\| messageEvents.some(m => m.role === 'USER')`; `effectiveAllLLM = isAllLLM && !hasUserPresence` |
| all-LLM pause | 119-137 | only under `effectiveAllLLM`: counts ASSISTANT rows since the last USER (skipping `systemSender`); `shouldPauseForAllLLM(turnCount) && turnCount > 0 && !guards.neverPauseForUser` → INFO `All-LLM pause threshold reached` + `repos.chats.update(chatId, { isPaused: true })` |
| queue | 139-166 | pop; entries equal to `turnState.lastSpeakerId` are SKIPPED; `turnQueue` written back even when only skipped |
| room map | 168-171 | `loadRoomCharacters(repos, participants)` |
| **selector arm** | **173-211** | `if (!nextParticipantId && selectionReason !== 'queue')`: `resolveCycleOrder` → `selectNextSpeaker(...)` → `nextParticipantId = result.nextSpeakerId` (`:187`), `selectionReason = result.reason` (`:188`), **then the new block `:190-210`** |
| no next speaker | 213-219 | `cycle_complete` / `user_turn` / `no_next_speaker` mapping |
| user-driven seat | 221-243 | `isUserDrivenSeat(...)` → `{ chain:false, reason:'user_turn', participantId, characterName }` |
| continue | 245-262 | INFO `Chain decision: continue` `{ chatId, chainDepth, nextParticipantId, characterName, selectionReason }` |

**The new block (commit hunk `@@ -186,6 +186,28`, pin `:190-210`), verbatim
logic:**

```ts
// A lone character's monologue is the selector's answer ... (bug 249). ... (comment, :190-196)
if (
  selectionReason === 'only_character' &&
  nextParticipantId !== null &&
  nextParticipantId === turnState.lastSpeakerId &&
  !effectiveAllLLM &&
  !guards.neverPauseForUser
) {
  logger.info('[TurnOrchestrator] Lone character has answered; floor returns to the operator', {
    chatId,
    chainDepth,
    participantId: nextParticipantId,
  })
  return { chain: false, reason: 'user_turn' }
}
```

- **Level / message / fields:** INFO; message
  `[TurnOrchestrator] Lone character has answered; floor returns to the operator`;
  context key order `chatId`, `chainDepth`, `participantId` (the third is the
  SELECTED seat id, `participantId`, not `nextParticipantId`). Logger is the
  same file logger as the sibling `[TurnOrchestrator]` lines (`:204`).
- **Return value:** `{ chain: false, reason: 'user_turn' }` — **no
  `participantId`, no `characterName`, no `selectionReason`**.
- **It lives INSIDE the selector arm.** A queue pick never reaches it; the
  selector is UNCHANGED (`lib/chat/turn-manager/selection.ts:80`
  `buildResult(onlyCharacter, 'only_character', ...)`, the "one active
  CHARACTER participant" case — user-controlled CHARACTER seats count).
- **Ordering consequence:** the check sits BEFORE the "No next speaker" and
  "user-driven seat" arms, so a lone **user-driven** seat that spoke last now
  answers `user_turn` WITHOUT `participantId` / `characterName` where it used
  to answer them (`:221-243`) — see A6.4.
- `resolveCycleOrder` returns `[]` for a single candidate with no write
  (`cycle-order.ts:187`), so the lone-character path never writes the chat row
  except via the queue block.

**Test (`turn-orchestrator.test.ts` +69, commit `@@ -227,6 +227,75`) — a new
`describe('a lone character (bug 249)')`; five `it`s, all mocking
`selectNextSpeaker` → `{ nextSpeakerId: 'llm-1', reason: 'only_character' }`
and `calculateTurnStateFromHistory` once via `lastSpokeWas(id)`:**

1. *hands the floor back to the operator once the only character has
   answered* — one LLM participant, `isAllLLMChat` mocked `true`, last speaker
   `llm-1`, `userParticipantId = '__user__'` (the persona-less operator "is
   present, with no seat") → `toEqual({ chain: false, reason: 'user_turn' })`.
2. *lets the only character speak when they did not speak last* — last speaker
   `null` → `chain: true`, `participantId: 'llm-1'`.
3. *keeps a genuinely all-LLM monologue going (no human present)* —
   `isAllLLMChat` true, last speaker `llm-1`, `userParticipantId = null` →
   `chain: true`.
4. *keeps an autonomous room monologue going even with an operator
   interjection on file* — last speaker `llm-1`, user `'__user__'`,
   `{ neverPauseForUser: true }` → `chain: true`.
5. *still honours a queued (nudged) turn for the same character* —
   `turnQueue: ['llm-1']`, last speaker `'someone-else'` →
   `chain: true`, `selectionReason: 'queue'`.

(The v4 tests mock the turn manager; the port's equivalent is a REAL-DB
differential — A4.)

### A1.2 `b82037d75` — `lib/background-jobs/handlers/conversation-render.ts`

Pin structure (`handleConversationRender`, `:19-131`; logger
`createServiceLogger('ConversationRenderHandler')` `:17`):

- `:24-32` load chat → WARN `[ConversationRender] Chat not found, skipping`
  `{ jobId, chatId }`; `:34-44` render → DEBUG `[ConversationRender] Chat has
  no events, nothing to render` `{ jobId, chatId }` (both unchanged).
- **`:46-77` step 3 (changed).** For EACH `interchange` (in `result.interchanges`
  order):
  ```ts
  const existing = await repos.conversationChunks.findByInterchangeIndex(payload.chatId, interchange.index)   // :56-59  PRE-write read
  const upserted = await repos.conversationChunks.upsert({ chatId, interchangeIndex, content, participantNames, messageIds }) // :60-66
  const needsEmbedding = payload.fullReembed || !existing || !existing.embedding || existing.content !== interchange.content  // :68-72
  const chunkId = existing?.id ?? upserted?.id                                                  // :73
  if (needsEmbedding && chunkId) toEmbed.push(chunkId)                                           // :74-76
  ```
  Interleave per chunk is exactly **read, write, read, write** (pinned by the
  test). The pre-fix `const now = new Date().toISOString()` is gone.
- **`:79-85` NEW unconditional DEBUG**, after the loop, before step 4:
  `[ConversationRender] Chunks upserted` with context **`{ jobId: job.id,
  chatId, interchangeCount: result.interchanges.length, needingEmbedding:
  toEmbed.length, fullReembed: !!payload.fullReembed }`** (key order as
  written; `fullReembed` is a real boolean).
- **`:87-121` step 4 (changed).** Gate is now **`if (toEmbed.length > 0)`**
  (was `result.interchanges.length > 0`). Inside the existing `try`: read
  `repos.embeddingProfiles.findAll()` (`:94`) → `find(p => p.isDefault)`
  (`:95`); if a default exists: `for (const chunkId of toEmbed)
  await enqueueEmbeddingGenerate(job.userId, { entityType:
  'CONVERSATION_CHUNK', entityId: chunkId, chatId: payload.chatId, profileId:
  defaultProfile.id })` (`:97-105`); **else** (previously an EMPTY `else {}`)
  **NEW DEBUG** `[ConversationRender] No default embedding profile; chunks left
  for the startup reconcile` with **`{ jobId: job.id, chatId, pending:
  toEmbed.length }`** (`:106-112`). The `catch` WARN (`:113-120`) is unchanged
  and now covers only the profile read + enqueues — the chunk re-read loop and
  `embeddedCount` are DELETED.
- `:123-130` the success INFO (`Conversation rendered successfully`,
  `{ jobId, chatId, interchangeCount, markdownLength, durationMs }`) unchanged.

**`lib/database/repositories/conversation-chunks.repository.ts`** (`import {
logger } from '@/lib/logger'` added `:20` — the BARE root logger, no module
context). `upsert(input)` is `:163`; the update arm is `:164-205` (unchanged:
`existing.content !== input.content` NULLs the vector when no embedding is
supplied — bug 17). The insert arm, **`:207-216`**:

```ts
const { id: suppliedId, ...data } = input as ConversationChunkInput & { id?: unknown };   // :207
logger.debug('Creating conversation chunk', {                                               // :208-212
  chatId: input.chatId, interchangeIndex: input.interchangeIndex,
  suppliedId: typeof suppliedId === 'string',
});
return this._create(data as Omit<...>, typeof suppliedId === 'string' ? { id: suppliedId } : undefined);  // :213-216
```

`_create` (`base.repository.ts:350-379`) uses `options?.id || this.generateId()`
and logs INFO `Entity created {collection, id}` — an existing unported
repository-layer line (see A6.6). `findByInterchangeIndex` (`:138`) is
`safeQuery(..., 'Error finding chunk by interchange index', {chatId,
interchangeIndex}, null)` — **a FALLBACK read**; its inner `findOneByFilter`
(`base.repository.ts:300-314`) is itself a fallback `safeQuery` with message
`Error finding entity by filter`, so on a read failure the ONLY line is the
inner `ERROR Error finding entity by filter { collection: 'conversation_chunks',
error }` and the result is `null` (⇒ `existing` null ⇒ queued).

**Tests.** `conversation-render.test.ts` (+81 / −… ): (a) the two existing
mock rows gain `content` (`{ id: 'chunk-0', content: 'Chunk 0', embedding:
null }`, `{ id: 'chunk-1', content: 'Chunk 1', embedding: [0.2] }`; and the
`fullReembed` pair) because the predicate now compares content; (b) a new
`describe('decides re-embedding from the chunk as it was before the write
(bug 248)')` with a default profile, two interchanges (`'Chunk 0'`, `'Chunk 1,
now longer'`): *queues a chunk whose content changed even though the read still
shows its old vector* (chunk-1 queued under the EXISTING id, never the upsert's
`synthetic-id`; 1 enqueue), *queues a brand-new chunk under the id the upsert
handed back* (`new-1`; the child cannot read its own create), *reads each chunk
before upserting it* (`['read','write','read','write']`).
`conversation-chunks-upsert.test.ts` (+48, commit `@@ -102`): *keeps a
caller-supplied id on insert, and keeps it out of the row data*
(`_create` called with `options = { id: 'child-assigned' }`, `'id' in data`
false) and *lets the base repository mint an id when none is supplied*
(`options` `undefined`).

### A1.3 `01f508d4e` — the pruned / renamed lines

| file | pin line | change |
|---|---|---|
| `lib/chat/transcript-projection.ts` | `:65-71` (`projectChatTranscript` signature `:65`; the removed line sat directly above the `// Get roleplay template` comment at `:71`) | REMOVED DEBUG `[Transcript] Projecting chat transcript` `{ chatId }` (the entry line). The EXIT line `:315` DEBUG `[Transcript] Projected chat transcript` `{ chatId, messages: messages.length, offSceneCharacters: offSceneCharacters.length }` **stays**. |
| `lib/mount-index/sync/apply-disk.ts` | `:55-59` (`readDiskFile`) | REMOVED DEBUG `[Sync] Reading disk file` `{ relativePath }` — the line `197104649` had ADDED six commits earlier. `:197` `[Sync] Disk applier ignoring action` `{ kind }` stays. |
| `lib/post-office/mailbox.ts` | `:324` (`markMailAlertsDelivered`) | DEBUG message `markMailAlertsDelivered` → **`Mail alerts marked delivered`**; fields unchanged and in order `{ chatId, vaultId, via, requested: owed.paths.length, marked }`. |
| `lib/post-office/mailbox.ts` | `:341` (`discardLetter`) | DEBUG message `discardLetter` → **`Letter discarded`**; fields unchanged `{ vaultId, path, deleted }`. |
| `components/wardrobe/hooks/useWardrobeItemActions.ts` | — | REMOVED client `console.debug('[useWardrobeItemActions] Duplicating wardrobe item', …)` — SPA-side, no v5 analogue (NO-PORT). |

**⚠ The per-letter WARN in the same function is UNCHANGED** and still carries
the function name: WARN `markMailAlertsDelivered: could not mark letter
alerted` `{ chatId, vaultId, path, error }` (`:316-321`). A blind
find-and-replace of `markMailAlertsDelivered` would corrupt it.

No tests changed in this commit.

---

## A2. v5 today — per surface (`main` at `56554d98c`)

### A2.1 bug 249 surfaces

- **`crates/quilltap-core/src/services/turn_orchestrator.rs`** (1,186 lines):
  `should_chain_next` `:324-?` (the signature `:324-334`: `db, chat_id,
  user_participant_id: Option<&str>, chain_depth, now_ms, chain_start_time_ms,
  config, guards: ChainGuards, draws: &DrawSource`). `ChainGuards`
  `{ never_pause_for_user }` `:97-103`; `ChainReason::UserTurn` `:107-122`;
  `ChainDecision::stop(reason)` is the PRIVATE constructor at `:170` (same
  module, usable). `effective_all_llm` is computed at `:411-412`
  (`user_participant_id.is_some() || roles.iter().any(|(r,_)| r == "USER")`),
  the all-LLM pause block `:413-453`, the queue block `:455-486`, the room load
  `:488-495`, **the selector arm `:497-523`** (`next_participant_id =
  result.next_speaker_id.clone()` `:521`, `selection_reason =
  result.reason.to_string()` `:522`), "No next speaker" `:525-532`. **The
  insertion point is between `:522` and the arm's closing brace `:523`.**
  `turn_state.last_speaker_id: Option<String>`, `next_participant_id:
  Option<String>`, `selection_reason: String` — the v4 condition is five
  plain tests.
- **`crates/quilltap-core/src/select_speaker.rs`** (NOT `services/` — the
  brief's path was off): `select_next_speaker` `:200`; the single-character
  case `:250-258` returns `build_result(active[0], "only_character", ...)`
  where `active` = present, non-archived CHARACTER seats **including
  user-controlled ones**. UNCHANGED by the commit. v5 shares the runaway: a
  persona-less one-on-one chat chains to `max_chain_depth` 20 on every
  message.
- **Production consumer:** `services/orchestrator.rs:4429-4460` (the chain
  loop): `effective_user_participant_id` = `initial.user_participant_id ??
  ('__user__' unless continue-mode)` (`:4433`-ish), `ChainGuards {
  never_pause_for_user: opts.never_pause_for_user }`, then `should_chain_next`;
  `!decision.chain || decision.participant_id.is_none()` ends the chain with
  `persist_turn_participant_id(.., None)` and a `chain_complete` frame
  carrying `reason: chain_reason_str(decision.reason)` (`"user_turn"`),
  `next_speaker_id: None`. **So the new return needs no consumer change** —
  the same arm already serves `no_next_speaker`; the SPA (`turn-order.ts:85`,
  `salon-conversation.ts:2554`) already handles `user_turn` with a null
  `nextSpeakerId` (the selector's own "no active seat" return produces it).
  Other `should_chain_next` callers: the harness test only (grep).
- **In-crate log pins:** `turn_orchestrator.rs` `mod tests` (the capture
  helper `decide(db, chat_id, depth, now_ms)` `:1069-1101` passes
  `user_participant_id = None`; `T` const `:1103`; five `chain_*` tests
  `:1106-1185` whose expected lines are **v5-literal snake_case**, e.g.
  `INFO {T} [TurnOrchestrator] Max chain depth reached chat_id={CHAT}
  chain_depth=20 max_chain_depth=20`). `seed_chat` `:1034-1066` seeds ONE llm
  seat `p1` with `characterId: ""` and NO messages. These are literal pins,
  **not** v4-captured: no oracle case in this family records v4 log lines.
- **The family:** `crates/quilltap-harness/tests/
  turn_orchestrator_tier2_equivalence.rs` (269 lines; header = the recipe,
  `:25-38`): env `QT_ORACLE_TURNORCH` + `QT_FIXTURE_TURNORCH` +
  `QT_FIXTURE_TURNORCH_MOUNT`; fixtures are built to `/tmp` by
  `harness/oracle/fixtures/build-turn-orchestrator-fixture.ts` (238 lines;
  reads `chats` / `characters` straight from the spec — **no committed `.db`**);
  oracle case `harness/oracle/cases/turn-orchestrator-tier2.ts` (300 lines,
  `tsx` real-DB path, no jest); spec `harness/oracle/fixtures/
  turn-orchestrator-tier2.json` (**10 chats, 18 ops**; the chain ops carry
  `note`s: chain-yes, user-turn stop, paused, depth, time, all-LLM pause at 3,
  queue-pop ×2, staff-excluded, bug 131). The Rust op enum `Op::ShouldChainNext`
  `:50-66` has **no guards field** — `let guards = ChainGuards::default()`
  `:164`, one for every op; the oracle's call `:256-261` is
  `shouldChainNext(repos, op.chatId, op.userParticipantId ?? null,
  op.chainDepth, op.chainStartTimeMs)` — 5 args, **no `config`, no `guards`**.
  Result object compared per op: `{op, chatId, chain, participantId,
  characterName, reason, selectionReason}` + the final `chats` dump
  (zero-normalization). No existing chat has ONE active character; the lone
  paths are unexercised.
- **Tier-3:** `orchestrator_tier3_equivalence` (3,861 lines) fixture
  `orchestrator-tier3.json` has 71 chats; the only 1-participant chats are
  `fa110005…`, `fb110006…`, `cb110006…`, `cb120006…`, `cb130006…` — each
  `isPaused: true` and driven as a single `kind:'chain'` call with
  `continueMode: true`, so they stop at the pause guard and **never reach the
  selector arm**. The 2-LLM chats select `weighted_selection` / `cycle_order`,
  not `only_character`. Expected: **no tier-3 case moves** (measure; A4).

### A2.2 bug 248 surfaces

- **`crates/quilltap-core/src/services/conversation_render_job.rs`** (535
  lines): `handle_inner` `:122-?`. Step 3 `:162-186` (upsert loop: builds
  `CcUpsert { embedding: None }`, mints `new_id = Uuid::new_v4()` per chunk
  BEFORE the write, `db.write(|ws| ws.main().conversation_chunks().upsert(&input,
  &new_id, &now_for_write)).await?` — result `(id, created)` is DISCARDED).
  Step 4 `:188-200`: `if !result.interchanges.is_empty()` →
  `enqueue_embeddings(...)`, error → WARN `Failed to enqueue embedding,
  continuing` (target `quilltap::jobs`, camelCase `jobId`, `chatId`, `error`).
  `enqueue_embeddings` `:217-262`: profile read via
  `db::embedding_profiles::find_all_full_json`, `isDefault` find, **returns
  `Ok(())` silently when none** (the empty `else`), then a
  `find_by_interchange_index` read PER interchange (`:240-243`, via
  `db.read_main`, with `?`) and `if payload.full_reembed || !chunk.has_embedding`
  → `enqueue_embedding_generate(...)`. Tracing target for the file's lines:
  `quilltap::jobs`, camelCase field names (P4.126 converted them).
- **`db/conversation_chunks.rs`**: `CcRow { id, chat_id, interchange_index,
  content, has_embedding, embedding_dim }` `:175-215` (`has_embedding =
  decoded dim > 0`, so a NULL **or empty** BLOB is "no embedding" — matches
  JS `!existing.embedding` for v4's hydration); `find_by_interchange_index`
  `:561` (rowid-order first row, mirrors `findOne`); **`upsert(input, new_id,
  now_iso) -> Result<(String, bool)>` `:591-640` ALREADY takes the caller's id**
  (P4.6BK) and returns the id used — so the repository half of the commit is
  already behaviourally in place: the insert arm uses `new_id`, the update arm
  ignores it. `embedding_status.rs:266` / `:313` carry the **precedent** for the
  repository-layer bare DEBUG: a `// v4 logs logger.debug('Creating embedding
  status row on …')` COMMENT and no tracing line (and `db/fallback.rs:847-848`:
  "v4's backend line … unported by standing convention; so are the success
  INFOs (`Entity created`, `Entity deleted`)").
- **Fallback home for the read:** `db/fallback.rs:193-204
  find_one_by_filter_or_none(collection, read)` emits ERROR `Error finding
  entity by filter` `{collection, error}` (`log_find_one_by_filter_failure`
  `:3100`) — exactly the inner line of v4's `findByInterchangeIndex`.
- **The families covering it:**
  - `crates/quilltap-harness/tests/embedding_remainder_equivalence.rs` (928
    lines; the render phase `:715-742` calls
    `handle_conversation_render_in_zone(&db, "render-<name>", user, &payload,
    &spec.render_now_iso, &UTC)` and compares the outcome tuple + SEVEN table
    dumps; recipe header `:99-125`, jest oracle
    `harness/oracle/cases/embedding-remainder.test.ts` (400 lines) with ONE
    stub — `ensureProcessorRunning`; committed fixture pair
    `crates/quilltap-web/tests/fixtures/embedding-remainder-{main,mount}.db`;
    spec `harness/oracle/fixtures/embedding-remainder.json`). Render cases
    today (6): `unrendered-chat-first-render`, `rendered-chat-reembeds-only-
    missing`, `rendered-chat-full-reembed-dedupes`, `healthy-chat-skips-
    already-embedded-chunk`, `empty-chat-returns-early`, `missing-chat-returns-
    early`. The corpus default profile is the BUILTIN tfidf one; a
    `clearDefault` prelude exists ONLY for `dimensionReconcileCases` (`:792`).
    **No log capture of any kind in this oracle** (it records `kind:'render'`
    rows of `{name, outcome, error}` and table dumps).
  - `conversation_chunks_tier2_equivalence.rs` — the repository's `upsert`
    update / create arms (P4.44, "the CREATE arm"), spec-driven.
  - In-crate `conversation_render_job.rs` `mod log_tests` `:333-535` (uses
    `test_support::global_capture::capture`, fixture `tools::annotations::
    log_tests::{db, CHAT, NOW, OTHER_CHAT, USER}`): `a_render_logs_v4s_success_line`
    `:372-406` **asserts `jobs(&lines,"DEBUG").is_empty()` at `:388`** — it will
    trip; `a_chat_with_no_events_...` `:409-426`, `a_failed_chat_read_...`
    `:429-456`, `a_missing_chat_...` `:459-472` (the two silence legs at `:455`
    and `:471` return before step 3, so they keep passing),
    `an_enqueue_failure_warns_with_v4s_field_names` `:478-506`.
  - **No production-SQL-capture plan pins the render job's statement order**
    (grep: the only `trace_v2` / capture users are `speaker_names_equivalence`,
    `build_context_tier3_equivalence`, the host test common module).

### A2.3 the 01f508d4e line surfaces

- **Transcript projection:** `crates/quilltap-core/src/api/transcript_projection.rs`
  + `api/chat_transcript.rs` carry **neither** `[Transcript] Projecting chat
  transcript` **nor** the surviving `Projected chat transcript` line
  (grep for both texts under `crates/` and `harness/`: no hit; the only
  `[Transcript]`-ish v5 lines are `[Messages API v1] …` at
  `chat_transcript.rs:77/98/105/132/138/167/181`). Nothing to remove.
- **Sync disk read:** `crates/quilltap-core/src/services/mount_index/sync/
  apply_disk.rs` — doc `:94-98`, `read_disk_file` `:99-109` (containment check
  first, then `tracing::debug!(target: "quilltap::mount_index", relativePath =
  %relative_path, "[Sync] Reading disk file")`, then the read); one in-crate pin
  `a_disk_read_logs_v4s_line_once` `:389-399` expecting `["DEBUG
  quilltap::mount_index [Sync] Reading disk file relativePath=Notes/a.md"]`;
  its sibling `an_escaping_disk_read_errs_silently` `:402-409` stays valid.
  Caller `sync/mod.rs:507`. `[Sync] Disk applier ignoring action` (`:375`)
  stays. **No oracle comparison captures the line** (grep `harness/`: none) —
  the P4.D274 work order (`p4.d274-...md:691, :1274`) is the only other record.
- **Post Office:** `crates/quilltap-core/src/post_office/mailbox.rs`:
  `LOG_TARGET = "quilltap::post_office::mailbox"` `:46`; `discard_letter`
  `:434-444` (`tracing::debug!(target: LOG_TARGET, vaultId, path, deleted,
  "discardLetter")` — message `:441`); `mark_mail_alerts_delivered` `:637-682`
  (WARN message `:668`, DEBUG `"markMailAlertsDelivered"` `:679`; fields
  `chatId, vaultId, via, requested, marked`); doc comments naming the old
  texts at `:418`, `:624`, `:633-635`.
  **Pin sites that carry the old DEBUG texts (all must move):**
  1. `services/orchestrator.rs:5799-5806` — unit pin
     `l.contains(" markMailAlertsDelivered ")` + literal `"DEBUG
     quilltap::post_office::mailbox markMailAlertsDelivered chatId=… via=tool-only
     requested=1 marked=1"`;
  2. `quilltap-harness/tests/orchestrator_tier3_equivalence.rs:2116-2160` —
     `filter(|l| l.contains(" markMailAlertsDelivered "))` and the literal
     `"DEBUG quilltap::post_office::mailbox markMailAlertsDelivered"` prefix
     built from the oracle's `mail` rows;
  3. **`harness/oracle/cases/orchestrator-tier3.test.ts:756`** — the ORACLE
     side: `Logger.prototype.debug` is spied and `if (message ===
     'markMailAlertsDelivered') mailLines.push(...)` — an EXACT-name filter;
  4. `quilltap-harness/tests/mail_alerts_delivered_equivalence.rs:228` —
     v5-side filter `l.contains("markMailAlertsDelivered") || …"markAlerted" ||
     …"Error finding document"` (v4's side records ALL DEBUG/WARN/ERROR lines
     of the call, `harness/oracle/cases/mail-alerts-delivered.test.ts:171-208`,
     so it follows v4 automatically);
  5. `quilltap-harness/tests/mail_carina_tools_equivalence.rs:855` and `:869` —
     literals `"DEBUG quilltap::post_office::mailbox discardLetter vaultId={rv}
     path={} deleted=true|false"` (v5-side pins "the texts are v4's at
     `12c336fad`").
  (`orchestrator_tier3`'s oracle also calls `discardLetter` at
  `orchestrator-tier3.test.ts:930` for cleanup only.)

---

## A3. Tier split

| unit | tier | why |
|---|---|---|
| S1-U1 decision | **tier 2** (real DB, structural + decision-object diff) | `shouldChainNext` reads/writes `chats`; deterministic once `Math.random` is pinned. Extends the existing family. |
| S1-U1 INFO line | in-crate literal pin (the siblings' convention) + OPTIONAL oracle capture | the family has no v4-line capture today; a literal pin matches `:1105-1185`. |
| S1-U2 decision | **tier 3** shape (`embedding_remainder_equivalence`, jest oracle on the real data layer, no model) | state diff already covers who is enqueued. |
| S1-U2 DEBUG lines | NEW v4-captured lines in that family (oracle splices the logger) + literal in-crate pins | closes "no log capture here". |
| S1-U2 repository half | tier 2 (`conversation_chunks_tier2_equivalence`) — one new `upsert` op with a supplied id | the only behaviour: supplied id kept on the create arm, ignored on the update arm. |
| S1-U3 | pure renames + a deletion; capture pins only | mail families already compare lines (v4-captured); the sync line is unit-pinned. |

---

## A4. Differential plan

### S1-U1 (bug 249)

Regenerate from the pin: `turn_orchestrator_tier2_equivalence` oracle NDJSON
(recipe header `:25-38`, Node `v24.13.1`, `tsx`, from the v4 checkout).

**Harness edits (both sides, in lockstep):**
1. Oracle case `turn-orchestrator-tier2.ts`: add `neverPauseForUser?: boolean`
   to `Op` (`:39-51`) and call `shouldChainNext(repos, op.chatId,
   op.userParticipantId ?? null, op.chainDepth, op.chainStartTimeMs,
   DEFAULT_CHAIN_CONFIG, { neverPauseForUser: op.neverPauseForUser === true })`
   (`DEFAULT_CHAIN_CONFIG` is exported by the service — the v4 test imports it).
   Absent key ⇒ `{ neverPauseForUser: false }` ≡ the default `{}` for every
   existing op.
2. Rust test `Op::ShouldChainNext` gets `#[serde(rename = "neverPauseForUser",
   default)] never_pause_for_user: bool`; build `ChainGuards { never_pause_for_user }`
   PER OP instead of the single `ChainGuards::default()` at `:164`.

**New spec chats + ops** (committed `turn-orchestrator-tier2.json`; ids in the
existing `cccc00NN…` / `eeee…` scheme; messages' `participantId` = the seat id;
keep ASSISTANT turns since the last USER **< 3** on every chat whose
`effectiveAllLLM` can be true, or the all-LLM pause WRITES `isPaused`):

| row | chat | op | expected shape (the oracle decides) |
|---|---|---|---|
| L1 | one llm seat (Aria); messages `[USER hello, ASSISTANT by seat]` | user `'__user__'`, depth 1 | `{chain:false, reason:'user_turn'}`, participantId/characterName/selectionReason null — **the bug-249 fix** |
| L2 | same, **no messages** | user `'__user__'` | `chain:true`, `participantId` = seat, `selectionReason:'algorithm'` (lastSpeaker null) |
| L3 | one llm seat; `[ASSISTANT by seat]` only | user **null** | `chain:true` (effectiveAllLLM true ⇒ monologue kept; turnCount 1) |
| L4 | one llm seat; `[USER, ASSISTANT by seat]` | user **null** | `{chain:false, reason:'user_turn'}` — presence comes from the USER row in history (the continue-mode of a persona-less chat) |
| L5 | one llm seat; `turnQueue ["<seat>"]`, messages `[USER, ASSISTANT by seat]` | user `'__user__'` | the queue entry equals the last speaker ⇒ SKIPPED and `turnQueue` written back `[]`; then the selector arm ⇒ early return `user_turn`; the final `chats` dump carries `turnQueue "[]"` — the nudge-after-speak path |
| L6 | one llm seat; `turnQueue ["<seat>"]`, messages `[USER]` (last speaker null) | user `'__user__'` | `selectionReason:'queue'`, `chain:true` (v4 test 5) |
| L7 | L1's chat shape | `neverPauseForUser:true` | `chain:true` (autonomous-room monologue, v4 test 4) |
| L8 | one **user-controlled** seat (a `controlledBy:'user'` character); `[USER by seat]` | user = the seat id | early return fires (`isAllLLM` false): `user_turn` **without** `participantId`/`characterName` where the pre-commit arm named them — the A6.4 finding, decided by the oracle |
| L9 | two CHARACTER seats, the second `status:'absent'` (so one ACTIVE) ; `[ASSISTANT by seat 1]` | user `'__user__'` | like L1 — "one active", not literally one participant |

L1–L9 add 9 chats / 9 ops; every pre-existing op's output must stay
byte-identical (the red-first proof is the per-op diff on the OLD ops
staying green while L1/L4/L5/L8/L9 go red against the unported v5).
`resolveCycleOrder` is a no-op for ≤1 candidate, so L1–L9 never write a
rotation.

**Capture pin (INFO line):** three in-crate tests next to `:1105-1185`:
the line fires once on L1's shape with the exact fields, and is SILENT on L2,
L3, L6, L7 (name them). Extend `decide` (`:1066`) to take the user participant
id and a messages seeder (the current `seed_chat` has no messages and passes
`None`). Expected rendered line under the module's snake_case house style:
`INFO quilltap_core::services::turn_orchestrator [TurnOrchestrator] Lone
character has answered; floor returns to the operator chat_id={CHAT}
chain_depth=2 participant_id=p1` (ruling candidate §A7.2: snake_case like the
seven siblings vs v4's camelCase).

**Optional stronger pin:** splice the oracle's logger (A6 / memory
`v4-oracle-job-host-and-log-capture`) and record `{level,message,fields}` per
`shouldChainNext` op into a `lines` field; compare in the Rust test through the
global capture. Not required: no other line in this family is v4-captured.

**Blast radius check (run, do not predict):** the tier-3 `orchestrator_tier3_
equivalence` and `help_chat_orchestrator_tier3_equivalence` families must stay
green with NO change; if either goes red the tier-3 oracle has a chain I
mis-read (A2.1 says the five lone-seat chats are paused).

### S1-U2 (bug 248)

Regenerate from the pin: `embedding_remainder_equivalence` (recipe `:99-125`,
jest, TZ=UTC, against /tmp COPIES of the committed fixture pair) and
`conversation_chunks_tier2_equivalence`.

1. **The state diff needs no change and must stay green** on the unmodified
   corpus — the in-process jest oracle has read-your-writes, so v4's
   pre-/post-fix states agree. State this as the acceptance gate: *the family
   passes before AND after the port; the new rows below are the proof of the
   new behaviour*.
2. **Add log capture to the oracle** (`embedding-remainder.test.ts` Phase 2,
   `:266-285`): splice the logger transports (memory
   `v4-oracle-job-host-and-log-capture`, point 2), record per render case the
   `[ConversationRender]` lines (level, message, fields in v4's order) into the
   `kind:'render'` row as `lines`. In the Rust test (`:715-742`) capture with
   `test_support::global_capture` (the handler logs on the caller thread —
   `handle_inner` and `db.read_main` closures both) and compare as whole
   strings via the `v4_line` renderer pattern
   (`chat_informs_tier2_equivalence.rs:174`). Compare only `[ConversationRender]`
   lines (the jest run emits lots of repository noise).
3. **New render cases** (spec `renderCases`, `RenderCase` gains optional
   `setDefaultProfileId` / `clearDefault` preludes copied from
   `DimensionReconcileCase`, run identically on both sides; restore the BUILTIN
   default afterwards — phases after depend on it):
   - `no-op-rerender-needing-nothing` — re-render a chat whose chunks are all
     embedded and unchanged (the `healthy` chat after its first render):
     DEBUG `Chunks upserted … needingEmbedding=0 fullReembed=false`, **no
     profile read, no `No default embedding profile` line**.
   - `no-default-profile-leaves-chunks-for-reconcile` — `clearDefault` prelude
     + a chat with a new chunk: `Chunks upserted … needingEmbedding=N`, then DEBUG
     `No default embedding profile; chunks left for the startup reconcile
     {jobId, chatId, pending: N}`, and NO enqueue (`background_jobs` diff
     unchanged).
   - `full-reembed-queues-every-chunk` already exists
     (`rendered-chat-full-reembed-dedupes`) — it now also pins
     `needingEmbedding = interchangeCount`, `fullReembed=true`.
   - `changed-content-queues-under-existing-id` — the existing
     `rendered-chat-reembeds-only-missing` already changes `f0…01`'s content and
     NULLs its vector; confirm in the measured output that it is queued under
     the EXISTING id (the `<chunk:{chatId}#{index}>` remap keeps pinned ids
     visible).
4. **`conversation_chunks_tier2_equivalence`:** one new `upsert` op carrying a
   caller `id` on the create arm (the row's id must be the supplied one) and one
   carrying an `id` on the update arm (ignored). In v5 the id arrives as the
   `new_id` argument, so the Rust op passes the supplied id as `new_id`; the v4
   oracle op passes `id` in the input. The existing placeholdering of minted
   ids must not mask the supplied id (pin it in the spec, like the other
   corpus-pinned ids).
5. **In-crate pins** (`log_tests`): replace `jobs(&lines,"DEBUG").is_empty()` at
   `:388` with the exact DEBUG set for that fixture (measure whether the shared
   `log_tests::db()` has a default embedding profile — if not, expect BOTH
   DEBUGs); add one test per new line with its silence legs.

**Code shape (specification, not code):** step 3 becomes per-interchange:
`existing = fallback::find_one_by_filter_or_none("conversation_chunks", ||
db.read_main(...find_by_interchange_index(&chat, idx)))` on the CALLER thread
→ `db.write(...upsert(&input, &new_id, &now)).await?` returning `(id, _)` →
`needs = full_reembed || existing.is_none() || !existing.has_embedding ||
existing.content != interchange.content` → `chunk_id = existing.map(id)
.unwrap_or(id_from_upsert)` → push. Then the unconditional DEBUG; then step 4
gated on `!to_embed.is_empty()`, reading the profile once, enqueueing from the
list (no re-read), the `else` DEBUG. Keep the per-chunk `Uuid::new_v4()` minted
before the write (ids are minted when no row exists; the corpus remap handles
it). The repository: add the supplied-id comment at the create arm, no tracing
line (A6.6).

### S1-U3 (log lines)

- **Mailbox rename:** change the two `tracing::debug!` messages
  (`mailbox.rs:441` → `"Letter discarded"`, `:679` → `"Mail alerts marked
  delivered"`); fields untouched; doc comments `:418`, `:624`, `:633-635`
  updated; the WARN at `:668` NOT touched.
- **Re-pin** (all five sites of A2.3): update the literals; in
  `mail_alerts_delivered_equivalence.rs:228` add `|| l.contains("Mail alerts
  marked delivered")` to the v5 filter (the oracle's own `logs` list already
  contains the renamed v4 line, so the equal-count assertion will go red loudly
  until the filter widens); update the ORACLE-side spy at
  `orchestrator-tier3.test.ts:756` to the new message; then regenerate
  `orchestrator_tier3_equivalence`'s oracle and the Rust prefix literal at
  `:2144`. Regenerate the mail oracles from the pin
  (`mail-alerts-delivered`, `mail-carina-tools` cases).
- **Disk read:** delete the `tracing::debug!` in `read_disk_file`
  (`apply_disk.rs:102-106`), fix the doc `:94-98`, REPLACE the positive unit
  test with a silence leg: reading a file logs **nothing** (`lines.is_empty()`);
  the escape test stays. P4.D274's work order records the line — leave history.
- **Transcript:** no code. Record "v5 never carried the line" in the round
  record.

---

## A5. Fixtures

- `turn-orchestrator-tier2.json` GROWS (9 chats / 9 ops); `build-turn-
  orchestrator-fixture.ts` needs **no change** (it iterates `spec.chats`; the
  character `Uma` is already `controlledBy: 'user'`; a lone user-seat chat needs
  a participant with that character — reuse `dddd0000…`). The DBs are built to
  `/tmp` per run — nothing committed to re-mint.
- `embedding-remainder.json` GROWS (two render cases + two prelude fields on
  `RenderCase`); the **committed pair** `embedding-remainder-{main,mount}.db`
  is NOT rebuilt (a rebuild mints fresh UUIDs and invalidates every consumer —
  the file's own header `:119-125`).
- `conversation-chunks` spec (`build-conversation-chunks-fixture.ts` +
  its json): two ops, no schema.
- Mail families: no fixture change; the oracle `.ts` cases change one string.

---

## A6. Traps

1. **`Db::write` closures are invisible to `captured_with`** (memory
   `capture-rig-writer-thread`). The render handler's new DEBUGs and the
   fallback read home must run on the caller thread: do the pre-read with
   `db.read_main` (caller-thread closure) — NOT inside the `db.write`
   closure. A second trap from the same note: a `captured_with` pin sharing a
   binary with a differential caches the callsite as "never" — the
   `log_tests` module already uses `global_capture`; keep it, and run each new
   pin together with its binary's siblings before trusting it.
2. **The pre-write read is a FALLBACK read, not a `?`.** v4's
   `findByInterchangeIndex` swallows a failure to `null` (ERROR `Error finding
   entity by filter`), so the chunk is simply queued. v5's old read lived INSIDE
   the caught `enqueue_embeddings` block (a failure → WARN `Failed to enqueue
   embedding, continuing`); lifted into step 3 with `?`, a read failure would
   now FAIL the job (retry → DEAD) — a silent regression class (memory
   `unify-review-finding-shapes`: "fallback reads v5 `?`s"). Use
   `fallback::find_one_by_filter_or_none` and pin the ERROR line with a
   planted failing read (a dropped `conversation_chunks` column on a COPY, as
   `a_failed_chat_read_warns_not_found_and_completes` plants `DROP TABLE
   chats`; careful — the upsert after it would also fail; plant so ONLY the
   SELECT fails, e.g. a BEFORE-UPDATE-free approach: rename a column the read
   names but the upsert's statements don't). If no clean plant exists, record
   the read failure as pinned at the home's unit level and say so.
3. **A read-then-write window now exists** (read on the pool, write on the
   writer). v4 has the same window; v5 had none because it re-read after the
   write. Concurrent EMBEDDING_GENERATE for the same chunk can only cause a
   redundant enqueue (the enqueue dedupes PENDING) — acceptable and v4-shaped.
   Do NOT "improve" by reading inside the writer closure: it hides the
   fallback ERROR from the capture rig and changes the statement order the
   pins see. Record the choice in the order.
4. **`has_embedding` ≡ JS `!existing.embedding`.** An empty vector hydrates NULL
   (memory `embedding-lane-oracle-gotchas`: "zero-length blobs hydrate NULL");
   v5's `dim > 0` matches. `existing.content !== interchange.content` is an
   exact decoded-string compare (`CcRow.content` is decoded through
   `CompressedText`, `marshal_cc_row` — keep it decoded).
5. **The step-4 gate changes WHICH reads happen.** A no-op render now makes no
   profile read; a SQL-trace-visible change and the only reason a `find_all_full_
   json` assertion (if any lane adds one) differs.
6. **The third DEBUG is a repository-layer line** (`Creating conversation
   chunk` from the BARE root logger): the standing convention (`embedding_status
   .rs:266/313`, `fallback.rs:847-848`: v5's `db::` layer logs nothing
   generic) says comment-only. The ledger row counts it as a port ("three
   DEBUG lines"); this survey recommends the comment and flags the choice
   (§A7.3).
7. **Mail rename: do not touch the WARN**, and do not grep-and-replace
   `markMailAlertsDelivered` repo-wide — the doc comments, the WARN text, the
   Rust function / `MailAlertsVia` names and the work orders keep it. The
   `orchestrator_tier3` mail assertion is guarded against vacuity by the `vias`
   check (`:2171-2175`, `["finalizer","preserved-partial","turn-skip"]`): if the
   oracle spy keeps the OLD exact name the oracle lines go empty and that guard
   fails loudly — good; do not weaken it.
8. **Unify finding shapes that apply** (memory `unify-review-finding-shapes`):
   a false "pinned" claim — do not write "the INFO line is capture-pinned"
   unless the pin exists and fails when the line is removed (mutation-prove
   it); a guard that passes on a constant; a `tracing` field name whose case
   differs from the neighbours (A7.2).
9. **A turn state the harness cannot reach:** `isAllLLMChat` is false for any
   user-controlled seat, so L8 exercises the lone-USER-seat branch, not the
   persona-less one; do not label L8 "the bug-249 case".
10. **Lone user-controlled seat that spoke last** (L8): the new early return
    precedes the `isUserDrivenSeat` arm, dropping `participantId` /
    `characterName` from the `user_turn` decision (`:221-243`) that the Salon
    uses to label the pause and aim the Skip button. Port v4 faithfully (the
    oracle decides); this is a candidate **v4 bug to file** under the port's
    usual practice, not a v5 divergence to invent a fix for.

---

## A7. Contract candidates and small rulings

**No contract (C1-style) candidates in this lane** — no shared type, signature
or wire shape changes; `ChainDecision::stop` is private and in-module, the
render handler's helper is local, `fallback::find_one_by_filter_or_none`
exists.

Rulings the lanes will want (small, can default as stated):
1. *Pre-read thread* (A6.3): `db.read_main` on the caller thread (default).
2. *Field-name case of the new INFO line*: the module's seven siblings are
   snake_case (`chat_id`, `chain_depth`) and pinned that way; the render job's
   lines were converted to v4's camelCase by P4.126. Default: **match the
   file's neighbours (snake_case)**; the lane records the choice.
3. *`Creating conversation chunk`*: default **comment-only**, per precedent.
4. *Oracle log capture for the lone-character INFO* (optional, A4): default
   literal in-crate pin only.

---

# PART B — LANE S2: data / CLI / memory-time

## B0. The span in one paragraph

`75d094e63` (2026-10-14, checklist 12) is a help-text and completion-template
byte change in the CLI plus a v4-only `migrations status` fix. `49f8906c3`
(2026-10-14, checklist 10) makes the new-account restore's pure remapper also
rewrite the seat ids hidden inside three JSON-string chat columns and each
message's whisper audience — one `lib/` hunk, plus tests-only "fidelity"
additions. `7458ff94e` (2026-10-14) appends a fixed parenthetical to a
non-empty hair line in `describeOutfit`; because `describeOutfit` is a leaf
every wardrobe / image / Aurora surface calls, it is a model-visible text move
with no caller hunk. `25b8fab6a` (2026-10-16) is **this port's own bug-247
filing converging**: v4 reads three word tables through `Object.hasOwn`, and
catches the parser in `resolveEpisodicAnchors`; v5's closed tables never had
the defect, so the port is "retire a harness pin by measurement". Of the four,
only `49f8906c3` changes v5 bytes in a data path, and it RED-FLAGS roughly
twenty existing restore cases by design (§9.4).

---

## B1. v4 hunks, file by file

### B1.1 `49f8906c3` — `lib/backup/restore/uuid-remap.ts`

`remapBackupData(data, targetUserId, remapper)` (`:70`) is called only in
`new-account` mode. Hunk 1 — new, between `planWardrobeImagePointerFixes` and
`remapBackupData`, **pin `:174-198`**:

```ts
const PARTICIPANT_ID_JSON_FIELDS = [
  'turnQueue',
  'spokenThisCycleParticipantIds',
  'cycleOrderParticipantIds',
] as const;                                                                    // :178-182

function remapJsonIdList(value: unknown, remapper: UuidRemapper): unknown {    // :189
  if (typeof value !== 'string') return value;
  try {
    const parsed: unknown = JSON.parse(value);
    if (!Array.isArray(parsed) || !parsed.every((id) => typeof id === 'string')) return value;
    return JSON.stringify(remapper.remapArray(parsed as string[]));
  } catch {
    return value;
  }
}
```

(The comment: "A value that is not a JSON string array is returned untouched — the
turn manager already reads a malformed value as empty, so there is nothing to
gain by rewriting it.") Semantics: non-string ⇒ the SAME value; `JSON.parse`
failure, non-array, or any non-string element ⇒ the ORIGINAL STRING untouched
(not re-serialised — whitespace preserved); a valid all-string array ⇒
`JSON.stringify` of `remapper.remapArray(...)`: compact (`["a","b"]`), element
order preserved, duplicates map to the same new id, every id minted-or-reused
in list order. An empty list `"[]"` ⇒ `"[]"` (no mint). `UuidRemapper.remap`
(`lib/backup/uuid-remapper.ts:31`) mints for EVERY unseen string, whether or not
it is a seat that exists.

Hunk 2 — **inside the chat loop, pin `:352-361`, immediately AFTER the
`scenarioSource` remap (`:348-350`, `545e299bb`) and BEFORE `// Remap
participants` (`:363`)**:

```ts
for (const field of PARTICIPANT_ID_JSON_FIELDS) {
  if (field in chat) {
    (remappedChat as Record<string, unknown>)[field] = remapJsonIdList(
      (chat as unknown as Record<string, unknown>)[field], remapper);
  }
}
```

`field in chat` is key PRESENCE (a present key holding `null`/`undefined` is
visited — non-string ⇒ unchanged); an ABSENT key stays absent; `remappedChat`
(the shallow copy built by `remapArrayFields(remapFields(chat, [...]), ['tags',
'impersonatingParticipantIds'])` `:326-343`) already holds the key, so the
assignment keeps its **position**. The mint order consequence: ids first seen in
these lists are minted **before** the participants / messages are remapped
(`:363-381`), which shifts every later counter id.

Hunk 3 — messages, **pin `:373-381`**: `remapArrayFields(remapFields(msg, ['id',
'swipeGroupId', 'participantId']), ['attachments', 'targetParticipantIds'])` —
`targetParticipantIds` (a whisper's audience) is remapped only when it is an
ARRAY (`Array.isArray`); null / strings / absent untouched; key position kept.

**Tests.** `uuid-remapper.test.ts` +43 (commit `@@ -524`), two cases under
`remapBackupData() - project FK remapping`:
1. *re-points the turn rotation, turn queue and whisper audiences at the
   renamed seats* — a chat with `participants [{id:'seat-a'},{id:'seat-b'}]`,
   `turnQueue '["seat-b"]'`, `spokenThisCycleParticipantIds '["seat-a"]'`,
   `cycleOrderParticipantIds '["seat-b","seat-a"]'`, one message
   `{participantId:'seat-a', targetParticipantIds:['seat-b']}` ⇒ each list
   remapped to the SAME new ids the participants received, order kept,
   `seatA !== 'seat-a'`.
2. *leaves a malformed or absent turn-rotation value as it found it* —
   `cycleOrderParticipantIds: 'not json'` stays `'not json'`; the absent
   `turnQueue` key is `not.toHaveProperty`.

**`restore-field-fidelity.test.ts` — +107 lines APPENDED to an EXISTING file**
(hunks `@@ -226`, `-247`, `-268`, `-893`; the ledger calls it NEW — §9.4). Tests
only, mocked repositories, merge mode: a `describe('restore field fidelity — 4.10
data-model additions')` of four `it`s plus a `tfidfCreate` mock in the repo
stub — each calls `restore('/tmp/backup.zip', { mode: 'merge', targetUserId:
'user-1' })` and `toMatchObject`s the create call: (1) a chat's
`conciergeMode: 'locked'`, `conciergeModeSetBy: 'operator'`,
`conciergeModeReason: 'manual'`, a `scenarioSource {mountPointId, path,
bodySha256}`, `scenarioNotes`, `cycleOrderParticipantIds` and a message's
`routeTrail` (two entries) survive; (2) a memory's `supersededAt`; (3)
`chat_settings.impersonationVoiceMode: 'ask'` + `wardrobeImageSettings
{imageProfileId, toolImagesEnabled}`; (4) `tfidf_vocabularies.textShapeVersion:
2`. **For the record: no code hunk — nothing to port.** (v5's equivalents are
covered by the `system_restore_state` carrier families: `textShapeVersion`,
`supersededAt`, `wardrobeImageSettings`, `impersonationVoiceMode`, `routeTrail`
all appear in `crates/quilltap-core/src/{services/backup,services/carriers,db}`.)

### B1.2 `7458ff94e` — `lib/wardrobe/slot-guidance.ts`, `outfit-description.ts`

`slot-guidance.ts` (pin `:44-53`):

```ts
export const HAIR_STYLING_PRECEDENCE_NOTE =
  "current styling — overrides the natural hair's texture and style in the " +
  'physical description';
```

Concatenated value (one string; **U+2014 EM DASH**, `e2 80 94` verified; a
straight ASCII apostrophe in `hair's`):

> `current styling — overrides the natural hair's texture and style in the physical description`

beside the three unchanged constants `HAIR_SLOT_GUIDANCE`,
`HAIR_PHYSICAL_BOUNDARY` (`:29`), `HAIR_PHYSICAL_DESCRIPTION_NOTE` (`:39`).
`outfit-description.ts` (`describeOutfit` is `:103-156`; import `:22`; doc
bullet `:95-97`): the grouped-slots loop (pin `:150-153`) is now

```ts
for (const [value, slotsForValue] of groups) {
  const note = slotsForValue.includes('hair') ? ` (${HAIR_STYLING_PRECEDENCE_NOTE})` : ''
  lines.push(`- **${slotsForValue.join(', ')}:** ${value}${note}`)
}
```

Resulting bytes: `- **hair:** braided crown, silver pins (current styling — overrides the
natural hair's texture and style in the physical description)\n`. Facts from the
function body: the note rides ANY group whose slot list contains `hair` (a
multi-slot item equipped across `hair` + another slot joins them, e.g.
`- **accessories, hair:** <value> (<note>)` — slot order is
`WARDROBE_SLOT_TYPES` order); an EMPTY hair slot never forms a group (no
fallback — `emptyFallback` null / `isSlotReportedWhenEmpty` false, `:142-147`),
so it never carries the note; the `allVisibleEmpty` early return `- completely
naked and unadorned\n` (`:113-115`), the `- naked` collapse (`:128-132`) and the
garment fallbacks are untouched; an OMITTED hair slot (`options.omit`) forms no
group. The trailing `\n` is unchanged (`lines.join('\n') + '\n'`).

**Test** (`outfit-utils.test.ts`, commit `@@ -130` and `@@ -162`; import line
`+import { HAIR_STYLING_PRECEDENCE_NOTE }` `:9`): two expectations now
interpolate the constant — `` `- **hair:** braided crown, silver pins
(${HAIR_STYLING_PRECEDENCE_NOTE})` `` (the full dressed outfit) and `` `-
**hair:** braided (${HAIR_STYLING_PRECEDENCE_NOTE})` `` (the naked-with-hair
outfit: `- naked`, `- **footwear:** barefoot`, `- **accessories:** no
accessories`, then the hair line). v4 callers at the pin (every one inherits the
note with NO caller hunk): `lib/image-gen/appearance-resolution.ts:182`,
`lib/image-gen/avatar-prompt.ts:187,195`, `lib/memory/cheap-llm-tasks/
image-scene-tasks.ts:927`, `lib/services/aurora-notifications/writer.ts:40,52,
67,79`, `lib/tools/handlers/wardrobe-handler-shared.ts:244`, `lib/wardrobe/
item-image-prompt.ts:78` (`describeOutfit(bySlot, { omit }).trimEnd()`),
`lib/wardrobe/resolve-equipped.ts:155`. **Help:** `help/wardrobe.md` — "Three
consequences follow" → "Four consequences follow"; a new third bullet:
`**A styled coiffure outranks the natural hair.** Should the physical
description insist on hair *perfectly straight* while the Hair slot holds
*voluminous curls*, the curls carry the day. Wherever Quilltap sets the outfit
down --- a portrait prompt, a Lantern backdrop, the catalogue beneath a kept
photograph --- the hairdo is marked as the current styling, taking precedence
over the natural texture and style, so that neither portraitist nor character
is left to guess which head of hair is actually in the room.` (inserted between
the "Hats remain Accessories" and "Undressing does not undo the hair" bullets;
the `help/` count stays 131).

### B1.3 `75d094e63` — CLI bytes

Pin lines (all three captured by v5):

| file | pin line | OLD → NEW (verbatim) |
|---|---|---|
| `packages/quilltap/bin/quilltap.js` (`--help` text) | `:106` | `  anchor-probe <characterId>    Probe whether episodic anchor lines suppress Memory Gate reinforcement` → `  anchor-probe <characterId>    Probe whether episodic anchor lines suppress reinforcement at admission` |
| `packages/quilltap/lib/completion/zsh.template` | `:46` | `    'anchor-probe:Probe whether episodic anchor lines suppress Memory Gate reinforcement'` → `    'anchor-probe:Probe whether episodic anchor lines suppress reinforcement at admission'` |
| `packages/quilltap/lib/completion/fish.template` | `:64` | `complete -c quilltap -n '__quilltap_no_subcommand' -f -a 'anchor-probe' -d 'Probe anchor-line effect on Memory Gate'` → `complete -c quilltap -n '__quilltap_no_subcommand' -f -a 'anchor-probe' -d 'Probe anchor-line effect on admission'` |

`bash.template` never carried the phrase (no hunk). At the pin
`git grep -i "memory gate" -- packages/quilltap` is EMPTY (`anchor-probe-command.js`
`:7,:25-29` already says "admission"). `packages/quilltap/README.md` +14 / −4
(docs ls `-R/--sort/-r/--links`, `docs tree --long` "accepted but reserved",
`memories --limit N` default 50 / `--full-titles` / `grep -l --paths-only`,
`themes registry remove|refresh|keygen|sign`, `--names-only`): documentation of
EXISTING v4 parsers, no flag added.

**`migrations` — record for NO-PORT.** `lib/migrations-commands.js` (pin
`:46-57`): NEW exported `extractMigrationId(fileContent)`:

```js
const constMatch = fileContent.match(/^\s*const\s+MIGRATION_ID\s*(?::\s*\w+\s*)?=\s*['"]([^'"]+)['"]/m);
if (constMatch && /^\s*id:\s*MIGRATION_ID\b/m.test(fileContent)) return constMatch[1];
const literalMatch = fileContent.match(/^\s*id:\s*['"]([^'"]+)['"]/m);
return literalMatch ? literalMatch[1] : null;
```

used by `extractMigrationsFromSource` for `id` and the `description` fallback
(measured by v4 at the commit: 62 of 228 scripts use the constant form; at the
pin my line-anchored count is 61 of 226 — the same fact). New test
`lib/__tests__/migrations-source-ids.test.js` (54 lines): four `extractMigration
Id` cases (literal, `MIGRATION_ID`, typed `MIGRATION_ID: string`, none ⇒ null)
and two repo-scan cases (> 100 scripts; every script yields an id).
**Confirmed NO-PORT:** `crates/quilltap-cli/src/main.rs:66` lists `"migrations"`
in `SUBCOMMANDS`, but the `match sub_name` (`:253-263`) has no arm — it falls to
`other => not_yet_available(other)` (`:106-111`, exit 1); there is no
`migrations_cmd.rs`; grep for `migrations` under `crates/quilltap-cli` finds only
`main.rs`, the three templates, `main_help.txt` and one unrelated `docs_cmd.rs:345`
sentence. When a v5 `migrations status` arrives it must read BOTH id forms from
day one (the ledger's note stands).

### B1.4 `25b8fab6a` bug 247 — `lib/memory/time/clock.ts`, `anchors.ts`

`clock.ts` (pin):

```ts
// Word tables are read only through `lookupWord`, which answers own keys       // :94-96
// alone: ... ("constructor 5") must not pass for a month (bug 247).
const MONTH_NAMES: Record<string, number> = { january: 0, … december: 11 }     // :97-100
const UNIT_DAYS: Record<string, number> = { day: 1, days: 1, week: 7, weeks: 7, month: 30, months: 30, year: 365, years: 365 }  // :102-104
const WORD_NUMBERS: Record<string, number> = { a: 1, an: 1, one: 1, … fourteen: 14, couple: 2, few: 3 }                         // :106-110
function lookupWord(table: Record<string, number>, word: string): number | undefined {   // :112-114
  return Object.hasOwn(table, word) ? table[word] : undefined
}
```

The three reads (`resolveEventTime`):
- `:167-173` — `const monthFirstMonth = monthFirst ? lookupWord(MONTH_NAMES,
  monthFirst[1]) : undefined`; `const dayFirstMonth = dayFirst ?
  lookupWord(MONTH_NAMES, dayFirst[2]) : undefined`; `const named =
  monthFirst && monthFirstMonth !== undefined ? { month: monthFirstMonth, day:
  parseInt(monthFirst[2], 10), year: monthFirst[3] ? parseInt(monthFirst[3], 10)
  : null } : dayFirst && dayFirstMonth !== undefined ? { month: dayFirstMonth,
  day: parseInt(dayFirst[1], 10), year: dayFirst[3] ? parseInt(dayFirst[3], 10)
  : null } : null` (the regexes `:165-166` unchanged).
- `:191` — `const n = /^\d+$/.test(agoMatch[1]) ? parseInt(agoMatch[1], 10) :
  lookupWord(WORD_NUMBERS, agoMatch[1])`.
- `:192` — `const unit = lookupWord(UNIT_DAYS, agoMatch[2])`; the guard `:193` is
  unchanged (`n !== undefined && n > 0 && n < 10000 && unit`).

`anchors.ts` `resolveEpisodicAnchors` (pin `:54-74`; doc `:46-53` gains "or makes
the parser throw"): the `when && referenceIso` block is now

```ts
try {
  const resolved = input.timeZone
    ? resolveWhenPhrase(input.when, input.referenceIso, input.timeZone)
    : resolveWhenPhrase(input.when, input.referenceIso)
  if (resolved) occurredAt = resolved
} catch {
  occurredAt = input.fallbackIso
}                                                  // :61-68
```

with the comment "Never throws: a phrase the parser chokes on costs that memory
its event time, not the extraction job around it (bug 247)." **There is NO log
line on the catch** (a bare `catch {}`; verified). `narrativeTime` (`:70-72`) is
computed after, unchanged. The only production caller is `extraction/run.ts:308`.

**Tests.** `rule-table.test.ts` +7 (commit `@@ -117`): inside `describe('write-only
rules')`, an `it.each` over **seven phrases verbatim**:
`'constructor 5'`, `'5 constructor'`, `'on constructor 5'`, `'constructor 5th'`,
`'constructor 5, 2026'`, `'a constructor ago'`, `'two constructor ago'`, each
named `a word naming an inherited property is not a month or a number: %s (bug
247)`, expecting `resolveEventTime(phrase, nowMs, clock)` `toEqual({ occurredAt:
null, window: null })`. `anchors.test.ts` +23: `describe('resolveEpisodicAnchors
— never throws (bug 247)')` with (a) *keeps the fallback when the phrase parser
throws* — `jest.isolateModules` + `jest.doMock('../clock', () => ({
resolveWhenPhrase: () => { throw new RangeError('Invalid time value') } }))`,
then `isolated({ when: 'constructor 5', referenceIso: ANCHOR, fallbackIso:
WINDOW_START, timelineMode: 'realtime' })` ⇒ `{ occurredAt: WINDOW_START,
narrativeTime: null }` and (b) *resolves "constructor 5" to the fallback through
the real parser* (same input, no mock, same expectation).

**Measured facts (node, regex against the pin's patterns):** `constructor 5`,
`on constructor 5`, `constructor 5th`, `constructor 5, 2026` match `monthFirst`;
`5 constructor` matches `dayFirst`; **`a constructor ago` and `two constructor
ago` match NO pattern at all** (the ago regex requires a `days?|weeks?|months?|
years?` unit word); `constructor days ago` matches the ago regex but resolves to
null both before the fix (`WORD_NUMBERS['constructor']` is `Object`; `Object > 0`
is `false`) and after. `Object.getOwnPropertyNames(Object.prototype)` filtered
to `/^[a-z]+$/` (the phrase is lower-cased and the group is `[a-z]+`) is
`['constructor']` — the ONLY reachable inherited key. So the observable defect
is exactly the five month-shaped phrases (the throw is `RangeError: Invalid time
value` from `formatToParts`); the WORD_NUMBERS / UNIT_DAYS reads are
**unobservable hardening** (§9.7).

---

## B2. v5 today — per surface (`main` at `56554d98c`)

### B2.1 the uuid remap

- `crates/quilltap-core/src/services/backup/uuid_remap.rs` (1,033 lines):
  `remap_backup_data` `:116`; **`remap_chat` `:814-909`**: `chain_owned(r, chat,
  [id, activeTypingParticipantId, lastTurnParticipantId, projectId,
  storyBackgroundImageId, imageProfileId, otherExtractionWatermarkMessageId,
  episodeExtractionWatermarkMessageId, turnExtractionWatermarkMessageId],
  ["tags","impersonatingParticipantIds"], target_user_id)` `:815-837`, the
  `scenarioSource` arm `:848-862` (guarded by `is_truthy`, assigned onto the
  existing key via `insert`), then participants `:864-877` and the **messages
  arm `:879-895`**: `chain(r, m, &["id","swipeGroupId","participantId"],
  &["attachments"])`. **The insertion point for the JSON-list loop is between
  `:862` and `:864`; the array list at `:889` grows to `["attachments",
  "targetParticipantIds"]`.** No `turnQueue` / `spokenThisCycleParticipantIds`
  / `cycleOrderParticipantIds` / `targetParticipantIds` handling exists
  anywhere under `services/backup` (grep: none).
- `uuid_remapper.rs`: `remap(&Value)` `:105` (mint-or-reuse over any JSON
  value, `map_key`), `remap_str` `:120`, `remap_array(&Value)` `:141` (a
  non-array ⇒ `[]`), `remap_fields` `:159` (string-valued only, in-place
  `insert` so position is kept), `remap_array_fields` `:176` (array-valued
  only). `with_id_source` is the deterministic counter.
- Consumers: the restore orchestrator
  `services/backup/restore/orchestrator.rs:131-160` (new-account pre-step) —
  the only production caller; `p4_9g6_seam_contract.rs:163` (composition smoke).
- **Family:** `crates/quilltap-harness/tests/backup_uuid_remap_equivalence.rs`
  (412 lines, **tier-1 exact, no normalization**, recipe in the oracle case
  header): compares all 39 collections byte-level (`COLLECTIONS` `:68-108`),
  the key set, `getMapping()` entry-for-entry in order, `getSize()`. Both sides
  mint `00000000-0000-4000-8000-<counter>`; the NDJSON lines carry
  `corpusSha256` and the test refuses a stale corpus. Oracle +
  corpus generator: `harness/oracle/cases/backup-uuid-remap.test.ts` (1,350
  lines; jest; `QT_CORPUS_OUT` writes
  `harness/oracle/fixtures/uuid-remap-corpus.json` — **41 cases, 6,011 lines,
  210 KB**; hand-authored cases are `{name, note, targetUserId, data: bag({...})}`
  — see `chat_scenario_source` `:842-882`, `chat_*_watermark` `:780-840`). The
  shared recipe: `QT_FIXTURE_SD_{MAIN,MOUNT,LLM}=…system-data-*.db`,
  `QT_CORPUS_OUT`, `QT_ORACLE_OUT`, header `:36-55`.
  **What the corpus holds for these keys (measured):** `turnQueue` `"[]"` ×2 and
  `cycleOrderParticipantIds` `"[]"` ×2 (both chats of `wide`), **and
  `spokenThisCycleParticipantIds` `"[\"e1000000-0000-4000-8000-000000000001\"]"`
  once** — on `wide` chat `c1000000-…001`, naming that chat's own participant
  `e1000000-…001` — plus `[]` once; `targetParticipantIds` appears NOWHERE.
- **Restore families that run `new-account`** (committed archives
  `crates/quilltap-web/tests/fixtures/restore-archives/*.zip`, **42 archives**,
  all derived from `restore-archive.zip`): measured with `unzip -p …
  '*/data/chats.json'`, **every archive that has a chats list carries
  `spokenThisCycleParticipantIds: ["e1000000-0000-4000-8000-000000000001"]` on
  chat `c1000000-…`** (~40 of 42). `system_restore_state` runs 17
  `new-account` cases (`harness/oracle/cases/system-restore.test.ts:200, :215,
  :217, :257, :263, :292, :431, :618-765`), and `restore_vintage_state.rs:337`
  runs each of its archives in `newacct` mode too.

### B2.2 describe_outfit

- `crates/quilltap-core/src/wardrobe.rs`: hair constants `:144-169`
  (`HAIR_SLOT_GUIDANCE :157`, `HAIR_PHYSICAL_BOUNDARY :164`,
  `HAIR_PHYSICAL_DESCRIPTION_NOTE :169` — P4.D87, byte-checked into one block);
  `describe_outfit(slots)` `:309-311` is a thin wrapper over
  **`describe_outfit_with_omit(slots, omit)` `:328-384`**; the grouped-line emit
  is `:380-382` (`for (value, slots_for_value) in &groups.entries {
  lines.push(format!("- **{}:** {}", slots_for_value.join(", "), value)) }`) — **the
  only edit site** (the ledger's `wardrobe.rs:72-73` is stale). Unit tests that
  pin the hair line and WILL need the note: `describe_outfit_rule_13_styled_hair_
  blocks_the_full_collapse` `:1003-1014` (`... - **hair:** marcel waves\n`),
  `describe_outfit_renders_a_styled_hairdo_and_skips_a_blank_one` `:1017-1040`
  (`- **hair:** braided crown\n`); in `crates/quilltap-core/tests/
  unreported_if_blank_slots.rs`, `still_renders_the_hairdo_when_one_is_set`
  `:111-121` asserts `out.contains("- **hair:** marcel waves")` (still true) and
  the blank-hair tests assert NO hair mention (still true: no group).
- **v5 callers** (every one inherits the note): `tools/wardrobe_wear.rs:297`,
  `tools/wardrobe_take_off.rs:327` (the coverage summary — the tools' output),
  `services/image_scene_tasks.rs:408`, `services/resolve_equipped.rs:198`
  (re-homed by P4.D279), `services/avatar_prompt.rs:139,157` (via
  `describe_outfit_with_omit`), `services/appearance_resolution.rs:235`,
  `services/wardrobe_item_image_prompt.rs:126` (with omit),
  `services/aurora_notifications.rs:55,70,82,99` (four builders). No SPA surface
  renders it.
- **Which corpora can move** (measured only by "does the committed corpus put a
  non-empty hair item into the equipped outfit"; the lane MEASURES, this is the
  candidate list): `harness/oracle/fixtures/wardrobe-item-image-prompt.json`
  (11 `"hair"` refs; `wardrobe_item_image_prompt_equivalence`),
  `avatar-job.json` (scenario `clothed_with_hairdo` `:456`, hair items `:198/:207`;
  `avatar_job_tier3_equivalence`), `wardrobe-tools.json` (2;
  `wardrobe_tools_equivalence`), `wardrobe-item-image-generation-tier3.json`
  (1; `wardrobe_item_image_generation_tier3_equivalence`),
  `outfit-displacement-tier2.json` (1; likely no prompt text),
  `appearance_sanitize_gate_tier3_equivalence.rs` (2 `hair` mentions). **Does NOT
  move:** `post_office_aurora_equivalence` — its oracle case
  `harness/oracle/cases/post-office-aurora.ts` types `OutfitSlotValues` with FOUR
  slots (no `hair`), so the Aurora corpus has no hair row; `story_background_job`,
  `chats_outfits_tier2`, `outfit_change_effects_tier3`, `outfit_llm_choose_tier3`,
  `wardrobe_wear_commit_tier2` (0 hair refs in their `.rs`; fixtures 0).
  There is **no tier-1 oracle case that imports `describeOutfit` directly**.
- `help/wardrobe.md` (repo-root `help/`, embedded in the binary — the
  `p4.9i2` tree): needs the same three-line edit. `docs/v4/CHANGELOG*.md`
  re-vendor belongs to the docs lane.

### B2.3 CLI bytes

- `crates/quilltap-cli/src/help/main_help.txt:19`:
  `  anchor-probe <characterId>    Probe whether episodic anchor lines suppress Memory Gate reinforcement`;
  `help/completion/zsh.template:46`; `help/completion/fish.template:64`
  (`-d 'Probe anchor-line effect on Memory Gate'`); `bash.template` has no such
  phrase. `help/anchor_probe_help.txt` (captured at `83334b9a5`, P4.D299)
  ALREADY says "suppresses reinforcement at admission" (`:5-6`) — **not** a
  stale "Memory Gate" (§9.6). `main.rs:12-24` documents the capture provenance
  (`main_help.txt` re-captured byte-identical at `83334b9a5`) — the comment
  grows one sentence at this pin.
- **What pins the bytes:** `crates/quilltap-cli/tests/cli_differential.rs`
  (the env-gated Tier R: `QT_V4_CHECKOUT` + Node 24, byte-diffs v5's binary
  against **v4's live launcher**, not a stored oracle): `ctx.case("main help",
  &["--help"])` `:2458`, `ctx.case("completion zsh", …)` `:4819`, `ctx.case(
  "completion fish", …)` `:4820` (`bash` `:4818`, `help` `:4817`, no-args `:4816`,
  unknown-shell `:4821`). `completion_behavior.rs` `include_str!`s the three
  templates (`:26,:27,:300`) for the flag-coverage guards (`anchor-probe` row
  `:323`) — they read flags, not the summary string.
- `docs/v4/packages-quilltap-README.md` (43,699 bytes) is byte-equal to v4's
  `packages/quilltap/README.md` at `83334b9a5` (verified); the commit's diff is
  +14 / −4 (hunks listed in B1.3).
- **`migrations`**: no v5 implementation (B1.3); nothing to port.

### B2.4 memory-time

- `crates/quilltap-core/src/memory_time/` (`mod.rs` 42, `anchors.rs` 196,
  `calendar.rs` 521, `clock.rs` 767, `rules.rs` 621, `age_label.rs` 118):
  `clock.rs:111-132 month_index` (a CLOSED `[&str; 12]` + `position`), `:136-144
  unit_days` (closed `match`), `:149-168 word_number` (closed `match`) — **v5
  answers nothing for `constructor` on any of the three** (doc comments `:111-116`
  and `:146-148` already describe v4's behaviour and the closed lists; they will
  want a "v4 `25b8fab6a` converged" clause). `resolve_event_time_inner`
  `:245-380`: named dates `:300-345`, the ago arm `:342-367`, `resolve_when_phrase`
  `:387-396`. `anchors.rs:59-78 resolve_episodic_anchors` — NO catch (nothing can
  throw), takes the zone as a required `&str`. `calendar.rs:146 zoned_parts`:
  "an unknown zone [reads] as UTC, so a misuse degrades instead of panicking".
  Jest twins: `anchors.rs` `mod jest_twins` `:80+` (mirrors `anchors.test.ts` at
  `83334b9a5`), `rules.rs` `mod jest_twins` `:328+` (`write_only_rules_phrase_to_
  expected` `:495-498`).
- **The harness pin:** `crates/quilltap-harness/tests/memory_time_equivalence.rs`
  (631 lines): module doc `:23-24` ("... and the `PROTOTYPE_KEY_MONTH_THROWS`
  pin"); the const + its ruling doc `:307-317` = `["constructor 5", "5
  constructor"]`; `check_rules_clock_anchors` `:541+`: `throws_measured:
  BTreeSet<String>` `:553`; the "v4's throw rows: v5 answers nothing" branch
  `:564-586` (asserts v4's `out.throws` starts with `RangeError`, v5 =
  `EventTime::default()`, inserts `utc["phrase"]`); the final
  `assert_eq!(throws_measured, throws_table, "PROTOTYPE_KEY_MONTH_THROWS drifted
  — v4's throwing phrases")` `:618-625` and the `eprintln!` count `:626-630`.
  Header recipe `:27-34` (two legs: `TZ=UTC` and `TZ=America/Chicago`, appended,
  `QT_ORACLE_MEMORY_TIME`). Oracle case `harness/oracle/cases/memory-time.ts`
  (418 lines, `tsx`, pure): `attempt()` wraps `event` rows so a throw is recorded
  as `{ throws: "RangeError: Invalid time value" }` (`:175-181`); `EVENT_PHRASES`
  `:241-272` already contains `'constructor 5'`, `'5 constructor'` (`:269`) and
  `'constructor days ago'` (`:254`); the `anchors` rows (`:394-409`, 9 whens ×
  4 refs × 3 fallbacks × 2 modes × 4 narrative × 4 zones) are NOT wrapped in
  `attempt` and none uses a constructor phrase. The sweep driver parses the
  header recipe (memory `sweep-driver-recipe-header-rules`).

---

## B3. Tier split

| unit | tier | why |
|---|---|---|
| S2-U1 remap | **tier 1, exact** (pure) | `backup_uuid_remap_equivalence`; corpus grows. Plus the downstream restore families (tier 2/3 state diffs) go red-by-design and are re-measured. |
| S2-U2 hair note | tier 1 (new small direct case) + the tier-3 prompt families by composition | the leaf has no direct oracle pin today; add one. |
| S2-U3 CLI bytes | **Tier R** (live-launcher byte diff) | no oracle file; the diff IS the oracle. |
| S2-U4 bug 247 | tier 1 (the `memory_time` family) — retirement by measurement | no code change. |

---

## B4. Differential plan

### S2-U1 (seat ids in new-account restore)

Regenerate: `backup_uuid_remap_equivalence` (oracle + corpus together, one jest
invocation; the NDJSON `corpusSha256` restamps). **Expected red-first sequence
at the pin with NO port:** `wide` fails on collection `chats` (and the mapping
list: the participant `e1000000-…001` is now minted EARLIER, shifting every later
counter id) while the 40 hand cases stay green (their lists are absent or
`"[]"`). That red is the proof the corpus asks. (The ledger predicted a
byte-identical oracle — §9.4.)

**Port:** in `remap_chat`, after the `scenarioSource` arm (`:862`) and before the
participants: for each field in `["turnQueue","spokenThisCycleParticipantIds",
"cycleOrderParticipantIds"]`, `if let Some(v) = chat.get(field)` (key presence,
any value incl. `null`) → `remapped[field] = remap_json_id_list(v, r)`; where
`remap_json_id_list` returns the value CLONED unless it is a `String` that parses
as a JSON array whose every element is a string, in which case it is the
**compact** `serde_json::to_string` of `r.remap_array(...)` (`["a","b"]` — no
spaces; `serde_json::to_string` is compact). Messages: `&["attachments",
"targetParticipantIds"]`.

**New corpus cases** (`backup-uuid-remap.test.ts`; `bag({chats:[...]})` shape as
`chat_scenario_source`):
- `chat_turn_state_json_ids` — v4's own test 1 (two seats; `turnQueue
  '["seat-b"]'`, `spokenThisCycle… '["seat-a"]'`, `cycleOrder… '["seat-b","seat-a"]'`;
  a message with `participantId:'seat-a'`, `targetParticipantIds:['seat-b']`).
- `chat_turn_state_json_edges` — one chat per edge, each its own seat set:
  `'not json'` (v4 test 2), `''`, `'null'` (JSON null ⇒ non-array ⇒ untouched),
  `'{"a":"b"}'`, `'[1,2]'` (non-string elements ⇒ untouched), `'["a",1]'`, a
  PRETTY-PRINTED valid list `'[ "seat-a" , "seat-b" ]'` (⇒ re-serialised compact,
  whitespace dropped — the byte-level proof), a duplicate list `'["x","x"]'`, a
  list naming an id that is NOT a seat, a list whose FIRST mention of a seat id
  precedes the participants (the mint-order proof: the id's counter is lower than
  every participant's), the key present with a **non-string** value
  (`turnQueue: null`, `turnQueue: 5`, `turnQueue: ['seat-a']` as a real array —
  all untouched), and an ABSENT key (v4 test 2's `not.toHaveProperty`).
- `message_whisper_audience` — `targetParticipantIds`: `['seat-b']`, `[]`, an
  unknown id, `null`, `'seat-b'` (a string ⇒ untouched), absent; plus one message
  with BOTH `attachments` and `targetParticipantIds` (order within the message:
  `remapFields` first, then arrays in `['attachments','targetParticipantIds']`
  order ⇒ attachment ids minted before audience ids).
- Keep `wide` as the real-shape case (its populated `spokenThisCycle…` now
  exercises the path with real data).

**The restore families (re-measure, expect red-by-design):** after S2-U1 the
`system_restore_state` `*_new_account` cases (17) and `restore_vintage_state`'s
`newacct` runs consume `remap_backup_data` through the orchestrator; until the
port lands every one differs on `chats.spokenThisCycleParticipantIds` (v4 writes
the participant's NEW id, v5 the archive's old one). Regenerate those oracles
from the pin (`system-restore.test.ts`) FIRST so the red set is the true one,
then land the port and require them all green by name. Classify the red set in
the round record as a **union red caused by the baseline move**, not a lane
regression (memory `unify-review-finding-shapes`: "union reds").

### S2-U2 (hair note)

Add the constant next to the other three (`wardrobe.rs:144-169` block; keep the
byte transcription — em dash, straight apostrophe); in `describe_outfit_with_omit`
emit the note per v4: `let note = if slots_for_value.iter().any(|s| *s ==
"hair") { format!(" ({HAIR_STYLING_PRECEDENCE_NOTE})") } else { String::new() }`
then `format!("- **{}:** {}{}", slots.join(", "), value, note)`. Update the two
wardrobe.rs unit tests and add: a hair line sharing a value with another slot
(`- **accessories, hair:** …` with the note once); an empty hair slot (no note,
no line); an OMITTED populated hair slot (no group). Then:

- **New direct tier-1 case** `harness/oracle/cases/describe-outfit.ts` (`tsx`,
  pure, `@/lib/wardrobe/outfit-description`) + `crates/quilltap-harness/tests/
  describe_outfit_equivalence.rs`: the cross product `{top, bottom, footwear,
  accessories, hair}` ∈ {empty, one item, two items, a shared multi-slot value}
  × `omit` subsets — at minimum the hair-bearing rows (hair alone; hair + naked
  collapse; hair shares a value with accessories; hair omitted). This closes the
  fact that no oracle imports `describeOutfit` directly.
- **Add hair rows to `post-office-aurora.ts`** (extend `OutfitSlotValues` with
  `hair`) so the four Aurora builders are pinned with a styled hair slot.
- **Regenerate and read the red set** of the candidate prompt families (B2.2)
  FROM THE PIN before the port: expect `wardrobe_item_image_prompt`,
  `avatar_job_tier3` (`clothed_with_hairdo`), `wardrobe_tools`,
  `wardrobe_item_image_generation_tier3` to turn red on the hair-bearing rows
  only; every one must go green after the single `wardrobe.rs` edit — if a
  family needs a SECOND change, the survey missed a caller (and a mutation of the
  note to `""` must turn each of them red again).

### S2-U3 (CLI bytes)

Edit the three files byte-for-byte per the B1.3 table; update `main.rs`'s
provenance comment. No oracle regen: Tier R compares against the live v4
launcher, so at a v4 checkout at or past `75d094e63` the cases `main help`,
`completion zsh`, `completion fish` are RED until the edit and green after (the
`main help` / `completion` trio is the tripwire — the same pattern the sync
verb recorded at `cli_differential.rs:4833-4836`). Check `anchor-probe help`
(`:5643`) stays green (it already says "admission"). Re-vendor
`docs/v4/packages-quilltap-README.md` from the pin (14+/4−) as a riders-only
change (a docs lane may own it). `migrations`: ratify NO-PORT with the
`SUBCOMMANDS` / `not_yet_available` evidence above.

### S2-U4 (bug 247)

Regenerate `memory_time_equivalence`'s oracle from the pin (both TZ legs). The
**expected first red** is `PROTOTYPE_KEY_MONTH_THROWS drifted` with
`throws_measured` = ∅ (v4 no longer throws; the two rows are now ordinary
`{occurredAt:null, window:null}` rows and compare green) — the table trips by
design (ledger §5.4: retire by MEASUREMENT: run, see the empty measured set,
then retire). Retire: delete the const + its doc (`:307-317`), the module-doc
mention (`:23-24`), the throws branch (`:564-586`), `throws_measured` /
`throws_table` and the assert (`:553, :618-625`), and fix the `eprintln!` (`:626`).
**Grow the corpus** (`memory-time.ts` `EVENT_PHRASES` `:241-272`): add the five
new v4 phrases, in v4's order, after the existing `'constructor 5', '5
constructor'` pair (whose comment `// v4 throws: MONTH_NAMES is a plain object`
`:268` must be rewritten to "v4 `25b8fab6a` answers nothing"):
`'on constructor 5'`, `'constructor 5th'`, `'constructor 5, 2026'`, `'a
constructor ago'`, `'two constructor ago'`; each is run over the existing 5
anchors × 7 zones (the loop right after `EVENT_ANCHORS` `:273`) = 35 rows each. Add **anchors** rows with
`when: 'constructor 5'` (v4 post-fix ⇒ `occurredAt` = the fallback, `narrativeTime`
as the mode dictates): extend the `when` list at `:398` by that one phrase —
×4 refs × 3 fallbacks × 2 modes × 4 narrative × 4 zones. Because the `anchors`
loop is not `attempt`-wrapped, this row is only valid against a v4 at/after the
fix — which the pin is. v5 needs NO code change: its closed tables answer
nothing and `resolve_episodic_anchors` has no throw path; the grown rows prove it
(and the two `ago` phrases are accepted as vacuous, §9.7).

Jest twins to add (v5-native, case-for-case ports of v4's new tests):
`rules.rs` `jest_twins` — an `it.each`-style test over the seven phrases
(`a_word_naming_an_inherited_property_is_not_a_month_or_a_number`) asserting
`EventTime::default()`; `anchors.rs` `jest_twins` — the second v4 test
(`constructor 5` ⇒ the fallback through the real parser). v4's first test (a
mocked parser throw, `jest.isolateModules`) has **no Rust analogue — NO-PORT**,
recorded.

---

## B5. Fixtures

- `uuid-remap-corpus.json` is REGENERATED by the oracle (committed); the NDJSON
  is `/tmp`. The committed restore archives need NO rebuild — their
  `spokenThisCycleParticipantIds` already exercises the new path; only the
  oracles regenerate.
- `memory-time` has no fixture (pure); `describe-outfit` is pure.
- Do NOT rebuild the `system-data-*.db` fixture family to "add a hair item" for
  the wardrobe families — extend via the spec/prelude route the family already
  uses (memory `shared-fixture-rebuilds-and-jest-setup-mocks`: recipes that
  rebuild a shared /tmp fixture re-mint ids per oracle).

---

## B6. Traps

1. **JSON.parse vs serde_json.** The only documented acceptance gap that can
   reach this path is V8 accepting a lone-surrogate `\u` escape that serde
   refuses (`jsstr.rs:195-200`, "the family's recorded divergence class").
   `'["\ud800"]'` is a valid all-string array to v4 (⇒ remapped), a parse error
   to `serde_json::from_str` (⇒ untouched in v5). No real seat id can be a lone
   surrogate; default: ignore and RECORD (§B7.1). Everything else (whitespace,
   nesting, numbers, `null`, duplicate keys in an object — irrelevant here)
   agrees. Re-serialisation is `JSON.stringify` of strings that are now ASCII
   uuids ⇒ `serde_json::to_string` compact is byte-identical; do NOT use
   `to_string_pretty`.
2. **Mint order is byte-observable** (tier 1, counter ids): the three lists are
   walked in the fixed field order, in list order, BEFORE participants and
   messages. Putting the loop after the participants passes every hand case with
   disjoint ids and fails `wide` and the "first mention precedes participants"
   row — that row is the guard.
3. **Key presence, not truthiness:** `field in chat` ⇒ `chat.get(field).is_some()`
   (so `null` counts, and is left `null`); do not use the module's `is_truthy`
   (the scenarioSource arm's guard) here.
4. **Vacuous equal (memory `tier-r-vacuous-equal-rows`):** a malformed plant makes
   both sides fail identically. Every new corpus row must self-check its own
   shape in the generator (assert the string it meant to plant, e.g. that the
   `'[ "a" , "b" ]'` row really has the spaces) — and each case must show a
   non-zero mapping delta somewhere, or it proves nothing.
5. **The restore red set is large and benign** (§B4): ~17 `system_restore_state`
   cases + the vintage `newacct` runs, one cell each. Classify, do not "fix" the
   Rust side back (CLAUDE.md: v4 drift tripwire "as designed").
6. **Hair note: bytes.** U+2014 not U+2013/`--`; the apostrophe is U+0027 inside a
   double-quoted JS literal — in Rust use a plain `"…"` literal with the
   characters, NOT an `r"…"` raw string with an escaped quote, and check with a
   source-census-style byte pin (the sibling constants are "byte-checked into one
   block", P4.D87). Mutation-prove: change one char of the constant and require
   a red.
7. **`any(|s| s == "hair")` on the group's slot list, not on the value.** A hair
   VALUE that happens to equal another slot's fallback text (e.g. "barefoot")
   would merge groups; the rule keys on slot names exactly as v4's
   `slotsForValue.includes('hair')`.
8. **Trailing newline / `.trimEnd()` callers:** `item-image-prompt.ts:78`
   `.trimEnd()`s; v5's `wardrobe_item_image_prompt.rs:126` uses `js_trim_end` —
   unchanged by the note (the note is mid-line).
9. **Tier R is env-gated and SKIPs green** (memory `oracle-runner-and-skip-traps`
   point 2/4): a Tier R run without `QT_V4_CHECKOUT` + `QT_NODE` reports `ok`
   after a `SKIP`. Run with `--nocapture` and look for the case names
   (`main help`, `completion zsh`, `completion fish`) before trusting a green or
   a mutation red; a suspiciously fast green is a SKIP.
10. **`memory_time` anchors rows depend on the fix.** The `anchors` loop is not
    wrapped in `attempt`; if the oracle is ever run against a v4 BEFORE
    `25b8fab6a` the new `constructor 5` row crashes the whole case (an uncaught
    `RangeError`), not just a row. The regen recipe must run from the pin
    worktree (memory `sweep-driver-spaced-path`: pass `--v5w ~/source/quilltap-v5`).
11. **`ZONELESS` / two-leg discipline unchanged:** new `event` rows are
    zone-explicit; rows must be present on BOTH legs (the test requires both).
12. **Out-of-contract zone in `resolveEpisodicAnchors`** (B7.2) — do not add an
    invalid-zone anchors row without a ruling.

---

## B7. Contract candidates and small rulings

**No contract candidates in this lane** — one private helper in `uuid_remap.rs`,
one `pub const` in `wardrobe.rs`, three text edits, and test retirements. No
signature crosses a lane.

Rulings the lanes will want (small, defaults stated):
1. *Lone-surrogate seat ids in the JSON lists* (B6.1): default **record as a
   divergence class, no row** (or one ruled row `JSON_LONE_SURROGATE_ID` if a
   unifier insists on a pin). Not worth a tolerant parser.
2. *Out-of-contract zone*: v4's new catch means `resolveEpisodicAnchors({ when:
   'yesterday', timeZone: 'Mars/Olympus', … })` returns the FALLBACK (the
   `RangeError` from `formatToParts` is swallowed), while v5's `zoned_parts`
   reads an unknown zone as UTC and RESOLVES. No production path reaches it
   (`run.ts:308` passes a validated clock zone; the extraction `clock` is built
   through `getMemoryTimeZone`). Default: **no row, no change**; if the human
   wants the divergence on record, add it to the `MEMORY_ZONE_VALIDITY`-style
   ruled tables.
3. *File the v4 oddity from A6.10* (lone user-seat decision loses its seat id)
   upstream — the port's standing practice; not a v5 change.
4. *`restore-field-fidelity.test.ts`*: tests-only; default **no v5 port**, but
   one cheap v5 audit row is allowed: a merge-mode restore of a chat carrying
   `cycleOrderParticipantIds` + a message `routeTrail` round-trips (the carrier
   families likely already cover it — measure before adding).

---

# §9. What the ledger rows got wrong or missed (hunk vs row)

1. **`6069a7acd`.** (a) "The turn-orchestrator oracle family and its log-line
   capture move": the family has **no v4 log-line capture at all** (A2.1) — the
   INFO line is a NEW pin, literal in-crate, and the siblings it would sit
   beside are v5-literal, not oracle-recorded. (b) The family cannot express the
   guard: the oracle case calls `shouldChainNext` with 5 args (no `config`, no
   `guards`) and the Rust op enum has no guards field — **both sides must grow
   `neverPauseForUser`** or the autonomous-room leg (v4 test 4) is untestable.
   (c) Missed: the early return sits **before** the user-driven-seat arm, so a
   lone user-controlled seat that spoke last now answers `user_turn` without
   `participantId` / `characterName` (A6.10) — a behaviour the ledger's "adds ONE
   early return" framing hides. (d) Path: `select_speaker.rs` is
   `crates/quilltap-core/src/select_speaker.rs`, not `services/`. (e) The ledger
   is right that the selector is unchanged and that this is not a convergence.
2. **`b82037d75`.** (a) Counts "three new DEBUG lines (one bare `Creating
   conversation chunk`)" as a port — the third is a repository-layer line from
   the bare root logger; the standing convention (`embedding_status.rs:266/313`,
   `fallback.rs:847-848`) is comment-only. (b) Missed the **fallback-read class**:
   v4's pre-write `findByInterchangeIndex` is a fallback read (inner `Error
   finding entity by filter` ERROR, then `null` ⇒ queued); v5's old read sat
   inside the caught block, so lifting it with `?` converts a read failure from
   a caught WARN-and-continue into a job failure. (c) "any production-SQL-capture
   plan that pins the render job's statement ORDER": there is none (grep).
   (d) Missed that `log_tests::a_render_logs_v4s_success_line` asserts **no DEBUG**
   (`:388`) and will trip on the new unconditional line. (e) The ledger's line
   numbers `:166-186` / `:217-262` for the upsert loop / `enqueue_embeddings`
   are `:162-186` / `:217-262` at main. (f) Correct: the repository half is
   already behaviourally in place (`upsert(input, new_id, now)`); the state diff
   in `embedding_remainder_equivalence` is unchanged by the fix (jest is
   in-process) — the new rows are log-capture rows, and the family had none.
3. **`01f508d4e`.** (a) "v5's transcript projection … carry the pre-commit lines
   where ported": v5's transcript projection carries **neither** the removed
   entry line nor the surviving `Projected chat transcript` exit line — nothing to
   remove. (b) The mailbox rename touches **five** pin sites, one of them an
   **oracle-side exact-name spy** (`orchestrator-tier3.test.ts:756`) the ledger
   does not mention; the per-letter WARN keeps its old text. (c) `[Sync] Reading
   disk file` is pinned by ONE in-crate unit test and no oracle comparison.
4. **`49f8906c3`.** (a) **"the oracle output is BYTE-IDENTICAL for the current
   corpus (an empty list remaps to `[]`)" is FALSE:** the `wide` case carries a
   populated `spokenThisCycleParticipantIds` (`["e1000000-0000-4000-8000-
   000000000001"]`) naming its own participant; v4 now remaps it and mints that
   id earlier, so `wide`'s `chats` and mapping change — the family goes red on
   the existing corpus by design. (b) **Missed the restore blast radius:** all
   ~40 committed restore archives carry the same populated list, so ~17
   `system_restore_state` `new-account` cases (+ the vintage `newacct` runs) go
   red on one cell each until the port lands (B4). (c) `restore-field-fidelity.
   test.ts` is **+107 appended to an existing file**, not "NEW, 107 lines" (hunks
   at `@@ -226 / -247 / -268 / -893`). (d) The `scenarioSource` remap is
   `:348-350` at the pin (the row says `:349-351`); the JSON-list loop `:352-361`.
   (e) The corpus holds `turnQueue "[]"` ×2 and `cycleOrderParticipantIds "[]"`
   ×2 as the row says — correct — but the row never mentions the populated
   spokenThis cell that falsifies its conclusion.
5. **`7458ff94e`.** (a) The edit site is `describe_outfit_with_omit`
   (`wardrobe.rs:328-384`, emit at `:380-382`), `describe_outfit` is a wrapper;
   the row's `wardrobe.rs:72-73` is a stale line reference. (b) "the tier-1
   `describe_outfit` family … direct oracle pins": **no oracle case imports
   `describeOutfit`** — the direct pins are v5-literal unit tests
   (`wardrobe.rs:1003-1040`, `tests/unreported_if_blank_slots.rs`); the only
   oracle touching it is `post-office-aurora.ts`, whose corpus has **no hair slot**
   (so Aurora does NOT trip unless hair rows are added). (c) The list of
   tripping callers is right; the corpora that actually hold a hair item are
   `wardrobe-item-image-prompt`, `avatar-job` (`clothed_with_hairdo`),
   `wardrobe-tools`, `wardrobe-item-image-generation-tier3` (B2.2) — measure.
6. **`75d094e63`.** (a) "`help/anchor_probe_help.txt:7` ALSO says 'Memory Gate'":
   **it does not** — the file (captured at `83334b9a5`, P4.D299) already says
   "reinforcement at admission" (`:5-6`); there is no stale-help action. (b)
   "re-capture + Tier R regen": there is **no regen** — Tier R diffs v5 against
   v4's LIVE launcher; the three red cases are `main help` (`cli_differential.rs:
   2458`), `completion zsh` (`:4819`), `completion fish` (`:4820`). (c) The
   `migrations` NO-PORT is confirmed: `SUBCOMMANDS` lists it (`main.rs:66`),
   `match sub_name` has no arm, `not_yet_available` exits 1. (d) The README mirror
   (`docs/v4/packages-quilltap-README.md`) is byte-equal to `83334b9a5`'s (verified);
   the diff to the pin is +14 / −4.
7. **`25b8fab6a`.** (a) "grow the corpus with v4's five new phrases (the `ago` arm
   through `UNIT_DAYS` / `WORD_NUMBERS` included — confirm v5's tables answer
   nothing for each)": the five are the table's 5 NEW phrases (7 total, 2 already
   in the corpus); **two of them (`a constructor ago`, `two constructor ago`)
   match no pattern at all** (the ago regex needs a unit word), and the `ago`
   arm's `lookupWord` reads are **unobservable** — pre-fix `constructor days ago`
   already resolves to null (`Object > 0` is false) and `UNIT_DAYS` is looked up
   with a regex-fixed key. Only the month-shaped phrases ever threw; `constructor`
   is the only reachable inherited key. The two `ago` rows are vacuous by
   construction (add them anyway — they are v4's table — and say so). (b) "the
   anchors catch has no v5 analogue (no throw path)": true for the corpus, but
   v4's catch is wider than the cause — an invalid zone string would now fall to
   the fallback in v4 while v5 resolves in UTC (B7.2). (c) The rows' `trips by
   design` claim is confirmed mechanically: `throws_measured` empties and the
   `PROTOTYPE_KEY_MONTH_THROWS drifted` assert is the first red.

---

*Survey file only. No source, test, fixture, ledger or CLAUDE.md edit was made
in producing it.*
