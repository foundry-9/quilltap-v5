# Survey — P4.D243: the chained-turn scene note (`ca363178d` part d)

**Date:** 2026-09-30. **Read-only measurement.** v4 at `ca363178d` (checkout
clean AT that commit; oracle baseline `97b25fc53`). v5 `main` at `735cf568e`.
Port from HUNKS (ledger §5.3); the spec is context, not the contract.

## Spec of record (summary)

`docs/developer/features/prompt-trust-and-anti-committee.md` §9 + §15 "Phase 4":
on a chained multi-character turn (second responder onward, Continue, Nudge)
the human's narration sits mid-history and the newest line may be a stale
character reply that contradicts it (a stopped turn that finished server-side,
a second tab). A one-paragraph trailing **scene note** names the human's latest
message as the state of the scene. It is empty (byte-for-byte nothing) when it
does not apply, so the first responder is unchanged. It lands on the uncached
tail, never in block 1, is not persisted, and moves neither
`IDENTITY_STACK_BUILDER_VERSION` nor `PROMPT_CACHE_STRUCTURE_VERSION`. §9.4
(curing the race itself) is out of scope. The help page
`help/chat-multi-character.md` gains "### Your Narration Stands" — **not this
lane's file** (`help/` belongs to the round's re-vendor).

---

## §A — v4

### A1. `lib/chat/context/user-narration-anchor.ts` (NEW, 105 lines, verbatim)

```ts
/**
 * The trailing scene note on chained multi-character turns.
 *
 * On a chained turn (the second responder onward, or a continue / nudge) no new
 * user message rides at the tail: the human's narration sits mid-history and
 * the newest thing in the prompt is another character's reply. When that reply
 * contradicts the narration — a stopped turn that finished server-side and
 * landed after the human's newer message, or a second tab posting mid-chain —
 * nothing tells the next character which account wins. This note does: the
 * human's latest message is the state of the scene.
 *
 * ## Where it lands
 *
 * Never in the cached prefix. `applyMultiCharacterTurnAnchor` edits system
 * block 1 and must never carry per-turn wording; this is a trailing per-turn
 * section on the uncached tail, pushed by `context-manager.ts` ahead of the
 * progressions report and the turn-skip note on the chained-turn path only.
 * Conditional, not structural, so neither `IDENTITY_STACK_BUILDER_VERSION` nor
 * `PROMPT_CACHE_STRUCTURE_VERSION` moves. Not persisted.
 *
 * ## Empty is byte-for-byte nothing
 *
 * When the note does not apply this returns `''` and the caller pushes nothing,
 * the same contract `lib/progressions/prompt-section.ts` keeps.
 *
 * Design of record: docs/developer/features/prompt-trust-and-anti-committee.md §9.
 */

import { logger } from '@/lib/logger'

const CONTEXT = 'chat.context.user-narration-anchor'

export interface BuildUserNarrationAnchorInput {
  /** Multi-character chat? Single-character chats have no race to settle. */
  isMultiCharacter: boolean
  /** True when this turn carries a new user message (first responder). */
  hasNewUserMessage: boolean
  /**
   * The history window this turn will send, oldest first. In a multi-character
   * chat other characters' replies are attributed to role `user` with their
   * `participantId`, so a character line is recognised by either signal.
   */
  historyWindow: ReadonlyArray<{ role: string; id?: string; participantId?: string | null }>
  /**
   * Row ids of the human's own turns (USER, no `systemSender`), captured
   * before whisper normalization re-roles Staff whispers to USER.
   */
  humanTurnMessageIds: ReadonlySet<string> | null | undefined
  /**
   * Fallback display name, resolved the way `{{user}}` is. Used only when the
   * matched human message's author cannot be named (an unseated user).
   */
  userName: string
  /**
   * Names the author of the matched human message from its `participantId`.
   * The human may drive several seats, and the "Speaking As" selection is not
   * necessarily who wrote the latest line, so the seat that wrote it wins.
   */
  nameForParticipant?: (participantId: string) => string | undefined
}

/** The note itself. Exported for tests and the help page's wording. */
export function renderUserNarrationAnchor(userName: string): string {
  return (
    `Scene note: ${userName}'s most recent message is the current state of the scene. ` +
    `Where any other speaker's line — before or after it — conflicts with what ${userName} narrated, ` +
    `${userName}'s account is what happened. Adjust without arguing; what you do about it is yours.`
  )
}

