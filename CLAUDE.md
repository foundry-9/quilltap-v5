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
- **Rounds 2026-07-30 → 2026-09-15 — ARCHIVED.** The verbatim round bullets
  for that span (the P4.D29 ∥ P4.20 ∥ P4.21 ∥ P4.9P drift + standing-red +
  dogfood round through the `31436bae4` drift catch-up round) now live in
  `docs/developer/porting/claude-md-status-history.md` §3; the full round
  records were always in `status-log.md`. The arc, compressed: 78 bullets —
  some fifty unification rounds, their dogfood passes, and their same-day
  riders — carried the oracle baseline from `dcd9440a` to `31436bae4`
  through orders P4.D29–P4.D188 and P4.20–P4.88, absorbing v4's numbered bug
  fixes from bug 8 to bug 142 and clearing the drift debt at nearly every
  round. The verticals that landed whole in that span: the Almanack; Taboo;
  the character archive (+ the CLI `db characters` family); the `p4.9i2`
  help/HelpChat vertical — the last unported surface of any size, with v4's
  `help/` tree vendored and embedded in the binary; `p4.9k` character
  generators (wizard / optimizer / rename & replace / external prompt / AI
  import); character subprompts; progressions; per-tier dressing
  instructions and the wardrobe container + group tiers; the hair slot;
  embedding profiles; prompt templates; the Concierge's four states, list
  marks and choice-at-creation; the realtime invalidation subsystem over the
  existing Event channel; the Salon chat gallery; Avatar Rolls + the avatar
  configuration cache; the transcript as a subscribed read; the images
  collection route; file logging; tooltips; the composer's typography,
  typeaheads and formatting toolbar; NanoGPT as the tenth provider; the LoRA
  train; and the GPT-Image-2.5 capability table. Dogfood findings #37–#117
  were worked in that span, most fixed the day they were found; the port
  filed v4 bugs 57, 61, 71, 81, 82, 84, 87, 97 and 105 upstream and then
  watched v4 converge onto its own fixes round after round. Rulings made:
  the #39 impersonation-overlay mechanism (human, 2026-08-06 — the overlay
  design STANDS), the P4.D40 (a)-edge deferral reversed on evidence
  (2026-08-03), P4.33's import-overwrite ID identity, and the `jsonschema`
  dependency for AI-import validation. Deferred items from that era are
  tracked in the work orders, the drift ledger, `phase-4.md`, and later
  bullets, not here.
