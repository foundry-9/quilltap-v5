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
completion half joins as `model::completion` when chat orchestration lands. The
v4-oracle-side canned injection (stubbing `generateEmbeddingForUser` to the same
vector) is exercised end-to-end by **Unit 1's** memory-gate differential (below).

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
remapped through the shared map). **Tracked deferrals:** `maybeEnqueueHousekeeping`
(fire-and-forget), the `skipGate`/`createMemoryDirect` direct path,
`applyNamePresenceCheck`'s cross-character resolution (needs the characters
vault-overlay read; corpus keeps `aboutCharacterId` null → verified no-op), and the
500 ms inter-retry delay (host-timing, no DB effect, omitted to keep the core
scheduler-free).

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
plus four repo self-tests. Next: the remaining memory-family follow-ons
(`memory-service`'s cascade-delete family + housekeeping, then the model-dependent
`memory-processor` per-turn extraction) per `docs/developer/porting/phase-3.md`.

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
The answer-confirmation *service* itself and the cheap-LLM `profileParameters`
forwarding fix (same v4 commit) remain unported Phase-3/Phase-4 work — this
catch-up only closes the marshaling gap on surfaces already ported.
