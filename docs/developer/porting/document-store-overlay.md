# The document-store overlay — port design

The next big Phase-2/3 slice, and the first that crosses the repo boundary into a
*subsystem*. It is the thing standing between "the flat repos are ported" and
porting `projects`, `groups`, `characters`, and the `wardrobe` vault — four
entities whose substantive content does **not** live in their own table but in a
**document store**, merged in on read and routed out on write by an *overlay*.

This document is the design slice: what the overlay is, where the bytes actually
live, the build order, the pilot, and — the crux — how to differentially test a
subsystem that *looks* like it writes files but really writes DB rows. It is
informed by a full read of the v4 subsystem (`store-backed.repository.ts`,
`document-store-overlay.ts`, the `project-store`/`group-store` dirs, the
`vault-overlay` dir, `character-properties-overlay.ts`, `database-store.ts`,
`doc-mount-file-links.repository.ts`, and the `mount-index` provisioning). No code
is written yet; this is the plan its implementer follows.

## The one decisive fact: the "document store" is DB rows, not files

For every store-backed entity the port cares about (character vault, project
store, group store) the mount point is created with **`mountType: 'database'`**.
`lib/mount-index/database-store.ts:4-8` states it outright:

> For mount points with `mountType === 'database'`, document bytes live in
> `doc_mount_documents` inside `quilltap-mount-index.db` — there is no filesystem
> path.

So a character's `manifesto.md` or a project's `properties.json` is **not** a file
on disk. It is a `content` string in a `doc_mount_documents` row in the mount-index
sibling DB — the exact DB the harness already materializes for the doc_mount_*
tier-2 cases. The `source: 'filesystem' | 'database'` enum on `doc_mount_files`
only distinguishes user-mounted Obsidian/folder vaults (filesystem) from these
managed stores (always `'database'`, hardcoded at
`doc-mount-file-links.repository.ts:619,760`). **The store-backed repos never touch
the filesystem.**

This collapses the hard part of the problem. There is no filesystem
nondeterminism (mtimes, dir ordering, fsync) to normalize, and **no new fixture
category** — the overlay's storage primitive is just more writes against the same
encrypted mount-index DB the harness already stands up.

## How a logical document is stored — the seven tables

A write of `(mountPointId, relativePath, content)` lands through one transaction,
`linkDocumentContent` (`doc-mount-file-links.repository.ts:738-864`), touching up
to four tables:

| Table | Role | Holds bytes? | Ported? |
|---|---|---|---|
| `doc_mount_points` | mount config (name, mountType, storeType, stats) | no | ✅ |
| `doc_mount_folders` | folder hierarchy `(mountPointId, path)`, self-ref `parentId` | no | ✅ |
| `doc_mount_files` | content-identity row, keyed by `sha256`; `fileType` + `source` | no (identity) | ✅ |
| `doc_mount_file_links` | the location row: `(mountPointId, relativePath) → fileId`, UNIQUE; per-link policy/conversion metadata | no | ❌ **(1074 L — the gap)** |
| **`doc_mount_documents`** | **text content** in `content`; UNIQUE `fileId` | **YES (text)** | ✅ |
| **`doc_mount_blobs`** | **binary content** in `data` BLOB; UNIQUE `fileId` | **YES (binary)** | ❌ (later) |
| `doc_mount_chunks` | embedding chunks per `linkId` | derived (rechunked) | ✅ |

**Path → content** resolves through a 3-table join, not a direct lookup (documents
are content-addressed by `fileId`, not path-indexed):

```sql
doc_mount_file_links l   -- WHERE l.mountPointId = ? AND LOWER(l.relativePath) = LOWER(?)
  JOIN doc_mount_documents d ON d.fileId = l.fileId   -- d.content = the bytes
  JOIN doc_mount_files     f ON f.id     = l.fileId   -- f.fileType, f.source, f.sha256
```

Path comparison is **case-insensitive** (`LOWER(...)`) — a load-bearing detail
(`Manifesto.md` written, `manifesto.md` read, must hit). The batched
`findManyByMountPointsAndPath` (one query per file path across all mount points) is
what the overlay read uses to hydrate all four store files at once.

`linkDocumentContent`'s invariants the port must reproduce:
- **find-or-create `doc_mount_files` by `contentSha256`** — identical content
  written to two paths reuses ONE file + ONE document row, with two link rows
  (dedup-by-sha);
