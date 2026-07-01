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

Phase 3 — the writer-task runtime (Unit 0) and the model-boundary core (Unit
0.5). Native infrastructure that replaces v4's child-process write machinery, so
verified by self-tests rather than a v4 oracle diff.

- `db::runtime`: `Db`, the `Clone + Send + Sync` handle every service holds — a
  per-partition read pool plus a `tokio::mpsc` write channel that is the only
  mutator. A dedicated OS thread owns the `WriterSet` (main + optional
  mount-index/llm-logs RW writers) and drains the channel serially, so batch
  apply stays serial (the property the folder-conflict remap and main-primary
  ordering assume). A write is a type-erased `FnOnce(&mut WriterSet)` closure
  carrying its own `oneshot` reply; `write_apply` remains available for the
  multi-DB job path, invoked inside a closure. Reads go direct to a pooled
  read-only connection (`PRAGMA key` first-and-only, per the read-path rule).
  API: `write` (async) / `write_blocking` / `read_main` / `read_mount_index` /
  `read_llm_logs`, plus `DbError::{WriterGone, WriterSpawn, PartitionUnavailable}`.
  Four self-tests: 100 concurrent writers serialize with no lost updates,
  read-after-write sees committed state, `write_blocking` commits, and a
  missing-partition read is a clean typed error.
- `model::embedding`: `EmbeddingProvider` (the tier-3 seam mirroring v4's
  `generateEmbeddingForUser`) with `EmbeddingResult` / `EmbeddingError` /
  `EmbeddingPriority`, plus `CannedEmbeddingProvider` — a deterministic responder
  keyed by exact input text (fixed vector; explicit failures for
  `SKIP_EMBEDDING_FAILED`; an unregistered input errors rather than answering).
  Async and generic (no trait object), three self-tests. The v4-oracle-side
  canned injection lands with Unit 1's memory-gate differential.
- Added `tokio` (`sync` only in the library — the writer is a plain OS thread, so
  no scheduler is pulled into the core; `macros`/`rt-multi-thread` dev-only).
