# Survey — P4.147, the backup/restore order (dogfood #141 + #142, P4.146 item 13, P4.D249's inform arm, P4.143's restore smalls)

Surveyed 2026-10-05, read-only. v5 `main` @ `6c3a3c635`; v4 `~/source/quilltap-server` @ `07b8f0209`
(the oracle baseline, tree clean). Ruling for the family: **`backup-restore-fix-dont-match`
(2026-08-03) — v5 FIXES v4's restore bugs on the READ side, pins each divergence both ways, and
the WRITER (`services/backup/collect.rs` / `marshal.rs`) stays byte-identical.** Nothing below
needs a writer change.

## §0 The phase lists, measured

**v4 `lib/backup/restore/restore.ts` (1,177 lines), the markers in order:** 1 Tags (`:107`) · 2
Connection profiles (`:118`) · 3 Image profiles (`:158`) · 4 Embedding profiles (`:169`) · 5 Files
**DEFERRED to 22a-bis** (`:180-190`, `filesRestored = 0`) · 6 Characters (`:192-201`) · 7 Chats +
messages (`:203-246`) · 9 Memories (`:247`) · 10 Prompt templates (`:269`) · 11 Roleplay templates
(`:286`) · 12 Provider models (`:302`) · 13 Projects (`:315-327`) · 13a Groups (`:329-345`) · 14 LLM
logs (`:347`) · 15 Plugin configs (`:365`) · 16 Chat settings (`:386`) · 17 Folders (`:424`) · 19
Wardrobe DEFERRED (`:448`) · 20 Outfit presets REMOVED (`:458`) · 21 Character plugin data (`:462`) ·
22 Annotations (`:475`) · **22a mount points (`:492-506`)** · **22a-bis Files (`:508-617`)** · 22b
folders (`:619`) · 22c files (`:636`) · 22d links (`:649`) · 22e documents (`:664`) · 22f blobs
(`:677`) · 22f-bis legacy wardrobe (`:721`) · 22g chunks (`:740`) · 22h project↔store (`:754`) ·
22h-i group↔store (`:767`) · 22h-ii group↔character (`:781`) · 22i chat documents (`:795`) · 22i-ii
informs (`:808-827`) · 22j vectors (`:829`) · 22k conv. chunks (`:860`) · 22l TF-IDF (`:878`) · 22m
embedding status (`:894`) · 22n text-replacement rules (`:910`) · **22o instance settings LAST
(`:937-954`)** · 23 npm plugins (`:956`) · 24 themes (`:993`) · 24a compact (`:1039`) · 25 reconcile
(`:1076`). (No 8, no 18.)

**v5 `crates/quilltap-core/src/services/backup/restore/orchestrator.rs` (2,573 lines):** the same
list, same numbering, with ONE ruled move — the files phase runs **after the whole doc-store family**
(`restore_mount_family`, 22a–22h-ii incl. 22f-bis, called at `:1006-1021`), at `:1023-1061`, before
22i (`:1069`). Markers: 1 `:218` · 2 `:245` · 3 `:344` · 4 `:371` · 5 comment `:399-447` · **6
`:448-460`** · 7 `:462-585` · 9 `:586` · 10 `:644` · 11 `:670` · 12 `:702` · **13/13a `:726-775`** ·
14 `:777` · 15 `:839` · 16 `:864` · 17 `:900` · 19/20 `:947-948` · 21 `:950` · 22 `:978` · **22a
`:1481-1521`** (inside `restore_mount_family` `:1426`) · 22b `:1523` · 22c `:1558` · 22d `:1584` · 22e
`:1617` · 22f `:1650` · 22f-bis `:1697-1733` · 22g `:1734` · 22h/h-i/h-ii `:1769` · **files
`:1023-1061`** · 22i `:1069` · **22i-ii informs `:1093-1117`** · 22j `:1119` · 22k `:1148` · 22l
`:1176` · 22m `:1205` · 22n `:1233` · **22o `:1261-1273`** · 23/24 `:1275` · 24a `:1302` · 25
`:1333`. The pre-restore wipe is `delete_all::delete_user_data` (`:96-112`), which truncates
`doc_mount_points` and every mount-index table (`services/delete_all.rs:259-270`,
`FORMAT3_MOUNT_TABLES`) and **leaves `instance_settings` alone** (`:276-278`).

---

## Item 1 — dogfood #141: a `replace` restore orphans every vault / project store / group store

### 1.1 The item, quoted
`dogfood-findings.md:13` (row 141): *"Phase 6 restores each character through the create path,
which (as v4's `CharactersRepository.create` does) DROPS the archive's
`characterDocumentMountPointId` and provisions a FRESH vault holding only the managed fields; phase
22a then restores the archive's real vault under its own id beside it. Measured on the copy: 144
mount points from a 77-store archive (55 fresh vaults …, 12 fresh project/group stores …, the
archive's 77 …), 41 duplicate names, and Friday's pointer on a 12-file vault … while her real one
(805 links …) sits orphaned … Projects likewise point at their fresh stores."* Standing note
`dogfood-findings.md:629-645` item 0: *"(a) characters at the archive's vault (preserve the
incoming `characterDocumentMountPointId` when the archive carries that mount, or re-point after 22a
and remove the fresh vault); projects and groups at their archive stores the same way."*

### 1.2 v5 today
- **Phase 6** `orchestrator.rs:448-460` → `restore_one_character` `:2085-2136`. Builds a
  `CharacterCreate` with **`character_document_mount_point_id: None`** (`:2109`, hard-coded), decodes
  the vault input with `serde_json::from_value(ch.clone())` (`:2117-2119`), then
  `db::character_vault::create_character_with_options` (`:2120-2131`). Doc comment `:2085-2087`:
  *"`repos.characters.create` inserts the slim row, provisions a FRESH vault (dropping any incoming
  `characterDocumentMountPointId`), and projects the managed fields into it."*
