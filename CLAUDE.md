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
- **Main's `target/` is garbage-collected with cargo-sweep, not wiped.**
  `/setupphase` ends by running `scripts/cargo-sweep.sh stamp` → the lanes'
  gate builds → `scripts/cargo-sweep.sh file`, leaving main with exactly the
  artifacts the lanes will clone (incl. the pinned amalgamation). `/cleanup` runs the same cycle around `cargo build --release` (the build `/dogfood` launches), keeping the release bins plus that lane set.
  **Never bare `cargo sweep --stamp`:** APFS freezes a file's atime after its
  first read, so the plain cycle deletes exactly the REUSED artifacts — the
  script's header has the measurement. Agents: never `cargo clean`, `rm -rf`
  main's `target/` (or its `deps`), run either step outside `/setupphase` / `/cleanup`,
  or touch `sweep.timestamp` unless asked. Lane worktree targets are still deleted
  whole at lane close, and START as an APFS clone of main's (`cp -cR`;
  `/carryout` rule 8) so dependencies never build cold in a lane.
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
- **Rounds 2026-09-15 → 2026-10-03 — ARCHIVED.** The verbatim round bullets
  for that span (the `ffb6b3119` bug-141/142 drift catch-up round through the
  `f6426e196` bug-174 review-follow-ups round and its dogfood pass) now live
  in `docs/developer/porting/claude-md-status-history.md` §4; the full round
  records were always in `status-log.md`. The arc, compressed: 27 bullets
  (rounds P4.D189–P4.D248, P4.89–P4.145 and eight dogfood passes) carried the
  oracle baseline from `ffb6b3119` to `f6426e196`/`e5c6bd0c0`, absorbing v4's
  bugs 141–176 (the stream watchdog; avatar-roll collapse; GPT-Image-2.5 and
  the image transport budget; the doc-opacity covenant; the default-system-
  prompt lockstep; the compressed-text codec + FTS5 message search; Inform;
  the auto-title chokepoint; the Scenario Builder; help docs by section; the
  Concierge overhaul and the text-side refusal seam; the Post Office mail
  tools; the `chats.renderedMarkdown` DROP; the anti-committee prompt squash;
  bug-174 vault-image bytes; the project roster gate; the structural boot
  pass), with D23 re-dumps at the Concierge states, the `renderedMarkdown`
  drop and standing informs. Dogfood findings #118–#134(b) were worked
  (#118, #123, #127, #133, #134 fixed; #121 host-zone dates ordered as P4.119;
  #124/#129 ordered); the port filed v4 bugs 143 and 144 (the latter later
  fixed upstream) and had bugs 175/176 fixed upstream in a different shape.
  Rulings: the animated-image decline at the host codec, the corrupt-second-
  frame divergence kept, the P4.D184 FATAL ruling (later overtaken by
  `e5c6bd0c0`), the project-roster and preserve-at-create decisions. Every
  §3 review in the span reported no blocking production defect except a
  handful caught and fixed at unification (an XSS in the inline hidden-image
  swap, an unbounded animation-frame decoder, a restore-store regression).
  Deferred items from that era are tracked in the work orders, the drift
  ledger, and later bullets, not here.
