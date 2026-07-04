# CLAUDE.md

Guidance for Claude Code when working in **quilltap-v5** — the next-generation
**native** Quilltap. This file is loaded every turn, so it stays short and points
at deeper docs. **The rules in "Standing rules" are not optional.**

## What this repo is

This is the ground-up native rewrite of Quilltap, currently a Next.js/React app
that lives in the separate **`quilltap-server`** repo (referred to here as **v4**;
its docs are mirrored under `docs/v4/`). v4 is the **reference oracle** — it
defines correct behavior. quilltap-v5 reimplements that behavior natively and is
checked against v4 mechanically (see "The differential port discipline").

**Target stack (decided June 2026):**

- **Core:** Rust. A portable engine (`quilltap-core`) holding the data layer,
  memory subsystem, job orchestration, and the single-writer invariant.
- **Frontend:** **Angular 21+** (zoneless, signals, standalone) — *not* React.
  Served as an SPA inside the Tauri webview.
- **Shell:** **Tauri 2** (desktop now; iOS/Android later via Tauri-mobile, with
  `uniffi`-generated Swift/Kotlin bindings over the same Rust core as the
  fallback path).
- **CLI:** a `quilltap` binary linking `quilltap-core` (first real consumer; v4's
  `npx quilltap` is its oracle).

Design docs (read before large changes), all under `docs/developer/porting/`:
`overview.md` (start here — methodology + phase roadmap + status),
`phase-0.md` (Phase-0 plan + the cipher finding), `api-boundary.md` (the
transport-agnostic boundary + the single-writer model + the enclave `step()`
seam), `phase-2-onramp.md` (the tier-2 DB-state oracle + fixtures — the Phase-2
machinery, now complete), `document-store-overlay.md` (the store-backed-entity
slice: `projects`/`groups`/`characters`/`wardrobe` vault — where the document
store lives, the overlay engine, and the build order), `phase-3.md` (the Phase-3
kickoff — the tier-3 mocked-LLM tier, the writer-task runtime, the tier-3 harness
scaffold, and the memory gate as first service). The `docs/v4/` tree is the v4
reference mirror, not v5 planning.

## Standing rules (apply on every task)

### Spelling — non-negotiable

The project is **"Quilltap"** (quill + tap), **never** "Quilttap". Never write
"quilttap" anywhere.

### ⚠️ The database cipher is ChaCha20/sqleet, NOT SQLCipher

The single most expensive fact in this port. Every identifier in v4 says
"sqlcipher" (`ENCRYPTION_MASTER_PEPPER`, `sqlcipherKey`), and `docs/v4/.../
DATABASE_ENCRYPTION.md` *wrongly* claims SQLCipher — but v4 sets no `cipher=`
pragma, so it uses the default cipher of `better-sqlite3-multiple-ciphers`:
**sqleet = ChaCha20-Poly1305**. Confirmed empirically (`PRAGMA cipher` →
`chacha20`).

- **Do NOT use `rusqlite` + `bundled-sqlcipher`** — it is AES-only and returns
  `NotADatabase` on every real Quilltap DB. (The retired `sqlcipher-probe` crate
  demonstrated this in Phase 0; don't reintroduce a bundled-sqlcipher feature.)
- The real DB layer links **SQLite3MultipleCiphers** (utelle), version matching
  what v4 bundles (**2.3.5**, on SQLite 3.53.2 in the matching amalgamation),
  opened with its default sqleet cipher — no `cipher=` pragma needed. The
  amalgamation is compiled by the dedicated **`quilltap-sqlite3mc-sys`** crate
  (`crates/quilltap-sqlite3mc-sys/build.rs`, vendored under its `vendor/`) and
  linked as `sqlite3` for the whole workspace; `quilltap-core` depends on it (the
  `db` module is the first consumer). That sys crate's version is **pinned and
  never bumped** so the 12 MB C compile caches across our per-commit version
  bumps — bumping it would force the ~4-min amalgamation recompile.
- **Two different ciphers — never conflate:** the `.dbkey` *file* wraps the
  pepper with **AES-256-GCM + PBKDF2** (that part of v4's docs is right; ported
  in `quilltap-core::dbkey`). The *databases* are **ChaCha20**.

### Opening a database (must match v4 byte-for-byte)

- Pepper → key via the **raw-hex form**: `PRAGMA key = "x'<hex>'"` (KDF skipped;
  we already derived via PBKDF2 when unwrapping `.dbkey`). The hex is
  `base64-decode(pepper) → hex`.
- `key` is the **first and only** pragma before the first read on a read-only
  open. **Do not** issue `journal_mode`/`foreign_keys` on a read path — mutating
  `journal_mode` on an existing encrypted file forces header writes that race the
  cipher context and surface as `NotADatabase`. (The writable path adds
  `foreign_keys = ON` + `journal_mode = TRUNCATE` — TRUNCATE not WAL, for
  cloud-sync safety, since instances live in iCloud/Dropbox.)

### The differential port discipline (the core methodology)

An AI-heavy port of a subtle system cannot be verified by inspection. **Every
ported unit arrives with an equivalence test against the v4 oracle.** Never
accept a port without one.

- **v4 is the oracle.** `harness/oracle/` runs from the v4 checkout
  (`npx tsx`), imports the **real** `lib/` code (never reimplements it), runs a
  fixed deterministic corpus, and emits NDJSON.
- **`quilltap-harness`** runs the same corpus through the Rust port and diffs
  field-by-field. Three tiers: (1) **exact** for pure functions (1e-12 for
  floats, exact for strings); (2) **structural DB diff** for repo/service ops
  (normalize legitimately-nondeterministic fields — timestamps, generated UUIDs
  via a remap, LLM text); (3) **mocked-LLM** for model-dependent paths (inject
  the same canned response both sides, then tier-2 on the writes).
- **Port leaf-to-root, pure-to-stateful.** Phase 1 pure functions → Phase 2 data
  layer → Phase 3 services/enclave → Phase 4 transports + Angular.
- **Small units.** One module/function per change, each independently
  oracle-checked. Carry forward v4's *why*-comments (the subtle invariants are
  what a port silently drops).
