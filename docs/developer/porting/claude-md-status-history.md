# CLAUDE.md Status history (archive)

Round bullets and superseded oracle-baseline paragraphs moved VERBATIM out of
CLAUDE.md's "Status" section on 2026-08-13 and 2026-09-19 (each time it had
grown past the per-turn size limit; precedent: the 2026-07-10 move of the
unit-by-unit journal to `status-log.md`). Newest-first authority for any round
remains
`docs/developer/porting/status-log.md` — this file only preserves the exact
phase-level summary text CLAUDE.md used to carry.

Contents:
1. Round bullets, the P4.6f/g/h round (2026-07-10) through the `5cc76688`
   drift catch-up round (2026-07-30), archived 2026-08-13. Continued in §3.
3. Round bullets, the drift + standing-red + dogfood round (2026-07-30)
   through the `31436bae4` drift catch-up round (2026-09-15), archived
   2026-09-19. Later rounds remain in CLAUDE.md.
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

## 3. Archived round bullets (2026-07-30 → 2026-09-15)

Archived from CLAUDE.md on 2026-09-19: the drift + standing-red + dogfood
round (P4.D29 ∥ P4.20 ∥ P4.21 ∥ P4.9P) through the `31436bae4` drift catch-up
round (P4.D182 → {P4.D183 ∥ P4.D184 ∥ P4.D185} ∥ P4.D186 ∥ P4.D187 ∥ P4.D188).
The text below is verbatim; CLAUDE.md carries one compressed arc bullet in its
place, and `status-log.md` remains the record of authority for every round.

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
- **The follow-ups round 2 (P4.72 ∥ P4.73 ∥ P4.74 ∥ P4.75): UNIFIED on
  main (2026-09-04) — P4.72 / P4.74 / P4.75 CLOSED, P4.73 PARTIAL (its
  `?action=generate` leg + P4.62(a)'s FILES leg OPEN); the baseline STAYS
  `0b0617fee`.** The second non-drift round: the whole P4.67 remainder (the
  `?action=` family at 32 endpoints, the duplicate-key rows, P4.62(c)) + the
  dispatch wrong-type census (240 rows; its `*_id` exclusion PINNED at 403
  after the review found it dropping real v4 body keys — the honest allow-list
  is a named candidate) ∥ the never-ported `/api/v1/images` COLLECTION route
  (list / upload / import-from-URL over a new host fetch seam / the `{id}`
  DELETE) over a new committed fixture + a 32-case real-DB family, **the host
  pixel codec threaded into chat uploads** (P4.D152's candidate; v5 now
  transcodes to WebP as v4 does), the `ChatCreate` wrong-type trio on both
  transports ∥ the failover `auth` chain arm, two stray status parsers retired
  (a thirteenth census-named), six shared-stage recipes re-staged, all sixteen
  `[Image Fallback]` calls dispositioned, the first written handler-logging
  inventory ∥ the streaming bubble's avatar (v4's ONE `shouldShowAvatars` —
  v5's `GROUP_ONLY` arm was an invention), the search-documents intermittent
  root-caused to the BEAT, the `title=` census + two copy repairs, the residue
  hosts. **The §3 review's headline catch: P4.73's dedup arm bypassed the UUID
  refusal and answered 201 where v4 answers 400 before any write** — fixed with
  five new arms on both sides, mutation-proven; twelve should-fixes across the
  four lanes; the cross-lane `ChatCreate` tripwire fired at the unified gate as
  designed. Gate: fmt/clippy both feature sets clean; 17/17 families fresh from the `0b0617fee` pin zero SKIP (+2 and +1 re-runs after the review fixes); **494 test binaries / 2,780 / 0 / 1 ignored, zero SKIP**; release build; ng 378 files / 5,945; full Playwright **274 / 0 / 0** after the gate's own catch — the image-detail beat's two seeds were pixel-identical and now dedup under the host codec, as v4's do (fixed spec-side). Versions: core 0.0.776, harness 0.0.671, web
  0.0.114, host 0.0.96, SPA 0.5.642. 💸 the dogfood queue gains the images
  route + the chat-upload WebP transcode + the streaming avatar on real data.
  **⚠ v4 landed TWELVE commits during the round (its 4.9 release-checklist
  push — classified in the ledger's §3 at the unification's drift step; the
  round's regens were all pinned). Next: that drift catch-up, then `p4.9i2`
  as its own round — see phase-4.md.** Round record: `status-log.md`.
- **The `d883a5ee1` drift catch-up round (P4.D153 ∥ P4.D154 ∥ P4.D155 ∥
  P4.D156 ∥ P4.D157 ∥ P4.D158): UNIFIED on main (2026-09-05) — ALL SIX
  CLOSED; the oracle baseline MOVES to `d883a5ee1` and the fifteen-row
  drift debt is CLEARED** (bug 119's row stays for `p4.9k`). v4's two
  live-Friday bug fixes absorbed whole — **v5 reproduced both**: bug 122
  (the memory-subject prefix `About <name>: ` through the three
  self-facing formatters, inside the token estimate; the RAW-path name
  lookup that cannot reach the vault; the oracle case's positional arity
  fixed FIRST — the ledger's silent-regen trap) and bug 121 (the USER-side
  attachment walk as a fourth leaf with v4's ten cases; re-hydration BEFORE
  `build_context` with the 80,000-char skip-whole budget; the orchestrator
  corpus widened to SEE it) ∥ the `0506517d3` collapse's seven corrections
  (five measured present in v5: the priority-5 params drop, the hard-coded
  `is_local`, the preview's inline filter, "File not found not found", the
  lowercase Brahma sentence) + the Pascal placeholder classifier once on
  each side ∥ bug 120 (Tier R 214 → 216/0), the About sentences, the
  `qt-checkbox`/toggle-row adoptions ∥ the `d4138b96b` dead-code decision
  (thirteen symbols DELETED — no twin had a production caller; seven
  families SPLIT, none frozen) ∥ the Opus 5 sampling strip red-first, every
  provider corpus re-recorded at the pin (two version-marker fields moved,
  nothing else), `2edd823c0`'s four bag-key blind spots as restore arms
  over a new archive, the `docs/v4/` mirror refreshed. **The §3 review's
  headline catch contradicted a lane's ratification:** `zod` 4.4.3 → 4.5.4
  DID move v5 bytes — both lanes diffed the locale, the change is in core
  (`unrecognized_keys` now continuable, so a stray key keeps a union branch
  alive and its refines fire); both hand-rolled Zod engines fixed
  red-first, the SPA corpus refreshed. Also at the wire: the neutrality
  sweep for `0506517d3` (409 families: 402 green, the seven non-green rows all run to ground — three were Zod 4.5's code-point length rule, one a fixture-vintage artifact, one an oracle mock lagging the collapse, one a moved import, one the deliberate repo-writer — none the collapse's; it is NEUTRAL). Gate: fmt/clippy both feature sets clean; release build; 496 binaries / 2,802 / 0 / 1 ignored with the 67-var env block (Tier R 216/0 inside it); 409-family sweep 402 + 6 repaired + 1 refused; the round's families by name from the pin zero SKIP; ng 380 files / 5,962+; full Playwright 274/274 zero skips. Versions:
  core 0.0.795, harness 0.0.685, cli 0.0.18, SPA 0.5.646. **The owed
  dogfood pass is the top next candidate.** Round record: `status-log.md`.
- **The follow-ups-round-2 + `d883a5ee1`-round dogfood pass RAN (2026-09-05,
  agent-driven, on the Friday copy) — 22 rows, 18 PASS, ONE finding found and
  FIXED, and the round's whole 💸 queue discharged bar three cost items.** Walk
  doc: `dogfood-walks/2026-09-05-images-route-memory-subject-pass.md`; record in
  `status-log.md`. The ledger's §2 probe **passed** at walk start (v4 HEAD = the
  baseline, no drift), so nothing could blame drift. **The pre-walk measurement
  handed the pass its best proof:** v4 had run its own bug-122 fix on this
  instance all day, so `llm_logs` already held v4's post-fix bytes
  (`[m_fb6d] [today] About Charlie: …`) for the same chat and character v5 was
  then driven through. **FIXED: finding #110** — the daily maintenance pass
  **deletes the operator's generated images in total silence**; found by
  consequence when `files` IMAGE fell 2,831 → 2,827 between two measurements and
  only `lastMaintenanceSweepAt` recorded that anything happened. The deletion is
  v4-faithful; the silence was the port divergence — v4 emits eleven lines v5
  dropped, and the collapse's own comments named the warn they were dropping
  (**finding #103's exact shape**). It had also escaped the P4.74 logging
  inventory, which surveys only `lib/background-jobs/handlers/*.ts` (scope gap
  recorded as a standing note). Fixed with v4's sentences at v4's levels, four
  capture-layer tests, six mutations; `maintenance_sweep_tier2_equivalence` green
  on a fresh oracle so behaviour is unmoved (`fda5852e`, core 0.0.796; gate 496
  binaries / 2,806 / 0, zero SKIP). **Proven live:** bug 122 verified row by row
  against `memories` (the *own-id* silences are half the proof); bug 121 spliced
  back into its carrying message for a third seat; the whole `/api/v1/images`
  collection route incl. the omit-null rule on real NULL rows, the `IMAGE_IN_USE`
  refusal and `?action=bogus` falling through to upload as v4's route does; the
  host pixel codec on the real chat-attachment path (188-byte PNG → 92 bytes of
  genuine WebP), closing P4.D152's candidate; the streaming avatar with its danger
  ring in the *waiting* state; the cheap-LLM priority-5 carry on a tapped Ollama
  wire with profile-only keys planted so the cheap task's sampling could not mask
  them; the uncensored desk on a Flagged chat; `instances default --json` in all
  four arms; the About sentences (U+0027); the Workbench's four placeholder arms
  with the `allowState` split proven both ways; and the export preview at
  2,885 = 2,895 − 10. ⭐ **Pascal's group tier CLOSED after six deferrals — and
  the reason it kept failing is the result:** a manual operator run has no
  invoking character, so the group tier is not searchable and the write correctly
  falls to chat; the project tier resolved in the identical setup, and adding
  `asCharacterId` gave `tier: "group", previous: 42`. Also banked: clearing every
  `isCheap` flag does NOT reach priority 5 — `cheapLLMSettings`'
  `defaultCheapProfileId`/`userDefinedProfileId` sit above it. **Five instrument
  errors caught**, incl. a gate piped through `tail` (CLAUDE.md's own named
  mistake) reporting 58 binaries for a 496-binary run, and the bug-121 gesture
  landing on the authoring seat because 12,388 of 12,607 user messages here carry
  a `participantId`. **Still owed:** the Brahma key-less sentence (blocked — all
  eleven providers have keys), the Opus 5 *byte* strip (HTTPS + a pre-builder
  projection; the outcome passed), the Brahma deep query, dedup/summaries, and
  #101. Every setting the walk changed was restored and every probe artifact
  removed.
- **The `p4.9i2` help/HelpChat round (P4.9I2A ∥ P4.9I2B ∥ P4.76 ∥ P4.77):
  UNIFIED on main (2026-09-05) — ALL FOUR CLOSED; the oracle baseline MOVES
  to `c2232cd9a` (the 4.9.0 release cut ratified as zero-code).** The last
  unported vertical of any size: the help/HelpChat family whole — v4's
  120-file `help/` tree vendored at the pin and EMBEDDED in the binary (v5
  had NO help tree; the host's cwd walk had synced an empty one since
  P4.6BM), the boot-time ensure wired (the P4.D77 backfill finally
  reachable), the help-docs read verbs + the nine help-chats verbs + REST
  edges, the six-strategy context resolver (v4's duplicate-wildcard quirk
  reproduced, a candidate upstream filing), the byte-exact help system
  prompt, the help-chat orchestrator on a LIVE send seam (`maxAgentTurns =
  10`; help chats DO extract memories) with seven differentials incl. a
  whole-tree content oracle and a tier-3 orchestrator family (12 cases);
  the SPA's Help dialog (Guide + Ask, the streaming fold, the entity picker,
  the rail entry) with v4's jest suites as parity specs ∥ P4.73 CLOSED WHOLE
  (`?action=generate` over an erased image-generation seam with a
  frozen-clock 37-case oracle; the FILES leg; the five recorded items) ∥
  P4.77 (the `zod` version tripwire; P4.D159's ratification; 19 → 6 capture
  rigs; `render_template`'s debug lines). **The §3 review caught four
  blocking findings before main:** a version-only conflict resolution that
  dropped P4.77's `test-support` feature block (clippy's `unexpected cfg`
  was the tripwire; every lane's non-version manifest delta audited); the
  id-less tool-row drop applied to every provider where v4's GOOGLE plugin
  KEEPS the row (per-provider now, with v4's `unknown_function` chain — a
  GOOGLE corpus arm is the follow-up); a `break` where v4 THROWS on a
  mid-stream provider error (a failed model call became a billed half reply
  — fixed with the `stream_error_mid_turn` corpus arm); the two unifier
  wires; and, from the activated help beats' first live run, **v5 never
  created the `help_docs` table** (v4's `ensureCollection` is lazy; a
  pre-help_docs instance booted to an empty Guide and dead sends — a boot
  ensure now, host-leg + mutation-proven). Twelve should-fixes fixed (headline: the SPA's auto-select left the
  LAST seat deselected where v4's effect deps re-select it — two specs had
  pinned the pre-fix `[]`). Gate: 26 families fresh from the pin zero SKIP;
  508 binaries / 2,872 / 0 zero SKIP; ng 387 / 6,244; Playwright 282/282 zero skips.
  Versions: core 0.0.805, harness 0.0.694, web 0.0.119, host 0.0.103, SPA
  0.5.651. 💸 the dogfood queue gains the Help dialog end to end, the
  generate route with a real key, a GOOGLE-seated help chat. Round record:
  `status-log.md`.
- **The `p4.9i2` help-round dogfood pass RAN (2026-09-05/06, agent-driven, on
  the Friday copy) — 31 rows, 21 PASS, findings #111–#115: TWO FIXED on main,
  two v4-faithful filing candidates, one order-sized gap.** Walk doc:
  `dogfood-walks/2026-09-06-help-dialog-pass.md`; record in `status-log.md`.
  **FIXED #111** — help streamed turns wrote no `llm_logs` rows (v4 logs every
  `streamMessage`; the loop bypassed `primary_stream`'s logger; `messageId`
  NULL as v4's help rows carry; tier-3 pinned, 26 rows, mutation-proven);
  Brahma's identical gap measured live (three calls, zero rows) and recorded
  as the follow-up. **FIXED #113** — a `data:` URL image import was a flat 500
  (reqwest has no data: URL processor; Node's fetch does) — the Fetch
  Standard's processor now runs in the host seam, six Node-measured vectors.
  **Recorded, v4-faithful, to cross-check on live v4 then file:** #112 (a
  tool-needing help turn ends silent on nine of ten providers — the loop's
  tool rows carry no `toolCallId` and the plugins drop them) and #114
  (Google refuses `additionalProperties` under `items` — `wardrobe_wear`/
  `wardrobe_take_off` — so every tool-enabled GOOGLE turn with the wardrobe
  tools in its slate is a 400; blocks the P4.9I2 §3 GOOGLE-keeps-tool-rows
  live leg). **Needs an order:** #115 — `chatCreate` lacks v4's whole
  `createChatSchema` parse (ten arms; a stored `controlledBy: "LLM"` split the
  server's and the SPA's readers). Also proven: the page context in the
  prompt, the two-seat chain's seven frame kinds, the seat snap-back
  discriminated, the generate route's Zod arms against v4's real schema (a
  vacuous first pass caught and re-run), the FILES leg's bytes-then-refuse
  orphan, the four `render_template` debug lines live. Still owed (human):
  B3/B4 image spend, the Brahma deep query, dedup/summaries, #101.
- **The `f699da6f6` 4.9.x drift catch-up round (P4.D160 ∥ P4.D161 ∥
  P4.D162 ∥ P4.78 ∥ P4.79): UNIFIED on main (2026-09-06) — ALL FIVE CLOSED;
  the oracle baseline MOVES to `f699da6f6` and the drift debt is CLEARED**
  (v4's 4.9.1 + 4.9.2 release cycles; bug 119's row stays for `p4.9k`). Bug
  123 whole — server: the per-emit OPTIONAL `paused` chain-complete key (v4
  set it at the four `executeTurnChain` emits only), the paused early-return
  with v4's info line, two re-vendored help pages; SPA: the seat-keyed Skip
  banner (three sentences byte-exact), the overlay-aware Skip with a SILENT
  unpause-first, the pause-you-did-not-cause toasts after the single
  reconcile point, v4's pause-sync drift a mutation-proven NO-COUNTERPART
  (v5 holds no local pause latch), two live beats ∥ bugs 124/125 whole —
  this port's own dogfood filings #112/#114 coming back fixed: the help loop
  through the `tool_call_threading` primitive it already had, with the
  family's FULL-slate comparand (the `{role, content}` key was blind to the
  pairing) + an id-less case; `additionalProperties` at the head of Google's
  strip list with the real wardrobe schemas appended to the recorded corpus
  (pre-existing rows byte-identical); a GOOGLE seat in the help-chat fixture
  pinning `keeps_idless_tool_rows` in BOTH directions ∥ finding #115 —
  v4's whole `createChatSchema` as ONE validation stage ahead of any work,
  36 → 108 capstone cases with the Zod `details` byte comparand (**43 bodies
  v4 refuses had been CREATING chats**; 14 more answered a downstream
  sentence); the host wire for `details` is a one-line, tripwire-held
  deferral ∥ finding #111's Brahma remainder — both engines log every
  streamed turn, and a mid-stream provider error stops the turn instead of
  persisting a half reply. **The §3 review: NO blocking findings** (the
  fifth such round); eight should-fixes on the unify branch — headline:
  D162's tie normalizer was transitive and uncapped (a burst case could
  quietly become a multiset compare), and P4.78 had three ported rules no
  corpus arm measured. **The reconcile's own catch: four identical version
  bumps auto-merged silently across five lanes** — recounted as base + total
  bumps. Every lane's first §2 probe FAILED (v4 ran its 4.9.2 cycle between
  ordering and pickup) and every lane STOPped as designed until the orders
  were repointed to one pin. Gate: 9/9 families fresh from the pin zero
  SKIP; google corpora re-recorded byte-identical; 508 test binaries / 2,891 passed / 0 failed / 1 ignored, ZERO `SKIP:` lines, exit 0 (the round's eight block families confirmed RUN by per-binary duration: capstone 3.31 s, orchestrator 3.02 s, help-chat 1.29 s, help tree 1.02 s); clippy both
  feature sets; release build; ng 387 files / 6,268; full Playwright 284 passed / 0 failed / 0 skipped (7.6 m), exit 0 — the suite grew 282 → 284 with P4.D161's two toast beats; the two Brahma beats green on a real reply.
  Versions: core 0.0.811, harness 0.0.700, web 0.0.120, host 0.0.105, SPA
  0.5.657. 💸 the dogfood queue gains a REAL server-side pause announced,
  the off-turn Skip, the #112/#114 live legs, a Console question's
  `llm_logs` rows, a `controlledBy: "LLM"` create refused. **The owed
  dogfood pass is the top next candidate.** Round record: `status-log.md`.
- **The `f699da6f6`-round dogfood pass RAN (2026-09-06/07, agent-driven, on
  the Friday copy) — 18 rows, 16 PASS, ONE finding found and FIXED, one
  recorded, and the round's whole 💸 queue discharged.** Walk doc:
  `dogfood-walks/2026-09-06-pause-skip-google-tools-pass.md`; record in
  `status-log.md`. The ledger's §2 probe passed at walk start and the one
  open drift row is an unported surface, so nothing could blame drift. **The
  pre-walk measurement bought the best comparand:** v4 has already written
  `llm_logs` rows for its OWN Brahma chats here — 56 across the two most
  recent — in exactly the shape P4.79 ports, so D1 was judged against v4's
  own bytes. **Proven live:** all four gates of bug 123's pause announcement,
  *discriminated* rather than asserted (the chain-error **warning**, the
  out-of-band mid-chain **info** — different severity and sentence — and
  **silence** when the operator paused it themselves, the same frame judged
  against a different pre-turn belief); the off-turn Skip banner in both
  sentences on one chat; Skip lifting a pause silently; an impersonated LLM
  seat skipping through the Bug-44 overlay; **dogfood #112 and #114 — this
  port's own filings — confirmed fixed on the configurations that produced
  them**, with one fresh-seat GOOGLE help turn proving bug 125's strip, bug
  124's threading and the P4.9I2 §3 id-less-row rule at once; P4.78 refusing
  six shapes with nothing written, its guard order proven by discriminator
  (the same bogus continuation id answers 404 with a good body, 400 with a
  bad one); and P4.79 writing six `CHAT_MESSAGE` rows for one Console
  question plus, against a purpose-built half-stream endpoint, persisting
  **no half reply and no salvage sentence**. **FIXED: finding #116** — the
  Google builder strips every tool from a turn on two arms v4 *announces* and
  v5 took in silence, so a GOOGLE-seated help turn came back empty with
  `UNEXPECTED_TOOL_CALL` and nothing in the log said why; the behaviour is
  v4-faithful and stays, the two log lines were the whole gap (the #103/#110
  class), ported with v4's counts, four capture tests, three mutations each
  reddening one and a fourth surviving *correctly* and recorded rather than
  chased (`4e62e936`, core 0.0.812; live-proven on the same instance).
  **RECORDED: finding #117** — a salon chat cannot be deleted on ANY v5
  surface (`DELETE /api/v1/chats/{id}` is 405 and no `chatDelete` verb
  exists); the client half is a documented P4.6g deferral whose note names
  only the card, so the server edge went unported with it — needs an order.
  Three instrument errors banked (a missed Send click made a whole arm
  vacuous; NULL-`role` `chat_messages` rows are system events, 52,000 of them
  predating v5; a dispatch verb answered 200 with a full payload having
  written nothing, every field being `#[serde(default)]`). **Still owed
  (human):** the Brahma deep query, dedup/summaries, and #101.
- **The `p4.9k` character-generators round (P4.9K0 → P4.9K1 ∥ P4.9K2 ∥
  P4.9K3 ∥ P4.9K4 ∥ P4.80 ∥ P4.81): UNIFIED on main (2026-09-07) — K0, K3,
  K4, P4.80 CLOSED; P4.81 CLOSED except item 7 (measured, not fixed); K1 and
  K2 OPEN — PARTIAL at their pure first units; the baseline STAYS
  `f699da6f6`** (no drift; the first round since P4.59 with none to absorb).
  The substrate whole (the four shared leaf modules tier-1 exact — 46/20/13/6
  rows; the `generatorProgress` Event kind + emitter; the web SSE re-framer
  with v4's byte-exact framing) ∥ the optimizer's pure arms with bug 119's
  `coerce_suggestion_array` byte-exact + the rename service's literal-scan /
  `GetSubstitution` / Canonicalize measurements banked as a ruling ∥ the
  wizard's generated prompt table (TEN `FIELD_PROMPTS`, not the order's
  thirteen) + `build_context_prompt` ∥ the whole edit-side SPA (wizard modal
  from both hosts, Rename & Replace, the prompt-editor modals) and the whole
  detail/list/cast SPA (optimizer + apply path, external prompt, the AI
  import on two mounts with the Summon stub retired), all mocked against §B
  with six beats gated by NAME ∥ **dogfood #117 CLOSED** (the `chatDelete`
  verb over the caller-less cascade, v4's whole DELETE dispatch, a 21-row
  table-census family over the new `chat-delete-*` trio, the trash button on
  both cards — and a v5-only workspace bug found on the way: every button
  inside a card's link was eaten by the capture-phase interceptor) ∥ the two
  host wires, the `text_block_turn` marker (its second stream now fires),
  the three chain-stop log lines, the composer's wide `hasActiveCharacters`
  twin with a live beat. **The §3 review caught two would-have-shipped SPA
  defects:** composite outfits saved with NO components (the create
  response's key misread — and the host specs' stubs answered the same wrong
  key), and gallery images the wizard server could never resolve (a vault
  link id sent as a files id); plus three chat-delete edge divergences (an
  empty body's 500, the root-level Zod issue, the silent broken-vault warn),
  a corpus-blind truthiness test, and a copy/pin batch — all fixed with pins;
  the qt-class guard caught an invented class at the unified gate. Gate:
  fmt + clippy both feature sets clean (re-checked after the review fixes' rustfmt); release build; the seven affected families regenerated FRESH from the `f699da6f6` pin through the sweep driver (7/7, then the chat-delete family re-run at 21 rows and the wizard-prompts family at 19 contexts after the review fixes, zero SKIP, changed bytes grepped — the repaired help-chat marker present ×2 and the old form ×0, 34 calls / 6 framed rows); `cargo test --workspace` with the round's 19-variable env block **515 test binaries / 2,932 passed / 0 failed / 1 ignored, zero `SKIP:` lines, exit 0** (every round family confirmed RUN by name); mutation proofs run at the wire (the pronouns null test, the object gate, the resolver warn — each reddening exactly its rows); SPA lint clean (incl. the qt-class guard), `npm test` 401 spec files / 0 failed, build clean; full Playwright **288 passed / 0 failed / 6 skipped (the six named `P49K1/K2_SERVER_LANDED` gates), exit 0** — after the reverse-`{{user}}` beat's first live run was repaired spec-side (unlock-first, the rendered-shape assertion, select-by-value) and the two P4.D161 pause-toast beats' once-per-page hook was made per-frame (red twice in the full suite, green alone and in three pairs with the new specs); the quill spec's one red in one run is its documented slow-stream intermittent (green alone). **Next: finish K1/K2 as RUNNER-sized units (the beats'
  first-run recipes are in their headers), then the owed dogfood pass** —
  see phase-4.md. Versions: core 0.0.819, harness 0.0.708, web 0.0.123, host 0.0.106, SPA 0.5.665; cli/tauri unchanged. Round record: `status-log.md`.
- **The `2f4254b42` character-subprompts round (P4.D163 → P4.D164 stacked ∥
  P4.D165 ∥ P4.9K1-resumed ∥ P4.9K2-resumed): UNIFIED on main (2026-09-07)
  — P4.D163/P4.D164/P4.D165 CLOSED, P4.9K1/P4.9K2 CLOSED with their Tier-2
  items 9/10 + K2's Tier-3 recording named OPEN; the oracle baseline MOVES
  to `2f4254b42` and the drift ledger's §3 is EMPTY.** v4's whole
  character-subprompts feature absorbed: **unit 1 was a measured data-safety
  fix landed red-first** — v5's `ChatParticipant` had no unknown-key carry,
  so every v5 participant rewrite dropped a v4-written `selectedSubpromptIds`
  on the shared Friday instance — then the vault-backed `subprompts` module
  (139-row tier-1 helpers; storage + fan-out + the case-insensitive resolver
  over the NEW committed `subprompts-{main,mount}.db`), the five verbs + REST
  edges + realtime with the guard ladders MEASURED, the four `help/` files
  re-vendored (121), the `## Additional Instructions` block with
  `IDENTITY_STACK_BUILDER_VERSION` still 2 (both P4.D103 goldens + the
  cross-implementation hash UNMOVED), the compiler bake, the fallback-only
  resolve, the greeting's RAW six-key context, the green room at BOTH
  `llm_choose` entrances, `subprompts_prompt_tier2_equivalence` end to end,
  and the whole SPA half with its walk LIVE (the single-character New-Chat
  picker a measured NO-COUNTERPART). The two resumed generator lanes landed
  every Tier-1 item in ONE worktree: rename (a literal scan under ECMAScript
  Canonicalize + `GetSubstitution`), the external prompt, the optimizer
  runner with bug 119's containment (the ledger's `15573c3a1` row ABSORBED),
  refresh-archive, both wizard runners + five generators, `ai_import` whole
  with a recorded `VALIDATION_UNAVAILABLE` refusal (⚠ no JSON-Schema crate —
  a `jsonschema`-class dependency is the retirement path, for the human),
  both host drivers LIVE (💸), seven REST arms, seven differentials, the NEW
  `character-generators-{main,mount}.db`. **The §3 review (three parallel
  readers): NO blocking findings; every should-fix fixed on the unify
  branch** — the green-room debug line dropped v4's `chatId` (ported at the
  wrong SITE; moved into the seat resolver, pin rewritten over the committed
  pair), the `SubpromptRecord` wire key order unpinned (both new families
  sort keys — pinned raw), the AI-import validation pin blind to its own
  refusal DISAPPEARING, a rename PREVIEW holding the single writer for the
  whole scan (dry run → the read pool), the `build_context` pool-failure warn,
  six `selectedSubpromptIds` reads on one helper, the `?action=` divergence
  rows rotted to prefixes, the picker's four-hop output chain pinned only by
  handler-direct specs (two click-driven wiring specs), the walk's ambiguous
  Delete locator. **The reconcile's own catch:** the dispatch census constant
  landed TWICE off the same base (unified to 426). Wires: the six §B DTOs
  folded into `core-contract.ts` with the casts retired; six gated beats
  flipped; the optimizer and AI-import beats' owed recipes discharged (their
  runners call the NON-streaming `send_message`, which the shared SSE mock
  could never answer — spec-local non-streaming mocks; the reinforced
  memories seeded by global setup's pre-server CLI write, no verb carrying
  `reinforcementCount`); the wizard beat's invented CLI subcommand respelled.
  Gate: 32/32 families fresh from the two pins zero SKIP; **529 test binaries / 2,980 passed / 0 failed / 1 ignored, zero `SKIP:` lines, exit 0**;
  clippy both feature sets; release build; ng 405 files / 6,436; full
  Playwright **295 passed / 2 failed / 0 skipped (8.3 m)** — the two reds the P4.D161 pause-toast beats, the documented full-suite intermittent (green alone); the suite grew 288 → 297 with the six gates' flips, every activated beat green after its first-run gesture fixes. Versions: core 0.0.834, harness 0.0.724, host
  0.0.113, web 0.0.127, SPA 0.5.673; cli/tauri unchanged. 💸 the dogfood
  queue gains the whole subprompts feature on real data and every generator
  runner with real spend — **the owed dogfood pass is the top next
  candidate.** Round record: `status-log.md`.
