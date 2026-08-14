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

### ⛔ NEVER wait on a long job with a `pgrep`/`grep` poll loop

**This has cost the human hours at a time, repeatedly.** The gates here run
long (`cargo test --workspace` = 400+ test binaries; the Playwright suite;
release builds), and the temptation is to write
`until ! pgrep -f "cargo test --workspace"; do sleep 30; done` and chain the
next step behind it. **That loop can never exit: the watcher shell's OWN
command line contains the pattern, so `pgrep -f` matches itself.** Launch
several and they match each other too. The real job finishes in minutes and
the watchers spin until timeout — and anything chained behind one (a
Playwright run, a gate step) *never starts at all*, while the transcript
still says "running". That is the worst failure mode available: silent,
invisible, and indistinguishable from slow.

- **Use `run_in_background: true` and wait for the completion
  notification.** It fires exactly once, when the command actually exits,
  and it carries the exit code. This is the mechanism for "tell me when X
  finishes" AND for "run B after A" — chain by starting B *from the
  notification*, never by polling for A's absence.
- **Do not launch a second watcher for a job you are already watching**,
  and **never watch a watcher.** One job, one background command, one
  notification. A watcher that greps another watcher's output file is the
  same bug wearing a disguise — it survives a fix aimed at `pgrep`, and
  killing the root leaves it waiting on a corpse forever. Chains die
  silently from the head down.
- **Never pipe a gate through `tail -N`.** It discards the per-binary
  `test result:` lines the round record needs; capture the full output (or
  `grep -E "^test result|FAILED"`) and read the file.
- If a process check is genuinely unavoidable: prefer a **sentinel file**
  the job writes on exit and test for that; failing that match the binary
  itself (`pgrep -x quilltap-web`), never a substring of your own command.
  A `pgrep -f` whose pattern could appear in the watcher is always wrong.
- **Symptom to recognize instantly:** several background tasks "still
  running" long after the work must have finished, and `pgrep -fl <pat>`
  shows only `/bin/zsh -c … eval 'until ! pgrep -f <pat>…'` shells. Kill
  them and re-run the real command in the background.

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
- **Rounds 2026-07-10 → 2026-07-30 — ARCHIVED.** The verbatim round bullets
  for that span (the P4.6f/g/h round through the `5cc76688` drift catch-up)
  now live in `docs/developer/porting/claude-md-status-history.md`; the
  full round records were always in `status-log.md`. The arc, compressed:
  every remaining server dispatch surface + SPA vertical landed and closed
  (characters, groups/projects/scenarios, the listing surfaces, the
  Commonplace Book, the Salon terminal pane, Document Mode, the whole
  Scriptorium incl. the bespoke file manager [D18], courier + chat images,
  autonomous rooms, the files family, the New-Chat vertical, the settings
  Chat/Images tabs + form fields, token/cost display, background
  generation, the LLM Inspector, My Photos, quick-hide, About + Profile,
  Generate Image, the home dashboard); ProseMirror was adopted (D17) with
  the v4-dialect markdown bridge; the Tauri 2 shell landed whole (P4.7,
  one-origin `qtap://`, the human M5 walk done 2026-07-18); the tabbed
  workspace shipped as the default shell (`p4.9j` — the F1 v4-retirement
  gate) with all 22 tab kinds hosted; the four-tier state cascade, Pascal
  custom tools end-to-end + the Workbench, the Brahma console, and the
  llm-consult wire all went LIVE; the episodic-recall campaign ran all
  three rounds and the memory extraction/fold pipeline was wired into
  production (P4.6bj); the provider layer was rewritten and hardened
  (P4.11 non-streaming builders, the P4.13/P4.14 `StreamMessage` rewrite,
  P4.10 dev-grade Docker packaging); the whole Data & System family
  landed through backup/restore/import/export EXECUTE in both modes
  (P4.9G1–G6 + P4.d23, with the ruled deliberate divergences from v4's
  own restore bugs); the Post Office + chunk-on-write closed; and the
  oracle baseline moved through ~15 pins to `5cc76688` with drift debt
  cleared at each round. Deferred items from that era are tracked in the
  work orders and later bullets, not here.
- **The drift + standing-red + dogfood round (P4.D29 ∥ P4.20 ∥ P4.21 ∥
  P4.9P): UNIFIED on main (2026-07-30) — ALL FOUR CLOSED; the oracle
  baseline MOVES to `dcd9440a`; dogfood #37 and #38 are FIXED.** The
  `dcd9440a` store-overlay read-hardening re-port (a failed
  `properties.json` read can no longer wipe a settings bag; the corrupt-
  store refusal arms through BOTH `StoreEntity`s, mutation-proven; a
  pre-existing lowercase-label `Display` divergence fixed on the way; the
  unit-4 routes-envelope tier-2 arm ESCALATED — v4 answers a deliberate
  contextful 503 where v5 still answers 500 + leaked detail, the fix
  belongs to `api/**` with an ordered shape recorded in the lane record) ∥
  the standing `enclave_step_tier3` red CLOSED — **the diagnosis REFUTED
  the planning hypothesis**: v4 never bails on the Fold turn; the oracle
  case still carried a W4.11a-era stub of `runPreContextPreCompute` that
  P4.19 retired in one sibling file and missed here, so the harness was
  lying about v4 and v5 was NEVER making an extra production call (zero v5
  source changed; the precompute family now diffs the DISTILL PROMPT
  itself — the window/cap/truncation in one comparand — and gained the two
  window-differing shapes, mutation-proven) ∥ P4.21: **image attachments
  reach the provider wire on every completion path** (dogfood #37 fixed —
  the carrying types, all four drop sites, the nine builders' recorded
  byte shapes, `attachmentResults` both modes, the corpus blind spot
  closed: request-envelopes 93 → 146, google-wire 10 → 18, all
  pre-existing vectors byte-identical, coverage shape-asserted; three
  v4-side finds recorded — the @openrouter/sdk refuses non-streaming
  vision sends in v4 itself, Grok's text/PDF arms are dead code, the
  stale `attachment-support.ts` client map) ∥ P4.9P: **the top
  page-toolbar vertical** (dogfood #38 fixed — the `uiSearch` verb +
  `GET /api/v1/ui/search` with a 23-case differential over a /tmp-built
  five-type fixture, quirks carried; the toolbar + slot service + shell
  cutover with the sidebar-footer stopgap RETIRED; queue-status badges
  over the live jobs route with v4's event-driven poll; the search
  bar/dialog/results; the content-width service on v4's exact key with
  the 72rem→75rem correction; four e2e beats incl. the lock/unlock gate
  walk). **The §3 review caught a shipping bug + two fidelity gaps, all
  fixed on the unify branch with pins:** the search dialog's open-seeding
  effect tracked `selectedTypes` (chips froze/reset in any pre-seeded
  dialog — spec added, mutation-proven), the Anthropic text-document
  decode-failure arm diverged from Node's never-throwing lenient decoder
  (now byte-faithful, probed on Node 24, pinned by the new
  `text-attachment-mangled-b64` corpus vectors), and the CHAT_MESSAGE
  `llm_logs` projection dropped the attachment bags v4 logs. Gate: 402
  test binaries / 1,717 / 0 (see the round record for the by-name list);
  clippy both feature sets; release build; ng 259 files / 3,154; full
  Playwright green (numbers in the round record). Deferred loud: the
  store-unavailable 503 envelope (escalated, ordered next), the Zod
  format-validator gap on property bags, wire-byte unit pins for drop
  sites 1/3, the Salon slot adoption behind the workspace per-tab toolbar
  bridge, the `?msg=` anchor + `/photos?tag=` filter, the ten no-analog
  queue-trigger sites. **💸 P4.21's live proof (real describe + in-chat
  vision on the Friday copy) joins the owed dogfood pass.** Versions:
  core 0.0.418, harness 0.0.363, web 0.0.55, SPA 0.5.326; host/cli/tauri
  unchanged.
