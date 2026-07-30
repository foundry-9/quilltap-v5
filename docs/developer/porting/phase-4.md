1. ~~**`p4.d23` — the restore file-replay dedupe.**~~ **DONE — CLOSED
   2026-07-26.** The ruled skip check is on main; `system_restore_state` runs 8
   cases over two new committed archives (`restore-archive-uploads.zip`,
   `restore-archive-gen2.zip`, both built by v4's real `createBackup`), and the
   divergence list grew by one named entry (`REPLAY_DEDUPE`) exactly as
   predicted. `PHASE_ORDER_RESIDUAL` was re-examined and **stays** — the check
   removes the two hazards, not the ordering, and a legacy disk-key file is still
   re-ingested on both sides. Two of the order's premises were disproved by
   running them (the id space, and v5's predicted `UNIQUE(fileId)` hazard — v5
   upserts, so its real cost was a duplicate LINK accumulating per restore
   generation). Lane record: `status-log.md`. **Two items outstanding:** the e2e
   restore beat (`zzz-restore-destructive.spec.ts`), which should ride the next
   round that already obliges a full Playwright run; and reporting the
   measurement to the v4 side, where this repair is currently marked out of
   scope.

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
   **⚠ UNFINISHED (noted 2026-07-22) — the one Phase-4 deliverable still
   half-built.** The Dockerfile dates from P4.2, *before* the SPA existed, and
   was never revisited: it copies no `assets/` (so the three P4.4u4
   `include_bytes!`/`include_str!` seed assets fail to compile), builds no
   Angular dist (`.dockerignore` excludes `apps` wholesale), and its
   ENTRYPOINT passes no `--spa-dir` — which is the *only* way to reach a dist
   (`quilltap-web/src/main.rs:47,87`; no env or binary-relative fallback), so
   the image serves the embedded placeholder pages. Every piece works
   independently — the Playwright suite runs the real binary over a real
   `ng build` dist via `--spa-dir` — they have just never been assembled.
   **The close-out is `work-orders/p4.10-dockerfile-spa-packaging.md`**
   (a single lane; it does not touch the D21 release deferral).

## Locked decisions

The four `api-boundary.md` invariants are re-affirmed and not restated (one
boundary; streaming only on `Event`; the `Db` ownership model; enclave
`step()` + injected driver). New locks, D1–D24:

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
  **D17 DECIDED (P4.6ag, 2026-07-14): ProseMirror ADOPTED — gate GREEN.**
  The committed byte-round-trip gate
  (`apps/web/src/app/editor/markdown-round-trip.spec.ts`, 28 corpus
  entries each traced to a v4 transformer/preserve flag) ran GREEN over a
  v4-dialect bridge (`markdown-dialect.ts`: em→`_`, the ported
  `stripMarkdownEscapes` + bracket strip, a single-`*`-literal markdown-it
  rule, softbreak→`\n`, checklists). The bespoke `qt-rich-editor` shipped
  and is adopted in BOTH sanctioned surfaces — the Document Mode pane
  (markdown files only, frontmatter split + raw-source toggle preserved;
  `USES_RICH_MARKDOWN_EDITOR = true`, absorb-once specced) and the chat
  composer (send-reads-handle, `ComposerSend` unchanged, roleplay-literal
  `*` preserved) — plus input rules + formatting commands and live e2e
  dialect-bytes beats. Deferred loud: inline emphasis-on-type rules,
  tables, strikethrough/highlight, the form-field consumers,
  TextReplacementPlugin, draft persistence.
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

### The schema exception (2026-07-16)

- **D23 — When v4's schema moves, v5 adopts the columns; the migration
  runner stays deferred.** CLAUDE.md's "the schema does not change during
  the port" rule assumed a *stationary* v4. It stopped being true at v4
  `61ec90bd` (4.8.0), which added two SQLite-only ALTERs —
  `chat_messages.pascalMeta TEXT DEFAULT NULL` and
  `chat_settings.customTools INTEGER DEFAULT 1`. **Human ruling
  (2026-07-16): re-dump `provisioning/fresh_schema.json` from v4 HEAD's
  live `generateDDL` and carry the new columns; do NOT port the migration
  runner** (it remains the tracked deferral recorded at
  `harness/oracle/provision/dump-fresh-schema.ts:14-21`). This is the
  cheap path and it matches how `dump-fresh-schema.ts` already treats
  `generateDDL` as the authority — the provisioning differential
  (`provisioning_equivalence.rs:8-11`) diffs v5's `sqlite_master` against
  v4's **live** schema precisely so v4 drift trips it, which is what
  happened here. **The accepted asymmetry, stated once so nobody
  rediscovers it:** a v5-provisioned DB carries two columns a pre-4.8.0 v4
  does not know (harmless — v4's repos are column-name-addressed, proven
  by every tier-2 differential), and a v4 instance **older than 4.8.0**
  opened by v5 will *lack* them, because the migration that would have
  supplied them is exactly what v5 does not run. Closing that second half
  is the migration runner's job, whenever it is picked up.
  **The rule this replaces is "v5 never changes the schema"; the rule
  going forward is "v5 never changes the schema *unilaterally* — it
  follows v4's, and only ever via a re-dump."** First applied by
  `p4.6ay-pascal-custom-tools-server.md`.

  **⚠ Amended 2026-07-17 by the unification that landed it — consequence
  #2 is SHARPER than written above.** v5 opening a pre-4.8.0 v4 instance
  does **not** merely lack the columns: it **cannot read or write
  messages at all** (`no such column: pascalMeta`, from both
  `insert_message` and the read SELECT). `tolerant_select_list` has a
  single caller (`chat_settings`), so only that half degrades gracefully.
  **A dogfood copy must be migrated to v4 4.8.0's two ALTERs before v5
  opens it.** Extending tolerance to the message paths would need its own
  ruling and differential. Second lesson from the same landing: **a column
  adoption is not done when the schema/repo/route spine has it** —
  enumerate every *reader* of the bag. Unit 10's list said nine sites; the
  tenth (`help_settings`'s independent projection) was caught only by a
  sibling lane's differential at unification.

### The v4-validator ruling (2026-07-17)

- **D24 — v4's tool validators discard the parse; the fix goes into v4
  first, and v5 ports the fixed behavior.** Every v4 tool validator is a
  boolean type guard (`input is X`) that throws away `safeParse`'s parsed
  data; the handler then destructures the ORIGINAL input. So v4's new
  `llmNumber` (`61ec90bd`) only ever flips REJECT→ACCEPT — the handler
  still reads the raw string, and JS coercion decides each site: right by
  accident (`Math.min("50",500)`), a string in the output
  (`rollCount:"3"`), or garbage (`{"type":6,"modifier":"2"}` →
  `total:"42"`). **Human ruling: fix `quilltap-server` (return the parsed
  data — the fix `llmNumber` was written to enable), then re-drift-check
  and port the fixed behavior.** Rejected: *broken-but-exact* (the house
  precedent, but it ports `total:"42"` at real cost and re-drifts the
  moment v4 is fixed) and *deviate + document* (weakens the differential
  as proof for the whole tool family). **This blocks P4.d5 units 2–5 and
  is why they are not on main.** Discovered by the P4.d5 lane driving v4's
  real code against its own order's false premise — the byte-consumption
  assertion caught it.
  **RESOLVED 2026-07-17: the fix landed in v4 as `e3593f75`
  ("fix: tool validators return the parse, so the leniency actually
  lands", 4.8.0-dev.62).** All 57 validators return `XInput | null`; all
  29 consuming call sites read the parse; the doc-edit dispatcher routes
  its 26 cases through the validators with a raw-input fallback on a
  failed parse; the external plugin interface stays boolean; the
  published tool-definition bytes are unchanged. v4's full suite green
  (8289 unit + 135 integration), plus three new handler-level pins in
  `rng-tool-lenient-numbers.test.ts`. **The resumed round's baseline is
  `e3593f75`; P4.d5 (resume at unit 2) and P4.6ay (resume at unit 1) are
  UNBLOCKED — their orders carry matching 2026-07-17 addenda.**
  **CLOSED OUT 2026-07-17 at the P4.d5 ∥ P4.6ay unification: the re-port
  landed in full.** P4.d5 is CLOSED (all five units + §2 on main; the
  ruling's contested arms all verified against fixed v4 — `{"type":"6"}`
  rolls a real d6 and consumes a byte, `{"modifier":"2"}` adds
  numerically); P4.6ay carries the remaining Pascal units (2, 4–9 —
  resume at unit 2). The `run_custom` catalogue entry is on main and
  verified INERT until the Pascal handler lands.

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
| **Misc system/UI routes** | `ui/search`, `deployment`, `startup-status`, `migration-warnings`, `browse-directory` (ported P4.6z), `data-dir`, `home` (ported P4.6au), `image-aesthetics` (ported P4.6ar), `search-replace` | Mostly thin composition over ported repos (`replace_in_messages`/`replace_in_memories` exist); port as dispatch handlers with spot differentials where logic is real. |
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
- **P4.10 — the dev-grade packaging close-out** (added 2026-07-22, extends
  the decomposition past P4.8's M6 review). The Dockerfile builds and serves
  the real SPA, `quilltap-web` resolves its dist without `--spa-dir`, the
  `quilltap` CLI ships inside the image (D12), and one doc says how to run
  all three modes. Order:
  `work-orders/p4.10-dockerfile-spa-packaging.md`. **Retires milestone M3's
  outstanding half** — M3 was recorded demoable at P4.2/P4.5 on the strength
  of the pieces, but no image has ever served the SPA. Release/signing/
  updater stay deferred (D21) and this order does not touch them.
- **P4.11 — the non-streaming request builders: CLOSED (unified on main
  2026-07-23).** Dogfood finding #23 FIXED: every request builder honours
  `RequestInput.stream` (v4's `sendMessage` body byte-for-byte per provider —
  Anthropic/OpenAI-compatible OMIT the key, Google switches only its URL,
  OpenRouter builds a whole different body via `@openrouter/sdk` semantics,
  incl. the new `BuildError::ProviderRefused` where v4's SDK refuses
  client-side). The blind spot that let it survive is closed: the
  request-envelope corpus records BOTH modes for all EIGHT providers
  (34 → 93 lines + google-wire 5 → 10, coverage-asserted, streaming half
  byte-identical), plus a call-site regression test on the bytes
  `execute_completion` hands the transport. The unit-9 live quartet on the
  Friday copy is P4.6bj's and P4.d12–d15's owed live proof — the cheap-LLM
  family (memory extraction, titles, distill, …) runs in production. Left
  open, recorded loud in the lane record: the OpenRouter streaming
  no-tools `callModel()` path (unported), the failed-call `llm_logs` row
  (deliberate-divergence candidate awaiting a human ruling — v4 logs no
  failures, v5 matches), `chat_messages.debugMemoryLogs` (no v5 writer),
  the unpinned extraction cadence, and the no-console-logging question.
  Order: `work-orders/p4.11-non-streaming-request-builders.md`.

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
is byte-identical at both commits (verified in-lane, human-approved).
**RE-PORTED + baseline REBASED to `6a8a77aa` (2026-07-13):** writer
builders + the once-only orchestrator announcement + the SPA "invited
to speak" chip, verified by the extended post-office-host tier-1 and
the regenerated orchestrator tier-3 differentials (record in
`status-log.md`). **Next candidates:** the ProseMirror editor decision
(D17), the courier/images Salon slices, the files-family server
surface (unlocks `/files` + FilePreview), autonomous-rooms settings,
or P4.7 (`quilltap-tauri`). Round record: `status-log.md`.

**The round as planned (2026-07-13): three parallel lanes, orders
written** (drift check at planning time: v4 HEAD still `6a8a77aa`;
four fresh surveys — the v4 courier/images Salon slices, the v4
autonomous-rooms surface, the v4 files-family surface, and the D17
editor scoping — inform the orders; key survey findings: the courier
transport / photo trio / image-generation handler are all ported so
lane A is mostly variant assembly plus TWO real ports [the
`uploadChatFile` service and the `add-tool-result` handler]; the
message DTO already carries every courier/image field so lane B is
pure UI; the enclave engine + host cadence + autonomous chat-create
are fully ported with ZERO dispatch variants, so the autonomous
vertical's server half is thin marshaling and rides as one full-stack
lane):

- **Lane A — P4.6ab, the courier + chat-images server surface**
  (`work-orders/p4.6ab-courier-images-server.md`): the courier pair
  (`messageResolveExternalTurn`/`CancelExternalTurn` over the frozen
  W4.4a4 transport), `messageSaveImage` + `chatPhotoAlbums`,
  `chatAddToolResult`, the chat-files family (the `uploadChatFile`
  port over the FileBytesStore seam + multipart web edge + list +
  delete), and the `imageProfileGenerate` un-refusal — over a NEW
  committed `courier-images-{main,mount}.db` fixture.
  `validate-key`/`list-models` stay refusal-armed.
- **Lane B — P4.6ac, the courier + images Salon SPA**
  (`work-orders/p4.6ac-courier-images-salon-spa.md`): the
  CourierBubble + message-row courier branch, attachment thumbnails +
  the ImageModal lightbox, the markdown store-image rewrite
  (`blobMountPointId`), SaveImageDialog, the in-chat PhotoGallery,
  the generate dialogs, and the composer attach affordance —
  probe-guarded e2e activating at unification. Deferred loud: the
  announcement/mail/RNG gutter tools, drag-drop upload.
- **Lane C — P4.6ad, the autonomous-rooms vertical**
  (`work-orders/p4.6ad-autonomous-rooms-vertical.md`): the seven
  dispatch variants wrapping the frozen `enclave::lifecycle` + the
  `systemAutonomousRooms` listing (a NEW `autonomous-{main,mount}.db`
  fixture), then the SPA — the Settings Chat tab's two autonomous
  cards, the shared room-card editor + EditEnclaveModal, the New-Chat
  autonomous toggle with v4's exact payload mapping, and the toolbar
  run-state badges. Closes the P4.6q autonomous deferral. Deferred
  loud: the Salon in-chat Edit-Enclave entry + salon-list toggle
  (lane B territory, named for the unifier/next slice).

Contention notes: lanes A and C are the round's two core-dispatch
writers (delimited end-blocks in `types.rs`/`engine.rs`, the P4.6v/w
precedent); lane A alone touches `crates/quilltap-web`; lanes B and C
split `apps/web/**` by directory (B: chat/salon/images +
core-contract owner; C: settings/new-chat/shell/autonomous + its own
delimited contract block). A bumps core+web+harness; C bumps
core+harness+SPA; B bumps the SPA (unifier accumulates). Round-wide
HANDS OFF: the `chat_get` terminal reconcile stub-probe and the
terminal closed-chip e2e beat — a separate human effort owns them.
**Banked for the next round (surveys done 2026-07-13, recorded in the
planning session):** the files-family surface (server: the general
`/api/v1/files` listing/upload/move/promote/delete + folders +
maintenance actions, repo methods included — only the byte GETs
exist; SPA: `/files` + legacy FileBrowser + FilePreview) and the D17
ProseMirror lane (SPA-only, splits by directory; the make-or-break is
a v4-dialect byte-round-trip markdown serializer — underscore-italic,
literal `*`, escape preservation — gated by the same test that killed
Lexical; the chat-composer half needs ~6 of the 7 v4 plugins).

**The P4.6ab ∥ P4.6ac ∥ P4.6ad round is UNIFIED on main (2026-07-13) —
P4.6ac/P4.6ad CLOSED; P4.6ab tier 1 LANDED, tier 2 OPEN.** Lane A
(P4.6ab) landed the courier + chat-images dispatch surface
(`api/chat_media.rs`: the courier pair over the frozen W4.4a4
transport, save-image/photo-albums, add-tool-result, chat-files
list/delete) over the new committed `courier-images-{main,mount}.db`
fixture, proven by `courier_images_routes_equivalence` (15 checks;
survey correction: `blobMountPointId` is DEAD in v4 — no echo added).
Lane B (P4.6ac, CLOSED) landed the whole courier + images Salon SPA
(the Courier bubble + message-row branch, thumbnails + the ImageModal
lightbox, the markdown store-image rewrite, SaveImageDialog +
PhotoGalleryModal, the generate dialog, the composer attach + conflict
flow) + the courier/images e2e walk. Lane C (P4.6ad, CLOSED) landed
the full autonomous-rooms vertical (the seven dispatch verbs over the
frozen `enclave::lifecycle`, the 24-case differential over the new
`autonomous-{main,mount}.db` fixture, the Settings Chat tab's two
autonomous cards + shared editor + EditEnclaveModal, the New-Chat
toggle, the shell run-state badges, the three-beat e2e walk) — the
P4.6q autonomous deferral is CLOSED. The same unification absorbed the
two human-effort terminal branches: the terminal-flow count-baseline
spec fix and the live `TerminalLivenessProbe` wire
(`EngineAssembly::terminal_probe` — the P4.2-era chat-GET stub-probe
deferral CLOSED; the walk grew kill→re-attach→exit beats).
Unification wires: `EngineAssembly.courier_resolve` +
`save_image_bytes` LIVE in the host (ChatSpine thread-bridge driver +
`ProductionFileBytes`); the `imageProfileGenerate` params reconciled
to the Shared-contract shape (still refusal-armed); lane B's e2e beats
activated by seeding the courier fixture chats into the shared e2e
instance (global-setup, CLI JSON row copy + the meta-sidecar blob
bytes). **Still OPEN (P4.6ab tier 2, loud refusals with recipes):**
the chat-file multipart upload leg (the SPA composer attach degrades
inline until it lands) and the `imageProfileGenerate` un-refusal;
`validate-key`/`list-models` stay named refusals. **Next candidates:**
the P4.6ab tier-2 remainder, the files-family server surface (survey
banked above), the D17 ProseMirror lane (survey banked above), the
Salon in-chat Edit-Enclave entry + salon-list autonomous toggle, or
P4.7 (`quilltap-tauri`). Round record: `status-log.md`.

**The next round as planned (2026-07-13): three parallel lanes, orders
written** (drift check at planning time: v4 HEAD still `6a8a77aa`;
four fresh surveys — the v4 files-family routes/lib/SPA, the P4.6ab
tier-2 edit points, the D17 editor scope [v4's Lexical bridge + the
prosemirror-markdown configurability facts], and the Edit-Enclave/
salon-toggle entry points — inform the orders; key survey findings:
the `imageProfileGenerate` un-refusal needs a NEW
`EngineAssembly.image_generation` seam [the W4.9a runner is dead in
production — `with_image_generation` has zero callers]; the chat-file
upload's SPA client is already LOCKED [`chat-files.api.ts`] so the
server matches it; the Lexical spike test was never committed, so the
ProseMirror gate test must be rebuilt from the dialect spec and
committed this time; the salon riders are pure UI — the
`includeAutonomous` filter and the Edit-Enclave modal are both fully
ported already):

- **Lane A — P4.6ae, the files-family server surface + the P4.6ab
  tier-2 close-out** (`work-orders/p4.6ae-files-family-server.md`):
  the general `/api/v1/files` dispatch surface (list/upload/move/
  promote/delete-with-associations + folders CRUD + the maintenance
  verbs; `filesSync` refusal-armed — reconciliation unported) over a
  NEW committed `files-{main,mount}.db` fixture, PLUS the
  `uploadChatFile` port + web-edge multipart leg and the
  `imageProfileGenerate` un-refusal over the new host-wired
  image-generation seam. This round's ONLY `crates/**` writer.
- **Lane B — P4.6af, the general Files SPA + the salon autonomous
  riders** (`work-orders/p4.6af-files-family-spa.md`): the `/files`
  screen (legacy FileBrowser + FilePreview family + the dialogs,
  shell nav wired; NO upload button — v4 parity), plus the
  `chatType`-gated Edit-Enclave header entry over the existing modal
  and the salon-list include-autonomous toggle + hidden-rooms hint
  (closing the P4.6ad deferral). Owns `core-contract.ts`.
- **Lane C — P4.6ag, the D17 ProseMirror decision lane**
  (`work-orders/p4.6ag-prosemirror-editor.md`): SPA-only, the D18
  mold — the committed byte-round-trip gate spec FIRST (v4's
  composer dialect: underscore-italic, literal `*`, escape
  preservation), then (only if GREEN) the bespoke `qt-rich-editor`
  adopted in the Document Mode pane (markdown files only, v4 parity)
  and the chat composer. Rejection with a decision record is a valid
  deliverable. Sole `package.json` dependency owner.

Contention notes: lane A is the single core-dispatch and `crates/**`
writer (no two-writer rule needed this round); lanes B and C split
`apps/web` by file — B: `screens/**` + `conversation-header.ts` +
contract; C: `editor/**` + `chat-composer.ts` + `documents/**` +
deps. Round-wide HANDS OFF: `chat/render/**` (unified 11.0.5) and
`apps/web/src/app/files/**` (the Scriptorium file manager — the
general browser lives in `screens/files/`). A bumps
core+web+host+harness; B and C each bump the SPA (unifier
accumulates). Deliberately left out of the round: P4.7 (Tauri — its
own round) and feature 5.1 (native embeddings — post-parity by its
own header, `docs/features/5.1-native-embeddings-and-chunked-
retrieval.md`).

**Drift amendment (2026-07-13, same day): the round is now FOUR
lanes.** After the three orders committed, v4 moved one commit to
**`dd0d9ff5`** (v4 `4.8.0-dev.52` — "DB size reduction: stale-chat
tidying, cold-tier embeddings, int8 quantization"). Classification:
BEHAVIOR drift on five ported surfaces (the embedding blob codec
[now self-describing int8/f16, magic `0xEB` — v5's raw-Float32
reader would mis-decode every blob after v4's one-way
`quantize-embeddings-v1` migration], the maintenance sweep [new
step-3 cache collapse + `caches` summary], the chat GET [an
enqueue-only cold-chunk re-embed side effect], the
conversation-chunks repo, instance settings) plus one new surface
(dataRetention setting + GET/PUT route + Settings card). The
re-port is **lane D — P4.d3**
(`work-orders/p4.d3-db-size-reduction-drift.md`), which also owns
regenerating the affected differentials (embedding_vector,
conversation_chunks/doc_mount_chunks/help_docs tier-2s,
maintenance_ops; memories_routes/salon_reads/settings_routes
verified non-diverging). All four orders re-pinned to baseline
`dd0d9ff5`; the three original lanes' surfaces are verified
untouched by the drift diff. Lanes A + D are the round's two
core-dispatch writers (delimited blocks, the P4.6v/w precedent);
lane D additionally owns `screens/settings/**` and appends
delimited data-retention blocks to lane B's contract files.
CLAUDE.md's baseline line moves to `dd0d9ff5` when P4.d3 unifies.
**Real-data caution recorded there too: back up Friday before
running v4 `4.8.0-dev.52`+ against it — quantization is one-way.**

**P4.d3 is UNIFIED on main (2026-07-14) — CLOSED; the oracle
baseline IS now `dd0d9ff5`.** Lane D ran ahead of the other three
(the codec was gating: any embedding-adjacent oracle regen at the
new baseline needed it). Landed: the header-aware quantized codec
(read legacy/int8/f16, write int8 byte-identical — JS `Math.round`
half-toward-+∞ and the NaN-propagating clamp pinned; the
encode-with-f64-scale / store-as-f32 asymmetry preserved), the
15-differential regen batch at `dd0d9ff5` (inventory correction:
`vector_indices_tier2` + `help_docs_upsert_tier2` also diverged —
both fixed), the cache collapse as maintenance step 3 + the
`caches` summary (NEW `collapse_stale_chat_caches_tier2` over the
`retention-caches` fixture family), cold-chunk re-embed on open
(NEW `cold_chunk_reembed_tier2`; the P4.6s
`enqueue_embedding_generate` seam made faithful — per-entity dedup
+ entity priorities), the dataRetention setting + dispatch pair
(settings_routes → 30 cases) and the SPA Data Retention card
(ng 622, live e2e beat; `global-setup` materializes
`instance_settings` — additive, the terminal_sessions precedent).
Deferred loud: the `EMBEDDING_GENERATE` execution handler,
`EMBEDDING_REAPPLY_PROFILE`, the backup-service leg, `db optimize`.
**Lanes A (P4.6ae) / B (P4.6af) / C (P4.6ag) remain OPEN and
unstarted — they now run against main-with-P4.d3 at `dd0d9ff5`;
lane A is the round's only remaining core-dispatch writer, so the
two-writer rule is moot unless the lanes overlap in time with a
future drift lane.** Round record: `status-log.md`.

**The P4.6ae ∥ P4.6af ∥ P4.6ag round is UNIFIED on main
(2026-07-14) — P4.6af CLOSED, P4.6ag CLOSED (D17 DECIDED:
ProseMirror ADOPTED, gate GREEN), P4.6ae OPEN (partial).** Landed:
the general files dispatch surface (nine verbs, the 25-case
`files_routes_equivalence` differential over the new committed
`files-{main,mount}.db` fixture) ∥ the `/files` SPA vertical
(legacy FileBrowser + preview + dialogs + shell nav) + the two
salon autonomous riders (Edit-Enclave header entry; the
include-autonomous toggle + hint + New-Autonomous-Room action,
live 3/3 walk) ∥ the bespoke `qt-rich-editor` (ProseMirror over
the v4-dialect bridge, 28-entry byte-round-trip gate GREEN)
adopted in the Document Mode pane AND the chat composer, with
input rules + formatting commands + live dialect-bytes e2e beats.
Unification wires: contract diffed name-for-name (no divergences);
the files e2e data beat's guard extended to cover the un-landed
upload REST leg (self-activates when P4.6ae unit 4 lands); the
terminal-flow chip baseline settled against the virtualized-list
mount race. Gate: 310 Rust suites / 1318 tests (files differential
fresh at `dd0d9ff5`), clippy both feature sets, ng test 691,
ng build clean, Playwright 45 passed + the 1 guarded files data
beat skipped. **The P4.6ae remainder (see its order header): the
P4.6ab tier-2 close-out (chatFileUpload + the
`imageProfileGenerate` un-refusal over the still-missing
`EngineAssembly.image_generation` seam), the `fileUpload` variant
+ upload REST leg, thumbnails/cleanup verbs + the chat-file link
leg, the FILE_HAS_ASSOCIATIONS itemized envelope + dissociate
arm.** Next candidates: finish P4.6ae, the editor form-field
consumers / tables (D17 tier 3), autonomous-rooms deferred cards,
or P4.7 (Tauri).

**The P4.6ah ∥ P4.6ai ∥ P4.6aj ∥ P4.d4 round ("finish P4.6ae + catch
up from v4") is UNIFIED on main (2026-07-14) — ALL FOUR orders CLOSED,
and with them P4.6ae and P4.6ab (tier 2) CLOSE. The oracle baseline
moved to `02865bdb`.** Landed: the files write + maintenance server
remainder (the chat-file upload leg + `action=link`, the general
`fileUpload` variant + `POST /api/v1/files?action=upload` REST leg,
the itemized `FILE_HAS_ASSOCIATIONS` envelope on `CoreError` + the
`dissociate` arm, `filesGenerateThumbnails`/`filesCleanupStale`/
`filesCleanupOrphans` — `files_routes_equivalence` grew 25 → 41
cases) ∥ the `imageProfileGenerate` un-refusal over the NEW
`EngineAssembly.image_generation` seam, wired LIVE in the host from
the W4.7f `Real*Provider`s (`image_generate_route_equivalence`, 4
cases) ∥ the SPA delete-associations close-out (REDUCED v4-faithful:
no v4 client sends `force` — dissociate-only; the dialog itself had
already landed in P4.6af) ∥ the `02865bdb` skip-signal drift re-port
(trailing-sentinel strip; `skip_signal_equivalence` 106 rows).
Unification wires: contract diffed name-for-name (no divergences);
the P4.6af guarded files data beat self-activated over the live
upload REST leg; a composer-attach live-leg beat added (the one
cross-lane proof neither lane could run alone). **Files-family
deferrals that remain (all loud, named):** `filesSync`,
`action=attach-mount-file` (the Librarian walk), thumbnail
*generation* (host codec; the byte-GET thumbnail route works),
cleanup-stale disk-key fs existence, `autoDescribeChatImageAttachment`
(no-op), `imageProfileValidateKey`/`ListModels` (live-provider-only).
Next candidates: the D17 tier-3 editor follow-ons (form-field
consumers, tables), the deferred autonomous-rooms cards (cron
preview + the 13 Chat-tab cards), P4.7 (Tauri), or a dogfood pass
over the now-complete files story.

**The P4.6ak ∥ P4.6al ∥ P4.6am round ("the D17 editor follow-ons +
salon dogfood round") is UNIFIED on main (2026-07-14) — ALL THREE
orders CLOSED, and with them dogfood findings #7, #8, #9 and the
standing finding-#6 select audit.** Landed: the text-replacement-rules
server surface (five verbs + REST edges + the conflict arm, the
15-case `text_replacements_routes_equivalence` differential over the
new committed `text-replacements-{main,mount}.db` fixture) +
`chatGetBackground` (all three arms) + the `regenerate-background`
loud refusal ∥ strikethrough/highlight marks + emphasis-on-type input
rules (the byte-round-trip gate grew +8), composition mode end-to-end,
the shared `qt-markdown-field` + `qt-formatting-toolbar` adopted in
the memory editor and the character edit/new fields, composer draft
persistence, and the text-replacement plugin + settings CRUD card ∥
the chained-response streaming render (finished chained/carina/host
bubbles visible as they complete), chat background display
(`--story-background-url` over the live resolver), and the last
dynamic-options `[value]` select conversion. Unification wires: the
CoreRequest union folded (contract name-for-name against `types.rs`);
the salon composition-mode + text-replacement bindings; the background
beat LIVE over a seeded story background; three new live composer
beats. Gate: 314 Rust suites / 1327 tests (both round differentials
fresh at `02865bdb`), clippy both feature sets, ng test 764, ng build
clean, full Playwright 52/52 zero skips. **Remaining in this surface
(loud, named):** the story-background generation subsystem, the
lane-B item-6 form-field adoptions (each a clean `qt-markdown-field`
swap), the GFM table transformer, the missing-host dialog consumers,
`roleplayTemplateId` toolbar awareness, `__bold__` on-type. Next
candidates: the remaining form-field adoptions as a rider on any SPA
order, the deferred autonomous-rooms cards (cron preview + the 13
Chat-tab cards), P4.7 (Tauri), or a dogfood pass over the
editor/backgrounds/files story.

**The P4.6an round is PLANNED (2026-07-14): the Chat-tab settings
cards remainder + the cron next-run preview — ONE lane**
(`work-orders/p4.6an-chat-tab-cards-cron-preview.md`), closing the
two remaining P4.6ad deferrals (the Salon Edit-Enclave entry landed
in P4.6af). Scope: the eleven still-deferred v4 Chat-tab cards
(Composer, Auto-Scroll, Token Display, Context Compression, Memory
Cascade, Image Description, Automation, Agent Mode,
Thinking / Reasoning, Answer Confirmation, Dangerous Content — of
the original "13", Composition Mode / Text Replacement / Data
Retention have since landed via P4.6al + P4.d3) mounted in v4's
exact 16-card order, plus the live `croner` next-run preview in the
shared autonomous room card. Survey-verified SPA-only: the server
Zod-parse already covers every key the cards write
(`settings_routes_equivalence` stands), no new dispatch variants, no
new fixtures; v5 already consumes `autoScrollOnResponseComplete` and
`thinkingDisplay` — the cards are the missing editors. Deliberately
NOT split (all eleven cards mount in one `chat-tab.ts` over one PUT
recipe; the cron preview rides the same tab's shared card) and
deliberately excluded: the Salon token/cost display rendering (a
Salon slice), P4.7 (Tauri), the form-field adoptions (a rider on any
SPA order). v4 baseline `02865bdb` (drift-checked at planning: no
movement). Round record: `status-log.md`.

**The P4.6an round is UNIFIED on main (2026-07-15) — P4.6an CLOSED,
and the last two P4.6ad deferrals CLOSE with it.** Landed: the eleven
remaining Chat-tab settings cards in v4's exact 16-card order over a
shared card substrate (`ChatSettingsCard` base + one shared
`['chatSettings']` query key — sixteen cards, ONE GET), each with
payload-asserting specs (whole-bag merges, the dogfood-#6
late-options regressions proven to bite); the tab placeholder
RETIRED; the live `croner@10.0.1` cron next-run preview in the shared
autonomous room card (Settings defaults + Edit-Enclave + New-Chat,
v4's exact strings; `isCronShapeValid` retired); the composer
spellcheck rider (ProseMirror `attributes` + the `setProps` nudge);
four live e2e beats. **The planned "SPA-only" held except one server
contingency, which fired as designed:** the `dangerousContentSettings`
key was covered but its PARSE was serde-struct, not Zod — explicit
`null` dropped, partial bags rejected, `1` re-emitted `1.0`; the
hand-rolled `zod_dangerous_content_settings` (the
`zod_cheap_llm_settings` mold) fixed it, `settings_routes_equivalence`
19 → 32 cases over a fresh `02865bdb` oracle, the old path re-tested
to prove the diff bites. Gate at unification: 314 Rust suites / 1327
tests / 0 failed (settings differential regenerated FRESH and run by
name, 32/32), clippy both feature sets, release build, ng test 846,
ng build clean, **full Playwright 56/56 zero skips**. Versions:
core 0.0.222, harness 0.0.201, host 0.0.17, web 0.0.21, SPA 0.5.101.
**Deferred loud from this surface:** the Salon token/cost display
rendering (a Salon slice — v4's `MessageRow`/`MessageActionBar`
consumers of `tokenDisplaySettings`). Next candidates: the Salon
token/cost display slice, the story-background generation subsystem,
the remaining form-field adoptions (a rider on any SPA order), P4.7
(`quilltap-tauri`), or a dogfood pass over the now-complete Settings
story. Round record: `status-log.md`.

**The P4.6ao ∥ P4.6ap ∥ P4.6aq round is PLANNED (2026-07-15): the
token/cost display + the background-generation subsystem + the
form-field adoptions — THREE lanes**
(`work-orders/p4.6ao-token-cost-background-server.md` ∥
`p4.6ap-token-cost-background-salon-spa.md` ∥
`p4.6aq-form-field-adoptions.md`), closing the P4.6an token/cost
deferral, the P4.6ak/P4.6am background-generation deferral, and the
P4.6al item-6 deferral. The split is server ∥ Salon-SPA ∥ forms-SPA
(not feature-verticals) because both feature verticals meet in
`api/types.rs`/`engine.rs`/`chat_media.rs` — one lane owns all of
`crates/**`. Lane A: the `chatGetCost` verb (v4's `?action=cost`,
RAW un-enveloped body) over the already-ported `chats_tokens`
aggregates, the `regenerate-background` un-refusal (edge-only — the
W4.9c job handler is ported AND registered live, the WebP transcoder
and `image_generation` seam are live), and the TITLE_UPDATE handler
(+ `considerTitleUpdate`/`considerHelpChatTitleUpdate`, unported
cheap-LLM tasks) — today the ported `context_summary` enqueues
TITLE_UPDATE jobs that die on the loud fallback, which is also why
auto-background-generation never fires; three differentials over a
new `cost-background-{main}.db` fixture family. Lane B: the
per-message token badge + compact chat-totals header summary (v4
quirks carried: `showPerMessageCost` is DEAD in v4,
`showSystemEvents` inert), the Story Backgrounds card in the Images
tab, the regenerate header entry + the 5s/36 active and 30s passive
polls. Lane C: the `minHeight` input + ten `qt-markdown-field` swap
sites (with the survey correction: the editable image-prompt fields
are `edit/appearance-tab.ts`, not the view details-tab the P4.6al
header named). v4 baseline `02865bdb` (drift-checked at planning: no
movement). Shared contracts §1–§3 pinned verbatim in all three
orders. Round record: `status-log.md`.

**The P4.6ao ∥ P4.6ap ∥ P4.6aq round is UNIFIED on main (2026-07-15) —
ALL THREE orders CLOSED, and with them the P4.6an token/cost deferral,
the P4.6ak/P4.6am background-generation deferral, and the P4.6al
item-6 form-field deferral.** Landed: the `chatGetCost` verb (RAW
un-enveloped body) + the `regenerate-background` un-refusal (edge-only;
the differential caught and fixed a latent `projectId`-omission bug in
the shared enqueue) + the TITLE_UPDATE handler (the live loud-failure
closed — `context_summary` had been enqueuing title jobs that died
unhandled, which also kept automatic background generation from
firing), three differentials over the new committed
`cost-background-{main,mount}.db` family ∥ the per-message token badge
+ the compact chat-totals header summary + the Story Backgrounds
Images-tab card + the Regenerate Background header entry with both
polls ∥ the `qt-markdown-field` `minHeight` input + eleven adoptions
across ten sites (three async-loading hosts got v4's loading-gate
structure — mount-before-content turns a load into an edit).
Unification wires: the §1/§2 request types folded into the
`CoreRequest` union; the two ACTIVATE-AT-UNIFY beats made LIVE (the
activation surfaced the e2e `image_profiles` ownership gap —
`image_profiles` joined the userId rewrite loop). Gate: fmt/clippy
both feature sets/release build clean; both oracles regenerated FRESH
at `02865bdb` and run by name (13-case routes, 10-case tier-3 +
runner-registration e2e); `cargo test --workspace` 317 suites / 1341
tests / 0 failed; ng test 968; ng build clean; full Playwright 60/60
zero skips, all four new beats LIVE. Versions: core 0.0.225, harness
0.0.204, host 0.0.18, web 0.0.22, SPA 0.5.113. **Deferred loud from
this surface:** the minHeight residual gap at the P4.6al-adopted sites
(values recorded in the P4.6aq unit-1 record — a one-line-per-site
rider), the Default Aesthetics Images-tab card, the LLM-Inspector
button, the boxed summary variant + `detailed=true`, project-page
backdrop arbitration, the no-host dialog consumers, the
`MessageCostEstimator`/`CarinaCostEstimator` consolidation. Next
candidates: P4.7 (`quilltap-tauri`), a dogfood pass over the
token/cost + backgrounds + editor story on the Friday copy, or the
small-rider pool above. Round record: `status-log.md`.

**The P4.6ar ∥ P4.6as ∥ P4.6at round is UNIFIED on main (2026-07-15) —
ALL THREE orders CLOSED, and with them the P4.6ao-round LLM-Inspector,
Default-Aesthetics, and minHeight-residual deferrals.** Landed: the
llm-logs read surface (eight repo reads, the
`llmLogsList`/`llmLogGet`/`llmLogDelete` verbs + REST edges) + the
`systemImageAestheticsGet`/`Set` pair over DRY'd `services::aesthetics`
helpers, two differentials (27-case llm-logs incl. the wire key-order
assertion, 13-case aesthetics incl. the unprovisioned-store arms) over
the new FOUR-file `inspector-{main,mount,llm,nostore-main}.db` family ∥
the whole LLM-Inspector SPA vertical (slide-over panel, entry/panel,
toolbar button + Cmd+Shift+L, per-message cpu icon, the reconcile-point
log refresh, the seeded-partition e2e walk) ∥ the shared
`aesthetic-editor-field` extraction + the Default Aesthetics Images-tab
card + the sixteen minHeight bindings. Unification wires: the §1/§2
request types folded into `CoreRequest`; `p4_6ar_wire_contract` added;
both ACTIVATE-AT-UNIFY beats LIVE (the aesthetics beat grew the reload
round-trip). **Findings banked:** v4's `?standalone=true` can never
return a row (BROKEN-BUT-EXACT, `$eq: null` → `= NULL`); the
garbage-limit NaN quirk (hand-rolled `js_min`); the item routes have no
ownership check; v4's always-mounted `role="dialog"` slide-over is a
permanent phantom modal (v5 declares the role only while open — a
documented divergence); the `db/llm_logs.rs` Phase-2 "serde_json sorts
keys" header note is STALE under `preserve_order` (doc follow-up in the
pool). Gate: fmt/clippy both feature sets (forced non-cached)/release
build clean; both oracles regenerated FRESH at `02865bdb` and run by
name; `cargo test --workspace` 320 suites / 1347 tests / 0 failed; ng
test 1107; ng build clean; full Playwright green zero skips with all
three new beats LIVE. Versions: core 0.0.228, harness 0.0.208, host
0.0.18, web 0.0.23, SPA 0.5.122. **Still deferred loud (the standing
pool):** the boxed summary variant + `detailed=true`, project-page
backdrop arbitration, the no-host dialog consumers
(CreateNPC/ComposeMail/InsertAnnouncement/AddCharacter, the
prompt-library screen), the source-mode toggle, the GFM table
transformer, `MessageCostEstimator`/`CarinaCostEstimator`
consolidation, the stale-seam-note doc sweep. Next candidates: P4.7
(`quilltap-tauri`), a dogfood pass over the Inspector + aesthetics +
token/cost story on the Friday copy, or the small-rider pool. Round
record: `status-log.md`.

**The P4.7a ∥ P4.7b round is UNIFIED on main (2026-07-16) — BOTH orders
CLOSED; P4.7 (the decomposition's last lettered step) is LANDED, with
the M5 walk staged for the human.** Landed: `crates/quilltap-tauri`
0.0.2 (tauri 2.11.5 / tauri-build 2.6.3 / wry 0.55.1) — boot through
the shared quilltap-web helpers (`resolve_instance_base_dir` +
`production_host_config` + `boot_startup_status`, extracted in web
0.0.24 so both transports run the identical recipe), §1
`dispatch`/`health` commands over the extracted
`dispatch_body`/`health_parts` (dispatch always resolves the envelope;
health returns `{status, body}`), §2 `events_attach` +
`quilltap://event`/`quilltap://resync` over `subscribe_with_backlog`
(Green-Room backlog-before-live preserved; re-attach REPLAYS the
still-active backlog), §3 the `qtap` custom protocol delegating the
full http::Request into the reused router (tower oneshot, permissive
CORS), §4 the terminal stream over paired IPC
(`terminal_attach`/`send`/`detach` + `tauri::ipc::Channel`, the frozen
WS unions verbatim, attach/send/detach semantics by reuse of the same
manager calls), and the 6-test tier-4 IPC contract suite mirroring
`contract.rs` ∥ the SPA D14 seam made real: the `CoreTransport` split
(HTTP byte-for-byte frozen), the Tauri transport
(invoke/listen/mockIPC-tested; shared `interpretHealth` cannot fork),
the §3 `apiUrl` resolver at every raw REST/byte site, the §4
`TerminalStreamTransport` seam + Tauri pipe, bootstrap selection via
`isTauri()` with the IPC modules in one lazy chunk (main bundle greps
ZERO `__TAURI_INTERNALS__`). Unification wires: the §1–§4 contract
diffed name-for-name across sides (six commands, arg keys, event
names, qtap origins — NO folds needed; lane B's specs pin the names);
the debug bundle rebuilt over a REAL `ng build`
(`target/debug/bundle/macos/Quilltap.app`); a locked walk instance
staged at `~/qt-m5-instance` (the e2e recipe: passphrase
`open sesame please`) and the app boot-smoked headless against it
(process stable, clean stderr — window content needs eyes). Gate:
fmt/clippy both feature sets/release build clean; `cargo test
--workspace` 324 suites / 1353 tests / 0 failed (`ipc_contract` 6/6 by
name); ng test 1150 (125 files); ng build clean; full Playwright
63/63 zero skips (the frozen-path proof — no locator edits).
Versions: core 0.0.228, harness 0.0.208, host 0.0.18, web 0.0.24,
tauri 0.0.2, SPA 0.5.126. **The one remaining acceptance step: the
human M5 walk** (launch `Quilltap.app --args --data-dir
~/qt-m5-instance`, unlock → salon → open chat → send → streamed
reply; the terminal pane exercises the §4 pairing). **Deferred loud:**
native niceties (menus/tray/window-state/deep links), updater/signing/
release bundles (D21 + the no-release hard stop), uniffi/mobile,
`Last-Event-ID`-style replay, the turnkey `tauri dev` loop. Next
candidates: the M5 human walk + a Tauri dogfood pass, a dogfood pass
over the Inspector + aesthetics + token/cost story on the Friday
copy, the small-rider pool, or the M6 screen-parity review. Round
record: `status-log.md`.

**The P4.6au ∥ P4.6av ∥ P4.7c round is UNIFIED on main (2026-07-16) —
ALL THREE orders CLOSED; the homepage exists end-to-end and the Tauri
shell is one-origin (dogfood finding #12's cause FIXED).** Landed: the
`systemHome` dispatch verb + `GET /api/v1/system/home` — v4's
`getHomeData` (224 lines) as `services::home` over the ported
repos/enrichment services, with `collation::locale_compare_base` (the
en-US primary-strength option v4's homepage character sort uses),
`FilesRepository::find_all`, the committed `home-{main,mount}.db`
fixture family + generator, and `home_routes_equivalence` (14 oracle
cases at `02865bdb`: 2 through v4's real route handler, 6
displayName-ladder/scoping through the real exported service, 6
raw-SQL mutation cases replayed identically both sides, + the
key-order claim and the always-on §1 wire-shape test) ∥ the Home
dashboard at `/` (`screens/home/`, replacing the redirect-to-salon
root): welcome greeting, the FIVE-action quick row (New Chat /
Autonomous Room / Continue Last / New Project — the order's survey
said four; Generate Image OMITTED, its `/generate-image` target is
unported), the recent-chats / projects / characters grid with
whole-card click per the finding-#4 pattern, the card Chat action
navigating to `/salon/new?characterId=` (documented divergence from
v4's NewChatModal), 16 sibling e2e specs' root-entries re-aimed ∥ the
P4.7c one-origin adoption: the spike ran GREEN on every gating check
(WKWebView pushState routing, localStorage persistence across
relaunch, isTauri, byte routes, devtools), so the Tauri window ships
on `qtap://localhost/` — the qtap handler serves the embedded dist
for non-API GET/HEAD and delegates `/api/*`+`/health` into the reused
router; every server-relative URL (including inside pre-rendered
bodies) now resolves; `apiUrl()` is identity on a qtap-origin page
(signature frozen); the fallback render-seam path was never taken and
NO quilltap-web edits were needed. Unification wires: the
`systemHome` fold into `CoreRequest` + the name-for-name wire diff;
the home beat ACTIVATED (2/2 by name); the SPA version union →
0.5.128. Gate: fmt/clippy both feature sets/release build clean;
`cargo test --workspace` 325 suites / 1357 tests / 0 failed (the home
differential 14/14 over a FRESH oracle); ng test 1172 (127 files); ng
build clean (zero `__TAURI_INTERNALS__`); full Playwright **65/65
zero skips**. Versions: core 0.0.230, harness 0.0.208, host 0.0.18,
web 0.0.25, quilltap-tauri 0.0.3, SPA 0.5.128. **The one remaining
acceptance step: the combined human M5 + finding-#12 walk** (recipe:
the P4.7c order header — the M5 beats on `~/qt-m5-instance`, the
image beats on the Friday copy, devtools-inspect). **Deferred loud:**
the `/generate-image` screen (M6 pool), NewChatModal-on-card (M6
parity), quick-hide filtering, Windows/Linux one-origin re-checks,
plus the standing P4.7/D21 and small-rider pools. Next candidates:
the human walk, then a homepage/Tauri dogfood pass on the Friday
copy, the small-rider pool, or the M6 screen-parity review. Round
record: `status-log.md`.

**The P4.6aw ∥ P4.6ax ∥ P4.8 riders + M6-review round is PLANNED
(2026-07-16): the small-rider pool + the M6 screen-parity review —
THREE lanes** (`work-orders/p4.6aw-rust-riders-depiction-hint.md` ∥
`p4.6ax-editor-riders.md` ∥ `p4.8-m6-screen-parity-review.md` — P4.8
extends the decomposition past P4.7 as the review step toward
milestone M6). v4 baseline `02865bdb` (drift-checked at planning: no
movement). Lane A (Rust riders + one SPA arm): the
`MessageCostEstimator`/`CarinaCostEstimator` consolidation (a
behavior-frozen refactor — the two host impls are byte-identical;
the existing title/carina tier-3 differentials are the proof), the
stale "serde_json sorts keys" comment sweep (~25 enumerated targets;
`preserve_order` made the rationale false), and the
depiction-guidelines no-vault suppression (v4's `disabledHint` arm —
v5 today fails reactively on save where v4 proactively suppresses
the editor). Lane B (editor riders): the `__bold__` on-type input
rule (parser already handles it — one `markInputRule` gap), the
form-field source-mode toggle (v4 default-ON, incl. the
`text-transforms.ts` source-branch port with a tsx tier-1 oracle),
and the GFM table transformer (LIVE in v4, lossy always-left-align
export carried byte-for-byte; recorded vectors extend the P4.6ag
byte gate). Survey findings that re-scoped the pool:
`roleplayTemplateId` toolbar awareness is NOT a rider (the v5
composer has no toolbar and the salon never fetches the chat's
template — a future Salon slice), and the boxed `ChatCostSummary`
variant is DEAD in v4 (zero callers — lane C records the WON'T-PORT
verdict). Lane C (P4.8, docs-only read-only): the M6 checklist
`docs/developer/porting/m6-screen-parity.md` — every v4
screen/dialog vs v5 with evidence-cited verdicts
(PARITY/DIVERGENCE-DOCUMENTED/MISSING/WON'T-PORT), the deferral
cross-reference (no dangling "deferred loud"), the prioritized
backlog for the remaining rounds, and the v4 retirement criteria.
Known-missing rows seeded: `/generate-image`, `/photos`, `/profile`,
`/about` (v5 has NO version surface), the tabbed workspace, the
Brahma console UI, quick-hide (tag/dangerous arms), the general
wardrobe dialog, the prompt-library + Core Whisper cards, the Data &
System tab, the no-host chat dialogs. A fold-free round: §1 pins NO
new wire surface; the one cross-lane visible change is lane B's
source-toggle default (§2). Round record: `status-log.md`.

**The P4.6aw ∥ P4.6ax ∥ P4.8 round is UNIFIED on main (2026-07-16) —
ALL THREE orders CLOSED; the M6 screen-parity checklist EXISTS
(`m6-screen-parity.md`) and the standing small-rider pool is now
EMPTY.** Landed: the cost-estimator consolidation (one trait / one
no-cost default / one host pricing impl; `CarinaCostEstimator` and
its twins retired; behavior-frozen — title + carina tier-3
differentials green over fresh `02865bdb` oracles, zero SKIP) + the
stale "serde_json sorts keys" sweep (15 files; `phase-2-onramp.md`
seam-#5 wording reconciled at unify) + the depiction-guidelines
no-vault suppression on both appearance tabs (v4's verbatim warning;
no fetch when suppressed) ∥ the `__bold__` on-type rule + the
form-field source-mode toggle (v4 default-ON; source-mode toolbar
transforms over a 32-row jsdom oracle driving v4's REAL
FormattingToolbar) + the GFM table transformer (hand-rolled
`qt_table` block rule — markdown-it's built-in rejected as wrong on
every axis; 19/20 recorded vectors byte-match v4, the 20th pins the
PRE-EXISTING block-separation dialect divergence bidirectionally;
recorder findings: `| :-: |` is NOT a table in v4, line-by-line
retry, alignment discarded on import) ∥ the M6 review: every v4
screen + screen-grade dialog verdict-ed with both-side citations,
four headline findings (F1: the tabbed workspace is v4's DEFAULT
shell — `p4.9j` needs a human ruling; F2: two distinct LLM-log
surfaces; F3: the chat-level Core Whisper override chain; F4: three
stale v5 docstrings), two WON'T-PORT verdicts rendered (boxed
ChatCostSummary + `detailed=true`; the redirect aliases +
`/foundry/*`), the 16-item `p4.9a–n` backlog (§4), and the v4
retirement criteria (§5). v4 drifted ONE docs-only commit
(`34746bed` — the Pascal-custom-tools feature SPEC; classified
benign, baseline stays `02865bdb`, but it previews a future drift
re-port when the feature lands as code). A fold-free round: §1 held
(no wire-surface change anywhere). Gate: fmt/clippy both feature
sets/release build clean; four oracles regenerated FRESH; both
committed vector files IDENTICAL to fresh runs; `cargo test
--workspace` 325 suites / 1357 / 0 failed (both tier-3s by name,
zero SKIP); ng test 128 files / 1247; ng build clean; full
Playwright green zero skips run alone (absorbing lane B's skipped
full-suite step). Versions: core 0.0.232, harness 0.0.209, host
0.0.19, web 0.0.25, quilltap-tauri 0.0.3, SPA 0.5.134. **Next
candidates: the human M5 + finding-#12 walk (STILL outstanding —
the P4.7c recipe), then the M6 backlog's items 1–4 as the natural
next round (`p4.9a-photos-view` ∥ `p4.9c-about-profile` ∥
`p4.9b-generate-image-screen` ∥ `p4.9d-quick-hide-provider` — lift
from `m6-screen-parity.md` §4), and the `p4.9j-workspace-tabs`
human ruling (§5.1).** Round record: `status-log.md`.

**The P4.d5 ∥ P4.6ay resumed-lanes unification is on main (2026-07-17)
— P4.d5 CLOSED; P4.6ay at units 1+3+10 of 10 (resume at unit 2; its
order header carries the resume instructions AND the warning about
v4's in-flight custom-tools/metadata feature).** The oracle baseline
is now `e3593f75` (CLAUDE.md's baseline bullet has the banked
`444c7fd6` disposition). **Next candidates: finish P4.6ay (units 2,
4–9 — the Pascal server surface; the long pole, and the natural next
lane), the human M5 + finding-#12 walk (STILL outstanding), the M6
backlog items 1–4, or the `p4.9j-workspace-tabs` human ruling.**
Round record: `status-log.md`.

**The d68638b4 drift-catch-up round is PARTIALLY UNIFIED on main
(2026-07-17) — P4.d7, P4.6az, and P4.6ba CLOSED; P4.6ay at units
11+2+5+6 (of its 11→2→5→6→4→8→9→7→12 order — resume at unit 4; its
status header carries the resume notes and the §4 obligations it now
owes lane BA's landed SPA).** The oracle baseline is `d68638b4`
(4.8.0-dev.72): the case-insensitive mount namespace (incl. the
Option-A `characters.metadata` column fold-in — a `generateDDL`
column despite the vault file being the app-level source of truth),
the metadata.json fact-sheet vault surface (+ the lazy backfill
wired at unification), and the Pascal in-chat SPA (popup dark +
flow beat probe-guarded until the server verbs land) are all
absorbed; the Pascal server remainder (units 4/8/9/7/12) is the one
open piece of the drift. **Next candidates: finish P4.6ay (units 4,
8, 9, 7, 12 — lights BA's popup and activates its beat; the clear
next lane), then the Workbench SPA (P4.6bb, spec'd in the round
plan), the human M5 + finding-#12 walk (STILL outstanding), the M6
backlog items 1–4, or the `p4.9j-workspace-tabs` human ruling.**
Round record: `status-log.md`.

**The P4.6ay resumed-carryout unification is on main (2026-07-17,
the second d68638b4-round unification) — units 4/8/9/7 + the
unit-12 compute half landed; `run_custom` is LIVE end-to-end and
BA's Salon custom-tools flow beat SELF-ACTIVATED over the unifier's
Tools-fixture seed (full Playwright 67/67).** P4.6ay stays OPEN on
exactly ONE item: **unit 12's route surface** (`pascal/workbench.rs`
+ `/api/v1/custom-tools` + the four workbench dispatch verbs — the
order header carries the full spec), which is also **P4.6bb's (the
Workbench SPA's) server dependency. Next candidates: the unit-12
route surface + the Workbench SPA (P4.6bb) as one round — the
natural pairing — the human M5 + finding-#12 walk (STILL
outstanding), the M6 backlog items 1–4, or the
`p4.9j-workspace-tabs` human ruling.** Round record:
`status-log.md`.

**The unit-12 ∥ P4.6bb Workbench round is UNIFIED on main
(2026-07-18) — P4.6ay is CLOSED (its last item, the unit-12 route
surface, landed as lane AY) and P4.6bb is CLOSED (the whole
Workbench SPA vertical, lane BB).** The `/api/v1/custom-tools`
surface (library / destinations / preview / audit — the four §W
dispatch verbs + REST edge, `AUDIT_RUNS = 10_000`, the
`{characterId}`-first metadata union, v5's first 422 via the new
additive `ErrorKind::Unprocessable`) is live under the `/custom-tools`
SPA vertical (three-mode shell + deep links, library, dual-mode
editor with repair + mtime-conflict flow, builder-form family,
proving bench, destination picker, all four entry points, the
byte-identical schema asset). New proof machinery: the committed
`workbench-{main,mount}.db` fixture family, the
`pascal_workbench_equivalence` (2-case) +
`pascal_workbench_route_equivalence` (24-case, shared-corpus-file)
differentials, the SPA's 115-row byte-level schema-port corpus spec,
and v4's 408-line tool-draft suite ported case-for-case. The four
Workbench e2e beats self-activated at unification. Deferred loud:
the `p4.9j` workspace-tab intents (openers use v4's own no-workspace
query-param fallback), the `finite` message arm (needs a corpus row
in `harness/oracle/`), the error-envelope `details` array
(pre-existing envelope shape), and the `is not valid JSON:`
engine-wording seam (unit 2's, compared by prefix). **Next
candidates: the human M5 + finding-#12 walk (STILL outstanding), the
M6 backlog items 1–4 (`p4.9a`/`p4.9c`/`p4.9b`/`p4.9d`), the
`p4.9j-workspace-tabs` human ruling, or a Workbench/Pascal dogfood
pass.** Round record: `status-log.md`.

**The M6 items 1–4 round (P4.9a ∥ P4.9c ∥ P4.9b ∥ P4.9d) is PARTIALLY
UNIFIED on main (2026-07-18) — P4.9c, P4.9b, and P4.9d CLOSED; P4.9a
OPEN, held back at unit 1 (resume notes in its order header).** Landed:
the About + Profile vertical (the `userProfileGet/Update/SetAvatar` +
`systemDataDir` verbs + REST edges over three fresh pinned-`d68638b4`
differentials; the health `version` carry — v5's UI can finally read its
own version; the `/about` screen with the M6-ruled local-badge
divergence; the `/profile` screen with the reduced avatar picker; the
`qt-user-menu` shell-footer dropdown) ∥ the standalone Generate Image
surface (the shared `ImageProfilePicker` + `provider-icon`, the
`/generate-image` screen over the live four-param `imageProfileGenerate`,
the restored homepage quick action, the standalone in-chat dialog + its
single composer-gutter opener) ∥ the quick-hide system (the three-key
signal service sharing v4's exact localStorage keys, the filter across
salon list / home / roster / detail / Prospero, the menu section MOUNTED
in the user menu at unification with its beat activated to real menu
clicks, the global tags card in Settings → Appearance, and the
ThemePreviewModal re-binned from `p4.9c`). Gate: 350 test binaries /
1,433 / 0 failed (the three new differentials by name, zero SKIP), clippy
both feature sets, release build, ng test 1,706 (151 files), ng build,
full Playwright 78/78 zero skips. Versions: core 0.0.271, harness
0.0.239, host 0.0.20, web 0.0.28, quilltap-tauri 0.0.4, SPA 0.5.169. **⚠
v4 DRIFTED to `616930db` during the round** (the llm-consult
custom-tools feature + Insert-Announcement Pascal + outcome-test
comparators — it touches the PORTED Pascal/workbench surfaces; zero
overlap with this round). The photos nav item stays disabled (`route:
null`) until P4.9a lands (§2a). **Next candidates: the `616930db` drift
catch-up round (classify → re-port; the natural next `/setupphase`),
finishing P4.9a (resume at unit 2), the `p4.9j` workspace-tabs round
(ruled: retirement gates on it), or the M6 backlog items 5+.** Round
record: `status-log.md`.

**The `616930db` drift-catch-up + P4.9a-resume round (P4.d8 ∥ P4.6bc ∥
P4.9a) is UNIFIED on main (2026-07-18) — ALL THREE CLOSED; P4.9a closes
with tier 2 deferred whole; the oracle baseline is now `616930db`.**
Landed: the llm-consult server re-port (the `llm` definition block +
contains/ncontains across the schema, the async consult seam through
`execute_custom_tool`, the `pascal::llm_consult` invoker over the
cheap-LLM ladder, `CUSTOM_TOOL_CONSULT`, `pascalMeta.llm` through all
three writers, the workbench scripted-oracle params — audit has no
live arm BY SHAPE — with 14 differentials over fresh `616930db`
oracles and the §C corpus regen 115 → 159) ∥ the Workbench SPA half
(the browser schema twin + tool-draft bijection + the consulted-oracle
card / condition chips / bench oracle card / library badge + the
byte-copied schema asset + the Inspector consult type; v4's new suites
ported case-for-case) ∥ the My Photos tier-1 vertical (the 811-line
user-gallery service, the four `photoGallery*` verbs + REST edges, the
committed `photos-{main,mount}.db` family + 34-case differential, the
`/photos` screen, the three-beat live walk). Wires: §B/§3 CoreRequest
folds, §C counts, the §2a photos nav flip (LIVE), BC's beat 6
self-activated. Gate: 353 binaries / 1,444 / 0; the 17 differentials
by name zero SKIP; ng 154 files / 1,844; full Playwright 83/83 zero
skips. Versions: core 0.0.279, harness 0.0.244, web 0.0.31, SPA
0.5.175. **Standing (the next-order pool): the consult is DARK in
production** — the three entrances hold no `CompletionProvider` and
the 60 s timeout decorator is unwired; one host-side erased-provider
thread through `EngineAssembly` closes all four (the natural first
item of the next order) — **plus P4.9a tier 2** (`imageInfoGet` + the
deep gallery modal family), the `979aec66` Insert-Announcement bank,
the `jsnum` DRY rider, `p4.9j` workspace tabs (v4 retirement gates on
it), and the M6 backlog items 5+. Round record: `status-log.md`.

**The consult-wire + image-detail + wardrobe round (P4.6bd ∥ P4.9a2 ∥
P4.9f1 ∥ P4.9f2) is UNIFIED on main (2026-07-19) — ALL FOUR CLOSED, and
`p4.9a` closes with P4.9a2.** Landed: the consult wire (the erased
`ConsultRunner` seam on `EngineAssembly`, `HostConsultRunner` rebuilding
the provider per consult, the `TimeoutConsult` decorator carrying v4's 60 s
`withTimeout` — **the llm consult is LIVE on all three entrances and now
costs real money**; the P4.d8 timeout deferral closes with it) + the §3
`jsnum` canonicalization ∥ the image-detail modal family (`imageInfoGet`,
the deep `ImageDetailModal` + `ImageMetadata` panel, prev/next with the
nested-Escape suppression, `ChatGalleryImageViewModal`, the aurora gallery
tab at `EmbeddedPhotoGallery` parity) ∥ the wardrobe server surface (chat
equip with **all seven modes incl. v4's deprecated `equip` alias**, outfit
read, the transfers pair over the already-ported service, the global
archetype tier; a new committed `wardrobe-routes-{main,mount}.db` family +
a 74-check / 66-case differential) ∥ the wardrobe SPA (the control dialog
in BOTH modes — in-chat staging with the one-shot `set_all` flush,
out-of-chat fitting room firing NO equip route — the tier-routed item
editor, the transfer dialog, three entry points, and the disabled
`wardrobe-tab.ts` stub retired). Wires: the §1 `CoreRequest` folds with
both casts retired, `EquippedSlots` moved into `core-contract.ts`, the §3
`photos_routes.rs` swap (which also fixed a latent `Number('+0x10')`
divergence), and F2's ACTIVATE-AT-UNIFY beat self-activating. Gate: 354
binaries / 1,450 / 0 failed with the round's 7 differentials by name zero
SKIP, clippy both feature sets, release build, ng test 171 files / 2,004,
full Playwright 86/86 zero skips. Versions (recounted from the commits —
two silent collisions): core 0.0.283, harness 0.0.246, host 0.0.22, web
0.0.34, SPA 0.5.183.

**⚠ The round's one user-visible gap: `wardrobePreviewAvatar` is
half-live** — the render step answers a typed refusal until the
`avatar_preview` host wire lands, so the wardrobe dialog's out-of-chat
Preview button reaches a loud refusal. **That wire is blocked on the
already-deferred production WebP codec seam** (P4.6y), so closing it means
porting that codec first. It is the natural first item of the next order.

**⚠ v4 had DRIFTED to `b8b12695`** (one commit: LaTeX/KaTeX math rendering
— it refactors `markdown-renderer.service.ts` and adds
`markdown-postprocess.ts` + `lib/markdown/math.ts`, and **touches ported
markdown/message-rendering surfaces**). Deliberately NOT absorbed in that
round, by the human's instruction. **That catch-up has since RUN as P4.d9
and is CLOSED — see the next section; the oracle baseline is now
`b8b12695` and the pin `/private/tmp/qt-v4-pin-616930db` is retired.**

---

## The P4.d9 `b8b12695` KaTeX/markdown drift catch-up round — UNIFIED 2026-07-19

**P4.d9 CLOSED** (`work-orders/p4.d9-katex-markdown-drift.md`). One SPA-only
lane, zero Rust source touched — the drift is behavior-neutral for the Rust
core because v4's renderer output only ever surfaces as `renderedHtml`,
which v5 omits by locked decision and the salon tier-2 diffs strip.

Landed: the shared math module (`normalizeMathDelimiters`,
`MATH_SKIP_PATTERN`, `REMARK_MATH_OPTIONS` with single-dollar math OFF) +
v4's `katexDepth` KaTeX-subtree skip in `applyRoleplayPatterns`;
`remark-math` + `rehype-katex` wired into the one Salon renderer at v4's
exact plugin positions (math parse between gfm and breaks, KaTeX render
before highlight) with step 2.5 normalization ahead of
`escapeMarkdownInBrackets`; the KaTeX stylesheet via `angular.json` + the
`.katex-display` overflow rule; `markdown-fixtures.json` regenerated from
v4's REAL renderer at `b8b12695` (23 → 34 fixtures); a live e2e math beat.

**The baseline move is proven, not assumed:** all seven oracle families that
transitively import v4's renderer (salon-reads/-mutations/-skip/
-swipe-generate, text-replacements-routes, cost-background-routes,
courier-images-routes) were regenerated at `b8b12695` and their differentials
re-run by name — zero SKIP, all green, committed oracles behaviorally
unchanged.

Gate: 354 binaries / 1,450 / 0; the seven differentials by name zero SKIP;
clippy both feature sets; release build; ng test 172 files / 2,029; ng build
clean; full Playwright 87/87 zero skips. Versions: core 0.0.283, harness
0.0.246, host 0.0.22, web 0.0.34, quilltap-tauri 0.0.4, SPA 0.5.189.

**Deferrals this round did NOT close** (each tracked in the order header):
v4's help `math-notation.md` (no v5 help-render surface — banked for
`p4.9i2`); FilePreviewText math (the P4.6af rich-stack deferral); and the
**composer backslash-escape seam** — qt-rich-editor's markdown serializer
escapes typed `\(`/`\)` to `\\(…\\)`, so `\(…\)` typed into the composer does
not render as math. The normalization itself is fixture-proven; closing the
round-trip is a dialect-bridge change.

**Next candidates:** the `avatar_preview` host wire + the WebP codec it
needs (**the named next Rust item**, blocked on the P4.6y codec seam);
`p4.9j` workspace tabs (v4 retirement gates on it — wants a DEDICATED
round, since it rewrites the shell and `app.routes.ts` and would collide
with any concurrent SPA lane); `p4.9i1`/`p4.9i2` (Brahma / HelpChat); M6
backlog rows 5/6/8–15; the composer backslash-escape seam; or the
`js_number_to_json` serialization rider. Round record: `status-log.md`.

---

## The P4.9J1 ∥ P4.9J2 workspace-tabs round — UNIFIED 2026-07-19

**Both orders CLOSED** (`work-orders/p4.9j1-workspace-core-shell.md`,
`p4.9j2-screen-hostability.md`); **`p4.9j` — v4's DEFAULT shell and the F1
v4-retirement gate — is LANDED and ON by default.** Two pure-SPA lanes over
the pre-committed contract file
(`apps/web/src/app/workspace/workspace-contract.ts`, unchanged by both):

- **P4.9J1**: the pure core (reducer/persistence/tab-meta/route-to-intent)
  with the captured-corpus tier-1 differential against v4's REAL
  `lib/workspace` + `lib/navigation` (committed
  `workspace-core-fixtures.json`, 144 replay assertions; lane J1 owns
  regen); the signal `WorkspaceService`; the two-pane keep-alive host +
  chrome (strip/divider/backdrop/portals/shortcuts/interceptor/intent);
  the flag (default ON, `quilltap.workspace.tabs !== '0'`) + 16 redirect
  guards + the shell cutover; in-lane hosting (12 no-input kinds); the e2e
  dual-mode harness (global route-mode opt-out for the whole existing
  suite; `workspace-flow.spec.ts` runs flag-on).
- **P4.9J2**: dual-mode signal inputs for the five param/query screens (+
  EntityTabs hosted mode); the self-close seam; the three in-tab drills
  (characters incl. group editor / prospero / scriptorium); the
  `SalonModePanes` child-tab source (per-document + terminal sibling tabs
  via embedded-view DOM-move portals — PTY/editor state survives, spec-
  proven); Salon backdrop reporting; the opener intents.
- **Unification wires**: the five AT-UNIFY kinds bound in the tab registry
  (salon/settings/character-edit/character-view/custom-tools); the REVERSE
  child-tab close via portal-registry disappearance (a seen key with no
  node ⇔ tab closed); the settings e2e beat extended onto the real screen
  + salon-funnel and characters-drill activation beats.

Gate: 354 Rust binaries / 1,450 / 0 (zero Rust changed); the corpus
byte-identical from the pinned `b8b12695` worktree; ng test 187 files /
2,258; ng build clean; full Playwright green zero skips (a pre-existing
composer-modes beat gained a pause-before-send gesture fix (the group turn chain's terminal state is run-order-dependent and can disable the composer)). Versions: crates
unchanged; SPA 0.5.209.

**Still not-wired tab kinds (loud, named):** `wardrobe` (the `asTab`
WardrobeView variant — ported by NEITHER lane; the dialog entry points on
the character screens keep working), `document-standalone` (J2 tier-2
item 7 — needs file-scoped document I/O; `doc_focus` folds in), `brahma`
(p4.9i1). Other named follow-ups: the round record in `status-log.md`.

**The drift catch-up ran and UNIFIED (2026-07-20): the state-cascade
round (P4.d10 ∥ P4.6be ∥ P4.d11), re-baselined at `7e6d13e5` after the
4.8.0 release sweep — all three orders CLOSED.** The oracle baseline is
now **`7e6d13e5` (4.8.0-dev.92)**; both pins retired — oracles
regenerate from the clean `~/source/quilltap-server` checkout directly
(pin again only on drift or a dirty tree). Landed: the four-tier state
cascade end-to-end (server modules + nine §A verbs + the four-tier
state tool + Pascal `$state` incl. workbench mock-state + the
math-notation prompt note) ∥ the state-cascade SPA (four-entity State
Editor, Group/General State entries, `$state` pills + tool-draft kind)
∥ the release-sweep SPA slice (single-dollar math promotion, katex
0.18.1, workbench backdrops). The release sweep's "no functional
change" commits (`93604767` dedup, `28e89f51` logging prune) were
VERIFIED, not ported — 53 oracle families regenerated + re-run by name,
all green. Round record: `status-log.md` (2026-07-20).

~~**Next candidates:** the `avatar_preview` host wire + the WebP codec
(the named next Rust item); the wardrobe `asTab` tab surface + the
standalone document surface (the two not-wired workspace kinds); a
workspace dogfood pass (the shell changed by default — high value; a
state-cascade dogfood beat would also exercise the new editor);
`p4.9i1`/`p4.9i2`; M6 backlog rows 5/6/8–15; the composer
backslash-escape seam; the chat-tier State-Editor opener rides the
ChatSidebar follow-up. Watch v4 for the 4.8.0 release tag (v4 is
mid-release-checklist — drift-check before every round).~~
*(Superseded by the workspace-tabs remainder round below.)*

---

## The workspace-tabs remainder round (P4.9I1A ∥ P4.9I1B ∥ P4.9J3 ∥ P4.9J4) — UNIFIED 2026-07-20

**All four orders CLOSED; the three not-wired workspace tab kinds are
GONE — all 22 tab kinds now host real screens and the `NotWiredPane`
refusal scaffold is retired.** `p4.9i1` (Brahma) CLOSED: the multi-turn
orchestrator + the eight-verb `brahma-console` dispatch family + REST
edges server-side (two new differentials over the committed
`brahma-{main,mount}.db` family; the send rides the new
`BrahmaConsoleSendDriver` host seam — **LIVE, real spend**) ∥ the whole
console SPA (dialog in both modes, streaming over the shared reducer,
rail entry). `p4.9j3` CLOSED: the `asTab` WardrobeView + the p4.9j
riders (openChatOnMount via `/salon/new` — documented divergence; the
Create-Character in-tab arm; the `mode=setup` guard bypass; the HTML5
drag-split beat; the accent ruling CORRECTED to no-change — theme packs
already carry v4's live tokens). `p4.9j4` CLOSED — and P4.9J2 tier-2
item 7 with it: the standalone Document Mode surface over the existing
P4.6w verbs (wire fold, the autosave/absorb/409 screen, the picker
standalone variant + rail entry). Round record: `status-log.md`
(2026-07-20).

**Next candidates:** the `avatar_preview` host wire + the WebP codec
(the named next Rust item, blocked-on-codec since P4.6y); a
workspace/state/brahma dogfood pass (`/dogfood` — the shell, the state
editor, and the new console + standalone documents have never been
hand-walked on real data); `p4.9i2` (HelpChat — nothing ported above
`services/help_doc_sync.rs`); the ChatSidebar surface (`p4.9h` — carries
the chat-tier State-Editor opener and the J2 item-8 narrow-pane
overlay); M6 backlog rows 5/6/8–15; the composer backslash-escape seam;
the brahma async context-summary/auto-title drive (deferred with the
production finalizer's). Watch v4 for the 4.8.0 release tag (v4 is
mid-release-checklist — drift-check before every round).

---

## The codec + fs seam round (P4.6bf ∥ P4.6bg) — PARTIALLY UNIFIED 2026-07-21

**P4.6bf CLOSED; P4.6bg OPEN (unit 1 landed — resume at unit 3).** Lane
BF (`work-orders/p4.6bf-avatar-preview-blob-codec-wire.md`): the
`HostAvatarPreviewRenderer` over the existing P4.1b `HostImageCodec` —
**`avatar_preview` is LIVE and the wardrobe out-of-chat Preview button
now costs real money** (the e2e beat pins the pre-provider no-API-key
arm at zero spend; the live render walk is a dogfood item); the
blob-transcode `WebpTranscoder` impl + the S1 `EngineAssembly.blob_webp`
field (deliberately dead — see below); the wardrobe-routes family
re-verified at `7e6d13e5` (74 checks / 0 SKIP); the ST
placeholder-DEFLATE seam DEFERRED with the empirical finding (byte
parity holds ONLY via flate2's zlib C backend; recipe banked in the lane
record). Lane BG (`work-orders/p4.6bg-docedit-fs-general-scope.md`),
unit 1 only: the doc-edit path-resolver host-filesystem branches
(general scope / fs mounts / legacy project fallback, `safe_realpath` +
`verify_path_is_within_base` byte-exact) behind the S2 `files_dir`
thread — **every call site still passes `None`, so production behavior
is unchanged**; the path-resolver differential extended with 10 fs cases
over the canonical-scratch `__ROOT__` sentinel recipe.

**The round's one AT-UNIFY item was NOT performable and is INHERITED by
P4.6bg unit 6:** wiring `EngineAssembly.blob_webp` into the
`store_mount_file` handlers needs BG's handler re-signature (open).
Until then the scriptorium WebP e2e beat stays probe-skipped and
`ReadyEngine.blob_webp` stays `#[allow(dead_code)]`. BG's unit-1 record
also flagged a pre-existing P4.d7 divergence (dup-named mounts:
v4 `findByName` counts overlaid names, v5 reads the raw column) —
spawned as a follow-up, not part of this round.

**Next candidates:** resume P4.6bg (units 3–6 — the tool-site fs I/O,
the fm/ui/text fs differentials, the engine `files_dir` wire + the
standalone-beat flip, conversion + the inherited blob_webp wire); a
wardrobe-Preview / workspace / state / brahma dogfood pass (the Preview
render has never been walked with a real key); `p4.9i2` (HelpChat); the
ChatSidebar surface (`p4.9h`); M6 backlog rows 5/6/8–15; the composer
backslash-escape seam. Watch v4 for the 4.8.0 release tag (v4 is
mid-release-checklist — drift-check before every round).

---

## The P4.6bg remainder — UNIFIED 2026-07-21 (the codec + fs seam round fully disposed)

**P4.6bg CLOSED (tier 1 complete + P4.6bf's inherited blob-WebP wire
RESOLVED); ONE loud tier-2 deferral: the conversion port** (`services/
mount_index/conversion.rs` + un-refusing convert/deconvert + its exact
tier-2 differential — enumerated in the order header, resume with a fresh
drift-check). Landed: the tool-site fs I/O (all six file-management verbs,
new-blank, the grep/list fs walks), the NEW `doc_fs_equivalence` family
(21 fs ops + byte-exact fs-tree diff), the engine `files_dir` wire + the
operator fs surface, the standalone general-scope LIVE round-trip beat,
and the S1 blob-WebP dispatch wire (the scriptorium WebP beat
self-activated — mount blob uploads transcode at the dispatch layer). One
DELIBERATE v5 divergence: `_general` is pre-created (v4's latent
fresh-instance quirk). Round record: `status-log.md` (2026-07-21).

**⚠ v4 DRIFTED to `e2eb3d21` (4.8.0-dev.93) during the lane — zero
`lib/` code (New-Chat picker components + help/chats.md + versions); the
oracle baseline STAYS `7e6d13e5`.** Owed: a P4.d-style SPA re-port of the
New-Chat picker behavior (full roster incl. default-user personas in
Select Characters; Play As limited to the cast; revert-to-yourself keeps
LLM control) onto the ported P4.6q `/salon/new` vertical + the
help/chats.md sync. v4 also carries an untracked in-progress
`episodic-recall-overhaul.md` feature doc — expect that feature to land.

**Next candidates:** the New-Chat picker drift re-port (small, SPA-only);
the conversion port (the P4.6bg tier-2 deferral); a
wardrobe-Preview/workspace/state/brahma/fs-documents dogfood pass (the
Preview render and the new general-scope documents have never been
hand-walked on real data); `p4.9i2` (HelpChat); `p4.9h` (ChatSidebar); M6
rows 5/6/8–15; the composer backslash-escape seam. Watch v4 for the
4.8.0 tag and the episodic-recall feature (drift-check before every
round).

## The episodic-recall drift catch-up — a 3-round campaign, ROUND 1 UNIFIED 2026-07-21

**ROUND 1 (P4.d12 ∥ P4.6bh ∥ P4.6bi) UNIFIED on main 2026-07-21** — the
episodic SPINE + both orthogonal character slices landed. The five new
columns exist and marshal (D23 re-dump), the pure `episodic` module +
memory-weighting/injector date logic are ported, the memory-row/pure +
character + new-chat oracle families rebased onto `8bf3cb5f`, and the
`canChooseOutfit` vault flag + wardrobe-permission PUT toggles +
Wardrobe-tab card + New-Chat picker re-port all landed. Gate: 361 Rust
binaries / 1,474 / 0 (key differentials fresh from `8bf3cb5f`, by name),
clippy both, release build, ng 203/2,448, Playwright 109 + 1 documented
flake. Versions: core 0.0.305, harness 0.0.263, host 0.0.28, SPA 0.5.245.
Round record + lane records in `status-log.md`. **Baseline is now MIXED**
(memory-row/pure + character + new-chat families at `8bf3cb5f`; the
deferred behavior families at `7e6d13e5`).

**ROUND 2 UNIFIED on main (2026-07-21) — P4.d13 CLOSED.** Retrieval is
time/entity-aware end-to-end (distill signals + TODAY line; recall-tags
retro flip / window boost / re-ask suspension; `search_memories_semantic`
occurred-within two-stage + entity-anchor union + retro multi-probe —
the recallContext/expansion deferral CLOSED; vault-summary date staging;
buildContext part 1 + the RETRO head constants); the deep-dive tools
carry `since`/`until`/`aboutCharacter` + episodic result fields +
`read_conversation` slicing + the anti-confabulation prose, and the
stale `memorySearch` catalog entry is GONE (57 tools); the §3 replay
harness is LIVE end-to-end (`chatRecallReplay` on the new
`RecallReplayDriver` host seam — one real cheap-LLM call per replay —
+ the `quilltap recall-replay` CLI, Tier-R-diffed); chat updates accept
`timelineMode`. Three NEW oracle families (distill = the memory-tasks
SPLIT, recall-replay, vault-conv-search); 12 families regenerated at
`8bf3cb5f`. Gate: 364 binaries / 1,496 / 0, zero SKIP by name, clippy
both feature sets, release build, ng 203/2,448 (SPA untouched), full
Playwright 110/110 zero skips. Versions: core 0.0.313, harness 0.0.270,
host 0.0.29, web 0.0.37, cli 0.0.2. Round record + lane records in
`status-log.md`.

**NEXT: ROUND 3 — the campaign's final round** (workstreams A-creation +
C part 2 + E + the Story's Clock SPA): the clocked creation-extraction
prompt + EVENT category + `kind`/`when`/`entities` coercion +
`capCandidates`; `resolveWhenPhrase`/`resolveCandidateAnchors` + the
turn-path `occurredAt` stamp; `applyEpisodicFallbackAnchors` (the gate
family `QT_ORACLE_GATE` stays SKIP until then); the fold-episode pass +
context-summary wiring; buildContext PART 2 (mini-recap +
`retrospective-recall` whisper + spam guard + `appendRetroSignature` —
round 2 ported `parse_retro_signatures` as a preservation carrier only,
and the vault-summary `time_range` gains its first production caller
here); the compression keep/drop flip; the gate date-guard +
housekeeping merge guard; the Salon "Story's Clock" timeline-mode SPA.
**Round-3 carry-ins flagged by rounds 1–2** (do not lose): the gate
tier-3 family stays un-regenerated (v4's first-write fallback is
non-inert on AUTO-source proper-noun content); the processor tier-3 +
memory-tasks CREATION cases + context-summary/fold + carina-extraction
families stay at `7e6d13e5` until round 3 ports them. (Original
round-1 planning section follows.)

**ROUND 3 UNIFIED ON MAIN (2026-07-22) — P4.d14/P4.d15/P4.9H1 all
CLOSED; THE CAMPAIGN CLOSES.** The `8bf3cb5f` drift is fully absorbed;
the round-3 family vintages all moved to `8bf3cb5f` (the gate family
un-SKIPPED and green). Gate: 365 binaries / 1,505 / 0 with all 27 round
families fresh + by-name zero SKIP; clippy both feature sets; release
build; ng 209/2,487; full Playwright green from the fresh dist. Round
record: `status-log.md`. **Next candidates, in rough value order:**
1. ~~**Wire the memory pipeline's job handlers**~~ — **DONE (P4.6bj,
   2026-07-22; order:
   `work-orders/p4.6bj-memory-pipeline-job-handlers.md`, record in the
   status log).** The `orchestrator_tier3` stale-RED closed first (the
   P4.d15 recap diagnosis was already healed by round 3 — the residual
   was the in-loop fold-episode seam), then `buildTurnTranscript` +
   both handler bodies landed with a new tier-3 family and BOTH
   handlers registered in the host — **the extraction/fold pipeline is
   LIVE in production and costs real money on every closed turn.** Its
   first live proof is the next dogfood pass (item 2 below).
2. **An episodic + sidebar dogfood pass** on the Friday copy — the
   retrospective mini-recap/whisper, the Story's Clock, and the sidebar
   are live surfaces nobody has hand-walked on real data.
3. ~~The v4 `8d86847a` **tabbed-workspace deep-links drift re-port**~~
   — **DONE (P4.d16, unified 2026-07-22; order:
   `work-orders/p4.d16-workspace-deeplinks-drift.md`).** The
   `salon-list` tab kind, the three drill-in payloads,
   `character-view` in the `?open=` layer, the terminal-popout
   salon+child funnel, six new redirect guards, and the `/salon/new`
   funnel translated to the v5-only `salon-new` tab kind (the
   no-modal divergence, recorded in `m6-screen-parity.md` F1). The
   workspace corpus regenerated at `e646f58b`.
4. ~~The v4 `deab0e5d` **theme/icons drift SPA re-port**~~ — **DONE
   (P4.d17, unified 2026-07-22; order:
   `work-orders/p4.d17-thinking-indicator-theme-drift.md`).** v5 had
   never ported QuillAnimation at all (the status strip showed a
   pulsing dot); the lane ported the indicator fresh at its
   post-drift shape (the `thinking` icon, the `.qt-thinking-indicator`
   hook, `qt-quill-animation` at all four call-site analogs) and
   refreshed Madman's Box 1.1.5 → 1.1.7. **With both lanes unified
   the four-commit drift is fully absorbed and the oracle baseline is
   `e646f58b`.**
5. `p4.9h2` (the settings remainder bucket), the P4.9H1 tier-3 sidebar
   deferrals, the ExtractionClock consolidation rider, p4.9i2
   (HelpChat), M6 rows 5+, or the conversion port.

Original round-3 plan follows.

**ROUND 3 PLANNED (2026-07-22) — three parallel lanes (orders
committed):** drift-check at planning: v4 HEAD == `8bf3cb5f`, tree
clean — no new drift; the round absorbs the FINAL un-ported portion of
`8bf3cb5f` (workstreams A-creation + C part 2 + E) plus the Story's
Clock SPA.

- **P4.d14** (`work-orders/p4.d14-episodic-creation-fold.md`) — lane A,
  Rust: the clocked creation prompts (CLOCK block, EVENT category,
  `kind`/`when`/`entities` coercion, `capCandidates`), the processor
  `resolveCandidateAnchors` + turn-path `occurredAt` stamp, the
  first-write `applyEpisodicFallbackAnchors`, the gate date guard +
  reinforce anchor upgrades + embedding anchor line (**`QT_ORACLE_GATE`
  un-SKIPs and goes green here**), the NEW fold-episode pass +
  context-summary wiring, the fold Timeline, the housekeeping
  `mergeSimilar` guard. Owns the memory subsystem files +
  `services/context_summary/**` + `services/mod.rs`.
- **P4.d15** (`work-orders/p4.d15-retro-minirecap-whisper.md`) — lane
  B, Rust: buildContext PART 2 — the scoped mini-recap (the
  vault-summary `time_range` gains its first production caller), the
  `retrospective-recall` whisper + sweep membership, the spam guard,
  the recall-history retro-signature machinery; the build-context
  fixture EXTENDS with matching vault summaries (the round-2 no-match
  guard inverts, keeping one inert arm). Owns
  `services/build_context.rs`, `recall_history.rs`,
  `services/commonplace_notifications.rs`. Rider: the compression
  keep/drop flip needs NO code (v5's `compressMemories` is an unported
  tracked deferral — comment update only, `build_context.rs:2264`).
- **P4.9H1** (`work-orders/p4.9h1-chat-sidebar-storys-clock.md`) — the
  SPA lane: the ChatSidebar vertical (the M6 `p4.9h` bucket SPLITS —
  this is the sidebar half; the settings remainder stays banked as
  `p4.9h2`) with **the Story's Clock** select (v4
  `ChatSidebar.tsx:1147–1165`, copy verbatim) over the frozen round-2
  chat-PUT `timelineMode` arm. Owns `apps/web`; NO server changes;
  owns the in-round Playwright run (the server lanes defer full
  Playwright to the unifier — port-4319 discipline).

Planning survey findings recorded in the orders: v4's
`carina-memory-extraction.ts` and `turn-transcript.ts` have ZERO diff
at `8bf3cb5f` (their families regen transitively);
`applyEpisodicFallbackAnchors` lives in `memory-service.ts:193` (not
the gate file); v5's `compressMemories` was never ported (the flip is
a comment-only disposition); the Story's Clock's v4-faithful home is
the unported ChatSidebar, hence the p4.9h split. Pascal `persist`
stays deferred (deferred in v4 itself). After this round the campaign
CLOSES and the whole oracle baseline should be uniformly `8bf3cb5f`
except families untouched since earlier vintages (established
pattern).

**ROUND 2 PLANNED (2026-07-21):** the order is
`work-orders/p4.d13-episodic-retrieval-tools-replay.md` — a deliberate
SINGLE lane (D's search handler and the §3 replay both consume B's new
`searchMemoriesSemantic` options, so parallel lanes could not run their
differentials in-lane). Eight sequenced tier-1 units: the distill
signals (the memory-tasks family SPLITS: search-extraction cases regen
at `8bf3cb5f`, creation cases stay `7e6d13e5`), recall-tags turn-aware,
the search options (occurredWithin two-stage / entity anchors /
multi-probe), vault-summary dates, buildContext part-1 threading + the
RETRO_HEAD constants (part 2 — mini-recap/whisper/spam-guard — is
round 3; the order pins the boundary and the no-matching-vault-summaries
fixture guard), the deep-dive tools (+ the v5 `memorySearch` catalog
deletion, 58 → 57), the recall-replay module + verb + NEW
`QT_ORACLE_RECALL_REPLAY` tier-3 family over a new committed
episodic-recall fixture, and the `quilltap recall-replay` CLI (Tier R).
Tier 2 picks up the verified v5 gap: the chat-PUT `timelineMode` accept
arm. The v4 design doc is now mirrored at
`docs/v4/developer/features/episodic-recall-overhaul.md` (a round-1
leftover, fixed at planning).

### Original round-1 planning record (2026-07-21)

**⚠ v4 DRIFTED to `8bf3cb5f` — the largest single drift in the port.**
`git log 7e6d13e5..HEAD` at planning shows two commits past the baseline:
`e2eb3d21` (New-Chat picker; the already-owed lib-free SPA re-port) and
**`8bf3cb5f` — "Unify episodic recall + character outfit work"**, a
squash-merge of THREE feature branches (~4,400 insertions, ~40 already-
ported `lib/` files):

- **episodic-recall-overhaul** — event-time on memories
  (`occurredAt`/`narrativeTime`/`entities`/`kind` + `chats.timelineMode`
  + `idx_memories_occurredAt`), time/entity-aware retrieval, a fold-time
  episode pass, a fourth (recall-on-reference) cadence, deep-dive tool
  time filters, and creation-side changes. Touches the WHOLE memory
  subsystem (weighting, gate, service, processor, recall-tags,
  housekeeping, injector, memory-tasks, context/fold), the tools, and a
  new pure `episodic` module. Design authority:
  `docs/v4/.../features/episodic-recall-overhaul.md` (five workstreams
  A–E + a §3 replay harness).
- **character-outfit-selection** — `canChooseOutfit` (a vault
  `properties.json` flag, optional-with-default `false`; **no DB
  column**) + the Aurora Wardrobe-tab editor + the Starting-Outfit
  default in the new-chat outfit-selector.
- **blissful-einstein** — persist the `canDressThemselves`/
  `canCreateOutfits` PUT toggles (**pre-existing DB columns**; just add
  to the character PUT allowlist).

This is a **3-round campaign** (the memory subsystem is one deep,
interdependent vertical that resists parallel splitting; the two
character slices are small and fully independent). The load-bearing
design fact enabling a clean split: the feature's **inert-path
guarantee** (spec §4 — "degrade to today, never block"): on the existing
fixtures (semantic memories, null `occurredAt`, non-retrospective turns)
v4's new code is byte-identical to old, differing only by the new column
values in emitted rows. So the foundation lands the columns + baseline
rebase first; the new *behavior* lands later with new fixtures.

**ROUND 1 — three parallel lanes (orders committed 2026-07-21; human
scoping ruling: "Foundation + both slices"):**

- **P4.d12** (`work-orders/p4.d12-episodic-spine-foundation.md`) — the
  KEYSTONE. The D23 schema re-dump (`chats.timelineMode` + the four
  `memories` columns + the index), the five columns through the memories/
  chats data layer (marshal, insert/update structs, defaults, the
  create-path `occurredAt` stamp), the pure `episodic` module (+ tier-1
  differential), the memory-weighting deltas (`episodicBonus` +
  event-clock age), the injector's dated dynamic head, and the
  **memory-family oracle rebase** onto `8bf3cb5f` for the row-emitting +
  pure families (verified inert). Rust; owns `fresh_schema.json` + the
  memory subsystem. Explicitly DEFERS all episodic *behavior* to rounds
  2/3.
- **P4.6bh** (`work-orders/p4.6bh-character-outfit-server.md`) — the
  character-outfit + wardrobe-permission SERVER slice: the
  `canChooseOutfit` vault field + the two tri-state PUT toggles + a
  characters-route differential. **No schema re-dump** (vault field +
  pre-existing columns). Rust; owns the character vault-overlay +
  `api/characters.rs`. Disjoint from P4.d12.
- **P4.6bi** (`work-orders/p4.6bi-outfit-newchat-spa.md`) — the SPA half:
  the Wardrobe-tab `canChooseOutfit` checkbox, the outfit-selector
  Starting-Outfit default, and the owed **New-Chat picker re-port**
  (`e2eb3d21`: full roster in Select Characters, cast-only Play As,
  keep-on-revert) + the `8bf3cb5f` outfit plumbing. Angular; owns
  `apps/web`. Consumes P4.6bh's `canChooseOutfit` contract (binding,
  reproduced verbatim in both orders).

**Baseline after round 1:** MIXED, by design — the memory-row/pure +
character + new-chat families rebase to `8bf3cb5f`; the extraction-prompt,
fold, retrieval, and deep-dive-tool families stay at `7e6d13e5` until
rounds 2/3 port their behavior (the established "untouched families keep
their vintages" pattern). CLAUDE.md's baseline paragraph updates at the
round-1 unification.

**ROUND 2 (future — workstreams B + D + §3):** time/entity-aware
retrieval (`searchMemoriesSemantic` `occurredWithin` + entity anchoring
+ multi-probe), `turnTemporal` made real, vault-summary date filters,
the deep-dive tools (`search` `since`/`until`/`aboutCharacter`,
`read_conversation` range, the orchestration prose), and the
`recall-replay` CLI + replay harness.

**ROUND 3 (future — workstreams A-creation + C + E):** the clocked
extraction prompt + EVENT category + `kind`/`when`/`entities` output,
the fold-time episode pass (new module), recall-on-reference (the fourth
cadence: enlarged head + scoped mini-recap + the `retrospective-recall`
whisper), the fold Timeline section, the gate date-guard, the
compression keep/drop flip, and the Salon "Story's Clock" timeline-mode
SPA switch.

The next `/setupphase` plans rounds 2/3 once round 1 lands.

## The provider-I/O rewrite round (P4.13 ∥ P4.14 ∥ P4.10) — UNIFIED on main 2026-07-23

**All three lanes landed; P4.14 and P4.10 are CLOSED; P4.13 stays OPEN on
exactly one item — unit 9, the 💸 human live proof on the Friday copy**
(a Salon turn where a character USES a tool result, on OpenAI + Anthropic
+ one chat-completions provider; it also re-checks #22's retry loop,
closes #25's row, and can upgrade response-bodies corpus families from
`synthetic: true` to real captures). Delivered: the `StreamMessage`
carrying type end-to-end (tool linkage reaches the wire on all eight
providers — a FOURTH flattening site found in the text-tool loop and a
FIFTH in the Carina query loop, both fixed), the always-on
`tool_wire_call_site` byte pin, the 29-case recorded-body
`response_parse_equivalence` corpus (which caught two MORE #24-class
production bugs on its first run: OpenRouter usage parsed to zeros;
Google raw's getter-only `functionCalls` key), the phase-B restructure
(`RequestMessage` deleted, `ProviderKind` as the one dispatch point,
id-less-tool arms unrepresentable), the ruled failed-cheap-call
`llm_logs` error row, the P4.14 non-validating stable merge sort (both
injector comparators + the audit-found Post Office `sort_newest_first`),
and the P4.10 packaging close-out (the Docker image builds, serves the
real SPA, ships the CLI; `quilltap-web` resolves its dist unaided;
`docs/developer/running.md`). Gate at unification: fmt/clippy both
feature sets/release build clean; 369 binaries / 1,538 / 0 with all
differential env vars; the round's 20 named families re-run
`--nocapture` over oracles regenerated fresh at `e646f58b`, zero SKIP;
all three committed corpora byte-identical after fresh regen; ng test
2,547; ng build clean; full Playwright green zero skips. The
pre-existing `enclave_step_tier3` red P4.13 found was FIXED on a
parallel branch and folded in at this same unification
(`enclave/step.rs` runs the fold-episode pass via
`FoldEpisodePassSeams`; differential green over a fresh TZ=UTC
`e646f58b` oracle; core → 0.0.337). **Standing (loud):** the
response-bodies corpus is all `synthetic: true` pending real captures;
~~the courier paste-resolver carries the enclave step's twin
bare-NoopSeams fold gap (unpinned)~~ **FIXED + unified 2026-07-24**
(the courier fold-episode follow-up lane: `FoldEpisodePassSeams` in
`courier_transport::run_summary_check`, embedding threaded through the
spine's `CourierResolveDriver`, the differential's new at-cadence
`resolve_cadence` case over the extended committed fixture family —
core 0.0.338, harness 0.0.287, host 0.0.31); #26/#27/#28 + the
tracing-subscriber question + `debugMemoryLogs` writer are the
dogfood-fixing run's scope. The original planning block follows.

## The provider-I/O rewrite round (P4.13 ∥ P4.14 ∥ P4.10) — PLANNED 2026-07-23

**The round the 2026-07-23 human rulings dictate** (recorded in
`dogfood-findings.md`'s standing notes and the memory note
`provider-io-divergence-and-post-5-0-refactor`): the provider-I/O rewrite
lands FIRST, then a dedicated dogfood-fixing run (#26/#27/#28 + whatever
the rewrite's live proof surfaces), then a fresh dogfood walk. Findings
#23/#24/#25 were three total outages in one seam in two days, all under a
green differential suite — the rewrite replaces v4's plugin-shaped
provider layer (a JS dynamic-loading artifact v5 carries without benefit)
with data + one typed pipeline, under the ruled invariant that **the wire
bytes stay byte-faithful to v4** and with the verification legs landing
BEFORE any restructuring. The divergence is a ruled one-off, NOT a
precedent.

Three parallel lanes, ownership fully disjoint:

- **P4.13** (`work-orders/p4.13-provider-io-rewrite.md`) — the deep lane.
  Phase A: P4.12's tool-linkage fix + call-site differential (finding #25;
  P4.12 folds in whole) and the recorded-body response-parse corpus (the
  #24 carry-out — v4's real plugins run under a network recorder so the
  SDK unwrap is inside the oracle loop). Phase B: the restructure under
  the completed corpus net + the RULED failed-cheap-call `llm_logs` error
  row (deliberate divergence). Phase C: Brahma-workaround re-check, the
  💸 live tool-use proof on the Friday copy, records. Owns `model/**` +
  the tool-loop/brahma/cheap-llm services + the provider oracle/fixture
  tree; bumps core + harness.
- **P4.14** (`work-orders/p4.14-memory-sort-total-order.md`) — the
  memory-injector sort-comparator panic (kills live turns today; outside
  the rewrite's seam, so ruled fixable now; the leading candidate cause of
  finding #26's silence). RULED 2026-07-23: the non-validating stable
  merge sort — cleared to dispatch. Owns
  `memory_injector.rs` + a new `stable_sort.rs`; bumps core + harness.
- **P4.10** (`work-orders/p4.10-dockerfile-spa-packaging.md`, written
  2026-07-22) — the dev-grade packaging close-out, unchanged; fully
  disjoint (Dockerfile/compose/docs + the `quilltap-web` dist-discovery
  unit); bumps web.

Explicitly left OUT of the round, per the sequencing ruling: fixes for
#26 (fold never fires), #27 (corpus-shaped cheap-LLM config in
`run_summary_check`), #28 (the retrospective classifier — needs the v4
bench comparison first), the tracing-subscriber question, and the
`chat_messages.debugMemoryLogs` writer — all belong to the post-rewrite
dogfood-fixing run. The next `/setupphase` plans that run once this round
unifies.

## The post-rewrite dogfood-fixing round (P4.15 ∥ P4.16 ∥ P4.17 ∥ P4.18) — UNIFIED on main 2026-07-24

**ALL FOUR LANES CLOSED.** Delivered: **P4.15** — finding #27 FIXED: both
broken `run_summary_check` sites (orchestrator + courier) thread the real
`cheapLLMSettings` + ALL the user's connection profiles + resolved danger
settings (absent-key default `PROVIDER_CHEAPEST` — the enclave's `"AUTO"`
default is a dead phantom; v4 has no `AUTO` strategy); the single-profile
differential blind spot is closed (both families gained a
selected-profile case — the fold/episode/title `llm_logs` rows carry the
configured cheap profile, red-then-green; the enclave family green
untouched); the finding-#22 `loadedMemories` rider LANDED
(`self_inventory` reports the real turn slate; `browserUserAgent` still
loud). **P4.16** — finding #28 dispositioned **NOT-A-BUG (classifier)**
with evidence (v5's classifier fires on the exact sampled turns; v4's
real classifier benched 20 💸 calls over both windows — the tight-vs-wide
window is a weak-to-null discriminator; the misses are the cheap MODEL
(gpt-5-nano ≪ deepseek-v4-flash) + temp-0.3 noise); no source changed, no
`p4.19` ordered; two threads banked in the findings table (the unported
proactive pre-compute path — recommended-not-ordered fidelity item; the
downstream whisper-suppression look — a future dogfood item). **P4.17** —
the ToolMessage rendering port: TOOL rows render as v4's collapsible tool
card (both layouts, grouping via `initiatedBy`, avatar-fallback,
`delegatedDisplay` short-circuit, `whispered to <names>` label rider,
36 new specs + a live e2e beat); the raw-JSON whisper bubble is gone.
**P4.18** — the ruled arm (a) tracing surface: `init_tracing()` in all
three bins (`RUST_LOG` default info), events at the job runner / spine
error frames / host pump / `log_failed_call`, the non-sibling
`eprintln!` conversions, `tower-http` `TraceLayer` at debug; log output
is explicitly OUTSIDE the differential contract (a first, recorded).

Unification wires: the core version recount (identical 338→339 bumps
merged silently → 0.0.341) + status-log record-order normalization. Gate:
fmt clean; clippy both feature sets `-D warnings` clean; release build
clean; `cargo test --workspace` (TZ=UTC, all four affected families'
env vars) **369 binaries / 1,550 / 0**; the four families re-run by name
over oracles regenerated FRESH from v4 at `e646f58b` (v4 verified clean
before and after) — zero SKIP, the courier `resolve_cadence`
selected-profile case visibly OK; ng test **213 files / 2,583**; ng build
clean; full Playwright **119/119 zero skips** against the fresh dist.
Versions: core 0.0.341, harness 0.0.288, host 0.0.32, web 0.0.39, cli
0.0.3, quilltap-tauri 0.0.5, SPA 0.5.267.

**Standing after the round:** finding #26 has every identified cause
fixed (#23/P4.11, the sort panic/P4.14, error rows/P4.13, #27/P4.15) and
now a tracing surface to catch any residual — it CLOSES at the fresh
dogfood walk. ~~**The walk is the next step (the ruled sequence's third
leg): P4.13 unit 9 (💸 live tool-use proof) rides it, plus the #26
re-check, the P4.17 tool-card on real data, and the first
tracing-assisted session.**~~ **THE WALK RAN 2026-07-24 and walked
CLEAN — zero new findings.** P4.13 unit 9 completed (tool use live on
OpenAI/Anthropic/DeepSeek — #25 + #22 CLOSED; the provider-I/O round
closes whole); #26 + #27 CLOSED on the Friday copy (three fold cycles on
the configured cheap profile); the P4.17 card and the tracing surface
both proved live; #29/#30 surfaced and dispositioned NOT-A-BUG
(v4-faithful; queued post-5.0 v4-first). Not walked: Part D
retrospective-recall live behavior, Part F 15/16, items 10/11 — the next
pass's starting list, recorded in `dogfood-findings.md`. Still banked:
the proactive pre-compute fidelity port, the downstream
retrospective-whisper look, the sibling-owned `eprintln!` sweep,
file-transport log parity, the response-bodies real-capture upgrades
(the walk did NOT capture real bodies). The original planning block
follows.

## The post-rewrite dogfood-fixing round (P4.15 ∥ P4.16 ∥ P4.17 ∥ P4.18) — PLANNED 2026-07-24

**The round the 2026-07-23 sequencing ruling ordered second** (rewrite →
**this** → a fresh dogfood walk): the open dogfood findings #26/#27/#28,
the tracing-subscriber question, and the dogfood-reported riders. v4
drift-checked clean at `e646f58b` at planning. Two survey corrections to
the standing records, verified 2026-07-24:

- **The `chat_messages.debugMemoryLogs` "writer gap" does not exist.**
  Both v5 extraction handlers already write it, byte-matching v4
  (`memory_extraction_job.rs:338`, `carina_memory_extraction.rs:257`).
  The P4.11 lane record's "no v5 writer" line is stale; P4.15 unit 6
  corrects the records. The item DROPS from the round's scope.
- **Finding #27 is TWO sites, not one:** `orchestrator.rs` AND
  `courier_transport.rs` both hard-code the corpus-shaped cheap-LLM
  selection config; the enclave step's `run_summary_fold` is already
  correct and is the reference implementation. The single-profile corpora
  make the defect differential-invisible — the fix's equivalence test MUST
  add a case where the selected profile differs from the responder's.
- **Finding #28's leading cause localized:** v5 has NO proactive
  pre-compute distill path (`pre-compute.service.ts` — the
  `messagesSinceLastSpoke` window that usually classifies the
  backward-looking question nearly alone); v5 always runs the fallback
  `slice(-12)` window. Prompt + parse are tier-1 proven byte-exact; the
  bench decides bug vs NOT-A-BUG.

Four lanes, ownership fully disjoint (each order carries the binding map):

- **P4.15** (`work-orders/p4.15-cheap-llm-config-thread.md`) — thread the
  real `cheapLLMSettings` + the user's connection profiles into both
  broken `run_summary_check` sites (finding #27; #26 closes at the walk —
  every identified cause is then fixed and error rows would surface a
  residual), with selected-profile differentials over extended
  multi-profile fixtures; tier-2 rider: the finding-#22 carry-out
  (`loadedMemories` → `self_inventory`). Owns `orchestrator.rs`,
  `courier_transport.rs`, `chat_settings.rs`; bumps core + harness.
- **P4.16** (`work-orders/p4.16-retrospective-distill-bench.md`) —
  diagnosis-first: bench v4's real distill classifier on the captured
  Friday turns over BOTH windows (💸 small, capped ~20 cheap calls) and
  disposition #28 (NOT-A-BUG record, or the follow-up proactive-path
  order `p4.19` — the port itself crosses P4.15's files and is explicitly
  NOT landed from this lane). Owns `build_context.rs`, `distill.rs`,
  `recall_replay.rs` (touched only if its arms require); bumps only if
  source lands.
- **P4.17** (`work-orders/p4.17-tool-message-display.md`) — port v4's
  `ToolMessage.tsx` (collapsible Tool Request/Response, Success/Failed
  badge, tool-icon header, embedded-vs-standalone grouping via
  `initiatedBy`, `delegatedDisplay` short-circuit) so TOOL rows stop
  rendering as raw-JSON whisper bubbles; rider: v4's dynamic
  `whispered to <names>` label. SPA-only; bumps SPA.
- **P4.18** (`work-orders/p4.18-tracing-subscriber.md`) — **RULED
  2026-07-24 (human): arm (a) — cleared to dispatch** (the standing
  2026-07-23 open question, settled): `tracing` + `tracing-subscriber`
  in the three bins + events at the surveyed swallow sites (job runner,
  spine error frames, host pump, `log_failed_call`), `RUST_LOG` default
  info, explicitly NO differential (log records are operator output, not
  data); arms (b) eprintln-only / (c) status quo recorded in the order.
  Owns the bins, `host.rs`/`spine.rs`, `job_runner.rs`/`cheap_llm_exec.rs`.

Left out of the round deliberately: P4.13 unit 9 (💸 live tool-use walk)
and the response-bodies corpus real-capture upgrades — both ride the
fresh dogfood walk that follows this round; the proactive-path port
(conditional on P4.16's bench); the chokidar-equivalent fs watcher and
the other standing seams (unchanged).

## P4.9G5 restore-execute (single lane) — UNIFIED on main 2026-07-25

**P4.9G5 CLOSED. Restore is LIVE in both modes, and the Backup & Restore family is
complete end to end.** All four Data & System cards that once answered a refusal
now work. Round record at the top of `status-log.md`.

`systemRestoreExecute` runs v4's 35-phase orchestrator in `replace` **and**
`new-account` (the latter over P4.9G6's `remap_backup_data`, which finally has its
caller — so `p4_9g6_seam_contract`'s compile-time pins are load-bearing now).
`system_restore_state` diffs **43 tables across all three partitions** against v4's
real restore over four archives.

**THREE divergences, not the two ruled.** Implementing the 2026-07-25 ruling found
the broadest one: v4 runs phase 5 (files) before both the Uploads mount
`deleteUserData` truncates and the project stores that restore at phase 13, so v4
cannot restore any user file into a fresh or wiped target **in either mode** — the
`>= 2` gate fix alone would not have helped. v5 runs files after the doc-store
family; no write changed, only when it happens. All three v4-side fixes are queued
post-5.0 in `dogfood-findings.md`.

**The unification wire:** the order's tier-1 arm
`restore_preview_writes_nothing` had never been delivered, and nothing else covered
it — preview being read-only was asserted in a comment, never proven, so a preview
that wrote would have passed everything. It is now proven over a populated library
and all five archives, and mutation-checked.

**Gate:** fmt; clippy both feature sets; release build; 381 binaries / 1,621 / 0;
five families by name over fresh `e646f58b` oracles, zero SKIP; corpus
byte-identical; no `apps/web` touched so no SPA run owed. Versions: core 0.0.356,
harness 0.0.305, host 0.0.36.

### Two v5 gaps recorded with tripwires — neither fixed, neither restore's

Both fail their own assertion when closed, so neither can rot into an excuse:

1. **A freshly provisioned character vault is not chunked for search.** v4 writes
   one `doc_mount_chunks` row per vault document as `create_character` writes it;
   v5 writes none, so a new character's vault is not semantically searchable until
   something reindexes. Invisible to the characters family's differentials because
   none of them dump that table. **Follow-up owed there, not here.**
2. **`chat_settings.cheapLLMSettings` writes explicit `null`s** where Zod omits
   absent optionals (`.nullable().optional()`). Pre-existing in the chat-settings
   write path; correct modelling is `Option<Option<String>>` across the settings
   bags, which ripples through every consumer.

### STILL OPEN under P4.9G5 — one item, and where it belongs

The tier-2 **e2e beat** (upload → preview → restore). It must run after the
delete-all describe (a real restore wipes the shared e2e instance) and obliges a
full Playwright run. Three lanes have deferred it for that reason. The server half
is proven, so it is a small write — **land it inside the next round that already
touches `apps/web` and runs a Playwright gate**, rather than giving it a round.

**Next candidates, in rough value order:**

1. **`work-orders/p4.9g4-qtap-export-import.md` — resume at import EXECUTE.** The
   last unported half of the Data & System surface, fully unblocked and disjoint;
   its resume list is in that order's header and the lane record.
2. **A dogfood pass over restore and the Data & System surfaces** — restore in both
   modes on a Friday copy is the obvious first walk, plus the standing debt (**walk
   Part D**, **Part F items 15/16**).
3. **The two recorded gaps above**, either as a small round or as riders.
4. **M6 backlog rows 6+**, then the standing pools (`p4.9i2`, `p4.9e3`, `p4.9h2`,
   the sidebar tier-3 deferrals, `browserUserAgent`, D21).

---

## The "finish the restore side" round (P4.9G5-resumed ∥ P4.9G6) — UNIFIED on main 2026-07-25

**P4.9G6 CLOSED. P4.9G5 still OPEN at units 4–5 — blocked on a human ruling at
unification, ✅ RULED 2026-07-25 and now UNBLOCKED (see below).** Round record at the top of `status-log.md`; the two lane
records sit beside it.

**Landed and live:** restore now works **as far as the preview**. Upload an
archive (`POST /api/v1/system/restore?action=upload`, octet-stream, back-pressured
to a temp zip behind the 1-hour upload store on `BackupHost`) and
`systemRestorePreview` answers v4's 41-key `RestoreSummary`. `parseBackupZip` +
`json_stream` + `legacy_migrations` are ported faithfully, including both
parse-time legacy folds (outfit presets → composite wardrobe items; the
per-character `equippedOutfit` single-UUID-or-null upgrade) and the streaming
array scanner's verbatim thrown messages, which reach the client because the
preview route leaks `error.message`. The extract directory is owned state
(`ExtractedBackup: Drop`) rather than v4's two `finally` sites, and the
differential asserts the scratch root is empty after every case — success or
failure. **The shared "recognized but not yet available" arm in `engine.rs` is
GONE**; `SystemRestoreExecute` refuses by naming the module it waits on.

Also landed, complete and unused: the whole `new-account` UUID remap (P4.9G6).

**New fixture family:** `crates/quilltap-web/tests/fixtures/restore-archives/` —
five archives built by v4's REAL `createBackup`, read byte-for-byte by BOTH sides,
so the restore claim never depends on v5's zip writer. No existing fixture moved.

**Gate:** 380 binaries / 1,616 / 0; `system_restore_equivalence` (5 preview
cases), `backup_uuid_remap_equivalence` (19 cases), `system_backup_equivalence`
(re-run over a fresh oracle) and the new `p4_9g6_seam_contract` all by name with
`--nocapture`, zero SKIP; clippy both feature sets; release build. **No `apps/web`
file was touched by either lane, so no `ng` or Playwright run was owed** — and
none was run. Versions: core 0.0.355, harness 0.0.303, host 0.0.35, web 0.0.44;
SPA unchanged at 0.5.271.

### ✅ THE BLOCKER IS CLEARED — RULED 2026-07-25

**The ruling (human): "I want this work, not just fail the same way v4 fails."
v5 DIVERGES on both findings — restore actually restores.** Full ruling, with the
reasoning and the both-directions assertion discipline: `status-log.md` →
**"Ruling — the two v4 restore bugs (2026-07-25)"**. It decides three things:
finding 1 needs **no** v5 code change (v5 already diverges for free, because its
types are correct — the ruling just permits the differential to accept it);
finding 2 **does** require a deliberate v5 change (`get_file_from_extracted_backup`
still reproduces v4's `backupFormat === 2` gate, so v5 restores no files either —
move it to `>= 2`); and the divergence is **reader-side only**, because fixing
finding 1 on the backup side would turn `system_backup_equivalence` red and make
v5's archives diverge from v4's on disk. **P4.9G5 units 4–5 are UNBLOCKED.**

Both v4-side fixes are queued post-5.0 in `dogfood-findings.md`, with finding 1
flagged as **more urgent than the sparse-array entry** — that one needs a >3 MB
blob to bite, this one bites every modern v4 restore, Friday included.

Bringing up unit 4's tier-2 state differential surfaced **two real bugs in v4's
restore**, both demonstrated by running v4's REAL `restore` against a backup v4
itself produced (the evidence is the `system-restore` oracle's Part 2, committed
and reproducible; the queue entry is in `dogfood-findings.md`):

1. **v4 rejects every `doc_mount_points` and `doc_mount_file_links` row from a
   modern archive.** `dumpMountIndexTable` is a raw `SELECT *`, so the archive
   carries the array columns as JSON *text* and the booleans as `0`/`1` — and
   `restore.ts` feeds those to Zod-validating `create`s. Folders, file rows,
   documents and chunks restore fine, so the result is a graph with all the
   content and **none of the stores or links that reach it: every character vault,
   project store and group store comes back unreachable.**
2. **v4 restores no user file at all.** `getFileFromExtractedBackup` gates the
   `files/<storageKey>` lookup on `backupFormat === 2`; a modern manifest declares
   `4`. One-line fix: `>= 2`.

A faithful v5 port reproduces **neither** (its typed readers coerce), so the
tier-2 state diff is not an equality. That is the same shape as the sparse-array
blob divergence, which a human ruled — and this needs the same ruling. The lane
correctly refused to land either a live-but-unproven restore or a dead one (the
order's own tier-3 rule: "refuse the whole verb or land the whole mode"). **The
orchestrator is written and compiles; it is banked on the lane branch's record,
not on main.**

**Resume list for P4.9G5 unit 4** (also in that order's status header):
1. ~~Get the ruling on the two v4 bugs.~~ **DONE 2026-07-25.**
2. Rework the state differential to diff the pre/post **delta** rather than the
   absolute post-state (the lane's third finding) — which also fixes minted-id
   labelling.
3. Chase two open leads: the 8-vs-0 `doc_mount_chunks` baseline gap, and whether
   `delete_user_data` removes the Quilltap Uploads mount and so makes `replace`
   unable to land any project-less file.
4. Then land units 4 and 5 together.

**Next candidates, in rough value order:**

1. **Finish P4.9G5 units 4–5 — now UNBLOCKED (ruled 2026-07-25).** The
   orchestrator is written and compiles (banked at
   `docs/developer/porting/banked/p4.9g5-unit4/`), the ruling is recorded, and
   `remap_backup_data` is on main waiting for its only caller. This is the last
   piece of Backup & Restore.
2. **`work-orders/p4.9g4-qtap-export-import.md` — resume at import EXECUTE.**
   Fully unblocked, fully disjoint, and the other half of the Data & System
   surface a user can still reach a refusal on.
3. **A dogfood pass** over the now-live restore preview plus the previous round's
   Data & System surfaces, and the standing walk debt (**Part D**, **Part F items
   15/16**).
4. **M6 backlog rows 6+**, then the standing pools (`p4.9i2`, `p4.9e3`, `p4.9h2`,
   the sidebar tier-3 deferrals, `browserUserAgent`, D21).

---

## The pre-compute + Data & System round (P4.19 ∥ P4.9G1 ∥ P4.9G2) — UNIFIED on main 2026-07-24

**P4.19 CLOSED. P4.9G2 CLOSED. P4.9G1 PARTIAL — resume there.** Full round
record (wires, gate, corrections) in `status-log.md`; the planning block that
scoped it follows below.

**Landed.** The chat spine runs v4's proactive pre-compute distill before
buildContext (`services/pre_compute.rs`; the pre-searched head suppresses the
fallback distill), pinned by a new tier-3 `precompute` differential (8 cases)
and two new `build_context_tier3` ops ∥ the Data & System **tasks-queue + jobs
server family** (`api/system_data.rs`, the host `JobPumpControl` seam, v4-parity
REST edges, the committed `system-data-*` fixture, an 18-case differential),
with all sixteen §1 verbs DEFINED and the unlanded ones refusing loudly ∥ the
**whole Data & System SPA tab** (nine cards in v4's order, both backup dialogs,
both 5-step import/export wizards, the delete-all dialog, the LLM log viewer +
character-edit F2 section, and the app-wide auto-lock idle provider).

**The unification wire earned its keep:** the §1 name-for-name diff caught the
three job verbs disagreeing on their id field (`id` server-side vs `jobId`
client-side) — every per-job action in the Tasks Queue card would have failed
to deserialize live. Reconciled toward `jobId`. Also corrected here: P4.19's
`orchestrator_tier3` "BLOCKED" finding did NOT reproduce from main — the oracle
regenerates cleanly (227 rows) and the differential passes, so unit 4c is
CLOSED and no v4-jest infra fix is owed (the lane's blockage was worktree-local).

**Gate:** 372 binaries / 1,560 / 0; four families regenerated fresh at
`e646f58b` and re-run by name zero SKIP; clippy both feature sets; release
build; ng 223 files / 2,621; full Playwright 124 passed / 1 gated skip / 0
failed. Versions: core 0.0.346, harness 0.0.293, host 0.0.33, web 0.0.40,
SPA 0.5.268.

**⚠ The one user-visible gap:** three Data & System cards (Backup & Restore,
Import / Export, Delete All Data) are fully built in the SPA but their server
families are OPEN — a user clicking them gets the loud "recognized but not yet
available" refusal. That is the single largest reason to run P4.9G1's
remainder next.

**Next candidates, in rough value order:**

1. **Finish the Data & System server remainder** — the three-lane round RAN
   and **UNIFIED on main 2026-07-24** (round record at the top of
   `status-log.md`). **P4.9G3 CLOSED**; **P4.9G4** and **P4.9G5** stay OPEN on
   named remainders, and they are the two highest-value next items:
   - **`work-orders/p4.9g5-backup-restore.md` — resume at unit 3**, the whole
     restore side (`parseBackupZip` + `previewRestore` + the octet-stream
     upload leg; `remapBackupData` + `new-account`; then `replace`). Shared
     contract §2 is UNBLOCKED — `services::delete_all::delete_user_data` is on
     main at the pinned signature, so unit 5 just calls it. Landing units 3–5
     deletes the last "recognized but not yet available" arm in `engine.rs`.
     ⚠ Read that order's header first: this round shipped a real bug (missing
     lazily-created tables failed the WHOLE backup) that the fixture-based
     differential could not see — restore's reads must apply the same
     `if_table` rule.
   - **`work-orders/p4.9g4-qtap-export-import.md` — resume at import
     EXECUTE** (`executeImport`: ten id maps, four per-entity importers,
     legacy presets, reconcile, four conflict strategies, the multipart
     `options` part, and the four-strategy DB-state differential).
   Live today from this round: Delete All Data, Create Backup (+ download),
   Export (all ten types), Import through the preview.

2. **A dogfood pass** over this round's live surfaces — the Data & System tab
   (passphrase, auto-lock enforcement, tasks queue, LLM logs) and the
   proactive pre-compute in real chats. Note P4.19's own framing: it is a
   FIDELITY port, so unchanged retrospective bite is NOT a failure. The
   standing walk list also still owes **Part D** (the retrospective downstream
   look) and **Part F items 15/16** (Story's Clock jump; per-chat Core-whisper
   override) from the 2026-07-24 walk.
3. **M6 backlog rows 6+** (`m6-screen-parity.md` §4) — the next unstarted
   screen-parity rows.
4. The standing pools: `p4.9i2` (help/HelpChat, which also holds the banked
   `math-notation.md`), `p4.9e3` (tools SearchReplaceModal), `p4.9h2`, the
   sidebar tier-3 deferrals, `browserUserAgent` threading (still banked on
   ownership), and D21 (release/signing, never yet started).

---

## The pre-compute + Data & System round (P4.19 ∥ P4.9G1 ∥ P4.9G2) — PLANNED 2026-07-24

**The first round after the ruled rewrite→fix→walk sequence closed** (the
2026-07-24 walk ran CLEAN — see the round block above; P4.13/#25/#22/#26/#27
all closed at it). v4 drift-checked clean at `e646f58b` at planning (HEAD
exactly the baseline, tree clean). Scope: M6 backlog row 5 (`p4.9g`, the
Data & System settings tab — the top unstarted row by value) as a
server ∥ SPA pair, plus the one banked Rust-spine item P4.16's disposition
explicitly recommended for scheduling. Three lanes, ownership fully
disjoint (each order carries the binding map; `services/mod.rs` is the one
append-only shared file):

- **P4.19** (`work-orders/p4.19-proactive-precompute-distill.md`) — the
  proactive pre-compute distill port (v4
  `pre-compute.service.ts` `proactiveRecallTask`): the per-character
  `messagesSinceLastSpoke` window, distill + semantic pre-search
  (limit 20 / minImportance 0.3 / cap 10), `BuildContextInput.
  {pre_searched_memories, recall_signals}`, and fallback suppression
  mirroring `context-manager.ts:1141-1145`. A FIDELITY port by P4.16's
  own framing — not expected to raise retrospective bite rate. New
  tier-3 pre-compute differential (`QT_ORACLE_PRECOMPUTE`) +
  `orchestrator_tier3`/build-context extensions. Owns the chat spine;
  bumps core + harness. (The compression half of v4's pre-compute is
  already ported inline — verified 2026-07-24, `orchestrator.rs:
  1673-1712`.)
- **P4.9G1** (`work-orders/p4.9g1-data-system-server.md`) — the Data &
  System server half: `.qtap` NDJSON export/import (both legs), the
  tasks-queue family over the already-complete `db/background_jobs.rs`
  (+ a NEW `EngineAssembly` job-pump-control seam — the host owns
  cadence), delete-all-data (the `DELETE_ALL_MY_DATA` sentinel,
  v4's exact deletion order), and backup/restore (zip staging, the
  single-use 30-min temp store, octet-stream upload, replace +
  new-account-remap modes). Sixteen new dispatch verbs (§1, binding) +
  v4-parity REST edges + byte/stream/multipart web-edge legs; a new
  committed `system-data-{main,mount,llmlogs}.db` fixture family; five
  differential families + a wire-contract pin. Bumps core + harness +
  web + host. Survey correction folded in: passphrase-change, auto-lock
  storage/Lock, and the LLM-logging key ALREADY EXIST server-side —
  G1 does not rebuild them.
- **P4.9G2** (`work-orders/p4.9g2-data-system-spa.md`) — the Data &
  System SPA half: the `system` tab in v4's nine-card order (Plugins
  renders nothing — WON'T-PORT), the passphrase / auto-lock / LLM-logging
  cards over EXISTING verbs, the backup/restore + export/import wizard
  dialogs, the tasks-queue card ("Simultaneous Labours"), the
  LLMLogViewerModal + the character-edit LLM-logs section (M6 F2), the
  delete-all card, and the app-wide **auto-lock idle provider**
  (completing the unlock screen's waiting `AUTOLOCK_RETURN_KEY` loop).
  ACTIVATE-AT-UNIFY beats over G1's verbs. Bumps SPA.

Scope corrections recorded at planning (in `m6-screen-parity.md` under
§2.6, dated 2026-07-24): the capabilities report (Providers tab), the
global search dialog (toolbar), the tools SearchReplaceModal (chat views →
`p4.9e3`), and the API-key export/import dialogs (API Keys tab) do NOT
live on the Data & System tab and are NOT this round's work. Also settled
at planning: **the banked sibling-owned `eprintln!` sweep is ALREADY
SATISFIED** (surveyed 2026-07-24 — zero `eprintln!` in all six named
files; the banked item dies). Left out deliberately: `browserUserAgent`
threading (crosses G1's quilltap-web files — banked until a round where
the ownership fits), the downstream retrospective-whisper look + Part D /
Part F 15-16 walk items (the NEXT dogfood pass's list), the
response-bodies real-capture upgrades, and the other standing seams
(unchanged).

---

## The import-execute + Post Office + chunk-on-write round (P4.9G4-resumed ∥ P4.9E2A ∥ P4.9E2B ∥ P4.6BK) — UNIFIED on main 2026-07-25

**ALL FOUR ORDERS CLOSED.** `.qtap` import works end to end (the Data & System
family is now complete — every card that once refused now works); the in-chat
Post Office is live in the Salon; database-store documents chunk on write, so a
freshly provisioned character vault / project store / group store is searchable
immediately; and **P4.9G5's owed restore e2e beat finally landed and runs
green**. The unification wired `EngineAssembly.announcement_preview` LIVE (⚠ real
spend, one cheap-LLM call per Generate) and removed BOTH §2 chunk tripwires —
E2A's fired on the first merged run, which is the discipline working. Full round
record, including the gate numbers and the one escalation deliberately not taken,
in `status-log.md`.

## The `231be14c` drift catch-up round — UNIFIED on main 2026-07-26 (P4.d18 ∥ P4.d19 ∥ P4.d20 ∥ P4.d21)

**ALL FOUR ORDERS CLOSED. The oracle baseline MOVES to `231be14c` and the v4
drift debt is CLEARED.** v4 had moved `e646f58b` → **`231be14c`**, four commits
in a single day; **none of the four was lib-free**, and together they landed on
two already-ported surfaces — the Story's Clock (P4.9H1) and the whole Pascal /
custom-tools family (P4.6ay / P4.6bb / P4.d8).

**Gate:** 386 binaries / 1,639 tests / 0 failed with zero SKIP lines; 18
differentials re-run by name over oracles regenerated fresh from the pinned
`/tmp/qt-v4-pin-231be14c` worktree; clippy both feature sets; release build;
`ng test` 233 files / 2,883; full Playwright **136 passed / 0 failed / 0
skipped**. Versions: core 0.0.370, harness 0.0.316, host 0.0.39, SPA 0.5.290.
Full round record in `status-log.md`.

**Zero source conflicts across 24 cherry-picked commits** — Ownership held
exactly, and `api/types.rs` was never opened. Both ACTIVATE-AT-UNIFY markers
self-activated. The wire closed two seams the lanes had predicted: the SPA's
three older `z.record` sites followed the server to `expected record` (P4.d20
had deliberately held off so as not to put the browser at odds with it), and the
corpus census — which caught P4.d19's new third row kind at 205-vs-236 — grew
into a full **replay** of those 31 gate verdicts through the browser's own
`tool-gate.ts`.

**Three pre-existing v5 bugs fixed on the way past** (none is drift; all three
user-visible): the `datetime-local` fictional base parsing to `0`; a sub-minute
LMT offset truncated in `timezone_offset_string` — unpredicted, caught by the
widened corpus; and `z.record` reporting `expected object` at four sites plus an
erased `run_custom` vault-failure sentence.

**⚠ One pre-existing divergence found and deliberately NOT fixed:** v4 re-parses
`chats.timestampConfig` through `TimestampConfigSchema` at the repository write
(schema key order, materialized defaults, unknown keys stripped, bad values
400'd) where v5 persists the request JSON verbatim; the chat-UPDATE path shares
it. Until ported, a partial timestamp config saved from the SPA lands in the DB
missing v4's defaults.

The original planning notes follow.

| v4 commit | what it is | lane |
| --- | --- | --- |
| `e3a9654f` | fictional story clocks frozen + base read in the wrong timezone. Rewrites `lib/chat/timestamp-utils.ts` (+98), adds migration `anchor-fictional-clock-base-v1`, changes the chats route + `TimestampConfigCard` | **P4.d18** |
| `faab6881` | the custom-tools popup becomes a two-phase dialog; new `lib/pascal/tool-vocabulary.ts`; `references` on the chat custom-tools listing; `CustomToolParamsForm` gains a stacked layout | **P4.d19** (server) ∥ **P4.d21** (SPA) |
| `6864bf0e` | `availableWhen`/`withheldWhen` availability gates; new `lib/pascal/{tool-gate,metadata-match}.ts`; roster enforcement; the run-custom handler's fact-sheet read reordered; `gate` on both Workbench surfaces; the whole Workbench gate editor | **P4.d19** (server) ∥ **P4.d20** (SPA) |
| `231be14c` | the Salon roll announcement wears the outcome's own state | **P4.d21** |

**Two findings the planning survey turned up, both pre-existing v5 gaps the
round closes in passing:**

1. **v5's fictional clock is worse than v4's pre-fix bug.**
   `chat_timestamp::parse_date_ms` delegates to `clock::iso_to_ms`, which
   requires a trailing `Z` and full `HH:MM:SS` — so a real `datetime-local`
   fictional base (`"1550-07-25T10:15"`) parses to **0**. The differential never
   caught it because the corpus keeps every base in the `…Z` shape; the port's
   own comment says exactly that. **Widening that corpus is a P4.d18 tier-1
   deliverable**, and it is the same blind-spot class as P4.11's one-mode
   request corpus.
2. **v5 has no `.qt-pascal-result` CSS at all.** The Workbench's proving bench
   and outcomes section already apply the class; nothing styles it. v4's new
   compound selectors "only restate an accent declared earlier in this file" —
   false in v5, so **P4.d21 ports the base block first**, giving the Workbench
   its accent for the first time.

Orders: `work-orders/p4.d18-fictional-story-clock-drift.md`,
`p4.d19-pascal-gate-vocabulary-server.md`, `p4.d20-workbench-gate-spa.md`,
`p4.d21-inchat-pascal-spa.md`. **P4.d19 is the critical path** — both SPA lanes
consume contracts it emits (§1 `references`, §2 `gate`, §3 the definition
corpus, all pinned verbatim in all four orders). `api/types.rs` is FROZEN for
the round; both new response fields ride inside `serde_json::Value` bodies. All
four lanes regenerate oracles from a **pinned detached worktree at `231be14c`**
(recipe: `oracle-regen-pinned-v4-worktree`), and **the committed baseline moves
to `231be14c` at unification**.

**Next candidates, in rough value order** (the drift catch-up that used to head
this list is DONE — no v4 drift debt remains as of 2026-07-26):

1. **A dogfood pass — now the clear top item**, because two rounds' worth of
   live surfaces have piled up behind it:
   - **From this round:** the Story's Clock actually advancing (walk item F15
     was already owed and is now finally testable — a fictional-time chat whose
     base was entered through the date-and-time picker), the boot-repair
     backfill on a real instance that has unanchored chats, a gated custom tool
     withheld from one character and dealt to another, and the two-phase run
     dialog with its reference panel.
   - **Still owed from the Post Office round:** `.qtap` import execute on real
     data, the three Post Office dialogs (**the announcement rewrite costs real
     money**), and search over a freshly created character's vault.
   - **Still owed from the 2026-07-24 walk:** **Part D** (the retrospective
     downstream look) and **Part F item 16** (per-chat Core-whisper override).
2. **The `TimestampConfigSchema` write-path normalization** — the divergence
   P4.d18 found and recorded: v4 re-parses `chats.timestampConfig` through Zod
   at the repository write (and at chat UPDATE); v5 stores the request JSON
   verbatim, so a partial config saved from the SPA lands missing v4's defaults
   and a bad value is persisted where v4 would 400. Small, well-scoped, and it
   has a probe-verified spec in the P4.d18 unit-2 lane record.
2. **The `chatRng` server verb + the RNG gutter dropdown.** P4.9E2B found the
   gap: P4.d5 ported the rng TOOL, not v4's `POST /chats/{id}?action=rng` route,
   so v5's dispatch surface has no `chatRng` and the gutter's dice button cannot
   be built. Small server lane + a rider; it closes the last of v4's gutter row.
3. **M6 backlog rows 8 and 10** (`m6-screen-parity.md` §4) — `p4.9e1` (the chat
   cast dialogs: AddCharacter + CreateNPC + SummonFromLore, needs tier-3 LLM
   services for Summon) and `p4.9e3` (the `ChatModals` barrel remainder, a round
   in itself, needs `?action=update-tool-settings`). Row 9 is now DONE.
4. **The blob `originalFileName` type widening** — recorded at this round's
   unification, deliberately not taken: no behavior change, ~a dozen
   differential-free construction sites. Wants a round that owns
   `db/doc_mount_file_links.rs`.
5. The standing pools: `p4.9i2` (help/HelpChat, which also holds the banked
   `math-notation.md`), `p4.9h2`, the sidebar tier-3 deferrals, the
   `chat_settings` explicit-`null` gap (still tripwired), `browserUserAgent`
   threading, and D21 (release/signing, never yet started).

---

### The round as planned (2026-07-25), for the record

v4 drift-checked clean at `e646f58b` at planning (HEAD exactly the baseline,
tree clean — `git log e646f58b..HEAD --oneline` empty). Four lanes, ownership
fully disjoint; `api/engine.rs` and two `mod.rs` files are the only shared
source files and are append-only per labelled region.

Scope is set by what the handoff sources actually say is owed: the **one**
remaining OPEN order remainder on main (G4's import execute), the highest-value
unstarted M6 screen row whose dependencies are all landed (row 9, the in-chat
Post Office dialogs — its post-office writers shipped in W4.6b), and the
higher-impact of the two v5 gaps P4.9G5 recorded with tripwires.

- **P4.9G4-resumed** (`work-orders/p4.9g4-qtap-export-import.md`, resumed
  against its own header + a new "Round 2" section) — `executeImport`: the
  ten-map orchestrator and its numbered dependency order, the four per-entity
  importers, the legacy-preset fold, the reconcile pass, all four conflict
  strategies + the route's `'replace'` → `'overwrite'` remap, the multipart
  `options` part, and the four-strategy DB-state differential. **The last
  unported Data & System half** — the SPA's Import wizard refuses by name today.
  Bumps core + harness + web.
- **P4.9E2A** (`work-orders/p4.9e2a-post-office-server.md`) — the in-chat Post
  Office server surface: the unported `lib/services/announcer/**` (`writer.ts` +
  `character-voiced.ts`, 310 lines), four dispatch verbs (announcement, the
  character-voiced preview, send-mail, mailbox list), a new committed
  `post-office-{main,mount}.db` fixture, a tier-2 route differential and a
  tier-3 mocked-LLM differential. **Unblocks the banked `979aec66` drift**
  (Pascal in Insert Announcement), which was NO-PORT-NOW only because this
  surface was unported. Owns `api/types.rs` this round. Bumps core + harness.
- **P4.9E2B** (`work-orders/p4.9e2b-post-office-spa.md`) — the in-chat Post
  Office SPA: Insert Announcement (with the preview→approve/edit/regenerate
  loop), Compose Mail, Whisper, the announcement/mail/RNG gutter buttons and
  composer drag-and-drop — closing the deferral `chat-composer.ts:46` names by
  hand. **The round's only `apps/web` toucher and only Playwright gate, so it
  also carries the owed P4.9G5 restore e2e beat** (upload → preview → restore),
  which three consecutive lanes have deferred for want of exactly that gate.
  Bumps SPA.
- **P4.6BK** (`work-orders/p4.6bk-chunk-on-write.md`) — chunk-on-write: v5 never
  chunks a database-backed document as it is written where v4 always does
  (v4 `lib/mount-index/database-store.ts:133-155`), so every freshly provisioned
  character vault, project store and group store is semantically unsearchable
  until something reindexes it. The gap survived because the **oracle side was
  pinned with `QUILLTAP_JOB_CHILD=1`** — 38 sites at planning — so both sides
  compared chunk-free. The lane closes the gap at both v5 write sites, un-pins
  the oracle cases (fixture builders stay pinned, so no committed `.db` moves),
  adds chunk coverage where none existed, and removes the `KNOWN_V5_GAPS`
  tripwire in `system_restore_state.rs`. Owns the round's §2 tripwire discipline.
  Bumps core + harness.

**Left out of this round deliberately:**

- **The `chat_settings` explicit-`null` gap** (the second P4.9G5 tripwire —
  v5 writes `null` where Zod omits absent `.nullable().optional()` keys). It
  needs `Option<Option<String>>` across the settings bags, which ripples through
  every consumer, and G4 is actively adding a new `ChatSettingsCreate`
  consumer. It is tripwired, so it cannot rot; it wants a round where nothing
  else touches settings.
- **The owed dogfood walk items** — Part D (the retrospective downstream look)
  and Part F items 15/16 (Story's Clock jump; per-chat Core-whisper override),
  plus a pass over the Data & System surfaces. A walk after this round covers
  more ground: restore, import execute and the Post Office all land into it.
- **M6 rows 8 and 10** (`p4.9e1` cast dialogs, `p4.9e3` chat-admin dialogs).
  Row 8 depends on tier-3 LLM services for Summon; row 10 is a round in itself
  and needs `?action=update-tool-settings`. Row 9 was chosen because every
  dependency is already landed.

---

### The round as planned (2026-07-26) — the chat action remainder

**Drift check at planning — and the revision it forced.** Planning began with v4
at `41f34180`, two commits past `231be14c`, both docs-only, tree DIRTY in
`lib/backup/restore/`. **Mid-planning v4 shipped the fixes**, and the round was
re-planned against them. v4 is now at **`c1507f47`, tree clean**, four commits
past the old baseline:

| Commit | What | Debt |
| --- | --- | --- |
| `20430561` | release notes | docs-only, none |
| `41f34180` | `docs/developer/found-bugs.md` (new) | docs-only, none |
| **`67ffb444`** | `fix(backup): restore brings back the stores, the links, and the files` — bugs 1–3 | **real `lib/` drift** |
| **`c1507f47`** | `fix(import): the blob reader waits for every chunk before it signs` — bug 4 | **real `lib/` drift** |

**The drift is tightly bounded and none of it reaches the chat surface.** It
touches `lib/backup/restore/{archive,restore,mount-index-coercion}.ts`,
`lib/import/quilltap-import-stream.ts`, and `lib/export/ndjson-writer.ts`
(**comments only** — the backup writer is untouched by design, so committed
archive fixtures do not move). Verified by import at planning: **six** oracle
case files reference those paths and **three** import a file whose behavior
changed (`system-restore`, `system-import`, `system-import-execute`);
**zero chat families** import any of them. The `4.8.0-dev.103 → .107` version
bump is inert — `appVersion` is normalized in both manifest-bearing
differentials.

So the round gained a **fourth lane** rather than changing the other three:
**P4.d22** converges v5 onto v4's fixes, retires the two tripwires, and **owns
the baseline move `231be14c` → `c1507f47`**. The three chat lanes regenerate
their own families straight from the clean checkout at `c1507f47` (the P4.6bj
precedent) and touch no baseline paragraph.

This is also the moment v4 asked for: its `found-bugs.md` says to land the fixes
"at a point where the v5 side is between rounds, so the baseline moves once."

**Why this scope.** Every work order on main is CLOSED, so the round is new
scope. The survey found a hole rather than a refinement: **v5 can create a chat
with a cast and then never change it.** There is no `ChatAddParticipant`,
`ChatUpdateParticipant` or `ChatRemoveParticipant` variant anywhere in
`api/types.rs`, and **nineteen** of v4's chat POST actions have no v5 verb.
This is also the server dependency under M6 rows 8 and 10.

Three lanes; ownership disjoint. `api/types.rs`, `api/engine.rs` and
`services/mod.rs` are the only shared source files and are append-only per
labelled region. `apps/web/**` belongs to one lane outright.

- **P4.9E1A** (`work-orders/p4.9e1a-chat-cast-avatars-server.md`) — the cast +
  avatar-override server surface: `add`/`update`/`remove-participant`,
  `rebuild-system-prompt`, `get`/`set`/`remove-avatar`,
  `toggle-avatar-generation`, **and the chat-PUT bag's participant families**,
  which `api/salon.rs`'s `chat_update` names as a deferral in so many words —
  v4 has two entrances and one implementation must serve both. New
  `chat-cast-{main,mount}.db` fixture family. Bumps harness.
- **P4.9E3A** (`work-orders/p4.9e3a-chat-admin-tools-server.md`) — the
  chat-admin + tools server surface: eleven verbs (`regenerate-title`, tags,
  `bulk-reattribute`, `merge-conversation`, `update-tool-settings`, `run-tool`,
  `rng`, `toggle-agent-mode`, `reclassify-danger`, `render-conversation`).
  `applyChatMerge` is the one genuinely unported subsystem; `rng` and the tool
  executor are already ported, so most of the rest is wiring over proven parts.
  New `chat-admin-{main,mount}.db` fixture family. **Bumps core** (the single
  bumper for both server lanes).
- **P4.9E1B** (`work-orders/p4.9e1b-chat-cast-dialogs-spa.md`) — the SPA: Add
  Character, Create NPC, participant edit/remove/rebuild, the RNG gutter tool
  (into the empty slot `chat-composer.ts:192` already documents), the chat
  tool-settings modal, and the avatar overrides at tier 2. Owns all of
  `apps/web/**`.
- **P4.d22** (`work-orders/p4.d22-restore-import-convergence.md`) — the
  restore/import convergence. **LANE COMPLETE; tier-1 item 3 OPEN on a human
  ruling and tier-1 item 4 named as an unexercised gap.** Both tripwires fired on
  the first oracle regenerated at `c1507f47`, and all five carve-out entries
  (three `EXPECTED_DIVERGENCES`, two `DIVERGENCE_DEPENDENTS`) plus
  `throw_ndjson_truncated_blob` are retired. Insisting on proof rather than v4's
  claim paid three times over:
  - **Bug 1 converged, and diffing the ROWS instead of counting them found a
    matching v5 gap the count-level pin had hidden** — restored stores came back
    with EMPTY pattern arrays, and an INTEGER `0` policy flag would have read as
    `true` (a store the user disabled, or a read-only document, coming back
    permissive). Ported v4's new coercion module with its own 20-case tier-1
    family covering the five arms no committed archive can reach.
  - **v4's status-table claim "files run after 22a on both sides" is true about
    ordering but does not settle the case it was written for.** In `replace` mode
    NEITHER engine restores a user file over the committed archives — they carry
    no Quilltap Uploads mount, so the `instance_settings` pointer dangles on both
    sides. That is a fixture characteristic, not a bug, but it means the family
    **cannot exercise the disaster-recovery case the fix targets**. Only
    `new-account` restores a file, and there the two placements write the SAME
    ROWS with the SAME VALUES in a DIFFERENT INSERTION ORDER →
    `PHASE_ORDER_RESIDUAL`, asserted in both directions, **awaiting a ruling**
    (the lane recommends adopting v4's `22a-bis`, which also closes a latent
    `UNIQUE(fileId)` hazard v4 documents).
  - **The second-generation residual is NOT exercised** — the family has no
    such archive, so v5's behaviour there is analysis, not measurement. By
    inspection v5 cannot reproduce it (its later phase makes the replay
    unique-suffix instead of collide), but that is deliberately left unbuilt:
    the residual's shape follows from the placement under ruling. Build it as
    the ruling's follow-up.

  Also landed: `summary.warnings` under diff for the first time; two
  normalization rules the retirement forced (colon-separated minted ids in every
  `storageKey`; a content hash living one table from its content). Two v5
  findings recorded and NOT fixed, neither restore's: the unported `refreshStats`
  (`V5_STATS_GAP`) and `DbError::Key`'s Display prefix leaking "key derivation
  failed:" into ~20 user-visible messages. Regenerated eight families; moved the
  baseline.

**Version bumps:** each lane bumps what it touches; the unifier recounts as
`base + total bumps` (concurrent bumps to one crate are normal and a clean
cherry-pick is not evidence they survived).

**Three survey findings the orders carry so no lane re-derives them.**

1. **`updateParticipantSchema` is three-valued on four fields.**
   `imageProfileId`, `selectedSystemPromptId`, `joinScenario` and
   `talkativeness` are `.nullish()`, and `helpers.ts:159-160,180` branches on
   `!== undefined` — absent and explicit `null` take different paths. The wire
   needs **`Option<Option<T>>`**. Getting it wrong would repeat the
   `chat_settings` explicit-`null` gap that is still tripwired.
2. **SummonFromLore is a tier-3 deferral with a concrete reason.**
   `SummonFromLoreModal.tsx` is 84 lines wrapping
   `components/settings/ai-import/AIImportWizard` — **703 lines, unported**.
   Porting Summon means porting Aurora's AI-import wizard, a round of its own.
3. **`ChatRng` renames v4's `type` key to `kind`**, because `type` is the
   `Request` enum's own serde tag. Pinned in §1 as the contract.

**Left out of this round deliberately:**

- **The `TimestampConfigSchema` write-path normalization** (P4.d18's recorded
  divergence, probe-verified spec in its unit-2 lane record). It straddles the
  two server lanes' file seam — the repository half is in `db/chats.rs` (E3A's),
  the UPDATE half in `api/salon.rs`'s `chat_update` (E1A's, which deliberately
  bypasses `db/chats.rs`). Forcing it into one lane would land two
  implementations that drift. It wants a round owning both files.
- **The `p4.9e3` dialog family's UI** — Merge, Reattribute, BulkReplace,
  RunTool, SearchReplace, AllLLMPause, SelectLLMProfile, LibraryFilePicker,
  ChatRename, ChatProject. Their **server** half lands in P4.9E3A; the screens
  are a round of their own (~3,000 LOC of v4 UI).
- ~~The backup/restore/import surface~~ — **no longer left out**: v4's fixes
  landed mid-planning and the round gained P4.d22 to converge onto them.
- **The owed dogfood walk** — Part D (the retrospective downstream look) and
  Part F items 15/16 (Story's Clock jump; per-chat Core-whisper override), plus
  the Post Office dialogs, `.qtap` import execute and restore on real data.
  Phase-4's own candidate list has ranked this **first** for two rounds running.
  It does not conflict with any lane here and **can be run by the human in
  parallel with this round.**


---

### Round outcome (2026-07-26) — the chat action remainder, UNIFIED

**All four lanes CLOSED.** The oracle baseline is now **`c1507f47`** and there is
no v4 drift debt. Full record in `status-log.md` under "Round record — the chat
action remainder"; gate numbers and deferrals in CLAUDE.md's Status bullet.

The round's headline: **v5 can change a conversation's cast**, which it had never
been able to do. Both of v4's entrances (the `?action=` verbs and the chat-PUT
bag) are closed and share one implementation.

**Next candidates, in rough value order:**

1. **⚠ THE RESTORE FILES-PHASE RULING — a human decision, and it blocks nothing
   else but wants answering while it is fresh.** v4 runs its restore files phase
   at `22a-bis`; v5 runs it after the whole doc-store family. Both write the same
   rows with the same values into the same mount at the same path — only the
   INSERTION ORDER differs — so it is `PHASE_ORDER_RESIDUAL` in
   `system_restore_state.rs`, asserted in both directions. v4 documents why a
   later slot is worse (the replay hard-links an archived content row, so 22f's
   blob insert violates `UNIQUE(fileId)` and refuses the ARCHIVED blob); **v5
   sits in exactly that slot**, a latent hazard no committed archive triggers.
   **P4.d22 recommends adopting `22a-bis`.** Its follow-up is building the
   second-generation archive the family lacks, so item 4 stops being analysis and
   becomes measurement.
2. **A dogfood pass — now badly overdue, and this round adds a lot to it.** The
   cast surface end to end on real data (add / remove / hand a character to the
   human / change who answers for one / rebuild a system prompt), Regenerate
   Title (**⚠ real spend**), the RNG gutter, merge-conversation,
   bulk-reattribute, and restore. **Still owed from earlier walks:** Part D (the
   retrospective downstream look) and Part F items 15/16 (Story's Clock jump;
   per-chat Core-whisper override).
3. **`p4.9e3` — the ChatModals dialog family.** Its SERVER half landed this
   round (eleven verbs, all differential-proven), so the remaining work is UI
   over a frozen surface: Merge, Reattribute, BulkReplace, RunTool,
   SearchReplace, AllLLMPause, SelectLLMProfile, LibraryFilePicker, ChatRename,
   ChatProject. **It must carry `GET /api/v1/tools` with it** — 727 LOC + the
   plugin registry, the reason `ChatToolSettingsModal` refuses by name today.
4. **The two `llm_choose` refusals** — add-participant and merge-conversation
   both need a cheap-LLM host seam the single-writer closure cannot host. One
   driver on the `ChatCreateDriver` pattern closes both.
5. **The `TimestampConfigSchema` write-path normalization** — deferred twice
   now because it straddles `db/chats.rs` and `api/salon.rs`'s `chat_update`.
   It wants a round that owns both files. Probe-verified spec in P4.d18's unit-2
   lane record.
6. The standing pools: the two v5 findings P4.d22 recorded (`V5_STATS_GAP`; the
   `DbError::Key` message-prefix leak into ~20 user-visible strings), `p4.9i2`
   (help/HelpChat), `p4.9h2`, the `chat_settings` explicit-`null` gap (still
   tripwired), `browserUserAgent`, and D21 (release/signing, never started).

---

### Round outcome (2026-07-27) — the embedding repair + chat-dialog family, UNIFIED

**All four lanes CLOSED** (P4.6BL ∥ P4.9E3B ∥ P4.9E3C ∥ P4.D24). The oracle
baseline is now **`e8a49597`** (v4 HEAD, 4.8.0-dev.108) and there is no v4 drift
debt. Full record in `status-log.md` under "Round record — the embedding repair +
chat-dialog family round"; gate numbers and deferrals in CLAUDE.md's Status
bullet.

**The round's headline: newly written text is searchable again.** The
EMBEDDING_GENERATE handler had never been ported, so every embed job v5 minted
died after three retries — 2,088 DEAD rows on the Friday copy and every chunk
written since v5 took over the instance unembedded. The worker is now live in the
production spine and the backlog heals on boot.

**Also closed by this round, from the list above:** item 3 (`p4.9e3` — the whole
ChatModals dialog family, server *and* UI, including the 727-LOC
`GET /api/v1/tools` inventory it had to carry), item 4 (both `llm_choose`
refusals, closed by one host driver), and item 5 (the `TimestampConfigSchema`
write normalization, deferred twice before this). Item 1's ruling was discharged
by `p4.d23` a day earlier.

**Next candidates, in rough value order:**

1. **A dogfood pass — now the clear top item, and this round makes it urgent
   rather than merely overdue.** Two things on main have never been exercised
   against real data and one of them is a repair: the **embedding worker's live
   proof** (the boot repair observed draining Friday's 2,088-row backlog; fresh
   chunks embedding; semantic search over new material finding it) — the e2e
   instance has no API keys by design, so a walk is the only way to see it — and
   the **whole chat-dialog family** (Rename with automatic naming ⚠ real spend,
   Merge, bulk and per-message reattribution, Run Tool, the tool cabinet, Search
   & Replace, Export, Chat Project, Select LLM Profile). **Still owed from
   earlier walks:** Part D (the retrospective downstream look) and Part F items
   15/16 (Story's Clock jump; per-chat Core-whisper override), plus the Post
   Office dialogs, `.qtap` import execute and restore on real data.
2. **`LibraryFilePickerModal`** — the one dialog P4.9E3C deferred by name, and
   the largest remaining hole in the chat surface: 616 LOC over six endpoints,
   one of which (`files?action=attach-mount-file`) is P4.9E3B's own deferral and
   needs the vision-LLM describe seam. Its `?action=group-stores` read is
   ALREADY on main and unconsumed (`ChatGroupStores`, mirrored into
   `core-contract.ts` at this round's wire) — so that much of its server half is
   free. Its own round, server + SPA together.
3. **The embedding remainder** — `EMBEDDING_REINDEX_ALL`, `chatQueueMemories`,
   and the startup-reconcile port (blocked on the unported CONVERSATION_RENDER
   handler; the boot repair pass is the sanctioned v5-only stand-in until then).
4. **Two v4-side items this round surfaced, for the human to carry upstream**
   (neither is v5 work): **stop-impersonate is unreachable from v4's own
   client** — it sends `DELETE ?action=stop-impersonate`, the action is
   registered only on the POST map, and DELETE hard-rejects unknown actions;
   v5's single-verb model is already correct. And **`AllLLMPauseModal` is
   unreachable in v4 itself** (`setAllLLMPauseModalOpen(true)` appears nowhere at
   `e8a49597`) — it wants either an opener or deleting. v5 deferred the dialog
   with the evidence rather than ship something nothing can open.
5. **M6 rows 6+** — the screen-parity backlog beyond the dialog family;
   `ProjectToolSettingsModal` (shares `ToolSettingsContent` with the modal that
   landed this round) is a cheap Prospero rider.
6. The standing pools: the two v5 findings P4.d22 recorded (`V5_STATS_GAP`; the
   `DbError::Key` message-prefix leak into ~20 user-visible strings), `p4.9i2`
   (help/HelpChat — the `help/custom-tools.md` drift from `e8a49597` joins this
   bank), `p4.9h2`, the `chat_settings` explicit-`null` gap (still tripwired),
   `browserUserAgent`, and D21 (release/signing, never started).

### Round outcome (2026-07-27, second round of the day) — the library picker + embedding remainder, UNIFIED

**All three lanes CLOSED** (P4.9E4A ∥ P4.9E4B ∥ P4.6BM). The oracle baseline
stays **`e8a49597`** (v4 did not move during the round); no drift debt. Full
records in `status-log.md` ("Lane record — P4.9E4A", "Lane P4.9E4B", "Lane
record — P4.6BM" units 1–7 + closing summary, and the round record); gate
numbers in CLAUDE.md's Status bullet.

**The round's headline: the last refusal arms in two surfaces are gone.** The
composer can attach a document-store file (the Librarian announces it, the
three-rung description ladder describes it — ⚠ one vision-LLM call per
genuinely unknown image, live in the production spine), and the embedding
family is complete: `CONVERSATION_RENDER` and `EMBEDDING_REINDEX_ALL` have
handlers (both had been minting dead jobs from live callers — the manual
render button and every BUILTIN refit), the startup reconcile replaces the
P4.6BL boot stand-in (retired), and `chatQueueMemories` was the surface's
last refusal. The picker dialog landed with all six gutter tools present,
plus the project Default Tool Settings dialog (rider A), the RNG residuals
(rider B), and the `allowToolUse` disposition (rider C — dead code in v4
itself, recorded as a v4-side item).

**Next candidates, in rough value order:**

1. **The dogfood pass — still the top item, now with more owed to it.** Never
   exercised on real data: the embedding worker (P4.6BL's live proof — boot
   repair draining Friday's backlog, now via the reconcile), the whole
   chat-dialog family (P4.9E3C), the library picker + attach flow (this
   round; the vision describe costs real money), the render/reindex handlers
   on a real library, and the still-owed walk Parts D (retrospective recall)
   and F (Data & System, destructive, scratch copy) and H.
2. **M6 rows 6+** — `p4.9h` (prompt library + Core Whisper chain + the memory
   cards + embedding-profiles management, for which P4.6BM's reindex handler
   is now ready), `p4.9i2` (help/HelpChat), `p4.9k` (character AI dialogs),
   `p4.9n` (files fidelity), `p4.9l` (composer toolbar), `p4.9m` (toast bus).
3. **The standing pools:** the `DbError::Key` message-prefix leak (237
   construction sites — wants a quiet solo lane), `V5_STATS_GAP` (tripwired),
   the `chat_settings` explicit-`null` gap (tripwired), `browserUserAgent`,
   `p4.9h2`, D21 (release/signing, never started), and the v4-side list in
   `dogfood-findings.md` (stop-impersonate DELETE, `AllLLMPauseModal`, the
   dead `allowToolUse` warning box).

### Round outcome (2026-07-28) — the P4.D25 embedding-warmth drift catch-up, UNIFIED

**P4.D25 CLOSED** (single lane; order
`work-orders/p4.d25-embedding-warmth-drift.md`). The oracle baseline MOVES to
**`083fdf68`** (v4 HEAD, 2026-07-28) and the drift debt is CLEARED. Full lane
records + the round record in `status-log.md`; gate numbers in CLAUDE.md's
Status bullet.

**The round's headline: v5 stops burning money on every boot.** v4's Bugs 6 + 7
fixes are mirrored — the boot reconcile now runs the shared `is_stale` gate
(cold-tiered chats skip; unknown staleness skips, never heals) and excludes
chunks already FAILED for the profile a re-embed would use; the cache sweep only
clears embeddings older than the retention cutoff, so a reopen re-embed
survives; and the `mark_as_embedded`/`mark_as_failed` upserts mean outcomes
actually land for entities that never got a status row. Two deviations from the
order stand, both recorded: the `&Db` re-signature was rejected (deadlock inside
`write_blocking`; `_conn` twins instead), and the reconcile's two
sentinel/fail-soft arms are unit-tested rather than differential-covered
(reaching them would empty `embedding_profiles` mid-corpus).

**Next candidates, in rough value order** (unchanged from the previous round,
with this round's live proof added to item 1):

1. **The dogfood pass — still the top item.** Everything the previous round
   owed (the embedding worker, the chat-dialog family, the picker/attach flow,
   walk Parts D/F/H) plus this round's: a boot against the Friday copy that
   does NOT mass re-embed (watch `skipped_stale` in the boot log), and a
   read-then-sweep cycle that keeps the reopened chat's vectors.
2. **M6 rows 6+** — `p4.9h` (prompt library + Core Whisper chain + the memory
   cards + embedding-profiles management), `p4.9i2` (help/HelpChat —
   `help/data-retention.md` joins that bank's sync-at-runtime set), `p4.9k`,
   `p4.9n`, `p4.9l`, `p4.9m`.
3. **The standing pools:** the `DbError::Key` message-prefix leak,
   `V5_STATS_GAP` (tripwired), the `chat_settings` explicit-`null` gap
   (tripwired), `browserUserAgent`, `p4.9h2`, D21, and the v4-side list in
   `dogfood-findings.md`.

### Round outcome (2026-07-30) — the `5cc76688` drift catch-up, UNIFIED

**P4.d26 ∥ P4.d27 ∥ P4.d28 ALL CLOSED** (orders
`work-orders/p4.d26-day-references-fresh-boost.md` /
`p4.d27-embedding-dimension-reconcile.md` /
`p4.d28-export-markdown-transcript.md`). The oracle baseline MOVES to
**`5cc76688`** (v4 HEAD, 2026-07-30) and the drift debt is CLEARED — the
fourth drift commit is the NO-PORT jobs-child proxy fix. Full round record
in `status-log.md`; gate numbers in CLAUDE.md's Status bullet.

**The round's headline: v5 reads the calendar the way the user does, keeps
one embedding standard, and exports a readable transcript.** Same-day
references ("the mission today") now reach recall against the SERVER-LOCAL
calendar with a fresh-event boost and echo guard; the boot dimension
reconcile converges every stored vector on the default profile's dimension
(the v4 outage class where a TF-IDF corpus survived under a neural default);
and a chat's Organize drawer can hand over a deterministic Markdown
transcript. The §3 review fixed a PRE-EXISTING mirrored-timezone rendering
bug in the host offset seam and found v4's mount-chunk reconcile count to be
dead code (reproduced behind a tripwire; the v4-side one-liner queued
post-5.0).

**Next candidates, in rough value order:**

1. **The dogfood pass** — now owing this round's live proofs (the dimension
   reconcile's first boot on the Friday copy: non-zero `mismatched_memories`
   and exactly ONE `mismatched-dim` reindex, second boot
   `reindex_enqueued=false`; a same-day recall walk on a Chicago host; an
   Export Markdown download from a rich chat) plus everything previously
   owed (walk Parts D/F items, the P4.6BM embedding-worker live proof
   already banked).
2. **The enclave-step pre-compute divergence — a dedicated follow-up order.**
   `enclave_step_tier3_equivalence` is a STANDING RED (pre-existing, P4.19):
   v5 runs the proactive pre-compute distill with a 1-message window where
   v4's `proactiveRecallTask` bails, then also runs the fallback distill —
   one extra cheap-LLM call + one error `llm_logs` row per affected
   autonomous turn (real money). Diagnosis notes in the P4.d26 unit-5 lane
   record; the fix needs its own differential case.
3. **Dogfood #37 — the image-attachment wire order** (unchanged; the
   strongest feature-sized candidate: no provider builder emits image parts,
   so vision calls go out text-only; first step is scoping whether in-chat
   vision is affected).
4. **The top page-toolbar lane** (#38: autonomous badges + queue badges +
   search + width toggle), `p4.9h` (prompt library + embedding-profiles
   management — now carrying the banked PUT trigger matrix and its
   `EMBEDDING_REAPPLY_PROFILE` handler dependency), `p4.9i2`, `p4.9o`, and
   the standing pools.

⚠ **Watch item:** v4's tree is DIRTY with in-flight store-overlay work
(`document-store-overlay.ts`, `backfill-{group,project}-stores.ts`) — the
next drift is brewing on a PORTED surface. Drift-check before the next
round; regenerate oracles from a pinned worktree until it lands and is
absorbed.
