# Survey — P4.D246: `9753d0eb2`'s DATA half (the `allowAnyCharacter` create default `false → true` at BOTH v4 sites, the project PUT answering the ENRICHED project, the roster auto-add REMOVED from chat create, the chat-PUT move-into-project half measured) + `help/**` and `docs/v4/**` re-vendored WHOLE at `e5c6bd0c0`

**Date:** 2026-10-03 · **v4:** `e5c6bd0c0` (target pin; baseline `f6426e196`) · **v5 main:** `538fcbf65` · **Kind:** read-only measurement — `git show`/`git diff`/`git archive` of v4 at both pins (the two trees extracted into the scratchpad for `diff -rq` / `md5`), reads of every v5 twin, `ggrep` of every `characterRoster` / `allowAnyCharacter` site in `crates/quilltap-core/src`, of every `projects.create(` in `harness/oracle/**`, and of every `projectUpdate` / `ProjectUpdate` consumer in `crates/**` and `apps/web/src/**`. Nothing was built or run in either repo.

## The finding in one line

**v5 has ONE create default where v4 now has TWO, and the second is not a one-character flip** — v4's `createProjectSchema` prefault (`schemas.ts:16`) AND `ProjectsRepository.prepareCreateData`'s `?? true` (`projects.repository.ts:60`) both moved, but v5's repository has NO seeding step at all (its `db/projects.rs:236` comment says the schema defaults make v4's seeding "redundant, reproduced here for free" — true only while both were `false`; now v4's seed is `true` while v4's `ProjectPropertiesSchema` read default STAYS `false`), so the lane must ADD a seed in `ProjectsRepository::create` and must NOT touch the `#[serde(default)]` read default. The PUT enrichment is a pure lift of the GET's existing body (`api/projects.rs:290-367`) into a shared helper, and v5's `notFound` re-read arm ALREADY exists (`:512`) — v4 converged onto it. The chat-create auto-add is ported and live (`services/chat_create.rs:1286-1320`) and must be deleted; **the chat-PUT half was NEVER ported** — `api/salon.rs:1554-1556` names it as a deliberate deferral, which v4's deletion now retires (a CONVERGENCE, comment-only). **Two red-first traps:** the chat-create capstone's fixture builder creates its project WITHOUT `allowAnyCharacter` (`build-chat-create-capstone.ts:378-386`), so rebuilt at the target pin it bakes `true` and the auto-add is skipped on BOTH sides — green whether or not v5 deletes the write; and the capstone dumps only MAIN tables while the roster lives in the project's store `properties.json` — so its visibility of the write must be MEASURED before any red-first claim. **No committed wire test pins the PUT body** (no REST route — dispatch only), but the SPA reads a key the server never sends (`project().roster`, `project-characters-card.ts:119`, vs the server's enriched `characterRoster`) — a P4.D247 matter the Shared contract must name.

---

## §A The v4 hunks (quoted from `git show 9753d0eb2`, verified at `e5c6bd0c0`)

### A1 The create default — TWO sites

`app/api/v1/projects/schemas.ts:16` (at the target):

```diff
-  allowAnyCharacter: z.boolean().prefault(false),
+  allowAnyCharacter: z.boolean().prefault(true),
```

`lib/database/repositories/projects.repository.ts:55-63` — `prepareCreateData` (called by `AbstractStoreBackedRepository.create` at `store-backed.repository.ts:138`, BEFORE the entity is validated):

```diff
-      allowAnyCharacter: data.allowAnyCharacter ?? false,
+      allowAnyCharacter: data.allowAnyCharacter ?? true,
       characterRoster: data.characterRoster ?? [],
```

The two sites are reachable separately. The route (`app/api/v1/projects/route.ts:68-80`) ALWAYS passes `allowAnyCharacter: validatedData.allowAnyCharacter` (the prefault fills an ABSENT key; `null` stays a Zod 400 — `prefault` is not `nullish`), so the repository seed fires only on a create whose data LACKS the key: `lib/import/quilltap-import/import-entities.ts:220,235` (a `.qtap` project with no key), `lib/backup/restore/restore.ts:317` (an archive project with no key), and every v4-side fixture builder that calls `repos.projects.create` without it (§D2). `??` also maps an explicit `null` to `true`.