- **The schema does not change during the port.** Same tables, same UUIDs, same
  cipher. The Rust core opens the exact DB file v4 writes.

### Never accept unverified Rust

Rust **does** build and test in this environment (`cargo build`/`cargo test`/
`cargo clippy` all run — rustup toolchain 1.96.0, plus the native DB build deps;
the amalgamation C compile caches after the first build). So compile + run the
tests before presenting Rust as done — a green `cargo test` is the baseline, not
a thing to defer to the user. But a passing local test is **not** the full proof
for crypto/cipher paths: those are proven by the **real-instance open** (opening
the actual encrypted Friday data — needs the real pepper, never in-sandbox) and
the **differential oracle diff** (which imports v4's real `lib/` from the
`quilltap-server` checkout). "Looks right" — and even "compiles and the unit test
passes" — is not enough there; flag when a change still awaits the real-data /
oracle proof, and flag version-specific crate API risks explicitly.

### Architectural invariants to preserve from v4

- **Single writer.** v4's parent-is-sole-DB-writer rule (forked child + buffered
  writes over IPC) becomes, in Rust, a type/ownership rule: only the writer task
  holds the RW connection; a channel is the only mutator. **Keep** the
  correctness parts that aren't Node workarounds: per-database partitioned apply
  (main / mount-index / llm-logs, each its own transaction), main-primary vs
  idempotent ordering, the folder-conflict id remap.
- **Enclaves must not assume an always-on host.** Model an autonomous run as a
  persisted `step()` + `RunState` state machine with cadence injected by a
  per-host driver. iOS background limits (~30s windows) break overnight runs in
  *any* language — design for resume-on-open / optional companion server now.
- **Transport-agnostic boundary.** One `Request`/`Response`/`Event` contract;
  transports (Tauri IPC, uniffi, an axum HTTP shim for CI) are thin. No business
  logic above the boundary. Streaming only ever on the `Event` channel.

## Repo layout

```
Cargo.toml                 # workspace root (members = crates/*)
rust-toolchain.toml        # pinned channel 1.96.0
crates/
  quilltap-sqlite3mc-sys/  # link-only: build.rs + vendor/ compile & link the
                           #   SQLite3MC (ChaCha20/sqleet) amalgamation for the
                           #   whole workspace. Version PINNED (keeps the 12 MB C
                           #   compile cached across our version bumps).
  quilltap-core/           # the portable engine (lib). Modules: dbkey, db
                           #   (cipher-correct DB layer), memory_weighting, …
                           #   depends on quilltap-sqlite3mc-sys for the cipher.
  quilltap-harness/        # differential tests vs the v4 oracle (tier-1 + tier-2).
  quilltap-fixture-sanitizer/ # tool: sanitize a COPY of a real instance into a
                           #   test-pepper-keyed fixture (scrub free text/BLOBs,
                           #   preserve structure; real pepper never persisted).
  (future) quilltap-cli, quilltap-tauri
harness/oracle/            # Node/tsx bridge driving v4's real lib/ code.
apps/web/                  # (future) Angular 21 SPA.
docs/v4/                   # mirror of the v4 server docs (reference only).
```

