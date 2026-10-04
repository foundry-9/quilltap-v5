# Survey — dogfood #136: explicit `null` in project / group `properties.json`

**Dated 2026-10-03.** v4 at `52d6e7ecd` (no project/group hunk between the
baseline `e5c6bd0c0` and it — the surfaces are identical at both pins). v5 at
`main` `29c4484e5`. Read by P4.146. The finding: `dogfood-findings.md` row 136
and "Standing notes for the next orders" item 1.

## §A — v4: the schemas and the null's three entry points

**A1 `ProjectPropertiesSchema`** `lib/schemas/project.types.ts:62-119`, in
schema order: `allowAnyCharacter` `.default(false)` (:64) · `characterRoster`
`.default([])` (:65) · `color` `HexColorSchema.nullable().optional()` (:68) ·
`icon` `z.string().max(50).nullable().optional()` (:69) ·
`defaultDisabledTools` `.default([])` (:73) · `defaultDisabledToolGroups`
`.default([])` (:75) · `defaultAgentModeEnabled` (:78) ·
`defaultAvatarGenerationEnabled` (:81) · `defaultImageProfileId` UUID (:84) ·
`defaultRoleplayTemplateId` UUID (:87) · `defaultAlertCharactersOfLantern
Images` (:90) · `answerConfirmationOverride` `z.enum(['ON','OFF'])` (:99) ·
`storyBackgroundsEnabled` (:103) · `staticBackgroundImageId` UUID (:105) ·
`storyBackgroundImageId` UUID (:107) — each of those eleven
`.nullable().optional()` — · `backgroundDisplayMode` `z.preprocess(…,
z.enum(['latest_chat','theme'])).default('theme')` (:116). **11 nullable-
optional keys, 5 defaulted.** Zod `^4.6.5` (`package.json:118`) keeps an
explicit `null` on a `.nullable().optional()` key and omits an absent one.

**A2 `GroupPropertiesSchema`** `lib/schemas/group.types.ts:44-48`: `color`
(:46), `icon` (:47), both `.nullable().optional()`.

Both overlays: `parseProperties: (v) => Schema.parse(v)`
(`lib/projects/project-store/overlay.ts:32-33`,
`lib/groups/group-store/overlay.ts:32-33`).