- **The `f6426e196` recorded-divergences round (P4.139 ∥ P4.140 ∥ P4.141 ∥
  P4.142 ∥ P4.143 ∥ P4.144 ∥ P4.145): UNIFIED on main (2026-10-03) — ALL
  SEVEN LANDED; the oracle baseline STAYS `f6426e196`; the ledger's §3 keeps
  its two UNPROCESSED rows (`9753d0eb2` the project roster gate,
  `e5c6bd0c0` v4's own fixes for bugs 175/176), PIN REQUIRED.** The API-key
  read class through v4's fallbacks with a census (P4.139); Option V — the
  host's display-zone VALUE threaded through the Salon spine, v4's
  `fileProcessing` frame, Carina's `CHAT_MESSAGE` row (P4.140); the
  transport KIND + v4's `network` trigger for timeouts, the 2xx shape guard
  on seven providers, ten plugin ERROR lines (P4.141); the overlay batch and
  chunk reads as v4's fallbacks with a ported strict-repository scope
  (P4.142); the data/zod smalls (P4.143); the fold-episode lines + the alias
  widening + the MPJ llm-logs comparand (P4.144); the attachment-only
  sentence + the `[Attached: …]` bubble + the ruled placeholder bytes
  (P4.145). **The §3 review (six readers): ONE BLOCKING lane regression —
  P4.139's wizard sent `''` for a keyless vision profile where v4 keeps the
  primary's key — and ONE blocking union handoff — without P4.142's strict
  wraps, backup and `.qtap` export silently dropped every vaulted character
  on a broken mount index; both fixed and pinned.** Also fixed: the cascade
  delete's shared-image check made strict (a recorded divergence, surfaced
  for the human's confirmation); background cheap tasks retry a real
  transport timeout (`is_timeout_failure` by kind — the bug-107 shape);
  the Anthropic usage read; the Ollama slash; P4.143's `strictFailures`
  pin converged; the census import guard widened; P4.140's POSIX-`TZ` docs
  corrected. Gate: the sweep 576 ok / 3 standing / 3 refused of 582 (no
  catch); Tier R 271/0; 660 binaries / 4,223 / 5 (three standing + two
  env-block artifacts) / 3 ignored, zero SKIP; SPA 466 / 8,801; full
  Playwright 357 / 0 / 6. Versions: core 0.0.1183, harness 0.0.1110, host
  0.0.176, web 0.0.213, tauri 0.0.8, SPA 0.5.794. **Next: the `e5c6bd0c0`
  drift catch-up, the owed dogfood pass, then a follow-ups smalls round** —
  `phase-4.md`. Round record: `status-log.md`.
- **The `e5c6bd0c0` drift catch-up round (P4.D245 ∥ P4.D246 ∥ P4.D247 ∥
  P4.D248): UNIFIED on main (2026-10-03) — ALL FOUR LANDED WHOLE; the oracle
  baseline MOVES to `e5c6bd0c0` and the ledger's §3 is EMPTY — but the v4
  checkout went DIRTY mid-gate (an uncommitted Inform-permanent change with
  a NEW migration), so PIN REQUIRED.** v4 `9753d0eb2` absorbed: the project
  roster as a TOOL-ACCESS gate through ONE `project_roster_access`
  chokepoint at v4's seven sites (the enumeration twin folded first; a NEW
  real-DB family), the `allowAnyCharacter` create default `true` at both of
  v4's sites with the READ default kept, the enriched project PUT, the
  chat-create roster auto-add deleted, and the Characters card whole — which
  fixed a v5 defect: the card had read a phantom `roster` key since P4.6l,
  so every v5 project showed an empty roster. v4 `e5c6bd0c0` (this port's
  own bug-175/176 filings, fixed upstream in a different shape): a failed
  collapse now DEFERS and boots (P4.135's D184 FATAL ruling OVERTAKEN), the
  PHASE 3.1 structural pass + the `/health` `structure` service (503
  `degraded`, with the SPA's carve-out so a damaged instance stays
  reachable), and — by the human's mid-lane R3 reversal — an absent
  dedicated table CREATED at boot from v4's DDL dump. **The §3 review (four
  readers): no blocking finding; the one that would have shipped — a created
  `doc_mount_files` lacked v4's UNQUOTED sha256 index, invisible to a test
  that filtered both sides the same way — fixed red-first**, with a
  SAVEPOINT per created table, the lock clearing the structural record, and
  an empty `projectId` as missing context (it had resolved the `project`
  scope to the whole `files/` root). Gate: sweep 579 ok / 4 / 3 of 586
  (three standing + the per-baseline `ai_import` stamp); Tier R 271/0; 665
  binaries / 4,272 / 6 (standing + env artifacts + a live-checkout guard red
  only from v4's dirt) / 3 ignored, zero SKIP; SPA 467 / 8,816; Playwright
  358 / 0 / 6; §S.1 live (degraded 503 → the dashboard renders). Versions:
  core 0.0.1199, host 0.0.179, web 0.0.216, tauri 0.0.9, SPA 0.5.797.
  **Next: the owed dogfood pass, then a follow-ups smalls round, then the
  Inform catch-up when v4 commits it** — `phase-4.md`. Round record:
  `status-log.md`.
- **The `f6426e196` recorded-divergences + `e5c6bd0c0` roster/structure
  dogfood pass RAN (2026-10-03, agent-driven, on the Friday copy) — 42 rows,
  38 PASS, 4 BLOCKED (each reasoned), THREE findings: #135 + #137 FIXED, #136
  ORDER-PENDING.** Walk doc:
  `dogfood-walks/2026-10-03-recorded-divergences-and-roster-pass.md`; record
  in `status-log.md`. The §2 probe FAILED at walk start (v4 committed the
  standing-informs change as `52d6e7ecd`) → `/driftcheck` first (DRIFT
  PENDING, 1 commit, PORT, a new migration); no row touched Inform. ⭐ The
  roster gate on a real turn both ways (v4's sentence + `allowed=false`, then
  admitted after a card add with no restart) and the operator wardrobe
  bypass; ⭐ a damaged structural table → `/health` 503 `degraded` and the SPA
  opens anyway; ⭐ v4's catch lines on posed + real endpoints (the timeout's
  one cheap retry, the 2xx guard, `fetch failed` → `network`, OpenRouter #7
  alone); the API-key, fallback, import and fold-episode rows. **FIXED #135**
  (`ef9c74c02`, SPA 0.5.798): every project's Files card listed its files
  nameless at `0 B` (keys the wire never sends, frozen in the spec since
  P4.6l). **FIXED #137** (`eec676900`, core 0.0.1200): the backup/export failure lines in
  v4's words with the bare message. **#136 ORDER-PENDING:** a project's /
  group's `properties.json` loses v4's explicit `null`s on the first v5 write
  and the read wire omits them (the `Option` + `skip_serializing_if` seam).
  **Next: the `52d6e7ecd` standing-informs catch-up (with #136 as a rider
  order), then the not-run rows** — `dogfood-findings.md` standing notes.