- **The `ff12f491` drift catch-up round (P4.D30 ∥ P4.D31 ∥ P4.D32 ∥
  P4.D33 ∥ P4.D34): UNIFIED on main (2026-07-31) — ALL FIVE CLOSED; the
  oracle baseline MOVES to `ff12f491` and the drift debt is CLEARED.**
  Nineteen v4 commits absorbed in five lanes. The Pascal canonical-reader
  re-port (blob-stored definitions load, boundary enforcement, the
  SOURCE_NOT_FOUND race skip; the new `read_mount_file_bytes_conn` twin; a
  pre-existing v5 strict-UTF-8 divergence fixed via `from_utf8_lossy`; the
  `pascal-run-custom-*` fixture REBUILT with two blob definitions; new
  6-case `pascal_definition_reader_equivalence` — an order premise
  disproved: boundary escapes are unreachable, pinned by unit test as v4
  pins them) ∥ restore memory-id preservation (one call site; the NEW
  `restore-archive-memory-graph.zip` because the seven committed archives
  were structurally BLIND to the bug in `new-account` mode — the
  `<minted-N>` normalizer labels correct and wrong ids identically; the
  sixteen 4.8 columns made measurable, twelve had no non-default value in
  any archive) ∥ the release-refactor neutrality sweep (**290 of 324
  non-sibling families regenerated + re-run at the pin — the four
  "no functional change" commits PROVEN neutral**; the helper mirrors; the
  dead-code follow; `33cca411` was NOT a NO-PORT — the CLI help/completion
  re-port fixed 6-of-135 Tier R reds; the `pricing_fetcher` oracle mock
  had been STARVING v4 since SDK 0.13; 28 families' recipes did not
  survive mechanical extraction — a stated shortfall; standing reds
  surfaced: the `canChooseOutfit` projection gap, `terminal_tools`) ∥ the
  provider SDK wire check (**openai 6.48→7.2 + `@openrouter/sdk`
  0.13.66→1.2.2 moved v4's wire NOT AT ALL** — all four corpora
  byte-identical, provably regenerated against the new SDKs; the three
  recorded refusals still refuse; TWO real pre-existing v5 bugs found and
  fixed on the authenticated OpenRouter pricing path — the SDK key remap
  v5 never reproduced [364/364 context lengths + 298 tool-capable models
  lost] and the 500-row page loop [catalogue at 364 and growing]; new
  `openrouter_sdk_pricing_equivalence` with the REAL SDK in the oracle
  loop) ∥ the SPA drift riders (the xterm-6 two-tier theme read; the
  exited-session input disable — newly live in BOTH apps; the three qt-*
  utilities + hover variants v5's templates referenced and nothing
  defined — 57 `qt-text` sites inheriting colour; five
  `qt-icon-button`→`qt-button-icon` transpositions; the shared Staff
  display-name table; `coreErrorMessage`). Six NO-PORTs dispositioned
  (`71dcc7e8` `80cafed5` `77c480d0` `ff12f491` `f46b0554` `0b9320a3`) plus
  the mid-round `e1be028b` (release infra, zero lib). The §3 unification
  review found NO blocking issues. Gate: the round's 15 differentials by
  name over oracles regenerated FRESH from a pinned `ff12f491` worktree,
  zero SKIP; fmt/clippy both feature sets; release build; full workspace
  tests; ng test; ng build; full Playwright (numbers in the round
  record). **💸 Live proofs owed to the next dogfood pass:** the
  OpenRouter pricing fix (real context lengths + tool-capable models with
  a real key) joins P4.21's vision proof and the toolbar/search walk.
  Versions: core 0.0.425, harness 0.0.368, cli 0.0.4, host 0.0.52, SPA
  0.5.331; web/tauri unchanged. Round record: `status-log.md`.
