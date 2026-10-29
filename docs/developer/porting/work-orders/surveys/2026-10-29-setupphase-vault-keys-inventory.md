# Survey — the vault keys (P4.D349) and the Inventory riders (P4.D350)

Read-only planning survey, **2026-10-29**. v4 = `~/source/quilltap-server`, read
ONLY through `git show 236c04e35:<path>` / `git show <sha> -- <paths>` and the
detached pin worktree `/tmp/qt-v4-pin-setupphase-7789ebed6` (code-identical to
**THE PIN** `236c04e35`, `4.12.0-dev.0`). **The oracle baseline is
`322a5488e`**; v5 = `main` at **`f6cdceedd`** (core 0.0.1379, host 0.0.217, web
0.0.251, cli 0.0.35, SPA 0.5.907). Every v4 line number below is AT THE PIN
(new-side hunk lines where a diff is cited); every v5 line number is at
`f6cdceedd`. Nothing is taken from commit prose — every claim is from a shipped
hunk; where a ledger row disagrees with a hunk this survey says so (§0.3).

## 0. Header

**Commits in scope**

| sha | subject | lane |
|---|---|---|
| `b27e5089a` | Add humanoid flag to physical descriptions for non-humanoid characters (#106) | P4.D349 |
| `eb90d9ed8` | Feature: wardrobe pronoun guidance | P4.D349 |
| `6588972d5` | Feature: diagrams in the Salon (iteration 1) — **SERVER half only** | P4.D349 (client → P4.D353) |
| `265b711dd` | Fix: wardrobe and inventory tools ask for a picture cue | P4.D350 (see §7 for the wardrobe half) |
| `074e1e01a` | Feature: self_inventory core-file checklist and holdings counts | P4.D350 |
| `0276d10d9` | Refactor: Inventory chokepoints (#107) | P4.D350 |

**Lanes informed:** P4.D349, P4.D350; contracts named with P4.D353 (the wire
fields the diagrams SPA reads), P4.D347 (the prompt note's placement),
P4.D341 (the `schema-key-order.json` dump), P4.D343 (the tool catalog regen),
P4.D358 / P4.D359 (SPA file collisions), P4.D360 (help / docs / vendored
schema).

### 0.1 Headline — ten things the planner must not miss

1. **The three LIVE-DATA erasure writers are measured and small.**
   `db/vault_character_write.rs:213-224` (`PhysicalPromptsJson`, five keys) +
   `:252-262` (`render_physical_prompts_json`) erase `"humanoid": false` on
   every v5 physical write; `:199-211` (`PropertiesJson`, six keys) +
   `:231-248` (`render_properties_json`) + the RMW patch writer
   `db/vault_character_update.rs:100-107` (`PROPERTY_KEYS`) / `:170-225` + the
   backfill `db/character_vault.rs:590-598` drop `canDrawDiagrams` /
   `diagramStyle` on every properties write; `vault_overlay.rs:2384-2446`
   (`build_wardrobe_item_file`) rebuilds an item from `WardrobeItem`
   (`:334-375`, no `pronoun_guidance` field) and drops `pronounGuidance` on
   every wardrobe projection. After P4.D349 every one of these PRESERVES the
   keys (§1.1, §1.2, §1.3 give the exact bytes). **The scaffold defaults move
   too** (`character_vault.rs:98` PROPERTIES_JSON_DEFAULT → eight keys;
   `:105-106` PHYSICAL_PROMPTS_JSON_DEFAULT → five keys with
   `"humanoid": true`) — both FEED THE CONTENT-DEDUP SHA, so every freshly
   provisioned vault's bytes change and `characters_scaffold_tier2` /
   `characters_create_tier2` / the provisioning families go red at regen until
   ported.
2. **`humanoid` and `diagramStyle` parse LENIENTLY; `canDrawDiagrams` does
   NOT.** `schema.ts:72-75` preprocesses `humanoid` (`false`/`"false"` →
   false, `true`/`"true"` → true, anything else → absent → hydrates `true`);
   `schema.ts:50-53` preprocesses `diagramStyle` through
   `diagramStyleOrDefault` (`'handDrawn' | 'clean'`, else `'handDrawn'`); but
   `canDrawDiagrams: z.boolean().optional().default(false)` (`schema.ts:45`)
   is as strict as `canChooseOutfit` — a non-boolean FAILS the whole
   `properties.json` parse (the keystone → `VaultUnavailable`). v5's
   `parse_vault_properties` (`vault_overlay.rs:191-195`) already has the strict
   arm shape to copy; `parse_vault_physical_prompts` (`:234-253`) gets the
   lenient one.
3. **The `[FORMATTING: DIAGRAMS]` note lands in `system_prompt.rs`, NOT in
   `build_context.rs`.** v5 pushes the math note at `system_prompt.rs:647` and
   the Taboo section at `:652`; the diagram push goes between them, gated on
   a new `Character.can_draw_diagrams: bool` (`:136-153`, `#[derive(Default)]`).
   The production populator is `services/system_prompt_compiler.rs:58-100`
   (`character_from_value`), not P4.D347's `build_context.rs` — whose two
   literal sites (`:3676`, `:3971`, tests) use `..Default::default()` and
   compile untouched. **One same-file touch is unavoidable:**
   `tools/self_inventory.rs:1322-1345` (`to_sys_char`) builds a full
   `system_prompt::PhysicalDescription` literal, so adding `humanoid` there is
   a one-hunk P4.D349 edit inside P4.D350's file (§7 names the hunk fence).
4. **`inventory-refs.ts:220` GARBLES at the pin.** `0276d10d9` moved the
   unequip kit fallback INTO `pickForGesture` (`refs.ts:206-213`), so the
   committed row `corner-kit-half-equipped-unequip-null` regenerates with the
   KIT as `out` while its id still says `-null`. v5 `inventory/refs.rs:278-289`
   must take the fallback in, and the two inline `.or_else(kit)` callers
   (`tools/inventory_unequip.rs:75`, `pascal/inventory_effects.rs:429-433`)
   must drop theirs.
5. **`tools/definitions/data.rs` is GENERATED** (`:1-8`:
   `harness/oracle/tools/gen-tool-catalog.mjs`; 76 entries; ZERO `effects`
   keys — `b15fe9404`'s per-tool `effects` additions never enter the catalog
   because `tool-definitions.ts:197` dumps `{ name, description, parameters }`
   only). The span changes the DESCRIPTION bytes of `wardrobe_create` /
   `wardrobe_update` (eb90d9ed8 + 265b711dd, the SAME entries),
   `wardrobe_list` / `wardrobe_read` (eb90d9ed8), `inventory_create` /
   `inventory_update` (265b711dd), `self_inventory` (074e1e01a), and —
   outside these lanes — `project-info-tool.ts`, `run-custom-tool.ts`,
   `state-tool.ts`. ONE regen at the pin is the only sane shape (§7.3).
6. **Three SPA halves have NO lane in the table:** b27e5089a's Humanoid
   checkbox (`DescriptionsTab.tsx` → v5 `screens/characters/edit/appearance-tab.ts`),
   eb90d9ed8's `PronounAwareToggle` / `WardrobePronounGuidanceField` / row
   note / both composers' pickers / the chat-start composer / the shared
   `STANDARD_PRONOUNS`, and 0276d10d9's client hooks (`refreshUnlessRealtime`,
   the topic map's `archetypeLists` + `equippedLists`, `isInContainer`). §6
   maps them; §7 proposes where they go and names the P4.D358 collision.
7. **`seatedCharacterIds` drops the `type === 'CHARACTER'` filter the landed
   port carries.** v5 `api/chat_inventory.rs:105-118` (`cast_of`) filters
   `kind == "CHARACTER"`; v4 `equip.ts:192-198` now takes any participant
   with a `characterId` not `removed` — for the chat `?action=inventory` GET,
   reset-defaults, and `inventory_give`'s receiver (`tools/inventory_give.rs:68-71`
   already matches the new rule — v4's `resolveReceiver` had the same
   two-conjunct filter before; only the GET / reset gate changes).
8. **The export key order is a dump the KEYSTONE re-dumps.**
   `services/qtap_export/schema-key-order.json` (kinds `character`, `chat`,
   `chat_message`, `chat_inform`, `memory`, `roleplay_template`, …; produced by
   `harness/oracle/fixtures/dump-export-key-order.ts` from v4's live Zod
   shapes) must gain `canDrawDiagrams`, `diagramStyle` after `canChooseOutfit`
   (`:38`) in the `character` kind — the same file P4.D341 re-dumps for
   `chat` / `memory` / `roleplay_template`. One dump at the pin carries both;
   P4.D349 must NOT dump it separately (§7.2, §9).
9. **v4 applies `CharacterSchema.parse` on EVERY repository read**
   (`base.repository.ts:132`), so `canDrawDiagrams: false` /
   `diagramStyle: 'handDrawn'` appear on every character object before the
   overlay runs, vault or not. v5 materialises the same for `canChooseOutfit`
   at `db/characters_read.rs:236`; both new keys join it there, and the
   overlay (`db/vault_read_overlay.rs:333-348`) overwrites them from
   `properties.json`.
10. **The duplicate action's 404 becomes a Zod 400 with v4's `validationError`
    body** `{"error":"Validation error","details":[…issues]}` (`responses.ts:108-118`):
    a missing `itemId` is `invalid_type` (`Invalid input: expected string,
    received undefined`), an empty one `too_small` with the custom message
    `itemId is required`. v5's hand-read 404 is `api/inventory_route_factory.rs:966-980`;
    the constructors exist (`api/zod_issues.rs:269`, `:360`).

### 0.2 Rulings owed (full list in §10)

R-A the SPA-half lane; R-B the `data.rs` regen owner / stacking; R-C whether
v5 adopts v4's location-level `inventory` publish (a v5-architecture choice);
R-D P4.D349's wave (the key-order dump dependency); R-E the Playwright beats
for the Humanoid checkbox and the pronoun-aware toggle (spend-free).

### 0.3 Ledger corrections (each verified against the pin's bytes)

- `b27e5089a` row: "its two WARN texts change + a new DEBUG" — exact:
  `Could not list chats for subprompt fan-out` `{characterId, subpromptId,
  error}` → `Could not list chats for identity-stack fan-out` `{characterId,
  reason, error}`; `Failed to recompile a seat after a subprompt change`
  `{chatId, participantId, characterId, subpromptId, error}` → `Failed to
  recompile a seat in an identity-stack fan-out` `{chatId, participantId,
  characterId, reason, error}`; NEW DEBUG `Identity-stack fan-out complete`
  `{characterId, reason, chatsTouched, seatsRecompiled}`; the service logger
  moves from `Subprompts:Fanout` to `SystemPromptCompiler:CharacterFanout`
  for those three; the INFO `Subprompt change fanned out` `{characterId,
  subpromptId, removeSelection, chatsTouched, seatsRecompiled}` stays under
  `Subprompts:Fanout` with the same keys (`chat-fanout.ts:55-62`). The row's
  "the subprompt fan-out folded into `character-fanout.ts`" is right.
- `b27e5089a` row says the scaffold writes `humanoid`: ✓ — but note the v4
  scaffold's `PHYSICAL_PROMPTS_JSON` (`character-scaffold.ts:66-72`) has NO
  `headAndShoulders` key (four tiers + `humanoid`), matching v5's four-key
  default at `character_vault.rs:105-106`, which becomes FIVE keys ending in
  `"humanoid": true`.
- `6588972d5` row: "the message is false: `properties.json` gains both keys
  on every write" ✓ (the full writer `managed-fields.ts:231-235`, the RMW
  `:430-466`, the scaffold `:46-50`).
- `0276d10d9` row (1): "a missing `itemId` is a Zod 400 where it was a 404"
  ✓ (`inventory-route-factory.ts:400-409`). Row (3) ✓ (§0.1 #7). Row (5): the
  `[InventoryEquip] Refused` `op` key is gained by EXACTLY two arms — the
  EQUIP cap breach (`op: 'equip'`, `equip.ts:1278-1286` via `fitToCaps`) and
  pick-up's INVALID_PARENT (`op: 'pick-up'`, `:1475-1483`); NOT_A_PARTICIPANT,
  OFF_ROSTER and NOT_EQUIPPABLE keep their old keys; pick-up's cap arm already
  had `op`. The arrival WARN gains `op: 'equip'` FIRST ✓. Row (6): v5's
  `inventory_update` tool does the SECOND `load_inventory_context`
  (`tools/inventory_update.rs:214-240`); v4 now resolves through
  `ctx.serverPool.resolve({ ...updated, origin, mountPointId })` — v5 carries
  `server_pool` on `LoadedInventoryContext` (`inventory/context.rs:43`), so
  the port is a local change.
- `0276d10d9` row (7) "the pool's `Elsewhere` external-parent origin
  DELETED" — the ORIGIN tag is deleted (`pool.ts:92-96`, `:169-183`:
  `externalParents: ReadonlyMap<string, InventoryRecord>`), the lookup
  itself stays. v5 `inventory/pool.rs:287-315` tags with the `Elsewhere`
  origin; nothing in the harness or oracle pins the name (grep: only the
  unrelated "Continue Elsewhere"), so dropping the tag is neutral.
- `074e1e01a` row ✓ in every byte checked; one addition: `SelfInventoryVaultIncludedParts`
  is SPLIT (`self-inventory-tool.ts:149-158`) — `vaultAccess.includedParts`
  keeps `{character, groups}`, `vault.includedParts` becomes `{character,
  groups, coreFiles}`; v5 shares ONE `VaultIncludedParts` for both
  (`tools/self_inventory.rs:2081-2098`), so the JSON output of every `vault`
  row gains a key.
- `265b711dd` row ✓; note the `wardrobe_create` DESCRIPTION's text also
  changes (`wardrobe-create-tool.ts:184-190`), not only the params.

## 1. Per commit, the shipped hunks

### 1.1 `b27e5089a` — the `humanoid` flag (43 files; prod 27)

**Vault / schema**

- `lib/database/repositories/vault-overlay/schema.ts:55-67`:
  `CharacterVaultPhysicalPromptsSchema` gains
  `humanoid: z.preprocess((v) => (v === false || v === 'false' ? false : v === true || v === 'true' ? true : undefined), z.boolean().optional())`
  — `null`, `0`, `"yes"`, `{}` all read as ABSENT; the four tiers stay
  `z.string().nullable()`.
- `lib/mount-index/character-vault.ts:229-240` `renderPhysicalPromptsJson`:
  a SIXTH key after `complete`: `humanoid: primary?.humanoid ?? true` —
  ALWAYS written (`JSON.stringify(…, null, 2)`), so the file's bytes are
  `{\n  "headAndShoulders": …,\n  "short": …,\n  "medium": …,\n  "long": …,\n  "complete": …,\n  "humanoid": true\n}`.
- `lib/mount-index/character-scaffold.ts:66-72` `PHYSICAL_PROMPTS_JSON`:
  `{ short: null, medium: null, long: null, complete: null, humanoid: true }`.
- `lib/database/repositories/vault-overlay/read-overlay.ts:258-267`
  (`hydrateOne`): `patched.humanoid = parsedPrompts.humanoid ?? true` —
  APPENDED after `updatedAt` on every vault character whose
  `physical-prompts.json` parses (the key order the wire and the export
  carry).
- `managed-fields.ts:151-158` (`readCharacterVaultManagedFields`, the
  snapshot reader) sets the same; `:486-505` (`applyDocumentStoreWriteOverlay`,
  case `physicalDescription`): `let humanoid = incoming.humanoid; if
  (humanoid === undefined) { current = await readCharacterVaultPhysicalPrompts(mountPointId, characterId); humanoid = current?.humanoid; logger.debug(…) }`
  then `renderPhysicalPromptsJson({ ...incoming, humanoid })` — a write that
  omits the flag keeps the vault's value (absent stays absent → written as
  `true` by the `?? true`); an explicit value is written as given.