- **The `ffb6b3119` bug-141 + bug-142 drift catch-up round (P4.D189 →
  P4.D190 ∥ P4.D191): UNIFIED on main (2026-09-15) — ALL THREE CLOSED; the
  oracle baseline MOVES to `ffb6b3119` and the drift ledger's §3 is EMPTY.**
  v4's stream watchdog (bug 141) absorbed whole — and the port is ELEVEN wrap
  sites, not v4's two: v4 has ONE `streaming.service.ts` funnel for every
  Salon-side consumer and v5 has none. P4.D189: `model/stream_watchdog.rs`
  as a substrate commit (per-gap 240 s / 120 s budgets, the once-only
  stalled `Err` then `None`, drop-on-stall as v5's whole "abandoned, not
  cancelled", v4's eight test shapes under `start_paused`) + `StreamError`'s
  `Stalled` kind with v4's message bytes; the ten Salon-side sites held by a
  per-file wrap census; the `LLMStreamStalledError` → `network` classifier
  arm through all four classify sites (red-first: v4's stalled message
  matches NO network pattern, so v5 had filed a stall as the unattributed
  `provider-error`); five `primary_stream_tier3` stall arms + a 13-family
  neutrality sweep at both pins (**six of thirteen families differ across
  two regens at the SAME pin** — minted ids/clocks — so "cmp at both pins"
  is not universal; recorded). P4.D190 (stacked on the substrate): the
  greeting wrapped at 90 s / 60 s, the ladder's own-profile gate at v4's
  three positions with both desks scoped out, **v4's three attempt warns +
  the exhaustion line v5's silent arms never carried**, eight capstone arms
  over the ordered `stream_calls` comparand — **the first regen recorded
  v4's PRE-fix ladder** because a class imported at jest top level belongs
  to the registry generation before `resetModules()` and v4's `instanceof`
  silently never fired (fixed on both oracles; a memory note). P4.D191: the
  bug-142 CONVERGENCE measured TOTAL (1 of 1,513 cells) and
  `DELETE_MISS_DIVERGENCE` retired red-first in BOTH directions with zero v5
  source change; `85813ddd2`/`364b04ac4` ratified NO-PORT on file lists
  (Tier R 223/0; the main checkout's binding measured ABI-matched to Node
  24); the two `help/` files byte-copied, the whole 124-file tree
  md5-identical to v4. **The §3 review (three parallel readers): NO
  blocking findings** — fixed at unification: a source `Err` counted as a
  chunk (v4 never counts a throw), the capstone's posed-failure-vs-reasoning
  order, a missing presence pin on the converged chat, the census's
  greeting row (the named cross-lane handoff). Surfaced for the next round:
  the tool loops re-stream with a `base_params.model` never refreshed after
  a cross-provider failover (pre-existing), six more unported greeting log
  lines, and the pre-existing `brahma_orchestrator_tier3` red on main (a
  committed fixture predating P4.D171's columns). Gate + versions: the
  round record in `status-log.md`. 💸 the dogfood queue gains a REAL
  stalled endpoint on the Friday copy (the one proof no canned stream can
  give). Round record: `status-log.md`.
- **The six-round dogfood backlog pass RAN (2026-09-15, agent-driven, on the
  Friday copy) — 14 rows run, 13 PASS, ONE finding found and FIXED, and ten 💸
  items discharged including both headline ones.** Walk doc:
  `dogfood-walks/2026-09-15-six-round-backlog-pass.md`; record in
  `status-log.md`. The ledger's §2 probe passed at walk start (v4 HEAD **is**
  the baseline, §3 EMPTY), so nothing could blame drift. **The pre-walk
  measurement killed three banked positives and bought better proofs:** v4 had
  already run its own avatar-roll collapse (2026-09-11, 1785 → 898) and its own
  bug-132 heal (2026-09-09, 2,908), so v5 was proven instead by booting on v4's
  healed DB and writing **nothing** (187 ledger rows byte-identical); v4 had
  written both of the instance's only two `routeTrail` rows; and v4 had already
  turned on `impersonationVoiceRewrite`, run 17 rehearsals, and written a real
  **progression** (Abigail's pregnancy, 2026-09-08). **FIXED: finding #118** —
  the paused-room notice promised an answer that can never come (v5's own
  banner, written correctly by dogfood #83, made false by bug 137 / P4.D186;
  P4.D187 shipped the accurate toast but not the banner, and both tests
  asserted only the opening words while the unit spec's comment had frozen the
  now-false claim as the point of the test). Commit `6e11605a`, SPA 0.5.722.
  **⭐ The stream watchdog is proven end to end** against a posed stalling
  endpoint: the greeting at `elapsed_ms: 90002`/90,000 with the own-profile gate
  ending the ladder (zero retry lines) and the static greeting delivered; the
  Salon at `elapsed_ms: 240001`/240,000 followed one millisecond later by
  `[Failover] … trigger: "network"` — the classification the whole port turns on
  — and, with a live understudy, `[Failover] Understudy answered` plus v5's own
  persisted two-row trail. **So the route-trail badge is proven in BOTH
  directions** (v4's bytes rendered; v5's bytes written and rendered).
  **⭐ The avatar cache hit three times for nothing** (files/rolls/`IMAGE_GENERATION`
  all unmoved). **⭐ Progressions** rendered and computed to the day from v4's
  entry, with the withheld ordinary turn 14 minutes later as the other half of
  the proof, and a malformed sibling dropped and NAMED on screen. Also live: a
  real `VOICE_REWRITE` rehearsal, the transcript's subscribed read, all seven
  avatar-roll route guards, the gallery enumerator and its 404, and
  `--lock-status`. Free proofs: finding #110's narrating sweep, P4.78's
  `details` array, P4.72's `?action=` honesty, and the cheap-LLM chain with its
  `Task failed` warn firing BEFORE the chain. **⚠ NANOGPT's stored key is DEAD
  on this copy** (401; it is the instance default) — environmental, but it
  **A second session ran after the human refreshed the
  NanoGPT key** (which took effect with NO restart — keys are read live from
  `api_keys` per call): 13 more rows, all PASS — **the watchdog's IDLE arm**
  (`budget_ms: 120000`, `elapsed_ms: 120406`, byte-exact `went quiet for
  120000ms after 2 chunk(s)`) **and the mid-stream rule with it** (no trail, no
  failover, the partial preserved); a paused-room `nudge` running a real turn
  with `isPaused` still 1 (and writing the rotation A5 then verified); the
  **stale-lock reclaim** after a SIGKILL, the lock's own history reading
  `stale-detected … PID 17168 is no longer running` with the hostname playing
  no part; the gallery 409's **four FLAT siblings**; `?download=1`; the Almanack
  row; and the memory-gate lines on both arms. **Final: 31 rows run, all PASS.** The close-out
  tidy-up became a row of its own and found **#119**: `quilltap db --lock-clean`
  refuses with *"its holder is alive"* over a PID `assess_lock` has just
  computed dead, and that the BOOT path reclaims. ⚠ **The walk mis-filed it as a
  v5-invented string and changed the wording — wrong.** v4 emits both lines
  verbatim (`packages/quilltap/bin/quilltap.js:630-634`); the grep that
  "established" otherwise never looked in `packages/quilltap/bin/`, and **Tier R
  failed 4 of 223 cases** (this bullet first said five; P4.D194 measured four). Reverted, **pinned both ways**, filed upstream as
  **v4 bug 144**; Tier R back to 223/0. cli 0.0.21, zero behaviour change. The
  standing lesson: **run the oracle BEFORE changing a user-facing string** — a
  failed grep is not proof of absence where an oracle exists.
  ⚠ **That session also CORRECTED this walk's own claim and then FILED it
  upstream as v4 bug 143** — the 10 duplicate `generationKey` groups all PREDATE
  v4's 2026-09-11 collapse (newest 2026-09-07), so they are survivors, not
  re-accumulation; **seven are the migration's deliberate protected-portrait
  branch** and three are an unprotected, unreferenced older row with a live blob
  its victim loop should have deleted. Low severity, no read affected — v4's
  `lookupCachedAvatar` and v5's `lookup_cached_avatar` both sort newest-first
  then verify the character tag and the blob. v4 commit `064ba85df`; **docs-only,
  so the drift ledger carries it as NO-PORT? with the regen rule still NO PIN
  REQUIRED.** 💸 still owed (human): the Brahma deep
  query, dedup/summaries, #101, and the bug-133 reroute trio + the two
  image-spend rows.
- **The `2075242f9` bug-145/146 drift catch-up + maintenance round (P4.D192
  ∥ P4.D193 ∥ P4.D194 ∥ P4.89 ∥ P4.90): UNIFIED on main (2026-09-16) — ALL
  FIVE CLOSED (P4.D192's Tier 2 item 7 a NAMED follow-up, its premise
  refuted); the oracle baseline MOVES to `2075242f9` and the ledger's §3 is
  EMPTY — but ⚠ v4's tree is DIRTY with bug 147 in flight ("the chat GET
  omits the rotation"), so the regen rule stays PIN REQUIRED.** v4's two
  code commits absorbed whole plus the two maintenance candidates: bug 145
  (the avatar-roll collapse deleted by `fileId`, taking the album photo the
  operator kept — **v5 measurably HAD it**, 39 red rows across 17 of 24
  scenarios before the port; `drop_victim_roll_link` over the three existing
  chokepoints with `gc_orphaned_file_row` widened to v4's per-table counts,
  the in-pass `protectedKept` census on BOTH exits, `keptClause` on both
  ledger sentences; a fourth boot arm) ∥ bug 146 (`resolveFloorSeatId` on
  both sides under a NEW tier-1 family — 54 rows incl. v4's still-uncommitted
  eighth case — the Salon banner re-keyed on the FLOOR with v4's fourth
  sentence, Skip posting the floor seat, a live two-user-seat beat that
  impersonates the LLM seat so the walk is total; three order premises
  refuted: `find` scans the whole predicate, the two-user-seat floor is a
  coin flip with an LLM present, the reload route is unreachable on v5) ∥ the
  bug-144 CONVERGENCE (v4 fixed this port's own #119 filing — Tier R RED on
  FOUR fresh-heartbeat cases at the target pin, not the five the walk
  claimed; BOTH lines moved; `describe_fresh_window` off `FRESH_MS`; 223/0
  at the pin; the deliberate false-claim pin retired; two help pages
  re-vendored, 124 stays 124; four NO-PORT ratifications on file lists) ∥
  the committed `brahma-main.db` widened in place through v4's own migration
  DDL (the gap MEASURED with v4's real `compareSchemas`; twelve tables
  cell-identical) — the standing `brahma_orchestrator_tier3` red CLOSED with
  zero core change **and a SECOND latent red with it** (`brahma_console_
  routes`, un-passable on main since P4.D171, passing only by SKIPping); the
  P4.50-class prefix RETIRED (no live site) ∥ the post-failover
  `params.model` rebuild (the tool loops re-streamed the PRIMARY's model to
  the UNDERSTUDY's provider — a hard-error failover-then-tool-call corpus arm
  RED-FIRST; **`model` moves and nothing else**, since v4's `modelParams` bag
  is computed once before the primary stream) + the six greeting-ladder log
  lines, capture-pinned. **The §3 review (four parallel readers): NO
  blocking findings — the sixth such round; fixed on the unify branch:** the
  heal imported the NON-Node-faithful `is_photos_relative_path` twin where
  v5's runtime roll rule reads the faithful one (the lane had measured the
  divergence and reverted its consolidation, then left the heal on the wrong
  home); P4.90's `:692` warn re-recorded as a DIVERGENCE (v4's catch sits
  behind a fallback-mode `safeQuery` and never fires on a repository read);
  an oracle-side pin that the new arm's failover still happens; v4's eighth
  floor-seat case taken from its dirty test; seven prose-form skip notices
  made the `SKIP:` sentinel. Gate: 16/16 + 17/17 families fresh from the two
  pins zero SKIP; Tier R 223/0; clippy both feature sets; release build; 571
  test binaries / 3,321 / 0 / 2 ignored, zero SKIP lines; ng 434 files /
  7,343; full Playwright 318 passed / 3 failed / 6 skipped (the trio the P4.D138 follow-up recorded as the standing full-suite intermittent class — 18/18 green by file in isolation; the skips the standing parks). Versions: core 0.0.927, harness
  0.0.820, host 0.0.137, web 0.0.147, cli 0.0.22, SPA 0.5.726. 💸 the
  dogfood queue gains the live `--lock-clean`, a real cross-provider
  failover + tool call, the planted album-copy proof, the fourth banner
  sentence, the six greeting lines on a dangling key. **Next: bug 147's
  catch-up when v4 commits it, then the owed dogfood pass** — see
  phase-4.md. Round record: `status-log.md`.
