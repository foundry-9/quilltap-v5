# Survey — Deus Ex Machina (established facts) + the model-visible prompt-text moves + the Green Room spread

Read-only planning survey, **2026-11-04**. **Baseline `236c04e35`; the round
PINS at v4 `624d0d25c`** (`main`, `4.13.0-dev.8`). Every v4 line number below
is AT THE PIN, read through `git show 624d0d25c:<path>` / `git diff
236c04e35 624d0d25c` / `git show <sha>` (the live v4 working tree carries
unrelated uncommitted optimizer edits and was never read). Every v5 line number
is at v5 `main` `ff1651177` (core 0.0.1427, host 0.0.223, web 0.0.256, cli
0.0.38, harness 0.0.1110, SPA 0.5.971). Nothing is taken from commit prose:
each claim comes from a shipped hunk. Where a hunk disagrees with the drift
ledger's row, the correction is written out (§0 and §1).

**Commits in scope (sha — subject — v4 `dev.N`):**

| sha | subject | proposed lane |
|---|---|---|
| `db6190b44` | salon: Deus Ex Machina — established facts (`4.12.0-dev.15`) | **DEUS-READ** (model-facing reads) + **DEUS-WRITE** (posting + the job); SPA → **SPA-DEUS** |
| `df8d4d70a` | salon: Deus Ex Machina toggle on the composer portrait; Quilltap tooltips in the composer (`dev.17`) | SPA-DEUS (toggle + icon); the tooltips / aria-labels → the round's WCAG / composer SPA lane |
| `671c59a53` | salon: one predicate for never-collapsed announcements (`dev.16`) | **NO-PORT for the fix** (v5 already has one predicate); the fact exemption rides SPA-DEUS |
| `ba6c8fe0f` | prompt-trust: retire the slips ledger, record repairs ("the probation") (`dev.27`) | **PROMPTS** (memory + context + catalogue); its `character-field-semantics.ts` half → the character-kind generators lane (§7) |
| `c69a08882` | feat: group scenes need not agree; informs carry feelings (`4.13.0-dev.7`) | PROMPTS |
| `7ff5fda89` | fix: story characters no longer invent where the user is (`dev.8`) | catalogue half → PROMPTS; field-semantics bullet → the character-kind lane (it sits INSIDE `3ae4cf582`'s new constant) |
| `d0201575a` | feat: roleplay and assistant sample prompts; story prompts match the stakes (`dev.5`) | PROMPTS |
| `4efea65e5` | wardrobe: Green Room multi-slot picks fill every slot they cover (`dev.29`) | PROMPTS (rider) |

The schema move `chat_messages."isEstablishedFact"` (the 22nd D23 move,
`add-established-fact-field-v1`) is the keystone / schema survey's. This
survey only CONSUMES it: every lane here assumes the row JSON carries
`isEstablishedFact` (bool or null) on read and accepts it on write.

## 0. Headline list — what a planner must not miss

1. **"Deus Ex Machina" must never reach a model.** v4's one module
   (`lib/chat/established-fact.ts:1-170`) holds every model-facing word
   (three frames, the reminder header, the transcript rule, the speaker label
   `Established fact`). v4 ships a test that scans every exported string and
   frame output for the UI name (`__tests__/unit/lib/chat/established-fact-leaks.test.ts`,
   329 lines). v5 needs a census guard of the same shape, beside the
   existing census guards — the label lives only in the SPA
   (`system-message-labels.ts` `'established-fact': 'Deus Ex Machina'`, the
   slash registry's `deus.label`).
2. **The frame is applied at READ time on ELEVEN read paths, never stored.**
   The stored body is what the operator typed. A port that frames once at
   write time will pass a narrow test and break edits, re-voicing and the
   opaque swap. The read paths (each with its v4 line and v5 home in §2):
   `buildMessageContext` (host or plain voice by chat-wide opacity), the
   new user message on the posting turn, the trailing reminder, the history
   access cut and the presence windows (exemptions, not frames), the summary
   fold, `toVisibleConversationMessage` (cheap-LLM transcripts AND the
   compression window), the extraction stretch window, the extraction turn
   transcript's opener, the Courier delta, and the in-scene rehearsal.
3. **`toVisibleConversationMessage` now keeps a machine fact as a `user`
   line** (`chat-tasks.ts:289-290`, before the role check). This is also the
   compression window's counting rule (bug 265,
   `context_compression::verbatim_window_start`), so a machine fact now
   COUNTS toward the verbatim window and shifts it. The ledger row does not
   mention this. `context_compression_equivalence` and
   `build_context_tier3` move on any corpus row that holds a fact.
4. **`ba6c8fe0f` is not prompt text only (ledger correction).** It also
   ships:
   - a consolidation output key `resolvedIds` (`consolidation-tasks.ts:91`)
     with a new WARN;
   - a plan field `resolvedMarks` (`consolidation-plan.ts:120, :455-468`)
     that WRITES `keywords` on existing memory rows during consolidation
     (`consolidation.ts:523-528`; the dry-run corpus `maintenance/consolidate.ts:186`);
   - a recall multiplier `resolved: 0.6` (`recall/tags.ts:226`), a free
     keyword `RESOLVED_KEYWORD = 'resolved'` (`:235`) and
     `resolvedMultiplier` (`:588-597`) joined LAST into the product
     (`:672-684`);
   - a new key in the strict tuning-override schema (it is derived from
     `RECALL_MULTIPLIERS`' keys, `recall/tuning.ts:77-79, :96`), so the
     replay route's `tuning` echo and its unrecognized-key 400 move;
   - the `Cluster consolidated` DEBUG gains `resolved` (`consolidation.ts:823`).

   ⚠ **Data consequence:** ANY existing memory whose free keywords already
   contain `resolved` (trimmed, case-insensitive) ranks ×0.6 from the first
   recall at the pin. Extraction writes free-form keywords, so this is
   plausible on Friday. Measure the count on the copy (§11).
5. **The prompt-template prune DELETES a row on existing instances, and v5's
   seed is LAZY.** The ledger says "every-boot seed … the first v5 boot of a
   Friday copy … removes it". That is false for v5. The seed runs only from
   the template reads (`api/prompt_templates.rs:359-385`, called at `:408 /
   :503 / :541 / :632`); the module doc at
   `services/builtin_prompt_templates.rs:8-14` records that v4 has no startup
   call either. So the prune fires on the **first template read**.
   - v5's read-side probe `needs_seeding` (`builtin_prompt_templates.rs:163-178`)
     answers `false` once all shipped rows are current. A retired built-in
     row alone would NEVER reach the writer. The probe must also answer
     `true` when a built-in row's `name` is not in the catalogue.
   - v4 runs `pruneRetiredBuiltIns` on EVERY seed, so its DEBUG
     `Built-in prompt templates pruned {builtInCount, shippedCount, removed}`
     logs on every template read. Ruling R1 covers this.
   - **Measured:** live Friday is already pruned. v4 HEAD has run against
     it, so any copy taken now is a NO-OP for the prune; no FK references
     `prompt_templates` (v4's `sqlite-initial-schema.ts:485-500` has only
     `idx_prompt_templates_userId`); characters hold their own copies in
     `characters.systemPrompts`. The delete matters only for v5-only
     instances and pre-`d0201575a` backups. Every such instance seeded
     `MODERN General` from v5's vendored table.
6. **The prompt-template routes fixture plants every refresh-arm row under
   `collidingName: 'MODERN General'`**
   (`harness/oracle/fixtures/build-prompt-templates-routes-fixture.ts:60`).
   At the pin that name is RETIRED, so every P4.D237 built-in plant
   (`staleBuiltInId`, the seven `builtinPlantIds` incl. `duplicate`) would
   be PRUNED instead of refreshed. The P4.D237 pins would then silently
   measure the prune.
   - Re-aim `collidingName` to a shipped name (`MODERN Roleplay`).
   - ADD prune rows: a retired built-in, a retired built-in plus a USER row
     of the same name (the user row survives), and a shipped-name duplicate.
   - Also: `apps/web/e2e/prompt-templates-flow.spec.ts:29` `BUILTIN = 'MODERN General'`.
   - Also the core unit test `builtin_prompt_templates.rs`
     `the_catalogue_parses_and_carries_the_shipped_21` (21 → 22,
     `rows[20]` → `rows[21]`, the `MODERN General` assert).
   - `builtin_prompt_templates_guard.rs` reads the LIVE checkout via
     `common::v4_root`, so it is ALREADY red today (22 `.md` files at v4
     HEAD). That is v4 drift, not a v5 bug. Run it with `QT_V4_CHECKOUT=$PIN`.
7. **The three prompt extractors must be taught new interpolations — they
   abort loudly, by design.**
   - `harness/tools/extract-prompt-text.cjs:12` throws on any `${`. At the
     pin `FOLD_SUMMARY_PROMPT` interpolates `${ESTABLISHED_FACT_TRANSCRIPT_RULE}`
     (`chat-tasks.ts` fold prompt), a constant from ANOTHER module built with
     `'…' + "…"`.
   - `harness/oracle/scripts/extract-memory-task-prompts.py:85/115` aborts
     on the two new blocks `REPAIRS_INSTRUCTION_BLOCK` (`shared.ts:167`) and
     `ESTABLISHED_FACT_INSTRUCTION_BLOCK` (`shared.ts:189-190`, which
     interpolates the same cross-module rule).
   - The consolidation extractor needs no change: `ba6c8fe0f` adds only text.
8. **`4efea65e5` must land at TWO v5 sites, not one.** v4 has ONE
   `applyOutfitSelections` (`apply-outfit-selections.ts:246`), and every
   consumer goes through it: chat create (`chats/route.ts:1420/1432`),
   participant add (`participants.ts:212`) and merge (`apply-chat-merge.ts:247`).
   - v5 split the LLM-pick dissolve in two:
     - the create arm `services/outfit_selections.rs:1443`;
     - `resolve_llm_pick` (`:283`), used by `chat_merge.rs:482` and
       `chat_participants.rs:1516`.
   - The spread goes in both. `resolve_llm_pick` has no `chat_id` for v4's
     DEBUG `{chatId, characterId}`, so thread it from the two callers.
9. **`671c59a53` is a v4 display defect that v5 never had (ledger:
   "measure whether v5 has the split check" — measured).** v5 has ONE
   predicate, `isAnnouncementChip`
   (`apps/web/src/app/chat/chat-view-model.ts:293-299`), and `message-row.ts:139-145`
   renders the exempt rows' bar static. The only port is the fact exemption
   (`announcement-render-items.ts:52`), which belongs with SPA-DEUS.
10. **`7ff5fda89`'s field-semantics bullet (`character-field-semantics.ts:132`)
    sits INSIDE `3ae4cf582`'s new `STAKES_AND_REACTION_DIRECTION` (`:126`).**
    It cannot be ported without the character-kind programme.
    `generators/field_semantics.rs` is a GENERATED file
    (`harness/oracle/tools/gen-field-semantics.mjs` over
    `harness/oracle/cases/generators-field-semantics.ts`). One lane must
    regenerate it once, from the pin, carrying `ba6c8fe0f` + `3ae4cf582` +
    `7ff5fda89` together. Recommend the character-kind generators lane (§7).
11. **Rulings owed (§10):**
    - R1: the prune DEBUG on v5's read-only probe path.
    - R2: `speaker_label`'s signature, which collides with the Brahma-lookup
      lane.
    - R3: the send-schema refine 400 shape.
    - R4: the `isEstablishedFact` wire key on the transcript projection.
    - R5: confirm the CandidatePass enum insertion position.

## 1. Per commit — the shipped hunks (what each DOES)

### 1.1 `db6190b44` — Deus Ex Machina: established facts (94 files, +2,500 / −93)

Server production hunks only (tests, docs, CSS and SPA are listed in §6).

**(a) The one module — NEW `lib/chat/established-fact.ts` (170 lines at the pin; pure, client-safe, no logger).**
- Constants:
  - `ESTABLISHED_FACT_KIND = 'established-fact'` (`:32`);
  - `ESTABLISHED_FACT_SPEAKER_LABEL = 'Established fact'` (`:35`);
  - `ESTABLISHED_FACT_SEAT_FRAME` (`:38-39`) = `(Established fact: what follows is true in this story and known to everyone; it needs no verification.)`;
  - `ESTABLISHED_FACT_HOST_FRAME` (`:42-43`) = `The Host makes it known, as established fact (true in this story, known to everyone, needing no verification):`;
  - `ESTABLISHED_FACT_PLAIN_FRAME` (`:46-47`) = `Established fact (true in this story, known to everyone, needing no verification):`;
  - `ESTABLISHED_FACT_REMINDER_HEADER` (`:50-52`) = `Established since you last spoke: true in this story, already known to you, needing no verification, and outranking any older memory or record that disagrees:`;
  - `ESTABLISHED_FACT_TRANSCRIPT_RULE` (`:58-60`) = `Lines labelled or marked as established fact are settled truth in this story. Carry them forward as facts of the world, not as anyone's claim, and do not let a later line's doubt undo them.`
- Predicates:
  - `isEstablishedFactAnnouncement` (`:74-76`: `systemSender === 'host' && systemKind === 'established-fact'`);
  - `isEstablishedFactSeatLine` (`:79-81`: `isEstablishedFact === true && !systemSender`);
  - `isEstablishedFact` (either);
  - `establishedFactKindOf` → `'announcement' | 'seat' | null`. The announcement is tested FIRST.
- Frames:
  - `frameEstablishedFact(body, {kind, voice})` (`:100-110`) = `${opener}\n\n${stripEstablishedFactFrame(body)}`. A seat line ignores `voice`.
  - `stripEstablishedFactFrame` (`:118-123`) removes a leading `${opener}\n\n` of any of the three openers, which makes framing idempotent. ⚠ It also strips an operator body that happens to START with an opener plus a blank line — that needs a corpus row.
  - `framedEstablishedFactBody(m, voice)` (`:129-136`) frames `content ?? ''` and never `opaqueContent`.
  - `frameEstablishedFacts(messages, {voice})` (`:144-158`) puts the framed body in BOTH `content` and `opaqueContent` for an announcement, in `content` only for a seat line, and returns the same array object when nothing changed.
  - `countEstablishedFacts` (`:161-170`).

**(b) Posting a seat line.**
- `sendMessageSchema` (`orchestrator.service.ts:132`) gains `isEstablishedFact: z.boolean().optional()`. The `superRefine` (`:133-158`) adds two issues on `path: ['isEstablishedFact']`, both BEFORE the existing content/files/tools issue:
  - `An established fact is public; it cannot be whispered`, when `targetParticipantIds?.length > 0`;
  - `An established fact needs words`, when `content.trim()` is empty.

  Zod collects ALL issues, so an empty fact with no files yields two issues.
- `request-helpers.ts:42` passes `parsed.isEstablishedFact || undefined`; `SendMessageOptions.isEstablishedFact` is at `types.ts:67`.
- `recordOperatorPost` (`operator-post.service.ts:54, :94-100`):
  - the USER row gains `isEstablishedFact: true` ONLY when set (spread; otherwise the key is absent and stores NULL);
  - the DEBUG `[OperatorPost] User message persisted` gains a third key, `isEstablishedFact: !!…`, on EVERY post — every existing capture of this line moves;
  - a NEW INFO `[OperatorPost] Operator line posted as established fact {chatId, userMessageId}`.
- Both callers thread the flag: the main path (`orchestrator.service.ts:647`) and the fair-rotation pause (`turn-holds.service.ts:184`).
- The posting turn's `newUserMessage` (`orchestrator.service.ts:772-777`) = `messageContentPrefix + (isEstablishedFact && content ? frameEstablishedFact(content, {kind:'seat', voice:'plain'}) : content)`. So the turn that posts the fact sees the plain seat frame AFTER the attachment prefix.

**(c) Posting a machine fact — the announcement route.**
- `insertAnnouncementSchema` (`app/api/v1/chats/[id]/schemas.ts:232-251`) gains the sender `{kind: 'fact'}`. Its new `superRefine` refuses `targetParticipantIds != null` with a fact sender (`path: ['targetParticipantIds']`, the same "public" sentence). ⚠ `!= null`, so even an EMPTY array `[]` is refused for a fact.
- `handleInsertAnnouncement` (`actions/announcement.ts:29-40`) runs a fact arm BEFORE the character check and the audience resolve: `postEstablishedFact({chatId, userId, contentMarkdown})`.
  - null → 400 `Failed to post established fact (empty content or unknown chat).`
  - otherwise INFO `[Chats v1] Established fact posted {chatId, messageId}` and **201** `{success: true, message}`.
- NEW `postEstablishedFact` (`lib/services/announcer/writer.ts:261-339`):
  - trims, and returns null on empty;
  - on an unknown chat: WARN `[Announcer] Established fact skipped — unknown chat {context:'announcer', chatId}`;
  - the row is `{type:'message', id, role:'ASSISTANT', content: trimmed, opaqueContent: trimmed, attachments: [], createdAt, participantId: null, systemSender:'host', systemKind:'established-fact', targetParticipantIds: null, customAnnouncer: null}`;
  - INFO `[Announcer] Established fact posted {context, chatId, messageId}`;
  - a catch → ERROR `[Announcer] Failed to post established fact {context, chatId, error}` + null;
  - then a lazy `enqueueEstablishedFactMemories`, with DEBUG `[Announcer] Established-fact memory job enqueued {context, chatId, messageId, jobId}` or WARN `[Announcer] Could not enqueue the established-fact memory job; the fact stands {…, error}`.
- (`3d39c303c` later moved both arms to the `created()` helper. **Verified byte-identical:** `created` = `NextResponse.json(data, {status: 201})` (`lib/api/responses.ts:207-209`), and both arms were already 201. The announcement half of that ledger row is NO-PORT, confirmed.)

**(d) The job — `ESTABLISHED_FACT_MEMORIES`.**
- `job.types.ts:42` puts the type right AFTER `CARINA_MEMORY_EXTRACTION`, which moves the `BackgroundJobTypeEnum` value list and any Zod enum error that prints it.
- Activity kind `memory` (`activity-kinds.ts:45`); the job topic shares the `MEMORY_EXTRACTION` arm (`realtime/job-topics.ts:80`); the handler is registered at `handlers/index.ts:63`.
- `enqueueEstablishedFactMemories` (`queue-service.ts:652-676`) dedupes against PENDING + PROCESSING jobs of this type with the same `messageId`. A failed check logs WARN `[EstablishedFactMemories] Failed to check for existing jobs during enqueue {error}` and falls through.
- The handler (`handlers/established-fact-memories.ts:65-164`):
  - chat missing → WARN `… Chat not found; skipping`;
  - message missing or not an announcement → WARN `… Fact message not found; skipping`;
  - an empty trimmed fact → silent return;
  - seats = `participants.filter(type==='CHARACTER' && controlledBy!=='user' && status!=='removed' && characterId)`. Absent and silent seats are INCLUDED.
- Per seat:
  - `findByIdRaw`; missing or archived → WARN `… Seat character missing or archived; skipping seat`;
  - otherwise `admitCandidate(MemoryCandidateSchema.parse(establishedFactCandidate(...)), {occurredAt: null, narrativeTime: null}, {userId})`;
  - outcome DEBUG `… Seat admitted the fact {…, memoryId, action}`, or WARN `… Seat did not admit the fact {…, reason}`;
  - a throw → WARN `… Admission failed for a seat; continuing`;
  - closing INFO `… Established fact admitted to the cast {jobId, chatId, messageId, seats, admitted, notAdmitted}`.
- The candidate (`:33-63`): `content = summary = fact`, `keywords: []`, `targeting {present, wide, information}`, `importance: 0.8`, `kind: 'semantic'`, `subject {holderId, aboutId: null}`, `provenance {source:'MANUAL', witnessedContext:'manual', chatId, projectId, sourceMessageId, anchorTimeMs: Date.parse(createdAt) or Date.now(), pass:'established-fact'}`.
- `CandidatePassEnum` gains `'established-fact'` between `'manual'` and `'consolidation'` (`lib/memory/candidate.ts:145`).

**(e) Reads — the context builder.**
- `buildMessageContext` (`context-builder.service.ts:1061-1078`): after the whisper filter and BEFORE `normalizeWhisperRoles`, `factVoice = isOpaqueAnywhere ? 'plain' : 'host'`. When any facts are present it frames them and logs DEBUG `[Context] Framed established facts {chatId, seatLines, machineFacts, voice}`. Then (`:1146-1155`) the ids of fact rows, taken from the PRE-normalization rows, mark `messagesWithParticipants` entries `isEstablishedFact: true`.
- `filterMessagesByHistoryAccess` (`message-attribution.ts:75-97`) keeps every fact row regardless of join time. A pre-join fact counts into `factsKept`, and DEBUG `[MessageAttribution] Kept pre-join established facts past the history-access cut {participantId, factsKept}` logs when > 0.
- `filterMessagesByPresenceWindows` (`:178`) keeps a fact unconditionally.
- **The trailing reminder:** NEW `lib/chat/context/established-fact-reminder.ts:63-115` `buildEstablishedFactReminder`.
  - On a swipe it cuts `events` at the first re-rolled message id.
  - `lastOwnMs = findLastOwnTurnMs(events, respondingParticipantId)`. The floor is `lastOwnMs ?? (hasHistoryAccess || !joinedAt ? null : Date.parse(joinedAt))`.
  - It keeps fact rows (both kinds), public or targeting this seat, with `Date.parse(createdAt) > floor`. With no floor it keeps all; a non-finite parse is DROPPED when a floor exists.
  - Speaker names are loaded only if a seat line is present. Items are `${speakerLabel(m, names)}: ${content}` for a seat line and the bare `content` for a machine fact.
  - **Cap 8000 UTF-16 code units** (`:42`) over `header.length + Σ(item.length + 2)`, dropping the OLDEST while `kept.length > 1`.
  - DEBUG `[EstablishedFact] Built the since-you-last-spoke reminder {chatId, participantId, selected, droppedByCap, isSwipe}`.
  - It returns `[HEADER, ...kept].join('\n\n')`, or `''` when there is nothing to remind.
- Called from `context-manager.ts:738-758` inside the `Promise.all` with progressions / inventory / referents / inform, gated on `respondingParticipant`. A failure logs WARN `[EstablishedFact] Could not build the reminder; the facts still ride in history {chatId, error}` and yields `''`.
- `turn-trailing.ts:51, :67`: the section sits right AFTER `inform` and before the referent ledger.

**(f) Reads — cheap-LLM transcripts and memory.**
- `toVisibleConversationMessage` (`chat-tasks.ts:289-290`, after the inform / off-scene / empty-content skips and BEFORE the role switch): a fact of either kind → `{role:'user', content: framed plain}`. The candidate type gains `isEstablishedFact` (`:264`).
- The fold prompt `FOLD_SUMMARY_PROMPT` gains `${ESTABLISHED_FACT_TRANSCRIPT_RULE} ` before `Output only the five sections…`.
- The summary fold (`context-summary.ts`):
  - `partitionMessagesIntoTurns` (`:272-276`) keeps a machine fact past the Staff skip and makes it START a new turn;
  - `turnsToChatMessages` (`:313-319`, now exported): a machine fact → role `user`, content UNFRAMED, speaker `Established fact`; a seat line → its speaker, content framed plain;
  - DEBUG `[Context Summary] Folding established facts {chatId, seatLines, machineFacts}` (`:423-430`).
- `speakerLabel` (`speaker-names.ts:69-77`) returns `Established fact` for a machine fact before the name lookup. The type widens to read `systemSender` / `systemKind`.
- Extraction:
  - `stretchWindow` (`extraction/window.ts:93`) frames either kind in the plain voice;
  - `buildTurnTranscript` (`turn-transcript.ts:303, :324`) frames a fact OPENER as a seat line, and the promoted user-controlled slice's `text` follows it;
  - the SELF / OTHER / episode bodies gain the block `ESTABLISHED FACTS\n${ESTABLISHED_FACT_TRANSCRIPT_RULE}` (`shared.ts:189-190`; `self.ts:91`, `other.ts:119`, `episode.ts:46`).
- The Courier delta (`courier-transport.service.ts:253, :347-360`): the speaker for a machine fact is `[Established fact]`, the content is framed plain for either kind, and DEBUG `Courier delta: framed established facts {chatId, framedFacts}`.
- In-scene rehearsal (`announcer/in-scene-voiced.ts:155-171`): machine facts pass the Staff drop, are re-roled `USER`, framed plain, and flagged so the history-access and presence filters keep them.

**(g) Carriers and wire.**
- `ChatMessageRowSchema.isEstablishedFact` (`chats-messages.ops.ts:77`, bool-or-0/1).
- `MessageEventSchema.isEstablishedFact` (`chat.types.ts:387`, between `isSilentMessage` and `systemSender`).
- The transcript projection (`transcript-projection.ts:239`, `event.isEstablishedFact || null` — so `false` projects `null`).
- Continue-elsewhere copies it (`apply-chat-continuation.ts:238`, `?? null`).
- `qtap-export.schema.json` +2.
- Prettify label `add-established-fact-field-v1` (`lib/startup/prettify.ts:132`), which belongs with the schema survey's boot ensure.

**Ledger row:** right on the outline. Corrections and additions:
- the compression-window count consequence (headline 3);
- the every-post DEBUG key change in `recordOperatorPost`;
- the posting turn's framed `newUserMessage`;
- `turn-transcript.ts` (the extraction opener) and `turn-holds.service.ts` (the pause path), both unlisted;
- the empty-array refusal in the announcement schema;
- the CandidatePass enum position;
- the reminder's 8000-unit cap and its swipe cut.

### 1.2 `df8d4d70a` — the composer-portrait toggle + composer tooltips (26 files, +667 / −369; SPA only)

- `SpeakingAsAvatar.tsx` (pin `:17-19, :63-82, :126-143`) becomes the
  established-fact toggle (`aria-pressed`, class
  `qt-chat-established-fact-toggle`), and initial-only portraits get a min
  width.
- NEW icon `public/images/icons/deus-ex-machina.svg` + the Madman's Box
  variant (`themes/bundled/madmans-box/icons/deus-ex-machina.svg`,
  `theme.json` 1.1.10) + the `icon-registry.ts` row.
- Native `title` attributes in `ChatComposer`, `ComposerGutterTools`,
  `FormattingToolbar`, `RngDropdown`, `CustomToolsButton` and
  `PendingInformChips` are replaced with `Tooltip`; icon-only buttons gain
  `aria-label`.
- No server hunk. Ledger row: correct.

### 1.3 `671c59a53` — one never-collapsed predicate (8 files, +84 / −23; SPA only)

`announcement-render-items.ts:50-56` adds `isAlwaysExpandedAnnouncement`
(Carina, established fact, Suparṇā `mail-delivery`, Pascal
`custom-tool-result`), and `isCollapsedAnnouncement` (`:65-68`) is now
exported. `VirtualizedMessageList.tsx` passes `isCollapsedAnnouncement`
instead of its own two-sender check — the v4 defect: a mail delivery or a
Pascal roll fell back to MessageRow's collapsed chip.

**v5:** `chat-view-model.ts:293-299` `isAnnouncementChip` is already the one
rule (P4.26 made the two exemptions kind-scoped), and the comment at
`message-row.ts:139-145` records that the exempt rows render a static bar.
**NO-PORT** for the fix. PORT only the `established-fact` exemption, inside
SPA-DEUS. **Ledger correction:** measured, no split check exists in v5.

### 1.4 `ba6c8fe0f` — "the probation" (52 files, +958 / −69)

- **Generators** — `character-field-semantics.ts`:
  - `TRUST_SAFEGUARDS_DIRECTION` (`:66`) "Five points" → "Six points", a sixth bullet (never tell `{{user}}` what they really meant…), and a fourth register example (`:79`);
  - `COMPANION_TRUST_DISPOSITION` (`:95-104`) is rewritten as six bullets; the "right as readily as they slipped" sentence is GONE;
  - `COMMITTEE_DRIFT_GUARDRAIL` (`:115-116`) gains the probation paragraph;
  - `EXAMPLE_DIALOGUE_COVERAGE` (`:184-190`) gains the plain-yes bullet and "most should not contain an objection".

  → v5 `generators/field_semantics.rs:43 / :80 / …` (GENERATED; §7 ownership).
- **Context** — `GROUP_SCENE_DISCIPLINE` (`context-builder.service.ts:833-841`):
  - "what the others said" → "what the other characters said";
  - the agree-then-add bullet is scoped to "with another character" and gains "a plain yes to the user … is a complete turn";
  - the speak-to-change list is reordered ("new information, an action actually taken, a joke, warmth, a question, a genuine objection, a refusal").

  → v5 `services/message_context.rs:1043-1050` (P4-era transcription of `e22f7b36`).
- **Extraction** — NEW `REPAIRS_INSTRUCTION_BLOCK` (`shared.ts:167-180`) interpolated in SELF (`self.ts:89`) and OTHER (`other.ts:117`), after AGREEMENTS and before ESTABLISHED FACTS. Plus:
  - the SELF 0.85 anchor (`self.ts:105-107`) gains "or made a repair";
  - the OTHER 0.85 anchor (`other.ts:136`) gains a repair sentence;
  - OTHER's skip list gains the kept-promises carve-out (`other.ts:105-108`);
  - the episode body gains one sentence (`episode.ts:44`).

  → v5 `memory_extraction/prompts/prompt_text.rs` (GENERATED; the extractor must be taught the new block — headline 7).
- **Consolidation:**
  - `ConsolidationOutputSchema.resolvedIds` (`consolidation-tasks.ts:91`, `.default([])`);
  - `validateConsolidationOutput` (`:168-181`) de-dupes the known handles (insertion order, `Set`) and logs WARN `Consolidation answer named unknown resolvedIds; dropped {dropped}`;
  - the bindery prompt gains two rules and a parenthesis, and the OUTPUT sample gains `"resolvedIds": []` (`:234-265`);
  - `planConsolidationWrites` (`consolidation-plan.ts:455-477`) builds `resolvedMarks`: per cluster, per `resolvedIds`, a member that is not a planned digest and not already marked; it is skipped silently if the keywords already contain `resolved` (exact, case-sensitive `includes`), else `keywords + 'resolved'`;
  - `executePlan` writes the keywords (`consolidation.ts:523-528`, DEBUG `Resolved grievances marked {characterId, count}`);
  - the outcome maps `resolvedIds` to ids (`:805`), the cluster DEBUG gains `resolved` (`:823`), and the maintenance corpus patches the keywords (`maintenance/consolidate.ts:186`).

  → v5 `consolidation_tasks.rs:134-170 / :342-370 / :373+` (validator), `consolidation_tasks/prompt_text.rs` (GENERATED), `consolidation_plan.rs:267-280 / :562 / :903-931`, `services/consolidation.rs:858 / :1499-1513 / :1632`, `services/memory_maintenance/consolidate.rs:349`.
- **Recall:**
  - `RECALL_MULTIPLIERS.resolved = 0.6` (`tags.ts:226`), `RESOLVED_KEYWORD` (`:235`);
  - `resolvedMultiplier` (`:588-597`) matches with `typeof k === 'string' && k.trim().toLowerCase() === 'resolved'` — note this is case-INsensitive, while the plan's skip is case-sensitive — and fires `resolved↓`;
  - `combineRecallMultipliers` appends it LAST to `parts` and to the product (`:672-684`). Float order: `… * fresh * resolved`.

  → v5 `recall_tags.rs:170-226` (table), `:229` `RECALL_MULTIPLIER_KEYS: [&str; 12]` → 13, `:824-880`, and `recall_tuning.rs:550-575` (the strict override arm).
- **Catalogue** — `qtap-plugin-default-system-prompts` 1.1.30: the ledger line removed from 18 files, the sixth rule added to all 21, an "ordinary time" default yes, and frequency counterweights → v5 `services/builtin_prompt_templates.json` (§1.7).
- **Ledger correction:** see headline 4.

### 1.5 `c69a08882` — informs carry feelings; the cast need not agree

- `INFORM_BLOCK_HEADER` (`lib/chat/context/inform-block.ts:67-70`) =
  `Things now true in this story and in you, as of this moment — what has happened, what you know, and how you feel or what you have resolved. Where any of it conflicts with an older memory, your records, or your usual temperament, this is the current truth:`
  → v5 `services/inform_block.rs:57-58` (P4.D254).
- `GROUP_SCENE_DISCIPLINE` gains `- The cast does not have to agree. If the characters before you took the measured view, yours may be fear, anger, or a decision to act. Never take a position only because everyone else has.` as the second-to-last bullet (`context-builder.service.ts:839`).
- Ledger row: correct.

### 1.6 `7ff5fda89` — the invented absence

- `character-field-semantics.ts:132`: one bullet inside `STAKES_AND_REACTION_DIRECTION` (a `3ae4cf582` constant).
- Plugin 1.1.33: one line in every story prompt, two in `MODERN_ROLEPLAY`.
- No code. Ledger row: correct, plus the dependency (headline 10).

### 1.7 `d0201575a` — roleplay / assistant samples + the PRUNE

- Plugin 1.1.32:
  - `MODERN_GENERAL.md` → `MODERN_ROLEPLAY.md` (display name **`MODERN Roleplay`**, category **`ROLEPLAY`**);
  - NEW `MODERN_ASSISTANT.md` (**`MODERN Assistant`**, category **`ASSISTANT`**);
  - a stakes passage in every companion / romantic / platonic prompt.
- The plugin's `index.js` reads `prompts/*.md` at runtime (`readdirSync(...).filter(.md).sort()`, pin `index.js:23989`), so the registry ships **22** prompts. The manifest's `promptCount: 21` is stale and read by nothing that matters.
- Sorted order at the pin: … `MISTRAL_ROMANTIC`, **`MODERN_ASSISTANT`**, `MODERN_PLATONIC`, **`MODERN_ROLEPLAY`**, `MODERN_ROMANTIC`, `OLLAMA_COMPANION`, `OLLAMA_ROMANTIC`.
- `prompt-templates.repository.ts`:
  - `_doSeedSamplePrompts` (`:49-75`): after the per-prompt upserts in the registry arm ONLY (`registryPrompts.length > 0`), `pruneRetiredBuiltIns(new Set(registryPrompts.map(p => p.name)))` (`:61`);
  - `pruneRetiredBuiltIns` (`:89-107`): `find({isBuiltIn: true})`; for each row whose `name` is not shipped, `deleteOne({id})` + INFO `Retired built-in prompt template removed {templateId, name}`; then ALWAYS DEBUG `Built-in prompt templates pruned {builtInCount, shippedCount, removed}`;
  - everything runs inside the seed's `safeQuery(…, 'Error seeding sample prompts')`.
- The filesystem fallback arm does not prune. In v5 that arm is NO-COUNTERPART already (`builtin_prompt_templates.rs` refusal 1).
- Ledger corrections:
  - the seed is lazy, not every-boot (headline 5);
  - name the probe gap;
  - categories `ROLEPLAY` / `ASSISTANT` are new free-form values: `PromptTemplateSchema.category` is `z.string()` (`template.types.ts:332`), so there is no enum to move.

### 1.8 `4efea65e5` — Green Room multi-slot spread (19 files; production = 3)

- NEW `spreadAcrossCoveredSlots(currentSlots, itemsById)` (`lib/wardrobe/slot-ops.ts:238-251`):
  - `slots = clone(currentSlots)`;
  - for each slot in `WARDROBE_SLOT_TYPES` order, for each id in the ORIGINAL `currentSlots[slot]`: when the item resolves, `addIdToSlot(slots, covered, id)` for every `slotsOf(item)` (the `types` filtered to wardrobe slots);
  - unresolvable ids stay as they are.
- `applyOutfitSelections` (`apply-outfit-selections.ts:199-201` `outfitSlotCount`; `:470-479`) runs the spread after `dissolveCompositesInSlots` in the LLM-pick arm only. DEBUG `[applyOutfitSelections] Spread multi-slot picks across every slot they cover {chatId, characterId}` logs only when the total placement count changed. `wornBundles` comes from the dissolve, untouched.
- `OUTFIT_SELECTION_PROMPT` (`cheap-llm-tasks/outfit-selection.ts:32`) gains one paragraph after the slot-names paragraph.
- v5:
  - `slot_ops.rs` (`slots_of` `:125`; an `add_id_to_slot` equivalent must be confirmed or added — v5 has `add_item_to_slot` `:418`, and the composite-free `addIdToSlot` semantics, append-if-absent, need checking);
  - `services/outfit_selections.rs:76-90` (the prompt const), `:1443` (create arm) and `:283` (`resolve_llm_pick`) — headline 8.
- The dialog's `add_to_slot` stays single-slot (v4 docblock); v5 `add_item_to_slot` is unchanged.

## 2. The v5 surface map (by `path:line` at `ff1651177`)

### 2.1 DEUS-READ

| v4 site | v5 home | what changes |
|---|---|---|
| `established-fact.ts` (whole) | NEW `crates/quilltap-core/src/established_fact.rs` (pure; beside `core_whisper.rs`) | constants + predicates + frames + `frame_established_facts` over `Value` rows |
| `buildMessageContext` frame pass `:1061-1078`, mark `:1146-1155` | `services/message_context.rs` — `compute_opaque_anywhere` `:1133`, `normalize_whisper_roles(&messages_after_whisper_filter, …)` `:1283` (frame BEFORE it), `build_conversation_messages` `:1371` (mark the mwp after) | frame + DEBUG + the mwp `is_established_fact` mark |
| `filterMessagesByHistoryAccess` `:75-97` | `message_attribution.rs:51-57` `HistoryMessage` (+`is_established_fact`) and `:86-101`; caller `services/build_context.rs:2709-2719`; `services/announcer/in_scene_voiced.rs:356` | keep facts; the DEBUG needs `participantId` — log at the callers or widen the pure fn (it is logger-free today) |
| `filterMessagesByPresenceWindows` `:178` | `message_attribution.rs:40-49` `AttributionMessage` (+flag), `:161-176` | keep facts |
| `buildEstablishedFactReminder` | NEW `services/established_fact_reminder.rs`; inputs from `core_whisper.rs:42-57` `WhisperEvent` (lacks `id` and `is_established_fact` — widen) + `find_last_own_turn_ms` `:132`; `js_date_parse_ms` for `msOf`; `jsstr` UTF-16 length for the cap | the section |
| `context-manager.ts:738-758` + `turn-trailing.ts:67` | `services/build_context.rs:3262-3277` (insert after `inform_block`), the reminder computed beside `build_referent_sections` `:3250` over `CadenceEvents` `:2902` (do NOT add a second transcript read — the `cadence_events_are_read_at_most_once` pin `:1616`) | trailing order |
| `toVisibleConversationMessage` `:289-290` | `chat_tasks.rs:62-79` `RawMessage` (+`is_established_fact`), `:101-…` | the frame arm + compression-window consequence (`context_compression.rs:114`) |
| `partitionMessagesIntoTurns` `:272-276` | `context_summary.rs:215-240` (`:233` Staff skip, `:237` `starts_new_turn`) | machine fact kept + starts a turn |
| `turnsToChatMessages` `:313-319` + the DEBUG `:423-430` | `services/context_summary.rs:628-660` | role / label / seat frame |
| `FOLD_SUMMARY_PROMPT` | `services/context_summary/prompt_text.rs:14-24` (GENERATED by `harness/tools/extract-prompt-text.cjs`) | regen + teach the extractor the interpolation |
| `speakerLabel` `:69-77` | `services/speaker_names.rs:117` (callers: `memory_extraction/window.rs:141`, `services/context_summary.rs:640`, `services/build_context.rs`) | label — **R2** |
| `stretchWindow` `:93` | `memory_extraction/window.rs:124-155` | frame |
| `buildTurnTranscript` `:303, :324` | `services/turn_transcript.rs:383-400` | opener frame + slice text |
| Courier `:253, :347-360` | `services/courier_transport.rs:348` (`resolve_speaker`), `:470-476` (push) | label + frame + DEBUG |
| in-scene `:155-171` | `services/announcer/in_scene_voiced.rs:294-330` | keep / re-role / frame / flag |
| extraction blocks | `memory_extraction/prompts/prompt_text.rs` (GENERATED) | **owned by PROMPTS** (one regen carries both commits) |

### 2.2 DEUS-WRITE

| v4 site | v5 home | what changes |
|---|---|---|
| `sendMessageSchema` `:132-158` | `api/types.rs:525-551` `Request::ChatSend` (+`is_established_fact: Option<bool>`, frozen file — additive); `api/engine.rs:2141-2175` (the refine arm); `api/chat_send.rs:25-38` `ChatSendRequest`; `services/orchestrator.rs:360-410` `SendMessageOptions` | flag + two refines — **R3** |
| `recordOperatorPost` `:54, :94-100` | `services/operator_post.rs:108-125` (params), `:272-292` (row + DEBUG) | flag on row, DEBUG key, INFO |
| callers `:647` / `turn-holds :184` | `services/orchestrator.rs:1910` (main) / `:993` (pause) | thread |
| `newUserMessage` `:772-777` | `services/orchestrator.rs:1966-1975` `final_user_message` | seat frame after the prefix |
| `insertAnnouncementSchema` + fact arm | `api/types.rs:4781-4788` `AnnouncerSenderWire` (+`Fact`), `api/chat_post_office.rs:204-266` `chat_announcement_post` (fact arm before the character check; `[]`-refusal; 201) | route |
| `postEstablishedFact` | `services/announcer/writer.rs` (beside `post_adhoc_announcement` `:144`, `post_inform_record` `:197`) | writer + lines |
| job type / kind / topic / registry | `services/job_runner.rs:168`, `api/system_data.rs:656`, `services/activity_kinds.rs:88`, `realtime/job_topics.rs:101`, `quilltap-host/src/spine.rs:3984-3991` (handler wrapper, the `CarinaMemoryExtractionHandler` `:3213-3222` precedent) | insert AFTER `CARINA_MEMORY_EXTRACTION` everywhere (order-bearing lists) |
| enqueue | `services/queue_service.rs:1153-1204` (`enqueue_carina_memory_extraction` precedent) | dedupe by `messageId` |
| handler | NEW `services/established_fact_memories.rs` over `services/admission/admit.rs:385` `admit_candidate` | per-seat admission |
| `CandidatePassEnum` | `memory_candidate.rs:176-215` (`CANDIDATE_PASS_VALUES` `[&str; 8]` → 9, the enum + `as_str`) | **R5** |
| transcript projection `:239` | `api/salon.rs:651` (beside `isSilentMessage`, `or_null`) | **R4** |
| continuation `:238` | `services/chat_continuation.rs:355` | `take("isEstablishedFact")` |

### 2.3 PROMPTS

| v4 site | v5 home |
|---|---|
| `INFORM_BLOCK_HEADER` `:67-70` | `services/inform_block.rs:57-58` |
| `GROUP_SCENE_DISCIPLINE` `:833-841` | `services/message_context.rs:1043-1050` |
| extraction bodies | `memory_extraction/prompts/prompt_text.rs` (regen; teach `extract-memory-task-prompts.py`) |
| bindery prompt | `consolidation_tasks/prompt_text.rs` (regen; `extract-consolidation-prompt.py` unchanged) |
| `resolvedIds` / `resolvedMarks` / execute | `consolidation_tasks.rs`, `consolidation_plan.rs`, `services/consolidation.rs`, `services/memory_maintenance/consolidate.rs` |
| `resolved` multiplier | `recall_tags.rs`, `recall_tuning.rs` |
| catalogue + prune | `services/builtin_prompt_templates.json` (regen via `harness/oracle/provision/dump-prompt-templates.ts` FROM THE PIN), `services/builtin_prompt_templates.rs:163-260`, `api/prompt_templates.rs:359-385` |
| Green Room spread | `slot_ops.rs`, `services/outfit_selections.rs:76-90 / :283 / :1443`, `services/chat_merge.rs:482`, `services/chat_participants.rs:1516` |

## 3. Schema / D23

Consumed, not owned: `chat_messages."isEstablishedFact"` (two shapes, schema
order between `isSilentMessage` and `systemSender`). The row read
`db/chats_messages_read.rs:91 / :154` and the writer
`db/chats_messages.rs:1081 / :1264` are the `isSilentMessage` precedent; they
belong to the schema lane. **Contract the schema lane must publish:** the read
emits `isEstablishedFact` as `Value::Bool` when the cell is 0/1 (absent when
NULL, mirroring `isSilentMessage` at `:154`), and `add_message` persists a
`true` key. DEUS-WRITE consumes both. No other D23 in this area. `resolved` is
a keyword VALUE (no schema). The prompt-template prune is a data delete
(headline 5).

## 4. Log lines (v4 text + keys + level → v5 home)

| level | v4 text | keys | v5 home |
|---|---|---|---|
| DEBUG | `[OperatorPost] User message persisted` | `{chatId, userMessageId, isEstablishedFact}` (**+key**) | `operator_post.rs:288-292` |
| INFO | `[OperatorPost] Operator line posted as established fact` | `{chatId, userMessageId}` | same |
| DEBUG | `[Context] Framed established facts` | `{chatId, seatLines, machineFacts, voice}` | `message_context.rs` |
| DEBUG | `[MessageAttribution] Kept pre-join established facts past the history-access cut` | `{participantId, factsKept}` | `message_attribution.rs` / callers |
| DEBUG | `[EstablishedFact] Built the since-you-last-spoke reminder` | `{chatId, participantId, selected, droppedByCap, isSwipe}` | new reminder |
| WARN | `[EstablishedFact] Could not build the reminder; the facts still ride in history` | `{chatId, error}` | `build_context.rs` |
| DEBUG | `[Context Summary] Folding established facts` | `{chatId, seatLines, machineFacts}` | `services/context_summary.rs` |
| DEBUG | `Courier delta: framed established facts` | `{chatId, framedFacts}` | `courier_transport.rs` |
| INFO | `[Chats v1] Established fact posted` | `{chatId, messageId}` | `chat_post_office.rs` |
| WARN / INFO / ERROR / DEBUG / WARN | `[Announcer] Established fact skipped — unknown chat` / `… Established fact posted` / `… Failed to post established fact` / `… Established-fact memory job enqueued` / `… Could not enqueue the established-fact memory job; the fact stands` | `{context:'announcer', …}` | `announcer/writer.rs` |
| WARN | `[EstablishedFactMemories] Failed to check for existing jobs during enqueue` | `{error}` | `queue_service.rs` |
| WARN ×4 / DEBUG / INFO | `[EstablishedFactMemories] Chat not found; skipping` · `Fact message not found; skipping` · `Seat character missing or archived; skipping seat` · `Seat did not admit the fact` · `Admission failed for a seat; continuing` · `Seat admitted the fact` · `Established fact admitted to the cast` | per §1.1(d) | new handler |
| WARN | `Consolidation answer named unknown resolvedIds; dropped` | `{dropped}` | `consolidation_tasks.rs` |
| DEBUG | `Resolved grievances marked` | `{characterId, count}` | `services/consolidation.rs` |
| DEBUG | `Cluster consolidated` | **+`resolved`** before `policyChatId` | `services/consolidation.rs:1512-1513` |
| INFO | `Retired built-in prompt template removed` | `{templateId, name}` | `builtin_prompt_templates.rs` |
| DEBUG | `Built-in prompt templates pruned` | `{builtInCount, shippedCount, removed}` | **R1** |
| DEBUG | `[applyOutfitSelections] Spread multi-slot picks across every slot they cover` | `{chatId, characterId}` | `outfit_selections.rs` (both sites) |

## 5. Harness / oracle impact

### 5.1 Families that REDDEN at a pin regen (v4 drift reds, not v5 bugs)

- **PROMPTS:**
  - `inform_block_equivalence` (the exported header);
  - `chat_informs_tier2_equivalence` / `chat_informs_routes_equivalence` where the block is rendered;
  - `multi_character_turn_anchor_equivalence` (holds `GROUP-SCENE DISCIPLINE` bytes);
  - `memory_tasks_equivalence` (SELF / OTHER / episode bodies);
  - `fold_episode_tier3_equivalence` / `fold_other_pass_tier3_equivalence` / `memory_pipeline_jobs_tier3_equivalence` (full requests);
  - `consolidation_tasks_equivalence` (prompt + validator output gains `resolvedIds`);
  - `consolidation_plan_equivalence` (plan gains `resolvedMarks`);
  - `memory_maintenance_consolidate_tier3_equivalence` / `memory_consolidation_routes_equivalence` (keyword writes, the DEBUG key);
  - `recall_tags_equivalence` (`fired` lists; `harness/oracle/cases/recall-tags.ts` prints the table);
  - `recall_tuning_equivalence` (`recall-tuning.ts` — the 13th key, the strict-schema message);
  - `prompt_templates_tier2_equivalence` + `prompt_templates_routes_equivalence` (22 rows, refreshed bodies, the prune — and the fixture re-aim, headline 6);
  - `builtin_prompt_templates_guard` (run with `QT_V4_CHECKOUT=$PIN`);
  - `slot_ops_equivalence` (new export) and `outfit_llm_choose_tier3_equivalence` / `chat-dialogs-llm-choose-tier3` / `chat_create_capstone` (the prompt paragraph + spread);
  - `build_context_tier3_equivalence` / `orchestrator_tier3_equivalence` on every multi-character or inform row.
- **DEUS-READ / -WRITE** (new rows, existing families):
  - `message_attribution_equivalence`, `chat_tasks_equivalence`, `context_compression_equivalence`, `context_summary_equivalence`, `context_summary_service_tier3_equivalence`, `speaker_names_equivalence`, `turn_transcript_equivalence`, `courier_transport_tier3_equivalence`, `in_scene_voiced_tier3_equivalence`, `core_whisper_equivalence` (if `WhisperEvent` widens), `build_context_tier3_equivalence`, `orchestrator_tier3_equivalence` (the DEBUG key on EVERY send);
  - `post_office_routes_equivalence` (`harness/oracle/cases/post-office-routes.test.ts` — the fact arm);
  - `system_jobs_routes_equivalence` / `system_jobs_collection_equivalence` / `background_jobs_tier2_equivalence` / `activity_tables_equivalence` (the enum list moves).
- **GARBLE check:** none of the oracle cases import a symbol these commits DELETED. Only additions and string edits: `turnsToChatMessages` gained an export, nothing was removed. No IMPORT-DEAD case in this area (measured by reading the hunks; the census belongs to `/setupphase`).

### 5.2 NEW families (closest recipe)

- `established_fact_equivalence` (tier-1, pure) — recipe `harness/oracle/cases/inform-block.ts` + `crates/quilltap-harness/tests/inform_block_equivalence.rs`. Rows: both kinds × both voices, idempotent re-frame, the operator-typed-opener strip, `frameEstablishedFacts` identity, counts.
- `established_fact_reminder_equivalence` (tier-1 over event lists) — recipe `core-whisper.ts` / `core_whisper_equivalence.rs` (same cadence helper). Rows: no prior turn with / without history access, a targeted-away fact, an unparseable `createdAt` with and without a floor, the 8000 cap with multi-byte bodies, a swipe cut, a seat line with an unresolved speaker.
- `established_fact_memories_tier3` (job + admission) — recipe `carina-memory-extraction-tier3.test.ts` / `carina_memory_extraction_tier3_equivalence.rs`. Rows: absent / silent / removed / user-controlled / archived seats, a missing message, a repeat post through admission dedup.
- The send-refine and announcement-fact arms ride `post_office_routes` and the send-route family (`orchestrator_tier3`'s HTTP rows or the web chat-send route family).
- A `deus_ex_machina_name_leak_guard` census (headline 1) — recipe: the existing source-text guards (`*_census.rs`, `*_guard.rs`).

## 6. SPA (SPA-DEUS) — file map, depth deliberately light

| v4 (pin) | v5 (`ff1651177`) |
|---|---|
| `lib/slash-commands/registry.ts:65-70` `deus` + `types.ts:30` | `apps/web/src/app/chat/slash-commands/registry.ts:30-…` `COMPOSER_ACTION_SPECS`, `slash-commands/types.ts`, `chat/slash-command-runner.ts` |
| `ChatComposer.tsx:96-97, 245-352, 472-485, 606-608` (armed chip, tinted editor, Send label, `/deus` arm) | `chat/chat-composer.ts` (send `:1149-1165`) |
| `KeyboardPlugin.tsx:59-61, :124` (Cmd/Ctrl+Shift+Enter one-line fact) | `editor/markdown-field.ts` (`submitOnEnter` / `submitOnModEnter`, composer binding `chat-composer.ts:513-514`) |
| `useSSEStreaming.ts` (`opts.establishedFact` → body `isEstablishedFact: true` + optimistic flag) | `screens/salon/salon-conversation.ts:2154 / :3972` (send body), `core/core-contract.ts:49` (the send DTO) |
| `useImpersonationVoice.ts` (`establishedFact` → never rehearse) | `chat/impersonation-voice/gate.ts:55` `shouldRehearseImpersonatedLine`, `impersonation-voice.state.ts:160` |
| `MessageRow.tsx:217-220, :401-404` (ribbon / band) | `chat/message-row.ts` |
| `system-message-labels.ts` (`'established-fact': 'Deus Ex Machina'`, accent `qt-chat-established-fact`, importance `high`) | `chat/system-message-labels.ts:14-62` (overrides), `:323` (accent), `:378` (importance table) |
| `announcement-render-items.ts:52` (exemption) | `chat/chat-view-model.ts:293-299` |
| `InsertAnnouncementDialog.tsx:82, :441` (the fact tab) | `chat/post-office/insert-announcement-dialog.ts:30` (`Mode`), `:422` (tabs) |
| `SpeakingAsAvatar.tsx` toggle (`df8d4d70a`) | `chat/speaking-as-avatar.ts:45-68` |
| icon + Madman's Box variant | the v5 icon set (`app/ui/icon.ts`) + the themes pack (P4.D357's files) |
| `_chat.css` +54 / +41 | the v5 `qt-components` chat CSS |
| `SalonView.tsx:1924-1975` | `screens/salon/salon-conversation.ts` |

The tooltip / aria-label half of `df8d4d70a` touches
`rng-dropdown.ts`, `custom-tools-popup.ts`, `pending-inform-chips.ts`,
`editor/formatting-toolbar.ts` and the gutter tools. These are the round's
WCAG / composer lane's files; give them there, not to SPA-DEUS.

## 7. The lane split, by FILE OWNERSHIP

### DEUS-READ (wave 0; creates the shared module FIRST as its Tier-1 commit)

- **Creates:**
  - `established_fact.rs`;
  - `services/established_fact_reminder.rs`;
  - harness `established_fact_equivalence.rs` + `established_fact_reminder_equivalence.rs` + the two oracle cases;
  - the name-leak guard.
- **Edits:**
  - `services/message_context.rs` (the frame pass and the mwp mark ONLY — not `GROUP_SCENE_DISCIPLINE`);
  - `message_attribution.rs`;
  - `services/build_context.rs` (trailing list + reminder + the history-access caller);
  - `core_whisper.rs` (`WhisperEvent` widening — additive fields);
  - `chat_tasks.rs`;
  - `context_summary.rs`;
  - `services/context_summary.rs`;
  - `services/context_summary/prompt_text.rs` + `harness/tools/extract-prompt-text.cjs`;
  - `services/speaker_names.rs` (R2);
  - `memory_extraction/window.rs`;
  - `services/turn_transcript.rs`;
  - `services/courier_transport.rs`;
  - `services/announcer/in_scene_voiced.rs`.

### DEUS-WRITE (wave 1, branched from DEUS-READ's Tier-1 commit)

- **Creates:**
  - `services/established_fact_memories.rs`;
  - the host handler wrapper;
  - its tier-3 family.
- **Edits:**
  - `api/types.rs` (two additive variants/fields at fixed positions);
  - `api/engine.rs` (the `ChatSend` refine arm);
  - `api/chat_send.rs`;
  - `services/orchestrator.rs` (`SendMessageOptions` + the two `record_operator_post` call sites + `final_user_message`);
  - `services/operator_post.rs`;
  - `api/chat_post_office.rs`;
  - `services/announcer/writer.rs`;
  - `services/queue_service.rs`;
  - `services/job_runner.rs`;
  - `api/system_data.rs`;
  - `services/activity_kinds.rs`;
  - `realtime/job_topics.rs`;
  - `memory_candidate.rs`;
  - `quilltap-host/src/spine.rs` (one registration);
  - `api/salon.rs:651`;
  - `services/chat_continuation.rs:355`.

### PROMPTS (wave 0)

- **Edits:**
  - `services/inform_block.rs`;
  - `services/message_context.rs` (the `GROUP_SCENE_DISCIPLINE` const ONLY, `:1032-1050`);
  - `memory_extraction/prompts/prompt_text.rs` + `harness/oracle/scripts/extract-memory-task-prompts.py`;
  - `consolidation_tasks/prompt_text.rs`;
  - `consolidation_tasks.rs`;
  - `consolidation_plan.rs`;
  - `services/consolidation.rs`;
  - `services/memory_maintenance/consolidate.rs`;
  - `recall_tags.rs`;
  - `recall_tuning.rs`;
  - `services/builtin_prompt_templates.{json,rs}`;
  - `api/prompt_templates.rs`;
  - the prompt-templates fixture builder + `apps/web/e2e/prompt-templates-flow.spec.ts:29`;
  - `slot_ops.rs`;
  - `services/outfit_selections.rs`;
  - `services/chat_merge.rs:482`;
  - `services/chat_participants.rs:1516` (the `chat_id` thread only).

### SPA-DEUS (wave 0; the server wire is additive — stub it in Vitest)

§6 left column, minus the tooltip / aria half.

### Collisions against the rest of the round

1. **`generators/field_semantics.rs` (GENERATED) — PROMPTS vs the character-kind lane (`3ae4cf582`, `7ff5fda89`).**
   - **Recommend:** the character-kind generators lane owns it whole and regenerates it ONCE from the pin. That regen carries `ba6c8fe0f`'s four constant edits for free.
   - PROMPTS does not touch it. It only records that the generator families (`generators_field_semantics`, `generators_wizard_prompts_equivalence`, `character_optimizer_prompts_equivalence`, `character_wizard_tier3`, `character_optimizer_tier3`, the External Prompt / AI-import families) move from `ba6c8fe0f` too.
   - `7ff5fda89`'s bullet cannot land without `STAKES_AND_REACTION_DIRECTION` anyway.
2. **`services/builtin_prompt_templates.json` — PROMPTS only.** `ba6c8fe0f` (1.1.30), `d0201575a` (1.1.32) and `7ff5fda89` (1.1.33) all land in ONE regen from the pin. The kind lane must not re-dump it.
3. **`services/message_context.rs` — DEUS-READ (frame pass `:1280-1371`), PROMPTS (const `:1043`), and the image-cap / image-transport-budget lanes (`d439f6dc9`, `29c1e2690` — the attachment region).** Disjoint regions. Each lane fences its hunk (`// === <lane> ===`); the unifier merges textually.
4. **`services/build_context.rs` — DEUS-READ (trailing list, reminder, history caller) vs the `0c47f4539` tier-walk / knowledge-injector lane and the 4.12.1 window-first recall lane.** Disjoint functions. DEUS-READ's trailing-list hunk is a one-line insert after `inform_block`.
5. **`services/orchestrator.rs` — DEUS-WRITE vs `604a082c4` (tagged SSE frames), `96156566b` (Agent-Mode flavour), `9eab8d2da` (gesture yields) and `29c1e2690`.** DEUS-WRITE's hunks are the two `record_operator_post` call sites, `final_user_message` and one `SendMessageOptions` field, all additive. Recommend that DEUS-WRITE touch no frame-emitting code.
6. **`services/speaker_names.rs` — DEUS-READ vs the Brahma-lookup lane (`c539c82e0` calls `speakerLabel` at `brahma-lookup-handler.ts:446 / :538` over full rows, so a machine fact there ALSO reads `Established fact`).** See R2.
7. **`api/types.rs` / `api/engine.rs` (frozen) — DEUS-WRITE vs every route-adding lane.** One field on `ChatSend`, one variant on `AnnouncerSenderWire`, both at fixed positions. Name them in the order so the unifier places them.
8. **`quilltap-host/src/spine.rs` and the four job-type lists — DEUS-WRITE only in this area.** No other surveyed commit adds a job type (`d4ddeb943` reuses one). The insertion point is right after `CARINA_MEMORY_EXTRACTION` in each list. `realtime/job_topics.rs` is ALSO the wardrobe survey's file (`2026-11-04-setupphase-wardrobe-items-pascal.md` names `db6190b44`'s `job-topics.ts` +1 as a collision): disjoint arms, fenced. Cross-checked: the character-kind survey's R-K1 makes the SAME call on `field_semantics.rs` (its lane K-C owns the file whole at the pin) and hands the catalogue half of `d0201575a` / `7ff5fda89` to this area — the two surveys agree.
9. **`memory_extraction/prompts/prompt_text.rs` — PROMPTS owns the regen.** DEUS-READ depends on the ESTABLISHED FACTS block reaching the extractor. That is a union obligation: the unifier re-runs `memory_tasks_equivalence` and the extraction tier-3 families after both lanes land. Alternatively, DEUS-READ stacks on PROMPTS' Tier-1 commit.
10. **`core/core-contract.ts` (SPA) — SPA-DEUS vs every SPA lane.** Additive fences.

## 8. Tiering

- **DEUS-READ:**
  - Tier 1:
    - the module + the name-leak guard;
    - the frame pass (both voices) + the mwp mark;
    - the history-access and presence exemptions;
    - the reminder + its trailing slot;
    - `toVisibleConversationMessage` (+ the compression-window rows);
    - the fold (partition, labels, frame, prompt regen);
    - `speaker_label`;
    - the extraction window + turn transcript;
    - the Courier;
    - in-scene.
  - Tier 2: the DEBUG lines that only report counts.
- **DEUS-WRITE:**
  - Tier 1:
    - the send flag + the two refines;
    - the operator-post key / INFO;
    - the posting turn's frame;
    - the announcement fact arm (201, the `[]` refusal);
    - `post_established_fact`;
    - the job (enqueue dedupe, handler, admission), the four lists + the host registration;
    - CandidatePass;
    - continuation.
  - Tier 2: the transcript-projection key (R4).
- **PROMPTS:**
  - Tier 1:
    - the header and discipline consts;
    - the extraction / bindery / catalogue regens (+ teaching the extractors);
    - `resolvedIds` / `resolvedMarks` / the keyword write;
    - the `resolved` multiplier + tuning key;
    - the prune (+ the probe fix);
    - the spread at both sites.
  - Tier 2: the prune DEBUG on the read path (R1).
- **SPA-DEUS:**
  - Tier 1: the composer arm (`/deus`, the shortcut, chip, Send label), the toggle, the send body key, the impersonation-voice bypass, the row ribbon / band, the label / accent / importance, the chip exemption, the Insert Announcement fact tab.
  - Tier 2: the icon variant and a Playwright beat.

## 9. Fixtures

- **Widened:**
  - `build-context-tier3.json` (+builder): a seat-line fact, a machine fact, a pre-join fact with no history access, an opaque seat (plain voice), a swipe;
  - `orchestrator-tier3.json`: a fact send, and a fact send on a paused chat;
  - the context-summary, courier, in-scene and turn-transcript fixtures (one fact row each);
  - `prompt-templates-routes.json` (+builder): the re-aim + prune rows;
  - the consolidation-plan corpus (`resolvedIds` rows incl. a digest id, a duplicate and an unknown handle);
  - the recall-tags corpus (keyword `Resolved ` with spaces and capitals).
- **Regenerate from the pin only:** the §5.1 pinned-worktree recipe, with `pin-plugin-sdks.sh` for the plugin dirs. The catalogue dump in particular must run in the pin. Its plugin `index.js` is in-tree, but `node_modules` must be the pin's.
- Memory note `shared-fixture-rebuilds-and-jest-setup-mocks`: a rebuilt pair re-mints ids — mask, never `ORDER BY` a minted id.

## 10. Rulings owed to the human

- **R1 — the prune's DEBUG on every template read.**
  - v4 seeds, and therefore prunes and logs `Built-in prompt templates pruned`, on EVERY template read. v5 skips the writer when the read-side probe finds nothing to do.
  - **Recommend:** the probe also counts built-ins and logs the DEBUG with `removed: 0` when nothing is owed, so the bytes match on every read. The writer arm logs it after a real prune. (The alternative is to record a DEBUG-only divergence.)
  - Either way, the probe must answer `true` for a retired built-in row (not a ruling — a correctness fix).
- **R2 — `speaker_label`'s shape.**
  - v4 has one function whose input widened to read `systemSender` / `systemKind`.
  - **Recommend:** widen v5's one function to take the row's sender and kind (one rule, as v4 has), landed in DEUS-READ's Tier-1 commit. The Brahma-lookup lane calls the new signature.
  - The alternative, an additive `speaker_label_with_kind`, avoids the textual collision but makes a second rule that a census must police.
- **R3 — the send refines' 400 body.** v4 throws a ZodError carrying up to THREE issues (the two fact issues on `isEstablishedFact`, then the content issue). v5's `ChatSend` arm answers one fixed string today (`engine.rs:2155-2163`).
  - **Recommend:** if the web edge already renders Zod issue bodies for sends, port all three issues in v4's order. Otherwise extend the existing single-string arm with the two fact strings in v4's precedence and RECORD the issue-list divergence beside the existing one.
  - A non-boolean `isEstablishedFact` must 400, not be ignored.
- **R4 — `isEstablishedFact` on the transcript wire.** v4 projects `event.isEstablishedFact || null` — always present, `null` unless `true` (`transcript-projection.ts:239`). **Recommend:** port it as `or_null` beside `isSilentMessage` (`api/salon.rs:651`); the SPA reads it for the ribbon.
- **R5 — CandidatePass position.** `'established-fact'` goes between `manual` and `consolidation`, which moves every enum issue message's `values` list. **Recommend:** port as positioned (it is v4's schema order). Measured: the only consumer of `CANDIDATE_PASS_VALUES` is `memory_candidate.rs` itself; no harness file or committed fixture pins the 8-value list.
- Not rulings, but call them out in the orders:
  - the ledger corrections in §0 / §1;
  - `671c59a53` NO-PORT;
  - the `3d39c303c` announcement half NO-PORT (verified);
  - `ba6c8fe0f`'s generator half moves to the kind lane.