- `db/character_vault.rs:463-489` `create_character_with_options`: nulls the FK again (`:473`),
  inserts the slim row, calls `ensure_character_vault(main, mount, &id, &name, vault, None)`
  (`:486`). `ensure_character_vault` `:246-366`: FK `None` → `find_by_name("<name> Character
  Vault")` and **adopt iff exactly one same-name `storeType='character'` store holds every
  `REQUIRED_VAULT_FILES` entry** (`:296-313`); otherwise mint a fresh `<name> Character Vault` (name
  uniquified with ` (N)`, `:318-327`), scaffold, project the managed fields, link
  (`link_character_to_vault` `:390-422`). In a `replace` restore the wipe has just deleted every
  `doc_mount_points` row, so the adopt arm can never fire at phase 6 → **always a fresh vault.**
- **No "preserve the pointer" option exists on any create.** `CharactersRepository::create`
  (`db/characters.rs:198`/`:318`) DOES write a supplied `character_document_mount_point_id`; it is
  only `create_character_with_options` that discards it.
- **Phase 13/13a** `orchestrator.rs:726-775`: `ProjectsRepository::create` /
  `GroupsRepository::create` → `StoreBackedRepository::create` (`db/store_backed.rs:229-251`) →
  `create_slim` (`:149-170`, **private**, inserts `officialMountPointId = NULL`) →
  `ensure_official_store::<E>` (`db/ensure_official_store.rs:79-158`): FK unset → mint a fresh
  `<prefix><name>` store (`storeType 'documents'`), `E::link_store` (a NEW `project_doc_mount_links` /
  `group_doc_mount_links` row), `set_official_mount_point_id` (pub, `store_backed.rs:132`), then
  `overlay::write_managed_fields` and a closing overlay `find_by_id`. The adopt step 2 is "deferred"
  (`ensure_official_store.rs:110-112`). Comment at `orchestrator.rs:728-732`: *"Both `create`s
  DISCARD the incoming `officialMountPointId` and provision a fresh store … the pointer is the thing
  that moves, which is why the differential normalizes exactly those two columns"* (the differential
  actually normalizes by ORIGIN, not by column — see §6).
- **22a** `orchestrator.rs:1481-1521` creates every archived `doc_mount_points` row under its
  archive id (`CreateOptions { id: id_of(mp) }`); 22b–22g restore folders/files/links/documents/
  blobs/chunks keyed by those ids; **22h** (`:1769-…`) restores the archive's
  `project_doc_mount_links` / `group_doc_mount_links` (fresh link ids — `PdmlCreate` takes no id).
  There is no unique index on `doc_mount_points.name`, so the duplicate names land silently.
- **The "startup heal":** v5 has NO boot-stage twin of v4's `backfillCharacterVaults`
  (`crates/quilltap-host/src/host.rs:1919-1930` says so). The adopt-by-name arm lives only inside
  `ensure_character_vault`, reached with `fk = None` from `tools/list_mail.rs:195`,
  `db/vault_character_update.rs:500`, `subprompts/storage.rs:171`, `post_office/deliver.rs:69`, and
  with the real FK from `api/chat_post_office.rs:1133`. **It only adopts when the pointer is
  ABSENT** — a character pointing at a fresh vault never reaches it. Projects/groups have no adopt
  arm at all.

### 1.3 v4 at `07b8f0209`
- `lib/backup/restore/restore.ts:192-201`: `const { userId, createdAt, updatedAt, ...charData } =
  character; await repos.characters.create(charData, { id: character.id });` — catch:
  `warnings.push(\`Failed to restore character "${character.name}": …\`)` + `moduleLogger.warn('Failed
  to restore character', { characterId, error })`.
- `lib/database/repositories/characters.repository.ts:262-296` (`create`, inside `safeQuery`):
  `const { characterDocumentMountPointId: _droppedMountId, ...rest } = data` → `_create` with
  `characterDocumentMountPointId: null` → `ensureCharacterVault(created)`. Its doc comment
  (`:254-256`): *"Any `characterDocumentMountPointId` in `data` is dropped: a freshly-created
  character always gets a freshly-provisioned vault, since pointing a new row at an existing vault
  would cross-link unrelated content."* and (`:270-272`) *"the import-reconciliation pass remaps that
  pointer to a vault the importer separately created if applicable"* — restore has no such pass.
- Projects `restore.ts:315-327` (`repos.projects.create(projectData, { id })`, whole hydrated row
  minus timestamps); groups `:329-345`, whose comment says outright *"`groups.create` (store-backed)
  discards any incoming officialMountPointId and provisions a fresh store"*. Both through
  `ensureProjectOfficialStore` / `ensureGroupOfficialStore`
  (`lib/database/repositories/projects.repository.ts:31,51`, `groups.repository.ts:31,50`).
- 22a `:492-506` restores the archive's stores under archive ids. **Reachable, deterministic: v4
  shares #141 exactly.**

### 1.4 Divergence
NONE today (v5 = v4, both orphan every archived store); the order CREATES a ruled divergence: v5
keeps each restored entity on the store the archive restored.

