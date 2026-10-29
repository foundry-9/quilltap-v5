# Survey — Storyteller mode (server + SPA) and declared tool effects (`b15fe9404` + `946a50a04` + `7d7e38d2f` + `8becfc6ce`)

Read-only survey, 2026-10-29. v5 `main` @ `f6cdceedd` (clean; core 0.0.1379, host 0.0.217, web 0.0.251,
cli 0.0.35, tauri 0.0.9, SPA 0.5.907). v4 read at the round pin **`236c04e35`** (code-identical to
`7789ebed6`, the detached worktree `/tmp/qt-v4-pin-setupphase-7789ebed6`; every `path:line` below that says
"pin" is that tree) and per commit through `git show`. Oracle baseline `322a5488e`.

**Commits in scope:**

| sha | subject | date | class |
|---|---|---|---|
| `b15fe9404` | Feature: Storyteller mode and declared tool effects | 2026-10-22 | PORT-NEW (Storyteller) + PORT (effects replace `DESTRUCTIVE_TOOL_NAMES`) + two D23 moves (the keystone's P4.D341) |
| `946a50a04` | Feature: drag storyteller bids into running order | 2026-10-22 | PORT-NEW rider (`keepOrder`); server 2 hunks, the rest SPA/help |
| `7d7e38d2f` | Test: regression tests for toast stacking and storyteller keepOrder (#114) | 2026-10-26 | tests only — spec input for `planStorytellerRun` (+ a toast test P4.D338 already covers) |
| `8becfc6ce` | Chore: dead code sweep for v4.11 release (#113) | 2026-10-26 | NO-PORT? — deletes `isComputedTool` (never port it) + three unrelated exports |

**Lanes informed:** **P4.D343** (Storyteller SERVER + declared tool effects + the `tool-build.test.ts`
re-aim; wave 1, stacks on `KEYSTONE`), **P4.D344** (Storyteller SPA; wave 0). Also constrains **P4.D341**
(the keystone — the column plumbing this lane depends on), **P4.D348** (the turn spine — the same
`orchestrator.rs` / `spine.rs` regions), **P4.D347** (the context builder — `build_context.rs`'s trailing
list), **P4.D345** (Pascal intervention — `trigger_runner.rs`), **P4.D359** (the SPA riders — the Inspector),
**P4.D360** (vendor — `help/`, `docs/v4/`, the export schema).

## Headline findings (a planner must not miss these)

1. **The pin's Storyteller is NOT `b15fe9404`'s shape.** `a8214b253` (2026-10-25, P4.D348's) refactored
   the spine three days later: the storyteller hold lives in `lib/services/chat-message/turn-holds.service.ts`
   (`resolveTurnHold` → `'storyteller'`, `finishHeldPost` → `storytellerHold`), the ceiling in
   `turn-tools.service.ts` (`resolveTurnEffectCeiling`, `TurnToolSlate.effectCeiling`), the skip offer in
   `turn-preparation.service.ts:117-146` (`isStorytellerTurn` / `isBid`), the pass handling in
   `turn-outcome.service.ts:75-125`. **Port the pin's bytes**, and expect P4.D343 and P4.D348 to meet in
   `crates/quilltap-core/src/services/orchestrator.rs` and `crates/quilltap-host/src/spine.rs` — §7 below
   gives a function-level ownership proposal and two named contracts. Three `b15fe9404` log lines do not
   survive to the pin (§4).
2. **Ledger corrections.** (a) The row says `applyEffectCeilingToTools` lives in `lib/tools/effects.ts`; at the
   pin it is a private fn in `lib/services/chat-message/streaming.service.ts:152-185`. (b) The row's
   "`storytellerBid` stripped from every transcript" correction stands: no hunk strips anything — the column is
   simply never copied into model messages (`lib` grep, §1.1.9). (c) `dry-scope.ts:10` names
   `bid.service.ts`, which does not exist — the bid lives in `orchestrator.service.ts:271 runStorytellerBid`.
   (d) The row's "`STORYTELLER_CONCURRENCY_PER_PROFILE=4` … SSE `participantId` + `concurrent:true`" is right;
   the `llm_logs` index comment "per-run budget sum" is copy-paste (confirmed, `add-llm-logs-storyteller-round-id-column.ts:103`).
3. **`toolChoice: 'none'` reaches the wire on FOUR v4 providers only** (nanogpt `body.tool_choice =
   params.toolChoice ?? 'auto'`, deepseek / z-ai `if (params.toolChoice) body.tool_choice = …`,
   openai-compatible `tool_choice: params.toolChoice ?? "auto"`); OpenRouter hard-codes `'auto'`; Anthropic,
   Google, OpenAI, Ollama, Grok never read it (the spec says so too, `storyteller-mode.md:452-455`). v5's
   `StreamParams` has NO `tool_choice` (deferred, `model/stream.rs:490`) and `streaming_provider.rs:200`
   pins `tool_choice: None`; `chat_completions.rs:676` already defaults `"auto"` for the openai-compatible
   body and passes it through for deepseek (`:835`) / z-ai (`:913`). P4.D343 must add the field and prove
   the per-provider wire bytes under `request_builder_equivalence` (§5.4).
4. **The withheld set widens 3 → 5 and the P4.D234 INFO retires.** v5 `tool_build.rs:757` still carries
   `DESTRUCTIVE_TOOL_NAMES = [doc_delete_file, doc_delete_folder, discard_mail]` + the INFO `Autonomous room:
   destructive tools filtered from per-turn list` (`:800`); at the pin the gate is `resolveTurnEffectCeiling`
   → `lowerToolEffect(storytellerCeiling, 'write')` + `applyEffectCeilingToTools` dropping every tool whose
   `maxEffectOfTool` is `destroy` (`delete_annotation`, `doc_delete_blob` join the three) with the DEBUGs
   `Autonomous room: destructive tools withheld by effect ceiling` / `Effect ceiling applied to tool list`, and
   the executor REFUSES a destroy call regardless (`tool-executor.ts:343-371`). `tool_build_equivalence.rs:312`
   greps the retired INFO and `:317` names a `p4d234_destructive_autonomous_mail_only` row whose premise
   ("discard_mail is the ONE destructive tool") no longer holds once `delete_annotation` (Scriptorium, always
   on) is `destroy` — re-measure, re-pin.
5. **`tool-build.test.ts` is IMPORT-DEAD at the pin by a STATIC import** (`:54 import { DESTRUCTIVE_TOOL_NAMES }
   from '@/lib/tools/destructive-tools'`) — jest fails module resolution, the whole case dies (not a silent
   `undefined`). The re-aim: drive v4's REAL `buildTools({ …, effectCeiling: 'write' })` for the autonomous
   not-allowed rows (the ceiling moved INSIDE `buildTools` at the pin) and add `effect_ceiling:
   Option<ToolEffect>` to v5's `BuildToolsInput` — not a hand copy of the set.
6. **The keystone must splice, not append, two chat columns.** `chats_read.rs:82-107` is a fixed 103-column
   SELECT in generateDDL order with positional marshaling (`isPaused` at index 18, `:324`); `turnMode` /
   `storytellerInviteFilter` sit between `isPaused` and `isManuallyRenamed` in v4's DDL (`DDL.md` hunk), so
   every index ≥ 19 shifts by 2 and the `prepare_select` "last column NULL" idiom (`:125-139`,
   `equippedInventory`) cannot cover a mid-list column on a pre-column table. `chats_messages_read.rs:84-91`
   APPENDS by rule (`pascalMeta` 44, `routeTrail` 45) → `storytellerBid` at 46 regardless of schema position.
   `llm_logs.rs:426` `LOG_COLUMNS` + the insert list `:552-573` gain `storytellerRoundId` after
   `autonomousRunId` (indices 7+ shift). All three are P4.D341's ("row-struct / reader / writer plumbing");
   P4.D343 consumes them (contract K in §7).
7. **Two AsyncLocalStorage twins are needed and v5 has a precedent for neither being ambient.**
   `round-context.ts` (the `llm_logs.storytellerRoundId` stamp) and `dry-scope.ts` (every repository write
   discarded during a bid). v5 threads context explicitly (`LogContext { autonomous_run_id }`,
   `llm_logging.rs:42-49`; `turn_control.rs:28-39` "No `AsyncLocalStorage` twin"). The round id becomes a
   second `LogContext` field threaded through `ProcessMessageInput.log_context`; the dry scope has NO v5
   analogue (no repository proxy) — it must be a **ruling** (§10 R-1): either a bid-mode flag on the four
   write sites the bid build reaches (the Core-whisper post, recall delivery — already `purpose:'bid'`-gated
   in v4 —, the inform consume, the raised hand, the cadence posts, `forceToolsOnNextMessage` spend) or a
   `Db` write-refusing wrapper. The spec's own list of what the scope caught live: "the Commonplace whisper,
   the scene-cache update and the stale-whisper sweep" (`storyteller-mode.md:447`).
8. **`turnMode` is PATCH-ONLY and rides BESIDE `chat` on the PUT.** v4 writes it only through
   `ChatsRepository.setTurnMode` (`'rotation'` stored as NULL), the PUT body is `{ chat: {...}, turnMode }`
   (`chatUpdateRequestSchema.turnMode`, `schemas.ts` hunk), leaving storyteller mode runs
   `abandonStorytellerRound(chatId,'abandoned')`. v5's `Request::ChatUpdate` (`types.rs:383`) already carries
   `concierge_state` as a `double_option` sibling — `turn_mode` follows that exact shape (contract W1);
   `storytellerInviteFilter` is an ordinary `chat` bag boolean (`salon.rs:2039-2049`'s non-nullable list).
   v5's web crate has NO chat PUT route (dispatch-only), so there is no REST arm to add.
9. **`storyteller_rounds` is the FIRST table with a server-held transient row set that v5 must create at
   boot AND truncate on `replace` restore but never carry**: `delete-service.ts:60-65` adds it to
   `clearFormat3Entities`' main list → `delete_all.rs:201-228` `FORMAT3_MAIN_TABLES` (+ its pinned test at
   `:643-657`); it is in no backup / `.qtap` carrier (the `.qtap` schema gains only `turnMode` /
   `storytellerInviteFilter` on the chat and `storytellerBid` on the message, §1.1.14).
10. **Model-visible bytes are all in two pure modules** (`lib/storyteller/prompts.ts` — the bid section, the
    plan section, its re-roll form, the four tier allowances; `lib/tools/effects.ts:226-244` — the ceiling
    refusal) plus four computed-effect notes appended to tool DESCRIPTIONS under a lowered ceiling
    (`state-tool.ts:130`, `edit-custom-tool-tool.ts:106-107`, `doc-open-document-tool.ts:89`,
    `run-custom-tool.ts:371`). Every byte is quoted in §1.1.4 / §1.1.5 — tier-1 exact families, no mocking.
11. **Rulings owed (§10):** R-1 the dry scope's v5 mechanism; R-2 plugin tool effects (v5 has no plugin
    tools — `pluginToolEffects` is always empty; record as NO-COUNTERPART or carry the map); R-3 the board
    timeout + concurrency constants as v5 host cadence; R-4 the SPA drag library (`@dnd-kit` ×3 in v4 — v5
    has no `@angular/cdk`; a dependency or a hand-rolled sortable); R-5 whether `STORYTELLER_BID` rows with
    a dry-scoped `messageId: null` count toward chat token aggregates (v4: they don't — the bid never hits
    the finalizer).

## 0. The span for this slice, in one paragraph

`b15fe9404` (dev.58, parent `036b32a78`) lands two things in one commit. Part 1 gives every tool a declared
**effect** (`none | read | write | destroy`; undeclared = `write`) beside its definition, collects them in
`BUILT_IN_TOOL_EFFECTS`, and replaces the curated `DESTRUCTIVE_TOOL_NAMES` with an **execution-time ceiling**
(`ToolExecutionContext.effectCeiling`, checked FIRST in `executeToolCallWithContext`) plus a list filter that
only spares the model offers it cannot use. Part 2 is **Storyteller mode**: a per-chat `turnMode`
(`rotation` NULL / `storyteller`) under which an operator post is recorded and, instead of a reply, opens a
**round** — every present LLM seat is asked for a **bid** in parallel (a full in-character call built by the
ordinary turn path with `purpose:'bid'`, the bid section trailing, tools offered with `toolChoice:'none'`,
run inside a **dry scope**, logged `STORYTELLER_BID`), the bids stream to the send as board frames, and the
operator chooses who speaks (in order / all at once / let it lie), each chosen turn running under its bid's
tier ceiling with a trailing plan section and storing the bid on its message. `946a50a04` (dev.61) adds
`keepOrder` so the operator's dragged order replaces the rotation's. `7d7e38d2f` ships the three-case
`plan-run-order.test.ts`. `8becfc6ce` deletes the unreferenced `isComputedTool` from `effects.ts`. Between
`b15fe9404` and the pin, `a8214b253` moved the storyteller hooks into the refactored phase modules (§1.1.6)
and `bdd09b75b` moved the trailing-section assembly into `lib/chat/context/turn-trailing.ts:56-70`, where
`storyteller` is LAST (§1.1.7). No model-visible storyteller string changed after `b15fe9404` except the
`946a50a04` DEBUG and help text.

## 1. Per commit, the shipped hunks (at the pin unless a commit is named)

### 1.1 `b15fe9404` — 194 files (+5,802 / −268); production 172 files

#### 1.1.1 `lib/tools/effect-types.ts` (NEW, 106 lines — whole file, import-free)

- `:16` `type ToolEffect = 'none' | 'read' | 'write' | 'destroy'`; `:18` `TOOL_EFFECTS` in that order;
  `:20` `TOOL_EFFECT_RANK = { none: 0, read: 1, write: 2, destroy: 3 }`; `:23`
  `UNDECLARED_TOOL_EFFECT: ToolEffect = 'write'`.
- `:30-33` `ToolEffectLookups { customToolEffects?: ReadonlyMap<string, ToolEffect> }`.
- `:41-51` `ComputedToolEffect` — a callable `(input, lookups?) → ToolEffect` with `computed: true`,
  `maxEffect`, and `usesAllowedUnder(ceiling) → string | null` (null when `TOOL_EFFECT_RANK[ceiling] >=
  TOOL_EFFECT_RANK[maxEffect]`, else `notes[ceiling] ?? null`).
- `:65-86` `computedToolEffect({ schema, resolve, maxEffect, notes? })` — `schema.safeParse(input ?? {})`;
  a FAILED parse counts as `UNDECLARED_TOOL_EFFECT` (`'write'`), never the resolver.
- `:88-90` `isComputedToolEffect(spec) = typeof spec === 'function'`; `:93-95` `maxToolEffect(a,b)` (`>=` keeps
  `a` on a tie); `:98-100` `isWithinCeiling(effect, ceiling) = rank[effect] <= rank[ceiling]`; `:103-106`
  `lowerToolEffect(ceiling | undefined, other)` → `other` when no ceiling, else the lower (`<=` keeps the ceiling).

#### 1.1.2 `lib/tools/effects.ts` (NEW, 244 lines at the pin — `8becfc6ce` removed `isComputedTool` `:216-220` of the original)

- `:104` `export * from './effect-types'`.
- `:107-182` **`BUILT_IN_TOOL_EFFECTS` — the full table** (71 entries, keyed by `function.name`, from each tool
  file's `xxxToolEffect` export; the per-file lines are in the grep below). Grouped by effect:
  - `none` (3): `rng` (`rng-tool.ts:134`), `submit_final_response` (`:89`), `whisper` (`:56`).
  - `read` (29): `ask_carina` (`:78`), `describe_image` (`:79`), `doc_close_document` (`:62`), `doc_focus`
    (`:96`), `doc_grep` (`:104`), `doc_list_blobs` (`:65`), `doc_list_files` (`:91`), `doc_read_blob` (`:69`),
    `doc_read_file` (`:94`), `doc_read_frontmatter` (`:70`), `doc_read_heading` (`:82`), `help_navigate`
    (`:86`), `help_search` (`:101`), `help_settings` (`:77`), `inventory_list` (`:87`), `inventory_read`
    (`:68`), `list_images` (`:90`), `list_mail` (`:42`), `log_grep` (`:66`), `log_read` (`:92`),
    `project_info` (`:119`), `read_conversation` (`:90`), `read_mail` (`:52`), `request_full_context` (`:58`),
    `run_sql` (`:103`), `search` (`search-scriptorium-tool.ts:293`), `self_inventory` (`:640`),
    `terminal_list` (`:39`), `terminal_read` (`:78`), `wardrobe_history` (`:76`), `wardrobe_list` (`:168`),
    `wardrobe_read` (`:141`), `search_web` (`web-search-tool.ts:91`).
  - `write` (30): `attach_image`, `doc_copy_file`, `doc_create_folder`, `doc_insert_text`, `doc_move_file`,
    `doc_move_folder`, `doc_str_replace`, `doc_update_frontmatter`, `doc_update_heading`, `doc_write_blob`,
    `doc_write_file`, `generate_image` (`image-generation-tool.ts:141`), `inventory_archive`,
    `inventory_create`, `inventory_equip`, `inventory_give`, `inventory_pick_up`, `inventory_unequip`,
    `inventory_update`, `keep_image`, `log_create_entry`, `send_mail`, `upsert_annotation`, `wardrobe_archive`,
    `wardrobe_create`, `wardrobe_rollback`, `wardrobe_take_off`, `wardrobe_update`, `wardrobe_wear`.
  - `destroy` (5): `delete_annotation` (`:68`), `discard_mail` (`:51`), `doc_delete_blob` (`:53`),
    `doc_delete_file` (`:68`), `doc_delete_folder` (`:68`).
  - **computed** (4, all `maxEffect: 'write'`): `state`, `edit_custom_tool`, `doc_open_document`,
    `run_custom` — §1.1.4.
  - **Note the key for the Scriptorium search is `search` and for web search `search_web`**; v5's catalog
    keys them the same (`tools/definitions/data.rs`). `inventory_archive` is `write` (archival, not deletion).
- `:185-188` `ToolEffectContext extends ToolEffectLookups { pluginToolEffects?: ReadonlyMap }`.
- `:191-193` `toolEffectSpecOf(name)` (own-property lookup); `:199-203` `effectOfToolCall(name, input, ctx)` —
  computed spec gets `(input, ctx)`; absent from the table → `ctx.pluginToolEffects?.get(name) ??
  UNDECLARED_TOOL_EFFECT`; `:210-214` `maxEffectOfTool(name, ctx)` — `spec.maxEffect` for computed;
  `:217-223` `highestToolEffect(iter)` (`none` for empty).
- `:226-231` **the model-visible refusal** (the tool RESULT text, `formattedText` AND `error`):
  ``Not run: `${name}` is a ${effect} call, and this turn allows only ${ceilingPhrase(ceiling)}. Nothing was changed. Carry on without it.``
  with `ceilingPhrase` `:233-244`: `none` → `tools with no effect (no lookups, no changes)`; `read` →
  `lookups (read-only tools)`; `write` → `lookups and changes, not deletions`; `destroy` → `any tool`.

#### 1.1.3 The 71 tool files (+4 each) — `export const xxxToolEffect: ToolEffectSpec = '<effect>'` appended after each definition; `lib/tools/destructive-tools.ts` DELETED (31 lines; its three names are now `destroy` entries — `discard_mail` with the comment "Post Office — deletes a letter through the same GC chokepoint as doc_delete_file" lost).

#### 1.1.4 The four computed effects — exact bytes of the per-ceiling notes (appended to the tool's `description` as `\n\n<note>` by `applyEffectCeilingToTools`, §1.1.6)

- `state-tool.ts:125-132`: `resolve: input.operation === 'fetch' ? 'read' : 'write'`; `maxEffect 'write'`;
  `notes.read` = `This turn allows only \`operation: "fetch"\`; \`set\` and \`delete\` will be refused.`
- `edit-custom-tool-tool.ts:101-109`: `resolve: input.action === 'schema' ? 'none' : input.content ===
  undefined ? 'read' : 'write'`; `notes.none` = `This turn allows only \`action: "schema"\`.`; `notes.read` =
  `This turn allows only \`action: "schema"\` and validation (no \`content\`); saving will be refused.`
- `doc-open-document-tool.ts:84-91`: `resolve: input.uri || input.path ? 'read' : 'write'`; `notes.read` =
  `This turn allows only opening an existing file (pass \`path\` or \`uri\`); creating a blank document will be refused.`
- `run-custom-tool.ts:366-373`: `resolve: lookups.customToolEffects?.get(input.tool) ?? 'write'`;
  `notes.read` = `This turn allows only the custom tools marked "read" in the list above; a tool with effects will be refused.`
- Under a `none` ceiling the three without a `none` note are DROPPED from the list (`usesAllowedUnder` → null);
  `edit_custom_tool` stays with its `none` note.

#### 1.1.5 `lib/pascal/tool-effect.ts` (NEW, 38 lines)

`customToolEffect(definition)` `:24-29` — `'write'` when ANY effect `isWriteEffect || isWardrobeEffect ||
isInventoryEffect || isLogEffect`, else `'read'` (no custom tool is ever `destroy` or `none`);
`customToolEffectMap(tools)` `:32-38`. Consumers: `buildTools` (`streaming.service.ts:360-364`, the
`customToolEffects` map), the Workbench library (`workbench.ts:212 effect: customToolEffect(...) === 'read' ?
'read' : 'write'`, entry field `effect: 'read' | 'write'` `:54`), the trigger runner (`trigger-runner.ts:171`).
v5 twins of the four predicates exist: `pascal/custom_tool_types.rs:1347-1364` (`is_write_effect`,
`is_wardrobe_effect`, `is_inventory_effect`, `is_log_effect`).

#### 1.1.6 The ceiling's plumbing at the pin (post-`a8214b253` homes)

- `lib/services/chat-message/streaming.service.ts`
  - `:71-79` `StreamOptions.logType?: LLMLogType` (pre-existing, `d1c06cd9d`) and NEW `toolChoice?: 'auto' |
    'none'` — doc: "offers the tools (so the request's cached prefix matches a real turn's) while asking the
    model not to call any — the storyteller bid. Providers that ignore it may still return a call; the caller
    discards it."; `:499` destructured; `:596` sent as `...(toolChoice && tools.length > 0 ? { toolChoice } :
    {})` → the plugin's `params.toolChoice` (see headline 3 for which plugins read it).
  - `:152-185` `applyEffectCeilingToTools(tools, ceiling, ctx)`: per tool, `maxEffectOfTool(name, ctx)` within
    ceiling → kept; else a computed spec's `usesAllowedUnder(ceiling)` note is appended to the description
    (`function.description` or top-level `description`, `\n\n` joined) and the tool kept; else dropped.
    DEBUG `Effect ceiling applied to tool list {ceiling, kept, dropped}` (`dropped` = names). A nameless
    tool is kept.
  - `:262-269` `BuildToolsOptions.effectCeiling?: ToolEffect`; `:272-280` `BuiltTools` gains
    `customToolEffects: Map<string, ToolEffect>` and `pluginToolEffects: Map<string, ToolEffect>`;
    `:466-474` `pluginToolEffects = await toolRegistry.getToolEffects(toolConfigs)` with WARN `Failed to read
    plugin tool effects; plugin tools count as write {userId, error}`; `:476-478` the ceiling applied LAST,
    after every other filter; every early return (`:330` etc.) returns empty maps.
- `lib/services/chat-message/turn-tools.service.ts` (NEW in `a8214b253`, carries the storyteller bits):
  `TurnToolSlate.effectCeiling: ToolEffect | undefined` `:64`, `customToolEffects` / `pluginToolEffects`
  `:65-66`; `TurnToolSlateParams.storytellerCeiling?: ToolEffect` `:85`; the Courier early return keeps
  `effectCeiling: params.storytellerCeiling` `:112`; `resolveTurnEffectCeiling(chat, chatSettings,
  storytellerCeiling)` `:221-238` — non-autonomous → `storytellerCeiling`; autonomous with policy ≠
  `always_refuse` AND `runDestructiveToolsAllowed === 1` (raw integer read) → `storytellerCeiling`; else
  DEBUG `Autonomous room: destructive tools withheld by effect ceiling {chatId, policy, allowedAtRoom}` and
  `lowerToolEffect(storytellerCeiling, 'write')`; passed to `buildTools` `:157`; DEBUG `[TurnTools] Tool slate
  resolved {chatId, characterId, toolCount, resolvedToolMode, effectCeiling, toolSettingsChanged}` `:188-195`;
  `buildTurnToolContext` `:318-360` copies `slate.customToolEffects` / `pluginToolEffects` / `effectCeiling`
  (only when set) onto the `ToolExecutionContext` `:356-358`.
- `lib/chat/tool-executor.ts:248-259` `ToolExecutionContext.effectCeiling? / customToolEffects? /
  pluginToolEffects?`; `:343-371` **the one check**, after the mountPool/operatorSurface refusal and before
  any dispatch: `effectOfToolCall(name, arguments, {customToolEffects, pluginToolEffects})`; above the ceiling
  → INFO `Tool call refused: above the turn's effect ceiling {context:'tool-executor', toolName, chatId,
  effect, ceiling}` and `{ toolName, success: false, result: { formattedText: message }, error: message }`
  (message = `describeCeilingRefusal`); within → DEBUG `Tool call within the turn's effect ceiling {context,
  toolName, effect, ceiling}`. No ceiling set → neither line.
- `lib/pascal/trigger-runner.ts:63-68` `effectCeiling?` param; `:170-183` a matched tool whose
  `customToolEffect(entry.definition)` is above it is skipped BEFORE `runCustomTool` with DEBUG `Triggered
  custom tool skipped — above the turn's effect ceiling {context:'pascal.trigger-runner', chatId, tool,
  phrase, effect, ceiling}`. Reached only from the reply site: `trigger-phrase-sites.ts:80-81, :99`
  (`runReplyTriggers.effectCeiling`), fed by the finalizer `message-finalizer.service.ts:490
  effectCeiling: storyteller?.effectCeiling`. The user-message site never carries one.
- `lib/plugins/tool-registry.ts:144-162` `getToolEffects(configs)`: for each plugin with a `getToolEffects`
  fn, `await plugin.getToolEffects(configs.get(pluginName) ?? {})`, keep entries whose value is in
  `TOOL_EFFECTS`; a throw → WARN `Plugin tool effects unavailable; its tools count as write {plugin, error}`.
  `lib/plugins/interfaces/tool-plugin.ts:286-296` and `packages/plugin-types/src/plugins/tool.ts:32-44,
  :273-290` add `ToolEffect` + `getToolEffects?`. MCP plugin 1.1.45 (per-server `effect`, default `write`),
  curl 1.0.32 (`curl` = `write`) — plugin bundles; **v5 has no plugin tools** (R-2).
- `lib/pascal/workbench.ts:47-54, :212` (above). `lib/schemas/chat.types.ts:1214` comment only
  (`runDestructiveToolsAllowed`: "1 = owner pre-authorized `destroy`-effect tools").

#### 1.1.7 Storyteller — the pure modules (`lib/storyteller/`, 9 files; all at the pin)

- **`types.ts` (61)**: `StorytellerTurnContext { bidRecord: StorytellerBidRecord; effectCeiling: ToolEffect;
  spokenSince: string[]; concurrentFollower?: boolean }`; `StorytellerBidRequest { roundId }`;
  `StorytellerBidCallOutcome = { ok:true; text; discardedToolCalls: string[]; offered } | { ok:false; reason }`;
  `StorytellerBidOffer { tools: Record<name,effect>; customTools: Record<name,effect>; destroyOffered; silent }`;
  `ParsedBid { speak; statedTier | null; tools: string[]; plan: string | null }`.
- **`tiers.ts` (65)**: `CUSTOM_TOOL_PREFIX = 'run_custom:'`; `effectOfNamedTool(name, offer)` `:36-42` —
  prefix stripped + trimmed → `offer.customTools[...]`; a bare custom name hits `customTools` first, then
  `tools`; `computeBidTier(parsed, offer)` `:44-65` — `toolsEffect` = max over named tools on the menu
  (unknown ones collected as `unknownTools`); `stated = parsed.statedTier ?? 'simple'`; `effect =
  max(TIER_EFFECT_CEILING[stated], toolsEffect)`; `raised = statedTier !== null && rank[toolsEffect] >
  rank[statedEffect]`; `tier = effect === 'destroy' && !offer.destroyOffered ? 'act' : EFFECT_TIER[effect]`.
- **`invite.ts` (62)**: `inviteProbability(t) = clamp(0.25 + 0.75 × t, 0, 1)`; `biddableSeats(participants,
  characters, impersonatingIds)` `:32-48` = `cycleCandidates(participants, characters)` (present, non-archived
  CHARACTER seats — v5 `cycle_order.rs:76 cycle_candidates`) minus `isUserDrivenSeat(p, impersonating ?? [])`
  (v5 `participant_filters.rs:55`), each with `talkativeness: participant.talkativeness ??
  character?.talkativeness ?? 0.5` and `silent: status === 'silent'`; `chooseInvitees(seats, {filter,
  random})` `:50-62` — filter off → all invited; on → per seat in order `random() < inviteProbability` (ONE
  draw per seat, `Math.random` default — inject the draw source in v5).
- **`bid-parse.ts` (87)**: `LABEL_LINE = /^\s*(?:[-*>]\s*)?[*_`~]*\s*(BID|TIER|TOOLS?|PLAN)\s*[*_`~]*\s*[:=\-–—]\s*(.*)$/i`;
  code fences stripped first (`/```[a-z]*\n?/gi`), `\r?\n` split; `TOOL` → `TOOLS`; continuation lines append
  to PLAN only; no `BID` → null; `speak = /^(speak|yes|bid|respond|reply|act)\b/`, `passes =
  /^(pass|no|none|decline|skip)\b/` on `stripWrapping(...).toLowerCase()` (`stripWrapping` strips
  `[\s*_`~"'[\]]` leading / `[\s*_`~"'[\].]` trailing); neither → null; tier: exact
  `STORYTELLER_TIERS` member else the FIRST `TIER_SYNONYMS` entry (insertion order: simple, reply, speak,
  lookup, 'look up', read, act, action, write, destroy, delete) whose key is a `startsWith` of the word, else
  null; tools: `/^(none|n\/a|-|—)?$/i` → `[]`, else split `/[,;]|\s+and\s+/`, each `stripWrapping` + trailing
  `()` removed, drop empty / `none`, de-duped by `Set`; plan: `^[\s*_`~"]+|[\s*_`~"]+$` stripped, `'' → null`.
  **JS-regex fidelity**: `\s` (Unicode), `\b`, case-insensitive `i` without `u` — use v5's JS-regex twin
  layer (Phase-1 `jsregex`), never `regex` defaults.
- **`cards.ts` (43)**: `cardFromOutcome(base, outcome, answeredAt = now ISO)` — `!ok` → `status:'error',
  reason`; unparseable → `status:'pass', reason:'No bid could be read from the reply'`; `!speak` →
  `status:'pass', plan: parsed.plan, reason: plan ? null : 'Passed'`; else `status:'bid', statedTier, tier,
  tierRaised, tools: computed.tools, plan: parsed.plan ?? '(no plan given)', reason: unknownTools.length ?
  `Named tools it does not have: ${unknownTools.join(', ')}` : null`.
- **`prompts.ts` (161)** — **model-visible, byte-exact**:
  - `NOTE_HEADER = '[Turn note from the Salon — not spoken by any character]'` (em dash).
  - `describeDisposition(t)` `:27-38`: `< 0.3` → `You are reserved: you speak when you have something real to
    add, and are content to let others carry a moment.`; `< 0.6` → `You are even-tempered: you speak when
    the moment invites you, and let it pass when it does not.`; `< 0.85` → `You are forthcoming: you are
    usually glad to take part.`; else `You are very talkative: you rarely let a moment pass without a word.`
  - `buildBidSection({characterName, talkativeness, offer})` `:48-96`, lines joined `\n`:
    `NOTE_HEADER` / `This is not your turn yet. The storyteller is asking each character whether they want to
    respond to what just happened, and will choose who goes ahead. Do not write your reply now — answer only
    with your bid.` / `` / disposition / [if `offer.silent`: `You are silent in this scene: if you go ahead,
    you may only act or emote, not speak aloud.`] / `` / `How much do you mean to do? Pick one tier:` /
    `- simple: just speak or act in character, using no tools.` / (read tools present, sorted code-unit:
    `- lookup: look things up first with read-only tools, then speak. Read-only tools: a, b.` else
    `- lookup: look things up first with read-only tools, then speak.`) / `- act: do something that changes
    the world (write a document, change clothes, move an object, send a letter, run a tool with effects),
    then speak.` / [if `destroyOffered`: `- destroy: delete something (<destroy names sorted>). Only when
    deletion is truly the point.`] / [if custom tools: `` / `Custom tools you can run (name them as
    run_custom:<name>), with what each does to the world:` / per entry sorted by `localeCompare` (ICU — the
    v5 ICU4X collator): `- <name>: read (lookup tier)` | `- <name>: write (act tier)`] / `` / `Reply with
    exactly these four lines and nothing else:` / `BID: speak or pass` / `TIER: simple, lookup, act or
    destroy` | `TIER: simple, lookup or act` / `TOOLS: the tools you plan to use, comma-separated, or none` /
    `PLAN: one or two sentences, in <characterName>'s own words, of what you mean to say or do` / `` /
    `Pass if you have nothing worth adding. Do not call any tool now.`
  - `buildPlanSection({plan, tier, toolsNamed, spokenSince, allowedTools, reroll?})` `:99-142`:
    reroll → `NOTE_HEADER` / `The storyteller chose you to go ahead. When you were asked, you planned this:`
    / `` / `"<plan>"` / `` / `The plan is private: do not announce it, quote it, or mention that you made one.
    You may adapt it to the moment.`; otherwise the same first four lines then (`spokenSince` non-empty:
    `Since you planned it, <A, B and C> <has|have> spoken. Read what they said before you answer.` else
    `Nobody has spoken since you planned it.`) / `The plan is private: do not announce it, quote it, or
    mention that you made one.` / `You may adapt it to the moment. If the moment for it has gone, reply with
    exactly this single line and nothing else:` / `` / `[NOTHING TO ADD]` (`NOTHING_TO_ADD_SENTINEL`, v5
    `skip_signal.rs:40`) / `` / `tierAllowance` / [if `toolsNamed`: `You mentioned: a, b.`].
    `joinNames`: ≤1 → as is; else `a, b and c`.
  - `tierAllowance(tier, allowedTools)` `:144-156` with `TIER_LABELS` (Simple/Lookup/Act/Destroy):
    simple → `Your bid was Simple: this turn allows no tools. Just speak or act.`; `list` = allowedTools
    non-empty ? ` Tools available: a, b.` : ``; lookup → `Your bid was Lookup: this turn allows read-only
    tools; anything that changes the world will be refused.<list>`; act → `Your bid was Act: this turn allows
    tools that look things up or change the world, but not deletion.<list>`; destroy → `Your bid was Destroy:
    this turn allows every tool, deletion included.<list>`.
