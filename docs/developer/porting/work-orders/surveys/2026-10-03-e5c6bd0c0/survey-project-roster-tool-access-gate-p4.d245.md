# Survey — P4.D245: `9753d0eb2`'s TOOL-ACCESS half (the project roster as a tool-access gate: the chokepoint, the doc path resolver, the three doc tools, `search_scriptorium`, the shared wardrobe tiers + the operator's equip, `project_info`'s formatter)

**Date:** 2026-10-03 · **v4:** `e5c6bd0c0` (the hunks are all `9753d0eb2`'s; `e5c6bd0c0` touches none of these files — `git diff 9753d0eb2 e5c6bd0c0 -- lib/ app/` lists only the bug-175/176 paths) · **v5 main:** `538fcbf65` · **Kind:** read-only measurement — `git show 9753d0eb2` / `git show e5c6bd0c0:<path>` of every tool-access hunk and the new module + test; reads of every v5 twin; `ggrep` of every caller of the shared-tier resolver, the project-tier resolvers and the enumerator on both sides; reads of the five harness families and their fixture builders. Nothing was built or run in either repo.

## The finding in one line

The roster becomes a gate in **seven v4 places** (two in the resolver, three in the doc tools, one in `search_scriptorium`, one in the shared wardrobe tier) through **one** chokepoint, and v5 has **no part of it** — but v5 carries the accessible-pool enumeration TWICE (`doc_edit/path_resolver.rs:408` `collect_accessible_mount_point_ids` and `tools/doc_edit/shared.rs:713` `get_accessible_mount_points` are hand-twins where v4 has ONE function), so a port that gates one copy leaves enumeration and resolution disagreeing (the bug-153 class) unless the two are folded first; **every existing differential family is NEUTRAL at the target except `search_tools_equivalence` (two `project_info` rows)**, because every minted doc/wardrobe fixture is rebuilt at the pin with the flipped create default (`allowAnyCharacter ?? true`) and so admits every character — the gate has no red-first arm anywhere today and needs a NEW real-DB family with explicitly closed projects; and the ledger/brief claim that `doc_opacity_equivalence` "needs v4's `canCharacterParticipate` mock" is **refuted** — v5's oracle runs v4's REAL repositories, so the real method answers.

---

## §A The v4 hunks (quoted from `9753d0eb2`; line numbers at `e5c6bd0c0`)

### A1 The chokepoint — NEW `lib/projects/roster-access.ts` (59 lines)

```ts
import { getRepositories } from '@/lib/repositories/factory';
import { logger } from '@/lib/logger';

export async function projectRosterAdmits(
  projectId: string | null | undefined,
  characterId: string | null | undefined,
): Promise<boolean> {
  if (!projectId || !characterId) return true;
  const allowed = await getRepositories().projects.canCharacterParticipate(projectId, characterId);
  logger.debug('[ProjectRoster] Tool access check', { projectId, characterId, allowed });
  return allowed;
}

export async function rosterGatedProjectId(
  projectId: string | null | undefined,
  characterId: string | null | undefined,
): Promise<string | undefined> {
  if (!projectId) return undefined;
  return (await projectRosterAdmits(projectId, characterId)) ? projectId : undefined;
}

export const PROJECT_ROSTER_REFUSAL =
  "You are not on this project's character roster, so its files are closed to you. Ask the user to add you to the roster in the project's Characters card.";
```

- JS truthiness on BOTH guards: an empty-string project or character admits with NO line; the DEBUG fires only when both are truthy.
- The logger is the ROOT `@/lib/logger` (no service name) — NOT the resolver's `DocEdit:PathResolver`. Fields in order `projectId, characterId, allowed`.
- The module's doc comment is the *why* to carry (`:1-20`): the roster gates TOOL access to project files and the project wardrobe and **nothing else** — chat membership, project instructions, automatic knowledge and equipped garments are untouched.

**The policy** (`lib/database/repositories/projects.repository.ts:186-206`, body UNCHANGED by the commit, doc comment rewritten):

```ts
async canCharacterParticipate(projectId: string, characterId: string): Promise<boolean> {
  return this.safeQuery(
    async () => {
      const project = await this.findById(projectId);
      if (!project) return false;
      if (project.allowAnyCharacter) return true;
      return project.characterRoster.includes(characterId);
    },
    'Error checking character participation',
    { projectId, characterId },
    false
  );
}
```

Fail-closed in **two** layers: `this.findById` is the store-backed `applyOverlayOne(await this._findById(id))` (`store-backed.repository.ts:65-67`); `_findById` is itself a fallback `safeQuery` (`Error finding entity by ID`, `{collection: 'projects', id}`, → `null` → `false` with NO outer line); an overlay throw (store missing/unreadable) is caught by the OUTER `safeQuery` → ERROR `Error checking character participation` `{collection: 'projects', projectId, characterId, error}` → `false`. Both lines go through the `Repository` logger (v5 maps it to `quilltap::db`, `db/fallback.rs:17-19`).

### A2 The doc path resolver (`lib/doc-edit/path-resolver.ts`, logger `createServiceLogger('DocEdit:PathResolver')`, `:32`)

**Enumeration** — `collectAccessibleMountPointIds` (`:324-386`), the standard arm only (the operator-override arm `:328-337` and the pre-built-pool arm `:341-347` return BEFORE it and are NOT gated):

```ts
  const vaultsVisible = !context.hideCharacterVaults;
  const projectId = await rosterGatedProjectId(context.projectId, context.characterId);
  if (context.projectId && !projectId) {
    logger.debug('Path resolver: character off project roster — project tier withheld', {
      projectId: context.projectId,
      characterId: context.characterId,
    });
  }
  const pool = await resolveTieredMountPool(
    { characterId: context.characterId, characterIds: context.characterIds, projectId },
    { includeParticipants: vaultsVisible },
  );
```

**Project scope** — `resolveProjectPath` (`:591-611`), AFTER the existing missing-project guard (which itself WARNs `project scope requires projectId in context` at `:597` — a line v5 has never carried, §B2) and BEFORE the `projects.findById` read:

```ts
  if (!(await projectRosterAdmits(context.projectId, context.characterId))) {
    logger.info('Project scope refused: character off project roster', {
      projectId: context.projectId,
      characterId: context.characterId,
    });
    throw new PathResolutionError(PROJECT_ROSTER_REFUSAL, 'ACCESS_DENIED');
  }
```

The project-scope gate applies **regardless of `operatorOverride` / `mountPool`** — it sits in `resolveProjectPath`, not in the collector. `getAccessibleMountPoints` (`:900-937`) routes through the SAME `collectAccessibleMountPointIds`, so the DEBUG fires on enumeration too (and on every `resolveDocumentStorePath` that consults the pool).

### A3 The doc tools

- `handleOpenDocument` (`lib/tools/handlers/doc-edit/document-ui-handlers.ts:73`; hunk `:126-136`), the new-blank branch only:
  ```ts
    const targetScope =
      context.projectId && (await projectRosterAdmits(context.projectId, context.characterId))
        ? 'project'
        : 'general';
    try {
      const resolved = await resolveDocEditPath(targetScope as DocEditScope, filePath, {
        projectId: context.projectId,
        characterId: context.characterId,
      });
  ```
  Two changes: the scope choice is gated, AND `characterId` now reaches the resolver (before, only `projectId`). For an admitted character the chokepoint therefore runs TWICE (here, then in `resolveProjectPath`) — two `[ProjectRoster]` DEBUG lines per new-blank open.
- `handleGrep` (`text-handlers.ts:588`; hunk `:844-848`), the legacy on-disk project walk only:
  ```ts
  if (
    !input.mount_point &&
    context.projectId &&
    (await projectRosterAdmits(context.projectId, context.characterId))
  ) {
    const officialMount = await resolveOfficialProjectMount(context.projectId);
  ```
  The gate is evaluated BEFORE `resolveOfficialProjectMount`, short-circuited by the two earlier conjuncts.
- `handleListFiles` (`text-handlers.ts:897`; hunk `:1070-1075`), the project branch:
  ```ts
  // Roster-gated: a character off the project roster lists no project files.
  if (
    shouldIncludeProject &&
    context.projectId &&
    (await projectRosterAdmits(context.projectId, context.characterId))
  ) {
  ```
  Short-circuit: no chokepoint call (no DEBUG) when `shouldIncludeProject` is false.

### A4 `search_scriptorium` (`lib/tools/handlers/search-scriptorium-handler.ts:165-173`)

```ts
        // The project tier is roster-gated (lib/projects/roster-access.ts).
        pool = await resolveTieredMountPool(
          {
            userId: context.userId,
            characterId: context.characterId,
            projectId: await rosterGatedProjectId(context.projectId, context.characterId),
          },
          { requireOwnership: true },
        )
```

Only the standard arm; the pre-built-pool and operator-surface arms are untouched.

### A5 The shared wardrobe tiers (`lib/wardrobe/shared-tiers.ts`, root `logger`)

`resolveSharedWardrobeTiersForChat(chatId, characterId, options: SharedWardrobeTierOptions = {})` (`:70-79`) now calls a NEW private `resolveProjectTierForChat` (`:82-110`) in place of `resolveProjectMountPointIdsForChat`:

```ts
async function resolveProjectTierForChat(chatId, characterId, options): Promise<string[]> {
  if (!chatId) return [];
  let projectId: string | null = null;
  try {
    const chat = await getRepositories().chats.findById(chatId);
    projectId = chat?.projectId ?? null;
  } catch (error) {
    logger.warn('[Wardrobe] Project lookup for chat failed', {
      chatId,
      error: error instanceof Error ? error.message : String(error),
    });
    return [];
  }
  if (!projectId) return [];
  const gated = options.operator ? projectId : await rosterGatedProjectId(projectId, characterId);
  if (!gated) {
    logger.debug('[Wardrobe] Character off project roster — project wardrobe withheld', {
      chatId, projectId, characterId,
    });
    return [];
  }
  return resolveProjectMountPointIds(gated);
}
```

`SharedWardrobeTierOptions { operator?: boolean }` (`:52-59`) with the doc "The human operator is choosing on the character's behalf (the Salon's outfit dialog), so the project roster does not apply. Character tool calls never set this." `chats.findById` is `_findById` (`chats.repository.ts:144-146`) — a FALLBACK read, so the new WARN's `catch` is **unreachable on a repository failure** (it logs `Error finding entity by ID` `{collection: 'chats', id}` and answers `null` → `[]` silently past it) — the P4.90 / P4.D225 "catch behind a fallback `safeQuery`" class.

**The operator:** `handleEquipSlot` (`app/api/v1/chats/[id]/actions/outfit.ts:208`; hunk `:221-222`): `resolveSharedWardrobeTiersForChat(chatId, characterId, { operator: true })` + the comment "The operator is dressing the character, so the project roster does not apply."

**Callers at the target** (`git grep 'resolveSharedWardrobeTiersForChat('`): `outfit.ts:222` (operator) and the seven wardrobe tool handlers — `wardrobe-{archive:63,list:83,read:123,take-off:69,update:70,wear:72}-handler.ts` with `context.characterId`, `wardrobe-create-handler.ts:219` with `targetCharacterId` (the gate keys on the TARGET). No other caller.

**Untouched (stay UNGATED):** `resolveProjectMountPointIdsForChat` (`tiered-mount-pool.ts:240`, still used by `image-generation-handler.ts:963`) and every direct `resolveProjectMountPointIds` / `sharedWardrobeTiersForCharacter` caller — `outfit.ts:141` (the GET), `participants.ts:219`, `chats/route.ts:589,1362`, the avatar / scene-state / story-background / wardrobe-announcement jobs, `apply-chat-merge.ts:166`, `scenario-builder/mount-pool.ts:90`, `apply-outfit-selections.ts`, `avatar-prompt.ts`, `resolve-equipped.ts` — "equipped garments are unaffected".

### A6 `project_info`'s formatter (`lib/tools/handlers/project-info-handler.ts:257-268`)

```ts
        info.allowAnyCharacter
          ? 'Project files and wardrobe: open to every character'
          : info.characterRoster.length > 0
            ? `Project files and wardrobe: roster only (${info.characterRoster.map(c => c.name).join(', ')})`
            : 'Project files and wardrobe: roster only (roster is empty)',
```

ONE line where there were up to two (`Characters: …` / `No characters in roster`, then `(Any character can participate)` or nothing). `allowAnyCharacter` is now checked FIRST, so an open project's roster names no longer appear in the formatted string at all. The handler's data (`executeProjectInfoTool`) and the tool definition are unchanged.

### A7 The tests the commit touched (oracle shapes)

- NEW `lib/projects/__tests__/roster-access.test.ts` (135 lines) — mocked repositories + mocked `getGeneralMountPointId`, the REAL tiered pool / resolver / shared tiers. Nine `it`s: rostered admitted / stranger refused; Allow Any admits a stranger; `(null, OFF)` and `('proj-1', null)` admit; `rosterGatedProjectId` on/off; `getAccessibleMountPoints` lists the project store for ON only; `resolveDocEditPath('project', 'plan.md', {projectId, characterId: OFF})` rejects `PathResolutionError` with `code: 'ACCESS_DENIED'`; the same for ON resolves `{mountPointId: PROJECT_STORE.id}`; the shared wardrobe OFF `[]` / ON `[PROJECT_STORE.id]`; `{ operator: true }` OFF → `[PROJECT_STORE.id]`.
- One added mock line each in `lib/doc-edit/__tests__/path-resolver-opacity-enumeration.test.ts:124` (`canCharacterParticipate: jest.fn().mockResolvedValue(true),` inside `projects`), `path-resolver-opacity-group-stores.test.ts:100` (`projects: { canCharacterParticipate: jest.fn().mockResolvedValue(true) },`) and `__tests__/unit/lib/tools/handlers/search-scriptorium-handler.test.ts:65` (same shape) — needed ONLY because those suites mock `getRepositories()` wholesale.

### A8 What the commit did NOT do

- No change to `resolveTieredMountPool`, `resolveProjectMountPointIds`, `resolveProjectMountPointIdsForChat`, the knowledge injector, the Scenario Builder pool, or any equipped-outfit / avatar / announcement read.
- No change to `canCharacterParticipate`'s body, the `ProjectPropertiesSchema` read default (`project.types.ts:64` `.default(false)` stays), the opacity covenant, `findEnabledMountPointByRef`, or the out-of-scope refusal sentence (`path-resolver.ts:513-515`).
- No gate on the doc tools' other handlers (read/write/move/blob) beyond what the resolver and the enumerator now do for them.
- No new tool, no tool-definition text change, no dispatch verb, no DDL.

---

## §B v5's twins (`main` `538fcbf65`)

### B1 The chokepoint — ABSENT

No `roster` / `can_character_participate` caller exists outside `db/projects.rs` (`ggrep -rn can_character_participate crates` → the definition alone). The policy twin is `db/projects.rs:364-380` `ProjectsRepository::can_character_participate(&self, project_id, character_id) -> Result<bool, OverlayError>` — `find_by_id` (hydrated, `store_backed.rs:203-206`) → `Ok(false)` on a miss, `allowAnyCharacter` `.unwrap_or(false)`, else roster contains. **It PROPAGATES both failure kinds** (`OverlayError::Db` from the slim read, `OverlayError::Unavailable` from the overlay) where v4 answers `false` with a line. The constructor `ProjectsRepository::new(main, mount)` (`:252`) needs BOTH connections — every gate site below has both. P4.D246 edits this file's doc comment only; the body is stable for this lane.

**Proposed home:** a flat `crates/quilltap-core/src/project_roster_access.rs` + `pub mod project_roster_access;` in `lib.rs` (alphabetically after `pub mod pricing;` / before `pub mod pronoun_gender;` — the `wardrobe_tiers.rs` precedent: v4 `lib/wardrobe/shared-tiers.ts` → flat `wardrobe_tiers.rs`; there is no `crate::projects` module, and a `projects/` directory would sit beside `db::projects` for one file). Its default tracing target `quilltap_core::project_roster_access` stands for v4's root logger. The fail-closed mapping needs ONE new `db/fallback.rs` home (`Error checking character participation` — §D3); the `Error finding entity by ID` arm reuses the existing `find_by_id_or_none` emitter (`fallback.rs:43-58`).

### B2 The path resolver (`crates/quilltap-core/src/doc_edit/path_resolver.rs`, 1,245 lines)

- `PathResolutionContext` (`:41-68`): `project_id`, `character_id`, `character_ids`, `hide_character_vaults`, `mount_point`, `operator_override`, `mount_pool`. **`character_id` comes from** `DocEditToolContext.character_id` (`tools/doc_edit/shared.rs:72`) via `build_read_resolution_context` (`shared.rs:560-593`) / `build_write_resolution_context` (`:597-635`) — both KEEP it since P4.D200; the new-blank open DROPS it (`document_ui.rs:210-213`, §B3); Document Mode's `resolve_operator_doc_path` (`documents/mod.rs:166-195`) passes the param its callers give it, which (v4 `documents.ts:648-659` and v5 alike) is `Some` only on the `document_store` + `self` arm — so the project-scope gate is UNREACHABLE on the operator surface on both sides (measured; no ruling needed).
- `collect_accessible_mount_point_ids` (`:408-470`): operator arm (`:413-430`, DEBUG `count`), pre-built pool (`:436-438`), then the covenant arm (`:441-470`) passing `project_id: context.project_id.clone()` straight into `resolve_tiered_mount_pool` — **ungated**.
- `resolve_project_path` (`:732-826`): the `MissingContext` refusal (`:739-744`) with **no** v4 WARN (`project scope requires projectId in context` — pre-existing absent line), then the slim `find_official_mount_point_id_raw` read — **ungated**.
- Lines: every resolver line uses the module's default target `quilltap_core::doc_edit::path_resolver` (`doc_opacity_equivalence.rs:303` `RESOLVER_TARGET` filters on it); field names are v4's camelCase where the family compares them by name (`count`), snake on the P4.100 warns. The unit capture idiom is `crate::test_support::captured_with` (`test_support.rs:123-133`) with the level prefix (`warn.starts_with("WARN ")`, `path_resolver.rs:973`).
- **The twin:** `tools/doc_edit/shared.rs:713-760` `get_accessible_mount_points` re-implements the covenant arm by hand (its own `TierContext` + `resolve_tiered_mount_pool` + `flatten_tier_pool`), sharing only the pre-built-pool arm (`prebuilt_pool_accessible_ids`, `path_resolver.rs:478`). v4 has ONE function. Callers: `text.rs:805` (grep), `text.rs:1199` (list), `blob.rs:112,157`.
- **The in-module tests a gate perturbs:** `warn_fixture()` (`:901-932`) has NO `projects` table; `out_of_scope_store_warns_and_denies_with_v4s_sentence` (`:934-984`) passes `project_id: p-1` + `character_id: c-1`, so a ported gate reads a missing table → the `Error finding entity by ID` ERROR + `false` → `r-1` withheld; its assertions still hold (the stranger store `s-1` is out of scope either way) but the fixture now models a broken instance. `a_resolved_store_logs_neither_warn` (operator override — no gate) and `an_enabled_store_resolves_on_the_pool_path_with_no_warn_at_all` (`:1214-1245`, no character → admitted) are unaffected.

### B3 The doc tools (`crates/quilltap-core/src/tools/doc_edit/`)

- `document_ui.rs:198-223` — the new-blank branch: `target_scope = if ctx.project_id.is_some() { Project } else { General }` (`:205-209`) — ungated; `blank_ctx = PathResolutionContext { project_id, ..Default::default() }` (`:210-213`) — **no `character_id`**, v4's pre-fix shape exactly.
- `text.rs:740` `handle_grep` — the legacy walk (`:884-887`): `project_id.filter(|pid| addressing.mount_point.is_none() && resolve_official_project_mount(...).is_none())` — ungated; the enumeration (`:805-815`) passes `character_id` through `get_accessible_mount_points` (the twin, §B2).
- `text.rs:1130` `handle_list_files` — the project branch (`:1267-1268`): `project_id.filter(|_| include_project)` (`include_project` `:1173`) — ungated.

### B4 `search_scriptorium` (`crates/quilltap-core/src/tools/search.rs`)

`build_pool_context` (`:610-670`): the standard arm (`:645-660`) passes `project_id: context.project_id.clone()` into `resolve_tiered_mount_pool` with `require_ownership: true` — ungated; `main` + `mount` are in hand; `context.character_id` is the gate's key. The pre-built-pool (`:628-638`) and `operator_surface` (`:639-644`) arms are v4's untouched arms.

### B5 The shared wardrobe tiers

- `wardrobe_tiers.rs:82-100` `resolve_shared_wardrobe_tiers_for_chat(main, mount, chat_id: &str, character_id: &str) -> SharedWardrobeTiers` — no options; the project half is `tools/wardrobe_shared.rs:63-72` `resolve_project_mount_point_ids_for_chat`, which swallows a chat-read `Err` SILENTLY (`_ => return Vec::new()`) where v4's `_findById` logs; `resolve_project_mount_point_ids` (`:47-57`) `unwrap_or_default()`s the link read silently where v4's `findByProjectId` (a 4-arg `safeQuery`, `project-doc-mount-links.repository.ts:62-74`) logs `Error finding links by project ID` — both pre-existing absent lines, outside this commit.
- The chat read to use: `chats_read::find_by_id_or_none` (`db/chats_read.rs:395-397`) — already v4's `_findById` shape through the fallback home.
- **Callers (nine, classified):** character-tool (no operator) — `tools/wardrobe_archive.rs:80`, `wardrobe_update.rs:155`, `wardrobe_list.rs:164`, `wardrobe_wear.rs:163`, `wardrobe_take_off.rs:153`, `wardrobe_read.rs:233`, `wardrobe_create.rs:269` (with `&target_character_id` — v4's `targetCharacterId`, matched); **operator** — `api/chat_outfits.rs:536-537` inside `chat_equip` (`:522`, "v4 `handleEquipSlot`"). No web-crate caller (`ggrep` over `crates/quilltap-web/src` finds none).
- The six UNGATED callers of `resolve_project_mount_point_ids_for_chat` (`generate_image.rs:1684`, `story_background_job.rs:1353`, `chat_participants.rs:1369`, `character_avatar_job.rs:288`, `aurora_notifications.rs:344`, `chat_create.rs:2200`) are v4's ungated sites (§A5) and must stay so.

### B6 `project_info` (`crates/quilltap-core/src/tools/project_info.rs:298-313`)

The OLD two-line shape verbatim: `Characters: {names}` / `No characters in roster` (`:303-310`), then `(Any character can participate)` when `allowAnyCharacter == Some(true)` (`:311-313`). The data side is unchanged on both sides.

### B7 Is v5 bitten?

Yes, wholly — there is no partial port. On a project with Allow Any Character OFF, every character in its chats reaches its files, lists, greps, searches and wears from its wardrobe exactly as before the v4 change; `project_info` describes the roster as participation. No dispatch verb is needed (`api/types.rs` FROZEN — untouched by this lane).

---

## §C The differential families

### C1 Existing families — by reading, at `e5c6bd0c0` on UNPORTED main

| family (case / builder) | touches | at the target | why |
|---|---|---|---|
| `search_tools_equivalence` (`search-tools-readwrite.test.ts` + `search-tools-search.test.ts` / `build-search-tools-fixture.ts`) | `project_info` formatter, `search_scriptorium` | **RED on two rows** — `pi_get_info_rich` (allowAny `true`, roster [A,B] → `Project files and wardrobe: open to every character`; the `Characters:` line and the parenthetical gone) and `pi_get_info_minimal` (allowAny `false`, roster [] → `…roster only (roster is empty)`); `resultJson` unmoved, `formatted` moved | the builder passes `allowAnyCharacter` EXPLICITLY on both creates (`:249-266`), so the flip does not reach it; every search case uses the OPEN project (`spec.projectId`) — the search gate admits, neutral |
| `doc_opacity_equivalence` (`doc-opacity.test.ts` / `build-doc-opacity-fixture.ts`) | resolver, enumerator, grep/list | NEUTRAL | REAL repositories (`doc-opacity.test.ts:16-19,130-135`) — **no `canCharacterParticipate` mock is needed** (the ledger/brief claim refuted); the project is created WITHOUT the flag (`:264-267`) so the pin's `?? true` bakes it OPEN; no DEBUG/INFO fires; the resolver-line comparison (pool ops only, `doc_opacity_equivalence.rs:663-700`) is unmoved |
| `doc_enum`, `doc_ui`, `doc_text`, `doc_blob`, `doc_fm`, `doc_fs`, `doc_edit_path_resolver` | the three doc tools, the resolver's project scope | NEUTRAL | every builder creates its project(s) without the flag (`build-doc-{enum:176,ui:178,text:173,blob:151,fm:181,fs:201,238}`, `build-doc-edit-path-resolver-fixture.ts:187,207,219`) → OPEN at the pin; `doc_ui`'s new-blank open now calls the chokepoint twice and resolves `project` as before |
| `wardrobe_tools_equivalence` (`wardrobe-tools.ts` / `build-wardrobe-tools-fixture.ts`) | the seven wardrobe tools | NEUTRAL | the fixture has NO project (`grep projects` → nothing) |
| `quilltap-web` `wardrobe_routes_equivalence` (committed `wardrobe-routes-{main,mount}.db`; corpus `wardrobe-routes.json`) | `chat_equip` | NEUTRAL — and the operator's **M-proof candidate** | `eq_project_tier` (corpus `:644-653`) equips the project-tier `pScarf` for Aria in `chat1` (projectId `d1000000-…01`); the committed pair predates the flip and was created without the flag (`build-wardrobe-routes-fixture.ts:206-209`) and without a roster add (repository-level `chats.create`), so Aria is very likely OFF the roster in a CLOSED project — the operator bypass keeps v4's answer unmoved; dropping `operator: true` in v5 would red it. **MEASURE the committed pair's `properties.json` first** (a scratch read through v5's `ProjectsRepository`); RUN only — the file is the web crate's |
| `tiered_mount_pool_equivalence`, `scenario_builder_mount_pool_equivalence`, `knowledge_injector*` | the pool, the SB pool, automatic knowledge | NEUTRAL | ungated by v4 (§A8) |
| `fallback_home_guard` | the new home literal | grows by one literal (§D3) | — |

**Root-logger captures:** no oracle case that drives a doc/wardrobe/search tool in a project records `@/lib/logger` (the 18 cases that doMock it are danger/refusal/host/pascal/help shapes), so the new `[ProjectRoster]` DEBUG reaches no existing comparison. A full-sweep red outside the table is a finding, not an expectation.

### C2 What is missing — the red-first arms do not exist anywhere

Because every minted fixture now bakes projects OPEN, **no existing family can go red on the gate at the target**. The order needs:

1. **A NEW real-DB family** `project_roster_access_equivalence` (`harness/oracle/cases/project-roster-access.test.ts` + `harness/oracle/fixtures/build-project-roster-access-fixture.ts` + `project-roster-access.json`), cloned from the `doc_opacity` recipe (the whole DB stack doMocked to the REAL modules + the cipher binding; minted pair; the resolver-line recorder) **plus** a root-`@/lib/logger` recorder for the `[ProjectRoster]` and `[Wardrobe]` lines (the `danger-routing.test.ts` doMock idiom). The builder sets `allowAnyCharacter` EXPLICITLY on every create (a `false`/roster-[Ada] CLOSED project, a `true` OPEN project, a CLOSED project whose official store row is deleted after the build — the `Unavailable` plant) so it is vintage-neutral. It mirrors `roster-access.test.ts`'s nine arms on a real DB and adds the compositions: enumeration ON/OFF; project scope ON/OFF/no-character; `document_store` by NAME of the closed project's own store for the stranger (§D4's misleading sentence, pinned); `doc_list_files` with no scope and with `scope: 'project'`; `doc_grep` (pool half); the shared tiers OFF/ON/operator/OPEN/no-project/missing-chat; one `wardrobe_list` tool op OFF vs ON; the fail-closed `Unavailable` arm (`Error checking character participation`). The chokepoint rows import `@/lib/projects/roster-access` dynamically and emit `absent` at the baseline pin, so the case runs at BOTH pins.
2. **`search_tools` grows a THIRD project** (CLOSED, `allowAnyCharacter: false`, roster [charB], one project-store doc + chunk): `pi_get_info_closed` (the third formatter branch, `roster only (<B's name>)` — no case reaches it today) and two search cases, `search_closed_offroster` (characterId A — the project chunk withheld) and `search_closed_onroster` (characterId B).
3. **The grep legacy-walk gate** is observable only on a project with NO official mount and an on-disk `<filesDir>/<projectId>` — the `doc_fs` recipe's legacy project + scratch root is the home (a CLOSED legacy project and an off-roster grep arm).

### C3 Red-first, measured by reading

At `e5c6bd0c0` on unported main: `search_tools` RED on the two `pi_*` rows (and on the three new rows once added); the new family RED on every OFF-roster composition arm (v5 admits everyone) and NON-COMPILING on the chokepoint/operator rows (no module, no options arg) — so land the composition arms first against the existing API, record the red, then port, then add the direct rows. At `f6426e196`: the composition arms GREEN on unported main (v4 had no gate) and RED on ported main — the both-directions proof.

---

## §D Traps and rulings

- **D1 — The enumeration twin.** Gating `collect_accessible_mount_point_ids` alone leaves `get_accessible_mount_points` advertising the project store an open then refuses (the bug-153 failure shape, now on the roster axis), and its DEBUG would not reach the `quilltap_core::doc_edit::path_resolver` target the families filter on. Fold the twin FIRST as a neutral substrate commit — `get_accessible_mount_points` calls a `pub(crate)` collector with `operator_override: false` (v4's `getAccessibleMountPoints` passes no override) — proven neutral by `doc_opacity` + `doc_enum` at the BASELINE pin before the gate lands.
- **D2 — The create-default flip makes every flag-less off-roster arm VACUOUS** (P4.D246's survey §D2, confirmed by the coordinator: 34 of 39 v4 builders that call `repos.projects.create` never set `allowAnyCharacter`). Rebuilt at `e5c6bd0c0` they bake `allowAnyCharacter: true`, so the gate admits everyone and an off-roster arm proves nothing; rebuilt at `f6426e196` they bake `false` with an EMPTY roster, so a ported v5 withholds the project tier from EVERY character on every doc family at the baseline — a both-directions red for the wrong reason. **Every builder a family of this lane rebuilds is this lane's to set** (P4.D246 edits only `build-chat-create-capstone.ts`): (a) the existing builders whose projects are the ADMIT case — `build-doc-{opacity:264,enum:176,ui:178,text:173,blob:151,fm:181,fs:201,238}-fixture.ts`, `build-doc-edit-path-resolver-fixture.ts:187,207,219` — gain an EXPLICIT `allowAnyCharacter: true` (identical bake at both pins; their families stay neutral at both); (b) every OFF-roster arm lives on a project created with an EXPLICIT `allowAnyCharacter: false` + a curated `characterRoster` (the new family's builder, the third `search_tools` project, the `doc_fs` closed legacy project), each beside an explicit `true` project for the admit arm; (c) non-vacuity is proven by mutation — removing the gate at each site must red its off-roster arm (M1–M7 in the order). `build-search-tools-fixture.ts:249-266` already sets the flag on both creates; `build-wardrobe-tools-fixture.ts` has no project; the committed `wardrobe-routes` pair is not rebuilt.
- **D3 — Fail-closed has two v4 lines, one new home.** v5's `can_character_participate` returns `Err` for both kinds. The chokepoint maps `Err(OverlayError::Db(e))` → `Error finding entity by ID` `{collection: "projects", id: <projectId>, error}` (the existing home shape; v4's inner `_findById`) and `Err(OverlayError::Unavailable{..})` → ERROR `Error checking character participation` `{collection: "projects", projectId, characterId, error}` at `quilltap::db` (a NEW `db/fallback.rs` fn + its `HOME_MESSAGES` literal in `fallback_home_guard.rs:26-60`), both answering `false`. **Measure** whether `overlay::apply_overlay_one` can surface `OverlayError::Db` on a projects read (a mount-index read failure inside the overlay — v4 would log the OUTER line there); if it can, that arm is misattributed and is recorded as a named divergence or the split is moved.
- **D4 — A misleading refusal, v4-faithful (candidate v4 note).** An off-roster character naming the project's OWN linked store under `document_store` scope now falls to the existing out-of-scope refusal (`path-resolver.ts:513-515`, v5 `path_resolver.rs:644-665`): "It is not linked to this project, and it is not one of your own group's stores" — false for that store (it IS linked; the roster is the wall). Port faithfully, pin the sentence, record a candidate v4 bug in the lane record (the human files).
- **D5 — The wardrobe WARN is unreachable in v4.** `[Wardrobe] Project lookup for chat failed` sits behind `chats._findById`'s fallback; v5's reachable twin is `chats_read::find_by_id_or_none`'s `Error finding entity by ID {collection: chats}`. With `find_by_id_or_none` returning `Option`, there is no v5 `Err` arm to host the WARN — record it as a v4-unreachable line (the P4.90 `:692` precedent), not ported; a pin proves a failing chat read logs the home line and answers `[]`.
- **D6 — Line counts are the only witness for two hunks.** `handleOpenDocument` passing `characterId` to the resolver changes NO outcome for an admitted character (the resolver re-admits) — only the `[ProjectRoster]` count (two lines, not one). The family must compare the root-logger lines per op, or the `character_id` hunk is vacuous.
- **D7 — Evaluation order.** grep: the gate BEFORE `resolve_official_project_mount` (v4 `:844-848`); list: `include_project` BEFORE the gate (no DEBUG when false); open: `project_id` truthiness before the gate. A `filter` closure in the wrong order changes the line sequence.
- **D8 — The project-scope gate ignores the operator flags.** It belongs in `resolve_project_path`, not the collector, and fires even under `operator_override` / `mount_pool` when a character is present (v4 `:603`). Measured unreachable on Document Mode (§B2).
- **D9 — `wardrobe_tiers` takes `&str`.** v4's `!characterId` / `!chatId` are truthiness; an empty `&str` must admit / answer `[]` with no line.
- **D10 — Ledger refutations:** (a) "the doc-opacity oracles will need the same [`canCharacterParticipate` mock] at the target" — REFUTED (§C1 row 2); (b) "eight [callers] listed in `wardrobe_*` tools" — seven tools + `api/chat_outfits.rs` = eight, plus nine call sites counting `wardrobe_create`'s target key — the classification stands at 7 + 1. Everything else in the ledger row's (1)–(5) is confirmed by reading.
- **No ruling is needed before launch.** No dispatch verb; no schema; no cross-lane file collision beyond `lib.rs` (P4.D248 adds a core module too — a one-line union) and `db/fallback.rs` (only if P4.D248 grows a home — the unifier unions appends).

## §E Ownership

**Edits:** NEW `crates/quilltap-core/src/project_roster_access.rs`; `crates/quilltap-core/src/lib.rs` (one `pub mod` line); `doc_edit/path_resolver.rs`; `tools/doc_edit/{shared.rs,document_ui.rs,text.rs}`; `tools/search.rs`; `wardrobe_tiers.rs`; `tools/wardrobe_{archive,update,list,wear,take_off,read,create}.rs` (the call-site argument only); `api/chat_outfits.rs` (the operator argument only); `tools/project_info.rs`; `db/fallback.rs` (ONE new fn); harness: `fallback_home_guard.rs` (one literal), NEW `project_roster_access_equivalence.rs` + case + builder + spec, `search_tools_equivalence.rs` + `search-tools-{readwrite,search}.test.ts` + `build-search-tools-fixture.ts` + `search-tools.json`, the `doc_fs` family + case + builder + spec (the grep arm); the eight existing doc builders named in §D2(a) (the explicit `allowAnyCharacter: true` only); `docs/CHANGELOG.md` + `status-log.md` (append).
**Reads, never edits:** `db/projects.rs` (P4.D246 — the doc comment moves, the body is stable), `db/tiered_mount_pool.rs`, `db/chats_read.rs`, `tools/wardrobe_shared.rs` (unless a helper is unavoidable — prefer none), `documents/mod.rs`, `api/documents.rs`, the six ungated `resolve_project_mount_point_ids_for_chat` callers, `crates/quilltap-web/tests/wardrobe_routes_equivalence.rs` + its corpus (RUN only).
**Must not touch:** `build-chat-create-capstone.ts` (P4.D246's only builder); `api/projects.rs`, `services/chat_create.rs`, `api/salon.rs`, the `projects_*` / `chat_create_capstone` families (P4.D246); `apps/web/**` (P4.D247); `help/**`, `docs/v4/**` (P4.D246); `crates/quilltap-host/**`, `crates/quilltap-web/src/**` (P4.D248); `api/types.rs` (FROZEN); every other fixture builder.

## §F Designed reds (§R.5 sentence)

**P4.D245:** at `e5c6bd0c0` on unported main, `search_tools_equivalence` goes RED on exactly two rows (`pi_get_info_rich`, `pi_get_info_minimal` — the `formatted` string only); every other family this lane touches (`doc_opacity`, `doc_enum`, `doc_ui`, `doc_text`, `doc_blob`, `doc_fm`, `doc_fs`, `doc_edit_path_resolver`, `wardrobe_tools`, the web `wardrobe_routes`) is NEUTRAL at BOTH pins once its builder sets `allowAnyCharacter: true` explicitly (§D2(a)) — without that edit the flag-less builders bake `true` at the target (vacuous) and `false` + an empty roster at the baseline (ported v5 red everywhere for the wrong reason); the lane's NEW `project_roster_access_equivalence` and its grown `search_tools` / `doc_fs` rows are the red-first arms, RED on every off-roster composition until the port lands.

## §G 💸 for the dogfood pass

- **Pre-walk measurement first:** list the Friday copy's projects' `allowAnyCharacter` + roster (v4's pre-flip projects were created CLOSED, and v4's chat create auto-added participants until `9753d0eb2` — a character added to a project chat later is OFF-roster and now loses project files on v4 AND v5).
- On a CLOSED project with an off-roster character in a real Salon turn: `doc_list_files` shows no project store; `doc_read_file` with `scope: project` answers v4's roster sentence verbatim; a new blank `doc_open_document` lands in General; `search_scriptorium` returns no project hits; `wardrobe_list` omits the project `Wardrobe/` items; the log carries `[ProjectRoster] Tool access check` with `allowed: false` and the resolver's two lines.
- The same character added to the roster (the P4.D247 card) — every one of the above restored, no restart.
- The Salon outfit dialog equipping a project-tier garment for the OFF-roster character — succeeds (the operator bypass).
- `project_info` on the three project shapes — the three sentences byte-for-byte.
- Addressing the closed project's store by name under `document_store` — the misleading out-of-scope sentence (§D4), recorded for the candidate v4 note.
