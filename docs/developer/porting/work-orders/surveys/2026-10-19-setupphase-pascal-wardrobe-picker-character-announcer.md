# Setupphase survey — the Pascal pair: the Workbench wardrobe picker (`5cb0e0190`) + `{{character}}` / `announceAsCharacter` (`a05c6f755`)

READ-ONLY planning survey for TWO work orders (a SERVER lane and an SPA lane).
Pin: v4 `e8e17b2d9` (v4 `main`, 2026-10-18). Oracle baseline (what v5 has
ported): `25b8fab6a`. Every v4 fact below was read through `git show
e8e17b2d9:<path>` / `git show <sha> -- <path>`; v4 line numbers are AT THE PIN.
v5 line numbers are at v5 `main` (`2e89d291a`). Port facts come from the
shipped hunks, not the commit messages (§8 records where the two disagree).

---

## §0 The span

Two consecutive v4 commits, both parented on the baseline line
(`5cb0e0190` parent `25b8fab6a`; `a05c6f755` parent `5cb0e0190`), neither a
D23 move (no `lib/schemas` column, no migration, no `generateDDL` hunk — the
`chat.types.ts` +9/−7 is the `customAnnouncer` DOC COMMENT only).
**`5cb0e0190`** gives the Workbench a wardrobe-reference picker: a read-only
server lens (`lib/pascal/bench-wardrobe.ts`, NEW) that, when the bench's fact
sheet seats a character (`{ characterId }`), resolves each dry-run wardrobe
gesture's item references against that character's wearable pool through the
applier's own `findInPool` / `resolveWearable` and returns
`wardrobeResolution` LAST in the `?action=preview` body; the
`benchCharacterId` helper narrows the `{ characterId }` arm to a NON-EMPTY id
(`{ characterId: "" }` becomes a hand-typed sheet → 200, where it was a 404);
`lib/wardrobe/wearable-ref.ts` (NEW) is the one reference-lookup rule and the
server pool's `findByTitle` delegates to it (behaviour-neutral); the SPA gains
`WardrobeVantage` editor state, `defaultWardrobeVantage`,
`wardrobeReachLabel`, the vantage loader hook, `WardrobeReferencePicker`,
`WardrobeVantageSelect`, the side-effects chip/advisory/slot-coverage UI and
the bench's would-apply / would-skip lines. **`a05c6f755`** adds the
`{{character}}` placeholder family (rendered in the consult prompt, outcome
message, chip label, effect expressions and wardrobe item refs; absent → as
written + a DEBUG), resolved per entrance (`run-service` reads the RAW row
when the entrance supplies no name; the trigger runner supplies the overlaid
name; the bench supplies the seated character's name), and the definition
key `announceAsCharacter` (between `defaultVisibility` and `parameters`) that
makes the run post with `customAnnouncer: { kind: 'character', characterId }`
BESIDE `systemSender: 'pascal'`; the SPA names that announcer on the Staff
bar / chip (`getAnnouncementSenderName`), and grows the BuilderForm toggle,
the Insert-value "Character name" entry, the run dialog's reference row and
the public JSON schema.

### §0.1 File separation against the sibling (outfit-history) survey

**The caller's premise is partly wrong (measured):** `05005c562` /
`4ae20977f` do NOT touch `lib/pascal/tool-draft.ts` nor
`app/api/v1/custom-tools/route.ts`. Per-file commit lists in
`25b8fab6a..e8e17b2d9` (`git log --format=%h -- <path>`):

| v4 file | commits in span | THIS pair's hunks | SIBLING's hunks |
|---|---|---|---|
| `app/api/v1/custom-tools/route.ts` | `a05c6f755`, `5cb0e0190` | **ALL** (doc header `:9-13`, import `:46`, `resolveMetadata` `:150-176`, `benchCharacterId` `:178-183`, `parseBenchRequest` `:191-219`, `handlePreview` `:233-292`) | **none** |
| `lib/pascal/tool-draft.ts` | `a05c6f755`, `5cb0e0190` | **ALL** | **none** |
| `lib/pascal/run-service.ts` | `05005c562`, `a05c6f755` | `:67-71` (`characterName?` param), `:110` (`resolveCharacterName` call), `:140` (`character` override), `:187-191` + `:198` (announcer), `:231` (`announcedBy`), `:238-262` (`resolveCharacterName` fn) | `:29` (import `turnContainersOf`), `:82-88` (the containers doc + `pendingWardrobeHistory?`), `:173` (`...turnContainersOf(params)` replacing the conditional `pendingWardrobeAnnouncements` spread) |
| `lib/pascal/wardrobe-effects.ts` | `4ae20977f`, `05005c562` | **none** | **ALL** (import; `ApplyWardrobeEffectsParams extends WardrobeTurnContainers`; `provenance: { source: 'pascal', actorCharacterId }` + `...turnContainersOf(params)`) |
| `lib/chat/tool-executor.ts` | `ebb480edd`, `05005c562` | **none** | **ALL** |
| `lib/pascal/side-effects.ts` | `05005c562` | none | ALL |

v5 landing sites, so the two orders confine each lane to disjoint hunks:

| v5 file | THIS pair (server lane) | SIBLING lane |
|---|---|---|
| `crates/quilltap-core/src/pascal/run_service.rs` | struct field after `metadata` (`:95-96`) — `character_name`; the destructure `:147-154`; a `resolve_character_name` call after `progress` (`:159-163`); `execute_custom_tool` args `:230-243` (new `character` arg); `PostPascalResultParams` literal `:300-310` (announcer); `Custom tool dealt` `:313-325` (`announcedBy` between `whispered` and `posted`); NEW `fn resolve_character_name` (after `run_custom_tool`, before/after `build_pascal_meta` `:340-421`); test helper literal `:482-497` (one line after `metadata`) | struct `:109-114` (`pending_wardrobe_announcements` → the containers + history), `apply_effects_for_run` call `:266-277`, test helper literal's LAST line `:496` |
| `crates/quilltap-core/src/pascal/wardrobe_effects.rs` | **none** | ALL |
| `crates/quilltap-core/src/services/tool_execution.rs` | **none** | ALL |
| `crates/quilltap-core/src/pascal/side_effects.rs` (`apply_effects_for_run` `:924-936`) | **none** | ALL |
| `crates/quilltap-core/src/tools/run_custom.rs` `RunCustomToolParams { … }` `:743-761` | ONE line after `metadata: &metadata,` (`:751`) — `character_name: <absent>` (the run_custom entrance supplies none) | the `pending_wardrobe_announcements` line `:760` (+ ctx threading elsewhere in the file) |
| `crates/quilltap-core/src/api/custom_tools.rs` | `resolve_metadata` `:1031-1100`, `BenchRequest` `:941-946`, `parse_bench_request` `:957-970`, `custom_tool_preview` `:1264-1385`; the operator route's literal `:818-836` — ONE line after `metadata: &metadata,` (`:828`) | the operator literal's `pending_wardrobe_announcements: None` line `:834-835` only |
| `crates/quilltap-core/src/pascal/trigger_runner.rs` | the character read `:141-180` (keep the name), the literal `:256-273` — one line after `metadata: &metadata,` (`:265`) | the literal's `pending_wardrobe_announcements: None` `:270-272` |

**Struct-literal hazard:** `RunCustomToolParams` has no `Default`, so BOTH
lanes must add a field to every literal (four sites). Rule for the orders:
this pair adds `character_name` IMMEDIATELY AFTER `metadata` (v4's position,
`run-service.ts:65-71`); the sibling edits only the
`pending_wardrobe_announcements` line(s) at the END. The changed lines are
≥ 5 apart at every site, so the 3-way merge is clean; name the sites in both
orders.

---

## §1 v4 at the pin

### §1.1 SERVER

#### `lib/pascal/bench-wardrobe.ts` (NEW, 119 lines — `5cb0e0190`, unchanged to the pin)

- `:31` `const CONTEXT = 'custom-tools.preview';`
- `:33-44` `foundOf(ref, item)` → **key order** `{ ref, found: { id, title,
  types: [...item.types], origin: item.origin, imageFileId: item.imageFileId
  ?? null } }`. `origin` is the pool item's own object (`WardrobeOrigin`
  key order `scope, id, name`, `wardrobe-container.ts:37-44`).