- **`bid-call.ts` (80)**: `UNLISTED_TOOLS = {run_custom, submit_final_response}`; `buildBidOffer({tools,
  customToolEffects, pluginToolEffects, silent})` `:28-53` — walks the slate in order (`function.name ||
  name`; nameless skipped), `tools[name] = maxEffectOfTool(name, {pluginToolEffects})` (NO customToolEffects
  passed — `run_custom` is unlisted anyway), `customTools` filled ONLY when `run_custom` was on the slate,
  `destroyOffered = some value === 'destroy'`; `callForBid(options, offer)` `:56-80` — `streamMessage({...,
  logType:'STORYTELLER_BID', toolChoice:'none'})`, concatenates `chunk.content`, a throw → WARN `Storyteller
  bid call failed {chatId, characterId, error}` + `{ok:false, reason}`; success DEBUG `Storyteller bid call
  returned {chatId, characterId, length}` + `{ok:true, text, discardedToolCalls: [] (never filled), offered}`
  (logger `StorytellerBidCall`).
- **`round-context.ts` (25)**: `runWithStorytellerRoundId(roundId, fn)` / `getStorytellerRoundId()` over an
  `AsyncLocalStorage<string>`; the ONE reader is `llm-logging.service.ts:225 storytellerRoundId:
  getStorytellerRoundId()`.
- **`sse-tag.ts` (58)**: `tagSseChunk(chunk, participantId)` — every `data: {json}` line whose JSON is a
  non-array object is re-serialised as `{...parsed, participantId: parsed.participantId ?? participantId,
  concurrent: true}` (KEY ORDER: original keys, then `participantId` only if absent, then `concurrent`);
  `taggingController(inner, pid)` (close = no-op, error forwarded); `sinkController()` discards.
- **`round.service.ts` (441)** — §1.1.8.

#### 1.1.8 `lib/storyteller/round.service.ts` — the lifecycle (logger context `storyteller.round`)

- `:45` `STORYTELLER_BOARD_TIMEOUT_MS = 45_000`; `:48` `STORYTELLER_CONCURRENCY_PER_PROFILE = 4`;
  `:53-63` `withRoundLock(roundId, fn)` — a per-round promise chain (`processSingleton('storyteller.roundLocks')`).
- `pendingCard(seat, 'pending' | 'kept-quiet')` `:71-88` — `characterName: seat.character?.name ??
  'Unknown'`, `reason: kept-quiet ? 'Kept quiet this round (talkativeness)' : null`, `askedAt: pending ? now
  : null`, `answeredAt: null`, `tierRaised: false`, `tools: []`, `plan: null`, `statedTier/tier: null`.
- `recordCard` `:90-94` → `repos.storytellerRounds.recordBid` under the lock; publishes realtime
  `storytellerRounds` scoped by chatId when a row came back.
- `bidForSeat` `:97-124`: `runWithStorytellerRoundId(round.id, () => runStorytellerBid({chatId, userId,
  participantId, roundId}))` (dynamic import of the orchestrator); a throw → `{ok:false, reason}`;
  `cardFromOutcome`; INFO `Storyteller bid landed {context, chatId, roundId, participantId, status, tier}`;
  record.
- `abandonStorytellerRound(chatId, reason = 'abandoned')` `:127-137` — `findActiveForChat`; none → return;
  `markClosed(id, reason)`; publish; INFO `Storyteller round closed unrun {context, chatId, roundId, reason}`.
- `openStorytellerRound({chatId, userId, triggerMessageId, onRound?, onBid?, timeoutMs?})` `:143-209`:
  `abandonStorytellerRound(chatId, 'superseded')` FIRST; chat 404 → `throw Error('Chat not found')`;
  `loadRoomCharacters`; `biddableSeats`; `chooseInvitees(seats, {filter: chat.storytellerInviteFilter ===
  true})`; `create({chatId, triggerMessageId, status:'open', bids: [...invited pending, ...keptQuiet],
  selection:null, closedReason:null, closedAt:null})`; publish; `onRound(round)`; INFO `Storyteller round
  opened {context, chatId, roundId, invited, keptQuiet}`; every pending card → `bidForSeat(...).then(onBid)`
  in parallel; `Promise.race([allSettled → false, setTimeout(timeoutMs) → true])`; timed out → INFO
  `Storyteller board timeout — the board opens with bids still out {context, chatId, roundId, timeoutMs}`;
  returns `findById(round.id) ?? round` (late bids keep landing on the row through `recordBid`, which drops
  them once the round is no longer `open`).
- `askStorytellerAnyway({chatId, userId, roundId, participantId})` `:212-228`: round missing / other chat /
  not open → `StorytellerRoundError('That round is no longer open', 409)`; seat not on the round →
  `('That seat is not on this round', 404)`; card re-set to `pending` (`reason:null, askedAt: now,
  answeredAt: null`) and recorded; returns `bidForSeat`'s answered card (the route answers `{ bid }`).
- `letStorytellerRoundLie({chatId, roundId})` `:231-241`: same 409 gate; `selection = {participantIds: [],
  order: null, chosenAt: now, names: []}`; `markClosed(id, 'let-lie', selection)`; publish; INFO
  `Storyteller round let lie {context, chatId, roundId}`.
- `StorytellerRoundError(message, status)` `:243-248`.
- `planStorytellerRun({chatId, roundId, participantIds, order, keepOrder?})` `:266-334`: 409 gate; `chosen =
  [...new Set(participantIds)]`; empty → `('Choose at least one bid, or let the round lie', 400)`; each id
  must be a card with `status === 'bid'` and a `tier` else `('Only a seat that bid to speak can be chosen',
  400)`; a destroy bid with `bids.length > 1` → `('A Destroy bid must be chosen on its own', 400)`; chat
  missing → `('Chat not found', 404)`; `keepOrder` → `ordered = bids` + DEBUG `Storyteller run keeps the
  operator's order {chatId, roundId, participantIds: chosen}` (no `context` key; `946a50a04`); else
  `calculateTurnStateFromHistory({messages (type==='message'), participants, userParticipantId:
  findUserParticipant(...)?.id ?? null, spokenThisCycleParticipantIds, cycleOrderParticipantIds})` →
  `resolveCycleOrder(repos, chat, characters, turnState)` (v5 `cycle_order.rs:341`; it may DRAW and persist
  a cycle — a write) → `rank(id) = cycle.indexOf(id)` or `cycle.length + chosen.indexOf(id)` → sort;
  turns: `{bid, participantId, characterName, concurrent: order === 'simultaneous' && tier ∈ {simple,
  lookup}, profileKey: participant.connectionProfileId ?? 'default'}`; STABLE sort concurrent-first.
- `runStorytellerRound({chatId, round, turns, order, runTurn, onTurnStart?, onTurnEnd?})` `:350-429`:
  `selection = {participantIds, order, chosenAt: now, names}`; `markRunning`; publish; INFO `Storyteller
  round running {context, chatId, roundId, order, speakers: names}`; `contextFor(turn, spokenSince)` →
  `StorytellerTurnContext { concurrentFollower: turn.concurrent && turns.find(t => t.concurrent) !== turn,
  bidRecord: {roundId, tier, plan: bid.plan ?? '', toolsNamed: bid.tools}, effectCeiling:
  TIER_EFFECT_CEILING[tier], spokenSince }`; `runOne` increments a shared `depth` counter (`myDepth =
  ++depth`) and runs `runTurn` under `runWithStorytellerRoundId`; `spoke` appends the name; the concurrent
  group runs per `profileKey` with at most 4 in flight (`runWithCap`), all groups in parallel, everyone with
  `spokenSince: []`; then `isCurrentTurnStopped()` → `ended = 'stopped'`; the sequential tail in order with
  `spokenSince: [...spoken]`, a stop before a turn → `'stopped'`, an `outcome.yielded` → `'yielded'` + break;
  `markClosed(id, 'ran', selection)`; publish; INFO `Storyteller round closed {context, chatId, roundId,
  ended, spoke}`; returns `{ended, lastOutcome}`.

#### 1.1.9 Storage: schemas, repository, migrations, carriers