- `lib/schemas/character.types.ts:79-85` `PhysicalDescriptionSchema.humanoid: z.boolean().optional()`.
- `lib/services/character-field-semantics.ts:168-169` the
  `'physical-prompts.json'` summary becomes
  `Image-generation prompts for your appearance as JSON: headAndShoulders, short, medium, long, complete, plus a \`humanoid\` flag (false when you have no head, shoulders or body to draw). Terse noun phrases, no clothing.`
  (model-visible through `self_inventory` `vault.coreFiles` — 074e1e01a's
  NEW constant, so P4.D350 lands this byte).
- `public/schemas/qtap-export.schema.json:439-441` the `physicalDescription`
  description grows (vendor — P4.D360).

**The pure helpers**

- `lib/characters/physical-description.ts:19-24, 49-67, 75-86, 90-104`:
  `PhysicalDescriptionVariants` adds `'humanoid'`; `VariantKey` excludes it;
  NEW `NON_HUMANOID_HEAD_AND_SHOULDERS_ORDER = ['mediumPrompt','longPrompt','shortPrompt','completePrompt','fullDescription']`
  (NO head-and-shoulders); `isHumanoid(desc) = desc?.humanoid !== false`;
  `NON_HUMANOID_IMAGE_DIRECTION = 'Not humanoid: no head, face, neck, shoulders, torso, arms, legs or any human or humanoid body; depict its form exactly as described and do not anthropomorphize it'`;
  `pickPhysicalDescription` uses the non-humanoid order ONLY for profile
  `'head-and-shoulders'` on a non-humanoid.
- `lib/characters/pronoun-gender.ts:53-73`: NEW `FigureGenderSource { pronouns?, physicalDescription?: Pick<…,'humanoid'> }`,
  `figureGender` / `figureGenderNoun` / `figureGenderPrefix` = the pronoun
  helper when humanoid, else `null` / `null` / `''`.

**The image side (every site, strings verbatim)**

