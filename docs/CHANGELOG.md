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