/**
 * Returns the scene note for a chained multi-character turn where the human
 * has spoken and a character has answered since, or `''` otherwise.
 */
export function buildUserNarrationAnchor(input: BuildUserNarrationAnchorInput): string {
  if (!input.isMultiCharacter || input.hasNewUserMessage) return ''
  const humanIds = input.humanTurnMessageIds
  if (!humanIds || humanIds.size === 0) return ''

  let lastHumanIndex = -1
  for (let i = input.historyWindow.length - 1; i >= 0; i--) {
    const id = input.historyWindow[i].id
    if (id && humanIds.has(id)) {
      lastHumanIndex = i
      break
    }
  }
  if (lastHumanIndex === -1) return ''

  const characterSpokeSince = input.historyWindow
    .slice(lastHumanIndex + 1)
    .some(m => m.role.toLowerCase() === 'assistant' || (!!m.participantId && !(m.id && humanIds.has(m.id))))
  if (!characterSpokeSince) return ''

  const authorParticipantId = input.historyWindow[lastHumanIndex].participantId
  const authorName = (authorParticipantId && input.nameForParticipant?.(authorParticipantId)) || input.userName

  logger.debug('[UserNarrationAnchor] Scene note applies to this chained turn', {
    context: CONTEXT,
    historyWindowSize: input.historyWindow.length,
    lastHumanIndex,
    resolvedFromSeat: authorName !== input.userName,
  })
  return renderUserNarrationAnchor(authorName)
}
```

**Rendered bytes** (one line, no trailing newline; the three template pieces
join with a single space after each `.`; both dashes are U+2014 EM DASH with a
space either side; apostrophes are ASCII `'`):

```
Scene note: <N>'s most recent message is the current state of the scene. Where any other speaker's line — before or after it — conflicts with what <N> narrated, <N>'s account is what happened. Adjust without arguing; what you do about it is yours.
```

Purity: `buildUserNarrationAnchor` is pure apart from the one `logger.debug`
(no I/O, no clock, no DB). `nameForParticipant` is an injected callback.

### A2. `lib/chat/context-manager.ts` hunks (with surroundings)

- `:208` — `import { buildUserNarrationAnchor } from '@/lib/chat/context/user-narration-anchor'`.
- `:602-607` — `BuildContextOptions` gains (after `turnSkip`):
  ```ts
    /**
     * Row ids of the human's own turns (USER, no `systemSender`), captured by the
     * caller before whisper normalization re-roles Staff whispers to USER. Feeds
     * the chained-turn scene note (`buildUserNarrationAnchor`). Absent → no note.
     */
    humanTurnMessageIds?: ReadonlySet<string>
  ```
- `:2703-2722` — computed right after `turnSkipInstruction`, before the
  `if (newUserMessage)` push (verbatim):
  ```ts
    // Chained multi-character turns: the human's narration sits mid-history and
    // the newest line is another character's, possibly one that contradicts it
    // (a stopped turn that finished server-side, or a second tab). The scene note
    // names the human's latest message as the state of the scene. Empty unless it
    // applies; never on the first responder, who has the human's line last.
    const userNarrationAnchor = buildUserNarrationAnchor({
      isMultiCharacter,
      hasNewUserMessage: !!newUserMessage,
      historyWindow: selectedMessages,
      humanTurnMessageIds: options.humanTurnMessageIds,
      userName: userCharacter?.name || 'User',
      nameForParticipant: participantId => {
        const seat = allParticipants?.find(p => p.id === participantId)
        return seat?.characterId ? participantCharacters?.get(seat.characterId)?.name : undefined
      },
    })
  ```
- `:2748-2760` — the chained branch (verbatim, after):
  ```ts
    } else if (userNarrationAnchor || turnSkipInstruction || progressionsLLMContext) {
      // Chained / continue turns carry no new user message, so neither the note
      // nor the progressions report can ride as a trailing section above. Push
      // them as their own trailing user message (same off-scene/timestamp
      // pattern) so the model sees them this turn, in the same order they would
      // have taken there. Anthropic 4.6+ rejects role=assistant tails, so 'user'
      // is required. The scene note leads: it is about the scene, the others
      // about the character.
      const trailingOnly = [userNarrationAnchor, progressionsLLMContext, turnSkipInstruction].filter(Boolean)
      contextMessages.push({
        role: 'user',
        content: trailingOnly.join('\n\n---\n\n'),
      })
    }
  ```
  **Before:** `} else if (turnSkipInstruction || progressionsLLMContext) {` and
  `const trailingOnly = [progressionsLLMContext, turnSkipInstruction].filter(Boolean)`;
  the comment ended `… so 'user'\n    // is required.` The pushed message has
  **no** `name`, **no** `metadata`, **no** `cacheControl`. Separator
  `'\n\n---\n\n'`; no heading of its own.
