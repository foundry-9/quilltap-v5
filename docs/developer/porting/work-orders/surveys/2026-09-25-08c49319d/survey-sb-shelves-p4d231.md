# SURVEY SB — `08c49319d` — fresh survey 2026-09-25 at that sha and on v5 main `2aed9a552`

v4 commit `08c49319d` "Scenario Builder on the scenario shelves", `4.10.0-dev.92`, 27 files
(+617 / −71). Parent `08c49319d~1` (`-dev.91`). Read only through `git show`; nothing
checked out. The drift-ledger row (`drift-ledger.md:216`) orders this as **P4.D230** (server + SPA),
with the `help/` half as **P4.D227**. **Two of that row's predictions are FALSE on
measurement (see §D.1 and §D.2).**

Files: `README.md`, `package.json`, `package-lock.json`, `packages/quilltap/package.json`
(version stamps, NO-PORT); `docs/CHANGELOG.md`, `docs/developer/features/scenario-builder.md`
(prose); four `help/` pages; 4 server source + 2 server tests; 7 client source + 1 client test;
`lib/query/keys.ts`.

---

## A. v4, per file (post-commit line numbers at `08c49319d`)

### A.1 `lib/scenario-builder/request-schema.ts` (whole schema, key order, :12-32)

```ts
export const scenarioBuildRequestSchema = z
  .object({
    mode: z.enum(['real', 'in-world']),
    location: z.string().trim().min(1).max(500),
    time: z.string().trim().min(1).max(200),
    details: z.string().trim().max(4000).default(''),
    connectionProfileId: UUIDSchema,
    projectId: UUIDSchema.nullish(),
    /** Cast character ids: New Chat selection, or the chat's participants. */
    characterIds: z.array(UUIDSchema).max(32).default([]),
    /** Groups named outright — the builder launched from a group's Scenarios card. */
    groupIds: z.array(UUIDSchema).max(32).default([]),                  // NEW :23
    /** Present when launched from the in-chat control. Adds the chat's own context. */
    chatId: UUIDSchema.nullish(),
    priorDraft: z.string().max(20_000).nullish(),
    revision: z.string().trim().max(2000).nullish(),
  })
  .refine((r) => (r.priorDraft == null) === (r.revision == null), {
    message: 'priorDraft and revision travel together',
  })
```

`groupIds` sits **between `characterIds` and `chatId`**. So the parsed OUTPUT always carries
`groupIds: []` right after `characterIds`, and the issue order puts `groupIds` issues after
`characterIds` issues. Its semantics are identical to `characterIds`: elements are checked first, then
`too_big` at 33+, and a non-array is an `invalid_type` (non-continuable, so it skips the refine).

### A.2 `app/api/v1/scenario-builder/route.ts` `handleBuild` (:35-201)

The refusal order is unchanged: JSON (:40), Zod (:44-48), profile (:52-56), tools off (:57-61), key
(:63-66), cast scoping (:68-86). **Then the new group block (:88-100) runs after the cast
scoping and before the chat check (:102-113):**

```ts
  // Named groups (the builder launched from a group's page): keep only ones that exist.
  const groupIds: string[] = []
  for (const id of [...new Set(body.groupIds)]) {
    try {
      const group = await repos.groups.findByIdRaw(id)
      if (group) groupIds.push(id)
    } catch (error) {
      logger.warn('Scenario Builder dropped an unreadable group id', {
        groupId: id,
        error: error instanceof Error ? error.message : String(error),
      })
    }
  }
```

- The loop dedups first (`new Set`, insertion order) and then checks existence. It has **no
  `.filter(Boolean)`**; that is not needed, because a uuid is never empty.
- **Unlike the cast block, it has NO "dropped" DEBUG.** When an id is absent, it is dropped
  silently.
- `repos` is `context.repos`, the GLOBAL `RepositoryContainer`
  (`lib/api/middleware/context.ts:61,116` `getRepositoriesSafe()`), NOT user-scoped. That matters:
  `UserScopedGroupsRepository` (`lib/repositories/user-scoped.ts:413-443`) has **no
  `findByIdRaw`**. Groups carry no `userId` (`lib/schemas/group.types.ts` `GroupRowSchema`), so
  there is no ownership filter.
