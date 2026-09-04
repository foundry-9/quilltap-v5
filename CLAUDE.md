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
- **The 4.8.2/4.8.3 drift catch-up + lock-order round (P4.D71 ∥ P4.D72 ∥
  P4.D73 ∥ P4.D74 ∥ P4.D75 ∥ P4.46 ∥ P4.D76): UNIFIED on main
  (2026-08-14) — ALL SEVEN CLOSED; the oracle baseline MOVES to
  `48396682` and the drift debt is CLEARED** (v4 HEAD `11553944` = the
  4.8.4 release, tests+docs only, NO-PORT). The group wardrobe tiers +
  bundle dissolution end-to-end (precedence character > group > project >
  general; leaf-id persistence on every wear path; `?scope=group` +
  transfers out of a group; **dogfood finding #78 CLOSED** — v4's Bug-61
  fix ported into the dialog with a deterministic race beat) ∥ the three
  composer features whole (smart typography with render-time quotes +
  type-time dashes; the `:`/`\` typeaheads + pickers over v4's
  code-identical engines and byte-copied corpora/datasets; bugs 62 + 63
  fixed — v5 had reproduced both) ∥ the three `chat_settings` columns
  (D23 re-dump + boot ensure + Zod-exact PUT arms incl. v4's whole
  `ZodError.message` 400 bodies) ∥ **the P4.D68 escalation DISCHARGED**
  (lock before ANY partition open on boot/unlock/setup, WAL-parked
  contended proofs; setup hardening — pepper never withheld, destructive
  retry refused; `.dbkey` unknown-field preservation with a v4-drop
  divergence pin) ∥ the SDK wire re-check (openai 7.4 / openrouter 1.2.32
  neutral outside self-dating markers). **The §3 review caught two
  would-have-shipped bugs, both fixed + pinned at unification:** the
  empty typeahead menu swallowed Enter/Tab/arrows (v4 falls through — a
  typo'd `:smiel` + Enter would not send), and first-run Setup died on a
  missing `data/` dir (the lock reorder outran the dir creation; every
  test had masked it by pre-creating `data/`). Gate + versions: the round
  record in `status-log.md`. Deferred loud: help docs → `p4.9i2`; the
  outfit-preview witness; the new-chat manual-compose client family; the
  google-wire recorded-not-asserted headers; the `p4.9l` composer
  toolbar (the pickers' composer entrance).
  **The owed dogfood pass RAN on the Friday copy (2026-08-14) — 38 steps,
  six parts, findings #79–#86.** Two real composer defects found and fixed
  on main, both by gestures neither suite makes: **#82** a fenced code
  block was a one-way door (v4's Enter-escape had no v5 counterpart), and
  **#84 ∥ #85** typed backslashes doubled on the wire (v4's Lexical export
  never escapes `\`; v5's serializer does) and the typeahead arrows died
  under a resting mouse pointer (v5 rebuilt every row where v4's React
  reconciles, so Chromium re-fired `mouseenter`). Five more reports
  diagnosed v4-faithful or as named deferrals (#79/#80/#81/#83/#86 — one
  of them a v5-invented banner string reworded). **Live proofs
  discharged:** the whole group-wardrobe tier surface incl. dissolution and
  the bug-61 race dialog, and the passphrase chain end to end — the bug-60
  one-`.dbkey` proof plus a rehydrate from a bundle sealed under the OLD
  passphrase, which proves the P4.D63 unit-7 re-encryption sweep. Still
  owed: Part C step 12 and the whole standing 💸 queue (Almanack, Taboo,
  OpenRouter pricing, the vision send, P4.D49). Record: `status-log.md` →
  "Dogfood pass — the 4.8.2/4.8.3 round"; rows in `dogfood-findings.md`.
- **The help-drift round (P4.D77 ∥ P4.D65-remainder ∥ P4.47 ∥ P4.9L):
  UNIFIED on main (2026-08-14) — ALL FOUR CLOSED; the oracle baseline
  MOVES to `24633026` and the drift debt is CLEARED.** v4's one drift
  commit (section-level help embeddings + Guide content search) absorbed
  across every ported surface: the `help_doc_chunks` substrate (the D23
  re-dump's generateDDL shape AND the migration shape via the boot
  ensure — the order's "they agree" premise REFUTED, both shapes
  deliberate; `chunk_index` is `f64` after the REAL-affinity defect the
  differential caught), the chunking over the Scriptorium chunker, the
  sync re-slice + upgrade backfill (one recorded transaction-shape
  divergence), the HELP_DOC job's chunk pass (three vacuous-green corpus
  masks found and fixed by mutation), the reindex/reapply riders, and
  `help_search`'s `max(doc, best-section)` blend + section-led tool
  block — the Guide client half banked verbatim at `p4.9i2` ∥ the D65
  items 5–6 coverage remainder (a STALE ORACLE MOCK found by consequence
  — jest.setup's `getDefaultEmbeddingProfile` null stub was starving
  v4's import of embedding jobs; item 5a ESCALATED: the preflight
  swallows read errors at ten sites, ordered next) ∥ P4.47's three
  smalls, one upgraded to a REAL FIX (v5 sent google's api key as
  `?key=` where v4's SDK sends `X-Goog-Api-Key` — three-way confirmed,
  both composition-level unit tests flipped) ∥ P4.9L: the composer
  formatting toolbar against a NEW v4-side jsdom recorder (56 vectors),
  v4's 2-column composer layout (**dogfood #75 CLOSED**, band-aid
  retired), the pickers' composer entrance, five live beats (suite 215
  → 219); v4's source-mode send discarding edits is a NEW v4-side
  filing. **The §3 review fixed at unification:** the settings
  error-status split (v4's `includes('Invalid') ? 400 : 500` — a
  threshold-only Zod failure answers 500) + the connection-profile
  duplicate arms' 409, both caught by the review's NEW per-row
  error-status assert (mutation-proven red-first); the Generate-Image
  disabled gate; the list-shape false-equivalence claim (now a recorded,
  spec-pinned divergence); four comment corrections. **The gate itself
  caught two sweep-driver defects:** the fixture shield dropped
  `.db.meta.json` sidecars, and the driver ran a committed-corpus
  family's RECORDING stage — clobbering the google-wire corpus against
  the pinned worktree (restored; the driver now never runs recording
  stages and warns on tracked-fixture writes). Gate + versions: the
  round record in `status-log.md`. 💸 owed: P4.D77's trio + #75's
  acceptance look, on the standing queue.
- **The `aa464abf` drift catch-up round (P4.D78 ∥ P4.D79 ∥ P4.D80 ∥
  P4.D81 ∥ P4.48): UNIFIED on main (2026-08-15) — ALL FIVE CLOSED; the
  oracle baseline MOVES to `aa464abf`.** Six v4 commits absorbed. The
  Ollama-thinking wire whole (the stateful `<think>` parser chop-exact vs
  v4's real parser; dual-channel reasoning on stream + non-stream; `think`
  ALWAYS on the body + `options.num_ctx`; the retry-without-think in BOTH
  compositions — the order's fourth quartet arm was WRONG about v4, the
  guard is key PRESENCE; the `toolUse` manifest flip via regen, which
  exposed and fixed the generator's augmentation table reverting P4.47(B)'s
  google header) ∥ bug 68 end-to-end (`multiCharacterPrefill` through the
  D23 re-dump — generateDDL and migration DDL DISAGREE again, both shapes
  carried; the once-only backfill boot ensure; the resolver + per-profile
  turn anchor; routes with the double-legged 400; export/import carry —
  the ownership tripwire FIRED and the ordered edit landed at unification;
  greeting reasoning persisted onto the first message) + the
  `profileParams` consolidation, which fixed THREE pre-existing v5
  defects: **the Salon primary stream had NO modelParams twin at all**
  (every per-model setting silently dropped on the main chat path), the
  Carina temperature read a nonexistent key, and **the SPA profile save
  silently dropped every non-sampling `parameters` key** ∥ bugs 66/69
  (the archivedAt enrichment + the beat FLIPPED live; the rehydrate
  digest self-heal for rows v4's pre-4.9 watcher damaged; bug 67 a pure
  convergence — v4 adopted v5's pinned source-mode send) ∥ P4.48 with its
  premise REFUTED twice over: v4 SWALLOWS those read errors too
  (`safeQuery` fallback mode), so the overlay leg landed as the
  byte-match fix and the DB-read-error refusal as a ruled divergence
  (both-directions tripwires); 23 sites, not 10. **The §3 review found NO
  blocking findings** (a first) — fixed anyway: the rerouted-profile
  fallback made loud, three comment corrections; the gate caught three
  recipe defects (a literal `npx jest ...` placeholder; a
  non-self-contained primary_stream recipe; two stale lane-pin paths).
  Gate: 435 test binaries / 2,125 / 0 zero SKIP; the 28 affected families
  fresh at the pin through the sweep driver; clippy both sets; release
  build; ng 324 files / 4,741; full Playwright green (numbers in the
  round record). Versions: core 0.0.564, harness 0.0.487, host 0.0.71,
  SPA 0.5.493. 💸 the live Ollama-thinking proof + the round's surfaces
  join the owed dogfood queue. Round record: `status-log.md`.
- **The `93ed8abf` drift round (P4.D82 → P4.D83 stacked ∥ P4.D84):
  UNIFIED on main (2026-08-16) — ALL THREE CLOSED; the oracle baseline
  MOVES to `93ed8abf` and the drift debt is CLEARED.** v4's three-commit
  day absorbed whole: bug 70 end-to-end (`resolveContextWindow`
  single-sourcing the window profile-first, `computeSafeInputLimit` with
  `ContextBudget` carrying `safeInputLimit`/`safetyMargin`, the
  green-field `turn_extras` accounting reserving room for tool schemas +
  splices BEFORE the context — and a pre-existing v5 deferral closed on
  the way: the tool-change notice now splices and `forceToolsOnNextMessage`
  clears) ∥ the sampling resolver at all FIVE call sites (the corpus found
  the Carina fifth; two typed seams closed — `CompletionParams` gained
  `top_p`, `max_tokens` became `Option` so absent stopped meaning 0; four
  tier-3 fixtures gained mixed-spelling bags after two were found
  measuring NOTHING), the profile-parameters wire (Ollama options table +
  keep_alive numeric sentinels + thinking-effort levels; OAC allow-list +
  `chat_template_kwargs` fold + tools on BOTH paths; DeepSeek/Z.AI
  converged onto the one applier with a pre-existing Z.AI effort-gate bug
  fixed; envelope corpus 191 → 257, the recorder's wrong-class catch),
  the per-profile Ollama request timeout (streaming first-byte /
  non-streaming whole-request, caller budget still wins, stalling-socket
  proofs), and optionsSchema on all eight declaring manifests via the
  generator ∥ the SPA's schema-driven provider-options panel (all five
  field types + showIf + multi-enum landed whole — the Tier-3 deferral
  condition was FALSE; the hardcoded Enable Thinking row DELETED with the
  P4.D81 divergence retired; the tool-use seed hint; the
  `supportsImageUpload` re-seed over the transcribed client attachment
  table). **The §3 review fixed five real findings before merge** —
  headline: the OAC `chat_template_kwargs` ARRAY-string omission
  (corpus-blind and mis-documented on both sides; now corpus-pinned) and
  the half-ported pre-send validation (v4's client-facing
  `validating`/`warning` statuses now emitted and sequence-compared);
  also the danger-reroute budget reading the ORIGINAL profile's window
  (fixed + mutation-proven via the danger fixture's differing window),
  the turn-extras estimator's flat 3.5 (now the provider's registry
  rate, GOOGLE-row pinned), and the unported OAC non-streaming
  `tool_calls` parse-back (landed with v4's own three-arm filter).
  Gate: 437 test binaries / 2,147 / 0 with the round's 53-variable env
  block, zero SKIP; the 26 affected families fresh at the pin through
  the sweep driver; clippy both feature sets; release build; ng 326
  files / 4,792; full Playwright green with the options round-trip
  beat's first activation (numbers in the round record). Versions: core
  0.0.575, harness 0.0.499, host 0.0.72, SPA 0.5.498. **💸 the round's
  live proofs: THREE DISCHARGED on the 2026-08-16 walk** (Max Tokens and
  Top P on a local wire + the Keep Alive sentinels, read through the new
  `harness/tools/wire-tap.py`; the request timeout on a cold model; the
  options panel on real data) — **OAC tools against llama-server remains
  OWED**, the one arm of v4 bug 71 never run against a real server on
  either side. That walk is PAUSED after Part B (findings #87/#88, both
  filed as v4 bugs 72/73); Parts C and D are the next pass.
  Riders carried: external-prompt-generator (D82) + `encodeDebugInfo`
  (D83) — their future lanes carry the drift edits. Round record:
  `status-log.md`.
- **The `d123658d` connection-profile-editor drift round (P4.D85 ∥
  P4.D86): UNIFIED on main (2026-08-17) — BOTH CLOSED; the oracle
  baseline MOVES to `d123658d` and the drift debt is CLEARED** (v4's one
  newer commit `9c01fa99` = the MODERN sample-prompt trio, plugin/help
  content only, NO-PORT with evidence). v4's fix for bugs 72/73 — this
  port's own dogfood findings #87/#88 coming back — plus bug 74 (profile
  tagging had never worked) absorbed whole. Server: the
  `resolve_editor_tags` flat resolver with BOTH `get-tags` call sites
  through it (characters convergence proven output-neutral), the three
  profile-tag verbs with v4's exact bodies, settings-routes 108 → 128
  cases over a fixture that finally carries tags (unsorted bag + dangling
  id + stale baseUrl; the three v4 action-gate arms RECORDED-ONLY with an
  exact-count guard), and **a real v5 divergence fixed**: cleared PUT
  keys answer as explicit `null` in schema position as v4's
  in-memory-merge does (`restore_cleared_nulls`, mutation-proven; the
  class is an open LEAD on other update surfaces). The
  `enrich_with_tags` `{id,name}` narrowing closed (the vacuous-corpus
  class — its own doc comment named the excuse). SPA: the
  `ProviderNumberField` draft/`syncedFrom` machinery with
  default-as-placeholder (the naive re-sync spelling mutation-pinned;
  the P4.D84 recorded number-clear divergence re-measured and RETIRED),
  the `outboundBaseUrl` chokepoint over v5's own FIVE sites with the
  always-send save body, the profile tag surface in its fixed form
  (immediate persistence, v4's toast sentences), the banked
  `Non-image attachments:` line, and v4's own verification walk as three
  e2e beats — the tag beat activated at unification, green on its first
  run. **The §3 review's catch (the cross-lane staleness class):**
  P4.D86's `EnrichedProfileTag` documented a narrowing P4.D85 closed in
  the same round — retyped to the full `TagDto` at the wire.
  `auto-configure` ratified UNPORTED (no action surface to refuse from;
  the sentence pinned by a recorded row). Gate: numbers in the round
  record. Versions: core 0.0.577, harness 0.0.501, SPA 0.5.505;
  host/web/cli/tauri unchanged. **💸 the dogfood queue gains:** profile
  tags end-to-end, the cleared-number heal, the poisoned-base-URL heal
  on real pre-bug-73 rows. **The owed dogfood pass (Parts C/D + the
  standing 💸 queue) remains the top next candidate.** Round record:
  `status-log.md`.
- **The `979652a9` drift round (P4.D87 ∥ P4.D88 ∥ P4.D89 ∥ P4.D90 ∥
  P4.49): UNIFIED on main (2026-08-18) — ALL FIVE CLOSED; the oracle
  baseline MOVES to `979652a9` and the drift debt is CLEARED.** v4's
  eight-commit day absorbed whole plus the long-owed file logging. The
  hair slot end-to-end (a FIFTH wardrobe slot — a hairdo, not hair —
  through ONE slot-meta registry replacing v5's ten hard-coded copies
  server-side and eleven SPA sites; `reportWhenEmpty` at every narration
  site; nudity over clothing slots only; the avatar `accessories || hair`
  guard on both branches; byte-exact prompts + tool definitions; the
  accepted one-miss hash invalidation; import/export/restore carry; the
  NEW `outfit_hash_equivalence` family; the rose badge + Green Room
  preview + the live wardrobe beat) ∥ Bug 75 (the `.qtap` composite
  `componentItemIds` leaf-first remap — v5 measurably HAD the bug;
  relationship-token differential over the new committed
  `qtap-import-bug75.qtap`) ∥ bug 76 (the `outboundApiKeyId` chokepoint
  over v5's FIVE outbound sites, always-send `|| null` heal, v4's 7-case
  suite mirrored 1:1 — **dogfood finding #90 CLOSED**) + bug 77 (the
  tool-execution notice landed as v4's surface in its FIXED form — v5 had
  never ported it; single-door publish, self-owned 6 s lifetime, close
  button; the order's retire-the-toasts premise REFUTED: they are v4's
  own, kept alongside) ∥ the workspace tab re-activation refresh (the
  visibility token + `onTabActivated` with the v5-`enabled` latch, the
  kind→prefix map over v5's fragmented keys with split spellings swept
  BOTH ways, silent hand-rolled reloads, v4's 8 parity assertions, two
  live e2e beats) ∥ P4.49 file logging LIVE (`combined.log`/`error.log`
  + rotation + the iCloud/Finder stray sweep; the ruled `both` default
  with recorded expiry; the CLI ruled a non-port by measurement — v4's
  CLI only READS logs; 33 cases, eleven mutations). **The §3 review: NO
  blocking findings; its catch — the bug-77 turn-end WIRING was unpinned
  (specs drove the private method; the production call could vanish
  unseen) — fixed + mutation-proven at unification.** A REAL v4 bug found
  and pinned both directions with a convergence tripwire, TO FILE
  UPSTREAM: v4 at `979652a9` crashes avatar generation on any pre-hair
  four-key `equippedOutfit` row with items equipped. Gate: 34/34 families
  fresh at the pin zero SKIP; 439 test binaries / 2,205 / 0; clippy both
  feature sets; release build; ng 331 files / 4,898; full Playwright
  **228/228 zero skips**. Versions: core 0.0.581, harness 0.0.503, web
  0.0.76, cli 0.0.10, tauri 0.0.7, SPA 0.5.514. **💸 the dogfood queue
  gains:** the `combined.log` acceptance grep, the poisoned-key heal, the
  notice lifecycle, the worn hairdo + rose badge + avatar regen, tab
  re-activation freshness. **The owed dogfood pass (Parts C/D + the
  standing 💸 queue) remains the top next candidate.** Round record:
  `status-log.md`.
- **The `c6ff8051` drift catch-up round (P4.D91 ∥ P4.D92): UNIFIED on main
  (2026-08-19) — BOTH CLOSED; the oracle baseline MOVES to `c6ff8051` and
  the drift debt is CLEARED.** v4's bugs-78/79 fix (converging onto this
  port's own filings) absorbed: the bug-78 tripwire fired and retired to a
  plain equality (v5 was never affected — its slot reader always defaulted
  a missing key; the coercion moved to the repository so `get/set` and the
  tool handler share ONE home, with `Object.entries`-faithful non-object
  shapes), the five silent import arms + the preflight refusal now push
  v4's exact sentences (measured via a built-to-fail five-item corpus case
  — no committed archive can express an import failure), the
  unvalidatable-row plant is a RECORDED warnings-only divergence (v4 names
  the validation failure, v5 the collision; neither writes), and a P4.48
  finding was CORRECTED by measurement (the "swallowed read" was really
  v4's `ensureCollection` rebuilding the dropped table — now a `plantProbe`
  comparand). Bug 80 landed as the WIDER v5 gap it exposed: the project
  story background had never been wired (dead client resolver reading a
  key the wire never carries), now reported to the workspace backdrop in
  v4's fixed one-reporter shape + the legacy per-view layer for the routed
  path; the `'theme'` subsystem fallback stays under the standing
  no-subsystem-background divergence, now naming bug 80's arm. **The §3
  review's catch:** the warning arms' quoted name is a JS template-literal
  interpolation in v4 — v5 rendered non-string names empty; one shared
  helper over `to_js_string` now carries it, unit-pinned. **The gate's own
  incident: the v4 checkout was switched to the `release` branch (4.8.4,
  2026-08-13 content) MID-UNIFY — the first, unpinned regen went red on two
  families and was discarded; the pinned-worktree re-run is the gate of
  record, and until the checkout returns to main EVERY regen needs the
  pin.** The e2e beat's first run also proved SQL-seeding a store-overlay
  property is invisible — seed through the API/UI. Gate: 439 test binaries
  / 2,208 / 0 with the round's env block; the four affected families fresh
  from the pinned worktree, re-run by name zero SKIP (the workspace-suite
  copies of two of them SKIP silently without their `QT_FIXTURE_*` vars —
  0.00 s is the tell); clippy both feature sets; release build; ng 331
  files / 4,908; full Playwright **229/229 zero skips** (the suite grew
  with the backdrop beat). Versions: core 0.0.584, harness 0.0.505, SPA
  0.5.518; host/web/cli/tauri unchanged. **💸 the dogfood queue gains:**
  the bug-78 read-repair on Friday-vintage rows, a failed import naming
  its dropped items, the project backdrop on real data. **That pass RAN on
  2026-08-19** — see the dogfood bullet below. Round record:
  `status-log.md`.
- **The owed dogfood pass RAN (2026-08-19, agent-driven, on the Friday
  copy) — two v5 findings, TWO v4 filings, nine live proofs discharged, and
  one 💸 item retired as unmeasurable.** Walk doc:
  `docs/developer/porting/dogfood-walks/2026-08-19-owed-pass.md`; record in
  `status-log.md`. **Discharged:** P4.49 file logging (both halves — v4's
  JSON line shape, all three stray shapes swept, protected families
  intact), the arm-(C) embedding burst, LLM-log retention (237 rows), the
  bug-78 legacy-outfit read repair (on rows *with items equipped* — the
  exact shape v4 crashes on), the hair slot end to end incl. a regenerated
  portrait carrying it, the live Taboo section (16 real phrases, after the
  math note), tri-tier dressing, P4.D49 durations + attribution, the
  project backdrop (verified by computed URL), tab re-activation freshness,
  and P4.41's chaining fallback. **v4 bug 71's OAC arm advanced as far as
  it can go:** the request half is PROVEN at the byte level (wire-tap shows
  a native `tools[]` array, settling what `pseudoToolMode: "auto"`
  resolves to); the parse-back stays unexercised because no local model
  returned `tool_calls` — the loop end to end IS proven on DeepSeek.
  **FILED UPSTREAM: v4 bug 82** (three leading system messages break strict
  local chat templates — v5 faithful, fix scoped to the Ollama/OAC builders
  so hosted requests stay byte-identical) and **v4 bug 81** (an
  OpenAI-Compatible profile can never hold an API key; the ordered shape is
  an OPTIONAL key via splitting `requiresApiKey` into requires/accepts).
  **OPEN on the v5 side: finding #96** — a provider failure logs as `key
  derivation failed` (`DbError::Key`'s `Display` prefix, that variant used
  as a catch-all at 10+ non-key sites); the fix is an error-kind split
  wider than a dogfood patch, and it matters because P4.49 made
  `combined.log` where an operator looks. **RETIRED from the 💸 queue: the
  orphan-reaper heal** — 0/0/0 orphans on data byte-identical to live
  Friday, because v4 landed its own reaper on 2026-08-06, three days after
  P4.31 measured 43+118. **Still owed:** the failed-import warnings, both
  notice surfaces (now unblocked — DeepSeek tool-calls reliably), the
  vision send, the Serper live-key smoke, whispered announcements, Pascal
  side effects, the roleplay-template quote delimiter, and the
  dedup/summaries first run (deferred by cost). Two open questions before
  anything is called a defect: the connection-profile list not refreshing
  on tab activation, and a twice-failed `STORY_BACKGROUND_GENERATION`
  against the Grok Images API.
- **The `9125f492` drift catch-up round (P4.D93 ∥ P4.D94): UNIFIED on main
  (2026-08-19) — BOTH CLOSED; the oracle baseline MOVES to `9125f492` and
  the drift debt is CLEARED.** v4's three commits past `c6ff8051` absorbed
  — v4 fixing bugs 81/82, the two this port filed from its own 2026-08-19
  dogfood walk, plus the Lantern uncensored-target change. P4.D93: a
  provider may ACCEPT a key without requiring one (`acceptsApiKey` through
  the manifest substrate + generator, one predicate home, the
  `resolve_connection_profile_api_key` gate+lookup composite at both
  Brahma sites — dangling id refuses loudly even where optional — and the
  SPA's unstarred optional OAC key field; the help-chat fourth site banked
  to `p4.9i2`); the spine measurement answered **v5 NEVER had bug 81's
  spine half** (the host key scan is capability-blind — pinned); the
  leading-system fold lands in the Ollama + OAC builders only (corpus 257
  → 263, all old rows byte-identical, DeepSeek's three surviving blocks
  the recorded regression guard; tier-2 closed the duplicate-predicate
  consolidation on the way). P4.D94: the story-background crafter selects
  candid vs concealment per call (seven generated constants; the concealed
  path proven BYTE-IDENTICAL at 5114 UTF-16 units), the flag carries
  through the empty-response retry unchanged, and the moderation reroute
  re-crafts candidly via a new `RerouteRecraft` seam on the shared
  machinery (avatar passes the no-op; its family the guard) — five new
  dangerous-chat fixture cases, seven red mutation proofs. **The §3 review
  read the whole combined diff: NO blocking findings** (the unifier's own
  conflict-marker slip was caught by the per-commit audit and amended).
  Gate: 439 test binaries / 2,231 / 0 over fresh `9125f492` oracles; the
  eight moved families by name zero SKIP; clippy both feature sets;
  release build; ng 331 files / 4,915; full Playwright **229/229 zero
  skips**. Versions: core 0.0.589, harness 0.0.508, host 0.0.73, SPA
  0.5.522. 💸 the dogfood queue gains the bearer-token OAC endpoint, the
  Qwen second-turn acceptance, and the candid story-background prompt.
  Round record: `status-log.md`.
- **P4.50 — the `DbError::Key` catch-all split (dogfood finding #96):
  CLOSED, UNIFIED on main (2026-08-19, solo stacked lane) — finding #96
  FIXED; the baseline STAYS `9125f492`.** `DbError::Internal(String)`
  (bare-message Display) at **243 of 246** construction sites; the census
  refuted the order's "dozen" premise downward — exactly TWO genuine
  key-derivation wraps (`Db::open`/`Writer::open_writable`) keep the
  prefix, held by the executable `db_error_key_guard` census (per-file
  exact counts, mutation-proven). Nothing observable moved: all 27
  `From<DbError>` shims reach the variant through catch-alls (the mapping
  inherited by construction), `db_error_response` still answers
  `ErrorKind::Internal`, and the `system_restore_state` leaked-prefix
  mask is RETIRED — restore warnings now byte-compare against v4's whole
  sentences. The two prefix-strip helpers retired, not retargeted. §3
  review: no blocking findings (the migration audited mechanically —
  every hunk a pure rename; the string-literal multiset moved by exactly
  one, the retired strip helper, rendered bytes identical). Gate: 440
  test binaries / 2,236 / 0 (+1 binary +5 tests, exactly the lane's
  delta) over a pin-fresh restore oracle; clippy both feature sets;
  release build; ng 331 / 4,915; full Playwright 229/229 zero skips.
  Versions: core 0.0.590, harness 0.0.509, host 0.0.74, web 0.0.77.
  Deferred loud: the three per-domain taxonomy candidates NAMED not
  built; 💸 the live `combined.log` look at a real failed turn joins the
  dogfood queue — **the owed dogfood pass over the round's surfaces is
  the top next candidate.** Round record: `status-log.md`.
- **The `c8a3cf77` per-turn-summaries round (P4.D95 ∥ P4.9L2 ∥ P4.51):
  UNIFIED on main (2026-08-20) — ALL THREE CLOSED; the oracle baseline
  MOVES to `c8a3cf77`.** The whole `870a57fa` drift absorbed (P4.D95):
  the instance-wide `memoryRecall.perTurnConversationSummaries` setting
  end-to-end (the recall bag now a STRUCT with one `to_json()` home; the
  SPA card with v4's strings byte-for-byte + a live round-trip beat), the
  `captureQueryEmbedding` hook as an out-param with v4's three firing
  semantics (before the dimension guard / never for probes / never on the
  text fallback; the try/catch arm a NO-PORT with evidence),
  `precomputed_embedding` on the vault summary search + the ramp-constant
  one-home, the proactive vector thread on BOTH return paths, and the
  build-context cadence whole (four gate conjuncts, the backwards
  stop-at-first fold-whisper dedup, the shared whisper target scope, the
  recap stand-down, the mini-recap's both-lists filter) — seven
  build-context ops + six red-then-green mutation proofs (the
  dimension-drift op exists BECAUSE a mutation survived the first five) ∥
  P4.9L2: the DocumentPane formatting toolbar (m6 row 14b CLOSED — v4's
  `DocToolbar` 1:1, no Nar button, the source branch on THIS pane's
  textarea through the frontmatter-recombine seam, ONE `toggleSourceMode()`
  behind both controls, the Salon threads its resolved template while the
  standalone view passes nothing; two live beats; two divergences recorded
  in the class doc) ∥ P4.51: both sweep riders discharged (the `W=`
  clobber proven both directions with a marker probe; the driver refuses
  unknown family names BEFORE any stage, exit 2 + suggestions, six
  self-test arms; three follow-ups recorded incl. the LIVE
  `brahma_console_routes` restored-recipe `W=` and the `nothing_to_run`
  vacuous-green class). **The §3 review fixed the would-have-shipped
  divergence** (an invalid recall-config value answered 200 and silently
  kept the stored bag where v4 400s "Validation error" — validation now
  runs first, pinned by three oracle arms incl. a writes-nothing
  composite), **and the gate's first by-name run caught two more**: the
  `housekeeping_config_set` silent standing red since v4 4.8.2 (a
  FIXTURE-VINTAGE artifact — now a RULED VINTAGE ROW with a repair
  tripwire; widening the shared `memories-{main,mount}.db` pair is a named
  maintenance item) and the oracle runner's record shaper dropping the
  composite's `storedAfter`. Gate: seven families by name over fresh
  PINNED oracles zero SKIP; 440 test binaries / 2,236 / 0; clippy both
  feature sets; release build; ng 332 files / 4,929; full Playwright
  **232/232 zero skips**. Versions: core 0.0.591, harness 0.0.511, SPA
  0.5.526; host/web/cli/tauri unchanged. 💸 the dogfood queue gains the
  per-turn cadence's live proof. Round record: `status-log.md`.
- **The `b8449b3e` anti-chorus + maintenance round (P4.D96 ∥ P4.52 ∥
  P4.53): UNIFIED on main (2026-08-21) — ALL THREE CLOSED; the oracle
  baseline MOVES to `b8449b3e` and the drift debt is CLEARED.** v4's
  `e22f7b36` absorbed whole: `isRecentlyAddressed` requires DIRECT address
  (the new vocative regex in core with the three JS-regex fidelity
  questions DECIDED BY MEASUREMENT — `JS_SPACE` spelled out, the `m`-flag
  gap closed by consuming JS-only line terminators, and **case folding a
  RECORDED DIVERGENCE in the safe direction, RATIFIED 2026-08-21**; the
  SPA client twin a character-for-character transcription, parity spec
  grown 1:1), the turn-skip note's restate-is-a-pass paragraph + reworded
  caution byte-exact, and the turn-anchor restructure with the byte-exact
  `GROUP_SCENE_DISCIPLINE` on BOTH routes — pinned by the NEW
  `multi_character_turn_anchor_equivalence` tier-1 family (no oracle drove
  that function before), `skip_signal_equivalence` red-first 15 → 43
  `recentlyAddressed` rows + the new `turnSkipNote` kind (an order premise
  REFUTED: `build_context_tier3` never carried the note — every op passes
  `turn_skip: None`; `orchestrator_tier3` is the one spine family carrying
  all three changed surfaces, measured 56/25/49 rows), and the `b8449b3e`
  jest-Sparkplug NO-PORT (our zone globalsetup CHAINS v4's, so the guard
  survives) ∥ P4.52: the committed `memories-{main,mount}.db` pair widened
  to the `b8449b3e` schema vintage (seven columns, measured not guessed;
  mount needed NOTHING; seeded rows byte-preserved cell-by-cell; TWO
  generateDDL columns deliberately absent — MANAGED_FIELDS no migration
  adds), the `housekeeping_config_set` RULED VINTAGE ROW retired to a
  plain equality (tripwire fired as designed, mutation-proven), and the
  round record's "pair is SHARED" claim REFUTED by measurement ∥ P4.53:
  sweep-recipe checkout aliases are UNFORGEABLE (`normalize()` rewrites
  every alias assignment; the five clobbering headers repaired — the live
  one had staged case + fixtures from MAIN during worktree sweeps;
  `--self-test` gained a tree-wide cross-alias-default header pin) and
  empty-stage `--run`s are a named REFUSAL (exit 2; the vacuous-green debt
  measured at 39 families, the committed artifact is the next maintenance
  inventory). **The §3 review: NO blocking findings** (the regex verified
  arm-by-arm incl. the asymmetric lone-CR pre-arm; the unifier's own
  mid-pick Cargo.lock slip caught by the next build and repaired
  pre-gate). Gate: 441 test binaries / 2,237 / 0 with the round's env
  block; the seven affected families fresh at a pinned `b8449b3e`
  worktree, zero SKIP, NDJSONs grepped for the changed bytes (56/25/49
  confirmed); driver self-test 0 failures; clippy both feature sets;
  release build; ng 332 files / 4,936; full Playwright green (numbers in
  the round record). Versions: core 0.0.593, harness 0.0.517, SPA
  0.5.527; host/web/cli/tauri unchanged. **💸 the dogfood queue gains the
  live group-scene walk** (does the discipline block break the chorus on a
  weak model — no oracle can judge that). Round record: `status-log.md`.
- **The anti-chorus + per-turn-summaries dogfood pass RAN (2026-08-21,
  agent-driven, on the Friday copy) — 23 rows, 18 PASS, ONE FIX SHIPPED,
  two findings, nine 💸 items discharged.** Walk doc:
  `dogfood-walks/2026-08-21-anti-chorus-pass.md`; record in `status-log.md`.
  (23 rows, 20 PASS; **eleven** 💸 items once the Serper smoke and Pascal side
  effects came off the list.)
  **Discharged:** the live group-scene walk (both anchor routes proven at the
  byte level on a real three-character chat; a character actually passed with
  the skip sentinel), the P4.D95 per-turn cadence (mutation-proven ON vs OFF
  over the PERSISTED whispers — the LLM *request* alone is not a
  discriminator), the P4.9L2 toolbar in both hosts, the vision send (a
  purpose-drawn PNG described correctly), the P4.50 `combined.log` look at a
  real failure, the bug-76 key heal, the tool-change splice-once, whispered
  announcements, the roleplay quote delimiter, and the failed-import warnings.
  **FIXED: finding #97** — `qt-tab-view` is an unstyled Angular custom element
  (`display: inline`) with no v4 counterpart, so `StandaloneDocumentView`'s
  `flex-1` host was inert and Document Mode's source textarea rendered 77 px in
  a 788 px pane; host → `h-full`, 77 → 612 px, commit `a42638e7`, gate green
  (ng 332/4,937; Playwright 232/232). **RECORDED: finding #98** — the `SERPER`
  key configured through v4's Settings → API Keys is invisible to v5, which
  reads only `SERPER_API_KEY`, because the search-provider plugin registry is
  the standing P4.42 deferral: web search is dark on a real instance. **The
  wire itself is PROVEN** — `api_keys.key_value` holds the raw secret, so the
  server was relaunched with that column read straight into the child's
  environment (never printed, never on disk, never off-host) and `search_web`
  returned five live results on a real turn, so advertised and executed agree
  the moment the provider exists. Only the *configured* path is missing.
  **P4.D35's Pascal side effects also closed end to end**: `agent_lambda` (the
  schema field is **`effects`**, not `sideEffects` — a grep for the wrong key
  is what first wrote this off) dry-ran in the Workbench under its own stated
  contract *"the bench computes effects; it never applies them"*, then committed
  live — a v4-written `metadata.lastLambdaOutput` overwritten in the character
  vault with every sibling key intact, `chipLabel` and the two-block bubble both
  rendering; the other three write paths stay unit-proven only. **Two
  v4-heuristic observations, both v4-faithful:** `and` sits in v4's
  `VOCATIVE_LEAD_INS`, so `…X and Y.` reads as addressing Y — a roll-call recap
  re-arms the very caution the anti-chorus fix withholds (**candidate upstream
  filing**); and the caution can never see the message that just addressed the
  responder (the user message is persisted AFTER the eligibility read, in v4
  as in v5). **#99 — a v4 bug, no v5 change:** on a failed `generate_image` the
  notice fires correctly but reads the generic `Failed to generate image` while
  the server sent the real sentence in `toolResult.error`, a **sibling** of the
  null `result`; v4 hoists it identically *and says the field exists so live UIs
  can show a useful message*, then v4's own client reads `result?.error` and
  drops it. v5 reproduces exactly, so it stays — **FILED as v4 Bug 84**
  (`bugs/bug-84-tool-error-sentence-never-reaches-the-ui.md`, v4 commit
  `c0984bdf`). ⚠ **#99 was
  first mis-filed as "the notice never appears," on three runs whose injected
  `setInterval` observers had silently died (`__ticks` frozen at 6).** The
  standing lesson is in `dogfood-findings.md`: verify a browser instrument is
  ticking before trusting any negative from it. **Still owed:** P4.D35's other
  three write paths, and the dedup/summaries first run.
- **The `12fe3e6f` thinking-turn drift round (P4.D97 ∥ P4.D98 ∥ P4.D99 ∥
  P4.54): UNIFIED on main (2026-08-22) — ALL FOUR CLOSED; the oracle
  baseline MOVED to `12fe3e6f`.** v4's bugs 84/85/86 absorbed whole (two
  were this port's own dogfood filings coming back fixed): the
  thinking-turn evaluator + registry join + the manifest substrate's
  first per-model facts + `thinkingTurnRule`, the prefill
  `runsThinkingTurn` threading, the model-aware DeepSeek strip, the
  retire-prefill heal over v4's own `migrations_state` ledger, the
  profile editor's three thinking-turn behaviors + the activated e2e
  beat, bug 84's two-layer client fix (finding #99 FIXED), and run lines
  for 32 of the 39 `nothing_to_run` families. §3: no blocking findings.
  Gate: 443 binaries / 2,253 / 0; ng 334 / 4,970; Playwright 233/233.
  Versions: core 0.0.599, harness 0.0.523, host 0.0.75, SPA 0.5.535.
  Round record: `status-log.md`. (This bullet was added a round late —
  the 12fe3e6f unification updated the status log but not this summary.)
- **The `4cb1035e` image + NanoGPT drift round (P4.D100 → P4.D101
  stacked ∥ P4.D102): UNIFIED on main (2026-08-22) — ALL THREE CLOSED;
  the oracle baseline MOVES to `4cb1035e`.** The `ca22ec45` catch-up +
  the two NanoGPT commits + v4 bug 87 (ruled IN mid-lane) absorbed
  whole: the honest image `list-models` verb end-to-end over a new
  `ErasedImageDiscovery` seam (the P4.D33 bank note retired at source;
  wired LIVE in the host), keyed model discovery for all five image
  plugins (**a real v4 bug found, TO FILE UPSTREAM: OpenRouter image
  discovery reads wire keys its own SDK's zod strips — every keyed
  listing throws; v5 reproduces with the
  `openrouter/models_live_every_signal` convergence tripwire**), the
  image-download seam + Z.AI URL→base64 (v5 measurably HAD the
  zero-byte bug), the gemini routing widening, and NanoGPT whole as the
  TENTH provider (manifest through the generator; `ProviderKind` +
  builder with the FLAT `reasoning_effort` allowlist; the dual
  `delta.reasoning ?? reasoning_content` dialect + bug 87's prose-echo
  guard as decoder state; images over the shared seam; embeddings with
  the catalogue pinned against v4's real plugin — the differential
  caught a doubled-error-prefix defect inspection missed; the thinking
  rule through the P4.D97 machinery, the exactly-two-rules guard moved
  2 → 3 by design; the census REFUTED four ordered legacy-table joins,
  guard-pinned) ∥ the SPA client half (the Fetch Models flow with v4's
  four label strings, the Z.AI/NanoGPT entries + Default Size panels,
  the embedding surface + badge CSS quirk preserved; two order items
  refuted by measurement; both gated beats FLIPPED LIVE). **The §3 read
  found no blocking findings; the unified gate caught two** — the
  routes oracle's `PLUGIN_DIRS` missing the nanogpt append (only the
  union could red), and the Fetch Models beat's first live run exposing
  an unreachable skip-guard (redesigned around an offline list-order
  discriminator, mutation-proven red-first). Gate: ten families fresh
  at the `4cb1035e` pin zero SKIP; manifests byte-identical; 443
  binaries / 2,261 / 0; clippy both sets; release build; ng 338 /
  5,016; full Playwright 235/235 zero skips. 💸 the dogfood queue
  gains the live-key Fetch Models smoke (the OpenRouter finding
  deserves a real key) + the NanoGPT chat/image/embeddings smoke.
  Versions: core 0.0.608, harness 0.0.531, host 0.0.77, SPA 0.5.539.
  Round record: `status-log.md`.
- **The `a6870c5a` prompts-trio round (P4.D103 ∥ P4.D104 ∥ P4.55):
  UNIFIED on main (2026-08-22) — ALL THREE CLOSED; the oracle baseline
  MOVES to `a6870c5a` and the drift debt is CLEARED.** The trio absorbed
  whole: the standing-instructions section end-to-end (the new
  `standing_instructions` module byte-exact incl. the ICU-collated
  name-then-instructions sort; the builder slot between Taboo and the
  tool instructions, template-processed; threaded to the live turn +
  Carina [the hand-built `{char, user: "User"}` context] +
  self-inventory, with the announcer/greeting exclusions verified per
  call site; the Prospero project-context whisper's duplicate section
  DROPPED; `PROMPT_CACHE_STRUCTURE_VERSION` 3 → 4; the groups verbs gain
  `instructions` + BOTH v4 validators ported whole — a premise refuted
  on the way: an empty-string PUT reads back `null` via the overlay's
  markdown reader) ∥ bug 88's second-person tool reinforcement (v5
  measurably HAD the `they CALLS them` bug) + the identity-stack
  person-consistency wording under the NEW version-stamped
  `compiledIdentityStacks` envelope (strict-equality read both
  directions, discard-on-merge, clear-on-drop; v4's golden table
  byte-copied and **v5's computed hash EQUALS v4's registered golden —
  a free cross-implementation proof**; the turn-time reader deferral
  UNCHANGED) ∥ the SPA client half (the shared `qt-prompt-field-label` +
  the twelve-key hints table proven byte-identical to v4's module twice
  over; the seven-surface migration sweep converging v5's drifted
  create/edit copy; the Group Instructions editor; the round-trip beat
  ACTIVATED — first live run green) ∥ P4.55: the `c8a3cf77` merge-verb
  silent-keep LEAD closed (the two memories config verbs were
  PERSISTING garbage — validate-first now; the autonomous-rooms
  ten-field patch leniency refused; projects update through v4's
  schema; the three missing-`else` `apiKeyId`/`baseUrl` sites fixed
  after measuring v4; the store_backed cleared-null echo measured NOT
  divergent; B2 data-retention present-null stays a NAMED next-round
  item). **The §3 review fixed TEN findings on the unify branch — the
  two that would have shipped:** `group_update` parsed before the
  existence check (400 where v4's find-first answers 404 — the
  guard-order class P4.55 got right on projects, missed on groups; the
  cross-lane blind spot), and the autonomous `title` max counted
  Unicode scalars where Zod counts UTF-16 units (astral titles passed
  v5, fail v4) — both fixed + arm-pinned; also the settings-routes
  stale-oracle floor, the beat's `waitForRequest` flake-in-waiting,
  the identity-compiler sentinel tripwire, and five smaller repairs.
  Gate: the 43-family regen+run sweep 43/43 ok zero SKIP over oracles
  FRESH at `a6870c5a` (the first pin-free round in five); 444 test
  binaries / 2,266 / 0 with the 75-variable env block; clippy both
  feature sets; release build; ng 341 files / 5,054; full Playwright
  **236/236 zero skips** (the suite grew with the activated beat). 💸
  the dogfood queue gains standing instructions on a REAL turn + the
  Group Instructions walk + the invalid-config 400s. Versions: core
  0.0.620, harness 0.0.542, SPA 0.5.544; host/web/cli/tauri unchanged.
  Round record: `status-log.md`.
- **The `f8973813` NanoGPT-caching + settings-wire round (P4.D105 ∥
  P4.56): UNIFIED on main (2026-08-22) — BOTH CLOSED; the oracle baseline
  MOVES to `f8973813`.** v4's one drift commit absorbed whole: NanoGPT
  prompt caching (plugin 1.0.3) — the Prompt Caching options group
  through the manifest generator (zero generator change; nine siblings
  byte-identical), the `promptCaching` body key behind the STRICT
  `=== true` gate (probed on v4) with the literal-`'1h'` TTL collapse and
  the consumed-keys asserts, and both-dialect cache-usage normalization
  (`nanogpt_cache_usage` shared by the non-streaming parse AND the
  streaming final chunk; the `??`-precedence pin MEASURED —
  `cache-read-zero-present`: v4 charges all 600 prompt tokens; cache
  reads excluded from prompt/total via the shared `sub_floor` leg;
  unconditional `rawProviderUsage: usage ?? null`), corpora 307 → 321 /
  46 → 52 / 16 → 22 with every pre-existing row byte-identical + eight
  mutation proofs; the `response_parse_equivalence` run-line debt closed
  ∥ P4.56: the P4.55 remainders — **B2 fixed red-first** (the harness
  serde-path rewire landed FIRST so `dr_put_null` could measure the real
  divergence: v4 400, v5 silent-keep; then `double_option` + the
  three-arm engine match), the new `GET/PUT
  /api/v1/settings/data-retention` edge decoding through the `Request`
  enum — **which uncovered two real bugs, fixed + pinned:
  `CoreResponse::BrahmaConsole` missing from `unwrap_to_http`'s success
  arm since P4.D57 (both brahma-console REST edges 500'd on every
  success) and the two data-retention handlers leaking `DbError` text** —
  the groups cleared-null pin (zero source change, the fresh oracle
  measured `"description": null` present-not-omitted),
  `settings_wire_actions` building its own fixture (5/5 from a clean
  /tmp), the float-literal store fix via `normalize_js_numbers` behind a
  new float-SENSITIVE comparand (the family's own normalizer would have
  made the arms vacuous), and the shared `classify_api_key_id` /
  `classify_base_url` readers (behavior-neutral, mutation-proven live at
  all three sites). **§3: NO blocking findings** (three non-blocking
  notes in the round record); the wire: the NanoGPT spec-fixture
  transcription gained the Prompt Caching group + two showIf render
  specs. **Mid-round incident:** v4's checkout went DIRTY during the
  lanes and later committed as `65f3476e` — dispositioned NO-PORT with
  evidence (CI/release infra + a comment-only lib edit + standalone-
  tarball native linking v5 doesn't have); both lanes and the unified
  gate ran every regen from pinned worktrees at `f8973813`. The gate's
  own catch: the first sweep launch piped through `| tail` (the standing
  rule's exact mistake) — re-run by name with full capture, which then
  caught two families silently SKIPping on missing env vars; re-run
  green. Gate: 12/12 families fresh at the pin zero unexplained SKIP;
  445 test binaries / 2,269 / 0 (exactly the union of the lanes'
  deltas); clippy both feature sets; release build; ng 341 files /
  5,056; full Playwright green (numbers in the round record). Versions:
  core 0.0.627, harness 0.0.550, web 0.0.78; host/cli/tauri/SPA-version
  unchanged. 💸 the dogfood queue gains the live NanoGPT caching smoke +
  the data-retention 400 on a live screen. Round record:
  `status-log.md`.
- **The four-round dogfood pass RAN (2026-08-23, agent-driven, on the
  Friday copy) — 37 rows, 27 PASS, ONE finding found and fixed, and three
  proofs that came free because v4 runs these same features on this
  instance.** Walk doc:
  `dogfood-walks/2026-08-22-four-round-pass.md`; record in `status-log.md`.
  **FIXED: finding #100** — every streamed chat message logged
  `durationMs = 0` (three sites hard-coded it; measured against 6,115
  v4-written rows where not one zero appears). `StreamLogCtx` now carries
  `started_at_ms` stamped where v4 takes its `startTime`; because no
  differential can tell a hard-coded zero from a measured one once the
  column is normalized, the guard is a **source census** in the
  `db_error_key_guard` idiom, mutation-proven. Gate: 446 binaries / 0
  failed + the three affected families green at a pinned `f8973813`;
  live proof 9,601 ms (`6500e1e1`; core 0.0.628, harness 0.0.551).
  **Free cross-implementation proofs:** the retire-prefill heal's ledger
  row arrived ALREADY WRITTEN BY v4 (so the cross-app guard is proven
  v4→v5 on real data) and v5 then reproduced v4's verdict byte-for-byte
  over the same 50 profiles; the `[STANDING INSTRUCTIONS]` section is
  **byte-identical to v4's at 773 bytes**, correctly between Taboo and
  the tool instructions; both apps stamp `compiledIdentityStacks`
  version 2. **Also proven live:** the 13-case thinking-turn evaluator
  matrix, bug 85's repro chat, the editor's thinking warning
  (mutation-proven both ways, warns-never-vetoes), Group Instructions
  round-trip incl. empty→`null`, the Prompt Caching card + `promptCaching`
  on the REAL NanoGPT wire in both TTL arms with no option-key leakage,
  the data-retention 400, the brahma-console edge answering on success,
  image `list-models` across four live keys (OpenRouter falling back
  honestly rather than throwing), a real NanoGPT image at 216,414 bytes,
  validate-first on the memories config, and **v4 bug 82's fold proven in
  both directions in one tap file** (OLLAMA folds to 1 leading system
  message, NANOGPT keeps 3, same model and conversation). **The
  2026-08-19 connection-profile refresh question is RESOLVED as NOT a
  defect.** Traps banked: the browser pane's `Cmd+Shift+R` does not
  reload (prove it before trusting a negative), `wire-tap.py` truncates
  at 8000 chars, and `llm_logs.request` is a pre-builder projection that
  cannot show the fold. 💸 still owed: the caching smoke, the Brahma
  budget on a deep query, the failed-`generate_image` sentence, the
  candid story background, Pascal's other three write paths, the NanoGPT
  embedding leg, a bearer-token OAC endpoint (blocked — no local server),
  and dedup/summaries.
- **The `a14a1811` vision round (P4.D106 ∥ P4.D107 ∥ P4.D108 ∥ P4.D109 ∥
  P4.57): UNIFIED on main (2026-08-23) — ALL FIVE CLOSED; the oracle
  baseline MOVES to `a14a1811` and v4 bugs 91–95 are absorbed whole.**
  The image-transport predicate pair (registry → static → unknown-true)
  wired into the describe-fallback's three sites, the ten-literal
  moderation finish-reason table replacing "known issue…try resending"
  for named refusals, and the three-tier attachment anchor (id carry +
  the pre-normalization user-turn set; the downstream-stamp measurement
  found ONE real re-anchor — the non-streaming regenerate funnel — fixed
  via `send_message_with_anchor` with a wire-byte pin; two NEW tier-1
  families) ∥ NanoGPT plugin 1.1.0 (`image_url` + the truthful ledger;
  corpus 321 → 341, every pre-existing row byte-identical; a tree-wide
  `attachment.url`-arm blind spot closed corpus-only) ∥ the
  `describe_image` looking verb end-to-end (catalog 57 → 58, the
  auto-describe module v5 never had, the three-tier handler with the
  no-album rule, the five Librarian rewrites; the production vision-tier
  wiring landed as the §4 unification wire — OrchestratorDeps + the
  spine thread the describe driver AND the photo-bytes store, ⚠ LIVE
  vision spend on every tool path) ∥ the attachment-failure warning
  toast through the reducer carry (identity-keyed once-per-done) + the
  client attachment table's staleness note retired (a14a1811 IS the
  upstream fix its header predicted) ∥ tri-state decode-once across all
  three settings verbs (byte-diff-proven zero-behavior-change; the
  differential's serde-bypass blindness closed). **The §3 review's
  headline catch: the vision tier was structurally unreachable** (the
  driver half without the bytes half = `no-bytes` starvation on every
  production path — fixed with wiring probes + mutation-proven pins);
  also fixed at unification: the `restream_into` attachment-ledger carry
  (bug 94's new reader made the stale value user-visible), auto-describe
  propagating DB failures raw as v4 does, `dangerMode` on the
  empty-response warn, the NANOGPT coverage floors, the id-set predicate
  extraction pin, and five smaller repairs. **FILED UPSTREAM (2026-08-23,
  v4 `7a6716b5`): the OpenRouter transport contradiction is v4 bug
  97** (the registry entry declares `supportsAttachments: false` while
  its static map transports — v4 PRODUCTION routes OpenRouter vision
  profiles to the describe-fallback and refuses OpenRouter describers
  while the guard sentence recommends them; jest never sees it); the
  moderation docblock's "(bug 94)" mis-number was corrected in the
  same commit. Gate: the pinned 24-family sweep 24/24 ok zero
  SKIP; clippy both feature sets; ng 341 files / 5,068; the full numbers
  in the round record. 💸 the dogfood queue gains the NanoGPT vision
  send, the Z.AI refusal sentence, the describe_image walk (the vision
  tier's first live run), the failed-attachment toast, and the
  whisper-tailed regenerate. Versions: core 0.0.643, harness 0.0.559,
  host 0.0.79, web 0.0.79, SPA 0.5.548. Round record: `status-log.md`.
- **The `0ba942b1` drift round (P4.D110 ∥ P4.D111 ∥ P4.58): UNIFIED on
  main (2026-08-23) — ALL THREE CLOSED; the oracle baseline MOVES to
  `0ba942b1` and the drift debt is CLEARED** (bugs 96 + 97 absorbed
  whole; `7a6716b5`'s one comment line ridden along). The title-verdict
  parser end-to-end (near-miss keys + fold pass + double-trim + four
  byte-exact warn arms with per-site task labels; the checkpoint-burned
  handler warn with cursor semantics UNCHANGED — the commit-prose trap
  settled by measurement at planning; `title_update_tier3` 10 → 17
  RED-FIRST with 5/7 new cases state-mismatching pre-fix; the warn
  WIRING pinned by a capturing tracing layer because the burned
  checkpoint's DB state is byte-identical to a genuine decline) ∥ the
  pre-announced bug-97 convergence (v4 fixed this port's own filing at
  `0ba942b1`; the manifest regen with nine siblings byte-identical, the
  predicate-test flip, the guard sentence's `NanoGPT, ` entry, the
  moderation mis-number note retired — every former both-directions pin
  now a plain equality, red-first per family; the help paragraph banked
  to `p4.9i2`) ∥ P4.58's corpus blind spots closed with ZERO v5 source
  change and THREE order premises refuted by measurement (no committed
  photo-tools DBs; `attach_image` not `list_images`; fixture-side
  mutations prove nothing — v5-source mutations used, nine proofs).
  **The §3 review read the whole combined diff against v4's real code:
  NO blocking findings** (the second such round); the one wire was the
  version recount (the playbook's silent-auto-merge trap fired again).
  Gate: the 7-family pinned sweep zero SKIP with changed bytes grepped;
  449 test binaries / 2,320 / 0; clippy both feature sets; release
  build; ng 341 files / 5,068; full Playwright 237/237 zero skips.
  Banked: v5's title-update handler carries 1 of v4's 8 log lines (a
  small handler-logging order — phase-4.md candidate 3). Versions: core
  0.0.645, harness 0.0.562; host/web/cli/tauri/SPA unchanged. Round
  record: `status-log.md`.
- **The vision-round dogfood pass RAN (2026-08-24, agent-driven, on the Friday
  copy) — 19 rows, 16 PASS, NO v5 defects, and eight 💸 items discharged.**
  Walk doc: `dogfood-walks/2026-08-24-vision-round-pass.md`; record in
  `status-log.md`. **The `describe_image` vision tier ran in production for the
  first time** — all three tiers proven on real images through Run Tool, with
  the `vision-call` arm a real 6,996 ms GROK call whose description persisted,
  then re-proven free as `stored-description` (`IMAGE_DESCRIPTION` rows 7 → 7).
  **The NanoGPT vision send is proven twice over**: `image_url` + a 3,000-char
  `data:` URL on the real wire (a new structural tap — `harness/tools/wire-tap.py`
  collapses `messages` to a count and cannot see content parts), and
  `zai-org/glm-4.6v` reading a purpose-drawn PNG exactly right. Also live: the
  bug-91 describe-fallback on an OLLAMA seat (v4's log line, zero `image_url`,
  the spliced description), the bug-97 OpenRouter convergence, the P4.D109
  attachment-failure toast on a real `image/bmp` drop, **bug 93's moderation
  sentence in BOTH arms** and **bug 96's near-miss title key** — the last two
  driven end-to-end by purpose-written provider stubs (an empty stream with a
  chosen `finish_reason`; a canned verdict under `Suggested_Title`), so the
  refusal path was proven **without composing anything a provider would have to
  refuse**. 💸 also discharged: bug 84's real sentence reaching the UI, Google
  Fetch Models on a real key (37 models, not the 8-id fallback — finding #91),
  and the concealed story-background variant at exactly 5,114 characters.
  **Unplanned proofs:** P4.D42's 300 s request bound fired to the millisecond
  and its retry succeeded; P4.50's split shows a real failure with no
  `key derivation failed:` prefix; P4.D85's cleared-null heal; P4.D78's Ollama
  `think`/`options`/`keep_alive` body; bug 54's sha256 dedup. **Two recorded
  rows, neither a v5 defect:** **#101** NanoGPT prompt caching writes a cache
  every turn and never reads one though the system blocks are byte-identical
  (the flag demonstrably reaches Anthropic; where the gateway puts its
  breakpoint is its own side of the wire and v4 sends the same body — raised
  for the human as a cost question), and **#102** a plain regenerate re-sends
  no attachments **because v4 does not either** (measured), so the
  whisper-tailed-regenerate 💸 item needs a **Lantern**-bearing chat, not the
  shape it was written against. Zero panics in ~2 hours on the real 800 MB
  instance. **Still owed:** Pascal's other three write paths, the Brahma budget
  on a deep query, dedup/summaries (human), and the candid story-background arm.
  **Finding #94 was RULED and FIXED the same day** (host 0.0.80): the Almanack
  measures free memory now — and the finding's own technique was wrong, so the
  fix measured it (macOS is `Pages free` PLUS `Pages speculative`, since
  `vm_stat` subtracts speculative where `os.freemem()` does not; Linux reads
  `MemFree` through a parser now shared with `MemTotal`). The mutation pass
  caught an unpinned WIRING — reverting the struct literal to `0.0` left every
  test green because they all called the function directly — so a
  `runtime_facts()` arm was added. Live: `Free Memory: 12.3 GB` against Node's
  12.22 GB on the same host.
- **The no-drift maintenance round (P4.59 ∥ P4.60 ∥ P4.61): UNIFIED on main
  (2026-08-25) — ALL THREE CLOSED; the baseline STAYS `0ba942b1`, and ⚠ v4
  drifted TWO commits DURING the round (`af1bc479` + `c93ec7ff`, both ported
  surfaces — the catch-up is the top next candidate; pin `0ba942b1` for
  every regen until it runs).** Dogfood **#98 CLOSED** — the configured
  search provider end-to-end: the native `SearchManifest`/`SearchRegistry`
  behind v4's site-plugins gate (one recorded divergence: the ten native LLM
  providers are not `SITE_PLUGINS_*`-gated), ONE registration answer feeding
  the runner (`serper_registered = true`, per-call keys live from `api_keys`
  through the now-load-bearing `DbSearchApiKeys`), the tools-inventory bool
  AND the providers listing's `type: 'search'` row (whole-row byte compare,
  key order included — whose first run caught the harness's own
  `Map::remove` swap-remove reordering under `preserve_order`); the
  plugin-arm-only `User-Agent` + the reachable `validateApiKey` probe; the
  tier-3 oracle rebuilt over v4's REAL registry + REAL dist plugin (17 → 26,
  the vacuous which-key arm caught by mutation and fixed with a header-echo
  comparand); **the SPA's invented `type === 'llm'` API-keys filter removed**
  (v4 filters on `providerAcceptsApiKey` alone — the invented filter was
  #98's remaining half) with `capabilities` optional; the salon web-search
  beat MOVED to the configured path (no env key; the seeded `api_keys` row
  is what reaches the wire) ∥ P4.60: the wrong-type-collapse adjudication
  COMPLETE — 14 DIVERGENT-FIXED / 6 FAITHFUL / zero escalations across
  custom-tools, characters photos, the four Brahma bodies (validated AFTER
  v4's 404 gate via `brahma_send_prepare`), the restore trio (guard order in
  ONE place — the two entrances used to disagree), the reindex `scope`
  (absent/null split + `String()` coercion), and two qtap neighbours the
  confirm-only pass found; the census is EXECUTABLE
  (`web_edge_body_parse_guard`) with the remaining pockets named
  (`system_data_routes` 13, `files_routes` 5, `llm_logs_routes` 1) ∥ P4.61:
  five of v4's eight `[Title Update]` log lines byte-faithful (`:89`/`:185`
  NO-PORTs with v4-source evidence — a dead branch and an unreachable
  catch), capture-layer presence + silence pins with six mutation proofs;
  the `docs/v4/` mirror refreshed at the baseline (19 modified + 97 added).
  **The §3 review: NO blocking findings** (fidelity re-checked against v4's
  real code; the lane-close timeline audited against the drift — no regen
  ever saw a moved tree). Gate: 13/13 families fresh from the pinned
  worktree zero SKIP with discriminating bytes grepped; 453 test binaries /
  2,338 / 0 with the round's 20-variable env block; clippy both feature
  sets; release build; ng 341 / 5,072; full Playwright green (numbers in
  the round record). Versions: core 0.0.655, harness 0.0.574, host 0.0.82,
  web 0.0.86, SPA 0.5.549. 💸 the dogfood queue gains the finding-#98
  scenario itself on the Friday copy (the `SERPER` row v4 wrote should now
  just work, no env var) + the title-update lines in a real `combined.log`.
  Round record: `status-log.md`.
- **The `f6a10055` wardrobe-containers drift round (P4.D112 ∥ P4.D113 ∥
  P4.D114): UNIFIED on main (2026-08-25) — ALL THREE CLOSED; the oracle
  baseline MOVES to `f6a10055` and the drift debt is CLEARED.** v4's four
  commits past `0ba942b1` absorbed whole. Server (P4.D112): the
  slug-collision vault fix (`build_slug_by_item_id_map` two-pass,
  nobody-on-collision — **v5 measurably HAD the bug**, red-first), the
  transfers explicit `source` container + `components: move|copy|none`
  (transitive same-container closure, plan-first id remap,
  refuse-on-collision with v4's title-in-"the ID of" quirk,
  components-land-first, post-write read-back → `unresolvedComponentIds`;
  corpus 8 → 18 + five web-edge tri-state cases; a NEW
  `TransferError::Server` fixed a pre-existing collapse of v4's two
  explicit serverError sentences), and the five `GroupWardrobe*` verbs
  dispatch-only (the project-tier precedent) over the NEW 15-case
  `group_wardrobe_routes_equivalence` real-DB family. SPA (P4.D113): the
  container module 1:1 + the verb router, the dialog's container browser
  (characters / General / projects / groups, v4's optgroups + banner copy),
  `canManage` on the row, the pinned editor — **v5 had v4's latent
  mis-target bug: any shared edit PUT Quilltap General; fixed** —
  `imagePrompt` preserved on Duplicate (v5 had that bug too), the transfer
  dialog's known-home hiding + component radio prompts, the download rider;
  the ordered response-fields render REFUTED by measurement (v4's client
  never reads `componentsTransferred`/`unresolvedComponentIds`). Both
  (P4.D114): the blob route's inline `Content-Disposition` (stored
  basename, header bytes vs v4's REAL helper), bug 98 via a 22-body
  measurement — **bug 98's shape was already absent from v5; the real find
  was the reverse: v5's create validated NOTHING but a non-blank name**
  (the full `PROJECT_CREATE_SCHEMA` landed, 18 differential arms + 9 unit
  tests, a v5-only whitespace-name refusal removed), the four download
  surfaces + transcribed `clipboard-utils`, and the create-toast fix (v4's
  fixed sentence, never the server's). **The §3 review: NO blocking
  findings** (the third such round; two loud out-of-ownership edits stood,
  the invented-banner check came back v4's-own-copy). Wires: the §2
  contract folded into `core-contract.ts` with the casts retired + the
  name-for-name diff clean; the component-transfer beat ARMED — it
  self-parks on the committed fixture's missing General store (widening
  that fixture is a named candidate). Gate: 6/6 families fresh from the
  PINNED worktree zero SKIP with changed bytes grepped; 454 test binaries
  / 2,353 / 0; clippy both feature sets; release build; ng 344 / 5,145;
  full Playwright **241 passed / 0 failed / 1 skipped** (the one skip is
  the store-probe park, by design). Versions: core 0.0.658, harness
  0.0.576, web 0.0.87, SPA 0.5.556; host/cli/tauri unchanged. Round
  record: `status-log.md`. **💸 DISCHARGED by the 2026-08-25 dogfood pass**
  (below) — the container browser, the component-carrying move, the Photos
  Download/Copy, and the create refusal all ran on real data.
- **The `f6a10055`-round dogfood pass RAN (2026-08-25, agent-driven, on the
  Friday copy) — 41 rows, 34 PASS, ONE finding found and FIXED, and two
  standing 💸 items discharged.** Walk doc:
  `dogfood-walks/2026-08-25-wardrobe-containers-pass.md`; record in
  `status-log.md`. **FIXED: finding #103** — a wardrobe component reference
  that goes unresolvable is dropped in **total silence**, and the next write
  to that container erases it from disk (found by consequence: moving one
  component out of a project took the parent outfit from 7 refs to 6, and
  moving it back did not restore it). The drop is v4-faithful and stays; the
  **warning** was the port divergence — v4 warns at BOTH drop sites and
  carries `characterId`/`mountPointId` for no other reason than to name them
  there. Restored verbatim with three capturing-layer tests (drop fields,
  cycle, and the silence leg), three mutations each reddening exactly one,
  and a LIVE proof in the real `combined.log` (`795ca3c5`, core 0.0.659;
  gate 454 binaries / 2,356 / 0). **Proven on real data:** the editor
  mis-target fix on BOTH shared tiers (General untouched at 13 items, newest
  `updatedAt` 2026-08-07), all five `groupWardrobe*` verbs, a MOVE that keeps
  every id (project 28 → 24, group 1 → 5) and a COPY that mints every id and
  rewires the refs (General 14 → 18), `unresolvedComponentIds` + v4's error
  line, the engineered collision answering v4's exact sentence with nothing
  written, the slug-collision fix writing a collider by UUID (the instance has
  **zero** natural colliders across 44 containers), `Content-Disposition`
  preferring the stored `.webp` basename over a `.png` `originalFileName` plus
  both RFC 5987 arms, the Photos/Scriptorium downloads (the latter on a row
  where the two names genuinely disagree), and the projects CREATE schema —
  nine shapes that used to answer 200 now 400 with nothing written, the
  whitespace-only name now accepted, and the toast reading v4's fixed
  `Failed to create project`. **💸 discharged: finding #98 is CLOSED**
  (`search_web` ran off the configured `api_keys` row with NO
  `SERPER_API_KEY` in the environment, and `providerList` carries the
  `"type":"search"` row) and the `[Title Update]` lines landed in a real
  `combined.log` (forced cheaply — the early checkpoints are interchanges
  2, 3, 5, 7, 10, so a new chat reaches the first in two turns). **Recorded,
  not filed:** the kebab menu is clipped by the list's own scroll container
  with a short list — **v4 is byte-identical there**, so it is a ported wart
  and a candidate upstream nicety; and `qt-image-gallery` still has no v5
  host. **Still owed:** Pascal's other three write paths (deferred a fourth
  time, but the recipe is now written down), the Brahma deep-query budget, and
  dedup/summaries.
- **The same-day human-authorized follow-up (E6 / E7 / G6) found a SECOND
  finding — #104, FIXED (core 0.0.660).** With image spend authorized: the
  **candid story-background arm** proved out to the character (an
  `IMAGE_PROMPT_CRAFTING` prompt of **4,255 UTF-16 units — exactly** the
  computed candid join; the same arithmetic reproduces P4.D94's recorded 5,114
  concealed, which validates the method), Generate Image downloaded under its
  file-id name, and the avatar-preview rider passed **both** arms with **zero
  `a[download]` anchors** in the DOM. **FIXED: finding #104** — every non-2xx
  from an SDK-backed image provider collapsed into the generic `Invalid
  response from <name> Images API`. v4 generates through the OpenAI SDK for
  **OPENAI/GROK/Z_AI/NANOGPT** and the SDK throws on any non-2xx with the API's
  own message, reserving that sentence for a 2xx with a malformed body; v5
  passed the response to the parser whatever the status. Found by replaying a
  real failed generation: Grok answered **`400 {"error":"Generated image
  rejected by content moderation."}`** — which also **explains the 2026-08-19
  pass's unexplained `Invalid response from Grok Images API`**. Fixed with the
  status gate plus the SDK's full three-way message rule (all four rules
  measured against the REAL SDK through a stub server), three tests, and a
  both-directions split guard added because a mutation widening the gate to ALL
  providers — silently replacing GOOGLE's and OPENROUTER's own sentences —
  stayed green until it existed. Five mutations, each reddening the right
  tests. **⚠ #104 was a DEAD FEATURE, not a bad string — measured after the
  fix:** the Concierge picks the uncensored-image reroute by KEYWORD-MATCHING
  the error (`is_image_moderation_error`), so while every non-2xx read
  `Invalid response from … Images API` nothing matched and **AUTO_ROUTE image
  generation was dead for all four SDK providers**. Same chat, before vs
  after: **FAILED** with one GROK row → **COMPLETED** with two, the second
  **NANOGPT/`chroma`** reading `Generated 1 image(s) (Concierge reroute)`. v4
  was never affected (its SDK throws that message), so there is nothing to
  file upstream; a sixth test pins the message through
  `is_image_moderation_error` with the pre-fix sentence as counter-example.
- **The `8f910137` drift catch-up round (P4.D115 ∥ P4.D116 ∥ P4.D117 ∥
  P4.D118): UNIFIED on main (2026-08-25) — ALL FOUR CLOSED; the oracle
  baseline MOVES to `8f910137` and the drift debt is CLEARED.** v4's five
  commits past `f6a10055` absorbed whole. The scenario-change feature
  end-to-end (the extracted `scenario_selection` resolver — a latent v5
  JS-truthiness divergence closed, empty pointers now fall through as v4's
  falsy test does, pinned by table-less-connection unit tests; the
  `chatSetScenario` verb with the MEASURED composite guard order [404
  beats 400 — the route layer gates before the handler], the chat-GET
  `scenarioText` projection, the Host revision announcement byte-exact +
  the cross-module `HOST_LINK_KINDS` pin, the transcript carry; the 50-case
  `chat_scenario_routes_equivalence` family over the NEW committed
  `chat-scenario-{main,mount}.db` fixture; SPA: the shared ScenarioSelect
  [the controlled-select `afterRenderEffect` idiom; the character tier's
  dropped ` — description` suffix restored — a pre-existing v5 divergence],
  the in-chat control in v4's slot, the raw-`controlledBy` cast read RULED
  an ownership read, and the `salon-scenario-flow` walk ACTIVATED at
  unification, green first run) ∥ bugs 100/102 (the census REFUTED the
  order's floor upward: **69 inert qt-* names over 364 call sites**; the
  sheet took v4's 490-line diff with zero fuzz, 37 files swept, and the
  `check-qt-classes` guard now runs in `npm run lint` AND ahead of
  `npm test` — component selectors subtracted mechanically, the cross-lane
  tripwire discharged at unification, 934 classes with every reference
  resolving) ∥ bug 99 measured-then-ported (the gallery tab had NO download
  control; v5 measurably HAD the stacking trap — the beat ran RED first,
  `elementFromPoint` returning the toolbar's queue badges; the fix is the
  body-reparent idiom moved to `afterNextRender`, a constructor reparent
  being silently undone under `@if`) ∥ bug 101 (templates byte-copied,
  Tier R red-first exactly the three completion cases → 188/0, the
  bash-driving `completion_behavior` guard red-proven against the pre-fix
  templates — v5 measurably HAD the bash half). `8f910137` itself
  NO-PORT-RATIFIED (CI + tests-only; its +18 test lines absorbed by the
  guard). **The §3 review caught one log-only fidelity gap, fixed at
  unification:** `source_label` used `is_some()` where v4's cascade uses JS
  truthiness (the audit shape: every `is_some()` transcribing a JS
  `x ? …` over a string). Gate: 456 test binaries / 2,376 / 0 with the
  round's env block, both families confirmed RUN; oracles fresh at the new
  baseline with changed-bytes greps matching the lane records; Tier R
  188/0; clippy both feature sets; release build; ng 347 files / 5,196;
  full Playwright green (numbers in the round record). Versions: core
  0.0.665, harness 0.0.577, cli 0.0.12, SPA 0.5.566. 💸 the dogfood queue
  gains the in-chat scenario picker, the gallery download, the restyled
  qt-* surfaces, and a real `docs --instance <TAB>` completion. Round
  record: `status-log.md`.
- **The `b220999d` drift catch-up round (P4.D119→P4.D120 stacked ∥ P4.D121
  ∥ P4.D122): UNIFIED on main (2026-08-26) — ALL FOUR ORDERS CLOSED; the
  oracle baseline MOVES to `b220999d` and the drift debt is CLEARED.**
  v4's three-feature day absorbed whole: per-tier dressing instructions
  end-to-end (the cascade module + `preserve_file_names` + the reader skip
  + the outfit-prompt thread at BOTH v5 `llm_choose` entrances + the four
  instructions verb pairs with the `double_option` tri-state + the SPA
  Section in both hosts), archive-instead-of-delete whole (the scenarios
  chokepoint with default suppression; the character-vault
  `build_scenario_file` rewrite — **v5's description-drop bug proven
  red-first**; `includeArchived` on every list verb AND the nine scenario
  mutate verbs' fresh-list returns, the two formerly-hard-coded-`true`
  wardrobe reads red-first; `archived_patch` idempotence; the Green Room
  never-auditions pins; the nine SPA hosts with the B7 quirks reproduced
  AND spec-pinned), and the Documents-search vertical (the LIKE engine
  with the fail-closed archived-vault exclusion, the two repo scans on the
  bare-column MIN rule, the `uiSearch` sixth type + chip reorder over the
  re-baselined 28-case corpus gate, the Documents card with the
  modified-click passthrough, the open-from-search choreography, the
  ACTIVE walk). **The §3 review (three parallel reviewers, verdict owned
  at the unify) caught three would-have-shipped findings, fixed
  red-first:** the three scoped instructions SET handlers parsed BEFORE
  the 404 gate (the `a6870c5a` guard-order class AGAIN — doc comments
  stated v4's order while the code inverted it; and v4 is inconsistent:
  the character-scenario routes parse FIRST, both now faithful), the
  scenario `archived: null` silent-keep (the present-but-null class
  AGAIN; Zod-4 sentences measured on v4; the sibling
  name/description/isDefault arms' null-tolerance recorded as a
  pre-existing LEAD), and the wardrobe REST edges' unknown-`?action=`
  fallthrough (`POST ?action=bogus` could CREATE — now v4's dispatcher
  envelope, wire-tested). The wires: the P4.D122 `PENDING_CROSS_LANE`
  document-opened listener discharged into `document-mode.ts`
  (spec-pinned, gate mutation-proven); the SPA's interim mutate-relist
  divergence RETIRED to v4's shape (the mutate verbs carry the flag;
  create stays flagless per v4's body-not-param quirk); the three gated
  beats flipped live — **their first live runs caught three gesture
  defects** (all-archived hides the WHOLE dropdown; Create needs a Type;
  the dialog + tab both mount the Section), fixed spec-side. Gate: the
  31-family sweep from the pinned worktree 31/31 ok zero SKIP
  (changed-bytes grepped); 461 test binaries / 2,426 / 0 with the 60-var
  env block, zero SKIP lines; clippy both feature sets; release build; ng
  351 files / 5,292; full Playwright **249/0/1** (the standing
  store-probe park; suite 245 → 250). New follow-ups recorded in
  phase-4.md: the duplicate "Quilltap General" e2e-fixture store
  (`builtin_mounts.rs` suspect), the present-but-null lead, four v4-side
  filing candidates. 💸 the dogfood queue gains the round's surfaces
  (the cascade on a real "Let character choose" turn, the archive walk,
  the Documents chip over real Friday stores). Versions: core 0.0.677,
  harness 0.0.586, web 0.0.92, SPA 0.5.576; host/cli/tauri unchanged.
  Round record: `status-log.md`.
- **The `b220999d`-round dogfood pass RAN (2026-08-26, agent-driven, on the
  Friday copy) — 41 rows, 37 PASS, ONE finding found and FIXED, nine 💸 items
  discharged, no v4 bugs to file.** Walk doc:
  `dogfood-walks/2026-08-26-instructions-archive-search-pass.md`; record in
  `status-log.md`. **The pre-walk measurement handed the pass its best proof:**
  v4 had run the brand-new dressing-instructions feature on this instance hours
  before the copy was taken (`Wardrobe/instructions.md` on four characters) and
  had already archived 17 wardrobe items across all four tiers — so v5 read
  **v4's own bytes back byte-identically**, and the cascade reached a real
  "Let character choose" turn carrying them (plus a second chat proving the
  **project-tier fall-through**). **FIXED: finding #105** (`599f6be9`, SPA
  0.5.577) — clicking a Documents search result *with a chat focused* threw
  NG0201 and did nothing: `OpenDocumentFromSearch` is `providedIn: 'root'`, so
  its injector never sees the Salon's component-provided `DocumentApi`; the lane
  had moved the lookup from render to click without fixing it, and both e2e
  beats run Home-focused while the unit harness stubs an injector that always
  answers. Fixed via `runInInjectionContext` (memoized, deliberately not
  registered globally), three mutation-proven TestBed guards, and a third e2e
  beat that ran RED against the pre-fix bundle. **Also proven live:** the
  archive surface end-to-end at every scope (incl. `preserve_file_names` proven
  by consequence — `instructions.md` survived a projection sweep — and
  P4.D120's `description` round-trip both directions), archived garments absent
  from the Green Room pool, the `archived: null` refusal and the unknown-action
  envelope writing nothing, the missing+invalid **404** and the archived
  200/409 asymmetry, the Documents chip over 4,924 links / 7,402 chunks with the
  fail-closed archived-vault exclusion, the in-chat scenario picker with both
  Host sentences byte-exact, the gallery modal's reparent, a real
  `docs --instance <TAB>`, and **two of Pascal's three remaining write paths**.
  **Measured, not filed:** exactly ONE `Quilltap General` on real data (the
  P4.D122 duplicate is a fixture property), and `systemHome` costs a steady
  **7.5 s** — the front door deserves its own look. **Still owed:** Pascal's
  **group** tier (needs a single-group chat), the Brahma deep-query budget,
  dedup/summaries, and the NanoGPT caching smoke / #101 cost question.
- **The `f3892158d` drift catch-up round (P4.D123→P4.D124 stacked ∥
  P4.D125): UNIFIED on main (2026-08-26) — ALL THREE CLOSED; the oracle
  baseline MOVES to `f3892158d`.** v4's jobs/activity-accounting rework +
  the whole realtime subsystem absorbed, with the round's settled
  mechanism divergence: the invalidation hints ride v5's EXISTING Event
  channel (engine broadcast → SSE `/api/events` → the Tauri pump) — no
  second WebSocket, per the locked boundary. Server: the total
  `JOB_TYPE_ACTIVITY` kind map (totality mechanical against BOTH v5's
  gate and v4's real enum), the in-flight activity registry (child-IPC
  legs NO-PORT; poll-scoped attribution — recorded NARROWER than Node's
  ALS across `tokio::spawn`; Drop-ends-the-span for cancellation), the
  jobs verb per the §A contract (`activeByKind`/`startedByKind` always,
  `activeByType` opt-in with the `|| includeJobs` quirk INSIDE the ported
  unit), 8 of v4's 10 span sites wired (the four no-surface rows held by
  an existence-tripwire census), the coalescing bus (host-armed — the
  core has no scheduler; no-op unarmed), the pure topic computation
  (73-case tier-1 family; an order premise REFUTED — `ChildWritePayload`
  kept v4's `{method,args}` shape, so ONE corpus drives both sides), all
  publish points 1:1 (the mutation pass exposed FOUR coverage gaps before
  confirming), and the terminal WS same-origin gate (v4's post-upgrade
  1008 framing measured then matched; the 19-case DB-free oracle caught
  the empty-Origin JS-truthiness arm on its first run). SPA: the chips'
  final state (adaptive 1.5 s/8 s factory-gated cadence, the
  `startedByKind` pulse; `notifyQueueChange` KEPT because v4 keeps it —
  the order's prose said retire, the code said keep), the realtime hub
  over the existing stream (per-leg NO-PORT table for the WS machinery;
  once-per-reconnect catch-up sweep), the topic map over v5's ACTUAL key
  spellings (chatKeys swept from ~30 raw sites first; `mountPoints` → []
  recorded), the shared clock + `nowMs` formatters, nine site migrations
  incl. v4's exact "Fallback polling (5s)" relabel. **The §3 review
  caught three would-have-shipped SPA findings, fixed red-first:** the
  hoist-cross-contaminated `year:` key in `formatRelativeDate`, the
  short-for-long weekday WITH the lane's spec pinning the divergence, and
  the regenerate cards' channel gate read untracked inside the
  function-form `refetchInterval` (the fallback could never re-arm on a
  mid-drain drop). **The activated hint beat's first live run caught a
  fourth enqueue site missing its publish** (v5's collection POST wrote
  the row API-side, bypassing the queue service's hint — fixed +
  census-pinned; the beat is its live wire proof). Riding the round: the
  chronic `ng` hang root-fixed (`ng-run.mjs` treats a spec BUILD failure
  as terminal for `test` — was a 30-min silent hang, now exit 1 in
  ~10 s), and thirteen wrapped-path tests took `ActivityTestGuard`
  (closing the structural counter race the one honestly-unreproduced
  workspace intermittent exposed). Gate: 469 test binaries / 2,514 / 0
  with the round's env block, zero SKIP; the four families fresh from the
  `f3892158d` pin with changed-bytes greps; clippy both feature sets;
  release build; ng 361 files / 5,398; full Playwright **252 passed / 0
  failed / 1 skipped** (the standing store-probe park). ⚠ v4 drifted TWO
  commits mid-round (`487ae57fe`, `561466cfe` — both NO-PORT? candidates
  in the drift ledger; ratifying them is the next round's cheap first
  item; pin `f3892158d` for every regen until then). 💸 the dogfood queue
  gains the chips over a real inline generation, the pulse, pushed
  invalidation with polling parked, the terminal origin refusal, and the
  relabeled toggle. Versions: core 0.0.688, harness 0.0.592, web 0.0.96,
  host 0.0.83, SPA 0.5.583. Round record: `status-log.md`.
- **The 4.9.0-push drift catch-up round (P4.D126 ∥ P4.D127 ∥ P4.D128 ∥
  P4.D129): UNIFIED on main (2026-08-27) — ALL FOUR CLOSED; the oracle
  baseline MOVES to `8872d7efc` and the fourteen-commit drift debt is
  CLEARED.** v4's whole 4.9.0 release push absorbed. The memory/backup
  trio red-first (the full-wipe chokepoint with its neighbour-scrub
  behavioural pin; the 900-id chunking at both `db/memories.rs` sites —
  a 40,000-id "too many SQL variables" failure measured pre-fix; bug
  103's shared legacy-column seeding with the NEW committed
  `restore-archive-legacy-profiles.zip` + the 306-case tier-1 family —
  which also fixed a pre-existing v5 `courierDeltaMode` default bug and
  found **a v4 REGRESSION inside `e000d6bfc` itself, FILED as v4 bug
  105** (v4 `b6c6d7793`): the seeding helper sits outside the per-item
  try, one malformed profile aborts a whole v4 import; v5 unaffected,
  pinned) ∥ the provider trio (bug 104's Z.AI vision-list drop
  red-first — corpus 341 → 343 with 339 rows byte-identical + the
  `glm-5.3-flash` rows; the 75 s compression budget with local-first +
  the `[CheapLLM] Task failed` warn under thread-scoped capture; the
  coalesce-trace silence pin) ∥ the client/CLI trio (the two solid
  hover utilities + the 20-site census with one pre-existing hover gap
  closed; the four completion flags Tier R red-first 3-by-name →
  188/0 + the token-level coverage guard mirrored; the About provider
  sentence + Live-interface bullet spec-pinned) ∥ the neutrality lane
  (415-family sweep, 410 green, 4 reds all dispositioned by pin
  sandwich, **NOT ONE attributable to `dcab791c2` — EXCEPT the
  measured 10/76 title-cleaner divergence no family could see**, landed
  at the unification wires: both v5 cleaners second-trim, red-first +
  a mutation-proven tier-3 arm; five NO-PORT ratifications; the
  blob-registry claim made executable; one vestigial wardrobe twin
  removed; nine recipe repairs + the `--nocapture` splice root-fix,
  regression-pinned at the wires). Also at unification: the
  finding-#47 web-edge tripwire RETIRED to a plain equality (v4
  converged at `13ddc5ee`; the standing "URGENT with the human" note
  is DISCHARGED), and the `backup_uuid_remap` neutrality gap closed by
  a byte-identical baseline-vs-target sandwich (corpus refreshed for
  pre-existing 4.8.2 staleness). **The §3 review: ZERO blocking
  findings** (four parallel reviewers + the unifier's reads; the one
  real minor — the census's two unrecorded sibling hover gaps — fixed
  at unification). Gate: 471 test binaries / 2,554 / 0 with the
  round's env block; the 15-family pinned sweep 15/15 zero SKIP with
  changed-bytes greps; clippy both feature sets; release build; ng 361
  files / 5,399; full Playwright **252 passed / 0 failed / 1 skipped**
  (the standing store-probe park). 💸 the dogfood queue gains the
  bug-103 seeding on a real pre-4.9 archive, the glm-5.3 wire proof
  (REPLACING the retired Z.AI refusal-sentence item), the 75 s
  compression fold + warn line, the About strings, live three-shell
  completion, and the two hover fills. Versions: core 0.0.696, harness
  0.0.598, web 0.0.97, cli 0.0.14, SPA 0.5.586. Round record:
  `status-log.md`.
- **The P4.D130 ∥ P4.62 ∥ P4.63 ∥ P4.64 round: UNIFIED on main
  (2026-08-27) — ALL FOUR CLOSED; the oracle baseline MOVES to
  `aec86a613`.** The `aec86a613` outfit pull-down whole (the pool-split
  twin with v4's 7-case transcription 1:1 PLUS a nine-case recorded-vector
  corpus that asks the ICU questions the transcription cannot — mutation-
  proven; the capture-phase-Escape pull-down; garments-only slot pickers
  with the `allItems`-passed-whole chip pin written RED first; the live
  dissolution beat) + both carried wardrobe e2e debts (the missing
  `instance_settings` MATERIALIZED — not a fixture regen, six families
  spared — with the create-scope beat LIVE and the transfer beat re-parked
  on its REAL blocker, named; the duplicate "Quilltap General" root-caused
  to the courier seeding — NOT the provisioner, measured idempotent — and
  reconciled by what each store holds, `sameName=1` + a standing tripwire)
  ∥ P4.62: the last three wrong-type-collapse pockets adjudicated whole
  (13+7+1, zero unadjudicated census rows; 11+3 FAITHFUL / 2+3
  DIVERGENT-FIXED incl. the Zod `validationError` envelope, the
  `zod_uuid` gate transcribed from Zod 4's own regex, the whole
  `writeBodySchema`, the `system/unlock` body gate that used to let `42`
  through to a passphrase change, and the per-action malformed-body 500s;
  two new families driving v4's REAL handlers over real HTTP, 15
  mutations; three escalations with ordered shapes) ∥ P4.63: the four
  harness follow-ups (the bug-105 divergence-aware oracle arm — **which
  v4 then fixed HOURS later (`679e450e3`), so the arm's convergence trip
  at the next baseline move is already booked by design**; the
  attach-mount-file red = bug-91 corpus vintage, profiles → OPENAI,
  canned calls 0 → 4 with a per-case vision-rung pin; the deadline-warn
  assert bound to one line with its vacuity MEASURED; both blob censuses
  comment-aware, the whole-file exemption now per-site) ∥ P4.64: the
  7.5 s dashboard profiled at real scale — **the standing hypothesis
  refuted: 97% was `enrich_chats_for_list`'s per-participant vault
  fan-out, a dropped-preload PORT DEFECT** (v4 batches up front;
  the-differential-cannot-see-a-dropped-batch class) — fixed
  payload-identically (sort-then-slice; dispatch payload byte-equal at
  real scale, **8.8 s → 0.39 s, 22.5×**; `home_routes_equivalence` 14/14
  discriminating); **the Salon list pays the same 8.6–12.2 s and needs
  v4's `ChatListPreloaded` batching — the named next candidate with this
  measurement as its justification.** The §3 review: NO blocking findings.
  v4 drifted THREE times during the round (`679e450e3` CONVERGENCE,
  `0bd841394` tooltips PORT-NEW, `1b0ce9eba` cleanup) — every regen
  pinned, the ledger updated mid-unify and at the move; the catch-up is
  the top next candidate. Gate: 473 test binaries / 2,557 / 0 with the
  five pin-fresh families zero SKIP (changed bytes grepped); clippy both
  feature sets; release build; ng 364 files / 5,435; full Playwright
  green (numbers in the round record). Versions: core 0.0.698, harness
  0.0.602, web 0.0.98, SPA 0.5.590. Round record: `status-log.md`.
- **The `b121ac77f` drift catch-up + chat-list-batching round (P4.D131 ∥
  P4.D132 ∥ P4.D133 ∥ P4.65): UNIFIED on main (2026-08-27) — ALL FOUR
  CLOSED; the oracle baseline MOVES to `b121ac77f` and the four-commit
  drift debt is CLEARED.** The bug-105 divergence arm retired on a
  measured FULL convergence (v4's post-fix leg byte-for-byte v5's
  long-standing assertion; the retirement measurably WIDENED coverage —
  the formerly-subtracted table now discriminates, mutation-proven) ∥
  the Tooltip vertical whole (the Angular primitive with v4's exact
  timing/flip/clamp/pin semantics + a NEW measured trap — a reparented
  node outlives its `@if` view; all nine action-bar buttons adopted with
  byte-exact copy incl. the re-attribute fix; **the ConfirmationBadge
  landed NET-NEW** — v5 had only its CSS, and the mapper had been
  dropping `confirmationOriginalContent`; the `1b0ce9eba` deletions; two
  live beats; suite 254 → 256) ∥ `instances restore-key` end-to-end
  (Tier R red-first 188/4 → **212/0** vs v4's REAL launcher incl. both
  destructive state blocks and the cross-engine sqlite-message byte
  risk verified; three new core dbkey seams with the P4.46 divergence
  doc RESCOPED; 💸 the real-pepper recovery walk banked, human-only) ∥
  the Salon chat-list `ChatListPreloaded` batching (the four missing
  batch paths, chunked; the drop-vs-503 vault arm CONVERGED onto v4 and
  pinned; payload-proven byte-identical on the Friday copy —
  **4,104,806 bytes md5-equal; enrich 12,984/8,256 → 2,227/1,451 ms,
  ~5.7×**; the widened fixture + a 30-object key-order pin). **The §3
  review found no blocking findings in any lane; the unified Playwright
  gate then caught the round's would-have-shipped defect no lane could
  see** — the widened fixture's broken-vault chat sorted FIRST (every
  position-based beat walked into the v4-faithful 503) and its broken
  character became the archive seeder's tie-broken copy template
  (Marchpane dropped by the roster overlay) — repaired fixture-side
  with zero product code, plus the `try_decrypt` IV-length panic
  guard, the fixture sort-key pin (loud builder throw), and three
  action-bar fidelity gaps (Delete danger chrome, swipe disabled
  utilities, the `2/3` counter bytes), all spec-pinned. Gate: 473 test
  binaries / 2,585 / 0; Tier R 212/0; ten pinned regens zero SKIP;
  clippy both feature sets; release build; ng 366 files / 5,458; full
  Playwright **255 passed / 0 failed / 1 skipped** (the standing
  store-probe park). Versions: core 0.0.701, harness 0.0.603, cli
  0.0.16, web 0.0.100, SPA 0.5.596. 💸 the dogfood queue gains the
  tooltips + pinnable badge, the Salon list's speed, and the
  restore-key recovery walk. Round record: `status-log.md`.