### 1.5 The archive always carries the vault? Measured on the 19 committed archives
Every archive except `restore-archive-minimal.zip` and `restore-archive-legacy-profiles.zip` (no
`doc-mount-points.json` rows at all) carries a `storeType:'character'` row for every character's
pointer, plus the project/group official stores — **except** `restore-archive-orphan-links.zip`,
where Riya points at `cc93ed5c-…` which is NOT in the archive (only Lorian's `35f24192-…` is). So
the three fallback shapes (no stores at all / the pointed store missing / present) are already
committed. ⚠ `restore-archive-gen2.zip` is #141's own fossil: an archive of a restored instance,
carrying 8 vault/project/group stores for 4 entities (the pointed "fresh" ones `6f96776d-…`,
`db3e4e14-…`, `cce70fe0-…`, `9da8a0dd-…` plus the orphaned `aff3114e-…` etc.) — "no duplicate names"
cannot be asserted absolutely on it, only "no duplicates beyond the archive's own".

### 1.6 Predicted hunks — the smallest READ-side shape: **preserve at phase 6 / 13 / 13a**
- **Shape A (recommended): preserve the pointer when the archive carries that store.** Compute,
  before phase 6, `archived_store_types: HashMap<id, storeType>` from `data.doc_mount_points`.
  - Characters: when `characterDocumentMountPointId` names an archived `storeType == 'character'`
    store, insert the slim row through `CharactersRepository::new(main).create(&slim_with_fk, opts)`
    (the FK column is written verbatim, `db/characters.rs:318`) and **skip `ensure_character_vault`
    entirely** — the vault's files arrive at 22a–22g. Otherwise fall through to today's
    `create_character_with_options` (v4-identical fresh vault; convergent on the minimal /
    legacy-profiles / orphan-links-Riya cases). Restore-local; no `db/character_vault.rs` edit
    needed.
  - Projects/groups: when `officialMountPointId` names an archived store, a slim-only create that
    writes the FK and provisions/links/overlays NOTHING (the archive's `properties.json`,
    `description.md` etc. and its `project_doc_mount_links`/`group_doc_mount_links` row restore at
    22a–22h). Needs ONE new pub method on `StoreBackedRepository` (e.g. `create_slim_linked(name,
    opts, mount_point_id)` over the private `create_slim` + `set_official_mount_point_id`) —
    `db/store_backed.rs`, NOT `db/projects.rs` / `db/groups.rs` (P4.148's) — called as
    `StoreBackedRepository::<ProjectEntity>::new(main, mount)` (the type and `new` are pub,
    `store_backed.rs:15,22`). Fallback: today's `create` (plus item 3's bag fix).
  - **new-account mode:** `uuid_remap::remap_character` (`uuid_remap.rs:444-…`) does NOT remap
    `characterDocumentMountPointId`, the project remap (`:200-215`) does not remap
    `officialMountPointId`, and groups remap only `id` (`:219-221`, comment *"officialMountPointId is
    discarded and re-provisioned"*) — while `doc_mount_points` ARE remapped (`:337`). **Do not touch
    `uuid_remap.rs`** (byte-proven by `backup_uuid_remap_equivalence`). Translate the pointer by
    INDEX PAIRING `extracted.data.doc_mount_points[i] ↔ data.doc_mount_points[i]` (the trick phase 5
    already uses, `:1036`) or by keeping the local `UuidRemapper` from `restore()` `:121-131` and
    passing a lookup down. Ruling needed on whether new-account preserves at all (§Open questions).
  - Safety net (optional, small): after 22a, any preserved entity whose store failed to restore
    (a `Failed to restore document store` warning) falls back to `ensure_character_vault(…, None)` /
    `ensure_official_store` so no row is left on a dangling FK.
  - Comments to rewrite: `:2085-2087`, `:728-732`, the phase-6 marker, and the restore-vintage
    family's comment `restore_vintage_state.rs:451-455` ("restore also provisions each restored
    character's vault … and those mint links of their own").
- **Shape B (rejected): re-point after 22a + GC the fresh store.** Must delete the fresh store's
  `doc_mount_points` row plus its `doc_mount_folders`, `doc_mount_file_links`, `doc_mount_documents`,
  `doc_mount_chunks` and the `project_/group_doc_mount_links` row, and refcount-GC
  `doc_mount_files` content rows (global, sha-keyed — a fresh vault's `identity.md` can share a
  content row with the archive vault's identical file). More writes, a GC with a correctness trap,
  and the fresh vault's managed-field projection runs for nothing. **Trade-off of A:** between phase
  6 and 22a a character/project/group row carries an FK to a store that does not exist yet
  (cross-partition, no SQL FK; nothing in phases 7–22 reads a vault — 22f-bis wardrobe runs after
  22a); and a vault whose required files are incomplete in the archive is adopted as-is (it is the
  user's own vault — that is the point).
- Blast radius: all inside `orchestrator.rs` (phase 6 / 13 / 13a + one setup map) + one method in
  `store_backed.rs`. Callers of `create_character_with_options`: `orchestrator.rs:2120` is the only
  restore caller (`ggrep -rn create_character_with_options` → `character_vault.rs:472-486` and the
  restore); `StoreBackedRepository::create` is used by `ProjectsRepository`/`GroupsRepository` only.

### 1.7 The proof
- Family: `crates/quilltap-harness/tests/system_restore_state.rs` (3,054 lines; tier-2 whole-DB
  dump of all three partitions, `QT_ORACLE_SYSTEM_RESTORE`), oracle
  `harness/oracle/cases/system-restore.test.ts` (725 lines, `RESTORE_CASES` `:123-374`). Every
  `replace` case on an archive with stores ALREADY exercises #141 on both sides; today both agree
  because the normalizer labels each fresh store id `<minted-N>`.
- **New divergence pin (both ways), modelled on `PHASE_ORDER_RESIDUAL` (`:190`) /
  `V5_STATS_GAP` (`:303`):** a `FRESH_STORE_RESIDUAL` carve that, **before normalization**, removes
  from v4's dump every `doc_mount_points` row whose id is not in the archive AND is the pointer of a
  restored entity whose archive pointer IS in the archive, plus every row keyed by it
  (`doc_mount_folders`, `doc_mount_file_links`, `doc_mount_documents`, `doc_mount_chunks`,
  `project_doc_mount_links`, `group_doc_mount_links`) and the `doc_mount_files` content rows only
  those links reference. Then assert: (v4 direction) the carve removed ≥ 1 store per such entity
  and v4's pointer ∉ archive ids; (v5 direction) v5 carved NOTHING and v5's pointer == the archive's
  pointer. ⚠ **Trap:** the `<minted-N>` first-encounter labelling (`:14-24`) shifts every later
  label if the carve runs after normalization — it must run on the raw dumps.
- Fallback arm stays a plain row-for-row equality on `restore_minimal`,
  `restore_legacy_profiles_replace`, and Riya in `restore_orphan_links_replace` (assert the carve is
  empty there on BOTH sides — convergence pin).
- A `restore_vintage_state`-style v5-only invariant (no oracle, never SKIPs) for §7's acceptance.

### 1.8 Fixtures
No new archive needed for #141 — `restore-archive.zip` (characters `a1000000-…0001/2` → vaults
`aff3114e-…` 11 links / `f6b22d51-…` 9 links; project `5c17e916-…` 4 links; group `60a8194d-…` 4
links) already carries the shape. Readers of `crates/quilltap-web/tests/fixtures/restore-archives/`:
`system_restore_state.rs`, `system_restore_equivalence.rs`, `restore_vintage_state.rs`,
`p4_9g6_seam_contract.rs`, `apps/web/e2e/zzz-restore-destructive.spec.ts` (new-account,
`restore-archive.zip`, asserts only the success text), `harness/oracle/cases/system-restore.test.ts`,
the nine `build-restore-archive*` builders and four `derive-restore-archive-*.py`, and
`build-store-delete-fixture.ts`. The oracle NDJSON for `system-restore` must be regenerated at the
pin (one regen, both #141 and #142 cases).

### 1.9 Risk
- ⚠ The v4 comment's own warning ("pointing a new row at an existing vault would cross-link
  unrelated content") does not apply to a replace restore (the vault IS this character's, by id) —
  but it DOES apply in new-account mode onto a non-empty target only if pairing is wrong; pairing by
  index is exact.
- The chunk-on-write insertion order in `doc_mount_chunks` (22f-bis) is unaffected (no legacy
  wardrobe rows in any committed archive: `wardrobe-items.json` is `[]`).
- Candidate v4 filing (post-5.0 v4-first list) — the human files it.

---

## Item 2 — dogfood #142: project-less files resolve Uploads through the TARGET's pointer

### 2.1 The item, quoted
`dogfood-findings.md:12` (row 142): *"The files phase resolves the Quilltap Uploads store through
the TARGET's `instance_settings.userUploadsMountPointId`, which the `replace` wipe deliberately
leaves alone and phase 22o only overwrites LAST; the archive's Uploads mount lands at 22a under the
ARCHIVE's id. When the two ids differ, every project-less file fails `Quilltap Uploads mount has not
been provisioned` (11 on the copy …) and 22o then repoints at the restored store, which never got
the bytes … the same holds for any restore into a FRESH instance."* Standing note item 0 (b):
*"the three built-in pointers (Uploads / Lantern / General) resolved to the ARCHIVE's ids before the
files phase."*

### 2.2 v5 today
- Files phase `orchestrator.rs:1023-1061` → `restore_one_file` `:1998-2083`: a `projectId`-less row
  calls `file_storage::write_user_upload_to_mount_store(main, mount, codec, &filename, bytes, &mime,
  "restored", None)` (`:2021-2023`) → `get_user_uploads_store` (`services/file_storage.rs:1219-1231`:
  `instance_settings::get_user_uploads_mount_point_id` = raw `read_setting("userUploadsMountPointId")`
  (`db/instance_settings.rs:761-767`), then `find_by_id_for_docedit` + `mountType == 'database'`) →
  on miss `Err(DbError::Internal("Quilltap Uploads mount has not been provisioned"))`
  (`file_storage.rs:1248-1252`) → warning `Failed to restore file "<name>": Quilltap Uploads mount
  has not been provisioned` (byte-compared against v4, `system_restore_state.rs:1916-1935`).
- 22o `orchestrator.rs:1261-1273`: raw upsert of every archived `instance_settings` row, comment
  *"LAST, because the mount-point keys point at the doc_mount_points restored above."*
- The ruled-divergence comment `:399-447` states the premise #142 breaks: *"22a restores the mount
  points, including the built-ins, with their archive ids — which is exactly what the surviving
  `instance_settings` pointer expects."*
- **The three built-in pointers during restore:** only Uploads is READ (the files phase). Lantern
  (`get_lantern_backgrounds_mount_point_id`, `instance_settings.rs:140`; error string `Lantern
  Backgrounds mount has not been provisioned`, `services/image_job_storage.rs:246`) and General
  (`get_general_mount_point_id`, `:134`) are never read by any restore phase (`ggrep -i
  "lantern|general_mount|user_uploads|builtin" services/backup/restore/*.rs` → nothing but
  `is_built_in` template flags). They are only WRITTEN at 22o — so after a restore into a target
  whose built-ins differ, 22o leaves them correct; the harm is limited to Uploads' files phase (and
  to any dangling archive value, below).
- Fresh-instance provisioning / boot: `services/builtin_mounts.rs:118-…` `ensure_builtin_mounts` →
  `ensure_one_mount` (`:304-335`): pointer dangles → mint a new store and rewrite the pointer — which
  is how the walk's reboot re-minted Uploads under a new id.

### 2.3 v4 at `07b8f0209`
`restore.ts:180-190` (phase 5 deferral) and `:508-512`: *"the Quilltap Uploads mount is back under
the id its `instance_settings` pointer still names"* — the same premise; 22a-bis calls
`writeLibraryFileBytes({…, projectId, subfolder: 'restored'})` (`:569-576`) through the uploads bridge
(`user-uploads-bridge.ts:94/98` throws `Quilltap Uploads mount has not been provisioned`); 22o
`:937-954` upserts last (comment: *"so a fresh instance's auto-provisioned defaults get overwritten by
the backup's values"*). Reachable: any restore into a target whose pointer ≠ the archive's. **v4
shares #142.**

### 2.4 Divergence
NONE today; the order creates a ruled divergence: v5 restores project-less files into the ARCHIVE's
Uploads store when the target's pointer differs (fresh instance / re-minted store).

### 2.5 Predicted hunks
- **Recommended:** a "22a-ter" step inside the files phase's preamble (after `restore_mount_family`,
  before `:1033`): for each of `MOUNT_POINT_SETTING_KEYS` (`uuid_remap.rs:44-48`, pub) — if
  `data.instance_settings` carries the key AND its raw value names a row in `data.doc_mount_points`
  (i.e. restored at 22a) → upsert it NOW with 22o's exact SQL. 22o later rewrites the same value
  (idempotent; still counted once there). This is ~20 lines in `orchestrator.rs`; no
  `file_storage.rs` change. In new-account mode the keys are already remapped consistently with
  `doc_mount_points` (`uuid_remap.rs:44-48` + `:337`), so the same code works — but it would MOVE the
  five `V5_STATS_GAP` new-account cases' files from the target's own Uploads store into the restored
  copy (ruling: replace-only recommended — §Open questions).
- **Not recommended for this order:** re-provisioning a built-in (`ensure_builtin_mounts`) when the
  archive carries NO Uploads mount (`restore-archive.zip` and 13 siblings carry only
  `generalMountPointId`/`lanternBackgroundsMountPointId`, and no built-in store rows). That would
  make `portrait.png` restore on ~14 currently-convergent `replace` cases and multiply the
  divergence surface; and 22o would then overwrite the minted pointer with the archive's dangling
  value unless 22o also learns to skip dangling built-in keys. Record it as a named follow-up.
- ⚠ **Data trap:** in 15 of the 19 archives `generalMountPointId` / `lanternBackgroundsMountPointId`
  are JSON-ENCODED (`"\"11111111-2222-4333-8444-555555555555\""`, `"\"66666666-…\""`), naming no
  archived store; only `-uploads`, `-gen2`, `-compact`, `-bug117` carry bare ids that match a store.
  The "names a restored row" guard must compare the RAW value (a quoted value simply never matches).

### 2.6 The proof
- **Already proven to exist on both sides:** the harness WORKS AROUND #142 by setup —
  `system_restore_state.rs:977-1007` (`aligns_uploads_pointer`: `restore_uploads_replace`,
  `restore_gen2_replace`, `restore_compact_replace`) + `align_uploads_pointer` `:1124-1148`, mirrored
  by the oracle's `alignUploadsPointer` (`system-restore.test.ts:149-160,232-234,642-655`). Its
  comment `:981-986` describes #142 verbatim ("the pointer dangles — both engines warn and restore
  nothing").
- **New cases (no new fixture):** `restore_uploads_fresh_replace` (`restore-archive-uploads.zip`,
  NO align: archive Uploads `e59f0187-…`, two project-less files `portrait.png`, `ledger.txt`) and
  optionally `restore_compact_fresh_replace` / `restore_gen2_fresh_replace`. Pinned both ways as a
  `FRESH_TARGET_UPLOADS` list: v4's `summary.warnings` carries exactly N `Failed to restore file
  "<f>": Quilltap Uploads mount has not been provisioned` and `main.files` lacks those rows; v5's
  carries zero and every such file's `storageKey` is `mount-blob:<archive uploads id>:…`. The three
  aligned cases stay as convergent controls (the fix is a no-op there).
- Tier: tier-2 DB diff + warnings byte-compare (existing `compare_warnings` `:1936`).

### 2.7 Fixtures
None new. Same readers as §1.8; the oracle case list grows (one regen of `system-restore` at the
pin).

### 2.8 Risk
The walk's 11 failures were 10 character-archive `.qtap` bundles + `hinge-log.md` — the archived
bundles ride the `keep_archived_character_bundles` path (`orchestrator.rs:81-90`); confirm on the
dogfood pass that kept bundles and restored rows do not collide on `restored/<name>`.

---

## Item 3 — P4.146 OPEN item 13: the restore's project/group property bags

### 3.1 The item, quoted
`work-orders/p4.146-properties-json-null-preservation.md:5` (Unification): *"item 13 sharpened —
the restore's `project_properties` helper copies SIX keys, so a restored project loses the VALUES of
`icon`, `defaultImageProfileId`, `answerConfirmationOverride` and the other nullable keys, not only
their nulls (v4 passes the whole hydrated project to `create`), and the restore's group create still
goes through value-or-absent `create` (a null colour lands ABSENT) — the fix is `fold_properties`
over the whole bag + `create_with_properties`."*

### 3.2 v5 today
- `orchestrator.rs:2438-2456` `project_properties`: copies exactly `allowAnyCharacter`,
  `characterRoster`, `color`, `defaultDisabledTools`, `defaultDisabledToolGroups`,
  `backgroundDisplayMode`. The project bag has **16** keys
  (`<ProjectEntity as StoreEntity>::property_keys()`, `db/projects.rs:182-201`) — the 10 dropped:
  `icon`, `defaultAgentModeEnabled`, `defaultAvatarGenerationEnabled`, `defaultImageProfileId`,
  `defaultRoleplayTemplateId`, `defaultAlertCharactersOfLanternImages`,
  `answerConfirmationOverride`, `storyBackgroundsEnabled`, `staticBackgroundImageId`,
  `storyBackgroundImageId`. Passed as `ProjectCreateInput.properties` (`orchestrator.rs:735-741`) →
  `seed_create_properties` → `write_managed_fields` (parse through `ProjectProperties`, three-state
  since P4.146, `projects.rs:78-170`).
- Groups `orchestrator.rs:749-763`: `GroupCreateInput { color: os(g,"color"), icon: os(g,"icon") }`
  → `GroupsRepository::create` (`db/groups.rs:126-138`), value-or-absent: a `null` lands absent.
  `create_with_properties` (`groups.rs:141-161`, pub) takes a `GroupProperties` bag whole.
- `fold_properties` is **private in `services/quilltap_import/entities.rs:417-427`** (P4.148's file);
  `property_keys()` / `parse_properties()` are on the pub `StoreEntity` trait
  (`db/document_store_overlay.rs:84,103,108`) — the restore can fold locally without touching
  `db/projects.rs` / `db/groups.rs` / the importer.

### 3.3 v4
`restore.ts:319-321` spreads the whole hydrated project (minus `createdAt`/`updatedAt`) into
`repos.projects.create`; groups `:338-339` likewise → `writeManagedFields(parseProperties(entity))`:
every bag key's value AND explicit `null` survive.

### 3.4 Divergence
v5 drops 10 project property VALUES and folds a group's `null` colour/icon to absent on the
fresh-store arm. **Blind today:** every committed archive's project carries exactly the six keys
(measured: `restore-archive.zip` / `-bag-keys` / `-uploads` `projects.json` = `allowAnyCharacter`,
`characterRoster`, `color`, `defaultDisabledTools`, `defaultDisabledToolGroups`,
`backgroundDisplayMode`); every group carries no `color`/`icon` key.

### 3.5 Interaction with item 1 (important)
Under Shape A the project/group **does not write a bag at all when the archive carries its store**
— the store's own `properties.json` restores byte-for-byte at 22e. Item 3 then only matters on the
FALLBACK arm (archive lacks the store: `restore_minimal`, `restore_legacy_profiles_replace` — whose
archives have no stores — and any real archive with a missing store). Still worth fixing: it is the
v4-convergent arm and is diff-compared there.

### 3.6 Predicted hunks
Replace `project_properties` with a whole-bag fold over `ProjectEntity::property_keys()` (keeping
explicit nulls); groups fold over `GroupEntity::property_keys()` → `GroupEntity::parse_properties` →
`create_with_properties` (an `Err` becomes the per-row `Failed to restore group "…"` warning, as the
importer does since P4.146's unification). DRY: two folds (import + restore) — propose P4.148 move
`fold_properties` to `db/document_store_overlay.rs` and this lane call it; if the fence forbids,
a restore-local copy with a `// one home pending` note and a named follow-up.

### 3.7 The proof
A NEW derived archive (the `derive-restore-archive-*.py` pattern, e.g.
`derive-restore-archive-bag-nulls.py` → `restore-archive-bag-nulls.zip`) from `restore-archive.zip`:
remove the project/group STORE rows (so the fallback arm runs) and widen the project row with all 16
keys incl. explicit `null`s and real values for `icon`/`defaultImageProfileId`/
`answerConfirmationOverride`, the group with `color: null`, `icon: "⚙"`. Case
`restore_bag_nulls_replace`; tier-2 compares the fresh store's `properties.json`
(`doc_mount_documents` content) row for row — red-first on v5 today, green after. Copy the recipe of
`derive-restore-archive-concierge-bogus.py` (+ its md5-unchanged check of `restore-archive.zip`).

---

## Item 4 — P4.D249 OPEN: a malformed `permanent` on restore

### 4.1 The item, quoted
`work-orders/p4.d249-standing-informs-server-schema-ensure-carriers.md:5`: *"the restore accepts a
malformed `permanent` (`null` / `"true"` / `1` → one-shot) where v4's Zod skips the row with `Failed
to restore chat inform` — recorded as a divergence under the backup/restore 'fix v4, don't match it'
ruling, no pin yet; integer-to-bool decoding reads any non-zero as `true` where v4 reads `=== 1`
(unreachable from either writer; recorded)."*

### 4.2 v5 today
22i-ii `orchestrator.rs:1093-1117` → `restored_inform` `:2202-2216`: `permanent: b(inform,
"permanent", false)` (`rows.rs:31-34`, `as_bool().unwrap_or(default)`), so `null`, `"true"`, `1`,
`0`, `"false"` all → `false` (a standing inform silently demoted to one-shot). Warning on a failed
insert: `"Failed to restore inform".to_string()` + `": {e}"` via `warn_row!` (`:1110-1115`). No WARN
log line on that arm. Unit test `:2222-2239` covers true / absent only.
The integer decode is `db/chat_informs.rs:265` (`row.get::<_, Option<bool>>(6)?.unwrap_or(false)` —
rusqlite reads any non-zero as `true`) — a READ-side db file, **not this lane's** (unreachable from
both writers; leave recorded).

### 4.3 v4
`restore.ts:813-827`: `const { id, createdAt, updatedAt, ...informData } = inform; await
globalRepos.chatInforms.create(informData, { id: inform.id });` → `_create` validates
`ChatInformSchema` (`lib/schemas/chat-inform.types.ts:51` `permanent: z.boolean().default(false)` —
absent/undefined → false; `null`, `"true"`, `1` FAIL). Catch: `warnings.push(\`Failed to restore
inform: ${error.message}\`)` (the ZodError's message = `JSON.stringify(issues, null, 2)`, e.g. an
`invalid_type` "expected boolean, received null" issue at path `["permanent"]` — exact bytes NOT
MEASURED, must be recorded through the oracle) + `moduleLogger.warn('Failed to restore chat inform',
{ informId, error })`; debug `'Restored chat informs', { total, restored }` (`:824-827`). Reachable.

### 4.4 Divergence
v5 restores a malformed-`permanent` row as a one-shot inform; v4 skips it with the ZodError warning
and a WARN line. (Also: v5 has no `Failed to restore chat inform` WARN and no `Restored chat
informs` debug line on ANY arm — measured: `ggrep -rn "Restored chat informs\|Failed to restore
chat inform" crates/quilltap-core/src` → zero hits.)

### 4.5 Predicted hunks / ruling
Ruling needed (§Open questions): keep v5's lenient restore (pin both ways) **or** converge on v4's
skip (then it is not a divergence; port the Zod issue bytes through `api/zod_issues.rs`'s existing
renderer). Either way: add v4's two log lines (WARN on the catch, DEBUG summary) with capture pins.

### 4.6 The proof / fixtures
**No committed restore archive carries `data/chat-informs.json`** (measured over all 19;
`system_restore_state.rs:1703-1745` `assert_pre_410_archive_restores_no_informs` asserts exactly
that). Needs a NEW derived archive `restore-archive-informs.zip` (from `restore-archive.zip`, chat
`c1000000-…` + a real participant id): rows with `permanent` = `true` / `false` / absent / `null` /
`"true"` / `1`, one consumed. Case `restore_informs_replace`. ⚠ Adding it moves
`assert_pre_410_archive_restores_no_informs`'s case set — exclude the new case by name.

---

## Item 5 — P4.143's restore items still OPEN

`work-orders/p4.143-…:428-472` (Tier 3, all deferred; confirmed OPEN on main by reading the code):
- **Item 14 (restore halves), OPEN:** (a) the message replay's serde arm — `orchestrator.rs:528-538`
  pushes `Failed to restore message in chat "<title>": <serde sentence>` where v4's `addMessage`
  throws a ZodError (`restore.ts:218-223`); (b) the chat create's DB-error arm `orchestrator.rs:517-
  524` — pushes the warning with `{e}` = `DbError`'s `Display` (`sqlite error: …`,
  `db/mod.rs:219`) and **logs NO `Failed to restore chat` WARN**, where v4's per-chat catch
  (`restore.ts:241-243`) always WARNs `{ chatId, error }` and renders the bare message. Both are
  small riders in this lane's file. (The serde-arm table `SERDE_ARM_DIVERGENCES` / item 12's
  schema-shape generator is NOT this lane's.)
- **Same class across the whole orchestrator:** every `warn_row!`/`warn_only!` (`:170-188`) renders
  `{e}` through `Display`, so any SQLite failure in any phase reads `…: sqlite error: …` where v4
  reads the bare message — the #137 / #140 class. A rider could route the macros through
  `db::fallback::error_text` (`db/fallback.rs:35-40`, call only — P4.149 owns the file). NOT MEASURED
  how many corpus rows reach a SQLite arm today (likely zero; needs a plant to pin).
- Items 11, 12, 13, 15, 16, 17: not in `services/backup/**` (or restore stays outside the strict
  scope by design and is pinned absent, per the P4.143 Unification) — not this lane's.

---

## Item 6 — the harness side, measured

- **Families:** `system_restore_state.rs` (tier-2 state, 3,054 lines), `system_restore_equivalence.rs`
  (preview, 180), `system_restore_guards_equivalence.rs` (route guards, 244 — untouched by this
  order), `restore_vintage_state.rs` (v5-only invariants over `migration-vintage/`, 712 — comment
  only), `backup_uuid_remap_equivalence.rs` (378 — MUST stay green untouched),
  `system_backup_equivalence.rs` (692 — writer; untouched), `backup_mount_index_coercion_
  equivalence.rs`, `p4_9g6_seam_contract.rs`. Oracle cases `system-restore.test.ts`,
  `system-restore-guards.test.ts`, `backup-uuid-remap.test.ts`, `system-backup.test.ts`,
  `backup-mount-index-coercion.test.ts`.
- **How the oracle builds a run:** each case restores a COMMITTED archive into a freshly
  provisioned target built like `build-provision-oracle.ts` (`system-restore.test.ts:84-90,536-640`;
  runs v4's three `provision-*-mount` migrations `:593-604`, so the TARGET has its own minted
  Uploads/Lantern/General ids). Rust: `fresh_instance` (`system_restore_state.rs:1150-1162`,
  `provision_fresh_instance`).
- **The divergence-list idiom** (what an `EXPECTED_DIVERGENCES` is here): named `const` tables
  asserted both ways — `PHASE_ORDER_RESIDUAL` (`:190`, multisets equal AND raw orders differ),
  `V5_STATS_GAP` (`:303-322`, masked columns + asserted directions), `REPLAY_DEDUPE` (`:359-384`),
  `ORPHAN_*` (`:2588-2607`), plus case-scoped helpers (`classify_restore_serde_arm`, the bug-117 sha
  check `:1960`, `assert_bag_keys_survive` `:1975`, `assert_memory_graph_intact` `:1009`). The new
  `FRESH_STORE_RESIDUAL` (§1.7) and `FRESH_TARGET_UPLOADS` (§2.6) follow the same idiom.
- **Archives that exist vs must be built:** vaulted characters + project/group stores — EXIST in 17
  of 19 archives. Project-less Uploads files + the archive's own Uploads store + its pointer —
  EXIST in `restore-archive-uploads.zip` (also `-compact`, `-gen2`, `-bug117`). A target whose
  built-in pointers DIFFER from the archive's — EXISTS by default (every case's fresh target mints
  its own; the three aligned cases opt out). **So #141 and #142 need only new CASES; items 3 and 4
  need two new derived archives** (`restore-archive-bag-nulls.zip`, `restore-archive-informs.zip`).

## Item 7 — the acceptance measure as tier-2 assertions

On a `replace` restore of an archive that carries every pointed store (e.g. `restore_uploads_fresh_
replace`), on v5's dump, v5-only (the oracle half is the both-ways pin above):
1. `COUNT(doc_mount_points)` == `archive.docMountPoints.len()` (7 on `-uploads`; v4 today: 7 + 4 = 11).
2. `SELECT lower(trim(name)), COUNT(*) … GROUP BY 1 HAVING COUNT(*) > 1` returns only the archive's
   own duplicates (none on `-uploads`; the 4 on `-gen2`).
3. Every `characters.characterDocumentMountPointId` == the archive row's value when the archive
   carries that `storeType:'character'` store; every `projects`/`groups.officialMountPointId` ==
   the archive's (the "Friday on the 805-link vault" analog: `COUNT(doc_mount_file_links WHERE
   mountPointId = <pointer>)` == the archive's count for that store — Lorian 11, Riya 9, project 4,
   group 4 on `restore-archive.zip`).
4. Zero `summary.warnings` containing `Uploads mount has not been provisioned`; every restored
   project-less `files.storageKey` starts `mount-blob:<archive userUploadsMountPointId>:`.
5. `instance_settings.userUploadsMountPointId` == the archive's AND names an existing
   `doc_mount_points` row.
6. No `doc_mount_points` row unreferenced by an entity pointer / built-in pointer / archive id
   (no orphan fresh stores).
Run the same six on a `new-account` case only if the ruling extends the fix there. The Friday-copy
acceptance (77 stores, 805 links, zero Uploads warnings, incl. a restore into a freshly provisioned
instance) stays a 💸 dogfood row.

---

## §Ownership proposal

**Edit:**
- `crates/quilltap-core/src/services/backup/restore/orchestrator.rs` (phase 6 / 13 / 13a preserve
  arms + the archived-store map; the 22a-ter built-in pointer pre-apply; `project_properties` →
  whole-bag fold; group `create_with_properties`; `restored_inform` per the ruling; P4.143 item 14
  riders; the comments at `:399-447`, `:728-732`, `:2085-2087`).
- `crates/quilltap-core/src/services/backup/restore/rows.rs` (only if a strict-bool accessor is
  needed for item 4).
- `crates/quilltap-core/src/db/store_backed.rs` (ONE new pub slim-linked create).
- Harness: `system_restore_state.rs` (new cases + `FRESH_STORE_RESIDUAL` + `FRESH_TARGET_UPLOADS` +
  the §7 invariants + the informs/bag-nulls arms), `restore_vintage_state.rs` (comment, and an
  invariant if the lane wants a no-oracle copy), `system_restore_equivalence.rs` (only if a preview
  case is added for the new archives); oracle `harness/oracle/cases/system-restore.test.ts`; NEW
  `harness/oracle/fixtures/derive-restore-archive-bag-nulls.py`,
  `derive-restore-archive-informs.py`; NEW zips under
  `crates/quilltap-web/tests/fixtures/restore-archives/`. One `system-restore` oracle regen at the pin.
- CHANGELOG + `status-log.md` (append).

**Must NOT touch:**
- `services/backup/collect.rs`, `services/backup/marshal.rs`, `services/backup/manifest.rs`,
  `services/backup/archive.rs` (the WRITER — byte-identical); `services/backup/uuid_remap.rs` /
  `uuid_remapper.rs` (byte-proven; translate pointers in the orchestrator instead).
- `services/quilltap_import/**`, `db/projects.rs`, `db/groups.rs` (**P4.148**) — call
  `create_with_properties` / `StoreEntity` methods, never edit them; `fold_properties`' move to a
  neutral home is P4.148's to make.
- `db/fallback.rs` and the repository fallback homes (**P4.149**) — call `error_text` only.
- `db/character_vault.rs` (not needed under Shape A), `db/chat_informs.rs` (the `=== 1` decode stays
  recorded), `services/file_storage.rs`, `services/builtin_mounts.rs`, `services/delete_all.rs`,
  `crates/quilltap-host/**`.
- The committed `restore-archive*.zip` files other than the new ones (md5-check them unchanged, as
  the P4.143 derive did).

## §Open questions (for the human)

1. **#141 shape:** preserve-at-create (Shape A, recommended) vs re-point-after-22a + GC (Shape B)?
2. **new-account mode:** extend the preserve to new-account (pointer translated by index pairing),
   or replace-only? v4 never remaps the store pointers there either; today a new-account restore
   also orphans every archived store.
3. **#142 scope:** pre-apply only the archive's built-in pointers whose value names an archived
   store (recommended), or also re-provision a built-in when the archive carries none (changes ~14
   convergent cases; needs 22o to skip dangling built-in keys)? And replace-only, or new-account too
   (it would move the five `V5_STATS_GAP` new-account cases' files into the restored Uploads copy)?
4. **Item 4:** keep v5's lenient `permanent` (malformed → one-shot) as a pinned divergence, or
   converge on v4's skip-with-ZodError? Either way port v4's `Failed to restore chat inform` WARN
   and `Restored chat informs` DEBUG.
5. **Item 3's DRY home:** may P4.148 move `fold_properties` into `db/document_store_overlay.rs` this
   round so the restore calls it, or does this lane carry a local fold with a named follow-up?
6. **P4.143 item 14 + the `sqlite error:` tail on restore warnings:** riders in this lane (it owns
   the file), or the next smalls round?
7. Both v4 filings (#141, #142) for the post-5.0 v4-first list — the human files them.
