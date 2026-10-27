# Survey B — memory programme + scenarios (P4.D265–D271, D276, D288–D292, D295–D302 + the `83334b9a5` follow-ups)

Read-only. v5 main @ `4e43b714e`; v4 @ `322a5488e` (git show only). Three parts below: **part 3** (P4.D288–D302 headers/Tier 3 + the `83334b9a5` recorded follow-ups, items `C-n`), **part 2** (the OFFSET ruling + the repository-fallback `?` reads, items `A1`, `B1`–`B9`), **part 1** (the `01a83539d`-round orders P4.D265–D271 + P4.D276, items `B1-n`).

**Totals (deduplicated):** OPEN 30 · NEEDS-RULING 4 · CLOSED ~38 · NO-ACTION ~44.

## The ruling already made — `OFFSET_OCCURRED_AT_ROW_SCHEMA`: REFUSE AS v4 DOES → item **A1** (part 2)
- v4: route `app/api/v1/memories/route.ts:109` admits `z.iso.datetime({offset:true})`; admission → `apply.ts:38-68` → `memories.repository.ts:535-549` `create` → `base.repository.ts:350-378` `_create` → `MemorySchema` (`memory.types.ts:115`, `common.types.ts:55` Z-only `TimestampSchema`) → ERROR ×3 (`Data validation failed` / `Error creating entity` / `Error creating memory`, all `collection:'memories'`) → `handleRouteError` 400 `{"error":"Validation error","details":issues}`. Refusal lands AFTER the embedding call (offset + embedding failure = 502).
- v5 paths: admission insert (`services/admission/apply.rs:166`) and consolidation digest (`services/consolidation.rs:875`) UNVALIDATED → fix once in `db/memories.rs:368` `MemoriesRepository::create` via the existing `api/zod_issues.rs:1135` `zod_memory_issues` + a new `DbError` validation variant mapped to 400 in `api/memories.rs::memory_create`. `.qtap` import + restore ALREADY refuse (P4.161). Measure the fold/update arm (v4 `_update` validates the merged row) with a new row before touching `update_for_character`.
- Pin to flip: `crates/quilltap-harness/tests/memories_routes_equivalence.rs:544-556` + `:2069-2084` (delete); oracle row `harness/oracle/cases/memories-routes.test.ts:836-839` needs no regen.

## Rulings still owed (each laid out with options + recommendation in its item)
1. **C-2 `DQS_QUOTED_IDENTIFIER_FALLBACK`** — recommend the RUNTIME `SQLITE_DBCONFIG_DQS_DML/DDL = off` at the connection chokepoints (no sys-crate recompile), gated on a census of v5 SQL using `"string"` literals.
2. **B5 regenerate-all's chat list** (v4: a failed `chats.findByUserId` → `[]` → EVERY memory-holding chat is an "orphan" → N `MEMORY_REGENERATE_CHAT` wipes on the standard profile). Options: A converge (faithful, destructive on a transient failure) / B keep strict + both-ways pin `REGENERATE_ALL_STRICT_CHAT_LIST` (+ optional v4 issue). **Recommend B**; converge the other two reads of that handler regardless.
3. **B6 (= part 1 B1-7) the maintenance corpus batch loader** (`Error loading memory batch`: v4 truncates the corpus at a failing page; v5 fails the run). Recommend keep strict (a truncated corpus plans folds/demotions from a partial view) — part 1 argued converge; rule with B5.
4. **B1-R1** the memory list's v5-only "Source" link — recommend remove (converge).

