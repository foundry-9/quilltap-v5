# Survey — P4.148: the `.qtap` import's warning text + the projects/groups data-layer smalls

Surveyed 2026-10-05, read-only. v5 `main` @ `6c3a3c635`; v4 `~/source/quilltap-server`
@ `07b8f0209` (clean). Nothing was built or run. Oracle output quoted below comes from
the cached `/private/tmp/oracle-system-import-execute.ndjson` (mtime 2026-10-05 10:31,
**pin not verified**). The lane must regenerate it from its own pin before treating any
row as a red-first prediction.

---

## 1. Dogfood #140: the import's per-item warning tails

**Source** (`dogfood-findings.md` row 140 + standing note 1, line 645): *"A `duplicate`
import's per-message warnings read `Failed to import message in chat "…": sqlite error:
UNIQUE constraint failed: chat_messages.id` where v4's `msgError.message` is the bare
`UNIQUE constraint failed: chat_messages.id`; a group planted with `"color": 5` read
`Failed to import group "…": invalid type: integer \`5\`, expected a string` (serde)
where v4's `repos.groups.create` throws the ZodError's message."* The note asks for: a
`DbError` rendered through `db::fallback::error_text`, the zod-shaped tail measured per
kind, and a census guard like `fallback_home_guard`.

### 1a. Where the prefix comes from

- `DbError::Sqlite`'s `Display` adds `sqlite error: `. `db/fallback.rs:35-40`
  `pub fn error_text(&DbError) -> String` strips it: `Sqlite(e) => e.to_string()`,
  every other variant is already bare. #137 already uses it on the backup path
  (`services/backup/mod.rs:124`).
- In the import, the prefix reaches a warning by **two routes**:
  1. **Directly**: a `{e}` where `e: DbError` (or `OverlayError::Db(DbError)`, whose
     `Display` forwards; see `document_store_overlay.rs:231`).
  2. **Pre-stringified**: the error is turned into a `String` early with
     `.map_err(|e| e.to_string())`, and that string is pushed later. Count per file
     (`services/quilltap_import/`): characters 5, document_stores 2, entities 9,
     **files 5 through one chokepoint `err_msg` (`files.rs:48-50`, `e.to_string()`)**,
     mod.rs 16 (all in the preserve-ids preflight, `:795-923`), profiles 1, reconcile 21.
     Separately, `DbError::Internal(e.to_string())` wraps a serde error: entities 4,
     profiles 3. That variant is bare, so it carries serde text but no prefix.
- So fixing only the push sites misses half the class. The fix point is every
  `DbError → String` conversion in the directory.

### 1b. Census: every warning site in `services/quilltap_import/*.rs`

**What the tail is today:** **D** = `DbError` rendered directly. **D→S** = a `DbError`
stringified early, prefix kept. **O→S** = an `OverlayError` stringified early (the
prefix shows only on its `Db(Sqlite)` arm). **SER** = serde text, through
`DbError::Internal` or `String`. **V5** = a v5-only sentence. **Det** = deterministic,
no error tail.

**v4 counterpart** is under `lib/import/quilltap-import/`. Every v4 tail is
`error instanceof Error ? error.message : String(error)`.

