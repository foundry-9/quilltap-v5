# Survey — the Inventory TOOLS + Pascal inventory support, server side (`e1872c94f` minus its SPA; `102a23648`'s tool/equip hunks + bug 253; `a53024d3c`'s `inventoryAggregate` half; `cad017599` as a NO-PORT candidate)

Read-only planning survey, **2026-10-20**. v4 = `~/source/quilltap-server`,
read ONLY through `git show 322a5488e:<path>` (**THE PIN** = v4 `main`
`322a5488e7bd93f0234a25b42f87d5418dbf4c2c`, `4.11.0-dev.53`) and
`git show <sha> -- <paths>` / `git diff e8e17b2d9 322a5488e -- <paths>`. **The
oracle baseline v5 has ported is `e8e17b2d9`**; v5 = `main` at **`5a94a9d1e`**
(`/Volumes/External 1/source_overflow/quilltap-v5`). Every v4 line number
below is AT THE PIN; every v5 line number is at `5a94a9d1e`. Nothing is taken
from commit prose (ledger §5.3) — every claim is from a shipped hunk. The
ledger rows `e1872c94f` (`drift-ledger.md:402`), `102a23648` (`:403`),
`cad017599` (`:405`) and `a53024d3c` (`:419`) were read first and are built on;
where a hunk disagrees with a row this survey says so (§8).

## Headline findings (written last)

1. **The slice sits ON TOP of the keystone sibling.** v5 has NO `lib/inventory`
   at all (no pool, no `equipItems`, no `resolveItem`, no `inventory.types`,
   no `equippedInventory` column, no `postPascalInventoryChange`) — every file
   here imports the `4fbdad186` base the keystone lane ports. Nothing in this
   slice can compile before the keystone's `inventory` module exists; the lane
   cut below is a DAG (keystone → tools ∥ Pascal), not a parallel swarm.
2. **The catalog at the pin is 76 tools, not 72** (63 + 9 `inventory_*` + 3
   `log_*` + `edit_custom_tool`); the ledger's "→ **72**" predates the Log and
   `edit_custom_tool`. `tool_definitions_equivalence` ("no extras") and
   `tools_inventory_equivalence` go red at the pin on regen until all four
   siblings land — a UNION red to plan for, not a lane red.
3. **Two oracle RUNTIME BREAKS at the pin, both positional:** v4 `buildTools`
   gained a 13th positional `canManageInventory` BEFORE `docToolsMode`
   (`streaming.service.ts:180`), so `harness/oracle/cases/tool-build.test.ts:142-162`
   now passes `'full'|'off'` as the inventory flag and shifts every later
   argument by one; `determineTextBlockToolOptions` gained an 8th
   (`pseudo-tool.service.ts:114`), so `pseudo-tool-prompts.ts:121-129`'s 7-arg
   call yields `inventoryToolFlags(undefined)` = all nine ON and every
   `build_text_block_system` / `build_simple_json_system` row grows nine
   sections. Both orders must own the oracle edit.
4. **Bug 253 is ONE hunk** — `lib/chat/context-manager.ts:2192-2193` builds
   `buildInventorySection` unconditionally; the section itself is
   `lib/inventory/prompt-section.ts` (keystone, `4fbdad186`). Bug 254's hunk
   (`:2174`, PORTED P4.D322) is the twin above it; v5's doc comment for
   `is_continue_mode` must gain "inventory" when the section lands.
5. **The effect-refine sentences the ledger quotes for `e1872c94f` are NOT the
   pin's.** `e4a60ad9e` (the Log) reworded them again: `an effect is exactly
   one of a write (target + value), a wardrobe gesture, an inventory gesture,
   or a log line` / `an effect needs target + value, wardrobe, inventory, or
   log` (`custom-tool.types.ts:1000,1003`). The definition family's refusal
   bytes are a THREE-sibling union (inventory + any-of + log).
6. **The `inventoryAggregate` half of `a53024d3c` shares four functions with
   the any-of half:** `gateGroups`/`whenGroups`/`effectWhenGroups` readers,
   `gateNamesInventory` (walks groups), `collectGroup` in the vocabulary, and
   `listComparatorHolds` in `metadata-match.ts` — the last changes PLAIN
   metadata semantics too (a list-valued key now matches), so it must land
   with whichever sibling goes first, under the one `pascal_custom_tools_
   execution_equivalence` family.
7. **The v5 wardrobe twins are two-halved (in-write / after-commit) and the
   inventory effects + tools must follow the same split:** `notifyInventoryChanged`
   posts a Pascal line AND refreshes the avatar (async), so `applyInventoryEffects`
   is an in-write planner + an after-commit `finish_*`, exactly
   `wardrobe_effects.rs`'s shape; the nine tool handlers are
   `run_inventory_tool_handler` + `finish_inventory_tool`, the
   `wardrobe_tool_table.rs` shape. The per-turn container becomes a THIRD
   member of `new_wardrobe_turn_containers()` (`Arc<Mutex<HashMap<String,
   PendingInventoryChange>>>`), and the orchestrator drains it in a
   `finally`-equivalent AFTER the turn is saved (102a23648), unlike the
   wardrobe set which drains BEFORE.
8. **`cad017599` is test-only (VERIFIED) and a good tier-3 corpus seed:** its
   seven `it`s are a title-keyed gate, the name-fallback rule, and an
   apostrophed `{{inventory.*}}` render — all reproducible over the keystone's
   fixture as `pascal_run_custom_handler_equivalence` rows. NO-PORT the file,
   mirror the cases.
9. **One ruling owed:** the mock bench's `InventoryContext.item()` is ALWAYS
   null (`bench-inventory.ts:67`), so the preview/audit `inventory` body renders
   `{{inventory.*}}` as written and every `$inventory` operand falls back —
   v5 must reproduce that asymmetry rather than "improve" it (P4.6ay unit 12's
   bench doctrine). And the job-child read-your-writes overlays
   (`child-repositories-proxy.ts`, +210) are NO-PORT (v5 has no forked child)
   — but the SEMANTICS they guarantee (two gestures in one run see each other)
   must be proven by `applyInventoryEffects`'s re-read (`inventory-effects.ts:207`).
10. **Tiering:** Tier 1 = the nine tools + the shared context/refs + the
    turn container + the flush + the roster gate + the Pascal subjects/operand/
    placeholders/effect + the preamble/description changes + the four off-site
    `false`s + the tools-list rows + the realtime topic; Tier 2 = the aggregate
    half, the bench mock, the transfers refactor (route byte-parity), the
    Almanack line; Tier 3 (typed refusal) = nothing — every surface here has a
    v5 home.

---

## 0. The span for this slice, in one paragraph

`e1872c94f` (131 files, +7,933 / −341; the squash of #102) gives a character
nine `inventory_*` tools behind the new per-character `canManageInventory`
flag (null = on; the FOURTEENTH D23 move, surveyed by the keystone sibling —
here it is READ at `orchestrator.service.ts:1015`, `tools/route.ts:628-635`,
`streaming.service.ts:180,309`, `pseudo-tool.service.ts:114,137`, the four
off-sites, and the Almanack ledger `phase3-ledgers.ts:586`). Every reference
resolves through ONE rule, `resolveInventoryRef` (`lib/inventory/refs.ts:89`
— id → title → name, case-insensitive, own tier first), over an
`InventoryContext` loaded ONCE per call or run (`lib/inventory/context.ts:50`).
Pascal gains the `inventory` / `inventoryCount` subjects in `when`s and gates
(literal gate keys), `{{inventory.<ref>.<field>}}` placeholders, the
`$inventory` operand (including a whole dice roll), and the `inventory` effect
(`equip` / `unequip` / `pick_up`) applied AFTER the wardrobe gestures,
fail-soft per gesture. The turn gains a THIRD container,
`pendingInventoryAnnouncements`, netted into one Pascal line per character
and drained by `flushPendingInventoryAnnouncements`; a give posts its own
line at once. The transfer route's body was extracted into
`lib/inventory/transfers.ts` so `inventory_give` shares it. `102a23648` then
fixed the Inventory dogfood: bug 253 (the "Carried and equipped" section built
on every turn), the flush moved into a `try … finally` AFTER the turn is saved,
`looksPlural` / `indefiniteArticle`, the project tier's real name, and the
project roster closing the project's shelf to off-roster characters'
own gestures (`roster-gate.ts`, `OFF_ROSTER` 409, `closedShelfMessage` in
the handlers). `a53024d3c` added the `inventoryAggregate` subject
(`<scope>.<field>` over `equipped` ⊆ `owned` ⊆ `reachable`;
`inventory-aggregate.ts`, `scopeObjects` on the real and mock contexts, the
one comparison table comparing a list entry by entry) beside the any-of
grammar the custom-tools sibling owns. `cad017599` is a jest test only.

**Measured: what the later span commits did to this slice.** Of the 22
commits, these touch the files above AFTER `e1872c94f`: `102a23648` (below),
`7263105c6` (the title lint — `lib/pascal/custom-tools.ts` +59 and NEW
`lib/pascal/tool-gate-keys.ts` 88 lines, `lib/inventory/pool.ts` +48,
`errors.ts` +62: NOT this slice; whichever sibling owns `7263105c6`),
`ea2cb60ab` / `cfa0cba29` / `c56fe278c` (`edit_custom_tool`, the vault-only
rule — `tool-executor.ts`, `run-custom-handler.ts`'s `characterIds` removal,
`custom-tools.ts`: the custom-tools sibling), `e4a60ad9e` (the Log —
`custom-tool.types.ts` +58, `side-effects.ts` +47, `run-custom-tool.ts` +6,
`tool-executor.ts` +29, `plugin-tool-builder.ts` +9, `tool-vocabulary.ts`
+6: the Log sibling), `2b5e8ee24` (the raised hand — `orchestrator.service.ts`
only), `a53024d3c` (below), `322a5488e` (bug 257 — `custom-tools.ts`,
`tool-draft.ts`: the custom-tools sibling). So "at the pin" for THIS slice's
hunks = `e1872c94f` + `102a23648` + `a53024d3c`'s aggregate half, with the
four sibling overlays named by line where they share a function (§5).

---

## 1. Hunk-by-hunk inventory (v4 AT THE PIN)

### 1.A The nine tool definitions (`lib/tools/inventory-*-tool.ts`, all NEW at `e1872c94f`, unchanged since)

Every definition is `{ type: 'function', function: { name, description,
parameters: zodToOpenAISchema(schema) } }` plus a `validate*Input` that is
`schema.safeParse(input)` → data or null. The catalog bytes v5 must carry are
the snapshot's (`lib/tools/__tests__/__snapshots__/tool-definitions-snapshot.test.ts.snap:1211-1480`,
nine `inventory*: parameters snapshot 1` blocks); they are REGENERATED, not
transcribed (§4).

| file | `function.name` | input schema (Zod, in declaration order) | description (verbatim) |
|---|---|---|---|
| `inventory-list-tool.ts:19-39,67-78` | `inventory_list` | `equipped?: boolean` ("true lists only what you have equipped in this chat; false only what you do not. Omit for both."), `kind?: enum item\|kit\|archetype`, `folder?: string` ("Only objects filed in this folder (or beneath it), e.g. \"Weapons\"."), `tier?: enum own\|group\|project\|general` (`InventoryToolTierSchema` `:16`), `include_archived?: boolean` ("Also list your own archived objects. Defaults to false.") | `List the objects you carry and can reach (not clothing — that is the wardrobe). Each row shows its state in this chat: "equipped" (you have it on you), "carried" (in your own vault, not equipped) or "reachable" (in a shared store). Archetypes are classes of object you can pick up an instance of with inventory_pick_up. Shared objects are read-only.` |
| `inventory-read-tool.ts:15-23,50-59` | `inventory_read` | `ref: InventoryRefInputSchema` = `z.string().min(1).max(200)` describe `The object: its id, its title ("Excalibur") or its name ("sword"), case-insensitive.` | `Read one object you carry or can reach in full: its description, rendering hints, metadata, the archetype it derives from, how many may be carried at once, and (for a kit) its members.` |
| `inventory-equip-tool.ts:15-23,36-46` | `inventory_equip` | `refs: InventoryRefsInputSchema` = `z.array(InventoryRefInputSchema).min(1).max(8)` describe `One to eight objects (or kits), each by id, title or name.` | `Take up objects (or kits) so you carry them in this chat. Each ref is an id, title or name. An object you own or can reach in a shared store may be equipped; an archetype must be picked up first with inventory_pick_up. If carrying one more would exceed its limit, nothing changes.` |
| `inventory-unequip-tool.ts:14-16,27-36` | `inventory_unequip` | `refs: InventoryRefsInputSchema` | `Set down objects you carry in this chat (a kit sets down all its members). The object stays where it lives; you simply stop carrying it. Each ref is an id, title or name.` |
| `inventory-pick-up-tool.ts:15-27,39-49` | `inventory_pick_up` | `ref` (re-described `The archetype to pick up an instance of: its id, title or name.`), `title?: string.min(1).max(200)` (`A proper title for your instance ("Old Faithful"). Omit to keep the archetype's.`), `equip?: boolean` (`Equip the new instance at once. Defaults to true.`) | `Pick up an instance of an archetype (a class of object, as inventory_list marks them) into your own vault, and by default equip it. The instance is yours: you may update, archive or give it. If equipping would exceed the limit on how many may be carried, nothing is created.` |
| `inventory-create-tool.ts:14-44,55-65` | `inventory_create` | `name: string.min(1).max(200)` (`The plain name of the object ("sword"). Objects of a kind share it.`), then `inventoryObjectFieldsShape` `:14-38` = `title?` max 200 (`A proper title ("Excalibur"), shown first when set.`), `description?` max 4000, `image_description?` max 2000 (`How the object looks, for pictures (rendering hints only).`), `emoji?` max 16, `max_equipped?: int 1..999` (`At most this many of its kind (same name) may be carried at once. Defaults to 1.`), `metadata?: record<string, string\|number\|boolean\|null>` (`Free-form facts about it, e.g. {"damage": "2d6", "charges": 3}.`); then `equip?: boolean` (`Equip the new object at once. Defaults to false.`) | `Make a new object of your own (not clothing) in your own vault, and optionally equip it. Give it a plain name, and optionally a title, a description, how it looks, an emoji and free-form metadata.` |
| `inventory-update-tool.ts:16-25,36-45` | `inventory_update` | `ref`, `name?: string.min(1).max(200)` (`A new plain name.`), `...inventoryObjectFieldsShape`; `.refine(some key ≠ 'ref' defined, { message: 'Name at least one field to change' })` | `Change one of your OWN objects: its name, title, description, how it looks, emoji, limit or metadata. Objects in shared stores are read-only. Name only the fields you want to change.` |
| `inventory-archive-tool.ts:14-17,29-38` | `inventory_archive` | `ref`, `archived: boolean` (`true to archive it, false to restore it.`) | `Archive one of your OWN objects (it is put away: no longer carried or listed, never destroyed), or restore one you archived with archived=false. Objects in shared stores are read-only.` |
| `inventory-give-tool.ts:16-19,30-39` | `inventory_give` | `ref` (`The object to give: one of your own, by id, title or name.`), `to: string.min(1).max(200)` (`Who receives it: another character in this chat, by name or id.`) | `Hand one of your OWN objects to another character in this chat. It leaves your vault for theirs and you stop carrying it; they decide whether to equip it. Shared objects and kits cannot be given.` |

