# Quilltap Changelog

## Recent Changes

### 5.0-dev

Phase 1 — pure-function ports to `quilltap-core`, each with a tier-1 differential
test against the v4 oracle:

- Memory: weighting/decay, ranking blend, recall-tag multipliers, recall-history
  ring buffer.
- Write path: write-batch partitioning, main-primary policy, folder-conflict id
  remap, unique-constraint detection.
- Context: sliding-window compression sizing; per-purpose context-budget
  arithmetic (summarize trigger, recent-message count, max-available, allocation
  split); the summarisation cadence (fold/hard gate, interchange count,
  title-check crossing, turn partition); per-character context shaping
  (history-access gate, presence windows, whisper visibility, role/name
  attribution).
- Enclave: autonomous-run budget verdict and progress-toward-binding-cap.
- LLM: completion cost estimate, cost-aware model selection, model classes,
  character-based token estimation.
- Turn manager: the turn-state machine — queue ops, history-derived state, and
  the spoken-this-cycle wrap; the all-LLM auto-pause thresholds; the
  participant-list filters (user/LLM/active resolvers); the display-only
  predicted turn order; and the weighted-random next-speaker selection (with the
  RNG injected for determinism).
- Memory name-resolution leaves: reinforced-importance formula, name+pronoun
  formatting, the about/holder name-set builders, and the word-boundary name
  matchers (presence / occurrence-count / about-character resolution) — the
  Unicode-boundary + lookahead regex reproduced without a backtracking engine.
- Embedding: L2 vector normalisation, the profile storage policy (Matryoshka
  truncate + optional normalise), cosine similarity with the dimension-mismatch
  guard and message, the fallback keyword/phrase scorer, the literal-phrase
  boost helpers, Float32 ↔ little-endian-byte BLOB conversion, and the legacy
  JSON-text recovery (`parseLegacyEmbeddingText` — reproducing JS `Object.values`
  ascending integer-key ordering for the index-keyed-object shape).
- Canon: the memory-extraction canon blocks (self / other ALREADY ESTABLISHED
  rendering) and the New-Chat scenario-text combiner.
- Mentioned-character scan: detecting non-participant characters named in a chat
  corpus (ASCII word-boundary alternation, longest-token-first, lowercased
  token→ids map).
- Novel-detail extraction: the deterministic proper-noun / date / currency /
  number-with-unit / CamelCase / acronym scanner (ASCII `\d`/`\b`, the JS `\s`
  whitespace set reproduced exactly, case-insensitive dedup).
- Chat-task text shaping: tool-artifact stripping, visible-conversation
  extraction, and the chat-card preview, over shared JS string primitives (the
  JS `\s`/`trim` set and UTF-16 length/slice).
- Docs: added `docs/developer/porting/phase-2-onramp.md` scoping the tier-2
  DB-state oracle and its fixtures (the next build); cross-linked from the
  porting overview and CLAUDE.md, and marked Phase 1 complete in the roadmap.
- Model context limit: `getModelContextLimit` (+ `hasExtendedContext`,
  `getSafeInputLimit`) — the override / provider-default tables ported as
  constants, with the plugin model-info, `FALLBACK_PRICING` rows, and registry
  default injected; reproduces v4's lookup order and substring matching, and the
  JS-truthy fall-through on a zero/null context value.
- Cheap-model classifiers: `isCheapModel` / `estimateModelCost` /
  `getCheapestModel` and their deprecated fallback tables — the registry-sourced
  recommended-list and default-model are injected (empty / none takes the
  fallback path), the string heuristics (expensive/mid/cheap indicators, the
  dashed-vs-undashed `o1`/`o3` split) are pure.
- Version compare: documented `compareVersions`' `localeCompare` fallback (the
  malformed-input path) as a deferred ICU-collation seam — the parseable
  numeric path stays exact; faithful collation waits on the ICU-crate decision.
- Tool canonicalization: byte-stable `UniversalTool` serialization for
  cache-prefix stability — deep code-unit key-sort of `function.parameters` plus
  the tool-name array sort. The name sort is a documented `localeCompare`
  residual seam (the lowercase snake_case tool-name corpus collates identically
  under code-unit order; the ICU-collation decision is deferred).
