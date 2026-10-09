# Survey — v4 `ed8b15b50` bug 186 (unique store names + the store-name reconcile) and `197104649` (the sync disk-read line)

Read-only planning survey, 2026-10-09. v4 = `~/source/quilltap-server` main,
HEAD `01a83539d` (tree clean, never checked out); v5 = main at `96cfdaaf7`.
Scope: bug 186 of `ed8b15b50` (the repository name chokepoint, the NOCASE
unique index + its boot ensure, `store-names.ts` / `reconcile-store-names.ts`,
the reconcile's call sites, the PATCH live-vault refusal, DDL.md / CLAUDE.md /
help prose) and `197104649` whole. **Out of scope** (other surveys): bugs
183/184 (wardrobe wording / whose wear) and bug 185 (the restore claim map,
`createBoundToVault`, `createBoundToStore`, the uuid remap) — but their
meeting points with this cluster are in §8.

Every fact below is from the hunks (`git show ed8b15b50 -- <path>`) and the
HEAD files; `mount-index-case-repair.ts` is a git-"binary" file (it holds a
non-UTF-8 byte), so read it with `git show --text` / `grep -a`. Line numbers
are v4 HEAD unless marked.

**Measured this survey** (`dump-fresh-schema.ts` run at v4 HEAD into
`$TMPDIR/setupphase-survey-store-names-sync/fresh-head.json`, v4 tree still
clean after): the generateDDL surface at HEAD differs from v5's committed
`fresh_schema.json` by EXACTLY three statements — two `main` tables (`chats`,
`memories` — the memory programme, another cluster) and ONE mount-index line
from this cluster:

```sql
CREATE UNIQUE INDEX "idx_doc_mount_points_name_nocase" ON "doc_mount_points" ("name" COLLATE NOCASE)
```

So bug 186 IS a D23 schema move (the dumper drives each repository's
`ensureCollection`, whose `onTableEnsured` now creates the index) — not only a
v5 boot ensure. See §2.4.

---

## 1. v4 at HEAD — file:line facts

### 1.1 `lib/mount-index/unique-mount-point-name.ts` (UNCHANGED by the span; the shared leaf)

- `:33-40` `nextUniqueMountPointName(takenNames: Set<string>, desired)`:
  builds `takenLower` = each taken name `.trim().toLowerCase()`; returns
  `desired` (UNTRIMMED, as given) if `desired.trim().toLowerCase()` absent;
  else the first `` `${desired} (${n})` `` for n = 2, 3, … whose
  `.trim().toLowerCase()` is absent. No `(1)`.

### 1.2 NEW `lib/mount-index/store-names.ts` (163 lines, `ed8b15b50`)

Pure; imports only `nextUniqueMountPointName`.

- `:27-29` `characterVaultName(characterName)` →
  `` `${(characterName || 'Untitled').trim()} Character Vault` ``.
  (`''` → `Untitled Character Vault`; `'  '` is truthy → trims to `''` →
  `' Character Vault'` with a LEADING space.)
- `:33` `VAULT_NAME_PATTERN = /^(.*\S)\s+Character Vault(?:\s+\(\d+\))*$/i`
  (JS non-unicode regex: `.` excludes `\n \r    `; `\s`/`\S` = JS
  WhiteSpace+LineTerminator incl. `﻿`, ` `, NOT `\u0085`; `\d` ASCII;
  the `/i` flag in non-unicode mode canonicalises via `toUpperCase` and REFUSES
  a non-ASCII→ASCII mapping, so `ſ`/Kelvin-sign do NOT match `s`/`k`).
- `:40-43` `vaultBaseName(storeName)` → `VAULT_NAME_PATTERN.exec(storeName.trim())`
  → `match[1]` or `null`. Greedy `(.*\S)` then `\s+`, so
  `'Lorian Character Vault (2) (2)'` → `'Lorian'`;
  `'Tess  Character Vault'` → `'Tess'`.
- `:49-54` `storeNameTimestamp(iso)`: `ms = Date.parse(iso)`; NaN →
  `iso.replace(/:/g, '')`; else `d = new Date(ms).toISOString()` and
  `` `${d.slice(0,10)}T${d.slice(11,13)}${d.slice(14,16)}${d.slice(17,19)}Z` ``
  (`2026-10-09T16:45:06.123Z` → `2026-10-09T164506Z`; milliseconds DROPPED,
  not rounded; a non-UTC-offset input is normalised to UTC by `Date.parse`;
  a year outside 0000–9999 gives `toISOString`'s `±YYYYYY` and the fixed
  slices produce garbage — an edge row, not a design point).
- `:57-59` `retiredVaultName(baseName, createdAt)` →
  `` `${baseName.trim()} Version ${storeNameTimestamp(createdAt)} Store` ``.
- `:62-82` types: `StoreNameRow {id, name, storeType?, createdAt}`,
  `VaultOwnerRow {id, name, characterDocumentMountPointId?, createdAt}`,
  `StoreRename {id, from, to, reason: 'live-vault'|'retired-vault'|'collision'}`
  (KEY ORDER id, from, to, reason — it is spread into the INFO line).
- `:84` `lower = name.trim().toLowerCase()`.
- `:86-87` `byAge = a.createdAt.localeCompare(b.createdAt) || a.id.localeCompare(b.id)`
  (ICU `localeCompare`, as the case repair).
- `:103-163` `planStoreNames(stores, characters): StoreRename[]` — the
  algorithm, exactly:
  1. `storeIds = Set(stores.id)`. For each character in `[...characters].sort(byAge)`:
     if `pointer && storeIds.has(pointer)` → `ownerByStore.set(pointer, c)`
     unless already set (oldest owner wins on a damaged two-owner instance);
     ELSE (no pointer, or a dangling one) → `adoptableNames.add(lower(characterVaultName(c.name)))`.
  2. `taken` = Set of LOWERED claimed names; `claim(store, name, reason)` adds
     `lower(name)` and records `finalName[store.id]`.
  3. Live vaults = stores with an owner, sorted by: `holdsCanonical` first
     (`lower(s.name) === lower(characterVaultName(owner.name))`), then
     `byAge(ownerA, ownerB)`, then `byAge(storeA, storeB)` (stable sort).
     Each claims `nextUniqueMountPointName(taken, characterVaultName(owner.name))`
     reason `live-vault`.
  4. Every other store, `.sort(byAge)` (store createdAt, id): `base =
     storeType === 'character' ? vaultBaseName(name) : null`; if `base !== null
     && !adoptableNames.has(lower(characterVaultName(base)))` → claim
     `nextUniqueMountPointName(taken, retiredVaultName(base, store.createdAt))`
     reason `retired-vault`; else claim `taken.has(lower(name)) ?
     nextUniqueMountPointName(taken, name.trim()) : name` (the UNTRIMMED name
     when free) reason `collision`.
  5. Output: iterate `stores` IN INPUT ORDER; emit `{id, from: store.name, to,
     reason}` where `to !== store.name` (exact compare — a trim-only change IS
     a rename).
  Consequences a port must keep: archived characters ARE owners (raw read,
  no filter); a `storeType='documents'` store is never retired, only suffixed;
  a retired name (`… Version … Store`) is not vault-shaped, so it is never
  re-retired (idempotence); an unlinked vault whose name a vault-less
  character bears is LEFT (adoption may claim it).