- **The `1fefadb9a` bug-147 drift catch-up + maintenance round (P4.D195 ∥
  P4.91 ∥ P4.92 ∥ P4.93): UNIFIED on main (2026-09-16) — ALL FOUR CLOSED
  WHOLE; the oracle baseline MOVES to `1fefadb9a` and the ledger's §3 is
  EMPTY (v4 AT the baseline at the close).** Bug 147 absorbed whole: the chat
  GET projects the cycle's two turn columns as RAW JSON strings at v4's
  position (`salon_reads_equivalence` regenerated at the target FIRST — the
  P4.D171 both-directions omission pin tripped on all five `get_*` cases as
  the ledger predicted, then retired; raw-`''` and raw-NULL arms added), the
  SPA's dormant chat-GET seed LIVE and grown its spoken half — **v5 had
  bug 147's sidebar half exactly as v4 did** (`spokenSinceUserTurn` had never
  been seeded, so the `spoken` status was unreachable) — with the P4.D187
  refetch interplay and the busy-flip re-seed both pinned, the
  `state.cycleOrder` read measured as a convergence, v4's agreement room as
  tier-1 corpus rows (which opened and closed a blind spot: nothing had ever
  driven `selectNextSpeaker`'s rotation argument), the help page byte-copied
  ∥ P4.91: the photos predicate consolidated RIGHT (the family repointed
  BEFORE the corpus grew 13 → 30 with the slash-run shapes RED on exactly
  three; Node's loop MOVED — and found un-Node-faithful on one arm itself;
  the twin deleted, SIX importers repointed) + the collapse census's
  `fileIds` as the ARRAY v4 logs through a named `…Json` file-layer
  convention; the ORDER half escalated for a ruling ∥ P4.92: the tool loops'
  `base_params` carry-over closed (the corpus taught to SEE
  `previousResponseId`/`stop` as side-channels; THREE cases — the tool
  modes are NOT exclusive; the strip on the three loop CLONES because the
  empty-response recovery is a fourth consumer v4 passes `stop` to); the
  `cacheKey` carry MEASURED (six of eleven providers; ONE v5 input) and
  ESCALATED ∥ P4.93: the help-sync collision split by NAME with a
  fixture-sensitive channel, ONE `CompletionRole` inverse over 32 mapper
  hunks with every default preserved (the "exposed pair" measured NOT
  exposed), the memories rig armed through `global_capture`, the fifteen
  absent `[CharacterAvatar]` lines. **The §3 review (four parallel readers):
  NO blocking findings, but ONE false Tier-1 claim** — the avatar
  save-failure line had only a silence leg and dropped v4's exception —
  fixed with a trigger plant + firing pin; five should-fixes fixed (two
  `Debug`-rendered structured fields onto the JSON convention, the routing
  read's swallowed error, P4.92's two missing re-stream floors + a dead
  fixture key, P4.D195's post-post agreement row + the busy-flip pin + a
  draw-dependent e2e Skip arm made deterministic). **The unified sweep's own
  catch:** the collapse family read the census warn's `fileIds` by its raw
  name after P4.91's rename (the thread-scoped capture sees the raw field;
  only the file layer applies the convention) — reader repointed.
  Gate: 53/60 sweep families ok at the pin (the seven reds = six pre-existing fixture-vintage + the sweep's own catch, fixed); 572 test binaries / 3,343 / 6 (the six pre-existing reds) / 2 ignored, every round family confirmed RUN; clippy both feature sets; release build; ng 434 files / 7,349; full Playwright 320 passed / 1 failed (the documented quill intermittent, green by file in isolation (1/1, 1.1 m)) / 6 skipped (the standing parks). Versions: core 0.0.935, harness 0.0.827, web 0.0.148, SPA
  0.5.727; host/cli/tauri unchanged. **Next: the fixture-vintage heal (six
  pre-existing reds, one order), the `cacheKey` order, the owed dogfood
  pass** — see phase-4.md. Round record: `status-log.md`.
- **The `53294163f` GPT-Image-2.5 drift catch-up + maintenance round (P4.94 ∥
  P4.D196 ∥ P4.D197 ∥ P4.95): UNIFIED on main (2026-09-17) — ALL FOUR CLOSED
  (two follow-ups deferred loudly by name); the oracle baseline MOVES to
  `5f0a57dc4`; ⚠ v4 landed `bcd7e4852` (bug 151, the image transport budget)
  MID-ROUND — UNPROCESSED, the next catch-up's first row; PIN REQUIRED.** v4's
  PR #62 absorbed whole: the OpenAI image capability table as ONE module
  (eight families, exact-then-longest-prefix), the OPENAI image dialect
  rewritten over it RED-FIRST against the `image-dialects` corpus re-recorded
  at the target (97 → 150 rows; exactly the seven predicted pre-existing rows
  moved), the per-model options schema byte-exact (incl. the `2048x2048`
  experimental description the order had not counted) through the existing
  `options-schema` action, v4 bugs 148 + 149 in `generate_image.rs` (v5 had
  both — the three schema defaults that outranked the profile, and the
  `?? 'square'` that erased every explicit size), the tool definition's bytes,
  the ONE quality list at the tool schema and the `?action=generate` arm, the
  eight-model manifest, the openai-SDK wire re-check (every other corpus
  byte-identical where the port did not move it — and the four SDK-bundling
  plugin dirs measured still on 7.10.0, a human `npm install` item) ∥ the
  SPA's eight-id fallback list (v4's client and plugin table DISAGREE on the
  `gpt-image-1`/`-1-mini` order — carried as v4's own), a recorded copy of
  v4's REAL `getOpenAIImageOptionsSchema` rendered in a 30-test spec + the
  live beat, the two `help/` pages byte-copied, both stamp commits
  NO-PORT-ratified (bug 150's fix lands on the image-UPLOAD dialog v5 never
  ported; v5's in-chat dialogs post through the dispatch client, never a REST
  path) ∥ **five committed fixture pairs widened to v4's 4.10 vintage through
  v4's own migration statements — SEVEN pre-existing differential reds
  CLOSED with zero core change** (the seventh latent, never in any sweep;
  three columns no round predicted, incl. BOTH llm-logs partitions lagging
  P4.D49) ∥ **the Salon turn's per-character prompt-cache key carried on
  every leg v4 carries it** — SEVEN emitting providers not six (the OAC base
  class writes `user`), a NINTH v5 consumer found and fixed (Carina keys
  under the ANSWERER), the corpus taught to see the key (RED-FIRST 76 of 78
  calls), a force-final case that had never existed, the builders' emission
  pins written for the first time. **The §3 review (four parallel readers +
  the unifier's own reads): NO blocking finding — the seventh such round;
  five should-fix groups landed at unification:** the OPENAI parser's DOUBLED
  bag warnings (found twice over), the tool-unsupported retry's inherited
  cache key, `chats.transcriptVersion` on the widened pairs (the lane's "no
  v4 write ever names it" was FALSE — v4's migration adds it one step before
  `generationKey`'s), the migrator's index gating + recipe set, the render
  spec's missing label assertions. Gate: fmt + clippy both feature sets clean (re-checked after every fix); release build; the pinned 21-family sweep at `5f0a57dc4` 19 ok + 2 harness-side reds fixed and green by name, zero SKIP; 572 test binaries / 3,380 passed / 3 failed / 2 ignored with the 24-var block, zero `SKIP:` lines — the three reds the two harness fixes (green by name) and the known `/tmp/qt-imggen-*` staging collision (green through the driver on its own staging); ng 435 files / 7,382; full Playwright 322 passed / 0 failed / 6 skipped (the standing parks; P4.D197's beat LIVE and green). Versions: core
  0.0.942, harness 0.0.833, web 0.0.150, SPA 0.5.730; host/cli/tauri
  unchanged. 💸 the dogfood queue gains a GPT Image 2.5 profile end to end
  (the FIRST time v5 sends the four extras), bug 148's symptom gone on a
  profile storing `hd`, a real `prompt_cache_key` on an OPENAI primary.
  **Next: the `bcd7e4852` bug-151 catch-up, then the two follow-ups, then
  the owed dogfood pass** — see phase-4.md. Round record: `status-log.md`.
- **The `bcd7e4852` bug-151 drift catch-up + follow-ups round (P4.D198 ∥
  P4.D199 ∥ P4.96 ∥ P4.97): UNIFIED on main (2026-09-17) — ALL FOUR CLOSED
  WHOLE; the oracle baseline MOVES to `bcd7e4852`; ⚠ v4 landed TWO commits
  DURING the unified gate (`1065a1f53` bug 152 + `89fcc3c0d` bug 153 — the
  doc-edit opacity covenant; classified UNPROCESSED in the ledger's §3; PIN
  REQUIRED at `bcd7e4852` until the catch-up runs — the top next candidate
  ahead of the dogfood pass).** v4's bug 151 (the image
  transport budget) absorbed whole along the seam its own hunks draw: the
  per-image transport shrink as ONE core module over a NEW fallible
  `ImageTranscoder::shrink_to_webp` (a `Result`, default `Err` — so "threw"
  and "nothing to do" never collapse; `HostImageCodec` implements it over
  `image` + `webp`), run FIRST at both attachment loaders with the provider
  ceiling kept as the backstop, v4's nine arms in order incl. the
  grow-discard's second conjunct and the `undefinedxundefined` log rendering;
  a NEW tier-1 family with `sharp` SCRIPTED below v4's real function (the same
  script as a Rust `ScriptedTranscoder`, so encoded bytes are byte-comparable
  — D19 stands for the real encoders); `file_attachment_tier3` grown on both
  paths incl. the `autoResize: false` verbatim arm; the Lantern walk's
  per-turn 2 MiB budget spent NEWEST-first over the base64 string length in
  `message_context.rs` section K over the seam's `LanternLoad` (the post-hoc
  placement proven equivalent: the prefix is order-neutral under v4's double
  reverse, only attachment membership moves), five `orchestrator_tier3` arms
  with an `fsmBytesFill` spec key (junk bytes both sides — v4's real sharp
  passes them through, so the two halves were independent); the help page
  re-vendored (124 files md5-identical). P4.96 closed P4.D196's item 10 — the
  generate verb's five shaping fields (v5 had been SILENTLY DROPPING every
  one; an unknown `quality` answered 201) as raw `Option<Option<Value>>`
  tri-states with v4's measured Zod envelope, the census, a dispatch wire
  pin, 30 route-family rows; P4.97 closed the two wire-key follow-ups — the
  tool-unsupported retry sends v4's WHOLE option bag with the primary-stream
  family recording every call's option bag (the existing retry cases already
  reached the leg; the blindness was the driver's empty fields) + v4's three
  retry log lines the port never had, and the request-envelopes cache-key
  pin as a NAMED coverage table over a corpus re-recorded 347 → 367 with the
  anthropic/ollama rows it never had. **The §3 review (four parallel readers):
  NO blocking finding; six should-fixes landed at unification — headline:
  P4.96's `chatId` was still serde-typed, so an explicit `null` over dispatch
  answered 201 where v4 400s (now the same tri-state, census 442 → 441, three
  wire arms, two route rows 400/400)**; also the mount loader's stage order
  pinned + mutation-proven, the host whole-budget tests, the primary-stream
  neutrality loop, the new family's jest filter anchored (it had been running
  v4's own real-sharp test), and P4.D198's withheld `attach_mount_file` red
  RECLASSED from "v4-side" to fixture vintage (five missing columns) and
  CLOSED by widening the committed pair — which found and fixed a regression
  in the fixture migrator (its index step ran BEFORE its ALTER since the
  `53294163f` index-gating fix). Gate: 575 test binaries / 3,422 / 1 (the recorded `/tmp/qt-imggen-*` staging collision, green by name) / 2 ignored, every round family confirmed RUN; the pinned sweeps 9/9 green; clippy both feature sets; release build; ng 435 files / 7,382; full Playwright 322 passed / 0 failed / 6 skipped (the standing parks). Versions: core
  0.0.950, harness 0.0.842, host 0.0.139, web 0.0.152, SPA 0.5.731. **💸 the
  dogfood queue gains the shrink's debug line + the per-turn WARN on a real
  multi-portrait turn, the generate verb's envelope, the named retry; the
  owed dogfood pass is the top next candidate** — see phase-4.md. Round
  record: `status-log.md`.