- **The dogfood-debt + sweep-debt round (P4.22→P4.23 ∥ P4.24 ∥ P4.25 ∥
  P4.26 ∥ P4.27): UNIFIED on main (2026-07-31) — ALL SIX ORDERS CLOSED;
  the baseline STAYS `ff12f491`.** The character-vault
  present-but-unparseable write refusal (finding #47 — a DELIBERATE
  DIVERGENCE pinned in BOTH directions; the corpus arms go red the moment
  v4 lands its own fix, and **the v4-side fix remains URGENT with the
  human**) + the store-unavailable contextful 503 end-to-end
  (`ErrorKind::Unavailable`, the `CoreError` entity carry, v4's exact
  two-key bodies, SPA mirror — the P4.D29 escalation CLOSED) ∥ LLM-log
  retention LIVE (the last unhandled job type — finding #40 CLOSED;
  ECMAScript calendar-day cutoff with UTC + DST-Chicago legs; the
  enqueuer's first differential caught a second bug — a SQL-NULL settings
  cell dropped that user from the sweep forever, fixed) ∥ the toast
  subsystem + the 106-file census (68 converted / 15 OPEN / 23 unported —
  finding #42; the OPEN rows are the follow-up worklist) ∥ the 91-row
  announcement audit (finding #43 — the ordered lead REFUTED; the real
  cause was v5-invented system-slab styling on every expanded
  announcement; six structural divergences fixed incl. legacy kind
  inference and the Staff author/portrait arm) ∥ D32's sweep debt cleared
  (`canChooseOutfit` — the omitting reader was a FIFTH site;
  `terminal_tools` case rot repaired; the committed
  `harness/tools/recipe_sweep.py` driver, 0 non_extractable). **The §3
  unification review caught two CONVERTED-marked census rows missing v4's
  success toasts** (the round's own bug class; fixed with spec pins) +
  four smaller edges. Gate: 407 test binaries / 1,746 / 0 (cargo exit 0)
  with the round's eleven oracle env vars; ten families by name fresh at
  `ff12f491` zero SKIP; clippy both feature sets; release build; ng 264
  files / 3,210; full Playwright green (numbers in the round record).
  New sweep debt recorded: the autonomous-rooms oracle's jest child-fork
  rot (pre-existing, diagnosed, next maintenance pass). Next candidates:
  a dogfood pass over the round's live surfaces, the toast census's 15
  OPEN rows, the app-wide renderingPatterns template gap (P4.26's banked
  finding), `p4.9h` — see phase-4.md. Versions: core 0.0.433, harness
  0.0.373, host 0.0.54, web 0.0.56, SPA 0.5.355; cli/tauri unchanged.
  **The round's two full-suite-only Playwright intermittents are CLOSED
  (2026-08-01, a spec-only follow-up):** both were one shape — a
  page-initiated refetch the beat triggered but never awaited — each
  reproduced deterministically with injected delays, then hardened with
  no assertion weakened and no product code changed. Suite 168/168 zero
  skips; SPA → 0.5.357, no crate touched. Nothing on the candidate list
  moved. Record: `status-log.md` → "Follow-up — the two flake-prone
  beats deflaked".
- **The `c4d4b0de` v4-drift catch-up round (P4.D35 ∥ P4.D36 ∥ P4.D37 ∥
  P4.D38 ∥ P4.D39 ∥ P4.D40): UNIFIED on main (2026-08-01) — ALL SIX
  ORDERS CLOSED; the oracle baseline MOVES to `c4d4b0de` and the drift
  debt is CLEARED.** v4 shipped TEN commits in roughly two days, four onto
  already-ported surfaces. The Pascal side-effects feature end to end (the
  closed eval-free expression grammar — ported TWICE, Rust + a client-safe
  TS twin, error sentences byte-identical, tokenizer walking UTF-16 units
  because v4's positions do; the tiered "write where it lives" applier
  split pure-plan/impure-commit over v5's four heterogeneous write paths;
  `chipLabel`; the two-block bubble; the Workbench Side Effects card and
  dry run) ∥ whispered manual announcements (the audience resolver, the
  POST-400/preview-silent asymmetry, empty-array→NULL, the
  audience-replaces-roster rewrite, the "Who hears it" dialog, the chip
  whisper tag on BOTH render sites since v5 chips only Staff-signed
  announcements) + announcement attribution in LLM context + the
  whisper-kind narrowing (**a real v5 leak closed**: Prospero's
  `group-context` whispers now honour All Whispers) + the whisper-label
  WCAG values in all six bundled themes ∥ the tri-tier wardrobe at chat
  start (merged pools filtered `isDefault` LAST so a personal opt-out
  still shadows a shared default, composite hydration, `join_all` resolve
  with serial caller-order commit, the 60 s bound, the deliberate-nudity
  contract) ∥ the editor's sub-list indentation contract (**PARTIAL by
  design** — v5's CommonMark parser never had v4's flattening bug; it
  gained unit-preserving export, Tab/Shift-Tab confined to list items, and
  the toolbar + source-mode controls). Two commits NO-PORT with evidence
  (`4f7e09fa` flushSync — v5's `afterNextRender` is already the deferred
  shape; `e1be028b` packaging). `generateDDL` untouched — no D23 re-dump.
  **The §3 review's headline catch: a `--ours` conflict resolution had
  silently deleted P4.D39's `futures-util` + tokio-`time` dependency
  block** (the playbook's "a Cargo.toml conflict is not version-only"
  rule), found by auditing every lane's non-version delta rather than by a
  build. It also found **six committed oracle recipes that could no longer
  run verbatim** — three pointing at retired `/tmp` pins, one leaning on a
  sibling recipe's staging, two sidecar readers defeated by the sweep's
  fixture shield — all repaired and green, none a port regression. Wires:
  three ACTIVATE-AT-UNIFY constants flipped LIVE, the §C corpus
  re-committed at 299 rows (10 title + 258 definition + 31 gate), and
  three contracts diffed name-for-name clean. Gate: 409 test binaries /
  1,798 tests / 0 failed with the round's 64-variable env block and **all
  42 families positively confirmed to have RUN**; clippy both feature
  sets; release build; ng test 268 files / 3,639; full Playwright 172/172
  zero skips.
  Versions: core 0.0.444, harness 0.0.382, host 0.0.56, SPA 0.5.374.
  **Outliving the round:** P4.D39's tier-3 client half (defect 2's
  composer side — rides the deferred new-chat wardrobe-composer family),
  and **a human ruling requested on P4.D40's (a)-edge** (`1. a` + a
  2-column child: v4's stack nests it, CommonMark makes siblings; landed
  as a both-directions pinned divergence). 💸 Live proofs owed to the next
  dogfood pass: cross-tier effect writes, tri-tier dressing (the
  merged-pool `llm_choose` now fires where it used to skip the model), and
  the whispered-announcement flow.
- **The hard-link-groups + restore-remainder round (P4.D41 ∥ P4.28 ∥ P4.29 ∥
  P4.30): UNIFIED on main (2026-08-03) — ALL FOUR CLOSED; the oracle baseline
  MOVES to `40319484` and dogfood findings #57–#60 CLOSE.** The whole
  `40319484` drift absorbed in one lane: `doc_mount_file_links.linkGroupId`
  through the D23 re-dump + the boot ensure + the orphan-backlog sweep, the
  write fan-out with orphan GC (**a required v5 behavior change too — v5
  relied on an `ON DELETE CASCADE` schema-generated tables don't have**),
  `link_groups` sibling re-chunking, link-binds/copy-doesn't, export/import
  carry, and the CLI's group-keyed links count (Tier R 136/136) — with TWO
  v4-side bugs found and queued (v4's own sibling-reindex pass is DEAD CODE
  — `queryJoined` never selects the column — pinned as v5's one deliberate
  divergence in this family via `CHUNK_DIVERGENCES`; and `gcOrphanedFileRow`
  throws on a mount index lacking the lazily-created blobs table) ∥ the
  restore/backup remainder under the standing ruling: annotations wiped
  (v4 never wipes them — pinned), #58 diagnosed to orphaned rows from
  store-deletes-without-children (43+118 measured on the real instance;
  fixed reader-side with named skip sentences + its own committed archive;
  **the delete-path ROOT CAUSE still needs an order**), #59's silent
  skipped-files now warn + surface, the job pump held still through
  restore/delete-all (RAII, operator-stop respected), the INSERT-tolerance
  survey run as a 7-archive × 2-mode MEASUREMENT over the NEW committed
  migration-vintage fixture (v4's real migration chain replayed; no restore
  site needs tolerance today; the one exposure pinned by a live tripwire) ∥
  the toast census's OPEN rows → ZERO (92 sentences byte-for-byte, 18
  reclassified UNPORTED with named lanes, invented inline surfaces retired)
  ∥ roleplay-template rendering threaded into every message surface (the
  P4.26-banked app-wide gap; parity corpus 40 → 51 captured from v4's real
  renderer; `roleplayTemplateName` proven dead in v4 itself — nothing to
  port). **The §3 unification review caught the round's would-have-shipped
  bug:** the group re-chunk pass had reached only the repo-method twin of
  `write_database_document` — the free-function twin (doc-edit / Document
  Mode / scenarios / characters API) got it on the unify branch with a
  mutation-proven pin; the review also sealed the predicted two-vintage
  seam (the vintage test now mirrors boot), added the #58 v4-convergence
  pin + the PumpPause WIRING test, restored v4's fixed files-browser
  failure sentences, and retired one more invented inline banner. Gate:
  411 test binaries / 1,833 / 0 with the round's env block; the round's
  families by name zero SKIP over fresh `40319484` oracles; clippy both
  feature sets; release build; ng 276 files / 3,780; full Playwright green
  (numbers in the round record). Standing loud: the `c988fbd2` Pascal
  run-presets drift catch-up OWED; `doc_text`/`doc_fm` stale-RED
  (pre-existing oracle-mock conflict with chunk-on-write — needs its own
  ruling); the vintage-tolerance follow-up tripwire. Versions: core
  0.0.452, harness 0.0.388, cli 0.0.5, SPA 0.5.395; host/web/tauri
  unchanged. Round record: `status-log.md`.