**A3 Write paths** (`lib/database/document-store-overlay.ts`):
- `writeManagedFields` :311-326 — `JSON.stringify(parseProperties(entity),
  null, 2)` (:316): create (`store-backed.repository.ts:156`), the startup
  backfills (`lib/startup/backfill-{project,group}-stores.ts:105`), the
  cutover migration (`migrations/scripts/cutover-projects-to-store.ts:329`,
  which wrote whole former rows — NULL columns became explicit nulls; the
  likely origin of LUC Ranch's eight).
- `applyWriteOverlay` :328-401 — read-modify-write: `current = (await
  readProperties(mp,id)) ?? parseProperties(entity)` (:383-384), `next =
  {...current}` (:385), each touched key copied verbatim incl. `null`
  (:386-388), `JSON.stringify(parseProperties(next), null, 2)` (:389),
  `writeDatabaseDocument` (:390).
- Formatting: 2-space indent, NO trailing newline; `writeDatabaseDocument`
  (`lib/mount-index/database-store.ts:108-170`) stores the string as given
  and takes the sha of those exact bytes (:140).

**A4 The three ways a null gets in on v4 today:**
1. **Create injects it:** `app/api/v1/projects/route.ts:74-75` `color:
   validatedData.color || null, icon: validatedData.icon || null` (so a
   project created with no colour, OR an empty-string colour, writes
   `"color": null, "icon": null`); `app/api/v1/groups/route.ts:89-90` the
   same for groups.
2. **PUT writes it:** `project-crud.ts:108` →
   `repos.projects.update(projectId, validatedData)`; `updateProjectSchema`
   (`app/api/v1/projects/[id]/schemas.ts:9-24`) has every property key
   `.nullable().optional()`.
3. **A stored bag carries it** (the cutover, the backfills, a v4-written file
   v5 later edits — LUC Ranch).

**A5 Read path** — `hydrateOne` :148-195 parses through the schema (:160) and
returns `{...row, ...properties, description, instructions, state}`
(:188-194): nulls SURVIVE to the wire. Routes spread the entity again
(`projects/route.ts:38`, `project-crud.ts:60/84/115` via `enrichProject`,
`groups/route.ts:65`, `group-crud.ts:34`). `readProperties` :266-309.
Home: `lib/services/home-data.service.ts:171-172` `color: project.color,
icon: project.icon` → a JSON `null` on the home wire.

**A6 v4 tests:** none exercises a null on a `properties.json` key
(`project-store-{read,write}-overlay.test.ts` cover null mounts and a
missing file; `create-project-schema.test.ts:26-27` only checks the schema
accepts `color: null`). No groups store-overlay unit test. **So the
equivalence proof must be a NEW corpus case, recorded through v4's real
overlay — there is no v4 test to mirror.**

## §B — v5: where the null is lost

**B1 `ProjectProperties`** `crates/quilltap-core/src/db/projects.rs:62-147`
(doc :62-70; the seam sentence :67-70 *"The null-vs-absent distinction on the
optional keys is the open-JSON seam (serde folds `null`→`None`); the corpus
keeps them present-or-absent."*). The eleven nullable-optional fields are
`Option<T>` + `default` + (rename) + `skip_serializing_if =
"Option::is_none"`: `color` (:78), `icon` (:80), `default_agent_mode_enabled`
(:90), `default_avatar_generation_enabled` (:96), `default_image_profile_id`
(:102), `default_roleplay_template_id` (:108), `default_alert_characters_of_
lantern_images` (:114), `answer_confirmation_override` (:123),
`story_backgrounds_enabled` (:129), `static_background_image_id` (:135),
`story_background_image_id` (:141). Related: `property_keys()` :160-179;
`parse_properties` :186-210 (rewrites `backgroundDisplayMode` only when
present ∧ non-null); `seed_create_properties` null-as-absent for the roster
defaults :458, :461 (P4.D246 — v4's `??`, correct, leave it).

**B2 `GroupProperties`** `crates/quilltap-core/src/db/groups.rs:27-41`
(`#[derive(Debug, Default, Serialize, Deserialize)]`; `color` :38, `icon`
:40, each `skip_serializing_if`, no explicit `default`); doc :31-34 names the
same deferral (*"an explicit `null` value is treated as absent here … a
tracked deferral"*); `parse_properties` :57-62; the ONE literal construction
from `GroupCreateInput` :117-120 (input fields :102-104).

**B3 The engine** `crates/quilltap-core/src/db/document_store_overlay.rs`:
`StoreEntity` docs :80-103 ("Optional fields use `skip_serializing_if`");
`hydrate_one` :293-381 spreads `serde_json::to_value(&properties)` over the
row (:361-367) — **the read-wire loss**; `read_properties` :563-608;
`serialize_properties` :612-617 (`parse_properties` → `to_string_pretty`,
2-space, no trailing newline — already v4's bytes); `write_managed_fields`
:631-655; `apply_write_overlay` :663-753 (read-modify-write; the seed comes
from `read_properties` → `to_value` (:721-733) which has ALREADY dropped the
nulls, the patch's null is inserted (:735-737) and dropped again by
`serialize_properties` (:738)) — **the write loss, twice**.

**B4 Blast radius — SMALL.** The typed bags live ONLY inside the overlay
engine; every consumer reads the hydrated `serde_json::Value`. **Direct
typed-field access: only `db/groups.rs:118-119`.** `ProjectProperties`
fields have ZERO non-serde accesses. The ~40 hydrated-`Value` consumers
mostly `.get(k).and_then(as_str/as_bool)` — null and absent alike, no change.
The ones that MOVE:
- `api/projects.rs:262-266` — create inserts `color`/`icon` only when
  non-empty (`or_null` :86-91); v4 writes an explicit `null` (A4.1).
- `api/groups.rs:278-279`, `:313-324` — the same on group create.
- `services/home.rs:348-349` — `HomepageProject.color/icon: Option<Value>`
  with `skip_serializing_if` (:133-137); `.get().cloned()` now yields
  `Some(Null)` → `null` appears on the home wire (v4 A5 emits `null` — a
  convergence; measure on the home family).
- `services/quilltap_import/entities.rs:603-609` — the group import's
  `opt_str_field` drops a null (v4's import runs `repos.groups.create` →
  `writeManagedFields` → keeps it); `:515` `fold_properties` (:417-427)
  already passes null through.
- Whole-entity emitters whose output may GAIN `null` keys (measure, do not
  presume — run the families): `services/qtap_export/{records.rs:483,546,
  entities.rs:118,123, preview.rs:173,186}`, `schema-key-order.json:275-283`,
  `services/backup/collect.rs:488,493`, `services/backup/uuid_remap.rs:
  208-211`.
- Unaffected (null-tolerant reads, verified): `services/chat_enrichment.rs:
  698`, `api/characters.rs:447-448` (`unwrap_or(Null)`), `services/
  chat_create.rs:1285,1288,1292`, `services/image_profile_resolution.rs:83`,
  `services/lantern_notifications.rs:149`, `api/salon.rs:317`,
  `services/chat_admin.rs:300`, `services/orchestrator.rs:1300`,
  `services/quilltap_import/reconcile.rs:476-487`.
- Pre-existing, OUT of scope: `services/backup/restore/orchestrator.rs:
  2280-2295` `project_properties` copies only six keys on restore (recorded,
  not this finding).

**B5 The three-state serde already exists.** `crates/quilltap-core/src/
services/mount_index/sync/types.rs:27-33` is a **pub** `double_option`
whose uses (:395-410, :505) pair `default, deserialize_with =
"double_option", skip_serializing_if = "Option::is_none"` on
`Option<Option<String>>` — the FULL round-trip shape (`Some(None)` → `null`,
`None` → omitted). Two private copies: `api/types.rs:4086-4097`,
`api/memories.rs:569`. No `serde_with` dependency.

**B6 The read wire has no projection struct** — `hydrate_one`'s `Value` IS
the wire: projects `project_list` `api/projects.rs:123-154`, `project_create`
:230-301, `enrich_project` :328-397, `project_get` :400-410,
`project_update` :549-590; groups `group_list` `api/groups.rs:111-165`,
`group_create` :295-342, `group_get` :352-365, `group_update` :428-447.

## §C — The corpora and the masks that hide the seam today

- Families (`crates/quilltap-harness/tests/`): `projects_tier2_equivalence`,
  `groups_tier2_equivalence`, `projects_routes_equivalence`,
  `groups_routes_equivalence`, `project_background_display_mode_equivalence`,
  `project_roster_access_equivalence`; plus `*_doc_mount_links_tier2`,
  `group_character_members_tier2`, `group_wardrobe_routes`,
  `mount_link_groups` (neutrality).
- Cases (`harness/oracle/cases/`): `projects-tier2.ts` (ops create, update,
  `plantProperties` raw bytes :136, deleteProperties, roster), `groups-
  tier2.ts` (plant :163), `projects-routes.test.ts`, `groups-routes.test.ts`,
  `project-background-display-mode.ts`, `store-unavailable-routes.test.ts`,
  `store-delete.test.ts`.
- **Corpora carry NO null cell on a nullable-optional key:**
  `harness/oracle/fixtures/projects-tier2.json` (22 ops; the only null is op
  18's `allowAnyCharacter: null`, a defaulted key) and `groups-tier2.json`
  (13 ops); planted bags (ops 11, 16, 20; groups 9) are present-or-absent;
  `groups-projects.json` (`build-groups-projects-fixture.ts:298-470`) has
  none.
- **The masks** (each a deliberate hide of THIS seam, to be lifted):
  `projects_routes_equivalence.rs:647-688` (`create_null_color_and_icon` —
  removes `color`/`icon` from BOTH bodies, then asserts only that v5's color
  is null-or-absent :684-687) and `:690-735` (`create_flag_absent_defaults_
  open` — the same removal); oracle-side notes
  `projects-routes.test.ts:355-357`, `:371-372`, `:387-391`; the `create`,
  `create_blank_description`, `create_whitespace_name` cases pass a real
  colour/icon to dodge it; `groups-routes.test.ts:319-389` — every
  successful create passes `color: '#abcdef', icon: 'gear'` for the same
  reason.
- `project-background-display-mode.ts:9` covers a null on a DEFAULTED key
  (`backgroundDisplayMode`, which v4 refuses) — not this seam.

## §D — The SPA is already null-tolerant (no SPA hunk)

Every consumer normalises null and absent alike (`||` / `??` /
`=== null || === undefined` — e.g. `screens/prospero/project-card.ts:26-29`,
`cards/project-header.ts:40-43`, `screens/home/project-item.ts:26`,
`screens/groups/group-card.ts:28-31`, `group-editor.ts:303-304`,
`cards/project-image-generation-card.ts:276-278`,
`project-model-behavior-card.ts:197-198`, `new-chat.state.ts:251`); contract
types already declare `T | null` (`core/core-contract.ts` `ProjectDetail`
:4174-4211). No e2e spec asserts null vs absent. **Consequence to expect:**
the group editor saves `color: this.color() || null` (`group-editor.ts:
337-338`), so once v5 keeps nulls a UI save with an empty colour writes an
explicit `null` — exactly v4.
