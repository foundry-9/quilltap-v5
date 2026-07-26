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
- **v5 never changes the schema *unilaterally*.** Same tables, same UUIDs, same
  cipher; the Rust core opens the exact DB file v4 writes. **v5 does FOLLOW v4's
  schema when v4 moves it** — by re-dumping `fresh_schema.json` from v4's live
  `generateDDL`, never by hand, and never inventing a change of our own (**D23**
  in `phase-4.md`; first applied for v4 4.8.0's `pascalMeta`/`customTools`). A
  red `provisioning_equivalence` after a drift check is that tripwire firing as
  designed — **it is v4 drift, not a v5 bug; do not "fix" v5 back to the old
  schema.** The migration runner stays deferred, so v5 cannot open a v4 instance
  older than the columns it now expects.

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
  quilltap-web/            # the axum HTTP transport (P4.2): dispatch + SSE +
                           #   binary routes + terminal WS + static serving.
  quilltap-cli/            # the `quilltap` binary (P4.3): dual-mode
                           #   (direct-core / HTTP client), v4's npx quilltap
                           #   as oracle.
  quilltap-tauri/          # the Tauri 2 desktop shell (P4.7): invoke dispatch,
                           #   the event pump, the qtap protocol delegating
                           #   into quilltap-web's router, terminal paired IPC.
harness/oracle/            # Node/tsx bridge driving v4's real lib/ code.
apps/web/                  # the Angular 21 SPA (zoneless, signals, standalone).
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

## Status (phase-level summary — the full log lives elsewhere)

**The unit-by-unit porting log is `docs/developer/porting/status-log.md`.**
Append new units, differentials, banked findings, deferrals, and round
records THERE. Update this summary only when a phase or round completes.

- **Phase 0 (scaffolding + differential harness): done.** Toolchain pinned
  (1.96.0); the cipher resolved (SQLite3MC/ChaCha20, NOT SQLCipher) and
  confirmed on real Friday data; `.dbkey` decryption ported; the harness
  proven.
- **Phase 1 (pure-function ports): done.** Every leaf family tier-1 exact
  against the v4 oracle (memory weighting/ranking, turn manager, context
  shaping, JS number/string/regex fidelity, the ICU/Unicode cluster closed:
  ICU4X en-US collation; `str::to_lowercase` byte-identical to JS).
- **Phase 2 (data layer): done.** All repos across the three partitions
  (main / mount-index / llm-logs), the document-store overlay engine
  (groups/projects) + the character/wardrobe vault (read + write), the
  minted-values remap machinery, the partitioned write applier. Every
  deferred seam closed (2026-07-01).
- **Phase 3 (services / engine): done (2026-07-08).** The single-writer
  runtime, the model boundaries, the whole memory family, chat
  orchestration (buildContext + the processMessage spine + the turn chain +
  turn-skipping), the tool subsystem (all 57 tools + both tool loops), the
  provider layer (declarative manifests, the five stream decoders, request
  builders, transport, pricing/logging/embeddings incl. the TF-IDF builtin),
  the post-office personified writers, the in-process job runner, and the
  enclave engine (`step()` + schedule tick). Every unit
  differential-verified against v4's real code.
- **Phase 4 (transports + hosts + the Angular SPA): in progress**
  (`docs/developer/porting/phase-4.md` — the 22 locked decisions + the
  decomposition). Done: P4.0 (boundary + composition root, M0), P4.1 (the
  four host-driver lanes: provider IO / files+images / PTY+Ariel /
  environment+cadence), P4.2 (`quilltap-web` HTTP transport + the
  production chat spine, M2), P4.3 (the `quilltap` CLI Tier R, M1), the
  P4.d drift re-ports (answer-confirmation, turn-skipping), P4.4 units 1–2
  (fresh-instance provisioning; chat creation + the Green Room), P4.5 (the
  Angular SPA foundation), P4.6a–e (the Salon vertical [M4, run LIVE] +
  the Salon consolidation + the Settings vertical, both with live
  Playwright walks). **Dogfooding a COPY of real Friday data is underway**
  (`docs/developer/porting/dogfood-findings.md` — findings #1/#2/#3a fixed,
  #3b ordered).
- **The P4.6f/g/h + P4.4u3 round: UNIFIED on main (2026-07-10).** Characters
  server slices 1–3 (reads / action verbs / sub-resource mutations) ∥ the
  Characters SPA (roster / detail / edit / create screens) ∥ Salon
  virtualization (dogfood finding #3b CLOSED) ∥ the built-in seeds (roleplay
  templates + the three mount stores). Slice 4 (create/update, wardrobe
  mutations, tags CRUD, depiction-guidelines, stats) unified 2026-07-11.
- **The P4.6i ∥ P4.6j characters-remainder round: UNIFIED on main
  (2026-07-11) — the P4.6f/g/i/j orders are all CLOSED.** All eight
  characters refusal arms LIVE (delete cascade + preview / per-character
  chats / photo gallery JSON legs / ST import-export JSON) + the SPA
  Conversations tab, delete flow, gallery, and ST import/Export-JSON, with
  three live e2e beats. The characters family's remaining deferrals are
  enumerated loud refusals (the tier-3 LLM services, the wardrobe dialog).
- **The P4.6k ∥ P4.6l ∥ P4.6m groups+projects+multipart round: UNIFIED on
  main (2026-07-11) — P4.6m CLOSED.** The groups + projects (Prospero)
  dispatch surface ∥ the Groups + Prospero SPA verticals (+ the
  characters upload/PNG riders and the dogfood-#6 select audit) ∥ the
  quilltap-web multipart machinery closing the photo-upload /
  photo-save-fileid / ST-PNG deferrals.
- **The P4.6n ∥ P4.6o ∥ P4.4u4 scenarios+import round: UNIFIED on main
  (2026-07-11) — P4.6n/P4.6o/P4.4u4 CLOSED, closing P4.6k, P4.6l, and
  P4.4u3's family-3 deferral with them.** The whole scenarios surface
  (group/project/general + participant-union + `list-files` + file
  add/remove) ∥ the scope-agnostic ScenariosManager + Wardrobe SPA
  cards + the general `/scenarios` page ∥ the quilltap-import seed
  subset + the startup sample-content seed (**default ON** — a fresh
  boot seeds Lorian + Riya + 42 memories) + `reset_builtins` (dispatch
  at the web edge). **No refusal arms remain in the
  groups/projects/scenarios surface.**
- **The P4.6p ∥ P4.6q ∥ P4.6r listing-surfaces + New-Chat round:
  UNIFIED on main (2026-07-12) — P4.6p/q/r CLOSED, closing the P4.6l
  listing-surface picker gaps.** The three global listing surfaces
  (roleplay templates + image profiles + global mount points, four
  new differentials over the extended groups-projects fixture) ∥ the
  New-Chat vertical (`/salon/new` + the Green Room over the global
  event stream, live e2e walk) ∥ the Templates & Images settings tabs
  + the three default-* pickers + reset-builtins enabled. Still
  refusal-armed: `imageProfileGenerate`/`ValidateKey`/`ListModels`;
  the mount-point action verbs have no variants (the Scriptorium
  surface).
- **The P4.6s ∥ P4.6t ∥ P4.6u Commonplace Book + terminal-pane round:
  UNIFIED on main (2026-07-12) — P4.6s/t/u CLOSED.** The memories
  dispatch surface (26 live variants, a 41-case differential over the
  new memories fixture; refusal-armed:
  `memoryGenerateEmbeddings`/`RebuildIndex`/`chatQueueMemories`) ∥
  the Commonplace Book SPA (the character Memories tab + the Settings
  Memory tab, e2e beats activated at unification) ∥ the Salon
  terminal pane (xterm.js + the split-pane scaffolding Document Mode
  reuses, live PTY e2e). Unification wired the embedding seam live
  (`EngineAssembly.memory_embedding` — memoryCreate/memorySearch run
  live in the real server). Deferred loud: extract-memories-dry-run +
  CLI memory-diff, memory-dedup, embedding-profiles management,
  conversation-summaries regen.
- **The P4.6v ∥ P4.6w ∥ P4.6x Document Mode + Scriptorium-server round:
  UNIFIED on main (2026-07-12) — P4.6w/P4.6x CLOSED; P4.6v OPEN
  (partial).** The whole Document Mode server surface (operator-doc-
  actions + `STANDALONE_CHAT_ID`, 11 chat-scoped + 7 standalone
  variants, chat_documents recents/move-sync, the qtap-target byte
  route) ∥ the Document Mode SPA (pane + picker + split integration +
  live e2e; **D17 Document-Mode spike RED** — markdown ships in the
  byte-exact textarea, ProseMirror is the named next decision) ∥ the
  P4.6v partial landing (chunker + pure leaves, the mounts fixture
  family, the READ/LIST keystone with `mountFilesList`/`mountFileRead`).
  **P4.6v units 4–9 remain OPEN** (write/ops/scan/blobs/convert +
  reindex/embed; D7 not yet closed; the `mount_refresh` seam stays
  unwired until they land — see the order header). Next candidates:
  finish P4.6v, the Scriptorium SPA (D18, over the frozen file-ops
  surface), the courier/images Salon slices, autonomous-rooms settings,
  or P4.7 (Tauri) — see phase-4.md.
- **The P4.6y mount-file-ops remainder round: UNIFIED on main
  (2026-07-13) — P4.6y and P4.6v CLOSED, D7 CLOSED,
  `EngineAssembly.mount_refresh` WIRED LIVE.** The whole Scriptorium
  mutation + indexing surface (single lane): the `storeMountFile`
  ingest pipeline (all three branches) + the blob routes, the
  file/folder mutation verbs + PATCH + folder-create, reindex / scoped
  embed / semantic search, the scanner + `mountScan`, the web-edge fs
  raw read + three multipart legs, and convert/deconvert refusal-armed
  behind v4's live capability guards. Eight differentials green over
  fresh `a7b1398d` oracles; full Playwright 29/29 at unification (the
  document beats exercise the live refresh seam). Refusing seams
  (loud, named): the production pdf/docx `DocumentTextExtractor`, the
  production WebP codec, `conversion.ts`, the chokidar-equivalent fs
  watcher (+ the db-store-event emitter chain), the `quilltap docs`
  CLI. Next candidates: the Scriptorium SPA (D18), the ProseMirror
  editor (D17), the courier/images Salon slices, autonomous-rooms
  settings, or P4.7 (Tauri) — see phase-4.md.
- **The P4.6z ∥ P4.6aa Scriptorium-SPA round: UNIFIED on main
  (2026-07-13) — P4.6z/P4.6aa CLOSED, D18 DECIDED.** The `/scriptorium`
  + `/scriptorium/:id` SPA vertical (grid + five dialogs +
  DirectoryPicker + FileTable) over the frozen file-ops surface, plus
  the one new server variant `systemBrowseDirectory` (route
  differential over the committed `browse-fs-tree/` fixture) ∥ the D18
  decision lane: the ngx-explorer 5.0.2 spike ran GREEN on its gating
  checks but adoption was REJECTED (no move/copy verb; a second theming
  engine) — the bespoke `qt-file-manager` shipped over the ported v4
  SVAR adapter helpers, behind the store detail's "New file manager
  (beta)" toggle (the unification wire), + the dogfood-#6 select audit
  (2 conversions, 7 proven safe). Gate: 305 Rust suites, ng test 546,
  full Playwright 33/33 with the file-manager walk ACTIVE. Deferred
  loud: the `/files` files-family page (server surface unported),
  FilePreview, the workspace-tab drill, cross-mount move/copy UI, drag
  relocation. The banked v4 drift `6a8a77aa` (nudge → a persisted Host
  announcement) was re-ported 2026-07-13 (see the status log): writer
  builders + the once-only orchestrator announcement + the SPA
  "invited to speak" chip, verified by the extended post-office-host
  tier-1 and the regenerated orchestrator tier-3 differentials.
  Next candidates: the ProseMirror editor (D17), the courier/images
  Salon slices, the files-family server surface, autonomous-rooms
  settings, or P4.7 (Tauri) — see phase-4.md.
- **The P4.6ab ∥ P4.6ac ∥ P4.6ad courier+images + autonomous round:
  UNIFIED on main (2026-07-13) — P4.6ac/P4.6ad CLOSED; P4.6ab tier 1
  LANDED, tier 2 OPEN.** The courier + chat-images dispatch surface
  (resolve/cancel external turn, save-image, photo-albums,
  add-tool-result, chat-files list/delete; 15-check differential over
  the new `courier-images-{main,mount}.db` fixture) ∥ the whole
  courier + images Salon SPA (Courier bubble, thumbnails + ImageModal,
  the markdown store-image rewrite, SaveImageDialog +
  PhotoGalleryModal, the generate dialog, composer attach + conflict
  flow, 2 live e2e beats) ∥ the full autonomous-rooms vertical (seven
  verbs over the frozen `enclave::lifecycle`, 24-case differential
  over the new `autonomous-{main,mount}.db` fixture, the Settings
  Chat tab + EditEnclaveModal + New-Chat toggle + shell badges, 3
  live e2e beats — the P4.6q autonomous deferral CLOSED). The same
  unification absorbed the two terminal branches: the count-baseline
  spec fix + the LIVE `TerminalLivenessProbe`
  (`EngineAssembly::terminal_probe` — the P4.2-era chat-GET
  stub-probe deferral CLOSED; the walk grew kill→re-attach→exit).
  Wires: `courier_resolve` + `save_image_bytes` live in the host;
  `imageProfileGenerate` params reconciled (STILL refusal-armed);
  lane B's beats seeded from lane A's fixture (pinned-id remap — the
  fixture families collide — + vault mounts); the e2e instance
  gained its llm-logs partition (committed `salon-llm-logs.db`).
  Gate: 307 Rust suites / 1294 tests (three fresh-oracle
  differentials by name), clippy both feature sets, ng test 618,
  full Playwright 38/38, every new beat ACTIVE. **P4.6ab tier 2
  stays OPEN (loud refusals + recipes):** the chat-file multipart
  upload leg (composer attach degrades inline until then) and the
  `imageProfileGenerate` un-refusal. Next candidates: the P4.6ab
  tier-2 remainder, the files-family server surface, the ProseMirror
  editor (D17), the Salon in-chat Edit-Enclave entry + salon-list
  autonomous toggle, or P4.7 (Tauri) — see phase-4.md.
- **The P4.6ae ∥ P4.6af ∥ P4.6ag files-family + editor round: UNIFIED
  on main (2026-07-14) — P4.6af/P4.6ag CLOSED, P4.6ae OPEN (partial).**
  The nine-verb general files dispatch surface (25-case differential
  over the new committed `files-{main,mount}.db` fixture) ∥ the
  `/files` SPA vertical (legacy FileBrowser + preview + dialogs +
  shell nav) + the salon autonomous riders (Edit-Enclave header entry;
  include-autonomous toggle + hint, live 3/3 walk) ∥ **D17 DECIDED:
  ProseMirror ADOPTED (gate GREEN)** — the bespoke `qt-rich-editor`
  (v4-dialect markdown bridge, 28-entry byte-round-trip gate) adopted
  in the Document Mode pane AND the chat composer, with input rules +
  commands + live dialect-bytes beats. Gate: 310 Rust suites / 1318
  tests, clippy both feature sets, ng test 691, Playwright 45 passed +
  1 guarded skip (the files data beat awaits the upload REST leg).
  **The P4.6ae remainder stays OPEN (its order header enumerates it):
  the P4.6ab tier-2 close-out (chatFileUpload + `imageProfileGenerate`
  over the still-missing `EngineAssembly.image_generation` seam), the
  `fileUpload` variant + upload REST leg, thumbnails/cleanup verbs,
  the itemized FILE_HAS_ASSOCIATIONS envelope + dissociate arm.**
  Next candidates: finish P4.6ae, the D17 tier-3 editor follow-ons
  (form-field consumers, tables), the deferred autonomous-rooms
  cards, or P4.7 (Tauri) — see phase-4.md.
- **The P4.6ah ∥ P4.6ai ∥ P4.6aj ∥ P4.d4 "finish P4.6ae + catch up
  from v4" round: UNIFIED on main (2026-07-14) — all four orders
  CLOSED, and P4.6ae + P4.6ab (tier 2) CLOSE with them.** The files
  write + maintenance server remainder (chat-file upload +
  `action=link`, `fileUpload` + the upload REST leg, the itemized
  FILE_HAS_ASSOCIATIONS envelope + dissociate, the three maintenance
  verbs — `files_routes_equivalence` 25 → 41 cases) ∥ the
  `imageProfileGenerate` un-refusal over the NEW
  `EngineAssembly.image_generation` seam wired LIVE in the host (4-case
  differential) ∥ the SPA delete-associations close-out (REDUCED
  v4-faithful: no v4 client sends `force` — dissociate-only) ∥ the
  `02865bdb` skip-signal trailing-sentinel re-port (106-row
  differential). Wires: the P4.6af guarded files data beat
  self-activated over the live upload leg; a composer-attach live-leg
  beat added. Files-family deferrals remaining (loud, named):
  `filesSync`, attach-mount-file, thumbnail generation (codec),
  cleanup-stale disk keys, auto-describe,
  `imageProfileValidateKey`/`ListModels`. Next candidates: the D17
  tier-3 editor follow-ons, the deferred autonomous-rooms cards, P4.7
  (Tauri), or a files-story dogfood pass — see phase-4.md.
- **The P4.6ak ∥ P4.6al ∥ P4.6am D17-editor-follow-ons + salon-dogfood
  round: UNIFIED on main (2026-07-14) — ALL THREE CLOSED, and dogfood
  findings #7/#8/#9 + the finding-#6 select audit CLOSE with them.**
  The text-replacement-rules surface + `chatGetBackground` (server,
  new committed fixture + 15-case differential;
  `regenerate-background` refusal-armed) ∥ strike/highlight marks +
  emphasis-on-type rules + the shared `qt-markdown-field` (memory
  editor + character edit/new fields) + composition mode + drafts +
  the text-replacement plugin/card ∥ the chained-response streaming
  render + chat background display + the last select fix. Wires: the
  CoreRequest union folded; the salon composer bindings; the
  background beat LIVE; three new live composer beats. Gate: 314
  suites/1327 tests, ng 764, Playwright 52/52. Deferred loud: the
  background GENERATION subsystem, the item-6 form-field adoptions,
  the table transformer, missing-host dialog consumers. Next
  candidates: the remaining form-field adoptions (a rider), the
  autonomous-rooms deferred cards, P4.7 (Tauri), or a dogfood pass.
- **The P4.6an Chat-tab-cards + cron-preview round: UNIFIED on main
  (2026-07-15) — P4.6an CLOSED, and the last two P4.6ad deferrals
  CLOSE with it.** Single lane: the eleven remaining Chat-tab
  settings cards in v4's full 16-card order (shared ChatSettingsCard
  substrate — sixteen cards, ONE deduped GET; the tab placeholder
  retired), the live `croner@10.0.1` cron next-run preview in the
  shared autonomous room card (all three consumers), the composer
  spellcheck rider (ProseMirror attributes + setProps nudge), four
  live e2e beats. The one server gap: `dangerousContentSettings`
  parse made Zod-faithful (explicit nulls kept, partial bags
  defaulted, `1` not `1.0`) — `settings_routes_equivalence` 19 → 32
  cases, fresh `02865bdb` oracle. Gate: 314 suites/1327, ng 846,
  Playwright 56/56 zero skips. Deferred loud: the Salon token/cost
  display rendering (a Salon slice). Next candidates: the Salon
  token/cost display, the background generation subsystem, the
  form-field adoptions (a rider), P4.7 (Tauri), or a Settings-story
  dogfood pass — see phase-4.md.
- **The P4.6ao ∥ P4.6ap ∥ P4.6aq token-cost + background-generation +
  form-fields round: UNIFIED on main (2026-07-15) — ALL THREE CLOSED,
  and the P4.6an token/cost, P4.6ak/am background-generation, and
  P4.6al item-6 deferrals CLOSE with them.** The `chatGetCost` verb
  (raw un-enveloped body) + the `regenerate-background` un-refusal
  (edge-only; a latent `projectId`-omission bug in the shared enqueue
  caught and fixed) + the TITLE_UPDATE handler (the live loud-failure
  closed; automatic background generation now fires), three fresh
  differentials over the new `cost-background-{main,mount}.db` family
  ∥ the per-message token badge + chat-totals header summary + the
  Story Backgrounds Images-tab card + the Regenerate Background entry
  with both polls ∥ the `qt-markdown-field` minHeight input + eleven
  form-field adoptions (three async-loading hosts got v4's
  loading-gate structure). Wires: the §1/§2 types folded into
  CoreRequest; both ACTIVATE-AT-UNIFY beats LIVE; `image_profiles`
  joined the e2e userId rewrite. Gate: 317 suites/1341, ng 968, full
  Playwright 60/60 zero skips. Deferred loud: the minHeight residual
  at the P4.6al-adopted sites, the Default Aesthetics card, the
  LLM-Inspector button, backdrop arbitration, the no-host dialogs.
  Next candidates: P4.7 (Tauri), a token-cost/backgrounds/editor
  dogfood pass, or the small-rider pool — see phase-4.md.
- **The P4.6ar ∥ P4.6as ∥ P4.6at LLM-Inspector + Default-Aesthetics +
  minHeight round: UNIFIED on main (2026-07-15) — ALL THREE CLOSED,
  and the P4.6ao-round Inspector / aesthetics-card / minHeight
  deferrals CLOSE with them.** The llm-logs read surface (eight repo
  reads, `llmLogsList`/`llmLogGet`/`llmLogDelete` + REST edges; v4's
  `?standalone=true` carried BROKEN-BUT-EXACT — `$eq: null` lowers to
  `= NULL` and can never match; the garbage-limit NaN quirk via
  hand-rolled `js_min`) + the `systemImageAestheticsGet`/`Set` pair
  over DRY'd `services::aesthetics`, two differentials (27 + 13
  cases, incl. a wire key-order assertion) over the new four-file
  `inspector-*` fixture family ∥ the LLM-Inspector SPA vertical
  (slide-over panel — `role="dialog"` declared only while OPEN, a
  documented divergence from v4's permanent phantom modal —
  entry/panel, toolbar button + Cmd+Shift+L, per-message cpu icon,
  the reconcile-point log refresh, a live seeded-partition walk) ∥
  the shared `aesthetic-editor-field` extraction (a re-port that
  corrected textarea-era drift) + the Default Aesthetics Images-tab
  card + the sixteen minHeight bindings. Wires: §1/§2 folded into
  CoreRequest; `p4_6ar_wire_contract`; both beats LIVE (the
  aesthetics beat grew a reload round-trip). Gate: 320 suites/1347,
  ng 1107, full Playwright 63/63 zero skips. Deferred loud: the boxed
  summary variant + `detailed=true`, backdrop arbitration, the
  no-host dialogs, the source-mode toggle, the GFM table transformer,
  the cost-estimator consolidation, the stale "serde_json sorts keys"
  seam-note sweep (`preserve_order` is on). Next candidates: P4.7
  (Tauri), an Inspector/aesthetics/token-cost dogfood pass, or the
  small-rider pool — see phase-4.md.
- **The P4.7a ∥ P4.7b Tauri round: UNIFIED on main (2026-07-16) — BOTH
  CLOSED; P4.7 (the decomposition's last lettered step) LANDED.** The
  `quilltap-tauri` shell (tauri 2.11.5): boot via shared quilltap-web
  helpers, §1 invoke `dispatch`/`health`, §2 the `quilltap://event`
  pump with Green-Room backlog replay (+ `quilltap://resync` on lag),
  §3 the `qtap` custom protocol delegating the full http::Request into
  the reused quilltap-web router (that's how the whole raw REST/byte/
  multipart surface came free), §4 terminal paired IPC over Channel
  (frozen WS unions), the 6-test tier-4 IPC contract suite ∥ the SPA
  D14 seam made real: the `CoreTransport` split (HTTP byte-for-byte
  frozen — full Playwright 63/63 as proof), the Tauri transport +
  `isTauri()` bootstrap selection (IPC modules in one lazy chunk), the
  `apiUrl` origin resolver at every raw site, the
  `TerminalStreamTransport` seam + Tauri pipe. Gate: 324 suites/1353,
  ng 1150, Playwright 63/63 zero skips, debug bundle over a real dist.
  **The human M5 walk COMPLETED 2026-07-18** (the walk record: the
  status log; findings #14 Cmd+R and #15 unthemed-gate-screens fixed
  in place along the way — `8528072d`/`b637e2c9`). Deferred
  loud: native niceties, turnkey `tauri dev`, updater/signing/release
  (D21), uniffi/mobile, Last-Event-ID replay.
- **The P4.6au ∥ P4.6av ∥ P4.7c homepage + Tauri-one-origin round:
  UNIFIED on main (2026-07-16) — ALL THREE CLOSED; dogfood finding
  #12's cause FIXED.** The `systemHome` verb + `GET /api/v1/system/
  home` (v4's `getHomeData` over ported repos/enrichment; the
  base-sensitivity collator; the `home-{main,mount}.db` family; a
  14-case differential vs v4's real service + route at `02865bdb`) ∥
  the Home dashboard at `/` (welcome + the five-action quick row +
  the recent-chats/projects/characters grid; the redirect-to-salon
  root retired; Generate Image OMITTED — `/generate-image` unported;
  card Chat → `/salon/new?characterId=`, a documented divergence) ∥
  the Tauri one-origin adoption (the window ships on
  `qtap://localhost/`; the qtap handler serves the dist and delegates
  `/api/*` into the reused router; `apiUrl()` identity on a
  qtap-origin page; no quilltap-web edits). Wires: the `systemHome`
  CoreRequest fold + name-for-name wire diff; the home beat ACTIVE.
  Gate: 325 suites/1357, ng 1172, full Playwright 65/65 zero skips.
  **The combined human M5 + finding-#12 walk COMPLETED 2026-07-18 —
  finding #12 CLOSED** (the quartet rendered on the Friday copy; walk
  record in the status log). Deferred loud: the
  `/generate-image` screen, NewChatModal-on-card, quick-hide,
  Windows/Linux one-origin re-checks.