- `lib/image-gen/avatar-prompt.ts:71-125` `buildFigureIdentityBlock`:
  returns `{ subjectNoun, humanoid, physicalText, physBlock }`;
  `NON_HUMANOID_SUBJECT_NOUN = 'non-humanoid entity'`; `subjectNoun = humanoid ? figureGenderNoun(character) ?? 'person' : NON_HUMANOID_SUBJECT_NOUN`;
  `physBlock = humanoid ? physSentence : [physSentence, \`${NON_HUMANOID_IMAGE_DIRECTION}.\`].filter(Boolean).join(' ')`
  (so a non-humanoid's `physBlock` is never `''`).
- `:181-268` `buildCharacterAvatarPrompt`: `leafCounts` filled BEFORE the
  branch; non-humanoid branch lists every slot's leaves title-only, omitting
  EMPTY slots, `outfitText = ''` when nothing is on (never `describeOutfit`'s
  naked fallback); `hasAppearance = Boolean(physicalText) || Boolean(outfitText) || !humanoid`;
  `intro = !humanoid ? \`${opening}, its whole form in frame.\` : topIsBare ? … : …`;
  `outro = humanoid ? 'Character portrait, detailed, high quality, natural lighting. Only one person in the image.' : 'Character portrait, detailed, high quality, natural lighting. Only one subject in the image.'`.
- `lib/wardrobe/item-image-prompt.ts:100-121` (the `worn` branch):
  `framing = !figure.humanoid ? 'Its whole form in frame, the item clearly visible on it' : hairOnly ? … : …`;
  `wearing = !figure.humanoid ? \`Bearing ${cueInline}\` : …`;
  `rest = !figure.humanoid ? 'The item is the subject; neutral studio backdrop; even light. Only one subject in the image.' : …`.
- `lib/image-gen/appearance-resolution.ts:59-67` `ResolvedCharacterAppearance.humanoid?: boolean`
  (ABSENT when humanoid; `false` only for a non-humanoid) set at THREE build
  sites (`:198`, `:232`, `:328`: `...(isHumanoid(x) ? {} : { humanoid: false })`);
  the LLM input gains `...(isHumanoid(char.physicalDescription) ? {} : { nonHumanoid: true })`
  (`:358`); `sceneDescriptionParts` (`:586-612`): non-humanoid →
  `[physicalDescription.trim().replace(/[.!?]+$/, ''), NON_HUMANOID_IMAGE_DIRECTION, …tail].filter(Boolean)`
  (NO gender prefix), humanoid → `[genderPrefix + physicalDescription, …tail]`
  where the tail is the `Wearing: …` / capitalised carrying fragments.
- `lib/image-gen/prompt-expansion.ts:284-290, 302-365` `buildExpansionContext`:
  per placeholder `figure = { pronouns, physicalDescription: resolved ? (resolved.humanoid === false ? {humanoid:false} : null) : placeholder.descriptions?.[0] }`,
  `nonHumanoid = figure.physicalDescription?.humanoid === false ? true : undefined`,
  `gender = figureGender(figure) ?? undefined`, and `...(nonHumanoid ? { nonHumanoid } : {})`
  on BOTH placeholder shapes (after `gender`, before `usageContext`).
- `lib/memory/cheap-llm-tasks/image-scene-tasks.ts`: the crafter's system
  prompt gains, after the `[man] or [woman]` rule (`:88`),
  `- If an entity is marked [not humanoid], it has NO head, face, neck, shoulders, torso, arms, legs or any human or humanoid body. Describe only the form its description gives, never call it a man, woman or person, and never imply a body part it lacks`;
  the story-background prompt gains after its gender-term rule (`:256`)
  `- If a character description says it is "Not humanoid", it has no head, face, shoulders or human body; describe only the form given and never imply a body part it lacks`;
  `craftImagePrompt` (`:519-530`): `genderHint = p.nonHumanoid ? ' [not humanoid]' : p.gender === 'male' ? ' [man]' : …`
  and a `  Form: ${NON_HUMANOID_IMAGE_DIRECTION}` line pushed FIRST (before
  the `  Gender:` line, which cannot co-occur);
  `updateSceneState` (`:727-736`): the baseline line becomes
  `  - Appearance: ${char.physicalDescription}${form}` with
  `form = char.nonHumanoid ? '\n  - Form: NOT HUMANOID — no head, face, shoulders, torso or limbs; never describe a body part it lacks' : ''`;
  `resolveAppearance` (`:947-953`):
  `  Character: ${name} (ID: ${id})${form}` with
  `form = char.nonHumanoid ? ' [not humanoid]\n  Form: NOT HUMANOID — no head, face, neck, shoulders, torso or limbs. Describe anything it wears or bears without naming a body part it lacks.' : ''`.
  `types.ts:113-120, 180-186, 255-262`: `nonHumanoid?: boolean` on the three
  input shapes.
- `lib/background-jobs/handlers/story-background.ts:65-75`
  `buildBasicEnumeration` now renders through `sceneDescriptionParts({physicalDescription: pick||name, clothingDescription: '', humanoid: isHumanoid(primary)}, figureGenderPrefix(char)).join('. ').trim()`;
  `:483-486` `genderPrefix = figureGenderPrefix(char!)`; the no-resolved
  fallback (`:506-517`) renders through `sceneDescriptionParts` the same way
  (a humanoid's bytes are unchanged: `genderPrefix + desc`).
- `lib/background-jobs/handlers/scene-state-tracking.ts:212-216`: the
  tracker's character gets `...(isHumanoid(pd) ? {} : { nonHumanoid: true })`.
- `lib/background-jobs/handlers/character-headshoulders-backfill.ts:59-69`:
  `if (!isHumanoid(pd)) { logger.debug('[HeadShouldersBackfill] Character is not humanoid, skipping', { context, jobId, characterId }); return; }`
  — placed AFTER the `!pd` return, BEFORE the idempotency check.
  `lib/startup/enqueue-headshoulders-backfill.ts:70-73`: the scan's
  `skipped++` condition becomes `!isHumanoid(pd) || (filled head prompt)`.
- `lib/services/character-optimizer.service.ts:528-531`: the physical
  prompt's rules append, after the headAndShouldersPrompt rule,
  `\n- THIS CHARACTER IS NOT HUMANOID: it has no head, face, neck, shoulders, torso or limbs. Never give any sub-field a body part it lacks, and do not suggest a headAndShouldersPrompt at all — it is unused for a non-humanoid character.`
  for a non-humanoid only.

**The identity stack + the PUT + the fan-out**

- `lib/chat/context/system-prompt-builder.ts:246-262` (`buildIdentityStack`):
  `formNote = isHumanoid(desc) ? '' : '\nYou have no humanoid body: no head, face, shoulders, hands or limbs. Never describe yourself as having or using one.'`;
  with text: `\n## Physical Appearance\nThis is how you look — "${desc.name}"${contextNote}: ${descText}${formNote}`;
  with NO text but the note: `\n## Physical Appearance${formNote}` (a
  humanoid with no text still gets no section). `IDENTITY_STACK_BUILDER_VERSION`
  is NOT bumped.
- `app/api/v1/characters/[id]/handlers/put.ts:93-101` the physical
  sub-schema gains `humanoid: z.boolean().optional()`; `:190-197` the
  normalised physical object spreads `...(rawPhysical.humanoid !== undefined ? { humanoid: rawPhysical.humanoid } : {})`
  (between `fullDescription` and `createdAt`); `:203-212` reads the vault's
  current flag BEFORE the update (`readCharacterVaultPhysicalPrompts`, only
  when `incomingHumanoid !== undefined` and the character has a vault);
  `:226-234` after the update, on a change: `recompileCharacterSeats(id, { reason: 'humanoid flag changed' })`
  + DEBUG `[Characters v1] Humanoid flag changed; identity stacks recompiled`
  `{characterId, humanoid, ...fanout}` (i.e. `chatsTouched`, `seatsRecompiled`).
- NEW `lib/services/system-prompt-compiler/character-fanout.ts` (104 lines):
  `recompileCharacterSeats(characterId, { reason, seatFilter?, prepareSeat? })`
  — candidates are LLM-controlled, non-removed seats of the character
  (`controlledBy !== 'user' && status !== 'removed'`), `prepareSeat` may write
  the seat first (returns the updated chat), `compileIdentityStackForParticipant(current, seat.id)`,
  per-seat try/catch (WARN above), `publishRealtime('chats', chat.id)` per
  touched chat, the DEBUG at the end; `lib/subprompts/chat-fanout.ts:42-62`
  is now a thin caller (`seatFilter` = the selected-subprompt test,
  `prepareSeat` = the `updateParticipant` strip when `removeSelection`).

### 1.2 `eb90d9ed8` — wardrobe pronoun guidance (39 files; prod 29)

- NEW `lib/characters/pronoun-presets.ts`: `STANDARD_PRONOUNS` (He/Him/His,
  She/Her/Her, They/Them/Their, It/It/Its — picker order, `as const`),
  `STANDARD_PRONOUN_SUBJECTS = ['he','she','they','it']`,
  `standardPronounLabel(subject)`, `standardPronounSubjectOf(pronouns)`
  (trim + lowercase ALL THREE fields and match a WHOLE preset; a partial
  match is `null` = custom).
- NEW `lib/wardrobe/pronoun-guidance.ts`: `normalizePronounGuidance(raw)`
  (array or a single string; each string `trim().toLowerCase().split('/')[0].trim()`
  — so `'He/Him/His'` → `he`; known subjects only; de-duplicated; PICKER
  ORDER regardless of input order; anything else `[]`);
  `hasNoPronounGuidance`; `fitsPronounGuidance(item, pronouns)` (no guidance
  → true; non-standard pronouns → true; else `includes(subject)`);
  `filterByPronounGuidance(items, pronouns, keepIds?)` → `{ items, hiddenCount }`
  (a non-standard character: `{ items: [...items], hiddenCount: 0 }` with no
  per-item test); `formatPronounGuidance` → labels joined `', '`.
- `lib/schemas/wardrobe.types.ts:125-134`: `PronounGuidanceEnum = z.enum(STANDARD_PRONOUN_SUBJECTS)`,
  `PronounGuidanceListSchema = z.array(PronounGuidanceEnum)`; `:185-186`
  `wardrobeItemFieldsSchema.pronounGuidance: PronounGuidanceListSchema.optional()`
  (REST create / update — an unknown value is a Zod 400 `invalid_value`, NOT
  normalised); `:296-305` `WardrobeItemSchema.pronounGuidance` optional.
- `vault-overlay/parsers.ts:386-387, 452` (`parseWardrobeItemFile`):
  `pronounGuidance = normalizePronounGuidance(parsed.data?.pronounGuidance)`,
  spread ONLY when non-empty (`...(pronounGuidance.length > 0 ? { pronounGuidance } : {})`),
  positioned between `replace` and `componentItemIds` in the literal.
- `lib/mount-index/character-vault.ts:354-357` (`buildWardrobeItemFile`):
  `if (normalized.length > 0) data.pronounGuidance = normalized` — a YAML
  LIST after `default` / `replace`, BEFORE `archived` / `archivedAt`
  (e.g. `pronounGuidance:\n  - she\n  - they`, in v4's `serializeFrontmatter`
  dialect — v5's Decision-A emitter `FmVal::Seq`).
- `lib/wardrobe/create-body.ts:56` `wardrobeItemFromCreateBody`:
  `pronounGuidance: normalizePronounGuidance(body.pronounGuidance)` (ALWAYS
  present on a created item, `[]` when none) — the REST create answers the
  key; `item-draft.ts` (client draft) carries it.
- Tools: NEW `lib/tools/wardrobe-pronoun-params.ts` — `pronounGuidanceToolParam('set'|'replace')`
  and `pronounAwareToolParam`; the exact description bytes are in the
  snapshot hunk (§1.2a). `wardrobe-create-tool.ts:68` adds
  `pronoun_guidance` after `appropriateness`; `wardrobe-update-tool.ts:58`
  the same (after `appropriateness`, before `types`), its DESCRIPTION gains
  `pronoun_guidance, ` between `appropriateness, ` and `coverage types`;
  `wardrobe-list-tool.ts:40` adds `pronoun_aware` after `include_equipped`,
  its description gains `Pronoun-aware by default: items whose pronoun guidance names other pronouns than yours are left out (the result says how many) — pass pronoun_aware: false to see them. `
  between `appropriateness context. ` and `Each item includes`;
  `WardrobeListItemResult.pronoun_guidance: string[]` (after
  `appropriateness`), `WardrobeListToolOutput.hidden_by_pronoun_guidance?: number`
  (after `total_count`); `wardrobe-read-tool.ts:52-53`
  `WardrobeReadToolOutput.pronoun_guidance: string[]` after `appropriateness`.
- `handlers/wardrobe-list-handler.ts:74-75, 105-114, 143, 181-182, 193, 212-233, 278-310`:
  `pronounAware = pronoun_aware !== false`; AFTER the type / appropriateness
  filters and BEFORE the ledger read: `pronouns = await readCharacterPronouns(repos, context.characterId)`
  (a failed `findById` → WARN `Wardrobe list could not read the character's pronouns; listing unfiltered`
  `{context: 'wardrobe-list-handler', characterId, error}` → `null` →
  unfiltered), `wornIds = allEquippedItemIds(equippedSlots)`,
  `filterByPronounGuidance(filteredItems, pronouns, wornIds)`; the INFO
  `Wardrobe list completed` gains `pronounAware`, `hiddenByPronounGuidance`
  AFTER `includeEquipped` and BEFORE `compositeCount`; the output gains
  `hidden_by_pronoun_guidance`; each item `pronoun_guidance: item.pronounGuidance ?? []`.
  Formatter: `hiddenNote = hidden > 0 ? \`(${hidden} more item${hidden !== 1 ? 's are' : ' is'} meant for characters with other pronouns; call wardrobe_list with pronoun_aware: false to see them.)\` : ''`;
  the empty case `Wardrobe: No items found matching the specified filters.\n${hiddenNote}`
  when hidden; per row ` [meant for: ${formatPronounGuidance(…)}]` directly
  AFTER the appropriateness tag and BEFORE the composite tag; the note is the
  LAST line.
- `handlers/wardrobe-read-handler.ts:69, 216, 293-295`: `pronoun_guidance`
  in the output (and `[]` on failure); the formatter line
  `  pronoun guidance: meant for ${labels}` AFTER `  appropriateness:` and
  BEFORE `  description:`, only when non-empty.
- `handlers/wardrobe-create-handler.ts:175, 232`: `pronounGuidance: pronoun_guidance`
  into the create body; `wardrobe-update-handler.ts:79, 106`:
  `if (pronoun_guidance !== undefined) patch.pronounGuidance = normalizePronounGuidance(pronoun_guidance)`.
- `lib/wardrobe/apply-outfit-selections.ts:357-366` (Let character choose):
  `offered = filterByPronounGuidance(pool.wearable(), character?.pronouns)`;
  `wardrobeItems = offered.items`; DEBUG `[applyOutfitSelections] Pronoun guidance held items back from the green room`
  `{chatId, characterId, hiddenCount}` when `hiddenCount > 0`.
- `public/schemas/qtap-export.schema.json:1520-1524` `pronounGuidance`
  (array of the enum; "Unknown values are dropped on import" — see §1.2b).
- Client (NO lane — §6): `components/wardrobe/PronounAwareToggle.tsx`,
  `wardrobe-item-editor/WardrobePronounGuidanceField.tsx`,
  `lib/hooks/use-pronoun-guidance-filter.ts`, `WardrobeBrowser.tsx`,
  `equipped-slot-row.tsx`, `outfit-composer.tsx`, `outfit-selector.tsx`,
  `wardrobe-control-dialog.tsx`, `wardrobe-item-editor.tsx`,
  `wardrobe-item-row.tsx`, `CharacterBasicInfo.tsx` (the picker spreads
  `STANDARD_PRONOUNS`).

**1.2a — the three param descriptions (byte-exact, from the snapshot):**

- `pronoun_guidance` (create): `Optional pronoun guidance: the pronouns (by subject: "he", "she", "they", "it") of the characters this item is meant for. Pronoun-aware wardrobe listings offer it only to characters with one of these pronouns; anyone can still wear it when they ask for it by name. Omit (or pass an empty array) to offer it to everyone.` — JSON Schema `{"type":"array","items":{"type":"string","enum":["he","she","they","it"]}}`.
- `pronoun_guidance` (update): `Replace the pronoun guidance: the pronouns (by subject: "he", "she", "they", "it") of the characters this item is meant for. Pronoun-aware wardrobe listings offer it only to characters with one of these pronouns; anyone can still wear it when they ask for it by name. An empty array offers it to everyone. Omit to leave unchanged.`
- `pronoun_aware` (list): `Whether to honour each item's pronoun guidance, listing only items meant for characters with your pronouns (plus items with no guidance, and anything you are wearing now). Defaults to true. Pass false to see the whole wardrobe — for instance when the scene calls for something you would not ordinarily be offered.` — `{"type":"boolean"}`.

**1.2b — the `.qtap` carriers at the pin.** `lib/import/quilltap-import/import-characters.ts:232-260`
(`importCharacterWardrobeItems`) builds the stored item from the exported
record; the pin's item builder is the one v5 ports at
`services/quilltap_import/characters.rs:540-570` — it carries `title /
description / imagePrompt / types / componentItemIds / appropriateness /
isDefault / replace / archivedAt / folder` and does NOT read `pronounGuidance`
(the key is `WardrobeItemSchema`-optional, so an exported item's guidance is
preserved only where the importer copies the record whole). **Measure at
port time**: whether `importCharacterWardrobeItems` spreads the record
(`{ ...item, id, characterId }`) or builds field-by-field — the first 40
lines after `:262` decide whether the schema's "dropped on import" claim is
true. The exporter (`lib/export/ndjson-writer.ts:231-262`) spreads the
overlaid item, so `pronounGuidance` IS exported when present.

### 1.3 `6588972d5` SERVER half — `canDrawDiagrams` / `diagramStyle` (46 prod files; server 13)

- NEW `lib/markdown/diagrams.ts` (client-safe, pure): `DIAGRAM_LANGUAGES`
  (`mermaid→mermaid, dot→graphviz, graphviz→graphviz, svg→svg`),
  `DIAGRAM_STYLES = ['handDrawn','clean']`, `DEFAULT_DIAGRAM_STYLE = 'handDrawn'`,
  `diagramStyleOrDefault(v) = v === 'handDrawn' || v === 'clean' ? v : 'handDrawn'`,
  `diagramEngineFor`, `hasDiagramFence` (the last two are the client's —
  P4.D353). The SERVER needs exactly the first four names.
- `vault-overlay/schema.ts:43-53`: `canDrawDiagrams: z.boolean().optional().default(false)`
  (STRICT), `diagramStyle: z.preprocess(diagramStyleOrDefault, z.enum(DIAGRAM_STYLES).optional().default('handDrawn'))`
  (LENIENT); `:237-238` `MANAGED_FIELDS` gains both.
- `managed-fields.ts:93-97` the snapshot reader copies both; `:225-236` the
  FULL writer's `properties.json` literal gains, after `canChooseOutfit`,
  `canDrawDiagrams: character.canDrawDiagrams ?? false, diagramStyle: diagramStyleOrDefault(character.diagramStyle)`
  (EIGHT keys, always); `:423-427` `propsKeys` → the eight; `:448-456` the
  fresh-vault seed gains the two; `:462-466` the `next` object:
  `canDrawDiagrams: 'canDrawDiagrams' in patch ? (patch.canDrawDiagrams ?? false) : current.canDrawDiagrams`,
  `diagramStyle: 'diagramStyle' in patch ? diagramStyleOrDefault(patch.diagramStyle) : current.diagramStyle`;
  `:467-473` DEBUG `[VaultOverlay] Writing diagram settings to properties.json`
  `{characterId, canDrawDiagrams, diagramStyle}` when `touched` includes
  either key.
- `read-overlay.ts:176-179` hydration copies `parsed.canDrawDiagrams`,
  `parsed.diagramStyle` (keys already exist on the Zod-parsed row, so their
  POSITION is the schema's: after `canChooseOutfit`, before `systemTransparency`).
- `character-scaffold.ts:46-50` `PROPERTIES_JSON` gains `canDrawDiagrams: false, diagramStyle: 'handDrawn'`
  → the scaffold file is
  `{\n  "pronouns": null,\n  "aliases": [],\n  "title": "",\n  "firstMessage": "",\n  "talkativeness": 0.5,\n  "canChooseOutfit": false,\n  "canDrawDiagrams": false,\n  "diagramStyle": "handDrawn"\n}`.
- `lib/schemas/character.types.ts:251-266`: `CharacterSchema.canDrawDiagrams: z.boolean().default(false)`,
  `diagramStyle: z.enum(DIAGRAM_STYLES).default('handDrawn')` — declared
  right after `canChooseOutfit` (export key order; every repository read).
- `app/api/v1/characters/[id]/handlers/put.ts:72-77`: `canDrawDiagrams: z.boolean().optional()`,
  `diagramStyle: z.enum(DIAGRAM_STYLES).optional()` (a bad style is a Zod 400
  `invalid_value`, message `Invalid option: expected one of "handDrawn"|"clean"`).
- `app/api/v1/characters/handlers/get.ts:79-82` (the LIST DTO):
  `canDrawDiagrams: character.canDrawDiagrams ?? false, diagramStyle: diagramStyleOrDefault(character.diagramStyle)`
  right after `canChooseOutfit`, before `defaultConnectionProfileId`.
- `app/api/v1/chats/[id]/helpers.ts:64-70` `getEnrichedCharacter`:
  `diagramStyle: diagramStyleOrDefault(charData.diagramStyle)` AFTER
  `archivedAt` (the chat participants' `character`).
- `lib/services/chat-enrichment.service.ts:113-118, 341-346, 372-375`
  `EnrichedCharacterDetail.diagramStyle: DiagramStyle` after `archivedAt`, on
  BOTH `getCharacterDetail` arms (the avatar-override early return and the
  main return).
- `lib/chat/context/system-prompt-builder.ts:33-46, 446-453`:
  `DIAGRAM_FORMATTING_INSTRUCTION` = the two lines
  ```
  [FORMATTING: DIAGRAMS]
  You can draw diagrams. Write one in a fenced code block and it renders as a picture: ```mermaid for flowcharts, sequences, timelines, mind maps and the like; ```dot for graphs of connections (Graphviz DOT); ```svg for freeform drawings. Put only the diagram source inside the fence.
  ```
  pushed in `buildSystemPrompt` DIRECTLY after `parts.push(MATH_FORMATTING_INSTRUCTION)`
  and before the Taboo section, ONLY `if (character.canDrawDiagrams === true)`
  — block 1, template-free, never in the identity stack; otherwise byte-identical.
- `public/schemas/qtap-export.schema.json:487-496` both keys (vendor).
- NOT updated by v4: `CORE_VAULT_FILE_SUMMARIES['properties.json']` (still
  names six fields) — port the pin's bytes as they are.

### 1.4 `265b711dd` — picture cues (16 files; prod 6)

- NEW `lib/tools/image-cue-guidance.ts`: `IMAGE_CUE_TARGET_CHARS = 200`
  (stated, never enforced); the four glosses (the resulting description bytes
  are in the snapshot hunk and reproduced in §2 under `data.rs`).
- `wardrobe-create-tool.ts:38-45, 184-190`: `description` → `Prose for you and the other characters to read. It is NEVER used for pictures — put what the item looks like in image_prompt as well.`;
  `image_prompt` → `Portrait Cue — ALWAYS supply this for a new garment. It is the ONLY text about the item that avatars, story backgrounds and item pictures ever see (the description is never shown to them; without a cue they get only the title). 200 characters or fewer of literal, comma-separated visual detail — colour, material, cut, notable features — as a painter would need it. No story, mood, history, feelings, names or Markdown. E.g. "ankle-length emerald silk gown, square neckline, gold beaded sash". It is joined into a comma-separated outfit list. May be omitted for a composite, whose components speak for it.`;
  the tool DESCRIPTION's clause `Optionally set image_prompt (a Portrait Cue) to steer image generation; if the operator allows it, a picture `
  becomes `For a single garment ALWAYS set image_prompt (a Portrait Cue of 200 characters or fewer, literal visual detail only): pictures are drawn from it, never from the description. If the operator allows it, a picture `.
- `wardrobe-update-tool.ts:46-52`: `description` → `New description. Omit to leave unchanged. Prose for you and the other characters to read. It is NEVER used for pictures — put what the item looks like in image_prompt as well.`;
  `image_prompt` → `New Portrait Cue — the only text about the item the image generator sees (never the description). 200 characters or fewer of literal, comma-separated visual detail — colour, material, cut, notable features — as a painter would need it. No story, mood, history, feelings, names or Markdown. Supply one whenever the item has none or its look changes. Omit to leave unchanged.`
- `inventory-create-tool.ts:23-26, 60-66`: `image_description` (shared by
  `inventory_update` through `inventoryObjectFieldsShape`) →
  `How the object looks, for pictures — the ONLY text about it the image generator sees (never the description). Supply it whenever the object would be visible in a portrait or scene. 200 characters or fewer of literal, comma-separated visual detail — colour, material, cut, notable features — as a painter would need it. No story, mood, history, feelings, names or Markdown. E.g. "dented brass pocket compass on a short chain".`;
  the `inventory_create` DESCRIPTION → `Make a new object of your own (not clothing) in your own vault, and optionally equip it. Give it a plain name, and optionally a title, a description, an emoji and free-form metadata. Set image_description (how it looks, 200 characters or fewer of literal visual detail) for anything that could appear in a picture: pictures never read the description.`
- `legacy/text-block-parser.ts:251` `CONTENT_PARAM_MAP` gains
  `wardrobe_create: 'description'` (BEHAVIOUR: a `[[CREATE_WARDROBE_ITEM …]]body[[/…]]`
  body used to land in `content`, which the tool drops).
- `legacy/text-block-prompt.ts:209-217, 271-274`: the Create Wardrobe Item
  block becomes
  ```
  ### Create Wardrobe Item
  Design and add a new clothing item to your wardrobe, or gift one to another character. You can also build composite outfits that bundle existing items.
  For every single garment ALWAYS give a cue: 200 characters or fewer of literal visual detail (colour, material, cut). Pictures are drawn from the cue, never from the description text between the tags.
  Single garment: [[CREATE_WARDROBE_ITEM title="Red Scarf" types="accessories" appropriateness="casual" cue="long crimson wool scarf, golden tassels"]]A soft crimson scarf with golden tassels[[/CREATE_WARDROBE_ITEM]]
  Composite outfit (bundles existing items by id or title): [[CREATE_WARDROBE_ITEM title="Rain Outfit" component_titles="Yellow Slicker,Dark Jeans,Wellington Boots"]]Practical attire for a downpour[[/CREATE_WARDROBE_ITEM]]
  Gift to another character: [[CREATE_WARDROBE_ITEM title="Red Scarf" types="accessories" cue="long crimson wool scarf, golden tassels" recipient="CharacterName"]]A gift for you[[/CREATE_WARDROBE_ITEM]]
  ```
  and the Create an Object block
  ```
  ### Create an Object
  Give anything that could appear in a picture a looks: 200 characters or fewer of literal visual detail. Pictures are drawn from it, never from the description text between the tags.
  [[CREATE_INVENTORY_ITEM name="compass" title="The Navigator" looks="dented brass pocket compass on a short chain" equip="true"]]A brass compass that points toward home[[/CREATE_INVENTORY_ITEM]]
  ```
  (`cue` → `image_prompt` and `looks` → `image_description` are EXISTING
  alias rows; v5 has `("wardrobe_create","cue") => "image_prompt"` at
  `tools/text_block_parser.rs:205`).
- `help/wardrobe.md`, `help/inventory.md` (vendor).

### 1.5 `074e1e01a` — `self_inventory` core files + holdings (16 files; prod 6)

- `lib/tools/self-inventory-tool.ts:16-42` `SELF_INVENTORY_SECTIONS` gains
  `'vault.coreFiles'` (after `'vault.groups'`) and `'holdings'`,
  `'holdings.wardrobe'`, `'holdings.inventory'`, `'holdings.customTools'` (at
  the end); `VAULT_SUB_SECTIONS` gains `vault.coreFiles`; NEW
  `HOLDINGS_SUB_SECTIONS`; the `sections` enum description and the
  `sections` array description change (bytes in the snapshot hunk — the
  enum text gains `, plus the core-file checklist` / `, and "vault.coreFiles" (the nine core files of your vault — path, whether each exists, and what belongs in it)` / `; "holdings" (counts of what is within your reach: wardrobe garments and outfits, Inventory objects, and custom tools), with dotted variants "holdings.wardrobe", "holdings.inventory", and "holdings.customTools"`;
  the array text lists eleven sections and adds `"holdings.*"`); the tool
  DESCRIPTION: `Ten top-level sections` → `Eleven top-level sections`, the
  vault clause grows (`…includeAutomaticImages is true; plus a checklist of the nine core files of your vault) — request just "vault.character", "vault.groups", or "vault.coreFiles" (each core file's path, whether it exists, and the short version of what belongs in it) for one part; `),
  a new `holdings` sentence before the `Pass a "sections" array` sentence,
  and `all ten` → `all eleven`.
- Types `:149-158` (the includedParts split, §0.3), `:195-217`
  `SelfInventoryCoreFile { path, exists, fileSizeBytes, uri, holds }`,
  `SelfInventoryVaultCoreFilesSection` (`{ available: true, mountPointName, files }`
  | `{ available: false, reason: 'no_vault' | 'mount_index_degraded' | 'error', message }`),
  `:513-575` `SelfInventoryTierCounts { character, group, project, general }`,
  `SelfInventoryHoldingsWardrobe` (`{ available: true, garments, outfits, byTier }`),
  `…Inventory` (`{ available: true, objects, kits, archetypes, carriedHere, byTier }`),
  `…CustomTools` (`{ available: true, count, names, byTier }`), each
  `| { available: false, message }`; `SelfInventoryHoldingsSection { includedParts, wardrobe?, inventory?, customTools? }`;
  `SelfInventoryToolOutput.holdings?` after `context`.
- `handlers/self-inventory/builders.ts:80-139` `buildVaultCoreFilesSection(character)`:
  degraded → `{ available: false, reason: 'mount_index_degraded', message: 'Mount index database is in degraded mode.' }`;
  no `characterDocumentMountPointId` → `no_vault`, `Character has no vault mount point.`;
  mount row missing → `no_vault`, `` `Mount point ${mountPointId} not found.` ``;
  `rows = docMountFiles.findByMountPointId(mountPointId)`, lower-cased path
  map; for each of the NINE `SINGLE_FILE_OVERLAY_PATHS` in order:
  `{ path: row?.relativePath ?? corePath, exists: Boolean(row), fileSizeBytes: row ? row.fileSizeBytes : null, uri: formatDocStoreUri({ mountPointName, mountPointId, path: foundPath }), holds: CORE_VAULT_FILE_SUMMARIES[corePath] }`;
  DEBUG `self_inventory: core files resolved` `{context: 'self-inventory-handler', characterId, present, total}`;
  `buildVaultWrapper` (`:927-933`) attaches `coreFiles` with the
  `{ available: false, reason: 'error', message }` catch; `resolveVaultIncludedParts`
  (`:1145-1151`) resolves `coreFiles` for bare `vault`.
- NEW `handlers/self-inventory/holdings.ts`: `buildHoldingsWardrobe` —
  `loadWearablePool(repos, characterId, undefined, { chatId })`, `wearable()`,
  `outfits = count(isComposite)`, `garments = wearable.length - outfits`,
  `byTier = countByTier(item.origin.scope)`; `buildHoldingsInventory` —
  `loadInventoryContext(repos, characterId, chatId)`, `live = pool.items.filter(!archivedAt)`,
  `archetypes` / `kits` (`kind === 'kit'`) / `objects`, `carriedHere = equippedIds.size`,
  `byTier` over `item.origin?.scope` (no origin → `character`);
  `buildHoldingsCustomTools` — `chatSettings.findByUserId(userId)`;
  `customTools === false` → `{ available: false, message: 'Custom tools are switched off for this instance.' }`
  WITHOUT resolving a roster; else `projectId = context.projectId ?? chat.projectId ?? null`,
  `resolveCustomToolRoster({ userId, chatId, characterId, characterMountPointId, projectId, metadata })`,
  `names` sorted `localeCompare`, `byTier` via `tierScopeOf` (`global → general`,
  `participant → character`, else the tier); each part `.catch(unavailable)`;
  DEBUG `self_inventory: holdings resolved` `{context, characterId, wardrobe, inventory, customTools}`
  (each `available ?? null`).
- `handlers/self-inventory/formatters.ts:53-72, 414-461, 506-508`:
  ```
  ## Core Vault Files
  <mountPointName>: <present> of <files.length> present
  - <path> [exists, <formatBytes(size)> | exists, empty | MISSING] — <holds>
      uri: <uri>
  (To fill in a missing or empty file, write it with doc_write_file at its uri.)
  ```
  (the `Unavailable — <message>` form when not available), appended as the
  vault section's THIRD part; `## Holdings` with `### Wardrobe`
  (`- Garments: N`, `- Outfits: N`, `- By tier: your vault N, groups N, project N, Quilltap General N` — zero tiers omitted, all zero → `none`),
  `### Inventory` (`- Objects`, `- Kits`, `- Archetypes`, `- Carrying in this chat`, `- By tier`),
  `### Custom Tools` (`- Available: N`, then when `> 0` `- By tier: …` and
  `- Names (run with run_custom): a, b`); each part `### X\nUnavailable — <message>`
  when not available; the holdings block is the LAST section of the report
  (after `context`), the parts joined with a blank line.
- `lib/services/character-field-semantics.ts:143-170` NEW
  `CORE_VAULT_FILE_SUMMARIES` — nine second-person strings keyed by the
  overlay paths (copy verbatim from the pin; `physical-prompts.json`'s is
  b27e5089a's final text, §1.1).
- `help/character-system-transparency.md`, `help/inventory.md`,
  `docs/FEATURES.md` (vendor).

### 1.6 `0276d10d9` — the Inventory chokepoints refactor (55 files; prod 42) — NOT neutral

Hunk by hunk against what v5 landed on 2026-10-27 (P4.D324–P4.D330):

| v4 file (pin) | what the hunk DOES | v5 today |
|---|---|---|
| `lib/schemas/inventory.types.ts:252-255` | NEW `duplicateInventorySchema = z.object({ itemId: z.string().min(1, 'itemId is required') })` | no twin; `inventory_types_equivalence` has no row |
| `lib/inventory/routes/inventory-route-factory.ts:400-409` | `handleDuplicate` parses the body with it BEFORE resolving → a missing / empty / non-string `itemId` is a **400 `{"error":"Validation error","details":[…]}`**, no longer `notFound('Inventory object')` | `api/inventory_route_factory.rs:966-980` hand-reads → 404 (P4.D325 R-4, now wrong) |
| `routes/responses.ts:44-59` | NEW `withInventoryErrors(handler)` — try/catch the WHOLE handler, refusals → `inventoryErrorResponse`, else rethrow | v5 maps refusals per mutation (`Fail::Refused`, `:682-710`); the only refusal-throwing step OUTSIDE v5's mapped region is `resolve_location` → measure: v4's `resolveInventoryLocation` throws plain `Error` (`location.ts:134`, `has no linked vault`) and `CharacterArchivedError` (`:91`), neither an `InventoryRefusal`; `findHome` (images route) answers responses, never throws refusals; so the WIDENING is observable only through `loadPoolForLocation` / `convertToWebP`-adjacent refusals that were already inside the old try blocks → **effectively neutral**, pin NEGATIVE |
| `inventory-route-factory.ts:184-199, 202-226, 229-262, 264-282, 400-417, 426-438, 457-470, 480-487, 505-519, 549-569` | every try/catch removed, each handler wrapped at the dispatch table | as above |
| `app/api/v1/inventory/[itemId]/images/route.ts:53-54, 142-166, 211-219` | `containerQuerySchema.scope` = `InventoryScopeSchema` (same four values); `upload` / `set-current` / `delete-image` wrapped | `api/inventory_item_images.rs` — neutral |
| `app/api/v1/inventory/transfers/route.ts:16-30` | wrapped whole | `api/inventory.rs:51-106` — neutral |
| `app/api/v1/chats/[id]/actions/inventory.ts:17-30, 36-39, 56-68, 72-77` | `castOf` DELETED → `seatedCharacterIds(chat)` (NO `type` filter); `OPERATOR = inventoryChangeContext(userId, 'operator')`; `handleInventoryEquip` wrapped; `loadEquipChat` | `api/chat_inventory.rs:105-118` keeps `kind == "CHARACTER"` → **drop it** (the GET at `:139`, the reset gate at `:306`) |
| `lib/inventory/equip.ts:93-98` | NEW `loadEquipChat(repos, chatId)`; `:101-103` `freshChat` through it; `:105-112` NEW `seatedCharacterIds` (`characterId && status !== 'removed'`, unique, participant order); `:115-118` `refuse(error, meta)` → INFO `[InventoryEquip] Refused` `{code, ...meta, context}` + throw; `:120-124` `assertParticipant` via `seatedCharacterIds` (keys unchanged) | `inventory/equip.rs:304-320` `assert_participant` — same rule already (R-18) |
| `equip.ts:131-141` | NEW `writeCarriedSet(repos, chatId, characterId, ids)` = `setEquippedInventory` + `publishRealtime('chats', chatId)` — the ONE carried-set writer | v5 writes + publishes inline at `equip.rs:767-769`, `:884-889`, `:1061-1068` (same bytes, three copies) |
| `equip.ts:160-180` (`fitToCaps`) | ONE cap check for equip + pick-up; the equip breach INFO gains `op: 'equip'` (`{code, op, chatId, characterId, itemId, cap, context}`); the arrival WARN `[InventoryEquip] Arrival dropped an object over its cap` becomes `{op: 'equip', chatId, characterId, itemId, cap, context}` (`op` FIRST) | `equip.rs:685` WARN, `:701` INFO — keys lack `op` |
| `equip.ts:190-195` (`holderVault`) | the kit text unchanged (`Character ${id} has no vault to hold a kit's instances`); the pick-up text becomes `Character ${id} has no vault to hold a picked-up object` (was `…to pick an object up into`); both plain `Error`s (500) | v5 `equip.rs` carries neither sentence verbatim (grep `has no vault` → only `inventory_give.rs:213`) — **measure the pick-up arm's current text** (P4.D324 pinned the old one per the ledger) |
| `equip.ts:226-233` (`EquipOptions.pool`) | arrival passes its already-loaded pool into `equipItemsHoldingLock` | `apply_defaults.rs` — check whether `equip_items_holding_lock` takes a pool |
| `equip.ts:265-275` | `dropCarriedEverywhere` publishes `chats` for EVERY chat it changed | `equip.rs:451` — measure the publish per chat |
| `equip.ts:285-290, 301-306, 430-436, 496-502` | `equipItemsHoldingLock` / `equipItems` / `unequipItems` / `pickUpItems` are `async` (NEUTRAL) | — |
| `equip.ts:475-483` | pick-up's INVALID_PARENT INFO gains `op: 'pick-up'` FIRST after `code` | `equip.rs:985` — add `op` |
| `equip.ts:515-516` | pick-up's `publishRealtime('inventory')` after the instantiations REMOVED (the location now publishes) | `equip.rs:1055` publishes |
| `lib/inventory/apply-defaults.ts:49-55` (`equipDefaults`) | reads the pool ONCE (no `archivedAt` filter in the ids — `pool.resolved()` already excludes archived), passes `{ lenient: true, pool }`, and WRITES THE EMPTY ENTRY ITSELF (`writeCarriedSet(…, [])`) when nothing was carried; arrival's `setEquippedInventory(chatId, characterId, ids)` → `writeCarriedSet` (publishes `chats`); reset's `null` clear → `writeCarriedSet(null)` | `inventory/apply_defaults.rs:160, 177, 386, 390` write with NO publish |
| `lib/inventory/equip-effects.ts:48-61` | NEW `inventoryChangeContext(userId, provenance, { announce?, pendingInventoryAnnouncements? })` — the ONE context builder; `flushPendingInventoryAnnouncements` uses it | `tools/inventory_tool_table.rs:238-243` builds the struct inline; `pascal/inventory_effects.rs` likewise |
| `lib/inventory/errors.ts:39-45` | `InventoryNotAParticipantError` MOVED here (same code / text / 400) | `inventory/errors.rs:364-371` already |
| `lib/inventory/equipped-set.ts:61-73` | NEW `loadEquippedSet(repos, chat, characterId)` — the readers' door; empty set → NO pool read | `inventory/equipped_set.rs` has `equipped_ids_of` / `read_equipped_set`; the shortcut is duplicated in `prompt_section.rs:80` / `:131` and `image_description.rs:84+` |
| `lib/inventory/image-description.ts:34-41, 48-56` | `describeEquippedInventoryForImages` DELETED; `loadCarryingPhrase` through `loadEquippedSet` | `inventory/image_description.rs:62-74` keeps the twin (`inventory-image-description.test.ts:36` imports `renderCarryingPhrase` only → NOT import-dead) |
| `lib/inventory/prompt-section.ts:56-63` | through `loadEquippedSet`; the DEBUG `[InventorySection] Built the carried-objects section` unchanged | `prompt_section.rs:73-100` |
| `lib/inventory/pool.ts:89-96, 169-183` | `externalParents` lose the `Elsewhere` ORIGIN (plain records); `kinOf` DELETED (`:344-347`); the roster-gate header comment | `inventory/pool.rs:287-315` tags; `:627` `kin_of` — delete (dead) |
| `lib/inventory/refs.ts:27-30, 149-213` | `INVENTORY_STATES` is the ONE spelling (`as const`; `custom-tool.types.ts:523` and `inventory-subjects.ts:33` read it); `pickForGesture(…, 'unequip')` = `matches.find(equipped) ?? matches.find(kind === 'kit') ?? null` | `inventory/refs.rs:278-289` lacks the kit arm (§0.1 #4) |
| `lib/inventory/item-mutations.ts:86-104` (`lintedWrite`), `:189-199, 252-261, 325-332, 346-353, 365-372, 574, 611` | one lint-then-write helper (NEUTRAL); `trimFolderPath` (NEUTRAL) | — |
| `item-mutations.ts:421-434, 453-481` (`dependentLocation`, `detachAll`) | every dependent's location resolved BEFORE any write; an unlinked vault → WARN `[InventoryMutations] Skipped instances whose vault is no longer linked` `{op: 'detach-all', mountPointId, itemId, ownerId, count, context: 'inventory'}` and the group is skipped; writes go through `location.readItems(true)` / `location.updateMany(patches)` (which now PUBLISH); the per-mount DEBUG `Baked instances in one mount` keeps `scope: location.scope` | `inventory/item_mutations.rs:946-1052`: resolves per group IN the loop and `continue`s SILENTLY on `None` (`:1009-1011`) — add the WARN and the two-phase resolve |
| `lib/inventory/location.ts:30-35, 103, 137-152, 169-191` | `ensureInventoryFolder` un-exported; `write(fn, landed)` wrapper: EVERY landed write through a location (`create`, `updateMany` when non-empty, `delete` when non-empty, `createFolder`, `renameFolder`, `deleteFolder`) → `publishRealtime('inventory')`; `update` rides `updateMany` | `inventory/location.rs:184-310` — no publish (v5's tools carry `publishes` lists drained after commit, `inventory_tool_table.rs:94-96, 234-236`; the REST factory publishes nothing — measured `grep publish_realtime api/inventory*.rs` → none) → **operator REST writes never publish in v5 today** (a measured gap v4's change closes) |
| `lib/inventory/dependents.ts:44, 102-150, 156-178, 206-216` | `readAllInventoryRecords` un-exported; `mountOwnerResolver()` memoises owners per operation; `mountScopeOf` / `isReachableFromKitTier` take a resolver | NEUTRAL (fewer reads) |
| `lib/inventory/transfers.ts:84-90` | one resolver per transfer (NEUTRAL) | — |
| `lib/inventory/container.ts:73-81` | NEW client-safe `isInContainer(item, container)` | SPA (`inventory-vantage-items.ts`) |
| `vault-overlay/inventory-readers.ts:167-183, 185-192, 194-247` | `InventoryFolderScan { records, duplicatePaths, ownedPaths }`; `scanInventoryFolder` EXPORTED; `readDuplicateInventoryPaths` DELETED | `db/inventory_readers.rs:332-346` has `records` + `duplicate_paths`; `:436` `read_duplicate_inventory_paths` exists |
| `vault-overlay/inventory-writes.ts:105-139, 142-160, 167-197, 214-243, 248-268, 275-298` | `projectMountInventory(mount, records, scan?)` sweeps ONLY `ownedPaths` (`ownsFile: (doc) => ownedPaths.has(doc.relativePath)` — a NOTE or a BROKEN file is no longer swept even when `isInventoryItemDocument` says yes); one scan per write (create / update / delete / folder ops via a `WeakMap` of scans) | `db/inventory_writes.rs:148-195` `owns_file = !duplicates.contains(path) && is_inventory_item_document(path, content)` → **observable**: a `.json` under `Inventory/` that parses as an object but is not a record is swept by v5 and kept by v4 (tier-2 DB diff) |
| `vault-overlay/item-folder-ops.ts:73-81, 379-386` | `ItemFolderTree.refile(mount, patches, current)` takes the list `read` returned | `db/item_folder_ops.rs` twin — signature only |
| `lib/realtime/topic-map.ts:35-40, 134-146`, `lib/schemas/realtime.types.ts:44-52` | the `inventory` topic refreshes `lists, folderLists, archetypeLists, equippedLists`; the reconnect sweep adds the last two | SPA `core/realtime-topic-map.ts:151-159` (R-2: `[]`); the dialog subscribes directly (`inventory-control-dialog.ts:347-350`: reloads `data` + `folders` only — NOT the archetype pickers nor the Carrying-now pane) |
| `lib/query/keys.ts:358-367` | `inventory.item`, `dependentLists`, `dependents` keys DELETED | SPA (no TanStack keys in v5) |
| `lib/hooks/use-inventory.ts:191-201, 213-226, 299-302` | `refreshUnlessRealtime` — own-write refresh only while the socket is down; `useInventoryDependents` DELETED | SPA `inventory-control-dialog.ts:refresh()` always reloads |
| `lib/tools/handlers/inventory-*.ts` | every handler-owned `publishRealtime('inventory' / 'chats')` REMOVED (`archive`, `create`, `give`, `update`); `loadToolChat` DELETED → `loadEquipChat`; NEW `loadOwnRecord(repos, context, itemId)` (`inventory-handler-shared.ts:102-112`); `characterChangeContext` through `inventoryChangeContext`; `inventory_give`'s receiver via `seatedCharacterIds(chat)`; `inventory_update`'s `after` via `ctx.serverPool.resolve({ ...updated, origin: vault.origin, mountPointId: vault.mountPointId })` | `tools/inventory_{archive,create,give,update}.rs` carry `publishes` (`:139`, `:192/212`, `:267`, `:252`); `inventory_update.rs:214-240` second load |
| `lib/pascal/inventory-effects.ts:99-103, 118, 171-176` | `loadEquipChat`; `inventoryChangeContext(userId, 'pascal', params)`; `pickForGesture(matches, gesture.action)` (the kit fallback moved) | `pascal/inventory_effects.rs:429-433` inline fallback |
| `lib/pascal/custom-tool.types.ts:523`, `inventory-subjects.ts:33` | the enum reads `INVENTORY_STATES` (NEUTRAL) | — |
| `help/inventory.md:21` | one new paragraph (vendor) | — |

## 2. The v5 surface map

### 2.1 P4.D349 (`b27e5089a` + `eb90d9ed8` + `6588972d5` server)

| v4 (pin) | v5 counterpart (`crates/quilltap-core/src/…`) | v5 today |
|---|---|---|
| `vault-overlay/schema.ts` `CharacterVaultPhysicalPromptsSchema.humanoid` | `vault_overlay.rs:116-124` (struct) + `:234-253` (`parse_vault_physical_prompts`) | five fields; a `humanoid` key is IGNORED on read |
| `schema.ts` `CharacterVaultPropertiesSchema.canDrawDiagrams` / `diagramStyle` | `vault_overlay.rs:102-111` + `:159-204` (`parse_vault_properties`; the `canChooseOutfit` arm `:191-195` is the strict template) | six fields |
| `schema.ts` `MANAGED_FIELDS` | `db/characters.rs:105-123` | lacks both |
| `character-vault.ts` `renderPhysicalPromptsJson` | `db/vault_character_write.rs:213-224` (`PhysicalPromptsJson`), `:252-262`; `PhysicalDescriptionWrite` `:74-87` (no `humanoid`) | FIVE keys — the erasure |
| `managed-fields.ts` the full `properties.json` writer | `vault_character_write.rs:199-211` (`PropertiesJson`), `:231-248` (`render_properties_json`), `:286-298` (the write) | SIX keys — the erasure |
| `managed-fields.ts` the RMW `properties-json` case | `db/vault_character_update.rs:100-107` (`PROPERTY_KEYS`), `:170-225` (the `next` bag, `render_properties_json` call `:212-219`) | six keys; no diagram DEBUG |
| `managed-fields.ts` the `physicalDescription` case (keep the vault's `humanoid`) | `vault_character_update.rs:243-262` | writes `render_physical_prompts_json(Some(&phys))` with no read-back; NEW read of the current file + DEBUG needed (a small reader — v5 has NO `readCharacterVaultPhysicalPrompts` twin; the overlay reads through `VaultFileMaps`) |
| `character-scaffold.ts` `PROPERTIES_JSON` / `PHYSICAL_PROMPTS_JSON` | `db/character_vault.rs:98`, `:105-106` (+ the typed mirrors in tests `:685-720`) | six / four keys |
| `character-vault.ts` backfill (`backfillCharacterVaultManagedFiles`) | `db/character_vault.rs:590-598` (props), `:614-624` (physical) | through the two renderers |
| `read-overlay.ts` `hydrateOne` | `db/vault_read_overlay.rs:333-348` (properties → add the two keys to the loop), `:375-397` (physical → insert `humanoid` AFTER the five tiers; v4 appends after `updatedAt`, and `patched` here already carries `createdAt`/`updatedAt`, so `insert` lands it last) | — |
| `read-overlay.ts` `default_physical` | `vault_read_overlay.rs:255-274` | no `humanoid` (v4's minted base has none either; the `?? true` appends it) |
| `characters.repository` Zod defaults on every read | `db/characters_read.rs:222-236` (`canChooseOutfit: false` at `:236`) | add `canDrawDiagrams: false`, `diagramStyle: "handDrawn"` beside it |
| `put.ts` `updateCharacterSchema` | `api/characters.rs:2211-2254` `UPDATE_SCHEMA_KEYS` (add both after `canChooseOutfit` `:2239`); the physical sub-object is NOT re-validated in v5 (grep `longPrompt` in `api/characters.rs` → none) — the `humanoid` boolean check and the diagram enum check are NEW arms; the recompile-on-flip is a NEW step after the update (the fan-out seam) | — |
| `get.ts` list DTO | `services/character_enrichment.rs:126-131` | `canChooseOutfit` only |
| `chats/[id]/helpers.ts` `getEnrichedCharacter` | `services/chat_participants.rs:452-473` (`EnrichedParticipantCharacter`, `archived_at` last) | add `diagram_style` after |
| `chat-enrichment.service.ts` `getCharacterDetail` | `services/chat_enrichment.rs:273-291` (struct, `archived_at` last), `:353+` (both arms) | add `diagram_style: String` |
| `lib/markdown/diagrams.ts` (server names) | NEW `markdown/diagrams.rs` or a `pub mod` in `characters/` — `DIAGRAM_STYLES`, `DEFAULT_DIAGRAM_STYLE`, `diagram_style_or_default(&Value) -> &'static str` | NO-COUNTERPART |
| `system-prompt-builder.ts` `DIAGRAM_FORMATTING_INSTRUCTION` + the push | `system_prompt.rs:521-528` (the const's neighbour), `:644-655` (the push site), `:136-153` (`Character.can_draw_diagrams`), `services/system_prompt_compiler.rs:58-100` (populate) | — |
| `system-prompt-builder.ts` the `formNote` | `system_prompt.rs:107-115` (`PhysicalDescription.humanoid: Option<bool>`), `:375-395` (the section); populators `system_prompt_compiler.rs:71-79`, `tools/self_inventory.rs:1337-1345` (P4.D350's file — §7) | — |
| `physical-description.ts` `isHumanoid`, the non-humanoid order, the DIRECTION | `characters/physical_description.rs` (`PhysicalDescriptionVariants:150-170` gains `humanoid: Option<bool>`, `from_value:172-185`; `order():108-150`; `pick_physical_description:194-210` takes the profile-specific branch) | — |
| `pronoun-gender.ts` `figure*` | `pronoun_gender.rs:1-37` (subject-only API) — add `figure_gender(subject, humanoid)` twins or take a `(Option<&str>, bool)` | — |
| `avatar-prompt.ts` | `services/avatar_prompt.rs:70-248` (the branch at `:107-150`, `has_appearance:190`, intro/outro `:193-204`), `:279-335` (`FigureIdentityBlock` gains `humanoid: bool`; `NON_HUMANOID_SUBJECT_NOUN`) | — |
| `item-image-prompt.ts` | `services/wardrobe_item_image_prompt.rs:165-196` | — |
| `appearance-resolution.ts` | `services/appearance_resolution.rs:40-66` (`PhysicalDescription` gains `humanoid`), `:87-109` (`ResolvedCharacterAppearance.humanoid: bool`, absent = true), the three build sites (`:238`, `:277`, `:435` regions), `:470-490` (`non_humanoid` on `CharacterAppearanceInput`), `:782-797` (`scene_description_parts`) | — |
| `prompt-expansion.ts` | `tools/generate_image.rs:1068-1110` (`build_expansion_context`), `services/image_scene_tasks.rs:45-54` (`ExpansionPlaceholder.non_humanoid`) | — |
| `image-scene-tasks.ts` (texts + 3 formatters) | `services/image_scene_tasks/prompt_text.rs` — GENERATED by `harness/oracle/cases/gen-image-scene-prompts.mjs` (re-run at the pin); `image_scene_tasks.rs:203-230` (placeholder block), `:279-287` (`CharacterAppearanceInput.non_humanoid`), `:395-430` (`resolve_appearance` block); `services/scene_state_tracking/mod.rs:57-63` (`CharacterBaseline.non_humanoid`), `:120-135` (the baseline line) | — |
| `story-background.ts` | `services/story_background_job.rs:628-680` (the two descriptions), `:1541-1555` (`build_basic_enumeration`), `:1566` (`physical_description_from_value` gains `humanoid`) | — |
| `scene-state-tracking.ts:212-216` | the tracker's baseline builder in `services/scene_state_tracking/` (the caller that fills `CharacterBaseline`) | — |
| `character-headshoulders-backfill.ts`, `enqueue-headshoulders-backfill.ts` | `services/headshoulders_backfill_job.rs:196-212` (insert the skip between the `!pd` return and `head_prompt_is_filled`), `services/headshoulders_backfill_enqueue.rs:147-157` | — |
| `character-optimizer.service.ts:528-531` | `generators/optimizer.rs:636` (the physical rules string) | — |
| `character-fanout.ts` + `chat-fanout.ts` | `subprompts/fanout.rs:107-213` (generalise into `recompile_character_seats(main, mount, character_id, options, seams)` with `seat_filter` / `prepare_seat` closures; `LOG_TARGET` `:33` → the three lines move to a new target) — the P4.D349 lane owns this file; `api/subprompts.rs:426, 500` stay callers of the thin twin | — |
| `pronoun-presets.ts`, `pronoun-guidance.ts` | NEW `characters/pronoun_presets.rs`, `wardrobe/pronoun_guidance.rs` (pure; a tier-1 family each) | NO-COUNTERPART |
| `wardrobe.types.ts` `PronounGuidanceEnum`, `WardrobeItemSchema.pronounGuidance` | `vault_overlay.rs:334-375` (`WardrobeItem.pronoun_guidance: Vec<String>`? — NB v4's `.optional()` with the parser's conditional spread means a READ item carries the key only when non-empty; `from_read_value:398-420`; the serialized shape `:330-333` doc) | — |
| `parsers.ts` `parseWardrobeItemFile` | `vault_overlay.rs:1108-1137` (`WardrobeItemFromFile`), `:1159-1260` (parse; insert after `replace` `:1209`) | — |
| `character-vault.ts` `buildWardrobeItemFile` | `vault_overlay.rs:2384-2446` (push after `replace` `:2419-2421`, before `archived` `:2422`) | the erasure |
| `wardrobeItemFieldsSchema` (REST) | `api/wardrobe_route_factory.rs:440-500` (`ItemFields` parse — add the enum array arm → `Err(())` → the existing Zod 400 path), `:340-356` / `:360-376` (the two serialisers — add `pronounGuidance` where v4's object carries it) | — |
| `create-body.ts` | `wardrobe/create_body.rs` (or wherever `WardrobeCreateBody` lives — `tools/wardrobe_create.rs:402-414` builds one) | — |
| `wardrobe-*-tool.ts` descriptions | `tools/definitions/data.rs:341-357` (GENERATED — §7.3) | pre-span bytes |
| `wardrobe-list-handler.ts` | `tools/wardrobe_list.rs:26-70` (structs), `:143-173` (validate), `:178-335` (`run` — the filter goes after `:223` and before the ledger read `:226-233`; the INFO `:299-324`), `:400-495` (formatter) | — |
| `wardrobe-read-handler.ts` | `tools/wardrobe_read.rs:26-60` (struct), `:290-300` (failure), `:350-360` (output), `:535-545` (formatter) | — |
| `wardrobe-create/update-handler.ts` | `tools/wardrobe_create.rs:118-250` (validate), `:400-414` (body); `tools/wardrobe_update.rs:100-140` (validate), the patch builder | — |
| `apply-outfit-selections.ts` | `services/outfit_selections.rs:909-1000` (the `llm_choose` attempt — the wearable list fed to the model) | — |
| `.qtap` carriers | `services/quilltap_import/characters.rs:540-570` (item build — add `pronoun_guidance` through the normaliser, per §1.2b's measurement); the export spreads the read item (`qtap_export/records.rs`) so the key rides when the overlay emits it | — |
| `qtap-export.schema.json` | `generators/qtap-export.schema.json:483` (vendor — P4.D360) | — |
| `schema-key-order.json` | `services/qtap_export/schema-key-order.json:38` (P4.D341's dump — §7.2) | — |

### 2.2 P4.D350 (`265b711dd` + `074e1e01a` + `0276d10d9`)

| v4 (pin) | v5 counterpart | v5 today |
|---|---|---|
| `image-cue-guidance.ts` + the four tool files | `tools/definitions/data.rs:196-202` (`inventory_create` / `inventory_update`), `:351-357` (`wardrobe_create` / `wardrobe_update`) — GENERATED | pre-span bytes |
| `text-block-parser.ts` `CONTENT_PARAM_MAP` | `tools/text_block_parser.rs` (`content_param` — grep `inventory_create` at `:170` is the alias table; the content map is the fn that answers `description` for `inventory_create`) | `wardrobe_create` falls through to `content` |
| `text-block-prompt.ts` | `tools/text_block_prompt.rs:156-160`, `:205-206` | pre-span bytes |
| `self-inventory-tool.ts` | `tools/definitions/data.rs:311-312` (GENERATED); `tools/self_inventory.rs:616-645` (`SelfInventoryOutput` + `holdings`), `:2070-2135` (section resolvers — split `VaultIncludedParts`), `:2440-2510` (the vault formatter — add core files), `:3110-3165` (the report tail) | ten sections |
| `self-inventory/builders.ts` `buildVaultCoreFilesSection` | NEW in `tools/self_inventory.rs` beside `build_vault_character_section`; helpers: `db/vault_read_overlay.rs:46` (`SINGLE_FILE_OVERLAY_PATHS`), `doc_edit/qtap_uri.rs:417` (`format_doc_store_uri(name, id, path, name_is_ambiguous, None, None)` — v4 passes no ambiguity flag here: `{ mountPointName, mountPointId, path }` → the authority rule decides from the name alone), `format_bytes.rs:16`, the `doc_mount_files` reader | — |
| `self-inventory/holdings.ts` | NEW `tools/self_inventory_holdings.rs` (or a `holdings` region) over `wearable_pool::load_wearable_pool(main, mount, character_id, None, LoadWearablePoolOptions { chat_id: Some(chat_id), .. })` (`:367-395`), `inventory::context::load_inventory_context` (`:111`; `LoadedInventoryContext.server_pool` / `.equipped_ids` `:40-45`), `pascal::roster::resolve_custom_tool_roster(&RosterContext, main, mount, roster_now_ms)` (`:1010-1035` — v5 takes a clock; use the tool's `now_ms`), the chat-settings `customTools` read | — |
| `character-field-semantics.ts` `CORE_VAULT_FILE_SUMMARIES` | NEW const in `generators/field_semantics.rs` (keyed by the nine paths; a compile-time length assert against `SINGLE_FILE_OVERLAY_PATHS`) | NO-COUNTERPART |
| `inventory.types.ts` `duplicateInventorySchema` | `api/inventory_route_factory.rs:966-980` + `api/zod_issues.rs` (`invalid_type("string", ["itemId"], None)`, `too_small_string_message(1, ["itemId"], "itemId is required")`) → `validation_error(&issues)` (`:175`) | 404 |
| `equip.ts` `seatedCharacterIds` | `api/chat_inventory.rs:105-118` (`cast_of` → drop the `kind` conjunct; rename), `:139`, `:306` | `CHARACTER`-only |
| `equip.ts` `refuse` / `fitToCaps` `op` keys | `inventory/equip.rs:685` (WARN), `:701` (INFO equip cap), `:985` (INFO pick-up invalid parent) | no `op` |
| `equip.ts` `holderVault` text | `inventory/equip.rs` pick-up vault arm (`:1020-1030` region) | measure |
| `equip.ts` `writeCarriedSet` publishes | `inventory/equip.rs:767-769`, `:884-889`, `:1061-1068` already; `inventory/apply_defaults.rs:160, 177, 386, 390` DO NOT publish → add `publish_realtime(Chats, Some(chat_id))` after each (or route through one `write_carried_set`) | — |
| `equip.ts` `dropCarriedEverywhere` per-chat publish | `inventory/equip.rs:451+` | measure |
| `location.ts` write → `publishRealtime('inventory')` | `inventory/location.rs:184-310`; v5's handler `publishes` lists (`tools/inventory_{archive:139,148, create:192,212, give:267, update:252}.rs`, drained at `inventory_tool_table.rs:234-236`) and the REST factory (NO publish today) | R-C |
| `equip-effects.ts` `inventoryChangeContext` | `inventory/equip_effects.rs` (NEW fn); callers `inventory_tool_table.rs:238-243`, `pascal/inventory_effects.rs`, `api/chat_inventory.rs` (`OPERATOR`) | inline structs |
| `equipped-set.ts` `loadEquippedSet` | `inventory/equipped_set.rs` (NEW fn); callers `prompt_section.rs:73-100, 131`, `image_description.rs:84-110` | duplicated shortcut |
| `image-description.ts` delete | `inventory/image_description.rs:62-74` `describe_equipped_inventory_for_images` → DELETE (dead after the above) | — |
| `pool.ts` `Elsewhere` tag + `kinOf` | `inventory/pool.rs:287-315` (drop the tag; keep the lookup), `:627` `kin_of` → DELETE | — |
| `refs.ts` `pickForGesture` | `inventory/refs.rs:278-289`; callers `tools/inventory_unequip.rs:75`, `pascal/inventory_effects.rs:429-433` | kit arm outside |
| `item-mutations.ts` `detachAll` | `inventory/item_mutations.rs:946-1052` (two-phase; WARN; `location.read_items` / `update_many`) | silent `continue` |
| `inventory-readers.ts` `ownedPaths` / `inventory-writes.ts` sweep | `db/inventory_readers.rs:332-346, 346-422, 436` (`InventoryFolderScan` gains `owned_paths`; `read_duplicate_inventory_paths` → delete), `db/inventory_writes.rs:148-195` (`owns_file` → `owned_paths.contains`), `:201-243`, `:243-326`, `:326-410` (one scan per write) | sweeps by `is_inventory_item_document` |
| `item-folder-ops.ts` `refile(…, current)` | `db/item_folder_ops.rs` (the wardrobe tree ignores it) | — |
| `inventory-handler-shared.ts` `loadOwnRecord` | `tools/inventory_shared.rs` (NEW fn); callers archive / give / update | inline pairs |
| `inventory-update-handler.ts` resolve-in-hand | `tools/inventory_update.rs:214-240` → `cx` context's `server_pool.resolve(…)` | second load |
| `inventory-give-handler.ts` `seatedCharacterIds` | `tools/inventory_give.rs:61-80` — already the two-conjunct rule | — |
| `realtime/topic-map.ts`, `use-inventory.ts` | SPA `core/realtime-topic-map.ts:151-159` (R-2 stands; the dialog's own subscription), `inventory/inventory-control-dialog.ts:340-355` (reload the archetype pickers + `chat.reload()` on `inventory`; `refresh()` only while disconnected), `inventory/inventory-vantage-items.ts` (`isInContainer`) | partial |
| `help/inventory.md`, `help/wardrobe.md`, `help/character-system-transparency.md`, `docs/FEATURES.md`, `docs/developer/API.md`, `docs/developer/features/complete/inventory.md` | `help/*.md` (repo root), `docs/v4/` — P4.D360 | — |

### 2.3 NO-COUNTERPART / NO-PORT candidates

- `lib/hooks/use-pronoun-guidance-filter.ts`, `use-inventory.ts`'s
  `useInventoryDependents` deletion, `lib/query/keys.ts` — React/TanStack
  mechanics; v5's signal-held stores (P4.D330 R-1 / R-2) have no twin; the
  BEHAVIOUR (what reloads on which topic; own-write refresh only while
  disconnected) is the port.
- `__tests__/unit/lib/inventory/chokepoints.test.ts` et al. — test-only;
  their `it` names are corpus seeds (§5).
- `.claude/commands/update-documentation.md`, `README.md`, `package.json`
  stamps, `packages/theme-storybook/*` — NO-PORT (infra / vendor).

## 3. Schema / D23 shapes

**None of the six commits carries a SQL migration or touches `generateDDL`.**
Every key is a vault / JSON key (the ledger §1 "Not D23" list: `humanoid`,
`canDrawDiagrams`, `diagramStyle`, `pronounGuidance`). `docs/developer/DDL.md:349-352`
(the vault-overlay table) changes prose only. No `fresh_schema.json` move, no
boot ensure, no stamping question. The two D23-CLASS DUMPS that DO move are
artifacts, not SQL: `schema-key-order.json` (`character` gains two keys —
P4.D341's dump) and the GENERATED `tools/definitions/data.rs` /
`image_scene_tasks/prompt_text.rs` (§7.3).

## 4. Log lines (v4 text + keys + level → v5 home)

| commit | level | text | keys | v5 home |
|---|---|---|---|---|
| b27e5089a | DEBUG | `[HeadShouldersBackfill] Character is not humanoid, skipping` | `context: 'background-jobs.headshoulders-backfill'`, `jobId`, `characterId` | `services/headshoulders_backfill_job.rs:196-212` NEW (between the `!pd` return and the filled check) |
| b27e5089a | DEBUG | `[VaultOverlay] Physical write carries no humanoid flag; keeping the vault's` | `characterId`, `mountPointId`, `humanoid` (the kept value `?? true`) | `db/vault_character_update.rs:243-262` NEW |
| b27e5089a | DEBUG | `[Characters v1] Humanoid flag changed; identity stacks recompiled` | `characterId`, `humanoid`, `chatsTouched`, `seatsRecompiled` | `api/characters.rs` PUT, after the update NEW |
| b27e5089a | WARN | `Could not list chats for identity-stack fan-out` (logger `SystemPromptCompiler:CharacterFanout`) | `characterId`, `reason`, `error` | `subprompts/fanout.rs:122-128` (text + keys CHANGE; `subpromptId` → `reason`) |
| b27e5089a | WARN | `Failed to recompile a seat in an identity-stack fan-out` | `chatId`, `participantId`, `characterId`, `reason`, `error` | `subprompts/fanout.rs:184-192` (CHANGES) |
| b27e5089a | DEBUG | `Identity-stack fan-out complete` | `characterId`, `reason`, `chatsTouched`, `seatsRecompiled` | `subprompts/fanout.rs` NEW (before the INFO) |
| b27e5089a | INFO | `Subprompt change fanned out` (logger `Subprompts:Fanout`) | unchanged: `characterId`, `subpromptId`, `removeSelection`, `chatsTouched`, `seatsRecompiled` | `subprompts/fanout.rs:203-212` unchanged |
| eb90d9ed8 | WARN | `Wardrobe list could not read the character's pronouns; listing unfiltered` | `context: 'wardrobe-list-handler'`, `characterId`, `error` | `tools/wardrobe_list.rs` NEW (the pronoun read) |
| eb90d9ed8 | INFO | `Wardrobe list completed` | gains `pronounAware`, `hiddenByPronounGuidance` after `includeEquipped` | `tools/wardrobe_list.rs:299-324` |
| eb90d9ed8 | DEBUG | `[applyOutfitSelections] Pronoun guidance held items back from the green room` | `chatId`, `characterId`, `hiddenCount` | `services/outfit_selections.rs` (the `llm_choose` path) NEW |
| 6588972d5 | DEBUG | `[VaultOverlay] Writing diagram settings to properties.json` | `characterId`, `canDrawDiagrams`, `diagramStyle` | `db/vault_character_update.rs:170-225` NEW (only when `touched` names either key) |
| 074e1e01a | DEBUG | `self_inventory: core files resolved` | `context: 'self-inventory-handler'`, `characterId`, `present`, `total` | `tools/self_inventory.rs` NEW |
| 074e1e01a | DEBUG | `self_inventory: holdings resolved` | `context`, `characterId`, `wardrobe`, `inventory`, `customTools` (`available ?? null`) | NEW |
| 0276d10d9 | INFO | `[InventoryEquip] Refused` | the equip CAP arm gains `op: 'equip'` after `code`; pick-up's INVALID_PARENT arm gains `op: 'pick-up'` after `code`; the other three arms unchanged | `inventory/equip.rs:701`, `:985` |
| 0276d10d9 | WARN | `[InventoryEquip] Arrival dropped an object over its cap` | `op: 'equip'` FIRST, then `chatId`, `characterId`, `itemId`, `cap`, `context` | `inventory/equip.rs:685` |
| 0276d10d9 | WARN | `[InventoryMutations] Skipped instances whose vault is no longer linked` | `op: 'detach-all'`, `mountPointId`, `itemId`, `ownerId`, `count`, `context: 'inventory'` | `inventory/item_mutations.rs:1009-1011` NEW (replaces the silent `continue`) |
| 0276d10d9 | DEBUG | `Inventory projection leaving duplicate-id files alone` | unchanged (`mountPointId`, `paths`, `context`) | `db/inventory_writes.rs` |
| 0276d10d9 | DEBUG | `[InventoryMutations] Baked instances in one mount` | `scope` now `location.scope` (same values) | `item_mutations.rs:1036-1044` |
| 0276d10d9 | INFO | `[Chats v1] Inventory ${mode}` | unchanged | `api/chat_inventory.rs` |
| 265b711dd | — | none | | |

## 5. Harness / oracle impact

Import-dead at the pin in this area (confirmed): **`cases/item-folders.ts`**
— `inventoryFolderStorePath` and `normalizeInventoryFolderPath` are DELETED
from `lib/inventory/folders.ts` (`:96-99`, `:133-136` gone) with NO
replacement wrapper; the generic `itemFolderStorePath(INVENTORY_FOLDER_RULES, …)`
/ `normalizeItemFolderPath(INVENTORY_FOLDER_RULES, input, pathError)` from
`lib/mount-index/item-folders` (already imported at `item-folders.ts:29-52`)
are what the deleted wrappers called. **Re-aim:** replace the two imports
(`:64`, `:69`) with local wrappers over the generic calls (the `pathError`
is `new InventoryFolderPathError(message)`); v5's twins
`inventory/folders.rs:86` (`normalize_inventory_folder_path`) and `:134`
(`inventory_folder_store_path`) become dead — delete them and re-point
`inventory_folders_equivalence.rs` rows at the generic Rust helpers. No other
case in this area is import-dead; `cases/context-feeders-leaves.ts` (imports
`image-scene-tasks`) is dead for P4.D347's reason (`findIntroducedOffSceneCharacterIds`),
not this lane's — P4.D347's re-aim must keep its `image-scene-tasks` rows.

**P4.D349**

| family (`crates/quilltap-harness/tests/`) | oracle case | effect at the pin | fixtures |
|---|---|---|---|
| `vault_json_parsers_equivalence` | `vault-json-parsers.ts:189-194` (physical rows), the properties rows | REDDEN: every `parseVaultProperties` row's output gains `canDrawDiagrams: false, diagramStyle: 'handDrawn'`; physical rows unchanged (an absent `humanoid` serialises as no key) — ADD rows: `humanoid: false / 'false' / 'true' / null / 'yes' / 0`, `diagramStyle: 'clean' / 'crayon' / null`, `canDrawDiagrams: 'true'` (STRICT → null) | none (inline) |
| `vault_character_write_equivalence` (tier 2) | `vault-character-write.ts` | REDDEN: `properties.json` eight keys, `physical-prompts.json` six keys (+ the sha-addressed doc rows move) | `vault-character-write-tier2.json` + `build-vault-character-write-fixture.ts` (consumed; rebuild NOT needed — the oracle writes in-case) |
| `characters_physical_tier2_equivalence` | `characters-physical.ts` | REDDEN: `humanoid` written; ADD the keep-the-vault's rows (write omitting the flag over a stored `false`; explicit `true`; the DEBUG capture) | `characters-physical-tier2.json`, `build-characters-physical-fixture.ts` (the builder writes `physical-prompts.json` — a rebuilt pair writes `humanoid: true` where the committed one does not; see `rebuilt-fixture-pair-vintage`) |
| `characters_scaffold_tier2_equivalence` | `characters-scaffold.ts` | REDDEN: both scaffold files' bytes + their sha rows | `characters-scaffold-tier2.json`, `build-characters-scaffold-fixture.ts` |
| `characters_create_tier2`, `characters_update_tier2`, `characters_mutations_equivalence`, `characters_reads_equivalence`, `characters_actions` | `characters-create.ts`, `characters-update.ts`, `characters-mutations.test.ts`, `characters-reads.test.ts` | REDDEN: every character object gains the two Zod-default keys (and `physicalDescription.humanoid` on vault reads); the PUT accepts the three new keys (ADD a bad `diagramStyle` 400 row, a `humanoid: 'no'` 400 row, the flip-recompiles row) | the committed pairs are READ (old scaffold bytes are fine) |
| `vault_read_overlay_equivalence` | `vault-read-overlay.ts` | REDDEN: hydration adds the three keys | `vault-read-overlay-tier2.json`, `build-vault-read-overlay-fixture.ts` |
| `metadata_vault_roundtrip_equivalence` | `metadata-vault-roundtrip.ts` | REDDEN: the RMW keeps / writes eight keys | — |
| `system_export_equivalence`, `qtap_import_equivalence`, `system_backup_equivalence`, `system_restore_*` | `system-export*.ts` etc. | REDDEN: character records carry two more keys in schema order (needs the key-order dump — §7.2); vault file bytes | the export/import fixtures |
| `character_archive_tier2_equivalence` | `character-archive-tier2.test.ts` (imports chat-enrichment) | REDDEN: `diagramStyle` on the detail | — |
| `system_prompt_equivalence` | `system-prompt.ts:224-245` (`buildIdentityStack` physical rows), `:407` (`buildSystemPrompt`) | GREEN for existing rows; ADD: non-humanoid with text / without text / humanoid-no-text; `canDrawDiagrams: true` (note after math, before taboo; with a precompiled stack; `diagramStyle` irrelevant) — the eight `it`s of `diagram-formatting.test.ts` and the three of `non-humanoid-appearance.test.ts` are the row list | — |
| `identity_compiler_equivalence` | `identity-compiler.ts` | GREEN (fixture characters are humanoid); ADD one non-humanoid seat | `identity-compiler.json`, builder |
| `physical_description_equivalence` (tier 1) | `physical-description.ts` (imports the module — ALIVE) | GREEN; ADD `isHumanoid` rows and the `head-and-shoulders` non-humanoid order (the 19-line test hunk) | `physical-description.json` |
| `small_utils_equivalence` | `small-utils.ts:32` (pronoun-gender) | GREEN; ADD the `figure*` rows (`pronoun-gender.test.ts:52-69`) | — |
| `wardrobe_item_image_prompt_equivalence` | `wardrobe-item-image-prompt.test.ts:46-47` (imports `buildFigureIdentityBlock`) | REDDEN (shape): the block gains `humanoid` on EVERY row; ADD the non-humanoid owner row (`item-image-prompt.test.ts:303`) | `wardrobe-item-image-prompt.json` |
| `inventory_image_description_equivalence` | `inventory-image-description.test.ts:36-39` | GREEN; ADD `non-humanoid-scene.test.ts`'s four rows (`sceneDescriptionParts` drops the prefix + direction; never doubles the period; humanoid unchanged; the image tool flags + no gender) and `avatar-prompt.test.ts`'s four | `inventory-image-description.json` |
| `avatar_job_tier3_equivalence`, `wardrobe_tools_avatar_trigger_equivalence` | `avatar-job.test.ts`, `wardrobe-tools-avatar-trigger.ts` | GREEN; ADD a non-humanoid character (`its whole form in frame.` / `Only one subject`) | `avatar-job.json`, builder |
| `story_background_job_tier3_equivalence` | `story-background-job.test.ts` | GREEN; ADD a non-humanoid cast member (both the resolved and the fallback paths; the enumeration) + the new guideline line in the canned prompt | `story-background-job.json` |
| `headshoulders_backfill_tier3` / `_enqueue` | the two cases | GREEN; ADD the skip row (DEBUG) + the scan's `skipped++` | `headshoulders.json`, `build-headshoulders-fixture.ts` (writes `physical-prompts.json`) |
| `appearance_sanitize_gate_tier3`, `image_gen_leaves`, `character_optimizer_prompts` (+ `_tier3`) | their cases | GREEN; ADD a non-humanoid row each (the optimizer's extra rule line) | — |
| `subprompts_prompt_tier2`, `subprompts_storage_tier2` | `subprompts-prompt.test.ts`, `subprompts-storage.test.ts` (import `chat-fanout`) | REDDEN only where the fan-out's WARN texts are capture-pinned (the unit `log_tests` in `fanout.rs:215+` certainly are) | — |
| `vault_wardrobe_item_file_equivalence` | `vault-wardrobe-item-file.ts:60-90` | GREEN; ADD `pronounGuidance: [she, they]`, a single string, `He/Him/His`, junk, empty | — |
| `vault_wardrobe_emit_equivalence` | `vault-wardrobe-emit.ts` (imports `character-vault`) | GREEN; ADD the YAML-list emission rows (position after `replace`) | — |
| `vault_wardrobe_write/read/public`, `wardrobe_routes_equivalence`, `group_wardrobe_routes`, `wardrobe_folder_writes_tier2` | the REST cases | GREEN; ADD create / PUT with `pronounGuidance` (+ a bad value 400 `invalid_value`), the read wire | the wardrobe-routes pairs |
| `wardrobe_tools_equivalence` (tier 2/3) | `wardrobe-tools.ts:218` (`wardrobe_list`) | REDDEN: every list item gains `pronoun_guidance: []`, every list output `hidden_by_pronoun_guidance: 0`, every read `pronoun_guidance: []`; the formatted text is unchanged until an item carries guidance. The fixture's characters have NO pronouns (grep the builder → none), so the FILTER is untestable without a widen: ADD a he/she character + guided items + a worn-but-hidden item (the five `wardrobe-handlers.test.ts` `it`s) | `wardrobe-tools.json`, `build-wardrobe-tools-fixture.ts` (WIDEN — rebuild hazard noted) |
| `outfit_llm_choose_tier3_equivalence` | `chat-dialogs-llm-choose-tier3.test.ts` (imports `apply-outfit-selections`) | GREEN; ADD a guided-items row (the DEBUG) | — |
| `tool_definitions_equivalence` | `tool-definitions.ts` | UNION RED until the catalog is regenerated (§7.3) | — |

**P4.D350**

| family | oracle case | effect | fixtures |
|---|---|---|---|
| `tool_definitions_equivalence` | `tool-definitions.ts` | REDDEN on the five entries (§7.3) | — |
| `pseudo_tool_prompts_equivalence` | `pseudo-tool-prompts.ts:18` | REDDEN: every row with `wardrobeCreate` / `inventoryCreate` on | — |
| `pseudo_tool_parsers_equivalence` | `pseudo-tool-parsers.ts` | GREEN (no `CREATE_WARDROBE_ITEM` body row exists — grep); ADD the `text-block-create-items.test.ts` rows (body → `description`) | — |
| `self_inventory_equivalence` | `self-inventory.ts` | REDDEN: every default / `vault` row gains `coreFiles` + `holdings` (JSON and text); the `includedParts` split; ADD the eight `self-inventory-handler.test.ts` rows (`vault.coreFiles` alone; `no_vault`; `holdings.inventory` alone; custom tools off; one failing part) — the fixture has NO Inventory objects or custom tools, so the holdings rows need a widen | `self-inventory.json`, `build-self-inventory-fixture.ts` (WIDEN) |
| `inventory_refs_equivalence` (tier 1) | `inventory-refs.ts:220` | **GARBLE** (§0.1 #4) — regen flips the row; rename it `corner-kit-half-equipped-unequip-picks-kit` in the oracle (a spec-name edit, not a semantics one) | — |
| `inventory_types_equivalence` | `inventory-types.ts` | GREEN; ADD `duplicateInventorySchema` rows (missing / `''` / non-string / ok) | — |
| `inventory_routes_equivalence` (web) | `inventory-routes.test.ts` | REDDEN: the duplicate 400 body; ADD an operator REST write's publish (if R-C adopts it — the realtime capture) | `chat-dialogs-web.json`? (`tools_inventory` shares it) |
| `inventory_tools_equivalence`, `tools_inventory_equivalence` | `inventory-tools.ts` | GREEN on bytes (the no-vault pick-up text is unreachable in the corpus; `inventory_update`'s `after` is the same object); REDDEN only on log-key rows (`op`) | `inventory-tools.json`, builder |
| `inventory_keystone_tier2_equivalence` | `inventory-keystone-tier2.ts` | REDDEN on: `detachAll` over an unlinked vault (the WARN + the skip-not-fail), a stray parseable-but-not-a-record `.json` under `Inventory/` (v4 keeps, v5 sweeps — ADD the row), the arrival `[]` write — all DB-diff visible | `inventory-keystone.json`, `build-inventory-mutations-fixture.ts` |
| `chats_inventory_tier2_equivalence` | `chats-inventory-tier2.ts` | GREEN (column writes unchanged) | — |
| `pascal_inventory_effects_equivalence` | `pascal-inventory-effects.ts` | GREEN (the kit fallback moved, same answers) | — |
| `realtime_topics_equivalence` | `realtime-topics.ts` | REDDEN if it dumps `queryKeysForTopic('inventory')` (four keys now) — v5's SPA mapping is R-2's ruled divergence; re-record the row | — |
| `inventory_folders_equivalence` | `item-folders.ts` | IMPORT-DEAD → the re-aim above | — |
| `inventory_image_description_equivalence` | `inventory-image-description.test.ts` | GREEN (`renderCarryingPhrase` survives) | — |

**Generated artifacts to regenerate at the pin (both lanes consume):**
`tools/definitions/data.rs` (`gen-tool-catalog.mjs`),
`services/image_scene_tasks/prompt_text.rs` (`gen-image-scene-prompts.mjs` —
P4.D349's two guideline lines), `services/qtap_export/schema-key-order.json`
(`dump-export-key-order.ts` — P4.D341).

**LIVE-DATA statement (after P4.D349):** a v5 physical write keeps
`humanoid` (explicit value as given; omitted → the vault's current); a v5
properties write (full, RMW, backfill, scaffold) carries `canDrawDiagrams` and
`diagramStyle`; a v5 wardrobe projection carries `pronounGuidance`. The P4.82
backfill skips a non-humanoid. Until then the ledger's §1 rule stands: no v5
build against an instance v4 at HEAD has written.

## 6. SPA

| v4 component (pin) | v5 Angular file | lane today |
|---|---|---|
| `app/aurora/[id]/view/components/DescriptionsTab.tsx` (Humanoid checkbox; the H&S hint toggles to `Not used for a non-humanoid character — its avatar is drawn from the Medium prompt, whole form in frame.`) | `apps/web/src/app/screens/characters/edit/appearance-tab.ts:15-45, 110-120` (the physical form; `headAndShouldersPrompt` at `:115`) | NONE |
| `app/aurora/[id]/view/types.ts` `humanoid?` | `core/core-contract.ts` (the `physicalDescription` type) | NONE |
| `CharacterBasicInfo.tsx` picker spreads `STANDARD_PRONOUNS` | `screens/characters/edit/character-form.ts:105-121` (`PRONOUN_PRESETS` — the four literals to share) | NONE |
| `components/wardrobe/PronounAwareToggle.tsx`, `WardrobePronounGuidanceField.tsx`, `use-pronoun-guidance-filter.ts` | NEW `wardrobe/pronoun-aware-toggle.ts`, `wardrobe/item-editor/wardrobe-pronoun-guidance-field.ts`, `wardrobe/pronoun-guidance-filter.ts` (+ a shared `characters/pronoun-presets.ts` and the pure rule twin for the SPA's own specs) | NONE |
| `WardrobeBrowser.tsx`, `wardrobe-control-dialog.tsx`, `equipped-slot-row.tsx`, `outfit-composer.tsx`, `outfit-selector.tsx`, `wardrobe-item-editor.tsx`, `wardrobe-item-row.tsx` | `wardrobe/wardrobe-browser.ts`, `wardrobe-control-dialog.ts`, `equipped-slot-row.ts`, `outfit-composer.ts`, `screens/new-chat/outfit-selector.ts`, `wardrobe/wardrobe-item-editor.ts`, `wardrobe/wardrobe-item-row.ts`, `wardrobe/item-draft.ts:38-52` (`pronounGuidance: PronounGuidance[]`) | NONE — and P4.D358's dialog / accessible-name hunks likely touch the same dialogs |
| `components/characters/CanDrawDiagramsToggle.tsx`, `CharacterEditView.tsx`, `CharacterDetailView.tsx`, `useCharacterEdit.ts`, `useCharacterView.ts`, `save-diagram-settings.ts`, `ParticipantCard.tsx`, `useParticipants.ts`, `salon/types.ts` | P4.D353's (`screens/characters/edit/character-edit.ts:169-179` is the `canChooseOutfit` card's neighbour; `screens/characters/choose-outfit-card.ts` the detail-page precedent) | P4.D353 |
| `lib/hooks/use-inventory.ts`, `use-inventory-vantage-items.ts`, `topic-map.ts` | `inventory/inventory-control-dialog.ts:340-355`, `inventory/inventory-vantage-items.ts`, `core/realtime-topic-map.ts:151-159` | P4.D350 (SPA bit) — collides with P4.D358 if it touches the Inventory dialog |

**Wire contract (JSON keys; `crates/quilltap-core/src/api/types.rs` carries
character responses as `Value` passthroughs — nothing to change there):**

- Character LIST item: `canDrawDiagrams: boolean` (never null; `?? false`),
  `diagramStyle: 'handDrawn' | 'clean'` (never absent) — after
  `canChooseOutfit`, before `defaultConnectionProfileId`.
- Character DETAIL / PUT response: the overlaid object — `canDrawDiagrams`,
  `diagramStyle` in schema position (after `canChooseOutfit`),
  `physicalDescription.humanoid?: boolean` (present on every vault read,
  appended after `updatedAt`).
- PUT body: `canDrawDiagrams?: boolean`, `diagramStyle?: 'handDrawn'|'clean'`
  (400 otherwise), `physicalDescription.humanoid?: boolean`.
- Chat participants' `character` (`getEnrichedCharacter`): `diagramStyle`
  after `archivedAt`; `EnrichedCharacterDetail` (`getCharacterDetail`):
  `diagramStyle` after `archivedAt`.
- Wardrobe item (REST + the `wardrobe_*` tools): `pronounGuidance?: ('he'|'she'|'they'|'it')[]`
  (a created item always answers it, `[]` when none; a read item only when
  non-empty — the parser's conditional spread); the tools' `pronoun_guidance:
  string[]` (always), `hidden_by_pronoun_guidance?: number`.
- SPA types: `core/core-contract.ts:3632` (list), `:3692` (detail),
  `:3368-3375` (participant character, after `archivedAt`), `:4420-4432`
  (wardrobe item, after `replace`). **Whoever adds a key to
  `core-contract.ts` does it in a lane-fenced block** (the file is touched by
  P4.D344 / D346 / D352 / D353 / D359 / the vault-keys SPA too).

Component-harness / Playwright specs touched: `appearance-tab.spec.ts`,
`item-draft.spec.ts`, `wardrobe-item-editor.spec.ts`, `wardrobe-item-row.spec.ts`,
`wardrobe-browser.spec.ts`, `wardrobe-control-dialog.spec.ts`,
`outfit-composer.spec.ts`, `outfit-selector.spec.ts`,
`inventory-control-dialog.spec.ts`; e2e `wardrobe-items-editor-flow.spec.ts`
(a guidance-field beat), `wardrobe-flow.spec.ts` (the toggle + the held-back
count), `characters-flow.spec.ts` (the Humanoid checkbox round-trips to
`physical-prompts.json`), `inventory-flow.spec.ts` (an operator write
refreshes another open dialog — needs R-C).

## 7. The lane split, by FILE OWNERSHIP

### 7.1 Confirmation + amendments

The planner's split holds with THREE amendments: (a) the wardrobe half of
`265b711dd` (the `wardrobe_create` / `wardrobe_update` description bytes and
the `CONTENT_PARAM_MAP` row) moves to **P4.D349** — the same catalog entries
eb90d9ed8 edits; (b) `tools/definitions/data.rs` is regenerated ONCE (§7.3);
(c) the SPA halves get a home (R-A).

**P4.D349 creates / edits (all `crates/quilltap-core/src/` unless noted):**
`vault_overlay.rs`, `db/vault_character_write.rs`, `db/vault_character_update.rs`,
`db/vault_read_overlay.rs`, `db/character_vault.rs`, `db/characters.rs`,
`db/characters_read.rs`, `api/characters.rs`, `api/wardrobe_route_factory.rs`,
`services/character_enrichment.rs`, `services/chat_enrichment.rs`,
`services/chat_participants.rs`, NEW `markdown/diagrams.rs` (or
`characters/diagrams.rs`), `system_prompt.rs`, `services/system_prompt_compiler.rs`,
`characters/physical_description.rs`, `pronoun_gender.rs`, NEW
`characters/pronoun_presets.rs`, NEW `wardrobe/pronoun_guidance.rs`,
`services/avatar_prompt.rs`, `services/wardrobe_item_image_prompt.rs`,
`services/appearance_resolution.rs`, `services/image_scene_tasks.rs`,
`services/image_scene_tasks/prompt_text.rs` (REGEN), `services/scene_state_tracking/mod.rs`
(+ the baseline builder), `services/story_background_job.rs`,
`services/headshoulders_backfill_job.rs`, `services/headshoulders_backfill_enqueue.rs`,
`generators/optimizer.rs`, `subprompts/fanout.rs`, `tools/generate_image.rs`,
`tools/wardrobe_list.rs`, `tools/wardrobe_read.rs`, `tools/wardrobe_create.rs`,
`tools/wardrobe_update.rs`, `tools/text_block_parser.rs` (the ONE map row —
see the P4.D350 overlap), `services/outfit_selections.rs`,
`services/quilltap_import/characters.rs`, `services/qtap_export/records.rs`
(only if the spread needs a hand), plus the harness families in §5 and the
oracle cases/fixtures they name. **One fenced hunk in P4.D350's file:**
`tools/self_inventory.rs:1337-1345` (`to_sys_char`'s `PhysicalDescription`
literal gains `humanoid`) — fence it `// === P4.D349: humanoid ===`.

**P4.D350 creates / edits:** `tools/definitions/data.rs` (REGEN — §7.3),
`tools/text_block_prompt.rs`, `tools/text_block_parser.rs` (the
`content_param` fn — if P4.D349 takes the wardrobe row the whole file is
P4.D349's; decide ONE owner: recommend **P4.D350 owns both text-block files
and lands the `wardrobe_create: 'description'` row** since it is one line in
a map P4.D350 already edits for the prompt), `tools/self_inventory.rs` (all
but the fenced hunk), NEW `tools/self_inventory_holdings.rs`,
`generators/field_semantics.rs` (`CORE_VAULT_FILE_SUMMARIES`),
`api/inventory_route_factory.rs`, `api/chat_inventory.rs`, `api/inventory.rs`,
`api/inventory_item_images.rs` (if R-C), `inventory/{equip,equip_effects,equipped_set,image_description,prompt_section,pool,refs,item_mutations,location,apply_defaults,folders,errors}.rs`,
`db/inventory_readers.rs`, `db/inventory_writes.rs`, `db/item_folder_ops.rs`,
`tools/inventory_{shared,archive,create,give,update,unequip,pick_up,equip}.rs`,
`tools/inventory_tool_table.rs`, `pascal/inventory_effects.rs`,
`crates/quilltap-web/src/inventory_routes.rs` (if the 400 needs the edge),
SPA `inventory/inventory-control-dialog.ts`, `inventory/inventory-vantage-items.ts`,
`core/realtime-topic-map.ts` (comment only), the §5 families + `item-folders.ts`.

**Disjointness proof:** the two file lists intersect ONLY at
`tools/self_inventory.rs` (one fenced hunk, 500+ lines from P4.D350's
nearest edit at `:2081`) and — unless the owner rule above is taken —
`tools/text_block_parser.rs`. `tools/definitions/data.rs` is NOT edited by
either lane by hand (regen contract). No other lane in the round's table
names a file in either list EXCEPT: `api/characters.rs` (P4.D356? no —
chat-settings only; P4.D348 the turn spine touches `services/build_context.rs`,
`services/orchestrator.rs`, not these), `system_prompt.rs` (P4.D343
Storyteller may push prompt text — if so its hunks must avoid `:521-529` and
`:644-655`; P4.D347 owns `build_context.rs` only), `core-contract.ts` (many —
fenced blocks), the wardrobe / inventory dialog `.ts` files (P4.D358 — the SPA
halves must STACK on P4.D358's tip or P4.D358 must not touch those files).

### 7.2 Named shared contracts

- **C-1 `system_prompt::Character.can_draw_diagrams: bool` + `PhysicalDescription.humanoid: Option<bool>`** —
  built by P4.D349 (`system_prompt.rs:107-115, 136-153`); populated by
  P4.D349 in `system_prompt_compiler.rs::character_from_value` and the fenced
  `self_inventory.rs` hunk; every other literal site compiles on
  `..Default::default()` (`build_context.rs:3676, 3971`; `carina_query.rs:1493`).
  P4.D347 consumes nothing new: the note is pushed inside `build_system_prompt`.
- **C-2 the wire keys** (§6) — built by P4.D349 (server), consumed by
  P4.D353 (`diagramStyle` on list / detail / participants; the PUT body) and
  by the vault-keys SPA (`physicalDescription.humanoid`, `pronounGuidance`).
  Exact names: `canDrawDiagrams`, `diagramStyle` (`'handDrawn' | 'clean'`),
  `physicalDescription.humanoid`, `pronounGuidance`, tools `pronoun_guidance`,
  `pronoun_aware`, `hidden_by_pronoun_guidance`.
- **C-3 `schema-key-order.json`** — built by P4.D341 (ONE dump at the pin,
  carrying `character`'s two new keys with the keystone's own kinds);
  consumed by P4.D349's export families. Either P4.D349 branches from
  `KEYSTONE` (wave 1 — recommended, R-D) or accepts `system_export_equivalence`
  red until unify.
- **C-4 `tools/definitions/data.rs`** — §7.3.
- **C-5 `inventory::equip_effects::inventory_change_context(user_id, provenance, announce, pending)`**
  and **`inventory::equipped_set::load_equipped_set`** — built and consumed
  within P4.D350; named so P4.D345 (Pascal intervention server) does not
  re-spell them if it touches `pascal/inventory_effects.rs` (it should not).
- **C-6 the realtime publish shape (R-C)** — if adopted, `InventoryLocation`
  publishes `Inventory` on landed writes; the tools' `publishes` lists drop
  their `Inventory` entries (keep `Chats` where v4 keeps it — none after the
  refactor: `dropCarriedEverywhere` publishes itself). Consumed by the SPA's
  dialog subscription (P4.D350 SPA bit).

### 7.3 The generated catalog (`data.rs`) — ONE regen

`harness/oracle/tools/gen-tool-catalog.mjs` run at the pin rewrites all 76
entries: this round's description deltas are `wardrobe_create` /
`wardrobe_update` (eb90d9ed8 + 265b711dd), `wardrobe_list` / `wardrobe_read`
(eb90d9ed8), `inventory_create` / `inventory_update` (265b711dd),
`self_inventory` (074e1e01a), `project_info` (`project-info-tool.ts` −17/+?),
`run_custom` (+16), `state` (+14) — the last three belong to other lanes
(P4.D345 / P4.D343 by the span's authorship; measure with
`git log 322a5488e..236c04e35 -- lib/tools/project-info-tool.ts lib/tools/run-custom-tool.ts lib/tools/state-tool.ts`).
Recommendation (R-B): **the regen lands as the FIRST commit of P4.D350**
(its branch tip is `CATALOG`); P4.D349 branches from `CATALOG` (it needs the
`pronoun_*` params to drive `wardrobe_tools_equivalence`); P4.D345 / P4.D343
branch from `CATALOG` too if their definitions moved, else their
`tool_definitions_equivalence` stays red until unify. `tool_definitions_equivalence`
is green only on a tree that carries the whole regen — a UNION family, as
last round. The v5 `validate()` functions ignore unknown keys (Zod's
non-strict objects), so a regenerated catalog advertising `pronoun_guidance`
before P4.D349's handler lands is harmless within the round.

### 7.4 Stacking

- P4.D350 first commit = the catalog regen (`CATALOG`); P4.D349 branches from
  `CATALOG` (and, per R-D, from `KEYSTONE` if the key-order dump matters more
  — the two are compatible if `CATALOG` is cut from `KEYSTONE`, which makes
  P4.D350 wave 1 as well: recommended).
- The SPA halves (R-A) stack on P4.D358's tip if P4.D358 edits any of the
  wardrobe / inventory / appearance files; otherwise wave 0 from `CATALOG`.

## 8. Tiering

**P4.D349**

- Tier 1 (must land): the three readers + three writers (§1.1–1.3 vault
  bytes), the scaffold defaults, `MANAGED_FIELDS` / `UPDATE_SCHEMA_KEYS` /
  `characters_read` defaults, the list / detail / participant wire keys, the
  PUT arms (incl. the Zod 400s), `[FORMATTING: DIAGRAMS]`, the identity
  stack's `formNote`, `isHumanoid` + the H&S order + `figure*`, the avatar
  prompt + wardrobe picture + `sceneDescriptionParts` + prompt expansion +
  the three cheap-LLM formatters + the two guideline lines (prompt_text
  regen), story-background enumerations, the backfill skip (both sites), the
  optimizer line, the wardrobe item parse / emit / REST schema /
  `wardrobeItemFromCreateBody`, the four `wardrobe_*` tools' params + output
  keys + formatted lines + the WARN, the green-room filter + DEBUG, the
  import carrier, every log line in §4 capture-pinned, the §5 families
  regenerated and green (or recorded union reds).
- Tier 2 (should land): `recompileCharacterSeats` as the generalised fan-out
  (the PUT flip recompiles LLM seats, publishes `chats`, the two WARN texts +
  the DEBUG move target); the per-site DEBUG keys; the fixture widens
  (wardrobe-tools pronouns, identity-compiler non-humanoid seat); the
  `diagramStyle` on `EnrichedCharacterDetail`'s early-return arm.
- Tier 3 (loud typed refusals): none needed — no model-dependent path is
  deferred. If the fan-out slips, the PUT must still ACCEPT `humanoid` and
  record `recompile: not_available` in its DEBUG rather than silently skip.

**P4.D350**

- Tier 1: the catalog regen; the text-block prompt + `CONTENT_PARAM_MAP` row;
  `self_inventory` `vault.coreFiles` + `holdings` (all three parts, the
  nine summaries, the `includedParts` split, the eleven-section texts);
  the duplicate 400; `seatedCharacterIds`; `pickForGesture`'s kit arm (+ the
  two callers); the `op` keys; `detachAll`'s two-phase resolve + WARN; the
  arrival / reset carried-set publishes; `inventoryChangeContext` /
  `loadEquippedSet` / `loadOwnRecord` chokepoints (and the deletions:
  `describe_equipped_inventory_for_images`, `kin_of`, `read_duplicate_inventory_paths`,
  the two folders twins); `ownedPaths` sweep rule; the `item-folders.ts`
  re-aim; `inventory_update`'s resolve-in-hand.
- Tier 2: R-C's publish relocation (or, if not adopted, the REST factory's
  missing `inventory` publish fixed the v5 way — operator writes must reach an
  open dialog either way); the SPA dialog reloads (archetype pickers,
  Carrying-now) and `refreshUnlessRealtime`; the `Elsewhere` tag drop; the
  memoised owner resolver (perf only).
- Tier 3: none; `holdings.customTools` when the roster subsystem refuses
  (degraded mount) answers v4's `{ available: false, message }` with the
  error text — that is v4's own shape, not a stub.

## 9. Fixtures

- **Consumed unchanged:** every committed real-DB pair (scaffold-built pairs
  carry the OLD six-/four-key files — correct for reads; a rebuild would
  write the NEW bytes, so never rebuild one just to "refresh").
- **Widen (P4.D349):** `build-wardrobe-tools-fixture.ts` (+ `wardrobe-tools.json`)
  — a he/she character, three guided items (one worn, one for the other
  pronouns, one unguided), a custom-pronoun character; `build-identity-compiler-fixture.ts`
  (one non-humanoid seat); `avatar-job`, `story-background-job`, `headshoulders`
  builders (one non-humanoid character each — `physical-prompts.json` with
  `"humanoid": false`). Regenerate the dependent oracles after each widen; the
  widening lane regenerates.
- **Widen (P4.D350):** `build-self-inventory-fixture.ts` (two Inventory
  objects + a kit + an archetype on two tiers, one carried; one custom tool;
  a vault with one core file MISSING and one EMPTY); `build-inventory-mutations-fixture.ts`
  (a dependent instance in a vault whose character link is then broken; a
  stray `Inventory/notes.json` object that is not a record).
- **Regenerated artifacts:** `data.rs` (P4.D350), `prompt_text.rs`
  (P4.D349), `schema-key-order.json` (P4.D341).
- **Oracle NDJSON:** all per-sweep (`QT_ORACLE_*`), nothing committed beyond
  the corpora named in §5.

## 10. Rulings owed to the human

- **R-A** The SPA halves (b27e5089a's checkbox, eb90d9ed8's toggle / field /
  pickers / shared presets, 0276d10d9's dialog reloads): a new lane
  (`P4.D361 vault-keys SPA`), or folded into P4.D359 (riders) with a stacking
  rule on P4.D358. Not a port decision — a lane-table decision.
- **R-B** `data.rs` regen owner and the `CATALOG` stacking (§7.3). Includes
  whether `project_info` / `run_custom` / `state` definition deltas belong to
  P4.D343 / P4.D345 (measure the authorship first).
- **R-C** Adopt v4's location-level `inventory` publish on landed writes (and
  drop the tools' handler-owned `Inventory` publishes), or keep v5's
  after-commit `publishes` lists and add the REST factory's missing publish
  there. Measured facts: `publish_realtime` is fire-and-forget and already
  called inside the equip chokepoints (`equip.rs:769`); the tool table's
  header (`inventory_tool_table.rs:16-25`) says publishes "cannot run in
  there" — a claim to re-measure (the avatar refresh cannot; a bus hint can).
- **R-D** P4.D349's wave: wave 1 from `KEYSTONE` (the key-order dump) — or
  wave 0 with a recorded transient red on the export families.
- **R-E** Playwright: the Humanoid checkbox and the pronoun-aware toggle are
  spend-free beats; the non-humanoid AVATAR is a 💸 beat (dogfood only).
- **NO-PORT candidates (not docs/infra):** none in this area. `cad017599`-style
  test-only commits: none.
- **Deliberate divergence to confirm:** NONE proposed; the `ownedPaths` sweep
  rule (a stray parseable `.json` survives) is v4's and v5 follows it.

## 11. Dogfood rows (on a Friday copy taken at any v4 point)

1. Open a character whose `physical-prompts.json` v4 wrote with
   `"humanoid": false` — the PUT of an unrelated field keeps it; the
   Descriptions save with the box unticked keeps it; the AI Wizard / Optimizer
   regen keeps it (the DEBUG line).
2. The non-humanoid avatar 💸 (`its whole form in frame.` / `Only one
   subject in the image.`); the worn wardrobe picture 💸 (`Bearing …`); a
   Lantern background with a mixed cast 💸 (`[not humanoid]` + the `Form:`
   line in the LLM log); the scene-state baseline's `- Form:` line.
3. The identity stack: a non-humanoid's turn carries `You have no humanoid
   body…`; flipping the box recompiles the seated chats (the Salon's prompt
   view updates without a restart; the `chats` realtime hint).
4. The head-and-shoulders backfill skips the non-humanoid at boot (`skipped`
   count + the DEBUG).
5. `properties.json` written by v4 with `canDrawDiagrams: true, diagramStyle:
   'clean'` survives a v5 rename / talkativeness edit; the Almanack / detail
   / list wires show both keys; a drawing character's prompt carries
   `[FORMATTING: DIAGRAMS]` directly after the math note (the LLM Inspector).
6. A wardrobe item v4 filed with `pronounGuidance: [she, they]` survives a
   v5 edit of its description; `wardrobe_list` from a he/him character hides
   it with the trailing note; `pronoun_aware: false` shows it; the worn item
   is never hidden; the green room's valet 💸 offers only fitting items.
7. `self_inventory` bare → the `## Core Vault Files` block (9 of 9 on a
   scaffolded vault; a hand-deleted `metadata.json` reads MISSING with its
   named `qtap://` uri) and `## Holdings` (counts match the dialog).
8. Text-block mode (a provider without native tools): `[[CREATE_WARDROBE_ITEM
   … cue="…"]]body[[/…]]` lands the body in the description and the cue in
   the Portrait Cue.
9. `POST …/inventory?action=duplicate` with `{}` → 400 `Validation error`;
   an operator REST create refreshes a second open Inventory dialog (R-C);
   a detach-all with one dependent in an unlinked vault bakes the rest and
   logs the WARN; a user-type seat carrying a character appears in the chat's
   Inventory GET.
10. The Friday copy's first boot after this round: NO ensure runs (nothing
    here is SQL) — only the next physical / properties / wardrobe WRITE
    changes bytes, and only by adding keys.
