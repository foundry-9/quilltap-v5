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
  **SPIKE OUTCOME (P4.6x, 2026-07-12): RED for the Document Mode markdown
  editor.** Empirically, the sanctioned vanilla scope (`lexical` +
  `@lexical/rich-text` + `@lexical/markdown`, 0.47) round-trips headings +
  inline text-formats losslessly but **throws outright** on any document
  containing a list, code fence, or table (`ListItemNode`/`CodeNode`/table
  nodes are not in those three packages — they need `@lexical/list` /
  `@lexical/code` / `@lexical/table`). Worse, v4 does **not** use the default
  `TRANSFORMERS`: its `MarkdownBridgePlugin` carries
  `preserveAsterisks`/`preserveUnderscores`/`preserveBackticks`/`preserveTildes`
  precisely because naive Lexical markdown round-trips are LOSSY on emphasis
  and escaping. A markdown-document editor that throws on lists/code or
  mangles emphasis would corrupt real files on save — a non-lossy port needs
  the full node set + v4's whole preservation bridge, i.e. the "half-port a
  second editor / framework fight" the gate rules out. **Decision: Document
  Mode ships the byte-exact `<textarea>` for markdown files too**
  (`USES_RICH_MARKDOWN_EDITOR = false`; the spike-gated Lexical deps were
  removed). ProseMirror stays the NAMED next-round decision for a rich
  markdown/chat-composer editor; the D17 chat-composer spike (Lexical's
  second, separate consumer) is untouched by this outcome and remains open.
- **D18 — Settled component choices carry over.** File manager: **ngx-explorer
  spike, build-our-own fallback** (`scriptorium-file-manager.md`).
  **SPIKE OUTCOME (P4.6aa, 2026-07-13): ngx-explorer 5.0.2 ran GREEN on
  all three gating checks** — renders under Angular 21 zoneless (OnPush +
  AsyncPipe, no zone.js), standalone-from-standalone interop (5.0.2 is
  `isStandalone: true`; the "NgModule-based" note was stale), and a mock
  `IDataService` drove a live listing + `createDir` — **but ADOPTION was
  REJECTED → the bespoke fallback shipped.** Decisive: `IDataService` has
  NO move/copy verb (a tier-1 must-land; its drag handlers are
  upload-only), and adopting it would reintroduce a second theming engine
  (`.nxe-*` + icon font — the exact svar-theme-bridge cost this decision
  set out to escape), a numeric-id↔path map, and a per-directory listing
  model at odds with the whole-mount `mountFilesList` envelope. The
  bespoke `qt-file-manager` (`apps/web/src/app/files/**`) ships over the
  ported v4 SVAR adapter helpers, native `qt-*` styling, path-native ids;
  ngx-explorer was uninstalled. Full record: the P4.6aa order header.
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
fresh-instance provisioning), P4.4 unit 2 (chat creation + the Green Room
— the seven leaf sub-units plus the P4.4u2b `handleCreate` spine,
`Request::ChatCreate` + `ChatCreateDriver`, the capstone tier-3
differential, and the web e2e incl. the Green-Room SSE replay), and P4.5
(the SPA foundation: unlock/setup/shell in a real browser against the
axum host), and **P4.6 (the first Salon vertical — milestone M4)** are
DONE. P4.6a landed the Salon dispatch surface (`api::salon`: the enriched
`listChats`, `chatGet` [minus the locked `renderedHtml` divergence],
`chatSettings`, the turn action, message edit/delete/swipe-switch, the
Salon-minimal chat PUT, the three impersonation verbs, the extended
`chatSend` gate) with two differentials against v4's real route handlers
plus the committed Salon web fixture; P4.6b landed the Angular Salon
(`/salon` list + `/salon/:id` conversation, the byte-for-byte TS port of
v4's markdown/roleplay/qtap-linkify renderer, streaming send over the
P4.5 reducer, tier-1 message actions); the unification wired and ran the
LIVE M4 Playwright walk (unlock → list → open baked history → send → a
streamed mock-LLM reply that survives reload). **The P4.6c ∥ P4.6d ∥
P4.6e round is also DONE and unified (2026-07-10):** P4.6c closed every
carried Salon follow-up (skipUserTurn differential, swipe-generate through
a `SwipeGenerateDriver` host seam, the pendingToolResults orchestrator
case, the full `processChatUpdates` chat bag, GET attachments) plus the
SPA tier-2 controls (the skip-signal TS port + Skip banner, Speaking-As,
pause/resume + nudge); P4.6d landed the Settings dispatch backfill
(`api::settings` — chat-settings GET default-injection + PUT, connection
profiles CRUD/enrichment/reorder, API keys with the masked projection, the
providers listing off the manifest Registry, models read/fetch, the wire
actions over injected seams), each family differential-verified vs v4's
real route handlers; P4.6e built the Settings SPA (the seven-tab shell,
the AI Providers tab, the provider setup wizard, basic Appearance with
server-persisted `themePreference`). The unification wired both named
seams live (the swipe engine arm; the `api::provider_actions` live wire
over `SyncWireTransport`) and un-skipped the LIVE Settings first-run
Playwright walk (fresh instance → setup → wizard → a validated
OPENAI_COMPATIBLE profile against the mock LLM), which caught and fixed a
real PUT fidelity bug (partial nested bags must get v4's Zod-default
materialization — proven by two new settings-routes corpus cases). Next:
the remaining Salon slices (full Salon: Document Mode pane, terminal pane,
courier, images) or the Memory/Images/Templates verticals, per the P4.6+
screen-family list above. The oracle baseline is `a7b1398d`. Tracked chat-creation
follow-ups: the
create-echo DTO shape (see the capstone test header) and the capstone
corpus extension (continuation create, outfit modes, the
scenario-precedence path cases, the greeting retry/reroute ladder
branches). The participants explicit-null marshaling seam is CLOSED
(2026-07-10): `ChatParticipant`'s `connectionProfileId` /
`imageProfileId` / `selectedSystemPromptId` are now the
present-keeps-null double-`Option` (the `removedAt` pattern), banked in
the `chats-tier2` corpus, and the capstone's
`strip_participant_null_seam` normalizer is dropped — the persisted
participant nulls diff byte-exact.