- `findByIdRaw` = `StoreBackedRepository.findByIdRaw` → `_findById`
  (`store-backed.repository.ts:78-80` → `base.repository.ts:247-258`). That is a **fallback-mode
  `safeQuery(…, 'Error finding entity by ID', { id }, null)`** that also runs `this.validate(result)`,
  so it never throws. **The route's new WARN is UNREACHABLE through v4's real code**, exactly as
  the cast WARN is (v5 already recorded and pinned the latter's absence).

The log-field bag, whole and in order (:115-123):

```ts
  logger.debug('Scenario Builder request accepted', {
    mode: body.mode,
    castCount: characterIds.length,
    groupCount: groupIds.length,          // NEW — post-filter count
    hasProject: !!body.projectId,
    inChat: !!chat,
    revising: body.revision != null,
    profileId: connectionProfile.id,
  })
```

The pass-through (:150-167) puts `groupIds` between `characterIds` and `chat` in the `input`
object handed to `runScenarioBuilder`.

### A.3 `lib/services/scenario-builder/scenario-builder.service.ts`

- The `ScenarioBuilderInput` interface (:52-66) adds
  `/** Groups named outright, already vetted by the caller (the group's Scenarios card). */
  groupIds?: string[]` after `characterIds`. The field is optional.
- The pool call (:132-137) is `resolveScenarioBuilderMountPool({ userId, projectId: input.projectId ?? null, characterIds: input.characterIds, groupIds: input.groupIds ?? [] })`.
- The run-starting DEBUG (:139-155), whole and in order: `{ mode, castCount: input.characterIds.length,
  namedGroupCount: input.groupIds?.length ?? 0, inChat, revising, profileId, provider, model,
  webAvailable, pool: { participants, groups, projects, hasGlobal } }`. The message is
  `'Scenario Builder run starting'`.

### A.4 `lib/scenario-builder/mount-pool.ts` (whole function :39-126)

- The options gain `groupIds?: string[]` (:43-44).
- :47 `const namedGroupIds = [...new Set((opts.groupIds ?? []).filter(Boolean))]` (dedup and
  filter, computed before `castIds`).
- **The group tier (:79-87). Named groups come FIRST, then the cast's union:**

```ts
  // 2. Group stores — the named groups, then the union over the whole cast
  //    (both helpers fail soft).
  const groupIds: string[] = []
  for (const groupId of namedGroupIds) {
    groupIds.push(...(await resolveMountPointIdsForGroup(groupId)))
  }
  for (const characterId of liveCastIds) {
    groupIds.push(...(await resolveGroupMountPointIdsForCharacter(characterId)))
  }
```

  `groupIds` is a plain array with duplicates allowed. The dedup is the unchanged
  `dedupeTierTriple` (:102-107), which keeps first occurrence, so a named group's ids keep their
  position ahead of the cast's. Participants are then excluded against the deduped scoped tiers
  (:108-113, unchanged).
- The pool DEBUG (:116-124), whole and in order: `'Resolved Scenario Builder mount pool'`
  `{ castCount, liveCastCount, namedGroupCount: namedGroupIds.length, participants, groups,
  projects, hasGlobal }`. `namedGroupCount` is the deduped/filtered count and **sits after
  `liveCastCount`**.
- The header prose (:7-9) adds: "Launched from a group's page, the run also names that group outright
  (`groupIds`), whose stores join the group tier whether or not any cast member belongs to it."

### A.5 `lib/mount-index/tiered-mount-pool.ts`

The NEW export (:183-206):

```ts
/**
 * One group's stores: its official store plus every store linked to it. For a
 * caller that holds a group rather than a member (the Scenario Builder launched
 * from a group's page). Returns `[]` on any lookup failure (fails soft).
 */
export async function resolveMountPointIdsForGroup(
  groupId: string | null | undefined,
): Promise<string[]> {
  if (!groupId) return [];
  try {
    const repos = getRepositories();
    const ids = new Set<string>();
    // findByIdRaw avoids a store read on this hot path — we only need the
    // group's officialMountPointId pointer, not its hydrated content.
    const group = await repos.groups.findByIdRaw(groupId);
    if (group?.officialMountPointId) ids.add(group.officialMountPointId);
    const links = await repos.groupDocMountLinks.findByGroupId(groupId);
    for (const link of links) ids.add(link.mountPointId);
    return [...ids];
  } catch (error) {
    logger.warn('Group store lookup failed', { groupId, error: errMsg(error) });
    return [];
  }
}
```

`resolveGroupMountPointIdsForCharacter` (:215-232) now reads:
`for (const membership of memberships) { for (const id of await resolveMountPointIdsForGroup(membership.groupId)) ids.add(id); }`,
and its outer catch is unchanged (WARN `'Group mount lookup failed' { characterId, error }`).

**The WARN wording change**, both WARN, both on the `TieredMountPool` service logger:
- BEFORE (`08c49319d~1`, per-membership inner catch): `logger.warn('Group store lookup failed for membership', { groupId: membership.groupId, characterId, error: errMsg(error) })`
- AFTER (`:203`, inside the new helper): `logger.warn('Group store lookup failed', { groupId, error: errMsg(error) })`. `characterId` is gone.

**Hidden semantic change (not in the commit message):** the fail-soft granularity moved.
- BEFORE: the official id went straight into the OUTER set before the links read, so a links-read
  throw KEPT the official store.
- AFTER: both reads sit inside the helper's own `try`, so a links-read throw drops the group's
  official store too (the helper returns `[]`).

**Neither arm is reachable through v4's real code.** `groups.findByIdRaw` → `_findById` and
`groupDocMountLinks.findByGroupId` (`group-doc-mount-links.repository.ts:64-76`) are both
fallback-mode `safeQuery` (default `null` / `[]`). The failure lands in each repository's own
ERROR line (`Error finding entity by ID` / `Error finding links by group ID`), not in this WARN.
So both the old and the new WARN wording are dead lines, and so is the granularity change.

On success, the id ORDER is identical before and after: per membership, official then links,
with first-occurrence dedup across groups.

### A.6 `lib/query/keys.ts` :91

`groups.list: () => ['groups', 'list'] as const` is new. Its only v4 consumer is `SaveScenarioDialog`.

### A.7 Client: `components/scenarios/ScenariosManager.tsx`

- The header doc adds the `shelf` bullet (:14-16).
- New imports: `next/dynamic`, `STAFF_AVATARS`, `useImagesHidden`, and `type SaveScenarioTargetKey`.
- The builder is `dynamic(() => import(...ScenarioBuilderDialog), { ssr: false })`.
- New exported type (:43-47):
  ```ts
  export type ScenarioShelf =
    | { kind: 'general' }
    | { kind: 'project'; projectId: string; projectName?: string | null }
    | { kind: 'group'; groupId: string }
  ```
- `shelfSaveTarget(shelf)` (:49-58) maps `general` → `'general'`, project → `` `project:${projectId}` ``,
  and group → `` `group:${groupId}` ``.
- The prop is `shelf?: ScenarioShelf` with doc "The shelf being managed; offers the Host's Scenario
  Builder when set." It also destructures `refresh` from the mutator, and adds
  `const imagesHidden = useImagesHidden()` and `const [builderOpen, setBuilderOpen] = useState(false)`.
- The toolbar (:201-221): `+ New scenario` is now wrapped in `<div className="flex items-center gap-2 flex-wrap">`,
  preceded, only when `shelf` is set, by:
  `<button type="button" className="qt-button qt-button-secondary qt-button-sm inline-flex items-center gap-1.5">`
  { `!imagesHidden` && `<img src={STAFF_AVATARS.host ?? '/images/avatars/host-avatar.webp'} alt="" className="h-4 w-4 rounded-full" />` }
  **`Ask the Host to set the scene`**.
  `useImagesHidden` defaults to `false` outside the Salon's provider (`images-hidden-context.tsx:18`;
  the only provider is `SalonView.tsx:1513`), so the avatar is effectively always shown on shelves.
- The mount (:253-266):
  ```tsx
  {shelf && builderOpen && (
    <ScenarioBuilderDialog
      isOpen={builderOpen}
      onClose={() => setBuilderOpen(false)}
      cast={[]}
      projectId={shelf.kind === 'project' ? shelf.projectId : null}
      projectName={shelf.kind === 'project' ? shelf.projectName : null}
      groupIds={shelf.kind === 'group' ? [shelf.groupId] : undefined}
      saveTargets="everywhere"
      defaultSaveTarget={shelfSaveTarget(shelf)}
      // Wherever it was filed, this shelf may have gained a row.
      onSaved={() => void refresh({ silent: true })}
    />
  )}
  ```
  It passes **no `onUse`**.

