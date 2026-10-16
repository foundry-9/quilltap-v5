# Survey — Pascal TRIGGER PHRASES, the SERVER side of `362426c8f` (with the `f0db8cbb8` seam)

Read-only planning survey, 2026-10-16. v4 = `~/source/quilltap-server`, read ONLY
through `git show 25b8fab6a:<path>` (the PIN = v4 main HEAD
`25b8fab6aa58636c6885ba63684c7f8fbfca9c3b`, `4.11.0-dev.10`, clean),
`git show 362426c8f:<path>` (THIS commit's own state of a file the next commit
re-touches), `git diff 83334b9a5 362426c8f -- <paths>` (the hunks) and
`git diff 362426c8f 25b8fab6a -- <paths>` (what landed on top). **Every v4 line
number is AT THE PIN** unless marked `(c362)` (the file as `362426c8f` left it)
or `(pre)` (the baseline `83334b9a5` shape v5 ported). v5 =
`"/Volumes/External 1/source_overflow/quilltap-v5"` (`~/source/quilltap-v5`
symlinks to it) at main `56554d98c`; v5 line numbers are on that tree. Nothing
here is taken from commit prose (drift ledger §5.3); every claim is from shipped
hunks. The ledger's §3 row for `362426c8f` (`drift-ledger.md:316`) and the
`f0db8cbb8` row were read first and are built on; where a hunk disagrees with
the row, §9 says so.

**Scope (this survey):** `lib/pascal/custom-tool.types.ts` (the `triggers` key),
`lib/pascal/trigger-phrases.ts` (NEW), `lib/pascal/trigger-runner.ts` (NEW),
`lib/pascal/run-service.ts` (NEW — the extraction from the two ported entrances),
`lib/pascal/tool-draft.ts`, `lib/pascal/workbench.ts`, `lib/chat/carina-parser.ts`,
`lib/database/repositories/chats-messages.ops.ts` + `lib/schemas/chat.types.ts`
(the `pascalMeta` widening), `lib/services/chat-message/message-finalizer.service.ts`
+ `orchestrator.service.ts` + `trigger-phrase-sites.ts` (NEW), the two extracted
entrances (`lib/tools/handlers/run-custom-handler.ts`,
`app/api/v1/chats/[id]/custom-tools/route.ts`), `lib/tools/handlers/help-settings-handler.ts`,
`public/schemas/qtap-custom-tool.schema.json`, `public/schemas/qtap-export.schema.json`,
the eight test files, and the ONE read site of `chatSettings.customToolTriggers`.
**Out of scope (siblings):** the schema / migration / settings-route PUT /
repository default / Almanack / `prettify` hunks (the KEYSTONE survey owns the
column — this survey only names where it is READ), and `components/**` (the SPA
survey).

---

## 0. The span, in one paragraph

Between the oracle baseline `83334b9a5` and the pin `25b8fab6a` v4 landed 25
commits (§3 of the ledger); the slice files of THIS survey are untouched by every
one of them except two: `362426c8f` ("trigger phrases", `4.11.0-dev.1→.3`, 60
files, +2,158 / −320) and `f0db8cbb8` ("wardrobe effects", `.4`, 44 files,
+2,158 / −124) — `git diff --stat 83334b9a5 362426c8f~1` over the slice paths
is EMPTY, so the pre-shape every v5 port in this slice was made from IS the
baseline, and `git log 362426c8f..25b8fab6a -- <slice>` names only `f0db8cbb8`
(plus `6069a7acd`, bug 249, which touches `lib/services/chat-message/
turn-orchestrator.service.ts` only — not this slice; the turn-chain sibling).
`362426c8f` does six things on the server: (a) a tool definition may declare
`triggers` (1–8 phrases, 3–80 chars, stored as written, trim- and
case-fold-unique), carried by the JSON-Schema mirror, the Workbench draft
(round-trip + four rules + a self-echo WARNING) and the library's new
`triggerCount` wire key; (b) a pure scanner (`scanTriggerPhrases`) that strips
Carina query lines through a NEW `stripCarinaQueryLines` (the parser's loop body
extracted into `parseQueryLine`), folds with `toLocaleLowerCase`, takes ONE hit
per tool (its earliest phrase), orders by offset with stable ties, and caps at 4
per message with the rest reported as `dropped`; (c) a never-throwing runner
(`runTriggeredCustomTools`) gated on `customTools !== false && customToolTriggers
=== true`, resolving the roster from the SPEAKER's perspective (a character seat's
sheet + vault, or the bare operator as nobody), running each hit through the run
service with `invokedBy: 'trigger'` / `triggerPhrase` / ONE `Date.now()` for the
message, Prospero on a refusal or an unposted outcome, then on to the next;
(d) two call sites next to the auto-RNG scans — the orchestrator AFTER the user
message persists (with the in-memory `existingMessages` splice) and the
finalizer AFTER the reply is saved (no splice), both emitting the pre-existing
`pascalResult` SSE event; (e) the EXTRACTION of the deal — cascade → execute →
effects → Pascal's post — out of the model handler and the manual route into ONE
`runCustomTool` (`lib/pascal/run-service.ts`), which moves two DEBUG lines, adds
two more, and changes the manual route's whisper derivation (§1.9); and (f) the
`pascalMeta.invokedBy` enum widened to `'trigger'` + an optional `triggerPhrase`
in the row schema, the event schema AND the `.qtap` export schema. The ONE read
of the new column is `triggersEnabled` (`trigger-runner.ts:66-70`), fed by the
two sites from the `chatSettings` the spine already holds. `f0db8cbb8` then
re-touches `run-service.ts` (+12: a `pendingWardrobeAnnouncements` param and
`userId` on the applier call), `run-custom-handler.ts` (+8), `custom-tool.types.ts`
(+233), `tool-draft.ts` (+159), `chats-messages.ops.ts` / `chat.types.ts`
(comments) — none of which changes a trigger-phrase behaviour; §8 draws the seam.

---

## 1. The v4 hunks, file by file (AT THE PIN)

### 1.1 `lib/pascal/custom-tool.types.ts` (1,584 at the pin; 1,351 `(c362)`) — the `triggers` key

**Constants (`:88-94`, all three from `362426c8f`):**

```ts
export const MAX_TRIGGERS = 8;                 // :88 "Cap on trigger phrases per tool."
export const MIN_TRIGGER_PHRASE_LENGTH = 3;    // :91 "keeps "a" and "ok" out. A length, not a word count."
export const MAX_TRIGGER_PHRASE_LENGTH = 80;   // :94
```

They sit AFTER `MAX_EFFECTS = 16` (`:85`) and BEFORE `MAX_EFFECT_TARGET_LENGTH =
200` (`:97`). (`f0db8cbb8` later inserts `MAX_WARDROBE_EFFECT_ITEMS = 8` /
`MAX_WARDROBE_ITEM_REF_LENGTH = 200` after `MAX_EFFECT_TARGET_LENGTH`, `:99-103`.)

**The schema key (`:986-996`), placed AFTER `chipLabel` (`:977-985`) and BEFORE
`description` (`:997`):**

```ts
    triggers: z
      .array(z.string().min(MIN_TRIGGER_PHRASE_LENGTH).max(MAX_TRIGGER_PHRASE_LENGTH))
      .min(1)
      .max(MAX_TRIGGERS)
      .optional()
      .describe(
        'Phrases that run this tool automatically, with every parameter at its default, when a message from anyone ' +
          'in the chat contains one — exact substrings, case-insensitive, no wildcards. Only honoured when the chat ' +
          'setting "Auto-Run Custom Tools on Trigger Phrases" is on (it is off by default).'
      ),
```

The shape's declaration order at the pin (`:955-1035`): `$schema`, `name`,
`title`, `chipLabel`, **`triggers`**, `description`, `disabled`,
`availableWhen`, `withheldWhen`, `revealOdds`, `defaultVisibility`,
`parameters`, `roll`, `llm`, `effects`, `outcomes`. Zod walks a shape in
declaration order, so this is the ISSUE-ORDER contract (a `triggers` issue
sorts after a `chipLabel` one and before a `description` one) and —
through `KNOWN_TOP_LEVEL_KEYS = Object.keys(QtapCustomToolSchema.shape)`
(`:1572`) — `triggers` is a KNOWN top-level key automatically (v5 keeps a
hand-written array; §2.1).

Zod's own messages the lane must reproduce byte-for-byte (the definition
differential pins `formatDefinitionIssues` sentences — §4.1): an empty array →
`Too small: expected array to have >=1 items` at `triggers`; nine → `Too big:
expected array to have <=8 items`; `'ok'` → `Too small: expected string to have
>=3 characters` at `triggers.0`; 81 chars → `Too big: expected string to have
<=80 characters`; a non-string → `Invalid input: expected string, received
number`; `triggers: null` → `Invalid input: expected array, received null`
(the `.optional()` accepts `undefined` only). The exact Zod 4.6.5 spellings are
what the corpus will record — do not hand-write them; the existing corpus rows
(`chip-label-*`, `shape-order-title-chip-description`) show the house style.

**The superRefine chain (`:1038-1043`)** is now FIVE steps in source order:
the outcome-ordering check, `validateReferences`, `validateGates`,
`validateEffects`, **`validateTriggers`** (`:1042`, the one added line).

**`validateTriggers` (`:1127-1156`):**

```ts
function validateTriggers(tool: { triggers?: string[] }, ctx: z.RefinementCtx): void {
  if (!tool.triggers) return;
  const seen = new Map<string, number>();
  tool.triggers.forEach((phrase, index) => {
    if (phrase !== phrase.trim()) {
      ctx.addIssue({ code: 'custom', message: `trigger ${index + 1} has leading or trailing whitespace`, path: ['triggers', index] });
    }
    const folded = phrase.toLocaleLowerCase();
    const earlier = seen.get(folded);
    if (earlier !== undefined) {
      ctx.addIssue({ code: 'custom', message: `trigger ${index + 1} repeats trigger ${earlier + 1} (phrases match regardless of case)`, path: ['triggers', index] });
    } else {
      seen.set(folded, index);
    }
  });
}
```

Facts a port must keep: (i) BOTH rules are checked for one phrase — a padded
duplicate raises two issues at the same path, whitespace first; (ii) the
duplicate names the FIRST occurrence (`earlier + 1`), and a third copy also
names the first (the map is never updated for a repeat); (iii) `trim()` is JS
`String.prototype.trim` (the `jsstr::js_trim` whitespace class, NOT Rust's
`str::trim`); (iv) the fold is `toLocaleLowerCase()` with no locale argument —
the mention ranker's precedent — which the CLAUDE.md ICU ruling makes
byte-identical to `str::to_lowercase` (`mentioned_characters.rs:85,101` already
uses it); (v) the refine runs only when the array parsed — a `triggers` array
that failed the element checks never reaches it (Zod aborts the object refine
when a field issue exists, the same rule `validateEffects` lives under).

