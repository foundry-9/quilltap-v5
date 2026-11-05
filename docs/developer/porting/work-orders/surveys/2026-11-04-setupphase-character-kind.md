# Survey: the character-kind programme (`b09ca46bb`, `96156566b`, `3ae4cf582`, plus the `7ff5fda89` / `6121f59b9` riders on its files)

This is a read-only planning survey, written **2026-11-04**. v4 is
`~/source/quilltap-server`, read ONLY through `git show 624d0d25c:<path>`,
`git show <sha>` and `git diff 236c04e35 624d0d25c`. The working tree carries
unrelated optimizer edits and was never read. **THE PIN** is `624d0d25c`
(`4.13.0-dev.8`) and **the oracle baseline** is `236c04e35`. v5 is `main` at
**`ff1651177`** (core 0.0.1427, host 0.0.223, web 0.0.256, cli 0.0.38,
harness 0.0.1110, SPA 0.5.971).

Every v4 line number below is AT THE PIN, and every v5 line number is at
`ff1651177`. Every claim comes from a shipped hunk. Where a hunk disagrees
with the ledger row, §0.3 says so. The spec of record is
`docs/developer/features/character-kind-and-roleplay-defaults.md` at the
pin. Its status line reads "Implemented 2026-11-04, all six phases". This
survey covers phases 1, 2 and 4. Phase 3 (the sample prompts, `d0201575a`)
and phase 5 (the group scenes and the inform header, `c69a08882`) belong to
other areas and are flagged only as collisions.

## 0. Header

**Commits in scope**

| sha | subject | v4 version | proposed lane |
|---|---|---|---|
| `b09ca46bb` | feat: character kind (roleplay or assistant) | dev.2 | **K-A** (server) + **K-D** (SPA) |
| `96156566b` | feat: Agent Mode and tool rules follow the character's kind | dev.4 | **K-B** |
| `3ae4cf582` | feat: prompt generators follow the character's kind | dev.6 | **K-C** |
| `7ff5fda89` | fix: story characters no longer invent where the user is (the `STAKES_AND_REACTION_DIRECTION` bullet only) | dev.8 | **K-C** (the sample-prompt half goes to the prompt-templates lane) |
| `6121f59b9` / `afec0af10` | 4.12.1: rule 6 of `native-tool-prompt.ts` only | 4.12.1 | **K-B** (the rest of 4.12.1 goes to the window-first lane) |

**Lanes informed:** K-A … K-D, proposed below (§7). **Contracts named with:**
the prompt-trust lane (`ba6c8fe0f`, `field_semantics.rs`), the 4.12.1
window-first lane (`native_tool_prompt.rs` vs `simple_json_prompt.rs`), the
stream-frames lane (`604a082c4`: `one_shot_loop.rs`, the Brahma, help-chat
and chat orchestrators), the Salon lanes (`db6190b44`, `9eab8d2da`,
`29c1e2690`, all in `services/orchestrator.rs`), the Brahma-lookup lane
(`c539c82e0`: `qtap-export.schema.json`), the keystone or D23 lane (the
`schema-key-order.json` re-dump), and the help / docs lane.

### 0.1 Headline: ten things the planner must not miss