**The P4.6f/g/h + P4.4u3 round is UNIFIED on main (2026-07-10)** — P4.6f
slices 1–3 + P4.6g + P4.6h (finding #3b closed) + P4.4u3, full gate green
(fresh-oracle differential sweep, 847 workspace tests, 194 SPA tests, the
8-spec Playwright suite incl. the two new walks). P4.6f "slice 4"
(create/quick-create/update, wardrobe mutations, tags CRUD + delete
fan-out, stats, depiction-guidelines) landed 2026-07-11, and the final
remainder became the P4.6i ∥ P4.6j round below. Round record:
`status-log.md`. The original round plan follows.

**The P4.6i ∥ P4.6j characters-remainder round is UNIFIED on main
(2026-07-11) — orders P4.6f / P4.6g / P4.6i / P4.6j are all CLOSED.** All
eight characters `not_available` arms are live and differential-proven
(delete cascade + preview via `services::cascade_delete`, per-character
chats, the `photos::character_gallery_service` JSON legs, ST import/export
JSON via `services::sillytavern`), and the SPA detail vertical is complete
over them (the Conversations tab, the delete/cascade-preview flow, the
gallery, ST import + Export-JSON) with three live `characters-flow` e2e
beats. Unification wires: the gallery contract reconciled to the pinned
`{entries,total,hasMore}` envelope (`linkId`/`blobUrl`; the avatar picker
fixed with it). **The characters family's remaining deferrals are all
enumerated loud refusals:** ST PNG import/export + the photo multipart
upload (quilltap-web multipart/binary routes), `photo-save-fileid` (host
file-store bytes seam), the tier-3 LLM services (ai-wizard / optimizer /
rename / ai-import), reset-builtins, refresh-archive, and the deferred SPA
verticals (the wardrobe dialog, rename/replace). **Next candidates:** the
remaining Salon slices (Document Mode pane, terminal pane, courier,
images), the Memory/Images/Templates verticals per the P4.6+ screen-family
list, the `.qtap` sample-content import, or P4.7 (`quilltap-tauri`).
Round record: `status-log.md`.

**The P4.6k ∥ P4.6l ∥ P4.6m groups+projects+multipart round is UNIFIED on
main (2026-07-11) — P4.6m is CLOSED; P4.6k/P4.6l are LANDED with
enumerated remainders.** Lane A (P4.6k) landed the groups + projects
(Prospero) dispatch surface — groups CRUD/members/mount-points, projects
CRUD/roster/chats/state/tool-settings/mount-points, project wardrobe,
background + aesthetic — proven by the `groups_routes_equivalence` (14)
and `projects_routes_equivalence` (33) differentials over the new
committed `groups-projects-{main,mount}.db` fixture. Lane B (P4.6l)
landed the Groups section + routed editor (on the Characters page), the
`/prospero` list + card-grid detail (8 cards, per-field immediate saves),
the enabled Projects nav item, the characters gallery-upload/PNG-export
riders, and the dogfood-#6 `<select [value]>` audit (3 converted, 5
proven safe). Lane C (P4.6m, COMPLETE) gave quilltap-web its multipart
machinery and closed the three byte-shaped characters deferrals: the
photo multipart upload, the fileId save leg (both storage-key modes), and
ST PNG export/import (the hand-rolled tEXt codec, tier-1 byte-exact; the
placeholder-DEFLATE and avatar-WebP-transcode seams declared).
Unification wires: the nested `group`/`project` update-bag
reconciliation (the differential-proven shape won over the SPA's flat
sends), the `.qt-page-container > *` dialog z-trap fix
(`:has(.qt-dialog-overlay)` raise), and six live e2e beats (upload, PNG
export, the four groups/projects walks). **Still refusal-armed:**
scenarios (both families + the participant-union; re-pin the scenario
body fields from v4's Zod schemas first — v4 uses
`filename`/`body`/`newFilename`, richer than the pinned sketch),
`list-files` two-branch, and the SPA Scenarios/Wardrobe cards. **Next
candidates:** the scenarios re-pin + remainder round (closing P4.6k/l),
the remaining Salon slices (Document Mode pane, terminal pane, courier,
images), the Memory/Images/Templates verticals, the `.qtap`
sample-content import, or P4.7 (`quilltap-tauri`). Round record:
`status-log.md`.

