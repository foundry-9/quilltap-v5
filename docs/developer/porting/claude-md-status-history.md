# CLAUDE.md Status history (archive)

Round bullets and superseded oracle-baseline paragraphs moved VERBATIM out of
CLAUDE.md's "Status" section on 2026-08-13 (it had grown past the per-turn
size limit; precedent: the 2026-07-10 move of the unit-by-unit journal to
`status-log.md`). Newest-first authority for any round remains
`docs/developer/porting/status-log.md` — this file only preserves the exact
phase-level summary text CLAUDE.md used to carry.

Contents:
1. Round bullets, the P4.6f/g/h round (2026-07-10) through the `5cc76688`
   drift catch-up round (2026-07-30). Later rounds remain in CLAUDE.md.
2. The superseded oracle-baseline paragraphs ("the previous baseline
   paragraph follows for history" chain). The CURRENT baseline paragraph
   remains in CLAUDE.md.

## 1. Archived round bullets (2026-07-10 → 2026-07-30)

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
- **The chat-action-remainder round (P4.9E1A ∥ P4.9E3A ∥ P4.9E1B ∥ P4.d22):
  UNIFIED on main (2026-07-26) — ALL FOUR CLOSED; the oracle baseline MOVES to
  `c1507f47` and the drift debt is CLEARED.** The hole the survey found is shut:
  **v5 can change a conversation's cast**, which it had never been able to do —
  the four participant verbs + the avatar-override family + the chat-PUT bag's
  three participant families, two entrances sharing ONE implementation
  (`chat_cast_routes_equivalence`, 72 cases over the new committed
  `chat-cast-{main,mount}.db`, three (`?action=`, PUT-bag) pairs asserted to land
  identical state) ∥ the eleven chat-admin + tools verbs incl. the newly ported
  `apply_chat_merge` (57 + 8 cases over `chat-admin-{main,mount}.db`; two host
  seams LIVE, **⚠ `RegenerateTitleDriver` costs one cheap-LLM call per press**)
  ∥ the SPA (Add Character + Create NPC, participant edit/remove/rebuild with the
  tri-state honoured at every call site, the RNG gutter tool, the
  avatar-generation switch) ∥ the restore/import convergence (all five carve-outs
  retired, eight families regenerated at `c1507f47`). The wire closed both lanes'
  escalations: the explicit-null collapse in `db::chats::ChatParticipant`
  (**E1A's both-directions tripwire FIRED on the first run after the fix, exactly
  as designed**) and the `ChatToggleAgentMode` tri-state the frozen §1 could not
  express, plus the duplicate agent-mode cascade E3A found on E1A's file. **Two
  real bugs surfaced on the cast walk's first live run**, both shipping before
  this round: `qt-collapsible-card` had no host `display` rule, so an inline box
  swallowed every click at the sidebar's Add Character footer once the cast
  overflowed (unclickable since P4.9H1); and the announcement picker dropped
  BOTH of v4's participant filters, so a soft-removed character stayed hidden
  from the off-scene picker forever — invisible until now because no beat had
  ever soft-removed a participant. Gate: 390 binaries / 1,649 / 0; **16
  differentials by name over fresh `c1507f47` oracles, zero SKIP**; clippy both
  feature sets; release build; ng 238 files / 2,974; full Playwright **139
  passed, zero skips**. Versions: core 0.0.380, harness 0.0.326, host 0.0.41,
  web 0.0.47, SPA 0.5.297. **✅ The round's one open item is RULED**
  (2026-07-26, human): v5 keeps its restore files-phase placement and gains a
  skip check — `p4.d23`; v4's `22a-bis` is NOT adopted. Deferred loud: `ChatToolSettingsModal` (needs the unported
  727-LOC `GET /api/v1/tools` inventory — refuses by name), `llm_choose` on both
  the add-participant and merge outfit paths, the `TimestampConfigSchema`
  normalization. Round record: `status-log.md`.
- **The embedding repair + chat-dialog family round (P4.6BL ∥ P4.9E3B ∥
  P4.9E3C ∥ P4.D24): UNIFIED on main (2026-07-27) — ALL FOUR CLOSED; the
  oracle baseline MOVES to `e8a49597` and the drift debt is CLEARED.** The
  EMBEDDING_GENERATE worker is **LIVE in the production spine** — dogfood
  finding #35's 2,088 dead jobs and every unembedded v5-written chunk were the
  steady state until now; the handler ports all four entity types with
  `isPermanentEmbeddingError` and the oversize/empty guards (omitting the
  classifier would have inherited v4's own tens-of-thousands-of-dead-rows
  incident on day one), and the backlog heals on boot (the startup-reconcile
  port stays DEFERRED LOUDLY on the unported CONVERSATION_RENDER handler) ∥ the
  chat-dialog server remainder: the 727-LOC tools inventory, chat export, the
  search-replace pair over both scopes, per-message reattribution, the
  outfit-summary read, the two `llm_choose` refusals closed by a host driver
  (**⚠ real spend per pick**), and the twice-deferred `TimestampConfigSchema`
  write normalization ∥ eleven dialog surfaces + the agent-mode badge + the Edit
  Content section, which closes the last "server is live, no UI can reach it"
  gaps in the chat surface ∥ the `e8a49597` operator-perspective mirror. **Two
  findings worth carrying:** `AllLLMPauseModal` is UNREACHABLE IN v4 ITSELF, so
  it is deferred with the evidence rather than shipped as a dialog nothing can
  open (a v4-side item); and P4.D24's tripwire ran **green** at the new baseline
  because its corpus seated the operator's character first in stored order —
  the old and new choices agreed on every row, so the fixture had to move, not
  the port (the P4.11 one-mode-corpus shape again; 13 → 20 cases, the
  red→green fingerprint produced by mutation). Wires: the §1 contract diffed
  name-for-name (E3B's audit-added `ChatGroupStores` mirrored into the SPA —
  **no client consumer yet**, its caller `LibraryFilePickerModal` being deferred
  by name); all four ACTIVATE-AT-UNIFY beats flipped LIVE; D24's two `apps/web`
  riders taken. Gate: 396 test binaries / 1,663 / 0 failed, the round's 12
  differentials by name over fresh `e8a49597` oracles **zero SKIP**, clippy both
  feature sets, release build, ng test 248 files / 3,070, ng build clean, full
  Playwright **151/151 zero skips** (incl. the P4.d23-owed restore beat, re-run
  unmodified). Versions: core 0.0.388, harness 0.0.335, host 0.0.43, web 0.0.50,
  SPA 0.5.311. Deferred loud: `LibraryFilePickerModal` (its own round),
  `attach-mount-file`, the tools-inventory plugin arm, EMBEDDING_REINDEX_ALL,
  `chatQueueMemories`. **The embedding worker's live proof on real data is
  owed** — the next dogfood walk's. A v4 bug for the human to carry upstream:
  **stop-impersonate is unreachable from v4's own client** (client sends DELETE;
  the action is registered only on POST) — v5 already models it correctly.
  Round record: `status-log.md`.