**Where the manager is used:**
- `app/scenarios/ScenariosView.tsx:53`: `shelf={{ kind: 'general' }}`.
- `app/prospero/[id]/components/ScenariosCard.tsx`: a new prop `projectName?: string | null`
  (doc: `Offered as the Host's default save home ("Project: <name>").`) and
  `shelf={{ kind: 'project', projectId, projectName }}`. `ProjectDetailView.tsx:186` passes
  `projectName={project.name}`.
- NEW `app/aurora/groups/components/GroupScenariosCard.tsx` (60 lines). It uses
  `useScenarioMutator(\`/api/v1/groups/${groupId}/scenarios\`)`, a collapsible `qt-card` like the
  Members/Linked Stores cards, and `ScenariosIcon`. Its strings, verbatim:
  - heading `Scenarios ({mutator.scenarios.length})`
  - subtitle `Reusable starting scenes offered whenever a member takes a seat`
  - manager props `scopeLabel="group"`, `shelf={{ kind: 'group', groupId }}`
  - `emptyMessage="No scenarios yet. Create one and it'll be offered whenever a member of this group joins a new chat."`
- `GroupDetailView.tsx`: `const [scenariosExpanded, setScenariosExpanded] = useState(false)`, and
  the card is placed **after `GroupLinkedStoresCard`**, last in the cards column (:273-277). It
  starts collapsed.

### A.8 `components/scenario-builder/ScenarioBuilderDialog.tsx`