- **`lib/schemas/storyteller.types.ts` (NEW, 152)** — client-safe Zod: `ChatTurnModeSchema = z.enum(['rotation',
  'storyteller'])`; `StorytellerTierSchema` (`simple, lookup, act, destroy`); `TIER_EFFECT_CEILING` (simple→none,
  lookup→read, act→write, destroy→destroy); `EFFECT_TIER` (the inverse); `TIER_LABELS` (Simple/Lookup/Act/
  Destroy); `StorytellerBidStatusSchema` (`pending, bid, pass, error, kept-quiet`); **`StorytellerBidSchema`
  key order** `participantId, characterId, characterName, status, statedTier?.nullable, tier?.nullable,
  tierRaised (default false), tools (default []), plan?.nullable, reason?.nullable, talkativeness: number,
  silent (default false), askedAt?.nullable, answeredAt?.nullable`; `StorytellerOrderSchema` (`sequential,
  simultaneous`); `StorytellerSelectionSchema { participantIds, order: Order.nullable(), chosenAt, names
  (default []) }`; `StorytellerRoundStatusSchema` (`open, running, closed`); **`StorytellerRoundSchema`** `{ id,
  chatId, triggerMessageId: string.nullable(), status, bids (default []), selection?.nullable, closedReason?:
  enum(['ran','let-lie','superseded','abandoned']).nullable, createdAt, updatedAt, closedAt?.nullable }`;
  `StorytellerBidRecordSchema { roundId, tier, plan, toolsNamed (default []) }`; `StorytellerRunRequestSchema
  { roundId, participantIds: string[], order?: Order, keepOrder?: boolean }` (`946a50a04` added `keepOrder`);
  `StorytellerAskRequestSchema { roundId, participantId }`; the route's local `StorytellerLieRequestSchema
  { roundId }` (`actions/storyteller.ts:28`). Zod 4: a `z.string()` id, NOT `z.uuid()` — a non-UUID
  `roundId` reaches the service and answers 409, not 400.
- **`lib/schemas/chat.types.ts`**: `MessageEventSchema.storytellerBid: StorytellerBidRecordSchema.nullable().
  optional()` (`:463-470`, after `carinaMeta`, before `pascalMeta`); `ChatMetadataSchema.turnMode:
  ChatTurnModeSchema.nullable().optional()` + `storytellerInviteFilter: z.boolean().default(false)` (`:892-903`,
  after `isPaused`, before `isManuallyRenamed`); the same two on `ChatMetadataBaseSchema` (`:1309-1312`).
  `lib/database/repositories/chats-messages.ops.ts:113-116` `ChatMessageRowSchema.storytellerBid` (same
  position) — the row schema generateDDL renders `chat_messages` from.
- **`lib/schemas/llm-log.types.ts`**: `LLMLogTypeEnum` gains `'STORYTELLER_BID'` LAST (after
  `SCENARIO_BUILDER`); `LLMLogSchema.storytellerRoundId: UUIDSchema.nullable().optional()` after
  `autonomousRunId`, before `provider`.
- **`lib/database/repositories/storyteller-rounds.repository.ts` (NEW, 120)** — `AbstractBaseRepository<
  StorytellerRound>('storyteller_rounds', StorytellerRoundSchema)`: `create` (`_create` — mints id,
  createdAt, updatedAt), `update`, `delete`; `findActiveForChat(chatId)` `:49-62` — `findByFilter({chatId})`,
  `status !== 'closed'`, sorted `createdAt` desc (`localeCompare`), first or null (a `safeQuery`: ERROR
  `Error finding active storyteller round {chatId}` → null); `findLatestForChat` `:65-76` — same without the
  status filter (`Error finding latest storyteller round`); `recordBid(roundId, bid)` `:83-98` — round
  missing or not `open` → DEBUG `Storyteller bid dropped — round not open {context:'storyteller-rounds.
  repository', roundId, participantId, status: round?.status ?? 'missing'}` + null; else replace the card
  with the same `participantId` (or append) and `_update(roundId, {bids})`; `markRunning(roundId, selection)`
  → `{status:'running', selection}`; `markClosed(roundId, reason, selection?)` → `{status:'closed',
  closedReason, closedAt: now, bids: [], ...(selection !== undefined ? {selection} : {})}`. Base-repository
  semantics to carry: `_update` stamps `updatedAt`, JSON columns (`bids`, `selection`) serialised by the
  SQLite backend, `safeParse` on read (an unparseable row DROPS silently — v5's P4.D265 `hydrate_raw_rows`
  class).
- `lib/database/repositories/index.ts` + `lib/repositories/factory.ts` register `storytellerRounds` in the
  container; `factory.ts:95-99` `getRepositories()` now returns `dryRepositories(container)` inside a dry
  scope (`isInDryScope()`).
- **`lib/database/repositories/chats.repository.ts`**: `TURN_MODE_FIELD = 'turnMode'` added to
  `patchOnlyFields()` (`:573-574`; the base refuses it in a whole-row update — the test hunk renames the
  patch-only describe to include `turnMode`); `setTurnMode(chatId, mode)` `:582-593` — `stored = mode ===
  'rotation' ? null : mode`; raw `updateOne({id}, {$set: {turnMode: stored, updatedAt: now}})`; `safeQuery`
  ('Failed to write turn mode', `{chatId, mode, context:'storyteller'}`, false); DEBUG `Turn mode write
  {chatId, mode, written, context:'storyteller'}`; returns `matchedCount > 0`. **It stamps `updatedAt`**
  (unlike `setConciergeMode`).
- **Migrations** (`migrations/scripts/`, both `introducedInVersion '4.11.0'`): `add-storyteller-mode-v1.ts`
  (`dependsOn ['sqlite-initial-schema-v1']`; `shouldRun` when `chats` AND `chat_messages` exist and any of
  the four is missing; `addColumnIfMissing('chats','turnMode','TEXT DEFAULT NULL')` INFO `Added
  chats.turnMode column`, `('chats','storytellerInviteFilter','INTEGER DEFAULT 0')` INFO `Added
  chats.storytellerInviteFilter column`, `('chat_messages','storytellerBid','TEXT DEFAULT NULL')` INFO
  `Added chat_messages.storytellerBid column`, then the table + index DDL quoted in §3 with INFO `Created
  storyteller_rounds table` (all `context: 'migration.add-storyteller-mode-v1'`); result message
  `Storyteller mode storage in place (${n} changes)`; ERROR `Storyteller-mode migration failed {context,
  error}`) and `add-llm-logs-storyteller-round-id-column.ts` (`dependsOn
  ['add-llm-logs-autonomous-run-id-column-v1']`; `shouldRun` false when the logs file is ABSENT or has no
  `llm_logs`; `ALTER TABLE "llm_logs" ADD COLUMN "storytellerRoundId" TEXT DEFAULT NULL` INFO `Added
  storytellerRoundId column to llm_logs {context:'migration.add-llm-logs-storyteller-round-id-column'}`;
  `CREATE INDEX IF NOT EXISTS "idx_llm_logs_storytellerRoundId" ON "llm_logs" ("storytellerRoundId")`
  ALWAYS executed; message `Added storytellerRoundId column + index to llm_logs`; ERROR `Failed to add
  storytellerRoundId column to llm_logs`). Registered LAST in `migrations/index.ts` after
  `addMemoryKeystoneMigration`. `lib/startup/prettify.ts:166-167` labels: `add-storyteller-mode-v1` →
  `Clearing a dais in the Salon for the Storyteller, and a slate on which the company may bid`;
  `add-llm-logs-storyteller-round-id-column-v1` → `Teaching the wire ledger to bundle each storyteller round
  under one ribbon…`.
- **Carriers**: `lib/backup/restore/delete-service.ts:60-65` appends `'storyteller_rounds'` to the
  replace-restore main truncate list (after `chat_wardrobe_history`); NO backup collector, NO `.qtap`
  record for rounds. `public/schemas/qtap-export.schema.json` chat properties gain `turnMode: {enum:
  ["rotation","storyteller",null], description: "The chat's turn mode. NULL reads as 'rotation' (the weighted
  rotation). 'storyteller' = each operator post opens a round: every LLM character is asked for a bid, and the
  operator chooses who speaks."}` and `storytellerInviteFilter: {type: ["boolean","null"], description:
  "Storyteller mode: invite each character to bid with probability 0.25 + 0.75 × talkativeness instead of
  inviting everyone. Default false."}` after `conciergeModeReason`; message properties gain `storytellerBid`
  (`type ["object","null"]`, `required ["roundId","tier","plan"]`, properties `roundId` (string), `tier`
  (enum of the four), `plan` (string), `toolsNamed` (array of string)) after `carinaMeta`, before
  `pascalMeta`. The export/import/backup code paths are untouched by the commit — `storytellerBid` rides as
  an ordinary message column through the existing whole-row carriers (v5: the `.qtap` chat-record column
  list, P4.167's carrier columns, and `chats_messages.rs:979`'s dynamic INSERT — verify each copies an
  unknown column or needs the key named).
- **`storytellerBid` is never stripped** — `grep storytellerBid lib` (pin) hits only the schemas, the
  finalizer write, the swipe copy and the SSE encoder; the model-message formatters read `content`. The
  `ChatMetadataSchema` comment "stripped from every transcript a model reads" describes nothing.
- `lib/repositories/dry-scope.ts` (NEW, 131) + `method-classification.ts` (NEW, 181 — the read/write prefix
  rulebook and `METHOD_OVERRIDES` moved VERBATIM out of `background-jobs/child/child-repositories-proxy.ts`,
  −157 lines there): `runDry(label, fn)` wraps an `AsyncLocalStorage<{label, discarded: Map}>`; inside it
  `getRepositories()` returns a Proxy whose every `write`-classified or `unknown`-classified method returns
  `Promise.resolve(syntheticWriteResult(key, args) ?? dryAddResult(key, args))` (an `add*` writer hands back
  `{...row}`), reads are bound to the real instance; `llmLogs` passes through (`DRY_SCOPE_PASS_THROUGH_REPOS`);
  `unknown` → WARN `Dry scope discarded an unclassified repository method {context:'dry-scope', label,
  method}` once per scope; on exit DEBUG `Dry scope discarded writes {context, label, discarded: {fqn:
  count}}` when any. **NO v5 counterpart** (v5 has no repository proxy; the job runner is in-process) — R-1.

#### 1.1.10 The turn path at the pin (the orchestrator family — all P4.D348's files, storyteller regions marked)

- `lib/services/chat-message/types.ts`: `SendMessageOptions.storytellerBid?: StorytellerBidRequest` `:121`
  and `storyteller?: StorytellerTurnContext` `:127`; `ProcessMessageResult.storytellerHold?: {
  triggerMessageId: string | null; carina: boolean }` `:288` and `bid?: StorytellerBidCallOutcome` `:290`.
- `turn-holds.service.ts:59-78` `resolveTurnHold(chat, options)`: `shouldHoldUserTurnForPause` → `'paused'`;
  else `!continueMode && chat.turnMode === 'storyteller' && !isOperatorWhisperSend(options) &&
  options.neverPauseForUser !== true` → `'storyteller'`; else null. `qualifiesForFairRotationCheck` `:86-98`
  requires `hold === null` (a storyteller post skips the fair-rotation guard). `finishHeldPost` `:233-279`:
  `repos.chats.update(chatId, {lastTurnParticipantId: null})` for BOTH holds; storyteller → returns
  `{isMultiCharacter, hasContent:false, messageId: userMessageId, userParticipantId, isPaused:false,
  storytellerHold: {triggerMessageId: userMessageId, carina: !!(content && parseCarinaQuery(content))}}` and
  emits NOTHING (the paused hold emits the `paused` chainComplete with `heldUserTurn`).
- `orchestrator.service.ts`: `:163-225 handleSendMessage` — after `processMessage`, `result.storytellerHold`
  → `openRoundOnSend` and RETURN (no chain, no housekeeping); `storytellerSingleTurn = continueMode &&
  neverPauseForUser !== true && (await findById(chatId))?.turnMode === 'storyteller'` (a SECOND chat read)
  feeds `executeTurnChain({singleTurn: options.singleTurn === true || isWhisper || storytellerSingleTurn})`.
  `:232-263 openRoundOnSend` — `hold.carina` → DEBUG `[Storyteller] Carina query — no round opens {chatId}`
  + `chainComplete {reason:'user_turn', nextSpeakerId:null, chainDepth:0}` and return; else
  `openStorytellerRound({onRound: enqueue storytellerRound frame; onBid: enqueue storytellerBid frame
  (only once `roundId` is known)})`, then `roundOpen {roundId}` and `chainComplete {reason:'round_open',
  nextSpeakerId:null, chainDepth:0}`. `:271-296 runStorytellerBid` — `runDry(\`storyteller-bid:${pid}\`, …)`
  around `processMessage(getRepositories(), chatId, userId, {continueMode:true, respondingParticipantId,
  storytellerBid:{roundId}}, sinkController(), new TextEncoder(), newSubmissionLedger())`; `result.bid ??
  {ok:false, reason:'The bid could not be built'}`; a throw → WARN `[Storyteller] Bid build failed {chatId,
  participantId, roundId, error}` + `{ok:false, reason}`. `:306-378 handleStorytellerRun(repos, chatId,
  userId, {roundId, participantIds, order, keepOrder?})` — `planStorytellerRun` BEFORE the stream exists
  (a 4xx is an ordinary response), then `openTurnStream(chatId, …)` (the registered stream: Stop / raised
  hand reach every turn) running `runStorytellerRound` with `runTurn` = `processMessage(repos, chatId,
  userId, {continueMode:true, respondingParticipantId, storyteller}, turn.concurrent ?
  taggingController(controller, pid) : controller, encoder, ledger)` → `{spoke: hasContent && !skipped,
  yielded, messageId, sceneTrackingContext}`; `onTurnStart` (sequential only) `turnStart {participantId,
  characterName, chainDepth: depth}`; `onTurnEnd` (sequential only) `turnComplete {participantId,
  messageId: result.messageId ?? '', chainDepth: depth, skipped: !spoke || undefined}`; once after the
  round, if `lastOutcome.sceneTrackingContext`: `runStorytellerRoundHousekeeping(repos, {chatId, userId,
  connectionProfile, memoryChatSettings})` then `triggerAfterTurnsHousekeeping`; finally ONE `chainComplete`
  — stopped → `{reason:'stopped', nextSpeakerId:null, chainDepth: turns.length, paused:true}`; yielded →
  `{reason:'yielded', …}`; else `{reason:'user_turn', …}`. Inside `processMessage`: `:394-396 isBid /
  storytellerTurn`; `:556-564 resolveTurnSkipOffer({…, isStorytellerTurn: !!storytellerTurn, isBid, …})`
  (`turn-preparation.service.ts:127-132`: a storyteller turn → `{offerSkip:true, recentlyAddressed:false,
  characterName}`; a bid → undefined); `:570-573` the raised hand is never taken by a held post or a bid;
  `:582-594 reinjectProsperoContext` skipped for a hold or a bid, `concurrentFollower` passed through;
  `:630-642` the hold return BEFORE the tool build; `:661 storytellerCeiling: storytellerTurn?.effectCeiling`
  into `resolveTurnToolSlate`; `:715-722 buildStorytellerSection` (`:1069-1113`: a bid → `buildBidOffer` from
  `slate.tools` / `slate.customToolEffects` / `slate.pluginToolEffects` / `silent: participant.status ===
  'silent'` + `buildBidSection({characterName, talkativeness: participant.talkativeness ??
  character.talkativeness ?? 0.5, offer})`; a speaking turn → DEBUG `[Storyteller] Speaking turn carries its
  bid {chatId, participantId, roundId, tier, ceiling: slate.effectCeiling}` + `buildPlanSection({plan, tier,
  toolsNamed, spokenSince, allowedTools: extractToolNames(slate.tools)})`); `:752-758` into
  `buildMessageContext`: `turnSkip: storytellerTurn ? undefined : turnSkip` (the plan section carries its
  own pass instruction — but `turnSkip` STILL feeds `resolveTurnOutcome`, so the sentinel is honoured),
  `purpose: isBid ? 'bid' : 'turn'`, `storytellerSection`; `:781-792` a Courier responder refuses a bid
  with `{ok:false, reason:'This character answers through the Courier, which cannot bid.'}`; `:836-857`
  the bid short-circuit — `callForBid({messages: formattedMessages, connectionProfile: effectiveProfile,
  apiKey, modelParams, tools: slate.actualTools, useNativeWebSearch:false, userId, chatId, characterId},
  bidOffer)` → returns `{isMultiCharacter, hasContent:false, messageId:null, userParticipantId, isPaused:
  chat.isPaused, bid}` (nothing streamed, saved or finalised); `:1052 storyteller: storytellerTurn` into the
  finalizer.
- `context-builder.service.ts:120-123, :1140-1141` pass `purpose` / `storytellerSection` through;
  `lib/chat/context-manager.ts:413-426` the option docs, `:763 storyteller: options.storytellerSection` in
  the sections bag, `:858` recall delivery skipped for `purpose === 'bid'` (assembled, not stamped);
  `lib/chat/context/turn-trailing.ts:42-70` `TrailingSections.storyteller?` LAST of eleven (coreWhisper,
  recall, mail, progressions, inventory, inform, referentLedger, closingEcho, turnSkip, turnYield,
  storyteller) — v5 `build_context.rs:3118-3127` has the first eight (no referentLedger / closingEcho —
  P4.D347's) and no storyteller slot.
- `message-finalizer.service.ts`: `FinalizeMessageResponseOptions.storyteller?` `:67-72`; `:305-317` after
  the assistant message is saved, `repos.chats.updateMessage(chatId, assistantMessageId, {storytellerBid:
  storyteller.bidRecord})` with WARN `Failed to record the storyteller bid on its message {chatId, messageId,
  error}` on failure; `:481-491 runReplyTriggers({…, effectCeiling: storyteller?.effectCeiling})`; `:567-572`
  a storyteller turn SKIPS the per-turn extraction / summary / danger triggers with DEBUG `Storyteller turn:
  housekeeping deferred to the end of the round {chatId, roundId}`; `:649-679 runStorytellerRoundHousekeeping`
  — DEBUG `Storyteller round housekeeping {chatId}`, then `triggerTurnMemoryExtraction`,
  `triggerContextSummaryCheck`, and the danger classification, once.
- `regenerate-swipe.service.ts:206-216` a swipe of a line with `storytellerBid` passes `storytellerSection:
  buildPlanSection({plan, tier, toolsNamed, spokenSince: [], allowedTools: [], reroll: true})`; `:346-347` the
  new swipe row copies `storytellerBid`.
- `llm-logging.service.ts:225` the row stamp; `lib/services/chat-message/index.ts` exports
  `handleStorytellerRun`.

#### 1.1.11 Routes (`app/api/v1/chats/[id]/`)

- `actions/storyteller.ts` (NEW, 94): GET `?action=storyteller-round` → `successResponse({ round:
  findActiveForChat(chatId) ?? findLatestForChat(chatId) })` (null when none; a closed round's `bids` is
  `[]`). POST `?action=storyteller-run` → `StorytellerRunRequestSchema.parse(body)`; INFO `[Storyteller]
  Running a selection {chatId, roundId, count: participantIds.length, order: body.order ?? 'sequential',
  keepOrder: body.keepOrder === true}`; `handleStorytellerRun(repos, chatId, user.id, {roundId,
  participantIds, order: body.order ?? 'sequential', keepOrder: body.keepOrder === true})` →
  `sseStreamResponse(stream)`. POST `?action=storyteller-ask` → `askStorytellerAnyway` →
  `successResponse({ bid })`. POST `?action=storyteller-lie` → `letStorytellerRoundLie` →
  `successResponse({ closed: true })`. `roundError` `:30-37`: a `StorytellerRoundError` → INFO
  `[Storyteller] Request refused {chatId, action, reason: error.message}` + `errorResponse(message, status)`
  (409 / 404 / 400 as above); a `ZodError` → `badRequest('Invalid request')`; anything else rethrown (the
  middleware's 500). All three POSTs run AFTER `handlePost`'s chat 404 (`notFound('Chat')`).
- `handlers/get.ts:81` the GET thunk `'storyteller-round'` sits between `'informs'` and `'group-stores'`
  (v5 `CHAT_GET_ACTIONS` → 18 keys); `:377-379` the chat GET wire gains `turnMode: chatMetadata.turnMode ??
  'rotation'` and `storytellerInviteFilter: chatMetadata.storytellerInviteFilter ?? false` right after
  `isPaused` (v5 `salon.rs:424`, `assemble_chat_get`).
- `handlers/post.ts:141-143` the POST thunks `'storyteller-run'`, `'storyteller-ask'`, `'storyteller-lie'`
  appended LAST, after `'save-image'` (v5 `CHAT_POST_ACTIONS` `wardrobe_routes.rs:762-821` → three new
  trailing keys, and the ordering test at `:1301` style).
- `helpers.ts:589-601 processChatUpdates`: `if (validatedData.turnMode)` → `before = updatedChat.turnMode ??
  'rotation'`; `setTurnMode`; `before === 'storyteller' && new !== 'storyteller'` → `abandonStorytellerRound
  (chatId, 'abandoned')`; refetch; INFO `[Chats v1] Turn mode set {chatId, from, to}`. Runs AFTER the `chat`
  bag write and BEFORE the Concierge flip. `schemas.ts`: `updateChatSchema.storytellerInviteFilter:
  z.boolean().optional()` (in the `chat` bag) and `chatUpdateRequestSchema.turnMode:
  ChatTurnModeSchema.optional()` (a top-level sibling; `null` or a bad string → the whole PUT is the
  middleware's 400 `Validation error`).
- `lib/query/keys.ts:73` `queryKeys.chats.storytellerRound(id) = ['chats', id, 'storytellerRound']`;
  `lib/schemas/realtime.types.ts:51-55` the NINTH topic `'storytellerRounds'` (after `inventory`);
  `lib/realtime/job-topics.ts:142` `storytellerRounds: []` (never published from the job child);
  `lib/realtime/topic-map.ts:93-95` `case 'storytellerRounds'` → `id ? [storytellerRound(id)] : [chats.all]`.

#### 1.1.12 The SPA half of `b15fe9404` (v4 React — the P4.D344 inputs; the component map is §6)

- **`components/chat/StorytellerBoard.tsx`** (301 at `b15fe9404`, 431 at the pin after `946a50a04`) —
  `useQuery(queryKeys.chats.storytellerRound(chatId))` fetching `GET ?action=storyteller-round` with
  `useRealtimeRefetchInterval(60_000)`; a `round === null` renders nothing; a non-open round renders the
  summary line `<div class="qt-storyteller-summary" role="status"><Icon users/> <span>text</span>` with text
  `running` → `Speaking now: A, B`; `let-lie` → `The last round was let lie.`; `superseded` / `abandoned` →
  `The last round closed unrun.`; else names ? `Last round: A, B` + (` (all at once)` when
  `selection.order === 'simultaneous'`) : `The last round has run.`; an open round renders `<section
  class="qt-storyteller-board" aria-label="The storyteller's board">` with header `The company bids` +
  (` — N still considering…` when pending > 0), tools `Reset order` (title `Put the cards back and let the
  room's rotation set the order`, only once arranged) and `Select all` (never includes a Destroy bid);
  per card (`cardClass`: `qt-storyteller-card` + `-selected` / `-draggable` / `-dragging` / `-act` /
  `-destroy` / `-quiet` for non-bid non-pending): an order badge `<span class="qt-storyteller-card-order"
  title="Speaks first|second|…|#N">` once arranged and selected; a checkbox `aria-label="Let <name> go ahead"`
  for `status==='bid'`; avatar + name; the tier badge `qt-badge qt-badge-secondary|info|warning|destructive`
  with `TIER_LABELS`; `plan` in `qt-storyteller-plan`; the status line (`Considering…` / `Passes: <plan>` or
  `Passes — <reason>` / `No bid — <reason ?? 'the call failed'>` / `Kept quiet this round`); the Destroy
  warning `This bid means to delete something, and cannot be undone. It goes ahead alone.`; the raised line
  `Asked for <Stated>, named a|an <Tier> tool.` (`an` only for act); `Tools: a, b`; `reason`; `Talkativeness
  0.50 · silent`; `Ask anyway` (secondary) for `kept-quiet` / `error`; below the cards (when > 1 biddable):
  `Chosen bids run in the order shown.` | `Drag the cards to set the running order; left alone, the room’s
  rotation decides.` (curly apostrophe); `A Destroy bid must be chosen on its own.` when mixed; buttons
  `Run in order` (primary; disabled when none chosen or mixed), `All at once` (title `Simple and Lookup bids
  speak at once; Act and Destroy bids follow one at a time`; disabled below 2 chosen), `Let it lie`. Toggle
  rule: ticking a destroy bid clears every other; ticking any other drops the destroy. Selection and
  arrangement reset on `roundId` change. Mutations: `storyteller-ask` (`onError` toast `Could not ask for that
  bid`, `onSettled` invalidate), `storyteller-lie` (`Could not close the round`). `onRun(roundId,
  chosenBids.map(id) in BOARD order, order, arranged)`. `withBid(round, bid)` patches one card; `arrangeBids
  (bids, arrangement)` (`:108-117`) — the arranged ids first (missing ones skipped), then unnamed bids in round
  order. Drag: `@dnd-kit/core` + `sortable` + `utilities` (MouseSensor distance 8, TouchSensor delay 250 /
  tolerance 5, KeyboardSensor `sortableKeyboardCoordinates`, `rectSortingStrategy`, `closestCenter`); only
  a `status==='bid'` card is draggable (`aria-roledescription="draggable bid"`); the card is its own activator.
- **`ChatSidebar.tsx`**: props `turnMode?`, `onSetTurnPosition?`, `storytellerInviteFilter?`,
  `onToggleStorytellerInviteFilter?`; `TurnModeControl` `:770-808` — `<div class="qt-storyteller-mode mt-3"
  role="radiogroup" aria-label="Turn mode">` with three `role="radio"` buttons `Normal` (title `The company
  takes turns on its own`), `Paused` (`Nobody answers until you nudge them`), `Storyteller` (`Each of your
  posts asks the company who wishes to answer; you choose`); the current one is `paused` when `isPaused`,
  else `storyteller` when `turnMode === 'storyteller'`, else `normal`; classes `qt-storyteller-mode-option` +
  `-paused` (active paused) | `-active`; clicking the active one does nothing. The invite checkbox
  (`qt-checkbox`, label `Invite by talkativeness`, title `Each round, a quiet character may be left out — the
  likelier the less talkative. Kept-quiet seats can still be asked from the board.`) renders only in
  storyteller mode. The old Pause button renders only when `onSetTurnPosition` is absent (`:875`).
- **`useChatControls.ts:226-262`**: `setTurnPosition(position)` — `paused` → `setPauseState(true)` + toast
  `Auto-responses paused`; else optimistic `isPaused:false, turnMode`, `PUT /api/v1/chats/${id}` body
  `{ chat: { isPaused: false }, turnMode }`, toast `Storyteller mode: each of your posts asks the company who
  wishes to answer` | `Auto-responses resumed`; on error toast `Could not change the turn mode` (+
  `console.error('[useChatControls] turn mode change failed', error)`); always `fetchChat()`.
  `toggleStorytellerInviteFilter` — optimistic flip, `persistChatField(chatId, {storytellerInviteFilter:
  next}, 'storyteller invite filter', rollback + toast 'Could not change the invitation setting')`.
- **`useSSEStreaming.ts`**: `StorytellerStreamEvent` (`round` / `bid` / `open`), `ConcurrentStream
  {participantId, content}`; the SSE frame type gains `concurrent?`, `storytellerRound?`, `storytellerBid?`,
  `roundOpen?`, `roundId?`; the reader (`:705-760`) hands the three board frames to `onStorytellerEvent` and
  returns; a `concurrent && participantId` frame keeps one bubble per participant — `turnStart` adds it
  (and clears `waitingForResponse`), `content` appends, `done` removes it and, unless `skipped` / `stopped`,
  pushes an ASSISTANT message `{id: messageId, content: finished, participantId, provider, modelName,
  routeTrail, isSilentMessage, reasoningContent, reasoningSegments}`; `carinaAnswer` / `hostAnnouncement` /
  `pascalResult` surfaced; `error` toasted; `runStorytellerSelection(roundId, participantIds, order,
  keepOrder = false)` (`:1541-1662`) — refuses while streaming (`One moment — the room is still speaking.`),
  aborts any open controller, POSTs `?action=storyteller-run` with `{roundId, participantIds, order,
  keepOrder}`, reads the stream in the chain dialect (`onTurnStart` sets the responding participant and
  resets the bubble; `onDone` / `onIntermediateDone` push the reply unless skipped/stopped or blank;
  `onTurnComplete` refetches; `onChainComplete` clears, scrolls, refetches, `announceChainPause(event)`,
  focuses); non-ok → `Error(errorData.error || 'The storyteller could not run that selection')`; the
  `finally` resets everything incl. `setHandRaised(false)` / `stoppingRef`. Exposes `runStorytellerSelection`,
  `concurrentStreams`.
- **`SalonView.tsx`**: `handleStorytellerEvent` writes the board's TanStack cache directly (`round` →
  `setQueryData({round})`; `bid` → `withBid` when the cached round id matches; `open` → invalidate);
  `<StorytellerBoard chatId busy={streaming || waitingForResponse || sending} avatarFor onRun=
  {runStorytellerSelection}/>` rendered ABOVE the composer only when `chat.turnMode === 'storyteller'`;
  `concurrentStreams` (with the character looked up per participant) into `VirtualizedMessageList`; the
  four sidebar props.
- **`VirtualizedMessageList.tsx:132-136, :421-437`**: `concurrentStreams` → one `<StreamingMessage>` per
  entry after the ordinary streaming slot (`streaming = content.length > 0`, `waitingForResponse =
  content.length === 0`, no tool batches / reasoning).
- **`LLMInspectorPanel.tsx`**: a `storyteller` filter category between `chat` and `memory` — group
  `['STORYTELLER_BID']`, label `Storyteller Rounds`; the filter ALSO admits any log with a
  `storytellerRoundId` (`:64-67`); at the pin `other` is derived (`LLMLogTypeEnum.options.filter(!CLAIMED)`,
  `eb2d87788` — P4.D348/P4.D359's). **`LLMInspectorEntry.tsx`**: badge class `STORYTELLER_BID:
  'qt-bg-primary/15 qt-text-primary'`, label `Bid`, and a round chip `<span class="text-[10px] font-mono
  px-1.5 py-0.5 rounded qt-bg-muted qt-text-secondary flex-shrink-0" title="Storyteller round <id>">round
  <id.slice(0,6)></span>` after the type badge when `log.storytellerRoundId`. **`llm-logs-card.tsx:46`**
  label `'STORYTELLER_BID': 'Storyteller Bid'`. **`WorkbenchLibrary.tsx:249-258`** the effect badge
  (`qt-badge qt-badge-warning` for `write` with title `Has side effects (state, wardrobe, inventory or the
  log): a write. Storyteller turns need an Act bid to run it.`; `qt-badge qt-badge-info` for `read`, title `No
  side effects — rolling and announcing only: a read. A Lookup bid may run it.`; text = the effect word)
  between the trigger-count badge and the parameter-count badge. `app/salon/[id]/types.ts:237-240`
  `Chat.turnMode?` / `storytellerInviteFilter?`.
- **`app/styles/qt-components/_storyteller.css`** (NEW, 203 lines; mirrored to theme-storybook
  `qt-components.css`): classes `.qt-storyteller-actions .qt-storyteller-board .qt-storyteller-board-header
  .qt-storyteller-board-tools .qt-storyteller-card .qt-storyteller-card-act .qt-storyteller-card-destroy
  .qt-storyteller-card-draggable .qt-storyteller-card-dragging .qt-storyteller-card-head
  .qt-storyteller-card-name .qt-storyteller-card-order .qt-storyteller-card-quiet .qt-storyteller-card-selected
  .qt-storyteller-cards .qt-storyteller-meta .qt-storyteller-mode .qt-storyteller-mode-option
  .qt-storyteller-mode-option-active .qt-storyteller-mode-option-paused .qt-storyteller-plan
  .qt-storyteller-summary .qt-storyteller-warning` (+ `@@ -4,6 +4,33` `_index.css` import). The byte copy is
  the SPA lane's (v5 keeps `qt-*` CSS under `apps/web/src/styles/qt-components/` — the P4.D357 WCAG lane
  touches the `qt-*` surfaces too; coordinate the file name).

#### 1.1.13 Docs / help / vendor (P4.D360 owns the bytes)

`help/storyteller-mode.md` NEW (120 lines at the pin; headings: Choosing the Mode / A Round, Step by Step /
Bids and Their Tiers / What the Chosen Character Is Told / What Is Kept, and What Is Not / Things That Skip
the Board / Silent Characters / In-Chat Navigation / Related Pages; `946a50a04` rewrote its `:40-52` for
dragging); `help/chat-multi-character.md` (+9 at `:322`, +1 nav at `:518`), `help/chat-participants.md`
(`:182` +2, `:233-258` the turn-mode control), `help/custom-tools.md` (`:466` +2), `help/pascals-workbench.md`
(`:12` +2), `help/tools-settings.md` (`:138` +10), `help/autonomous-rooms.md` (`:92` one line);
`docs/developer/DDL.md` (§3 quotes it), `API.md` (`:2650-2669` the four endpoints; `946a50a04` rewrote the
run paragraph), `FEATURES.md`, `BACKGROUND_JOBS_CHILD.md` (the classification move), the plugin-types
README / CHANGELOG, `docs/CHANGELOG.md` (+27, +6, +5, +4), the spec moved to
`docs/developer/features/complete/storyteller-mode.md` (+88), `.claude/commands/update-documentation.md`
(NO-PORT), README badge. The `help_navigate` tool's page list — check v5's embedded `help/` index gains
`storyteller-mode` (the vendor lane re-vendors `help/` whole).

### 1.2 `946a50a04` — drag storyteller bids into running order (18 files +385 / −102)

Server (two hunks, both at the pin as quoted above): `storyteller.types.ts:144-145 keepOrder?: z.boolean().
optional()`; `round.service.ts:268-322` the `keepOrder` branch + DEBUG `Storyteller run keeps the operator's
order {chatId, roundId, participantIds}`; `actions/storyteller.ts:58, :64` the INFO key + the pass-through;
`orchestrator.service.ts:310` the request type. SPA: `StorytellerBoard.tsx` +271/−… (the sortable board,
`arrangeBids`, `withBid` export, order badges, Reset order, the two hint sentences);
`useSSEStreaming.ts:1547, :1593` `keepOrder = false` → body; `_storyteller.css` +35 (`-draggable`,
`-dragging`, `-card-order`, `-board-tools`); `__tests__/unit/components/chat/storyteller-board-arrange.
test.ts` (4 cases, §5.6). `help/storyteller-mode.md`, `API.md`, the spec, CHANGELOG. **Port with
`b15fe9404` as one unit** (the ledger is right).

### 1.3 `7d7e38d2f` — tests only (VERIFIED)

`lib/storyteller/__tests__/plan-run-order.test.ts` (90 lines; quoted in full in the task's source): three
cases — rotation order `['c','a','b']` from `resolveCycleOrder` when not arranged; `keepOrder` keeps
`['b','a','c']` and `resolveCycleOrder` is NOT called; `simultaneous` + `keepOrder` over `[c(act), d(lookup),
a(act), b(simple)]` → `[['d',true],['b',true],['c',false],['a',false]]` (the concurrent group first in the
operator's order, then the Act tail in the operator's order). The toast-stacking test is P4.D338's surface
(no change). → seed rows for the tier-1 `plan_storyteller_run` corpus (§5.1).

### 1.4 `8becfc6ce` — dead-code sweep (NO-PORT)

`lib/tools/effects.ts` −6: `isComputedTool(name)` removed (the pin has no such export — never add a v5
twin). The other three deletions (`components/inventory/index.ts` barrel, `resolveMemoryMaintenance`,
`ProjectFileInfo`) are outside this slice; `harness/oracle` imports none of the four (ledger census confirmed
— `grep -rn "isComputedTool\|resolveMemoryMaintenance\|ProjectFileInfo" harness/` → 0 hits).

### 1.5 Observable event ORDER on a storyteller send and run (what the tier-3 frame diff pins)

- **Send in storyteller mode (public post):** `status initializing` → `turnStart {participantId,
  characterName, chainDepth:0}` + `status resolving` (the responder IS resolved and announced at `:448-467`
  BEFORE the hold returns at `:630` — a held post still names the seat that would have answered; the same
  as the paused hold) → the post-recording frames an ordinary send emits before the tool build
  (`fileProcessing`, Pascal / RNG / Librarian rows, `status` beats) → `storytellerRound {storytellerRound:
  <round row with pending cards>}` → `storytellerBid {storytellerBid: {roundId, bid}}` × N as they land (within 45 s) → `roundOpen
  {roundOpen:true, roundId}` → `chainComplete {chainComplete:true, reason:'round_open', nextSpeakerId:null,
  chainDepth:0}`. An `@Name:` post: the same prefix, then `chainComplete {reason:'user_turn', …}` only.
  (`a8214b253`'s `encodeTurnDoneEvent` key order is P4.D348's.)
- **`storyteller-run` sequential:** per turn `turnStart {participantId, characterName, chainDepth: n≥1}` →
  the turn's own frames (`status`, `content`…, `done`) → `turnComplete {participantId, messageId, chainDepth:
  n, skipped?}`; after all → ONE `chainComplete` (`user_turn` | `yielded` | `stopped`+`paused:true`,
  `chainDepth: turns.length`). **All at once:** the concurrent group's frames carry NO `turnStart` /
  `turnComplete` from the runner (`onTurnStart` / `onTurnEnd` return early), but every frame the turn itself
  writes — including its own depth-0 `turnStart` at `:462` — is tagged `participantId` + `concurrent:true`;
  then the Act / Destroy tail as sequential turns.

## 2. The v5 surface map (file:line at `f6cdceedd`; what v5 does TODAY)

### 2.1 Tool effects

| v4 (pin) | v5 counterpart | v5 today |
|---|---|---|
| `lib/tools/effect-types.ts` | **NEW** `crates/quilltap-core/src/tools/effects.rs` (one module for both v4 files; `pub enum ToolEffect { None, Read, Write, Destroy }` with `as_str` / `rank`) | nothing |
| `lib/tools/effects.ts` `BUILT_IN_TOOL_EFFECTS` | **NEW** table in `tools/effects.rs` keyed by `function.name` — NOT a field on `ToolDef` (`tools/definitions/mod.rs:19-23` is `{key, name, json}` with byte-exact JSON; adding a field there is fine but the four computed effects need code, so one table + one `effect_of_tool_call(name, &Value, &EffectLookups)` is the DRY home) | `tool_build.rs:757 DESTRUCTIVE_TOOL_NAMES` (3 names), `:768 destructive_allowed`, `:774 filter_destructive_tools`, `:788 apply_autonomous_destructive_filter` (+ the P4.D234 INFO `:800`) — ALL RETIRE |
| the four computed effects | the resolvers read the call's RAW `arguments` with the tool's own validator: v5's `tools/state.rs`, `tools/edit_custom_tool.rs`, `tools/doc_edit/*` (`doc_open_document`), `tools/run_custom.rs` each already parse their input — expose a `pub fn <tool>_effect(input: &Value, lookups) -> ToolEffect` beside each, mirroring v4's per-file export | none |
| `lib/pascal/tool-effect.ts` | **NEW** `pascal/tool_effect.rs` `custom_tool_effect(&QtapCustomTool) -> ToolEffect` over `custom_tool_types.rs:1347-1364`'s four predicates; `custom_tool_effect_map` | none |
| `ToolExecutionContext.effectCeiling / customToolEffects / pluginToolEffects` | `services/tool_execution.rs:257-316` `ToolExecutionContext` (+3 fields; `create_tool_context` `:323` leaves them unset) | no ceiling |
| `executeToolCallWithContext`'s check (`tool-executor.ts:343-371`) | `tools/executor.rs:2250-2275` `BuiltInToolRunner::run` — the check goes right after the mountPool/operatorSurface refusal (`:2260-2271`) and BEFORE `self.dispatch` / the fallback (plugin tools fall through the fallback in v5) | none |
| `applyEffectCeilingToTools` + `BuildToolsOptions.effectCeiling` + `BuiltTools.customToolEffects/pluginToolEffects` | `services/tool_build.rs:526-582 BuildToolsInput` (+`effect_ceiling: Option<ToolEffect>`), `:586-590 BuiltTools` (+ the two maps), `:595 build_tools` — apply LAST, after the disabled-tool filter and the `run_custom` description build (`:464-478`) | the autonomous filter is a POST-step in `orchestrator.rs:2348-2370` |
| `resolveTurnEffectCeiling` (`turn-tools.service.ts:221-238`) | `services/tool_build.rs` as `pub fn resolve_turn_effect_ceiling(chat_type, policy, allowed_at_room, storyteller: Option<ToolEffect>) -> Option<ToolEffect>` (pure; the caller in `orchestrator.rs:2348` passes the result as `effect_ceiling`) | `destructive_allowed` |
| `buildTurnToolContext` copies the maps / ceiling | `orchestrator.rs:786 turn_tool_context(TurnToolContextArgs)` | none |
| `trigger-runner.ts:63-68, :170-183` | `pascal/trigger_runner.rs:74-86 RunTriggeredCustomToolsParams` (+`effect_ceiling: Option<ToolEffect>`), the skip inside `run_triggered` `:121+` before `run_custom_tool`; `services/trigger_phrase_sites.rs:90-134 ReplyTriggerParams` (+ the field); `message_finalizer.rs:1357-1372` the call site (+ the field) | no ceiling. ⚠ P4.D345 (intervention) edits `trigger_runner.rs` too — §7 |
| `tool-registry.ts getToolEffects`, plugin-types 2.9.0 | **NO-COUNTERPART** — v5 has no tool-plugin registry (`tools/executor.rs:2274` "the injected fallback"); `plugin_tool_effects` stays an empty map (R-2). The vendor lane records plugin-types 2.9.0 / MCP 1.1.45 / curl 1.0.32 as NO-PORT | — |
| `workbench.ts:54 effect` | `pascal/workbench.rs:50-69 CustomToolLibraryEntry` (+`effect: &'static str` after `trigger_count`; `:399` the builder) → the wire key `effect` between `triggerCount` and `parameterCount` (`core-contract.ts:7637-7656`) | no key |
| `streaming.service.ts toolChoice` | `model/stream.rs:499-536 StreamParams` (+`tool_choice: Option<String>` — `'none'` only ever sent), `model/streaming_provider.rs:200 tool_choice: None` → `params.tool_choice.map(Value::String)`, `request_builder/mod.rs:111 RequestInput.tool_choice` already exists; `chat_completions.rs:669-678` (openai-compatible, defaults `"auto"`), `:832-836` (deepseek, pass-through), `:908-914` (z-ai, pass-through); the nanogpt body — find its builder (`grep -n nanogpt request_builder/`) and give it `?? "auto"`; anthropic / google / responses_api / ollama / grok / openrouter must NOT emit it (openrouter hard-codes `tool_choice:'auto'` already — verify `chat_completions.rs`'s openrouter arm) | deferred |
| `LLMLogTypeEnum 'STORYTELLER_BID'` | `services/llm_logging.rs:435` `log_type::STORYTELLER_BID` after `SCENARIO_BUILDER`; `primary_stream.rs:1062 log_stream_message_call(row_log_type)` already takes the row type | absent |

### 2.2 Storyteller — server

| v4 (pin) | v5 counterpart | v5 today |
|---|---|---|
| `lib/schemas/storyteller.types.ts` | **NEW** `crates/quilltap-core/src/storyteller/types.rs` (serde structs in v4 key order; `StorytellerBid`, `StorytellerRound`, `StorytellerSelection`, `StorytellerBidRecord`, the enums) | nothing |
| `lib/storyteller/{tiers,invite,cards,prompts,bid-parse,sse-tag}.ts` | **NEW** `storyteller/{tiers,invite,cards,prompts,bid_parse,sse_tag}.rs` (pure; `invite.rs` over `cycle_order.rs:76 cycle_candidates` + `participant_filters.rs:55 is_user_driven_seat`; `prompts.rs` imports `skip_signal::NOTHING_TO_ADD_SENTINEL`; `bid_parse.rs` over the JS-regex twin) | nothing |
| `lib/storyteller/round-context.ts` | `services/llm_logging.rs:45-49 LogContext` (+`storyteller_round_id: Option<String>`), `:315` the stamp (+ a line), threaded through `ProcessMessageInput.log_context` (`orchestrator.rs:485`) — explicit, like `autonomous_run_id` (`enclave/step.rs:728` precedent) | `LogContext { autonomous_run_id }` only |
| `lib/storyteller/round.service.ts` | **NEW** `services/storyteller_rounds.rs` (the lifecycle: open / ask / lie / plan / run; the per-round lock → a `Mutex<HashMap<String, Arc<Mutex<()>>>>` process static, or serialise through the single writer — the writer task already serialises writes; the lock in v4 guards READ-MODIFY-WRITE of `bids`, so `record_bid` must be ONE `Db::write` closure that re-reads inside the writer) | nothing |
| `lib/repositories/dry-scope.ts` + `method-classification.ts` | **NO-COUNTERPART** — R-1 (§10). The v5 bid build must not write: the sites a `purpose:'bid'` build still reaches in v5 are the Core-whisper post (`build_context.rs` `core_whisper`), the stale-whisper sweep, the scene-cache update (`compression_cache`), the inform consume (finalizer — not reached, the bid returns before), `forceToolsOnNextMessage` spend (`orchestrator.rs:2123-2140`), the raised-hand take (gated `!isBid` in v4 — mirror), the Prospero cadence post (gated), `agentTurnCount: 0` reset (`orchestrator.rs:1414+`, v4 `:496-498` — NOT gated by `isBid` in v4 but a bid is continue-mode so it never fires), `resolveCycleOrder`'s draw persist inside `plan_storyteller_run` (a REAL write, not in the bid) | — |
| `lib/database/repositories/storyteller-rounds.repository.ts` | **NEW** `db/storyteller_rounds.rs` (reader + `StorytellerRoundsWriter` on `writers.main()`; JSON `bids` / `selection` columns; `find_active_for_chat`, `find_latest_for_chat`, `create`, `record_bid`, `mark_running`, `mark_closed`) | nothing |
| `chats.repository.ts setTurnMode` + `patchOnlyFields` | `db/chats.rs` `set_turn_mode(chat_id, mode) -> bool` beside `set_concierge_mode` `:1818` (the raw `UPDATE … SET "turnMode"=?, "updatedAt"=?`; `ChatUpdate` `:617` must NOT gain `turn_mode`) | none |
| `turn-holds.service.ts resolveTurnHold / finishHeldPost` | `services/paused_hold.rs` (`should_hold_user_turn_for_pause`) + `orchestrator.rs:809 finish_held_user_turn` + `:1188-1203` the hold read + `:2100-2110` the hold return — **P4.D348's region**; P4.D343 adds the storyteller arm through contract S1 (§7) | paused only |
| `handleSendMessage`'s `storytellerHold` branch + `storytellerSingleTurn` | `crates/quilltap-host/src/spine.rs:1460 run_send` — after `process_message_with_ledger` (`:1642`) and before `execute_turn_chain` (`:1725`); `single_turn: false` at `:1736` is where bug 258's `is_whisper` (P4.D348) and `storyteller_single_turn` both land — contract S2 | neither |
| `openRoundOnSend` / `runStorytellerBid` / `handleStorytellerRun` | **NEW** `crates/quilltap-host/src/storyteller_spine.rs` (or a `ChatSpine` impl block in a new file): the round opener (needs the spine's `ProcessMessageInput` factory — `make_chain_input`-style closure at `spine.rs:1692-1718`), the bid runner (a `process_message_with_ledger` call with `storyteller_bid: Some(..)` and a discarding sink), the run endpoint (registers `turn_control::register_active_turn(&chat_id)` `spine.rs:1468` like `run_send`, and drives `runStorytellerRound` with per-turn `process_message_with_ledger` calls under a tagging `EventSink` wrapper) | nothing |
| `sinkController` / `taggingController` | an `EventSink` adapter in `services/chat_events.rs` or the host: `TaggingSink { inner, participant_id }` that rewrites each `ChatEvent` to carry `participantId` (when absent) + `concurrent: true` — the v5 `ChatEvent` is an `untagged` enum (`chat_events.rs:276-278`) with typed payloads, so the tag must be a serialisation-time merge (a `ChatEvent::Tagged { inner: Box<ChatEvent>, participant_id, concurrent: true }` variant whose `Serialize` flattens inner then the two keys — key order `inner keys…, participantId, concurrent` exactly as v4's spread) ; `DiscardSink` for the bid | none |
| `SendMessageOptions.storytellerBid / storyteller`; `ProcessMessageResult.storytellerHold / bid` | `orchestrator.rs:399-437 SendMessageOptions` (+2), `message_finalizer.rs:572-594 ProcessMessageResult` (+2) — **shared structs** (contract S1) | none |
| `resolveTurnSkipOffer` `isStorytellerTurn / isBid` | `orchestrator.rs:1722-1740` the `turn_skip` resolve (+ the two arms) and `:2714 turn_skip: turn_skip.clone()` into the context (storyteller → `None`) while `:3808` keeps using `turn_skip` for the sentinel — the SPLIT the lane must make | one value feeds both |
| `buildStorytellerSection` + the bid short-circuit | `orchestrator.rs` between the slate (`:2346`) and `build_context_input` (`:2650`) for the section; the bid return after `formatted_messages` is built and before the Courier dispatch (`:2916`) — v4's order is Courier-refusal (`:781-792`) THEN bid (`:836`) but both precede any streaming | none |
| `BuildMessageContextOptions.purpose / storytellerSection`; `turn-trailing.ts storyteller` LAST | `services/build_context.rs` `BuildContextInput` (+`purpose: ContextPurpose`, `storyteller_section: Option<String>`), `:3118-3127` the trailing list (+ last), the recall delivery skip (`deliver_recall` call — find `deliver_recall(` in `build_context.rs`) — **P4.D347's file**; contract C1 (§7) | eight sections |
| `message-finalizer` `storyteller?` (bid write, trigger ceiling, housekeeping deferral) + `runStorytellerRoundHousekeeping` | `message_finalizer.rs:598-660 FinalizeOptions` (+`storyteller: Option<StorytellerTurnContext>`), the `storytellerBid` write after the assistant save (find the `update_message` of `debugMemoryLogs` — v4's `:305` sits just before the recall-record write `:319`), `:1357` the trigger call, `:1520-1547` the triggers block (+ the deferral DEBUG), a new `pub async fn run_storyteller_round_housekeeping` beside `trigger_turn_memory_extraction` `:1625` — ⚠ the context-summary check is a DOCUMENTED DEFERRAL in v5 (`:1529-1535`); the round housekeeping inherits the same hole | none |
| `regenerate-swipe.service.ts:206-216, :346-347` | `services/regenerate_swipe.rs:642+` the `BuildContextArgs` (+`storyteller_section` from the target's `storytellerBid` in reroll form), `:1060-1064` the swipe row (+ copy `storytellerBid`) — P4.D348 also edits this file (`eb2d87788` streams the swipe through `streamMessage`); the two hunks are disjoint regions (§7) | none |
| `actions/storyteller.ts` | **NEW** `api/chat_storyteller.rs` (the four handlers over `Response`), `api/types.rs` `Request::{ChatStorytellerRound, ChatStorytellerRun, ChatStorytellerAsk, ChatStorytellerLie}` + `Response::{…}` (contract W1), `api/engine.rs` four arms (the run arm through `SpineFactory` like `Request::ChatSend` `:2070`), `crates/quilltap-web/src/wardrobe_routes.rs` GET arm + three POST arms + `CHAT_GET_ACTIONS` (18th key between `informs` and `group-stores`) + `CHAT_POST_ACTIONS` (three trailing keys) + the ordering test `:1301` | none |
| `handlers/get.ts:377-379` the chat GET wire | `api/salon.rs:424` (+`turnMode` `?? 'rotation'` and `storytellerInviteFilter` `?? false` right after `isPaused`) — the keystone widens `chats_read::find_by_id`; the wire key is this lane's | absent |
| `helpers.ts processChatUpdates turnMode` + `schemas.ts` | `api/salon.rs:1597 chat_update` (+ a `turn_mode: Option<Option<Value>>` param; the `double_option` sibling on `Request::ChatUpdate` `types.rs:383-403`; `salon.rs:2039-2049` the bag's boolean list + `storytellerInviteFilter`) — ⚠ P4.D356 edits `chat_update`'s neighbours (`coreWhisper` is the SETTINGS PUT, `settings.rs`, not this fn — verify) | absent |
| `delete-service.ts storyteller_rounds` | `services/delete_all.rs:201-228 FORMAT3_MAIN_TABLES` (+ append) + the test `:643-657`; `services/backup/collect.rs` — nothing to add (not carried) | absent |
| `realtime.types.ts 'storytellerRounds'` | `realtime/types.rs:40-48, :52-63` (+ `RealtimeTopic::StorytellerRounds => "storytellerRounds"` LAST) + the test `:109-126` ("v4's eight in order" → nine) + `crates/quilltap-harness/tests/realtime_topics_equivalence.rs` + `realtime_publish_sites_guard.rs` (lists every `publish_realtime(` site by text — the round service's six calls must be added) + SPA `core/realtime-topic-map.ts` | eight topics |
| `llm-logging.service.ts:225` | `services/llm_logging.rs:315` (+`storyteller_round_id: ctx.storyteller_round_id.clone()`), `db/llm_logs.rs:283/329/373` the three row structs, `:426 LOG_COLUMNS`, `:552-573` the insert (keystone's plumbing; this lane's stamp) | none |
| `help_navigate`'s page registry | v5 embeds `help/` whole; the vendor lane's re-vendor adds `storyteller-mode.md` | — |

### 2.3 SPA — see §6.

## 3. Schema / D23 shapes (the keystone P4.D341 carries the moves; this lane consumes them)

**The two migrations' exact DDL** (`migrations/scripts/add-storyteller-mode-v1.ts:55-86`,
`add-llm-logs-storyteller-round-id-column.ts:97-106`):

```sql
ALTER TABLE "chats" ADD COLUMN "turnMode" TEXT DEFAULT NULL;                 -- addColumnIfMissing
ALTER TABLE "chats" ADD COLUMN "storytellerInviteFilter" INTEGER DEFAULT 0;
ALTER TABLE "chat_messages" ADD COLUMN "storytellerBid" TEXT DEFAULT NULL;
CREATE TABLE IF NOT EXISTS "storyteller_rounds" (
  "id" TEXT PRIMARY KEY,
  "chatId" TEXT NOT NULL,
  "triggerMessageId" TEXT,
  "status" TEXT NOT NULL,
  "bids" TEXT DEFAULT '[]',
  "selection" TEXT DEFAULT NULL,
  "closedReason" TEXT DEFAULT NULL,
  "createdAt" TEXT NOT NULL,
  "updatedAt" TEXT NOT NULL,
  "closedAt" TEXT DEFAULT NULL,
  FOREIGN KEY ("chatId") REFERENCES "chats"("id") ON DELETE CASCADE
);
CREATE INDEX IF NOT EXISTS "idx_storyteller_rounds_chat" ON "storyteller_rounds" ("chatId", "status");
-- llm-logs partition (skipped entirely when the logs file is absent):
ALTER TABLE "llm_logs" ADD COLUMN "storytellerRoundId" TEXT DEFAULT NULL;
CREATE INDEX IF NOT EXISTS "idx_llm_logs_storytellerRoundId" ON "llm_logs" ("storytellerRoundId");
```

**generateDDL's fresh rendering** (from the Zod schemas; `docs/developer/DDL.md` at the pin agrees):
`chats`: `"turnMode" TEXT DEFAULT NULL` and `"storytellerInviteFilter" INTEGER DEFAULT 0` between
`"isPaused"` and `"isManuallyRenamed"` (the ledger says the fresh `turnMode` renders bare `TEXT` — the
nullable-optional rule; DDL.md's annotated copy writes `TEXT DEFAULT NULL`; **the re-dump decides**, and the
difference is immaterial to reads); `chat_messages`: `"storytellerBid" TEXT DEFAULT NULL` after
`"confirmationOriginalContent"`, before `"pascalMeta"`; `storyteller_rounds` rendered from
`StorytellerRoundSchema` in the repository container (id, chatId, triggerMessageId, status, bids, selection,
closedReason, createdAt, updatedAt, closedAt — **without** the FK and **without** the index: both must be
boot ensures per the 2026-10-19 ruling, the index via `migration_indexes.json`); `llm_logs`:
`"storytellerRoundId" TEXT` after `"autonomousRunId"` and its index in the dedicated-DB DDL. Two-shape
columns: all three ALTERed columns carry the same shape in both (`TEXT DEFAULT NULL` / `INTEGER DEFAULT
0`); the fresh `turnMode` may lack `DEFAULT NULL` — column-gate the ensure on presence, not on shape.

**Stamping (ping-pong rule):** `add-storyteller-mode-v1.shouldRun` re-runs whenever ANY of the four is
missing, so a v5-provisioned instance whose `fresh_schema.json` carries all four is a no-op for v4 → **do
not stamp** (nothing v4 would re-run). `add-llm-logs-storyteller-round-id-column-v1.shouldRun` is false when
the column exists → **do not stamp**. (The keystone survey owns the final call; this is the measured input.)
`prettify.ts` labels are v4-only boot text — v5's boot ensures log their own lines; the labels are NO-PORT.

**`turnMode` semantics:** `NULL` reads `'rotation'` everywhere (`handlers/get.ts:378 ?? 'rotation'`,
`helpers.ts:591 ?? 'rotation'`, the schema `.nullable().optional()`); `setTurnMode('rotation')` writes NULL
(never the string); only `setTurnMode` writes it (`patchOnlyFields`). `storytellerInviteFilter` is a plain
boolean column, written through the ordinary `chat` bag. `storytellerBid` JSON shape: `{"roundId": string,
"tier": "simple"|"lookup"|"act"|"destroy", "plan": string, "toolsNamed": string[]}` (serde key order =
`StorytellerBidRecordSchema` order; v4 writes it with `updateMessage` → a `JSON.stringify` of the record,
`toolsNamed` always present). `storyteller_rounds.bids` is a JSON array of `StorytellerBid` in schema key
order; `selection` a JSON object or NULL; `status ∈ open|running|closed`; `closedReason ∈
ran|let-lie|superseded|abandoned`.

**Restore / vintage:** the keystone's `restore_vintage_state` mirror-boot list must include the new
ensures (memory note `vintage-restore-mirror-boot-list`); `storyteller_rounds` is TRUNCATED by a replace
restore (`delete_all.rs` list) and never restored.

## 4. Log lines (v4 text + structured keys + level → v5 home or NEW)

| # | v4 line (pin) | level / logger | keys | v5 home |
|---|---|---|---|---|
| 1 | `Effect ceiling applied to tool list` | DEBUG `streaming.service` | `ceiling, kept, dropped[]` | NEW `tool_build.rs` (replaces P4.D234's INFO `Autonomous room: destructive tools filtered from per-turn list` `:800` — RETIRE it and the harness grep `tool_build_equivalence.rs:312`) |
| 2 | `Autonomous room: destructive tools withheld by effect ceiling` | DEBUG `TurnTools` | `chatId, policy, allowedAtRoom` | NEW `tool_build.rs::resolve_turn_effect_ceiling` |
| 3 | `[TurnTools] Tool slate resolved` | DEBUG `TurnTools` | `chatId, characterId, toolCount, resolvedToolMode, effectCeiling, toolSettingsChanged` | `a8214b253`'s line (P4.D348 decides whether it lands; if it does, `effectCeiling` is this lane's key) |
| 4 | `Tool call refused: above the turn's effect ceiling` | INFO `tool-executor` | `context:'tool-executor', toolName, chatId, effect, ceiling` | NEW `tools/executor.rs::run` |
| 5 | `Tool call within the turn's effect ceiling` | DEBUG | `context, toolName, effect, ceiling` | NEW, same site |
| 6 | `Triggered custom tool skipped — above the turn's effect ceiling` | DEBUG `pascal.trigger-runner` | `context, chatId, tool, phrase, effect, ceiling` | NEW `pascal/trigger_runner.rs::run_triggered` |
| 7 | `Failed to read plugin tool effects; plugin tools count as write` | WARN | `userId, error` | NO-COUNTERPART (no plugin registry) |
| 8 | `Plugin tool effects unavailable; its tools count as write` | WARN registry | `plugin, error` | NO-COUNTERPART |
| 9 | `Storyteller bid call failed` | WARN `StorytellerBidCall` | `chatId, characterId, error` | NEW `storyteller/bid_call.rs` |
| 10 | `Storyteller bid call returned` | DEBUG | `chatId, characterId, length` (UTF-16 length) | NEW |
| 11 | `Storyteller bid landed` | INFO `storyteller.round` | `context, chatId, roundId, participantId, status, tier` | NEW `services/storyteller_rounds.rs` |
| 12 | `Storyteller round closed unrun` | INFO | `context, chatId, roundId, reason` | NEW |
| 13 | `Storyteller round opened` | INFO | `context, chatId, roundId, invited, keptQuiet` | NEW |
| 14 | `Storyteller board timeout — the board opens with bids still out` | INFO | `context, chatId, roundId, timeoutMs` | NEW |
| 15 | `Storyteller round let lie` | INFO | `context, chatId, roundId` | NEW |
| 16 | `Storyteller run keeps the operator's order` | DEBUG (no `context`) | `chatId, roundId, participantIds` | NEW (`946a50a04`) |
| 17 | `Storyteller round running` | INFO | `context, chatId, roundId, order, speakers[]` | NEW |
| 18 | `Storyteller round closed` | INFO | `context, chatId, roundId, ended, spoke[]` | NEW |
| 19 | `Storyteller bid dropped — round not open` | DEBUG `storyteller-rounds.repository` | `context, roundId, participantId, status` | NEW `db/storyteller_rounds.rs` |
| 20 | `Error finding active storyteller round` / `Error finding latest storyteller round` | ERROR (safeQuery) | `chatId` | NEW (the fallback-read class, P4.149's homes) |
| 21 | `Failed to write turn mode` | ERROR (safeQuery) | `chatId, mode, context:'storyteller'` | NEW `db/chats.rs::set_turn_mode` |
| 22 | `Turn mode write` | DEBUG | `chatId, mode, written, context:'storyteller'` | NEW |
| 23 | `[Chats v1] Turn mode set` | INFO | `chatId, from, to` | NEW `api/salon.rs::chat_update` |
| 24 | `[Storyteller] Request refused` | INFO | `chatId, action, reason` | NEW `api/chat_storyteller.rs` |
| 25 | `[Storyteller] Running a selection` | INFO | `chatId, roundId, count, order, keepOrder` | NEW |
| 26 | `[Orchestrator] Recording the operator post without a reply` | INFO | `chatId, userId, hold:'paused'|'storyteller', hasContent, attachmentCount` | `orchestrator.rs:1196-1202` today logs `[Orchestrator] Chat paused — recording the user message without a reply` — `a8214b253` MERGED the two (P4.D348's rename); the storyteller arm adds `hold:'storyteller'` |
| 27 | `[Storyteller] Carina query — no round opens` | DEBUG | `chatId` | NEW (host round opener) |
| 28 | `[Storyteller] Bid build failed` | WARN | `chatId, participantId, roundId, error` | NEW |
| 29 | `[Storyteller] Speaking turn carries its bid` | DEBUG | `chatId, participantId, roundId, tier, ceiling` | NEW `orchestrator.rs` |
| 30 | `Failed to record the storyteller bid on its message` | WARN `MessageFinalizer` | `chatId, messageId, error` | NEW `message_finalizer.rs` |
| 31 | `Storyteller turn: housekeeping deferred to the end of the round` | DEBUG | `chatId, roundId` | NEW |
| 32 | `Storyteller round housekeeping` | DEBUG | `chatId` | NEW |
| 33 | `Dry scope discarded writes` / `Dry scope discarded an unclassified repository method` | DEBUG / WARN `dry-scope` | `label, discarded{}` / `label, method` | NO-COUNTERPART (R-1) |
| 34 | migration INFOs (§1.1.9) | INFO `migration.*` | — | the keystone's boot-ensure lines |
| — | `[Orchestrator] Storyteller chat — recording the operator post; a round will open` (INFO), `Tools above the turn's effect ceiling filtered from per-turn list` (INFO), `[Storyteller] Round housekeeping failed` (WARN) | **`b15fe9404` lines that do NOT exist at the pin** (merged / dropped by `a8214b253`) — do not port | — |

## 5. Harness / oracle impact

### 5.1 Existing families — what the pin does to each

| family (`crates/quilltap-harness/tests/`) | oracle case | at the pin | why / what to do |
|---|---|---|---|
| `tool_build_equivalence.rs` | `cases/tool-build.test.ts` | **IMPORT-DEAD** (static `import { DESTRUCTIVE_TOOL_NAMES } from '@/lib/tools/destructive-tools'` `:54` — jest fails to resolve the module; the case does not run) | **RE-AIM (P4.D343, first unit):** drop the import and the hand post-filter `:194-203`; for `chatType === 'autonomous'` cases compute `effectCeiling = destructiveAllowed ? undefined : 'write'` (v4 `resolveTurnEffectCeiling` logic, inlined — it is 6 lines, or import it from `@/lib/services/chat-message/turn-tools.service` if the jest env loads it) and pass `effectCeiling` INTO `buildTools`; emit `customToolEffects` / `pluginToolEffects` (as sorted entries) beside `tools`; the Rust side passes `effect_ceiling` into `build_tools` and compares the two maps. Re-measure `p4d234_destructive_autonomous_mail_only` (`:317`): with `delete_annotation` = `destroy` the slate's withheld set is `{discard_mail, delete_annotation, doc_delete_blob?}` — rename/retire the row's premise; the INFO grep `:312` → the two DEBUGs. The recipe header `:44-48` stays (same env vars). Fixture `harness/oracle/fixtures/tool-build.json` (27+ cases) + `build-tool-build-fixture.ts` — add rows: a storyteller ceiling of each level over the full slate (`none` drops everything but `rng` / `whisper` / `submit_final_response` / `edit_custom_tool`+note; `read` keeps the 29 reads + the four computed with notes); a `run_custom` roster with one read and one write tool (the `customToolEffects` map + the `run_custom` note). |
| `orchestrator_tier3_equivalence.rs` | `cases/orchestrator-tier3.test.ts` (+ `fixtures/build-orchestrator-fixture.ts`, `orchestrator-tier3.json`) | REDDENS only through P4.D348's shifts (`a8214b253`'s `encodeTurnDoneEvent` key order, the merged hold INFO); the storyteller paths are NOT exercised by the committed corpus (every chat `turnMode` NULL) — the family is **GREEN for this lane on the union** and is the home for the NEW storyteller rows (§5.2) | add rows in the SAME fixture builder (the builder runs v4's real migrations at the pin, so the new columns exist): a storyteller chat (`setTurnMode` through the repo) with 2 LLM seats + the operator; the canned provider answers the TWO bid calls (keyed by the request's last trailing message — the bid section bytes are in the canned key, so a byte drift in `prompts.rs` is a canned MISS, loud) and the chosen speaking turn. Needs the oracle's `createLLMProvider` mock to pass `params.toolChoice` through to the recorded key (it records `provider|model|temperature|messages` today — add `toolChoice` to the key or assert it per call) |
| `native_tool_loop_tier3_equivalence.rs` / `text_tool_loop_tier3_equivalence.rs` | `native-tool-loop-tier3.test.ts` (executor CANNED) | unchanged (the ceiling is inside the executor, canned away) | none; the ceiling refusal is proven in `tool_executor_ceiling` (§5.2) |
| `message_finalizer_tier3_equivalence.rs` | `message-finalizer-tier3.test.ts` | unchanged for `storyteller: undefined`; the `runReplyTriggers` call gains `effectCeiling: undefined` | add two rows: a storyteller turn (`storyteller: {bidRecord, effectCeiling:'read', spokenSince: []}`) → the `storytellerBid` written on the row + the deferral DEBUG + NO extraction/danger enqueue; a trigger phrase under a `read` ceiling naming a write tool → skipped (needs the `pascal-run-custom-triggers.ts` extension's roster — `extend-pascal-run-custom-triggers.ts`) |
| `regenerate_swipe_tier3_equivalence.rs` | `regenerate-swipe-tier3.test.ts` | unchanged for rows without `storytellerBid` | add a row whose target carries `storytellerBid` → the reroll plan section in the canned key + the copied column on the new swipe |
| `salon_reads_equivalence.rs` | `salon-reads.test.ts` (committed pair `crates/quilltap-web/tests/fixtures/salon-{main,mount}.db`) | **REDDENS**: v4's chat GET wire gains `turnMode: 'rotation'` + `storytellerInviteFilter: false` after `isPaused`; the COMMITTED pair LACKS the columns (built before the move) — v4's `SELECT *` + `?? 'rotation'` / `?? false` still renders both keys, v5's strict `ALL_COLUMNS` SELECT would fail `no such column: turnMode` | the keystone widens the pair (memory `rebuilt-fixture-pair-vintage`: diff `sqlite_master`, add through v4's real migration, never by hand) and `chats_read.rs`; this lane adds the two wire keys in `salon.rs:424` and a `turnMode:'storyteller'` + filter-on row |
| `chats_read_equivalence.rs` | `chats-read.ts` | **REDDENS** (keystone): `storytellerInviteFilter: false` materialized, `turnMode` dropped when NULL | keystone |
| `llm_logs_tier2_equivalence.rs` / `llm_logs_routes_equivalence.rs` | `llm-logs-tier2.ts` (`SELECT *`), `llm-logs-routes.test.ts` | **REDDENS** (keystone): the new column in every dumped row; the routes' row DTO gains `storytellerRoundId` | keystone plumbs; this lane adds a `STORYTELLER_BID` row with `storytellerRoundId` set + the SPA type |
| `realtime_topics_equivalence.rs` + `realtime_publish_sites_guard.rs` + `realtime/types.rs` test | `realtime-topics.ts` | **REDDENS**: nine topics; `queryKeysForTopic('storytellerRounds', id)` → `[['chats', id, 'storytellerRound']]`, bare → `[chats.all]` | P4.D343 (server topic + the six publish sites) / P4.D344 (the SPA map, keyed `chatKeys.storytellerRound`) |
| `pascal_workbench_equivalence.rs` / `pascal_workbench_route_equivalence.rs` | `pascal-workbench.test.ts` / `-route.test.ts` | **REDDENS**: every library entry gains `effect` | P4.D343 (`workbench.rs` + `custom_tool_effect`); P4.D346/P4.D359 do NOT touch the entry struct — confirm |
| `pascal_trigger_runner_equivalence.rs` / `pascal_trigger_phrases_equivalence.rs` | `pascal-trigger-runner.test.ts` / `pascal-trigger-phrases.ts` | unchanged (no ceiling in the corpus) | add the D9 rows (v4's two new `it`s, §1.1.3 of `trigger-runner.test.ts`) — ⚠ P4.D345 edits `trigger_runner.rs` for the hold; the ceiling check sits BEFORE `run_custom_tool` and the hold inside it — disjoint lines, same fn (§7) |
| `provisioning_equivalence.rs` / `restore_vintage_state.rs` / `system_restore_*` | `characters-provision.ts`, `system-restore.test.ts` | **REDDENS** (keystone): the fresh schema, the ensures, the truncate list | keystone + this lane's `delete_all.rs` row |
| `request_builder_equivalence.rs` (+ google / wire) | `request-prefix-hashes.ts`?, the recorded `request-envelopes.recorded.ndjson` | unchanged until `tool_choice` is set | add `mode`-tagged vectors with `toolChoice:'none'` for nanogpt (`"none"`), deepseek, z-ai, openai-compatible, and NEGATIVE vectors (anthropic / google / openai / ollama / grok / openrouter emit NO `tool_choice` / keep `'auto'`) — recorded from v4's real plugins (P4.11's recorder) |
| `tool_definitions` snapshot / `tool-definitions.ts`, `tool-definitions-canonical.ts` | — | unchanged definitions; `tool-definitions-canonical.ts` is on the known IMPORT-DEAD list (`UniversalTool` TYPE import — erased by tsx; **verify**, likely alive) | the effects snapshot (v4 `tool-definitions-snapshot.test.ts:191-266`) becomes `tool_effects_equivalence` |
| `custom_tool_schema_embed_guard`, `qtap_schema_embed_guard`, `public_schemas_vendor_guard` | — | **REDDENS**: `qtap-export.schema.json` +19 (`generators/qtap-export.schema.json` embed) | vendor lane (P4.D360) re-vendors; this lane's `.qtap` carriers must accept `turnMode` / `storytellerInviteFilter` / `storytellerBid` on import (the chat-record column list — P4.167's `import_chat_record_columns`) |
| `almanack_tier2_equivalence` | — | the Almanack's `runDestructiveToolsAllowed` label text unchanged by the commit (only the schema comment moved) | none |
| `trigger_sites_order_guard`, `stream_watchdog_wrap_census`, `web_edge_body_parse_guard`, `COLLAPSE_CENSUS` | — | the new web arms (`storyteller-run` reads a body with `and_then(Value::as_` → a `COLLAPSE_CENSUS` row, memory `frozen-engine-seams-and-web-censuses` §3); the new `openTurnStream` twin in the host needs the watchdog census row if it wraps a stream | P4.D343 |

### 5.2 NEW families this lane must add (each driving v4's REAL code at the pin)

1. **`tool_effects_equivalence` (tier 1)** — oracle `cases/tool-effects.ts` importing `@/lib/tools/effects`:
   dump `BUILT_IN_TOOL_EFFECTS` as `{name: effect | {computed: maxEffect}}` (the 71 rows);
   `effectOfToolCall(name, input, ctx)` over a corpus of raw inputs (valid / invalid / missing per computed
   tool, `run_custom` with and without a `customToolEffects` hit, an unknown plugin name with / without a
   `pluginToolEffects` entry); `maxEffectOfTool`; `highestToolEffect`; `isWithinCeiling` ×16;
   `lowerToolEffect` incl. `undefined`; `describeCeilingRefusal` for every (effect, ceiling) pair — exact
   strings; `usesAllowedUnder` for the four computed × four ceilings (exact note strings); and
   `applyEffectCeilingToTools` is private — reach it through `buildTools({effectCeiling})` in the re-aimed
   `tool-build` family instead. Seed `it`s: v4 `effects.test.ts` (4) + `tool-definitions-snapshot.test.ts`
   `:191-266`.
2. **`tool_executor_ceiling_equivalence` (tier 3 — v4's `executeToolCallWithContext` with the handlers
   mocked as `tool-executor.effect-ceiling.test.ts` does)** — the five v4 `it`s as rows: a write under `read`
   (refused, the exact result object `{toolName, success:false, result:{formattedText}, error}`), a deletion
   under `write`, a computed effect judged by input (`state fetch` ok / `state set` refused), `run_custom`
   ranked by the named custom tool, an undeclared plugin tool as write; plus the two log lines captured
   (INFO refused / DEBUG within). v5 side: `BuiltInToolRunner::run` with a recording fallback.
3. **`storyteller_pure_equivalence` (tier 1)** — oracle `cases/storyteller-pure.ts` over
   `@/lib/storyteller/{bid-parse,tiers,cards,invite,prompts,sse-tag}`: `parseBidReply` over ≥40 replies
   (the v4 `bids.test.ts` five + fenced / emphasised / reordered / `TOOL:` / `=` / en-dash separators /
   `BID: Speak!` / `pass — nothing to add` / synonyms `reply`, `look up`, `write`, `delete`, `action` / a
   TOOLS line `run_custom:coin, doc_read_file and state()` / empty / no BID / continuation plan lines / CRLF);
   `computeBidTier` over offers (the `bids.test.ts` six + a `destroy` claim with destroy offered);
   `cardFromOutcome` (fixed `answeredAt`); `inviteProbability` over a grid; `biddableSeats` + `chooseInvitees`
   with a scripted `random` sequence (the invite test's `0.5`-weight seats); `buildBidSection` /
   `buildPlanSection` / `describeDisposition` / `tierAllowance` over a grid (every byte); `tagSseChunk` over
   multi-frame chunks incl. a non-JSON line and a frame that already carries `participantId`. Fixed-clock,
   no DB. Seed `it`s: `bids.test.ts` (16), `prompts-and-tagging.test.ts` (7).
4. **`plan_storyteller_run_equivalence` (tier 2)** — v4's REAL `planStorytellerRun` + `runStorytellerRound`
   driven with `runTurn` canned (the `plan-run-order.test.ts` mocks, generalised): the three `7d7e38d2f` rows +
   the 400 / 404 / 409 refusals (`StorytellerRoundError` message + status) + the `profileKey` grouping + the
   `markRunning` / `markClosed` row deltas (`selection`, `bids: []`, `closedAt`) + the stopped / yielded
   endings (`isCurrentTurnStopped` injected). Needs a real two-DB fixture with a storyteller chat and a
   planted `storyteller_rounds` row (the orchestrator builder extended).
5. **`storyteller_rounds_tier2`** — `db/storyteller_rounds.rs` vs the v4 repository: `create` /
   `findActiveForChat` ordering (two open rounds — `createdAt` desc) / `findLatestForChat` / `recordBid`
   (replace vs append; a closed round drops the bid + the DEBUG) / `markRunning` / `markClosed` with and
   without `selection` / a corrupt `bids` JSON row (v4 `safeParse` drops the row). Plus `set_turn_mode`
   (`'rotation'` → NULL, the DEBUG, `updatedAt` stamped) and the patch-only refusal (a whole-row update
   carrying `turnMode` is REFUSED by v4's base repository — v5's `ChatUpdate` simply has no field; pin the
   absence with a source guard).
6. **`storyteller_round_tier3`** (the big one, in `orchestrator_tier3` or its own binary) — v4's REAL
   `handleSendMessage` on a storyteller chat (frames `storytellerRound` → `storytellerBid`×N → `roundOpen` →
   `chainComplete round_open`; the `storyteller_rounds` row after; the TWO `STORYTELLER_BID` `llm_logs` rows
   with `storytellerRoundId` and `messageId NULL`; NOTHING else written — the dry-scope proof: a
   `sqlite_master`-wide table-delta before/after must be ONLY `storyteller_rounds` + `llm_logs` (memory
   `tsx-oracle-cjs-stubs-and-per-op-processes` "table deltas")); an `@Name:` post (`user_turn`, no round);
   a whisper in storyteller mode (one reply, no round, bug 258 — P4.D348's `single_turn` must be on the
   union); a nudge (one turn, no ceiling, no chain); then `handleStorytellerRun` sequential (two speakers:
   `turnStart`/`turnComplete` depths 1,2, the plan section in each canned key — the second with `Since you
   planned it, A has spoken.`, `storytellerBid` on both rows, housekeeping enqueues ONCE after the round)
   and `simultaneous` (tagged frames; `concurrentFollower` → one Prospero post); `keepOrder`; a Destroy-alone
   refusal (400 before any stream); `storyteller-ask` on a kept-quiet seat (filter on, scripted random);
   `storyteller-lie`; leaving the mode through the PUT closes the round (`abandoned`); a new post supersedes
   an open round (`superseded`). The v4 oracle must splice `logger.transports` for the INFO/DEBUG lines
   (memory `v4-oracle-job-host-and-log-capture`) and hold the job host off.
7. **`chat_turn_mode_routes_equivalence` (tier 2, routes)** — PUT `{chat:{}, turnMode:'storyteller'}` /
   `'rotation'` / `null` (400 `Validation error`) / `'x'` (400); GET `?action=storyteller-round` (null /
   open / latest closed with `bids: []`); the three POSTs' 400/404/409 bodies and the `Invalid request`
   Zod arm; `storytellerInviteFilter` through the bag (non-boolean → `Invalid storytellerInviteFilter`?
   — measure v4: `updateChatSchema` is Zod, so a non-boolean is the whole-request 400); the GET wire keys.
   Home: `salon_mutations` / `salon_reads` oracles (`salon-mutations.test.ts`).

### 5.3 IMPORT-DEAD census for this area (confirmed at the pin)

- `cases/tool-build.test.ts:54` — DEAD (static import, module deleted). Re-aim in §5.1.
- No other `harness/oracle` file imports `destructive-tools`, `isComputedTool`, or anything `b15fe9404`
  renamed (`grep -rn "destructive-tools\|isComputedTool\|DESTRUCTIVE_TOOL_NAMES" harness/` → the one hit).
  The fixture builders `build-autonomous-rooms-web-fixture.ts`, `build-enclave-lifecycle-fixture.ts`,
  `build-enclave-step-fixture.ts` mention "destructive" only as the `runDestructiveToolsAllowed` /
  `destructiveToolPolicy` seeds (grep'd — no import of the module).
- `a8214b253` moved `resolveTurnSkipOffer` / `maybePauseForUserSeatTurn` etc. — the orchestrator oracle
  imports `handleSendMessage` only (alive). P4.D348's survey owns the rest of that commit's census.

### 5.4 The `toolChoice` wire proof

`request_builder_equivalence` records v4's REAL plugin bodies (P4.11's recorder, `mode`-tagged vectors). Add
per provider a `streamMessage` vector with `tools: [one tool]` + `toolChoice: 'none'`: expect
`tool_choice: "none"` on nanogpt / deepseek / z-ai / openai-compatible, `tool_choice: "auto"` on openrouter,
and NO key on anthropic (Anthropic's wire form would be `tool_choice: {type:'none'}` — v4 never sends it),
google (`toolConfig` absent), openai responses, ollama, grok. The v5 `StreamParams.tool_choice` must
therefore be mapped by the chat-completions family only; the other builders ignore the field.

### 5.5 Sweep / gate mechanics

Recipe headers for the NEW families go in each test's doc block (the sweep driver's `--self-test` rules,
memory `sweep-driver-recipe-header-rules`); lane-private `/tmp/<order>/` outputs (memory
`parallel-lane-shared-resources`); `QT_V4_CHECKOUT=<pin worktree>` on the workspace gate; the
`orchestrator_tier3` corpus additions re-mint ids per oracle run (mask minted ids). The storyteller rows
spend NO real model calls (canned); the dogfood rows do (§11).

### 5.6 Seed corpora v4 shipped (`it` names — transcribe case for case)

`bids.test.ts` 16, `prompts-and-tagging.test.ts` 7, `dry-scope.test.ts` 4 (NO-COUNTERPART unless R-1 picks
a wrapper), `tool-executor.effect-ceiling.test.ts` 5, `effects.test.ts` 4, `tool-definitions-snapshot.test.ts`
`:191-266` (5 + per-tool), `plan-run-order.test.ts` 3, `trigger-runner.test.ts` D9 2,
`storyteller-board-arrange.test.ts` 4 (SPA: `arrangeBids` — keeps round order / follows the arrangement /
appends a late bid / drops an arranged seat no longer in the round), `base-repository-patch-only.test.ts`
(+`turnMode`), `get.dispatch.test.ts` (+`'storyteller-round'` in the action list). Listed verbatim in §1.

## 6. SPA — v4 component → v5 Angular home; the wire contract; the specs

### 6.1 Component map (`apps/web/src/app/**`; the SPA is zoneless signals + TanStack Angular)

| v4 (React) | v5 home | today |
|---|---|---|
| `components/chat/StorytellerBoard.tsx` | **NEW** `chat/storyteller-board.ts` (+ `.spec.ts`) — standalone component: inputs `chatId`, `busy`, `avatarFor` (or the participants list), output `run` emitting `{roundId, participantIds, order, keepOrder}`; `injectQuery` on `chatKeys.storytellerRound(chatId)` → `core.dispatch({type:'chatStorytellerRound', chatId})`; `ask` / `lie` mutations as dispatches; the summary line / board template with the exact v4 strings (§1.1.12); `arrangeBids` / `withBid` as exported pure fns (`chat/storyteller-board.pure.ts`) | nothing |
| the `@dnd-kit` sortable | **R-4**: no `@angular/cdk` in `package.json`; options (a) add `@angular/cdk` (DragDrop — mouse/touch/keyboard-ish, a 100 KB+ dependency; a RULING), (b) a hand-rolled pointer-events sortable (mouse `distance: 8`, touch `delay: 250`, keyboard arrows) in `chat/storyteller-sortable.ts` — Tier 2 either way; Tier 1 ships the board WITHOUT drag (selection + the three buttons + `keepOrder:false`), Tier 2 adds arrangement | — |
| `ChatSidebar.tsx` `TurnModeControl` + the invite checkbox | `chat/sidebar/participants-section.ts:55-66` (the Pause button; `isPaused` input `:131`, `togglePause` output `:142`) → replace with the three-radio `qt-storyteller-mode` radiogroup + the checkbox; new inputs `turnMode`, `storytellerInviteFilter`; new outputs `setTurnPosition`, `toggleStorytellerInviteFilter`; `chat/sidebar/chat-sidebar.ts:156-162` (the COLLAPSED strip's pause icon — v4 kept it) `:247-256` pass-through, `:373`/`:438` | Pause button only |
| `useChatControls.ts setTurnPosition / toggleStorytellerInviteFilter` | `screens/salon/salon-conversation.ts` beside `setPauseState` `:3553` / `togglePause` `:3562` (the dispatch `{type:'chatUpdate', chatId, chat:{isPaused:false}, turnMode}` — contract W1 — then the toasts and the chat refetch; the filter through `chat:{storytellerInviteFilter}` with the optimistic rollback) | pause only |
| `useSSEStreaming` board frames + concurrent bubbles + `runStorytellerSelection` | `core/chat-stream.reducer.ts` (the frame fold: `storytellerRound` / `storytellerBid` / `roundOpen` → a new `storyteller: StorytellerStreamEvent | null` slot + seq; `concurrent && participantId` frames → `concurrentStreams: ConcurrentStream[]` folded per participant; `chainComplete.reason === 'round_open'` recognised — `chainReason` is already a string `:388`), `core/core-contract.ts:5150-5200` (the frame type: `concurrent?`, `storytellerRound?`, `storytellerBid?`, `roundOpen?`, `roundId?`), `screens/salon/salon-conversation.ts:3708 runTurn` (a sibling `runStorytellerSelection` that dispatches `chatStorytellerRun` and folds the same stream; the board's cache writes `setQueryData` on the three events — memory `tanstack-angular-data-lags-invalidate`: read `getQueryData`, not `data()`, when deciding on a bid patch) | `handAnswered` / `heldUserTurn` folds exist (`:316-324`, `:375-392`) as the pattern |
| `VirtualizedMessageList` `concurrentStreams` | `chat/message-list.ts` (+ input) rendering one `chat/streaming-message.ts` per entry after the ordinary streaming row | one streaming slot |
| `SalonView` board placement | `screens/salon/salon-conversation.ts:503` above `<qt-chat-composer`, gated `chat().turnMode === 'storyteller'` | — |
| `LLMInspectorPanel` filter + `LLMInspectorEntry` chip | `chat/llm-inspector-panel.ts:27-56` (`storyteller: ['STORYTELLER_BID']` between `chat` and `memory`, label `Storyteller Rounds`, the `storytellerRoundId` admission `:167`), `chat/llm-inspector-entry.ts:18-54` (badge class + label `Bid` + the round chip), `chat/llm-logs.api.ts:117` (`storytellerRoundId?: string | null` beside `autonomousRunId`), `screens/settings/system/llm-logs-card.ts` (label `Storyteller Bid`). ⚠ P4.D359 edits the Inspector for `eb2d87788`'s derived `other` list — same file, different lines (§7) | — |
| `WorkbenchLibrary` effect badge | `screens/custom-tools/workbench-library.ts:290-310` (between the trigger badge `:303-306` and the parameter badge `:308`), `core/core-contract.ts:7637-7656 CustomToolLibraryEntry` (+`effect: 'read' | 'write'` after `triggerCount`) — ⚠ P4.D346 edits the Workbench for the intervention fields; the library file vs the editor — confirm disjoint | — |
| `app/salon/[id]/types.ts Chat.turnMode / storytellerInviteFilter` | `core/core-contract.ts:3395-3430` the chat type (+ `turnMode: 'rotation' | 'storyteller'`, `storytellerInviteFilter: boolean`) | — |
| `_storyteller.css` | `apps/web/src/styles/qt-components/_storyteller.css` NEW + an `@import` in `_index.css:25-34`; **`scripts/check-qt-classes.mjs` fails the build on a template class with no CSS rule** (the "false lint claim" class) — every `qt-storyteller-*` the templates use needs its rule | 13 files |
| `lib/query/keys.ts storytellerRound` + `topic-map.ts` | `chat/chat-keys.ts:55` (+`storytellerRound: (chatId) => ['chat', chatId, 'storytellerRound']`), `core/realtime-topic-map.ts:151` (+ `case 'storytellerRounds'` → `id ? [chatKeys.storytellerRound(id)] : [chatKeys.all]`) + `core/realtime-topic-map.spec.ts:136` ("last topic is `inventory`" → `storytellerRounds`) | — |

### 6.2 The wire contract between P4.D343 (builds) and P4.D344 (consumes) — **contract W1** (BINDING names)

`crates/quilltap-core/src/api/types.rs` + `apps/web/src/app/core/core-contract.ts`:

- `Request::ChatUpdate { …, turn_mode: Option<Option<Value>> }` (`double_option`, serde key `turnMode`,
  a sibling of `chat`; absent → untouched; `"rotation"` / `"storyteller"` → `set_turn_mode` (+ the
  abandon on leaving); `null` or anything else → `BadRequest "Validation error"` with nothing written) ↔
  `ChatUpdateRequest.turnMode?: 'rotation' | 'storyteller'`. The bag key `chat.storytellerInviteFilter:
  boolean` (non-boolean → the whole PUT refused).
- `Request::ChatStorytellerRound { chat_id }` → `Response::ChatStorytellerRound(Value)` = `{ "round":
  StorytellerRound | null }` (v4's `successResponse` body; the SPA reads `.round`).
- `Request::ChatStorytellerRun { chat_id, round_id: String, participant_ids: Vec<String>, order:
  Option<String>, keep_order: Option<bool> }` (keys `roundId`, `participantIds`, `order`, `keepOrder`;
  `order` defaults `"sequential"`; a non-enum `order` → `BadRequest "Invalid request"` (v4's ZodError arm))
  → `Response::ChatStorytellerRun(ChatSendResultDto-like)`: the frames ride the `Event` channel
  chat-scoped exactly like `ChatSend`; the dispatch reply is `{ "ok": true }`-class (v4's reply is the SSE
  stream itself — v5's `ChatSend` precedent returns a typed DTO after the stream; mirror: `{ ended:
  "ran"|"stopped"|"yielded" }` is EXTRA information v4 never answers — keep the reply EMPTY (`Response::Ok`
  / `serde_json::Value::Null`) to stay byte-faithful on the web edge, which answers `sseStreamResponse`).
  Refusals: `Conflict` 409 `That round is no longer open`; `NotFound` 404 `That seat is not on this round` /
  `Chat not found`; `BadRequest` 400 `Choose at least one bid, or let the round lie` / `Only a seat that bid
  to speak can be chosen` / `A Destroy bid must be chosen on its own`. (`ErrorKind` has `Conflict`? — if
  not, carry the status in `details` and let the web edge restore 409, the `chat_media` 502 precedent.)
- `Request::ChatStorytellerAsk { chat_id, round_id, participant_id }` → `Response::ChatStorytellerAsk
  (Value)` = `{ "bid": StorytellerBid }` (the ANSWERED card — the dispatch awaits the bid call; the SPA's
  `onSettled` invalidates regardless).
- `Request::ChatStorytellerLie { chat_id, round_id }` → `{ "closed": true }`.
- Frames on `EventPayload::Chat(ChatEvent)`: `{"storytellerRound": StorytellerRound}`,
  `{"storytellerBid": {"roundId": string, "bid": StorytellerBid}}`, `{"roundOpen": true, "roundId": string}`,
  `chainComplete` with `reason: "round_open"`; concurrent turns' frames carry `"participantId"` (when the
  frame lacked one) and `"concurrent": true` as the LAST two keys.
- The chat GET wire (`chatGet` → the `Chat` type): `turnMode: 'rotation' | 'storyteller'` (never null on
  the wire), `storytellerInviteFilter: boolean`, both after `isPaused`.
- Realtime: topic `"storytellerRounds"` scoped by chat id → the SPA invalidates `chatKeys.storytellerRound`.
- `LlmLogDto.storytellerRoundId?: string | null`; log type `"STORYTELLER_BID"`.
- `CustomToolLibraryEntry.effect: 'read' | 'write'` after `triggerCount`.
- TS types `StorytellerBid`, `StorytellerRound`, `StorytellerSelection`, `StorytellerBidRecord`,
  `StorytellerTier`, `StorytellerOrder`, `TIER_LABELS` in `core/core-contract.ts` (v4 `storyteller.types.ts`
  field for field, optional/nullable preserved).

P4.D344 cuts from `main` (wave 0) and codes to this contract with `it.skipIf(SERVER_LANDED)` gates on the
live beats (memory `react-closure-reads-in-signal-ports`, the stand-in rule); the unifier flips them.

### 6.3 Specs to add

- **Component harness (vitest):** `chat/storyteller-board.spec.ts` (render null / summary variants ×5 /
  open board; toggle rules incl. the destroy exclusivity; Select all excludes destroy; `Run in order`
  disabled states; `All at once` below 2; `Ask anyway` dispatch + error toast; `Let it lie`; the hint
  sentences; the order badges + Reset once arranged — the `arrangeBids` four cases from v4 verbatim; the
  `withBid` patch), `chat/sidebar/participants-section.spec.ts` (the radiogroup: current position for the
  four `(isPaused, turnMode)` combos, titles, clicking the active one is a no-op, the checkbox renders only
  in storyteller mode), `screens/salon/salon-turn-controls.spec.ts` (`setTurnPosition` dispatch bodies +
  toasts; the filter's optimistic rollback), `core/chat-stream.reducer.spec.ts` (the three board frames; a
  concurrent frame set → one bubble per participant, `done` pushes the message with the v4 fields, skipped /
  stopped push nothing; `round_open` leaves `waitingForResponse` false), `chat/llm-inspector-panel.spec.ts`
  (the filter admits a `CHAT_MESSAGE` row with `storytellerRoundId`), `chat/llm-inspector-entry.spec.ts`
  (chip text `round abcdef`), `screens/custom-tools/workbench-library.spec.ts` (badge class + title per
  effect), `core/realtime-topic-map.spec.ts` (the ninth topic). Harness gotchas: `coreStreamStub()` +
  `provideTanStackQuery` for any component injecting `RealtimeService` / `injectQuery` (memory
  `spa-component-harness-gotchas`); `retry: false`.
- **Playwright (`apps/web/e2e/`):** NEW `salon-storyteller-flow.spec.ts` (own-instance on the salon pair,
  memory `own-instance-e2e-fixture-choice`): switch Group Expedition to Storyteller (the radiogroup; the
  sidebar's PUT), post → the board appears with the pending cards → bids land (the mock LLM must answer a
  BID-formatted reply: `startMockLlm(reply, port)` is a FIXED reply today (`support/mock-llm.ts:57-62`) — a
  reply of `BID: speak\nTIER: simple\nTOOLS: none\nPLAN: I shall put the kettle on.` works for BOTH the bids
  and the speaking turn (the turn then "says" those four lines — acceptable for an e2e that asserts the
  board, the badges and the run frames; or extend the mock with a `replyFor(requestBody)` option that keys
  on the bid header's presence, a small change in `support/mock-llm.ts` owned by this lane), tick two →
  `Run in order` → two replies → the summary line `Last round: A, B`; `Let it lie`; Normal → the board
  folds. A second describe for the Inspector filter. The spec is GATED `SERVER_LANDED` in P4.D344 and flipped
  at unification; keep the describes non-serial.

## 7. The lane split, by FILE OWNERSHIP

The planner's two lanes stand — **P4.D343** (server, wave 1 on `KEYSTONE`) and **P4.D344** (SPA, wave 0
from `main`) — with ONE amendment: P4.D343 is large (two features) and should be ordered as **two internal
units committed in order** — unit E (effects + the ceiling + the `tool-build` re-aim; no storyteller code,
no spine edit beyond the ceiling plumbing) and unit S (Storyteller) — so the unifier can land E even if S
slips. Whether to cut S into its own lane id is the planner's call; file ownership below is the same either
way.

### 7.1 P4.D343 — files it CREATES (nobody else may touch)

`crates/quilltap-core/src/tools/effects.rs`; `crates/quilltap-core/src/pascal/tool_effect.rs`;
`crates/quilltap-core/src/storyteller/{mod,types,tiers,invite,cards,prompts,bid_parse,sse_tag,bid_call}.rs`;
`crates/quilltap-core/src/services/storyteller_rounds.rs`; `crates/quilltap-core/src/db/storyteller_rounds.rs`;
`crates/quilltap-core/src/api/chat_storyteller.rs`; `crates/quilltap-host/src/storyteller_spine.rs`;
`crates/quilltap-harness/tests/{tool_effects_equivalence,tool_executor_ceiling_equivalence,
storyteller_pure_equivalence,plan_storyteller_run_equivalence,storyteller_rounds_tier2,
storyteller_round_tier3,chat_turn_mode_routes_equivalence}.rs`; `harness/oracle/cases/{tool-effects,
storyteller-pure,storyteller-rounds,plan-storyteller-run}.ts` (+ a `storyteller-round-tier3.test.ts` if not
folded into the orchestrator case); `harness/oracle/fixtures/build-storyteller-fixture.ts` if the
orchestrator builder is not extended.

### 7.2 P4.D343 — files it EDITS, and who else edits them

| file | P4.D343's region | other lane(s) + their region | meeting point |
|---|---|---|---|
| `crates/quilltap-core/src/services/tool_build.rs` | `BuildToolsInput` (+`effect_ceiling`), `BuiltTools` (+2 maps), the ceiling application at the END of `build_tools`, `resolve_turn_effect_ceiling` REPLACING `:749-810` (`DESTRUCTIVE_TOOL_NAMES`, `destructive_allowed`, `filter_destructive_tools`, `apply_autonomous_destructive_filter`) | none | — |
| `crates/quilltap-core/src/tools/executor.rs` | `BuiltInToolRunner::run` `:2255-2271` — insert the ceiling check after the mountPool refusal | P4.D350 (Inventory chokepoints) edits inventory HANDLERS in this file, not `run` | region-disjoint; the unifier merges |
| `crates/quilltap-core/src/services/tool_execution.rs` | `ToolExecutionContext` +3 fields `:257-316`; `create_tool_context` unchanged | none known | — |
| `crates/quilltap-core/src/pascal/trigger_runner.rs` | `RunTriggeredCustomToolsParams` +`effect_ceiling`; the skip BEFORE `run_custom_tool` inside `run_triggered` | **P4.D345** (`839575d89` `mayHold:true` on the trigger entrance — a field on the `run_custom_tool` call) | **contract T1**: P4.D343 adds the `if let Some(ceiling) = params.effect_ceiling { … continue }` block as the FIRST statement of the per-hit loop body; P4.D345 touches only the `run_custom_tool(…)` argument list below it. Both must leave the loop header untouched. |
| `crates/quilltap-core/src/services/trigger_phrase_sites.rs` | `ReplyTriggerParams` +`effect_ceiling`, the pass-through `:116-133` | P4.D345? (no — the user-message site carries no hold field) | — |
| `crates/quilltap-core/src/pascal/workbench.rs` | `CustomToolLibraryEntry` +`effect` `:50-69`, the builder `:399` | **P4.D345** adds `intervention` fields to the SAME struct (v4 `workbench.ts:49 intervention: boolean` is `839575d89`'s) | **contract T2**: field ORDER in the struct = v4's `workbench.ts:31-60` order: `… llm, triggerCount, intervention (P4.D345), effect (P4.D343), parameterCount …`. Each lane adds its own line; the unifier orders them. The serialised key order is pinned by `pascal_workbench_equivalence` on the union. |
| `crates/quilltap-core/src/model/stream.rs`, `model/streaming_provider.rs:200`, `model/request_builder/chat_completions.rs` (`:669-678`, `:832-836`, `:908-914`, the nanogpt arm) | `StreamParams.tool_choice` + the mapping | **P4.D356** (bug 269 NanoGPT image sizes — the nanogpt builder's IMAGE arm, `chat_completions.rs:47-67, :207-213`) | region-disjoint in `chat_completions.rs`; both add vectors to `request-envelopes.recorded.ndjson` — **append-only, distinct `mode`/`name` tags**, re-record from the pin once at unification |
| `crates/quilltap-core/src/services/llm_logging.rs` | `LogContext.storyteller_round_id` + the stamp `:315`; `log_type::STORYTELLER_BID` `:435` | P4.D348 (`eb2d87788` logs once per call via `record()` — `primary_stream.rs`, maybe `llm_logging.rs`'s params) | field-level: P4.D343 adds ONE struct field + ONE constant + ONE line in the row build; P4.D348 owns the call-site changes |
| `crates/quilltap-core/src/db/llm_logs.rs` | NOTHING (the keystone adds the column to the row structs / `LOG_COLUMNS` / the insert) — contract K | **P4.D341** | K |
| `crates/quilltap-core/src/db/chats.rs` | `set_turn_mode` beside `set_concierge_mode` `:1818` (a new fn; `ChatUpdate` `:617` untouched) | **P4.D341** adds the two columns to the chat row struct / writers; P4.D349 (vault keys) does not touch chats | new fn only — append after `set_concierge_mode`'s block; K names the row fields |
| `crates/quilltap-core/src/db/chats_read.rs`, `chats_messages_read.rs`, `chats_messages.rs` | NOTHING — the keystone splices `turnMode` (idx 19), `storytellerInviteFilter` (20) + marshals, appends `storytellerBid` (46) + `PascalMeta`-style typed field, and names the column in the INSERT list | **P4.D341** | K |
| `crates/quilltap-core/src/services/orchestrator.rs` | see 7.4 | **P4.D348** (bug 258 + `a8214b253`'s shifts; `maybe_pause_for_user_seat_turn` `:872-1050`; the hold INFO rename `:1196-1202`; `execute_turn_chain` `:4836-5109`), **P4.D347** (nothing in this file — `build_context_input`'s args only if the referent ledger needs a new input; confirm), **P4.D345** (the consult/hold wiring reaches `OrchestratorDeps.consult` — a field, not a region) | 7.4 |
| `crates/quilltap-host/src/spine.rs` | `run_send` `:1674-1683` (after `initial`, before the DTO): the `storyteller_hold` branch; `:1736 single_turn` (the `storyteller_single_turn` disjunct); the `ChatSpine` gains a `run_storyteller_run` / `run_storyteller_bid` entry that `storyteller_spine.rs` implements (an `impl ChatSpine` block in the NEW file keeps this file's edit to two hunks) | **P4.D348** (bug 258: `is_whisper` → `waitForEarlierTurns` BEFORE `process_message_with_ledger` `:1636-1642` and the `single_turn` disjunct `:1736`) | **contract S2** |
| `crates/quilltap-core/src/services/message_finalizer.rs` | `FinalizeOptions.storyteller`; the `storytellerBid` write after the assistant save; `:1357-1372` (+`effect_ceiling`); `:1520-1547` the deferral; the new `run_storyteller_round_housekeeping` | P4.D348 (`a8214b253` moved v4's finalizer bits? — the ledger names `turn-outcome.service.ts` for settle/yield; v5's finalizer carries the yield record — P4.D348 may touch `:1340-1360`'s neighbourhood) | fenced hunks (`// === P4.D343 ===`), each a self-contained `if let Some(st) = &opts.storyteller` block; the unifier merges by fence |
| `crates/quilltap-core/src/services/regenerate_swipe.rs` | `BuildContextArgs` +`storyteller_section` (`:642+`), the swipe row copy (`:1060-1064`) | **P4.D348** (`eb2d87788`: the swipe streams through the logged funnel — the STREAM call region, `regenerate_swipe_stream.rs`?) | region-disjoint; `storyteller_section` is one arg + one `insert` |
| `crates/quilltap-core/src/services/build_context.rs` | NOTHING directly — **contract C1** (P4.D347 adds the slot) | **P4.D347** (`bdd09b75b`: the trailing list `:3118-3127` gains referentLedger / closingEcho; the budget rework) | C1 |
| `crates/quilltap-core/src/api/types.rs` | `Request::ChatUpdate.turn_mode`; four new `Request` + `Response` variants; the `StorytellerRound`/`Bid` DTO types | **FROZEN for stage-2 lanes in a keystone round** (memory `frozen-engine-seams-and-web-censuses`) — the keystone owns `types.rs` / `engine.rs`?; P4.D345 / P4.D347 / P4.D354 / P4.D355 also need variants | the round's standing rule applies: either the keystone pre-declares every lane's variants (planner collects them — list in W1) or each lane adds an append-only fenced block in the Chats family region + its own engine arms, and the unifier merges fences. **Recommend the fenced-block rule** (variants are additive; the `dispatch_wrong_type_census` / action-list guards must be re-measured LAST) |
| `crates/quilltap-core/src/api/engine.rs` | four dispatch arms (+ `turn_mode` in the `ChatUpdate` arm `:2008-2030`) | as above | fenced |
| `crates/quilltap-core/src/api/salon.rs` | `chat_update` `:1597` (+`turn_mode` param, the setter + abandon + INFO; `storytellerInviteFilter` in the boolean list `:2039-2049`), `assemble_chat_get` `:424` (+2 keys) | none in the span (P4.D356's `3b8b6f42b` is `settings.rs`) | — |
| `crates/quilltap-core/src/services/delete_all.rs` | `FORMAT3_MAIN_TABLES` + the test | **P4.D355** (`cc5f2347c` the Log in backups / delete-all — `log_entries` in the SAME list / the llm-logs partition list) | both APPEND one row; v4's order: `…, chat_wardrobe_history, storyteller_rounds` in `clearFormat3Entities` — check where `cc5f2347c` put `log_entries` (a different partition list, most likely); the unifier keeps v4's order |
| `crates/quilltap-core/src/realtime/types.rs` | `StorytellerRounds` LAST + the test | P4.D345 adds `customToolInterventions` (`839575d89`, 2026-10-22 — SAME DAY, which came first in v4's `REALTIME_TOPICS`? `b15fe9404` dev.58 precedes `839575d89` dev.60 → `storytellerRounds` is NINTH, `customToolInterventions` TENTH) | **contract R1**: P4.D343 appends `storytellerRounds` at index 8, P4.D345 appends `customToolInterventions` at index 9; the order test on the union lists ten |
| `crates/quilltap-web/src/wardrobe_routes.rs` | one GET arm + `CHAT_GET_ACTIONS` (18th key, after `informs`), three POST arms + `CHAT_POST_ACTIONS` (trailing), the order test `:1301` | P4.D345 (`?action=interventions` lives under `/chats/[id]/custom-tools`, a DIFFERENT router — confirm), P4.D347 (`absent-mentions` GET — `d68c1af88` — on the chat GET list?) | the action lists are append-in-v4-order; each lane adds its own literals; the unifier re-runs the order test |
| `crates/quilltap-harness/tests/tool_build_equivalence.rs` + `harness/oracle/cases/tool-build.test.ts` + `fixtures/tool-build.json` + `build-tool-build-fixture.ts` | the re-aim (§5.1) | none | — |
| `orchestrator_tier3_equivalence.rs` + `cases/orchestrator-tier3.test.ts` + `fixtures/build-orchestrator-fixture.ts` + `orchestrator-tier3.json` | NEW storyteller rows + the `toolChoice` key in the canned mock | **P4.D348** (the frame-order shifts, bug 258 rows), **P4.D347** (context bytes shift the canned keys for EVERY row) | **the corpus is a shared fixture**: each lane adds rows under its own name prefix (`storyteller_*`, `whisper_*`, …) and re-records the oracle from the pin; the canned keys are re-minted per regen; land P4.D347 FIRST at unification (it moves every key), then P4.D348, then P4.D343's rows |
| `message_finalizer_tier3_equivalence.rs` / `regenerate_swipe_tier3_equivalence.rs` + cases | new rows | P4.D348 | same rule |
| `realtime_topics_equivalence.rs` / `realtime_publish_sites_guard.rs` | the topic + six sites | P4.D345 (its topic + sites) | R1 |
| `pascal_workbench_equivalence.rs` + route | the `effect` key | P4.D345 (`intervention`) | T2 |
| `salon_reads_equivalence.rs` / `salon_mutations` | the wire keys + PUT rows | none | — |
| `docs/developer/porting/status-log.md`, `phase-4.md` | the lane record | every lane (append-only) | — |

### 7.3 P4.D344 — files (SPA)

CREATES: `apps/web/src/app/chat/storyteller-board.ts` + `.spec.ts` (+ `storyteller-board.pure.ts`,
`storyteller-sortable.ts` if R-4 picks hand-rolled), `apps/web/src/styles/qt-components/_storyteller.css`,
`apps/web/e2e/salon-storyteller-flow.spec.ts`. EDITS: `core/core-contract.ts` (the W1 types — **shared with
every SPA lane**; append-only fenced blocks, the standing rule), `core/chat-stream.reducer.ts` (+ spec),
`chat/sidebar/participants-section.ts` (**P4.D358** `eed72d60c` adds accessible names to its controls — the
Pause button P4.D344 REPLACES; contract: P4.D358 skips the Pause button (it goes away) or the unifier takes
P4.D344's radiogroup which already carries `aria-label="Turn mode"` / `role="radio"` / `aria-checked`),
`chat/sidebar/chat-sidebar.ts` (pass-through inputs; P4.D358 may touch its `aria-label`s `:159` — disjoint
lines), `screens/salon/salon-conversation.ts` (**P4.D352** `2b2b190b6` prose mode + `778b97b2b`,
**P4.D359** `9a88f862f` name chips, **P4.D358** `10e09b7c3` — four lanes in one 3,700-line file: each adds a
fenced region; P4.D344's are (a) the board element above `<qt-chat-composer` `:503`, (b) `setTurnPosition` /
`toggleStorytellerInviteFilter` beside `setPauseState` `:3553`, (c) `runStorytellerSelection` beside
`runTurn` `:3708`, (d) the three sidebar bindings at `:389-404` / `:492`), `chat/message-list.ts` (+ input),
`chat/llm-inspector-panel.ts` + `chat/llm-inspector-entry.ts` + `chat/llm-logs.api.ts` (**P4.D359** owns
`eb2d87788`'s derived `other` list in the SAME `FILTER_GROUPS` constant — contract I1: P4.D344 adds the
`storyteller` entry + label + the `storytellerRoundId` admission; P4.D359 replaces the `other` array by the
derived filter; the union keeps both), `screens/custom-tools/workbench-library.ts` (+ spec; **P4.D346**
owns the Workbench EDITOR for intervention — `WorkbenchLibrary.tsx` was touched by `839575d89` too (an
`intervention` badge?) → contract T2 on the SPA side: both add a badge; v4's order in the badge row is
`llm, triggers, intervention?, effect, params` — measure `WorkbenchLibrary.tsx` at the pin `:240-270`),
`screens/settings/system/llm-logs-card.ts`, `chat/chat-keys.ts`, `core/realtime-topic-map.ts` (+ spec;
P4.D346 adds `customToolInterventions` — R1's SPA half), `apps/web/src/styles/qt-components/_index.css`
(one `@import` — **P4.D357** / P4.D346 (`839575d89` added an import too) edit this file: append-only lines).

### 7.4 `orchestrator.rs` — function-level ownership between P4.D343 and P4.D348 (+ C1 for P4.D347)

`process_message_with_ledger` is one 3,100-line fn (`:1101-4247`). Both lanes edit it. Proposed regions
(by the existing banners, which both lanes must keep verbatim as anchors):

| region (banner) | lines today | owner | the other lane's contract |
|---|---|---|---|
| `SendMessageOptions` / `ProcessMessageResult` structs | `:399-437`, `message_finalizer.rs:572-594` | **P4.D343 adds** `storyteller_bid: Option<StorytellerBidRequest>`, `storyteller: Option<StorytellerTurnContext>`; `storyteller_hold: Option<StorytellerHold>`, `bid: Option<StorytellerBidCallOutcome>` (all `Default::default()`-able) | P4.D348 adds nothing to the structs (bug 258's `single_turn` already exists) — **contract S1** |
| the hold read `Paused chat — hold the user's turn` | `:1171-1203` | **P4.D348** (the `[Orchestrator] Recording the operator post without a reply` merge, `resolveTurnHold`'s shape) | P4.D343 needs `hold == Storyteller` computed HERE from `chat.turnMode` + `!continue_mode && !is_operator_whisper_send && !never_pause_for_user` → **S1 asks P4.D348 to build the `TurnHold` enum (`Paused | Storyteller | None`) with the storyteller arm INCLUDED** (it is three conjuncts reading a column the keystone provides) and to log `hold:"storyteller"`; P4.D343 consumes `hold` |
| fair-rotation guard | `:1225-1316` | P4.D348 (`qualifiesForFairRotationCheck` takes `hold`) | S1: `hold.is_some()` disables it |
| the hold return `Paused chat — the user's turn is recorded` | `:2085-2110` | **P4.D348** builds `finish_held_user_turn(hold)` returning the paused frame OR the storyteller `ProcessMessageResult { storyteller_hold: Some(StorytellerHold { trigger_message_id, carina: parse_carina_query(content).is_some() }) , is_paused: false, … }` + the `lastTurnParticipantId: null` write for both | P4.D343 consumes the field in the host |
| `turn_skip` resolve + raised hand | `:1711-1775` | **P4.D343** adds the `is_storyteller_turn` / `is_bid` arms (`resolveTurnSkipOffer`) and `!is_bid` on the hand take | P4.D348 leaves these lines (its bug-262 work is in `maybe_pause_for_user_seat_turn`) |
| Prospero cadence gate | `:1777-1841` | **P4.D343** adds `concurrent_follower` + the `!hold && !is_bid` skip | — |
| tool build gate + slate | `:2123-2491` | **P4.D343** (the ceiling into `build_tools`, the two maps kept on the slate, `:2348-2370` REPLACED by `resolve_turn_effect_ceiling`) | P4.D348 does not touch |
| build context args | `:2641-2724` | **P4.D343** adds `turn_skip: if storyteller.is_some() { None } else { turn_skip.clone() }`, `purpose`, `storyteller_section` (the section built just above from the slate) — **through C1's input fields** | P4.D347 adds its own inputs (referent ledger) — adjacent arg lines, fenced |
| Courier short-circuit + the bid return | `:2907-2943` | **P4.D343** inserts the bid refusal inside the courier branch and the bid short-circuit right AFTER it (before `:2944` the fallback frame) | — |
| primary stream / loops / drains / stop | `:3027-3799` | **P4.D348** (`eb2d87788` logging) | P4.D343 none |
| sentinel handling | `:3801-3849` | **P4.D343** keeps `turn_skip` feeding the sentinel (no change) — nothing to edit; record why | — |
| terminal branches / finalizer call | `:3851-4247` | **P4.D343** adds `storyteller: options.storyteller.clone()` to `FinalizeOptions` | P4.D348 (`a8214b253` `encodeTurnDoneEvent`) edits the `done` frame emits in the same block — fenced, different statements |
| `execute_turn_chain` | `:4836-5109` | **P4.D348** | P4.D343 passes `single_turn` from the host (S2); no change here |
| `chain_reason_str` / `ChainReason` (`turn_orchestrator.rs:107-131`) | — | P4.D343 adds `RoundOpen => "round_open"` (emitted ONLY by the host's round opener — `ChainCompletePayload.reason` is a String, so the host can emit the literal without touching the enum; prefer the literal, no enum change) | — |

**Contract S1 (P4.D348 builds, P4.D343 consumes):** in `orchestrator.rs` — `pub enum TurnHold { Paused,
Storyteller }`, `fn resolve_turn_hold(chat: &Value, options: &SendMessageOptions) -> Option<TurnHold>`
(v4 `turn-holds.service.ts:59-78`, incl. `is_operator_whisper_send(options) = !continue_mode &&
target_participant_ids.as_ref().map_or(false, |t| !t.is_empty())` from bug 258), `finish_held_user_turn(…,
hold: TurnHold, content: &str, …)` returning the storyteller result shape above, and
`ProcessMessageResult.storyteller_hold: Option<StorytellerHold>` with `pub struct StorytellerHold {
pub trigger_message_id: Option<String>, pub carina: bool }` declared in `message_finalizer.rs`. The INFO
line `[Orchestrator] Recording the operator post without a reply` with `hold = "paused" | "storyteller"`.
If P4.D348 lands first, P4.D343 only consumes; if P4.D343 lands first, it ships S1 itself with a
`// === S1 (P4.D348 owns the rename) ===` fence and P4.D348 rebases.

**Contract S2 (`spine.rs::run_send`):** P4.D348 computes `let is_whisper = is_operator_whisper_send(&req)`
and awaits `turn_control::wait_for_earlier_turns(&chat_id)` BEFORE `process_message_with_ledger`; P4.D343
inserts, after `initial` and before the `dto` (`:1674`): `if let Some(hold) = initial.storyteller_hold.take()
{ return self.open_round_on_send(chat_id, hold, …).await; }` and computes `let storyteller_single_turn =
req.continue_mode && chat_turn_mode(&db, &chat_id) == Storyteller` (ONE fresh read, as v4 `:184`); the
`ExecuteTurnChainOptions.single_turn` line becomes `single_turn: is_whisper || storyteller_single_turn`
(P4.D348 writes `is_whisper ||`, P4.D343 appends `|| storyteller_single_turn`; the unifier joins the two
one-token hunks). The round opener / bid runner / run endpoint live in `storyteller_spine.rs` (P4.D343 only).

**Contract C1 (P4.D347 builds, P4.D343 consumes):** `build_context.rs` `BuildContextInput` gains `pub purpose:
ContextPurpose` (`#[derive(Default)] enum ContextPurpose { #[default] Turn, Bid }`) and `pub storyteller_section:
Option<String>`; the trailing list `:3118-3127` gains `storyteller_section.clone().unwrap_or_default()` LAST
(after P4.D347's `turn_yield_instruction`, after its own new `referent_ledger` / `closing_echo` which go
between `inform_block` and `turn_skip_instruction` — v4 `turn-trailing.ts:56-70` order); `deliver_recall`
(`:3237`) is skipped when `purpose == Bid` (`recall_delivery = None`). `build_context_input(BuildContextArgs)`
(`orchestrator.rs:5223`) threads two new args. P4.D347 ships the fields with their defaults (a no-op for every
caller) and the Bid skip; P4.D343 fills them. If P4.D343 lands first it ships C1 under a fence.

**Contract K (P4.D341 builds, P4.D343 consumes):** `chats_read::find_by_id` emits `turnMode` (string or
absent/null — the keystone picks v4's `nullable().optional()` rendering: NULL → key absent? v4's hydrate emits
`turnMode: null`? — measure `chats-read.ts`'s "nullable-optionals dropped" rule: ABSENT) and
`storytellerInviteFilter: bool` (default false); `chat_messages` rows carry `storytellerBid: Option<Value>`
through `chats_messages_read` (absent when NULL) and `ChatEventInput` / the INSERT list accept the key;
`llm_logs.rs` row structs + `LOG_COLUMNS` + the insert carry `storyteller_round_id: Option<String>` (the
`omit_columns_the_table_lacks` idiom extended so a pre-column `*-llmlogs.db` fixture still inserts);
`storyteller_rounds` exists on every instance after boot (the ensure + FK + `idx_storyteller_rounds_chat`
via `migration_indexes.json`); the committed `salon-{main,mount}.db` pair widened. P4.D343 writes `set_turn_mode`
(its own fn) and the two wire keys.

### 7.5 Stacking

P4.D343 branches from `KEYSTONE` (it compiles against K's row fields and reads `storyteller_rounds`).
P4.D344 cuts from `main`. P4.D347 / P4.D348 are wave 0 per the table; P4.D343 therefore cannot branch from
their tips — it ships S1 / C1 under fences when it reaches them first, and the unifier lands P4.D347 →
P4.D348 → P4.D343 (the context bytes move the canned keys; the hold rename moves a captured line; then the
storyteller rows). Recommend the planner either (a) move P4.D347 / P4.D348 to wave 0 on `KEYSTONE` too so
P4.D343 can stack on P4.D348's tip (cleanest — S1 and S2 stop being fences), or (b) keep the fence rule.

**Two measured corrections to 7.3 / 7.4:** (1) at the pin `WorkbenchLibrary.tsx:240-269`'s badge row is
`rollForm → llm → triggers → effect → params …` with NO intervention badge — `839575d89`'s hunk to that
file is something other than a badge (P4.D346's survey characterises it); T2's SPA half is therefore only
"P4.D344 inserts the effect badge between triggers and params". (2) K's `turnMode` read: a SQLite NULL
reaches Zod as `null`, and `.nullable().optional()` KEEPS `null` (only `undefined` is dropped) — so
`chats_read::find_by_id` emits `turnMode: null` on a NULL cell (the "nullable-optionals dropped" rule in
`chats-read.ts:12` is about participants parsed from JSON); the GET wire's `?? 'rotation'` is this lane's.
Measure both sides in `chats_read_equivalence` before pinning.

## 8. Tiering

### P4.D343 (server)

- **Tier 1 — must land:** the effects table + resolver + ceiling helpers (`tools/effects.rs`) with
  `tool_effects_equivalence`; the executor check with its two lines and the exact refusal text
  (`tool_executor_ceiling_equivalence`); `BuildToolsInput.effect_ceiling` + the two maps + the list filter
  with the computed notes; `resolve_turn_effect_ceiling` replacing the P4.D234 filter (the 3→5 widening, the
  retired INFO, the re-aimed `tool_build` family green from the pin); the trigger-runner ceiling (D9 rows);
  `StreamParams.tool_choice` + the per-provider mapping with recorded vectors; `custom_tool_effect` + the
  Workbench `effect` key; `log_type::STORYTELLER_BID` + `LogContext.storyteller_round_id` + the stamp;
  `set_turn_mode` + the PUT sibling + the abandon-on-leave + the two GET wire keys + `storytellerInviteFilter`
  in the bag; `db/storyteller_rounds.rs` + `storyteller_rounds_tier2`; the pure storyteller modules with
  `storyteller_pure_equivalence` (every prompt byte); `plan_storyteller_run` + `run_storyteller_round` with
  `plan_storyteller_run_equivalence`; the four API verbs + web arms + action lists; the realtime topic + six
  publish sites; the `delete_all` row; the host round opener / bid runner / run endpoint with
  `storyteller_round_tier3` covering: open (frames, the two bid rows, the table-delta dry proof), `@Name:`,
  nudge single turn, sequential run (plan bytes, `storytellerBid` written, housekeeping once), the
  refusals, ask / lie / supersede / abandon-on-leave. The finalizer's storyteller arm (bid write, trigger
  ceiling, deferral). The swipe's reroll section + column copy.
- **Tier 2 — should land:** `simultaneous` (the tagging sink, per-profile cap 4, `concurrentFollower` on the
  Prospero cadence) — a real concurrency feature in a single-writer runtime: the four concurrent
  `process_message_with_ledger` calls share one `Db` (fine — the writer serialises) but each needs its own
  `OrchestratorDeps` set (the spine builds one per send today — the run endpoint builds N); the board
  timeout as a `tokio::time::timeout` in the host (R-3); `keepOrder`; the Courier bid refusal; the
  `[TurnTools] Tool slate resolved` DEBUG if P4.D348 lands it.
- **Tier 3 — explicit typed refusals:** plugin tool effects (`pluginToolEffects` always empty — v5 has no
  plugin tools; a `NO-COUNTERPART` note, no refusal needed); the dry scope as a REPOSITORY proxy (R-1 picks
  the mechanism; whatever is not covered is a recorded divergence — never a silent write); if
  `simultaneous` slips to a later round, `order: "simultaneous"` answers `Response::error(ErrorKind::
  Unprocessable, "not_available: storyteller simultaneous")`-class loud refusal and the SPA's `All at once`
  button stays enabled only when the server advertises it (a capability key is NOT v4 — prefer landing it).

### P4.D344 (SPA)

- **Tier 1:** the board (no drag), the turn-mode radiogroup + invite checkbox, `setTurnPosition` /
  `toggleStorytellerInviteFilter`, the three frames in the reducer + the board cache writes,
  `runStorytellerSelection` (sequential), the Inspector filter + chip + label, the Workbench badge, the
  realtime topic map, the types, the CSS, the component specs, the gated e2e.
- **Tier 2:** drag-to-arrange + `keepOrder` + order badges + Reset (R-4), the concurrent bubbles.
- **Tier 3:** none (a missing drag library is a visible absence, not a stub — the hint sentence under the
  cards must then be omitted rather than promise dragging).

## 9. Fixtures

- **Consumed by P4.D343 (keystone-delivered, contract K):** `fresh_schema.json` with the five columns +
  `storyteller_rounds`; `migration_indexes.json` + the FK ensure; the widened committed pairs this lane's
  families open WITHOUT the host boot — `crates/quilltap-web/tests/fixtures/salon-{main,mount}.db`
  (`salon_reads`), `inspector-{main,mount}.db` + `inspector-llm.db` / `salon-llm-logs.db` (`llm_logs_*`),
  `chat-send-{main,mount}.db`, `workbench-{main,mount}.db` (`pascal_workbench_*` — no new column needed
  there, the entry's `effect` is computed), `autonomous-{main,mount}.db` (`autonomous_rooms_web` reads
  `runDestructiveToolsAllowed` — unchanged); every `*-llmlogs.db` the `omit_columns_the_table_lacks` idiom
  must cover for `storytellerRoundId`. The per-run builders (`build-orchestrator-fixture.ts`,
  `build-native-tool-loop-fixture.ts`, `build-salon-fixture.ts`, `build-tool-build-fixture.ts`) run v4's
  real migrations at the pin and need no widening — but a builder that seeds a chat through the repository
  must use `setTurnMode` for a storyteller chat (the whole-row `update` REFUSES the patch-only field).
- **Delivered / extended by P4.D343:** `harness/oracle/fixtures/tool-build.json` (+ ceiling rows),
  `orchestrator-tier3.json` (+ `storyteller_*` rows; a storyteller chat with two LLM seats, one `silent`,
  one `talkativeness 0.2` for the filter row, a `run_custom` roster with a read and a write tool for the
  bid menu), `native-tool-loop` untouched, `message-finalizer-tier3` (+2 rows), `regenerate-swipe-tier3`
  (+1), `request-envelopes.recorded.ndjson` (+ the `toolChoice` vectors, append-only), new
  `storyteller-pure.json` (the parse / tier / prompt corpora — committed), `pascal-run-custom-triggers`
  extension (a write-effect tool with a trigger phrase, for D9). Who regenerates: P4.D343 regenerates its
  own families from the pin; the orchestrator corpus is re-recorded by the LAST lane to land at unification
  (P4.D347 → P4.D348 → P4.D343 order, §7.5).
- **P4.D344:** the e2e salon pair (`salon-main.db` / `salon-mount.db` via the host boot — the ensures
  widen it at runtime; the P4.D160-style backfill must ALTER an existing `chats`); the `support/mock-llm.ts`
  reply option if extended.

## 10. Rulings owed to the human

- **R-1 — the dry scope's v5 mechanism.** v4 guarantees a bid writes nothing by proxying EVERY repository
  write (`dry-scope.ts`), llmLogs excepted. v5 has no proxy. Options: (a) a `purpose == Bid` flag threaded to
  the known write sites the bid build reaches — v5 already routes the post-office writers through
  `BuildContextSeams` (`build_context.rs:735 post_core_whisper`, `:867/:897 sweep_stale_whispers`, the
  compression-cache write `:1175`, the recall delivery `:3237`) and the orchestrator's through
  `OrchestratorSeams`, so a `DryBuildContextSeams` wrapper that returns the would-be row without writing
  (v4's `dryAddResult` semantics) plus `purpose`-gates on the remaining direct `db.write` sites
  (`forceToolsOnNextMessage` spend `orchestrator.rs:2123-2140`, the inform consume — not reached, the
  agent-turn reset `:1414+` — continue mode, so not reached) is faithful and auditable; (b) a `Db` handle
  whose `write()` records-and-discards for the bid call (a runtime guarantee like v4's, but it would also
  discard the `llm_logs` row unless the logs partition is exempted — v4 exempts `llmLogs` by name, so a
  partition-level exemption is the same rule). **Recommend (a) + a table-delta proof in
  `storyteller_round_tier3`** (the oracle already can: memory `tsx-oracle-cjs-stubs-and-per-op-processes`).
  Either way the WARN/DEBUG `Dry scope …` lines are NO-COUNTERPART.
- **R-2 — plugin tool effects.** v5 has no tool-plugin registry; `pluginToolEffects` is always empty, so an
  (impossible) plugin tool would count as `write`. Record NO-COUNTERPART for `getToolEffects`, plugin-types
  2.9.0, MCP 1.1.45, curl 1.0.32 (vendor lane ratifies); carry the `plugin_tool_effects` map parameter
  anyway so the signatures match v4's and a future registry slots in.
- **R-3 — the two constants as host cadence.** `STORYTELLER_BOARD_TIMEOUT_MS = 45_000` and
  `STORYTELLER_CONCURRENCY_PER_PROFILE = 4` are v4 engine constants; D-series rule: the engine holds no
  clocks — the host (`storyteller_spine.rs`) owns the timeout (`tokio::time::timeout`) and the per-profile
  semaphore; the core exposes the constants. Confirm this reading of the cadence rule for a per-request
  timeout (the enclave precedent is per-host cadence injection).
- **R-4 — the SPA drag dependency.** `@dnd-kit/{core,sortable,utilities}` in v4; v5 has no `@angular/cdk`.
  Add `@angular/cdk` (DragDrop) or hand-roll. A dependency addition is a ruling by standing rule.
- **R-5 — bid rows and token aggregates.** A bid's `llm_logs` row has `messageId NULL` and never reaches the
  finalizer, so v4 adds NO cost to `chats` token aggregates for bids (the Inspector shows them; the chat
  cost summary does not). v5's cost tracker runs in the spine per `process_message` — confirm the bid path
  bypasses `PricingCostTracker` exactly as v4 bypasses `finalizeMessageResponse`'s aggregate write (a
  faithful OMISSION the dogfood row should confirm, not a divergence to fix).
- **R-6 — `ErrorKind::Conflict` exists** (`types.rs:5973`) — no ruling; noted so the 409 is typed.
- **R-7 — the `[TurnTools] Tool slate resolved` DEBUG** is `a8214b253`'s; whether P4.D348 ports the
  per-turn DEBUG (v5 has no `turn_tools` module) decides whether `effectCeiling` appears in any v5 log at
  slate time. Not a blocker.
- **R-8 — NO-PORT candidates for ratification (vendor lane):** `8becfc6ce`'s four deletions; `prettify.ts`
  labels; `child-repositories-proxy.ts`'s −157 (the classification move — v5's job runner is in-process);
  `.claude/commands/update-documentation.md`; theme-storybook mirror; the plugin bundles.

## 11. Dogfood rows the lanes should queue (💸 = spends a model call)

1. 💸 Switch a three-seat Friday-copy chat to **Storyteller**; post a public line → the board shows three
   cards `Considering…`, bids land with tiers and plans; the Inspector's **Storyteller Rounds** filter shows
   three `Bid` rows with the same round chip and `messageId` NULL; the chat's token/cost summary did NOT move
   (R-5); the DB table delta of the open is `storyteller_rounds` + `llm_logs` only (R-1 proof on real data).
2. 💸 Tick two, **Run in order** → two replies with `turnStart` depths 1 and 2; the second's prompt (Inspector
   request) carries `Since you planned it, <A> has spoken.`; both rows have `storytellerBid`; memory
   extraction / danger enqueued ONCE after the round; the summary line `Last round: A, B`.
3. 💸 A **Lookup** bid whose turn calls `doc_write_file` → the tool result reads `Not run: \`doc_write_file\` is
   a write call, and this turn allows only lookups (read-only tools). Nothing was changed. Carry on without
   it.` and the INFO line; a **Simple** bid's slate has no tools.
4. 💸 **All at once** with two Simple seats and one Act seat → two bubbles stream concurrently (frames tagged),
   ONE Prospero briefing, the Act seat afterwards; `Stop` mid-group → `chainComplete stopped paused:true`.
5. Post an `@Name:` question in storyteller mode → answered, no round. Whisper → one reply, no round, no
   chain (bug 258 + storyteller). Nudge → one turn, no ceiling, no chain.
6. **Invite by talkativeness** on with a 0.1-talkativeness seat → it is `Kept quiet this round` more often
   than not over five rounds; **Ask anyway** fetches its bid (409 once the round ran).
7. Leave storyteller mode with a round open → the board folds to `The last round closed unrun.`; a new post
   in storyteller mode supersedes an open round; **Let it lie** → `The last round was let lie.`
8. Swipe a storyteller line → the regenerate request carries the reroll plan section (no pass, no tools) and
   the new swipe row keeps `storytellerBid`.
9. Autonomous room with destructive tools NOT allowed → the per-turn slate lacks `delete_annotation` and
   `doc_delete_blob` too (the 3→5 widening) and the two DEBUGs appear; allowed → all five present.
10. The Workbench library shows `read` / `write` badges; a write-effect tool with a trigger phrase said by a
    Lookup-tier storyteller turn does NOT fire (the DEBUG), fires under Act.
11. `.qtap` export → import of a storyteller chat round-trips `turnMode`, `storytellerInviteFilter`,
    `storytellerBid`; a `replace` restore leaves `storyteller_rounds` empty; the first v5 boot on a copy v4
    already migrated runs both ensures as NO-OPs; on a copy taken BEFORE `b15fe9404` ran, the ALTERs land and
    `storyteller_rounds` is created with its FK and index.
12. 💸 NanoGPT or DeepSeek profile: a bid request's wire body carries `"tool_choice":"none"`; Anthropic:
    no `tool_choice` key and the model still answers in the four-line format (the prompt's instruction).