- **The library picker + embedding remainder round (P4.9E4A ∥ P4.9E4B ∥
  P4.6BM): UNIFIED on main (2026-07-27) — ALL THREE CLOSED; the previous
  round's five loud deferrals all close with it.** The composer attaches
  document-store files end to end (`chatAttachMountFile` + the
  `ImageDescribeDriver` seam LIVE in the spine — **⚠ one vision-LLM call per
  attach of a genuinely unknown image**; kept-image markdown and cached blob
  descriptions cost nothing; the `GET files` mount-file read-back the order
  wrongly said existed was ported too) ∥ the LibraryFilePickerModal + gutter
  entry (all six gutter tools present) + the project Default Tool Settings
  dialog + the RNG residuals + the `allowToolUse` disposition (**dead code in
  v4 itself** — recorded v4-side, not ported) ∥ the embedding family COMPLETE:
  CONVERSATION_RENDER (the pure 224-LOC renderer tier-1 byte-exact, 11 cases)
  + the startup reconcile (the P4.6BL boot stand-in RETIRED) +
  EMBEDDING_REINDEX_ALL (both had been minting DEAD jobs from live callers) +
  `chatQueueMemories` un-refused. **The unify skill's new §3 code review ran
  and BLOCKED once, as designed:** the ACTIVATE-AT-UNIFY store-attach beat had
  seeded a native-text file — v4's document branch writes no blob, so attach
  404s in BOTH apps (now on the v4-side list); fixed as the spec's gesture
  (binary seed, chip-count assertion). Review also fixed the reindex phase-1
  wipe gate (`!partial` → `scope == "all"`) and recorded the describe
  auto-pick coverage deferral in E4A's header. Gate: fmt/clippy both feature
  sets/release build clean; `cargo test --workspace` **1,670 / 0** with the
  round's env vars; the round's four families FRESH at `e8a49597` + the four
  neutrality families re-run BY NAME zero SKIP; ng test 252 files / 3,120;
  ng build clean; full Playwright **155/155 zero skips** (the store-attach beat
  LIVE). Deferred loud: embedding-profiles management routes (`p4.9h`),
  character-rename `fullReembed` (rename service unported),
  `triggerSceneStateTracking` (no handler). Versions: core 0.0.396, harness
  0.0.342, host 0.0.48, web 0.0.51, SPA 0.5.319. **The dogfood pass is now
  the top next item** — it owes the embedding worker's live proof, the
  chat-dialog family, the picker/attach flow (real describe spend), and walk
  Parts D/F/H — see phase-4.md. Round record: `status-log.md`.
- **The P4.D25 `083fdf68` embedding-warmth drift catch-up: CLOSED, UNIFIED on
  main (2026-07-28, single lane) — the oracle baseline MOVES to `083fdf68` and
  the drift debt is CLEARED.** v4's fixes for its own Bugs 6 + 7, mirrored into
  the already-ported embedding/maintenance family — a real-money bug v5
  reproduced line-for-line until now: the boot reconcile read the cache
  collapse's deliberate cold-tiering as damage and re-embedded the whole cold
  tier on every boot (~$2/restart on the measured Friday instance), and the
  next sweep cleared it again. Landed: the `mark_as_embedded`/`mark_as_failed`
  UPSERTS + required `user_id` threaded through all four consolidated mark
  sites (v4's thirteen); the `clear_embeddings_for_chat` `older_than` age
  guard bound to the cache sweep's cutoff (reopen warmth survives a full
  retention window); the boot reconcile's staleness gate (shared `is_stale`,
  both fail-soft arms — unknown staleness SKIPS, never heals) +
  FAILED-profile-exclusion SQL + `skipped_stale`. The order's proposed `&Db`
  re-signature was correctly REJECTED by the lane (the reconcile runs inside
  `write_blocking`; a nested write would deadlock) — `is_stale_conn` /
  `resolve_stale_chat_days_conn` twins instead. Every fixture the drift made
  structurally blind was extended (the remainder corpus had gone ALL-stale
  the moment the gate landed; four in-window chats restored its arms) and
  every first-run-green family was mutation-proven, per the D24 rule. Gate:
  fmt, clippy both feature sets, release build, **399 test binaries / 1,673
  tests / 0 failed** with all nine env vars, the seven families re-run by
  name zero SKIP over oracles regenerated FRESH at `083fdf68` (each NDJSON
  grepped for a new-baseline marker). No `apps/web` change — no SPA gate
  owed. The §3 review found no blocking issues. Versions: core 0.0.399,
  harness 0.0.345, host 0.0.49; web 0.0.51, cli 0.0.3, quilltap-tauri 0.0.5,
  SPA 0.5.319 unchanged. **The dogfood pass remains the top next item** (it
  now also owes this round's live proof: a boot against the Friday copy that
  does NOT mass re-embed). Round record: `status-log.md`.