## Proposed lanes (disjoint file ownership)
- **L-A "memory repository + Memories API"** — A1, B1, B2, B3, B4, B5/B6 (after rulings), C-1 (`find_by_ids` validation), C-5 (`create_project_explicit_wins`), B1-1 (`update_tier_bulk` per-chunk `updatedAt`), B1-3 (list `limit=0`), the stale oracle comment `memories-routes.test.ts:808-811`. Owns `db/memories.rs`, `db/memories_read.rs`, `db/fallback.rs` (a fenced region), `db/mod.rs` (DbError variant), `api/memories.rs`, `services/memory_regenerate_job.rs`, `services/memory_repair.rs`, `services/memory_maintenance/corpus.rs`; families `memories_routes`, `memories_read`, `admission_tier3`, `maintenance_ops_tier2`, `memory_graph_repair_tier2`, `memory_regenerate_jobs_tier3`. (Large; if split, A1 + C-5 + B1-3 + B3's ROUTE rows go to one lane owning `api/memories.rs` + `memories_routes`, the repository conversions to the other — they collide on `memories_routes_equivalence.rs` otherwise.)
- **L-B "scenarios server smalls"** — C-7 (vault parser WARNs), C-9 (`ensure_official_store` step 2), C-10 (`StoreUnavailable` message), C-11 (`reload: null`), C-12 (qtap decoder fold), C-8 (host L37 capture row), optionally C-19 (the startup heals — M–L, planner's call), plus B1-5 (summary bridge WARN keys — same `VaultUnavailable::message` theme, disjoint file). Owns `vault_overlay.rs`, `db/vault_read_overlay.rs`, `db/ensure_official_store.rs`, `db/characters_read.rs` (message only), `api/types.rs` (the `reload` field only), `doc_edit/qtap_uri.rs`, `quilltap-host/src/host.rs` (extracted arm), `services/conversation_summary_vault_bridge.rs`; families `scenario_location`, `chat_scenario_routes`, `p4_d288_wire_contract`.
- **L-C "instance lock"** — C-6. Owns `quilltap-host/src/lock.rs` + `instance_lock_port_equivalence` / `instance-lock-port.ts` + `host_lock_ordering.rs`. (If L-B takes C-8, both touch `quilltap-host` but different files.)
- **L-D "recall + extraction harness rows"** (harness-mostly) — C-3 (NEW `recall_probe_fixture_equivalence`), C-4 (text-expansion jest twin, `services/recall/select.rs` tests), B1-4 (hybrid floor 0.75 row), C-13 (Carina project row — Carina builder/pair), C-14 (dangerous-route + turn-grain `llm_logs` rows — pipeline pair), C-17 (about=holder census, docs).
- **L-E "SPA lows"** — C-16 (createMemory `action?`, read-only `config`, provenance whitespace), C-15 (frame-error toast duration + the `STREAM_FAILURE_ERROR` const/guard — its small Rust half `spine.rs` / `api/types.rs` const), B1-8 (`getErrorMessage` → `core/error-utils.ts`), B1-R1 (after ruling).
- **Standalones:** B1-2 `services/frozen_archive.rs` (lock-free compute); C-2 `db/mod.rs` DQS (after ruling — touches the connection chokepoint, and `db/mod.rs` is also L-A's for the DbError variant → sequence or give C-2 to L-A); B1-6 `[CheapLLM]` lines + the duplicated `build_cheap_llm_selection` → give to whoever owns `CHEAP_LLM_KEY_ORDER` (`cheap_llm.rs`, another slice).
- **Optional / recommend NO-ACTION:** C-18 (D288 cosmetics).

---

# Survey B part 3: P4.D288–P4.D302 headers + Tier 3, and the `83334b9a5` round's recorded follow-ups

v5 main @ `4e43b714e`; v4 @ `322a5488e` via `git show`.

## OPEN

### C-1 — `FIND_BY_IDS_UNVALIDATED`: `find_by_ids` does not drop schema-invalid rows (v4 `validateSafe` does)   [OPEN]
- Source: P4.D290 lane record (status-log ~180786, 180902 "FINDING for the human"); pinned both ways in `admission_tier3_equivalence.rs:89-98` (`invalid_row_neighbour`).
- v5 site: `crates/quilltap-core/src/db/memories_read.rs:506-517` `find_by_ids` → `query_memories` with no row-schema check; a non-UUID / schema-invalid row stays a LIVE neighbour, so bug 200's orphan widen in `services/admission/neighbours.rs` is unreachable except by race.
- v4 site @322a5488e: `lib/database/repositories/memories.repository.ts:210-225` `findByIds` → base `findByFilter` → `validateSafe` per row against `MemorySchema` (silent drop).
- Fix shape: run `api::zod_issues::zod_memory_issues` per hydrated row inside `find_by_ids` (or the `findByFilter`-class reader `query_memories`) and drop refused rows silently (v4's `validateSafe` drops without a line — confirm at `base.repository.ts`). Measure blast radius first: every `query_memories` caller (it would also drop the offset-`occurredAt` rows C-0/A1 now refuses on write — consistent with v4). Retire the `FIND_BY_IDS_UNVALIDATED` pin (both sides then orphan).
- Differential: `admission_tier3_equivalence` (`invalid_row_neighbour` flips to strict), `memories_read_equivalence`.
- Files a lane would own: `db/memories_read.rs` (shared with the repository-fallback lane B1–B6 → SAME lane), `crates/quilltap-harness/tests/admission_tier3_equivalence.rs`.
- Size: M

### C-2 — `DQS_QUOTED_IDENTIFIER_FALLBACK`: v5's SQLite keeps the double-quoted-string fallback; v4 builds `SQLITE_DQS=0`   [NEEDS-RULING]
- Source: P4.D288 unit 10 (status-log 180379, "FLAGGED for a ruling"); header OPEN "awaits the human". Memory note `sqlite-dqs-build-difference`.
- v5 site: `crates/quilltap-sqlite3mc-sys/build.rs:53-64` sets no `SQLITE_DQS`; pinned both ways at `crates/quilltap-harness/tests/cursor_bundle_equivalence.rs:60-79` (4 rows) + `:257-287`.
- v4 site: `better-sqlite3-multiple-ciphers@12.11.1` `deps/defines.gypi:17` `SQLITE_DQS=0` → `no such column: "chatId" - should this be a string literal in single-quotes?`.
- Options: (a) **runtime, no recompile** — `conn.set_db_config(DbConfig::SQLITE_DBCONFIG_DQS_DML, false)` + `DQS_DDL` (rusqlite 0.37 exposes both, `config.rs:49,53`) at the connection chokepoints (`db/mod.rs` `Writer::open_writable*` ~`:324`, `runtime::Db::open`; it is a db_config call, not a pragma, so the "key is the first and only pragma" rule holds — but put it after `PRAGMA key`). Needs a census of v5 SQL that relies on `"literal"` strings (grep `= "` inside SQL text) — any such query would start erroring. (b) compile-time `.define("SQLITE_DQS","0")` — forces the 4-min amalgamation recompile once and breaks the sys-crate cache invariant; (c) keep and record as ruled (like `STAT4_NOT_COMPILED`).
- Recommendation: (a) — v4-faithful failures on a missing column (the strict-scope WARN arms fire), no sys-crate change; gate it on the SQL census.
- Differential: `cursor_bundle_equivalence` (retire the DQS table — the 4 rows compare strict); full workspace gate (any red = a v5 query using a DQS literal).
- Files: `crates/quilltap-core/src/db/mod.rs` (+ `db/runtime.rs` if separate), `crates/quilltap-harness/tests/cursor_bundle_equivalence.rs`, the census test.
- Size: S (code) / M (census + gate)

### C-3 — P4.D291 #24: `recall_probe_fixture_equivalence` (the whole-pipeline regression over v4's probe set)   [OPEN]
- Source: P4.D291 Tier 2 item 24 (R-S), header OPEN "unstarted"; the `83334b9a5` NEXT 2.
- v5 site: no such family (`ls crates/quilltap-harness/tests | grep recall_probe` empty). Pipeline: `crates/quilltap-core/src/services/recall/{search,score,select,deliver}.rs`.
- v4 site @322a5488e: `__tests__/fixtures/recall-probes/{probe-set.json,harness.ts,README.md}`, `__tests__/unit/lib/memory/recall/probe-fixture.test.ts`.
- Fix shape: exactly as the order's item 24 spells (`work-orders/p4.d291-*.md` item 24): a loader reads `$QT_V4_ROOT/__tests__/fixtures/recall-probes/probe-set.json` at test time (SKIP with reason when unset), remaps non-UUID ids → deterministic RFC UUIDs (memory note `memory-raw-reads-drop-non-uuid-fixtures`), plants 64-d vectors as Float32 LE blobs (`memory-search-plants-need-vectors`), stubs the 15-entry embedding table as a canned provider, runs `search_recall` per turn (A, B, C, F1, F2, M, O2, O3, O4); oracle `harness/oracle/cases/recall-probe-fixture.test.ts` reusing v4's `probeWorld` + mocks, emitting per-turn candidates + trace; 1e-12.
- Differential: NEW `crates/quilltap-harness/tests/recall_probe_fixture_equivalence.rs` + `harness/oracle/cases/recall-probe-fixture.test.ts`.
- Files: those two (new) only — disjoint.
- Size: M–L

### C-4 — P4.D291 #23's one unpinned leg: "never expands a text search"   [OPEN]
- Source: P4.D291 header OPEN (item 23's leg).
- v5 site: `crates/quilltap-core/src/services/recall/select.rs:33-38` `wants_expansion` (`strategy == Vector && expand_related`), used `:60` and `search.rs:129`; no test row has a TEXT strategy with `expand_related: true` + a related neighbour.
- v4 site @322a5488e: `__tests__/unit/lib/memory/recall/recall-stages.test.ts:108-113` `it('never expands a text search')`.
- Fix shape: add the transcribed jest twin to `select.rs`'s test module (or the tier-1 recall family) — a seed with `relatedMemoryIds: ['nb']`, text strategy, `recallContext` asking for expansion → `expanded == 0`. Mutation-prove by dropping the `strategy == Vector` conjunct.
- Differential: unit-only jest twin (pure).
- Files: `crates/quilltap-core/src/services/recall/select.rs` (tests).
- Size: S

### C-5 — `create_project_explicit_wins` row (no project store in the committed pair)   [OPEN]
- Source: P4.D298 header OPEN; order item 13 row list (`p4.d298-*.md:685`).
- v5 site: `crates/quilltap-core/src/api/memories.rs` `memory_create` (project resolution, explicit `projectId` over the chat's); harness `crates/quilltap-harness/tests/memories_routes_equivalence.rs:1931` lists only `create_project_derived_from_chat`.
- v4 site @322a5488e: `app/api/v1/memories/route.ts` POST handler (explicit `projectId` → `repos.projects.findById`; 404 `Project not found`).
- Fix shape: an in-case plant on both sides of a project row + its official store (properties.json) — the `jest-oracle-in-case-plants` pattern; do NOT grow the shared `memories-{main,mount}.db` pair (other families read it). Row: chat in project P1, body `projectId: P2` → the stored row carries P2.
- Differential: `memories_routes_equivalence` + `harness/oracle/cases/memories-routes.test.ts`.
- Files: those two harness files (shared with A1 + B3 rows — same lane).
- Size: M

### C-6 — the instance lock: lenient `read_lock_file`, v4's lock lines, the `module` binding   [OPEN]
- Source: P4.D299 header OPEN (pre-existing, review) + Tier 3 #17, #18 (R-L5).
- v5 site: `crates/quilltap-host/src/lock.rs`:
  - `:107-130` `LockFileContent` requires every field; `environment: EnvironmentType` is a closed enum; `:446-453` `read_lock_file_raw` returns `None` on any serde failure, silently (a lock missing `processArgv0` is `Corrupt`).
  - `:672-…` `acquire_instance_lock`: no `Instance lock acquired` INFO (clean create), no `Stale instance lock detected, claiming` WARN, no `Instance lock acquired after stale claim` INFO.
  - `:826,835,876,899` (heartbeat lost / taken over / not-owned / released) lack `module = "database:instance-lock"` (the `:419,427,685,691` lines carry it).
- v4 site @322a5488e `lib/database/backends/sqlite/instance-lock.ts`: `:34` `moduleLogger = logger.child({ module: 'database:instance-lock' })`; `:396-424` `readLockFile` validates ONLY `pid` number / `hostname` string / `history` array → WARN `Lock file has invalid structure, treating as corrupt {lockPath}`; non-ENOENT read failure → DEBUG `Could not read lock file, treating as absent {lockPath, error}`; `:553-560` WARN `Stale instance lock detected, claiming {lockPath, stalePid, staleHostname, staleEnvironment, staleStartedAt, reason}`; `:579-582` INFO `Instance lock acquired after stale claim {lockPath, pid}`; `:617-621` INFO `Instance lock acquired {lockPath, pid, environment}`.
- Fix shape: make the typed view lenient (only `pid`/`hostname`/`history` required; the rest `#[serde(default)]`, `environment` an open string-backed enum keeping unknown values) with v4's two read lines; add the three acquire lines in v4's order and field sets; add `module` to the four lines. Keep the raw-map rewrite (R-L1).
- Differential: `crates/quilltap-harness/tests/instance_lock_port_equivalence.rs` + `harness/oracle/cases/instance-lock-port.ts` (add rows: a lock missing `processArgv0`, an unknown `environment`, a stale claim, a clean acquire — capture lines); `crates/quilltap-host/tests/host_lock_ordering.rs`.
- Files: `crates/quilltap-host/src/lock.rs`, the lock harness pair.
- Size: M

### C-7 — the character vault parser's two skip WARNs (the "scenario parser WARNs", P4.D295 S4)   [OPEN]
- Source: the `83334b9a5` unification record "the scenario parser WARNs (P4.D295 S4)" (status-log 184932). Identified by reading both parsers: the store-tier parser (`db/scenarios.rs:170-178`, L13; `:288-296`, L14) is complete; the VAULT parser is not.
- v5 site: `crates/quilltap-core/src/vault_overlay.rs:983-1059` `parse_scenario_file` — `:1032` `return None; // no usable title → skip` and `:1044` `return None; // empty body → skip`, both silent; the fn takes no `character_id`.
- v4 site @322a5488e: `lib/database/repositories/vault-overlay/parsers.ts:594` `parseScenarioFile(doc, characterId)`; `:640-645` WARN `Scenarios/*.md file has no title; skipping {characterId, mountPointId, relativePath}`; `:658-663` WARN `Scenarios/*.md body is empty; skipping {characterId, mountPointId, relativePath}`.
- Fix shape: add `character_id: &str` to `parse_scenario_file` and thread it from its three callers (`db/vault_read_overlay.rs:196`, `:223` — `parse_scenario_files_sorted` — and `scenarios/location.rs:210`); emit both WARNs with v4's fields in order.
- Differential: `crates/quilltap-harness/tests/scenario_location_equivalence.rs` (character location arm) or the vault read-overlay family — plant a whitespace-body `Scenarios/x.md` and a frontmatter-only nameless file in a character vault; capture lines.
- Files: `crates/quilltap-core/src/vault_overlay.rs`, `crates/quilltap-core/src/db/vault_read_overlay.rs`, `crates/quilltap-core/src/scenarios/location.rs` (one call), one harness family.
- Size: S

### C-8 — P4.D295 HANDOFF (1): the host L2 → L37 capture row   [OPEN]
- Source: P4.D295 header OPEN; status-log 182747-182750, 184928-184930. The comment half LANDED (`host.rs:1961-1967` now names L2 → L37).
- v5 site: `crates/quilltap-host/src/host.rs:1968-1975` (the `ensure_general_scenarios_folder` Err arm → WARN `Error ensuring general scenarios folder, continuing startup {context, error}`). Not reachable from a static plant: phase 3.4's built-in seed creates `Scenarios/` first and its failure is FATAL.
- v4 site @322a5488e: `instrumentation.ts:809-819` (L37); `lib/scenarios/location.ts:412` (L2).
- Fix shape: extract the arm into a small `fn warn_general_scenarios_failure(e: &DbError)` (or make `seed_built_ins` take the ensure as a parameter) and capture-test it with a forced `Err`, asserting L2 then L37 order with the location's real failing ensure over a plant that fails only the folder write (e.g. a `BEFORE INSERT` trigger on the folder table added AFTER the seed in the test). No fault-injection env var in production.
- Differential: host unit/capture test (`crates/quilltap-host/tests/`, `global_capture` per memory note `capture-rig-writer-thread`).
- Files: `crates/quilltap-host/src/host.rs` (the extracted fn only), one host test.
- Size: S

### C-9 — P4.D295 HANDOFF (3): `ensure_official_store` step 2 (adopt a still-linked store)   [OPEN]
- Source: P4.D295 lane record (status-log 182702-182708; HANDOFF (3) at 182915-182917).
- v5 site: `crates/quilltap-core/src/db/ensure_official_store.rs:27-31` (module header "tracked deferral"), `:117-119` (the skipped branch) — v5 MINTS a new store where v4 adopts.
- v4 site @322a5488e: `lib/mount-index/ensure-official-store.ts:102-125` — `findLinks(entityId)` → eligible `database`/`documents` stores → prefer `isOwnStoreName` → raw FK write → INFO `Adopted existing linked store as ${entityLabel} official {<idKey>, mountPointId, storeName}`.
- Fix shape: implement step 2 over the existing `E::find_store_links` seam (raw FK write, no overlay re-read), with v4's INFO. Reachable through every scenario write route, both create hooks, and the union.
- Differential: `crates/quilltap-harness/tests/scenario_location_equivalence.rs` — drop the fixture's BARE-store UNLINK and add the adopt row (both sides adopt).
- Files: `crates/quilltap-core/src/db/ensure_official_store.rs`, the scenario_location pair (+ its fixture builder).
- Size: M

### C-10 — P4.D295 HANDOFF (4): `characters_read::find_by_id`'s `StoreUnavailable.message` is a v5 sentence   [OPEN]
- Source: P4.D295 lane record (status-log ~182800-182806).
- v5 site: `crates/quilltap-core/src/db/characters_read.rs:289-297` — `message: format!("applyDocumentStoreOverlayOne: vault unavailable for character {} (mount {})", …)`.
- v4 site @322a5488e: `CharacterVaultUnavailableError` message `Character <id> has no usable vault (characterDocumentMountPointId=<mp>): properties.json missing` — already ported as `db/vault_read_overlay.rs:75-80` `VaultUnavailable::message()`.
- Fix shape: `message: u.message()`. Then the chat-scenario helper rendering L28/L31's `error` becomes the identity (delete it). Measure: other 503 bodies that surface this message may move (grep `StoreUnavailable` renderers).
- Differential: `chat_scenario_routes_equivalence` (L28/L31 rows) + any 503-body families the measurement finds.
- Files: `crates/quilltap-core/src/db/characters_read.rs`, the chat-scenario helper. Shares the `VaultUnavailable::message` theme with B1-5 (summary bridge) — same lane is natural.
- Size: S

### C-11 — `RELOAD_NULL_DECODES_ABSENT`: `reload: null` is accepted (200 + write) where v4 400s   [OPEN — converge]
- Source: P4.D295 HANDOFF (5) (STOP-and-flag: a C2 wire-shape change); pinned both ways at `crates/quilltap-harness/tests/chat_scenario_routes_equivalence.rs:160-170`.
- v5 site: `crates/quilltap-core/src/api/types.rs:3819` `#[serde(default)] reload: Option<serde_json::Value>` — a JSON `null` decodes to `None`, so the floor's `Some(Null)` refusal (unit-pinned in `chat_scenario`) never sees it.
- v4 site @322a5488e: the chat `?action=scenario` body `reload: z.boolean().optional()` (not nullable) → flat 400 `Validation error`, nothing written.
- Fix shape: decode `reload` tri-state through the house double-option decoder (`db/serde_tristate.rs:20` `double_option`; `api/types.rs:4678` has a local twin; `crates/quilltap-web/tests/tri_state_edges_share_the_decoder.rs` pins the edges) so `null` reaches the floor as present-null → 400; same on the REST edge. Retire the pin.
- Differential: `chat_scenario_routes_equivalence` (`verb_reload_null_refuses` flips strict) + `crates/quilltap-web/tests/p4_d288_wire_contract.rs` decode row.
- Files: `crates/quilltap-core/src/api/types.rs` (one field), the chat-scenario edge in `quilltap-web`, the two tests.
- Size: S

### C-12 — L-6 fold: `doc_edit/qtap_uri.rs`'s private `decode_uri_component` duplicates `scenarios::decode_uri::js_decode_uri_component`   [OPEN]
- Source: P4.D295 Tier 3 #22.
- v5 site: `crates/quilltap-core/src/doc_edit/qtap_uri.rs:138-~200` (`fn decode_uri_component -> Result<String, ()>`); `crates/quilltap-core/src/scenarios/decode_uri.rs:41` (V8's `Decode`, `UriMalformed`).
- v4 site: both call JS `decodeURIComponent`.
- Fix shape: delete the private twin; map `UriMalformed` → the existing `Malformed percent-encoding in qtap:// URI segment` error at `:204`. Prove byte-equal behaviour first over the qtap tests (V8's decoder is the stricter, faithful one; any difference is a qtap fix).
- Differential: unit-only (the `qtap_uri` tests + a lone-surrogate / overlong row).
- Files: `crates/quilltap-core/src/doc_edit/qtap_uri.rs` (and `scenarios/decode_uri.rs` if moved to a shared module).
- Size: S

### C-13 — Carina's project (bug 222) + the `pass: carina` provenance have no differential row   [OPEN]
- Source: P4.D296 header OPEN; lane record status-log 181627-181629.
- v5 site: `crates/quilltap-core/src/services/extraction/context.rs` (threads `project_id` / description to every unit); every Carina corpus chat has `projectId: null`.
- v4 site @322a5488e: `lib/memory/extraction/context.ts` + the Carina pass (`pass: 'carina'` provenance).
- Fix shape: grow `harness/oracle/fixtures/build-carina-memory-extraction-fixture.ts`'s spec with one chat in a project (with a description) and assert the stored rows' `projectId` + the prompt's project block + `pass: carina`. Regenerate the pair (shared-fixture rules: mask minted ids).
- Differential: `carina_memory_extraction_tier3_equivalence` + its oracle.
- Files: the Carina builder/spec, `crates/quilltap-harness/tests/carina_memory_extraction_tier3_equivalence.rs`, the committed Carina pair.
- Size: M

### C-14 — the Unmoderated-route (`isDangerousChat`) and turn-grain extraction `llm_logs` rows   [OPEN]
- Source: P4.D296 header OPEN; status-log 181630-181631.
- v5 site: the extraction route's `is_dangerous_chat` (carried, no corpus row); `memory_pipeline_jobs_tier3` compares its own `llm_logs` but not the turn grain's extraction rows.
- v4 site @322a5488e: `lib/memory/extraction/context.ts` dangerous-route selection; extraction `llm_logs` writes.
- Fix shape: two rows in `memory_pipeline_jobs_tier3` — a chat flagged dangerous routed to the uncensored profile (assert `connectionProfileId` + log row), and a turn-grain job's `llm_logs` dump compared.
- Differential: `crates/quilltap-harness/tests/memory_pipeline_jobs_tier3_equivalence.rs` + its oracle/fixture (`build-memory-pipeline-jobs-fixture.ts`).
- Files: those (shared with C-13 only via no file — disjoint).
- Size: S–M

### C-15 — P4.D301: the 8-second toast on the v5-only resolved-dispatch frame-error path, and the duplicated `Failed to generate response` literal   [OPEN]
- Source: P4.D301 header OPEN.
- v5 sites: `apps/web/src/app/screens/salon/salon-conversation.ts:~4040` `if (frameError) this.toasts.showError(frameError, SEND_FAILURE_TOAST_MS)` (a path v4 has no twin of); `apps/web/src/app/core/send-failure.ts:31` `STREAM_FAILURE_ERROR = 'Failed to generate response'` duplicates `crates/quilltap-host/src/spine.rs:2492` `error: "Failed to generate response".to_string()` (doc: `api/types.rs:6135`).
- v4 site @322a5488e: `app/salon/[id]/hooks/useSSEStreaming.ts:240` `SEND_FAILURE_TOAST_MS = 8000` (send-failure catch only, `:1244-1247`); other error toasts use `showErrorToast`'s default duration.
- Fix shape: (a) the resolved-dispatch frame-error toast uses the default duration (it is not a failed send — no restore); (b) put the Rust literal behind a `pub const STREAM_FAILURE_ERROR` in `api/types.rs` used by `spine.rs`, and add a source-text guard test asserting the SPA constant equals the Rust one (cross-language: the guard is the only DRY available).
- Differential: SPA vitest (`salon-conversation` send-failure spec) + a small harness/source guard.
- Files: `salon-conversation.ts` (one line), `core/send-failure.ts` (doc), `crates/quilltap-host/src/spine.rs`, `crates/quilltap-core/src/api/types.rs`, one guard test.
- Size: S

### C-16 — SPA lows: P4.D300 + P4.D302 cosmetics   [OPEN]
- Source: P4.D300 header OPEN (low); P4.D302 header OPEN (cosmetic).
- v5 sites:
  - `apps/web/src/app/memory/memory.api.ts:93-102` `createMemory` returns `action: data['action'] as AdmissionActionKind` (non-optional) though its doc (`:85-91`) says it may be absent; `memory-editor.ts:189-191` re-widens it by hand. Fix: `action?: AdmissionActionKind` on `MemoryCreateReply`, drop the re-widen.
  - `apps/web/src/app/screens/settings/memory/memory-settings-document.ts:22-23,89` exports `config: WritableSignal<T>`; no external writer (`grep '.config.set('` hits only `memory-extraction-grain-card.ts`, its OWN signal). Fix: `config: Signal<T>` via `config.asReadonly()`.
  - `apps/web/src/app/chat/sidebar/chat-scenario-control.ts:129-130` — the provenance div's leading whitespace before `From:` (the node `getByText(/^From:/)` tripped on). Fix: start the text immediately after the `<div …>`.
  - (P4.D302 "four jsdom files transcribed" — a lane-record wording only; the header already records the correction → NO-ACTION.)
- Differential: unit-only (SPA vitest/lint; the scenario-browser Playwright file still green).
- Files: the three SPA files above (+ specs). Disjoint from B1-8/B1-R1's memory-list files except `apps/web/src/app/memory/` directory — group all SPA lows in ONE SPA lane.
- Size: S

### C-17 — P4.D290 Tier 3 #21 (LR-8): the "about = holder" reader census   [OPEN]
- Source: P4.D290 order LR-8 (`p4.d290-*.md:366-370`, Tier 3 #21); status-log 180900.
- v5 site: after a name-presence flip `services/admission/apply.rs` `insert_row` stores `aboutCharacterId` = the HOLDER (v4-faithful, tier-3 pinned). Unmeasured: which readers treat `about == holder` vs `about IS NULL` differently (recall scope / self-inventory / inter-character pools / the SPA's about label).
- v4 site @322a5488e: `lib/memory/admission/apply.ts` (`aboutCharacterId || null`) + the readers.
- Fix shape: a measurement-only census — grep every `aboutCharacterId` reader in v4 at the pin and v5, tabulate NULL-vs-holder handling, record matches; any v5-only difference becomes a row. Likely no code.
- Differential: record in status-log; rows only if a divergence is found.
- Files: none (docs) unless a divergence is found.
- Size: S

### C-18 — the D288 recorded cosmetics: the rezone's second zone scan, the recompute's strict numeric reads   [OPEN — optional / recommend NO-ACTION]
- Source: P4.D288 header OPEN ("recorded, cosmetic / unreachable from v4-written rows").
- v5 site: `crates/quilltap-core/src/db/memory_rewrite_heals.rs:107-112` (`r.get(1)?` etc. — a TEXT `importance` cell fails the boot ensure where JS coerces); `:153-175` `instance_memory_time_zone` re-scans `chat_settings` separately from the memory calendar's own refresh.
- v4 site @322a5488e: `migrations/scripts/recompute-reinforced-importance-v1.ts:67-93` (`row.importance ?? 0.5` over JS values); `rezone-legacy-memory-occurred-at-v1.ts`.
- Fix shape: read the three numerics as `rusqlite::types::Value` → JS `Number()` coercion (`jsnum::number_from_str`); leave the zone scan (cosmetic). Unreachable from any v4-written row — recommend NO-ACTION.
- Differential: `p4d288` ensure family (a TEXT-importance plant) if done.
- Files: `crates/quilltap-core/src/db/memory_rewrite_heals.rs`.
- Size: S

### C-19 — P4.D295 R-Y: v4's PHASE 3.4 / 3.4b startup heals (`ensureProjectScenariosForAllProjects`, `ensureGroupScenariosForAllGroups`, `backfill-group-stores`)   [OPEN — deferred feature, not a small]
- Source: P4.D295 Tier 3 #19 (the P4.4 deferral; planner decision "the startup heals deferred", `phase-4.md:8043`).
- v5 site: `crates/quilltap-host/src/host.rs:~1955-1975` runs only 3.4c (General); no project/group loop.
- v4 site @322a5488e: `instrumentation.ts:761-800` (3.4 / 3.4b), `lib/startup/ensure-project-scenarios.ts:77` WARN `ensure-project-scenarios: failed for project; continuing`, `lib/startup/ensure-group-scenarios.ts:80`, `{total, provisioned, healed, failed}` counts + L34–L36.
- Fix shape: port both loops over `resolve_scenario_location(..., ensure)` with v4's counts/lines; pairs naturally with C-9 (step 2 adoption is what the backfill exercises).
- Differential: a host boot family (planted projects/groups lacking stores / `Scenarios/`).
- Files: `crates/quilltap-host/src/host.rs`, new `services/startup_scenarios.rs` (or similar), `ensure_official_store.rs` (shared with C-9 → same lane).
- Size: M–L (flag to the planner whether it belongs in a smalls round)

## CLOSED

- **ADMISSION_SINGLE_TERM_LINK** — retired at the `83334b9a5` gate (v4's process-singleton index cache; with `reloadAll` per case v4 links `f1`): `memories_routes_equivalence.rs:2064-2068` compares the row strictly. Leftover: a STALE oracle comment `harness/oracle/cases/memories-routes.test.ts:808-811` ("MEASURED divergence … The Rust side pins it as `ADMISSION_SINGLE_TERM_LINK`") — a one-line doc fix for whichever lane owns that file (A1's).
- **CREATE_KEY_ORDER_RESIDUE** — fixed: `services/admission/apply.rs:43-55` `without_embedding` uses `shift_remove`; `apply::with_key` inserts at schema position.
- **P4.D290 HANDOFF (2) `run_anchor_gate_probe_in_zone` threading** — landed: `api/memories.rs:2380,2466` calls `run_anchor_gate_probe_in_zone(db, provider, &input, host_zone_fallback)`; `anchor_gate_probe.rs:500-519`.
- **P4.D290 / P4.D297 "stale docs naming `plan_memory_merge` / `PARTIAL_DEFAULTS_CLOBBER`"** — the remaining mentions (`api/memory_consolidation.rs:15`, `memory_consolidation_routes_equivalence.rs:15`, `memory_merge_plan_equivalence.rs:17`, `admission/fold.rs:16`) are accurate past-tense retirement history; no live claim.
- **P4.D292 `lexical_text_writers_guard` forward-only window** — fixed at unification (bidirectional window + embed-call gate): `crates/quilltap-harness/tests/lexical_text_writers_guard.rs:40-52`.
- **P4.D295 HANDOFF (2)** dispatch-census scenario `note` re-citations — done ("P4.D288 re-cited at `83334b9a5`", `crates/quilltap-web/tests/dispatch_wrong_type_census.rs:756-822`).
- **P4.D295 HANDOFF (6)** `p4_d288_wire_contract` refusal rows — dropped at unification (the file now decodes the P4.D295 verbs, `p4_d288_wire_contract.rs:102-133`).
- **P4.D288 dead `p4d288_not_yet_available` scaffold, Almanack dropped-column lines, paginated / `findByImportance` filters, NULL `reinforcedImportance`** — fixed at unification (header).
- **"the Almanack / API-doc smalls"** — the Almanack's four `autoHousekeepingSettings` lines → v4's `Memory maintenance` line (now in `almanack/render.rs`); the stale `api/types.rs` memory-variant docs refreshed (HANDOFF (d)). Nothing further recorded under this name.
- **P4.D296 three `?` reads / P4.D290 + P4.D297 `?` reads** — see part 2 (B1–B6); the in-loop fold zone + `pass_cap` + `js_string` fixed at unification.

## NO-ACTION

- **P4.D298 `[Memories API] Error listing memories`** — DEAD in v4: `app/api/v1/memories/route.ts:224-246` `return listMemoriesByCharacter(...)` (an `async` fn, `:301`; likewise `:419`, `:438`) inside `try` WITHOUT `await`, so a rejection bypasses the catch — the ERROR at `:245` can only fire on a synchronous throw, which none of the three helpers can produce. Nothing to port (record the measurement; part 2's B8 is superseded by this).
- **P4.D289 `AdmissionRow` struct-order serialisation** (`admission/types.rs:20-66` vs v4 `lib/memory/candidate.ts:259-279`) — never serialised in production (no `to_value` of a row in `admission/` or `services/admission/`); the harness compares key-set + values. Optional zero-risk reorder (characterId, aboutCharacterId, chatId, projectId, content, summary, keywords, tags, importance, source, sourceMessageId, sourceMessageTimestamp, witnessedContext, occurredAt, narrativeTime, entities, kind).
- **P4.D289 `instanceTimeZone="null"`** (`services/memory_time_zone.rs:115`) — the house idiom for a JSON-null field in a tracing line (`db/scenarios.rs:290` etc.); cosmetic.
- **P4.D289 duplicated JS helpers / P4.D290 duplicated JS-truthiness helpers** — `admission/decide.rs:97`, `admission/fold.rs:235`, `services/memory_maintenance/run.rs:752`, `services/admission/apply.rs:68` (+ ~15 more outside memory). A DRY fold onto `inventory/types.rs:187` `js_truthy` / a `jsnum`/`jsstr` home is possible (S) but crosses every subsystem's files — only worth it as its own lane; behaviour-neutral.
- **P4.D289 `zoned_time_to_utc_ms` without TimeClip** (`memory_time/calendar.rs:193`) — unreachable (bounded years).
- **P4.D289 `FoldSource::from_value` drops non-string link ids** (`admission/fold.rs:94-122`) — unreachable: v4's rows are `MemorySchema`-parsed (uuid strings).
- **P4.D289 Tier 3 #15–#16** — shims deleted at §S.5; routed items landed. **#17 `ZONELESS_PARSE_AS_UTC`** — a RULED divergence (host-zone discipline).
- **P4.D288 Tier 3 #19–#20** — NO-PORT (runner lines, `PRETTY_LABELS`, bare-`delete` ERROR, `withLoadedStore` WARN, `normalize-vector-storage`, `patchOnlyFields`, job-child `announceRetirement`).
- **P4.D290 Tier 3 #19** NO-PORT (job-child admission); **#20** landed (§R.10(h)).
- **P4.D291 Tier 2 #22** — proof-strength only (every rule has a green row by name). **The two `[Recall] … (non-fatal)` WARNs** (v4 `lib/memory/recall/assemble.ts:561,610`) — v5's `search_vault_conversation_summaries` (`services/memory_recap/mod.rs:178`) is infallible, so the catch has nothing to catch (recorded unreachable). **Tier 3 #26/#27/#28/#31** landed (§S.5 / P4.D298 / P4.D299). **#29** Phase-2 memory compression (`compressMemories`) stays the tracked standing deferral (`build_context.rs:79-84`) — feature-sized, not a small. **#30** swipe's missing recall record + `human_keywords` drop — v4-faithful; candidate v4 GitHub issues (draft + human go-ahead). **#32** help landed (P4.D303).
- **P4.D292 Tier 3 #20–#22** — NO-PORT / no-code (debounce without a host; `processSingleton`; the neural probe path is P4.D290's).
- **P4.D295 Tier 3 #20** NO-PORT (typed dispatch); **#21** carriers landed (P4.D293 / P4.D294).
- **P4.D296 R-W** (Tier 3 #24) — the extraction dry run + `memory-diff`: loud refusals stand (feature-sized). **The Concierge-policy + cost-event WARNs** — infallible seams in v5, no home.
- **P4.D297 Tier 3 #26** `Response::MemoryMaintenance` rename — deferred by R-J. **#27** NO-PORT. **#28** CLI `memories maintain/status/validate --repair` — loud refusals stand (the `quilltap memories` family is feature-sized; see part 1).
- **P4.D298 Tier 3 #22–#25** — recorded, no change.
- **P4.D299 Tier 3 #16** (memory-diff port resolution, `memories` family, `db memories --tier`), **#19** (no server lock override), **#20** (no `PORT` env knob — a later order may add one).
- **P4.D300 Tier 3 #23–#26** — v4 has none / retired / carried.
- **P4.D301 item 14** (unreachable), **item 16** (💸 unlanded), **Tier 3 #17–#20** NO-PORT / pre-existing.
- **P4.D302 Tier 3 #14–#17**; the "four jsdom files" wording (lane-record only).

---

# Survey B part 2: the OFFSET_OCCURRED_AT ruling and the repository-fallback `?` reads

Read-only survey. v5 is main @ HEAD (`4e43b714e`). v4 is read at the pin `322a5488e` through `git show`.

Counts: OPEN 5 (A1, B1–B4), CLOSED 3, NEEDS-RULING 2 (B5, B6), NO-ACTION 1 (B8).

---

## (A) `OFFSET_OCCURRED_AT_ROW_SCHEMA`: RULED "refuse as v4 does" (no v4 bug filed)

### A1: Validate the memory ROW schema at the repository `create`, so an offset `occurredAt` gets a 400 as in v4   [OPEN]
- **Source:** P4.D298 (header OPEN; status-log ~182005 and 182192 (b)), the `83334b9a5` round's NEXT 2. Ruled 2026-10-27: REFUSE AS v4 DOES.
- **v4 @322a5488e:**
  - **The route lets the offset through.** `app/api/v1/memories/route.ts:109` declares `occurredAt: z.iso.datetime({ offset: true }).nullable().optional()`. Line `:591` passes it into `admitCandidate`.
  - **Admission reaches the repository.** In `lib/memory/admission/admit.ts:522/527` the `insert` / `insert-linked` arms call `insertRow`. `apply.ts:38-68` then calls `getRepositories().memories.create({... occurredAt: row.occurredAt ?? null ...})`.
  - **The repository validates the whole row.** `memories.repository.ts:535-549` calls `safeQuery(() => this._create({...data, tier: data.tier ?? 'hot', consolidatedFrom: data.consolidatedFrom ?? []}), 'Error creating memory', {characterId})`. `base.repository.ts:350-378` `_create` runs `this.validate({...data,id,createdAt,updatedAt})` against `MemorySchema`.
  - **The schema is Z-only.** `memory.types.ts:115` has `occurredAt: TimestampSchema.nullable().optional()`. `common.types.ts:55` has `TimestampSchema = z.iso.datetime().or(z.date())…`, which accepts `Z` only.
  - **Three ERROR lines, in this order** (the first from `validate` at `base.repository.ts:131-141`, the other two from the nested rethrow `safeQuery`s, which add `collection`; `safe-query.ts:64-70`):
    1. `Data validation failed {collection:'memories', error:<ZodError.message>}`
    2. `Error creating entity {collection:'memories', error}`
    3. `Error creating memory {collection:'memories', characterId, error}`
  - **Nothing catches it in admission or the route.** The ZodError reaches `handleRouteError` (`lib/api/middleware/context.ts:166-167`), which returns `validationError` (`lib/api/responses.ts:108-118`): `{"error":"Validation error","details":zodError.issues}` with status 400.
  - **What the issue is.** A string that fails the datetime check is the string option's bare `invalid_format` (format `datetime`, the NO-offset JS pattern, `path:["occurredAt"]`, `"Invalid ISO datetime"`). v5's `zod_timestamp_issues` doc (`api/zod_issues.rs:1500-1520`) measured the same.
  - **Order matters.** The refusal happens AFTER the embedding call and the neighbour search, and BEFORE the admission ledger is recorded (`admit.ts:285` runs after `applyAction`). So an offset combined with an embedding failure answers 502, not 400.
- **v5 site:**
  - **Nothing validates the row.** `crates/quilltap-core/src/db/memories.rs:368-373` `MemoriesRepository::create` calls `create_row` and validates no row schema. It already logs v4's lines 2 and 3 on `Err` (`db/fallback.rs:853` `log_create_failure`, `:1283` `log_memory_create_failure`).
  - **The insert path today.** `services/admission/apply.rs:119-170` `insert_row` builds a `MemCreate` with `occurred_at: row.occurred_at` and writes it. Admission (`services/admission/admit.rs:834/843`) maps a write error to `Err(DbError)`. `api/memories.rs:776-912` `memory_create` therefore returns 201 `insert-linked` with the offset stored.
  - **The schema check already exists.** `api/zod_issues.rs:1135` `zod_memory_issues` → `nullable_timestamp_issues("occurredAt")` → `zod_iso_datetime_ok` (Z-only `ZOD_ISO_DATETIME_PATTERN`, `:1863`). It is the ONE port of `MemorySchema`. Today it is called only by `db/memories.rs:164` `parse_create_memory`, whose doc at `:160-162` says: "Production `create` callers (memory extraction, the manual create) are NOT gated by this yet — P4.161 Tier 3 item 13".
- **Every v4 `memories.create` caller at the pin, all through `_create` + `MemorySchema`:**

  | v4 caller | v5 caller | Status |
  |---|---|---|
  | admission `apply.ts:40` (manual create, extraction, Carina, every admission caller) | `apply.rs:166` | **unvalidated** |
  | `consolidation.ts:484` (digest create) | `services/consolidation.rs:875` | **unvalidated**; offset reachable only from a v5-stored offset member |
  | `import-entities.ts:587` (`.qtap`; catch → `Failed to import memory: <msg>` warning, skipped) | `services/quilltap_import/memories.rs:459` | **already validated** (P4.161) |
  | `restore.ts:370` (catch → `Failed to restore memory: …`) | `services/backup/restore/orchestrator.rs:991` | **already validated** (P4.161) |

  - The v4 edit PUT (`[id]/route.ts:19-29`) has no `occurredAt`, so it is not a path.
  - **The fold arms also validate in v4.** Absorb / reinforce / promote write through `memories.update` → `_update` → `this.validate(updated)` (`base.repository.ts:404`). `fold.ts:136-137` patches `occurredAt` when the earliest one differs. So an offset candidate that FOLDS into a row whose `occurredAt` is later also refuses in v4, as `Error updating memory`. v5's `MemoriesRepository::update` / `update_for_character` do not validate either. This needs measurement with a corpus row before deciding whether the update side is needed.
- **Fix shape:**
  1. In `MemoriesRepository::create`, build the entity v4 validates (`apply.rs::created_entity` already shapes it), run `zod_memory_issues` on it, and on any issue log `ERROR quilltap::db Data validation failed collection=memories error=<zod_error_message>`.
  2. Return a NEW `DbError::Validation { message, issues }`, a variant that carries the issues. The existing `inspect_err` then emits v4's lines 2 and 3, so all three lines come out in v4's order.
  3. In `api/memories.rs::memory_create`, map that error from `admit_candidate` → `bad_request("Validation error")`. This is the same 400 shape as the route's other refusals; details stay the recorded divergence, L-1.
  4. Doing this at the repository covers admission, consolidation, and (redundantly) import / restore, at v4's exact point after the embedding.
  5. Measure the fold arm first: add `create_offset_occurred_at_fold` (an offset note reinforcing the MANUAL twin, with a later `occurredAt` on the target). If v4 refuses, add the same validation to `update_for_character` over the merged row.
  6. Update the `parse_create_memory` doc at `db/memories.rs:160-162`, which says the production callers are not gated.
- **Both-ways pin to flip:**
  - `crates/quilltap-harness/tests/memories_routes_equivalence.rs:544-556` (the `OFFSET_OCCURRED_AT_ROW_SCHEMA` const) and `:2069-2084` (the match arm): delete both. The row `create_offset_occurred_at_row_refusal` then compares like every 400 row: status + `error`, plus the row count before and after (`after = count(&db)` at `:2061`).
  - Grow the row's line capture to the three ERROR lines if the family compares lines.
  - The oracle row: `harness/oracle/cases/memories-routes.test.ts:836-839` (`occurredAt: '2026-03-01T10:00:00+05:30'`). It needs no regen; v4 already answers 400.
- **Differential:** `memories_routes_equivalence` + `memories-routes.test.ts`. Add the fold row, plus an offset + embedding-failure row (both sides 502). Unit-test `MemoriesRepository::create` refusing with no row written. Optionally add an `admission_tier3_equivalence` row for an extraction-shaped offset (unreachable in production).
- **Files a lane would own:**
  - `crates/quilltap-core/src/db/memories.rs`
  - `crates/quilltap-core/src/db/mod.rs` (the `DbError` variant + `Display`)
  - `crates/quilltap-core/src/api/memories.rs` (the map at `memory_create`)
  - possibly `crates/quilltap-core/src/services/admission/admit.rs` (pass the error through)
  - `crates/quilltap-harness/tests/memories_routes_equivalence.rs`
  - `harness/oracle/cases/memories-routes.test.ts`
- **Size:** M.

### A2: `.qtap` import and backup restore refuse an offset `occurredAt`   [CLOSED]
- **Evidence:** P4.161 (dogfood #152) pre-validates both paths through `parse_create_memory` → `zod_memory_issues` (`services/quilltap_import/memories.rs:459`, `services/backup/restore/orchestrator.rs:991`). The `occurredAt` check is Z-only (`api/zod_issues.rs:1156` → `:1869`).
- **Note:** A1's repository validation makes these a second, redundant check; that is harmless.

---

## (B) Repository-fallback `?` reads (memory programme)

The common v4 shape:
- Each repository read is a FALLBACK `safeQuery`: it logs `ERROR <msg> {collection:'memories', …ctx, error}` and returns `[]` / `null` / `0`. Inside `strictRepositoryFailuresActive()` the same line carries `strictFailures: true` and the call rethrows (`safe-query.ts:51-72`).
- Most memory reads nest a base `findByFilter` / `findOneByFilter` fallback (`Error finding entities by filter` / `Error finding entity by filter`, `base.repository.ts`). Outside the strict scope, the INNER line is the one that fires; the outer line fires only in strict mode.
- The v5 precedent: `db/fallback.rs` (114 `pub fn`s; e.g. `find_by_filter_or_empty` `:144`, `find_by_filter_strict_aware` `:175`, `find_one_by_filter_or_none` `:193`, `paginated_memories_or_empty` `:1619`, `index_rows_or_empty` `:2990`), plus `fallback_home_guard`.

**Recommended fix shape for B1–B4 (one lane): convert in the repository, not at each caller.** v4's fallback lives on the repository method, so EVERY caller falls back. Wrap the `memories_read.rs` function bodies in strict-aware fallback homes and keep `Result`, so the strict scope still propagates. The affected functions and their caller counts:

| `memories_read.rs` function | Callers |
|---|---|
| `find_by_ids` | 6 |
| `search_by_content` | 5 |
| `find_by_id_for_character` | 2 |
| `count_without_embedding` | 4 |
| `find_distinct_chat_ids` | 1 |
| `find_by_character_id_in_batches` | 1 |

Audit each caller for a `?` that depended on `Err`. The NEW homes go in `db/fallback.rs`, with `fallback_home_guard` rows.

### B1: `find_by_ids` / `search_by_content` (admission neighbours)   [OPEN]
- **Source:** P4.D290 header OPEN (`neighbours.rs`).
- **v5 site:**
  - `services/admission/neighbours.rs:180` (`search_by_content`, the unindexed-neighbour word scan).
  - `:290` (`find_by_ids`, the digest hop).
  - `:382` (`find_by_ids`, the hit rows).
  - All `?` → admission fails → `memory_create` returns 500 / the extraction job fails.
  - The repository bodies: `db/memories_read.rs:506-517` (`find_by_ids`) and `:639-655` (`search_by_content`) are plain `query_memories(...)?`.
- **v4 @322a5488e:**
  - `lib/memory/admission/neighbours.ts:123` (`searchByContent`), `:175` and `:213` (`findByIds`).
  - `memories.repository.ts:210-225`: `findByIds` → `'Error finding memories by IDs' {count: ids.length}` fallback `[]`, over an inner `findByFilter` fallback.
  - `:298-322`: `searchByContent` → `'Error searching memories by content' {characterId, queryLength}` fallback `[]`.
- **Fix shape:** Make `find_by_ids` / `search_by_content` strict-aware fallbacks that log v4's lines (the inner `Error finding entities by filter` outside strict; outer + inner `strictFailures=true` inside strict). The admission sites keep `?` (they now see `Ok([])`).
- **Differential:** Add throwing-plant rows to `memories_read_equivalence` (per the memory note "Throwing plants must fail both reads": a generated read column added after the rows). Add an `admission_tier3_equivalence` row: a candidate admitted with the memories read failing → v4 inserts with no neighbours.
- **Files:**
  - `crates/quilltap-core/src/db/memories_read.rs`
  - `crates/quilltap-core/src/db/fallback.rs`
  - `crates/quilltap-harness/tests/memories_read_equivalence.rs` + its oracle case
  - `crates/quilltap-harness/tests/admission_tier3_equivalence.rs`
- **Size:** M.

### B2: `find_by_ids` (maintenance fold of near-duplicates)   [OPEN]
- **Source:** P4.D290 header OPEN (`fold_near_duplicates.rs`).
- **v5 site:** `services/memory_maintenance/fold_near_duplicates.rs:259`. The `?` sits inside `with_admission_lock` and fails the maintenance phase.
- **v4 @322a5488e:** `lib/memory/maintenance/fold-near-duplicates.ts:168` calls `findByIds(ids)` (fallback `[]`). It then finds no survivor, and that group is skipped.
- **Fix shape:** Covered by B1's repository conversion. Confirm that the empty-rows arm matches v4's skip line.
- **Differential:** A `maintenance_ops_tier2_equivalence` row with the read failing mid-fold.
- **Files:** as B1, plus `crates/quilltap-harness/tests/maintenance_ops_tier2_equivalence.rs`.
- **Size:** S (on top of B1).

### B3: `find_by_id_for_character` (memory edit + neighbour validation)   [OPEN]
- **Source:** P4.D290 header OPEN (`update_memory.rs`).
- **v5 site:**
  - `services/admission/update_memory.rs:222` (the existing row; `?` → 500 where v4 answers NotFound → 404).
  - `:272` (the related-id check; `?` where v4 marks the id invalid).
  - The repository: `db/memories_read.rs:454-465`.
  - The in-closure `find_by_id` re-reads at `:378` and `:402` stand in for v4's `update` return. They are NOT fallback reads (v4's equivalent is the rethrow `Error updating memory for character`), so leave them.
- **v4 @322a5488e:** `lib/memory/admission/update-memory.ts:84` and `:112` call `findByIdForCharacter`. `memories.repository.ts:139-152` logs `'Error finding memory by ID for character' {characterId, memoryId}` and falls back to `null`, over the inner `findOneByFilter` fallback (`Error finding entity by filter`).
- **Fix shape:** Convert `find_by_id_for_character` to a strict-aware `_or_none` home. A failed read then answers v4's 404 (or the invalid-related-id 400).
- **Differential:** `memories_routes_equivalence` item-PUT rows with a throwing plant.
- **Files:** as B1, plus `crates/quilltap-harness/tests/memories_routes_equivalence.rs`.
- **Size:** S.

### B4: `count_without_embedding` (graph repair)   [OPEN]
- **Source:** P4.D290 / P4.D297 headers OPEN (`memory_repair.rs`).
- **v5 site:**
  - `services/memory_repair.rs:103-104`: `count_without_embedding` with `?` → the route answers 500.
  - The repository: `db/memories_read.rs:1132-1140`.
  - The dangling scan at `:101` CORRECTLY has no fallback. v4 says "No fallback: a scan that failed must not read as a clean graph" (`memories.repository.ts:1338-1340`), so leave it.
- **v4 @322a5488e:** `lib/memory/repair.ts:55`. `memories.repository.ts:1482-1494` logs `'Error counting memories without embedding' {characterId}` and falls back to `0`.
- **Fix shape:** A strict-aware `_or_zero` home (the precedent is `unconsidered_hot_count_or_zero`). Also fix `memory_repair.rs`'s module doc at `:12-14`, which covers the scan only.
- **Differential:** `memory_graph_repair_tier2_equivalence` (+ its oracle). Add a row where the count read fails and the report shows `missingEmbeddings: 0`.
- **Files:**
  - `crates/quilltap-core/src/db/memories_read.rs`
  - `crates/quilltap-core/src/db/fallback.rs`
  - `crates/quilltap-core/src/services/memory_repair.rs` (doc)
  - `crates/quilltap-harness/tests/memory_graph_repair_tier2_equivalence.rs`
- **Size:** S.

### B5: Regenerate-all's three reads (chat list, in-flight job snapshot, distinct chatIds)   [NEEDS-RULING, for the chat list only]
- **Source:** P4.D296 header OPEN ("three `?` reads where v4 falls back (regenerate-all's chat list is a RULING candidate…)"); status-log 184920-184922.
- **v5 site** (`crates/quilltap-core/src/services/memory_regenerate_job.rs`, written by P4.D296 `93a4fdc47`):
  - `:235-237` `chats_read::find_by_user_id` `.map_err(...)?`. That function (`db/chats_read.rs:511`) already DROPS per-row invalid rows as v4's `validateSafe` does; only a whole-query failure propagates.
  - `:246-253` `BackgroundJobsRepository::find_by_user_id` ×2 (`db/background_jobs.rs:762`) `?`.
  - `:298-300` `memories_read::find_distinct_chat_ids` `?` (`db/memories_read.rs:1296-1308`).
  - Each `Err(message)` fails the job, as v4's `throw` would.
- **v4 @322a5488e** (`lib/background-jobs/handlers/memory-regenerate-all.ts`):
  - `:35` `repos.chats.findByUserId` → `chats.repository.ts:164-166` → base `findByFilter`: fallback `[]`, ERROR `Error finding entities by filter {collection:'chats', error}`.
  - `:41-42` `backgroundJobs.findByUserId`: `'Error finding background jobs by user ID' {userId}`, fallback `[]`.
  - `:83` `memories.findDistinctChatIds`: `'Error listing distinct memory chatIds' {}`, fallback `[]`.
- **What v4 does when the chat list read fails:** `validChatIds` is empty, so no per-chat wipes are queued. Then EVERY chatId that holds memories counts as an "orphan", and each gets a `MEMORY_REGENERATE_CHAT` wipe on the standard profile (dangerous-profile routing is lost). A transient chats-table failure therefore retires the memories of every chat and re-extracts them on the wrong profile.
- **The other two reads are benign either way.** An empty job snapshot only loses the dedup short-circuit; the enqueue dedups anyway. An empty distinct list only skips the orphan sweep.
- **RULING (the chat list):**
  - **Option A: converge.** Use `find_by_filter_or_empty("chats", …)` with v4's line, so v5 queues the orphan wipe for every chat exactly as v4 does. It is faithful and destructive on a transient read failure.
  - **Option B: keep strict.** Record a ruled divergence, e.g. `REGENERATE_ALL_STRICT_CHAT_LIST`, pinned both ways: v4 enqueues N orphan wipes, v5 fails the job with v4's line logged and no wipe. Optionally file a v4 GitHub issue (draft first; the human's go-ahead is needed).
  - **Recommendation: B.** It follows the same safety reasoning as the strict-export ruling and v4's own "a scan that failed must not read as a clean graph". Converge the other two reads (A) regardless.
- **Fix shape** (whichever way the chat list is ruled):
  - Make the job snapshot use `find_by_filter_or_empty`-shaped homes with `'Error finding background jobs by user ID'`, and the distinct list use `'Error listing distinct memory chatIds'` (both falling back to `[]`).
  - Under B, the chat list logs v4's `Error finding entities by filter` line and then fails.
- **Differential:** `memory_regenerate_jobs_tier3_equivalence` (+ its oracle case). Add three throwing-plant rows; the chat-list row becomes the both-ways pin under B.
- **Files:**
  - `crates/quilltap-core/src/services/memory_regenerate_job.rs`
  - `crates/quilltap-core/src/db/fallback.rs`
  - `crates/quilltap-core/src/db/background_jobs.rs` (if the home sits there)
  - `crates/quilltap-core/src/db/memories_read.rs` (`find_distinct_chat_ids`, shared with B1's lane)
  - `crates/quilltap-harness/tests/memory_regenerate_jobs_tier3_equivalence.rs`
- **Size:** M.

### B6: P4.D271 #17: the maintenance corpus batch loader (`Error loading memory batch`)   [NEEDS-RULING, same class as B5]
- **Source:** P4.D271 Tier 3 #17, also recorded at `services/memory_maintenance/corpus.rs:20-23`.
- **v5 site:** `services/memory_maintenance/corpus.rs:89-92` `load` → `memories_read::find_by_character_id_in_batches` (`db/memories_read.rs:477-503`), which is `?` per page, so the run fails.
- **v4 @322a5488e:** `memories.repository.ts:179-203`. Each page is `safeQuery(... 'Error loading memory batch', {characterId, skip, batchSize}, [])`. A failing page returns `[]` and ENDS the generator, so the corpus is TRUNCATED. `lib/memory/maintenance/corpus.ts:55` is the only caller.
- **Why it needs a ruling:** Maintenance over a truncated corpus plans folds / demotions from a partial view. Demotion is reversible, but the cap counts come from a separate query (`count_cap_breakdown`), so a truncated corpus can demote or fold the wrong rows.
- **Ruling options:**
  - **Converge:** the per-page fallback, with v4's line and early end.
  - **Keep strict:** a ruled divergence, pinned both ways.
- **Recommendation:** keep strict, the same reasoning as B5. Flag it alongside B5 so the human rules both at once.
- **Differential:** `maintenance_ops_tier2_equivalence` / `memory_maintenance_tier2_equivalence` with a page-two throwing plant.
- **Files:**
  - `crates/quilltap-core/src/db/memories_read.rs`
  - `crates/quilltap-core/src/services/memory_maintenance/corpus.rs`
  - `crates/quilltap-core/src/db/fallback.rs`
- **Size:** S.

### B7: P4.D268 Tier 3 #19: `Failed to retrieve memories` / `… inter-character memories` warnings   [CLOSED]
- **Evidence:** P4.D291's recall pipeline ported them. `services/recall/assemble.rs:469`, `:561` and `:705` push v4's `Failed to retrieve memories: …` / `Failed to retrieve inter-character memories: …` warnings (v4 `lib/memory/recall/assemble.ts:331/370/451`).
- **Still open, but not this class:** the two `[Recall] … (non-fatal)` WARNs absent from `assemble.rs` (P4.D291's own recorded item).

### B8: P4.D298 `Error listing memories` (the route's list catch)   [NO-ACTION — superseded: v4 catch is DEAD, see part 3]
- Not a `?` read in the strict sense. It is surveyed under P4.D298's smalls in the main report, and is listed here only because it shares `memories_read.rs` / `fallback.rs` with B1–B4. **Lane grouping: put it in the same lane.**

### B9: `find_by_id_for_character`'s sibling `find_by_id` (`memories_read.rs:448`)   [CLOSED (not a site)]
- **Why it is not a site:** Its only memory-programme uses in the surveyed files are the in-closure re-reads in `update_memory.rs:378/402`. Those stand in for v4's `update` return value, which is a rethrow class in v4, not a fallback. No change is needed. It is recorded so a lane does not "fix" it.

---

## Shared-file grouping (for lanes)

- **Lane "memory repository fallbacks" (B1, B2, B3, B4, the non-ruled halves of B5 and B6, and B8):**
  - Owns `db/memories_read.rs` and `db/fallback.rs`.
  - Owns the doc of `services/memory_regenerate_job.rs` / `services/memory_repair.rs` / `services/memory_maintenance/corpus.rs`.
  - Owns the families `memories_read_equivalence`, `admission_tier3`, `maintenance_ops_tier2`, `memory_graph_repair_tier2`, `memory_regenerate_jobs_tier3`.
  - It needs the B5 / B6 rulings FIRST.
- **Lane "offset occurredAt refusal" (A1):**
  - Owns `db/memories.rs`, `db/mod.rs` (the `DbError` variant), the `memory_create` arm of `api/memories.rs`, and `memories_routes_equivalence` + `memories-routes.test.ts`.
  - **Collides** with B3's `memories_routes_equivalence` item-PUT rows and with any P4.D298 smalls in `api/memories.rs` (`Error listing memories`, `create_project_explicit_wins`). Either merge A1 with the P4.D298 smalls into ONE `api/memories.rs` + `memories_routes` lane, or move B3's route rows into that lane.
- **One shared-index risk:** `db/fallback.rs` is append-heavy. If two lanes add homes there, give each a fenced region (the P4.D324 precedent).

---

# Survey B part 1: the `01a83539d`-round memory orders (P4.D265–D271, P4.D276)

Every item was checked against v5 main @ `4e43b714e` and v4 @ `322a5488e`. The memory refactor round (`83334b9a5`, P4.D288–D302) rewrote or retired much of this code, so most items are CLOSED or NO-ACTION.

**Counts:** OPEN 8 · NEEDS-RULING 1 · CLOSED 25 · NO-ACTION 23 (most NO-PORT / routed-and-landed / 💸 items are grouped one line each)

**How the OPEN items group by files:**
- **Lane "memory repo smalls"** (`db/memories.rs`, `db/memories_read.rs`, `services/memory_maintenance/corpus.rs`): B1-1 (`update_tier_bulk` stamps `updatedAt` once per chunk) and B1-7 (the batch-loader fallback). B1-7 is the same class as the parent's "repository-fallback `?` reads" (P4.D290/D296/D297 → `memory_repair.rs` etc.) and shares `db/fallback.rs` with it, so put it in that lane.
- **Lane "cheap-LLM lines"** (`cheap_llm.rs`, `services/image_job_common.rs`, `tools/generate_image.rs`): B1-6. It shares files with the `25b8fab6a` round's `CHEAP_LLM_KEY_ORDER` follow-up, so give both to one owner.
- **Standalone S items, disjoint files:**
  - B1-2 `services/frozen_archive.rs`
  - B1-3 `api/memories.rs:164`
  - B1-4: harness rows only (`memory-extraction-pass.ts` + `memory_extraction_pass_equivalence.rs`)
  - B1-5 `services/conversation_summary_vault_bridge.rs`
- **SPA** (`apps/web/src/app/memory/*`, `screens/settings/memory/api-error-text.ts`): B1-8 and the B1-R1 ruling.

---

## OPEN

### B1-1 — `update_tier_bulk` mints one `updatedAt` per call; v4 re-stamps per chunk   [OPEN]
- Source: P4.D265 status header, OPEN (recorded).
- v5 site: `crates/quilltap-core/src/db/memories.rs:796-~850`. It sets `let now = now_iso();` once (`:816`) and uses that `now` for `updatedAt` in every chunk's `UPDATE`, and for `supersededAt` as well.
- v4 site @322a5488e: `lib/database/repositories/memories.repository.ts:1148-1185`.
  - The outer `now` drives `supersededAt` and the passed `updatedAt: now`.
  - `this.updateMany(...)` per chunk goes through `base.repository.ts:526-540`. There `$set: {...update, updatedAt: this.getCurrentTimestamp()}` OVERRIDES `updatedAt` with a fresh timestamp for each chunk.
  - Net effect: `supersededAt` = the outer `now`; `updatedAt` = a per-chunk mint.
- Fix shape: inside the `for chunk` loop, bind `let chunk_now = now_iso();` and push it for `updatedAt`. Keep the outer `now` for `supersededAt` and `consolidatedAt`. Fix the doc comment at `:784-790`.
- Differential: unit-only. The tier-2 families strip `updatedAt`, so timestamps are normalized. Add a unit test that splits the ids across two chunks (SQLITE_VARIABLE_CHUNK_SIZE+1 ids) and asserts `supersededAt` is shared and `updatedAt` ≥ `supersededAt`.
- Files a lane would own: `crates/quilltap-core/src/db/memories.rs`
- Size: S

### B1-2 — the frozen-archive cache holds its process-global `Mutex` across `compute`'s DB reads   [OPEN]
- Source: P4.D268 status header, OPEN (recorded) — "a later cleanup computes outside the lock".
- v5 site: `crates/quilltap-core/src/services/frozen_archive.rs`:
  - `:222-227` `cache()` returns a `MutexGuard<'static>`.
  - `:257-264` `cache().get_or_compute_with(..., |..| compute_frozen_archive(db, ...))` runs the two `db.read_main` reads (`:341-372`, `find_hot_digests` / `find_most_important`) under the guard.
  - `:146-203` is the hit/miss/insert logic.
  - Effect: every turn's archive compute is serialized process-wide.
- v4 site @322a5488e: `lib/memory/recall/archive.ts:80-115`. `cache.get(key)` (`:104`) → `await computeFrozenArchive(...)` (`:113`) → `cache.set(...)` (`:115`). The awaits interleave freely on v4's single thread; nothing is serialized.
- Fix shape:
  - Split `get_or_compute_with` into a `lookup` step (under the lock; refreshes recency on a hit) and an `insert` step (under the lock; LRU trim + the `[FrozenArchive] Computed archive` DEBUG).
  - In `get_or_compute_frozen_archive`, drop the guard before `compute_frozen_archive` and re-take it to insert.
  - An invalidation that lands mid-compute can be overwritten by the stale insert. v4 has the same window, since it awaits between get and set, so keep that behavior and document it.
- Differential: unit-only (concurrency). The existing `frozen_archive.rs` tests (`GLOBAL_SERIAL`, `:884-1090`) keep the semantics. Add one test where a compute closure calls `cache()`; before the fix it deadlocks, after the fix it passes.
- Files a lane would own: `crates/quilltap-core/src/services/frozen_archive.rs`
- Size: S

### B1-3 — memory list `limit=0` → 1 in v5; v4's `parseInt(limit) || 50` gives 50   [OPEN]
- Source: P4.D269 Tier 3 #22 ("recorded for the follow-ups round").
- v5 site: `crates/quilltap-core/src/api/memories.rs:164-166`. `if let Some(limit_raw) = limit { let limit = limit_raw.clamp(1, 200);` The wire type is `api/types.rs:1690-1691` `limit: Option<i64>` (`Request::MemoryList`). There is no REST edge; it is dispatch only.
- v4 site @322a5488e: `app/api/v1/memories/route.ts:323-331`. `if (limitParam) { const limit = Math.max(1, Math.min(200, parseInt(limitParam, 10) || 50)); const offset = Math.max(0, parseInt(offsetParam || '0', 10) || 0);`
- Fix shape:
  - `let limit = if limit_raw == 0 { 50 } else { limit_raw.clamp(1, 200) };` This mirrors `0 || 50`. Negatives clamp to 1, as `Math.max(1, …)` does.
  - Check that `offset` mirrors `|| 0` too (it already clamps ≥0).
  - Correct the comment at `:165`, which claims exact v4 parity.
- Differential: `crates/quilltap-harness/tests/memories_routes_equivalence.rs` + `harness/oracle/cases/memories-routes.test.ts`. Add a `list_limit_zero` row (v4 query `limit=0`, v5 `limit: 0`).
- Files a lane would own: `crates/quilltap-core/src/api/memories.rs` (list arm only), the two harness files.
- Size: S

### B1-4 — the hybrid per-turn OTHER floor's `>=` at exactly 0.75 has no corpus row   [OPEN]
- Source: P4.D270 Unification OPEN. A `>` mutation survives. The record suggests a `mode_hybrid_floor_half` row (a candidate at 0.75 plus one at 0.5).
- v5 site: `crates/quilltap-core/src/services/extraction/rate_limit.rs:71-77` (`apply_importance_floor`: `d.importance >= floor`), called at `services/extraction/run.rs:446-456` (the batch floor) and `:470-471` (the throttle floor). The floor value is `db/instance_settings.rs:806` `per_turn_other_floor: 0.75`, wired at `extraction/passes/other_turn.rs:33-34`.
- v4 site @322a5488e: `lib/memory/extraction/rate-limit.ts:44-45` `drafts.filter(d => d.importance >= floor)`. Called at `lib/memory/extraction/run.ts:262` and `:276`.
- Fix shape: add a corpus row in which a hybrid-mode OTHER batch has a draft at exactly 0.75 (kept) and one at 0.5 (floored, with the "Hybrid floor …" DEBUG line). If cheaper, add a pure tier-1 row that calls `applyImportanceFloor` at the boundary. No production change. Mutation-prove `>`.
- Differential: `crates/quilltap-harness/tests/memory_extraction_pass_equivalence.rs` + `harness/oracle/cases/memory-extraction-pass.ts` (pure tsx).
- Files a lane would own: those two harness files.
- Size: S

### B1-5 — the summary bridge's `Failed to resolve character vault store` WARN keys diverge from v4   [OPEN]
- Source: P4.D271 Unification OPEN ("the summary bridge's … WARN keys (pre-existing)").
- v5 site: `crates/quilltap-core/src/services/conversation_summary_vault_bridge.rs:278-284`. It logs `character_id = …` (snake key), an `error` built from v5's own text (`"vault unavailable for character {id} (mount {mount})"`), and no `context`. The digest twin already does this correctly at `services/commonplace_digest_vault_bridge.rs:252-262`: `context = "file-storage.character-vault-bridge"`, `characterId`, and `error` = `VaultUnavailable{..}.message()`.
- v4 site @322a5488e: `lib/file-storage/character-vault-bridge.ts:74-81`. `logger.warn('Failed to resolve character vault store', { context: 'file-storage.character-vault-bridge', characterId, error: error.message })`.
- Fix shape: copy the digest bridge's field set into the summary bridge (`context`, `characterId`, `error` = `crate::db::vault_read_overlay::VaultUnavailable { character_id, mount_id }.message()`). Remove the doc comment's "v5's own text" claim (`:261-270`). Better still, have both bridges call one shared helper (DRY).
- Differential: `crates/quilltap-harness/tests/vault_summary_mirror_tier2_equivalence.rs`, or `context_summary_service_tier3_equivalence.rs` if a dangling-vault plant already exists there. Pin the line's fields with a byte compare.
- Files a lane would own: `crates/quilltap-core/src/services/conversation_summary_vault_bridge.rs` (+ optionally `commonplace_digest_vault_bridge.rs` for the shared helper), one harness family.
- Size: S

### B1-6 — four `[CheapLLM]` lines missing from the selection ladder, and a duplicated `build_cheap_llm_selection`   [OPEN]
- Source: P4.D271 Unification OPEN + Tier 3 #18 ("`build_cheap_llm_selection`'s `[CheapLLM]` lines, pre-existing").
- v5 sites:
  - `crates/quilltap-core/src/services/image_job_common.rs:126-146` and its private twin `crates/quilltap-core/src/tools/generate_image.rs:1562-1585`. Both are byte-identical bodies with no DEBUG lines.
  - `crates/quilltap-core/src/cheap_llm.rs:283-293`: USER_DEFINED not found has only a comment ("v4 warns and falls through"), no WARN.
  - `cheap_llm.rs:~320-330`: the priority-5 fallback has no WARN.
  - The only `[CheapLLM]` lines present are `:355`/`:378` (the uncensored pair).
- v4 site @322a5488e:
  - `lib/llm/cheap-llm-user-selection.ts:46-66` `selectCheapLLMFromProfiles`:
    - DEBUG `[CheapLLM] No connection profiles to select a cheap LLM from {context:'selectCheapLLMFromProfiles'}` (`:52`);
    - DEBUG `[CheapLLM] Selected cheap LLM for user profile set {context, defaultProfileId, provider, modelName, connectionProfileId}` (`:58`).
  - `lib/llm/cheap-llm.ts:233` WARN `[CheapLLM] USER_DEFINED profile not found, falling through {context:'getCheapLLMProvider', userDefinedProfileId}`.
  - `lib/llm/cheap-llm.ts:278` WARN `[CheapLLM] Using FALLBACK to cheapest model for current provider {context, provider, cheapModel, currentProfileModel}`.
- Fix shape:
  - Make `image_job_common::build_cheap_llm_selection` the one `select_cheap_llm_from_profiles` (v4's name) with the two DEBUGs. Delete the `generate_image.rs` twin and point its 9 callers at the one function.
  - Add the two WARNs in `get_cheap_llm_provider`.
  - Coordinate with `CHEAP_LLM_KEY_ORDER` (the `25b8fab6a` round's follow-up — same files).
- Differential: `crates/quilltap-harness/tests/cheap_llm_selection_equivalence.rs` + `harness/oracle/cases/cheap-llm-selection.ts`. Capture lines per row: a no-profiles row, a USER_DEFINED-missing row, and a fallback row.
- Files a lane would own: `crates/quilltap-core/src/cheap_llm.rs`, `services/image_job_common.rs`, `tools/generate_image.rs` (the twin deletion only), the cheap-llm-selection harness pair.
- Size: S–M

### B1-7 — the maintenance corpus batch loader (`Error loading memory batch`)   [DUPLICATE of part 2 B6 — NEEDS-RULING there; this entry argues converge, B6 argues keep strict]
- Source: P4.D271 Tier 3 #17 (recorded divergence); re-recorded in P4.D297's `corpus.rs` docblock (`:20-23`).
- v5 site:
  - `crates/quilltap-core/src/db/memories_read.rs:477-502` `find_by_character_id_in_batches`: `query_memories(...)?` per page, so an error aborts.
  - `crates/quilltap-core/src/services/memory_maintenance/corpus.rs:88-101` `MaintenanceCorpus::load` propagates `?`, so the whole maintenance run fails.
- v4 site @322a5488e: `lib/database/repositories/memories.repository.ts:177-200` `findByCharacterIdInBatches`. Each page is `this.safeQuery(() => this.findByFilter(...), 'Error loading memory batch', { characterId, skip, batchSize }, [])`. A failing page logs ERROR and yields `[]`, which ends the generator, so the corpus is truncated at that page.
- Fix shape:
  - Wrap each page read in the strict-aware fallback (`db::fallback`, e.g. a `find_by_filter_or_empty`-style helper). Emit v4's ERROR line `Error loading memory batch {characterId, skip, batchSize, error}` and end the loop with the batches gathered so far.
  - Inside the strict scope (export) it still rethrows.
  - Update `corpus.rs:20-23`'s doc.
  - This is the same class as the parent's repository-fallback `?` reads (P4.D297 `memory_repair.rs`), so put it in that lane.
- Differential: `crates/quilltap-harness/tests/memory_maintenance_tier2_equivalence.rs` (+ its oracle case). Use a throwing plant that fails the SECOND page only: a corrupt row past offset 250, or a generated column. See the memory notes "throwing plants must fail both reads". Compare corpus size / phase counts + the captured ERROR line.
- Files a lane would own: `crates/quilltap-core/src/db/memories_read.rs` (batch fn), `services/memory_maintenance/corpus.rs` (doc), `db/fallback.rs` (if a new helper), the maintenance tier-2 pair.
- Size: S–M

### B1-8 — `getErrorMessage` lives in `memory/memory-format.ts`, with `api-error-text.ts` re-exporting it   [OPEN]
- Source: P4.D276 Unification OPEN ("R-A's two items a later ruling", the homing half).
- v5 site: `apps/web/src/app/memory/memory-format.ts:90-~110` (the transcription of v4 `getErrorMessage`); `apps/web/src/app/screens/settings/memory/api-error-text.ts:15-17,25,34` imports and re-exports it. Other callers include `brahma-console-settings.ts`.
- v4 site @322a5488e: `lib/error-utils.ts:19-33`, a generic, non-memory home.
- Fix shape: move it to a neutral `apps/web/src/app/core/error-utils.ts` (v4's module name), re-point the imports, and drop the re-export. The behavior and its specs stay unchanged.
- Differential: unit-only (SPA vitest; existing `api-error-text.spec.ts` / `memory-list.spec.ts` keep passing).
- Files a lane would own: `apps/web/src/app/memory/memory-format.ts`, `apps/web/src/app/screens/settings/memory/api-error-text.ts`, new `core/error-utils.ts`, importers.
- Size: S

---

## NEEDS-RULING

### B1-R1 — v5's memory list shows a "Source" link that v4's list never renders   [NEEDS-RULING]
- Source: P4.D276 R-A (Tier 3 #19; "a ruling the human may take in a later round").
- v5 site: `apps/web/src/app/memory/memory-list.ts:45-51` (the docblock records the divergence), `:171` (`(navigateToSource)="navigateToSource($event)"` bound on every card), `:434` (`navigateToSource` handler). The card renders "Source" for AUTO rows with `sourceMessageId` + `chatId`.
- v4 site @322a5488e: `components/memory/memory-list.tsx:336-343`. `<MemoryCard … onEdit onDelete onViewDigest isDeleting />` passes NO `onNavigateToSource`, so `memory-card.tsx:161`'s `&& onNavigateToSource &&` guard hides the link. v4 never shows it.
- Options:
  - (a) CONVERGE: drop the `(navigateToSource)` binding and handler. v5 then shows exactly what v4 does.
  - (b) KEEP as a ruled v5 enhancement and record it as a named divergence.
- Recommendation: (a). The port's rule is v4-faithful UI, the link is not in the oracle, and the docblock already calls it "pre-existing, not drift" rather than a wanted feature. If the human values the link, it belongs upstream as a v4 feature request.
- Differential: unit-only (SPA vitest: `memory-list.spec.ts` asserts no "Source" text on an AUTO row with a source message).
- Files a lane would own: `apps/web/src/app/memory/memory-list.ts`, `memory-list.spec.ts` (+ `memory-card.ts` docblock).
- Size: S

The realtime half of R-A is CLOSED (see below).

---

## CLOSED (with evidence)

### P4.D265
- **`apply-memory-tiers-migration.ts`'s `ledger: false` arm exercised by nothing** — CLOSED. It is now exercised by four oracle cases: `harness/oracle/cases/system-import.test.ts:628`, `system-import-execute.test.ts:1543`, `system-export.test.ts:782`, `recall-replay.test.ts:130`.
- **Tier 3 #23 — a watermark write over a chat row v4's schema refuses** — CLOSED (moot). P4.D288 dropped the chat watermark columns; `otherExtractionWatermarkMessageId` now appears only in the legacy cursor repair / remap readers (`db/memory_extraction_cursors_repair.rs`, `db/memory_tiers_repair.rs`). There is no column writer left.
- **Tier 3 #24 — consumers + `MEMORY_REGENERATE_CHAT` handler** — CLOSED. The consumers landed in the round. The handler landed in P4.D296: `crates/quilltap-core/src/services/memory_regenerate_job.rs`, registered at `crates/quilltap-host/src/spine.rs:3251,3927`.

### P4.D266
- **A float `maxMemories` on the housekeep POST is floored** — CLOSED (surface retired). v4 @pin has no housekeep POST (`git grep maxMemories` hits only the optimizer, self-extraction and `characters/[id]` POST). v5 replaced housekeeping with the one maintenance job (P4.D297, `services/memory_maintenance/`).
- **The dedup fold-failure WARN's `error` text was unverified** — CLOSED (superseded). The fold was rewritten in P4.D297: `services/memory_maintenance/fold_near_duplicates.rs:298-304` WARN `Near-duplicate fold failed; its rows are kept` with `error = error_text(&e)`, compared by P4.D297's families.
- **`housekeep_run_demotes` not added (wall-clock)** — CLOSED (subject retired; demotion is `memory_maintenance/demote.rs`, covered by `memory_maintenance_tier2_equivalence`).
- **Tier 3 #20 — the job-completion frozen-archive hook** — CLOSED. `frozen_archive.rs:310` `invalidate_frozen_archives_for_completed_job` is called from `job_runner.rs`.

### P4.D267
- **`best_cosine` over `f64::max` where `Math.max` propagates NaN** — CLOSED. P4.D291 rewrote it: `services/recall/score.rs:130-132` folds with `crate::jsnum::math_max`.
- **An unparsable `clock_iso` falls to the wall clock where v4 reaches an Invalid Date** — CLOSED. `services/recall_replay.rs:608` `js_date_parse_ms(&clock_iso).map_or(f64::NAN, …)` mirrors v4's `Date.parse` → NaN.
- **Tier 3 #17 — the CLI's `--memory-budget` / `--tuning*` / `--signals-from` / `--as-of`** — CLOSED. They landed in `crates/quilltap-cli/src/recall_replay_cmd.rs:5-6,129-150`.
- **Tier 3 #18 — delivery stamps / budget / pool / names / `include_cold`** — CLOSED. Landed by P4.D268 / P4.D269 and rewritten by P4.D291.
- **Tier 3 #19 — the heap search unported** — CLOSED. `db/vector_store.rs:29-30,82,307` `search_heap` is ported (the linear/heap switch).
- **Tier 3 #20 — help docs** — CLOSED (P4.D278, then P4.D303 vendored `help/`).
- **Tier 3 #21 — the R-L handoff doc fixes** — CLOSED (landed at union per the status header).

### P4.D268
- **`present_participant_names` as a set** — CLOSED / no-op. It is now a slice (`recall_tags.rs:488`), and `recall_replay.rs:573,730` builds a `Vec`. The record already measured it order-insensitive.
- **Tier 3 #19 — v4's `warnings.push('Failed to retrieve memories: …')` / inter-character** — CLOSED. P4.D291 ported these: `services/recall/assemble.rs:469,561,705`.
- **Tier 3 #20 — `MEMORY_REGENERATE_CHAT`** — CLOSED (P4.D296; see P4.D265 #24).

### P4.D269
- **`PROBE_STORE_LOAD_FAILURE` — `vector_store::load` `Err` → the probe's 500** — CLOSED. `db/vector_store.rs:108-115` `load` is infallible (v4's catch → empty store; P4.D288 C1-3). The row was RETIRED on measurement (`services/anchor_gate_probe.rs:49-53`; `anchor_gate_probe_tier3_equivalence.rs:28-31`).
- **Tier 3 #20 — the probe's heap-search tie order** — CLOSED (`search_heap` ported, as above).

### P4.D270
- **`memory_pipeline_jobs_tier3` never pays the fold OTHER call end to end** — CLOSED (re-homed). `crates/quilltap-harness/tests/fold_other_pass_tier3_equivalence.rs` (P4.D296) runs nine handler jobs through `handle_memory_extraction`'s pass branch, including the `foldOtherCatchup` legacy keys. `memory_pipeline_jobs_tier3_equivalence.rs:684-689` documents the hand-off.
- **E9 — the regenerate rebuild (+ Tier 3 #20)** — CLOSED. `services/memory_regenerate_job.rs` (P4.D296 R-V) clears both fold cursors and enqueues the other-fold / episode-fold catch-ups.

### P4.D271
- **An unrecognised `trigger` string parses to `manual`** — CLOSED (subject retired). v4 @pin has no `MEMORY_CONSOLIDATION` job (only the `memoryConsolidation` settings key). v5's maintenance ledger echoes any non-null `reason` (`services/memory_maintenance/ledger.rs:175-178`).
- **`mirror_touched_buckets` → NaN** — CLOSED (gone on both sides: `touchedBuckets` absent from v4 lib @pin and from v5 core).
- **Tier 3 #19 — SPA cards / help / carriers** — CLOSED (P4.D276 / P4.D278 / P4.D272–D273 landed).

### P4.D276
- **R-A, realtime half — the unscoped `memories` hint mapped narrowly** — CLOSED. P4.D300 R-AB widened it to every memories key: `apps/web/src/app/core/realtime-topic-map.ts:57-60,137-147`.

---

## NO-ACTION

- **P4.D265 `hydrate_raw_rows`'s silent `continue` on a non-object row** (`db/memories_read.rs:344`) — unreachable by construction. `query_raw_memories` → `marshal_row` always yields an object, and v4's SQLite rows are always objects. A WARN could be added for symmetry, but no plant can reach it.
- **P4.D265 Tier 3 #20 / #21 / #22** — NO-PORT by architecture (the migration runner lines, the job-child buffer arms, `patchOnlyFields`, `findSupersededBy`).
- **P4.D266 Tier 3 #21** — NO-PORT (buffered child arms, NaN guard, `setTier` plumbing). **#22** — routed to siblings, all landed (P4.D271/D276/D278/D270).
- **P4.D268 Tier 3 #15–#17** — routed to P4.D267 / P4.D266 / P4.D271 / P4.D269, all landed and later superseded by P4.D291. **#18** — v4's hook WARN arm is unreachable in v5, pinned negative.
- **P4.D269 Tier 3 #19 / P4.D271 Tier 3 #15** — the `quilltap memories` verb family (incl. `consolidate`) is still `not_yet_available` (`crates/quilltap-cli/src/main.rs:62,109-111,265`). This is a standing, recorded deferral (P4.D297 #28 / P4.D299 #16 keep the loud refusals). It is a feature-sized port (L), not a small; flag it for a future order if wanted.
- **P4.D269 Tier 3 #21** — no v5 REST memories family (R-A). **#23** — the recorded `v8_json_parse_message` class (1e21 / 1e400 / lone surrogates).
- **P4.D270 `pick_connection_profile_id` drops a truthy non-string id** — now `services/extraction/context.rs:272-288` `pick_extraction_connection_profile_id`; v4 is `lib/memory/extraction/context.ts:117-123`. Unreachable: v4's chat rows are Zod-parsed (`connectionProfileId: string|null`), so a non-string never reaches the picker on either side.
- **P4.D270 Tier 3 #21 / #22 / #23** — routed (R2 to P4.D268) / no analog (`patchOnlyFields`) / P4.D265's accessor.
- **P4.D271 Tier 3 #16** — NO-PORT by architecture. **#20** — 💸 live consolidation (dogfood only).
- **P4.D276 gated Consolidation beat leaves the private instance's settings clobbered** — torn down in `afterAll`; harmless.
- **P4.D276 Mnemo / Orla consolidation legs (Tier 3 #20)** — 💸 dogfood only.
- **P4.D276 Tier 3 #21** — `anchor-gate-probe` is CLI-only in v4 too. **#22** — the Cleanup-archives beat was retired by P4.D300 (L-10, Tier 3 #25). **#23** — nothing recorded.