- **The `52d6e7ecd` standing-informs drift catch-up round (P4.D249 ∥
  P4.D250 ∥ P4.146): UNIFIED on main (2026-10-04) — ALL THREE LANES LANDED
  WHOLE; the oracle baseline MOVES to `52d6e7ecd`; the ledger's §3 holds
  ONE row (`a434c715b`, bugs 177/178 — PDF text extraction, UNPROCESSED,
  pushed), so the regen rule is PIN REQUIRED at `52d6e7ecd`.** v4's standing
  (per-chat) informs absorbed whole: the FOURTH D23 re-dump (EXACTLY one
  line — `chat_informs.permanent INTEGER DEFAULT 0`) + v4's migration
  re-homed as a boot ensure with BOTH v4 shapes carried (generateDDL's
  nullable schema-order column for a fresh table, the migration's `NOT NULL
  DEFAULT 0` appended for an existing one — the ensure itself
  differentially compared against v4's REAL migration, `table_info` +
  `sqlite_master` byte-equal), ONE `is_inform_in_force` predicate under
  every read and delete with a source census, the delivery/posting
  comparators, the block's swipe merge + stamped-row exclusion + `standing`
  counts, the `permanent` body key as a tri-state refusing `null` on both
  transports, the 201 / list / cancel bytes, import / export key order /
  backup / restore carriers, `help/inform.md` + five `docs/v4/` paths +
  `qtap-export.schema.json` re-vendored (P4.D249); the SPA's "Keep it
  standing in this chat" checkbox, the flipped guidance tail, the three-way
  toast, the chip's standing label / hover / withdraw title, the three
  contract types, and ONE gated Playwright beat the unifier flipped LIVE
  (P4.D250 — which also found and fixed a pre-existing v5 defect: the
  guidance paragraph ran two sentences together where Angular drops a
  whitespace-only node); and **dogfood #136 FIXED** — the eleven project +
  two group `.nullable().optional()` keys three-state over `double_option`,
  so a v4-written `properties.json` keeps its explicit `null`s through read,
  read-modify-write and create (LUC Ranch's one toggle changes ONE key),
  create writes v4's `color || null`, the home wire and the group import
  follow, the tier-2 corpora grown with null cells through v4's REAL overlay
  and the routes families' masks LIFTED (P4.146). **The §3 review (four
  parallel readers + the unifier): NO blocking defect — the fourteenth such
  round; seven should-fixes landed, led by a false claim:** P4.D249's items
  9/10 said the `[Inform]` debug lines and `Inform batch created` were
  capture-pinned — the differential compared v4's field order against
  LITERALS and nothing captured v5's lines (now captured, armed against
  tracing's `Interest`-cache race after the pin flaked once); the routes
  family's three `permanent` 400 arms passed a CONSTANT as v5's effects (now
  a before/after row count); the backup's `F::Bool` failed whole on a NULL
  cell the reader maps to `false` (`F::BoolDefault`); P4.146's group import
  kept a non-string colour where v4 fails the group; P4.D250's beat could
  pass on a one-shot server (now waits on the saved assistant row); the
  sweep driver's second `--` (zero tests, reported OK) fixed with a
  self-test. Gate: fmt/clippy both feature sets/release clean;
  the full sweep from the pin 581 ok / 3 standing / 3 refused of 587 (no lane- or unifier-caused red); Tier R 271/0; `cargo test --workspace` 666 binaries / 4,315 / 5 (three standing + two env-block artifacts, each green by name) / 3 ignored, zero SKIP;
  SPA 467 / 8,828; full Playwright 354 / 5 / 6 (12.6 m; every red green alone — the P4.66 bubble, the regenerate-stream intermittent 3/3 twice, and three timing-cluster files 2/2, 13/13, 3/3; the standing beat LIVE and green). Versions: core 0.0.1205,
  host 0.0.180, web 0.0.217, SPA 0.5.802; harness frozen 0.0.1110. 💸 the
  dogfood queue gains the standing inform end to end on the Friday copy,
  the first boot ALTERing the 34-row table, the export/import/backup/restore
  round trips, the SPA checkbox + chip, LUC Ranch's one-key toggle, the null
  colour on create / home / group editor, a refused `"color": 5` import.
  **Next: the `a434c715b` bug-177/178 catch-up, the owed dogfood pass, then
  the follow-ups smalls round** — `phase-4.md`. Round record:
  `status-log.md`.
- **The `07b8f0209` two-commit drift catch-up round (P4.D251 ∥ P4.D252 ∥
  P4.D253): UNIFIED on main (2026-10-05) — ALL THREE LANDED; the oracle
  baseline MOVES to `07b8f0209` and the ledger's §3 is EMPTY (v4 AT the
  baseline, clean).** Bug 177 absorbed (the PDF arm reads the
  `DocumentTextExtractor` seam FIRST with v4's three lines, under the first
  tier-1 family over `extractFileContent` — the old header's premise was
  false: pre-fix v4 failed EVERY PDF); bug 178 NO-PORT-RATIFIED. The
  impersonated-line voice is a three-state mode end to end: the FIFTH D23
  re-dump (`impersonationVoiceRewrite` → `impersonationVoiceMode TEXT DEFAULT
  'off'` in place), v4's `impersonation-voice-mode-v1` re-homed as a boot
  ensure that REPLACES the P4.D179 one (kept, it would ping-pong a shared
  instance with v4) and is differentially compared in three shapes, the PUT's
  enum arm with the retired key ignored, the Almanack's labels, the restore's
  legacy translation, 17 pairs narrowed; the SPA's `ask` draft stage with NO
  model call, Restate, the three-radio card, both beats LIVE. **The §3 review
  (three readers + the unifier): no blocking finding — the fifteenth such
  round — but one real SPA defect and five tests that could not fail, all
  fixed:** the settings radios re-lit the OLD mode for a macrotask after every
  successful save (a Playwright `.check()` race); P4.D253's corpus could not
  tell converter-first from fallback-first (bug 177 itself — now a precedence
  row, mutation-proven); "Send posts it" passed with nothing posted; `Always
  restate` never compared; the ping-pong plant matched the translation; the
  legacy float row re-drove the integer arm. Gate: the round record. Versions:
  core 0.0.1208, host 0.0.181, web 0.0.219, SPA 0.5.805. **Next: the owed
  dogfood pass (the Friday copy's first boot translates its toggle `1` →
  `'ask'` — measured), then the follow-ups smalls round** — `phase-4.md`.
  Round record: `status-log.md`.
- **The two-round dogfood pass RAN (2026-10-05, agent-driven, on the Friday
  copy) — 28 rows, 28 PASS, ZERO v5 defects in the rounds' surfaces; the
  restore row surfaced TWO restore defects v4 shares (ruling: FIX v5).**
  Walk doc: `dogfood-walks/2026-10-05-standing-informs-voice-mode-pdf-pass.md`;
  record in `status-log.md`. v4 had already migrated the live instance past
  both rounds, so the boot ensures were proven as exact NO-OPS. ⭐ The `ask`
  draft stage on a real Salon turn: zero `VOICE_REWRITE` rows until Restate,
  exactly one after, none on a picker change, Cmd+Enter posting verbatim;
  `always` eager, `never` direct. ⭐ A standing inform delivered on two real
  turns (absent from the non-target's request), carried on a swipe,
  withdrawn, refusing `permanent: null` on both transports, surviving a
  `.qtap` round trip and a restore. ⭐ LUC Ranch's toggle moved ONE line.
  ⭐ A PDF through the converter seam's fallback on the wizard and Summon
  From Lore. ⭐ A planted retired-key backup restored as `'ask'`.
  **ORDER-PENDING #141 (high):** a `replace` restore orphans every
  character's (and project's, group's) archive store behind a fresh one —
  Friday's 805-link vault unreferenced, 144 stores from 77. **#142:** the
  files phase resolves Uploads through the TARGET's pointer, so a restore
  into a fresh instance loses every project-less file. #140 import-warning
  tails (smalls); #138/#139 v4-faithful. ⚠ **Walk rule:** launch the dogfood
  server with the 2-hour background limit — 30 minutes killed a restore.
- **The `07b8f0209` follow-ups + restore round (P4.147 ∥ P4.148 ∥ P4.149 ∥
  P4.150 ∥ P4.151 ∥ P4.152): UNIFIED on main (2026-10-05) — ALL SIX LANES
  LANDED; the oracle baseline STAYS `07b8f0209` (no drift absorbed; §3
  EMPTY, v4 AT the baseline at both probes).** The three dogfood orders of
  the 2026-10-05 walk + the follow-ups smalls: **#141 + #142 FIXED** — a
  `replace` restore keeps every character on the ARCHIVE's vault and every
  project/group on its archive store (Shape A preserve-at-create, pinned
  both ways as `FRESH_STORE_RESIDUAL` over 20 cases), and the archive's
  built-in pointers are pre-applied after 22a so Uploads resolves to the
  archive's store on a fresh target (`FRESH_TARGET_UPLOADS`); the inform
  `permanent` arm CONVERGED; **#140 FIXED** — every `.qtap` import warning
  carries v4's BARE tail under a NEW `import_warning_text_guard` (103
  offenders on unported main → 0), the project import validates BEFORE it
  writes, `parse_properties` carries v4's hex / 50-code-point / uuid /
  ON-OFF rules with the ZodError bytes, the six `z.uuid()` gates; the
  repository-fallback class round 3 (v4's RETHROW lines as homes, the
  BLOB-named profile DROPPED per row, the chat PUT gate + `list_chats` as
  fallback reads, three held sites converted + twenty re-classed); the
  `doc_mount_points` four-ALTER heal with v4's lines, the collapse's ONE
  line, Google's `No parts found` WARN + the `content.text` fallback
  (measured REACHABLE), the display zone a REQUIRED spine argument; the
  harness smalls (v4's real `ChatSettingsSchema` over stored modes, the
  four-mode informs ensure, the Google `.wire` row, PDF rows under a
  scripted converter over the REBUILT generators pair); the nine
  project-detail toasts on v4's sentences, the picker refocus
  (`afterRenderEffect` — a plain `effect` focuses before the projected node
  re-attaches, measured). **The §3 review (six parallel readers): ONE
  BLOCKING, lane-introduced regression — the import's new pre-validate ran
  on the RAW bag ahead of v4's create seed (`allowAnyCharacter ?? true`),
  so a project carrying `null` that v4 imports OPEN was DROPPED — fixed
  through v4's seeded create-time parse at the import and both restore
  arms, red-first in the oracle; eleven should-fixes with it** (`list_chats`
  emptying on one corrupt row where v4 drops the row; the inform wraps'
  read-failure line; `modelClass` off the profile drop; the preserve arm
  unvalidated + four `sqlite error:` tails; a dead duplicated helper; seven
  more snake_case import lines; a route test's hand-baked MINTED id passing
  on the not-found arm; a census blind to `\`-continued SQL; v4's one
  `onTableEnsured` split in two). Gate: fmt/clippy (both feature sets)/release clean on the final tree; the full sweep from the pin 586 ok / 3 standing / 3 refused of 592 (no lane- or unifier-caused red; every round family ok by name); Tier R 271/0; `cargo test --workspace` 671 binaries / 4,377 / 6 (the three standing + two env-block artifacts, each ok in the sweep) / 3 ignored, zero SKIP; SPA 467 files / 8,889, lint + build clean; full Playwright **353 passed / 7 failed / 6 skipped (12.9 m)** — the six skips the standing parks; the seven reds EXACTLY the recorded Salon-streaming timing cluster in five untouched spec files (`salon-regenerate-stream-flow`, `salon-roleplay-template-flow` ×2, `salon-streaming-avatar-flow` ×2, `salon-thinking-indicator` — the P4.d17 quill, `salon-transcript-subscribed-read`), each re-run by FILE alone afterwards, one invocation at a time: **3/3, 2/2, 2/2, 1/1, 2/2 — all green**; P4.152's two touched files (`projects-flow`, `salon-inform-flow`, the standing beat LIVE) green in the full run. The `salon-regenerate-stream-flow` counter: red in the full suite, 3/3 by file (6 of 10 by-file runs green since 2026-09-30). Versions: core 0.0.1234, host 0.0.185, web 0.0.222, SPA 0.5.809; harness frozen 0.0.1110; cli 0.0.29, tauri 0.0.9 unchanged. 💸 the dogfood queue gains
  the six lanes' rows (phase-4.md NEXT 1). **Next: the owed dogfood pass,
  then the follow-ups smalls round (three human rulings first: the
  preserve arm's completeness guard, the `pdf-parse` vm-modules row, the
  sync ruling), then a drift catch-up when v4 moves** — `phase-4.md`. Round
  record: `status-log.md`.
- **The `07b8f0209` follow-ups + restore dogfood pass RAN (2026-10-06,
  agent-driven, on the Friday copy + planted clones + a fresh instance) — 21
  rows, all terminal; TWO v5 defects FIXED in place, ONE high-severity gap
  ORDER-PENDING.** Walk doc:
  `dogfood-walks/2026-10-06-followups-restore-imports-fallbacks-pass.md`;
  record in `status-log.md`. ⭐ **#141 + #142 closed live:** a `replace`
  restore of a fresh 1.27 GB backup kept all 77 store ids and every entity's
  pointer (Friday on her 807-link vault), and a restore into a freshly `setup`
  instance adopted the archive's Uploads store with zero Uploads warnings.
  Also live: #140's bare import tails + Zod refusals with no half-written
  store, the nine project-detail toasts, the refocus, P4.149's fallbacks on
  planted clones, P4.150's four ALTER lines + the ONE collapse line + Google's
  `No parts found`. **FIXED:** #143 (a refused project-detail select kept
  showing the refused value; SPA 0.5.810) and #144 (one unreadable chat row
  made EVERY project GET answer 500 — `chats_read::find_all` now drops it as
  v4's `_findAll` does; core 0.0.1235) + #147 (a log prefix). **#149
  ORDER-PENDING (high):** a v5-provisioned instance carries NONE of the 57
  secondary indexes a migrated v4 instance has (four UNIQUE) — a restore into
  one took 2 h 26 m against 9 m; measure a real v4 first boot FIRST (standing
  note). #145/#146 smalls; #148 v4-faithful. **Still owed:** the standing
  queue.
- **The `94fbb1ae3` fresh-instance-indexes + follow-ups smalls round
  (P4.D254 ∥ P4.153 ∥ P4.154 ∥ P4.155 ∥ P4.156 ∥ P4.157 ∥ P4.158): UNIFIED on
  main (2026-10-06) — ALL SEVEN LANDED; the oracle baseline MOVES to
  `94fbb1ae3` and the ledger's §3 is EMPTY (v4 AT the baseline, clean).**
  `94fbb1ae3` absorbed (P4.D254): the inform block under v4's ONE vouching
  header as a TRAILING context section (after recall / mail / progressions,
  before the turn-skip note; the trailing-only message on chained turns), the
  new `[Inform] Delivering …` line capture-pinned per op. **Dogfood #149
  FIXED** (P4.153): a real v4 first boot measured, v4's MIGRATION-created
  index family dumped through its real `MigrationRunner` into a committed
  `migration_indexes.json` (D23) and replayed at provisioning
  (`idx_doc_mount_folders_mp_path` UNIQUE, the human's ruling);
  **#146** (P4.154 — the V8 `JSON.parse` twin rewritten from V8's parser,
  every template recorded on Node 24.13.1); **#145** (P4.156 — the inform
  surface on v4's fallback shape and keys); the import's whole-entity
  refusal with v4's three lines (P4.155); eight restore smalls incl. the
  preserve-arm completeness backfill and a 63-line log census (P4.158); the
  generators pair rebuilt + one `v4_root()` (P4.157). **The §3 review (six
  readers): ONE BLOCKING lane defect, ONE false claim, ONE planning gap — all
  fixed with tests:** the backfill matched prompts/scenarios by PROJECTED
  path, so a renamed prompt or heading-titled scenario was written AGAIN on
  every `replace` restore (a second default prompt) — now by parsed name;
  P4.154's "no V8 shape left" was false (two classes found by fuzzing real
  Node, recorded, fixed); P4.153's re-aim left nothing comparing v5's tables
  with v4's LIVE generateDDL — `provisioning_equivalence` (1d) now REQUIRES
  `QT_FRESH_SCHEMA_LIVE` and restores D23's tripwire. Every HANDOFF landed
  bar §S.2's whole-row inform validator (deferred by name). Gate: sweep 587
  ok / 3 standing / 3 refused of 593; Tier R 271/0; 673 binaries / 4,408 / 6
  (standing + one env artifact + the unifier's census catch, fixed) / 3
  ignored, zero SKIP; SPA 467 / 8,889; full Playwright 359 / 2 / 6 (12.7 m) — both reds green by file alone (the voice `ask` beat 3/3; the recorded `salon-regenerate-stream-flow` intermittent 2/3 with a different beat, then 3/3). Versions: core 0.0.1242,
  host 0.0.187, SPA 0.5.811. **Next: the owed dogfood pass (the fresh-
  instance restore timing; Friday's disbelieved inform), then the follow-ups
  round (the index family on pre-round instances first)** — `phase-4.md`.
  Round record: `status-log.md`.
- **The `94fbb1ae3` smalls-round dogfood pass RAN (2026-10-06 evening,
  agent-driven, on the Friday copy + planted clones + three fresh instances)
  — 19 rows, all terminal; ZERO defects in the round's surfaces, THREE
  pre-existing divergences ORDER-PENDING.** Walk doc:
  `dogfood-walks/2026-10-06-inform-trailing-indexes-restore-backfill-pass.md`;
  record in `status-log.md`. ⭐ `94fbb1ae3`'s motivating case flipped on one
  transcript: the standing Tessarium inform rides the final user message
  under the vouching header and Abigail now believes it. ⭐ **#149 closed
  live:** a fresh `setup` instance carries the migration index family and the
  1.28 GB restore into it took **7 m 17 s** (2 h 26 m before). ⭐ P4.158's
  backfill, store-claim fallback and parsed-name matching on a real archive;
  #145/#146 closed live. **#150 (high):** a corrupt sibling DB stops v5's boot
  where v4 boots DEGRADED. **#151:** v4's middleware ERROR for a
  store-unavailable 503 never logged. **#152:** an import writes a memory v4's
  `MemorySchema` refuses. **Still owed:** the standing queue.
- **The `94fbb1ae3` boot-hardness + validation + follow-ups round (P4.159 ∥
  P4.160 ∥ P4.161 ∥ P4.162 ∥ P4.163 ∥ P4.164): UNIFIED on main (2026-10-07) —
  ALL SIX LANES LANDED; the oracle baseline STAYS `94fbb1ae3` — but v4 moved
  SIX commits past it during the round (the wardrobe wear ledger + item
  images, three NEW migrations), recorded UNPROCESSED in the ledger's §3;
  by the human's ruling every regen stayed PINNED (PIN REQUIRED).** Dogfood
  **#150 FIXED** (P4.159 — a sibling that cannot open or fails `quick_check`
  boots DEGRADED: v4's mount-index ladder, the LLM logs' one attempt, no
  writer / no pool, Absent ≠ Degraded, `/health` 503 `degraded`; v4 Bug 180
  filed); pre-round instances BACKFILL v4's migration index family at boot
  (P4.160); **#152 FIXED** (P4.161 — memories, informs, prompt templates,
  folders, tags, file rows validated WHOLE on import / restore through ports
  of v4's schemas); **#151 FIXED** (P4.162 — v4's context-middleware ERRORs,
  RFC uuid gates, log-text smalls); the repository-fallback class round 5
  (P4.163 — C1's homes, the delete / docedit callers, the `[Projects v1]`
  lines); harness smalls (P4.164). **The §3 review (six readers + the
  unifier): ONE BLOCKING finding — P4.161's restore refused the `{"0":…}`
  embedding every real full backup carries (v4's own restore drops every
  embedded memory) — RULED FIX v5 (`INDEX_KEYED_EMBEDDING`, both ways);
  eight should-fixes and three sweep-caught union reds fixed red-first; the
  pin's symlinked `node_modules` had followed the live checkout to HEAD's
  deps — `npm ci --offline` in the pin, sweep re-run whole.** Gate: sweep
  592 ok / 4 / 3 refused of 599; Tier R 271/0; 674 binaries / 4,492 / 5
  (standing + env artifacts) / 3 ignored, zero SKIP; SPA 467 / 8,889;
  Playwright 350 / 11 / 6 (16.9 m) — all eleven reds green by file alone (the seven-file Salon-streaming cluster + `salon-documents-flow` ×2 and `workspace-flow` ×2, new to it). Versions: core 0.0.1252, host 0.0.189, web 0.0.224.
  **Next: the owed dogfood pass (a full Friday restore's embedding count
  first), `/driftcheck` once v4 settles, then the follow-ups round** —
  `phase-4.md`. Round record: `status-log.md`.
- **The `f5e953a3f` wardrobe-programme + convergence drift catch-up round
  (P4.D255 → {P4.D256 ∥ P4.D262 ∥ P4.D263 ∥ P4.D264} ∥ P4.D257 ∥ P4.D258 ∥
  P4.D259 ∥ P4.D260 ∥ P4.D261): UNIFIED on main (2026-10-08) — ALL TEN LANES
  LANDED; the oracle baseline MOVES to `f5e953a3f`; the ledger's §3 holds the
  NINE commits v4 landed mid-round (`1825bfd53` … `783385873` — the memory
  programme + a release-notes update, UNPROCESSED; a SEVENTH D23 move
  pending), PIN REQUIRED.** v4's
  wardrobe programme whole: the SIXTH D23 re-dump (`wardrobe_wear_stats` +
  `chat_settings."wardrobeImageSettings"`; NO `migration_tables.json` — R-A
  overruled) with three boot ensures (P4.D255, the keystone); the read-time
  `origin` + `wear`, `?action=wear-history`, delete hooks (P4.D256); the ONE
  `commit_equipped_outfit` chokepoint under every equip path + the four tools'
  wear text and pictures (P4.D262); item images — route, generation job,
  settings, transfers, Almanack (P4.D263); the backup / restore / `.qtap`
  carriers (P4.D264); the chat gallery's two passes (P4.D257); bugs 180/181
  CONVERGED + bug 179 NO-PORT (P4.D258); physical backups + retention + the
  daily optimize + `quilltap db optimize` (P4.D259); the dependency move +
  `help/` 130 + `docs/v4/` (P4.D260); the whole SPA side (P4.D261). **The §3
  review (seven readers + the unifier): ONE finding that would have shipped —
  P4.D259's startup `VACUUM INTO` ran beside a live writer (TRUNCATE →
  `database is locked`); now JOINED before the pumps as v4's `connect()` does,
  pinned** — plus eight should-fixes red-first (a General archetype through a
  character's item routes — a DELETE wiped its ledger; a manual join left
  undressed; a malformed settings cell dropping the row; R-B's untested 500
  arm; a web 500 masked as 400; silent read swallows; a spent provider call;
  stale claims) and three union reds the gate caught (P4.D263's grown builder
  under P4.D256's plants; a shared `/tmp` item-images fixture; the flipped
  beats' worker-restart cascade + a fixture without `chat_settings`). Gate:
  sweep 608 ok / 3 standing / 3 refused of 614; Tier R 282/0; 697 binaries /
  4,621 / 6 (standing + env artifacts + the fixed collision) / 3 ignored, zero
  SKIP; SPA 474 / 9,012; Playwright 366 / 0 / 6 (11.8 m). Versions: core 0.0.1264, host
  0.0.193, web 0.0.226, cli 0.0.30, SPA 0.5.821. **Next: the owed dogfood
  pass, then the memory-programme catch-up round, then a follow-ups smalls
  round** — `phase-4.md`. Round record: `status-log.md`.
- **The `f5e953a3f` wardrobe-programme + owed `94fbb1ae3` boot-hardness
  dogfood pass RAN (2026-10-09, agent-driven, on the Friday copy + five planted
  clones + two fresh instances) — 36 rows, all terminal; FOUR v5 defects FIXED
  in place, two ORDER-PENDING, one ruling requested.** Walk doc:
  `dogfood-walks/2026-10-09-wardrobe-programme-optimize-degraded-boot-pass.md`;
  record in `status-log.md`. ⭐ The day's first boot ran v4's PHASE 0.75 whole
  (backups, optimize, the startup trio joined before the pumps); the wear
  ledger end to end on real turns (one `wardrobe_wear` call wearing two
  garments; `wardrobe_read`'s tally); degraded siblings on clones; a full
  backup → `replace` restore keeping every tally and vector. **FIXED:** #153
  (a stopped `quilltap-web` never released the instance lock — now v4's
  SIGINT/SIGTERM shutdown) and #158 (every multipart `.qtap` import over 2 MB
  answered `No file provided`) in `9ea2945c3` (web 0.0.227, host 0.0.194);
  #155 (Start Chat dead inside the workspace) and #154 in `17792dd49` (SPA
  0.5.822). **ORDER-PENDING:** #156 (115 `with_both_conns` callers answer a
  500 on a degraded mount index), #157 (log text). **RULING:** #159
  (`new-account` restore orphans every vault — v4-shared). C5 (a real
  garment picture) RAN on the human's authorization; v4 bugs 183 / 184 FILED
  (`5abcd01ea`, local). ⚠ **Drift measured:**
  v4 HEAD already tiered + consolidated the live instance; v5 at the baseline
  refuses the 365 `CONSOLIDATED` digests on restore and HARD-DELETES what v4
  demotes — **run no v5 build against live Friday before the memory catch-up**
  (ledger `d58548051`). **Next: the memory-programme catch-up round, then the
  follow-ups smalls round (with #156/#157)** — `phase-4.md`.
- **Oracle baseline: `f5e953a3f` (2026-10-07, v4 main — "feat(startup):
  daily database optimize before migrations", `4.10.0-dev.117`), adopted
  at the `f5e953a3f` wardrobe-programme round's unification (2026-10-08).**
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
