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
  **The human M5 walk is the one remaining acceptance step** (the
  staged instance + recipe: the status-log round record). Deferred
  loud: native niceties, turnkey `tauri dev`, updater/signing/release
  (D21), uniffi/mobile, Last-Event-ID replay. Next candidates: the M5
  walk + a Tauri dogfood pass, a Friday-copy dogfood pass, the
  small-rider pool, or the M6 screen-parity review.
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
  **Remaining acceptance: the combined human M5 + finding-#12 walk**
  (recipe: the P4.7c order header). Deferred loud: the
  `/generate-image` screen, NewChatModal-on-card, quick-hide,
  Windows/Linux one-origin re-checks. Next candidates: the human
  walk, a homepage/Tauri dogfood pass, the small-rider pool, or the
  M6 screen-parity review.
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
  (`p4.9l`). **Next: the human M5+#12 walk (still outstanding); then
  the M6 backlog items 1–4 (`p4.9a`/`p4.9c`/`p4.9b`/`p4.9d`) as the
  natural next round; `p4.9j` (workspace tabs — v4's DEFAULT shell)
  needs a human ruling first.**
- **Oracle baseline: v4 HEAD `02865bdb`** (rebased 2026-07-14 with the
  P4.d4 skip-signal drift re-port — the trailing-sentinel strip; the
  P4.d3 quantized embedding codec note stands: ⚠ v4's
  `quantize-embeddings-v1` migration is one-way — back up Friday
  before first running v4 `4.8.0-dev.52`+ against it. Drift-check
  before every round. Known benign drift past the baseline: `34746bed`
  (2026-07-16) is a docs-only feature SPEC (Pascal custom pseudo-tools,
  no code) — expect a real drift re-port when it lands as code.
  Versions: core 0.0.232, harness 0.0.209, host 0.0.19, web 0.0.25,
  quilltap-tauri 0.0.3, SPA 0.5.134.
- **Standing deferrals + gotchas:** tracked in the work orders, the
  status log, and the memory notes — not here.