- `:47-77` `resolveBenchWardrobeAgainst(pool, effects)` — PURE:
  - skips every non-wardrobe effect (`isApplicableWardrobeEffect`) and every
    `clear_slot` gesture ("A cleared slot names no item; there is nothing to
    find.");
  - per item ref (the RENDERED, trimmed refs of `ResolvedWardrobeGesture.items`,
    i.e. AFTER `{{character}}`/`{{params.x}}` substitution), `lookup = {
    itemId: ref, itemTitle: ref }`;
  - `take_off` → `findInPool(pool, lookup)` (archived items ARE found) →
    `foundOf` or `{ ref, found: null, reason: 'not_found', message:
    wardrobeItemNotFoundMessage(null, ref) }`;
  - otherwise → `resolveWearable(pool, lookup, wardrobe.mode ?? 'wear',
    wardrobe.slot)`; ok → `foundOf`; refusal → `{ ref, found: null, reason:
    resolution.reason, message: resolution.reason === 'not_found' ?
    wardrobeItemNotFoundMessage(null, ref) : resolution.message }`.
    **Key order on a miss: `ref, found, reason, message`.** The not-found
    message is deliberately re-spelled with `(null, ref)` →
    `Wardrobe item not found with title "<ref>"` (resolveWearable's own would
    say `… with ID "<ref>" with title "<ref>"`). Archived →
    `Item "<title>" is archived and cannot be worn` (`wearRefusal`); slot →
    `Item "<title>" (types: <t1, t2>) cannot be added to the "<slot>" slot`
    (`wear-ops.ts:78-92`).
  - pushes `{ index, items }` (index = the definition effect's index, the
    `ResolvedEffect.index`, so skipped effects keep their numbers).
- `:84-119` `resolveBenchWardrobe(repos, characterId, effects, toolName)`:
  - `gestures = (effects ?? []).filter(isApplicableWardrobeEffect && action !==
    'clear_slot')`; **none → `undefined`** (no key on the wire);
  - `pool = await loadWearablePool(repos, characterId, [], { operator: true })`
    — `[]` = NO project tier ("No project mounts and no chat: the lens has no
    project tier."); `operator` is irrelevant once the project tier is given;
  - catch → **WARN** verbatim:
    `logger.warn('Workbench preview: wardrobe resolution skipped — the wardrobe could not be read', { context: CONTEXT, tool: toolName, characterId, error: getErrorMessage(error) })`
    → `undefined` (fail-soft; the roll still returns). ⚠ v4's real
    `loadWearablePool` never throws (v5's port says so: `wearable_pool.rs:367-371`,
    "Never fails") — the arm is reachable in v4 only by mock (the v4 test
    `still returns the roll when the wardrobe cannot be read` mocks a
    rejection). §6 T8.
  - **DEBUG** verbatim, always on success:
    `logger.debug('Workbench preview: wardrobe gestures resolved against the chair', { context: CONTEXT, tool: toolName, characterId, found: <count found !== null>, missing: <count found === null> })`.

#### `lib/pascal/workbench.ts` (+34, `5cb0e0190`) — the wire types `:301-332`

```ts
export type PreviewWardrobeItemResolution =
  | { ref: string; found: { id: string; title: string; types: WardrobeItemType[]; origin: WardrobeOrigin; imageFileId: string | null } }
  | { ref: string; found: null; reason: 'not_found' | 'archived' | 'slot'; message: string };
export interface PreviewWardrobeResolution { index: number; items: PreviewWardrobeItemResolution[] }
```

#### `app/api/v1/custom-tools/route.ts` (both commits)

- Header `:9-13`: "`POST ?action=preview` — dry-run a definition through the
  one true execution core. Posts nothing, writes nothing. With a character in
  the chair, also says which garment each wardrobe gesture would have found
  (a read)."
- `resolveMetadata` `:150-176` now returns `Parsed<{ metadata; characterName? }>`:
  null/undefined → `{ metadata: {} }`; `benchCharacterId(metadata)` null →
  `{ metadata: <verbatim> }`; else `ctx.repos.characters.findById(characterId)`
  (OVERLAID): `!character` → `notFound('Character')` (404); found →
  `{ metadata: character.metadata ?? {}, characterName: character.name }`
  (comment: "The seated character's name answers `{{character}}`; a
  hand-typed sheet names nobody, so the placeholder renders as written, as
  for a run nobody made."); `CharacterVaultUnavailableError` → 422 with its
  message; anything else rethrows.
- **`benchCharacterId` `:178-183`** (the PORT arm):
  `if (!metadata || typeof metadata !== 'object' || Object.keys(metadata).length !== 1) return null;`
  `const characterId = metadata.characterId; return typeof characterId === 'string' && characterId.length > 0 ? characterId : null;`
  → `{ "characterId": "" }` (which fails `MetadataInputSchema`'s
  `strictObject({ characterId: min(1) })` branch and lands in the record
  branch) is now a HAND-TYPED SHEET: 200 with `{ characterId: "" }` as the
  sheet. Preview AND audit share it (both go through `parseBenchRequest`).
- `parseBenchRequest` `:191-219` returns `{ body, definition, metadata,
  ...(characterName ? { characterName } : {}) }` (truthy spread).
- `handlePreview` `:233-292`:
  - `:265` `...(characterName ? { character: characterName } : {})` — between
    `now: benchNowMs` and `state` in the overrides literal (order immaterial);
  - `:272-275` `const chairCharacterId = benchCharacterId(body.metadata);`
    (from the RAW BODY, not the resolved sheet) `wardrobeResolution =
    chairCharacterId ? await resolveBenchWardrobe(ctx.repos, chairCharacterId,
    result.effects, definition.name) : undefined;` — note it runs AFTER the
    deal, so `{{character}}` in a wardrobe ref is already the seated name;
  - **response spread order `:280-288`:** `...result`, `gate` (only
    `hasToolGate`), `progress` (only non-empty), **`wardrobeResolution`
    (only when defined) — LAST.**
- `handleAudit` `:294-330` — NO character, NO wardrobe resolution
  (`simulateOutcomes` never renders); only the `benchCharacterId` arm moves it.

#### `lib/wardrobe/wearable-ref.ts` (NEW, 51 lines — whole)

```ts
type RefTarget = Pick<WardrobeItem, 'id' | 'title'>;
export function matchWearableTitle<T extends RefTarget>(pool: readonly T[], title: string): T | undefined {
  const wanted = title.trim().toLowerCase();
  if (!wanted) return undefined;
  return pool.find((item) => item.title.toLowerCase() === wanted);
}
export function isTemplatedWearableRef(ref: string): boolean {
  return scanPlaceholders(ref).length > 0;
}
export function resolveWearableRef<T extends RefTarget>(pool: readonly T[], ref: string): T | undefined {
  const trimmed = ref.trim();
  if (!trimmed || isTemplatedWearableRef(trimmed)) return undefined;
  return pool.find((item) => item.id === trimmed) ?? matchWearableTitle(pool, trimmed);
}
```

Client-safe (imports `scanPlaceholders` from `lib/pascal/placeholders`). Its
doc says `findInPool` comes "through here" — only `findByTitle` actually does
(`findInPool`'s id half is `pool.get`, unchanged).

#### `lib/wardrobe/pool.ts` `:241-246` — behaviour-neutral

`findByTitle: (title) => matchWearableTitle(layers.own, title) ??
matchWearableTitle(wearable().filter((item) => !ownIds.has(item.id)), title)`
— the same trim / lower-case / own-first rule as before (titles not trimmed).
v5 `wearable_pool.rs:162-180` `find_by_title` already implements it (with the
own-layer-first-match note); no move.

#### `lib/pascal/placeholders.ts` (`a05c6f755`)

- `:23-34` `PlaceholderRef` gains `| { kind: 'character' }` between `now` and
  `params`.
- `:55-58` `classifyPlaceholder`: after `now`, comment "`character` is the
  rolling character's name — the one whose metadata, progressions and wardrobe
  the run reads and writes. A run nobody made has none, and the placeholder
  then renders as written." → `if (key === 'character') return { kind: 'character' };`
  (exact key; `character.x` stays `unknown`).

#### `lib/pascal/expressions.ts` — NO hunk; the grammar moves transitively

`:88-90` `isKnownRef(name) = classifyPlaceholder(name).kind !== 'unknown'`, so
`{{character}}` is now a known ref in an effect expression. The refusal text
`:133` is UNCHANGED (still omits `character`):
`` `{{${name}}} is not a reference this grammar knows — use value, roll, dice, llm, params.x, metadata.key, or state.path` ``.

#### `lib/pascal/custom-tools.ts` (`a05c6f755`; CONTEXT `'pascal.custom-tools'` `:107`)

- `OutcomeSubjects` `:793-…`: `character?: string` declared between `params`
  and `metadata` (doc: "The rolling character's name, for `{{character}}`.
  Absent when nobody in particular rolled — the placeholder then renders as
  written."). Its runtime key position in `executeCustomTool`'s literal
  `:1584-1593`: `value, roll, params, metadata, progress, state,
  ...(character ? { character } : {}), ...(llm ? { llm } : {})` — i.e.
  between `state` and `llm`. **Not wire-observable** (an internal object; no
  serialization) — record it, but no byte pin hangs on it.
- `PlaceholderSubjects` `:1319-1338`: `character?: string` after `params`.
- `resolvePlaceholderValue` `:1366-1367`: `case 'character': return vars.character;`
- `renderTemplate` `:1386-1430`, the miss switch `:1414-1416`:
  `logger.debug('Custom tool message references {{character}} but nobody in particular rolled', { context: CONTEXT });`
  — NO other field (same shape as the `{{now}}` line `:1411-1413`).
- `executeCustomTool` overrides `:1551-1552` `character?: string` ("The
  rolling character's name, for `{{character}}`. Absent for a run nobody
  made."); `:1573` `const character = overrides?.character?.trim() || undefined;`
  → reaches the consult prompt `:1579`, the subjects `:1590`, the message
  `:1596`, `chipLabel` `:1603`; `EffectSubjects` spreads `subjects` `:1608-1613`
  so the expression resolver (`resolveEffects` `:1195-1200`) and the wardrobe
  ref renderer (`resolveWardrobeGesture` `:1257-1263`, `renderTemplate(raw,
  subjects)`) see it.
- `simulateOutcomes` — no hunk (the audit never renders, never has a character).

#### `lib/pascal/custom-tool.types.ts` (`a05c6f755`)

- `:711-716` effect `value` describe gains `{{character}}` after `{{now}}`
  (describe text is not ported in Rust).
- **`:1013-1019` `announceAsCharacter`** — between `defaultVisibility`
  (`:1012`) and `parameters` (`:1020`):
  `z.boolean().optional().describe('true: the rolling character announces the outcome under their own name and face, in place of Pascal. ' + 'A run nobody made (the bare operator) is still announced by Pascal. Default false.')`
- `:1579` `KNOWN_TOP_LEVEL_KEYS = Object.keys(QtapCustomToolSchema.shape)` →
  grows to 17 by derivation.

#### `lib/pascal/tool-vocabulary.ts` (`a05c6f755`)

`ToolVocabulary.character: boolean` `:86-87` (doc "True when some rendered
string quotes `{{character}}`.") — **between `now` and `wears`**, in the
interface AND the returned object `:225`; `isEmptyVocabulary` adds
`!vocabulary.character` after `!vocabulary.now`; the `found` accumulator
`character: false`; `collectPlaceholders` `:292-294` `case 'character':
found.character = true; break;`.

#### `lib/pascal/run-service.ts` — this pair's hunks (sibling's in §0.1)

- `:67-71` `characterName?: string | null` ("The rolling character's name,
  for `{{character}}`, when the entrance already holds it. Absent with a
  `characterId` → read here.").
- `:110` `const characterName = await resolveCharacterName(characterId,
  params.characterName, chatId);` — after `progress`, BEFORE the cascade.
- `:140` `...(characterName ? { character: characterName } : {}),`.
- `:187-191` comment + `const announcerCharacterId =
  entry.definition.announceAsCharacter === true ? characterId : null;` →
  `:198` `...(announcerCharacterId ? { announcerCharacterId } : {}),` in the
  `postPascalResult` call (between `targetParticipantIds` and `pascalMeta`).
- `:222-233` DEBUG `Custom tool dealt` field order: `context, chatId, tool,
  tier, invokedBy, state, effects, whispered, announcedBy:
  announcerCharacterId ? 'character' : 'pascal', posted`.
- `:238-262` `resolveCharacterName(characterId, supplied, chatId)`:
  `if (!characterId) return null;` `if (supplied !== undefined) return
  supplied?.trim() || null;` then `getRepositories().characters.findByIdRaw(characterId)`
  → `character?.name?.trim() || null`; catch → DEBUG verbatim
  `logger.debug('Custom tool run: rolling character name unavailable, {{character}} renders as written', { context: CONTEXT /* 'pascal.run-service' */, chatId, characterId, error: getErrorMessage(error) })`
  → `null`.
- **Per entrance** (no hunk in `run-custom-handler.ts` nor the operator route
  `app/api/v1/chats/[id]/custom-tools/route.ts` — verified, neither file
  changes in the span): `run_custom` and the operator popup pass NO
  `characterName` → the RAW read; the trigger runner passes one; the bench
  bypasses the service.

#### `lib/pascal/trigger-runner.ts` (`a05c6f755`)

`:100` `let characterName: string | null = null;` `:113` `characterName =
character.name ?? null;` (from the OVERLAID `findById` it already did for
metadata `:103`) `:175` `characterName,` in the `runCustomTool` call (after
`metadata`). A run with no speaker character keeps `null` (and the service
returns `null` before reading anyway).

#### `lib/services/pascal/writer.ts` (`a05c6f755`; CONTEXT `'pascal'` `:43`)

- `PostPascalResultParams.announcerCharacterId?: string | null` `:102-110`.
- The message literal `:135-151`: `type, id, role: 'ASSISTANT', content,
  opaqueContent, attachments: [], createdAt, participantId: null,
  systemSender: 'pascal', systemKind: 'custom-tool-result',
  targetParticipantIds, pascalMeta, ...(announcerCharacterId ? {
  customAnnouncer: { kind: 'character', characterId: announcerCharacterId } }
  : {})` — **`customAnnouncer` LAST, key order `kind, characterId`.**
- INFO `:155-163` verbatim: `logger.info('[Pascal] Custom-tool outcome posted',
  { context: CONTEXT, chatId, messageId, tool: pascalMeta.tool, state:
  pascalMeta.state, whispered: Boolean(message.targetParticipantIds),
  announcerCharacterId: announcerCharacterId ?? null })`.
- Unchanged siblings: DEBUG `Pascal outcome dropped — no such chat`
  `{ context, chatId, tool }` `:128`; ERROR `[Pascal] Failed to post
  custom-tool outcome` `{ context, chatId, tool, error }` `:167-172`.

#### `lib/schemas/chat.types.ts` `:424-437` — doc only

`customAnnouncer` is now documented as also set "for a Pascal outcome whose
tool sets `announceAsCharacter` (then `systemSender` stays `'pascal'`)"; the
character arm "references a workspace character — a participant, or one who
is not in this chat. The renderer looks among the participants first, then the
off-scene character lookup". Schema unchanged.

#### Consequences with NO hunk (pre-existing readers now see Pascal rows with a `customAnnouncer`)

- `lib/chat/context/announcement-attribution.ts` `attributeAdhocAnnouncements`
  `:109-128`: `resolveAnnouncerName(m.customAnnouncer, …)` → a Pascal row
  gets `[<Name>] ` prefixed to `content` AND `opaqueContent` (idempotent);
  name from the context builder's `characters.findById` map
  (`context-builder.service.ts:903-920`, WARN `[Context] Could not resolve
  announcer name`). v5 `announcement_attribution.rs:106-200` +
  `services/message_context.rs:1235-…` are already the same rule.
- `lib/export/markdown-transcript.ts:94-100` — customAnnouncer wins (v5
  `services/markdown_transcript.rs:224-245` same).
- `courier-transport.service.ts:250-262` — **`systemSender` is checked FIRST**
  → the Courier keeps `[Staff: Pascal]` (v5 `services/courier_transport.rs:345-351`
  same). The commit message's "the Courier … attribute it to them" is FALSE
  for the shipped code (§8).
- `chat-activity.ts:46` / `conversation-summary-vault-bridge.ts:61` — skip
  `customAnnouncer` rows; Pascal rows were already skipped by `systemSender`.
- `transcript-projection.ts:266-279` — `offSceneCharacters` collects
  customAnnouncer ids NOT in the participants, so a participant announcer
  ships no off-scene card (hence the SPA's participants-overwrite map).

#### `public/schemas/qtap-custom-tool.schema.json` (`a05c6f755`; 43,493 → 44,021 bytes)

- `chipLabel.description`: placeholder list gains `{{character}}` after
  `{{dice}}` → "… ({{value}}, {{roll}}, {{dice}}, {{character}}, {{params.name}}, {{metadata.key}}, {{state.path}}, {{llm}}) …".
- NEW property after `defaultVisibility`, before `parameters`:
  ```json
  "announceAsCharacter": {
    "type": "boolean",
    "default": false,
    "description": "When true, the rolling character announces the outcome under their own name and avatar instead of Pascal, and the model reads it as that character's line. The roll record is unchanged. A run made by nobody in particular (the bare operator, with no character seat) is still announced by Pascal."
  },
  ```
- `llm.prompt.description`: "— {{value}}, {{roll}}, {{dice}}, {{character}}, {{params.name}}, {{metadata.key}}, {{state.path}} — but not {{llm}} itself. …".
- outcome `message.description`: inserts "{{character}} (the rolling
  character's name), " after the `{{dice}}` clause, and the tail becomes
  "… {{llm}} on a tool with no llm block, or {{character}} on a run nobody in particular made."

Byte-copy it from `git show e8e17b2d9:public/schemas/qtap-custom-tool.schema.json`
(44,021 bytes). ⚠ The LIVE checkout's copy is past the pin (Inventory, +313
lines) — §6 T12.

### §1.2 SPA

#### `lib/pascal/tool-draft.ts`

- `5cb0e0190` `:255-288`:
  ```ts
  export type WardrobeVantage =
    | { kind: 'character'; characterId: string }
    | { kind: 'container'; container: WardrobeContainer }
    | { kind: 'none' };
  export function defaultWardrobeVantage(mountPointId: string | null, destinations: CustomToolDestinations): WardrobeVantage {
    if (!mountPointId) return { kind: 'none' };
    const character = destinations.characters.find((c) => c.mountPointId === mountPointId);
    if (character) return { kind: 'character', characterId: character.characterId };
    const group = destinations.groups.find((g) => g.stores.some((store) => store.mountPointId === mountPointId));
    if (group) return { kind: 'container', container: { scope: 'group', id: group.groupId } };
    const project = destinations.projects.find((p) => p.stores.some((store) => store.mountPointId === mountPointId));
    if (project) return { kind: 'container', container: { scope: 'project', id: project.projectId } };
    if (destinations.general?.mountPointId === mountPointId) return { kind: 'container', container: GENERAL_CONTAINER };
    return { kind: 'none' };
  }
  ```
  Editor state, NEVER on `ToolDraft`, never serialized.
- `a05c6f755`: `ToolDraft.announceAsCharacter: boolean` `:305-306` ("The
  rolling character announces the outcome instead of Pascal. False = no key
  emitted.") after `defaultVisibility`; `newDraft` `:363` `false`;
  `draftFromDefinition` `:749` `definition.announceAsCharacter ?? false`;
  `definitionFromDraft` `:951` `if (draft.announceAsCharacter)
  doc.announceAsCharacter = true;` (after `defaultVisibility`, before
  `parameters`); `auditPlaceholders` `:1094-1100` adds `case 'character':`
  to the never-flagged group with "`{{character}}` names whoever rolls; a run
  nobody made renders it as written, which is the runtime's convention rather
  than an authoring slip."

#### `lib/wardrobe/wardrobe-container.ts` `:80-105` — `wardrobeReachLabel`

```ts
export function wardrobeReachLabel(origin: WardrobeOrigin | null | undefined, characterName?: string | null): string | null {
  if (!origin) return null
  switch (origin.scope) {
    case 'character': { const name = characterName?.trim() || origin.name.trim(); return `${name || 'this character'} only` }
    case 'group':   return `members of ${origin.name}`
    case 'project': return `chats in ${origin.name}`
    case 'general': return 'everyone'
  }
}
```
The four forms: **`<name> only`** (fallback `this character only`),
**`members of <group>`**, **`chats in <project>`**, **`everyone`**; null with
no origin.

#### `lib/hooks/use-wardrobe-vantage-items.ts` (NEW, 54)

`useWardrobeVantageItems(vantage | null)`: character →
`useCharacterWardrobeItems(characterId, { includeArchived: true })` (own +
groups + General, NO project tier) → `{ items, resolutionItems: items,
loading, fetched }`; container → `useWardrobeContainerItems(container, {
includeArchived: true })` → `{ items, resolutionItems, loading, fetched }`;
none/null → `{ items: [], resolutionItems: [], loading: false, fetched:
false }`. No new query key/route.

#### `components/wardrobe/wardrobe-reference-picker.tsx` (NEW, 336) — strings VERBATIM

- Props: `isOpen, onClose, vantage, characterName?, headerControl?, action?:
  'wear' | 'take_off', slot?, highlightId?, reference: 'title' | 'id',
  onReferenceChange, onPick(item, { reference })`. Body mounted only while
  open (filters reset per open).
- State defaults: `search ''`; **`showArchived = action === 'take_off'`**;
  `coversSlotOnly = true`; `expanded = new Set()`.
- Filter: archived hidden unless `showArchived`; `slot && coversSlotOnly &&
  !item.types.includes(slot)` hidden; `title.toLowerCase().includes(term)`.
- Outfits = `selectComposedOutfits(visible)`; garments =
  `selectGarments(visible).sort((a,b) => a.title.localeCompare(b.title))`
  grouped by `WARDROBE_SLOT_TYPES` order, "A multi-slot garment hangs under
  its first slot only, every badge shown."
- Row (`<li>`, `qt-bg-muted rounded` when highlighted): button
  `aria-label={`Pick ${item.title}`}`, `aria-current="true"` when highlighted,
  `data-testid="wardrobe-reference-option"`; thumbnail size 40; default star
  `<span title="Default outfit" aria-label="Default outfit">★{' '}</span>`;
  title; composite suffix `<span className="qt-text-secondary"> · outfit</span>`;
  `SlotBadge` per type; origin badge `qt-badge qt-badge-wardrobe-shared`
  = `wardrobeOriginLabel(item.origin) ?? (characterName ? `${characterName}'s own` : 'their own')`
  (**ASCII apostrophe here**); `archived` badge `qt-badge qt-badge-secondary`;
  line `{reach ? `Reach: ${reach}` : null}{reach ? ' · ' : null}{formatWearLine(wearOf(item))}`
  with `reach = wardrobeReachLabel(item.origin, characterName)`.
- Composite expander: `aria-label={isOpen ? `Hide what ${item.title} is made of` : `Show what ${item.title} is made of`}`,
  icon `chevron-down` / `chevron-right`; component list
  `aria-label={`${item.title} components`}`: `{component.title}` +
  `` ` — ${component.types.map(t => WARDROBE_SLOT_META[t].label).join(', ')}` ``
  or `<em>a piece no longer in reach</em>` (resolved from `resolutionItems`).
- Footer: `role="radiogroup" aria-label="Reference by"`, text `Reference by:`,
  radios (name `wardrobe-reference-form`) labelled `title` / `id`; when `id`,
  `<p className="qt-hint">{REFERENCE_FORM_HINT}</p>`; button `Cancel`.
- Modal: `BaseModal title="Choose a garment or outfit" maxWidth="2xl"
  showCloseButton`; `headerControl` first.
- `none` vantage → body renders NOTHING but the header control.
- Empty (fetched, zero items): `Nothing hangs here yet. Items are added from
  the Wardrobe.` + (only with the wardrobe-dialog provider) button
  `Open the Wardrobe` → `wardrobeDialog.open(vantage.kind === 'character' ?
  { characterId } : undefined)` then `onClose()`.
- Search `type="search"` placeholder `Search by title…`, `aria-label="Search the wardrobe"`, autoFocus;
  checkbox `Show archived`; when `slot`: `Covers {WARDROBE_SLOT_META[slot].label} only`.
- `Opening the wardrobe…` (loading && !fetched); `Nothing here answers to
  that.` (fetched, nothing visible); sections `aria-label="Outfits"` /
  `"Garments"` with `<h3>` `Outfits` / `Garments`; slot `<h4>`
  `WARDROBE_SLOT_META[slot].groupLabel`.
- `pick` → `onPick(item, { reference })` then `onClose()`.
- `REFERENCE_FORM_HINT` `:333-334`: `'An id names this exact garment and no other; a title names whichever garment of that name is nearest when the tool runs.'`

#### `components/custom-tools/WardrobeVantageSelect.tsx` (NEW, 184) — strings VERBATIM

- `encodeWardrobeVantage`: `char:<id>` / `encodeWardrobeContainer(...)` /
  `'none'`; `decodeWardrobeVantage`: `char:` with empty id → none; mangled →
  none.
- `wardrobeVantageOrigin`: character → `{ scope: 'character', id, name:
  characterName ?? '' }`; general → `{ scope: 'general', id: null, name:
  'Quilltap General' }`; project → name `?? 'this project'`; group → name
  `?? 'this group'`; a `character` container → name `''`.
- `wardrobeVantageName` = `origin.name || (scope === 'character' ? 'this character' : null)`.
- Label: `Browsing{' '}` + `<strong>{name}’s</strong>` (character) or
  `<strong>{name}</strong>` (container) + `{' '}wardrobe`.
- `<select aria-label="Whose wardrobe to browse">`: `<option value="none">— choose —</option>`;
  `<optgroup label="Characters">` (only when any) of `characterName`;
  `<optgroup label="Shared wardrobes">`: `Quilltap General`, then projects
  `Project · {projectName}`, then groups `Group · {groupName}`.
- Hint (`showHint` default true), `text-xs qt-text-secondary`:
  - character: `What {name} can reach: {name}’s own wardrobe, the wardrobes of {name}’s groups, and Quilltap General. Another character rolling this tool sees their own.`
    + when `homeProjectName`: ` This tool lives in {home}’s store; garments from {home}’s wardrobe will also be in reach when it runs there.`
  - container: `Every garment in this wardrobe, and who can reach it: {wardrobeReachLabel(origin)}.`
  - none: `Choose whose wardrobe to browse before picking garments.`

#### `components/custom-tools/SideEffectsSection.tsx` (+319) — strings VERBATIM

- New props `vantage = { kind: 'none' }`, `onVantageChange?`,
  `destinations?`, `homeProjectName = null`.
- `hasWardrobe = draft.effects.some(kind === 'wardrobe')`; pool =
  `useWardrobeVantageItems(hasWardrobe ? vantage : null)` ("Read nothing until
  a wardrobe row exists"); `characterName = vantage.kind === 'character' ?
  vantageName : null`; `referenceForm` state default `'title'` (lasts the
  editor session); `picking: { effectId, index, highlightId } | null`.
- `WardrobeVantageSelect` after the header row, before `None declared…`, only
  when `hasWardrobe && onVantageChange`.
- `WardrobeFields` gains `pool, poolFetched, vantage, vantageName,
  characterName, onChoose(index, highlightId)`:
  - `resolved[i] = vantage none ? undefined : resolveWearableRef(pool, ref)`;
  - `slotIsCoupled(g) = (wear && mode === 'add_to_slot') || take_off`;
    `coupled` only when every concrete (non-blank, non-templated) row
    resolved; slot `<option>` suffix **` (not covered)`** when `coupled &&
    !covered.has(slot)`;
  - slot advisory `data-testid="wardrobe-slot-advisory"`:
    `⚠ “{title}” covers {formatSlotLabels(item.types)} — not {WARDROBE_SLOT_META[slot].label}`;
  - resolved row (chip): button `aria-label={`Item ${index + 1}: ${item.title} — choose another`}`,
    `data-testid="wardrobe-reference-chip"`, thumbnail 28, title + ` · outfit`
    for a composite, slot badges, origin badge
    `wardrobeOriginLabel(item.origin) ?? `${characterName ?? 'Their'}${characterName ? '’s' : ''} own``
    (**curly ’** here → `Imogen’s own` / `Their own`), reach text
    (`qt-text-xs qt-text-secondary`), `archived` badge; the toggle button text
    `by id` / `by title`, `title={`${byId ? 'Referenced by id' : 'Referenced by title'} — click to switch. ${REFERENCE_FORM_HINT}`}`,
    `aria-label={byId ? 'Reference by title instead' : 'Reference by id instead'}`
    (click → `setItem(index, byId ? item.title : item.id)`), then trash;
    archived + `wear` → `<p>⚠ {archivedWearMessage(item.title)}</p>` =
    `⚠ Item "<title>" is archived and cannot be worn`;
  - unresolved row: text input as before + when `!templated && vantage !==
    none && poolFetched && ref.trim() !== ''` a badge
    `data-testid="wardrobe-reference-unresolved"` = character vantage
    `not in {vantageName ?? 'this character'}’s reach`, else
    `not in {vantageName ?? 'this wardrobe'}`; button **`Choose…`**
    `title="Browse the wardrobe and pick a garment or outfit"`; trash; when
    templated `<p className="qt-hint">chosen at run time</p>`. **Never a draft
    error** (validateDraft unchanged).
- `WardrobeReferencePicker` mounted once at the section end: open when
  `picking` targets a wardrobe effect; `headerControl` = a
  `WardrobeVantageSelect` (no `showHint` override → shows the hint); `action`
  `take_off` / `wear` / undefined; `slot` only when coupled; `onPick`:
  `items = gesture.items.length > 0 ? gesture.items : ['']`, value =
  `reference === 'id' ? item.id : item.title`, replace at `picking.index`.

#### `components/custom-tools/ProvingBench.tsx` (+167)

- Props `vantage?`, `onVantageChange?` (local fallback state).
  `chairCharacterId = vantage.kind === 'character' ? vantage.characterId : ''`.
- `FactSheet` becomes `{ mode: 'character' } | { mode: 'manual'; text }`; the
  initial mode is `character` when a chair exists; a vantage change seats the
  character (latched on the id, flips the sheet to character mode).
- The character `<select>` (`aria-label="Character whose fact sheet to use"`,
  first option `— nobody in particular —`) writes the VANTAGE (empty → `none`
  only if the vantage was a character).
- `BenchRoll` gains `wardrobeResolution?` and `chairName?` (computed at
  success from the destinations query when the sheet is in character mode).
- `gestureVerb` `:750-765`: wear → verb `wear`, suffix
  `` ` (${mode.replace(/_/g, ' ')}${slot ? ` ${slot}` : ''})` `` when mode ≠
  `wear`; take_off → `take off` + `` ` from ${slot}` ``; clear_slot → `clear`.
  `describeBenchGesture` re-built on it (same output as before).
- `BenchWardrobeLines` `:781-823`: no resolution or `clear_slot` → the old
  `→ wardrobe: {describe} <em>(would attempt)</em>`; else per item:
  found → `→ wardrobe: {verb} <thumb 28 inline> {title}{detail ? ` (${detail})` : ''}{suffix} <em>(would apply)</em>`
  with `detail = [formatSlotLabels(types), wardrobeOriginLabel(origin)].filter(Boolean).join(' · ')`;
  miss → `→ wardrobe: {verb} “{ref}”{suffix} — {why} <em>(would skip)</em>`,
  `why = reason === 'not_found' ? `not in ${chairName ? `${chairName}’s` : 'the chair’s'} reach` : entry.message`.
  Matched by `entry.index === effect.index`.

#### `components/custom-tools/WorkbenchEditor.tsx` (+50)

Destinations query (`queryKeys.customTools.destinations()`); `vantage`
(initial none), `vantageChosen`, `vantageSeed`; seed key
`loaded|<location.mountPointId ?? ''>` → while not chosen by hand,
`defaultWardrobeVantage(location?.mountPointId ?? null, destinations)` (so it
follows the file after Save As…); `chooseVantage` sets and latches chosen;
`homeProjectName` = the project whose stores contain the file's mount;
passed to SideEffectsSection (`homeProjectName` only under a character
vantage) and ProvingBench.

#### `components/wardrobe/wardrobe-item-thumbnail.tsx` — doc comment only (±4).

#### `a05c6f755` SPA — strings VERBATIM

- `BuilderForm.tsx:342-355` — after the `Results` select, same row:
  `<label title="On: the character who rolled announces the outcome under their own name and face. A roll nobody in particular made is still Pascal's to announce.">`
  checkbox `qt-checkbox` bound to `draft.announceAsCharacter`, text
  **`The character announces it`**.
- `OutcomesSection.tsx:852` — Insert value menu, after the `Parameter: …`
  items and before `Metadata key…`: `<MenuItem label="Character name" onClick={() => insertAtCursor('{{character}}')} />`.
- `CustomToolRunDialog.tsx:92-93` `character?: boolean` ("The tool quotes
  `{{character}}`. Absent on older servers.") after `now?`; `:881-883` after
  the `{{now}}` row: `{ placeholder: '{{character}}', meaning: 'the name of the character the run is made for' }`.
- `system-message-labels.ts:63-83` `getAnnouncementSenderName(message, announcerNames?)`:
  character announcer with id → `announcerNames?.[id]?.trim()` if non-empty;
  `custom` → `displayName?.trim()` if non-empty; else
  `getSystemSenderDisplayName(message.systemSender)` ("falls back to the
  Staff member rather than inventing one").
- `SalonView.tsx:253-264` `announcerNames`: off-scene cards first (`c.name`),
  then participants overwrite (`p.character.id → p.character.name`); threaded
  through `VirtualizedMessageList` → `MessageRow` (+memo compare) →
  `AnnouncementGroup` / `AnnouncementChip` / `AnnouncementBarContents`; the
  bar (`MessageRow.tsx:312`) and both chips (`AnnouncementChip.tsx:67,109`)
  use `getAnnouncementSenderName`. `getMessageAvatar` `:1321-1325` — comment
  only: "customAnnouncer takes precedence over systemSender: when both are
  set, the named speaker wears the bubble."

---

## §2 The v5 seams

### §2.1 Server (Rust)

| unit | v5 site | change |
|---|---|---|
| `{{character}}` classify | `pascal/placeholders.rs:55-81` (`PlaceholderRef`), `:87-95` (`classify_placeholder`) | `Character` variant after `Now`; `"character" => return PlaceholderRef::Character` |
| grammar | `pascal/expressions.rs:131-133` `is_known_ref` | none (transitive); refusal text `:212` unchanged |
| render / resolve | `pascal/custom_tools.rs:1494-1511` `TemplateVars` (+`character: Option<&'a str>`), `:1518-1630` `render_template` (new miss arm beside `Now` `:1581-1588`: `tracing::debug!(target: "quilltap::pascal", "Custom tool message references {{{{character}}}} but nobody in particular rolled")`, no fields), `:1641-1672` `resolve_placeholder_value` (`Character => vars.character.map(String)`) | compile-forced exhaustive arms |
| subjects | `OutcomeSubjects` `:526-551` (+`character`), constructors `:1690` (test), `:2196`, `:2263`, `:2396` (`simulate_outcomes` → `None`); `EffectSubjects` `:1150-1159`; `resolve_effects` `TemplateVars` `:1339-1351` | thread it |
| execute | `execute_custom_tool` `:2109-2300` (positional args — add `character: Option<&str>`, v4's `trim() \|\| undefined` applied INSIDE, `js_trim`); prompt vars `:2176-2190`, message `:2222-2236`, chip `:2241-2255`, effects `:2262-2278` | callers: `run_service.rs:230`, `api/custom_tools.rs:1341`, harness `pascal_custom_tools_execution_equivalence.rs:402` (+ in-file tests) |
| definition key | `pascal/custom_tool_types.rs:1226-1276` struct (`announce_as_character: Option<bool>`, `#[serde(rename = "announceAsCharacter", skip_serializing_if = "Option::is_none")]` between `default_visibility` and `parameters`); `safe_parse` `:2852-…` — an `opt_bool("announceAsCharacter", …)` AFTER the `defaultVisibility` arm `:2981-2994` and BEFORE `parameters` `:2996` (issue ORDER); the literal `:3155-3172`; `KNOWN_TOP_LEVEL_KEYS: [&str; 16]` `:4000-4017` → 17 | the `unknown-keys-*` definition rows are the tripwire |
| vocabulary | `pascal/tool_vocabulary.rs:55-108` (`character: bool` between `now` `:96` and `wears` `:101`), `is_empty_vocabulary` `:111-128`, accumulator `:145`, output `:303`, scanner `:334` | wire order = v4's |
| run service | §0.1 table; tri-state `character_name` (§6 T2) | + `resolve_character_name` via `crate::db::characters_read::find_by_id_raw_or_none` (`db/characters_read.rs:305-307`) |
| trigger runner | `pascal/trigger_runner.rs:141-180` (keep `character.get("name")`), literal `:256-273` | supply `Some(name-or-None)` |
| writer | `services/pascal_writer.rs:78-90` (`announcer_character_id: Option<String>`), `build_pascal_message` `:107-143` (append `customAnnouncer` AFTER `pascalMeta`, `{kind, characterId}`), `post_pascal_result` `:148-168` | ⚠ v5 logs NOTHING here today (no INFO, no `dropped` DEBUG, no ERROR) — §6 T14 |
| bench metadata | `api/custom_tools.rs:1031-1100` `resolve_metadata` — the `{characterId}` test at `:1066-1072` (add `!is_empty()`), return `(metadata, Option<String> name)` from the overlaid character `:1080-1099`; `BenchRequest` `:941-946`; `parse_bench_request` `:957-970`; the doc comment `:1038-1053` PINS the OLD arm (item 1: "`{ characterId: "" }` … hydrating the empty id into a 404") — rewrite to v4's new rule | audit shares it (`custom_tool_audit` `:1390`) |
| bench preview | `custom_tool_preview` `:1264-1385`: pass `character` into `execute_custom_tool` `:1341-1352`; after `gate` `:1360-1374` and `progress` `:1378-1382`, insert `wardrobeResolution` LAST | NEW module `pascal/bench_wardrobe.rs` (register in `pascal/mod.rs`) or a section of `pascal/workbench.rs` (561 lines) |
| bench lens reads | `wearable_pool.rs:372-…` `load_wearable_pool(main, mount, character_id, Some(&[]), LoadWearablePoolOptions { operator: true, ..Default::default() })`; `wear_ops.rs:125-136` `find_in_pool`, `:150-187` `resolve_wearable`, `:112-123` `wardrobe_item_not_found_message`, `WearableRefusal::as_str` `:89-97` | read through `db.read_main` + `db.read_mount_index`; the read error IS the WARN's v5 home |
| `find_by_title` | `wearable_pool.rs:162-180` | no move (behaviour-neutral) |
| consequences | `announcement_attribution.rs`, `services/message_context.rs:1235-…`, `services/markdown_transcript.rs:224-245`, `services/courier_transport.rs:345-380`, `chat_activity.rs:58-94`, `services/conversation_summary_vault_bridge.rs:54-75` | no code move; corpus growth only |

### §2.2 SPA (Angular)

| unit | v5 site | change |
|---|---|---|
| placeholder twin | `apps/web/src/app/pascal/placeholders.ts:33-45`, `:55-…` | `{ kind: 'character' }` + the classify line |
| definition twin | `pascal/custom-tool-types.ts:798-815` (`QtapCustomTool.announceAsCharacter?: boolean` after `defaultVisibility`), parse arm after `:1920-1925`, literal `:2002` (after `defaultVisibility`), `KNOWN_TOP_LEVEL_KEYS` `:2549-2566` (16 → 17), comment "16 since `362426c8f`'s `triggers`" | twin of the Rust arm |
| draft | `pascal/tool-draft.ts:258-292` (`ToolDraft.announceAsCharacter`), `KNOWN_KEY_ORDER` `:298-314` (insert after `defaultVisibility`), `newDraft` `:336-…`, `draftFromDefinition` `:696-…` (`:724`), `definitionFromDraft` `:915-…` (`:931`), `auditPlaceholders` `:1088-1106` (`case 'character':` beside `'now'`); NEW `WardrobeVantage` + `defaultWardrobeVantage` | |
| wearable-ref | NEW `apps/web/src/app/wardrobe/wearable-ref.ts` (no v5 twin exists — grep found none) | uses `pascal/placeholders.ts` `scanPlaceholders` `:99` |
| reach label | `wardrobe/wardrobe-container.ts` (`wardrobeOriginLabel` `:61`) — add `wardrobeReachLabel` | |
| vantage loader | `wardrobe/wardrobe.api.ts` `loadCharacterWardrobeItems` `:380` (pass NO `projectId`), `loadWardrobeContainerItems` `:307` (`includeArchived: true` both; `.resolutionItems`) — precedent `wardrobe/wardrobe-item-editor.ts:654-689` | a small signal-based loader (no query key) |
| picker | NEW `wardrobe/wardrobe-reference-picker.ts` from `slot-ui.ts` `SlotBadge` `:28`, `wardrobe-item-thumbnail.ts` `:59`, `composed-outfits.ts:40,48`, `wear-display.ts:36,57`, `slot-meta.ts` (`WARDROBE_SLOT_META`, `formatSlotLabels` `:130`), `equipped-slots.ts:53` `isComposite`, `wardrobe-dialog.service.ts:35` (Open the Wardrobe) | |
| vantage select | NEW `screens/custom-tools/wardrobe-vantage-select.ts` | `encodeWardrobeContainer` `wardrobe-container.ts:106` |
| side effects | `screens/custom-tools/side-effects-section.ts` (mount `:150-155`, inputs `:265-269`), `wardrobe-fields.ts` (item rows `:113-134`, slot options `:104-106`, inputs `:150-155`); `archivedWearMessage` `wardrobe/staged-live-outfits.ts:319` | |
| bench | `screens/custom-tools/proving-bench.ts` (`FactSheet` `:58`, select `:228-240`, `sheet` `:492`, `characterId` `:582-585`, `:717`, `:742-750`, `benchMetadata` `:758-765`, wardrobe line `:170-176`, `describeBenchGesture` `:957-972`); `workbench.api.ts` `BenchRoll` `:117-123` | |
| editor | `screens/custom-tools/workbench-editor.ts` (`<qt-side-effects-section>` `:214`, `<qt-proving-bench>` `:264`, `location` `:336`); `workbench.api.ts:139` `fetchDestinations` | |
| toggle / insert | `screens/custom-tools/builder-form.ts:224-236` (after Results), `outcomes-section.ts:839-857` (between the `Parameter:` loop and `Metadata key…`) | |
| run dialog | `chat/custom-tools.api.ts:74-75` (`character?: boolean` after `now?`), `chat/custom-tools-popup.ts:798-803` (row after `{{now}}`) | |
| Salon | `chat/system-message-labels.ts:75-77` (add `getAnnouncementSenderName`); `chat/message-row.ts:641-645` `staffSender` (has `chat` input `:527`); `chat/chat-view-model.ts:354` (chip `sender`, `buildRenderItems` needs the names) and the stale comment `:94-96` ("the writer sets one or the other, never both"); `chat/announcement-group.ts` (`chat` input `:145`) | `announcerNames` from `ChatDetail` (offScene then participants) |
| schema | `apps/web/public/schemas/qtap-custom-tool.schema.json` (byte copy at the pin, 44,021) | guard `crates/quilltap-harness/tests/public_schemas_vendor_guard.rs:40-55` byte count 43,493 → 44,021 |
| contract | `apps/web/src/app/core/core-contract.ts:7615-7642` `CustomToolRunResult`; `:3044-3048` `CustomAnnouncer`; append-only fenced regions `// === P4.Dxxx: … ===` / `// === end P4.Dxxx ===` (precedent `:4524-4582`, `:4584-4691`) | |

---

## §3 Shared contract (server ↔ SPA) — VERBATIM

### §3.1 The `wardrobeResolution` wire (preview body, LAST key)

```ts
// === P4.D<SERVER>: the Workbench wardrobe lens (append-only) ===
export type PreviewWardrobeItemResolution =
  | { ref: string; found: { id: string; title: string; types: WardrobeItemType[]; origin: WardrobeOrigin; imageFileId: string | null } }
  | { ref: string; found: null; reason: 'not_found' | 'archived' | 'slot'; message: string };
export interface PreviewWardrobeResolution { index: number; items: PreviewWardrobeItemResolution[] }
// BenchRoll += wardrobeResolution?: PreviewWardrobeResolution[]  (chairName is SPA-local, never on the wire)
// === end P4.D<SERVER> ===
```

JSON key order (bytes): body = `…run result…, "gate"?, "progress"?,
"wardrobeResolution"?`; entry `{"index":0,"items":[…]}`; hit
`{"ref":"…","found":{"id":"…","title":"…","types":[…],"origin":{"scope":"…","id":…,"name":"…"},"imageFileId":null}}`;
miss `{"ref":"…","found":null,"reason":"not_found","message":"Wardrobe item not found with title \"…\""}`.
Present ONLY when the RAW body's `metadata` is exactly `{ characterId:
<non-empty string> }`, the character resolved (else 404/422 earlier), the run
carries ≥1 non-`clear_slot` wardrobe gesture that resolved (not skipped), and
the pool read succeeded.

### §3.2 `resolveMetadata` / `characterName`

Server-internal: `resolve_metadata` returns `(Map, Option<String>)` — the
seated character's overlaid `name` (untrimmed; `execute_custom_tool` trims),
`None` for a hand-typed sheet. `{ "characterId": "" }` → hand-typed (200,
sheet `{ "characterId": "" }`), for preview AND audit. No new wire key; the
preview's `message` / `chipLabel` / `effects` simply render `{{character}}`.

### §3.3 The `customAnnouncer` message row

```json
{ "type":"message","id":"…","role":"ASSISTANT","content":"🎲 **…**\n\n…","opaqueContent":"…",
  "attachments":[],"createdAt":"…","participantId":null,"systemSender":"pascal",
  "systemKind":"custom-tool-result","targetParticipantIds":null,"pascalMeta":{…},
  "customAnnouncer":{"kind":"character","characterId":"<rolling character id>"} }
```

Set iff the definition says `announceAsCharacter: true` AND the run has a
non-empty `characterId`. `pascalMeta` unchanged. SPA type already exists
(`core-contract.ts:3044` `CustomAnnouncer`, `MessageDto.customAnnouncer`
`:3238`) — no contract growth.

### §3.4 The definition key

`announceAsCharacter?: boolean` — position between `defaultVisibility` and
`parameters` in BOTH twins' parse order, struct/literal order, and
`KNOWN_TOP_LEVEL_KEYS` (17, schema order: `$schema, name, title, chipLabel,
triggers, description, disabled, availableWhen, withheldWhen, revealOdds,
defaultVisibility, announceAsCharacter, parameters, roll, llm, effects,
outcomes`). A non-boolean → Zod's `announceAsCharacter: Invalid input:
expected boolean, received <type>` through `formatDefinitionIssues` (record
from the oracle, don't hand-write). The draft emits it only when `true`.
`ToolVocabulary`/`CustomToolReferences` gain `character` (after `now`).

### §3.5 e2e gating

The SPA lane's Playwright beats that need the server (the preview's would
apply / would skip lines; a `{{character}}` roll; a character-announced bubble
naming the character on the Staff bar) ride behind
`const P4D<SERVER>_SERVER_LANDED = false` (the `P4D288_SERVER_LANDED`
precedent, `e2e/scenario-browser-flow.spec.ts:46`; P4.D307's
`P4D304/305/306_SERVER_LANDED`), flipped by the unifier at §S.9. Pure UI
beats (the picker over a seeded wardrobe, the chip, the advisory, the toggle
round-trip through Save) are LIVE in-lane. The SPA lane develops against §3.1
without the server: its component specs stub the preview response.

---

## §4 Differential plan

Families that MOVE (regen from the PINNED worktree, never the live checkout):

1. **`pascal_custom_tool_definition_equivalence`** (generator
   `harness/oracle/cases/pascal-custom-tool-definition.ts`, 949 lines; the
   SHARED corpus — server family + the SPA copy
   `apps/web/src/testing/fixtures/pascal-custom-tool-definition.oracle.ndjson`,
   449 rows). Grow rows: `announce-as-character-true` / `-false` /
   `-string` / `-null` (null is NOT `optional()` — Zod refuses), effect value
   `"{{character}} + ' wins'"` NOW LOADS (was refused), wardrobe item
   `"{{character}}'s armour"`, `unknown-keys-*` rows where
   `announceAsCharacter` is no longer reported. **Regenerated ONCE from the
   pin by the unifier for both consumers** (P4.D307 R-2 / R-F rule); each lane
   runs the pin regen on its branch and records the sha.
2. **`pascal_custom_tools_execution_equivalence`** (jest
   `pascal-custom-tools-execution.test.ts`): rows with a `character`
   override — message, chip label, consult prompt (scripted invoker echoes
   the prompt), effect expression `{{character}}` (string concat), wardrobe
   ref rendering, absent (`{{character}}` stays + the DEBUG captured),
   whitespace-only name (trimmed → absent), and the renderTemplate unit rows
   (v4's own `substitutes…` / `stays as written…`).
3. **`pascal_expressions_equivalence`** (`pascal-expressions.ts` — it already
   records `classifyPlaceholder`): `character`, `character.x`, ` character `
   (trimmed by callers), plus a parse row `{{character}}` accepted.
4. **`pascal_tool_vocabulary_equivalence`**: `{{character}}` in message, chip,
   prompt, wardrobe item → `character: true`; the existing rows grow the
   `character: false` key (wire order after `now`).
5. **`pascal_workbench_route_equivalence`** (corpus
   `harness/oracle/fixtures/workbench-route-cases.json`, 71 cases; fixtures
   `crates/quilltap-web/tests/fixtures/workbench-{main,mount}.db`): the
   `preview-empty-character-id` row (`:435-442`) **trips BY DESIGN** — re-aim
   it (now 200 with the sheet verbatim; v4's gated definition then declines
   metadata), and add `audit-empty-character-id`. Add
   `preview-character-name-as-imogen` (a definition whose message/chip say
   `{{character}}`), `preview-character-hand-typed` (renders as written).
   Wardrobe rows need wardrobe items in the workbench fixture — see §5.
6. **`announcement_attribution_equivalence`** (`announcement-attribution.ts`):
   grow `attributeAdhocAnnouncements` rows with `systemSender: 'pascal'`,
   `systemKind: 'custom-tool-result'` + a character announcer (named → tag;
   deleted → untouched; opaqueContent tagged), and the no-announcer Pascal row
   (untouched — staff fallback only for `systemKind === 'announcement'`).
7. **Tier-2 entrances** — `pascal_run_custom_handler_equivalence`,
   `pascal_custom_tools_route_equivalence`, `pascal_trigger_runner_equivalence`
   (fixture `harness/oracle/fixtures/pascal-run-custom.json` + the
   `pascal-run-custom-{main,mount}.db` pair): a definition with
   `announceAsCharacter: true` and a `{{character}}` message, run as a
   character (row carries `customAnnouncer`, message names the RAW name) and
   as nobody (no announcer, as written); the trigger arm (overlaid name).
   These diff the persisted rows — the `customAnnouncer` column
   (`db/chats_messages.rs:161,1075,1248`).
8. **`public_schemas_vendor_guard`**: byte count 43,493 → 44,021 at the pin.

Families that do NOT move (ledger over-lists them): `pascal_run_custom_equivalence`
(the description builder — no hunk in `run-custom-tool.ts`; it quotes raw
templates); `pascal_writers_equivalence` (pure builders only; the announcer is
on the post). `wearable_pool_equivalence` — `findByTitle` behaviour-neutral.
`p4_6ay_workbench_wire_contract` pins Request shapes only — add the new
response type there only if the SPA contract grows a Response variant (it
doesn't; `CustomToolPreview(Value)`).

NEW families:

- **`pascal_bench_wardrobe_equivalence`** (tier 1, tsx, Node 24): drive v4's
  REAL `buildWearablePool` + `executeCustomTool` (effects) +
  `resolveBenchWardrobeAgainst` over fixed tiers (v4's own test's pool:
  own `Sundress` top+bottom, archived `Old Coat`; General `Cloak of Night`),
  rows = the seven v4 `it`s minus the mock-only fail arm, plus `take_off`
  with a slot, `replace`, multi-item gestures, a `clear_slot` beside a wear
  (index preserved), a templated ref left unrendered, an id that is also a
  title, case/trim. Closest recipe: `wear_ops_equivalence` (`$N/node --import
  tsx $V5W/harness/oracle/cases/wear-ops.ts`, builds pools from fixed tiers)
  and `wearable_pool_equivalence`.
- **SPA corpora** (`apps/web/src/testing/fixtures/*.oracle.ndjson`, the
  `pascal-progress.oracle.ndjson` precedent): `wearable-ref.oracle.ndjson`
  (v4's real `resolveWearableRef` / `matchWearableTitle` /
  `isTemplatedWearableRef`), `wardrobe-vantage.oracle.ndjson`
  (`defaultWardrobeVantage` over v4's six test destinations +
  `wardrobeReachLabel` four forms + null), and
  `getAnnouncementSenderName` rows (v4's three `it`s + custom + blank-name).

---

## §5 Fixtures

- **Workbench fixture pair** (`harness/oracle/fixtures/build-workbench-fixture.ts`,
  no wardrobe today — grep finds none): wardrobe preview route rows need
  Imogen's vault `Wardrobe/*.md` items + a General item. Prefer keeping the
  route family's wardrobe rows to the KEY's presence/absence and the
  hand-typed / empty-id arms, and put the resolution matrix in the NEW tier-1
  family (no DB). If the pair is rebuilt, mind the vintage hazard (memory:
  "Rebuilt fixture pair vintage" — diff `sqlite_master`, run
  `runV4Migrations` in the builder) and that `workbench.json` is shared with
  `pascal_workbench_equivalence`.
- **`pascal-run-custom.json`** grows ONE definition (`announced_dresser` or
  similar: `announceAsCharacter: true`, message `{{character}} rolls {{value}}`,
  chip `For {{character}}`) — shared by three tier-2 families (handler, route,
  trigger runner) and with the SIBLING lane's outfit-history rows; merge by
  id at union.
- Character names in fixtures: one with surrounding whitespace in the RAW row
  (proves the trim), one vault-broken character (the run-service raw read
  still names it; the trigger runner WARN-skips it).

---

## §6 Traps

- **T1 — raw vs overlaid per entrance.** `run_custom` + the operator popup
  supply no name → `find_by_id_raw_or_none` (v4 `findByIdRaw` =
  `_findById`, a `safeQuery` FALLBACK — a failed row read logs the repository
  ERROR and answers null, never the catch). `name` is NOT a vault-managed
  field (`vault-overlay/schema.ts:207-223`), so raw == overlaid except on a
  broken vault — where the raw read STILL names the character. Do not reuse
  `tools/run_custom.rs:583` `read_character_hydrated` (overlaid; fails on a
  broken vault). The trigger runner and the bench use their overlaid reads.
- **T2 — tri-state `characterName`.** v4 distinguishes `undefined` (read
  here) from `null` (no name, no read) from a string (`trim() || null`). The
  trigger runner always supplies (`character.name ?? null`). Model it as
  `Option<Option<&str>>` (or an enum), not `Option<&str>`.
- **T3 — two trims.** `resolveCharacterName` trims `|| null`; then
  `executeCustomTool` trims `|| undefined` again. The bench passes the
  UNTRIMMED name only when truthy. A whitespace-only name renders as written
  + the `{{character}}` DEBUG. Use `js_trim`.
- **T4 — `benchCharacterId` reads the RAW body**, and only `length === 1`
  objects; `{ characterId: "x", other: 1 }` stays hand-typed (the existing
  `preview-character-id-plus-key` row).
- **T5 — the `preview-empty-character-id` row trips BY DESIGN** at the pin;
  re-aim, don't "fix" v5 back; also rewrite `resolve_metadata`'s doc item 1.
- **T6 — the not-found message** is `(null, ref)` — NOT `resolve_wearable`'s
  `(ref, ref)` string; the take_off arm never calls `resolve_wearable`.
- **T7 — `result.effects` are post-render**: `{{character}}` / `{{params.x}}`
  in a ref are substituted before the lens; a ref that keeps an unknown
  placeholder is looked up as written (and misses).
- **T8 — the WARN arm.** v4's real loader never throws; v5's
  `load_wearable_pool` never fails either. The v5 home is the
  `read_main`/`read_mount_index` failure (a degraded mount index); pin it
  there, and record the arm as reachable-by-mock-only in v4.
- **T9 — `mode ?? 'wear'`**; `slot` only matters for `add_to_slot`;
  `clear_slot` gestures and non-wardrobe effects contribute nothing; zero
  qualifying gestures → NO key (not `[]`).
- **T10 — spread order**: `wardrobeResolution` after `progress` after `gate`.
- **T11 — key order of the posted row**: `customAnnouncer` after `pascalMeta`
  (the returned `PostedPascalMessage.message` / SSE frame is the observable).
- **T12 — the LIVE v4 checkout is past the pin** (`a6292f169`, Inventory):
  `route.ts`, `custom-tool.types.ts`, `custom-tools.ts`, `placeholders.ts`,
  `run-service.ts`, `tool-draft.ts`, `tool-vocabulary.ts`, `workbench.ts`,
  `ProvingBench.tsx`, `SideEffectsSection.tsx`, `WardrobeVantageSelect.tsx`,
  `BuilderForm.tsx`, `OutcomesSection.tsx` and the public schema all differ
  from the pin (`git diff --stat e8e17b2d9 a6292f169`, 24 files / +2,506).
  Every regen from the pinned worktree; `public_schemas_vendor_guard` with
  `QT_V4_CHECKOUT` at the live tree goes red by design.
- **T13 — "never both" is now false.** v5 `chat-view-model.ts:94-96` says
  the writer sets one or the other; `resolveMessageAuthor` already checks
  `customAnnouncer` first (so the avatar is right) — but the Pascal row is
  still carved OUT of the chip path (`isAnnouncementChip` `:270-276`), so the
  visible fix is `message-row.ts`'s `staffSender`. The Courier keeps
  `[Staff: Pascal]` in BOTH v4 and v5 (Staff checked first).
- **T14 — v5's `post_pascal_result` is silent.** v4's INFO `[Pascal]
  Custom-tool outcome posted` (now carrying `announcerCharacterId`), the
  `dropped — no such chat` DEBUG and the ERROR have no v5 home; adding the
  field means porting the INFO line (a pre-existing gap — decide in the order
  whether this lane closes it; the writer runs on the caller thread, but
  `db.write` closures are invisible to `captured_with` — memory "Capture rig
  vs the writer thread").
- **T15 — the struct literal is shared with the sibling lane** (§0.1).
- **T16 — `execute_custom_tool` is positional** (10 args, `too_many_arguments`
  allowed); add `character` at the END to keep the diff obvious; every caller
  listed in §2.1.
- **T17 — apostrophes.** Picker origin fallback `` `${characterName}'s own` ``
  / `their own` (ASCII); the chip's `Imogen’s own` / `Their own` (U+2019);
  reach/hint strings use U+2019; the BuilderForm title's `Pascal's` is ASCII.
- **T18 — `KNOWN_TOP_LEVEL_KEYS`** is hand-written in BOTH twins (Rust
  `[&str; 16]`, SPA array + the SPA draft's `KNOWN_KEY_ORDER`); a stale list
  keeps `announceAsCharacter` in `unknownKeys` and re-emits it in the wrong
  position.
- **T19 — LIVE-DATA (the shared doc store).** Today a v4-authored tool whose
  effect `value` says `{{character}}` is REFUSED at load by v5 (the grammar
  refusal) and DROPS OFF v5's roster; in a message / chip / wardrobe ref it
  renders as written; `announceAsCharacter` loads (tolerated unknown key) but
  v5 posts as Pascal, and v5's Workbench `unknownKeys` keeps the key on save
  (re-emitted at the END, not in schema order). After the port: none of
  these. The SPA twin also refuses such a file today (`draftFromDefinition` →
  null).
- **T20 — Angular `{{`.** Every `{{character}}` literal in a template must be
  a TS constant (the `ITEM_PARAM_PLACEHOLDER` precedent,
  `wardrobe-fields.ts:28-32`).

---

## §7 v4 tests to mirror

Server (`5cb0e0190`):
- `__tests__/unit/app/api/v1/custom-tools/route-preview-wardrobe.test.ts` —
  `POST ?action=preview — wardrobe resolution`: `finds a garment by title`;
  `finds a garment by id`; `reports an unknown title as not found`; `refuses
  an archived garment under wear, but finds it under take_off`; `refuses a
  slot the garment does not cover under add_to_slot`; `resolves nothing for a
  hand-typed fact sheet`; `still returns the roll when the wardrobe cannot be
  read`.
- `__tests__/unit/lib/wardrobe/wearable-ref.test.ts` — `resolveWearableRef`:
  `prefers an id over a title that happens to match`; `matches a title
  case-insensitively, trimmed`; `takes the first title match in pool order`;
  `never resolves a templated reference`; `resolves nothing for a blank or
  unknown reference`; `matchWearableTitle`: `ignores ids`;
  `isTemplatedWearableRef`: `spots a placeholder anywhere in the string`.
- `__tests__/unit/lib/wardrobe/wearable-ref-delegation.test.ts` — `findInPool
  title half`: `delegates to matchWearableTitle`; `keeps own-items-first,
  archived included`; `finds by id without the title rule`.
- `__tests__/unit/lib/pascal/wardrobe-vantage.test.ts` —
  `defaultWardrobeVantage`: `opens a character vault's tool on that
  character`; `opens a group store's tool on the group wardrobe`; `opens a
  project store's tool on the project wardrobe`; `opens a General-store tool
  on Quilltap General`; `opens an unattached store or an unsaved file on
  nobody`; `prefers the character when a vault is also attached to a group`.
- `__tests__/unit/lib/wardrobe/wardrobe-container.test.ts` —
  `wardrobeReachLabel`: `names who can reach each scope`; `is null without an
  origin`.

Server (`a05c6f755`):
- `custom-tools-execution.test.ts` `{{character}}`: `substitutes the rolling
  character's name`; `stays as written when nobody in particular rolled`;
  `reaches the message, the chip label and a wardrobe reference through
  executeCustomTool` (`chipLabel: 'Armour for {{character}}'`, item
  `"{{character}}'s armour"`, message `'{{character}} is suited.'` →
  `Charlie is suited.` / `Armour for Charlie` / `["Charlie's armour"]`).
- `run-service.test.ts` `the rolling character`: `renders {{character}} with
  the name the entrance supplied, without a read`; `reads the name when the
  entrance did not supply one`; `leaves {{character}} as written when the
  name cannot be read` (mock-only rejection — §6 T1); `posts under Pascal
  unless the tool says the character announces it`; `hands the post to the
  rolling character when announceAsCharacter is on`; `leaves a run nobody
  made with Pascal, even when announceAsCharacter is on`.
- `tool-vocabulary.test.ts`: `reports a tool that quotes {{character}}` (+
  `character: false` in two whole-object expectations).
- `services/pascal/writer.test.ts` `postPascalResult`: `posts as Pascal, with
  no announcer, by default`; `wears the announcing character's name and face,
  and stays Pascal's record`.
- `tool-draft.test.ts`: the §6.2 round-trip gains `announceAsCharacter: true`;
  defaults emit no key (normalize deletes `false`).

SPA:
- `__tests__/unit/components/custom-tools/wardrobe-reference-picker.test.tsx`
  — `WardrobeReferencePicker`: `sections outfits apart from garments, grouped
  by slot`; `filters by title`; `hides garments that do not cover the named
  slot`; `hides archived items for wear and shows them for take_off`; `picks
  by title, and by id when asked`; `says nothing but the prompt for the none
  vantage`; `shows the empty state for a wardrobe with nothing in it`.
  `SideEffectsSection — wardrobe references`: `renders a resolved reference as
  a chip with its slots and origin`; `names a character-owned garment as
  theirs`; `keeps a templated reference as text, chosen at run time`;
  `advises on an unresolved reference without making it a draft error`;
  `warns when an archived garment is to be worn`; `flags slots the chosen
  items do not cover under add_to_slot` (`“Sundress” covers Top, Bottom — not
  Footwear`, option `Footwear (not covered)`); `switches a chip between title
  and id`; `fills a reference from the picker, and the vantage never reaches
  the file`; `serializes byte-identically whatever the vantage`.
- `app/salon/[id]/components/system-message-labels.test.ts`
  `getAnnouncementSenderName`: `names the Staff member when nobody else
  announces it`; `names the announcing character on a character-announced
  Pascal outcome`; `falls back to the Staff member when the announcer cannot
  be named`.
- No ProvingBench / WorkbenchEditor / BuilderForm / run-dialog test in the
  span — the SPA lane writes its own (would apply / would skip lines, the
  latched vantage seed, the toggle round-trip, the `{{character}}` row).

---

## §8 Open questions / ledger disagreements

1. **Sibling-file premise.** `tool-draft.ts` and `custom-tools/route.ts` are
   touched ONLY by this pair in the span; `run-service.ts` is the only mixed
   file (§0.1). `wardrobe-effects.ts`, `tool-executor.ts` and
   `side-effects.ts` are the sibling's alone.
2. **Ledger over-listing.** The `a05c6f755` row lists
   `pascal_run_custom_equivalence` and `pascal_writers_equivalence` as
   intersecting; neither moves (no description hunk; the writers family diffs
   only the pure body builders). The `5cb0e0190` row's "`wearable_pool.rs`
   `find_by_title` (no move)" is right.
3. **Commit message vs code.** `a05c6f755` says the Courier attributes a
   character-announced outcome to the character; the shipped Courier checks
   `systemSender` FIRST (`[Staff: Pascal]`). Port the code. Its "LLM speaker
   tag" claim holds (announcement attribution names the announcer).
4. **`wearable-ref.ts` doc** claims `findInPool` comes through it; only the
   title half does (via `findByTitle`). Behaviour-neutral either way.
5. **`OutcomeSubjects` key position** ("between `state` and `llm`", ledger) is
   accurate but not observable — the object is never serialized.
6. **v5 writer silence (§6 T14)** — the ledger assumes a log FIELD addition;
   v5 has no such line at all. Ruling owed: port v4's three writer lines in
   this lane, or record a divergence.
7. **Where the lens lives** — a NEW `pascal/bench_wardrobe.rs` (v4's module
   split) vs a section of `pascal/workbench.rs`; recommend the new module
   (mirrors v4, keeps `workbench.rs` the library/destinations reader).
8. **Workbench fixture growth** (§5) — tier-1 family for the matrix + route
   rows for key presence only, or rebuild the pair with wardrobe items.
   Recommend the former.
9. **Help** (`help/custom-tools.md`, `pascals-workbench.md`, `wardrobe.md`)
   and `docs/v4/` spec moves are the vendoring lane's (`p4.9i2` route), not
   these two lanes'.