- **The first-responder branch is UNCHANGED** — the anchor is never pushed into
  `trailingContextSections` (`:2732-2736`).
- **Window = `selectedMessages`** — the POST-trim output of
  `selectRecentMessages(messagesToProcess, …)` (`:2083`), i.e. after history-
  access filter → whisper filter → `attributeMessagesForCharacter` (`:2010-2038`)
  → summary-anchor drop (`:2052-2059`) → budget trim. Each element carries
  `{role ('user'|'assistant', attributed), id, name, participantId (msg.participantId || undefined), thoughtSignature}`.
  In multi mode `role === 'assistant'` iff `participantId === respondingParticipant.id`;
  every other character's reply is role `user` with its `participantId`.
- **No `isContinueMode` gate** — the anchor fires on Continue / Nudge too (unlike
  progressions, which skip continue mode).
- `isMultiCharacter` here is buildContext's own (`:749-755`):
  `!!(respondingParticipant && allParticipants && allParticipants.length > 1 && participantCharacters && messagesWithParticipants)`.
- **Cache placement:** it is the LAST `contextMessages.push` in `buildContext`,
  after the history loop (`:2172`, where the only `cacheControl` is the
  summary-head breakpoint) and after the off-scene / timestamp pushes. It can
  never land in the system blocks or ahead of the summary breakpoint.

### A3. `lib/services/chat-message/context-builder.service.ts`

- `:1134-1135` (inside the single `buildContext({...})` call at `:1077`, after
  `regenerationOfMessageIds`):
  ```ts
      // The human's own turns, for the chained-turn scene note.
      humanTurnMessageIds: userTurnMessageIds,
  ```
- It is **the SAME set bug 95 computes** (`:986-993`):
  `new Set(messagesAfterWhisperFilter.filter(m => m.type === 'message' && m.role === 'USER' && !m.systemSender && m.id).map(m => m.id as string))`
  — computed AFTER the whisper visibility filter, BEFORE `normalizeWhisperRoles`
  re-roles Staff whispers (`:984`), and before rehydrate / buildContext's own
  window trim. It also feeds the attachment anchor (`:1269`, unchanged).
- v4 has exactly ONE `buildContext(` caller (`git grep` at `ca363178d`; the
  `help-chat/context-resolver.ts` hits are an unrelated local function). Two
  `buildMessageContext` callers — `orchestrator.service.ts:1182` and
  `regenerate-swipe.service.ts:189` (`newUserMessage: undefined, isContinueMode: true`)
  — so **a swipe/regenerate in a multi-character chat also gets the note.**

### A4. `nameForParticipant`

Not a module function — an **inline closure** at `context-manager.ts:2718-2721`:
`allParticipants?.find(p => p.id === participantId)` → if `seat?.characterId`
(truthy) → `participantCharacters?.get(seat.characterId)?.name`, else
`undefined`. `allParticipants` = `chat.participants` (ALL seats, removed
included — `context-builder.service.ts:1097`); `participantCharacters` keyed by
characterId. Final name: `(authorParticipantId && nameForParticipant(id)) || userName`
— empty/absent seat name falls to `userName = userCharacter?.name || 'User'`.
**v5 has no twin** (`ggrep name_for_participant|nameForParticipant crates/` → 0);
the `'User'` fallback has a precedent at `build_context.rs:2216-2221`.

### A5. The debug line

`logger.debug('[UserNarrationAnchor] Scene note applies to this chained turn', { context: 'chat.context.user-narration-anchor', historyWindowSize, lastHumanIndex, resolvedFromSeat })`
— field order `context, historyWindowSize, lastHumanIndex, resolvedFromSeat`;
emitted ONLY on the applying path. `resolvedFromSeat` is **`authorName !== userName`**
(name inequality, NOT "came from a seat").

### A6. Tests

`__tests__/unit/lib/chat/context/user-narration-anchor.test.ts` (89 lines).
Base: `humanIds = {u1,u2}`, window
`[USER u1 p-user, ASSISTANT a1 p-a, USER u2 p-user, USER a2 p-b]`, `userName 'Owen'`,
multi true, hasNew false.