**The P4.6n ∥ P4.6o ∥ P4.4u4 scenarios+import round is UNIFIED on main
(2026-07-11) — P4.6n / P4.6o / P4.4u4 are all CLOSED, and they close
P4.6k, P4.6l, and P4.4u3's family-3 deferral.** Lane A (P4.6n) made the
whole scenarios surface live — the `scenarios-common` service port, all
13 group/project arms + the participant-union, the general
(instance-wide) family (6 new variants over the "Quilltap General"
mount, with the unprovisioned race arms), and the project `list-files`
two-branch + file add/remove — proven by `scenarios_routes_equivalence`
(41) plus the extended groups (14) / projects (39) differentials over
the extended fixture. **No refusal arms remain in the
groups/projects/scenarios surface.** Lane B (P4.6o) landed the
scope-agnostic ScenariosManager family (project Scenarios card + the
general `/scenarios` page + the nav item) and the Wardrobe card +
ProjectWardrobeManager. Lane C (P4.4u4) landed the quilltap-import
seed subset (`.qtap` is plain JSON; characters + wardrobe +
scenario-migration + memories, `skip`; loud typed refusals outside the
subset), the startup seed wire (zero-characters gate, both avatars),
and `reset_builtins`. Unification wires: the A↔B contract diffed clean
(zero drift — a first), reset-builtins dispatched at the WEB EDGE
(`?action=reset-builtins`, codec lives at the edge per the P4.6m
precedent), and `seed_sample_content` default-ON (v4 parity — a fresh
v5 boot now lands Lorian + Riya + 42 memories; the setup contract test
asserts it). **Next candidates:** the New-Chat form SPA vertical (the
scenario pickers' primary consumer — no `/salon/new` route yet; named
in P4.6o's deferrals), the remaining Salon slices (Document Mode pane,
terminal pane, courier, images), the Memory/Images/Templates
verticals, or P4.7 (`quilltap-tauri`). Round record: `status-log.md`.

**The round as planned (2026-07-10): four parallel lanes, orders
written** (drift check at planning time: v4 HEAD still `a7b1398d`; four
fresh surveys — the characters API + UI, v4's long-chat rendering, the
first-boot seeds — inform the orders):

- **Lane A — P4.6f, the Characters server surface**
  (`work-orders/p4.6f-characters-server.md`): the characters-family
  dispatch backfill (list DTO / detail + read actions / create / update /
  cascade delete / thin action verbs / prompts-scenarios-plugin-data-
  wardrobe sub-resources / tags CRUD incl. the delete fan-out; tier 2:
  stats, per-character chats, the photo gallery service, ST
  import/export, depiction-guidelines) + the committed characters web
  fixture. The repo layer is already fully ported — this is handler
  assembly with jest real-DB differentials vs v4's real handlers.
  Tier-3 deferrals: the four LLM services (wizard / optimizer / rename /
  ai-import), reset-builtins, refresh-archive.
- **Lane B — P4.6g, the Characters SPA**
  (`work-orders/p4.6g-characters-spa.md`): `/characters` list +
  `/characters/:id` view (9 tabs, per-field autosave Default Settings) +
  edit (form-with-save, the four vantage points, system-prompts editor)
  + the plain create page, over the pinned Shared contract (mocked until
  unification; live e2e at unification over lane A's fixture). The
  wardrobe dialog (~5k lines) and the AI wizards are deferred verticals.
- **Lane C — P4.6h, Salon virtualization**
  (`work-orders/p4.6h-salon-virtualization.md`): closes dogfood finding
  #3b by porting v4's OWN architecture (`@tanstack/react-virtual` →
  the Angular adapter, estimate 150 / overscan 5 / dynamic measurement,
  NO pagination) + the `useAutoScroll` semantics (100px stick threshold,
  400ms settle, multi-strategy scroll-to-bottom, completion-gated
  auto-scroll, jump button) + memoized client-side markdown (the locked
  divergence stands — windowing bounds the render cost) + a separate
  committed long-chat fixture + the scroll e2e beat.
- **Lane D — P4.4u3, the built-in seeds**
  (`work-orders/p4.4u3-builtin-seeds.md`): the Standard/Quilltap-RP
  built-in roleplay templates (closing the deferred `delimiters`
  discriminated-union marshaling; v4 seeds update-in-place on EVERY
  startup) + the three built-in mount stores (settings-pointer
  idempotent provision-or-adopt, verbatim row shapes, subfolder
  scaffolds) wired into fresh provisioning AND every assembly. The
  sample-content import (`lorian-and-riya.qtap`, ~2,500-line import
  service) stays deferred as its own future order.