1. **Every v5 `properties.json` write ERASES `kind`. This is a LIVE-DATA
   hazard of the same class as P4.D349's `canDrawDiagrams` / `diagramStyle`.**
   There are four writers:
   - the read-modify-write patch writer
     `db/vault_character_update.rs:100-110` (`PROPERTY_KEYS`, 8 keys) and
     `:160-256`, which seeds from `parse_vault_properties` at
     `vault_overlay.rs:107-121` (a struct with no `kind`) and renders through
     `render_properties_json`;
   - the full-sync writer `db/vault_character_write.rs:219-235`
     (`PropertiesJson`) and `:313-330`;
   - `render_properties_json` at `:251-277`;
   - the restore backfill `db/character_vault.rs:590-601`.

   A v4 character set to **Assistant** silently reverts to roleplay on the
   next v5 write that touches pronouns, aliases, title, firstMessage,
   talkativeness, canChooseOutfit or the diagram keys. The key is dropped,
   and an absent key reads as `roleplay`.

   v4 always WRITES the key. `managed-fields.ts:238` writes it on a full sync,
   and `:456` / `:472` write it on every patch, including over a legacy file
   (v4's test `an unrelated patch over a legacy file writes the default
   kind`). So the v5 bytes diverge from v4's on every properties write, even
   for roleplay characters.

2. **The scaffold default moves and feeds the content-dedup sha.**
   `character-scaffold.ts:51` adds `kind: 'roleplay'` after `diagramStyle`.
   v5's `PROPERTIES_JSON_DEFAULT` (`db/character_vault.rs:98`) and its typed
   mirror test (`:690-726`) go from 8 keys to 9. `characters_scaffold_tier2`,
   `characters_create_tier2`, `characters_provision_tier2` and
   `provisioning_equivalence` redden at regen until K-A ports this.
3. **`kind` parses LENIENTLY.** `vault-overlay/schema.ts:59-62` is
   `z.preprocess(characterKindOrDefault, z.enum(['roleplay','assistant']).optional().default('roleplay'))`.
   A `null`, a number, `'Assistant'` or `'robot'` all read as `roleplay`,
   and none of them fails the keystone parse. It is the `diagramStyle` arm
   (`vault_overlay.rs:217-220`), not the strict `canDrawDiagrams` arm. Because
   of the Zod default, **every `parseVaultProperties` output gains
   `kind: 'roleplay'`**, so all 28 `propsCase` rows in
   `cases/vault-json-parsers.ts` move at regen.
4. **The responding character's kind now changes EVERY native-tools Salon
   turn, not just Agent-Mode turns.** `turn-tools.service.ts:190` passes
   `characterKindPolicy(character).toolRules` to the native tool-rules
   builder. Every existing character is `roleplay`, so every tool-bearing
   native turn on v4 HEAD hears `## Using tools` plus "A tool call is
   something you do; making one does not change who you are or how you
   speak." (`native-tool-prompt.ts:40-42`). Before, it heard the MANDATORY
   preamble.

   This text sits in the system message, which is part of the canned
   `provider|model|temperature|messages` key. **Every tier-3 case with
   native tools moves**: `orchestrator_tier3`, `enclave_step_tier3`,
   `primary_stream_tier3`, the regenerate-swipe and retry-uncensored
   families, and anything else that drives `processMessage`. The leg is
   measured at the first regen (§5.2).
5. **Agent Mode on a roleplay character is a different mode.** Three things
   change:
   - `buildAgentModeInstructions(maxTurns, 'in-character')` returns a NEW
     `## Acting in the scene` block (`agent-mode-resolver.service.ts:116-127`).
   - `buildForceFinalMessage('in-character')` returns a NEW sentence
     (`:167-170`).
   - `submit_final_response` is NOT OFFERED. `orchestrator.service.ts:689`
     sets `offerSubmitFinalResponse: agentMode.enabled && agentMode.flavor === 'research'`,
     which reaches `buildTools` at `turn-tools.service.ts:153`.

   The tool is also not terminal: `native-tool-loop.service.ts:145`
   (`submitOffered`) gates detection (`:171`) and the force-final promotion
   (`:465`). A stray call runs through the executor like any other tool
   (`tool-executor.ts:1091`). That matches v5's `executor.rs:664`, so v5
   needs no executor change.

   The research bytes are UNCHANGED, verified byte for byte against v5
   `agent_mode.rs:156-194`.
6. **Brahma, Help chat and the one-shot loop pin `assistant` + `research`.**
   The sites are `brahma-console/orchestrator.service.ts:257/264/331`,
   `help-chat/orchestrator.service.ts:271/276/349` and
   `agent-loop/one-shot-loop.ts:115/118/213`. Their bytes do not change, so
   `brahma_orchestrator_tier3`, `brahma_console_tier3`,
   `help_chat_orchestrator_tier3`, `carina_query_tier3` and
   `scenario_builder_tier3` must STAY GREEN at regen. That is the convergence
   proof for these sites.
7. **Three oracle cases go silently wrong at the pin, by signature drift
   rather than import death.** tsx strips types, so a missing argument
   compiles. Each must be re-aimed, not "fixed in v5":
   - `cases/pseudo-tool-prompts.ts:35-38` calls `buildNativeToolInstructions(true)`.
     At the pin that is `PREAMBLES[true]`, which is `undefined`, so the row
     records the string **`"undefined\n\n1. **Never narrate…"`**.
   - `cases/native-tool-loop-tier3.test.ts:343-345` passes
     `agentMode: {enabled, maxTurns}` with no `flavor`. At the pin
     `submitOffered` is false, so the 4 submit rows in
     `fixtures/native-tool-loop-tier3.json` (`enabled: true`) record
     "submit is an ordinary call".
   - `cases/turn-extras.ts:150` passes no `flavor`, which falls through to
     research. That row is vacuously unchanged.
8. **The Wizard gains a NEW read and three NEW log lines; the ledger names
   only External Prompt's line.** `resolveWizardCharacterKind`
   (`character-wizard.service.ts:746-766`) works as follows:
   - with a `characterId`, it does `repos.characters.findById` (an overlaid
     read) and logs DEBUG `[CharacterWizard] Writing for the character's kind {characterId, kind}`;
   - with no id, it logs DEBUG `[CharacterWizard] New character; writing for the default kind {kind}`;
   - on a throw, it logs WARN `[CharacterWizard] Could not read the character; writing for the default kind {characterId, error}`
     and writes for the default.

   The Optimizer adds DEBUG `[CharacterOptimizer] Optimizing for the
   character's kind {characterId, kind}` (`character-optimizer.service.ts:738`).
   External Prompt adds DEBUG `External prompt meta-prompt built for the
   character's kind {characterId, kind}` (`external-prompt-generator.service.ts:133`),
   which fires BEFORE the system-prompt lookup.
9. **The Wizard's system-prompt field moves out of `FIELD_PROMPTS`.**
   `FIELD_PROMPTS` goes from 10 entries to 9. The field becomes
   `buildSystemPromptFieldPrompt(kind)` (`:224`), routed by
   `wizardFieldPrompt(field, kind)` (`:254`), and gets its own **2500-token**
   budget through `wizardFieldMaxTokens` (`:731-736`; it was a shared 1000).
   - v5 `wizard_prompts.rs:169` (`FIELD_PROMPT_SYSTEM_PROMPT`) and `:211-222`
     (`FIELD_PROMPTS`, 10 entries) change.
   - The inline match at `wizard.rs:938-942` changes.
   - `cases/generators-wizard-prompts.ts:43/82` record `Object.keys(FIELD_PROMPTS)`,
     so that record LOSES `systemPrompt`.
10. **`field_semantics.rs` is a three-commit file.** `ba6c8fe0f` (the
    prompt-trust lane) rewrites `TRUST_SAFEGUARDS_DIRECTION`,
    `COMPANION_TRUST_DISPOSITION`, `COMMITTEE_DRIFT_GUARDRAIL` and
    `EXAMPLE_DIALOGUE_COVERAGE`. `3ae4cf582` adds five exports and two
    functions. `7ff5fda89` adds one bullet. **ONE lane must own the file at
    the pin's bytes** (ruling R-K1). The generator prompt bytes depend on all
    three.

### 0.2 Rulings owed (full list in §10)

- **R-K1:** who owns `generators/field_semantics.rs`.
- **R-K2:** who owns `tools/native_tool_prompt.rs`.
- **R-K3:** whether to port the three agent-mode INFO lines that v5 never
  logged and that now carry `flavor`.
- **R-K4:** hunk ownership of `services/orchestrator.rs`.
- **R-K5:** one re-vendor of `qtap-export.schema.json` and one re-dump of
  `schema-key-order.json`.
- **R-K6:** the SPA kind module and a chokepoint spec.

### 0.3 Ledger corrections (each verified against the pin's bytes)

1. **Row `96156566b`:** "the `tool_build_equivalence` family's slates for a
   roleplay character LOSE `submit_final_response`" is **wrong**.
   `cases/tool-build.test.ts:160-169` drives v4's `buildTools` directly with
   its own `agentModeEnabled`. `buildTools` is untouched in this span; the
   change is in its CALLER (`turn-tools.service.ts:153` gets the value from
   `orchestrator.service.ts:689`). **`tool_build_equivalence` does not move**
   from this commit. The slate loss shows up only where the orchestrator runs:
   `orchestrator_tier3` (the 2 `agentModeEnabled: true` chats,
   `fixtures/orchestrator-tier3.json:1098`, `:2029`), `enclave_step_tier3`
   and `primary_stream_tier3`, through the logged request's tools and turn
   extras' `tool_schema_tokens`.
2. **Row `96156566b`:** the row says the INFO line "gains `flavor`". In fact
   **three** v4 lines gain it:
   - `[AgentMode] Resolved agent mode` (`agent-mode-resolver.service.ts:95-98`,
     via `...result`);
   - `Injected agent mode instructions` (`orchestrator.service.ts:864-869`);
   - `Agent mode max turns reached, forcing final response`
     (`native-tool-loop.service.ts:413-418`).

   **v5 logs NONE of the three**. A grep for each text in
   `crates/quilltap-core/src` finds nothing, and `native_tool_loop.rs:631`
   says "host-side logging, omitted". This is a pre-existing gap (R-K3).
   `[AgentMode] Resolved agent mode` also fires from the chat GET
   (`app/api/v1/chats/[id]/handlers/get.ts:348`) and the agent-mode toggle
   (`actions/agent-mode.ts:75`). The wire keys do NOT change: both emit only
   `enabled` / `enabledSource`.
3. **Row `3ae4cf582`:** the row names only External Prompt's DEBUG line.
   Add:
   - the Optimizer's DEBUG (`:738`);
   - the Wizard's two DEBUG lines and one WARN (`:751`, `:757`, `:760`);
   - the Wizard's NEW `repos.characters.findById` when `characterId` is set
     (`:754`);
   - the REMOVED exports `META_SYSTEM_PROMPT` (now `buildMetaSystemPrompt(kind)`,
     `:57`) and `FIELD_PROMPTS.systemPrompt`;
   - the NEW exports `wizardFieldPrompt` and `wizardFieldMaxTokens`.

   The v5 oracle cases import neither removed name, so nothing in v5 dies:
   the only `FIELD_PROMPTS` reader is `generators-wizard-prompts.ts`, which
   iterates it, and no case imports `META_SYSTEM_PROMPT`.
4. **Row `b09ca46bb`:** the row is accurate. One addition: v4's character
   ITEM GET (`[id]/handlers/get.ts`) emits `kind` only because the hydrated
   row carries it. v5 `character_enrichment.rs:243-268` (`build_detail`)
   spreads the row, so K-A gets this for free once `characters_read.rs:236-244`
   and the read overlay carry the key. **No other wire carries `kind`.**
   `6588972d5` put `diagramStyle` on chat participants
   (`chat_participants.rs:376`, `chat_enrichment.rs:294`), but
   `b09ca46bb` did NOT do the same for `kind`. Do not add it there.
5. **Row `6121f59b9`:** "take `main`'s file, not 4.12.1's" is confirmed.
   `native-tool-prompt.ts:34` at the pin is the 4.12.1 rule 6 inside
   `NATIVE_TOOL_RULES`. `simple-json-prompt.ts:266` carries the same new
   sentence, but that file belongs to the window-first lane.

## 1. Per commit, the shipped hunks

### 1.1 `b09ca46bb`: the `kind` flag (31 files; 15 production)

- **NEW `lib/characters/kind.ts`** (99 lines at the pin, client-safe):
  - `CHARACTER_KINDS` (`:28`) and `DEFAULT_CHARACTER_KIND = 'roleplay'`
    (`:32`);
  - `characterKindOrDefault` (`:35-37`): EXACT `=== 'roleplay' | 'assistant'`,
    so case matters;
  - `resolveCharacterKind` (`:43-47`) and `isAssistantCharacter` (`:50-52`;
    no v4 production caller at the pin);
  - from `96156566b` / `3ae4cf582`: `AgentModeFlavor`, `ToolRulesVariant`,
    `GeneratorDirectionKey`, `KindPolicy`, `KIND_POLICIES` (`:86-89`),
    `kindPolicy` and `characterKindPolicy`.
- **Vault:**
  - `vault-overlay/schema.ts:59-62` (the lenient arm) and `:251`
    (`MANAGED_FIELDS` gains `'kind'` after `'diagramStyle'`);
  - `managed-fields.ts`:
    - `:99`: the snapshot `snapshot.kind = props.kind`;
    - `:238`: full sync, `kind: characterKindOrDefault(character.kind)`
      written LAST, after `diagramStyle`;
    - `:428`: `propsKeys` gains `'kind'`;
    - `:456`: the `current` seed when the file is absent;
    - `:472`: `'kind' in patch ? characterKindOrDefault(patch.kind) : current.kind`;
    - `:481-486`: a NEW DEBUG `[VaultOverlay] Writing character kind to properties.json { characterId, kind }`,
      emitted only when `kind` was touched, after the diagram line;
  - `read-overlay.ts:180`: `kind: parsed.kind` spread after `diagramStyle`;
  - `character-scaffold.ts:51`: `kind: 'roleplay'`.
- **Schema:** `character.types.ts:273` declares
  `kind: z.enum(CHARACTER_KINDS).default('roleplay')` after `diagramStyle`
  and before `systemTransparency`. So the export key order moves from 49
  keys to 50 (§3).
- **API:**
  - `[id]/handlers/put.ts:81`: `kind: z.enum(CHARACTER_KINDS).optional()`.
    A bad value is a Zod `invalid_value` with options `['roleplay','assistant']`.
    `null` is refused, because `.optional()` is not nullable.
  - `handlers/get.ts:84`: the list DTO gains
    `kind: resolveCharacterKind(character)` after `diagramStyle`.
- **`.qtap` schema:** `public/schemas/qtap-export.schema.json:496-500`.
- **SPA:**
  - `components/characters/CharacterKindPicker.tsx` (80 lines: radiogroup,
    `useId` name, legend "Kind of character", two options with hints);
  - `app/aurora/shared/save-character-kind.ts` (PUT `{kind}`, two success
    toasts, `throw new Error('Failed to update the kind of character')`);
  - the hooks `useCharacterView.ts` (`handleSaveKind`, optimistic update,
    error toast, `console.error`, refetch) and `useCharacterEdit.ts:299-313`;
  - the cards `CharacterDetailView.tsx:245-250` and
    `CharacterEditView.tsx:260-265`, both ABOVE the diagrams card.
- **Chokepoint test:**
  `__tests__/unit/lib/characters/kind-chokepoint.test.ts` scans `lib/`,
  `app/`, `components/` and `hooks/` (excluding `__tests__` and
  `node_modules`) for
  `/\bkind\s*[!=]==?\s*['"](?:roleplay|assistant)['"]|['"](?:roleplay|assistant)['"]\s*[!=]==?\s*[\w.?]*\bkind\b/`.
  Only `lib/characters/kind.ts` is exempt, and the test has five self-test
  probes.
- **Docs:** `help/character-editing.md`, `docs/developer/DDL.md`, the spec.
- No migration and no D23 move. Nothing reads `kind` for behaviour in this
  commit.

### 1.2 `96156566b`: Agent Mode flavours + tool-rules preamble (26 files; 11 production)

- `agent-mode-resolver.service.ts`:
  - `:92`: `flavor: characterKindPolicy(character).agentFlavor`, where
    `character` is the RESPONDING character and may be null, which gives
    in-character;
  - `:116-128`: `buildAgentModeInstructions(maxTurns, flavor)`. The
    in-character block is verbatim in the hunk, ends `**${maxTurns} tool steps**`
    and is `.trim()`ed. The research text is unchanged.
  - `:167-172`: `buildForceFinalMessage(flavor)`.
- `native-tool-loop.service.ts`:
  - `:145`: `submitOffered = agentMode.enabled && agentMode.flavor === 'research'`;
  - `:171`: submit detection;
  - `:417`: force-final INFO gains `flavor`;
  - `:427`: the flavour's message;
  - `:465`: `if (submitOffered && chunk.rawResponse)` gates promotion.
  - Unchanged: `:131` (`effectiveMaxTurns = enabled ? maxTurns : 5`) and
    `:260` (iteration summaries while `enabled`). In-character still
    iterates to `maxTurns` and still runs the force-final pass.
- `orchestrator.service.ts:519` (resolve), `:689` (`offerSubmitFinalResponse`)
  and `:868` (INFO `flavor`).
- `turn-tools.service.ts:87` (`offerSubmitFinalResponse` param), `:153`
  (into `buildTools.agentModeEnabled`) and `:190` (the variant from the kind).
- `turn-extras.ts:58` (`agentMode.flavor`) and `:91`.
- `pseudo-tool.service.ts:53-55`:
  `buildNativeToolSystemInstructions(variant)`.
- `native-tool-prompt.ts`: `NATIVE_TOOL_RULES` (`:24-34`, exported),
  `PREAMBLES` (`:36-43`) and `buildNativeToolInstructions(variant, hasTools = true)`
  (`:55-63`). The `assistant` preamble plus the rules equals the old output
  byte for byte, except for 4.12.1's rule 6 (§1.4).
- Pinned callers: one-shot, Brahma and Help, as in §0.1 item 6.
- `help/agent-mode.md` +26 / −6 and `help/character-editing.md` +1.

### 1.3 `3ae4cf582` (+ `7ff5fda89`'s bullet): generators by kind (23 files; 6 production)

- `character-field-semantics.ts` at the pin:
  - `:126-133`: `STAKES_AND_REACTION_DIRECTION`, six bullets including
    `7ff5fda89`'s off-screen `{{user}}` bullet;
  - `:140-145`: `ASSISTANT_TASK_DIRECTION`;
  - `:147-150`: `GENERATOR_DIRECTIONS`;
  - `:157-159`: `generatorDirectionFor`;
  - `:168`: `FLATTENING_DRIFT_SIGNAL`;
  - `:175-177`: `kindDriftSignalFor` (null for `task`).
- `ai-import.service.ts:209-231`: `SYSTEM_PROMPTS_PROMPT` gains
  `${generatorDirectionFor(DEFAULT_CHARACTER_KIND)}` (`:228`) between
  `TRUST_SAFEGUARDS_DIRECTION` and `GATED_COMPANION_TRUST_DISPOSITION`, each
  after a blank line. It is a constant, always stakes.
- `character-wizard.service.ts`:
  - `:224-251`: `buildSystemPromptFieldPrompt(kind)`, the direction between
    trust and gate;
  - `:254-256`: `wizardFieldPrompt`;
  - `:731-736`: `wizardFieldMaxTokens` (`systemPrompt` 2500,
    `exampleDialogues` / `firstMessage` 1000, `scenarios` 4000, otherwise 500);
  - `:746-766`: `resolveWizardCharacterKind`;
  - `:917` and `:1183`: called once per run, AFTER the context prompt and
    BEFORE the field loop, in both twins;
  - `:958-959` and `:1236-1237`: the field loops.
- `external-prompt-generator.service.ts`:
  - `:57-83`: `buildMetaSystemPrompt(kind)`, the direction between trust and
    gate;
  - `:131-133`: kind + DEBUG after the not-found return, before the
    system-prompt lookup;
  - `:162`: the token estimate;
  - `:180` and `:223`: the call and the log.
- `character-optimizer.service.ts`:
  - `:310-339`: `getAnalysisPrompt(kind)`. The flattening look-for is
    appended to the Committee bullet, and `\n\n${driftSignal}` after the
    guardrail.
  - `:450-490`: `getSystemPromptSuggestionPrompt(analysis, prompt, kind)`
    adds two lines after `- ${COMMITTEE_DRIFT_GUARDRAIL}` (`:482-483`).
  - `:623-640`: `getNewSystemPromptsSuggestionPrompt(analysis, kind)` adds
    `- ${generatorDirectionFor(kind)}` (`:636`).
  - `:737-738`: kind + DEBUG after the `Character not found` throw.
  - The call sites are `:870`, `:1115` and `:1140`.

### 1.4 `6121f59b9`: rule 6 (the K-B part only)

`native-tool-prompt.ts:34` rewords rule 6. The sentence "If you find
nothing, say you don't recall" becomes "If the search lists conversations
from that period, read the likeliest one with `read_conversation` before
answering. Only if it finds nothing and lists no conversation from that
period, say you don't recall". v5 `native_tool_prompt.rs:24` carries the old
sentence.

## 2. The v5 surface map

### 2.1 K-A: the flag, the vault, the wire (`b09ca46bb` server)

| v4 (pin) | v5 home (`ff1651177`) | earlier port |
|---|---|---|
| `kind.ts` (whole) | **NEW** `crates/quilltap-core/src/characters/kind.rs` + one `pub mod kind;` in `characters/mod.rs` (beside `diagrams.rs`) | n/a |
| `schema.ts:59-62` lenient parse | `vault_overlay.rs:97-121` (`CharacterVaultProperties` gains `kind: CharacterKind`, serialized LAST) and `:217-231` (`parse_vault_properties`: `kind_or_default(obj.get("kind"))`, never fails) | P4.22, P4.D349 |
| `schema.ts:251` `MANAGED_FIELDS` | `db/characters.rs:105-123` (`MANAGED_FIELDS`: add `"kind"` after `"diagramStyle"`; routes a kind-only PUT to the vault and strips it from slim writes) | P4.D349 |
| `managed-fields.ts:238` full sync | `db/vault_character_write.rs:125-160` (`CharacterVaultWriteInput` gains `pub kind: Option<Value>`, RAW like `diagram_style`; serde camelCase threads import, restore, post office and subprompts for free), `:219-235` (`PropertiesJson` + `kind`), `:313-330` | P4.D349 |
| `managed-fields.ts:428/456/472/481-486` RMW patch | `db/vault_character_update.rs:100-110` (`PROPERTY_KEYS` gains `"kind"`, 9 keys), `:226-256` (the `kind` arm + the NEW DEBUG with fields `characterId`, `kind`, AFTER the diagram DEBUG), `:647-660` (`empty_properties_default` gains `Roleplay`) | P4.22, P4.D349 |
| `render_properties_json` | `db/vault_character_write.rs:251-277` (one more parameter; the `clippy::too_many_arguments` allow already stands) | P4.D349 |
| restore backfill | `db/character_vault.rs:590-601` | P4.158, P4.D349 |
| scaffold `:51` | `db/character_vault.rs:98` (`PROPERTIES_JSON_DEFAULT`, 9 keys) + the typed mirror test `:690-726` | P4.D349 |
| `read-overlay.ts:180` | `db/vault_read_overlay.rs:333-352` (the key loop gains `"kind"` after `"diagramStyle"`) | P4.D349 |
| `CharacterSchema` default | `db/characters_read.rs:236-244` (materialize `"kind": "roleplay"` after `diagramStyle` on every raw read) | P4.D349 |
| PUT `:81` | `api/characters.rs:2230-2260` (the accepted-keys list gains `"kind"` after `"diagramStyle"`) and `:2405-2440` (the validator gains an enum arm modelled on the `diagramStyle` arm `:2428-2436`, `null` refused) | P4.D349 |
| list GET `:84` | `services/character_enrichment.rs:132-145` (insert `kind` after `diagramStyle`, through the resolver) | P4.D349 |
| `.qtap` schema `:496-500` | `generators/qtap-export.schema.json:491` area. **Shared with `c539c82e0`**; one re-vendor (R-K5) | P4.D272 / P4.D294 |
| `.qtap` key order | `services/qtap_export/schema-key-order.json` (`character` 49 → 50, `kind` after `diagramStyle`). **One re-dump** (R-K5); the pinned test `key_order.rs:238-240` adds a `kind` assertion | P4.D294 / P4.D327 |
| chokepoint test | **NEW** `crates/quilltap-harness/tests/character_kind_chokepoint_census.rs` (§5.4) | n/a |

### 2.2 K-B: Agent Mode + tool rules (`96156566b` + rule 6)

| v4 (pin) | v5 home | earlier port |
|---|---|---|
| `ResolvedAgentMode.flavor` / resolver `:92` | `services/agent_mode.rs:58-65` (add `flavor: AgentModeFlavor`) and `:116-148` (take the character's kind or policy; `None` gives in-character) | W4.4, P4.9e3a |
| `buildAgentModeInstructions` `:116-128` | `agent_mode.rs:156-182` (+ flavor; the in-character text verbatim) | W4.4 |
| `buildForceFinalMessage` `:167-172` | `agent_mode.rs:188-194` | W4.4 |
| resolver callers | `services/orchestrator.rs:1437-1445` (pass the responding `character`), `api/salon.rs:318`, `services/chat_admin.rs:309` (pass the primary character; wire unchanged) | W4.4, P4.9e3a |
| `orchestrator.service.ts:689` | `services/orchestrator.rs:2217` (`agent_mode_enabled` → `agent_mode.enabled && flavor == Research`) | W4.1g |
| `turn-tools.service.ts:190` | `services/orchestrator.rs:2373` (`build_native_tool_system_instructions(policy.tool_rules)`) | W4.x |
| `turn-extras.ts:58/91` | `services/turn_extras.rs:76-84` (`TurnExtrasOptions` + `agent_mode_flavor`), `:111-117`; caller `orchestrator.rs:2445-2450` | P4.D82 |
| loop `:145/171/465` | `services/native_tool_loop.rs:289-295` (`submit_offered`), `:771-780` (promotion gated), `:634-660` (`build_force_final_message(agent_mode.flavor)`); the test constructors `:948`, `:1065` | P4.13 / P4.14 |
| `pseudo-tool.service.ts:53` | `services/pseudo_tool.rs:76-79` | W4.x |
| `native-tool-prompt.ts` (whole) | `tools/native_tool_prompt.rs:1-25` (`NATIVE_TOOL_RULES` pub const, `PREAMBLES`, `build_native_tool_instructions(variant, has_tools)`, rule 6 at the PIN) | W4.x, P4.D13 |
| pinned callers | `services/agent_loop/one_shot_loop.rs:261/264/628`, `services/brahma_console/orchestrator.rs:466/472/552`, `services/help_chat/orchestrator.rs:819/822/907` (pass `Assistant` / `Research`) | P4.165 / P4.9i2a |
| the three INFO lines (R-K3) | `agent_mode.rs` (resolver), `orchestrator.rs` beside `:2840-2845` (the injected-instructions block at `:2848`), `native_tool_loop.rs:634` | not ported |

### 2.3 K-C: generators (`3ae4cf582` + `7ff5fda89` bullet)

| v4 (pin) | v5 home | earlier port |
|---|---|---|
| the field-semantics additions | `generators/field_semantics.rs` (5 consts + `generator_direction_for` + `kind_drift_signal_for`; `ALL_EXPORTS` `:179-205` gains `STAKES_AND_REACTION_DIRECTION`, `ASSISTANT_TASK_DIRECTION`, `FLATTENING_DRIFT_SIGNAL`); and, per R-K1, `ba6c8fe0f`'s four rewritten consts `:46`, `:62`, `:78`, `:83` | P4.9k0, P4.D237, P4.D241 |
| `SYSTEM_PROMPTS_PROMPT` `:228` | `generators/ai_import.rs:203-232` (insert between `:228` and `:230`) | P4.9k2 |
| Wizard prompts | `generators/wizard_prompts.rs:169-207` (→ `build_system_prompt_field_prompt(kind)`), `:211-222` (`FIELD_PROMPTS` loses `systemPrompt`, 9 entries), `:225` (`field_prompt` → `wizard_field_prompt(field, kind)`) | P4.9k2 |
| Wizard runner | `generators/wizard.rs:704-…` (`run_wizard_core`, the one body of both twins): `resolve_wizard_character_kind` after `:905`, before the loop; `:936-942` → `wizard_field_prompt` + `wizard_field_max_tokens` | P4.9k2 |
| External Prompt | `generators/external_prompt.rs:100-135` (`META_SYSTEM_PROMPT` `LazyLock` → `build_meta_system_prompt(kind)`), `:339-340` (kind + DEBUG after the not-found return), `:383`, `:432`, `:511` | P4.9k1 |
| Optimizer | `generators/optimizer.rs:423` (`get_analysis_prompt(kind)`), `:533` (+ kind), `:744` (+ kind), after `:1780` (kind + DEBUG), `:1959`, `:2032`, `:2059` | P4.9k1, P4.D241 |

### 2.4 K-D: the SPA (`b09ca46bb` client)

| v4 (pin) | v5 home |
|---|---|
| `CharacterKindPicker.tsx` + `save-character-kind.ts` | **NEW** `apps/web/src/app/screens/characters/character-kind-card.ts` (+ `.spec.ts`), modelled on `can-draw-diagrams-card.ts` (155 lines; owns its own PUT + toast + refetch). The radio `name` needs a per-instance id (v4's `useId` reason: the workspace keeps the view and edit tabs side by side) |
| `kind.ts` client half | **NEW** `apps/web/src/app/core/character-kind.ts` (`CHARACTER_KINDS`, `resolveCharacterKind`) (R-K6) |
| view `:245` | `screens/characters/view/character-detail.ts:224` (insert ABOVE `<qt-can-draw-diagrams-card>`; `[disabled]="isArchived()"` as the sibling) |
| edit `:260` | `screens/characters/edit/character-edit.ts:163` (insert ABOVE) |
| types | `core/core-contract.ts:3565` (`DetailCharacter`), `:3830` (`CharacterListItem`), `:3886` (`CharacterDetail`): `kind?: 'roleplay' \| 'assistant'` beside `diagramStyle` |
| chokepoint | **NEW** SPA spec running v4's regex verbatim over `apps/web/src` (R-K6) |

### 2.5 NO-COUNTERPART / NO-PORT

- `isAssistantCharacter` has no v4 production caller. Port it into
  `kind.rs` for parity; it costs nothing.
- `README.md`, `package*.json`, `CLAUDE.md` and `CHANGELOG` stamps: NO-PORT.
- `docs/developer/DDL.md`: goes to the docs mirror lane.
- `help/*` (`character-editing.md`, `agent-mode.md`, `character-creation.md`,
  `ai-character-import.md`, `character-external-prompt.md`,
  `character-optimizer.md`) and the spec: go to the round's help / docs lane.
  `character-optimizer.md` is also edited by `ba6c8fe0f`, so ONE re-vendor
  at the pin.

## 3. Schema / D23

- **No D23 move and no migration.** `kind` lives in the vault JSON.
- **The export key order moves** (`CharacterSchema` `:273`): `character` goes
  from 49 keys to 50, with `kind` after `diagramStyle` and before
  `systemTransparency`. It is regenerated by
  `harness/oracle/fixtures/dump-export-key-order.ts`. Other schema moves in
  this span (`c539c82e0`'s `ProjectSchema.brahmaConnectionProfileId` and the
  settings types) touch the same dump, so the dump needs ONE owner (R-K5).

## 4. Log lines (v4 text + keys + level → v5 home)

| v4 | level | keys (order) | v5 home | lane |
|---|---|---|---|---|
| `[VaultOverlay] Writing character kind to properties.json` (`managed-fields.ts:482`) | debug | `characterId, kind` | `vault_character_update.rs` after `:233-239` | K-A |
| `[AgentMode] Resolved agent mode` (`agent-mode-resolver.service.ts:95`) | info | `chatId, enabled, maxTurns, enabledSource, flavor` | `agent_mode.rs` or each caller (R-K3) | K-B |
| `Injected agent mode instructions` (`orchestrator.service.ts:864`) | info | `chatId, maxTurns, enabledSource, flavor` | `orchestrator.rs` at the `:2848` splice (R-K3) | K-B |
| `Agent mode max turns reached, forcing final response` (`native-tool-loop.service.ts:413`) | info | `chatId, iterations, maxTurns, flavor` | `native_tool_loop.rs:634` (R-K3) | K-B |
| `[CharacterWizard] New character; writing for the default kind` (`:751`) | debug | `kind` | `wizard.rs` | K-C |
| `[CharacterWizard] Writing for the character's kind` (`:757`) | debug | `characterId, kind` | `wizard.rs` | K-C |
| `[CharacterWizard] Could not read the character; writing for the default kind` (`:760`) | **warn** | `characterId, error` | `wizard.rs` | K-C |
| `External prompt meta-prompt built for the character's kind` (`:133`) | debug | `characterId, kind` | `external_prompt.rs` (`target: quilltap::external_prompt_generator`) | K-C |
| `[CharacterOptimizer] Optimizing for the character's kind` (`:738`) | debug | `characterId, kind` | `optimizer.rs` after `:1780` | K-C |

## 5. Harness / oracle impact

### 5.1 Families that redden at a post-pin regen (v4 drift reds, NOT v5 bugs)

- **K-A:**
  - `vault_json_parsers_equivalence`: all 28 `propsCase` outputs gain
    `kind`.
  - `characters_scaffold_tier2`, `characters_create_tier2`,
    `characters_provision_tier2`, `characters_update_tier2`,
    `characters_slim_tier2` and `characters_physical_tier2`: each
    `properties.json` write now carries `"kind"`, and the scaffold sha moves.
  - `characters_mutations_equivalence` (the PUT).
  - `characters_read_equivalence` and `characters_reads_equivalence`: raw
    and hydrated rows plus the list DTO.
  - `provisioning_equivalence`.
  - `metadata_vault_roundtrip_equivalence`.
  - `system_export_equivalence`: the character records gain `kind` in its
    slot.
  - `qtap_import_equivalence` and `system_backup_equivalence` /
    `system_restore_equivalence` / `system_restore_state`: the vault file
    bytes.
  - `store_name_reconcile_equivalence`: it dumps character reads.
  - Measure the rest by grepping the regen NDJSON for `"kind":"roleplay"`.
- **K-B:**
  - `pseudo_tool_prompts_equivalence` (re-aim, §0.1 item 7).
  - `native_tool_loop_tier3_equivalence` (re-aim).
  - `turn_extras_equivalence` (add rows).
  - `orchestrator_tier3_equivalence`: the preamble moves on every tool-bearing
    native case, and the 2 agent chats go in-character.
  - `enclave_step_tier3_equivalence`, `primary_stream_tier3`, and the
    `regenerate-swipe-tier3` / `retry-uncensored-tier3` families.
  - **Must stay green:** `brahma_orchestrator_tier3`, `brahma_console_tier3`,
    `help_chat_orchestrator_tier3`, `carina_query_tier3`,
    `scenario_builder_tier3` and `tool_build_equivalence`.
- **K-C:**
  - `generators_leaf_equivalence` / the field-semantics recorder
    `cases/generators-field-semantics.ts`. Its `exports_` list is
    hand-written (`:34`), so NEW exports appear only when added. The
    `ba6c8fe0f` rewrites redden it regardless.
  - `generators_wizard_prompts_equivalence` (`FIELD_PROMPTS` keys 10 → 9).
  - `character_optimizer_prompts_equivalence`: `getAnalysisPrompt()` with
    no kind falls through `kindPolicy(undefined)` to roleplay, so it
    reddens with stakes text, correct-by-default rather than garbage.
  - `ai_import_assembly_equivalence`.
  - `ai_import_tier3_equivalence`, `character_wizard_tier3_equivalence`,
    `character_optimizer_tier3_equivalence` and
    `external_prompt_tier3_equivalence`: the canned keys include the system
    messages.

### 5.2 Re-aims (oracle side, before any v5 diff)

1. `cases/pseudo-tool-prompts.ts:35-38`: emit `build_native` per variant
   (`assistant` / `roleplay` × `hasTools` true / false / default) and
   `build_native_system` per variant, plus a `native_tool_rules` row for the
   exported constant. The v5 test `pseudo_tool_prompts_equivalence.rs:46-50`
   reads `variant`.
2. `cases/native-tool-loop-tier3.test.ts:94/343-345` + the fixture: add
   `flavor` to `agentMode`. The 4 existing `enabled: true` rows get
   `research`, so their bytes are preserved. Add in-character rows copied
   from v4's `native-tool-loop.service.test.ts:366` / `:395` titles:
   - a stray submit runs as an ordinary call and replaces nothing;
   - max-turns asks for prose and never promotes.
3. `cases/turn-extras.ts:139-150`: a `flavor` column, with both flavours
   for each enabled row.
4. `cases/character-optimizer-prompts.ts:93/110/130`: one row per kind for
   each of the three prompts.
5. `cases/generators-wizard-prompts.ts`: add `buildSystemPromptFieldPrompt`
   × 2 kinds and `wizardFieldMaxTokens` over the 10 field names plus one
   unknown.
6. `cases/generators-field-semantics.ts:34`: add the three consts plus
   `generatorDirectionFor` / `kindDriftSignalFor` × 2 kinds (and `null`
   rendered).

### 5.3 New differentials (closest recipe)

- **Tier 1, `kind.rs`:** a NEW `cases/character-kind.ts`, modelled on
  `cases/vault-json-parsers.ts` (a tsx recorder). Rows:
  `characterKindOrDefault` over `'roleplay'`, `'assistant'`, `'Assistant'`,
  `null`, `5`, `undefined`, `'robot'` and `''`, then `kindPolicy` and
  `characterKindPolicy(null)`. Corpus ids are v4's `kind.test.ts` titles.
- **Tier 1, agent mode:** a NEW `cases/agent-mode-flavor.ts`, modelled on
  `turn-extras.ts`, using `agent-mode-flavor.test.ts`'s 8 titles. Exercise
  the resolver with characters `{kind:'assistant'}`, `{}`, `null` and
  `{kind:'robot'}` across the cascade.
- **Tier 2, the vault:** extend `cases/characters-mutations.test.ts` beside
  `:966-972` (the `diagramStyle` PUT rows). Rows:
  - `{kind:'assistant'}`, `{kind:'robot'}` (400), `{kind:null}` (400);
  - an unrelated `talkativeness` PATCH over an assistant file, which must
    keep it (v4 `:1830`);
  - the same over a legacy file, which writes `roleplay` (`:1836`);
  - a hand-planted `"kind": 5`, which reads roleplay.

  Plus `vault-json-parsers.ts` rows `p-kind-assistant`, `p-kind-robot`,
  `p-kind-null`, `p-kind-number` beside `:196-200`.
- **Tier 3, the Salon:** an assistant-kind plant in `orchestrator_tier3`
  (research bytes + `submit_final_response` on the slate + the MANDATORY
  preamble). **Plant in-case** by copying the per-case DB and rewriting the
  character's `properties.json` document. Do not edit the SHARED
  `build-orchestrator-fixture.ts`; see memory note
  `jest-oracle-in-case-plants`.
- **Tier 3, the Wizard:** `character-wizard-tier3` rows with `characterId`
  (stored assistant gives task; absent id gives the default DEBUG; an
  unreadable vault gives the WARN). `external-prompt-tier3` and
  `character-optimizer-tier3` each get one assistant row (the task
  direction; no flattening).

### 5.4 The chokepoint census (K-A)

`crates/quilltap-harness/tests/character_kind_chokepoint_census.rs` is
modelled on the `source_census/` guards (e.g.
`chat_informs_in_force_census.rs`). It scans `crates/quilltap-core/src`,
`crates/quilltap-web/src` and `crates/quilltap-cli/src` for a Rust shape of
v4's regex. Matching `kind` alone is not enough: `"kind"` is everywhere in
v5 (descriptor kinds, memory kinds), and `role == "assistant"` must stay
legal. The fence:

```
\bkind\b[^;\n]{0,60}?(==|!=)\s*(Some\()?"(roleplay|assistant)"|"(roleplay|assistant)"\s*(==|!=)[^;\n]{0,40}\bkind\b
```

It exempts `characters/kind.rs`, and it carries v4's five self-test probes
translated (memory note: guards pin literals plus a self-test).

## 6. SPA

- **The card** (`character-kind-card.ts`):
  - copy: legend "Kind of character"; hint "Whether this character is a
    player in the drama or a member of the staff. It decides how they are
    directed when the curtain is up.";
  - the two options and hints are verbatim from `CharacterKindPicker.tsx:27-38`;
  - toasts are verbatim from `save-character-kind.ts:11-14`; the error toast
    reads `Failed to update the kind of character`;
  - the card is disabled while saving, and when there is no character (plus
    `isArchived()` on the view, as its sibling does).
- **Zoneless traps:** seed the selected value from the input through
  `linkedSignal`, never inside a query function (memory note
  `unify-review-finding-shapes`: forms seeded in a cached queryFn).
- **e2e:** a gated beat in `e2e/characters-flow.spec.ts`, or a new
  `character-kind-flow.spec.ts`, modelled on `e2e/diagrams-flow.spec.ts`:
  1. pick Assistant on the view tab;
  2. reload;
  3. check that the edit tab shows Assistant;
  4. check that `properties.json` holds `"kind": "assistant"`.

  Plus a two-tab beat (the view and edit tabs both mounted, where the radio
  groups must not merge).

## 7. The lane split, by FILE OWNERSHIP

| lane | commits | OWNS (exclusive) | touches (hunk-level, shared) |
|---|---|---|---|
| **K-A**: kind keystone, server | `b09ca46bb` server | NEW `characters/kind.rs` (+ `characters/mod.rs` line), `vault_overlay.rs`, `db/vault_read_overlay.rs`, `db/vault_character_write.rs`, `db/vault_character_update.rs`, `db/character_vault.rs`, `db/characters.rs` (`MANAGED_FIELDS`), `db/characters_read.rs`, `services/character_enrichment.rs`, NEW census; oracle: NEW `cases/character-kind.ts`, `cases/vault-json-parsers.ts`, `cases/characters-mutations.test.ts` (kind rows) | `api/characters.rs` (the accepted-keys list + validator only); `schema-key-order.json` and `qtap-export.schema.json` → HANDOFF (R-K5) |
| **K-B**: Agent Mode + tool rules | `96156566b`, `6121f59b9` rule 6 | `services/agent_mode.rs`, `tools/native_tool_prompt.rs` (whole, at the pin; R-K2), `services/pseudo_tool.rs`, `services/native_tool_loop.rs`, `services/turn_extras.rs`; oracle: `pseudo-tool-prompts.ts`, `native-tool-loop-tier3.test.ts` + `fixtures/native-tool-loop-tier3.json`, `turn-extras.ts`, NEW `agent-mode-flavor.ts` | `services/orchestrator.rs` (four hunks, R-K4), `one_shot_loop.rs` / `brahma_console/orchestrator.rs` / `help_chat/orchestrator.rs` (3 args each), `api/salon.rs:318`, `services/chat_admin.rs:309`; `orchestrator-tier3.test.ts` (in-case assistant plant) |
| **K-C**: generators | `3ae4cf582`, `7ff5fda89` bullet | `generators/field_semantics.rs` (whole at the pin; R-K1), `wizard_prompts.rs`, `wizard.rs`, `ai_import.rs`, `external_prompt.rs`, `optimizer.rs`; oracle: `generators-field-semantics.ts`, `generators-wizard-prompts.ts`, `character-optimizer-prompts.ts`, `ai-import-assembly.ts`, the four generator tier-3 cases | none |
| **K-D**: SPA | `b09ca46bb` client | NEW `character-kind-card.ts` (+spec), NEW `core/character-kind.ts` (+ chokepoint spec), `view/character-detail.ts`, `edit/character-edit.ts`, e2e beat | `core/core-contract.ts` (three interface lines) |

**Stacking:** `kind.rs` is the shared contract. K-A lands it as its FIRST
commit, and K-B and K-C branch from that commit. This is the P4.D279 keystone
pattern. The named contract (§R) is:

```rust
characters::kind::{
    CharacterKind,
    DEFAULT_CHARACTER_KIND,
    kind_or_default(&Value) -> CharacterKind,
    resolve_character_kind(Option<&Value>) -> CharacterKind,
    is_assistant_character,
    AgentModeFlavor { Research, InCharacter },
    ToolRulesVariant { Assistant, Roleplay },
    GeneratorDirectionKey { Stakes, Task },
    KindPolicy,
    kind_policy,
    character_kind_policy(Option<&Value>),
}
```

K-B's tier-3 rows need K-A's read overlay to carry `kind` (the assistant
plant), so K-B's assistant rows wait on K-A. Its roleplay rows do not:
absent `kind` already resolves to roleplay. K-D needs only the wire, so it is
parallel and can be gated with stand-in data.

**Collisions to flag:**

- `generators/field_semantics.rs`: `ba6c8fe0f` (prompt-trust lane). R-K1
  recommends K-C owns the file WHOLE at the pin's bytes, taking `ba6c8fe0f`'s
  four rewrites. The prompt-trust lane keeps extraction, recall,
  consolidation and the context builder.
- `tools/native_tool_prompt.rs`: the 4.12.1 window-first lane. R-K2
  recommends K-B owns it. The window lane owns only `simple_json_prompt.rs:186`
  (the same sentence, in its own wording).
- `services/orchestrator.rs`: the Salon lanes (`db6190b44` established facts,
  `9eab8d2da` hand-raise yield, `604a082c4` stream frames, `29c1e2690` image
  budget). K-B touches ONLY `:1437-1445`, `:2217`, `:2373`, `:2445-2450` and
  the new INFO by `:2848`.
- `one_shot_loop.rs`, `brahma_console/orchestrator.rs`,
  `help_chat/orchestrator.rs`: `604a082c4` (stream frames). K-B's edits are
  argument-only, so the unifier resolves them trivially.
- `brahma_console/orchestrator.rs`: `c539c82e0` adds Brahma lookup tools, but
  its hunks are in `tool-executor.ts`, `tools/index.ts`,
  `plugin-tool-builder.ts` and `one-shot.service.ts`, NOT in the Brahma
  orchestrator. Brahma stays `research` + `assistant`, so `submit_final_response`
  stays on Brahma's slate. There is no semantic collision.
- `qtap-export.schema.json` (`c539c82e0` also edits it) and
  `schema-key-order.json`: one owner each (R-K5).
- `builtin_prompt_templates.json` / `.rs`: `d0201575a` + `7ff5fda89`'s
  sample-prompt half belong to the prompt-templates lane, not this area.
  `MODERN_ASSISTANT.md` tells the operator to set the kind to Assistant,
  which is a documentation dependency only.
- `context_builder` / `inform_block`: `c69a08882` is spec phase 5 and goes to
  the context-builder / inform lane, not this area.
- `api/characters.rs`: any other lane adding PUT keys in this span. Measure
  at planning.

## 8. Tiering

- **Tier 1 (must land):**
  - K-A: `kind.rs` + the four erasure writers + the scaffold + read +
    materialize + PUT + list + census.
  - K-B: flavour + preambles + `submit_offered` + the pinned callers + rule 6.
  - K-C: all four generators + the 2500 budget + the log lines.
  - K-D: the card on both tabs.
- **Tier 2:** the R-K3 INFO lines; the in-character tier-3 rows; the e2e
  two-tab beat.
- **Tier 3 / NO-PORT:** the `isAssistantCharacter` parity, docs-only stamps.

## 9. Fixtures

- No builder changes are required. Plant the assistant kind **in-case**,
  through a keyed SQLITE_PATH connection, by rewriting the vault's
  `properties.json` document row (memory note `jest-oracle-in-case-plants`).
  The content-addressed doc store needs a proper insert; see
  `recall-lane-oracle-gotchas`.
- `fixtures/native-tool-loop-tier3.json` gains `flavor` per row. It is
  committed, so edit it and its builder's corpus together.

## 10. Rulings owed to the human

- **R-K1: owner of `generators/field_semantics.rs`.** Recommend **K-C owns it
  whole at the pin** (`ba6c8fe0f` + `3ae4cf582` + `7ff5fda89`). The
  prompt-trust lane would then consume rather than edit it. The alternative,
  the prompt-trust lane owning it with a HANDOFF to K-C, serializes two
  lanes on one file.
- **R-K2: owner of `tools/native_tool_prompt.rs`.** Recommend **K-B, at the
  pin's bytes**, including 4.12.1's rule 6. The window-first lane takes only
  `simple_json_prompt.rs`.
- **R-K3: port the three agent-mode INFO lines** that v5 never logged
  (`[AgentMode] Resolved agent mode`, `Injected agent mode instructions`,
  `Agent mode max turns reached, forcing final response`), now with `flavor`.
  Recommend **PORT in K-B, capture-pinned**. Without them the flavour a turn
  ran under is invisible in v5's logs, and this is where dogfooding would
  look first. The alternative is to record them as a residue.
- **R-K4: hunk ownership of `services/orchestrator.rs`.** Recommend K-B owns
  only the named hunks, sibling Salon lanes rebase, and the unifier resolves.
- **R-K5: one re-dump of `schema-key-order.json`** (the keystone or D23
  lane, covering every schema move in the span) and **one re-vendor of
  `qtap-export.schema.json`** (the help / docs lane). K-A HANDOFFs both.
  Recommend this.
- **R-K6: a SPA kind module + a chokepoint spec** (v4's regex verbatim over
  `apps/web/src`). Recommend yes, so the SPA has one reader, as v4 does.

## 11. Dogfood rows (when the copy rule allows; see `dogfood-waits-for-catchup`)

1. **Before the first v5 write, on the Friday copy:** count `properties.json`
   documents carrying `"kind"`, and how many say `assistant`. This measures
   the erasure hazard's reach.
2. On a v4-written assistant character, change talkativeness in v5. Then:
   - `properties.json` must still say `assistant`;
   - the file must be byte-equal to v4's after the same write.
3. On an assistant character with Agent Mode on, run a Salon turn. Expect
   the MANDATORY preamble, `## Agent Mode Instructions`, and
   `submit_final_response` on the slate (LLM Inspector).
4. Do the same on a roleplay character. Expect:
   - `## Using tools` + `## Acting in the scene`;
   - no `submit_final_response` on the slate;
   - at max turns, the in-character force-final sentence.
5. Run a Brahma or Help chat turn and confirm it still gets the MANDATORY
   preamble and the research text.
6. On an existing assistant character:
   - run the Wizard's system prompt: expect the task direction and up to
     2500 tokens;
   - run the External Prompt: expect the DEBUG line with `kind=assistant`;
   - run the Optimizer: expect no flattening look-for.
7. Click the Aurora card on the view tab with the edit tab open beside it.
   The radio groups must stay independent, the toast must read the
   kind's sentence, and a reload must keep the choice.