| case | input delta | expected |
|---|---|---|
| applies | base | `renderUserNarrationAnchor('Owen')` AND the literal `"Scene note: Owen's most recent message is the current state of the scene. Where any other speaker's line — before or after it — conflicts with what Owen narrated, Owen's account is what happened. Adjust without arguing; what you do about it is yours."` |
| single-char | `isMultiCharacter:false` | `''` |
| first responder | `hasNewUserMessage:true` | `''` |
| no human in window | ids `{elsewhere}` / `undefined` / `new Set()` | `''` ×3 |
| staff whisper only | window `[USER u2 p-user, USER host-1 participantId:null]` | `''` |
| role-assistant w/o pid | window `[user u2, assistant a9]` (no pids) | not `''` |
| seat wins | window `[USER u1 p-user-2, USER u2 p-user, ASSISTANT a1 p-a]`, `userName 'Alex'`, names `{p-user:Owen,p-user-2:Alex}` | `render('Owen')` |
| unseated fallback | base, `userName 'Alex'`, `nameForParticipant: () => undefined` | `render('Alex')` |

`__tests__/unit/context-management.test.ts` (+85, a `describe('user narration anchor (chained turns)')`):
history `[USER u1 participant-user "((The engine stalls.))", ASSISTANT a1 participant-b "The engine is running fine."]`,
responder `participantA`, `userCharacter {name:'Alex'}` (the `participant-user`
seat's character `char-user` is ALSO named `Alex`, so the test cannot tell seat
from fallback). Asserts: (1) tail role `user`, content starts
`"Scene note: Alex's most recent message is the current state of the scene."`;
(2) with `turnSkip {offerSkip:true,…,characterName:'Lyra'}` the tail starts
`Scene note:`, contains `\n\n---\n\n`, and `Scene note:` precedes `[NOTHING TO ADD]`;
(3) ids `{not-in-window}` → messages `toEqual` the no-ids run, no `Scene note:`;
(4) with `newUserMessage:'Onward.'` the ids run `toEqual` the no-ids run.

### A7. Version constants — NEUTRAL (verified)

`git grep` at `ca363178d`: `IDENTITY_STACK_BUILDER_VERSION = 2`
(`lib/chat/context/system-prompt-builder.ts:154`), `PROMPT_CACHE_STRUCTURE_VERSION = 4`
(`lib/llm/cache-key.ts:39`). `git diff 97b25fc53 ca363178d -- lib/` mentions
both ONLY inside the new file's doc comment — neither value changed. System
blocks 1–3 and `toolsArrayHash` are untouched. **But** `historyTailHash`
(`lib/llm/cache-prefix-hashes.ts:72-82`: all non-system messages EXCEPT THE
LAST) moves on every anchor turn: with the anchor as the new last message, the
former tail (the last character reply) enters the hashed frozen history. That
is a per-turn value in `llm_logs`, not a cache-structure change.

---

## §B — v5 on `main`

- **`BuildContextInput`** `crates/quilltap-core/src/services/build_context.rs:511-632`.
  **No `Default` derive / impl** — every construction lists every field. No
  human-turn-ids field today. Related fields: `new_user_message: Option<String>`
  (`:518`), `is_continue_mode`, `turn_skip: Option<TurnSkip>` (`:617`),
  `user_character: Option<system_prompt::UserCharacter>` (`:515`, `{name, description}`),
  `all_participants: Option<Vec<FullParticipant>>` (`FullParticipant` `:411`,
  has `id`, `character_id: Option<String>`), `participant_characters:
  Option<HashMap<String, ContextCharacter>>` (keyed by character id, `.name`).
- **`build_context`** `:1871`. `is_multi_character` `:1914-1917` — the exact
  twin of v4 `:749` (responding participant && all.len() > 1 && map && mwp).
- **Window:** `selected_messages` (`:3197`, `Vec<SelectableMessage>` from
  `select_recent_messages(&messages_to_process, …)`; `SelectableMessage`
  `message_selector.rs:17` = `{role, content, id: Option, thought_signature,
  name, participant_id: Option}`); multi-mode roles come from
  `attribute_messages_for_character` (`message_attribution.rs:230`, lowercase
  `user`/`assistant`, `participant_id` `""→None` at `:253-257`).