Contention notes: lane A owns `api/**` + the web crate; lane D owns
`provisioning.rs` + `host.rs` + `roleplay_templates.rs`; lane B owns
`app.routes.ts` + shell nav + `core-contract.ts`; lane C owns `chat/**`
+ `screens/salon/**`; the salon fixture pair + `build-salon-fixture.ts`
are FROZEN (lane C builds a separate long-chat fixture; lane A a
separate characters fixture). `db/mod.rs` / `services/mod.rs` /
CHANGELOG / CLAUDE.md are union-resolved at unification per
`[[parallel-round-reconciliation]]`.

**The round as planned (2026-07-11): three parallel lanes, orders
written** (drift check at planning time: v4 HEAD still `a7b1398d`; three
fresh surveys — the scenario routes + Zod schemas, the ScenariosManager/
ProjectWardrobeManager UI, the quilltap-import pipeline — inform the
orders; the scenario contract re-pin was done AT PLANNING TIME so both
scenario lanes run unblocked):

- **Lane A — P4.6n, the scenarios server remainder**
  (`work-orders/p4.6n-scenarios-server.md`): closes P4.6k — the
  `scenarios-common` list/read/write service port (the
  `resolveScenarioBody` slice is already in `db/scenarios.rs`), the 13
  refusal-armed group/project scenario arms + the participant-union
  made live, the **general (instance-wide) scenarios family the P4.6k
  round missed** (6 net-new variants over the "Quilltap General"
  mount), and the project `list-files` two-branch + file add/remove.
  The re-pin: the create bag is `{filename, name?, description?,
  isDefault?, body}` (byte-identical Zod schemas across all three
  families), update drops `filename`, rename is `{newFilename}` — the
  opaque `scenario` bag variants survive unchanged.
- **Lane B — P4.6o, the Scenarios + Wardrobe SPA remainder**
  (`work-orders/p4.6o-scenarios-wardrobe-spa.md`): closes P4.6l — the
  scope-agnostic ScenariosManager family (project card + the general
  `/scenarios` page behind the disabled nav item) and the Wardrobe
  card + ProjectWardrobeManager (self-contained 360-ln inline form; no
  wardrobe-control dialog needed). The New-Chat form (753+833 ln, the
  scenario pickers' primary consumer; no `/salon/new` route exists
  yet) is the named NEXT SPA vertical, deliberately not a rider.
- **Lane C — P4.4u4, the sample-content import**
  (`work-orders/p4.4u4-sample-content-import.md`): closes P4.4u3's
  family-3 deferral — the quilltap-import SEED SUBSET (`.qtap` is
  plain JSON, not an archive: legacy monolithic format only;
  characters + wardrobe + the legacy scenario→scenarios migration +
  memories, `conflictStrategy:'skip'`), the startup wire
  (zero-characters gate, per-file swallow, avatar seeding from
  `Lorian.webp`), and tier-2 `reset_builtins` as a service (its
  dispatch arm is a unification wire — lane A owns `api/types.rs`).
  Everything outside the subset refuses loudly, including payloads
  with unsupported entity kinds (a recorded deliberate divergence).

Contention notes: lane A owns `api/**` (ALL variant edits incl. the
general family) + `db/scenarios.rs` + the groups-projects fixture
family; lane B owns `apps/web/**`; lane C owns
`services/quilltap_import*` + the host seeding site +
`assets/first-startup/**` and touches NO `api/**`. Lanes A and C both
bump core + harness (unifier accumulates); lane B alone bumps the SPA.
`services/mod.rs` / `db/mod.rs` / `api/mod.rs` / CHANGELOG /
status-log are union-resolved at unification per
`[[parallel-round-reconciliation]]`.

**The P4.6p ∥ P4.6q ∥ P4.6r listing-surfaces + New-Chat round is
UNIFIED on main (2026-07-12) — all three orders CLOSED, closing the
three P4.6l listing-surface picker gaps.** Lane A (P4.6p) made the
three global listing/CRUD surfaces live — roleplay templates (5
variants + the tier-1 `generateRenderingPatterns`), image profiles
(5 variants + `imageProviderList`), global mount points (5 variants +
capabilities + the delete cascade) — proven by four new differentials
(25/21/18/13 cases) over the extended groups-projects fixture;
ErrorKind gained Forbidden(403) + Conflict(409). Lane B (P4.6q)
landed the whole New-Chat vertical: `/salon/new`, the two-pane
picker, in-place Play-As, the four-source scenario dropdown, the
verbatim create payload, and the Green Room dialog over the global
event stream — with the live `new-chat-flow` e2e walk. Lane C (P4.6r)
populated the Templates & Prompts and Images settings tabs (managers
+ full delimiter editor), enabled the three default-* pickers, and
enabled reset-builtins. Unification reconciled the diverged B↔C
contract appendix to lane B's union fold. **Still refusal-armed:**
`imageProfileGenerate`/`ValidateKey`/`ListModels`; the mount-point
action verbs have no variants (D7, the Scriptorium surface). **Next
candidates:** the remaining Salon slices (Document Mode pane,
terminal pane, courier, images), the Memory (Commonplace Book)
vertical, the Scriptorium/file-manager vertical (mount-point verbs +
ngx-explorer spike), autonomous-rooms settings (unblocks the New-Chat
autonomous toggle), or P4.7 (`quilltap-tauri`). Round record:
`status-log.md`.

