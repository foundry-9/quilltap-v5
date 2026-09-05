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

  **Amended 2026-08-18 — the one release-adjacent piece that is scheduled,
  not deferred indefinitely.** D21 defers release work; it does not defer
  *knowing what version this is*. v5 has no product version at all today,
  which is why the About badge carries a recorded divergence, why one build
  reports four different numbers (host / web / tauri / cli each answer with
  their own `CARGO_PKG_VERSION`), and why `docs/CHANGELOG.md` runs ~19,400
  lines under a single flat heading. **Human ruling (2026-08-18): the first
  real release is `5.0.0`, the counter until then is the semver prerelease
  `5.0.0-dev.N`** (the shape v4 already prints), one canonical string with
  *derived* platform projections — never a hand-maintained parallel number
  per platform. Ordered as
  [`work-orders/pb1-product-version-manifest.md`](./work-orders/pb1-product-version-manifest.md),
  the first **PB** ("pre-beta") order, to run when parity is winding down
  and **before the first build a beta tester installs** — see the standing
  pre-beta gate at the tail of this file. Everything else under D21
  (signing, publishing, the updater, multi-arch Docker, cross-platform CI)
  stays deferred.
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
2. **The enclave-step pre-compute divergence** — ~~a dedicated follow-up
   order~~ **DONE (P4.20, 2026-07-30): the planning hypothesis was REFUTED
   — the red was a stale ORACLE mock (a W4.11a-era stub P4.19 retired in
   one file and missed in the enclave-step case), not a v5 divergence, and
   no production money was ever being spent. The family is back in the
   normal green gate; the precompute family now diffs the distill prompt.**
3. **Dogfood #37 — the image-attachment wire order** — **DONE (P4.21,
   2026-07-30): in-chat vision WAS affected; attachments now reach the
   wire on every completion path across all nine providers, corpus-pinned
   (146 envelopes). 💸 The live proof rides the next dogfood pass.**
4. **The top page-toolbar lane (#38)** — **DONE (P4.9P, 2026-07-30): the
   toolbar + `uiSearch` vertical landed; the sidebar-footer stopgap is
   retired.**

### Round outcome (2026-07-30) — the drift + standing-red + dogfood round, UNIFIED

**P4.D29 ∥ P4.20 ∥ P4.21 ∥ P4.9P ALL CLOSED** (orders
`work-orders/p4.d29-store-overlay-read-hardening.md` /
`p4.20-enclave-precompute-window.md` / `p4.21-image-attachments-wire.md` /
`p4.9p-page-toolbar.md`). The oracle baseline MOVES to **`dcd9440a`**;
dogfood findings #37 and #38 are FIXED. Full round record in
`status-log.md`; gate numbers in CLAUDE.md's Status bullet.

### Round outcome (2026-07-31) — the `ff12f491` drift catch-up round, UNIFIED

**P4.D30 ∥ P4.D31 ∥ P4.D32 ∥ P4.D33 ∥ P4.D34 ALL CLOSED** (orders
`work-orders/p4.d30-pascal-canonical-reader.md` / `p4.d31-restore-memory-ids.md`
/ `p4.d32-release-refactor-sweep.md` / `p4.d33-provider-sdk-wire-check.md` /
`p4.d34-terminal-spa-riders.md`). Nineteen v4 commits absorbed; the oracle
baseline MOVES to **`ff12f491`** (v4 HEAD `e1be028b` is one release-infra
commit past it — NO-PORT). The four release-refactor commits proven
output-neutral by D32's 290-family sweep; the SDK majors proven wire-neutral
by D33's byte-identical corpora; two real pre-existing v5 bugs fixed on the
OpenRouter pricing path; restore's memory-id bug fixed with the archive that
can actually see it. Full round record in `status-log.md`; gate numbers in
CLAUDE.md's Status bullet.

**Next candidates, in rough value order:**

1. **The dogfood pass** — long owed and still first: the embedding worker's
   live proof, the chat-dialog family, the picker/attach flow, walk Parts
   D/F/H, P4.21's 💸 vision proof, the toolbar/search walk — and now ALSO
   **P4.D33's 💸 OpenRouter pricing proof** (boot with a real OpenRouter
   key: cost estimation must show real context lengths and tool-capable
   models — before the fix every model parsed to `contextLength: null`,
   `supportsTools: false`).
2. **The `canChooseOutfit` projection gap** — surfaced by D32's sweep:
   v5's character read projection omits `canChooseOutfit`, which v4 emits
   (`characters_read` / `characters_actions` red at BOTH pins; `git log -S`
   proves no drift commit introduced it — it is the P4.6bh outfit round's
   leftover: the vault flag landed, the DB read projection didn't).
   Lane-sized; its differential families already exist and are red.
3. **The store-unavailable 503 envelope** — the P4.D29 unit-4 escalation:
   v4 maps Project/Group/CharacterVault store-unavailable errors to a
   deliberate contextful 503 (`{error, projectId}` etc.) where v5 answers
   500 + a leaked internal detail. The ordered shape (additive
   `ErrorKind::Unavailable` + an entity-id field on `CoreError` + the two
   `overlay_to_db` replacements + the vault sibling) is in the P4.D29
   unit-4 lane record. Small; touches `api/**`.
4. **Sweep debt from D32** — the `terminal_tools` oracle did not survive
   regeneration (unresolved — diagnose whether the case or the recipe
   rotted), and 28 families' header recipes could not be run mechanically
   (list in the D32 lane record / `/tmp/d32-rest-final.json`); worth a
   maintenance pass that makes every header recipe actually runnable (the
   `harness-recipes-are-runnable` rule).
5. **`p4.9h`** (prompt library + embedding-profiles management — carrying
   the banked PUT trigger matrix, its `EMBEDDING_REAPPLY_PROFILE`
   dependency — port target v4 AT/AFTER `13f0ebd7` per D33's bank — and
   four of the queue-badge trigger sites), the workspace per-tab toolbar
   bridge (unlocks the Salon slot adoption — the P4.9P tier-2 ruling),
   the Zod format-validator gap on property bags (P4.D29's deferral),
   `p4.9i2`, `p4.9o`, and the standing pools.

**Standing regen note:** v4 HEAD `e1be028b` is lib-inert past the
`ff12f491` baseline — oracles may regenerate straight from the checkout
until v4 moves again; drift-check before every round.

### Round outcome (2026-07-31) — the dogfood-debt + sweep-debt round, UNIFIED

**P4.22 → P4.23 ∥ P4.24 ∥ P4.25 ∥ P4.26 ∥ P4.27 — ALL SIX ORDERS CLOSED**
(orders `work-orders/p4.22-character-vault-properties-clobber.md` /
`p4.23-store-unavailable-503-envelope.md` / `p4.24-llm-log-cleanup.md` /
`p4.25-toast-subsystem.md` / `p4.26-announcement-rendering-audit.md` /
`p4.27-sweep-debt-maintenance.md`). The oracle baseline STAYS `ff12f491`
(v4 HEAD `e1be028b`, NO-PORT). Dogfood **#40 CLOSED** (the last unhandled
job type — LLM-log retention is live), **#42's toast subsystem LANDED**
(106-file census: 68 converted / 15 open / 23 unported), **#43's
announcement audit ran** (91 rows; the headline: v5-invented system-slab
styling on every expanded announcement, plus five more structural
divergences, all fixed), **#47's clobber guard LANDED** (deliberate
divergence; the v4-side fix stays URGENT with the human), the P4.D29 503
escalation CLOSED, and D32's sweep debt cleared (`canChooseOutfit`,
`terminal_tools`, recipe runnability + the committed sweep driver).
The §3 unification review caught two CONVERTED-marked census rows missing
v4's success toasts (fixed with spec pins) — full round record in
`status-log.md`; gate numbers there and in the CHANGELOG.

**Follow-up, CLOSED (2026-08-01):** the two full-suite-only Playwright
intermittents this round's record flagged and spawned a chip for (the
Rename Chat automatic-naming revert and the auto-lock idle warning under
the fake clock) are **deflaked** — both were one shape, a page-initiated
refetch the beat triggered but never awaited; both reproduced
deterministically with injected delays before being hardened; no
assertion weakened and no product code changed. The suite is
**168/168, zero skips**. Nothing on the candidate list below moved.
Record: `status-log.md` → "Follow-up — the two flake-prone beats
deflaked" and its unification-review subsection.

**Round UNIFIED (2026-08-01) — the `c4d4b0de` v4-drift catch-up
(P4.D35 ∥ P4.D36 ∥ P4.D37 ∥ P4.D38 ∥ P4.D39 ∥ P4.D40): ALL SIX ORDERS
CLOSED; the oracle baseline MOVES to `c4d4b0de` and the drift debt is
CLEARED.** v4 had shipped TEN commits in ~two days, four onto ported
surfaces. Landed: the Pascal side-effects feature end to end (the closed
eval-free expression grammar in Rust AND a client-safe TS twin, the
tiered "write where it lives" applier, chipLabel, the two-block bubble,
the Workbench Side Effects card + dry run) ∥ whispered manual
announcements (the audience resolver, the POST-400/preview-silent
asymmetry, the audience-aware in-character rewrite, the "Who hears it"
dialog, the chip whisper tag on both render sites) + announcement
attribution in LLM context + the whisper-kind narrowing (Prospero's
`group-context` whispers now honour All Whispers — a real v5 leak
closed) + the whisper-label WCAG values in all six themes ∥ the tri-tier
wardrobe at chat start (merged pools, composite hydration, concurrent
resolve with serial commit, the 60 s bound, the deliberate-nudity
contract) ∥ the editor's sub-list indentation contract (PARTIAL by
design — v5 never had v4's flattening bug; it gained unit-preserving
export, Tab/Shift-Tab, and the toolbar controls). Two commits are
NO-PORT with evidence (`4f7e09fa` flushSync; `e1be028b` packaging).
**The §3 review's headline catch: a `--ours` conflict resolution had
silently dropped P4.D39's `futures-util`/tokio-`time` dependency block**
— the playbook's "a Cargo.toml conflict is not version-only" rule, found
by auditing every lane's non-version delta. It also found SIX committed
oracle recipes that could no longer run verbatim (retired /tmp pins, a
recipe leaning on a sibling's staging, two sidecar readers defeated by
the fixture shield) — repaired, all green, none a port regression.
Gate: 409 binaries / 1,798 tests / 0 failed with the round's 64-variable
env block and every one of its 42 families positively confirmed to have
RUN; clippy both feature sets; release build; ng test 268 files / 3,639;
full Playwright 172/172 zero skips. Versions: core
0.0.444, harness 0.0.382, host 0.0.56, SPA 0.5.374.

**Next candidates, in rough value order:**

1. **A dogfood pass — now the clear top item, and it owes more than
   before.** This round's live surfaces join the queue: a custom tool
   whose effects write across the state tiers and onto a character's
   fact sheet (💸 real spend on the roll's consult if it has one), the
   tri-tier dressing on the Friday copy (💸 the merged-pool `llm_choose`
   now fires for characters whose wardrobe is entirely shared, which
   previously skipped the model silently), the whispered-announcement
   flow, and a 4-space-nested Markdown document that must survive an
   edit without reflowing. Plus everything already owed: the toast walk,
   announcement rendering on real history, the first completed
   `LLM_LOG_CLEANUP`, walk Parts G/H, the P4.D31 restore-memory-id
   proof, and P4.21's vision proof.
2. ~~A human ruling on P4.D40's (a)-edge~~ — **RULED 2026-08-02: v5
   KEEPS its CommonMark behavior; the divergence STANDS and no structural
   pre-pass is adopted** (reasoning at the pin in
   `markdown-round-trip.spec.ts`; order header updated). The ruling is
   EVIDENCE-CONDITIONAL: `harness/tools/list_indent_edge_scan.py` is
   committed and found **0 hits** over the dogfood copy's disk-backed
   documents, but the **store-backed documents need the pepper** and are
   an owed dogfood item — hits there reopen it in favour of the pre-pass,
   because the consequence is destructive on SAVE. Nothing to build
   unless that scan turns something up.
3. **The toast census's 15 OPEN rows** (the P4.25 lane record enumerates
   each with its owed sentences — `useProjectDetail`'s 20, the files
   family's 13, `useNewChat`'s 9, AuroraView, character edit/new) — a
   natural single SPA lane.