- **Progressions** `:3637-3664` (skipped in continue mode); **turn-skip**
  `:3671-3676`; first-responder push `:3680-3756` (trailing order core whisper →
  recall → mail → progressions → turn-skip, `:3718-3734`); **chained branch
  `:3757`** `} else if !turn_skip_instruction.is_empty() || !progressions_llm_context.is_empty() {`
  with **`trailing_only` `:3764`** `[progressions_llm_context, turn_skip_instruction]`,
  pushed `role "user"`, `metadata/name/cache_control: None` (`:3768-3775`).
  The anchor computes between `:3676` and `:3679`.
- **"No new user message":** `input.new_user_message` — v5 branches on
  `if let Some(..)` (`:3680`), v4 on JS truthiness (see D7).
- **Name resolution:** no seat→name helper exists for this shape; build the
  closure from `input.all_participants` + `input.participant_characters`.
  `'User'` precedent `:2216-2221` (`.filter(|n| !n.is_empty()).unwrap_or_else(|| "User")`).
- **Tracing convention for a v4 `CONTEXT` module:** `progressions/prompt_section.rs:44,112,204-206`
  — `target: "<v4 CONTEXT string>"` plus `context = CONTEXT`. Twin:
  `target: "chat.context.user-narration-anchor"`. (`build_context.rs` itself uses
  `target: "quilltap::build_context"`, e.g. `:2900`.) Pure `lib/chat/context/*`
  twins live top-level in core (`message_selector.rs`, `core_whisper.rs`,
  `mentioned_characters.rs`) → new `crates/quilltap-core/src/user_narration_anchor.rs`
  + `pub mod` in `lib.rs` (alphabetical, near `:214`/`:229`).
- **`user_turn_message_ids`** `services/message_context.rs:113`
  (`pub(crate)`, returns `HashSet<String>`; twin of v4's predicate incl. empty
  `systemSender`/`id` → excluded). Called at **`:1301`** on
  `messages_after_whisper_filter` (after the whisper filter, from the PRE-normalization list —
  `normalize_whisper_roles` at `:1297` only re-roles a copy; same point as v4).
  Still read at `:1518` (`select_attachment_anchor_index(&anchor_view, &user_turn_message_ids)`),
  so the fill must **clone**. No visibility change needed (same crate, local value).
- **v4 `context-builder.service.ts` twin = `message_context.rs::build_message_context`**
  (`:1177`). It receives a pre-built `build_input` and fills the message-
  dependent fields at `:1405-1427`, then calls `build_context` at `:1431`. The
  one fill line goes there (beside `build_input.messages_with_participants = …`).
- **Every `BuildContextInput {` construction** (each must name the new field):
  | site | what it is | value |
  |---|---|---|
  | `services/orchestrator.rs:4499` (`build_context_input`, `:4446`) | the shared builder for BOTH production paths; its message fields are placeholders | `None` placeholder (filled by `message_context`) |
  | `services/build_context.rs:4373` (`a_blocked_turn`, test) | unit test | `None` |
  | `services/build_context.rs:4706` (`a_turn`, test) | unit test | `None` |
  | `quilltap-harness/tests/build_context_tier3_equivalence.rs:802` | tier-3 driver | from a new op field |
