# The porting status log (unit-by-unit)

> The append-only, unit-by-unit journal of the quilltap-v5 port. This is the
> full institutional memory: every ported unit, its differential, its banked
> findings, its tracked deferrals, and every round's unification record.
> **Append new units HERE** — CLAUDE.md carries only a phase-level summary
> (updated at phase/round boundaries) so it stays small enough to load every
> turn. Moved out of CLAUDE.md on 2026-07-10; the content below is verbatim
> from that file and keeps its original in-place update conventions
> ("update as it moves").

**Phase 0 (scaffolding + differential harness): done.** Toolchain pinned;
monorepo skeleton; `.dbkey` decryption ported & verified; cipher resolved
(SQLite3MC/ChaCha20) and confirmed on real Friday data (37 tables, 33 chars,
20 320 memories); differential harness proven.

**Phase 1 (pure-function ports): in progress.** Each unit ships with a tier-1
exact-equivalence test against the v4 oracle. Ported so far: memory
weighting/ranking, recall tags + history, write-partition + folder remap,
context-compression sizing, enclave budget math (incl. the autonomous-room
per-turn context cap `computeAutonomousContextCap` + its `DEFAULT_AUTONOMOUS_TARGET_TURNS`/
`MIN_AUTONOMOUS_CONTEXT_TOKENS` constants — v4's token-budget pacing, ported
2026-07-01 when it landed upstream), LLM pricing + model selection +
model classes, context-budget arithmetic, token estimation, the full turn manager (turn-state
machine, all-LLM auto-pause, participant-list filters, predicted turn order, and
weighted next-speaker selection with the RNG injected), the context-summary
cadence (fold/hard gate, interchange count, title-check crossing, turn
partition), the per-character context shaping (history-access gate, presence
windows, whisper visibility, role/name attribution), the pure memory
name-resolution leaves (reinforced-importance, name+pronoun formatting,
about/holder name-set builders, and the word-boundary name matchers —
`nameAppears`/`countNameOccurrences`/`resolveAboutCharacterId`, the Unicode
boundary + lookahead reproduced without a backtracking engine via the `regex`
crate plus a hand-rolled boundary check), the mentioned-character corpus scan
(`findMentionedCharacterIds` — ASCII `\b` alternation, longest-token-first), the
deterministic novel-detail extraction (`extractNovelDetails` — proper-noun /
date / currency / number-unit / CamelCase / acronym scan with ASCII `\d`/`\b`
and the JS `\s` set reproduced exactly), the chat-task artifact strippers
(`stripToolArtifacts` / `extractVisibleConversation` / `getCharacterChatPreview`,
over shared JS string primitives in `jsstr`), the embedding vector-math hot
paths (L2
normalisation, profile storage policy, cosine similarity + dimension-mismatch
guard, fallback keyword/phrase scorer, literal-phrase boost, Float32↔LE-byte
BLOB conversion, and the legacy JSON-text embedding recovery
[`parseLegacyEmbeddingText`, reproducing JS `Object.values` ascending
integer-key ordering]), the canon/scenario text helpers (self/other canon-block
rendering, the New-Chat scenario-text combiner), and a batch of small leaf
utilities (chat predicates, semver,
pronoun→gender, tag-style, char-count), and the JS number formatters (the
`Number.prototype.toFixed` kernel — V8 half-away-from-zero rounding on the exact
f64 value via IEEE-754 mantissa/exponent + u128 — and `formatBytes` /
`formatCostForDisplay` / `formatTokenCount` built on it), and the
`canonicalize*` tool serializers (deep code-unit key-sort of
`function.parameters` + the tool-name array sort, the latter a documented
`localeCompare` seam). **The collation/ordering wave is done:**
`parseLegacyEmbeddingText`, the `toFixed` formatters, `canonicalize*`, and
`compareVersions`' `localeCompare` fallback (documented as a residual seam — the
numeric path is exact). **The registry-seam wave is done:** the cheap-model
classifiers (`isCheapModel` / `estimateModelCost` / `getCheapestModel`, registry
recommended-list / default injected, string heuristics pure) and
`getModelContextLimit` (+ `hasExtendedContext` / `getSafeInputLimit`) — its
override/default tables ported as constants, the plugin model-info /
`FALLBACK_PRICING` rows / registry default injected. The single ICU-collation
decision is now **RESOLVED (2026-06-30): added ICU4X** (`icu` 2.2, compiled data)
as `crate::collation::locale_compare` — `Collator::try_new` → a
`CollatorBorrowed<'static>` configured to **en-US / tertiary**, matching Node's
no-arg `Intl.Collator` (verified the order `a,A,ä,b,B,e,é,z,Z` against ICU 78).
The two ported `localeCompare` sites use it (`compareVersions` fallback,
`canonicalize` tool-name sort), each with a mixed-case/accent differential row
proving the ICU path; the vault's code-unit sorts stay code-unit (faithful to v4's
vault code, which sorts by code unit there, not `localeCompare`). The companion
`toLowerCase` case-mapping seam (`tags.nameLower`, `text_replacement_rules`) is
also **RESOLVED**: `str::to_lowercase` is byte-identical to JS `toLowerCase`
(verified on İ/final-sigma/ß/digraphs), so no ICU case-mapping crate is needed —
non-ASCII corpus rows prove it. The whole Unicode-fidelity cluster is now closed.

**Phase-2 on-ramp (tier-2 DB-state oracle): the pilot round-trips green.** The
`folders` repo now round-trips green through the tier-2 harness: both v4 and the
Rust port run the same create + update on the same seed fixture (synthetic,
test-pepper-keyed) and the canonical `folders` dumps match byte-for-byte (ids +
timestamps pinned both sides → zero normalization). This established the
machinery: `quilltap-core`'s `db` module (the writable ChaCha20 open + the
single-writer `Writer` + `FoldersRepository` create/update + canonical dump),
the amalgamation build relocated into core (probes retired), the TS oracle
(`harness/oracle/{fixtures,cases}/folders-tier2*`), and the harness diff test.
Remaining on-ramp breadth: ~~the generated-UUID remap / timestamp-placeholder
normalization~~ (**done** — see "the remap machinery" below), ~~the `WriteBatch`
partitioned-apply path~~ (**done** — see "the partitioned write applier" below),
and the real-snapshot fixture sanitizer. From here Phase 2 is the same mechanical
loop, repo by repo.

**Phase 2 proper: in progress.** Repos #5–#7 — `conversation_annotations`,
`provider_models`, and `help_docs` — were ported **in parallel** (three agents,
each on its own new files; the shared `db/mod.rs` wiring + version/doc edits
serialized afterward), and each round-trips green in the pinned
zero-normalization form (`conversation_annotations_tier2_equivalence`,
`provider_models_tier2_equivalence`, `help_docs_tier2_equivalence`). They bank
three still-unverified marshaling shapes: `conversation_annotations` a
**REAL-affinity unbounded-int column** (`messageIndex` is
`z.number().int().min(0)` with no `.max()` → REAL by v4's `mapToSQLiteType`,
bound `f64`, the integer-valued cell collapsed back by `js_number_to_json`) plus
a nullable UUID column; `provider_models` two **nullable REAL number columns**
(`contextWindow` / `maxOutputTokens`, bare `z.number()` → REAL), two
boolean-default columns, and enum TEXT columns; and `help_docs` the **first
tier-2 BLOB column** (`embedding`, Float32 little-endian bytes via
`embedding_blob::float32_to_blob`, empty/null → NULL, dumped as hex for bit-exact
compare — and proving a text-only `update` leaves the BLOB untouched). The
distinctive `upsert*` methods on these three are deferred (their internal
`now`/`generateId()` needs the remap-normalization form, not the pinned form).

A **second parallel batch** (repos #8–#10) — `roleplay_templates`,
`image_profiles`, and `connection_profiles` — was ported the same way and each
round-trips green (`roleplay_templates_tier2_equivalence`,
`image_profiles_tier2_equivalence`, `connection_profiles_tier2_equivalence`).
`roleplay_templates` banks the **first array-of-objects JSON column**
(`renderingPatterns`, each element a typed serde struct in schema field order
with `skip_serializing_if` optionals — the `tags.visualStyle` typed-struct rule
extended over an array) plus a nullable JSON-object column (`dialogueDetection`);
`delimiters` is held empty and `narrationDelimiters` kept to its plain-string
form (no built-in guard ported — the corpus never mutates a built-in row).
`image_profiles` banks the **Taggable lineage** (`userId` + JSON `tags` array)
and the first **open/arbitrary-JSON object column** (`parameters`, `z.record` →
`serde_json::Value`). `connection_profiles` is the **widest surface to date**
(~29 columns: three enum TEXT, eight booleans, two nullable REAL int-overrides,
five REAL token counters, three nullable strings, the `tags` array, the open
`parameters` object). New **tracked deferred seam**: open-JSON object columns
with **two or more keys** diverge (`serde_json::Value` sorts keys vs v4's
insertion-order `JSON.stringify`) — the corpora constrain `parameters` to `{}` /
single-key; close before multi-key open-JSON data (see "Deferred seams" in
`docs/developer/porting/phase-2-onramp.md`).

A **third parallel batch** (repos #11–#15, five at a time) — `plugin_config`,
`embedding_profiles`, `terminal_sessions`, `character_plugin_data`, and
`tfidf_vocabulary` — was ported the same way and each round-trips green
(`plugin_config_tier2_equivalence`, `embedding_profiles_tier2_equivalence`,
`terminal_sessions_tier2_equivalence`, `character_plugin_data_tier2_equivalence`,
`tfidf_vocabulary_tier2_equivalence`). `plugin_config` banks the **UserOwned
lineage** (a `userId` scope column) plus an open-JSON `config` object and an
**optional boolean** (`enabled` — no default, so INTEGER 0/1 when present, SQL
NULL when the key is absent, confirmed empirically). `embedding_profiles` (the
Taggable lineage again) banks an enum TEXT column and two **nullable REAL number
columns** (`dimensions` bare `z.number()`, `truncateToDimensions`
`.int().positive()` — min-only → REAL) plus two boolean-default columns.
`terminal_sessions` is a clean string-heavy repo (nullable strings + a nullable
timestamp + a nullable REAL `exitCode`); v4's `create` injects no
nondeterministic default, so the pinned form holds. `character_plugin_data` banks
the first **open-JSON _value_ column** (`data`, `z.unknown()` → compact JSON text
via `prepareForStorage`). `tfidf_vocabulary` is the **first repo that overrides
the base `create`/`update`**: v4 mints `updatedAt` unconditionally (ignoring any
passed value), so the port mints it via `clock::now_iso` and the harness
placeholder-normalizes only `updatedAt` (ids / `createdAt` / payload columns stay
pinned and diff exactly) — the minted-timestamp form narrowed to one column, no
id remap; it also banks the first **plain-string columns holding JSON text**
(`vocabulary` / `idf`, bound single-encoded). The `plugin_config` /
`character_plugin_data` open-JSON corpora are constrained to `{}` / single-key,
same tracked seam.

A **fourth parallel batch** (repos #16–#20, five at a time) — `users`,
`conversation_chunks`, `files`, `chat_documents`, and `embedding_status` — was
ported the same way and each round-trips green (`users_tier2_equivalence`,
`conversation_chunks_tier2_equivalence`, `files_tier2_equivalence`,
`chat_documents_tier2_equivalence`, `embedding_status_tier2_equivalence`). All
five are **main-DB** repos. `users` is the plainest surface yet (all strings + five
nullable TEXT columns). `conversation_chunks` banks the **second BLOB column**
(`embedding`, like `help_docs` — a text-only update leaves it untouched) plus a
min-only REAL int (`interchangeIndex`) and two JSON string-array columns.
`files` is the **widest repo to date** (~23 columns, Taggable): a bare-number
REAL (`size`), two nullable REAL columns, an optional boolean (`isPlainText` —
banks both present 0/1 and absent → NULL), two JSON arrays, three enum TEXT
columns, and many nullable strings. `chat_documents` banks an enum + a boolean +
nullable strings. `embedding_status` is the **second base-method-override repo**
(after `tfidf_vocabulary`): v4 mints `updatedAt` unconditionally, so the port
mints it via `clock::now_iso` and the harness placeholder-normalizes only that
column (id / `createdAt` / payload pinned).

The **mount-index sibling-DB slice** then ported the first five repos that do NOT
live in the main DB (v4's `quilltap-mount-index.db`): `group_character_members`
(the serial pilot), then `project_doc_mount_links`, `group_doc_mount_links`,
`doc_mount_folders`, and `doc_mount_points` in parallel — each round-trips green
(`group_character_members_tier2_equivalence`,
`project_doc_mount_links_tier2_equivalence`,
`group_doc_mount_links_tier2_equivalence`,
`doc_mount_folders_tier2_equivalence`, `doc_mount_points_tier2_equivalence`). The
machinery extension was **TS-side only**: the Rust `Writer::open_writable` already
opens any ChaCha20 file by path, so the "mount-index" partition is just *which file
the writer was opened against* — no Rust change. The fixture builder + oracle point
`SQLITE_MOUNT_INDEX_PATH` at the fixture (with a throwaway main DB at `SQLITE_PATH`),
seed/run through v4's real mount-index repos (whose `getCollection` override creates
the table there on first access), flush via `closeMountIndexSQLiteClient`, and read
back through `getRawMountIndexDatabase()` **directly** (not `rawQuery`, which targets
the main backend). `generateCreateTable` emits no FK constraints, so the cross-DB
refs are plain TEXT needing no seeded parents. The three join tables
(`group_character_members` / `project_doc_mount_links` / `group_doc_mount_links`) are
the plainest shape (`id` + two UUID-as-TEXT refs + timestamps); `doc_mount_folders`
banks a **nullable-UUID** column (`parentId`, null = root); `doc_mount_points` is the
**widest of the family** (18 columns — four enum TEXT, a boolean, two JSON
string-arrays banking empty + non-empty, three nullable strings/timestamp, three
**REAL-affinity int counters** integer-collapsed in the dump), and its runtime
ALTER-TABLE migrations are no-ops on a fresh schema-generated table.

The **llm-logs sibling DB** then followed on the same TS-only machinery
(`llm_logs` → `SQLITE_LLM_LOGS_PATH` / `getRawLLMLogsDatabase()`;
`llm_logs_tier2_equivalence`, pinned form). It is the **widest repo in Phase 2**
(18 columns): an 18-variant enum, four nullable UUIDs, a nullable REAL
(`durationMs`), an open-JSON `rawProviderUsage` (constrained null/`{}`/single-key),
and **five nested typed-struct JSON columns** (`request`, `response`, `usage`,
`cacheUsage`, `requestHashes`) reproduced byte-for-byte with serde structs in
schema field order — integer-valued nested numbers as `i64` (so they render `3`,
not `3.0`, matching `JSON.stringify`), the lone fractional `temperature` an `f64`,
optional nested fields `skip_serializing_if` (omitted, not null). One difference
from mount-index: the backend disconnect *does* close the llm-logs client, so the
oracle reads the raw handle before `closeDatabase()`. **Both sibling partitions are
now covered; no sibling DB remains unported.** See "Deferred seams" item 6 in
`docs/developer/porting/phase-2-onramp.md`.

Separately, the deferred **`upsert*` methods** on six already-ported repos are now
ported, each with a tier-2 case in the **minted-values remap form** (the upsert
mints `id`/`createdAt`/`updatedAt` on create and `updatedAt` on update, so the test
pins nothing for the upsert ops — it remaps `id` to first-seen tokens in
natural-key order and placeholders both timestamps; the folders-remap
`createdAt == updatedAt` invariant is dropped since an upsert-update legitimately
differs): `conversation_annotations.upsert` (find by chatId+messageIndex+
characterName; update subset {content, sourceMessageId} — added an
`Option<Option<_>>` nullable setter for `sourceMessageId`),
`help_docs.upsertByPath` (leaves the `embedding` BLOB untouched on update; create
stores NULL — proven by the test), `provider_models.upsertModel` (the find
replicates v4's `findByProviderAndModelId`: a falsy `baseUrl` is left
**unconstrained**, NOT matched as NULL), `plugin_config.upsertForUserPlugin`
(merges `{...existing, ...new}` config, kept `{}`/single-key),
`character_plugin_data.upsert` (open-JSON `data`, `{}`/single-key), and
`tfidf_vocabulary.upsertByProfileId` (rides the base-method-override minting).
Each adds a private find-by-key SELECT and mints via `clock::now_iso` + `uuid`.

A **fifth parallel batch** (five repos, `create`/`update`/`delete` each, pinned
ids + timestamps → zero normalization) spans the main DB and the mount-index
sibling DB, each round-tripping green (`chat_settings_tier2_equivalence`,
`wardrobe_tier2_equivalence`, `doc_mount_files_tier2_equivalence`,
`doc_mount_documents_tier2_equivalence`, `doc_mount_chunks_tier2_equivalence`).
`chat_settings` (main DB, plain `AbstractBaseRepository`) is the **widest
JSON-object surface in Phase 2** (~33 columns, ~15 nested typed-struct JSON columns
in schema field order with `i64` nested ints so they render bare) and banks the
**first INTEGER-affinity number column** (`sidebarWidth`, `.min().max()` both
integer → INTEGER, vs the prior min-only/bare REAL numbers); `cheapLLMSettings`
keeps its uppercase acronym; the `*ForUser` default-injecting helpers and the
multi-key open-JSON `tagStyles` key order are out of scope (`tagStyles` kept `{}`).
`wardrobe` (`wardrobe_items`, main DB) is the first repo whose **public CRUD is
vault-only** — v4's `WardrobeRepository` writes to the document store and throws
without a mount, with no SQL write mirror — so the differential drives v4's **real
base-repository SQL CRUD** (`_create`/`_update`/`_delete`) via a thin subclass
exposing the protected internals (the marshaling the schema-translator builds from
`WardrobeItemSchema` and the table reads consume); it banks the first repo with
**two JSON array columns** (`types` — the first enum-string array — and
`componentItemIds`) and a **nullable soft-delete timestamp** (`archivedAt`); the
vault-overlay public write path is now **ported/verified** (seam #7 closed — see
"the public wardrobe write path" below). The three mount-index siblings ride the same TS-only
machinery as `doc_mount_points`: `doc_mount_files` is the **narrowest tier-2 repo
to date** (all-required, no JSON/boolean/nullable; re-banks a `fileSizeBytes`
min-only REAL int + two enum TEXT); `doc_mount_documents` is the file-content store
keyed by a UNIQUE `fileId` (a `plainTextLength` min-only REAL int + plain TEXT
content/sha); and `doc_mount_chunks` is the **first mount-index sibling repo to
carry a BLOB column** (the `embedding` Float32 LE BLOB, empty/null → NULL, dumped
as hex, a text-only update proven to leave it untouched — like
`conversation_chunks`/`help_docs` — plus two REAL-affinity int counters and a
nullable `headingContext`; `updateEmbedding` out of scope).

The **document-store storage primitive** (`doc_mount_file_links`) — build step 1 of
the document-store overlay slice (`docs/developer/porting/document-store-overlay.md`)
— is ported and green (`doc_mount_file_links_tier2_equivalence`). It ports v4's
`writeDatabaseDocument` + `linkDocumentContent` + `ensureLinkFolderId`, the
byte-landing path every store-backed entity (project/group store, character vault)
calls: a `(mountPointId, relativePath, content)` write is content-addressed by
SHA-256 and split in one transaction across `doc_mount_files` (find-or-create by
sha → dedup), `doc_mount_documents` (the bytes, upsert by `fileId`), and
`doc_mount_file_links` (the location, upsert by `(mountPointId, relativePath)` —
rewrite-in-place), with `doc_mount_folders` rows auto-created for parent segments.
The Rust INSERTs list **exactly v4's column subset** so SQLite fills the same DDL
defaults on the unset columns. It also ports the pure leaves it needs
(`sha256OfString`, `detectDatabaseFileType`, `normaliseRelativePath`, and the
per-document policy `coercePolicyBool`/`policyFromFrontmatterData`/
`policyFromContent`). This is the **first multi-table-dump differential**: the
tier-2 case drives v4's real `linkDocumentContent` and diffs all four resulting
tables in the minted-values remap form, extended with a **shared cross-table id-map**
(so `document.fileId`/`link.fileId`/`link.folderId`/`folder.parentId` FKs verify by
relationship; `mountPointId` is the pinned seeded store id). The corpus banks a
fresh JSON + markdown write, subfolder creation, dedup-by-sha, link
upsert-in-place, and the markdown frontmatter policy cascade
(`character_read: false` → all `allow*` = 0). The oracle drives `linkDocumentContent`
directly (not `writeDatabaseDocument`) to avoid the post-write `reindexSingleFile`
chunk/embed pass — its only skip-switch `QUILLTAP_JOB_CHILD=1` reroutes repos
through the forked-child write proxy. Deferred: arbitrary-YAML frontmatter (scalar
subset only, lands with the character-vault YAML decision), `linkBlobContent`, and
the read/GC/conversion helpers.

The **document-store overlay engine + the `groups` store-backed pilot** (build
steps 2-3 of the slice) are ported and green (`groups_tier2_equivalence`).
`quilltap-core::db::document_store_overlay` ports v4's generic
`createDocumentStoreOverlay` + `AbstractStoreBackedRepository` as a Rust generic
over a `StoreEntity` trait (typed `Properties` bag, `entity_label`,
`property_keys`, `parse_properties`); the four overlay paths
(`properties.json`/`description.md`/`instructions.md`/`state.json`) + the
failure-asymmetric read/write logic are shared (`load_store_files` batched join,
`apply_overlay[_one]` **drop-vs-throw**, `read_properties`, `write_managed_fields`,
`apply_write_overlay` route+strip+**properties RMW**). `quilltap-core::db::groups`
binds it for `groups`: the slim row (id/name/officialMountPointId/timestamps)
lives in the **main** db, the store in the **mount-index** db, so
`GroupsRepository` spans both connections (new `Writer::connection()` seam), and
`ensure_official_store` ports `ensureOfficialStore`'s find/create provisioning
(mint a `Group Files: <name>` mount point + link + raw FK) + the pure
`nextUniqueMountPointName` (tier-1 unit test). `create` runs v4's 5-step sequence
(slim row → provision → write four files → overlay re-read). The differential
drives v4's REAL `repos.groups.create`/`.update` end-to-end — **no mocked storage
boundary, no `QUILLTAP_JOB_CHILD`** (database-backed `reindexSingleFile` chunks
with no model, deterministically; its only divergence, the link `chunkCount` +
the derived `doc_mount_chunks` rows, is pinned/excluded) — and diffs **seven
tables across both dbs** (the slim `groups` row + `doc_mount_points` / `_files` /
`_documents` / `_file_links` / `_folders` + `group_doc_mount_links`) in the
minted-values remap form with **one shared cross-db id-map** (so
`groups.officialMountPointId` → the store, `link.fileId` → `file.id`, etc. verify
by relationship). Banks the 5-step create, `properties.json` byte-exact (both
keys + the empty bag), a store-only update (slim `updatedAt` NOT bumped) with a
properties RMW that preserves the untouched `icon`, a DB-only `name` update,
dedup-by-sha (`"{}"` shared by three links across two stores; `""` by two),
orphan-on-rewrite, and (second test) the keystone throw-vs-drop asymmetry.
**Tracked deferrals:** the `ensureOfficialStore` **adopt branch** (startup-heal of
a hand-linked store — corpus always provisions fresh), the property/`state`
**null-vs-absent + multi-key insertion order** (open-JSON seam — corpus kept
`{}`/single-key).

**`projects` (build step 4) + the store-backed generalization** are ported and
green (`projects_tier2_equivalence`). The slim-row plumbing + provisioning that
`groups` proved is now the generic `quilltap-core::db::store_backed`
(`StoreBackedRepository<E: StoreEntity>` = v4's `AbstractStoreBackedRepository`):
the `StoreEntity` trait gained `slim_table` / `store_name_prefix` /
`find_store_links` / `link_store`, and `ensure_official_store` became generic over
`E`. `GroupsRepository` was refactored to a thin wrapper over the generic base
(re-verified green); `quilltap-core::db::projects` is the second instance.
`ProjectsRepository` adds the **16-key `properties.json` bag**
(`ProjectPropertiesSchema` — five Zod-`.default` keys ALWAYS materialized in schema
order, eleven `.nullable().optional()` → `skip_serializing_if`) and the
**character-roster ops** (`addToRoster` / `removeFromRoster` /
`setAllowAnyCharacter` / `canCharacterParticipate` / `findByCharacterId`), each a
`properties.json` RMW through `update` (or an in-memory `findAll` filter). The
differential drives v4's REAL `repos.projects.create`/`.update`/roster ops and
diffs the same seven tables (slim `projects` row + the store tables +
`project_doc_mount_links`) in the shared-cross-db-id-map remap form
(`chunkCount` pinned, `doc_mount_chunks` excluded). Banks a rich create (roster +
color + `defaultImageProfileId` + `backgroundDisplayMode`, the optional keys
interleaved with the materialized defaults in schema order — byte-exact), a
minimal create (the five defaults only), the `characterRoster` array RMW
(add/remove preserving the other fifteen keys), the `allowAnyCharacter` bool RMW,
and a DB-only `name` update.

**`stableUuidFromString` (build step 5)** is ported and green
(`stable_uuid_equivalence`) — the first character/wardrobe **vault** (Family B)
leaf, in the new `quilltap-core::vault_overlay` module. It derives the
deterministic id every folder-enumerated vault entity carries
(`stableUuidFromString('<kind>:<mountPointId>:<relativePath>')`, backing
prompt/scenario/wardrobe ids chat references depend on): SHA-256 over UTF-8 bytes
→ first 16 bytes → v8 version nibble + RFC-4122 variant → hyphenated hex. Tier-1
exact, incl. a non-ASCII source (no case mapping in this leaf).

**`doc_mount_blobs` (build step 8)** is ported and green
(`doc_mount_blobs_tier2_equivalence`) — the document store's **binary** byte-store
(`quilltap-core::db::doc_mount_blobs`), sibling of the text store
`doc_mount_documents`. v4 hand-writes this repo + its DDL (the `data BLOB` column
is deliberately omitted from `DocMountBlobMetadataSchema`), so the port reproduces
the `CREATE TABLE` verbatim (incl. the `FOREIGN KEY (fileId) REFERENCES
doc_mount_files(id)`) and ports `upsertByFileId` (sha **recomputed from the
bytes**, `sizeBytes = data.len()`, overwrite-in-place by `fileId`) + the
metadata/read/delete accessors. Tier-2 dumps the `data` BLOB as hex (mirrors
`help_docs`/`doc_mount_chunks`); the fixture seeds the parent `doc_mount_files`
rows the FK needs (the writable open enforces `foreign_keys = ON`). Banks insert /
overwrite-in-place / the sha-recompute rule (all-zero advisory shas) / a non-UTF-8
binary round-trip. `linkBlobContent` (the binary analogue of `linkDocumentContent`)
remains deferred.

With Family A (the generic store-backed engine: storage primitive, overlay,
`groups`, `projects`) complete, the first vault leaf (`stableUuidFromString`)
done, and the binary store (`doc_mount_blobs`) done, the remaining document-store
work is the heaviest piece: the character/wardrobe **vault overlay** (steps 6–7 —
the nine-target projection + the wardrobe YAML round-trip), gated on the
long-deferred ICU-collation / Unicode-case-mapping and YAML-emitter-fidelity
decisions.

That vault overlay is being ported **leaf-first** (the discipline of pure-to-
stateful), so the decision-free pure helpers land before the stateful read/write
overlay that forces the YAML/ICU calls. Done so far: `stableUuidFromString`
(above) and the **wardrobe-component leaves** (`quilltap-core::vault_overlay`,
green via `vault_component_leaves_equivalence`) — `parseComponentItemsField`
(coerce `componentItems:` → clean `Vec<String>`), `parseWardrobeTypesField`
(all-or-nothing enum validation + first-seen dedup, `None` on empty/invalid), and
`detectComponentCycles` (the save-time component-graph cycle check). These touch
no YAML and no case-mapping/collation. Also done: the **vault write-projection string leaves**
(`vault_string_leaves_equivalence`) — `slugifyWardrobeTitle`,
`buildSlugByItemIdMap`, `sanitizeFileName`, `buildSystemPromptFile` (+ the private
`escapeYaml` = `JSON.stringify` quote path, via `serde_json::to_string`), and
`buildScenarioFile`. **The two vault decisions are now LOCKED** (2026-06-29; see
`[[vault-yaml-icu-decisions]]` + the design doc): **(A) hand-roll the wardrobe
YAML emitter** (the eemeli/yaml dependency is isolated to `Wardrobe/*.md`, build
step 7 — prompts use `escapeYaml`, scenarios are frontmatter-less, the JSON files
use `JSON.stringify`), and **(B) code-unit seam + pinned corpus for
`localeCompare`** (no ICU crate for the vault; the slug `toLowerCase` is a
non-issue). Also done: the **JSON projection parsers** (`vault_json_parsers_equivalence`) —
`parseVaultProperties` + `parseVaultPhysicalPrompts`, reproducing Zod
`safeParse` → fall-back-to-null (unknown-key stripping, required-nullable
presence, the `talkativeness` range, the 1–20-UTF-16 `pronouns` fields). Also
done: the **legacy `wardrobe.json` migration parser**
(`parse_legacy_wardrobe_json`, `vault_legacy_wardrobe_equivalence`, 39 cases) —
the first vault leaf to validate an **array of full `WardrobeItemSchema` items**,
so it reproduces **Zod 4's `z.uuid()` and `z.iso.datetime()` string formats
verbatim** (regex sources lifted from the live schema: `[1-8]`/`[89abAB]` UUIDs +
the all-zero/all-`f` sentinels; leap-year-aware ISO dates with a `Z`-only zone;
JS `\d` → ASCII `[0-9]`; the `regex` `$` confirmed to match JS's absolute-end
anchor incl. trailing-newline rejection). Faithful to Zod — any bad item nulls
the whole array, `.default()` keys materialized, output in schema order, unknown
keys stripped (root `presets`, per-item, in-`outfit`), and a present `outfit`
validated-then-discarded (only `{ items }` returned). The two regexes are the
first `LazyLock<Regex>` statics in the vault module. **The read-side YAML
decision is now resolved and built: a hand-rolled constrained reader, no YAML
crate in the vault** (the read-side companion to locked Decision A). The
**Markdown frontmatter parser** (`quilltap-core::markdown::parse_frontmatter`,
`markdown_frontmatter_equivalence`, 52 cases) is the shared read-path foundation:
it reproduces v4 `parseFrontmatter`'s structural logic (the `---\n`-only opener,
exactly-`---` close, UTF-16 `bodyStartOffset` computed even on a non-object body,
empty/comments-only → `{}`, array/scalar → null, dup-key → null) and a
hand-rolled **YAML 1.2 core-schema** subset reader (scalar resolution with
`yes`/`no` as strings, double/single quotes + JSON-style escapes, the
whitespace-gated `#` comment rule, flow `[a,b]` and block `- item` sequences).
Out-of-subset constructs (nested/flow maps, block scalars, anchors/tags, exotic
numbers) are a documented seam — kept out of the corpus, resolving conservatively
(null/string or parse error), never silently wrong. Also done: **all three
per-file frontmatter parsers** built on that reader — `parse_prompt_file` +
`parse_scenario_file` (`vault_frontmatter_parsers_equivalence`, 26 cases) and
`parse_wardrobe_item_file` (`vault_wardrobe_item_file_equivalence`, 20 cases) —
producing `CharacterSystemPrompt`/`CharacterScenario`/`WardrobeItemFromFile`
directly (not via Zod), so the JS `.trim()`/`.slice(0,n)` caps use the `jsstr`
UTF-16 primitives (name ≤100, title ≤200, description ≤500); `isDefault` is
`=== true`; the prompt body is the post-frontmatter content `trimStart`ed; title
resolution is frontmatter `name`/`title` → first `# heading` → filename-without-
`.md` (a heading-as-title is dropped from the body, a frontmatter title is not).
The wardrobe parser adds the id sanity check (`/^[0-9a-f-]{36}$/i` else
`stableUuidFromString`), the required `types` (reusing
`parse_wardrobe_types_field`), the raw `componentItemIds` (reusing
`parse_component_items_field`, resolved later by the overlay), and the
archived/flags/timestamp-precedence logic. Added `jsstr::js_trim_start` +
`markdown::body_after` (UTF-16-offset→byte slice). **The vault is now fully
ported up to the stateful overlay.**

The **stateful read overlay is now in progress**, sub-unit 1 done: the
directory-listing load (`DocMountDocumentsRepository::find_many_by_mount_points_in_folder`,
`vault_folder_read_equivalence`) — v4 `findManyByMountPointsInFolder`'s 3-table
join + SQL `LIKE` prefilter + the JS non-recursive single-level + extension
post-filter, returning the overlay-consumed row subset. It established the first
**read-differential** harness shape: a builder seeds stores + a file corpus via
v4's real `linkDocumentContent` (driven directly — NOT `writeDatabaseDocument`,
whose `QUILLTAP_JOB_CHILD=1` breaks `initializeDatabase`; see
`[[document-store-oracle-gotchas]]`), then both v4 and the Rust port READ the same
fixture so minted ids/timestamps match and rows compare exactly. Sub-unit 2 — the
**`hydrateOne` heart** — is also done (`quilltap-core::db::vault_read_overlay`,
`vault_read_overlay_equivalence`): v4's `hydrateOne` + `applyDocumentStoreOverlay`
+ `…One`, operating on the character as a `serde_json::Value` (the overlay is a
JSON merge). Folds `properties.json` (pronouns/aliases/title/firstMessage/
talkativeness), the five markdown fields (via `markdownToNullable`, empty → null),
`physicalDescription` (base-reuse or a clock-minted base), `systemPrompts` (the
Decision-B code-unit sort + the exactly-one-`isDefault` normalization), and
`scenarios`. Banks the keystone drop-vs-throw asymmetry (batched DROP on a missing
`properties.json`, single Unavailable error) — verified end-to-end against v4's
real `applyDocumentStoreOverlay` over a 7-character / 6-store seeded fixture (only
the minted physical timestamps placeholdered). Sub-unit 3 — the **wardrobe read
overlay** — is also done (`read_character_vault_wardrobe` +
`resolve_and_check_component_items`, `vault_wardrobe_read_equivalence`): v4's
`readCharacterVaultWardrobe`. Enumerates `Wardrobe/*.md` (Decision-B code-unit
sort → `parseWardrobeItemFile`, dropping unparseable), builds the in-vault slug/id
maps (first-claimer wins a slug; every item is id-addressable), and resolves each
item's raw `componentItems:` refs to canonical ids (slug-first then UUID, unknown
dropped) then clears any item whose resolved components form a cycle. The cycle
pass reads the **live** (already-mutated) component lists, so a mid-pass clear
changes later items' walks, mirroring v4's mutable `itemById` (banked: a mutual
`a → b`/`b → a` cycle clears `a`, then `b` survives because `a` was already
emptied). Empty/missing folder falls through to legacy `wardrobe.json`
(`parseLegacyWardrobeJson`); neither → `null`. Read-differential (three cases)
drives v4's REAL `readCharacterVaultWardrobe` over a shared seeded fixture and
compares each `{ items } | null` exactly (no normalization — this path mints no
clock value); plus four tier-1 resolver unit tests. **Tracked deferral:** the
archetype-seeding branch (`findArchetypes` over the General/project `Wardrobe`
stores) is not ported — the corpus keeps no General store provisioned, so v4's
`findArchetypes` returns `[]` and the seed is a verified no-op (close before
reading vaults that reference shared archetypes). Sub-unit 4 — the **wardrobe YAML
emitter** (Decision A, the only eemeli/yaml site) — is also done
(`build_wardrobe_item_file`, `vault_wardrobe_emit_equivalence`): v4's
`buildWardrobeItemFile` over a hand-rolled, faithful port of eemeli/yaml 2.9.0's
`stringifyString` + `foldFlowLines` (default options) for the bounded wardrobe
value space (string scalars / boolean `true` / block sequences). Reproduces
plain/single/double quote selection, the core-schema reparse-safety quoting, line
folding past width 80, and `|`/`|-`/`>` block scalars — operating on UTF-16 code
units (fold offsets), with the control-char force-quote matched on code points
(eemeli's `/u` flag: a valid astral char is not a surrogate match) and
`JSON.stringify` escaping byte-exact. Tier-1 differential over a 100-item corpus
(every quoting edge, folding, block scalars, surrogate-pair fold offsets, the
slug/UUID `componentItems` map, all flag branches) against v4's real
`buildWardrobeItemFile`, plus three exact unit tests. **Both vault decisions are
now fully discharged.** Sub-unit 5 — the **wardrobe write projection** — is also
done (`db::vault_wardrobe_write`, `vault_wardrobe_write_equivalence`): v4's
`projectVaultWardrobe` / `projectArrayIntoVaultFolder`. Re-projects an
authoritative `WardrobeItem` list into a store's `Wardrobe/` folder — each item to
`Wardrobe/<title>.md` (filename collisions get `-1`/`-2`/… suffixes), files not
produced this pass are swept, the legacy `wardrobe.json` is deleted — composing the
ported leaves (`build_slug_by_item_id_map` / `build_wardrobe_item_file` /
`sanitize_file_name`) over the write primitive (`write_database_document`) and a
new GC delete (`delete_database_document` + `delete_with_gc`). Tier-2 differential
drives v4's REAL `projectVaultWardrobe` over a two-op create-then-rename/sweep
sequence (filename collision, composite slug recompute, legacy json cleanup) and
diffs five mount-index tables in the shared-cross-table-id-map remap form (reindex
`chunkCount` / `doc_mount_chunks` pinned/excluded, as for groups/projects). **With
this the entire document-store slice — Family A (generic store-backed) and Family B
(the character/wardrobe vault, read + write) — is complete.**

**The public wardrobe write path (seam #7) is now ported and green**
(`quilltap-core::db::vault_wardrobe_public`, `vault_wardrobe_public_equivalence`):
v4's vault-only `WardrobeRepository.create`/`update`/`delete`, composed over the
verified leaves — resolve the character's mount (`find_by_id_raw` →
`characterDocumentMountPointId`), read current items (`read_character_vault_wardrobe`),
apply + `assertNoCycles` (`detect_component_cycles`, v4's exact `… → …; …` message),
re-project (`project_vault_wardrobe`), minting `updatedAt` on update; a missing
mount throws (`NoMount`). Verified by a **read-back differential** driving v4's REAL
public repo over a baked character+vault fixture (both DBs): create, composite
create (ref by id), rename update, cycle-forming update (throws, folder unchanged),
real delete (surviving composite's dangling ref DROPS on read), delete-missing →
false, and a create for a non-existent character (throws no-mount) — comparing each
op's read-back item list (minted `updatedAt` normalized). Read-back rather than a
byte dump because `build_wardrobe_item_file` writes the minted `updatedAt` into the
content-addressed `.md`; the projection primitive is separately byte-verified
(`vault_wardrobe_write_equivalence`). **Deferred:** the General/project archetype
tiers (same boundary as `read_character_vault_wardrobe`).

**The `characters` repo is now in progress (the store-backed capstone).** It is
NOT a generic store-backed entity — it's a `TaggableBaseRepository` with the
bespoke vault overlay (read overlay + wardrobe read/write already ported), so it's
being ported leaf-first too. Sub-unit 1 — the **managed-fields write projection** —
is done (`db::vault_character_write::write_character_vault_managed_fields`,
`vault_character_write_equivalence`): v4's `writeCharacterVaultManagedFields`.
Projects every vault-managed content field out to its file in v4's exact order —
`properties.json` (the typed pronouns/aliases/title/firstMessage/talkativeness bag,
2-space pretty-print), the five markdown files (`None` → `""`), and (only when a
primary `physicalDescription` exists) `physical-description.md` +
`physical-prompts.json` (`renderPhysicalPromptsJson`), then the `Prompts/` +
`Scenarios/` folder projections — composing the ported leaves
(`build_system_prompt_file` / `build_scenario_file` / `sanitize_file_name` /
`project_array_into_vault_folder`) over `write_database_document`. Banks the
**integer-valued-float `properties.json` seam end-to-end** (`talkativeness: 1.0` →
bare `1` via a `serialize_with` mirroring `js_number_to_json`, since the bytes feed
the dedup SHA; the five keys are a typed struct, not `serde_json::Value`, to fix
key order). Tier-2 differential drives v4's REAL `writeCharacterVaultManagedFields`
over a full-create-then-reproject sequence (a `Prompts/` filename collision, a
folder sweep, the physical-skip-on-clear behavior — physical-* files PERSIST — and
`talkativeness: 1`) and diffs five mount-index tables in the
shared-cross-table-id-map remap form (`chunkCount` / `doc_mount_chunks`
pinned/excluded). Sub-unit 2 — the **slim-row marshaling** — is also done
(`db::characters`, `characters_slim_tier2_equivalence`): the base-repository SQL
CRUD (`_create`/`_update`/`_delete`) over the MAIN-db `characters` table. v4's
overridden `_create`/`_update` strip the `MANAGED_FIELDS` set before the write —
those live in the vault now — so the persisted row is the non-managed complement;
a fresh fixture's table still has the managed columns
(`ensureCollection`/`CharacterSchema`), but both sides omit them from every write
so they sit at their DDL defaults identically. Banks the **widest nullable-boolean
surface in Phase 2** (seven `z.boolean().nullable().optional()` columns — INTEGER
0/1 present, NULL absent) plus a typed JSON-object column (`defaultTimestampConfig`,
a nine-field struct in schema order, NOT `serde_json::Value`), an open JSON column
(`sillyTavernData`, kept `null`/single-key), two typed-struct array columns
(`partnerLinks`/`avatarOverrides`), a string-array (`tags`), two boolean-default
(`isFavorite`/`npc`), an enum TEXT (`controlledBy`), and many nullable UUIDs.
`update` is a partial `SET` that reproduces v4's full `$set` on-disk result (the
fixture cells are already canonical). Tier-2 differential drives v4's REAL
protected internals via a thin subclass over a create/create/update/delete
sequence, pinned zero-normalization form. Sub-unit 3a — `scaffoldCharacterMount`
— is also done (`db::character_vault`, `characters_scaffold_tier2_equivalence`):
populates a fresh database-backed character store with the preset structure —
seven empty top-level folders, six blank markdown files (deduped by the
empty-string sha to ONE file/document row, six links), and two seeded JSON files
(`properties.json` + the four-key `physical-prompts.json`, FIXED default content)
— via the verified storage primitive (folders through the new
`DocMountFileLinksRepository::ensure_folder_path`, files through
`write_database_document`, skip-if-link-exists). Verified standalone (the create
flow's `writeCharacterVaultManagedFields` overwrites the identity files +
`properties.json`, masking the scaffold defaults) by a tier-2 differential driving
v4's REAL `scaffoldCharacterMount`, diffing five mount-index tables in the
shared-cross-table-id-map remap form (`chunkCount`/`doc_mount_chunks`
pinned/excluded). Sub-unit 3b — `ensureCharacterVault` + the **`create`
integration** — is also done (`db::character_vault::create_character` /
`ensure_character_vault`, `characters_create_tier2_equivalence`): v4's full create
end-to-end — slim `_create` (FK nulled) → `ensure_character_vault` (mint a `<name>
Character Vault` mount point, scaffold, project managed fields, link the FK +
confirm it stuck) — verified against v4's REAL `repos.characters.create` over SIX
tables across both DBs (slim `characters` row + the five store tables) in the
shared-cross-db-id-map remap form (everything minted, FKs verify by relationship;
`chunkCount`/`doc_mount_chunks` pinned/excluded). Banks the **orphan-on-rewrite**
default-`properties.json` row (scaffold writes it, the managed bag overwrites it,
no GC → 9 files = 8 live + 1 orphan), the five identity-md overwrites (the
`physical-*` scaffold defaults survive — no physicalDescription), and a
systemPrompt + scenario projected into `Prompts/`+`Scenarios/`. (The
`ensureCharacterVault` adopt branch — startup-heal of a hand-linked same-name
store — is now ported too; see the startup-backfill note below.) Sub-unit 4a — the **`update`
vault integration** — is also done (`db::vault_character_update`,
`characters_update_tier2_equivalence`): v4's `applyDocumentStoreWriteOverlay` (the
managed-field write **router** — markdown routing, the `properties.json`
**read-modify-write** that preserves untouched keys, physical, `systemPrompts`/
`scenarios` reprojection) + the `update` orchestration (route → slim `_update` for
the unmanaged remainder, skipped when empty so a managed-only update does NOT bump
`updatedAt`). Verified over a fixture baked by v4's REAL create, driving v4's REAL
`repos.characters.update` across SIX tables in the shared-cross-db-id-map remap
form; banks the RMW preservation, a DB-only field update, and a prompt
reprojection (sweep + write, orphan/GC counts matching v4 via the shared DDL).
(provision-on-the-fly — a managed-field patch on a vault-less character — is now
ported too; see the startup-backfill note below.) Sub-unit 4b — the **array / sub-array ops** — is also done
(`db::vault_character_arrays`, `characters_arrays_tier2_equivalence`): the
`systemPrompts`/`scenarios`/`partnerLinks` mutators + the
`setFavorite`/`setControlledBy`/`setCanBeCarina` setters. Each sub-array op is v4's
three-beat shape — `find_by_id` (read overlay) → mutate-in-memory (the per-op
`onBeforeAdd`/`onAfterBuild`/`onAfterRemove` default normalization) →
`update_character` (the 4a write overlay) reprojects the `Prompts/`/`Scenarios/`
folder (or writes the slim `partnerLinks` column). The minted item
id/`createdAt`/`updatedAt` never reach disk (the projection writes
`<sanitize(name|title)>.md` from the verified builders; the read side re-derives a
prompt's id from its path), so the DB effect is deterministic. Added a **scoped**
`find_by_id` — the slim columns the ops consume (`id`,
`characterDocumentMountPointId`, `partnerLinks`) + the overlaid
`systemPrompts`/`scenarios`; FULL slim-row read marshaling is sub-unit 4c, with a
read-differential. Tier-2 differential over a fixture baked by v4's REAL create (one
baked prompt/scenario/partner link), driving v4's REAL repository methods across SIX
tables in the shared-cross-db-id-map remap form (`chunkCount`/`doc_mount_chunks`
pinned/excluded); the id-taking prompt/scenario ops carry a `targetName`/`targetTitle`
resolved to the current id via `findById` on each side. Banks addSystemPrompt
(default-demote + non-default), updateSystemPrompt (rename → sweep + content),
setDefaultSystemPrompt, deleteSystemPrompt (deleting the default → survivor
promotion), the three scenario ops, the two partner ops, and the three setters.
Sub-unit 4c — the **`findBy*` read path** — is also done (`db::characters_read`,
`characters_read_equivalence`), **completing the characters capstone**: the
slim-row read marshaling (row → `Character`, the inverse of sub-unit 2 = v4
`hydrateRow` + Zod parse) + the ten `findBy*` queries
(`find_by_id`/`find_by_id_raw`/`find_all`/`find_by_user_id`/`find_user_controlled`/
`find_llm_controlled`/`find_by_ids`/`find_by_default_image_id`/
`find_by_avatar_override_image_id`/`find_by_tag`), each overlaying the vault. The
marshaling reproduces v4's net read shape (nullable cells OMITTED when NULL — v4
`undefined` dropped by `JSON.stringify` — JSON columns parsed, booleans coerced,
`.default([])`/`.default(false)`/`controlledBy='llm'` materialized; the managed
columns hold their DDL=Zod defaults so `scenarios`/`systemPrompts`/`aliases`→`[]`,
`talkativeness`→`0.5`, the nullable managed fields omitted — then the overlay
overwrites them for a vaulted char). The two JSON-array filters (`tags`,
`avatarOverrides.imageId`) use SQLite `json_each`, matching v4's query translator.
Verified by a read-differential: both sides READ a copy of one fixture baked by
v4's REAL create (four characters + vaults) and run the same 11 queries, comparing
the hydrated lists exactly (ids/timestamps identical, no remap; only
`physicalDescription`'s read-minted createdAt/updatedAt placeholdered, lists sorted
by id) — `findByIdRaw` isolating the slim marshaling. Sub-unit 4b's array ops were
refactored to ride this full `find_by_id` (re-verified green), closing the
scoped-reader deferral. **The characters startup-backfill family is now ported**
(2026-07-01), closing the last three characters deferrals — the
`ensureCharacterVault` **adopt branch**, **provision-on-the-fly**, and
**physicalDescription-via-update**. v4 first searches for a populated same-name
`'character'` store (`doc_mount_points::find_by_name`: `enabled=1`, trimmed
case-insensitive match) that passes `vault_has_required_files` (all six required
files present in `doc_mount_file_links`) and **adopts** it iff exactly one
qualifies (ambiguous / zero → fresh provision); the FK-write-and-confirm is now
the shared `link_character_to_vault`. When a managed-field `update` lands on a
vault-less character, `apply_document_store_write_overlay` now **provisions on the
fly** (build the post-cutover `CharacterVaultWriteInput` → `ensure_character_vault`
→ re-read + confirm FK → continue routing) instead of erroring — and that update
path is exactly how a live character reaches the adopt branch, so the two seams
compose. physicalDescription-via-update (the write of `physical-description.md` +
`physical-prompts.json` on a non-null patch, strip-from-DB) was already coded and
is now **proven**. Each ships a green six-table cross-DB remap differential
(`characters_adopt`/`characters_provision`/`characters_physical`
`_tier2_equivalence`) driving v4's REAL `repos.characters.update`/`.create` — the
adopt case's keystone assertion is a **single** surviving mount point (the orphan
store reused, FK relinked, no duplicate). The peer repos
`background_jobs` and `vector_indices` (both
independent, no characters/store-backed coupling) were drafted in parallel.
**`vector_indices` is now integrated and green** (`vector_indices_tier2_equivalence`):
the first **standalone two-table** repo (`vector_indices` metadata + `vector_entries`
embeddings, MAIN db, no base-repository) — a third Float32-BLOB column, two
REAL-affinity number columns, a `saveMeta` upsert (`id == characterId`, pinned),
and v4's exact op semantics (batch-shared `createdAt`, per-id `removeEntries` loop,
embedding-only update, two-op `deleteByCharacterId`); minted-values remap form.
**`background_jobs` is now integrated and green** (`background_jobs_tier2_equivalence`):
v4's `BackgroundJobsRepository`, the durable work queue (UserOwned, no base-method
override). Three REAL-affinity number columns (`priority`/`attempts`/`maxAttempts`
— bare `z.number()` → REAL, not INTEGER) + open-JSON `payload`; the full queue API
(`claimNextJob` atomic claim, `markFailed` exp-backoff DEAD-vs-FAILED, `markCompleted`,
`pause`/`resume`, `cancel`/`cancelByType`, `resetAllProcessingJobs`/`resetStuckJobs`,
`deleteByTypesAndStatuses`) verified over a 13-op differential in the minted-timestamp
placeholder form, with the exact `lastError` strings (em-dash included) diffed
byte-for-byte. **Discovered v4-on-SQLite limitation:** `markCompleted`'s dotted
`payload.result` merge throws `no such column` on v4's SQLite backend, so that path
is a forward v5-only capability (pure `merge_result_into_payload` + unit tests; the
differential exercises only the no-result path). With this, all three peer repos of
the characters capstone (`characters` sub-unit 1, `vector_indices`, `background_jobs`)
are landed, and characters sub-unit 2 (slim-row marshaling) is done; the remaining
characters sub-units (provisioning + scaffold, the `create`/`update` vault
integration, array ops + `findBy*`) are next.

**The `chats` repo is now in progress — the last and largest repo (the
conversation capstone).** v4's `ChatsRepository` is ~2,900 lines across 6 ops
files (~67 methods); messages live in a separate `chat_messages` table. Being
ported leaf-first. Sub-unit 1 — the **slim-row marshaling** — is done
(`quilltap-core::db::chats`, `chats_tier2_equivalence`): `create` / `update` /
`delete` over the **~96-column** `chats` table (MAIN db, the widest marshaling
surface in Phase 2). Banks the typed `participants` **array-of-objects JSON
column** (`ChatParticipant`, 18 fields in schema order, nullable optionals
`skip_serializing_if`, `displayOrder` `i64`, `talkativeness` via a JS-number
`serialize_with` so `1.0` → `1`; `.refine()` requires ≥1 participant); the simple
JSON-array columns; the **plain-string** `turnQueue` /
`spokenThisCycleParticipantIds` (`z.string()` holding JSON text, bound raw); the
numeric columns (all bound `f64`); booleans; enum TEXT; and the nullable
string/uuid/timestamp tail. Two invariants banked: `update` **never mints
`updatedAt`** (preserved unless the caller passes one — only a new message bumps
it), so the differential is the pinned zero-normalization form; and on SQLite
`create` writes nothing to `chat_messages`. Verified by a tier-2 differential
driving v4's REAL `ChatsRepository` over a create×3 / update×3 (both updatedAt
branches) / delete sequence. **Tracked deferrals:** `delete`'s participant-vault
summary sweep (external subsystem), the open-JSON object columns' multi-key
insertion order (corpus kept `{}`/single-key/null). Sub-unit 2 — the **slim-row
read path** — is also done (`db::chats_read`, `chats_read_equivalence`): the read
marshaling (inverse of sub-unit 1 = v4 `_findById` = hydrateRow + Zod parse) + the
`findBy*` queries (`findById`/`findAll`/`findByUserId`/`findByCharacterId`/
`findByType`/`findRecentSummarizedByCharacter`). Reproduces v4's net read shape:
nullable-optional columns OMITTED when `NULL`, `.default(...)`
numbers/bools/enums/arrays + `state` (`{}`) materialized, numbers JS-rendered, and
`participants` re-parsed per-element so each participant's own defaults
materialize (`controlledBy:'llm'`, `displayOrder:0`, `isActive:true`,
`status:'active'`, `hasHistoryAccess:false`) and its nullable-optionals drop. The
`participants.characterId` filters use `json_each`+`json_extract`;
`findRecentSummarizedByCharacter` reproduces the `$exists`/`$nin`/`$ne` filter +
`ORDER BY "lastMessageAt" DESC`+`LIMIT`. Read-differential: both sides READ a copy
of one v4-baked fixture (seven chats — a rich chat hitting every marshaling
branch, a minimal chat, salon/help/brahma types, summarized chats with distinct
`lastMessageAt`), 16 queries compared exactly (no normalization). Sub-unit 3 —
the **`chat_messages` read path** — is also done (`db::chats_messages_read`,
`chats_messages_read_equivalence`): v4's `ChatMessagesOps` read surface
(`getMessages`/`getMessageCount`/`findChatIdForMessage`). Messages live in their
own MAIN-db `chat_messages` table (one row per event); `getMessages` reads every
row for a chat ordered by `createdAt` and validates each through
`ChatEventSchema`, a three-member union (`MessageEvent`/`ContextSummaryEvent`/
`SystemEvent`). The read dispatches on the `type` discriminator and reconstructs
each member — required columns read directly, nullable-optionals OMITTED on
`NULL`, the array/object JSON columns (`rawResponse` [`z.record`], `attachments`,
`reasoningSegments`, `dangerFlags`, `hostEvent`, `customAnnouncer`, `carinaMeta`,
`pendingExternalAttachments`, `summaryAnchor`, …) parsed straight to JSON. **No
read-side default materialization is needed**: v4 runs `ChatEventSchema.parse`
*before* every insert, so each `.default(...)` (`attachments`→`[]`, a
`DangerFlag`'s `userOverridden`/`wasRerouted`→`false`) and the exact
int-vs-float number representation are already baked into the stored bytes — so
the read parses the JSON columns straight to `serde_json::Value` (no struct
re-serialization that would turn `1`→`1.0`). Read-differential: both sides READ a
copy of one fixture baked by v4's REAL `repos.chats.addMessages` (one chat +
twelve messages covering every event member + JSON column), 7 queries compared
exactly (no normalization). (The `isSilentMessage` seam that this sub-unit
originally deferred is now **fully RESOLVED** — see the write-side note under
sub-unit 4a below and phase-2-onramp seam #8: the "drop" premise was wrong, the
read coerces the TEXT-affinity `"1.0"` back to a bool, and the write emits it.)
Sub-unit 4a — the **`chat_messages` write path** — is also
done (`db::chats_messages`, `chats_messages_tier2_equivalence`): v4's
`addMessage`/`addMessages` (the row insert + the chat metadata side-effect).
**`updateMessage`/`deleteMessagesByIds`/`clearMessages` are sub-unit 4b.** The
write marshaling is the inverse of sub-unit 3 but harder — the port reproduces
`ChatEventSchema.parse`'s output bytes itself: materialize each `.default(...)`
and emit every JSON-column object in **schema field order** (matching v4's
`JSON.stringify` of the Zod-parsed object) with integer-valued nested numbers
rendered bare (the stored bytes are compared directly), so each fixed-shape nested
object is a typed struct in schema order (`dangerFlags`/`reasoningSegments`/
`hostEvent`/`customAnnouncer`/`carinaMeta`/`summaryAnchor`/
`pendingExternalAttachments`); the open-JSON `rawResponse` is corpus-constrained
to `{}`/single-key (seam #5). A `message` insert names the `MessageEvent` columns
(always writing `attachments`); a `context-summary`/`system` insert omits
`attachments` so SQLite fills its `DEFAULT '[]'` — mirroring v4's
insert-only-validated-keys. The metadata side-effect recounts
`countVisibleMessages`, bumps `lastMessageAt`/`updatedAt` to a minted `now` only
for an actual `type:'message'` event, and folds `spokenThisCycleParticipantIds`
over the batch via the ported `computeSpokenThisCycleAfterMessage`, routing
through the sub-unit-1 `chats.update` (extended with `lastMessageAt` +
`spokenThisCycle` setters). Tier-2 differential drives v4's REAL
`addMessage`/`addMessages` over a kitchen-sink message (every JSON column), a
context-summary (non-actual: no `lastMessageAt` bump, `updatedAt` preserved,
count 0), and a mixed batch (whisper + system event + public message), diffing
BOTH `chat_messages` (pinned) and `chats` (`lastMessageAt`/`updatedAt` collapsed
to `<ts>` only when they differ from the seed sentinel — a stray mint is caught).
A `message` insert also carries **`isSilentMessage`** (seam #8, write side — now
closed): `Some(true)` → `"1.0"`, `Some(false)` → `"0.0"`, `None` → `NULL`, the
TEXT-affinity bytes v4 produces by binding the JS number `1`/`0` as a REAL that
SQLite converts to text on store (empirically probed; a new `addMessages` op
carries both a true and a false silent message). context-summary/system inserts
omit the column. Sub-unit 4b — the **`chat_messages` mutation path** — is also done (same
`db::chats_messages`, `chats_messages_ops_tier2_equivalence`): v4's
`updateMessage` / `deleteMessagesByIds` / `clearMessages`. `updateMessage`
reproduces v4's `{...existing, ...updates}` → `ChatEventSchema.parse` →
`$set: validated` by reading the existing event (sub-unit-3 read), overlaying the
update keys, re-validating into `ChatEventInput`, and DELETE + re-INSERTing the
merged event — byte-identical to v4's `$set` because a validly-created row's
non-member columns already sit at their DDL defaults, and it reuses the 4a insert
marshaling. `deleteMessagesByIds` deletes each `(id, chatId)` row and recounts
`messageCount` only when something was removed (so `update` preserves
`updatedAt`); `clearMessages` deletes all and resets `messageCount`→0 +
`lastMessageAt`→null (`updatedAt` preserved). Tier-2 differential over a seed of
three chats pre-populated via `addMessages`, diffing BOTH tables with ZERO
normalization (no 4b op mints a chat timestamp). Sub-unit 5 — the **participant
ops** — is also done (`db::chats_participants`,
`chats_participants_tier2_equivalence`): v4's `ChatParticipantsOps`
(`addParticipant` / `updateParticipant` / `removeParticipant` /
`setParticipantStatus` + the four pure `get*Participants` filters). Each mutator
is a read-modify-write of the `participants` JSON column — `find_by_id`
(sub-unit-2 read; `chats` has no vault overlay) → mutate the array in memory
(minting the participant's own id/`createdAt`/`updatedAt`, re-validated through
the participant schema so the Zod defaults materialize + unknown keys strip) →
`update` the chat — and the chat's OWN `updatedAt` is never bumped (v4 `_update`
preserves it; the minted clock values live INSIDE the participants JSON).
`addParticipant` carries the **user-control side-effect** (a `controlledBy:'user'`
participant is appended to `impersonatingParticipantIds` and, when nobody is
typing, set as `activeTypingParticipantId`); `removeParticipant` carries the
**last-participant guard** (`ParticipantOpError::LastParticipant`, v4's thrown
`Error`, leaving the chat unmutated). Banks the **`removedAt` three-shape seam**:
key absent (never removed), the minted string (removed), and an explicit JSON
`null` (a `setParticipantStatus` to a non-removed status clears it) — which
forced widening `ChatParticipant.removedAt` to a double-`Option` with a
**present-keeps-null deserializer** (plain serde maps a stored `null` to the
outer `None`, dropping it; v4's Zod `.nullable().optional()` keeps it through a
re-read + re-write — the differential earned this fix). Tier-2 differential
drives v4's REAL ops (`setParticipantStatus` reached via the private
`participantsOps` field — it is not on the repository surface) over four seeded
chats, diffing the `chats` table; participant ids (pinned seed + minted) are
remapped to first-appearance tokens across the three referencing cells and nested
participant timestamps are sentinel-placeholdered (a value equal to the seed
sentinel stays pinned — proving createdAt preservation + no stray mint), while
chat-level timestamps are diffed exactly (proving "updatedAt not bumped").
Sub-unit 6 — the **remaining four ops files** — is also done, ported **in
parallel** (four agents, each on its own new module + differential; the shared
`ChatUpdate` setters + `mod.rs` wiring pre-staged serially), **completing the
`chats` capstone** (the entire `ChatsRepository` public surface is now ported):
**impersonation** (`db::chats_impersonation`, `chats_impersonation_tier2_equivalence`
— RMW on `impersonatingParticipantIds`/`activeTypingParticipantId`/
`allLLMPauseTurnCount`, the activeTyping reassign-or-clear, mints nothing → zero
normalization); **tokens** (`db::chats_tokens`, `chats_tokens_tier2_equivalence` —
`incrementTokenAggregates` lowering v4's `$inc`/`$set` to one self-referential
`UPDATE … SET col = col + ?` with a minted `updatedAt` + conditional cost
accumulation, and `resetTokenAggregates`; sentinel-aware `updatedAt`
normalization); **search** (`db::chats_search`, `chats_search_equivalence` —
`count`/`find`/`searchMessagesGlobal`/`replaceInMessages`, the `$regex`→SQL `LIKE`
mangling reused verbatim from `memories` [including v4's broken-but-exact
behavior on regex-special inputs], the role/`createdAt DESC`/`limit` filter, and
the split/join replace-all which mints nothing); and **outfits**
(`db::chats_outfits`, `chats_outfits_tier2_equivalence` — RMW on the
`equippedOutfit` JSON column, stored as **raw `Value`** so partial/extra-key
slots are preserved verbatim [v4 never re-validates it], the remove path
mutating each character's slots in place with v4's `before.includes` guard so
absent slots stay absent; the corpus banks a partial-slot character to prove
shape preservation). **New tracked seam:** the `equippedOutfit` open-JSON
key-order divergence (`serde_json::Value` sorts vs v4's insertion order) — corpus
constrained to sorted key order, same family as `parameters`/`sillyTavernData`.
**Tracked deferrals across the whole chats repo:** `delete`'s participant-vault
summary sweep (external subsystem), the open-JSON multi-key insertion-order seams,
and the `equippedOutfit` key-order seam. (The `isSilentMessage` TEXT-affinity seam
is now CLOSED — read and write both — see sub-units 3 and 4a above.)

The **`memories` repo is ported whole** (`quilltap-core::db::memories` +
`db::memories_read`, `memories_tier2_equivalence` + `memories_read_equivalence`).
A plain MAIN-db `AbstractBaseRepository<Memory>` — **no base-method override**
(only the `embedding` BLOB registration) and **no vault overlay**, so every read
is a single-connection SELECT + marshal (simpler than the store-backed
`characters`). The whole surface landed in one unit: the write/mutation side
(`create` — the **fourth Float32-BLOB** column, three JSON-array columns
`keywords`/`tags`/`relatedMemoryIds`, and the three numeric columns where
`importance`/`reinforcedImportance` are **INTEGER-affinity** by `mapToSQLiteType`
[min `0`/max `1` are integers] while `reinforcementCount` is min-only **REAL** —
all bound `f64`, NUMERIC affinity + `js_number_to_json` keeping them byte-exact;
`update` a partial SET that **never names `embedding`** so the BLOB survives a
text-only patch [the `conversation_chunks`/`help_docs` rule]; `delete`;
`updateForCharacter`/`deleteForCharacter` ownership gates; `bulkDelete`;
`updateAccessTime{,Bulk}`; `replaceInMemories` literal substring replace;
`deleteByChatId`/`deleteBySourceMessageId{,s}`) and the read side (all ~30
`findBy*`/`count*`). Banks the **`$regex` → SQL `LIKE` seam**: v4 builds a
`RegExp` from `escapeRegex(query)` and the translator mangles its **source**
(`source.replace(/\.\*/g,'%').replace(/\./g,'_')`, wrapped `%…%`) — reproduced
byte-for-byte so SQLite (same engine) matches identically; the JSON-array
`keywords` `$in`/`$regex` go through `json_each`. Also banks the
`findByCharacterAboutCharacters` **window function** (verbatim CTE
`ROW_NUMBER() … PARTITION BY aboutCharacterId`), `findByCharacterIdPaginated`'s
SQL-filter-then-in-memory-search, and the importance tiers. **New tracked
marshaling seam:** the normal `findByFilter` path OMITS NULL nullable-optional
columns (v4 `undefined` dropped by `JSON.stringify`), but the **raw-SQL**
`findByCharacterAboutCharacters` path KEEPS them as `null` — its `rawQuery` rows
carry explicit NULLs that `MemorySchema.safeParse` retains for a `.nullable()`
field — so the port marshals that one method with `keep_nulls = true`. Verified
two ways: a tier-2 differential (the write/mutation op sequence — rich + minimal
create, the owned/not-owned no-op branches, the bulk/delete-by family — minted
`updatedAt`/`lastAccessedAt` placeholdered), and a read-differential (39 queries
over a v4-baked 6-memory fixture, **zero normalization** since nothing is
mutated; a returned `embedding` is the `Float32Array` `{"0":…}` object rebuilt
from the BLOB).

Repo #4, `prompt_templates`
(`quilltap-core::db::prompt_templates`), round-trips green
(`prompt_templates_tier2_equivalence`): `create` + `update` + `delete` from v4's
`PromptTemplatesRepository` (built-in seeding out of scope). Banks the **first
JSON array column** (`tags` → compact JSON text via `serde_json::to_string` of a
`Vec<String>`; arrays are order-preserving, so no key-order subtlety like the
`tags.visualStyle` object) and several **nullable string columns** (`userId`
null-for-built-in, `description`, `category`, `modelHint`). Adds the **built-in
read-only guard** — `update`/`delete` read the target's `isBuiltIn` and refuse to
mutate a built-in row, returning a not-modified result (`Ok(false)`; v4's `null`
/ `false`) rather than throwing. The harness exercises the guard two ways via an
`expectNoop` flag (a built-in-targeted update and delete), proving both sides
report not-modified on top of the byte-identical dump. Ids + timestamps pinned →
zero normalization.

Repo #3, `text_replacement_rules`
(`quilltap-core::db::text_replacement_rules`), round-trips green
(`text_replacement_rules_tier2_equivalence`): `create` + `update` + `delete` from
v4's `TextReplacementRulesRepository`. It is the **first repo with conflict
detection**, and so the first to need a repo-level *read*: `create`/`update` scan
existing rows and reject a duplicate `(fromText, caseSensitive)` pair
(`TrrError::Conflict`, the analogue of v4's `TextReplacementRuleConflictError` →
HTTP 409; case-sensitive compares exactly, case-insensitive lowercased, the flag
is part of the key, `update` re-checks only when the pair changes). It widens
marshaling again — a real INTEGER number column (`sortOrder`) and two boolean
columns (`caseSensitive`/`enabled`, the latter read back for the check). The
harness corpus exercises the conflict path two ways (a conflicting create and a
conflicting update), each flagged `expectThrow` so both sides independently prove
the rejection (oracle: v4 threw; Rust: `TrrError::Conflict`) on top of the
final-state dump diff. Ids + timestamps pinned → zero normalization. This added
the canonical-dump `js_number_to_json` refinement (an integer-valued REAL cell
renders as a JSON integer, mirroring JS `JSON.stringify`, so REAL-affinity numeric
columns align byte-for-byte). Its case-insensitive conflict branch was the second
`toLowerCase` case-mapping site — **now CLOSED** (a non-ASCII `Café`/`CAFÉ` corpus
pair proves `str::to_lowercase` matches JS in the conflict check).

Repo #2, `tags` (`quilltap-core::db::tags`),
round-trips green through the tier-2 harness (`tags_tier2_equivalence`): `create`
+ `update` + `delete` ported from v4's `TagsRepository`. It widens the marshaling
surface past `folders`' all-strings shape — the `quickHide` boolean stored as
INTEGER 0/1, the nullable `visualStyle` JSON-object column stored as compact JSON
in schema field order (reproduced with a typed struct so key order matches v4's
`JSON.stringify`, **not** a sorted `serde_json::Value`), and the `nameLower`
derivation (`(nameLower || name).toLowerCase()` on create, re-derived from `name`
on update) — and adds the `delete` op to the harness. Determinism unchanged: ids
+ timestamps pinned both sides → zero normalization. The Unicode **case-mapping**
question for `nameLower` (`toLowerCase` vs `to_lowercase`) is now **RESOLVED**:
`str::to_lowercase` is byte-identical to JS `toLowerCase` (locale-independent
Unicode default mapping — verified on İ → `i`+combining-dot, final Σ → ς, ß,
digraphs), so no ICU case-mapping is needed; a non-ASCII corpus row
(`İSTANBUL ÉCOLE ΣΟΦΟΣ Straße`) proves it against the oracle, keeping `findByName`
correct on real data.

**The remap machinery (minted-values tier-2): done.** The on-ramp's
generated-UUID remap + timestamp-placeholder normalization is built and green
(`folders_remap_tier2_equivalence`). `folders.create` now ports v4 `_create`'s
minted defaults (`id = options?.id || generateId()`, timestamps `|| now`) and
returns the id used; `quilltap-core::clock` (`now_iso` / pure
`iso_from_unix_ms`) reproduces `new Date().toISOString()`, and `uuid` mints v4
ids. The test creates a parent + child with NOTHING pinned (both sides mint
different random UUIDs + clocks), then one normalization (in the harness, over
both dumps) walks rows in natural-key order, collapses id columns (`id`,
`parentFolderId`) to first-seen tokens — so the child→parent FK is verified
without pinning literal ids — and placeholders timestamps after asserting the
per-row `createdAt == updatedAt` invariant. This is the normalization form for
the repos/ops that can't take injected ids/clocks; the pinned zero-normalization
form (`folders` / `tags`) remains preferred where the op allows it.

**The partitioned write applier: done.** `quilltap-core::write_apply` ports v4's
`applyWritesUnsafe` quartet — the writer-task apply path that sequences the pure
`write_partition` leaves into real orchestration: each partition (main /
mount-index / llm-logs) in its own `BEGIN IMMEDIATE` transaction; main-primary
(`AUTONOMOUS_ROOM_TURN`) commits main first then secondaries best-effort, while
idempotent jobs apply secondaries first so a secondary failure blocks the main
commit; plus the concurrent `docMountFolders.create` unique-conflict reconcile +
folder-id remap. The engine is generic over an injected `ApplyHost` (the three
connections + repo dispatch + reconcile lookup) — the same orchestration-vs-rows
split v4 uses (it unit-tests the applier with fake DBs + recording repos; the row
writes go through repos, each tier-2-verified separately). So the differential is
**tier-1-style trace equivalence**, not tier-2: `write_apply_equivalence` runs a
committed 12-scenario corpus through both the Rust engine and v4's REAL
`applyWritesUnsafe`, diffing the observable trace (per-partition exec sequence,
ordered dispatches with post-remap args, reconcile lookups, resolved/threw). That
oracle (`harness/oracle/cases/write-apply.test.ts`) runs under **v4's jest**, not
tsx — the applier's `getRawDatabase()`/`getRepositories()` singletons are
`jest.mock`-injected; v4's jest picks up the v5-tree oracle file via an extra
`--roots`. **The `__finalizeFile` + post-commit side effects are now ported**
(deferred-seam #4): the staging→final rename runs inside the main transaction
loop with undo-on-rollback (renames reversed before rethrow), `cleanupStagingDirs`
drops the per-job `.staging/<jobId>` shell, and `dispatchInvalidations` fires the
deduped/ordered vector-store + mount-cache targets — both post-commit, both
skipped on a throw. The pure path/target computation (`path_dirname`,
`find_staging_root`, `collect_invalidations`) lives in the engine; the fs/cache
ops route through four `ApplyHost` methods (harness records them). The trace grew
four fields (renames, mkdirs, staging cleanup, invalidation notifications) + three
scenarios; the oracle records the fs mutators via a jest `fs` mock and the
`notifyChild` mock. **No write_apply deferrals remain.**

**Phase 3 (services / engine): in progress.** Unit 0 — the **writer-task
runtime** — is ported and green (`quilltap-core::db::runtime`), making the
single-writer *ownership* rule a live, compiler-enforced invariant (the shell from
`api-boundary.md` Part 2). `Db` is the `Clone + Send + Sync` handle every service
holds: a per-partition `ReadPool` (pooled read-only opens — `PRAGMA key` first and
only, per the read-path rule) plus a `tokio::mpsc::Sender` that is the **only**
mutator. A dedicated OS thread owns the `WriterSet` (main + optional
mount-index/llm-logs RW `Writer`s) and drains the channel serially via
`blocking_recv`, so batch-apply is naturally serial (the property v4's
folder-conflict remap + main-primary ordering assume). A write is a type-erased
`FnOnce(&mut WriterSet)` closure carrying its own `oneshot` reply — services call
the same typed repositories, but only ever on the writer thread (the `{method,
args}` reflection dissolves into the type system; `write_apply` stays available for
the multi-DB job path, invoked *inside* a closure). `Db::write` (async) /
`write_blocking` (for the plain-`#[test]` harness) / `read_main` /
`read_mount_index` / `read_llm_logs`. Verified by four self-tests: **100 concurrent
writers serialize with no lost updates** (a read-modify-write increment reaching
the writer count), read-after-awaited-write sees committed state, `write_blocking`
commits, and a sibling-partition read on a main-only instance is a clean typed
error (`DbError::PartitionUnavailable`). `tokio` added — `sync` only in the lib
(the writer is a plain OS thread; no scheduler pulled into the core),
`macros`/`rt-multi-thread` dev-only.

Unit 0.5 — the **model-boundary core** — is also ported and green
(`quilltap-core::model`). `model::embedding` defines `EmbeddingProvider` (the
tier-3 seam: an async `generate_embedding_for_user` mirroring v4's
`generateEmbeddingForUser`, with `EmbeddingResult` / `EmbeddingError` /
`EmbeddingPriority`) plus `CannedEmbeddingProvider` — a deterministic responder
keyed by exact input text (fixed vector; explicit failures drive
`SKIP_EMBEDDING_FAILED`; an unregistered input is a surfaced error, never a silent
answer). The boundary is async (`-> impl Future + Send`) and consumers take a
**generic** `P: EmbeddingProvider` (not a trait object), so the async-fn-in-trait
return needs no boxing and the future stays `Send`. Three self-tests. The
v4-oracle-side canned injection (stubbing `generateEmbeddingForUser` to the same
vector) is exercised end-to-end by **Unit 1's** memory-gate differential (below).
The **completion half** is now also ported and green (`model::completion`,
2026-07-02): `CompletionProvider` — the seam at v4's
`provider.sendMessage(params, apiKey)` (the `LLMParams`/`LLMResponse` subset the
cheap-LLM path consumes; API-key acquisition stays host-side, the
temperature/uncensored fallbacks stay ported orchestration *inside* the
differential) — plus `CannedCompletionProvider`, keyed by the exact call input
(`canned_completion_key` = provider | model | temperature-or-`-` | the
`[{role, content}]` JSON; failure entries carry their exact message so
message-inspecting fallbacks can be driven). Five self-tests; the oracle-side
injection lands with the memory-processor differential.

Unit 1 — the **memory gate** — is also ported and green
(`quilltap-core::services::memory_gate` + `db::vector_store`), the **first
tier-3 → tier-2 differential** and the first service to drive the whole Unit-0
write path end to end. It ports v4's `createMemoryWithGate` / `runMemoryGate`
(`lib/memory/memory-service.ts` + `lib/memory/memory-gate.ts`): embed the
candidate (one retry), search the character's `CharacterVectorStore` (the ported
`db::vector_store` shim — load off the read pool, linear cosine top-K, incremental
flush on the writer), then decide by cosine band — `SKIP_NEAR_DUPLICATE` (≥ 0.90),
`REINFORCE` (≥ 0.85), `INSERT_RELATED` (≥ 0.70, link the related memories),
`INSERT` (below), `SKIP_EMBEDDING_FAILED` (embedding unavailable after retry). The
thresholds are the authoritative exported constants
(`NEAR_DUPLICATE_THRESHOLD`/`MERGE_THRESHOLD`/`RELATED_THRESHOLD` = 0.90/0.85/0.70;
the v4 file's `0.80` header comment is stale — ported the constants, let the
differential prove the bands). `reinforce_memory` re-extracts novel details
(reusing the ported `extract_novel_details`), appends footnotes, bumps
count/`reinforcedImportance`/`lastReinforcedAt`, and **re-embeds + rewrites the
vector on a content change**; `link_related_memories` writes both sides. The
service is `async` + generic over `EmbeddingProvider`; reads go through
`Db::read_main`, every mutation through `Db::write` (a closure on the `WriterSet`).
`MemUpdate` gained the `Some`-gated `embedding` BLOB setter (the gate's
`updateForCharacter({ embedding })`) and a `related_memory_ids` setter; a
`dump_table_json_conn` free function snapshots a table off a read-only pooled
connection after a service commits. **Verified two ways:** four core self-tests
(all outcomes over an in-memory `Db` + canned provider), and the tier-3 → tier-2
differential — a jest oracle drives v4's REAL `createMemoryWithGate` (mocking ONLY
`generateEmbeddingForUser` to the corpus's canned vectors, wiring the REAL
`better-sqlite3-multiple-ciphers` cipher binding back in past `jest.setup`'s global
DB mocks — see `[[jest-real-db-oracle]]`) over a seven-scenario corpus (one per
outcome, each on its own character), and the Rust gate is diffed across `memories`
+ `vector_indices` + `vector_entries` in the shared-cross-table-id-map remap form
(minted ids/timestamps remapped/placeholdered; `relatedMemoryIds` array elements
remapped through the shared map). **Tracked deferrals:** the
`skipGate`/`createMemoryDirect` direct path, and the 500 ms inter-retry delay
(host-timing, no DB effect, omitted to keep the core scheduler-free).
(`applyNamePresenceCheck`'s cross-character resolution and the
`maybeEnqueueHousekeeping` watermark check are now **CLOSED** — ported with
the memory processor and the watermark unit, see below.)

The **memory deletion chokepoint** — the first memory-family follow-on — is now
ported and green (`db::memories::delete_with_unlink` / `delete_many_with_unlink`,
`memory_delete_tier2_equivalence`). v4 places `deleteMemoryWithUnlink` /
`deleteMemoriesWithUnlinkBatch` in `memory-gate.ts` (parallel to
`createMemoryWithGate` on the write side), but they are pure `memories`-table
operations — a neighbour-unlink scan wrapped around the repo's own
`updateForCharacter` / `delete` / `bulkDelete` — so they live on the repository.
Every cascade path (housekeeping retention sweeps, chat-wipe, swipe-group cleanup,
single-memory delete) funnels through one of these two so a deleted id never lingers
in another memory's `relatedMemoryIds`. `delete_with_unlink` does v4's
`LIKE '%"<id>"%'` neighbour pre-filter (the quoted id prevents partial-UUID
collisions), the per-neighbour character-scoped rewrite, then the row delete —
idempotent (a missing row returns false without touching neighbours);
`delete_many_with_unlink` does the one-pass scan of every row with a non-empty
links array, scrubs every doomed id from each neighbour in one update, then deletes
the doomed set grouped by character. Verified by a **tsx real-DB** differential (no
model call — deletion touches no LLM; the module functions run directly under
`getRepositories()` + `rawQuery` after `initializeDatabase()`) driving v4's REAL
chokepoint over a pre-seeded nine-memory graph (cross-linked across two
characters), diffing the `memories` dump in the sentinel-aware minted-`updatedAt`
form (an untouched neighbour stays at the seed sentinel — proving no stray bump);
plus four repo self-tests.

The **memory-service cascade-delete family** — the second memory-family follow-on
— is also ported and green (`services::memory_service`,
`memory_cascade_tier2_equivalence`): v4's `deleteMemoryWithVector` + the three
`deleteMemoriesBy*WithVectors` cascades (source-message, swipe-group, chat-wipe)
from `memory-service.ts`, the vector-store-aware wrappers over the chokepoint.
`delete_memory_with_vector` confirms ownership first (the chokepoint is
characterId-agnostic), deletes through the chokepoint, then removes the vector
non-fatally; the cascades group the doomed set by character in first-appearance
order, count only vectors the store actually held (`hasVector` first, each
character's cleanup non-fatal), then batch-delete through the chokepoint — the
swipe-group variant gathers the whole group up front so the neighbour scan sweeps
once. Added `CharacterVectorStore::remove_vector` (v4 `removeVector`: un-add a
same-flush add, else track for deletion, drop any pending update) — so a store
whose sweep removed nothing flushes as a no-op and its metadata `updatedAt` is
provably NOT bumped. Verified by a tsx real-DB differential (no model call)
driving v4's REAL memory-service over an 8-op sequence on an 11-memory /
6-character fixture (cross-character links, two vector-less memories, one
entry-less store), asserting each op's return against the spec on both sides and
diffing `memories` + `vector_indices` + `vector_entries` in the sentinel-aware
minted-`updatedAt` form; plus three service self-tests and three store unit
tests.

**Memory housekeeping** — the third memory-family follow-on — is also ported and
green (`services::housekeeping`, `memory_housekeeping_tier2_equivalence`): v4's
`runHousekeeping` / `getHousekeepingPreview` / `needsHousekeeping`, the retention
sweep the `MEMORY_HOUSEKEEPING` job runs. Three passes then a gated apply:
retention (MANUAL a hard protection override, else the blended
`calculate_protection_score` ≥ 0.5 protects; an unprotected memory goes only when
below the importance floor AND old AND inactive), the opt-in similarity merge
over the **stored** vector index (no model call; ≥ threshold folds into the
more-important/newer survivor — and the merge pass does NOT consult protection,
faithful to v4), and cap enforcement (lowest-effective-weight unprotected from
the tail, with v4's all-protected pre-check). The apply deletes through the
chokepoint then cleans the vector store non-fatally; `dry_run` reports without
writing; detail reasons use the ported JS `toFixed` byte-exactly. Added
`clock::{now_unix_ms, iso_to_ms}` (the strict `Date.parse` inverse of
`iso_from_unix_ms`) and `CharacterVectorStore::all_entries`. Verified by a tsx
real-DB differential over a 6-op / 15-memory / 3-character corpus diffing BOTH
the per-op result objects (counts, id lists, details — the wall-clock-derived
month numbers in reasons placeholdered) and the three table dumps
(sentinel-aware); plus three self-tests. **Corpus freshness:** the spec's
"recent" seed dates age past the 6-month windows ~2026-12 — refresh them when
regenerating after that (both sides stay in agreement regardless; only the
banked outcome descriptions/sanity counts assume fresh dates).

**The memory-processor unit is in progress** (the model-dependent per-turn
extraction, v4 `memory-processor.ts`). Its tier-1 half — the **memory-extraction
pure leaves** (`quilltap-core::memory_tasks`) — is ported and green
(`memory_tasks_equivalence`): the SELF/OTHER extraction prompt builders (the
byte-stable bodies in a **generated** `prompt_text` submodule extracted
mechanically from the v4 source; the first-person-user + autonomous-room
preambles; the ORIENTING CONTEXT footer with its 1500-UTF-16-unit truncation),
the shared `render_turn_context` (roster branches, the user-controlled-slice
single-rendering rule), the message builders (`None` = v4's no-slice early
return), and the response parsers (`parse_memory_candidate_array` /
`parse_other_candidates_by_subject` — fence stripping via `strip_code_fences`
[v4 hosts it in `ai-import.service.ts`], closed-vocabulary targeting-tag
validation with present/wide/information defaults, JS-truthy `JSON.stringify`
coercion, `HARD_CANDIDATE_CAP` = 2 + per-subject/total caps, JS
`Number.isInteger` subjectIndex semantics, and the null-item TypeError that
empties the whole SELF array; `importance` kept as the raw JSON number so
integer emissions re-serialize bare). The jest oracle drives v4's REAL
extractors over a committed 14-case corpus with ONLY `executeCheapLLMTask`
mocked (the seam v4's own extraction tests use), capturing the built messages
byte-for-byte and feeding each case's canned response into the real parser.
**The memory-processor unit is now COMPLETE** — the `processTurnForMemory`
orchestration is ported and green (`services::memory_processor`,
`memory_processor_tier3_equivalence`), the first tier-3 differential to pin
BOTH model boundaries. New modules: `cheap_llm` (v4 `lib/llm/cheap-llm.ts`'s
pure five-priority provider selection + the uncensored-for-dangerous-chats
resolution + `build_character_cache_key`, registry seam injected) and
`services::cheap_llm_exec` (v4 `core-execution.ts`: the executor holding the
session no-custom-temperature cache, the 0.3-temp try + message-inspecting
retry, the 2048 max-tokens floor, the uncensored retry on empty responses —
API-key acquisition and the fire-and-forget `logLLMCall` llm-logs write are
tracked host-side deferrals). The processor ports the per-character rate
limiter (future-dated fixture rows make `countCreatedSince` wall-clock-proof),
the SELF/OTHER passes (OTHER canon from the observer's vault
`Others/<subject>.md` via `read_vault_text_file`, falling back identity →
description → none), dry-run collection, and the per-outcome debug lines
byte-for-byte. **The gate's `applyNamePresenceCheck` deferral is CLOSED**: the
cross-character AUTO lookup now reads both characters through the
vault-overlaid `characters_read::find_by_id` and resolves via the Phase-1
`resolve_about_character_id` (`MemoryGateOutcome` gained
`reinforcement_count`); the differential banks a real flip. The oracle mocks
only the model/infra seams (`createLLMProvider` — recording each exact
`provider|model|temperature|messages` canned key for the Rust
`CannedCompletionProvider` to replay, so prompt/selection divergence surfaces
as a canned-miss — plus canned embeddings, a constant API key, no-op
`logLLMCall`) over a two-database fixture (real vaults + canon file +
gate-band vectors); three calls bank throttle/skip/dup-user logs, all five
gate outcomes, all four canon sources, the uncensored fallback, and usage
aggregation; result objects AND the three tables are diffed (gate differential
re-verified green).

**The gate's `maybeEnqueueHousekeeping` watermark check is now also ported and
green** (`memory_watermark_tier3_equivalence`), closing the gate's last
write-side deferral. New: `services::queue_service` (the `enqueueJob` +
`enqueueMemoryHousekeeping` slice — mint a PENDING `background_jobs` row,
de-dupe against in-flight jobs for the same (userId, characterId), maxAttempts
1; `ensureProcessorRunning` deferred to the job-runner unit, the oracle pins
v4's auto-start to a no-op to match), `services::housekeeping_outcome_cache`
(v4's in-memory ineffective-sweep back-off — kept process-global as in v4, a
`OnceLock<Mutex<HashMap>>` keyed by characterId), and a scoped
`chat_settings::find_auto_housekeeping_settings_by_user_id` read (the full
`findByUserId` marshaling is a later chat-settings read sub-unit). The gate
now runs the check after INSERT / INSERT_RELATED — awaited rather than v4's
`void` (same DB effect once settled; the oracle sleeps for v4's promises).
Differential: seven gate INSERTs over a seeded fixture banking a real enqueue,
below-watermark, the `perCharacterCapOverrides` raise, disabled settings, the
durable 15-minute throttle (future-`updatedAt` seed = always in-window), the
in-flight dedupe, and the in-memory back-off (both sides record the same
outcome through their real cache first); four tables diffed
(`background_jobs` + the three memory tables); the gate and processor
differentials re-verified green with the watermark path live.

**Chat orchestration (Phase-3 Unit 3) is now in progress — waves 1–2 ported and
green** (2026-07-02; decomposition + wave roadmap in
`docs/developer/porting/chat-orchestration.md` — v4's engine is
`lib/services/chat-message/` (~11.7k lines) + `buildContext` + the stateful turn
chain, being ported leaf-first in four waves). **Wave 1** (six parallel tier-1
units, fresh-oracle exact differentials): the **template processor**
(`templates` — `processTemplate`/`buildTemplateContext`/
`processCharacterTemplates`, JS ASCII-`\w` token rule, the two-pass `{{trim}}`
quirk ported faithfully) + the **turn-predicate gap** (`is_users_turn` /
`is_participants_turn` / `get_selection_explanation` added to `select_speaker`);
the **chat timestamps** (`chat_timestamp` — resolve/calculate/should-inject/
format + fictional time, clock injected as `now_ms`; **`jiff` added** (pinned
0.2.31) for the IANA UTC-offset lookup after ICU4X's offset API proved
instant-blind across DST — proven byte-exact against `Intl` on both US DST
boundaries, Kolkata/Chatham fractional offsets, and the invalid-zone throw; v4's
CUSTOM-token sequential-replace bug reproduced and banked) + the **formatting
prompt hint** (`template_prompt_hint`); the **memory-injector formatters**
(`memory_injector` — metadata tag, scene state + sceneHash/`_unchanged_`
compaction, memories, inter-character interleave, frozen archive, dynamic head,
summary; one note: full-precision *debug floats* compared at the tier-1 1e-12
tolerance — a 1-ULP `Math.pow` libm divergence that never survives the
`toFixed(2)` rendering); the **message selector** (`message_selector`, the
greedy tail fit incl. the force-include-last-truncated rule) + the
**core-whisper trigger** (`core_whisper`, first/periodic/silence/
context-transition precedence); the **carina parser** (`carina_parser` — JS
ASCII-`\w` name rule, JS-dot-excludes-line-terminators, smart-quote pairing);
and the **message formatter** (`message_formatter` — the anti-hijack
`stripCharacterNamePrefix`/`truncateAtForeignSpeaker`/
`normalizeContentBlockFormat` + name-field/provider helpers, the
`LEGACY_PROVIDER_NAME_SUPPORT` hyphen quirk) + **finish-reason extraction**
(`finish_reason`). **Wave 2:** the **system-prompt builder** (`system_prompt` —
identity stack / public identity card / other-participants / reinforcement /
`buildSystemPrompt`, composing `templates` + `chat_timestamp` with the clock
injected; banked: `{{timestamp}}` in a *character field* is emptied by the
identity-stack context, only per-turn additions resolve it); the **stateful
turn-orchestration decision core** (`services::turn_orchestrator` —
`should_chain_next` guard chain [not-found → paused → depth → time], the
all-LLM auto-pause WRITE even on a don't-chain return, the turnQueue pop
skipping only the immediate last speaker + write-back,
`persist_turn_participant_id`, and the nudge/queue/dequeue/skipUserTurn/query
action core; RNG + wall-clock injected; `ChatUpdate` gained `turn_queue` +
nullable `last_turn_participant_id` setters; verified by a 13-op tsx real-DB
tier-2 differential over a two-DB seeded fixture, **zero normalization** — no
op mints a chat timestamp); and the **streaming model boundary**
(`model::stream` — `StreamChunk` faithful to v4's chunk vocabulary,
`StreamingCompletionProvider` over a tokio `mpsc::Receiver` of
`Result<StreamChunk, StreamError>` so mid-stream failure is first-class,
`CannedStreamingProvider` sharing `canned_completion_key`; oracle-side
injection lands with the wave-3 primary-stream differential, as
`model::completion` did with the memory processor). **Wave 3 batch 1 — the
seven mutually-independent model-calling/DB-reading services — is ported and
green** (2026-07-02; six parallel agents on disjoint files, the shared
`ChatUpdate` setters + `services/mod.rs` module set pre-staged serially):

- **Compression service half** (`services::compression`,
  `compression_tier3_equivalence`): v4 `applyContextCompression` +
  `compressConversationHistory` (the `MESSAGE_COMPRESSION_PROMPT` verbatim,
  `estimateTokens` = `ceil(utf16/4)`, max-tokens 4000, the trim+re-estimate
  parser) over the ported sizing leaves + `CheapLlmTaskExecutor`.
  System-prompt compression stays permanently disabled (fresh per-character
  identity prompt) — the result shape matched exactly, the dead
  `compressSystemPrompt` path not ported. No DB writes → the tier-3
  differential is result-object equivalence over a 6-case corpus (happy path,
  empty window, LLM-failure warning byte-exact, uncensored fallback,
  empty-from-both throw, Unicode/UTF-16 estimate), completions pinned by
  oracle-recorded canned keys.
- **Context-summary service half** (`services::context_summary`,
  `context_summary_service_tier3_equivalence`): `generateContextSummary` /
  `invalidateContextSummaryIfMessageCovered` / `checkAndGenerateSummaryIfNeeded`
  + the three cheap-LLM tasks (`foldChatSummary`, the two title generators;
  prompt bodies in a generated `prompt_text` submodule) over the ported cadence
  leaves. The fold bumps `compactionGeneration`/`lastSummaryTurn`/
  `summaryAnchorMessageIds` (new pre-staged `ChatUpdate` setters), appends the
  `context-summary` event, **sweeps prior-generation Librarian summary
  whispers** (in scope; the re-post is not), and writes the title;
  `queue_service` gained `enqueue_title_update`. The four cross-subsystem side
  effects (Librarian re-post, vault mirror, relevant-conversations refresh,
  cost events) are a default-no-op `ContextSummarySeams` trait matching the
  oracle's jest mocks (tracked deferrals); `generateContextSummaryAsync` is not
  separately ported (no forked-child write-drop hazard in v5 — callers await or
  spawn). 11-op differential diffing result objects + `chats`/`chat_messages`/
  `background_jobs`.
- **Knowledge injector + first-message context**
  (`services::knowledge_injector` + `services::first_message_context`,
  `knowledge_injector_equivalence` + `first_message_context_equivalence`):
  `retrieveKnowledgeForTurn` (embed-once, per-tier chunk search via the new
  `document_search` child reproducing v4's candidate SQL + cosine/literal-boost
  blend over the ported vector/BLOB leaves, dedupe best-score-wins, greedy
  inline-vs-pointer budget pack, exact rendering strings incl. the 120-char
  word-boundary teaser; pure leaves `dedupe_tier_triple` +
  `format_self_uri`/`format_doc_store_uri`), `memory_service` gained
  `search_memories_semantic` (text fallback ported; the `recallContext`
  re-rank/expansion deferred — no wave-3 consumer), and
  `loadParticipantMemories`/`loadProjectContext`/`buildFirstMessageContext`
  (Recent + Semantic [limit 8, minScore 0.4] + text-fallback, importance sort,
  per-participant error-swallow). Read-only → two read-differentials
  (real-DB-under-jest, only `generateEmbeddingForUser` canned), zero
  normalization.
- **Participant + user-identity resolvers** (`services::participant_resolver` +
  `services::user_identity_resolver`, `participant_resolver_tier2_equivalence`
  + `user_identity_resolver_equivalence`): `resolveRespondingParticipant`
  (continue-mode throw vs normal-mode fallback, zero/one/multiple LLM-candidate
  paths, the multiple path over the ported turn state + weighted selection with
  RNG injected), `loadAllParticipantData`, `getRoleplayTemplate` (chat →
  project → user/global fallback, the inherited default PERSISTED via the new
  `ChatUpdate.roleplay_template_id` setter — the chat's own
  `roleplayTemplateId` column, `updatedAt` preserved), `resolveUserIdentity`
  (four-source chain preferring the active-typing "Speaking As" participant),
  and `resolveConnectionProfile`; scoped reads added
  (`connection_profiles::find_by_id` full net-read marshaling,
  `roleplay_templates::find_system_prompt_by_id`, `users::find_name_by_id`).
  Two tsx real-DB differentials (14 + 5 ops), the one write diffed
  zero-normalization. **Deferred:** host-side API-key acquisition (the
  `cheap_llm_exec` pattern); `connection_profiles.parameters` multi-key
  open-JSON order (corpus `{}`).
- **Primary stream + recovery + provider failover** (the largest;
  `services::primary_stream`/`recovery`/`provider_failover` + the **first
  typed `Event` vocabulary** `services::chat_events`,
  `primary_stream_tier3_equivalence`): `ChatEvent` is `#[serde(untagged)]`
  with exactly the `Status`/`Content`/`Reasoning`/`Done` variants these
  services emit, each serializing byte-identical to v4's single-key SSE frame,
  plus the fire-and-forget `EventSink` seam + `RecordingSink`.
  `run_primary_stream` ports the sending→streaming status flip, cumulative
  reasoning capture/flush, the tool-unsupported retry-without-tools, the
  request-limit recovery early-return, and the idempotent OOC-marker
  `preservePartialOnError`; `save_assistant_message` is the persistence
  primitive the finalizer wave reuses; the `lib/llm/errors.ts` classifiers +
  an en-US `toLocaleString` grouper are ported (recovery text reaches the DB);
  recovery ports the byte-exact message builders + 50-UTF-16-unit static
  fallback chunking (`recoveryType` columns already wired); failover ports the
  same-provider retry + uncensored reroute (`DangerousContentRouter` injected
  so the connections read + key decryption stay host-side) + the five exact
  reason strings. 12-call differential mocking ONLY `streamMessage`
  (rule-match + record → stateful per-key `CannedStreamingProvider` queues) +
  a recording SSE controller; diffs the ordered event trace + both table dumps
  + result objects. **Deferred:** `save_assistant_message`'s tool/image
  branches + confirmation keys (finalizer wave), the real dangerous-content
  resolution, swallowed-persist logging.
- **Carina markup runner** (`services::carina_runner`,
  `carina_runner_tier3_equivalence`): `runCarinaMarkupQuery` (parse →
  consulting → query → public-answer splice [never for whispers] → Prospero on
  error, never-throws catch-all) + the ported `postCarinaResponse`
  (`carina_runner::writer` — the byte-exact `systemSender:'carina'` message).
  A direct read established `runCarinaQuery` drags in the wave-4 tool loop,
  `findCharactersByName`, the commonplace writer, and the Brahma console, so
  per the pre-agreed STOP rule the query engine is the injected `RunCarinaQuery`
  seam and `postProsperoCarinaError` a recorded seam (both jest-mocked
  identically; tracked deferrals). 7-case differential diffing the ordered
  runner trace + `chat_messages`.

All ten differentials (the eight new + `chats_tier2` / `turn_orchestrator`
re-verified, proving the new `ChatUpdate` setters inert on existing paths) run
green against freshly regenerated oracles.

**Wave 3 batch 2 — the finalizer + the `buildContext` capstone — is also
ported and green** (2026-07-02, two parallel agents):

- **The message finalizer** (`services::message_finalizer`,
  `message_finalizer_tier3_equivalence`): v4's `finalizeMessageResponse` +
  `calculateNextSpeaker` — clean (the anti-hijack truncation incl. the
  keep-leading-first-line branch) → reasoning/tool-anchor re-basing
  (shift+clamp, the rewrite-collapse) → the answer-confirmation SKIP gates
  (user-driven → `confirmed: null` + the `confirmationResult` event, which v4
  DOES emit on that path; silent skip; the three-level active gate) →
  persistence (the batch-1 `save_assistant_message` extended with the
  confirmation key bag, `isSilentMessage`, and the `files.addLink` image loop
  — `db::files::add_link` added) → carina markup over the ported runner (the
  query seam owns the post + the new `ChatEvent::CarinaAnswer` emit) → the
  `updatedAt` bump → next speaker over the ported turn manager → the full done
  payload (`DonePayload` extended additively; recovery frames unchanged,
  regression re-verified) → cost tracking (estimation seamed with evidence —
  pricing-fetcher/connections unported — but `trackMessageTokenUsage`'s
  chat-aggregate half ported, awaited per the watermark precedent, the
  null-cost token-counter increment banked) → the background triggers
  (`enqueue_memory_extraction` + `enqueue_chat_danger_classification` added to
  `queue_service`, the turn-closed/autonomous and sticky/classified/no-summary
  gates banked firing, not-firing, and deduping). Verified by a ten-call
  tier-3 differential over a two-DB v4-baked fixture diffing per-op results +
  the ordered event traces (all ids pinned) + the compression/cost seam
  records + four table dumps in a pre-run-snapshot sentinel-aware form.
  **Tracked deferrals:** the active confirmation call + project-override read
  (wave 4), `saveToolMessages` non-empty (wave 4), the async-compression / RNG
  seams (gates banked), the summary-check invocation (gate reproduced; the
  corpus banks the skip path — the real call lands with the `processMessage`
  spine), and the danger-resolver OFF short-circuit (corpus keeps mode
  non-OFF).
- **The `buildContext` capstone** (`services::build_context`,
  `build_context_tier3_equivalence`): v4's ~1,600-line context assembler
  composed from the ported subsystem — system-prompt blocks 1–3, the budget
  math (the `CONTEXT_HISTORY_BUDGET_RATIO`/`MEMORY_BUDGET_RATIO` consts now
  mirrored), phase-1 budget compression over `services::compression`, the
  two-pool memory retrieval (`search_memories_semantic` + the archive/head
  formatters), scene state, inter-character memories (the window-function read
  + per-character relevance), knowledge retrieval, summary-anchor drop + the
  Librarian `SUMMARY_CONTENT_PREFIX` cache breakpoint, multi-character
  attribution/whisper shaping, timestamp injection, and the trailing
  Commonplace recall fold into the user message (the
  `buildCommonplaceLLMContext`/persona/timestamp content builders ported
  verbatim). The unported feeders and every whisper-posting side effect are a
  `BuildContextSeams` trait (default no-ops) mirrored by the oracle's jest
  mocks — recap, keyword distillation, mount-pool resolution, frozen archive,
  live wardrobe, off-scene introductions, and the core/commonplace/mail/host
  posts — per the `ContextSummarySeams` precedent. Verified by a tier-3
  differential driving v4's REAL `buildContext` over a two-DB fixture (real
  vault + `Knowledge/` chunks + memories/vectors, frozen wall clock both
  sides), diffing the full `BuiltContext` byte-for-byte across seven ops
  (plain, recall+knowledge, skip-memories, timestamp on, compression applied
  with recorded canned keys, summary-anchor/breakpoint, multi-character) — the
  only normalizations the recall-adjustment debug fields the search's own
  deferral omits, plus a serde_json float-parse canonicalization (no
  `float_roundtrip` feature) discovered via a 1-ULP text-fallback score.
  **Tracked deferrals:** phase-2 `compressMemories`, the off-scene scan
  composition, the core-whisper branch (config read + packet), the scene-cache
  prior-emission read, `EVERY_N_MINUTES` resolution, and
  `autonomousContextCap`/cached-compression plumbing (the `processMessage`
  spine).

**The `processMessage` spine + `executeTurnChain` is now also ported and
green** (`services::orchestrator`, `orchestrator_tier3_equivalence`) —
completing the planned wave-3 roadmap. It composes the landed wave-1..3
services into one full user-message → assistant-response cycle: participant +
user-identity resolution → `build_context` → `run_primary_stream`
(+ recovery/failover) → empty-response recovery →
`finalize_message_response` → the finalizer-**deferred**
`check_and_generate_summary_if_needed` invocation (CLOSED here, wired where
v4 wires it) → `execute_turn_chain` re-entering `process_message` per turn
(depth-20 / 300 s guards, clock injected; the
`turnStart`/`turnComplete`/`chainComplete` frames + the empty-response done
fields added to `chat_events`). The many unported subsystems it touches
(attachment / tool / agent-mode / danger / courier / RNG / prospero-cadence /
chat-settings read) are `OrchestratorSeams` with their v4 gates reproduced
and banked inactive. The **first end-to-end tier-3 differential** drives v4's
REAL send path over a six-case corpus (full single turn, continue-mode,
empty-response retry, mid-stream preserve-partial, a summary-check firing a
real fold, and a multi-character chain), mocking ONLY the model boundaries +
out-of-scope subsystems (matching the Rust seams) and freezing
`Date`/`Math.random`; the ordered event trace + `chats`/`chat_messages`/
`background_jobs` dumps diff green (`message_finalizer` + `primary_stream`
differentials re-verified). **Two open items discovered by this unit are now
BOTH CLOSED:** (1) v4's `buildMessageContext` wrapper
(`context-builder.service.ts`) is **now ported and green** (2026-07-03,
`services::message_context`) — see the next paragraph. (2) A flagged
chain-depth divergence — now
**INVESTIGATED and RESOLVED (2026-07-03) as an oracle-harness artifact, not a
v5 bug**. The flag: a non-continue single-LLM-char + user chat where v4 chained
the sole LLM character to `max_depth` while the Rust spine stopped at
`user_turn`. Root cause: the differential's oracle **froze `Date.now()`**, so
every minted message got an identical `createdAt`; v4's `getMessages` sorts
`ORDER BY createdAt ASC`, and under the all-equal tie the non-continue USER row
(which carries the user participant id) could sort *after* the assistant
replies, flipping `calculateTurnStateFromHistory`'s `lastSpeakerId` to the user
→ `selectNextSpeaker`'s cycle-wrap (which ignores `spokenThisCycle`) re-picks
the sole LLM character every turn → `max_depth`. The Rust side stamps
`createdAt` from a **real monotonic clock**, so the latest ASSISTANT always
sorts last → correctly stops at `user_turn`. Proven by making the v4 oracle
clock **tick +1ms/read**: v4 then also stops at `user_turn`. The ported
`should_chain_next`/`select_next_speaker`/`calculate_turn_state_from_history`
are byte-faithful; **v5 is correct** (its real-clock ordering matches
real-world v4). Fix + pinning: the orchestrator oracle clock now advances 1ms
per read, the differential diffs
`spokenThisCycleParticipantIds`/`turnQueue`/`lastTurnParticipantId` **exactly**
(previously placeholdered), the job-payload
`turnOpenerMessageId`/`extractionAnchorMessageId` are remapped through the
shared message idmap, and two chain-depth cases were added
(`noncontinue_single_user_chain` → `user_turn`; `noncontinue_two_llm_maxdepth`
→ genuine `max_depth`). See `[[chain-depth-frozen-clock-artifact]]`. Then wave 4
(tools, providers, danger/agent/courier/confirmation, enclave) per the
decomposition doc.

**The `buildMessageContext` wrapper is now ported and green** (2026-07-03,
`services::message_context`, v4 `context-builder.service.ts`) — the wrapper
between `processMessage` and `buildContext`, ported leaf-first. The three pure
leaves ride a dedicated tier-1 differential (`message_context_leaves_equivalence`,
12 cases driving v4's REAL exports): `build_conversation_messages` (the
type/role filter + the `assistantAfter` reverse pass + TOOL-result render with
the `>3`-turn elision + compact-args slice), `normalize_whisper_roles` (Staff
re-role to USER, the opaque-body swap, the attachment-bearing-stays-ASSISTANT
exemption), and `collect_lantern_image_file_ids_for_character` (the own-turn-stop
walk, history cutoff, dedup, lookback cap, reversal). The composition
(`build_message_context`) runs the A–D whisper pre-filters (commonplace strip +
the `relevant-conversations` survival exception; TOOL-whisper target filtering;
opaque-anywhere over the LLM participants' `systemTransparency` — the responder
from `character`, the rest from `participantCharacters`; whisper re-role), then
`buildConversationMessages`, the ported `build_context`, then
`formatMessagesForProvider` (multi-character), the Lantern merge, the
trailing-prefix injection (L), and the multi-character scene block (M — the
Anthropic system-instruction route vs the non-Anthropic `[Name]` prefill). It is
wired into the orchestrator spine where the direct `build_context` call sat: the
wrapper's `formattedMessages` now feed the stream (so `formatMessagesForProvider`
+ the scene block reach the wire). The **K file-loading half**
(`loadChatFilesForLLM` + `processFileAttachmentFallback`) is the injected
`MessageContextSeams` (default no-op, wave-4 file subsystem); the pure
id-collection leaf IS exercised. Verified by REBUILDING the orchestrator oracle
to drive v4's REAL `buildMessageContext` (the passthrough mock dropped; ONLY the
K file-loader mocked, mirroring the Rust seam) — every pre-existing case now runs
the real wrapper (each corpus chat is multi-character: `isMultiCharacterChat` is
true for ≥1 LLM participant, so the scene block + name prefixing apply
throughout, changing the canned stream keys, which the regenerated oracle records
and the Rust port reproduces byte-for-byte). The corpus gained five cases banking
the wrapper's new logic: `nonanthropic_scene` (an OPENROUTER profile → the
non-Anthropic `[Name]` prefill route), `commonplace_strip` (A: strip + the
`relevant-conversations` exception kept), `opaque_swap` (C: the persona-free
`opaqueContent` body reaches the LLM) vs `transparent_no_swap` (C: content
preserved), and `tool_whisper_filter` (B: the operator-targeted TOOL whisper
filtered out of the responder's context). **Tracked deferral:** the K file
subsystem (`loadAndProcessFiles` / `loadChatFilesForLLM` /
`processFileAttachmentFallback`) is wave-4 (W4.4); the L prefix injection with a
non-empty Lantern prefix lands when that seam is closed. With this, the
orchestrator family has no open items — wave 4 is next.

**Drift catch-up (2026-07-01): the answer-confirmation columns.** v4 commit
`29f3ae63` (a Salon consistency-check + re-affirmation feature) added DDL/schema
fields to six already-ported marshaling surfaces. A drift check (regenerating
every affected oracle from current v4 and re-running the existing differentials
unchanged) confirmed no regression — the new columns are additive/nullable-default,
so every pre-existing corpus still passed. The marshaling was then extended to
match and re-verified byte-exact against v4's current oracle output:
`chat_settings.answerConfirmationSettings` (new nested JSON-object column, schema
position between `thinkingDisplay`/`storyBackgroundsSettings`), `chats.
answerConfirmationOverride` (nullable enum TEXT, parallel to `conciergeOverride`,
wired in both write and read), `chat_messages`' five new `MessageEvent` fields
(`confirmed`/`confirmationChecked`/`confirmationRevised`/`confirmationNotes`/
`confirmationOriginalContent` — ordinary nullable boolean/string columns, NOT the
`isSilentMessage` TEXT-affinity seam), `projects` properties.json's
`answerConfirmationOverride` (now a 17-key bag, added to
`PROJECT_STORE_MANAGED_FIELDS` too), and `llm_logs`' new `ANSWER_CONFIRMATION`
enum member (a corpus-only change — the column is plain TEXT on the port side).
The answer-confirmation *service* itself remains unported wave-4 work — this
catch-up only closed the marshaling gap on surfaces already ported. (The
cheap-LLM `profileParameters` forwarding fix, flagged unported here at the
time, landed the next day inside the wave-3 ports: `cheap_llm` /
`cheap_llm_exec` carry `profile_parameters` end-to-end, and `LLMParams` /
`StreamParams` forward it through the completion/stream seams.)

**Drift check (2026-07-03): v4 `8efe1ba9..f69200bb` (17 commits) audited.**
Every commit was classified against the ported surface; no ported unit is
stale. Findings: (1) `8cf7272e` (profileParameters forwarding) and `29f3ae63`'s
service-layer halves are IN the wave-3 ports (ported 2026-07-02 from post-fix
source — the finalizer's confirmation gates + the `confirmationResult` event
frame included); the remaining forwarding sites live only in unported wave-4
callers (ai-import, wizards, greeting, gatekeeper, announcer), and the
`ANSWER_CONFIRMATION` log-type mapping sits inside the deferred host-side
`logLLMCall`. (2) `69fa611e` changed v4's jest config (unit runs now exclude
`*.integration.test.*`; `^better-sqlite3-multiple-ciphers$` now maps to the
mock) — the oracle machinery is unaffected (no oracle file uses that suffix or
requires that bare name; the abs-path `requireActual` bypasses the mapper),
proven by regenerating the memory-gate oracle under the new config and
re-running its differential green. (3) New unported v4 surfaces, all wave-4 /
Phase-4, tracked in the decomposition docs: the anthropic plugin's
adaptive-thinking + sampling-param-rejection rules for Sonnet 5 / Opus 4.7+ /
Fable / Mythos (`733fa12c`/`36d04ab0` — lands with the provider manifest), the
wardrobe move/copy transfers endpoint (`77650571` — an API route over ported
repo ops plus the deferred General/project archetype tier, which now has a
second consumer), the wardrobe public READ trio
(`findByCharacterId`/`findByCharacterIdRaw`/`findByIdForCharacter` — the
delete route now pre-checks via `findByIdForCharacter`, `fafd5449`),
`lib/chat/qtap-linkify.ts` + the markdown-renderer step 3.5 (`52eb0eb8` —
Phase-4 rendering; NOTE its regex uses lookbehind, unsupported by the Rust
`regex` crate), and the workspace/tab lib (`b8368c5a`/`c74bde4a` — Phase-4
Angular state). The `docs/v4/` mirror was refreshed (CHANGELOG, DDL.md, the
answer-confirmation feature doc).

**Wave 4 (W4.0): the wardrobe drift batch is DONE** (2026-07-03). The whole
General/project **shared-archetype tier** that the drift check flagged is now
ported and closed — substantially larger than the plan's one-line bullet
implied, because it pulls in the entire General/project-wardrobe subsystem and a
new `instance_settings` reader. Landed:

- **`db::instance_settings`** — the per-instance key/value store (main db); only
  `get_general_mount_point_id` is needed (the "Quilltap General" store id), and it
  tolerates a missing table like v4's `readSetting` try/catch. Unit tests.
- **The archetype-seeding generalization of the read overlay.** v4's
  `readCharacterVaultWardrobe` seeds shared archetypes (`findArchetypes(true)`)
  into the component-resolution maps so a composite can reference a household item
  it doesn't hold. The v5 `resolve_and_check_component_items` used index-valued
  maps (no room for items outside the local vec), so it was generalized to accept
  a `SeedArchetype` list (id/title/components) with v4's local-wins gap-fill;
  `read_character_vault_wardrobe` gained `seed_archetypes` + an injected
  `fetch_archetypes` closure (character/public reads pass `true`; the
  General/project readers pass `false`). Backward-compat proven: the existing
  `vault_wardrobe_read`/`vault_wardrobe_public` differentials stay green (their
  corpora provision no General store → empty seed → no-op), plus two new resolver
  unit tests bank real seeding + a cycle routed through an archetype node.
- **`db::archetype_wardrobe`** — `read_general_wardrobe` / `read_project_wardrobe`
  (via the overlay with `seed=false`, `characterId` coerced to null, archived
  filter), the `find_archetypes` insertion-ordered General-under-project merge
  (project shadows on id collision), `find_archetype_by_id`, and the
  `ensure_*_wardrobe_folder` helpers.
- **The public READ trio** (`db::wardrobe_read`) — `find_by_character_id`
  (v4 `getOverlaidWardrobeItems`: resolve the mount, seeded read, coerce
  characterId, archived filter, graceful `[]`) + `find_by_id_for_character`
  (owned-then-archetype-fallback). `findByCharacterIdRaw` is a **tracked
  deferral** (deprecated, reads the pre-cutover `wardrobe_items` table the vault
  era drops, no W4.0 consumer). Verified by a read-differential
  (`wardrobe_public_read_equivalence`) against v4's REAL repo — five cases where a
  character composite references a General archetype by **slug AND UUID** and both
  resolve only because the read seeds the shared tier, plus the archetype
  fallback.
- **The public WRITE generalization** (`db::vault_wardrobe_public`) — the
  character-only path became a `WardrobeLocation` (character/General/project)
  routing `create/update/delete` through shared at-location primitives, with
  `buildCyclePeers` seeding General archetypes for character/project scopes and
  the new `create/update/delete_project_wardrobe_item` entry points; a `null`
  characterId now resolves to Quilltap General instead of `NoMount`. Re-verified
  green.
- **`services::wardrobe_transfers`** — v4's `transfers/route.ts` POST (move/copy
  across the four tiers) + the GET destination enumeration, composed over the
  ported repo ops/readers/writers + `ensure_official_store`. Verified by a tier-2
  differential (`wardrobe_transfers_tier2_equivalence`) driving v4's **REAL POST
  handler** (jest-real-DB oracle: `getServerSession` mocked, the real encrypted DB
  wired past jest.setup) over five scenarios (copy→general, move→project,
  copy→character, same-source/dest reject, id-collision reject), diffing the
  outcome + seven mount-index tables in the shared-cross-db-id-map remap form. One
  **harness-normalization gotcha** surfaced and was fixed (not a port bug): a
  copy's minted id/timestamps live in the content-addressed `.md`, so `fileId`
  tokens must be assigned by the `file_links` walk (store+path stable) — walked
  before `files`/`documents` — and `chunkCount` (a v4-reindex-only value) must be
  pinned BEFORE the sort, else it perturbs same-store file ordering. See
  `[[wardrobe-transfers-remap-gotcha]]`.

**Wave 4 (W4.1a): the RNG subsystem is DONE** (2026-07-03), the first sub-unit of
the tool subsystem. v4's pre-message RNG auto-detect path — scan the user message
for dice/coin/bottle patterns, execute them, and write TOOL messages into the
chat *before* the model turn — is ported end to end, closing the orchestrator's
`user_message_rng` seam. Three sub-units:

- The **pure detector** (`quilltap-core::rng_patterns`, `rng_patterns_equivalence`,
  54 cases) — v4's `rng-pattern-detector.service`
  (`detect_rng_patterns`/`convert_patterns_to_tool_calls`/
  `detect_and_convert_rng_patterns`). The three regexes reproduce JS fidelity:
  ASCII `\b`/`\d` via `(?-u:\b)`/`[0-9]`, the JS-`.` line-terminator exclusion
  (`[^\n\r\u{2028}\u{2029}]`), the coin `flip.{1,3}coin` 1–3-char quirk ("flip a
  coin" matches, "flip the coin" does NOT), and the spin-bottle `{0,50}` bound.
  Tier-1 differential over both the detected patterns and the converted tool
  calls, banking bounds rejections (`d1`/`d1001`/`101d6`/overflow), non-ASCII
  adjacency, and a ReDoS adversarial string. `RngType` serializes back to v4's
  number-or-string union (a bare number for dice).
- The **executor** (`quilltap-core::tools::rng`, `rng_executor_equivalence`, 14
  cases) — v4's `rng-handler` (`execute_rng_tool` / `secure_random_int` /
  `roll_dice` / `flip_coin` / `spin_the_bottle` / `format_rng_results` + the Zod
  input validation). The randomness source is an **injected byte stream**
  (`RandomBytes` trait — production `OsRandomBytes` over `getrandom`; the
  differential replays a committed sequence), so `secureRandomInt`'s
  rejection-sampling *variable-length* byte consumption is itself part of what the
  diff proves (the `random01`-injection precedent extended to a byte stream). The
  differential drives v4's REAL `executeRngTool` against a real fixture DB (spin
  resolves participant names through the ported repos, filtering `isActive`) under
  a jest-real-DB oracle with **only `crypto.randomBytes` pinned**, diffing the
  `RngToolOutput` + the formatted string + asserting byte-exact stream consumption
  (a rejection-sampling-rejects case included both for dice and spin).
- The **orchestrator seam closure**: the ported detector + executor now run inline
  in `process_message` (the seam removed), writing a TOOL message per detected
  pattern — the content JSON a typed struct in v4's field order
  (`{tool, initiatedBy:'auto-detect', success, result, prompt, arguments:{type,
  rolls}}`), byte-identical — and appending it to `existing_messages` so the model
  turn sees the results. The byte source is injected via
  `OrchestratorDeps::rng_bytes`. The tier-3 corpus gained three cases (`rng_dice`
  → one dice TOOL row, `rng_two_patterns` → dice-then-coin ordering, `rng_no_fire`
  → a rejected-pattern content writing nothing) and `autoDetectRng` was flipped ON
  globally (a per-user setting; existing corpus content carries no RNG patterns,
  so they no-op); `orchestrator_tier3_equivalence` re-verified green whole. **New
  gotcha:** a jest `crypto` mock must NOT set `__esModule: true` / an explicit
  `default` — that nulls the default import `import crypto from 'crypto'` the vault
  overlay uses (→ `createHash` undefined); spread the real module and override only
  `randomBytes` (see `[[jest-crypto-randombytes-mock]]`).

**Wave 4 (W4.1b): the tool-subsystem pure leaves are DONE** (2026-07-03) — the
pure foundations the tool loops (W4.1e/f), executor (W4.1c), and handler catalog
(W4.1d) will consume, all tier-1 exact against v4's real `lib/tools/` + service
code. Three sub-units:

- **b.2** tool-call threading (`services::tool_call_threading`, v4
  `tool-call-threading.ts`): `build_assistant_tool_call_message` /
  `build_tool_result_messages` — the callId-present-vs-absent pairing rule
  (toolCalls array + native `tool`-role results, else content-only + `[Tool
  Result: <name>]` user messages), empty/whitespace-prose collapse,
  reasoning/thoughtSignature forwarding, and the arguments `JSON.stringify`
  (order-preserving `Value` + integer-valued-float collapse).
  `tool_call_threading_equivalence` (22 cases).
- **b.1** the pseudo-tool machinery (`tools::{simple_json_parser,
  text_block_parser, simple_json_prompt, text_block_prompt, native_tool_prompt,
  pseudo_tool_support}` + `services::pseudo_tool`): the three-tier simple-json
  parser (strict → bounded-jsonrepair → balanced-brace), the text-block
  parser/converter (params/content, alias/param-alias maps, number/boolean
  coercion, wardrobe single-op wrapping), both prompt builders, the native-tool
  prompt, mode resolution (`resolve_tool_mode`/`should_use_text_block_tools`),
  and the service wrappers. **The two backreference regexes are hand-rolled** (the
  `regex` crate has no backreferences): the simple-json `<alias\s*>…</\1>` tag
  scanner (leftmost, non-greedy-to-first-close-or-`$`, UTF-16 offsets) and the
  text-block content form (hybrid — the `regex` crate matches the intricate
  attribute/escaped-quote open tag, a manual scan finds the backreference close).
  **The jsonrepair tier is a bounded hand-rolled subset** (single-quoted /
  curly-"smart"-quoted strings/keys, unquoted keys, trailing commas) — a
  strict-JSON-plus-relaxations recursive-descent that consumes the whole input or
  fails; out-of-subset malformations (arbitrary garbage, code fences, unquoted
  string *values*) resolve **conservatively to a tier failure → `[]`**, matching
  v4's failure shape, never a different non-empty parse; the corpus pins both
  sides (a repaired case per relaxation + a "not json at all" fail case; the
  code-fence case is the documented seam, excluded). Differentials:
  `pseudo_tool_parsers_equivalence` (138 cases — all five tag aliases × case
  variants, the three tiers each hit incl. balanced-brace recovery, missing-name /
  total-failure → `[]`, second-block-dropped, strip idempotency, text-block
  content/self-closing/multi-param/escapes/malformed-not-matched, `parseTagParams`
  escapes, mode resolution, `formatSimpleJsonToolResult`, stop sequences) and
  `pseudo_tool_prompts_equivalence` (40 cases — each builder over flag combos, the
  simple-json builder rendering signatures from the real b.3 definitions, plus the
  `determineTextBlockToolOptions`/`determineEnabledToolOptions` config mappers).
  The `log*ToolUsage` wrappers are logging-only → not ported; `checkModelSupportsTools`
  (async pricing lookup) is the host-side boundary, its boolean the injected
  `supports_native_tools` input.
- **b.3** the tool-definition catalog (`tools::definitions`): all **57**
  definitions from the **56** `*-tool.ts` files (`search-scriptorium-tool` exports
  both `searchScriptorium` and `searchScriptoriumBrahma`; `ALL_TOOLS` matches the
  directory exactly — no omission or extra). Stored as **byte-exact static JSON**
  transcribed from v4's `JSON.stringify({name, description, parameters})` output —
  NOT by re-implementing the Zod→JSON-Schema emitter (v4's `zodToOpenAISchema` is
  a thin wrapper over Zod 4's `z.toJSONSchema`, static-data-at-import) — in a
  generated `data` submodule (the `prompt_text` precedent) produced by the
  checked-in `harness/oracle/tools/gen-tool-catalog.mjs` (regen recipe in its
  header). Accessors (`definition_by_key`/`_by_name`, `all_universal_tools`) bridge
  into the existing `canonicalize::UniversalTool`. The byte-exact differential
  (`tool_definitions_equivalence`) proves the serde preserve-order round-trip
  reproduces JS `JSON.stringify` (no float/escaping divergence over the real
  payloads), catalog completeness, and a `canonicalize_universal_tools` spot-check
  over the full real catalog (its own differential predated real definitions). The
  `.default()`-in-`required` quirk (rng `rolls`, askCarina `whisper`) is preserved
  verbatim; determinism verified (dump twice, identical).

Out of scope (later W4.1 sub-units): the tool loops,
`processToolCalls`/`saveToolMessages`, `buildTools` slate construction, the
handlers, and provider wire parsing.

**Wave 4 (W4.1c): tool execution + persistence primitives are DONE**
(2026-07-03; `services::tool_execution`, v4 `tool-execution.service.ts`) — the
execution harness + the TOOL-row writer between the tool loops (W4.1e/f) and the
handlers (W4.1d). Three sub-units. **c.1** — `save_tool_messages` +
`compute_tool_message_targets` + `db::files::add_tag` (the inherited
`TaggableBaseRepository.addTag`): the TOOL-row persistence primitive through the
ported `chats_messages::add_message` path (verified: a TOOL row is
`type:'message'`, so it bumps minted `lastMessageAt`/`updatedAt` but is excluded
from `messageCount` — `countVisibleMessages` drops SYSTEM/TOOL — and never
touches `spokenThisCycle`, the cycle helper returning null for non-USER/ASSISTANT
roles), the whisper gate (ALWAYS_PRIVATE {`search`,`read_conversation`} +
VAULT_READ doc tools vs `allowCrossCharacterVaultReads`, **whispered to the user
participant** `[userParticipantId]` — NOT the calling character, resolved from
`computeToolMessageTargets`), the generic content JSON as a typed struct in v4
field order (`toolName, success, result, arguments, callId, [anchorOffset],
[seq], provider, model, prompt` — `js_number_to_json` for the anchors, `metadata?`
keys dropped when absent), and the generated-image link+tag loop. Tier-2
differential (`tool_execution_tier2_equivalence`) drives v4's REAL
`saveToolMessages` over the whisper matrix, content omission (anchors/callId +
metadata present vs absent), the multi-message batch + `firstToolMessageId`, and
the image link+tag — diffing `chat_messages`/`chats`/`files` + the return values
in the shared-content-sorted-id-remap form (minted message ids remapped, minted
timestamps sentinel-placeholdered). **c.2** — `process_tool_calls` + the injected
`ToolRunner` boundary (v4 `executeToolCallWithContext` + every handler, all
W4.1d) + `ToolExecutionContext` (`create_tool_context`; the `emitCarinaAnswer`
callback + typed `loadedMemories` are documented deferral slots): the per-call
dispatch harness (detection frame → per-tool `tool_executing` status → dispatch →
generated-image extraction → tool-result frame; a handler failure is the in-band
failure `ToolMessage`, never a Rust error). `services::chat_events` gained the
additive `toolsDetected` + `toolResult` frames (byte-matching v4's SSE JSON).
Tier-3 differential (`tool_execution_process_tier3_equivalence`) drives v4's REAL
`processToolCalls` with ONLY `executeToolCallWithContext` mocked (canned per-call
results keyed by `name|JSON.stringify(args)|callId`), diffing the ordered frames +
`toolMessages` + `generatedImagePaths`. **c.3** — spine wiring: `save_tool_messages`
wired into the finalizer's `toolMessages.length > 0` gate **inside**
`save_assistant_message`, before the assistant image-link loop (so a generated
image's `linkedTo` order is `[firstToolMessageId, assistantMessageId]` as v4), and
the orchestrator tool-only terminal branch (`persist_tools_only` = the TOOL rows +
the explicit `updatedAt` bump, then the `toolsExecuted: true` done frame). The
finalizer direct-drive caught a real bug — the done frame's `toolsExecuted` was
hardcoded `false`; now `!tool_messages.is_empty()` (v4 `toolMessages.length > 0`).
Both branches are **corpus-dormant** until the tool loops (W4.1e/f) produce a
non-empty slate (v4's inline tool-only block cannot be end-to-end-driven without
them); `message_finalizer_tier3_equivalence` gained a `tool-save` case driving
v4's REAL finalizer with an injected slate (state → public, search → whispered),
and `orchestrator_tier3_equivalence` is re-verified green (the branch inert, the
new `allowCrossCharacterVaultReads` field on `FinalizerChat` inert). The canonical
`ToolMessage` now lives once in `services::tool_execution`;
`services::tool_call_threading` reuses it (its narrow 3-field subset removed),
matching v4's single `chat-message/types.ts` definition (threading differential
re-verified). **Tracked deferrals:** the executor dispatch + handlers (W4.1d), the
tool loops (W4.1e/f), `buildTools`, provider wire parsing (W4.7); the `ToolRunner`
/ `ToolExecutionContext` callback slots to their owning units.

**Wave 4 (W4.1d batch 1): the first tool-handler batch is DONE** (2026-07-04).
The nine immediately-portable tools + the real dispatching `ToolRunner` are
ported, each handler with a differential driving v4's REAL handler byte-exact
(five agents on disjoint files, the shared `loaded_memories` typing + module
skeleton + dispatcher wired serially). Handlers live in the `tools::` family:
`read_conversation` + `upsert_annotation`/`delete_annotation`
(`tools::read_conversation`/`tools::annotations`, over the ported
`conversation_annotations` repo — extended with `find_by_chat_id`/
`find_by_message_index`/`delete_annotation` — and the ported
`crate::scriptorium::{merge,strip}_annotations` leaves;
`scriptorium_tools_equivalence`); `terminal_read`/`terminal_list`
(`tools::terminal`, over new `terminal_sessions` `find_by_id`/`find_by_chat_id`
reads + the ported `crate::terminal_clean::clean_terminal_output`, the live-PTY /
transcript scrollback lifted to an **injected seam** — `full_content` fed
identically both sides; `terminal_tools_equivalence`); `whisper` (`tools::whisper`,
resolves the target by name/alias among `can_receive_whisper` participants and
writes exactly one `chat_messages` row through `add_message` — STOP-rule checked,
**no post-office side effect**; reuses the ported `strip_text_block_markers`;
`whisper_tool_equivalence`); `help_settings`/`help_navigate`/`submit_final_response`
(`tools::help` — `help_settings` needed and got the full
`chat_settings::find_by_user_id` net-read marshaling, the other four profile reads
being sanitizer-subset scoped SELECTs; the last two pure; `help_tools_equivalence`);
and the capstone `self_inventory` (`tools::self_inventory`, the ten-section
introspection report composing ~a dozen repo readers [`llm_logs` `find_last_by_chat_id`,
`doc_mount_files` `find_vault_files_by_mount_point_id`, `group_character_members`
reads, `chat_documents::find_by_chat_id`, `characters_read::find_all_raw`, …] +
the ported `build_system_prompt` / `resolve_connection_profile` /
`get_model_context_limit` + new `crate::folder_utils` leaves +
`qtap_uri::format_scoped_uri`; the host-environment bits — runtime mode, client
shell, release-notes/changelog file reads, `isMountIndexDegraded` — are an
**injected `SelfInventoryEnv` seam**; `self_inventory_equivalence`). The gate's
`LoadedMemoriesContext` is now **typed** (`{ semantic, interCharacter, recap }`)
since its consumer landed. The **dispatching runner**
(`tools::executor::BuiltInToolRunner`) reproduces v4 `executeToolCallWithContext`'s
built-in dispatch rows (the `{ formattedText, … }` result shape, the failure
`null`/`error` mapping, the dispatcher-side guards + the annotation
character-name resolution) and holds an **injected inner `ToolRunner` fallback**
for unported names (the loud default reproduces v4's `Unknown tool: <name>` for
names v4 doesn't know, and a "recognized but not yet available" failure naming a
not-yet-ported built-in — batches 2–5 extend the ported set without touching
callers). An end-to-end dispatcher differential (`tool_dispatch_equivalence`)
drives v4's REAL `executeToolCallWithContext` over a mixed batch (read, two writes
with character-name resolution, a pure tool, a handler failure, an invalid-input
failure); the unknown-tool loud fallback is unit-tested (v4's genuine unknown path
routes through the unported plugin registry, so it stays out of the oracle batch).
Existing `tool_execution_*` + `message_finalizer` + `orchestrator` differentials
re-verified green. **Tracked deferrals:** the plugin-vs-built-in routing
precedence (the plugin registry is unported — for a no-plugin instance the
dispatch is exact); `self_inventory`'s `quilltap.releaseNotes`/`.changelog` file
reads (the env seam supports them; the corpus requests only `quilltap.version`);
the `terminal_read` scrollback source; `read_conversation`'s deprecated
`findByCharacterIdRaw`.

**Wave 4 (W4.1e): the native tool loop + the finalizer response-RNG are DONE**
(2026-07-04). Two sub-units. **e.1** — `services::native_tool_loop`
(`native_tool_loop_tier3_equivalence`): v4's `runNativeToolLoop`, the bounded
stream → detect → execute → thread → re-stream loop the orchestrator runs after
the primary stream. Composes the landed pieces — `process_tool_calls` +
`ToolRunner` (c), `build_assistant_tool_call_message`/`build_tool_result_messages`
(b.2), the streaming provider + canned-key machinery, `apply_reasoning_chunk`/
`flush_reasoning_segment`/`next_turn_seq` (primary-stream) — with two new seams: a
`ToolCallDetector` (v4 `detectToolCallsInResponse`, provider wire parse deferred to
W4.7) and the frozen `ToolRunner` (W4.1d), both injected. Reproduces the iteration
control (max `agentMode.enabled ? maxTurns : 5`), the batch anchor/seq stamping
(UTF-16 `fullResponse.length` + the shared `nextTurnSeq`), the agent-mode
`submit_final_response` accept (siblings-first, `args.response`-replace-vs-preserve
on `realWorkIterations`, the reasoning/anchor drop on replace), the iteration-0
ghost-wrap guardrail, the output-token truncation guard (real `extractFinishReason`
→ per-call recoverable-failure results), the threaded-slate re-stream, and the
max-turns force-final pass (the forced `[assistant, user-nudge]` + the
submit-in-response promotion). The partial agent-mode module
(`services::agent_mode`) ports the pure helpers the loop consumes
(`buildForceFinalMessage` / `generateIterationSummary` /
`extractSubmitFinalResponseFromText` + `ResolvedAgentMode` as an input struct — the
resolver cascade is W4.4). The loop's ONLY DB write is the agent-mode
`repos.chats.update({agentTurnCount})` bump (new `ChatUpdate.agent_turn_count`
setter; no `updatedAt` mint). Wired into the orchestrator spine at v4's
composition point (corpus-dormant: `buildTools` is W4.1g so the tool slate is
empty AND the primary stream leaves `raw_response` null → the loop breaks
immediately; `orchestrator_tier3` re-verified green). The tier-3 differential
drives v4's REAL `runNativeToolLoop` with a three-boundary mock split (canned
streams via recorded keys / canned detection by raw-response marker / canned tool
results — mirroring the Rust seams) over seven case families (callId
single-iteration → continuation [native threading], no-callId text fallback,
truncation-guard reject, agent submit real-work-replace + no-work-preserve +
ghost-wrap reject, multi-iteration → force-final with the submit promotion),
diffing the ordered event trace + result state (`fullResponse`/`toolMessages`
[name/success/content/callId/anchor/seq]) + the `chats` `agentTurnCount` dump.
**e.2** — the finalizer's assistant-response RNG is now REAL (a.3's inline-code
precedent): the ported `rng_patterns` detector + `tools::rng` executor run against
the cleaned response, write a `TOOL` row per pattern (the `auto-detect-response`
content shape — `initiatedBy:'auto-detect-response'` + a UTF-16-`indexOf`
`anchorOffset` placing the result after the notation, DISTINCT from the
user-message `auto-detect` shape), and push onto `toolMessages` so the done event's
`toolsExecuted` reflects it. The `RngDetector` seam is CLOSED — only the CSPRNG byte
source is injected (`RandomBytes`, extending the finalizer's inputs the way
`OrchestratorDeps::rng_bytes` did; the orchestrator now shares ONE `rng_bytes`
across the user-message + assistant-response auto-detect, dropping the
`finalizer_rng` generic). Added `jsstr::js_index_of` (UTF-16 `indexOf`). The
message-finalizer differential gained a fire case (`2d6` → committed bytes → the
byte-exact `Rolled 2d6: [3, 5] = **8** total` TOOL row + `toolsExecuted:true`) and
a no-fire case, its oracle un-stubbing `detectAndConvertRngPatterns` and mocking
`crypto.randomBytes` to the committed sequence (the crypto-mock gotcha:
spread-real-override-only). **Tracked deferrals:** the agent-mode resolver cascade
(W4.4), `detectToolCallsInResponse`'s provider wire parse (W4.7), the text tool
loop (W4.1f), `buildTools` + the spine's real tool slate + runner wiring (W4.1g).

**Wave 4 (W4.1d batch 2): the seven wardrobe tool handlers are DONE**
(2026-07-04). `tools::{wardrobe_list, wardrobe_read, wardrobe_create,
wardrobe_update, wardrobe_archive, wardrobe_wear, wardrobe_take_off}` (v4
`lib/tools/handlers/wardrobe-*-handler.ts`), each byte-exact against v4's REAL
handler. They compose the already-ported vault-public CRUD
(`db::vault_wardrobe_public`, incl. the `WardrobeLocation` character/General/
project generalization), the public READ trio + shared-archetype tier
(`db::wardrobe_read`, `db::archetype_wardrobe`), and the equipped-outfit ops
(`db::chats_outfits`), over new pure leaves in `crate::wardrobe` (`unionTypes`,
`describeOutfit`, `expandComposites`, the flag-driven `wearItemIntoSlots`/
`replaceItemIntoSlots`, `describeWardrobeEffect`, `normalizeNoItemSentinel`) and
the DB-touching `tools::wardrobe_shared` (`resolveWardrobeItemAcrossTiers`, the
persisted equip primitives `equipItem`/`replaceItem`/`addToSlot`/`removeFromSlot`
over `ChatOutfitsRepository`, `resolveEquippedOutfitForCharacter`, the coverage
summary, and `resolveProjectMountPointIdsForChat`), plus a new
`wardrobe_read::find_by_ids_for_character`. `BuiltInToolRunner` gained the seven
dispatch rows — each runs inside a single `Db::write` closure that hands the sync
handler BOTH the main + mount-index writer connections (the `wardrobe_transfers`
precedent), and `wardrobe_{archive,wear,take_off}` return a pending-announcement id
list folded into the per-turn set inside the closure. **The
`pendingWardrobeAnnouncements` shape (the flagged wrinkle):** the field became
`Arc<Mutex<HashSet<String>>>` — interior mutability so the handlers record an
announcement through the immutable `ToolRunner::run` boundary WITHOUT changing the
trait signature (W4.1e consumes that trait); cloning the context shares the set
(v4's shared-Set semantics). The end-of-turn DRAIN (Aurora posting) stays a
documented deferral; the legacy "no set → enqueue immediately" fallback is not
ported (the ported context always carries a set). Avatar generation on equip is an
**image-subsystem seam** (out of scope): the corpus keeps `avatarGenerationEnabled`
false so v4's `triggerAvatarGenerationIfEnabled` is a no-op, matching the port
(which omits it). Verified by `wardrobe_tools_equivalence`: a 25-op sequence over a
two-DB baked fixture (caller + recipient vaults, a General archetype, a chat with
participants + a seeded equipped outfit) driving v4's REAL handlers — success /
invalid-input / edge per handler (gift to a chat participant, composite+equip,
unknown component, shared read-only refusal, add-to-slot mismatch, archive) — with
per-op Output + `format*` string diffed and a final read-back of both wardrobes /
archetypes / equipped outfit; minted ids/timestamps are positionally normalized
(create mints an id + timestamps; update/archive mint `updatedAt` inside the
content-addressed `.md`), the underlying table bytes inherited from
`vault_wardrobe_public_equivalence` / `chats_outfits_tier2_equivalence`. The
dispatcher differential (`tool_dispatch_equivalence`) gained a `wardrobe_list`
call (real handler both sides); the existing `tool_execution_*` + `tool_dispatch`
differentials re-verified green after the announcements field-type change.
**Tracked deferrals:** the end-of-turn announcement drain (Aurora posting), the
General/project archetype write tiers beyond W4.0's, and `findByCharacterIdRaw`.

**Wave 4 (W4.1d batch 3a): the doc-edit foundation, part 1 — the tiered mount
pool + the `qtap://` URI codec — is DONE** (the first half of the batch-3
foundation the ~26 `doc_*` handlers sit on). The canonical `dedupeTierTriple` is
hoisted out of the knowledge injector into its true home
`db::tiered_mount_pool` (v4 `lib/mount-index/tiered-mount-pool.ts`), joined by the
ported `resolve_tiered_mount_pool` / `classify_mount_tier` / `flatten_tier_pool`;
the knowledge injector now consumes the dedup from there (differential re-verified
green). The five-tier resolution (character / participant / group / project /
global) reproduces the ownership gate (fails closed without `userId`), the
pre-resolved character-mount fast path (ignored under ownership), the
per-RESPONDING-character group tier (never a co-participant's groups), graceful
global-null, per-tier error swallowing, and the character>group>project>global
dedup — the resolver takes BOTH a main + mount-index `&Connection` (v4's
`getRepositories()` spans both DBs). Verified by a 9-case read-differential
(`tiered_mount_pool_equivalence`) driving v4's REAL `resolveTieredMountPool` over
a two-DB fixture (2 characters + minted vaults, a group G1 with an official +
linked store + charA membership, a project P1 with two stores + colliding links to
charA-vault/G1-official/General, the Quilltap General singleton in
`instance_settings`); the matrix banks the ownership pass/fail (incl. the subtle
case where a null character tier leaves its own vault in the project tier), the
fast path, the participant tier + self-exclusion + flag-off, and the per-character
group tier — zero normalization (every id pinned/shared). The full `qtap://` URI
codec (`doc_edit::qtap_uri`, v4 `qtap-uri.ts`) is ported and **unified** with the
producers previously hoisted into the knowledge injector (now re-exported from
this canonical home; `self_inventory` + the RAG renderer stay green):
`parse_qtap_uri` / `format_qtap_uri` / `is_qtap_uri` / `qtap_uri_to_resolver_input`
/ `QtapUriError` + `format_self_uri` / `format_scoped_uri` / `format_doc_store_uri`.
It reproduces JS `encodeURIComponent` / `decodeURIComponent` **exactly** (a
V8-faithful `Decode` — `%XX` runs assembled + validated as UTF-8 sequences, so a
malformed escape throws `MALFORMED` where V8 does), the last-`:` fragment split,
the 1–6 `BAD_LEVEL` bounds, the encoded-slash-inside-a-segment rule, non-ASCII
round-trips, and the **insertion-ordered** query map (`serde_json::Map` under
`preserve_order`). Verified by a 54-row tier-1 differential
(`qtap_uri_equivalence`) over parse (parts / thrown code + byte-exact message),
canonical re-emit, resolver triple, and every producer. Added the scoped
mount-point reads the resolver + URI producers need
(`doc_mount_points::{find_by_id_for_docedit, find_enabled_for_docedit,
count_by_name}`, `groups::find_official_mount_point_id_raw`). **Documented seam:**
`parseFragment`'s `parseInt` renders astronomically-long digit levels via JS float
(corpus keeps levels small); the query multi-key order is insertion-ordered but the
corpus stays single-key on that axis. **The batch-3a pure leaves are now also DONE**
(`doc_edit::{diacritics, mime_registry, unified_diff, markdown_parser}`, v4
`lib/doc-edit/{diacritics, mime-registry, unified-diff, markdown-parser}.ts`),
verified by one grouped tier-1 differential (`doc_edit_leaves_equivalence`, 81
rows): the NFD diacritics matcher (`unicode-normalization` added; NFD +
strip-combining + the `findAllMatches`/`findUniqueMatch` UTF-16 index/length remap —
proven byte-exact on precomposed/decomposed Latin + Hangul), the MIME registry
(`detectMimeFromExtension` / the `isJson*` predicates /
`parseContent`/`serializeContent`/`validateJson` — happy-path bytes byte-exact,
`serde_json` pretty == `JSON.stringify(x,null,2)`, with the V8 `JSON.parse` message
TEXT a **documented normalized seam**), the hand-rolled unified diff (the greedy
look-ahead algorithm reproduced exactly), and the markdown heading ops
(`slugifyHeading` [ASCII `\w` + JS `\s`], `parseHeadingTree` [ATX + code-fence
exclusion + duplicate-slug suffixes + UTF-16 offsets], `findHeadingSection`
[byte-exact thrown messages], `readHeadingContent`/`replaceHeadingContent`) plus
`serializeFrontmatter`/`updateFrontmatterInContent` — the latter reusing the ported
eemeli scalar emitter (now `pub(crate)`) so `YAML.stringify` is byte-exact over the
frontmatter value space (string/bool/number/null scalars + flat sequences; nested
maps / exotic numbers / non-identifier keys a documented seam). The v4
`document-policy.ts` needed no new port (its leaves already live in
`db::doc_mount_file_links`). **The batch-3a path resolver + URI producers are now
also DONE, completing batch 3a** (`doc_edit::{path_resolver, uri_producers}`, v4
`lib/doc-edit/{path-resolver, uri-producers}.ts`), verified by a 23-case
read-differential (`doc_edit_path_resolver_equivalence`) driving v4's REAL
`resolveDocEditPath` + `docStoreUriFor`/`uriForResolvedPath`/
`buildDocStoreUriResolver`. The `document_store` scope resolves over the tiered
mount pool (the SELF token, name-vs-id matching, ambiguity/not-found/disabled
errors, traversal/absolute/missing-path guards) and the `project` scope aliases the
official mount — all with byte-exact `PathResolutionError` codes + messages. The
legacy on-disk branches (a `filesystem`/`obsidian` mount's real path, the project
legacy `<filesDir>` fallback, the entire `general` scope) are a **host-filesystem
seam** deferred to the Phase-4 host (`ResolveError::FsSeam`); every corpus store is
database-backed so v4 returns `absolutePath:''` early and the seam is never hit. The
URI producers (`docStoreUriFor` / `uriForResolvedPath` / `buildDocStoreUriResolver`)
ride the ported qtap producers + `doc_mount_points::{count_by_name,
find_enabled_for_docedit}`. Added `projects::find_official_mount_point_id_raw` (the
slim pointer the project-alias reads; v4 uses the overlaid `projects.findById`,
whose throw-on-corrupt-store edge the raw read treats as a normal resolve — a
documented minor seam, never hit with a real provisioned store). **The doc-edit
foundation (W4.1d batch 3a) is complete** — the ~26 `doc_*` tool handlers (batch 3b)
sit on it.

**Wave 4 (W4.1f): the text-tool loop is DONE** (2026-07-04). Ported
`runTextToolPass` (`services::text_tool_loop`): the strategy-driven
detect-text-markers → execute → re-stream-continuation pass the orchestrator runs
after the native loop. Where the native loop reads provider-native calls off the
raw response (W4.7 wire parse), this pass reads text markers OUT OF the streamed
prose. The engine is **strategy-agnostic** behind a `TextToolStrategy` trait
(`has_markers`/`parse`/`strip`/`format_tool_result`/`stop_sequences`) — ships
`SimpleJsonStrategy` + `TextBlockStrategy` (composed from the b.1 leaves), and takes
a **provider-text-markers strategy as an injected seam** (the provider plugin's
detector/parser/stripper is unported W4.7; default `None`). Reproduces v4's flow
exactly: the entry gate, the per-iteration parse + call-signature fingerprint, the
`MAX_DUPLICATE_TOOL_CALLS` **nudge branch** (do-not-execute + the byte-exact
synthetic user nudge, the em-dash included — its byte-exactness enforced by the
continuation canned-key match, since the nudge rides the ledger into the slate),
the `MAX_TEXT_TOOL_ITERATIONS` cap, the **un-stripped-assistant-turn ledger**
(markers kept on purpose — stripping broke simple-json continuations), the
DISPLAY-ONLY flat reasoning on the continuation (no positioned segments), the
`usage`/`cache_usage`/`raw_response`/`thought_signature` **overwrite-on-done** (a
done with no usage NULLs them), the caller-owned in-place mutation, the
preserve-partial + rethrow on continuation failure, and `assembleStrippedWithOffsets`
(strip once per raw segment, keep non-whitespace, `\n\n`-join, and stamp each
batch's tool messages with the UTF-16 prose offset where its emitting segment ends
— a whitespace-only segment is dropped and its end offset inherits). Wired into the
orchestrator spine at v4's composition point (after the native loop): the provider
pass gated on the injected `provider_text_strategy` seam, then simple-json vs the
text-block fall-through per an injected `resolved_tool_mode` (defaulting to
`TextBlock` — v4's else-branch; the real tool-config plumbing + `buildTools` slate
is a W4.1g deferral). Corpus-dormant (no canned stream emits markers, empty tool
slate); `orchestrator_tier3_equivalence` re-verified green with the two new
`OrchestratorDeps` fields inert. Verified by `text_tool_loop_tier3_equivalence`
(nine case families, **DB-free** — the pass writes nothing, so no fixture): the
three-boundary mock split (streams by recorded canned key / tools by canned per-call
/ the strategy) drives v4's REAL `runTextToolPass` — simple-json single-iteration +
text-block multi-call over the REAL ported strategy functions, and a synthetic
`<<T:name:argsJson>>` strategy (trivially identical in TS + Rust) for
multi-iteration (two anchors + `\n\n` math), the duplicate nudge, the parse-empty
no-op, a mid-continuation stream failure (partial preserved, error propagates), the
iteration cap, empty-stripped-segment assembly (surrogate-pair UTF-16), and
stopSequences forwarding (per-continuation `stop` recorded + diffed). **Tracked
deferrals:** the provider-text-markers strategy (→ W4.7 provider manifest); the
spine's real tool-mode/tool-slate plumbing (→ W4.1g).

**Wave 4 (W4.1d batch 3b) is DONE — the entire doc-edit tool subsystem except
the photo trio is ported, green, and dispatched.** `db::database_store` ports v4's
`lib/mount-index/database-store.ts` (read/write/move/delete documents, folder
create/delete/move, existence checks) by composing the ported storage leaves,
adding the repo finders it needs (`doc_mount_folders` /
`doc_mount_file_links` find-by-path/by-mount + a `LinkRow` join + a REAL-affinity
`chunkCount`/`fileSizeBytes` coercion fix that had been silently failing the
access gates). `tools::doc_edit::shared` ports the access-control family
(cross-character vault visibility, `systemTransparency` opacity, the
`character_read`/`character_write` gates, the folder-protected-descendants guard,
the read/write resolution-context builders, `getAccessibleMountPoints`,
`resolveOfficialProjectMount`) and the `applyQtapUri`/`isTextFile`/DB read-write
dispatch. The first eight handlers (`doc_read_file`/`doc_write_file`/
`doc_str_replace`/`doc_insert_text` + `doc_read_frontmatter`/
`doc_update_frontmatter`/`doc_read_heading`/`doc_update_heading`) run behind a
v4-faithful `executeDocEditTool` dispatcher; the Librarian-announcement + reindex
layers are documented **no-op seams** (mocked in the oracle, the wave-3
whisper-posting-seam precedent). Added a `documentMode` `ChatUpdate` setter.
Verified by `doc_text_equivalence` — a **jest-real-DB** differential driving v4's
REAL `executeDocEditTool` + `formatDocEditResults` over a 26-op corpus (line/
offset/JSON reads, self + project + `qtap://` addressing, blocked read +
read-only write, str_replace unique/not-found/multiple/diacritics, insert
start/end/before, frontmatter read/keys/none/merge/replace, heading
read/not-found/update) plus a two-table dump; the write ops' minted `mtime` is
placeholdered, read `mtime` diffed exactly. The remaining handler groups then
landed the same way — **file-management** (`doc_move_file`/`doc_copy_file`/
`doc_delete_file`/`doc_create_folder`/`doc_delete_folder`/`doc_move_folder` over
the `database_store` primitives; the `chat_documents` move-sync a corpus-verified
no-op seam; `doc_fm_equivalence`, 20 ops), **document-UI** (`doc_open_document`/
`doc_close_document`/`doc_focus` + three new `chat_documents` scoped ops
[`find_open_for_chat`/`open_document`/`close_document_by_id`] + the `documentMode`
`ChatUpdate` setter that does NOT bump `updatedAt`; the `doc_focus`
no-`formattedText` path builds its result map in v4 key order for the
`JSON.stringify` fallback; the new-blank-doc `fs.writeFile` path is a tracked
FsSeam; `doc_ui_equivalence`, 9 ops), **blob** (`doc_write_blob`/`doc_read_blob`/
`doc_list_blobs`/`doc_delete_blob` over the newly-ported `link_blob_content`
binary storage primitive [the binary analogue of `link_document_content`, closing
that long-standing deferral] + the blob-repo `create`/find/list/read/delete; the
WebP `transcodeToWebP` is a native passthrough seam identical on both sides;
`doc_blob_equivalence`, 11 ops), and **enumeration** (`doc_grep`/`doc_list_files`
over a new `doc_mount_documents::find_all_by_mount_point_id` + `list_database_files`
+ `get_accessible_mount_points`; `is_regex` uses the `regex` crate [JS-only regex
features a documented seam], the default diacritics path byte-faithful; the fs/
general branches the FsSeam; `doc_enum_equivalence`, 14 ops). All 23 non-photo
`doc_*` tools are wired into `BuiltInToolRunner` (one `run_doc_edit` dispatch
through `execute_doc_edit_tool` inside a both-connections `Db::write` closure,
building v4's `{ formattedText, ...result }` row), with
`tool_dispatch_equivalence` extended by two doc-edit rows (`doc_read_file` +
`doc_read_frontmatter` on a transparent character's own vault) and re-verified
green. **The photo group (`keep_image`/`list_images`/`attach_image`) is a tracked
scoped deferral** — it drags in the unported images-v2 store +
`keep-image-markdown` sidecar builder + `chunkAndInsertExtractedText`, beyond the
named byte-source seam — so it stays out of `PORTED_TOOLS` and routes to the loud
fallback. **Tracked deferrals across 3b:** the photo group, the converted-blob
`extractedText` read branch (`doc_read_file` non-text path), the new-blank
`doc_open_document` fs path, the fs/obsidian/general mount branches (FsSeam), and
`is_regex` grep's JS-regex parity.

**Wave 4 (W4.1d batch 4): the four search/introspection tool handlers are DONE**
(2026-07-04). Ported `search` / `project_info` / `help_search` /
`request_full_context`, each byte-exact against v4's REAL handler and wired into
`BuiltInToolRunner`. **`search`** (`tools::search`, v4
`search-scriptorium-handler`) is the Scriptorium unified search over four sources
— memories (the ported `search_memories_semantic`, `now_ms` injected;
`recallContext` is off for this tool so its deferred re-rank stays unexercised),
conversations (the new `db::conversation_search` = v4 `searchConversationChunks`, a
faithful sibling of `document_search` over `conversation_chunks` BLOB embeddings —
NOT merged, v4 keeps them separate), documents (`document_search`), and knowledge
(the same document search narrowed per tier to `Knowledge/` with the tier-specific
literal boosts) — reproducing the per-source error-swallowing branches, the
tier-ordered dedup (character > group > project > global; the knowledge-labeled row
wins a chunk shared with the deferred document rows), the `qtap://` URI tagging via
`DocStoreUriResolver`, the operator/Brahma surface (memory forced off,
operator-wide stores + operator-wide conversations by userId), the 500-char UTF-16
content truncation, and the exact result-strings/labels (`(score*100).toFixed(0)%`
via the ported `to_fixed`). Serves BOTH the standard `search` and the Brahma
definitions (the handler validates with the full schema always; operator surface is
the switch). **NOT purely read-only** — the memory branch bumps `lastAccessedAt`
(v4 `updateAccessTimeBulk`). **`project_info`** (`tools::project_info`) does
`get_info` (roster + item counts + the linked store summary via the new pure leaf
`db::project_store_naming::pick_primary_project_store` = v4 `pickPrimaryProjectStore`,
tier-1 unit-tested) and `get_instructions`. **`help_search`** (`tools::help_search`
+ new `db::help_search`) is semantic search over `help_docs` embeddings with the
automatic keyword fallback on an embedding failure (the `extract_search_terms`
keyword extractor added to `embedding_vector`, JS `\w`/`\s`/stop-word faithful);
v4's `ensureHelpDocsSynced` disk index-build is a **documented host seam** (a no-op
once `help_docs` has rows — the tool path only reads stored embeddings, the
knowledge-fixture precedent). **`request_full_context`** (`tools::request_full_context`)
flips the chat's `requestFullContextOnNextMessage` flag; ported as a self-contained
single-column `UPDATE` (byte-identical to v4's `repos.chats.update`, which does not
mint `updatedAt`) so it needs **no `db/chats.rs` change** (that file is owned by the
parallel doc-edit-handler unit this round). The dispatcher now carries an
injectable `ErasedEmbeddingProvider` (default a never-succeeds `NoEmbeddingProvider`
— faithful to "no embedding profile": the search branches degrade, `help_search`
falls back to keyword) so `search`/`help_search` reach the embedding seam without a
second generic on the shared `BuiltInToolRunner`; a real provider wires with W4.1g.
New additive read helpers: `conversation_chunks::find_all_with_embeddings`,
`help_docs::find_all`/`find_all_with_embeddings`, `doc_mount_blobs::count_by_mount_point`,
`files::count_by_project_id`, `doc_mount_points::find_store_naming_by_id`. Verified
by `search_tools_equivalence` — 24 cases across two jest real-DB oracles driving
v4's REAL handlers (only `generateEmbeddingForUser` mocked to canned dim-8 vectors,
`Date.now()` frozen to the pinned `now_ms`), each case on a fresh two-DB fixture
copy (search bumps `lastAccessedAt`; request_full_context writes), comparing
serialized result JSON + `format*` output byte-for-byte (float-safe: identical f64
bits render identically under ryu + V8, js-number + preserve_order applied) and,
for request_full_context, the full `chats` row (flag flips to 1, `updatedAt` +
every other column preserved). `knowledge_injector` / `first_message_context` /
`tool_execution_process_tier3` re-verified green (the `document_search` module was
made `pub` + one read added — no behavior change). **Tracked deferrals:** the
provider wiring (W4.1g); `search`'s operator-surface `run_sql`-adjacent Brahma
gating stays as-is; `help_search`'s disk sync (host seam).

**Wave 4 (W4.1d batch 5, part 1): the `state` + `run_sql` handlers are DONE**
(2026-07-04). `state` (`tools::state`) ports v4's persistent per-chat/per-project
key-value store — the pure path helpers (`parsePath`/`getAtPath`/`setAtPath`/
`deleteAtPath`, undefined-vs-null distinguished, intermediate object/array
creation) + the `mergeState` spread (chat overrides project) + the fixed-field-
order output serializer reproducing every per-branch `JSON.stringify`; chat writes
ride `chats.update({state})` (no `updatedAt` mint), project writes the store-backed
`state.json` overlay. `run_sql` (`tools::run_sql`, Brahma read-only SQL) ports the
defense-in-depth read-only guard verbatim (literal/comment-stripping pre-scan +
forbidden-keyword/single-statement/mutating-PRAGMA checks, then rusqlite
`Statement::readonly` fail-closed, then the `max_rows` cap), the `<blob: N bytes>`
sanitize + `js_number_to_json` REAL rendering, and byte-identical SQLite error
strings (same SQLite3MC engine); the `operatorSurface` gate is the dispatcher
guard. Both are wired into `BuiltInToolRunner`. Verified by
`state_sql_tools_equivalence` (34 cases, one jest real-DB oracle over a fresh
three-DB fixture copy per case): state cases diff the serialized output +
`formatStateResults` + the `chats` dump (zero normalization) + a project-`state`
read-back (overlay bytes proven by `projects_tier2`); run_sql cases diff the
serialized envelope across each target DB, blob sanitize, truncation, and every
refusal. **Tracked deferrals:** `run_sql` Zod-validation-message fidelity beyond
the non-object case (the pre-scan/prepare failures cover the real refusals); the
`state` open-JSON multi-key insertion-order seam (corpus kept single-key/sorted).

**Wave 4 (W4.1d batch 5, part 2): the Post Office (`send_mail` / `list_email`) +
`ask_carina` are DONE** (2026-07-04). A new `crate::post_office` module ports v4's
`lib/post-office/` (mailbox storage: slugify/compose/parse/reply-preface +
`deliver_letter`/`read_letter`/`list_mailbox`; the shared `compose_and_deliver_letter`
delivery service; the agent-facing instruction snippets) over the ported vault
primitives — the delivery `sentAt` injected so it can be pinned. Added
`db::character_resolver` (`resolve_character_by_name_or_id`) and `crate::format_time`
(the UTC-pinned `formatDateTime` — v4's system-TZ `toLocaleDateString`, reproduced
in UTC; documented harness constraint). `send_mail`/`list_email` compose those over
both writer connections and are wired into `BuiltInToolRunner`. `ask_carina`
(`tools::ask_carina`) rides the existing `RunCarinaQuery` + `PostProsperoCarinaError`
seams from `services::carina_runner` — handler + differential done, but its dispatch
stays on the loud fallback until the W4.5 query engine is orchestrator-injected as
the seam (the `onPosted`/`emitCarinaAnswer` slot is the tool-context deferral).
Verified by `mail_carina_tools_equivalence`: the mail half (real-DB, delivery clock
pinned) drives v4's REAL handlers over a fresh two-DB fixture copy per scenario,
diffing serialized output + `format*` + the delivered letter content read back
byte-for-byte (send-then-list round-trip, reply preface, validation/refusal paths,
empty/single/plural listings); the carina half (DB-free) injects canned seams and
diffs output + `format*` + recorded Prospero args. **Tracked deferrals:** the
Suparṇā mail-check helpers (`collect_unalerted_mail`/`mark_alerted`); the ask_carina
dispatch seam wiring (W4.5).

**Wave 4 (W4.1d batch 5, part 3): the `search_web` handler is DONE** (2026-07-04).
`tools::web_search` ports v4's `search_web` with the whole search boundary (the
plugin `searchProviderRegistry` + API-key lookup + Serper fallback) behind the
injected `WebSearchProvider` seam; the portable half is validation + the byte-exact
outcome→output mapping (not-configured / missing-key / provider-failure error
strings) + the built-in formatter (`publishedDate` via a UTC-pinned
`toLocaleDateString()` in `format_time`). Wired into `BuiltInToolRunner` with a
default `NotConfiguredWebSearch` (faithful to a no-search-plugin instance). Verified
by `web_search_tool_equivalence` (DB-free, jest-mocked registry). **Deferrals:** the
provider's own `formatResults`, host-side API-key acquisition, date-only
`publishedDate`.

**Wave 4 (W4.1d batch 5, part 4): the `generate_image` pure leaves are DONE**
(2026-07-05), ported leaf-first ahead of the stateful handler. `crate::image_gen`
ports `resolveOrientation` (the pure `(provider, model, orientation)` →
request-mutation mapping — `matchModel` exact + longest-prefix, `realize`
strategy-honouring + degrade-to-hint, host fallback; the plugin-registry
declarations passed in as data) and `parsePlaceholders` (the `{{name}}` scanner,
name `.trim()`-ed). Verified by `image_gen_leaves_equivalence` (tier-1, DB-free)
driving v4's REAL functions with the registry jest-mocked. **Scoped deferral:** the
full `executeImageGenerationTool` handler + `saveGeneratedImage` persistence — they
compose the image-provider call + WebP + Lantern store/notification (host seams),
the W4.2 dangerous-content classify/route path (double profile reroute), and three
cheap-LLM tasks (`craftImagePrompt` / `resolveCharacterAppearances` /
`sanitizeAppearance`), several themselves large unported units; the handler lands
once those seams/subsystems exist.

**Wave 4 (W4.1g): `buildTools` + the tool-slate spine wiring is DONE — W4.1 is
CLOSED** (2026-07-04). Ported v4's `buildTools` + the built-in half of
`buildToolsForProvider` (`services::tool_build`): the flag→tool-set construction
over the b.3 catalog, `is_tool_disabled` (individual-id filter; plugin-group
patterns never bite a built-in's empty source metadata), the `allowToolUse ===
false` + `disabledTools === undefined` short-circuits (the latter returns EMPTY,
not all), `apply_image_constraints_to_tool` (pure, unit-tested), and the
canonical (universal/OpenAI) provider shape (v4's `getProvider===null` fallback —
the provider `formatTools` reshape is the W4.7 deferral). `checkModelSupportsTools`
+ `provider.supportsWebSearch` are **injected registry-seam inputs**
(`ProcessMessageInput::model_supports_native_tools` / `provider_supports_web_search`
+ the `BuildToolsInput` fields, the `getModelContextLimit` precedent). Added
`plugin_config::find_by_user_id` — read for faithfulness (v4's DB access) but
**unused for the built-in slate** (plugin tools deferred). Ported the orchestrator
flag region (`orchestrator.service.ts:758–905`): `helpToolsEnabled` /
`canDressThemselves` (`!== false`) / `canCreateOutfits` / `documentEditingEnabled`
(mount-index read), `characterIsTransparent` + the `self_inventory` strip into
`effectiveDisabledTools`, the overlay-free `askCarinaEnabled` probe (`findAllRaw` +
`canBeCarina` OR transparent fallback, error-swallowed), the autonomous-room
destructive-tool filter (`DESTRUCTIVE_TOOL_NAMES` + the `destructiveToolPolicy`
CEILING × `runDestructiveToolsAllowed === 1` raw-int read), `checkResolvedToolMode`
→ `useTextBlockTools` → `actualTools` (`[]` under any pseudo-tool surface), the
mode-switched `toolInstructions`, and the simple-json `initialStopSequences`.
**Closed the spine seams**: the real slate flows to the primary stream (`tools` +
`useNativeWebSearch` + `stop`), the native loop (the real `BuiltInToolRunner` +
the injected W4.7 `ToolCallDetector` — production `NoToolCallDetector`; the
`self_inventory` host env + `search` embedding provider are the standing host
seams), and the text-tool passes' `continuationTools` (`useTextBlockTools ? [] :
actualTools`); the finalizer + tool-only terminal now receive the real tool
messages/images. Verified by a new **`tool_build_equivalence`** differential (27
flag-matrix cases driving v4's REAL `buildTools`, byte-exact slate + both
capability flags, incl. the Brahma workspace-strip / memory-search / sqlAccess
variants + the destructive filter) and the rebuilt **`orchestrator_tier3`** (18
cases running the REAL `buildTools` + flag region; a per-call **tools-at-wire**
assertion proves the exact slate reaches the provider on every case — 18/20-tool
variants; new cases bank the `self_inventory` transparent-vs-not strip + the
`ask_carina` transparency probe [via the existing `opaque_swap`/`transparent_no_swap`
chats], `disabled_tools` filtering [rng/state/terminal_read removed at the wire],
and `textblock_mode` [empty slate + simple-json instructions]).
`native_tool_loop` / `text_tool_loop` / `message_finalizer` / `primary_stream`
differentials re-verified green. **Tracked deferrals:** the plugin tool registry +
the provider `formatTools` reshape + image-provider constraint enrichment (W4.7);
a native tool CALL end-to-end THROUGH the orchestrator spine is proven in
composition by the tools-at-wire proof (slate/runner/detector reach the loop) +
the standalone `native_tool_loop_tier3` (drives v4's REAL loop) — the detector /
`raw_response` plumbing is wired but the corpus carries no native call (the
multi-character tool-call re-threading is that unit's concern).

**Drift check (2026-07-05): v4 `f69200bb..42242a3e` (5 commits) audited — no
ported unit is stale.** Every round-1/round-2 (W4.1d3–g) oracle was generated
against v4 HEAD `42242a3e` and diffed green, which is itself the strongest form
of the check. Classification: the standalone Document Mode family
(`d973a849`/`2416345a`/`d3e47672`) is Phase-4 surface (a new
`lib/documents/operator-doc-actions.ts` + the workspace/tab lib + API routes)
plus one additive constant (`STANDALONE_CHAT_ID`, a sentinel `chat_documents`
chatId for chat-less opens) — NOTE it gives the d3b-deferred
`chatDocuments.renameFilePathInStore` move-sync seam a second consumer;
`42242a3e` adds an additive `chats.getLastPlayedMessageAt` read (participant/
user-authored `type:'message'` with `systemSender IS NULL`) consumed only by the
unported stale-chat maintenance sweep (`lib/maintenance/
collapse-stale-chat-assets.ts`) — port together; `2a0360ac` is a v4-side
test-loader fix (its jest `moduleNameMapper` mock-vs-real-binding issue — our
oracles' abs-path requires already bypass it). The `docs/v4/` mirror was
refreshed (CHANGELOG, API.md).

**Wave 4 (W4.4a): the chat-message service batch is in progress.** Part 1 — the
**agent-mode resolver** — is ported and green (`services::agent_mode`, verified
through `orchestrator_tier3_equivalence`). v4's `resolveAgentModeSetting` (the
Global → Character → Project → Chat cascade), `DEFAULT_AGENT_MODE_SETTINGS`, and
`buildAgentModeInstructions` are ported, closing the orchestrator's agent-mode
seam: the spine reads the project's `defaultAgentModeEnabled` (a store-managed
field → the overlaid `projects.findById` read), resolves the cascade, fires the
`agentTurnCount: 0` reset on a non-continue turn (v4
`repos.chats.update({ agentTurnCount: 0 })`, no `updatedAt` bump), feeds
`agentMode.enabled` to `buildTools` (adding `submit_final_response`), injects the
byte-exact agent-mode system-prompt block into `formattedMessages` at the
first-non-system position, and passes the resolved `ResolvedAgentMode` to the
native loop. The orchestrator tier-3 corpus gained an `agent_mode_on` case
(chat-level opt-in via `agentModeEnabled`, custom `maxTurns: 15` via
`agentModeSettings`) banking the instruction injection (the recorded stream key
proves byte-exactness), the `submit_final_response` slate addition at the wire,
and the reset (seeded `agentTurnCount: 5` → 0); resolver unit tests cover the
full cascade matrix. Part 2 — **regenerate-swipe** — is also ported and green
(`services::regenerate_swipe`, `regenerate_swipe_tier3_equivalence`): v4's
`regenerateMessageAsSwipe`, the sibling entry point to `process_message`.
Composes the ported responder/identity resolvers + `build_message_context`
(continue-mode, context = everything strictly before the target) + the
`CompletionProvider` seam (a single non-streaming generation) + the swipe-group
bookkeeping (write back the original's `swipeGroupId` on first regeneration; the
new swipe shares the original's `createdAt` + participant, index = max group
index + 1) + the ported `delete_memories_by_source_message_with_vectors` cascade
(gated by the per-user `memoryCascadePreferences.onSwipeRegenerate`). The
orchestrator's `build_context_input` / `BuildContextArgs` were made reusable
(scalar clock/model-limit fields, not `&ProcessMessageInput`). The four-case
differential (first regen, existing group, KEEP_MEMORIES, not-assistant throw)
drives v4's REAL repo over a two-DB fixture and diffs `chats` / `chat_messages` /
`memories` / `vector_indices` / `vector_entries` (the canned completion key proves
the rebuilt continue-mode prompt). **Tracked deferral:** the swipe's
`rawResponse` / `reasoningContent` / `thoughtSignature` are null (the cheap-LLM
`CompletionResponse` subset carries none; the richer wire-decoded response is
W4.7). **New oracle gotcha:** a jest-real-DB oracle that exercises the vector
store must `doMock('@/lib/embedding/vector-store', requireActual)` — jest.setup
mocks it globally, so without the un-mock the cascade's store `load()` is silently
empty and no vectors are removed (see `[[jest-real-db-oracle]]`).

Part 3 — the **compression cache service** — is also ported and green
(`services::compression_cache`, `compression_cache_tier3_equivalence`): v4's
`triggerAsyncCompression` / `getCachedCompression` / `invalidateCompressionCache`
(+ `hashString` / `isCacheValid` / `cacheKey` / the persist/load/clear DB layer).
The durable cache lives in the `chats.compressionCache` column (a JSON object,
per-participant in multi-char chats; added the `ChatUpdate.compression_cache`
update setter — a JSON `null` clears to SQL NULL, no `updatedAt` bump); a
process-global in-memory map is the fast path. `withPersistLock` is not ported
(the single-writer task already serializes the load-modify-save); there is no
in-flight-promise state (the trigger computes synchronously within its async fn),
so `isFallback` is always false. The five-op differential (trigger→persist,
guard, get-DB-hit, get-miss, invalidate) drives v4's REAL functions, diffing the
persisted column (minted `createdAt` normalized) + the `getCachedCompression`
return. **Remaining W4.4a plumbing (tracked deferral):** the finalizer's
`AsyncCompressionTrigger` real production impl (the trigger inputs — messages /
systemPrompt / options — must thread through the finalizer's `CompressionContext`)
and the `buildContext` cached-compression window (the `cachedCompressionResult` /
`cachedCompressionMessageCount` inputs, computed by the spine via
`getCachedCompression`) are additive spine plumbing; the finalizer / build_context
differentials keep the recording / empty-cache seams until then.

**Wave 4 (W4.2): the dangerous-content ("Concierge") orchestration subsystem is
DONE** (`services::dangerous_content`), replacing the injected
`DangerousContentRouter` stub with the real resolution. Ported v4's
`lib/services/dangerous-content/` + the `CHAT_DANGER_CLASSIFICATION` job runner,
leaf-to-root: `chat_override` (the two-field `isConciergeOffDuty` /
`getConciergeState` / `isChatActiveDangerous` derivation — off-duty preserves the
label and wins over the classification), `resolver`
(`resolveDangerousContentSettings` — global + per-chat off-duty / moderation-exempt
short-circuits + the DEFAULT/OFF_DUTY constants), `gatekeeper` (content
classification: the moderation-provider path behind an injected `ModerationProvider`
seam that collapses v4's unported plugin registry + `autoDetectModerationApiKey` +
`provider.moderate` — the port still runs the ported `mapModerationResult` over the
raw result so the score/category math is verified; the cheap-LLM classify path with
the byte-exact `CLASSIFICATION_SYSTEM_PROMPT` in a generated `prompt_text` submodule
over the `CompletionProvider` seam; `parseClassificationResponse`; `CATEGORY_LABELS`
/ `MODERATION_CATEGORY_MAP`; the module-global classification LRU cache),
`provider_routing` (the REAL implementor of the FROZEN
`services::provider_failover::DangerousContentRouter` trait — the trait shape
expresses the resolution fine: `DangerSettings` + `EffectiveProfile` + `userId` is
sufficient, and the reason strings the failover doesn't consume are dropped;
`resolveProviderForDangerousContent` + `resolveImageProviderForDangerousContent` +
`resolveUncensoredImageProfileForReroute` + `isImageModerationError`, the connection
resolution ported, the API-key material an injected `ApiKeyResolver` seam per the
`cheap_llm_exec` precedent), `manual_flip` (`applyConciergeFlip` — the tri-state
operator flip written via a raw multi-column chat `UPDATE` that mints no `updatedAt`,
byte-identical to v4's `chats.update`, because the frozen `ChatUpdate` in
`db/chats.rs` is owned by the parallel W4.4a batch — the
`[[standalone-write-avoids-frozen-chatupdate]]` pattern generalized to the danger
column set), and `gatekeeper_job` (`handleChatDangerClassification` — the
sticky/exempt/off-duty/mode-OFF bails, the context-summary-else-concatenated-messages
input, the cheap-LLM selection via the ported `get_cheap_llm_provider`, the classify
call, the `DANGER_CLASSIFICATION` system event + token aggregate on the LLM path
only [which mints `updatedAt`], and the chat-level danger-field persistence via a
raw `UPDATE`). Added additive net reads: `connection_profiles::{find_all,
find_by_user_id}` + `image_profiles::{find_by_id, find_all}`. Verified by THREE
differentials, all green against v4 HEAD: **`danger_resolver_equivalence`** (tier-1
pure resolver + override matrix via a tsx oracle, PLUS a tier-2 jest-real-DB
manual-flip chat-row dump — sentinel-aware `dangerClassifiedAt`, `updatedAt` diffed
exact to prove no bump), **`danger_routing_equivalence`** (the reroute matrix over a
baked `connection_profiles`/`image_profiles` fixture — `rerouted` + profile identity
+ resolved key + the exact reason string, the API key a canned seam both sides [the
oracle monkey-patches `findApiKeyByIdAndUserId`, the port injects the same
`apiKeyId`→key map]; text + image + post-hoc reroute + `isImageModerationError`),
and **`danger_gatekeeper_tier3_equivalence`** (drives v4's REAL
`handleChatDangerClassification` over a seeded fixture with BOTH model boundaries
canned — safe/dangerous/borderline/parse-failure LLM classifications [completions
pinned by oracle-recorded canned keys], the moderation-provider path incl. a
provider failure, all the skip branches — diffing `chats` + `chat_messages`
sentinel-aware: `updatedAt`/`dangerClassifiedAt` stay at the `2020` seed on the
moderation/skip paths [no bump] and placeholder to `<ts>` when the LLM path's token
aggregate mints; the minted system-event id/createdAt placeholdered). **Tracked
deferrals (seams):** the moderation plugin registry, the cheap-LLM / routing
API-key acquisition, `logLLMCall`, the job-runner infrastructure
(`ensureProcessorRunning`), and the Concierge personified-announcement writers
(`postConcierge{Manual,Danger}Announcement` — seamed no-op, a W4.6 deferral). The
gatekeeper's raw-message-concatenation truncation uses Unicode-scalar (not UTF-16)
boundaries — a minor seam, corpus kept ASCII on that path; the message-fallback and
mode-OFF (userB) branches are covered. **The unification is now DONE (W4.2u,
2026-07-06):** the real `DangerContentRouter` + resolver are wired into the
`process_message` spine, replacing the injected `NoRouter` / hardcoded
`DETECT_ONLY` stub. The spine resolves the effective danger settings
(`resolve_dangerous_content_settings` over the global sub-object + the chat's
`conciergeOverride`/`chatType` — off-duty/exempt collapse to OFF), computes
`is_chat_active_dangerous`, and reproduces v4 `resolveMessageDangerState`'s FIRST
branch: an actively-dangerous, non-continue turn with content synthesizes danger
flags (attached to the saved user message via `updateMessage`) and — under
AUTO_ROUTE with a non-`isDangerousCompatible` profile — reroutes the primary
stream through an uncensored provider via the real router (its `ApiKeyResolver`
seam injected). The classify branch stays the gatekeeper seam (a behavioral no-op
on the diffed trace/tables when not-dangerous — its `classifying` status is
outside the shared status vocabulary). The finalizer now honors the resolver OFF
short-circuit for the danger-classification enqueue
(`FinalizerChatSettings.danger_mode_off`), and the memory-extraction +
danger-classification enqueues use the ORIGINAL `connectionProfile.id`
(`FinalizeOptions.connection_profile_id`), distinct from the rerouted
`effectiveProfile.id` (which the persisted assistant message + cost tracking keep)
— matching v4. Two orchestrator-corpus cases added, driving v4's REAL danger
resolution (global AUTO_ROUTE, no `uncensoredTextProfileId` so the empty-response
failover stays inert; a canned `findApiKeyByIdAndUserId` seam): `danger_off_short_circuit`
(off-duty chat → resolved OFF → no classification enqueue, router never consulted)
and `danger_live_reroute` (dangerous chat + AUTO_ROUTE + uncensored profile → the
primary stream reroutes, proven by a distinct recorded canned stream key + the
flags on the user message). `orchestrator_tier3_equivalence`,
`message_finalizer_tier3_equivalence`, `primary_stream_tier3_equivalence`,
`danger_resolver_equivalence`, `danger_routing_equivalence`, and
`danger_gatekeeper_tier3_equivalence` re-verified green against regenerated
oracles; the pre-existing orchestrator cases are a behavioral no-op under the real
resolver.

**Drift re-port (W4.d1, 2026-07-06): the Myers unified diff is now ported and
green.** v4 commit `8617ce7a` ("tighten Document Mode change diffs") rewrote
`lib/doc-edit/unified-diff.ts` and added `lib/doc-edit/line-diff.ts`, staling
the ported `doc_edit::unified_diff` (the old W4.1d3a greedy 3-line-lookahead
walker). Now caught up: the new leaf `doc_edit::line_diff` ports `diffLines`
(a Myers O(ND) shortest-edit-script diff over line arrays — a byte-faithful
transcription, NOT a crate, incl. the exact `k === -d || (k !== d && v[k-1] <
v[k+1])` tie-break in both the forward and backtrack passes so the recovered
op order matches under ties) + `changedBlockIndices`; and
`doc_edit::unified_diff` is rewritten on top of it — git-style hunks with
3-line context, maximal changed runs coalesced when their expanded ranges
touch (`start <= last.end + 1`), correct `@@ -start,count +start,count @@`
ranges via `format_range` (count 0 → `start-1,0`), empty content = zero lines,
and a whole-file replacement-hunk fallback past `MAX_DIFFABLE_LINES = 10000`
combined lines. The old greedy walker is deleted (v4 deleted it). Verified by
the regenerated + extended `doc_edit_leaves_equivalence` (106 rows: coalesce
vs split hunks, context truncation at file start/end, the formatRange shapes
incl. the delete-at-top/empty-side `0,0` range, create-from-empty and
empty-from-content, a shifted-block case, a Unicode line, the >10 000-line
whole-file fallback, plus `diffLines`/`changedBlockIndices` rows driven
directly, mirroring v4's own new unit tests); the `doc_text` / `doc_fm`
handler differentials re-verified green against regenerated oracles (their
handlers do not build the diff payload — confirmed 2026-07-06). **Still
seam-side:** the ported doc-edit handlers do NOT yet emit the
`change: { kind: 'edited', diff }` payload that consumes `generateUnifiedDiff`
— when W4.6b ports the Librarian save-announcement writer, the handlers must
START producing it (currently omitted).

**Wave 4 (W4.7a): the provider manifest + registry core is DONE** (2026-07-06),
the first W4.7 unit. Replaced v4's npm-plugin provider registry (which does not
survive the port — no Node, no dynamic import, no shipping third-party JS into the
Rust core) with the declarative-manifest + compiled-discriminator design of
`provider-manifest.md`. New `quilltap-core::provider_manifest`: the serde manifest
schema (deserialization IS the JSON-Schema validation — a missing field / bad enum
/ wrong `schemaVersion` each fails loud with a typed `ManifestError` naming the
field; `Manifest::from_json` is the third-party load/validate path but NO fs /
network / signing in the core), the `StreamDecoder` (`chat-completions-sse` /
`responses-api-sse` / `anthropic-sse` / `google-parts` / `ollama-ndjson`) +
`RequestTransform` (`none`/`anthropic`/`openai`/`google`/`deepseek`) **closed
enums** (the exact values W4.7b/c implement against — renaming forbidden, adding a
variant fine), the nine built-in manifests GENERATED from v4's registered plugin
metadata by the checked-in `harness/oracle/providers/gen-provider-manifests.mjs`
(transcription not re-derivation — the tool-catalog precedent; `include_str!` +
parsed once behind a `LazyLock`; the getter-checked fields pulled off the built
plugin objects, the decoder/transform/endpoints/auth/baseUrl from a fixed
augmentation table), the `Registry` accessors reproducing v4's `provider-registry`
convenience getters (`get_provider` **EXACT-case** `Map.get` — v4 does NOT resolve
`legacyNames` in lookup, they are display metadata only; `all_providers` in
registration order; `supports_capability`/`attachment_support`/`message_format`/
`chars_per_token` [default **3.5**]/`tool_format` [default **openai**]/
`cheap_model_config`/`default_context_window` [default **8192**]/`model_pricing`
[the STATIC fallback tier — empty on every built-in today, W4.7e brings the live
fetcher]), and `rewrite_localhost_url` (pure — v4's `rewriteLocalhostUrl` with the
host-side VM/gateway resolution injected as `Option<&str>`; a hand-rolled URL
rewriter reproducing `new URL().toString()` for the localhost subset — scheme/host
lowercase, default-port drop, empty-path `/`, userinfo/port/query/fragment
preserved). Verified by `provider_registry_equivalence` (a tsx oracle initializes
v4's REAL registry the runtime way — load the built `plugins/dist/*` bundles +
`initializeProviderRegistry` — and drives every convenience getter over every
provider: 253 NDJSON rows, tier-1 exact, incl. the absent-field defaults, the
legacy-name lookups that must NOT resolve, and a determinism dump; the Rust side
answers from the manifests) plus malformed-manifest fail-loud unit tests
(missing field / bad enum / wrong `schemaVersion`). Generator determinism verified
(dump twice, identical). **The four registry-seam replacements are closed in their
LEAF consumers, none skipped:** (1) `message_formatter::get_provider_name_support`
now consults the manifest registry before the legacy fallback (v4's
`getProviderNameSupport` shape) — a **real behavior change** from the pre-W4.7a
empty-registry state: DEEPSEEK / Z_AI / OPENAI_COMPATIBLE (manifests advertise
name-field support; no legacy-table row / hyphen-vs-underscore miss) now report
name-field support; its oracle regenerated to initialize the real registry
(118 rows). (2) `model_context`'s `registry_default` + `model_info` and (3)
`cheap_model`'s recommended-list/default inputs KEEP their injected parameters
(the orchestrator spine populates them), but their oracles were regenerated with
the real registry so the injected values reflect real manifest data (ANTHROPIC
default 200000, DEEPSEEK/Z_AI 131072 — banked as fall-through queries proving the
seam) — `model_context_equivalence` (25 queries) + `cheap_model_equivalence`
green. (4) `tool_build`'s `provider_supports_web_search` stays a corpus-controlled
knob in its differential (the manifest `capabilities.webSearch` equals v4's
`provider.supportsWebSearch` for all nine — proven in the registry oracle);
`tool_build_equivalence` (27 cases) re-verified green. **All four pins moved to
"the registry value equals the pinned value," asserted in
`provider_registry_equivalence`** (`messageFormat`/`defaultContextWindow`/
`cheapModelConfig`/`webSearch`-capability rows) so a manifest drift is caught
there, not silently in a leaf. **Tracked handoffs / deferrals:** the **spine-side
seam removals** — sourcing `provider_supports_web_search` / the model-context
registry default / the cheap-model recommended-list+default from the registry at
the orchestrator composition point (i.e. dropping the `ProcessMessageInput` /
`BuildToolsInput` fields the spine constructs) — are deferred to the
orchestrator-spine owner (this unit kept the injections flowing + made the leaves
able to source from `Registry`, without touching `services/orchestrator.rs` or its
corpus); `baseUrl`/`endpoints`/`auth`/`streamDecoder`/`requestTransform` are
carried as manifest data but only the decoder/transform enum VALUES are
load-bearing here — the endpoints/auth/baseUrl are best-effort transcription that
W4.7b/c refine against recorded wire fixtures (not differential-checked in W4.7a);
third-party manifest loading (fs, signing) is a design open item (the load/validate
path exists, only the built-ins are wired); manifest pricing is the static
fallback tier (W4.7e = the live fetcher). W4.7b/c are next (b independent of a; c
after b).
**Wave 4 (W4.7c — the provider tool wire + request builders): DONE** (2026-07-07;
part 1 `crate::model::tool_wire`, part 2 `crate::model::request_builder`).

**Part 2 — the request builders + the four `RequestTransform` hooks — is DONE**
(`crate::model::request_builder`, `request_builder_equivalence` +
`request_builder_google_equivalence`). The sans-IO request-side counterpart to
the W4.7b decoders: `build_request(provider, &RequestInput) -> BuiltRequest`
(method / url / headers / body VALUE — no HTTP; the transport is W4.7d),
dispatched by the manifest (`baseUrl`+`endpoints.chat` → url, `auth` → headers).
Every SDK / raw fetch sends `JSON.stringify(body)` VERBATIM (confirmed by the
`record-request-envelopes.mjs` fetch-intercept recorder), so bodies are built as an
ordered `serde_json::Map` (preserve_order) with keys inserted in v4's exact
assignment order, integer-valued numbers bare (`js_number_to_json`); `Body::remove`
uses `shift_remove` (JS `delete` preserves order — the default swap_remove would
reorder). The four hooks: **anthropic** (`applyMidHistoryBreakpoint` + the
tool-result batching + assistant-tool_call→content-blocks expansion + the
cache-control hierarchy [tools→system→messages] + the adaptive-thinking /
sampling-param-rejection rules — the `SAMPLING_PARAMS_REJECTED_MODELS` prefix list
[Sonnet 5 / Opus 4.7·4.8 / Fable 5 / Mythos 5·preview] ported as a **compiled
constant**, NOT lifted to the manifest — the rules are prefix-regex matching, not
per-model data, and the W4.7a manifest has no slot; a clean lift would need a new
`samplingRejectedModelPrefixes` field, deferred as a manifest-schema follow-up);
**openai** (`previous_response_id` chaining — send only the last user message; the
fallback-to-full-input on a send error is a transport concern, W4.7d); **google**
(the recursive JSON-Schema sanitizer `sanitizeSchemaForGoogle` + the
`thoughtSignature` round-trip in `formatMessagesForGoogle`); **deepseek**
(`reasoning_content` echo on a tool-call turn + `stripThinkingIncompatibleParams`).
Chat-completions family (deepseek / z-ai [+ the `web_search` tool + the glm-5.2+
`reasoning_effort` default] / openrouter [the raw-fetch tools path] / ollama /
openai-compatible base) and the responses-API family (openai / grok) are all
**byte-exact against the wire**. **Google's genai-SDK `config → generationConfig`
wire framing is DEFERRED to the transport** (the SDK owns that mechanical
serialization + reorders keys, e.g. `{name,args}`→`{args,name}`); the google
request LOGIC — the sanitizer + `contents`/`systemInstruction`/`shouldDisableTools`
— is ported and verified against v4's REAL plugin (`formatMessagesForGoogle` via
bracket access; the sanitizer via the wire `functionDeclarations`, which the SDK
passes through faithfully). Verified by `request_builder_equivalence` (31 rows,
7 providers, byte-exact body/url/method) + `request_builder_google_equivalence`
(5 rows). With this, **W4.7c is fully DONE**; the remaining provider-layer units
are W4.7d (transport + errors + `api_keys`), W4.7e (pricing/capability/logging/
embeddings), W4.7f (image dialects + moderation + web search).

**Wave 4 (W4.7f): the image wire dialects + OpenAI moderation + Serper web search
are DONE** (2026-07-07). New sans-IO seam `crate::model::wire` (`WireTransport` /
`SyncWireTransport` + `WireResponse` + canned transports) sits between the ported
request builders / response parsers and the host HTTP client (W4.7d). **The five
image dialects** (`crate::model::image_dialects` — v4
`plugins/dist/qtap-plugin-{openai,google,grok,openrouter,z-ai}/image-provider.ts`;
the plan's four was corrected to FIVE, z-ai was omitted): `build_image_request` +
`parse_image_response` per provider, with the SDK families (OPENAI png / GROK jpeg
/ Z-AI png — mimeType HARDCODED; a non-2xx is the SDK throw surfaced verbatim) and
the raw-fetch families (GOOGLE's two dialects — Imagen `:predict` with the ONLY
manufactured moderation error [`Google Imagen rejected prompt by content
policy${reason}`, the prediction/data/`filteredReason` fallback chain] + Gemini
`:generateContent`; OPENROUTER's chat-endpoint with the `negativePrompt`/`style`
prompt-append, `quality:'hd'`→`4K`, and the `data:(image/…);base64,` regex parse)
inspecting the status themselves. The three refusal-keyword **GAPs are carried
faithfully** (Gemini `textResponse || 'No images…'`, OpenRouter `Model declined…`,
z-ai's absent moderation handling never match `is_image_moderation_error` — never
widen the keyword set). `GeneratedImageData` gained `url` + optional `data` (v4's
`GeneratedImage`, for z-ai's dual b64+URL happy path — the only provider populating
`url`). `RealImageProvider` composes build + the transport seam + parse (closing the
`generate_image` provider seam). **Orientation data** (`crate::image_gen_data`):
the real per-provider `getImageGenerationModels` (OPENAI/GOOGLE/OPENROUTER per-model)
+ `getImageProviderConstraints` (GROK/Z-AI provider-level) declarations transcribed
as a compiled-constant module feeding `resolve_orientation` (the dall-e-2
empty-mapping degrade-to-hint preserved). **The OpenAI moderation wire**
(`crate::services::dangerous_content::moderation_wire`): `build_moderation_request`
+ `parse_moderation_response` (`POST {base}/v1/moderations`, `Object.entries`
category order, `category_scores[cat] ?? 0`, empty-results → clean, HTTP error →
`OpenAI moderation API error ({status}): {errorText}`) + `RealModerationProvider`
closing the W4.2 gatekeeper seam (auto-detect the OPENAI connection profile →
`apiKeyId` → the injected `ApiKeyResolver`). **The Serper web-search wire**
(`crate::tools::web_search`): `build_serper_request` / `map_serper_results` (the
`knowledgeGraph` unshift boundary) / the plugin error set + the DISTINCT env-var
fallback error set / `format_web_search_results` (the built-in formatter, ported
once, used twice — `(Published: Invalid Date)` for free-form dates) +
`RealWebSearchProvider` closing the W4.1d5 seam (over `SyncWireTransport` + a
`SearchApiKeyLookup`). Verified by THREE new tier-1 differentials driving v4's REAL
plugins over `global.fetch`/SDK mocked to committed payloads
(`image_dialects_equivalence` — every dialect's request bytes + parsed response +
rejection strings + the `is_image_moderation_error` verdict matrix incl. the gaps,
PLUS the orientation transcription vs v4's real declarations;
`moderation_wire_equivalence`; `web_search_wire_equivalence` — the plugin set +
`formatResults` rows). Regenerated three tier-3 differentials green: `web_search_tool`
(the REAL `RealWebSearchProvider` over a canned transport + the REAL handler's
env-var fallback path, previously untested), `danger_gatekeeper_tier3` (the
moderation provider UN-MOCKED — v4 drives the REAL `moderationPlugin.moderate` over
canned `fetch`, Rust drives the REAL `RealModerationProvider` over the canned wire
keyed by the `token=<case>` marker, the failure case a canned 500), and
`image_generation` (the REAL image dialect over a canned wire reverse-mapped from
the oracle's recorded images). **Tracked deferrals (handed to the round
unification / W4.7d):** the api-key lookups (moderation auto-detect's
`db::api_keys` resolution, the search `getAllApiKeys` scan) stay behind the
injected `ApiKeyResolver` / `SearchApiKeyLookup` seams until W4.7d's `db::api_keys`
lands; the real HTTP transport implementing `WireTransport` is W4.7d's; the
transcode stays the injected `ImageTranscoder` seam (no image-codec crate);
`is_image_moderation_error` still lives ONLY in W4.2's `provider_routing`.

**Part 1 — the tool wire — DONE** (`crate::model::tool_wire`).
Ported v4's `packages/plugin-utils/src/tools/{converters,
parsers,text-parsers}.ts` + the per-plugin `formatTools`/`parseToolCalls`/
`hasTextToolMarkers`/`parseTextToolCalls`/`stripTextToolMarkers` glue, dispatched
here by the manifest `ToolFormat` (the W4.7a registry replaces `getProvider`;
only `GOOGLE` has `ToolFormat::Google`, so gating the two Google-specific
behaviors on it reproduces v4's per-plugin dispatch). Three pieces:
`format_tools_for_provider` (the reshape: Anthropic `input_schema` / Google
`parameters` / OpenAI passthrough, each dropping non-`function` tools; unknown
provider → canonical passthrough), `detect_native_tool_calls` (the native parse
`parseOpenAI`/`parseAnthropic`/`parseGoogle` + the Google `functionCalls` fast
path; unknown provider → `[]`), and the provider text-markers trio (the composite
XML suite `parse_all_xml_*`/`has_any_xml_*`/`strip_all_xml_*` for every provider,
Google's tool_use-only variant, gated by `provider_has_text_markers`). Number
fidelity: parsed `arguments` are walked by `normalize_js_numbers` (integer-valued
floats collapse, matching `JSON.stringify`). Regex fidelity: the one backreference
(`/<(\w+)>([^<]*)<\/\1>/gi`) is hand-rolled (`parse_named_tag_pairs`, ASCII-word
tags + case-insensitive close), and the others use `(?-u:\s)` / ASCII-`\w`
hand-rolls + `(?i)` (the exotic-Unicode-case-fold and non-ASCII-tag-whitespace
divergences are documented seams). Byte offsets replace v4's UTF-16 offsets in the
XML parsers (both monotonic → identical dedup/sort). **Three live seams CLOSED:**
(1) the native-tool-loop `ToolCallDetector` — new `RegistryToolCallDetector`
(`native_tool_loop`); (2) the text-tool-loop provider-text-markers strategy — new
`ProviderTextMarkersStrategy` (`text_tool_loop`); (3) the W4.1g `formatTools`
reshape — `tool_build::format_tools_for_provider` (available + tested, NOT wired
into `build_tools` — a spine handoff, since the orchestrator oracle was generated
with an empty registry → canonical anthropic tools; wiring it needs the
spine-owned orchestrator oracle regenerated with the real registry). Verified by
`tool_wire_equivalence` (a new tier-1 differential — the `record-tool-wire.mjs`
recorder drives each v4 plugin's REAL tool-wire methods over a single-authored
corpus emitting `{kind,provider,case,input,result}`; the Rust side reads the input
+ diffs byte-exact; 231 rows across anthropic/openai/google/deepseek covering all
three `toolFormat` branches + real recorded rawResponses, plus a Rust dispatch-
uniformity test proving the other five providers key on `toolFormat`). The two
loop differentials were **regenerated with the real detector/strategy** (swapping
their synthetic ones): `native_tool_loop_tier3_equivalence` now drives v4's REAL
Anthropic `parseToolCalls` over REAL anthropic `content[]` rawResponses (Rust:
`RegistryToolCallDetector::built_in()`), and `text_tool_loop_tier3_equivalence`
now drives v4's REAL DeepSeek plugin text markers over real `<tool_use>` XML
(Rust: `ProviderTextMarkersStrategy::built_in("DEEPSEEK")`) — both green (corpus
transformed so the extracted tool calls are unchanged; non-empty stop-forwarding
stays proven by the simple-json case). **Standing handoff to the orchestrator-spine
owner** (the one part-1 seam-wiring left for unification; part 2 is DONE — see
above): wire the real `RegistryToolCallDetector` into the `process_message`
orchestrator spine (drop the injected `NoToolCallDetector` at the composition
point — `native_tool_loop`'s `detector` param), wire `ProviderTextMarkersStrategy`
into the spine's Phase-19 provider-text pass (gated by `provider_has_text_markers`),
and wire `format_tools_for_provider` into `build_tools`, regenerating the
spine-owned `orchestrator_tier3` oracle with the real registry.

**Wave 4 (W4.7b): the five stream decoders are DONE** (2026-07-06,
`crate::model::decoders`). The sans-IO push-state-machine wire decoders that turn
a provider's streamed bytes into the normalized `StreamChunk` sequence (the
`model::stream` vocabulary, **NOT extended**) — each a `StreamDecoder`
(`push(&[u8])` / idempotent `finish()`) correct fed one byte at a time. A shared
spec-faithful SSE frame splitter (`decoders::sse` — `\n`/`\r`/`\r\n` lines,
`data:`/`event:` fields, multi-line data, comment keep-alives, blank-line
dispatch, `\r\n`-split-across-pushes safe) underlies three of them. The five:
`chat_completions_sse` (openai-compatible / deepseek / z-ai / openrouter — the
tool-call accumulator keyed by `tool_calls[].index` concatenating fragmented
argument strings, reasoning routing, usage in the trailing frame, `[DONE]`),
`responses_api_sse` (openai / grok — `response.output_text.delta` +
CUMULATIVE `response.reasoning_summary_text.delta` re-sends + terminal
`response.completed` → v4's Chat-Completions-shaped `rawResponse`),
`anthropic_sse` (`content_block_start`/`delta`/`stop` state machine,
`input_json_delta` per-index buffering, thinking/signature accumulation, usage
split across `message_start`/`message_delta`, mid-stream `error` events),
`google_parts` (genai `generateContentStream` — verified `data:`-prefixed SSE at
the SDK's `processStreamResponse`, parts iteration with `thought===true` →
reasoning, `thoughtSignature` from the last chunk, functionCall parts, the
thinking-model `finalContent` fallback), and `ollama_ndjson` (newline-delimited
JSON, whole-object tool_calls normalized to OpenAI shape, `done:true` terminal).
Each also assembles the terminal `rawResponse` value v4 hands back for
`detectToolCallsInResponse` (W4.7c) byte-for-byte. Verified by
`stream_decoders_equivalence`: a **checked-in fetch-mock recorder**
(`harness/oracle/providers/record-stream-fixtures.mjs` — overrides `global.fetch`
to replay a committed wire transcript through the provider's real SDK/transport,
driving v4's REAL plugin `streamMessage` generator) records the normalized chunk
NDJSON per case; the Rust decoders replay each transcript at **whole-buffer /
per-SSE-frame / byte-at-a-time** and diff the chunk sequence + `rawResponse`.
Committed: the wire transcripts (`fixtures/streams/<decoder>/*.wire`), the
recorded NDJSON (`*.recorded.ndjson`), the recorder + a `regenerate` script.
Adversarial cases banked per decoder (keep-alives, fragmented tool-call JSON with
escaped quotes, Unicode/multi-byte/emoji splits, mid-stream error, empty +
usage-absent streams, interleaved thinking/text, cumulative-reasoning re-sends,
split JSON line). **Three STOP-rule divergences from the design-doc table were
confirmed against v4 source at HEAD `8617ce7a` and handled (flagged):** (1) the
four "chat-completions-sse" providers do NOT share one normalization — deepseek
and z-ai go through the OpenAI SDK (`delta.reasoning_content`, the
`{choices:[{index,message:{...},finish_reason}],usage}` rawResponse) while
openrouter's tool/vision path is a raw-`fetch` `streamViaChatCompletions`
(`delta.reasoning`, the camelCase `{choices:[{finishReason,delta:{toolCalls}}]}`
rawResponse), and deepseek vs z-ai further differ on cache source
(`prompt_cache_hit_tokens` vs `prompt_tokens_details.cached_tokens`) and whether
`rawProviderUsage` is emitted (z-ai yes, deepseek no) — reproduced via an internal
`Flavor` (`DeepSeek`/`ZAi`/`OpenRouterRaw`/`OpenAiCompatible`) selector over ONE
shared parser, the decoder enum NAME unchanged; (2) `google-parts` is
`data:`-prefixed SSE (the table caption's "JSON array / newline" is superseded by
the SDK source); (3) openrouter's no-tools OpenResponses SDK path is a distinct
undocumented wire — **tracked deferral** (out of scope). **Two documented
transport-artifact normalizations** in the differential (not decoder logic):
google's SDK-injected `sdkHttpResponse` wrapper is stripped from `rawResponse`
(a sans-IO decoder never sees HTTP headers), and ollama is push-boundary-sensitive
BY DESIGN (v4 splits each network read on `\n` with NO cross-read buffer, so a
split JSON object is silently lost — a faithfully ported v4 bug), so ollama is
diffed at whole + line-aligned chunkings only; the byte-at-a-time lossy
bug-parity is a Rust-side unit test. **Recorder note:** run under `npx tsx` (some
plugins import extensionless sibling `.ts` modules — z-ai's `./models`); Node 24.

**Wave 4 (W4.7d): transport + errors + the `api_keys` table is DONE**
(2026-07-07), closing the LAST unported repo. Ported: (1) **`db::api_keys`** —
v4 hosts this collection inside `ConnectionProfilesRepository` (no dedicated
repo), so the v5 marshaling boundary is the table (its own `db::api_keys`
module). PLAINTEXT `key_value` (the DB cipher is the only protection — every
fixture/seam uses SYNTHETIC keys); `provider` is a FREE-FORM string
(`ProviderEnum = z.string().min(1)`), matched by exact equality.
`create`/`update`(RMW full-`$set`)/`delete`/`recordUsage`(rides `update`, bumps
`updatedAt`) + `findById`(UNSCOPED)/`findByIdAndUserId`/`getApiKeysByUserId`
(the per-row `safeParse` DROP, keyed on the exercised empty-provider invalidity
— full Zod validation a documented seam). Tier-2 `api_keys_tier2_equivalence`
(minted-values remap: id remap + `createdAt`/`updatedAt`/`lastUsed` placeholder;
banks the boolean 0/1, the nullable `lastUsed` null-vs-set CONTRAST proving
recordUsage, and the malformed-row drop). (2) **`services::api_key_service`** —
`get_api_key_for_connection_profile` / `get_api_key_for_cheap_llm_selection`
(local → `Some("")`) + the user-scoped wrappers (ownership pre-checks, `userId`
strip structural) + `find_active_api_key_for_provider` (the web-search/moderation
provider-SCAN style — NOT unified with the profile-follow style). The
`ApiKeyResolver` seam is CLOSED with a real DB-backed `ConnApiKeys` (over
`find_by_id_and_user_id`); the spine composition-point wiring (danger routing,
cheap_llm_exec, image handler, embeddings, web search) is handed to W4.4b (the
spine owner), per the order. (3) **`services::llm_errors`** — the unported half
of `lib/llm/errors.ts`: the 8 error classes folded into one `LlmProviderError`
tagged by `LlmErrorKind`, `handle_provider_error` (the string normalizer, MATCH
ORDER precedence-bearing: apikey→ratelimit→network→model→token→invalid→generic),
and `user_friendly_error` (byte-exact strings, en-US `toLocaleString` grouping,
JS `a && b` truthiness where `0` is falsy — reproduced). Reuses the already-ported
predicates/parsers from `primary_stream`. Tier-1 `llm_errors_equivalence` (54
rows: handle + construct + predicate regression, incl. precedence-collision
rows). (4) **`model::response_parse`** — the sans-IO non-streaming parsers for
the 5 wire families (chat-completions with the W4.7b `ChatFlavor` split
[OpenAiCompatible/DeepSeek/ZAi/OpenRouter — cache-read subtraction, reasoning,
tool calls per flavor], responses-API [OpenAI/Grok — `output_text`, reasoning
summaries, `buildRawResponse` reshape], anthropic [text/thinking-block concat],
google [non-thought parts, `thoughtSignature`], ollama) → `NonStreamingResponse`.
**`model::provider_models_api`** — validate/models endpoints + list parsers
(openai chat-prefix filter, z-ai image-model drop, google `supportedActions` +
`models/` strip, ollama `/api/tags`). Unit-tested against the verbatim-read v4
source; the recorded-payload tier-1 differential (fetch-mock recorder, the
W4.7b pattern) is a **tracked deferral**. (5) **`model::transport`** — the
`ProviderTransport` IO boundary: the trait + `TransportPolicy` (SDK-default
timeout/retry knobs) + the pure per-provider header builder (`User-Agent` on
all, openrouter `HTTP-Referer`/`X-Title`) are ALWAYS compiled (IO-free); the
concrete `reqwest` impl is behind the **non-default `native-transport`** cargo
feature (rustls, so the default core build stays IO-free — the STOP rule; both
`cargo test` default AND `--features native-transport` pass). No timeout/abort/retry
exists at v4's provider tier (SDK defaults apply) — integration-smoke tier, no
differential. **`model::completion_provider`** — the production CompletionProvider
composition (`build_request` → `transport_headers` + auth injection → transport
→ `parse_for_provider` → `CompletionResponse`), generic over the trait, fake-transport
unit-tested. (6) **The W4.7c Google `config → wire` framing deferral is CLOSED**:
`build_request` now emits the genai-SDK wire body for GOOGLE — the flat `config`'s
sampling/output fields nest under `generationConfig` (in the SDK's FIXED field
order, not insertion order), `systemInstruction`/`safetySettings`/`tools` stay at
root, each content `{role,parts}`→`{parts,role}`, the `functionCall`
`{name,args}`→`{args,name}` (id first if present), `partialArgs`/`willContinue`
THROW, root order `contents,systemInstruction,safetySettings,tools?,generationConfig`.
Byte-verified against the recorded wire (fetch-intercept under the real genai SDK)
by `request_builder_google_wire_equivalence` (5 cases incl. the thought-sig
functionCall reorder). **Tracked deferrals:** the recorded-payload non-streaming
parse + validate/models differential; the `auto-associate.ts` settings feature +
the 3 unscoped `findApiKeyById` call sites (unported Phase-4 surfaces); z-ai's
static-model-list merge (config data, dynamic-path filter/sort ported); the spine
ApiKeyResolver wiring (W4.4b).

**Wave 4 (W4.8): the background job runner is DONE** (2026-07-06). v4's
forked-child job processor (`lib/background-jobs/host/{processor-host,
job-dispatcher}.ts` + `child/child-entry.ts`) is re-expressed as an **in-process
runner** over the single-writer runtime. The fork/IPC/buffered-write-proxy
architecture does NOT port (an encoded decision) — `db::runtime::Db` already
makes "only the writer task holds the RW connection" a compiler-enforced
ownership rule, so v5 job handlers run in-process and write through `Db` like
every request-path service (the job-level all-or-nothing write buffer is
deliberately dropped for direct-writing handlers — v4 runs those same services
unbuffered on the request path; `write_apply` remains available for a batch-mode
handler that needs main-primary ordering, the autonomous turn/Unit 4). New
`services::job_runner`: the claim-loop core (`pump_claim` — the `claiming`
reentrancy lock, the `maxConcurrentJobs` instance-settings read each pump
[`instance_settings::get_max_concurrent_jobs`, default 4 clamp 1–32], the
claim-until-full loop over the ported `claim_next_job` [on the writer, since it
mutates PENDING→PROCESSING], and the `PumpOutcome`'s next-wake-delay via
`find_next_scheduled_at` + `clamp_wake_delay` returned to the host); dispatch by
job type through a `HandlerRegistry` (a type-string → `Box<dyn JobHandler>` map
whose handlers close over their own model seams and decode the payload) with a
**loud fallback** for unregistered types (`KNOWN_JOB_TYPES` → "recognized but not
yet available"; unknown → v4 `getHandler`'s `No handler registered for job type:
<type>` — a later order adds a row without touching callers); completion/failure
(`markCompleted` NOW wiring `merge_result_into_payload` — closes Phase-2 deferral
#3, forward-only since v4-on-SQLite throws on the dotted `payload.result`; a
handler `Err`/`Failed` → `markFailed` with the ported backoff); and recovery
(`reset_orphaned_jobs` at startup, `tick_stuck_reset` on the 5-minute cadence).
**All timers are host-driver seams** (STOP rule honored: no tokio timers in the
runner core) — the host driver owns cadence, per the enclave `step()` philosophy.
New `services::job_scheduler` with the pure decision leaves (`clamp_wake_delay` =
v4 `armWakeTimer`'s `min(max(rawMs,100), 300_000)`; `should_run_startup_tick` =
the 20 h recent-run window) + the cadence constants. The **`ensureProcessorRunning`
seam is CLOSED**: `queue_service::enqueue_job` now fires a process-global wake hook
(`set_wake_hook` / `JobRunner::install_wake_hook` → `JobRunner::wake`; a no-op
until the host registers it — faithful to v4's `QUILLTAP_JOB_CHILD` no-op).
Extended `queue_service` with the read/admin surface (`get_job_status` /
`get_queue_stats` / `get_active_counts_by_type` / `cancel_job` /
`get_pending_jobs_for_chat` / `cleanup_old_jobs` / `cleanup_finished_jobs`), the
retention windows (`retention_cutoff_iso` + the per-status constants), and the
portable scheduler sweep bodies (`run_scheduled_housekeeping` /
`run_scheduled_cleanup`, over a new scoped `chat_settings::find_all_scheduler_settings`).
Ported the **stale-chat asset maintenance sweep** (`services::maintenance::
collapse_stale_chat_assets`, v4 `collapse-stale-chat-assets.ts`) with the new
`chats_messages_read::get_last_played_message_at` scoped read (v4 `42242a3e`), the
keep-set avatar-sha resolution (`resolveCharacterAvatar`'s vault-link →
`files.findById` fallback, sha only), the `isPhotosRelativePath` pure leaf (ported
into `db::doc_mount_file_links` with a faithful `path.posix.dirname`), and the four
protection branches (current / current-sha / album-or-vault-link [via
`doc_mount_files::find_by_sha256` + `doc_mount_file_links::find_by_file_id` +
`doc_mount_points::find_store_type_by_id`] / character-reference [via new raw
`characters_read::count_by_{default_image_id,avatar_override_image_id}`]); the
`deleteFileCompletely` storage-bytes delete is a documented **host FsSeam** (the
DB metadata-delete half ports via `files.delete`). Verified two ways: a tier-1
differential (`photos_relative_path_equivalence`, driving v4's REAL
`isPhotosRelativePath` — banks the `/photos/` root, `my-photos/`, `photosx/`
guards) and a **tsx real-DB tier-2 differential** (`maintenance_sweep_tier2_equivalence`,
driving v4's REAL `collapseStaleChatAssets` over a two-DB fixture, zero
normalization — exercises all four skip branches + the fresh-chat guard: one
unprotected file deleted, five survive), plus eleven runner self-tests
(concurrency cap, wake-on-enqueue, claim-order priority/FIFO, loud fallback,
stuck/orphan reset, drain-on-shutdown, and an end-to-end memory-housekeeping
dispatch enqueue→claim→dispatch→markCompleted-merge). The
`memory_watermark_tier3` / `context_summary_service_tier3` differentials were
regenerated green with the wake hook live (the DB effect is unchanged — the wake
is a no-op in the differential harness). **Tracked deferrals:** the autonomous-room
job types (`AUTONOMOUS_ROOM_TURN` / `_SCHEDULE_TICK` — Unit 4 owns them; the runner
dispatches them via the loud fallback until then); the memory-extraction handler's
payload→`TurnMemoryExtractionContext` assembly (v4 `buildTurnTranscript`, unported
— the runner dispatches to whatever handler the host registers, and the E2E test
registers a real `MEMORY_HOUSEKEEPING` handler instead); the danger-scan enqueuer
sweep's per-chat classification decision-tree (leans on the full
`chat_settings.findAll` marshaling + connection resolution — the enqueue helpers
themselves are ported); the maintenance sweep's storage-bytes FsSeam. See the
memory note for the fork/IPC non-port rationale.
**Wave 4 (W4.9b): the photo trio is DONE — the last deferred tool handlers**
(2026-07-06). `keep_image` / `list_images` / `attach_image` (v4
`lib/tools/handlers/doc-edit/photo-handlers.ts`) are ported and dispatched,
closing the W4.1d3b photo-group deferral. New `crate::photos` module:
`keep_image_markdown` (v4 `keep-image-markdown.ts` — `build_kept_image_markdown`
over the ported `serialize_frontmatter` [= `YAML.stringify` of the tags/linkedBy/
linkedById/linkedByRole/generationModel bag], the `## Original prompt` /
`## Revised prompt`-only-if-different / `### Scene at {ts}` / attribution-footer
sections; `parse_kept_image_frontmatter` + the caption regex `/ saved this image
at [^\s]+ with this caption: (.+)$/m` reproduced with `JS_WS_CLASS`;
`build_slug_and_filename` [`toLowerCase` + `[^a-z0-9]+`→`-` collapse + 60-cap];
`build_attach_description_from_kept_image`; `sha256_of_string`/`sha256_of_buffer`;
`linkedByRole` back-compat → `'character'`), `photos_paths` (the `photos/` folder
helpers — `is_photos_relative_path` the POSIX-dirname case-insensitive test), and
`save_image_to_album` (v4 `save-image-to-album.ts`: resolve the FileEntry via
`files::find_by_id` with the **mount-blob fallback** [link-id →
`find_by_id_with_content` → sister by sha256 → `doc_mount_blobs::read_data_by_file_id`
→ ingest], IMAGE-category validation, the bytes via the injected
**`FileBytesStore`** seam, the per-mount `photos/`-only **dedup guard** [a sha
elsewhere does NOT collide → `ALREADY_SAVED`], sceneState parse + malformed
placeholder, the markdown build, `resolve_unique_relative_path` +
`ensure_folder_path`, `link_blob_content` with `extractedText` +
`extractionStatus='converted'`, then the chunk rollup [the chunker is NOT
re-ported — `chunkCount` pinned / `doc_mount_chunks` excluded, the groups/projects
precedent — but `plainTextLength` set exactly], + the recorded mount-invalidation
/ embedding-enqueue seams). The three `tools::photo` handlers
(`handle_keep_image`/`handle_list_images`/`handle_attach_image`) compose that over
the ported vault reads (`characters_read::find_by_id` for the overlaid
name+FK+`systemTransparency`, `find_store_naming_by_id` for the character-store
mount) + the semantic `document_search` (v4 `searchDocumentChunks` over `photos/`,
minScore 0.3, literal boost, dedupe-by-fileId best-score) with the SILENT fallback
to plain listing, the Shared-Vaults peer visibility
(`collect_peer_character_ids_for_reads` — the chat's `allowCrossCharacterVaultReads`
gate, transparent peers only), tag/saved_by filters, the 200-char prompt excerpt,
pagination (limit ≤ 100), and the `attach_image` self-vault-only rule +
`/api/v1/mount-points/{id}/blobs/{encodeURI(path)}` descriptor
(width/height joined from sister FileEntries). Wired into `BuiltInToolRunner`
(removed from the loud fallback) — each dispatches inside a both-connections
`Db::write` closure (the wardrobe/doc-edit precedent); `list_images`' semantic
embedding is computed up front (the dispatcher's `ErasedEmbeddingProvider`) and
fed into the sync closure; `formattedText` byte-exact (v4 routes photos through
`executeDocEditTool` → `formatDocEditResults`, so keep/list get
`{ formattedText, ...result }` while `attach_image` passes its descriptor ARRAY
through unchanged). Added photo-facing reads: `files::{find_by_id, find_by_sha256}`
(the FileEntry subset — sha256/category/width/height/generation*/mimeType/storageKey),
`doc_mount_file_links::{find_by_id_with_content, set_chunk_rollups,
delete_chunks_by_link_id}` + the `LinkWithContent` struct. Verified by
**`photo_tools_tier3_equivalence`** — a jest-real-DB oracle driving v4's REAL
`executeDocEditTool` over a two-DB fixture (Aurora + Basil, both transparent, full
vaults; a chat with `sceneState` + `allowCrossCharacterVaultReads` + a malformed-
scene chat; three baked photos via v4's REAL `saveImageToAlbum` [Date frozen to
pin `keptAt`] with seeded chunk embeddings; ingested FileEntries with generation
metadata), ONE fresh copy per case, `generateEmbeddingForUser` + `readImageBuffer`
+ the two mount-index side-effect modules jest-mocked (mirrored by the Rust
`CannedEmbeddingProvider` / `CannedBytes` / `NoSideEffects`) — over 14 ops (keep
fresh [six-table dump: `doc_mount_points`/`_files`/`_blobs`/`_file_links`/`_folders`
+ `files`, shared-id-map remap + `<ts>` + `<cc>`], keep-duplicate → `ALREADY_SAVED`,
keep-with-malformed-scene, plain listing + tag/saved_by/pagination filters,
semantic ranking + the cross-vault peer photo surfacing both ways + the silent
embedding-failure fallback, attach by link-id, attach by file-id → sha, attach
cross-vault refusal, attach missing) — result JSON + `formatDocEditResults` diffed
byte-for-byte (keep results positionally UUID-normalized for the minted link id).
`tool_dispatch_equivalence` gained a `list_images` row (Friday's empty album →
"No images saved yet.", real handler both sides) and the five `doc_*` handler
differentials re-verified green (shared `doc_mount_file_links` surface, additive
reads only). **Tracked deferrals (seams, all standing host boundaries):** the
`FileBytesStore` production impl (host `fileStorageManager`), the
mount-invalidation / embedding-enqueue side effects (the EMBEDDING_GENERATE
enqueue via `queue_service` is a recorded seam this round), the chunker
(`chunkCount` pinned), and `resolve_unique_relative_path`'s sha1 collision
fallback (non-deterministic, never hit — a photo filename embeds a millisecond
ISO timestamp).

**Wave 4 (W4.3): the answer-confirmation service is DONE** (2026-07-06,
`services::answer_confirmation`), closing the finalizer's `NoAnswerConfirmation`
seam with the real runner. Ported v4's `answer-confirmation.service.ts` — the
Salon pre-landing consistency check + re-affirmation: the gate/leaf functions
(`is_answer_confirmation_active` three-level cascade [chat > project > global,
`'OFF'` beats an inherited `'ON'`], `has_checkable_inputs` over the static
`CONFIRMATION_READ_TOOLS` set, `find_latest_commonplace_whisper` [backward scan
for the last `systemSender:'commonplaceBook'` message targeting this participant],
`is_user_driven_turn`, and `gather_confirmation_inputs` — the reference block
[whisper section + per-tool `=== Lookup result ===` sections] with the
`REFERENCE_CHAR_BUDGET = 24_000` UTF-16 oldest-first truncation) and
`run_answer_confirmation` (never-throws): the consistency check over
`CheapLlmTaskExecutor` + `CompletionProvider` (the `CONSISTENCY_SYSTEM_PROMPT` +
the `--- REFERENCE INFORMATION --- / --- REPLY TO CHECK ---` user message, byte
bodies in a generated `prompt_text` submodule), the fenced-JSON verdict parser
(v4's `extractJson` — a fence-anywhere/first-`{…}` extractor, DISTINCT from the
leading/trailing `strip_code_fences`; throws on parse failure → could-not-verify),
the **uncensored escalation** of the check's cheap selection on a dangerous chat
(composing the ported `is_chat_active_dangerous` + `resolve_uncensored_cheap_llm_selection`
— the re-affirmation stays on the character's OWN profile, NOT escalated), and the
re-affirmation pass with its four outcome mappings (rewrite+reply →
confirmed:true/revised + notes + `revisedContent`; rewrite+empty → confirmed:null;
stood-by → confirmed:false; error → confirmed:null with notes). The finalizer's
gate leaves (`isAnswerConfirmationActive` / `isUserDrivenTurn`) were HOISTED into
the service (single source of truth); the finalizer seam is now a generic
`RealAnswerConfirmation` runner (async, over an injected `CompletionProvider` +
the shared executor) — the finalizer reads the prior messages, finds the whisper,
gathers the reference, emits the `confirming` status frame, calls the runner (which
fires the `affirming` frame via an `on_affirming` callback before the re-affirmation
pass), and applies the rewrite's tool-anchor drop + reasoning collapse to a single
offset-0 block. Added `jsstr::utf16_slice_from` (JS `.slice(start)`) and
`FinalizerConfirmationInputs` (cheap selection + connection profile + danger
settings + available profiles, threaded from the orchestrator composition point; the
project override is resolved above the seam into
`FinalizerChat::answer_confirmation_project_override`). **Timeouts are host-side**
(v4's 25 s / 60 s `withTimeout` — no tokio timers in the core, only the
failure→could-not-verify mapping is ported, the gate 500 ms-delay precedent).
Verified by `answer_confirmation_tier3_equivalence` — a jest real-DB oracle driving
v4's REAL `finalizeMessageResponse` with the feature ON over a 14-case corpus
(chat-OFF-beats-project-ON / project-ON-global-false / global-on / fully-off gate
matrix; the user-driven skip [`confirmed:null` + event, no LLM call — proven by the
canned-miss]; the no-checkable-inputs silent skip; whisper-only + whisper-plus-tool
references [an out-of-scope tool excluded]; the 24 K oldest-first truncation with a
non-ASCII row; consistent→true; inconsistent→standby→false+notes;
inconsistent→revise→true/revised+original-stashed+`content`-event;
revise-empty→null; check-parse-failure→null; and the dangerous escalation whose
recorded canned key proves the check's cheap-profile switch to OLLAMA/dolphin while
the re-affirmation stays on the character's ANTHROPIC/claude-sonnet), completions
pinned by oracle-recorded canned keys; each op's result + the ordered event trace
(`confirming`/`affirming`/`confirmationResult`/`done`) + `chats` / `chat_messages`
diffed (sentinel-aware minted timestamps; the `confirmation*` columns exact; the
minted TOOL-row ids placeholdered, everything else pinned). Plus 15 module
self-tests (all leaves + all `run_answer_confirmation` outcome bands over a canned
provider). `message_finalizer_tier3` + `orchestrator_tier3` re-verified green
against regenerated oracles (the orchestrator corpus keeps the feature OFF — the
gate is never active in the spine; the answer-confirmation differential drives the
finalizer directly with it ON). **Tracked deferrals:** the timeout timers (host);
the orchestrator spine's real cheap-LLM-selection / danger-settings / available-
profiles plumbing into `FinalizerConfirmationInputs` (the same seam boundary as the
compression `cheapLLMSelection` — the feature-off orchestrator corpus keeps the
inputs inert); `logLLMCall`'s `ANSWER_CONFIRMATION` log-type mapping (host-side).

**Wave 4 (W4.9a): the image-generation subsystem is DONE** (2026-07-06). v4's
long-deferred `executeImageGenerationTool` is ported end to end and dispatched.
New model boundary `model::image` (the tier-3 seam at v4's
`provider.generateImage(params, apiKey)`): the `ImageProvider` trait +
`CannedImageProvider` keyed by the exact merged request via `image_gen_key`
(`provider|model|<params JSON in v4 field order>` — the key proves
`mergeParameters` + `applyOrientation`, incl. the orientation-driven
`size`/`aspectRatio`), plus a SEPARATE `ImageTranscoder` seam for the WebP
transcode (no image-codec crate in the core — the `doc_blob` precedent;
`PassthroughTranscoder` the default + the differential's identical-both-sides
transcode). The three cheap-LLM tasks (`services::image_scene_tasks` —
`craftImagePrompt` [the placeholder detail block + style-trigger + aesthetic
sections, the quote-strip + UTF-16 truncation parser], `resolveAppearance` [the
per-character JSON selection parser], `sanitizeAppearance` [explicit → safe
rewrite]) run over the ported `CheapLlmTaskExecutor` with the byte-exact system
prompts in a GENERATED `prompt_text` submodule (mechanically extracted from v4 by
the checked-in `harness/oracle/cases/gen-image-scene-prompts.mjs`). Appearance
resolution (`services::appearance_resolution` — `resolveCharacterAppearances`
[the sceneState fast path, the trivial-case skip, the cheap-LLM call with the
dangerous-chat uncensored upgrade, the default fallbacks] + the FIVE-step
`sanitizeAppearancesIfNeeded` gate IN ORDER [OFF → pass; dangerous+uncensored →
pass; safe → pass; dangerous+uncensored-available → pass; dangerous+none →
sanitize] with `wasSanitized` flagged; wardrobe context via
`resolve_equipped_leaf_items_by_slot`, a new `leafItemsBySlot` sibling of the
title-only `resolve_equipped_outfit_values` in `tools::wardrobe_shared`,
imagePrompt preferred over title). The handler spine (`tools::generate_image`):
input validation + profile load/validate (API key via the `ApiKeyResolver`
seam), the Concierge integration composing W4.2 (prompt classification when
`scanImagePrompts`, expanded-prompt classification when `scanImageGeneration`,
the AUTO_ROUTE reroute via `resolve_image_provider_for_dangerous_content`, the
POST-HOC reroute on a provider moderation error via
`is_image_moderation_error` + `resolve_uncensored_image_profile_for_reroute` —
`isImageModerationError` COMPOSED from W4.2, not duplicated), `resolveOrientation`
mutating the merged params, and the tier-based prompt-expansion fallback chain
(original → craft → complete/long/medium/short/name on LLM failure). Placeholder
resolution (`{{me}}`/`{{I}}`/`{{char}}` = the caller, `{{user}}` = the other
participant, by-name/alias) reads the vault-overlaid `characters_read` on both
connections. `saveGeneratedImage` (base64 decode [Node `Buffer.from(x,'base64')`
semantics] → WebP transcode [the injected seam, off the writer thread] → SHA-256
→ the Lantern Backgrounds store write under `tool/` composing `link_blob_content`
+ `resolveUniqueRelativePath` + `ensureFolderPath` — the store id read from a new
`instance_settings::get_lantern_backgrounds_mount_point_id`, refusing to write
when the mount is unprovisioned per source → the `files` row [post-transcode
mime/size, width/height, `linkedTo=[chatId]`, `source='GENERATED'`,
`category='IMAGE'`, `generationPrompt`/`generationModel`/`generationRevisedPrompt`]
→ tag inheritance [`getInheritedTags`] → the Lantern notification, a recorded
`LanternNotificationSink` seam with the byte-exact string
`lantern_character_image_notification` ported HERE and handed to W4.6b). The
avatar trigger (`services::avatar_generation` —
`triggerAvatarGenerationIfEnabled`: the `avatarGenerationEnabled` gate + the
autonomous-chat skip + profile resolution [override → chat-level → global
default] + the `CHARACTER_AVATAR_GENERATION` enqueue via new
`queue_service::enqueue_character_avatar_generation` with `findPendingForChat`
dedupe) — **closing the W4.1d2 wardrobe deferral** (the corpus kept the flag
false; now the trigger is real and banked firing). `generate_image` is dispatched
through `BuiltInToolRunner` (removed from the loud-fallback set) via an ERASED
`ImageGenerationRunner` seam (an `Arc<dyn …>` boxed-future erasure over the
handler's seven generics, the `ErasedEmbeddingProvider` precedent; default
`NotConfiguredImageGeneration` returns v4's dispatcher `if (!imageProfileId)`
guard error "Image generation is not enabled for this chat"), threading the
generated-image descriptors (`{ id, filepath, … }`) into the already-ported
`process_tool_calls` image extraction + the finalizer link loop (the image
link-loop order `[firstToolMessageId, assistantMessageId]` banked live). Verified
by `image_generation_tier3_equivalence` — a jest real-DB oracle driving v4's REAL
`executeImageGenerationTool` over a two-DB fixture (characters + vaults + equipped
outfits + image profiles incl. an uncensored one + connection profiles + chat
settings with the Concierge scan flags + a provisioned Lantern Backgrounds store),
mocking ONLY the image provider (canned by exact request — the recorded key proves
the merged params incl. orientation), the completion boundary (recorded keys prove
all three task prompts + the classification prompts), Sharp/WebP (deterministic
pass-through both sides so the store bytes match `link_blob_content`), and the
Lantern notification; diffing the result object + `files` + the five mount-index
store tables (shared-cross-db id-map remap; content-addressed bytes carry the
minted file id) + `background_jobs` (avatar enqueue). `tool_dispatch_equivalence`
gained a `generate_image` row and the finalizer / orchestrator differentials were
re-verified green (the executor's new field/method/dispatch additive + inert on
the existing corpora); `wardrobe_tools` re-verified (its corpus keeps
`avatarGenerationEnabled` false, so the now-real trigger stays a no-op). **Tracked
deferrals (host / cross-subsystem seams):** the aesthetic subsystem
(`resolveAesthetic` / `resolveDepictionGuidelines` / `getProjectOfficialMountPointId`
— v4 error-swallows it [an ad-hoc image never breaks on a guidance read], so the
port supplies `None` and keeps the swallow shape; injectable later), `logLLMCall`,
the real WebP encoder (W4.7f-adjacent host work), and the personified Lantern
writer (W4.6b). The avatar + story-background JOB HANDLERS are the follow-up
**W4.9c** (below).

**Wave 4 (W4.9c): the avatar + story-background job handlers are DONE**
(2026-07-07), removing both job types (`CHARACTER_AVATAR_GENERATION` /
`STORY_BACKGROUND_GENERATION`) from the runner's loud fallback. New units:
**the two scene cheap-LLM tasks** (`services::image_scene_tasks`
`derive_scene_context` [no max-tokens] + `craft_story_background_prompt` [the
GROK 1000-char vs 1200 length guidance, the shared `buildAestheticSection`, the
`4000` max-tokens override], prompts regenerated byte-exact into
`prompt_text`); **the aesthetics module** (`services::aesthetics` —
`resolve_aesthetic` [tiered project-official → Quilltap General, 4000-char cap,
fail-soft], `get_project_official_mount_point_id` [the raw projects pointer],
`resolve_depiction_guidelines` [the Ariel Clause, per-character vault
`depiction-guidelines.md`]); **the avatar prompt builder**
(`services::avatar_prompt` `build_character_avatar_prompt` — the physical-text
preference chain, the `6b6e39ad` bare-top collarbone-crop branch that routes
AROUND `describeOutfit`'s "completely naked" fallback, the 600-char aesthetic
preamble) + a new `wardrobe::describe_outfit_with_omit` (the omit-aware variant
`describeOutfit` needs — an omitted-but-populated slot ≠ an empty slot; the
existing `describe_outfit` delegates with `omit=[]`, byte-identical); **the two
storage bridges** (`services::image_job_storage` — `write_character_avatar_to_vault`
[the character vault `images/history/`, throw if no vault] +
`write_lantern_background_to_mount_store` [the Lantern Backgrounds store
`generated/`, refuse if unprovisioned], both over v4 `storeMountFile`'s blob
branch); **the two handlers** (`services::{character_avatar_job,
story_background_job}` — each a `handle_*` core + a generic `*Handler`
`JobHandler` holding the seams) sharing `services::image_job_common`
(cheap-selection build, base64 decode, the job-path `build_job_gen_params`, the
`generate_with_reroute` post-hoc-moderation-reroute flow, the `ProjectImageUpload`
FsSeam); and **the enqueue side** (`queue_service::enqueue_story_background_generation`
[chat-level dedupe, priority −1] + `services::image_profile_resolution`
`resolve_image_profile_for_chat` [the four-tier chain] + `queue_story_background_if_enabled`
[the gate; the TITLE_UPDATE handler wiring point is documented — v5 has no
TITLE_UPDATE handler registered yet]). Added the `characterAvatars` /
`storyBackgroundImageId` / `lastBackgroundGeneratedAt` `ChatUpdate` setters (no
`updatedAt` bump). **Aesthetics differ per handler:** avatars use **aurora
only** (the Ariel Clause deliberately does NOT apply to avatars); story
backgrounds use **lantern + aurora + the Ariel Clause**. **The two storage-branch
keys differ:** avatar branches on `chat.projectId`, story on
`payload.projectId`. Both handlers reuse the W4.9a image subsystem (the
image/completion/moderation/transcoder/api-key seams, the Concierge pre-scan +
post-hoc moderation reroute, `resolve_orientation`) and the W4.8 job runner.
Verified by TWO jest real-DB tier-3 differentials
(`avatar_job_tier3_equivalence`, 9 cases; `story_background_job_tier3_equivalence`,
8 cases — plus a runner-registration E2E each) driving v4's REAL handlers over a
two-DB fixture (one fresh copy per case), mocking ONLY the model/infra seams
(canned image provider keyed on `provider|model|<params in `to_key_value` field
order>`, canned completions by recorded key, passthrough convertToWebP/
transcodeToWebP, canned orientation registry, canned api key, no-op logLLMCall,
frozen `Date`), diffing the mount-index store tables + `files` + `chats` +
`characters`/`projects` + the Lantern notification `chat_messages` rows in the
shared-cross-db-id-map remap form (minted store ids → tokens, minted timestamps →
`<ts>`, `chunkCount` pinned; the injected `now_ms` = the frozen clock so
`characterAvatars.generatedAt` / `lastBackgroundGeneratedAt` diff exactly).
**Tracked deferrals:** `logLLMCall` (not emitted — the `generate_image`
precedent; W4.7e owns it); the project-store `fileStorageManager.uploadFile`
branch (the injected host FsSeam); the avatar pre-scan classify + AUTO_ROUTE
prompt reroute and the story sanitize-gate danger path (kept OFF in the corpora —
proven byte-for-byte by the W4.2 danger differentials the handlers compose
verbatim; the post-hoc image-moderation reroute IS exercised). `generate_image`'s
aesthetic slots stay `None` (STOP-noted — the handlers are the primary
consumers). `image_generation_tier3` + `wardrobe_tools` re-verified inert.

**Wave 4 (W4.6a): the buildContext feeder closures are DONE** (2026-07-06).
Closed the READ/COMPUTE half of the `BuildContextSeams` trait
(`services::build_context`) — the ten former seams now run real, and the trait
is shrunk to only the W4.6b whisper-POSTing methods
(`post_host_off_scene_announcement` / `post_host_timestamp_announcement` /
`post_core_whisper` / `post_commonplace_whisper` [returns `posted`, gating the
persists] / `post_suparna_mail`). New feeder modules:
`services::frozen_archive` (`getOrComputeFrozenArchive` — effective-weight-ranked
top-25, process-cached per compaction generation, `localeCompare` id sort),
`services::memory_recap` (`generateMemoryRecap` = the tiered-memory narrative +
the vault conversation-summary recall lists over `search_document_chunks` /
`read_database_document` / `parse_frontmatter`; the two recap+distill prompt
bodies byte-exact in a generated `prompt_text` submodule) + its `distill`
submodule (`extractMemorySearchKeywords`, the object-or-bare-array parser with
the closed temporal/context vocabularies), `services::off_scene` (v4's Host
off-scene SCAN block — `characters.findByUserId` overlay read + the exclusion/
mention-scan/introduced-diff + the pure content builders `buildOffScene-
Characters{Content,OpaqueContent}` / `renderOffSceneCard` / `applyHostTemplates`
/ `findIntroducedOffSceneCharacterIds`; the Host POST is W4.6b), `services::core_whisper`
(Aurora's `resolveCoreWhisperConfig` [chat→character→global] + `assembleCorePacket`
reading the character's own + every group's shared `Core/**.md` [the recursive
folder read + case-fold dedup] + `stripFrontmatterBody` + the three content
builders; the POST + stale-sweep are W4.6b), `services::suparna_mail` (the mail
READ — `collectUnalertedMail` + `markAlerted` [the double-announce flip] +
`buildSuparnaMailLLMContext`; the POST is W4.6b), and
`services::scene_state_tracking` (the `updateSceneState` cheap-LLM task + the two
scene-state prompt bodies byte-exact + `capClothingSummary`). **Closed with
existing code:** the tiered mount pool (`resolve_tiered_mount_pool`, no
ownership/participant tier — v4's buildContext call passes neither),
`getMemoryRecallSettings` (`instance_settings::get_memory_recall_settings`, v4's
`down-weight` default; read faithfully, the search-leg re-rank still deferred),
and the live-wardrobe clothing override (adding the pure leaves
`hash_equipped_slots` / `has_equipped_items` / `decorate_outfit_items_title_only`
to `crate::wardrobe` + a `resolve_equipped_outfit_leaf_values` variant returning
raw items per slot, so the `titleOnly` imagePrompt-preferring decoration matches
v4). The **scene-cache + recall-history persist writes**
(`chats.update({ commonplaceSceneCache })` / `{ commonplaceRecallHistory }`) are
ported directly (three new `ChatUpdate` setters: `sceneState` /
`commonplaceSceneCache` / `commonplaceRecallHistory`), gated on the commonplace
POST's `posted` (matching v4's `if (posted)`); the prior-emission `_unchanged_`
compaction reads the same `commonplaceSceneCache` column. New scoped reads:
`chats_read::find_core_whisper_overrides`,
`characters_read::find_core_whisper_enabled`,
`groups::find_name_and_official_mount_point_id_raw`, and the recursive
`doc_mount_documents::find_many_by_mount_points_in_folder_opts`. **Verified:**
`build_context_tier3_equivalence` runs green with the feeder jest mocks DROPPED
one-for-one against the real feeders (the base fixture's memories now feed the
real frozen archive; the orchestrator fixture's vault conversation summaries feed
the real recap) — plus a new **`context_feeders_leaves_equivalence`** tier-1
differential proving the pure builders/formatters/config resolvers byte-exact
against v4's REAL exports (off-scene content/opaque, core-whisper config + the
three builders, `renderRelevantConversationsBlock`, `buildSuparnaMailLLMContext`
[TZ=UTC harness seam], `capClothingSummary`); `knowledge_injector` /
`first_message_context` / `orchestrator_tier3` re-verified green. **Tracked
deferrals:** (1) the orchestrator spine still passes `cheap_llm_selection: None`
into buildContext (it threads only a `cheap_llm_settings_present` bool for
compression), so the recap/distill feeders are gated OFF there and stay mocked in
the orchestrator oracle — v4's orchestrator DOES resolve a `cheapLLMSelection`
(the corpus has vault summaries → recap fires), so un-mocking would surface a
spine plumbing gap, not a feeder bug (the recap/distill feeders themselves are
proven real in `build_context_tier3`); closing this is the spine owner's
follow-up (thread a resolved `CheapLlmSelection` at the orchestrator composition
point). (2) The scene-state-tracking JOB WRAPPER (`handleSceneStateTracking` —
danger pre-classification + wardrobe baselines + the clothing-hash cache
reconciliation + `createSystemEvent` token tracking + the persist) lands with the
W4.8 runner-dispatch row (this unit ports the cheap-LLM task it drives).

**Wave 4 (W4.4a4): the Courier transport + the compression-cache spine plumbing
is DONE** (2026-07-07). Ported v4's `courier-transport.service.ts` (the manual /
clipboard dispatch) as `services::courier_transport` + `courier::render_markdown`:
the two Markdown renderers (`renderCourierRequestAsMarkdown` /
`renderCourierDeltaAsMarkdown` — byte-exact, the `\n{3,}`→`\n\n` collapse + JS
`trimEnd()+'\n'` via `jsstr::js_trim_end`, the `escapeFilename` bracket/paren
escapes, `formatBytes` sizes), `buildCourierDeltaEvents` (the per-character
checkpoint scan — the strict `createdAt <= resolvedAt` skip [equality matters],
the checkpoint-message defensive exclusion, targeted-whisper filtering [surfaced
only when the responder is sender OR target], the exact Staff speaker-label map,
`repos.files.findById` attachment loading with skip-on-missing, dedup by fileId),
and `dispatchCourierTransport` (always render the full bundle; delta primary +
full fallback when `courierDeltaMode !== false` AND a checkpoint exists; the
union attachments; the placeholder ASSISTANT message [`content:''`, the bundle in
`pendingExternalPrompt`, provider/model from the effective profile] through the
ported `chats_messages::add_message`; `chats.update({isPaused, updatedAt})`; the
`pendingExternalTurn` frame FIRST then `done{pendingExternalTurn:true}` — both
byte-exact). The paste/cancel resolvers (`resolve_external_turn` /
`cancel_external_turn`) are public service functions composing `update_message` +
`chats.update` (checkpoint advance + unpause) + the ported finalizer triggers
(memory extraction / danger classification / summary check, awaited per the
watermark precedent). Added to `chat_events`: the `PendingExternalTurn` variant +
`DonePayload.pending_external_turn` (serialized after `model_name` to match v4's
`{…, provider, modelName, pendingExternalTurn}` order). Added the
`ChatUpdate.courier_checkpoints` write setter (the read + create-write marshaling
already existed — no drift). The orchestrator courier gate (was erroring) is
CLOSED: after `build_message_context` + the `preparing` status, an effective-courier
turn dispatches — the tool build is SKIPPED (`is_effective_courier` guards
`build_tools` / `resolved_tool_mode` / `tool_instructions`, matching v4 so the
bundle's system prompt carries no tool instructions) and the turn halts on
`isPaused=true` (the frozen `should_chain_next` already stops on paused). **Compression
plumbing:** the finalizer's real `AsyncCompressionTrigger` (now an async `&self`
seam over `compression_cache::trigger_async_compression`; `RealAsyncCompression`
holds db+completion+executor, `NoAsyncCompression` the no-op) computes + persists
the cache when the gate fires (the finalizer builds `updatedMessages` = visible
history + user msg + assistant reply + the byte-exact options); and the
`build_context` cached-compression window (`cached_compression_result` /
`cached_compression_message_count` — phase-1 uses a warm cache verbatim [no sync
compression call → no canned key consumed] + the dynamic effective-window sizing
from the cache's message count). The orchestrator reads `get_cached_compression`
before buildContext (per v4's `runPreContextPreCompute` compressionTask) but stays
inert until the spine threads a real `cheap_llm_selection` (the same tracked
deferral as W4.6a's recap/distill; `build_context_tier3`'s warm-cache case proves
the code path directly). Verified: a NEW `courier_transport_tier3_equivalence`
(drives v4's REAL `dispatchCourierTransport` over a two-DB fixture, four cases —
first-send full bundle / delta with whisper-filter-both-directions + the `<=`
boundary + the `[Staff: The Commonplace Book]` label / `courierDeltaMode:false`
forced-full / attachment union — diffing the `ProcessMessageResult` + the SSE
trace + the persisted placeholder bytes [the bundle proves the renderers] +
`isPaused`, the minted placeholder id normalized); a `courier_send` case added to
`orchestrator_tier3` (the spine branch end-to-end through `handleSendMessage` —
proving the courier tool-skip's effect on the bundle + the frame order incl.
`preparing`); a warm-cache case in `build_context_tier3`; the finalizer trigger
adaptation in `message_finalizer_tier3` (the gate proof — the trigger no-ops at
this message count, so persistence is proven by `compression_cache_tier3`
separately); `compression_cache_tier3` re-verified. **Tracked deferrals:** the
paste/cancel route handlers aren't exported (Phase-4 HTTP transport) — their
constituent repo ops are tier-2/tier-3-proven and the ported service functions are
unit-tested; the orchestrator spine's `cheap_llm_selection` threading (shared with
W4.6a) keeps the cached-compression read inert in `process_message`.

**Wave 4 (W4.6b): the post-office / personified whisper writers are DONE**
(2026-07-07). Every v4 `lib/services/<persona>-notifications/writer.ts` is ported
into a new `services::<persona>_notifications` module, each `post*` posting ONE
`chat_messages` row through the ported `add_message` (the `carina_runner::writer`
idiom — a `serde_json::json!` `MessageEvent` literal → `ChatEventInput` → the
single-writer channel, Err→None, error-swallowing/`!content.trim()` early returns
preserved) with the exact `systemSender`/`systemKind`/targeting tuple read
verbatim from source. The steampunk/Wodehouse voice strings are byte-exact.
**Host** (`host_notifications`, systemSender `host`): the full add/remove/
status-change/scenario/user-character/multi-character-roster/silent-mode/
join-scenario/timestamp/off-scene-characters/continuation/merge/no-user-character
family (`hostEvent` payloads on add/remove/status/off-scene; `add` reads the
joining character's vault `identity.md`), reusing the W4.6a `off_scene` builders +
`message_formatter::build_multi_character_context_section`. **Prospero**
(`prospero_notifications`, `prospero`): connection-profile-change +
project/general/group/vault context re-injection posts + `postProsperoCarinaError`
(closes `carina_runner::PostProsperoCarinaError`) + the DB context loaders over
ported repos. **Librarian** (`librarian_notifications`, `librarian`): every
open/rename/save/delete/folder/attach/write/move/copy/blob/upload/summary
announcement + `contentHiddenFromCharacters`/`documentHiddenFromCharacters`; the
`summary` post carries `summaryAnchor {compactionGeneration}` and re-exports the
canonical `SUMMARY_CONTENT_PREFIX`. **Concierge** (`concierge_notifications`,
`concierge`/`danger`): danger (with classifier details, reusing
`gatekeeper::category_label` + `jsnum::to_fixed`) + the four manual-transition
kinds. **Suparṇā** (`suparna_notifications`, `suparna`/`mail-delivery`): the
persona-voiced `buildSuparnaMailWhisper` (distinct from the W4.6a LLM context) +
targeted non-opaque post + `surfaceOperatorMailForChat` (`mark_alerted` AFTER the
post). **Aurora** (`aurora_notifications`, `aurora`): `postCoreWhisper`
(`core-whisper`, targeted, reusing the W4.6a `core_whisper` builders) + the
opening/change outfit whispers + the `WARDROBE_OUTFIT_ANNOUNCEMENT` drain
(`flush_pending_wardrobe_announcements` over the `Arc<Mutex<HashSet>>` +
`queue_service::enqueue_wardrobe_outfit_announcement` + the
`handle_wardrobe_outfit_announcement` job body — closes the W4.1d2 deferral).
**Commonplace** (`commonplace_notifications`, `commonplaceBook`): the persona/LLM
whisper builders (the canonical home; `build_context.rs` keeps a private 5-key
copy — dedup is a handoff) + `postCommonplaceWhisper` (`opaqueContent ?? null`) +
`refreshRelevantConversationsOnFold` (per-target prior sweep, reusing the W4.6a
recap/relevant-conversations reads). **Lantern** (`lantern_notifications`,
`aurora`/`lantern` by kind): the image notification + `isLanternImageAlertEnabled`.
Plus the two non-whisper closures: the **conversation-summary vault bridge**
(`conversation_summary_vault_bridge` — `writeConversationSummaryToVaults` /
`removeConversationSummariesFromVaults` over the ported document store +
`serialize_frontmatter`, spanning both DBs; per-character best-effort;
`is_conversational_message` / `compute_conversation_stats`) and
`delete_conversation_with_vault_sweep` (participants captured BEFORE the row
delete + the `syncVaults` skip) — **closing the LAST Phase-2 deferral** — and
**cost events** (`cost_events` — `createSystemEvent` + the memory/title/context
wrappers: a SYSTEM row + the ported `increment_token_aggregates` bump). **Verified:**
six tier-1 pure-builder differentials (`post_office_{host,librarian,prospero,
commonplace,aurora,concierge_lantern_suparna}_equivalence`, byte-exact vs v4's real
exports); the combined `post_office_writers_tier3_equivalence` (drives v4's real
`post*` + `createContextSummaryEvent`/`createTitleGenerationEvent` over a two-DB
fixture — real vaults for Host `add` — diffing `chat_messages` byte-for-byte + the
cost `chats` aggregate, one case per row-shape/systemKind: public opaque-pair,
targeted, `hostEvent`, `summaryAnchor`, non-opaque, null-opaque, SYSTEM); and the
`vault_summary_mirror_tier2_equivalence` (mirror + rename-in-place-by-conversationId
+ dedup + `syncVaults:false` skip + the delete sweep, five mount-index tables in the
shared-cross-db id-map remap form, `content` byte-diffed). **Non-spine seams closed
LIVE:** the Concierge announcer seams in `dangerous_content`
(`RealDangerAnnouncer`/`RealConciergeAnnouncer` — the W4.2
`postConcierge{Danger,Manual}Announcement` deferrals; `danger_gatekeeper_tier3` +
the manual-flip case regenerated with the bubbles posted on both sides) and the
context-summary Librarian re-post + cost events (`RealContextSummarySeams`;
`context_summary_service_tier3` regenerated). The announcer/`ContextSummarySeams`
traits went async (RPITIT `-> impl Future + Send`, no boxing);
`generate_context_summary`'s public no-seams path stays `NoopSeams`, so the spine
callers are untouched. **Tracked handoffs (spine-owned, deferred):** wiring the
`BuildContextSeams::post_*` (`post_core_whisper` [Aurora] /
`post_commonplace_whisper` [Commonplace] / `post_host_off_scene_announcement` +
`post_host_timestamp_announcement` [Host] / `post_suparna_mail` [Suparṇā, which
must switch to `build_suparna_mail_whisper` over the letters]) + the
`OrchestratorSeams::post_prospero_context` + the end-of-turn
`flush_pending_wardrobe_announcements` into the orchestrator/build_context spine
(each with its corpus case) + the `WARDROBE_OUTFIT_ANNOUNCEMENT` job-runner
dispatch row (W4.8); the context-summary `mirror_summary_to_vaults` +
`refresh_relevant_conversations` seams (need vault fixtures + embedding — the
mirror is separately proven by `vault_summary_mirror_tier2`); rewiring the image
subsystem's Lantern sink (`generate_image::lantern_character_image_notification` is
a truncated placeholder) to the full byte-exact `lantern_notifications::build_content`
(regenerates `image_generation_tier3`); the `build_context` private commonplace
builder dedup; and the Librarian save-announcement `change:{kind:'edited',diff}`
coupling in the doc-edit handlers (`doc_edit::unified_diff`/`line_diff` are ready
per W4.d1).

**The Phase-3 endgame is fully planned (2026-07-06).** Every remaining unit
has an agent-ready work order checked in under
`docs/developer/porting/work-orders/` — W4.2u (the danger spine unification
above), W4.3 (answer-confirmation), W4.4a4 (courier + the compression spine
plumbing), W4.4b (file/attachment — closes `process_files` + the Lantern K
seam; bytes/resize are injected host seams), W4.5 (carina query — closes
`RunCarinaQuery` + wires `ask_carina`; the Brahma one-shot console is the
W4.5b follow-up), W4.6a (the buildContext feeder closures — recap / distill /
frozen archive / core-whisper packet / Suparṇā mail read / off-scene scan /
scene-state; the mount-pool + recall-settings seams close with already-ported
code), W4.6b (the post-office personified writers + the vault summary mirror
+ the `chats.delete` sweep [the last Phase-2 deferral] + cost events [SYSTEM
rows through the ported `add_message` — verified: no new table]), W4.7a–f
(the provider layer, decomposed in `provider-manifest.md` §"The porting
decomposition": manifest+registry, the five sans-IO stream decoders, request
builders/transforms/tool-wire [closes `ToolCallDetector`, `formatTools`, the
text-markers strategy], transport/errors/`api_keys` [the one unported table],
pricing/capability/`logLLMCall`/embeddings, image dialects + moderation +
web search), W4.8 (the job runner — decided: the fork/IPC/buffered-proxy
architecture does NOT port; in-process handlers over the single-writer `Db`,
cadence host-driven, closes `ensureProcessorRunning` + the `markCompleted`
merge deferral), and W4.9a/b (image generation with a new `model::image`
canned seam + the photo trio; the avatar/story-background job handlers are
W4.9c). Then Unit 4, the enclave (`docs/developer/porting/enclave-engine.md`:
`step()` = the ported turn handler, one transition per claimed job, the turn
on the `write_apply` main-primary path, cadence host-side). The batch table
and the per-round parallelism rules (the spine files and the orchestrator
oracle corpus each have exactly ONE owner per round; start every round with a
v4 drift check) are in `docs/developer/porting/chat-orchestration.md`.

**Drift check (2026-07-07): v4 `8617ce7a..6b6e39ad` (1 commit) audited — no
ported unit is stale.** `6b6e39ad` ("take generated-image description off the
reply hot path; fix bare-topped avatars") touches only PENDING surfaces, and
both work orders are retrofitted to the new truth: (1)
`lib/chat/file-attachment-fallback.ts` (→ **W4.4b**) — `generateImageDescription`
now tries persisted text FIRST (`files.findById` →
`generationRevisedPrompt.trim()` → `generationPrompt.trim()` →
`description.trim()`, returning `reusedPersistedDescription: true` with no
vision call; the columns it reads are ported marshaling, and W4.9a's
`saveGeneratedImage` already writes the `generation*` fields), and the vision
fallback is hardened (downsize to the DESCRIPTION provider's limit, the
`IMAGE_DESCRIPTION_INSTRUCTION` constant, a 60 s host-timing timeout, and
best-effort `logLLMCall` type `IMAGE_DESCRIPTION` on success AND failure — a
new call site for the W4.7e logLLMCall closure; the log type is corpus-only
for the ported TEXT column). The W4.4b order's spec + corpus list are
updated. (2) `lib/wardrobe/avatar-prompt.ts` (→ **W4.9c**) — the bare-top
branch routes around the ported `describeOutfit`'s "completely naked"
fallback (accessories-only call or `''`) and swaps in the collarbone-crop
intro; `describeOutfit` itself is UNCHANGED (the ported leaf stays valid).
Drift note added to the W4.9a order's W4.9c scope. Also: `help/chat-settings.md`
is help-content data (not a ported surface). The `docs/v4/` CHANGELOG mirror
is refreshed. **New oracle baseline for in-flight/future orders: `6b6e39ad`.**

**Round-3 unification (Phase B) — ALL groups (1–8) DONE; Round 3 COMPLETE.** The three
Round-3 units (W4.4a4 courier, W4.6b post-office writers, W4.7c request builders)
landed with their spine seams INERT; this pass wires them live.
**Group 1 (W4.7c tool reshape/detector/strategy):** `tool_build::build_tools`
now applies `format_tools_for_provider` as its final step (Anthropic `input_schema`
etc. at the wire; OPENAI passthrough keeps `tool_build_equivalence` green), and the
spine constructs `RegistryToolCallDetector::built_in()` + gates the provider-text
pass on `provider_has_text_markers` internally (the `tool_detector` /
`provider_text_strategy` fields dropped from `OrchestratorDeps`).
`orchestrator_tier3` regenerated with the real provider registry initialized on the
v4 side (both reshape identically; the tools-at-wire assertion compares the reshaped
slate). **Group 2 (W4.6b whisper writers):** `BuildContextSeams` is now async
(RPITIT) with a `RealBuildContextSeams` impl delegating to the W4.6b writers —
core-whisper + commonplace (with v4's stale-whisper sweeps), host timestamp +
off-scene (the off-scene scan now returns the newcomer cards so the writer builds
the announcement + `introducedCharacterIds`), and Suparṇā mail (built from the
unalerted letters, targeted at the responding participant); the commonplace `posted`
still gates the scene-cache/recall-history persists. The Prospero cadence block
(public announcement + group-context whisper) is wired directly into the spine (the
`post_prospero_context` seam dropped). `build_context_tier3` + `orchestrator_tier3`
regenerated with the writers un-mocked (commonplace / host / prospero group-context
whisper rows now appear in the orchestrator's diffed `chat_messages` dump).
**Group 3 (end-of-turn wardrobe drain):** the spine threads ONE shared
`pendingWardrobeAnnouncements` set through every per-turn tool context and drains it
at turn close (v4 `orchestrator.service.ts:1406`) via
`flush_pending_wardrobe_announcements`; added `WardrobeOutfitAnnouncementHandler`
(a `JobHandler`) for the runner registry. **Group 4 (Lantern sink):** deleted the
truncated `lantern_character_image_notification` placeholder; `LanternNotificationSink`
is now async with a `RealLanternNotification` delegating to the byte-exact W4.6b
`post_lantern_image_notification`; `image_generation_tier3` regenerated with the
persisted `character-image` content diffed byte-exact (incl. the tail the
placeholder dropped). **Group 5 (commonplace dedup):** removed the private
commonplace builders from `build_context.rs`, reusing the canonical
`commonplace_notifications` versions (byte-identical for the per-turn whisper).

**Round-3 Phase B — Groups 6, 7 & 8 now DONE — Round 3 COMPLETE.**
**Group 8 (`cheap_llm_selection` spine threading) — DONE + green.** The
`processMessage` spine resolves a real `CheapLlmSelection` at the composition point
(v4 `getCheapLLMProvider` over the user's connection profiles + the chat settings'
`cheapLLMSettings`, registry-cheapest seam injected `None`) and threads it into
`BuildContextArgs` (the recap/distill feeders + the cached-compression window) + the
finalizer's `FinalizerCompression` (async-compression trigger) + the dangerous-path
`uncensoredFallbackOptions`. `orchestrator_tier3` regenerated dropping the
`generateMemoryRecap` + `extractMemorySearchKeywords` mocks one-for-one: v4's real
recap yields empty content (no memories/vault summaries seeded), and the distill
feeder now fires **61 live cheap-LLM calls** across the 22 cases — each replayed
byte-for-byte by the Rust distill (proving the spine-resolved selection matches v4);
the empty `memories` table + the corpus's `compression_enabled: false` keep the
stream canned keys uncascaded. `regenerate_swipe_tier3` re-verified.
**Group 7 (context-summary vault-mirror + relevant-conversations-refresh LIVE) —
DONE + green.** `RealContextSummarySeams::mirror_summary_to_vaults` /
`refresh_relevant_conversations` (no-ops) now run live: the fold mirrors the summary
into every participant vault (`writeConversationSummaryToVaults`) then re-runs the
relevant-past search against it (`refreshRelevantConversationsOnFold`), in that order.
The seam methods take the built inputs (participant char ids + `compute_conversation_stats`
+ `connection_max_context`) and `RealContextSummarySeams` is generic over an embedding
provider. `context_summary_service_tier3` extended to a two-DB fixture (main +
mount-index with one provisioned vault + a pre-seeded prior summary whose chunk
carries a canned unit embedding); the mirror is diffed by the `doc_mount_file_links`
path set (`Conversation Summaries/Old Title A.md` on both sides), the refresh's
`relevant-conversations` whisper by the `chat_messages` dump. Oracle un-mocks the
mirror/refresh one-for-one (incl. un-mocking `character-vault-bridge` so
`getCharacterVaultStore` resolves the real minted vault — [[jest-real-db-oracle]]).
`vault_summary_mirror_tier2` + `orchestrator_tier3` re-verified.
**Group 6 (Librarian doc-save `change:{created,body}`/`{edited,diff}` coupling) —
DONE + green** (v4 commit `8617ce7a`). The five mutating doc-edit write handlers
(`doc_write_file`/`doc_str_replace`/`doc_insert_text`/`doc_update_frontmatter`/
`doc_update_heading`) now emit the Librarian doc-save announcement: `doc_write_file`
captures the pre-image before writing (absent → `Created{body}`, present →
`Edited{diff}`), the four edit handlers build `Edited{diff}` via the W4.d1
`generate_unified_diff`. Ported `resolveActorOrigin` (`doc_edit::shared::resolve_actor_origin`:
ByUser vs ByCharacter+name via the slim `characters_read::find_by_id_raw` — `name` is
not vault-overlaid, so it matches v4's overlaid `findById().name`) + a
`librarian_scope_from` DocEditScope→LibrarianScope mapper. Added a
`#[serde(skip)] pending_librarian_announcement: Option<LibrarianWriteAnnouncement>`
field to the shared `DocEditToolResult` (never serialized — v4 puts `change` only in
the announcement call, not the tool result, so the ~23-handler result shape is
byte-unchanged) + a `with_librarian_announcement` chainable setter; each write
handler builds the announcement inside the synchronous `execute_doc_edit_tool`
`Db::write` closure and stashes it there, and the executor spine (`run_doc_edit`)
`take`s it out of the closure return and posts it via the already-ported
`post_librarian_write_announcement` AFTER the closure returns (the wardrobe-drain
`pending*` precedent — NOT a new write model; best-effort, a failed post never fails
the tool). Refactored the writer's message assembly into a shared
`build_librarian_message` + `write_announcement_message`, and added the synchronous
`post_librarian_write_announcement_conn` (posts over an already-held RW `main`
connection) so the direct-drive `doc_text` differential posts the same row. `doc_text`
regenerated with the write announcement LIVE on the v4 side (un-mocked
`postLibrarianWriteAnnouncement` + `contentHiddenFromCharacters`/`documentHiddenFromCharacters`;
the fixture's existing chat+participant now targeted) and a THIRD dumped table — the
MAIN-db `chat_messages` (ordered by `content`, a remap-invariant key with no minted
uuid/timestamp in the persona body) — diffing the 10 Librarian rows (8
edited-by-character + 2 created-by-character) byte-for-byte (persona content + opaque
content + `systemSender:'librarian'` + per-kind `systemKind` + null targeting).
`doc_fm`/`doc_ui`/`doc_blob`/`doc_enum`/`tool_dispatch` re-verified green (the additive
`#[serde(skip)]` field is `None` for every non-write handler; those corpora never
invoke a write handler). (The G6 deferral — the remaining file-management / blob /
open Librarian announcements — is now CLOSED by **W4.6c**, below.)

**W4.6c: the remaining Librarian doc-edit announcements are DONE** (2026-07-07),
closing the Round-3 Group-6 leftover. The file-management, blob, and document-UI
handlers now emit their announcements — **move / copy / delete / folder-created /
folder-deleted / open / blob-write**. The G6 field was generalized from
`Option<LibrarianWriteAnnouncement>` to `Option<PendingLibrarianAnnouncement>` (an
enum with one variant per announcement kind, each carrying the frozen W4.6b writer's
argument struct; still `#[serde(skip)]`, so the ~23-handler serialized result shape
is byte-unchanged), plus `From` impls and a chainable `with_librarian_announcement`.
Each database-store handler branch builds its announcement inside the synchronous
`execute_doc_edit_tool` `Db::write` closure (it needs the RW connections for
`uri_for_resolved_path` / `resolve_actor_origin` / a new synchronous
`document_hidden_from_characters` handler helper) and stashes it; the executor spine
(`run_doc_edit`) `take`s it out and dispatches by kind to the matching async
`post_librarian_*_announcement` after the closure. `doc_open_document` ports v4's
**bespoke** open-origin resolution (`characters.find_by_id_raw` name → `opened-by-
character` else `opened-by-user`, NOT the shared `resolve_actor_origin`); `doc_move_folder`
passes no `hiddenFromCharacters` (→ `false`, matching v4's folder-move site, unlike
the file-move site); `doc_delete_blob` fires the shared delete announcement with
scope `document_store`. Added the synchronous `post_librarian_*_announcement_conn`
siblings for the seven writers that lacked one (over a shared `post_librarian_message_conn`)
+ a `post_pending_librarian_announcement_conn` dispatcher so the direct-drive
differentials post over the held RW `main` connection. Regenerated `doc_fm` /
`doc_blob` / `doc_ui` with the announcement writers LIVE (un-mocked) and a MAIN-db
`chat_messages` dump added to each (ordered by `content`), diffing the Librarian rows
byte-for-byte (7 file-management / 3 blob / 2 open rows). The open announcement is an
actual `type:'message'` event, so it bumps the chat's `updatedAt` on both sides — the
doc-ui "updatedAt never bumped by open/close" pin is retired accordingly (the
`chat_messages` dump now proves the announcement posted on both sides). `doc_text` +
`tool_dispatch` re-verified green (the enum generalization is inert for the write kind
and for the non-announcing read handlers). **Tracked deferrals (unchanged):** the
filesystem-mount announcement sites stay behind the existing `FsSeam` (never execute
for database stores); `syncChatDocuments*` stays the corpus-verified no-op seam.

**Round-4 prep (2026-07-07): every remaining work order is written.** The five
orders that lacked specs are now agent-ready under
`docs/developer/porting/work-orders/`, each from a fresh v4 survey at
`6b6e39ad`: **W4.7d** (transport + errors + the `api_keys` table — the LAST
unported repo; survey facts: it is a hand-rolled PLAINTEXT collection inside
v4's `ConnectionProfilesRepository`, `provider` is a free-form string, and v4
has NO transport-tier timeout/abort/retry anywhere — SDK defaults apply;
the google `config → generationConfig` wire split + the `{args,name}` reorder
close here), **W4.7e** (pricing fetcher / `checkModelSupportsTools` /
`logLLMCall` / embedding wire; plan correction: the BUILTIN TF-IDF vectorizer
is NOT ported — only the `tfidf_vocabulary` storage repo is — so it's a
splittable sub-unit; the 19-variant log-type enum's `TOOL_CONTINUATION` has no
emitter; `stableStringify` SORTS keys, unlike every other ported serializer),
**W4.7f** (image dialects + moderation + web search; plan corrections: FIVE
image providers — z-ai was omitted — and the refusal-keyword gap covers
openrouter AND google-gemini AND z-ai, all faithful; only Google Imagen
manufactures a moderation error, the others are recorded upstream SDK
throws), **W4.9c** (the avatar + story-background job handlers — job types
`CHARACTER_AVATAR_GENERATION` / `STORY_BACKGROUND_GENERATION`; ports the two
remaining scene tasks + the REAL aesthetics module [`resolveAesthetic` /
`resolveDepictionGuidelines` — avatar is aurora-only/no-Ariel, background is
lantern+aurora+Ariel] + `buildCharacterAvatarPrompt` with the `6b6e39ad`
bare-top branch + the storage bridges + the story enqueue/gate), and
**W4.6c** (small — the Round-3 Group-6 leftover: the file-management / blob /
open Librarian announcements, threaded via a generalized
`PendingLibrarianAnnouncement` enum on the `#[serde(skip)]` result field).
The Round-4 lane layout + contention notes (f→d api-key dependency; the
W4.5∥W4.6c `tools/executor.rs` overlap; e's spine handoff) are in
`chat-orchestration.md`; the decomposition corrections are in
`provider-manifest.md`. v4 HEAD is still `6b6e39ad` — no drift; the oracle
baseline is unchanged.

**Wave 4 (W4.7e): pricing / capability / logging / embeddings — sub-units 1–4
DONE and green; sub-unit 5 split to W4.7e2** (2026-07-07). Four of the five
W4.7e sub-units are ported, each with a differential against v4's REAL code:

- **The LLM logging service** (`services::llm_logging`, v4
  `llm-logging.service.ts`) closes the standing `logLLMCall` host-side deferral.
  `summarize_request`/`summarize_response` (FULL content, UTF-16 `contentLength`,
  `hasAttachments`, `toolCalls` mapped `{name, arguments}` only when present),
  `is_logging_enabled` over the ported `chat_settings::find_by_user_id` (logs BY
  DEFAULT — a missing settings row, a missing `llmLoggingSettings`, and a read
  error all → enabled; skip ONLY when settings exist and `enabled` is explicitly
  false), the row writer `log_llm_call` (mint id + timestamps → the ported
  [`db::llm_logs`] `create` on the llm-logs writer partition; usage built only
  when a token field is present then `?? 0`, cacheUsage passed through, request
  hashes kept only when a tier is present, `rawProviderUsage` null-collapsed to
  SQL NULL; never throws), `map_task_type_to_log_type` (verbatim incl. the
  `|| 'SUMMARIZATION'` default), and the 19-variant `LLMLogType` string constants
  (`TOOL_CONTINUATION` carried but never emitted — UI-only in v4). **The
  autonomous-run id is an explicit `LogContext` field**, not a thread-local:
  v4's `AsyncLocalStorage` (`autonomous-run-context.ts`) does not port to the
  scheduler-free core — the enclave/job-runner supplies it when Unit 4 lands,
  every request-path caller passes `LogContext::none()`.
- **The cache-prefix hashes** (`cache_prefix_hashes`, v4
  `cache-prefix-hashes.ts`, `request_prefix_hashes_equivalence`, 17 rows): the
  per-tier SHA-256 (first 16 hex) of the cacheable request regions
  (`systemBlock{1,2,3}` / `toolsArrayHash` / `historyTailHash`). Reproduces the
  **sorted-key `stableStringify`** — the OPPOSITE of every insertion-order JSON
  serializer in this port, named distinctly so nobody reuses the wrong one — and
  the history-tail mapping's **`undefined`-renders-literally** quirk (a message's
  absent `name`/`toolCallId`/`toolCalls` renders as the literal text `undefined`,
  NOT omitted, NOT `null`, because v4's `.map` assigns the keys before
  `stableStringify` coerces `undefined`).
- **The pricing fetcher + cost estimation + `checkModelSupportsTools`**
  (`services::pricing_fetcher`, v4 `pricing-fetcher.ts` +
  `cost-estimation.service.ts` + `pseudo-tool-support.ts`,
  `pricing_fetcher_equivalence`, 6 scenarios): **sans-IO** — the fetch is the
  injected `PricingFetch` seam (raw response JSON handed back), `now_ms` explicit,
  the caches per-instance (v4's module-globals — the `CheapLlmTaskExecutor`
  precedent). Ports **JS `parseFloat`** string-price semantics (leading-prefix
  parse, garbage → NaN propagates through `× 1e6`), the **two OpenRouter response
  casings as separate parsers** (public snake_case vs SDK camelCase — never
  unified), the 24 h TTL + the 5 min NEGATIVE cache
  (`openRouterPublicFetchFailureAt` — `[]` inside the window, reset on success),
  `PROVIDER_TO_OPENROUTER_SLUG` + exact-then-fuzzy match, `findCheapestAvailableModel`
  filters, and the **`estimateMessageCost` cascade** with all source tags
  (`openrouter` / `registry` / `openrouter-estimate` / `unavailable` all banked;
  `fallback` is shadowed by the registry substring superset, faithful to v4) —
  closing the finalizer's cost-estimation evidence seam. The
  `LEGACY_FALLBACK_PRICING` rows (18, ANTHROPIC/OPENAI/GOOGLE/GROK; the other
  three empty) are a **generated** Rust static
  (`harness/oracle/pricing/gen-fallback-pricing.mjs`, the tool-catalog
  transcription precedent). The differential drives v4's REAL async exports with
  `global.fetch` / `@openrouter/sdk` / the repos mocked + a stepped clock; the
  Rust side replays each scenario on a fresh `PricingFetcher` + a scripted
  `SeqFetch`. **`model_supports_native_tools`** (the injected orchestrator field)
  is now backed by `check_model_supports_tools`; the field removal is handed to
  **Round-4's spine owner (W4.4b)** per the work order.
- **The embedding wire** (`model::embedding_wire`, the plugin embedding providers,
  `embedding_wire_equivalence`, 12 rows): sans-IO per-provider **request builders
  + response parsers** (the W4.7c `BuiltRequest` precedent) — OpenAI
  (`POST {base}/embeddings` `{model, input, dimensions?}`, `data.data[0].embedding`
  + snake_case usage, the `error.error?.message || statusText` message), Ollama
  (the up-front empty/whitespace throw, `POST /api/embed`
  `{model, input, truncate:true, options:{num_ctx}}` with the `/api/show`-derived
  `num_ctx` [scan `model_info` for a `.context_length` key, `min(ctx, 16384)`,
  fail → 8192 `derived:false`, cache only `derived:true`], the 404 → legacy
  `/api/embeddings` `{model, prompt}` fallback, the `assertFiniteEmbedding`
  NaN/Inf guard with both exact messages), and OpenRouter (the SDK
  `embeddings.generate` request body + the **base64-Float32 LE decode** via the
  ported `embedding_blob` reads, array passthrough, `usage` incl. `cost`). Vectors
  are kept `f64` (v4's `number[]`); the `f32` narrowing is the storage boundary.
  `applyEmbeddingProfile` (truncate + L2) was already ported
  (`embedding_vector`). `base64` added to `quilltap-core`. The differential drives
  v4's REAL plugin providers (jest, fetch/SDK mocked) recording the built
  request(s) + parsed result; the Rust side rebuilds + reparses.

Enabled the `float_roundtrip` serde_json feature in the harness so an oracle's
exact-float text (e.g. a pricing rate `0.09999999999999999`) parses
correctly-rounded to match the core's own f64 (the default fast parser is 1-ULP
lossy — surfaced by a `parseFloat × 1e6` rate).

**W4.7e3 — the `logLLMCall` call-site closures — is DONE (2026-07-07); the
per-oracle differential regenerations are staged follow-ups.** The six in-scope
call sites now write `llm_logs` rows through the W4.7e writer. `cheap_llm_exec`
carries an optional `CheapLlmLogConfig` (Db + per-service userId/chatId/messageId
+ LogContext) set by `CheapLlmTaskExecutor::with_logging` and a per-call
`task_type` on `execute` (each internal call site hard-codes its literal, so no
spine signature changes) — so every successful cheap-LLM provider call writes one
row (log type via `map_task_type_to_log_type`), covering compression /
answer-confirmation / image scene tasks / memory extraction / context summary /
scene-state / recap. The **executor-attached** design keeps the four spine files
(`orchestrator`/`message_finalizer`/`message_context`/`build_context`) untouched:
the request/spine path constructs `::new()` (no logging) — a spine-owner follow-up
wires `with_logging` there (the `cheap_llm_selection: None` precedent). The
gatekeeper's LLM-classify path writes a `DANGER_CLASSIFICATION` row
(`classify_content` gained a `db` param; its 5 non-spine callers pass it); the
**moderation path is NOT ported** — the projected `ModerationResult` drops the raw
per-category `flagged` v4 serializes in `JSON.stringify({flagged, categories})`, so
byte-exact content needs the W4.2/W4.7f moderation seam widened (a tracked seam;
the differential banks the absence). `generate_image` (4 sites), the avatar/story
job handlers (via the shared `image_job_common::generate_with_reroute`, 4 sites,
avatar carrying `characterId`), and `primary_stream` (on `chunk.done`, with
`compute_request_prefix_hashes` at the wire + `extract_finish_reason` +
rawProviderUsage + usage/cacheUsage) each write their rows; `durationMs` emits **0**
(the frozen-clock differential expectation — a real value needs a spine-injected
stream clock, a follow-up; and `primary_stream`'s request messages are the lossy
`StreamParams` shape [no attachments/name/toolCalls], to verify in its regen). All
request-path sites pass `LogContext::none()`. **Proof:** a new in-process self-test
(`cheap_llm_exec::tests::logging_writes_one_row_through_the_real_writer`) drives a
cheap-LLM task through a real single-writer `Db` (main + llm-logs partitions, the
`llm_logs` table hand-rolled) and asserts exactly one v4-shaped row (type
SUMMARIZATION, sent temperature/maxTokens, response + usage, no durationMs).
**Staged follow-ups (each its own commit-able step, un-mock `logLLMCall` on the v4
oracle + dump `llm_logs`, per the degradation plan):** `compression_tier3` (the
named writer proof — but its oracle is fully DB-mocked, so it needs the
[[jest-real-db-oracle]] real-DB conversion on both sides), `danger_gatekeeper_tier3`
(bank the moderation-path absence + the LLM-path row), `answer_confirmation_tier3`,
`image_generation_tier3` (the 4-site matrix + the IMAGE_PROMPT_CRAFTING/APPEARANCE
cheap rows), `avatar_job`/`story_background_job_tier3`, `primary_stream_tier3`
(CHAT_MESSAGE + requestHashes assertion), `memory_processor_tier3`,
`context_summary_service_tier3`. `orchestrator_tier3` is explicitly NOT regenerated
(it does not dump `llm_logs`; its corpus is spine-owned). Not attempted this pass to
avoid a half-regenerated oracle; the writer's parts are already verified (the hash
tier-1 diff, the Phase-2 `llm_logs_tier2` create diff, the summarize/task-map
self-tests, and now the in-process end-to-end proof). **Sub-unit
5 — the BUILTIN TF-IDF/BM25 vectorizer** (`qtap-plugin-builtin-embeddings`:
TF-IDF + BM25 + Porter stemming + optional bigrams, `loadState` from the ported
`tfidf_vocabulary` rows) — is **split off as W4.7e2** (it has no dependency on
sub-units 1–4; only the `tfidf_vocabulary` STORAGE repo is ported, NOT the
vectorizer — the decomposition doc's "builtin already ported" claim was wrong).

**Tracked follow-ups (explicit, per the W4.7e work order's degradation plan — the
call-site closures + oracle regenerations serialize last):** the `logLLMCall`
writer's **through-a-real-call-site row diff** (regenerate the smallest cheap-LLM
oracle, `compression_tier3`, with logging un-mocked + the `llm_logs` table dumped)
and the five **call-site logging closures** (`cheap_llm_exec` — the dynamic
task→type map covering every cheap-LLM consumer — plus `primary_stream`
[`CHAT_MESSAGE` + requestHashes/rawProviderUsage/finishReason at the wire], the
gatekeeper [`DANGER_CLASSIFICATION`], answer confirmation [`ANSWER_CONFIRMATION`],
and image generation [`IMAGE_GENERATION`]) with their oracle regenerations, each
its own commit-able step. Not started to avoid leaving an oracle half-regenerated;
the writer's constituent parts ARE verified (the hash tier-1 diff, the Phase-2
`llm_logs_tier2` create diff, and the summarize/task-map self-tests). ~~**Sub-unit
5 — the BUILTIN TF-IDF/BM25 vectorizer** — is split off as W4.7e2.~~ (**DONE** —
see the W4.7e2 note below.)

**Wave 4 (W4.7e2): the BUILTIN TF-IDF/BM25 embedding provider is DONE**
(2026-07-07). v4's zero-network fallback embedder
(`plugins/dist/qtap-plugin-builtin-embeddings/`) is ported end to end — pure
computation, no model seam, no HTTP. New `quilltap-core::tfidf`: the **Porter
stemmer + tokenizer** (`tfidf::porter` — a byte-for-byte transcription of v4's
hand-rolled stemmer over `Vec<char>` [== JS UTF-16 code units for the ASCII
domain the tokenizer guarantees], NOT a crate, since a divergent stem shifts
every stored vocabulary index; the `STOP_WORDS` set verbatim, `stem` [steps
1a–5b in v4's exact order + quirks], `tokenize` [lowercase → replace non-`[a-z0-9\s]`
with space → split → drop stop/short → stem], `generate_bigrams`), the
**`TfIdfVectorizer`** (`tfidf::vectorizer` — `fit_corpus`/`transform`/`get_state`/
`load_state`/`is_fitted`, the BM25 IDF `ln((N-df+0.5)/(df+0.5)+1)` + TF
saturation `(tf·(k1+1))/(tf+k1·(1-b+b·dl/avgdl))`, sorted-term vocabulary →
index map, L2-normalized f64 output, the fit clock injected; `state_vocabulary_json`
/ `state_idf_json` reproducing `JSON.stringify` of the state), and the
**`BuiltinEmbeddingProvider`** wrapper (`tfidf::provider` — synchronous
`generate_embedding`, the exact not-fitted message). Host glue
`services::builtin_embedding::generate_builtin_embedding` (v4
`generateBuiltinEmbedding`: read `tfidf_vocabulary.findByProfileId`, `JSON.parse`
the `vocabulary`/`idf` columns, `loadState`, transform, then narrow to Float32 +
`applyEmbeddingProfile`) over new scoped reads `embedding_profiles::find_by_id`
(+ `EmbeddingProfileRow`, `normalizeL2 !== false`) and
`tfidf_vocabulary::find_by_profile_id` (+ `TvReadRow`). The **`EMBEDDING_REFIT`
job handler** (`services::embedding_refit_job::handle_embedding_refit` — resolve
the profile [not-found error, non-BUILTIN skip], gather every character's
memories [`${summary}\n\n${content}`] via `characters_read::find_by_user_id` +
`memories_read::find_by_character_id`, append help docs [`${title}\n\n${content}`,
read failure swallowed], `fit_corpus`, persist via `upsertByProfileId`, then
`enqueue_embedding_reindex_all`; empty-corpus/no-characters/no-memories → skip),
registered with the W4.8 runner via `EmbeddingRefitHandler`; the debounce
scheduler is host-timing (only the pure `is_builtin_profile` gate is ported).
`queue_service::enqueue_embedding_reindex_all` added (priority −1; the REINDEX
handler stays on the loud fallback). Verified by two differentials, both green:
a **tier-1 `tfidf_vectorizer_equivalence`** (159 rows driving v4's REAL
`stem`/`tokenize`/`generateBigrams`/`TfIdfVectorizer` — dozens of stemmer words
per suffix family, tokenizer casing/punctuation/digits/non-ASCII, bigrams, four
fit corpora × [getState + transform], loadState-from-persisted-JSON, and the two
throw messages; `idf`/vectors compared at 1e-12) and a **tier-3
`embedding_refit_tier3_equivalence`** (a jest-real-DB oracle driving v4's REAL
`handleEmbeddingRefit` over a two-DB fixture [characters + vaults + memories +
help docs + a BUILTIN profile], diffing `tfidf_vocabularies` + `background_jobs`
in the minted-timestamp placeholder form + a runner-registration E2E
[enqueue → pump → dispatch → the vocab row lands]). **Documented seam (the ln
libm divergence):** macOS's system `ln` AND the `libm` crate diverge from V8's
`Math.log` by ≤1 ULP on many inputs, so the persisted `idf` JSON column is
compared NUMERICALLY at 1e-12 in the tier-3 diff (a 1-ULP ≈1e-16 divergence
never affects search); the fittedAt column IS byte-exact (the Rust fit clock is
injected to v4's frozen value), and everything else (vocabulary / avgDocLength /
vocabularySize / includeBigrams / the reindex job payload) is byte-exact. The
jest oracle pins `ensureProcessorRunning` → no-op so the enqueued reindex row
stays PENDING (the Rust port defers the runner the same way).

**Round-4 unification (2026-07-07): DONE — Round 4's provider/image/librarian
lanes are integrated on main.** The four parallel branches (W4.7d, W4.7e
sub-units 1–4, W4.9c, W4.6c) were cherry-picked onto main alongside the
already-landed W4.7f, with union-resolved mod-decl/doc conflicts. **One real
cross-branch conflict surfaced and was fixed at unification** (the reason the
pass exists): the W4.9c handlers were written against the pre-W4.7f
`GeneratedImageData` (`data: String`) while W4.7f widened it to
`Option<String>` + `url` (z-ai's dual shape) — both handlers now reproduce
v4's exact falsy no-op (`rawData = imageData.data || imageData.b64Json;
if (!rawData)` — missing AND empty-string both warn+return). Verified on the
integrated tree: the full workspace gate (619 core tests + harness self-tests,
clippy `-D warnings` on default AND `native-transport`, fmt) and ALL eleven
Round-4 differentials re-run green against freshly regenerated v4 oracles at
`6b6e39ad`, plus `build_context_tier3` proving W4.7e's harness
`float_roundtrip` enablement is inert on existing normalizations. **Remaining
Round-4 lanes (not yet run): W4.4b (owns spine) and W4.5**; the standing spine
handoffs for the next spine owner are unchanged (W4.7e's
`model_supports_native_tools` sourcing + W4.7d's ApiKeyResolver composition
wiring, both → W4.4b) — plus **W4.7e2** (the BUILTIN TF-IDF/BM25 vectorizer)
and the W4.7e logLLMCall call-site closures/regens as tracked follow-ups.

**W4.4b: the chat file/attachment LLM-load subsystem is ported and its two spine
seams are CLOSED** (`OrchestratorSeams::process_files` = v4 `loadAndProcessFiles`;
`MessageContextSeams::load_lantern_images` = the `buildMessageContext` section-K
file loader). New pure `files::` leaves: **`text_detection`** (v4
`lib/files/text-detection.ts` — the 96-entry ext→MIME table + null-byte/non-printable
content sniffing, its own tier-1 differential `text_detection_equivalence`, 41
cases), **`image_processing`** (v4 `lib/files/image-processing.ts` — the
base64-size + provider-limit resize DECISION over an injected `ImageTranscoder`
seam; NO image codec in the core [the `doc_blob` `transcodeToWebP` precedent]; the
geometric ×0.8 loop + the quality-fallback / exhausted-JPEG quirks reproduced
faithfully; `get_provider_max_base64_size` reads the registry manifest), and
**`attachment_support`** (v4 `lib/llm/attachment-support.ts`'s client-safe
`PROVIDER_ATTACHMENT_CAPABILITIES` — the source `profileSupportsMimeType` consults
for NON-image types; images are gated solely by the profile's `supportsImageUpload`
flag, a DISTINCT dataset from the registry `maxBase64Size`). New services:
**`file_fallback`** (v4 `file-attachment-fallback.ts`, from the `6b6e39ad` source —
`generate_image_description`'s three tiers IN ORDER: the persisted-prompt reuse
FIRST [`files.findById` → first non-empty of `generationRevisedPrompt`/
`generationPrompt`/`description`, `reusedPersistedDescription: true`, no vision
call], then the vision call over the `CompletionProvider` seam [profile selection,
the pre-vision downsize to the description provider's limit, the refusal heuristics
— empty / reasoning-token exhaustion / suspicious-keyword — byte-faithful, the
uncensored retry], then the `IMAGE_DESCRIPTION` `logLLMCall` write on both success
and failure via the real `llm_logging::log_llm_call`; plus `convert_text_file_to_inline`
[the exact `[User attached text file: …]` markers], the keep-vs-drop rule
[`unsupported`+no-error → KEEP raw, `unsupported`+error → drop], and
`format_fallback_as_message_prefix` [the `⚠️` error marker]) and **`chat_files`**
(the LLM-load half of `chat-files-v2` — `load_chat_files_for_llm` [NO dedup by id,
per-file load failure skipped] + `load_mount_file_as_attachment` [the Scriptorium
mount-blob path] + `read_file_as_base64` over the injected `FileBytesStore` byte
seam + the resize decision; plus `load_and_process_files` [v4
`loadAndProcessFiles`, the faithful positional-pairing bug kept] and the Lantern
K-loader `load_lantern_images` + `RealMessageContextSeams`). **The vision call
reuses the completion seam** via new `CompletionParams.attachments` +
`CompletionResponse.finish_reason` + `canned_completion_key_with_attachments`
(byte-identical to the base key when attachments empty → every pre-W4.4b oracle
keys unchanged). The K seam went **async** (RPITIT `+ Send`, the `BuildContextSeams`
precedent; `process_message`'s `CMP` bound gained `+ Sync`). Widened
`db::files::FileEntry` with `size` + `description`; added
`find_link_meta_by_linked_to` + `doc_mount_file_links::find_with_content_by_file_id`
(v4 `findByFileId` in the with-content shape). Verified: 640 core tests + the
`text_detection` tier-1 differential green; `orchestrator_tier3` regenerated +
`message_context_leaves` re-run **green** (the new seams are inert on the existing
corpus — file ids empty, no prior-image attachments). The `file_attachment_tier3`
differential (jest real-DB, driving v4's REAL `loadAndProcessFiles` +
`buildMessageContext` over FSM/Sharp/model/Librarian mocks) is the byte-exact
proof. **Tracked deferrals (the two inherited spine handoffs, prose items outside
the deliverables checklist):** sourcing `model_supports_native_tools` from
`pricing_fetcher::check_model_supports_tools` (needs a pricing dep in the spine)
and wiring `ConnApiKeys` into the danger/cheap/image composition points — each
with its own orchestrator-oracle regen. Also deferred (host-side, unchanged from
v4): the upload/ingest half of `chat-files-v2` (Phase-4 FSM storage), the real
image codec + FSM byte layer (the `ImageTranscoder`/`FileBytesStore` production
impls), and the 60 s image-description timeout timer (only the timeout→error
mapping is ported).

**Wave 4 (W4.5): the Carina query engine is DONE** (`services::carina_query`, v4
`carina.service.ts` `runCarinaQuery`) — the isolated reference-answer engine the
markup runner / orchestrator / finalizer / `ask_carina` all drive. The async
`run_carina_query` composes the ported subsystems in v4's order: answerer
resolution (`db::character_resolver::find_characters_by_name`, all matches
oldest-first, prefer `canBeCarina`, else `askerOpensCarinaLine` —
operator/user-controlled/`canBeCarina`-asker via the overlay-free raw read); the
Brahma gate (`is_brahma_name` + sentinel `BRAHMA_CARINA_ANSWERER_ID` +
`brahmaIsReachable` on `systemTransparency`) with the console engine behind the
injected `RunBrahmaConsole` seam (default `UnavailableBrahmaConsole` → llm-failed;
the gate + sentinel-id post path ARE ported — the console is the **W4.5b**
follow-up); the NOT-participant-scoped profile chain (answerer default →
`connection_profiles::find_default` [added] → first web-search-capable via
`Registry::supports_capability(_, WebSearch)` → no-profile); the system prompt
(`build_identity_stack` + `## Scenario` + the surface-level asker card over the
OVERLAID `characters_read::find_by_id` [title/pronouns/aliases are vault-managed]
+ the byte-exact `## Reference Query` framing + the Commonplace memory-recall
block via `search_memories_semantic` limit 12 / minImportance 0.3 →
`format_memories_for_context` budget 1200 → `build_commonplace_llm_context`);
`loadPriorCarinaExchanges` replay; Carina's OWN 5-iteration
detect→execute→re-stream tool loop (a NO-OP sink for the tool frames, matching
v4's swallowing `StreamController`) + the forced-text final turn; the
`systemSender:'carina'` post via the ported `post_carina_response` + the live
`carinaAnswer` emit (v4's `onPosted`, engine-owned); and the
`CARINA_MEMORY_EXTRACTION` enqueue. Also ported: `services::carina_memory_extraction`
(v4 `handleCarinaMemoryExtraction` — the SELF-only synthetic one-slice transcript
over the ported `process_turn_for_memory`; the debug-log write + the cost event
with the pricing/limits injected) and `queue_service::enqueue_carina_memory_extraction`
(dedupe by `carinaMessageId` across the user's PENDING+PROCESSING jobs). **The
`RunCarinaQuery` seam was converted to async** (RPITIT `-> impl Future + Send`) —
the work orders' "frozen" is the behavior + argument shape, not the sync-ness (an
artifact of the canned test impl; every real caller is already async and awaits,
matching how `BuildContextSeams`/`ContextSummarySeams`/`LanternNotificationSink`
went async — see `[[w4.5-carina-async-seam]]`); `run_carina_markup_query` /
`execute_ask_carina` became generic over the seam (RPITIT is not dyn-compatible),
and the sync `#[test]` harnesses gained a current-thread runtime.
`carina_runner_tier3` + `mail_carina_tools` re-verified green against fresh v4
oracles (behavior identical — NOT regenerated). Verified by
`carina_query_tier3_equivalence` (13 cases driving v4's REAL `runCarinaQuery`,
diffing `CarinaResult` + the `carinaAnswer` event + `chat_messages`/`chats`/
`background_jobs` in the shared-id-map remap form — the system-prompt + recall
bytes proven inside the canned stream key; no engine divergence) and
`carina_memory_extraction_tier3_equivalence` (the SELF-only outcome over v4's REAL
handler). **Corpus constraint (documented seam):** keep the memory recall SMALL —
the Rust `format_memories_for_context` uses a fixed 3.5 chars/token while v4 uses
the answerer-provider rate, so the 1200-token budget BOUNDARY must never be the
limiter or the system-prompt bytes diverge. **Tracked handoffs to the spine owner
(W4.4b / unification):** the `ask_carina` `BuiltInToolRunner` dispatch row +
constructing the real `RunCarinaQuery` at the orchestrator/finalizer composition
point (needs the engine deps the spine owns) + the live `@Name:`/`ask_carina`
spine-corpus cases; `orchestrator_tier3` / `message_finalizer_tier3` were NOT
regenerated this round (spine-owned). **W4.5b (the Brahma one-shot console)** is a
tracked follow-up.

wiring, both → W4.4b) — plus ~~**W4.7e2** (the BUILTIN TF-IDF/BM25 vectorizer)~~
(**DONE** — see the W4.7e2 note above) and the W4.7e logLLMCall call-site
closures/regens as tracked follow-ups.

**Round-4-remainder unification (2026-07-08): DONE — Phase 3's port surface is
fully integrated on main.** The four parallel lanes (W4.4b, W4.5, W4.7e2,
W4.7e3) were cherry-picked onto main with NO cross-branch code conflicts (docs/
mod-decl unions only — the disjoint-files discipline held completely; contrast
the prior round's `GeneratedImageData` type drift). Verified on the integrated
tree: the full workspace gate (886 tests, clippy `-D warnings` default +
`native-transport`, fmt) and a fifteen-differential sweep against freshly
regenerated v4 oracles at `6b6e39ad` — the four units' own proofs, the
W4.4b-regenerated orchestrator corpus, the shared-file cross-checks
(`answer_confirmation` [touched by both 4b+5], `message_context_leaves`,
`carina_runner` + `mail_carina_tools` over the now-async `RunCarinaQuery`
seam), and the e3-touched tier-3s (`danger_gatekeeper`, `primary_stream`,
`image_generation`, `avatar_job` — proving the live logging closures inert on
their corpora). **Standing follow-ups (the next spine/wiring pass):** W4.4b's
two inherited handoffs were DEFERRED by its session (`model_supports_native_tools`
sourcing + the ConnApiKeys spine wiring); W4.5's spine closure (the `ask_carina`
dispatch row + the real `RunCarinaQuery` at the orchestrator/finalizer
composition points + the live `@Name:`/`ask_carina` corpus cases); W4.7e3's
spine `with_logging` wiring + the staged per-oracle `llm_logs`-dump regens;
W4.5b (the Brahma one-shot console). Then Round 5: Unit 4, the enclave.

**W4.10a (the spine wiring pass): DONE — the three deferred composition-point
seams are closed.** (1) **`model_supports_native_tools` sourcing:** the
`ProcessMessageInput` field is DROPPED; `process_message` computes it in-spine via
the real `pricing_fetcher::check_model_supports_tools` over an injected
`&PricingFetcher<PF>` (new `PF: PricingFetch` generic on `OrchestratorDeps`; the
fetch stays a seam — only OPENROUTER consults the cache, every other provider
answers from the static fallback table). `build_pricing_context` reads the user's
connection profiles (empty `api_keys` — the live HTTP fetch is Phase-4 host
wiring). (2) **The real ApiKeyResolver:** added `DbApiKeys(Db)` (the owned-`Db`
form of `ConnApiKeys` — the `DangerContentRouter` STORES the resolver, so it can't
hold the router's borrowed connection); the danger router is now constructed with
`DbApiKeys`, reading the fixture-seeded `api_keys` table end to end (the
W4.7d→W4.4b handoff). (3) **The carina spine closure:** `RealCarinaQuery` (an
adapter implementing the frozen `RunCarinaQuery` seam over `run_carina_query`) is
wired into the finalizer's `@Name:` markup path; and the `ask_carina` dispatch row
lands on `BuiltInToolRunner` via an ERASED `ErasedAskCarina` seam (the
`ErasedImageGeneration` precedent — a `TypedAskCarina<EMB,STR,TR,TD,BRA,P>` owning
the engine seams, erased into an `Arc<dyn AskCarinaRunner>` whose `run` takes the
per-turn `&dyn EventSink`), additive with a default reproducing the prior loud
fallback (`ask_carina` moved into `PORTED_TOOLS`; the `onPosted` sink is a no-op in
the tool path — faithful to v4's absent-client-stream case, the answer still posts
to `chat_messages`). Verified: `orchestrator_tier3` regenerated + green with a
**live `@Name:` markup case** (Oracle `canBeCarina` answerer + a recorded inner
carina stream that proves the engine's system-prompt bytes in composition; the
carina message posts, the `carinaAnswer` event emits, `CARINA_MEMORY_EXTRACTION`
enqueues — `carinaMessageId` remapped through the shared idmap, the `carinaAnswer`
payload's minted id/createdAt placeholdered); the oracle un-mocks
`checkModelSupportsTools`, mocks `getPricingCache` empty, un-monkey-patches
`findApiKeyByIdAndUserId` (reads the seeded rows), and points `textblock_mode` at
an OPENAI `o1-mini` profile (`supportsTools:false` in FALLBACK → text-block mode,
distinct from the OPENROUTER cases which default true). `tool_dispatch` gained an
`ask_carina` row driving the REAL engine's not-found early-return (a nonexistent
answerer → v4's "No answerer by that name is on duty." with NO model call) against
v4's REAL `executeToolCallWithContext`. `message_finalizer_tier3` /
`carina_runner_tier3` / `mail_carina_tools` / `tool_build_equivalence` /
`regenerate_swipe_tier3` / `tool_dispatch_equivalence` all re-verified green
(additive-inert). **Deferred (flagged):** a live `ask_carina` TOOL-CALL case
THROUGH the `process_message` spine — the erased-seam `'static` field needs OWNED
engine providers, but the spine's (and the differential's) streaming/embedding
providers are borrowed + shared with the primary stream, so the spine can't
construct a `TypedAskCarina` from its deps. The dispatch + engine are proven
instead by the `ask_carina` seam unit tests (default + canned), the live `@Name:`
markup case (the engine end to end through the finalizer), and the `tool_dispatch`
`ask_carina` row (the dispatch + engine not-found path against v4's real executor);
the `mail_carina_tools` differential proves `execute_ask_carina` itself. Closing
the spine tool-call case needs the spine to own/Arc-share the engine providers (a
production-shaped concern) — tracked as a follow-up. (The `with_logging` /
orchestrator `llm_logs`-dump item stays post-round, coupled to W4.10b's
primary-stream regen; W4.5b's real Brahma console keeps the default
`UnavailableBrahmaConsole` seam.)
**W4.10b (the W4.7e3 `logLLMCall` per-oracle regens): six of seven differentials
regenerated with `logLLMCall` live + an `llm_logs` dump/diff — the writer is now
proven byte-for-byte through real call sites, not just the in-process self-test.**
Each regen un-mocks `logLLMCall` on the v4 oracle (a fresh `SQLITE_LLM_LOGS_PATH`,
read via `getRawLLMLogsDatabase()` before `closeDatabase()`, rows dumped with
id/createdAt/updatedAt placeholdered + sorted by canonical JSON) and attaches the
llm-logs partition + (for the cheap-LLM paths) a `with_logging` executor on the
Rust side (a shared `crates/quilltap-harness/tests/common` helper). **Done:**
`compression_tier3` (CONTEXT_COMPRESSION, 6 rows — the DB-free jest oracle
converted to real-DB on both sides), `danger_gatekeeper_tier3`
(DANGER_CLASSIFICATION, 4 rows; v4's moderation-path `modelName:'moderation'`
rows are a tracked unported-logging seam, filtered on both sides),
`answer_confirmation_tier3` (ANSWER_CONFIRMATION, 13 rows, per-call executor with
the assistant messageId + responder characterId), `image_generation_tier3`
(IMAGE_GENERATION + IMAGE_PROMPT_CRAFTING, per-case), `avatar_job_tier3` +
`story_background_job_tier3` (IMAGE_GENERATION via `generate_with_reroute` incl.
the reroute leg; the story handler's full SUMMARIZATION/IMAGE_PROMPT_CRAFTING/
APPEARANCE_RESOLUTION matrix via a per-case executor), and `memory_processor_tier3`
+ `context_summary_service_tier3` (MEMORY_EXTRACTION / SUMMARIZATION+
TITLE_GENERATION via per-call/per-op executors). **Two real port bugs the row
diffs surfaced and fixed:** v4's `summarizeResponse` ALWAYS emits `error`/
`finishReason` and `summarizeRequest` ALWAYS emits `temperature`/`maxTokens`
(present as `null`), but the port's `LlmLogResponseSummary`/`LlmLogRequestSummary`
skipped them when `None` — all four are now the present-null-vs-absent
double-`Option` (the `chats.removedAt` pattern; a generalized `de_double_opt`
deserializer), so the summarize path stores them present-null while a raw tier-2
write with the key absent still stores them absent (`llm_logs_tier2` re-verified —
its fixture has `error`/`temperature` absent, `finishReason`/`maxTokens` present).
Also: several corpus userIds were non-UUIDs (`user-1`) that the llm_logs schema's
`z.uuid()` validation silently dropped — fixed to real UUIDs. **The one remaining
regen — `primary_stream_tier3` (CHAT_MESSAGE) — is now DONE (W4.11b, below).**
`orchestrator_tier3` is explicitly NOT regenerated (it dumps no `llm_logs`; its
corpus is spine-owned). The W4.7e3 spine-side `with_logging` wiring remains the
standing follow-up.

**W4.11b (the `primary_stream_tier3` logging regen + the failover log gap): DONE**
(2026-07-08), closing W4.10b's last deferred regen. The oracle's model mock is
RELOCATED from the service-level `streamMessage` wrapper down to `createLLMProvider`
(the wrapper — and its terminal CHAT_MESSAGE `logLLMCall`, streaming.service.ts:407
— now runs for real), `logLLMCall` un-mocked, `SQLITE_LLM_LOGS_PATH` set, `Date.now`
frozen. The recorded canned keys are byte-identical to the old service-level keys
(provider/model/temperature/messages), and every pre-existing event trace +
`chat_messages`/`chats` dump is UNCHANGED (the relocation moves the mock boundary,
not behavior — the port's event emission is untouched and matched). **The real port
gap the survey flagged is fixed:** the provider-failover retry legs
(`provider_failover.rs`) now write CHAT_MESSAGE rows — v4's `restreamInto` logs per
`streamMessage` call — sharing `primary_stream::{StreamLogCtx, log_chat_message_call}`
(the row construction, NOT forked; `StreamLogCtx.character_id` widened to
`Option<&str>`). Two v4 `characterId` details reproduced: the failover rows carry
`characterId = NULL` (v4's `restreamInto` passes none), and the tool-unsupported
retry row ALSO carries `characterId = NULL` (v4's retry `streamMessage` call omits
`characterId`, unlike the primary attempt — the differential caught this). A new
entry point `attempt_empty_response_recovery_with_log` + a `FailoverLogCtx`
{db, message_id} carry the logging; the orchestrator's existing
`attempt_empty_response_recovery` keeps the no-logging path (threading the spine's
db + `preGeneratedAssistantMessageId` into the failover is a spine-owner follow-up —
the orchestrator diff filters CHAT_MESSAGE anyway). **Closed the documented
`db::llm_logs` `temperature` seam** (`LlmLogRequestSummary`): an integer-valued
temperature (e.g. `1.0`, common on the CHAT_MESSAGE path) now serializes bare (`1`)
via `js_number_to_json`, matching v4's `JSON.stringify` — the first integer-valued
temperature through a CHAT_MESSAGE row surfaced it; `llm_logs_tier2` (fractional
`0.7`) re-verified inert. `durationMs` is 0 on both sides (oracle freezes `Date.now`;
port hard-codes 0 — a real stream clock is a spine follow-up), `requestHashes`
diffed as a row column. Verified: `primary_stream_tier3_equivalence` regenerated +
green (12 calls, 6 CHAT_MESSAGE rows — 2 primary + 4 failover, none for recovery),
a failover-logging unit test (one row per retry leg, characterId NULL), 665 core
tests, `llm_logs_tier2` re-run, clippy `-D warnings` (default + `native-transport`),
fmt.
~~W4.5b (the Brahma one-shot console)~~ (**DONE** — see below). Then Round 5:
Unit 4, the enclave.

**Wave 4 (W4.5b): the Brahma one-shot console is DONE** (2026-07-08,
`services::brahma_console`, v4 `lib/services/brahma-console/one-shot.service.ts`
`runBrahmaQuery`) — closing the `RunBrahmaConsole` seam W4.5 left injected. The
isolated operator console the Carina engine invokes when the answerer is Brahma:
a single `[system, question]`-only query (never the Salon transcript — Carina's
isolation contract) that returns the final answer text, executing tools at the
**operator surface** (`run_sql` + all-store doc access) with their side effects
standing but nothing persisted (no assistant/TOOL rows, no tokens, no SSE). New
module: `resolve_brahma_connection_profile` (the `null`-console-profile collapse
to the user's default) + `normalize_tool_call_signature` (v4's two
`orchestrator.service` helpers — the only imports from the SEPARATE Phase-4
streaming console, which is NOT ported), the byte-exact `build_brahma_system_prompt`
(base brief + `BRAHMA_SQL_PROMPT` in a GENERATED `prompt_text` submodule via the
checked-in `harness/oracle/cases/gen-brahma-prompts.mjs` — the tool-catalog
transcription precedent), `requires_api_key` (the manifest `configRequirements`,
`?? true`), and `run_brahma_query` composing the ported units in v4's order:
default profile → api-key gate (UNSCOPED `api_keys::find_by_id`, the two exact
detail strings) → the console tool slate via `build_tools` (agent mode, doc
read/write, read-only `run_sql`, search-without-memories; NO `ask_carina`
recursion guard, NO workspace tools) → tool mode (`simple-json` COERCED to
`text-block`, deliberate) → the native/text-block instruction builders +
`build_agent_mode_instructions(25)` → the 25-turn agent loop (its OWN loop, NOT a
reuse of `native_tool_loop`: force-final push at turn 25, native-`detectTool` /
text-block detection, `submit_final_response` via tool args + the raw-text
fallback, the `MAX_DUPLICATE_TOOL_CALLS = 2` dup-count / stale-iteration
stuck-loop guard with the byte-exact nudge + `lastToolResultText` reminder,
threading via `build_assistant_tool_call_message`/`build_tool_result_messages`,
execution through the injected `ToolRunner` with `operator_surface: true` + a
fresh `pendingWardrobeAnnouncements`, the no-op SSE sink) → the empty-answer /
final-answer resolution; NEVER errors out (every failure is a
`BrahmaConsoleResult { ok:false, detail }`). `RealBrahmaConsole` implements the
frozen trait (generic-consumed over the streaming provider / tool runner /
detector; deps on the constructor). Verified by
`brahma_console_tier3_equivalence` — a jest real-DB oracle driving v4's REAL
`runBrahmaQuery` over a four-user main-DB fixture (a default profile with a valid
SYNTHETIC api key; a user with none; a no-`apiKeyId` default; a
missing-`apiKeyId` default) across nine cases (no-profile; both api-key detail
strings; a plain answer; submit via tool args AND via raw-text; empty → 'empty
response'; a `run_sql` iteration — a real SELECT whose byte-exact result threads
into the continuation; and the duplicate-call stuck-loop guard — the byte-exact
nudge proven by the 4th continuation's canned key), mocking ONLY `streamMessage`
(scripted per-case + RECORD the `provider|model|temperature|messages` key — so the
system-prompt bytes incl. `BRAHMA_SQL_PROMPT` + the tool instructions are proven
by the Rust replay) and `detectToolCallsInResponse` (by marker); tool EXECUTION
is REAL both sides (the real `BuiltInToolRunner`; provider registry initialized so
`buildTools` is fully real; ANTHROPIC + a fictional model makes
`checkModelSupportsTools` return `true` deterministically, matching the Rust
injected value); the console never persists so the diff is the
`BrahmaConsoleResult` per case (no table dumps). Plus nine module unit tests (the
25-turn loop bound + the dup + stale guards over a seeded in-memory `Db` driven by
a call-order scripted stream provider, the never-throws / no-profile sentinel over
a bare `Db`, and the pure helpers). **Oracle gotcha:** `jest.setup` globally mocks
`@/lib/plugins/provider-validation` to a partial WITHOUT `requiresApiKey`, so the
oracle must `doMock(..., requireActual)` it back (the [[jest-real-db-oracle]]
un-mock pattern). **Tracked handoffs / deferrals:** the spine/Carina swap-in
(constructing `RealBrahmaConsole` at the `answer_as_brahma` composition point —
where W4.5 injects `UnavailableBrahmaConsole`) is a unification one-liner (needs
the streaming/runner/detector deps the spine owns); the differential
doc-edit-write + search cases are deferred (both handlers are proven by
`doc_text`/`doc_fm` + `search_tools`, and the console dispatches through the
identical real `BuiltInToolRunner`; a doc write threads a per-side-minted `mtime`
that a canned-stream-key replay cannot reproduce, so `run_sql` — deterministic
result — proves the operator-surface loop + threading + continuation-key match
instead).

**Wiring-round unification (2026-07-08): DONE — W4.10a, W4.5b, and W4.10b are
integrated on main.** The three parallel lanes cherry-picked onto main with
ZERO source-level conflicts for the second consecutive round (docs/version
conflicts only, union-resolved). **One integration fix:** the last pick's
`--theirs` resolution on `crates/quilltap-harness/Cargo.toml` clobbered
W4.10b's `tempfile` dev-dependency (a version-file resolution strategy must
diff the whole file, not assume version-only — recorded in the reconciliation
memory note); restored at unification and caught by the gate. **The W4.5b
spine swap-in is done:** the orchestrator differential's carina composition now
constructs the REAL `RealBrahmaConsole` (behaviorally inert — no corpus case
names Brahma on either side — so it proves the generic composition typechecks;
a live Brahma corpus case is a tracked follow-up in the same
provider-ownership family as the live ask_carina-through-spine case). Verified
on the integrated tree: the full workspace gate (898 tests, clippy `-D
warnings` default + `native-transport`, fmt) and an **eighteen-differential
sweep** against freshly regenerated v4 oracles at `6b6e39ad` — the three
lanes' own proofs (orchestrator + tool_dispatch + the five W4.10a shared
re-verifications; brahma_console; the six W4.10b regens + `llm_logs_tier2`)
plus the `carina_query` cross-check. Versions: core 0.0.134, harness 0.0.128.
**Standing follow-ups after this round — ALL FOUR CLOSED OR NARROWED by the
W4.11 cleanup round (2026-07-08, see below):** ~~the spine `with_logging`
wiring + an orchestrator `llm_logs` dump~~ (**DONE — W4.11a**); ~~the W4.10b
step-6 `primary_stream_tier3` regen~~ (**DONE — W4.11b**, incl. the failover
CHAT_MESSAGE log-gap fix + the `temperature` seam close); ~~the gatekeeper
moderation-path logging seam~~ (**DONE — W4.11c**); the live
`ask_carina`-through-spine + live-Brahma orchestrator corpus cases — the
provider-ownership blocker is RESOLVED (W4.11a's Arc impls + seam wiring),
narrowed to two precise remainders: the ask_carina case needs the per-turn
sink threaded through `ToolExecutionContext` (the W4.1c `emitCarinaAnswer`
slot — v4 emits a `carinaAnswer` frame from the TOOL path that the Rust
`run_ask_carina` NullSink swallows), and the live-Brahma case needs an
`isDefault=1` fixture connection profile (a corpus-wide ripple). Plus one
new small item from W4.11b: the orchestrator spine's failover-log wiring
(thread its db + `preGeneratedAssistantMessageId` into
`attempt_empty_response_recovery_with_log`). Then Round 5: Unit 4, the
enclave (`enclave-engine.md`).

**W4.11c: the gatekeeper moderation-path `logLLMCall` seam is now CLOSED**
(2026-07-08) — the last tracked `logLLMCall` seam. The moderation seam was
widened so the wire's raw per-category `flagged` survives the projection:
`gatekeeper::ModerationCategoryScore` gained a `flagged` field (matching v4's
`ModerationCategoryResult`; `moderation_wire::into_gatekeeper` now carries it
through), while `map_moderation_result` still reads ONLY `category`/`score` —
faithful, v4's projection never consults per-category `flagged`. The
`ModerationOutcome::Moderated` branch of `classify_inner` now writes v4's
(`gatekeeper.service.ts:279`) `modelName:'moderation'` `DANGER_CLASSIFICATION`
row: provider = the wire provider name, one `user` request message,
`response.content` = a hand-built `JSON.stringify({flagged, categories})`
(ordered `serde_json::Map`, each category `{category, flagged, score}`, `score`
via `js_number_to_json` so an integer-valued score renders bare), `userId` +
`chatId` only (no messageId/characterId/usage/temperature — the summarizers'
present-null double-`Option` handles v4's absent `?? null` fields),
awaited-and-ignored (the writer never throws — the LLM-path precedent),
`LogContext::none()`. A moderation-provider **failure** writes no row (v4
identical — the throw skips the log, reaching the outer catch → safe fallback),
and a classification-cache hit never reaches the provider. The
`danger_gatekeeper_tier3_equivalence` differential dropped its `strip_moderation`
filter and now diffs BOTH moderation rows byte-for-byte (regenerated green
against v4 `6b6e39ad`; the oracle already ran the real `logLLMCall` since W4.10b
— no v4-side change beyond a fresh regen). `danger_routing_equivalence` +
`moderation_wire_equivalence` re-verified green (the wire types are unchanged).
With this every `logLLMCall` call site is ported (the W4.11b
`primary_stream_tier3` regen and the W4.11a spine `with_logging` wiring
landed in the same round).

**W4.11a (spine `with_logging` + owned-provider plumbing): the Arc/logging
half is DONE; the two live corpus cases are DEFERRED with precise blockers.**
Added `Arc<T>` blanket impls for the three provider seams (`EmbeddingProvider`
/ `CompletionProvider` / `StreamingCompletionProvider` in `model/{embedding,
completion,stream}.rs`; delegate to the inner value) — the production-shaped
ownership answer: one concrete provider shared BY VALUE between a borrowed spine
dep and an owned, effectively-`'static` erased seam (a bare clone would
duplicate any stateful queues; a delegation + a shared-consumption unit test
each). Wired the `ask_carina` tool seam into the spine
(`OrchestratorDeps.ask_carina: &ErasedAskCarina` + the per-turn
`BuiltInToolRunner::with_ask_carina`; `not_available` default keeps a no-engine
build's loud fallback), closing the ask_carina-through-spine DISPATCH wiring the
W4.10a note tracked (previously the spine's runner carried no engine → loud
fallback). The **`with_logging` + orchestrator `llm_logs` dump is green**: the
harness materializes the llm-logs partition and constructs a per-call
`with_logging` executor (`chat_id` = the call's chat, `message_id: None` —
matching v4's cheap-LLM `logLLMCall` context), and diffs the `llm_logs` dump
via the shared `common::{dump_llm_logs, oracle_llm_logs}`. The cheap-LLM rows
— the distill `MEMORY_EXTRACTION` (per-call `characterId`) + the summary fold's
`SUMMARIZATION` + `TITLE_GENERATION` — match v4 **byte-for-byte** (the harness
now carries the canned completion's `usage` through so the logged `usage`
matches). Two row families are documented seam/mock artifacts filtered from
BOTH sides: `CHAT_MESSAGE` (the Rust primary_stream logs these; v4's
service-level `streamMessage` mock swallows its own CHAT_MESSAGE log — proven
byte-exact by `primary_stream_tier3`/W4.11b) and `DANGER_CLASSIFICATION` (v4's
`resolveMessageDangerState` classifies the user message INLINE — a documented
spine seam, behaviorally inert here since the canned response resolves
non-dangerous → no reroute). The oracle also mocks `runPreContextPreCompute` to
its inert empty result so v4's SECOND (pre-compute) distill call — the
unported pre-compute recall path, a spine deferral — does not double the
`MEMORY_EXTRACTION` rows (behaviorally identical: empty memories →
`preSearchedMemories: undefined`, the compression cache empty). The harness's
erased `ask_carina` engine (a real `TypedAskCarina` over Arc clones of the
shared providers + a SEPARATE tool runner + its own console) and a live
`RealBrahmaConsole` over the shared Arc streaming are constructed and
**inert-verified** against the 23-case corpus. **The two live corpus cases are
DEFERRED (real blockers, both surfaced by the survey):** (1) the ask_carina
tool-call case — v4 emits a `carinaAnswer` SSE frame from the TOOL path
(`orchestrator.service.ts:1067` wires `toolContext.emitCarinaAnswer`, and
`filter_events` KEEPS `carinaAnswer`), but the Rust `run_ask_carina` handler
passes `&NullSink` (the `emitCarinaAnswer` context slot is a documented W4.1c
deferral), so matching it requires threading the per-turn sink through
`ToolExecutionContext` in `services/tool_execution.rs` — OUTSIDE this lane's
file ownership, and a feature beyond the Arc/logging mandate; "fix the port not
the diff" forbids filtering v4's frame. (2) the live-Brahma `@Name:` case — the
console's success path needs the user's DEFAULT connection profile
(`find_default` requires `isDefault=1`, which no fixture profile has) to carry a
valid api key; adding a global default ripples through `resolve_carina_profile`
/ cheap-LLM selection / danger resolution across the 23 existing cases, exceeding
a safe budget. The seams' behavior is independently proven by `carina_query_tier3`,
`brahma_console_tier3`, `mail_carina_tools`, the `ask_carina` seam unit tests
(default + canned), the `tool_dispatch` `ask_carina` not-found row, and the
orchestrator `carina_markup` case (the finalizer `@Name:` engine end-to-end).

**Cleanup-round unification (2026-07-08): DONE — W4.11a, W4.11b, and W4.11c
are integrated on main.** The three lanes cherry-picked onto main with ZERO
source-level conflicts for the third consecutive round (CLAUDE.md/CHANGELOG
unions only; every branch's Cargo.toml delta was verified version-only before
take-theirs — the prior round's tempfile lesson applied — and all three lanes
bumped to the same numbers, so versions auto-aligned). Verified on the
integrated tree: the full workspace gate (903 tests, clippy `-D warnings`
default + `native-transport`, fmt) and a **thirteen-differential sweep**
against freshly regenerated v4 oracles at `6b6e39ad` — the three lane proofs
(`orchestrator_tier3` with the llm_logs dump, `primary_stream_tier3` with the
CHAT_MESSAGE rows + requestHashes, `danger_gatekeeper_tier3` with the
moderation rows) plus ten cross-checks (`danger_routing`, `llm_logs_tier2`,
`tool_dispatch`, `message_finalizer_tier3`, `carina_query_tier3`,
`brahma_console_tier3`, `mail_carina_tools`, `compression_tier3`,
`memory_processor_tier3`, `context_summary_service_tier3`). Versions: core
0.0.135, harness 0.0.129. **Every pre-enclave follow-up is now closed or
precisely narrowed** (see the standing-follow-ups block above): the remaining
small items are the two live orchestrator corpus cases (ask_carina blocked on
the W4.1c `emitCarinaAnswer` sink threading; Brahma on an `isDefault` fixture
profile), the spine failover-log threading, and — further out — the W4.7f/W4.2
moderation plugin registry + api-key host seams. **Round 5 (Unit 4, the
enclave) is ready to start:** `enclave-engine.md` + the u4 work order are
refreshed for the landed logging reality (the per-run token accounting's
`llm_logs` substrate is now live and byte-verified); its one named spine gap
(parameterize `log_chat_message_call`'s hard-coded `LogContext::none()`) is in
the order's ground rules.

**Round 5 (Unit 4, the enclave engine) is in progress — U4.1–U4.3 are DONE
and green** (2026-07-08, branch `u4-enclave`; three parallel agents on
disjoint files, integrated + gated). New module family
`quilltap-core::enclave`: **U4.1** `enclave::milestones` (the pacing bitmask
logic — near-end sets the halfway bit too so a vaulted halfway never fires
late; the pre/post-turn exhausted-action rules incl. the grace grant — plus
the Host-voiced milestone/grace bodies via a checked-in generator that
evaluates v4's own template literals under V8; byte-exact composition proof
lands in U4.4's tier-3 chat_messages diff; the Phase-1 `enclave_budget`
differential regenerated, zero drift). **U4.2** `enclave::cron` — croner
10.0.1 semantics **hand-rolled** (the Rust croner crate REJECTED: v4 passes
no timezone option, so croner-JS runs on V8 local-Date semantics, not its
own `fromTZ` path; jiff `Compatible` disambiguation proven identical to ES
`LocalTZA`); `next_occurrence` + the throw-vs-null `try_next_occurrence`
split; 124-row × 2-tz tier-1 differential (DST both directions, one-off
datetimes, L/W/#n modifiers, the `?`-before-star OR quirk, V8 ISO
day-overflow normalization); the harness pins croner's version and fails
loud on a bump. **U4.3** `enclave::announce` + `enclave::lifecycle` — the
run-start row contract + Host announcement writers (the carina-writer
idiom; banner caps summary + name list byte-exact) and the whole lifecycle
service (begin / start-scheduled / start-manual with cron-slot consumption /
pause / resume with pause-interval accumulation / stop with the runId bump /
update-settings with invalid-cron whole-edit rejection / startup +
failed-turn reconciliation, every `runStateMessage` string verbatim);
`ChatUpdate` gained 21 autonomous setters, `queue_service` the
AUTONOMOUS_ROOM_TURN/_SCHEDULE_TICK enqueues; 38-op tier-2 real-DB
differential (18 chats, 7 jobs, 6 banners), the cron seam closed at
integration so it proves the lifecycle∘cron composition. Banked v4 facts:
the startup-reconcile stamp is `lastMessageAt ?? runStartedAt ?? now` (a
coalesce, NOT the spec's max — spec fixed); rollback-on-enqueue-failure
leaves currentRunId + zeroed counters in place and manual start re-throws;
resume re-enqueue has NO rollback guard; the manual-start
`scheduleNextRunAt: null` explicit-write quirk.

**U4.4 (the capstone) is also DONE — Round 5 / Unit 4 / PHASE 3 IS
COMPLETE** (2026-07-08). `enclave::step` ports `handleAutonomousRoomTurn` as
the persisted one-transition `step()` (guards incl. the `(createdAt, id)`
concurrent-sibling tie-break; the idle→running fallback + banner; pre-turn
budget over the daily read + `last_local_midnight_iso` [jiff, injected tz];
the grace/near-end branching over the U4.1 leaves; speaker selection over
the ported turn manager with `userParticipantId=None`; `process_message`
with the autonomous options + the run `LogContext`; the post-turn
monotonic-max/+1 accounting pinned to the LOCAL snapshot with v4's
buffered-read why-comments carried; milestones; the 9c fold on the
participant-profile ?? default ?? first chain, UNtagged [outside the run
scope, faithful]; re-enqueue) + `schedule_tick` (seed / stale-advance /
fresh start / wedge heal) — **direct writes through `Db`** (decision #3
REVISED: the v4 oracle side runs unforked, so the differential pins
in-process direct-write semantics; `write_apply` keeps its own proof,
re-verified). The LogContext gap is CLOSED (`log_chat_message_call`
parameterized via `RunPrimaryStreamOptions`/`ProcessMessageInput.log_context`,
default none — `primary_stream_tier3`/`orchestrator_tier3` regenerated
green); the `autonomous_context_cap` context-manager clamp was found
UNPLUMBED in v5 and closed (`BuildContextInput.autonomous_context_cap`, the
shrink-only clamp on `budget_info.max_available`; `build_context_tier3`
re-verified). The runner dispatch rows (`AUTONOMOUS_ROOM_TURN` [a
host-composed step-runner closure; `step`'s future is non-`Send` via the
finalizer's carina trait object, so the handler bridges on a dedicated
thread] + `AUTONOMOUS_ROOM_SCHEDULE_TICK`) are live with v4's
dispatcher-level post-`markFailed` reconcile hook, plus two runner E2E
tests (tick → start → 3 turns → halfway → grace → `budget:turns` end; wedge
heal + failure → resumable `paused`). **Three faithful-port findings** (all
banked in the corpus, recorded in `enclave-engine.md`): (1) v4's
`getTotalTokenUsageSince` `$ne:null` translator bug is REAL — the daily
token sum is ALWAYS 0 on SQLite, the daily-budget gates never bind, ported
broken-but-exact (probe committed); (2) `turn_error:` is dead code — v4's
`handleSendMessage` stream shell swallows every error, a failed turn counts
+ re-enqueues (banked `stream_error_swallow`); (3) `suppressAutomaticImages`
has NO consumer in v4 (declared, set, never read — nothing to plumb).
Verified by `enclave_step_tier3_equivalence` — 19 calls / 20 chats / THREE
DBs (main + mount-index + llm-logs), driving v4's REAL handlers with only
the model boundaries mocked at `createLLMProvider` (zero canned misses =
the full spine prompts match byte-for-byte); diffs `chats` + `chat_messages`
(the Host announcements byte-exact, completing U4.1's composed-string
proof) + `background_jobs` + `llm_logs` (12 run-tagged CHAT_MESSAGE + 11
tagged distill rows vs the untagged fold rows — the LogContext proof,
two-sided). Full workspace gate green (705 core tests; clippy `-D warnings`
default + `native-transport`; fmt). **Standing follow-ups:** the
provider-failover retry legs' LogContext (pinned none at the site, the
pre-existing failover-log threading item); the real stream duration clock
(`durationMs` 0). **Next: Phase 4** (transports + the Angular SPA + the
host drivers — the fork/timer seams: the 60 s scheduler timer, the
`FileBytesStore`/`ImageTranscoder`/fs seams, the HTTP transport).

**Drift check (2026-07-08): v4 `6b6e39ad..6bf88959` (1 commit) audited — no
ported unit is stale.** `6bf88959` ("The Green Room" — a status dialog
narrating new-conversation startup) touches only UNPORTED surfaces: the new
`lib/chat/creation-progress.ts` (an in-memory per-progressId event bus + a
standalone SSE route — a Phase-4 host/transport concern; in v5 these progress
events would ride the boundary's `Event` channel, noted for the Phase-4
transport design) and `apply-outfit-selections.ts`, which gained an optional
`progress?: CreationProgressEmitter` narration around its `llm_choose` path —
`applyOutfitSelections` belongs to the chat-CREATION flow (POST
`/api/v1/chats` / participant actions / chat merge), which is not in the
Phase-3 port scope (the port covers per-turn sends; the commit itself states
autonomous rooms and per-turn sends are untouched). The ported surfaces it
composes are UNCHANGED at this commit (`resolveEquippedOutfitForCharacter`'s
4-arg signature predates the baseline; `chooseLLMOutfit` / `cheap-llm.ts` /
`chats.setEquippedOutfit` untouched); the rest is React UI, API routes, and
docs/help data. The `docs/v4/` mirror was refreshed (CHANGELOG, API.md).
**New oracle baseline for in-flight/future orders: `6bf88959`.**

**Drift check (2026-07-08, at the Phase-4 kickoff): v4 `6bf88959..2494a84b`
(1 commit) audited — no ported unit is stale.** `2494a84b` ("copy conversation
UUID from Salon header and Organize drawer") is pure React UI + docs: a new
`components/chat/CopyChatIdButton.tsx` (two variants over the existing
`useCopyToClipboard` hook), the Salon header title becoming a link to the
conversation URL, help-content text, and version bumps. The ONLY `lib/` touch
is a tsc cast fix in a **test mock**
(`lib/wardrobe/__tests__/apply-outfit-selections.progress.test.ts` — not
runtime code); no API route changed. Classification: Phase-4 SPA reference
surface — the copy-UUID buttons + header link land with the Salon vertical
(P4.6), noted in the phase-4.md screen inventory's reference set. The
`docs/v4/` CHANGELOG mirror was refreshed. **New oracle baseline for
in-flight/future orders: `2494a84b`.**

**Phase 4 (transports + hosts + the Angular SPA): KICKOFF PLANNED**
(2026-07-08, `docs/developer/porting/phase-4.md` — start there). Built from
three fresh surveys (the v5 seam/deferral sweep, the v4 API surface: 124
routes / ~162 action verbs / one terminal WS / 9 binary asset routes / a
confirmed-vestigial auth layer, and the v4 UI surface: ~24 screens / ~535
components / the 11k-line `qt-*` theme CSS). **22 locked decisions**, headed
by: the axum **HTTP transport is a first-class deployment** (Docker-Desktop-
style local web use — run the container or binary, open a browser), with **no
authentication** (localhost-trust; bind-address is the only knob — bare binary
defaults `127.0.0.1`; anyone wanting auth proxies) while v4's **pepper-unlock
readiness gate survives as a non-auth concept** (503 + setup flow); the
**browser and the Tauri webview are co-equal hosts of the one Angular SPA**
(one `CoreClient` seam; every shell integration needs a web path); the
dispatch surface is `POST /api/dispatch` + one scope-tagged `GET /api/events`
SSE + the binary resource GETs + the terminal WS — NOT a reproduction of v4's
REST tree (the `Request` enum is action-centric and grows per consumer);
crate layout `quilltap-core::api` (pure contract) + `quilltap-host`
(composition root + all timers/IO drivers) + `quilltap-web` + `quilltap-cli`
(dual-mode: direct-core or HTTP client — single-writer is per-process) +
`quilltap-tauri` (last) + `apps/web`. The differential discipline continues
for every Phase-4 core port (the route-logic backfill: chat creation, wizards,
help-chat orchestrator, backup/restore, import/export, unlock/pepper-vault
service, the markdown renderer + `qtap-linkify` [lookbehind hand-rolled],
Document Mode ops, the Brahma streaming console, …); **tier 4** covers the
rest (transport contract tests over a shared corpus, a headless HTTP e2e
smoke in CI, CLI diffs vs `npx quilltap`, Playwright for the SPA — v4 is the
behavioral reference, not a byte target). Decomposition P4.0 (boundary +
composition root) → P4.1 (host-driver lanes; the one all-new piece is the
production streaming composer: request_builder → transport → decoders →
StreamChunk) → P4.2/P4.3 (`quilltap-web` + Dockerfile ∥ the CLI) → P4.4
(backfill, interleaved) → P4.5 (SPA foundation: CoreClient + SSE reducer +
the `qt-*`/theme port) → P4.6 (SPA verticals, Salon first) → P4.7 (Tauri);
milestones M0–M6. Non-goals: uniffi/mobile, plugins beyond the provider
manifests, any release/signing/publishing work, new features before parity.

**Phase 4 (P4.0): the boundary + composition root is DONE — milestone M0**
(2026-07-08; work order
`docs/developer/porting/work-orders/p4.0-boundary-composition-root.md`; drift
check at round start: v4 HEAD still `2494a84b`). New `quilltap-core::api`:
the contract types (`Request` internally tagged / `Response` adjacently
tagged / the scope-tagged `Event` envelope whose one family wraps the
existing `services::chat_events::ChatEvent` byte-exact frames — nothing
emits yet, the channel + envelope exist so later units add variants, not
plumbing), the `QuilltapCore` trait (RPITIT dispatch + broadcast subscribe),
`api::provision` (the control-flow port of v4 `provisionDbKey` — env pepper
/ `.dbkey` / hash-mismatch-fatal → resolved / needs-setup / needs-passphrase
/ needs-vault-storage; the minimal boot core of the P4.4 unlock-service
backfill, unit-tested, its differential lands with P4.4), and the
engine-backed `CoreEngine` with the first variants (health, unlock-state /
unlock / lock, list-instances, list-chats) — the readiness gate enforced in
dispatch (D2), `Lock` a REAL teardown through the new
`EngineAssembler`/`EngineShutdown` seams (drivers stop, `Db` clones drop,
the writer thread exits, state returns to needs-passphrase — faithful to v4
`lockDbKey` incl. the env-pepper-booted-can't-re-unlock consequence).
`dbkey` gained the WRITE path (`save_dbkey`/`generate_pepper`/`hash_pepper`/
`read_pepper_hash` — PBKDF2-SHA256 × 600k, 32-byte salt, 16-byte-IV
AES-256-GCM, v4's exact JSON field order + 0600 mode), round-trip verified
against the Friday-verified reader; the `Setup` Request variant is
deliberately NOT implemented (a fresh instance also needs schema creation —
unported P4.4 surface; wiring Setup now would mint empty cipher-correct DBs
with no tables). New `quilltap-host` crate (the composition root, D20 — the
ONLY crate that owns timers): base-dir/platform path resolution (explicit →
`QUILLTAP_DATA_DIR` → platform default), the launcher instance-registry READ
path (`instances.json` schema v1 incl. the POSIX owner/0600 permission
refusal; write verbs are P4.3), and the cadence drivers — the job-runner
pump loop (v4 dispatcher semantics over the ported `PumpOutcome`:
orphan-reset once, enqueue wake, next-due delay, 2 s poll), the 5-minute
stuck-job reset, and the 60 s + immediate autonomous schedule tick (v4
`scheduled-autonomous-rooms.ts`, per-chat-settings-user enqueue). The
seam-free handler set is registered (`AUTONOMOUS_ROOM_SCHEDULE_TICK` /
`WARDROBE_OUTFIT_ANNOUNCEMENT` / `EMBEDDING_REFIT`); every other job type
stays on the loud fallback until its P4.1 lane. **Wake-hook gotcha solved:**
`queue_service::set_wake_hook` is a process-global first-wins `OnceLock`,
but assemblies come and go (lock/unlock) and tests run several hosts per
process — the host registers ONE forwarding hook fanning out to a registry
of weak per-assembly targets (spurious cross-instance wakes pump an empty
queue, harmless; torn-down targets self-prune). M0 integration tests boot a
fixture instance headless (env-pepper AND `.dbkey` paths), dispatch the
first variants, pump enqueued jobs to COMPLETED through the real loop, and
prove lock → unlock → drivers-restart. **Next: P4.1 host-driver lanes**
(provider IO / files+images / PTY / environment) ∥ P4.2 (`quilltap-web`) +
P4.3 (CLI) per the decomposition.

**Phase 4 (P4.1a): the provider-IO host-driver lane is DONE** (2026-07-08).
Five deliverables: (1) **the production streaming composer**
(`quilltap-core::model::streaming_provider::WireStreamingProvider` — the
biggest single gap, now closed): `StreamParams` → `RequestInput` (`stream:
true`, tools/top_p/stop/web-search/profile-params/cache-key/
previous_response_id all flowing) → `build_request` + the shared auth
injector (**`apply_auth` hoisted** into `model::provider_auth`, the
completion + streaming paths cannot drift) → `ProviderTransport::
execute_stream` → the **manifest-selected W4.7b decoder** (the
`ChatCompletionsFlavor` split applied internally: DEEPSEEK/Z_AI/OPENROUTER;
google over the ported `is_thinking_model` predicate [made `pub` in
`request_builder::google`]; ollama takes the model as its default echo) →
the normalized `StreamChunk` channel. The pump is a **plain OS thread**
(`blocking_recv`/`blocking_send` — the core stays scheduler-free); a
transport error or `DecodeError` becomes an `Err(StreamError)` item after
the chunks already emitted (v4 streams can fail after yielding); EOF drives
the idempotent `finish()`. Keys ride an injected `ProviderKeySource`
(provider → plaintext key — the failover path re-calls the SAME provider
seam with a different provider id); a profile `baseUrl` swaps the manifest
base (localhost-rewritten). **Documented deliberate divergence:** OPENROUTER
always streams the raw chat-completions wire (v4's no-tools OpenResponses
SDK protocol — the W4.7b out-of-scope wire — is not ported). Verified by
the "free" differential `streaming_composer_equivalence`: all **21
committed W4.7b wire fixtures** replayed through the FULL compose path over
a fake transport at whole-buffer + byte-at-a-time (ollama line-aligned per
the ported no-buffer bug), diffing the chunk sequences + error parity
against the recorded v4 NDJSON — green first run — plus 8 unit tests (auth
per manifest scheme incl. google's query param, decoder selection for all
nine providers, mid-stream + pre-stream errors, unknown provider, EOF
finish-once). (2) **The reqwest wire transports** (`quilltap-host::wire`):
`ReqwestWireTransport` (async `WireTransport` — a completed non-2xx
exchange is `Ok`, per the W4.7f dialect contract) + `BlockingWireTransport`
(`SyncWireTransport` over `reqwest::blocking` ALWAYS driven on a dedicated
thread — a blocking client panics on a tokio runtime thread; loopback-smoke
tested from inside a runtime). (3) **The live `PricingFetch`**
(`quilltap-host::providers::LivePricingFetch`): the three pricing HTTP
calls (public openrouter models with v4's `Content-Type` header; the
`@openrouter/sdk` `models.list()` wire surveyed + reproduced — `GET
/api/v1/models` with Accept/Bearer/HTTP-Referer/X-OpenRouter-Title; ollama
`/api/tags`), each under v4's 3 s fail-fast pricing timeout, any failure →
`None` (the ported fetcher owns the negative cache). (4) **The API-path
`EmbeddingProvider`** (`quilltap-core::services::embedding_provider::
ApiEmbeddingProvider` over the `WireTransport` seam): the full port of v4
`generateEmbeddingForUser` — profile resolution (explicit → `find_by_id`,
fallback → the new `embedding_profiles::find_default`; none → the exact
"No embedding profile configured"), the BUILTIN dispatch to the ported
`generate_builtin_embedding`, the registry gate (unknown /
embeddings-incapable), `rewrite_localhost_url` on a profile baseUrl, the
requiresApiKey gate over `api_keys::find_by_id_and_user_id` (missing → the
exact "No API key found for {p} embedding profile"), the three wire
dialects over the frozen `embedding_wire` builders/parsers (openai;
ollama incl. the `/api/show` num_ctx derivation + derived-only cache + the
404 legacy fallback; openrouter via the RECORDED SDK wire — the SDK's own
body key order + Zod response projection reproduced), and
`apply_embedding_profile`. **New v4 fact (banked):** v4 `generateEmbedding`'s
outer catch is DEAD CODE — the body `return`s the async calls WITHOUT
`await`, so the "Embedding failed for provider …" wrap never fires and raw
plugin errors escape unwrapped (the `turn_error:` broken-but-exact family);
ported faithfully (plain throws surface with `provider: None`). The
priority lane's 50 ms wait loop is host-timing (accepted + ignored,
documented). Verified by `embedding_provider_tier3_equivalence` — a
jest-real-DB oracle driving v4's REAL `generateEmbeddingForUser` over a
baked fixture (9 profiles / 3 synthetic api keys / a BUILTIN vocabulary
fitted by v4's REAL vectorizer so both sides load identical stored bytes)
with ONLY `global.fetch` scripted (real `Response` objects — the bundled
OpenRouter SDK consumes them), 12 cases (openai truncate+L2 / builtin
default + explicit-missing fallback / no-profile / missing-key / ollama
modern + 404-legacy incl. a failed `/api/show` / openrouter base64-Float32
+ 429 / openai 429 / builtin not-fitted / normalizeL2-false), the Rust side
replaying over a `CannedWireTransport` registered FROM the oracle-recorded
wire (a request-building divergence = loud canned miss), results/errors
diffed exactly. (5) **The `ProviderIo` constructor bundle**
(`quilltap-host::providers`) — policy/user-agent/`BASE_URL` knobs +
constructors for the streaming composer, the completion transport, the
W4.7f `RealImageProvider`/`RealModerationProvider`/`RealWebSearchProvider`,
and the pricing fetcher — and **the spine api-keys closure**:
`build_pricing_context` now populates the connection-profile api keys (v4
`getApiKeyForProvider` via `findApiKeyByIdAndUserId`), proven inert under
the canned pricing seam by a freshly regenerated
`orchestrator_tier3_equivalence`. **Tracked deferrals:** the production
spine composition (a `ChatSend` dispatch assembling `OrchestratorDeps` from
these drivers + the model-dependent job-handler registrations) → P4.2; the
completion-call `withTimeout` family (25 s/60 s) → the spine composition;
the OpenRouter OpenResponses no-tools wire (documented divergence); the
real stream duration clock (`durationMs` 0, spine-owned).

**Phase 4 (P4.1c): the PTY / terminal host driver is DONE** (2026-07-08; work
order `docs/developer/porting/work-orders/p4.1c-pty-terminal.md`; oracle
baseline `2494a84b`, no drift). New `quilltap-host::terminal` — the session
manager over **`portable-pty`** (replacing node-pty), v4
`lib/terminal/pty-manager.ts` faithful: spawn (shell `opts.shell ||
SHELL ?? /bin/bash` [JS-falsy fallthrough], cwd → the injected `files_dir`,
80×24, env = inherited + caller overrides + `QUILLTAP_DATA_DIR` set
authoritatively LAST, `TERM=xterm-256color`; directories are constructor
params — lane d's `paths` untouched), the **256 KB ring buffer capped in
UTF-16 code units** (v4's JS `.slice`), the raw transcript append stream at
`<logs>/terminals/<id>.log` (under `logs/`, never indexed; the DB row stores
only `transcriptPath`), the `terminal_sessions` row at spawn (pinned id) +
the exit-stamp update, per-subscriber mpsc broadcast with the attach replay
(ring buffer as ONE `output` frame, then `meta`), kill (SIGTERM via
`libc::kill` — portable-pty's own `kill()` is SIGKILL; note an INTERACTIVE
shell ignores SIGTERM, exactly as under v4's node-pty) / write / resize /
kick-for-chat (flush-then-kill-then-forget; the exit sequence still stamps
the row from the reader thread), the exit sequence in v4's order (stamp meta
→ final `session-closed` Ariel flush → `exit` broadcast + channel close →
transcript end → DB update → close announcement), and the **Ariel flush
drivers** (a per-session tokio task computing the idle [30 s since last
chunk] / max-age [120 s since the buffer started] deadlines — v4's two
setTimeouts as one select loop; durations are config knobs, v4 constants
default). The **verbatim WS protocol types** (`terminal::protocol` — the v4
Zod unions field-for-field, nullable meta fields as explicit `null`,
round-trip tests against literal v4 JSON) land here so P4.2's WS route only
marshals; the **production `TerminalScrollbackSource`**
(`terminal::scrollback::PtyScrollbackSource`) reproduces v4's terminal-read
resolution (in-map session AND DB `exitedAt == null` → ring buffer; else
the transcript tail, last 1 MB lossy-UTF-8, errors → `''`). New core
`services::ariel_notifications` (the W4.6b writer idiom): the three Ariel
announcement writers — session-opened (the truthy-label ` — "…"` suffix),
terminal-output (returns `None` on empty/whitespace; the
`computeFenceLength` longest-backtick-run+1 rule; the 16 K-UTF-16 elide
keeping 8 K head+tail with the en-US-grouped `[N characters elided]` marker;
`systemKind: terminal-output-<reason>`), session-closed (the `=== 0` /
`== null` / `with exit code N` label) — each posting one
`systemSender: 'ariel'` ASSISTANT row with the `<!-- terminalSessionId:… -->`
embed through the ported `add_message`, plus
`reconcile_terminal_sessions_for_chat` (the startup-orphan sweep; the
live-PTY probe injected; the explicit-NULL `exitCode` write via the appended
`terminal_sessions::mark_session_exited`, which mints `updatedAt` per v4's
base `_update`). **Verified:** the Ariel writer differential
(`ariel_writers_tier3_equivalence` — 18 cases driving v4's REAL
`postAriel*` + `reconcileTerminalSessionsForChat` over a v4-baked one-DB
fixture; per-case posted/reconciled results + `chat_messages` (13 rows) /
`chats` / `terminal_sessions` diffed byte-for-byte, sentinel-aware exit
stamps — green on first run), 10 real-PTY host integration tests (output →
subscriber/ring/transcript, exit → DB stamp, attach replay, the UTF-16 ring
cap under >256 KB, kill/kick, subscribe/write guards, idle + max-age flush
drivers, scrollback live→transcript flip, reconcile spare-the-live), a
fixture-driven END-TO-END test (real PTY → idle flush → the REAL posted row
+ the `chat-update` broadcast), and `terminal_sessions_tier2` /
`terminal_tools` re-verified against fresh oracles. **Tracked deferrals:**
the shell-init alias/completions bootstrap (`lib/terminal/shell-init.ts`
targets the Node launcher `packages/quilltap/bin/quilltap.js`; a v5 alias
needs the P4.3 `quilltap` binary path — every session still carries
`QUILLTAP_DATA_DIR`, degradation = v4's own plain-shell fallback), the
session-opened announcement call site (v4's spawn REST route → P4.2, the
writer is ported + verified), the WS route/upgrade (P4.2), xterm.js (P4.6).
The exit frame's `signal` string is portable-pty's signal NAME (v4 sends
node-pty's numeric signal stringified) — a documented host-tier divergence
(the SPA consumes it loosely).

**Phase 4 (P4.1d): the environment/cadence lane is DONE** (2026-07-08; work
order `docs/developer/porting/work-orders/p4.1d-environment-cadence.md`; v4
baseline `2494a84b`). **The single-instance lock** (`quilltap-host::lock`, v4
`instance-lock.ts`) — PID-in-file (not `flock()` — VirtioFS/network mounts),
hostname disambiguation across VM/container boundaries, atomic
`O_CREAT|O_EXCL` create with the EEXIST re-read, re-entrant same-PID refresh,
dead-PID stale claim, the different-host rule (a docker/lima/wsl2 lock with a
heartbeat fresher than 5 min REFUSES, else stale-claims; a foreign LOCAL lock
is always stale), the 50-entry history cap, and v4's exact pretty-JSON file
format (a v4-Electron lock parses; the `absent|corrupt|active|stale|suspect`
classification is shaped for P4.3's CLI verbs). Acquired in
`HostAssembler::assemble` (a live conflict → `BootError::Assemble`, the typed
boot error), heartbeated every 60 s by a host loop, released on shutdown
(stop-flag-first so a release never reads as a loss); a LOST lock stops the
drivers then runs `HostConfig::on_lock_lost` (default: exit 1 — v4's
close-and-`process.exit(1)`). **The four scheduler sweeps** run as stop-aware
host loops (v4 `instrumentation.ts` order): cleanup (immediate + 24 h),
housekeeping (5-min grace, the 20 h recent-COMPLETED-`reason:'scheduled'`-job
short-circuit via `find_recent_by_type`, 24 h), maintenance (grace + the
`lastMaintenanceSweepAt` 20 h window + 24 h), and the danger scan (the
all-users-OFF start gate — a check failure also skips — + immediate + 10 min).
New core services: `services::scheduled_maintenance` (v4
`runScheduledMaintenance` — four independently-isolated sweeps in order
[finished jobs / stale-chat asset collapse / orphaned mount-index files /
closed terminal sessions] + the end-of-pass stamp recorded regardless of
per-sweep failures; the transcript unlink is the injected `TranscriptStore`
seam, `FsTranscriptStore` in the host over `<base>/logs/terminals/<id>.log`)
and `services::danger_scan` (v4 `runScheduledDangerScan` — the W4.8 deferral
CLOSED: per-user resolved-mode gate, the per-chat
exempt/off-duty/sticky-dangerous/safe-not-grown filter, the
controlledBy-filtered participant-profile-first-then-fallback resolution, the
summary → classification / no-summary->50 → CONTEXT_SUMMARY / ≤50 →
classification tree at priority −2, per-chat enqueue failures swallowed).
Ported the two missing repo ops: `doc_mount_file_links::sweep_orphaned_files`
(the `NOT IN (SELECT fileId …)` reaper) and the terminal reaper
(`terminal_sessions::find_closed_before` — both v4 guard layers: the SQL
`$lt` prefilter + the parsed-instant re-check so a live PTY is never reaped —
+ `cleanup_closed_sessions`). `queue_service` gained `enqueue_context_summary`
(plain enqueue, NO dedupe — faithful) +
`enqueue_chat_danger_classification_with_priority` (v4's `options.priority ??
-1` passthrough; the scan's −2). `quilltap-host::env` carries the production
`SelfInventoryEnv` (runtime-mode over the paths.ts docker[+`/app`]/lima
probes — kept DISTINCT from the lock's probe, which has no `/app` check —
the release-notes semver scan + changelog read, mount-index-degraded derived
from the `Db`, the flattened `LEGACY_FALLBACK_PRICING` rows; **documented
seam:** the flat env's single `registry_default_context = 8192` falls through
to the ported per-provider constant table, so DEEPSEEK/Z_AI models resolve
8192 until the env goes provider-aware). Verified by TWO new differentials,
green against v4 HEAD: `danger_scan_tier2_equivalence` (a 10-chat / 3-user
gate-matrix fixture; the pre-check + result counts + the `background_jobs`
dump in the minted-values form — the oracle neutralizes v4's job host via the
`childCrashed` latch) and `maintenance_ops_tier2_equivalence` (drives v4's
REAL `runScheduledMaintenance` over a two-DB fixture with BUILD-TIME-relative
day-keyed rows [wall-clock-robust], proving both new repo ops inside the real
orchestration: the 7-vs-30-day per-status job windows, FAILED-never-reaped,
the live-session guard, both transcript path forms + the ENOENT
not-counted rule, the orphan sweep, and the stamp — plus a fixture gotcha:
`ChatSchema` carries a `.refine()`, so materialize `chats` via the repo's
lazy `getCollection`, not `ensureCollection`). The adjacent
`terminal_sessions` / `background_jobs` / `maintenance_sweep` tier-2
differentials re-verified green against fresh oracles; lock unit tests +
host-cadence integration tests (conflict boot error; loss handler with the
drivers stopped; the maintenance 20 h window honored across a re-boot; the
danger gate + a live enqueue) + service self-tests. **Tracked handoffs:**
the launcher lock verbs + write-lock (P4.3, over `classify_lock_status`); the
startup-conflict HTTP surface (P4.2); the flat-`SelfInventoryEnv`
provider-awareness follow-up; the maintenance sweep's storage-bytes half
stays lane b's FsSeam (called through the ported collapse).

**Phase 4 (P4.1b): the file/image host-driver lane is DONE** (2026-07-08;
work order `docs/developer/porting/work-orders/p4.1b-files-images.md`; v4
baseline `2494a84b`). The byte layer is real. New core
`services::file_storage` (v4 `file-storage/manager` + the project/user-uploads
bridges + `webp-conversion` + `blob-transcode` + `store-file`'s database blob
branch + `images-v2`'s `createFile`/`ingestImageBuffer` + `cascade-delete`'s
`deleteFileCompletely`): the pure key/path logic (`safe_filename`, storage
keys, thumbnail keys, the `mount-blob:{mp}:{blob}` codec), the WebP
**policies** in core (`convert_to_webp` q90 / `transcode_to_webp` q85 — the
mime/extension rewrite + failure-passthrough shapes are v4 JS code, so they
live above the pixel seam) over a low-level `PixelCodec` seam, the manager
ops dispatching mount-blob keys through the ported `doc_mount_blobs` and disk
keys through a `StorageBackend` seam, the ingest engine (auto-WebP → sha
dedup with the storage-existence recheck → orphaned-metadata cleanup → the
user-uploads bridge write → the `files` row with post-transcode mime/size +
tag inheritance), and the production seam impls (`ProductionFileBytes` for
BOTH `FileBytesStore` seams; `RealProjectImageUpload` for
`ProjectImageUpload`). New core `services::help_doc_sync` (v4
`help-doc-sync.ts` — its LOCAL loose frontmatter/url/title regex quirks, the
hash-skip, upsert + embedding-clear) over a host-walked file list (the walk
is host-side by documented decision). New `quilltap-host` modules:
`image_codec` (`image` + `webp` crates — libwebp bindings for lossy WebP
encode per D19; implements `files::image_processing::ImageTranscoder`
[metadata + resize_step], `model::image::ImageTranscoder` [convertToWebP],
`PixelCodec`, + the thumbnail op; documented degradations: animated GIF →
first-frame, AVIF/HEIC decode unwired → v4's own failure-passthrough branch,
`resize_step` decode failure returns the original bytes), `files_store`
(the local disk backend: tilde expansion, the `buildSafePath` traversal guard
incl. the `..`-text strip, ENOENT-tolerant delete + legacy `.meta.json`
sidecar unlink, the transient-error fs retry backoff; + the help-doc tree
walker), and `apply_fs` (the four `ApplyHost` fs ops — inventory completion,
unit-tested, no production consumer since U4.4 moved the enclave to direct
writes). `db::instance_settings` gained `get_user_uploads_mount_point_id`
(append-only region). **Two new differentials, both green** (D10):
`help_doc_sync_equivalence` (a tsx real-DB oracle driving v4's REAL
`syncHelpDocs` over a committed fixture help tree — `process.chdir` into the
fixture root before importing, since `HELP_DIR` is captured at module load —
banking created/updated/unchanged/empty-skip, CRLF + unclosed/EOF-fence
frontmatter quirks, the bare-`url:`-never-matches rule, the title-case
fallback, the embedding clear on change, and the untouched-row sentinel
proof) and `image_ingest_tier2_equivalence` (a jest real-DB oracle driving
v4's REAL `ingestImageBuffer` with ONLY sharp mocked to a deterministic
passthrough — mirrored by the Rust `PassthroughPixelCodec`, so the WebP
policy itself is under test — banking fresh ingest, the dedup linkedTo
merge + no-op, the orphaned-metadata recheck re-ingest, webp/svg
passthroughs, and the gif convert, diffing per-op entry records + six tables
across both DBs in a shared-UUID-substring-remap form with the
`doc_mount_points` aggregates pinned per the refreshStats precedent).
**Tracked handoffs/deferrals:** the keep_image mount-blob-fallback ingest
runs inside a `Db::write` closure where `ProductionFileBytes::ingest` fails
LOUD (a connection-scoped store needs executor-owner wiring); the frozen
`ProjectImageUpload` trait is infallible while v4's `uploadFile` throws (an
upload failure returns an `fs-seam:error:` sentinel — widen to `Result` in a
unification pass); the maintenance sweep still deletes metadata-only (route
it through `delete_file_completely` — a one-line unification edit);
`refreshStats` unported (standing precedent); `uploadChatFile` → P4.4;
thumbnail serving routes → P4.2; the legacy storage-key migration form not
ported.

**P4.1 unification (2026-07-08): DONE — all four host-driver lanes are
integrated on main.** The four lane branches (P4.1a provider IO / P4.1b
files+images / P4.1c PTY+Ariel / P4.1d environment+cadence) were
cherry-picked onto main in a/c/d/b order; every conflict was a mechanical
union (the host `lib.rs` mod decls + doc header, the host `Cargo.toml`
dependency additions — hand-merged whole-file per the prior round's tempfile
lesson, incl. the shared `libc` line c+d both wanted — the append-only
`db::terminal_sessions` c+d additions [`mark_session_exited` +
`find_closed_before`], and the CHANGELOG/CLAUDE.md doc blocks); zero
source-level type drift between lanes for the fourth consecutive round, and
the flagged image-seam overlap is composition, not duplication (lane b's
`HostImageCodec` IMPLEMENTS the three core transcoder/codec seams; lane a's
`ProviderIo` only CONSTRUCTS the W4.7f providers over the wire). Verified on
the integrated tree: the full workspace gate (769 core + 51 host tests +
the harness/doc tests, ~1,086 total; core re-run green under
`native-transport`; clippy `-D warnings` on BOTH default and
native-transport; fmt) and a **twelve-differential sweep** against freshly
regenerated v4 oracles at `2494a84b` — the four lanes' own proofs
(`streaming_composer` [committed fixtures], `embedding_provider_tier3`,
`orchestrator_tier3` [regenerated], `ariel_writers_tier3`,
`danger_scan_tier2`, `maintenance_ops_tier2`, `help_doc_sync`,
`image_ingest_tier2`) plus the four adjacent re-verifications
(`terminal_sessions_tier2`, `terminal_tools`, `background_jobs_tier2`,
`maintenance_sweep_tier2`). Versions: core 0.0.139, harness 0.0.132, host
0.0.2. **Standing follow-ups (recorded, deliberately NOT implemented this
pass):** lane b's four handoffs — the keep_image mount-blob-fallback ingest
needs a connection-scoped store (`ProductionFileBytes::ingest` fails LOUD
inside a `Db::write` closure until the executor-owner wiring lands), the
frozen `ProjectImageUpload` trait should widen to `Result` (an upload
failure currently returns the `fs-seam:error:` sentinel), the maintenance
sweep's byte-delete should route through `delete_file_completely` (a
one-line edit), and the harness→host dev-dependency (the help-doc-sync
differential walks the PRODUCTION tree walker) is a deliberate
dependency-direction note; lane d's flat-`SelfInventoryEnv`
`registry_default_context` seam (the provider-agnostic 8192 falls through
to the per-provider constant table, so DEEPSEEK/Z_AI under-report until the
env goes provider-aware). **P4.2 handoffs the lanes named:** the production
spine composition (a `ChatSend` dispatch assembling `OrchestratorDeps` from
the P4.1 drivers + the model-dependent job-handler registrations), the
terminal WS route marshalling over `terminal::protocol`, the thumbnail
serving routes, and the startup-conflict 503 surface over
`classify_lock_status`. Next: P4.2 (`quilltap-web`) ∥ P4.3 (CLI).

**Phase 4 (P4.2): quilltap-web + the production chat-send spine — DONE,
milestone M2** (2026-07-08). The production spine (`quilltap-host::spine`):
`ChatSpine` implements the new `api::ChatSendDriver` seam — generic over ONLY
the four model boundaries (embedding / completion / streaming / pricing
fetch), every other seam REAL, mirroring the tier-3 orchestrator
differential's construction (`RealBuildContextSeams`,
`RealAnswerConfirmation` under a host 25 s + 60 s timeout ceiling,
`RealAsyncCompression`, a pricing-backed `CostTracker`, `RealCarinaQuery`,
`RealBrahmaConsole`, the erased ask_carina engine, `DangerContentRouter`
over `DbApiKeys`, a thread-bridged Prospero writer, `OsRandomBytes`). Each
dispatch runs `process_message` + `execute_turn_chain` on a dedicated thread
+ current-thread runtime (the U4.4 non-`Send` bridge), frames riding the
engine `Event` broadcast; a turn error emits v4's transport-shell
`{error, errorType, details}` frame (new `EventPayload::ChatError`).
`EngineAssembler::assemble` grew the event broadcast + an `EngineAssembly`
return (shutdown + optional chat driver); the engine's `ChatSend` arm is
readiness-gated, with the typed "chat dispatch not assembled" refusal for
driver-less embedders. `ProductionSpineFactory` wires the P4.1a `ProviderIo`
drivers and registers the model-dependent job handlers per assembly
(`AUTONOMOUS_ROOM_TURN` via the step-runner closure, `MEMORY_HOUSEKEEPING`
[the v4 handler body as glue over ported pieces — its end-to-end
differential rides the P4.4 jobs vertical], `CHAT_DANGER_CLASSIFICATION`,
`CARINA_MEMORY_EXTRACTION`, `CHARACTER_AVATAR_GENERATION`,
`STORY_BACKGROUND_GENERATION` — the image handlers constructed per job so
`now_ms` is the wall clock). **Documented host-tier seams** (spine.rs module
header): the provider→key scan (first active key per provider — v4 follows
the profile's `apiKeyId`; divergence only under multiple same-provider
keys), the `chat_settings`→`OrchestratorChatSettings` + timestamp-config
projections (NEW differential-less mappings, to fold into the P4.4/P4.5
verified readers), the single 85 s confirmation ceiling (v4 splits
25 s/60 s inside the service), and the step's best-effort profile
pre-resolve. New crate **`quilltap-web`** (the axum transport, D1–D5):
`POST /api/dispatch` (ErrorKind→status; the Locked 503 merges v4's
`{error:"Setup required", setupUrl:"/setup", pepperState}` body alongside
the typed envelope), `GET /api/events` (one global SSE stream — v4's
`data:` frame encoding, incrementing `id:` fields, the `: keep-alive`
comment every 15 s, lag = resync), `GET /health` (200 healthy / 423 locked
/ 409 lock-conflict over `classify_lock_status` / 503 unhealthy — the
P4.1d startup-conflict handoff closed), the D4 binary GETs (files proxy /
files by id + the cached WebP thumbnail action / the mount-point raw file
read / the blob read with documents fallback — v4's cache/sha/disposition
headers), the D5 terminal surface (spawn posts the session-opened Ariel
announcement — the P4.1c handoff closed — plus list/get/kill/write/delete
and the WS marshalling `terminal::protocol` verbatim with v4's
unknown-session exit-then-close-1000), static SPA serving with the index
fallback + embedded steampunk placeholders, and the D2 bind policy. The
host assembler now constructs a per-assembly `TerminalManager` (exposed via
`Host::terminal_manager()`, cleared on Lock). **Verified:** the M2 e2e
smoke (always-on CI: a COMMITTED v4-baked test-pepper fixture instance
[`crates/quilltap-web/tests/fixtures/`, built by the orchestrator fixture
builder; user ids rewritten to `SINGLE_USER_ID` at test setup] — real HTTP
dispatch → live SSE content/done frames → the assistant row + chat bumps
asserted in the DB), the transport contract tests (statuses, the Locked
body, the unlock round trip, exact SSE frame bytes), the terminal REST+WS
integration over a real PTY (incl. the Ariel announcement row), the
binary-route matrix (bytes/mime/cache/sha/RFC-5987 headers, the thumbnail
cache write), and the Dockerfile BUILT + RUN (196 MB image; `/health` 423
needs-setup on an empty volume). **Tracked deferrals:** the non-raw JSON
mount-file read envelope + themes assets/fonts + `characters/{id}/photos`
(P4.4), the `Setup`/`Store`/`ChangePassphrase` dispatch variants (P4.4),
`Last-Event-ID` replay + creation-progress events (P4.4), the real
`/setup` UI (P4.5), the WS `action=signal` non-SIGTERM delivery (the
P4.1c manager exposes SIGTERM only), and the spine mappings above.
**Phase 4 (P4.3): the `quilltap` CLI Tier R is DONE — milestone M1**
(2026-07-08; work order `docs/developer/porting/work-orders/
p4.3-quilltap-cli.md`; v4 baseline `2494a84b`). New **`quilltap-cli`** crate
(bin `quilltap`) — the v4 launcher's direct-mode verb set, every shipped verb
**byte-diffed against `node <v4>/packages/quilltap/bin/quilltap.js` on shared
fixtures** (`tests/cli_differential.rs`, env-gated on `QT_V4_CHECKOUT` +
Node 24; 118 cases diffing stdout + stderr + exit code, green). Shipped: the
router (v4 `locateSubcommand` — global value flags skipped with their values,
first bare token decides, an instance named `db` never mis-routes; all 11
subcommands recognized, unshipped ones exit loud; bare `quilltap` prints a
banner pointing at `quilltap-web` per D12); **`db`** legacy flags (--tables /
--count / raw SQL reader+writer / --json / --write / --llm-logs /
--mount-points) with **V8 `console.table` reproduced byte-for-byte**
(`vtable.rs` — box-drawing, `(index)` column, `util.inspect` string quoting;
non-TTY form, the diffed one; TTY value-coloring a documented divergence) and
JS number/JSON rendering (`nodefmt.rs` — better-sqlite3's lossy
integer→double conversion included); the **lock commands** (--lock-status /
--lock-clean / --lock-override — literal-ANSI classification
ACTIVE/SUSPECT/STALE, heartbeat ages, last-10 history, operating on the raw
JSON so unknown fields survive; corrupt-lock silent-clean quirk ported);
**`docs`** read verbs (list / show / ls / dir / tree / read incl. --rendered
+ the TTY-binary guard, `qtap://` addressing over the ported core codec with
the document-store-only + `self` rejections, `assertDocsSchema`'s
migration refusal with instance hints); **`instances`** CRUD (list / show /
path / add / remove / set-passphrase / default / rename + the interactive
prompts); **`completion`** (bash/zsh/fish — v4's templates transcribed
byte-exact). The resolution chain + pepper unlock are v4's `db-helpers.js`
ported over `quilltap-core::dbkey` (`resolve.rs` — the five-step precedence,
the one-shot stderr hint + `QUILLTAP_QUIET_HINTS`, `loadDbKey`'s
internal-sentinel-first order, the `hasPassphrase` strip-and-rewrite
migration, flag → env → hidden-TTY-prompt with Ctrl-C exit 130, the exact
no-TTY error, and Node's AES-GCM failure message verbatim on a wrong
passphrase). **`quilltap-host` additions (the P4.1d handoffs, closed):**
`lock.rs` gained `verify_pid_is_quilltap` (the v4 probe regex verbatim —
`node|electron|quilltap|next-server`, per-OS ps//proc/tasklist),
`classify_lock_status_probed` (emits the `Suspect` state), and
`acquire_write_lock`/`release_write_lock` (the CLI write-lock — refuse on
live/suspect holder with v4's exact multi-line messages, claim stale with
history preserved, no override); `instances.rs` gained the full v4 registry
surface (read/write on an insertion-ordered `Value` so unknown fields
survive, atomic 0600 tmp+rename writes, `resolve/list/upsert/remove/
set_passphrase/default/rename/verify_passphrase`, `expand_path`).
**Documented divergences:** TTY table colors; the Node readline
pipe-buffer discard (two piped prompt answers work in v5, hang-and-exit in
v4 — asserted v5-side); heartbeat elapsed-seconds normalized in the diff;
`db --repl` deferred. **Deferred (tracked):** the db high-level verbs
(schema/find/chats/messages/logs/message/log/memories/characters/optimize/
backup/integrity — recognized, loud), docs files/status/find/grep +
memories/logs (Tier B), every server-required verb + the
HTTP-dispatch mode (P4.4), themes/migrations/maintenance/file-verify, and
wiring bare `quilltap` to embed `quilltap-web` (unification/next round).

**P4.2/P4.3 unification (2026-07-08): DONE — both transport lanes are
integrated on main; milestones M1 and M2 both stand.** The two lane
branches cherry-picked onto main with only the four expected mechanical
conflicts (CLAUDE.md/CHANGELOG two-block unions; host `Cargo.toml`
verified version-only on BOTH sides before resolving to 0.0.4 — the
tempfile lesson applied; Cargo.lock taken from the web lane then
regenerated to pick up `quilltap-cli`). Zero source-level conflicts for
the fifth consecutive round — only P4.2 touched host `lib.rs` (the spine
append), and the ownership matrix held completely. Verified on the
integrated tree: the full workspace gate (**1,110 tests / 0 failed**;
769 core under `native-transport`; clippy `-D warnings` on both feature
sets; fmt), the **124-case CLI differential re-run live against the v4
launcher** (136 s, green), and the quilltap-web suites re-surfaced
(`m2_chat_send_end_to_end`, the dispatch/SSE + locked-vault contract
tests, terminal REST+WS round-trip, binary routes). The round's core
diffs are additive/visibility-only on ported surfaces (`find_by_storage_key`
new read, `build_pricing_context` pub, `SelfInventoryEnv: Clone`,
`execute_completion`'s opt-in per-call `base_url` override whose `None`
path is byte-identical) — no oracle-covered path changed, so no
differential regens were required. Versions: core 0.0.141, host 0.0.4,
web 0.0.1, cli 0.0.1. **Standing follow-ups after this round:** wiring
bare `quilltap` to embed/exec `quilltap-web` (deferred deliberately — a
next-round decision); the P4.3 Tier-B verbs (db high-level verbs, docs
files/status/find/grep, memories, logs) + `db --repl`; the HTTP-dispatch
CLI mode with the server-required verbs (P4.4); P4.2's named deferrals
(themes/photos routes, Setup/Store/ChangePassphrase variants,
Last-Event-ID replay, the non-raw mount-file JSON envelope, the real
`/setup` UI → P4.4/P4.5); and the still-unregistered job handlers
(MEMORY_EXTRACTION needs v4 `buildTurnTranscript`, SCENE_STATE_TRACKING
needs the W4.6a job wrapper, CONTEXT_SUMMARY/TITLE_UPDATE shells, the
EMBEDDING_GENERATE family — all P4.4 backfill). Next: P4.4 route-logic
backfill (chat creation + unlock/pepper-vault first) ∥ P4.5 SPA
foundation, per the decomposition.

**Drift check (2026-07-09): v4 `2494a84b..a7b1398d` (2 commits) audited —
BOTH stale ported units; a drift re-port round is REQUIRED before further
Phase-4 work builds on the affected spine surfaces.** The two commits:
**`b90cd1f5`** ("nothing to add" turn-skipping for group chats) and
**`a7b1398d`** (answer-confirmation amendments anchored to the current
conversation). Empirically verified against fresh v4-HEAD oracles:
**three differentials FAIL** — `answer_confirmation_tier3` (a7b1398d
rewrote the re-affirmation system prompt + restructured the reaff user
message [labeled sections] + added `buildRecentConversationContext` [a
compact Staff/tool/silent-filtered transcript, 8 K-UTF-16/20-message caps]
+ `characterName` threading; 10 vs 13 ANSWER_CONFIRMATION rows),
`orchestrator_tier3`, and `enclave_step_tier3` (both by the same
mechanism: v4's `processMessage` now computes skip eligibility for
qualifying group chats [>2 active char participants OR ≥2 LLM chars] and
injects an ephemeral `[NOTHING TO ADD]` Turn note into the outgoing
context — 21 recorded stream keys per oracle carry it → canned misses).
**Three differentials verified still GREEN at HEAD** (ports stale but
corpora unaffected): `turn_state` (v4 `calculateTurnStateFromHistory` now
sets `lastSpeakerId` from a Host turn-pass record — corpus has none),
`turn_orchestrator_tier2` (v4 `shouldChainNext` now excludes
Staff/systemSender messages from the all-LLM pause counter + threads
`selectionReason: 'queue'|'algorithm'`; `executeTurnChain` continues past
skipped turns), and `chats_tier2` (the new `turnSkippingEnabled`
nullable-boolean column is additive — the answer-confirmation-columns
catch-up pattern applies). **Classified inert** (no regen needed):
`build_context_tier3`/`message_context` (the `turnSkip` option is gated
off in every direct-drive corpus), `message_finalizer_tier3`
(confirmation OFF in corpus), `post_office_host` (the turn-pass writers
are NEW exports; `postHostMessage`'s `hostEvent` fields going optional is
inert for existing callers), `chat_events`/`primary_stream` (the done
`skipped`/`skippedParticipantId` fields + `turnComplete.skipped` + the new
`hostAnnouncement` frame are additive), `regenerate_swipe`/`courier`/
`carina_query` (sibling entry points never pass `turnSkip`), and the
`message-formatter` → `response-normalizer` extraction (a pure move with
re-exports — the ported `message_formatter` and every oracle import stay
valid). **The re-port scope (unported v4 surface):** the new pure module
`lib/chat/turn-manager/skip-signal.ts` (358 lines — sentinel detection
with the strip-and-keep-prose `cleaned` path, `isTurnPassMessage`,
`findSkippedSinceLastSubstantive` [the stall guard],
`isFirstCharacterTurn`, `isRecentlyAddressed`, `qualifiesForTurnSkipping`,
`computeSkipEligibility`), `buildTurnSkipInstruction` + the two
buildContext injection routes (trailing section vs its-own-trailing-user-
message on chained turns), the orchestrator eligibility/sentinel/
`handleTurnSkip` path (Host post + `computeSpokenThisCycleAfterSkip`
persist [that leaf is ALREADY ported] + the `skipped` done frame + the
tools-ran-clears-sentinel rule), the `nudge`/`chainSelectionReason`
summoned-withhold threading, the turn-state/chain-gate deltas, the Host
turn-pass writers, the `chats` marshaling catch-up, and the
answer-confirmation catch-up. The Salon Skip-button route, migration,
qtap-export schema, and UI are P4.4/P4.6 surface. The `docs/v4/` mirror is
refreshed (CHANGELOG, DDL.md, nothing-to-add.md, salon-answer-
confirmation.md). **Oracle baseline for new work orders: `a7b1398d` —
but the three failing differentials pin their units to the OLD baseline
until the re-port lands.**

**P4.d1: the answer-confirmation drift catch-up is DONE** (2026-07-09,
v4 `a7b1398d`). `services::answer_confirmation` is current again: the new
pure `build_recent_conversation_context` (the compact recent-dialogue
transcript the re-affirmation anchors to — `type:'message'` +
no-`systemSender` + not-silent + non-blank filtering, the last-20 cap, the
8 000-UTF-16-unit TAIL slice with the `[…earlier conversation truncated…]`
prefix added AFTER the slice [faithful], name attribution REUSING the
Phase-1 `get_participant_name` with the JS-`||` empty-name fallthrough to
the `User`/`Character` role fallbacks), the rewritten re-affirmation
system prompt (`build_reaffirmation_system_prompt` — the optional
`You are <name>. ` anchor over a mechanically-extracted byte-exact body in
`prompt_text`), the labeled-sections re-affirmation user message (the
leading scene block when context is non-blank; the reference relabeled
"your background knowledge — NOT the conversation"; the rewritten closing
instruction), `RunAnswerConfirmationOptions.{character_name,
conversation_context}`, and the finalizer threading
(`message_finalizer` builds the context from the prior messages +
`chat.participants` + the participant-character name map and passes
`character.name`; `FinalizerConfirmationRun` widened — pass-through-only
consumers untouched). Corpus extended 14 → 17 (`scene_over_twenty` — 24
seeded dialogue rows, only the last 20 render, both name paths;
`scene_truncate` — an over-budget transcript with `é` at the cut boundary
+ an astral `🗼` in the kept tail; `scene_none_staff_only` — Staff
whisper + silent + whitespace-only rows → null context, no scene block),
the oracle's `triggers.participantCharacters` now carries the responder
(name attribution live on both sides), and the reaff-call discriminator
keys on the prompt's fixed opening. `answer_confirmation_tier3_equivalence`
regenerated GREEN against v4 HEAD (17 calls, 19 canned completions — 6
anchored re-affirmations, 2 with scene blocks, 1 truncated — + 19
ANSWER_CONFIRMATION `llm_logs` rows); `message_finalizer_tier3_equivalence`
re-verified INERT against a regenerated HEAD oracle (confirmation OFF in
its corpus). Eight new unit tests for the pure leaves. The sibling
turn-skipping re-port (P4.d2) still pins `orchestrator_tier3` /
`enclave_step_tier3` to the old baseline.
**P4.d2: the "nothing to add" turn-skipping port is DONE** (2026-07-09,
v4 `b90cd1f5`; oracle baseline `a7b1398d`). The whole feature is ported
across the already-ported spine, leaf-first per the work order. New pure
module **`crate::skip_signal`** (v4 `lib/chat/turn-manager/skip-signal.ts`
whole, over the ALREADY-PORTED `normalize_content_block_format` /
`strip_character_name_prefix` / `find_mentioned_character_ids` /
`is_participant_present` — v4's `response-normalizer.ts` extraction is a
pure refactor with re-exports, so the ported `message_formatter` stays the
canonical home): the sentinel detection (the wrapper-shedding
`isSentinelLine`, the no-name strip guard, the sentinel+prose → `cleaned`
path), `is_turn_pass_message`/`turn_pass_participant_id`,
`find_skipped_since_last_substantive` (whispers don't terminate the walk),
`qualifies_for_turn_skipping` (>2 active chars OR ≥2 LLM),
`is_first_character_turn` (greetings count, Staff doesn't),
`is_recently_addressed` (the 10-turn visible window + targeted-whisper
hit), and `compute_skip_eligibility` (recentlyAddressed unconditional; the
withhold precedence not-multi-character → feature-disabled →
first-character-turn → summoned → already-skipped → all-others-skipped;
the vacuous-`.every()` stall form is unreachable with a consistent roster
— documented, the `.every()` exercised non-trivially). The history
functions take raw `getMessages` JSON rows. **Turn state:**
`calculate_turn_state_from_history` advances `lastSpeakerId` past a Host
turn-pass record (the converters in `turn_orchestrator` +
`participant_resolver` tag it via `TURN_PASS_VIEW_TYPE` carrying
`hostEvent.participantId` — `MessageView` itself is unchanged, so the
lane-frozen finalizer's converter needed no edit; its walk can never see a
pass as the newest relevant record). **Chain gates:** `should_chain_next`
excludes Staff (`systemSender`) rows from the all-LLM pause counter and
returns `selection_reason: Option<ChainSelectionReason>` (queue |
algorithm) on the chain-true decision; `execute_turn_chain`'s initial +
per-turn gates treat a skipped turn as rotation-advancing (only
no-content-AND-no-skip stops), stamp `skipped` on EVERY chained
`turnComplete` frame, and thread `chain_selection_reason` into the chained
input. **Context:** `BuildContextInput.turn_skip` + the byte-exact
`build_turn_skip_instruction` (base note + the recently-addressed
caution), pushed as the LAST trailing section on a new user message or as
its own trailing `role: user` message on chained/continue turns
(`message_context` passes the input through unchanged). **Spine:**
eligibility computed after the existing-messages read (summoned = `nudge
== true` OR queue-popped; `turnSkippingEnabled !== false`), the sentinel
handling after the wardrobe drain via the unit-tested
`resolve_sentinel_action` precedence (tools-ran clears the bare sentinel
so the tool-save branch wins; offer → `handle_turn_skip`; no offer →
the empty-response branch; cleaned prose falls through as a real reply),
and `handle_turn_skip` (Host post → fresh re-read →
`compute_spoken_this_cycle_after_skip` → the chat update minting
`updatedAt` UNCONDITIONALLY [faithful] → the `hostAnnouncement` frame +
the skipped `done` frame). **Events/writers/marshaling:**
`TurnCompletePayload.skipped` (always present on chained frames), the new
`HostAnnouncement` variant, the skip done frame as the dedicated
`DoneSkipped`/`SkipDonePayload` variant in v4's exact key order;
`host_notifications` gained the three byte-exact turn-pass builders +
`post_host_turn_pass_announcement` (one-key `hostEvent`, errors swallowed,
returns the persisted MessageEvent); `chats` gained the
`turnSkippingEnabled` nullable-boolean marshaling (create + `ChatUpdate`
double-`Option` setter + omit-when-NULL read). **Two shape deviations
from v4 (fold-in at unification, both forced by the binding ownership
matrix — `message_finalizer.rs` is P4.d1's this round):**
`ProcessMessageResult`'s `skipped`/`skipped_participant_id` ride a
`TurnResult` wrapper (Derefs to the inner result, so the untouchable host
spine + enclave step compile unchanged), and the skip done frame is a
separate `ChatEvent` variant instead of two `DonePayload` fields (the
finalizer constructs `DonePayload` as a full literal). **Verified — ten
differentials green against fresh v4-HEAD oracles:** the NEW
`skip_signal_equivalence` (99 rows, tier-1 exact); `turn_state` (+4
turn-pass rows incl. the malformed-guard fall-through);
`turn_orchestrator_tier2` (+the Staff-in-pause-window chat — 2 real turns
+ 1 Staff row ≠ threshold 3 → chain continues — and `selectionReason` on
every decision); `chats_tier2` (+create-true / update-false / null
round-trip on `turnSkippingEnabled`); `chats_read` (+the materialized
toggle on the rich chat); `post_office_host` (+the three builders);
`post_office_writers_tier3` (+the llm AND user turn-pass rows, 21 rows);
**`orchestrator_tier3` regenerated at 27 calls** — every qualifying
group-chat case now carries the Turn note in its recorded canned keys
(byte-proving the instruction + both injection routes), plus four new
cases: `skip_fire` (bare sentinel → the Host turn-pass row + the
`hostAnnouncement` + skipped `done` frames + the rotation advancing past
the passer + the chain continuing to two more turns then the all-LLM
pause), `sentinel_prose` (the cleaned prose persists as a normal reply),
`nudge_withhold` (summoned → no note in the recorded key), and
`skip_disabled` (`turnSkippingEnabled: false` at create → no note);
**`enclave_step_tier3` regenerated at 20 calls** incl. the NEW
`autonomous_pass` case (a room's speaker passes → the Host turn-pass row,
no assistant message, the turn still counts against the run budget, the
job re-enqueues — the enclave step itself needed zero logic change, only
the result-shape ripple); and `build_context_tier3` /
`message_context_leaves` / `primary_stream_tier3` re-verified inert.
Test-infra catch-ups: the enclave/host-boot hand-rolled chats DDLs + the
committed quilltap-web fixture gained the new column (the web fixture via
an idempotent ALTER at test setup — v4's `add-turn-skipping-field-v1`
migration effect on an old instance). **Out of scope per the order
(P4.4/P4.6):** the Salon Skip-button route + chat-GET `canSkipTurn`, the
migration script, the qtap-export schema line, help content, and all UI.

**P4.d unification (2026-07-09): DONE — both drift re-port lanes are
integrated on main; the three stale spine differentials are green again at
v4 HEAD `a7b1398d`, and the oracle baseline advances to `a7b1398d`
unconditionally.** The two lane branches (P4.d1 answer-confirmation
catch-up; P4.d2 turn-skipping) cherry-picked with only the two doc unions
(CLAUDE.md/CHANGELOG) — zero source-level conflicts for the sixth
consecutive round; every shared `Cargo.toml` delta was verified
version-only and identical (both lanes → core 0.0.142 / harness 0.0.133).
**The two ownership-forced workarounds P4.d2 flagged were FOLDED at
unification** (the reason this pass exists): (1) the `TurnResult` wrapper
dissolved — `skipped`/`skipped_participant_id` now live on
`ProcessMessageResult` proper (`message_finalizer.rs`), every constructor
sets them, the `Deref` shim is deleted; (2) the dedicated
`DoneSkipped`/`SkipDonePayload` variant dissolved — `DonePayload` gained
the two optional fields declared between `tools_executed` and `turn` so
the skip frame serializes v4's exact `{…, toolsExecuted, skipped,
skippedParticipantId, provider, modelName}` order (a new byte-level unit
test pins the serialized STRING), and `handle_turn_skip` emits a plain
`ChatEvent::done`. One straggler test-infra catch-up: the `host_cadence`
danger-scan fixture's hand-rolled chats DDL gained `turnSkippingEnabled`
(the sibling fixtures were caught by the lane). Verified on the
integrated tree: the full workspace gate (**1,127 tests / 0 failed**;
clippy `-D warnings` on default AND `native-transport`; fmt) and a
**thirteen-differential sweep** against freshly regenerated v4 oracles at
`a7b1398d` — the lanes' own proofs (`skip_signal` 99 rows,
`answer_confirmation` 17 calls, `orchestrator_tier3` 27 calls,
`enclave_step_tier3` 20 calls, `turn_state`, `turn_orchestrator_tier2`,
`chats_tier2`, `chats_read`, `post_office_host`,
`post_office_writers_tier3`, `message_finalizer_tier3`) plus the two
fold cross-checks (`courier_transport_tier3`, `primary_stream_tier3` —
both `DonePayload` consumers). **Regen gotcha recorded:** the
enclave-step oracle MUST be generated with `TZ=UTC` in the invocation
env (the recipe's line — V8 caches the local zone at process start, so
the in-file `process.env.TZ` pin is not sufficient); a local-TZ regen
diverges only on `scheduleNextRunAt` (the croner local-Date semantics).
Versions: core 0.0.142, harness 0.0.133, host 0.0.5, web 0.0.2.
**Standing follow-ups (unchanged from the lanes + the prior round):**
the P4.4/P4.6 turn-skipping surface (Skip-button route + `canSkipTurn`
GET + migration + qtap-export + UI + help), the two live orchestrator
corpus cases (ask_carina sink threading; the Brahma `isDefault` fixture
profile), the spine failover-log threading, and the P4.4 ∥ P4.5 round
(route-logic backfill ∥ SPA foundation) as the next planned work.

**P4.4 unit 1 (the unlock/pepper-vault service + fresh-instance
provisioning) is DONE.** The CORE now creates an
**encrypted-from-byte-zero** instance at `Setup` — no plaintext window
(v4 creates its DBs plaintext in pre-setup migrations then encrypts in
place; v5 keys every partition on creation via `Writer::open_writable`).
New `quilltap-core::services::provisioning`: `provision_fresh_instance`
opens the three writable partitions, replays the captured **generateDDL
schema** (the tier-2-fixture-proven, v4-compatible surface — v4's real
repositories create it on first access; a migration-accumulated instance
has the SAME column set but a different column ORDER, which v4's
column-name-addressed repos accept, so a byte-match with the
migration-accumulated form is a tracked deferral needing the migration
runner, unnecessary for correctness), and seeds v4's deterministic
first-boot rows. The schema + the captured `chat_settings` seed row live
in the crate (`fresh_schema.json` / `chat_settings_seed.json`,
`include_str!`'d), dumped from v4 by
`harness/oracle/provision/dump-fresh-schema.ts`. **The seed:** the single
user (composing the ported `users.create`) + the default `Built-in
TF-IDF` embedding profile (`embedding_profiles.create`) + the default
chat settings via a **raw INSERT of v4's captured row** — the ported
`ChatSettings` nested structs serialize optional keys as explicit `null`
(built for the always-present tier-2 corpus), but v4's `updateForUser`
OMITS keys the input doesn't supply, so byte-exact seeding replays the
capture rather than composing `create`. (New v4 fact: `users.create`
called with no `options.id` MINTS an id; v4's boot converges to
SINGLE_USER_ID via multiple `getOrCreateSingleUser` calls whose 2nd finds
the row by email and migrates it — the oracle calls it twice; v5
provisions directly at the converged id.) New contract:
`Request::{Setup, StorePepper, ChangePassphrase}` +
`Response::{Setup, Ack}` + `ErrorKind::Unauthorized` (→ HTTP 401); the
engine wires them (setup provisions+assembles from `needs-setup`
[verbatim v4 `setupDbKey` message, pepper shown once]; store writes the
`.dbkey` from `needs-vault-storage`; change-passphrase re-wraps from
`resolved`). `dbkey` gained `change_passphrase` (decrypt-with-the-old-
passphrase — NOT try-internal-first — re-wrap, write BOTH `.dbkey` files
for v4 parity; no DB re-encryption) over refactored
`encrypt_dbkey_json`/`write_dbkey_file` helpers. **Verified:** the
provisioning differential (`provisioning_equivalence.rs`, gated) —
v5's `sqlite_master` per partition equals v4's LIVE generateDDL schema
byte-for-byte; the seed rows (users/chat_settings/embedding_profile)
match with minted id/timestamps normalized; both cross-compat
directions — a v4-built instance opens under v5's ported reads, a
v5-provisioned instance opens under v4's REAL repositories
(`verify-v5-provisioned.ts`), and a v5 change-passphrase `.dbkey`
unlocks under v4's real `unlockDbKey` (`verify-dbkey-crosscompat.ts`) —
plus the web `/setup` e2e over real HTTP (empty dir → 423/needs-setup →
`setup` dispatch → the host's REAL spine assembles against the fresh
instance → `listChats` = `[]`) and unit tests for provisioning, the
engine setup/store/change flows, and the dbkey round-trips. **Named
deferrals:** the sample-content seed import
(`first-startup/imports/lorian-and-riya.qtap` → 2 characters + 42
memories + avatars, drags in the unported import service — a fresh
instance boots and the SPA is fully usable with zero characters); the
built-in roleplay templates (`Standard`/`Quilltap RP` — need the
`delimiters` discriminated-union marshaling completed on the ported
`roleplay_templates` repo); and the three built-in mount stores
(`Quilltap General`/`Quilltap Uploads`/`Lantern Backgrounds` +
`instance_settings` pointers — needed before the image/upload/general-
scenario verticals). **Unit 2 — the chat creation flow + Green Room (D6)
— is the next P4.4 order** (deferred this round: the survey shows it
composes several large unported subsystems — `buildChatContext`, the
greeting/first-message generator, the identity-stack compiler, scenario
mount resolution, autonomous-room start — so it is its own full unit).
Versions: core 0.0.143, harness 0.0.134, web 0.0.3.

**P4.5 (the Angular SPA foundation) is DONE.** New `apps/web` — Angular 21
(standalone + zoneless + signals, esbuild, Tailwind v4, Vitest), npm, no
component library (D13). **`CoreClient`** is the one transport seam (D14):
`dispatch` over `POST /api/dispatch`, ONE global `EventSource` on
`/api/events` (scope-tagged; reconnect = resync), and the `/health`
readiness vocabulary; the hand-written TS contract mirror lives in ONE
module (`src/app/core/core-contract.ts`) for at-a-glance diffing against
the Rust enums; TanStack Query (the Angular adapter) layers server state
over dispatch. **The SSE stream reducer** (`chat-stream.reducer.ts`) is a
pure fold of chat-scoped `ChatEvent` frames ported from v4's
`useSSEStreaming`/`useMessageStreaming`: content append, cumulative
reasoning live-replace, tool-batch splice at `anchorOffset`, turn/chain
lifecycle, the skip/empty/pending-external done family, mid-stream error —
with the v5 structural adaptation (subscribe by `chatId` BEFORE
dispatching; the dispatch promise resolves at turn completion) and a
committed frame-trace fixture. **The `qt-*` CSS system** ports
file-per-file (`src/styles/qt-components/_*.css` + globals), plus the six
bundled theme packs (styles/tokens/fonts/textures under `public/themes/`)
behind a `ThemeService` that applies by id, injects `@font-face`, and
persists to localStorage; the base UI primitives (icon, brand-name,
loading/empty/error states, form-actions, section-header, avatar,
chevron) carry v4's classes + microcopy verbatim. **Screens:** the
startup gate (health → `pepperState` routing + the 409 lock-conflict and
unhealthy cards), unlock, the setup wizard (one-time pepper reveal; both
`needs-setup` and `needs-vault-storage` modes), and the app shell (nav
skeleton, theme switcher, the `listChats` list). **Verified:** 39
component/unit tests (reducer over the committed trace, CoreClient
parsing, ThemeService, wizard/unlock/gate) + Playwright e2e against the
REAL `quilltap-web` binary over a passphrase-locked COPY of the committed
fixture (locked → wrong-passphrase error → unlock → shell + a bundled
theme applied). **Documented divergences** (reconcile when the server
themes service lands with the Settings vertical): the theme asset-URL
rewrites (`/api/themes/assets` → bundled `/themes/...`) and the
localStorage theme persistence. SPA at 0.1.x; no crate changes from this
lane.

**P4.4/P4.5 unification (2026-07-09): DONE — both lanes are integrated on
main.** Zero source-level conflicts for the seventh consecutive round
(the one union: `docs/CHANGELOG.md`); ownership held exactly (P4.4 only
`crates/**` + `harness/oracle/**`; P4.5 only `apps/web/**` +
`.gitignore`). **The shared-contract cross-check passes byte-for-byte**:
the TS mirror's `setup`/`storePepper`/`changePassphrase` requests, the
`{"type":"setup","data":{pepper,message}}` / `{"type":"ack","data":{}}`
responses, and the kebab-case `unauthorized` error kind all match the
Rust serde output exactly. **The deferred LIVE setup-wizard e2e is
CLOSED at unification** (`apps/web/e2e/setup-flow.spec.ts`): a second
server against an EMPTY data dir walks needs-setup → the wizard
(mismatched-confirm validation) → the real `setup` dispatch → the
one-time pepper reveal → the shell on the freshly provisioned encrypted
instance (`quilltap.db` + `quilltap.dbkey` on disk) — the full first-run
story browser-to-disk. **Verified on the integrated tree:** the full
workspace gate (1,136 tests / 0 failed; clippy `-D warnings` on default
AND `native-transport`; fmt), the provisioning differential regenerated
green against v4 HEAD `a7b1398d` (schema byte-exact per partition, seed
rows, v5-reads-v4) plus BOTH v4-side cross-compat scripts (v4 opened the
v5-provisioned instance; v4 unlocked the v5 change-passphrase `.dbkey`),
and the SPA suite (39 unit tests + 2 Playwright e2e incl. the new live
setup flow). Versions: core 0.0.143, harness 0.0.134, web 0.0.3, SPA
0.1.1. **Standing follow-ups:** P4.4 unit 2 (the chat creation flow +
Green Room D6 — its own order; the P4.4 survey confirmed it composes
several large unported subsystems: `buildChatContext`, the
greeting/first-message generator, the identity-stack compiler, scenario
mount resolution), the P4.4 named deferrals (the sample-content seed
import, the built-in roleplay templates, the three built-in mount
stores), the P4.5 theme divergences (fold when the themes service
lands), and then the P4.6 first Salon vertical (M4) per the
decomposition.

**P4.4 unit 2 (chat creation + the Green Room) is in progress** (solo lane;
oracle baseline `a7b1398d`, no drift at lane start). Ported leaf-first.
**Sub-unit 1 — the preset-scenario resolvers — is DONE and green**
(`db::scenarios`, `scenario_resolvers_equivalence`): v4's
`resolveScenarioBody` slice (`lib/mount-index/{scenarios-common,project,
group,general}-scenarios.ts`) — resolve a chosen preset scenario's body
(post-frontmatter, trimmed) out of a document store's `Scenarios/` folder,
composing the verified `read_database_document` + `parse_frontmatter`; the
three scoped wrappers (project/group take a resolved `mountPointId`, general
reads the "Quilltap General" pointer from main-DB `instance_settings`). The
path normalization (folder prefix, `/\.md$/i` suffix, leading-slash strip)
and the swallow-to-null-on-miss are byte-faithful. Read-differential drives
v4's REAL `resolveGeneralScenarioBody`/`resolveProjectScenarioBody` over a
baked two-store fixture (bare / full-path / missing-`.md` / leading-slash /
missing-file → null / whitespace-only-body → null). The
list/read-by-path/set-default write surface is a **P4.6 deferral**.
**Sub-unit 7 — the Green Room creation-progress bus (D6) — is also DONE**
(`services::creation_progress`; v4 `lib/chat/creation-progress.ts`): the
`kind`-tagged frames (status/log/wardrobe-start/wardrobe-result/done/error)
are a new `api::EventPayload::CreationProgress` variant scope-tagged by
`progress_id` on the one global `/api/events` stream, plus the core-adjacent
`CreationProgressBus` (200-frame cap, replay-on-subscribe via
`active_snapshot`, 60 s TTL after the terminal `done` — pruned lazily, no
core timer) and the inert-without-`progressId` `CreationProgressEmitter`
(fans each frame to the bus + the live broadcast). v4's un-emitted terminal
`error` frame is faithful (`fail` ported, never called by `handleCreate`).
Unit-tested for cap/replay/TTL + the v4 frame serialization; the frame
TRACE is diffed in the capstone, and the transport replay-on-subscribe
wiring lands with the spine. **Sub-unit 2 — `buildChatContext` — is also
DONE** (`services::chat_initialize`; v4 `lib/chat/initialize.ts`): resolves
the `{systemPrompt, firstMessage, character, userCharacter}` seed bundle —
the vault-overlaid responding character, the optional user-controlled
character (explicit id or the character's `defaultPartnerId`, gated on
`controlledBy === 'user'`), the system-prompt selection
(`selectedSystemPromptId` → `isDefault` → first → nothing), the scenario
override, and the template pass — porting `initialize.ts`'s OWN flat
`buildSystemPrompt` (distinct from the per-turn identity-stack builder) over
the verified template processor + `characters_read`. Read-differential
(`chat_context_init_equivalence`) drives v4's REAL `buildChatContext` over a
baked three-character fixture (llm / user / llm-with-`defaultPartner`),
comparing `systemPrompt` + `firstMessage` + resolved character/user-character
ids and names (bare / user+scenario / selected-non-default-prompt /
default-partner), zero normalization. **Sub-unit 3 — the identity-stack
compiler write side — is also DONE** (`services::system_prompt_compiler`; v4
`compiler.ts` `compileAllIdentityStacks`): precompiles each LLM-controlled
CHARACTER participant's identity stack (the verified `build_identity_stack`
with `{{user}}`/`{{scenario}}`/`{{persona}}` resolved) and persists the
`{participantId → stack}` map to `chats.compiledIdentityStacks` via a new
`ChatUpdate.compiled_identity_stacks` setter (nullable JSON object, no
`updatedAt` bump — the `compression_cache` pattern). Errors never propagate
past the create handler. Tier-2 differential (`identity_compiler_equivalence`)
drives v4's REAL `compileAllIdentityStacks` over a baked chat (Aria/llm rich,
Bob/llm, Sam/user, Ghost/llm-removed), diffing the persisted map byte-for-byte
(only the two active LLM participants; user/removed skipped;
`physicalDescription` surfaces). The single-participant compile is a P4.6
deferral; the spine populating `precompiled_identity_stack` per turn is
verified in the capstone. **Sub-unit 4 — outfit selections — is also DONE**
(`services::outfit_selections`; v4 `apply-outfit-selections.ts` +
`chooseLLMOutfit`): `apply_outfit_selections` dispatches each character's
`OutfitSelection` (`default`/`manual`/`none`/`previous_chat`/`llm_choose`) to
`set_equipped_outfit`, composing `resolve_default_outfit` (default-marked
items, oldest-first, per-slot) + the byte-exact `OUTFIT_SELECTION_PROMPT` +
its id/slot-validating response parser over the verified `CheapLlmTaskExecutor`
+ wardrobe reads, with the `6bf88959` progress narration
(wardrobe-start/wardrobe-result/log) riding the Green Room emitter.
**Documented seam:** the ported executor's infallible parser means a
malformed-JSON response yields empty slots (vs v4's throw → default-fallback);
the corpus keeps responses valid JSON and drives the fallback via a provider
failure. The pure leaves (default resolution, prompt layout, parser) are
unit-tested here; the composed `applyOutfitSelections` tier-3 diff
(`equippedOutfit` + progress frames) rides the capstone. **Sub-unit 5 — the
initial-greeting core — is also DONE** (`services::initial_greeting::
generate_greeting_message`; v4 `initial-greeting.ts`): streams a short
in-character greeting over the streaming model boundary (v4 `streamMessage` +
concatenate), accumulates content + usage, returns `{content,
contentFilterDetected}`; `buildContextSection` folds project + memories + the
recent-conversations block into the augmented prompt; `logLLMCall`
(`CHAT_MESSAGE`) is an optional injected config. DB-free tier-3 differential
(`initial_greeting_equivalence`) drives v4's REAL `generateGreetingMessage`
(streaming provider + `logLLMCall` mocked), recording the request messages
(proving the augmented prompt bytes) and diffing `{content,
contentFilterDetected}` across success / content-filter / empty-no-usage /
whitespace-only / with-context. The route ladder `autoGenerateFirstMessage`
(participant/profile/key + the four-attempt retry matrix + the Concierge
reroute) is the spine's (capstone-verified). **Sub-unit 6 — chat continuation —
is also DONE** (`services::chat_continuation`; v4 `apply-chat-continuation.ts`):
`apply_chat_continuation` posts the Host continuation-from bubble, replays the
carryover window (most-recent Librarian summary onward) with participant ids
remapped by shared `characterId` + lifecycle fields stripped, replicates turn
state with the same remap, and posts the continuation-to tail bubble in the
source chat last — composing the verified Host writers + the single-writer
message/update path. The pure leaves (participant-id map, librarian anchor,
message projection: drop-unmapped-author / drop-all-targets-gone / hostEvent
remap) are unit-tested; the composed diff (both chats' tables, minted-remap
form) rides the capstone (the continuation-create case). **All six leaf
sub-units are done; next is sub-unit 8 — the `handleCreate` spine + `ChatCreate`
dispatch + the capstone tier-3 differential + the quilltap-web integration
test.**

**P4.4u2 unification (2026-07-09): the seven leaf sub-units are integrated
on main** (a pure fast-forward — the solo lane branched from main HEAD, so
zero conflicts; ownership held exactly: only `crates/**` +
`harness/oracle/**` + docs). Verified on the integrated tree: the full
workspace gate (**1,161 tests / 0 failed**; clippy `-D warnings` on default
AND `native-transport`; fmt) and the four gated differentials re-run green
against FRESHLY regenerated v4 oracles at `a7b1398d`
(`scenario_resolvers_equivalence`, `chat_context_init_equivalence`,
`identity_compiler_equivalence`, `initial_greeting_equivalence`; sub-units
4/6/7 are unit-test-verified with their composed diffs riding the
capstone). Versions: core 0.0.150, harness 0.0.138. **Next: the P4.4u2b
order — sub-unit 8** (the `handleCreate` spine + `Request::ChatCreate` +
the `ChatCreateDriver` host assembly + the capstone tier-3 differential +
the quilltap-web integration test); the full inventory is in the
`[[p4-4-u2-chat-creation]]` memory note and the unit-2 work order. Then
P4.6 (the first Salon vertical, M4).

**P4.4u2b (the `handleCreate` spine + `ChatCreate` dispatch): DONE and
unified on main (2026-07-10) — P4.4 unit 2 (chat creation + the Green
Room) is COMPLETE.** The solo lane landed three commits (pure
fast-forward; one `cargo fmt` fix folded into the capstone commit at
unification): **(1)** `services::chat_create` — `handle_create` composing
the seven leaf sub-units in v4's exact order (continuation ownership
precheck → autonomous preconditions [user-controlled reject, ≥2 LLM,
cron via `enclave::cron`] → `buildAllParticipants` +
`pickWeightedByTalkativeness` [RNG injected] → the scenario precedence
chain → `build_chat_context` → participant minting → project
defaults/roster auto-add → `chats.create` [+ the autonomous column
block] → outfits [never fatal] → the compiler → the
continuation/autonomous/normal branch over the seed writers → enrich →
the ad-hoc autonomous auto-start → finish) +
`auto_generate_first_message` (the 4-attempt ladder incl. the
content-filter Concierge reroute) + the previously-unported memory-recap
recent-conversations helpers; `services::chat_enrichment`
(`enrich_participant_summary` + `get_character_summary`, the
no-preloaded 201-body path); `photos::resolve_character_avatar` (the URL
half); `api::chat_create` (`ChatCreateDriver`) + `Request::ChatCreate` /
`Response::ChatCreate` + the readiness-gated engine arm. Two write
paths: the caller-opened writable connections for the `&Connection`
sub-units + direct repo writes, and the single-writer `Db` for the seed
writers / continuation / avatar / greeting-log. **(2)** The production
`ChatCreateSpine` in `quilltap-host::spine` — per dispatch it opens its
OWN writable Writers (busy_timeout guards the rare overlap with the
engine writer thread; the outfit sub-unit holds writable connections
across an LLM await, which the sync `Db::write` channel cannot host),
shares the ChatSpine provider Arcs + the cheap-LLM executor +
`DbApiKeys`, and runs on the Send-bridge dedicated thread; the engine
owns ONE shared `CreationProgressBus` and the `/api/events` SSE replays
`active_snapshot` to late subscribers; `chat_create_end_to_end` proves
it over real HTTP (201 + listChats + a LATE subscriber replaying the
Green-Room frames). **(3)** The capstone tier-3 differential
(`chat_create_capstone_equivalence`) driving v4's REAL `handleCreate`
(jest, mocked NextRequest/auth, model boundaries canned by recorded
keys) over a 6-case corpus — single-char first message, two-char +
scenario (Host adds + scenario + Prospero + Aurora seed rows
byte-exact), no-progress (inert emitter), autonomous ad-hoc (auto-start
→ the AUTONOMOUS_ROOM_TURN job + run-start banner), generated greeting
(the ladder builds a byte-identical prompt → canned stream hit), and
autonomous cron (next-run stamped, TZ=UTC oracle) — diffing 6 sections
each (`chats` / `chat_messages` / `projects` / `background_jobs` / the
ordered Green-Room frame trace / the 201 DTO) in the minted-values remap
form. Unification verified: the full workspace gate (**1,171 tests / 0
failed**; clippy `-D warnings` default + `native-transport`; fmt) and
the capstone re-run green against a freshly regenerated v4 oracle at
`a7b1398d`. Versions: core 0.0.152, harness 0.0.139, host 0.0.6, web
0.0.4. **Tracked follow-ups (two fidelity findings + the corpus
extension, one already in flight as a spun-off subtask):** (1) the
persisted `chats.participants` drops explicit-null
`connectionProfileId`/`imageProfileId`/`selectedSystemPromptId` (needs
the `removedAt` double-`Option` pattern + `chats_tier2`/`chats_read`/
`chats_participants` regens — normalized as a bounded seam in the
capstone, a genuine null-vs-value divergence still surfaces); (2) the
201 DTO body is built from a re-read (NULL columns dropped) vs v4's
create-echo (explicit nulls kept) — flagged for P4.5/P4.6 confirmation;
(3) extend the capstone corpus to the order's floor (continuation
create, the outfit modes + failure, the scenario-precedence path cases,
the greeting retry/reroute branches, no-connection-profile). Standing
deferrals unchanged (`handleImport`, participant `?action=` verbs, chat
merge, `handleList` enrichment → P4.6). **Next: P4.6, the first Salon
vertical (M4)** — it consumes the `chatCreate` contract; report the TS
mirror shape to that order.

**Follow-up (1) — the participants explicit-null marshaling seam — is
now CLOSED** (2026-07-10, the spun-off subtask, unified as a pure
fast-forward). `ChatParticipant`'s `connectionProfileId` /
`imageProfileId` / `selectedSystemPromptId` are the present-keeps-null
double-`Option` (the `removedAt` pattern + `de_double_opt_string`;
`roleplayTemplateId` stays single-`Option` — v4's
`buildCharacterParticipant` never writes it), banked with explicit-null
participant rows in the `chats-tier2` corpus, and the capstone's
`strip_participant_null_seam` normalizer is DROPPED — the persisted
participant nulls diff byte-exact. The double-`Option` stays contained
at the DB marshaling boundary (consumers project into their own input
structs, e.g. `RespondingParticipant`, so no service files changed).
Unification verified: the full workspace gate (1,171 tests / 0 failed;
clippy `-D warnings` default + `native-transport`; fmt) and SIX
differentials re-run green against freshly regenerated v4 oracles at
`a7b1398d` (`chats_tier2`, `chats_read`, `chats_participants_tier2`,
`chats_messages_tier2`, `identity_compiler`, and the chat-create
capstone with the strip removed). Versions: core 0.0.153, harness
0.0.140. Remaining chat-creation follow-ups: (2) the create-echo DTO
shape and (3) the capstone corpus extension, above.

**Phase 4 (P4.6): the first Salon vertical is DONE — milestone M4 stands,
run live** (2026-07-10; two parallel lanes unified on main, zero
source-level conflicts, one CHANGELOG union). **P4.6a (the Salon server
surface):** new `api::salon` dispatch handlers + contract variants —
`chatSettings`, the enriched `listChats`
(`excludeTagIds`/`limit`/`includeAutonomous`; `services::chat_enrichment`
grew the LIST orchestration [`enrich_chats_for_list` + tag filtering +
`_allTagIds` stripped via `#[serde(skip)]`; the batched-list vault-only
avatar quirk reproduced] and the DETAIL participant path
[`enrich_participant_detail`/`get_character_detail` incl. the
avatar-override branch]), `chatGet` (the full single-chat projection —
enriched participants, all messages, off-scene characters, agent-mode
cascade — minus the deliberately-omitted `renderedHtml`: the **locked
markdown divergence**, v5 renders client-side), the turn action
(query/nudge), message edit / delete (the memory-cascade confirmation
protocol) / swipe-switch, the Salon-minimal chat PUT (isPaused/title), the
three impersonation verbs, and the extended `chatSend` gate (the
superRefine blank-content rejection + `nudge` + `pendingToolResults`
pre-inserted as TOOL messages). Verified by `salon_reads_equivalence`
(6 cases: settings + 3 list variants + solo/group GET) and
`salon_mutations_equivalence` (11 cases, zero-mint zero-normalization) —
both byte-exact vs v4's REAL route handlers over the new committed Salon
web fixture (`crates/quilltap-web/tests/fixtures/salon-*.db`). New reads:
`tags::find_by_ids`, `conversation_chunks::count_stats_by_chat_id`.
**P4.6b (the Salon SPA):** real Angular routing (`/salon` + `/salon/:id`),
the list as v4-faithful `ChatCard`s over the enriched DTO, the
conversation read path (swipe-group collapse, render-item pipeline with
staff announcement chips / whisper + silent labels / reasoning blocks),
a **byte-for-byte TS port of v4's `renderMarkdownToHtml`** (pinned
unified/remark/rehype + roleplay-rendering + qtap-linkify, verified
against 23 fixtures captured from v4's real renderer), streaming send over
the P4.5 reducer (optimistic bubble, live markdown, done → canonical
refetch), tier-1 message actions (copy / edit / delete + cascade dialog /
regenerate + swipe arrows), the textarea-MVP composer, and the header with
`CopyChatIdButton`. 76 Vitest tests. **Unification verified:** the full
workspace gate (1,174 tests / 0 failed; clippy `-D warnings` default +
`native-transport`; fmt), fresh-oracle re-runs of both Salon differentials
+ `orchestrator_tier3` (the lane's orchestrator threading inert on the
corpus), and ALL THREE Playwright specs green including the **live M4
e2e** — unlock → list → open the baked group history (staff chip) → send
in the solo chat → the streamed mock-LLM reply renders live and survives
reload, through the real binary + spine + an OPENAI-compatible mock.
**Unification wiring:** the e2e instance switched to the Salon fixture,
the user-id rewrite extended to the user-scoped tables the send path reads
(api_keys/connection_profiles/chat_settings/…), the mock `baseUrl` rewrite
moved BEFORE server launch (the CLI write-lock refuses a live holder — the
spec's original in-test rewrite could never work; the mock now listens on
a fixed `MOCK_LLM_PORT`), and the M4 spec un-skipped +
unlock-state-tolerant. Versions: core 0.0.154, harness 0.0.142, host
0.0.7, web 0.0.5, SPA 0.2.1. **Tracked follow-ups:** the turn
`skipUserTurn` differential case (a minted-value Host post, excluded from
the zero-mint differential), swipe **generate** through dispatch (the
model driver), the `pendingToolResults` orchestrator corpus case, the full
`processChatUpdates` field set (roster/conciergeState families), the GET
attachment-resolution branch + chat-settings default-injection branch, and
the SPA tier-2 controls (Skip banner + the skip-signal TS port,
Speaking-As, pause/resume) — plus the standing full-Salon deferrals
(Document Mode pane, terminal pane, courier UI, images, sidebar/modals,
Lexical). **Next:** the remaining Salon slices or the Settings vertical
per the P4.6+ screen-family list in `phase-4.md`.

**P4.6c/d/e unification (2026-07-10): DONE — the Salon consolidation, the
Settings server surface, and the Settings SPA are integrated on main; the
first-run story is complete end to end.** The three lane branches
cherry-picked with zero source-level conflicts for the eighth consecutive
round (CHANGELOG + version unions only; the SPA lock re-synced to 0.3.0).
**Both named unification wires closed live:** (1) the swipe-generate
engine-arm swap — `EngineAssembly`/`ReadyEngine` carry the P4.6c
`SwipeGenerateDriver` (+ a `ready_swipe` gate), the `MessageSwipe` generate
branch delegates to it, `SpineBundle` exposes the `ChatSpine` impl; (2) the
P4.6d provider wire actions went LIVE via the new `api::provider_actions`
module — the dyn-erased `ProviderActionsDriver` (the `ChatSendDriver`
precedent) + the live seam impls composed IN CORE over `SyncWireTransport`
(the W4.7f `Real*Provider` precedent; the host factory plugs
`io.sync_wire_transport()` + the shared completion): the per-provider
`validateApiKey` matrix surveyed from v4 (SDK-family models-list GET with
the requiresApiKey guard, OPENAI's `POST /v1/moderations {"input":"test"}`
probe, ANTHROPIC [claude-haiku-4-5, max_tokens 1] / GOOGLE
[gemini-2.5-flash] minimal-completion probes via the ported request
builders, OLLAMA `/api/tags`; every wire failure → `Ok(false)`, never
`Err` — v4's catch), and the live models fetcher
(`models_list_request`/`parse_models_list` + the transcribed 11-model
anthropic static fallback; unknown provider is the one `Err`). **Documented
divergence:** the per-plugin model-metadata enrichment
(`getModelsWithMetadata`/`getModelInfo`) is v4 plugin data not in the
manifest — `modelsWithInfo` carries `{id}` rows only (same net cache effect
as v4's metadata-less providers). **The live Settings e2e caught a real
port bug, fixed per the discipline:** the chat-settings PUT deserialized
nested `cheapLLMSettings`/`themePreference` via the strict storage structs,
but v4's base-repo merge-then-`validate` runs the FULL nested Zod schema —
a partial bag (the wizard's exact `{strategy:'PROVIDER_CHEAPEST'}` save)
materializes the defaults and OMITS the nullable-optional ids. The PUT now
applies Zod-parse semantics (`zod_cheap_llm_settings` /
`zod_theme_preference` — defaults, unknown-key strip, schema order,
present-null kept vs absent omitted), proven by two new corpus cases in the
regenerated 21-case `settings_routes_equivalence` (byte-exact vs v4's REAL
handler). Verified on the integrated tree: the full workspace gate (clippy
`-D warnings` default + `native-transport`, fmt), a **twelve-differential
fresh-oracle sweep** at `a7b1398d` (salon skip/swipe-generate/mutations/
reads, settings routes [21 cases] + wire actions, providers listing, the
28-case orchestrator regen, connection_profiles/provider_models/
chat_settings tier-2s, regenerate_swipe_tier3 — all green, zero
divergences), the SPA suite (139 Vitest), and **ALL FIVE Playwright specs
green including the newly-LIVE Settings first-run walk**: fresh instance →
setup → the provider wizard → a validated OPENAI_COMPATIBLE profile against
the mock LLM → the profile in the Providers tab (three spec corrections:
v4's real hyphenated `OpenAI-Compatible` display name, the
no-key-input optional-key step [`requiresApiKey: false` renders no key
field], a strict-mode locator). Versions: core 0.0.156, harness 0.0.143,
host 0.0.9, web 0.0.6, SPA 0.3.1. **Tracked follow-ups:** the UUID-format
check on the cheap-LLM id fields (type-level only — the Zod `z.uuid()`
seam, corpus sends valid ids); P4.6d's named deferrals (the themes service
[the SPA keeps client-side bundled packs], embedding/image profile route
families, api-key export/import + auto-associate, auto-configure, tag
actions, the Templates/Data-tab route families); P4.6c's (the mount-file
attachment branch, participant/conciergeState PUT families, the impersonate
menu + turn-queue UI); P4.6e's (the `.qtap-theme` registry UI, the other
five tabs' placeholder cards, key export/import dialogs); and the standing
full-Salon list. **Next:** the remaining Salon slices (Document Mode /
terminal / courier / images) or the Memory/Images/Templates verticals, per
the P4.6+ screen-family list in `phase-4.md`.

**Next-round prep (2026-07-10): the four-lane round is planned, orders
written** (drift check clean, v4 HEAD still `a7b1398d`; four fresh
surveys): **P4.6f** the Characters server surface (dispatch backfill over
the fully-ported characters repo layer; the four LLM services — wizard /
optimizer / rename / ai-import — deferred), **P4.6g** the Characters SPA
(list/view/edit/create over a pinned Shared contract; the ~5k-line
wardrobe dialog + AI wizards deferred as their own verticals), **P4.6h**
Salon virtualization (dogfood finding #3b — v4 itself virtualizes via
`@tanstack/react-virtual` + a `useAutoScroll` controller; the order ports
that architecture, keeps the client-side-markdown locked divergence since
windowing bounds the render cost, and adds a separate long-chat fixture +
the scroll e2e beat), and **P4.4u3** the built-in seeds (Standard /
Quilltap-RP roleplay templates incl. the deferred `delimiters`
discriminated-union marshaling — v4 seeds update-in-place on EVERY
startup, find-by-(name,isBuiltIn) — plus the three built-in mount stores
with settings-pointer idempotent provision-or-adopt + subfolder scaffolds,
wired into fresh provisioning AND every assembly; the `lorian-and-riya.qtap`
sample import stays deferred with its ~2,500-line import service). Round
layout + ownership matrix in `phase-4.md`; orders under
`docs/developer/porting/work-orders/` (p4.6f/g/h, p4.4u3).

**P4.6f slice 1 — the characters READ surface (2026-07-10).** Lane A opens
with the characters read handlers as dispatch variants. The binding
`Request`/`Response` contract for the whole characters + tags family is
declared up front (shared verbatim with the P4.6g SPA lane; a name change
touches both orders). New `services::character_enrichment` ports v4's
`enrichWithDefaultImage` reduced wrapper + the hand-assembled list whitelist
DTO (`characters/handlers/get.ts:58-92`) + the detail spread, reproducing v4's
JS `||` (falsy→null) vs `??` (nullish→default) coercions and the N+1
partner-name / chat-count fan-out. Handlers (`api::characters`): `character_list`
(in-memory npc/controlledBy filter, createdAt-desc sort), `character_get`,
`character_default_partner`, `character_get_tags`, and the `prompts` /
`scenarios` / `wardrobe` / `plugin-data` (map + item) sub-resource GETs.
Marshaled reads added: `character_plugin_data::{find_by_character_id,
get_plugin_data_map, find_by_character_and_plugin}` and
`tags::find_details_by_ids`. Two seams closed against the oracle: plugin
`data` round-trips as its **raw stored JSON string** (v4 does not re-parse the
column), and a tag's `visualStyle` is **omitted** when null (v4's `.optional()`
→ undefined → dropped). Fixture (`build-characters-fixture.ts` + `characters.json`
+ committed `characters-{main,mount}.db`): five characters covering
favorite/npc/controlledBy(both)/canBeCarina/default-partner-pair/tags/two
system-prompts(one default)/two scenarios(one default)/a vault avatar/a legacy
avatar/two wardrobe items/plugin data/a broken-vault character. Proven:
`characters_reads_equivalence` — 13 cases vs v4's real route handlers, byte-exact
after key-sort + number-canon (the detail's read-time-minted
`physicalDescription.{createdAt,updatedAt}` normalized, the established
char-read pattern). NOTE for later slices: v4's character `addTag`/`removeTag`
are the generic `TaggableBaseRepository` pattern (findById → push/filter the
slim `tags` column → update), NOT a dedicated ported op — the add-tag/remove-tag
verbs compose them from `find_by_id` + `update_character`. Versions: core
0.0.159, harness 0.0.144. Remaining P4.6f: the mutations (create/quick-create/
update/delete-cascade), the action verbs, the sub-resource mutations, tags CRUD
+ the delete fan-out, the heavier read actions (stats/chats), the photo gallery,
ST import/export, depiction-guidelines, and the Tier-3 refusals.

**P4.6f slice 2 — the characters action verbs (2026-07-10).** The thin
`characters/[id]/handlers/post.ts` verbs as dispatch handlers:
`character_favorite`, `character_toggle_controlled_by`,
`character_toggle_carina`, `character_set_default_partner` (partner-exists /
must-be-`controlledBy:'user'` / not-self guards — note v4 checks controlledBy
BEFORE the self-check, so a self-partner where self is llm returns the
controlledBy message), `character_avatar` (resolve + `image/*` validation; set
and clear), `character_add_tag` / `character_remove_tag` (the generic
`TaggableBaseRepository` pattern composed from `find_by_id` +
`update_character` — characters have no dedicated tag mutator). Two load-bearing
findings closed against the oracle: (1) the flip/avatar echo is v4 base
`_update`'s MERGE — `validate({...preUpdateOverlaidRead, ...patch, updatedAt:
now})`, the patch overlaid on the PRE-update read, NOT a re-read (the P4.6c D4
finding), so an explicit `defaultImageId: null` from the patch survives in the
echo where a fresh `find_by_id` would omit it; (2) `update_character` could
never NULL a nullable slim column — `slim_update_from_patch`'s `Option<String>`
fields collapse an absent key and an explicit JSON `null` to the same `None`
(= skip), but v4's `_update` NULLs a column set to `null`. Fixed additively:
`update_character` now issues a supplementary `SET <col>=NULL` for the nullable
slim columns present-as-null in the DB-bound patch (`NULLABLE_SLIM_COLUMNS`).
The fix is regression-checked: `characters_update_tier2` (whose corpus DOES
send explicit nulls) still passes. Added `tags::find_full_by_id` (the marshaled
Tag entity for the add-tag `{success, tag}` echo). Proven:
`characters_actions_equivalence` — 11 cases (seven verbs + two set-partner
guard failures + avatar set/clear) vs v4's real handlers, echo + post-op slim
`find_by_id_raw` diffed (op-minted `updatedAt` + read-time-minted
`physicalDescription` ts normalized; v4's 201/`{error:msg}` REST shapes mapped
to the dispatch envelope). Versions: core 0.0.160, harness 0.0.145. Still
remaining in P4.6f: create/quick-create/update handlers, delete-cascade, the
sub-resource mutations (prompts/scenarios/plugin-data/wardrobe), tags CRUD +
delete fan-out, the heavier read actions (stats/chats), the photo gallery, ST
import/export, depiction-guidelines, and the Tier-3 refusals.

**P4.6f slice 3 — the sub-resource mutations (2026-07-10).** The prompts /
scenarios / plugin-data mutation handlers (`api::characters::character_{prompt,
scenario,plugin_data}_*`), composed over the already-proven
`vault_character_arrays::{add,update,delete,set_default}_system_prompt` /
`{add,update,remove}_scenario` and `character_plugin_data` ops. Handler-level
concerns ported: v4's ownership `findById` + `checkOwnership`, the
prompt-exists pre-check (`notFound('Prompt')`), scenario update/delete's
null→`notFound('Scenario')`, the `{message:'Scenario removed'}` delete body.
One seam closed against the oracle: the plugin-data POST/PUT upsert echo returns
`data` as the input OBJECT, NOT the stored string — v4's `upsert` returns the
base create/update entity (`validate({...existing, ...{data:inputObject}})`),
whose `data` is the input value; the item GET's `data` is the DB-re-parsed
string (slice 1). So the upsert handler re-reads for the row metadata
(id/timestamps) then overlays the input `data`. Added
`character_plugin_data::delete_by_character_and_plugin`. The
`set-default-prompt` verb (contract-shared with the SPA) maps to
`set_default_system_prompt` — v4 has no dedicated route for it (the prompt PUT
with `{isDefault:true}` is the diffed path), so it ships implemented but not in
the differential. Proven: `characters_subresources_equivalence` — 9 cases
(prompt/scenario create/update/delete, plugin upsert existing+new, plugin
delete) vs v4's real handlers; update/delete target baked sub-items resolved by
name (stable across copies), create/upsert normalize the minted
id/createdAt/updatedAt. Versions: core 0.0.161, harness 0.0.146. Remaining in
P4.6f: create/quick-create/update, delete-cascade, wardrobe mutations, tags CRUD
+ delete fan-out, stats/chats, the photo gallery, ST import/export,
depiction-guidelines, and the Tier-3 refusals.
---

**P4.6f slice 4a — create / quick-create / update (2026-07-10).** The three
character-mutation handlers (`api::characters::{character_create,
character_quick_create, character_update}`), composed over the already-proven
`character_vault::create_character` (vault provisioning) and
`vault_character_update::update_character` (write overlay). Handler-level
concerns ported: v4 `createCharacterSchema`'s slim defaults + managed-field bag
(`controlledBy`→`'llm'`, `npc`→`false`, empty tags/partnerLinks/avatarOverrides,
`defaultImageId` null; the managed inputs deserialize straight off the body,
unknown keys ignored); quick-create's fixed `"Character created during chat
import"` description; update's `findByIdRaw`-first ownership (broken-vault
characters stay editable), the `updateCharacterSchema` key whitelist (Zod strips
unknowns) with the empty-string transforms (`defaultConnectionProfileId`/
`defaultImageProfileId` `""`→omit, `characterDocumentMountPointId` `""`→null).
Both echoes reload through the overlay (v4 create reloads via `findById`; update
returns `applyDocumentStoreOverlayOne`), so the echo diff transitively re-proves
the vault round-trip in composition — every managed field reads back from the
vault the handler just wrote; the raw storage rows stay byte-proven by the
standing `characters_create_tier2` / `vault_character_update` tier-2
differentials (a documented scoping — not re-dumped here). Oracle gotcha: v4's
`put.ts` calls `revalidatePath('/')` (`next/cache`) which throws outside a Next
request context — stubbed to a no-op via `jest.doMock(..., {virtual:true})`
(jest can't resolve `next/cache` from the /tmp mirror otherwise). v4 quirk
preserved: a managed-only update that replaces the scenarios array does NOT
clear a now-dangling `defaultScenarioId`. Proven:
`characters_mutations_equivalence` — 5 cases (create-full/minimal,
quick-create, update-managed/slim) vs v4's real POST/PUT handlers, echo-diffed
with minted `id`/`createdAt`/`updatedAt`/`characterDocumentMountPointId`
blanked. Versions: core 0.0.163, harness 0.0.148. Remaining in P4.6f:
delete-cascade, wardrobe mutations, tags CRUD + delete fan-out, stats/chats, the
photo gallery, ST import/export, depiction-guidelines, and the Tier-3 refusals.
---

**P4.6f slice 4c — wardrobe mutations (2026-07-10).** The four wardrobe
item-mutation handlers (`api::characters::character_wardrobe_{create,get,update,
delete}`), composed over the already-proven vault-public wardrobe CRUD
(`create/update/delete_vault_wardrobe_item`), `wardrobe_read::
find_by_id_for_character`, and the equipped-reference cleanup
(`ChatOutfitsRepository::remove_equipped_item_from_all_chats`). Each gated by
v4's OVERLAID `findById` ownership (a broken vault → 503, unlike the delete/
update character paths which use `findByIdRaw`); update/delete pre-check
existence via `findByIdForCharacter` (empty project scope, matching v4's no-opts
call). The echo-shape seam (proven against the oracle): v4's CREATE echo is the
constructed object — it carries `migratedFromClothingRecordId: null` (create sets
it explicitly) but OMITS `archivedAt` — whereas the UPDATE echo is the full
read-shaped item (includes `archivedAt: null`). So create serializes the
write-struct with `migrated_from_clothing_record_id: Some(None)` +
`archived_at: None`, and update re-reads through `find_by_id_for_character` for
the v4-exact Value bytes (the reads differential's proven shape) rather than
serializing the write-struct (whose `skip_serializing_if` would drop the null
fields v4 emits). The four arms replace their `not_available` refusals. Proven:
the `characters_mutations_equivalence` differential extended to 9 cases (the +4
wardrobe create/get/update/delete; item ids discovered by title on both sides
since they mint at fixture-build). Versions: core 0.0.164, harness 0.0.149.
Remaining in P4.6f: delete-cascade, tags CRUD + delete fan-out, stats/chats, the
photo gallery, ST import/export, depiction-guidelines, and the Tier-3 refusals.
---

**P4.6f slice 4d — tags CRUD + the delete fan-out (2026-07-10).** The five tags
handlers (`api::characters::{tag_list, tag_get, tag_create, tag_update,
tag_delete}`) over v4's `tags/route.ts` + `tags/[id]/route.ts`. All six taggable
tables (characters / chats / connection_profiles / image_profiles /
embedding_profiles / files) live in MAIN, so the handlers are main-only
(`db.read_main` / `db.write` with just the main writer — no mount). Added to the
tags repo: `find_all`, `find_by_name` (case-insensitive `nameLower`),
`count_tag_usage` + `remove_tag_from_table` (generic over the `TAGGABLE_TABLES`
whitelist — a tags-only patch changes only `tags` + `updatedAt`, so the raw
UPDATE reproduces v4's base-repo re-validated write byte-for-byte), and a
`visual_style` field on `TagUpdate` (nullable, Zod-default materialized via the
`TagVisualStyle` serde defaults). Handler seams: the list DTO whitelist (NO
`nameLower`/`userId`, `visualStyle ?? null` always present) vs. the detail's full
spread (`{...tag, _count, totalUsage}`); create's dedup-returns-existing; the
rename-conflict guard fires only when the name actually changes; the delete
fan-out sweeps every taggable table then deletes the tag. Name sort uses the
ported `collation::locale_compare` (ICU en-US). **Fixture extended** (deliverable
#14): tagged the connection profile / image profile / legacy file with "Adventure"
(so the delete fan-out exercises FIVE of six entity shapes with real mutations —
embedding_profiles stays a verified no-op) and materialized the empty
`embedding_profiles` table (v4's `findAll` auto-creates it via `ensureCollection`
per-case; the Rust raw SQL 404s without it). Rebuilding the fixture re-mints the
non-pinned vault ids, so ALL four characters differentials (reads / actions /
subresources / mutations) were regenerated + re-run green against the new .db.
Proven: `characters_mutations_equivalence` extended to 15 cases (+ tag list / get
/ create-new / create-dedup / update / delete); tag_delete additionally diffs all
six taggable tables + the tags table against the oracle's post-delete dump (ids
baked-identical, no remap). Regen gotcha: regenerate each oracle in its OWN clean
jest invocation — a batched multi-oracle run left a stale mount id. Versions:
core 0.0.165, harness 0.0.150. Remaining in P4.6f: delete-cascade, stats/chats,
the photo gallery, ST import/export, depiction-guidelines, and the Tier-3
refusals.
---

**P4.6f — depiction-guidelines GET/PUT (2026-07-10).** The Ariel-Clause editor
file (`api::characters::{character_depiction_guidelines,
character_depiction_guidelines_update}`). GET: overlaid `findById` ownership →
`DocMountDocumentsRepository::find_by_mount_point_and_path(mount,
"depiction-guidelines.md")` (RAW single-tier, NOT the trimmed/capped
`read_store_file_internal`; the editor shows exactly what's on disk) →
`{content}` (`''` when no vault/file; a read error falls soft to `''`, v4's
`readStoreFile` catch). PUT: RAW `findByIdRaw` ownership (broken-vault characters
stay editable) → `writeStoreFile` semantics (empty/whitespace →
`database_store::delete_database_document`, else `write_database_document`) →
`{success:true}`; no vault → BadRequest. Both arms replace `not_available`.
Proven: `characters_mutations_equivalence` extended to 18 cases (depiction
get-empty / put-write / put-clear; each PUT reads the file back through the GET
path — the differential compares the readback content on both sides). Versions:
core 0.0.166, harness 0.0.151. Remaining in P4.6f: delete-cascade, stats/chats,
the photo gallery, ST import/export, and the Tier-3 refusals.
---

**P4.6f — the `stats` read action (2026-07-10).** `api::characters::
character_stats` over v4 `[id]/handlers/get.ts:293`. Ownership (overlaid
`findById`) → the Promise.all fan-out reproduced as sequential reads (memories
`count_by_character_id`, chats `find_by_character_id`, wardrobe
`find_by_character_id`, the vault links `find_by_mount_point_id`, group
memberships `find_group_ids_by_character_id`), the links fetched once and reused
for photos/knowledge/core (v4's `isPhotosRelativePath` + the `images/avatar.webp`
/ `images/history/` special-cases; `knowledge/` + `core/` prefix counts) and the
present-paths set for the `characterFiles` N/8 health figure (per-canonical-path
so case-variant duplicates can't overcount). Groups hydrated by looping the
deduped ids through the overlay (`GroupsRepository::find_by_id` → `{id, name,
description, color, icon}`). All-ported reads; no new leaf. The arm replaces its
`not_available`. Proven: `characters_reads_equivalence` extended with `stats` (+
a `depiction_guidelines` GET case) — the fixture Aria reads memories 2 /
conversations 1 / wardrobeItems 2 / photos 1 / scenarios 2 / characterFiles 8-of-8
/ groups []. Versions: core 0.0.167, harness 0.0.152. Remaining in P4.6f:
delete-cascade, the `chats` read action, the photo gallery, ST import/export, and
the Tier-3 refusals.
---

**P4.4u3 — the built-in seeds (roleplay templates + the three mount stores):
done (2026-07-10).** Two of the three P4.4 named seed deferrals closed so a
fresh v5 instance matches a fresh v4 instance.

- **Family 1 — the `delimiters` discriminated-union marshaling** (the Phase-2
  scope dodge, `db::roleplay_templates`). Typed serde structs in Zod schema
  field order for the three kinds (`wrap` / `linePrefix` / `tagPrefix`): an
  internally-tagged (`#[serde(tag="kind")]`) enum with the shared
  `name`/`buttonName`/`style`/`hideDelimiter?`/`addOns?` before the
  kind-specific tail, plus the `StringOrPair` untagged union shared by `wrap`'s
  `delimiters` and the row-level `narrationDelimiters` (bare string → plain
  TEXT; pair → JSON array text, mirroring `documentToRow`'s array-value branch
  on a schema-`'unknown'` column). `addOns` carries field-level serde defaults
  matching Zod's, so a partial input materializes identically. The read-side
  `kind:'wrap'` backfill (`TemplateDelimiterSchema`'s `z.preprocess`) lives in
  `parse_delimiters`, exercised by the update path: v4 `_update` reads →
  validates → `$set`s EVERY column, so a legacy kind-less delimiter is upgraded
  to `kind:'wrap'` on the next update; the port always rewrites the `delimiters`
  column (from the patch or the re-parsed existing value) to reproduce that,
  byte-for-byte, while leaving the identity columns partial. The tier-2 corpus
  now exercises every kind (+ addOns present/absent, hideDelimiter, tokenPattern
  present/absent, narration string/pair) and a `rawDelimiters` post-seed
  kind-less row round-tripped through update.
- **Family 1 seeder** (`services::builtin_templates`): v4's every-startup
  `seedBuiltInTemplates` — find-by-`(name, isBuiltIn)`, INSERT (minted id) when
  absent, drift-UPDATE six fields + `updatedAt` when present. THE QUIRK v4 has
  and the port reproduces: the INSERT path (`this.validate` → Zod parse) stores
  delimiters in SCHEMA order, but the drift-UPDATE path (`collection.updateOne`
  raw `$set`) stores them in the SEED-LITERAL order of the `BUILT_IN_TEMPLATES`
  literal. The seed data module (`builtin_templates.json`) is generated verbatim
  from v4's real seeder (`dump-builtin-templates.ts`, double-seeded to capture
  the seed-literal order); the INSERT path re-parses it through the typed union
  (→ schema order), the UPDATE path serializes it raw (→ seed-literal order).
- **Family 2 — the three mount stores** (`services::builtin_mounts`): v4's
  `provision-{lantern-backgrounds,user-uploads,general}-mount` migrations as one
  idempotent provision-or-adopt unit, keyed by the `instance_settings` POINTER
  (not by name): empty/dangling pointer → mint fresh (verbatim `doc_mount_points`
  INSERT + `ON CONFLICT` pointer upsert), live pointer → adopt; always ensure the
  subfolders via the ported `ensure_folder_path`. `ensure_mount_index_tables`
  ports `ensureMountIndexTables` (CREATE IF NOT EXISTS, a no-op on the generateDDL
  schema) and the `instance_settings`-existence guard ports the migration
  `shouldRun`. Plus `ensure_general_scenarios_folder` (the runtime re-ensure) and
  the three `instance_settings` pointer setters.
- **Wiring**: `provision_fresh_instance` runs both families (main + mount-index
  writers held open together for the cross-partition mount step); the host
  assembler runs both on EVERY assemble/unlock via a spawned-and-joined OS thread
  (so `write_blocking` is legal from the sync boot path and the async `Unlock`
  dispatch alike). Idempotent: a pre-existing instance adopts + drift-updates,
  never duplicates.
- **Differentials**: `builtin_templates_equivalence` (drives v4's REAL
  `seedBuiltInTemplates` over fresh / stale-builtin / user-same-name states),
  `builtin_mounts_equivalence` (drives the REAL migration `run()` over empty /
  dangling / live states, shared-id-map remap), `provisioning_equivalence`
  extended (a fresh v5 instance's roleplay_templates / doc_mount_points /
  doc_mount_folders / instance_settings diffed against a
  fresh-v4-with-migrations+seed instance — build-provision-oracle.ts now runs
  seedBuiltInTemplates + the migration `run()`s, mount-index path aligned so the
  manager and migrations share the file), the tier-2 corpus regen, a host
  adopt/seed test, and the web setup e2e asserting 2 templates + 3 mounts + 3
  pointers post-setup. All green.
- **Deferred (unchanged)**: the `lorian-and-riya.qtap` sample-content import
  (needs the ~2,500-line quilltap-import service; v4 gates it on zero-characters
  and swallows failures — a fresh v5 instance is fully functional without it).
- **Gotcha banked**: v4's mount migrations open the mount-index via
  `getMountIndexDatabasePath()` (`<dataDir>/data/quilltap-mount-index.db`), which
  IGNORES `SQLITE_MOUNT_INDEX_PATH` — the manager honors the env var. Any oracle
  running the migrations in-process must place the mount-index under `data/` so
  both agree, or they write/read different files. Oracle scripts that import
  `better-sqlite3` at top level must run from a mirror INSIDE the v4 checkout
  (node_modules resolves by walking up from the script) — the `/tmp` mirror the
  `@/`-only cases use fails on the bare import.
- **Versions**: core 0.0.159, host 0.0.10, harness 0.0.144, web 0.0.7.
**P4.6g — the Characters SPA vertical (lane B), 2026-07-10.** The
`apps/web` characters screens over the pinned p4.6f Shared contract
(coded against the TS mirror + `CoreClient.dispatchData`; MOCKED responses
in the component tests — the live e2e beat lands at unification over lane
A's fixture, the P4.6b precedent). `apps/web` 0.4.0 → 0.5.0.

- **Foundation.** The core-contract TS mirror gained every character/tag
  `Request` variant (serde names transcribed verbatim from the p4.6f
  Shared contract) + the list / detail / stats / tags / cascade-preview /
  physical-description / pronouns DTOs. A pure `processTemplate` port
  substitutes `{{char}}`/`{{user}}`. `characters.api.ts` holds the query
  keys + `dispatchData` read helpers + `characterAvatarSrc` (the
  `?v={defaultImageId}` cache-bust). Routes (`/characters`, `/new`,
  `/:id`, `/:id/edit`) + the shell Characters nav went live.
- **LIST** (`AuroraView.tsx`): cards over `characterList` with the v4 sort
  (NPCs last → favorites first → chat count desc → name A–Z), the three
  optimistic inline toggles, Chat/Export/Delete, the cascade delete dialog
  over `cascade-preview`, and the ST import dialog (JSON via dispatch, PNG
  via the multipart web route). Groups grid / Summon-From-Lore /
  Reset-Builtins deferred (omitted / disabled).
- **DETAIL** (`[id]/view/**`): the nine-tab hall — header (stat line +
  optimistic toggles + Convert-to-NPC), Details (highlighted read + the
  template replace/reverse fan-out: `characterUpdate` scalars/scenarios/
  physicalDescription + per-prompt `characterPromptUpdate`, ported from
  v4 `TemplateHighlighter` + `apply-character-field-updates`), System
  Prompts (read), Tags CRUD, the Default Settings autosave tab (per-field
  `characterUpdate`/`characterSetDefaultPartner` with the exact v4 payload
  shapes, test-pinned), Photo Gallery (grid + remove), Appearance (phys
  desc read + depiction-guidelines editor), and the deferred Wardrobe /
  Conversations / Memories bodies.
- **CREATE** (`new/NewCharacterView.tsx`): the plain form → `characterCreate`;
  the four vantage points DISTINCT with v4's helper copy verbatim; singular
  scenario. **EDIT** (`[id]/edit/**`): explicit-save (ONE `characterUpdate`
  bag + `window.confirm` dirty guard), the scenarios array editor, the tag
  chip editor, the System-Prompts CRUD modals, the Appearance tab (separate
  `physicalDescription` + depiction saves), the avatar picker (`characterAvatar`).
- **Built parallel** in two isolated worktrees (VIEW ∥ EDIT+CREATE) over
  the committed foundation+list, then integrated by disjoint-subtree
  checkout. **Gate:** `tsc` clean (app + spec), `ng test --no-watch`
  **182 green**, `ng build` succeeds. The Playwright `characters-flow.spec.ts`
  skeleton is written + skipped (un-skip at unification).
- **Deferrals** (disabled affordances / omitted, no stubbed logic): the
  wardrobe dialog, the AI import wizard (Summon-From-Lore), the inline AI
  Wizard, the optimizer, Rename/Replace, reset-builtins, external prompt,
  refresh-archive, the Groups grid, Memories/Conversations tab bodies,
  prompt-template import, the image-generation-profile picker (no P4.6d
  contract), photo upload, Lexical-equivalent markdown editing (plain
  textareas this round). **Simplifications flagged:** the edit dirty guard
  is a plain `window.confirm` (not v4's three-way alert); the timestamp
  card is mode/format/interval only; `systemTransparency`/`coreWhisperEnabled`
  ride `CharacterDetail`'s catch-all (unverified vs the oracle until P4.6f
  lands). **Standing caveat:** the DTO bytes are proven only when lane A's
  `characters_*_equivalence` differentials land; this lane is component-tested
  against the pinned contract, not the oracle.
**P4.6h — Salon virtualization (dogfood finding #3b): done (2026-07-10).** A
port of v4's OWN virtualization (`app/salon/[id]/components/VirtualizedMessageList.tsx`
+ `hooks/useAutoScroll.ts`, HEAD `a7b1398d`), not a perf project. Adopted
`@tanstack/angular-virtual` 5.0.7 (pinned; the official Angular adapter over
the exact `virtual-core` v4 uses — peer `@angular/core >=19` satisfied by 21)
in `chat/message-list.ts`, windowing the existing `chat-view-model`
render-item array (estimate 150, overscan 5, stable render-item keys, dynamic
measurement, total-size spacer + translated absolute rows). Row heights carry
a `padding-bottom:1rem` (the inter-row gap the old `space-y-4` gave, now
inside each measured row since absolute rows drop the list's `space-y`).
Measurement rides a `VirtualRow` directive = v4's `ref={virtualizer.measureElement}`
(measure in `afterNextRender`, prune with `measureElement(null)` on destroy).
Markdown is memoized in a new `render/render-cache.ts` keyed by
`(content, renderingPatterns, dialogueDetection)` (v4's `LazyMessageContent`
memo), so a windowed re-mount is a Map hit — the second half of the fix
(windowing bounds HOW MANY rows render; the cache bounds HOW OFTEN a row's
markdown recomputes). `chat/auto-scroll.ts` (`AutoScrollController`) ports
`useAutoScroll` verbatim — `SCROLL_THRESHOLD=100`, `SETTLE_DELAY_MS=400`,
`SCROLL_CHECK_DEBOUNCE_MS=100`; initial settle + one-time instant
multi-strategy scroll-to-bottom (never smooth with dynamic sizes),
stick-to-bottom tracking, completion-gated auto-scroll (reads
`autoScrollOnResponseComplete`, default false), scroll-on-user-send (wired via
a `viewChild(MessageList)` in `salon-conversation.ts`), the jump-to-bottom
button (`showScrollToBottom = isSettled && !isAtBottom`) — with a unit test
over a fake scroll element (settle gate, 100px threshold, suppress/re-enable,
completion gating). GOTCHA banked: the `@angular/build:unit-test` jsdom
harness does NOT run `afterRender`/`afterRenderEffect` hooks, so the adapter's
own `_willUpdate` (which computes the visible `range`) never fires there —
message-list additionally drives `_willUpdate()` from a plain `effect()`
(guarded no-op in the browser, where afterRenderEffect works and the e2e
proves it). `calculateRange` also forces an empty window when the scroll
container's `offsetHeight===0` (jsdom's default), so the component spec stubs
`HTMLElement.prototype.offsetHeight`. A SEPARATE committed long-chat fixture
(`crates/quilltap-web/tests/fixtures/salon-long-*.db`, ~300 mixed messages —
markdown-heavy, whispers, packed staff-announcement runs — via a NEW
`harness/oracle/fixtures/build-long-chat-fixture.ts` through v4's real
`repos.chats.addMessages`; the salon fixture pair is FROZEN, never touched)
backs a new `e2e/salon-scroll.spec.ts` on its OWN locked server (the shared
global-setup server stays pinned to the small salon fixture): open → interactive
< 3s → landed at bottom → windowed DOM (< 60 rows vs 300) → scroll up → jump
button → click → back at bottom → composer present. E2e recipe note: each
`quilltap db --write` unwraps the .dbkey via PBKDF2 (~5s), so the per-hook
timeout is raised and only the tables this read walk touches (`chats`,
`characters`, `chat_settings`) are rewritten; the fixture must materialize the
empty tables the read path reads (`memories`, `files`, `tags`,
`conversation_chunks`, `vector_indices`, `background_jobs`) or they surface as
`no such table` at runtime. All 22 SPA unit files (151 tests) + all 6
Playwright specs + the prod build green. No server/Rust change; the
client-side-markdown locked divergence stands. SPA 0.4.0.

---

**The P4.6f ∥ P4.6g ∥ P4.6h ∥ P4.4u3 unification (2026-07-10).** The four lane
branches cherry-picked onto `unify-p46fgh-44u3` from main — the ninth
consecutive round with zero source-level conflicts (only version files +
append-only docs; versions resolved core 0.0.162 / harness 0.0.147 / SPA
0.5.1, host 0.0.10 + web 0.0.7 from lane D). The two P4.6g child worktrees
(detail/view ∥ edit/create) were verified subsumed by the lane's consolidation
commit before removal.

- **The Shared contract wire:** all 48 characters/tags `Request` variants
  match name-for-name between `api/types.rs` and the SPA mirror
  (`core-contract.ts`) — verified mechanically (variant-name extraction diff).
- **The e2e wire:** `characters-flow.spec.ts` un-skipped on a spec-private
  server (port 4322, the salon-scroll recipe) over the committed
  `characters-*` fixture. THREE fixes emerged:
  (1) **accname gotcha** — the favorite star's accessible NAME is its text
  content (`☆`), not its `title` attr (content outranks title in accname
  computation); locate icon-glyph buttons `getByTitle`.
  (2) **the scroll-strategy drain window** — the salon-scroll walk set
  `scrollTop = 0` within 300ms of landing, and the controller's pending final
  correction (v4-faithful, +300ms) yanked it back to the bottom; the spec now
  waits 450ms post-landing.
  (3) **programmatic scrolls fire no scroll events in a frame-throttled
  renderer** (scroll events dispatch during the rendering steps, and an
  occluded page produces no frames — rAF never fires); the spec scrolls up
  with REAL `page.mouse.wheel` input, which is also the behavior the stick
  tracker actually guards.
- **P4.6f scope (IMPORTANT):** the lane landed slices 1–3 only (reads / action
  verbs / sub-resource mutations, each differential-proven vs fresh v4
  oracles). The banked remainder — create/quick-create/update, delete-cascade,
  wardrobe mutations, tags CRUD + the delete fan-out, stats/chats, the photo
  gallery, ST import/export, depiction-guidelines, and re-pointing the Tier-3
  refusal list — is **slice 4, OPEN under the same order**
  (`work-orders/p4.6f-characters-server.md`). Until it lands, the SPA's
  edit-save / create page / Default-Settings autosave / add-tag surfaces
  answer the loud `not_available` refusal (they render and fail gracefully);
  the e2e's edit-title→Save and add-tag beats are annotated for restoration.
- **Gate:** fmt + clippy (default AND native-transport) clean; the 847-test
  workspace sweep green; the six new/extended differentials re-verified
  against FRESH oracles regenerated from v4 at `a7b1398d` (characters reads
  13 / actions 11 / subresources 9 cases; builtin-templates 3 states;
  builtin-mounts 3 states; provisioning incl. v5-reads-v4 AND v4-reads-v5 —
  the latter needs `QT_FIXTURE_V5_PROVISIONED`, not the header's OUT var
  name); 194 SPA unit tests; the SPA prod build; the full 8-spec Playwright
  suite (7 tests) green including the two new walks.

**P4.6f slice-4 unification (2026-07-11).** The five lane commits (slice 4a
create/quick-create/update; 4c wardrobe mutations; 4d tags CRUD + the
six-table delete fan-out; depiction-guidelines GET/PUT; stats) cherry-picked
from `claude/characters-server-p4-6f-396f76` onto main — only the CHANGELOG
conflicted (both-sides union). The unification wire: the `characters-flow`
e2e's two annotated beats RESTORED — add-tag via the Tags tab's
Enter-to-create path (`tagCreate` + `characterAddTag`, proven across a
reload) and edit-title→Save (`characterUpdate`, proven on the roster card
after a full reload). Two spec findings: the "Edit Character" link renders
on the detail view's DETAILS tab, not the header (the walk must switch back
off the Tags tab first — the first failure screenshot showed a healthy page
with no such link), and the now-three-reload walk needs a 60s test budget.
Gate: fmt + release build clean; clippy default AND native-transport clean;
1,207 workspace tests green; the five characters/tags differentials
re-verified by name against FRESH v4 oracles at `a7b1398d` (mutations 18 /
reads 15 / actions 11 / sub-resources 9 / tags tier-2 — the tags fixture
builder must run FROM the v4 checkout, it imports v4 lib); 194 SPA unit
tests; the SPA prod build; the full Playwright suite 7/7. Versions: core
0.0.167, harness 0.0.152, SPA 0.5.2. **The P4.6f order's remaining OPEN
items:** delete-cascade + cascade-preview, the per-character `chats` read,
the photo gallery (photo-list/save/remove), ST import/export — plus the
tier-3 refusal deferrals (ai-wizard, optimizer, rename, ai-import,
reset-builtins).

**Dogfood finding #4 — whole-card click on the characters roster
(2026-07-11).** v4's `AuroraView` card navigates from a click ANYWHERE on the
card (`cursor-pointer` + `handleCardClick` with a
`closest('button')`/`closest('a')` guard for the inner toggles/actions); the
P4.6g port had narrowed the click target to the avatar+name `<a>` only, so a
click on the description/body did nothing — and the e2e missed it because it
clicked the name link directly. Fix: `character-card.ts` gets the v4-faithful
card-level `(click)` with the same guard (the inner `<a>` stays for
middle-click); `characters-list.spec.ts` adds the navigate-from-body /
no-navigate-from-star unit test; the `characters-flow` e2e's detail-open beat
now clicks the card BODY (`p.line-clamp-3`). Gate: 195 SPA unit tests, prod
build, the characters e2e green. SPA 0.5.3.

---

## P4.6i — Characters server remainder (lane A, in progress)

Closes the OPEN slice-5 remainder of `p4.6f-characters-server.md`. v4 baseline
`a7b1398d` (drift-checked clean at lane start). Own branch
`claude/p4-6i-characters-remainder-5cf414`.

**Unit 1 — `deleteMemoriesWithUnlinkBatch`: ALREADY LANDED (pre-existing).**
The order's 2026-07-11 survey said "NOT ported", but the fresh v5 survey found
it done: `MemoriesRepository::delete_many_with_unlink` (`db/memories.rs:529`,
Phase-3 memory family) with a passing tier-2 differential
(`memory_delete_tier2_equivalence.rs` + `harness/oracle/cases/memory-delete-
tier2.ts`) that drives v4's REAL `deleteMemoriesWithUnlinkBatch` /
`deleteMemoryWithUnlink` (single-item + batch, missing-row noop, empty-batch,
by-character grouping). No duplicate `memory-gate-unlink-*.ts` authored (the
"extend, don't recreate" rule). Cascade-delete (unit 3) consumes it.

**Unit 4 — `?action=chats` enriched recent-chats DTO (DONE).**
`api::characters::character_chats` (`api/characters.rs`) ports the `chats`
action of `characters/[id]/handlers/get.ts` (105-229): overlaid ownership →
`chats_read::find_by_character_id` → filter to the caller's `userId` → per-chat
`get_messages` + `lastMessageAt` (max `type==='message'` createdAt round-tripped
through `clock::iso_to_ms`/`iso_from_unix_ms` = JS `new Date(...).toISOString()`,
else `chat.updatedAt`) → **stable** desc sort → optional lowercased search over
title + message content → `slice(offset, offset+limit)` (defaults 10/0) → enrich
(project map via `ProjectsRepository::find_by_id`; tags `{tag:{id,name}}` via
`tags::find_by_ids`; `_count.messages` = non-SYSTEM/TOOL `type==='message'`;
`_count.memories` via `memories_read::count_by_chat_id`; scriptorium status from
`renderedMarkdown` + `ConversationChunksRepository::count_stats_by_chat_id`; 3
recent messages; story background via `FilesRepository::find_by_id` →
`/api/v1/files/{id}`; `isDangerousChat`). Wired the `CharacterChats` dispatch
arm. NO fixture change needed (the committed "Solo Voyage" chat + 2 memories
suffice; new read cases append to the existing oracle). No new `Response`
variant — `Response::Character` already covers `{chats,total}`.
Differential: extended `characters_reads_equivalence` with six cases
(`chats_plain` / `chats_search_title` / `chats_search_content` /
`chats_search_miss` / `chats_limit0` / `chats_offset1`) — all green against a
FRESH v4 oracle at `a7b1398d`. Regen: the characters-reads header recipe
(`QT_ORACLE_OUT=/tmp/oracle-characters-reads.ndjson`,
`QT_ORACLE_CHARACTERS_READS` for the Rust side). Versions: core 0.0.168,
harness 0.0.153.

**Unit 5 — ST import/export JSON legs (DONE).**
`services::sillytavern::export_st_character` / `import_st_character` port the
JSON paths of `lib/sillytavern/character.ts`. Export (`character_export`,
`CharacterExport` format=json) = overlaid character → `chara_card_v2` card
(systemPromptContent from the default/first prompt; scenarioContent 1→content,
many→`## title\ncontent` joined; `sillyTavernData` base OR the default card;
`title || undefined`, `mes_example = exampleDialogues || ''`). Reads only stable
fields → no minted id in the output. Import (`character_import`,
`CharacterImport` JSON body) = `body.characterData || body` → `importSTCharacter`
(`.data` unwrap; `mes_example` array→JSON.stringify; `system_prompt`→one Default
prompt; `scenario`→one Default scenario; `sillyTavernData = data`) → create
DIRECTLY through the create primitive (NOT `character_create` — which hardcodes
`silly_tavern_data: None`; import writes it to the slim column) → echo
`{character:{id,name,description,defaultImageId:null,createdAt,updatedAt,_count:{chats}}}`.
Wired the `CharacterExport` / `CharacterImport` dispatch arms. PNG legs deferred
(loud `export-png` / handled at the web multipart route). Differentials:
`characters_reads_equivalence` +`export_json`; `characters_mutations_equivalence`
+`st_import_card` (diffs the create echo AND the overlay readback of the created
character — proving the ST scenarios/systemPrompts/firstMessage/exampleDialogues/
sillyTavernData round-tripped; minted ids/ts blanked by the existing `norm`).
The export handler returns a raw `JSON.stringify(card)` download body, so the
reads oracle's `response.json()` yields the JSON TEXT as a string — the case
carries a `parseStringBody` flag to parse it back for the semantic diff. Both
green vs FRESH v4 oracles at `a7b1398d`. Versions: core 0.0.169, harness 0.0.154.

**Unit 2 (part 1) — photo gallery LIST + REMOVE (DONE).**
`photos::character_gallery_service::{list_character_gallery,
remove_from_character_gallery}` port the read + delete JSON legs of
`lib/photos/character-gallery-service.ts`; the shared
`photos::photo_link_summary::get_photo_link_summary_by_sha256` ports
`getPhotoLinkSummaryBySha256` (api::salon carries a byte-identical private copy
predating this — unify later). List: overlaid ownership → vault resolution
(`characterDocumentMountPointId` → mount point must be database+character) →
`findByMountPointId` → keep `isPhotosRelativePath` ∪ `images/avatar.webp` ∪
`images/history/*` → createdAt-desc → clamp(1,200)/offset paginate → entries
(linkId/mountPointId/relativePath/fileName/blobUrl/mimeType/sha256/
fileSizeBytes/keptAt/caption[frontmatter ?? description]/tags/linkSummary);
absent/broken vault → `{entries:[],total:0,hasMore:false}`. Remove (write): null
`defaultImageId` / filter `avatarOverrides` when they point at the link, then
`deleteWithGC` (extended to RETURN `fileGC`). Wired the `CharacterPhotoList` /
`CharacterPhotoRemove` dispatch arms. Two small shared-code deltas: `LinkRow`
gained a `description` field (appended to the join SELECT at index 18 — additive,
existing consumers unaffected) for the caption fallback; `delete_with_gc` now
returns the `fileGC` bool (both prior callers ignore it via `?`). Differentials:
`characters_reads_equivalence` +`photo_list`; `characters_mutations_equivalence`
+`photo_remove_avatar` (+ its GC-table dump: links/files/blobs + defaultImageId,
baked ids → no remap). **KEY RECIPE:** the reads AND mutations oracles now
`jest.doMock('@/lib/file-storage/character-vault-bridge', requireActual)` — the
jest.setup global mocks it to a fake `mock-vault-mount`, which made the gallery
spuriously empty; un-mocking resolves the real vault. Both green at `a7b1398d`.
Versions: core 0.0.170, harness 0.0.155. **Save-by-id (photo-save) stays a loud
`not_available` refusal until part 2.**

**Unit 2 (part 2) — photo gallery SAVE-by-id (DONE, linkId leg LIVE).**
`photos::character_gallery_service::{save_to_character_gallery,
save_link_to_character_gallery}` port the write core + the `linkId` save-by-id
leg. save-to: overlaid ownership → vault → empty/mime guards → sha256 →
re-upload dedup (via `get_photo_link_summary_by_sha256`) → kept-image markdown
(character attribution) → slug/filename (prefer uploader ext via `sanitizeLeafName`
else the timestamped slug) → `resolve_unique_relative_path` → `ensure_folder_path`
→ `link_blob_content` → `chunk_and_insert_extracted_text`; `kept_at` INJECTED
(the ISO clock) so the mint is testable. save-link: source `findByIdWithContent`
→ mime guard → mount-blob bytes → save-to. `chunk_and_insert_extracted_text` /
`resolve_unique_relative_path` reused from `save_image_to_album` (made
`pub(crate)`). Wired `CharacterPhotoSaveById` — `linkId` LIVE; **the `fileId`
leg is a loud `not_available("photo-save-fileid")` DEFERRAL** (it reads bytes via
the host file store `fileStorageManager.downloadFile`, which the characters
dispatch doesn't wire — the same host-bytes seam `keep_image` carries; port a
`FileBytesStore` into the characters dispatch to close it). The multipart upload
leg stays the web route. Differential: `characters_mutations_equivalence`
+`photo_save_link` — **RECIPE:** freeze `global.Date` to a `FIXED_KEPT_AT`
(`2026-04-01T12:00:00.000Z`) in the oracle (the photo-tools pattern) and inject
the same `kept_at` on the Rust side (call `save_link_to_character_gallery`
directly via `db.write`, bypassing the dispatch's `now_iso` mint); then the
return value (linkId blanked — the only mint) AND the written `photos/` link row
(relativePath / fileId / originalMimeType / extractedText markdown / description,
raw-column dump) diff byte-exact. Green at `a7b1398d`. Versions: core 0.0.171,
harness 0.0.156. **Unit 2 (gallery list/save/remove) COMPLETE** except the
enumerated fileId-host-bytes + multipart deferrals.

**Unit 3 — cascade delete + preview (DONE). CLOSES the P4.6f server remainder.**
`services::cascade_delete` ports `lib/cascade-delete.ts`:
`find_exclusive_chats_for_character` (the only AI-controlled CHARACTER
participant is this one; user-controlled participants ignored),
`find_exclusive_images_for_character` (defaultImageId + avatarOverrides →
resolve_character_avatar → vault-link exclusive-by-construction, else the legacy
`linkedTo` + not-used-elsewhere check), `find_exclusive_images_for_chats`
(message attachments → files → not-linked-elsewhere / not-used-by-a-character),
`get_cascade_delete_preview`, `execute_cascade_delete` (delete exclusive chats +
their images, character images [vault-link via `remove_from_character_gallery`,
legacy via `delete_file_completely`], memories via `delete_many_with_unlink`,
vector index via `delete_by_character_id`, plugin data, the slim row). All over
the RAW character (`find_by_id_raw` — broken-vault-safe). Wired
`CharacterCascadePreview` (read, overlaid-ownership gate then the raw preview) +
`CharacterDelete` (`findByIdRaw` ownership → executor → `{success, deletedChats,
deletedImages, deletedMemories}`). **The last two of the eight characters
`not_available` refusals are now LIVE.** Two seams flagged: the legacy-`files`
exclusive-image branch + `find_exclusive_images_for_chats` are ported faithfully
but NOT corpus-exercised (Aria's avatar is a vault-link, her exclusive chat has
no attachments); `delete_file_completely`'s host byte reclaim
(`fileStorageManager.deleteFile`) is a host seam (the core deletes the `files`
metadata row). `characters.delete` removes only the slim row (the vault mount
stays, minus the removed avatar link — matches v4). Differentials:
`characters_reads_equivalence` +`cascade_preview`;
`characters_mutations_equivalence` +`character_delete_cascade` (body:
deletedChats=1/deletedImages=1/deletedMemories=2; + a full multi-table dump —
characters/chats/messages/memories/pluginData [MAIN] + links/files/blobs
[MOUNT], baked ids → no remap). Green at `a7b1398d`. Versions: core 0.0.172,
harness 0.0.157.

**P4.6i LANE COMPLETE.** All eight characters `not_available` arms from the P4.6f
remainder are LIVE (delete / cascade-preview / chats / export[json] / import[json]
/ photo-list / photo-save[linkId] / photo-remove). Remaining loud deferrals
(reported, not silent): ST PNG export/import (`export-png` + multipart web route),
the photo multipart upload leg, the `fileId` photo-save leg (`photo-save-fileid`,
host bytes seam), plus the pre-existing P4.6f tier-3 LLM refusals (ai-wizard /
optimizer / rename / ai-import / reset-builtins / refresh-archive). Unit 1
(`deleteMemoriesWithUnlinkBatch`) was found ALREADY ported (order survey stale) —
covered by the existing `memory_delete_tier2` differential; no duplicate authored.
Oracle regen recipes: the characters-reads / characters-mutations `.test.ts`
headers (both now un-mock `character-vault-bridge`; the mutations file freezes
`global.Date` to `FIXED_KEPT_AT` for photo-save).
**P4.6j unit 1 — the Conversations tab (SPA, 2026-07-11).** Lane B of the
characters-remainder round (worktree `claude/p4-6j-characters-remainder-spa`).
Replaced `view/tabs/conversations-tab.ts` (a 23-line empty-state placeholder)
with the real per-character chat list, ported from v4
`components/character/character-conversations-tab.tsx` + `components/chat/
ChatCard.tsx` fed by `lib/chat-utils.ts transformCharacterChatToCardData`
(`showAvatars=false`, `showProject`, `showPreview`, `useRelativeDates`). Over
`characterChats {characterId, search?, limit?, offset?}` via
`injectInfiniteQuery` (v4 `CHATS_PER_PAGE=10`; `getNextPageParam` mirrors v4's
`hasMore = page.length === 10`). Debounced (300ms) search box; an
IntersectionObserver sentinel plus a testable "Load more" button. New
`character-conversation-card.ts` (display-only, links to `/salon/:id`) carries
the message/memory badges, a STATIC scriptorium badge (v4 colours + the
descriptive half of the title — no click-to-render in this contract), the
dangerous `*`, the relative date (ported `formatChatListDate`), the preview
(ported `getCharacterChatPreview` verbatim, incl. its oldest-of-recent-three
quirk), and project + tags. Contract: added `CharacterChatMessagePreview` /
`CharacterChatSummary` / `CharacterChatsResult` to `core-contract.ts` (byte
list confirmed against v4 `app/api/v1/characters/[id]/handlers/get.ts` `chats`
action); `fetchCharacterChats` + `characterKeys.chats` in `characters.api.ts`.
Wired into `character-detail.ts` (`[characterId]`, `[characterName]`).
**Divergence flagged:** the story-background thumbnail renders when present —
v4's `ChatCard` gates it behind `showAvatars`, false in this tab; the work
order enumerates it as a card field. The v4 per-card delete/re-extract/
re-render, "Refresh Conversation Archive", and "New Chat" actions hit routes
outside this vertical's contract and are omitted (a follow-up vertical). Gate:
6 new unit tests (empty state / card render / Salon link / dangerous marker /
pagination append / debounced search), 201 SPA unit tests, the SPA prod build
clean. Component-tested against MOCK `CoreClient`; the LIVE e2e beats land at
unification over lane A's fixture. SPA 0.5.4.

**P4.6j unit 2 — the delete + cascade-preview entry point (SPA, 2026-07-11).**
Verified `list/character-delete-dialog.ts` against the finalized `CascadePreview`
DTO: it is byte-faithful to v4 `components/character-delete-dialog.tsx` (renders
title + messageCount per exclusive chat, the total exclusive-image count, and the
memory count; the two cascade checkboxes drive `cascadeChats`/`cascadeImages`,
both defaulting true) — no change needed. v4 renders neither per-chat
`lastMessageAt` nor the three separate image counts, so the SPA doesn't either.
Added a "Delete Character" affordance to `edit/character-edit.ts` (danger zone,
beside the deferred Rename/Replace): opens the dialog, dispatches
`characterDelete`, invalidates the roster list query, and navigates to
`/characters`. **Divergence flagged:** v4's `CharacterDeleteDialog` is used ONLY
by the roster `app/aurora/AuroraView.tsx` — there is no detail/edit delete in
v4; this entry point is an additive SPA affordance the order requests (the
navigate-away is required since the character's pages cease to exist). The
roster-level delete wiring (P4.6g `characters-list.ts`) is unchanged. Gate: 1
new unit test (open dialog → preview → confirm → dispatch → navigate to roster),
202 SPA unit tests, the SPA prod build clean. SPA 0.5.5.

**P4.6j unit 3 — the photo gallery verify (SPA, 2026-07-11).** Verified
`view/tabs/gallery-tab.ts` against the finalized `CharacterGalleryEntry`. Fresh
v4 survey (`components/images/embedded-gallery/`): the list endpoint returns
`{ entries }`, each entry `{ id, filename, filepath, url?, mimeType, size,
width?, height?, createdAt, caption: string|null, tags: string[] }` where `id`
is the vault `doc_mount_file_links.id` (the linkId) and delete uses that `id`.
The P4.6g `fetchCharacterPhotos` read only `photos`/`images`, so it would miss
the real `entries` envelope — fixed to read `entries` first with the legacy keys
as fallback (reconciled with lane A's pinned bytes at unification). Added
`caption`/`tags` to the `CharacterPhoto` contract type + a `CharacterGalleryEntry`
alias. The tile now renders the caption (img `alt`/`title` + a bottom overlay);
remove keeps `linkId ?? id`, so an `id`-only entry deletes by id-as-linkId.
Upload stays the deferred multipart web route (disabled control + inline note).
Gate: 2 new unit tests (the `{ entries }` envelope + caption render; remove-by-id
when linkId absent), 204 SPA unit tests, the SPA prod build clean. SPA 0.5.6.

**P4.6j unit 4 — ST import verify + Export (JSON) + live e2e beats (SPA,
2026-07-11).** Verified `list/character-import-dialog.ts`: the JSON leg reads the
file client-side and dispatches `characterImport {payload}` (the PNG tEXt-chunk
leg rides the deferred `POST /characters?action=import` multipart web route);
`imported` fires so the roster refetches — covered by a new spec (parse →
dispatch → refresh; malformed-JSON error microcopy). Added the Export (JSON)
action: v4's `?action=export&format=json` returns the ST card with a
`Content-Disposition: <name>.json`; over dispatch the SPA now dispatches
`characterExport {format:'json'}` (`fetchCharacterExport`, unwrapping a
`card`/`character` wrapper defensively) and downloads the returned card via a
Blob (`triggerJsonDownload`), replacing the roster's prior `window.open` route
hit. PNG export stays the deferred binary web route. Added the three live
`characters-flow` e2e beats — Conversations tab → chat card → `/salon/:id`;
create-then-delete a throwaway via the edit-view cascade dialog → gone from the
roster; Photo Gallery lists → remove — as `test.fixme` (activated AT unification
over lane A's fixture, per the P4.6b/g precedent). Gate: 3 new unit tests (export
dispatch+download; import parse+refresh; import error), 207 SPA unit tests, the
SPA prod build clean, `playwright --list` compiles the 4-beat spec. SPA 0.5.7.
**Fixture ask for lane A (Shared-contract channel):** the fixme delete beat is
self-contained (creates its own throwaway), but the Conversations + Gallery beats
need the fixture's lead character (Aria) to carry ≥1 exclusive chat and ≥1
gallery photo.

---

## The P4.6i ∥ P4.6j characters-remainder round — UNIFIED on main (2026-07-11)

Both lanes cherry-picked onto `unify/p4.6ij` (lane A's five server commits,
then lane B's four SPA commits; only CHANGELOG/status-log union conflicts).
v4 baseline re-verified at `a7b1398d` before unification.

**The unification wires (one commit):**

- **The gallery contract reconciled.** Lane B's unit-3 survey had coded the
  SPA against v4's embedded-gallery web-route shape (`{id, filename,
  filepath, url?}`) with defensive fallbacks; lane A's pinned dispatch
  envelope is `{entries, total, hasMore}` with entries `{linkId,
  mountPointId, relativePath, fileName, blobUrl, mimeType, sha256,
  fileSizeBytes, keptAt, caption, tags, linkSummary}`. The SPA
  `CharacterPhoto` type now IS that entry; `fetchCharacterPhotos` reads
  `entries` only; the gallery tab tracks/removes by `linkId` and renders
  from `blobUrl`. The **avatar picker** was a latent third consumer of the
  wrong shape (never live pre-round): it now selects the `linkId` — which is
  what `characterAvatar {imageId}` stores for vault photos (cascade/remove
  null `defaultImageId` against the link id) — and renders from `blobUrl`.
- **The three live e2e beats activated** (`test.fixme` dropped). Lane B's
  fixture ask was satisfiable without a rebake: Aria's vault avatar
  (`images/avatar.webp`) is a gallery entry per the list rules, and "Solo
  Voyage" is her chat. Gesture fixes found by the live walk (fix the
  gesture, never the assertion): (1) beats sharing one server must be
  unlock-state-tolerant (`unlockIfLocked` helper — the old single-test file
  never hit this; each failing beat had been getting a fresh locked server
  only because Playwright restarts the worker after a failure); (2) a
  quick-created character has no description, so the roster card's
  `p.line-clamp-3` is empty/unclickable AND the card title is an `h2`
  inside the routerLink anchor (not `h3`); (3) the delete-dialog confirm
  must be scoped to `qt-character-delete-dialog` — the edit view's
  danger-zone button keeps the same accname under the overlay; (4) count
  gallery tiles by the `Delete this photo` affordance — a bare `img[alt]`
  count catches the detail header's avatar.

**The full gate:** `cargo fmt` clean; clippy `-D warnings` clean on the
default set AND `--features quilltap-core/native-transport`; release build
clean; characters-reads (24 cases) + characters-mutations (22 cases) oracles
regenerated FRESH from v4 at `a7b1398d` and both differentials re-run green
by name; `cargo test --workspace` green (275 suites, incl. the 842-test
core suite); `ng test` 206 green; `ng build` clean; the FULL Playwright
suite 10/10 green against the fresh build (the four characters-flow beats
incl. the three new live ones).

**What stays deferred (all loud, none silent):** ST PNG export/import + the
photo multipart upload (quilltap-web multipart/binary routes); the
`photo-save-fileid` fileId leg (host file-store bytes seam); the P4.6f
tier-3 LLM refusals (ai-wizard / optimizer / rename / ai-import /
reset-builtins / refresh-archive); the P4.6g deferred verticals (wardrobe
dialog, rename/replace). Lane B's flagged divergences stand as recorded
(story-background thumbnail; the additive edit-view delete entry point).

**Orders P4.6f, P4.6g, P4.6i, P4.6j are all CLOSED.** Versions after the
round: core 0.0.172, harness 0.0.157, host 0.0.10, web 0.0.7, SPA 0.5.8.

---

## P4.6k (lane A) unit 1 — Groups server surface (2026-07-11, in progress)

The groups half of the Prospero dispatch backfill (`groups_routes_equivalence`).
New `crates/quilltap-core/src/api/groups.rs` + the pinned Groups/Projects
`Request`/`Response` variants (the full Shared contract, per the work order's
"pin in the first commit") + engine dispatch. Landed + differential-proven vs
v4's REAL route handlers (`groups/route.ts`, `[id]/actions/group-crud.ts`,
`[id]/mount-points/route.ts`):

- **Reads:** list (createdAt-desc + `_count.members` cross-partition), detail
  (rich + empty), members (`{id,name}` null-filtered), mount-points (dangling
  link filtered; official store IS in the list; empty group).
- **Mutations:** create (`|| null` coercion, `state:{}`, Scenarios/Knowledge
  folder-ensure dumped as DB state), update (passthrough patch), delete
  (memberships + links dropped, row gone, **official store SURVIVES**),
  addMember (idempotent), removeMember (result ignored), mount link (idempotent
  echo `{link, mountPoint}`), mount unlink (`{message}` + link dump).

Recipe notes for the next lanes / regen:
- Fixture built with pinned entity ids via `create(data, {id,...})` (store-backed
  create accepts CreateOptions); `officialMountPointId`/link ids are MINTED at
  build but baked → both differential sides read identical values (no remap).
  Only the route-level CREATE mints fresh ids → blank `id`/`createdAt`/
  `updatedAt`/`officialMountPointId`. Update mints a fresh `updatedAt` → blank it.
- v4's `DocMountPointSchema` DTO **omits** null nullable fields (`lastScannedAt`/
  `lastScanError`/`conversionError`) — the hydrator (`find_full_json_by_id`) must
  skip them, not render null.
- Fixture regen recipe is in the builder header
  (`build-groups-projects-fixture.ts`); oracle regen in `groups-routes.test.ts`.

Repo additions (shared db files, append-only union-safe):
`group_character_members::{add_member,remove_member,delete_by_group_id}`,
`group_doc_mount_links::{unlink,delete_by_group_id,link_returning}`,
`doc_mount_points::find_full_json_by_id`.

Still OPEN under lane A: projects units (2), scenarios + union (3), project
wardrobe (4), list-files/background/aesthetic (5). Their variants answer the
loud `not_available` refusal until landed. Versions: core 0.0.173, harness
0.0.158.

## P4.6k (lane A) unit 2 — Projects server surface (2026-07-11, in progress)

Projects CRUD + roster + chats + state + tool-settings + mount-points, all
differential-proven vs v4's REAL route handlers (`projects/route.ts`,
`[id]/actions/{project-crud,roster,chats,state,tools}.ts`, `mount-points/route.ts`)
via `projects_routes_equivalence` (21 cases). Faithful ports of the quirks:
O(n²) list `_count` (chats.findAll+files.findAll per project), the enriched
roster detail (`{id, name, [defaultImageId], defaultImage, tags, chatCount}` —
defaultImageId OMITTED when the character's is undefined, matching JS
stringify), list-chats pagination + `lastMessageAt ?? updatedAt` sort fallback,
delete nulling chats/files projectId but NOT touching `projectDocMountLinks`,
hand-rolled roster (idempotent add / always-write remove), state
get(`{success,state}`)/set(`{success,state}`)/reset(`{success,previousState}`),
tool-settings, mount link/unlink.

Gotchas banked:
- **null-vs-absent seam at create:** v4's route injects `color/icon: x || null`;
  v5's `ProjectProperties` folds null→absent (a documented repo-layer open-JSON
  seam). The create differential passes `icon` explicitly to avoid it; a create
  omitting color/icon diverges (v4 stores null, v5 omits) — carried from Phase 2.
- Update/create/mount-link mint a fresh `updatedAt`/link id → blank those keys;
  all other ids are baked → no remap.
- Repo additions: `project_doc_mount_links::{unlink, link_returning}`.

Still OPEN under lane A: scenarios + union (3), project wardrobe (4),
list-files/background/aesthetic (5). Versions: core 0.0.174, harness 0.0.159.

## P4.6k (lane A) units 4+5partial — project wardrobe + background/aesthetic (2026-07-11)

Unit 4 (project wardrobe, 5 variants) + Unit 5's background + aesthetic get/set
(3 of 5 variants), all differential-proven in projects_routes_equivalence (35
cases total). Reused the P4.6f vault-write machinery (create/update/delete_
project_wardrobe_item, read_project_wardrobe) pointed at the project store's
Wardrobe/ folder. Gotchas: the wardrobe UPDATE must re-read through
read_project_wardrobe (the WardrobeItem struct serialize skips None fields; the
Value read path renders them null — v4's JS object always emits the full shape);
aesthetic GET returns the RAW store-file content (readStoreFile does NOT trim —
only the injection path does); aesthetic SET empty/whitespace DELETES the file.

STILL OPEN under lane A: Unit 3 (scenarios both families + participant-union,
12 variants) and Unit 5's list-files two-branch (2 variants) — both answer the
loud not_available refusal. Versions: core 0.0.176, harness 0.0.161.
### P4.6m unit 1 — the SillyTavern PNG codec (tier-1) — LANDED

Lane C of the P4.6k ∥ P4.6l ∥ P4.6m round (branch
`claude/multipart-binary-routes-b08c2c`). v4 baseline `a7b1398d` (no drift).

Ported the PNG legs of v4 `lib/sillytavern/character.ts` into
`quilltap-core::services::sillytavern`: `create_st_character_png`,
`parse_st_character_png`, `generate_placeholder_png`, `calculate_crc32`, and
the chunk framing. v4 hand-rolls the whole codec with NO image library
(CRC32 poly `0xedb88320`, `u32 len | type | data | u32 CRC` chunk framing, a
256×256 name-hash solid-colour placeholder, the `tEXt` insert immediately
after IHDR); the port matches, staying pure and adding no core dependency.
The JSON embedded in the `tEXt` chunk is `serde_json::to_string` over
`export_st_character` (preserve_order → byte-identical to v4's
`JSON.stringify`).

**Two faithful-port notes carried forward:** (1) `parse_st_character_png`
replicates v4's null-terminator `continue` that does NOT advance the chunk
offset — a `tEXt` chunk with no NUL byte loops forever in v4 too; valid PNG
text chunks always carry a keyword/NUL separator so it is unreachable in
practice (documented, not altered). (2) The placeholder DEFLATE is a
declared seam: v4 emits `zlib.deflateSync` (compressed), the port emits
stored (uncompressed) DEFLATE blocks via a hand-rolled `zlib_stored` — both
valid zlib that inflate to identical raw scanlines.

**Differential:** new `harness/oracle/cases/st-png.ts` drives v4's REAL
`createSTCharacterPNG`/`parseSTCharacterPNG` over a corpus (rich
`sillyTavernData` round-trip, default-baseData, unicode/emoji fields;
placeholder ascii + unicode names; decode cases: chara card / ccv2 keyword /
bare-data / spec-without-data / bad-signature / bad-JSON / other-keyword /
no-tEXt). `st_png_equivalence` asserts: encode/real-avatar BYTE-IDENTICAL;
encode/placeholder tEXt+IHDR byte-identical and IDAT inflates (flate2
dev-dep) to identical pixels; decode results match. Green (3 real-avatar, 2
placeholder, 8 decode).

Regen recipe:
```
cd ~/source/quilltap-server
npx tsx <worktree>/harness/oracle/cases/st-png.ts > /tmp/oracle-st-png.ndjson
cd <worktree>
QT_ORACLE_ST_PNG=/tmp/oracle-st-png.ndjson \
  cargo test -p quilltap-harness --test st_png_equivalence
```

Versions: core 0.0.173, harness 0.0.158. Next: the multipart machinery +
photos POST route (unit 2), the PNG export route (unit 3), the ST import
route + main-avatar vault write (unit 4), refusal-arm retirement +
`photo_link_summary` dedup (unit 5).

---

### P4.6m unit 2 — multipart machinery + the photos POST route (all 3 legs) — LANDED

Gave `quilltap-web` its first `multipart/form-data` machinery and the first
v4-shaped multipart route.

- **`quilltap-web::multipart`** — a browser-`FormData`-shaped helper over axum's
  `Multipart` extractor: whole-body buffering (v4 buffers too — no streaming
  uploads), `file(name)` (a part is a "file" ⟺ it carried a `filename`, matching
  v4's `instanceof File`), `text(name)`, `all_text(name)` (non-empty string
  values, file parts dropped — v4's `getAll(...).filter(typeof === 'string')`).
  Reusable for the remaining multipart routes (import [unit 4] + the deferred
  images-v2 / attachments / mount-ingest / .qtap families).
- **`POST /api/v1/characters/{id}/photos`** (`characters_routes.rs`) — v4's
  content-type dispatch: JSON → `{fileId|linkId}` (exactly-one refine); else a
  multipart upload. Three legs behind the ported gallery service:
  `save_link_to_character_gallery` (linkId), `save_to_character_gallery` (upload
  bytes), and the fileId leg = `files.find_by_id` + the image guard + the
  two-mode `download_file` (`mount-blob:` → the DB blob, else the disk
  `LocalStorageBackend`) + delegate. v4's error mapping reproduced exactly
  (`Character not found` → 404; the 7-keyword list → 400; else 500). Thin edge
  code — the write closures run on the writer thread (`db.write` with both
  partition connections); no business logic in the transport.
- **Deps:** axum `multipart` feature; `rusqlite` promoted to a normal dep (the
  write closures name `rusqlite::Connection`); reqwest dev-dep gained
  `multipart` + `json` for the route test.

**Proofs.** (1) Route-level integration
(`crates/quilltap-web/tests/characters_photos_routes.rs`): all three legs in
both fileId storage modes (local-key off disk, `mount-blob:` off the DB) + every
error arm (missing file, non-image mime, fileId-not-found, non-image fileId,
both-ids, neither-id, character-not-found) — REAL HTTP multipart/JSON bodies
against the committed characters fixture (Aria + her vault). (2) A tier-2
differential (`character-photo-upload-tier2` oracle +
`character_photo_upload_tier2_equivalence`) driving v4's REAL
`saveToCharacterGallery` over the UPLOAD-specific filename→path branches (dotless
timestamped slug vs the dotted `sanitizeLeafName`), the dedup guard, and the two
400-keyword arms — the freshly-written `photos/` link dumped and diffed
byte-exact (the minted per-blob `fileId` is the one blanked value; frozen
`kept_at`). The shared write spine was already proven by `photo_save_link`.

**Scoping note (proportionality, documented — NOT a silent gap):** the fileId
leg's byte fetch is `download_file`, already differentially proven
(`file_storage` tests; the same function `files_routes` serves) and re-exercised
end-to-end by the route test in BOTH storage modes; its write is the proven
`save_to_character_gallery` spine. So the tier-2 differential targets the one
genuinely-new core write branch (the upload filename→path logic) rather than
re-baking a `files` table into the committed characters fixture (which stays
frozen for lanes A/B). The committed `characters-*.db` is UNCHANGED, so the
existing characters-reads / characters-mutations oracles do NOT need regen.

Regen recipe (jest /tmp mirror; Node 24):
```
N=~/.nvm/versions/node/v24.13.1/bin ; V5W=<worktree>
TMPO=/tmp/qt-photo-upload-oracle
rm -rf "$TMPO"; mkdir -p "$TMPO/cases" "$TMPO/fixtures"
cp "$V5W/harness/oracle/cases/character-photo-upload-tier2.test.ts" "$TMPO/cases/"
cp "$V5W/harness/oracle/fixtures/characters.json"                   "$TMPO/fixtures/"
cd ~/source/quilltap-server
QT_FIXTURE_CHARACTERS_MAIN=$V5W/crates/quilltap-web/tests/fixtures/characters-main.db \
QT_FIXTURE_CHARACTERS_MOUNT=$V5W/crates/quilltap-web/tests/fixtures/characters-mount.db \
QT_ORACLE_OUT=/tmp/oracle-photo-upload.ndjson \
  $N/npx jest --silent --watchman=false --testTimeout=120000 \
    --roots "$PWD" --roots "$TMPO/cases" -- character-photo-upload-tier2
cd $V5W
QT_ORACLE_PHOTO_UPLOAD=/tmp/oracle-photo-upload.ndjson \
  cargo test -p quilltap-harness --test character_photo_upload_tier2_equivalence
```

Versions: core 0.0.174, harness 0.0.159, web 0.0.8. Next: the PNG export route
(unit 3), the ST import route + main-avatar vault write (unit 4), refusal-arm
retirement + `photo_link_summary` dedup (unit 5).

---

### P4.6m unit 3 — the SillyTavern PNG export route — LANDED

`GET /api/v1/characters/{id}?action=export` (`characters_routes::characters_get`)
— v4 `handlers/get.ts`'s export action. `format=png` → `create_st_character_png`
over the overlaid character + the avatar bytes (`read_avatar_bytes`: the vault
link's blob, else the legacy `files` row via `download_file`; missing/unreadable
→ the placeholder), streamed `image/png` with `Content-Disposition: attachment;
filename="<name>.png"`. `format=json` → the pretty ST card as an attachment
(v4-faithful REST parity; the SPA's JSON path is dispatch `character_export`).
Non-export actions → 400 (a loud pointer to `/api/dispatch`); unknown character
→ 404. Overlaid read via the api layer's `read_main` → `read_mount_index`
nesting. Closes the `export-png` deferral.

**Faithful edge note:** Aria's avatar is a WebP; v4's `createSTCharacterPNG`
reads the IHDR length from the avatar and (for a non-PNG container) walks off the
end — `Buffer.subarray` clamps, appending the `tEXt` at the tail. The port
clamps identically (`insert_offset.min(len)`), byte-for-byte the same broken-but-
faithful output. The placeholder leg (no avatar) is a valid 256×256 PNG that
round-trips through `parse_st_character_png`.

Proof: `crates/quilltap-web/tests/characters_export_route.rs` — the real-avatar
embed (card keyword + spec present), the placeholder round-trip, the JSON leg,
and the 404 / 400 arms. The PNG codec's byte-exactness is proven by unit 1's
`st_png_equivalence` tier-1 differential.

Version: web 0.0.9. Next: the ST import route + main-avatar vault write (unit 4),
refusal-arm retirement + `photo_link_summary` dedup (unit 5).

---

### P4.6m unit 4 — the ST import multipart route + the main-avatar vault write — LANDED

Two pieces.

- **`write_main_avatar_to_vault`** (`services/image_job_storage.rs`) — v4
  `writeCharacterAvatarToVault({ kind: 'main' })`: `transcode_to_webp` (the
  injected codec seam) → `ensure_folder_path('images')` → delete-then-insert at
  `images/avatar.webp` (`find_by_mount_point_and_path` → `delete_with_gc` the
  existing avatar, then `link_blob_content` the replacement) → returns the new
  link id. Errs when the character has no database-backed vault. The sibling
  `write_character_avatar_to_vault` (history kind) is unchanged.
- **`POST /api/v1/characters?action=import`** (`characters_routes::
  characters_import_post`) — v4 `handleImport`, multipart leg. A `.png`
  (`content-type image/png` or `.png` name) → `parse_st_character_png` (null →
  400 `Invalid SillyTavern PNG file`); a `.json` → `serde_json` parse. The card
  is created through the ported `character_import` spine (proven by
  `st_import_card`); for the PNG leg the bytes are written as the main avatar
  (non-fatal per v4 — failure keeps the character) and `defaultImageId` is set
  (raw slim update) + reflected in the echo. Wrong `action` / non-multipart → a
  loud 400 pointer to `/api/dispatch`.

**Proofs.** (1) Route integration
(`crates/quilltap-web/tests/characters_import_route.rs`): the PNG import (create
+ avatar + `defaultImageId`), verified END-TO-END by re-exporting the created
character through the unit-3 route and confirming the container is the
transcoded WebP carrying the card; the JSON-file leg (no avatar); the error arms
(no file, invalid ST PNG, unsupported type, wrong action). (2) A tier-2
differential (`character-avatar-write-tier2` oracle +
`character_avatar_write_tier2_equivalence`) driving v4's REAL
`writeCharacterAvatarToVault({kind:'main'})` over Aria (who already has an
avatar, so the delete-then-insert REPLACEMENT is exercised): the link row's
stable fields (`relativePath`/`fileName`/`originalMimeType`/`storedMimeType`) +
the replaced link-count (1) + the blob's decoded metadata (16×16 WebP, non-empty)
diffed exactly; the WebP bytes + `sha256` are the declared codec seam
(`[[w4-9c-image-job-handlers]]` — sharp vs the host `image`/`webp` stack).

**Scoping note:** the CREATE half of the import is proven by the standing
`st_import_card` differential; this unit's differential isolates the genuinely-new
`write_main_avatar_to_vault`. The oracle imports sharp by absolute path
(`packages/quilltap/node_modules/sharp`) since jest can't resolve it from the
/tmp mirror.

Regen recipe:
```
N=~/.nvm/versions/node/v24.13.1/bin ; V5W=<worktree>
TMPO=/tmp/qt-avatar-write-oracle
rm -rf "$TMPO"; mkdir -p "$TMPO/cases" "$TMPO/fixtures"
cp "$V5W/harness/oracle/cases/character-avatar-write-tier2.test.ts" "$TMPO/cases/"
cp "$V5W/harness/oracle/fixtures/characters.json"                   "$TMPO/fixtures/"
cd ~/source/quilltap-server
QT_FIXTURE_CHARACTERS_MAIN=$V5W/crates/quilltap-web/tests/fixtures/characters-main.db \
QT_FIXTURE_CHARACTERS_MOUNT=$V5W/crates/quilltap-web/tests/fixtures/characters-mount.db \
QT_ORACLE_OUT=/tmp/oracle-avatar-write.ndjson \
  $N/npx jest --silent --watchman=false --testTimeout=120000 \
    --roots "$PWD" --roots "$TMPO/cases" -- character-avatar-write-tier2
cd $V5W
QT_ORACLE_AVATAR_WRITE=/tmp/oracle-avatar-write.ndjson \
  cargo test -p quilltap-harness --test character_avatar_write_tier2_equivalence
```

Versions: core 0.0.175, harness 0.0.160, web 0.0.10. Next: refusal-arm
retirement + `photo_link_summary` dedup (unit 5).

---

### P4.6m unit 5 — refusal-arm retirement + `photo_link_summary` dedup — LANDED

- **Refusals re-messaged to point at the now-live REST routes** (all three P4.6m
  deferrals closed): `character_export`'s `format=png` arm and
  `character_photo_save_by_id`'s fileId leg no longer `not_available` — they
  return a `bad_request` naming the quilltap-web route that carries the binary
  (`GET …?action=export&format=png`, `POST …/photos {fileId}`); the
  `character_import` doc note now describes the multipart route
  (`POST /api/v1/characters?action=import`) reusing its create spine + writing the
  avatar. `not_available` stays `pub` (the remaining P4.6f tier-3 LLM refusals).
- **`photo_link_summary` unified:** `api::salon`'s byte-identical private copy is
  deleted; `resolve_message_attachments` now calls the shared
  `photos::photo_link_summary::get_photo_link_summary_by_sha256`. Pure code
  motion (identical body) — no behavior change; the module note is updated.

Version: core 0.0.176. **P4.6m is COMPLETE** — all five units landed; the three
standing byte-shaped characters deferrals (photo multipart upload,
photo-save-fileid, SillyTavern PNG export/import) are CLOSED.
## P4.6l — Groups + Projects SPA (lane B, in progress)

Tier-4 SPA lane; no Rust. Coded against a mocked `CoreClient` over the
p4.6k/l Shared contract (lane A pins the server side in parallel).

**Commit 1 — the Groups vertical.** `apps/web/src/app/screens/groups/`:
`groups.api.ts` (dispatch helpers + TanStack keys), `group-card.ts`,
`group-create-dialog.ts`, `groups-section.ts` (the Characters-page section
above the roster), `group-members-card.ts`, `group-stores-card.ts`,
`group-editor.ts` (routed at `/characters/groups/:id`). Wired into
`characters-list.ts` (Groups section + toolbar Create Group button) and
`app.routes.ts`. `core-contract.ts` gained all 18 group + 40 project
Request variants + the groups/projects DTO interfaces. `ui/format-bytes.ts`
ports v4 `lib/utils/format-bytes`.

Faithful v4 behaviors: card = 10×10 swatch + emoji (users-icon fallback),
member count, 2-line-clamp description, Edit link + immediate no-confirm
Delete; editor = explicit-Save `<form>` (name*/description/color/icon, no
autosave, no image upload) over two collapsed cards. The routed editor path
diverges from v4's `/aurora/groups/[id]` → `/characters/groups/:id` (v5
idiom; recorded).

**Recorded divergence / loud deferral:** the "Link Document Store" picker in
the Scriptorium card is a DISABLED affordance (v4-register tooltip). Reason:
`groupMountPointList`/`projectMountPointList` return only the LINKED stores;
the GLOBAL mount-points listing (v4 `GET /api/v1/mount-points`) is not a
ported dispatch surface this round (it belongs to the future Scriptorium
vertical). List + unlink are live; linking a new store is not.

**Finding-#6 discipline:** the Add-Member `<select>` (async options) binds
`[selected]` per option and reads the choice via `(change)`, never `[value]`
on the select.

**Gate (commit 1):** `ng test` 35 files / 219 tests green (incl. 7 new
groups tests); `ng build` clean; `groups-flow.spec.ts` parses (2 beats) and skips
until lane A's `groups-projects-{main,mount}.db` fixture lands (auto-activates
via a fixture-existence guard). SPA 0.5.12.

**Commit 2 — the Projects (Prospero) vertical, tier 1.** `apps/web/src/app/
screens/prospero/`: `projects.api.ts`, `project-card-state.ts` (the
first-visit localStorage memory), `project-card.ts`, `project-create-dialog.ts`,
`project-delete-dialog.ts`, `prospero-list.ts` (the `/prospero` list), plus the
routed `project-detail.ts` composing `cards/project-header.ts`,
`cards/project-characters-card.ts`, `cards/project-model-behavior-card.ts`,
`cards/project-settings-card.ts`, `cards/project-chats-section.ts`, the
`state-editor-modal.ts`, and the reused groups `group-stores-card.ts` for the
Scriptorium. Routes `/prospero` + `/prospero/:id` registered; the Projects nav
item enabled → `/prospero`. The shared salon `chat-card.ts` grew an optional
`removable` mode (v4 `actionType="remove"`, disassociates). `collapsible-card.ts`
now seeds `defaultOpen` in `ngOnInit` (a bound signal input isn't readable in the
constructor — the pre-existing `forceOpen` effect is unaffected).

Faithful behaviors: list card = swatch/emoji (folder fallback) + "N chats • M
files"; delete confirm copy ("disassociated but not deleted"); detail = 1/2/3-col
`grid-flow-row-dense`; header inline edit saves name+description+instructions
together; Characters "Allow Any Character" immediate toggle + roster grid (no add
picker, "added when chats are associated"); Model Behavior Agent Mode + Answer
Confirmation immediate selects; Settings instructions + Project State modal; chats
section paginated (page size 20) with the removable ChatCard.

**Loud deferrals / recorded divergences (commit 2):** (a) Default Roleplay
Template select + Default Tool Settings row disabled (no roleplay-templates /
tools listing dispatch surface in v5 this round); (b) project Scriptorium
link-store picker disabled (no global mount-points listing — same as groups);
(c) Project Instructions is a plain `<textarea>`, not v4's `MarkdownLexicalEditor`
(bytes round-trip exactly); (d) the chats IntersectionObserver auto-load is a
"Load more" button (v4's visible fallback). Tier-2 cards (Files, Scenarios,
Wardrobe, Image Generation) are the next slice.

**Gate (commit 2):** `ng test` 36 files / 229 tests green (10 new projects
tests + the groups suite); `ng build` clean; `projects-flow.spec.ts` parses
(2 beats) and skips until lane A's fixture lands. SPA 0.5.13.

**Commit 3 — the characters riders + the `<select [value]>` audit (tier 2).**
Riders (byte-leg web routes lane C ships; called by `fetch`, live at
unification): `characters.api.ts` gained `uploadCharacterPhoto` (multipart
`POST /api/v1/characters/{id}/photos`) and `downloadCharacterPng`
(`GET ?action=export&format=png` → blob download). The gallery tab's Upload
Photo button is now live (failed upload surfaces the v4 400-keyword message via
`{error}`); the roster card gained an "Export as PNG" button beside the JSON
export. The ST-import PNG multipart leg was ALREADY wired (P4.6j). The live
`characters-flow` upload/PNG beats defer to unification (the base web binary
lacks lane C's routes); the riders are unit-tested with mocked `fetch`.

**The finding-#6 `<select>` audit (per-site verdict):**
- CONVERTED to per-option `[selected]` (saved value + async options): 
  `settings/providers/cheap-llm-card.ts` (userDefinedProfileId + 
  defaultCheapProfileId), `settings/providers/profile-modal.ts` (provider + 
  apiKeyId), `settings/wizard/steps/model-selection-step.ts` (provider).
- SAFE, no change (recorded): `characters/edit/details-tab.ts` pronoun preset 
  (STATIC options); `characters/new/new-character.ts` connection-profile 
  (NEW form — value starts '' before options, only set by user post-load); 
  `characters/view/tabs/details-tab.ts` reverse-user dialog (options from 
  already-loaded sync data, dialog opens post-load); 
  `settings/providers/api-key-modal.ts` provider (CREATE-only modal, 
  `provider=signal('')`); `settings/providers/profile-modal.ts:475` modelClass 
  (STATIC `modelClasses` array).
Every NEW select this lane wrote (group members picker, project model-behavior 
selects) binds `[selected]` per option from the start.

**Gate (commit 3):** `ng test` 36 files / 231 tests green (2 new gallery-upload 
tests; settings + characters specs green after the conversions); `ng build` 
clean. SPA 0.5.14.

**Commit 4 — the project-detail tier-2 cards (Files + Image Generation).**
`cards/project-files-card.ts` (list first 10 files, image thumbnails via the
blob filepath, name/size/category, a plain-`<img>` lightbox on click; "Browse
All Files" disabled — the FileBrowser/FilePreview family + project file upload
are tier-3 loud deferrals). `cards/project-image-generation-card.ts` (Avatar
Generation + Announce Lantern Images + Story-Background display-mode immediate
selects, all static-option → per-option `[selected]`; the Default Image Profile
select disabled — no image-profiles listing dispatch surface this round) +
`cards/project-aesthetic-field.ts` (the two aesthetic textareas over
`projectAestheticGet/Set`, byte-exact round-trip; v4's Lexical editor → plain
textarea, recorded). Both wired into `project-detail.ts`.

**OPEN tier-2 remaining (deferred LOUDLY — disabled "not yet available" cards in
the detail grid, NOT silent):**
- **Scenarios card + ScenariosManager** — blocked on the scenario dispatch body
  fields: v4's REST uses `filename`/`body`/`newFilename` (+ `name`/`description`/
  `isDefault`, 5 fields on create), but the pinned dispatch contract only has
  `{name, content, isDefault}` (3 fields, no explicit filename on create). Lane A
  must pin the exact scenario create/update/rename bodies before this can be
  built without speculative field mapping. The core-contract's
  `Group/ProjectScenario*` variants may need field changes at that point.
- **Wardrobe card + ProjectWardrobeManager** (v4 360-ln self-contained inline
  form: title/description/imagePrompt/slot-types/appropriateness/isDefault/
  replace/composite componentItemIds). Banked to a follow-up slice; the
  `projectWardrobe*` dispatch variants (opaque `item` bag) are in the contract.

**Gate (commit 4):** `ng test` 36 files / 237 tests green (6 new: ImageGen +
Files + aesthetic round-trip); `ng build` clean. SPA 0.5.15.

## The P4.6k ∥ P4.6l ∥ P4.6m groups+projects+multipart round — UNIFIED on main (2026-07-11)

Three lanes cherry-picked onto `unify/p4.6klm` (lane A's four server
commits, lane C's five multipart/PNG commits, lane B's four SPA commits;
conflicts only on version files + the CHANGELOG/status-log unions). v4
baseline re-verified at `a7b1398d` before unification. The reconciliation
restored lane C's `flate2` harness dev-dep dropped by a version-line
resolution (the whole-file Cargo.toml check earns its keep again).

**The unification wires (two commits):**

- **The A↔B dispatch contract diffed name-for-name:** 53 group/project
  variants, identical on both sides. But the LIVE walks caught a
  field-shape drift the name diff can't see: the SPA sent
  `groupUpdate`/`projectUpdate`/`projectCreate` fields FLAT while lane
  A's differential-proven shape nests the patch in a `group`/`project`
  bag. Senders, `core-contract.ts`, and unit tests reconciled to the
  pinned server shape (the same reconciliation class as P4.6i/j's
  gallery envelope: the differential-proven side wins).
- **A real SPA layering bug:** `.qt-page-container > *` (the v4-ported
  story-background rule) gives every direct page child a z-1 stacking
  context, trapping any `.qt-dialog-overlay` (fixed, z-60) opened from an
  EARLY child beneath later siblings — the groups Create dialog was
  unclickable under the roster grid. Diagnosed live in the browser
  (`elementFromPoint` pierced the overlay). Fixed in
  `_content.css` with `.qt-page-container > *:has(.qt-dialog-overlay)
  { z-index: 60 }` — systemic, covers every dialog-in-early-child site.
- **Live e2e beats added/activated:** the characters gallery multipart
  upload + the ST PNG-export download (NEW beats over lane C's routes —
  the PNG assertion checks the embedded card, since the container is
  v4-faithful to the avatar bytes: Aria's WebP avatar yields a RIFF
  container per v4's clamped-offset quirk); the groups/projects walks
  activated over lane A's fixture. Gesture fixes (fix the gesture, never
  the assertion): the fixture userId-rewrite tolerates absent tables
  (`tags` — never materialized; repos auto-ensure) and un-scoped
  store-backed rows (groups/projects have no `userId` column); strict-mode
  locator scoping for headings/buttons that lane B's own commit-4 cards
  made ambiguous; the upload beat tolerates an empty gallery (the earlier
  remove beat may have taken the last photo).

**The full gate:** `cargo fmt` clean; clippy `-D warnings` clean on the
default set AND `--features quilltap-core/native-transport`; release
build clean; the five round oracles regenerated FRESH from v4 at
`a7b1398d` (groups-routes 14, projects-routes 33, st-png 13,
photo-upload 5, avatar-write 1) and every differential re-run green by
name; `cargo test --workspace` green (283 suites / 1221 tests); `ng test`
36 files / 237 green; `ng build` clean; the FULL Playwright suite 16/16
green (incl. the two new rider beats and the four newly-live
groups/projects beats). Mid-gate ENOSPC handled per the standing recipe
(lane worktrees removed post-cherry-pick, incremental cache cleared,
`CARGO_INCREMENTAL=0`).

**What stays OPEN (all loud, none silent):** P4.6k unit 3 — scenarios
(both families + the participant-union, 12 refusal-armed variants; the
scenario body fields need a re-pin from v4's Zod schemas first — lane B
found v4 uses `filename`/`body`/`newFilename`, richer than the pinned
sketch) and unit 5's `list-files` two-branch (2 variants); P4.6l's
Scenarios + Wardrobe cards (loud disabled cards, blocked on/banked with
the same re-pin); the P4.6l recorded divergences (disabled link-store /
roleplay-template / tool-settings / image-profile pickers pending their
listing surfaces; textarea editors for Lexical; the Load-more button).
P4.6m is COMPLETE — the photo-upload / photo-save-fileid / ST-PNG
deferrals are CLOSED; the remaining multipart families (images-v2, chat
attachments, files upload, mount ingest, .qtap, themes) stay deferred
with the reusable `quilltap-web::multipart` helper waiting.

**Orders:** P4.6m CLOSED; P4.6k and P4.6l LANDED-partial with enumerated
remainders (see their status headers). Versions after the round: core
0.0.176, harness 0.0.161, host 0.0.10, web 0.0.10, SPA 0.5.16.

---

### P4.6o (lane B) — the Scenarios + Wardrobe SPA remainder — LANE COMPLETE (branch, awaits unification)

Tier-4 SPA lane; v4 is the behavioral/visual reference, no byte target.
Drift-checked v4 clean at `a7b1398d`. Closes the P4.6l SPA remainder (the
two loud-disabled Prospero cards + the disabled `scenarios` nav item).
Five commits on `claude/p4-6o-scenarios-wardrobe-spa-fc23ae`.

**Contract re-pin (`core-contract.ts`).** Re-pinned the SPA scenario bag
to v4's Zod-schema shape, identical across the group/project/general
families: create rides a nested `scenario` bag
`{filename, name?, description?, isDefault?, body}` (optionals truly
absent), update drops `filename` (the path rides the variant), rename
takes `newFilename`. The `ScenarioDto` gains
`filename`/`rawIsDefault`/`body`/`lastModified`/`createdAt`/`updatedAt`
(superseded the `{name, content, isDefault}` sketch). Added the six
net-new general `scenario*` request variants (`scenarioList`/`Create`/
`Get`/`Update`/`Rename`/`Delete`) + a `WardrobeItemDto` and
`WardrobeSlotType`. **Lane A owns the matching Rust change — reconcile
the variant/bag names + response bytes at unification.**

**The scope-agnostic `qt-scenarios-manager` family.** manager + row +
editor modal + a `ScenarioMutator` service interface with project-
(`projectScenario*`) and general- (`scenario*`) scoped factories over
`CoreClient` (`screens/scenarios/`). Exactly v4-shaped: the manager makes
no dispatches itself; the scope lives in the mutator. Delete uses
`window.confirm`, rename `window.prompt` on the FILENAME, set-default
re-sends update with `isDefault: true` (no dedicated verb). The editor is
a plain `<textarea>` (established Lexical divergence; body round-trips
byte-exact). Added a `closeOnBackdrop` input to the shared Modal
(default true, backward-compatible) for the no-click-outside editor.
12 vitest specs against a mock mutator.

**The `qt-project-wardrobe-manager`.** Self-contained inline draft form +
rows (`screens/prospero/wardrobe/`), fed by a project-scoped mutator over
`projectWardrobe*`. Blank optional strings ride as `null` (v4
`handleSave`); the composite picker excludes the item being edited; the
slot-type floor keeps ≥1 slot; rows show Composite/Default/Archived
badges and prefer the Portrait Cue over the description. 7 vitest specs.

**Wiring.** Both cards replace their loud-disabled placeholders on the
project detail; the general `/scenarios` page renders the manager at page
scope; the `scenarios` nav item is enabled and its route registered.
Removed the now-unused `CollapsibleCard` import from `project-detail.ts`.

**e2e (authored-but-mocked; activate at unification over lane A's
fixture).** `scenarios-flow.spec.ts` (project card: create → `.md`
suffix → edit body → set default → rename → delete; general page: create
+ list) + a wardrobe beat in `projects-flow.spec.ts` (create default item
→ badges → delete). Fixture-guarded `test.skip` until
`groups-projects-main.db` exists; both `window.confirm`/`window.prompt`
beats install one-shot `page.on('dialog')` handlers. Discovered by
`playwright --list`; not run live in-lane (no fixture / built binaries).

**Gate:** `ng test` 38 files / 256 tests green; `ng build` clean;
Playwright specs discovered/parse. SPA 0.5.16 → 0.5.21 (five commits).

**OPEN under this order (all loud, none silent):** the New-Chat form
(the primary scenario-picker consumer — the named NEXT SPA vertical; no
`/salon/new` route yet); the Lexical-equivalent markdown editor
(textarea divergence); the P4.6l recorded divergences (disabled
link-store / roleplay-template / tool-settings / image-profile pickers;
Load-more vs IntersectionObserver). No group scenarios card (matches v4).

**When this unifies, mark `p4.6l-groups-projects-spa.md` CLOSED.**

## P4.4u4 (lane C) — the sample-content import (the quilltap-import seed subset)

**In progress on `claude/p4-4u4-sample-content-7f9828`** (drift check clean at
v4 `a7b1398d`). Closes P4.4u3's family-3 deferral (the
`lorian-and-riya.qtap` sample-content import) and unblocks the characters
family's `reset-builtins` deferral.

**Unit 1 — the import service + differential + assets (DONE).**
`quilltap-core::services::quilltap_import` (new module family:
`mod`/`characters`/`memories`/`reconcile`). Ported the SEED SUBSET of v4's
`executeImport` (`lib/import/quilltap-import/`):

- `parse_export_file` + the legacy-JSON `format`/`version` hard pins
  (`quilltap-export` / `'1.0'`); loud NDJSON refusal on sniff.
- `execute_import(main, mount, user_id, export, options)` over the two
  connections (the sync-handlers-over-both-connections idiom): characters
  (id-then-name existence check → `skip`, else `create_character` — which
  mints a fresh id, provisions the vault, projects the managed fields — with
  the legacy `scenario` string → `scenarios[]` migration, then per-character
  **vault-backed** wardrobe via `create_vault_wardrobe_item`) → remap-only
  memories (`aboutCharacterId` remaps through the character map; `chatId`/
  `projectId` → null via the empty maps; `sourceMessageId`/`lastReinforcedAt`
  pass through) → the character reconcile loop (a faithful no-op for the seed).
- The one-big-try error semantics (a chokepoint DbError → `success:false` with
  the error in `warnings`; per-item failures → `warnings` + continue).
- **The deliberate divergence — loud typed refusals** (`ImportError`):
  unsupported entity kinds enumerated (anything beyond characters+memories),
  NDJSON payloads, non-`skip` conflict strategies, non-empty per-character
  `pluginData`. The seed file never trips any of them.

Key survey findings banked: `.qtap` is plain JSON, NOT an archive; the seed's
plural `physicalDescriptions` is IGNORED by `create` (which reads singular
`physicalDescription ?? null`) so physical files are skipped; wardrobe is
**vault-backed** (each item → a `Wardrobe/*.md`, no slim `wardrobe_items`
row); v4's `create` MINTS fresh ids (the source id is stripped) so imported
ids — incl. reset-builtins' "preserved" ids — are minted, not preserved;
memories are always-insert (a 2nd import duplicates them).

Committed assets (byte-identical to v4 @ `a7b1398d`):
`assets/first-startup/imports/lorian-and-riya.qtap`
(sha256 `b833f1e358a0a7d7608ae11b9f4cb3f473dcad5f55bce9dc1eb59f9eedc1970f`) +
`assets/first-startup/avatars/Lorian.webp`
(sha256 `c24d500c2923369a0152d45e5db4369388010e8c1a789d62374ac146052317d9`).

Differential — `qtap_import_equivalence` (tier-2): v4's REAL `executeImport`
over the committed `.qtap` on empty fixtures vs the port. Row-diffs
characters + `wardrobe_items` (empty, vault-backed) + memories + the vault
`points`/`folders`/`links` with a shared minted-id remap (FKs verify by
relationship, incl. each memory's remapped `characterId`/`aboutCharacterId`);
`memories` is walked LAST so the 2nd-import dupes can't offset the token
counter. The `doc_mount_files`/`_documents` content sha is a minted-content
seam (wardrobe `.md` embeds its minted id + timestamps) — diffed by row-count
+ `fileSizeBytes` multiset instead (the deterministic char-managed bytes are
already byte-proven by `characters_create_tier2`). Also exercises the
name-match `skip` branch (2nd import: characters skipped, wardrobe/vault
unchanged, memories doubled). Plus 5 Rust unit tests for the refusal arms.

Regen recipe (Node 24, from the v4 checkout):
```
N=~/.nvm/versions/node/v24.13.1/bin ; cd ~/source/quilltap-server
QT_FIXTURE_QTAPIMPORT_MAIN=/tmp/qt-qtapimport-main.db \
QT_FIXTURE_QTAPIMPORT_MOUNT=/tmp/qt-qtapimport-mount.db \
  $N/npx tsx <v5>/harness/oracle/fixtures/build-qtap-import-fixture.ts
QT_FIXTURE_QTAPIMPORT_MAIN=/tmp/qt-qtapimport-main.db \
QT_FIXTURE_QTAPIMPORT_MOUNT=/tmp/qt-qtapimport-mount.db \
  $N/npx tsx <v5>/harness/oracle/cases/qtap-import.ts > /tmp/oracle-qtap-import.ndjson
# run: QT_ORACLE_QTAPIMPORT + QT_FIXTURE_QTAPIMPORT_{MAIN,MOUNT} \
#   cargo test -p quilltap-harness --test qtap_import_equivalence
```

Versions after unit 1: core 0.0.177, harness 0.0.162.

**Unit 2 — the startup seed wire + avatars (DONE).** v4
`lib/startup/seed-initial-data.ts`'s gated tail, ported to
`quilltap-core::services::quilltap_import::seed` (run inside one
`Db::write` closure over both connections):

- `seed_sample_content` — the zero-characters gate (v4 :66–68:
  `findByUserId(SINGLE_USER_ID).length > 0 → return`) then
  `seed_from_imports` + `seed_avatars`.
- `seed_from_imports` — parse the embedded `.qtap` (`seed_assets`) and run
  `execute_import` with the seed options.
- `seed_avatars(main, mount, codec, target_names, report)` — match each
  seed avatar to its character by case-insensitive name, idempotency check
  via `resolve_character_avatar`, `write_main_avatar_to_vault` (main kind,
  delete-then-insert), `characters.update({defaultImageId})`. Both WebP
  avatars are stored AS-IS (WebP not in the transcodable set → passthrough
  → deterministic blob sha, no codec seam).
- `reseed_avatars_for_characters` — the reset-builtins entry point (unit 3).

Every layer swallows + collects errors into `SeedReport::warnings` (v4:
seeding never blocks boot). The two seed assets are embedded via
`include_bytes!`/`include_str!` (`seed_assets`) — no runtime filesystem
dependency.

**Host wire:** `HostConfig::seed_sample_content` (**default OFF this
lane**) threads to `HostAssembler`; `assemble` runs the gated seed after
`seed_built_ins`, on a joined OS thread through `write_blocking` with
`HostImageCodec`, gated on the mount-index partition. Default-off keeps
existing fresh-provision tests at zero characters (e.g.
`host_builtin_seeds` asserts `doc_mount_points == 3`, which the char
vaults would bump to 5). **Flipping the default ON — and updating the
fresh-boot e2e fixtures that then assert the seed — is a UNIFICATION
step.**

**⚠️ Survey correction (fix-the-port, not-the-diff):** the work order's
survey says "Riya has no avatar file", but v4 `a7b1398d` has BOTH
`first-startup/avatars/Lorian.webp` AND `Riya.webp`. The differential
(v4's real `reseedAvatarsForCharacters`) seeds both, so the port must too.
Committed both (Riya.webp sha256
`6b37bd5dbdfad0fde18083e354dbe5a6cb2bd7c3401f0af6fa5501e9201ebfe6`);
`SEED_AVATARS` carries both; the seed produces TWO avatars.

Differentials:
- `seed_avatars_equivalence` (tier-2): v4's real `executeImport` +
  `reseedAvatarsForCharacters(['Lorian','Riya'])` vs the port. Per
  character: the `images/avatar.webp` link + blob
  (relativePath/fileName/originalMimeType/storedMimeType/**sha256 exact**/
  size) diffed, `defaultImageId == link.id`, the two blobs are distinct
  files, and a 2nd reseed no-ops (defaultImageId stable, still 1 link
  each). Reuses the qtap-import fixtures (now with `doc_mount_blobs`
  materialized via its repo's hand-written DDL trigger).
- `host_sample_content_seed` (host integration smoke): provision fresh →
  boot with the flag on → 2 characters + 42 memories + 2 avatar links + 8
  wardrobe links; Lock + drop → second boot → gate short-circuits (state
  unchanged, no doubling).

Regen recipe (Node 24, from the v4 checkout — note the fixture rebuild now
materializes `doc_mount_blobs`; run from the v4 root so
`first-startup/avatars/` resolves):
```
N=~/.nvm/versions/node/v24.13.1/bin ; cd ~/source/quilltap-server
QT_FIXTURE_QTAPIMPORT_MAIN=/tmp/qt-qtapimport-main.db \
QT_FIXTURE_QTAPIMPORT_MOUNT=/tmp/qt-qtapimport-mount.db \
  $N/npx tsx <v5>/harness/oracle/fixtures/build-qtap-import-fixture.ts
QT_FIXTURE_QTAPIMPORT_MAIN=/tmp/qt-qtapimport-main.db \
QT_FIXTURE_QTAPIMPORT_MOUNT=/tmp/qt-qtapimport-mount.db \
  $N/npx tsx <v5>/harness/oracle/cases/seed-avatars.ts > /tmp/oracle-seed-avatars.ndjson
# run: QT_ORACLE_SEED_AVATARS + QT_FIXTURE_QTAPIMPORT_{MAIN,MOUNT} \
#   cargo test -p quilltap-harness --test seed_avatars_equivalence
```

Versions after unit 2: core 0.0.178, harness 0.0.163, host 0.0.11.

**Unit 3 — reset_builtins as a service (tier 2, DONE).** v4
`handleResetBuiltins` (`app/api/v1/characters/handlers/post.ts:196`) ported
to `quilltap-core::services::quilltap_import::reset::reset_builtins`
(run inside one `Db::write` closure):

- Cascade-delete each built-in (`BUILTIN_CHARACTER_NAMES = ['Lorian',
  'Riya']`) via `execute_cascade_delete(id, false, false)` (P4.6i);
  capture `preserved_ids` first.
- Seed-id → preserved-id remap: `find_builtin_character_ids` (`{name: id}`
  from `data.characters`) + `replace_mapped_ids_recursively` (the recursive
  string remap over the whole export), then re-`execute_import` with the
  seed options.
- `reseed_avatars_for_characters(['Lorian','Riya'])`.
- Returns `{deleted_character_ids, preserved_ids, post_reset_ids,
  remapped_id_count, import}`.

**Banked quirk (ported faithfully):** `create` MINTS a fresh id (strips the
source id), so the seed-id → preserved-id remap does NOT make the
re-imported character keep its old id — `postResetIds` are a THIRD set of
minted ids, different from `preservedIds`. The remap machinery is
effectively vestigial for character ids; ported verbatim (the differential
pins it). Confirmed on both sides (v4 body: preserved
`dca3ec57…`/`3bd0cbae…`, postReset `0cba9cdf…`/`f50edb1b…`,
remappedIdCount 2).

Differential — `reset_builtins_equivalence` (tier-2): v4's REAL
`handleResetBuiltins` driven through the collection route
(`POST /characters?action=reset-builtins`, jest, auth-session +
startup-state mocked, character-vault-bridge un-mocked) over a PRE-SEEDED
instance (Lorian + Riya + both avatars) vs the Rust service. Diffs the
result shape (deletedCharacterCount / remappedIdCount / preserved+postReset
present / postReset ≠ preserved / imported {2,42}), the post-state counts
(2 chars, 42 memories, 2 avatar links, 2 chars-with-avatar), and the
normalized post-reset characters + memories rows. (Cascade delete / import
/ avatar seed are each byte-proven separately — this pins the composition +
the two pure helpers + the result shape.)

**Fixture change:** the shared qtap-import fixture builder now materializes
the tables the cascade delete touches (`chats` / `files` /
`character_plugin_data` via `ensureCollection`, `vector_indices` /
`vector_entries` via a `VectorIndicesRepository` read) — the Rust port
never issues DDL, so they must pre-exist (v4 auto-creates them lazily).
Harmless extra empty tables for the plain-import + avatar oracles (never
dumped); all three oracles regenerated. The reset oracle also seeds the
single `users` row (the v4 auth middleware needs `users.findById`; the Rust
reset reads characters by userId directly and needs none).

**⚠️ UNIFICATION WIRE (deferred, loud):** the `reset-builtins` DISPATCH arm
— the `Request::CharactersResetBuiltins`-style variant in `api/types.rs`
(lane A's file this round) + the `api/characters.rs` dispatch arm calling
`reset_builtins` — is NOT in this lane. The `pub` service + its differential
are delivered; wire the dispatch at unification (the
`[[p4.6i-characters-remainder-server]]` photo_link_summary precedent). Also
flip `HostConfig::seed_sample_content` to default-ON and update the
fresh-boot e2e fixtures at unification.

Regen recipe (jest, from the v4 checkout — /tmp mirror, cwd = v4 root):
```
N=~/.nvm/versions/node/v24.13.1/bin ; V5W=<worktree>
TMPO=/tmp/qt-reset-builtins-oracle
rm -rf "$TMPO"; mkdir -p "$TMPO/cases" "$TMPO/fixtures" "$TMPO/lib"
cp "$V5W/harness/oracle/cases/reset-builtins.test.ts" "$TMPO/cases/"
cp "$V5W/harness/oracle/fixtures/qtap-import-tier2.json" "$TMPO/fixtures/"
cp "$V5W/harness/oracle/lib/tier2.ts" "$TMPO/lib/"
cd ~/source/quilltap-server
QT_FIXTURE_QTAPIMPORT_MAIN=/tmp/qt-qtapimport-main.db \
QT_FIXTURE_QTAPIMPORT_MOUNT=/tmp/qt-qtapimport-mount.db \
QT_ORACLE_OUT=/tmp/oracle-reset-builtins.ndjson \
  $N/npx jest --silent --watchman=false --testTimeout=120000 \
    --roots "$PWD" --roots "$TMPO/cases" -- reset-builtins
# run: QT_ORACLE_RESET_BUILTINS + QT_FIXTURE_QTAPIMPORT_{MAIN,MOUNT} \
#   cargo test -p quilltap-harness --test reset_builtins_equivalence
```

**P4.4u4 lane C COMPLETE** (units 1–3): the quilltap-import seed subset +
the startup seed wire + reset_builtins, all differential-proven. Versions
after the lane: core 0.0.179, harness 0.0.164, host 0.0.11. Deferred to
unification: the reset-builtins dispatch wire; the
`seed_sample_content` default-on flip + e2e fixture activation. Both
characters-family orders' (P4.6f/P4.6i) `reset-builtins` deferral is now
UNBLOCKED (the service exists; only the dispatch wire remains).

## P4.6n (lane A) unit 1 — scenarios-common service surface (2026-07-11, in progress)

Ported the write/list half of v4's `lib/mount-index/scenarios-common.ts`
into `quilltap-core::db::scenarios` (the module already carried the
read-only `resolveScenarioBody` slice): `parse_scenario_doc`,
`list_scenarios_in_folder`, `read_scenario_by_path`,
`set_scenario_default_in_folder`, `build_scenario_file_content`,
`resolve_scenario_path`, plus the `ParsedScenario` DTO
(`description` omitted-when-absent) and `ListScenariosResult`.

Carried v4's why-comments: the alphabetically-first `rawIsDefault`
default-conflict winner (losers demoted in the RESPONSE only, on-disk
frontmatter untouched, warning naming winner+offenders); the empty-body
drop in `parse_scenario_doc`; the deliberate NO-transaction sequenced
set-default (partial failure → alphabetical-tiebreaker + soft warning);
`resolve_scenario_path`'s nested-path rejection. **Boundary seam:** v4's
`resolveScenarioPath` `decodeURIComponent`s the raw Next.js segment;
v5's dispatch delivers an already-decoded JSON string, so no decode is
applied (same seam as the existing `resolve_scenario_body`).

Supporting reads added to `doc_mount_documents`: `VaultFolderDoc` gained
a `last_modified` field (`l.lastModified`, the DTO's `lastModified` —
purely additive; the vault overlay's per-file parsers ignore it) and the
folder query selects it; a new `find_with_link_by_mount_point_and_path`
returns the full joined `DocMountDocumentWithLink` shape `readScenarioByPath`
consumes (case-insensitive path compare, like `findByMountPointAndPath`).

Helper ports (the list-files legacy branch, unit 5): `mime::detect_mime_type`
+ `EXTENSION_MIME_MAP` (v4 `scanner.ts`) and
`folder_utils::{normalize_folder_path, derive_folder_path_from_storage_key,
resolve_effective_folder_path}` (v4 `folder-utils.ts`).

**Differential:** the pure leaves (`resolve_scenario_path`,
`build_scenario_file_content`, `parse_scenario_doc`) + the mime/folder
helpers carry tier-1 unit tests transcribed from v4's logic; the composed
list/read/set-default surface is proven byte-for-byte by the
`scenarios_routes_equivalence` differential landing with the route arms
(units 2–4). Core 0.0.176 → 0.0.177.

## P4.6n (lane A) unit 2 — Groups scenarios + participant-union (2026-07-11, in progress)

Made the 7 groups-scenario refusal arms live. New `api::scenarios` module
holds the **shared** mount-scoped CRUD every family composes: validate the
scenario bag (the Zod schemas duplicated across v4's route files — the
empty-`body` custom message `Scenario body cannot be empty` is byte-exact),
sanitise+strip the filename, the collision guard, `buildScenarioFileContent`
→ `writeDatabaseDocument`, the isDefault demotion via
`setScenarioDefaultInFolder`, and the fresh re-list. `api::groups` resolves
the group's official store (v4 ensures BOTH `Scenarios/` and `Knowledge/`):
`ensure_group_scenarios_store` (collection routes, provisions) vs
`load_group_scenarios_store` (item routes, RAW FK, 404 "…no official store
yet"). Added `Response::Scenario`.

`group_scenarios_union`: re-resolves every requested character id through
the user-scoped `characters_read::find_by_id` BEFORE the unscoped membership
table (the security invariant / why-comment), collects distinct groups,
skips zero-scenario groups, catches per-group failures, and sorts by
`groupName.localeCompare` (ICU4X en-US). The ONE sanctioned exception to
Groups' per-responding-character isolation.

**Differential:** `scenarios_routes_equivalence` (new) — 18 groups cases
over the extended fixture: list, get, get-missing(404), get-nested(400),
create(+sanitisation `:`→`_` +isDefault demotion, ts-blanked), collision(400),
update(ts-blanked), empty-body(400), update-missing(404), rename, rename-noop,
rename-conflict(400), delete, delete-missing(404), and union
{aria→[Beacon,Gamma] sorted+Zephyr-skipped, bram→[Gamma], empty→[], unknown→[]}.
Reads/rename/delete keep the baked fixture timestamps (rename/delete mint
none — `move` touches the LINK's updatedAt, not the DTO's d.createdAt/updatedAt/
l.lastModified); create/update blank them. Error arms compared as HTTP
status + message (v4 `{error}` vs the dispatch `Error{kind,message}`).
v4's `?action != rename → 400` has no v5 dispatch equivalent (intentionally
out of the differential). Core 0.0.177 → 0.0.178, harness 0.0.161 → 0.0.162.

Regen recipe (Node 24 @ `~/.nvm/versions/node/v24.13.1/bin`, from the v4
checkout, `/tmp` mirror; jest ignores `.claude/`):
```
TMPO=/tmp/qt-gp-oracle; W=<worktree>
cp $W/harness/oracle/cases/scenarios-routes.test.ts $TMPO/cases/
cp $W/harness/oracle/fixtures/groups-projects.json $TMPO/fixtures/
cd ~/source/quilltap-server
QT_FIXTURE_GP_MAIN=$W/crates/quilltap-web/tests/fixtures/groups-projects-main.db \
QT_FIXTURE_GP_MOUNT=$W/crates/quilltap-web/tests/fixtures/groups-projects-mount.db \
QT_ORACLE_OUT=/tmp/oracle-scenarios-routes.ndjson \
  $N/npx jest --silent --watchman=false --testTimeout=120000 \
    --roots "$PWD" --roots "$TMPO/cases" -- scenarios-routes
# then: QT_ORACLE_SCENARIOS_ROUTES=/tmp/oracle-scenarios-routes.ndjson cargo test -p quilltap-harness --test scenarios_routes_equivalence
```

## P4.6n (lane A) unit 3 — Projects scenarios (2026-07-11, in progress)

Made the 6 projects-scenario refusal arms live, mirroring unit 2 over the
shared `api::scenarios` CRUD. Two family differences: projects ensure ONLY
`Scenarios/` (no `Knowledge/`), and the store resolvers use
`ProjectsRepository::find_by_id` (overlay, for the collection routes' name →
`ensure_official_store::<ProjectEntity>`) vs `find_official_mount_point_id_raw`
(the item routes' RAW FK, 404 "…no official document store yet"). The
projects GET envelope drift (bare `NextResponse.json` vs `successResponse`) is
a no-op — both are `NextResponse.json(body, {status})`, so the body shapes are
identical (verified). There is no projects participant-union.

**Differential:** `scenarios_routes_equivalence` extended with 12 projects
cases over Iota (opening[default], climax): list, get, get-missing(404),
get-nested(400), create(+isDefault demotion, ts-blanked), collision(400),
update(ts-blanked), empty-body(400), rename, rename-conflict(400), delete,
delete-missing(404). Core 0.0.178 → 0.0.179, harness 0.0.162 → 0.0.163.

## P4.6n (lane A) unit 4 — General (instance-wide) scenarios (2026-07-11, in progress)

Added the general family. Six NEW `Request` variants (`ScenarioList`,
`ScenarioCreate {scenario}`, `ScenarioGet/Update/Rename/Delete {scenarioPath …}`
— the bare `scenario*` names from the shared contract) + the `api::scenarios`
general handlers, which resolve the singleton "Quilltap General" store from
`instance_settings.generalMountPointId` (`general_mount_id` — read the pointer,
ensure `Scenarios/`) and reuse the same shared CRUD as groups/projects.

The pre-provision RACE ARMS survive: GET → `{mountPointId:null, scenarios:[],
warnings:[]}`; POST → 400 "Quilltap General mount has not been provisioned yet —
restart the server"; the item routes → 404 "Quilltap General mount has not been
provisioned yet" (the `not_found` helper's " not found" suffix matches v4's
`notFound(msg)` byte-for-byte).

**Differential:** `scenarios_routes_equivalence` extended with 13 general
cases — list (the **default-conflict warning**: aurora & dusk both raw-default
→ aurora wins, dusk demoted-in-response, warning names both), get, get-missing(404),
create(+isDefault demoting BOTH siblings, ts-blanked), collision(400),
update(ts-blanked), rename (baked, the raw-default state travels with the move →
warning persists), delete (last conflict gone → no warning), and the three race
arms (`instance_settings.generalMountPointId` deleted on BOTH sides via
`unprovisionGeneral` / `db.write_blocking`). The whole surface is now live —
43 scenario cases total (18 groups + 12 projects + 13 general). Retired the
`scenarios::not_available` refusal helper. Core 0.0.179 → 0.0.180,
harness 0.0.163 → 0.0.164.

## P4.6n (lane A) unit 5 — project list-files + add/remove (2026-07-11, in progress)

Made the 3 project-file refusal arms live. `project_file_list` is the
two-branch v4 `handleListFiles`: **Branch A** — when `getProjectDocumentStore`
resolves a primary store (`project_doc_mount_links` → `find_store_naming_by_id`
→ `pick_primary_project_store`), list that store's `doc_mount_files` via
`DocMountFileLinksRepository::find_by_mount_point_id` (LinkRow, natural rowid
order = v4's `queryLinks`), DTO keyed `mimeForMountFile(fileType, fileName)`
(the `blob` case → `mime::detect_mime_type`), `category` from the mime prefix,
`updatedAt = lastModified || updatedAt` (lastModified always set; LinkRow has no
separate updatedAt — documented seam), explicit null description/folderPath/
width/height. **Branch B** — `files.findAll().filter(projectId===)` via
`find_by_project_id_for_listing` (rowid order), `folderPath` from
`resolve_effective_folder_path(folderPath, storageKey)`, and null
description/width/height DROPPED (the file read marshals them away).

`project_file_add`: ownership → `files.findById` (404 'File') →
`files.update(fileId, {projectId})`. `project_file_remove`: ownership →
`clear_project_id` (a guarded raw `SET projectId = NULL` — `FileUpdate.project_id:
Option<String>` can't express NULL). Both answer `{success:true}`.

**Differential:** `projects_routes_equivalence` extended — `list_files_iota`
(Branch A, 11 store files incl. the seeded logo.png/scenarios/wardrobe, baked
committed ts → no blank), `list_files_lambda` (Branch B, 2 legacy files,
folderPath `/sub/`, width/height only on the png), `list_files_kappa` (empty),
`add_file` (+ the file-row projectId dump), `add_file_missing` (404 File),
`remove_file` (+ dump). Helper ports (`mime`, `folder_utils`) landed in unit 1.

**P4.6n surface COMPLETE — no tier-3 refusal arms remain in the scenarios/
list-files family.** Core 0.0.180 → 0.0.181, harness 0.0.164 → 0.0.165.

## The P4.6n ∥ P4.6o ∥ P4.4u4 scenarios+import round — UNIFIED on main (2026-07-11)

Three lanes cherry-picked onto `unify/p4.6no-4u4` (lane A's six scenario/
list-files commits, lane C's three import commits, lane B's five SPA
commits; conflicts only on version files + the CHANGELOG/status-log
unions — the whole-file Cargo.toml check found NO dependency drift this
round). v4 baseline re-verified at `a7b1398d` before unification.

**The unification wires (one commit):**

- **The A↔B scenario contract diffed name-for-name and field-level:**
  19 request variants (7 group + 6 project + 6 general) + the opaque
  create/update `scenario` bag + `newFilename` + `characterIds` —
  identical on both sides, and `dispatchData` is response-tag-agnostic,
  so lane B's "richer DTO" re-pin needed NO server-side reconciliation.
  The first round since P4.6i with zero contract drift at unification.
- **The reset-builtins dispatch wire went in at the WEB EDGE, not as a
  core dispatch arm:** `POST /api/v1/characters?action=reset-builtins`
  (`quilltap-web::characters_routes`) runs the differential-proven
  `quilltap_import::reset::reset_builtins` inside one `Db::write` with
  `HostImageCodec`, echoing v4's response shape (the full 11-key
  QuilltapExportCounts, zeros for unported kinds). Rationale: core has
  no pixel-codec seam — every codec-needing leg lives at the edge (the
  P4.6m precedent). A route-level roundtrip test covers the fresh-import
  round and the delete-and-reimport round; the second round asserts
  post-reset ids DIFFER from "preserved" ids (v4's create-mints quirk
  the `reset_builtins` differential pinned — the remap is vestigial).
- **`HostConfig::seed_sample_content` flipped to default ON** (v4
  parity — its startup seeding is unconditional). Fallout: exactly one
  test — `host_builtin_seeds` pins the 3 built-in mount stores and now
  opts out (the seed adds the two character vaults' stores on top);
  the quilltap-web fixture boots are unaffected (their fixtures have
  characters, so the zero-characters gate short-circuits).
- **Lane B's fixture-guarded e2e beats self-activated** — the specs
  gate on `existsSync(groups-projects-main.db)`, which lane A's commit
  satisfies; no spec edits were needed.

**Mid-gate ENOSPC (worse than the standing recipe's case):** the disk
hit literal zero — even Bash was blocked (the harness couldn't create
its own output file). Recovery: the Monitor tool still launched (its
plumbing survived), and one Monitor-run `rm -rf` of the three
already-cherry-picked lane worktrees' `target/` dirs + the main
`target/debug/incremental` freed 55+ GB. Recipe note: when Bash itself
is ENOSPC-blocked, Monitor is the escape hatch.

**The full gate:** `cargo fmt --check` clean; clippy `-D warnings`
clean on the default set AND `--features
quilltap-core/native-transport`; release build clean; the six round
oracles regenerated FRESH from v4 at `a7b1398d` (groups-routes 14,
projects-routes 39, scenarios-routes 41, qtap-import, seed-avatars,
reset-builtins) and every differential re-run green BY NAME;
`cargo test --workspace` green (289 suites, 1,243 tests, 0 failed);
`ng test` green (256); `ng build` clean; the FULL Playwright suite
green (19/19), incl. the newly-activated scenarios walk (project card
create→edit→default→rename→delete + the general page) and the
wardrobe beat.

**Gate fallout — three fixes, all assertion/gesture class (no product
bugs):** (1) the seed default-ON flip moved the truthful post-setup
state, so `setup_flow_end_to_end` (web `contract.rs`) now asserts the
v4-parity fresh boot — 2 characters, 42 memories, **5** mount stores
(3 built-ins + the pair's vaults) — and `host_builtin_seeds` opts out
of the seed (it pins the 3 built-in mounts); (2) the P4.6o wardrobe
rows' "Edit" buttons broke the projects-flow walk's bare
`getByRole('Edit')` — scoped to `qt-project-header` (the standing
later-cards-duplicate-names gotcha); (3) the scenarios walk's row
actions timed out inside the dense project-card grid — `qt-scenario-row`
is container-query adaptive, so a narrow container renders the `⋮`
kebab instead of inline buttons; the spec gained a `rowAction` helper
(inline-if-visible, else kebab → `menuitem`). **Flake note:** one
contended full run (concurrent with `cargo test --workspace`) failed
groups-flow's create-dialog beat; it passes in isolation and in the
final uncontended full run — watch it, don't chase it yet.

**Orders:** P4.6n CLOSED, P4.6o CLOSED, P4.4u4 CLOSED — and they close
P4.6k (its scenarios/list-files remainder), P4.6l (its
Scenarios/Wardrobe cards), and P4.4u3's family-3 deferral +
the characters family's reset-builtins deferral (P4.6f/P4.6i).
**The groups/projects/scenarios surface has NO remaining refusal
arms.** Versions after the round: core 0.0.184, harness 0.0.168,
host 0.0.12, web 0.0.12, SPA 0.5.22.

## P4.6p — Listing-surfaces server (lane A, in progress)

Lane A of the P4.6p ∥ P4.6q ∥ P4.6r round (New-Chat + listing surfaces).
v4 baseline `a7b1398d` (drift-check clean at lane start). Own branch/worktree;
unification is separate. Closes the three P4.6l "unported listing surfaces"
(global mount-points / roleplay-templates / image-profiles disabled pickers).

### P4.6p unit 1 — `generateRenderingPatterns` (tier-1 pure) — LANDED

Ported v4 `lib/chat/annotations.ts` `generateRenderingPatterns` (+ the private
helpers `delimiterToPrefixSuffix` / `addOnClassesFor` / `composeClassName` /
`buildDelimiterPattern` / `buildWrapPattern`) into the new pure module
`crates/quilltap-core/src/services/annotations.rs`. It consumes the P4.4u3
`db::roleplay_templates` marshaling types (`TemplateDelimiter` /
`RenderingPattern` / `DelimiterAddOns` / `StringOrPair`). Pure string generation
— emits regex *source* strings, never compiles a regex — so it's a tier-1 EXACT
differential. Covers all three delimiter kinds, add-on class composition, the
same-open/close negative-lookaround, the `]`-suffix markdown-link exclusion, the
empty-suffix degrade-to-line-prefix defensive branch, the kind-tagged dedupe
key, and the narration append (string + tuple, covered-vs-uncovered).

Differential `annotations_rendering_patterns_equivalence` (25 cases). The corpus
input (delimiters + narrationDelimiters) rides the NDJSON so both sides run
identical input; output diffed byte-for-byte (object keys order-insensitive,
array order preserved).

Regen recipe (Node 24, from the v4 checkout):
```
cd ~/source/quilltap-server
~/.nvm/versions/node/v24.13.1/bin/npx tsx \
  <worktree>/harness/oracle/cases/annotations-rendering-patterns.ts \
  > /tmp/oracle-annotations.ndjson
QT_ORACLE_ANNOTATIONS=/tmp/oracle-annotations.ndjson \
  cargo test -p quilltap-harness --test annotations_rendering_patterns_equivalence
```

Versions: core 0.0.185, harness 0.0.169.

### P4.6p — fixture extension (the shared groups-projects fixture)

Extended `build-groups-projects-fixture.ts` with the listing-surfaces rows, all
ADDITIVE and invisible to the existing groups/projects/scenarios reads (they
enumerate none of these tables; DIANA/MP_INDEXED are referenced by no
group/project):
- roleplay: `seedBuiltInTemplates()` (Standard + Quilltap RP, ids minted &
  baked — resolved by name in the test) + RT_USER_1 (all three delimiter kinds,
  an addOns-bearing wrap, renderingPatterns PRESENT) + RT_USER_2 (delimiters +
  EMPTY patterns → the GET read-time regen).
- tags: TAG_A..D; DIANA character tagged [TAG_B, TAG_C] (the image
  sortByCharacter case — a NEW character so Aria/Bram/Cleo stay pristine).
- image profiles: IP_1 (default, apiKeyId=APIKEY, tags [TAG_A]), IP_2 (tags
  [TAG_B, TAG_C] → 2 matching DIANA), IP_3 (plain).
- MP_INDEXED "Indexed Store" mount + one embedded chunk (4-byte Float32 BLOB) so
  the LIST GROUP-BY count and the GET-[id] hydrate-and-filter count both see a
  non-zero.

Regenerated the three dependent oracles (groups 14 / projects 39 / scenarios 41)
and re-ran their Rust differentials — all green, confirming zero perturbation.

Regen recipe (Node 24, from the v4 checkout):
```
W=<worktree>; N=~/.nvm/versions/node/v24.13.1/bin
QT_FIXTURE_GP_MAIN=$W/crates/quilltap-web/tests/fixtures/groups-projects-main.db \
QT_FIXTURE_GP_MOUNT=$W/crates/quilltap-web/tests/fixtures/groups-projects-mount.db \
  $N/node --import tsx $W/harness/oracle/fixtures/build-groups-projects-fixture.ts
# then regen groups/projects/scenarios oracles (see their .test.ts headers)
```

### P4.6p unit 2 — roleplay-templates dispatch surface — LANDED

Ported v4's `roleplay-templates/route.ts` + `[id]/route.ts` as
`api::roleplay_templates` (five variants: list / create / get / update / delete),
composed over the ported `db::roleplay_templates` repo + three new full-JSON
route reads (`find_full_json_by_id`, `find_all_for_user`,
`find_id_by_user_and_name`) and the pure `generate_rendering_patterns`.

**ErrorKind widened:** added `Forbidden` (403) + `Conflict` (409) — v4's
`responses.ts` vocabulary the built-in guards / duplicate-name arms require.
Updated the two exhaustive `quilltap-web` ErrorKind→StatusCode matches
(dispatch.rs, characters_routes.rs) + the three harness `http_for` test helpers.
**Bumped quilltap-web** (0.0.13) — the only lane-A touch outside
`quilltap-core::api`; noted for the unifier.

Pinned v4 quirks (oracle-observed, ported faithfully):
- LIST is a **bare JSON array**, built-in-first then `name.localeCompare`.
- Route-read marshaling omits null nullables (v4 backend hydrateRow: null →
  undefined → Zod-omitted). `narrationDelimiters` is NOT a registered JSON
  column (heterogeneous union → 'unknown'), so a stored `[open,close]` pair
  reads back as the **raw JSON string**.
- GET-[id] regenerates empty renderingPatterns on the fly (non-persisted).
- CREATE/UPDATE return the in-memory `validate(entityInput)` (not a DB re-read):
  create's `narrationDelimiters` echoes the input verbatim (array stays array),
  description/dialogueDetection present as null-or-value.
- **PUT `updateData` ALWAYS sets name/description/systemPrompt** to
  `validatedData.field?.trim()` (undefined when absent), which the `_update`
  merge overwrites → the full `RoleplayTemplateSchema.parse` re-validate then
  REQUIRES name+systemPrompt (a partial body omitting either → **400 Validation
  error**), and every successful update **drops `description`** unless a string
  is supplied. The dup-name 409 precedes this 400. The 400's Zod `details[]` are
  dropped by the Response error envelope (message-only) — status+message match.

Differential `roleplay_templates_routes_equivalence` (21 cases): list / 3 gets
(user / read-regen / built-in) / 404 / 2 creates (auto-regen; narration-array) /
4 create-validation arms / dup 409 / update happy / built-in 403 / dup 409 / 2
regen arms / null-description / missing-required 400 / delete / built-in 403 /
404. Built-in ids (minted at fixture build) resolved by name on both sides.

Regen recipe (Node 24, from the v4 checkout):
```
W=<worktree>; N=~/.nvm/versions/node/v24.13.1/bin; TMPO=/tmp/qt-gp-oracle
mkdir -p $TMPO/cases $TMPO/fixtures
cp $W/harness/oracle/cases/roleplay-templates-routes.test.ts $TMPO/cases/
cp $W/harness/oracle/fixtures/groups-projects.json $TMPO/fixtures/
cd ~/source/quilltap-server
QT_FIXTURE_GP_MAIN=$W/crates/quilltap-web/tests/fixtures/groups-projects-main.db \
QT_FIXTURE_GP_MOUNT=$W/crates/quilltap-web/tests/fixtures/groups-projects-mount.db \
QT_ORACLE_OUT=/tmp/oracle-roleplay-templates-routes.ndjson \
  $N/npx jest --silent --watchman=false --testTimeout=120000 \
    --roots "$PWD" --roots "$TMPO/cases" -- roleplay-templates-routes
QT_ORACLE_ROLEPLAY_ROUTES=/tmp/oracle-roleplay-templates-routes.ndjson \
  cargo test -p quilltap-harness --test roleplay_templates_routes_equivalence
```

Versions: core 0.0.186, harness 0.0.170, web 0.0.13.

### P4.6p unit 3 — image-profiles dispatch surface — LANDED

Ported v4's `image-profiles/route.ts` + `[id]/route.ts` as `api::image_profiles`
(list [+ `?sortByCharacter=`] / create / get / update / delete +
`imageProviderList`; the three LLM/IO actions are loud refusal arms), composed
over the ported repo + new reads (`find_by_user_id`, `find_id_by_user_and_name`,
`unset_all_defaults` [mints updatedAt on flipped rows], and the
nullable-clearing `IpUpdate` tri-state `Option<Option<String>>` for
apiKeyId/baseUrl), the api-key + tag enrichment (`{id,label,provider,isActive}`,
`[{tagId, tag}]`), and the manifest `Registry`.

**Manifest change:** added an optional `imageGenerationModels` field (serde
default, `deny_unknown_fields` absent → backward compatible) to the `Manifest`
struct + the five image-capable manifests (openai/google/grok/z_ai/openrouter),
byte-exact-transcribed from v4's plugin `getImageGenerationModels()`/
`supportedModels` so `list-providers`' `defaultModels` matches. No other manifest
consumer reads it; the 19 provider_manifest unit tests + providers_listing stay
green.

Pinned quirks: list = default-first then createdAt DESC; `?sortByCharacter=`
re-sorts by matching-tag count DESC with v4's tie-break `b.isDefault?1:a.isDefault?-1:0`
and appends `matchingTags`/`matchingTagCount`; create returns the in-memory
validated object + `apiKey` (apiKeyId/baseUrl present-as-null, tags raw `[]`);
GET/list/update read-marshal omits null nullables then enriches; PUT is per-field
`!== undefined` gated (apiKeyId tri-state clear, `''`→null baseUrl,
isDefault→unset-others, tags NOT updatable).

**Provider-probe gotcha (banked):** v4's `createImageProvider` registry probe is
a NO-OP in the jest sandbox — the plugin registration doesn't reach
`requireProvider`, so it accepts EVERY provider there (standalone Node and
production both reject unknown/non-image). So the reject path can't be
oracle-tested; it's covered by a core unit test (`probe_matches_v4_production`:
accepts the 5 image providers + the GOOGLE_IMAGEN alias, rejects
unknown/non-image/wrong-case), and the ACCEPT path is proven end-to-end by
`create_happy`. `create_provider_unavailable` was dropped from the differential
for this reason. Oracle registration recipe: the jest oracle loads the 9 plugin
dists (`plugins/dist/qtap-plugin-*`) and `registerProvider`s them per case (needs
`@jest-environment node` for `ReadableStream`), which populates `getAllProviders`
(so `list-providers` returns the real 5) even though the probe stays lenient.

Differential `image_profiles_routes_equivalence` (18 cases + a refusal-arm test).
Regen recipe (Node 24, from the v4 checkout):
```
W=<worktree>; N=~/.nvm/versions/node/v24.13.1/bin; TMPO=/tmp/qt-gp-oracle
mkdir -p $TMPO/cases $TMPO/fixtures
cp $W/harness/oracle/cases/image-profiles-routes.test.ts $TMPO/cases/
cp $W/harness/oracle/fixtures/groups-projects.json $TMPO/fixtures/
cd ~/source/quilltap-server
QT_FIXTURE_GP_MAIN=$W/crates/quilltap-web/tests/fixtures/groups-projects-main.db \
QT_FIXTURE_GP_MOUNT=$W/crates/quilltap-web/tests/fixtures/groups-projects-mount.db \
QT_ORACLE_OUT=/tmp/oracle-image-profiles-routes.ndjson \
  $N/npx jest --silent --watchman=false --testTimeout=120000 \
    --roots "$PWD" --roots "$TMPO/cases" -- image-profiles-routes
QT_ORACLE_IMAGE_ROUTES=/tmp/oracle-image-profiles-routes.ndjson \
  cargo test -p quilltap-harness --test image_profiles_routes_equivalence
```

Versions: core 0.0.187, harness 0.0.171.

### P4.6p unit 4 — global mount-points dispatch surface — LANDED (tier 2)

Ported v4's `mount-points/route.ts` + `[id]/route.ts` as `api::mount_points`
(list / get / create / patch / delete-cascade), composed over the ported
`db::doc_mount_points` repo + the new `find_all_full_json` (unscoped global read),
the two embedded-count reads (`count_embedded_by_mount_point_ids` cheap GROUP-BY
`IS NOT NULL`; `count_nonempty_embeddings_by_mount_point_id` expensive
`IS NOT NULL AND length(embedding) > 0`), and the pure `derive_mount_capabilities`
(inline; unit-tested). Whole family is mount-index-partitioned (reads via
`read_mount_index`, writes on the mount-index writer).

Pinned quirks: LIST = `{mountPoints}` (createdAt DESC, cheap count, no
capabilities); GET-[id] = `{mountPoint:{…, embeddedChunkCount(expensive),
capabilities}}`; create returns the in-memory validated DocMountPoint (the three
nullables present as null) + optional `warning`; PATCH's whole-handler try/catch
→ a bad body 500s `Failed to update mount point` (NOT 400) and the echo omits
count/capabilities; DELETE runs the exact ordered cascade (chunks → files [+ the
orphan-file GC via the link snapshot] → documents → blobs → folders →
project-links → the point). The 12 action verbs + semantic-search get NO variants
(D7 — the Scriptorium/file-manager surface).

Seams: `verifyBasePath` and the watcher (attach/detach/refresh) are injected —
a non-database create always warns (deterministic); the oracle drives a
nonexistent basePath so v4's real `verifyBasePath` also returns false → both
produce the warning string. The character-scaffold arm (create + PATCH-flip)
calls the ported `scaffold_character_mount`; both verified via a folder-path dump
(7 folders: Outfits/Prompts/Scenarios/Wardrobe/files/images/lore). The full
cascade verified via a per-table count dump (all 0 + mountPointExists false).

**Oracle gotcha (banked):** the mount-points route imports the watcher, which
pulls **chokidar (ESM)** — jest CJS chokes on it. The oracle `jest.doMock`s
`@/lib/mount-index/watcher` to no-op seams so chokidar never loads (verifyBasePath
in `scanner` stays real). Needs `@jest-environment node`.

Differential `mount_points_routes_equivalence` (13 cases). Regen recipe (Node 24,
from the v4 checkout):
```
W=<worktree>; N=~/.nvm/versions/node/v24.13.1/bin; TMPO=/tmp/qt-gp-oracle
mkdir -p $TMPO/cases $TMPO/fixtures
cp $W/harness/oracle/cases/mount-points-routes.test.ts $TMPO/cases/
cp $W/harness/oracle/fixtures/groups-projects.json $TMPO/fixtures/
cd ~/source/quilltap-server
QT_FIXTURE_GP_MAIN=$W/crates/quilltap-web/tests/fixtures/groups-projects-main.db \
QT_FIXTURE_GP_MOUNT=$W/crates/quilltap-web/tests/fixtures/groups-projects-mount.db \
QT_ORACLE_OUT=/tmp/oracle-mount-points-routes.ndjson \
  $N/npx jest --silent --watchman=false --testTimeout=120000 \
    --roots "$PWD" --roots "$TMPO/cases" -- mount-points-routes
QT_ORACLE_MOUNT_ROUTES=/tmp/oracle-mount-points-routes.ndjson \
  cargo test -p quilltap-harness --test mount_points_routes_equivalence
```

Versions: core 0.0.188, harness 0.0.172.

## P4.6q (lane B) — the New-Chat SPA vertical (2026-07-12, in progress)

Lane B of the P4.6p ∥ P4.6q ∥ P4.6r round. Tier-4 (v4's React app is the
behavioral reference, not a byte target). Server side of chat creation is
FULLY LIVE (`Request::ChatCreate` + the Green-Room bus, both e2e-proven in
P4.4u2). Branch `claude/p4-6q-chat-spa-porting-7f01c1`. Drift check clean
(v4 HEAD == `a7b1398d`). Worktree `node_modules` symlinked to the main
checkout (identical `package.json`; the lane adds no npm deps).

### P4.6q unit 1 — core-contract.ts re-pins + the listing-surface appendix — LANDED

- **`ChatCreateRequest`** re-pinned from the flattened v4 `POST /api/v1/chats`
  body + the live `services/chat_create::ChatCreateRequest`: `title` +
  `participants` (`ChatCreateParticipantInput`), `imageProfileId`, the four
  scenario-source fields (precedence `scenarioId` > project > group > general),
  free `scenario`, `timestampConfig`, `projectId`, `avatarGenerationEnabled`,
  `outfitSelections` (`ChatCreateOutfitSelectionInput` + `OutfitSelectionMode`),
  `continuationFromChatId`, `progressId`, and the carried autonomous fields
  (deferred this round; kept for shape).
- **`ChatCreateDto`** re-pinned to the live `{ chat: { id, participants?, … } }`
  echo (`ChatCreateResultDto`), matching `data.chat.id` (the web e2e reads it).
- **`CreationProgressFrame`** re-pinned FROM `services/creation_progress.rs`:
  the `kind`-tagged Green-Room frame folded FLAT into `ScopedEvent` (`kind`,
  `message`, `level`, `characterId`, `characterName`, `slots`, `ts`) plus
  `OutfitPreviewSlots` / `OutfitPreviewEntry`. Kept flat (not a distributed
  union) so `ScopedEvent`'s chat-frame consumers are unaffected.
- **Listing-surface appendix** (BINDING, byte-identical B↔C): roleplay-template
  / image-profile / mount-point request variants + DTOs (transcribed from the
  p4.6p Shared contract + the live Rust DTOs `db/roleplay_templates.rs`,
  `db/image_profiles.rs`, `db/doc_mount_points.rs`), folded into `CoreRequest`
  via `ListingSurfaceRequest`. Lane B's own functional need from it is only
  `imageProfileList` (the picker); the rest is for the shared block + lane C.
- Gate: `tsc --noEmit` clean, `ng build` clean. SPA 0.5.22 → 0.5.23.

### P4.6q unit 2 — the new-chat state service + pure logic — LANDED

- **`new-chat.types.ts`** — the cast entry (`NewChatSelectedCharacter`), the form
  state (`NewChatFormState`, minus the deferred autonomous slice), the scenario
  option shapes, and the dropdown token constants (`USER_CONTROLLED_PROFILE`,
  `CUSTOM_SCENARIO_VALUE`, the three source prefixes).
- **`new-chat.logic.ts`** (pure, unit-tested) — `generateTitle`, `applyPlayAs`
  (the in-place Play-As transition), `applyProfileChange`, `scenarioSelectPatch`
  (the token → source-field patch that never touches free text), `sortRoster`,
  `seedSelectedCharacter`, and `buildCreateRequest` (the verbatim submit payload:
  booleans only-when-true, optionals absent, the scenario precedence chain,
  timestampConfig dropped when mode NONE).
- **`new-chat.state.ts`** — `NewChatState` (v4 `useNewChat`): the batched load +
  seeding precedence (pin 4), the single-LLM default propagation (v4's post-load
  effect, run imperatively on cast change), and `handleCreate` (the submit spine
  opening the Green Room before the dispatch; the dispatch resolving is the ready
  signal). Reuses the live reads `characterList` / `connectionProfileList` /
  `scenarioList` / `projectList` / `projectGet` / `projectScenarioList` /
  `characterGet` / `characterDefaultPartner` / `groupScenariosUnion`.
- **`green-room.types.ts`** — the dialog state + the `GreenRoomController` seam
  the submit spine drives (the concrete controller/reducer/dialog land in u6).
- **DECISION (recorded):** the group participant-union is fetched faithfully (v4
  `useNewChat` fetches it) but NEVER rendered — v4's `/salon/new` page destructures
  `projectScenarios`/`generalScenarios` but not `groupScenarios`, so the form's
  group optgroup is dead UI. Ported as a fetched-but-unrendered seam.
- **`new-chat.logic.spec.ts`** — Vitest transcription of v4's
  `NewChatForm.test.tsx` Play-As + scenario-layering behaviors + `generateTitle`
  + the payload precedence/only-when-true rules. Gate: `tsc` clean (app + spec),
  `ng test` green (280), `ng build` clean. SPA 0.5.23 → 0.5.24.

### P4.6q unit 3 — the character picker panel — LANDED

- **`character-picker-panel.ts`** (`qt-new-chat-picker`, v4 `CharacterPickerPanel`):
  the two-pane picker over `NewChatState`. Left: the searchable, `sortRoster`-sorted
  roster; right: the selected cast with the "Speaks First" badge (index 0) and the
  per-character connection-profile + system-prompt selects. The profile select's
  `Play As (User)` option (`USER_CONTROLLED_PROFILE`) flips the entry via
  `applyProfileChange`; select/remove resets `scenarioId` only (path selections
  survive) then defers to the state's single-LLM propagation. `[selected]`-per-option
  on both async selects (the dogfood-#6 class). Reuses `qt-avatar` +
  `characterAvatarSrc`.
- **`character-picker-panel.spec.ts`** — renders the roster, seeds an LLM entry on
  select (first profile), asserts the Play-As option. Gate: `tsc` clean, `ng test`
  green (283), `ng build` clean. SPA 0.5.24 → 0.5.25.

### P4.6q unit 4 — the Green Room (creation-progress dialog) — LANDED

- **`green-room.reducer.ts`** (pure, unit-tested) — `applyGreenRoomFrame`: folds one
  creation-progress frame (v4 `applyEvent`) into the dialog state — `status`/`log`
  (cap 100, drop oldest), the `wardrobe-start`/`wardrobe-result` upsert (keep a
  resolved outfit if a later frame lacks slots), `done` → "The players are ready.",
  `error` → "Something went awry."
- **`green-room.state.ts`** — `GreenRoomStore` (`@Injectable` root, v4
  `CreationProgressProvider`): on `begin(progressId)` subscribes to the ONE global
  `CoreClient.events$`, filters frames scope-tagged with that `progressId` + a
  creation `kind`, and folds them. No bespoke SSE route (D3/D6) — the server
  buffers/replays; our stream is already open. `complete()` closes; `fail(msg)`
  keeps it open with the error + Close button.
- **`outfit-slots-preview.ts`** (`qt-outfit-slots-preview`, v4 `OutfitSlotsPreview`)
  — the read-only four-slot outfit render.
- **`green-room-dialog.ts`** (`qt-green-room-dialog`, v4 `ChatCreationProgressModal`)
  — the blocking, non-dismissable dialog (no backdrop/Escape/✕); the error state
  offers the only Close. Copy verbatim: "The Green Room", "Fetching the players from
  the green room…", "The players are ready.", "Something went awry."
- **`green-room.reducer.spec.ts`** — the transitions + copy + the 100-cap. Gate:
  `tsc` clean, `ng test` green (292), `ng build` clean. SPA 0.5.25 → 0.5.26.

### P4.6q unit 5 — the form body + shared children — LANDED

- **`new-chat-form.ts`** (`qt-new-chat-form`, v4 `NewChatForm`): the Play-As select
  (with the `usePersonaDisplayName` duplicate-name disambiguation ported inline),
  the image-profile picker, the scenario dropdown (project/general/character
  sources + preview + layered notes), the outfit selector, the avatar toggle, the
  timestamp card, and the project row (picker/read-only). Autonomous toggle is a
  loud disabled-with-title deferral. The group optgroup is intentionally absent
  (v4's page passes no group scenarios — dead UI).
- **`image-profile-picker.ts`** (`qt-image-profile-picker`, v4 `ImageProfilePicker`)
  — self-fetching over `imageProfileList` (`sortByCharacter`/`sortByUserCharacter`),
  the lane-A live variant (coded against the pinned `{profiles, count}` shape; an
  error read yields the empty state until unification). `[selected]`-per-option.
- **`outfit-selector.ts`** (`qt-outfit-selector`) — per-character mode radios
  default / llm_choose (hidden for the user persona) / none; `manual` renders loudly
  disabled-with-title (the wardrobe-composer deferral); `previous_chat` (continuation)
  not rendered. Emits `{characterId, mode}`.
- **`timestamp-config-card.ts`** (`qt-timestamp-config-card`) — the compact "Reality
  Injection Mode" card (mode radios, interval, format, custom format, timezone +
  Detect, injection method, fictional time + base, compact info line). Selecting
  `EVERY_N_MINUTES` seeds a 15-min interval.
- **`new-chat-form.spec.ts`** — the scenario-layering render (preview + append hint +
  relabeled 'Additional scenario notes' editor) + the Play-As option listing,
  transcribed from v4's `NewChatForm.test.tsx`. Gate: `tsc` clean, `ng test` green
  (295), `ng build` clean. SPA 0.5.26 → 0.5.27.

### P4.6q unit 6 — the /salon/new route + page — LANDED

- **`app.routes.ts`** — added `salon/new` BEFORE `salon/:id` (which previously
  swallowed the path as `id="new"`; the dangling links from salon-list /
  project-chats / project-header now resolve).
- **`new-chat-page.ts`** (`qt-new-chat-page`, v4 `app/salon/new/page.tsx`): reads
  `?projectId=` / `?characterId=` / `?autonomous=1`, constructs `NewChatState`
  (with the `GreenRoomStore`), composes the picker + form + submit spine + the
  Green Room dialog, and navigates to `/salon/<id>` on success. Back link +
  project card + connection-profile warning + the load/pre-flight error banner.
  `?autonomous=1` → a loud not-yet-available notice; the page proceeds as an
  ordinary new chat (autonomous mode deferred). Gate: `tsc` clean, `ng build`
  clean (lazy `new-chat-page` chunk). SPA 0.5.27 → 0.5.28.

### P4.6q unit 7 — the Salon-list rider + the e2e beat — LANDED

- **`salon-list.ts`** — added the "New Chat" header affordance (routerLink
  `/salon/new`). The empty-state "Start a new chat" link and the
  project-chats/project-header links now resolve.
- **`e2e/new-chat-flow.spec.ts`** — the walk: unlock (tolerant) → Salon-list
  "New Chat" → `/salon/new` → pick the first roster character (its profile
  auto-seeds, "Speaks First" appears, Create enables) → Create → the URL lands
  on `/salon/<id>` with the streamed greeting (`MOCK_LLM_REPLY`) rendered. Runs
  against the committed salon fixture the global setup provisions (same recipe as
  `m4-salon.spec.ts`; starts the mock on `MOCK_LLM_PORT`). Discovered by
  `playwright test --list`; the live run is verified at unification (the Green
  Room assertion is best-effort — the dispatch resolving closes the dialog).
- Prettier-normalized the new-chat modules (they were committed unformatted in
  u1–u6; the repo does not enforce Prettier globally). Gate: `tsc` clean,
  `ng test` green (295), `ng build` clean, `playwright --list` discovers the beat.
  SPA 0.5.28 → 0.5.29.

**Lane B status:** all Tier-1 + Tier-2 deliverables landed. Deferrals (Tier 3,
loud): autonomous mode (disabled-with-title), manual outfit composition
(disabled-with-title), the continuation/"change of venue" entry (`NewChatModal`),
and the Lexical editor (plain-textarea divergence). Sibling dependency:
`imageProfileList` is lane A's live variant (mocked here; wired at unification).
The listing-surface appendix in `core-contract.ts` is the BINDING byte-identical
B↔C block.

## P4.6r (lane C) — Templates & Images settings SPA + picker enablement — LANE COMPLETE (branch, awaits unification) (2026-07-12)

Tier-4 SPA lane; v4 baseline `a7b1398d` (drift check clean at lane
start). Branch `claude/p4-6r-templates-images-spa-c11fc4`. Three
commits.

**What landed:**

1. **The Templates & Prompts tab** — the Roleplay Templates manager
   (`screens/settings/templates/`): the read-only built-in grid
   (Preview + Copy-as-New), My Templates (create / edit /
   delete-with-inline-confirm), the global Default Template selector
   (over the live `chatSettings` / `chatSettingsUpdate` surface writing
   `defaultRoleplayTemplateId`), the create/edit modal (name /
   description / LLM Prompt / narration single-or-pair), and the FULL
   Formatting Delimiters editor (wrap / linePrefix / tagPrefix, the
   style datalist, hideDelimiter, and the bold/italic/reverse/
   underline/border/font flourishes). renderingPatterns are omitted on
   save so the server regenerates. Duplicate-name 409 + built-in-guard
   403 surface verbatim in the banner.
2. **The Images tab** — the Image Profiles card
   (`screens/settings/images/`): the profile grid (Default / Uncensored
   badges, model + API-key metadata, the parameters summary,
   alphabetical-by-name sort per v4's card), the create/edit form modal
   (Profile Name; Provider select over the live registry with
   `FALLBACK_PROVIDERS`; API-key select filtered by provider; Model
   select over the provider `defaultModels`; a Parameters JSON textarea;
   isDefault + isDangerousCompatible), and delete-with-inline-confirm.
3. **The three pickers** — the project Model-Behavior roleplay-template
   picker, the project Image-Generation image-profile picker, and the
   character Defaults-tab image-profile picker are LIVE, binding the
   existing `defaultRoleplayTemplateId` / `defaultImageProfileId` fields
   over the per-field immediate-save flow. `[selected]`-per-option
   throughout (the dogfood-#6 async-option seeding).
4. **The reset-builtins rider** — "Reset Built-in Characters"
   (characters roster) is LIVE via a confirm dialog + result banner over
   the WEB-EDGE `POST /api/v1/characters?action=reset-builtins` route
   (live since P4.4u4), dispatched via `fetch` (the upload/PNG-rider
   precedent).

**Contract:** the listing-surface DTOs + Request interfaces
(`RoleplayTemplateDto` + the `TemplateDelimiter` union +
`RenderingPattern`; `ImageProfileDto` + `ImageProviderInfo`; the
`roleplayTemplate*` / `imageProfile*` / `imageProviderList` Request
interfaces) landed as the byte-identical B↔C **core-contract appendix
block** appended at the end of `core-contract.ts`. Lane C does NOT edit
the `CoreRequest`/`CoreResponse` unions (lane B owns) — the SPA data
layers dispatch the new variants through a localized
`as unknown as CoreRequest` seam (`templates.api.ts` /
`image-profiles.api.ts` `listingDispatch`). **Unification wires the
union + drops the cast.** Global mount-point variants are pinned in the
P4.6p contract but SPA-UNCONSUMED (Scriptorium vertical) — NOT declared
in lane C's appendix; the unifier keeps lane A's Rust mirrors / lane B's
block if it declared them.

**Tests:** pure helpers exact-unit-tested
(`template-form.spec.ts` delimiter/narration round-trip + omissions;
`image-profile-form.spec.ts` provider normalization + API-key
filtering); mocked-CoreClient CRUD specs
(`roleplay-templates-card.spec.ts` built-in guard + delete;
`image-profiles-card.spec.ts` default badge + sort + empty);
`project-model-behavior-card.spec.ts` async-option seeding + save. Two
stale project-card "disabled affordance" specs flipped to enabled;
`characters-list.spec.ts` reset-button assertion flipped to enabled.
`ng test` 289 (43 files) / `ng build` clean. e2e beats authored + fixture-guarded
(activate at unification over lane A's extended fixture):
`projects-flow.spec.ts` (the Model-Behavior template picker seeds +
persists) and a new `settings-flow.spec.ts` describe (Templates create→
edit→delete; Images card lists the fixture profiles). `playwright
--list` discovers all three.

**Deferred loudly (enumerated):** the image-profile Validate /
list-models buttons (variants refusal-armed — Validate renders
disabled-with-title, Model uses `defaultModels` only); the structured
per-provider parameters editor (a JSON textarea stands in — tier-2/8);
the template "Draft formatting instructions" helper
(`generateFormattingPromptHint` transcription); template/profile tag
pickers (tags vertical); the mount-points management UI (Scriptorium);
the group-stores / project-files browse buttons (file-manager). The
`systemPrompt` editor is a plain textarea (v4 uses a Markdown Lexical
editor — no rich editor in the SPA).

**Flags for unification:** (1) the global Default Template selector
writes `defaultRoleplayTemplateId` into the chat-settings row — confirm
P4.6d's `chatSettingsUpdate` accepts/round-trips that field (it is a v4
`settings/chat` field, so expected present). (2) v4's image-profiles
CARD sorts alphabetically by name (the SERVER returns default-first);
lane C matched the card (alphabetical) — the contract's "default-first"
refers to the server ordering. (3) the `roleplayTemplateList` envelope
is a BARE array; `fetchRoleplayTemplates` accepts array-or-wrapper
defensively — reconcile against lane A's real body. Versions after the
lane: SPA 0.5.25 (accumulated 0.5.23/24/25 across the three commits).

## The P4.6p ∥ P4.6q ∥ P4.6r listing-surfaces + New-Chat round — UNIFIED on main (2026-07-12)

Three lanes cherry-picked onto `unify/p4.6pqr` (lane A's five server
commits, lane B's seven New-Chat SPA commits, lane C's three
settings-SPA commits; conflicts on version files + the
CHANGELOG/status-log unions + the predicted core-contract appendix).
v4 baseline re-verified at `a7b1398d` before unification. The
whole-file version check found no dependency drift.

**The unification wires (one commit):**

- **The B↔C core-contract appendix DIVERGED as lane B's memory note
  predicted** — lane B folded the listing-surface variants into the
  `CoreRequest` union (`…Bag` bags, inline delimiter-union arms); lane
  C shipped `…Input` names behind a localized
  `as unknown as CoreRequest` cast seam. Reconciled to lane B's union
  fold: lane C's `templates.api.ts` / `image-profiles.api.ts` renamed
  to the `…Bag` types and the cast seams dropped (requests now
  typecheck through the union). Lesson: "byte-identical appendix in
  both lanes" needs the block committed BEFORE the lanes fork, or one
  lane named as the block's author with the other consuming it as a
  spec — prose alone drifted.
- **The A↔B/C contract diffed name-for-name against the Rust
  variants:** all 16 live variants match (5 roleplay + 6 image + 5
  mount). The three refusal-armed image-profile action interfaces had
  GUESSED shapes (`prompt`/`profileId`) — reconciled to the Rust
  mirrors (opaque `payload` / `provider`+`apiKeyId`).
  `sortByUserCharacter` annotated read-by-neither-server (v4's picker
  sends it; v4's route reads only `sortByCharacter`; serde ignores it
  — wire parity).
- **DTO nullability widened per lane A's oracle pins:**
  `RoleplayTemplateDto.description`/`dialogueDetection` →
  `| null` (route reads omit null nullables; the create/update
  echoes carry them AS null), and `ImageProfileCreateBag`
  apiKeyId/baseUrl accept explicit null (v4's `|| null` coercion).
  Lane C's `FALLBACK_PROVIDERS` literals gained the required
  `legacyNames`.
- Verified `defaultRoleplayTemplateId` is plumbed through
  chat_settings create/update (lane C's flag 1) — the live settings
  walk exercises it.
- Lane C's fixture-guarded beats self-activated (the groups-projects
  fixture predates the round, so the `existsSync` guards were always
  true — they were live-run for the first time at the gate; see
  fallout).

**The full gate:** `cargo fmt --check` clean; clippy `-D warnings`
clean on the default set AND `--features
quilltap-core/native-transport`; release build clean; the seven round
oracles regenerated FRESH from v4 at `a7b1398d` (annotations 25 /
roleplay-templates 21 / image-profiles 18 / mount-points 13 / groups
14 / projects 39 / scenarios 41) and every differential re-run green
BY NAME; `cargo test --workspace` green (293 suites, 1,250 tests, 0
failed); `ng test` green (328); `ng build` clean; the FULL Playwright
suite green (23/23), incl. the newly-live `new-chat-flow` walk
(unlock → Salon "New Chat" → pick → auto-seeded profile → Create →
`/salon/<id>` with the streamed greeting) and the P4.6r settings +
picker beats.

**Gate fallout — three fixes, all gesture/assertion class (no product
bugs):** (1) the Templates beat asserted visibility on
`qt-template-form-modal` — an Angular HOST with no box of its own
(the overlay child is position-fixed), so Playwright reports it
hidden while the dialog is fully rendered; assert on
`getByRole('dialog')` (NEW standing gotcha for modal hosts). (2)
Strict-mode: a created template's name also appears as an option
inside the Default Template selector `section.qt-card` — the template
cards are `div.qt-card`, scope by element type; and a
delete-confirm locator must not `filter({has: Delete})` on the card
whose Delete button the click just replaced with Confirm/Cancel.
(3) The projects picker beat reloads on a DETAIL page, but its
`unlockIfLocked` hard-coded the Projects LIST heading as the ready
signal — the helper gained the settings-flow ready-override param.

**Orders:** P4.6p CLOSED, P4.6q CLOSED, P4.6r CLOSED — closing the
three P4.6l "unported listing surfaces" picker gaps. **Still
refusal-armed after the round:** `imageProfileGenerate` /
`imageProfileValidateKey` / `imageProfileListModels` (the wire-seam
stretch did not land); the twelve mount-point action verbs +
semantic-search have NO variants (D7 — the Scriptorium surface).
Deferred loudly in the SPA: autonomous mode, manual outfit
composition, the continuation entry, the Lexical editors, the
Validate/list-models buttons, tag pickers, the mount-points
management UI. Versions after the round: core 0.0.188, harness
0.0.172, host 0.0.12, web 0.0.13, SPA 0.5.34.

## P4.6s (lane A) — the memories server surface — IN PROGRESS (branch) (2026-07-12)

### P4.6s unit 1 — the memories-web fixture + the read arms — LANDED (branch)

The NEW committed `memories-{main,mount}.db` fixture
(`harness/oracle/fixtures/build-memories-web-fixture.ts`, spec
`memories-web.json`), built via v4's REAL repos + the REAL builtin
TF-IDF vectorizer: FIXTURE_USER (`e18e05bc…`, the web e2e rewrites to
SINGLE_USER_ID), one connection + api key + a `chat_settings` row
(cheapLLMSettings → the connection), a BUILTIN **default** embedding
profile with a fitted `tfidf_vocabularies` row; three characters —
**Mnemo** (51 memories: 40 nautical fillers + tagged pair + near-dup
anchor + related pair + two swipe-source AUTO memories; 47 embedded via
`generateMissingEmbeddings`, 4 bare), **Orla** (4), **Pip** (0); two
tags (one with a visualStyle); a salon chat (USER + two swipe-sibling
ASSISTANT messages) and an autonomous chat (two ASSISTANT messages).

Gotchas banked while building the fixture + oracle:
- **`initializePlugins()` is REQUIRED** in both the fixture builder and
  the oracle per-case setup — the BUILTIN embedding path calls
  `providerRegistry.createEmbeddingProvider('BUILTIN')` (embedding-service.ts
  :204), which throws "Provider 'BUILTIN' not found in registry" until the
  provider plugins are registered. Fitting the vocab via the imported
  `TfIdfVectorizer` alone is NOT enough — generation goes through the
  registry.
- **The oracle needs a ~150 ms settle before `closeDatabase()` per case.**
  The item-GET access-time bump (and the search access bumps) are
  fire-and-forget; without the settle they corrupt the single-writer /
  DB-manager state for the NEXT case, so the auth middleware's
  `repos.users.findById` returns null → a spurious 500 "User not found"
  in place of the faithful 404. With the settle, `get_missing` correctly
  returns 404 "Memory not found" (the route's real behavior).

First five arms LANDED + differential-proven (`memories_routes_equivalence`,
17 read cases): `memoryList` (paginated + legacy in-memory paths, the
`tagDetails` full-tag enrichment via `tags::find_all`, search /
minImportance / source filters, importance-on-RAW + createdAt/updatedAt
sorts), `memoryGet` (tagDetails + a synchronous access-time bump — the v5
read boundary has no fire-and-forget lane), `memoryCountByChat`,
`memoryByMessage` (the swipe-group expansion + the trimmed
`{id,summary,characterId,importance}` shape), `memoryCharacterCounts`
(count-desc, stable ties). The read shape ECHOES `embedding` as an
index-keyed Float32 object — v4 and v5 match byte-for-byte (the f32→JSON
shortest-round-trip is identical). Ownership uses MAIN-only
`characters_read::find_by_id_raw` (id is overlay-invariant; the broken-vault
drop is out of the fixture). Response variant: `Response::Memory(Value)`.
Bumps: core 0.0.189, harness 0.0.173.

### P4.6s unit 2 — the write + search arms — LANDED (branch)

`memoryCreate` / `memoryUpdate` / `memoryDelete` / `memoryDeleteByChat` /
`memorySearch` over the ported gate + semantic-search engine. The two
embedding arms (create + search) take an injected `EmbeddingProvider`; the
engine holds an `Option<ErasedEmbeddingProvider>` on the **ReadyEngine only**
(NOT threaded through `EngineAssembly` this lane — that would break the host
+ web-test `EngineAssembly` initializers outside lane-A ownership), defaulted
to `None`, so `MemoryCreate`/`MemorySearch` answer the loud not-assembled
refusal until a future host-wiring lane adds the assembly field. The
differential proves both arms LIVE via a directly-constructed
`ApiEmbeddingProvider` over the fixture's builtin default profile.

Gotchas banked:
- **The oracle MUST un-mock `@/lib/embedding/embedding-service`.** jest.setup
  (:410) stubs `generateEmbeddingForUser` to a 3-dim `[0.1,0.2,0.3]` vector;
  left mocked, the gate/search embed at 3 dims while the fixture's baked
  vectors are 286 (vocab size) → a "Vector dimension mismatch: expected 286,
  got 3" throw on create (→ 500) and a silent text fallback on search
  (`usedEmbedding:false`). `jest.doMock(... requireActual)` restores the real
  builtin TF-IDF. (Also un-mock `@/lib/embedding/vector-store`.)
- **Pin the clock only for the search (read) cases.** The recency ranking
  uses `new Date()`; a `jest.useFakeTimers({now, doNotFake:[timers]})` pins it
  deterministically, but a frozen `Date.now()` breaks v4's single-writer IPC
  deadline logic → the WRITE cases 500. So `clock: true` gates only search;
  writes run on the real clock (minted timestamps blanked). v5 passes the same
  `FIXED_NOW_MS` = 1_783_000_000_000.
- Search scores compare at **1e-6 rounding** (f32-storage + ln-ULP seams); the
  re-read embedding vectors on create are **blanked** (id/timestamps/embedding).
- The "near-duplicate" create posts content IDENTICAL to the anchor → cosine
  1.0 → SKIP_NEAR_DUPLICATE (returns the anchor unchanged, no bump), not
  REINFORCE. v5's gate matches. The **skipGate direct-create path is a v5
  non-port** (the Phase-3 gate always runs the gate) — accepted-but-ignored,
  documented divergence; and SKIP_EMBEDDING_FAILED → server error is ported
  (memory_id None → Internal) but not differential-exercised (the real builtin
  never fails).
Bumps: core 0.0.190, harness 0.0.174.

### P4.6s unit 3 — housekeeping + config arms — LANDED (branch)

`memoryHousekeepPreview` (GET `{success, preview:{wouldDelete/Merge/Keep,
totalBefore/After, details}}`), `memoryHousekeep` (POST — dryRun→preview else
run; `{success, dryRun, result:{…, details?}}`, `details` present ONLY on
dryRun via key-omission not null), `memoryHousekeepSweep` (enqueue
MEMORY_HOUSEKEEPING, `{success, jobId}`), `memoryHousekeepingConfigGet/Set`
(per-user `chat_settings.autoHousekeepingSettings`, v4 default-injection +
merge-patch via `update_for_user` Text assignment), plus the three
instance-wide config pairs over NEW additive `db/instance_settings`
getters/setters — `set_memory_recall_settings`,
`get/set_memory_extraction_limits`, `get/set_memory_extraction_concurrency`
(concurrency default **1**, key `memoryExtractionConcurrency`, distinct from
`maxConcurrentJobs`'s 4). The extraction-concurrency runtime-override push into
the processor is a host seam (no-op).

Gotchas banked:
- **The fixture MUST create the `instance_settings` table.** v4's `readSetting`
  try/catches a missing table → default, but an INSERT (the recall/extraction
  SETs) throws → 500. Added `CREATE TABLE IF NOT EXISTS instance_settings` to
  the builder (the groups-fixture precedent). Both differential sides need it.
- **The oracle SPLITS into two files** — `memories-routes.test.ts` (24 read/
  write/search) + `memories-config.test.ts` (12 housekeep/config). A single
  jest run past ~24 cases hits a **cumulative cross-case contamination** (the
  auth middleware's `users.findById` starts returning null → spurious 500
  "User not found"/"Internal server error" clustered at the tail; writes-in-
  process, so not a forked-writer issue — a harness/instance-lock resource
  creep). Under ~20 cases per run it is clean. The Rust test loads BOTH oracles
  (`QT_ORACLE_MEMORIES_ROUTES` + `QT_ORACLE_MEMORIES_CONFIG`).
- Housekeep is **clock-sensitive** (`crate::clock::now_unix_ms`, no override).
  Proven with a **deletes-nothing config** (`maxMemories:10000, minImportance:0`;
  the fixture's ~4-month-old memories miss the default 6-month age gate) → a
  clock-independent `deleted:0` result on both sides; the deletion logic itself
  is `memory_housekeeping_tier2`-proven.
Bumps: core 0.0.191, harness 0.0.175.

### P4.6s unit 4 — regenerate + backfill status — LANDED (branch); TIER 1 COMPLETE

`memoryBackfillProgress`, `memoryRegenerateAllStatus`, `memoryRegenerateAll`
(the full v4 handler: `deleteByTypesAndStatuses` wipe → cheap-profile
resolution [defaultCheapProfileId → standard; uncensored/dangerous-compatible →
dangerous] → deduped `enqueue_memory_regenerate_all`; `{success, jobId, isNew,
cleared, message}`). NEW additive `services/queue_service` enqueuers
`enqueue_memory_regenerate_all` (userId-deduped, returns `(jobId, isNew)`) +
`enqueue_embedding_generate`. Job-structural differential over the config-oracle
file (3 cases → 15).

Gotcha: **v4's processor auto-claims the fan-out to `PROCESSING`** in the jest
env while v5 (no processor in-test) leaves it `PENDING` — a timing artifact, so
the job-row `status` is BLANKED in the differential; `type` + `payload`
(standardProfileId / dangerousProfileId, both the fixture's cheap CONN) are the
port's proof.

**Tier 1 of the P4.6s surface is COMPLETE and differential-proven** (39 cases):
list (both paths) / get / countByChat / byMessage / characterCounts / create /
update / delete / deleteByChat / search / housekeep preview+run+sweep /
housekeeping-config get+set / recall-config get+set / extraction-limits get+set /
extraction-concurrency get+set / backfillProgress / regenerateStatus /
regenerateAll. Bumps: core 0.0.192, harness 0.0.176.