**The round as planned (2026-07-12): three parallel lanes, orders
written** (drift check at planning time: v4 HEAD still `a7b1398d`;
three fresh surveys — the v4 New-Chat form UI, the v4
roleplay-templates/image-profiles/mount-points route surfaces, and the
v5 current state — inform the orders; key survey findings: the server
`ChatCreate` + Green Room are FULLY live so the New-Chat vertical is
SPA-only, the D16 server-side markdown renderer needs NO core port
[v4 renders at GET-time; v5's locked divergence renders client-side —
the seam resolved to omission], and all four repo layers for the three
listing families are already ported, so lane A is handler assembly):

- **Lane A — P4.6p, the listing-surfaces server round**
  (`work-orders/p4.6p-listing-surfaces-server.md`): the three global
  listing/CRUD surfaces the P4.6l round enumerated as unported —
  roleplay templates (5 variants + the tier-1
  `generateRenderingPatterns` pure port), image profiles (5 variants +
  the registry-backed `imageProviderList`; `generate`/`validate-key`/
  `list-models` refusal-armed, wire-seam stretch), global mount points
  (5 variants + the pure capabilities derivation + the delete
  cascade; the twelve action verbs get NO variants — D7). Extends the
  `groups-projects` fixture and regenerates every dependent oracle.
- **Lane B — P4.6q, the New-Chat SPA vertical**
  (`work-orders/p4.6q-new-chat-spa.md`): `/salon/new` (the named next
  vertical) — the `useNewChat` port, the two-pane character picker,
  in-place Play-As (the eight pinned behaviors), the four-source
  scenario dropdown with prefix tokens, the submit spine with v4's
  exact payload, and the Green Room dialog over the existing
  creation-progress events; re-pins `core-contract.ts`'s provisional
  `ChatCreateRequest` + `CreationProgressFrame`. Deferred loudly:
  autonomous mode, manual outfit composition, the continuation
  ("change of venue") entry.
- **Lane C — P4.6r, the Templates & Images settings SPA**
  (`work-orders/p4.6r-templates-images-spa.md`): populates the two
  placeholder Settings tabs (the roleplay-templates manager, the
  image-profiles card) and enables the three disabled pickers
  (project model-behavior template picker, project + character
  image-profile pickers) over lane A's variants; rider: enable the
  stale "Reset Built-in Characters" button (its web route went live
  in P4.4u4).

Contention notes: lane A owns `api/**` (ALL variant edits) + the
`db/*` read-path extensions + the groups-projects fixture family;
lanes B and C split `apps/web/**` by directory (B: routes + contract
+ `screens/new-chat/**` + salon-list rider; C: settings/prospero/
characters screens + e2e beats), with the new listing-surface
contract interfaces landing as ONE byte-identical pinned appendix
block in `core-contract.ts` in both lanes (the unifier keeps a single
copy). Lane A bumps core + harness; lanes B and C both bump the SPA
(unifier accumulates). CHANGELOG / status-log are append-only
union-merge blocks.

**The P4.6s ∥ P4.6t ∥ P4.6u Commonplace Book + terminal-pane round is
UNIFIED on main (2026-07-12) — all three orders CLOSED.** Lane A
(P4.6s) made the memories dispatch surface live — 26 variants
(list/CRUD/search/housekeeping/configs/backfill/regenerate) proven by
`memories_routes_equivalence` (41 cases, split routes+config oracles)
over the new committed `memories-{main,mount}.db` fixture; loud
refusal variants stand for `memoryGenerateEmbeddings` /
`memoryRebuildIndex` / `chatQueueMemories`. Lane B (P4.6t) built the
Commonplace Book SPA (the character Memories tab + the Settings
Memory tab) with its e2e beats activated green at unification. Lane C
(P4.6u) built the Salon terminal pane (xterm.js, WS session service,
SplitLayout scaffolding Document Mode will reuse, embed-on-expanded-
chip, pop-out route) with a live PTY e2e. The unification wired the
P4.6s embedding seam LIVE (`EngineAssembly.memory_embedding` → the
spine's `ApiEmbeddingProvider` — memoryCreate/memorySearch live in
the real server). **Still deferred (named):** extract-memories-dry-run
+ CLI memory-diff, memory-dedup, embedding-profiles management,
conversation-summaries regen, the Document Mode pane, the Lexical
editors. **Next candidates:** the Document Mode Salon slice (its
split-pane mount point now exists), the Scriptorium/file-manager
vertical (mount-point verbs; the P4.6p order's tier-3 notes hold the
verb-by-verb survey), the courier/images Salon slices,
autonomous-rooms settings, or P4.7 (`quilltap-tauri`). Round record:
`status-log.md`.