The two Phase-0 probe crates (`sqlcipher-probe`, `sqlite3mc-probe`) have been
retired: the amalgamation build lives in the `quilltap-sqlite3mc-sys` crate (it
moved out of `quilltap-core` so the expensive C compile stays cached across
version bumps), and their findings are recorded here and in
`docs/developer/porting/phase-0.md`.

## Working environment

- **Toolchain:** rustup, pinned via `rust-toolchain.toml` (channel **1.96.0**).
  Don't paste the placeholder — use the real version. A `rust-toolchain.toml`
  is an *override file*: an invalid `channel` makes every `cargo` command in the
  tree fail.
- **Native build deps:** Xcode CLT (clang) + `cmake`. The DB build compiles the
  SQLite3MC amalgamation via the `cc` crate; `buildtime_bindgen` needs Clang.
- **`Cargo.lock` is committed** (this repo produces binaries). The `.gitignore`
  still lists it from the Phase-0 scaffold — that's inconsistent; prefer keeping
  the lock tracked and removing the ignore line.
- **macOS dev:** account for BSD tool variants; GNU coreutils/`gnu-sed` are
  installed under `g`-prefixed names.
- **Plan large changes with the most capable model; delegate well-specified
  subtasks to cheaper agents.** Don't use `git stash`/worktrees with agents.

## Running the differential harness

```bash
# 1. generate oracle output from the v4 checkout (imports real lib/ code)
cd ~/source/quilltap-server
npx tsx ~/source/quilltap-v5/harness/oracle/cases/memory-weighting.ts > /tmp/oracle-weighting.ndjson
npx tsx ~/source/quilltap-v5/harness/oracle/cases/ranking-blend.ts    > /tmp/oracle-ranking.ndjson

# 2. run the Rust diff (env vars point at the NDJSON; tests skip if unset)
cd ~/source/quilltap-v5
QT_ORACLE_WEIGHTING=/tmp/oracle-weighting.ndjson \
QT_ORACLE_RANKING=/tmp/oracle-ranking.ndjson \
  cargo test -p quilltap-harness
```

A standalone self-test (`now_constant_matches_iso`) guards the harness's own
fixed clock/date math against drift — run `cargo test -p quilltap-harness` with
no env vars to exercise it.

## Verifying / opening a real instance (Friday)

Friday lives at `~/iCloud/Quilltap/Friday`; DB files are in `data/`. To open a
**copy** (never the live file) from Rust, point `quilltap-core::dbkey` at the
data dir — it reads and decrypts `quilltap.dbkey` itself (no env var, no saved
pepper). iCloud may evict file contents to placeholders; if a copy opens with 0
tables, force-download the source (`brctl download …`) before copying. The pepper
is the master key to all data — never commit it, never write it where it syncs.

## Conventions

- **Writing voice:** user-facing strings (UI, help, prompts) keep v4's
  *steampunk + Roaring-20s + Wodehouse + Lemony Snicket* register. `CHANGELOG`
  is the exception — terse, plain American English.
- **Feature/personified-system names** carry over from v4 (the Salon, Aurora,
  Prospero, the Scriptorium, the Commonplace Book, the Lantern, the Concierge,
  Pascal, Carina, the Librarian, the Host, etc.). When porting a subsystem, keep
  its name and its `systemSender` semantics.
- **Character fields are four distinct vantage points** plus `manifesto` —
  identity / description / personality / title are **not interchangeable**;
  never collapse them. (Full definitions: `docs/v4/.../` and the v4 CLAUDE.md.)
- **Principles:** encapsulation, single source of truth, SRP, DRY, KISS, YAGNI.

## Hard stops (ask first)

- **No stubs or `TODO` code** unless agreed in advance.
- **Don't change the on-disk schema or cipher** during the port — it breaks the
  oracle comparison and existing instances.
- **Database writes against a real instance:** operate on a **copy**. Never point
  a writable open at live Friday data.
- **Don't initiate a release.** This repo's release process isn't established
  yet; set it up deliberately, don't improvise.

## Status (update as it moves)

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

The rest of wave 4 (the remaining tool subsystem — the text tool loop (W4.1f) /
`buildTools` / registry (W4.1g), the remaining handler-catalog batches [2 wardrobe,
3 doc-edit, 4 embedding/search, 5 host-seamed], the agent-mode resolver — danger,
answer-confirmation, courier/compression-cache/regenerate-swipe, carina query,
buildContext seam-closers, provider manifest) follows per the chat-orchestration
decomposition.