- **The P4.6aw ∥ P4.6ax ∥ P4.8 riders + M6-review round: UNIFIED on
  main (2026-07-16) — ALL THREE CLOSED; the small-rider pool is EMPTY
  and the M6 screen-parity checklist EXISTS.** The cost-estimator
  consolidation + the stale "sorts keys" sweep + the
  depiction-guidelines no-vault hint ∥ the `__bold__` rule + the
  form-field source-mode toggle (default ON on every markdown field) +
  the GFM table transformer (19/20 vectors byte-match; the 20th pins
  the pre-existing block-separation dialect gap bidirectionally) ∥
  `docs/developer/porting/m6-screen-parity.md` — every v4 screen/dialog
  verdict-ed, the 16-item `p4.9a–n` backlog, the v4 retirement
  criteria. Gate: 325 suites/1357, ng 1247 (128 files), full Playwright
  green zero skips. Deferred loud: editor table styling (one `_chat.css`
  rule), the block-separation gap, the composer-toolbar slice
  (`p4.9l`). **Next: ~~the human M5+#12 walk~~ (DONE 2026-07-18); the
  M6 backlog items 1–4 (`p4.9a`/`p4.9c`/`p4.9b`/`p4.9d`) as the
  natural next round; `p4.9j` (workspace tabs — v4's DEFAULT shell)
  RULED 2026-07-18: PORT IT, and v4 retirement gates on it (§5.1
  option b; the ruling block in `m6-screen-parity.md` F1) — sequencing
  vs items 1–4 left to the next /setupphase.**
- **The P4.d5 ∥ P4.6ay resumed-lanes unification: UNIFIED on main
  (2026-07-17) — P4.d5 CLOSED; P4.6ay units 1+3 landed (units 2, 4–9
  open — resume at unit 2).** The whole dice/rng + lenient-numbers
  drift re-port (the rng `modifier` end-to-end: tool output
  `modifier`/`total`, the shared-scanner prose detector, both spine
  call sites + persisted TOOL rows; the `llm_number` seam across the
  28-field tool surface; the tool catalog at 58 with `run_custom` +
  the rng `modifier`) ∥ the Pascal custom-tool definition format
  (102-row differential, full Zod strings) + execution core (117
  rows, byte-consumption pinned). The `run_custom` catalogue entry is
  on main and verified INERT until the Pascal handler lands — P4.6ay
  unit 4's byte-identity obligation is UNBLOCKED. Mid-unification the
  v4 checkout went DIRTY (the in-flight custom-tools/metadata
  feature); all nine oracle families were regenerated from a PINNED
  detached v4 worktree at `e3593f75` (recipe in the round record).
  Gate: 332 suites / 1392 tests / 0 failed (nine differentials by
  name, zero SKIPs), clippy both feature sets, release build, ng test
  128 files / 1247, ng build clean, full Playwright 65/65 zero skips.
  Deferred loud: P4.d5 tier-2 item 6's four uncovered quoted-number
  families (coverage, not behavior); the `js_value` → `jsnum` lift.
  Round record: `status-log.md`.
- **The d68638b4 drift-catch-up round: PARTIALLY UNIFIED on main
  (2026-07-17) — P4.d7, P4.6az, P4.6ba CLOSED; P4.6ay resumes at
  unit 4.** The case-insensitive mount namespace (NOCASE indexes via
  the D23 re-dump — which also folded in v4's `characters.metadata`
  generateDDL column, human-ruled Option A, inert in v5 — the boot
  repair pass, case-preserving ops, the 409 name arms) ∥ the
  metadata.json fact-sheet vault surface (fail-soft parser, `{}`
  hydration, the guarded anti-clobber write, whole-object patch,
  scaffold seed; the lazy backfill wired at unification) ∥ the Pascal
  in-chat SPA (wire mirror, Pascal bubble, query-gated composer
  popup, Custom Tools card, the All-Whispers toggle with a LIVE e2e
  beat) ∥ P4.6ay units 11/2/5/6 (the metadata re-port + roster +
  Pascal writer + Prospero error; `run_custom` still verified inert).
  Gate: 339 test binaries / 1400 / 0 failed, the round's 31
  differentials by name zero SKIP over fresh `d68638b4` oracles,
  clippy both feature sets, ng 1286, full Playwright 67/67 zero
  skips. The Workbench SPA is P4.6bb. Round record: `status-log.md`.
- **The P4.6ay resumed-carryout unification: on main (2026-07-17,
  the second d68638b4-round unification) — units 4/8/9/7 + the
  unit-12 compute half landed; `run_custom` is LIVE end-to-end.**
  The LLM tool + handler, catalogue registration + the
  `delegatedDisplay` stamp, the build_tools-resolved roster +
  `customTools` gate + `pascalResult` SSE, the chat custom-tools
  route + the `chatCustomToolsList`/`chatCustomToolRun` verbs, and
  the workbench compute (`list_all_custom_tools` +
  `simulate_outcomes`). The unifier's `seedPascalToolsFixture` wire
  seeded a Tools roster onto Aria's e2e vault and **BA's Salon
  custom-tools flow beat SELF-ACTIVATED** (popup → run → the Pascal
  bubble walks live). Gate: 344 test binaries / 1413 / 0, the lane's
  ten differentials by name zero SKIP over fresh `d68638b4` oracles,
  clippy both feature sets, ng 1286, full Playwright 67/67 zero
  skips. **P4.6ay stays OPEN on exactly ONE item — unit 12's route
  surface (`workbench.rs` + `/api/v1/custom-tools` + the four
  workbench verbs), which is also P4.6bb's server dependency: the
  natural next round is that route surface + the Workbench SPA
  together.** Round record: `status-log.md`.
- **The unit-12 ∥ P4.6bb Workbench round: UNIFIED on main (2026-07-18)
  — P4.6ay CLOSED (at last), P4.6bb CLOSED.** The `/api/v1/custom-tools`
  server surface (the four §W workbench dispatch verbs + REST edge;
  `pascal/workbench.rs`; `AUDIT_RUNS = 10_000`; the
  `{characterId}`-first metadata union; v5's FIRST 422 via the new
  additive `ErrorKind::Unprocessable`) ∥ the whole `/custom-tools`
  Workbench SPA vertical (three-mode shell + deep links, library,
  dual-mode editor with repair + mtime-conflict flow, builder-form
  family, proving bench, destination picker, all four entry points,
  the byte-identical schema asset; the client-safe schema port
  byte-diffed against a committed 115-row corpus; v4's 408-line
  tool-draft suite ported case-for-case). New committed
  `workbench-{main,mount}.db` fixture family; the 2-case + 24-case
  workbench differentials green over fresh `d68638b4` oracles; the
  four Workbench e2e beats SELF-ACTIVATED at unification. Deferred
  loud: `p4.9j` workspace-tab intents, the `finite` message arm, the
  error-envelope `details` array, the `is not valid JSON:` wording
  seam. Next candidates: ~~the human M5+#12 walk~~ (DONE 2026-07-18 —
  finding #12 CLOSED; #14/#15 fixed in place, `8528072d`/`b637e2c9`),
  the M6 backlog items 1–4, ~~the `p4.9j` ruling~~ (RULED 2026-07-18:
  port the tabbed workspace, retirement gates on it), or a
  Workbench/Pascal dogfood pass — see phase-4.md.
- **The M6 items 1–4 round (P4.9a ∥ P4.9c ∥ P4.9b ∥ P4.9d): PARTIALLY
  UNIFIED on main (2026-07-18) — P4.9c/P4.9b/P4.9d CLOSED; P4.9a OPEN,
  held back at unit 1** (branch preserved; resume notes in its order
  header — the photos nav item stays disabled until it lands). Landed:
  About + Profile (the four profile/data-dir verbs, three fresh
  pinned-baseline differentials, the health `version` carry, both
  screens, the `qt-user-menu` shell footer dropdown) ∥ the standalone
  Generate Image surface (shared picker + `/generate-image` +
  the restored home quick action + the in-chat standalone dialog with
  its gutter opener) ∥ the quick-hide system (three-key service on v4's
  exact localStorage keys, filters across salon/home/roster/detail/
  Prospero, the menu section mounted at unification with its beat
  activated, the global tags card in Appearance, ThemePreviewModal —
  re-binned from p4.9c). Gate: 350 suites / 1,433 / 0, three new
  differentials by name zero SKIP, clippy both feature sets, ng test
  1,706 (151 files), full Playwright 78/78 zero skips. **⚠ v4 DRIFTED
  to `616930db` during the round** (llm-consult + Insert-Announcement
  Pascal + outcome comparators — touches the PORTED Pascal/workbench
  surfaces; a drift catch-up round is owed; oracles keep regenerating
  from a pinned `d68638b4` worktree until it runs). Next candidates:
  the `616930db` drift catch-up, finishing P4.9a, `p4.9j` (workspace
  tabs), or M6 items 5+ — see phase-4.md.
- **The P4.d8 ∥ P4.6bc ∥ P4.9a `616930db` drift-catch-up + P4.9a-resume
  round: UNIFIED on main (2026-07-18) — ALL THREE CLOSED; P4.9a closes
  with tier 2 deferred whole.** The llm-consult re-port both sides (the
  `llm` block + contains/ncontains, the async consult seam,
  `pascal::llm_consult` + CUSTOM_TOOL_CONSULT, `pascalMeta.llm`, the
  workbench scripted-oracle params — audit has no live arm by shape —
  the Workbench consulted-oracle SPA surfaces + the byte-copied schema
  asset + the Inspector consult type; the §C corpus 115 → 159) ∥ the
  My Photos tier-1 vertical (user-gallery service, four `photoGallery*`
  verbs + REST edges, committed `photos-{main,mount}.db` + 34-case
  differential, the `/photos` screen + the LIVE nav item, three live
  beats). `979aec66` (Pascal in Insert Announcement) dispositioned
  NO-PORT-NOW (announcer surface unported; BANKED for that slice).
  Gate: 353 suites / 1,444 / 0, the round's 17 differentials by name
  zero SKIP over fresh `616930db` oracles, clippy both feature sets,
  ng 1,844 (154 files), full Playwright 83/83 zero skips. **Standing:
  the consult is DARK in production** (no dispatch-layer
  `CompletionProvider`; the 60 s timeout unwired) — one host-side
  erased-provider thread through `EngineAssembly` closes it; the
  natural first item of the next order. Next candidates: that consult
  wire, P4.9a tier 2 (deep gallery modals), `p4.9j` (workspace tabs —
  retirement gates on it), or M6 items 5+ — see phase-4.md.
- **The consult-wire + image-detail + wardrobe round (P4.6bd ∥ P4.9a2 ∥
  P4.9f1 ∥ P4.9f2): UNIFIED on main (2026-07-19) — ALL FOUR CLOSED, and
  `p4.9a` closes with P4.9a2.** The consult wire (the erased
  `ConsultRunner` seam + `HostConsultRunner` + the 60 s `TimeoutConsult`
  — **the llm consult is LIVE on all three entrances and now costs real
  money**; the P4.d8 timeout deferral closes with it) + the `jsnum`
  canonicalization ∥ the image-detail modal family (`imageInfoGet`, the
  deep modals, prev/next with the nested-Escape suppression, the aurora
  gallery tab) ∥ the wardrobe server surface (chat equip **all seven
  modes incl. v4's deprecated `equip` alias**, outfit read, transfers,
  the global archetype tier; new `wardrobe-routes-{main,mount}.db` +
  74 checks / 66 cases) ∥ the wardrobe SPA (the control dialog in both
  modes, the tier-routed item editor, three entry points, the stub
  retired). Gate: 354 binaries / 1,450 / 0, the round's 7 differentials
  by name zero SKIP, clippy both feature sets, ng test 171 files /
  2,004, full Playwright 86/86 zero skips. **⚠ One user-visible gap:
  `wardrobePreviewAvatar` is half-live** — its render step is
  refusal-armed pending the `avatar_preview` host wire, which is blocked
  on the already-deferred production WebP codec seam; that wire is the
  natural first item of the next order. (⚠ v4 had DRIFTED to `b8b12695`
  — LaTeX/KaTeX — and this round deliberately did NOT absorb it;
  **that catch-up ran as P4.d9 and is now CLOSED — see the next
  bullet.**)
- **The P4.d9 `b8b12695` KaTeX/markdown drift catch-up round: UNIFIED on
  main (2026-07-19) — P4.d9 CLOSED; the oracle baseline MOVES to
  `b8b12695`.** Single SPA-only lane (zero Rust source touched): the
  shared math normalizer (`normalizeMathDelimiters` + `MATH_SKIP_PATTERN`
  + `REMARK_MATH_OPTIONS`, single-dollar math deliberately OFF so
  currency prose survives) + v4's `katexDepth` KaTeX-subtree skip in
  `applyRoleplayPatterns`; `remark-math` + `rehype-katex` wired into the
  ONE Salon renderer at v4's exact plugin positions (v5 needed one
  pipeline where v4 needs two); the KaTeX stylesheet + `.katex-display`
  overflow rule; `markdown-fixtures.json` regenerated from v4's REAL
  renderer at `b8b12695` (23 → 34 fixtures, byte-parity); a live e2e math
  beat. **The baseline-move neutrality proof:** all SEVEN oracle families
  that transitively import v4's renderer (salon-reads/-mutations/-skip/
  -swipe-generate, text-replacements-routes, cost-background-routes,
  courier-images-routes) regenerated at `b8b12695` and re-run BY NAME,
  all green with committed oracles behavior-unchanged — v4's
  `renderedHtml` never reaches the diffed payloads. Gate: 354 binaries /
  1,450 / 0, the seven differentials by name zero SKIP, clippy both
  feature sets, release build, ng test 172 files / 2,029, full Playwright
  87/87 zero skips. Unification wire: the new math beat was moved off
  "Solo Voyage" onto "Group Expedition" — its sends shifted the P4.6ap
  chat-totals baseline (15.4K → 15.5K); no spec asserts totals or counts
  on the group chat. Deferred loud (unchanged by this round): help
  `math-notation.md` (no v5 help surface — banked for `p4.9i2`),
  FilePreviewText math (the P4.6af rich-stack deferral), and the composer
  backslash-escape seam (`\(…\)` typed into qt-rich-editor serializes to
  `\\(…\\)`; the `\(…\)` → `$$` normalization is proven by the captured-v4
  fixtures instead). Next candidates: the `avatar_preview` wire + the WebP
  codec (the named next Rust item), `p4.9j` (workspace tabs — retirement
  gates on it, wants a DEDICATED round), `p4.9i1`/`p4.9i2`, or M6 rows 5+
  — see phase-4.md.