- **upsert `doc_mount_documents` by `fileId`** (UNIQUE) — this is where bytes land;
- **upsert `doc_mount_file_links` by `(mountPointId, relativePath)`** — rewriting a
  path updates the link in place, never duplicates;
- `fileSizeBytes` = UTF-8 byte length; `plainTextLength` = **JS `.length`** (UTF-16
  code units) — these differ for non-ASCII content (a real divergence risk: a Rust
  `str::len()` is UTF-8 bytes; `plainTextLength` must count UTF-16 code units);
- post-write `reindexSingleFile` (chunk → `doc_mount_chunks`) + `emitDocumentWritten`
  run **only** when `QUILLTAP_JOB_CHILD !== '1'` — the oracle suppresses both (see
  the test strategy).

## The two overlay families

### Family A — the generic store-backed engine (projects, groups)

One engine instantiated twice. `createDocumentStoreOverlay<T,P>`
(`document-store-overlay.ts`) + `AbstractStoreBackedRepository<T>`
(`store-backed.repository.ts`); each repo supplies a thin `StoreOverlayBinding`
(`store-backed.repository.ts:36-48`) with 8 members: `managedFields`,
`entityLabel`, `idLogKey`, `applyOverlay`, `applyOverlayOne`, `applyWriteOverlay`,
`writeManagedFields`, `ensureOfficialStore`.

The split is **authoritative-per-field, not merge-from-one**:
- the **slim DB row** owns `id`, `name`, `officialMountPointId`, `createdAt`,
  `updatedAt` (in the main DB);
- the **store** owns every *managed* field, projected into **four fixed files**:
  - `properties.json` — the keystone settings bag (`JSON.stringify(parse(props), null, 2)`);
  - `description.md`, `instructions.md` — raw markdown bodies (no frontmatter);
  - `state.json` — arbitrary JSON (`JSON.stringify(state ?? {}, null, 2)`).

Managed fields per entity (stripped from the DB row on every insert/update):
- **projects** — `description`, `instructions`, `state` + 16 `ProjectPropertiesSchema`
  keys (`project.types.ts:138-159`);
- **groups** — `description`, `instructions`, `state` + 2 keys (`color`, `icon`)
  (`group.types.ts:105-113`).

Read overlay (`hydrateOne`): `{ ...row, ...properties, description, instructions,
state }` — store overrides row on overlap. Missing `properties.json` or null mount
⇒ the typed `*StoreUnavailableError`. `description`/`instructions` use
`markdownToNullable` (`'' → null`, absent → `null`). `state.json` corrupt/absent ⇒
`{}` (non-fatal). **Failure asymmetry** (deliberate): `findById` (`applyOverlayOne`)
**throws**; `findAll` (`applyOverlay`) **logs + drops** the bad row.