- **The `p4.9k` + `2f4254b42` dogfood pass RAN (2026-09-07, agent-driven, on
  the Friday copy) — 24 rows, 22 PASS, 1 PARTIAL, ZERO v5 defects, and both
  rounds' whole 💸 queue discharged.** Walk doc:
  `dogfood-walks/2026-09-07-subprompts-generators-pass.md`; record in
  `status-log.md`. The ledger's §2 probe passed at walk start (v4 AT the
  baseline, §3 empty), so no step could blame drift. **The pre-walk
  measurement bought the three headline proofs:** v4 had shipped subprompts
  that morning and RUN the feature on the live instance before the copy was
  taken, so v5 was judged against v4's own bytes — it reads
  `Subprompts/initiating.md` byte-for-byte, **keeps the
  `selectedSubpromptIds` the pre-unit-1 code dropped** through a foreign
  participant write, and ⭐ **rebuilds the whole compiled identity stack
  byte-identical to v4's** (md5 `b9beceb0…`, 10,133 chars, envelope version
  2) after a selection toggle — one gesture proving the render, the compiler
  bake, the envelope and both cache transitions. Also byte-checked against
  v4's own artefacts: the green-room `subpromptsNote` (246 chars, IDENTICAL,
  with the fifth bullet in all four v4 consults and v5's) and the optimizer's
  suggestions-file frontmatter/prose (v4 wrote two in April). **Every
  generator runner ran live:** ⭐ a dry-run rename scanned **6,858,481
  occurrences in 1.65 s while five writes landed during it** (the §3 unify
  read-pool fix at real scale); an execute with Additional Replacements + the
  per-pair case flag behind v4's exact confirm; refresh-archive both arms (no
  UI exists); an external prompt with the optional Scenario and a lowered
  cap; ⭐ the optimizer's **vault-suggestions arm, which no spec covers**;
  the Wizard with Skip-physical + Select-all + Background Context; and Summon
  From Lore from the toolbar mount the spec never walks — with the recorded
  `VALIDATION_UNAVAILABLE` divergence surfacing **honestly** as a
  non-critical Review note. **B8 complements K1/K2's OPEN Tier-2 item 9:**
  the raw SSE headers and `data: {json}\n\n` framing match v4's handler
  exactly — the wire is right, only the assertions stop short. Also live:
  `chatDelete` from the trash button that sits INSIDE the card link (the
  interception fix), the P4.81 Zod-`details` host wire, the composer's wide
  predicate on a purpose-built two-user-seat chat, and
  `[TurnOrchestrator] Chain error, stopping` with its safety pause (plus a
  free P4.D135 failover proof). **Five apparent divergences were run to
  ground and every one is v4-faithful** — incl. the unattributed greeting
  `llm_logs` rows (all 19 of v4's own nulls are greeting calls) and the
  orphaned `conversation_chunks` row (v4 left four). Six instrument notes
  banked. **Still owed (human):** the Brahma deep-query budget,
  dedup/summaries, and #101.
- **The generator follow-ups + prompt-templates round (P4.82 ∥ P4.83 ∥
  P4.84 ∥ P4.85 ∥ P4.86): UNIFIED on main (2026-09-07) — ALL FIVE CLOSED;
  the baseline STAYS `2f4254b42`, the ledger's §3 stays EMPTY, and P4.9K1 /
  P4.9K2 close WHOLE with it.** The first non-drift round since the
  follow-ups round 2: the `CHARACTER_HEADSHOULDERS_BACKFILL` handler + boot
  scan (the last named-refusal job type — v4's `Failed to select cheap LLM`
  catch proven DEAD; the scan's `instance_settings` flag is SHARED with v4,
  so a v5 boot on Friday scans nothing) ∥ prompt templates whole (the
  vendored 21-prompt catalogue + guard — v5 NEVER had the seeding; the
  `d123658d`-round `9c01fa99` ratification premise corrected; the live beat
  found v4's lazy `ensureCollection`) ∥ the SPA follow-ups (the review
  pane's renders on a bare CommonMark pipeline; two K4 rows are v4 BUGS —
  the `imported` count compares an object `> 1`, the apply banner reads a
  stale closure) ∥ the Rust follow-ups + K1/K2's items 9/10 for three
  families (**the oracles' "`logLLMCall` stays REAL" premise was FALSE for
  every jest run** — `jest.setup.ts:379` no-ops the module) ∥ the AI-import
  validation engine (`jsonschema` 0.55 under the human's written ruling;
  `VALIDATION_UNAVAILABLE` DELETED; the truthy-non-array arms reproduced).
  **The §3 review: NO blocking findings; six should-fixes fixed at
  unification** — headline: the prompt-templates CREATE path lacked the
  table ensure its reads had (a first-ever POST 500'd), the optimizer's
  stream-end arm was gated on `loading` where v4's is unconditional, and
  P4.86's `llmLogCalls` v5 leg never observed the port. Gate: 11/11
  families fresh zero SKIP; 537 test binaries / 3,018 / 0;
  clippy both feature sets; release build; ng 409 / 6,484; full Playwright
  300 passed / 0 failed / 0 skipped (7.6 m). Versions: core 0.0.846, harness 0.0.735, web 0.0.131, host
  0.0.114, SPA 0.5.681. 💸 **the round's whole queue was DISCHARGED by the
  2026-09-07/08 pass** (below). Round record: `status-log.md`.
- **The generator follow-ups + prompt-templates dogfood pass RAN
  (2026-09-07/08, agent-driven, on the Friday copy) — 20 rows, 16 PASS, 2
  BLOCKED-by-design, ZERO v5 defects, and the round's whole 💸 queue
  discharged.** Walk doc:
  `dogfood-walks/2026-09-07-generator-followups-prompt-templates-pass.md`;
  record in `status-log.md`. The ledger's §2 probe passed at walk start (v4
  HEAD **is** the baseline, §3 EMPTY), so no step could blame drift; five
  boots on the real 800 MB instance, **zero panics and zero `ERROR` lines**.
  **The pre-walk measurement bought the pass its two best proofs and then
  corrected itself.** v4 had left a **real DEAD backfill job** here
  (`baef7a26-…`, Charlie, 3/3 attempts, `No response from model`,
  2026-06-13) — so after clearing that character's head-and-shoulders through
  the Appearance tab, **v5 finished the work v4 gave up on**: 212 chars
  written and `short`/`medium`/`long` surviving **md5-identical**, which is
  the whole-merged-object rule proven on v4's own leftovers; a second reset
  then took **4 ms** with zero handler sentences and `llm_logs` unmoved
  (against 3,500 ms), which is the idempotence gate. The cross-app leg holds
  (v4's flag → v5 scans, logs and writes nothing), and the positive leg
  **refuted my own §0.5 prediction of 0 enqueues**: `scanned=45 enqueued=2
  skipped=43`, because `hasSeed` also reads `fullDescription`, which lives
  outside the `physical-prompts.json` I had counted. Both jobs then ran —
  Devin populated, the archived Tuman refused with v4's byte-identical
  sentence (v4 would enqueue him too). **Prompt templates gave the round's
  cleanest cross-app result:** the modal lists all **27** of v4's built-ins
  across **three seeding vintages**, the count never moved, and v4's `GEMINI
  Companion` stayed at 1,986 chars against v5's vendored 2,906 — then
  deleting one row proved that negative non-vacuous, emitting **exactly one**
  seed line and inserting **v5's** 2,372 chars. `MODERN General` is a
  **three-way byte agreement** (v4's row, v5's catalogue, Charlie's vault —
  3,547, md5 `6ca2b46521e4`). Also live: the review pane's three renders on a
  real twelve-field generation with v4's exact markup; `runTemplateSave`
  proven by a counting discriminator (`{{char}} → Charlie` 16 → 19);
  `[Chats v1] Impersonation stopped` with v4's three-field bag; all four
  route-level starting lines plus the **`EXTERNAL_PROMPT`** and
  **`CHARACTER_OPTIMIZER`** `llm_logs` types the lane measured; and the real
  `jsonschema` engine answering **`Validation passed`** on a real export with
  `VALIDATION_UNAVAILABLE` nowhere. ⭐ **`?action=generate`'s code-point gate
  discriminated at zero spend** — 4000 astral code points (8000 UTF-16 units)
  passed and died at `No API key configured` while 4001 answered `Validation
  error`. ⭐ A free confirmation rode along: the truthy-non-array
  `sourceFileIds: "not-an-array"` logged `sourceFileCount: 12` — the JS
  `.length` of that string, exactly what v4 logs. **Two arms are BLOCKED by
  the port's own design, both MEASURED not assumed:** the AI-import repair
  loop (the assembler normalizes before validation — three injected
  corruptions all validated clean) and the two continue-mode toasts (their
  own buttons are disabled by the predicate the toasts guard). 💸 still owed
  (human): the Brahma deep query, dedup/summaries, #101, and the re-measured
  90 s/120 s compression row.
- **The `25f534c0b` progressions + bug-126 drift catch-up round (P4.D166 ∥
  P4.D167 → {P4.D168 ∥ P4.D169} ∥ P4.D170): UNIFIED on main (2026-09-09) —
  ALL FIVE CLOSED; the oracle baseline MOVES to `25f534c0b`; ⚠ v4 landed
  NINE commits DURING the round (recorded by the unification's opening
  `/driftcheck`, all UNPROCESSED — the regen rule stays PIN REQUIRED and
  `qtap_schema_embed_guard` is RED against the live checkout until the
  route-trail port re-vendors the export schema).** Bug 126 whole (ownership
  as a PID + `startedAt` snapshot KEYED BY LOCK PATH — v4's one global made
  correct for a host holding several; heartbeat freshness for EVERY
  environment closing v5's own fail-open claim; the renamed-process
  release; **the loss teardown made ORDERED — v5's default arm had exited
  without ever stopping the PTY children**; the CLI's shared `assess_lock`,
  Tier R 216 → 223/0; v4's `lock-helpers.js` UNTOUCHED, so the write lock
  and the launcher's classifier keep the hostname comparison by design —
  and the SPA's lock-conflict screen now gets a 503 where it wants 409, a
  named candidate) ∥ the progressions engine tier-1 exact over a committed
  555-row corpus (the order's U+202F prediction REFUTED on Node 24 / ICU
  78; `toFixed` half-up pinned) ∥ the prompt path (the chokepoint, the ONE
  memoised cadence read shared with the Core whisper, the trailing section
  after Suparṇā's mail and before the turn-skip note, the forced greeting +
  Carina reports, the negative cache guarantee with both version constants
  UNMOVED, `help/` 121 → 122) ∥ the Pascal `progress` family end to end (the
  read subject, `{{now}}`, the effect target with create-on-write /
  normalise / post-validation rollback, one clock per run at all four
  entrances, the NEW `pascal_side_effects_equivalence` family, six corpora
  widened from ZERO progress coverage with executable floors, the committed
  run-custom pair rebuilt) ∥ the SPA half whole (the client-safe twins over
  an extracted Zod shim, the Progressions card + editor modal, the Workbench
  affordances, the run popup, BOTH `public/schemas/` vendors GUARDED —
  hazard 10 discharged — and both gated beats flipped LIVE at unification).
  **The §3 review (four parallel readers + the unifier's reads): NO blocking
  findings; the should-fixes landed on the unify branch** — headline: the
  `endTime` refine ran after an aborting Zod issue (measured at the pin, six
  rows added, red-first), `IndexMap::remove` SWAP-reordering keys that reach
  disk at three applier sites (order-blind on every family), the CLI's
  `Math.round` twin at five sites, a 2026-10-28 wall-clock time bomb in the
  roster test, the SPA shim's `received number`, and — at the wire — the
  SPA twin FAILING nine of the Pascal lane's rows when its consumer corpus
  was re-recorded (the cross-lane blind spot), plus four gesture defects in
  the activated beats' first runs. Gate: 541 test binaries / 3,065 / 0 / 1 ignored with the 38-variable env block, zero SKIP; the 24-family sweep 24/24 fresh from the pin (+ the engine family re-regenerated after the refine fix); Tier R 223 cases inside; clippy both feature sets; release build; ng 417 files / 6,887; full Playwright 302 / 3 / 0 (the three reds the documented P4.D161 / P4.d17 intermittents, 3/3 green by file in isolation; suite 300 → 305). Versions: core 0.0.857, harness 0.0.749, host 0.0.117, cli 0.0.20, web 0.0.132, SPA 0.5.690; tauri unchanged. 💸 the dogfood queue gains the
  whole feature on the Friday copy + a live hostname flip (human). Round
  record: `status-log.md`.
- **The `78b381a96` twelve-commit drift catch-up round (P4.D171 → {P4.D172
  ∥ P4.D173} ∥ P4.D174 ∥ P4.D175 ∥ P4.D176 ∥ P4.D177): UNIFIED on main
  (2026-09-10) — ALL SEVEN CLOSED; the oracle baseline MOVES to `78b381a96`
  and the twelve-row drift debt is CLEARED** (the mid-round `cc65d6bfc`, bug
  133, stays UNPROCESSED — the next round's one row; PIN REQUIRED until it
  lands). Absorbed whole: the two schema moves as ONE D23 re-dump + both boot
  ensures (P4.D171 — the substrate lane the stacked pair built on; its survey
  correction STRUCK §C.2's chat-GET key: v4 never projects
  `cycleOrderParticipantIds`, pinned both directions); the turn manager from
  the TIP (P4.D172 — the ordered `DrawSource`, the six selection sites over
  the whole-room batched map [bug 131, v5 measurably had it], the strike,
  `?action=turn`'s `state.cycleOrder`, the alias census made executable);
  the message route trail (P4.D173 — the ONE recording chokepoint, twelve
  record sites + the three empty-response arms v5 lacked, persistence + the
  `done` frame, a 64-row tier-1 compose family, the stacked lanes' fixture-
  at-tip/oracle-at-`5841a8c62` split re-regenerated whole at unification);
  the Salon chat gallery server half (P4.D174 — the nine-source enumerator
  over a new committed `chat-gallery-{main,mount}.db`, the two dispatch-only
  verbs, `?download=1` on all three byte routes, bug 130); bug 128's
  `memories` topic + bug 132's writers/ladder/boot heal + `help/**` at 123
  byte-identical + six NO-PORT ratifications (P4.D175); the gallery SPA
  (P4.D176) and the Salon smalls SPA — the route-trail badge NET-NEW under
  the avatar, the drawn rotation in the participants list, the `memories`
  topic (P4.D177). **The §3 unification review (seven parallel readers, the
  verdict owned at the unify) caught SIX blocking findings; THREE would have
  shipped, all fixed red-first on the unify branch:** the finalizer seeded
  `loadRoomCharacters` with `{id, name}` where v4 seeds the WHOLE responding
  record and the preloaded copy WINS — so the character who just spoke drew
  at the 0.5 default on every assistant turn, the primary production draw
  site; the gallery 409's `relativePath`/`keptAt` rode `CoreError::details`,
  which every transport renders NESTED where v4 answers four FLAT siblings —
  and the family's own local renderer had flattened `details`, a shape no
  transport had (the Taboo §3 class again); the SPA's turn effect re-seeded
  the rotation from a chat-GET key neither app sends, on every send and
  refetch, wiping the rotation the turn response had just set (activation
  would have caught nothing — every beat asserts the post-turn state); and
  a FOURTH, caught by the activated route-trail beat's first live run: the
  orchestrator finalized from a pre-failover profile local, so after any
  understudy recovery the row and the trail's answering entry named the seat
  that fell over (pre-existing since P4.D135; v4 reads the streaming state's
  copy — now re-read at both recovery sites). Also
  fixed: the detail view's double delete confirm, the silent gallery-door
  save, the undated/mis-gated ALREADY_SAVED sentence, the bare `Gallery`
  label (v4 always numbers), the `[EmptyResponse] … retrying same provider`
  warn the port dropped, the heal's usability probe that failed the BOOT
  where v4 degrades, and a dozen smalls. Gate + versions: the round record
  in `status-log.md` (core 0.0.874, harness 0.0.766, host 0.0.123, web
  0.0.136, SPA 0.5.695). **Next: the bug-133 catch-up, then the round's
  named OPEN items, then the owed dogfood pass** — `phase-4.md`.
- **The `cc65d6bfc` bug-133 catch-up + `78b381a96`-round remainders round
  (P4.D178 ∥ P4.87 ∥ P4.88): UNIFIED on main (2026-09-10) — ALL THREE
  CLOSED (P4.87 with two coverage items named OPEN); the oracle baseline
  MOVES to `cc65d6bfc` and the ledger's §3 is EMPTY.** Bug 133 absorbed
  whole: the sanitizer's fourth parameter re-meant as "does THIS scene
  route uncensored" (story backgrounds pass `uncensored_image_target`, the
  image tool `AUTO_ROUTE && <profile set>` — a unit pin plus a DETECT_ONLY
  corpus case, the tool path having been corpus-BLIND), the story reroute
  barred for a moderated chat with the candid re-craft and v5's
  `RerouteRecraft` seam DELETED (v4 re-crafts nowhere at the tip), **the six
  reroute-path log lines both handlers were missing** (v5's whole reroute
  path was silent — the #103/#110 class; the order's avatar-bag survey
  claim REFUTED by the lane and pinned as an absence), the story corpus's
  two moderated reroute rows RED-FIRST + two arms, the NEW
  `appearance_sanitize_gate_tier3_equivalence` family over v4's REAL
  sanitizer (nothing drove it), the help page re-vendored ∥ the previous
  round's OPEN remainders: per-case draw arrays for the two frozen-zero
  tier-3 families (and ONE shared `DrawSource` per case — v4 pins one
  cursor), the different-provider understudy + concierge-seeded trail arms,
  the three `[Failover]` bags as exact key sets, the two classify-before-
  reset tests, the NEW `chat_continuation_tier2` family ∥ the seven
  `[MemoryGate]`/`[Memories API]` lines, ONE home for the migrations-ledger
  tables (the guard drop measured v5-only), the P4.D171 plants through
  export/import + the `marshal_row` census (100 columns, not 98) + the
  ensure sites folded, the gallery's six-site `safeQuery` degrade with
  plant-probe arms — and P4.88's escalation landed at unification (the
  gallery route's failed chat read answers v4's 404, not a 500). **§3
  review: NO blocking code finding**; the lane's methodological find — a
  guard whose other conjuncts are false in every test arm is not tested at
  all — plus the review's own shapes (a recomputed log bag, presence-only
  pins, a v4-impossible dropped-table scenario, UTF-16 vs scalars) are in
  the round record and the memory note. Gate: 19/19 families fresh from
  one `cc65d6bfc` pin zero SKIP; 552 test binaries / 3,172 / 0; clippy both
  feature sets; release build; SPA 310 passed / 2 failed (the documented P4.D161 intermittent, 2/2 green in isolation) / 1 skipped. Versions: core 0.0.886,
  harness 0.0.775, host 0.0.125. **The owed dogfood pass is the top next
  candidate** — see phase-4.md. Round record: `status-log.md`.
- **The `f4ad2c8d1` In-Their-Own-Words drift catch-up round (P4.D179 ∥
  P4.D180 ∥ P4.D181): UNIFIED on main (2026-09-11) — ALL THREE CLOSED; the
  oracle baseline MOVES to `f4ad2c8d1` and the drift debt is CLEARED.** v4's
  impersonated-line voice rewrite + bug 134 absorbed whole. Server: the
  `chat_settings."impersonationVoiceRewrite"` column through the D23
  re-dump (seed 34 → 35) + a boot ensure + the route arm with v4's sentence
  (present-but-null 400s, pinned at the wire); the `VOICE_REWRITE` log type
  + both `map_task_type_to_log_type` arms — **v5 had filed
  `announcement-rewrite` as `SUMMARIZATION` since it was ported**, pinned
  by a NEW 31-row tier-1 family censusing v5's own match arms; the Almanack
  row; `help/**` 123 → 124. The rehearsal whole: `voice_rewrite_core`
  extracted with the announcer family byte-identical at BOTH pins (v4's
  refactor proven neutral on v4's own code), the new `in_scene_voiced`
  service (the filter order whisper → history → presence from the FULL
  event list; UTF-16 `max_tokens`; the stored compiled stack read by the
  rehearsal while the orchestrator's turn-time deferral stays), the
  `chatImpersonationVoicePreview` verb in v4's MEASURED ladder order, the
  LIVE host wire (💸), a NEW committed pair + 27-case tier-3 family whose
  first run caught a real defect (the `systemSender` played filter tested a
  TEXT column as a bool), the wire test proving the driver WIRED + the
  `llm_logs` row. SPA: the gate with v4's twelve cases, a client-safe
  `carina-parser.ts` over a recorded corpus, **the composer-clear
  restructure** (the composer stops clearing on emit; the Salon's one door
  clears on send), the dialog string for string + the shared review panel,
  the cue, the Composer-card toggle, the label sites, and **bug 134 measured
  then ported** (v5 never had the mount-only snapshot; the live-read facts
  pinned structurally; v4's memory-cascade "Remember this choice" arm —
  which v5 had NEVER carried — landed with its invalidation). **The §3
  review found no blocking code in any lane; its headline: the in-scene
  service's claim that the formatter's `name` reaches the wire was FALSE on
  name-supporting providers** — v5's `CompletionMessage` carries no `name`,
  the same shape as the whole turn path, now RECORDED both directions as
  `[CHEAP_LLM_NAME_FIELD_GAP]` (136 v4 rows, 0 v5) with the turn-path
  measurement a named candidate; also two silently-skipping pins un-guarded,
  the remember-arm wiring pinned, six comment corrections. **The unifier's
  root fix:** four families (`post_office_routes` + three help-chat) were
  RED on main behind oracle vars no gate set — v4's own jest side recorded
  `no such column` on the pre-`78b381a96` committed pairs — widened through
  v4's migration SQL (the P4.52 script + the two P4.D171 rows). The
  activated beats' first execution caught three gesture defects, no product
  defect (an inline-host locator, a class-vs-element selector, the shared
  mock answering a non-streaming call with SSE). Gate: 14/14 families fresh
  at the pin zero SKIP; 556 test binaries / 3,192 / 0; clippy both feature
  sets; release build; ng 431 files / 7,193; full Playwright 312 passed / 3 failed / 1 skipped over four runs (the three reds the documented P4.D161/P4.d17 intermittents, green in run 3 and by file; every round beat green — see the round record for what the activated beats' first runs found).
  Versions: core 0.0.894, harness 0.0.783, host 0.0.129, web 0.0.142, SPA 0.5.707; cli/tauri unchanged. **Next: the owed dogfood pass, then the
  `name`-field turn-path measurement** — see phase-4.md. Round record:
  `status-log.md`.
- **The `31436bae4` drift catch-up round (P4.D182 → {P4.D183 ∥ P4.D184 ∥
  P4.D185} ∥ P4.D186 ∥ P4.D187 ∥ P4.D188): UNIFIED on main (2026-09-15) —
  ALL SEVEN CLOSED; the oracle baseline MOVES to `31436bae4`; the ledger's
  eight rows are ABSORBED / NO-PORT-RATIFIED, and FOUR rows v4 landed after
  the round was ordered stay UNPROCESSED (bug 141 PORT, the ABI heal NO-PORT?,
  the bug-142 filing NO-PORT?, the bug-142 fix CONVERGENCE onto P4.D183's own
  `DELETE_MISS_DIVERGENCE` pin — trips at the next move) — PIN REQUIRED at
  `31436bae4`.** Absorbed whole: the substrate (`files.generationKey` via the
  D23 re-dump + column/index ensure through every read/write/export/import
  surface; `chats.transcriptVersion` as a boot ensure ONLY — outside v4's Zod
  chat schema by design — with its negatives pinned; the export-schema +
  eight-file `help/**` re-vendor); the Salon transcript as a subscribed read
  (the funnel's ONE announce point at v4's six conditions incl. the measured
  search-replace guard, the projection extraction proven byte-neutral at both
  pins, the `chatTranscript` verb + `/api/v1/messages` GET edge, the chat-GET
  key; the SPA's `reconcileTranscript` over a 38-vector corpus from v4's real
  module, the cheap conditional read on `chats` hints, the optimistic bubble
  back INSIDE the array — **dogfood #106's separate-signal fix retired for
  v4's shape**); the avatar configuration cache (the key chokepoint over a
  31-shape tier-1 corpus with the UTF-16 key sort measured, lookup-before-
  spend, `force` from the manual regenerate only, vault-always with the legacy
  `folders` mint deleted, the collapse as a boot heal under v4's own ledger id
  over a 17-scenario tier-2 family — a no-op on the already-collapsed Friday);
  Avatar Rolls (the album predicate replacing both hand-rolled copies, the
  rolls service with set-as-avatar via the album LINK and delete via the
  roll's OWN link, three verbs + two REST sub-routes over a new committed
  pair; the gallery-tab section + "Show shared"); the paused-chat hold (the
  predicate consulted ONCE at the record → prepare-turn seam, the three
  `!hold` conjuncts each pinned with the others OPEN by a baseline-vs-target
  v4 measurement, `heldUserTurn` on the frame; the client's once-per-pause
  notice, Nudge/Skip leaving the pause standing, bug 139's resume-then-ask).
  **The §3 review (seven parallel readers) caught FIVE blocking findings,
  THREE would have shipped:** a `Map::remove` swap-remove reaching
  `chats.characterAvatars` (hidden by one-key fixtures + a key-sorting
  normalizer — the pin needed FOUR seats), the Salon's turn tail seeding off
  the query's last good data so a FAILED turn-boundary read let the sweep
  delete the operator's own line, and the transcript incident beat posting a
  verb that exists on neither side (activation would have parked it forever —
  re-gestured onto `messageEdit` through the funnel); plus five new log lines
  with no capture pin and a source census that scanned a sixth of a file.
  Two of the seven mutation proofs SURVIVED as first written (a three-seat map;
  a test filter that ran zero tests) and were repaired — recorded. Gate: the
  46-family sweep from one pin; fmt + clippy both sets; release build; 569
  test binaries / 3,271 / 0 / 2 ignored, zero SKIP; ng 434 files / 7,325;
  full Playwright 318 passed / 1 failed / 6 skipped (the red the documented quill intermittent, green in isolation; the skips the five named parks + the standing gallery park). Versions: core 0.0.913, harness
  0.0.805, host 0.0.134, web 0.0.146, SPA 0.5.721. 💸 the dogfood queue gains
  the whole round on the Friday copy (the cache's first HIT — measure the
  collapsed population first). Two P4.D187 beats stay parked on the named
  shared-fixture title-checkpoint hazard (phase-4.md). Round record:
  `status-log.md`.

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


## Superseded baseline paragraph (replaced at the 979652a9-round unification, 2026-08-18)

- **Oracle baseline: `d123658d` (2026-08-17, v4 main — "fix:
  connection-profile editor bugs 72, 73 and 74"), adopted at the
  d123658d-round unification; the drift debt is CLEARED at the pin, and
  `9c01fa99` (sample-prompt content) is dispositioned NO-PORT — a drift
  check landing on it alone owes nothing.** Pin a detached worktree at
  `d123658d` for every regen whenever the v4 checkout isn't cleanly on it
  (`recipe_sweep.py --v4 <pin-path>`; ALL THREE symlink classes: root
  node_modules, `packages/quilltap/node_modules`, the
  `plugins/dist/*/node_modules` dirs). [...] (tail unchanged — see the live
  paragraph in CLAUDE.md, which carries the same standing rules.)

## Superseded baseline paragraph — `979652a9` (replaced 2026-08-19 at the c6ff8051-round unification)

- **Oracle baseline: `979652a9` (2026-08-18, v4 main — "feat(workspace):
  refresh a tab's data when it is navigated back to"), adopted at the
  979652a9-round unification; the drift debt is CLEARED at the pin, and
  the `bugfix` branch's only unabsorbed content is the test-only
  `009c49b2` deflake (NO-PORT).** Pin a detached worktree at
  `979652a9` for every regen whenever the v4 checkout isn't cleanly on it
  (`recipe_sweep.py --v4 <pin-path>`; ALL THREE symlink classes: root
  node_modules, `packages/quilltap/node_modules`, the
  `plugins/dist/*/node_modules` dirs). **Drift-check BOTH branches every
  round** (`git log <baseline>..main` AND `git diff main bugfix -- lib/
  app/ packages/` — measure bugfix with `diff`, never the commit list).
  The sweep driver remains the sanctioned per-family regen path — never
  run two sweeps concurrently. The distill-transitive TZ pins, the
  committed-fixture rule, and the venue/staging rules stand unchanged.

## Superseded baseline paragraph (replaced at the b8449b3e-round unification, 2026-08-21)

- **Oracle baseline: `c8a3cf77` (2026-08-20, v4 main — the version bump
  atop `870a57fa`, "Per-turn conversation summaries with embedded vector
  reuse (#38)"), adopted at the c8a3cf77-round unification.** ⚠ **v4 is
  ALREADY PAST the baseline** (`e22f7b36`, "feat(salon): anti-chorus
  discipline for multi-character scenes" — the next round's drift, and the
  TOP next candidate): pin a detached worktree at `c8a3cf77` for EVERY
  regen until that catch-up runs
  (`recipe_sweep.py --v4 <pin-path>`; ALL THREE symlink classes: root
  node_modules, `packages/quilltap/node_modules`, the
  `plugins/dist/*/node_modules` dirs). **Drift-check BOTH development
  branches every round** (`git log <baseline>..main` AND `git diff main
  bugfix -- lib/ app/ packages/` — measure bugfix with `diff`, never the
  commit list; `release` is release-history only, but note WHICH branch
  the checkout occupies before any regen). The sweep driver remains the
  sanctioned per-family regen path — never run two sweeps concurrently.
  The distill-transitive TZ pins, the committed-fixture rule, and the
  venue/staging rules stand unchanged. (The superseded baseline paragraphs
  formerly kept here "for history" are archived verbatim in
  `docs/developer/porting/claude-md-status-history.md`.)

## Superseded baseline paragraph (removed from CLAUDE.md 2026-08-22, at the 4cb1035e-round unification)

- **Oracle baseline: `b8449b3e` (2026-08-20, v4 main — "fix(tests):
  disable V8 Sparkplug for jest", atop `e22f7b36`), adopted at the
  b8449b3e-round unification (2026-08-21).** v4 had NOT moved past it at
  unification — verify before the next round starts. (The 12fe3e6f round,
  unified 2026-08-22, moved the baseline in the status log and phase plan
  but never updated this bullet — corrected at the 4cb1035e round.)

## The 4cb1035e baseline paragraph (superseded at the a6870c5a round, 2026-08-22)

- **Oracle baseline: `4cb1035e` (2026-08-22, v4 main — "fix(nanogpt):
  suppress the gateway's reasoning echo (plugin 1.0.2, bug 87)"),
  adopted at the 4cb1035e-round unification (2026-08-22).** ⚠ v4 HAD
  ALREADY MOVED past it at unification — the prompts trio (`8f868109`
  project/group standing instructions in the cacheable system prompt,
  `346e855f` second-person tool reinforcement [bug 88], `a6870c5a`
  grammatical-person consistency), on ported prompt surfaces — **pin a
  detached worktree at `4cb1035e` for EVERY regen until that catch-up
  runs; it is the top next candidate.** (The drift-check/pin/sweep-driver
  boilerplate continued as in the current paragraph.)

## Baseline paragraph superseded at the f8973813-round unification (2026-08-22)

- **Oracle baseline: `a6870c5a` (2026-08-22, v4 main — "feat(prompts):
  grammatical-person consistency in assembled prompts"), adopted at the
  a6870c5a-round unification (2026-08-22).** v4 had NOT moved past it at
  unification (verified immediately before the unified regen). (The
  drift-check/pin/sweep-driver boilerplate continued as in the current
  paragraph.)