- **The `5cc76688` drift catch-up round (P4.d26 ∥ P4.d27 ∥ P4.d28): UNIFIED
  on main (2026-07-30) — ALL THREE CLOSED; the oracle baseline MOVES to
  `5cc76688` and the drift debt is CLEARED** (the fourth drift commit is the
  NO-PORT jobs-child proxy fix, dispositioned at planning). Same-day recall
  + the fresh-event boost end-to-end (the new pure `day_references` resolver
  with a TWO-zone-leg tier-1 family — under TZ=UTC alone this bug class is
  invisible; the distill merge + local TODAY line with a REQUIRED Chicago
  oracle leg; `fresh_event_multiplier` ×1.6/×1.35 + the echo guard; the
  `occurredWithin` ungating at all three sites; the `server_tz` seam — the
  host's IANA zone, NOT the story `timezone` — threaded from every
  production entrance) ∥ one enforced embedding standard (the boot dimension
  reconcile as v4 Phase 3.7, `_conn` twins + direct-connection deduped
  enqueue; the reindex handler's mount-chunk phase 4 + memories-table
  fan-out + stale/FAILED skips; the housekeeping merge-pass skip; **v4's
  mount-chunk count found DEAD — wrong-database `tableExists` — reproduced
  faithfully behind a TRIPWIRE**, the one-line v4-side fix queued post-5.0;
  the PUT trigger matrix banked whole for `p4.9h` with its unported
  `EMBEDDING_REAPPLY_PROFILE` dependency) ∥ Export Markdown (the pure
  transcript renderer, 53-row byte differential; `calculate_timestamp_at`
  extracted with a 1969 clock sentinel; the verb + web edge with RFC 5987 +
  `no-store`; the Organize button + live beat; the content-disposition lift
  fixing a real astral-char header bug). **The §3 review fixed a
  PRE-EXISTING user-visible bug:** the host `local_offset_minutes` carried
  jiff's east-positive sign where core consumes JS west-positive — a
  no-timezone chat on any non-UTC host rendered at the MIRRORED offset;
  fixed as one shared convention-pinned fn. The gate caught its own trap
  too: P4.d26 made the four distill-transitive tier-3 oracles TZ-sensitive
  and their regen recipes didn't pin TZ=UTC — now they do. Gate: 401 test
  binaries / 1,705 / 0 with the round's 25 families by name zero SKIP over
  oracles regenerated fresh from a pinned `5cc76688` worktree; clippy both
  feature sets; release build; ng 252 files / 3,121; full Playwright
  **156/156 zero skips**. The round's live proofs join the owed dogfood
  pass. (Its "standing loud" enclave_step_tier3 red was CLOSED by P4.20 the
  next round: the red was a stale ORACLE mock, not a v5 divergence, and no
  production money was ever being spent.) Versions: core 0.0.411,
  harness 0.0.356, host 0.0.51, web 0.0.54, cli 0.0.3, quilltap-tauri
  0.0.5, SPA 0.5.320.

## 2. Superseded oracle-baseline paragraphs

- **Oracle baseline: `de9f70bf` (2026-08-11, v4 4.8.0-dev), adopted at
  the P4.D65-finish + sweep-rot unification — NO v4 drift debt remains.**
  `de9f70bf` is v4's Bug-57 fix (the `carriedBlobIds` first-occurrence
  dedupe this port shipped first; convergence, not new behavior). v4 HEAD
  == baseline, tree clean at the gate. Oracles regenerate straight from
  `~/source/quilltap-server`; pin a detached worktree on any further
  drift (`oracle-regen-pinned-v4-worktree`, or `recipe_sweep.py --v4
  <pin>`). The sweep driver (`harness/tools/recipe_sweep.py --run` /
  `--run-all --families`) is the sanctioned per-family regen path after
  P4.45. The distill-transitive TZ pins, the committed-fixture rule, and
  the venue/staging rules stand unchanged. Drift-check before every
  round — v4 ships daily.
  The previous baseline paragraph follows for history:
- **Oracle baseline: `ed8934f1` (2026-08-10, v4 4.8.0-dev), adopted at
  the character-archive round-2 unification — NO v4 drift debt
  remains.** v4 HEAD is `0472cf6c`, ONE commit past the baseline:
  docs-only (the Bug-57 filing — the rehydrate duplicate-blob-claim bug
  this port found and diverged on; zero `lib/`/`app/` change) — NO-PORT.
  When v4 FIXES Bug 57, that lands on the ported preflight and is a
  small drift round that retires v5's divergence pin to a plain
  equality. Oracles regenerate straight from
  `~/source/quilltap-server`; pin a detached worktree on any further
  drift (`oracle-regen-pinned-v4-worktree`, or `recipe_sweep.py --v4
  <pin>`). The distill-transitive TZ pins, the committed-fixture rule,
  and the venue/staging rules stand unchanged. Drift-check before every
  round — v4 ships daily.
  The previous baseline paragraph follows for history:
- **Oracle baseline: `d553f72a` (2026-08-10, v4 4.8.0-dev), adopted at
  the character-archive round-1 unification.** ⚠ v4 HEAD is `ed8934f1`,
  ONE commit past it — "feat(docker): pass filesystem document stores
  through to the container (bug 56)", NOT lib-free: the new
  `lib/mount-index/base-path-availability.ts` + `scanner.ts` + the two
  mount-points routes land on the PORTED Scriptorium surface (the rest is
  Docker/CLI packaging + two help docs → the `p4.9i2` bank). **A drift
  catch-up is OWED (phase-4.md candidate 1); until it runs, pin a
  detached worktree at `d553f72a` for any MOUNT-POINTS-family regen**
  (`oracle-regen-pinned-v4-worktree`, or `recipe_sweep.py --v4
  d553f72a`); other families regenerate straight from the checkout while
  HEAD stays `ed8934f1`. The distill-transitive TZ pins, the
  committed-fixture rule, and the venue/staging rules stand unchanged.
  Drift-check before every round — v4 ships daily.
  The previous baseline paragraph follows for history:
- **Oracle baseline: `f6eac168` (2026-08-08, v4 4.8.0-dev), adopted at
  the P4.D60∥P4.D61∥P4.44 unification — NO v4 drift debt remains.** The
  two commits past `1bed814f`: `f521fc0c` (Bugs 48/49 filing, docs-only,
  NO-PORT) and `f6eac168` itself (Bugs 47–51, absorbed by this round).
  v4's tree was CLEAN at `f6eac168` at the round's regens. Oracles
  regenerate straight from `~/source/quilltap-server` while HEAD stays
  `f6eac168`; pin a detached worktree on any further drift
  (`oracle-regen-pinned-v4-worktree`, or `recipe_sweep.py --v4 <pin>`).
  The distill-transitive TZ pins, the committed-fixture rule, and the
  venue/staging rules stand unchanged. Drift-check before every round —
  v4 ships daily.
  The previous baseline paragraph follows for history:
- **Oracle baseline: `1bed814f` (2026-08-07, v4 4.8.0-dev), adopted at
  the P4.D57∥D58∥D59 unification — NO v4 drift debt remains.** v4's
  tree was CLEAN at `1bed814f` at the round's regens (the P4.D56-era
  dirty `AboutView.tsx` pair landed as `ddd7576b`, part of this
  round). **v4 HEAD is `2a17b3c4`, ONE commit past the baseline:
  docs-only — NO-PORT** (it renamed `docs/developer/found-bugs.md` →
  `docs/developer/bugs.md` and split the catalogue one-file-per-bug
  under `docs/developer/bugs/`; zero `lib/`/`app/`/dependency change).
  The v4 bug catalogue now lives at `bugs.md` (index) + `bugs/bug-<n>-
  <title>.md` per open bug + `bugs/fixed/` (see the `v4-bugs-doc-
  location` memory). Oracles regenerate straight from
  `~/source/quilltap-server` while HEAD stays `2a17b3c4`; pin a
  detached worktree on any further drift
  (`oracle-regen-pinned-v4-worktree`, or `recipe_sweep.py --v4
  <pin>`). The distill-transitive TZ pins, the committed-fixture rule,
  and the venue/staging rules stand unchanged. Drift-check before
  every round — v4 ships daily.
  The previous baseline paragraph follows for history:
- **Oracle baseline: `62c63dc3` (2026-08-07, v4 4.8.0-dev.178),
  adopted at the P4.D56 unification — NO v4 drift debt remains.**
  The two commits between `f4955e0e` and it stayed NO-PORT as
  dispositioned (`cc0bbebf` test-only, `3fa36825` docs-only).
  Oracles regenerate straight from `~/source/quilltap-server` while
  HEAD stays `62c63dc3`; pin a detached worktree on any further
  drift (`oracle-regen-pinned-v4-worktree`, or `recipe_sweep.py
  --v4 <pin>`). At the round's regen v4's tree carried two dirty
  files (`app/about/AboutView.tsx` + an image), verified outside
  every oracle import graph (justification in the round record).
  The distill-transitive TZ pins, the committed-fixture rule, and
  the venue/staging rules stand unchanged. Drift-check before every
  round — v4 ships daily.
  The previous baseline paragraph follows for history:
- **Oracle baseline: `f4955e0e` (2026-08-06, v4 4.8.0-dev.175),
  adopted at the found-bugs convergence round's unification — NO v4
  drift debt remains.** v4 HEAD `3fa36825` is TWO commits past it,
  BOTH zero-lib NO-PORT (verified by name): `cc0bbebf` test-only
  (two jest suites + CHANGELOG) and `3fa36825` docs-only (the Bug 44
  catalogue — the #39 mechanism correction specced upstream; when
  v4 IMPLEMENTS Bug 44, that lands on ported turn-resolution and is
  a real drift round). Oracles regenerate straight from
  `~/source/quilltap-server`; pin a detached worktree on any further
  drift (`oracle-regen-pinned-v4-worktree`, or `recipe_sweep.py --v4
  <pin>`). The distill-transitive TZ pins, the committed-fixture
  rule, and the venue/staging rules stand unchanged. Drift-check
  before every round — v4 ships daily.
  The previous baseline paragraph follows for history:
- **Oracle baseline: `3adefeba` (2026-08-06, v4 HEAD, tree clean),
  adopted at the Taboo + maintenance round's unification — NO v4
  drift debt remains.** ⚠ SUPERSEDED note (2026-08-06, the fallback +
  wire round): v4 has since moved to `7bcd8515` with a dirty tree —
  see the round bullet above; pin regens at `3adefeba`. `3adefeba` is release-notes docs atop
  `7df7de8e` (the Taboo feature, absorbed by P4.D50), lib-identical
  to it. Oracles may regenerate straight from
  `~/source/quilltap-server` while HEAD stays `3adefeba`; pin a
  detached worktree on drift/dirty
  (`oracle-regen-pinned-v4-worktree`), or pass the pin to the sweep
  driver (`recipe_sweep.py --v4 <pin>` — P4.40's addition; committed
  recipes never name a pin). The almanack NDJSON `baseline:` markers
  name the CASE vintage (`f7f1a956`), not the regen pin. The
  distill-transitive TZ pins, the committed-fixture rule, and the
  venue/staging rules stand unchanged. Drift-check before every
  round — v4 ships daily.
  The previous baseline paragraph follows for history:
- **Oracle baseline: `f7f1a956` (2026-08-05), adopted at the
  `f7f1a956` Almanack round's unification. ⚠ v4 HEAD is `7df7de8e` —
  TWO commits past it: `44e2e4fe` (docs-only, NO-PORT) and
  **`7df7de8e` "feat(taboo): instance-wide forbidden phrases in the
  system prompt", which LANDED within the hour of this round's
  unification and IS the owed drift catch-up**, on PORTED chat-spine
  surfaces (`system-prompt-builder.ts`, `context-manager.ts`,
  `cache-key.ts`, `settings.types.ts`, `instance-settings/index.ts`,
  `self-inventory/builders.ts` + a new settings route/component). The
  Taboo round runs first or alongside the resumed P4.37 (phase-4.md
  candidates 1-2); PIN a detached worktree at `f7f1a956` for every
  oracle regen until it is absorbed.** New: jest-based Chicago-leg
  regens need `--globalSetup
  harness/oracle/lib/jest-zone-globalsetup.cjs` + `QT_ORACLE_TZ`
  (v4's jest configs force TZ=UTC before workers fork). The
  distill-transitive TZ pins, the committed-fixture rule, and the
  recipe-sweep venue rules stand unchanged.
  The previous baseline paragraph follows for history:
- **Oracle baseline: `7189a968` (2026-08-05), adopted at the `7189a968`
  round's unification. ⚠ v4 HEAD is `0cde7fbc` (the Almanack rewrite),
  ONE commit past it, and v4's tree was DIRTY at unification — a drift
  catch-up is OWED (phase-4.md candidate 0): the
  `add-llm-logs-profile-columns-v1` migration (D23 territory), the
  UUID-remap list additions, `durationMs` + profile ids at ported
  logging call sites, the `getTotalTokenUsage*` `$ne: null` fixes; the
  Almanack report itself is unported surface. Until it runs, PIN a
  detached worktree at `7189a968` for every oracle regen.** The
  distill-transitive TZ pins, the committed-fixture rule, and the
  recipe-sweep venue rules stand unchanged.
  The previous baseline paragraph follows for history:
- **Oracle baseline: `7fe9fe40` (v4 4.8.0-dev.152, 2026-08-04), adopted
  at the `7fe9fe40` round's unification — NO v4 drift debt remains.**
  The two commits past `49769ec4` are both absorbed (`4bbeab47` →
  P4.D44, `7fe9fe40` → P4.D45). v4's tree was clean at `7fe9fe40`
  throughout the round, so every family regenerated straight from
  `~/source/quilltap-server`; pin a detached worktree only on
  drift/dirty (`oracle-regen-pinned-v4-worktree`). ⚠ The
  distill-transitive TZ=UTC pins, the committed-fixture regen rule, and
  the `/tmp`-pins-die-between-rounds rule stand unchanged — plus P4.34's
  new venue rule: run any `unstaged_jest_roots` family with
  `--v5w ~/source/quilltap-v5` (jest ignores `.claude/` venues), and
  prefer `recipe_sweep.py --run-all --results …` so classifications
  survive the round. Drift-check before every round — v4 ships daily.
  The previous baseline paragraph follows for history:
- **Oracle baseline: `49769ec4` (v4 4.8.0-dev.150, 2026-08-03), adopted
  at the `49769ec4` round's unification.** The four commits past
  `40319484` are absorbed or dispositioned: `74ec93b5` → P4.D42,
  `c988fbd2` → P4.D43, `51c350a1` + `49769ec4` NO-PORT (build/packaging,
  zero shipped behavior). **⚠ v4 HEAD is `7fe9fe40`, TWO commits past
  the baseline, BOTH behavior on ported surfaces — a drift catch-up is
  OWED:** `4bbeab47` (roleplay-template picker at chat creation — the
  ported chat-create route + the New-Chat SPA; two help docs → the
  `p4.9i2` bank) and `7fe9fe40` (stop teaching models asterisk
  narration — the aurora/commonplace/suparna writers + `core-whisper` +
  `native-tool-prompt.ts`; note v5's native-tool-prompt rule-1 wording
  was ALREADY stale from `8bf3cb5f`, so that catch-up closes two debts
  at once; mirror its 189-line feature doc). Until it runs, regenerate
  chat-create-family and writer/prompt-transitive oracles from a
  worktree PINNED at `49769ec4`; everything else regenerates straight
  from the checkout. ⚠ The distill-transitive TZ=UTC pins, the
  committed-fixture regen rule, and the `/tmp`-pins-die-between-rounds
  rule stand unchanged.
  The previous baseline paragraph follows for history:
- **Oracle baseline: `40319484` (v4 4.8.0-dev.147, 2026-08-03), adopted at
  the hard-link-groups round's unification.** The one commit past
  `c4d4b0de` is fully absorbed. **⚠ v4 HEAD is `c988fbd2`, ONE commit past
  the baseline — "feat(pascal): run presets for custom tools" — landing on
  the PORTED Pascal custom-tools surface: a drift catch-up is OWED**
  (`lib/pascal/tool-presets.ts` new + `custom-tool.types.ts` + the chat
  custom-tools route + `CustomToolRunDialog` + `lib/query/keys.ts`;
  `help/custom-tools.md` joins the `p4.9i2` bank). Regenerate
  pascal-family oracles from a worktree PINNED at `40319484` until it
  runs; the system/backup/restore/doc-mount families were verified
  untainted by name (the drift's four lib files are all pascal-side) and
  regenerate straight from the checkout. The round regenerated: the ten
  system-family oracles + the uuid-remap corpus at the union vintage,
  the doc-mount-file-links + mount-link-groups families (NEW), the 49-of-51
  deliverable-8 sweep, and the 31-of-38 neutrality sweep. ⚠ The
  distill-transitive TZ=UTC pins, the committed-fixture regen rule, and
  the `/tmp`-pins-die-between-rounds rule stand unchanged.
  The previous baseline paragraph follows for history:
- **Oracle baseline: `c4d4b0de` (v4 HEAD, 2026-08-01), adopted at the
  `c4d4b0de` drift-round unification — NO v4 lib drift debt remains.** The
  ten commits past `ff12f491` are all absorbed or dispositioned (see the
  round bullet above). v4's tree was CLEAN at `c4d4b0de` throughout the
  round, so every family regenerated straight from
  `~/source/quilltap-server`; pin a detached worktree only on drift/dirty
  (`oracle-regen-pinned-v4-worktree`). SDK majors verified at planning and
  unchanged (openai 7.2.0, `@openrouter/sdk` 1.2.2). **42 families
  regenerated there**, including the whole pascal/tool/workbench family
  over the REBUILT `pascal-run-custom-{main,mount}.db` (new vault ids in
  its committed `.meta.json` sidecar; a project tier and two
  effects-bearing tools added, with STORE DUMPS now diffed on all three
  route families), the post-office/announcer/context-transitive set, and
  the wardrobe/llm-choose/chat-cast/capstone set (three fixtures gained a
  shared wardrobe tier — all three had been structurally blind to the
  merge). Families outside those sets keep their prior regen vintage.
  ⚠ v4 has shipped ten commits in two days — **drift-check before every
  round.** ⚠ Since P4.d26 the distill-transitive tier-3 oracles are
  TZ-SENSITIVE; their recipes pin TZ=UTC. ⚠ The standing committed-fixture
  rule is unchanged (point oracles at the committed DBs; run
  fixture-mutating recipes against /tmp copies) — and note that **a recipe
  naming a `/tmp` pinned worktree from an earlier round is dead on
  arrival**, since those do not survive between rounds; six such recipes
  were repaired this round.
  The previous baseline paragraph follows for history:
- **Oracle baseline: `ff12f491` (v4 4.8.0-dev.135, 2026-07-31), adopted at
  the `ff12f491` drift-round unification — NO v4 lib drift debt remains.**
  The nineteen commits past `dcd9440a` are all absorbed or dispositioned
  (see the round bullet above). **⚠ v4 HEAD is `e1be028b`, ONE commit past
  the baseline: release packaging only (Dockerfile ×2 / README / versions /
  one build-script const) — zero `lib/`, `app/`, or dependency change,
  verified by name; NO-PORT.** Oracles may regenerate straight from
  `~/source/quilltap-server` while HEAD stays `e1be028b`; pin a detached
  worktree on any further drift (`oracle-regen-pinned-v4-worktree` — the
  pin needs `plugins/node_modules` + per-plugin
  `plugins/dist/*/node_modules` symlinks for provider corpora, and the
  installed SDK majors must match package.json: openai 7,
  `@openrouter/sdk` 1.2). The round regenerated: the whole
  pascal/tool/workbench family + the rebuilt `pascal-run-custom-*`
  fixture (D30), the restore/backup/remap/import family + the new
  memory-graph archive (D31), 290 non-sibling families (D32's sweep), and
  the four provider corpora byte-identical (D33). Families outside those
  sets keep their prior regen vintage. ⚠ Since P4.d26 the
  distill-transitive tier-3 oracles are **TZ-SENSITIVE** — their recipes
  pin TZ=UTC (+ the America/Chicago legs where named); never regenerate
  without the pins. ⚠ The standing committed-fixture regen rule applies
  unchanged: point oracles at the committed DBs, never a rebuild — and
  note D32's finding that some family recipes MUTATE committed fixtures
  in place (`embedding-generate-*`, `embedding-remainder-*`,
  `episodic-recall-*`): run those against /tmp COPIES.
  The previous baseline paragraph follows for history:
- **Oracle baseline: `dcd9440a` (2026-07-30), adopted at the P4.D29
  store-overlay-hardening unification. ⚠ v4 HAS ALREADY DRIFTED PAST IT.**
  The one commit past `5cc76688` is `dcd9440a` (a failed `properties.json`
  read no longer wipes a settings bag → P4.D29). Nine families regenerated
  there — the two the drift changes (`groups_tier2`, `projects_tier2`) and
  seven neutrality families (`groups_routes`, `projects_routes`,
  `group_doc_mount_links_tier2`, `project_doc_mount_links_tier2`,
  `vault_read_overlay`, `system_restore_state`, `system_import_state`) —
  all green, the happy paths output-neutral; the unification re-ran the
  round's other families (enclave-step, precompute, ui-search,
  file-attachment, attach-mount-file, the three provider corpora) fresh
  from a pinned `dcd9440a` worktree too. Families the round did not touch
  keep their prior regen vintage. **⚠ v4 moved THREE commits past the
  baseline during the round: `83118077` ("pascal custom-tool definitions
  load through the canonical mount reader") lands on the PORTED
  `lib/pascal/custom-tools.ts` — a drift catch-up is OWED (boundary
  enforcement via `resolveFsAbsolute`, blob-stored definitions becoming
  readable, a new `SOURCE_NOT_FOUND` race skip; the pascal /
  tool-definitions / workbench families are its blast radius) — while
  `71dcc7e8` and `80cafed5` are test-coverage-only (NO-PORT). v4's tree is
  CLEAN at `80cafed5`, but regenerate oracles from a worktree pinned at
  `dcd9440a` until the Pascal drift is absorbed
  (`oracle-regen-pinned-v4-worktree` — note the pin also needs
  `plugins/node_modules` + per-plugin `plugins/dist/*/node_modules`
  symlinks when regenerating provider corpora).** ⚠ Since P4.d26 the
  distill-transitive tier-3 oracles (orchestrator / salon-swipe /
  regenerate-swipe / enclave-step, plus the distill/precompute/replay/
  build-context families) are **TZ-SENSITIVE** — their recipes pin TZ=UTC;
  never regenerate without the pins. ⚠ The standing committed-fixture
  regen rule applies unchanged.
  The previous baseline paragraph follows for history:
- **Oracle baseline: `5cc76688` (v4 HEAD, 2026-07-30), adopted at the
  5cc76688 drift-catch-up unification — NO v4 drift debt remains.** The four
  commits past `083fdf68`: `505dcb1f` (same-day recall + fresh boost →
  P4.d26), `7391404e` (one embedding standard → P4.d27), `b3ee00f1` (Export
  Markdown → P4.d28), and `5cc76688` itself — **NO-PORT** (its only lib
  change is the forked-job-child write-buffer proxy, a locked v5 non-port;
  log-only even in v4; no oracle case imports it). Thirty-one oracle files
  regenerated fresh at `5cc76688` at unification, every one marker-checked.
  ⚠ **v4's working tree is DIRTY with in-flight store-overlay work**
  (`document-store-overlay.ts`, `backfill-{group,project}-stores.ts`) — the
  next drift is brewing on a PORTED surface; regenerate oracles from a
  pinned detached worktree until it lands and is absorbed
  (`oracle-regen-pinned-v4-worktree`). ⚠ Since P4.d26 the
  distill-transitive tier-3 oracles (orchestrator / salon-swipe /
  regenerate-swipe / enclave-step, plus the distill/precompute/replay/
  build-context families) are **TZ-SENSITIVE** — their recipes pin TZ=UTC
  (the day-references + distill families additionally carry a REQUIRED
  America/Chicago leg); never regenerate without the pins. ⚠ The standing
  committed-fixture regen rule applies unchanged.
  The previous baseline paragraph follows for history:
- **Oracle baseline: `083fdf68` (v4 HEAD, 2026-07-28), adopted at the P4.D25
  embedding-warmth drift-catch-up unification — NO v4 drift debt remains.** The
  four commits past `e8a49597` are v4's own fixes for its `found-bugs.md` Bugs 6
  and 7 (`a0243abd` the boot reconcile's stale exclusion, `f7cc887b` the
  `clearEmbeddingsForChat` age guard, `a5d6cee5` the mark* upserts + the
  FAILED-profile exclusion) plus a version chore; all three behavior commits
  carry an explicit "Oracle note for the v5 port". Seven families regenerated
  there — the five the drift changes (`embedding_status_tier2`,
  `conversation_chunks_tier2`, `collapse_stale_chat_caches_tier2`,
  `embedding_generate_jobs`, `embedding_remainder`) and the two neutrality
  families (`maintenance_sweep_tier2`, `cold_chunk_reembed_tier2`). Families the
  round did not touch keep their prior regen vintage. v4's tree is clean at
  `083fdf68`, so oracles regenerate straight from `~/source/quilltap-server`.
  `help/data-retention.md` rode along in `f7cc887b` and needs no v5 action (v5
  syncs help docs from disk at runtime). ⚠ v4 is mid-4.8/4.9 dev — drift-check
  before every round. ⚠ **When regenerating a family whose fixture is
  COMMITTED, point the oracle at the committed DBs** (the case headers' recipes
  rebuild into `/tmp` and a rebuild mints fresh UUIDs).
  The previous baseline paragraph follows for history:
- **Oracle baseline: `e8a49597` (v4 HEAD, 2026-07-27, 4.8.0-dev.108), adopted
  at the embedding-repair + chat-dialog round's unification — NO v4 drift debt
  remains.** The one commit past the prior `c1507f47` baseline is v4's fix for
  its own Bug 5 (a composer custom-tool run consulting the first participant's
  fact sheet rather than the operator's character); its ONLY `lib`/route change
  is `app/api/v1/chats/[id]/custom-tools/route.ts`, and lane **P4.D24** mirrored
  it into `api/custom_tools.rs`. Everything else in the commit is docs/version
  chores plus `help/custom-tools.md` (joins the `p4.9i2` bank). The pascal route
  family regenerated there (13 → 20 cases, the fixture gaining five perspective
  rooms) along with its fixture-invalidated sibling
  `pascal_run_custom_handler` (24); this round's other ten families regenerated
  there too. **No other oracle family imports the drifted file** (the pascal
  families are its only importers, verified at planning), so every other
  committed oracle keeps its prior regen vintage. v4's tree is **clean at
  `e8a49597`**, so oracles regenerate straight from `~/source/quilltap-server`;
  pin a detached worktree only on drift/dirty
  (`oracle-regen-pinned-v4-worktree`). ⚠ v4 is mid-4.8/4.9 dev and has shipped
  four commits in a single day before now — and shipped this one *mid-planning*
  — so drift-check before every round. ⚠ **When regenerating a family whose
  fixture is COMMITTED, point the oracle at the committed DBs**: the case
  headers' recipes rebuild into `/tmp` and then copy over the committed files,
  and a rebuild mints fresh UUIDs, so running the oracle against a fresh build
  without the copy diverges on ids that were never the port's doing.
  The previous baseline paragraph follows for history:
- **Oracle baseline: `c1507f47` (v4 HEAD, 2026-07-26), adopted at the
  P4.d22 restore/import-convergence unification — NO v4 drift debt remained.**
  v4's `67ffb444` (restore bugs 1–3) + `c1507f47` (import bug 4) fixed the four
  defects this port found; `20430561` and `41f34180` between them are docs-only.
  **All EIGHT families in the drift's blast radius were regenerated at
  `c1507f47` and re-run by name** — three convergence proofs
  (`system_restore_state`, `system_restore_equivalence`, `system_import_equivalence`)
  and five neutrality proofs (`system_import_state`, `system_export_equivalence`,
  `system_backup_equivalence`, `backup_uuid_remap_equivalence`,
  `system_delete_data_equivalence`; the uuid-remap corpus regenerated
  **byte-identical**, and `system_backup_equivalence` re-proved the archive bytes
  unmoved, which is what makes the committed `restore-archives/` fixtures still
  valid). Families the round did not touch keep their prior regen vintage. v4's
  tree is **clean at `c1507f47`**, so oracles regenerate straight from
  `~/source/quilltap-server`; pin a detached worktree only on drift/dirty
  (`oracle-regen-pinned-v4-worktree`). ⚠ v4 is mid-4.8/4.9 dev and has shipped
  four commits in a single day before now — drift-check before every round.
  **✅ THAT ROUND'S ONE OPEN ITEM IS RULED (2026-07-26, human): v5 KEEPS its
  placement and gains a skip check — `p4.d23`. Do NOT adopt `22a-bis`.** v4 moved its
  files phase to `22a-bis` where v5 runs it after the whole doc-store family.
  Both write the SAME ROWS with the SAME VALUES into the same mount at the same
  path — only the INSERTION ORDER differs — so it is `PHASE_ORDER_RESIDUAL` in
  `system_restore_state.rs`, asserted in both directions (align the placements
  and the test fails). v4 documents why its slot is right and later slots are
  worse: after 22c the replay hard-links to an archived content row and 22f's
  blob insert then violates `UNIQUE(fileId)`, refusing the ARCHIVED blob. v5 sits
  in that later slot — a latent hazard no committed archive triggers. **The lane
  recommended adopting `22a-bis` and was OVERRULED**: v4's own `found-bugs.md`
  names the proper repair (teach the replay to skip re-ingesting a file the
  archive already carries store rows for) and that check is only writable from
  v5's slot — at `22a-bis` the archived rows do not exist yet, so there is
  nothing to consult. It removes BOTH hazards instead of trading one for the
  other. Ordered as `work-orders/p4.d23-restore-file-replay-dedupe.md`; the
  ruling is in `status-log.md` → "Ruling — the restore file-replay dedupe" and
  inline in `system_restore_state.rs`. The lane did not act because the order forbade moving the
  phase order without a ruling. Details: `status-log.md` → "Lane record — P4.d22
  units 2–3".
- **P4.d23 — the restore file-replay dedupe: CLOSED on main (2026-07-26, single
  lane); the ruling is DISCHARGED and the skip check is LIVE.** v5's restore no
  longer re-ingests a file whose document-store rows the archive already carries
  (`orchestrator.rs` → `carried_store_rows`), and the divergence list GREW by one
  named entry (`REPLAY_DEDUPE`) exactly as the ruling predicted. Two committed
  archives built by v4's REAL `createBackup` make the claim measurement rather
  than analysis — `restore-archive-uploads.zip` (a store-backed `files` row) and
  `restore-archive-gen2.zip` (taken from an instance that was itself restored) —
  built by a SEPARATE builder so the existing five are byte-untouched
  (`system_backup_equivalence` re-proves it). `system_restore_state` 4 → **8
  cases**, the four new ones asserted in both directions and mutation-tested.
  **Two of the order's own premises were disproved by running them** (the point
  of the "establish before designing" instruction): the archived store rows do
  NOT key on the `files` row's id — `doc_mount_blobs.fileId` is a
  `doc_mount_files.id`, a disjoint space, and the storage key is the only exact
  handle; and **v5's slot never carried the predicted `UNIQUE(fileId)` hazard**,
  because v5's `link_blob_content` upserts by `fileId` and so REUSES the archived
  blob. v5's real cost was a spurious duplicate LINK per carried file,
  unique-suffixed and accumulating one more copy on every restore generation —
  quieter than the predicted crash and, over generations, worse. That correction
  is why the differential's tripwire is link arithmetic; the first assertions
  written passed with the check disabled, and only the mutation test caught it.
  v4's own second-generation loss is now MEASURED, not reasoned: it refuses two
  archived links and a folder where v5 restores the same archive with zero
  warnings. `PHASE_ORDER_RESIDUAL` was re-examined and **STAYS** — structurally,
  since a legacy disk-key file is still re-ingested on both sides, so the two
  slots still differ in insertion order; the check removed the hazards, not the
  ordering. Versions: core 0.0.381, harness 0.0.328. **Two items outstanding:**
  the e2e restore beat (`zzz-restore-destructive.spec.ts` — `apps/web` was not
  this lane's; it should ride the next round that already obliges a full
  Playwright run), and reporting the measurement back to the v4 side, where this
  repair is currently marked out of scope. Lane record: `status-log.md`.
  The previous baseline paragraphs follow for history:
- **Oracle baseline: `231be14c` (v4 HEAD, 2026-07-25), adopted at the
  P4.d18 ∥ P4.d19 ∥ P4.d20 ∥ P4.d21 drift-round unification.** Eighteen families
  regenerated there (the whole pascal/tool family plus chat-timestamp, the new
  fictional-clock-anchor, and the chat-create capstone). The §3 corpus has ONE
  source case file and TWO committed copies (harness + SPA), verified `diff -q`
  identical at unification. Superseded by `c1507f47` above.
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

## Superseded baseline paragraph — 03154b72 (archived 2026-08-14 at the 4.8.2/4.8.3-round unification)

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


## Superseded baseline paragraph (48396682, replaced at the help-drift unification 2026-08-14)

- **Oracle baseline: `48396682` (2026-08-13, v4 main — "merge: 4.8.3 back
  into main"), adopted at the 4.8.2/4.8.3 drift-round unification — NO v4
  drift debt remains.** v4 HEAD is `11553944` ("merge: 4.8.4 back into
  main"), ONE release past the baseline and **NO-PORT, verified**: the
  delta is two composer-typeahead test files, a jest test helper, and
  release docs — `git diff 48396682 main -- lib/ app/ packages/` is
  EMPTY, so oracles regenerate straight from the checkout while HEAD
  stays there. **Drift-check BOTH branches every round** (`git log
  <baseline>..main` AND `git log main..bugfix -- lib/ app/ packages/`;
  release content reaches main squashed via the `release:`/`merge:`
  pair, so measure drift with `git diff`, not the bugfix commit list)
  and verify the checkout's branch before any regen — pin a detached
  worktree on any mismatch, drift, or dirty tree
  (`oracle-regen-pinned-v4-worktree`, or `recipe_sweep.py --v4 <pin>`).
  The sweep driver remains the sanctioned per-family regen path — never
  run two sweeps concurrently (shared /tmp paths race; measured), and
  the provisioning family's two v4-side legs must run from the v4
  checkout (recipe repaired this round). The distill-transitive TZ pins,
  the committed-fixture rule, and the venue/staging rules stand
  unchanged.

## Superseded baseline paragraph — `24633026` (archived at the aa464abf-round unification, 2026-08-15)

- **Oracle baseline: `24633026` (2026-08-14, v4 main — "feat:
  section-level help embeddings and content search in the Guide"),
  adopted at the help-drift unification — NO v4 drift debt remains.**
  ⚠ v4's working tree carries uncommitted **Ollama "Enable Thinking"
  WIP** (the next drift, already in flight) — verify branch +
  cleanliness before ANY regen; pin a detached worktree on
  mismatch/drift/dirt (`oracle-regen-pinned-v4-worktree`, or
  `recipe_sweep.py --v4 <pin>`). **Drift-check BOTH branches every
  round** (`git log <baseline>..main` AND `git diff main bugfix -- lib/
  app/ packages/` — measure bugfix with `diff`, never the commit list;
  bugs 64/65 sit BELOW the 4.8.3 marker and are pre-baseline). The
  sweep driver remains the sanctioned per-family regen path — never run
  two sweeps concurrently; it now copies `.db.meta.json` sidecars when
  shielding, NEVER runs a committed-corpus family's recording stage,
  and warns when a family's stages modify tracked fixtures. The
  distill-transitive TZ pins, the committed-fixture rule, and the
  venue/staging rules stand unchanged.

---

## Superseded baseline paragraph (replaced at the 93ed8abf-round unification, 2026-08-16)

- **Oracle baseline: `aa464abf` (2026-08-15, v4 main — "fix:
  archived-seat badge (66), source-view send (67), archive digest
  clobber (69)"), adopted at the aa464abf-round unification.** ⚠ v4 HEAD
  is ALREADY PAST it: **`f933ba9c` (bug 70, context budget honors Max
  Context) is the queued next drift** — top candidate in `phase-4.md`;
  part is likely v4 converging on v5's `context_budget.rs` shape
  (MEASURE, `convergence-lane-measure-dont-assume`). **Pin a detached
  worktree at `aa464abf` for EVERY regen until that round lands**
  (`recipe_sweep.py --v4 <pin-path>`; ALL THREE symlink classes: root
  node_modules, `packages/quilltap/node_modules`, the
  `plugins/dist/*/node_modules` dirs). **Drift-check BOTH branches every
  round** (`git log <baseline>..main` AND `git diff main bugfix -- lib/
  app/ packages/` — measure bugfix with `diff`, never the commit list).
  The sweep driver remains the sanctioned per-family regen path — never
  run two sweeps concurrently. The distill-transitive TZ pins, the
  committed-fixture rule, and the venue/staging rules stand unchanged.

---

## Superseded baseline paragraph (replaced at the d123658d-round unification, 2026-08-17)

- **Oracle baseline: `93ed8abf` (2026-08-15, v4 main — "fix: local
  providers send the profile's parameters; OAC can call tools (bug 71)"),
  adopted at the 93ed8abf-round unification; the drift debt is CLEARED
  at the pin.** Pin a detached worktree at `93ed8abf` for every regen
  whenever the v4 checkout isn't cleanly on it (`recipe_sweep.py --v4
  <pin-path>`; ALL THREE symlink classes: root node_modules,
  `packages/quilltap/node_modules`, the `plugins/dist/*/node_modules`
  dirs). **Drift-check BOTH branches every round** (`git log
  <baseline>..main` AND `git diff main bugfix -- lib/ app/ packages/` —
  measure bugfix with `diff`, never the commit list). The sweep driver
  remains the sanctioned per-family regen path — never run two sweeps
  concurrently. The distill-transitive TZ pins, the committed-fixture
  rule, and the venue/staging rules stand unchanged.