Write overlay (`applyWriteOverlay`, runs *before* the DB write): routes touched
store fields to the four files (properties via read-modify-write so a partial patch
doesn't clobber), strips managed keys, returns the DB-only remainder. If that
remainder is empty, `update` skips SQL entirely and re-reads. **Store-write then
DB-write, no enclosing transaction, no rollback** — port the *ordering*, not a
transaction. The create path is a 5-step sequence: insert slim row → provision/adopt
official store (`ensureOfficialStore`) → set FK raw (`setOfficialMountPointId`,
bypassing the overlay) → `writeManagedFields` (all four files) → overlay re-read;
the "refuse to return a storeless entity" throw is load-bearing.

**Projects and groups are structurally identical** — same engine, same base, same
four file names; they differ only in the properties-bag size and that
`ProjectsRepository` layers roster ops + `prepareCreateData` + an in-memory
`findByCharacterId` on top. One Rust port generalizes both.

### Family B — the character/wardrobe markdown vault (later, separate slices)

The character vault is the same idea, much heavier: **nine** projection targets
(`character-properties-overlay.ts`), including folder-enumerated `Prompts/*.md`,
`Scenarios/*.md`, `Wardrobe/*.md`, plus `physical-prompts.json` and a
`physical-description.md`. Managed set: `identity`, `description`, `manifesto`,
`personality`, `exampleDialogues`, `pronouns`, `aliases`, `title`, `firstMessage`,
`talkativeness`, `physicalDescription`, `systemPrompts`, `scenarios`
(`vault-overlay/schema.ts:173`). `systemTransparency` is intentionally DB-only.

What makes it harder than projects/groups, and why it is deferred to its own slices:
- **YAML frontmatter round-trip** (`Wardrobe/*.md`, `Prompts/*.md`) via the `yaml`
  npm library — reproducing its quoting/escaping/folding/null rendering byte-for-byte
  in Rust is a dedicated sub-problem (`serde_yaml` is not a drop-in match). The
  JSON + hand-built-string files (properties, physical-prompts, scenario headings)
  are tractable; the YAML files are the risk.
- **`stableUuidFromString`** (`parsers.ts:154`): SHA-256 of `(mountPointId,
  relativePath)` → first 16 bytes → set UUID version 8 + variant. Backs
  systemPrompt/scenario/wardrobe ids that chat references depend on. A pure leaf —
  **port it first with a tier-1 test.**
- **A concentration of the deferred ordering seams**: folder reads sort by
  `relativePath.localeCompare(...)` (ICU collation), slug/title use `.toLowerCase()`
  and case-insensitive collision suffixes (`-1`, `-2`). The vault overlay is the
  forcing function for the long-deferred ICU-collation + Unicode-case-mapping
  decision (see phase-2-onramp "Deferred seams" #1–#2).
- **Wardrobe writes ZERO SQL.** Its public CRUD is pure vault round-trip; the
  `wardrobe_items` table is migration-only and slated for removal. The v5
  `wardrobe_tier2_equivalence` differentials only the *base SQL marshaling* (see
  phase-2-onramp seam #7) — the real wardrobe contract is a vault document op and is
  ported here, not as a SQL repo.
- **Read-time timestamp synthesis**: the character read overlay mints
  `new Date().toISOString()` for a synthesized `physicalDescription` record on
  *every read* with no prior record — a determinism trap on the *read* path, not
  just writes.

## The build order (dependency-first)

The overlay sits on top of a storage primitive that is **not yet ported**. Build
bottom-up:

1. **`doc_mount_file_links` + `linkDocumentContent`** (the byte-landing transaction)
   and the `writeDatabaseDocument` derivation — **DONE** (2026-06-29,
   `quilltap-core::db::doc_mount_file_links`, green via
   `doc_mount_file_links_tier2_equivalence`). Ports `writeDatabaseDocument` +
   `linkDocumentContent` + `ensureLinkFolderId` + the pure leaves
   (`sha256OfString`, `detectDatabaseFileType`, `normaliseRelativePath`, the
   per-document policy). The corpus covers dedup-by-sha (two paths, identical
   content → shared file/document rows), link upsert-in-place (rewrite a path → no
   duplicate), folder auto-creation, the policy cascade, and the UTF-16
   `plainTextLength` vs UTF-8 `fileSizeBytes` split (ASCII-only so far). The
   first **multi-table-dump differential**: all four resulting tables are diffed in
   the minted-values remap form with a **shared cross-table id-map**. NB the oracle
   could not use `writeDatabaseDocument` directly — its post-write
   `reindexSingleFile` would mutate the link rows and its only skip-switch
   (`QUILLTAP_JOB_CHILD=1`) reroutes `getRepositories()` through the forked-child
   write proxy — so it drives v4's real `linkDocumentContent` with the trivial
   `writeDatabaseDocument` input derivation replicated. `readDatabaseDocument` and
   `linkBlobContent` (binary, step 8) remain deferred.
2. **The generic overlay engine** (`createDocumentStoreOverlay` +
   `AbstractStoreBackedRepository`) — **DONE** (2026-06-29,
   `quilltap-core::db::document_store_overlay`). A Rust generic over a
   `StoreEntity` trait (typed `Properties` bag, `entity_label`, `property_keys`,
   `parse_properties`); the four overlay paths + the failure-asymmetric
   read/write logic are shared. `load_store_files` (the batched join read),
   `apply_overlay[_one]` (drop vs throw), `read_properties`, `write_managed_fields`,
   `apply_write_overlay` (route + strip + properties RMW). The `runOnChain`
   per-mount write serialization is dropped (a Node-concurrency workaround; the
   single-writer Rust model is inherently serialized).
3. **`groups` — the pilot.** **DONE** (2026-06-29, `quilltap-core::db::groups` +
   `ensure_official_store`). Smallest store-backed surface (a 2-key bag, no
   roster, no subclass methods); exercises the *entire* engine (four files, both
   JSON + both markdown, the keystone throw-vs-drop, the 5-step create) with the
   least incidental marshaling. Green via `groups_tier2_equivalence` — drives v4's
   REAL `repos.groups.create`/`.update` end-to-end and diffs seven tables across
   the main + mount-index dbs in the shared-id-map remap form;
   `reindexSingleFile`'s `chunkCount`/`doc_mount_chunks` artifact is pinned/excluded
   (no `QUILLTAP_JOB_CHILD` needed — database-backed reindex uses no model). The
   `ensureOfficialStore` adopt branch (step 2 of its resolution order) is the only
   piece deferred — the corpus always provisions fresh; it lands with the
   startup-backfill slice.
4. **`projects`** — **DONE** (2026-06-29, `quilltap-core::db::projects` +
   `store_backed`). Same engine + the larger 16-key bag + the roster ops +
   in-memory `findByCharacterId`. This step generalized the slim-row plumbing +
   provisioning into `StoreBackedRepository<E: StoreEntity>` (v4's
   `AbstractStoreBackedRepository`): `StoreEntity` gained `slim_table` /
   `store_name_prefix` / `find_store_links` / `link_store`, and
   `ensure_official_store` became generic over `E`. `groups` was refactored onto
   the generic base (still green); `projects` is the second instance. Green via
   `projects_tier2_equivalence` — banks the 16-key `properties.json` bag
   byte-exact (five materialized Zod defaults in schema order + eleven
   skip-if-absent optionals) and the roster ops (`characterRoster` array RMW,
   `allowAnyCharacter` bool RMW). The `ensure-project-store` adopt branch rides
   the same step-2 deferral as groups (the corpus always provisions fresh).
5. **`stableUuidFromString`** (tier-1) — **DONE** (2026-06-29,
   `quilltap-core::vault_overlay::stable_uuid_from_string`, green via
   `stable_uuid_equivalence`). SHA-256 over the source's UTF-8 bytes → first 16
   bytes → v8 version nibble + RFC-4122 variant → hyphenated hex. Exact match to
   v4 incl. a non-ASCII source (no case mapping in this leaf). The first vault
   (Family B) leaf, ported ahead of the stateful overlay.
6. **`characters`** vault overlay — the nine-target projection over JSON +
   hand-built strings (defer the YAML files if needed by pinning the corpus).
7. **`wardrobe`** vault CRUD — the YAML round-trip + folder reprojection + cycle
   detection + per-mount serialization; depends on the YAML-emitter fidelity
   decision.
8. **`doc_mount_blobs`** — **DONE** (2026-06-29, `quilltap-core::db::doc_mount_blobs`,
   green via `doc_mount_blobs_tier2_equivalence`). The binary byte-store: ports
   the hand-written repo's `upsertByFileId` (sha recomputed from the bytes,
   overwrite-in-place by `fileId`) + the metadata/read/delete accessors, with the
   hand-written DDL (the `data BLOB` column the Zod schema omits + the FK)
   reproduced verbatim. Tier-2 BLOB-as-hex, fixture seeds the parent
   `doc_mount_files` rows the FK needs; banks insert / overwrite / sha-recompute /
   a non-UTF-8 binary round-trip. `linkBlobContent` (the binary `linkDocumentContent`
   analogue — the `(mountPointId, relativePath)` split) is the remaining deferral.

Steps 1–4 are the "design slice" payoff: they unblock `projects` and `groups`
entirely and lay the storage primitive every vault op needs. Steps 5–8 are the
heavier vault family, gated on the ICU/YAML decisions.

## The tier-2 oracle strategy

**v4 has no store-round-trip test to copy** — its overlay tests `jest.mock` the
storage boundary (`conversation-summary-vault-bridge.test.ts:12-18`) and only check
which path/content string was passed. So the differential is new, but the seam is
clean and reuses the mount-index machinery already built:

- **Drive v4's REAL storage + overlay code** (`database-store.ts`,
  `document-store-overlay.ts`, the storage repos) against fixture DBs — point
  `getRawMountIndexDatabase()` / `getRepositories()` at the fixtures, exactly as the
  existing mount-index tier-2 cases do. **Do not** use v4's `jest.mock` of
  `database-store` — that defeats the purpose.
- **Fixtures:** a mount-index DB (seeded with one `doc_mount_points` row — the
  store) + a main DB (the slim entity row, when driving through `repos.groups`).
  Both are categories the harness already materializes. **No temp filesystem dir.**
- **`QUILLTAP_JOB_CHILD=1`** in *both* the oracle and the Rust run — suppresses
  `reindexSingleFile` (chunk/embed, model-dependent) and `emitDocumentWritten`,
  isolating a clean content-only diff. (Chunk coverage, if ever needed, is a
  separate tier-3 mocked-LLM case.)
- **Dump four tables, not one:** `doc_mount_documents` (content + shas),
  `doc_mount_files` (sha/fileType/source identity), `doc_mount_file_links`
  (path/folderId/policy/conversionStatus), `doc_mount_folders` (hierarchy) — plus
  the slim entity row from the main DB. A single write touches all four; diffing
  only the document row misses link/folder correctness.
- **Remap form, not pinned.** `linkDocumentContent` mints every id
  (`randomUUID()` for file/document/link/folder rows) and every timestamp
  (`createdAt`/`updatedAt`/`lastModified`) internally — nothing can be injected.
  Reuse the established minted-values remap (first-seen id tokens in natural-key
  order — sha or path; placeholder timestamps). Drop the folders-remap
  `createdAt == updatedAt` invariant on *update* ops (an update touches only
  `updatedAt`/`lastModified`), as the upsert cases already do.

### Corpus scenarios that must be covered

- **Create** a group → slim row in main DB + four content rows + four link rows +
  the folder rows, all consistent; `properties.json` byte-exact (ordered struct,
  2-space pretty-print, defaults materialized).
- **Update** touching only store fields → DB `_update` skipped, store files
  rewritten, link rows upserted in place (not duplicated).
- **Update** touching a DB-only field (`name`) → slim row updated, store untouched.
- **dedup-by-sha** — two paths, identical content → one file + one document row,
  two links.
- **keystone error** — a row with a null `officialMountPointId` or missing
  `properties.json`: `findById` throws, `findAll` drops (both observable).
- **case-insensitive path** — write `Manifesto.md`, read `manifesto.md`, must hit
  (intersects the `toLowerCase` deferral; ASCII corpus masks it).
- **`markdownToNullable`** — a `null` description writes `''` but hydrates back to
  `null`; pin both the stored empty string and the hydrated null.

## Determinism + deferred seams this slice forces

- **`properties.json` / `state.json` key order** — `JSON.stringify(parse(x), null,
  2)`. `properties.json` follows Zod `.shape` declaration order with defaults
  materialized → a **typed serde struct in schema field order, 2-space
  pretty-print** (NOT `serde_json::Value`, which sorts). This extends the existing
  JSON-column-key-order rule to *pretty-printed multi-key* objects. `state.json` is
  arbitrary user JSON in insertion order → lands the **open-JSON multi-key seam**
  (phase-2-onramp #5) directly in the write path; constrain the corpus to
  `{}`/single-key until an insertion-order JSON value is in place.
- **Byte-exact serialization is load-bearing beyond the dump** — it feeds the
  `sha256` content-dedup, so a key-order or whitespace mismatch changes *file-row
  identity*, not just visible bytes. Compare stored content (and ideally its sha)
  hex-exact.
- **UTF-16 `plainTextLength` vs UTF-8 `fileSizeBytes`** — reproduce the JS `.length`
  (code-unit) semantics for `plainTextLength`; a naive `str::len()` diverges on
  non-ASCII.
- **YAML emitter fidelity** (Family B only) — budget as a dedicated sub-problem or
  pin the corpus to YAML-safe values.
- **ICU collation + Unicode case mapping** (Family B) — the vault folder sorts and
  slug case-folding are the densest concentration of the long-deferred
  `localeCompare`/`toLowerCase` sites; this slice is the natural place to finally
  make that decision (phase-2-onramp #1–#2).
- **Read-time timestamp synthesis** (character `physicalDescription`) — placeholder
  on the read path, not just write.

## Definition of done for the design slice

This document. The implementable conclusion: **port `doc_mount_file_links` +
`linkDocumentContent` + `writeDatabaseDocument` first (the storage primitive), then
the generic overlay engine, then `groups` as the pilot, then `projects`** — all on
the existing mount-index tier-2 harness with `QUILLTAP_JOB_CHILD=1` and the
minted-values remap, dumping the four storage tables + the slim row. The character
and wardrobe vault families are deferred to their own later slices, gated on the
`stableUuidFromString` leaf, the YAML-emitter decision, and the ICU/case-mapping
decision. No filesystem fixture is needed — the store is DB rows.