| # | v5 site | Leading sentence | v5 tail | v4 counterpart | v5 WARN today vs v4 WARN |
|---|---|---|---|---|---|
| 1 | characters.rs:367 | `Failed to import character "{name}": ` | D→S (`:254,293,350` `to_string`) + SER (`:318,323`) | import-characters.ts:204 | v5 `:368` `character_id` vs v4 `:209` `characterId` |
| 2 | characters.rs:506 | `Wardrobe item "…" referenced N component item(s)…` | Det | :309 | — |
| 3 | characters.rs:552 | `Failed to import wardrobe item "{title}": character has no wardrobe vault` | **V5** | :331 (v4 throws `Cannot create wardrobe item: no Character Vault or Quilltap General mount is available. Wardrobe items are stored exclusively in the document store.`, `wardrobe.repository.ts:346`) | **no v5 WARN**; v4 `:336` `{wardrobeItemId, characterId, error}` |
| 4 | characters.rs:557 | same head, `{msg}` (Cycle) | v4's message already (`vault_wardrobe_public.rs`) | :331 | no v5 WARN |
| 5 | characters.rs:560 | same head, `{e}` | D | :331 | no v5 WARN |
| 6 | characters.rs:584 | `Failed to import plugin data for "{plugin_name}": ` | D | :370 | v5 `:587` `plugin_name, character_id` vs v4 `:375` `pluginName, characterId` |
| 7 | configuration.rs:106 | `Failed to import prompt template "{name}": ` | D | import-configuration.ts:73 | **no v5 WARN**; v4 `:78` `{templateId, error}` |
| 8 | configuration.rs:149 | `Failed to import provider model "{model_id}": ` | D | :114 | **no v5 WARN**; v4 `:119` `{modelId, error}` |
| 9 | configuration.rs:190 | `Plugin "…" was exported without its settings…` / `…imported without these secret settings…` | Det | :164 | — |
| 10 | configuration.rs:205 | `Failed to import plugin config for "{plugin_name}": ` | D | :173 | **no v5 WARN**; v4 `:178` `{pluginName, error}` |
| 11 | configuration.rs:235 | `Failed to import instance setting "{key}": ` | D | :214 | **no v5 WARN**; v4 `:219` `{key, error}` |
| 12 | document_stores.rs:271 | `Failed to import mount point "{mp_name}": ` | D | import-document-stores.ts:137 | v5 `:272` `name` matches v4 `:138` `{name, error}` |
| 13 | document_stores.rs:385 | `Failed to import folder "{path}": ` | D (`DmfCreate`, `doc_mount_folders.rs:234`) | :202 | v4 has none either |
| 14 | document_stores.rs:510 | `Failed to import document "{relative_path}": ` | D (`link_document_content_with_ids`) | :280 | — |
| 15 | document_stores.rs:525 | `Failed to restore hard link for group "{id}": ` | D (`bind_link_group`) | :294 | — |
| 16 | document_stores.rs:646 | `Failed to import blob "{relative_path}": ` | D→S (`:604,:635`) + V5 `invalid blob base64: …` (`:574`) | :356 | — |
| 17 | document_stores.rs:693 | `Failed to link project {p} to mount point {m}: ` | D | :390 | — |
| 18 | entities.rs:187 | `Failed to import tag "{name}": ` | D + SER (`:129,:167`) | import-entities.ts:74 | camelCase `tagId` ✓ |
| 19 | entities.rs:363 | `Failed to import roleplay template "{name}": ` | D + SER (`:344,:356`) | :172 | `templateId` ✓ |
| 20 | entities.rs:497 | `Failed to import project "{name}": ` | O→S (`:469,:477`) + `create_project` `:526` O→S | :239 | v5 `:498` `project_id` vs v4 `:244` `projectId` |
| 21 | entities.rs:585 | `Failed to import group "{name}": ` | O→S + **SER from `parse_properties`** (`:622`) | :304 | v5 `:586` `group_id` vs v4 `:309` `groupId` |
| 22 | entities.rs:725 | `Failed to import message in chat "{title}": ` | D (`add_message`) | :363 | — |
| 23 | entities.rs:728 | same head | SER (`ChatEventInput`) — v4's `addMessage` throws a ZodError (`ChatEventSchema` union) | :395 | — |
| 24 | entities.rs:736 | `Failed to import chat "{title}": ` | D→S (`:686,:701,:812`) + SER (`:792`) + Zod for the Concierge enums (`:786`) | :405 | `chatId` ✓ |
| 25 | files.rs:125 | `Failed to import folder "{path}": ` | D→S via `err_msg` | import-files.ts:88 | **no v5 WARN**; v4 `:93` `{folderId, path, error}` |
| 26 | files.rs:257 | `File "…" was exported without its contents and was skipped.` | Det | :205 | — |
| 27 | files.rs:389 | `Failed to import file "{original_filename}": ` | D→S via `err_msg` (+ nested `Failed to upload file '…': {err_msg}` `:302-306`) | :287 | **no v5 WARN**; v4 `:292` `{fileId, error}` |
| 28 | files.rs:398 | `{n} file link(s) pointed at entities…` | Det | :301 | — |
| 29 | memories.rs:73 | `Memory references non-existent character {id}` | Det | import-entities.ts:457 | — |
| 30 | memories.rs:159 | `Failed to import memory: ` | D | :513 | **no v5 WARN**; v4 `:518` `{memoryId, error}` |
| 31 | mod.rs:772, :944 | the preflight's collision message (pushed and returned) | Det | execute.ts:282, :293 | — |
| 32 | mod.rs:1040 | `Import refused before anything was written: {message}` | D→S (16 preflight `to_string`s) | execute.ts:506 | `Preserve IDs preflight failed` ✓ (`mod.rs:1034`) |
| 33 | mod.rs:1076 | `Import failed: {e}` | D (`import_body` → `Result<(), DbError>`, `mod.rs:1303`) | execute.ts:997 | — |
| 34 | mod.rs:1145 / :1232 | the two memory-embedding sentences | Det | execute.ts:359 / :393 | v4 `:354` `Imported memories left unembedded {userId, memoryCount, reason}` **absent in v5** |
| 35 | mod.rs:1456 | `Failed to import conversation annotation: ` | D | execute.ts:668 | — |
| 36 | mod.rs:1506 | `Failed to import chat document: ` | D | execute.ts:693 | — |
| 37 | mod.rs:1602 | `Dropped an imported inform: {reason}` | Det | execute.ts:756 | ✓ |
| 38 | mod.rs:1630 | `Failed to import inform: ` | D | execute.ts:778 | — |
| 39 | profiles.rs:227 | `Failed to import connection profile "{name}": ` | D + SER (`parse_connection_profile` `:245-248`) | import-profiles.ts:117 | `profileId` ✓ |
| 40 | profiles.rs:409 | `Failed to import image profile "{name}": ` | D + SER (`:391,:401`) | :182 | `profileId` ✓ |
| 41 | profiles.rs:520 | `Failed to import embedding profile "{name}": ` | D + SER (`:503,:512`, `parse_embedding_profile` `:546-549`) | :247 | `profileId` ✓ |
| 42 | reconcile.rs:153 | `Failed to remove the placeholder vault for an imported character: ` | D→S (`:97,:99,:116,:147`) | reconcile.ts:152 | v5 `:156` `character_id, scaffold_mount_id` vs v4 `characterId, scaffoldMountId` |
| 43 | reconcile.rs:281 / :317 | defaultImageId / avatar-override sentences | Det | :283 / :303 | — |
| 44 | reconcile.rs:369 | `Failed to reconcile character relationships: ` | D→S | :336 | `character_id` vs `characterId` |
| 45 | reconcile.rs:449 | `Failed to reconcile chat relationships: ` | D→S | :420 | `chat_id` vs `chatId` |
| 46 | reconcile.rs:498 | `Failed to reconcile project relationships: ` | O→S | :472 | `project_id` vs `projectId` |
| 47 | reconcile.rs:550 / :582 / :614 | connection / image / embedding profile relationships | D→S | :517 / :542 / :567 | `profile_id` vs `profileId` (three sites) |
| 48 | reconcile.rs:646 | `Failed to reconcile roleplay template relationships: ` | D→S | :593 | `template_id` vs `templateId` |
| 49 | seed.rs:60 / 82 / 104 / 145 / 158 / 181 / 212 / 221 | the seed-import report | mixed | v4 `seed-initial-data` (not `executeImport`) | **out of scope**: the report goes to a boot log, not an import result |