**The round as planned (2026-07-12, second round of the day): three
parallel lanes, orders written** (drift check at planning time: v4
HEAD still `a7b1398d`; four fresh surveys — the v4 memories surface,
the v4 mount-point verbs/Scriptorium, the v4 remaining Salon slices +
autonomous settings, and the v5 current state — inform the orders;
key survey findings: the memory ENGINE is fully ported with zero
dispatch variants and a placeholder SPA tab, the terminal REST +
WebSocket server surface is fully live so the terminal pane is
SPA-only, `services/housekeeping.rs` + `db/instance_settings.rs`
exist, and `lib/tools/memory-dedup.ts` is unported):

- **Lane A — P4.6s, the memories server surface**
  (`work-orders/p4.6s-memories-server.md`): the Commonplace Book
  dispatch backfill — the collection endpoint's ~20 `?action=` verbs
  + the two-code-path list, the item CRUD (incl. the
  PUT-does-not-re-embed quirk), and `chatQueueMemories` — over the
  fully-ported memory engine, with the embedding seam threaded per
  the P4.6c provider-actions precedent, a NEW committed
  `memories-{main,mount}.db` fixture, and the
  `memories_routes_equivalence` differential. No variants (loud
  deferrals): `extract-memories-dry-run` (streaming; the CLI
  memory-diff order), memory-dedup (service unported),
  embedding-profiles management, conversation-summaries.
- **Lane B — P4.6t, the Memory SPA vertical**
  (`work-orders/p4.6t-memory-spa.md`): the per-character Memories tab
  (infinite-scroll list with id-dedupe, buckets/badges card, the
  create/edit editor with a plain-textarea stand-in, delete, the
  housekeeping dialog) + the Settings Memory tab (backfill /
  housekeeping / recall / regenerate cards); owns `core-contract.ts`
  and authors the memory block; fixture-guarded e2e beats over lane
  A's new fixture.
- **Lane C — P4.6u, the Salon terminal pane**
  (`work-orders/p4.6u-salon-terminal-pane.md`): the first remaining
  Salon slice — the xterm.js surface, the WS session client (pinned
  from the Rust protocol source), TerminalPane + the split-pane
  scaffolding Document Mode reuses next, TerminalEmbed via the
  `<!-- terminalSessionId:UUID -->` marker, session picker +
  spawn/kill, pane-state via the existing chatUpdate bag, the pop-out
  route, and a LIVE in-lane `terminal-flow` e2e walk (the server side
  already exists end-to-end).

Contention notes: lane A owns `api/**` (ALL variant edits) + additive
`db/*` extensions + the new memories fixture (existing fixtures
FROZEN — no dependent-oracle regens); lanes B and C split
`apps/web/**` by directory (B: `core-contract.ts` owner +
settings/memory + the characters memories tab + `app/memory/**`; C:
`app.routes.ts` + salon/chat screens + `app/terminal/**` + the only
npm dep additions). Per the P4.6pqr lesson, every shared
core-contract block has exactly ONE named author (B: the memory
block; C: its delimited terminal appendix) — nothing is written
byte-identically twice. Lane A bumps core + harness; lanes B and C
both bump the SPA (unifier accumulates). CHANGELOG / status-log are
append-only union-merge blocks.

**The round as planned (2026-07-12, third round of the day): three
parallel lanes, orders written** (drift check at planning time: v4
HEAD still `a7b1398d`; two fresh surveys — the v4 Document Mode
surface [chat-scoped + standalone + the client] and the v4
Scriptorium/mount-index surface [the file-op verbs, the lib layer,
the SVAR wrapper] — plus a v5-side sweep inform the orders; key
survey findings: the mount-index DATA layer is fully ported but v4's
`lib/mount-index/` SERVICE layer [chunker, file-ops strategies,
store-file, read-file, scanner, reindex] has NO v5 port — the
doc-edit tools reimplemented only the database-mount subset; the
Document Mode path machinery [path resolver with `operatorOverride`,
database store, Librarian writers, the doc UI tools] IS ported, so
the DM server lane is P4.6s-class route assembly; Document Mode has
NO message marker [state = the chats `documentMode` flag + Librarian
announcements + client reload]; v4's DocumentPickerModal consumes the
mount files LISTING, which pins one lane-A variant into the SPA
contract; the standalone DM surface is a workspace TAB in v4, and v5
has no workspace system):