4. **The app-wide `renderingPatterns`/`dialogueDetection` template gap**
   (P4.26's banked finding): v5 never fetches the chat's roleplay
   template, so EVERY message renders with the defaults where v4 threads
   `template.renderingPatterns` into every row — a Salon-wide fidelity
   gap wanting its own order.
5. **The autonomous-rooms oracle rot** (sweep debt): v4's forked job
   child dies under jest outside the pinned worktree — diagnose and
   repair the case (or suppress the fork) so the family can regenerate
   again.
6. **`p4.9h`** (prompt library + embedding-profiles management + the
   banked PUT trigger matrix), the new-chat wardrobe-composer family
   (which now also owes P4.D39's deferred client half), the workspace
   per-tab toolbar bridge, the Zod format-validator gap, `p4.9i2` (whose
   bank grew again — v5's shipped help text still describes a
   custom-tool format without `effects` or `chipLabel`), `p4.9o`, and
   the standing pools.

**Standing regen note:** the oracle baseline is **`c4d4b0de`** (v4 HEAD,
2026-08-01). v4's tree was clean throughout the round, so oracles
regenerate straight from `~/source/quilltap-server`; pin a detached
worktree only on drift/dirty. ⚠ v4 has shipped ten commits in two days —
drift-check before every round. Recipe regens should go through
`harness/tools/recipe_sweep.py --run <family>` (atomic regen-then-run,
the fixture shield, the SKIP detector); note that a recipe pointing at a
`/tmp` pinned worktree from an earlier round is dead on arrival, since
those do not survive between rounds.

## The hard-link-groups + restore-remainder round (P4.D41 ∥ P4.28 ∥ P4.29 ∥ P4.30) — UNIFIED 2026-08-03

All four orders CLOSED; the oracle baseline MOVES to **`40319484`**
(4.8.0-dev.147). Findings #57–#60 closed; the toast census's OPEN rows are
zero (18 sentences reclassified UNPORTED with named future lanes); every
conversation renders with its own template's patterns. Round record:
`status-log.md`.

**Next candidates, in rough value order:**

1. **The `c988fbd2` + `74ec93b5` drift catch-up** — v4 shipped both
   mid-round. `c988fbd2` (Pascal run presets): `lib/pascal/tool-presets.ts`
   (new) + `custom-tool.types.ts` + the chat custom-tools route +
   `CustomToolRunDialog.tsx` + `lib/query/keys.ts` + `help/custom-tools.md`
   — the pascal/workbench families' blast radius. `74ec93b5` (bounded
   provider requests so a stalled call can't wedge a turn):
   `lib/chat/context-manager.ts` + `lib/memory/cheap-llm-tasks/
   core-execution.ts` + `lib/promise-timeout.ts` + every plugin provider —
   the provider-I/O + cheap-LLM families' blast radius. Two build-only
   siblings (`51c350a1`, `49769ec4`) look NO-PORT; confirm at planning.
   Drift-check before planning — v4 has been shipping daily.
2. **The mount-index delete path leaves children behind (the #58 ROOT
   CAUSE)** — a `doc_mount_points` delete that takes neither its links nor
   its folders is how the real instance accumulated 43+118 orphans. P4.28
   fixed the restore side only; the delete path lives in
   `services/mount_index/**` and wants a small order of its own (v4-side
   twin queued on the post-5.0 list).
3. **The `doc_text` + `doc_fm` stale-RED** (pre-existing, surfaced by
   P4.D41's mass regen): their oracle cases mock the reindex module, which
   ALSO silences v4's P4.6BK chunk-on-write, so mocked-v4 diverges from
   chunking-v5 on `chunkCount`. Both candidate fixes change what the family
   proves — the doc-edit oracle owner's ruling, then the repair.
4. **The vintage INSERT-tolerance follow-up** — the live tripwire
   `a_column_a_migration_never_added_still_loses_that_collection` pins the
   exposure (one late-vintage missing column loses `chats` + two cascading
   collections); the repair is ~20 typed creates adjudicated per-site over
   the committed migration-vintage fixture.
5. **A dogfood pass** — it owes this round's live proofs (a real `docs
   link` + edit-either-side walk on the Friday copy, the restore of a
   damaged archive with the new skip sentences, the pump pause, template
   patterns on real chats, the backup skipped-files toast) plus the
   standing queue (walk Part F items 15/16, #61's fresh-copy re-walk, the
   P4.D31/P4.21/D33 💸 proofs).
6. The standing pools: `p4.9h`, `p4.9i2` (the bank grew `help/
   custom-tools.md` again), `p4.9l` (composer toolbar — `narrationDelimiters`
   joins the template fetch when it lands), the P4.D41 tier-2 item 9
   committed grouped-pair fixture, the PumpPause in-flight counter, and the
   toast census's UNPORTED rows riding their future screen lanes.

**Standing regen note:** the oracle baseline is **`40319484`**
(4.8.0-dev.147, 2026-08-03). ⚠ v4 HEAD is `c988fbd2`, ONE commit past it
(the item-1 drift): regenerate pascal-family oracles only from a worktree
PINNED at `40319484` until that catch-up runs; the system/backup/restore/
doc-mount families were verified untainted by name and regenerate straight
from the checkout. The distill-transitive TZ pins, the committed-fixture
rule, and the recipe-sweep driver notes all stand unchanged.

## The `49769ec4` drift catch-up + store-delete round (P4.D42 ∥ P4.D43 ∥ P4.31 ∥ P4.32) — UNIFIED 2026-08-04

All four orders CLOSED; the oracle baseline MOVES to **`49769ec4`**
(4.8.0-dev.150). Provider requests are bounded end-to-end (dogfood's
ten-silent-minutes class is closed), custom-tool run presets are live,
dogfood #58's root cause is fixed at both ends (cascade + reaper), and
the `doc_text`/`doc_fm` stale-RED is repaired per the human ruling.
Round record: `status-log.md`.

**Next candidates, in rough value order:**

1. **The `4bbeab47` + `7fe9fe40` drift catch-up** — v4 shipped both
   during the round, BOTH behavior on ported surfaces. `4bbeab47`
   (roleplay-template picker at chat creation): the ported
   `app/api/v1/chats/route.ts` (+29) + the New-Chat SPA family
   (`NewChatForm`/`useNewChat`/`types`/`NewChatModal`/
   `NewChatPageClient`); two help docs → `p4.9i2`. `7fe9fe40` (stop
   teaching models asterisk narration): the aurora/commonplace/suparna
   notification writers + `core-whisper.ts` + `native-tool-prompt.ts` —
   and v5's `native-tool-prompt.ts` rule-1 wording is ALREADY stale from
   `8bf3cb5f` (found by P4.D42's sweep), so this lane closes two debts;
   mirror `docs/developer/features/roleplay-block-narration.md`.
   Drift-check before planning — v4 has been shipping daily.
2. **P4.33 — import overwrite claims the whole store + store identity
   by ID** (`work-orders/p4.33-import-overwrite-id-identity.md`; the
   P4.31 escalation, **RULED 2026-08-04**: option (b) full folder
   clear, overwrite matches by ID not name, import CREATE preserves
   archive store ids, and character-vault references resolve by ID
   everywhere — ruling record in `status-log.md`). Standalone; can run
   alone or join the next round; drift-independent of item 1 (verified:
   neither drift commit touches `lib/import/**`).
3. **The recipe-rot maintenance order** — P4.D42's 75-family sweep
   re-measured the standing debt with fresh numbers: 19 families'
   recipes cannot be re-run mechanically; ~7 more are stale-red for
   pre-existing causes (`compression_tier3` latent-red since the P4.13
   ruled error row; `pseudo_tool_prompts` from `8bf3cb5f` — partly
   closed by candidate 1; others enumerated in the sweep record). The
   D32/P4.27 debt has not shrunk; it wants its own order before it hides
   the next real red.
4. **A dogfood pass** — owes this round's live proofs (the orphan reaper
   against the Friday copy's 43+118, the presets round-trip on real
   data, the bounded-turn behavior) plus the standing queue (walk Part D
   / Part F items 15/16, the P4.D31/P4.21/D33 💸 proofs, the vintage
   tripwire).
5. The standing pools: `p4.9h`, `p4.9i2` (the bank grew again —
   `help/custom-tools.md` + `4bbeab47`'s two chat docs), `p4.9l`, the
   P4.D41 tier-2 item 9 committed grouped-pair fixture, the P4.31
   note-grade items (the wiring pin's setup discharge, the vacuous
   assertion, the scratch dirs).

**Standing regen note (SUPERSEDED by the `7fe9fe40` round below):** the
oracle baseline was **`49769ec4`** (4.8.0-dev.150, 2026-08-03) until the
`7fe9fe40` round unified.

---

## The `7fe9fe40` round (P4.D44 ∥ P4.D45 ∥ P4.33 ∥ P4.34): UNIFIED on main (2026-08-04) — ALL FOUR CLOSED; the oracle baseline MOVES to `7fe9fe40` and the drift debt is CLEARED

Candidates 1–3 of the previous list ran as one four-lane round (orders
committed at planning; round record + four lane records in
`status-log.md`). The New-Chat roleplay-template picker is live
end-to-end (tri-state `roleplayTemplateId` riding the `ChatCreate`
flatten seam — `api/types.rs` never opened; the capstone family grew
five `rt_*` cases + the un-normalized `chat_template_ids` section after
mutation testing caught the UUID-normalizer blindness); the thirteen
staff strings + the native-tool-prompt rule stopped teaching asterisk
narration (the six direct families' measured RED→GREEN flip; the
`8bf3cb5f` wording debt closed with it); the import family now matches
stores by ID with id-preserving create and the full folder clear (four
`store_identity_*` arms + `FOLDER_CLEAR_DIVERGENCE`, all
both-directions); and the recipe-rot debt is REPAIRED, not re-measured —
the "19 unrunnable" hypothesis was refuted (8 venue-healed, 2
driver-healed, 1 sweep artifact, 4+4 genuinely broken and fixed), the
sweep driver gained its self-test + the durable `--run-all` artifact,
and the autonomous-rooms oracle race is fixed. **The escalated
remainder:** `context_summary_service_tier3` stays red — the unifier
confirmed the cause is a STALE ORACLE MOCK (v5's live fold-episode pass
is faithful; v4 folds in production at `lib/chat/context-summary.ts:519`)
— a small dedicated harness order is owed to un-mock it (the P4.32
`doc_text`/`doc_fm` precedent). Gate numbers, wires, and the §3 review
findings: the round record in `status-log.md`.

**Next candidates, in rough value order:**

0. **⚠ AN INBOUND v4 DRIFT IS EXPECTED, ON PORTED SURFACE — check for it
   FIRST.** The 2026-08-04 dogfood walk produced a v4-side design spec,
   `~/source/quilltap-server/docs/developer/features/import_export_update.md`
   (uncommitted at hand-off), which the human intends to implement: entity
   exports gain the three unexposed types (projects / groups / document
   stores), `memory` records stop carrying `embedding` (writer AND reader —
   an existing archive's vectors must be DROPPED, not trusted), and possibly
   a re-embed at the end of restore. **All of that is already-ported
   surface.** Named blast radius: `system_export_equivalence`,
   `system_import_equivalence`, `system_import_state`, the restore families
   if §3a lands, and the SPA's export/import dialogs for the picker's type
   list. **Drift-check before planning anything else**; if it has landed,
   this is the round. Note the port has one deliberate divergence living in
   this exact family (`EXPECTED_DIVERGENCES` — the sparse-array blob read,
   the three ruled restore bugs, `REPLAY_DEDUPE`, the store-identity arms):
   re-check each against the new v4 rather than assuming they survive.
1. **A dogfood pass** — owes this round's live proofs (the picker on
   real data; the de-asterisked whispers in a real chat) plus the
   standing queue: the orphan reaper against the Friday copy's 43+118,
   the presets round-trip, the bounded-turn behavior, walk Part D /
   Part F items 15/16, the P4.D31/P4.21/D33 💸 proofs, the vintage
   tripwire.
2. **`p4.35` — the streaming `.qtap` import**
   (`work-orders/p4.35-streaming-qtap-import.md`, written 2026-08-04 out
   of the dogfood walk). Dogfood #63 raised the transport ceiling to v4's
   real 10 GB, so a 791 MB real export is now *reachable* — and held
   about three times over (raw `Bytes` + the whole-record `Vec` + a
   CLONING assembler). Stage the upload to disk, read lines from a
   `BufRead`, assemble from owned records: ~3× → ~1×, which is v4's own
   shape. **Bounded deliberately** — v4 assembles the whole export too,
   so going below 1× would mean abandoning its ten-map orchestrator and
   is explicitly out of scope. Guarded by the three import families.
3. **The context-summary oracle un-mock** — the escalated
   `context_summary_service_tier3` red: update the oracle case to run
   v4's REAL `runFoldEpisodePass` (canned model both sides), regenerate,
   and retire the escalation (the P4.32 un-mock precedent; diagnosis
   pinned in the round record and `common/mod.rs`'s helper doc).
4. **The remaining recipe-rot tail** — 27 families still carry
   `unstaged_jest_roots` (named by `recipe_sweep.py --list`; each needs
   its own staged-mirror conversion + verification run), 20 static /tmp
   collisions. Mechanical but per-family; can ride any round as a rider
   or run as a second maintenance pass.
5. The standing pools: `p4.9h`, `p4.9i2` (the bank grew again —
   `4bbeab47`'s two chat docs), `p4.9l`, the P4.D41 tier-2 item 9
   committed grouped-pair fixture, the P4.31 note-grade items.

**Standing regen note:** the oracle baseline is **`7fe9fe40`**
(4.8.0-dev.152, 2026-08-04), adopted at this round's unification — NO v4
drift debt remains as of the unification (drift-check before every
round; v4 ships daily). Oracles regenerate straight from
`~/source/quilltap-server`; pin a detached worktree only on drift/dirty.
The distill-transitive TZ pins, the committed-fixture rule, and the
recipe-sweep driver notes stand unchanged — plus the new rule from
P4.34: **run any `unstaged_jest_roots` family with
`--v5w ~/source/quilltap-v5`** (jest ignores `.claude/` venues), and
prefer `recipe_sweep.py --run-all --results …` so classifications
survive the round.

## The `7189a968` round (P4.D46 ∥ P4.D47 ∥ P4.D48 ∥ P4.36) — UNIFIED 2026-08-05

**ALL FOUR CLOSED; the oracle baseline MOVES to `7189a968` and the
predicted import/export drift is ABSORBED.** The server port (embedding
strip + per-memory re-embed enqueue, the fifteen export types, the
doc-stores ordering fix, compact backup + restore 24a/25, the plugin
`enabled` carry) ∥ the SPA half (the fifteen-type picker, the preview
`detail` line, the compact toggle, the gated beat flipped LIVE) ∥ the
Anthropic-SDK wire check (byte-neutral, proven, not assumed) + the five
infra NO-PORT dispositions + the container-timezone resolver ∥ the
context-summary oracle un-mock (the escalated stale red RETIRED; a
second stale mock of the same class found and fixed by consequence).
Details: the round record in `status-log.md`; the four order status
headers.

**Next candidates, in rough value order:**

0. **⚠ The `0cde7fbc` Almanack drift catch-up — v4 moved DURING this
   round and the drift is REAL, on ported surfaces.** Migration
   `add-llm-logs-profile-columns-v1` (llm_logs gains nullable
   `connectionProfileId`/`imageProfileId` — D23 re-dump territory on
   the llm partition), both columns join the new-account restore's UUID
   remap list (the ported `uuid_remap`), the llm-logging service
   threads the profile ids + `durationMs` through ported call sites,
   `LLMLogsRepository.getTotalTokenUsage*`'s `$ne: null` never-matches
   bug is fixed (v5 may carry the broken-but-exact twin from P4.6ar —
   check), and the Almanack report itself is UNPORTED surface (a
   port-or-defer decision for the round planner). **Until it runs,
   regenerate any llm-logs-touching or restore-family oracle from a
   worktree pinned at `7189a968`.** ⚠ v4's tree was left DIRTY with
   in-flight almanack test work at unification — re-check before
   pinning.
1. **A dogfood pass** — the queue is now substantial: this round's live
   proofs (a real post-strip export size on the Friday copy, an import
   that enqueues embeddings, a compact backup/restore round-trip, the
   container-TZ resolver in a real `docker run`), plus the standing
   backlog (the orphan reaper's 43+118, P4.D43's presets, walk Parts
   C 17–19 / D / G / H, the P4.D31/P4.21/D33 💸 proofs).
2. **The `gen-provider-manifests.mjs` repair** (a small order): teach
   the generator `imageGenerationModels` (three providers expose it on
   the built plugin; grok + z-ai need P4.6p's source-level
   transcription re-derived) so the manifest regen recipe stops
   deleting a field five committed manifests carry. The warning header
   + safe diff-into-scratch recipe landed in P4.D48.
3. **The remaining recipe-rot tail** — 27 `unstaged_jest_roots`
   families + 20 static /tmp collisions (P4.36's family turned out NOT
   to be one — its order's warning was stale).
4. The standing pools: `p4.9h`, `p4.9i2` (+ `7189a968`'s two help docs:
   `system-backup-restore.md`, `system-import-export.md` — no v5
   action, runtime sync, but the bank tracks them), `p4.9l`, the
   P4.D41 tier-2 item 9 fixture, the P4.31 note-grade items, the
   `perl-base` purge candidate on v5's Docker image (P4.D48's caveat).

**Standing regen note:** the oracle baseline is **`7189a968`**
(2026-08-05), adopted at this round's unification. **v4 HEAD is
`0cde7fbc`, ONE commit past it (+ a dirty tree at last check) — the
catch-up is candidate 0 above; pin a detached worktree at `7189a968`
for any regen until it lands.** The distill-transitive TZ pins, the
committed-fixture rule, and the recipe-sweep venue rules stand
unchanged.

## The `f7f1a956` Almanack round (P4.D49 ∥ P4.37 ∥ P4.38 ∥ P4.39) — PARTIALLY UNIFIED 2026-08-05

**P4.D49, P4.38, P4.39 CLOSED; P4.37 OPEN (partially unified — resume
list in its order header); the oracle baseline MOVES to `f7f1a956`.**
The `0cde7fbc` drift's ported-surface half is fully absorbed (the
llm-logs D23 re-dump, the write spine + six call sites, the token-usage
un-zero that makes the autonomous daily budget BIND for the first time,
the remap additions, the jest-TZ defuse); the Almanack's pure half
(renderer + phase manifest + progress `phase` frame) and its whole SPA
are on main; the Almanack's collectors/verbs/host-wire are HELD on the
preserved branch `claude/almanack-server-porting-693d77` pending their
tier-2 differential (the lane's own record forbade unifying them
unverified). The manifests generator is repaired and the recipe safe as
written; `perl-base` is purged from the Docker image. Details: the round
record in `status-log.md`; the four order status headers.

**Next candidates, in rough value order:**

1. **The resumed P4.37 — the Almanack server remainder.** Rebase the
   held commits, write unit 12 (the `almanack-*` fixture family + the
   tier-2 differential, fixture plan in the lane record), the
   `AlmanackHost` wire, the space-form date-stamp arm (§3 finding),
   flip `P437_SERVER_LANDED`, re-diff §1. Until it lands the Almanack
   card shows an empty Previous Editions list and the four verbs do not
   exist — the SPA is DONE and waiting.
2. **⚠ The Taboo drift catch-up — LANDED, now OWED.** v4 shipped
   `7df7de8e` ("feat(taboo): instance-wide forbidden phrases in the
   system prompt") within the hour of this round's unification
   (observed at cleanup, 2026-08-05). It is the in-flight feature the
   round watched brewing, on PORTED chat-spine surfaces
   (`system-prompt-builder.ts`, `context-manager.ts`, `cache-key.ts`,
   `settings.types.ts`, `instance-settings/index.ts`,
   `self-inventory/builders.ts` + a new settings route/component/help
   doc). The catch-up round runs FIRST or alongside the resumed P4.37
   (their surfaces look disjoint — verify at planning); pin at
   `f7f1a956` for every regen until it is absorbed.
3. **A dogfood pass** — the owed queue keeps growing: the `7189a968`
   round's live proofs, the standing backlog (walk Parts C 17–19 / D /
   G / H, the P4.D31/P4.21/D33 💸 proofs), and now this round's: the
   enclave daily budget binding on a real room, the llm_logs profile
   attribution on real calls, the Almanack itself once the resumed
   P4.37 lands.
4. **A maintenance lane**: the two families whose oracles cannot
   regenerate at `f7f1a956` (`context_summary_service_tier3`,
   `memory_processor_tier3` — `no such table: llm_logs`, the P4.36
   stale-mock class, escalated by P4.D49); the image-path `durationMs`
   zeros (follow-up chip filed 2026-08-05); the thread-local
   tracing-capture race in `job_runner`'s
   `failed_job_emits_a_tracing_event`; the recipe-rot tail; three
   full-suite-only Playwright intermittents observed at this round's
   gate (the two terminal-pane beats + the Rename Chat beat — each
   isolation-green; the deflake-round treatment applies).
5. The standing pools: `p4.9h`, `p4.9i2` (+ `0cde7fbc`'s three help
   docs: `the-almanack.md` NEW, `system-capabilities-report.md`
   rewritten, `system-tools.md`), `p4.9l`, the P4.D41 tier-2 item 9
   fixture, the P4.31 note-grade items.

**Standing regen note:** the oracle baseline is **`f7f1a956`**
(2026-08-05), adopted at this round's unification. v4 HEAD is
`44e2e4fe`, ONE commit past it (docs-only, NO-PORT), **and v4's tree is
DIRTY with the in-flight Taboo feature — pin a detached worktree at
`f7f1a956` for EVERY oracle regen until the Taboo round absorbs it**
(`oracle-regen-pinned-v4-worktree`). New this round: jest-based
Chicago-leg regens need `--globalSetup
harness/oracle/lib/jest-zone-globalsetup.cjs` + `QT_ORACLE_TZ` (v4's
jest configs force TZ=UTC before workers fork — an env-passed TZ is
silently clobbered); the distill + llm-log-cleanup recipes carry it,
and both families now zone-mark their NDJSONs. The distill-transitive
TZ pins, the committed-fixture rule, and the recipe-sweep venue rules
stand unchanged.

## The Taboo + maintenance round (P4.37-resumed ∥ P4.D50 ∥ P4.40) — UNIFIED 2026-08-06

All three orders CLOSED (headers + the round record in `status-log.md`).
The Almanack is LIVE end to end (collectors oracle-verified, host wire
live, walk active); the Taboo feature is absorbed whole (storage →
prompt section → cache-key v3 → verbs/REST → the Settings card, all
differential-pinned); the maintenance debt is cleared (both escalated
tier-3 oracles regenerable, `compression_tier3`'s standing red closed
by the same corpus defect, the tracing race fixed, two e2e beats
hardened, the sweep driver drift-safe via `--v4`). The §3 unification
review fixed the Taboo `double_option` dispatch-leg bug and five
Almanack fidelity minors on the unify branch.

**Next candidates, in rough value order:**

1. **A dogfood pass** — now unambiguously top; the owed queue: the
   Almanack's first report on the Friday copy (💸 none — no model
   calls), the live Taboo section on a real turn (add a phrase, see
   the prompt carry it; the cache-key v3 rollover), the P4.D49 live
   proofs (enclave daily budget binding on real spend; llm_logs
   profile attribution), plus the standing backlog (walk Parts C
   17–19 / D / G / H, the P4.D31/P4.21/D33 💸 proofs, the OpenRouter
   pricing fix with a real key).
2. **The standing pools:** `p4.9h` (embedding-profiles management +
   the banked PUT trigger matrix), `p4.9i2` (the help surface — the
   bank now holds the two Taboo docs + the three Almanack-era docs +
   earlier), `p4.9l` (the composer-toolbar slice), the P4.D41 tier-2
   item 9 fixture, the P4.31 note-grade items.
3. **The BUILTIN (TF-IDF) provider-manifest decision** — the
   Almanack's provider table omits the BUILTIN row until a manifest
   exists (pinned both directions; P4.37's named deferral).
4. **D21 / release-grade packaging** remains deliberately unstarted
   (the "don't initiate a release" standing rule).

**Standing regen note:** the oracle baseline is **`3adefeba`**
(2026-08-06, v4 HEAD, tree clean), adopted at this round's
unification — lib-identical to `7df7de8e` (the Taboo feature; the
only delta above it is `docs/releases/4.8.0.md`). NO v4 drift debt
remains. Oracles may regenerate straight from
`~/source/quilltap-server` while HEAD stays `3adefeba`; pin a
detached worktree on drift/dirty (`oracle-regen-pinned-v4-worktree`),
or sweep with `recipe_sweep.py --v4 <pin>` (P4.40's addition —
recipes never name a pin themselves; the two rules compose). The
almanack-family NDJSON `baseline:` markers name the CASE vintage
(`f7f1a956`), not the pin — regenerating at any lib-identical-or-later
baseline keeps them consistent until the case itself moves. The
distill-transitive TZ pins, the committed-fixture rule, and the
venue/staging rules stand unchanged.

## The fallback + wire + embedding-profiles round (P4.41 ∥ P4.42 ∥ P4.9H2A ∥ P4.9H2B) — UNIFIED 2026-08-06

All four orders CLOSED (P4.9H2A with units 6+7 deferred loudly — the
four maintenance verbs refuse by name; the SPA cards + gated beats
already exist). The chaining fallback un-wedges OpenAI multi-turn chats
(finding #69); `search_web` executes on every tool-running surface
incl. the production enclave, with the inventory bool DERIVED from the
provider's presence; embedding-profiles management is live end to end
(CRUD + the PUT trigger matrix + the reapply handler + REST edges +
the Settings cards + the `p4.9o` Scriptorium badge). The §3 review
caught the PUT echo-null wire defect before it shipped and the
first-run CRUD beat exposed the vintage-fixture embedding-table gap
(both fixed on the unify branch). Round record: `status-log.md`.

**Next candidates, in rough value order:**

1. **⚠ The v4 drift catch-up — OWED.** v4 moved four commits past
   `3adefeba` DURING the round (`13ddc5ee` vault-overlay guards + help
   sync; `3bb664f0` backup/store-delete integrity — several arms are
   v4 ADOPTING this port's queued fixes, so the self-retiring
   convergence pins will trip at the baseline move, by design;
   `7bcd8515` mount-index blobless GC / sibling reindex / the
   embedding-dimension reconcile / doc attach / thumbnail sweep;
   `d60fc34d` docs) and its tree is DIRTY with in-flight
   memory-service/fold-episode work. All on PORTED surfaces. Pin a
   detached worktree at `3adefeba` for every regen until it runs.
2. **The P4.9H2A units 6+7 follow-up** — memory-dedup (the 446-LOC
   synchronous union-find algorithm) + conversation-summaries regen
   (+ its handler), each with its differential; the verbs, SPA cards,
   gated beats (`P49H2A_MAINTENANCE_LANDED`), deps, and the fixture's
   crafted near-duplicate memories are all in place.
3. **A dogfood pass** — the owed 💸 queue: the Serper live-key smoke,
   the chaining fallback on a real OpenAI enclave, the MOUNT-partition
   reapply + encrypted VACUUM-INTO backup (sandbox-blind in the
   differential), profile management on the Friday copy, plus the
   standing backlog (the Almanack first report, live Taboo, the P4.D49
   budget/attribution proofs, OpenRouter pricing with a real key).
4. **The standing pools:** `p4.9i2` (help surface), `p4.9l` (composer
   toolbar), the rest of the `p4.9h2` bucket (prompt library, global
   Core Whisper card, tag pickers, memory editor), the BUILTIN TF-IDF
   manifest decision, the P4.D41 tier-2 item 9 fixture.

**Standing regen note:** the oracle baseline is **`3adefeba`**
(unchanged this round). **⚠ v4 HEAD is `7bcd8515`, FOUR commits past
it, tree DIRTY — pin a detached worktree at `3adefeba` for EVERY
oracle regen until the catch-up round absorbs the drift**
(`oracle-regen-pinned-v4-worktree`, or `recipe_sweep.py --v4 <pin>`).
The distill-transitive TZ pins, the committed-fixture rule, and the
venue/staging rules stand unchanged.

## The `f4955e0e` found-bugs convergence round (P4.D51 ∥ P4.D52 ∥ P4.D53 ∥ P4.D54 ∥ P4.D55 ∥ P4.43) — UNIFIED 2026-08-06

**ALL SIX ORDERS CLOSED; the oracle baseline MOVES to `f4955e0e` and the
drift debt is CLEARED.** v4's coordinated "bugs 8–43" batch (eleven
commits; at the new baseline every catalogued v4 bug 1–43 is fixed) was
absorbed whole: the convergence sweep retired ~25 named both-direction
pins across seven differential families (v4 adopting fixes this port
made first), the four genuine ports landed (interchange sub-chunking
UTF-16 end-to-end, the AllLLMPauseModal + opener, the OpenRouter vision
send path, the orphan-thumbnail sweep with the new `StorageBackend`
list seam), and P4.43 closed P4.9H2A whole (memory-dedup +
conversation-summaries regeneration LIVE, both beats active). v4 HEAD
`cc0bbebf` (one commit past the baseline) is test-only — NO-PORT,
lib-identical; oracles regenerate straight from the checkout.

**Survivor pins (deliberate, still both-directions):**
`PHASE_ORDER_RESIDUAL`, `V5_STATS_GAP`, `PLANTED_ORPHANS` — plus TWO
NEW v4 restore bugs P4.D51 measured while retiring bug 12's pins (the
22a-bis `restored/`-folder replay collision; the >3 MB phantom-copy
dedup miss), both pinned and queued v4-side.

**Next candidates, in rough value order:**

1. **The dogfood pass — now carrying the round's 💸 queue on top of the
   standing one:** the OpenRouter vision live send, arm (C)'s one-time
   boot render+embed burst on the Friday copy, the memory-dedup +
   summaries-regen first live run, plus the standing items (the Serper
   live-key smoke, the OpenAI chaining fallback, the MOUNT-partition
   reapply + encrypted VACUUM-INTO backup, profile management on the
   Friday copy, the Almanack first real-data report, live Taboo, the
   P4.D49 budget/attribution proofs, OpenRouter pricing with a real
   key).
2. ~~**The finding-#39 re-ruling (HUMAN)**~~ — **✅ RULED 2026-08-06
   (same day): the overlay design STANDS; v4's bug-27
   mutate-and-restore mechanism is ruled a MISTAKE and its correction
   is queued v4-FIRST** (it moves the oracle — the two overlay gate
   sites are named on `dogfood-findings.md`'s #39 entry; ruling
   record in `status-log.md` → "Ruling — the #39 impersonation
   mechanism"). v5 stays faithful to bug-27's shipped flips until v4
   migrates, then absorbs it as an ordinary drift re-port.
3. **Small follow-ups recorded this round:** the conversation-chunks
   `upsert` create-arm corpus op (needs minted-id normalizer machinery
   in that family); P4.D55's vision-path headers/abort pinning gap; the
   P4.D54 AllLLMPause live-opener e2e beat (needs a seeded paused
   all-LLM chat); P4.D51's bug-43 tier-2 per-delete
   `cleanup_thumbnails` (needs a `StorageBackend` seam through
   `file_delete`/`file_upload`).
4. **The standing pools:** `p4.9i2` (help surface), `p4.9l` (composer
   toolbar), the rest of the `p4.9h2` bucket (prompt library, global
   Core Whisper card, tag pickers, memory editor), the BUILTIN TF-IDF
   manifest decision, the P4.D41 tier-2 item 9 fixture.

**Standing regen note:** the oracle baseline is **`f4955e0e`**
(4.8.0-dev.175, adopted at this round's unification). v4 HEAD
`cc0bbebf` is test-only past it (lib-identical, verified by name) —
oracles regenerate straight from `~/source/quilltap-server`; pin a
detached worktree on any further drift
(`oracle-regen-pinned-v4-worktree`, or `recipe_sweep.py --v4 <pin>`).
The distill-transitive TZ pins, the committed-fixture rule, and the
venue/staging rules stand unchanged. Drift-check before every round —
v4 ships daily.

## The P4.D56 Bug 44 impersonation-overlay round (2026-08-07) — CLOSED

The pre-announced drift round ran and UNIFIED on main the same day v4
shipped it: `62c63dc3` (4.8.0-dev.178) implements the #39-ruled overlay
— impersonation never writes `controlledBy`, the new
`isUserDrivenSeat` helper gates attribution and who-responds, and the
owner-seat readers deliberately stay on the column. v5 absorbed it
whole (order `work-orders/p4.d56-impersonation-overlay-drift.md`,
CLOSED; round record in `status-log.md`): the helper + the two
`salon.rs` handler rewrites + the exact change-list threading, the
keep-list verified against v4, twelve moving + twelve neutrality
families fresh at the pin, and the impersonation beat re-gestured BACK
(the Stop button returned to the participant card, stop driven through
the UI). Gate: 419 binaries / 1,956 / 0; ng 294 / 4,015; Playwright
189/189 zero skips. Versions: core 0.0.509, harness 0.0.432, SPA
0.5.431.

**Next candidates, in rough value order:**

1. **The dogfood pass — long owed, now unblocked** (no drift debt
   remains): the standing 💸 queue (the OpenRouter vision live send,
   arm (C)'s boot burst on the Friday copy, the memory-dedup +
   summaries-regen first run, the Serper live-key smoke, the OpenAI
   chaining fallback, the MOUNT-partition reapply + encrypted
   VACUUM-INTO backup, profile management, the Almanack first
   real-data report, live Taboo, the P4.D49 budget/attribution proofs,
   OpenRouter pricing with a real key) **plus this round's live
   surface: a real impersonate → turn-pause → skip → stop cycle on the
   Friday copy** (the overlay end-to-end, including the healed Stop
   affordance).
2. **The sweep-driver rot maintenance pass:** the six tier-1 turn
   families SKIP-masquerade in `recipe_sweep.py` (extraction rot,
   flagged at the f4955e0e round, re-confirmed at this one) plus the
   four families named in the f4955e0e round record (prose parens; the
   wrong env name for `turn_pause_filters`; `help-sync-guards`
   staging).
3. **Small follow-ups:** the conversation-chunks `upsert` create-arm
   corpus op (minted-id normalizer machinery); P4.D55's vision-path
   headers/abort pinning gap; the P4.D54 AllLLMPause live-opener e2e
   beat (seeded paused all-LLM chat); P4.D51's bug-43 tier-2 per-delete
   `cleanup_thumbnails` (`StorageBackend` seam); the
   `impersonating_ids` four-site extraction consolidation (this
   round's style note).
4. **The standing pools:** `p4.9i2` (help surface — now also carrying
   `help/chat-participants.md`), `p4.9l` (composer toolbar), the rest
   of the `p4.9h2` bucket, the BUILTIN TF-IDF manifest decision, the
   P4.D41 tier-2 item 9 fixture.

**Standing regen note (SUPERSEDED by the P4.D57–59 round below):** the
oracle baseline was **`62c63dc3`** (4.8.0-dev.178, adopted at the P4.D56
unification); it MOVED to `1bed814f` at the P4.D57∥D58∥D59 unification.

## The `1bed814f` drift catch-up round (P4.D57 ∥ P4.D58 ∥ P4.D59, 2026-08-08) — CLOSED

**ALL THREE ORDERS CLOSED; the oracle baseline MOVES to `1bed814f` and
the drift debt is CLEARED.** v4's three-commit day absorbed whole:
`6452e2c3` — the Brahma Console agent-turn budget as
`instance_settings['brahmaConsole']` (default 25 → 50, bounds 5–200,
one shared resolver read by BOTH Brahma paths, the
`brahmaConsoleSettings`/`brahmaConsoleSettingsUpdate` dispatch verbs +
`GET/PUT /api/v1/settings/brahma-console`, the 12-case
settings-routes family, both brahma tier-3 oracles regenerated with
the 50-cap prompt, and the Settings → Chat card in v4's slot with a
LIVE round-trip beat) ∥ `1bed814f` — the salon impersonation
reconcile (dogfood **#71/#72 CLOSED**: the client
`isUserDrivenSeat`/`findActiveUserParticipant` twins, the banner gate
re-diverged onto the overlay, the optimistic-bubble attribution fix,
and the `SpeakingAsAvatar` composer cue) ∥ `ddd7576b` — the About
workspace backdrop, dispositioned **NO-PORT** (v5 ships no About
background asset; recorded in `m6-screen-parity.md` §1.4). The §3
review found no blocking issues (one fixture-vintage comment
contradiction fixed on the unify branch); the wire folded the two
request variants into the SPA contract name-for-name and activated
the gated beat. Gate + versions: the round record in `status-log.md`.

**Next candidates, in rough value order:**

1. **The dogfood pass — long owed, still top** (no drift debt
   remains): the standing 💸 queue (the OpenRouter vision live send,
   arm (C)'s boot burst on the Friday copy, the memory-dedup +
   summaries-regen first run, the Serper live-key smoke, the OpenAI
   chaining fallback, the MOUNT-partition reapply + encrypted
   VACUUM-INTO backup, profile management, the Almanack first
   real-data report, live Taboo, the P4.D49 budget/attribution
   proofs, OpenRouter pricing with a real key), **plus the P4.D56
   round's impersonate → turn-pause → skip → stop cycle on the Friday
   copy — which now ALSO walks this round's surface: the impersonated
   seat's own turn banner + Skip, the speaking-as composer portrait,
   and a raised Brahma Console budget on a real deep query.**
2. **The sweep-driver rot maintenance pass:** the six tier-1 turn
   families SKIP-masquerade in `recipe_sweep.py` (extraction rot,
   flagged at the f4955e0e round, re-confirmed at P4.D56) plus the
   four families named in the f4955e0e round record.
3. **Small follow-ups:** the conversation-chunks `upsert` create-arm
   corpus op; P4.D55's vision-path headers/abort pinning gap; the
   P4.D54 AllLLMPause live-opener e2e beat; P4.D51's bug-43 tier-2
   per-delete `cleanup_thumbnails` (`StorageBackend` seam); the
   `impersonating_ids` four-site extraction consolidation.
4. **The standing pools:** `p4.9i2` (help surface — now also carrying
   `help/chat-participants.md`), `p4.9l` (composer toolbar), the rest
   of the `p4.9h2` bucket, the BUILTIN TF-IDF manifest decision, the
   P4.D41 tier-2 item 9 fixture.

**Standing regen note:** the oracle baseline is **`1bed814f`**
(4.8.0-dev, adopted at the P4.D57∥D58∥D59 unification; v4's tree was
CLEAN there at the round's regen). Oracles regenerate straight from
`~/source/quilltap-server` while HEAD stays there; pin a detached
worktree on any further drift (`oracle-regen-pinned-v4-worktree`, or
`recipe_sweep.py --v4 <pin>`). The distill-transitive TZ pins, the
committed-fixture rule, and the venue/staging rules stand unchanged.
Drift-check before every round — v4 ships daily.

## The `f6eac168` drift catch-up round (P4.D60 ∥ P4.D61 ∥ P4.44, 2026-08-08) — CLOSED

**ALL THREE ORDERS CLOSED; the oracle baseline MOVES to `f6eac168` and
the drift debt is CLEARED.** v4's Bugs 47–51 commit — filed from this
port's own 2026-08-08 dogfood walk — absorbed whole: **P4.D60** (server)
landed `select_next_speaker_after_user_message` + the spine's
`maybe_pause_for_user_seat_turn` fair-rotation pause (bug 50; the
`fair_rotation_pause` tier-3 case mutation-proven; the Carina markup side
effect deferred loud at BOTH `user_message_carina` sites), the byte-exact
Brahma budget-exhaustion salvage in both paths (bug 47; both tier-3
families grew budget cases via a runtime `maxAgentTurns` override so the
committed fixtures stayed untouched), the chat-GET projection of
`impersonatingParticipantIds`/`activeTypingParticipantId` (bug 51;
`get_impersonated` + the `[]`/`null` default arms), and the five-copy
`impersonating_ids` consolidation ∥ **P4.D61** (SPA) landed
impersonate-takes-the-turn as a client `turnOverride` LAYERED above v5's
server-authoritative turn (a documented mechanism divergence from v4's
client-computed turn; cleared on send), the latch-keyed speaking-as
turn-follow, and the seed-once `impersonationSync` port (list re-applied
when non-empty; speaking-as seeded once; the live `chat()` fallbacks
removed — the stale-refetch clobber shown red pre-fix), with the reload
beat ACTIVATE-AT-UNIFY flipped live ∥ **P4.44** closed three standing
debts: the conversation-chunks upsert CREATE arm (minted-id normalizer
over spec-pinned literals, 9 → 11 rows), the bug-43 per-delete/overwrite
`cleanup_thumbnails` over the existing `StorageBackend` seam (both skip
sites; chat-media twins verified un-wired in v4 itself), and the provider
request-header pin (recorder + corpus at `f6eac168`, byte-identical on
every pre-existing key; post-`apply_auth` subset diff + the 8-provider
coverage floor; abort-arming deferred loud — wall-clock, unit-tier-proven).
**The §3 unification review caught one would-have-shipped spec defect:**
the seed-once parity spec was a FALSE GREEN (TanStack structural sharing
kept the deep-equal stub's reference, so the sync effect never re-fired
and the spec passed against pre-fix code); repaired with a per-fetch
`updatedAt` bump and mutation-proven both directions (65/66 red under an
unconditional re-apply, 66/66 green reverted). Gate: fmt; clippy both
feature sets; release build; **419 test binaries / 1,978 tests / 0
failed** with the round's env block; the round's seven differentials by
name `--nocapture` zero SKIP over oracles regenerated FRESH at `f6eac168`
(request-envelopes corpus byte-identical); ng test 296 files / 4,065;
ng build clean; full Playwright green zero unexplained skips (numbers in
the round record). Versions: core 0.0.518, harness 0.0.440, SPA 0.5.444;
host/web/cli/tauri unchanged.

**Next candidates, in rough value order:**

1. **The dogfood pass — long owed, still top** (no drift debt remains):
   the standing 💸 queue (unchanged from the previous round's list),
   **plus this round's live surfaces:** a real two-user-seat rotation on
   the Friday copy (post as one seat, watch the pause hand the floor to
   the other instead of the sole LLM answering), a Brahma
   budget-exhaustion run (set the budget low, watch the salvage answer
   instead of a silent hang), impersonate → reload → the overlay
   survives, and the speaking-as turn-follow in a real multi-seat room.
2. **The sweep-driver rot maintenance pass:** the six tier-1 turn
   families SKIP-masquerade in `recipe_sweep.py` (flagged at f4955e0e,
   re-confirmed at P4.D56) plus the four families named in the f4955e0e
   round record.
3. **Small follow-ups:** the P4.D54 AllLLMPause live-opener e2e beat
   (needs an all-LLM chat seeded into the e2e fixture — the D61 deferral
   names the constraint); P4.D51's bug-43 remaining note; the brahma
   budget-override fixture-residue hardening note (the D60 review's
   footnote — a reset-after-case or fixture comment).
4. **The standing pools:** `p4.9i2` (help surface — now also carrying
   `help/chat-multi-character.md` + `help/chat-turn-manager.md`),
   `p4.9l` (composer toolbar), the rest of the `p4.9h2` bucket, the
   BUILTIN TF-IDF manifest decision, the P4.D41 tier-2 item 9 fixture.

**Standing regen note:** the oracle baseline is **`f6eac168`** (v4
4.8.0-dev, adopted at the P4.D60∥P4.D61∥P4.44 unification; v4's tree was
CLEAN there at the round's regens). Oracles regenerate straight from
`~/source/quilltap-server` while HEAD stays there; pin a detached
worktree on any further drift (`oracle-regen-pinned-v4-worktree`, or
`recipe_sweep.py --v4 <pin>`). The distill-transitive TZ pins, the
committed-fixture rule, and the venue/staging rules stand unchanged.
Drift-check before every round — v4 ships daily.

## The character-archive drift catch-up campaign (planned 2026-08-10) — ROUND 1 IN FLIGHT

**v4 drifted `f6eac168` → `d553f72a`** (five commits; classification in the
status log's "Round planned" record): the complete character-archive
feature — a D23 schema change (three `characters` columns), export
fidelity + `preserveIds` (WP A2/B1), the prune-in-place archive service
with passphrase-keyed AES-GCM bundles, guards/chokepoints across the
turn/tool/mail surfaces, wipe/restore spare-bundle options, CLI
subcommands, and a full Aurora client surface — plus Bugs 52/54/55 fixes.

**A two-round campaign** (the archive service composes the export/import
substrate, so it cannot land in one parallel round — the episodic-recall
precedent):

- **Round 1 (orders committed):** P4.D62 (the export/import/file-storage
  substrate) ∥ P4.D63 (schema + guards + chokepoint + crypto/re-encrypt +
  wipe options + the two verbs refusal-armed) ∥ P4.D64 (the whole SPA,
  tombstone-read beats ACTIVATE-AT-UNIFY, action beats gated
  `CHARACTER_ARCHIVE_SERVER_LANDED`). Orders:
  `work-orders/p4.d62-export-import-archive-substrate.md`,
  `work-orders/p4.d63-archive-schema-guards-crypto.md`,
  `work-orders/p4.d64-archive-spa.md`. The Shared contract is identical
  in all three and binding.
- **Round 2 (owed after round 1 unifies; write its order at that
  /setupphase against the then-current v4 HEAD):** the archive service
  whole + rehydrate + participant flips, the two verbs un-refused, the
  files-delete `ARCHIVE_BUNDLE_HELD` guard, the export-picker archived
  filter, the three-key export carry, the CLI `db characters` family,
  the SPA gate flips + action beats, and the round's e2e archive →
  rehydrate walk.

**⚠ Standing pin until round 1 unifies:** the oracle baseline stays
`f6eac168` for all families outside these lanes; the round's own families
regenerate at `d553f72a` (pin a detached worktree on further drift —
`oracle-regen-pinned-v4-worktree`, or `recipe_sweep.py --v4 d553f72a`).
Bug 53 (file-storage reconciliation) is MOOT for v5 — the subsystem is
unported; banked in P4.D62's record. The owed dogfood pass and the other
standing candidates (previous section) queue behind the drift per standing
practice.

## The character-archive round-1 unification (P4.D62 ∥ P4.D63 ∥ P4.D64, 2026-08-11) — ROUND 1 DONE

**P4.D62 and P4.D64 CLOSED; P4.D63 OPEN at unit 7 only** (the re-encrypt
wire + differential — its status header carries the resume list). The
oracle baseline MOVES to **`d553f72a`**. Full round record in
`status-log.md`; the §3 review's six fixed findings and the seeder's
first-live-run discoveries (including the v4-side archived-seat-badge GET
bug, to be filed upstream) are recorded there and in the order headers.

**Next candidates, in rough value order:**

1. **The `ed8934f1` (Bug 56) drift catch-up** — v4 moved one commit past
   the pin during the round: `lib/mount-index/base-path-availability.ts`
   (new) + `scanner.ts` + the two mount-points routes land on the PORTED
   Scriptorium surface (the rest is Docker/CLI packaging + two help docs
   for the `p4.9i2` bank). Until absorbed, pin `d553f72a` for any
   mount-points-family regen.
2. **Round 2 of the character-archive campaign** (write its order at that
   /setupphase, against the then-current v4 HEAD): the archive service
   whole (prune-in-place, bundle write/verify, tombstone commit ordering,
   participant flips, rehydrate + re-chunk/re-embed), the two verbs
   un-refused (shapes pinned in round 1's Shared contract), P4.D63 unit
   7's re-encrypt wire + differential, the files-delete
   `ARCHIVE_BUNDLE_HELD` guard, the export-picker archived filter, the
   three-key export carry (pin v4's key-presence-by-schema-vintage
   semantics), the CLI `db characters` family, the SPA gate flip
   (`CHARACTER_ARCHIVE_SERVER_LANDED`) + the four action beats, plus the
   banked D62/D63 items named in their headers (the preflight
   error-propagation, the duplicate+preserveIds oracle arm, the
   repo-wrapper tier-2 case, the four archived-arm fixture extensions).
3. **The owed dogfood pass** (standing queue unchanged) — now also
   gaining round 1's live surfaces: the roster toggle over a real
   archived character (once round 2 can make one), the preserveIds
   import arms, and the Bug-55 404s on a real dangling row.
4. **The sweep-driver rot maintenance pass** (standing) — select-speaker
   re-confirmed the SKIP-masquerade shape this round; the recipe-prose
   shell-keyword trap is a new entry for it.

**Standing regen note:** the oracle baseline is **`d553f72a`** (adopted at
this unification). v4 HEAD is `ed8934f1`, ONE commit past it — see
candidate 1; pin a detached worktree at `d553f72a` for mount-points
families until it is absorbed (`oracle-regen-pinned-v4-worktree`, or
`recipe_sweep.py --v4 d553f72a`). The distill-transitive TZ pins, the
committed-fixture rule, and the venue/staging rules stand unchanged.
Drift-check before every round — v4 ships daily.

## The character-archive round-2 + Bug-56 unification (P4.D65 ∥ P4.D66 ∥ P4.D67, 2026-08-11) — ROUND 2 LANDED

**P4.D66 and P4.D67 CLOSED; P4.D65 OPEN at its resume list** (unit 1 —
the service + verbs + differential — unified; its status header carries
the precise OPEN list, which now includes the §3 review's owed corpus
arms). **The oracle baseline MOVES to `ed8934f1` and the Bug-56 drift
debt is CLEARED.** The archive lifecycle is LIVE end-to-end (SPA beats
10/10; the CLI family Tier R 188/0; the CLI's REST edge added at
unification after the §3 review caught the round's cross-lane blind
spot). One deliberate divergence shipped pending the human's
confirmation: the preserveIds preflight dedupes carried blob ids
(v4 cannot rehydrate a twice-linked-blob vault; v4-side fix queued).
Full round record in `status-log.md`.

**Next candidates, in rough value order:**

1. **Finish P4.D65** (its status header's OPEN list): the re-encrypt
   wire (which CLOSES P4.D63), the files-delete `ARCHIVE_BUNDLE_HELD`
   guard, the export archived filter, the non-null export-carry arm,
   the banked round-1 oracle arms, and the §3-owed corpus arms (the
   positive `background_jobs` leg, `avatarOverrides` keep,
   `archivedAvatarFileId`, the passphrase-400 arms, the
   twice-linked-blob differential pin).
2. **The owed dogfood pass** (standing queue) — now also gaining round
   2's live surfaces: archive → rehydrate on the Friday copy, the CLI
   family against real data (offline export of a real bundle), the
   Bug-56 409 on a genuinely unreachable store.
3. **The two v4-side filings** (human): the rehydrate blob-collision
   fix and round 1's archived-seat-badge GET gap.
4. **The sweep-driver rot maintenance pass** (standing).

**Standing regen note:** the oracle baseline is **`ed8934f1`** (adopted
at this unification; v4 HEAD == baseline, tree clean at the gate). No
drift debt remains; oracles regenerate straight from
`~/source/quilltap-server`; pin a detached worktree on any further
drift (`oracle-regen-pinned-v4-worktree`, or `recipe_sweep.py --v4
<pin>`). The distill-transitive TZ pins, the committed-fixture rule,
and the venue/staging rules stand unchanged. Drift-check before every
round — v4 ships daily.

## The P4.D65-finish + sweep-rot round (P4.D65-resumed ∥ P4.45, 2026-08-11) — UNIFIED

**The oracle baseline MOVES to `de9f70bf` and the Bug-57 drift debt is
CLEARED** (v4 converged onto this port's dedupe; the pins retired to plain
equalities, the fixture grew the twice-linked-blob shape as a
mutation-proven equality arm). **P4.D63 CLOSED** (the re-encrypt wire
landed: the sweep at the ChangePassphrase dispatch arm, `{success,
archives}` on the wire the P4.D64 card already reads, a 6-case
differential + live web wire test — and the differential's first run
caught a real `write_blocking` panic on the async path). **P4.45 CLOSED**
(the sweep driver classifies recipes by indentation, refuses
unattributable run lines, deletes jest-convention oracles too; 32 headers
scoped; 39 repaired families proven via the committed `--run-all`
artifact). Also landed: the `ARCHIVE_BUNDLE_HELD` files-delete guard, the
export picker's archived filter, and the non-null export-carry arm that
caught the stale `schema-key-order.json` (exported character records had
the three archive keys in the wrong slot). The §3 review fixed the
sweep's upload-failure reason (v4's `uploadRaw` wrapper — a contractual
UI string) and the holder-lookup 500 leak before merge. Full round record
in `status-log.md`.

**Next candidates, in rough value order:**

1. **The owed dogfood pass** (standing queue, several rounds deep) — now
   also gaining this round's live surfaces: a real passphrase change over
   an archive library (the re-encryption sweep + the settings card's
   `archives` summary), the held-bundle delete refusal + `force=true`,
   and the archive → rehydrate walk on the Friday copy, plus the standing
   💸 queue (OpenRouter pricing, vision, extraction, Almanack, Taboo,
   impersonation, Brahma salvage, …).
2. **P4.D65 items 5–6** (the small open remainder; its header enumerates
   them): the banked round-1 tier-2 arms (preflight error propagation,
   the four archived-character arms, the `setParticipantStatus` wrapper
   case) and the §3-owed corpus arms (the positive `background_jobs` leg,
   `avatarOverrides` keep, `archivedAvatarFileId`, the passphrase-400
   arms, prune-re-run).
3. **The two v4-side human filings:** the archived-seat-badge GET gap
   (round 1's) — the Bug-57 filing+fix is DONE (`de9f70bf`).
4. **The banked `external_tmp_input` driver extension** (P4.45 unit-4
   record carries the candidate list and the direction-ambiguity blocker).

**Standing regen note:** the oracle baseline is **`de9f70bf`** (adopted at
this unification; v4 HEAD == baseline, tree clean at the gate). No drift
debt remains; oracles regenerate straight from `~/source/quilltap-server`;
pin a detached worktree on any further drift
(`oracle-regen-pinned-v4-worktree`, or `recipe_sweep.py --v4 <pin>`). The
sweep driver is now the sanctioned path for family regens (`--run` /
`--run-all --families`); the distill-transitive TZ pins, the
committed-fixture rule, and the venue/staging rules stand unchanged.
Drift-check before every round — v4 ships daily.

## The `03154b72` 4.8.1-release drift catch-up round — UNIFIED (2026-08-12)

**P4.D68 ∥ P4.D69 ∥ P4.D70, plus the parallel wardrobe-flow deflake — ALL
CLOSED; the oracle baseline MOVES to `03154b72` and the drift debt is
CLEARED.** v4 released 4.8.0 + 4.8.1 (main is now `4.9.0-dev.0`, and v4
develops on TWO branches — main + `bugfix`). Landed: the bug-60 one-file
dbkey port (the phantom `quilltap-llm-logs.dbkey` write shed, the
cross-compat oracle grown to both directions with cross-side
mutation-proven tripwires), the bug-59 measured convergence + the
`failed_gate_probe_seeds_nothing` pin, the bug-58 NO-PORT with the full
writable-open lock enumeration, the repo-wide spelling sweep wired into
the workspace gate, the `db characters` completion templates (Tier R
red-first → 188/0), the standalone streaming indicator + the About
release-freshness mirror, and the `wardrobe-flow` `set_all` deflake
(finding #78 → **v4 Bug 61**, filed). Round record: `status-log.md`.

**Next candidates, in rough value order:**

1. **The boot open-before-lock reshape (the P4.D68 escalation — a real
   data-safety finding, its own small order):** v5's `boot_ready` runs
   `Db::open` (writable — `journal_mode = TRUNCATE` header writes on all
   three partitions) BEFORE `HostAssembler::assemble` acquires the
   instance lock; v4 locks before `new Database`. In the contended case
   (bug 58's Ignite scenario) v5 performs three unlocked journal-mode
   writes against a database another process holds, then refuses. All ROW
   writes are behind the lock; the exposure is the open sequence itself +
   the fresh-`Setup` provisioning corner. The fix is moving lock
   acquisition ahead of `Db::open` in the engine boot path (a host-seam
   reshape). Full record: the P4.D68 order header + status-log unit 3.
2. **The owed dogfood pass** (standing queue, several rounds deep) — now
   also gaining the bug-60 live proof (a passphrase change on the Friday
   copy leaves exactly ONE `.dbkey` file, and a pre-existing stale
   `quilltap-llm-logs.dbkey` survives untouched) on top of the
   re-encryption sweep, held-bundle guard, and the standing 💸 queue.
3. **P4.D65 items 5–6** (the small open remainder; its header enumerates
   them).
4. **The two v4-side human filings** (archived-seat-badge GET gap; and
   now v4 Bug 61 — the wardrobe staged-edit race — awaits v4's fix, which
   will be a small drift round on the ported dialog when it lands).
5. **The banked `external_tmp_input` driver extension** (P4.45 unit-4
   record).

**Standing regen note:** the oracle baseline is **`03154b72`** (v4 main
HEAD, adopted at this unification; the checkout was back on main at the
gate, dirty ONLY with the v4 Bug-61 filing — docs, outside every oracle
import graph). **v4 now develops on two branches: drift-check BOTH**
(`git log <baseline>..main` AND `git log main..bugfix -- lib/ app/
packages/`) **and verify the checkout's branch before any regen**; pin a
detached worktree on any mismatch/drift/dirty
(`oracle-regen-pinned-v4-worktree`, or `recipe_sweep.py --v4 <pin>`). The
sweep driver remains the sanctioned per-family regen path; the
distill-transitive TZ pins, the committed-fixture rule, and the
venue/staging rules stand unchanged. Drift-check before every round.

## The 4.8.2/4.8.3 drift catch-up + lock-order round — UNIFIED (2026-08-14)

**P4.D71 ∥ P4.D72 ∥ P4.D73 ∥ P4.D74 ∥ P4.D75 ∥ P4.46 ∥ P4.D76 — ALL SEVEN
CLOSED; the oracle baseline MOVES to `48396682` and the drift debt is
CLEARED** (v4 HEAD `11553944` is the 4.8.4 release — tests + docs only,
zero lib/app/packages, NO-PORT; lib-identical to the baseline). Landed:
the group wardrobe tiers + bundle dissolution both sides (dogfood finding
**#78 CLOSED** — v4's Bug-61 fix ported, the race beat forces it
deterministically), the three composer features whole (smart typography,
emoji, Unicode — engines code-identical, corpora byte-replayed, the
caret-anchored typeahead built without precedent), the three
`chat_settings` columns through the D23 re-dump + boot ensure + Zod-exact
PUT arms, the P4.D68 escalation DISCHARGED (lock before ANY partition
open on boot/unlock/setup, contended proofs via WAL-parking) + setup
hardening (pepper never withheld; destructive retry refused) + the
`.dbkey` unknown-field preservation, and the SDK wire re-check
(neutrality proven; the "byte-identical outside self-dating markers"
restatement). The §3 unification review caught and fixed two
would-have-shipped bugs: the empty typeahead menu swallowing Enter
(v4 falls through — a typo'd `:smiel` + Enter would not send) and
first-run Setup dying on a missing `data/` dir (the lock reorder outran
the dir creation; every test had masked it). Round record:
`status-log.md`.

**Next candidates, in rough value order:**

1. **The owed dogfood pass** (standing queue, several rounds deep) — now
   ALSO gaining this round's live surfaces: group-held garments + bundle
   dissolution on the Friday copy, the bug-61 dialog race gone, smart
   typography + the `:`/`\` typeaheads + pickers in real writing, the
   fresh-instance Setup walk (missing-dir arm now covered by test, worth
   one human walk), the bug-60 live proof, the re-encryption sweep +
   held-bundle guard, and the standing 💸 queue.
2. **P4.D65 items 5–6** (the small open remainder; its header enumerates
   them).
3. **The v4-side human filing** (the archived-seat-badge GET gap).
4. **The banked `external_tmp_input` driver extension** (P4.45 unit-4
   record).
5. Banked smalls: the google-wire recorded-not-asserted headers (P4.D76),
   the sibling settings arms' Zod-collapse seam (P4.D73's bank), the
   `p4.9l` composer toolbar (would make the pickers reachable from the
   composer), the stale `docs/v4` API.md mirror sweep.

**Standing regen note:** the oracle baseline is **`48396682`** ("merge:
4.8.3 back into main", adopted at this unification). v4 HEAD is
`11553944` ("merge: 4.8.4 back into main") — **NO-PORT, verified**: the
delta is two composer-typeahead test files, a jest helper, and release
docs; `git diff 48396682 main -- lib/ app/ packages/` is empty, so
oracles regenerate straight from the checkout while HEAD stays there.
Drift-check BOTH branches and the checkout's branch before any regen;
pin a detached worktree on mismatch/drift/dirty. The sweep driver
remains the sanctioned regen path — **never run two sweeps
concurrently** (they race on shared /tmp oracle paths; measured this
round), and the provisioning family's two v4-side legs must run from
the v4 checkout (recipe repaired this round). The distill-transitive TZ
pins, the committed-fixture rule, and the venue/staging rules stand
unchanged. Drift-check before every round.

## The help-drift round (P4.D77 ∥ P4.D65-remainder ∥ P4.47 ∥ P4.9L) — UNIFIED 2026-08-14

All four lanes CLOSED (order status headers carry the per-lane verdicts; the
round record is in `status-log.md`). The oracle baseline MOVES to
**`24633026`** and the drift debt is CLEARED. Highlights: the server half of
v4's section-level help search is live end to end (the Guide client half
banked verbatim at `p4.9i2`, m6 row 11); the composer formatting toolbar +
v4's composer layout landed (dogfood #75 closed); the settings Zod-collapse
arms, the google auth-transport fix (`?key=` → `X-Goog-Api-Key`), and the
sweep driver's staging-dependency class all closed; the P4.D65 archive
coverage remainder landed with a stale-oracle-mock catch. The §3 review
fixed, at unification: the settings error-status split (v4's
`includes('Invalid') ? 400 : 500`) + the connection-profile duplicate 409s —
both caught by the review's new per-row error-status assert; the composer
Generate-Image disabled gate; the list-shape divergence pin; and the gate
itself caught + fixed two sweep-infrastructure defects (the fixture shield's
missing `.meta.json` sidecar copy; the driver running a committed corpus's
RECORDING stage — which clobbered and then briefly committed the google-wire
corpus before restoration).

**Next candidates, in rough value order:**

1. **The v4 Ollama "Enable Thinking" drift, already in flight** — visible as
   uncommitted WIP in v4's working tree during this round (a `think-parser`,
   provider-options schema rows, `help/connection-profiles.md`); expect the
   catch-up round the moment v4 commits it. Drift-check will catch it.
2. **The D65-5a escalation order**: the import preflight's exists-checks
   swallow repo READ errors to "id free" at TEN sites
   (`quilltap_import/mod.rs:805,826,835,845`, `entities.rs:445,532`,
   `preview.rs:124,130,148,172`) where v4 propagates and refuses the whole
   import — a real v5 defect with a live consumer (the archive service);
   the fix is core source + one planted-unreadable-table differential arm.
3. **The owed dogfood pass** — now ALSO gaining this round's live surfaces:
   the composer toolbar + v4 layout on real data (#75's acceptance look),
   the delimiter buttons over a real template, and the standing 💸 queue
   plus P4.D77's trio (the upgrade backfill on the Friday copy — reachable
   once `ensure_help_docs_synced` is wired, the `p4.9i2` seam; a real
   `help_search` section answer; the reindex/reapply riders at scale).
4. **`p4.9l2`** — the DocumentPane formatting toolbar (m6 §4 row 14b; every
   component exists, the pane's wiring does not).
5. **`p4.9i2`** — the HelpChat/Guide vertical, now carrying the enriched
   P4.D77 bank (the search route's exact snippet mechanics, the client
   debounce/stale-tagging, the help-md copy deltas).
6. Banked smalls: the settings repo-validate sibling fields
   (`imageDescriptionProfileId` / `uncensoredImageDescriptionProfileId` /
   `contextCompressionSettings` / `thinkingDisplay` — named at the P4.47
   deferral site); the stale `docs/v4` API.md mirror sweep. ~~The two
   v4-side filings~~ **FILED 2026-08-14 as v4 bugs 66 (the
   archived-seat-badge GET gap) and 67 (the source-mode send discarding
   edits)** — committed docs-only in the v4 checkout (not pushed; the
   human's tree carries the in-flight Ollama WIP). Their fixes arrive as
   ordinary drift; v5's pins flip by design when they land.

**Standing regen note (supersedes the one above):** the oracle baseline is
**`24633026`** ("feat: section-level help embeddings and content search in
the Guide", adopted at the help-drift unification). ⚠ v4's working tree
carried uncommitted Ollama-thinking WIP through this round — verify
branch + cleanliness before any regen and pin a detached worktree on any
mismatch/drift/dirt (`recipe_sweep.py --v4 <pin>`). Drift-check BOTH
branches (bugfix measured by `git diff`, never the commit list — bugs
64/65 live BELOW the 4.8.3 marker and are pre-baseline). The sweep driver
remains the sanctioned regen path — never run two sweeps concurrently;
the driver now (this round) copies `.db.meta.json` sidecars when
shielding fixtures, NEVER runs a committed-corpus family's recording
stage (recording is a by-hand act), and warns whenever a family's stages
leave tracked fixture bytes modified. The distill-transitive TZ pins, the
committed-fixture rule, and the venue/staging rules stand unchanged.
Drift-check before every round.

## The `aa464abf` drift catch-up round (P4.D78 ∥ P4.D79 ∥ P4.D80 ∥ P4.D81 ∥ P4.48) — UNIFIED 2026-08-15

All five lanes CLOSED and unified on main (the round record is in
`status-log.md`). The oracle baseline MOVES to **`aa464abf`** and the round's
drift debt is CLEARED — but see candidate 1: v4 moved again mid-round.
Highlights: the whole Ollama-thinking wire (the `<think>` stream parser, the
`think`/`num_ctx` request fields with the retry-without-think, the `toolUse`
manifest flip); bug 68's `multiCharacterPrefill` column end to end (D23
re-dump + boot ensure with the once-only backfill, the resolver, the
per-profile turn anchor, routes, export/import carry, the SPA checkbox);
greeting reasoning persisted onto the first message; the `profileParams`
consolidation — which also fixed THREE real pre-existing v5 defects (the
Salon primary stream had NO `modelParams` twin, the Carina temperature read a
nonexistent key, and the SPA's profile save silently dropped every
non-sampling `parameters` key); bugs 66/69 absorbed (the archivedAt
enrichment + flipped beat; the rehydrate digest self-heal) with bug 67 a pure
convergence (v4 adopted v5's pinned source-mode behavior); and the P4.48
escalation closed with its premise REFUTED — v4 swallows those read errors
too (`safeQuery` fallback mode), so the DB-read-error refusal landed as a
ruled divergence and the overlay leg as the byte-match fix. The §3 review
found NO blocking findings (a first); the gate caught and fixed three recipe
defects + two stale lane-pin paths.

**Next candidates, in rough value order:**

1. **The `f933ba9c` bug-70 drift catch-up** — v4 moved DURING the round:
   context budget honors the profile's Max Context (`resolveContextWindow`
   single-sourcing, `computeSafeInputLimit`, the new `turn-extras.ts`
   tool-schema/agent-splice token accounting; touches context-manager,
   orchestrator, context-builder, token-counter, model-context-data —
   ported surfaces all). v5's `context_budget.rs:92` already resolves
   profile-max-context-first, so part is likely v4 converging on v5's
   shape — MEASURE (`convergence-lane-measure-dont-assume`). **Pin
   `aa464abf` for every regen until this lands.**
2. **The owed dogfood pass** — now also gaining this round's live surfaces:
   a real Ollama thinking run (ticked → fold; prefill unticked → v4's
   bug-68 verification shape), a real `num_ctx` send, the parameters-bag
   round-trip on real data, a clobbered-digest rehydrate if the Friday copy
   has one; plus the whole standing 💸 queue.
3. **The v4-side filing** owed from P4.48: the import preflight's swallowed
   DB read errors (v4 proceeds into a partial apply).
4. **`p4.9l2`** (DocumentPane formatting toolbar), **`p4.9i2`** (the
   HelpChat/Guide vertical + its enriched bank).
5. Banked smalls: the settings repo-validate sibling fields (named at the
   P4.47 deferral site); the stale `docs/v4` API.md mirror sweep; **NEW
   this round** — v5's `onProviderChange` never re-seeds
   `supportsImageUpload` from the capability map on a provider switch
   (v4's `handleProviderChange` does; pre-existing, found by the §3
   review); the optionsSchema machinery question (delete-not-extend the
   hardcoded Enable Thinking row when it ever lands).

**Standing regen note (supersedes the one above):** the oracle baseline is
**`aa464abf`** (2026-08-15, v4 main — "fix: archived-seat badge (66),
source-view send (67), archive digest clobber (69)"), adopted at this
round's unification. ⚠ v4 HEAD is ALREADY PAST it (`f933ba9c`, candidate 1)
— **pin a detached worktree at `aa464abf` for every regen** until the
catch-up lands (`recipe_sweep.py --v4 <pin-path>`; remember ALL THREE
symlink classes: root node_modules, `packages/quilltap/node_modules`, the
`plugins/dist/*/node_modules` dirs). Drift-check BOTH branches every round
(bugfix measured by `diff`, never the commit list). The sweep driver remains
the sanctioned per-family regen path — never run two sweeps concurrently.
The distill-transitive TZ pins, the committed-fixture rule, and the
venue/staging rules stand unchanged.

---

## After the `93ed8abf` drift round (P4.D82 → P4.D83 stacked ∥ P4.D84) — UNIFIED 2026-08-16

The whole three-commit drift absorbed (bug 70's context budget +
turn-extras accounting; the sampling resolver at all five call sites —
the corpus found the Carina fifth; the profile-parameters wire for
Ollama/OAC/DeepSeek/Z.AI with OAC tool calling on both paths; the
per-profile Ollama request timeout; optionsSchema served for all eight
declaring providers and rendered by the SPA's new schema-driven panel,
retiring the hardcoded Enable Thinking row and the P4.D81 machinery
deferral). The §3 review fixed five real findings before merge — headline:
the OAC `chat_template_kwargs` array-string omission (corpus-blind,
mis-documented on both sides) and the half-ported pre-send validation
(v4's client-facing `validating`/`warning` statuses now emitted and
comparands). Round record: `status-log.md`.

**Next candidates, in rough value order:**

1. **The owed dogfood pass** — now further grown: a real local-model turn
   showing the profile's Max Tokens / Top P on the wire, the schema-driven
   options panel on real data, an OAC tool call against a real
   llama-server, the Ollama per-profile request timeout on a cold large
   model; plus the whole standing 💸 queue (Almanack real-data report,
   Taboo live turn, OpenRouter pricing, the vision send, P4.D49
   budget/attribution, P4.D77's trio, #75's acceptance look, the
   aa464abf-round Ollama-thinking proofs).
2. **The v4-side filing** owed from P4.48: the import preflight's
   swallowed DB read errors (v4 proceeds into a partial apply).
4. **`p4.9l2`** (DocumentPane formatting toolbar), **`p4.9i2`** (the
   HelpChat/Guide vertical + its enriched bank).
5. Banked smalls: the `Non-image attachments:` line under the provider
   select (NEW this round — the client attachment table it needs now
   exists at `apps/web/.../providers/attachment-support.ts`); the settings
   repo-validate sibling fields (P4.47 deferral site); the stale
   `docs/v4` API.md mirror sweep; the reroute LOOKUP-half residue
   (`model_context_limit` is host-resolved for the original profile —
   recorded at `orchestrator.rs`'s build-args site; fixing it means
   moving the registry lookup into the engine).
5. Carried riders (for their future lanes, not standalone orders):
   `external-prompt-generator.service.ts` (the `f933ba9c` + `d89babc4`
   edits) and `encodeDebugInfo` (`streaming.service.ts:487`, the fifth
   `resolveSamplingParams` site — v5 emits no debug frame).

**Standing regen note (supersedes the one above):** the oracle baseline is
**`93ed8abf`** (2026-08-15, v4 main — "fix: local providers send the
profile's parameters; OAC can call tools (bug 71)"), adopted at this
round's unification; the drift debt is CLEARED at the pin. Pin a detached
worktree at `93ed8abf` for every regen if v4's checkout moves
(`recipe_sweep.py --v4 <pin-path>`; ALL THREE symlink classes: root
node_modules, `packages/quilltap/node_modules`, the
`plugins/dist/*/node_modules` dirs). Drift-check BOTH branches every round
(bugfix measured by `diff`, never the commit list). The sweep driver
remains the sanctioned per-family regen path — never run two sweeps
concurrently. The distill-transitive TZ pins, the committed-fixture rule,
and the venue/staging rules stand unchanged.

---

## The `d123658d` connection-profile-editor drift round (P4.D85 ∥ P4.D86) — UNIFIED on main (2026-08-17)

Both lanes CLOSED; the oracle baseline MOVES to **`d123658d`** and the
drift debt is CLEARED at the pin (v4's one commit past it, `9c01fa99`,
is classified NO-PORT below). v4's `d123658d` fixed bugs 72/73 — this
port's own dogfood findings #87/#88 coming back — plus bug 74 (profile
tagging had never worked, three layers deep).

- **P4.D85 (server):** the `resolve_editor_tags` flat-tag resolver with
  both `get-tags` call sites through it (the characters convergence
  proven output-neutral), the three profile-tag verbs with v4's exact
  bodies and repo semantics, the settings-routes family 108 → 128 cases
  over a fixture that finally carries tags (order-preservation /
  drop-missing / omitted-`visualStyle` all measurable; the three v4
  action-gate arms RECORDED-ONLY with an exact-count guard), and a real
  v5 divergence fixed: cleared PUT keys now answer as explicit `null` in
  schema position, as v4's in-memory-merge does (`restore_cleared_nulls`,
  five corpus arms, mutation-proven). The `enrich_with_tags` `{id,name}`
  narrowing closed (the vacuous-corpus class). `auto-configure` is
  UNPORTED by ratified deviation (no action surface exists to refuse
  from); its sentence is pinned by a recorded row.
- **P4.D86 (SPA):** the `ProviderNumberField` draft/`syncedFrom`
  machinery with default-as-placeholder (the naive re-sync spelling
  mutation-proven RED), the `outboundBaseUrl` chokepoint over v5's own
  FIVE sites with the always-send save body (the P4.D84 recorded
  number-clear divergence re-measured and RETIRED), the profile tag
  surface in its fixed form (card pills off the `{tagId, tag}` envelope,
  the modal editor with v4's immediate persistence and toast sentences),
  the banked `Non-image attachments:` line, and v4's own verification
  walk as three e2e beats (the tag beat activated at unification).

**The §3 review's catch:** P4.D86's `EnrichedProfileTag` doc + type
carried a claim its own sibling made stale mid-round (the `{id,name}`
narrowing P4.D85 closed) — retyped to the full `TagDto` with the doc
rewritten, the cross-lane blind-spot class. **NO-PORT:** v4 `9c01fa99`
(the MODERN sample-prompt trio + model-specific prompt rewrites) touches
only `plugins/dist/qtap-plugin-default-system-prompts/**` +
`help/prompts.md` — plugin/help content v5 consumes from the instance at
runtime, zero `lib/`/`app/` code; `d81ccc17` is the bug filings,
docs-only.

Gate + versions: the round record in `status-log.md`.

**Next candidates, in rough value order:**

1. **P4.49 — v4's file logging** (`work-orders/p4.49-file-logging.md`,
   ORDERED 2026-08-18 from dogfood finding #93; **the human raised its
   priority explicitly**: every v5 diagnosis currently starts with less
   instrumentation than v4 had, and the walk that found it immediately hit
   a warning readable only by whoever was watching the terminal). v4
   writes `combined.log` + `error.log` into the instance's `logs/` with
   rotation and a startup sweep for iCloud/Finder conflict files; v5 has
   no file appender at all. Small and disjoint — 194 lines of v4 to port,
   its own 36-case test file as the parity corpus, no `api/**`, no
   `services/**`, no `apps/web/**`, no oracle regen — so it composes with
   any drift lane. One ruling left for the human (the default; see the
   order's unit 6).
2. **The owed dogfood pass** — Parts C/D of the paused 2026-08-16 walk
   (the bug-70 context-budget legs; the standing 💸 queue: the Almanack
   real-data report, the live Taboo turn, OpenRouter pricing with a real
   key, the vision send, the P4.D49 budget/attribution proofs, the
   orphan-reaper boot heal, the quote-delimiter roleplay template, OAC
   tools against a real llama-server) — now further grown by THIS round's
   surfaces: profile tags end-to-end on real data, the cleared-number
   heal on a real profile, the poisoned-base-URL heal on any pre-bug-73
   row in the Friday copy.
3. **The v4-side filing owed from P4.48** (the import preflight's
   swallowed DB read errors).
3. **`p4.9l2`** (DocumentPane formatting toolbar), **`p4.9i2`** (the
   HelpChat/Guide vertical + its enriched bank, incl. the two banked
   help-doc drift edits).
4. Banked smalls: the settings repo-validate sibling fields (P4.47
   deferral site); the stale `docs/v4` API.md mirror sweep; the reroute
   LOOKUP-half residue (`orchestrator.rs` build-args site); the P4.D85
   lead — v4's in-memory-merge update answer is a BASE-repository
   property, so the cleared-null divergence may exist on OTHER update
   surfaces whose v4 twin clears columns (unmeasured either way).
6. Carried riders: `external-prompt-generator.service.ts` (P4.D82) and
   `encodeDebugInfo` (P4.D83) for their future lanes.

**Standing regen note (supersedes the one above):** the oracle baseline
is **`d123658d`** (2026-08-17, v4 main — "fix: connection-profile editor
bugs 72, 73 and 74"), adopted at this round's unification; the drift
debt is CLEARED at the pin, and v4's `9c01fa99` (sample-prompt content,
NO-PORT) is dispositioned — a drift check landing on it alone owes
nothing. Pin a detached worktree at `d123658d` for every regen if v4's
checkout moves (`recipe_sweep.py --v4 <pin-path>`; ALL THREE symlink
classes). Drift-check BOTH branches every round (bugfix measured by
`diff`, never the commit list). The sweep driver remains the sanctioned
per-family regen path — never run two sweeps concurrently. The
distill-transitive TZ pins, the committed-fixture rule, and the
venue/staging rules stand unchanged.

## The `979652a9` drift round (P4.D87 ∥ P4.D88 ∥ P4.D89 ∥ P4.D90 ∥ P4.49): ORDERED 2026-08-18

The drift check found v4 EIGHT commits past `d123658d` (HEAD `979652a9`,
tree clean; `bugfix` measured by `diff` — its only unabsorbed content is
`009c49b2`, a test-only typeahead deflake, NO-PORT). Five behavior
commits land on ported surfaces; the round is a drift catch-up plus the
already-ordered P4.49. Orders (each carries its own survey-verified
starting points, dated 2026-08-18):

- **P4.D87** (`work-orders/p4.d87-wardrobe-hair-core.md`) — the hair
  slot's server half (`4423ad10`): the slot-meta registry over v5's TEN
  hard-coded four-slot copies, the `reportWhenEmpty` contract, nudity
  over clothing slots only, both avatar branches (the `accessories ||
  hair` guard), the prompt bytes, tool definitions, the outfit hash's
  accepted invalidation, import/export/restore carry — PLUS Bug 75
  (`40d507cc`'s one ported surface: the `.qtap` composite
  `componentItemIds` leaf-first remap; **v5 has the bug today**,
  `quilltap_import/characters.rs:429-456`). Owns all
  `quilltap-core`/`quilltap-harness` edits and ~20 family regens
  (`chats_outfits_tier2` is guaranteed red until its oracle regenerates
  — by design).
- **P4.D88** (`work-orders/p4.d88-wardrobe-hair-spa.md`) — the hair
  slot's SPA half: build the registry (v5 has none), consolidate the 11
  hard-coded sites, the rose badge tokens, the `?? []` forward-compat
  guard, the Green Room preview; the live beat gated
  `P4D87_HAIR_SLOT_LANDED`. Owns `app/wardrobe/**` +
  `core-contract.ts` this round. Shared contract (the registry rows +
  wire shapes) binding and identical with P4.D87.
- **P4.D89** (`work-orders/p4.d89-client-bugs-76-77.md`) — bug 76
  (`8bd802a3`: the `outboundApiKeyId` chokepoint over v5's FIVE outbound
  sites, always-send `|| null`, the `savedTakesApiKey ?? true` twin;
  v4's 7-case suite mirrored 1:1; **closes dogfood finding #90**) + bug
  77 (`25767c0f`) — where the survey found **v5 never ported the
  tool-execution notice at all** (settled-only toasts substituted), so
  the lane lands the surface in its fixed form (single-door publish,
  self-owned 6 s lifetime, close button, `role="status"`), retiring the
  invented toasts.
- **P4.D90** (`work-orders/p4.d90-workspace-tab-refresh.md`) — the
  workspace tab re-activation refresh (`979652a9`): the visibility
  token + `onTabActivated` primitive, the kind→query-prefix map over
  v5's fragmented keys (split spellings invalidated on BOTH sides,
  recorded), `{silent}` re-loads on the hand-rolled views, v4's
  deliberately-untouched roster (+ v5-only `salon-new` ruled into the
  editors bucket by v4's own reasoning), v4's 8-assertion parity spec
  mirrored. The `wardrobe-control-dialog.ts` hook is an AT-UNIFY edit
  (P4.D88 owns the file).
- **P4.49** (`work-orders/p4.49-file-logging.md`, previously ordered;
  baseline note updated — the drift does not touch `lib/logger.ts`) —
  runs beside the drift lanes; the only other crate-touching lane
  (`quilltap-web`/`quilltap-cli`).

**NO-PORT dispositions this round:** `dd3616a1` (docs + plugin-dist
rebuild noise — `git show --stat -- lib/ app/ packages/quilltap/` is
EMPTY; its wardrobe `help/*.md` edits ride the `p4.9i2` bank, and
`help/character-editing.md` is touched by BOTH `4423ad10` and
`dd3616a1` — take in order); `8fe63c4f` (the bug-76 filing, docs);
`3d391ac6` (merge); `009c49b2` (bugfix, test-only). **Banked riders:**
the REST of `40d507cc` + `4423ad10`'s generator/image-analysis hair
edits ride the future generators lane (verified unported: no
wizard/optimizer/summon/ai-import surface exists in v5; Summon is a
named SPA refusal stub).

**Execution shape:** all five lanes in parallel, each in its own
worktree per the `carryout` skill; version bumps — D87: core+harness
(+host if needed); P4.49: web+cli; D88/D89/D90: SPA (unifier recounts).
Regens pin a detached v4 worktree at `979652a9`. The owed dogfood pass
(candidate 2 above) stays queued behind the round and gains its
surfaces (the hair slot end-to-end, the healed api-key save, the notice
lifetime, tab-refresh on real data, the first greppable `combined.log`).

**ROUND CLOSED — UNIFIED on main 2026-08-18; ALL FIVE LANES CLOSED; the
oracle baseline MOVES to `979652a9` and the drift debt is CLEARED.** The §3
review found no blocking findings; its one substantive catch (the bug-77
turn-end WIRING unpinned — specs drove the private method, not the send
path) was fixed + mutation-proven on the unify branch. Three order premises
were refuted by measurement in-lane and stand as recorded: the empty-hair
SILENCE mandate (v4's components render "Empty"/"nothing" — reportWhenEmpty
is a lib/-only rule), the retire-the-toasts premise (they are v4's own,
raised alongside the notice), and the unknown-provider key arm (the
displayability filter still applies once keys have loaded). Gate: the round
record in `status-log.md`. TWO v4-side filings owed: the avatar-crash bug
on pre-hair `equippedOutfit` rows (D87's convergence tripwire armed).

**Next candidates, in rough value order** (updated at the `c6ff8051`-round
unification, 2026-08-19 — that round closed P4.D91 + P4.D92, absorbing v4's
bugs-78/79 convergence and the bug-80 project backdrop; the two v4-side
filings from candidate 2 of the previous list are DISCHARGED — filed AND
fixed by v4):

1. **The owed dogfood pass** — Parts C/D of the paused walk + the standing
   💸 queue: the P4.49 acceptance run (the bug-70 warning grepped out of
   `combined.log`), the bug-76 poisoned-row heal, the notice lifecycle on
   a real image turn, the hair slot end-to-end (worn hairdo, rose badge,
   avatar regen), tab re-activation freshness — and from the `c6ff8051`
   round: a real pre-hair chat row's outfit surviving a wardrobe write
   (the bug-78 read-repair on Friday-vintage data), a failed import
   naming its dropped items, and a project's story background painting on
   the workspace backdrop (`latest_chat` mode) on the Friday copy.
2. **`p4.9l2`** (DocumentPane formatting toolbar), **`p4.9i2`** (the
   HelpChat/Guide vertical + its enriched bank, now incl. the wardrobe
   help drift from `4423ad10`/`dd3616a1` and `help/story-backgrounds.md`
   from `c6ff8051`).
3. Banked smalls: the split-query-key-spelling consolidation (D90's map
   doc names every file both sides); the settings repo-validate sibling
   fields; the stale `docs/v4` API.md mirror sweep; the reroute
   LOOKUP-half residue; the P4.D85 cleared-null LEAD on other update
   surfaces.
4. Carried riders: the generators lane bank (external-prompt-generator,
   `encodeDebugInfo`, the `40d507cc` taxonomy + `4423ad10` hair edits for
   wizard/optimizer/ai-import/image-analysis).

**The `9125f492` drift round (P4.D93 ∥ P4.D94) — UNIFIED on main
2026-08-19; BOTH LANES CLOSED; the oracle baseline MOVES to `9125f492`
and the drift debt is CLEARED.** Gate: 439 test binaries / 2,231 / 0 with
fresh oracles at the pin; the eight moved families by name zero SKIP;
clippy both feature sets; release build; ng 331 files / 4,915; full
Playwright 229/229 zero skips. The §3 review read the whole combined diff
and found no blocking issues. **P4.50 (the `DbError::Key` split) ran
stacked as ordered and UNIFIED same day (2026-08-19): finding #96 FIXED —
`DbError::Internal` at 243 of 246 sites, the two genuine key wraps held by
the executable `db_error_key_guard` census, the restore-family
leaked-prefix mask retired (warnings byte-compare whole), no observable
byte moved (27 From-shims all reach the variant through catch-alls). Gate:
440 binaries / 2,236 / 0; both moved families by name zero SKIP; Playwright
229/229. §3: no blocking findings (the migration audited mechanically —
every hunk a pure rename; the literal multiset moved by exactly one, the
retired strip helper, bytes identical). Round record: `status-log.md`.**
**Next candidates: the owed 💸 dogfood items** (a real bearer-token
OAC endpoint, a Qwen-template model surviving turn 2, a candid
story-background prompt on a dangerous chat, the failed-turn
`combined.log` look), then `p4.9l2`/`p4.9i2` and the banked smalls per
the standing list above. Maintenance riders: the two
`W=` self-clobbering recipe headers (`carina_memory_extraction` /
`carina_query`), the sweep driver's exit-0-on-unknown-family wart. The
original plan paragraph follows for the record.

**The `9125f492` drift round (P4.D93 ∥ P4.D94, then P4.50 stacked) —
PLANNED 2026-08-19.** Candidate 1 above (the owed dogfood pass) RAN on
2026-08-19 and is discharged (two v5 findings, v4 bugs 81/82 filed — and
v4 fixed both the same day, which with the Lantern commit is this round's
drift). Three v4 commits past `c6ff8051`: `decd8ef9` (the story-background
candid/concealment selection — behavior change on ported surfaces),
`93bd3e7c` (the bug filings, docs, NO-PORT), `9125f492` (the bugs-81/82
fix — v4 converging onto this port's own filings; `acceptsApiKey`
end-to-end + the leading-system-message fold). The round:

- **P4.D93** (`work-orders/p4.d93-oac-api-key-and-system-fold.md`) —
  bugs 81/82: the manifest `acceptsApiKey` flag + predicate pair, the
  shared key resolver at the two Brahma sites (help-chat leg banked to
  `p4.9i2`; the spine site measured, not assumed), the leading-system
  fold in the Ollama + OAC builders only, the settings SPA half, the
  request-envelopes corpus grown (incl. the DeepSeek no-fold regression
  guard).
- **P4.D94** (`work-orders/p4.d94-lantern-uncensored-target.md`) —
  `decd8ef9`: the seven-constant prompt split through the generator with
  the concealed path byte-identity-pinned, the handler flag + retry
  carry, the reroute candid re-craft as a story-only hook on the shared
  reroute machinery (avatar differential as the guard), the story
  fixture grown dangerous-chat coverage + full corpus re-record.
- **P4.50** (`work-orders/p4.50-db-error-kind-split.md`) — dogfood
  finding #96: the `DbError::Key` catch-all (246 construction sites)
  split to a bare-message `Internal` variant with a census, an inverted
  differential obligation (no v4-pinned byte moves), and a regrowth
  guard. ⚠ Runs STACKED after the two drift lanes unify — it touches
  their files.

The two drift lanes are disjoint (ownership tables identical in both
orders; no shared contract). At unification the baseline moves to
`9125f492`.

**The `c8a3cf77` per-turn-summaries round (P4.D95 ∥ P4.9L2 ∥ P4.51):
UNIFIED on main (2026-08-20) — ALL THREE CLOSED; the oracle baseline MOVES
to `c8a3cf77`.** ⚠ v4 moved AGAIN mid-round: `e22f7b36` ("feat(salon):
anti-chorus discipline for multi-character scenes") is one commit past the
pin — **the drift catch-up is the top next candidate; pin `c8a3cf77` for
every regen until it runs.** The §3 review fixed the recall-config
invalid-value divergence (200-silent-keep → v4's 400, three oracle arms
incl. a writes-nothing composite); the gate's first by-name family run
caught the `housekeeping_config_set` fixture-vintage standing red (now a
RULED VINTAGE ROW with a repair tripwire — the `memories-{main,mount}.db`
widening is a named maintenance item, the pair being shared) and the oracle
runner's record shaper dropping the composite's `storedAfter`. Gate: seven
families by name over fresh pinned oracles zero SKIP; 440 binaries / 2,236
/ 0; clippy both feature sets; release build; ng 332 / 4,929; full
Playwright **232/232 zero skips**. Versions: core 0.0.591, harness 0.0.511,
SPA 0.5.526. **Next candidates:** the `e22f7b36` anti-chorus drift
catch-up; the owed 💸 dogfood queue (now + D95's live proof — the per-turn
list refreshing between folds with zero extra embedding calls);
`p4.9i2` (its bank grew the `memory-recall-relevance` help section); the
maintenance bank (the memories fixture widening, P4.51's three driver
follow-ups, the banked smalls per the standing list). Round record:
`status-log.md`. The original plan paragraph follows for the record.

**The `c8a3cf77` per-turn-summaries round (P4.D95 ∥ P4.9L2 ∥ P4.51) —
PLANNED 2026-08-20.** Drift-checked at planning: v4 moved two commits past
`9125f492` — `870a57fa` ("Per-turn conversation summaries with embedded
vector reuse (#38)", a behavior change squarely on the ported memory/context
spine: `searchMemoriesSemantic`, the vault conversation-summary search,
`runPreContextPreCompute` → `buildMessageContext` → `buildContext`, the
consolidated whisper, the retro mini-recap, the fold refresh, instance
settings, the memories API — no schema change, so no D23 re-dump) and
`c8a3cf77` (version-bump-only, NO-PORT). `bugfix` measured by `diff`:
nothing new beyond the test-only `009c49b2`; checkout on `main`, clean.
The round:

- **P4.D95** (`work-orders/p4.d95-per-turn-conversation-summaries.md`) —
  the whole `870a57fa` drift: the `memoryRecall.perTurnConversationSummaries`
  setting end-to-end (defaults, reader/writer, the recall-config verbs,
  the SPA recall card with v4's strings), the `captureQueryEmbedding`
  hook with its exact firing semantics, `precomputedEmbedding` on the
  vault search + the ramp-constant consolidation, the proactive vector
  thread (both return paths), and the build-context cadence whole
  (fold-whisper dedup via the backwards stop-at-first scan, the shared
  whisper target scope, recap stand-down, no-vector sit-out, the retro
  mini-recap's both-lists filter, `debugRelevantConversations`). Six
  named families grow or re-run; v4's three new test files are the
  oracle models.
- **P4.9L2** (`work-orders/p4.9l2-document-pane-toolbar.md`) — the
  m6 §4 row-14b named gap: the DocumentPane formatting toolbar (the
  `.qt-doc-toolbar` mount mirroring v4's `DocToolbar` 1:1, no Nar
  button, source branch on THIS pane's textarea, the shared
  `showSource` signal, `roleplayTemplateId` from the Salon only), with
  the two document-flow beats extended live. SPA-only.
- **P4.51** (`work-orders/p4.51-sweep-maintenance-smalls.md`) — the two
  standing maintenance riders: the `W=` self-clobbering recipe headers
  (`carina_memory_extraction` / `carina_query` tier-3s, proven by
  end-to-end driver runs) and the sweep driver's
  exit-0-on-unknown-family wart (nonzero + named error + self-test arm).

All three lanes are disjoint (ownership tables identical in the three
orders; no shared contract). Harness and the SPA are each bumped by two
lanes — the unifier accumulates. At unification the oracle baseline moves
to `c8a3cf77`. Deliberately left out: the owed 💸 dogfood items (run via
`/dogfood`, not a work order), `p4.9i2` (queued; gains the
`help/memory-recall-relevance.md` bank from P4.D95), and the candidate-3
banked smalls (the split-query-key spelling sweep and the cleared-null
LEAD would collide with P4.D95's SPA/settings surfaces — next round).

**Standing pre-beta gate (not a candidate — do not value-order it against
drift lanes).** One item is parked deliberately *outside* the candidate list
above, to run when parity work is winding down and **before the first build
anyone outside this repo installs**:

- **PB1 — the product version manifest**
  ([`work-orders/pb1-product-version-manifest.md`](./work-orders/pb1-product-version-manifest.md)).
  The canonical `5.0.0-dev.N` in `[workspace.package]`, inherited by
  host/web/cli/tauri only (never the pinned sys crate, never the
  core/harness ledgers); the four transports made to agree; the About
  divergence retired; the derived platform projections (the Debian `~`
  trap, the MSI three-field limit, the Docker `+` illegality); changelog
  anchoring. It touches the gate — a health-shape assertion goes red by
  design on its first run — so it is worth nothing while v4 drift still
  lands every other day, and worth doing before testers arrive. D21 as
  amended carries the ruling; everything else release-shaped (signing,
  publishing, the updater, multi-arch Docker, cross-platform CI) stays
  deferred and unordered.

**Standing regen note (supersedes the one above):** the oracle baseline is
**`c8a3cf77`** (2026-08-20, v4 main — the version bump atop `870a57fa`,
"Per-turn conversation summaries with embedded vector reuse (#38)"),
adopted at the `c8a3cf77`-round unification (2026-08-20). ⚠ **v4 is
ALREADY PAST it** (`e22f7b36`, the anti-chorus salon commit — the next
drift): pin a detached worktree at `c8a3cf77` for EVERY regen until that
catch-up runs (`recipe_sweep.py --v4 <pin-path>`; ALL THREE symlink
classes — root `node_modules`, `packages/quilltap/node_modules`, the
`plugins/dist/*/node_modules` dirs). Drift-check all THREE branches every
round (main, bugfix measured by `diff` never the commit list — its only
unabsorbed content today is the test-only `009c49b2` — and `release` for
checkout occupancy only). The sweep driver remains the sanctioned per-family regen
path — never run two sweeps concurrently. The distill-transitive TZ pins,
the committed-fixture rule, and the venue/staging rules stand unchanged.

**The `b8449b3e` anti-chorus + maintenance round (P4.D96 ∥ P4.52 ∥ P4.53)
— PLANNED 2026-08-20.** Drift-checked at planning: v4 moved two commits
past `c8a3cf77` — `e22f7b36` ("feat(salon): anti-chorus discipline for
multi-character scenes", a behavior change on three ported surfaces: the
`isRecentlyAddressed` direct-address rewrite in the client-safe
skip-signal module, the turn-skip note's restate-is-not-substantive
paragraph + reworded caution, and the `applyMultiCharacterTurnAnchor`
restructure appending the new `GROUP_SCENE_DISCIPLINE` block on BOTH
anchor routes — no schema change, no D23 re-dump) and `b8449b3e`
("fix(tests): disable V8 Sparkplug for jest", bug 83 — jest launch infra,
NO-PORT, though every oracle regen at the pin now inherits
`--no-sparkplug`, which only reduces worker-SIGSEGV flakes). `bugfix`
measured by `diff`: HEAD still the 2026-08-13 `3a76b17d`, nothing
unabsorbed beyond the test-only `009c49b2`; checkout on `main`, clean.
The round (all three lanes pin `b8449b3e`; the baseline MOVES to
`b8449b3e` at unification; the lanes meet nowhere — no shared contracts,
no gated beats):

- **P4.D96** (`work-orders/p4.d96-anti-chorus-drift.md`) — the whole
  `e22f7b36` drift: the direct-address rewrite in BOTH the Rust core
  (`skip_signal.rs`) and the SPA client twin (with the parity spec grown
  1:1 from v4's eight new cases), the turn-skip note bytes
  (`build_turn_skip_instruction`), the turn-anchor restructure +
  byte-exact `GROUP_SCENE_DISCIPLINE` (`message_context.rs`) with a NEW
  tier-1 turn-anchor oracle family (none drives that function today),
  red-first regen of the skip-signal family, the JS-regex fidelity traps
  pre-surveyed (`(?-u:\b)` per `mentioned_characters.rs:80`, UTF-16
  longest-first sort, the `im` flags), help docs → the `p4.9i2` bank, and
  the `b8449b3e` NO-PORT disposition.
- **P4.52** (`work-orders/p4.52-memories-fixture-vintage.md`) — the
  `c8a3cf77` round's named maintenance order: widen the committed
  `memories-{main,mount}.db` pair to the current v4 schema vintage
  (measured against `generateDDL` at the pin, seeded rows
  byte-preserved), retire the `housekeeping_config_set` RULED VINTAGE ROW
  to a plain equality (its tripwire fires by design), regen every
  consumer family fresh, and resolve by measurement whether the pair's
  consumer set is the three surveyed files or the round record's wider
  "memories/memory families" wording.
- **P4.53** (`work-orders/p4.53-sweep-driver-followups.md`) — P4.51's
  three recorded driver follow-ups: the LIVE `brahma_console_routes`
  restored-recipe `W=` self-clobber (+ four cosmetic twins, exact lines
  surveyed), the `nothing_to_run` refusal for empty-stage `--run`s (no
  more vacuous "OK" greens; the exposed rows are the next maintenance
  inventory), and `normalize()` neutralizing any `^(V5W|V5|W)=` header
  alias so the class is unforgeable — each with `--self-test` arms and
  marker-probe mutation proofs.

Deliberately left out of the round: the merge-verb silent-keep-on-invalid
sweep (the `c8a3cf77` LEAD — its arms would land in the memories/brahma/
settings case files both maintenance lanes own pieces of; run it NEXT
round over the widened fixture), the owed 💸 dogfood queue (a `/dogfood`
pass, not an order — and it should run soon: the queue now spans Almanack,
Taboo, vision, Serper, whispers, Pascal side effects, dedup/summaries,
per-turn cadence, and the D96 group-scene walk), `p4.9i2` (the bank grows
again this round), and PB1 (parked by standing rule).

**The `b8449b3e` round — UNIFIED on main (2026-08-21). All three lanes
CLOSED; the oracle baseline MOVES to `b8449b3e` and the drift debt is
CLEARED.** P4.D96 landed whole (the case-folding recorded divergence awaits
human ratification — v5 can only over-detect "directly addressed", the safe
direction, same class as `crate::mentioned_characters`); P4.52 landed whole
(the widened pair is stamped `SCHEMA VINTAGE: v4 b8449b3e`; the sibling
memories/memory families build their own /tmp fixtures, so the committed
pair's consumers are exactly `memories_routes_equivalence` + two Playwright
specs); P4.53 landed whole (checkout aliases unforgeable; `nothing_to_run`
a named refusal). **Next candidates:** the next v4 drift catch-up (check
both branches first, as always); the merge-verb silent-keep sweep (the
`c8a3cf77` LEAD — now UNBLOCKED, both colliding maintenance lanes closed;
run its arms over the WIDENED memories fixture); the owed 💸 dogfood queue
(+ D96's live group-scene walk); the maintenance bank (P4.53's 39-family
`nothing_to_run` inventory — ~34 want a scoped `cargo test` run line; the
latent fixture-vintage class — any committed pair whose `characters` table
predates the vault fold-in trips `canChooseOutfit`, and P4.52's measurement
tooling generalizes); `p4.9i2` (its bank grew `help/chat-multi-character.md`
+ `help/turn-skipping.md`). PB1 stays parked by the standing rule. Round
record: `status-log.md`.

**Standing regen note (supersedes the one above):** the oracle baseline is
**`b8449b3e`** (2026-08-20, v4 main — "fix(tests): disable V8 Sparkplug for
jest", atop `e22f7b36`, the anti-chorus commit), adopted at the
`b8449b3e`-round unification (2026-08-21). v4 had NOT moved past it at
unification. Drift-check BOTH development branches every round (`git log
b8449b3e..main` AND `git diff main bugfix -- lib/ app/ packages/` — measure
bugfix by `diff`, never the commit list; note WHICH branch the checkout
occupies before any regen, and pin a detached worktree for regens whenever
v4 HEAD is past the baseline — all three symlink classes). The sweep driver
remains the sanctioned per-family regen path — never run two sweeps
concurrently; since P4.53 its `--self-test` also guards recipe headers
against cross-alias defaults, and `--run` refuses empty-stage families by
name. The distill-transitive TZ pins, the committed-fixture rule, and the
venue/staging rules stand unchanged.

**The `12fe3e6f` thinking-turn drift round (P4.D97 ∥ P4.D98 ∥ P4.D99 ∥
P4.54): UNIFIED on main (2026-08-22) — ALL FOUR CLOSED; the oracle baseline
MOVES to `12fe3e6f`.** v4's bugs 84/85/86 absorbed whole (two of the three
were this port's own dogfood filings coming back fixed): the thinking-turn
evaluator + registry join + the manifest substrate's FIRST per-model facts,
the prefill `runsThinkingTurn` threading (create default + both
`use_prefill` producers), the model-aware DeepSeek strip, the retire-prefill
heal over v4's OWN `migrations_state` ledger (cross-app once-only in both
directions), the profile editor's three thinking-turn behaviors + the
activated e2e beat, bug 84's TWO-LAYER client fix (reducer carry + resolver
at both render sites; finding #99 FIXED), and run lines for 32 of the 39
`nothing_to_run` families (29 by P4.54 + 3 by the D97 rider). **Next
candidates:** the `ca22ec45` drift catch-up (image-profiles Fetch Models +
Z.AI image generation — landed DURING this round; pin `12fe3e6f` for every
regen until it runs); the merge-verb silent-keep sweep (the `c8a3cf77`
LEAD, still unblocked); the owed 💸 dogfood queue (+ this round's five: the
live bug-85 repro chat, the heal on a Friday-vintage copy, the editor's
model-facts arm, the failed-generate_image sentence, the P4.D96 group-scene
walk); the small maintenance trio (a run line for
`response_parse_equivalence`; `p4_6ay_workbench_wire_contract` into
`EXEMPT_FAMILIES` — needs synthetic self-test families first, see P4.54's
lane record; `settings_wire_actions`' recipe leaning on a sibling's /tmp
fixture — measured FAILING, not skipping, without it); `p4.9i2` (its bank
grew `help/connection-profiles.md` + another `chat-multi-character.md`
touch). PB1 stays parked. Round record: `status-log.md`.

**Standing regen note (supersedes the one above):** the oracle baseline is
**`12fe3e6f`** (2026-08-21, v4 main — "fix(deepseek): decide thinking from
the model, not the request body (bug 86)"), adopted at the
`12fe3e6f`-round unification (2026-08-22). ⚠ v4 HAD ALREADY MOVED past it
at unification (`ca22ec45`, image-provider Fetch Models + Z.AI image
generation — ported surfaces) — **pin a detached worktree at `12fe3e6f`
for EVERY regen until the `ca22ec45` catch-up runs** (all three symlink
classes). Drift-check BOTH development branches every round (`git log
12fe3e6f..main` AND `git diff main bugfix -- lib/ app/ packages/` —
measure bugfix by `diff`, never the commit list; note WHICH branch the
checkout occupies before any regen). The sweep driver remains the
sanctioned per-family regen path — never run two sweeps concurrently. The
distill-transitive TZ pins, the committed-fixture rule, and the
venue/staging rules stand unchanged.

**The `4cb1035e` image + NanoGPT drift round (P4.D100 → P4.D101 stacked ∥
P4.D102): UNIFIED on main (2026-08-22) — ALL THREE CLOSED; the oracle
baseline MOVES to `4cb1035e`.** The `ca22ec45` catch-up plus the two
NanoGPT commits absorbed whole: the honest image `list-models` verb
end-to-end (the P4.D33 bank note retired at source; the refusal replaced
by v4's source/fetchError/cache-only-live flow over a new
`ErasedImageDiscovery` engine seam, wired LIVE in the host), the five
image plugins' keyed model discovery (**a real v4 bug found, TO FILE
UPSTREAM: at `d5830439` v4's OpenRouter image discovery reads wire keys
its own SDK's zod strips/renames, so every keyed list throws and falls
back — v5 reproduces the SDK projection with the
`openrouter/models_live_every_signal` convergence tripwire**), the
image-download seam + the Z.AI URL→base64 conversion (v5 measurably HAD
the bug), the gemini `startsWith('gemini')` routing widening, the whole
NanoGPT provider (manifest through the generator, `ProviderKind` +
builder with the FLAT `reasoning_effort` allowlist, the dual
`delta.reasoning ?? reasoning_content` dialect + v4 bug 87's prose-echo
guard as decoder state — **the D101 lane's effective pin is `4cb1035e`,
ruled IN by the human at lane start** — images over the shared download
seam, embeddings with the catalogue pinned against v4's real
`getEmbeddingModels()`, the thinking rule through the P4.D97 machinery
with the exactly-two-rules guard moved 2 → 3 by design, and the census
that REFUTED four ordered joins as legacy-table NO-PORTs with a guard
test), and the SPA client half (the Fetch Models flow with v4's four
label strings, the Z.AI/NanoGPT provider entries + size panels, the
NanoGPT embedding surface + badge CSS with the undefined-border quirk
preserved, two order items refuted by measurement, both gated beats
FLIPPED LIVE at unification). **The §3 review found no blocking findings
in the read; the unified sweep's first run caught the round's cross-lane
blind spot** — the image-profiles-routes oracle's `PLUGIN_DIRS` missed
the nanogpt append (D100 authored it pre-manifest, D101 appended the
other two lists; only the union could red) — fixed on the unify branch.
**Next candidates:** the next v4 drift catch-up (three prompts commits
already sit past the baseline — `8f868109` project/group standing
instructions in the cacheable system prompt, `346e855f` second-person
tool reinforcement [bug 88], `a6870c5a` grammatical-person consistency —
ported prompt surfaces, PROMPT_CACHE_STRUCTURE_VERSION territory); the
merge-verb silent-keep sweep (still unblocked, now that this round's
settings-case collisions are landed); the owed 💸 dogfood queue (+ this
round's: the live-key Fetch Models smoke incl. the OpenRouter finding
with a real key, and the NanoGPT chat/image/embeddings smoke — needs a
NanoGPT key); the small maintenance trio (unchanged); `p4.9i2` (its bank
grew the four NanoGPT help docs + the image-generation-profiles
rewrite). PB1 stays parked. Round record: `status-log.md`.

**Standing regen note (supersedes the one above):** the oracle baseline is
**`4cb1035e`** (2026-08-22, v4 main — "fix(nanogpt): suppress the
gateway's reasoning echo (plugin 1.0.2, bug 87)"), adopted at the
`4cb1035e`-round unification (2026-08-22). ⚠ v4 HAD ALREADY MOVED past it
at unification (`8f868109` + `346e855f` + `a6870c5a`, the prompts trio —
ported surfaces) — **pin a detached worktree at `4cb1035e` for EVERY
regen until the prompts catch-up runs** (all three symlink classes).
Drift-check BOTH development branches every round (`git log
4cb1035e..main` AND `git diff main bugfix -- lib/ app/ packages/` —
measure bugfix by `diff`, never the commit list; note WHICH branch the
checkout occupies before any regen). The sweep driver remains the
sanctioned per-family regen path — never run two sweeps concurrently. The
distill-transitive TZ pins, the committed-fixture rule, and the
venue/staging rules stand unchanged.

## P4.55 — the merge-verb silent-keep sweep (lane closed 2026-08-22)

Tier 1 and Tier 2 both landed whole; the lane record with the measurements
is in `status-log.md`. **Named next-round items this lane deliberately did
NOT take:**

- **B2 — the data-retention present-`null` state collapse (CONFIRMED
  divergent, DEFERRED by ownership).** `Request::DataRetentionSettingsUpdate`
  carries `#[serde(default)] stale_chat_days: Option<serde_json::Value>`
  (`api/types.rs`), so serde maps an explicit `null` to `None`
  indistinguishably from an absent key; `engine.rs` then builds `{}` and the
  handler keeps the current value at 200, where v4's Zod `.default()` fires
  only for `undefined` and answers 400. The fix is the known `double_option`
  pattern in `types.rs` plus the three-arm match in `engine.rs` — **both
  files belong to other lanes** (`api/types.rs` was P4.D103's this round),
  which is why P4.55 left it.
  It also needs a harness rewire before it can be pinned honestly: the
  settings-routes differential calls
  `settings::data_retention_settings_update(db, body)` DIRECTLY
  (`settings_routes_equivalence.rs:219-221`), bypassing the `Request` enum's
  serde entirely, and there is no REST edge for data-retention in
  `quilltap-web` at all — so a null arm added today would pass green against
  the broken wire. The rewire is a `dataRetention` edge mapping mirroring
  taboo/brahma's, a `seedDataRetention`, and an `after` refetch map entry.
- **The groups-side cleared-null pin.** P4.55 measured the store-backed
  echo on the PROJECTS side and found it NOT divergent (see the lane
  record's `update_clear_description` arm); `db/store_backed.rs` was not
  touched. The groups side inherits that verdict by construction — one
  generic `update`, two `StoreEntity` impls — but its own pinning arm rides
  the next round, because P4.D103 owned the groups families this round.

**The `a6870c5a` prompts-trio round (P4.D103 ∥ P4.D104 ∥ P4.55): UNIFIED
on main (2026-08-22) — ALL THREE CLOSED; the oracle baseline MOVES to
`a6870c5a` and the drift debt is CLEARED.** The prompts trio absorbed
whole: the standing-instructions section end-to-end (module + builder
slot + the four call sites + the Prospero whisper drop + the groups
`instructions` verbs with BOTH v4 validators; `PROMPT_CACHE_STRUCTURE_
VERSION` 3 → 4), bug 88's second-person tool reinforcement, the
identity-stack person-consistency wording under the version-stamped
`compiledIdentityStacks` envelope (v5's golden hash EQUALS v4's
registered one), the SPA's shared prompt-field label + twelve-key hints
table + migration sweep + Group Instructions editor (beat activated,
first live run green), and the `c8a3cf77` merge-verb silent-keep lead
CLOSED (A1/A2 were PERSISTING garbage; B1's ten-field leniency; E2's
schema; the D1–D3 missing-`else` trio; store_backed measured NOT
divergent). The §3 review fixed ten findings on the unify branch — the
two that would have shipped: `group_update`'s parse-before-find (400
where v4 answers 404) and the autonomous title max counted in scalars
instead of UTF-16 units. Gate: 43-family sweep 43/43 zero SKIP fresh at
the pin; 444 binaries / 2,266 / 0; clippy both sets; release build; ng
341 / 5,054; full Playwright 236/236 zero skips.

**Next candidates, in rough value order** (updated at the
`a6870c5a`-round unification, 2026-08-22):

1. **The next v4 drift catch-up** (check both branches first, as
   always).
2. **The owed 💸 dogfood queue** — now incl. this round's: standing
   instructions on a REAL turn on the Friday copy (project + group
   prompts reaching a live model — no oracle judges that), the Group
   Instructions editor walk, the invalid-config 400s on live surfaces;
   plus the standing items (the dedup/summaries first run, P4.D35's
   other three write paths, the NanoGPT/Fetch-Models live-key smokes).
3. **B2 — the data-retention present-`null` collapse** (CONFIRMED
   divergent, deferred by ownership; needs the settings-routes
   `dataRetention` edge-mapping rewire first — see "P4.55" above).
4. Banked smalls: the groups-side cleared-null pin (no defect expected),
   the memories float-literal echo nit, the shared-`baseUrl`-helper
   cleanup, the P4.54-era `response_parse_equivalence` run line +
   `settings_wire_actions` fixture leaning (unchanged).
5. `p4.9i2` (its bank grew the trio's ten help files + the help-chat
   builder's two wording changes) and the generators-lane bank (grew the
   four person-clause files + the "never flip a field's form of address"
   rule).

PB1 stays parked by the standing rule. Round record: `status-log.md`.

**Standing regen note (supersedes the one above):** the oracle baseline
is **`a6870c5a`** (2026-08-22, v4 main — "feat(prompts):
grammatical-person consistency in assembled prompts"), adopted at the
`a6870c5a`-round unification (2026-08-22). v4 had NOT moved past it at
unification (verified immediately before the unified regen — the first
pin-free round in five). Drift-check BOTH development branches every
round (`git log a6870c5a..main` AND `git diff main bugfix -- lib/ app/
packages/` — measure bugfix by `diff`, never the commit list; its only
unabsorbed content today is the test-only `009c49b2`; note WHICH branch
the checkout occupies before any regen, and pin a detached worktree
whenever v4 HEAD is past the baseline — all three symlink classes). The
sweep driver remains the sanctioned per-family regen path — never run
two sweeps concurrently. The distill-transitive TZ pins, the
committed-fixture rule, and the venue/staging rules stand unchanged.

### The `a14a1811` vision round (P4.D106 ∥ P4.D107 ∥ P4.D108 ∥ P4.D109 ∥ P4.57) — UNIFIED 2026-08-23

All five lanes closed same-day; **the oracle baseline MOVES to `a14a1811`**
(v4 main, "characters can look at images, and images reach vision models",
bugs 91–95; the intervening `65f3476e` + `718c9ada` both NO-PORT with
evidence). The transport predicate + moderation finish reasons + the
three-tier attachment anchor (two NEW tier-1 families; the downstream-stamp
measurement found and fixed a real re-anchor on the non-streaming
regenerate funnel via `send_message_with_anchor`) ∥ NanoGPT plugin 1.1.0
(`image_url` + the truthful ledger; corpus 321 → 341; a tree-wide
`attachment.url`-arm blind spot closed) ∥ the `describe_image` looking verb
end-to-end (catalog 57 → 58; the auto-describe module; the Librarian
rewrites) ∥ the attachment-failure toast + the client attachment table's
staleness note retired as v4's own upstream fix arrived ∥ tri-state
decode-once across all three settings verbs (byte-diff-proven
zero-behavior-change). **The §4 wires made the vision tier REACHABLE in
production** (OrchestratorDeps + spine thread the describe driver AND the
photo-bytes store), and **the §3 review's headline catch was exactly that
wire's missing half** (driver-without-bytes = `no-bytes` starvation);
also fixed at unification: the `restream_into` attachment-ledger carry
(bug 94's new reader had made the stale value user-visible), auto-describe
propagating DB failures raw, `dangerMode` on the empty-response warn, the
NANOGPT coverage floors, the id-set predicate extraction pin, and five
smaller repairs. **TO FILE UPSTREAM (v4-side):** the OpenRouter registry
entry declares `supportsAttachments: false` while its static map
transports — v4 production routes OpenRouter vision profiles to the
describe-fallback and refuses OpenRouter describers (jest never sees it);
plus the `moderation-finish-reason.ts` "(bug 94)" docblock mis-number.
Gate + versions: the round record in `status-log.md`.

**Next candidates, in rough value order** (updated at the
`a14a1811`-round unification, 2026-08-23):

1. **The `3c041e46` drift catch-up** (v4 bug 96 — the title-verdict
   module extracted from `cheap-llm-tasks/chat-tasks.ts` + the
   title-update handler fix; a behavior change on a ported surface,
   classified at this unification; check both branches first, as
   always).
2. **The owed 💸 dogfood queue** — now incl. this round's: the NanoGPT
   vision send (a real image to a real routed vision model), a real
   Z.AI `sensitive` refusal showing the named sentence, the
   `describe_image` walk (all three serve tiers on a fresh upload —
   the vision tier's first LIVE run since the unification wire), the
   failed-attachment warning toast, a whisper-tailed regenerate
   carrying its image; plus the standing items (the caching smoke, the
   Brahma budget, the failed-`generate_image` sentence, the candid
   story background, Pascal's other three write paths, the NanoGPT
   embedding leg, dedup/summaries).
3. ~~The two v4-side filings~~ **FILED (2026-08-23, v4 `7a6716b5`)**:
   the OpenRouter registry/static-map transport contradiction is **v4
   bug 97** (`bugs/bug-97-openrouter-registry-denies-vision.md`, fix
   spec included — OPEN, awaiting the v4-side fix; v5's pins converge
   at the drift round after it lands), and the moderation-docblock
   mis-number was corrected in the same commit (a comment-only lib
   edit — NO-PORT beyond optionally retiring v5's own discrepancy
   note in `moderation_finish_reason.rs`).
4. Corpus maintenance candidates recorded this round: photo-tools rows
   for the width/height-NULL omission + the whitespace-only-description
   quirk; a settings-routes NANOGPT/Z_AI create-with-omitted-flag row
   (the `supportsImageUpload` seed default).
5. `p4.9i2` (the bank grew `connection-profiles.md`'s two-questions
   section, `dangerous-content.md`'s refusal section, and the retitled
   `keep-image-tools.md`).

PB1 stays parked by the standing rule. Round record: `status-log.md`.

**Standing regen note (supersedes the one above):** the oracle baseline
is **`a14a1811`** (2026-08-22, v4 main — "fix(images): characters can
look at images, and images reach vision models (bugs 91-95)"), adopted
at the a14a1811-round unification (2026-08-23). ⚠ v4 HEAD is now TWO
commits past it: `3c041e46` (bug 96 — candidate 1 above) and
`7a6716b5` (this port's own bug-97 filing — docs + a comment-only lib
line, NO-PORT class): **pin `/tmp/qt-v4-a14a1811` (already prepared,
all three symlink classes) for every regen** until the catch-up lands.
Drift-check BOTH development branches every round; `bugfix`'s only
unabsorbed content remains the tests-only `009c49b2`. The sweep driver
remains the sanctioned per-family regen path — never run two sweeps
concurrently.

### The `f8973813` round (P4.D105 ∥ P4.56) — UNIFIED 2026-08-22

Both lanes closed same-day; the baseline STAYS `f8973813` (v4's newer
`65f3476e` = CI/release infra + a comment-only lib edit + standalone-tarball
native linking v5 doesn't have — NO-PORT with evidence, recorded in the round
record). NanoGPT prompt caching whole (options group via the generator, the
strict-gate `promptCaching` body key, both-dialect cache usage with the
measured `??`-precedence pin, streaming `rawProviderUsage`; corpora 321 /
52 / 22, every pre-existing row byte-identical) ∥ the settings-wire
remainder (B2 fixed red-first via `double_option` behind the harness
serde rewire; the new data-retention REST edge, which uncovered the
`BrahmaConsole` success-arm 500 standing since P4.D57 + two leaked-DbError
sentences, all fixed + pinned; the groups cleared-null pin zero-change;
`settings_wire_actions` self-contained; the float-literal store fix; the
shared classify readers, mutation-proven at all three sites). §3: no
blocking findings. Gate: the round record in `status-log.md`.

**Next candidates, in rough value order** (updated at the
`f8973813`-round unification, 2026-08-22):

1. **The next v4 drift catch-up** (check both branches first, as always;
   `65f3476e` is already dispositioned NO-PORT — the check starts from
   `f8973813` and will list it again; the disposition in the round record
   is the answer).
2. **The owed 💸 dogfood queue** — now incl. this round's: the live
   NanoGPT caching smoke (real key, Claude-routed model, two turns —
   `cacheUsage` in the LLM Inspector + cost display, the Prompt Caching
   card, the 1h TTL) and the data-retention invalid-config 400 on a live
   screen; plus the standing items (standing instructions on a REAL turn,
   the Group Instructions walk, the dedup/summaries first run, P4.D35's
   other three write paths, the Fetch-Models live-key smoke).
3. **The tri-state decode-once adoption** for `taboo` / `brahma-console`
   (P4.56's recorded lead: both still re-derive the tri-state at three
   call sites each; the data-retention edge demonstrates the shape).
   Small, well-scoped.
4. `p4.9i2` (the bank grew the NanoGPT Prompt Caching help bullets this
   round) and the generators-lane bank (unchanged).
5. Maintenance: the wider `docs/v4/` mirror staleness (~8 differing
   files + unmirrored `bugs/fixed/` rows — P4.D105's Tier-2 note).

PB1 stays parked by the standing rule. Round record: `status-log.md`.

### `p4.9i2` bank — `help/connection-profiles.md` (P4.D111, v4 `0ba942b1`)

Banked 2026-08-23 by the bug-97 convergence lane. v4's `0ba942b1` added ONE
paragraph to `help/connection-profiles.md`, immediately after the
"They are not the same question…" paragraph (v4 line 417) and immediately
before "Formerly the checkbox was taken as the whole answer…". It is carried
here VERBATIM for the `p4.9i2` help-doc port; the house voice is v4's and must
not be re-worded:

> A third possibility, rarer and more vexing still, is a connector that *can* send a picture but has neglected to say so. **OpenRouter** was in precisely this position: its connector has forwarded images competently for some time, while the paperwork it files with Quilltap on startup still declared the old abstinence. Quilltap, reading the paperwork rather than the deed, routed every OpenRouter image to the description fallback and — with a straight face — refused an OpenRouter profile the post of describer in the very sentence that recommended OpenRouter for the job. The declaration has been corrected and now takes its list of formats directly from the connector that does the sending, so the two can no longer fall out of step. If your describer or your vision profile sits on OpenRouter, it will simply begin receiving the pictures themselves; nothing needs re-ticking.

Nothing else in that file moved at `0ba942b1`. The rest of the `p4.9i2` bank is
unchanged.

### The `0ba942b1` drift round (P4.D110 ∥ P4.D111 ∥ P4.58) — UNIFIED 2026-08-23

All three lanes closed same-day; **the oracle baseline MOVES to `0ba942b1`**
(v4 main — the bug-97 fix) and the drift debt is CLEARED: `3c041e46` (bug 96),
`7a6716b5` (the filing; one comment line), `0ba942b1` (the convergence). The
title-verdict parser whole (near-miss keys + fold pass + double-trim + four
byte-exact warn arms + the checkpoint-burned handler warn, cursor semantics
unchanged; `title_update_tier3` 10 → 17 red-first; the warn WIRING pinned by a
capturing layer because the burned checkpoint's DB state is byte-identical to
a decline) ∥ the bug-97 convergence (manifest regen with nine siblings
byte-identical; the predicate flip; the guard sentence's NanoGPT entry; the
moderation mis-number note retired; every former pin a plain equality,
red-first per family; the help paragraph banked to `p4.9i2`) ∥ the corpus
blind spots (photo-tools NULL-omission + whitespace quirk at both ends;
settings-routes seed-default quartet; zero v5 source change; three order
premises refuted by measurement). §3: **no blocking findings.** Gate: 7/7
pinned sweep zero SKIP; 449 binaries / 2,320 / 0; clippy both sets; release
build; ng 341 / 5,068; Playwright 237/237. Versions: core 0.0.645, harness
0.0.562. Round record: `status-log.md`.

**Next candidates, in rough value order** (updated at the `0ba942b1`-round
unification, 2026-08-23):

1. **The next v4 drift catch-up** (check both branches first, as always).
2. **The owed 💸 dogfood queue** (unchanged by this round — its surfaces are
   oracle-covered): the NanoGPT vision send, the Z.AI refusal sentence, the
   `describe_image` walk, the failed-attachment toast, the whisper-tailed
   regenerate, the caching smoke, the Brahma budget, the
   failed-`generate_image` sentence, the candid story background, Pascal's
   other three write paths, the NanoGPT embedding leg, dedup/summaries; a
   future pass could add a live look at the new title-verdict warn lines in
   `combined.log`.
3. **The title-update handler logging gap** (P4.D110's banked finding: 7 of
   v4's 8 log lines unported in `title_update_job.rs` — silent no-cheap-LLM /
   failed-job / story-background outcomes; small, well-scoped, pairs with any
   wider handler-logging sweep).
4. The tri-state web-edge survey sites (P4.57's bank) and the
   `taboo`/`brahma-console` three-call-site residue (P4.56's note).
5. `p4.9i2` (the bank grew the bug-97 connection-profiles paragraph this
   round) and the stale `docs/v4/` mirror maintenance.

PB1 stays parked by the standing rule.

**Standing regen note (supersedes the one above):** the oracle baseline is
**`0ba942b1`** (2026-08-23, v4 main — "fix(openrouter): the plugin declares
the vision path it already implements (bug 97)"), adopted at the
`0ba942b1`-round unification (2026-08-23). v4 had NOT moved past it at
unification (verified immediately before the unified regen). Drift-check BOTH
development branches every round (`git log 0ba942b1..main` AND `git diff main
bugfix -- lib/ app/ packages/` — measure bugfix by `diff`, never the commit
list; its only unabsorbed content today is the test-only `009c49b2`; note
WHICH branch the checkout occupies before any regen, and pin a detached
worktree — all three symlink classes — whenever v4 HEAD is past the baseline
or the checkout is dirty). The `/tmp/qt-v4-a14a1811` and
`/tmp/qt-v4-pin-unify-0ba942b1` worktrees are removed post-round; build a
fresh lane-unique pin per lane. The sweep driver remains the sanctioned
per-family regen path — never run two sweeps concurrently. The
distill-transitive TZ pins, the committed-fixture rule, and the venue/staging
rules stand unchanged.

**The no-drift maintenance round (P4.59 ∥ P4.60 ∥ P4.61) — PLANNED
2026-08-24.** Drift-checked at planning: **v4 has NOT moved** (`git log
0ba942b1..main` empty; `bugfix` HEAD still `3a76b17d`, nothing unabsorbed
beyond the tests-only `009c49b2`; checkout on `main`, clean) — the first
round in weeks with zero drift debt, spent on the three highest-value
banked items. The baseline STAYS `0ba942b1`; the lanes meet nowhere (no
shared contracts, one identical ownership table in all three orders):

- **P4.59** (`work-orders/p4.59-configured-search-provider.md`) — dogfood
  finding #98: the configured-path search provider. The Serper provider
  registered natively the way v4's `enabledByDefault: true` dist plugin is
  (`serper_registered` flips real), the per-call key resolved from
  `api_keys` through the already-wired-inert `DbSearchApiKeys`, the
  plugin's own `executeSearch` sentences ported for the newly-live
  registered arm, the `GET /api/v1/providers` `type: 'search'` entry
  (retiring `provider_list()`'s "documented absence"), and the SPA
  API-keys surface offering Serper. The P4.42 tier-3 family grows the
  registered arms red-first with v4's REAL registry initialized with the
  REAL dist plugin; the site-plugins env gate is measured, not assumed.
- **P4.60** (`work-orders/p4.60-wrong-type-collapse-edges.md`) — the
  P4.57-banked wrong-type-collapse adjudication: the eleven enumerated
  edge sites (custom-tools / characters ×3 / backup ×4 / brahma /
  embedding-profiles `scope`, plus the qtap confirm-only pass), each read
  against its v4 route's Zod, verdicts FAITHFUL / DIVERGENT-FIXED /
  DIVERGENT-RECORDED, fixes decoding through the `Request` enum with
  guard order matching v4 (the `group_update` lesson), every fixed arm
  pinned red-first in its owning routes family.
- **P4.61** (`work-orders/p4.61-title-update-handler-logging.md`) —
  P4.D110's banked finding: the seven missing `[Title Update]` log lines
  (v5 carries 1 of v4's 8) ported byte-faithfully with capturing-layer
  presence + silence pins per the differential-blind-to-log-only-fix
  discipline; rider: the stale `docs/v4/` mirror refreshed mechanically
  at the baseline (~8 files + `bugs/fixed/` rows).

Deliberately left out: **`p4.9i2` (help/HelpChat) — the biggest remaining
unported vertical, which now deserves its own DEDICATED round**: v5 has
the help-doc substrate (chunks/sync/search, P4.D77) but no `help/` content
directory, no help-chat service, none of v4's three `help-chats` REST
routes, no Guide client, and a content bank grown across ~15 rounds — a
proper survey-heavy multi-lane round, recommended as the NEXT round if v4
stays quiet; the owed 💸 dogfood remainder (Pascal's other three write
paths, the Brahma deep-query budget, dedup/summaries — human calls, not
orders; the #101 NanoGPT-caching cost question also awaits the human);
the tri-state `taboo`/`brahma-console` three-call-site residue (P4.56's
note — adjacent to P4.60's territory but a different class; next
maintenance pass); PB1 (parked by standing rule).

**The no-drift maintenance round — UNIFIED on main (2026-08-25). ALL THREE
LANES CLOSED; the baseline STAYS `0ba942b1` — and ⚠ v4 drifted DURING the
round.** P4.59 landed whole (dogfood #98 CLOSED: the Serper provider
registered natively behind v4's site-plugins gate, per-call keys live from
`api_keys`, the providers listing's `type: 'search'` row, the SPA's invented
`type === 'llm'` API-keys filter removed — v4 filters on
`providerAcceptsApiKey` alone; the salon web-search beat now proves the
CONFIGURED path with no env key); P4.60 landed whole (the complete
adjudication table — 14 DIVERGENT-FIXED, 6 FAITHFUL, zero escalations; the
Brahma trio validates after the 404 gate; the restore guard order lives in
one place; the executable `web_edge_body_parse_guard` census; remaining
pockets NAMED: `system_data_routes` 13 / `files_routes` 5 /
`llm_logs_routes` 1); P4.61 landed (5 of 8 log lines byte-faithful, `:89` +
`:185` NO-PORTs with v4-source evidence; the `docs/v4/` mirror refreshed at
the baseline). **The §3 review: NO blocking findings** (fidelity re-checked
against v4's real code; the lane-close timeline audited against the drift
commits — no regen ever saw a moved tree). Gate: 13/13 families fresh from
the pinned worktree zero SKIP; 453 test binaries / 2,338 / 0; clippy both
feature sets; release build; ng 341 / 5,072; full Playwright green (numbers
in the round record). Versions: core 0.0.655, harness 0.0.574, host 0.0.82,
web 0.0.86, SPA 0.5.549. Round record: `status-log.md`.

**Next candidates, in rough value order** (updated at the no-drift-round
unification, 2026-08-25):

1. **The `c93ec7ff` drift catch-up** — v4 moved TWO commits past the
   baseline mid-round, BOTH on ported surfaces: `af1bc479` (gallery
   download buttons across My Photos/avatar-grid/Scriptorium + the
   mount-blob route's inline `Content-Disposition` with the stored
   basename) and `c93ec7ff` (bug 98 — the projects create schema stops
   refusing a blank description). Check both branches first, as always;
   **pin `0ba942b1` for EVERY regen until this lands.**
2. **The owed 💸 dogfood queue** — now incl. the finding-#98 scenario
   itself on the Friday copy (the `SERPER` row v4 wrote should just work,
   no env var) and the title-update log lines in a real `combined.log`;
   plus the standing items (Pascal's other three write paths, the Brahma
   deep-query budget, dedup/summaries, the candid story background).
3. **The next wrong-type-collapse order**: `system_data_routes.rs`'s 13
   sites (the largest remaining pocket; P4.60's census makes it a
   measurement, not a grep), then `files_routes.rs`'s 5.
4. **`p4.9i2` — help/HelpChat as a dedicated round** (sized in the
   planning note above).
5. The handler-logging sweep (P4.61's named deferral, incl. the
   `cost_events::create_system_event` sibling) and the
   `taboo`/`brahma-console` tri-state residue.

PB1 stays parked by the standing rule.

**Standing regen note (supersedes the one above):** the oracle baseline
remains **`0ba942b1`** (2026-08-23, v4 main — the bug-97 fix), retained at
the no-drift-round unification (2026-08-25). ⚠ **v4 HEAD is now TWO commits
past it** (`af1bc479` gallery downloads + `c93ec7ff` bug 98 — candidate 1
above, both on ported surfaces): **pin a detached worktree at `0ba942b1`
for EVERY regen until the catch-up lands** (all three symlink classes; the
unification's `/tmp/qt-v4-pin-unify-p459round-0ba942b1` is removed
post-round — build a fresh lane-unique pin per lane). Drift-check BOTH
development branches every round (`git log 0ba942b1..main` AND `git diff
main bugfix -- lib/ app/ packages/` — measure bugfix by `diff`, never the
commit list; its only unabsorbed content today is the tests-only
`009c49b2`; note WHICH branch the checkout occupies before any regen). The
sweep driver remains the sanctioned per-family regen path — never run two
sweeps concurrently. The distill-transitive TZ pins, the committed-fixture
rule, and the venue/staging rules stand unchanged.

**The `f6a10055` wardrobe-containers drift round (P4.D112 ∥ P4.D113 ∥
P4.D114) — PLANNED 2026-08-25.** v4 sits FOUR commits past `0ba942b1`
(the two known at the last unification plus two wardrobe commits from
2026-08-25): `af1bc479` (gallery download buttons + the mount-blob
route's inline `Content-Disposition`), `c93ec7ff` (bug 98 — the projects
CREATE schema stops refusing a blank/null description), `d7263f39` (the
wardrobe dialog browses and edits every container — a NEW group wardrobe
CRUD API, transfers gain an explicit `source {scope,id}`, the item
editor's shared-edit mis-target fixed, Duplicate preserves
`imagePrompt`), and `f6a10055` (moving/copying an outfit brings its
same-container components along — plan-first id remap, refuse-on-
collision, post-write read-back — plus the `buildSlugByItemIdMap`
collision fix, which v5 measurably shares). All four are behavior drift
on ported surfaces. Three lanes, orders committed:
`p4.d112-wardrobe-containers-server.md` (core: group CRUD verbs +
transfers components machinery + the slug fix),
`p4.d113-wardrobe-containers-spa.md` (the whole
`apps/web/src/app/wardrobe/` folder incl. `af1bc479`'s wardrobe hunk;
group-dependent beats gated ACTIVATE-AT-UNIFY),
`p4.d114-downloads-bug98.md` (the remaining download surfaces + the blob
header + the create schema; no cross-lane contract). D112↔D113 share a
binding contract (the five `groupWardrobe*` verbs, the transfers
`source`/`components` body + response fields, the container scope-string
spellings). At unification the oracle baseline MOVES to `f6a10055`; until
then **pin `f6a10055` for every regen** (the checkout sat clean on `main`
at HEAD = the pin at planning). `bugfix` measured at planning: nothing
unabsorbed beyond the tests-only `009c49b2`.

**The `f6a10055` wardrobe-containers drift round — UNIFIED on main
(2026-08-25): ALL THREE ORDERS CLOSED; the oracle baseline MOVES to
`f6a10055` and the drift debt is CLEARED.** Full round record in
`status-log.md`; per-order outcomes in the three status headers. The §3
unification review found NO blocking findings. Headline facts: v5
measurably had THREE of the bugs v4's commits fix (the slug-collision
rewire, the shared-edit mis-target to Quilltap General, the dropped
Portrait Cue on Duplicate) plus one all its own (the create-project
validation vacuum — the far larger half of the bug-98 unit); the
`componentsTransferred`/`unresolvedComponentIds` render ask was refuted
(v4's client never reads them).

**Next candidates, in rough value order** (updated at the
`f6a10055`-round unification, 2026-08-25):

1. **The owed 💸 dogfood queue** — gains this round's live surfaces: the
   container browser on real Friday data (browse a project/group wardrobe
   in place, edit there, star there), a real component-carrying outfit
   move, the My Photos Download/Copy buttons on real photos, and a
   create-project refusal reading v4's sentence.
2. **Widen the committed `characters-*` e2e fixture with a Quilltap
   General store** (instance_settings + the builtin-mount tables) so the
   armed component-transfer beat and the container-browser write half can
   run instead of self-parking on the `hasGeneralStore` probe — the
   fixture gap predates the round and also blocks the "Shared —
   everywhere" create scope beat that has never been exercisable.
3. **The next wrong-type-collapse order**: `system_data_routes.rs`'s 13
   sites, then `files_routes.rs`'s 5 (P4.60's census).
4. **`p4.9i2` — help/HelpChat as a dedicated round** (the bank grew the
   wardrobe-containers help rewrites this round).
5. The handler-logging sweep (P4.61's deferral; this round added the
   group-wardrobe handlers' unported `logger.info`/`warn` lines and the
   `project_wardrobe_create` guard-order lead to its inventory).

PB1 stays parked by the standing rule.

**Standing regen note (supersedes the one above):** the oracle baseline is
**`f6a10055`** (2026-08-25, v4 main — "feat(wardrobe): moving or copying
an outfit brings its components along"), adopted at the
`f6a10055`-round unification (2026-08-25). ⚠ the v4 checkout's TREE WAS
DIRTY at unification (in-progress edits on the chats routes; HEAD
unmoved) — the unified gate ran every regen from the pinned worktree
`/tmp/qt-v4-pin-unify-f6a10055` (removed post-round; build a fresh
lane-unique pin per lane whenever HEAD moves past the baseline or the
checkout is dirty, all three symlink classes). Drift-check BOTH
development branches every round (`git log f6a10055..main` AND `git diff
main bugfix -- lib/ app/ packages/` — measure bugfix by `diff`, never the
commit list; its only unabsorbed content today is the tests-only
`009c49b2`; note WHICH branch the checkout occupies before any regen).
The sweep driver remains the sanctioned per-family regen path — never run
two sweeps concurrently. The distill-transitive TZ pins, the
committed-fixture rule, and the venue/staging rules stand unchanged.

**⚠ Post-unification drift note (2026-08-25, same day):** v4 HEAD moved
TWO commits past the fresh `f6a10055` baseline while the round was
unifying — `44a8137e` (feat(salon): the scene can be changed without
leaving the conversation) and `8018c487` (fix(images): a character's
photo gallery can download a picture again, bug 99 — a follow-up on the
download surfaces this round just ported) — and the checkout's tree is
still dirty (Aurora/Prospero header edits in progress). **The catch-up
slots between candidates 1 and 2 above** (after the owed dogfood pass or
alongside it); pin `f6a10055` for every regen until it lands, and
re-survey bug 99 against P4.D114's surfaces at planning — v5 may or may
not share it, since v5's photo galleries got their download buttons from
`af1bc479`'s port, not v4's older gallery code.

**The `8f910137` drift catch-up round (P4.D115 ∥ P4.D116 ∥ P4.D117 ∥
P4.D118): UNIFIED on main (2026-08-25) — ALL FOUR CLOSED; the oracle
baseline MOVES to `8f910137` and the drift debt is CLEARED.** The
scenario-change feature end-to-end (the extracted resolver + the
`chatSetScenario` verb + the GET projection + the Host revision writer +
the transcript carry server-side; the shared ScenarioSelect + the in-chat
control + the activated `salon-scenario-flow` walk client-side), the
client-fixes pair (bugs 100/102 — the qt-* sheet made real over a
69-name/364-site census, the 37-file sweep, and the `check-qt-classes`
guard now wired into `npm run lint` and ahead of `npm test`; bug 99 —
the gallery download and the modal's body-reparent out of the workspace
stacking trap, measured red-first), and bug 101's completion templates
(byte-copied, Tier R red-first 188/0, plus the bash-driving
`completion_behavior` guard). `8f910137` NO-PORT-RATIFIED (CI +
tests-only). Round record: `status-log.md` → "The `8f910137` drift
catch-up round"; drift state: `drift-ledger.md` (baseline `8f910137`,
regen rule pin-free while HEAD sits at the baseline and the checkout is
clean).

**Next candidates, in rough value order** (updated at the
`8f910137`-round unification, 2026-08-25):

1. **The owed 💸 dogfood queue** — gains this round's live surfaces: the
   in-chat scenario picker on real Friday data (seed → preset → revision
   bubble → no-op → clear), the gallery download + detail-modal controls
   on a real character gallery, the restyled qt-* surfaces at a glance
   (the 364 formerly-inert sites now style), and a real
   `quilltap docs --instance Friday <TAB>` completion. Standing items
   carried: Pascal's other three write paths (recipe in the 2026-08-25
   walk doc), the Brahma deep-query budget, dedup/summaries (human), the
   NanoGPT caching smoke, and the #101 cache-read cost question.
2. **Widen the committed `characters-*` e2e fixture with a Quilltap
   General store** so the armed component-transfer beat and the
   container-browser write half stop self-parking (pre-existing; also
   blocks the "Shared — everywhere" create-scope beat).
3. **The next wrong-type-collapse order**: `system_data_routes.rs`'s 13
   sites, then `files_routes.rs`'s 5 (P4.60's census).
4. **`p4.9i2` — help/HelpChat as a dedicated round** (the bank grew
   this round: the three scenario help rewrites + the gallery-download
   bullet).
5. The handler-logging sweep (P4.61's deferral; this round added the
   scenario handler's shared-helper recompile warn — v5 logs the generic
   `[Chats v1] Failed to recompile identity stacks` where v4's site says
   "…after scenario change" — to its inventory).

PB1 stays parked by the standing rule.

**The `b220999d` drift catch-up round (P4.D119 → P4.D120 stacked ∥
P4.D121 ∥ P4.D122) — PLANNED 2026-08-25 (/setupphase).** v4 shipped five
commits past `8f910137` in one day, all three features landing on
just-ported surfaces (drift-ledger §3, rows now ORDERED): `b86bb1a5`
per-tier dressing instructions (hits the tri-tier cascade, the vault
projection sweep, the four wardrobe routes, the outfit-selection prompt),
`d25dacc1` archive-instead-of-delete for scenarios + wardrobe (84 files —
hits the P4.D115/D116 scenario feature unified the day before, the
character-vault round-trip where **v5 replicates v4's description-drop bug
verbatim**, the two hard-coded-`true` wardrobe reads, the Green Room
pins), and `b220999d` the Documents search chip (hits the P4.9P `uiSearch`
verb + chip reorder, the doc-mount repos, the qtap-uri producers, the SPA
search dialog). The two docs-only specs (`a47d3e03`, `2417cbed1`) ride
their implementing lanes for NO-PORT ratification at unify. **Three lanes:**
the server halves of the two wardrobe/scenario commits run as ONE stacked
lane (P4.D119 then P4.D120 — v4 names the first the second's prerequisite,
and v5's vault overlay shares files between them), the SPA halves of both
as P4.D121, and the search feature whole as P4.D122. Ownership + the
two-part shared contract are pinned identically across the four orders. At
unification the oracle baseline MOVES to `b220999d`; until then **pin per
the ledger** (feature-lane pins at `d25dacc1` / `b220999d`; unrelated
families at `8f910137`). Port-4319: only one lane runs Playwright at a
time — cross-lane beats are authored gated.

**The `b220999d` drift catch-up round (P4.D119→P4.D120 stacked ∥ P4.D121 ∥
P4.D122): UNIFIED on main (2026-08-26) — ALL FOUR ORDERS CLOSED; the oracle
baseline MOVES to `b220999d` and the drift debt is CLEARED.** The per-tier
dressing instructions end-to-end (the cascade module, `preserve_file_names`,
the reader skip, the outfit-prompt thread at BOTH v5 `llm_choose` entrances,
the four instructions verb pairs + the Section in both SPA hosts),
archive-instead-of-delete whole (scenario `archived` frontmatter across all
four scopes with default suppression, the character-vault
`build_scenario_file` rewrite — **v5's description-drop bug proven
red-first** — `includeArchived` end-to-end incl. the nine mutate verbs'
fresh-list returns, the wardrobe `archivedPatch` semantics, the Green Room
pins, the nine SPA hosts with the B7 quirks reproduced), and the
Documents-search vertical (the LIKE engine with the fail-closed
archived-vault exclusion, the two repo scans, the `uiSearch` sixth type +
chip reorder, the Documents card with the modified-click passthrough, the
open-from-search choreography, the ACTIVE walk). The §3 review's headline:
the three scoped instructions SET handlers parsed BEFORE the 404 gate
(would have shipped 400-where-v4-404s), the scenario `archived: null`
silent-keep, and the REST edges' unknown-`?action=` fallthrough (a bogus
POST could CREATE) — all fixed on the unify branch with red-first pins.
Round record: `status-log.md`; drift state: `drift-ledger.md` (baseline
`b220999da`, regen rule pin-free while HEAD sits at the baseline).

**Next candidates, in rough value order** (updated at the `b220999d`-round
unification, 2026-08-26):

1. **The owed 💸 dogfood queue — RAN 2026-08-26** (41 rows, 37 PASS, finding
   #105 found and fixed; walk doc
   `dogfood-walks/2026-08-26-instructions-archive-search-pass.md`). Every
   surface this round added is discharged — the Dressing Instructions round
   trip and the cascade on a real "Let character choose" turn (character AND
   project tiers), the archive walk at every scope, an archived garment absent
   from the Green Room pool, the Documents chip over the real stores — as are
   the carried `8f910137` items (the in-chat scenario picker, the gallery
   download, the qt-* restyle, a real `docs --instance <TAB>`) and **two of
   Pascal's three write paths** (chat + project). **What is still owed:**
   Pascal's **group** tier — now precisely characterized, it needs a chat whose
   participants resolve to exactly one group (`groupTier.status == "single"`),
   which real Friday has none of because its two groups overlap on Charlie — and
   the three human cost calls: the Brahma deep-query budget, dedup/summaries,
   and the NanoGPT caching smoke / the #101 cache-read question.
2. **`systemHome` — the landing dashboard — costs a steady 7.5 s on a real
   instance** (dogfood 2026-08-26: 7.50 s and 7.70 s on back-to-back *warm*
   dispatches against the Friday copy; 859 chats, 32 live characters, 8
   projects, 45 vaults). Correct output, no panic, and **no v4 comparison was
   run — so this is NOT filed as a divergence**; it is the app's front door
   costing seven and a half seconds of server time, which deserves its own
   look. Starting point from reading the handler (a hypothesis, not a profile —
   nothing here was measured per-query): `services::home::get_home_data` loads
   the world to render a handful of cards — `chats_read::find_by_user_id` (all
   859), `ProjectsRepository::find_all()`, `characters_read::find_by_user_id`,
   `FilesRepository::find_all()` — and the projects and characters reads both
   take the **mount-index connection**, so each entity hydrates through the
   document-store / vault overlay before the in-memory stats pass trims to 12
   recent chats, 8 projects and a few characters. **First step for whoever
   takes this: profile it** (per-repo timings around the four loads), then
   decide whether the fix is v5-local (narrower projections, a batched overlay
   read) or whether v4 pays the same cost and the answer is to leave it alone
   and say so. ⚠ v4 composes the same `findAll` shape, so a "fix" that changes
   what the dashboard *shows* would be a divergence — the target is the cost of
   producing the same payload.
3. **The duplicate "Quilltap General" store collision** (P4.D122's e2e
   find): the committed e2e fixture serves TWO enabled stores with the
   name; the suspected cause is `services/builtin_mounts.rs` (the
   ensure-or-adopt creating a second row after the boot repair has run) —
   needs its own small order; the beat derives its expected ref so it
   stays green either way. **Narrowed by the 2026-08-26 dogfood pass:** the
   real instance has exactly ONE store by that name and **no duplicate store
   names at all**, so this is a property of the committed fixture, not of
   instances in the wild.
4. **The present-but-null validation lead** (the §3 review's recorded
   class): the scenario bags' name/description/isDefault arms still
   tolerate explicit null where v4's Zod refuses — same class as the
   `archived` fix this round; needs its own measured corpus pass (and a
   sweep for other bag validators with the pattern).
5. **Widen the committed `characters-*` e2e fixture with a Quilltap
   General store** (pre-existing; still parks the component-transfer beat
   and the "Shared — everywhere" create-scope beat).
6. **The next wrong-type-collapse order**: `system_data_routes.rs`'s 13
   sites, then `files_routes.rs`'s 5 (P4.60's census).
7. **`p4.9i2` — help/HelpChat as a dedicated round** (the bank grew ten
   help-file rows this round: the three instructions files + the seven
   archive files, and `help/search.md`).
8. The handler-logging sweep (P4.61's deferral; this round added the
   group/project instructions handlers' info/debug lines and v4's
   unknown-action warn to its inventory).
9. The v4-side filing candidates from this round: the startup-migration
   dedupe hole (one line), the three unconverted `scenarios[0]` sites, the
   unguarded scenario default-SET write path, and the `qt-icon`
   `[class.-rotate-90]` inert-transform wart at `terminal-embed.ts:53`.

PB1 stays parked by the standing rule. The `qtap-export.schema.json` file
port remains a NAMED standalone flag (v5 has never shipped the file).

---

## The `f3892158d` drift catch-up round (P4.D123→P4.D124 stacked ∥ P4.D125) — UNIFIED 2026-08-26

All three orders CLOSED; the oracle baseline MOVES to `f3892158d`. The
jobs/activity accounting and the whole realtime subsystem absorbed, with
the round's settled mechanism divergence: the invalidation hints ride
v5's EXISTING Event channel (engine broadcast → SSE `/api/events` → the
Tauri pump) — no second WebSocket, per the locked transport-agnostic
boundary. Full record: `status-log.md` → "The `f3892158d` drift catch-up
round"; the §3 review's findings (three blocking, all SPA-side, all
fixed red-first on the unify branch) → "The `f3892158d`-round §3
unification review". The chronic `ng` hang gained a root fix on the way:
`tools/ng-run.mjs` now treats a spec BUILD failure as terminal for
`test` (was a 30-minute silent hang).

**Next candidates, in rough value order** (updated at the
`f3892158d`-round unification, 2026-08-26):

1. **Run `/driftcheck` FIRST** — v4 landed ELEVEN more commits during
   this round's unification gate (the 4.9.0 release push; ledger §1
   lists the shas with the verdict UNCLASSIFIED; at least one —
   `21f573039` — touches the just-ported realtime code). Then ratify the
   two already-classified post-baseline commits (`487ae57fe` tests +
   neutral extraction, `561466cfe` knip sweep — drift-ledger §3, both
   NO-PORT? with ratification notes).
   Rider: check v5's `help_doc_chunks` twin pins the
   registerBlobColumns-re-assert trap the new v4 test pins, and whether
   v5 carries now-vestigial twins of the knip-deleted exports.
2. **The owed dogfood pass over this round's live surfaces**: the chips
   counting a real inline image generation (the `generate_image` tool on
   the Friday copy — "Img" lit for the whole span), a `startedByKind`
   pulse from sub-poll work, the pushed invalidation on a real enqueue
   with polling verified parked (zero background fetches in an idle
   window, v4's own verification shape), the terminal WS same-origin
   refusal against the running server, and the tasks queue's "Fallback
   polling (5s)" toggle.
3. **`systemHome` — the 7.5 s landing dashboard** (carried; profile
   first, then decide v5-local fix vs record-and-leave).
4. The present-but-null validation lead (carried).
5. Widen the committed `characters-*` e2e fixture with a Quilltap
   General store (carried; still parks the component-transfer beat).
6. The next wrong-type-collapse order (`system_data_routes.rs` 13 sites,
   `files_routes.rs` 5 — P4.60's census, carried).
7. `p4.9i2` — help/HelpChat as a dedicated round (the bank gained the
   `help/system-tasks-queue.md` rows from BOTH of this round's commits).
8. The handler-logging sweep (carried; this round added v4's
   reconcile-pause `logger.warn` context lines to its inventory).
9. The duplicate "Quilltap General" e2e-fixture store (carried).

PB1 stays parked by the standing rule. The `qtap-export.schema.json`
file port remains a NAMED standalone flag.

**Next candidates, in rough value order** (updated at the 4.9.0-push round
unification, 2026-08-27 — the fourteen-commit drift block is fully absorbed,
baseline `8872d7efc`, drift debt CLEARED):

1. **The owed dogfood pass over this round's live surfaces + the standing
   💸 queue.** This round adds: a real pre-4.9 archive restored on the
   Friday copy (bug 103's seeding + the `Seeded connection-profile
   columns…` debug line in `combined.log`); a `glm-5.3-*` attachment
   reaching the real Z.AI wire as `image_url` (replaces the RETIRED
   refusal-sentence item); a real compression fold living past 45 s /
   abandoned at 75 s + the `[CheapLLM] Task failed` line; the About
   page's new bullet + provider sentence on screen; live shell
   completion (`docs docker-mounts --format <TAB>` in all three shells,
   `--uri`/`--base64` in fish); the two solid hover fills on a real
   hover. Carried from before: Pascal's group-tier write path, the
   Brahma deep-query budget, dedup/summaries (human), the NanoGPT
   caching smoke / #101 cost question.
2. **Watch for v4's `release: 4.9.0` squash + the new bugfix fork** —
   the ledger's §1 expects both; `/driftcheck` on arrival (probe BOTH
   branches).
3. Named follow-ups from this round's lanes: an oracle-side
   divergence-aware case kind for the bug-105 v4-regression tripwire
   (the arm belongs to `system_import_state` — v5's pin is unit-side
   only); the `attach_mount_file_equivalence` pre-existing red (oracle
   yields zero canned vision calls — needs its own diagnosis); the
   `a_fired_deadline_warns…` prefix-match target assert (one-line
   tightening); the `embedding_blob_binding_guard` notes (whole-file
   REGISTRY_ALLOWED exemption; comment-vacuity in the census).
4. `systemHome` — the 7.5 s landing dashboard (carried).
5. The present-but-null validation lead (carried).
6. Widen the committed `characters-*` e2e fixture with a Quilltap
   General store (carried; still parks the component-transfer beat).
7. The next wrong-type-collapse order (`system_data_routes.rs` 13 sites,
   `files_routes.rs` 5 — P4.60's census, carried).
8. `p4.9i2` — help/HelpChat as a dedicated round (the bank gained this
   round's rows: `help/system-backup-restore.md` "Restoring an Older
   Backup", `help/connection-profiles.md`, `help/cli-completion.md`,
   `packages/quilltap/README.md`).
9. The handler-logging sweep + the duplicate "Quilltap General"
   e2e-fixture store (carried).

PB1 stays parked by the standing rule. The `qtap-export.schema.json`
file port remains a NAMED standalone flag.

## The `d883a5ee1` drift catch-up round (P4.D153 ∥ P4.D154 ∥ P4.D155 ∥ P4.D156 ∥ P4.D157 ∥ P4.D158) — UNIFIED 2026-09-05

**UNIFIED on main (2026-09-05) — ALL SIX ORDERS CLOSED; the oracle baseline
MOVES to `d883a5ee1` and the drift debt is CLEARED** (fourteen rows absorbed
or ratified; `15573c3a1` / bug 119 stays UNPROCESSED for the unported `p4.9k`,
by the ledger's own instruction). Landed: bug 122's memory-subject prefix
through the three self-facing formatters at v4's template positions and
inside the token estimate, the RAW-path `find_names_by_ids`, the resolver's
zero-query early return, three call sites, the oracle case's positional arity
fixed FIRST (P4.D153) ∥ bug 121's USER-side attachment walk as a fourth
`message_context_leaves` leaf with v4's ten cases, the re-hydration BEFORE
`build_context` with the skip-whole budget and the `unsupported`-with-error
drop, the `load_user_attachments` seam, the orchestrator corpus widened to SEE
the splice (P4.D154) ∥ the `0506517d3` collapse's six server-side
corrections + the Pascal placeholder classifier ONCE on each side — five of
the seven were measured present in v5 (P4.D155) ∥ bug 120 red-first on Tier R
(214 → 216/0), the three About sentences, the two `qt-checkbox` attributes,
the Answer Confirmation row on the shared toggle row, (f1) a convergence by
construction (P4.D156) ∥ the `d4138b96b` dead-code decision: thirteen
symbols, every one option (ii) DELETE — not one twin had a production caller —
seven families SPLIT, none frozen, the LoRA bounds pinned at their new home
(P4.D157) ∥ the Opus 5 sampling strip red-first on two new corpus rows, every
provider corpus re-recorded at the pin with exactly two version-marker fields
moving, the three packaging rows ratified on that measurement, `2edd823c0`'s
four bag-key blind spots as restore arms over the new committed
`restore-archive-bag-keys.zip`, the `docs/v4/` mirror refreshed, the §G help
bank (P4.D158). **The §3 review's headline catch, fixed at the wire — it
contradicted a lane's ratification:** `6e1a64ea6`'s `zod` 4.4.3 → 4.5.4 DID
move v5 bytes. Both lanes diffed the LOCALE; the change is in core
`schemas.js` — a strict object's `unrecognized_keys` issue is now
`continue: true`, so an object with a stray key stays a live union branch and
its refines fire. Both hand-rolled Zod engines (`custom_tool_types.rs` and
its SPA twin) took the flag plus parsed-value `hasComparator` semantics,
red-first in two steps then green (258 definitions); the SPA's committed
corpus refreshed (13 rows) and nine hand-captured rows re-captured. Also at
the wire: the re-affirmation selection through the ONE `selection_from_
profile` (D155's should-fix), the name-lookup pool failure logged (D153), the
two `screens/custom-tools/**` placeholder readers neither lane owned, D157's
three doc references, and **P4.D158's unit 3 run here — the `0506517d3`
neutrality sweep: 409 families: 402 green, the seven non-green rows all run to ground — three were Zod 4.5's code-point length rule, one a fixture-vintage artifact, one an oracle mock lagging the collapse, one a moved import, one the deliberate repo-writer — none the collapse's; it is NEUTRAL**. Gate: fmt/clippy both feature sets clean; release build; 496 binaries / 2,802 / 0 / 1 ignored with the 67-var env block (Tier R 216/0 inside it); 409-family sweep 402 + 6 repaired + 1 refused; the round's families by name from the pin zero SKIP; ng 380 files / 5,962+; full Playwright 274/274 zero skips. Versions: core 0.0.795,
harness 0.0.685, cli 0.0.18, SPA 0.5.646. Round record: `status-log.md` →
"Round record — the `d883a5ee1` drift catch-up round unification".

**Next candidates, in rough value order** (updated at this unification,
2026-09-05):

1. **The owed dogfood pass** on the Friday copy over this round's surfaces
   (the 💸 list in the round record: bug 122 on a real multi-character turn,
   bug 121's second-responder quote, the opus-5 send on a real profile,
   `instances default --json`, the Workbench's bare `{{state.}}`, the
   priority-5 params on the wire) + the standing queue (Pascal's group tier,
   the Brahma deep query, dedup/summaries, #101, the LoRA wire-byte look).
2. **`p4.9i2` — help/HelpChat as a dedicated round** (the bank gained this
   round's rows: `help/file-uploads.md` "A word on company",
   `help/memory-recall-relevance.md` "Whose Life Is It, Anyway?", the four
   `e9a9c538e` help files, the help-chat API-key sentence; it also owns bug
   119's `p4.9k` sibling — the character optimizer with `15573c3a1`'s
   post-fix shape).
3. **P4.73's remainder:** `POST /api/v1/images?action=generate` and
   P4.62(a)'s FILES leg, plus the review's recorded items (the DELETE's
   orphan cleanup vs the archived-character guard, `zod_url_ok`'s
   authority-less schemes, the two `[Images v1]` info lines, the unbounded
   JSON read, the order-dependent `cannedFetch`).
4. **The Zod-emulation maintenance item this round opened:** v5 hand-rolls
   Zod semantics in TWO engines (`pascal/custom_tool_types.rs` + the SPA
   twin) and transcribes Zod sentences at ~150 edge sites; the oracle's
   `node_modules` resolve the LIVE tree, so every v4 dependency bump is a
   regen event for all of them. Worth a committed `zod-version` tripwire (a
   harness test that reads v4's installed `zod` version and fails when it
   moves past the recorded one) so the next bump is caught at ordering, not
   at a family's first red.
5. **The census's honest totality** — the per-variant allow-list of the
   fields v4 really reads from the URL and the ~160 unclassified body-key
   rows; then the fixes the census names.
6. **The present-but-null validation lead** (`api/**`-wide).
7. The carried smalls: the thirteen `CaptureLayer` copies → one
   `#[cfg(test)]` helper; the `?action=` endpoint census; `files_write_
   routes.rs`'s transcribed DDL; the `chat_files_post_*` 500-vs-400
   divergence; the streaming column's duplicated markup; the thirteenth
   participant-status copy + five harness copies; the `MessageContextSeams`
   dead `provider` argument on both methods; `render_template`'s four
   missing `logger.debug` lines (handler-logging inventory); the
   `announcer_tier3` fixture's blindness to the memory-subject prefix (the
   shared `post-office` pair needs a targeted memory).

PB1 stays parked by the standing rule. The `qtap-export.schema.json` file
port remains a NAMED standalone flag.

**The ordering-time section follows for history:**

## The `d883a5ee1` drift catch-up round (P4.D153 ∥ P4.D154 ∥ P4.D155 ∥ P4.D156 ∥ P4.D157 ∥ P4.D158) — ORDERED 2026-09-05

**Six parallel lanes, all drift.** The ledger's §2 probe passed at ordering
(v4 `main` at `d883a5ee1`, tree clean, both logs empty, no pin worktrees
outstanding); its §3 held FIFTEEN rows, of which FOURTEEN are ordered here
and marked `ORDERED(p4.d15x)` — the fifteenth (`15573c3a1`, bug 119)
belongs to the unported character optimizer (`p4.9k`) and stays UNPROCESSED
by the ledger's own instruction. **Regen rule: PIN REQUIRED at
`d883a5ee1`** (the round's target baseline) for every lane, with P4.D157
carrying a second pin at `0b0617fee` for its frozen-family evidence. This
is candidate 1 of the list below, grown from twelve commits to fourteen by
the two live-Friday bug fixes (121, 122) that landed after that list was
written — **both of which v5 reproduces whole today.**

- **P4.D153** — `work-orders/p4.d153-memory-subject-prefix-bug122.md`:
  v4 bug 122 (`d883a5ee1`) — the memory-subject prefix (`About <name>: ` /
  `About another character: `) through the three self-facing formatters at
  v4's exact template positions and INSIDE each line's token estimate, the
  RAW-path `find_names_by_ids` (v5's `find_by_ids` is the overlaid twin —
  the port must not reintroduce the vault dependency v4's docblock refuses),
  the `services/memory_subject.rs` resolver (no query when the subject set is
  empty), the three call sites (build-context over the archive∪head UNION,
  Carina, the character-voiced announcer). The oracle case's positional
  calls are fixed FIRST (the ledger's silent-regen trap), the corpus gains a
  `prefix` leaf + targeted-memory rows for all three kinds (today every
  self-facing row leaves `aboutCharacterId` null — vacuous). Owns
  `memory_injector.rs`, `db/characters_read.rs`, `build_context.rs`,
  `carina_query.rs`, `character_voiced.rs`.
- **P4.D154** — `work-orders/p4.d154-user-attachment-rehydration-bug121.md`:
  v4 bug 121 (`e288ae2ec`) — the USER-side attachment walk
  (`collect_unseen_user_attachments_for_character`, lookback 20) as a FOURTH
  `message_context_leaves` leaf with v4's ten cases, the re-hydration step
  BEFORE `build_context` (per-file 80,000-char budget that skips whole, the
  `unsupported`-with-error drop, the `messages_for_conversation` copy, the
  hoisted shared cutoff, the merged-attachments seeding), a sibling
  `load_user_attachments` seam on `MessageContextSeams`. The orchestrator
  tier-3 builder is widened with a `files` row + a USER attachment + a
  second character's turn — the corpus keeps attachments EMPTY today and is
  blind to the fix. Owns `message_context.rs`, `chat_files.rs`.
- **P4.D155** — `work-orders/p4.d155-collapse-corrections-server-and-pascal.md`:
  the `0506517d3` collapse's seven behaviour corrections — NOT the refactor:
  (a) priority-5 cheap-LLM selections carry `profile_parameters` + derive
  `is_local` (v5 reproduces the drop at `cheap_llm.rs:319/:334` and the
  hard-coded `false` at `cheap_llm_exec.rs:136`; the port IS the
  eight-twins-through-one-`selection_from_profile` collapse), (b) the export
  PREVIEW count through `is_file_excluded_from_export` (v5's `preview.rs`
  names this exact divergence in a comment), (c) `api/documents.rs:1060`'s
  "File not found not found" (a missing-file delete arm added red-first),
  (d) `brahma_console/mod.rs:359`'s lowercase sentence, (e) the Pascal
  placeholder classifier ONCE on each side (five Rust spellings + six SPA
  sites; a bare `{{params.}}` is `unknown`; `{{params.toString}}` pinned —
  the browser Workbench is where the prototype leak is real), (g) the
  self-inventory catch measured. Owns `cheap_llm*.rs`, `api/documents.rs`,
  `brahma_console/**`, `qtap_export/**`, `pascal/**` on both sides.
- **P4.D156** — `work-orders/p4.d156-client-cli-drift-bug120-about-checkbox.md`:
  bug 120 (`af2023c9a` — `instances default --json` read AND stripped; v5
  has the same defect and no JSON branch at all; Tier R red-first + the
  help line + the fish block), `e9a9c538e`'s three About sentences (v5 at
  the pre-fix bytes), `bbcb318c6`'s two `qt-checkbox` attributes (v5's
  inputs carry NO class), and `0506517d3`'s three CLIENT corrections ((f1)
  measured as a convergence by construction — v5 already shares one
  outfit-choice card; (f2) the server sentence preference, the wizard half a
  NO-COUNTERPART; (f3) the Answer Confirmation row onto the shared
  `qt-settings-toggle-row` six siblings already use). Owns
  `apps/web/src/app/screens/**`, `styles/**`, `crates/quilltap-cli/**`;
  holds port 4319.
- **P4.D157** — `work-orders/p4.d157-dead-code-sweep-decision.md`: the
  `d4138b96b` dead-code sweep — fourteen deleted v4 exports imported BY NAME
  by seven committed oracle cases, which fail to LINK at any pin past it.
  A per-SYMBOL decision with evidence: (ii) delete-and-retire where the v5
  twin is dead too (measured today: the pricing trio, the roster pair whose
  only caller has zero callers, the token-warning pair), (iii) unit-pin
  from the frozen oracle where a v5 twin is live, (i) FROZEN at `0b0617fee`
  only where a live twin cannot be pinned without v4. The families are
  SPLIT (surviving rows byte-identical), never retired whole. Plus the LoRA
  bounds' new-home check. Owns the seven twin modules (function-level) + the
  seven families; never `lib.rs` or a module declaration.
- **P4.D158** — `work-orders/p4.d158-wire-recheck-neutrality-ratifications.md`:
  the one known wire change (`48f4b42ec` — `^claude-opus-5(-|$)` in the
  anthropic sampling-rejected table, v5 reproduces the bug; two corpus rows
  red-first, no opus-5 row exists today), the packaging trio's corpus
  re-check at the pin (openai 7.4 → 7.10, openrouter 1.2.32 → 1.2.106,
  plugin-utils 2.6's `buildRequestBody`; byte-identical outside the
  self-dating markers, the P4.D76 method), the Zod 4.5 locale MEASURED at
  ordering (uuid regex identical; three message arms moved — `.length(n)`
  "exactly", non-finite names, exclusive unions — none of which v5
  transcribes), the `0506517d3` neutrality bulk sweep AFTER the siblings
  close (excluding every §A family by name), the three ratifications with
  `2edd823c0`'s four restore blind spots turned into corpus arms over a NEW
  archive (none of the four keys appears in any v5 restore family today),
  the `docs/v4/` mirror refresh (521 lines behind the baseline on `API.md`
  alone) + the `?action=` read, the §G help bank. Owns the provider corpora
  + recorders, the restore builders, `docs/v4/**`, one regex line.

**Shared contracts §A–§H + the Ownership table** are byte-identical across
the six orders (built from one scratch file and spliced; md5-checked).
**Pick order at unification:** P4.D157 (retires/splits families the others
never touch) → P4.D153 → P4.D154 (both core; disjoint files) → P4.D155
(core + `pascal/` SPA) → P4.D156 (SPA + CLI) → P4.D158 (corpora + restore
arms + the mirror; its sweep artifact is the round's neutrality record).
Five lanes bump core and/or harness — recount at the wire.

**Deliberately left out of the round:** `15573c3a1` (bug 119 → `p4.9k`);
the P4.73 remainder (`?action=generate`, P4.62(a)'s FILES leg); `p4.9i2`;
the census's honest totality; the present-but-null lead; the carried smalls
— all still below, unchanged. The owed dogfood pass (candidate 3) runs
after this round unifies, gaining bug 122's live proof (a multi-character
turn where one character's memories are ABOUT another), bug 121's (a text
attachment quoted by the SECOND responder), the opus-5 send on a real
profile, and `instances default --json`.

## The follow-ups round 2 (P4.72 ∥ P4.73 ∥ P4.74 ∥ P4.75) — UNIFIED 2026-09-04

**UNIFIED on main (2026-09-04) — P4.72 / P4.74 / P4.75 CLOSED, P4.73 PARTIAL
(the `?action=generate` leg + P4.62(a)'s FILES leg OPEN by its own header);
the oracle baseline STAYS `0b0617fee`, the ledger's one §3 row (`15573c3a1`,
bug 119 → `p4.9k`) unmoved.** Landed: the whole P4.67 remainder (the family at
32 endpoints, the per-site duplicate-key rows, P4.62(c)) + the dispatch
wrong-type census (240 rows / 125 variants, with its exclusion PINNED at 403
and its totality claim withdrawn) + the two `actionLogger.warn` pins ∥ the
`/api/v1/images` COLLECTION route as dispatch verbs + thin edges (list /
upload / import-from-URL over a NEW host fetch seam / the `{id}` DELETE
retiring the P4.9a2 refusal) over a NEW committed `images-{main,mount}.db`
pair and a 32-case real-DB family, **the host pixel codec threaded into the
chat-upload dispatch arm** (P4.D152's named candidate — a composition-level
wiring pin whose files-row half the §3 review found had never run), the
`ChatCreate` wrong-type trio answering v4's flat `Validation error` on BOTH
transports (the typed decode is the host driver's — both lanes were right
about different layers) ∥ the failover `auth` chain arm with BOTH reason
spellings, the eleventh + twelfth participant-status copies retired (a
THIRTEENTH found and census-named: `db/chats_messages.rs`, whose status IS
read), six shared-stage recipes re-staged lane-unique, all sixteen v4
`[Image Fallback]` calls dispositioned + `create_system_event`'s catch, and
the first WRITTEN handler-logging inventory (205 rows) ∥ the streaming
bubble's avatar column with v4's ONE `shouldShowAvatars` (v5's `≥2` arm was
an invention — removed), a mid-turn beat, the search-documents intermittent
root-caused to the BEAT (two causes, 30/30 ×10), the `title=` census as a
committed script + two byte-exact copy repairs, twelve of thirteen residue
hosts adjudicated (the thirteenth at unification). **The §3 review's
headline catch:** P4.73's dedup arm bypassed the UUID refusal and answered
201 where v4 answers 400 before any write — fixed with five new arms on both
sides, mutation-proven; plus twelve should-fixes across the four lanes (the
round record has every one). Gate: fmt/clippy both feature sets clean; 17/17 families fresh from the `0b0617fee` pin zero SKIP (+2 and +1 re-runs after the review fixes); **494 test binaries / 2,780 / 0 / 1 ignored, zero SKIP**; release build; ng 378 files / 5,945; full Playwright **274 / 0 / 0** after the gate's own catch — the image-detail beat's two seeds were pixel-identical and now dedup under the host codec, as v4's do (fixed spec-side). Versions: core 0.0.776,
harness 0.0.671, web 0.0.114, host 0.0.96, SPA 0.5.642. Round record:
`status-log.md` → "Round record — the follow-ups round 2 unification".

**Next candidates, in rough value order** (updated at this unification,
2026-09-04):

1. **The twelve-commit drift catch-up** → **ORDERED 2026-09-05 as the
   `d883a5ee1` round (P4.D153–P4.D158), grown to fourteen rows by bugs
   121/122** (ledger §3 — v4's 4.9 release-checklist push landed DURING
   this round): the Anthropic
   Opus 5 sampling strip (`48f4b42ec`), the CLI `instances default --json`
   bug 120 (`af2023c9a`, Tier R red-first), the About sentences + the
   cheap-LLM `qt-checkbox` (`e9a9c538e`, `bbcb318c6`), the big
   duplicate-collapse refactor with its named behaviour corrections
   (`0506517d3` — hunk survey first, then the D32-class neutrality sweep),
   the dead-code sweep's one check (`d4138b96b`, the LoRA bounds' new home),
   and the SDK/bundle wire re-check for the three packaging commits
   (`6e1a64ea6` / `b52b996c1` / `06658535f`). Pin `0b0617fee` for every
   regen until it runs. Four more help files join the `p4.9i2` bank.
2. **`p4.9i2` — help/HelpChat as a dedicated round** (the bank holds
   thirteen notes now).
3. **The owed dogfood pass over this round's surfaces** on the Friday copy
   (the 💸 list in the round record: the images collection route end to end,
   the chat-upload WebP transcode, the streaming avatar on a real
   multi-character turn, the search-open shape) + the standing queue.
4. **P4.73's remainder:** `POST /api/v1/images?action=generate` (the lane
   record's survey — the erased image-GENERATION seam, the Concierge stack
   from a route handler, both tripwires stay ARMED until it lands; the
   `activity_span_sites_guard` substring trap), and P4.62(a)'s FILES leg with
   its CORRECTED shape (v4 REFUSES a non-UUID id — 500 `Failed to upload
   file`, orphaned blob — where v5 filter-maps and succeeds). Plus the
   review's recorded items: the DELETE's orphan cleanup vs the
   archived-character write guard, `zod_url_ok`'s authority-less schemes, the
   two `[Images v1]` info lines, the unbounded JSON read, the order-dependent
   `cannedFetch`.
5. **The census's honest totality** — a per-variant allow-list of the fields
   v4 really reads from the URL and the ~160 unclassified body-key rows
   (`ChatSend`'s four, the announcement/mail ids, …); then the fixes the
   census names, by row (P4.72's second-level structs too).
6. **The present-but-null validation lead** (`api/**`-wide; ownership is free
   again).
7. The carried smalls: the thirteen `CaptureLayer` copies → one
   `#[cfg(test)]` helper; the `?action=` endpoint census (the family's list
   is hand-maintained); `files_write_routes.rs`'s transcribed DDL; the
   `chat_files_post_*` bare/empty 500-vs-400 divergence the family cannot
   see; the streaming column's duplicated markup; the thirteenth
   participant-status copy + five harness copies; `p4.9k` (with bug 119's
   post-fix shape); the `docs/v4/` mirror refresh.

PB1 stays parked by the standing rule. The `qtap-export.schema.json` file
port remains a NAMED standalone flag.

**The ordering-time section follows for history:**

## The follow-ups round 2 (P4.72 ∥ P4.73 ∥ P4.74 ∥ P4.75) — ORDERED 2026-09-03

**Four parallel lanes, no drift.** The ledger's §2 probe passed at ordering
(v4 `main` at `15573c3a1`, tree clean, both logs empty); its one §3 row
(`15573c3a1`, bug 119) belongs to the unported character optimizer (`p4.9k`)
and is NOT ordered here — it stays UNPROCESSED by the ledger's own
instruction. **Regen rule: PIN REQUIRED at `0b0617fee`** for every lane. The
round is drawn from candidates 2–5 of the list below (candidate 1, the owed
dogfood pass, ran 2026-09-03 and is marked DISCHARGED below): the second
non-drift round since P4.59, taking the follow-ups debt three rounds have
carried.

- **P4.72** — `work-orders/p4.72-query-param-remainder-and-dispatch-type-census.md`:
  the P4.67 remainder whole (the other seventeen `?action=` sites into
  `query_param_semantics_equivalence`, the per-site duplicate-key rows,
  P4.62(c)) + the dispatch-level wrong-TYPE **census** (every `Request`
  variant whose v4 twin is a Zod `.parse`, the ordered per-verb shape —
  the fix crosses three lanes' files, so the `ChatCreate` trio's fix is
  P4.73's). Owns `quilltap-web/src/**` (less `lib.rs`, the new
  `images_routes.rs`, two named regions) + `api/chat_media.rs`.
- **P4.73** — `work-orders/p4.73-images-collection-route-and-ingest-codec.md`:
  the never-ported `/api/v1/images` COLLECTION route (list / upload /
  import-from-URL / `?action=generate` / the `{id}` DELETE that today
  answers a loud refusal) as dispatch verbs + thin edges + a NEW real-DB
  family, retiring the two tripwires built to fire when it lands
  (`lora_log_anchor_guard`'s ninth anchor, `activity_span_sites_guard`'s
  row 9); **the host codec threaded into the chat-upload dispatch arm and
  every images arm** (P4.D152's named candidate — a NEW convergence with a
  composition-level wiring pin); P4.62(a); the `ChatCreate` wrong-type trio.
  Owns `api/types.rs`, `api/engine.rs`, `api/files.rs`, the new
  `api/images.rs`, the host seams, `core-contract.ts`.
- **P4.74** — `work-orders/p4.74-core-smalls-auth-arm-status-copies-recipes-logging.md`:
  the failover `auth`/`no-api-key-configured` chain arm in
  `primary_stream_tier3_equivalence` (P4.68's written-out shape), the
  eleventh + twelfth participant-status copies onto the one home, every
  recipe staging into the shared `/tmp/qt-oracle-stage` re-staged
  lane-unique, the `[Image Fallback]` fields + `create_system_event`
  capture-pinned, and the first WRITTEN handler-logging inventory. Owns
  three named service files + the named harness families.
- **P4.75** — `work-orders/p4.75-spa-smalls-streaming-avatar-search-intermittent-title-census.md`:
  the streaming bubble's avatar column (v4 `StreamingMessage.tsx:85` —
  ALWAYS-only gate, the responding-character resolver, the danger ring for
  free) with a mid-turn beat; the `workspace-search-documents` in-chat
  intermittent's ROOT CAUSE (1-in-3 red in isolation — not suite context);
  the SPA-wide `title=` census as a committed script + fills; the dozen
  residue hosts adjudicated; `#move-folder` measured. Owns the SPA +
  Playwright.

Binding across all four: §A well-formed actions never move; §B one query
reader; §C one participant-status home; §D one codec accessor
(`Engine::qtap_pixel_codec()`); §E the `Request`/`Response` enums are
P4.73's; §F/§G the two named same-file regions (`photos_routes.rs`'s
`image_delete_not_available`, `files_routes.rs`'s `files_upload_post` tags
block) — the only sanctioned same-file splits, named in commit messages;
§H the streaming avatar is client-only. Version-bump ownership is in each
order (three lanes bump core + harness; two bump web; two bump the SPA —
recount at unification).

**Deliberately left out of the round:** `p4.9i2` (help/HelpChat — a
standalone ~2,500-LOC vertical with eight bank notes, wanting its own
survey-heavy round, recommended NEXT), `p4.9k` (the character AI dialogs +
bug 119's post-fix optimizer shape — the same class), the present-but-null
validation lead (`api/**`-wide; would collide with P4.72/P4.73's `api/*.rs`
regions — next round, after §E's ownership frees up), the fixes the P4.72
census will name (by row, next round), the `docs/v4/` mirror refresh, and
the standing 💸 queue (Pascal's group tier, the Brahma deep query,
dedup/summaries, #101, the LoRA wire-byte look). PB1 stays parked by the
standing rule; the `qtap-export.schema.json` file port remains a NAMED
standalone flag.

## The `0b0617fee` drift catch-up round (P4.D148 ∥ P4.D149 ∥ P4.D150 ∥ P4.D151 ∥ P4.D152) — UNIFIED 2026-09-03

**UNIFIED on main (2026-09-03) — ALL FIVE ORDERS CLOSED; the oracle baseline
MOVES to `0b0617fee`; the `15573c3a1` row (bug 119, the unported character
optimizer → `p4.9k`) stays UNPROCESSED in the ledger's §3 by design.** v4's
five-commit day absorbed whole: the Concierge state chosen at chat creation
end-to-end (server: `conciergeState` on the create request through the
EXISTING flip chokepoint on all three branches right after the system-prompt
message, the greeting ladder's attempt 0 on the uncensored desk asked WITH
the chat row, the one shared desk closure, the capstone corpus 19 → 32 with
two NEW comparands — `message_order` over `rowid` and `stream_calls` over the
ordered call trace — and the harness `NoApiKeys` seam that had made every
Concierge reroute unreachable; SPA: the dropdown in v4's slot, the
omit-when-monitored body rule, v4's two client tests 1:1, the gated
create-time beat flipped LIVE at unification; Continue Elsewhere seeding a
recorded NO-COUNTERPART) ∥ bug 115 (`distill_memory_search` takes the latency
class; the fallback interactive — pinned at the REAL call sites by a
budget-recording provider since a deadline class is provably invisible to
the corpus: the build-context oracle is byte-identical at the target pin,
the old baseline AND the new one) + the inter-character timing log
(capture-pinned, the `!skip_memories` arm included) ∥ bug 116 (the describer
arrival verdict ahead of every content check, the `CompletionResponse.
cache_usage` widening at 23 sites with the real composition's thread pinned,
the tier-3 corpus widened with optional `attachmentResults`/`cacheUsage`
bags — 6 of 8 new rows RED first, the invented kitten description persisted
pre-fix — and a 14-row tier-1 `verdict` kind) + bug 118 re-proven a no-op
(eleven manifests byte-identical from the pin — v5 has carried the truthful
block since P4.D107) ∥ bug 117's four legs (transcode-then-hash at chat
upload with the codec as a PARAMETER — production still passes
`NotConfiguredPixelCodec`; import + both restore arms from the bridge / the
archived blob; the `realign-file-entry-sha256-v1` boot heal in the P4.D140
ledger shape over v4's REAL migration, with its presence-vs-drift stamp rule
a RECORDED both-directions divergence; the within-tree BOOLEAN comparand
with a harness-only byte-changing codec, and the honest measurement that
the join half is non-discriminating by construction — the DEDUP is the
red-first arm). **The §3 review (five parallel readers + the unifier's own
read): NO blocking findings — the fifth such round;** fixed at unification:
the heal's blob lookup folding EVERY DB error into the "orphaned" bucket
(v4 lets a driver throw escape), a boot comment claiming v4 parity where the
lane had pinned a divergence, three shape items in the create path (a dead
emptiness guard, a silent missing-id no-op v4 lacks, an over-claiming
Err-parity comment), a spliced harness doc, a stale field comment. Gate:
26/26 families fresh from the unify pin zero SKIP with changed-bytes greps;
clippy both feature sets; release build; 489 test binaries / 2,761 / 0 with the round's env block, zero SKIP; ng 376 / 5,925; full Playwright 271 passed / 1 failed / 0 skipped (the red is the documented `workspace-search-documents` intermittent — same shape, 1-in-3 red in isolation on this build, no lane touched the surface; promoted to a named candidate). Round
record: `status-log.md` → "Round record — the `0b0617fee` drift catch-up
round unification". Versions: core 0.0.768, harness 0.0.662, web 0.0.105,
host 0.0.95, SPA 0.5.631.

**Next candidates, in rough value order** (updated at this unification,
2026-09-03):

1. ~~**The owed dogfood pass over this round's surfaces**~~ **DISCHARGED — the
   pass RAN 2026-09-03** (15 rows, 13 PASS, 1 PARTIAL, 1 human; zero v5
   defects; eight 💸 items discharged; B6 discharged the same day —
   `dogfood-walks/2026-09-03-concierge-creation-sha256-pass.md`, the
   `status-log.md` record). Still owed from the standing queue: Pascal's
   group tier, the Brahma deep-query budget, dedup/summaries, #101, the LoRA
   wire-byte look. _(The list as it stood at unification follows.)_ The owed
   dogfood pass over this round's surfaces on the Friday copy:
   a chat created Uncensored greeting from the frank desk (the Concierge
   bubble second in the transcript, the sidebar control reading it back);
   the describer verdict against a real gateway that drops images (the
   live 38-token shape); **the sha256 heal — measure the population FIRST**
   (ledger §5.5; v4 will have run its own migration, so the expected proof
   is v5 honouring the ledger row and writing nothing); the interactive
   distill budget on a stalling cheap route; plus the standing queue
   (Pascal's group tier with its recipe, the Brahma deep query,
   dedup/summaries, #101, the re-measured compression row, the LoRA
   wire-byte look, and the follow-ups round's items — the danger ring on a
   real Flagged chat, the subset refusals, the Docker Ollama walk, the LoRA
   modal's writers, `count: 20` through the image-profile route, the
   failover rows).
2. **The P4.67 remainder** (its order's OPEN list: seventeen `?action=`
   sites into the family, the per-site duplicate-key rows, P4.62(a) the
   `FileUpload.tags` raw carry, P4.62(c) the `chat_file_link` guard tidy;
   the recorded `characters_get` fold) — and, joining that census, **the
   dispatch-level wrong-TYPE class this round's review named:** a
   non-string `conciergeState` / `roleplayTemplateId` / `timestampConfig`
   answers the dispatch decode envelope where v4's `createChatSchema.parse`
   answers the flat `Validation error`.
3. **Threading the host codec into chat uploads** (P4.D152's named
   candidate — v4 transcodes chat-uploaded bitmaps through sharp; v5 stores
   the original bytes, the documented `api/files.rs:1116` passthrough; a
   one-line change at `api/engine.rs` now that the codec is a parameter,
   but a NEW convergence that needs its own differential over a
   byte-changing codec and a dogfood look at real uploads).
4. **The follow-ups round's items 5–6, carried:** the streaming bubble's
   missing avatar (P4.69), the `auth`/`no-api-key-configured` chain arm
   (P4.68), the eleventh/twelfth status-parser copies, the
   `POST /api/v1/images?action=generate` route, the three shared-stage
   recipes, `p4.9i2` (the HelpChat/Guide client + the banked help docs —
   now six more rows from this round), the handler-logging sweep (the
   `[Image Fallback]` warn fields are unpinned — a capture-layer test),
   the present-but-null lead, the `title=` census, the dozen residue
   hosts, finding #109 (v4-first).
5. **The `workspace-search-documents-flow.spec.ts:208` suite-context
   intermittent, now a NAMED maintenance item** (third round it has fired:
   `.qt-chat-messages-list` resolved-but-HIDDEN after the in-chat card
   click — the SILENT standalone arm fires and its tab backgrounds the
   salon; green in isolation, once reproduced in a two-spec recheck with no
   round-related spec present). It wants its own root-cause look
   (`open-document-from-search` + the workspace tab activation order), not
   another "recorded, not this lane" line.
6. **The `15573c3a1` row (bug 119)** rides `p4.9k` (the character
   optimizer is unported); the `docs/v4/` mirror refresh at the next
   maintenance pass.

PB1 stays parked by the standing rule. The `qtap-export.schema.json` file
port remains a NAMED standalone flag (P4.D152 banked the `files.sha256`
`description` hunk there).

**The ordering-time section follows for history:**

## The `0b0617fee` drift catch-up round (P4.D148 ∥ P4.D149 ∥ P4.D150 ∥ P4.D151 ∥ P4.D152) — ORDERED 2026-09-02

**A drift round.** The ledger's §2 probe passed at ordering (v4 `main` at
`0b0617fee`, tree clean, both logs empty) and its §3 held FIVE UNPROCESSED
rows — one PORT-NEW, three PORT (one log-only), one NO-PORT? — none a
convergence (bugs 115–118 are v4's own filings). The standing rule (drift
debt before new scope) makes the catch-up the whole round; the follow-ups
list's item 1 (the three-row catch-up) grew to five rows when v4 filed and
fixed bugs 116–118 the same evening, and its item 3 (the v5-side
measurements those filings asked for) was TAKEN at the drift check and is
folded into the two bug-fix lanes. Five lanes, ownership disjoint at file
level, the shared-contract + ownership blocks byte-identical across all
five orders (md5-checked at ordering). Orders under `work-orders/`:

- **P4.D148** `p4.d148-concierge-at-creation-server.md` — the server half
  of `303288fb4`: `conciergeState` on the `chatCreate` request (v4's
  spelled-out enum, `.optional()` not nullable), `apply_requested_
  concierge_state` through the EXISTING `apply_concierge_flip` chokepoint
  at all three create branches right after the system-prompt message (the
  continuation branch BEFORE the replay), the greeting ladder's "attempt 0"
  — the reroute body reshaped into ONE closure asked WITH the fresh chat
  row (a Vouched chat never reroutes under a global AUTO_ROUTE; an
  Uncensored one reroutes under a global OFF), the content-filter attempt
  reusing it and skipping when attempt 0 ran, five log lines; the capstone
  corpus WIDENED (it seeds no danger settings and no uncensored profile
  today) with ten red-first cases incl. the `Briefing the Concierge…`
  frame and the bubble-position dumps.
- **P4.D149** `p4.d149-concierge-at-creation-spa.md` — the client half:
  the **The Concierge** dropdown above Starting Scenario (`Monitored
  (default)` — v4's form label differs from the sidebar's), the tone icon,
  the shared `detail` sentence (no `hint`), `conciergeState` in the form
  state with the OMIT-when-monitored body rule, v4's two client tests
  transcribed 1:1, an ungated body-rule beat + a GATED create-time beat
  (`P4D148_SERVER_LANDED`); Continue Elsewhere seeding is a NO-COUNTERPART
  (v5's continue-chat flow is unported — recorded); two help hunks →
  `p4.9i2`.
- **P4.D150** `p4.d150-interactive-distill-and-timing-log.md` — bug 115
  (`distill_memory_search` gains `options: CheapLlmTaskOptions`; the
  build-context FALLBACK passes `interactive()`, the proactive pass and
  recall-replay keep the default — v5 reproduces the 90 s + free retry
  today) pinned by the P4.D136 unit idiom (a budget-recording provider at
  both sites + call counts; the corpus is provably blind — the
  pin-vs-baseline byte-identity is recorded as the measurement) + the
  `c9faa2c74` inter-character timing debug line (`durationMs`,
  `loadedCount` = importance + relevance lengths, `includedCount`),
  capture-pinned with three arms.
- **P4.D151** `p4.d151-describer-arrival-verdict-bug116.md` — bug 116:
  `verify_image_reached_model` (attachment ledger first; absent/zero usage
  is SILENCE; cache reads added back; `<= 66` refuses) ahead of every
  content check in the describe tier, the warn + the long sentence + the
  metadata keys, failing INTO the fallback chain; the
  `CompletionResponse.cache_usage` widening (§B — the lane edits every
  construction site incl. ten harness one-liners); the corpus widened with
  optional `attachmentResults`/`cacheUsage` on the canned vision entries +
  six red-first cases; `verifyImageReachedModel` as a tier-1 `fallback_
  engine` kind; bug 118 as a manifest regen proven BYTE-IDENTICAL (v5 has
  carried the truthful block since P4.D107 — measured at the drift check);
  `b448eddd7` ratified NO-PORT with its file list.
- **P4.D152** `p4.d152-files-sha256-stored-bytes-bug117.md` — bug 117's
  four legs: chat upload transcodes THROUGH the bridge's own function
  before hashing and records the bridge's `sha256` (+ the disagree warn),
  `.qtap` import and restore's replay branch take the bridge's hash, the
  carried-store-rows branch reads the archived blob's own hash by parsed
  blob id (the RULED DIVERGENCE at `orchestrator.rs:994` re-read first),
  and v4's `realign-file-entry-sha256-v1` migration as a boot heal in the
  P4.D140 ledger shape (honour either app's row; write only on a pass that
  realigned ≥ 1). **Measured at ordering:** v5's chat-upload path hands
  the bridges `NotConfiguredPixelCodec` (a documented passthrough
  divergence), so v5's OWN upload rows never had the symptom — the live
  damage is on rows v4 wrote on the shared instance and on v5's
  import/restore rows (those paths carry the host codec); threading the
  host codec into chat uploads is a NAMED CANDIDATE, not this lane's. The
  comparand is a within-tree BOOLEAN (`files.sha256 == doc_mount_blobs.
  sha256`) with a harness-only byte-changing codec so the upload arm is
  red first; a NEW heal family over v4's REAL migration with its seven
  cases; a NEW committed `restore-archive-bug117.zip`.

Shared contracts §A–§G (identical in all five): the create-time wire
(P4.D148 ↔ P4.D149), the `CompletionResponse.cache_usage` widening is
P4.D151's alone (P4.D150 edits no harness file), `distill_memory_search`'s
signature is P4.D150's, the `files.sha256` invariant + the heal's ledger
shape are P4.D152's (P4.D151 keeps `attach_mount_file_equivalence`), the
manifest regen is P4.D151's, Playwright is P4.D149's with the create-time
beat gated, lanes never write the ledger. **Unifier pick order: P4.D150 →
P4.D151 → P4.D152 → P4.D148 → P4.D149** (the smallest core lane first; the
widening before the two lanes whose harness files it touches; the SPA +
full Playwright last, flipping `P4D148_SERVER_LANDED`). Four lanes bump
core, three bump harness — recount at unification.

**Execution arrangement:** disk is the constraint (89 GB free at ordering
against the playbook's 50–70 GB per long lane) — run in TWO waves with
`CARGO_INCREMENTAL=0`: wave 1 = P4.D148 + P4.D151 + P4.D152 (the three
heavy core lanes), wave 2 = P4.D150 + P4.D149 (small core + the SPA lane,
which needs no cargo target). Opus-class agents for P4.D148/P4.D151/
P4.D152 (each reshapes a spine and designs a corpus); Sonnet-class for
P4.D150 and P4.D149 (transcriptions with named pins). One worktree per
lane; `df -h ~` between waves.

**Deliberately left out of the round:** the P4.67 remainder (seventeen
`?action=` sites, the duplicate-key rows, P4.62(a)/(c) — non-drift, and
it would collide with nothing here but the standing rule clears drift
first), the follow-ups list's items 5–6 (the streaming-bubble avatar, the
`auth` chain arm, the eleventh/twelfth status-parser copies, the
`POST /api/v1/images?action=generate` route, the three shared-stage
recipes, `p4.9i2`, the handler-logging sweep, the present-but-null lead,
the `title=` census, finding #109), the host-codec-for-chat-uploads
convergence P4.D152 names, the `docs/v4/` mirror refresh (the next
maintenance pass, after the baseline moves), and the standing 💸 queue
(now gaining: a chat created Uncensored greeting from the frank desk on
real data, the describer verdict against a real gateway that drops
images, the sha256 heal on the Friday copy — measure the population first
per ledger §5.5, the interactive distill budget on a stalling cheap
route). PB1 stays parked by the standing rule; the `qtap-export.schema.
json` file port remains a NAMED standalone flag (P4.D152 banks its
`description` hunk there).

## The follow-ups round (P4.67 ∥ P4.68 ∥ P4.69 ∥ P4.70 ∥ P4.71) — UNIFIED 2026-09-02

**UNIFIED on main (2026-09-02) — P4.68, P4.69, P4.70 and P4.71 CLOSED; P4.67
CLOSED for Tier 1 items 1–2/4 + P4.62(b), PARTIAL for Tier 1 item 3 and Tier
2 item 5, OPEN for P4.62(a)/(c); the oracle baseline STAYS `6d2a50382`.** A
non-drift round — but v4 drifted TWO more commits DURING it (`02d4efa1b` bug
115 + `c9faa2c74`, on top of the `303288fb4` the round started against), so
every regen in every lane and at the unification ran from a pinned worktree at
`6d2a50382`; the ledger's §3 holds THREE UNPROCESSED rows and its §1 records
the human's three open v4 filings (bugs 116–118, each "Applies" to v5). Round
record: `status-log.md` → "Round record — the follow-ups round unification".
The §3 unification review (five parallel readers + the unifier's reads) found
TWO blocking findings, both P4.67 — the subset edges answering v4's envelope
for actions v4 dispatches and v5 does not (advertising `scan` in the sentence
that refused it), and coverage claims exceeding the code (14 of ~31 sites, no
duplicate-key row) — both fixed on the unify branch, plus seventeen should-fix
items across all five lanes (headline: the `orchestrator_tier3` wiring census
that a tool-unsupported retry in the corpus would have reddened on a correct
tree; the bare-executor census that ended its production zone at a mid-file
`#[cfg(test)]`; `ChatRefetchTally`'s mark that was always zero; the
image-profile generate route running the TOOL's schema where v4's ROUTE
refuses first; the Ollama key-test URL that repaired v4's double slash). The
unify's own first catch: P4.67's committed recipe named its `/tmp` pin, the
rule P4.71 wrote down the same day. Gate: 43 + 7 + 61 families fresh from the pin, zero SKIP; clippy both feature sets; release build; ng 376 / 5,911; full Playwright 270/0/0 (zero-skip); **488 test binaries / 2,745 passed / 0 failed / 1 ignored — exit 0, ZERO `SKIP:` lines**.

**Next candidates, in rough value order** (updated at the follow-ups round
unification, 2026-09-02):

1. **The three-row drift catch-up** (ledger §3, all UNPROCESSED): `303288fb4`
   the Concierge state on the New Chat form (PORT-NEW — `conciergeState` on
   `POST /api/v1/chats` through `applyConciergeFlip` after the system-prompt
   message at all three create branches, the greeting ladder's "attempt 0" on
   the uncensored desk, the form's dropdown, Continue Elsewhere seeding;
   `services/chat_create.rs`, `manual_flip.rs`, the SPA `screens/new-chat/**`,
   the Continue Elsewhere dialog; families `chat_create_capstone`,
   `initial_greeting`, `first_message_context`), `02d4efa1b` bug 115 (PORT —
   the dynamic-head fallback distill passes `interactive`; v5's
   `build_context.rs:2339` call carries no class today and reproduces the bug;
   the pin is P4.D136's compile-pin idiom), `c9faa2c74` (PORT log-only — the
   inter-character memory timing debug line, capture-pinned). One lane; the
   three rows' surfaces overlap on `build_context.rs`/the chat-create spine.
2. **The owed dogfood pass over this round's surfaces** on the Friday copy:
   the danger ring on a real Flagged chat (and its absence on an Uncensored
   one), the unknown-action envelope + the restored subset refusals through a
   v4-shaped client, the Docker walk with a real Ollama profile at
   `localhost:11434` (P4.71's 💸 — and the one `docker build` on a quiet
   machine the lane could not run: it OOMs here from `main`'s unmodified
   Dockerfile too), the LoRA modal's structured writers on a real NanoGPT
   profile, a `count: 20` through the image-profile generate route, the
   failover legs' `llm_logs` rows on a real understudy; plus the standing
   queue (Pascal's group tier with its recipe, the Brahma deep query,
   dedup/summaries, #101, the re-measured compression row, the LoRA
   wire-byte look).
3. **The v5-side measurements the three open v4 filings ask for** (ledger
   §1): bug 116 — does v5's describe tier (`services/file_fallback.rs`) read
   `attachmentResults.failed` / `usage.promptTokens` before believing a
   description; bug 117 — which bytes v5's chat-upload path hashes (input vs
   stored) at `api/chat_media.rs` / the upload bridge; bug 118 — whether the
   generated NanoGPT manifest carries v4's stale `attachmentSupport` block
   (the generator's augmentation table is the fix site). Each becomes a
   CONVERGENCE row when v4 fixes it; measuring first tells the port whether
   it is already divergent.
4. **The P4.67 remainder** (its order's OPEN list): the other seventeen
   `?action=` sites into the family, per-site duplicate-key rows for the
   classified non-action keys, P4.62(a) the `FileUpload.tags` raw carry,
   P4.62(c) the `chat_file_link` guard tidy; the recorded `characters_get`
   fold.
5. **The follow-ups this round's own measurements opened:** v5's streaming
   bubble renders NO avatar (P4.69 — porting it needs
   `respondingParticipantId` resolved against the cast + v4's
   `shouldShowAvatars` gate; the danger ring then lands there for free); the
   `auth`/`no-api-key-configured` chain arm (P4.68's named shape: a per-call
   primary override + a keyless understudy with `allowTierFallback: false`,
   both `modelClass: null`); `answer_confirmation.rs:336`'s eleventh
   status-parser copy with `parse_attr_status`'s old unknown→Active rule
   (out of P4.68's ownership; one line); the harness's twelfth copy that
   `panic!`s on an unknown status (`message_attribution_equivalence.rs:27`);
   the `POST /api/v1/images?action=generate` route v5 never ported (v4's
   ninth `[Image LoRA]` anchor AND the one image call site that rewrites a
   `baseUrl` — the `lora_log_anchor_guard` tripwire fires the day it lands);
   the three families still staging into the shared `/tmp/qt-oracle-stage`
   (`mail_carina_tools`, `photo_tools`, `precompute`); the two v4 filing
   candidates in `dogfood-findings.md`'s standing notes (the un-rewritten
   plugin default base URL; the Ollama double slash after a gateway rewrite).
6. The carried smalls: `p4.9i2` (the HelpChat/Guide client + the banked help
   docs), the handler-logging sweep (P4.68/P4.70 each added a row), the
   present-but-null validation lead, the SPA-wide `title=` census (612 v4 /
   379 v5), the dozen residue hosts, `#move-folder`'s single-instance id,
   finding #109 (v4-first).

PB1 stays parked by the standing rule. The `qtap-export.schema.json` file
port remains a NAMED standalone flag.

**The ordering-time section follows for history:**

## The follow-ups round (P4.67 ∥ P4.68 ∥ P4.69 ∥ P4.70 ∥ P4.71) — ORDERED 2026-09-02

**Not a drift round.** The ledger's §2 probe passed at ordering (v4 `main`
at `6d2a50382`, tree clean, both logs empty, §3 EMPTY) and the
`6d2a50382`-round dogfood pass had already discharged the previous
candidates list's item 1 (22/22 PASS, zero v5 defects), so the round is
drawn from the carried candidates (items 2–4 above, the P4.D138
follow-up's items 2–6) plus the P4.62/P4.D135 deferred shapes. Five lanes,
ownership disjoint at file level, the shared-contract + ownership blocks
byte-identical across all five orders (md5-checked at ordering). Orders
under `work-orders/`:

- **P4.67** `p4.67-query-param-semantics-sweep.md` — the query-parameter
  semantics class at every v5 REST edge: v4's ONE action reader
  (`lib/api/middleware/actions.ts` — `''` is falsy → the no-action leg;
  unknown → `{error: "Unknown action: <x>", availableActions}`; the FIRST
  duplicate wins) vs v5's 31 `Query<HashMap>` reads in 13 route files
  (LAST wins, `Some("")` reaches the "other" arms with v5-invented
  sentences). A shared first/last/all reader, every `?action=` site
  rewritten, every non-action key classified by its v4 reader shape, a
  NEW web-edge family over the DB-free jest idiom; plus P4.62's three
  deferred core shapes (`FileUpload.tags` raw carry, `payload: []` on the
  jobs enqueue, the `chat_file_link` guard tidy).
- **P4.68** `p4.68-status-parsers-and-cheap-llm-remainder.md` — the
  behaviour-neutral consolidation of the seven `parse_status` copies +
  three string-level tool sites onto `chat_predicates::
  participant_status_from_str` (the two `build_context.rs` parsers use
  DIFFERENT unknown rules — adjudicated per v4 twin, never unified
  blind), the failover log thread at the orchestrator's empty-response
  recovery call (every leg's `CHAT_MESSAGE` row, as v4's `restreamInto`),
  the three chain-walk corpus blind spots, the `CheapLlmTaskExecutor::
  new()` gap measured (the 2026-09-02 grep finds every bare site past a
  `#[cfg(test)]` marker — closed by census if that holds), and
  `precompute_equivalence` made discriminating on the uncensored reroute
  (an uncensored profile seeded into the `episodic-recall-*` pair,
  `allProfiles` threaded both sides).
- **P4.69** `p4.69-spa-followups-danger-ring-modal-beats.md` — the SPA
  lane (owns Playwright): v4's assistant-side message-avatar danger ring
  never ported (`SalonView:1489` → `VirtualizedMessageList` →
  `MessageRow`/`StreamingMessage` → `MessageDesktopAvatar`'s
  `qt-chat-avatar-dangerous`, Flagged only — v5's CSS rule exists, no
  input does), the two v5-invented quick-hide `console.warn`s (v4's probe
  hook is a bare `catch {}`), the image-profile modal's structured
  writers (a LoRA write clobbers a mid-edit JSON textarea — a v5-invented
  control), the temp bubble's seat, `waitForChatRefetch`'s scope + the
  injection hook's silent degradation, the workspace-search positional
  pick, the six slider suffix pins, and the component-transfer beat's
  precisely-scoped un-park (materialize `projects` + `groups` in
  `beforeAll`; the Copy arm's stale count).
- **P4.70** `p4.70-image-schema-and-fixture-debts.md` — the whole
  `generate_image` tool-input schema (v4's Zod `safeParse` incl. the
  `llmNumber` string coercion on `count`, max 10; v5 validates the
  prompt only, so `count: 20` generates where v4 refuses), red-first
  over `image_generation_tier3`; the `[Image LoRA]` warnings' dropped
  `{context, chatId, jobId, profileId}` spread (v4 has five sites, v5
  two) + the `style-options` anchor, capture-pinned; the
  `image_gen_leaves` header's shared `/tmp/qt-oracle-stage`; the
  committed `system-data-*` fixture widened to the baseline vintage (the
  connection-profile import leg has measured nothing since bug 68) with
  a cell census and all nine consumers re-run; the
  `projects_routes_equivalence` `latest_chat` GET arm + a planted
  retired-mode row so the normalize line discriminates.
- **P4.71** `p4.71-host-gateway-resolver.md` — v4 `lib/host-rewrite.ts`
  whole into `quilltap-host` (`isVMEnvironment`, the once-cached
  `resolveHostGateway` — `QUILLTAP_HOST_IP` then Docker
  `host.docker.internal`, no bridge-IP fallback — the three log lines),
  injected at every construction site whose v4 twin rewrites
  (`provider-registry.ts`'s five `resolveBaseUrl` sites +
  `abstract-provider-registry.ts:201`'s `validateApiKey`), the ONE
  missing core seam (`completion_provider.rs:184`'s hard `None`), a
  tier-1 family against v4's REAL module with the environment mocked per
  row, wiring pins per site, and the Linux `--add-host` flag +
  `QUILLTAP_HOST_IP` in `running.md`. 💸 the container walk with a real
  Ollama profile joins the dogfood queue.

Shared contracts §A–§F (identical in all five): well-formed actions never
move (P4.67 ↔ P4.69), ONE participant-status home (P4.68), danger
styling is a client predicate (P4.69), the `system-data-*` fixture is
P4.70's this round and P4.67 consumes it at base (the unifier re-runs
P4.67's three consuming families over the widened pair), the gateway
seam is P4.71's (P4.68's census RECORDS host findings), the
`episodic-recall-*` pair is P4.68's. Unifier pick order: P4.68 → P4.71 →
P4.70 → P4.67 → P4.69 (core consolidation first; the host seam next; the
fixture widening before the web lane that consumes it; the SPA + full
Playwright last). Four lanes bump core and harness — recount at
unification.

**Deliberately left out of the round:** `p4.9i2` (the HelpChat/Guide
client + the banked help docs — a standalone vertical wanting its own
round), the handler-logging sweep (log-only; P4.68/P4.70 carry two of
its rows as Tier 3), the present-but-null validation lead (`api/**`-wide;
would collide with P4.67's three `api/*.rs` regions), the SPA-wide
`title=` census (612 v4 sites vs 379 v5 — recorded as Tier 3 in P4.69),
the dozen residue hosts (visual judgments), finding #109 (a v4-first
filing), and the standing 💸 queue (Pascal's group tier with its recipe,
the Brahma deep query, dedup/summaries, #101, the re-measured
compression row) — the next dogfood pass gains this round's surfaces
(the danger ring on a real Flagged chat, the unknown-action envelope, the
Docker Ollama walk). PB1 stays parked by the standing rule; the
`qtap-export.schema.json` file port remains a NAMED standalone flag.


## The `6d2a50382` drift catch-up round (P4.D143 ∥ P4.D144 ∥ P4.D145 ∥ P4.D146 ∥ P4.D147) — UNIFIED 2026-09-02

**UNIFIED on main (2026-09-02) — ALL FIVE CLOSED; the oracle baseline MOVES
to `6d2a50382` and the drift debt is CLEARED (the ledger's §3 is EMPTY).**
Round record: `status-log.md` → "Round record — the `6d2a50382` drift
catch-up round unification". The §3 unification review (four parallel
readers + the unifier's own reads of the load-bearing hunks) found NO
blocking findings — the fourth such round — and fixed nine should-fix items
on the unify branch (headline: v4's `limit` is a `parseInt` PREFIX parse
where the new chats collection GET used Rust's whole-string parse, pinned by
two route arms; the list leg's leaked error where v4 answers the fixed
`Failed to fetch chats`; v4's dropped "still opens the chat when the mark
itself is clicked" case transcribed; the tier-1 background-mode family's
retired-list row gaining a shape guard; the SPA `modeLabels` typed over the
contract union as v4's is). Gate: the 33 affected families regenerated fresh at the new baseline through the sweep driver, 33/33 zero SKIP; 484 test binaries / 2,694 / 0 (zero SKIP) with the round's env block; clippy both feature sets; release build; ng 376 files / 5,883; full Playwright 268 passed / 0 failed / 1 skipped (the standing store-probe park).

**Next candidates, in rough value order** (updated at the `6d2a50382` round
unification, 2026-09-02):

1. **The owed dogfood pass over this round's surfaces** on the Friday copy:
   the bug-114 collapse on the REAL 607-row population (measure FIRST — the
   💸 proof expires when v4 runs its own migration there; the lane measured
   it intact on 2026-09-02), the Concierge mark on every list with the four
   tones + the drawn bubble, "Dangerous Chats" hiding the uncensored row
   and no longer hiding a vouched chat, the footer affordance keyed on the
   probe, the per-turn enqueue guard on a real Uncensored chat (the
   "six times in four minutes" symptom gone), the absent-participant gate on
   a real story background + the reworded 400, a real `'project'`/`'static'`
   project reading `theme`, the Move-to-Project picker over real folders +
   a create through it; plus the standing queue (the `[CheapLLM]` warn
   ordering, Pascal's group tier, the Brahma deep query, dedup/summaries,
   #101, the LoRA wire-byte look).
2. **The eight private participant-status parsers** (six services + two
   tools) consolidating onto `chat_predicates::participant_status_from_str`
   — behaviour-neutral, spans files no lane owned (P4.D146's follow-up).
3. **The `?action=` (present-but-empty) and duplicate-query-param classes**
   at every v5 REST edge — v4's `searchParams.get` returns `''` (falsy →
   the no-action leg) and the FIRST duplicate; axum answers `Some("")` and
   the LAST. Repo-wide idiom, wants a cross-cutting order (P4.D143's
   review).
4. **The remaining LoRA follow-ups + the earlier small items** carried
   verbatim from the P4.D138 follow-up's list (its items 2–6), plus:
   `projects_routes_equivalence` has no `latest_chat` background GET arm;
   the GET's own normalize is a non-discriminating line; the picker's
   `#move-folder` id is single-instance by contract; the `console.warn` on
   the quick-hide probe has no v4 counterpart (v4 is silent).

PB1 stays parked by the standing rule. The `qtap-export.schema.json` file
port remains a NAMED standalone flag.

**The ordering-time section follows for history:**

Five lanes, all drift (the standing rule: drift debt before new scope).
The ledger's six rows are ORDERED; the four PORT commits map to five
orders and the two NO-PORT? rows ride P4.D143's lane record for
ratification. Orders under `work-orders/`:

- **P4.D143** `p4.d143-concierge-list-marks-server.md` — v4 `c43d3b1b4`
  server half: the derived `conciergeState` + `dangerCategories` pair on
  all four chat-list payloads in place of the raw label (key order
  preserved), `concierge_state_uses_uncensored_route` with
  `should_use_uncensored_route` delegating, the per-turn
  `CHAT_DANGER_CLASSIFICATION` enqueue gated on `is_classifier_on_duty`
  (the "six times in four minutes" the 2026-08-27 pass saw), and the
  `has-dangerous` probe v5 never had (§H, Tier 2). Ratifies `f3351d54f`
  + `6d2a50382`.
- **P4.D144** `p4.d144-concierge-list-marks-spa.md` — the SPA half: the
  presentation table as the ONE string home (v4's module has no server
  consumer — §B), `ConciergeMark` through the P4.D132 Tooltip, the pill
  and sidebar reads onto the table, `shouldHideChat` as the one
  quick-hide rule (its recorded non-port ruling retired), the four
  filters, the `.qt-concierge-mark` CSS, the §A DTOs; beats gated.
- **P4.D145** `p4.d145-folders-unique-path-bug114.md` — v4 `a5df98b3f`
  (bug 114): the ledger's "D23 re-dump" premise REFUTED at ordering
  (generateDDL cannot emit an expression index); the unique index arrives
  through a collapse-then-index boot ensure in the
  `mount_index_case_repair` idiom (index-presence guard, NO ledger row —
  v4's `shouldRun()` is `!indexExists()`), `ensure_by_path` over seven v5
  create sites + two private `find_folder_by_path` copies deleted, the
  net-new rusqlite unique-constraint predicate, the restore quiet-drop
  arm; the watcher site NO-COUNTERPART. Open with the Friday-copy
  measurement (607 rows / 24 folders at v4's count — expires, §5.5).
- **P4.D146** `p4.d146-absent-participants-story-background.md` — v4
  `70505745a`: `is_participant_present` at the three story-background
  sites (the enqueue twin is `image_profile_resolution.rs`, not the
  title job), the reworded 400, the `backgroundDisplayMode` narrowing +
  normalizer at every read/validate/write, the dead GET arms, the SPA
  card's retired options; the committed story fixture is structurally
  blind (every participant `active`) and gets widened.
- **P4.D147** `p4.d147-move-to-project-folder-picker.md` — v4
  `a00e18f0d` (bug 113): measured at ordering — v5 has NO folder picker
  (the P4.6af tier-3 text field), so the lane builds v4's post-fix
  `FolderPicker` over the existing verbs; owns Playwright (the round's
  only lane whose beat needs no sibling wire).

Shared contracts §A–§H (identical in all five): the list-payload wire,
the SPA-only presentation table + the shared predicate name, the
`story_background_job.rs` / `api/projects.rs` / restore-orchestrator
region splits (with the `ABSENT_PARTICIPANTS_PENDING_P4D146` and
`BACKGROUND_MODE_PENDING_P4D146` tripwires for the sibling-pin drift),
`core-contract.ts` regions, the folders route wire, and the
`chatsHasDangerous` verb. Unifier pick order: D145 → D146 → D143 → D144 →
D147.

Deliberately left out of the round: phase-4 candidates 2–6 from the
P4.D138 follow-up (the LoRA log-only follow-ups, message-bubble danger
styling, the `precompute_equivalence` blindness, the gateway resolver,
the small follow-ups) — drift debt first; the owed 💸 items (the
`[CheapLLM]` warn ordering, Pascal's group tier, the Brahma deep query,
dedup/summaries, #101, the LoRA wire-byte look) stay on the dogfood
queue; finding #109 is a v4-first filing.

## The P4.D138 follow-up (units 5–7) — UNIFIED 2026-09-01

The resumed LoRA-train lane closed P4.D138 WHOLE the same day the round-2
unification left it OPEN: bugs 110/111, the `list-models` `loraSupport` map +
`options-schema` + the NanoGPT catalog cache, the HuggingFace lookup +
`lora-metadata`. The drift ledger's §3 is EMPTY; the baseline stays
`4622411fd`. Round record: `status-log.md` → "Round record — the P4.D138
follow-up unification".

**Next candidates, in rough value order** (updated at the P4.D138 follow-up
unification, 2026-09-01):

1. **The owed dogfood pass** — the standing 💸 queue plus the two rounds'
   surfaces: the LoRA editor on a real NanoGPT profile end to end (a
   declaring family's rows, the cap flag, a real Query against HuggingFace
   with and without a token — the one arm no test may exercise — and a
   real generation carrying `lora_url_N`/`lora_scale_N`; v4 records the
   same live proof as outstanding), the bug-112 boot recompute on the
   Friday copy (measure the population FIRST, ledger §5.5), the four-state
   Concierge walk on a real chat, an Uncensored chat taking the uncensored
   route with no danger paint, the themed sliders, the clock-free mid-turn
   bubble; plus the round-1 items carried (understudy, reroute-with-an-
   image, curly-quote resolve, stand-in toasts) and the older queue.
2. **The LoRA train's recorded follow-ups:** the `[Image LoRA]` warnings'
   `{context, chatId, jobId, profileId}` spread + the
   `tools.generate_image.style-options` anchor (log-only, capture pins);
   the prompt-only `validate_image_generation_input` divergence row in
   `image_generation_tier3` (`count: 20` → v4 refuses, v5 generates); the
   `image_gen_leaves` header's shared `/tmp/qt-oracle-stage`; the modal's
   structured writers replacing a mid-edit JSON textarea (now reachable —
   a declaring provider without an `optionsSchema`);
   `ImageModelListing.loraSupport` with no reader on either side.
3. **Message-bubble danger styling was never ported** (the round-2 §C
   measurement) — binds `shouldShowDangerStyling(chat)` when it lands.
4. **`precompute_equivalence` is blind to the uncensored predicate**
   (P4.D141's measurement).
5. The host-side gateway resolver, the empty-response failover legs'
   `llm_logs` rows + chain-walk corpus blind spots,
   `CheapLlmTaskExecutor::new()`'s missing chain (all carried).
6. The round-2 small follow-ups (the temp bubble's seat vs the server's,
   `waitForChatRefetch`'s unscoped match + the injection hook, the
   workspace-search beat's positional pick, the dozen residue hosts + the
   slider suffix bytes, the bare-column guard for the shared SQL filter,
   `help/chats.md` → `p4.9i2`), P4.62's escalations, the
   `system-data-main.db` widening, the component-transfer beat un-park, the
   present-but-null lead, `p4.9i2`, the handler-logging sweep, the SPA-wide
   `title=` sweep (all carried).

PB1 stays parked by the standing rule. The `qtap-export.schema.json` file
port remains a NAMED standalone flag.

**The superseded candidates list from the round-2 unification follows for
history:**

## The round-2 drift catch-up (P4.D138 ∥ P4.D139 ∥ P4.D140 ∥ P4.D141 ∥ P4.D142 ∥ P4.66) — UNIFIED 2026-09-01

Five of six orders CLOSED; **P4.D138 OPEN at units 5–7** (its resume list is
in `status-log.md` → "P4.D138 — lane status: OPEN at units 5–7", extended at
the unify with the §3 review's carried items). The oracle baseline MOVES
`7fb668263` → `4622411fd`; the ledger's three LoRA-train rows stay as
PARTIAL. Round record: `status-log.md` → "Round record — the drift catch-up
round 2 of 2 unification".

**Next candidates, in rough value order** (updated at the round-2 drift
catch-up unification, 2026-09-01):

1. **Finish P4.D138 (units 5–7) — the LoRA train's server remainder:** bug
   110's family-first `apply_loras` + bug 111's error-level request log
   (and v4's unported `Posting NanoGPT image request` DEBUG line — port
   both), the `list-models` `loraSupport` read side + the `options-schema`
   verb + the NanoGPT detailed-catalog cache (which retires the
   `LORA_SUPPORT_PENDING_P4D138_UNIT6` strip and flips
   `P4D138_LORA_SERVER_LANDED` live), and the HuggingFace lookup behind
   `POST ?action=lora-metadata` (the repo's second mocked non-LLM HTTP
   provider). Carried into its resume list by the review: the `[Image
   LoRA]` warnings' dropped `{context, chatId, jobId, profileId}` spread +
   the missing `style-options` anchor (capture pins), the prompt-only
   `validate_image_generation_input` divergence row, `kept[0]`'s guard,
   the shared-stage recipe header. Until it lands the image-profile modal's
   options-schema fetch fails silently into the legacy panel on every open
   (one 400 line per open in the server log).
2. **The owed dogfood pass** — the standing 💸 queue plus this round's
   surfaces: the bug-112 boot recompute on the Friday copy (measure the
   population FIRST, ledger §5.5 — v4 has run daily there since
   `735d9408c`), a Salon list dated by conversation, a restore keeping its
   chats' own dates; the four-state Concierge walk on a real chat (Vouched
   → Uncensored → Monitored with the sentences, the pair read back, the
   header pill), an Uncensored chat taking the uncensored route with no
   danger paint; the LoRA editor on a real NanoGPT profile (blocked on
   candidate 1); the sliders' themed accent + focus ring; the mid-turn
   user bubble on a real multi-character turn (finding #106, now
   clock-free); plus the round-1 items carried (understudy, reroute-with-
   an-image, curly-quote resolve, stand-in toasts) and the older queue.
3. **Message-bubble danger styling was never ported** (found by the §C
   wire measurement): v4's `VirtualizedMessageList → MessageRow →` bubble
   `dangerous` prop has no v5 twin on `qt-message-list`. When it is ported
   it binds `shouldShowDangerStyling(chat)` — never the raw label.
4. **`precompute_equivalence` is blind to the uncensored predicate**
   (P4.D141's recorded measurement): seed an uncensored profile into
   `episodic-recall-*.db` and thread `allProfiles` on both sides so the
   cheap-LLM swap arm discriminates.
5. **The host-side gateway resolver** (P4.D134's follow-up, carried), the
   empty-response failover legs' `llm_logs` rows + chain-walk corpus
   blind spots (round 1, carried), `CheapLlmTaskExecutor::new()`'s missing
   chain (carried).
6. **Small review follow-ups recorded this round:** the temp bubble's seat
   vs the server's resolution (P4.66); `waitForChatRefetch`'s unscoped
   `chatGet` match + the injection hook's silent degradation (P4.66's
   beat); the workspace-search Documents beat's positional `.first()`
   chat pick; the modal's structured writers replacing a mid-edit JSON
   textarea once `loraSupport` renders without a schema (P4.D139); the
   dozen residue component hosts with no display rule + the unpinned
   slider suffix bytes (P4.D142); an executable bare-column guard for the
   shared `CHARACTER_AUTHORED_MESSAGE_FILTER` (P4.D140); `help/chats.md`
   (+22 at `735d9408c`) → the `p4.9i2` bank.
7. P4.62's escalations, the `system-data-main.db` widening, the
   component-transfer beat un-park, the present-but-null lead, `p4.9i2`,
   the handler-logging sweep, the SPA-wide `title=` sweep (all carried).

PB1 stays parked by the standing rule. The `qtap-export.schema.json`
file port remains a NAMED standalone flag (this round recorded two more
no-counterpart hunks against it: the `loras` entry and the widened
`conciergeOverride` enum).

**The superseded candidates list from the round-1 unification follows for
history:**

**Next candidates, in rough value order** (updated at the round-1 drift
catch-up unification, P4.D134 ∥ P4.D135→P4.D136 ∥ P4.D137, 2026-09-01 —
the eight-row drift prefix is CLEARED and the baseline moves to
`7fb668263`; eight commits remain, the pre-planned round 2):

1. **The round-2 drift catch-up** (drift-ledger §3, eight rows): the
   THREE-commit D-stacked LoRA train (`84f33ce94` → `648d5c8aa` →
   `2ece98c90`), bug 112's `lastMessageAt` redefinition (`735d9408c` — it
   lands on the P4.64/P4.65 home + Salon-list surfaces), the Concierge
   four-state (`60e3c4a0a`), `qt-range` (`5f56f7a7d`), and the two
   docs-only ratifications (`e41fcb12e`, `4622411fd`). PIN REQUIRED at
   `7fb668263` until it lands. **ORDERED 2026-09-01** as six lanes:
   `p4.d138-lora-train-server` ∥ `p4.d139-lora-train-spa` ∥
   `p4.d140-chat-activity-bug112` ∥ `p4.d141-concierge-four-state` ∥
   `p4.d142-qt-range-inline-host` ∥ `p4.66-optimistic-bubble-reconcile`
   (the last two fold in dogfood findings #107 and #106; `4622411fd`
   ratifies at the round's `/unify`).
2. **The owed dogfood pass** — the standing 💸 queue plus round 1's
   surfaces: a real dead-endpoint understudy answering with correct
   attribution + the exhausted chain's roll + the tier pick crossing
   providers (recipe in the P4.D135 record), the reroute-with-an-image
   walk + the re-measured compression row (P4.D136 record — the old 75 s
   C4 numbers are SUPERSEDED), the live curly-quote doc-edit resolve
   (P4.D137), and the failing-over toast naming each stand-in. Carried:
   the tooltips/badge walk, the restore-key real-pepper walk (human
   only), Pascal's group tier, the Brahma deep-query budget,
   dedup/summaries, the NanoGPT caching smoke / #101.
3. **The host-side gateway resolver** (P4.D134's named follow-up): v5
   has never rewritten a localhost URL in production
   (`with_localhost_gateway` has zero call sites outside core —
   measured); v4's post-`1560bd43b` resolver is only two strategies, but
   porting it ADDS wire behavior (a provider's base URL changes inside
   Docker) and v4's module-global answer cache is its own fidelity
   question. Named in `rewrite.rs`'s header.
4. **The empty-response failover legs' missing `llm_logs` rows** (the
   round-1 §3 review's recorded follow-up): the orchestrator's recovery
   call takes the no-log entry where v4's `restreamInto` logs every leg;
   the hard-error site already threads the log. Fix shape: thread
   `FailoverLogCtx` at `orchestrator.rs`'s recovery call
   (`provider_failover.rs:158-165` records it in-code). Plus the three
   chain-walk corpus blind spots named in the round record (mid-chain
   empty, the no-key auth-reason bytes, fail-then-recover).
5. **P4.62's escalations** (carried; ordered shapes in its lane record):
   the wrong-typed `tagId` carry, the jobs-enqueue `payload: []`, the
   `chat_media::chat_file_link` guard simplification.
6. **Widen the committed `system-data-main.db` past
   `multiCharacterPrefill`** (carried — and round 1 measured the two NEW
   fallback columns landing in the same hole; the understudy remap is
   unit-pinned meanwhile).
7. **Un-park the component-transfer beat** (carried, P4.D130's scope).
8. **`CheapLlmTaskExecutor::new()`'s missing chain** (P4.D135's named
   gap): the two bare-constructor production sites
   (`tools::generate_image`'s prompt expansion, one `enclave::step` leg)
   have no fallback chain — closing it means giving those two a `Db`.
9. The present-but-null validation lead (carried); `p4.9i2` help/HelpChat
   (the bank gained 7 more rows this round); the handler-logging sweep
   (carried — round 1 added `resolve_provider_for_dangerous_content`'s
   per-arm lines to its list); the SPA-wide `title=` sweep (carried,
   keyed to v4).

PB1 stays parked by the standing rule. The `qtap-export.schema.json`
file port remains a NAMED standalone flag (round 1 recorded another
no-counterpart hunk against it).

**The superseded candidates list from the P4.D131 unification follows for
history:**

1. **The owed dogfood pass** — the standing 💸 queue plus this round's
   surfaces: the tooltips + pinnable confirmation badge on real turns,
   the Salon list's ~5.7× on the Friday copy (enrich 12,984 → 2,227 ms),
   and **the `instances restore-key` real-pepper recovery walk** (human
   only — the pepper never goes to an agent: run against a COPY of
   Friday, prove the rebuilt `.dbkey` opens all three partitions).
   Carried from earlier rounds: Pascal's group tier, the Brahma
   deep-query budget, dedup/summaries, the NanoGPT caching smoke / #101.
2. **P4.62's escalations, ordered shapes in its lane record:** the
   wrong-typed `tagId` carry, the jobs-enqueue `payload: []`, and the
   `chat_media::chat_file_link` guard simplification.
3. **Widen the committed `system-data-main.db` past
   `multiCharacterPrefill`** (the standing vintage vacuity — every
   connection-profile import in the family fails identically on both
   sides; cross-lane with the system-data routes differentials).
4. **Un-park the component-transfer beat** (P4.D130's precise scope:
   materialize `projects`/`groups` in `wardrobe-flow.spec.ts`, fix the
   Copy arm's vacuous count-0 assert).
5. **Small named follow-ups from this round's §3 review** (batchable
   into any adjacent lane): Ctrl-C during a CLI prompt skips the lock
   Drop (v4 releases via exit handlers; self-heals via stale-lock
   reclaim — `resolve.rs:222` / `restore_key.rs`);
   `characters_read::find_by_ids` is the one un-chunked batch on the
   list path (real ceiling 32,766); the `workspace-search-documents`
   focused-tab intermittent (hidden ⇒ wrong arm ⇒ the
   `OpenDocumentFromSearch` focused-tab read raced — one suite-context
   red, green in isolation ×3, recorded in the P4.D132 lane record).
6. The present-but-null validation lead (carried).
7. `p4.9i2` — help/HelpChat as a dedicated round (the bank gained
   `system-import-export.md`, `answer-confirmation.md`,
   `chat-message-actions.md`, `database-protection.md` this round-pair).
8. The handler-logging sweep (carried).
9. The SPA-wide `title=`-as-tooltip sweep — **keyed to v4 adopting
   Tooltip beyond the action bar**, not unilateral v5 restyling (191
   template files carry `title=`; the P4.D132 emit JSON already holds
   the action-bar rows).

PB1 stays parked by the standing rule. The `qtap-export.schema.json`
file port remains a NAMED standalone flag.

**The superseded candidates list from the P4.D130 unification follows for
history:**

1. **The `679e450e3` + `0bd841394` drift catch-up** (drift-ledger §3, both
   UNPROCESSED). `679e450e3` is the bug-105 CONVERGENCE — v4 adopting this
   port's own filing; at the baseline move past it,
   `system_import_state`'s `execute_bug105_seed_abort` trips BY DESIGN:
   regenerate, measure v4's post-fix output, and retire the classifier +
   the `skip` insert + the blanked body to a plain equality (ledger §5.4),
   updating `profiles.rs`'s unit-pin doc alongside. `0bd841394` is
   PORT-NEW: `components/ui/Tooltip.tsx` (body-portalled, 200 ms
   dwell/focus-immediate, flip+clamp, pinnable/interactive) adopted by the
   message action bar's eleven buttons (each gaining an explicit
   aria-label) and the answer-confirmation badge (now a real pinnable
   button with structured content) + style/storybook riders. ⚠ The v4
   checkout is DIRTY continuing the same salon surface — expect a
   follow-on commit; probe before planning and pin every regen.
2. **The Salon chat-list `ChatListPreloaded` batching** (P4.64's
   measurement is the justification): the Salon list pays the same
   8.6–12.2 s enrichment the dashboard used to and genuinely needs every
   row enriched (`_allTagIds` feeds `filter_chats_by_excluded_tags`);
   v5's port dropped v4's up-front batch reads entirely. Needs the four
   missing batched read paths — `files::find_by_ids`,
   `projects::find_by_ids`, `doc_mount_file_links::find_by_ids_with_content`,
   `conversation_chunks::count_by_chat_ids` (`characters_read::find_by_ids`
   and `memories_read::count_by_chat_ids` exist, unused by this service).
   Payload-identity discipline per the P4.64 order.
3. **The owed dogfood pass** — the standing queue plus this round's
   surfaces: the outfit pull-down on real Friday wardrobes (a composite
   pool, dissolution, the garments-only pickers), the home dashboard's
   0.39 s on the Friday copy (was ~9 s), and the previous round's items
   (bug-103 seeding, glm-5.3 wire, the 75 s compression fold, About
   strings, three-shell completion, hover fills; carried: Pascal's group
   tier, the Brahma deep-query budget, dedup/summaries, the NanoGPT
   caching smoke / #101).
4. **P4.62's escalations, ordered shapes in its lane record:** the
   wrong-typed `tagId` carry (widen `Request::FileUpload.tags` in
   `api/types.rs` + DB-backed arms), the jobs-enqueue `payload: []` (v4
   accepts 201; fix `jobs_enqueue`'s `!payload.is_object()` in
   `api/system_data.rs`, arm in `system_jobs_collection_equivalence`),
   and the `chat_media::chat_file_link` `fileId is required` guard
   simplification.
5. **Widen the committed `system-data-main.db` past
   `multiCharacterPrefill`** (P4.63's surfaced vacuity: every
   connection-profile import in the family fails identically on both
   sides — that import has measured nothing since bug 68). Cross-lane:
   the fixture is shared with the system-data routes differentials.
6. **Un-park the component-transfer beat** (P4.D130's precise scope):
   materialize `projects` + `groups` in `wardrobe-flow.spec.ts`'s
   `beforeAll` (DDL in `fresh_schema.json`; the salon instance's `groups`
   step is the precedent), fix the Copy arm's vacuously-green
   `option[value^="character:"]` count-0 assert, re-drive the move beat.
7. The present-but-null validation lead (carried).
8. `p4.9i2` — help/HelpChat as a dedicated round (the bank gained
   `help/wardrobe.md`'s Composite Items + chat-start Manual mode).
9. The handler-logging sweep (carried).

PB1 stays parked by the standing rule. The `qtap-export.schema.json`
file port remains a NAMED standalone flag.


### `p4.9i2` bank — `help/file-uploads.md` + `help/chat-settings.md` (P4.D151, v4 `0b0617fee`)

Banked 2026-09-03 by the bug-116 lane. v4's `0b0617fee` touched two help docs.
Both are carried VERBATIM for the `p4.9i2` help-doc port — the house voice is
v4's and must not be re-worded.

**`help/chat-settings.md`** — one NEW bullet appended to the
"These settings govern which model is called upon to do the describing" list,
after the "Should a describer prove sluggish…" bullet:

> - **The describer's word is checked before it is believed.** A gateway that fronts hundreds of models — NanoGPT, OpenRouter and their kind — may accept your picture with every appearance of politeness and route it to a model that quietly disregards it. The model, asked to describe an image it was never shown, will describe *an* image: fluently, at length, in tidy sections, and entirely out of its own head. Quilltap now examines the bill. A consultation charged for the instruction alone did not look at your picture, whatever prose came back, and the answer is discarded unread rather than filed. So too when the provider itself reports the attachment as never sent. In either case the failure names the offending profile and the fallbacks take their turn as they would after any other refusal.

…and one **Prerequisites** bullet REPLACED (the old text is
"The describing model must genuinely accept images; ticking the box on a model
that cannot see produces empty answers rather than descriptions"):

> - The describing model must genuinely accept images. Ticking the box on a model that cannot see is now caught rather than believed — the consultation fails by name and passes to the fallbacks — but it still costs you a wasted call, so tick it only where it is true

**`help/file-uploads.md`** — one NEW paragraph after the message-attachment
bullet list, before the "### Character Profiles" heading:

> **A word on pictures.** An image attached to a chat is quietly shown to a
> describing model shortly after it lands (see [Chat Settings](chat-settings.md)),
> and the description is filed in three places: on the file's own record, on
> every shelf in the [Scriptorium](scriptorium.md) where those same bytes appear,
> and in the search index — so *"the photograph of the kettle on the windowsill"*
> finds the picture later even though a picture holds no words. Until 4.9.0 only
> the first of the three was reached for any bitmap the house converted to WebP
> on its way in, which was rather a lot of them; converted uploads made before
> that release are repaired on first start.

⚠ That second paragraph describes **bug 117**, not 116 — its last two sentences
are only true once P4.D152's `sha256` realignment + boot heal have landed. If
`p4.9i2` runs before P4.D152 closes, bank the paragraph but do not publish it.
The rest of the `p4.9i2` bank is unchanged.
### `p4.9i2` bank — `help/chats.md` + `help/dangerous-content.md` (P4.D149, v4 `303288fb4`)

Banked 2026-09-03 by the Concierge-at-creation SPA lane. v5 renders no help
surface yet (the whole `p4.9i2` pool), so the two hunks below are carried here
VERBATIM from the `303288fb4` pin for whichever round ports the help docs. The
house voice is v4's and must not be re-worded.

**1. `help/chats.md` — a NEW `###` section, inserted after the roleplay-template
section's "(The dropdown keeps its counsel…)" line and immediately before
`## The Chat Interface`:**

<!-- BANKED VERBATIM FROM v4 303288fb4 — do not re-word -->

### A Word With the Concierge, Before the Doors Open

Some conversations announce their character before the first syllable is spoken, and it has always been a small indignity to have to start such a chat in the ordinary way, wait for the room to be dressed, open the sidebar, and only *then* inform the Concierge of what everybody already knew — by which time the opening line had gone out through the ordinary desk and, on occasion, come back refused.

The new-chat form therefore carries **The Concierge** directly above **Starting Scenario**, offering the same four postures as the chat's own sidebar, in the same two companies:

- **The Concierge decides** — *Monitored* (the default, and the state of every chat that has ever been created without a word on the subject) and *Flagged*.
- **You decide** — *Vouched Safe* and *Uncensored*.

Beneath the dropdown, the Concierge states plainly what the posture you have selected commits him to. Choose one other than Monitored and he posts a brief note at the top of the new conversation saying so, immediately after the system prompt and before the scene is set — the history is thereby honest about which arrangement was in force from the very first word. The opening greeting is then composed under that arrangement: a chat opened *Uncensored* goes to the frank desk on the first attempt rather than after a refusal, and a chat opened *Vouched Safe* is never rerouted at all.

Two consequences worth knowing before you choose. A chat created *Uncensored* or *Flagged* wears its mark in every list from its first appearance, and vanishes the moment you pull the **Quick-hide** cord with *Dangerous Chats* selected — which is generally the point. And when you take a conversation elsewhere by way of **Continue Elsewhere**, the new venue inherits the old one's posture, so a spirited conversation does not quietly become a decorous one on changing rooms; the dropdown is right there should you wish otherwise.

None of this is a life sentence. The **The Concierge** control in the chat's own sidebar remains exactly where it was, ready to reconsider the matter at any hour.

**2. `help/dangerous-content.md` — under `## The Per-Chat Concierge Switch`, the
opening paragraph is REPLACED (v4 softens "It is the only place…" to "It is
where…", now that it is no longer the only place) and a second paragraph is
added after it, before "Two questions, taken together…":**

<!-- BANKED VERBATIM FROM v4 303288fb4 — do not re-word -->

Every chat keeps a small brass switch in the sidebar — found under the **Chat** section of the Chat Sidebar — bearing four positions arranged under two headings: **The Concierge decides** (Monitored, Flagged) and **You decide** (Vouched Safe, Uncensored). It is where a chat's relationship with the Concierge is adjusted, reconsidered, or — should the operator so insist — dispensed with entirely.

The same four positions, in the same two companies, are also offered on the **new-chat form**, above **Starting Scenario**, for the conversations whose character is not in doubt before they begin. A posture chosen there is in force from the very first word: the Concierge posts his note at the top of the fresh history, and the opening greeting is composed under the arrangement rather than discovering it after a refusal. See [Chats Overview](chats.md) for the particulars. Everything below applies identically whichever of the two controls you reached for.

Nothing else in either file moved at `303288fb4`. The rest of the `p4.9i2` bank
is unchanged.