## 11. Dogfood rows to queue (💸 = spends a model call)

These wait for the catch-up, per the 2026-11-04 rule.

- 💸 `/deus` a line in a three-seat chat. Expect:
  - the ribbon;
  - the next speaker's request carries the plain seat frame on the posting turn, and the host frame in history on later turns when every seat is transparent;
  - a seat that joined later (no history access) still sees it;
  - the trailing `Established since you last spoke:` section appears once, then is gone after that seat speaks.
- 💸 Cmd/Ctrl+Shift+Enter on one line; a whispered `/deus` and an empty `/deus` are refused with v4's sentences.
- 💸 The Insert Announcement fact tab:
  - a full-width band, never chipped;
  - 201;
  - the Jobs panel shows `ESTABLISHED_FACT_MEMORIES`;
  - every non-removed LLM seat (absent and silent too) gains one MANUAL memory at 0.8 importance;
  - a repeat post deduplicates through admission;
  - an `[]` audience is refused.
- 💸 A fact survives a summary fold (labelled `Established fact`), a Courier hand-off (`[Established fact]` + the plain frame) and an in-scene rehearsal. The UI name appears in NO Inspector request.
- **Measure first on the Friday copy:**
  - the count of memories with a `resolved` keyword (any case or padding) — each ranks ×0.6 at the pin;
  - whether `prompt_templates` holds any `isBuiltIn` row outside the 22 shipped names (expected: none, since v4 already pruned).
- 💸 A consolidation run on a character with a hurt and a later forgiveness: one digest leading with the resolution, the hurt row's keywords gain `resolved`, and recall ranks it below the repair.
- The Import-from-Template modal lists `MODERN Roleplay` and `MODERN Assistant`, not `MODERN General`. A v5-only instance with the old row loses it on the first modal open (one INFO). A character that imported `MODERN General` keeps its copy.
- 💸 Green Room with a dress listed only under `top`: it occupies `top` and `bottom` on chat create, on participant add, and on merge.
- 💸 An inform "You are very alarmed" is believed; a five-seat scene does not converge on one measured stance.