- Docs: CLAUDE.md's "Never accept unverified Rust" corrected — `cargo
  build`/`test`/`clippy` do run in this environment and should be run before
  presenting Rust as done; the real-instance open + oracle diff remain the proof
  for crypto/cipher. Status sections (CLAUDE.md, `overview.md`, `phase-3.md`)
  updated for Units 0 and 0.5.

Phase 3 — the **memory gate** (Unit 1), the first decision service. Ported v4's
`createMemoryWithGate` / `runMemoryGate`, verified the new tier-3 → tier-2 way (a
canned embedding injected identically on both sides, then a structural DB diff).

- `services::memory_gate`: the pre-write similarity gate — `INSERT` /
  `INSERT_RELATED` / `REINFORCE` / `SKIP_NEAR_DUPLICATE` / `SKIP_EMBEDDING_FAILED`
  by cosine band (`NEAR_DUPLICATE_THRESHOLD` 0.90 / `MERGE_THRESHOLD` 0.85 /
  `RELATED_THRESHOLD` 0.70; the stale v4 header comment ignored). Async, generic
  over an `EmbeddingProvider`, reading off the read pool and funnelling every
  mutation through the writer thread — the first service to drive the whole Unit-0
  write path end to end. Reinforcement re-extracts novel details, appends
  footnotes, bumps count/importance, and re-embeds on a content change; related
  inserts bidirectionally link. Deferred (tracked): `maybeEnqueueHousekeeping`,
  the `skipGate` direct path, `applyNamePresenceCheck`'s cross-character lookup,
  and the 500 ms inter-retry delay.
- `db::vector_store`: the in-memory `CharacterVectorStore` shim (v4
  `vector-store.ts`) — load off a read connection, linear cosine top-K (stable
  descending, dimension guard), and an incremental flush (add/update/saveMeta)
  through the writer.
- `db::memories::MemUpdate` gained `embedding` (the `Some`-gated BLOB setter the
  gate writes through) and `related_memory_ids` setters; `dump_table_json_conn`
  lets the harness snapshot a table off a read-only pooled connection after a
  service commits.
- Differential: a tier-3 oracle drives v4's REAL `createMemoryWithGate` under jest
  (mocking only `generateEmbeddingForUser`, with the real cipher binding wired in
  via `better-sqlite3-multiple-ciphers`) over a seven-scenario corpus — one per
  outcome, each on its own character — and the Rust gate is diffed across
  `memories` + `vector_indices` + `vector_entries` in the shared-cross-table
  id-map remap form. Four core self-tests exercise the outcomes over an in-memory
  `Db` + canned provider.

Docs — Phase 2 marked complete; Phase 3 kickoff drafted. Docs only, no crate
source changed.

- `overview.md`: the Phase-2 roadmap row now reads repo-inventory-complete (every
  v4 repository round-trips green through the tier-2 harness), with the residual
  Phase-3-coupled deferrals named; the stale "nineteen repos" status prose was
  corrected the same way, and the document list + Phase-3 row now point at the new
  kickoff doc.
- `phase-2-onramp.md`: deferred seam #4 (`write_apply`'s `__finalizeFile` +
  post-commit effects) flipped from open to resolved, matching the
  ported-and-verified state.
- Added `docs/developer/porting/phase-3.md` — the Phase-3 kickoff: the tier-3
  mocked-LLM tier; the writer-task runtime (Unit 0); the tier-3 harness scaffold
  (Unit 0.5); the memory gate as first service (Unit 1), with a caution to port
  its similarity-band constants (0.90 / 0.85 / 0.70), not the file's stale
  0.80/0.70 doc comment; and the three Phase-2-carried deferrals.

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

Phase 2 — the `chats` repo, sub-unit 1: slim-row marshaling
(`quilltap-core::db::chats`). The first cut of the last and largest repo (v4's
`ChatsRepository`, a `TaggableBaseRepository`). Ports `create` / `update` /
`delete` over the **~96-column** `chats` table (MAIN db) — the widest marshaling
surface in Phase 2. Banks: the typed `participants` **array-of-objects JSON
column** (`ChatParticipant`, 18 fields in schema order, nullable optionals
`skip_serializing_if`, `displayOrder` an `i64`, `talkativeness` rendered the JS
way so an integer-valued `1.0` → `1`; the schema `.refine()` requires ≥1
participant); the simple JSON-array columns; the **plain-string** `turnQueue` /
`spokenThisCycleParticipantIds` columns (which hold JSON text `'[]'` but are
`z.string()`, bound raw); the number-affinity columns (all bound `f64`);
booleans; enum TEXT; and the long tail of nullable strings/uuids/timestamps. Two
invariants banked: `update` **never mints `updatedAt`** (it preserves the
existing value unless the caller passes one — only a new message bumps it), so
the whole differential is the pinned zero-normalization form; and on SQLite
`create` writes nothing to `chat_messages`. Verified by a tier-2 differential
(`chats_tier2_equivalence`) driving v4's REAL `ChatsRepository` over a
create×3 / update×3 (both the preserved- and explicit-`updatedAt` branches) /
delete sequence, diffing the `chats` dump byte-for-byte. **Tracked deferrals:**
`delete`'s participant-vault summary sweep (external subsystem), the open-JSON
object columns' multi-key insertion order (constrained to `{}`/single-key/null),
and the rest of the repo (messages, participants, impersonation, tokens, search,
outfits, read queries) — the remaining sub-units.

The `chats` repo — sub-unit 2: the **slim-row read path** (`db::chats_read`,
`chats_read_equivalence`). Ports the read marshaling (the inverse of sub-unit 1's
~96-column write = v4 `_findById` = hydrateRow + Zod parse) + the `findBy*`
queries (`findById` / `findAll` / `findByUserId` / `findByCharacterId` /
`findByType` / `findRecentSummarizedByCharacter`). The marshaling reproduces v4's
net read shape: nullable-optional columns OMITTED when `NULL` (v4 `undefined`
dropped by `JSON.stringify`), `.default(...)` numbers/bools/enums/arrays + `state`
(`{}`) materialized, numbers rendered the JS way, and `participants` re-parsed
per-element so each participant's own defaults materialize (`controlledBy: 'llm'`,
`displayOrder: 0`, `isActive: true`, `status: 'active'`, `hasHistoryAccess:
false`) and its nullable-optionals drop. `findByCharacterId` /
`findRecentSummarizedByCharacter` use the nested `participants.characterId`
`json_each` + `json_extract` match v4's query translator emits; the latter
reproduces the `$exists`/`$nin`/`$ne` → `IS NOT NULL` / `NOT IN` / `!=` filter +
`ORDER BY "lastMessageAt" DESC` + `LIMIT`. Verified by a read-differential: both
sides READ a copy of one fixture baked by v4's REAL `repos.chats.create` (seven
chats — a rich chat exercising every marshaling branch, a minimal chat, salon /
help / brahma types, summarized chats with distinct `lastMessageAt`), running 16
queries compared exactly (no normalization — nothing mutated).

The `chats` repo — sub-unit 3: the **`chat_messages` read path**
(`db::chats_messages_read`, `chats_messages_read_equivalence`). Ports v4's
`ChatMessagesOps` read surface — `getMessages` / `getMessageCount` /
`findChatIdForMessage`. Messages live in their own MAIN-db `chat_messages` table
(one row per event); `getMessages` reads every row for a chat ordered by
`createdAt` and validates each through `ChatEventSchema`, a three-member union
(`MessageEvent` / `ContextSummaryEvent` / `SystemEvent`). The marshaling
dispatches on the `type` discriminator and reconstructs each member: required
columns read directly, nullable-optional columns OMITTED when `NULL`, and the
array/object JSON columns (`rawResponse` [`z.record`], `attachments`,
`reasoningSegments`, `dangerFlags`, `hostEvent`, `customAnnouncer`, `carinaMeta`,
`pendingExternalAttachments`, `summaryAnchor`, …) parsed straight to JSON. No
read-side default materialization is needed: v4 runs `ChatEventSchema.parse`
*before* every insert, so each `.default(...)` (e.g. `attachments` → `[]`, a
`DangerFlag`'s `userOverridden` / `wasRerouted` → `false`) and the exact
int-vs-float number representation are already baked into the stored bytes.
Verified by a read-differential: both sides READ a copy of one fixture baked by
v4's REAL `repos.chats.addMessages` (one chat + twelve messages covering every
event member and JSON column), running 7 queries compared exactly (no
normalization). **Tracked seam:** `isSilentMessage` — its
`z.union([boolean, number.transform])` maps to TEXT affinity, so a stored boolean
round-trips as the string `"1"` and v4 drops the whole message on read; the
corpus keeps it absent and the column is not read here (close before reading real
data that sets it).

The `chats` repo — sub-unit 4a: the **`chat_messages` write path**
(`db::chats_messages`, `chats_messages_tier2_equivalence`). Ports v4's
`ChatMessagesOps.addMessage` / `addMessages` — the row insert plus the chat
metadata side-effect. The write marshaling is the inverse of sub-unit 3 but
harder: the port must reproduce `ChatEventSchema.parse`'s output bytes itself —
materialize each Zod `.default(...)` (`attachments` → `[]`, a `DangerFlag`'s
`userOverridden`/`wasRerouted` → `false`) and emit every JSON-column object in
schema field order (matching v4's `JSON.stringify` of a Zod-parsed object) with
integer-valued nested numbers rendered bare (`1`, not `1.0`), since the stored
bytes are compared directly. Each fixed-shape nested object (`dangerFlags`,
`reasoningSegments`, `hostEvent`, `customAnnouncer`, `carinaMeta`,
`summaryAnchor`, `pendingExternalAttachments`) is a typed struct in schema order;
the open-JSON `rawResponse` is corpus-constrained to `{}`/single-key (seam #5). A
`message` insert names the `MessageEvent` columns (always writing `attachments`);
a `context-summary`/`system` insert omits `attachments` so SQLite fills its
`DEFAULT '[]'` — matching v4's insert-only-validated-keys behavior. The metadata
side-effect recounts visible messages (`countVisibleMessages`), bumps
`lastMessageAt`/`updatedAt` to a minted `now` only for an actual `type:'message'`
event, and folds `spokenThisCycleParticipantIds` over the batch via the
already-ported `computeSpokenThisCycleAfterMessage`; it routes through the
sub-unit-1 `chats.update` (extended with `lastMessageAt` +
`spokenThisCycleParticipantIds` setters). Verified by a tier-2 differential
driving v4's REAL `addMessage`/`addMessages` over a kitchen-sink message (every
JSON column), a context-summary (non-actual: no `lastMessageAt` bump, `updatedAt`
preserved, count 0), and a mixed batch (whisper + system event + public message),
diffing BOTH the `chat_messages` and `chats` tables. `chat_messages` is pinned;
the `chats` `lastMessageAt`/`updatedAt` collapse to `<ts>` only when they differ
from the seed sentinel (so a preserved-sentinel `updatedAt` stays pinned and a
stray mint would be caught). The differential caught a real bug: serde's
`camelCase` rename produced `estimatedCostUsd`, dropping the schema's
`estimatedCostUSD` value — fixed with an explicit rename.

The `chats` repo — sub-unit 4b: the **`chat_messages` mutation path**
(`db::chats_messages`, `chats_messages_ops_tier2_equivalence`). Ports v4's
`updateMessage` / `deleteMessagesByIds` / `clearMessages`. `updateMessage`
reproduces v4's `{...existing, ...updates}` → `ChatEventSchema.parse` →
`$set: validated`: it reads the existing event (reusing the sub-unit-3 read),
overlays the update keys, re-validates into the typed `ChatEventInput`, and
DELETE + re-INSERTs the merged event — which yields the byte-identical row
(a validly-created row's non-member columns already sit at their DDL defaults, so
resetting them is a no-op) while reusing the 4a insert marshaling. A
freshly-added `dangerFlags` bakes its defaults; an untouched `reasoningSegments`
round-trips byte-for-byte; a context-summary's `attachments` stays at its
`DEFAULT '[]'`; a not-found id no-ops. `deleteMessagesByIds` deletes each
`(id, chatId)` row and, when any were removed, recounts `messageCount` (so
`update` preserves `updatedAt`); a nonexistent id removes nothing and leaves
metadata untouched. `clearMessages` deletes all of a chat's rows and resets
`messageCount`→0 + `lastMessageAt`→null (`updatedAt` preserved). Verified by a
tier-2 differential driving v4's REAL methods over a seed of three chats
pre-populated via `addMessages`, diffing BOTH the `chat_messages` and `chats`
tables with ZERO normalization — no 4b op mints a chat timestamp, so the seed's
baked timestamps are read identically by both sides.

The `chats` repo — sub-unit 5: the **participant ops** (`db::chats_participants`,
`chats_participants_tier2_equivalence`). Ports v4's `ChatParticipantsOps`:
`addParticipant` / `updateParticipant` / `removeParticipant` /
`setParticipantStatus` plus the four pure in-memory filters
(`getCharacter`/`getActive`/`getLLMControlled`/`getUserControlled`Participants).
Each mutator is a read-modify-write of the `participants` JSON column —
`findById` → mutate the array in memory (minting the participant's own
id/createdAt/updatedAt) → `update` the chat — and the chat's OWN `updatedAt` is
never bumped (v4 `_update` preserves it; the minted clock values live inside the
participants JSON). `addParticipant` validates through the participant schema
(materializing the Zod defaults, stripping unknown keys) and carries the
user-control side-effect (a `controlledBy: 'user'` participant is appended to
`impersonatingParticipantIds` and, when nobody is typing, set as
`activeTypingParticipantId`); `removeParticipant` carries the last-participant
guard (throws, leaving the chat unmutated). Banks the `removedAt` three-shape
seam: absent (never removed), the minted string (removed), and an explicit JSON
`null` (a `setParticipantStatus` to a non-removed status clears it) — which
forced widening `ChatParticipant.removedAt` to a double-`Option` with a
present-keeps-null deserializer (plain serde maps a stored `null` to the outer
`None`, dropping it; v4's Zod `.nullable().optional()` keeps it through a re-read
+ re-write). Tier-2 differential drives v4's REAL ops (with `setParticipantStatus`
reached via the private ops field — not on the repository surface) over four
seeded chats, diffing the `chats` table; participant ids (pinned seed + minted)
are remapped to first-appearance tokens across the three referencing cells, and
nested participant timestamps are sentinel-placeholdered (a value equal to the
seed sentinel stays pinned — proving createdAt preservation and no stray mint),
while chat-level timestamps are diffed exactly.

Phase-2 on-ramp — the real-snapshot fixture sanitizer (Deliverable B), a new
`quilltap-fixture-sanitizer` crate (library + `--source/--dest/--verify` CLI). It
takes a COPY of a real instance, recovers the pepper from the copy's `.dbkey` (in
memory only — never printed, logged, or written), sanitizes each database, and
re-keys the output under the committed throwaway test pepper. It is schema-frozen
by construction: the destination schema is replayed verbatim from the source's own
`sqlite_master`, every row is copied (row counts + the FK-id graph preserved), and
numbers / 0-1 booleans / enum tokens (by name + the `*Type`/`*Status`/`*Kind`/
`*Mode`/`*Role` suffixes) / timestamps / ids + UUID-valued TEXT are kept, while all
other TEXT is scrubbed to deterministic same-length pseudo-text, JSON columns are
deep-scrubbed to stay valid (keys / numbers / bools / uuid-and-enum leaves kept),
BLOBs become deterministic same-length bytes, and the document store's content↔sha
invariant is recomputed so a scrubbed file's `sha256` still matches its bytes.
Document-store PATH strings keep their structural skeleton (folder names + the
managed vault filenames like `properties.json`) so a sanitized vault still resolves,
scrubbing only the title stems. The scrub is one-way (`SHA-256(column ‖ original)`,
the original never appears in the output) and equality-preserving (identical
originals map identically, keeping content-dedup relationships). The binary refuses
a source path that looks like a live instance and never writes the `.dbkey`. Per the
project decision (2026-07-01) NO Friday-derived data is committed — the committed
test is synthetic (a re-key A→B round-trip proving the policy: structure preserved,
free text / JSON / BLOB scrubbed, content↔sha recomputed); real snapshots are
regenerated locally on demand. Verified locally against a copy of Friday: 188,031
main-db rows + 20,772 mount-index rows sanitized and re-keyed, 3,400 document-store
files re-hashed, and the sanitized output read back through the ported repos —
20,868 memories, 609 chats, and 33 characters (through the full vault overlay,
which resolves because the structural path segments are preserved) — marshaling
cleanly against real-shaped rows.

Phase-2 deferred-seam closure — ported the `characters` startup-backfill family,
closing the last three characters deferrals: the `ensureCharacterVault` adopt
branch, provision-on-the-fly, and physicalDescription-via-update. On a
managed-field `update` to a vault-less character, `apply_document_store_write_overlay`
now provisions a vault on the fly (build the post-cutover write input →
`ensure_character_vault` → re-read + confirm FK → continue routing) instead of
erroring. `ensure_character_vault` now first searches for a populated same-name
`'character'` store (`doc_mount_points::find_by_name` — `enabled=1`, trimmed
case-insensitive match) that passes the new `vault_has_required_files` check (all
six required files present in `doc_mount_file_links`) and adopts it when exactly
one qualifies (ambiguous or zero → fresh provision); the FK-write-and-confirm is
factored into the shared `link_character_to_vault`. The two seams compose — a live
`update` is how a character reaches the adopt branch. physicalDescription-via-update
(writing `physical-description.md` + `physical-prompts.json` on a non-null patch and
stripping it from the DB patch) was already coded; it is now proven. Each seam
ships a green six-table cross-DB shared-id-map remap differential
(`characters_adopt` / `characters_provision` / `characters_physical`
`_tier2_equivalence`) driving v4's REAL `repos.characters.update`/`.create`; the
adopt case asserts a single surviving mount point (the orphan store reused and its
FK relinked, no duplicate). Added `doc_mount_points::find_by_name` and
`doc_mount_file_links::relative_paths_lower`.

Phase-2 deferred-seam closure — closed the WRITE side of the
`chat_messages.isSilentMessage` seam (#8), completing it. The read side was
already resolved; this closes the write. A `message`-type insert now emits the
same TEXT-affinity bytes v4 stores: `true` → `"1.0"`, `false` → `"0.0"`, absent →
`NULL`. That representation arises because v4's `prepareForStorage(bool)` returns
the JS number `1`/`0`, better-sqlite3 binds it as a REAL, and SQLite converts the
REAL to text on store (`"1.0"`) — confirmed by a raw better-sqlite3 probe. The
Rust binding reproduces it by binding `Some(1.0_f64)` / `Some(0.0_f64)` / `None`;
context-summary / system inserts still omit the column so SQLite fills its DDL
default. Verified by a new `chats_messages_tier2` `addMessages` op carrying both a
`true` and a `false` silent message, byte-compared in the pinned `chat_messages`
dump against v4's REAL `addMessages`.

Phase-2 deferred-seam closure — ported the PUBLIC wardrobe write path (seam #7):
v4's `WardrobeRepository.create`/`update`/`delete`, in the new
`quilltap-core::db::vault_wardrobe_public`. These are v4's vault-only overrides —
resolve the owning character's document-store mount, read the current
`Wardrobe/*.md` items, apply the change, cycle-check, and re-project the folder,
throwing when no mount resolves (there is no SQL mirror). The prior
`wardrobe_tier2` port verified only the legacy base-SQL marshaling; this ports the
composition itself, over the already-verified leaves (`read_character_vault_wardrobe`
+ `project_vault_wardrobe` + `detect_component_cycles` + characters
`find_by_id_raw`), including the read-modify-project round-trip, the minted-`updatedAt`
on update, and the `assertNoCycles` guard (v4's exact `… → …; …` message). Verified
by a **read-back differential** (`vault_wardrobe_public_equivalence`) driving v4's
REAL public repo over a baked character+vault fixture: create, a composite create
referencing the first by id, a rename update, a cycle-forming update that throws, a
real delete (with the surviving composite's now-dangling ref DROPPING on read), a
delete of the already-gone id returning false, and a create against a non-existent
character that throws no-mount — comparing each op's read-back item list (minted
`updatedAt` normalized). A read-back tier rather than a table dump because
`build_wardrobe_item_file` writes the item's minted `updatedAt` into the
content-addressed `.md`, which a byte-level dump can't normalize; the projection
primitive is separately byte-verified (`vault_wardrobe_write_equivalence`). Scope:
the character tier only — the General/project archetype tiers stay deferred (same
boundary as `read_character_vault_wardrobe`). Four unit tests cover the patch merge,
cycle rejection, and the read→item conversion.

Phase-2 deferred-seam closure — ported the write applier's `__finalizeFile` +
post-commit side effects (seam #4), the last deferred pieces of
`quilltap-core::write_apply`. `__finalizeFile` now runs inside the main-DB
transaction loop (ensure-dir + staging→final rename), tracked so a later failure
in that partition undoes the renames in reverse before rethrowing; `cleanupStagingDirs`
drops the per-job `.staging/<jobId>` shell post-commit; and `dispatchInvalidations`
fires the deduped, ordered vector-store / mount-cache targets post-commit (both
skipped when the batch throws). The engine keeps v4's orchestration-vs-effect
split — the pure path/target computation (`path_dirname` = Node posix `dirname`,
`find_staging_root`, `collect_invalidations`) lives in the engine; the fs/cache
ops route through four new `ApplyHost` methods (production wires real fs/IPC; the
harness records them). The `write_apply_equivalence` trace differential grew four
observable fields (renames incl. undo-on-rollback, mkdirs, staging cleanup,
invalidation notifications) and three scenarios, verified against v4's REAL
`applyWritesUnsafe` — the oracle now records the fs mutators via jest `fs` mock +
the `notifyChild` mock (12 scenarios green). Also added four `write_apply` unit
tests.

Phase-2 deferred-seam closure — closed the `chat_messages.isSilentMessage` seam
(#8), and corrected its premise. The deferral claimed the TEXT-affinity round-trip
(`z.union([boolean, number.transform])` → TEXT) made v4's `getMessages` DROP a
silent message. Probed empirically against v4: it does NOT — a written `true` is
stored as numeric TEXT (`"1.0"`), and the read applies the row-schema union
(coerce to number, `=== 1`) → a real boolean, so the message is KEPT with
`isSilentMessage: true`. The real gap was that `db::chats_messages_read` never read
the column and so omitted the field. Fixed by reading `isSilentMessage` and
reproducing the coercion (numeric-TEXT `=== 1.0` → bool; `NULL` → omitted); the
read corpus gained a silent-message row proving the output matches the oracle. (The
write side does not yet emit the `"1.0"` representation — a bounded follow-up, since
the write corpus never sets it.)

Phase-2 deferred-seam closure — ported `TagVisualStyleSchema`'s per-field defaults
(seam #3). v4's base `_create` runs the doc through `TagSchema.parse`, so a PARTIAL
`visualStyle` gets its missing fields materialized; the Rust `TagVisualStyle` now
carries serde defaults matching each Zod `.default(...)` (`foregroundColor` →
`#1f2937`, `backgroundColor` → `#e5e7eb`, the four bools → `false`). `emoji`
(`.optional().nullable()`, no default) gained a double-`Option` + present-keeps-null
deserializer for the absent-vs-null trichotomy (absent → dropped as v4 `undefined`;
explicit `null` → kept). Proven by two partial-style tags corpus creates —
`{ bold: true }` (emoji dropped, all six defaults expand) and `{ emoji: null,
italic: true }` (emoji null kept) — each byte-identical to the oracle.

Phase-2 deferred-seam closure — closed the `toLowerCase` case-mapping seam
(`tags.nameLower`, `text_replacement_rules` conflict detection) by proving
`str::to_lowercase` is byte-identical to JS `String.prototype.toLowerCase`. Both
implement locale-independent Unicode default case mapping; verified empirically on
every gnarly case — `İ` → `i` + combining dot (`0069 0307`), a FINAL `Σ` → `ς`
(the context-sensitive Final_Sigma rule), `ß` (unchanged), `É`→`é`, and titlecase
digraphs (`ǅ`→`ǆ`). The evaluated `icu_casemap` option is therefore unnecessary —
no code change, just differential proof: the `tags` tier-2 corpus gained a tag
named `İSTANBUL ÉCOLE ΣΟΦΟΣ Straße` (whose stored `nameLower` matches the oracle
byte-for-byte), and `text_replacement_rules` a non-ASCII case-insensitive conflict
pair (`Café` then `CAFÉ`, both lowercasing to `café`) that fires duplicate
rejection identically on both sides. With the collation seam (above) this closes
the whole Unicode-fidelity cluster.

Phase-2 deferred-seam closure — added ICU collation (`icu` 2.2, ICU4X) as
`quilltap-core::collation::locale_compare`, closing the `localeCompare` seam. v4
sorts several lists with `a.localeCompare(b)` (no locale) — true ICU collation,
not the code-unit order Rust's `str: Ord` gives. Node's no-arg `Intl.Collator`
resolves to en-US / tertiary (probed against ICU 78); `Collator::try_new` returns
a `CollatorBorrowed<'static>` over the baked compiled data (held in a `LazyLock`),
and ICU4X's tables match Node's for common Latin + accents (verified the order
`a,A,ä,b,B,e,é,z,Z` and the pairwise signs). The two ported `localeCompare` sites
now use it — `compareVersions`' malformed-input fallback and `canonicalize`'s
tool-name array sort — and each differential gained a divergent row (mixed
case/accents, e.g. `apple` < `Banana`) that exercises the ICU path against the
oracle, where code-unit order would disagree. The `canonicalize` `parameters`
key-sort stays code-unit (v4 uses `Object.keys().sort()` there, not collation).
Future Phase-3 name sorts reuse `locale_compare`. (The `toLowerCase` case-mapping
seam is separate and closed next.)

Phase-2 deferred-seam closure — proved the open-JSON multi-key key-order fix (#5)
end-to-end. With `preserve_order` enabled (below), a MULTI-KEY value in
deliberately NON-SORTED key order was added to each affected corpus and its
differential re-run green, confirming the port emits v4's `JSON.stringify`
insertion order rather than sorted keys: `plugin_config.config`,
`character_plugin_data.data`, `image_profiles.parameters`,
`connection_profiles.parameters`, `chat_settings.tagStyles`, `chats.state` +
`chats.sillyTavernMetadata`, and `chats_outfits.equippedOutfit` (a key-order chat
that appends a higher-sorting characterId before a lower one). Refreshed the
now-stale `chats_outfits` doc comment (it described the pre-`preserve_order`
sorted-key seam). Corpus-only; no Rust logic change.

Phase-2 deferred-seam closure (begins) — enabled `serde_json`'s `preserve_order`
feature workspace-wide (both crates), so every `Value::Object` is an `IndexMap`
emitting INSERTION order, matching v4's `JSON.stringify`. This is the locked
decision for the open-JSON multi-key key-order seam (`parameters` / `config` /
`equippedOutfit` / `sillyTavernData` / `state` / `tagStyles` / `data` / …), which
the typed-struct trick could not cover. Foundational + no-regression: the full
suite stays green (the existing single-key corpora are order-invariant), and it
makes the harness stricter — a re-serialized `Value` now preserves on-disk key
order instead of sorting, so a masked key-order difference would surface (none
did). Per-column multi-key corpus proofs follow as each affected repo is swept.

The `chats` repo — sub-unit 6: the **remaining four ops files**, ported in
parallel (four agents, each on its own new module + differential; the shared
`ChatUpdate` setters + `mod.rs` wiring pre-staged serially). This **completes the
`chats` capstone** — the entire `ChatsRepository` public surface is now ported.
- **impersonation** (`db::chats_impersonation`, `chats_impersonation_tier2_equivalence`):
  v4 `ChatImpersonationOps` — `addImpersonation`/`removeImpersonation`/
  `getImpersonatedParticipantIds`/`setActiveTypingParticipant`/
  `updateAllLLMPauseTurnCount`. RMW on `impersonatingParticipantIds` +
  `activeTypingParticipantId` (the activeTyping reassign-or-clear on remove) +
  `allLLMPauseTurnCount`; mints nothing, so the differential is zero-normalization.
- **tokens** (`db::chats_tokens`, `chats_tokens_tier2_equivalence`):
  v4 `ChatTokenTrackingOps`. `incrementTokenAggregates` lowers v4's `$inc`/`$set`
  to one self-referential `UPDATE … SET col = col + ?` with an unconditionally
  minted `updatedAt` and a conditional `estimatedCostUSD = current + cost` (+
  `priceSource`); `resetTokenAggregates` zeroes the counters + nulls the cost via
  `update` (preserving `updatedAt`). Sentinel-aware `updatedAt` normalization
  (increment mints → `<ts>`; reset preserves → pinned, diffed exactly).
- **search** (`db::chats_search`, `chats_search_equivalence`):
  v4 `ChatSearchReplaceOps` — `countMessagesWithText`/`findMessagesWithText`/
  `searchMessagesGlobal`/`replaceInMessages`. The `searchMessagesGlobal`
  `$regex`→SQL `LIKE` translation reuses `memories`' exact mangling
  (`escapeRegex` → `source.replace(/\.\*/g,'%').replace(/\./g,'_')`, bare `LIKE`,
  no `ESCAPE`), reproducing v4's broken-but-exact behavior on regex-special
  inputs; the role filter + `createdAt DESC` + `limit`; and the split/join
  replace-all (which mints nothing). Read-differential over the method results +
  the post-replace `chat_messages` dump.
- **outfits** (`db::chats_outfits`, `chats_outfits_tier2_equivalence`): v4's
  `getEquippedOutfit`/`getEquippedOutfitForCharacter`/`setEquippedOutfit`/
  `removeEquippedItemFromAllChats` (in `chats.repository.ts`). RMW on the
  `equippedOutfit` JSON column, stored as **raw `Value`** (v4 never re-validates
  it through Zod), so partial / extra-key slots objects are preserved verbatim —
  the remove path mutates each character's slots in place, dropping the item only
  from slots it was actually in (v4's `before.includes` guard), never
  materializing absent slots. Corpus banks a partial-slot character to prove the
  shape-preservation. **Tracked seam:** the open-JSON key-order divergence
  (`serde_json::Value` sorts; v4 emits insertion order) — corpus constrained to
  sorted key order, same as `parameters`/`sillyTavernData`.

Build — extracted the SQLite3MC (ChaCha20/sqleet) amalgamation into a dedicated
`quilltap-sqlite3mc-sys` crate (its `build.rs` + `vendor/`, moved out of
`quilltap-core`). Cargo's build-script fingerprint includes the package version,
so the per-commit version bump on `quilltap-core` used to throw away the cached
`libsqlite3.a` and recompile the 12 MB amalgamation from scratch (~4 min). The
sys crate's version is pinned, so that C compile now caches across our version
bumps: a `quilltap-core` version bump rebuilds in ~2 s instead of ~4 min. No
`links` key (libsqlite3-sys already claims `sqlite3`); `quilltap-core` depends on
the sys crate and references it as `use quilltap_sqlite3mc_sys as _;` so its
link-search flags reach the final binary. Cipher behavior unchanged, verified by
the tier-2 differentials still opening real ChaCha20 databases.

Phase 2 — the `memories` repo, ported whole
(`quilltap-core::db::memories` + `db::memories_read`). A plain main-DB
`AbstractBaseRepository<Memory>` (no overrides except the `embedding` BLOB
registration, no vault overlay), so every read is a single-connection SELECT +
marshal. Ports the full surface: the write/mutation side (`create` with embedding
BLOB + JSON-array columns + the three numeric columns — `importance` /
`reinforcedImportance` are INTEGER-affinity, `reinforcementCount` REAL, all bound
`f64`; `update` leaving the BLOB untouched; `delete`; `updateForCharacter` /
`deleteForCharacter` ownership gates; `bulkDelete`; `updateAccessTime{,Bulk}`;
`replaceInMemories`; `deleteByChatId` / `deleteBySourceMessageId{,s}`) and the
read side (all ~30 `findBy*` / `count*` queries, incl. the `$regex` → SQL `LIKE`
mangling reproduced byte-for-byte, the `findByCharacterAboutCharacters` window
function, `findByCharacterIdPaginated`'s in-memory search, and the importance
tiers). Banks a marshaling seam: the normal `findByFilter` path omits NULL
nullable-optional columns (v4's `undefined` dropped by `JSON.stringify`), but the
raw-SQL `findByCharacterAboutCharacters` path keeps them as `null` (its rawQuery
rows carry explicit NULLs that `MemorySchema.safeParse` retains) — the port
mirrors both. Verified two ways: a tier-2 differential (`memories_tier2_equivalence`,
the write/mutation sequence, minted-timestamp placeholder form) and a
read-differential (`memories_read_equivalence`, 39 queries over a v4-baked fixture,
zero normalization — nothing mutated, so no minted timestamp; a returned
embedding is the `Float32Array` `{"0":…}` object rebuilt from the BLOB).

Phase 2 — the `CharactersRepository` read path
(`quilltap-core::db::characters_read`), characters sub-unit 4c — the capstone's
last piece. Ports the slim-row read marshaling (row → `Character`, the inverse of
sub-unit 2's write marshaling = v4 `hydrateRow` + Zod parse) + the `findBy*`
queries, each overlaying the character vault. The marshaling reproduces v4's net
read shape over the slim columns: required strings present; `.nullable().optional()`
TEXT/UUID/JSON cells **omitted** when `NULL` (v4 emits `undefined`, dropped by
`JSON.stringify`) and parsed when present; `.default(false)` booleans coerced from
INTEGER; `.nullable().optional()` booleans omitted/coerced; `.default([])` arrays
parsed (`NULL`/empty → `[]`); `controlledBy` defaulting to `'llm'`. The managed
columns sit at their DDL defaults, so it reproduces their Zod defaults directly
(`scenarios`/`systemPrompts`/`aliases` → `[]`, `talkativeness` → `0.5`, the nullable
managed fields omitted); for a vault-linked character the read overlay then
overwrites every managed field. Queries: `find_by_id` / `find_by_id_raw` /
`find_all` / `find_by_user_id` / `find_user_controlled` / `find_llm_controlled` /
`find_by_ids` / `find_by_default_image_id` / `find_by_avatar_override_image_id` /
`find_by_tag` (the last two via SQLite `json_each`, matching v4's query translator).
Verified by a read-differential (`characters_read_equivalence`): both sides READ a
copy of one fixture baked by v4's REAL create (four characters + vaults), run the
same 11 queries, and compare the hydrated lists exactly (ids/timestamps identical —
no remap — only `physicalDescription`'s read-minted createdAt/updatedAt
placeholdered, lists sorted by id). `findByIdRaw` isolates the slim marshaling (no
overlay). Also refactored sub-unit 4b's array ops to ride this full `find_by_id`
(re-verified green), closing the scoped-reader deferral.

Phase 2 — the `CharactersRepository` array / sub-array ops
(`quilltap-core::db::vault_character_arrays`), characters sub-unit 4b. Ports the
`systemPrompts` / `scenarios` / `partnerLinks` mutators + the
`setFavorite` / `setControlledBy` / `setCanBeCarina` setters. Each sub-array op is
v4's three-beat shape: `find_by_id` (the read overlay) → mutate the array in memory
(applying the per-op `onBeforeAdd` / `onAfterBuild` / `onAfterRemove` default
normalization) → `update_character` (the 4a write overlay) reprojects the
`Prompts/` / `Scenarios/` folder (or writes the slim `partnerLinks` column). The
minted item `id` / `createdAt` / `updatedAt` never reach disk — the projection
writes `<sanitize(name|title)>.md` from `build_system_prompt_file` /
`build_scenario_file`, and the read side re-derives a prompt's id from its path —
so the DB effect is deterministic. Added a scoped `find_by_id` (the slim columns
the ops consume — `id` / `characterDocumentMountPointId` / `partnerLinks` — plus
the overlaid `systemPrompts` / `scenarios`; full slim-row read marshaling is
sub-unit 4c). The setters are thin `update_character(id, { … })` wrappers (no read,
no vault). Verified by a tier-2 differential (`characters_arrays_tier2_equivalence`)
over a fixture baked by v4's REAL create (one baked prompt / scenario / partner
link), driving v4's REAL repository methods across SIX tables in the
shared-cross-db-id-map remap form (`chunkCount`/`doc_mount_chunks` pinned/excluded);
the id-taking prompt/scenario ops carry a `targetName` / `targetTitle` resolved to
the current id via `findById` on each side. Banks addSystemPrompt (default-demote +
non-default), updateSystemPrompt (rename → sweep + content), setDefaultSystemPrompt,
deleteSystemPrompt (deleting the default → survivor promotion), the three scenario
ops, the two partner ops, and the three setters.

Phase 2 — `applyDocumentStoreWriteOverlay` + the `CharactersRepository.update`
integration (`quilltap-core::db::vault_character_update`), characters sub-unit 4a.
The managed-field write **router** — distinct from sub-unit 1's create-time writer
(which projects every field unconditionally): the update path routes only the
fields **present in the patch**, and `properties.json` is a **read-modify-write**
(a patch touching only `title` preserves pronouns/aliases/firstMessage/
talkativeness). Routes markdown (`None`→`""`), the properties RMW (seeded from the
current `properties.json`, falling back to the empty-managed default), physical
(non-null writes the two files; null leaves them), and `systemPrompts`/`scenarios`
(reproject the folder — sweep + write). Returns the unmanaged remainder;
`update_character` runs the slim `_update` for it (skipped when empty — a
managed-only update does NOT bump the slim row's `updatedAt`). The DB-bound
remainder is marshaled back through the slim repo's typed update. Verified by a
tier-2 differential (`characters_update_tier2_equivalence`) over a fixture baked by
v4's REAL create, driving v4's REAL `repos.characters.update` across SIX tables
(slim `characters` row + the five store tables) in the shared-cross-db-id-map remap
form (`chunkCount`/`doc_mount_chunks` pinned/excluded). Banks markdown routing, the
properties RMW preserving untouched keys (asserted), a DB-only field update
(`isFavorite` true→false → slim `_update`), and a `systemPrompts` reprojection
(sweep the old `Prompts/Default.md`, write the new one) on a managed-only update —
the orphan-on-rewrite + sweep-GC row counts matching v4 byte-for-byte via the
shared DDL. Added the public `render_properties_json` (the RMW serializer, reusing
the create-time `properties.json` shape + the `talkativeness` js-number rule) and
`DocMountFileLinksRepository::ensure_folder_path`'s sibling read
`link_exists_at_path` (used by 3a). **Tracked deferral:** provision-on-the-fly (a
patch with managed fields on a vault-less character) — the corpus always has a
vault; lands with the startup-backfill slice.

Phase 2 — `ensureCharacterVault` + the `CharactersRepository.create` integration
(`quilltap-core::db::character_vault`), characters sub-unit 3b — the store-backed
capstone's keystone. `create_character` runs v4's full create end-to-end: the
slim-row `_create` (FK nulled — a fresh character always provisions a fresh vault),
then `ensure_character_vault` mints a `<name> Character Vault` mount point
(mount-index DB), scaffolds its preset structure, projects the managed fields
(`write_character_vault_managed_fields`, sub-unit 1), and links it by setting
`characterDocumentMountPointId` on the slim row (main DB) — confirming the write
stuck (v4's `linkCharacterToVault` turns a silent "linked but not linked" into a
loud error). A character spans two databases, so the differential
(`characters_create_tier2_equivalence`) drives v4's REAL `repos.characters.create`
and diffs SIX tables — the main slim `characters` row + the mount-index store
tables (`doc_mount_points` / `_folders` / `_files` / `_documents` / `_file_links`)
— in the shared-cross-db-id-map remap form (nothing pinned; every id minted, FKs
verify by relationship; timestamps placeholdered; the link `chunkCount`
pinned and `doc_mount_chunks` excluded, as for groups/projects). Banks the 6-step
create, the **orphan-on-rewrite** default-`properties.json` file/document row (the
scaffold writes it, then the managed bag overwrites it; `writeDatabaseDocument`
does no GC, so the old row persists — 9 files, 8 live + 1 orphan), the five
identity markdown overwrites (the `physical-*` scaffold defaults survive — no
physicalDescription), and one systemPrompt + one scenario projected into `Prompts/`
+ `Scenarios/` (10 links). **Tracked deferral:** the `ensureCharacterVault` adopt
branch (startup-heal of a hand-linked same-name store) — the corpus always
provisions fresh; it needs a richer `doc_mount_points` read and lands with the
startup-backfill slice.

Phase 2 — `scaffoldCharacterMount` (`quilltap-core::db::character_vault`),
characters sub-unit 3a (the store-backed capstone's stateful provisioning glue,
mount-index DB). Populates a freshly-created database-backed character store with
the preset structure: seven empty top-level folders (Prompts/Scenarios/Wardrobe/
Outfits/lore/images/files), six blank Markdown files
(identity/description/manifesto/personality/physical-description/example-dialogues,
content `""`), and two seeded JSON files (`properties.json` +
`physical-prompts.json`, FIXED default content). The six blank files share the
empty-string content sha, so they dedup to ONE `doc_mount_files` /
`doc_mount_documents` row with six distinct links; result: 7 folders, 3 files, 3
documents, 8 links. All writes go through the verified storage primitive — folders
via the new `DocMountFileLinksRepository::ensure_folder_path` (v4 `ensureFolderPath`,
walks the path directly so a single segment makes one root folder; a sibling of
`ensure_link_folder_id` which walks a file's dirname), files via
`write_database_document` (idempotent, skip-if-link-exists). Verified standalone
(the create flow's `writeCharacterVaultManagedFields` overwrites the five identity
markdown files + `properties.json`, so the create differential would mask the
scaffold defaults — verifying here pins the default bytes). Tier-2 differential
(`characters_scaffold_tier2_equivalence`) drives v4's REAL `scaffoldCharacterMount`
and diffs five mount-index tables (points / folders / files / documents / links) in
the shared-cross-table-id-map remap form; the seeded `mountPointId` is pinned, the
link `chunkCount` (a `reindexSingleFile` artifact) pinned and `doc_mount_chunks`
excluded (as for groups/projects).

Phase 2 — the `characters` repo **slim-row marshaling**
(`quilltap-core::db::characters`), the first sub-unit of v4's
`CharactersRepository` (the store-backed capstone). Ports the base-repository SQL
CRUD (`_create`/`_update`/`_delete`) over the MAIN-db `characters` table. v4's
public `create`/`update` orchestrate the character vault (provision + project +
overlay) — a later sub-unit; both strip the `MANAGED_FIELDS` set (identity,
description, manifesto, personality, exampleDialogues, pronouns, aliases, title,
firstMessage, talkativeness, physicalDescription, systemPrompts, scenarios) before
the SQL write, leaving the non-managed "slim row" this differential checks. A
fresh fixture's table still has the managed columns (`ensureCollection` generates
them from `CharacterSchema`), but both sides omit them from every write, so they
sit at their DDL defaults identically. Banks the **widest nullable-boolean surface
in Phase 2** — seven `z.boolean().nullable().optional()` columns
(`defaultAgentModeEnabled`, `defaultHelpToolsEnabled`, `canDressThemselves`,
`canCreateOutfits`, `systemTransparency`, `coreWhisperEnabled`, `canBeCarina`),
INTEGER 0/1 when present, SQL NULL when absent — plus a typed JSON-object column
(`defaultTimestampConfig`, a nine-field struct in schema order so the compact JSON
matches `JSON.stringify` key order, NOT `serde_json::Value`), an open JSON column
(`sillyTavernData`, kept `null`/single-key per the multi-key seam), two
typed-struct array columns (`partnerLinks` `{partnerId,isDefault}`,
`avatarOverrides` `{chatId,imageId}`), a string-array column (`tags`), two
boolean-default columns (`isFavorite`/`npc`), an enum TEXT column (`controlledBy`),
and many nullable UUID columns. `update` is a partial `SET` that reproduces v4's
full `$set` on-disk result (the fixture cells are already in validated canonical
order). Verified by a tier-2 differential (`characters_slim_tier2_equivalence`)
driving v4's REAL protected internals via a thin subclass over a create / create /
update / delete sequence, diffing the `characters` table in the pinned
zero-normalization form (ids + timestamps pinned both sides).

Phase 2 — the `background_jobs` repo (`quilltap-core::db::background_jobs`), v4's
`BackgroundJobsRepository` — the durable work queue (memory extraction, context
summaries, embedding generation, autonomous room turns, …). A
`UserOwnedBaseRepository` (a `userId` column) with NO base-method override, so
`create`/`update`/`delete` honor pinned id/createdAt/updatedAt; on top of CRUD it
ports the full queue API. Banks three **REAL-affinity** number columns
(`priority`/`attempts`/`maxAttempts` — all bare `z.number().default(N)` → REAL,
NOT INTEGER; integer-collapsed in the dump) and the open-JSON `payload` column
(kept `{}`/single-key per the multi-key key-order seam). Ports and verifies the
queue ops: `claimNextJob` (atomic `SELECT … ORDER BY priority DESC, createdAt ASC
LIMIT 1` then UPDATE in a transaction, `attempts += 1`), `markFailed` (exponential
backoff `min(30·2^attempts, 300)`s, DEAD-vs-FAILED on `attempts >= maxAttempts`),
`markCompleted`, `pause`/`resume`, `cancel`, `cancelByType`, `resetAllProcessingJobs`,
`resetStuckJobs`, and `deleteByTypesAndStatuses` — with the exact `lastError`
strings byte-for-byte (`"Cancelled by user"`, `"Superseded by new reindex"`, the
em-dash `"Orphaned on startup — killed"`, `"Timed out after N minutes"`). The
nested-JSON path finders (`findPendingForChat`/`ForEntity`) reproduce v4's
`json_extract(payload, '$.chatId')` translation. Verified by a tier-2 differential
(`background_jobs_tier2_equivalence`) driving v4's REAL repo over a 13-op sequence
and diffing the table in the minted-timestamp placeholder form (ids + createdAt +
every deterministic column — status/attempts/lastError/payload/priority/maxAttempts
— diffed EXACTLY; only the four mintable timestamp columns placeholdered).
**Discovered v4-on-SQLite limitation:** `markCompleted`'s dotted `payload.result`
merge throws `no such column: payload.result` on v4's SQLite backend (no dotted
JSON sub-key translator), so that path is unreachable there; the port keeps the
merge as a forward v5 capability (via the pure `merge_result_into_payload`, three
unit tests) and the differential exercises only the no-result path (v4's working
behavior).

Phase 2 — the `vector_indices` repo (`quilltap-core::db::vector_indices`), v4's
`VectorIndicesRepository`. The first **standalone two-table** repo — it does NOT
extend the base repository; it manages `vector_indices` (per-character metadata)
+ `vector_entries` (per-embedding rows) in the MAIN db directly. Banks the third
Float32-BLOB embedding column (little-endian via `embedding_blob::float32_to_blob`,
`None`/empty → SQL NULL, never a zero-length blob; dumped as hex for a bit-exact
compare), two REAL-affinity number columns (`version`/`dimensions`, bare
`z.number()` → REAL, integer-collapsed in the dump), and a `saveMeta` upsert keyed
by `characterId` (`id == characterId`, so the meta `id` is pinned, not minted).
Reproduces v4's exact op semantics: `addEntries` mints one shared `createdAt`
across the batch; `removeEntries` is a per-id delete loop (not a single `IN (…)`);
`updateEntryEmbedding` touches only the embedding column (no timestamp);
`deleteByCharacterId` is two independent ops (entries then meta), not one SQL
transaction. Verified by a tier-2 differential (`vector_indices_tier2_equivalence`)
driving v4's REAL repo over a full op sequence (saveMeta create/update, addEntry,
addEntries, updateEntryEmbedding, removeEntries, and a `deleteByCharacterId` that
wipes a second character entirely) and diffing both tables in the minted-values
remap form (entry `id` remapped, timestamps placeholdered, `characterId`/embedding
pinned).

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

Phase 2 — the character vault **managed-fields write projection**
(`quilltap-core::db::vault_character_write::write_character_vault_managed_fields`),
v4's `writeCharacterVaultManagedFields` — the first piece of the `characters`
repo (a `TaggableBaseRepository` with a bespoke vault overlay, not a generic
store-backed entity). Projects every vault-managed content field of a character
out to its file, in v4's exact order: `properties.json` (the typed
`pronouns`/`aliases`/`title`/`firstMessage`/`talkativeness` bag, 2-space
pretty-print), the five markdown files (`identity` / `description` / `manifesto`
/ `personality` / `example-dialogues`, `None` → `""`), and — only when a primary
`physicalDescription` is present — `physical-description.md` +
`physical-prompts.json` (`renderPhysicalPromptsJson`), then the `Prompts/` and
`Scenarios/` folder projections. Composes the already-ported pure leaves
(`build_system_prompt_file` / `build_scenario_file` / `sanitize_file_name`) and
the folder projector (`project_array_into_vault_folder`) over the document-store
write primitive. `properties.json` feeds the content-dedup SHA, so an
integer-valued `talkativeness` (e.g. `1.0`) is serialized as the bare integer `1`
(a `serialize_with` mirroring `js_number_to_json`) to match `JSON.stringify`
byte-for-byte; the five `properties.json` keys are a typed struct (serde
preserves struct field order, unlike `serde_json::Value`). Verified by a tier-2
differential (`vault_character_write_equivalence`) driving v4's REAL
`writeCharacterVaultManagedFields` over a two-op sequence (a full create with a
`Prompts/` filename collision `Default Voice.md`/`Default Voice-1.md` and two
scenarios, then a reproject that sweeps the dropped prompt + both old scenarios,
clears `physicalDescription` — physical-* files PERSIST, v4 skips and does not
delete — and renders `talkativeness: 1`) and diffing five mount-index tables in
the shared-cross-table-id-map remap form; plus four exact unit tests. v4's
post-write reindex runs (database-backed chunking, no model); its only divergence
(link `chunkCount` + `doc_mount_chunks`) is pinned/excluded, exactly as the
groups/projects/wardrobe store-backed tests do.

Phase 2 — the character vault **wardrobe write projection**
(`quilltap-core::db::vault_wardrobe_write`), v4's `projectVaultWardrobe` +
`projectArrayIntoVaultFolder` — the final wardrobe write piece, and with it the
whole document-store slice is complete. Re-projects an authoritative
`WardrobeItem` list into a vault store's `Wardrobe/` folder: each item is written
as `Wardrobe/<title>.md` (filename collisions disambiguated with `-1`/`-2`/…
suffixes), any `.md` file in the folder not produced by the current list is swept,
and the legacy `wardrobe.json` is deleted so the folder layout is the single
on-disk source. Composes the already-ported pure leaves
(`build_slug_by_item_id_map`, the Decision-A `build_wardrobe_item_file` emitter,
`sanitize_file_name`) over the document-store write primitive
(`write_database_document`) and a new GC delete (`delete_database_document` +
`delete_with_gc`: unlink, then drop the file row when its last link is gone —
chunks/documents cascade via the FK). Verified by a tier-2 differential
(`vault_wardrobe_write_equivalence`) driving v4's REAL `projectVaultWardrobe` over
a two-op sequence (an initial 5-item projection with a `Hat.md`/`Hat-1.md`
filename collision and a composite emitting `componentItems` slugs, then a rename
that sweeps the old file + recomputes the composite's slug and removes two items)
and diffing five mount-index tables (`doc_mount_points` / `_files` / `_documents`
/ `_file_links` / `_folders`) in the shared-cross-table-id-map remap form. v4's
post-write reindex runs (database-backed chunking, no model); its only divergence
(link `chunkCount` + `doc_mount_chunks`) is pinned/excluded, exactly as the
groups/projects store-backed tests do.

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