- **The `89fcc3c0d` opacity-covenant drift catch-up + follow-ups round
  (P4.D200 ∥ P4.98 ∥ P4.99): UNIFIED on main (2026-09-18) — ALL THREE
  CLOSED WHOLE; the oracle baseline MOVES to `89fcc3c0d`; ⚠ v4 landed
  `baa85e19b` (bug 154, the default-system-prompt lockstep) on the round's
  first morning — every lane STOPped as ordered and resumed under the human's
  recorded waiver, every regen pinned; bug 154 keeps its ledger row as the
  next catch-up's first, PIN REQUIRED at `89fcc3c0d`.** Bugs 152 + 153
  absorbed whole (P4.D200): the covenant as a SUBTRACTION —
  `hide_character_vaults` on `PathResolutionContext` set by both builders
  which now KEEP `character_id` (v5 measurably dropped it — the group tier
  keyed on a `None`), the tiered pool's `FlattenOptions { include_character_
  tier }`, the self-token gate's new conjunct (a CONDITION, not a new arm —
  the commit message misdescribes it), `find_enabled_mount_point_by_ref` +
  the NOT_FOUND → ACCESS_DENIED split with v4's three-sentence message and
  the two `vaultsHidden` warns (the path resolver had ZERO tracing before —
  five pre-existing absent v4 lines restored with capture pins), the
  `AccessibleMountPointsQuery` derived at all four enumeration call sites
  (the blob WRITE resolver without peers — the hunks, not the prose), the
  help page re-vendored, a NEW real-DB `doc_opacity_equivalence` over a
  purpose-built fixture mirroring v4's 13 + 15 regression cases RED-FIRST
  21/44 ∥ P4.98: `Request::ImagesGenerate`'s five body keys as the
  `Option<Option<Value>>` tri-state with ONE shared decoder for REST,
  dispatch and Tauri (the collection route had generated and SAVED an image
  on a `chatId: null` v4 refuses), a new dispatch wire test red-first on
  three arms, the census's sibling raw list (441 → 441), the Zod
  type-renderer's fourth hand-copy retired ∥ P4.99: v4's `Recoverable
  request error detected, attempting recovery` INFO line (the hunk's level,
  not the candidate list's "warn") with a capture pin + silence + ORDER legs,
  GOOGLE's cache-key ignorer pair in BOTH google corpora against v4's real
  plugin + a unit pin that v5's builder is key-blind (the order's M5
  prediction FALSE — the wire reframer drops a stray `config` key),
  `CorpusScript` in one home, the tier-1 log asserts anchored. **The §3
  review: NO blocking findings (the seventh such round); fixed on the unify
  branch:** the `ImageProfileGenerate` distinctness twin (P4.98's own
  finding — a decode-raw test stays green against a plain `Option<Value>`),
  the google request-logic family's ignorer overclaim (its recorder's call
  cannot receive the key — the v4 proof is the wire family's), a JS-truthiness
  arm in `describe_characters`, two missing silence legs (the self-token one
  over the REAL fixture — the pool's character-tier read applies the vault
  overlay), a level assert, `help_tree_equivalence` run at the target pin
  for the first time, an anchored field pin, four nits. Gate: 19/19 families
  fresh from the two pins zero SKIP; 577 test binaries / 3,449 / 0; clippy
  both feature sets; release build; ng 435 / 7,382; full Playwright
  **322 / 0 / 6** (the standing parks). Versions: core 0.0.960, harness
  0.0.850, web 0.0.154. 💸 the dogfood queue gains the covenant on the
  Friday copy, the `chatId: null` refusal over dispatch, the recovery line on
  a real token-limit turn. **Next: the bug-154 catch-up, then the owed
  dogfood pass** — `phase-4.md`. Round record: `status-log.md`.
- **The `baa85e19b` bug-154 default-system-prompt drift catch-up +
  maintenance round (P4.D201 ∥ P4.D202 ∥ P4.100 ∥ P4.101 ∥ P4.102): UNIFIED
  on main (2026-09-18) — ALL FIVE CLOSED WHOLE; the oracle baseline MOVES to
  `baa85e19b` and the drift ledger's §3 is EMPTY (v4 AT the baseline at both
  probes).** Bug 154 absorbed end to end: the `default_system_prompt` resolver
  home (22-case tier-1 family over v4's REAL module) folded into the chat
  initializer — the `??`-on-content change the commit message never mentions,
  whose red-first case CANNOT EXIST (v4 guards an empty-content prompt three
  times over, measured) — and the voice preview, which had reproduced v4's
  PRE-fix stale-column chain comment and all; `systemPromptsPatch`'s twin
  under all four system-prompt writers (v5 REPRODUCED the lockstep gap whole:
  none wrote the column) with the transient-id null rule; the clear arm; the
  `characterUpdate` chokepoint with v4's empty-payload rule and partial
  write; the arrays family's per-op `defaultColumnTrail` (its six-table
  census is a FINAL-STATE diff — three new write ops left the dump
  byte-identical); seven mutations arms with readbacks; the widened
  `characters-*` pair (RED on unported main at the baseline, a panic BEFORE
  the lane's block); the SPA twin over v4's five vectors verbatim, the two
  broken New-Chat seeds red-first (a stale column opened a chat with NO
  system prompt), the star's optimistic write with rollback-then-refetch (M4
  SURVIVED until the spec held the relist), the 4xl dialog, the star beat
  FLIPPED LIVE ∥ the bounded `drain_expecting` across 62 sites (reproduced
  two ways first) + the four opacity rows ∥ ONE Zod-issue home (the key-order
  worry did NOT fire — all eight copies already agreed against the real zod
  4.5.4; 17 constructors in 9 files → 7 in 3, nineteen neutrality families)
  ∥ `request_envelope` with both `tri()` helpers retired and a census guard
  over 26 tri-state variants. **The §3 review: NO blocking finding; seven
  should-fixes landed at unification — headline: the uuid HALF of v4's
  `z.uuid()` gate on the character PUT's `defaultSystemPromptId`** (a
  non-uuid STRING reached the chokepoint's miss AFTER the generic patch had
  landed, so the `name` beside it persisted where v4 writes nothing —
  red-first on a new mutations arm); also the reachable miss warn's missing
  capture pin, a TENTH reader of the widened pair with its own oracle the
  lane's re-run list missed, the envelope helper's path-wins ordering, the
  census stripper over-stripping PRODUCTION code, the drain's `Lagged` arm,
  the first-prompt default seed. Wires: the flip + the
  `characterPromptSetDefault` dispatch wire test (its first run corrected its
  own precondition — the widen populated NO column). Gate: fmt/clippy both
  feature sets; release build; the 35-family sweep from the pin 31 ok + the
  four pre-existing fixture-vintage reds (all proven pre-existing on pairs
  no lane owned — now the heal order's TEN); ng 437 files / 7,408 / 0; full
  Playwright **321 passed / 2 failed / 6 skipped (9.0 m)** — the six skips the standing parks; the two reds the documented P4.d17 quill intermittent and the P4.D188 avatar-rolls beat, each re-run by FILE afterwards, one invocation at a time: the quill green alone (1/1); the avatar-rolls beat RED alone too and root-caused to the spec's own unconditional `ALTER TABLE files ADD COLUMN generationKey` on its copy of the pair P4.D201 widened — the widen's Playwright readers, which the lane's harness re-run could never reach — guarded on `pragma_table_info` and green alone (1/1, `fix(e2e)`); the star beat green LIVE alone (1/1, 55 s); the search-documents spec 3/3 alone; `cargo test --workspace` **581 test binaries / 3,469 passed / 4 failed / 2 ignored** with the round's env block (the four reds EXACTLY the pre-existing fixture-vintage families — `subprompts_prompt_tier2`, `chat_delete`, `character_wizard_tier3`, `subprompts_routes` — their oracle vars deliberately IN the block so the true state shows rather than a false `SKIP:`; every round family confirmed RUN by name, incl. the star wire test and the tenth reader against its fresh oracle; the 424 `SKIP:` lines are families outside the block, none of them this round's).
  Versions: core 0.0.966, harness 0.0.857, web 0.0.157, SPA 0.5.741;
  host/cli/tauri unchanged. **Next: the fixture-vintage heal (ten families),
  the two uuid-gate copies outside the home, then the owed dogfood pass** —
  see phase-4.md. Round record: `status-log.md`.
- **The six-round backlog dogfood pass RAN (2026-09-18, agent-driven, on the
  Friday copy) — 39 rows, 37 PASS, ZERO v5 defects, and the six rounds' 💸
  queue discharged bar the spend-bound items.** Walk doc:
  `dogfood-walks/2026-09-18-six-round-image-opacity-pass.md`; record in
  `status-log.md`. The ledger's §2 probe passed at walk start (v4 AT the
  baseline, §3 EMPTY), so no step could blame drift. **The pre-walk
  measurement bought the pass its best proofs:** v4 landed bug 154 on the live
  instance at 07:04 that morning and the §5.5 population survived — **both**
  arms of the resolution order sat on real data in genuinely disagreeing
  states (Friday's column names a prompt she no longer has; Sunny's names one
  she does while her flag sits elsewhere), so a new Sunny chat opened on the
  **column's** prompt and a new Friday chat fell through to the **flagged**
  one, where pre-fix it opened with **no system prompt at all**. ⭐ **The
  opacity covenant ran on bug 152's own row** (Leilani / `Severed` /
  `00bb0f9c…`): she reaches her group store by name AND by id, her listing
  shows three tiers and **not one of the instance's 52 character vaults**, a
  stranger store answers the three-sentence ACCESS_DENIED with `characters:
  e14cb17a-… vaultsHidden: true` — the log line being the fix's own signature
  — peer/`self`/her-own-vault all answer one indistinguishable refusal, and
  the **same chat's** transparent seat sees her 200-file vault. ⭐ **The four
  GPT Image 2.5 body keys reached the wire for the first time at ZERO spend**
  (the body is assembled and logged before the HTTP call, so a deliberately
  invalid key made the proof free); `gpt-image-1` collapses sizes 14 → 5 and
  quality 7 → 5 losing exactly `xhigh`/`max`; bugs 148 and 149 both proven;
  all five `imagesGenerate` nulls refuse on both transports with `files`
  unmoved; `imageProfileGenerate` answers v4's Zod `details` envelope with 404
  beating 400. ⭐ **`spoken` rendered in the participant rail for the first
  time in v5**; ⭐ **bug 146's fourth banner sentence** named the FLOOR seat
  while the composer sat elsewhere, with Skip posting the floor id on both the
  client dispatch and the persisted `turn-pass`; ⭐ **bug 144's corrected
  `--lock-clean` pair** read back on a genuinely dead PID — v4 having adopted
  this port's own filing #119 — then cleaned once stale; a restart left
  `migrations_state` **md5-identical** at 187 rows. ⭐ **The bug-151 shrink
  beat its own estimate: 1,946,202 → 70,872 bytes (27.5×), 5712×4284 →
  1024×768.** **RECORDED: #120** — the five largest help documents have **no
  embeddings at all**, document or section, because both implementations cap
  `EMBEDDING_MAX_CHARS` at 128 KiB and the chunk pass sits inside the failing
  `try`; text search still reaches them; **v4-faithful, a candidate upstream
  filing, no v5 change**. **Three instrument errors caught before they became
  findings:** the pane's `F5` does not reload; a `<select>` set to an absent
  option value goes to `selectedIndex: -1` and looks like a dead panel; and a
  file attached by id but linked to a DIFFERENT chat is correctly refused,
  after which **the model hallucinates a confident, detailed, wrong
  description** — downloading and *looking at* the image settled it, so the
  standing lesson gains a corollary: **when a model describes an image, check
  the image**. Zero panics across five boots. **Still owed:** the per-turn
  Lantern budget (needs generated portraits = image spend), a real token-limit
  turn, a function-calling refusal, the four planted proofs (bug 145's
  collapse, the greeting ladder on a dangling key, a cross-provider failover
  with a tool call, a chained OPENAI re-stream), and the standing queue
  (dedup/summaries, the Brahma deep query, #101, the re-measured compression
  row).
- **The `f45a517a9` thirteen-commit drift catch-up round (P4.D203 →
  {P4.D204 ∥ P4.D205 ∥ P4.D207 ∥ P4.D208 ∥ P4.D209 → P4.D210} ∥ P4.D206 ∥
  P4.D211): UNIFIED on main (2026-09-22) — ALL NINE LANDED; the oracle
  baseline MOVES to `f45a517a9`; the ledger's §3 keeps THREE rows (bugs
  161/162 + their docs commit, v4 HEAD `a2db63da7`, PIN REQUIRED).** The
  largest round this port has run: the compressed-text codec + the `qt_text`
  UDF on every connection (v5 can READ and WRITE a v4-4.10 instance again —
  brotli parity byte-identical to 262 KB), FTS5 message search (the five
  objects verbatim + the boot reconciler; the `.`-defect retired), Inform on
  both sides, the streamed swipe on both sides (the `status` frame carries
  `kind` first — §S.2 was wrong), the Scriptorium contract (bugs 155/156/157;
  bug 159's image half as a seam), `quilltap sync` (Tier R 223 → 244), bug
  158, Zod 4.6.5 (neutral; `is_zod_email` converged at tier 2). **The §3
  review (six readers) found THREE BLOCKING defects and fixed them:** the
  three Inform REST arms answered 500 on SUCCESS (the unwrapper matched no new
  variant; the route family never touched axum); the backup manifest omitted
  `chatInforms`; the SPA's bug-(b) selection ran over the PRE-refetch swipe map
  (inert by zoneless timing — v4's `fetchChat` writes state before it
  resolves). **The unified sweep found two more:** `informRowIds` skipped when
  empty (v4 always emits it), and the joined blob row reading the nullable
  `originalFileName` as a bare `String` (every text-blob import failed —
  P4.D209 had widened only the write side). Twenty-two should-fixes landed
  with tests; three dead recipes repaired; a merge-tooling trap recorded (a
  naive union resolver truncated an arm on a diff3-split adjacency conflict).
  **OPEN by name:** P4.D205's seven items; the image seam wired into the sync
  applier ONLY; `update_message` DELETE+INSERT vs UPDATE under the triggers;
  P4.D210's live Tier R rows + the writer-hold ruling; the fixture-vintage
  heal (eleven families, six pairs). Gate: 68 families fresh from the pin
  through the driver, all ok; SPA 440 files / 7,466; Playwright 329 passed / 1 failed (the pre-existing P4.66 bubble beat, 2/2 alone) / 6 skipped;
  `cargo test --workspace` 611 binaries / 3,612 passed / 1 failed (the `cli_differential` env-block artifact, 244/0 by name) / 3 ignored. Versions: core 0.0.992, harness 0.0.890,
  host 0.0.146, web 0.0.170, cli 0.0.25, SPA 0.5.747. **Next: the bug-161/162
  catch-up, the fixture-vintage heal, the image seam, then the owed dogfood
  pass on a Friday copy v5 can at last read and write** — `phase-4.md`.
  Round record: `status-log.md`.
- **The `a2db63da7` bug-161/162 drift catch-up + maintenance round (P4.D212 ∥
  P4.D213 ∥ P4.D214 ∥ P4.103 ∥ P4.104 ∥ P4.105 ∥ P4.106): UNIFIED on main
  (2026-09-23) — ALL SEVEN LANDED; the oracle baseline MOVES to `a2db63da7`
  ; ⚠ v4 COMMITTED bugs 163/164 (`00c290c9a`, the auto-title chokepoint)
  minutes after the fast-forward — the ledger's §3 holds that ONE
  UNPROCESSED row and the regen rule stays PIN REQUIRED.** Bug 161 absorbed whole (ONE `speaker_names` resolver
  shared by the fold and the episode pass — v5 had reproduced the defect
  verbatim; the fold turn's required `speaker`; `FOLD_SUMMARY_PROMPT`
  regenerated mechanically, 1,585 bytes identical; the debug line; the
  rebuild-summary verb with v4's 409/400 order, ONE update leaving
  `lastFullRebuildTurn` alone, priority 0, the `chats` publish; the Salon's
  Organize entry with v4's strings, its beat run LIVE) ∥ bug 162 (the ONE CLI
  opener with v4's per-target strings — a v5 defect on `sync`'s strings found
  and closed with it; Tier R 244 → 266, 266/0 at the target, nine designed
  reds at the baseline; P4.D210's live rows as canned-stub rows) ∥ the
  fixture-vintage heal (six pairs widened through v4's own ALTERs, SEVEN
  standing reds closed, every reader re-run) ∥ bug 159's image half made a
  port (the encoder threaded to all TEN blob-write sites over a shared D19
  comparand with real sharp on every decodable row, a census, and THREE real
  defects found behind the seam and fixed: a pre-`186eb09cb` transcode copy,
  the general upload's un-configured codec, the vault/Lantern/generate
  writers recording the INPUT's size) ∥ `update_message` as v4's UPDATE
  under the FTS5 triggers (red-first on 7 of 8 ops; the search `safeQuery`
  arm) ∥ P4.D205's seven items closed as planted-row arms + the bug-158
  heal differential + the provider-SDK guard + the two-link-blob bundle.
  **The §3 review (four parallel readers): NO blocking finding — the eighth
  such round; nine should-fixes landed at unification**, headline: the
  orchestrator family's CONVERGENCE pin was still armed on the union (a
  cross-lane handoff nobody took — it would have panicked at the new
  baseline); the opener's path-suffix strip guarded on `CannotOpen`; the SDK
  guard SKIPs without plugin installs. **The wire's first live run of the
  P4.D213 beat FAILED on its own assertion** (it read the fold cursor through
  `chatGet`, whose projection is v4's hand-built object carrying
  `contextSummary` alone — v5 matches) and was repaired. **RULED (the
  human, 2026-09-23):** the host WebP codec encodes ONE frame, so wiring the
  seam stores an animated GIF as a still where v4 keeps every frame — the
  codec will DECLINE animated inputs (v4's store-original fallback keeps the
  frames); a small order, not yet landed. Gate: fmt/clippy both feature sets/release clean; the 91-family sweep from the pin 86 ok after the sweep's three catches (a courier oracle case serializing a Node Buffer for a compressed column; a recipe assuming an unstaged mirror; a stale version constant), the five reds fixture-vintage pairs measured for the heal; Tier R 266/0 at the target, nine designed reds at the baseline; `cargo test --workspace` (148-var block) **620 binaries / 3,640 / 6 (the five vintage families + the recorded imggen staging collision, green by name) / 3 ignored**; SPA 441 / 7,474 / build + lint clean; full Playwright **326 / 5 / 6** — four reds green by file alone, the fifth REAL: the avatar-rolls beat's PNG plate can no longer read as kept once the album save normalizes for real (v4 identical); its seed made a WebP as production rolls are (green alone after the re-seed: 1/1 (53 s)).
  Versions: core 0.0.1001, harness 0.0.913, host 0.0.147, web 0.0.176, cli
  0.0.27, SPA 0.5.748. **Next: the bug-163/164 catch-up when v4 commits it,
  the heal's next list, the animated ruling, then the owed dogfood pass** —
  `phase-4.md`. Round record: `status-log.md`.
- **The `00c290c9a` bug-163/164 drift catch-up + maintenance round (P4.D215
  ∥ P4.107 ∥ P4.108 ∥ P4.109 ∥ P4.110): UNIFIED on main (2026-09-23) — ALL
  FIVE LANDED; the oracle baseline MOVES to `00c290c9a`; v4 HEAD `d1c06cd9d`
  (the Scenario Builder + its spec) sits UNPROCESSED in the ledger's §3, so
  the regen rule is PIN REQUIRED at `00c290c9a`.** Bugs 163/164 absorbed
  whole: ONE `services/auto_title.rs` chokepoint (v4's four outcomes, the
  post-LLM re-read, the refused arms' `extraPatch`-only write, the Lantern
  enqueue moved in) under the fold (v5 had reproduced BOTH bugs verbatim),
  the title-update job and regenerate-title; the fixture heal's five mains
  (four standing reds closed; the next ten measured); the ruled animated
  decline at the host codec (frame count ≥ 2); `get_messages` as v4's
  fallback `safeQuery` with a census-chosen strict sibling, and
  `update_message` answering v4's `null`; the shared census lexer + walkers,
  one `materialize_salon_instance`, `execute_import`'s required codec. **The
  §3 review found ONE BLOCKING defect — P4.108's frame counter ran the
  animation decoders with NO allocation limit** (a tiny crafted GIF/WebP
  declaring a huge canvas → ~17 GB per frame on every upload/import/sync);
  fixed to charge `decode`'s own 512 MiB limit the way `decode` charges it
  (the reviewer's RGBA suggestion was measured WRONG — it would flatten a
  large legal no-alpha animation), pinned on both sides, red-first. Also
  fixed: the importer's two reads to STRICT (P4.109's recorded handoff),
  unknown-type rows WARN as v4, regenerate-title's reads under v4's one outer
  catch + the `missing` arm; the `docs/v4/developer/bugs/` mirror made
  current. Gate: sweep 32/32 from the new pin; 621 binaries / 3,663 / 1 (the
  unification's own census trip, fixed) / 3 ignored; Tier R 266/0; SPA 441 /
  7,474; full Playwright **331 passed / 0 failed / 6 skipped (10.1 m)** (the six skips the standing parks). Versions: core 0.0.1013,
  harness 0.0.930, host 0.0.149, web 0.0.179. **Next: the Scenario Builder
  catch-up, the ten-file heal, then the owed dogfood pass** — `phase-4.md`.
  Round record: `status-log.md`.
- **The `d1c06cd9d` Scenario Builder drift catch-up + maintenance round
  (P4.D216 → {P4.D217} ∥ P4.D218 ∥ P4.D219 ∥ P4.111 ∥ P4.112): UNIFIED on
  main (2026-09-23) — ALL SIX LANDED; the oracle baseline MOVES to
  `d1c06cd9d` and the ledger's §3 is EMPTY (v4 AT the baseline, clean).** The
  Scenario Builder ported end to end: the substrate (ONE shared
  `run_one_shot_tool_loop` under the Brahma one-shot with v4's five new debug
  lines, `DocToolsMode` + the extras bag + the third `search` variant, the
  stream call's `log_type` + `SCENARIO_BUILDER`, the pre-built mount pool
  through the executor / search / doc-edit / path resolver without weakening
  P4.D200's covenant, `groupList { characterIds }`), the service + three verbs
  + `scenarioBuilderProgress` frames + the host driver + the SSE edge
  (disconnect = abort) + `help/` 126 → 127, the SPA dialog + save dialog on New
  Chat and the Salon sidebar — **the four beats LIVE at unification, 4/4 on
  their first run** — and bug 165 (v5 had it verbatim). P4.111 closed TEN
  vintage reds; P4.112 the previous review's smalls. **The §3 review (four
  parallel readers): NO blocking defect — the ninth such round; seven
  should-fixes fixed on the unify branch, headline: v5 logged a WARN v4 can
  never emit** (the route's unreadable-cast-id line — v4's `findById` is a
  fallback `safeQuery` and never throws), found by pinning the route log
  lines for the first time; also v4's mid-stream error frame + ERROR line
  (a silent SSE end before), a text-block tool-mode corpus (one-mode before),
  the loop's `tools` field as v4's array (the test had been built on the
  divergence), the SPA's profile cache key shared with two flag-less shapes,
  an error reply overturning a delivered scene, the capture rig's loud
  install. **RULED after unification (the human, 2026-09-23): the
  corrupt-second-frame input keeps v5's behaviour** (P4.112 item 7 — the
  corrupt frame is thrown away and the first frame kept as a still; v4 stores
  the original — a deliberate divergence, pinned both ways). Gate:
  67/67 sweep families from the pin; 629 binaries / 3,696 / 0 / 3 ignored;
  Tier R 266/0; SPA 447 / 7,654; full Playwright **328 / 7 / 6 (12.1 m)** — the seven reds one Salon-streaming timing cluster in five untouched files, each green alone (3/3, 2/2, 2/2, 1/1, 2/2). Versions: core
  0.0.1027, harness 0.0.947, host 0.0.152, web 0.0.186, SPA 0.5.755. **Next:
  the review follow-ups as a smalls lane, then the owed dogfood
  pass (the Host end to end, real spend)** — `phase-4.md`. Round record:
  `status-log.md`.
- **The `d1c06cd9d` review-follow-ups smalls round (P4.113 ∥ P4.114 ∥
  P4.115 ∥ P4.116 ∥ P4.117): UNIFIED on main (2026-09-24) — ALL FIVE LANDED;
  the baseline STAYS `d1c06cd9d`; ⚠ v4 landed FOUR commits mid-round (PRs
  #64–#67 — the Salon image quick-hide, the `@` mention typeahead, the chats
  `?action=has-dangerous` removal, a release script) and its checkout is
  dirty with an in-flight help-doc split (apparently dogfood #120) — every
  regen was pinned, the ledger's §3 holds the four as UNPROCESSED, PIN
  REQUIRED.** The db-layer smalls (`update_message` reads one raw row and
  validates the MERGED event; the corrupted-row skip gains `createdAt` under
  zod 4.6.5's seconds-REQUIRED rule and `participantId`; v4's
  `[InstanceSettings]`, `Character not found` and five JSON-setting WARNs), the
  tool-loop smalls (`doc_read_file`'s key order, the doc-edit operator surface,
  the search INFO/WARN, per-chunk normalization, the empty signature), the
  Scenario Builder edge (the post-abort gate at the host publisher, the
  poisoned registry recovering, the stream committed at v4's `accepted`
  point via an in-process watch — no wire change), the SPA smalls (both
  builder dialogs portaled; one raw `['connection-profiles']` entry), and
  thirteen dead heals removed. **The §3 review (five readers): no blocking
  defect in lane code; ONE blocking gate red** — P4.115's recorded handoff
  (the emptied `COLLAPSE_CENSUS` row) nobody took, red on the union, fixed
  red-first. Also fixed: v4's `Search scriptorium tool execution failed`
  ERROR; P4.113's NULL-content update divergence pinned both ways; P4.115's
  M3 re-measured SURVIVING; the portal destroy hooks mutation-proven; an
  understudy-remap coverage overclaim; `ai_import_tier3`'s `V4_APP_VERSION`.
  **⚠ ESCALATED:** the portaled builder closes on its first click in a
  narrow (< 640 px) Salon pane — v4 identical; a ruling. Gate: 101/101 sweep
  families from the pin; 630 binaries / 3,710 / 0 / 3 ignored, Tier R 266/0;
  SPA 448 / 7,668; Playwright 328 / 7 / 6 (the known Salon-streaming
  cluster; `salon-regenerate-stream-flow` flaky alone on this tree AND on a
  main build — a `2/3` counter watch item). Versions: core 0.0.1044, harness
  0.0.964, host 0.0.154, web 0.0.191, SPA 0.5.760. **Next: the narrow-pane
  ruling, the four-commit drift catch-up, the owed Host dogfood pass** —
  `phase-4.md`. Round record: `status-log.md`.
- **Oracle baseline: `d1c06cd9d` (2026-09-23, v4 main — "feat(scenarios):
  Scenario Builder — the Host researches and drafts a starting scene",
  `4.10.0-dev.67`), adopted at the `d1c06cd9d` Scenario Builder drift
  catch-up + maintenance round unification (2026-09-23).**
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