- **Lane A — P4.6v, the mount-index file-ops server surface**
  (`work-orders/p4.6v-mount-index-file-ops-server.md`): closes the
  standing D7 Scriptorium refusal — the `lib/mount-index/` service
  layer ported leaf-to-root (chunker tier-1; file-ops' four
  strategies + sha verify; store-file's three ingest branches;
  read-file; reindex sync-in-request + embed-async split; scanner
  tier 2) under ~20 mount-file dispatch variants + the multipart/raw
  web-edge legs, over a NEW committed `mounts-{main,mount}.db`
  fixture + a committed fs tree. Refusal-armed WITH variants:
  `mountConvert`/`mountDeconvert`; named seam deferrals: the
  `DocumentTextExtractor` production impl (pdf/docx) and the fs
  watcher. The Scriptorium SPA (D18's ngx-explorer spike) is
  deliberately NEXT round, over this lane's then-frozen surface.
- **Lane B — P4.6w, the Document Mode server surface**
  (`work-orders/p4.6w-document-mode-server.md`): the
  `operator-doc-actions.ts` core (with `STANDALONE_CHAT_ID` and the
  tier-1 pure `computeRenameTarget`/`pickUntitledDocumentPath`), the
  11 chat-scoped + 7 standalone document dispatch variants, the
  `chat_documents` repo extensions (recents + the move-sync sweeps),
  the qtap-target byte route, and the `MountRefreshScheduler` seam
  (None + loud skip in-lane; the unifier wires lane A's reindex/embed
  in — the `memory_embedding` precedent), over a NEW committed
  `documents-{main,mount}.db` fixture.
- **Lane C — P4.6x, the Document Mode SPA vertical**
  (`work-orders/p4.6x-document-mode-spa.md`): closes the P4.6u
  "Document Mode pane" deferral — the document pane in the frozen
  split scaffolding's `documentContent` slot, the `useDocumentMode`
  state store, the Document Picker modal (consuming lane A's
  `mountFilesList`), autosave + mtime-409 reload, the tool-result
  reload wiring, and **the D17 Lexical spike** (vanilla core for the
  markdown editor; red = textarea-everywhere recorded loudly,
  ProseMirror stays the named fallback decision). Deferred loudly:
  the standalone/workspace-tab surface, multi-document tabs, the
  change-tracker/gutter plugins.

Contention notes: the two server lanes split `api/**` by module (A:
`api/mount_files.rs` + `mount_points.rs`; B: `api/documents.rs` +
`db/chat_documents.rs`) and BOTH append to `api/types.rs` /
`api/engine.rs` / the web router only at the end inside their own
delimiter blocks (the unifier keeps both sides — the first round to
run two core-dispatch writers). Lane C owns `apps/web/**` wholesale
and is the single author of the whole core-contract addition (both
families). Each lane delivers its own new fixture family — no
dependent-oracle regens anywhere. `tools/doc_edit/**` is FROZEN for
everyone (no dedup refactor mid-port). A and B bump
core/web/harness; C bumps the SPA (unifier accumulates). CHANGELOG /
status-log are append-only union-merge blocks.

**The P4.6v ∥ P4.6w ∥ P4.6x Document Mode + Scriptorium-server round is
UNIFIED on main (2026-07-12) — P4.6w and P4.6x CLOSED; P4.6v stays OPEN
with a partial landing.** Lane B (P4.6w) made the whole Document Mode
server surface live: the `operator-doc-actions` core
(`quilltap-core::documents`, with `STANDALONE_CHAT_ID` and the tier-1
`computeRenameTarget`), the 11 chat-scoped + 7 standalone document
dispatch variants, the `chat_documents` recents/move-sync extensions,
and the qtap-target byte route — proven by the 16-row pure oracle +
the 24-case `documents_routes_equivalence` over the committed
`documents-{main,mount}.db` fixture. Lane C (P4.6x) delivered the
Document Mode SPA — the pane (byte-exact textarea for EVERY file type
after the **D17 Document-Mode spike came back RED**; ProseMirror is
the named next-round editor decision), the document state store, the
picker (consuming `mountFilesList`), the split integration with the
`dividerPosition` ownership move, autosave + 409 reload, tool-result
reloads — its e2e beats ACTIVATED at the gate. Lane A (P4.6v) landed
its first three units (the pure leaves incl. the chunker, the
`mounts-{main,mount}.db` + fs-tree fixture family, and the READ/LIST
keystone with `mountFilesList`/`mountFileRead`) — **units 4–9 remain
OPEN (write/ops/scan/blobs/convert + reindex/embed) and D7 is NOT yet
closed.** Because lane A closed partial, the `MountRefreshScheduler`
seam (`EngineAssembly.mount_refresh`) could NOT be wired at
unification — it stays `None` + loud skip, and wiring it is now a
named deliverable of the P4.6v remainder. **Next candidates:** finish
P4.6v (its order header enumerates the remainder; closes D7 and wires
the refresh seam), then the Scriptorium SPA (D18 ngx-explorer spike
over the frozen file-ops surface), the ProseMirror editor decision
(D17), the courier/images Salon slices, autonomous-rooms settings, or
P4.7 (`quilltap-tauri`). Round record: `status-log.md`.

**The P4.6y mount-file-ops remainder round is UNIFIED on main
(2026-07-13) — P4.6y CLOSED, and with it P4.6v CLOSED, D7 CLOSED, and
`EngineAssembly.mount_refresh` WIRED LIVE.** The single resumption
lane (no siblings — the ingest pipeline couldn't honestly split)
delivered every tier-1 AND tier-2 unit of the P4.6v remainder,
differential-proven against fresh v4 `a7b1398d` oracles: the mounts
fixture extension (TF-IDF embedding profile + extraction substrate +
pinned chunks; mount-read regenerated); the converters
(`markdown_to_text` tier-1 exact) + the refusing `DocumentTextExtractor`
seam; `storeMountFile` (all three ingest branches, incl. the
optimistic-mtime CONFLICT and the WebP-transcode seam whose refusing
default takes v4's own encode-failure fallback) + the blob routes; the
full file/folder mutation surface (four strategies, sha256 verify,
v4's dest-exists-before-same-path copy quirk pinned); PATCH
rename+description + folder-create; reindex + scoped embed +
`mountSemanticSearch` (v4's JS `||` falsy defaults pinned); the
scanner + `mountScan`; the web-edge fs raw read + the three multipart
legs; convert/deconvert refusal-armed behind v4's live capability
guards. The production `DbMountRefreshScheduler` wires the P4.6w seam
at `host.rs` (a spawned writer job — never re-enters the busy writer),
proven by the new refresh-parity differential plus a fresh
documents-routes re-run. The unification gate ran the FULL Playwright
suite (29/29 — the Document Mode beats now exercise live
chunk+refresh on write). **Standing deferrals (loud, named):** the
production pdf/docx `DocumentTextExtractor` and WebP codec,
`conversion.ts` behind the refusal-armed convert/deconvert verbs, the
chokidar-equivalent fs watcher INCLUDING the db-store-event emitter
chain, and the `quilltap docs` CLI subcommands. **Next candidates:**
the Scriptorium SPA (D18 ngx-explorer spike, now over a fully frozen
file-ops surface — the D18 wire contract is the P4.6v §Shared-contract
variant table + the P4.6y contract pins), the ProseMirror editor
decision (D17), the courier/images Salon slices, autonomous-rooms
settings, or P4.7 (`quilltap-tauri`). Round record: `status-log.md`.

**The P4.6z ∥ P4.6aa Scriptorium-SPA round is UNIFIED on main
(2026-07-13) — P4.6z and P4.6aa CLOSED, D18 DECIDED.** Lane A (P4.6z)
delivered the Scriptorium SPA vertical — `/scriptorium` (store grid +
card + the five dialogs + DirectoryPicker + scan) and
`/scriptorium/:id` (header + info cards + patterns + scan/re-chunk +
the classic FileTable with upload/expand-describe/delete) over the
frozen P4.6v/P4.6y dispatch surface, plus the round's one new server
variant: `systemBrowseDirectory` (the DirectoryPicker's browse route),
differential-proven against v4's real route over the committed
`browse-fs-tree/` fixture. Lane B (P4.6aa) settled **D18**: the
ngx-explorer 5.0.2 spike ran GREEN on all three gating checks but
adoption was REJECTED (no move/copy verb in `IDataService`; a second
theming engine; numeric-id/per-directory model mismatch) — the bespoke
`qt-file-manager` shipped over the ported v4 SVAR adapter helpers
(node-id / listing-to-tree / the event→wire map re-targeted at
dispatch / error-translation / reindex-after-copy), plus the
dogfood-#6 `<select [value]>` audit (two risky sites converted to
`[selected]`-per-option; seven proven safe). The unification wire put
the "New file manager (beta)" toggle on the store detail
(`@defer`-loaded) and deduped `MountCapabilities` into core-contract.
Gate: 305 Rust suites 0 failed, the browse differential fresh-green,
ng test 546, full Playwright **33/33** with the file-manager walk
ACTIVE (three e2e ordering/gesture fixes recorded in the P4.6aa
header — the old spec name sorted before foundation and its unlock
broke foundation's locked start deterministically). **Standing
deferrals (loud, named):** the `/files` general-files page (the
files-family server surface — `/api/v1/files`, FileBrowser/FilePreview
— is unported; the nav item stays disabled), the workspace-tab drill,
cross-mount move/copy UI, drag-and-drop relocation (clipboard
paste shipped). **v4 drift note:** v4 HEAD moved one commit past the
baseline during the round (`6a8a77aa` — "nudge is now a persisted Host
announcement, not a client-only note"); every file this round touches
is byte-identical at both commits (verified in-lane, human-approved),
but the nudge path IS a ported unit — classify and re-port it (a
p4.d-style drift order) when the next round rebases the oracle
baseline. **Next candidates:** the v4-drift re-port (the nudge Host
announcement), the ProseMirror editor decision (D17), the
courier/images Salon slices, the files-family server surface (unlocks
`/files` + FilePreview), autonomous-rooms settings, or P4.7
(`quilltap-tauri`). Round record: `status-log.md`.