- **The P4.9J1 ∥ P4.9J2 workspace-tabs round: UNIFIED on main (2026-07-19)
  — BOTH CLOSED; `p4.9j` LANDED (v4's DEFAULT shell; the F1 v4-retirement
  gate) and ON by default.** The pure workspace core (reducer / persistence
  / tab-meta / route-to-intent, captured-corpus tier-1 differential against
  v4's real `lib/workspace` — 144 replay assertions, corpus regen owned by
  the workspace lane) + the signal store + the two-pane keep-alive host and
  all chrome + the flag (default ON) + 16 redirect guards + shell cutover +
  the e2e dual-mode harness ∥ every input-driven screen made hostable
  (dual-mode inputs, self-close, the three in-tab drills, the SalonModePanes
  child-tab source with DOM-move portals, backdrop reporting, opener
  intents). Unification wired the five AT-UNIFY kinds, the reverse
  child-tab close (portal-registry disappearance), and grew the workspace
  walk to six beats. Gate: 354 binaries / 1,450 / 0 (zero Rust changed),
  corpus byte-identical from the pinned `b8b12695` worktree, ng 187 files /
  2,258, full Playwright green zero skips (one pre-existing composer beat
  gained a pause-before-send gesture fix (the group turn chain's terminal state is run-order-dependent and can disable the composer)). Still not-wired (loud): the wardrobe
  `asTab` surface, `document-standalone`, `brahma` (p4.9i1). v4 drifted
  to `c53510c7` then `7e6d13e5` during/after the round; **the catch-up
  round ran and is UNIFIED — see the next bullet.**
- **The `7e6d13e5` state-cascade drift catch-up round (P4.d10 ∥ P4.6be ∥
  P4.d11): UNIFIED on main (2026-07-20) — ALL THREE CLOSED; the oracle
  baseline MOVES to `7e6d13e5` (4.8.0-dev.92) and the drift debt is
  CLEARED.** The four-tier state cascade server-side (the pure
  `state::{paths,cascade}` modules + the general-state mount document
  with its host-boot seed, the four-tier state tool + v4's new definition
  bytes, the nine §A chat/group/general state dispatch verbs with the
  enriched cascade get-state, Pascal `$state` end-to-end incl. the
  workbench mock-state `state` param, the universal math-notation
  system-prompt note, and the release-sweep verification — the
  `93604767`/`28e89f51` "no functional change" claims proven by a
  53-family regen-and-re-run sweep, the D23 re-dump ZERO-diff, the
  Anthropic model-family boundary pinned into the request-envelope
  corpus 31 → 34) ∥ the state-cascade SPA (the four-entity State Editor
  modal, Group State on the group editor, the General State card, the
  workbench mock-state card + read-only `$state` pills, the tool-draft
  `state` kind, the re-copied `qtap-custom-tool.schema.json`) ∥ the
  release-sweep SPA slice (single-dollar math promotion + the markdown
  parity fixtures regenerated 34 → 40, katex 0.18.1, the workbench
  dialog backdrops on `qt-dialog-overlay`). Wires: the §C corpus counts
  (175 = 10 title + 165 definition, 58 accept / 107 reject) + both
  consumers green; the §A/§B name-for-name contract diff clean; a
  two-trap locator gesture fix as the state beats first ran live. Gate:
  357 binaries / 1,454 / 0; the round's 24 differentials by name over
  fresh `7e6d13e5` oracles zero SKIPs; clippy both feature sets; release
  build; ng 190 files / 2,342; full Playwright 96/96 zero skips, the
  three ACTIVATE-AT-UNIFY state beats LIVE. Deferred loud: the chat-tier
  State-Editor opener (rides the ChatSidebar follow-up), Pascal
  `persist` (deferred in v4 itself), the `8ee56f6e` corpus-seed bank,
  help/`math-notation.md` (p4.9i2). Next candidates: the
  `avatar_preview` wire + WebP codec, the two not-wired workspace kinds,
  a workspace/state dogfood pass, or M6 rows 5+ — see phase-4.md.
- **The workspace-tabs remainder round (P4.9I1A ∥ P4.9I1B ∥ P4.9J3 ∥
  P4.9J4): UNIFIED on main (2026-07-20) — ALL FOUR CLOSED; the three
  not-wired workspace tab kinds are GONE (all 22 kinds host real
  screens; the NotWiredPane scaffold retired).** The Brahma Console
  end-to-end (`p4.9i1` CLOSED): the 619-line multi-turn orchestrator
  (independent of the ported one-shot engine; 25-turn loop, both stuck
  guards, text-block downgrade, frames on the Event channel per the
  `ChatSend` split), the eight-verb `brahma-console` dispatch family +
  REST edges, the committed `brahma-{main,mount}.db` family, two
  differentials (tier-2 routes 14/14; tier-3 mocked-LLM orchestrator
  5 arms, frames + rows) ∥ the whole console SPA (dialog both modes,
  shared-reducer streaming, rail entry) — **the send rides the new
  `BrahmaConsoleSendDriver` host seam and is LIVE (real spend)** ∥ the
  `asTab` WardrobeView + the p4.9j riders (openChatOnMount via
  `/salon/new` — documented divergence; Create-Character in-tab;
  `mode=setup` guard bypass; the HTML5 drag-split beat; the accent
  ruling CORRECTED to no-change — theme packs already carry v4's live
  tokens) ∥ the standalone Document Mode surface over the existing
  P4.6w verbs (P4.9J2 tier-2 item 7 CLOSED with it). The first live
  run of the 8 ACTIVATE-AT-UNIFY beats caught a REAL port bug —
  `write_database_document` returned a second clock reading ≠ the
  stored `lastModified` (spurious 409 on open→edit→write; fixed, core
  0.0.292, regression-tested). Gate: 359 suites / 1,459 / 0 with the
  two new differentials by name over fresh `7e6d13e5` oracles zero
  SKIP; clippy both feature sets; release build; ng 201 files / 2,439;
  full Playwright 107/107 zero skips (every ACTIVATE-AT-UNIFY beat LIVE). Deferred loud: the
  **general-scope document fs wire** (the picker's top "New blank
  document" refuses on every host — the standing FsSeam deferral now
  has a user-visible affordance, pinned by a beat), the brahma async
  context-summary/auto-title drive, HelpChat (p4.9i2),
  `wardrobePreviewAvatar` (WebP codec), per-instance storage keys.
  Next candidates: the `avatar_preview` wire + WebP codec, the
  general-scope fs wire, a workspace/state/brahma dogfood pass, p4.9h
  (ChatSidebar), M6 rows 5+ — see phase-4.md.
- **The codec + fs seam round (P4.6bf ∥ P4.6bg): PARTIALLY UNIFIED on
  main (2026-07-21) — P4.6bf CLOSED; P4.6bg OPEN at unit 1-of-6 (resume
  at unit 3; its order header carries the resume list).** The
  `HostAvatarPreviewRenderer` over the EXISTING P4.1b `HostImageCodec` —
  **`avatar_preview` is LIVE; the wardrobe out-of-chat Preview button
  costs real money** (the e2e beat pins the pre-provider no-API-key arm
  at zero spend; the live render walk is a dogfood item) — plus the
  blob-transcode `WebpTranscoder` impl and the `EngineAssembly.blob_webp`
  seam (deliberately dead: the engine call-site wire is INHERITED by
  P4.6bg unit 6's handler re-signature; the scriptorium WebP beat stays
  probe-skipped until then) ∥ the doc-edit path-resolver host-filesystem
  branches (general / fs mounts / legacy project fallback; `safe_realpath`
  walk-up + boundary check byte-exact) behind a `files_dir` thread every
  call site still passes `None` to — production behavior unchanged until
  BG's units 3–5 open the tool-site fs I/O and flip the engine wire. The
  ST placeholder-DEFLATE seam DEFERRED with the empirical finding (parity
  only via flate2's zlib C backend — recipe banked). Gate: 359 test
  binaries / 1,470 / 0; the two round differentials by name over fresh
  `7e6d13e5` oracles zero SKIP (DPR 25+6 fs-extended; wardrobe 74
  checks); clippy both feature sets; release build; ng 201/2,439; full
  Playwright green (one by-design probe skip). BG's record also flags a
  pre-existing P4.d7 dup-name divergence (follow-up spawned).
- **The P4.6bg remainder: UNIFIED on main (2026-07-21) — P4.6bg CLOSED
  (tier 1 complete) with ONE loud tier-2 deferral (the conversion port);
  the codec + fs seam round is fully disposed and P4.6bf's inherited
  blob-WebP wire is RESOLVED.** The doc-edit tool surface does real
  host-disk I/O on filesystem-backed paths (fs/obsidian mounts, the
  `general` scope, the legacy project fallback); the engine threads
  `files_dir` through all 11 doc-verb arms; the Document-Mode operator
  surface works on fs paths; the standalone "New blank document"
  general-scope round-trip is LIVE (the FsSeam refusal GONE — one
  deliberate v5 divergence: `_general` pre-created over v4's latent
  fresh-instance quirk); **mount blob uploads now transcode to WebP at
  the dispatch layer** (the scriptorium beat self-activated). New
  `doc_fs_equivalence` family (21 fs ops + byte-exact fs-tree diff).
  Gate: 360 binaries / 1,471 / 0, five differentials by name over fresh
  oracles zero SKIP, clippy both feature sets, release build, ng
  201/2,439, full Playwright green zero skips. **⚠ v4 DRIFTED to
  `e2eb3d21` (4.8.0-dev.93) during the lane — ZERO lib/ code (New-Chat
  picker components + help doc + versions); the oracle baseline STAYS
  `7e6d13e5`; a SPA re-port of the picker behavior is OWED** (+ watch
  the untracked episodic-recall-overhaul feature doc). Next candidates:
  the New-Chat picker drift re-port, the conversion port, a
  wardrobe-Preview/workspace/state/brahma/fs-documents dogfood pass,
  p4.9i2, p4.9h, M6 rows 5+ — see phase-4.md.
- **The episodic-recall drift catch-up, ROUND 1 of 3 (P4.d12 ∥ P4.6bh ∥
  P4.6bi): UNIFIED on main (2026-07-21).** v4's largest single drift
  (`8bf3cb5f`, a squash-merge of episodic-recall + character-outfit +
  wardrobe-permission). Round 1 landed the episodic **spine** (data +
  pure logic) + both orthogonal character slices: the D23 re-dump
  (`chats.timelineMode` + `memories.{occurredAt,narrativeTime,entities,
  kind}` + `idx_memories_occurredAt`) through the data layer; the pure
  `episodic` module (4 exports, 94-case tier-1); memory-weighting
  `episodicBonus` + the event-clock age; the injector's dated dynamic
  head; the memory-row/pure oracle **rebase** onto `8bf3cb5f`; the
  `canChooseOutfit` vault flag + the `canDressThemselves`/
  `canCreateOutfits` PUT toggles (server); and the Wardrobe-tab card +
  outfit-selector seed + the New-Chat picker re-port (`e2eb3d21`: full
  roster, cast-only Play-As, keep-on-revert) (SPA). **The episodic
  BEHAVIOR is rounds 2/3 — the columns are inert until then.** Round-3
  carry-ins flagged by the lane: the gate tier-3 family stays
  un-regenerated (v4's first-write `applyEpisodicFallbackAnchors` is
  non-inert on AUTO-source proper-noun content — the inert-path
  boundary); the turn-path write `occurredAt` stamp defers with the
  processor extraction prompt. Gate: 361 binaries / 1,474 / 0 (key
  differentials fresh from `8bf3cb5f`, by name), clippy both, release
  build, ng 203/2,448, Playwright 109 + 1 documented flake. Round record
  + lane records in `status-log.md`; the campaign roadmap +
  round-2/3 scope in `phase-4.md`. **Next: ROUND 2** (time/entity-aware
  retrieval + deep-dive tools + the replay harness), then ROUND 3
  (creation-side + cadence + stop-destroying + "Story's Clock").
- **The episodic-recall drift catch-up, ROUND 2 of 3 (P4.d13, single
  lane): UNIFIED on main (2026-07-21) — P4.d13 CLOSED; the episodic
  columns start EARNING.** One deliberate single-lane round (workstreams
  B + D + the §3 replay harness all consume one search surface).
  Retrieval is time/entity-aware end-to-end: the distill episodic
  signals (retrospective/timeRange/entities + the TODAY clock line — the
  memory-tasks family SPLIT, new `QT_ORACLE_DISTILL`), recall-tags
  turn-aware (past 1.15 flip / window ×1.3 / re-ask suspension),
  `search_memories_semantic` occurred-within two-stage + entity-anchor
  union + retro multi-probe (the long-standing recallContext/expansion
  deferral CLOSED), vault-summary date staging (new
  `QT_ORACLE_VAULT_CONV`; no production caller until round 3's
  mini-recap), buildContext part-1 threading + `RETRO_HEAD_*` (part 2 =
  round 3). Deep-dive tools: search `since`/`until`/`aboutCharacter` +
  episodic result fields + span filter, `read_conversation` interchange
  slicing, the stale `memorySearch` catalog entry DELETED (57 tools),
  the anti-confabulation prose in both prompt builders. The §3 replay
  harness is LIVE end-to-end: `services/recall_replay.rs` + the
  `chatRecallReplay` verb on the new `RecallReplayDriver` host seam
  (**one real cheap-LLM call per replay**) + the `quilltap
  recall-replay` CLI (HTTP-only like v4's; Tier-R differential), over
  the NEW committed `episodic-recall-{main,mount}.db` fixture (new
  tier-3 `QT_ORACLE_RECALL_REPLAY`, 13 cases tabling both ranking
  paths). Riders: the chat-PUT `timelineMode` accept arm; two
  pre-existing port bugs fixed (search-path `lastAccessedAt` bump
  scope; the recall-history persist shape — a LIVE write path). Gate:
  364 binaries / 1,496 / 0 with all 12 round families regenerated fresh
  at `8bf3cb5f` (zero SKIP by name), clippy both feature sets, release
  build, ng 203/2,448 (SPA untouched), full Playwright 110/110 zero
  skips. **Next: ROUND 3 — the campaign's final round** (creation-side
  extraction + cadence part 2 + stop-destroying-episodes + the Story's
  Clock SPA; the full carry-in list is in phase-4.md and the round
  record).
- **The episodic-recall drift catch-up, ROUND 3 of 3 (P4.d14 ∥ P4.d15 ∥
  P4.9H1): UNIFIED on main (2026-07-22) — ALL THREE CLOSED; THE
  CAMPAIGN CLOSES.** Creation-side: the clocked extraction prompts +
  EVENT category + `kind`/`when`/`entities` coercion + `capCandidates`,
  the processor `resolveCandidateAnchors` + turn-path `occurredAt`
  stamp, the first-write fallback anchors, the gate date guard +
  reinforce anchor upgrades (**`QT_ORACLE_GATE` un-SKIPPED and green**),
  the NEW fold-episode pass + fold Timeline, the housekeeping merge
  guard ∥ recall-on-reference part 2: the scoped dated mini-recap (the
  vault `time_range`'s first caller), the `retrospective-recall`
  whisper + sweep membership, the retro-signature spam guard ∥ the
  ChatSidebar SPA vertical (participants / Chat / Visibility / Organize
  sections, the four-affordance reconciliation, **the Story's Clock**
  switch, the per-chat Core-whisper override + chat-tier State-Editor
  opener — the state-cascade deferral CLOSED). Gate: 365 binaries /
  1,505 / 0, all 27 round families fresh at `8bf3cb5f` + by-name zero
  SKIP, clippy both feature sets, release build, ng 209/2,487, full
  Playwright green zero skips. **Standing (loud):** the
  `orchestrator_tier3` family is stale-RED from a PRE-EXISTING gap (v5
  omits v4's memory-recap block upstream of build_context — dedicated
  follow-up owed); **the ported memory-extraction pipeline is DORMANT
  in production** (no `CONTEXT_SUMMARY`/`MEMORY_EXTRACTION` job
  handlers in `quilltap-host`) — wiring them is the top next candidate;
  the v4 `deab0e5d` theme/icons drift (lib-free) owes a small SPA
  re-port; `p4.9h2` + the sidebar tier-3 deferrals stay banked. Next
  candidates: see phase-4.md's campaign section. **(Both standing
  items CLOSED by P4.6bj — next bullet.)**
- **P4.6bj memory-pipeline job handlers: CLOSED on main (2026-07-22,
  single lane) — THE EXTRACTION/FOLD PIPELINE IS LIVE.** Unit 0 closed
  the `orchestrator_tier3` stale-RED (the P4.d15 recap diagnosis was
  already healed by round 3; the residual was the in-loop fold-episode
  seam — `run_summary_check` now folds with the new
  `FoldEpisodePassSeams`, episode pass live, the other four arms still
  the oracle-mocked no-ops). Then `buildTurnTranscript` (new tier-1
  family, 17 cases) + the `handleMemoryExtraction` /
  `handleContextSummary` handler bodies (the CS job path runs
  `RealContextSummarySeams` — Librarian re-post / vault mirror /
  refresh / cost events / episode pass all live — + the −2 danger
  chain; new tier-3 `memory_pipeline_jobs` family: 10 cases, SIX
  diffed tables incl. `background_jobs`, thrown-error strings pinned)
  + BOTH handlers registered in `ProductionSpineFactory` (the host
  read closing carina's `memory_extraction_limits` deferral too) —
  **three rounds of episodic work now RUN in production and cost real
  cheap-LLM money on every closed turn.** Unification gate: 367
  binaries / 1,508 tests / 0 failed; SEVEN differentials by name (the
  four round families + the processor / carina / background-jobs
  transitives) over oracles regenerated fresh at v4 HEAD `e646f58b`
  (lib-identical to `8bf3cb5f` for these families — verified by
  import), zero SKIP; clippy both feature sets; release build; ng test
  209 files / 2,487; full Playwright 111 passed + the documented
  wardrobe `set_all` full-suite flake re-proven green in isolation
  (3/3, :252 at 499ms), zero skips, fresh dist + rebuilt debug bins.
  Live proof owed: the next dogfood pass (the e2e instance has no API
  keys by design). Order:
  `work-orders/p4.6bj-memory-pipeline-job-handlers.md`; records in the
  status log. Versions: core 0.0.325, harness 0.0.281, host 0.0.30
  (core/harness accumulate over the parallel dogfood-finding fixes
  `0.0.322`/`0.0.323` on main).
- **The `e646f58b` v4-drift catch-up round (P4.d16 ∥ P4.d17): UNIFIED
  on main (2026-07-22) — BOTH CLOSED; the drift debt is CLEARED.** The
  workspace deep-links re-port (`8d86847a`: the `salon-list` tab kind,
  drill-in payloads, `character-view` in the `?open=` layer, the
  terminal-popout salon+child funnel, six new redirect guards, the
  `/salon/new` funnel as the v5-only `salon-new` tab — the no-modal
  divergence, recorded in `m6-screen-parity.md` F1 — and the workspace
  corpus regenerated at `e646f58b`) ∥ the thinking-indicator + theme
  re-port (`deab0e5d`/`ab0f175e`: v5 had never ported QuillAnimation —
  the `thinking` icon, the `.qt-thinking-indicator` motion hook,
  `qt-quill-animation` at all four call-site analogs in
  `streaming-message.ts`, Madman's Box 1.1.5 → 1.1.7 with the icons-map
  entry expressed as the unlayered `[data-icon]` CSS override). Zero
  Rust changed. Gate: fmt/clippy both feature sets clean, 367 binaries
  / 1,508 / 0; ng test 211 files / 2,547; ng build clean; full
  Playwright 117/117 zero skips (D16's five deep-link beats + D17's
  indicator/theme beats LIVE). Banked loud: the two help docs
  (`p4.9i2`). Versions: SPA 0.5.263; crates unchanged. Next
  candidates: the episodic/sidebar/memory-pipeline dogfood pass,
  `p4.9h2`, the sidebar tier-3 deferrals, `p4.9i2`, M6 rows 5+ — see
  phase-4.md.
- **P4.10 — the dev-grade packaging close-out: ORDER WRITTEN, not started
  (2026-07-22).** The three run modes are all decided and built (D1 desktop
  + server, D12 CLI), but Phase-4 deliverable 6 ("Packaging (dev-grade)")
  never got finished: the **Dockerfile predates the SPA** — it copies no
  `assets/` (so the P4.4u4 seed `include_bytes!` fail to compile), builds no
  `ng build` dist (`.dockerignore` excludes `apps`), and passes no
  `--spa-dir`, the only way to reach one (no env / binary-relative
  fallback), so the image serves the placeholder pages. Every piece works
  independently — Playwright runs the real `quilltap-web` over a real dist —
  so this is assembly, not porting. Order:
  `work-orders/p4.10-dockerfile-spa-packaging.md` (single lane; unit 4 is the
  only Rust touch). **Not** a D21 release item: nothing is published, signed,
  or tagged. It is a strong next-round candidate — until it lands, no one can
  run v5's server mode without building it from source by hand.
- **P4.11 — the non-streaming request builders: CLOSED, UNIFIED on main
  (2026-07-23, single lane) — dogfood finding #23 FIXED; the cheap-LLM
  family is LIVE on real data.** Every request builder honours
  `RequestInput.stream`, reproducing v4's `sendMessage` body byte-for-byte
  per provider: DeepSeek/Z.AI/Ollama/OpenAI/Grok flip the flag (dropping
  `stream_options`), Anthropic + OpenAI-compatible OMIT the `stream` key,
  Google switches only its URL, and OpenRouter builds a wholly different
  body (`@openrouter/sdk` zod re-emission — key reorder, snake_case,
  undeclared-key drops, and the new `BuildError::ProviderRefused` where the
  SDK refuses client-side so v4 sends nothing). The blind spot that let a
  total outage survive a differential-verified port is closed: the
  request-envelope corpus records BOTH modes for all EIGHT providers (34 →
  93 lines + google-wire 5 → 10, coverage-asserted, the 34 pre-existing
  streaming vectors byte-identical), plus a call-site regression test on
  the bytes `execute_completion` hands the transport. Three pre-existing
  divergences the widened corpus exposed are fixed (OPENAI_COMPATIBLE had
  NO coverage + its `stop` key-order bug; OpenRouter's missing
  `route:'fallback'`). The unit-9 live quartet on the Friday copy — 24
  MEMORY_EXTRACTION + 1 TITLE_GENERATION `llm_logs` rows, jobs COMPLETED,
  fresh AUTO memories with `occurredAt` — **is P4.6bj's and P4.d12–d15's
  owed live proof: extraction/fold and three episodic rounds now run in
  production.** Unit 8 recorded (no code change): v4 does NOT log failed
  cheap calls and v5 matches; an error-row divergence awaits a human
  ruling. Deferred loud: OpenRouter's streaming no-tools `callModel()`
  path (unported), the extraction cadence unpinned by any differential, no
  console logging anywhere (a standing open question). (The prior "no v5
  writer for `chat_messages.debugMemoryLogs`" line was STALE — corrected by
  P4.15: both extraction handlers write it, `memory_extraction_job.rs:338` /
  `carina_memory_extraction.rs:257`.) (Dogfood #22, the sibling finding,
  was already FIXED on main `2aa3d01b`.)
- **The provider-I/O rewrite round (P4.13 ∥ P4.14 ∥ P4.10): UNIFIED on
  main (2026-07-23) — ALL THREE CLOSED** (P4.13's last item, unit 9's 💸
  human live proof, completed at the 2026-07-24 dogfood walk — findings
  #25 and #22 CLOSED there). The ruled
  one-off divergence executed: the `StreamMessage` carrying type
  end-to-end — **tool-call linkage reaches the wire on all eight
  providers (dogfood #25 FIXED, closes at the walk)** — with FIVE
  flattening sites fixed (the order's three + the text-tool loop's
  continuation + the Carina query loop), the always-on
  `tool_wire_call_site` byte pin and 29-case `response_parse_equivalence`
  recorded-body corpus (its first run caught two MORE #24-class
  production bugs: OpenRouter usage parsed to ZEROS; Google raw's
  getter-only `functionCalls`), the phase-B restructure (`RequestMessage`
  deleted, `ProviderKind` the one dispatch point, id-less-tool arms
  unrepresentable), the ruled failed-cheap-call `llm_logs` error row (a
  deliberate divergence), the P4.14 non-validating stable merge sort
  (arm (a) ruled; both injector comparators + the audit-found Post
  Office `sort_newest_first` — the live turn-killing panic is gone,
  #26's re-check unblocked panic-side), and the P4.10 packaging
  close-out (the Docker image builds/serves the real SPA + ships the
  CLI; dist resolution chain; `docs/developer/running.md`; container
  walk human-verified). Gate: 369 binaries / 1,538 / 0 with all env
  vars; the round's 20 named families `--nocapture` zero SKIP over
  fresh `e646f58b` oracles; three corpora byte-fresh; clippy both
  feature sets; release build; ng 2,547; full Playwright green zero
  skips. The pre-existing `enclave_step_tier3` red P4.13 found (the
  enclave step never ran fold-episode) was FIXED on a parallel branch
  and folded in at this same unification (`enclave/step.rs` →
  `FoldEpisodePassSeams`; differential green over a fresh TZ=UTC
  oracle; core → 0.0.337). The courier paste-resolver's twin
  bare-NoopSeams fold gap was FIXED and unified right after the round
  (2026-07-24, single follow-up lane): `courier_transport`'s
  `run_summary_check` runs `FoldEpisodePassSeams`, the embedding
  provider threaded through `resolve_external_turn` → the spine's
  `CourierResolveDriver`, and the courier differential grew the
  at-cadence `resolve_cadence` case (14 cases, fresh `e646f58b`
  oracle; the extended fixture family committed incl. a new empty
  llm-logs partition) — all three bare-summary-check call sites are
  now closed. Versions after it: core 0.0.338, harness 0.0.287, host
  0.0.31. Standing loud: the all-synthetic response-bodies corpus,
  #26/#27/#28 + tracing-subscriber (`debugMemoryLogs` is NOT a gap — v5
  writes it; the P4.11 note was stale, corrected by P4.15). The ruled
  sequence's remaining legs (the dogfood-fixing run, then the fresh
  walk) BOTH RAN — see the next two bullets.
- **The post-rewrite dogfood-fixing round (P4.15 ∥ P4.16 ∥ P4.17 ∥
  P4.18): UNIFIED on main (2026-07-24) — ALL FOUR CLOSED.** Finding #27
  FIXED (both summary-check sites thread the real `cheapLLMSettings` +
  all user profiles + danger; selected-profile differential cases close
  the single-profile blind spot; absent-key default `PROVIDER_CHEAPEST`
  — the enclave's `"AUTO"` is a dead phantom) ∥ finding #28
  dispositioned NOT-A-BUG (classifier) — v4's real classifier benched
  over both windows (20 💸 calls); the misses are the cheap MODEL +
  temp-0.3 noise; banked: the unported proactive pre-compute path
  (fidelity item), the downstream whisper-suppression look ∥ the
  ToolMessage rendering port (collapsible tool card both layouts +
  grouping + `whispered to <names>`; the raw-JSON whisper gone; live
  e2e beat) ∥ the RULED arm-(a) tracing surface (subscriber in all
  three bins, events at the swallow sites, `TraceLayer`; log output
  explicitly outside the differential contract). The #22 `loadedMemories`
  rider LANDED (`self_inventory` reports the real slate). Gate: 369
  binaries / 1,550 / 0 (the four affected families fresh at `e646f58b`
  by name zero SKIP), clippy both feature sets, release build, ng 213
  files / 2,583, full Playwright 119/119 zero skips. Deferred loud:
  browserUserAgent, the sibling-owned `eprintln!` sweep, file-transport
  log parity.
- **The 2026-07-24 post-rewrite dogfood walk (the ruled sequence's third
  leg): COMPLETE, walked CLEAN — zero new findings.** On the Friday
  copy: Part A tool use across OpenAI/Anthropic/DeepSeek (**#25 + #22
  CLOSED — P4.13 unit 9 complete, the provider-I/O round closes**; the
  P4.17 card live; #29/#30 surfaced and dispositioned NOT-A-BUG,
  v4-faithful, queued as post-5.0 v4-first product items), Part B the
  context-summary fold + cheap-LLM config (**#26 + #27 CLOSED** — three
  fold cycles on the configured cheap profile; 66/66 AUTO memories carry
  `occurredAt`), the 💸 llm-consult live, Part E the recall-replay CLI
  (the P4.d13 live proof), Part F outfit + heavy-character items. NOT
  walked (the next pass starts here): Part D retrospective-recall live
  behavior (the #28 downstream look), Part F items 15/16 (Story's Clock
  jump; per-chat Core-whisper override), items 10/11 (blocked by #30).
  Record: `dogfood-findings.md`.
- **The pre-compute + Data & System round (P4.19 ∥ P4.9G1 ∥ P4.9G2):
  UNIFIED on main (2026-07-24) — P4.19 and P4.9G2 CLOSED; P4.9G1 PARTIAL
  (resume there).** The chat spine now runs v4's proactive pre-compute
  distill before buildContext (`services/pre_compute.rs`; the pre-searched
  head suppresses the fallback distill), pinned by a new tier-3 `precompute`
  differential (8 cases) + two new `build_context_tier3` ops ∥ the Data &
  System **tasks-queue + jobs server family** (`api/system_data.rs`, the host
  `JobPumpControl` seam — Stop genuinely halts claiming, v4-parity REST
  edges, the committed `system-data-*` fixture, an 18-case differential),
  with all sixteen §1 verbs DEFINED and unlanded ones refusing loudly ∥ the
  **whole Data & System SPA tab** (nine cards in v4's order, both backup
  dialogs, both 5-step import/export wizards, the delete-all dialog, the LLM
  log viewer + character-edit F2 section, and the app-wide **auto-lock idle
  provider** — the enforcement half v5 never had). The §1 name-for-name wire
  diff caught a REAL drift before it shipped: the three job verbs carried
  `id` server-side vs `jobId` client-side, so every per-job Tasks Queue
  action would have failed to deserialize live — reconciled toward `jobId`.
  P4.19's `orchestrator_tier3` "BLOCKED" finding was CORRECTED at
  unification: it does not reproduce from main (oracle regenerates, 227 rows,
  differential green) — unit 4c is CLOSED and no v4-jest infra fix is owed.
  Gate: 372 binaries / 1,560 / 0, four families regenerated fresh at
  `e646f58b` and re-run by name zero SKIP, clippy both feature sets, release
  build, ng 223 files / 2,621, full Playwright 124 passed / 1 gated skip / 0
  failed. **⚠ Three Data & System cards are BUILT but their server families
  are OPEN** (Backup & Restore, Import / Export, Delete All Data — they answer
  the loud not-yet-available refusal), which is the top reason to run
  P4.9G1's remainder next; its order's status header enumerates the resume
  list, and the delete-all e2e beat is written and gated behind the named
  `DELETE_ALL_SERVER_LANDED` constant. Next candidates: finish P4.9G1, a
  dogfood pass over this round's live surfaces (+ the still-owed walk Part D
  and Part F items 15/16), or M6 rows 6+ — see phase-4.md. Versions: core
  0.0.346, harness 0.0.293, host 0.0.33, web 0.0.40, cli 0.0.3,
  quilltap-tauri 0.0.5, SPA 0.5.268.
- **The "finish P4.9G1" round (P4.9G3 ∥ P4.9G4 ∥ P4.9G5): UNIFIED on main
  (2026-07-24) — P4.9G3 CLOSED; P4.9G4 and P4.9G5 PARTIAL.** Two of the four
  built-but-refusing Data & System cards went LIVE: **Delete All Data**
  (`services/delete_all.rs` ported table-for-table, the `DELETE_ALL_MY_DATA`
  sentinel re-check, `system_delete_data_equivalence` 7 cases diffing a
  row-count map of EVERY table in all three partitions) and **Create Backup**
  (38-collection collect → manifest → staging tree → zip, the `BackupHost`
  single-use 30-min temp store, the byte download leg). **Export is LIVE**
  (all ten types, byte-exact NDJSON, 42 cases) and **Import is live through
  the PREVIEW** (19 cases). Riders: the `/api/v1/system/jobs` collection edge
  — closing P4.9G1's blind spot where `jobs_list`/`jobs_enqueue` had no edge
  and no oracle (8 cases, green first run) — and the change-passphrase REST
  alias. **The unification wire caught a REAL production bug no lane could
  see:** `collect.rs` applied v4's missing-table tolerance only to its
  `query_all` reads, not the ~7 direct `db::` finder reads, so **Create Backup
  returned a bare 500 on any instance that had never touched provider models
  (or tags, or connection profiles)** — v4 lazily creates the collection and
  `safeQuery`s to `[]`. Fixed (`if_table`/`if_table_opt`) plus a
  `tracing::error!` at the swallow site; the differential was blind because
  the fixture had just been widened to carry every table, while the e2e
  instance genuinely lacks them. Gate: 377 binaries / 1,591 / 0 with all SIX
  system families regenerated fresh at `e646f58b` **against the widened
  `system-data-*` fixture** and re-run by name zero SKIP, clippy both feature
  sets, release build, ng 223 files / 2,621, full Playwright **126 passed / 0
  failed / 0 skips** (the two per-lane reds were the documented run-order
  flakes and did not reproduce). `api/types.rs` stayed FROZEN all round — the
  §1 diff vs `core-contract.ts` is clean. **STILL OPEN, both named in their
  order headers:** P4.9G5 units 3–5 (the WHOLE restore side — §2 is unblocked,
  `delete_user_data` is on main at the pinned signature) and P4.9G4's import
  EXECUTE; both refuse loudly by name, and `SystemRestorePreview` /
  `SystemRestoreExecute` are the only two variants left in `engine.rs`'s
  not-yet-available arm. Deferred loud: legacy `<base>/files/**` disk bytes
  survive the wipe (needs a `StorageBackend` at the dispatch layer); v4's four
  sibling unlock actions get no REST alias. **A v4 BUG — RULED 2026-07-24,
  v5 DIVERGES (the port's one deliberate reader divergence):**
  `assembleExportFromStream`'s `every()` over a SPARSE array means v4 cannot
  round-trip a document-store blob larger than the 3 MB chunk size; v5's
  reader now waits for every chunk (reader-only — the writer still emits v4's
  exact bytes, and 0-/1-chunk blobs are unchanged), and the short stream
  reaches v4's OWN truncation error, which its sparse `every` had made
  unreachable. Asserted both directions via `EXPECTED_DIVERGENCES` in
  `system_import_equivalence.rs`; rationale in the status log's "Ruling — the
  sparse-array blob divergence". **v4's own half is QUEUED post-5.0** (human,
  2026-07-24) on the new "post-5.0 v4-side FIXES" list in
  `dogfood-findings.md` — a one-liner in v4, deliberately not made during the
  port because it moves the oracle baseline. Next candidates: finish
  P4.9G5's restore side, then P4.9G4's import execute; or a dogfood pass over
  this round's live surfaces (+ the still-owed walk Part D and Part F items
  15/16) — see phase-4.md. Versions after the round: core 0.0.352, harness
  0.0.299, host 0.0.34, web 0.0.43, cli 0.0.3, quilltap-tauri 0.0.5, SPA
  0.5.270. **After the two follow-up lanes that unified the same day** (the
  sparse-array ruling + the `qtap_import` corpus-shape fix, and the SPA
  bundle-warnings lane): core **0.0.353**, harness **0.0.301**, SPA
  **0.5.271**; host/web/cli/tauri unchanged.
- **The "finish the restore side" round (P4.9G5-resumed ∥ P4.9G6): UNIFIED on
  main (2026-07-25) — P4.9G6 CLOSED; P4.9G5 still OPEN at units 4–5, blocked on a
  human ruling at unification and ✅ RULED the same day (UNBLOCKED).** Restore now works **as far as the preview**: the
  octet-stream `?action=upload` leg (back-pressured, behind the 1-hour upload
  store on `BackupHost`), `json_stream` + `legacy_migrations` + `parseBackupZip`
  (both parse-time legacy folds; the streaming scanner's thrown messages carried
  verbatim because the preview route leaks `error.message`), and
  `systemRestorePreview` over v4's **41-key** `RestoreSummary`. The extract dir
  is owned state (`ExtractedBackup: Drop`), and the differential asserts an empty
  scratch root after every case. **The shared "recognized but not yet available"
  arm in `engine.rs` is GONE.** New committed `restore-archives/` family — five
  archives built by v4's REAL `createBackup`, read byte-for-byte by BOTH sides, so
  the restore claim never depends on v5's zip writer; no existing fixture moved.
  P4.9G6 landed the whole `new-account` UUID remap (pure; 19-case tier-1 EXACT
  family with **zero** normalization, all 38 collections byte-compared, corpus
  sha256-pinned per NDJSON line, first-run-green so sensitivity was proven by
  three deliberate mutations) — **complete and differential-proven but with no
  caller**, since the orchestrator is its only consumer. **⚠ THE BLOCKER: unit 4
  found two real v4 restore bugs** — v4 rejects every `doc_mount_points` /
  `doc_mount_file_links` row from a modern archive (raw `SELECT *` dump vs
  Zod-validating creates, so **every character vault, project store and group
  store comes back unreachable**) and restores **no user file at all**
  (`backupFormat === 2` vs a manifest that says `4`). v5 reproduces neither, so
  the tier-2 state diff is not an equality — the same shape as the sparse-array
  blob divergence. **✅ RULED 2026-07-25 (human): "I want this work, not just fail
  the same way v4 fails" — v5 DIVERGES on both, so units 4–5 are UNBLOCKED**
  (authority: `status-log.md` → "Ruling — the two v4 restore bugs"; finding 1 needs
  no v5 change, finding 2 needs the `>= 2` gate, and the divergence is
  reader-side only — the writer stays byte-identical). The lane refused to land a
  live-but-unproven restore or a dead one, per its own tier-3 rule; the
  orchestrator is written and banked in the lane record, not on main. Because
  there was no `ACTIVATE-AT-UNIFY` marker to flip, the §2 wire became
  `p4_9g6_seam_contract.rs`: compile-time signature pins plus an end-to-end
  `parse_backup_zip` → `remap_backup_data` composition proof (bijective relabel,
  disjoint id sets, manifest-is-the-caller's-job). Gate: 380 binaries / 1,616 /
  0; the round's four families by name with `--nocapture` zero SKIP; clippy both
  feature sets; release build; **no SPA run owed — neither lane touched
  `apps/web`.** Versions: core 0.0.355, harness 0.0.303, host 0.0.35, web
  0.0.44; cli 0.0.3, quilltap-tauri 0.0.5, SPA 0.5.271 unchanged. Next: **finish
  P4.9G5 units 4–5 (UNBLOCKED — the orchestrator is banked at
  `docs/developer/porting/banked/p4.9g5-unit4/`)**; or P4.9G4's import execute
  (unblocked, disjoint); or a dogfood pass (+ the owed walk Part D and Part F
  items 15/16) — see phase-4.md.
- **P4.9G5 restore-execute: CLOSED, UNIFIED on main (2026-07-25, single lane) —
  RESTORE IS LIVE IN BOTH MODES and the Backup & Restore family is COMPLETE.**
  All four Data & System cards that once answered a refusal now work.
  `systemRestoreExecute` runs v4's 35-phase orchestrator in `replace` **and**
  `new-account` (over P4.9G6's `remap_backup_data` — which finally has its
  caller, making `p4_9g6_seam_contract`'s compile-time pins load-bearing);
  `system_restore_state` diffs **43 tables across all three partitions** against
  v4's real restore over four archives (incl. `restore_new_account`, so the mode
  that went live is the mode that is proven). **THREE divergences, not the two
  ruled** — implementing the ruling found the broadest: v4 runs phase 5 (files)
  before both the Uploads mount `deleteUserData` truncates AND the project stores
  that restore at phase 13, so **v4 cannot restore any user file into a fresh or
  wiped target in either mode**, and the `>= 2` gate fix alone would not have
  helped; v5 runs files after the doc-store family (no write changed, only when
  it happens — v4's own comment calls the list "dependency order"). All three
  v4-side fixes queued post-5.0. Both of the previous lane's open leads are
  answered and **one was diagnosed backwards**: the `doc_mount_chunks` gap is not
  a baseline difference (the oracle's new `preState` dump proves both baselines
  are zero) but a real v5 gap. The unification wire closed the order's
  never-delivered tier-1 arm `restore_preview_writes_nothing` — preview being
  read-only had been *asserted in a comment, never proven*, so a preview that
  wrote would have passed every test in the repo; now proven over a populated
  library and all five archives, mutation-checked. Gate: fmt; clippy both feature
  sets; release build; **381 binaries / 1,621 / 0**; five families by name over
  fresh `e646f58b` oracles zero SKIP (uuid-remap corpus byte-identical); **no
  `apps/web` touched so no SPA run owed**. Versions: core 0.0.356, harness
  0.0.305, host 0.0.36; web 0.0.44, cli 0.0.3, quilltap-tauri 0.0.5, SPA 0.5.271
  unchanged. **TWO v5 gaps recorded with tripwires that FAIL when closed, neither
  fixed, neither restore's:** (a) a freshly provisioned character vault is not
  chunked for search where v4 chunks each document as `create_character` writes
  it (invisible to the characters differentials — none dump that table); (b)
  `chat_settings.cheapLLMSettings` writes explicit `null`s where Zod omits absent
  `.nullable().optional()` keys (needs `Option<Option<String>>` across the
  settings bags). **STILL OPEN — one item:** the tier-2 e2e beat (upload →
  preview → restore), which must run after the delete-all describe and obliges a
  full Playwright run; three lanes have deferred it, and it should ride the next
  round that already touches `apps/web`. Next: **P4.9G4's import execute** (the
  last unported Data & System half), a restore/Data-&-System dogfood pass (+ the
  owed walk Part D and Part F items 15/16), the two recorded gaps, or M6 rows 6+
  — see phase-4.md.
- **The import-execute + Post Office + chunk-on-write round (P4.9G4-resumed ∥
  P4.9E2A ∥ P4.9E2B ∥ P4.6BK): UNIFIED on main (2026-07-25) — ALL FOUR
  CLOSED.** `.qtap` import EXECUTE (the ten-map orchestrator, four per-entity
  importers, legacy folds, the seven-loop reconcile, all four conflict
  strategies; new 11-case `system_import_state`) — **the Data & System family
  is COMPLETE, every card that once refused now works** ∥ the in-chat Post
  Office server surface (the unported `lib/services/announcer/**` + four
  dispatch verbs, new committed `post-office-{main,mount}.db`, 32-case routes +
  7-case tier-3 differentials; the banked `979aec66` drift folded in) ∥ the
  Post Office SPA (Insert Announcement with its preview→approve/edit/regenerate
  loop, Compose Mail, Whisper, the megaphone + envelope gutter buttons in v4's
  grid order, seven beats) **plus P4.9G5's owed restore e2e beat — three lanes
  had deferred it; it runs green** ∥ **chunk-on-write** (v5 never chunked a
  database-store document as it was written where v4 always does, so every
  fresh character vault / project store / group store was unsearchable until
  reindexed; both write sites + a third the pin concealed, ALWAYS-chunk ruled
  and named as a divergence, `KNOWN_V5_GAPS` retired, a restore phase-order
  infidelity fixed on the way past). ⚠ The order's premise that the
  `QUILLTAP_JOB_CHILD` pin was oracle-side was **wrong** — it was Rust-side,
  across 18 families. Wires: `EngineAssembly.announcement_preview` **LIVE**
  over a host runner that rebuilds the LOGGING cheap executor per call
  (**⚠ real spend — one cheap-LLM call per Generate**); BOTH §2 chunk
  tripwires removed, E2A's having **fired on the first merged run** as
  designed; the §1 wire diffed name-for-name, clean. Gate: 385 binaries /
  1,633 / 0 with 22 families regenerated fresh at `e646f58b` zero SKIP, clippy
  both feature sets, release build, ng 227 files / 2,669, full Playwright
  **134/134 zero skips**. Two E2B scope corrections stand: composer
  drag-and-drop **is a phantom** (v4 never had it — the claim came from v5's
  own P4.6ac record), and the **RNG dropdown is deferred** because v5 has no
  `chatRng` verb (P4.d5 ported the rng TOOL, not v4's `?action=rng` route).
  Deferred loud: the blob `originalFileName` type widening (recorded, not
  taken — no behavior change, ~a dozen differential-free sites). Next
  candidates: a dogfood pass over this round's live surfaces (+ the owed walk
  Part D and Part F items 15/16), the `chatRng` verb, M6 rows 8/10 — see
  phase-4.md.
- **The `231be14c` v4-drift catch-up round (P4.d18 ∥ P4.d19 ∥ P4.d20 ∥ P4.d21):
  UNIFIED on main (2026-07-26) — ALL FOUR CLOSED; the drift debt is CLEARED and
  the oracle baseline MOVES to `231be14c`.** v4 had moved four commits in a
  single day, none of them lib-free, landing on two already-ported surfaces.
  The fictional story clock re-port (`parse_timestamp_in_timezone`,
  `ensure_fictional_base_real_time`, the creation anchor, and v4's migration as
  a **boot-repair pass** over the main partition — the P4.d7 precedent — so any
  instance v5 boots is backfilled; the corpus went 68 → 140 rows, 43 of them in
  the `calc` family) ∥ the Pascal **availability gate** end-to-end
  (`availableWhen`/`withheldWhen`, the shared fail-soft metadata table, gate
  BEFORE the `disabled` tombstone so a gated-out name stays claimable by a
  farther tier, `gate` on both Workbench surfaces) + the **tool vocabulary**
  (`references` on every roster listing — vocabulary, never odds) ∥ the
  Workbench gate SPA (client-safe `tool-gate`/`metadata-match`, the draft layer,
  "Who may reach for it", the `gated` badge, the bench verdict) ∥ the in-chat
  Pascal SPA (the two-phase run dialog + reference panel, the stacked params
  layout, the roll announcement wearing its own outcome state, and the
  `.qt-pascal-result` base block **v5 had never had at all**, which also gives
  the Workbench the accent it had been asking for since P4.6bb).
  **Zero source conflicts across 24 cherry-picked commits**; `api/types.rs`
  never opened; both ACTIVATE-AT-UNIFY markers self-activated. The wire closed
  two predicted seams: the SPA's three older `z.record` sites followed the
  server to `expected record` (P4.d20 had deliberately held off rather than put
  the browser at odds with it), and the corpus census — which caught P4.d19's
  new third row kind at **205-vs-236**, exactly as "a truncated fixture must not
  pass silently" promises — grew into a full **replay** of those 31 gate
  verdicts through the browser's own evaluator. **THREE pre-existing v5 bugs
  fixed on the way past, none of them drift, all user-visible:** a
  `datetime-local` fictional base parsing to **0**; a sub-minute LMT offset
  truncated in `timezone_offset_string` (unpredicted — caught on the first run
  of the widened corpus); and `z.record` reporting `expected object` at four
  sites plus an erased `run_custom` vault-failure sentence. Gate: 386 binaries /
  1,639 tests / 0 failed with **zero SKIP lines**, 18 differentials by name over
  oracles regenerated fresh at `231be14c`, clippy both feature sets, release
  build, ng 233 files / 2,883, full Playwright **136/136 zero skips**.
  **⚠ One pre-existing divergence found and deliberately NOT fixed:** v4
  re-parses `chats.timestampConfig` through `TimestampConfigSchema` at the
  repository write (schema key order, materialized defaults, unknown keys
  stripped, bad values 400'd) where v5 stores the request JSON verbatim; the
  chat-UPDATE path shares it, and until it is ported a partial config saved from
  the SPA lands in the DB missing v4's defaults. Deferred loud: three help docs
  banked for `p4.9i2` (v5's help text still describes the OLD custom-tools
  popover), theme-storybook NO-PORT, the migration pretty-label NO-PORT.
  Versions: core 0.0.370, harness 0.0.316, host 0.0.39, SPA 0.5.290. Round
  record: `status-log.md`.
- **Oracle baseline: `231be14c` (v4 HEAD, 2026-07-25), adopted at the
  P4.d18 ∥ P4.d19 ∥ P4.d20 ∥ P4.d21 drift-round unification — NO v4 drift debt
  remains.** Eighteen families regenerated there at unification (the whole
  pascal/tool family plus chat-timestamp, the new fictional-clock-anchor, and
  the chat-create capstone); families the round did not touch keep their prior
  regen vintage. The §3 corpus has ONE source case file and TWO committed
  copies (harness + SPA), verified `diff -q` identical at unification.
  Regenerate from a **pinned detached worktree** (`oracle-regen-pinned-v4-worktree`)
  — v4 shipped four commits in one day during the last round and cannot be
  assumed still at HEAD. ⚠ v4 is mid-4.8/4.9 dev — drift-check before every
  round. **v4 moved to `20430561` during this unification and it is
  DISPOSITIONED: docs-only** (v4's own `docs/CHANGELOG.md` +
  `docs/releases/4.8.0.md`, zero `lib/`, `app/`, `components/` or `packages/`
  code), so the baseline stays `231be14c` and **no drift debt is owed**.
  The previous baseline paragraph follows for history:
- **Oracle baseline: `e646f58b` (v4 HEAD, 2026-07-22), adopted at the
  P4.d16 ∥ P4.d17 drift-round unification — NO v4 drift debt remains.**
  The only fixture the round moved is the workspace corpus
  (`workspace-core-fixtures.json`, `_meta.baseline: e646f58b`; regen
  recipe in the P4.d16 lane record). No Rust oracle family imports the
  four drifted commits' files (verified at the P4.6bj unification), so
  every committed Rust-side oracle keeps its prior regen vintage —
  `8bf3cb5f`-or-earlier per the paragraphs below. Oracles regenerate
  directly from `~/source/quilltap-server`; pin a detached worktree
  only on drift/dirty (recipe: `oracle-regen-pinned-v4-worktree`).
  ⚠ v4 is mid-4.8/4.9 dev — drift-check before every round. The P4.11
  unification (2026-07-23) regenerated the request-envelope +
  google-wire fixtures at `e646f58b` (34 → 93 + 5 → 10 lines, both-mode)
  — v4 verified still at `e646f58b`, clean. The provider-I/O-round
  unification (2026-07-23) re-verified v4 at `e646f58b` clean and
  regenerated ALL THREE provider corpora byte-identical (request-envelopes
  93, google-wire 10, response-bodies 29 — the new family, all
  `synthetic: true` pending real captures). The dogfood-fixing-round
  unification (2026-07-24) regenerated the orchestrator / courier-images /
  enclave-step / self-inventory oracles fresh — v4 verified still at
  `e646f58b`, clean. The pre-compute + Data & System round's unification
  (2026-07-24) re-verified v4 at `e646f58b` clean and regenerated the
  precompute (NEW), system-jobs-routes (NEW), build-context-tier3 and
  orchestrator-tier3 families fresh there. Versions (after the 2026-07-24
  dogfood-fixing-round
  unification): core 0.0.341, harness 0.0.288, host 0.0.32, web 0.0.39,
  cli 0.0.3, quilltap-tauri 0.0.5, SPA 0.5.267.
  The previous versions line follows for history: (after the 2026-07-23
  provider-I/O-round unification, then the courier fold-episode
  follow-up) core 0.0.337 → 0.0.338, harness 0.0.286 → 0.0.287, host
  0.0.30 → 0.0.31, web 0.0.38, cli 0.0.2, quilltap-tauri 0.0.4, SPA
  0.5.263; (after the P4.11
  unification) core 0.0.328 (0.0.329 after a parallel dogfood fix),
  harness 0.0.282, host 0.0.30, web 0.0.37, cli 0.0.2, quilltap-tauri
  0.0.4, SPA 0.5.263.
  The previous baseline paragraph follows for history:
  **Oracle baseline: UNIFORM `8bf3cb5f` after the episodic round-3
  unification (2026-07-22).** v4 HEAD is `e646f58b` (4 commits past the
  baseline): `deab0e5d` theme/icons + `e646f58b` lint-chore are
  lib-free (the theme/icons SPA re-port stays owed); **`8d86847a`
  (tabbed-workspace deep-links) TOUCHES PORTED lib/ surface**
  (`lib/workspace/{tab-meta,types,workspace-persistence}` +
  `lib/navigation/route-to-intent`) — a workspace corpus-recapture +
  SPA re-port is OWED (dispositioned at the P4.6bj unification; the
  committed workspace corpus keeps its `b8b12695` vintage until that
  round runs). The memory-pipeline oracle families import none of the
  drifted files (verified by name at the P4.6bj unification), so their
  regen ran straight from `~/source/quilltap-server` at HEAD
  (lib-identical to `8bf3cb5f` for those families). All episodic-campaign
  families now regenerate at `8bf3cb5f`, including the previously
  deferred gate / processor / memory-tasks-creation / context-summary /
  carina / recall-history set; families untouched since earlier rounds
  keep their prior vintages. Oracles regenerate directly from
  `~/source/quilltap-server`; pin a detached worktree only on
  drift/dirty (recipe: `oracle-regen-pinned-v4-worktree`). Versions
  (after the 2026-07-22 round-3 unification): core 0.0.321, harness
  0.0.277, host 0.0.29, web 0.0.37, cli 0.0.2, quilltap-tauri 0.0.4,
  SPA 0.5.251.
  The previous baseline paragraph follows for history:
  **Oracle baseline: MIXED after the episodic round-2 unification
  (2026-07-21).** v4 HEAD is `8bf3cb5f` (4.9-dev). Rounds 1–2 rebased
  their families to **`8bf3cb5f`**: round 1's memory-row/pure +
  character + new-chat families (provisioning, memories
  read/tier-2/routes+config, chats read/tier-2, episodic, weighting,
  injector, delete, cascade, housekeeping, ranking, vault-json-parsers,
  characters mutations/reads/create/update/provision/scaffold,
  vault-character-write) and round 2's retrieval/tools/replay families
  (distill [NEW — the memory-tasks SPLIT], recall-tags,
  context-feeders-leaves, build-context, search-tools,
  scriptorium-tools, tool-definitions + canonical, pseudo-tool-prompts,
  tool-build, recall-replay [NEW], vault-conv-search [NEW],
  salon-mutations). The **round-3 families stay at `7e6d13e5`**:
  `QT_ORACLE_GATE` (gate tier-3, SKIP by design), the processor tier-3,
  the memory-tasks CREATION cases (`QT_ORACLE_MEMORY_TASKS`),
  context-summary/fold, carina-extraction, recall-history. Regenerate a
  round-3 family at `8bf3cb5f` only when round 3 ports it; families
  untouched since earlier rounds keep their prior vintages. v4's
  checkout is clean at `8bf3cb5f`, so **oracles regenerate directly
  from `~/source/quilltap-server`**; pin a detached worktree only on
  drift/dirty (recipe: `oracle-regen-pinned-v4-worktree` — symlink
  node_modules at root + `packages/{quilltap,plugin-types,plugin-utils}`
  + `plugins/dist/*`). ⚠ v4 is mid-4.8/4.9 dev — a version/tag commit
  may land; drift-check before every round. Versions (after the
  2026-07-21 episodic round-2 unification): core 0.0.313, harness
  0.0.270, host 0.0.29, web 0.0.37, cli 0.0.2, quilltap-tauri 0.0.4,
  SPA 0.5.245.
  The previous baseline paragraph follows for history:
  MIXED after the episodic round-1 unification (2026-07-21): round 1's
  families at `8bf3cb5f`; the deferred behavior families (gate,
  processor, memory-tasks tier-1, recall-tags, context-summary/fold,
  carina-extraction) at `7e6d13e5`. Versions at that unification: core
  0.0.305, harness 0.0.263, host 0.0.28, web 0.0.36, quilltap-tauri
  0.0.4, SPA 0.5.245.
  The previous baseline paragraph follows for history:
  v4 `7e6d13e5` (4.8.0-dev.92), adopted 2026-07-20 at
  the state-cascade drift-catch-up unification. Both prior pins
  (`qt-v4-pin-b8b12695`, `qt-v4-pin-7e6d13e5`) are RETIRED. Every family
  the state-cascade + release-sweep drift touched regenerated at
  `7e6d13e5` (incl. the 53-family neutrality sweep and the seven
  renderer-transitive families); untouched families' committed oracles
  keep their earlier regen vintages. Versions at that unification: core
  0.0.297, harness 0.0.257, host 0.0.27, web 0.0.36, quilltap-tauri
  0.0.4, SPA 0.5.241.
  The previous baseline paragraph follows for history:
  v4 `b8b12695` (4.8.0-dev.76), adopted 2026-07-19
  at the P4.d9 KaTeX drift-catch-up unification; oracles regenerated
  from the pinned detached worktree `/private/tmp/qt-v4-pin-b8b12695`
  after the `c53510c7`/`7e6d13e5` drift. All seven families the
  KaTeX drift transitively touches regenerated there and proven
  output-neutral; untouched families' committed
  oracles keep their earlier regen vintages. (The old
  pin `/private/tmp/qt-v4-pin-616930db` stays RETIRED.) Versions (after
  the 2026-07-19 p4.9j workspace-tabs unification): core 0.0.283, harness
  0.0.246, host 0.0.22, web 0.0.34, quilltap-tauri 0.0.4, SPA 0.5.209.
  The previous baseline paragraph follows for history:
  v4 `616930db` (4.8.0-dev.75), adopted 2026-07-18
  at the drift-catch-up unification. Every family the llm-consult
  drift touched regenerated there (the drift was
  Pascal-family-confined); untouched families' committed oracles keep
  their earlier regen vintages. Versions at that unification: core
  0.0.283, harness 0.0.246, host 0.0.22, web 0.0.34, quilltap-tauri
  0.0.4, SPA 0.5.183.
  The previous baseline paragraph follows for history:
  v4 `d68638b4` (4.8.0-dev.72), adopted 2026-07-17
  at the d68638b4-round unification (every family the drift touched
  regenerated there; untouched families' committed oracles date to
  `e3593f75`, verified behavior-neutral across the gap at round
  planning). The previous baseline paragraph follows for history:
  v4 `e3593f75` (4.8.0-dev.62), adopted 2026-07-17
  at the P4.d5 ∥ P4.6ay unification. The `02865bdb`→`e3593f75`
  drift is fully absorbed EXCEPT the Pascal feature itself (P4.6ay
  units 2, 4–9 + the unstarted SPA — the open order). v4 HEAD at
  unification was `444c7fd6`, two commits past the baseline, both
  dispositioned (lib-behavior-free, verified): `8e4b00d4` (the Salon
  whisper-visibility client fix — the toggle surface is unported in
  v5; its new `whisper-visibility.ts` helper + tests are the port
  target for that future Salon slice; two `help/*.md` edits — v5
  syncs help docs from disk at runtime; RunToolModal copy — unported;
  → 4.8.0-dev.63) and `444c7fd6` (two feature docs, mirrored under
  `docs/v4/developer/features/`). **The predicted in-flight
  custom-tools/character-metadata feature LANDED (2026-07-17): v4 is
  now at `d68638b4` (4.8.0-dev.72)** — the drift is classified and a
  FOUR-lane catch-up round is PLANNED (P4.d7 ∥ P4.6ay-resumed ∥
  P4.6az ∥ P4.6ba; orders committed, round record "Round planned —
  the d68638b4 drift catch-up" in the status log). The round's
  oracles regenerate at `d68638b4`; main's committed oracles remain
  at `e3593f75` until the lanes land — expect the pascal +
  tool-definitions + provisioning tripwires to trip during the round,
  by design. Drift-check before every round; if the v4 tree is dirty,
  regenerate oracles from a pinned detached worktree (round record).
  The P4.d3 note stands: ⚠ v4's `quantize-embeddings-v1` migration is
  one-way — back up Friday before first running v4 `4.8.0-dev.52`+
  against it. ⚠ **v5 CANNOT read or write messages on a pre-4.8.0 v4
  instance** (`no such column: pascalMeta`) — migrate a dogfood copy
  to 4.8.0's two ALTERs before pointing v5 at it. Still NOT drift:
  v4's embedding blob-registration bug is structurally impossible in
  v5 (no registry exists; no `repair-text-embeddings` needed).
  Versions: core 0.0.271, harness 0.0.239, host 0.0.20, web 0.0.28,
  quilltap-tauri 0.0.4, SPA 0.5.169.
- **Standing deferrals + gotchas:** tracked in the work orders, the
  status log, and the memory notes — not here.