Output interfaces (the executor's `result` payload minus `formattedText`):
`InventoryListToolOutput { success, items: InventoryListItemResult[], total_count, error? }`
(`:60-65`; row `{ id, name, title, state, kind, tier, folder, is_own,
description, kit_state?, archived? }` `:44-58`); `InventoryReadToolOutput`
`:27-48` (`id, name, title, state, kind, tier, is_own, description,
image_description, emoji, metadata, parent_id, archetype_title, max_equipped,
can_copy, has_picture, folder, members?`); `InventoryEquipToolOutput
{ success, equipped, created, already_equipped, skipped: {ref, reason}[], error? }`
`:27-34`; `InventoryUnequipToolOutput { success, unequipped, not_equipped: string[] }`;
`InventoryPickUpToolOutput { success, item?, archetype?: {id,name,title}, equipped?, error? }`;
`InventoryCreateToolOutput { success, item?, equipped?, error? }`;
`InventoryUpdateToolOutput { success, item?, fields?: string[], error? }`;
`InventoryArchiveToolOutput { success, item?, archived?, changed?, error? }`;
`InventoryGiveToolOutput { success, item?, recipient?: {id,name}, error? }`.
Every `item` is an `InventoryObjectSummary { id, name, title, state, tier, kind }`
(`inventory-handler-shared.ts:41-48`) — key order is the wire.

### 1.B `lib/tools/inventory-tool-flags.ts` (59 lines, NEW)

- `INVENTORY_TOOL_DEFINITIONS` `:22-32` — flag → definition **in offer order**
  `inventoryList, inventoryRead, inventoryEquip, inventoryUnequip,
  inventoryPickUp, inventoryCreate, inventoryUpdate, inventoryArchive,
  inventoryGive`. `INVENTORY_TOOL_FLAGS = Object.keys(...)` `:39`.
- `inventoryToolFlags(canManageInventory)` `:42-45` — all nine `= (flag !==
  false)`; **`null`/`undefined` read as ON** (the same `!== false` doctrine as
  `canDressThemselves`).
- `pickInventoryToolFlags(source)` `:48-54` — copies only DEFINED flags (absent
  stay absent — matters for the prompt-option objects compared byte-wise).
- `enabledInventoryToolDefinitions(options)` `:57-59` — filter then map, offer
  order.

### 1.C The handlers (`lib/tools/handlers/inventory-*.ts`, NEW; the `102a23648` hunks marked ⟨102a⟩)

**`inventory-handler-shared.ts` (167 lines)** — "Mirrors
`wardrobe-handler-shared.ts`" (`:4-5`).
- `InventoryToolContext extends Pick<WardrobeTurnContainers,
  'pendingInventoryAnnouncements'> { userId, chatId, characterId }` `:29-34`.
- `tierOf(item, own)` `:52-56`: own → `'own'`; `origin.scope` `group`/`general`
  as is; ANYTHING ELSE → `'project'` (so a missing origin reads `project`).
- `kindOf` `:58-61` (`isArchetype` → `archetype`, else `kit` or `item`);
  `stateFor` `:65-68` (archetype → null; equipped → `equipped`; own →
  `carried`; else `reachable`); `summarize` `:70-79` (`title: item.title?.trim()
  || null`); `summarizeMatch` `:82`.
- `formatSummary` `:87-90`: `` `${inventoryDisplayName({name,title})} [${id}] — ${state ?? 'archetype'}, ${tier}` ``
  (`inventoryDisplayName` is the keystone's `resolve.ts:115`).
- `loadToolInventory` `:94` → `loadInventoryContext(repos, characterId, chatId)`;
  `loadToolChat` `:98-100` (`repos.chats.findById` cast to `EquipChat`).
- `characterChangeContext` `:104-110`: `{ userId, provenance: 'character',
  announce: true, ...(pending ? { pendingInventoryAnnouncements } : {}) }`.
- `resolveToolRef(ctx, ref, opts)` `:114-119` → `resolveInventoryRef(ref,
  ctx.pool, ctx.equippedIds, opts)`.
- ⟨102a⟩ `closedShelfMessage(ctx, ref, {archetypesOnly?})` `:127-132`: when
  `ctx.closedShelf` is non-empty, resolve the ref over a pool of ONLY the
  closed-shelf items (`owns: () => false`, `includeArchetypes: true`), filter
  to archetypes when asked, and answer
  `new InventoryOffRosterError([inventoryLabel(first.item)]).message`
  (`errors.ts:44-49`: `` `${names.map(n => `"${n}"`).join(', ')} ${len === 1 ? 'is' : 'are'} on the project's shelf, and you are not on this project's character roster. Ask the user to add you to the roster in the project's Characters card.` ``,
  code `OFF_ROSTER`, status 409).
- `inventoryNotFoundMessage(ref)` `:136-138`: `` `No object "${ref}" is in your inventory or within reach. Use inventory_list to see what there is.` ``
- `sharedInventoryItemReadOnlyMessage(item, verb)` `:141-146`:
  `` `"${inventoryLabel(item)}" lives in a shared store — you can carry it but not edit, archive or give it. Only objects in your own vault can be ${verb}.` ``
  (`verb` ∈ `changed` | `archived` | `given`).
- `errorMessageOf(error, fallback)` `:149-152`: an `InventoryRefusal`'s own
  message; any `Error`'s message; else the fallback.
- `logInventoryTool(tool, context, data, level='debug')` `:155-168` — ONE line
  per handler call: `logger[level]('[InventoryTools] <tool>', { context:
  'inventory-tools', chatId, characterId, tool, ...data })`. Refusals and
  invalid input log at `info`; successes at `debug`. Field order:
  `context, chatId, characterId, tool`, then the data keys in the order each
  call spells them (§1.C per handler).

**`inventory-tool-table.ts` (59 lines)** — `run(execute, format)` wraps each
handler `:31-44`: destructures `{ success, error, ...payload }`; on success
`result = { formattedText: format(r), ...payload }`, `error: undefined`; on
failure `result: null`, `error`. `INVENTORY_TOOL_TABLE` `:46-56` in the nine
names' order (list, read, equip, unequip, pick_up, create, update, archive,
give); `INVENTORY_TOOL_NAMES = Object.keys(table)` `:59`.

**`inventory-list-handler.ts` (103)** — invalid input → `{ success:false,
items:[], total_count:0, error:'Invalid input for inventory_list.' }` `:47`
(logged `info`, `{ outcome: 'invalid input' }`). Loop `:55-79` over
`ctx.pool.items` in pool order: skip archived unless `include_archived && own`;
`kind` filter; `tier` filter; `folder` filter (`inFolder` `:33-38`: trim,
strip leading/trailing `/`, lower-case; empty want → true; `have === want ||
have.startsWith(want + '/')`); kits: `kitState(item, equippedItems, byId)`
(keystone `resolve.ts:140`), `equipped = kit === 'all'`; `equipped` filter
EXCLUDES archetypes outright; row fields in the order shown in §1.A;
`description: excerpt(item.description)` `:27-31` (whitespace-collapsed,
trimmed, >140 → first 139 chars + `…`); `kit_state` / `archived: true` only
when set. Success log `{ outcome: 'listed', count, filters: parsed }`; catch
→ `{ error: errorMessageOf(e, 'Failed to list the inventory') }` logged
`{ outcome: 'failed', error }`. `formatInventoryListResults` `:88-103`:
failure `` `Inventory Error: ${error || 'Unknown error'}` ``; empty
`Inventory: nothing matches.`; else header `` `Inventory (${n} object${n === 1 ? '' : 's'}):` ``
and one line per row `` `  ${label} [${id}] (${state}, ${tier})${kit}${folder}${shared}${archived}${description}` ``
with `state` = `archetype — pick up an instance` for archetypes, `kit` =
`` ` [kit: ${kit_state}]` ``, `folder` = `` ` [folder: ${folder}]` ``, `shared`
= ` [shared — read-only]` when `!is_own`, `archived` = ` [archived]`,
`description` = `` ` - ${description}` ``.

**`inventory-read-handler.ts` (98)** — invalid → `Invalid input: ref is
required.` `:32`. Resolve with `{ includeArchetypes: true, includeArchived:
true }`; best match = first equipped ?? first own ?? first `:37`; none →
⟨102a⟩ `closedShelfMessage(ctx, ref) ?? inventoryNotFoundMessage(ref)` `:40`
(logged `info`, `{ refs:[ref], resolved:[], outcome:'not found' }`). Members
`:45-52`: kit `itemIds` mapped to `{ id, name, title, state }` or
`{ id, name:'(unreachable)', title:null, state:null }`. Output `:54-74`
(key order as §1.A; `archetype_title` = `parentName ?? (archetype ? title ||
name : null)` when `parentId`, else null; `metadata: item.metadata ?? {}`;
`has_picture: !!imageFileId`; `members` only for kits). Format `:81-98`:
line 1 `` `${emoji ? emoji + ' ' : ''}${display} [${id}]` ``; line 2
`` `State: ${kind === 'archetype' ? 'archetype (pick up an instance with inventory_pick_up)' : state} · ${tier}${is_own ? '' : ' [shared — read-only]'}` ``;
then optional `Instance of: …`, `Description: …`, `Looks: …`; always
`` `At most ${max_equipped} carried at a time${can_copy ? '' : ' · cannot be copied'}${has_picture ? ' · has a picture' : ''}` ``;
optional `Folder: …`; `Metadata: ${JSON.stringify(metadata)}` when non-empty
(**JS key order of the metadata object**); `Members:` + `` `  ${display} [${id}] (${state ?? 'archetype'})` ``.

**`inventory-equip-handler.ts` (113)** — invalid → `Invalid input: refs must
be a list of one to eight object references.` `:42`; `Promise.all([ctx, chat])`
`:46`; no chat → `This chat could not be found.` `:47`. Per ref `:51-74`:
resolve with `{ includeArchetypes: true }`; none → skipped with ⟨102a⟩
`closedShelfMessage ?? inventoryNotFoundMessage`; `pickForGesture(matches,
'equip')` → dedup push of `item.id`; else first equipped → `already_equipped`;
else an archetype → `` `"${inventoryLabel(archetype.item)}" is an archetype — pick up an instance of it with inventory_pick_up.` ``,
else `` `"${ref}" cannot be carried.` ``. Nothing to equip `:76-80`: logged
`info` `{ refs, resolved:[], outcome:'nothing to equip', skipped }`; with
`already_equipped` non-empty → the success output as is; else
`{ ...failure(skipped reasons joined by ' ' || 'Nothing to equip.'), skipped }`.
Then ONE `equipItems(repos, chat, characterId, ids, characterChangeContext(context))`
`:82` (keystone chokepoint — throws `CAP_EXCEEDED` / `NOT_EQUIPPABLE` /
⟨102a⟩ `OFF_ROSTER`); `equipped` summarized own = `created ∪ pool.owns`;
`created` always own; `alreadyEquipped` ids re-found in the pool; kit-member
skips → `` `a kit member could not be carried (${skip.reason})` `` `:90`.
Success log `{ refs, resolved: ids, outcome:'equipped', equipped, created }`;
catch → `info` `{ refs, outcome:'refused', error }`, `failure(errorMessageOf(e,
'Failed to equip'))`. Format `:105-113`: `Equipped <summary>` /
`  (a new instance was made for a kit: <id>)` / `Already equipped: <summary>` /
`Skipped "<ref>": <reason>`, joined `\n`, or `Nothing changed.`.

**`inventory-unequip-handler.ts` (71)** — same invalid text as equip `:33`;
per ref `pickForGesture(matches, 'unequip') ?? first kit ?? null` `:43`
(a kit sets down its members); none → `not_equipped` push of the RAW ref;
`unequipItems` only when ids non-empty `:50-52`; output
`not_equipped: [...notEquipped, ...result.notEquipped]` `:56` (raw refs
first, then the chokepoint's ids). Log `{ refs, resolved, outcome:'unequipped',
unequipped }`. Format: `Set down <summary>` / `Not carried: "<ref>"` or
`Nothing changed.`. Never fails on state.

**`inventory-pick-up-handler.ts` (85)** — invalid → `Invalid input: ref names
the archetype to pick up.` `:38`; the archetype = first match with
`isArchetype` `:45`; none → ⟨102a⟩ `closedShelfMessage(ctx, ref,
{ archetypesOnly: true }) ?? (matches.length > 0 ? new
InventoryInvalidParentError(`"${label}" is not an archetype; equip it with
inventory_equip instead`).message : notFound)` `:49-52` (the shelf answer
FIRST — "magic bow" may also name the character's own bow); logged `info`
`{ refs:[ref], resolved: every match id, outcome:'refused', error }`.
`pickUpItems(repos, chat, characterId, [archetype.id], ctx, { title, equip })`
`:56-59` (keystone `equip.ts:451`, cap-before-write). Output `:68-73`
(`item: summarize(created, true, equipped)`, `archetype: { id, name, title }`,
`equipped`). Format `:80-85`: `` `Picked up ${summary} — an instance of ${archetypeDisplay}, in your own vault${equipped ? ', equipped' : ', not equipped'}.` ``;
no item → `` `Nothing was picked up from ${archetype}.` ``.

**`inventory-create-handler.ts` (78)** — invalid → `Invalid input: name is
required; see the tool description for the other fields.` `:36`;
`resolveInventoryLocation('character', characterId, repos, userId)` `:40` (no
vault → `You have no vault to keep a new object in.`); the body through
`createInventorySchema.parse` `:42-54` with `kind:'item'`, `name` trimmed,
`title` trimmed-or-null when given, the five optional fields only when
defined, `isArchetype:false, canCopy:false, equippedByDefault:false`;
`createInventoryItem(repos, vault, body)` (keystone); `publishRealtime('inventory')`
`:56`; `resolveItem({...record, origin: vault.origin, mountPointId}, () =>
undefined)` `:57`; `equip` → `equipItems(..., { known: [item] })` `:63` (the
`known` option is `e1872c94f`'s `equip.ts:163` — found even where the pool
cannot yet read the file), `equipped = equipped.some(id) || alreadyEquipped.includes(id)`.
Log `{ refs:[name], resolved:[id], outcome:'created', equipped }`. Format
`:75-78`: `` `Created ${summary} in your own vault${equipped ? ', equipped' : ''}.` ``.

**`inventory-update-handler.ts` (85)** — invalid → `Invalid input: ref and at
least one field to change are required.` `:36`; resolve `{ includeArchetypes:
true, includeArchived: true }`; none → not-found (NO shelf message here);
first own match or `sharedInventoryItemReadOnlyMessage(matches[0].item,
'changed')` `:46` (logged `info` `{ refs, resolved:[first id], outcome:
'refused: shared' }`); `vault.findItem(id)` → the raw record `:49`; patch
`:52-66` in field order `name` (trimmed), `title` (trim || null),
`description`, `imageDescription`, `emoji`, `maxEquipped`, `metadata`
(merged key by key into the RECORD's own metadata; `null` deletes);
`updateInventoryItem(repos, vault, record, patch, { chatId })` `:68`
(keystone); null → `` `Failed to change "${ref}".` ``; `publishRealtime('inventory')`;
`after` = the item re-read from a SECOND `loadToolInventory` `:71`, else
`{ ...match.item, ...patch, metadata }`; `fields = Object.keys(patch)` (patch
key order). Format: `` `Changed ${fields.join(', ')} on ${summary}.` ``.

**`inventory-archive-handler.ts` (83)** — invalid → `Invalid input: ref and
archived (true or false) are required.` `:38`; own-only as update (verb
`archived`); `setInventoryItemArchived(vault, record, archived)` →
`{ changed }` `:54`; when changed: `publishRealtime('inventory')`; archiving →
`dropCarriedEverywhere(repos, characterId, record.id, [chatId]) > 0` →
`publishRealtime('chats', chatId)` `:57-58`; archiving an EQUIPPED one →
`notifyInventoryChanged(repos, { id: chatId }, characterId, { equipped: [],
unequipped: [match.item] }, characterChangeContext(context))` `:60-61` (a
setting-down, announced as one). Log `{ refs, resolved, outcome: changed ?
'changed' : 'unchanged', archived }`. Output `item: summarize(match.item,
true, !archived && match.equipped)`. Format `:77-83`: unchanged →
`` `${summary} was already ${archived ? 'archived' : 'in use'}; nothing changed.` ``;
archived → `` `Archived ${summary}. It is put away — no longer carried or listed — and can be restored.` ``;
restored → `` `Restored ${summary}.` ``.

**`inventory-give-handler.ts` (134)** — invalid → `Invalid input: ref (the
object) and to (the receiver) are required.` `:75`; `refuse(error, resolved)`
`:78-81` logs `info` `{ refs, resolved, to, outcome:'refused', error }`.
Order `:83-117`: chat (`This chat could not be found.`); resolve (default
opts — no archetypes, no archived); none → not-found; not own → read-only
`'given'`; kit → `` `"${label}" is a kit; give its objects one by one.` ``;
`resolveReceiver` `:42-66` over `chat.participants` filtered `characterId &&
status !== 'removed'`: an id match (`toLowerCase`) loads only that character;
a name loads the whole cast in parallel (`Promise.all`); for the first
candidate that matches (by id, or `name.trim().toLowerCase() === wanted`):
`id === giver` → `You cannot give an object to yourself.`; archived →
`` `${name} is archived and cannot receive anything.` ``; else
`{ id, name }`; none → `` `No one named "${to}" is in this chat to receive it.` ``;
source location, destination with `{ ensure: true }`, `source.findItem` (no
record → not-found); no destination → `` `${receiver.name} has no vault to receive it.` ``;
**1.** `transferInventoryItem(repos, { action:'move', source, destination,
record, userId })` `:101-107`; `publishRealtime('inventory')`; **2.**
`dropCarriedEverywhere(repos, characterId, item.id, [chatId])`;
`publishRealtime('chats', chatId)`; **3.** `notifyInventoryChanged(repos,
{ id: chatId }, characterId, { equipped: [], unequipped: [], given: { item:
moved, toName } }, characterChangeContext(context))` `:111-117`. Log
`{ refs, resolved:[item.id], to: receiver.id, outcome:'given' }`. Output
`item: summarize(moved, true, false)`, `recipient`. Catch → `refuse(errorMessageOf(e,
'Failed to give the object'))`. Format `:130-134`:
`` `Handed ${display} [${id}] to ${recipient.name}. It is in their vault now; they may equip it.` ``.

### 1.D `lib/inventory/*` in this slice (the keystone owns `pool`, `equip` (base), `resolve`, `location`, `item-mutations`, `errors` (base), `prompt-section`, `equipped-set`, `routes/*`, `writer.ts`'s `postPascalInventoryChange`)

**`refs.ts` (238 lines; NEW `e1872c94f` + `a53024d3c` `scopeObjects`)** — pure,
client-safe.
- `InventoryState = 'equipped'|'carried'|'reachable'|'absent'`; `INVENTORY_STATES`
  best-first `:28-31`. `ResolvedInventoryPool { items, owns(id) }` `:34-39`;
  `InventoryRefMatch { item, equipped, own }` `:41-45`.
- `byIdCache: WeakMap<pool.items, Map>` `:59-68` (one id map per `items`
  array identity — v5: build per pool, no observable difference).
- `isEquippedIn` `:71-82`: non-kit → `equippedIds.has(id)`; kit → the
  equipped ids mapped to items, `kitState(item, equipped, byId) === 'all'`.
- **`resolveInventoryRef(ref, pool, equippedIds, opts)` `:89-119`:**
  `norm` = `(v ?? '').trim().toLowerCase()`; empty → `[]`; candidates =
  items with `(includeArchetypes || !isArchetype) && (!archivedAt ||
  (includeArchived && pool.owns(id)))`; rules in order `norm(id) === wanted`,
  `!!title && norm(title) === wanted`, `norm(name) === wanted`; **the first
  rule with ANY hit wins (pool-wide)**; matches are `[...own, ...notOwn]`,
  each half in pool order.
- `stateOf(matches)` `:122-128` over carriable (non-archetype, non-archived):
  any equipped → `equipped`; any own → `carried`; any → `reachable`; else
  `absent`.
- `equippedKinCountByName(name, pool, ids)` `:131-139`: equipped, not
  archived, not a kit, `isKin(item, {name})` (keystone — case-insensitive
  name equality). `equippedKinCount(ref, …)` `:142-149`: the FIRST match
  WITH archetypes, then by its name; no match → 0.
- `pickForGesture(matches, action)` `:156-162`: unequip → first equipped;
  equip → first `!equipped && !isArchetype && !archivedAt` (kits allowed).
- `InventoryContext { characterId, chatId, state(ref), equippedKinCount(ref),
  item(ref), scopeObjects(scope), pool?, equippedIds? }` `:169-187`;
  `inventoryContextFrom(characterId, chatId, pool, equippedIds)` `:190-217`:
  `item` = first equipped ?? first own ?? first (DEFAULT opts — no archetype
  ever renders), `scopeObjects` memoised per scope.
- `scopeObjectsOf` `:225-238`: non-archetype, non-kit, non-archived;
  `reachable` = all; `equipped` = in `equippedIds`; `owned` = equipped OR
  `pool.owns`; mapped to `{ name, title: title || name, metadata: metadata ?? {} }`
  (NOTE `||`, not `?.trim() ||` — a whitespace title is kept here).

**`context.ts` (78; NEW + ⟨102a⟩)** — `LoadedInventoryContext extends
InventoryContext { pool, equippedIds, serverPool, closedShelf }` `:27-37`;
`resolvedPoolOf(pool)` `:40-47` = `pool.resolved()` + the holder's own
ARCHIVED records (`pool.own.filter(archivedAt && !seen)` resolved) appended;
`loadInventoryContext(repos, characterId, chatId)` `:50-78`: chat =
`chatId ? repos.chats.findById : null`; `projectId = chat?.projectId ?? null`;
`loadInventoryPool(repos, { characterId, chatId, projectId })`; `equipped =
chat ? readEquippedSet(chat, characterId, serverPool) : []`; ⟨102a⟩ `shelfOpen
= await projectShelfOpenTo(projectId, characterId)`; `closedShelf` = the
project-scope items not carried; `pool` = the full pool, or the pool minus
those; DEBUG `[InventoryContext] Loaded` `{ characterId, chatId, reachable:
pool.items.length, equipped: equippedIds.size, projectShelf: projectId ?
(shelfOpen ? 'open' : 'closed by roster') : 'none', context: 'inventory' }`
`:69-76`.

**`roster-gate.ts` (54; NEW ⟨102a⟩)** — `isCharacterGesture(provenance) =
provenance !== 'operator'` `:25-27`; `projectShelfOpenTo(projectId,
characterId)` `:33-46`: no project → true; `projectRosterAdmits(projectId,
characterId)` (v5 `project_roster_access.rs`); a THROW → WARN
`[InventoryRoster] Roster lookup failed; the project shelf stays closed`
`{ projectId, characterId, context:'inventory', error }` → false;
`isOnClosedShelf(item, carriedIds) = origin?.scope === 'project' &&
!carriedIds.has(id)` `:49-54`.

**`equip.ts` — this slice's hunks on the keystone's file** (`e1872c94f`:
`:163` `known?`, `:187-210` `dropCarriedEverywhere`, `:278` `pool.resolveId(id)
?? known.get(id)`, `:451-534` `pickUpItems`; ⟨102a⟩ `:113-135`
`assertShelfReach` + `NOTHING_CARRIED`, `:319` and `:481` the two call
sites, the `InventoryOffRosterError` import).
- `dropCarriedEverywhere(repos, characterId, itemId, alsoChatIds=[])`
  `:187-210`: `findChatIdsCarryingInventoryId` ∪ `alsoChatIds` (Set, found
  first); per chat `withChatInventoryLock(chatId, () =>
  repos.chats.removeEquippedInventoryId(chatId, characterId, itemId))`;
  DEBUG `[InventoryEquip] Dropped an object from every carried set`
  `{ characterId, itemId, checked, changed, context }`; returns `changed`.
  Never re-enters the lock.
- `assertShelfReach(chat, characterId, items, ctx)` `:113-133`: operator →
  return; `fromShelf = items.filter(isOnClosedShelf(item, NOTHING_CARRIED))`
  (i.e. every project-scope item — carried ones were never planned); none or
  shelf open → return; else INFO `[InventoryEquip] Refused` `{ code:
  'OFF_ROSTER', chatId, characterId, projectId: chat.projectId ?? null,
  itemIds, provenance, context }` and THROW `InventoryOffRosterError(names =
  title || name)`. Called in `equipUnserialized` `:319` BEFORE the cap (on
  the planned items: existing item or the archetype of an instance-to-be),
  and in `pickUpItems` `:481` after the archetype checks, before the cap.
- `pickUpItems(repos, chat, characterId, archetypeIds, ctx, { title?, equip? })`
  `:451-534`: `assertParticipant`; `equip = opts.equip !== false`; inside
  `withChatInventoryLock`: `freshChat`, `poolFor`, `readEquippedSet`; per
  archetype id `pool.resolveId` — not an archetype or archived →
  `InventoryInvalidParentError(archetype && !isArchetype ? `"${title ||
  name}" is not an archetype; equip it instead` : 'The parent must be an
  archetype this character can reach', { archetypeId })` + INFO
  `[InventoryEquip] Refused` `{ code, chatId, characterId, archetypeId,
  context }`; the shelf check; when equipping, the cap over `[...current,
  ...planned]` by `isKin` — `kin + 1 > maxEquipped` → `InventoryCapExceededError(id,
  title || name, maxEquipped)` + INFO `{ code, op:'pick-up', chatId,
  characterId, itemId, cap, context }` (NOTHING written yet); vault via
  `resolveInventoryLocation('character', …)` (none → plain `Error(`Character
  ${id} has no vault to pick an object up into`)`); `instantiate(repos,
  vault, archetype.id, { pool, overrides: { title } })` per archetype (the
  SAME trimmed title for every one); `publishRealtime('inventory')`; equip →
  `setEquippedInventory(chatId, characterId, [...current ids, ...created
  ids])` + `publishRealtime('chats', id)`; DEBUG `[InventoryEquip] Picked up`
  `{ op:'pick-up', chatId, characterId, archetypes, created, equipped,
  context }`; `notifyInventoryChanged(repos, fresh, characterId, { equipped:
  equip ? created : [], unequipped: [], pickedUp: created }, ctx)`; returns
  `{ created, equipped }`.

**`equip-effects.ts` (246; keystone base `4fbdad186` 111 lines + this
slice's +133 / ⟨102a⟩ +18)** — the chokepoint `notifyInventoryChanged`
`:114-167`: nothing changed and no give → return; **a pending map AND
`announce` AND not a give → `recordPendingInventoryChange(pending, who,
change, provenance)` + DEBUG `[InventoryEffects] Inventory change joined the
turn` `{ chatId, characterId, provenance, context }` and RETURN (no avatar
refresh now)**; a pending map AND a give → `strikeFromPendingInventoryChange(pending,
`${chatId}:${characterId}`, item.id)` then fall through; then (try) when
`announce`: `repos.characters.findById` → `describeInventoryChange(name ??
'Someone', change)` → `postPascalInventoryChange({ chatId, content })` when
non-null; ALWAYS `refreshAvatarForAppearance(repos, { userId, chatId,
characterId }, 'inventory-change')` (v5 `outfit_change_effects`'s
`refresh_avatar_for_outfit` twin; the keystone's `4fbdad186` +11 on
`outfit-change-effects.ts`); DEBUG `[InventoryEffects] Inventory change
effects fired` `{ chatId, characterId, announced, provenance, equipped:
labels, unequipped: labels, context }`; catch → WARN `[InventoryEffects]
Inventory change effects failed` `{ chatId, characterId, context, error }`.
- `describeInventoryChange(characterName, change)` `:90-108`: a give →
  `` `*${name} hands ${withArticle(item,'the')} to ${toName}.*` ``; else
  clauses in order `sets down …` (the), `takes up …` (a; EXCLUDING ids in
  `pickedUp`), `picks up …` (a); none → null; `` `*${name} ${clauses.join(' and ')}.*` ``.
  `withArticle` `:70-76`: a trimmed title wins outright; `'the'` →
  `the <name>`; ⟨102a⟩ `looksPlural(name)` (`:59-62`: last whitespace word
  ends in `s` but not `ss`/`us`/`is`, case-insensitive) → `some <name>`;
  else `indefiniteArticle(name)` (`:65-68`: `/^(uni|use|usu|uti|eu|one|once)/i`
  → `a`; `/^[aeiou]/i` → `an`; else `a`) + space + name. `joinList` `:78-82`:
  0/1 → join; 2 → `A and B`; n → `A, B, and C` (Oxford comma).
- `recordPendingInventoryChange` `:174-198`: key `chatId:characterId`; entry
  `{ userId, chatId, characterId, equipped:[], unequipped:[], pickedUp:[],
  provenances:[] }`; provenance appended once in first-seen order; an
  equipped item cancels a pending unequip of the same id else `pushUnique`;
  an unequipped item cancels a pending equip AND removes it from `pickedUp`
  else `pushUnique`; `pickedUp` `pushUnique`.
- `strikeFromPendingInventoryChange` `:206-216` — removes the id from all
  three lists of ONE entry.
- `flushPendingInventoryAnnouncements(context, repos?)` `:223-246`: absent or
  empty → return; snapshot `Array.from(values)`, `pending.clear()` FIRST;
  per entry `notifyInventoryChanged(repos, { id: chatId }, characterId,
  { equipped, unequipped, pickedUp }, { userId, provenance: provenances[0] ??
  'character', announce: true })` — NO pending map in that ctx, so each posts
  now; DEBUG `[InventoryEffects] Drained the turn's Inventory announcements`
  `{ characters: n, provenances: [{ characterId, provenances }], context }`.

**`transfers.ts` (141; NEW — the route body extracted verbatim)** —
`transferInventoryItem(repos, { action, source, destination, record, userId })`
`:54-141`: `locationKey` equality → `InventoryTransferSameLocationError`
(`InventoryShapeError('Source and destination are the same')` `:46-51` — the
route's old `badRequest` text, now a 400 via `inventoryErrorResponse`); copy →
`assertCopyable([pool.resolve(record)], { op:'transfer-copy', itemId })`;
move → `source.writableMountPointId()` first; kits → `assertKitMembers` with
the contextual-reach set; a non-kit move → `findKitsStrandedByMove` minus
self → INFO `[InventoryTransfers] Move refused: it would strand kits`
`{ op, itemId, kits, context }` + `InventoryKitStrandedError`; `planned` =
`{ ...record, id: copy ? randomUUID() : id, folder:
normalizeInventoryFolderPathOrRoot(folder), archivedAt: copy ? null :
archivedAt ?? null, createdAt/updatedAt: copy ? now : record's }`; id
collision at the destination → `InventoryShapeError('An object with that id
already exists at the destination', { itemId })` (**was `badRequest` in the
route** — now a 400 through the error mapper; same status, body shape per
`inventoryErrorResponse`); `carryInventoryItemImages` BEFORE `destination.create`;
move → `source.delete([id])` (`[]` → plain `Error('Failed to remove the
object from its source after the move')` — **was `serverError` 500 in the
route; through `inventoryErrorResponse` a plain Error is still a 500**, measure);
`commitMovedInventoryImages`; INFO `[InventoryTransfers] Inventory object
transferred` `{ userId, action, itemId, resultItemId, from: source.scope, to:
destination.scope, folder, context }` (the route's old prefix was
`[InventoryTransfers v1]` — the LINE TEXT MOVED); returns `{ stored, item }`
resolved at the destination. `app/api/v1/inventory/transfers/route.ts:26-33`
is now `resolveInventoryLocation` ×2 → `transferInventoryItem` →
`successResponse({ item })`.

### 1.E The executor, the builders, the parsers, the prompts

- **`lib/chat/tool-executor.ts`**: import `:81`; `ToolExecutionContext.pendingInventoryAnnouncements?`
  `:206`; `BUILT_IN_TOOLS` gains `...INVENTORY_TOOL_NAMES` after
  `wardrobe_history` `:288` (and the Log's three + `edit_custom_tool` —
  siblings); the arm `:795-807`: `const inventoryTool =
  INVENTORY_TOOL_TABLE[name]`; no `characterId` → `{ toolName, success:false,
  result:null, error:'The inventory tools need a character to act for.' }`;
  else `inventoryTool(arguments, { userId, chatId, characterId, ...(pending ?
  { pendingInventoryAnnouncements } : {}) })` → `{ toolName, success, result,
  error }`. Position: AFTER the wardrobe arm, BEFORE the Log arm.
- **`lib/tools/handlers/run-custom-handler.ts:58`** `pendingInventoryAnnouncements?`
  on `RunCustomToolContext` (threaded by `turnContainersOf`); the
  `characterIds` removal is `cfa0cba29` (sibling).
- **`lib/wardrobe/turn-containers.ts`**: `PendingInventoryChange { userId,
  chatId, characterId, equipped, unequipped, pickedUp, provenances }` `:22-31`;
  the third optional container `:40`; `newWardrobeTurnContainers()` returns
  all three `:46-50`; `turnContainersOf` carries it `:57`.
- **`lib/tools/plugin-tool-builder.ts`**: `BuildToolsOptions extends
  InventoryToolFlags` `:199`; the debug `buildToolsForProvider` log gains
  `...pickInventoryToolFlags(options)` after `wardrobeArchive` `:401`; the
  push `:578-580` `for (const definition of enabledInventoryToolDefinitions(options))`
  AFTER `wardrobe_archive`'s push, BEFORE `submit_final_response`.
- **`lib/services/chat-message/streaming.service.ts`** `buildTools(…,
  canDressThemselves, canCreateOutfits, canManageInventory?, docToolsMode,
  …)` `:180` — a NEW POSITIONAL at index 13 (0-based 12); `:309`
  `...inventoryToolFlags(canManageInventory)` after `wardrobeArchive`.
- **`lib/services/chat-message/pseudo-tool.service.ts`**:
  `TextBlockEnabledToolOptions extends EnabledToolOptions, InventoryToolFlags`
  `:83`; `determineTextBlockToolOptions(…, canCreateOutfits?,
  canManageInventory?)` `:114` (8th positional) → `...inventoryToolFlags(canManageInventory)`
  `:137` LAST in the returned object (after `wardrobeArchive`); both system-
  instruction builders spread `...pickInventoryToolFlags(enabledOptions)`
  after `wardrobeArchive` `:168`, `:259`.
- **`lib/tools/legacy/text-block-prompt.ts`**: `TextBlockPromptOptions extends
  InventoryToolFlags` `:17`; nine blocks `:232-295` pushed AFTER the
  wardrobe-archive block, in flag order (verbatim):
  - `inventoryList`: `\n### List Inventory\nSee the objects you carry and can reach (not clothing). Each shows whether it is equipped, carried in your vault, or reachable in a shared store; archetypes can be picked up.\n[[LIST_INVENTORY /]]\nOnly what you have equipped: [[LIST_INVENTORY equipped="true" /]]`
  - `inventoryRead`: `\n### Read an Object\n[[READ_INVENTORY ref="Excalibur" /]]`
  - `inventoryEquip`: `\n### Equip an Object\nTake up an object (or kit) by id, title or name; several separated by commas.\n[[EQUIP_ITEM ref="Excalibur" /]]\n[[EQUIP_ITEM refs="lantern,signet ring" /]]`
  - `inventoryUnequip`: `\n### Unequip an Object\nSet down an object you carry.\n[[UNEQUIP_ITEM ref="lantern" /]]`
  - `inventoryPickUp`: `\n### Pick Up an Object\nMake an instance of an archetype in your own vault, equipped unless equip="false".\n[[PICK_UP ref="magic bow" /]]\nWith a title of its own: [[PICK_UP ref="magic bow" title="Whisperwind" /]]`
  - `inventoryCreate`: `\n### Create an Object\n[[CREATE_INVENTORY_ITEM name="compass" title="The Navigator" equip="true"]]A brass compass that points toward home[[/CREATE_INVENTORY_ITEM]]`
  - `inventoryUpdate`: `\n### Update an Object\nChange one of your own objects. Only your own can be changed.\n[[UPDATE_INVENTORY_ITEM ref="compass" description="A dented brass compass" /]]`
  - `inventoryArchive`: `\n### Archive an Object\nPut away one of your own objects (archived="false" restores it).\n[[ARCHIVE_INVENTORY_ITEM ref="compass" archived="true" /]]`
  - `inventoryGive`: `\n### Give an Object\nHand one of your own objects to another character in this chat.\n[[GIVE_ITEM ref="lantern" to="Marcus" /]]`
- **`lib/tools/legacy/text-block-parser.ts`**: `TEXT_BLOCK_TOOL_NAME_MAP`
  `:127-146` — 19 aliases: `inventory_list`, `list_inventory`, `inventory` →
  `inventory_list`; `inventory_read`, `read_inventory`; `inventory_equip`,
  `equip_item`; `inventory_unequip`, `unequip_item`; `inventory_pick_up`,
  `pick_up`; `inventory_create`, `create_inventory_item`; `inventory_update`,
  `update_inventory_item`; `inventory_archive`, `archive_inventory_item`;
  `inventory_give`, `give_item`. (NOTE the wardrobe aliases `equip` /
  `unequip` STAY wardrobe — only the `_item` forms are inventory.)
  `PARAM_ALIAS_MAP` `:230-237`: `inventory_read` `{ id, title, name, item →
  ref }`; `inventory_equip` / `inventory_unequip` `{ id, title, name, item →
  ref; items → refs }`; `inventory_pick_up` `{ id, name, item, archetype →
  ref }` (NO `title` — title is a real parameter); `inventory_update` `{ id,
  item → ref; image, looks → image_description; max → max_equipped }` (NO
  `title`/`name` — real parameters); `inventory_archive` `{ id, title, name,
  item → ref }`; `inventory_give` `{ id, title, name, item → ref; recipient,
  target → to }`; `inventory_create` `{ image, looks → image_description;
  max → max_equipped }`. `CONTENT_PARAM_MAP` `inventory_create: 'description'`
  `:251`. Coercion `:371-404`: for `internalName.startsWith('inventory_')`
  ONLY `archived` / `equip` (`'true'`/`'false'` → boolean) and `max_equipped`
  (non-blank numeric → `Number`) convert; EVERY other string stays a string
  (`"42"` titles, `"true"` names); the generic branch is skipped
  (`continue`). Then `:436-444`: `inventory_equip` / `inventory_unequip`
  collapse `ref` + `refs` into `refs: [String(ref), ...(refs string
  split(',') | refs array map String)]`, each trimmed, empties dropped;
  `ref`/`refs` removed from the rest.
- **`lib/tools/simple-json-parser.ts:113-120`** `TOOL_NAME_ALIASES`:
  `list_inventory`, `inventory` → `inventory_list`; `read_inventory`;
  `equip_item`; `unequip_item`; `pick_up`; `give_item` (seven — `create_/
  update_/archive_inventory_item` are NOT aliased here).
- **`lib/tools/simple-json-prompt.ts`**: `SimpleJsonPromptOptions extends
  InventoryToolFlags` `:44`; `:237-239` `renderToolEntry(definition)` per
  enabled definition, after `wardrobeArchive`.
- **`lib/tools/index.ts:640-658`** re-exports (barrel; v5 has no barrel).

### 1.F The Pascal grammar (`lib/pascal/*`) — `e1872c94f` + `a53024d3c`'s aggregate half

**`custom-tool.types.ts` (2,114 lines at the pin)**
- `MAX_INVENTORY_REF_LENGTH = 200` `:175`. `InventoryRefSchema` `:188-203`:
  `z.strictObject({ $inventory: z.string().min(1).max(320).refine(path =>
  splitInventoryReference(path)?.field === 'metadata', { message: 'must be
  "<object>.metadata.<key>", e.g. "sword.metadata.damage"' }).describe('"<object>.metadata.<key>": the object by id, title or name, then one of its metadata keys.'),
  fallback: z.union([z.number().finite(), z.string(), z.boolean()]).describe('Required. Used whenever the object or key is absent or holds a value of a different type.') })`.
  `isInventoryRef(v)` `:205-207` = object with a `$inventory` key.
- `NumberOrParamRefSchema` `:210` = `[number.finite, ParamRef, StateRef,
  InventoryRef]` (union ORDER — a `$inventory` object is tried LAST, after
  `$state`'s strict object rejects it); `CustomToolParameterSchema.default`
  union gains `InventoryRefSchema` and its describe reads `… May be a $state
  or $inventory reference whose fallback matches this type.`; the default-type
  check reads `.fallback` for both refs. `RollSchema` `:318` =
  `[RollDice, RollRange, InventoryRef]`. `NumberOperandSchema` `:340`,
  `AnyOperandSchema` `:347-354`, `StringOperandSchema` `:363` each gain
  `InventoryRefSchema` LAST.
- `InventoryStateValueSchema` `:515` enum of the four states;
  `InventoryKeySchema` `:523` `string.min(1).max(200)`;
  `InventoryStateComparatorSchema` `:530-536` strict `{ eq?: state | ParamRef,
  neq?: … }` refine `must test something: `eq` or `neq` against equipped,
  carried, reachable or absent`; `GateInventoryStateComparatorSchema`
  `:542-548` literal states, same message; `GateInventoryCountComparatorSchema`
  `:552-560` strict six numeric keys, refine `hasComparator` → `AT_LEAST_ONE`
  (`must specify at least one comparator (gt, gte, lt, lte, eq, neq)`).
- `ToolGateGroupSchema` `:598-636` (the any-of sibling renamed it; the
  inventory keys are THIS slice): `inventory?: record(InventoryKey,
  GateInventoryStateComparator)` describe `Test an object's state for the
  invoking character in this chat, keyed by a LITERAL object reference (id,
  title or name) — e.g. { "sword": { "eq": "equipped" } }. With no character
  or chat, every test declines.`; `inventoryCount?` describe `Test how many
  kin (same name) of an object the invoking character has equipped in this
  chat.`; `inventoryAggregate?: record(AggregateKeySchema, GateComparator)`
  describe `INVENTORY_AGGREGATE_DESCRIPTION` `:507-512` (verbatim: `Test one
  attribute across every object in a scope of the rolling character's bag,
  keyed "<scope>.<field>": scope equipped, owned (own tier, equipped or not)
  or reachable (everything they can see); field count, names, titles,
  metadata.<key> (a list of the objects' values) or
  metadata.<key>.min/.max/.sum/.count (a number). A list compares entry by
  entry: eq holds when any entry equals the operand, neq when none does,
  contains / ncontains likewise; ordering needs a fold. With no character or
  chat, every test declines.`); the refine counts all five records → `must
  test at least one metadata key, progress field, inventory object, or
  inventory aggregate` `:636`. `AggregateKeySchema` `:502-505` =
  `z.string().superRefine(parseAggregateKey → addIssue(reason))`.
- `WHEN_SUBJECTS_SHAPE` `:665-702`: `inventory?: record(InventoryKey,
  InventoryStateComparator)` describe `Test an object's state for the rolling
  character in this chat — equipped, carried (own vault), reachable (a shared
  store) or absent — keyed by object reference (a template). A run nobody
  made declines every test.`; `inventoryCount?: record(InventoryKey,
  NumericComparator)` (same describe as the gate's); `inventoryAggregate?:
  record(AggregateKey, MetadataComparator)`. `testsSomething` `:705-720`
  counts the three records (non-empty) before `hasComparator`/`roll`/`llm`/
  `params`/`metadata`/`progress`. The two "must test something" sentences at
  `:757` and `:839` (quoted in headline 5's neighbour — they END in `,
  `inventory`, `inventoryCount`, or `inventoryAggregate``).
- `MAX_INVENTORY_EFFECT_ITEMS = 8` `:900`; `InventoryEffectActionSchema`
  `:903` enum `equip | unequip | pick_up`; `CustomToolInventoryGestureSchema`
  `:914-926` strict `{ action (describe `"equip" takes up the listed objects
  (or kits); "unequip" sets them down; "pick_up" makes an instance of each
  listed archetype in the rolling character's vault.`), items: array(string
  1..200).min(1).max(8) (describe `The objects, each by id, title or name
  (case-insensitive). A template: {{params.x}} and the other placeholder
  families are substituted before lookup. pick_up names archetypes.`),
  equip?: boolean (describe `pick_up only. Equip the new instances. Default
  true.`) }`. `CustomToolEffectSchema` `:943-1010` gains `inventory?:
  CustomToolInventoryGestureSchema` describe `An Inventory gesture on the
  rolling character instead of a write: equip, unequip or pick up objects.`;
  the superRefine counts kinds `[isWrite, isWardrobe, isInventory, isLog]`
  (the Log sibling's fourth) — `> 1` → `an effect is exactly one of a write
  (target + value), a wardrobe gesture, an inventory gesture, or a log line`
  `:1000`; `0` → `an effect needs target + value, wardrobe, inventory, or
  log` `:1003`. `isWriteEffect` / `isWardrobeEffect` / `isInventoryEffect`
  `:1028-1060` each require the OTHER keys undefined. `inventoryGestureProblems`
  `:1075-1085`: `items` empty → `${action} needs at least one object`
  (field `items`); `equip` defined on a non-`pick_up` → `equip is only used
  with pick_up` (field `equip`).
- `QtapCustomToolSchema.effects` describe `Side effects applied after the
  run: writes into tiered persistent state or the rolling character's
  metadata, wardrobe gestures that change what the rolling character is
  wearing, or inventory gestures that change what they carry.` (the Log
  sibling may have extended it further — measure at the pin).
- `validateGates` `:1502-1535`: per clause, per group (`gateGroups`, index in
  the path only for an array), per subject `inventory` / `inventoryCount`, a
  key containing `{{` → `a gate's ${subject} key must be a literal object
  reference; "${key}" carries a placeholder, and a gate has no parameters to
  fill it` at path `[clause, (g,) subject, key]`.
- `validateWhenSubjects` `:1615-1745`: `inventoryAggregate` →
  `validateMetadataOperands` per key (operand refs only); `inventory` →
  `validateInventoryKey` (placeholders in the KEY: `params.x` undeclared →
  `references undeclared parameter "x"`; `{{llm}}` without a block →
  `references {{llm}}, but the tool declares no `llm` block`) then for
  `eq`/`neq` a `$param` → undeclared → `references undeclared parameter
  "${p}"` at `[...path,'inventory',key,op]`; non-string type → `${op} compares
  an inventory state with parameter "${p}", which is ${type} rather than a
  string`; `inventoryCount` → key check + `validateComparator(declared,
  comparator, 'number', `the equipped count of "${key}"`, …)`.
- `validateEffects` `:1746-1800`: `effect.inventory` → the gesture problems
  at `[effects, i, 'inventory', field]`, then `validateInventoryKey` per item
  at `[..., 'inventory', 'items', m]`; return (no write checks).
- `validateRollRefs` `:1891-1930`: a whole-roll `$inventory` → fallback must
  be a string that `parseDiceNotation` accepts, else `a roll drawn from
  $inventory needs a dice-notation fallback, like "1d4"` at `['roll',
  'fallback']`; a field-level `$state`/`$inventory` with a non-number
  fallback → `roll.${field} uses a ${'$state' | '$inventory'} reference whose
  fallback is ${typeof} rather than a number` (the `$state` sentence CHANGED
  shape — it used to be the literal `$state`; v5 `custom_tool_types.rs:3714`
  hard-codes `uses a $state reference` and must become the two-way form).
- `resolveOperandType` reads `.fallback`'s type for `$inventory` as for
  `$state`.
- `formatDefinitionIssues` / `flattenIssues` `:2060-2094`: the any-of
  sibling's `isNotAnArray` filter AND the `invalid_key` arm (`:2086` — a
  record KEY's own refinement message, i.e. the aggregate-key parser's
  `reason` and the progress key's, replaces `Invalid key in record`). **The
  `invalid_key` arm is load-bearing for THIS slice's aggregate keys** — a
  shared contract (§5).

**`placeholders.ts`** — `PlaceholderRef` gains `{ kind:'inventory', ref,
field: InventoryPlaceholderField, key? }` `:35`; `InventoryPlaceholderField
= 'title'|'name'|'description'|'state'|'metadata'` `:49`;
`splitInventoryReference(rest)` `:63-78`: the FIRST `.metadata.` splits
(both halves non-empty) → `{ ref, field:'metadata', key }`; else a trailing
`.title`/`.name`/`.description`/`.state` with something before it → `{ ref,
field }`; else null. `classifyPlaceholder` `:133-136`: `inventory.` prefix →
split, or fall through to `unknown`.

**`inventory-subjects.ts` (123; NEW)** — pure, client-safe.
`INVENTORY_STATE_VALUES` `:34`; `parseInventoryOperand(path)` `:37-41` (the
split, metadata only); `resolveInventoryValue(operand, ctx)` `:49-60`: null
ctx → fallback; unparseable → fallback; `ctx.item(ref)?.metadata?.[key]`;
`typeof value !== typeof fallback` → fallback; a non-finite number →
fallback; else the value. `inventoryStateHolds(test, ref, ctx, onDecline?)`
`:72-86`: null ctx → `onDecline('no inventory to read (nobody in particular
rolled, or the vault could not be read)')` → false; `state = ctx.state(ref)`;
`eq` mismatch → false; `neq` match → false; else true (NOTE: NO decline
reason is reported on an ordinary mismatch). `inventoryCountHolds` `:92-110`:
null ctx → `onDecline('no inventory to count (…)')`; `count =
ctx.equippedKinCount(ref)`; the four orderings via `compareOrdered`, then
`eq` / `neq`. `gateNamesInventory(gate)` `:113-123` (any group names
`inventory` / `inventoryCount` / `inventoryAggregate`).

**`inventory-aggregate.ts` (208; NEW `a53024d3c`)** — pure. `AGGREGATE_SCOPES
= ['equipped','owned','reachable']`, `AGGREGATE_FOLDS = ['min','max','sum','count']`;
`parseAggregateKey(key)` `:82-112`: `dot = indexOf('.')`, `dot <= 0` →
`` `"${key}" must be <scope>.<field>, where the scope is equipped, owned, reachable` ``;
bad scope → `` `"${scope}" is not a scope; use equipped, owned, reachable` ``;
rest `count`/`names`/`titles`; `metadata.<path>`: the LAST segment a fold word
WITH something before it → `{ kind:'metadata', key: before, fold }`, else
`{ kind:'metadata', key: path }` (so `metadata.a.b` is key `a.b`, and
`metadata.stats.max` folds `stats`); else `` `"${rest}" is not a field; use count, names, titles, or metadata.<key> (optionally ending .min, .max, .sum or .count)` ``.
`isListAggregate` `:115-118` (names, titles, un-folded metadata);
`aggregateKeyEndsInFold` `:125` (Workbench warning — SPA sibling);
`aggregateValue(parsed, objects)` `:144-171`: count → `objects.length`;
names/titles → the lists; metadata → per object with `hasOwnProperty(key)`,
`entriesOf(value)` = a primitive → `[it]`, an array → its primitives, else
`[]`; no fold → `flat()`; `count` fold → how many objects have ≥1 FINITE
number; `min`/`max`/`sum` over the finite numbers, NONE → `undefined`
(absent). `inventoryAggregateHolds(comparator, key, ctx, hooks)` `:178-195`:
null ctx → `hooks.onDecline('no inventory to aggregate (nobody in particular
rolled, or the vault could not be read)')` → false; a bad key →
`onDecline(reason)`; `metadataComparatorHolds(comparator, key, value ===
undefined ? {} : { [key]: value }, hooks)` — i.e. an absent fold declines as
`the character has no such metadata key`. `describeAggregateValue` `:198-208`
for the debug line: null ctx → null; bad key → null; absent → `'absent'`; a
list → `` `list of ${n}` ``; else the number.

**`metadata-match.ts` hunk (`a53024d3c`, SHARED with the any-of half — see
§5)** — `metadataComparatorHolds` `:95-96`: an array whose every entry
`isPrimitive` → `listComparatorHolds` `:137-172`: any ordering key →
`` `${k} orders ${key}, which is a list; fold it with .min/.max/.sum/.count first` ``
(declines); `eq` → `entries.some(=== operand)`; `neq` → `!entries.some`;
`contains`/`ncontains` → operand must be a string (`` `${k} looks for ${JSON.stringify(operand)}, and only a string can be a substring` ``),
held = any STRING entry `includes(operand)`, `contains` wants held,
`ncontains` wants !held; keys AND. A non-primitive array still declines whole
with the reworded `the key holds an array that is not all numbers, strings
and booleans, which cannot be compared` `:100`.

**`tool-gate.ts`** — `evaluateToolGate(definition, metadata, progress?,
inventory?)` `:92-129` (4th positional; `bag = inventory ?? null`); `gateHolds`
`:142-181` after metadata/progress: `inventory` → `inventoryStateHolds(comparator,
ref, inventory)` (NO decline hook — the gate logs nothing); `inventoryCount`
→ `inventoryCountHolds`; `inventoryAggregate` → `inventoryAggregateHolds(…,
{ resolveOperand: k => comparator[k] as Primitive })` (literal operands, no
decline hook). `heldGroupOf` / `heldGroup` are the any-of sibling's.

**`custom-tools.ts`** (2,221 lines at the pin) —
- `RosterContext.inventory?: InventoryContext | null` `:212`;
  `loadInvokerInventory(ctx)` `:485-519`: `ctx.inventory !== undefined` → it;
  no `characterId` or `chatId` → DEBUG `Custom-tool gates: no character or
  chat to read an inventory for; inventory tests decline` `{ context,
  chatId: ctx.chatId || null, characterId: ctx.characterId ?? null }` → null;
  `loadInventoryContext(getRepositories(), characterId, chatId)` → DEBUG
  `Custom-tool gates: loaded the invoker’s inventory` `{ context, chatId,
  characterId, reachable, equipped }` (the apostrophe is U+2019); catch →
  DEBUG `Custom-tool gates: the invoker’s inventory could not be read;
  inventory tests decline` `{ context, chatId, characterId, error }` → null.
  `invokerInventory` thunk `:556-559` (`bag ??= …`, loaded ONCE), passed to
  `evaluateToolGate` ONLY when `gateNamesInventory(availableWhen) ||
  gateNamesInventory(withheldWhen)` `:586-588`, else `undefined`. ORDER of
  reads for a gated definition: metadata (always), progress (when named),
  inventory (when named).
- `resolveParams(definition, supplied, state, inventory?)` `:791`;
  `coerceParam` `:816-830`: an omitted parameter's default — `$state` →
  `resolveStateValue`; `$inventory` → `resolveInventoryValue(default,
  inventory)`; else the literal.
- `resolveRollField` `:866-900`: a `$inventory` spec → `resolveInventoryValue`;
  a non-finite result → `CustomToolRunError(`${tool}: roll.${field}
  $inventory reference resolved to ${JSON.stringify(value)} rather than a
  finite number`)` (unreachable through the schema — the fallback is
  load-typed as a number; keep as a regression guard). `rollRange` threads
  `inventory` into all four fields.
- `OutcomeSubjects.inventory?` `:1011`; `resolveOperand(…, inventory?)`
  `:1025-1045`: `$state` → state; `$inventory` → `resolveInventoryValue`;
  then `$param`. `matchesComparator` / `matchesMetadataComparator` (8th arg
  after the `subject` label) / `matchesLlmComparator` thread it.
- `matchesWhenGroup` `:1269-1416` (the any-of sibling split `matchesWhen`):
  after `progress`, the inventory block `:1308-1357`: `inventoryKeys` =
  keys of both records; when any, `renderVars = { ...subjects, dice:
  subjects.dice ?? '' }`; per `inventory` key: `ref = renderTemplate(rawKey,
  renderVars).trim()`; `operandFor(op)` = `String(resolveOperand(…, `inventory
  "${rawKey}" ${op}`, …))`; `holds = inventoryStateHolds({eq, neq}, ref,
  inventory, decline(rawKey))` where `decline` logs DEBUG `Custom tool
  inventory test did not match` `{ context, tool, key, reason }`; ALWAYS DEBUG
  `Custom tool inventory state test` `{ context, tool, key: rawKey, ref,
  state: inventory ? inventory.state(ref) : null, holds }`; false → return
  false. Per `inventoryCount` key: render; `test[op] = requireNumber(tool,
  resolveOperand(…, `inventoryCount "${rawKey}" ${op}`, …), label)`;
  `inventoryCountHolds`; DEBUG `Custom tool inventory count test` `{ context,
  tool, key, ref, count: inventory ? inventory.equippedKinCount(ref) : null,
  holds }`. Then the aggregate block `:1359-1381`: per key
  `inventoryAggregateHolds(comparator, key, inventory, { resolveOperand: k =>
  resolveOperand(tool, comparator[k], params, `inventoryAggregate "${key}"
  ${k}`, state, inventory), onDecline: reason => DEBUG 'Custom tool
  inventoryAggregate test did not match' { context, tool, key, reason } })`;
  ALWAYS DEBUG `Custom tool inventoryAggregate test` `{ context, tool, key,
  value: describeAggregateValue(key, inventory), holds }`. Then `llm`.
- `ResolvedEffect` gains `{ index, inventory: ResolvedInventoryGesture }`
  `:1446-1452`; `ResolvedInventoryGesture { action, items: string[], equip? }`
  `:1454-1461`; `isApplicableInventoryEffect` `:1486`; `isApplicableEffect`
  ORs it `:1500-1504`; `resolveEffects` `:1522` dispatches `isInventoryEffect`
  → `resolveInventoryGesture` `:1631-1660`: each item `renderTemplate(raw,
  subjects).trim()`; empty → DEBUG `Custom tool inventory reference rendered
  empty; dropped` `{ context, tool, effectIndex, reference: raw }` and
  skipped; none left → `skipped(index, 'every object reference rendered
  empty')`; result `{ index, inventory: { action, items, ...(pick_up ?
  { equip: gesture.equip !== false } : {}) } }` — `equip` is PRESENT only on
  `pick_up`, normalised to a boolean. The skip reason for a malformed effect
  reads `effect is neither a complete write nor a wardrobe or inventory
  gesture` (the Log sibling may have reworded — measure).
- `PlaceholderSubjects.inventory?` `:1720`; `resolvePlaceholderValue`
  `:1756-1773` `case 'inventory'`: no ctx → undefined; `state` →
  `ctx.state(ref)`; `item = ctx.item(ref)` null → undefined; `title` →
  `title?.trim() || name`; `name`; `description` → `description ?? undefined`;
  `metadata` → `item.metadata?.[key]` (the renderer's primitive rule then
  applies). `renderTemplate`'s unrendered-debug `:1831-1840` `Custom tool
  message references an inventory object it cannot render` `{ context,
  placeholder: whole, reason }` with reason `no inventory to read (nobody in
  particular rolled)` | `the object is absent, or has no such field or
  metadata key` | `the metadata key does not hold a primitive`.
- `prepareRoll(definition, inventory?)` `:1910-1940`: a `$inventory` roll →
  `resolveInventoryValue`; a string that parses as dice → `{ form:'dice',
  notation: resolved, parsed }`; else DEBUG `Custom tool $inventory roll fell
  back` `{ context, tool, reference: $inventory, resolved, fallback }` and
  the fallback parsed (not dice → `CustomToolRunError(`${tool}: the $inventory
  roll's fallback is not dice notation this build can roll`)`). `drawRoll`
  threads it. `executeCustomTool(definition, params, overrides)` `:1982`:
  `inventory = overrides?.inventory ?? null` → `resolveParams`, `prepareRoll`,
  `drawRoll`, the llm prompt render, the subjects, the message, the
  chipLabel render all receive it. `simulateOutcomes(definition, params,
  runs, metadata, llm, state, progress = {}, inventory = null)` `:2162`.

**`tool-vocabulary.ts`** — `ToolVocabulary` gains, AFTER `clearsSlots` (wire
order): `inventoryReads: string[]` `:114`, `inventoryAggregates` `:122`
(`a53024d3c`, between `inventoryReads` and `equips`), `equips` `:124`,
`unequips` `:126`, `picksUp` `:128`. `isEmptyVocabulary` ANDs all five.
`collectToolVocabulary` `:163-300`: `collectGroup` `:192-199` adds `metadata`
keys, `progress` ids, `inventory` + `inventoryCount` keys → `found.inventory`,
`inventoryAggregate` keys → `found.inventoryAggregates`, for every outcome
group, every gate group, every effect-condition group; an `inventory` effect
→ `collectPlaceholders(item)` per item, then the items into
`equips`/`unequips`/`picksUp` by action, `continue` (skips the wardrobe arm);
`collectPlaceholders` `case 'inventory': found.inventory.add(ref.ref)` `:365`;
`collectStateRefs(node, into, inventory)` `:379-395`: a `$inventory` object →
`parseInventoryOperand(node.$inventory)` → `inventory.add(parsed.ref)` and
RETURN (not walked further). All sorted (`sorted` = JS default sort — UTF-16
code-unit order; v5's existing `add`/`sorted` twins already pin this).

**`side-effects.ts`** — `ApplyCustomToolEffectsParams` gains
`pendingInventoryAnnouncements?` `:162` and `inventory?: InventoryContext |
null` `:164`; `applyCustomToolEffects` `:174-216`: writes, then the wardrobe
pass, then `inventoryGestures = effects.filter(isApplicableInventoryEffect)`;
`carried = length ? await applyInventoryEffects({ userId, chatId, toolName,
characterId, gestures, inventory: params.inventory, ...(pending ?
{ pendingInventoryAnnouncements } : {}) }) : []` `:196-208`; the early
return when `changes.length === 0 && carried.length === 0` (and the Log's
`logged`); merge `[...writes, ...changes, ...carried, ...logged].sort((a,b) =>
a.index - b.index)` `:215` (JS stable sort). `AppliedEffect.target` doc
gains `"inventory.equip" / "inventory.unequip" / "inventory.pick_up" (the
equipped ids before, and an InventoryEffectRecord)`.

**`inventory-effects.ts` (228; NEW)** — `InventoryEffectTarget` the three
strings `:49`; `InventoryEffectRecord { item: { id, title }, created?: true,
equipped: string[] }` `:52-58`; `applyInventoryEffects(params)` `:82-228`:
no gestures → `[]`; no `characterId` → DEBUG `Custom tool: no rolling
character, inventory effects skipped` `{ context:'pascal.inventory-effects',
chatId, tool, gestures: n }` → `[]`; `ctx = isLoaded(params.inventory) ?
params.inventory : await loadInventoryContext(…)` and `chat =
repos.chats.findById(chatId)` (try) → WARN `Custom tool inventory effects
skipped — the inventory could not be read` `{ context, chatId, tool,
characterId, error }` → `[]`; no chat → WARN `Custom tool inventory effects
skipped — no such chat` `{ context, chatId, tool }` → `[]`; `changeCtx =
{ userId, provenance:'pascal', announce:true, ...(pending ? {…} : {}) }`;
`skip(index, reason, detail)` → DEBUG `Custom tool inventory effect not
applied` `{ context, chatId, tool, characterId, effectIndex, reason,
...detail }`; `land(index, target, previous, items, after, created)` pushes
`{ index, applied: { target, previous, next: { item: { id, title:
inventoryLabel(item) }, ...(created ? { created: true } : {}), equipped:
after } } }` per item. Per gesture: `before = Array.from(ctx.equippedIds).sort()`;
**pick_up**: per ref `resolveInventoryRef(ref, pool, ids, { includeArchetypes:
true }).find(isArchetype)` else `skip(index, 'no reachable archetype by that
reference', { reference })`; none → `continue` (no re-read); `pickUpItems(repos,
chat, characterId, archetypeIds, changeCtx, { equip })`; `after = equip ?
[...before, ...created ids].sort() : before`; `land('inventory.pick_up',
before, created, after, createdSet)`. **equip/unequip**: per ref
`resolveInventoryRef(ref, pool, ids)` (no archetypes); `pick = equip ?
pickForGesture('equip') : pickForGesture('unequip') ?? first kit ?? null`;
none → `skip(index, matches.length === 0 ? 'no object by that reference' :
`nothing to ${action}`, { reference })`; dedup ids; none → `continue`;
`equipItems` → `after = [...before, ...equipped ids].sort()`, `land('inventory.equip',
before, result.equipped, after, createdSet)`; `unequipItems` → `land('inventory.unequip',
before, result.unequipped, before minus gone)`. A THROW (cap, roster, …) →
`skip(index, getErrorMessage(error), { action, items })` and `continue`.
After a LANDED gesture: `ctx = await loadInventoryContext(…)` (re-read; a
throw → WARN `Custom tool inventory effects stopped — the inventory could
not be re-read` `{ context, chatId, tool, error }` and `break`). Finally
DEBUG `Custom tool inventory effects applied` `{ context, chatId, tool,
characterId, applied: targets, announcement: pending ? 'joined the turn' :
'posted now' }`. Returns `IndexedAppliedEffect[]`.

**`run-service.ts`** — `RunCustomToolParams.pendingInventoryAnnouncements?`
`:96`; `:141` `const inventory = await loadRunInventory(definition,
characterId, chatId)` BETWEEN the cascade read and `executeCustomTool`;
`executeCustomTool(…, { inventory, private, metadata, progress, … })`;
`applyCustomToolEffects({ …, inventory, ...turnContainersOf(params) })`;
`loadRunInventory` `:282-310`: no `characterId` → null; `collectToolVocabulary(definition)`;
`needed = inventoryReads + inventoryAggregates + equips + unequips + picksUp
> 0` else null (NO read); try `loadInventoryContext(getRepositories(),
characterId, chatId)`; catch → WARN `Custom tool run: the rolling
character's inventory could not be read; inventory tests decline` `{ context,
chatId, characterId, tool, error }` → null.

**`workbench.ts`** — `CustomToolLibraryEntry.rollForm: 'range' | 'dice' |
'inventory'` `:41`; `:194-199` `typeof roll === 'string' ? 'dice' : roll &&
'$inventory' in roll ? 'inventory' : 'range'`.

**`bench-inventory.ts` (78; NEW + `a53024d3c` metadata rows)** —
`MockInventoryRowSchema` `:32-37` strict `{ ref: string 1..200, state: enum,
count?: int 0..999, metadata?: record(string, primitive | primitive[]) }`
(`MockPrimitiveSchema` = `number.finite | string | boolean`);
`MockInventorySchema = array.max(64)` `:41`; `mockInventoryContext(rows)`
`:44-78`: keyed by `ref.trim().toLowerCase()`, LATER rows win, blank refs
skipped; `count ?? (equipped ? 1 : 0)`; `{ characterId:'bench', chatId:null,
state: lookup?.state ?? 'absent', equippedKinCount: lookup?.count ?? 0,
item: () => null, scopeObjects }` — scope membership `equipped` → {equipped};
`owned` → {equipped, carried}; `reachable` → + reachable; an equipped row
repeats `count` times, others once; each `{ name: ref, title: ref, metadata }`.
NO `pool` / `equippedIds` keys (so `inventory-effects.ts`'s `isLoaded` is
false for a mock — irrelevant, the bench never applies).

**`app/api/v1/custom-tools/route.ts`** — `PreviewBodySchema.inventory:
MockInventorySchema.nullish()` `:106` (after `state`, before `llm`);
`AuditBodySchema.inventory` `:117`; preview `:265` `inventory = body.inventory
? mockInventoryContext(body.inventory) : null` → `executeCustomTool(…,
{ private, inventory, metadata, progress, now, … })` and `gate:
evaluateToolGate(definition, metadata, progress, inventory)` `:289`; audit
`:322-334` `simulateOutcomes(definition, params, AUDIT_RUNS, metadata, llm,
state, flattenProgressions(metadata, Date.now()), body.inventory ?
mockInventoryContext(body.inventory) : null)`. NO new response key — the
inventory never appears in the body (unlike `wardrobeResolution`).

**`lib/tools/run-custom-tool.ts`** — the preamble sentence `:98` (verbatim in
§1 header-level: `Some tools record side effects when they run — adjusting
the scene's persistent state, the rolling character's own records, what they
are wearing, or what they carry, server-side, as part of the roll.`);
`describeRoll` `:108` `` `dice from ${roll.$inventory} (else ${roll.fallback})` ``
for an `$inventory` roll (checked BEFORE the range form); `SCOPE_WORDS`
`:169-173` (`equipped: { possessive: "your equipped objects'", count: 'how
many objects you have equipped' }`, `owned: { "your owned objects'", 'how
many objects you own' }`, `reachable: { "your reachable objects'", 'how
many objects you can reach' }`); `LIST_WORDS` `:176-181` (`eq → includes`,
`neq → includes no`, `contains → has an entry containing`, `ncontains → has
no entry containing`); `describeAggregate(key, comparator)` `:184-199`: a
bad key → `describeComparator(comparator, `your inventory's ${key}`)`; count
→ `describeComparator(comparator, words.count)`; subject = metadata fold
`count` → `` `how many of ${possessive minus trailing '} carry a numeric metadata.${key}` ``;
other metadata → `` `${possessive} metadata.${key}${fold ? '.' + fold : ''}` ``;
names/titles → `` `${possessive} ${kind}` ``; a non-list → `describeComparator`;
a list → one clause per present list key `` `${subject} ${verb} ${describeOperand(operand)}` ``.
`describeWhenGroup` `:203-224` appends, after `llm`: `inventory` →
`` `your ${ref} is ${describeOperand(eq)}` `` / `` `your ${ref} is not ${describeOperand(neq)}` ``;
`inventoryCount` → `describeComparator(comparator, `your equipped ${ref}
count`)`; `inventoryAggregate` → `describeAggregate`. `buildRunCustomDescription`
`:289-296`: after the `Side effects:` line, `` `    Reads: your inventory for ${reads.join(', ')}` ``
when `inventoryReads` non-empty, then `` `    Takes stock of: ${inventoryAggregates.join(', ')}` ``
when non-empty, then the outcome rows. `describeEffects` `:334-357`: after
the wardrobe clauses, per inventory effect `` `${equips|unequips|picks up} ${items.join(', ')}` ``
(verb by action), dedup; then the Log's `writes to the log`. NOTE `Reads:` is
emitted REGARDLESS of `revealOdds` — it sits between `Side effects:` and the
outcomes; measure whether the `revealOdds: false` early return precedes it
(`:280-288`).

### 1.G The off-sites, the tools list, realtime, the Almanack, schemas

- **Brahma** `one-shot.service.ts:103` + `orchestrator.service.ts:218`: a
  13th positional `false,  // canManageInventory — no inventory_* tools here`
  to `buildTools`; `:139` / `:259` `...inventoryToolFlags(false)` in the
  text-block option objects. **Scenario Builder** `:174`, `:213` same two.
  **Carina** `carina.service.ts:643` the positional only (no text-block
  object). **Help chat** `help-chat/orchestrator.service.ts:271` the 8th
  positional `false` to `determineTextBlockToolOptions`.
- **`app/api/v1/tools/route.ts`**: `BUILT_IN_TOOL_SCHEMAS` gains the nine
  `:110` (after `wardrobe_history`, before `doc_read_file`); `BUILT_IN_TOOLS`
  rows `:343-406` after `wardrobe_history`'s row, before `doc_read_file`:
  `inventory_list` / `List Inventory` / `List the objects the character
  carries and can reach, with their states`; `inventory_read` / `Read
  Inventory Object` / `Read one object in full: descriptions, metadata,
  archetype, kit members`; `inventory_equip` / `Equip Objects` / `Take up
  objects or kits in this chat`; `inventory_unequip` / `Unequip Objects` /
  `Set down carried objects in this chat`; `inventory_pick_up` / `Pick Up
  Object` / `Make an instance of a reachable archetype in the character's
  vault, optionally equipped`; `inventory_create` / `Create Object` / `Make
  a new object of the character's own`; `inventory_update` / `Update Object`
  / `Change one of the character's own objects`; `inventory_archive` /
  `Archive Object` / `Put away (or restore) one of the character's own
  objects`; `inventory_give` / `Give Object` / `Hand one of the character's
  own objects to another participant`; all `source:'built-in', category:
  'inventory'`. `chatContext.canManageInventory` `:594,628,635,656`
  (`character.canManageInventory !== false`, inside the same try as the two
  wardrobe flags — a failed load WARNs `[Tools v1] Failed to load character
  for wardrobe check` and leaves all three true); the availability arm
  `:842-854` for the nine → `available=false`, `unavailableReason =
  'Character does not have inventory management enabled'`.
- **`lib/schemas/realtime.types.ts:50`** `'inventory'` appended LAST to
  `REALTIME_TOPICS` (after `memories`). **`lib/realtime/job-topics.ts`**:
  `TOPIC_ID_FIELDS.inventory: []` `:141`; `INVENTORY_PATH_PATTERN =
  /(^|[\\/"])Inventory\//` `:149`; `writesInventoryFile(write)` `:151-158`:
  `method.startsWith('docMount')` and the pattern tests
  `JSON.stringify(args ?? [])` (try/catch → false); `topicsForWriteBatch`
  `:201-204`: BEFORE the namespace lookup, `!seen.has('inventory') &&
  writesInventoryFile(write)` → `seen.add('inventory')`, `hints.push({ topic:
  'inventory' })` (bare, unscoped). **`topic-map.ts`** is the SPA sibling's.
- **Almanack**: `types.ts:239` `canManageInventory: number` after
  `canCreateOutfits`; `phase3-ledgers.ts:164` zero, `:586` `SUM(CASE WHEN
  "canManageInventory" IS NOT 0 THEN 1 ELSE 0 END) AS inventory` after the
  `outfits` column, `:606` `canManageInventory: num(row?.inventory)`;
  `render.ts:518` `- **May manage their inventory**: N` after `May create
  outfits`, before `With a Core-whisper override`.
- **`lib/schemas/character.types.ts:232`**, **`characters/[id]/handlers/put.ts:77`**
  — the flag's schema/PUT key (keystone's D23 lane; noted for the READ sites
  only). **`lib/schemas/chat.types.ts:542-550`** a doc comment only
  (`InventoryEffectRecord` under `MessageEvent.pascalMeta.effects`).
- **`lib/database/repositories/chats.repository.ts:839-876`**
  `findChatIdsCarryingInventoryId(characterId, itemId)` (a `$like
  '%itemId%'` prefilter on the `equippedInventory` JSON column, then
  `normalizeEquippedInventory(row)[characterId]?.includes(itemId)`; fallback
  `[]` with ERROR `Failed to find chats carrying an inventory id`; DEBUG
  `Chats carrying an inventory id` `{ characterId, itemId, chats, context }`)
  and `removeEquippedInventoryId(chatId, characterId, itemId)` (read-modify-
  write of the whole map, `false` when absent/not listed; fallback `false`
  with `Failed to remove an id from equipped inventory`; DEBUG `Equipped
  inventory id removed` `{ chatId, characterId, itemId, written, context }`).
  **These two are repository methods on the keystone's `equippedInventory`
  column — Shared contract (§5): the keystone owns the column + its repo
  module; this slice needs these two methods, so they are named here as the
  keystone's deliverable.**
- **`lib/background-jobs/child/child-repositories-proxy.ts`** (+210):
  the job child's equipped-inventory + database-document read-your-writes
  overlays — **NO-PORT** (v5 has no forked job child; the ledger agrees). Its
  guarantee — a second gesture in one run sees the first — is carried in v5
  by `applyInventoryEffects`'s synchronous re-read on the writer thread.

### 1.H Tests v4 shipped (the oracle's seed corpus; `it` counts at the pin)

| file | `it`s | what |
|---|---|---|
| `__tests__/unit/lib/tools/handlers/inventory-handlers.test.ts` | 18 | list; pick_up ×4 (incl. cap-before-write, the netted turn); equip ×2; own-only ×3; give ×5; the roster ×3 (⟨102a⟩) |
| `__tests__/unit/lib/pascal/custom-tools-inventory.test.ts` | 24 | the `inventory` subject ×4, `inventoryCount` ×2, placeholders ×3, `$inventory` ×3, the gate ×2, lazy roster load ×2, the mock bench ×1, the roster description ×1, any-of + aggregate at run time ×5, aggregate gate in any group ×1 |
| `__tests__/unit/lib/pascal/inventory-effects.test.ts` | 8 | pick-up audit; off-roster pick-up lands nothing; absent ref; cap skips one gesture, next lands; `equip:false`; file-order visibility; joined turn netted; nobody rolled |
| `__tests__/unit/lib/pascal/side-effects-partition.test.ts` | 1 | wardrobe then inventory, merged by index |
| `__tests__/unit/lib/inventory/refs.test.ts` | 12 | `resolveInventoryRef` ×5, `stateOf` ×2, `equippedKinCount` ×2, `pickForGesture` ×2, `inventoryContextFrom` ×1 |
| `__tests__/unit/lib/inventory/equip.test.ts` (+34 / +8) | 13 total | `pickUpItems`, `dropCarriedEverywhere`, the plural/article rule (⟨102a⟩) |
| `__tests__/unit/lib/pascal/inventory-aggregate.test.ts` | 22 | `parseAggregateKey`; real-context scopes ×4; the mock bag ×2; folds ×3; the per-entry rule ×5; `inventoryAggregateHolds` ×3; §8 acceptance ×3; any-of gates ×2 |
| `__tests__/unit/lib/pascal/custom-tool-definition.test.ts` (+76 / +109) | 127 total | `the Inventory subjects, operand and effect` ×6; `inventoryAggregate and any-of` ×5; `no raw reads of a gate or a when` ×1 (a grep test — v5-irrelevant) |
| `__tests__/unit/lib/pascal/tool-vocabulary.test.ts` (+30 / +2) | 22 | `reports inventory reads from tests, gates, placeholders and $inventory, and the three gestures` |
| `__tests__/unit/lib/pascal/tool-gate-keys.test.ts` (+23) | 5 | any-of walk (the `7263105c6` + `a53024d3c` siblings') |
| `__tests__/unit/lib/pascal/custom-tools-execution.test.ts` (+14) | 100 | `compares a list of primitives entry by entry` |
| `lib/tools/legacy/__tests__/text-block-parser.test.ts` (+16) | 40 | `keeps numeric- and boolean-looking text as text`; `still converts the parameters that are numbers or booleans` |
| `__tests__/unit/lib/pascal/tool-draft-inventory.test.ts` | 11 | `lib/pascal/tool-draft.ts` — **SPA sibling** (v5 `apps/web/src/app/pascal/tool-draft.ts`) |
| `__tests__/unit/lib/background-jobs/child-proxy-equipped-inventory.test.ts` | 5 | NO-PORT with its module |
| `__tests__/unit/context-management.test.ts` (+23) | 94 | `carries the Inventory on a continue-mode turn (a chained speaker, a nudge, an autonomous room)` (bug 253); `reports progressions on a continue-mode turn (bug 254)` |
| `__tests__/unit/lib/services/chat-message/tool-execution.service.test.ts` (+2) | 30 | the new container in `createToolContext` |
| `__tests__/unit/lib/pascal/custom-tools-measure-twice.test.ts` (`cad017599`) | 7 | §7 |
| `__tests__/helpers/inventory-fake-store.ts` (+11 / +11) | — | the keystone's in-memory store (the Inventory jest fixture); `addCharacter` / `state.chats` |
| `lib/tools/__tests__/tool-definitions-snapshot.test.ts` (+18) | 2 | nine `ALL_TOOLS` keys inserted between `imageGeneration` and `keepImage`; the `.snap` +310 |
| the Brahma / Scenario Builder service tests | ±11 / ±5 | the positional `false` |

---

## 2. The v5 intersection map (file:line at `5a94a9d1e`)

Verdict legend: **LACKS** (no v5 twin), **PRE-DRIFT** (a v5 twin at the
`e8e17b2d9` shape that this slice widens), **HAS** (identical already).

### 2.1 The storage the slice stands on — ALL `LACKS`, ALL the keystone sibling's

`rg -il inventory crates/quilltap-core/src` finds only unrelated hits (the
`self_inventory` tool, the tools-list `tools_inventory.rs` — v4's
`GET /api/v1/tools` — `knowledge_injector`, the Almanack). There is NO
`crates/quilltap-core/src/inventory/` module, no `equippedInventory` column
read anywhere (`rg equippedInventory` → nothing), no `InventoryRecord` /
`ResolvedInventoryItem` type, no `post_pascal_inventory_change`
(`services/pascal_writer.rs:187` has only `post_pascal_result`), no
`refresh_avatar_for_appearance`. Every file in §1.C/§1.D imports from that
base: `loadInventoryPool`, `readEquippedSet`, `resolveItem`, `inventoryDisplayName`,
`inventoryLabel`, `isKin`, `kitState`, `resolveInventoryLocation`,
`createInventoryItem`, `updateInventoryItem`, `setInventoryItemArchived`,
`instantiate`, `assertCopyable`, `assertKitMembers`, `findKitsStrandedByMove`,
`findRecordsAnywhere`, `isReachableFromKitTier`, `mountScopeOf`,
`carryInventoryItemImages`, `commitMovedInventoryImages`, `locationKey`,
`loadPoolForLocation`, `normalizeInventoryFolderPathOrRoot`, the
`InventoryRefusal` family (`errors.ts` base), `withChatInventoryLock`,
`equipItems`/`unequipItems` (`equip.ts` base), `postPascalInventoryChange`,
`refreshAvatarForAppearance`, `buildInventorySection`,
`inventoryErrorResponse`, `createInventorySchema`, `InventoryUpdateBody`.
**Shared contract (keystone → this slice), name-level, §5.**

### 2.2 The tools (LANE T) — the wardrobe twin, file by file

| v4 | v5 twin | verdict / what moves |
|---|---|---|
| `inventory-*-tool.ts` ×9 | `crates/quilltap-core/src/tools/definitions/data.rs` (63 `ToolDef`s, `catalog_is_complete_and_parses` asserts 63 at `definitions/mod.rs:103`) | **LACKS** — regen through `harness/oracle/tools/gen-tool-catalog.mjs` from `tool-definitions.ts` at the pin; the nine keys land between `imageGeneration` and `keepImage` in `ALL_TOOLS` order (`tool-definitions-snapshot.test.ts` at the pin: 76 keys). The count assert → 76 ONLY when the Log (+3) and `edit_custom_tool` (+1) siblings land in the same union; alone this lane makes it 72 and the oracle "no extras" check fails (§4 union red). |
| `inventory-tool-flags.ts` | none (`tool_build.rs:234` has nine `wardrobe_*: bool` fields on `BuildToolsForProviderOptions:668-697`; `pseudo_tool.rs:34-55` `TextBlockEnabledToolOptions`; `text_block_prompt.rs:14-35` / `simple_json_prompt.rs:18-39` `Option<bool>` fields) | **LACKS** — v5 has no flag-record abstraction; port as a `struct InventoryToolFlags { list, read, equip, unequip, pick_up, create, update, archive, give: bool }` + `inventory_tool_flags(can_manage: Option<bool>)` + `enabled_inventory_tool_definitions(&flags) -> Vec<&'static str>` (catalog keys `inventoryList`…`inventoryGive`) in a new `tools/inventory_tool_flags.rs`, and ADD the nine `bool` / `Option<bool>` fields to the three option structs after `wardrobe_archive` (wire-order of `pickInventoryToolFlags` on the debug line and the prompt objects). |
| `handlers/inventory-tool-table.ts` | `tools/wardrobe_tool_table.rs:1-110` (`WARDROBE_TOOL_NAMES: [&str; 9]` `:90`, `WardrobeToolContext` `:102`, `run_wardrobe_tool_handler` + `finish_wardrobe_tool` + `WardrobeToolOutput::run`) | **LACKS** — new `tools/inventory_tool_table.rs` with `INVENTORY_TOOL_NAMES: [&str; 9]`, `InventoryToolContext<'a> { user_id, chat_id, character_id, pending_inventory_announcements: Option<&PendingInventoryAnnouncements> }`, the same two halves (§2.6), and `InventoryToolRun::run(format)` projecting `{ formattedText, ...payload }`. |
| `handlers/inventory-handler-shared.ts` | `tools/wardrobe_shared.rs` (`shared_wardrobe_item_read_only_message` `wardrobe_tool_table.rs:40-45`) | **LACKS** — new `tools/inventory_shared.rs` (`tier_of`, `kind_of`, `state_for`, `summarize`, `format_summary`, `closed_shelf_message`, `inventory_not_found_message`, `shared_inventory_item_read_only_message`, `error_message_of`, `log_inventory_tool` with the `[InventoryTools] <tool>` line at `debug`/`info` under target `quilltap::inventory-tools`). |
| the nine handlers | `tools/wardrobe_{list,read,create,update,archive,wear,take_off}.rs` (592 / 649 / 717 / 378 / 245 / 529 / 373 lines) | **LACKS** — nine new modules `tools/inventory_{list,read,equip,unequip,pick_up,create,update,archive,give}.rs`; `tools/mod.rs:7-49` gains eleven `pub mod` lines. |
| `tool-executor.ts:81,206,288,795-807` | `tools/executor.rs:94-162` `BUILT_IN_TOOLS` (after `wardrobe_history` `:120`), `:170-230` `PORTED_TOOLS`, `:594-640` `dispatch` (the wardrobe arm `:612`), `:1648-1720` `run_wardrobe_tool` + `wardrobe_snapshot` `:1886`; `services/tool_execution.rs:255-300` `ToolExecutionContext` (`pending_wardrobe_announcements` `:293`, `pending_wardrobe_history` `:299`), `create_tool_context` `:317-350` | **PRE-DRIFT** — add the nine names to BOTH consts (after `wardrobe_history`; the Log's three and `edit_custom_tool` are siblings' lines in the same const — a merge seam, §5), a `name if INVENTORY_TOOL_NAMES.contains(&name) => self.run_inventory_tool(tc, ctx).await` arm AFTER the wardrobe arm; a `pending_inventory_announcements: Option<PendingInventoryAnnouncements>` field after `pending_wardrobe_history` on the context, minted by `create_tool_context` from the widened factory. The no-character refusal text `The inventory tools need a character to act for.` is the arm's own (the wardrobe arm's twin — measure `wardrobe_snapshot`'s `""` flattening at `:1886` and do NOT copy it: v4 refuses BEFORE building the context). |
| `plugin-tool-builder.ts:199,401,578-580` | `services/tool_build.rs:266` `build_tools_for_provider`, the wardrobe pushes `:340-371`, the `BuildToolsInput` `:495-528`, `build_tools` `:543-640` (`wardrobe_* : input.can_create_outfits` `:618-626`) | **PRE-DRIFT** — nine `push_key(&mut universal, "inventoryX")` after `wardrobeArchive` (`:370`) and BEFORE `submitFinalResponse`; `BuildToolsInput.can_manage_inventory: bool` after `can_create_outfits` `:505` (v5 is a struct, so v4's positional index 12 is a NAMED field — NO positional hazard on the Rust side; the hazard is the ORACLE's, §4). |
| `streaming.service.ts:180,309` | same `build_tools` (v5 fused `buildTools` into `tool_build.rs`) | covered above |
| `pseudo-tool.service.ts:83,114,137,168,259` | `services/pseudo_tool.rs:34-55`, `determine_text_block_tool_options` `:92-125` (7 params), the two builders `:128-151` / `:181-204` | **PRE-DRIFT** — an 8th `can_manage_inventory: Option<bool>` param; nine fields on `TextBlockEnabledToolOptions` set `!= Some(false)`; the two builders copy them as `Some(o.inventory_x)` after `wardrobe_archive`. |
| `text-block-prompt.ts:17,232-295` | `tools/text_block_prompt.rs:14-35` options, the `WARDROBE_ARCHIVE` const `:155-159` + the push chain `:174-182` | **PRE-DRIFT** — nine `Option<bool>` fields; nine `const` blocks (verbatim §1.E) pushed after `WARDROBE_ARCHIVE`, in flag order. |
| `text-block-parser.ts:127-146,230-237,251,371-404,436-444` | `tools/text_block_parser.rs:140-185` `text_block_name_alias`, `:188-225` `param_alias`, `:228-245` `has_param_alias_map`, `:248-258` `content_param`, `:403-470` `convert_text_block_to_tool_call_request` | **PRE-DRIFT** — 19 alias arms; 8 tools' param aliases + `has_param_alias_map` arms; `content_param("inventory_create") = "description"`; the coercion loop: for `internal_name.starts_with("inventory_")` ONLY `archived`/`equip` booleans and `max_equipped` numbers (v4 `!isNaN(Number(value))` → `js_number(s).is_some()`; v4 `value.trim() !== ''` → `js_trim`), every other string untouched; then the `refs` collapse (split on `,`, `js_trim` each, drop empties; the `ref`/`refs` keys removed, `refs` appended LAST — key order of the resulting object: the rest in original order, then `refs`). |
| `simple-json-parser.ts:113-120` | `tools/simple_json_parser.rs:94-121` `tool_name_alias` | **PRE-DRIFT** — seven arms. |
| `simple-json-prompt.ts:44,237-239` | `tools/simple_json_prompt.rs:18-39`, `render_tool_entry` `:153`, the push chain `:230-240` | **PRE-DRIFT** — nine `Option<bool>`; render each enabled catalog definition after `wardrobe_archive`. |
| `app/api/v1/tools/route.ts:110,343-406,594-656,842-854` | `services/tools_inventory.rs:49-93` `BUILT_IN_TOOLS` (wardrobe rows `:66-74`), `SCHEMA_KEYS` `:100-126`, `ChatContext` `:153-161`, `build_chat_context` `:169-262` (the flags `:223-240`), the availability `match` `:238-330` | **PRE-DRIFT** — nine `BuiltInTool` rows after `wardrobe_history` (`:74`), nine `SCHEMA_KEYS` pairs after `("wardrobe_history", "wardrobeHistory")` `:126`, `can_manage_inventory` on the context (read `!= Some(false)` inside the same character load), the nine-id arm with the reason string. The family `tools_inventory_equivalence` (43-entry table, raw bytes) goes red on regen until all four siblings' rows exist (the Log's 3 + `edit_custom_tool` — v4's table at the pin has 56 rows; measure). |
| `orchestrator.service.ts:1015,1106,1167,1180,1753,1758-2044` | `services/orchestrator.rs:2122-2125` (the two flags), `:2271-2300` `build_tools` call (`can_create_outfits` `:2294`), `:2369-2398` the two `determine_text_block_tool_options` calls, `:3517-3555` the containers + `turn_tool_context`, `:3651-3657` the wardrobe flush, `:3658+` the sentinel / finalize / tool-save / empty branches | **PRE-DRIFT** — `can_manage_inventory = character.get("canManageInventory") != Some(false)` beside the two; the input field; the 8th argument ×2; a third container from the widened factory cloned onto BOTH loop contexts (`:3554-3555` and the text-pass copy at `:3621-3622` — measure the second site); and a NEW drain `flush_pending_inventory_announcements(db, &pending_inventory)` that runs on EVERY exit of the terminal region `:3658-3700` (v4's `finally` `:2043` — INCLUDING the `handleTurnSkip` early return and every `return` inside the three branches). In Rust: wrap the region in an inner `async` block / closure whose result is bound, then drain, then return (no `finally`; the P4.D306 wardrobe drain stays BEFORE the sentinel as v4's `:1753`). |
| `wardrobe/turn-containers.ts:22-57` | `services/outfit_change_effects.rs:51-67` (`PendingWardrobeAnnouncements`, `PendingWardrobeHistory`, `new_wardrobe_turn_containers() -> (…, …)`), `OutfitChangeContext` `:85-98`, `flush_pending_wardrobe_announcements` `:265` | **PRE-DRIFT** — `pub type PendingInventoryAnnouncements = Arc<Mutex<HashMap<String, PendingInventoryChange>>>`; the factory returns a 3-tuple (every caller: `create_tool_context` `tool_execution.rs:328`, the orchestrator `:3517`, the Brahma console `brahma_console/orchestrator.rs` — measure with `rg new_wardrobe_turn_containers`); `turn_containers_of` has no v5 twin (the three are passed explicitly) — thread the third wherever the first two are (`run_custom.rs:505-508,767-768`, `run_service.rs:122-125,292-293,584-585,864-865`, `side_effects.rs` `apply_effects_for_run:937-938`). |
| `brahma one-shot:103,139` / `orchestrator:218,259`; scenario `:174,213`; carina `:643`; help `:271` | `brahma_console/mod.rs:236-237`, `brahma_console/orchestrator.rs:395-396`, `scenario_builder/mod.rs:365-366`, `carina_query.rs:543-544`, `help_chat/orchestrator.rs:766-767` (each `can_create_outfits: …` on `BuildToolsInput`), plus the text-block option objects in Brahma / Scenario Builder (`rg "wardrobe_archive: false"` in those two) and the help chat's `determine_text_block_tool_options` call | **PRE-DRIFT** — `can_manage_inventory: false` at the five `BuildToolsInput` sites (NOTE help chat passes `true` for the two wardrobe flags but v4 passes `false` for inventory ONLY to `determineTextBlockToolOptions` — the help chat's `buildTools` call in v4 does not pass a 13th positional at all → `undefined` → `inventoryToolFlags(undefined)` = ON; measure `help-chat/orchestrator.service.ts`'s `buildTools(` call at the pin and set the v5 field accordingly: likely `true`), and nine `false`s on the two text-block objects. |

### 2.3 The chat spine's turn end (LANE T) — `equip-effects.ts` ↔ `outfit_change_effects.rs`

`notifyWardrobeChanged` (`outfit_change_effects.rs:231`) is `refresh_avatar_for_outfit`
then `record_pending_wardrobe_announcement` (`:197-212`) — the SAME shape as
`notifyInventoryChanged` but with the order reversed (v4 inventory: record-
and-return FIRST, avatar only on the now-path). **LACKS**: a new
`services/inventory_change_effects.rs` with `InventoryChangeProvenance`,
`InventoryChangeContext`, `InventoryChange`, `describe_inventory_change`
(pure — `looks_plural`, `indefinite_article`, `with_article`, `join_list`),
`notify_inventory_changed` (after-commit, takes `&Db`),
`record_pending_inventory_change` / `strike_from_pending_inventory_change`
(pure over the map), `flush_pending_inventory_announcements`. The avatar leg
reuses `refresh_avatar_for_outfit`'s machinery with `source_context =
"inventory-change"` (the keystone's `refreshAvatarForAppearance` rename —
measure what the keystone names it).

### 2.4 The Pascal engine (LANE P) — the wardrobe precedent, file by file

| v4 | v5 twin | verdict |
|---|---|---|
| `custom-tool.types.ts` (§1.F) | `pascal/custom_tool_types.rs` (4,120 lines): `NumberOrParamRef` `:400`, `StateRef` `:423`, `AnyOperand` `:462`, `StringOperand` `:478`, `Roll` `:525`, `ToolGate` `:685` (`metadata`, `progress` only), `WhenObject` `:725`, `When` `:777`, `EffectWhen` `:815`, `CustomToolWardrobeGesture` `:881`, `CustomToolEffect` + `CustomToolEffectBody::{Write, Wardrobe}` `:926-960`; the parsers `parse_number_or_param_ref` `:1580`, `parse_any_operand` `:1600`, `parse_string_operand` `:1631`, `parse_gate_comparator` `:1774`, `parse_tool_gate` `:1858`, `parse_roll` `:2326`, `parse_when_like` `:2363`, `parse_custom_tool_wardrobe_gesture` `:2619`, `parse_custom_tool_effect` `:2686` (the refine texts `:2772,2775`), `parse_when` `:2792`; `validate_gates` `:3244`, `validate_when_subjects` `:3317`, `validate_effects` `:3423`, `validate_wardrobe_gesture` `:3490`, `validate_roll_refs` `:3693` (`uses a $state reference whose fallback is` `:3714`), `resolve_operand_type` `:3936`, `format_definition_issues` / `flatten_issues` `:3974-4040` | **PRE-DRIFT** everywhere: an `InventoryRef { inventory: String, fallback: StateRefFallback-like }` arm on the three operand enums + `NumberOrParamRef` + `Roll::Inventory` + the parameter default; `ToolGate(Group).inventory / inventory_count / inventory_aggregate: Option<Vec<(String, _)>>` (the group rename + `Vec<ToolGateGroup>` union is the any-of sibling's); `WhenObject.inventory / inventory_count / inventory_aggregate`; `CustomToolEffectBody::Inventory(CustomToolInventoryGesture)`; `inventory_gesture_problems`; the five reworded sentences; `validate_inventory_key`; the `$inventory` roll rule; the two-way `uses a ${kind} reference` sentence. The Zod-twin corpus is `apps/web/src/testing/fixtures/pascal-custom-tool-definition.oracle.ndjson` (458 rows — SPA sibling's spec; this lane grows the ORACLE generator `harness/oracle/cases/pascal-custom-tool-definition.ts` and the SPA sibling re-vendors). |
| `placeholders.ts:35,49-78,133-136` | `pascal/placeholders.rs:55-85` `PlaceholderRef` (no inventory arm), `classify_placeholder` `:91-150` | **PRE-DRIFT** — `Inventory { reference, field: InventoryPlaceholderField, key: Option<String> }`; `split_inventory_reference` (byte-indexed `find(".metadata.")` is safe — ASCII infix); the arm BEFORE `Unknown`. |
| `inventory-subjects.ts` | none | **LACKS** — new `pascal/inventory_subjects.rs` (pure). |
| `inventory-aggregate.ts` | none | **LACKS** — new `pascal/inventory_aggregate.rs` (pure); `Math.min/max` over `f64` with JS semantics (`-0`, no NaN present since filtered finite); `sum` is a left fold in list order (float order matters — keep it). |
| `metadata-match.ts:95-172` | `pascal/metadata_match.rs:107-191` (`Value::Array(_) => "an array"` `:128` declines) | **PRE-DRIFT** — the list branch BEFORE the primitive refusal (shared with the any-of sibling, §5). |
| `tool-gate.ts:92-181` | `pascal/tool_gate.rs:89-128` `evaluate_tool_gate(def, metadata, progress)`, `gate_holds` `:129-195` | **PRE-DRIFT** — a 4th `inventory: Option<&dyn InventoryContext>`; the three loops after progress (state/count via `inventory_subjects`, aggregate via `inventory_aggregate_holds` with a literal resolver). |
| `custom-tools.ts` (§1.F) | `pascal/custom_tools.rs`: `RosterContext` is `pascal/roster.rs` (`rg "pub struct RosterContext"`), `resolve_params` `:259`, `coerce_param` `:299`, `resolve_roll_field` `:402`, `roll_range` `:472`, `OutcomeSubjects` `:526-560`, `OperandSpec` `:564-581` (`Literal / Ref / State`), `resolve_operand` `:592`, `matches_when` `:1018` / `matches_when_object` `:1034`, `matches_effect_when` `:1170`, `ResolvedEffect::{Applicable, Wardrobe, Skipped}` `:1201-1250`, `resolve_effects` `:1340`, `resolve_wardrobe_gesture` `:1454`, `TemplateVars` `:1503-1522`, `render_template` `:1530`, `resolve_placeholder_value` `:1661`, `execute_custom_tool` `:2165` (12 positional), `simulate_outcomes` `:2411` | **PRE-DRIFT** — `OperandSpec::Inventory(&InventoryRef)`; `OutcomeSubjects.inventory: Option<&dyn InventoryContext>` and `TemplateVars.inventory`; the two subject loops + the aggregate loop in `matches_when_object`; `ResolvedEffect::Inventory { index, inventory }` + its serializer (`{ action, items, equip? }` — `equip` only on `pick_up`); `resolve_inventory_gesture`; the placeholder arm; the `$inventory` whole-roll arm in `execute_custom_tool`'s roll match (it is NOT `Roll::Dice` nor `Range` — a third arm resolving to dice at run time, with `roll_form = Dice` and `notation` = the resolved/fallback string); `simulate_outcomes` gains `inventory`; `execute_custom_tool` gains `inventory` (a 13th positional — the v5 signature is already `#[allow(clippy::too_many_arguments)]`-class; a drafter may prefer an `ExecuteOverrides` struct, but the harness families call it positionally in ~10 places — count before choosing). |
| the roster's lazy load (`custom-tools.ts:485-519,556-588`) | `pascal/roster.rs:` `resolve_custom_tool_roster` (progress thunk — `rg invoker_progress`) | **PRE-DRIFT** — `RosterContext.inventory: Option<Option<Box<dyn InventoryContext>>>` (tri-state: unset → lazy; `Some(None)` → declines; `Some(Some)` → given) with the three DEBUG lines; loaded through the keystone's `load_inventory_context(db, character_id, chat_id)` ONLY when `gate_names_inventory`. The roster runs inside a read closure today — the inventory load needs main + mount reads (the pool) → the same `db.read_main(|main| db.read_mount_index(…))` nesting `tools_inventory.rs:225-227` uses. |
| `tool-vocabulary.ts` | `pascal/tool_vocabulary.rs:55-113` `ToolVocabulary` (ends `wears, takes_off, clears_slots`), `collect_tool_vocabulary` `:176`, `collect_state_refs` `:349-369` | **PRE-DRIFT** — five new `Vec<String>` fields in wire order (`inventoryReads`, `inventoryAggregates`, `equips`, `unequips`, `picksUp` with `#[serde(rename)]`), `is_empty_vocabulary`, the per-group collector, the `$inventory` arm in `collect_state_refs` (returns without descending). |
| `side-effects.ts:162-216` + `inventory-effects.ts` | `pascal/side_effects.rs:146-213` `ApplyCustomToolEffectsParams`, `apply_custom_tool_effects` `:206-250` (writes → `apply_wardrobe_effects` → `merge_applied`), `apply_effects_for_run` `:926-990` (the after-commit `finish_wardrobe_effects`); `pascal/wardrobe_effects.rs:1-200` (the two halves, `IndexedAppliedEffect` `:141`, `ApplyWardrobeEffectsParams` `:149`) | **PRE-DRIFT / LACKS** — new `pascal/inventory_effects.rs` shaped as the wardrobe applier: the in-write half plans and commits each gesture through the keystone's in-write `equip_items` / `unequip_items` / `pick_up_items` (which themselves cannot call the async notify — they return the `InventoryChange` to the caller), re-reading the context after each landed gesture ON THE WRITER THREAD; the after-commit half (`finish_inventory_effects`) runs ONE `notify_inventory_changed` per landed gesture in order (v4 calls notify INSIDE each chokepoint — one per gesture — so a run with two gestures and NO pending map posts TWO Pascal lines; with the map it nets to one). `apply_custom_tool_effects` returns a third flag / a `Vec<InventoryChange>` for the caller; the `merge_applied` stable sort already handles three lists. |
| `run-service.ts:96,141,282-310` | `pascal/run_service.rs:95-130` `RunCustomToolParams`, `run_custom_tool` `:153-300` (cascade read `:209-241`, `execute_custom_tool` `:246`, `apply_effects_for_run` `:281`) | **PRE-DRIFT** — `pending_inventory_announcements`; `load_run_inventory` BETWEEN the cascade and the deal (its WARN line); thread into the deal and the applier. |
| `workbench.ts:41,194-199` | `pascal/workbench.rs:80-83` `RollForm { Range, Dice }`, `:390-393` | **PRE-DRIFT** — `RollForm::Inventory` (serialized `"inventory"`). |
| `bench-inventory.ts` | `pascal/bench_wardrobe.rs` (`resolve_bench_wardrobe_against`, PURE, wire as `Value`) | **LACKS** — new `pascal/bench_inventory.rs`: `MockInventoryRow` parsed with v4's Zod strictness (unknown key → the 400's Zod-issue bytes — the route family compares status+body) + `mock_inventory_context(rows) -> impl InventoryContext`. |
| `custom-tools/route.ts:106,117,265,289,322-334` | `api/types.rs:2468-2510` (`CustomToolPreview { definition, params, private, metadata, state, llm }`, `CustomToolAudit { definition, params, metadata, state, llm }`), `api/custom_tools.rs:1301-1450` `custom_tool_preview` (the `execute_custom_tool` call `:1382`, `evaluate_tool_gate` `:1421`), `:1451-1560` `custom_tool_audit` (`simulate_outcomes` `:1514`); the axum edge `crates/quilltap-web/src/custom_tools_routes.rs:161-200` (preview/audit body → Request) | **PRE-DRIFT** — an `inventory: Option<serde_json::Value>` on both variants (after `state`, before `llm`); parse through `bench_inventory` (absent/null → `None`); pass to the deal, the gate, the audit. The web edge is a pass-through (`rg "state" custom_tools_routes.rs` to find the body-field copy). |
| `run-custom-tool.ts:98,105-224,289-296,334-357` | `tools/run_custom.rs:95-110` `RUN_CUSTOM_PREAMBLE` (`or what they are wearing, server-side` `:105`), `describe_effects` `:119-160`, `describe_roll` `:167`, `describe_when` `:358-400`, `build_run_custom_description` `:402-470` | **PRE-DRIFT** — the preamble bytes (this is ALSO the byte-identity obligation with `definitions/data.rs`'s `run_custom` description — `empty_roster_description_matches_data_rs` — so the catalog regen and this edit land TOGETHER); `describe_roll`'s `$inventory` arm; `SCOPE_WORDS` / `LIST_WORDS` / `describe_aggregate` / the three `describe_when_group` clauses (the group/any-of rendering is the sibling's); `Reads:` / `Takes stock of:` lines; the inventory clauses in `describe_effects` (before the Log's). |
| `realtime.types.ts:50`, `job-topics.ts:141,149-158,201-204` | `realtime/types.rs:15-43` `RealtimeTopic` (7) + `REALTIME_TOPICS: [_; 7]`, `as_str` `:45-57`; `realtime/job_topics.rs:136` `REPOSITORY_TOPICS`, `topics_for_write_batch` `:202-229` | **PRE-DRIFT** — `RealtimeTopic::Inventory` (`"inventory"`, LAST), the 8-array, the regex `(^|[\\/"])Inventory\/` over `serde_json::to_string(&args)` (v4 `JSON.stringify` of the args ARRAY — v5's `ChildWritePayload.args` is already the verbatim `Value` array), the `inventory` hint pushed BEFORE the namespace lookup. `realtime_topics_equivalence` (28 rows, oracle-owned corpus) + `realtime_publish_sites_guard` (every `publish_realtime` site in v5 must be censused — the eight new `publishRealtime('inventory')` sites are the keystone's; the handlers' `publishRealtime('inventory')` / `('chats', id)` calls in create/update/archive/give are THIS lane's and must be registered in the guard). |
| the Almanack (§1.G) | `almanack/types.rs:369-380` `CharacterBreakdownInfo` (`can_create_outfits` `:378`), `phase3_ledgers.rs:544` (the SQL), `:575`, `render.rs:950-951`, `almanack/mod.rs:327` | **PRE-DRIFT** — one field after `can_create_outfits` (`#[serde(rename_all = "camelCase")]` → `canManageInventory`), the SQL column, the render line between `May create outfits` and `With a Core-whisper override`. `almanack_tier2_equivalence` / `almanack_render_equivalence` + the v5 fixture DDL at `phase3_ledgers.rs:1193,1249` (the test's `characters` table needs the column — the keystone's `fresh_schema.json` re-dump supplies production; the in-crate fixture SQL is this lane's). |
| `chats.repository.ts:839-876` | `db/chats_outfits.rs:123-230` (`get_equipped_outfit_for_character` `:133`, `set_equipped_outfit` `:152`, `remove_equipped_item_from_all_chats` `:190` — the `equippedOutfit` twins) | **LACKS** — the keystone's `db/chats_inventory.rs` (or wherever it homes `equippedInventory`) must export `find_chat_ids_carrying_inventory_id` + `remove_equipped_inventory_id` with the two DEBUG lines and the two fallback homes. Shared contract (§5). |
| `context-manager.ts:2192-2193` (bug 253) | `services/build_context.rs:2920-2955` (the progressions block, bug 254 ported, "Built on every turn, continue mode included") | **LACKS** the section (keystone's `prompt-section.ts`); THIS lane contributes only: the ungated call site BELOW the progressions block (same order as v4 `:2174` → `:2192`), the `is_continue_mode` doc comment gaining `inventory`, and `build_context_tier3_equivalence`'s row. Ownership: the keystone writes `build_inventory_section`; the call site is a one-hunk rider either lane can own — assign it to the KEYSTONE (it needs the section to exist) and record bug 253 there. |

### 2.5 What this slice must NOT touch (siblings')

- `crates/quilltap-core/src/db/*` for the column / the pool / the vault
  (`characters_read.rs:69,196` the `SELECT` list and `put_opt_bool` for
  `canCreateOutfits` — the keystone adds `canManageInventory` beside it;
  `db/characters.rs:296,408`; `api/characters.rs:2225` the PUT key list;
  `db/vault_character_update.rs:437`; `quilltap_import/characters.rs`; the
  restore orchestrator's field list; `db/store_name_reconcile.rs`'s `SELECT *`).
- `apps/web/**` (the SPA sibling: `tool-draft.ts`, the Workbench, the run
  dialog, `core-contract.ts`, `realtime-topic-map.ts`, the Aurora Profiles
  select, the nine `ToolMessage` rows).
- The any-of grammar (`ToolGateGroup`/`Vec<…>` union, `When::Array`,
  `EffectCondition`, `gate_groups`/`when_groups`/`effect_when_groups`,
  `held_group`, `any_group_holds`, `is_not_an_array`, the gate-key census
  `tool-gate-keys.ts`), `edit_custom_tool`, bug 257, `cfa0cba29`'s vault-only
  rule — the custom-tools sibling.
- The Log (`log_*` tools, the `log` effect, `writes to the log`, the
  `logged` merge list, the fourth effect kind) — the Log sibling.
- `help/`, `docs/v4/`, the two public schemas — the vendoring sibling.

### 2.6 The two-halves rule, stated once for the drafter

Every v4 inventory handler is `async` and awaits repository calls and the
notify inline. v5's handlers run SYNC inside ONE `Db::write` closure holding
both writer connections (`executor.rs:1648-1700`, `wardrobe_tool_table.rs`
header): a handler answers its output PLUS the `InventoryChange`s it made
(and the `publishRealtime` topics it owes); `finish_inventory_tool` then
runs the after-commit effects in v4's order — `publish_realtime('inventory')`,
`dropCarriedEverywhere`'s per-chat rewrites are IN-WRITE (they are writes),
`publish_realtime('chats', id)`, then `notify_inventory_changed` (record into
the pending map, or post + avatar). `inventory_give`'s `transferInventoryItem`
is a multi-store write (source delete + destination create + image carry) —
in-write, one closure. The second `loadToolInventory` in `inventory_update`
(`:71`) is a re-read in the same closure. The log lines a handler writes
inside the write land on the writer thread (memory note
`capture-rig-writer-thread`): the differential must use `global_capture`.

---

## 3. Schema / migrations / boot ensures

**None in this slice.** The FOURTEENTH move (`characters."canManageInventory"
INTEGER DEFAULT NULL`, `add-character-inventory-flag-v1`, `dependsOn
add-character-wardrobe-flags-v1`, column-gated, registered LAST) and the
`equippedInventory` chat column (`add-chat-equipped-inventory-v1`,
`4fbdad186`) are the keystone sibling's (`fresh_schema.json` re-dump,
`migrations_state` stamps, the mirror-boot list, `provisioning_equivalence`,
`zero_migrations_rerun_equivalence`, `restore_vintage_state`). This slice
READS the flag only (§0's seven sites) and READS/WRITES the chat column only
through the keystone's repository (`setEquippedInventory`,
`findChatIdsCarryingInventoryId`, `removeEquippedInventoryId`,
`getEquippedInventory`). Two fixture consequences the lane DOES own:

- the Almanack ledger's in-crate fixture SQL (`almanack/phase3_ledgers.rs:1193,1249`)
  must gain the `canManageInventory INTEGER` column or the `SUM(CASE WHEN
  "canManageInventory" …)` query fails the unit test;
- every committed fixture pair this lane's families run over
  (`pascal-run-custom-{main,mount}.db`, `workbench-*.db`, `wardrobe-tools`,
  `chat-dialogs-*.db`) was baked BEFORE both migrations — a v5 boot of a copy
  must run the keystone's two ensures first (memory note
  `rebuilt-fixture-pair-vintage`); the `tools_inventory_equivalence` oracle
  (v4's real `GET /api/v1/tools` over `chat-dialogs-main.db`) runs v4's
  migrations at open, so BOTH sides see the column — but `characters_read.rs:69`'s
  `SELECT` list must already name it (keystone) or v5 reads `null` → ON by
  accident and a planted `0` never reaches the availability arm. Plant a
  `canManageInventory = 0` character in that family's case list to catch it.

---

## 4. The differential plan

Tier legend: **T1** exact (pure, byte-equal), **T2** structural DB diff,
**T3** mocked-LLM / scripted, **R** Tier R (CLI). Recipe = the closest
existing harness file; "pin" = v4 `322a5488e` (PIN REQUIRED — the v4 tree is
at the pin and clean today, but the regen rule is the ledger's).

### 4.1 LANE T — the tools

| unit | tier | family (extend) | fixture / corpus | new oracle work |
|---|---|---|---|---|
| the nine definitions (catalog bytes) | T1 | `tool_definitions_equivalence` (`harness/oracle/cases/tool-definitions.ts` + `-canonical.ts`; regen `gen-tool-catalog.mjs`) | none | regen at the pin → 76 entries (`catalog_is_complete_and_parses` → 76; `definitions/mod.rs:91-103`'s history comment gains the four siblings' lines). **UNION red**: this lane alone yields 72 and the oracle's "no extras" assertion fails on `logCreateEntry`, `logGrep`, `logRead`, `editCustomTool` — either the unifier regens once for all four, or each lane's regen is run with the OTHER three keys filtered (the `tool-definitions.ts` case imports every definition file, so a lane cannot regen at the pin without the siblings' tool files existing in v4 — they DO exist in v4; the filter is on the v5 side: a lane-local `catalog_is_complete` carve naming the three families, lifted at unification). |
| the nine handlers + format strings + the mutated state | T2/T3 | `wardrobe_tools_equivalence` (`harness/oracle/cases/wardrobe-tools.ts`, 399 lines; `fixtures/build-wardrobe-tools-fixture.ts`; `wardrobe-tools.json` op matrix, 129 keys) | a NEW `build-inventory-tools-fixture.ts` over the keystone's store twin of `__tests__/helpers/inventory-fake-store.ts` (a caller + a receiver character with vaults holding `Inventory/` files, a project store with an archetype + a kit, a General tier, a chat with both seated, a seeded `equippedInventory`) — the keystone's fixture if it bakes one; else this lane's | `inventory-tools.ts` oracle driving v4's REAL `INVENTORY_TOOL_TABLE[name](input, context)` with a REAL `pendingInventoryAnnouncements` map per op (then `flushPendingInventoryAnnouncements` as a step), comparing per op: the Output (minus `formattedText`), the `format*` string, the mutated state (both vaults' `Inventory/*.md`, the chat's `equippedInventory` map, `chat_messages` system rows with `systemKind = 'inventory-change'`), and the `[InventoryTools]` / `[InventoryEquip]` / `[InventoryEffects]` lines. Minted ids/timestamps positionally normalised (the wardrobe family's rule). v4's 18 `it`s (§1.H) are the op list; add: a `refs` with duplicates, a kit unequip by kit name, `include_archived`, the `folder` filter with `/` edges, `metadata` with a `null` delete, `inventory_update` on an instance (override), a give by id vs name, the `\"42\"` title through the text-block parser. |
| `resolveInventoryRef` / `stateOf` / `equippedKinCount` / `pickForGesture` / `inventoryContextFrom` / `scopeObjectsOf` | T1 | NEW `inventory_refs_equivalence` (recipe: `pascal_bench_wardrobe_equivalence` — a `node --import tsx` pure oracle over a hand-built pool) | the oracle builds `ResolvedInventoryItem[]` literals (no DB) | `inventory-refs.ts` with v4's 12 `refs.test.ts` cases + the aggregate's 4 scope cases + corners: a title equal to another item's name (title round wins pool-wide), an archetype named by id with `includeArchetypes:false` (nothing), own-archived with/without the flag, a kit whose members are half equipped, a ref with surrounding whitespace, mixed-case id. |
| `describeInventoryChange` + the pending-map netting | T1 | NEW rows in the same family (or `inventory_change_effects_equivalence`) | none | `describe` rows: titled vs untitled, `lockpicks`/`gloves`/`compass`/`abacus`/`iris`, `unicorn horn`/`one-eyed idol`/`amulet`/`eu…`, 1/2/3-item Oxford lists, the give line, picked-up excluded from `takes up`; `record_pending` rows: equip then unequip (nets), unequip then equip, pick-up then set down (removed from `pickedUp`), two provenances, `strike`. |
| `flush` + `notify` on the spine (the ONE line per character, posted AFTER the saved turn) | T3 | `orchestrator_tier3_equivalence` (`tool-execution-process-tier3.test.ts` / `orchestrator_tier3`) | the salon fixture + a character with an `Inventory/` vault | a scripted turn whose canned response calls `inventory_equip` ×2 (one netted), asserting the `chat_messages` ORDER: the tool-save row, THEN Pascal's `inventory-change` row (102a's `finally`); and a turn-skip exit still flushes. |
| the text-block / simple-JSON parsers + prompts | T1 | `pseudo_tool_parsers_equivalence` (`pseudo-tool-parsers.ts`, 282 lines) / `pseudo_tool_prompts_equivalence` (`pseudo-tool-prompts.ts`) | none | parsers: the 19 aliases, the 8 alias maps, `inventory_create` content → `description`, `"42"`/`"true"` stays text, `archived="false"`, `max_equipped="3"` / `""` / `"abc"`, `ref` + `refs="a, b,,c"` collapse, `refs` array; prompts: **RUNTIME BREAK** — `pseudo-tool-prompts.ts:121-129` calls `determineTextBlockToolOptions` with SEVEN args → at the pin the 8th is `undefined` → `inventoryToolFlags(undefined)` = nine `true` → every `build_text_block_system` / `build_simple_json_system` row gains the nine sections. The lane MUST add an 8th `canManageInventory` to `DetInput` + the call (with `undefined` kept as a case so the "null = on" default is pinned) and add `det-tb-inventory-off`. |
| `buildTools` slate | T1 | `tool_build_equivalence` (`tool-build.test.ts:142-162`, `fixtures/tool-build.json` 40 cases) | none | **RUNTIME BREAK** — the oracle passes `docToolsMode` at index 13 which is now `canManageInventory` (`'full'`/`'off'` → `!== false` → ON; and `docToolsMode` receives `askCarinaEnabled` (a boolean → neither `'read'` nor `'full'` → `off`!), `askCarinaEnabled` receives `includeWorkspaceTools`, … — every case past the pin shifts). Fix: insert `c.canManageInventory ?? true` at index 12 under a `PIN_HAS_INVENTORY_FLAG` export probe (the file's existing `PIN_HAS_DOC_TOOLS_MODE` pattern), so the same case runs at both pins; add `p4d_inventory_off` / `p4d_inventory_null` cases; the 34 workspace cases gain nine tools between `wardrobe_archive` and `submit_final_response` (sorted compare — order-blind, but PRESENCE moves). |
| `GET /api/v1/tools` | T2 | `tools_inventory_equivalence` (`chat-dialogs-tools.test.ts`, 3 cases; `chat-dialogs-{main,mount}.db`) | the committed pair + the keystone's column | the nine rows (table now 58 at the pin incl. the Log's 3 + `edit_custom_tool`) + a `canManageInventory: 0` plant → `unavailableReason`. UNION red with the two siblings as the catalog's. |
| the executor arm + `BUILT_IN_TOOLS` | T1/T2 | `tool_execution_tier2_equivalence` (`tool-execution-tier2.ts`) | — | `inventory_list` with no `characterId` → the refusal text; `characterIds` is dead (sibling). |
| the Brahma / Scenario / Carina / Help off-sites | T3 | `brahma_orchestrator_tier3_equivalence`, `help_chat_orchestrator_tier3_equivalence`, the Scenario Builder family, `carina_query` | existing | the slate assertions: NO `inventory_*` on Brahma / Scenario / Carina; Help chat's `buildTools` passes ELEVEN positionals (`help-chat/orchestrator.service.ts:237-249`) → `canManageInventory` undefined → the nine ARE offered to the help chat's native slate (as the two wardrobe flags are) while its TEXT-BLOCK path passes `false` (`:271`) — pin BOTH (v5 `help_chat/orchestrator.rs:766-767` already carries `true, true`; add `can_manage_inventory: true` there and nine `false`s on its text-block options). |
| the Almanack line | T2/T1 | `almanack_tier2_equivalence` + `almanack_render_equivalence` (`__tests__/helpers/almanack/fixture.ts` +1 at `e1872c94f`) | the Almanack fixture | the count column; the render line; the v5 in-crate fixture SQL. |
| the realtime topic + the write-batch hint | T1 | `realtime_topics_equivalence` (`realtime-topics.ts`, 28 rows — oracle-owned corpus) + `realtime_publish_sites_guard` | none | rows: a `docMountFileLinks.linkDocumentContent` write whose args name `…/Inventory/foo.md` (hint `inventory` bare, FIRST, then `mountPoints`), a `docMountFiles.create` with `Inventory/` inside a JSON string value (`"` edge), a non-`docMount` method with the path (NO hint), a lowercase `inventory/` (NO hint — the regex is case-sensitive), and an `Inventory/` preceded by `\\` (the escaped backslash arm). |
| the transfers refactor | T2 | `wardrobe_transfers.test.ts`'s twin for the Inventory — the keystone's `inventory-transfers` route family if it exists; else a NEW `inventory_transfers_equivalence` | the keystone's fixture | status+body parity for the route's five refusals (same-location 400, not-copyable, kit stranded, id collision 400, the move's source-delete 500) and the INFO line's new prefix `[InventoryTransfers]` (was `[InventoryTransfers v1]`). If the keystone already ported the ROUTE body inline, this lane's job is to move it into `inventory::transfers` and keep that family green. |

### 4.2 LANE P — the Pascal grammar

| unit | tier | family (extend) | corpus | new rows |
|---|---|---|---|---|
| the definition schema: `$inventory` on the four unions + the parameter default + the roll; `inventory` / `inventoryCount` / `inventoryAggregate` on the gate group and the when; the inventory gesture; the five sentences; `validateGates` / `validateWhenSubjects` / `validateEffects` / `validateRollRefs`; the `invalid_key` arm | T1 | `pascal_custom_tool_definition_equivalence` (`pascal-custom-tool-definition.ts`, 968 lines; the SPA vendors its NDJSON as the 458-row corpus) | none | v4's 11 new `it`s as rows (§1.H) + one row per sentence: each of the four `must test …` variants, `must test at least one metadata key, progress field, inventory object, or inventory aggregate`, both effect-kind sentences, `equip is only used with pick_up`, `pick_up needs at least one object`, the placeholder-in-gate-key (bare AND inside an array group — the path `[clause, g, subject, key]`), `$param` to a number parameter for a state operand (`eq compares an inventory state with parameter "p", which is number rather than a string`), containment on a count (`must specify at least one comparator (gt, gte, lt, lte, eq, neq)` — the strict object's own refusal on `contains`), a whole-roll `$inventory` with a numeric fallback, a `roll.min` `$inventory` with a string fallback (`roll.min uses a $inventory reference whose fallback is string rather than a number`), the `$inventory` describe bytes on the parsed DATA (the parsed data IS compared byte-wise — `fallback` serialises as the JS value), every aggregate key form (`equipped.count`, `owned.names`, `reachable.titles`, `equipped.metadata.a`, `equipped.metadata.a.b`, `equipped.metadata.stats.max`, `equipped.metadata..max` → key `.`? measure), the three parser reasons through `invalid_key`. The `gate` rows (`evaluateToolGate` with a 4th arg): a mock context built in the oracle from `mockInventoryContext` rows (§1.F) — v4's `custom-tools-inventory.test.ts` "the gate" ×2 + `cad017599`'s (c). |
| `resolveInventoryValue`, `inventoryStateHolds`, `inventoryCountHolds`, `parseAggregateKey`, `aggregateValue`, `inventoryAggregateHolds`, the list comparator | T1 | `pascal_custom_tools_execution_equivalence` (`pascal-custom-tools-execution.test.ts`, 66 cases — `matchesWhen` rows at `:590,662,1924`) | the oracle's own literals | `matchesWhen` rows with an `inventory` subject on a MOCK context (the bench's) AND on a REAL `inventoryContextFrom` over literal items (so `item()`, `{{inventory.*}}` and `$inventory` reads resolve); the 22 `inventory-aggregate.test.ts` `it`s as rows (an empty scope is a fact: `eq` declines, `neq`/`ncontains` hold; a list with a non-primitive declines whole; the fold-word key); `compares a list of primitives entry by entry` for a PLAIN metadata key (the semantics change on existing metadata — one row with `knownLanguages: [...]`). |
| `executeCustomTool` with `inventory` (the `$inventory` roll, the fallback DEBUG, parameter defaults, placeholders, the llm prompt render, chipLabel) | T1 (FixedBytes) | same family (`:1780` `executeCustomTool` rows) | — | the whole-roll from `sword.metadata.damage = "2d6"` (dice draw count pinned by `FixedBytes::consumed`), the fallback `"1d4"` when the key holds `3` (a number — `typeof !== typeof fallback`), `{{inventory.<ref>.title}}` title-or-name, `.metadata.k` primitive vs list (left as written), `.state` with NO item (renders `absent` — `state()` answers before `item()`), the apostrophed title (`cad017599`), `{{inventory…}}` with a null context (as written + the DEBUG reason). |
| the placeholders classifier + `splitInventoryReference` | T1 | the same (or `pascal_expressions_equivalence`'s pure shape) | — | `inventory.a.b.metadata.c.d` (ref `a.b`, key `c.d`), `inventory.x.metadata.` (null → unknown), `inventory..title` (null), `inventory.x.state`, `inventory.title` (null), `inventory.x.metadata.y.state` (metadata wins — FIRST `.metadata.`). |
| the vocabulary | T1 | `pascal_tool_vocabulary_equivalence` (`pascal-tool-vocabulary.ts`, whole-object bytes) | — | v4's row (`reports inventory reads from tests, gates, placeholders and $inventory, and the three gestures`) + an aggregate-only tool (`inventoryAggregates` set, `metadata` NOT grown by `metadata.<key>` aggregate keys), `$inventory` as a parameter default, `$inventory` inside a `roll.min`, a `pick_up` effect item with `{{params.x}}` (verbatim in `picksUp` AND `params` grown), an any-of outcome (sibling's shape — keep ONE row so the union is proven). |
| `buildRunCustomDescription` (preamble, `describeRoll`, `Reads:` / `Takes stock of:`, the three when clauses, the effect clauses) | T1 | `pascal_run_custom_equivalence` (`pascal-run-custom.ts`, 21 rows) + the in-crate `empty_roster_description_matches_data_rs` | — | one roster with: an `$inventory` roll, `inventory`/`inventoryCount`/`inventoryAggregate` whens (count fold, list fold, names list with `eq`+`ncontains`, a bad key), three gestures (dedup), `revealOdds:false` + `inventoryReads` (pins that `Reads:` is NOT emitted — the early `continue` at `:267-268` precedes it). |
| the side-effects partition (writes → wardrobe → inventory → (log), merged by index) | T1 | `pascal_side_effects_equivalence` (`pascal-side-effects.json`, 139 keys; `plan_run`) | the committed corpus | v4's `side-effects-partition.test.ts` (1) as a step: the `applied` list order + `pascalMeta.effects` target strings `inventory.equip` etc. — the oracle mocks `applyInventoryEffects` as it mocks `applyWardrobeEffects` (`pascal-side-effects.test.ts:71`), so only the MERGE is proven here; the applier itself is below. |
| `applyInventoryEffects` (real chokepoints, re-read between gestures, the skips, the records, the netted line) | T2/T3 | `pascal_wardrobe_effects_equivalence` (`pascal-wardrobe-effects.json` 23 steps; `build-pascal-wardrobe-effects-fixture.ts`; `apply_effects_for_run` driven whole) | a NEW `build-pascal-inventory-effects-fixture.ts` (a vault with instances + a project archetype + a kit, `equippedInventory` seeded, avatar generation off, `background_jobs` materialized; the project roster BOTH ways) | `pascal-inventory-effects.json` steps = v4's 8 `it`s + the cap on the SECOND of two gestures, a kit by name, `equip:false`, a reference rendering empty, the no-pending (posted now) shape vs the joined shape; compared as BYTES: the applied records (`previous` sorted ids, `next` `{ item:{id,title}, created?, equipped }`), the chat's `equippedInventory`, the vault's new instance files (ids normalised), `chat_messages` `inventory-change` rows, the pending map (sorted entries), the lines (`pascal.inventory-effects` context, `[InventoryEquip]`, `[InventoryEffects]`). |
| the roster's lazy inventory load (gates) | T2 | `pascal_roster_equivalence` (`pascal-custom-tools-discovery.test.ts`, 49 scenarios, store reads mocked) + `pascal_definition_reader_equivalence` | the scenario corpus | scenarios with an `inventory`-gated definition and (a) `ctx.inventory` given (a mock), (b) no character (declines, nothing loaded — assert NO `[InventoryContext] Loaded` line), (c) a real load over the `pascal-run-custom` fixture through `pascal_custom_tools_route_equivalence`'s GET (the roster surface) — the three DEBUG lines. |
| the run service end to end (`loadRunInventory`, the deal, the applier, the posted line AFTER the Pascal result) | T3 | `pascal_run_custom_handler_equivalence` (`pascal-run-custom-handler.test.ts`, 4 cases + plants) and `pascal_custom_tools_route_equivalence` (POST run) | `pascal-run-custom-{main,mount}.db` EXTENDED with `Inventory/` files for CHAR_A/CHAR_B (an `extend-pascal-run-custom-inventory.ts` beside the two existing extenders) | `cad017599`'s "Measure Twice" as rows (the title-keyed gate opens for A, closed for B, no-context closed; the render), a tool whose effect picks up and whose message quotes `{{inventory…}}` — the `pascalMeta.effects` TEXT bytes (key order `target, previous, next`), the `inventory-change` system row's content and its `rowid` ORDER relative to Pascal's result row (v4: the effects apply BEFORE the Pascal post, and the gesture's notify posts INSIDE the applier — so the `inventory-change` row precedes the result row on the NO-pending path; through a model turn the map defers it until the `finally`). |
| the Workbench library `rollForm:'inventory'` | T2 | `pascal_workbench_equivalence` (`workbench.json`) | the workbench fixture + one `$inventory`-rolled definition file | one entry. |
| the bench (`inventory` on preview/audit; `mockInventoryContext`; the gate verdict with a mock) | T2 | `pascal_workbench_route_equivalence` (`workbench-route-cases.json`, 98 keys — read by BOTH sides) | — | preview: an `inventory` body with rows (`count` default, a later duplicate row winning, a `metadata` list), a definition gated on it (`gate` present in the body), `{{inventory.x.title}}` rendering AS WRITTEN (the mock's null `item()`), a `$inventory` operand falling back; audit: an aggregate `when` over the mock scopes (deterministic `min === max`); a body with `inventory: null` (same as absent); an invalid row (unknown key → 400 bytes). |
| `pascal_simulate_equivalence` | T1 | `pascal-simulate.ts` (26 rows; calls `simulateOutcomes(def, params, runs, metadata, llm, state)`) | — | the 7th/8th args default — add an `inventory` mock row set and an aggregate when. |
| the trigger runner with an inventory effect | T3 | `pascal_trigger_runner_equivalence` | the trigger room | one phrase firing a `pick_up` effect → NO pending map on the runner path (posted now) — pins the "posted now" DEBUG value. |

### 4.3 Runtime breaks at the pin (the orders must own these edits)

1. `harness/oracle/cases/tool-build.test.ts:142-162` — the positional
   shift (§4.1). Red for ~34 of 40 cases BEFORE any v5 change; the fix is in
   the oracle, with a pin probe so the pre-inventory pin still runs.
2. `harness/oracle/cases/pseudo-tool-prompts.ts:121-129` — the 7-arg call
   now means "inventory ON"; the v5 side lacks the sections → red until LANE
   T lands; the oracle must grow the 8th arg (keep an `undefined` row).
3. `harness/oracle/cases/tool-definitions.ts` / `-canonical.ts` — import
   every `*-tool.ts`; at the pin they emit 76 rows; the v5 catalog (63 →
   72 with this lane) fails "no extras" on the four sibling tools — a UNION
   red, carve by name per lane.
4. `harness/oracle/cases/chat-dialogs-tools.test.ts` — v4's table is 58 rows
   at the pin (9 inventory + 3 log + 1 edit + the 43 + 2 others — measure);
   v5's 43-row table is red by design until all three lanes land — the same
   carve.
5. `harness/oracle/cases/pascal-run-custom-handler.test.ts:62,349,464-473`
   and `pascal-trigger-runner.test.ts:251` still pass `characterIds` —
   `cfa0cba29` removed it from `RunCustomToolContext`; tsx does not type-check,
   the key is ignored → NOT a runtime break (the custom-tools sibling's
   cleanup).
6. `harness/oracle/cases/pascal-custom-tool-definition.ts:963-964` calls
   `evaluateToolGate(data, metadata[, progress])` — the 4th arg optional →
   no break; rows with inventory gates must pass a mock (new rows only).
7. `harness/oracle/cases/pascal-side-effects.test.ts:71` mocks
   `applyWardrobeEffects` — at the pin `applyCustomToolEffects` also imports
   `applyInventoryEffects` and the Log's applier; an inventory gesture in a
   step would reach the REAL applier (and `getRepositories()` — which that
   oracle mocks) → a step with an inventory gesture needs the mock extended;
   the EXISTING steps carry none → green at the pin (measure).
8. `pascal-custom-tools-execution.test.ts` — `matchesWhen` rows with the
   OLD `when` shapes still parse (any-of is additive) → green at the pin;
   the `metadata-match` list change flips NO existing row (no existing row
   holds a list — measure by regen).
9. `wardrobe-tools.ts` / the wardrobe fixture builders — untouched by the span
   (`git diff --stat e8e17b2d9 322a5488e -- lib/wardrobe lib/tools/wardrobe-* lib/tools/handlers/wardrobe-*` — measure; the `outfit-change-effects.ts` +11 at `4fbdad186` is `refreshAvatarForAppearance`, additive).

---

## 5. Proposed lane cut (DISJOINT files) + the Shared contracts

**A DAG, not a swarm:** KEYSTONE (the keystone sibling's survey: `4fbdad186`'s
`lib/inventory` base + the two D23 moves + the routes + `prompt-section`) →
**LANE T** (tools + the turn container + the spine) ∥ **LANE P** (the Pascal
grammar + appliers + bench). T and P are disjoint in FILES but both consume
the keystone's `inventory` module and both touch two shared seams named
below. The custom-tools sibling (any-of / `edit_custom_tool` / bug 257) and
the Log sibling touch the SAME v5 files as LANE P (`custom_tool_types.rs`,
`custom_tools.rs`, `side_effects.rs`, `tool_vocabulary.rs`, `run_custom.rs`)
— they cannot be disjoint by file; they must be disjoint by HUNK with the
contracts in §5.3, and the unifier cherry-picks in the order P → any-of →
Log (P first because the Log's effect-kind sentence names "an inventory
gesture" and the any-of's `testsSomething` sentence names `inventoryAggregate`).

### 5.1 LANE T — `p4.dXXX-inventory-tools-and-turn.md` (bumps `quilltap-core` + `quilltap-web`)

v4 → v5:
- `lib/tools/inventory-*-tool.ts` ×9 → `tools/definitions/data.rs` (regen) +
  the count assert (72, carved for the siblings' four);
- `lib/tools/inventory-tool-flags.ts` → NEW `tools/inventory_tool_flags.rs`;
- `handlers/inventory-tool-table.ts`, `inventory-handler-shared.ts`, the nine
  handlers → NEW `tools/inventory_tool_table.rs`, `tools/inventory_shared.rs`,
  `tools/inventory_{list,read,equip,unequip,pick_up,create,update,archive,give}.rs`;
  `tools/mod.rs`;
- `lib/inventory/refs.ts`, `context.ts`, `roster-gate.ts`, `transfers.ts`,
  `equip-effects.ts` (whole), the `equip.ts` hunks (`dropCarriedEverywhere`,
  `pickUpItems`, `known`, `assertShelfReach`), the `errors.ts` `OFF_ROSTER`
  class, the `pool.ts` `projectNameOf` hunk → NEW `inventory/refs.rs`,
  `inventory/context.rs`, `inventory/roster_gate.rs`, `inventory/transfers.rs`,
  `services/inventory_change_effects.rs`; the equip/errors/pool hunks as
  additions INSIDE the keystone's modules (`inventory/equip.rs`,
  `inventory/errors.rs`, `inventory/pool.rs`) — **the ONE file-level overlap
  with the keystone; sequence it (keystone lands first) or hand the keystone
  these three hunks by name**;
- `lib/wardrobe/turn-containers.ts` → `services/outfit_change_effects.rs:51-67`
  (the 3-tuple factory) + every caller (§2.2);
- `lib/chat/tool-executor.ts` → `tools/executor.rs` (the consts + the arm) +
  `services/tool_execution.rs` (the field + `create_tool_context`);
- `plugin-tool-builder.ts`, `streaming.service.ts`, `pseudo-tool.service.ts`,
  `text-block-prompt.ts`, `text-block-parser.ts`, `simple-json-parser.ts`,
  `simple-json-prompt.ts` → `services/tool_build.rs`, `services/pseudo_tool.rs`,
  `tools/text_block_prompt.rs`, `tools/text_block_parser.rs`,
  `tools/simple_json_parser.rs`, `tools/simple_json_prompt.rs`;
- `orchestrator.service.ts` (the flag, the two 8th args, the third container,
  the `finally` drain) → `services/orchestrator.rs`; the five off-sites →
  `brahma_console/{mod,orchestrator}.rs`, `scenario_builder/mod.rs`,
  `carina_query.rs`, `help_chat/orchestrator.rs`;
- `app/api/v1/tools/route.ts` → `services/tools_inventory.rs`;
- `app/api/v1/inventory/transfers/route.ts` → the keystone's route module
  calls `inventory::transfers::transfer_inventory_item` (one hunk, LANE T);
- `chats.repository.ts:839-876` → the keystone's `equippedInventory`
  repository module (TWO methods LANE T needs — contract §5.3.1);
- `realtime.types.ts` + `job-topics.ts` → `realtime/types.rs`, `realtime/job_topics.rs`;
- the Almanack trio → `almanack/{types,phase3_ledgers,render}.rs` + `mod.rs`;
- `lib/chat/context-manager.ts:2192-2193` (bug 253) → `services/build_context.rs`
  — ASSIGNED TO THE KEYSTONE (it owns the section) with the doc-comment hunk;
- `child-repositories-proxy.ts` → NO-PORT (ratify at unification).
- Oracles/harness: `tool-definitions.ts` regen, NEW `inventory-tools.ts` +
  fixture builder, NEW `inventory-refs.ts`, the `pseudo-tool-parsers.ts` /
  `-prompts.ts` rows + the 8th arg, `tool-build.test.ts` (the positional
  fix + rows + `tool-build.json`), `chat-dialogs-tools.test.ts` rows,
  `realtime-topics.ts` rows, the Almanack fixture, an `orchestrator_tier3`
  scripted turn; `crates/quilltap-harness/tests/{tool_definitions,tools_inventory,tool_build,pseudo_tool_parsers,pseudo_tool_prompts,realtime_topics,almanack_*,orchestrator_tier3}_equivalence.rs` + NEW `inventory_tools_equivalence.rs`, `inventory_refs_equivalence.rs`.

### 5.2 LANE P — `p4.dYYY-pascal-inventory-grammar.md` (bumps `quilltap-core`)

v4 → v5:
- `custom-tool.types.ts` (§1.F, the inventory hunks ONLY) → `pascal/custom_tool_types.rs`;
- `placeholders.ts` → `pascal/placeholders.rs`;
- `inventory-subjects.ts`, `inventory-aggregate.ts`, `bench-inventory.ts`,
  `inventory-effects.ts` → NEW `pascal/inventory_subjects.rs`,
  `pascal/inventory_aggregate.rs`, `pascal/bench_inventory.rs`,
  `pascal/inventory_effects.rs`; `pascal/mod.rs`;
- `metadata-match.ts:95-172` (the list rule) → `pascal/metadata_match.rs`
  (**contract §5.3.4 — ONE owner; proposed: LANE P**, because the aggregate is
  its reason to exist and its rows live in the inventory corpus);
- `tool-gate.ts` (the 4th arg + the three loops) → `pascal/tool_gate.rs`;
- `custom-tools.ts` (`RosterContext.inventory`, the lazy load, `resolveParams`
  … `simulateOutcomes` — the inventory hunks) → `pascal/custom_tools.rs`,
  `pascal/roster.rs`;
- `tool-vocabulary.ts` (the five fields, the inventory arms) → `pascal/tool_vocabulary.rs`;
- `side-effects.ts:162-216` (the inventory params + the third pass) →
  `pascal/side_effects.rs`;
- `run-service.ts` → `pascal/run_service.rs`; `run-custom-handler.ts:58`
  → `tools/run_custom.rs` (`RunCustomToolContext.pending_inventory_announcements`);
- `workbench.ts` → `pascal/workbench.rs`;
- `run-custom-tool.ts` (the preamble sentence, `describeRoll`, the three
  clauses, `Reads:`/`Takes stock of:`, the inventory effect clauses) →
  `tools/run_custom.rs` + the `run_custom` entry in `definitions/data.rs`
  (byte-identity with LANE T's regen — **contract §5.3.5**);
- `custom-tools/route.ts` (the `inventory` body key ×2 + the three call
  sites) → `api/types.rs`, `api/custom_tools.rs`, `crates/quilltap-web/src/custom_tools_routes.rs:161-220`.
- Oracles/harness: `pascal-custom-tool-definition.ts` rows (+ the SPA
  sibling re-vendors the NDJSON), `pascal-custom-tools-execution.test.ts`
  rows, `pascal-tool-vocabulary.ts` rows, `pascal-run-custom.ts` rows,
  `pascal-simulate.ts` rows, `pascal-side-effects.json` + `.test.ts` step
  (+ mock), NEW `pascal-inventory-effects.ts` + `build-pascal-inventory-effects-fixture.ts`
  + `.json`, `pascal-custom-tools-discovery.test.ts` scenarios,
  `extend-pascal-run-custom-inventory.ts` + handler/route/trigger rows,
  `workbench.json` + `workbench-route-cases.json` rows; the matching
  `crates/quilltap-harness/tests/pascal_*` files + NEW `pascal_inventory_effects_equivalence.rs`.

### 5.3 Shared contracts (name-level; the unifier's checklist)

1. **KEYSTONE → T, P (the `inventory` module's public surface):**
   `inventory::pool::{load_inventory_pool, LoadInventoryPoolOptions, InventoryPool}`
   (with `resolved()`, `resolve(record)`, `resolve_id`, `owns(id)`, `own`
   records, `get(id)`); `inventory::equipped_set::read_equipped_set(chat,
   character_id, &pool) -> Vec<ResolvedInventoryItem>`;
   `inventory::resolve::{resolve_item, inventory_display_name, inventory_label,
   is_kin, kit_state, is_equippable}`; `inventory::location::{resolve_inventory_location,
   InventoryLocation { scope, mount_point_id, origin, find_item, read_items,
   create, delete, writable_mount_point_id }, location_key}`;
   `inventory::item_mutations::{create_inventory_item, update_inventory_item,
   set_inventory_item_archived, instantiate, assert_copyable, assert_kit_members}`;
   `inventory::dependents::{find_kits_stranded_by_move, find_records_anywhere,
   is_reachable_from_kit_tier, mount_scope_of}`; `inventory::item_images::{carry_inventory_item_images,
   commit_moved_inventory_images}`; `inventory::folders::normalize_inventory_folder_path_or_root`;
   `inventory::errors::{InventoryRefusal { code, message, status, details },
   InventoryCapExceededError, InventoryInvalidParentError, InventoryNotEquippableError,
   InventoryKitStrandedError, InventoryShapeError}` + the `inventory_error_response`
   mapper; `inventory::equip::{with_chat_inventory_lock, equip_items,
   equip_items_holding_lock, unequip_items, EquipChat, EquipResult { equipped,
   created, already_equipped, skipped }, UnequipResult { unequipped,
   not_equipped }}` **returning the `InventoryChange` to the caller instead of
   notifying** (v5's two-halves rule — the keystone must NOT embed an async
   notify in the in-write chokepoint; T supplies `notify_inventory_changed`);
   `db::chats_inventory::{set_equipped_inventory, get_equipped_inventory,
   normalize_equipped_inventory, find_chat_ids_carrying_inventory_id,
   remove_equipped_inventory_id}`; `services::pascal_writer::post_pascal_inventory_change(db,
   chat_id, content)` (`systemKind: 'inventory-change'`); `services::outfit_change_effects::refresh_avatar_for_appearance(db,
   who, "inventory-change")` (or the keystone's name for the rename);
   `inventory::prompt_section::build_inventory_section`; the
   `ResolvedInventoryItem` struct with `id, name, title, kind, is_archetype,
   parent_id, parent_name, item_ids, metadata, description, image_description,
   emoji, max_equipped, can_copy, image_file_id, folder, archived_at, origin
   { scope, id, name }, mount_point_id`; `schemas::inventory::{create_inventory_schema
   parse, InventoryUpdateBody}`; `characters_read` returning `canManageInventory`.
2. **T → P:** `services::inventory_change_effects::{InventoryChangeProvenance,
   InventoryChangeContext, InventoryChange, PendingInventoryChange,
   PendingInventoryAnnouncements (type), notify_inventory_changed,
   record_pending_inventory_change}`; `inventory::refs::{InventoryContext
   (trait: state, equipped_kin_count, item, scope_objects, pool, equipped_ids),
   InventoryState, resolve_inventory_ref, pick_for_gesture, inventory_context_from,
   AggregateObject, AggregateScope}` — NOTE `AggregateScope`/`AggregateObject`
   are declared in v4's `inventory-aggregate.ts` (P) and imported by `refs.ts`
   (T): put the two TYPES in `inventory::refs` (T) and let P's module re-export
   them, or P lands a stub-free `pascal::inventory_aggregate` FIRST — the
   cheapest is T owning the types; `inventory::context::{load_inventory_context,
   LoadedInventoryContext { closed_shelf }}`; `inventory::equip::pick_up_items`
   + `dropCarriedEverywhere`; `new_wardrobe_turn_containers() -> (set, map,
   inventory_map)`; `ToolExecutionContext.pending_inventory_announcements`;
   `RunCustomToolContext.pending_inventory_announcements` (`tools/run_custom.rs:505`
   is P's file — T hands P the field name).
3. **P → T:** nothing at run time; `tools/run_custom.rs`'s preamble bytes
   must equal `definitions/data.rs`'s `runCustom` description that T regens
   (`empty_roster_description_matches_data_rs`) — both lanes edit the SAME
   sentence; the unifier runs the in-crate test on the union.
4. **P ↔ the custom-tools (any-of) sibling — HUNK-disjoint in five files:**
   `custom_tool_types.rs`: P owns `InventoryRef`, the three operand arms, the
   `Roll` arm, the parameter default, the gate group's three records, the
   when's three records, the gesture, `is_inventory_effect`,
   `inventory_gesture_problems`, `validate_inventory_key`, the `$inventory`
   roll rule, the two-way `uses a ${kind} reference` sentence; the sibling
   owns `MAX_WHEN_GROUPS`, `ToolGateGroup` + the `Vec` union, `gate_groups`,
   `When::Array`, `when_groups`, `EffectCondition`, `effect_when_groups`, the
   index-in-path forms, `is_not_an_array`; the `invalid_key` arm in
   `flatten_issues` (P needs it for aggregate keys, the sibling's progress
   keys already use `superRefine` — give it to P); the FOUR "must test …"
   sentences are UNION strings (each lane writes the whole pin sentence —
   identical bytes, so the merge is textual). `custom_tools.rs`: P owns the
   inventory + aggregate blocks inside `matches_when_object` and
   `resolve_inventory_gesture`; the sibling owns `matches_when`'s array arm,
   `any_group_holds`, `matches_effect_when_group`, `collect_metadata_tested`'s
   group walk. `tool_gate.rs`: P owns the 4th arg and the three loops in
   `gate_holds`; the sibling owns `held_group_of` / `heldGroup`. `tool_vocabulary.rs`:
   P owns the five fields + `collect_state_refs`'s arm + the effect arm; the
   sibling owns `collect_group`'s group iteration — **both need
   `collect_group`**: P writes it over a single group, the sibling wraps it in
   the group loops. `run_custom.rs`: P owns `describe_aggregate` / `SCOPE_WORDS`
   / `LIST_WORDS` / the three clauses / `Reads:` / `Takes stock of:`; the
   sibling owns `describe_when`'s `(…) or (…)` rendering and `describe_when_group`'s
   extraction. `metadata_match.rs`: P (list rule). `inventory-subjects.ts`'s
   `gateNamesInventory` walks `gateGroups` — P writes it over `&ToolGate` and
   the sibling makes it iterate groups. `tool-gate-keys.ts` (`7263105c6` +
   `a53024d3c`): whoever owns `7263105c6`.
5. **P ↔ the Log sibling:** `CustomToolEffectBody::Log`, the fourth kind in
   the refine, the two effect-kind sentences (UNION strings — write the pin
   bytes), `logged` in the merge, `writes to the log`, the `effect is neither
   a complete write, a wardrobe or inventory gesture, nor a log line` skip
   text (`custom-tools.ts:1562` at the pin — the pin bytes name BOTH; each
   lane writes the pin sentence).
6. **T ↔ the Log / custom-tools siblings in `executor.rs`'s two consts and
   `tool_build.rs`'s pushes:** additive lines at DIFFERENT positions (inventory
   after `wardrobe_history`; the Log after `whisper`; `edit_custom_tool` after
   `run_custom`) — a clean textual merge. The `GET /api/v1/tools` table
   (`tools_inventory.rs`) likewise (inventory rows after `wardrobe_history`;
   the Log's after `read_conversation`; `edit_custom_tool` after `run_custom`
   — measure v4's order at the pin).
7. **T ↔ the SPA sibling:** the `inventory` realtime topic name
   (`realtime/types.rs` ↔ `core/realtime-topic-map.ts`), the tool-message
   payload keys (the nine outputs' key order ↔ `chat/tool-message.ts`), the
   `canManageInventory` flag on the character wire (keystone), the preview
   body's `inventory` key and the bench's `MockInventoryRow` shape (P ↔ the
   Workbench), `rollForm: 'inventory'` (P ↔ the library screen), the
   `inventoryReads/inventoryAggregates/equips/unequips/picksUp` wire on
   `references` (P ↔ the run dialog).

---

## 6. Traps

1. **Three different "no match" verbs, three different refusals.** `inventory_read`
   / `inventory_equip` / `inventory_pick_up` consult `closedShelfMessage`
   FIRST (⟨102a⟩); `inventory_update` / `inventory_archive` / `inventory_give`
   do NOT (a shared/closed-shelf object there is read-only or not-found by
   the plain rule). A port that routes all six through one helper diverges.
2. **`resolveInventoryRef`'s rounds are pool-wide, not per item** (`cad017599`'s
   second describe): the TITLE round is skipped only when NO item's title
   matches; an untitled instance named by its kind name is found by the NAME
   round even when a titled sibling exists. Rust: three passes over the
   candidates, each returning on the first non-empty hit set.
3. **`tierOf` defaults to `'project'`** for any origin scope other than
   `group` / `general` (including a MISSING origin). Keep the fall-through.
4. **The `known` escape hatch in `equipItems`** (`equip.ts:163,278`) exists
   for the job child's unreadable buffered file. In v5 the file IS readable in
   the same write — but `inventory_create` still passes `known: [item]`;
   port the option (it is the oracle's shape) even though the pool will find
   the file.
5. **The second `loadToolInventory` in `inventory_update`** (`:71`) re-reads
   the pool to report the resolved `after` — in v5 do it in the same closure
   (a second mount read), never after commit (the summary must reflect the
   committed file).
6. **Give = move FIRST, then drop, then post** (the ledger's §5.3 note). A
   refused move leaves the giver carrying it everywhere. `dropCarriedEverywhere`
   takes each chat's lock in turn and never re-enters — in v5 the writer
   thread IS the lock; the per-chat `withChatInventoryLock` collapses to
   sequential rewrites inside one write. `alsoChatIds = [context.chatId]`
   guarantees the CURRENT chat is rewritten even when the `$like` prefilter
   misses it.
7. **`findChatIdsCarryingInventoryId`'s `$like '%id%'` prefilter** then a
   JS filter — the prefilter is an optimisation; v5 may scan all chats with
   an `equippedInventory` (as `remove_equipped_item_from_all_chats` does at
   `chats_outfits.rs:190`) — the set of chat ids returned is the same; the
   DEBUG's `chats:` count must match (it counts the FILTERED list).
8. **The pending map nets BY ID, keeps provenances in first-seen order, and
   `flush` uses `provenances[0]`** for the re-notify's provenance — but the
   re-notify's ctx carries NO map, so it posts now; and the `[InventoryEffects]
   Drained` DEBUG reports EVERY provenance. A give STRIKES the item from the
   giver's entry before posting its own line.
9. **`notifyInventoryChanged` with a map and `announce:true` returns BEFORE
   the avatar refresh** — the refresh happens at flush time, once per
   character; with no map (the operator route, the trigger runner, the flush
   itself) it refreshes at once. Arrival defaults pass `announce:false` and
   no map → refresh only (keystone's path, same function).
10. **The orchestrator drains the inventory map in `finally` — AFTER the
    saved turn — but the wardrobe set BEFORE the sentinel** (`:1753` vs
    `:2043`). Two different positions; the `handleTurnSkip` early return
    still drains the inventory (it is inside the `try`). A v5 port that
    drains both at `:3651` posts Pascal's line BEFORE the tool-save row —
    the exact bug `102a23648` fixed.
11. **`inventoryToolFlags(undefined)` is ON** — every v4 caller that omits the
    positional (the help chat's `buildTools` `:237-249` with eleven args)
    offers the nine. Pin it, do not "fix" it.
12. **The text-block coercion skips the generic branch** for ANY
    `inventory_*` tool — so `title="true"` and `name="42"` stay strings, but
    ALSO `equipped="yes"` stays the string `"yes"` (then fails Zod → `Invalid
    input for inventory_list.`). Both are observable.
13. **`convertTextBlockToToolCallRequest`'s `refs` collapse puts `refs` LAST**
    in the arguments object (`{ ...rest, refs }`) and drops BOTH `ref` and
    `refs` from `rest`; `String(ref)` on a boolean-coerced value cannot occur
    (refs are never coerced) — but a `ref` that reached the generic branch
    cannot either (the inventory branch `continue`s). Key order matters for
    the parsers' exact-equality rows.
14. **`InventoryRefSchema`'s `$inventory` max is `MAX_INVENTORY_REF_LENGTH +
    120` = 320** (`:192`) — Zod 4.6.7 `.max()` counts UTF-16 code units
    (memory note `carrier-oracle-measurement-gotchas` says code points for
    `.length()`; `.max()` on a string is `length` = UTF-16 units — measure
    with an astral title at 319/320/321).
15. **The union ORDER of the four operand schemas puts `InventoryRefSchema`
    LAST** — a strict object with both `$state` and `$inventory` keys fails
    `StateRefSchema` (strict) and `InventoryRefSchema` (strict) → the
    `invalid_union` message tree; v5's `parse_*_operand` chains must try in
    the same order and compose the same issue list (the definition family
    compares the SENTENCE).
16. **`resolveInventoryValue` type-checks `typeof value === typeof fallback`**
    — a metadata `3` against a `"1d4"` fallback falls back (so the "Measure
    Twice" render of `length_feet` works only because the placeholder path
    does NOT type-check). And a NaN/Infinity number falls back; `-0` is a
    number → kept (JS `typeof -0 === 'number'`) → renders `0` via the
    renderer's `String(-0)` = `"0"`.
17. **`stateOf` ignores archetypes and archived items even when the matches
    include them** (`includeArchetypes` callers) — `equippedKinCount` DOES
    include archetypes for the first match (to count by the archetype's
    name) but counts only non-archived, non-kit, equipped kin.
18. **`scopeObjectsOf` uses `item.title || item.name`** (no trim) while
    every other title read uses `title?.trim() || name` — a whitespace-only
    title reaches `titles` lists as whitespace. Reproduce.
19. **`aggregateValue` folds with `Math.min(...numbers)`** — spread over a
    list; identical results for any finite list; `sum` is left-to-right
    (`reduce`). `count` fold counts OBJECTS with ≥1 finite number, not
    numbers. An empty `names` list for an EMPTY scope compares as a fact
    (`neq` holds); an absent FOLD declines with `the character has no such
    metadata key` — two different decline reasons from the same table.
20. **`inventoryStateHolds` reports NO decline reason on an ordinary
    mismatch** (only the null-context one) — the `Custom tool inventory test
    did not match` DEBUG fires ONLY for null context, while the
    `Custom tool inventory state test` DEBUG ALWAYS fires with `holds`. The
    aggregate path logs BOTH (`did not match` with the table's reason, then
    the `test` line with `value`).
21. **The gate logs nothing for inventory** (no decline hook in `gateHolds`),
    and `evaluateToolGate` is pure; the DEBUG lines around the roster's lazy
    load are `custom-tools.ts`'s (`CONTEXT = 'pascal.custom-tools'` — measure
    the exact value), with the U+2019 apostrophe in `invoker’s`.
22. **`prepareRoll` on an `$inventory` roll records `notation` = the
    RESOLVED string** (e.g. `"2d6"`), not the `$inventory` path; `rollForm`
    is `'dice'` in the run result; the Workbench library alone says
    `'inventory'`.
23. **`resolveInventoryGesture` normalises `equip` to a boolean ONLY on
    `pick_up`** and OMITS the key otherwise — the serialized `ResolvedEffect`
    (compared as bytes in the execution family) has `{ action, items }` or
    `{ action, items, equip }`.
24. **`applyInventoryEffects`'s `after` for `pick_up` with `equip:false` is
    `before`** (unchanged) and the record still carries `created: true`; for
    `equip`, `after = sort([...before, ...equipped ids])` INCLUDING ids that
    `equipItems` reports as `equipped` but were created for a kit (created
    set ⊆ equipped). `previous` is the SAME sorted `before` for every item of
    one gesture. Ids sort by JS default (UTF-16) — UUIDs are ASCII, fine.
25. **The re-read after a landed gesture happens only when something was
    attempted** (`continue` on an empty id list skips it) and a re-read
    failure `break`s the loop (later gestures silently skipped — only the
    WARN says so).
26. **`describeInventoryChange`'s plural rule is on the LAST whitespace word**
    and `indefiniteArticle` is on the FIRST letters of the WHOLE name (not
    the first word) — `"old axe"` → `an old axe`; `"unicorn horn"` → `a`;
    `"Eustace"` → `a` (the `eu` rule); `"hour-glass"` → `a` (no silent-h
    rule). Case-insensitive regexes.
27. **`loadRunInventory` reads the bag ONLY when the vocabulary names it** —
    a definition that quotes `{{inventory.x.title}}` in its MESSAGE counts
    (placeholders are reads), one that only has an `$inventory` parameter
    DEFAULT counts (the `$inventory` walk), but a `{{inventory…}}` inside the
    LLM PROMPT counts too (`collectPlaceholders(definition.llm.prompt)`).
    And a run with `characterId` null never reads. The manual-run route's
    `asCharacterId` → the same `runCustomTool` → the same rule.
28. **The bench's mock has no `pool`/`equippedIds`** — `applyInventoryEffects`
    is never reached from the bench; but `loadInvokerInventory` returns the
    given `ctx.inventory` even when it is a MOCK — the route-run path never
    passes one. `mockInventoryContext`'s `item()` is always null (headline 9).
29. **`[InventoryTransfers] Inventory object transferred`** — the line text
    moved from `[InventoryTransfers v1]` to `[InventoryTransfers]` when the
    body moved into `lib/inventory/transfers.ts`; if the keystone ported the
    route body INLINE with the `v1` prefix, this lane re-homes the line.
30. **`inventory_create`'s `metadata` schema refuses nested objects and
    arrays** (`record<string, primitive|null>`) — the `inventory_update`
    merge deletes on `null` and keeps the RECORD's other keys (an instance's
    overrides merge over its OWN file, not the resolved archetype view —
    `record.metadata` is the raw file's).
31. **Zod `.refine` on `inventory_update`** fires AFTER the object parse; a
    body with only `ref` fails validation → the handler's generic invalid
    text (the refine message `Name at least one field to change` never
    reaches the model — it is swallowed by `validate*Input`'s null).
32. **`loadInventoryContext` with `chatId: null`** (the Workbench library
    path, `RosterContext` without a chat) is never called — the roster
    declines before loading; but `loadInventoryContext` itself supports it
    (`equipped = []`, `projectId = null`, shelf open). Pin the signature.

---

## 7. Tiering suggestion

**Tier 1 — must land (the model-visible / spine surface):**
- LANE T: the nine definitions in the catalog (76 at the pin; 72 + carve
  alone); `inventory_tool_flags`; the nine handlers + the shared module +
  the table; `refs` / `context` / `roster_gate` / `inventory_change_effects`
  (describe, notify, record, strike, flush); the `equip.rs` hunks
  (`pick_up_items`, `drop_carried_everywhere`, `known`, `assert_shelf_reach`)
  + `OFF_ROSTER`; the third turn container + the executor arm + the
  `finally` drain; the builders / parsers / prompts; the tools-list rows +
  availability; the five off-sites; the realtime topic + hint.
- LANE P: `InventoryRef` on the four unions + the roll + the default; the
  gate's `inventory` / `inventoryCount`; the when's two subjects; the gesture
  + `is_inventory_effect` + the sentences; the load-time rules; the
  placeholders; `inventory_subjects`; the roster's lazy load; the deal's
  threading (`resolve_params` → `simulate_outcomes`); `resolve_inventory_gesture`;
  `inventory_effects` + the third pass in `side_effects`; `run_service`'s
  load; the preamble sentence + `describe_roll` + the two when clauses +
  `Reads:` + the effect clauses; the bench's `inventory` body on preview /
  audit (the Workbench sibling's beats depend on it).
- KEYSTONE (named for completeness): bug 253's call site, the two repository
  methods, `post_pascal_inventory_change`, `refresh_avatar_for_appearance`.

**Tier 2 — should land in the same round (model-visible but additive):**
- LANE P: `inventoryAggregate` whole (`inventory_aggregate.rs`, the gate's
  and when's third record, the list rule in `metadata_match.rs`, the
  `AggregateKeySchema` + `invalid_key` sentence arm, `SCOPE_WORDS` /
  `LIST_WORDS` / `describe_aggregate` / `Takes stock of:`, the vocabulary's
  `inventoryAggregates`, the bench rows' `metadata`, `scope_objects` on both
  contexts). Reason it is not Tier 1: it rides the any-of sibling's
  `gate_groups` walk in `gateNamesInventory` and the readers; land it after
  the any-of or with the group readers stubbed as single-group (no stubs in
  code — the single-group `ToolGate` IS a valid reader until the sibling's
  union lands).
- LANE P: `rollForm: 'inventory'` (one enum arm).
- LANE T: `inventory::transfers` as the route's and the give's ONE path (byte
  parity on the route is the keystone's family; the extraction is a
  refactor whose only observable is the log prefix); the Almanack line.

**Tier 3 — explicit typed refusals: NONE.** Every v4 surface in this slice
has a v5 home. The two NO-PORTs are ratifications, not refusals:
`child-repositories-proxy.ts` (no job child) and `cad017599` (test-only;
its cases mirrored as rows — §4.2).

---

## 8. Open questions / rulings owed / ledger disagreements

1. **Ledger `e1872c94f` row says the catalog goes "→ **72**".** At the pin
   `ALL_TOOLS` has 76 keys (`tool-definitions-snapshot.test.ts`, measured) —
   the Log's three and `edit_custom_tool` landed after the row was written.
   The regen is ONE event for four lanes; the unifier's count is 76.
2. **Ledger `e1872c94f` row quotes the effect-kind sentences as `an effect is
   exactly one of a write (target + value), a wardrobe gesture, or an
   inventory gesture` / `an effect needs target + value, wardrobe, or
   inventory`.** At the pin (`custom-tool.types.ts:1000,1003`) they read
   `…, an inventory gesture, or a log line` / `…, inventory, or log`
   (`e4a60ad9e`). Likewise the `resolveEffects` skip text (`custom-tools.ts:1562`)
   and `side-effects.ts:215`'s merge list. Each lane writes the PIN bytes.
3. **Ledger `102a23648` row: "arrival defaults are ungated holds only if
   arrival uses `operator` provenance or bypasses `assertShelfReach` —
   measure at port time."** Measured: `assertShelfReach` is called from
   `equipUnserialized` (`equip.ts:319`) and `pickUpItems` (`:481`) and returns
   at once for `provenance === 'operator'` (`isCharacterGesture`); the
   keystone's `apply-defaults.ts` (arrival) is the only other `equipItems`
   caller — whether it passes `'operator'` is the KEYSTONE survey's to
   confirm (the roster-gate header says arrival is ungated; `cad017599`'s
   `CTX` uses `provenance: 'operator', announce: false` as the arrival shape).
4. **Ruling owed — the bench's always-null `item()`** (headline 9): keep v4's
   asymmetry (placeholders render as written, `$inventory` falls back, the
   gate and counts answer) or let the Workbench sibling ask v4 for a richer
   mock? Recommend: port as is; record as a divergence candidate for v4.
5. **Ruling owed — the help chat's slate** (`help-chat/orchestrator.service.ts:237-249`
   passes ELEVEN args to `buildTools` → `canManageInventory` undefined → the
   nine inventory tools ARE offered to the help chat's native slate, exactly
   as the two wardrobe flags are; its text-block path passes `false`). Port
   faithfully (`can_manage_inventory: true` on `help_chat/orchestrator.rs:766`)
   or file a v4 bug? Recommend: port + file (the help chat has no character
   to act for — the executor would answer `The inventory tools need a
   character to act for.` on every call).
6. **Ruling owed — `transferInventoryItem`'s two error-shape moves** (§1.D):
   the id-collision `badRequest` → `InventoryShapeError` (still 400 through
   `inventoryErrorResponse`, but now with the refusal envelope's `code:
   'INVALID_SHAPE'` + `details.itemId`), and the source-delete `serverError`
   → a plain `Error` (500 through the mapper). Body bytes CHANGE at the
   route; the keystone's route family must regen at the pin — is that the
   keystone's or this lane's red? (Proposed: this lane's, as the extraction's
   owner.)
7. **Ruling owed — `AggregateScope` / `AggregateObject` ownership** (§5.3.2):
   v4 declares them in P's `inventory-aggregate.ts` and imports them in T's
   `refs.ts`; the cheapest cut moves the two types into `inventory::refs`
   (T) with P re-exporting. Approve the move or sequence P before T.
8. **Ruling owed — `execute_custom_tool`'s 13th positional vs an overrides
   struct.** Seven v5 call sites (measured). A struct is cleaner; a positional
   is the smaller diff for the two sibling lanes editing the same file.
   Recommend positional (the any-of and Log lanes do not touch the signature).
9. **Ruling owed — which lane owns bug 253's call site** (§2.4 last row):
   proposed the KEYSTONE (it writes `build_inventory_section`); if the
   keystone's order declines it, LANE T takes it as a one-hunk rider with
   the `build_context_tier3_equivalence` row.
10. **Measure at port time (not rulings):** (a) the exact v4 `CONTEXT`
    constant for the roster's DEBUG lines in `custom-tools.ts` (`'pascal.custom-tools'`?);
    (b) whether `revealOdds: false` precedes `Reads:` (§1.F — it does at
    `:267-268`, `continue`); (c) `MockInventoryRowSchema`'s 400 bytes for an
    unknown key through the route; (d) Zod `.max(320)` on an astral-plane
    ref; (e) the `GET /api/v1/tools` table size at the pin (58 rows) and the
    Log/edit rows' positions; (f) whether `pascal-side-effects.test.ts`
    needs `applyInventoryEffects` mocked for the existing corpus to stay
    green at the pin; (g) the keystone's name for `refreshAvatarForAppearance`
    and `postPascalInventoryChange` in Rust; (h) the orchestrator's SECOND
    tool-context copy (`orchestrator.rs:3621`?) that must also receive the
    third container.
11. **Disagreement with the brief's file list:** `lib/pascal/tool-draft.ts`
    and `tool-draft-inventory.test.ts` are the Workbench's CLIENT model (v5
    `apps/web/src/app/pascal/tool-draft.ts`) — the SPA sibling's, not this
    slice's; `lib/pascal/tool-gate-keys.ts` is NEW at `7263105c6` (the title
    lint) with only an `a53024d3c` group-walk hunk — not this slice's either.
12. **`cad017599` NO-PORT — VERIFIED and recommended**, with its seven cases
    mirrored as `pascal_run_custom_handler_equivalence` rows over the extended
    fixture (§4.2), ratified at the round's unification. Its one subtle claim
    — the title round wins pool-wide — is ALSO pinned in `inventory_refs_equivalence`.