- **Every `build_context(` caller:** `message_context.rs:1431` (production — the
  only one), `build_context.rs:4482`/`:4774` (tests), harness `:1058`. The
  `help_chat/context_resolver.rs` hits are an unrelated fn. `build_message_context`
  callers: `orchestrator.rs:2534` (Salon turns — first responder, chained legs,
  continue, nudge, autonomous-room turns via `process_message`) and
  `regenerate_swipe.rs:723` (`final_user_message: None`, `:629`). Both get the
  ids through the single fill in `message_context.rs` — matching v4, where the
  ids are supplied inside `buildMessageContext` for both callers. Greeting,
  Carina, Brahma, help chat, Scenario Builder do NOT reach `build_context`
  (v4's do not reach `buildContext` either) — no edit, no anchor.
- **No host / web / cli / tauri file** constructs `BuildContextInput` or calls
  `build_message_context` (`ggrep` over `quilltap-{host,web,cli,tauri}` → 0).

---

## §C — differential families

**New tier-1 family (right first unit — the function is pure):**
`harness/oracle/cases/user-narration-anchor.ts` (tsx, imports v4's REAL
`@/lib/chat/context/user-narration-anchor`; `nameForParticipant` as a corpus
name map; emit `{id, out}` per row) + `crates/quilltap-harness/tests/user_narration_anchor_equivalence.rs`
(`QT_ORACLE_USER_NARRATION_ANCHOR`). Corpus: v4's eight unit cases verbatim
plus the D-trap rows (empty-string pid/id, removed seat, empty seat name,
seat name == userName, `role:'ASSISTANT'` uppercase, last-human not last-user,
human id outside the window, `hasNew` with ids). Recipe:
`cd ~/source/quilltap-server && npx tsx $V5/harness/oracle/cases/user-narration-anchor.ts > /tmp/oracle-user-narration-anchor.ndjson`.
The debug line needs a separate capture pin (v4's logger at debug is not on the
NDJSON channel; pin the message + field NAMES and values per arm on the Rust
capture layer against the hunk's bytes).

**`build_context_tier3`** (`harness/oracle/cases/build-context-tier3.test.ts`
+ `harness/oracle/fixtures/build-context-tier3.json`, 47 ops; fixture DBs are
`/tmp`-built by `build-context-tier3-fixture.ts`; recipe header
`build-context-tier3.test.ts:56-65`: `QT_FIXTURE_OUT/QT_FIXTURE_MOUNT_OUT` →
fixture, then `QT_FIXTURE_BC_MAIN/QT_FIXTURE_BC_MOUNT/QT_ORACLE_OUT` → jest;
Rust `QT_ORACLE_BUILD_CONTEXT`). **No existing op qualifies:** the 13
multi-character ops (`multi_character_turn`, `sp_seat_*`×4,
`progressions_period_*`×2, `progressions_once_silenced`, `inform_*`×5) all
carry `newUserMessage`; the two chained ops (`progressions_chained_no_user_message`,
`progressions_chained_with_turn_skip`) are single-character. The case passes
`options` itself (`:415-445`), so `humanTurnMessageIds` must be a NEW op field
(`string[]` → `new Set`) on both sides — buildContext never derives it. New ops
needed: multi + no new user + ids in window + character after (applies);
+ turnSkip (order `[anchor, turnSkip]`); + progressions (order
`[anchor, progressions]` — needs a non-continue chained op, since progressions
skip continue mode); continue-mode multi (fires, progressions absent); seat-
named author vs `Charlie` fallback (the driver's `user_character` is
`Charlie`, `:821-824`); ids absent (neutral); window-trim drops the human
(neutral). Every existing op is the neutrality leg (no ids passed today).

**`orchestrator_tier3_equivalence`** (`orchestrator-tier3.test.ts:44-50`;
`QT_FIXTURE_OUT/QT_FIXTURE_MOUNT_OUT` via `build-orchestrator-fixture.ts`, then
`QT_FIXTURE_ORCH_MAIN/QT_FIXTURE_ORCH_MOUNT/QT_ORACLE_OUT`; Rust
`QT_ORACLE_ORCHESTRATOR`). **The corpus HAS anchor-firing calls — predicted, to
be measured by the regen at the pin.** Almost every chat has 2 seats
(LLM + a user persona seat with a character), which makes buildContext's
`isMultiCharacter` true (D1). Predicted to FIRE (continue mode, seeded human
`USER` then a character):
`commonplace_strip` (UAWW), `tool_whisper_filter` (U,TOOL,A), `disabled_tools`
(UA), `textblock_mode` (UA), `tool_settings_changed` (UA), `sentinel_prose`
(UA; the human row's `participantId` = user seat `590c…` → seat-named),
`rehydrate_user_attachments` (A,U[seat fe1c…],A — 2 LLM + user), and the
chained legs ≥2 of `noncontinue_two_llm_maxdepth` (2 LLM, no user seat; the
posted human row has no seat → `userName` fallback). Predicted NOT to fire:
`continue_mode`, `summary_fold`, `lantern_budget_*` (human last), `opaque_swap`/
`transparent_no_swap` (only `participantId:null` whispers after the human),
`multi_chain`/`skip_fire`/`nudge_withhold`/`rotation_*`/`paused_*` (no human
row), all first-responder calls. The anchor changes the request messages → the
canned stream key (`provider|model|temperature|messages`) and the
`CHAT_MESSAGE` `llm_logs` rows (request body + `historyTailHash`) on those
cases; the pre-port Rust run against the regenerated oracle is the red-first
leg (key misses). The P4.129 render-aware normalizer is unaffected (it
normalizes the render's wall clock, not trailing user messages).

**`regenerate_swipe_tier3_equivalence`** (`QT_FIXTURE_REGEN_MAIN/QT_FIXTURE_REGEN_MOUNT/QT_ORACLE_REGEN`;
corpus `regenerate-swipe-tier3.json`, all chats LLM + user seat → multi).
Predicted to FIRE: `first_regen` (target idx 3, previous U,A,A),
`plain_swipe_long_history`, `override_swipe_budgeted_for_override`,
`override_swipe_empty_trail` (target idx 20, previous ends U,A). Predicted
neutral: `existing_group`, `keep_memories`, `inform_reapply_*` (target directly
after the human), `not_assistant`. **Must be regenerated at the pin** — a
family the ledger row does not name.

**Expected NEUTRAL (name them in the neutrality proof):**
`retry_uncensored_tier3` (the swipe service is a recording boundary there,
`retry_uncensored_tier3_equivalence.rs:31-33,235-244`), `enclave_step_tier3`
(goes through `process_message`, but its corpus seeds no `USER` row —
`enclave-step-tier3.json` has 28 `ASSISTANT`, 0 `USER`; verify by regen),
`request_prefix_hashes_equivalence`, `primary_stream_tier3`,
`native_tool_loop_tier3`, `text_tool_loop_tier3` (canned message arrays, never
`build_context`), `identity_compiler_equivalence`,
`multi_character_turn_anchor_equivalence` (block 1 — **a name collision, not
this feature**), `chat_continuation_tier2`, `turn_orchestrator_tier2`
(no `build_context`), `brahma_orchestrator_tier3`, `help_chat_orchestrator_tier3`.

---

## §D — traps / premises

1. **"Multi-character" is buildContext's predicate, not "two LLM characters".**
   `allParticipants.length > 1` counts the user's persona seat, and the service
   passes the participant list whenever `isMultiCharacterChat` holds (≥1 active
   LLM seat — `lib/chat/turn-manager/utils.ts:204-208`; v5
   `participant_filters.rs:254`). So an ordinary 1:1 chat WITH a persona seat
   gets the note on Continue / Nudge / regenerate-of-a-later-reply. The spec's
   "single-character chats are byte-identical" holds only for seatless chats.
   Port the condition, not the prose.
2. **Push order** `[anchor, progressions, turnSkip]`, joined `"\n\n---\n\n"`;
   the `else if` gains the anchor as its FIRST disjunct. The first-responder
   branch is untouched. Progressions are absent in continue mode, so the
   anchor-before-progressions order needs a NON-continue chained op to be seen.
3. **Window = post-trim `selected_messages`.** Scanning `messages_to_process`
   (pre-trim) or the raw history silently changes which human row is "last",
   and fires when the human's row was trimmed or summary-folded (v4 returns `''`).
4. **Last HUMAN, not last user-role.** The backward scan matches ids in the set;
   other characters wear role `user` in multi mode. Staff whispers are not in
   the set (captured pre-normalization).
5. **Role test truthiness:** `m.role.toLowerCase() === 'assistant' || (!!m.participantId && !(m.id && humanIds.has(m.id)))`.
   `participantId` `""`/`null`/`undefined` → not a character (v5 attribution
   already maps `""→None`); a row with a pid whose id IS a human id is not a
   character (a second human seat). `m.id` `""` → `!("" && …)` = true → counts
   if it has a pid. Rust: `role.to_lowercase() == "assistant" || (pid.is_some_and(|p| !p.is_empty()) && !id.as_deref().is_some_and(|i| !i.is_empty() && set.contains(i)))`.
   The scan's `if (id && humanIds.has(id))` likewise treats `""` as no match.
6. **Author naming:** `(pid && name(pid)) || userName`. Seat removed / absent →
   `find` misses → fallback; seat with `characterId` `""`/null → fallback;
   character missing from the map or named `""` → fallback. `userName` =
   `user_character.name` filtered non-empty, else the literal `'User'`
   (`:2216` precedent). v5 always passes `Some(identity)` (`orchestrator.rs:1224`).
7. **`hasNewUserMessage` is JS truthiness: `!!newUserMessage`.** v5's
   `Option<String>` must be `.as_deref().is_some_and(|s| !s.is_empty())`.
   Recorded pre-existing gap beside it: v4 `if (newUserMessage)` sends `''`
   to the chained branch while v5's `if let Some(..)` (`:3680`) takes the
   first-responder branch for `Some("")` (`orchestrator.rs:1836-1843` builds
   `Some(content)` whenever not continue). Out of scope unless measured
   reachable; do not let the anchor's gate inherit it.
8. **No continue-mode gate** — unlike progressions (`:3637`). A wrong copy of
   the progressions gate would silently drop every Continue / Nudge note.
9. **`resolvedFromSeat` = name inequality.** When the seat's character name
   equals `userName` (the common persona case) it logs `false`. Field order
   `context, historyWindowSize, lastHumanIndex, resolvedFromSeat`; a
   sorted-field capture cannot see order (memory note
   `a-sorted-field-capture-cannot-see-v4-field-order.md`) — pin values per arm;
   `historyWindowSize`/`lastHumanIndex` as unsigned/`usize` render identically
   (non-negative on the applying path).
10. **The empty-set contract.** v4 treats `undefined`, `null` and an empty set
    identically (`''`). v5 may type the field `Option<HashSet<String>>` with
    `None` at every non-production site (faithful: v4's field is optional and
    only the wrapper sets it); what must not happen is the production fill
    being skipped — then every chained Salon turn silently loses the note with
    every non-orchestrator family green. The orchestrator/regenerate regen is
    the proof the fill is live.
11. **Prefix neutrality.** The anchor is a trailing `role:user` message with
    no `cache_control`; system blocks / tools hashes and both version constants
    are unchanged (A7). `historyTailHash` DOES move on anchor turns (expected,
    v4-identical). The attachment anchor (`message_context.rs:959-969`) cannot
    pick the note (no `metadata.message_id`, no `is_user_turn`) — same as the
    existing turn-skip trailing message.
12. **Naming collision:** `applyMultiCharacterTurnAnchor` /
    `multi_character_turn_anchor_equivalence` is the block-1 identity anchor —
    a different feature; leave it alone.
13. **Help page** "Your Narration Stands" lands with the `help/` re-vendor lane,
    not here; `help_tree_equivalence` is that lane's proof.

---

## §E — ownership (exact v5 files)

**Edits:**
- NEW `crates/quilltap-core/src/user_narration_anchor.rs` (render + build, the
  v4 doc comment's *why* carried, the debug line).
- `crates/quilltap-core/src/lib.rs` — one `pub mod user_narration_anchor;` line
  (shared file; trivial merge with any sibling adding a module).
- `crates/quilltap-core/src/services/build_context.rs` — the new
  `BuildContextInput` field (`:511-632`), the anchor computation between `:3676`
  and `:3679`, the `else if` (`:3757`) and `trailing_only` (`:3764`), the two
  test literals (`:4373`, `:4706`) + unit pins.
- `crates/quilltap-core/src/services/message_context.rs` — ONE fill line near
  `:1418` (`build_input.<field> = Some(user_turn_message_ids.clone());`), no
  visibility change.
- `crates/quilltap-core/src/services/orchestrator.rs:4499` — ONE placeholder
  line in `build_context_input`'s literal (unavoidable: the struct has no
  `Default`). **Planning input:** this is a shared, frequently-touched file;
  the edit is one line inside the struct literal. `regenerate_swipe.rs` needs
  NO edit (it goes through `build_context_input` + `build_message_context`).
- Harness: `crates/quilltap-harness/tests/build_context_tier3_equivalence.rs`
  (field at `:802` + op field), `harness/oracle/cases/build-context-tier3.test.ts`
  (op field → `options.humanTurnMessageIds`), `harness/oracle/fixtures/build-context-tier3.json`
  (new ops), NEW `harness/oracle/cases/user-narration-anchor.ts` + NEW
  `crates/quilltap-harness/tests/user_narration_anchor_equivalence.rs`.
  Orchestrator / regenerate-swipe: regen at the pin only (no driver change
  unless the lane adds a dedicated chained-multi case).
- `docs/CHANGELOG.md` (+ the current `docs/changelog/` month file if that is
  the convention), `docs/developer/porting/status-log.md`.

**Touches NOTHING in:** `memory_tasks.rs` / `memory_tasks/`, `generators/**`,
`builtin_prompt_templates.*`, `quilltap-host/src/{spine,host}.rs` (or any host
file), `quilltap-web`, `quilltap-cli`, `quilltap-tauri`, `help/`, `apps/web`.
No schema change.