That is 50 `warnings.push` sites plus 2 `.push(format!` sites (entities.rs:725 and
:728), 52 in all. Of these, 41 are in the import proper; the rest are the seed's 9 and
the preflight's 2 returns.

**Leading sentences:** every v5 head matches v4's byte for byte (the quoted name
interpolations included). The only exception is #3, `character has no wardrobe vault`,
where the whole tail is a v5 sentence. **v4 makes that arm unreachable in practice**:
the vault is provisioned moments earlier by `create_character_with_options`.

**WARN lines v4 logs that v5 never has (9):** wardrobe item, prompt template, provider
model, plugin config, instance setting, files folder, file, memory, and `Imported
memories left unembedded`. `Failed to schedule vocabulary refit after import` is not in
the list: v5 never ported the refit (`mod.rs:1157-1176`, P4.D62 deferral).

**snake_case WARN fields still open (P4.143's OPEN item):** characters.rs:368 and :587;
entities.rs:498 and :586; reconcile.rs:156, :370, :450, :499, :553, :585, :617 and :649.
That is **12 lines**. The lane needs `rename-only` hunks.

### 1c. The two classes of tail

**(i) A DB/engine error.** v4's `error.message` is SQLite's bare sentence. Four tails
appear in the cached oracle today:
- `Failed to import folder "…": UNIQUE constraint failed: index 'idx_doc_mount_folders_mp_parent_name_nocase'`,
  in cases `execute_skip_all`, `execute_overwrite_all`, `execute_duplicate_all`,
  `execute_cross_instance_skip` and `route_replace_remap`.
- `Failed to import message in chat "A Lesson in Lift": UNIQUE constraint failed: chat_messages.id`,
  in `execute_duplicate_all` and `execute_chats_informs_duplicate`.
- `Failed to import file "atlas-plates.bin": Failed to upload file 'atlas-plates.bin': Project … has no linked database-backed document store. Run convert-project-files…`,
  in `execute_files_cross_instance`. This one is a `DbError::Internal`-class sentence,
  so it carries no prefix.

The harness masks all of them today (§5). Both engines run SQLite, and v5's indexes are
v4's DDL (`fresh_schema.json`). So once the `sqlite error: ` prefix goes, the tails
should agree **verbatim** and the mask can lift.

**(ii) A schema refusal.** v4 validates at the repository's Zod `validate`
(`base.repository.ts:130-141`), so the tail is `ZodError.message` =
`JSON.stringify(issues, null, 2)`. v4 pins zod `^4.6.5` and
`node_modules/zod/package.json` reads `4.6.5`. v5 decodes into a serde DTO first and
answers serde's sentence. These are the nine `SERDE_ARM_DIVERGENCES` rows
(`system_import_state.rs:674-731`) plus:
- the **group** (`color: 5`, the dogfood reference arm; `create_group` →
  `GroupEntity::parse_properties` → serde, entities.rs:622);
- the **project** (same class, but through `repo.create` — see the trap below);
- **messages** (#23);
- **characters** (#1, `:318/:323`).

The one place v5 already renders v4's Zod bytes is the chat's three Concierge enums
(`chat_override.rs:333-353` `concierge_columns_zod_error` →
`api::zod_issues::zod_error_message`).

The issue home is `crates/quilltap-core/src/api/zod_issues.rs`. It provides
`zod_error_message` (`:1065`, byte-identical to `JSON.stringify(…, null, 2)`),
`zod_group_issues` (`:571`, row columns only), `zod_chat_event_issues` (`:872`) and
`zod_uuid_ok` (`:519`). Constructors exist for `invalid_type`, `invalid_value`,
`invalid_uuid`, `too_big_string` and the others. **There is no `invalid_format`/`regex`
constructor**, and `HexColorSchema` (`common.types.ts:77`,
`z.string().regex(/^#(?:[0-9a-fA-F]{3}){1,2}$/)`) needs one. The
`zod_issues_home_guard.rs` census pins the home: a new constructor must live in
`zod_issues.rs`.

**Closing serde-vs-Zod for every kind is P4.143 Tier 3 item 12**: a committed,
GENERATED schema-shape table. That is too large for a smalls lane.

**Recommended scope (needs a ruling, see §Open questions):**
- Close the DB half whole.
- Close the **group + project properties** half through item 2(a). The
  `parse_properties` chokepoint returns zod issues, so the import tail, the PUT path
  and the overlay-read `properties.json unparseable: …` detail all converge at once.
- Leave the nine `SERDE_ARM_DIVERGENCES` rows and the message/character serde arms as
  recorded divergences.

**⚠ TRAP — the project import writes before it validates.** Path:
1. `create_project` (entities.rs:508-534) → `ProjectsRepository::create`
   (projects.rs:297)
2. → `StoreBackedRepository::create` (store_backed.rs:229-250)
3. → `create_slim` **INSERTs the row** (`:161-168`)
4. → `ensure_official_store`
5. → `write_managed_fields` → `serialize_properties` → `parse_properties` fails
   (document_store_overlay.rs:617-620, `properties parse: …`).

The import has no per-item SAVEPOINT (grep: none in `quilltap_import/` or
`store_backed.rs`). By reading, a bundle project with `color: 5` therefore leaves a slim
row plus a store **with no `properties.json`**, which every later read refuses (`properties.json
missing`, document_store_overlay.rs:316-324). v4's `_create` validates the whole entity
BEFORE any write (`store-backed.repository.ts:143-144`), so v4 writes nothing.

**NOT RUN** — the lane must reproduce this red-first. The group path does not have the
problem because `create_group` parses first (entities.rs:622).

### 1d. Predicted hunks

1. **Import directory only.** Add ONE local helper in
   `services/quilltap_import/mod.rs`, e.g. `pub(super) fn item_error_text(e:
   &DbError) -> String { crate::db::fallback::error_text(e) }`, plus an
   `OverlayError` twin that matches `OverlayError::Db(d)` → `error_text(d)`, other →
   `to_string()`. **Do not edit `db/fallback.rs`** (P4.149's file).
2. Replace each D / D→S / O→S rendering:
   - `files.rs` `err_msg`: one line fixes 5 sites.
   - `map_err(|e| e.to_string())` on a `DbError`/`OverlayError` in characters,
     entities, document_stores, reconcile and mod.rs (the preflight); about 53
     sites, each a mechanical swap.
   - The `{e}` on direct-`DbError` pushes: about 20 sites, each becoming `{}` +
     `item_error_text(&e)`.
   - The matching `tracing::warn!(error = %e …)`: render the same text.
3. The 12 snake_case renames. Optionally, the 9 absent WARN lines with v4's fields (a
   capture-pinned rider).
4. The project import validates its properties before `repo.create`, as
   `create_group` already does (entities.rs:520-524).

### 1e. Proof

- **`system_import_state.rs`** (`QT_ORACLE_SYSTEM_IMPORT_EXECUTE`, oracle
  `harness/oracle/cases/system-import-execute.test.ts`, 2,522 lines; fixtures
  `crates/quilltap-web/tests/fixtures/system-data-{main,mount,llmlogs}.db`).
  - Lift `mask_warning`'s `QUOTED_FAMILIES` mask (`:605-623`, applied at `:965-1019`) so
    an engine tail compares VERBATIM.
  - **Red-first on unported main**: the six cases above, on the `sqlite error: ` prefix.
  - Keep the `SERDE_ARM_DIVERGENCES` carve as is.
  - `COLON_FAMILIES` (`:626-637`) has no oracle row today, so lifting it is vacuous
    without a plant.
- **New oracle cases, by payload construction** (the `malformedItemsPayload` recipe,
  test.ts `:1290-1360`), with no fixture rebuild:
  - `execute_group_property_refusals`: a group with `color: 5`, `color: "red"`, a
    51-char `icon`;
  - `execute_project_property_refusals`: the same over a project, plus a non-uuid
    `defaultImageProfileId`.

  Expected: a v4 warning carrying a ZodError tail, and **no row** in `groups` /
  `projects` / `doc_mount_points` on either side. The project case is the red-first for
  the TRAP.
- **A new census guard** `crates/quilltap-harness/tests/import_warning_text_guard.rs`,
  copying `fallback_home_guard.rs` (136 lines) and its `mod source_census` with
  `production_zone` / `string_literals`. It scans
  `services/quilltap_import/*.rs` minus `seed.rs` and refuses:
  - `map_err(|e| e.to_string())`;
  - `fn err_msg … e.to_string()`;
  - a `"Failed to import …: {e}"` / `"…relationships: {e}"` literal whose argument is
    not routed through the helper. The lexer can't type-check, so allowlist literals
    that interpolate a known-bare value (`{msg}`, `{reason}`).
- **WARN fields:** a unit `captured_with` pin per renamed line, using the field-order
  recipe of P4.143 item 9.

### 1f. Fixtures

No committed fixture moves. `system-data-main.db` has **22 readers**, including
`system_backup_equivalence`, `system_export_equivalence`,
`system_delete_data_equivalence`, `qtap_schema_validate_equivalence`, `reconcile.rs`
and 8 oracle cases. **It must not be regenerated.** New cases build their payload in the
test.

---

## 2. P4.146's OPEN items

Source: `work-orders/p4.146-properties-json-null-preservation.md` line 5,
**Unification**.

### 2(a) `parse_properties` checks no hex / 50-char icon / UUID

*"`GroupEntity::parse_properties` / the project twin check no hex pattern, 50-char icon
limit or UUID format, so an import of `"color": "red"` passes both where v4 refuses
it."*

**v5:**
- `db/groups.rs:67-72`: object check, then `serde_json::from_value` into
  `GroupProperties` (`:37-50`, two `Option<Option<String>>`).
- `db/projects.rs:209-235`: object check, the `backgroundDisplayMode` normalize, then
  serde into `ProjectProperties` (`:78-180`). `answerConfirmationOverride` is a plain
  `String` (`:84`) with **no ON/OFF enum check either**.

**v4:**
- `group.types.ts:43-46`: `color: HexColorSchema.nullable().optional()`,
  `icon: z.string().max(50).nullable().optional()`.
- `project.types.ts:62-119`: `characterRoster: z.array(UUIDSchema)`,
  `color`/`icon` as for groups, and `defaultImageProfileId`,
  `defaultRoleplayTemplateId`, `staticBackgroundImageId`, `storyBackgroundImageId` as
  `UUIDSchema.nullable().optional()`.
  `answerConfirmationOverride: z.enum(['ON','OFF']).nullable().optional()`.
- `HexColorSchema` (`common.types.ts:77`) = `z.string().regex(/^#(?:[0-9a-fA-F]{3}){1,2}$/)`.
  `UUIDSchema` = `z.uuid()`.
- `parseProperties` = `XPropertiesSchema.parse` (`project-store/overlay.ts:33`,
  `group-store/overlay.ts:33`). It runs at **every** site:
  - the read overlay (`document-store-overlay.ts:160`, throws
    `properties.json unparseable: ${err.message}`);
  - `readProperties` (`:295`);
  - `writeManagedFields` (`:316`);
  - the update RMW (`:384,:389`).

**Every v5 caller of `parse_properties`** (11 hits):
- `document_store_overlay.rs:334` (overlay read → `Unavailable`), `:598`
  (`read_properties`), `:618` (`serialize_properties` = create + RMW write), `:731`
  (RMW seed);
- `services/quilltap_import/entities.rs:622` (group import);
- tests: groups.rs:394; projects.rs:570, 579, 581, 703, 727, 776, 778;
- harness `project_background_display_mode_equivalence.rs:104`.

Through those, the blast radius is every project/group read, create, PUT, import, and
restore (the restore via `repo.create` in P4.147's `orchestrator.rs:751-766`).

**Divergence:** v5 accepts — and on READ hydrates — a `properties.json` with a non-hex
colour, an over-long icon, a non-uuid id or a non-ON/OFF override. v4 refuses all of
these on every path.

**Hunks:**
- `parse_properties` gains post-decode checks that yield `ZodIssue`s in schema order,
  rendered with `zod_error_message` as the `Err(String)`.
- `api/groups.rs:225` `is_valid_hex_color` already exists; move it, or call one shared
  predicate.
- The `invalid_format` regex issue needs a NEW constructor in `api/zod_issues.rs`.
  Measure its key order at the pin with the `zod_issues.rs` header's node recipe, e.g.
  `show("invalid_format/regex", z.object({a: z.string().regex(/^#(?:[0-9a-fA-F]{3}){1,2}$/)}), {a:"red"})`.
- Icon length is counted in code points: `jsstr::zod_len_max_ok`.

**Proof:**
- `projects_tier2_equivalence.rs` / `groups_tier2_equivalence.rs` (corpora
  `harness/oracle/fixtures/projects-tier2.json`, `groups-tier2.json`; oracles
  `projects-tier2.ts`, `groups-tier2.ts`). Add parse-refusal cells recorded through v4's
  REAL `XPropertiesSchema`.
- `project_background_display_mode_equivalence` must stay green.
- The §1e import cases.

**Fixtures:** `projects-tier2.json` and `groups-tier2.json` each have exactly one reader
pair (their oracle + their family). Safe to grow.

**⚠ Risk / ruling:** tightening the READ path makes any already-stored invalid bag
unreadable. That is v4's behaviour, but a v5-written bag from an import before the fix
could carry `color: "red"`. Ask whether to tighten reads, or only writes and imports.

### 2(b) `double_option`: a db → services dependency

**v5 today:**
- `db/groups.rs:24` and `db/projects.rs:23` both
  `use crate::services::mount_index::sync::types::double_option;`.
- The public definition is `services/mount_index/sync/types.rs:27-33`, used there at
  `:399, :408, :505`.
- Private copies: `api/types.rs:4098-4104`, **68** `deserialize_with = "double_option"`
  uses; `api/memories.rs:569-575`, 2 uses, spelled `Ok(Some(Option::deserialize(de)?))`
  and equivalent.
- The harness imports the services path once (`groups_tier2_equivalence.rs:72`). Five
  harness tests keep private copies (`chat_message_fts_reconcile`,
  `chat_message_fts`, `doc_mount_write_metadata`, `embedding_profiles_tier2`,
  `connection_profiles_tier2`). Leave those alone.
- `db/document_store_overlay.rs:90` mentions it in a doc comment only.

**Proposed home:** `crates/quilltap-core/src/db/serde_tristate.rs` (new; `pub mod` in
`db/mod.rs`), holding `pub fn double_option`.

**Repoint list:**
- `db/groups.rs:24`, `db/projects.rs:23`, `groups_tier2_equivalence.rs:72`;
- `services/mount_index/sync/types.rs`: replace the fn with `pub use
  crate::db::serde_tristate::double_option;`, so its three uses and any outside path
  keep compiling. That is a one-line edit in a file P4.146 was fenced from — confirm no
  sibling lane owns it this round;
- optional, the same move: `api/types.rs` (delete `:4093-4104`, add a `use`) and
  `api/memories.rs:568-575`.

**Proof:** none is behavioural. The existing tier-2 families stay green. An optional
unit test pins the absent/null/value triple once, in the new home.

### 2(c) `create_with_properties` silently ignores `GroupCreateInput.color/icon`

- **v5:** `db/groups.rs:140-161`; the doc comment says "The bag SUPERSEDES
  `input.color`/`input.icon`, which are not read". Its callers, `api/groups.rs:318-337`
  and the import at entities.rs:603-633, both pass `color: None, icon: None`. The plain
  `create` (`:126-136`) builds the bag from them.
- **Hunk choices:**
  - (i) `debug_assert!(input.color.is_none() && input.icon.is_none())` in
    `create_with_properties`. Zero cross-file edits.
  - (ii) drop the two fields from `GroupCreateInput` and give `create` its own args.
    That touches `services/backup/restore/orchestrator.rs:753-760` (**P4.147's file**)
    and `document_store_overlay.rs` tests `:1078,:1112`.
- **Recommend (i)**, unless P4.147 converts the restore to `create_with_properties`
  (P4.146 item 13 asks it to). Then `create` has no production caller and (ii)
  dissolves 2(c) and 2(d) together.

### 2(d) Plain `GroupsRepository::create` has no differential

- **v5 production callers of plain `create`:** exactly one,
  `services/backup/restore/orchestrator.rs:765` (P4.147).
- **Recommend:** leave this to P4.147, which owns the only caller and is asked to
  replace it. Otherwise add a db-level unit arm in `db/groups.rs` tests.

### 2(e) `create_null_colour` passes `None` and never exercises wire `null`

- **v5:** `groups_routes_equivalence.rs:726-739` calls `groups::group_create(&db, …,
  None)` directly. `Request::GroupCreate` (`api/types.rs:826-838`) has `color:
  Option<String>` plain serde, so wire `null` → `None` (fine: v4's `|| null` stores
  `null` for both).
- **Hunk:** test-only. The `create_null_colour` row decodes
  `json!({"name":…,"color":null})` through `serde_json::from_value::<Request>` and
  dispatches (precedent: `character_rename_equivalence.rs`,
  `chat_scenario_routes_equivalence.rs`).
- **Oracle:** unchanged; it already sends `null`.

### 2(f) The projects routes `check` never compares the 201

- **v5:** `projects_routes_equivalence.rs:335-348` compares `oracle[name]["body"]` only.
  The oracle records `status` for every row (`projects-routes.test.ts:131,179-181,209`).
  The create rows are at `:617-771`, about 10 rows.
- **groups_routes_equivalence.rs has the same gap** (no `201` anywhere in the file).
- **Hunk:** each success-create row asserts `oracle[name]["status"] == 201` and that v5
  answered a success variant. The precedent is `projects_routes_equivalence.rs:1302-1311`
  (`assert_eq!(oracle[name]["status"].as_i64(), Some(200), "{name}: v4's status")`).
  The v5 status lives at the web edge, not in `Response`, so the pin is "v4 says 201 and
  v5 is not an `Error`".

---

## 3. P4.143's import-side OPEN items

Source: `work-orders/p4.143-data-zod-smalls-import-mask-restore-serde-trail-errors.md`
line 3, **Unification**, the "OPEN by name" list.

### 3(a) The IMPORT's DB-error arm

*"the IMPORT's DB-error arm (`Error creating entity` / `Failed to create chat` on a
SQLite failure inside `_create`, not only on validation — item 14 named only
restore's)"*

- **v5:** `services/quilltap_import/entities.rs:804-812`, `repo.create(&create, …)
  .map_err(|e| e.to_string())?`. No repository line is logged. The validation arm
  (`:786-797`) does log, through
  `chat_override.rs:384-410 log_chat_create_validation_failure`: `Data validation
  failed`, then strict-gated `Error creating entity` + `Failed to create chat` with
  `strictFailures=true`.
- **v4:** `chats.repository.ts:233-281`, `create` = `safeQuery(…, 'Failed to create
  chat', {})` around `_create`. On a SQLite throw (no validate failure) v4 logs the
  `_create` line and `Failed to create chat`, then the caller's WARN. **The exact pair
  and whether `Data validation failed` is absent: NOT MEASURED.** Measure with the
  existing `Logger.prototype` spy (P4.143 item 10, in `system-import-execute.test.ts`)
  on a planted UNIQUE collision. The `duplicate` chat case already collides on
  messages, not on the chat row; a chat-id collision needs `preserveIds` plus a planted
  row.
- **Hunk:** a sibling `log_chat_create_db_failure(&error_text)` beside
  `log_chat_create_validation_failure` in `services/dangerous_content/chat_override.rs`,
  called at entities.rs:811. ⚠ `chat_override.rs` may be owned elsewhere; if so, put the
  helper in the import module.
- **Proof:** `system_import_state`'s `repoLogs` comparand (`REPO_LOG_CASES`, `:940-947`
  region). One new planted case.

### 3(b) snake_case WARN fields

The 12 sites are listed in §1b. v4's names: `characterId`, `pluginName`, `projectId`,
`groupId`, `scaffoldMountId`, `chatId`, `profileId`, `templateId`. Done inside §1.

### 3(c) Item 11: every other fallback home honouring the strict scope

*"item 11 now NARROWER — the scope exists; what remains is every OTHER fallback home
honouring it"*

- **Importer call sites under `with_strict_repository_failures`:**
  `services/quilltap_import/mod.rs:977` (`execute_import`) and `preview.rs:85`
  (`preview_import`).
- **Homes that read the flag today:** only `db/fallback.rs:412` and `:445` (the two
  overlay batch reads), plus `chat_override.rs:391`.
- The importer calls no `fallback::` fn directly (grep). It reaches the homes through
  the repositories.
- **This is P4.149's (`db/fallback.rs` + the repos).** Not proposed here.

---

## 4. P4.D246's OPEN items on the projects routes

Source: `work-orders/p4.d246-project-roster-data-half-help-mirror.md` line 3,
**Unification**, plus Tier 3 `:256-268`.

### 4(a) Item 16: the add/remove-character `z.uuid()` gate

- **v5:** `api/projects.rs:679-728` `project_character_add` and `:730-768`
  `project_character_remove`. `character_id: String` has no format check.
  - Add: 404 `Character` for a non-uuid.
  - Remove: 200 and an always-written update.
- **v4:** `app/api/v1/projects/[id]/actions/roster.ts:56-90, 95-115`.
  - `repos.projects.findById` → 404 `Project` FIRST.
  - Then `addCharacterSchema.parse(body)` (`schemas.ts:26-32`, `characterId: z.uuid()`),
    uncaught → `handleRouteError` (`lib/api/middleware/context.ts:166-167`) →
    `validationError(err)` = 400 `{error:'Validation error', details:[issue]}`.
  - The issue is `invalid_format`/`uuid` at `["characterId"]`, which v5 already has as
    `ZodIssue::invalid_uuid`.
  - Reachable: yes.
- **The same gate is missing on four sibling ops:**

  | v4 schema | v4 parse site | v5 handler |
  |---|---|---|
  | `addChatSchema` / `removeChatSchema`, `chatId: z.uuid()` (`schemas.ts:34-40`) | `chats.ts:143`, `:173` | `project_chat_add` `api/projects.rs:889`, `project_chat_remove` `:916` |
  | `addFileSchema` / `removeFileSchema`, `fileId: z.uuid()` (`:42-48`) | `files.ts:128`, `:158` | `project_file_add` `:1988`, `project_file_remove` `:2020` |

  Same order (project 404, then parse). Recommend ONE helper for all six:
  `uuid_gate(field, value) -> Option<Response>` using `zod_uuid_ok` +
  `Response::validation_error(zod_issue_details(..))` (`api/types.rs:4644`).
- **A non-string id** never reaches the handler: `Request::ProjectCharacterAdd`
  (`types.rs:1348-1357`) types it `String`, so it is a dispatch decode error. That is
  the `dispatch_wrong_type_census` class. Out of scope; say so.
- **Proof:** `projects_routes_equivalence` + `projects-routes.test.ts`. Six new 400
  rows, each with a body + `details` compare, and a silence leg (no `[Projects v1]` INFO).

### 4(b) Item 17a: the GET's error-line bytes

- **v5:** `api/projects.rs:418-422`, `tracing::error!(error = %e, "project GET
  failed")`. `e` is a `DbError`, so the line carries the `sqlite error: ` prefix too.
- **v4:** `project-crud.ts:85-87`, `logger.error('[Projects v1] Error fetching
  project', { projectId }, error)`, then `serverError('Failed to fetch project')`.
- **Hunk:** `tracing::error!(projectId = %project_id, error = %error_text(&e),
  "[Projects v1] Error fetching project")`. This copies the `api/salon.rs:83-91`
  precedent (`chat_get_failed`, "the Error as the third argument → `error = %e`, the
  file layer's hoisted convention").
- **Proof:** the routes family's `get_store_corrupt` arm (`:1430-1450`) gains a line
  capture.
- **Related, not ordered:** v5 lacks **19** of v4's 25 `[Projects v1]` lines. Examples:
  `Chat added/removed`, `File added/removed`, `Project deleted`, `Error listing project
  characters|chats|files`, `Default tool settings updated`, `Mount point
  linked/unlinked`, `Project aesthetic updated`, `Created project scenario`,
  `Error fetching projects`. A census rider, if wanted.

### 4(c) Item 17b: the chat PUT's slim `row_exists` project gate

- **v5:** the order says `api/chats.rs`, but **that file does not exist**. The gate is
  `api/salon.rs:1613-1625`, `db.read_main(|c| row_exists(c, "projects", &id))`
  (`row_exists` `:1868-1885`, slim `SELECT 1`, and "no such table" → false). A DB error
  answers `internal(e)` (500, prefixed text).
- **v4:** `app/api/v1/chats/[id]/helpers.ts:504-509`, `repos.projects.findById(id)`
  (store-backed). The `_findById` fallback answers a DB read error as null → 404.
  `applyOverlayOne` throws on a broken store → propagates → `handleRouteError`
  (`context.ts:176-185`) → 503 `{error:'Project document store unavailable',
  projectId}`.
- **Divergence:** for a project whose store is broken, v4 answers 503 and v5 accepts the
  move. On a DB read error, v4 answers 404 and v5 answers 500.
- **Hunk:** one arm in `api/salon.rs`: `ProjectsRepository::find_by_id` via
  `read_both`, with the overlay error mapped through the existing
  `overlay_to_db`/`db_error_response`. **`salon.rs` is a hot shared file**; confirm no
  sibling owns it, or defer.
- **Proof:** the chat PUT route family (not surveyed here; grep `chat_update` in the
  harness), with a corrupt-store plant like `get_store_corrupt`.

### 4(d) The Scenarios-folder ensure discards its `Err`

- **v5:** `api/projects.rs:282-287`, `let _ = links.ensure_folder_path(mp,
  "Scenarios");` inside the writer closure. The INFO `[Projects v1] Project created`
  is emitted after the closure (`:298-302`).
- **v4:** `app/api/v1/projects/route.ts:82-100`: INFO `Project created`, THEN
  try/`ensureProjectScenariosFolder`/catch →
  `logger.warn('[Projects v1] Failed to ensure project Scenarios folder on create', {
  projectId: project.id, error: ensureError.message })`.
- **Hunk:** the closure returns `(project, Option<DbError>)`. After the INFO, warn
  `projectId`, `error = %error_text(&e)`. This keeps v4's INFO-then-WARN order without
  moving the ensure out of the writer.
- **Proof:** a plant that makes the folder ensure fail (e.g. a pre-existing
  conflicting `Scenarios` row), or a unit capture. Reachability in a clean instance is
  low.

### 4(e) The missing-character roster arm

- **v5:** `enrich_project` skips a missing character with `continue`
  (`api/projects.rs:347-348`). `_count.characters` is the raw roster length. No row in
  either family exercises it.
- **v4:** `project-crud.ts:34-66` (`filter(Boolean)`; `_count.characters:
  project.characterRoster.length`) and `roster.ts:30-46` (list-characters filters too).
- **Proof:** one new oracle + Rust row pair in projects_routes. PUT `characterRoster:
  [<real>, <well-formed missing uuid>]` (`updateProjectSchema` accepts any uuid), then
  GET and list-characters. Expect `_count.characters == 2` and one entry. Staged
  through the API, so **no fixture rebuild**. `groups-projects-main.db` has 18
  readers.

### 4(f) The `test_support` `FieldVisitor` doc comment

- **v5:** `crates/quilltap-core/src/test_support.rs:42-57` describes `"<LEVEL> <target>
  <field>=<value> … "<message debug>""`. The rig actually renders the MESSAGE first,
  unquoted (`record_debug` `:76-82`), then the fields. Source: status-log.md lines 163351
  and 163397.
- **Hunk:** doc-only. ⚠ `test_support.rs` is shared by every lane; a doc-comment-only
  edit should merge cleanly, but name it.

### 4(g) (noted, theoretical) `Promise.all` vs in-order enrichment

v4 enriches in parallel; v5 enriches in order. With two broken vaults, the 503's
`characterId` could differ. No hunk proposed.

---

## 5. The harness today

| Family | Oracle case | Fixture(s) | Warnings handling |
|---|---|---|---|
| `system_import_state.rs` | `system-import-execute.test.ts` | `quilltap-web/tests/fixtures/system-data-*.db` (22 readers) | `mask_warning` `:965-1019`: `QUOTED_FAMILIES` (`:605-623`) mask the tail to `<ENGINE>` **unless** it is a ZodError message (P4.143's exception); `COLON_FAMILIES` (`:626-637`) always masked; `Import failed:` masked except V8's `Cannot read properties`; `SERDE_ARM_DIVERGENCES` (9 rows, `:674-731`) carved both ways with an exercised-count assert (`:1769-1781`). **To lift for #140:** drop the `<ENGINE>` arm for `QUOTED_FAMILIES` (and `Failed to link project`). Six cases go red on unported main, green after. |
| `system_import_equivalence.rs` | `system-import.test.ts` | system-data-* | Preview/read side. `mask_parser_text` (`:203`) masks V8-vs-serde JSON-parse text only. Unaffected. |
| `qtap_import_equivalence.rs` | `qtap-import.ts` | `qtap-import-tier2.json` + `qtap-import-bug*.qtap` | Compares `warnings` VERBATIM for bug75/117/158 (`:547-561, :725, :805`). No engine tails. Must stay green. |
| `projects_routes_equivalence.rs` | `projects-routes.test.ts` (900 lines) | `groups-projects-{main,mount}.db` (18 readers) | Bodies only on success. Items 2(f), 4(a), 4(b), 4(e). |
| `groups_routes_equivalence.rs` | `groups-routes.test.ts` (705 lines) | groups-projects-* | Items 2(e), 2(f). |
| `projects_tier2_equivalence.rs` | `projects-tier2.ts` | `projects-tier2.json` (1 reader pair) | Item 2(a). |
| `groups_tier2_equivalence.rs` | `groups-tier2.ts` | `groups-tier2.json` (1 reader pair) | Items 2(a), 2(b) (its `:72` import). |
| `fallback_home_guard.rs` | — | — | The pattern for the new `import_warning_text_guard.rs`. |
| `zod_issues_home_guard.rs` | — | — | Moves if a new `ZodIssue` constructor (regex) is added. Constructors must live in `api/zod_issues.rs`. |

---

## §Ownership proposal

**P4.148 edits (v5):**
- `crates/quilltap-core/src/services/quilltap_import/{characters,configuration,document_stores,entities,files,memories,mod,profiles,reconcile}.rs`.
  `preview.rs`, `seed*.rs`, `reset.rs`, `ndjson.rs` and `legacy_presets.rs` are not
  needed.
- `crates/quilltap-core/src/db/projects.rs`, `db/groups.rs` (`parse_properties`,
  the `double_option` import, the `create_with_properties` assert).
- NEW `crates/quilltap-core/src/db/serde_tristate.rs` + one `pub mod` line in
  `db/mod.rs`.
- `crates/quilltap-core/src/services/mount_index/sync/types.rs`: ONE line (`pub use`
  in place of the fn).
- `crates/quilltap-core/src/api/projects.rs`: the GET error line (`:418-422`), the
  Scenarios WARN (`:282-302`), the six uuid gates (`:679-768`, `:889-942`,
  `:1988-2045`).
- `crates/quilltap-core/src/api/zod_issues.rs`: ONE new constructor (regex
  `invalid_format`), **if** 2(a) is in scope.
- Optional: `api/types.rs` / `api/memories.rs` repoint of their private
  `double_option`; `test_support.rs` doc comment (4f).
- `api/salon.rs:1613-1625` for 4(c): **recommend OUT** unless confirmed unowned.
  `api/chats.rs` does not exist.

**Harness / oracle the lane owns:**
- `system_import_state.rs` + `system-import-execute.test.ts` (new payload-built cases,
  mask lift).
- `projects_routes_equivalence.rs` + `projects-routes.test.ts`.
- `groups_routes_equivalence.rs` (+ `groups-routes.test.ts` only if needed).
- `projects_tier2_equivalence.rs` / `groups_tier2_equivalence.rs` + their `.ts` +
  `projects-tier2.json` / `groups-tier2.json`.
- NEW `import_warning_text_guard.rs`.
- `project_background_display_mode_equivalence.rs`: re-run only.

**Must NOT touch:**
- `services/backup/**` (P4.147, including `restore/orchestrator.rs`, the only plain
  `GroupsRepository::create` caller and `project_properties`).
- `db/fallback.rs` and every other `db/*` repository (P4.149). The import uses
  `fallback::error_text` read-only, through a local helper.
- `db/store_backed.rs`, unless ruled in (see Q3).
- `services/dangerous_content/chat_override.rs`, unless 3(a) is ruled in and it is
  unowned.
- No committed `.db` fixture (`system-data-*`, `groups-projects-*`) is regenerated.

## §Open questions

1. **#140's Zod half — scope.** Close only DB tails + group/project properties (this
   survey's recommendation)? Or also take P4.143 Tier 3 item 12 (the generated
   schema-shape table retiring all nine `SERDE_ARM_DIVERGENCES` rows + message +
   character)?
2. **Tightening `parse_properties` on READ** (2a). It converges with v4 but makes any
   stored invalid bag unreadable (503). Tighten reads too, or only create/import/PUT?
3. **The project-import write-before-validate TRAP** (§1c, NOT RUN). Fix it in the
   import (parse before `repo.create`, as the group does), or in
   `StoreBackedRepository::create` (validate before `create_slim`)? The second covers
   restore too, but `store_backed.rs` is shared.
4. **2(c)/2(d):** will P4.147 move the restore's group create onto
   `create_with_properties`? If yes, delete plain `create` and the two dead fields
   here, sequenced after P4.147.
5. **4(c)** lives in `api/salon.rs`, not `api/chats.rs`. In this lane or deferred?
6. **The 9 absent import WARN lines and the 19 absent `[Projects v1]` lines.** Rider
   here, or a separate census order?
7. The cached oracle NDJSON's pin is unknown. The lane must regenerate at its own pin
   before trusting the six-case red-first prediction.