- **The P4.D131-round dogfood pass RAN (2026-08-27, agent-driven, on the
  Friday copy) — 22 rows, 21 PASS, ZERO v5 defects found by the walk, sixteen
  💸 items discharged across four rounds** (A9 + C5 + C4 human-side
  2026-08-28/29). Walk doc:
  `dogfood-walks/2026-08-27-tooltips-salon-speed-pass.md`; record in
  `status-log.md`. **The ledger was STALE at walk start** — `/driftcheck`
  ran first (`11edb1c6`) and found 2 commits past the baseline
  (`1560bd43b` PORT — v4 retires Lima/WSL2 across six ported surfaces incl.
  **deleting `isVM`** from `/api/v1/system/data-dir`; `7819afb1d` NO-PORT?);
  **regen rule flipped to PIN REQUIRED.** Discharged: the Salon list at
  real scale (**779 chats / 4.1 MB / 1.34 s** vs P4.64's measured
  8.6–12.2 s) and `systemHome` (**0.31 s** vs 8.8 s); the whole tooltip
  vertical (nine anchors, zero `title`s, no body-node accumulation) plus
  **three branches the plan never listed** — `focusin` opens at **13 ms**
  against the 200 ms dwell, `focusout` closes, outside-pointerdown
  dismisses a pinned bubble; the net-NEW ConfirmationBadge over a measured
  population (5,736 confirmations — vouched 5,544 with **0** detail /
  amended 164 all-detail / stood-by 28 all-detail, which *is* the
  pin-gate's justification); the IV-length guard **end-to-end through the
  CLI with NO pepper** (3-byte IV and 16-byte junk control byte-identical,
  no panic); all four realtime items (chips moving on real work; **pushed
  invalidation proven by discriminator** — 0 app fetches over 17.4 s idle,
  then 13 within 12.7 s of curl-fired jobs the browser could not have
  known about; the WS origin gate correct on **all eight arms** against a
  real PTY; the relabel); the two hover fills; the About strings; and
  **all three completion templates byte-identical to v4's REAL launcher**
  plus a real `<TAB>`. **Four apparent defects were chased to root cause
  and none was real** — the surviving `title=`s are v4's own
  (`TokenBadge` was never converted), `startedByKind` flat during
  background jobs is `runAttributedToJob` on BOTH sides (the pulse fires
  for inline work), the WS arms all reading `1000 Session not found` was a
  bogus session id, and two were **instrument error**. Four instrument
  slips in one pass (a 2 px hover miss, a synthetic `pointerenter` with no
  `pointerleave`, a liveness check on the *original* `fetch`, a 60 ms sleep
  that took 1103 ms across the bridge) keep **prove the instrument before
  trusting a negative** as the standing rule. **`restore-key` with the real pepper CLOSED
  human-side (2026-08-28)** — server down, no `--force`, so the proof arm
  the agent skipped ran: all three partitions `opens with this pepper ✓`
  before the write, 42 characters read back after. **Bug 104's glm-5.3
  vision send also CLOSED human-side** — a 1.8 MB JPEG read by
  `glm-5.3-flash` (a model id with no `v`), with **zero
  `IMAGE_DESCRIPTION` rows in the window**, so no describe-fallback ran.
  **The 75 s compression budget CLOSED PARTIAL** — three v5 calls
  (30,080/26,633/25,459 ms) on the remote cheap LLM prove production picks
  the 75 s branch; the discriminating 40–75 s band is provider-latency luck
  (18 of 397 historical calls) and the `[CheapLLM] Task failed` warn needs
  >75 s, **never once crossed in 400 real calls** — both unit-proven
  instead. Two corrections banked: compression fires on context PRESSURE
  (`compressible_tokens > max_available × 0.50`), not conversation length —
  the first target's characters sat on 1,024,000-token windows, ten times
  over the bar — and duration does NOT track prompt size (13.0 s @ 287 KB
  vs 30.1 s @ 242 KB). **Still owed:** Pascal's group tier, the Brahma deep
  query, and dedup/summaries + the NanoGPT caching cost question (#101). **⚠ Post-walk, finding #106 RECORDED (2026-08-29, NOT
  fixed — needs an order): the user's own message renders TWICE for most of
  a multi-character turn.** v4 keeps the optimistic bubble INSIDE the
  message array so a refetch replaces it; v5 holds it in a separate signal
  appended at render and clears it only at turn end — latent until
  P4.D123–D125 started refetching the chat mid-turn
  (`CHAT_DANGER_CLASSIFICATION` completed six times in four minutes on the
  live instance). **The whole Playwright suite is green through it**,
  because every beat asserts the POST-turn transcript and the defect is
  strictly mid-turn; the owning lane's first deliverable is that missing
  gesture. **Finding #107 also RECORDED (2026-08-29, not fixed): the
  Markdown formatting toolbar overflows its column on BOTH sides** (New
  Chat's scenario field) — the CSS is byte-identical to v4's, but v5
  interposes `<qt-markdown-field>` whose host class has **no rule anywhere**,
  so it renders `display: inline` and constrains nothing. **Third instance
  of the inline-host family** (after #97's `qt-tab-view` and the Almanack
  walk's `qt-entity-tabs`), **20 call sites**; the standing note proposes a
  guard over every `host: { class: 'qt-…' }` without a matching CSS rule.
- **The drift catch-up round 1 of 2 (P4.D134 ∥ P4.D135→P4.D136 ∥ P4.D137):
  UNIFIED on main (2026-09-01) — ALL FOUR ORDERS CLOSED; the oracle baseline
  MOVES to `7fb668263` and the round's eight-row drift prefix is CLEARED**
  (eight commits remain — the pre-planned round 2: the LoRA train ×3, bug
  112, the Concierge four-state, `qt-range`, two docs rows). The Lima/WSL2
  retirement whole (env/lock/CLI with Tier R red-first 212 → 214/0, the
  data-dir `isVM` wire deletion with two renamed deletion pins, the
  host-rewrite two-strategy collapse, self-inventory/almanack retirements,
  the SPA About mirrors + Discord rider, the grep census; **one follow-up
  opened: v5 has never had a host gateway resolver** — measured, named in
  `rewrite.rs`) ∥ provider/model fallback chains END-TO-END (the two
  `connection_profiles` columns through the D23 re-dump — the order's
  column position was WRONG, generateDDL places them after `modelClass`;
  the pure engine tier-1 at 158 cases; both Salon entrances + cheap-LLM +
  image description; both id-remap paths; the delete-nulls cascade WITH
  v4's `updatedAt` stamp; the SPA understudy picker + live round-trip
  beat) ∥ bugs 106/107 (v5 measurably HAD bug 106 in both halves, proven
  red-first; the budget rewrite with the latency class threaded from 45
  call sites, the timeout-only retry, five of six handler guards —
  scene-state deferred loud; the recap ceiling's compile-pin FIRED as
  designed, and the outfit consult's inversion measured as v4's own and
  reproduced) ∥ bugs 108/109 (both proven red-first — v5's bug-108 coat
  silently DELETED the found span where v4 spliced `"undefined"`; the
  25-entry fold table entry-for-entry; the rebuilt per-UTF-16-unit
  diacritics map; the 5/25 replay split executable). **The §3 review (four
  parallel reviewers): ZERO blocking; four groups fixed at unification —
  headline: the `[CheapLLM] Task failed` warn fired AFTER the chain where
  v4 warns BEFORE it** (a rescued task still counts — the very counter bug
  107 was measured from; capture-pinned, mutation-proven); the
  failing-over toast now re-fires on a message change (the second
  stand-in's name is news; the branch gained its first specs); three
  classifier ladder-order rows; the doc-text guard-placement ops moved off
  `.yaml` (a SUPPORTED text format — the discriminator was vacuous both
  sides) onto `.png` with insert's own mutation-proven placement op. Gate:
  23-family pinned sweep + uuid-remap's replay leg zero unexplained SKIP;
  Tier R 214/0; 475 test binaries / 2,632 / 0 zero SKIP; clippy both
  feature sets; release build; ng 5,477/0 + build clean; full Playwright
  green (numbers in the round record). Versions: core 0.0.719, harness
  0.0.616, host 0.0.86, cli 0.0.17, SPA 0.5.600. 💸 the dogfood queue
  gains the dead-endpoint understudy walk, the reroute-with-an-image +
  re-measured compression row (the 75 s C4 numbers are SUPERSEDED), the
  live curly-quote resolve, and the stand-in toasts. Round record:
  `status-log.md`.
- **The round-2 drift catch-up (P4.D138 ∥ P4.D139 ∥ P4.D140 ∥ P4.D141 ∥
  P4.D142 ∥ P4.66): UNIFIED on main (2026-09-01) — FIVE CLOSED, P4.D138
  OPEN at units 5–7; the oracle baseline MOVES to `4622411fd` (v4 HEAD —
  zero drift) with the LoRA train's three ledger rows PARTIAL.** The LoRA
  train's client half whole + server units 1–4 (the model matchers + LoRA
  support resolver over a 101-row tier-1 family with the JS-`.` class
  measured; the `loras` write guard answering v4's Zod ENVELOPE through a
  new `CoreError.details` carry; the params builder + the five-site
  consolidation — v5 measurably HAD v4's "three sites read only quality"
  drift, and the widened corpora found a second pre-existing defect, the
  tool-input schema DEFAULTS v5 never applied; the NanoGPT dialects recorded
  at the commit-1 pin with bug 110 PRE-fix by name; the manifest regen);
  **units 5–7 OPEN** (bugs 110/111, the `list-models` `loraSupport` read
  side + `options-schema` + the catalog cache, the HuggingFace
  `lora-metadata` lookup) — the routes family strips v4's key behind a
  MEASURED tripwire, the SPA's beats stay gated, and its options-schema
  fetch 400s silently into the legacy panel until they land ∥ bug 112 whole
  (`chat_activity` chokepoint — the in-memory truthiness and SQL `IS NULL`
  spellings mirrored, not unified, the `''`-sender seam MEASURED; both
  write sites red-first; the six readers; restore re-deriving from the
  replayed transcript; the ai-import twin NO-COUNTERPART; the boot recompute
  heal in the P4.D97 ledger shape — a no-drift boot writes NO row, the
  cross-app hazard; the four SPA flips; the e2e seed landmine; plus the
  `allowCheapFallback` P4.D135 remainder fixed out of mandate) ∥ the
  Concierge four-state whole (the predicate family reshaped at every call
  site with the two overloaded predicates DELETED as v4 deleted them; the
  resolver's operator arms; the flips + five sentences byte-exact; the
  `conciergeState` PUT arm closing v5's long-named deferral with the
  guard order and `double_option` tri-state pinned; the classifier-gate
  corpora that can finally SEE the gate — both families were green on a
  reverted gate before; the SPA control in v4's slot, the single-pill
  badge, the client twin, the four-state walk LIVE at unification) ∥
  `qt-range` byte-identical across all twelve v5 range hosts + finding
  #107's `qt-markdown-field` rule + the host-class guard at the ordered
  NARROW scope ∥ finding #106 FIXED with the suite's first mid-turn
  observation beat (the realtime hint injected at the wire through the
  app's REAL `EventSource` handler; 12/12 samples duplicated pre-fix).
  **The §3 review (five parallel readers) fixed six groups — three would
  have shipped:** the sidebar select's PERMANENT optimistic latch (a
  refetch, an auto-flip or another tab could never win after the first
  pick; v4 derives from props and re-applies — now the P4.D115 idiom,
  pinned both ways), the bubble-echo predicate scoped across two CLOCKS
  (browser vs server — the Docker deployment splits them; now an id
  snapshot of the rows on screen at send time), and
  `post_office_writers_tier3` silently BROKEN by the kind rename (its
  fixture drove the retired strings; the lane record credited coverage to
  a family that never mentions them); also v4's `safeQuery` FALLBACK at
  both new last-played reads, `to_key_value`'s orientation-inserted `size`
  slot (corpus-blind until the new row), the qt guard's one-line-header
  regex + the ordered self-test that had not landed, the modal's
  providerKey dep, and byte/doc repairs. **The gate's own catch:** v4's
  `overflow-hidden` markdown frame clips its OWN toolbar pickers (v4
  filing candidate; v5 keeps the frame without it, recorded). Gate: the
  36-family sweep 36/36 zero SKIP from the `4622411fd` pin; **477 test binaries / 2,655 passed / 0 failed / 1 ignored, ZERO `SKIP:` lines — exit 0** (the first full run stopped fail-fast at binary 26 on `avatar_job_tier3`, the second at `image_generation_tier3` — the key-mirror catch and the `/tmp/qt-imggen-*` pair collision recorded above; `image_generate_route_equivalence` shares that pair AND its env-var names with the tier-3 family, so its oracle var was withheld from the block and it ran GREEN by name against its own snapshot under `/tmp/unify-r2/route/`);
  clippy both feature sets; release build; ng 373 files / 5,782; full
  Playwright **259 passed / 0 failed / 3 skipped** (the standing store-probe park + the two D138-gated LoRA beats; the suite grew 256 → 262 with the two LoRA beats, the four-state walk, the two mid-turn bubble beats and their siblings — the first run went 257/2/3 on the two gate catches above, both repaired and re-run whole). Versions: core 0.0.732, harness 0.0.626, host
  0.0.89, web 0.0.101, SPA 0.5.614. 💸 the dogfood queue gains the bug-112
  boot recompute on the Friday copy (measure the population FIRST), the
  four-state walk on a real chat, the Uncensored route without danger
  paint, the themed sliders, the clock-free mid-turn bubble. **Next: finish
  P4.D138 (units 5–7), then the owed dogfood pass.** Round record:
  `status-log.md`.
- **The P4.D138 follow-up (units 5–7, the resumed LoRA-train lane): UNIFIED
  on main (2026-09-01) — P4.D138 CLOSED WHOLE; the drift ledger's §3 is
  EMPTY; the baseline stays `4622411fd`.** Bug 110's family-first
  `apply_loras` with the corpus re-recorded at the tip (exactly the two
  predicted rows moved) + bug 111's error-level request log and v4's debug
  line, capture-pinned; the `list-models` `loraSupport` map, the
  `options-schema` action and the NanoGPT detailed-catalog cache (the unit-1
  narrowing RETIRED at source; the round-2 tripwire FIRED as designed and is
  deleted; the two SPA LoRA beats LIVE — their first run corrected
  `LORA_MODEL` to a declaring family and fixed three gestures); the
  HuggingFace lookup + `lora-metadata` behind an engine gate and the host
  transport, over a 57-row differential carrying the canned wire per row
  (one recorded divergence: V8's own `SyntaxError` wording). **The §3 review:
  NO blocking findings; five fidelity items fixed on the unify branch** —
  the bug-111 line fired on the malformed-2xx arm v4 excludes (and said it
  did not), both log lines printed the raw model where v4 posts `hidream`,
  the `new URL()` stand-in mis-parsed four WHATWG shapes its doc called
  unreachable (six corpus rows added, v4 agreeing; mutation-proven), the
  host transport read the body before the status decided, the over-cap beat
  passed with zero flags. Gate: 9/9 families fresh at the baseline zero
  SKIP; **479 test binaries / 2,665 passed / 0 failed / 1 ignored — exit 0** with the lane-scoped env block (the eight affected families' recipe vars plus the HuggingFace family; the untouched families' oracle vars deliberately withheld — their /tmp oracles were retired at the round-2 cleanup hours earlier and they were proven at that gate on main; a first run with the stale block failed `brahma_console_routes` on a missing file, the recorded "deleted-path reads like a regression" trap; cargo captures a passing test's SKIP line, so their silence is the capture, not a claim — the affected families' positive proof is the by-name sweep above); clippy both feature sets; release build; ng 373 files /
  5,782; full Playwright **258 passed / 3 failed / 1 skipped** in the full run (the skip is the standing store-probe park; the two LoRA beats LIVE and green) — the three reds are `salon-documents-flow` ×2 and the `workspace-flow` terminal pop-out, Document-Mode/terminal surfaces this lane never touches, the same three the lane record classified, green twice earlier today in this session's full runs and **18/18 green re-run in isolation** — the standing full-suite intermittent class, recorded, not this lane. Versions: core 0.0.736, harness 0.0.630,
  host 0.0.91, SPA 0.5.615. 💸 the dogfood queue gains the LoRA editor on a
  real NanoGPT profile end to end (a real Query against HuggingFace is the
  one arm no test may exercise). **Next: the owed dogfood pass** — see
  phase-4.md.
- **The round-2 + P4.D138-follow-up dogfood pass RAN (2026-09-02,
  agent-driven, on the Friday copy) — 20 rows, 18 PASS, ONE finding found and
  FIXED, and the round's whole 💸 queue discharged.** Walk doc:
  `dogfood-walks/2026-09-02-round2-lora-concierge-pass.md`; record in
  `status-log.md`. **The ledger was STALE at walk start** — `/driftcheck` ran
  first (`28245beb`) and found ONE commit past the baseline (`70505745a`
  **PORT** — v4 keeps Absent/removed participants out of story backgrounds
  and retires two project background modes); **the regen rule flips to PIN
  REQUIRED** and the catch-up is the next candidate. **FIXED: finding #108** —
  the image-profile editor named the wrong provider (a real NanoGPT profile
  read *OpenAI* beside its NanoGPT key, model and options panel; **11 of 14**
  profiles on real data). The Provider select's rows come from an `@for` over
  an async list while the value was bound `[value]`, so Angular's binding
  landed before the options existed and the browser settled on row 0 — the
  controlled-select class **the same file had already fixed twice** for Model
  and Size. v4's React re-applies `value` on the render that fills the list.
  Display-only (a round trip wrote `NANOGPT` back), fixed with a third
  `afterRenderEffect`, four specs mutation-proven, and the live LoRA beat
  gaining the missing assertion (`b11dce1a`, SPA 0.5.616; Playwright
  **261/0/1**). **RECORDED: #109** — #107's *cause* is closed but its
  *symptom* survives: the formatting toolbar still overhangs by 62.9 px a
  side, because `.qt-formatting-toolbar` is byte-identical to v4's and v4 only
  hides it with the `overflow-hidden` v5 deliberately omits to keep the
  pickers reachable; a **v4-first filing**. **💸 discharged:** the LoRA train
  end to end incl. **the live HuggingFace query** (the round's named owed
  proof) and the write guard's Zod envelope (the order's premise corrected —
  over-cap is a client FLAG, malformed entries are what refuse); the bug-112
  boot recompute in BOTH arms, with a **free cross-app proof** — v4 had
  already written the ledger row, so v5 honoured it and healed nothing, then
  healed exactly the measured 13 once it was removed, then wrote **no** row on
  a no-drift boot; the four-state Concierge on real `UNCENSORED`/`OFF` chats
  with all ten sentences byte-exact and the PUT's 404-beats-400 guard order;
  the Uncensored route measured three ways (extraction reroutes, recall does
  not, the stream keeps its seat — v4's call-site map exactly); the themed
  sliders; the clock-free mid-turn bubble (67 samples, never above 1); the
  dead-endpoint understudy walk with its stand-in toast; and the live
  curly-quote resolve across three fold classes at once. **Still owed:** the
  `[CheapLLM] Task failed` warn ordering, the reroute-with-an-image +
  re-measured compression row, Pascal's group tier, the Brahma deep query,
  dedup/summaries, #101, and a LoRA **wire-byte** look (`llm_logs.request` is
  a pre-builder projection; `wire-tap.py` cannot tap HTTPS). **Four instrument
  errors were caught and recorded** — a `unicode_escape` false DIFFERS, a
  leaf-text scan counting the composer as a bubble, **composition mode**
  swallowing two sends outright, and a `--` needle for an em dash the table
  folds to one hyphen.
- **The `6d2a50382` drift catch-up round (P4.D143 ∥ P4.D144 ∥ P4.D145 ∥
  P4.D146 ∥ P4.D147): UNIFIED on main (2026-09-02) — ALL FIVE CLOSED; the
  oracle baseline MOVES to `6d2a50382` and the drift debt is CLEARED.** v4's
  six-commit day absorbed whole: the Concierge list marks (server: the
  derived `conciergeState`/`dangerCategories` pair on all four list payloads
  at v4's slots, `concierge_state_uses_uncensored_route`, the per-turn
  `CHAT_DANGER_CLASSIFICATION` enqueue gated on the classifier being on
  duty — red-first, the "six times in four minutes" symptom — and the
  `has-dangerous` probe v5 never had; SPA: the presentation table ONCE in
  the SPA diffed against v4's module EXECUTED at the sha, `ConciergeMark`
  over the Tooltip, `shouldHideChat` as the one quick-hide rule with the
  P4.9d non-port ruling retired) ∥ bug 114 (the ledger's "D23 re-dump"
  premise REFUTED — v4's `generateDDL` cannot emit an expression index; the
  unique index arrives through an index-guarded collapse boot ensure with NO
  ledger row, `ensure_by_path` over seven sites with two private lookups
  deleted, the restore quiet-drop arm; Friday measured intact at 607 rows /
  24 folders) ∥ absent participants out of story backgrounds + the
  background-mode normalizer at the overlay parse (the ONE chokepoint —
  restore needed nothing, proven by mutation) ∥ bug 113 (v5 had NO folder
  picker; v4's post-fix one built fresh, live beat). **The §3 review: NO
  blocking findings** (the fourth such round); nine should-fix items fixed
  — headline: v4's `limit` is a `parseInt` PREFIX parse where the new chats
  collection GET used Rust's whole-string parse, and its list leg leaked the
  verb's error where v4 answers a fixed sentence; v4's dropped
  click-passthrough case transcribed. The activated D144 beat's first live
  run caught its own seeding reading `data.chats` off an array response.
  Gate: 33/33 families fresh at the new baseline zero SKIP; 484 test binaries / 2,694 / 0 zero SKIP; clippy both feature sets; release build; ng 376 files / 5,883; full Playwright **268/0/1** (the standing store-probe park). Versions: core 0.0.750, harness 0.0.642,
  web 0.0.103, host 0.0.92, SPA 0.5.623; cli/tauri unchanged. 💸 **ALL SIX
  items DISCHARGED by the 2026-09-02 dogfood pass** (below). Round record:
  `status-log.md`.
- **The `6d2a50382`-round dogfood pass RAN (2026-09-02, agent-driven, on the
  Friday copy) — 22 rows, 22 PASS, ZERO v5 defects, and the round's whole 💸
  queue discharged.** Walk doc:
  `dogfood-walks/2026-09-02-concierge-marks-folders-pass.md`; record in
  `status-log.md`. The ledger's §2 probe **passed** at walk start, so no step
  had the "it may be the drift" excuse. **The pre-walk measurement killed one
  banked proof and bought two better ones (ledger §5.5):** `folders` held **24**
  rows, not P4.D145's 607 — v4 ran its **own** bug-114 collapse hours earlier
  (`583 → 24`, *exactly* the shape `folders_collapse_heal_equivalence`'s Friday
  scenario asserts, a free cross-implementation agreement). So v5 was proven
  instead by (a) booting on v4's healed DB writing **nothing** — the ledger
  still holds only v4's row, byte-unchanged, which is the port's deliberate
  no-ledger-row design meeting a real cross-app ledger — and (b) collapsing a
  **planted** set (`scanned=30 surviving=26 deleted=4 repointed=1`) whose child
  `parentFolderId` was repointed onto the **survivor**, oldest-`createdAt`
  winning on both the NULL and `COALESCE(projectId,'')` legs. **Proven on real
  data:** the Salon's **73 Flagged / 10 Vouched / 2 Uncensored** marks matching
  the DB row for row across all four §A payloads; the hide delta **799 → 724,
  exactly −75**, with ⭐ **all three `OFF`+`isDangerousChat=1` chats surviving**
  (the pre-fix raw-label rule would have hidden them — `c43d3b1b4`'s whole
  point); the footer's **third arm isolated** (no hidden-tag key and no
  `hideDangerous` key at first open, so only the live `chatsHasDangerous` probe
  could keep the section visible); the enqueue guard as a **same-chat A/B** with
  both other guards held open; the absent-participant gate on three chats
  (payload filtered, scene context intact, **back-fill side door closed**,
  silent counts as present, nobody-present refusing byte-exact); and the folder
  picker's four option lists matching the DB exactly, with real data supplying
  the nested `[160, 160, 9492]` indent for free and a re-create returning the
  **same ids** (the `ensure_by_path` cutover, invisible to every sequential
  differential). **Three §3-review fixes proven live:** `?limit=1abc` → exactly
  1 of 799 (`parseInt` PREFIX parse), a paused offline query falling through to
  Root (`isLoading`), and the `modeLabels` toast reading a real label. **Three
  corrections, none an app bug** (walk Findings + `dogfood-findings.md` Standing
  notes): the mark draws for **all three** non-Monitored states; a Flagged chat
  is **sticky, never re-checked**, so it cannot be the enqueue positive arm; and
  the standing "store-overlay properties cannot be SQL-seeded" note is too
  strong — the plant works when `contentSha256`/`plainTextLength` **and** the
  file row's `sha256`/`fileSizeBytes` move with the content (that is how the
  retired-mode project, absent from real data, was posed). **Deferred with its
  recipe:** Pascal's **group** tier — the effects cascade searches chat →
  project → group for a key that **already exists**, so it must be pre-seeded
  via `groupStateSet`, and the chat must satisfy `groupTier.status == "single"`.
  Still owed: the re-measured 90 s/120 s compression row, the Brahma deep query,
  dedup/summaries, #101, and the LoRA wire-byte look (blocked).
- **The follow-ups round (P4.67 ∥ P4.68 ∥ P4.69 ∥ P4.70 ∥ P4.71): UNIFIED
  on main (2026-09-02) — P4.68/P4.69/P4.70/P4.71 CLOSED, P4.67 PARTIAL
  (its header names the remainder); the baseline STAYS `6d2a50382`, with
  THREE UNPROCESSED drift rows + three open v4 filings (116–118) in the
  ledger.** The first non-drift round since P4.59, unified under PIN
  REQUIRED after v4 moved twice mid-round. Landed: the one query-parameter
  reader for every REST edge with v4's three real action-dispatch shapes
  (FIRST wins, `?action=` folds, v4's envelopes byte-exact; 79 of 98 new
  rows red before the rewrite) ∥ the participant-status parsers consolidated
  with one fidelity fix, the failover legs' `llm_logs` rows + run-id context,
  the bare-executor gap closed by census, `precompute_equivalence` finally
  seeing the uncensored reroute, the vintage-stale `episodic-recall-*` pair
  rebuilt ∥ v4's assistant-avatar danger ring (the CSS was dead), the
  invented quick-hide warn retired, the modal's parameters as an object, the
  fragile beats repaired and the component-transfer beat UN-PARKED — the
  Playwright suite is zero-skip ∥ the whole `generate_image` schema as v4's
  Zod parse (v5 generated and SAVED images v4 refuses), the `[Image LoRA]`
  caller context, the `system-data-*` fixture migrated in place through
  v4's own schema translator (the connection-profile import leg measured
  nothing since bug 68) ∥ the host gateway resolver injected at every
  provider construction site (57-row tier-1 family). **The §3 review: TWO
  blocking findings, both P4.67, fixed at unification** — the subset edges
  advertised actions they refused, and the coverage claims exceeded the
  code — plus seventeen should-fix items (headline: a wiring census a
  faithful retry would have reddened; a production-zone census ending at a
  mid-file `#[cfg(test)]`; a refetch mark that was always zero; the
  image-profile route running the TOOL's schema where v4's ROUTE refuses;
  the Ollama double slash v5 had "repaired"). The gate's own catches: a
  committed recipe naming a `/tmp` pin; a stale `lastMessageAt` arm from
  before bug 112; a pre-existing Pascal fixture vintage rot (recorded).
  Gate: 43 + 7 + 61 families fresh from the pin, zero SKIP; clippy both feature sets; release build; ng 376 / 5,911; full Playwright 270/0/0 (zero-skip); **488 test binaries / 2,745 passed / 0 failed / 1 ignored — exit 0, ZERO `SKIP:` lines**. Versions: core 0.0.758, harness 0.0.654, web
  0.0.104, host 0.0.94, SPA 0.5.628. 💸 the dogfood queue gains the danger
  ring, the subset refusals via a v4-shaped client, the Docker Ollama walk
  (+ one `docker build` on a quiet machine), the modal's writers on a real
  NanoGPT profile, `count: 20` through the image-profile route, the
  failover rows on a real understudy. **Next: the three-row drift catch-up
  (`303288fb4` + bug 115 + the timing log), then the dogfood pass** — see
  phase-4.md. Round record: `status-log.md`.
- **The `0b0617fee` drift catch-up round (P4.D148 ∥ P4.D149 ∥ P4.D150 ∥
  P4.D151 ∥ P4.D152): UNIFIED on main (2026-09-03) — ALL FIVE CLOSED; the
  oracle baseline MOVES to `0b0617fee`** (the `15573c3a1` bug-119 row stays
  UNPROCESSED by design — an unported surface, `p4.9k`). v4's five-commit
  day absorbed whole: the Concierge state chosen at chat creation end-to-end
  (the flip through the existing chokepoint on all three create branches,
  the greeting's attempt 0 on the uncensored desk asked WITH the chat row,
  the capstone corpus 19 → 32 with `message_order` + `stream_calls`
  comparands and the harness api-key seam that had made every reroute
  unreachable; the SPA dropdown + body rule + the create-time beat LIVE;
  Continue Elsewhere seeding a NO-COUNTERPART) ∥ bug 115 + the timing log
  (pinned at the real call sites — the corpus is provably blind, the oracle
  byte-identical across three pins) ∥ bug 116 (the arrival verdict ahead of
  every content check; `CompletionResponse.cache_usage` at 23 sites; 6 of 8
  new tier-3 rows red first) + bug 118 (a no-op, re-proven: eleven manifests
  byte-identical) ∥ bug 117 (transcode-then-hash with the codec as a
  parameter — production keeps the not-configured passthrough; import + both
  restore arms; the boot heal in the P4.D140 ledger shape with a
  both-directions divergence pin on v4's presence-not-drift stamp; the
  within-tree boolean comparand + a harness byte-changing codec, the DEDUP
  being the red-first arm). **§3 review: NO blocking findings** (the fifth
  such round); fixed at unification: the heal folding every DB error into
  the orphan bucket, a parity-claiming boot comment, three create-path shape
  items, a spliced doc, a stale field comment. Gate: 26/26 families fresh at
  the pin zero SKIP; clippy both feature sets; release build; 489 test binaries / 2,761 / 0 with the round's env block, zero SKIP; ng 376 / 5,925; full Playwright
  271 passed / 1 failed / 0 skipped (the red is the documented `workspace-search-documents` intermittent — same shape, 1-in-3 red in isolation on this build, no lane touched the surface; promoted to a named candidate). Versions: core 0.0.768, harness 0.0.662, web 0.0.105, host
  0.0.95, SPA 0.5.631. 💸 the dogfood queue gains the created-Uncensored
  greeting, the describer verdict against a real gateway, the sha256 heal on
  the Friday copy (measure the population FIRST), the interactive distill
  budget. **Next: the owed dogfood pass** — see phase-4.md. Round record:
  `status-log.md`.
- **The `0b0617fee`-round + follow-ups-round dogfood pass RAN (2026-09-03,
  agent-driven, on the Friday copy) — 15 rows, 13 PASS, 1 PARTIAL, 1 human;
  ZERO v5 defects and eight 💸 items discharged.** Walk doc:
  `dogfood-walks/2026-09-03-concierge-creation-sha256-pass.md`; record in
  `status-log.md`. The ledger's §2 probe **passed** at walk start, and the one
  drift row (bug 119) is an **unported** surface, so no step could blame it.
  **The pre-walk measurement is the pass's best result (ledger §5.5):** v4 had
  run its OWN bug-117 migration on this instance at 02:43 that morning
  (`4.9.0-dev.120` = `0b0617fee`), healing 117 rows — so the banked proof was
  dead on arrival and was replaced by a stronger pair. v5 **booted on v4's
  healed DB and wrote nothing** (ledger md5 identical, zero realign lines — the
  recorded presence-not-drift divergence meeting a real cross-app ledger), and
  on a **planted** population it reported `scanned=2791 realigned=5 orphaned=2
  malformed_key=0` with `orphaned`/`malformed_key` **matching v4's own run**
  and the five healed values **byte-identical to the ones v4's migration
  wrote**; a third boot proved it idempotent. **Proven live:** the
  Concierge-at-creation feature in **all four states** (Monitored omits the key
  entirely; Vouched → `'OFF'`; Flagged → NULL + `isDangerousChat=1`, exactly
  v4's `manual-flip.ts:11` mapping; Uncensored → `'UNCENSORED'`), with the
  uncensored greeting **airtight** — the seat was Z.AI, the only `llm_logs` row
  is DEEPSEEK, so attempt 0 went to the desk and the Concierge bubble sits
  second in the transcript — and Flagged giving a **second, different** routing
  proof (`settings_source="global"` vs `"chat-uncensored"`); bug 116's verdict
  with real arithmetic (1077 billed prompt tokens vs the 66 ceiling →
  `Arrived`) on a real JPEG through a `supportsAttachments:false` seat, plus a
  free contrast arm where every describer refused and v5 spliced the **honest
  error** rather than inventing a description; P4.68's failover `llm_logs`
  thread (three legs, three rows, the providers' real errors); the `?action=`
  semantics on both v4 shapes incl. the loud unserved-`scan` refusal and
  byte-identical action lists; the image route's Zod gate with **the 404
  beating the 400**; the danger ring; and the image-profile modal's structured
  writers on the real NANOGPT `FLUXNSFWunlock`. **Two apparent failures were
  INSTRUMENT ERROR** (measuring the ring on the wrapper, not the descendant the
  CSS targets; reading an Angular signal in the same tick as the synthetic
  `change`) — both now standing notes. **PARTIAL: A9** — the inter-character
  timing line is live with all five fields, but the interactive distill budget
  is a *deadline*, unobservable without a stall (and the constants are 45 s
  interactive / 90 s + retry background, not the 85 s a stale note claimed).
  **Scope note:** the bug-117 **chat-upload** leg cannot exhibit its fix in
  production — `chat_files.rs:705` threads `NotConfiguredPixelCodec` at every
  production call, so stored bytes ARE source bytes; P4.D152's named candidate
  (thread the HOST codec) is what closes it. **💸 still owed:** Pascal's
  group tier, the Brahma deep-query budget, dedup/summaries (cost), #101, and
  the LoRA wire-byte look (blocked). **The Docker/container walk (B6) was
  DISCHARGED the same day**, after `ARG CARGO_BUILD_JOBS=4` fixed an OOM on a
  stock Docker Desktop (`060ba01f`): pointing the container at the dogfood copy
  (already provisioned, auto-unlocking) removed the passphrase blocker, and one
  host listener captured BOTH halves — `HOST-HEADER: host.docker.internal`
  (P4.71's rewrite) and `GET //api/tags` (v4's double slash, the flipped pin) —
  followed by a real local-model completion through the container in 8 s. The
  repo's first CI workflow rides along, **manual-only**
  (`.github/workflows/docker-image.yml`, `workflow_dispatch`).
- **Oracle baseline: `0b0617fee` (2026-09-02, v4 main — bugs 116-118 fixed),
  adopted at the `0b0617fee` drift catch-up round unification (2026-09-03).**
  **Drift state, the drift-check method, and the pinned-worktree regen
  recipe live in `docs/developer/porting/drift-ledger.md`** — maintained
  by `/driftcheck` and by `/unify` at baseline moves; the other porting
  commands run the ledger's §2 freshness probe instead of re-deriving
  drift, and lanes never write it. **Never restate the drift count here —
  read the ledger's §1** (this bullet restated it once, went stale within
  hours, and the restatement is gone for good). Still no `release: 4.9.0`
  squash as of the 2026-08-29 check — re-probe BOTH branches every time. The sweep driver
  remains the
  sanctioned per-family regen path — never run two sweeps concurrently;
  since P4.53 it refuses empty-stage families by name, `--self-test`
  guards recipe headers against cross-alias defaults, and since this
  round it pins the `--nocapture` splice against the continued-command
  regression. The distill-transitive TZ pins, the committed-fixture rule,
  and the venue/staging rules stand unchanged. (The superseded baseline
  paragraphs formerly kept here "for history" are archived verbatim in
  `docs/developer/porting/claude-md-status-history.md`.)
- **Standing deferrals + gotchas:** tracked in the work orders, the
  status log, and the memory notes — not here.