- The props (:48-65) gain `groupIds?: string[]` ("Groups named outright (a group's Scenarios card):
  their stores join the pool."), `saveTargets?: 'cast' | 'everywhere'` and
  `defaultSaveTarget?: SaveScenarioTargetKey`.
- `onUse` becomes OPTIONAL: "…Omit where there is no box."
- The `SaveScenarioTargetKey` type is re-exported.
- **The mode default changed for EVERY surface (:114-117):**
  `cast.length > 0 || !!projectId || (groupIds?.length ?? 0) > 0 ? 'in-world' : 'real'`, where it
  was `cast.length > 0 ? … : 'real'`. The commit message is silent on this: the New Chat form
  with a project and an empty cast now opens **in-world**.
- The build body (:161-172) adds `groupIds: groupIds ?? []` between `characterIds` and `chatId`.
  **The dialog now ALWAYS sends `groupIds`** (`[]` on New Chat and in-chat).
- `handleUse` returns early `if (draft === null || !onUse)`.
- **The review footer (:222-254) in shelf mode (`!onUse`):**
  - The left slot is `<span />`.
  - The first right button reads **`Close`** (`{onUse ? 'Cancel' : 'Close'}`).
  - The primary button is `Save as scenario…` (`qt-button-primary`, disabled `!draft?.trim()`,
    opens save).
  - With `onUse` set, the footer is unchanged: left `Save as scenario…` (secondary), `Cancel`,
    and primary `Use this scene`.
  - The inputs-pane footer (`Cancel` / `Set the scene`) is untouched.
- The save mount passes `targets={saveTargets}` and `defaultTarget={defaultSaveTarget}`.
- The builder **stays open after a save** (the save dialog closes itself). The spec says:
  "After a save the shelf refreshes silently; the builder stays open."

### A.9 `components/scenario-builder/SaveScenarioDialog.tsx` (full rewrite of the target half)

The type (:36-40):
`export type SaveScenarioTargetKey = 'general' | \`project:${string}\` | \`group:${string}\` | \`character:${string}\``

The props add `targets?: 'cast' | 'everywhere'` (default `'cast'`) and
`defaultTarget?: SaveScenarioTargetKey` ("Preselected home; falls back to General when it is not on
offer."). `interface GroupRow` → `NamedRow`, and
`const byName = (a, b) => a.name.localeCompare(b.name)`.

State: `const [chosenTarget, setTarget] = useState<string | null>(null)`. It stays null until the user
picks.

Queries, all gated on `isOpen`:
- `castGroupData`: `queryKeys.groups.byCharacters(castKey)`, `/api/v1/groups?characterIds=…`,
  enabled `isOpen && !everywhere && castKey.length > 0`.
- `allGroupData`: `queryKeys.groups.list()`, `/api/v1/groups`, enabled `isOpen && everywhere`.
- `projectData`: `queryKeys.projects.list()`, `/api/v1/projects`, enabled `isOpen && everywhere`.
- `characterData`: `queryKeys.characters.list()`, `/api/v1/characters`, enabled
  `isOpen && everywhere`. The comment reads "The list endpoint already leaves archived characters
  out" — **verified**: `app/api/v1/characters/handlers/get.ts:24-33` excludes `archivedAt` unless
  `?archived=include|only`.

Lists:
- `projects`: in everywhere mode, `[...projectData.projects].sort(byName)`. In cast mode,
  `projectId ? [{ id: projectId, name: projectName || 'this project' }] : []`.
- `groups`: `[...((everywhere ? all : cast) ?? [])].sort(byName)`. **Cast-mode groups are now
  SORTED by name** (they were in server order before).
- `characters`: in everywhere mode, sorted by name. In cast mode it is `cast` **unsorted**.

Selection:
- `offered` is a `Set` of `general`, `project:<id>`…, `group:<id>`…, and `character:<id>`….
- `target = chosenTarget ?? (defaultTarget && offered.has(defaultTarget) ? defaultTarget : 'general')`.
  The preselection therefore flips from General to the shelf's home once the list that offers it
  arrives.

**The target key construction, before/after:**
- BEFORE: `target === 'project' && projectId` → `/api/v1/projects/${projectId}/scenarios`;
  `saved = { kind: 'project', projectId, path }`; option `value="project"`.
- AFTER (:164-166, :196-197): `target.startsWith('project:')` →
  `` `/api/v1/projects/${target.slice('project:'.length)}/scenarios` ``;
  `saved = { kind: 'project', projectId: target.slice('project:'.length), path: data.path }`;
  option ``value={`project:${p.id}`}``. **This applies in EVERY mode** (cast mode included).

The `<select>` (:277-312) keeps `Quilltap General` bare at the top, then three **`<optgroup>`s,
now present in cast mode too**, each shown only when non-empty:
- `label="Projects"`, options `Project: {p.name}`
- `label="Groups"`, options `Group: {g.name}`
- `label="Characters"`, options `{c.name}&rsquo;s scenarios`

Unchanged strings: the title `File this scene as a scenario`, the labels `Name` /
`Description (optional)` / `Where it lives`, `A character&rsquo;s own scenarios keep a title and a
body only.`, `A scenario wants a name before it can be filed.`, `Choose where the scenario should
live.`, `The scenario could not be filed (HTTP ${res.status}).`, `The scenario could not be filed.`,
the toast `“${trimmedName}” has been filed among the scenarios.`, `Cancel`, and
`Save` / `Filing…`. After a save it still does `invalidateQueries(queryKeys.scenarios.all)` →
toast → `onSaved(saved)` → `onClose()`.

### A.10 Help (four pages, no new file; the tree stays **129**, confirmed by `git ls-tree` = 129 at `08c49319d`)

- `general-scenarios.md` +4: new `## Asking the Host` section (the button beside `+ New scenario`;
  it reads Quilltap General; files to the General shelf by default or anywhere else).
- `groups.md` +8/−0: a new bullet **Tend the Scenarios shelf** (the Scenarios card's CRUD "exactly as
  a project's card does"; the Host reads the group's own store, its linked stores and General) and a
  new related link `[The Scenario Builder](scenario-builder.md)`.
- `project-scenarios.md` +1: the **Ask the Host to set the scene** bullet.
- `scenario-builder.md` +20/−1: "two places" → "five places" (adds the General page, the project's
  card and the group's card); a paragraph on the shelf pool (General only / project + General /
  group official + linked + General; "From a project or a group he assumes an in-world place unless
  you say otherwise; from the General page, a real one."); a new `## Called to a Shelf` section (no
  Use; Save is primary; every home listed; archived characters excepted; the home preselected; the
  shelf refreshes; the dialog stays open until **Close**).
- v5's four pages are md5-identical to v4's `08c49319d~1` (measured), so the port is a clean
  byte-copy.

### A.11 Tests in the commit

- `app/api/v1/scenario-builder/__tests__/route.test.ts`: `GROUP_ID_1/2` consts, `makeRepos` gains
  `groups: { findByIdRaw }`, and a new `describe('POST /api/v1/scenario-builder?action=build — named groups')`
  with `it('passes existing group ids through and drops unknown ones')` and
  `it('defaults to no named groups')`.
- `lib/scenario-builder/__tests__/mount-pool.test.ts`: three new `it`s. These are MOCKED repos, so
  they are case names only.
  - `a group named outright joins the group tier — official and linked stores — with no cast`
  - `a named group the cast also belongs to is not counted twice`
  - `an unknown named group contributes nothing and does not throw`
- `components/scenario-builder/__tests__/ScenarioBuilderDialog.test.tsx`: new
  `describe('ScenarioBuilderDialog — launched from a scenarios shelf')` with:
  - `sends the named group ids and defaults to in-world` (body `toMatchObject({ mode: 'in-world', characterIds: [], groupIds: ['grp-1'] })`)
  - `offers no "Use this scene"; Save is the primary action` (exactly ONE `Save as scenario…`, plus `Close`)
  - `Save lists every home, preselects the shelf, and files it there` (options `Group: Aeronauts Club`, `Project: The Estate`, `/Riya.s scenarios/`; select value `group:grp-1`; `onSaved({kind:'group',groupId:'grp-1',path:'Scenarios/rain.md'})`)
  - `saving to another project posts to that project` (default `general`; change to `project:proj-1`)
- No test for `resolveMountPointIdsForGroup` directly, for the WARN rename, or for ScenariosManager
  / GroupScenariosCard.

---

## B. v5 counterparts on main `2aed9a552`

### B.1 Server

**`crates/quilltap-core/src/api/types.rs:3986-4018`.** The build verb is:

```rust
    ScenarioBuilderBuild {
        run_id: String,
        #[serde(default = "empty_json_object")]
        body: serde_json::Value,
    },
```

**The body is ONE raw `Value`** (the §S.1 tri-state rule at object granularity). **Adding `groupIds`
touches NO `Request` field.** There is nothing to add to `api/types.rs`.

**`crates/quilltap-web/tests/dispatch_wrong_type_census.rs:2580`** holds
`const EXCLUDED_BY_THE_ROUTE_IDENTIFIER_RULE: usize = 449;`. The P4.D217 entry (:2570-2579) already
records that "the build's v4 BODY rides as ONE raw `serde_json::Value` (`body`), which
`typed_request_fields` never sees". **The census does NOT move for `groupIds`** (it stays 449).
The collision with the Concierge lanes that the order worried about does not exist for this lane.

**`services/scenario_builder/request_schema.rs` (415 lines) is the Zod twin.**
- It needs a `group_ids: Vec<String>` on `ScenarioBuildRequest` (:92-103), a parse block cloned
  from `characterIds` (:304-332, with elements first and then `too_big_array(32)`) placed between
  `characterIds` and `chatId`, and `to_value` (:129-162) emitting `"groupIds": [...]` right after
  `characterIds`.
- The header doc table (:5-18) gains the row.

**`api/scenario_builder.rs` (537 lines).** `scenario_builder_prepare` (:292-434) does:
- cast scoping at :377-402. This reads the overlaid character with
  `db.read_main(|main| db.read_mount_index(|mount| characters_read::find_by_id(...)))`. A failed
  read is a SILENT miss, with a comment explaining the WARN is unreachable.
- the chat check at :405-424.
- the accepted DEBUG at :411-420: `mode, castCount, hasProject, inChat, revising, profileId`.

The new group block goes **between :403 and :405**. The groups existence read has two candidates:
`db::groups::find_official_mount_point_id_raw` (`groups.rs:188-208`, `Ok(Some(_))` = exists) or
`find_name_and_official_mount_point_id_raw` (`:165-185`). Then:
- add `groupCount = group_ids.len()` to the DEBUG between `castCount` and `hasProject`
- set `group_ids` on `ScenarioBuilderInput` (:436 builder)

**`services/scenario_builder/mod.rs`.** `ScenarioBuilderInput` (:123-135) has no `group_ids`; add it
after `character_ids`. The run (:276-310) calls
`mount_pool::resolve_scenario_builder_mount_pool(main, mount, &pool_user, pool_project.as_deref(), &pool_cast)`,
which needs `group_ids` added. The `Scenario Builder run starting` DEBUG (:300-311) is
`mode, castCount, inChat, revising, profileId, provider, model, webAvailable, pool`, and it needs
`namedGroupCount = input.group_ids.len()` after `castCount`.
- v4's field is `input.groupIds?.length ?? 0` over the route-vetted list, NOT deduped again. The
  route dedups first, so for route callers it is the deduped count.
- In-module test fixtures (e.g. :672 `character_ids: Vec::new()`) need the new field.

**`services/scenario_builder/mount_pool.rs` (210 lines).**
`resolve_scenario_builder_mount_pool(main, mount, user_id, project_id, character_ids)` (:88-94)
assembles the tiers in this order: cast vaults (:101-150), then group (:152-160, cast union only),
then project (:163), then General (:166-172), then dedup and participant exclusion (:176-191).
- It needs a `group_ids: &[String]` parameter, the `namedGroupIds` dedup + `filter(non-empty)`
  (the `push_unique` idiom :65-69), and the named groups pushed FIRST at :153.
- The pool DEBUG (:200-208) needs `namedGroupCount` after `liveCastCount`.
- The module header (:38-51, the "four log lines") should gain the v4 sentence.

**`db/tiered_mount_pool.rs:207-250`, `resolve_group_mount_point_ids_for_character`:**

```rust
    for group_id in memberships {
        // Per-membership try/catch (v4) — a failed lookup for one group is logged
        // and skipped, never aborting the whole resolution.
        let mut per_membership = || -> Result<(), super::DbError> {
            if let Some(Some(off)) = groups::find_official_mount_point_id_raw(main, &group_id)? {
                if !off.is_empty() { push_unique(&mut ids, off); }
            }
            let links = GroupDocMountLinksRepository::new(mount).find_by_group_id(&group_id)?;
            for link in links { push_unique(&mut ids, link); }
            Ok(())
        };
        let _ = per_membership();
    }
```

This is SILENT. The comment says "is logged" but it logs nothing. v4's old WARN was
`Group store lookup failed for membership {groupId, characterId, error}`, and the new one is
`Group store lookup failed {groupId, error}`. v5 has neither, and the memberships read's
`Err(_) => return Vec::new()` (:227) also lacks v4's `Group mount lookup failed {characterId, error}`.
It also pushes the official id into the OUTER list before the links read, which is v4's OLD
granularity.
- **Port shape:** a new `pub fn resolve_mount_point_ids_for_group(main, mount, group_id) -> Vec<String>`
  (a local list; `[]` on any `Err`), called per membership.
- **But see §D.3:** v4's WARN is unreachable, and the faithful v5 failure line is the repositories'
  fallback ERROR, which is the `mount_pool.rs:111-129` precedent.
- No v5 grep hit for either WARN string in `crates/`.

**Route edge.** `crates/quilltap-web/src/scenario_builder_routes.rs` (270 lines) reads raw bytes
(`serde_json::from_slice::<Value>` :133) and forwards `body: raw` (:165). No change is needed.
`web_edge_body_parse_guard` (`crates/quilltap-harness/tests/web_edge_body_parse_guard.rs`) has
**no scenario-builder row** (grep: none) and the edge has no collapsing idiom, so it is unmoved.

**Host driver.** `crates/quilltap-host/src/spine.rs:2539-2590` (`impl ScenarioBuilderDriver for
ChatSpine`, the Send bridge) → `run_scenario_builder_build` (:1758). It forwards the prepared input,
so no change is needed beyond the struct field.

### B.2 SPA (`apps/web/src/app/`)

- **`core/core-contract.ts:8158-8169`, `ScenarioBuildRequestInput`:** `mode, location, time,
  details?, connectionProfileId, projectId?, characterIds?, chatId?, priorDraft?, revision?`. Add
  `groupIds?: string[]` after `characterIds`. `ScenarioBuilderBuildRequest { type, runId, body }`
  (:8180-8184) is unmoved.
- **`scenario-builder/scenario-builder-run.state.ts`** (251 lines) passes `input` straight through as
  `body` (:114, :138). No change.
- **`scenario-builder/scenario-builder-dialog.ts`** (548 lines):
  - Inputs (:387-398) are `cast`, `projectId`, `projectName`, `chatId`, and the outputs `closed`,
    `use`, `saved`. **`use` is an Angular `output()`, which always exists, so v4's "no `onUse`"
    needs an explicit input** (e.g. `saveTargets === 'everywhere'`, or a `shelf`/`offerUse` flag).
  - The mode seed in `ngOnInit` (:481-484) is `this.cast().length > 0 ? 'in-world' : 'real'`, and it
    needs `|| !!projectId() || groupIds().length > 0`.
  - The build body (:505-517) needs `groupIds: this.groupIds() ?? []` between `characterIds` and
    `chatId`.
  - The review footer (:318-341) needs the shelf branch (`<span />`, `Close`, primary
    `Save as scenario…`).
  - The save mount (:347-356) needs `[targets]` / `[defaultTarget]`.
  - `handleUse` (:541-546).
- **`scenario-builder/save-scenario-dialog.ts`** (299 lines):
  - The target signal (:186-187) is `signal('general')`, doc `general | project | group:<id> |
    character:<id>`, so the old key is still `'project'`.
  - The `<select>` (:104-126) has no optgroups, `value="project"`, groups unsorted, and cast
    characters.
  - `save()` (:220-298) has `target === 'project' && projectId` at :243 and :273.
  - The groups query (:199-203) is `groupsByCharactersKey(castKey)` / `fetchGroupsByCharacters`,
    enabled `castKey.length > 0`.
  - It needs: `targets` / `defaultTarget` inputs, a nullable `chosenTarget` + the `offered` computed,
    the three everywhere-mode queries, `project:<id>` keys, and optgroups.
- **`scenario-builder/scenario-builder.api.ts`** (108 lines): `scenarioBuilderKeys`,
  `groupsByCharactersKey`, `connectionProfilesKey`, `fetchGroupsByCharacters`. It has no all-groups,
  projects or characters fetchers, but **those exist elsewhere and OWN the same keys**:
  - `screens/groups/groups.api.ts:21-28`: `groupKeys.list()` = `['groups','list']` (v4's new key,
    already present), whose `fetchGroups` → `GroupCardModel[]`. Consumers: `groups-section.ts:81`,
    `group-editor.ts:337`.
  - `screens/prospero/projects.api.ts:20-21,45`: `projectKeys.list()` = `['projects','list']`,
    whose `fetchProjects` → `ProjectCardModel[]`.
  - `screens/characters/characters.api.ts:44-46,113`: `characterKeys.list()` = `['characters','list']`,
    whose `fetchCharacterList` → `CharacterListItem[]` (the default excludes archived, per P4.D64).
- **`scenario/scenario.api.ts:34-43`, `scenarioKeys`:** `all`, `general(inclArch)`,
  `project(id, inclArch)`, `group(characterIdsKey, inclArch)` (the New-Chat union, NOT a per-group
  shelf key), and `character`. The save dialog already invalidates `scenarioKeys.all`. The shelf
  managers are NOT TanStack-cached: `makeScenarioMutator` owns signals, so the refresh must be the
  explicit `mutator.refresh({ silent: true })`, v4's exact call, which already exists on
  `ScenarioMutator` (`screens/scenarios/scenarios.api.ts:54`).
- **`screens/scenarios/shared/scenarios-manager.ts`** (190 lines): inputs `mutator`, `scopeLabel`,
  `emptyMessage`. The toolbar (:47-60) is the checkbox and then a bare `+ New scenario` button. It has
  no `shelf` and no builder. Add: a `shelf` input, the `ScenarioShelf` type, `shelfSaveTarget`, the
  Host button (the `chat-scenario-control.ts:111-123` pattern: `HOST_AVATAR` from
  `scenario-builder/host-avatar`, `injectImagesHidden()`, whose root default is `false` per
  `chat/hidden-image/images-hidden.ts:28-33`), and a `@defer`'d `qt-scenario-builder-dialog` (the
  `new-chat-form.ts:326-336` idiom).
- **`screens/scenarios/scenarios-page.ts:42-47`**: add `[shelf]="{ kind: 'general' }"`.
- **`screens/prospero/cards/project-scenarios-card.ts`** (47 lines): inputs are `projectId` and
  `defaultOpen` only, with no `projectName`. Its host is `screens/prospero/project-detail.ts:141`
  (`<qt-project-scenarios-card [projectId]="id()" [defaultOpen]="firstVisit()" />`), which has
  `project()` available.
- **`screens/scenarios/scenarios.api.ts:256-330`** has only `projectScenarioMutator` and
  `generalScenarioMutator`. **There is no `groupScenarioMutator`.**
- **`screens/groups/`: there is NO group Scenarios card** (grep `scenario` in `screens/groups/` finds
  nothing). The group page is `group-editor.ts` (391 lines). The cards column is :194-208
  (`<div class="mt-12 space-y-6 max-w-2xl">` with `qt-group-members-card`, then
  `qt-group-stores-card`). The new card goes after `qt-group-stores-card` and should follow
  `project-scenarios-card.ts`'s `qt-collapsible-card` shape with `icon="scenarios"`.
- **The server verbs for the group shelf exist:** `Request::GroupScenario{List,Create,Get,Update,Rename,Delete}`
  (`types.rs:876-920`), all with `include_archived` on List/Update/Rename/Delete. **But the TS
  twins `GroupScenarioUpdateRequest` / `GroupScenarioRenameRequest` / `GroupScenarioDeleteRequest`
  (`core-contract.ts:1674-1692`) LACK `includeArchived?`.** `GroupScenarioListRequest` has it, and
  the project twins have it on all four. The contract needs widening before a `groupScenarioMutator`
  can mirror `projectScenarioMutator`.
- **SPA oracle recorder:** `apps/web/oracle/scenario-builder.recorder.ts` →
  `src/app/scenario-builder/__fixtures__/scenario-builder-oracle.json` (source string
  `v4 d1c06cd9d — parse-agent-stream.ts applyAgentStreamEvent + ScenarioBuilderDialog.tsx
  describeHostActivity`; 29 `toolCallCases` + 40 `activityCases`), decoded by
  `oracle-corpus.testing.ts`. **It records NEITHER the save dialog NOR the request body**, only the
  two pure functions, and this commit touches neither (`describeHostActivity`'s dialog hunks are
  elsewhere in the file). See §D.4.
- **Other hosts of the builder** (the mode-default change reaches them):
  `screens/new-chat/new-chat-form.ts:326-336` (passes `builderProjectId()` with a possibly empty
  cast) and `chat/sidebar/chat-scenario-control.ts:126-135`.

---

## C. Families, oracle cases, fixtures

"Recipes" here are the regen blocks in each family's own `//!` header. `harness/tools/` holds only
`recipe_sweep.py` (the sweep driver) plus stubs, with no per-family recipe file.

1. **`scenario_build_request_schema_equivalence`** (`crates/quilltap-harness/tests/`), oracle
   `harness/oracle/cases/scenario-build-request-schema.ts` (pure tsx, no fixture), env
   `QT_ORACLE_SCENARIO_BUILD_REQUEST_SCHEMA`. Header recipe (:20-28):
   `$N/npx tsx $V5W/harness/oracle/cases/scenario-build-request-schema.ts > /tmp/oracle-scenario-build-request-schema.ndjson`.
   **MOVES:** the family diffs the parsed OUTPUT bytes, and every accepting case gains
   `"groupIds":[]` after `characterIds` at the new pin. That is **red-first on every success row**
   before the twin grows. The corpus should add `groupIds` rows mirroring `characterIds`: absent,
   `[]`, a valid list, 33 entries, a bad element, a 33rd bad element (element-before-size), a
   non-array (aborts the refine), and `null`.
2. **`scenario_builder_mount_pool_equivalence`** (harness), oracle
   `harness/oracle/cases/scenario-builder-mount-pool.test.ts` (jest; calls
   `resolveScenarioBuilderMountPool({ userId, projectId, characterIds })` at :185-189), plants
   `harness/oracle/fixtures/scenario-builder-mount-pool.json` (1 plant + 12 arms + 2 plants + 2
   arms), and the fixture from the UNCHANGED `build-doc-opacity-fixture.ts`. That fixture has ONE
   group, **Severed** `d2000000-0000-4000-8000-000000000001`, with official
   `e2000000-…-0000000000f1` and linked `e2000000-…-0000000000f4`, and members Leilani + Abigail
   (`doc-opacity.json:10-14`). Env: `QT_ORACLE_SBPOOL`, `QT_FIXTURE_SBPOOL_MAIN/_MOUNT`. The header
   recipe (:39-57) is PIN REQUIRED and MINTED (rebuild → regen → cargo test), staged in
   `/tmp/qt-oracle-stage-sb-pool`. The comparand is the five pool fields plus the
   `ScenarioBuilderMountPool` logger's lines as a field SET (the `scenario_builder_capture` rule).
   **MOVES:** every arm's `Resolved Scenario Builder mount pool` line gains `namedGroupCount: 0`,
   which is red-first. New arms need `groupIds` in the step shape plus the call. At minimum:
   - a named group with no cast (Severed → `[f1, f4]`)
   - named + a member cast (dedup → no double count; named first)
   - an unknown group id (`[]`)
   - dup/empty named ids (`namedGroupCount` post-dedup)

   **A named-group-NOT-in-the-cast arm needs a SECOND planted group**, because the fixture's only
   group already has both named characters as members. The `Group store lookup failed` WARN
   (TieredMountPool logger) is NOT recorded by this case, which records only the pool logger.
3. **`tiered_mount_pool_equivalence`** (harness), oracle `harness/oracle/cases/tiered-mount-pool.ts`
   (tsx; drives only `resolveTieredMountPool`), fixture built by
   `harness/oracle/fixtures/build-tiered-mount-pool-fixture.ts` + `tiered-mount-pool.json`. Env:
   `QT_ORACLE_TMP`, `QT_FIXTURE_TMP_MAIN/_MOUNT`. Header recipe (:16-26). **Expected NOT to move**:
   the success-path order is identical before and after (§A.5), so a regen at the new pin is a
   neutrality proof (`cmp`). The home for a direct `resolveMountPointIdsForGroup` tier-2 arm (a new
   v5 `pub fn`) would be here; the case has no log capture.
4. **`scenario_builder_tier3_equivalence`** (harness), oracle
   `harness/oracle/cases/scenario-builder-tier3.test.ts` (jest; `runScenarioBuilder` with
   `input: { … characterIds: c.characterIds … }` :320-335), fixture
   `harness/oracle/fixtures/scenario-builder-tier3.json` (21 cases incl. `no_cast_no_project`, which
   is exactly the General-shelf pool), doc-opacity fixture, env `QT_ORACLE_SBT3` +
   `QT_FIXTURE_SBT3_MAIN/_MOUNT`, header recipe (:42-63, PIN REQUIRED, MINTED). The comparand
   includes the log lines (:638-646). **MOVES:** `Scenario Builder run starting` gains
   `namedGroupCount: 0` (and the pool line gains `namedGroupCount`), so every case goes red-first.
   A group-shelf case (`groupIds: [Severed]`, empty cast, in-world) and a project-shelf case are the
   natural additions.
5. **`scenario_builder_capture`** (`crates/quilltap-harness/tests/scenario_builder_capture/mod.rs`)
   is the shared STRUCTURAL capture (`(level, message, fields)`; field ORDER not compared, per the
   :7 doc). It is not a family and does not move, but it is why families 2 and 4 cannot see
   `namedGroupCount`'s POSITION.
6. **`scenario_builder_routes_equivalence`** (`crates/quilltap-web/tests/`), oracle
   `harness/oracle/cases/scenario-builder-routes.test.ts`, plants
   `harness/oracle/fixtures/scenario-builder-routes.json` over the committed
   `crates/quilltap-web/tests/fixtures/chat-send-{main,mount}.db`. Env `QT_ORACLE_SB_ROUTES`; header
   recipe (:60-72). **Log lines are compared with context keys IN ORDER** (:40-47), so **every
   accepted case goes red-first** on `groupCount` between `castCount` and `hasProject`. The recorded
   run projection (oracle :143-153: `mode, characterIds, chat, priorDraft, revision, aborted`; Rust
   :250) does NOT include `groupIds`, so extend both sides.

   New cases:
   - existing + unknown group ids (the route test's case)
   - duplicated ids (dedup before the check)
   - default `[]`
   - an unreadable (BLOB-named) group row: the WARN is unreachable, so pin its ABSENCE and v4's
     repository ERROR, exactly the cast precedent

   **The chat-send fixture must be checked for a `groups` table/row**, since a plant may be needed.
7. **`scenario_builder_dispatch_wire`, `scenario_builder_disconnect`,
   `scenario_builder_midstream_failure`** (web tests): no expected movement (the body is raw). Re-run.
8. **`dispatch_wrong_type_census`**: stays at **449** (§B.1). Re-run as a neutrality check.
9. **SPA:**
   - `scenario-builder-dialog.spec.ts` (1076 lines). The `toEqual` exact-body pin at :487-497 (and
     the bare `toMatchObject` :502) **will redden** once `groupIds: []` is always sent. The
     `opens in real mode when the cast is empty` row (:380) still holds (no project).
   - `SaveScenarioDialog — within the builder` (:747) will move on the `project` → `project:<id>`
     key.
   - `scenarios-manager.spec.ts` (`describe('ScenariosManager')` :163).
   - e2e `scenario-builder-flow.spec.ts` (four beats; all on New Chat and in-chat, using
     `Use this scene` / `Save as scenario…`) and `scenarios-flow.spec.ts` (shelves) are the homes
     for live shelf beats.
   - **The SPA oracle fixture does NOT move** (§D.4).
10. **Help:** `help_tree_equivalence` plus the embed guard. There are four byte-copies; the count
    stays 129, but the vendored-count literals are unaffected since there is no new file.

---

## D. Traps

1. **The `dispatch_wrong_type_census` prediction is FALSE.** The drift-ledger row and the order
   framing both expect a new `*_ids` field in `api/types.rs`, but `Request::ScenarioBuilderBuild`
   carries the v4 body as ONE raw `serde_json::Value`, so `groupIds` never becomes a typed Request
   field. The census literal stays **449**. There is no collision with the Concierge lanes on this
   literal. If a lane DOES touch `api/types.rs` for this, that is a wrong turn.
2. **The "restore the absent WARN" prediction is WRONG-SHAPED.** The ledger row says v5's silent
   per-membership closure lacks v4's warn "in both its old and new wording, a pre-existing absent
   line to restore with a capture pin". Both the old and the new `Group store lookup failed…` WARN
   are **unreachable through v4's real code**: `groups.findByIdRaw` → `_findById` and
   `groupDocMountLinks.findByGroupId` are fallback-mode `safeQuery`, which log their own ERROR and
   answer `null`/`[]`. The same holds for the route's new `Scenario Builder dropped an unreadable
   group id` WARN.
   - The v5 precedents are the cast WARN in `api/scenario_builder.rs:388-395`, recorded unreachable
     with its absence pinned, and `mount_pool.rs:111-129`, which reproduces the repository ERROR
     `Error finding entity by ID {collection, id, error}` instead of the dead WARN.
   - Porting the WARN text verbatim into v5 would emit a line v4 cannot emit.
   - The honest port: v5's `Result`-typed reads mean a real v5 `Err` path exists, so decide per
     §E.1. Either reproduce the repository fallback ERROR (`Error finding entity by ID`
     `{collection:"groups", id}` / `Error finding links by group ID {groupId}`) or keep silent, and
     pin whichever with a planted unreadable row.
   - Also: the existing v5 comment "a failed lookup for one group is logged" (`tiered_mount_pool.rs:232`)
     is false today.
3. **Hidden granularity change in `resolveGroupMountPointIdsForCharacter`.** In v4 the official id
   now dies with a links-read failure (the helper's local Set is discarded). v5 still pushes the
   official id into the outer list first (the OLD behaviour). This is unreachable in v4, but the
   new `resolve_mount_point_ids_for_group` must build a LOCAL list and return `[]` on either `Err`.
4. **The SPA oracle fixture will NOT show the target-key change.** The order framing says it will,
   but `scenario-builder-oracle.json` records only `applyAgentStreamEvent` and
   `describeHostActivity`, and neither is touched. The `project:<id>` change is proven only by the
   vitest spec ports of v4's four new `it`s plus the existing save spec reddening. A recorder regen
   at `08c49319d` should be byte-identical except the `source` string, which is a neutrality proof,
   not a red.
5. **Commit message vs hunks:**
   - **(a)** The message never mentions the mode-default change, which reaches New Chat: a project
     plus an empty cast now opens **in-world**.
   - **(b)** It never mentions that **optgroups and name-sorted groups now appear in cast mode too**.
     Cast-mode characters stay unsorted.
   - **(c)** "Unknown group ids are dropped" hides that the dedup precedes the check, that there is
     NO dropped-count DEBUG (unlike the cast), and that the WARN is dead.
   - **(d)** "the builder stays open after a save" is in the spec prose, not the message.
   - **(e)** The dialog now sends `groupIds: []` on EVERY surface (the body bytes change for New
     Chat and in-chat).
   - **(f)** The `Cancel` → `Close` relabel is only in the review footer, and only on a shelf.
6. **The dedup precedes the existence check**, in both the route (`new Set(body.groupIds)`, then
   `findByIdRaw`) and the pool (`new Set(filter(Boolean))`). `groupCount` counts post-existence, and
   the pool's `namedGroupCount` counts post-dedup. The service's `namedGroupCount` is
   `input.groupIds?.length ?? 0` with NO dedup of its own: it trusts the route.
7. **The dropping asymmetry, as the order states it, is moot in practice.** The order says an
   unreadable id is dropped with a WARN and an absent one silently. With `safeQuery`, BOTH are
   dropped silently at the route logger. An unreadable row costs v4 a repository ERROR
   (`Error finding entity by ID`, on the repository logger the route family must decide whether to
   record), and v4's `findByIdRaw` also runs `validate()`, so a schema-invalid group row answers
   `null`. v5's `find_official_mount_point_id_raw` selects one column and does NOT validate. The
   existence semantics of the two differ for a malformed row (see §E.2).
8. **Query-key cache-shape collision (P4.116 class).** v4's new `queryKeys.groups.list()`,
   `projects.list()` and `characters.list()` store the ENVELOPE (`{groups}` / `{projects}` /
   `{characters}`) via `apiFetch`. In v5 those exact keys are already owned by `fetchGroups` →
   `GroupCardModel[]`, `fetchProjects` → `ProjectCardModel[]` and `fetchCharacterList` →
   `CharacterListItem[]`. **The save dialog must reuse those fetchers and their array shapes** (each
   carries `id` + `name`), never store an envelope under the shared key.
9. **Angular has no "omitted output".** v4's shelf mode is keyed on `!onUse`, but v5's `use` is an
   `output()` that always exists. Drive the footer from an explicit input (`saveTargets ===
   'everywhere'` is the natural one, since it is set exactly when `onUse` is absent in v4). The
   choice is a design call to record, not a fidelity leak.
10. **The group shelf needs a group mutator the SPA does not have**, and the TS contract's group
    Update/Rename/Delete twins lack `includeArchived?` even though the Rust accepts it. Without the
    widening, "Show archived" plus a mutate would refetch the non-archived list (a silent
    divergence from the project card).
11. **The SPA preselection timing is v4's:** `target` is General until the list that offers the
    default arrives, then it flips, unless the user already chose. `[selected]`-per-option (v5's
    idiom) must re-evaluate against the computed target, and an e2e beat must wait for the option
    to exist before asserting the select's value (the v4 test does a `waitFor`).
12. **The PIN is REQUIRED.** v4 HEAD is past `08c49319d`, and the ledger has this in a
    nine-commit backlog with the Concierge chain. Regen every moved family from a detached worktree
    at `08c49319d` (or the round's target), never the live checkout. Families 2, 4 and 6 are
    MINTED-fixture recipes: rebuild, regen, then cargo, against the same build.

---

## E. Open questions

1. **The failure line for a v5 `Err` in the group reads** (the route existence check, and the new
   `resolve_mount_point_ids_for_group`). Options:
   - (a) reproduce v4's repository fallback ERRORs (`Error finding entity by ID {collection, id, error}`
     for groups; `Error finding links by group ID {groupId, error}` for links), the `mount_pool.rs`
     precedent
   - (b) stay silent, matching v4's route/pool loggers

   The WARN strings themselves should NOT be emitted (dead in v4). Needs a ruling or the precedent
   applied by name.
2. **What does "exists" mean for the route's group check in v5:** row present (the one-column raw
   read), or row present AND decodable/valid (v4's `validate`)? A BLOB-named group row separates
   them. Planting one on both sides settles it by measurement.
3. **Should `resolve_group_mount_point_ids_for_character` also gain v4's outer
   `Group mount lookup failed {characterId, error}`** (also unreachable in v4: `findByCharacterId`
   is `safeQuery` too), or is that the same dead-line class? Probably the same class; confirm and
   record it.
4. **The second group for the mount-pool family:** plant it in `scenario-builder-mount-pool.json`
   (a `groups` row + `group_doc_mount_links` row, no members), or widen the doc-opacity builder
   (which touches every opacity reader)? Planting is the lower-blast option.
5. **The chat-send fixture pair (routes family):** does it have a `groups` table and a usable row?
   If not, the routes family's group arms need a plant (possibly the vintage-widening path).
6. **Where the SPA's shelf `ScenarioShelf` type and `shelfSaveTarget` live:** the manager (v4) or
   `scenario-builder/`? And does the manager `@defer` the builder like the other two hosts
   (v4's `next/dynamic`)? The default is to follow `new-chat-form.ts`.
7. **Whether the `help/` half really splits** to P4.D227 as the ledger's ORDERED cell says, or rides
   with P4.D230. The four pages are clean byte-copies either way.
