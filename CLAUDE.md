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
scaffold, and the memory gate as first service), `phase-4.md` (**the current
phase** — transports + host drivers + the Angular SPA: the 22 locked decisions,
incl. the first-class no-auth HTTP/Docker deployment, the crate layout, the
tier-4 verification strategy, and the P4.0–P4.7 decomposition). The `docs/v4/`
tree is the v4 reference mirror, not v5 planning.

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
  quilltap-host/           # the composition root (P4.0): boots quilltap-core::api's
                           #   CoreEngine, owns ALL cadence (job pump / stuck reset /
                           #   enclave tick), instance registry + path resolution.
  (future) quilltap-web, quilltap-cli, quilltap-tauri
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