- Number formatting: the JS `Number.prototype.toFixed` kernel (V8
  half-away-from-zero rounding on the f64's exact value, via IEEE-754
  mantissa/exponent + u128 — distinct from Rust's half-to-even formatter), and
  the display formatters built on it (`formatBytes`, `formatCostForDisplay`, and
  both the `K` and lowercase-`k` `formatTokenCount` variants).
- Small leaf utilities: chat-type/participant predicates, semver parse/compare,
  pronoun→gender hint, tag-style merge, char-count colour class.

Phase 2 on-ramp — the tier-2 DB-state oracle (structural DB diff for repo/service
ops), built as a thin vertical slice over the `folders` repo:

- Oracle harness (TypeScript, drives v4's real `lib/`): a committed plaintext
  fixture spec (`harness/oracle/fixtures/folders-tier2.json`) under a throwaway
  test pepper; a fixture builder that materializes a fresh ChaCha20 DB at test
  time via v4's own `ensureCollection` + `FoldersRepository.create`; and the
  `folders-tier2` case that copies the fixture, runs a fixed create + update
  through the real repo, and emits the canonical post-op `folders` dump as NDJSON.
- Canonical dump shaping (`harness/oracle/lib/tier2.ts`): columns in on-disk
  order, rows sorted by a stable key, BLOBs as hex, nulls explicit.
- Determinism: ids and timestamps pinned on both sides (CreateOptions on create,
  explicit `updatedAt` on update), so the dump needs zero normalization — the
  strongest tier-2 form. The id-remap / timestamp-placeholder fallbacks are
  reserved for later repos that cannot take injected ids/clocks.
- Rust DB layer (`quilltap-core::db`): the writable cipher-correct open (key
  pragma first, then `foreign_keys = ON` + `journal_mode = TRUNCATE`), the
  single-writer `Writer` that solely holds the RW connection, the `folders`
  repo's `create` + `update` ported from v4, and a canonical `dump_table_json`
  matching the oracle's shape.
- Build: the SQLite3MultipleCiphers amalgamation build (`build.rs` + `vendor/`)
  moved from the probe into `quilltap-core`, which now links the ChaCha20/sqleet
  library for the whole workspace; the workspace `rusqlite` dependency switched
  off `bundled-sqlcipher` to the amalgamation (`buildtime_bindgen`). The
  throwaway `sqlcipher-probe` / `sqlite3mc-probe` crates are retired.
- Harness: tier-2 differential test `folders_tier2_equivalence` — copies the
  same seed fixture, runs the Rust ops, structural-diffs the dump against the
  oracle NDJSON (`QT_ORACLE_FOLDERS` + `QT_FIXTURE_FOLDERS`, skip-if-unset).
  The `folders` repo round-trips green.

Phase 2 — repo-by-repo over the real DB (each ported repo arrives with its
tier-2 case):

- `tags` repo (`quilltap-core::db::tags`): `create`, `update`, and `delete`
  ported from v4's `TagsRepository` + base-repo internals. Widens the tier-2
  marshaling surface past `folders`' all-strings shape — a boolean column
  (`quickHide` stored as INTEGER 0/1), a nullable JSON-object column
  (`visualStyle` stored as compact JSON in schema field order, reproduced with a
  typed struct so key order matches v4's `JSON.stringify` rather than a sorted
  map), and the `nameLower` derivation (`(nameLower || name).toLowerCase()` on
  create; re-derived from `name` on update). Adds the `delete` op to the harness.
- Harness: tier-2 differential test `tags_tier2_equivalence` plus its fixture
  builder + `tags-tier2` oracle case, driven by the committed
  `harness/oracle/fixtures/tags-tier2.json` (the create op carries a
  fully-specified `visualStyle` so no Zod inner-default expansion is involved).
  Ids and timestamps pinned both sides → zero normalization. The `tags` repo
  round-trips green (`QT_ORACLE_TAGS` + `QT_FIXTURE_TAGS`, skip-if-unset).
- Generated-UUID remap + timestamp-placeholder normalization (the tier-2
  machinery for ops that mint their own ids/clocks, not just the pinned-id sync
  path). `folders.create` now ports v4 `_create`'s minted-values defaults
  (`id = options?.id || generateId()`, timestamps `|| now`) and returns the id
  used, so a caller can wire it into a dependent op. New `quilltap-core::clock`
  (`now_iso` / pure `iso_from_unix_ms`) reproduces v4's
  `new Date().toISOString()` shape; `uuid` (v4) generates ids. Verified by the
  `folders_remap_tier2_equivalence` test: a parent + child created with NOTHING
  pinned, so both v4 and Rust mint different random UUIDs and timestamps. One
  normalization (in the harness) runs over both dumps — rows walked in
  natural-key (`path`) order, id columns (`id`, `parentFolderId`) collapsed to
  first-seen tokens (`ID_0`, `ID_1`), so the child→parent FK relationship is
  verified without pinning the literal id; timestamps placeholdered after
  asserting the `createdAt == updatedAt` create invariant per row. Round-trips
  green (`QT_ORACLE_FOLDERS_REMAP` + `QT_FIXTURE_FOLDERS_REMAP`, skip-if-unset).
- The partitioned write APPLIER (`quilltap-core::write_apply`) — the writer-task
  apply path ported from v4's `applyWritesUnsafe` / `applyPartition` /
  `applySecondaryBestEffort` / `applyFolderCreateIdempotent`. Sequences the pure
  `write_partition` leaves into the real orchestration: each partition (main /
  mount-index / llm-logs) commits in its own `BEGIN IMMEDIATE` transaction;
  main-primary jobs (`AUTONOMOUS_ROOM_TURN`) commit main first then apply
  secondaries best-effort (a dropped doc-store effect can't lose the chat turn),
  while idempotent jobs apply secondaries first so a secondary failure prevents
  the main commit; and the concurrent `docMountFolders.create` unique-conflict
  reconcile resolves to the existing row and remaps the discarded buffered folder
  id for the rest of the batch. The engine is generic over an injected
  `ApplyHost` seam (the three connections + repo dispatch + the reconcile
  lookup), mirroring how v4 unit-tests this orchestration with fakes.
- Harness: `write_apply_equivalence` — a tier-1-style TRACE differential over a
  committed 9-scenario corpus (`harness/oracle/fixtures/write-apply.json`). Both
  sides emit the same observable trace (per-partition exec sequence, ordered repo
  dispatches with post-remap args, reconcile lookups, resolved/threw outcome).
  The oracle (`harness/oracle/cases/write-apply.test.ts`) drives v4's REAL
  `applyWritesUnsafe` — it runs under v4's jest (not tsx) because the applier's
  `getRawDatabase()` / `getRepositories()` singletons are `jest.mock`-injected;
  v4's jest resolves the v5-tree oracle file via an extra `--roots`. Deferred
  (documented): `__finalizeFile` (fs rename + undo-on-rollback) and the
  post-commit `cleanupStagingDirs` / `dispatchInvalidations` side effects.
- `text_replacement_rules` repo (`quilltap-core::db::text_replacement_rules`):
  `create`, `update`, and `delete` ported from v4's
  `TextReplacementRulesRepository`. The first repo with **conflict detection** —
  and so the first to need a repo-level *read*: `create`/`update` scan the
  existing rows and reject a duplicate `(fromText, caseSensitive)` pair
  (case-sensitive rules compare `fromText` exactly, case-insensitive ones
  compare lowercased; the `caseSensitive` flag is part of the key, and `update`
  only re-checks when that pair changes). A conflict surfaces as
  `TrrError::Conflict`, the analogue of v4's `TextReplacementRuleConflictError`.
  Single-user (no `userId`). Widens the tier-2 marshaling surface past `tags`
  with a real INTEGER number column (`sortOrder`) and two boolean columns
  (`caseSensitive`, `enabled`).
- Harness: tier-2 differential `text_replacement_rules_tier2_equivalence` plus
  its fixture builder + `text-replacement-rules-tier2` oracle case, driven by the
  committed `harness/oracle/fixtures/text-replacement-rules-tier2.json`. The op
  sequence includes two conflicting ops flagged `expectThrow`: both the oracle
  (asserting v4 threw `TextReplacementRuleConflictError`) and the Rust port
  (asserting `TrrError::Conflict`) prove the rejection independently, and the
  final-state dump confirms the rejected writes left no trace (a port lacking the
  check would have diverged). Ids + timestamps pinned → zero normalization.
  Round-trips green (`QT_ORACLE_TRR` + `QT_FIXTURE_TRR`, skip-if-unset). The
  toLowerCase case-mapping seam (shared with `tags.nameLower`) gains a second
  site here — tracked in the deferred-seams list.
- Canonical dump: `js_number_to_json` — the dump's REAL-cell rendering now
  mirrors JS `JSON.stringify(number)`, collapsing an integer-valued double
  (`9.0` → `9`) so a REAL-affinity numeric column (e.g. `z.number().int()`,
  which SQLite stores as an 8-byte float) matches the oracle, where
  better-sqlite3 hands JS a `Number` and `JSON.stringify` drops the `.0`. First
  exercised by `text_replacement_rules`' `sortOrder`.
- `prompt_templates` repo (`quilltap-core::db::prompt_templates`): `create`,
  `update`, and `delete` ported from v4's `PromptTemplatesRepository` (built-in
  *seeding* is a startup concern, out of scope). Widens the tier-2 marshaling
  surface with the **first JSON array column** (`tags: z.array(UUIDSchema)` →
  compact JSON text, `["id"]` / `[]`; reproduced via `serde_json::to_string` of a
  `Vec<String>` — arrays are order-preserving, so no key-order subtlety) and
  several **nullable string columns** (`userId` null-for-built-in, `description`,
  `category`, `modelHint`). Adds the **built-in read-only guard**: `update`/
  `delete` read the target's `isBuiltIn` and refuse to mutate a built-in row,
  returning a not-modified result (`Ok(false)`; v4's `null` / `false`) rather
  than throwing — a read-then-guard pattern that suppresses the op instead of
  raising. Plain `AbstractBaseRepository` (nullable `userId`).
- Harness: tier-2 differential `prompt_templates_tier2_equivalence` plus its
  fixture builder + `prompt-templates-tier2` oracle case, driven by the committed
  `harness/oracle/fixtures/prompt-templates-tier2.json`. The op sequence
  exercises the array column on create and on update (replacing the array), the
  nullable columns (null vs present), and the guard two ways via an `expectNoop`
  flag — an update and a delete that both target the built-in seed row; both
  sides assert the op reported not-modified (Rust `Ok(false)`; oracle `null` /
  `false`) and the final-state dump confirms the built-in row stayed
  byte-identical. Ids + timestamps pinned → zero normalization. Round-trips green
  (`QT_ORACLE_PROMPT_TEMPLATES` + `QT_FIXTURE_PROMPT_TEMPLATES`, skip-if-unset).
- Three more plain-base repos ported in parallel (each `create` / `update` /
  `delete`, pinned form, its own tier-2 case round-tripping green):
  - `conversation_annotations` (`quilltap-core::db::conversation_annotations`):
    banks a **REAL-affinity unbounded-int column** — `messageIndex` is
    `z.number().int().min(0)` with no `.max()`, and v4's schema translator
    (`mapToSQLiteType`) only assigns INTEGER affinity when a numeric field has
    both an integer min and max, so it maps to REAL; bound as `f64`, the dump's
    `js_number_to_json` collapses the integer-valued cell back to a bare integer.
    Also a **nullable UUID column** (`sourceMessageId`). Harness
    `conversation_annotations_tier2_equivalence` (`QT_ORACLE_CONV_ANNOTATIONS` +
    `QT_FIXTURE_CONV_ANNOTATIONS`).
  - `provider_models` (`quilltap-core::db::provider_models`): banks **two
    nullable REAL number columns** (`contextWindow`, `maxOutputTokens` — both
    bare `z.number()`, no min/max → REAL), **two boolean-default columns**
    (`deprecated`, `experimental` → INTEGER 0/1), and **enum TEXT columns**
    (`provider`, `modelType`). The corpus supplies every column explicitly so no
    Zod create-time default is relied on. Harness
    `provider_models_tier2_equivalence` (`QT_ORACLE_PROVIDER_MODELS` +
    `QT_FIXTURE_PROVIDER_MODELS`).
  - `help_docs` (`quilltap-core::db::help_docs`): the **first tier-2 BLOB
    column** — `embedding` is a Float32 buffer (little-endian `f32` bytes via
    `embedding_blob::float32_to_blob`), with empty/null → SQL NULL and the dump
    emitting BLOBs as lowercase hex on both sides for bit-exact comparison
    (fixture uses only exactly-float32-representable values so the f64→f32 cast
    is lossless). Banks that a **text-only update preserves the BLOB**: the
    partial `UPDATE SET` never names the embedding column, mirroring v4's
    whole-row rewrite that re-persists the existing embedding unchanged. Harness
    `help_docs_tier2_equivalence` (`QT_ORACLE_HELP_DOCS` + `QT_FIXTURE_HELP_DOCS`).
- A second parallel batch of three repos (each `create` / `update` / `delete`,
  pinned form, its own tier-2 case round-tripping green):
  - `roleplay_templates` (`quilltap-core::db::roleplay_templates`): the **first
    array-of-objects JSON column** — `renderingPatterns: z.array(...)` stored as a
    compact JSON array of objects, each element modeled by a typed serde struct in
    schema field order (`#[serde(rename_all = "camelCase")]` + `skip_serializing_if`
    on the optionals) so the key order and omitted-optional behavior match v4's
    `JSON.stringify(zodParsed)` byte-for-byte — plus a **nullable JSON-object
    column** (`dialogueDetection`). `delimiters` is held empty and
    `narrationDelimiters` kept to its plain-string form (the discriminated-union /
    tuple forms buy no new marshaling coverage). No built-in guard ported (the
    corpus never mutates a built-in row). Harness
    `roleplay_templates_tier2_equivalence` (`QT_ORACLE_ROLEPLAY_TEMPLATES` +
    `QT_FIXTURE_ROLEPLAY_TEMPLATES`).
  - `image_profiles` (`quilltap-core::db::image_profiles`): banks the **Taggable
    lineage** (`userId` + a JSON `tags` array) and the first **open / arbitrary-
    JSON object column** (`parameters`, `z.record`), modeled as `serde_json::Value`
    → compact JSON text, plus boolean and nullable-string columns. Harness
    `image_profiles_tier2_equivalence` (`QT_ORACLE_IMAGE_PROFILES` +
    `QT_FIXTURE_IMAGE_PROFILES`).
  - `connection_profiles` (`quilltap-core::db::connection_profiles`): the
    workhorse profile repo and the **widest marshaling surface** to date — ~29
    columns spanning three enum TEXT columns, eight booleans, two nullable REAL
    int-overrides (`maxContext`/`maxTokens`), five REAL token counters, three
    nullable strings, the `tags` array, and the open `parameters` object. The
    corpus supplies every column explicitly. Harness
    `connection_profiles_tier2_equivalence` (`QT_ORACLE_CONNECTION_PROFILES` +
    `QT_FIXTURE_CONNECTION_PROFILES`).
  - New tracked deferred seam (open-JSON multi-key key order): an open-JSON object
    column with **two or more keys** would diverge — `serde_json::Value` sorts keys
    while v4's `JSON.stringify` preserves insertion order. The `image_profiles` /
    `connection_profiles` corpora constrain `parameters` to `{}` or single-key
    objects; see "Deferred seams" in `docs/developer/porting/phase-2-onramp.md`.

- A third parallel batch — five plain-base single-table repos (each `create` /
  `update` / `delete`, its own tier-2 case round-tripping green):
  - `plugin_config` (`quilltap-core::db::plugin_config`): the **UserOwned lineage**
    (a `userId` scope column) plus an **open-JSON object column** (`config`,
    `z.record`) and an **optional (nullable) boolean** (`enabled`,
    `z.boolean().optional()` with no default → INTEGER 0/1 when present, SQL NULL
    when the key is absent — confirmed empirically). Harness
    `plugin_config_tier2_equivalence` (`QT_ORACLE_PLUGIN_CONFIG` +
    `QT_FIXTURE_PLUGIN_CONFIG`).
  - `embedding_profiles` (`quilltap-core::db::embedding_profiles`): the Taggable
    lineage again, widened with an **enum TEXT** column (`provider`), two **nullable
    REAL number** columns (`dimensions` bare `z.number()`, `truncateToDimensions`
    `z.number().int().positive()` — min-only, so REAL not INTEGER), and two
    **boolean-default** columns (`normalizeL2`, `isDefault`). Harness
    `embedding_profiles_tier2_equivalence` (`QT_ORACLE_EMBEDDING_PROFILES` +
    `QT_FIXTURE_EMBEDDING_PROFILES`).
  - `terminal_sessions` (`quilltap-core::db::terminal_sessions`): a clean
    string-heavy repo — nullable string columns (`label`, `transcriptPath`), a
    nullable timestamp (`exitedAt`), and a **nullable REAL** column (`exitCode`,
    `z.number().int()`, no max). v4's `create` injects no nondeterministic defaults,
    so the pinned zero-normalization form holds. Harness
    `terminal_sessions_tier2_equivalence` (`QT_ORACLE_TERMINAL_SESSIONS` +
    `QT_FIXTURE_TERMINAL_SESSIONS`).
  - `character_plugin_data` (`quilltap-core::db::character_plugin_data`): the first
    **open-JSON _value_ column** (`data`, `z.unknown()`) — any JSON value stored as
    compact JSON text via v4's `prepareForStorage`, modeled as `serde_json::Value`.
    Harness `character_plugin_data_tier2_equivalence`
    (`QT_ORACLE_CHARACTER_PLUGIN_DATA` + `QT_FIXTURE_CHARACTER_PLUGIN_DATA`).
  - `tfidf_vocabulary` (`quilltap-core::db::tfidf_vocabulary`): the first repo that
    **overrides the base `create`/`update`** — v4 mints `updatedAt =
    getCurrentTimestamp()` unconditionally (a passed `updatedAt` is ignored), so the
    port mints it via `clock::now_iso` and the harness placeholder-normalizes only
    that one column (ids / `createdAt` / every payload column stay pinned and diff
    exactly). Also the first **plain-string columns that hold JSON text**
    (`vocabulary`, `idf`, bound single-encoded, not re-stringified), plus a bare
    `z.number()` REAL (`avgDocLength`) and an int-positive REAL (`vocabularySize`).
    Harness `tfidf_vocabulary_tier2_equivalence` (`QT_ORACLE_TFIDF_VOCABULARY` +
    `QT_FIXTURE_TFIDF_VOCABULARY`).
  - The `plugin_config` / `character_plugin_data` open-JSON corpora are constrained
    to `{}` or single-key objects, same as the tracked multi-key key-order seam.

- A fourth parallel batch — five more main-DB repos (each `create` / `update` /
  `delete`, its own tier-2 case round-tripping green):
  - `users` (`quilltap-core::db::users`): the plainest surface yet — all strings
    plus five **nullable TEXT** columns (`email`, `name`, `image`, `emailVerified`,
    `passwordHash`), no booleans/numbers/JSON/BLOB. Harness
    `users_tier2_equivalence` (`QT_ORACLE_USERS` + `QT_FIXTURE_USERS`).
  - `conversation_chunks` (`quilltap-core::db::conversation_chunks`): the **second
    tier-2 BLOB column** (`embedding`, Float32 LE bytes via
    `embedding_blob::float32_to_blob`, null/empty → NULL, dumped as hex; a text-only
    update leaves it untouched) plus a REAL int (`interchangeIndex`,
    `z.number().int().min(0)` — min-only → REAL) and two **JSON string-array
    columns** (`participantNames`, `messageIds`). Harness
    `conversation_chunks_tier2_equivalence` (`QT_ORACLE_CONVERSATION_CHUNKS` +
    `QT_FIXTURE_CONVERSATION_CHUNKS`).
  - `files` (`quilltap-core::db::files`): the **widest repo to date** (~23 columns,
    Taggable) — a bare-`z.number()` REAL (`size`), two **nullable REAL** columns
    (`width`/`height`), an **optional boolean** (`isPlainText` — banks both the
    present 0/1 and the absent → NULL case), two JSON arrays (`linkedTo`, `tags`),
    three enum TEXT columns (`source`, `category`, `fileStatus`), and several
    nullable strings. Harness `files_tier2_equivalence` (`QT_ORACLE_FILES` +
    `QT_FIXTURE_FILES`).
  - `chat_documents` (`quilltap-core::db::chat_documents`): an enum TEXT column
    (`scope`), a boolean (`isActive`), and two nullable strings. Harness
    `chat_documents_tier2_equivalence` (`QT_ORACLE_CHAT_DOCUMENTS` +
    `QT_FIXTURE_CHAT_DOCUMENTS`).
  - `embedding_status` (`quilltap-core::db::embedding_status`): the second repo that
    **overrides the base `create`/`update`** with an unconditionally-minted
    `updatedAt` (like `tfidf_vocabulary`) — the port mints it via `clock::now_iso`
    and the harness placeholder-normalizes only `updatedAt` (id / `createdAt` /
    payload pinned). Two enum TEXT columns (`entityType`, `status`) + a nullable
    timestamp + a nullable string. Harness `embedding_status_tier2_equivalence`
    (`QT_ORACLE_EMBEDDING_STATUS` + `QT_FIXTURE_EMBEDDING_STATUS`).

Phase 2 — the mount-index sibling-DB slice (the first repos NOT in the main DB).
These tables live in v4's dedicated `quilltap-mount-index.db`. The tier-2
machinery was extended to target a sibling DB: the fixture builder + oracle point
`SQLITE_MOUNT_INDEX_PATH` at the fixture (with a throwaway main DB at
`SQLITE_PATH`), seed/run through v4's real repos (whose `getCollection` override
routes there), flush via `closeMountIndexSQLiteClient`, and read back through
`getRawMountIndexDatabase` directly (not `rawQuery`, which targets the main
backend). The Rust `Writer` needed no change — `open_writable` already opens any
ChaCha20 file by path, so the partition is simply which file the writer opened.
Five repos ported in one slice (a serial pilot, then four parallel), each with its
own tier-2 case round-tripping green (pinned ids + timestamps → zero
normalization):

  - `group_character_members` (`quilltap-core::db::group_character_members`): the
    pilot — the plainest join table (`id` + two UUID-as-TEXT refs + timestamps).
    Harness `group_character_members_tier2_equivalence`
    (`QT_ORACLE_GROUP_CHARACTER_MEMBERS` + `QT_FIXTURE_GROUP_CHARACTER_MEMBERS`).
  - `project_doc_mount_links` / `group_doc_mount_links`
    (`quilltap-core::db::{project_doc_mount_links,group_doc_mount_links}`):
    structurally identical join tables (cross-DB refs stored as plain TEXT — v4's
    `generateCreateTable` emits no FK constraints). Harnesses
    `project_doc_mount_links_tier2_equivalence` /
    `group_doc_mount_links_tier2_equivalence`.
  - `doc_mount_folders` (`quilltap-core::db::doc_mount_folders`): adds a **nullable
    UUID** column (`parentId`, null = mount-point root) — banks both the null and
    non-null paths. Harness `doc_mount_folders_tier2_equivalence`.
  - `doc_mount_points` (`quilltap-core::db::doc_mount_points`): the widest of the
    family (18 columns) — four enum TEXT columns, a boolean (`enabled`, banks 0 and
    1), two **JSON string-array** columns (`includePatterns`/`excludePatterns`,
    banks empty and non-empty), three nullable strings/timestamp, and three
    **REAL-affinity int counters** (`fileCount`/`chunkCount`/`totalSizeBytes`,
    `z.number().int()` with no min&max → REAL, integer-collapsed in the dump). Its
    runtime ALTER-TABLE "migrations" are no-ops on a fresh schema-generated table.
    Harness `doc_mount_points_tier2_equivalence`.

Phase 2 — the llm-logs sibling DB + the deferred `upsert*` methods (two
independent slices).

`llm_logs` (`quilltap-core::db::llm_logs`): the SECOND sibling-DB partition (v4's
`quilltap-llm-logs.db`) and the widest repo in Phase 2 — 18 columns including FIVE
nested typed-struct JSON columns (`request`, `response`, `usage`, `cacheUsage`,
`requestHashes`), an open-JSON `rawProviderUsage`, a nullable REAL (`durationMs`),
an 18-variant enum, and four nullable UUIDs. Same TS-only sibling-DB machinery as
the mount-index slice but pointed at `SQLITE_LLM_LOGS_PATH` / read back through
`getRawLLMLogsDatabase()` (the backend disconnect closes this client, so the
oracle reads before `closeDatabase()`). The nested JSON is reproduced byte-for-byte
with serde structs in schema field order: integer-valued nested numbers as `i64`
(so they render `3`, not `3.0`, matching `JSON.stringify`), `temperature` the lone
`f64` (kept fractional), optional nested fields `skip_serializing_if` (omitted, not
null). Pinned zero-normalization form; `rawProviderUsage` constrained to
null/`{}`/single-key (the open-JSON seam). Harness `llm_logs_tier2_equivalence`.

The deferred `upsert*` methods on six already-ported repos are now implemented,
each with its own tier-2 case in the REMAP (minted-values) form: the upsert mints
`id`/`createdAt`/`updatedAt` on the create branch and `updatedAt` (preserving
`id`/`createdAt`) on the update branch, so the test pins nothing for the upsert
ops — it remaps `id` to first-seen tokens in natural-key order and placeholders
both timestamps (the folders-remap `createdAt == updatedAt` invariant is dropped,
since an upsert-update legitimately differs). Each `upsert*` adds a private
find-by-key SELECT and mints via `clock::now_iso` + `uuid`.

  - `conversation_annotations.upsert` — find by (chatId, messageIndex,
    characterName); update sets only {content, sourceMessageId}. Added a nullable
    setter (`Option<Option<_>>`) for `sourceMessageId`. Harness
    `conversation_annotations_upsert_tier2_equivalence`.
  - `help_docs.upsertByPath` — find by `path`; update sets {title, url, content,
    contentHash}, leaving the `embedding` BLOB untouched; create stores a NULL
    embedding. The test proves an upsert-update preserves a non-null embedding.
    Harness `help_docs_upsert_tier2_equivalence`.
  - `provider_models.upsertModel` (+ a thin `upsertModelForProvider` loop) — find
    replicates v4's `findByProviderAndModelId`: `baseUrl` joins the predicate only
    when truthy (a falsy baseUrl leaves the column unconstrained — NOT "match
    NULL"). Update writes the full data. Harness
    `provider_models_upsert_tier2_equivalence`.
  - `plugin_config.upsertForUserPlugin` — find by (userId, pluginName); update
    MERGEs `{...existing, ...new}` config (corpus keeps the merge {}/single-key).
    Harness `plugin_config_upsert_tier2_equivalence`.
  - `character_plugin_data.upsert` — find by (characterId, pluginName); update sets
    {data} (open-JSON, {}/single-key). Harness
    `character_plugin_data_upsert_tier2_equivalence`.
  - `tfidf_vocabulary.upsertByProfileId` — find by `profileId`; update writes full
    data. Builds on the base-method-override minting (create/update mint
    `updatedAt` themselves). Harness `tfidf_vocabulary_upsert_tier2_equivalence`.

Phase 2 — a fifth parallel batch of five repos (`create` / `update` / `delete`
each, pinned ids + timestamps → zero normalization), spanning the main DB and the
mount-index sibling DB:

  - `chat_settings` (`quilltap-core::db::chat_settings`): a plain main-DB
    `AbstractBaseRepository`, and the **widest JSON-object surface in Phase 2** —
    ~33 columns including ~15 nested typed-struct JSON columns reproduced in schema
    field order (serde structs, not key-sorting `serde_json::Value`), nested integer
    fields typed `i64` so they render bare. Banks the **first INTEGER-affinity number
    column** (`sidebarWidth`, `.min(256).max(512)` — both bounds integer → INTEGER,
    unlike the prior min-only/bare REAL numbers). The `cheapLLMSettings` column keeps
    its uppercase acronym (camelCase would mangle it). The `*ForUser`
    default-injecting helpers and the multi-key open-JSON `tagStyles` key order are
    out of scope (the corpus keeps `tagStyles` `{}`). Harness
    `chat_settings_tier2_equivalence`.
  - `wardrobe` (`quilltap-core::db::wardrobe`, table `wardrobe_items`): the first
    repo whose **public CRUD is vault-only** — v4's `WardrobeRepository` writes to
    the document store and throws without a mount, with no SQL write mirror — so the
    differential drives v4's **real base-repository SQL CRUD** (`_create`/`_update`/
    `_delete`) against the table via a thin subclass exposing the protected
    internals (the marshaling the schema-translator builds from `WardrobeItemSchema`
    and the table's reads consume). Banks the first repo with **two JSON array
    columns** (`types` — the first enum-string array — and `componentItemIds`) and a
    **nullable soft-delete timestamp** (`archivedAt`, exercised null and
    set-to-non-null), alongside two booleans and several nullable string/UUID
    columns. The vault-overlay write path itself is NOT ported/verified (tracked
    deferral); the unarchive (`archivedAt` → NULL) nullable-setter is implemented but
    not in the corpus. Harness `wardrobe_tier2_equivalence`.
  - `doc_mount_files` (`quilltap-core::db::doc_mount_files`): a mount-index sibling-DB
    repo and the **narrowest tier-2 repo to date** (all-required columns, no JSON/
    boolean/nullable). Re-banks a REAL-affinity min-only int (`fileSizeBytes`,
    `.int().min(0)` → REAL, integer-collapsed) and two enum TEXT columns; v4's
    `getCollection` adds a non-UNIQUE sha256 lookup index that touches no row bytes.
    Harness `doc_mount_files_tier2_equivalence`.
  - `doc_mount_documents` (`quilltap-core::db::doc_mount_documents`): a mount-index
    sibling-DB repo — the database-backed file-content store keyed by a UNIQUE
    `fileId`. Banks a `plainTextLength` min-only REAL int, a UUID-as-TEXT UNIQUE
    natural key, and plain TEXT content/sha columns (the content-addressable +
    joined-view read helpers are out of scope). Harness
    `doc_mount_documents_tier2_equivalence`.
  - `doc_mount_chunks` (`quilltap-core::db::doc_mount_chunks`): a mount-index
    sibling-DB repo and the **first sibling-DB repo to carry a BLOB column** — the
    `embedding` Float32 little-endian BLOB (empty/null → NULL, dumped as hex for
    bit-exact compare, and a text-only update proven to leave it untouched, like
    `conversation_chunks`/`help_docs`) plus two REAL-affinity min-only int counters
    (`chunkIndex`/`tokenCount`) and a nullable `headingContext`. The `updateEmbedding`
    BLOB-mutating path is out of scope. Harness `doc_mount_chunks_tier2_equivalence`.

Phase 2 — the document-store STORAGE PRIMITIVE
(`quilltap-core::db::doc_mount_file_links`), build step 1 of the document-store
overlay slice. Ports v4's `writeDatabaseDocument` + `linkDocumentContent` +
`ensureLinkFolderId` — the byte-landing path every store-backed entity
(project/group store, character vault) ultimately calls. A
`(mountPointId, relativePath, content)` write is content-addressed by SHA-256 and
split across three tables in one transaction (find-or-create `doc_mount_files` by
sha → upsert `doc_mount_documents` by `fileId` → upsert `doc_mount_file_links` by
`(mountPointId, relativePath)`), with `doc_mount_folders` rows auto-created for any
parent path. Also ports the pure leaves it needs: `sha256OfString`,
`detectDatabaseFileType`, `normaliseRelativePath`, and the per-document policy
(`coercePolicyBool` / `policyFromFrontmatterData` / `policyFromContent`, scalar
frontmatter subset). The tier-2 differential (`doc_mount_file_links_tier2_equivalence`)
drives v4's REAL `linkDocumentContent` against a mount-index fixture and diffs all
FOUR resulting tables in the minted-values remap form, extended with a SHARED
cross-table id-map (so `document.fileId` / `link.fileId` / `link.folderId` /
`folder.parentId` FKs verify by relationship); `mountPointId` is the pinned seeded
store id. The corpus covers a fresh JSON + markdown write, subfolder creation,
dedup-by-sha (a second path with identical content reuses one file + one document
row), link upsert-in-place (rewriting a path), and the markdown frontmatter policy
cascade (`character_read: false` → all `allow*` = 0). The oracle drives
`linkDocumentContent` directly rather than `writeDatabaseDocument` to avoid the
post-write `reindexSingleFile` chunk/embed pass (which would mutate the link rows;
its only skip-switch, `QUILLTAP_JOB_CHILD=1`, reroutes repos through the
forked-child write proxy). Deferred: arbitrary-YAML frontmatter (scalar subset
only — lands with the character-vault YAML decision), the UTF-16 `plainTextLength`
vs UTF-8 `fileSizeBytes` split is reproduced but only exercised on ASCII content,
and `linkBlobContent` / the read/GC/conversion helpers.

Phase 2 — the document-store OVERLAY ENGINE + the `groups` store-backed pilot
(`quilltap-core::db::{document_store_overlay, ensure_official_store, groups}`),
build steps 2-3 of the overlay slice. Ports v4's generic
`createDocumentStoreOverlay` + `AbstractStoreBackedRepository` as a Rust generic
over a `StoreEntity` trait, plus `ensureOfficialStore` provisioning, bound to
`groups`. A group's substantive content lives not in `groups` columns but in its
official document store as four overlay files (`properties.json` — the typed
`color`/`icon` bag in schema order, 2-space pretty-print; `description.md` /
`instructions.md` — raw markdown, empty → `null` on read; `state.json`). The slim
row (id/name/officialMountPointId/timestamps) lives in the MAIN db, the store in
the MOUNT-INDEX db, so `GroupsRepository` spans both connections (new
`Writer::connection()` seam). Reads overlay the store (the `doc_mount_documents`
3-table path→content join, new `find_[many_by]_mount_point[s]_and_path`); writes
route store-resident fields to the store and strip them from the slim patch
(properties via read-modify-write so a partial patch preserves untouched keys);
create runs the 5-step sequence (slim row → provision a `Group Files: <name>`
mount point + link + raw FK → write the four files → overlay re-read). Failure is
asymmetric (v4): `find_by_id` THROWS `OverlayError::Unavailable`, `find_all` DROPS
the bad row. Also ports the pure `nextUniqueMountPointName` (tier-1 unit test).
The tier-2 differential (`groups_tier2_equivalence`) drives v4's REAL
`repos.groups.create`/`.update` end-to-end (no mocked storage boundary, no
`QUILLTAP_JOB_CHILD`) and diffs SEVEN tables across BOTH dbs — the slim `groups`
row + `doc_mount_points` / `_files` / `_documents` / `_file_links` / `_folders` +
`group_doc_mount_links` — in the minted-values remap form with ONE shared
cross-db id-map (so `groups.officialMountPointId` → the store, `link.fileId` →
`file.id`, etc. verify by relationship). v4's post-write `reindexSingleFile` runs
(database-backed stores chunk with no model — deterministic); its only divergence,
the link `chunkCount` + the derived `doc_mount_chunks` rows, is pinned/excluded.
The corpus banks the 5-step create, `properties.json` byte-exact (both keys + the
empty bag), a store-only update (slim `updatedAt` NOT bumped) with a properties
RMW that preserves the untouched `icon`, a DB-only `name` update (store
untouched), dedup-by-sha (`"{}"` shared by three links across two stores; `""` by
two), and orphan-on-rewrite. A second test banks the keystone throw-vs-drop
asymmetry. Deferred: step-2 store adoption (the startup-heal heuristic — the
corpus always provisions fresh), `state`/property null-vs-absent + multi-key
insertion order (open-JSON seam — corpus kept `{}`/single-key), and the
`projects` generalization (a larger bag + roster ops).

Phase 2 — the character vault **wardrobe YAML emitter** (Decision A — the only
eemeli/yaml site), `quilltap-core::vault_overlay::build_wardrobe_item_file`, v4's
`buildWardrobeItemFile`. Projects a `WardrobeItem` to its `Wardrobe/*.md` content:
a YAML frontmatter block (keys in v4's exact insertion order; `componentItemIds`
translated to slugs with a UUID fallback) plus the description body. Per locked
Decision A the YAML is hand-rolled — the emitted bytes feed the content-dedup
SHA, so a quoting mismatch is a silent mis-dedup, not just a test gap. The emitter
is a faithful port of eemeli/yaml 2.9.0's `stringifyString` + `foldFlowLines`
(default options) for the bounded value space (string scalars, the boolean `true`,
block sequences of string scalars): plain/single/double quote selection, the
core-schema reparse-safety quoting (a scalar that would reparse as
number/bool/null is quoted), line folding past width 80, and block scalars
(`|`/`|-`/`>`) for multiline values. It operates on UTF-16 code units throughout
(as JS does) so fold offsets, the control-char force-quote check (matched on code
points, per eemeli's `/u` flag — a valid astral character is not a surrogate
match), and `JSON.stringify` escaping align byte-for-byte. Verified by a tier-1
differential (`vault_wardrobe_emit_equivalence`) against v4's real
`buildWardrobeItemFile` over a 100-item corpus spanning every quoting edge,
folding, block scalars, surrogate-pair fold offsets, the slug/UUID map, and all
flag branches; plus three exact unit tests. This was the last open vault decision;
the only wardrobe write piece still ahead is the stateful folder projection
(`projectVaultWardrobe` — filename dedup/rename/sweep + multi-table writes).

Phase 2 — the character vault **wardrobe read overlay**
(`quilltap-core::db::vault_read_overlay::read_character_vault_wardrobe` +
`quilltap-core::vault_overlay::resolve_and_check_component_items`), v4's
`readCharacterVaultWardrobe`. Enumerates `Wardrobe/*.md` (the Decision-B code-unit
sort, then `parseWardrobeItemFile`, dropping unparseable files), builds the
in-vault slug/id lookup maps (first-claimer wins a slug; every item is addressable
by id), and resolves each item's raw `componentItems:` refs to canonical ids —
slug-first then UUID, unknown refs dropped — before a cycle check that clears any
item whose resolved components form a cycle. The cycle pass reads the **live**
(already-mutated) component lists, so clearing one item mid-pass changes later
items' walks, exactly mirroring v4's mutable `itemById` (proven in the corpus: a
mutual `a → b`/`b → a` cycle clears `a`, then `b` survives because `a` was already
emptied when `b`'s walk ran). An empty/missing `Wardrobe/` folder falls through to
the legacy `wardrobe.json` (`parseLegacyWardrobeJson`); neither present → `null`.
Verified by a read-differential (`vault_wardrobe_read_equivalence`, three cases)
driving v4's REAL `readCharacterVaultWardrobe` over a shared seeded fixture —
slug/UUID/collided-slug/unknown resolution, the live-mutation cycle asymmetry, a
self-cycle clear, an archived item, the legacy fallback, and the empty-vault
`null` — comparing each `{ items } | null` exactly (no normalization; this read
path mints no clock value). Plus four tier-1 unit tests on the resolver.
**Tracked deferral:** the archetype-seeding branch (`findArchetypes` over the
General/project `Wardrobe` stores) is not ported — the corpus keeps no General
store provisioned, so v4's `findArchetypes` returns `[]` and the seed is a
verified no-op.

Phase 2 — the character vault **read overlay** (`quilltap-core::db::vault_read_overlay`),
the heart of the Family-B read path: v4's `hydrateOne` + `applyDocumentStoreOverlay`
+ `applyDocumentStoreOverlayOne`. Folds a character's vault files onto the
character so every read sees vault values transparently. Because the overlay is a
plain JSON merge, the port operates on the character as a `serde_json::Value`
object (not a fully-typed `Character`), patching the managed keys with values from
the already-ported pure parsers: `properties.json` →
pronouns/aliases/title/firstMessage/talkativeness; the five markdown fields
(identity/description/manifesto/personality/exampleDialogues) via
`markdownToNullable` (empty → null); `physical-description.md` +
`physical-prompts.json` → `physicalDescription` (base-reuse when the character
already has one, else a minted base with `stableUuidFromString('physical:<mp>')` +
clock-minted timestamps); `Prompts/*.md` → `systemPrompts` (the Decision-B
code-unit sort + parse + the exactly-one-`isDefault` normalization: keep the first
declared default and demote the rest, or promote the first when none is marked);
`Scenarios/*.md` → `scenarios`. The keystone is `properties.json`: a linked vault
that lacks it is broken — the batched apply DROPS the character (one corrupt vault
can't take down the roster) while the single apply returns an Unavailable error
(v4 throws → 503). Verified by a read-differential
(`vault_read_overlay_equivalence`) driving v4's REAL `applyDocumentStoreOverlay`
over seven input characters against a six-store seeded fixture — pass-through, full
overlay, drop, partial (arrays replaced with `[]`), physical mint, and all three
prompt-default cases — comparing the hydrated characters exactly (only the minted
physical timestamps placeholdered), plus the `…One` throw on the broken vault.

Phase 2 — the vault read overlay's directory-listing load
(`DocMountDocumentsRepository::find_many_by_mount_points_in_folder`), the first
stateful sub-unit of the character read overlay (Family B). Ports v4's
`findManyByMountPointsInFolder`: the 3-table join with a SQL
`LOWER(relativePath) LIKE '<folder>/%'` prefilter, then v4's JS post-filter
(case-folded prefix, non-empty remainder, single-level only — no `/` in the
remainder — and an extension match). The overlay-consumed subset of the row is
returned (`content`/`mountPointId`/`relativePath`/`fileName` + the document
`createdAt`/`updatedAt`); v4's unused `recursive` option is not ported. Verified
by the first **read-differential**: a fixture builder seeds two pinned stores and
writes a corpus via v4's real `linkDocumentContent` (driven directly — not
`writeDatabaseDocument`, whose `QUILLTAP_JOB_CHILD=1` skip-switch reroutes repos
through the forked-child write proxy and breaks `initializeDatabase`); both v4 and
the Rust port then READ the SAME fixture, so minted ids/timestamps are identical
and the returned rows compare exactly (sorted by `(mountPointId, relativePath)`,
the read having no defined order). The corpus covers the IN-clause across two
stores and excludes a top-level file, a nested file, and a wrong-extension file,
plus the empty-mount-point short-circuit (`vault_folder_read_equivalence`).

Phase 2 — the vault `Wardrobe/*.md` parser
(`quilltap-core::vault_overlay::parse_wardrobe_item_file`), the third and last
per-file frontmatter parser. Reuses the title fallback chain (frontmatter `title`
→ first `# heading` → filename-without-`.md`) and the already-ported
`parse_wardrobe_types_field` (a valid `types` list is required, else skip) /
`parse_component_items_field` (raw author refs kept for the overlay's later
resolution pass). Reproduces the id sanity check (`/^[0-9a-f-]{36}$/i` — 36 chars,
hex-or-`-`; otherwise `stableUuidFromString`, incl. a 36-char non-hex id that must
fall back), the non-empty-string fields (`appropriateness`/`imagePrompt`), the
boolean flags (`default || isDefault`, `replace`), the `archivedAt` precedence
(non-empty string wins, else `archived: true` → `doc.updatedAt`), the
`typeof === 'string'` keep of `migratedFromClothingRecordId` (incl. empty), and
the frontmatter-vs-doc timestamp precedence. Output is built directly (not via
Zod), so its nullable fields are ALWAYS present (`null` or value) and a heading
used as the title is dropped from the body (an empty body → `null` description,
NOT a skip). Tier-1 exact differential (`vault_wardrobe_item_file_equivalence`)
over 20 cases against v4's real `parseWardrobeItemFile`.

Phase 2 — the vault frontmatter READ parsers
(`quilltap-core::vault_overlay::parse_prompt_file` / `parse_scenario_file`),
built on the hand-rolled frontmatter reader. Each turns a vault markdown file
into a `CharacterSystemPrompt` / `CharacterScenario`, or `None` (skip — the
overlay falls back to the DB value for that one file). Faithful to v4: the
objects are built directly (not via Zod), so the JS `.trim()` / `.slice(0, n)`
caps are reproduced with the `jsstr` UTF-16 primitives (name ≤100, title ≤200,
description ≤500); `isDefault` is `=== true` (a `"true"` string → false); the
prompt body is the content after the frontmatter, `trimStart`ed; scenario title
resolution is frontmatter `name` → first `# heading` (`/^#\s+(.+)$/` with the JS
whitespace set) → filename-without-`.md`, and a heading used as the title is
dropped from the body while a frontmatter-supplied title leaves the body intact.
Added `jsstr::js_trim_start` and `markdown::body_after` (UTF-16-offset → byte
slice). Tier-1 exact differential (`vault_frontmatter_parsers_equivalence`) over
26 cases against v4's real `parsePromptFile`/`parseScenarioFile`, incl. multibyte
content to cover the UTF-16 body offset and every skip condition.

Phase 2 — the Markdown frontmatter parser + a hand-rolled YAML reader
(`quilltap-core::markdown::parse_frontmatter`), the shared read-path foundation
for the vault's per-file parsers. v4's `parseFrontmatter`
(`lib/doc-edit/markdown-parser.ts`) calls eemeli/yaml's `YAML.parse`; the read
side is the companion to locked Decision A, so this hand-rolls a parser for the
constrained subset our own emitters produce plus simple hand-edits — no YAML
crate in the vault — matching eemeli/yaml's **YAML 1.2 core-schema** output on
that subset. Reproduces the structural logic exactly (the `---\n`-only opener so
CRLF frontmatter isn't recognized; the exactly-`---` closing line; UTF-16
`bodyStartOffset` computed even when the YAML fails to yield an object;
empty/whitespace/comments-only → `{}`; array/scalar root → null; duplicate keys
→ null, since eemeli throws) and the scalar resolution (`~`/`null`/empty → null;
`true`/`false` case-variants → bool while `yes`/`no` stay strings; decimal
int/float → number; ISO timestamps and URLs stay strings; double-quoted
JSON-style escapes incl. `\uXXXX`; single-quoted `''`; the whitespace-gated `#`
comment rule; flow `[a, b]` and block `- item` sequences). Tier-1 exact
differential (`markdown_frontmatter_equivalence`) over 52 cases against v4's real
`parseFrontmatter`. Nested maps, flow maps, block scalars, anchors/tags, and
exotic numbers (hex/octal/exponent/`.inf`/`.nan`) are the documented
out-of-subset seam — kept out of the corpus; they resolve conservatively (a
null/string or a parse error), never to a silently-wrong typed value.

Phase 2 — the legacy `wardrobe.json` migration parser
(`quilltap-core::vault_overlay::parse_legacy_wardrobe_json`), the next
decision-free vault-overlay leaf (Family B). Unlike the two JSON projection
parsers, this validates an array of full `WardrobeItemSchema` items, so it
reproduces Zod 4's `z.uuid()` and `z.iso.datetime()` string formats verbatim
(the regex sources lifted from the live schema: version-nibble `[1-8]` /
variant `[89abAB]` UUIDs plus the all-zero/all-`f` sentinels; ISO dates with
leap-year arithmetic and a `Z`-only zone; JS `\d` rewritten to ASCII `[0-9]`).
Faithful to Zod's rules — any single bad item nulls the whole array; `.default()`
keys (`componentItemIds`/`isDefault`/`replace`) are materialized; output is in
schema order regardless of input key order; unknown keys are stripped (root
`presets`, per-item extras, in-`outfit` extras); and a present `outfit` is
validated (a malformed one fails the parse) then discarded — only `{ items }` is
returned. Tier-1 exact differential (`vault_legacy_wardrobe_equivalence`) over 39
cases against v4's real `parseLegacyWardrobeJson`, covering the valid shapes
(full/minimal-with-defaults/all-nulls/multi/empty/presets-stripped/outfit-valid)
and every interesting violation (bad/missing id, empty/missing title, bad-enum/
empty/non-string types, bad-uuid/non-array/null componentItemIds, non-bool/null
booleans, bad timestamps incl. non-leap `2023-02-29`, offset-zone, no-zone, and
trailing-newline rejection — confirming the `regex` `$` matches JS's absolute-end
anchor).

Phase 2 — the vault JSON projection parsers (`quilltap-core::vault_overlay`), the
next decision-free slice of the character/wardrobe vault overlay (Family B, build
step 6). `parseVaultProperties` + `parseVaultPhysicalPrompts` reproduce v4's Zod
`safeParse`-then-fall-back-to-`null` semantics (`vault-overlay/parsers.ts`): parse
the file JSON, validate against the vault schema, return the typed value or `None`
on a JSON-parse error OR any schema violation. Faithful to Zod's rules — unknown
keys stripped (default `z.object`, top-level and inside `pronouns`); a
`.nullable()` field is required-present (key must exist, value may be `null`) and
serializes `null` when unset; a `.nullable().optional()` field may be absent;
`talkativeness` is range-checked `0.1 ≤ t ≤ 1.0`; the nested `pronouns` fields are
required strings of 1–20 UTF-16 code units. Tier-1 exact differential
(`vault_json_parsers_equivalence`) over 24 cases against v4's real functions
(valid/all-nulls/extra-stripped/invalid-JSON/non-object/missing-key/range-bounds/
non-array-aliases/non-string-element/pronoun-missing-field/too-long/empty/
wrong-type), with integer-valued floats canonicalized on both sides so
`talkativeness: 1.0` (which v4 emits as `1`) compares equal. (`headAndShoulders`
present-`null` is the one tracked null-vs-absent divergence, kept out of the
corpus.)

Phase 2 — the vault write-projection string leaves (`quilltap-core::vault_overlay`),
the next decision-free slice of the character/wardrobe vault overlay (Family B,
build step 6). Five pure functions from v4's `character-vault.ts`:
`slugifyWardrobeTitle` (kebab slug — `toLowerCase` → JS-trim → collapse
non-`[a-z0-9]` runs to `-` → strip ends; the `[^a-z0-9]→-` filter makes it
collation/case-safe, so no ICU per the locked Decision B), `buildSlugByItemIdMap`
(first-wins `(itemId → slug)` list), `sanitizeFileName` (replace `\ / : * ? " < >
|` with `_`, collapse JS-whitespace runs, JS-trim, 100-UTF-16-unit slice,
`untitled` fallback — reusing the existing `jsstr` whitespace/trim/UTF-16
helpers), `buildSystemPromptFile` (the `Prompts/*.md` frontmatter, exercising the
private `escapeYaml` = `if /[:#"'\n]/ then JSON.stringify(v) else v`, reproduced
with `serde_json::to_string` which matches `JSON.stringify` for strings), and
`buildScenarioFile` (plain `# title\n\nbody`, no frontmatter). Tier-1 exact
differential (`vault_string_leaves_equivalence`) over 27 cases against v4's real
functions, incl. unicode→dash slugs, punctuation, the `escapeYaml` quote triggers
(`:`/`#`/`"`/`'`/`\n`), and the empty→`untitled` filename path. Per the locked
decisions, this confirms the prompt/scenario write projections need NO eemeli/yaml
(only `Wardrobe/*.md`, build step 7, does) and the slug path needs no ICU.

Phase 2 — the vault wardrobe-component pure leaves (`quilltap-core::vault_overlay`),
the first slice of the character/wardrobe vault overlay (Family B, build step 6),
ported leaf-first ahead of the stateful overlay so the YAML-emitter and
ICU-collation decisions the *write* path forces are not yet on the critical path.
Three decision-free pure functions: `parseComponentItemsField` (coerce a raw
`componentItems:` value → clean `Vec<String>`: non-arrays → `[]`, trim, drop
empty/non-string), `parseWardrobeTypesField` (validate a `types:` value against
`WardrobeItemTypeEnum` — all-or-nothing, de-dup first-seen, `None` on
empty/invalid), and `detectComponentCycles` (the save-time component-graph cycle
check: direct self-ref, indirect, sub-cycle, diamond-safe, deep-chain). Tier-1
exact differential (`vault_component_leaves_equivalence`) over 22 cases against
v4's real `parsers.ts` / `expand-composites.ts`. No YAML, no
case-mapping/collation — the JSON/array/graph leaves the vault needs, verified
before the projection that consumes them.

Phase 2 — `doc_mount_blobs` (`quilltap-core::db::doc_mount_blobs`), build step 8
of the document-store overlay slice: the document store's **binary** byte-store,
the sibling of the (ported) text store `doc_mount_documents`. Bytes (avatars,
PDF/DOCX content, any non-text) live in a `data BLOB NOT NULL` column keyed UNIQUE
by `fileId`. Unlike the Zod-schema repos, v4 hand-writes this repo and its DDL —
the `data` column is deliberately ABSENT from `DocMountBlobMetadataSchema`
(metadata reads never hydrate the bytes) — so the port reproduces the hand-written
`CREATE TABLE` verbatim (incl. the `FOREIGN KEY (fileId) REFERENCES
doc_mount_files(id)`). Ports `upsertByFileId` (insert-or-replace by `fileId`,
**recomputing `sha256` from the actual bytes** — the caller's sha is advisory —
with `sizeBytes = data.len()`; an existing row overwritten in place) plus the
metadata/`readData`/`delete` accessors. The tier-2 differential
(`doc_mount_blobs_tier2_equivalence`) drives v4's REAL `upsertByFileId` against a
mount-index fixture that seeds the parent `doc_mount_files` rows the FK requires
(enforced under the writable open's `foreign_keys = ON`), and diffs the table with
the `data` BLOB dumped as lowercase hex (bit-exact, mirrors `help_docs` /
`doc_mount_chunks`) in the minted-values remap form (`id` remapped, timestamps
placeholdered; `fileId` pinned, content compared directly). Banks a fresh insert,
an overwrite-in-place on a repeat `fileId`, the sha-recompute rule (every op
passes an all-zero advisory sha), and a non-UTF-8 binary payload (a PNG header +
`deadbeef`) round-tripping through the BLOB. `linkBlobContent` (the
`(mountPointId, relativePath)` content/link split, the binary analogue of
`linkDocumentContent`) remains deferred.

Phase 2 — `stableUuidFromString` (`quilltap-core::vault_overlay`), build step 5
of the document-store overlay slice: the first **character/wardrobe vault** leaf,
ported leaf-first ahead of the stateful vault overlay (Family B). It derives the
deterministic id every folder-enumerated vault entity (system prompts, scenarios,
wardrobe items) carries — `stableUuidFromString('<kind>:<mountPointId>:<relativePath>')`
— which chat references depend on. SHA-256 over the source's UTF-8 bytes → first
16 bytes → version nibble 8 (custom) + RFC-4122 variant → hyphenated lowercase
hex. Tier-1 exact differential (`stable_uuid_equivalence`) against v4's real
function over the `prompt:`/`scenario:`/`wardrobe-item:` prefixed forms, an empty
string, and a non-ASCII path (SHA-256 runs over UTF-8 both sides — the accented
source agrees byte-for-byte; there is no case mapping here, unlike the
`toLowerCase`/`localeCompare` seams).

Phase 2 — the `projects` store-backed entity + the store-backed GENERALIZATION
(`quilltap-core::db::{store_backed, projects}`), build step 4 of the overlay
slice. Generalizes the slim-row plumbing + provisioning that `groups` proved into
a reusable `StoreBackedRepository<E: StoreEntity>` (v4's
`AbstractStoreBackedRepository`): the `StoreEntity` trait gains `slim_table` /
`store_name_prefix` / `find_store_links` / `link_store`, and `ensure_official_store`
becomes generic over `E` (the group/project ensure wrappers collapse into one).
`GroupsRepository` is refactored to a thin wrapper over the generic base (still
green); `projects` is the second instance. `ProjectsRepository` adds the **16-key
`properties.json` bag** (`ProjectPropertiesSchema` — five Zod-`.default` keys
ALWAYS materialized in schema order: `allowAnyCharacter` / `characterRoster` /
`defaultDisabledTools` / `defaultDisabledToolGroups` / `backgroundDisplayMode`; the
other eleven `.nullable().optional()` → `skip_serializing_if`) and the
**character-roster operations** (`addToRoster` / `removeFromRoster` /
`setAllowAnyCharacter` / `canCharacterParticipate` / `findByCharacterId`), each a
`properties.json` read-modify-write through `update` (or an in-memory `findAll`
filter). The tier-2 differential (`projects_tier2_equivalence`) drives v4's REAL
`repos.projects.create`/`.update`/roster ops end-to-end and diffs the same seven
tables across both dbs (the slim `projects` row + the store tables +
`project_doc_mount_links`) in the shared-cross-db-id-map remap form, `chunkCount`
pinned + `doc_mount_chunks` excluded (database-backed reindex uses no model). The
corpus banks a rich create (roster + color + `defaultImageProfileId` +
`backgroundDisplayMode`, the optional keys interleaved with the materialized
defaults in schema order — byte-exact) and a minimal create (only the five
defaults), `addToRoster`/`removeFromRoster` (the `characterRoster` array RMW
preserving the other fifteen keys), `setAllowAnyCharacter` (a bool RMW), and a
DB-only `name` update. The `ensureOfficialStore` step-2 adopt branch stays
deferred (corpus always provisions fresh); the property null-vs-absent +
multi-key insertion-order seam is unchanged (corpus kept to present/absent +
`{}`/single-key `state`).

Docs — the document-store-overlay design slice
(`docs/developer/porting/document-store-overlay.md`): the port plan for the
store-backed entities (`projects`, `groups`, `characters`, the `wardrobe` vault).
Establishes that the "document store" is DB rows in the mount-index DB (text in
`doc_mount_documents`, binary in `doc_mount_blobs`), not filesystem files, so no
filesystem fixture is needed; maps the generic overlay engine
(`createDocumentStoreOverlay` + `AbstractStoreBackedRepository`) shared by projects
and groups vs the heavier character/wardrobe markdown-vault family; sets a
dependency-first build order (port `doc_mount_file_links` + `linkDocumentContent` +
`writeDatabaseDocument` first, then the engine, then `groups` as pilot, then
`projects`); and specifies the tier-2 oracle strategy (drive v4's real storage code
against the existing mount-index fixtures with `QUILLTAP_JOB_CHILD=1`, dump the four
storage tables + the slim row, minted-values remap form). Linked from `overview.md`
and `CLAUDE.md`.