**Formatted sentence shape** (the route's `errors[].reason`): `triggers.1:
trigger 2 has leading or trailing whitespace; triggers.1: trigger 2 repeats
trigger 1 (phrases match regardless of case)` — the `'; '` join and the
`path.join('.')` prefix `formatDefinitionIssues` already applies (v5
`custom_tool_types.rs:3290-3318`).

### 1.2 `lib/pascal/trigger-phrases.ts` (92, NEW, pure, client-safe)

Whole file surveyed; the load-bearing lines:

- `:19` `import { stripCarinaQueryLines } from '@/lib/chat/carina-parser';` —
  the one dependency (the SPA's `tool-draft` imports this module, so the SPA
  twin must also carry the Carina strip or a stub; the SPA survey's concern).
- `:27` `export const MAX_TRIGGER_RUNS_PER_MESSAGE = 4;`
- `:29-36` `interface TriggerHit { tool: string; phrase: string; index: number }`
  — `phrase` is AS WRITTEN in the definition (not folded); `index` is the
  UTF-16 offset into the FOLDED, STRIPPED haystack ("for ordering only").
- `:39-41` `foldTriggerText(text) = text.toLocaleLowerCase()`.
- `:47-51` `triggersContainedIn(text, triggers)`: `[]` for empty text or no
  triggers; else the phrases (as written, in DEFINITION order) whose fold the
  folded text `includes`; a zero-length phrase is skipped. Used only by the
  Workbench self-echo warning (§1.5) — pure.
- `:59-92` `scanTriggerPhrases(content, tools)` →
  `{ hits: TriggerHit[]; dropped: TriggerHit[] }`:
  1. `:63` empty content → `{ hits: [], dropped: [] }`.
  2. `:65` `haystack = foldTriggerText(stripCarinaQueryLines(content))`;
     `:66` a haystack that is whitespace-only after the strip (`!haystack.trim()`)
     → empty result. Note the order: strip THEN fold.
  3. `:69-82` per tool (roster ITERATION order — `[...roster.tools.values()]`,
     i.e. the roster `Map`'s insertion order): skip a tool with no/empty
     `triggers`; for each phrase, `needle = fold(phrase)`, skip an empty needle,
     `index = haystack.indexOf(needle)` (UTF-16 `indexOf`), keep the LOWEST index
     — a strictly-lower test (`index < best.index`), so two phrases at the same
     offset keep the FIRST in definition order; one `TriggerHit` per tool.
  4. `:86` `all.sort((a, b) => a.index - b.index)` — V8's stable sort; ties keep
     roster order.
  5. `:88-91` `hits = all.slice(0, 4)`, `dropped = all.slice(4)`.
  
  Plain substring: `fire` matches `fireplace`; no word boundaries, no regex.

`__tests__/unit/lib/pascal/trigger-phrases.test.ts` (80) pins: fold both sides
(`'She cried ABRACADABRA!'` / `['AbraCadabra']` → `index: 10`); substring not
word; one hit per tool with the EARLIEST phrase (`'open sesame, then
abracadabra'` → `open sesame` at 0); order by appearance not roster; both of
`fire` / `firestorm` fire; cap 4 + `dropped: ['t4']`; a Carina line (`'@Alice:
abracadabra'`, `'@Alice? “abracadabra”'`) never scanned; spoken lines around a
query still scanned; empty content / a triggerless tool → empty; and
`triggersContainedIn` returning phrases AS WRITTEN (`'Abracadabra'`) and `[]`
for `undefined`.

### 1.3 `lib/chat/carina-parser.ts` (148; +72 / −27 in the span)

The refactor (`:89-112` NEW `parseQueryLine(line): CarinaQuery | null`) is the
old loop body verbatim: `LINE_RE.exec(line)` (`:53` unchanged regex
`^@(${NAME_SOURCE})([?:])\s*(.*)$`), `characterName = m[1].trim()` → null when
empty, `question = extractQuestion(m[3])` → null when empty (the comment moved:
"An `@Name:` with no question text isn't a usable query."), else `{
characterName, whisper: m[2] === '?', question }`. `parseCarinaQuery`
(`:118-131`) now loops `for (const line of content.split(/\r?\n/))` returning the
first non-null `parseQueryLine` — **behaviour byte-identical to the baseline**
(the test file's 400 existing cases are unchanged; the diff touches only the
import line and appends a `describe`).

NEW `stripCarinaQueryLines(text)` (`:140-148`):

```ts
export function stripCarinaQueryLines(text: string): string {
  if (!text) { return text; }
  return text.split(/\r?\n/).filter((line) => parseQueryLine(line) === null).join('\n');
}
```

Facts: empty text passes through AS-IS (`''` → `''`); EVERY line that parses as
a query is removed, not just the first; the rejoin is `'\n'` so CRLF is
NORMALISED to LF (`'a\r\n@Alice: q\r\nb'` → `'a\nb'`); a line that does not
parse (an `@Alice:` mid-line, a bare `@Alice:` with no question, `@X:` with a
one-char name) is KEPT verbatim; a lone `\r` is not a separator (the existing
`split_crlf_or_lf` rule, v5 `carina_parser.rs:150-167`). The appended
`describe('stripCarinaQueryLines')` (`lib/chat/__tests__/carina-parser.test.ts:428-445`)
pins exactly those four: `'@Alice: one\nsaid aloud\n@Bob? two'` → `'said
aloud'`; a middle line with smart quotes removed; three non-parsing lines kept
whole; CRLF → LF and `''` → `''`.

### 1.4 `lib/pascal/run-service.ts` (219 at the pin; 209 `(c362)`) — the ONE deal

`RunCustomToolParams` (`:48-81`):

| field | type | note |
|---|---|---|
| `userId`, `chatId` | string | |
| `chat?` | `ChatMetadata \| null` | "when the entrance already holds it — saves the cascade a read. Absent → read here." |
| `entry` | `DiscoveredCustomTool` | the CALLER resolves the roster |
| `parameters` | `Record \| null \| undefined` | null/undefined = every default |
| `private` | `boolean \| undefined` | undefined = the definition's default |
| `characterId` | `string \| null` | null = a run nobody made |
| `metadata` | `Record<string, unknown>` | `{}` for a run nobody made |
| `whisperTargetIds` | `string[] \| null` | null → the post is not targeted |
| `invokedBy` | `'llm' \| 'user' \| 'trigger'` | |
| `callerParticipantId` | `string \| null` | recorded in pascalMeta when set |
| `triggerPhrase?` | string | as written in the definition |
| `nowMs?` | number | defaults to `Date.now()` |
| `pendingWardrobeAnnouncements?` | `Set<string>` | **`f0db8cbb8` only** (`:76-80`) |

`RunCustomToolOutcome` (`:83-85`): `{ ok: true; result: CustomToolRunResult;
posted: MessageEvent | null; appliedEffects: AppliedEffect[] } | { ok: false;
reason: string; whisper: boolean }`.

`runCustomTool` (`:94-219`), in order:

1. `:96` `nowMs = params.nowMs ?? Date.now()`; `:100` `progress =
   flattenProgressions(metadata, nowMs)`.
2. `:104-121` the cascade: `chat = params.chat ?? (await
   getRepositories().chats.findById(chatId))`; if a chat, `resolveStateCascade({
   chat, groupScope: characterId ? { kind: 'character', characterId } : { kind:
   'none' } })`; ANY throw (the chat read included) → **DEBUG** `Custom tool run:
   state cascade unavailable, defaulting to {}` `{ context: 'pascal.run-service',
   chatId, tool: entry.definition.name, error }` and `cascade = null`. No chat
   → cascade stays null SILENTLY (no line).
3. `:123-148` `executeCustomTool(entry.definition, params.parameters ?? null, {
   private, metadata, progress, now: nowMs, state: cascade?.merged ?? {},
   ...(entry.definition.llm ? { llmInvoke: buildCustomToolLlmInvoker({ userId,
   chatId }) } : {}) })`; a `CustomToolRunError` → `whisper = params.private ??
   entry.definition.defaultVisibility === 'whisper'`, **DEBUG** `Custom tool run
   refused` `{ context, chatId, tool, invokedBy: params.invokedBy, reason:
   error.message }`, `return { ok: false, reason: error.message, whisper }`;
   anything else RETHROWN.
4. `:152-166` effects BEFORE the post: `result.effects?.length ?
   applyCustomToolEffects({ userId [f0db], chatId, toolName: result.tool,
   effects: result.effects, cascade, characterId, metadataSnapshot: metadata,
   nowMs, ...(pendingWardrobeAnnouncements [f0db]) }) : []`.
5. `:168-169` `whispered = result.visibility === 'whisper'`;
   `targetParticipantIds = whispered && params.whisperTargetIds?.length ?
   params.whisperTargetIds : null`.
6. `:171-176` `toolTitle = displayTitle(entry.definition)`;
   `buildPascalResultContent({ toolTitle, ...(chipLabel), message })`.
7. `:178-204` `postPascalResult({ chatId, content, opaqueContent,
   targetParticipantIds, pascalMeta: { tool, toolTitle, ...(chipLabel),
   definitionTier: entry.tier, definitionMountId: entry.mountPointId, params,
   rollForm, ...(notation), raw, ...(diceRolls), value, state, outcomeIndex,
   ...(metadataTested), ...(llm), ...(effects when non-empty), invokedBy:
   params.invokedBy, ...(params.invokedBy === 'trigger' && params.triggerPhrase ?
   { triggerPhrase } : {}), ...(params.callerParticipantId ? {
   callerParticipantId } : {}) } })`. **Key order: `invokedBy`, `triggerPhrase`,
   `callerParticipantId`** — `triggerPhrase` lands between the two, and ONLY on a
   `'trigger'` run (a `'user'` run carrying a `triggerPhrase` emits none —
   `run-service.test.ts:128-135` pins exactly this).
8. `:206-216` **DEBUG** `Custom tool dealt` `{ context, chatId, tool:
   result.tool, tier: entry.tier, invokedBy, state: result.state, effects:
   appliedEffects.length, whispered, posted: posted !== null }` — on EVERY
   successful deal at every entrance (NEW).
9. `:218` `return { ok: true, result, posted, appliedEffects }`.

Never posts Prospero (`:90-92`: "that stays with the entrance").

`__tests__/unit/lib/pascal/run-service.test.ts` (150) pins: deals against `{}`
when the cascade throws (`cascade: null` reaches the applier); reads the chat
itself when `chat` is absent (`findById` called with the id, THEN the cascade);
group scope `{kind:'character', characterId}` vs `{kind:'none'}`; effects
before post (an order array); effects stand when the post returns null
(`{ ok: true, posted: null }`, `appliedEffects` length 1); `invokedBy` ×3 with
`triggerPhrase` present ONLY for `'trigger'` and `callerParticipantId` always
echoed; whisper targets only when `private: true`; a refused run (`parameters:
{ nonsense: 1 }`) → `ok: false`, NO post, NO applier call.

### 1.5 `lib/pascal/tool-draft.ts` (1,807 at the pin) — the Workbench draft (SPA-side twin in v5)

- `:68` `import { foldTriggerText, triggersContainedIn } from './trigger-phrases';`
- `:258-259` `ToolDraft.triggers: string[]` ("Empty = no `triggers` key emitted"),
  declared after `chipLabel` (`:257`) and before `description`.
- `:317` `newDraft()` → `triggers: []`.
- `:702` `draftFromDefinition`: `triggers: [...(definition.triggers ?? [])]`.
- `:901` `definitionFromDraft`: `if (draft.triggers.length > 0) doc.triggers =
  [...draft.triggers];` — emitted right after `chipLabel` (`:900`) and before
  `description` (`:902`); `tool-draft.test.ts:930-942` pins the first five keys
  `['$schema','name','chipLabel','triggers','description']` and the absence of the
  key while empty.
- `:968` `DraftIssue.where` gains `| { section: 'triggers'; index?: number }`
  between the `identity` arm and the `gate` arm.
- `:1491` `validateDraft` calls `validateDraftTriggers(draft, issues)` right after
  `validateChipLabelPlaceholders` and before the `description` checks.
- `:1651-1690` `validateDraftTriggers`: returns early on an empty list; ERRORS —
  `at most 8 trigger phrases` (`{section:'triggers'}`, no index) when
  `length > 8`; per phrase (`{section:'triggers', index}`): `a trigger phrase
  needs at least 3 characters` ELSE `a trigger phrase is at most 80 characters`
  (an if/else — one of the two, never both); `a trigger phrase must not begin or
  end with a space`; `"${phrase}" is listed twice (phrases match regardless of
  case)` when the fold was already `seen` (the draft's `Set` names the REPEAT
  only, unlike the loader's "repeats trigger N"); then WARNINGS — for every
  outcome message, the chip label, and (when `llmEnabled`) the consult prompt,
  one per contained phrase: `${what} contains the trigger "${phrase}" — a
  character who repeats Pascal's line will set this tool off again` with `what`
  ∈ `this message` (`{section:'message', id}`) / `the chip label`
  (`{section:'identity', field:'chipLabel'}`) / `the consult prompt`
  (`{section:'llm', field:'prompt'}`).

`tool-draft.test.ts:917-982` pins: round-trip as written and in order; the key
position; `['ok',' abracadabra','Hocus','HOCUS']` → errors at indexes `[0,1,3]`
(all `error`); nine phrases → an error; the echo is a WARNING with the phrase
quoted; chip-label and consult-prompt echoes.

### 1.6 `lib/pascal/workbench.ts` (298)

`:43` `CustomToolLibraryEntry.triggerCount: number` ("How many trigger phrases
the tool listens for (0 = none)"), declared after `llm` and before
`parameterCount`; `:194` `triggerCount: entry.definition.triggers?.length ?? 0`
in `buildCustomToolLibrary`. A NEW wire key between `llm` and `parameterCount`
on every library row — `pascal_workbench_equivalence` (which compares the
canonicalised row against the oracle's `value`, `pascal_workbench_equivalence.rs:160`)
trips at the pin until v5 emits it.

### 1.7 `lib/pascal/trigger-runner.ts` (241, NEW)

- `:41` `CONTEXT = 'pascal.trigger-runner'`.
- `:43-48` `TriggeredSpeaker { participantId: string | null; characterId: string
  | null }`.
- `:50-63` `RunTriggeredCustomToolsParams { userId; chatId; content; speaker;
  projectId: string | null; characterIds: string[]; chatSettings:
  Pick<ChatSettings,'customTools'|'customToolTriggers'> | null | undefined;
  onPosted?: (message: MessageEvent) => void }`.
- **`:66-70` `triggersEnabled(cs) = cs?.customTools !== false &&
  cs?.customToolTriggers === true`** — THE read site of the keystone's column.
  `null`/`undefined` settings → false; an absent `customTools` → true-side
  (only an explicit `false` kills it); `customToolTriggers` must be exactly
  `true`.
- `:73-88` `runTriggeredCustomTools(params): Promise<MessageEvent[]>`: `:75`
  `if (!triggersEnabled(params.chatSettings) || !params.content?.trim()) return
  [];` — SILENT ("a log here would fire on every message of every chat");
  `:77-87` the whole run wrapped: any throw → **WARN** `Trigger-phrase scan
  failed; no custom tools fired` `{ context, chatId, speakerParticipantId:
  params.speaker.participantId, error: getErrorMessage(error) }` → `[]`.
- `:90-241` `runTriggered`:
  1. `:92` ONE `nowMs = Date.now()` for the whole message.
  2. `:98-124` perspective: `metadata = {}`, `characterMountPointId = null`; if
     `speaker.characterId`: `characters.findById` → not found → **WARN**
     `Trigger-phrase scan skipped — speaking character not found` `{ context,
     chatId, characterId }` → `[]`; else `metadata = character.metadata ?? {}`,
     `characterMountPointId = character.characterDocumentMountPointId ?? null`;
     a throw (a `CharacterVaultUnavailableError` or any read failure) → **WARN**
     `Trigger-phrase scan skipped — speaking character unreadable` `{ context,
     chatId, characterId, error }` → `[]` — "Nothing was asked for, so no
     Prospero bubble".
  3. `:128-136` `resolveCustomToolRoster({ userId, chatId, characterId:
     speaker.characterId, characterMountPointId, characterIds:
     params.characterIds, projectId: params.projectId, metadata })` — per call,
     never cached; a throw here propagates to the outer WARN.
  4. `:138-141` `scanTriggerPhrases(content, [...roster.tools.values()].map(e =>
     e.definition))`.
  5. `:143-150` `dropped.length > 0` → **WARN** `Trigger phrases over the
     per-message cap; later tools not fired` `{ context, chatId, fired:
     hits.map(h => h.tool), dropped: dropped.map(h => h.tool) }`.
  6. `:151` no hits → `[]`.
  7. `:156` `whisperTargetIds = speaker.participantId ? [speaker.participantId]
     : [userId]`.
  8. `:159-238` per hit, in order: `entry = roster.tools.get(hit.tool)`
     (`continue` if gone); `runCustomTool({ userId, chatId, entry, parameters:
     null, private: undefined, characterId: speaker.characterId, metadata,
     whisperTargetIds, invokedBy: 'trigger', callerParticipantId:
     speaker.participantId, triggerPhrase: hit.phrase, nowMs })` — a THROW
     becomes `{ ok: false, reason: getErrorMessage(error), whisper:
     entry.definition.defaultVisibility === 'whisper' }`;
     `!outcome.ok` → **DEBUG** `Triggered custom tool refused` `{ context,
     chatId, tool: hit.tool, phrase: hit.phrase, reason }` +
     `postProsperoCustomToolError({ chatId, toolName: hit.tool, reason,
     whisper: outcome.whisper, callerParticipantId: whisperTargetIds[0] })` +
     `continue`; `!outcome.posted` → **DEBUG** `Triggered custom tool ran but its
     announcement did not land` `{ context, chatId, tool }` + Prospero with
     `reason: 'the outcome could not be posted to the chat'`, `whisper:
     result.visibility === 'whisper'`, same caller + `continue`; else
     `params.onPosted?.(outcome.posted)`, `posted.push(...)`, **INFO** `Custom
     tool fired by trigger phrase` `{ context, chatId, tool, phrase,
     speakerParticipantId, invokedBy: 'trigger', state: outcome.result.state }`.
  9. `:240` return `posted` (firing order).

Note the Prospero `callerParticipantId` for a bare-operator refusal is the
`userId` (the manual route's rule: "a UUID no character's filter admits");
`postProsperoCustomToolError` (`lib/services/prospero-notifications/writer.ts:802-811`)
targets `whisper && callerParticipantId ? [it] : null`.

`__tests__/unit/lib/pascal/trigger-runner.test.ts` (203; the scanner real, the
roster / run service / Prospero mocked) pins: the gate (off, null settings,
master off → `[]` with NO `findById` and NO roster call; blank content → no
roster); perspective (a character speaker's roster call `{ characterId,
characterMountPointId: 'vault-1', characterIds, projectId, metadata: {blessed:
true} }` and the run's `{ characterId, metadata }`; the bare operator → NO
`findById`, roster `{ characterId: null, characterMountPointId: null, metadata:
{} }`, run `{ characterId: null, metadata: {}, whisperTargetIds: ['user-1'] }`;
a `CharacterVaultUnavailableError` → `[]`, no run, no Prospero); the deal (two
tools in order of appearance with `parameters: null, private: undefined,
invokedBy: 'trigger', triggerPhrase: 'draw my sword', callerParticipantId,
whisperTargetIds: [PARTICIPANT_ID]`, ONE `nowMs` shared by both runs, `posted`
ids in order, `onPosted` ×2; the cap → exactly 4 runs; a refused first run →
Prospero `{ chatId, toolName: 'broken', reason: 'no such parameter', whisper:
true, callerParticipantId }` and the second still posts; a THROWING run →
Prospero `{ toolName: 'spell', reason: 'boom' }` and `[]`; a throwing roster →
resolves `[]`; `'@Alice: abracadabra'` → no run).

### 1.8 `lib/services/chat-message/trigger-phrase-sites.ts` (107, NEW)

- `:19` `TriggerSettings = Pick<ChatSettings,'customTools'|'customToolTriggers'>
  | null | undefined`; `:21-24` `RoomShape { projectId?: string | null;
  participants: Array<{ type: string; characterId?: string | null }> }`.
- `:27-31` `roomCharacterIds(chat)` = participants with `type === 'CHARACTER'`
  AND a truthy `characterId`, in stored order (the roster's `'participant'`
  tier).
- `:54-76` `runUserMessageTriggers({ userId, chatId, chat, content,
  isContinueMode, userParticipant, userParticipantId, chatSettings,
  existingMessages, onPosted })`: `:55` `if (params.isContinueMode ||
  !params.content) return []` (NOTE: `!content` — an empty string; a
  whitespace-only one goes through to the runner's `trim` gate); speaker
  `{ participantId: userParticipantId ?? null, characterId: seat?.type ===
  'CHARACTER' && seat.characterId ? seat.characterId : null }` — the seat is
  `resolveRespondingParticipant`'s `userParticipant`, so a user-driven /
  impersonated CHARACTER seat speaks AS that character; `projectId:
  chat.projectId ?? null`; `:74` `params.existingMessages.push(...triggered)` —
  the in-memory splice "the RNG TOOL rows and Carina's public answer rely on".
- `:94-107` `runReplyTriggers({ userId, chatId, chat, content,
  characterParticipantId, characterId, chatSettings, onPosted })`: `:95`
  `if (!params.content) return []`; speaker `{ participantId:
  characterParticipantId, characterId }`; NO splice.

`__tests__/unit/lib/services/chat-message/trigger-phrase-sites.test.ts` (125)
pins the two seat derivations, `characterIds: ['char-1','char-2']` from a
3-participant room with a PERSONA in the middle, the splice (`[{id:'earlier'},
{id:'pascal-1'}]`), continue mode / empty content → no runner call — and THREE
SOURCE-TEXT assertions: `ledger.userMessagePersisted = true` precedes
`await runUserMessageTriggers(` in `orchestrator.service.ts`;
`saveAssistantMessage(` precedes `await runReplyTriggers(` inside
`finalizeMessageResponse`; `regenerate-swipe.service.ts` never mentions
`runUserMessageTriggers|runReplyTriggers|runTriggeredCustomTools`.

### 1.9 The two extracted entrances — what moved VERBATIM, what CHANGED

#### 1.9.1 `lib/tools/handlers/run-custom-handler.ts` (254 at the pin; 246 `(c362)`; −138 / +~30)

Imports (`:21-22`): `resolveCustomToolRoster, type DiscoveredCustomTool` and
`runCustomTool, type RunCustomToolOutcome`; GONE: `executeCustomTool`,
`CustomToolRunError`, `CustomToolRunResult`, `displayTitle`,
`applyCustomToolEffects`, `flattenProgressions`, `resolveStateCascade`,
`buildCustomToolLlmInvoker`, `buildPascalResultContent`, `postPascalResult`.

Line by line against the baseline `(pre)`:

| pre (baseline) | pin | verdict |
|---|---|---|
| `nowMs = Date.now()` at the top; the metadata read with its vault-unreadable `reportFailure` (`isPrivate === true` whisper) | `:120-137` UNCHANGED | verbatim |
| `const progress = flattenProgressions(metadata, nowMs)` before the roster | DELETED (the run service derives it) | moved |
| roster + unknown-tool `failure` (`No custom tool named "…". Available: …` / `This scene has none.`) | `:139-167` UNCHANGED | verbatim |
| `chat = chats.findById(chatId)`; `resolveStateCascade({chat, groupScope})`; catch → DEBUG `run_custom: state cascade unavailable, defaulting to {}` `{context:'run-custom-handler', chatId, error}` | the run service's `:104-121` → DEBUG `Custom tool run: state cascade unavailable, defaulting to {}` `{context:'pascal.run-service', chatId, tool, error}` | **line text + context + fields CHANGED** (gains `tool`) |
| `executeCustomTool(def, parameters ?? null, {private, metadata, progress, now, state, llmInvoke?})`; catch → `reason = CustomToolRunError ? message : getErrorMessage`; `reportFailure(…, whisper = isPrivate ?? defaultVisibility === 'whisper')` | `:172-201`: `runCustomTool({…, whisperTargetIds: callerParticipantId ? [it] : null, invokedBy:'llm', callerParticipantId ?? null, nowMs, ...(pendingWardrobeAnnouncements [f0db])})`; a THROW (non-`CustomToolRunError`, rethrown by the service) → `reportFailure(…, getErrorMessage(error), whisper)`; `!outcome.ok` → `reportFailure(context, toolName, outcome.reason, outcome.whisper)` | the model result + the WARN `Custom tool run failed` + the Prospero post are IDENTICAL for both arms; what is NEW is the preceding DEBUG `Custom tool run refused` from the service on the refused arm |
| effects BEFORE post via `applyCustomToolEffects({chatId, toolName, effects, cascade, characterId ?? null, metadataSnapshot, nowMs})` | the service's `:152-166` (same args + `userId` [f0db] + the set [f0db]) | verbatim (at c362); f0db adds two args |
| `whispered = result.visibility === 'whisper'`; `targetParticipantIds = whispered && callerParticipantId ? [it] : null` | the service's `:168-169` over `whisperTargetIds` | **equivalent** (the handler passes `[caller]` or null) |
| `pascalMeta` 17 keys with `invokedBy:'llm'` + `callerParticipantId` when set | the service's `:183-203` with `invokedBy: params.invokedBy` (= `'llm'`) and the `triggerPhrase` spread that is EMPTY here | **byte-identical for the LLM entrance** |
| `posted` null → `reportFailure(…, 'the outcome could not be posted to the chat', whispered)`; INFO `Custom tool run completed` `{context, userId, chatId, tool, tier, rollForm, value, state, outcomeIndex, whispered}`; the success output | `:206-239` UNCHANGED | verbatim |
| (none) | the service's DEBUG `Custom tool dealt` before the handler's INFO | NEW line |

#### 1.9.2 `app/api/v1/chats/[id]/custom-tools/route.ts` (509 at the pin; −173 / +~40)

`handleRun` (`:374-507`). Verbatim from the baseline: the chat / perspectives /
`preferOperator` / `resolveForPerspective` / unknown-tool `badRequest` block
(`:380-426`, incl. DEBUG `Manual custom-tool run named an unknown tool`). Then:

| pre (baseline) | pin | verdict |
|---|---|---|
| `whispered = body.private === true`; `targetParticipantIds = whispered ? [ctx.user.id] : null` | `:451-462` `runCustomTool({ userId: ctx.user.id, chatId: id, chat, entry, parameters: body.parameters ?? null, private: body.private, characterId: body.asCharacterId ? perspective.characterId : null, metadata: body.asCharacterId ? perspective.metadata : {}, whisperTargetIds: [ctx.user.id], invokedBy: 'user', callerParticipantId: null })`; `:485` `whispered = result.visibility === 'whisper'` | **BEHAVIOUR CHANGE #1:** a tool with `defaultVisibility: 'whisper'` run manually with `private` ABSENT was posted UNTARGETED (`body.private === true` false) at the baseline; at the pin it is targeted to `[ctx.user.id]` (visibility is the DEFINITION's) — and the response's `result.whispered` flips from `false` to `true` for that case |
| `metadata = body.asCharacterId ? perspective.metadata : {}`; `nowMs = Date.now()`; `progress = flattenProgressions(metadata, nowMs)` | the service derives both (`nowMs` from `Date.now()` inside the service — the route passes none) | moved |
| `resolveStateCascade({ chat, groupScope: body.asCharacterId ? {kind:'character', characterId: body.asCharacterId} : {kind:'none'} })`; catch → DEBUG `Manual custom-tool run: state cascade unavailable, defaulting to {}` `{context: HANDLER, chatId, error}` | the service's line (`Custom tool run: state cascade unavailable…`, `pascal.run-service`, + `tool`) | **line text + context + fields CHANGED**; the scope is equivalent (`perspective.characterId === body.asCharacterId` when named) |
| `executeCustomTool(def, body.parameters ?? undefined, …)` | `params.parameters ?? null` | equivalent (`resolveParams` treats both as "none"; v5 already takes `Option`) |
| `CustomToolRunError` → DEBUG `Manual custom-tool run refused` `{context: HANDLER, chatId, tool: body.tool, reason}`; Prospero `{ whisper: whispered (= body.private === true), callerParticipantId: whispered ? ctx.user.id : null }`; `badRequest(error.message)` | `:464-482` on `!outcome.ok`: the SAME DEBUG line (survives) AFTER the service's `Custom tool run refused`; Prospero `{ whisper: outcome.whisper (= body.private ?? defaultVisibility === 'whisper'), callerParticipantId: outcome.whisper ? ctx.user.id : null }`; `badRequest(outcome.reason)` | **BEHAVIOUR CHANGE #2:** the refusal bubble of a default-whisper tool with `private` absent is now WHISPERED to the operator (was public) |
| a non-`CustomToolRunError` throw → rethrown out of `handleRun` (→ the middleware's 500) | the service rethrows; `handleRun` has NO `try` around `runCustomTool` → the same 500 | verbatim |
| effects `applyCustomToolEffects({ chatId, toolName, effects, cascade, characterId: body.asCharacterId ? perspective.characterId : null, metadataSnapshot, nowMs })` | the service's | verbatim (c362) |
| `pascalMeta` with `invokedBy: 'user'` and NO `callerParticipantId` | the service's with `callerParticipantId: null` → key omitted | byte-identical |
| INFO `[Pascal] Manual custom-tool run completed` `{context, chatId, tool, state, whispered}`; `successResponse({ messages: pascalMessage ? [pascalMessage] : [], result: { tool, value, state, message, whispered } })` | `:487-506` UNCHANGED (but `whispered` now the definition-derived value) | verbatim text; the VALUE moves with change #1 |

Both changes are consequences of the extraction (the service computes
`whispered` the handler's way), not separate hunks; they are PORT facts because
`pascal_custom_tools_route_equivalence` diffs `{status, body}` + the posted
rows, so a corpus row with a `defaultVisibility: 'whisper'` tool and no
`private` would flip. Check the existing `whispered` fixture tool's rows
(§4.3) — the fixture's `whispered.tool.json` is exactly that tool.

### 1.10 The two sites

**`orchestrator.service.ts` (2,245):** `:34` `import { runUserMessageTriggers }
from './trigger-phrase-sites'`; `:67` `encodePascalResultEvent` joins the
streaming import (it was already exported — `streaming.service.ts:870-875`,
`data: ${JSON.stringify({ pascalResult: message })}\n\n`, and already used at
`:1361` for the tool loop's `emitPascalResult`). The call (`:881-892`) sits
after the user message persists (`:829` `ledger.userMessagePersisted = true`,
inside `if (!isContinueMode && options.content)` `:813-840`) and the RNG
auto-detect (`:766-810`, which runs BEFORE the persist so its TOOL rows precede
the user message), and BEFORE the Carina user-message path (`:894-930`):

```ts
  await runUserMessageTriggers({
    userId, chatId, chat, content, isContinueMode, userParticipant, userParticipantId, chatSettings,
    existingMessages,
    onPosted: (message) => safeEnqueue(controller, encodePascalResultEvent(encoder, message)),
  })
```

`content` is the local `const content` (`''` in continue mode, `:811`);
`userParticipant` / `userParticipantId` are `resolveRespondingParticipant`'s
(`:386-401`); `chatSettings` is `repos.chatSettings.findByUserId(userId)`
(`:476`) — the SAME object the auto-RNG gate reads, so no new read.

**`message-finalizer.service.ts` (761):** `:28` `encodePascalResultEvent` added to
the streaming import; `:39` `import { runReplyTriggers }`. The call (`:453-462`)
sits after the saved reply + the response RNG auto-detect (`:382-432`) and
BEFORE the Carina assistant-markup path (`:464-495`) and the done event
(`:497`):

```ts
  await runReplyTriggers({
    userId, chatId, chat, content: cleanedResponse,
    characterParticipantId: characterParticipant.id, characterId: character.id,
    chatSettings,
    onPosted: (msg) => safeEnqueue(controller, encodePascalResultEvent(encoder, msg)),
  })
```

`chatSettings` is `triggers.chatSettings` (`:91`; `types.ts:400-406`
`TriggerContext.chatSettings: ChatSettings | null`). The comment block
(`:441-452`) records "No wardrobe announcement set either — the turn's
tool-execution context is already drained here" and "A swipe never reaches this
path".

`message-finalizer.service.test.ts` (+21, `:586-599`) pins, inside the existing
`Please roll 1d20` RNG case: the runner called ONCE with `{ userId, chatId,
content: 'Please roll 1d20', speaker: { participantId: 'participant-1',
characterId: 'char-1' }, characterIds: ['char-1','char-2'] }` and its
`invocationCallOrder` AFTER every `repos.chats.addMessage` call (the reply AND
the RNG TOOL row).

### 1.11 `pascalMeta` — the row, the event, the export schema

- `chats-messages.ops.ts:171-172` (`ChatMessageRowSchema.pascalMeta`):
  `invokedBy: z.enum(['llm', 'user', 'trigger'])`, `triggerPhrase:
  z.string().optional()` — declared between `invokedBy` and
  `callerParticipantId` (`:173`). Comment `:126-127`: "invokedBy = model reach
  vs. user Run-Tool vs. trigger phrase (triggerPhrase = the phrase as written in
  the definition)".
- `chat.types.ts:553-555` (`MessageEventSchema.pascalMeta`): the same two
  lines, same position; prose `:474-478`.
- `public/schemas/qtap-export.schema.json:972-973`: `"invokedBy": { "enum":
  ["llm", "user", "trigger"], "description": "Whether the model reached for the
  tool, the user ran it themselves, or a trigger phrase said in the chat set it
  off." }` and NEW `"triggerPhrase": { "type": "string", "description": "The
  phrase that fired a triggered run, as written in the definition. Present only
  when invokedBy is 'trigger'." }` — **IN THIS COMMIT** (+3/−1; `f0db8cbb8`
  touches this file for ONE `effects` description, +2/−2). The `required` list
  (`:929`) is unchanged.
- `public/schemas/qtap-custom-tool.schema.json:35-42`: the `triggers` property
  (`type: array`, `minItems 1`, `maxItems 8`, `uniqueItems: true`, items
  `{type: string, minLength 3, maxLength 80}`, the long description ending
  "Never sent to the model."), after `chipLabel` and before `description`.
  `custom-tool-definition.test.ts:699-709` pins the mirror as "deliberately
  weaker than Zod" — `uniqueItems` is case-SENSITIVE and there is no trim rule
  (`['Abracadabra','abracadabra']` and `[' abracadabra']` validate against the
  mirror, fail the loader).

### 1.12 `lib/tools/handlers/help-settings-handler.ts` (288)

`:113` `customToolTriggers: settings.customToolTriggers,` in
`fetchCategorySettings`'s `chat` bag, right after `customTools` (`:112`) and
before `llmLoggingSettings`. The `help_settings` tool OUTPUT grows by one key
for the `chat` category — the P4.d6 `customTools` precedent: `help_tools_equivalence`
compares the output EXACTLY, so it trips at the pin until v5's bag
(`tools/help.rs:215`) emits it.

### 1.13 `__tests__/unit/lib/pascal/custom-tool-definition.test.ts` (+44)

Mirror-agreement rows (`:639-644`): a trigger list, an empty list, `['ok']`,
`'x'.repeat(81)`, nine phrases, an exact duplicate — each accepted/rejected
IDENTICALLY by Ajv over the mirror and by Zod; the "weaker" test (`:701-711`);
and `describe('triggers')` (`:724-748`): stored AS WRITTEN (`['Open Sesame']`);
`['abracadabra', 'open sesame ']` → contains `trigger 2 has leading or trailing
whitespace`; `['Abracadabra','ABRACADABRA']` → contains `trigger 2 repeats
trigger 1`; `KNOWN_TOP_LEVEL_KEYS` contains `'triggers'`.

### 1.14 The one NO-PORT test in the slice

`__tests__/unit/app/api/v1/settings/chat/route.custom-tool-triggers.test.ts`
(49, NEW) is the KEYSTONE's (the PUT arm, `true`/`false` echoed, `'yes'` → ≥400
with no write, the Zod default `false`) — named here only because the brief
listed the seven test files; it belongs to the column's lane.

---

## 2. v5 TODAY, per surface (main `56554d98c`)

### 2.1 The definition — `crates/quilltap-core/src/pascal/custom_tool_types.rs` (3,424)

- Constants `:81-127`: `MAX_CHIP_LABEL_LENGTH = 160` (`:101`), `MAX_EFFECTS = 16`
  (`:104`), `MAX_EFFECT_TARGET_LENGTH = 200` (`:107`). **Missing:** the three
  trigger constants (insert after `MAX_EFFECTS`, matching v4's order).
- `QtapCustomTool` (`:980-1010`): `chip_label: Option<String>` (`:990-991`,
  `#[serde(rename = "chipLabel", skip_serializing_if)]`) then `description`
  (`:992`). **Missing:** `pub triggers: Option<Vec<String>>` between them
  (serialize `skip_serializing_if = "Option::is_none"` — v4 keeps `undefined`
  absent).
- `safe_parse` assembly: `chipLabel` parsed at `:2482-2489` ("Positioned
  between `title` and `description` … the issue ORDER contract"); the struct
  literal `:2668-2684`; the superRefine chain `:2683-2686`
  (`validate_outcome_ordering`, `validate_references`, `validate_gates`,
  `validate_effects`). **Missing:** a `triggers` arm between `chip_label` and
  `description` (array-of-string parse with Zod's `Too small`/`Too big`
  sentences for both the array and each element, `Invalid input: expected
  array, received null` for null — the house helpers `parse_string` (`:2484`)
  and whatever `parse_array`-shaped helper `parameters` / `effects` use) and a
  fifth `validate_triggers(&tool, &mut issues)` after `validate_effects`.
- `KNOWN_TOP_LEVEL_KEYS: [&str; 15]` (`:3321-3337`), a HAND-WRITTEN array —
  **must become 16 with `"triggers"` after `"chipLabel"`** or
  `collect_unknown_keys` (`:3342`) reports `triggers` as unknown. (v5's roster
  currently discards the list — `roster.rs:198` `let _unknown_keys = …` — so the
  effect is only the definition differential's `unknownKeys` row, which the
  corpus pins; v4 logs DEBUG `Custom tool carries keys this build does not know`
  at `custom-tools.ts:294`, which v5 never ported.)
- `display_title(name, title)` `:2705`; `format_definition_issues` `:3290-3318`.
- The SPA twin `apps/web/src/app/pascal/custom-tool-types.ts` (2,225) shares
  the corpus `apps/web/src/testing/fixtures/pascal-custom-tool-definition.oracle.ndjson`
  (`custom-tool-types.corpus.spec.ts:40`) — the SPA survey owns it; named here
  because the corpus rows this lane ADDS flow to both consumers.

### 2.2 The scanner + Carina strip

- `crates/quilltap-core/src/carina_parser.rs` (246): `LINE_RE` `:71-78`,
  `extract_question` `:85-109`, `parse_carina_query(Option<&str>)` `:113-146`
  (the loop body INLINE — the `parseQueryLine` extraction has no twin),
  `split_crlf_or_lf` `:150-167`. **Missing:** `parse_query_line(&str) ->
  Option<CarinaQuery>` (extract the body of `:122-144` — the behaviour of
  `parse_carina_query` must stay byte-equal; `carina_parser_equivalence` proves
  it) and `pub fn strip_carina_query_lines(text: &str) -> String` (empty → empty
  `String`; `split_crlf_or_lf` → filter `parse_query_line(..).is_none()` →
  `join("\n")`).
- No `pascal/trigger_phrases.rs`. **Missing whole:** `MAX_TRIGGER_RUNS_PER_MESSAGE`,
  `TriggerHit { tool, phrase, index }` (`index` a UTF-16 offset — `jsstr::js_index_of`
  `message_finalizer.rs:1253` is the twin of `indexOf`; `crate::jsstr::utf16_len`),
  `fold_trigger_text` (= `str::to_lowercase`, the CLAUDE.md byte-identity
  ruling; `mentioned_characters.rs:85,101` precedent), `triggers_contained_in`,
  `scan_trigger_phrases` (stable sort → `sort_by_key` / `sort_by` on
  `Vec::sort_by` which IS stable).

### 2.3 The run service — what v5 has instead

v5 has NO run service; the deal is duplicated in the two entrances, with ONE
shared helper already extracted:

- `crates/quilltap-core/src/tools/run_custom.rs` (965): `RunCustomToolContext`
  `:444-465` (`user_id, chat_id, character_id, character_mount_point_id,
  character_ids, project_id, caller_participant_id, now_ms` — NO `on_posted`:
  "v5 returns the posted message instead", `:440-443`);
  `execute_run_custom_tool` `:529` → `_with_consult(db, ctx, input, rng,
  llm_invoke: Option<&dyn LlmInvoker>) -> (RunCustomToolOutput,
  Option<PostedPascalMessage>)` `:551`; the deal: cascade `:683-715` (reads the
  chat + `resolve_state_cascade` inside `db.read_main`, degraded-mount
  fallback, `unwrap_or(None)` — **NO log line** where v4 had `run_custom: state
  cascade unavailable…`), `tool_state` `:716-719`, `progress` `:722-729`,
  `execute_custom_tool(&def, parameters.as_ref(), is_private, metadata_arg,
  Some(&tool_state), rng, llm_invoke, Some(&progress), ctx.now_ms)` `:731-756`
  (`CustomToolRunError(reason)` → `report_failure` `:749-752`),
  `apply_effects_for_run` `:761-770`, whisper targets `:772-779`, `pascal_meta`
  assembled field-for-field `:784-845` (`invokedBy` HARD-CODED `"llm"` `:842`;
  `callerParticipantId` `:843-845`), `post_pascal_result` `:847-856`, the
  unposted arm `:858-871`, the output `:873-886`. **The handler emits NO tracing
  line at all** (`grep tracing:: run_custom.rs` → none): v4's WARN `Custom tool
  run failed`, WARN `run_custom validation failed`, WARN `run_custom named an
  unknown tool`, INFO `Custom tool run completed` were never ported (see §9).
- `crates/quilltap-core/src/api/custom_tools.rs` (1,633): `chat_custom_tool_run`
  `:645-1001`; the prep closure `:675-795` (perspectives, `prefer_operator`,
  roster, metadata asymmetry, cascade INSIDE the read closure `:755-760`);
  **`whispered = private == Some(true)` `:802`** (the BASELINE rule — §1.9.2
  change #1 lands here); the invoker arm `:813-836`; progress `:843-847`;
  execute `:850-866`; the refused arm `:867-885` (Prospero `whisper: whispered`,
  `caller_participant_id: if whispered { Some(user_id) }` — change #2 lands
  here; **no DEBUG `Manual custom-tool run refused` line** — never ported);
  effects `:893-901`; `pascal_meta` `:908-966` (`invokedBy` `"user"` `:966`, no
  caller); post `:974-983`; the response `:985-1000`. The ONE tracing line in
  the file is the LIST handler's DEBUG `Custom-tool roster resolved for the
  operator` (`:508-516`).
- `crates/quilltap-core/src/pascal/side_effects.rs`: `ApplyCustomToolEffectsParams`
  `:126-145` (`chat_id, tool_name, effects, cascade, character_id,
  metadata_snapshot, now_ms` — NO `user_id`, NO announcement set: the
  `f0db8cbb8` seam, §8); `apply_custom_tool_effects(writers, params)` `:176`;
  **`apply_effects_for_run(db, chat_id, &result, cascade, character_id,
  &metadata, now_ms) -> Vec<AppliedEffect>`** `:825-870` — the only already-shared
  piece of the deal (both entrances call it: `run_custom.rs:761`,
  `custom_tools.rs:893`).
- `crates/quilltap-core/src/services/pascal_writer.rs`: `PostPascalResultParams`
  `:77-91` (`pascal_meta: Value`, `target_participant_ids: Option<Vec<String>>`),
  `build_pascal_message` `:108-144` (validates the assembled object into
  `ChatEventInput` — a malformed `pascalMeta` fails the WHOLE post → `None`),
  `post_pascal_result` `:149-170` (chat-exists guard, `Err → None`). **No
  `[Pascal] Custom-tool outcome posted` INFO / `Failed to post` ERROR lines
  ported.**
- `crates/quilltap-core/src/services/prospero_notifications.rs`:
  `ProsperoCustomToolErrorArgs { chat_id, tool_name, reason, whisper,
  caller_participant_id }` `:735-746`, `post_prospero_custom_tool_error(db,
  args) -> Option<Value>` `:750-770`.
- `crates/quilltap-core/src/pascal/roster.rs`: `RosterContext` `:104-123`
  (`metadata: Option<Map>` — `Some({})` is v4's truthy `{}`),
  `resolve_custom_tool_roster(&ctx, main, mount, roster_now_ms)` `:588-596`
  (SYNC, over two connections — the runner must call it inside
  `db.read_main(|main| db.read_mount_index(|mount| …))` as the route does at
  `:675-795`).
- `crates/quilltap-core/src/pascal/llm_consult.rs` + the `SeamConsultInvoker`
  the executor builds (`tools/executor.rs:690-700`) — the runner's consult seam
  must be threaded the same way the executor's `run_run_custom` does (the
  trigger runner runs INSIDE the spine, where the `ConsultRunner` is in hand).

### 2.4 `pascalMeta` in Rust

`crates/quilltap-core/src/db/chats_messages.rs`: `ChatEventInput.pascal_meta:
Option<PascalMetaIn>` `:165`; `PascalMetaIn` `:285-333` — `invoked_by: String`
(`:330`, **a plain String, NOT an enum**) then `caller_participant_id:
Option<String>` `:331-332`. **Missing:** `#[serde(default, skip_serializing_if
= "Option::is_none")] pub trigger_phrase: Option<String>` BETWEEN them (the key
order the wire carries). Because `invoked_by` is a String, a v4-4.11-written
row with `'trigger'` already round-trips through v5's read/insert (§9 #2); the
round-trip unit test `:1175-1189` is the place to add a `'trigger'` +
`triggerPhrase` row. `api/types.rs:2434` mentions `invokedBy:'user'` in a doc
comment only. The `.qtap` / backup / restore carriers carry `pascalMeta` as the
row's opaque JSON (`grep pascalMeta services/carriers services/backup` → none),
so no carrier code changes; the VENDORED export schema
`crates/quilltap-core/src/generators/qtap-export.schema.json:972` still has
`["llm", "user"]` and no `triggerPhrase` — **re-vendor** (the
`qtap_schema_validate_equivalence` family runs Ajv-equivalent validation over
it; a 4.11 export carrying `'trigger'` would be REFUSED by v5's validator until
then — the real live-data hazard, narrower than the ledger's).

### 2.5 The chat spine — where the two sites go

- `crates/quilltap-core/src/services/orchestrator.rs` (3,800+):
  `OrchestratorSeams::chat_settings(&self, user_id) -> Option<OrchestratorChatSettings>`
  `:276-278` (the ONE settings read, `:1379`); `OrchestratorChatSettings`
  `:290-330` carries `auto_detect_rng` (`:300`) and `custom_tools` (`:304`) —
  **missing `custom_tool_triggers: bool`** (the keystone's column reaches the
  spine through THIS struct; the lane adds the field + its population in the
  production seam impl and in the tier-3 recording seam). The user-message
  path: RNG `:1866-1922` (BEFORE the persist, as v4), the persist `:1925-1960`
  (`user_message_persisted: true` `:1952`, DEBUG `[Orchestrator] User message
  persisted` `:1955`), the dangerFlags retirement note `:1962-1968`, the
  paused-chat hold `:1970-2007`, then `final_user_message` `:2009`. **The Carina
  user-message path is a STANDING DEFERRAL** (`:1991-2003`, the module note
  `:45-48`: `OrchestratorSeams::user_message_carina`, never wired). **The
  trigger site goes between the persist (`:1960`) and the paused-chat hold
  (`:1970`)** — v4's `:881` sits after the persist, before Carina, and v4's
  paused-chat return (`:872-887` in v4's numbering per the v5 note) is AFTER
  Carina; the site must run on a HELD turn too (v4 runs it — the paused return
  is below it). `existing_messages` is the `Vec<Value>` the RNG rows push onto
  (`:1918`); `resolve_responding_participant` `:1254-1278` yields
  `resolution.user_participant: Option<Value>` (`participant_resolver.rs:144`,
  the seat object with `type` / `characterId`) and `user_participant_id`
  (`:146`). The `Event` sink: `ChatEvent::pascal_result(message)`
  (`chat_events.rs:575-580`; the variant `:357-360`; SSE `{pascalResult: msg}`
  `:975-980`) — already consumed by the SPA reducer
  (`apps/web/src/app/core/chat-stream.reducer.ts:292-293`, dedupe by id) and
  typed at `core-contract.ts:5078`.
- `crates/quilltap-core/src/services/message_finalizer.rs` (2,020):
  `FinalizerChatSettings` `:518-527` (`cheap_llm_settings_present,
  auto_detect_rng: Option<bool>, answer_confirmation_global_enabled`) —
  **missing `custom_tools: Option<bool>` + `custom_tool_triggers: Option<bool>`**
  (v4's `Pick` reads both off the full settings; the finalizer never had
  `customTools`); `FinalizeOptions.chat_settings` `:622`,
  `.character_participant: FinalizerParticipant` `:591` (`.id`),
  `.chat: FinalizerChat` (needs `project_id` + the participants list for
  `roomCharacterIds` — check the struct carries them; `calculate_next_speaker`
  `:1739` reads the chat's participants, so they are there or loadable). The
  reply path: save `:1028`, RNG `:1231-1310`, **the trigger site goes between
  the RNG block (`:1310`) and the Carina call (`:1312-1322`)**, then the chat
  bump `:1324-1336`, next speaker `:1339-1350`, done `:1352`. The orchestrator
  populates `FinalizerChatSettings` at `:3685-3690` — extend there.
- `regenerate-swipe` — v5's swipe path (`services/regenerate_swipe*.rs` or the
  `chat_continuation` family; grep `fn regenerate_swipe`) must stay free of the
  runner (v4's source-text assertion; a v5 `grep`-style guard test is the
  cheapest twin).
- `crates/quilltap-core/src/tools/executor.rs:660-700` (`run_run_custom`): the
  executor builds `RunCustomToolContext` + `SeamConsultInvoker` and calls
  `execute_run_custom_tool_with_consult`; its note `:680-685` records that the
  `pascalResult` splice is left to the finalizer / post-turn refresh. The
  trigger sites DO have the sink in hand (they are in the spine), so they emit
  `ChatEvent::pascal_result` directly — v4's `onPosted` shape.

### 2.6 The custom-tools route + web shim

`api/types.rs:2423-2444` `Request::ChatCustomToolsList` / `ChatCustomToolRun`
(`chat_id, tool, parameters, private, as_character_id`); `engine.rs:5066` the
arm; `quilltap-web/src/custom_tools_routes.rs:60-75` `custom_tools_post`
(`AVAILABLE = ["run"]`). **No new action, no new route, no new request
variant** in this slice — the trigger lane adds nothing to
`web_edge_body_parse_guard.rs`'s `COLLAPSE_CENSUS` (the `custom_tools_routes.rs`
rows at `:37` / `:172` stay as counted) nor to the dispatcher action lists; the
keystone's settings PUT is the only route hunk and it is a `bool_field` sibling
in `api/settings.rs:937-941`.

### 2.7 The Workbench library + the Workbench SPA

`crates/quilltap-core/src/pascal/workbench.rs`: `CustomToolLibraryEntry`
`:50-72` (`llm: bool` `:65`, `parameter_count` `:66`); the builder `:380-400`.
**Missing:** `pub trigger_count: usize` between `llm` and `parameter_count`
(`#[serde(rename = "triggerCount")]`, `entry.definition.triggers.as_ref().map_or(0,
Vec::len)`). SPA (the SPA survey's): `apps/web/src/app/screens/custom-tools/
{builder-form.ts, workbench-library.ts (:296 reads parameterCount),
workbench.api.ts, workbench-editor.ts}`, `apps/web/src/app/pascal/tool-draft.ts`
(`ToolDraft` `:215`, `chipLabel` `:219`, `newDraft` `:290`,
`draftFromDefinition` `:599`/`:620`, `definitionFromDraft` `:817`/`:824`,
`DraftIssue` `:888-893`, `validateDraft` `:1432`), `tool-draft.spec.ts`, the
e2e specs `apps/web/e2e/{workbench-flow,workbench-gate-flow,
workbench-progress-flow,salon-custom-tools-flow}.spec.ts`, and
`apps/web/src/app/screens/settings/chat/automation-settings.ts`.

### 2.8 `help_settings`, the Almanack, the chat-settings column (named, not surveyed)

`tools/help.rs:215` `obj.insert("customTools".into(), field(&s, "customTools"))`
— the lane (or the keystone; the order must say which) adds the
`customToolTriggers` line right after. `db/chat_settings.rs`: struct `:815`
`custom_tools`, create `:1052`/`:1089`, update `:1192-1194`, tolerant list
`:1378`, read `:1486-1491` (absent → `true`; the new column reads absent →
`false`), the `chat_settings_column_sites_guard` census
(`ADOPTED_BOOLEAN_COLUMNS`); the ensure shape
`db/chat_settings_impersonation_voice_mode_repair.rs` (header `:1-40`) and the
`restore_vintage_state` boot-align list (`restore_vintage_state.rs:109-158`);
`almanack/phase3_ledgers.rs:616,750`, `render.rs:1118`. All KEYSTONE.

### 2.9 The vendored schema + the vendor guard

`apps/web/public/schemas/qtap-custom-tool.schema.json` is a byte-copy guarded
by `public_schemas_vendor_guard.rs:48-53` (`bytes: 39_471` at `25f534c0b`) — at
the pin the file is 883 lines after BOTH commits (`+8` triggers, `+102`
wardrobe); the trigger lane's copy must be `git show 362426c8f:public/schemas/
qtap-custom-tool.schema.json` ONLY IF the wardrobe lane is not stacked in the
same round; if it is, ONE copy at the pin and ONE byte count, owned by whichever
lane the order names (§8).

### 2.10 Harness families in this slice (each: test file → oracle case → recipe venue → what it covers)

| family (`crates/quilltap-harness/tests/`) | oracle case (`harness/oracle/cases/`) | recipe venue | covers | trigger impact |
|---|---|---|---|---|
| `pascal_custom_tool_definition_equivalence.rs` | `pascal-custom-tool-definition.ts` (tsx, `TZ=UTC`) | pin worktree | verdict + parsed data + unknownKeys + full rejection sentence; gate rows | EXTEND: triggers rows (§4.1); trips on `KNOWN_TOP_LEVEL_KEYS` otherwise unchanged |
| `pascal_run_custom_handler_equivalence.rs` | `pascal-run-custom-handler.test.ts` (jest, `/tmp/qt-pascal-mirror`) | pin, fixture copied to /tmp | the LLM entrance over `pascal-run-custom-{main,mount}.db`; model output + posted rows; `nowMs` pin | regen at pin: should stay GREEN (the LLM arm is byte-identical, §1.9.1) — proves the extraction; NEW log lines not pinned here |
| `pascal_custom_tools_route_equivalence.rs` | `pascal-custom-tools-route.test.ts` (jest) | pin | GET roster + POST run `{status, body}` + rows | regen at pin: the `whispered` fixture tool rows may FLIP (§1.9.2) — measure |
| `pascal_workbench_equivalence.rs` | `pascal-workbench.test.ts` (jest, `workbench-{main,mount}.db`) | pin | the library rows | RED at pin on `triggerCount` until emitted |
| `pascal_workbench_route_equivalence.rs` | `pascal-workbench-route.test.ts` + `workbench-route-cases.json` | pin | preview / audit / library / destinations | library leg RED on `triggerCount` |
| `pascal_writers_equivalence.rs` | `pascal-writers.ts` (tsx) | pin | pure bodies + the reason normaliser | unchanged |
| `pascal_side_effects_equivalence.rs` | `pascal-side-effects.test.ts` + `pascal-side-effects.json` | pin | the applier plan + WARN capture lines (`captured_with`) | unchanged by triggers; the `f0db8cbb8` lane's |
| `pascal_custom_tools_execution_equivalence.rs` | `pascal-custom-tools-execution.test.ts` | pin | the execution core | unchanged |
| `pascal_run_custom_equivalence.rs` | `pascal-run-custom.ts` | pin | `buildRunCustomDescription` bytes | unchanged by triggers (`f0db8cbb8` changes the preamble + `Side effects:` line — §8) |
| `carina_parser_equivalence.rs` | `carina-parser.ts` (tsx) | pin | `parseCarinaQuery` | EXTEND: `strip` rows (§4.2) |
| `message_finalizer_tier3_equivalence.rs` | `message-finalizer-tier3.test.ts` + `message-finalizer-tier3.json` + `build-message-finalizer-fixture.ts` | pin | the finalize sequence, SSE trace, dumps; `runCarinaMarkupQuery` / `detectAndConvertRngPatterns` MOCKED | the reply site (§4.4) |
| `orchestrator_tier3_equivalence.rs` | `orchestrator-tier3.test.ts` | pin, `TZ=UTC` | the send spine end-to-end | the user-message site (§4.4) |
| `help_tools_equivalence.rs` | `help-tools.ts` + `build-help-tools-fixture.ts` | pin | `help_settings` exact output | RED at pin on `customToolTriggers` (keystone or this lane — the order decides) |
| `chat_settings_tier2_equivalence.rs`, `chat_settings_column_sites_guard.rs`, `chat_settings_voice_mode_ensure_equivalence.rs` (the ensure template) | — | — | the column | KEYSTONE |
| `public_schemas_vendor_guard.rs` | — | — | byte copy | byte count moves |
| `qtap_schema_validate_equivalence.rs` | (the vendored `generators/qtap-export.schema.json`) | — | the validator | re-vendor |

Sweep driver: `harness/tools/recipe_sweep.py --v4 <pin worktree> --v5w
~/source/quilltap-v5 --run <family>` (the spaceless path — memory note
`sweep-driver-spaced-path`); families are comma-separated on `--families`.

---

## 3. The proposed tier split

| unit | v4 source | tier | why |
|---|---|---|---|
| T1 the definition `triggers` key + `validateTriggers` + `KNOWN_TOP_LEVEL_KEYS` | `custom-tool.types.ts` | **1 (exact)** | pure Zod; sentences are route payload |
| T2 `strip_carina_query_lines` + `parse_query_line` | `carina-parser.ts` | **1** | pure; the existing family's row shape extends with a second `kind` |
| T3 `scan_trigger_phrases` / `triggers_contained_in` / `fold_trigger_text` | `trigger-phrases.ts` | **1** | pure (needs T2); `index` is a UTF-16 offset — corpus must carry astral + CRLF + Carina rows |
| T4 `run_custom_tool` (the run service) | `run-service.ts` | **2 (structural DB)** | the two existing entrance families regen GREEN = the extraction proof; the two NEW DEBUG lines + the moved cascade line pinned by capture in a v5 unit binary (v4's are `logger.debug` in jest — capture via the transports splice, memory note `v4-oracle-job-host-and-log-capture`) |
| T5 `run_triggered_custom_tools` + `triggers_enabled` | `trigger-runner.ts` | **2 + mocked seams** | a NEW real-DB family over the `pascal-run-custom` fixture: both sides run the REAL runner (v4's `runTriggeredCustomTools` with REAL roster/run-service/writers, only the logger spliced) and dump the posted rows (`chat_messages` system rows: Pascal + Prospero) + the returned message ids + the captured lines |
| T6 the two sites | `trigger-phrase-sites.ts` + the two spine hunks | **3 (mocked LLM) → 2** | the existing tier-3 families (a trigger-bearing tool planted in their fixtures, the setting ON for one case) — the SSE trace gains `pascalResult` frames in a pinned position; the `chat_messages` dump gains the Pascal row BETWEEN the user message and the assistant reply |
| T7 `pascalMeta.triggerPhrase` + the String `invoked_by` | the row/event schemas | **2** | the row round-trip test + the T5 dumps |
| T8 `triggerCount` | `workbench.ts` | **2** | the workbench family at the pin |
| T9 `help_settings` bag | `help-settings-handler.ts` | **2** | `help_tools_equivalence` at the pin (if this lane owns it) |
| T10 the schema byte copies | both `public/schemas/*.json` + `generators/qtap-export.schema.json` | guard | vendor guard + the export validator family |

---

## 4. The differential plan

### 4.1 T1 — extend `pascal-custom-tool-definition.ts` (the committed-list corpus both sides read)

Add after the `chip-label-*` block (`:478-489`) a `// ---- triggers (362426c8f)
----` block mirroring v4's own test rows PLUS the sentence-order cases v4's test
does not pin: `triggers-stored-as-written` (`['Open Sesame']`),
`triggers-at-cap` (8), `triggers-over-cap` (9), `triggers-empty-array`,
`triggers-too-short` (`['ok']`), `triggers-at-min` (`['abc']`),
`triggers-over-long` (81), `triggers-at-max` (80), `triggers-not-an-array`
(`'abracadabra'`), `triggers-null`, `triggers-element-not-a-string` (`[7]`),
`triggers-padded` (`['abracadabra', 'open sesame ']` → `triggers.1: trigger 2
has leading or trailing whitespace`), `triggers-case-duplicate`
(`['Abracadabra','ABRACADABRA']`), `triggers-padded-duplicate` (BOTH issues at
one index, whitespace first), `triggers-third-copy-names-first`
(`['a b c','A B C','a B c']` → `trigger 2 repeats trigger 1; trigger 3 repeats
trigger 1`), `triggers-unicode-fold` (`['Straße','STRASSE']` — NOT a duplicate
under `toLocaleLowerCase` (`ß` has no lowercase mapping change; `STRASSE` →
`strasse`) — pins that the fold is lower-casing, not case-folding),
`triggers-turkish-i` (`['İstanbul','istanbul']` — `İ`.toLocaleLowerCase() in
the default locale is `i̇` (i + U+0307), so NOT a duplicate; v5's
`to_lowercase` gives the same two code points — a byte-identity row),
`triggers-with-chip-and-description-issues` (the issue ORDER across
`chipLabel`/`triggers`/`description`), `shape-order-chip-triggers-description`
(the existing `shape-order-*` idiom), and `unknown-keys-triggers-known` (the
`unknownKeys` row must NOT list `triggers`). The SPA corpus spec reads the same
NDJSON — the SPA survey takes the rows as given.

### 4.2 T2 — extend `carina-parser.ts`

Add a second row kind `{ id, kind: 'strip', content, out: string }` driving
`stripCarinaQueryLines` over: v4's four test inputs, every existing
`parseCarinaQuery` input (so `strip` is exercised on the whole 60-row corpus —
the one-line change `cases.map(…)`), plus `'@Alice:\n@Alice: real'` (the empty
query kept, the real one removed), CRLF/LF mixes, a lone `\r` inside a line,
trailing `\n` (`'a\n'` → `'a\n'` — the final empty line is a non-query line and
rejoins), U+2028 inside a line. `carina_parser_equivalence.rs` gains a `kind`
discriminator (`Row` `:26-30`) with the old rows defaulting to `parse`.

### 4.3 T3 — a NEW tier-1 case `pascal-trigger-phrases.ts`

Drives the REAL `scanTriggerPhrases` / `triggersContainedIn` /
`MAX_TRIGGER_RUNS_PER_MESSAGE` over `(content, tools[])` rows emitting `{ hits,
dropped }` whole (`tool`, `phrase`, `index`). Rows: v4's ten; `index` after a
Carina line was stripped (the offsets SHIFT — `'@Alice: q\nabracadabra'` →
index 0); `index` past an astral char (`'😀 abracadabra'` → 3, not 2); CRLF
content (`'a\r\nabracadabra'` → the strip normalises to `'a\nabracadabra'`,
index 2); two tools, same offset (roster order kept); two phrases of ONE tool at
the same offset (definition order kept); a phrase with uppercase in the
DEFINITION matched by lowercase content (`phrase` returned as written); a
zero-length phrase in the list (skipped — only reachable through the
`Pick<…,'name'|'triggers'>` shape, since the schema refuses it); whitespace-only
content after the strip; `tools` with `triggers: undefined`; five tools → 4 +
1 dropped with the DROPPED one being the LATEST by offset, not by roster.
New family `pascal_trigger_phrases_equivalence.rs`, env
`QT_ORACLE_PASCAL_TRIGGER_PHRASES`, recipe `cd ~/source/quilltap-server; TZ=UTC
npx tsx <V5W>/harness/oracle/cases/pascal-trigger-phrases.ts > /tmp/oracle-pascal-trigger-phrases.ndjson`.

### 4.4 T4 — the run service: the two entrance families as the extraction proof + a capture pin

1. Regen `pascal_run_custom_handler_equivalence` and
   `pascal_custom_tools_route_equivalence` at the pin BEFORE porting (the
   measurement): the handler family should be green (byte-identical arm); the
   route family's rows on the `whispered` tool with `private` absent are the
   §1.9.2 flips — record them as the PRE-measurement, then port, then both
   green.
2. Add route corpus rows that pin the two changes explicitly:
   `run-default-whisper-no-private` (expects `targetParticipantIds:[userId]`,
   `result.whispered: true`) and `run-default-whisper-refused-no-private` (a
   parameter nobody declared on the whispered tool → the Prospero row targeted
   to `[userId]`, 400 body `{ error: reason }`).
3. The log lines: v4's `Custom tool run: state cascade unavailable, defaulting
   to {}` (DEBUG, `pascal.run-service`, `chatId, tool, error`), `Custom tool run
   refused` (DEBUG, `chatId, tool, invokedBy, reason`), `Custom tool dealt`
   (DEBUG, `chatId, tool, tier, invokedBy, state, effects, whispered, posted`)
   and the entrances' surviving `Manual custom-tool run refused` (DEBUG,
   `api.v1.chats.custom-tools`, `chatId, tool, reason`), `Custom tool run
   failed` (WARN, `run-custom-handler`, `userId, chatId, tool, reason`), `Custom
   tool run completed` (INFO), `[Pascal] Manual custom-tool run completed`
   (INFO) — NONE exist in v5 today (§2.3, §9 #1). Port them as `tracing`
   lines with v4's field NAMES in v4's order (`target: "quilltap::pascal"`, the
   list handler's precedent `custom_tools.rs:508-516`), and pin them in the two
   jest oracle cases by splicing `logger.transports` (memory note
   `v4-oracle-job-host-and-log-capture` §2 — `LOG_LEVEL=debug` before the
   import; drop `service`/`environment`; fold `error.message`) into a `logs:
   [{level, message, fields}]` field per case row, compared on the Rust side
   through `test_support::global_capture` (memory note
   `capture-rig-writer-thread`: install at the top of EVERY test in the binary;
   the cascade read is on the caller thread — `db.read_main` — so it IS
   capturable; the Pascal post's lines inside `db.write` are NOT — `post_pascal_result`
   has none today). The cascade-unavailable arm needs a plant: a chat row
   whose `metadata` is unparseable makes v4's `findById` throw (jest in-case
   plant via a keyed `SQLITE_PATH` connection — memory note
   `jest-oracle-in-case-plants`), and v5's `chats_read::find_by_id` must throw
   on the SAME plant (memory note `throwing-plant-must-fail-both-reads`).
4. The handler-family row set grows: `run-llm-unposted` (a plant that makes
   the Pascal post fail — a dropped `chat_messages` table is recreated by the
   writer; use a VIRTUAL generated column that throws on insert) is the one arm
   left unpinned.

### 4.5 T5 — a NEW real-DB family `pascal_trigger_runner_equivalence`

Oracle `harness/oracle/cases/pascal-trigger-runner.test.ts` (jest, the
`pascal-run-custom` fixture mirrored to `/tmp/qt-pascal-trigger-mirror` — a
NEW scratch path, never the handler family's `/tmp/qt-pascal-mirror`; memory
note `parallel-lane-shared-resources`), driving the REAL
`runTriggeredCustomTools` with REAL roster / run service / writers / repos and
the logger spliced, over a fixture extended with trigger-bearing tools (§5).
Per case `{ params (speaker, content, chatSettings, characterIds, projectId),
posted: [ids], rows: the posted chat_messages system rows (id + createdAt
positionally normalised — the handler family's idiom), logs }`. Cases: the gate
×3 (off / master off / null → nothing read: assert ZERO new rows AND no log);
blank content; a character speaker with a sheet-gated tool (one visible, one
withheld → only the visible fires — the roster honours gates); the bare
operator (`speaker {null,null}` → `whisperTargetIds: [userId]` on a
default-whisper tool → the Pascal row targeted `[userId]`); a user-driven
CHARACTER seat (the seat's vault); a missing character id (WARN `… not
found`); a broken vault — CHAR_D (WARN `… unreadable`, no rows); two hits in
appearance order (`invokedBy:'trigger'`, `triggerPhrase`, `callerParticipantId`,
ONE `createdAt` instant? — no: `nowMs` is the clock the EFFECTS stamp; pin a
progression-writing tool so `updatedAt` equality across the two runs proves the
single reading); the cap (5 tools → 4 Pascal rows + WARN `… over the
per-message cap` `fired:[…] dropped:[…]`); a refused run (`parameters: null`
on a tool whose parameter has NO default → `CustomToolRunError` → DEBUG
`Triggered custom tool refused` + a Prospero `custom-tool-error` row targeted
by `defaultVisibility`) followed by a good one; a Carina-only content; a tool
with `disabled: true`; `private: undefined` on a `defaultVisibility: 'whisper'`
tool (targeted to the speaker's participant id). The Rust side: a new
`pascal::trigger_runner::run_triggered_custom_tools(db, params, consult:
Option<&dyn ConsultRunner>, now_ms, sink: impl FnMut(Value))` with `now_ms`
INJECTED (the P4.D169 rule — v4 reads `Date.now()` at `:92`; the oracle freezes
it as the handler family does with `nowMs`).

### 4.6 T6 — the sites in the tier-3 families

- `message-finalizer-tier3.test.ts` mocks `runCarinaMarkupQuery` and
  `detectAndConvertRngPatterns` (its header `:13-20`) but NOT the trigger
  runner → with the fixture's settings `customToolTriggers` default `false` it
  is a silent no-op TODAY (the regen at the pin stays green — measure). Add ONE
  case: a chat whose settings row has `customToolTriggers: 1` and a planted
  tool whose phrase the canned reply says → the SSE trace gains a `pascalResult`
  frame AFTER the reply's `content` frames and BEFORE `done`; the
  `chat_messages` dump gains the Pascal row after the assistant row (the
  minted-id remap handles it); `background_jobs` unchanged. The Rust side's
  `FinalizerChatSettings` carries the two flags; the runner runs for real over
  the fixture (no seam — the roster, the run service and the writer are all
  real-DB; the consult seam passes `None` and the planted tool declares no
  `llm`).
- `orchestrator-tier3.test.ts` (its `jest.doMock` list `:235-700` mocks
  `markup-runner` `:680` — not the trigger runner): the same one case on the
  user-message path — the Pascal row lands BETWEEN the user message and the
  assistant reply, the `pascalResult` frame precedes the first `status` frame
  of the model leg, and the assistant's `CHAT_MESSAGE` `llm_logs` row's request
  CONTAINS Pascal's line (the splice) — the strongest pin available for
  `existingMessages.push(...triggered)`. Both fixture builders gain a
  `Tools/<name>.tool.json` doc-store file with `triggers` + a settings flip for
  the one chat (the builders already plant stores; memory note
  `shared-fixture-rebuilds-and-jest-setup-mocks`: re-minted ids per oracle →
  mask, never ORDER BY).
- A v5 source guard (`trigger_sites_order_guard.rs`, the `chat_settings_column_
  sites_guard` idiom) pinning v4's three source-text assertions: in
  `orchestrator.rs` the `user_message_persisted: true` literal precedes the
  runner call; in `message_finalizer.rs` `save_assistant_message(` precedes it;
  the swipe module never names it.

### 4.7 T7–T10

- T7: the row round-trip unit (`chats_messages.rs:1175-1189`) gains a
  `'trigger'` + `triggerPhrase` wire string; the T5 dumps pin the key ORDER
  (`invokedBy`, `triggerPhrase`, `callerParticipantId`) through the byte compare.
- T8: `pascal-workbench.test.ts` / `workbench.json` — add one tool with
  `triggers` (a count of 2) so `triggerCount: 0` AND `2` both appear; the
  library's `tools.len() == 8` assertion (`pascal_workbench_equivalence.rs:169`)
  moves to 9 (both sides read the fixture builder).
- T9: `help-tools.ts`'s settings ops already dump the whole `chat` bag → regen
  at the pin shows the new key; the fixture builder's settings row carries the
  column after the keystone's re-dump (ORDER the keystone first).
- T10: `public_schemas_vendor_guard.rs` byte count + the `qtap_schema_validate`
  family's regen after re-vendoring `generators/qtap-export.schema.json` from
  the pin.

---

## 5. Fixtures

- `crates/quilltap-web/tests/fixtures/pascal-run-custom-{main,mount}.db` +
  `pascal-run-custom-main.db.meta.json` (`vaultA..D`), spec
  `harness/oracle/fixtures/pascal-run-custom.json` (`testPepperBase64`, `userId
  e18e05bc-…`, `seedTimestamp 2026-04-01`), builder
  `build-pascal-run-custom-fixture.ts`, columns migrator
  `migrate-pascal-run-custom-columns.ts`. Rooms: `CHAT c1…01` (user plays
  CHAR_A), the five P4.d24 perspective rooms `c1…02-06`; characters `CHAR_A/B/C`
  + the broken-vault `CHAR_D`; tools `ansible` (per-character variants), `coin`
  + `whispered` (char A's vault only), a group `GROUP a2…aa`. **Extend** (a
  REBUILD through the builder, never a hand edit — memory note
  `rebuilt-fixture-pair-vintage`: the rebuilt pair must carry every later-added
  column; diff `sqlite_master` against the committed pair and run
  `runV4Migrations` in the builder) with: `spell.tool.json` (`triggers:
  ['abracadabra','open sesame']`, a `min===max` roll, one `state.spells.cast`
  effect), `oracle.tool.json` (`triggers: ['consult the oracle']`, a
  `metadata` gate — `availableWhen: { blessed: true }` — so the sheet decides),
  `initiative.tool.json` (`triggers: ['draw my sword']`, a required parameter
  WITHOUT a default → refused on a triggered run), `hush.tool.json`
  (`defaultVisibility: 'whisper'`, `triggers: ['keep it quiet']`), and five
  one-word-trigger tools `t0..t4` for the cap; CHAR_A's `metadata.json` gains
  `blessed: true`, CHAR_B's not; ONE chat (a new `c1…07`, user-driven CHAR_B
  seat) with `chat_settings.customToolTriggers = 1` — which REQUIRES the
  keystone's column in the builder's DDL (the builder provisions through v4's
  real `initializeDatabase` at the pin, so the column arrives with the pin;
  memory note `builder-fixtures-lack-instance-settings` for the settings plant
  idiom).
- `crates/quilltap-web/tests/fixtures/workbench-{main,mount}.db` + `workbench.json`
  + `workbench-route-cases.json`: one more tool with `triggers` (T8).
- `harness/oracle/fixtures/message-finalizer-tier3.json` +
  `build-message-finalizer-fixture.ts`; the orchestrator tier-3 fixture + its
  builder: one trigger-bearing tool in a doc store the room's character can
  reach, one chat with the setting on (T6).
- `harness/oracle/fixtures/pascal-side-effects.json`: unchanged by triggers.
- Committed corpora both sides read: `pascal-custom-tool-definition.ts`'s
  `cases` list (also the SPA's NDJSON), `carina-parser.ts`'s `cases`.

---

## 6. Traps and risks (with the memory notes that apply)

1. **`toLocaleLowerCase` vs `to_lowercase`:** byte-identical per the ICU
   ruling, but the corpus must INCLUDE the two rows that would expose a
   divergence if a future fold changed (`İ`, `ß`, and a Greek final sigma —
   `'ΟΔΥΣΣΕΥΣ'` lowercases to `'οδυσσευς'` with a final `ς` in both JS and
   Rust: another identity row worth pinning). Never substitute
   `to_ascii_lowercase`.
2. **`index` is a UTF-16 offset into the FOLDED haystack** — folding can change
   the UTF-16 LENGTH (`İ` → two code units), so an offset AFTER such a character
   differs from the unfolded offset. v4 computes `indexOf` on the folded string;
   v5 must fold FIRST and measure with `jsstr`'s UTF-16 helpers, not byte
   offsets. (`index` is "for ordering only" but the differential compares it.)
3. **`!params.content` vs `!content?.trim()`:** the sites gate on empty string,
   the runner on whitespace — a whitespace-only message reaches the runner and
   returns `[]` BEFORE any read. Pin that no roster read happens (T5's
   zero-rows + no-log case).
4. **The runner never throws, so its failures are LOG-ONLY** — the four WARN
   lines are the only evidence; without the capture pins a port that swallowed
   an error silently would pass every row. Capture is mandatory (memory note
   `capture-rig-writer-thread`: `global_capture::install()` in every test of
   the binary; the runner's lines are on the caller thread — the roster read is
   `db.read_main`, the character read is `db.read_main` — capturable; v4's
   `postProsperoCustomToolError` inside the loop is a repo write whose own
   lines v5 does not have).
5. **The orchestrator's `user_message_carina` is a STANDING DEFERRAL** (never
   wired); the trigger site goes ABOVE where Carina would go, so it has no
   ordering dependence on that deferral — but the v5 placement note at
   `orchestrator.rs:1991-2003` (the paused-chat hold's measured position) must
   be re-measured: v4's trigger call is ABOVE the paused-chat return, so a held
   turn still fires triggers in v4 and must in v5.
6. **The finalizer runs in the job child for autonomous rooms** (v4 `:24-28` of
   the runner); v5's single-writer runtime has no child — `onPosted` is the
   spine's `Event` sink and the autonomous driver's sink is a no-op; nothing to
   port, but the order record should say so.
7. **The executor's `on_posted` gap** (`executor.rs:680-685`): the LLM
   entrance's Pascal post surfaces only at the post-turn refresh in v5; the
   trigger sites DO emit live. A reader comparing the two entrances' SSE traces
   must not "fix" the executor in this lane (it is the P4.6ay-era recorded
   divergence).
8. **`invoked_by` is a `String`** — do NOT narrow it to an enum in this lane
   (a narrowing would REFUSE future v4 values the way the ledger feared; the
   String is the faithful reading of v4's restore which parses rows through
   Zod only on the way IN of a `.qtap`). Where v5 DOES validate on the way in
   (the `.qtap` import's schema validator), re-vendor the schema (T10).
9. **The route's two behaviour changes (§1.9.2)** will show in the route family
   regen as reds on the `whispered` tool rows; they are v4 drift, not v5 bugs —
   the D23 tripwire posture: port the new rule, do not carve.
10. **`KNOWN_TOP_LEVEL_KEYS` is derived in v4, hand-written in v5** — the
    definition corpus's `unknownKeys` row catches the omission only if a row
    carries `triggers`; §4.1's `unknown-keys-triggers-known` row is the pin.
11. **The two jest families share `/tmp/qt-pascal-mirror`** — the T5 family
    takes its own path; the sweep driver refuses a collision (`--collisions`).
    Memory note `parallel-lane-shared-resources` (the two-gate cap: queue behind
    sibling PIDs, never a pattern); `no-pattern-kill-on-shared-machine`.
12. **Gates read oracles at run time** (`lane-gate-hygiene`): per-unit oracle
    paths under `/tmp/<order>/…`; no in-place regen mid-gate; `set -o
    pipefail` or redirect-then-read.
13. **The stacked lane (`f0db8cbb8`) edits the SAME five files** (§8): if both
    lanes run in one round, the trigger lane is the BASE and the wardrobe lane
    starts from its branch — or the run-service signature is agreed up front
    (§7) so both can land; memory note `keystone-lane-swarm-gotchas` (fmt and
    bumps only after every gate; re-measure censuses LAST).
14. **A jest oracle in-case plant on a stale singleton** (`jest-oracle-in-case-plants`)
    for the cascade-unavailable arm; **v4's index cache** does not apply (no
    memory index here).
15. **The `serde_json::Map ==` trap** (`serde-json-map-eq-ignores-order`): the T5
    and T6 row compares must be `to_string` byte compares so `triggerPhrase`'s
    position is checked.
16. **A triggered run's `nowMs` is ONE reading for the message** but the Pascal
    rows' `createdAt` are minted per post (`post_pascal_result` reads
    `now_iso()` per call) — do not try to pin `createdAt` equality across two
    hits; pin `updatedAt` on a progression effect instead (§4.5).
17. **`triggersEnabled` reads `customTools !== false`** — an ABSENT key is
    enabled; v5's `OrchestratorChatSettings.custom_tools: bool` (`:304`) is
    populated with `?? true` (the P4.d6 absent-column rule) so the semantics
    survive the struct; the finalizer's struct must carry `Option<bool>` for
    both to keep the `Pick`'s three-state (`null` settings → disabled).
18. **The SPA twin of `trigger-phrases.ts` needs `stripCarinaQueryLines`** —
    `apps/web/src/app/chat/carina-parser.ts` exists with its own oracle spec
    (`carina-parser.oracle.spec.ts` over `apps/web/src/testing/fixtures/
    carina-parser.ndjson`); the SPA survey extends it from the same T2 corpus.

---

## 7. Contract candidates (names a sibling lane needs)

### 7.1 From the KEYSTONE (consumed here)

- `db::chat_settings::ChatSettings.custom_tool_triggers: bool` (schema-ordered
  after `custom_tools`, before `composition_mode_default`; read absent →
  `false`; `ChatSettingsUpdate.custom_tool_triggers: Option<bool>`); the
  tolerant-read index shift (every positional `r.get(N)` after index 17 moves
  by one — the column census guard pins the six sites);
  `fresh_schema.json` / `chat_settings_seed.json` with `customToolTriggers: 0`;
  the boot ensure `db::chat_settings_custom_tool_triggers_repair::ensure_chat_settings_custom_tool_triggers`
  (UNSTAMPED, table-gated, `INTEGER DEFAULT 0` appended) + its
  `restore_vintage_state` boot-align entry; `api/settings.rs` PUT arm
  `bool_field(v, "Invalid customToolTriggers value (must be boolean)")`.
- Whether the keystone or this lane adds `help.rs:215`'s sibling line and the
  Almanack's `custom_tool_triggers` — the order must assign ONE owner (the
  `help_tools_equivalence` and `almanack-*` families regen once).

### 7.2 PROVIDED by this lane (proposed spellings)

- `pascal::custom_tool_types::{MAX_TRIGGERS: usize = 8, MIN_TRIGGER_PHRASE_LENGTH:
  usize = 3, MAX_TRIGGER_PHRASE_LENGTH: usize = 80}`;
  `QtapCustomTool.triggers: Option<Vec<String>>` (wire key `triggers`, after
  `chipLabel`); `KNOWN_TOP_LEVEL_KEYS: [&str; 16]`.
- `carina_parser::{parse_query_line(&str) -> Option<CarinaQuery>,
  strip_carina_query_lines(&str) -> String}`.
- `pascal::trigger_phrases::{MAX_TRIGGER_RUNS_PER_MESSAGE: usize = 4,
  TriggerHit { tool: String, phrase: String, index: u64 }, fold_trigger_text(&str)
  -> String, triggers_contained_in(&str, Option<&[String]>) -> Vec<String>,
  scan_trigger_phrases<'a>(&str, impl IntoIterator<Item = (&'a str,
  Option<&'a [String]>)>) -> ScanResult { hits, dropped }}`.
- **`pascal::run_service::{RunCustomToolParams<'a> { user_id, chat_id, chat:
  Option<&'a Value>, entry: &'a DiscoveredCustomTool, parameters:
  Option<&'a Map<String, Value>>, private: Option<bool>, character_id:
  Option<&'a str>, metadata: &'a Map<String, Value>, whisper_target_ids:
  Option<Vec<String>>, invoked_by: InvokedBy, caller_participant_id:
  Option<&'a str>, trigger_phrase: Option<&'a str>, now_ms: i64,
  pending_wardrobe_announcements: Option<&'a mut HashSet<String>> /* the
  f0db8cbb8 seam — RESERVED, `None` here */ }, InvokedBy { Llm, User, Trigger }
  (serialises `"llm"|"user"|"trigger"`), RunCustomToolOutcome { Ok { result:
  CustomToolRunResult, posted: Option<PostedPascalMessage>, applied_effects:
  Vec<AppliedEffect> }, Refused { reason: String, whisper: bool } },
  run_custom_tool<R: RandomBytes + Send>(db: &Db, params: RunCustomToolParams<'_>,
  rng: &mut R, llm_invoke: Option<&dyn LlmInvoker>) -> Result<RunCustomToolOutcome,
  DbError>}`** — the `Err` arm is v4's rethrow (a non-`CustomToolRunError`); the
  wardrobe-effects lane EXTENDS this signature (it adds `user_id` to
  `ApplyCustomToolEffectsParams` and threads the set).
- `pascal::trigger_runner::{TriggeredSpeaker { participant_id: Option<String>,
  character_id: Option<String> }, TriggerSettings { custom_tools: Option<bool>,
  custom_tool_triggers: Option<bool> }, triggers_enabled(Option<&TriggerSettings>)
  -> bool, RunTriggeredCustomToolsParams { user_id, chat_id, content, speaker,
  project_id: Option<String>, character_ids: Vec<String>, chat_settings:
  Option<TriggerSettings> }, run_triggered_custom_tools(db, params, consult:
  Option<&dyn ConsultRunner>, now_ms: i64, on_posted: &mut dyn FnMut(&Value))
  -> Vec<Value>}` (never returns `Err`).
- `services::trigger_phrase_sites::{run_user_message_triggers(..),
  run_reply_triggers(..)}` with `room_character_ids(&[Value]) -> Vec<String>`.
- `db::chats_messages::PascalMetaIn.trigger_phrase: Option<String>` (between
  `invoked_by` and `caller_participant_id`).
- `pascal::workbench::CustomToolLibraryEntry.trigger_count: usize` (wire
  `triggerCount`, between `llm` and `parameterCount`) — the SPA's
  `workbench.api.ts` entry type gains the same key.
- `OrchestratorChatSettings.custom_tool_triggers: bool`,
  `FinalizerChatSettings.{custom_tools, custom_tool_triggers}: Option<bool>`.
- The wire shape of `triggers` on a definition for the SPA: `string[]`, 1–8,
  each 3–80, as written; absent when none. The event `pascalResult` carries the
  full message with `pascalMeta.invokedBy: 'trigger'` + `triggerPhrase`.

---

## 8. The `f0db8cbb8` seam (what the NEXT stacked lane changes on top)

`git diff 362426c8f 25b8fab6a` over the shared files:

- `lib/pascal/run-service.ts` (209 → 219): `+:76-80` the
  `pendingWardrobeAnnouncements?: Set<string>` param; `:150-151` the comment
  "metadata, progress and wardrobe effects skip"; `+:154` `userId,` on the
  `applyCustomToolEffects` call; `+:162-164` the set spread. **Everything else
  — the cascade, the refusal, the pascalMeta, the `Custom tool dealt` line — is
  `362426c8f`'s.** So the trigger lane ports lines `:1-75, :81-149, :152-153,
  :155-161, :165-219` of the pin file; the wardrobe lane owns `:76-80, :154,
  :162-164`.
- `lib/tools/handlers/run-custom-handler.ts` (246 → 254): `+:52-56`
  `RunCustomToolContext.pendingWardrobeAnnouncements?: Set<string>`; `+:190-192`
  the spread into `runCustomTool`. The trigger lane ports the rest (`:1-51,
  :57-189, :193-254`).
- `lib/chat/tool-executor.ts` `+:625` `pendingWardrobeAnnouncements:
  context.pendingWardrobeAnnouncements` — wardrobe lane only.
- `lib/pascal/custom-tool.types.ts` (+233): the `wardrobe` effect union
  (`CustomToolWardrobeGestureSchema`, `isWriteEffect` / `isWardrobeEffect`,
  `wardrobeGestureProblems`, `validateWardrobeGesture`, the `effects`
  `.describe` text) — the trigger hunks (`:88-94`, `:986-996`, `:1042`,
  `:1127-1156`) are disjoint from it; the wardrobe lane's `validateEffects`
  rewrite (`:1279-1291`) moves the `when` check to the top of the loop — the
  trigger lane must NOT touch `validate_effects`.
- `lib/pascal/tool-draft.ts` (+159): `DraftEffect.kind` / `wardrobe`,
  `emptyDraftWardrobeGesture`, `newDraftEffect`, `wardrobeFromDraft`,
  `validateDraftWardrobeGesture`, the rename/find-references arms — disjoint
  from the trigger hunks (SPA side).
- `lib/database/repositories/chats-messages.ops.ts` / `lib/schemas/chat.types.ts`:
  COMMENT-only (`effects` prose) — no schema change; `target: z.string()`
  already admits `wardrobe.wear`.
- `lib/tools/run-custom-tool.ts` (+31): the preamble sentence `'…adjusting the
  scene's persistent state, the rolling character's own records, or what they
  are wearing, server-side…'` and `Side effects: ${describeEffects(...)}` —
  MODEL-VISIBLE bytes; `pascal_run_custom_equivalence` regen at the pin is RED
  on those rows for the TRIGGER lane too (the family has one corpus): the order
  must either pin that family at `362426c8f` for the trigger lane or hand it
  to the wardrobe lane whole.
- `public/schemas/qtap-custom-tool.schema.json` (+102) and
  `qtap-export.schema.json` (+2/−2 description): ONE byte copy at the pin if
  both lanes land in the round, else the trigger lane copies
  `362426c8f:`'s files and the vendor guard's byte count is set TWICE.
- `lib/pascal/side-effects.ts` (+79), `wardrobe-effects.ts` (NEW 269),
  `tool-vocabulary.ts`, `custom-tools.ts` (+92), `wear-ops.ts` — wardrobe lane.

**The seam, stated once:** the trigger lane lands `run_custom_tool` with
`pending_wardrobe_announcements: Option<…>` RESERVED (always `None`, unread) so
the wardrobe lane adds the field's USE, not the field; `ApplyCustomToolEffectsParams`
gains `user_id` in the wardrobe lane (it is only needed for `wearItem`'s
`source: 'tool'` attribution there).

---

## 9. What the ledger row (`drift-ledger.md:316`) got wrong, or left out

1. **"the handler's OWN log lines … are GONE — the run service's replace them
   (a ported capture pin on those texts WILL trip)."** Nothing can trip: v5's
   `tools/run_custom.rs` and the `chat_custom_tool_run` arm of
   `api/custom_tools.rs` emit ZERO tracing lines (§2.3) — v4's baseline
   `run_custom: state cascade unavailable…`, `Manual custom-tool run: state
   cascade unavailable…`, `Manual custom-tool run refused`, `Custom tool run
   failed`, `Custom tool run completed`, `[Pascal] Manual custom-tool run
   completed`, `run_custom validation failed`, `run_custom named an unknown
   tool` were never ported, and no capture pin exists in either family. The
   lane ADDS v4's pin-state lines; it moves nothing. Also "its refused line":
   the baseline handler had no DEBUG refused line of its own — its refusal was
   the surviving WARN `Custom tool run failed` via `reportFailure`.
2. **"⚠ LIVE-DATA: a v5 at the baseline parsing `pascalMeta` with the
   two-value enum would DROP / refuse rows v4 4.11-dev writes with
   `'trigger'`."** v5 has no two-value enum: `PascalMetaIn.invoked_by` is a
   plain `String` (`chats_messages.rs:330`), so the row read, the write, the
   backup and the `.qtap` row carriers already pass `'trigger'` through. The
   narrower real hazard is the VENDORED export schema
   (`generators/qtap-export.schema.json:972`, `enum ["llm","user"]`, no
   `triggerPhrase`) used by the `.qtap` validator: a 4.11 export carrying a
   triggered row is refused at validation until re-vendored.
3. **"`.qtap` export schema NOT touched by this commit (the dirt touches it for
   `effects`)."** The commit's own `--stat` (which the row reproduces) shows
   `public/schemas/qtap-export.schema.json | 3 +-`: THIS commit widens
   `invokedBy` to three values and ADDS `triggerPhrase` (`:972-973`);
   `f0db8cbb8` changes one `effects` description only.
4. **Two route behaviour changes the row does not record** (§1.9.2): the manual
   route's `whispered` now derives from `result.visibility` (the definition's
   `defaultVisibility`) instead of `body.private === true`, so a default-whisper
   tool run without `private` is now targeted to the operator and reports
   `whispered: true`; and its refusal bubble's `whisper` is `body.private ??
   defaultVisibility === 'whisper'`. Both are measurable in
   `pascal_custom_tools_route_equivalence` on the existing `whispered` fixture
   tool.
5. **"a FOURTH superRefine `validateTriggers`"** — it is the fifth step of the
   chain (ordering, references, gates, effects, triggers); fourth among the
   `validate*` functions. Harmless, but a lane counting steps should read
   `:1038-1043`.
6. **Omitted:** `KNOWN_TOP_LEVEL_KEYS` is DERIVED in v4 (`:1572`) so `triggers`
   becomes a known key for free, while v5's hand-written 15-entry array
   (`custom_tool_types.rs:3321`) must be edited — and the definition corpus's
   `unknownKeys` row is the only tripwire.
7. **Omitted:** the orchestrator's user-message Carina path, which the row
   names as the trigger site's lower neighbour, is a STANDING v5 DEFERRAL
   (`orchestrator.rs:45-48, 1991-2003`); the site's v5 position is bounded by
   the persist above and the paused-chat hold below instead.
8. **The row's "`run-custom-handler.ts` … `whisper = isPrivate ??
   defaultVisibility === 'whisper'`" catch** is right, and worth stating
   precisely: that `catch` now sees ONLY rethrown non-`CustomToolRunError`s;
   the refused arm reads `outcome.whisper`, which the service computed the same
   way — so the model's result is unchanged on both arms.
9. **`pendingWardrobeAnnouncements` / the `userId` on the applier** are
   `f0db8cbb8`'s lines inside the pin's `run-service.ts` (§8), not this
   commit's — the row's (e) description of the service is of the `(c362)`
   shape and is correct as such.