- **The `49769ec4` drift catch-up + store-delete round (P4.D42 ∥ P4.D43 ∥
  P4.31 ∥ P4.32): UNIFIED on main (2026-08-04) — ALL FOUR CLOSED; the
  oracle baseline MOVES to `49769ec4` and dogfood #58's root cause is
  FIXED.** Bounded provider requests end-to-end (the 45 s/180 s cheap-LLM
  attempt deadline + v4's timeout message bytes + the ruled error row,
  the 60 s memory-recap phase ceiling, `CompletionParams.
  request_timeout_ms` → a per-call `TransportPolicy` with retries-off
  under a budget, the 600 s → 300 s default, and streaming bounded
  first-byte-only — all unit-tier proven with stalling socket servers,
  the three provider corpora regenerated BYTE-IDENTICAL at the pin) ∥
  the Pascal run-presets vertical (the listing's `vaultMountPointId`,
  the `tool-presets` TS contract + v4's suite 1:1, the presets section
  as its own component over the EXISTING mount-file verbs, a live e2e
  beat) ∥ the store-delete cascade chokepoint (ONE transaction;
  documents/blobs/group-links divergences pinned BOTH directions over
  the new committed `store-delete-*` family's whole-table census) + the
  boot/daily orphan reaper (heals the measured 43+118 on next boot —
  the live proof is owed) + the bare repo delete defused ∥ the ruled
  doc-edit oracle un-mock (`doc_text`/`doc_fm` GREEN with the chunk
  pass positively asserted; the six recipes repaired and
  sweep-runnable). **The §3 review caught four real minors before they
  shipped** — the worst a fail-soft gate that would have silently
  no-opped the #58 repair forever on text-only instances; also the
  presets host's display:inline, the recap ceiling's dropped v4
  warnings entry, and the P4.32 doc-staleness set (all fixed with pins
  on the unify branch, `70bf9f05`). **The one escalation was RULED same-day**
  (import overwrite claims the WHOLE store incl. folders; store identity
  by ID, not name; import create preserves archive ids; character-vault
  references by ID everywhere) — ordered as `p4.33-import-overwrite-id-
  identity.md`, pinned both directions meanwhile. D42's
  75-family neutrality sweep re-measured the recipe-rot debt (19
  unrunnable + ~7 stale-red incl. `compression_tier3` latent since
  P4.13 and the `8bf3cb5f` native-tool-prompt wording gap) — wants its
  own maintenance order. Gate: 412 test binaries /
  1,848 / 0 with the round's 31-var env block; 14 families regenerated
  fresh from a PINNED `49769ec4` worktree and re-run by name zero SKIP;
  clippy both feature sets; release build; ng 278 files / 3,829; full
  Playwright 177/177 zero skips, the preset beat LIVE.
  Versions: core 0.0.461, harness 0.0.395, host 0.0.57, web 0.0.57,
  SPA 0.5.398; cli/tauri unchanged. Round record: `status-log.md`.
- **The `7fe9fe40` drift catch-up + import-identity + recipe-rot round
  (P4.D44 ∥ P4.D45 ∥ P4.33 ∥ P4.34): UNIFIED on main (2026-08-04) — ALL
  FOUR CLOSED; the oracle baseline MOVES to `7fe9fe40` and the drift
  debt is CLEARED.** The New-Chat roleplay-template picker end-to-end
  (the tri-state `roleplayTemplateId` riding the `ChatCreate` flatten
  seam — `api/types.rs` never opened; capstone family 14 → 19 cases
  with the un-normalized `chat_template_ids` section after mutation
  testing caught the UUID-normalizer blindness; the SPA dropdown +
  touched latch + omit-on-failed-fetch; a live e2e beat closing the
  loop to the persisted column) ∥ the asterisk-narration re-port
  (thirteen strings + native-tool-prompt rule 1 byte-exact, closing the
  `8bf3cb5f` wording debt with it — all six direct families' measured
  RED→GREEN flip; the broken build-context fixture builder + its
  missing TZ pin repaired) ∥ the P4.33 ruling discharged (import
  overwrite claims folders; store identity is the ID with id-preserving
  create; the by-ID census — four `store_identity_*` arms +
  `FOLDER_CLEAR_DIVERGENCE`, all both-directions with convergence
  retirement) ∥ the recipe-rot repair (the "19 unrunnable" hypothesis
  REFUTED — 8 venue-healed, 2 driver-healed, 1 sweep artifact, 4+4 real
  and fixed; the driver gained `--self-test` + the durable `--run-all`
  artifact; the autonomous-rooms oracle fork race fixed; the R1
  ruled-row pin on `compression_tier3`). **The §3 review's headline:
  two lane records disagreed on the one remaining red — adjudicated by
  measurement, D45's "P4.13 ruled row" claim was wrong, and the
  confirmed cause is a STALE ORACLE MOCK** (v4 folds live in
  production, `lib/chat/context-summary.ts:519`; v5 is faithful; the
  un-mock is a small owed order). Gate: 412 test binaries / 1,848 / 0;
  the 19-family phase-2 sweep 18 ok + the escalated red; clippy both
  feature sets; release build; ng 278 files / 3,843; full Playwright
  **178 passed / 0 failed / 0 skipped (4.5 m)** — the suite grew 177 → 178 with the new template-picker beat. Versions: core 0.0.465, harness 0.0.397, host 0.0.58,
  SPA 0.5.401. Round record: `status-log.md`.
- **The `7189a968` import/export drift round (P4.D46 ∥ P4.D47 ∥ P4.D48 ∥
  P4.36): UNIFIED on main (2026-08-05) — ALL FOUR CLOSED; the oracle
  baseline MOVES to `7189a968`.** The predicted export/import overhaul
  absorbed end to end: the embedding strip (writer + reader + one
  `EMBEDDING_GENERATE` per imported memory), all FIFTEEN export types
  + the exhaustive listing, the doc-stores-before-group-links ordering
  fix (mutation-proven), compact backup + restore steps 24a/25 +
  `RestoreSummary.embeddingReconcile`, the tri-state plugin-config
  `enabled` carry, the widened `system-data-*` fixture + the committed
  `restore-archive-compact.zip` ∥ the SPA fifteen-type picker + preview
  `detail` line + compact toggle (the gated beat LIVE — suite 178 →
  179) ∥ the `be2c9cbb` Anthropic-SDK jump PROVEN wire-neutral (four
  corpora byte-identical at 0.115, dated) + five infra NO-PORTs + the
  `QUILLTAP_TIMEZONE`/`TZ` container resolver ∥ the escalated
  `context_summary_service_tier3` stale red RETIRED (a SECOND stale
  mock of the P4.20 class found by consequence and fixed; the fold
  pass's WRITES are now comparands). Gate: 412 binaries / 1,854 / 0;
  twelve families by name over PINNED `7189a968` oracles zero SKIP;
  clippy both feature sets; release build; ng 281 files / 3,870; full
  Playwright **179/179 zero skips**. Versions: core 0.0.467, harness
  0.0.399, web 0.0.60, SPA 0.5.407. Round record: `status-log.md`.
- **The `f7f1a956` Almanack round (P4.D49 ∥ P4.37 ∥ P4.38 ∥ P4.39):
  PARTIALLY UNIFIED on main (2026-08-05) — P4.D49/P4.38/P4.39 CLOSED;
  P4.37 OPEN (its pure half landed; resume list in its order header);
  the oracle baseline MOVES to `f7f1a956` and the `0cde7fbc`
  ported-surface drift debt is CLEARED.** The llm-logs D23 re-dump (the
  partition's FIRST — two profile-attribution columns, no new indexes)
  through the 18→20-column write spine with pragma-guarded read
  tolerance + the ruled STRICT-create / TOLERANT-create_for_restore
  split, the six ported call sites (profile ids + measured durations),
  the `getTotalTokenUsageSince` un-zero — **the autonomous daily token
  budget now BINDS on real spend** (mutation-proven case; the fixture
  rework that kept 17 sibling cases meaningful), the UUID-remap
  additions (measurable corpus case), TEN widened hand-rolled DDLs (+
  an ELEVENTH caught by the §3 unification review in the web test
  venue), the `QT_ORACLE_LLM_LOGS` env split, and the `f7f1a956`
  jest-TZ defuse (`jest-zone-globalsetup.cjs` + zone-marked NDJSONs) ∥
  the Almanack PURE half (byte-exact renderer over a 7-case
  mutation-proven differential incl. `toLocaleString` half-expand +
  `locale_date_time_us`; the phase manifest; the `phase` frame kind
  with wire-bytes-unchanged pins) — **the collectors/verbs/host wire
  are HELD on the preserved branch pending their tier-2 differential**
  (the lane's own record; the feature is dark until the resumed lane)
  ∥ the whole Almanack SPA (Providers-tab card + viewer + the shared
  `qt-progress-bar` + both meter migrations + the §1 mirror; §3 review
  restored v4's report typography + documented the card-root
  divergence; `P437_SERVER_LANDED` stays false) ∥ the manifests
  generator repaired (byte-identity proven, RECIPE ROT retired) + the
  Docker `perl-base` purge with the container walk re-run. Escalated:
  `context_summary_service_tier3` + `memory_processor_tier3` oracle
  regen fails v4-side at `f7f1a956` (P4.36 stale-mock class —
  maintenance lane). Gate: see the round record. Versions: core
  0.0.474, harness 0.0.403, host 0.0.59, web 0.0.61, tauri 0.0.6,
  SPA 0.5.412. Round record: `status-log.md`.
- **The Taboo + maintenance round (P4.37-resumed ∥ P4.D50 ∥ P4.40):
  UNIFIED on main (2026-08-06) — ALL THREE CLOSED; the oracle baseline
  MOVES to `3adefeba` and the drift debt is CLEARED.** The Almanack
  server remainder (the held collectors absorbed + oracle-VERIFIED:
  the committed `almanack-{main,mount,llmlogs,llmlogs-legacy}.db`
  family + the 72-check `almanack_tier2_equivalence`, mutation-proven,
  two v5 defects caught by its first runs; `AlmanackHost` wired LIVE
  in `quilltap-host` — **the report is reachable end-to-end in
  production, 💸 none**; the space-form date arm; the walk ACTIVE —
  its first live run caught the `qt-entity-tabs` inline-host bug) ∥
  the whole Taboo feature (`instance_settings['taboo']` storage with
  v4's normalization, the byte-equal `[STYLE: FORBIDDEN PHRASES]`
  section between the math note and tool instructions,
  `PROMPT_CACHE_STRUCTURE_VERSION` 2 → 3 with BOTH v4 goldens
  reproduced, `TabooSettings`/`TabooSettingsUpdate` + the REST edge
  with merge-over-current PUT, the Settings → Chat card in v4's slot;
  `settings_routes_equivalence` 32 → 50, `system_prompt_equivalence`
  56 → 65 + 2 goldens, a `build_context_tier3` op; help docs → the
  `p4.9i2` bank) ∥ the maintenance sweep (the two escalated tier-3
  oracles regenerable again — the cause was NON-UUID corpus profile
  ids v4's `0cde7fbc` Zod refuses, not the predicted stale-mock;
  **`compression_tier3`'s two-round standing red closed by the same
  defect, its owed un-mock order MOOT**; the tracing Interest-cache
  race fixed with nothing weakened; two of the three e2e
  intermittents reproduced-then-hardened, the third honestly
  unreproduced and recorded; the sweep driver gained `--v4 <pin>` +
  the venue false-positive fix — 16-of-27 flagged families were
  already correct). **The §3 unification review fixed, on the unify
  branch: the Taboo `double_option` dispatch-leg bug (an explicit
  `null` silently kept the list where v4 400s — the web edge's
  hand-built variant made the differential blind; serde-pinned) and
  five Almanack fidelity minors** (cheap-LLM user scoping; the four
  route error arms' fixed v4 sentences; integral `size` JSON; the
  `progressId` zod-uuid gate; the registry-membership skip) plus
  recipe repairs the unify regen exposed (the lane-pin purge, the
  settings-routes build stage, the fmc builder's `doc_mount_blobs`).
  Wires: the three cross-lane recipe leftovers; the §1 name-for-name
  diff clean. Gate: 414 test binaries / 1,911 / 0 with the round's
  env block; the 11 families by name zero SKIP over the single
  `7df7de8e` unify pin (the predicted cache-key union hazard did NOT
  materialize); clippy both feature sets; release build; ng test 287
  files / 3,926; ng build clean; full Playwright green zero skips
  (numbers in the round record) — the Almanack walk + the Taboo beat
  both LIVE. Versions: core 0.0.481, harness 0.0.407, host 0.0.60,
  web 0.0.62, SPA 0.5.416; cli/tauri unchanged. Round record:
  `status-log.md`. **💸 Live proofs owed to the next dogfood pass:**
  the Almanack's first real-data report, the live Taboo section on a
  real turn, + the P4.D49 budget/attribution proofs.
- **The fallback + wire + embedding-profiles round (P4.41 ∥ P4.42 ∥
  P4.9H2A ∥ P4.9H2B): UNIFIED on main (2026-08-06) — ALL FOUR CLOSED
  (P4.9H2A tier 2 deferred loudly); the baseline STAYS `3adefeba`.**
  The OpenAI conversation-chaining fallback restored (dogfood #69 —
  a failed chained Responses-API request retries once with full
  input; the wedge is gone; fake-transport quartet + wire-byte pin +
  a tier-3 driving v4's REAL provider with the SDK mocked below it) ∥
  the Serper web-search wire (the assembly carries the PROVIDER and
  the inventory bool derives from `is_some()` — advertised and
  executed cannot disagree; live on chats, Carina, Brahma, Run Tool,
  AND the production enclave; `mock-serper.ts` is the repo's first
  mocked non-LLM external HTTP provider; 💸 the live-key smoke is a
  dogfood item) ∥ embedding-profiles management server-side (eleven
  verbs + REST edges + the P4.d27-banked PUT trigger matrix proven as
  `background_jobs`/`embedding_status` STATE over the new committed
  `embedding-profiles-{main,mount}.db` family, 34-case routes
  differential; the EMBEDDING_REAPPLY_PROFILE handler with
  VACUUM-INTO backups — ⚠ the differential's MOUNT leg is
  sandbox-blind on both sides; the real-instance proof is owed) ∥ the
  SPA (the Embedding Profiles / Memory Deduplication / Regenerate
  Conversation Summaries cards in v4's order, the `p4.9o` Scriptorium
  badge live on both chat-card sites). **Units 6+7 (dedup +
  summaries implementations) refuse loudly by name** — the cards and
  gated beats (`P49H2A_MAINTENANCE_LANDED`) await the follow-up. The
  §3 review caught the PUT echo-null wire defect (fixed +
  corpus-pinned + mutation-proven), the leaked-error 500 arms (all 25
  → v4's fixed sentences), five per-action body-parse divergences,
  and — via the freshly-activated CRUD beat failing its FIRST live
  run — the vintage e2e fixture's missing embedding tables. Gate:
  417 binaries / 1,931 / 0 zero SKIP; nine differentials by name over
  pin-fresh oracles; clippy both sets; release build; ng 292/3,956;
  full Playwright 185 passed / 2 gated skips (the one red is the
  documented wardrobe `set_all` intermittent, green in isolation ×3).
  **⚠ v4 DRIFTED during the round (4 commits past `3adefeba` + a
  dirty tree, all on ported surfaces — several arms are v4 ADOPTING
  this port's queued fixes, so the convergence pins will trip at the
  baseline move by design): the drift catch-up is the top next
  candidate; pin `3adefeba` for every regen until it runs.**
  Versions: core 0.0.486, harness 0.0.411, host 0.0.61, web 0.0.63,
  SPA 0.5.422. Round record: `status-log.md`.
- **The `f4955e0e` found-bugs convergence round (P4.D51 ∥ P4.D52 ∥
  P4.D53 ∥ P4.D54 ∥ P4.D55 ∥ P4.43): UNIFIED on main (2026-08-06) —
  ALL SIX ORDERS CLOSED; the oracle baseline MOVES to `f4955e0e`, the
  drift debt is CLEARED, and P4.9H2A closes WHOLE.** v4's coordinated
  "bugs 8–43" batch (eleven commits; at the new baseline every
  catalogued v4 bug 1–43 is fixed) absorbed end-to-end: ~25
  both-direction convergence pins retired to plain equalities across
  seven families (v4 adopting fixes this port made first — incl. the
  #47 vault clobber, the store-delete cascade, the import
  store-identity trio, the gen-2 restore skip check, the
  sibling-reindex, #67/#68, #45/#46, and the #29/#33/#54 attribution
  set), the four genuine ports landed (interchange sub-chunking
  UTF-16 end-to-end + the chunks-repo embedding-NULL + reconcile arm
  (C); the AllLLMPauseModal + opener; the OpenRouter non-streaming
  vision path + capability-map flip + the Grok/base64/Ollama stream
  fixes; the orphan-thumbnail sweep over a new `StorageBackend` list
  seam), the five-field chat-GET projection + `allowToolUse` reached
  the SPA's controlled selects, impersonation mirrors v4's
  `controlledBy` flips, and P4.43 landed memory-dedup +
  conversation-summaries regeneration LIVE (both beats active). The
  cross-lane `ANNOTATION_SWEEP_PENDING_P4D53` tripwire fired at the
  unified gate exactly as designed and was retired on the evidence.
  **The §3 review's headline catch:** the bug-38 attach path dropped
  v4's `originalFileName` fallback behind a deliberately narrowed
  projection the corpus could not see; also fixed — the thumbnail
  sweep's error shape (v4's CODE throws where its doc-comment claims
  never), a vision `response_format` `name:null` v4 would drop, and
  the retired #67/#68 shim's implicit fixture-shape guards re-pinned
  mechanically. Bug 12's convergence measured PARTIAL — two NEW v4
  restore bugs found and queued (`PHASE_ORDER_RESIDUAL`/
  `V5_STATS_GAP`/`PLANTED_ORPHANS` survive). Gate: 419 test binaries
  / 1,951 / 0; ~53 families fresh at `f4955e0e`, zero SKIP; clippy
  both feature sets; release build; ng 294 files / 4,015; full
  Playwright **189/189 zero skips** (the salon-fixture regen staled a
  transcribed vault-id literal in the Pascal e2e seed — now derived;
  two beats re-gestured to the fixture's new seeds + bug-27
  semantics). **Standing:** the finding-#39 re-ruling was RULED the
  same day (human): the overlay design STANDS and v4's bug-27
  mutate-and-restore is a MISTAKE — the correction is queued
  v4-FIRST (`dogfood-findings.md` #39; ruling record in
  `status-log.md` → "Ruling — the #39 impersonation mechanism"); v5
  stays faithful to the shipped flips until v4 migrates; 💸
  the round's live proofs join the owed dogfood pass (the OpenRouter
  vision send, arm (C)'s boot burst on the Friday copy, the
  dedup/summaries first run). Versions: core 0.0.508, harness
  0.0.431, host 0.0.63, web 0.0.65, SPA 0.5.430. Round record:
  `status-log.md`.
- **The P4.D56 Bug 44 impersonation-overlay drift round: CLOSED,
  UNIFIED on main (2026-08-07, single lane) — the oracle baseline
  MOVES to `62c63dc3` and the drift debt is CLEARED.** v4 implemented
  the #39-ruled overlay (the pre-announced round): impersonation
  never writes `controlledBy` or recompiles identity stacks — the
  new `is_user_driven_seat` helper gates attribution
  (`find_active_user_participant` / the attribution name lookup's
  selected branch / user-identity) and who-responds (selection's
  user_turn reason / the LLM-candidate filter / the chain pause /
  the skipUserTurn gate), answer-confirmation restructured
  truth-table-neutral, and the owner-seat readers (the keep-list —
  half the fix) verified untouched. Stop's profile arm is a
  profile-only reassignment. The impersonation beat re-gestured BACK
  (the Stop button returned to the card; stop driven through the
  UI). Twelve moving + twelve neutrality families fresh at the pin
  (the sweep's six-family SKIP-masquerade re-run manually — the rot
  repair is a named maintenance item). §3 review: no blocking
  findings; one style note (the four-site `impersonating_ids`
  extraction). Gate: 419 binaries / 1,956 / 0; ng 294 / 4,015; full
  Playwright 189/189 zero skips. Versions: core 0.0.509, harness
  0.0.432, SPA 0.5.431. **The owed dogfood pass is now the top next
  candidate** — it gains this round's live surface (a real
  impersonate → pause → stop cycle). Round record: `status-log.md`.
- **The `1bed814f` drift catch-up round (P4.D57 ∥ P4.D58 ∥ P4.D59):
  UNIFIED on main (2026-08-08) — ALL THREE CLOSED; the oracle baseline
  MOVES to `1bed814f` and the drift debt is CLEARED.** v4's
  three-commit day absorbed whole: the Brahma Console agent-turn
  budget as an instance setting (default 25 → 50, bounds 5–200; the
  shared resolver read by BOTH Brahma paths; the
  `brahmaConsoleSettings`/`Update` verbs + REST edge; the 12-case
  settings-routes family with a `>= 12` stale-oracle count guard; both
  brahma tier-3 oracles regenerated with the 50-cap prompt bytes; the
  Settings → Chat card in v4's slot with a LIVE round-trip beat —
  ACTIVATE-AT-UNIFY flipped) ∥ the salon impersonation reconcile
  (dogfood **#71/#72 CLOSED** — the client
  `isUserDrivenSeat`/`findActiveUserParticipant` twins with parity
  specs, the turn banner re-diverged onto the overlay [what P4.D56
  reverted, back WITH its v4-client oracle], the optimistic-bubble
  attribution fix, the `SpeakingAsAvatar` composer cue; the
  turn-banner half proven at unit-spec level per the weighted-random
  e2e limitation) ∥ the About-backdrop NO-PORT (v5 ships no About
  background asset — recorded in `m6-screen-parity.md` §1.4). One
  recorded D57 deviation: the update field carries
  `Option<Option<Value>>` so present-but-invalid values 400 at the
  handler instead of collapsing at the web edge (the Taboo §3 lesson,
  prevented by design). §3 review: no blocking findings (one
  fixture-vintage comment contradiction fixed at the wire). Gate: 419
  test binaries / 1,970 / 0 (the three families by name zero SKIP over
  fresh `1bed814f` oracles), clippy both feature sets, release build,
  ng 296 files / 4,046, full Playwright 190/190 zero skips (the suite
  grew 189 → 190 with the activated brahma-console beat). Versions:
  core 0.0.512, harness 0.0.434, web 0.0.66, SPA 0.5.436. **The owed
  dogfood pass remains the top next candidate** — it gains this
  round's surface (the impersonated seat's banner + Skip, the
  speaking-as portrait, a raised Brahma budget on a real deep query).
  Round record: `status-log.md`.
- **The `f6eac168` drift catch-up round (P4.D60 ∥ P4.D61 ∥ P4.44):
  UNIFIED on main (2026-08-08) — ALL THREE CLOSED; the oracle baseline
  MOVES to `f6eac168` and the drift debt is CLEARED.** v4's Bugs 47–51
  (filed from this port's own dogfood walk) absorbed whole: the
  fair-rotation first-responder pause (`select_next_speaker_after_user_
  message` + the spine guard; Carina markup deferred loud at BOTH
  `user_message_carina` sites), the byte-exact Brahma budget-exhaustion
  salvage in both paths (runtime budget override — committed fixtures
  untouched), the chat-GET impersonation projection + the five-copy
  `impersonating_ids` consolidation ∥ the SPA client half:
  impersonate-takes-the-turn as a `turnOverride` layered above v5's
  server-authoritative turn (documented mechanism divergence), the
  latch-keyed speaking-as turn-follow, the seed-once `impersonationSync`
  port, the reload beat ACTIVATE-AT-UNIFY flipped live ∥ P4.44's three
  standing debts: the chunks upsert CREATE arm (minted-id normalizer),
  per-delete `cleanup_thumbnails` over `StorageBackend` (bug 43 tier 2
  CLOSED; chat-media twins verified un-wired in v4 itself), the provider
  request-header pin (post-`apply_auth` subset + 8-provider coverage
  floor; abort-arming deferred loud, unit-tier-proven). **The §3 review
  caught one would-have-shipped defect:** the seed-once parity spec was
  a FALSE GREEN (TanStack structural sharing kept the deep-equal stub's
  reference so the sync effect never re-fired) — repaired + mutation-
  proven both directions. Gate: 419 test binaries / 1,978 / 0 with the
  round's env block; the seven differentials by name zero SKIP over
  fresh `f6eac168` oracles (request-envelopes corpus byte-identical);
  clippy both feature sets; release build; ng 296 files / 4,065; full
  Playwright green (numbers in the round record). Versions: core
  0.0.518, harness 0.0.440, SPA 0.5.444. **The owed dogfood pass remains
  the top next candidate** — it gains this round's surfaces (the
  two-user-seat rotation pause, the Brahma salvage on a low budget,
  impersonate → reload). Round record: `status-log.md`.
- **The character-archive drift catch-up, ROUND 1 of 2 (P4.D62 ∥ P4.D63 ∥
  P4.D64): UNIFIED on main (2026-08-11) — P4.D62/P4.D64 CLOSED, P4.D63
  OPEN at unit 7 only; the oracle baseline MOVES to `d553f72a`.** v4's
  character-archive feature (`01e481f6` + Bugs 52/54/55) absorbed as far
  as the substrate: the whole `.qtap` preserveIds machinery
  (vault-carrying character exports + carried row ids, the 16-kind
  preflight with refuse-on-collision + rehydrate-only skip-if-present,
  the Bug-52 avatar remap, Bug-54 sha256 dedup, Bug-55 typed 404s) ∥ the
  three archive columns (D23 re-dump + boot ensure + per-column read
  tolerance), the write guard + the API-layer `archived=` chokepoint +
  every turn/tool/mail refusal arm, the byte-exact bundle crypto (17-arm
  tier-1) + the engine-held runtime passphrase cache, the wipe/restore
  spare-bundle options, and `characterArchive`/`characterRehydrate`
  DEFINED refusal-armed ∥ the whole SPA surface with six tombstone-read
  beats LIVE over a seeded archived island and four action beats gated
  for round 2. **The §3 review fixed six findings pre-merge** (headline:
  the one-default embedding rule had leaked into help-doc sync, where v4
  keeps the first-profile fallback), and the beats' first live runs
  found **a v4-side bug to file upstream** (the archived-seat sidebar
  badge cannot light on a fresh load in v4 — the chat GET's enrichment
  never got `archivedAt`; v5 reproduces faithfully, pinned by the beat).
  Gate: 421 test binaries / 1,997 / 0; 25 families regenerated fresh at
  the `d553f72a` pin and re-run by name; clippy both feature sets;
  release build; ng 298 files / 4,138; full Playwright green (numbers in
  the round record). **Round 2** (service + verbs + CLI + gate flips) and
  **the `ed8934f1` Bug-56 drift catch-up** are the top next candidates —
  see phase-4.md. Versions: core 0.0.522, harness 0.0.443, web 0.0.68,
  host 0.0.65, SPA 0.5.450.
- **The character-archive ROUND 2 + Bug-56 round (P4.D65 ∥ P4.D66 ∥
  P4.D67): UNIFIED on main (2026-08-11) — P4.D66/P4.D67 CLOSED, P4.D65
  OPEN at its resume list (P4.D63 stays OPEN at unit 7 with it); the
  oracle baseline MOVES to `ed8934f1` and the drift debt is CLEARED.**
  The archive service LIVE end-to-end (the 889-line port, both verbs,
  the 8-case differential over the new committed
  `character-archive-{main,mount}.db` family; the four SPA action beats
  ACTIVE — archive/rehydrate walk live) ∥ the whole CLI `db characters`
  family (status/archives/archive/rehydrate/export incl. offline bundle
  decrypt; Tier R 136 → 188/0 vs v4's REAL launcher; the db verb
  entrance v5 never had) ∥ the Bug-56 base-path-availability port
  (byte-exact diagnosis sentences, the folder-create
  assert-before-recursive-mkdir + 409, the store-create warning rewrite;
  both mount families regenerated fresh). **The §3 review caught the
  round's would-have-shipped bug — the cross-lane blind spot:** no lane
  served the CLI's `POST /api/v1/characters/{id}?action=` URL on v5's
  server (D65 reasoned from the SPA, D66's Tier R stubbed the wire) —
  the thin REST edge landed at unification, and its live wire test then
  caught two more: missing-character 500-vs-404 (fixed at v4's route
  placement) and **a v4 bug v5 reproduced faithfully — v4 cannot
  rehydrate a vault linking the same bytes twice** (per-link blob export
  duplication × the undeduped `carriedBlobIds`); v5's preflight now
  dedupes first-occurrence (CONFIRMED by the human 2026-08-11; filed as
  v4 Bug 57, to be fixed v4-side). Also fixed:
  the archive differential's `background_jobs` blindness (fixture
  extended by mutation — the table never existed, so enqueues failed
  soft on BOTH sides; the positive leg stays owed), four CLI
  swallowed-SQL-error sites, the Tier R wire-parity assertion, v4's
  no-backend sentence, and the two action beats' `?section=` gesture
  (the workspace-hosted settings page ignores it exactly as v4 does).
  Gate: 423 test binaries / 2,010 / 0 with the round's env block; the
  round's differentials by name fresh at `ed8934f1` zero SKIP; clippy
  both feature sets; release build; ng 4,138; full Playwright green with
  the archive spec 10/10 (numbers in the round record). Versions: core
  0.0.526, harness 0.0.446, cli 0.0.8, web 0.0.69, SPA 0.5.451. Next
  candidates: finish P4.D65, the owed dogfood pass (now with the live
  archive/CLI surfaces), the two v4-side filings, the sweep-rot pass —
  see phase-4.md.
- **The P4.D65-finish + sweep-rot round (P4.D65-resumed ∥ P4.45): UNIFIED
  on main (2026-08-11) — P4.D63 and P4.45 CLOSED; P4.D65 OPEN at items 5–6
  only; the oracle baseline MOVES to `de9f70bf` and the drift debt is
  CLEARED.** v4 fixed its Bug 57 at `de9f70bf` (converging onto this
  port's twice-linked-blob rehydrate dedupe — zero v5 source change
  needed); the divergence pins retired to plain equalities and the archive
  fixture grew the twice-linked shape as a mutation-proven equality arm
  covering BOTH post-dedupe legs. The D63 unit-7 re-encrypt wire is LIVE:
  a passphrase change now re-encrypts the archive library (the sweep at
  the ChangePassphrase dispatch arm — made `async` after its differential
  caught a real `write_blocking` panic; `{success, archives}` on the wire
  the P4.D64 settings card already reads; 6-case
  `archive_reencrypt_tier2_equivalence` + a live web wire test that
  archives → changes → rehydrates). Also live: the `ARCHIVE_BUNDLE_HELD`
  files-delete guard (three arms incl. the unheld leg) and the export
  picker's archived filter; the non-null export-carry arm caught a STALE
  `schema-key-order.json` (the three archive keys were being appended at
  the END of every exported character record — regenerated with the
  shipped generator). P4.45 repaired the sweep driver at the root
  (recipes classified by INDENTATION, not first-word guessing; 32 run
  lines scoped; unattributable runs refused; the jest-side stale-oracle
  deletion hole closed; the 39-family `--run-all` proof committed) — the
  driver is now the sanctioned regen path. **The §3 review fixed two
  findings before merge:** the sweep's upload-failure `reason` leaked the
  bare backend error where v4 wraps via `uploadRaw` (a contractual UI
  string on a surface going live this round — routed through the existing
  `upload_raw` helper, pinned by a failing-upload unit test), and the
  holder-lookup error arm leaked raw `DbError` text where v4 answers the
  fixed `Failed to delete file` 500. Gate: 425 test binaries / 2,013 / 0
  with the round's env block; the 20 affected families fresh at
  `de9f70bf` by name through the driver, zero SKIP; clippy both feature
  sets; release build; ng 298 files / 4,138; full Playwright **202/202
  zero skips**. Versions: core 0.0.529, harness 0.0.452, host 0.0.66,
  web 0.0.70. **The owed dogfood pass is the top next candidate** — it
  gains the live re-encryption sweep + held-bundle guard. Round record:
  `status-log.md`.
- **The `03154b72` 4.8.1-release drift catch-up round (P4.D68 ∥ P4.D69 ∥
  P4.D70 + the wardrobe deflake): UNIFIED on main (2026-08-12) — ALL THREE
  CLOSED; the oracle baseline MOVES to `03154b72` and the drift debt is
  CLEARED.** v4 released 4.8.0 + 4.8.1 (main now `4.9.0-dev.0`; the
  effective lib/app drift was its bugs 58–60 + CLI completions + two
  client fixes). The bug-60 port: `change_passphrase` sheds the phantom
  `quilltap-llm-logs.dbkey` (v5 reproduced the write faithfully until
  now), proven by the dbkey cross-compat oracle grown to BOTH directions
  (v4's REAL `changePassphrase` drives the v4→v5 leg; one-file assertions
  on both sides, mutation-proven cross-side tripwires) ∥ bug 59 MEASURED
  as structural convergence (v5's seed gate already fails closed; a new
  `failed_gate_probe_seeds_nothing` pin) ∥ bug 58 NO-PORT (no migration
  runner) with the full writable-open lock enumeration — **which found
  the round's one standing item: v5's boot opens all three partitions
  writable BEFORE the instance lock is acquired** (unlocked
  `journal_mode = TRUNCATE` header writes in the contended case; the
  exact class bug 58 closes; needs its own small order — see phase-4.md
  candidates and the P4.D68 order header) ∥ the repo-wide spelling sweep
  (`harness/tools/check_spelling.py` + the harness `spelling_guard`
  test — the standing rule finally has mechanical enforcement) ∥ the
  `db characters` completion templates byte-copied (Tier R red-first
  3-by-name → 188/0) ∥ the standalone streaming indicator above a tool
  block + the About release-freshness mirror (spec-pinned, v4-client
  oracle). Riding the round: the `wardrobe-flow` `set_all` beat — the
  suite's longest-standing intermittent — deflaked spec-only (seed a
  worn accessory; an EMPTY snapshot cannot be waited on), with the
  underlying lost-edit race measured (3 ms margin), kept v4-faithful,
  and **filed upstream as v4 Bug 61** (dogfood finding #78). ⚠ v4 now
  develops on TWO branches (main = 4.9-dev, `bugfix` = 4.8.x) and the
  checkout sat on `bugfix` at planning — drift-check BOTH and verify the
  checkout's branch before any regen. Gate: numbers in the round record
  (`status-log.md`). Versions: core 0.0.531, harness 0.0.454, cli 0.0.9,
  SPA 0.5.454; host/web/tauri unchanged.
- **Oracle baseline: `03154b72` (2026-08-12, v4 main HEAD — "merge: 4.8.1
  back into main", version `4.9.0-dev.0`), adopted at the 4.8.1-release
  drift-round unification — NO v4 drift debt remains.** v4 released 4.8.0
  and 4.8.1 and now develops on TWO branches: `main` (4.9-dev) and
  `bugfix` (4.8.x maintenance; release content reaches main squashed via
  the `release:`/`merge:` pair, so measure drift with `git diff
  <baseline> main`, not the bugfix commit list). **Drift-check BOTH
  branches every round** (`git log <baseline>..main` AND `git log
  main..bugfix -- lib/ app/ packages/`) and verify the checkout's branch
  (`git branch --show-current`) before any regen — pin a detached
  worktree on any mismatch, drift, or dirty tree
  (`oracle-regen-pinned-v4-worktree`, or `recipe_sweep.py --v4 <pin>`).
  At this round's gate the checkout was back on main at the baseline with
  two dirty files — the v4 Bug-61 filing (`docs/developer/bugs/*`), this
  round's own upstream filing, docs-only and outside every oracle import
  graph. The sweep driver remains the sanctioned per-family regen path;
  the distill-transitive TZ pins, the committed-fixture rule, and the
  venue/staging rules stand unchanged.
  (The superseded baseline paragraphs formerly kept here "for history" are
  archived verbatim in `docs/developer/porting/claude-md-status-history.md`.)
- **Standing deferrals + gotchas:** tracked in the work orders, the
  status log, and the memory notes — not here.