**What did NOT move:** `lib/schemas/project.types.ts:64` — `allowAnyCharacter: z.boolean().default(false)` — the READ default for a stored bag missing the key. `public/schemas/qtap-export.schema.json` (`"allowAnyCharacter": { "type": "boolean" }`, no default; v5's copy `crates/quilltap-core/src/generators/qtap-export.schema.json:1411`). `migrations/scripts/cutover-projects-to-store.ts:234` (`!!row.allowAnyCharacter`). The slim DDL (`"allowAnyCharacter" INTEGER DEFAULT 0`). No migration flips existing projects — only NEW ones default open.

### A2 The roster policy doc comment

`projects.repository.ts:177-186` — the `canCharacterParticipate` JSDoc is rewritten (body unchanged):

```
   * The roster policy: may this character use their tools on the project's
   * files and shared wardrobe? (`allowAnyCharacter`, or on the roster.) It does
   * not govern who may chat in the project. Call sites go through
   * `projectRosterAdmits` in `lib/projects/roster-access.ts`.
   …
   * @returns Promise<boolean> True if the roster admits the character
```

### A3 The PUT answers the enriched project — `app/api/v1/projects/[id]/actions/project-crud.ts`

A new module-private `enrichProject(project, repos)` (`:24-68`) — the GET's former body moved VERBATIM: `chats.findAll()` filtered by `projectId`; `files.findAll()` filtered; per roster id `characters.findById` (a miss → `null`, filtered), `chatCount` = project chats whose `participants` include the id, `enrichWithDefaultImage(char.defaultImageId)`; the entry `{ id, name, defaultImageId, defaultImage, tags: char.tags || [], chatCount }`; the result `{ ...project, characterRoster: <entries>.filter(Boolean), _count: { chats, files, characters: project.characterRoster.length } }` (note: `_count.characters` counts the STORED roster, misses included). Its doc comment (quote it verbatim at the port): *"Every response the page swaps into its state must carry this shape, or the Characters card reads string ids as entries and shows an empty roster."*

`handleGetDefault` (`:73-89`) — unchanged behaviour, now `successResponse({ project: await enrichProject(project, repos) })` INSIDE its local try/catch (`serverError('Failed to fetch project')` + `logger.error('[Projects v1] Error fetching project', { projectId }, error)`).

`handlePutDefault` (`:94-116`):

```diff
   const project = await repos.projects.update(projectId, validatedData);
+  if (!project) {
+    return notFound('Project');
+  }
 
   logger.info('[Projects v1] Project updated', { projectId, userId: user.id });
 
-  return successResponse({ project });
+  return successResponse({ project: await enrichProject(project, repos) });
```

The PUT has NO local try/catch: an `enrichProject` throw propagates to `handleRouteError` AFTER the update committed (the write stands; the client sees 500/503). The `findById` → `notFound` pre-check and the uncaught `updateProjectSchema.parse` before the write are unchanged.

### A4 The roster auto-add removed — chat create

`app/api/v1/chats/route.ts:1134-1162` (target): `resolveProjectDefaults(repos, projectId)` loses its third parameter `participants: ChatParticipantBaseInput[]` and the whole `if (!project.allowAnyCharacter) { … repos.projects.update(projectId, { characterRoster: [...roster, ...newIds] }) }` block (baseline `:1161-1172`); its doc comment drops "and, as a side effect, the roster update …" and gains (verbatim at `:1139-1141`): *"The character roster is never touched here: it is a hand-curated access list (project file tools + shared wardrobe), edited only from the project's Characters card, so joining a chat must not grant access."* The one caller (`:1267`) drops `participantsWithTimestamps`.

### A5 The roster auto-add removed — chat PUT move-into-project

`app/api/v1/chats/[id]/helpers.ts:504-513` (`processChatUpdates`): the `if (!project.allowAnyCharacter) { … repos.projects.update(validatedData.chat.projectId, { characterRoster … }) }` block deleted, replaced by (`:511-512`): *"Moving a chat into a project never edits the roster — it is a hand-curated access list (lib/projects/roster-access.ts)."* The `findById` → `{ error: 'Project not found', status: 404 }` gate stays.

### A6 The vendored trees

`git diff --stat f6426e196 e5c6bd0c0 -- help/ docs/ README.md`:

| path | lines | commit |
|---|---|---|
| `help/project-characters.md` | +/− 251 (mostly −) | `9753d0eb2` |
| `help/project-chats.md` | 9 | `9753d0eb2` |
| `help/project-settings.md` | 28 | `9753d0eb2` |
| `help/projects.md` | 21 | `9753d0eb2` |
| `help/cli-migrations.md` | +8 | `e5c6bd0c0` |
| `docs/CHANGELOG.md` | +54 | both |
| `docs/developer/API.md` | 24 | both |
| `docs/developer/bugs.md` | 4 | `e5c6bd0c0` |
| `docs/developer/bugs/fixed/bug-175-collapse-failure-exits-process.md` | NEW, 179 lines | `e5c6bd0c0` |
| `docs/developer/bugs/fixed/bug-176-ledger-skips-shouldrun.md` | NEW, 188 lines | `e5c6bd0c0` |
| `README.md` | version badge only | both |

`help/` holds 129 files at BOTH pins (no add/delete — every row a modification). The `docs/developer/API.md` hunk of `9753d0eb2` documents (a) the `true` default and the roster's new meaning, (b) "Returns the same enriched project as `GET` (roster entries are objects, not ids)" on the PUT, (c) on `?action=add-character`: *"Archived characters are refused (400). This is the only way onto the roster; creating a chat does not add its participants."* — (c)'s first sentence documents the EXISTING route (`roster.ts:76-79` unchanged by the commit; `d553f72a` added it). No front-matter `url:`/`title:` line moved in any of the five help pages.

### A7 What the commit did NOT do (data half)

- No new route, no new action, no schema change to `updateProjectSchema` / `addCharacterSchema` / `removeCharacterSchema` (`app/api/v1/projects/[id]/schemas.ts`, untouched).
- The create POST's response is NOT enriched (`route.ts` returns the hydrated create echo, as before).
- `handleAddCharacter` / `handleRemoveCharacter` (`roster.ts:56-117`) untouched — still `{ success: true }`.
- No v4 test file covers the data half (the commit's tests are `roster-access.test.ts` and the two opacity mocks — P4.D245's). The oracle shapes are therefore the EXISTING route/repo families' cases (§C).

---

## §B v5's twins (on `main` `538fcbf65`)

### B1 The API default — `crates/quilltap-core/src/api/projects.rs:242-252`

```rust
    // createProjectSchema: allowAnyCharacter prefault false, characterRoster
    // prefault []. …
    let allow_any = obj
        .get("allowAnyCharacter")
        .and_then(Value::as_bool)
        .unwrap_or(false);
```

`validate_project_create` (`:196-223`) has already refused a present `null` (`PROJECT_CREATE_SCHEMA:187` — `nullable: false`), so `unwrap_or` sees only absent → the prefault. Bitten: a REST-less `projectCreate` with no flag answers `allowAnyCharacter: false` where v4 now answers `true`. The value is then ALWAYS inserted into `properties` (`:251`), so the repository seed (B2) never fires on this path — exactly v4's split.

### B2 The repository — `crates/quilltap-core/src/db/projects.rs`

- `ProjectsRepository::create` (`:259-276`) hands `input.properties` straight to the generic `StoreBackedRepository::create` with NO seeding; `parse_properties` (`:186-210`) → `serde_json::from_value::<ProjectProperties>` where `allow_any_character` is `#[serde(default)]` (`:73-74`) → **`false`** on an absent key, and an explicit `null` is a serde **error** (`invalid type: null, expected a boolean`), not v4's `?? true`.
- The `ProjectCreateInput` doc (`:233-238`) — *"mirrors v4's `prepareCreateData` seeding `allowAnyCharacter`/`characterRoster` — the schema defaults make the seeding redundant, reproduced here for free"* — is now FALSE and must be rewritten.
- The `#[serde(default)]` is ALSO the read default every overlay hydrate uses (`parse_properties` is "the one chokepoint every project property bag passes through — the overlay READ, the write overlay's read-modify-write serialize, and `write_managed_fields` on create", `:180-185`) — v4's unchanged `ProjectPropertiesSchema.default(false)`. **It must stay `false`**; the seed belongs in `ProjectsRepository::create` before the inner create, over the `properties` object (absent OR `null` → `true`; `characterRoster` absent/`null` → `[]`), which covers all three callers at once: `api/projects.rs:259`, `services/quilltap_import/entities.rs:515` (`fold_properties`, `:417-427`, copies present keys only — so an absent key stays absent and a `null` stays `null`), `services/backup/restore/orchestrator.rs:736` (`project_properties`, `:2278-2295`, present keys only).
- `can_character_participate` (`:365-385`) — `unwrap_or(false)` on a READ (correct, v4's schema default); its doc comment (`:364-365`) gets v4's A2 rewrite. P4.D245 READS this function and must find its body unchanged.

### B3 The PUT — `api/projects.rs:499-527`

`project_update` runs `find_by_id` (→ `NotFound`), `validate_project_patch` (→ 400), `repo.update` inside ONE `with_both_conns` writer closure and answers `Response::Project(json!({ "project": project }))` — **the RAW hydrated project** (`characterRoster` as string ids, no `_count`). The `None => ProjectUpdateOutcome::NotFound` arm (`:512`) already IS v4's new `if (!project) return notFound('Project')` — v5 was ahead at the baseline (v4 then answered 200 `{project: null}` on that race), so this half is a CONVERGENCE with no code. Reached by dispatch only: `Request::ProjectUpdate { project_id, project }` (`api/types.rs:1330-1335`, `#[serde(tag = "type", rename_all = "camelCase")]` → `{"type":"projectUpdate","projectId":…,"project":{…}}`) → `api/engine.rs:3255-3261`. **No REST route exists** (`crates/quilltap-web/src` has no projects route; `projects` appears only in `characters_routes.rs:1214` as a count). v5 does NOT log v4's `[Projects v1] Project updated` INFO (pre-existing; `[Projects v1]` appears only at `:1963`, `:2011`).

`project_get` (`:290-367`) is the enrichment v4 now shares — byte-equal to v4's `enrichProject` already (proven by `projects_routes_equivalence`'s `get_iota` "rich roster" + `get_kappa` "empty" rows): `project_chats` (`:106-112`, `chats_read::find_all` filtered — v4's `findAll().filter`), `FilesRepository::count_by_project_id` (v4 `files.findAll().filter(...).length` — same count), `characters_read::find_by_id` per id with a miss skipped, the present-non-null `defaultImageId` rule (`:330-337`), `enrich_with_default_image`, `tags` defaulting `[]`, `chatCount`; `obj.insert("characterRoster", …)` keeps the key's position (serde_json `preserve_order` — the JS spread-then-assign twin), `_count` appended. Its error arm maps everything to `internal("Failed to fetch project")` with `tracing::error!(error = %e, "project GET failed")` (v4 logs `'[Projects v1] Error fetching project', { projectId }` — pre-existing byte divergence on the line).

**Transaction trap:** `with_both_conns` (`services/image_job_common.rs:542-560`) runs the closure inside `db.write`. If the enrichment ran INSIDE that closure and returned `Err`, whether the update rolls back depends on `db.write`'s transaction semantics — where v4's write has already committed before `enrichProject` runs. The faithful shape is the v4 sequence: the write closure returns the stored project, THEN the enrichment runs as a separate read (`read_both`, `:77-83`), its error mapped through `db_error_response` (the PUT's propagate, NOT the GET's local 500).

### B4 Chat create — `services/chat_create.rs:1252-1323`

Step 6 "Project defaults + roster (v4 L1077-1113)": after the five defaults, `:1286-1320` reads `allowAnyCharacter` (`unwrap_or(false)`), collects CHARACTER participant ids not on the roster (deduplicated — v4 never deduped; moot now), and writes `repo.update(project_id, {characterRoster: merged})`. **Bitten.** Delete `:1286-1320` whole, keep the `find_by_id` → `HandleCreateError::NotFound("Project")` gate (`:1260-1262`), rename the step comment, carry v4's A4 doc sentence. `repo` stays used by the gate. (The `participants` vec is still needed later — only the roster read of it goes.)

### B5 Chat PUT move-into-project — `api/salon.rs` `chat_update` (`:1554-1660`)

**Never ported.** The doc comment's deferral list (`:1552-1556`): *"Deferrals (named in the report): the top-level `roleplayTemplateId`/`imageProfileId` shortcuts …, and the `projectId` `characterRoster` auto-add (a store-backed properties.json RMW)."* The `projectId` gate at `:1610-1619` uses `row_exists(conn, "projects", &id)` (a slim-row probe where v4 uses the overlay `findById` — pre-existing, not this commit). `ggrep -rn 'characterRoster\|character_roster\|add_to_roster' crates/quilltap-core/src` outside `api/projects.rs` / `db/projects.rs` finds NO other roster WRITE (`tools/project_info.rs` reads; `quilltap_import/reconcile.rs:464-469` remaps ids on import; `qtap_export/records.rs:493-523` reads for names; `backup/uuid_remap.rs:213` remaps). So v4's A5 deletion CONVERGES onto v5: the work is retiring the deferral sentence (and carrying v4's one-line why-comment at the gate). No behaviour moves.

### B6 The add/remove-character arms — `api/projects.rs:599-670`

`project_character_add` already refuses an archived character with v4's sentence byte-for-byte (`:613-618`, *"That character is archived; rehydrate them before adding them to a roster."*, `bad_request` → 400) — pinned by `projects_routes_equivalence`'s `add_character_archived` row (Rust `:1167-1174`, oracle `projects-routes.test.ts:579-584`). `project_character_remove` always writes, never 404s on a non-member. Both answer `{ success: true }`. Pre-existing gaps (not this commit): neither logs v4's `[Projects v1] Character added to project` / `… removed from project` INFO (`roster.ts:87,113`), and neither validates `characterId` as `z.uuid()` (`addCharacterSchema`/`removeCharacterSchema`, `[id]/schemas.ts:26-32`) — a non-uuid id answers v5 404 `Character not found` (add) / 200 (remove) where v4 answers 400 `Validation error`.

### B7 The vendored trees

`help/` (repo root) is byte-identical to v4 `help/` at `f6426e196` (`diff -rq` empty); against the target exactly the five A6 pages differ. Embedded by `crates/quilltap-host/build.rs:86-92` (`<repo>/help`); count literals `help_tree_embed_guard.rs:76` and `host_help_docs_boot.rs:105` = 129, UNMOVED. `docs/v4/` is byte-identical to v4 `docs/` at `f6426e196` except the standing residual `docs/v4/packages-quilltap-README.md` (byte-identical to v4 `packages/quilltap/README.md` at BOTH pins) and two git-IGNORED `.DS_Store`s (`docs/v4/developer/.DS_Store`, `docs/v4/developer/features/.DS_Store` — `git status --ignored` `!!`). Against the target exactly five paths differ: `CHANGELOG.md`, `developer/API.md`, `developer/bugs.md`, and the two NEW `developer/bugs/fixed/bug-17{5,6}-*.md`. **The v4 root `README.md` has no mirror** (the ledger's §3 row and the brief name "README" — refuted: only the version badge moved, and v5 mirrors only `packages/quilltap/README.md`).

Target md5s (for the lane's table): `help/cli-migrations.md` `de636267…`, `help/project-characters.md` `f25f11a2…`, `help/project-chats.md` `53198bd1…`, `help/project-settings.md` `6c79df0b…`, `help/projects.md` `2928b56d…`; `docs/CHANGELOG.md` `0436cea9…`, `docs/developer/API.md` `b6134e91…`, `docs/developer/bugs.md` `7272ca80…`, `bug-175-…` `6b4866f8…`, `bug-176-…` `fa4352b0…`.

---

## §C The differential families

| family (harness test ↔ oracle case) | what it pins | at `e5c6bd0c0` on unported main | after the port |
|---|---|---|---|
| `projects_routes_equivalence` ↔ `harness/oracle/cases/projects-routes.test.ts` (fixture: committed `crates/quilltap-web/tests/fixtures/groups-projects-{main,mount}.db`) | create echo; PUT echo; GET; roster add/remove; archived refusal | **RED on 9 rows by reading:** the five successful create rows that OMIT the flag — `create_blank_description` (`:372-378`), `create_whitespace_name`, `create_null_color_and_icon`, `create_name_astral_within_max`, `create_unknown_key_stripped` — echo `allowAnyCharacter: true` at the target; the four successful PUT rows — `update` (`:501`), `update_surviving_mode_latest_chat`, `update_unknown_key_stripped`, `update_clear_description` — gain the enriched `characterRoster` + `_count` (Iota's roster is the "rich" one `get_iota` pins). NEUTRAL: `create` (sends `true` explicitly), every 4xx create/update row, `get_*`, `add_character*`, `remove_character`, `update_store_corrupt` (the 503 fires in `find_by_id`, before enrichment), `get_store_corrupt` | GREEN |
| `projects_tier2_equivalence` ↔ `harness/oracle/cases/projects-tier2.ts` + `harness/oracle/fixtures/projects-tier2.json` (Rust drives `ProjectsRepository::create` itself via `split_create_input`, `:281-300`; the comparand is the FINAL-STATE dump, `documents` included) | the repository create + the `properties.json` bytes | **RED on exactly ONE cell by reading the op sequence:** all six `create` ops omit the flag, but only **Alpha**'s final `properties.json` keeps the seeded value (Beta is set `true` by `setAllowAnyCharacter` — the `beta_props` assert at `:485-494` already expects `true`; Gamma, Delta and Zeta are overwritten by `plantProperties`; Epsilon's is DELETED and then rewritten by an `update` RMW that seeds from the READ default — so Epsilon is the standing D3 guard that the read default stays `false`). Count it from the NDJSON at both pins before claiming it | GREEN |
| `chat_create_capstone_equivalence` ↔ `chat-create-capstone.test.ts` (fixture `/tmp/qt-cc-*.db` built by `harness/oracle/fixtures/build-chat-create-capstone.ts` AT THE PIN) | `handle_create` incl. 6 project-bearing cases (`outfit_default_tri_tier`, `outfit_llm_choose_shared_composite`, `rt_project_default_beats_user`, `rt_explicit_beats_both_defaults`, `sp_greeting_and_green_room_carry_block`; `vb_ok_project_id_nil_uuid` is the 404) | **VACUOUS as it stands** (§D1): the builder's project (`:378-386`) has no flag → `true` at the target → no auto-add on either side. AND possibly BLIND: the comparand dumps MAIN `projects` (`chat-create-capstone.test.ts:127`, Rust `:1080`, `:1160-1164`) while the roster lives in the store's `properties.json` — measure | GREEN; red-first only after §D1's two fixes |
| `help_tree_equivalence` ↔ `help-tree-sync.test.ts` (v4's REAL `ensureHelpDocsSynced` walking `$PIN/help`) | the whole vendored tree through the reconcile gate | **RED** (five content hashes / section chunks move) | GREEN after the byte-copy |
| `help_section_size_equivalence` ↔ `help-section-size.ts` | section chunking over the tree | RED or neutral by content — run and record | GREEN |
| `help_tree_embed_guard` (no oracle) | embedded == disk | GREEN before and after (blind to staleness by construction — memory note `help-embed-guard-cannot-see-a-stale-revendor`) | GREEN, 129 |
| `host_help_docs_boot` (`quilltap-host`) | 129 rows boot | GREEN, unmoved | GREEN |
| `help_context_resolver_equivalence`, `help_chat_orchestrator_tier3_equivalence`, `help_chats_routes_equivalence`, `help_docs_*` | url → page maps / committed `help-chat-main.db` | NEUTRAL by reading (no `url:`/`title:` moved; the committed fixture is not rebuilt) — run and record | — |
| `chats_messages_ops_tier2`, `chats_messages_tier2` | no `projectId` in their corpora | NEUTRAL | — |
| `quilltap_import_*` / `system_import_execute` / `system_restore` / `backup_*` | project rows from v4-written exports/archives | NEUTRAL by reading — v4 export/backup always materialize `allowAnyCharacter` (`findAll` hydrates through the schema default) so the seed never fires; run the import + restore families once and record | — |
| `dispatch_wrong_type_census` | 441 | UNMOVED (no verb added) | 441 |

No family needs a new oracle CASE file. One case EDIT and one builder EDIT (§D1, §D3); arms are added to the two project families (§E Tier 1).

---

## §D Traps and rulings

**D1 — The capstone red-first is vacuous unless the builder pins the flag, and may be blind regardless.** (a) `build-chat-create-capstone.ts:378-386` creates "The Lantern Project" with `characterRoster: []` and NO `allowAnyCharacter`; at `f6426e196` v4's seed bakes `false` (the auto-add fires on both sides — that is what the capstone has been comparing), at `e5c6bd0c0` it bakes `true` (neither side writes — green whether or not v5 is ported). Fix: pass `allowAnyCharacter: false` explicitly in the builder (identical at both pins). (b) Measure whether the capstone SEES the write at all: the roster write is a store-backed `update` that routes `characterRoster` to `properties.json` in the MOUNT db; the dump is MAIN-only. If the slim row's `updatedAt` (minted, not blanked in `table_rows`, `:494-504`) were bumped the family would already be flaky, so the likeliest reading is BLIND. Prove it with the M3 mutation at the BASELINE pin (delete v5's `:1286-1320` with the builder fixed; the baseline oracle still writes the roster): if the family stays green it is blind, and the lane adds a `projectRoster` comparand (the hydrated project's `characterRoster` + `allowAnyCharacter` read through `ProjectsRepository::find_by_id` on the Rust side and `repos.projects.findById` in the case) — then red-first at the target.

**D2 — The seed flips EVERY v4 fixture builder that creates a project without the flag (cross-lane).** `ggrep -l 'projects.create('` over `harness/oracle/` finds 39 files; 34 never mention `allowAnyCharacter` (e.g. `build-doc-opacity-fixture.ts:264-267`, `build-doc-edit-path-resolver-fixture.ts`, `build-wardrobe-routes-fixture.ts`, `build-context-tier3-fixture.ts`; `build-search-tools-fixture.ts:249-266` already sets it explicitly on both of its creates). Any of them rebuilt at the target pin bakes `allowAnyCharacter: true` where the baseline baked `false`. Both sides read the same fixture, so most families stay neutral — but **P4.D245's roster gate is exactly where this matters**: an off-roster arm over a target-built fixture is admitted by `allowAnyCharacter: true` and proves nothing. That lane must set the flag explicitly in every builder its families rebuild (a planning note for P4.D245 — not this lane's files).

**D3 — Keep the READ default.** Flipping `ProjectProperties::allow_any_character`'s `#[serde(default)]` to `true` would make every stored bag missing the key read open — v4's `ProjectPropertiesSchema.default(false)` did not move. The `projects_tier2` Epsilon sequence (`deleteProperties` then an `update` RMW) already pins it — M2 must turn it red; a `plantProperties` arm WITHOUT the key is the second, direct pin. Likewise `can_character_participate`'s `unwrap_or(false)` and `tools/project_info.rs:312`'s `== Some(true)` stay.

**D4 — Null.** v4's `?? true` treats `null` as absent; v5's serde rejects `null` outright. Through the API `null` is a 400 on both sides (unreachable). Through import/restore a `null` flag reaches the seed only if v4's importer does not validate the project against the export schema first — measure (`lib/import/quilltap-import/` validation path) before adding a corpus arm; a unit pin on the seed covers the semantics either way.

**D5 — The PUT enrichment's error class and placement** (§B3): run it AFTER the write closure, map its error through `db_error_response` (v4's PUT has no local catch — a store-unavailable read is the 503, not the GET's fixed 500). Do NOT route the PUT through the GET's `internal("Failed to fetch project")`.

**D6 — The SPA reads a key the server never sends (P4.D247's, named here for the contract).** `apps/web/src/app/core/core-contract.ts:4173-4182` types `ProjectDetail.characterRoster: string[]` plus an optional `roster?: ProjectRosterCharacter[]`, and `screens/prospero/cards/project-characters-card.ts:119` renders `this.project().roster ?? []` — but `project_get` sends the enriched entries under **`characterRoster`** and no `roster` key at all; the unit spec seeds `roster` (`projects.spec.ts:56,479,503`). So v5's Characters card shows an EMPTY roster on real data TODAY, on the GET — v4's bug in a different place. The enriched PUT does not break anything the SPA reads (it reads only `updated.allowAnyCharacter`, `project-characters-card.ts:143-150`; `project-detail.ts:369`, `project-image-generation-card.ts:267`, `project-model-behavior-card.ts:258` ignore the result). The SPA has no add-character helper (`projects.api.ts` has `removeProjectCharacter` only, `:89-95`) — the verb `projectCharacterAdd` exists (`core-contract.ts:1794-1798`, `api/types.rs:1348-1352`); **no new dispatch verb is needed this round.**

**D7 — The ledger row, checked.** Correct: the two default sites, the GET line `:352`, the auto-add at `chat_create.rs:1248-1310` (live range `:1286-1320`), "no roster write in v5's chat PUT". Refined: the chat-PUT half was not merely absent — it is a NAMED deferral (`salon.rs:1555`) that v4's deletion retires (CONVERGENCE). Refuted: "README" in the mirror list (no root-README mirror exists). Unmentioned: v5's `notFound` re-read arm already existed (CONVERGENCE); v5 has NO repository seed (the second default is an ADD, not a flip).

**D8 — Nothing needs the human.** No dispatch verb, no schema change, no DDL, no fixture pair rebuilt (the groups-projects pair is read, not rebuilt; the capstone's `/tmp` fixture is per-run).

---

## §E Ownership row (P4.D246)

**Edits:** `crates/quilltap-core/src/api/projects.rs` (the create default; the shared enrichment helper; `project_update`'s enriched answer; optional Tier 2 INFO lines); `crates/quilltap-core/src/db/projects.rs` (the `create` seed, the `ProjectCreateInput` doc, `can_character_participate`'s DOC COMMENT only — body unchanged); `crates/quilltap-core/src/services/chat_create.rs` (delete the auto-add); `crates/quilltap-core/src/api/salon.rs` (the `chat_update` doc comment's deferral sentence + one why-comment at the `projectId` gate — COMMENT ONLY); `crates/quilltap-harness/tests/{projects_routes,projects_tier2,chat_create_capstone}_equivalence.rs`; `harness/oracle/cases/{projects-routes.test.ts,projects-tier2.ts,chat-create-capstone.test.ts}`; `harness/oracle/fixtures/{projects-tier2.json,build-chat-create-capstone.ts}`; optionally a NEW `crates/quilltap-web/tests/project_update_dispatch_wire.rs`; `help/**` (five files, byte-copied); `docs/v4/**` (five paths, byte-copied); `docs/CHANGELOG.md` + `status-log.md` (append).

**Reads, never edits:** `crates/quilltap-core/src/doc_edit/path_resolver.rs`, `tools/**` (incl. `tools/project_info.rs`), `wardrobe_tiers.rs`, `tools/wardrobe_shared.rs`, `api/chat_outfits.rs` (all P4.D245); `apps/web/**` (P4.D247); `crates/quilltap-host/**`, `crates/quilltap-web/src/health.rs` (P4.D248); `api/types.rs` (FROZEN); `api/engine.rs` (no change needed — the verbs exist); `services/image_job_common.rs`; `services/quilltap_import/**`, `services/backup/**` (the seed covers them without an edit).

**Sibling seams:** P4.D245 READS `db/projects.rs::can_character_participate` — this lane changes its doc comment only, so the body P4.D245 reads is stable. P4.D245 owns the 34 builders' flag (D2) for its own families; this lane edits only `build-chat-create-capstone.ts`. P4.D248 owns `help/cli-migrations.md`'s CONTENT upstream but not the file in v5 — this lane re-vendors it.

---

## §F The §R.5 designed-reds sentence

**P4.D246:** on unported main at `e5c6bd0c0`, `projects_routes_equivalence` is RED on nine rows (the five flag-less successful creates echo `allowAnyCharacter: true`; the four successful PUTs answer the enriched project), `projects_tier2_equivalence` is RED on ONE cell (Alpha's final `properties.json`; the other five projects' final bytes are fixed by a later op), `help_tree_equivalence` is RED on the five re-vendored pages, and `chat_create_capstone_equivalence` stays GREEN at the target until the lane pins the builder's `allowAnyCharacter: false` (and, if measured blind, adds the roster comparand) — after which it is RED on the five project-bearing success cases; every other family is neutral to this lane.

---

## §G 💸 dogfood rows

1. Create a project in the SPA with no change to the toggle → it opens with **Allow Any Character ON** (`properties.json` reads `"allowAnyCharacter": true`); an existing project on the Friday copy reads as it did (no migration).
2. On a roster-locked project (Allow Any OFF): start a chat with a character NOT on the roster inside the project → the project's roster is UNCHANGED afterwards (the `properties.json` bytes before/after); move an existing chat into that project from the Salon → roster UNCHANGED.
3. Toggle Allow Any (a PUT) → the dispatch response's `project.characterRoster` is an array of `{id,name,…,chatCount}` objects and `_count` is present (read the response in the browser pane's network log) — the payload P4.D247's card consumes.
4. `help` search for "roster" lands on the rewritten `project-characters` page; `help/cli-migrations.md` shows the eight new lines.
5. The mirror: `docs/v4/developer/bugs/fixed/bug-175-*.md` and `bug-176-*.md` open.

---

## §H Proposed half of the Shared contract (P4.D246 → P4.D247)

All three verbs EXIST today (`api/types.rs:1330-1358`, engine arms `api/engine.rs:3255-3287`, SPA request types `core-contract.ts:1789-1800`); `api/types.rs` is not touched; the census stays 441. Wire = `POST /api/dispatch` with the `type`-tagged camelCase body; the SPA reads `data` through `CoreClient.dispatchData`.

1. **`projectUpdate`** — request `{ type: 'projectUpdate', projectId: string, project: <partial updateProjectSchema bag> }` (unchanged). Response AFTER this lane, on success: `{ project: P }` where `P` is the hydrated stored project (every key it has today, in today's order) with exactly two changes, byte-identical to `projectGet`'s `project`:
   - `characterRoster` (same key position) = an ARRAY OF OBJECTS, one per stored roster id whose character still exists, in roster order: `{ id: string, name: string, defaultImageId?: string /* OMITTED when the character has none */, defaultImage: EnrichedImage | null, tags: string[], chatCount: number /* project chats with this character as a participant */ }`; a roster id with no character is DROPPED.
   - `_count: { chats: number, files: number, characters: number }` appended LAST — `characters` is the STORED roster length (dropped misses still counted), so it may exceed `characterRoster.length`.
   Errors unchanged: 404 `Project not found` (absent before the write, or gone on the re-read); 400 `Validation error` (any `updateProjectSchema` failure, nothing written); 503 on a store-unavailable read (now also possible from the post-write enrichment — the write has then LANDED).
2. **`projectCharacterAdd`** — request `{ type: 'projectCharacterAdd', projectId: string, characterId: string }`; response `{ success: true }` (idempotent — an existing member writes nothing). Errors: 404 `Project not found`; 404 `Character not found`; 400 `That character is archived; rehydrate them before adding them to a roster.` (byte-exact, v4 `roster.ts:78`). The response does NOT carry the project — the SPA refetches `projectGet` (v4's `handleAddCharacter` hook does the same).
3. **`projectCharacterRemove`** — request `{ type: 'projectCharacterRemove', projectId: string, characterId: string }`; response `{ success: true }`, ALWAYS written (a non-member is not an error). Error: 404 `Project not found`.
4. **`projectGet`** — unchanged; its `project.characterRoster` is ALREADY the enriched array above. **The SPA's `ProjectDetail` type (`core-contract.ts:4181-4182`: `characterRoster: string[]`, `roster?: ProjectRosterCharacter[]`) and the card's `project().roster` read (`project-characters-card.ts:119`) do not match the server and never have** — P4.D247 retypes `characterRoster` as `ProjectRosterCharacter[]` (the existing interface, `:4159-4166`, already has the six fields; `defaultImageId` must become optional) and drops `roster`. `projectCreate`'s echo stays the RAW project (`characterRoster: string[]`, no `_count`) — v4 did not enrich the create.
5. **`allowAnyCharacter`** on a `projectCreate` with the key absent now answers `true`.