### 1.3 NEW `lib/mount-index/reconcile-store-names.ts` (71 lines, `ed8b15b50`)

- `:27` `log = logger.child({ module: 'mount-index:reconcile-store-names' })`.
- `:29-32` `StoreNameReconcileResult { renamed: StoreRename[]; skippedReason: 'job-child'|'mount-index-unavailable'|null }`.
- `:34-71` `reconcileStoreNames(trigger: string)`:
  - `:35-38` `QUILLTAP_JOB_CHILD === '1'` → DEBUG `Store-name reconcile skipped in the job child` `{trigger}` → `{renamed: [], skippedReason: 'job-child'}`.
  - `:39-43` `db = isMountIndexDegraded() ? null : getRawMountIndexDatabase()`; null → WARN `Store-name reconcile skipped: mount-index database unavailable` `{trigger}` → `skippedReason: 'mount-index-unavailable'`.
  - `:45-49` `Promise.all([repos.docMountPoints.findAll(), repos.characters.findAllRaw()])` — BOTH are `_findAll` (`base.repository.ts:263-277`): `safeQuery` fallback `[]` on error (ERROR `Error finding all entities`), and every row failing its Zod schema DROPPED. ⚠ So a failed/emptied characters read makes every vault "unlinked" → every vault-shaped `character` store RETIRED; a store row dropped by Zod is not planned but still holds its name under the index.
  - `:50-56` DEBUG `Store-name reconcile planned` `{trigger, storeCount, characterCount, renameCount}`.
  - `:57` no renames → `{renamed: [], skippedReason: null}` (no write, no publish).
  - `:59-64` ONE `now = new Date().toISOString()`; `UPDATE "doc_mount_points" SET "name" = ?, "updatedAt" = ? WHERE "id" = ?` in ONE `db.transaction`: phase 1 every rename → `` `__renaming__ ${id}` ``; phase 2 every rename → `to` (both in rename-list order).
  - `:66-68` per rename INFO `Renamed document store to its rightful name` `{trigger, id, from, to, reason}`.
  - `:69` `publishRealtime('mountPoints')`; returns `{renamed, skippedReason: null}`.
  - A UNIQUE failure in phase 2 (only reachable via a Zod-dropped row holding a name) throws out of the transaction (rolled back) to the caller.
- Triggers in use: `'boot'`, `'restore'`, `'qtap-import'`, `'character-created'`, `'character-renamed'`, `'character-deleted'`.

### 1.4 `lib/database/repositories/mount-index-case-repair.ts` (`ed8b15b50` hunk `:330-367`)

- `:334` `MOUNT_POINT_NAME_NOCASE_INDEX = 'idx_doc_mount_points_name_nocase'`.
- `:343-352` `ensureMountPointNameUniqueIndex(db)`: `repairMountPointNameCollisions(db)` FIRST; then `nocaseUniqueIndexIsValid(db, NAME)` (`:66-73`: `sqlite_master` sql uppercased contains `UNIQUE` and `NOCASE`) → return; else `DROP INDEX IF EXISTS "idx_doc_mount_points_name_nocase"`, `CREATE UNIQUE INDEX "idx_doc_mount_points_name_nocase" ON "doc_mount_points" ("name" COLLATE NOCASE)` (two `db.exec`, no transaction), INFO `Mount-index: created the unique document-store name index` (root `logger`, NO fields).
- `:369-404` `repairMountPointNameCollisions` UNCHANGED in body (comment only): `SELECT id, name, createdAt FROM doc_mount_points`; group by `name.trim().toLowerCase()` (Map insertion order); per group ≥2 sort `createdAt.localeCompare || id.localeCompare`, keeper first; each loser `nextUniqueMountPointName(takenLower, loser.name.trim())`, `takenLower.add(newName.toLowerCase())`, `UPDATE doc_mount_points SET name = ? WHERE id = ?` (NO `updatedAt`), then **WARN** `Mount-index case repair: renamed document store whose name collided with a peer except for casing` `{mountPointId, keptMountPointId, oldName, newName}`.
- The header's "no DB index, because backup restore must be able to recreate legacy rows verbatim" line (`:12-15`) was NOT updated in the header (only the section comment) — v4 left a stale comment.

### 1.5 `lib/database/repositories/doc-mount-points.repository.ts` (`ed8b15b50`)

