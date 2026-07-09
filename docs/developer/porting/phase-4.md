# Phase 4 kickoff — transports, hosts, and the Angular SPA

> The Phase-4 plan and its **locked decisions**. Read alongside
> [`api-boundary.md`](./api-boundary.md) (the contract this phase implements),
> [`overview.md`](./overview.md) (the roadmap), and
> [`scriptorium-file-manager.md`](./scriptorium-file-manager.md) (the one UI
> component decision settled early). Inventories below come from three fresh
> surveys (2026-07-08): the v5 seam/deferral sweep, the v4 API/transport
> surface, and the v4 UI surface.

## Where we are (entering Phase 4)

**Phase 3 is complete** (2026-07-08, the enclave capstone U4.4). The entire
engine — data layer, memory family, chat orchestration, tools, providers,
danger, post office, job runner, enclave — exists as `quilltap-core`, a
**library**: 700+ tests, every unit differential-verified against v4 (oracle
baseline `2494a84b` after the kickoff-day drift check). What does *not* exist
yet:

- No `Request`/`Response` enums, no `dispatch`, no `QuilltapCore` trait — only
  the `Event`-side vocabulary (`services::chat_events::ChatEvent`, which already
  serializes byte-identical to v4's SSE frames).
- No binaries. No transport. No UI. No composition root that assembles the
  engine's injected seams into a running host.
- A known set of injected host seams with canned/no-op defaults (inventoried
  below), left open **deliberately** — the core is scheduler-free and IO-free by
  design.

Phase 4 is everything between that library and a human.

## What Phase 4 delivers

1. **The Core API boundary, implemented** — the `Request`/`Response`/`Event`
   contract from `api-boundary.md`, plus the composition root that assembles a
   running engine.
2. **Transports** — axum HTTP (first-class, see D2), the CLI, the Tauri 2
   desktop shell. (uniffi/mobile stays deferred.)
3. **Host drivers** — production implementations for every injected seam:
   timers/cadence, provider HTTP, file bytes, image codecs, PTY, fs.
4. **The Angular SPA** — the whole UI, rewritten (v4's React does not port),
   running identically in a plain browser and in the Tauri webview.
5. **The remaining engine surface** — the route-handler-level services v4 has
   that Phase 3 never scoped (chat creation, wizards, backup/restore,
   import/export, help chat, …), ported with the same differential discipline.
6. **Packaging (dev-grade)** — a Dockerfile for the web deployment and a Tauri
   bundle. No release process (standing hard stop).

## Locked decisions

The four `api-boundary.md` invariants are re-affirmed and not restated (one
boundary; streaming only on `Event`; the `Db` ownership model; enclave
`step()` + injected driver). New locks, D1–D22:

### Deployment & transport shape

- **D1 — The HTTP transport is first-class, not a CI shim.** Two co-equal
  deployments ship: (a) the self-contained Tauri desktop app, and (b) a **local
  web server** (Docker-Desktop-style: run the container or the bare binary,
  open a browser). Same core, same SPA. This is also v4's own deployment shape,
  so it is the most oracle-faithful one — and it *is* the "optional companion
  server" from `api-boundary.md` Part 3 (an always-on enclave host), so no
  separate companion-server feature exists.
- **D2 — No authentication.** Confirmed against v4 source: the session layer is
  already synthetic single-user (`lib/auth/session.ts` fabricates a session
  around `SINGLE_USER_ID`; no cookie/token is ever validated) — dropping it
  loses no behavior. v5 is localhost-trust: anyone wanting auth or remote
  exposure proxies through something that provides it (Caddy/Traefik/…).
  The only security knob is the **bind address**: the bare binary defaults to
  `127.0.0.1` (a flag widens it); in a container the server binds `0.0.0.0`
  and the port publish scopes exposure (`-p 127.0.0.1:8080:80` = machine-local).
  **What survives from v4's middleware is the readiness gate, which is not
  auth:** `ensureServerReady` → `PepperNotReadyError` → HTTP 503 with
  `setupUrl` when the pepper vault is locked/unset. Port that concept intact
  (the unlock state machine: `resolved` / `needs-setup` / `needs-passphrase` /
  `needs-vault-storage`, the 3-attempt limit, auto-lock resume).
- **D3 — The dispatch surface is the contract, not v4's REST tree.** The HTTP
  transport exposes: `POST /api/dispatch` (the `Request` enum), `GET
  /api/events` (the `Event` channel as SSE — **one global stream per client**,
  every event tagged with its scope ids `chat_id`/`room_id`/`progress_id`;
  `Last-Event-ID` replay is best-effort per transport), the **binary resource
  routes** (D4), the **terminal WebSocket** (D5), `GET /health`, and static SPA
  serving. v4's 124 REST routes / ~249 handlers are **not** reproduced — they
  are the *checklist* for Request coverage, not the wire shape. (The SPA is a
  rewrite and calls `dispatch`; the CLI does too.)
- **D4 — Binary assets are real URLs, never enum dispatch.** Browsers need
  `<img src>`/font/download URLs. One **resource resolver** lives in the
  core; each transport maps it to native URLs — axum as GET routes, Tauri as a
  custom protocol handler. The v4 binary surface to reproduce:
  `files/proxy/[...key]`, `files/[id]` (+`?action=thumbnail`),
  `mount-points/[id]/files/[...path]`, `mount-points/[id]/blobs/[...path]`,
  `characters/[id]/photos`, `themes/assets/[...path]`, `themes/fonts/[...path]`,
  `wardrobe/preview-avatar`, `wardrobe/analyze-image`.
- **D5 — The terminal is the one sanctioned bidirectional side-channel.** The
  `Event` channel is server-push only; the terminal needs client→server
  (`input`/`resize`/`ping`) as well as server→client
  (`output`/`exit`/`meta`/`pong`/`chat-update` — v4's Zod union, kept
  verbatim). axum: the single WebSocket route (`/api/v1/terminals/[id]/stream`,
  as in v4's `server.ts`); Tauri: paired events. The PTY itself is a host
  driver: a Rust pty crate (**`portable-pty`** as the default candidate)
  replaces node-pty.
- **D6 — Green Room / creation-progress events ride the `Event` channel.**
  v4's in-memory per-id bus (`lib/chat/creation-progress.ts`, levels
  `log`/`info`/`warn`/`error`/`status` + terminal `done`/`error`, buffered so a
  late subscriber replays) becomes `Event` variants + a small core-adjacent
  buffer — **not** a bespoke SSE route. Already noted at the `6bf88959` drift
  check; locked here.
- **D7 — The `Request` enum is action-centric and grows incrementally.** v4's
  behavioral surface is ~**162 action verbs** layered over the routes (the
  `?action=` dispatch pattern dwarfs the REST surface). Mirror that: variants
  named by operation (`(resource, verb)`), added **when a consumer needs them**
  (the SPA vertical or CLI subcommand being built) — the same
  no-speculative-enumeration rule Phase 3 used for `Event`. `Response` uses
  typed DTOs (the uniffi payoff); v4's JSON envelope semantics
  (`lib/api/responses.ts`: success/notFound/badRequest/…) are the reference for
  the HTTP serialization.

### Crate & repo layout

- **D8 — Layout.** The contract types (`Request`/`Response`/`Event`, the
  `QuilltapCore` trait, DTOs) live in **`quilltap-core::api`** — pure types, no
  IO, so every transport shares them without dragging drivers. New crates:
  - **`quilltap-host`** — the composition root + production drivers (tokio
    timers, reqwest wiring, image codecs, PTY, fs, the pricing fetch). The only
    crate that turns seams into real IO.
  - **`quilltap-web`** — the axum transport: dispatch, SSE, binary routes,
    terminal WS, static serving, readiness, bind policy.
  - **`quilltap-cli`** — the `quilltap` binary (v4's `npx quilltap` is its
    oracle).
  - **`quilltap-tauri`** — the desktop shell (last).
  - **`apps/web`** — the Angular SPA.
  The core keeps its default-build purity (no scheduler, no IO); `cargo test`
  on the core must stay green without any host crate.

### Ordering & method

- **D9 — Transport-first ordering.** Boundary + host first, then axum + CLI,
  then the SPA developed **against the axum transport in a plain browser**
  (the fastest dev loop; no webview in the way), Tauri wrapping last. The
  full order is in "Decomposition" below.
- **D10 — The differential discipline does not end.** Any *engine* behavior
  ported in Phase 4 (chat creation, wizards, backup, the markdown renderer, …)
  still arrives with its v4-oracle differential at the appropriate tier.
  Tier 4 (below) covers what has no oracle: transports and the UI.
- **D11 — Schema, cipher, and the writing voice stay frozen.** Same tables,
  same ChaCha20 cipher, same `.dbkey` handling; the post-port cleanup of
  vestigial v4 cruft remains a separate, later effort. User-facing strings keep
  the steampunk/Wodehouse register (the SPA inherits v4's microcopy).

### The CLI

- **D12 — The CLI is dual-mode.** Direct-core when it can own the data dir
  (no server running); an HTTP-dispatch client against a running server
  otherwise — because the single-writer invariant is **per-process** (one
  process owns an instance's DB files; v4 solves the same problem with its
  instance lock + HTTP verbs, and the v5 CLI mirrors that: v4's `docs`
  write/scan verbs already call the running server). Read-only direct opens
  against a running server's files follow the read-path pragma rules.
  Subcommand parity target (from the v4 launcher): `db`, `docs`, `themes`,
  `memories`, `memory-diff`, `instances`, `logs`, `migrations`, `maintenance`,
  `file-verify`, `completion`, plus the global flags
  (`--port/--data-dir/--instance/--open/--passphrase/--version/--update`).
  The v5 launcher's download-manager/native-module-healing machinery does
  **not** port (a single static binary needs neither).

### The Angular SPA

- **D13 — Angular tooling.** Angular CLI defaults (esbuild builder), standalone
  + zoneless + signals throughout; no Nx; npm. **No component library** — v4
  has none either (~20 hand-rolled primitives in `components/ui/`); port those
  primitives as the v5 primitive set.
- **D14 — One transport seam.** A single injectable `CoreClient` wraps
  dispatch + the event stream (fetch/SSE against axum today, Tauri invoke/
  events in the shell — components never know which). **Server state via
  TanStack Query for Angular** (`@tanstack/angular-query` — v4's ~52
  `useQuery`/`useMutation` sites port 1:1 in mental model); the SSE stream is
  consumed by one stream-reducer service feeding signals (v4's
  `useSSEStreaming` union — incremental `content`, live-replace `reasoning`,
  positioned `reasoningSegments`, tool batches spliced at `anchorOffset` — is
  the spec; those two splicing behaviors are the subtle parts).
- **D15 — The `qt-*` theme system ports faithfully.** v4's styling is
  CSS-deep: ~11k lines of custom-property-driven semantic `qt-*` CSS +
  Tailwind v4 (`darkMode: 'class'`) + **6 bundled themes** + the
  `.qtap-theme` pluggable format. All of it ports as CSS (custom properties
  and per-theme stylesheets are framework-neutral); this is a load-bearing
  port, not a reskin, and it is what keeps the app looking like Quilltap.
- **D16 — Markdown pipeline.** Keep **unified/remark/rehype** in the SPA
  (framework-agnostic; `remark-gfm`, `remark-breaks`, `rehype-highlight` carry
  over from v4's react-markdown stack) rather than an Angular-specific
  markdown lib. `qtap-linkify` ports to shared TS for the client. The
  **server-side** markdown renderer (`markdown-renderer.service.ts` +
  `qtap-linkify` step 3.5, used for pre-rendered announcement bodies) ports
  into the core **with a tier-1 differential** — its regex uses lookbehind,
  unsupported by the Rust `regex` crate, so hand-roll the boundary check (the
  Phase-1 name-matcher precedent). The W4.6b writers seam the renderer call
  until this lands.
- **D17 — Composer: spike Lexical's vanilla core, fallback ProseMirror.**
  v4's chat composer is Lexical (0.43) with ~7 custom plugins. Lexical's core
  is framework-agnostic (the React bindings are a separate package), so an
  Angular wrapper is plausible — spike it; if it fights, ProseMirror is the
  fallback. (Same spike-with-fallback pattern as the file manager.)
- **D18 — Settled component choices carry over.** File manager: **ngx-explorer
  spike, build-our-own fallback** (`scriptorium-file-manager.md`, unchanged).
  Terminal: **xterm.js** (framework-agnostic, ports directly). Virtualized
  message list: **`@tanstack/virtual-core`** (vanilla core; matches v4's
  variable-height behavior — Angular CDK's autosize is still experimental).
  Drag-and-drop: **Angular CDK drag-drop** (replaces dnd-kit). Toasts: a small
  service (v4's is bespoke too). PDF/docx preview: `pdfjs-dist` + `mammoth`
  carry over (framework-agnostic).

### Host drivers

- **D19 — Image codecs: pure-Rust first.** The two `ImageTranscoder` seams and
  the thumbnail/resize paths use the `image` crate family (+ a WebP encoder)
  as the default; native libvips/Sharp-equivalents only if a quality/perf
  spike proves the pure-Rust stack insufficient. (v4 used Sharp; byte-parity
  of encoded output is **not** required — dimensions/policy behavior is.)
- **D20 — All cadence lives in `quilltap-host`.** The core owns no timers
  (locked in Phase 3); the host driver implements the full timer inventory
  (below) with tokio. The web deployment and the Tauri app run the **same**
  driver set; only mobile (deferred) would differ.

### Non-goals (Phase 4 explicitly does not include)

- **D21 — Deferred surfaces:** uniffi + any mobile shell (until Tauri-mobile
  is proven/disproven); a plugin system beyond the provider manifests (v4's
  npm plugin tools/routes — `plugin-routes/[...path]`, plugin tool dispatch —
  do not port; the `ToolRunner` inner-fallback seam stays, loud);
  release/publishing/signing/updater work (dev builds + the in-repo Dockerfile
  only).
- **D22 — No new features during the port.** Parity with v4 first; the
  v5-only capabilities already banked (e.g. `markCompleted`'s payload merge)
  stay dormant until post-parity.

## The boundary contract in detail

What the surveys pin down beyond `api-boundary.md`:

**Event families.** The `Event` enum is the union of four vocabularies, all
already characterized:

1. **Chat stream frames** — `services::chat_events::ChatEvent` already exists
   and serializes byte-identical to v4's `StreamChunkData` SSE frames (content
   deltas, cumulative reasoning, status stages, tool detection/results, turn
   lifecycle `turnStart`/`turnComplete`/`chainComplete`, done payloads, errors).
   The Brahma console, help chat, and the character wizard streams **reuse this
   same frame shape** in v4 — they become the same `Event` family, scope-tagged.
2. **Creation progress** (D6) — leveled log/status frames with per-id replay.
3. **Low-vocabulary progress** — backup, model-classes enumeration, system
   tools listing (v4 streams these too; simple progress/log frames).
4. **Terminal** (D5) — the bidirectional side-channel, not part of `Event`.

**Response envelope.** Typed DTOs per variant; the HTTP serialization follows
v4's envelope semantics. Two cross-cutting responses: the readiness 503
(`setupUrl`, `pepperState`) and the standard error envelope.

**The readiness gate** (D2) is a boundary concern, not a transport one: every
transport must refuse dispatch (except the unlock/setup family) until the
pepper vault is unlocked.

## The remaining engine surface (route-logic backfill)

Phase 3 scoped the per-turn engine. v4's route handlers also carry real logic
that has **no v5 port yet** — each of these is an ordinary differential-ported
unit (tier per its nature), done in Phase 4 when its consumer (SPA vertical /
CLI subcommand) needs it:

| Unit | v4 source | Notes |
|---|---|---|
| **Chat creation flow** | `POST /api/v1/chats` + `apply-outfit-selections.ts` + participant actions + chat merge | Flagged at the `6bf88959` drift check; composes ported repos/services + the `llm_choose` outfit path; emits creation-progress events (D6). |
| **Character AI wizard + AI import** | `?action=ai-wizard-stream` / `ai-import-stream` handlers | Streaming; reuses the chat frame family. `strip_code_fences` already ported. |
| **Help-chat orchestrator** | `lib/services/help-chat/orchestrator.service.ts` | The help tools + `help_docs` search are ported; the orchestrator shell is not. |
| **Brahma streaming console** | `lib/services/brahma-console/orchestrator.service.ts` | The one-shot is ported (W4.5b); the interactive streaming console is not. |
| **Backup / restore** | `v1/system/backup*`, `v1/system/restore` | Streams progress; touches the raw DB files — respect the single-writer + copy rules. |
| **Import / export** | the `import`/`export`/`*-preview`/`export-entities` actions | Entity round-trip logic. |
| **Standalone Document Mode ops** | `lib/documents/operator-doc-actions.ts` + `STANDALONE_CHAT_ID` | Second consumer of the `chatDocuments.renameFilePathInStore` move-sync seam. |
| **Server-side markdown renderer + qtap-linkify** | `markdown-renderer.service.ts`, `lib/chat/qtap-linkify.ts` | D16; tier-1 differential; lookbehind hand-rolled. |
| **Unlock / pepper-vault service** | `v1/system/unlock` (`setup`/`unlock`/`store`/`lock`), pepper vault, change-passphrase, auto-lock | `dbkey` decryption is ported; the vault/state-machine service around it is not. |
| **auto-associate** | `auto-associate.ts` + 3 unscoped `findApiKeyById` sites | Small settings feature. |
| **Themes service** | `v1/themes*` + theme validation | Serves the `.qtap-theme` bundles the SPA consumes. |
| **Misc system/UI routes** | `ui/search`, `deployment`, `startup-status`, `migration-warnings`, `browse-directory`, `data-dir`, `home`, `image-aesthetics`, `search-replace` | Mostly thin composition over ported repos (`replace_in_messages`/`replace_in_memories` exist); port as dispatch handlers with spot differentials where logic is real. |
| **Wardrobe raw read** | `findByCharacterIdRaw` | Deprecated pre-cutover read; port only if a consumer materializes. |
| **z-ai static model list** | plugin config data | Merge data into the manifest; dynamic path already ported. |

Inherited small follow-ups from Phase 3 (each already precisely narrowed):
the failover retry legs' `LogContext` threading; the real stream duration
clock (`durationMs` is pinned 0); the two live orchestrator corpus cases
(`ask_carina`-through-spine — needs the per-turn `carinaAnswer` sink threaded
through `ToolExecutionContext` — and live-Brahma — needs an `isDefault`
fixture profile); the moderation plugin registry question (v5 ships the
OpenAI moderation wire; a *registry* of moderation providers only if a second
provider materializes).

## Host-driver inventory (what `quilltap-host` must implement)

From the seam sweep (statuses verified against the Phase-3 ledger):

**Provider/model IO** — the production `StreamingCompletionProvider` composer
(**does not exist at all**: `request_builder(stream)` → `ProviderTransport::
execute_stream` → `model::decoders` → `StreamChunk`; this is the biggest
single gap), the `CompletionProvider` adapter over the existing sans-IO
`execute_completion`, a reqwest `WireTransport`/`SyncWireTransport` (for
moderation, embeddings, Serper), the API-path `EmbeddingProvider`, the live
`PricingFetch` (+ populating `build_pricing_context`'s connection-profile api
keys), the real `ImageProvider` HTTP. The `ReqwestTransport` behind
`native-transport` already exists for non-streaming — wire it in and add the
streaming half.

**Files/images/fs** — `FileBytesStore` ×2 (chat files + photo album: the FSM
byte layer, i.e. the upload/ingest half of `chat-files-v2`), `ImageTranscoder`
×2 (D19), `ProjectImageUpload`, the `ApplyHost` fs mutators
(rename/mkdir/staging-cleanup/invalidations), the filesystem/obsidian mount
branches behind `ResolveError::FsSeam`, `doc_open_document`'s new-blank-file
path, the help-docs disk sync, and `deleteFileCompletely`'s storage-bytes
half.

**Terminal** — the PTY manager (D5) + `TerminalScrollbackSource`.

**Timers/cadence** (all tokio, in the host): the job-runner pump
(`PumpOutcome.next_wake_ms` + the enqueue wake hook), the 5-minute stuck-job
reset, the enclave 60 s schedule tick + per-`StepOutcome` re-enqueue, the
scheduler sweeps, the `withTimeout` family (answer-confirmation 25 s/60 s,
image-description 60 s, pricing 3 s), `TransportPolicy` knobs, and —
optionally — backgrounding the async-compression trigger (v5 currently awaits
inline; same DB effect).

**Environment** — `SelfInventoryEnv` (version, runtime mode, client shell,
release-notes/changelog reads, mount-index-degraded), `RandomBytes`
(`OsRandomBytes` exists — wire it), instance locking (v4's single-instance
lock semantics), the data-dir/instance resolution (shared with the CLI).

Already real and wired (do not re-scope): `LanternNotificationSink`,
`ErasedAskCarina`/`RealCarinaQuery`, `RealBrahmaConsole` (inert pending its
live corpus case), the danger router's `DbApiKeys`, the
`OrchestratorSeams`/`BuildContextSeams`/`ContextSummarySeams` families, the
built-in `ToolRunner`, the wardrobe transfers service + public read trio
(W4.0), the anthropic sampling/adaptive-thinking rules (W4.7c).

## Verification: tier 4

The oracle discipline continues where an oracle exists; tier 4 covers what has
none:

1. **Core ports keep tiers 1–3.** Everything in "route-logic backfill" ships
   with its differential, per the standing rules.
2. **Transport contract tests.** A committed `Request`+`Event` corpus replayed
   through each transport (axum via real HTTP; Tauri via its IPC harness) must
   produce identical `Response` bodies and ordered `Event` traces. The
   dispatch implementation itself is tested once, below the transports —
   transports prove only marshalling.
3. **Headless end-to-end smoke.** A scripted chat send against a sanitized
   fixture instance through the running axum binary (dispatch → SSE trace →
   DB state), in CI. This is the Phase-4 analogue of the orchestrator
   differential — same fixture lineage, no v4 process involved.
4. **CLI differentials.** Where output is deterministic and comparable, diff
   `quilltap <cmd>` against `npx quilltap <cmd>` on the same fixture (the
   read verbs of `db`/`docs`/`memories`/`instances` are the candidates).
5. **The SPA has no oracle.** v4 is the *behavioral reference*, not a byte
   target: Playwright end-to-end against the axum transport + component
   tests. Visual/UX parity is reviewed by a human against the running v4 app.

## Decomposition and ordering

Units sized for one session each (the Phase-3 work-order model continues —
per-round orders in `work-orders/` as each round starts, one owner per
crate/file region per round, a v4 drift check opens every round):

- **P4.0 — The boundary + composition root.** `quilltap-core::api` (the
  contract types + `QuilltapCore` impl over the engine; first variants:
  health, unlock/setup family, instances, list-chats), `quilltap-host`
  skeleton (Db assembly, job-runner + enclave drivers ticking, seam wiring
  for what's already real). Exit: an integration test boots a fixture
  instance headless and pumps jobs.
- **P4.1 — Host drivers** (parallel lanes, disjoint crates/modules):
  (a) provider IO (the streaming composer + reqwest wiring + pricing +
  embeddings); (b) files/images (FileBytesStore/FSM, transcoders,
  ApplyHost fs); (c) PTY/terminal; (d) environment (SelfInventoryEnv,
  instance lock, data-dir resolution).
- **P4.2 — `quilltap-web`.** Dispatch + SSE + binary routes + terminal WS +
  static serving + readiness + bind policy (D2/D3/D4/D5) + the Dockerfile.
  Exit: the headless e2e smoke (tier-4 #3) runs in CI.
- **P4.3 — `quilltap-cli`.** The launcher + dual-mode + subcommands in
  oracle-parity order (`db`/`docs` first — they exercise the most engine).
  Runs in parallel with P4.2 after P4.0.
- **P4.4 — Route-logic backfill.** The table above, ordered by what the SPA
  verticals need next (chat creation and unlock/pepper-vault first — the
  first vertical needs both). Interleaves with P4.5+.
- **P4.5 — SPA foundation.** `apps/web` scaffold, `CoreClient` + the SSE
  stream reducer, the `qt-*` CSS + theme port, the primitive set, the
  startup-gate chain + unlock + setup wizard screens.
- **P4.6+ — SPA verticals**, each a screen family against the ported
  backfill: Salon read-only (list/open/read) → Salon send (streaming,
  tools, whispers) → full Salon (Document Mode, terminal pane, courier,
  images) → Settings (7 tabs) → Characters/Groups/Projects → Scriptorium +
  Files (the ngx-explorer spike gates here) → Tools/ops cards → Photos →
  Help → Workspace/tabs → Brahma console. (~24 screens, ~535 v4 components
  as the reference inventory.)
- **P4.7 — `quilltap-tauri`.** The shell around the finished SPA: webview,
  custom protocol for resources, the same host driver set, native niceties
  as progressive enhancement (D14's one-seam rule keeps this thin).

Milestones (each independently demoable):

| # | Milestone |
|---|---|
| M0 | Headless engine boots a fixture via `quilltap-host` (P4.0) |
| M1 | `quilltap db --tables` / `docs ls` against a fixture, diffed vs `npx quilltap` (P4.3) |
| M2 | Scripted chat send round-trips over HTTP dispatch + SSE in CI (P4.2) |
| M3 | The Docker image serves unlock + the SPA shell on localhost (P4.2/P4.5) |
| M4 | Browser: unlock → salon list → open chat → send → streamed reply (first vertical) |
| M5 | The Tauri app runs the same SPA against the same core (P4.7) |
| M6 | Screen-parity checklist complete; v4 retirement review (same DB files, so migration = open them) |

## How to resume in a fresh session

Open with: *"Continuing the quilltap-v5 native port. Read CLAUDE.md,
docs/developer/porting/overview.md, and docs/developer/porting/phase-4.md.
Phase-4 status lives in CLAUDE.md — pick up from the latest status block,
writing the round's work orders first."* Run a v4 drift check before each
round. **Progress:** P4.0 (M0), P4.1 (all four host-driver lanes), P4.2
(M2, `quilltap-web` + Docker), P4.3 (M1, the CLI Tier R), the P4.d drift
re-port round, P4.4 unit 1 (the unlock/pepper-vault service +
fresh-instance provisioning; unit 2 — chat creation + Green Room — is the
next P4.4 order), and P4.5 (the SPA foundation: unlock/setup/shell in a
real browser against the axum host) are DONE. The oracle baseline is
`a7b1398d`. Next per the decomposition: P4.4 unit 2, then the P4.6 first
Salon vertical (M4).
