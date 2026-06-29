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