- `:27-38` `class MountPointNameTakenError extends Error`: `code = 'MOUNT_POINT_NAME_TAKEN'`, `requestedName`, `holder: {id, name}`, message `` `A document store named "${holder.name}" already exists. Names are matched without regard to case — please choose a different name.` `` (the HOLDER's stored casing; U+2014 em dash), `name = 'MountPointNameTakenError'`.
- `:54-86` `onTableEnsured`: the four ALTER self-heals (unchanged) then `:85` `ensureMountPointNameUniqueIndex(db)` (was `repairMountPointNameCollisions(db)`).
- `:97-104` `create(data, options)` → `await this.assertNameFree(data.name, null)` then `_create`.
- `:106-109` `update(id, data)` → `if (data.name !== undefined) await this.assertNameFree(data.name, id)` then `_update`.
- `:112-116` `findNameHolder(name, exceptId)`: `needle = name.trim().toLowerCase()`; `this.findAll()` (Zod-dropping, `[]` on error); first `mp.id !== exceptId && mp.name.trim().toLowerCase() === needle`, else `null`.
- `:118-128` `assertNameFree`: holder → WARN `Refused a document-store name another store holds` `{name, mountPointId: exceptId, holderId: holder.id}` (root `logger`; `mountPointId` is `null` on create) → throw `MountPointNameTakenError(name, holder)`.
- Every v4 caller of `docMountPoints.create`/`update` now passes this gate:
  `character-vault.ts:188`, `ensure-official-store.ts:134`, `restore.ts:610`,
  `quilltap-import/reconcile.ts:148`, `import-document-stores.ts:93/124`,
  the two routes, `[id]/route.ts:308/397` (no `name` → no check), and the
  migration `cutover-projects-to-store.ts:291`. Every one that sets a name
  already picks a free one first, so the throw is a backstop.

### 1.6 The routes (`ed8b15b50`)

- `app/api/v1/mount-points/route.ts:169-179` create: `findNameHolder(validatedData.name, null)` (replaces the inline scan; same semantics); WARN `[Mount Points v1] Rejected duplicate mount point name` `{name, clashesWith, userId}`; `conflict(...)` 409 with the same sentence as §1.5's message (holder casing). Unchanged otherwise.
- `app/api/v1/mount-points/[id]/route.ts` PATCH (`:92-176`), after `updateMountPointSchema.parse(body)` (whole-body parse first; a Zod error → the catch → 500):
  - NEW `:104-121`: `if (validatedData.name !== undefined && validatedData.name !== existing.name)` (EXACT compare — a re-casing IS caught) → `owner = (await repos.characters.findAllRaw()).find(c => c.characterDocumentMountPointId === id)` (first in `_findAll` row order; archived characters included) → WARN `[Mount Points v1] Rejected rename of a live character vault` `{mountPointId, characterId, userId}` → **409** `` `This vault belongs to ${owner.name} and is named after them. Rename the character to rename the vault.` ``.
  - `:123-136` then the duplicate check through `findNameHolder(name, id)`; WARN `[Mount Points v1] Rejected duplicate mount point rename` `{mountPointId, name, clashesWith, userId}`; 409 same sentence.
  - `:173-175` catch-all → ERROR `[Mount Points v1] Error updating mount point` → 500 `Failed to update mount point` (a `MountPointNameTakenError` raced past the pre-check lands here).
  - Note the help (`help/mount-points.md`) still says "Changing only the capitalization of a name is always allowed" — false for a LIVE vault (v4-faithful quirk, do not "fix").

### 1.7 `CharactersRepository` (`lib/database/repositories/characters.repository.ts`, `ed8b15b50`)

- `:67-78` module fn `reconcileVaultNames(trigger, characterId)`: lazy-imports and awaits `reconcileStoreNames(trigger)`; ANY throw → WARN `Store-name reconcile after a character change failed` `{trigger, characterId, error: message}` (root `logger`); never fails the caller.
- `:283-322` `create`: `withCreateDefaults(data, null)` → `_create` → `ensureCharacterVault(created)` → `:305` `reconcileVaultNames('character-created', created.id)` → INFO `Character created`.
- `:341-361` `createBoundToVault` (bug 185, carriers' scope): NO reconcile.
- `:399-420` `update`: `existing = findByIdRaw(id)` (validated; Zod-failing row → null) … `result = hasDbWork ? _update : _findById`; `:413-415` `if (result && existing && data.name !== undefined && data.name !== existing.name)` → `reconcileVaultNames('character-renamed', id)` (BEFORE the closing overlay read). A JSON `null` name counts as "defined and different".
- `:496-512` `delete`: `result = _delete(id)`; if true → INFO `Character deleted` `{characterId}` → `:504` `reconcileVaultNames('character-deleted', id)`.
- v4 callers of these in a JOB CHILD (where the reconcile would skip as `job-child`): measured — `character-avatar.ts:83` and `character-headshoulders-backfill.ts:145` update `avatarOverrides` / `physicalDescription` only (never `name`), and no job handler creates or deletes a character. So the job-child arm is unreachable from the character triggers; v5 has no job child at all → that arm is NO-PORT (record it).

### 1.8 Provisioning names (UNCHANGED by the span — answers "does `ed8b15b50` change the provisioning name?": NO)

- `lib/mount-index/character-vault.ts:151` `vaultName = `${character.name} Character Vault`` (UNTRIMMED, no `Untitled` fallback); adoption `findByName(vaultName)` filtered to `storeType === 'character'` (`:154-169`); create `:185-186` `nextUniqueMountPointName(allStoreNames, vaultName)` → `docMountPoints.create` (now name-gated). What CHANGED is that `create` immediately reconciles afterwards, so a provisioned `"X Character Vault (2)"` beside an orphan `"X Character Vault"` ends the create as `"X Character Vault"` with the orphan retired; an untrimmed/blank-named character's vault ends trimmed / `Untitled …`.
- `ensure-official-store.ts:132` (project/group stores, `storeType='documents'`) unchanged; the reconcile never retires them, only suffixes a collision.
- The built-in mounts (`provision-*-mount.ts` migrations, raw `INSERT`) unchanged; ledger-gated in v4.

### 1.9 Call sites of `reconcileStoreNames` (`ed8b15b50`)

- **Boot** — `instrumentation.ts:634-645`: PHASE 3.2, inside the fire-and-forget `backfillCharacterVaults().then(async () => { … })`: FIRST statement of the `.then` (`:642-645` `reconcileStoreNames('boot')`), BEFORE `migrateVaultPhysicalFiles`, `refreshVaultWardrobe`, `moveSharedWardrobeToGeneral`, `enqueueHeadShouldersBackfill`. A throw skips the rest of that chain and lands in `:678-683` WARN `Error during character vault backfill or physical-file migration` `{context: 'instrumentation.register', error}`. Under a degraded mount index the backfill's per-character catches log and complete, then the reconcile logs its own `mount-index-unavailable` WARN.
- **Restore** (carriers own the site) — `lib/backup/restore/restore.ts:602-614` 22a picks a free name per incoming store (`takenStoreNames` = all current names; `nextUniqueMountPointName(taken, mpData.name)`; DEBUG `Restored document store under a free name` `{mountPointId, archived, name}`; archive `createdAt` kept because the retired stamp reads it) and `:1279-1290` 23a `reconcileStoreNames('restore')` → DEBUG `Post-restore store-name reconcile complete` `{renamed: count, skippedReason}`; a throw → `warnings.push('Failed to tidy document-store names after restore: <msg>')` + WARN `Post-restore store-name reconcile failed` `{error}`.
- **`.qtap` import** (carriers own the site) — `lib/import/quilltap-import/execute.ts:986-994`, after `importWardrobeItemImages`, before the re-embed: `reconcileStoreNames('qtap-import')`; throw → `warnings.push('Failed to tidy document-store names after import: <msg>')` (no log line). And `import-document-stores.ts:82-95` overwrite arm: `overwriteName = existing.name.trim().toLowerCase() === mp.name.trim().toLowerCase() ? mp.name : nextUniqueMountPointName(new Set([...takenNames].filter(n => n.trim().toLowerCase() !== existing.name.trim().toLowerCase())), mp.name)`; `takenNames.add(overwriteName)` (the old name is NOT removed from `takenNames`); `update(existing.id, {name: overwriteName, …})`.
- **Characters** — §1.7, and therefore TRANSITIVELY: every restore `repos.characters.create` (`restore.ts:246`, the unbound arm), every import create (`import-characters.ts:148/183`), the import overwrite delete (`:138`), `quilltap-import/reconcile.ts:325` update (only if the name changes), and `delete-service.ts:172` (`deleteUserData` — replace-mode restore AND `deleteAllUserData`), which fires one reconcile per character inside a `Promise.all`. Those wipe-time reconciles are STATE-INERT (`clearFormat3Entities` `:77-88` then deletes every `doc_mount_points` row) but they log INFO lines with trigger `character-deleted`.

### 1.10 Prose

- `docs/developer/DDL.md` `:1829-1838` (doc_mount_points section): the "deliberately no DB unique index" paragraph REPLACED by the `CREATE UNIQUE INDEX` block + the chokepoint/backstop paragraph + a new "Character-vault names" paragraph (the rules of §1.2 + call sites + "parent-only … two-phase transaction … The mount-points rename route refuses to rename a live vault (409)").
- v4 `CLAUDE.md` gains one standing bullet: "Document-store names are one case-insensitive namespace … pick a free name with `nextUniqueMountPointName`, never insert one raw … A new path that creates, renames or unlinks a vault calls it rather than naming one by hand."
- `help/mount-points.md` `:75-77`: the "Store names are unique" bullet rewritten + two NEW bullets ("A character's vault wears the character's name", "Old vaults are retired, not discarded" with the example `Tester Version 2026-10-09T164506Z Store`). (`system-backup-restore.md`, `wardrobe.md` are the carriers/wardrobe clusters' text.)

### 1.11 `197104649` — the sync disk read (log line only)

- `lib/mount-index/sync/apply-disk.ts:55-60` NEW `readDiskFile(targetPath, relativePath)`: `absolute = resolveInTarget(targetPath, relativePath)` FIRST (an escape throws before any log) → DEBUG `[Sync] Reading disk file` `{relativePath}` (root `logger`, the file's `:16` import) → `fs.readFile(absolute)`.
- `lib/mount-index/sync/index.ts:266-279` `bytesFor`: the store side unchanged; the disk side `return readDiskFile(targetPath, action.relativePath)` (was the inline dynamic-import `fs.readFile(resolveInTarget(...))`). Behaviour identical; one DEBUG per disk-side read.
- New `__tests__/unit/lib/mount-index/sync/disk-boundary.test.ts` (56 lines) — an import-boundary guard (only `apply-disk.ts` may touch `fs`) — v4-test-only, nothing to port (v5's `sync/` already reads only through `apply_disk::resolve_in_target`).

---

## 2. v5 today — file:line counterparts

### 2.1 The naming leaf and the repair

- `crates/quilltap-core/src/db/ensure_official_store.rs:54-67` `next_unique_mount_point_name(&HashSet<String>, &str)` — v4's leaf; ⚠ uses Rust `str::trim` (Unicode `White_Space`: trims `\u0085`, does NOT trim `﻿`) where v4's `.trim()` is JS (`crate::jsstr::js_trim`). A latent divergence on exotic names; the new port should use `js_trim` and the leaf can be converged in the same unit (pin with a row).
- `crates/quilltap-core/src/db/mount_index_case_repair.rs`:
  - `:1-52` module header — still says "vault names: app-level … no DB index, because backup restore must be able to recreate legacy rows verbatim" (`:13-16`) — must be REVERSED.
  - `:98-112` `nocase_unique_index_is_valid` (reusable); `:114` `table_exists`.
  - `:132-141` `next_suffixed_name` (repair-local, NOT trimmed — the repair passes `loser.name.trim()`).
  - `:168-178` `keep_oldest_less` (ICU `locale_compare`) — reuse for `byAge`.
  - `:538-603` `repair_mount_point_name_collisions` — ported; ⚠ logs **`tracing::info!`** with snake_case fields (`mount_point_id`, `kept_mount_point_id`, `old_name`, `new_name`) where v4 logs **WARN** with camelCase `{mountPointId, keptMountPointId, oldName, newName}`. Same pre-existing divergence on the folder (`:322-330`, v4 `{mountPointId, keptFolder, oldName, newName, oldPath}`) and link (`:437-…`, v4 `{mountPointId, keptPath, oldPath, newPath}`) lines. Not this span's drift, but this unit edits the file: a rider.
  - NO `ensure_mount_point_name_unique_index`, no `MOUNT_POINT_NAME_NOCASE_INDEX` const.
- `crates/quilltap-core/src/services/builtin_mounts.rs:246-350` `ensure_mount_index_tables` — the boot hook where v4's three lazy `onTableEnsured` sites collapse. `:318-324` the `doc_mount_points` `lazy_ensure` closure: `ensure_doc_mount_points_columns` then `repair_mount_point_name_collisions` — THIS is where the ensure replaces the bare repair (same order as v4's `onTableEnsured`; one closure, one ensure-failure line `Failed to ensure doc_mount_points table in mount index database` under `LogAndContinue`, recorded for the P4.D248 structural pass). Runs from `ensure_builtin_mounts_with` (`:131-157`), from the host boot (`quilltap-host/src/host.rs:1916-1920`, `LogAndContinue`) and from fresh provisioning (`Propagate`).
- ⚠ `builtin_mounts.rs:359-391` `ensure_one_mount` re-provisions a built-in store EVERY boot when its pointer dangles, via the raw `insert_mount_row` (`:401-428`) with the fixed name (`Lantern Backgrounds` / `Quilltap Uploads` / `Quilltap General`) and NO free-name pick. With the unique index present (it is created earlier in the same pass), a dangling pointer + any store already holding that name in any casing → `UNIQUE constraint failed` → the provision PROPAGATES → v5 boot FATAL. v4's migration is ledger-gated and never re-runs, so v4 never meets this. Edge, but a hard-stop shape — the lane must decide (measure: plant it; options: adopt-by-name, or free-name pick, or record).

### 2.2 The repository

- `crates/quilltap-core/src/db/doc_mount_points.rs`:
  - `:165-202` `create(&DmpCreate, &CreateOptions) -> Result<(), DbError>` — a raw INSERT, NO name gate.
  - `:205-296` `update(id, &DmpUpdate) -> Result<bool, DbError>` — NO name gate.
  - `:437-447` `find_all_names` (raw `SELECT name`), `:503` `find_by_name`, `:740` `count_by_name`, `:798-810` `find_all_full_json` (raw, unsorted, no Zod drop).
  - No `find_name_holder`, no taken-name error type.
- Every v5 production caller of `create`/`update` (all would pass the new gate): `db/character_vault.rs:332` (vault provision), `db/ensure_official_store.rs:129`, `api/mount_points.rs:239` (create) / `:484` (PATCH), `services/quilltap_import/document_stores.rs:188` (overwrite update — still writes `mp_name` VERBATIM: v4's new `overwriteName` is NOT ported) / `:243` (create; free name via `next_unique_mount_point_name` at `:224` — already v4's shape), `services/quilltap_import/reconcile.rs:141` (the scaffold-name hand-back), `services/backup/restore/orchestrator.rs:2246` (restore 22a — NO free-name pick today; legacy duplicates are inserted verbatim and the boot repair suffixes them). Raw INSERTs outside the repo: `builtin_mounts.rs:412` (v4's migration is raw too), `db/database_store.rs:801` and the photos/embedding test plants (tests only).

### 2.3 Characters — create / rename / delete chokepoints

- **Create**: `db/character_vault.rs:440-458` `create_character` → `:464-490` `create_character_with_options(main, mount, slim, vault, opts)` — slim row with NULL FK, then `ensure_character_vault` (`:240-…`; vault name `:267` `format!("{character_name} Character Vault")`, adopt `:294-313`, free name `:319-327`, create `:332`). This is v4's `create` (every v5 create path funnels here: `api/characters.rs:1370/1424/1499`, `services/quilltap_import/characters.rs:339`, `services/backup/restore/orchestrator.rs:3090`). No INFO `Character created` line in v5.
- **Rename**: `db/vault_character_update.rs:371-404` `update_character(main, mount, id, &Map) -> Result<bool, OverlayError>` — reads `existing = characters_read::find_by_id_raw` FIRST (`:381-383`), runs the write overlay, the slim update, the null clears. v4's `update` twin; every rename path funnels here (`api/characters.rs:1119/1172/1237/1281/2308`, `generators/rename.rs:734`, `pascal/side_effects.rs:784`, …).
- **Delete**: `db/characters.rs:442-447` `CharactersRepository::delete` — MAIN-ONLY (cannot reconcile). Its three production callers: `services/cascade_delete.rs:388` (inside `execute_cascade_delete(main, mount, …)`, `:308`; the API delete `api/characters.rs:2164`, and `services/quilltap_import/reset.rs`), `services/quilltap_import/characters.rs:291-293` (import overwrite; v4 `import-characters.ts:138`), `services/delete_all.rs:373-377` (v4 `delete-service.ts:172`, replace-mode restore + delete-all-data).
- **Raw character reads**: `db/characters_read.rs:319-321` `find_all_raw(main)` = `query_raw(main, "", &[])` (`:230-248`) — `SELECT <cols> FROM characters`, NO per-row Zod drop, an error PROPAGATES (v4: drop + `[]`). For the reconcile and the PATCH owner check the lane needs v4's fallback-read shape (the P4.149 `chats_read::find_all` precedent) or a recorded divergence.

### 2.4 The schema surface

- `crates/quilltap-core/src/services/provisioning/fresh_schema.json` — mount-index carries the folder/link NOCASE pair (`:110`, `:114`) and `idx_doc_mount_points_createdAt`, NOT the new index. **Measured: a HEAD re-dump adds exactly `CREATE UNIQUE INDEX "idx_doc_mount_points_name_nocase" ON "doc_mount_points" ("name" COLLATE NOCASE)`** (plus the memory programme's two table rows). → a D23 re-dump line; `provisioning_equivalence` (1d, `QT_FRESH_SCHEMA_LIVE` required) goes red at HEAD until the re-dump lands — the tripwire firing as designed.
- `crates/quilltap-core/src/services/provisioning/migration_indexes.json` — UNAFFECTED: no v4 migration creates the index (the only creator is `mount-index-case-repair.ts`), and `dump-migration-indexes.ts` omits any name `fresh_schema.json` carries. (The lane can confirm with the dumper at the pin; prediction: byte-identical.)
- It STILL needs the v5 boot ensure (§2.1): every pre-round v5-provisioned instance and every v4 instance not yet booted at `ed8b15b50`+ lacks the index, and v4 re-checks it every table init (tampered-index replacement included).

### 2.5 The routes

- `crates/quilltap-core/src/api/mount_points.rs`:
  - `:44-61` `find_store_name_clash(db, desired, exclude_id) -> Option<String>` — `SELECT id, name` (⚠ with the new index this may be served by the covering NOCASE index — order irrelevant to a single match, but do not reuse it as a findAll); `str::trim` + `to_lowercase` (should be `js_trim`).
  - `:194-260` `mount_point_create` — 409 sentence matches; ⚠ v5 logs NO `[Mount Points v1] Rejected duplicate mount point name` WARN (pre-existing gap).
  - `:356-500` `mount_point_update` — per-field validation (a failure → 500, v4's catch-all quirk), then `:461-475` the duplicate check (409, sentence matches; ⚠ NO `Rejected duplicate mount point rename` WARN). NO live-vault refusal; reads only the mount index (`db.read_mount_index`). The new check belongs between `:459` (after `flipped`) and `:461`, needs a MAIN read (`characters_read` raw, `characterDocumentMountPointId == id`, first in row order).
  - Engine wiring `api/engine.rs:3225/3233`; 409 is the existing `ErrorKind::Conflict` → no new web route, no new `COLLAPSE_CENSUS` row (the census keys routes, and these exist).
- SPA: `apps/web/src/app/screens/scriptorium/scriptorium.store.ts:77` / `scriptorium-list.ts:204` (`updateStore`) surface the 409 message as-is; v4 `ed8b15b50` changed NO component → no SPA unit (an optional Playwright beat: rename a live vault → the sentence).

### 2.6 The boot sequence (where the boot reconcile goes)

- `crates/quilltap-host/src/host.rs:1272-…` `seed_built_ins` — a fresh-thread `db.write_blocking(|ws| …)` with `main` and (when present) `mount_index`. The mount-aware block `:1709` `if let Some(mi) = ws.mount_index() { … }` runs, in order, … `:1916-1920` `ensure_builtin_mounts_with(..., LogAndContinue)` (→ the index ensure) → `:1924-1931` general Scenarios folder → `:1939-1955` general `state.json` → `:1987-1991` P4.82 `enqueue_headshoulders_backfill` (v4's LAST step of the same `.then` chain). v5 has NO `backfillCharacterVaults` / `migrateVaultPhysicalFiles` / `refreshVaultWardrobe` / `moveSharedWardrobeToGeneral` boot stage (`character_vault.rs:184-200`, `headshoulders_backfill_enqueue.rs:28-29` record the absence; the lazy adopt in `ensure_character_vault` stands in).
- **Proposed placement:** immediately BEFORE the P4.82 enqueue (`:1987`) — v4 order is reconcile → (absent stages) → head-shoulders enqueue, and a reconcile throw in v4 SKIPS the enqueue with WARN `Error during character vault backfill or physical-file migration` `{context: 'instrumentation.register', error}` (v5 logs that line nowhere today — the lane adds it at this arm). Since planStoreNames never retires a name a vault-less character bears, running without the backfill is safe (the adopt-first ordering v4's comment cites is preserved by the `adoptableNames` guard).
- **Degraded mount index (P4.159):** `ws.mount_index()` is `None` → the whole block is skipped. v4 still reaches the reconcile and logs WARN `Store-name reconcile skipped: mount-index database unavailable` `{trigger: 'boot'}` — the lane adds that line in an `else` arm (v5's boot already tolerates the degraded partition; no writer, so no write is attempted).
- **Transactions inside the closure:** the reconcile's ONE transaction must nest inside `write_blocking`'s connection state — use a SAVEPOINT (the P4.D248 precedent, `db/table_shape.rs:461-467`) unless the lane measures `conn.is_autocommit()` true there.

### 2.7 Sync (`197104649`)

- `crates/quilltap-core/src/services/mount_index/sync/mod.rs:483-510` `bytes_for` — the store side matches; the disk side `:505-509` calls `apply_disk::resolve_in_target` (`sync/apply_disk.rs:79`) then `std::fs::read` inline, mapping the error through `node_io_message(&e, "open", &absolute)`. Missing: the DEBUG line. The faithful shape: a `pub(crate) fn read_disk_file(target, rel) -> Result<Vec<u8>, String>` in `apply_disk.rs` (resolve → `tracing::debug!(target: "quilltap::mount_index", relativePath = %rel, "[Sync] Reading disk file")` → read), `bytes_for` delegating. Existing debug convention in the file: `apply_disk.rs:356-360`.
- `crates/quilltap-harness/tests/sync_engine_equivalence.rs` does NOT capture logs, and its oracle (`harness/oracle/cases/sync-engine.test.ts:132-134`) MOCKS `logger.debug/info/warn` — so no family pins (or will redden on) the line. Pin it in-crate with `crate::test_support::captured_with` (`test_support.rs:129`; mind the memory note: a `Db::write` closure is invisible to `captured_with` — `bytes_for` is not a writer closure, so a direct capture works) — one positive (a disk-side create/modify logs `{relativePath}` once) and one negative (an escaping path logs nothing).

---

## 3. What the ledger row / commit prose got wrong (or left open)

1. **"possibly `fresh_schema.json` (the NOCASE index — measure)" → MEASURED YES.** The index is in the generateDDL surface at HEAD (repository `ensureCollection` creates it). It is a D23 re-dump line, and `provisioning_equivalence` will redden at HEAD on it (together with the memory programme's table moves) — one re-dump for the round.
2. The ledger lists `services/builtin_mounts.rs` only as a naming site; it is ALSO the boot-ensure home (§2.1) and the owner of a new boot-FATAL edge (dangling built-in pointer + squatted name, §2.1).
3. The ledger's predicted-red list names `mount_case_repair_equivalence`. **Predicted GREEN** at HEAD: its oracle (`harness/oracle/cases/mount-case-repair.ts:160-163`) calls `repairMountPointNameCollisions` directly (body unchanged) and dumps only the folder/link tables' indexes. It must be EXTENDED (§4), not regenerated-to-green.
4. Commit prose "only the API routes enforced the namespace" — v4's auto-provisioning paths already picked free names; what was missing was the repository gate + the index. Prose "restore/import pick free names" — import CREATE already did; the new parts are restore 22a and the import OVERWRITE arm.
5. Not in the ledger: v5's three case-repair lines are INFO/snake_case vs v4's WARN/camelCase (pre-existing); v5's two `[Mount Points v1] Rejected duplicate …` WARNs are missing (pre-existing); v5 `next_unique_mount_point_name` / `find_store_name_clash` use Rust `trim` not `js_trim` (pre-existing).
6. v4's own `mount-index-case-repair.ts` header (`:12-15`) still claims "no DB index" — stale in v4; v5's copy of that header must be reversed regardless.

---

## 4. Differential plan

### Existing families — predictions at the HEAD pin

- `provisioning_equivalence` — **RED** at HEAD (1d live generateDDL) until the D23 re-dump adds the index line (shared with the memory programme's re-dump).
- `mount_case_repair_equivalence` — predicted GREEN (see §3.3); extend.
- `mount_points_routes_equivalence` (oracle `harness/oracle/cases/mount-points-routes.test.ts`, groups-projects fixture) — predicted GREEN: `create_name_clash` / `patch_name_clash` / `patch_name_case_only_self` keep their 409 bodies; `patch_happy` targets `GAMMA_EXTRA_MP` (a project store, `:84`), not a vault. Responses only — no log capture. Extend.
- `doc_mount_points` tier-2 (`harness/oracle/cases/doc-mount-points-tier2.ts`) — predicted GREEN unless its corpus creates a taken name (re-run at the pin to confirm).
- **Character families — re-run at the pin; red wherever a corpus creates / renames / deletes a character on a fixture holding a non-conforming store name** (the post-change reconcile then renames OTHER stores and bumps their `updatedAt`): `characters_create_tier2_equivalence`, `characters_provision_tier2_equivalence` (any namesake `(2)` vault now ends plain with the older one retired), `characters_adopt_tier2_equivalence` (an ambiguous-adoption fallback now retires the populated orphans), `characters_update_tier2_equivalence`, `character_rename_equivalence`, `character_archive_tier2_equivalence`, `characters_arrays/physical/scaffold_tier2`, `vault_character_write_equivalence`, plus cascade/delete families. Oracle cases that call the character mutators: `characters-adopt.ts`, `characters-mutations.test.ts`, `character-rename.test.ts`, `characters-physical.ts`, `characters-arrays.ts`, `characters-provision.ts`, `characters-update.ts`, `characters-create.ts`, `characters-slim.ts`, `external-prompt-tier3.test.ts`, `images-routes.test.ts`, `pascal-side-effects.test.ts`, `post-office-routes.test.ts`, `post-office-writers-tier3.ts`, `realtime-topics.ts` (`realtime-topics` may gain a `mountPoints` hint on a rename-producing create).
- Restore/import families (`system_restore_state`, `restore_vintage_state`, `system_import_state`, `qtap_import_equivalence`, `backup_uuid_remap_equivalence`) — red at HEAD for bug 185 AND this cluster (free names + end-of-run reconcile + per-character reconciles); carriers own the regen, this cluster owns the reconcile semantics they compare.

### New cases (all against v4's REAL code)

1. **Tier-1 `store_names_equivalence`** (NEW oracle `harness/oracle/cases/store-names.ts`, imports v4's real `lib/mount-index/store-names.ts`): per row emit `characterVaultName`, `vaultBaseName`, `storeNameTimestamp`, `retiredVaultName`, and `planStoreNames(stores, characters)` (the rename list, ORDER and key order included). Corpus: every jest case in `__tests__/unit/lib/mount-index/store-names.test.ts` (`:39-154`: the V4test case, rename, delete, two live namesakes with the holder keeping, lowest-free suffix, older-gets-plain, adoptable orphan left, operator-renamed left, live-before-other + oldest-first collisions) PLUS adversarial rows: blank / whitespace-only / leading-trailing-space names; `﻿`, ` `, `\u0085`, ` ` around names (the JS-vs-Rust `trim` / `\s` / `.` classes); `TESTER CHARACTER VAULT`; `ſ`/Kelvin-sign spellings (non-unicode `/i`); `Character Vault` alone; `X Character Vault (2) (3)`; `X Character Vault(2)` (no space); equal `createdAt` tie-broken by id; non-ISO / offset / NaN `createdAt` (`Date.parse` fallback + `replace(/:/g,'')`); year 10000; archived owner; dangling pointer; two characters on one store; retired-name collisions (two retirees of one base and one createdAt → `(2)`); a `documents` store squatting a live vault's canonical name; idempotence (plan the applied result → `[]`).
2. **Tier-2 `store_name_reconcile_equivalence`** (real DB, the strong one — anti-ping-pong): a v4 builder plants main `characters` + mount `doc_mount_points` (with the NOCASE index) per scenario; the oracle runs v4's REAL `reconcileStoreNames(trigger)` (real repositories, `QUILLTAP_JOB_CHILD` unset, logger transports spliced per the `v4-oracle-job-host-and-log-capture` note) and dumps BEFORE/AFTER `doc_mount_points` rows (`id, name, storeType, createdAt`, `updatedAt` → `<now>` iff renamed), the result object, and the captured lines (DEBUG `planned` keys, each INFO `{trigger,id,from,to,reason}` in order). v5 runs the port over the same plants. Scenarios: the jest swap case (`reconcile-store-names.test.ts:55-66`, two-phase through the index), a no-op instance (zero writes, zero lines but `planned`), a Zod-dropped store row holding a name (phase-2 UNIQUE → rollback → the error text), a Zod-dropped character row (its vault RETIRED — v4-faithful), degraded mount index (the WARN; job-child arm NO-PORT), and **a sanitized Friday-shaped fixture** (`quilltap-fixture-sanitizer` output) planned both ways — byte-equal rename lists are the ping-pong proof. Plus a **cross-run** assertion: v4-reconciled DB → v5 reconcile = no-op, and v5-reconciled → v4 = no-op.
3. **Tier-2 index ensure** — extend `mount-case-repair-spec.json` + `mount-case-repair.ts` with `run: 'mountpoints-index'` (calls v4's `ensureMountPointNameUniqueIndex`) and widen the index dump to `doc_mount_points`: the jest case (`mount-index-case-repair.test.ts` `+` hunk: repair then refuse `TESTER CHARACTER VAULT` with `/UNIQUE/`), a tampered same-name non-UNIQUE index (DROP + CREATE), a valid index (no-op, no line), idempotence, and the INFO line captured.
4. **Tier-2 repository gate** — extend the `doc-mount-points` tier-2 oracle: create refused (`' tester character vault'` vs `Tester Character Vault`), update onto a peer refused, update re-casing own name allowed, update without `name` ungated; compare: no row written, the error message bytes, the WARN `{name, mountPointId, holderId}` (null on create).
5. **Routes** — extend `mount-points-routes.test.ts` (needs a fixture character pointing at a store — plant in the case's fresh copy): PATCH a live vault to a new name → 409 sentence + no write; PATCH re-casing a live vault → 409; PATCH a live vault with its EXACT name → 200; PATCH an archived character's vault → 409; and capture the three `[Mount Points v1]` WARNs (two pre-existing + one new).
6. **Character chokepoints, tier-2** — mirror `characters-create.ts` / `characters-update.ts` / a delete case on a fixture carrying an orphan `"X Character Vault"`: create X → new vault plain + orphan retired; rename X→Y → vault follows; delete → retired; a reconcile failure (planted dropped-row collision) → the WARN `Store-name reconcile after a character change failed` `{trigger, characterId, error}` and the character write still committed.
7. **Boot (host)** — `host_boot_hardness`-style in-crate tests: the boot renames a planted orphan exactly once (second boot zero lines); a degraded mount index logs the skip WARN; a planted failure logs v4's `Error during character vault backfill or physical-file migration` and the head-shoulders enqueue does NOT run.
8. **Sync** — in-crate capture pins for `[Sync] Reading disk file` (§2.7). No oracle change.

v4 jest tests a lane can mirror: `__tests__/unit/lib/mount-index/store-names.test.ts` (154 lines), `reconcile-store-names.test.ts` (73), the `mount-index-case-repair.test.ts` `+25` hunk, `lib/database/repositories/__tests__/doc-mount-points-find-by-name.test.ts` `+32` (the chokepoint), `disk-boundary.test.ts` (nothing to mirror).

---

## 5. Proposed unit decomposition (one lane, "store names"; the sync line can ride it)

**U1 — the pure naming module.** NEW `crates/quilltap-core/src/mount_index/store_names.rs` (or `services/mount_index/store_names.rs` — wherever `mount_index` leaf modules live): `character_vault_name`, `vault_base_name` (hand-rolled JS classes, not `regex` `\s`/`(?i)`), `store_name_timestamp` (`episodic::js_date_parse_ms` → `clock::iso_from_unix_ms`), `retired_vault_name`, `StoreNameRow`, `VaultOwnerRow`, `StoreRename {id, from, to, reason}`, `plan_store_names`. Converge `db/ensure_official_store.rs` `next_unique_mount_point_name` onto `js_trim` (one-line + a row). Tests: NEW `crates/quilltap-harness/tests/store_names_equivalence.rs`, NEW `harness/oracle/cases/store-names.ts`, NEW `harness/oracle/fixtures/store-names-corpus.json`. Owns: those + `db/ensure_official_store.rs` (the leaf only) + the module registration line.

**U2 — the index ensure.** `db/mount_index_case_repair.rs`: `MOUNT_POINT_NAME_NOCASE_INDEX`, `ensure_mount_point_name_unique_index` (repair → validity → DROP/CREATE → INFO `Mount-index: created the unique document-store name index`), header reversed; rider: the three repair lines to WARN with v4's camelCase keys. `services/builtin_mounts.rs:318-324` calls the ensure in place of the bare repair (+ its module comments). Decide/record the dangling-built-in-pointer FATAL edge (§2.1). Tests: extend `harness/oracle/cases/mount-case-repair.ts`, `harness/oracle/fixtures/mount-case-repair-spec.json`, `crates/quilltap-harness/tests/mount_case_repair_equivalence.rs`; `builtin_mounts.rs` in-file tests (`:469-…`, `:728-…`).

**U3 — the repository gate.** `db/doc_mount_points.rs`: `find_name_holder(name, except_id) -> Option<(id, name)>` (js_trim + lowercase, v4 `findAll` drop semantics or recorded), a `MountPointNameTaken { requested, holder_id, holder_name }` error (new `DbError` arm or a dedicated type — its `Display` = v4's message bytes), the WARN, gate inside `create` and `update` (when `name` is `Some`). `api/mount_points.rs` switches `find_store_name_clash` to it and adds the two missing `[Mount Points v1] Rejected duplicate …` WARNs. Tests: extend `harness/oracle/cases/doc-mount-points-tier2.ts` + its harness test. ⚠ Lands TOGETHER with restore 22a's free-name pick (§8) or restore of a legacy archive with duplicate names starts dropping stores.

**U4 — the reconcile.** NEW `crates/quilltap-core/src/db/store_name_reconcile.rs` (or beside U1): `reconcile_store_names(main: &Connection, mount: Option<&Connection>, trigger: &str) -> Result<StoreNameReconcileResult, DbError>` — `None` → WARN skip + `skipped_reason = MountIndexUnavailable`; reads via fallback-shaped `find_all` (stores) + raw characters (v4 drop semantics); DEBUG `planned`; SAVEPOINT two-phase UPDATE with one `now`; per-rename INFO (target `quilltap::mount_index`, `module = "mount-index:reconcile-store-names"`, fields `trigger, id, from, to, reason`); realtime `MountPoints` hint when renamed > 0 (check whether `write_apply`'s post-commit hook already emits it for a writer-closure write — avoid a double hint; `realtime/publish_sites.rs` pin). Plus `reconcile_vault_names_or_warn(main, mount, trigger, character_id)` (the swallow + WARN `Store-name reconcile after a character change failed` `{trigger, characterId, error}`). Tests: NEW `crates/quilltap-harness/tests/store_name_reconcile_equivalence.rs`, NEW `harness/oracle/cases/store-name-reconcile.ts` (+ builder under `harness/oracle/fixtures/`).

**U5 — the character call sites.** `db/character_vault.rs` `create_character_with_options` (after `ensure_character_vault`, trigger `character-created`); `db/vault_character_update.rs` `update_character` (v4's condition: `existing` Some, update did not error, patch has `name` and it differs — JSON-null counts); delete: a NEW single `delete_character_and_reconcile(main, mount, id)` (where it lives is the lane's call — `db/characters.rs` can't, it is main-only) used by `services/cascade_delete.rs:388`, `services/quilltap_import/characters.rs:291-293`; `services/delete_all.rs:373-377` per §6 Q3. A census test: `CharactersRepository::delete` has no other production caller. Tests: the character families' regen at the pin + U4's scenarios through these entry points.

**U6 — the PATCH live-vault refusal.** `api/mount_points.rs` `mount_point_update`: after validation, before the duplicate check, when `name` present and `!= existing.name` exactly → main raw characters read → first owner → WARN `[Mount Points v1] Rejected rename of a live character vault` `{mountPointId, characterId, userId}` → 409 sentence. Tests: extend `harness/oracle/cases/mount-points-routes.test.ts` + `crates/quilltap-harness/tests/mount_points_routes_equivalence.rs`.

**U7 — the boot call.** `crates/quilltap-host/src/host.rs` (`seed_built_ins`, before `:1987`): `reconcile_store_names(main, Some(mount_index), "boot")`; Err → WARN `Error during character vault backfill or physical-file migration` `{context: "instrumentation.register", error}` and SKIP the head-shoulders enqueue; the degraded `else` arm → the skip WARN. Tests: the host boot test file(s) (`crates/quilltap-host/tests/host_boot_hardness.rs` or wherever P4.135/P4.159 pinned the boot).

**U8 — prose.** `docs/v4/developer/DDL.md` (the doc_mount_points section, re-vendored), `help/mount-points.md` (vendored; `help_tree_equivalence` on a HEAD regen — coordinate with whichever lane re-vendors `help/` wholesale this round), the v5 module headers; optionally a v5 CLAUDE.md standing rule mirroring v4's new bullet (unifier's call).

**U9 — the sync line (`197104649`).** `services/mount_index/sync/apply_disk.rs` (new `read_disk_file`) + `services/mount_index/sync/mod.rs:505-509` delegating + in-crate capture tests. Independent of U1–U8; can ride any lane that does not own `sync/`.

The D23 re-dump line (`fresh_schema.json`) belongs to the round's single keystone re-dump lane, not this one (§8).

---

## 6. Open questions / measurements before coding

1. **Write-closure transaction state** (U4/U7): is the `write_blocking` / `Db::write` connection already inside a transaction where the reconcile runs (boot, inside create/update/delete, inside restore)? Measure `is_autocommit()`; SAVEPOINT either way is safe.
2. **Fallback-read semantics** (U3/U4/U6): port v4's `_findAll` (Zod drop + `[]` on error) for both reads, or record a divergence? Faithful has a sharp edge — a failed characters read retires every vault — but a divergence here IS a ping-pong source on a shared instance with one corrupt row. Recommend faithful + a dogfood plant.
3. **Wipe-time reconciles** (`delete_all.rs`, replace-mode restore): v4 fires N concurrent reconciles whose writes are erased by `clearFormat3Entities`. Measure v4's actual line sequence (tier-2 over `deleteUserData` with logger capture); then port as one reconcile per delete (likely: the first logs every retirement, the rest are no-ops) or record a log-cadence divergence.
4. **Ordering inside restore/import**: the intermediate per-character reconciles (unbound restore creates; import creates/overwrites) change INTERMEDIATE names, and since a live vault "holding canonical" wins ties, the end state can depend on them. The carriers lane must keep v5's step order identical to v4's around every character create and the store inserts — measure with the restore/import families at the pin.
5. **The dangling built-in pointer** (§2.1): plant it on a v4-HEAD-booted copy and see v5's boot fail; rule: adopt the same-name store, pick a free name, or record.
6. **Friday's state**: whether live Friday has already been booted by v4 at `ed8b15b50`+ (index present, names reconciled). If not, v5's first boot after this round renames stores on the dogfood copy — measure `planStoreNames` on the copy via the oracle BEFORE booting v5, and confirm v4-then-v5 is a no-op (§4.2).
7. **`realtime` double-hint**: whether `write_apply`'s post-commit hook already publishes a `mountPoints` hint for `doc_mount_points` writes made inside a writer closure (`realtime/job_topics.rs:135`).

---

## 7. v4 jest tests the lane can mirror (paths)

- `__tests__/unit/lib/mount-index/store-names.test.ts` (NEW, 154 lines).
- `__tests__/unit/lib/mount-index/reconcile-store-names.test.ts` (NEW, 73 lines — the swap; the job child).
- `__tests__/unit/lib/database/repositories/mount-index-case-repair.test.ts` (`+25`: `ensureMountPointNameUniqueIndex (bug 186)`).
- `lib/database/repositories/__tests__/doc-mount-points-find-by-name.test.ts` (`+32`: `name chokepoint (bug 186)` — create refused ignoring case + leading space; update onto a peer refused; own re-case and nameless update allowed).

---

## 8. Meeting points with other clusters

- **Carriers (restore / `.qtap` import, bug 185)** CONSUME:
  - `reconcile_store_names(main: &Connection, mount: Option<&Connection>, trigger: &str) -> Result<StoreNameReconcileResult, DbError>` with `StoreNameReconcileResult { renamed: Vec<StoreRename>, skipped_reason: Option<SkipReason> }` — restore 23a calls it with `"restore"` (DEBUG `Post-restore store-name reconcile complete` `{renamed: <count>, skippedReason}`; Err → the warning text + WARN `Post-restore store-name reconcile failed`), import with `"qtap-import"` (Err → the warning text only).
  - `next_unique_mount_point_name` (exists) for restore 22a's free-name pick (archive `createdAt` KEPT) and the import overwrite arm's `overwriteName` (`document_stores.rs:188`).
  - The rule that their **bound** character create (v4 `createBoundToVault`) and bound store create (`createBoundToStore`) do NOT reconcile, while every unbound create through `create_character_with_options` DOES (U5 puts it there).
  - **Hard landing dependency:** U3's repository gate + U2's index must not reach main before restore 22a's free-name pick, or a legacy/new-account archive with a colliding name loses stores (`Failed to restore document store …`). Either land in the same union with both lanes' tests green together, or move the 22a free-name pick into this lane (it is ~5 lines in `orchestrator.rs` — but that file is the carriers' — decide at planning).
- **Characters / wardrobe clusters**: any lane editing `db/character_vault.rs`, `db/vault_character_update.rs`, `services/cascade_delete.rs` or `services/quilltap_import/characters.rs` this round shares files with U5 — assign U5's call sites to ONE owner.
- **The D23 keystone (schema re-dump)**: `fresh_schema.json` gains the index line (measured); `migration_indexes.json` predicted unchanged. One re-dump for the round (the memory programme moves `chats`/`memories` in the same dump).
- **Help / docs vendoring**: `help/mount-points.md` (this cluster's text) rides whichever lane re-vendors `help/` + `docs/v4/` for the round.
- **Dogfood #156** (115 `with_both_conns` callers 500 on a degraded mount index): under a degraded mount index v5's rename/delete routes fail before reaching the reconcile, where v4 reaches it and logs the skip WARN — record, not fix, here.
- **Host boot (P4.159 owners)**: U7 touches `seed_built_ins`'s mount-aware block and its degraded `else`.
