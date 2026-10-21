# Survey — bug 255 (Ariel reads full-screen programs, `a6292f169`, THE FIFTEENTH D23 MOVE + a terminal-emulator crate) and the four smalls (`1d0a31132` scenario sort, `406dbdd5a` the bug-133 re-ruling, `680b71044` `/photos` server half, `7263105c6`'s bug-256 half, `5a8338b30` the toast z-index)

Read-only planning survey, **2026-10-20**. v4 = `~/source/quilltap-server`,
read ONLY through `git show 322a5488e:<path>` (**THE PIN** = v4 `main`
`322a5488e7bd93f0234a25b42f87d5418dbf4c2c`, `4.11.0-dev.53`, 2026-10-20) and
`git show <sha> -- <paths>`. The tree is AT the pin, so every v4 line number
below is at `322a5488e`. **The oracle baseline v5 has ported is `e8e17b2d9`**;
v5 = `main` at **`5a94a9d1e`** (`/Volumes/External 1/source_overflow/quilltap-v5`).
Every v5 line number is at `5a94a9d1e`. Nothing is taken from commit prose
(ledger §5.3) — every claim is from a shipped hunk or a file at the pin. The
ledger rows `1d0a31132` (`drift-ledger.md:404`), `a6292f169` (`:406`),
`7263105c6` (`:407`), `406dbdd5a` (`:408`), `680b71044` (`:410`), `5a8338b30`
(`:415`) and the §1 "PENDING — the FIFTEENTH move" note (`:251-268`) were read
first and are built on; where a hunk disagrees with a row this survey says so
(§8).

## Headline findings (written last)

1. **The FIFTEENTH move renders `"cols" REAL` / `"rows" REAL` on a fresh v4
   instance, NOT `INTEGER DEFAULT NULL`** — v4's `schema-translator.ts:320-328`
   maps a `z.number().int()` with no `.min()`/`.max()` to `REAL` (the committed
   `fresh_schema.json:32` already shows `"exitCode" REAL` for the identical Zod
   shape). The migration appends `INTEGER DEFAULT NULL`. Two shapes, both
   carried — the P4.D249 `chat_informs.permanent` precedent — and the ledger
   row + `DDL.md` + the §1 note all say INTEGER: VERIFY at the re-dump (§3, §8).
2. **The crate ruling: `vt100` 0.18.1** (MIT, MSRV 1.85, deps `vte` 0.15 +
   `unicode-width` 0.2 + `itoa`; `cargo info vt100` reaches the registry here).
   It reproduces every xterm-headless behaviour `screen.ts` relies on
   (alt-screen `?1049` with save/restore + clear, absolute/relative cursor moves,
   `CUF` in place of spaces, `EL`/`ED`, per-row wrap flags, a capped
   scrollback) except xterm's REFLOW on resize and the parse-settle
   asynchrony (which v5 does not need). `alacritty_terminal` drags a PTY/event
   loop + `polling`/`rustix`/`signal-hook`; `termwiz` is NOT a screen model
   (and `wezterm-term`, which is, is NOT on crates.io — `cargo info` fails);
   `vte` alone is a parser. Recommendation §1.11.
3. **Runtime breaks at the pin the orders must own:** (a) the oracle case
   `harness/oracle/cases/appearance-sanitize-gate.test.ts:283-291` calls v4's
   `sanitizeAppearancesIfNeeded` with the OLD eight positional arguments — at
   the pin the third is `imageTargetUncensored` and the fourth
   `cheapLLMSelection`, so the case hands a boolean where a selection object is
   read (§4.3); (b) `harness/oracle/cases/doc-edit-path-resolver.ts:201-235`
   passes `characterId` / `buildDocStoreUriResolver(A)` to producers that no
   longer take them — runs, but the `uri-self` / `res-self` rows now come back
   NAMED and the family's equality pins trip (§4.5); (c) every v4 fixture
   builder that creates `terminal_sessions` rows through the real repository
   (`build-terminal-sessions-fixture.ts`, `build-terminal-tools-fixture.ts`,
   `build-ariel-writers-fixture.ts`) now bakes an 13-column table — the
   committed tier-2 corpora's `terminal_sessions` dumps grow two cells (§4.1).
4. **The bug-133 "both-direction pins" are not named carves:** the harness
   holds NO `BUG_133`-style carve (grep §2.3). What trips is the 30-case
   equality grid `appearance_sanitize_gate_tier3_equivalence` whose INPUT shape
   (two booleans) no longer exists at the pin, plus the `story_background_job`
   / `image_generation` tier-3 families' `uncensored_image_target` rows, and
   `tool_routes_dangerous_to_uncensored` (`generate_image.rs:2804`) whose
   predicate v4 deleted. The lane retires by re-shaping the grid to v4's new
   third parameter, not by deleting pins (§4.3, §5.4).
5. **v5 HAS bug 256 at eleven producer call sites** (`uri_producers.rs:22-39,
   100-123`, `librarian_notifications.rs:107-121`, `self_inventory.rs:945,
   1941-1953, 2440`, `document_ui.rs:294, 359`, `path_resolver.rs:591, 713,
   1101`) and v4 KEEPS `formatSelfUri` at exactly two owner-only sites
   (`knowledge-injector.ts:372`, `prospero-notifications/writer.ts:589`) — v5's
   `knowledge_injector.rs:482` / `prospero_notifications.rs:464` stay. Pins
   that trip: `doc_edit_path_resolver_equivalence` (`uri-self`, `res-self`),
   `doc_enum_equivalence` (every `qtap://self/…` uri), `doc_text_equivalence`
   (one `uri`), `post_office_librarian_equivalence` (`ds-self` rows ×3) (§2.5).
6. **v5 HAS the `/photos` defect in both directions:**
   `user_gallery_service.rs:128-145` fans over every enabled store with no
   vault filter and `:395-408` deletes a vault link v4 now refuses. v5 ALSO
   lacks `?action=albums` and `?album=` entirely — `photos_routes.rs:70-88`
   builds `PhotoGalleryList{q,tag,limit,offset}` and `api/photos.rs:115` has no
   action arm; `listAllPhotoAlbumOptions`'s twin EXISTS (`photo_album_options.rs`,
   P4.D275) so `albums` is a one-call wire (§2.4).
7. **The scenario sort is Tier 1 + three stateful re-sorts:** v4's
   `compareScenarios` (`lib/scenarios/sort.ts:41-48`) is `localeCompare(…,
   {sensitivity:'base'})` on the stripped title, then the stripped filename,
   then the RAW title at `base`, then `path` with DEFAULT options — v5 has both
   collators (`collation.rs:51, 57`). v5 sorts by PATH at `db/scenarios.rs:232`
   and `vault_read_overlay.rs:402`; the `readCharacterVaultScenarios` twin is
   the same overlay fold. The SPA's `scenario-select.ts` renders the wire order
   with NO local sort (v4's `ScenarioSelect.tsx:79-100` sorts every optgroup
   locally) (§2.2).
8. **The toast is a one-line SPA port with the tiers confirmed:**
   `_toast.css:35` `z-index: 50`; v5's dialog overlay is `z-[60]`
   (`_surfaces.css:203-206`) and the raised-overlay tokens in use are
   `z-[70]`×5, `z-[80]`×3, `z-[90]`×9, `z-[100]`×19, `z-[101]`×1, `z-[9999]`×2 —
   exactly v4's ladder, so `1000` lands in the same gap (§2.6).
9. **Lane cut:** bug 255 as its OWN lane (`P4.D3xx-A`, gated on the crate
   ruling; owns `crates/quilltap-host/src/terminal/**`, a NEW
   `quilltap-core/src/terminal_screen.rs`, `tools/terminal.rs`,
   `db/terminal_sessions.rs`, `db/terminal_sessions_geometry_repair.rs` (new),
   `ariel_notifications.rs`'s terminal-output heading, `definitions/data.rs`'s
   one `terminal_read` row, `host.rs`'s one ensure block, the web/tauri/SPA
   `PtySessionMeta` fields; bumps core + host + web + tauri + SPA); the four
   smalls as ONE lane (`P4.D3xx-B`; §5 lists the disjoint files; bumps core +
   web + SPA). `fresh_schema.json` is the KEYSTONE sibling's (one re-dump
   carrying the 13th–16th moves); bug 255's lane consumes it.
10. **Rulings owed (§8):** the crate; whether `terminal_read`'s rendered
    `scrollback` is a Tier-1 recorded-stream family (recommended: yes, over
    v4's `screen.test.ts` streams + a Claude-Code-shaped capture) or a
    documented divergence class; the fresh-shape `REAL` vs the ledger's
    `INTEGER`.

---

## 0. The span for this slice, in one paragraph

`a6292f169` (bug 255) replaces escape-code stripping with a server-side
terminal emulator: every PTY session feeds an `@xterm/headless` `Terminal`
(`lib/terminal/screen.ts`, new, 196 lines — `TerminalScreen` with a 5,000-line
scrollback, `render()` → `{buffer:'normal'|'alternate', lines, cols, rows}`,
`takeSummary()` → a screen snapshot while the alternate buffer is active or
the lines since the previous summary marker on the normal one,
`renderTranscript(raw, cols, rows)` for exited sessions); the PTY manager
chains Ariel flushes per session and posts `kind: 'screen'` snapshots under a
new heading; `terminal_read` returns rendered lines (`buffer` in the output,
a `Screen:` sentence in the formatted text, `rawScrollback` as the raw tail's
last 64 KB, the description re-worded) and replays an exited transcript's last
4 MiB at the geometry now stored in `terminal_sessions.cols` / `rows` (the
FIFTEENTH D23 move: two nullable number columns on a generateDDL-rendered
table, plus the column-gated migration `add-terminal-session-geometry-v1`;
NULL reads back as 80×24). `1d0a31132` adds the pure comparator
`lib/scenarios/sort.ts` and applies it in `buildScenarioList` (after
default-conflict resolution, which still walks path order), in the vault
readers through a new `parseScenarioFilesSorted`, and in `ScenarioSelect`'s
optgroups. `406dbdd5a` re-rules bug 133: ONE predicate
`isUncensoredImageProfile(profile, policy)` (`isDangerousCompatible === true`
OR the desk's image profile id); `sanitizeAppearancesIfNeeded` takes
`imageTargetUncensored` in place of `isDangerousChat` +
`routesDangerousToUncensored`, cleans ONLY under a Moderated chat's pre-screen,
and gains a DEBUG line; the pre-flight image reroute stays put on an already
uncensored profile; the tool passes the profile it ended up on; story
backgrounds craft candidly for an uncensored own profile or any Unmoderated
chat and route up front only `routesDirectToDesk`. `680b71044` (server half)
makes `/photos` roll up every enabled NON-character store, adds
`?album=<mountPointId>` (one store, 404 `Album not found` when absent or
disabled) and `GET ?action=albums` (`listAllPhotoAlbumOptions`), and makes
`removeFromUserGallery` / `getUserGalleryEntry` refuse a character-vault link.
`7263105c6`'s bug-256 half removes every `qtap://self` PRODUCER (the three
uri-producers drop `characterId`, the Librarian prints a bare path for a
`self`/missing mount, Document Mode's `chat_documents` row and every
announcement carry the RESOLVED store name through a new
`publishedMountPoint`, self_inventory names the vault and gives a legacy
`self` row no URI, and two resolver error texts change). `5a8338b30` raises
the toast container from `z-index 50` to `1000`.

---

## 1. v4 AT THE PIN — hunk-by-hunk inventory

### 1.1 `lib/terminal/screen.ts` (NEW, 196 lines) — the screen model

- `:24-25` imports `Terminal`, `IBuffer`, `IMarker` from `@xterm/headless`
  (`package.json:79` `"@xterm/headless": "^6.0.0"`; lockfile `:6462-6465`
  resolves **6.0.0**). `:28` `logger.child({ module: 'terminal-screen' })`.
- `:31` `SCREEN_SCROLLBACK_LINES = 5000`. `:34` `ScreenBufferKind = 'normal' |
  'alternate'`. `:36-46` `ScreenRendering { buffer, lines: string[], cols,
  rows }`. `:49-52` `ScreenSummary { kind: 'output' | 'screen'; text }`.
- `:62-84` **`readLines(buffer, from, to, joinWrapped)`** — the whole text
  model, verbatim semantics:
  - rows `y` in `[max(0,from), to)`; `buffer.getLine(y)` null → skipped;
  - `continues = joinWrapped && buffer.getLine(y+1)?.isWrapped === true`;
  - `text = continues ? line.translateToString(false) :
    line.translateToString(true).trimEnd()` — i.e. a row that a wrapped row
    follows is read at FULL width (never-written cells as spaces, no trim);
    any other row is read with xterm's trim-right (never-written cells only)
    and then `trimEnd()` (written spaces too — "full-screen programs pad with
    written spaces");
  - `if (joinWrapped && line.isWrapped && y > from && out.length > 0)` →
    appended onto `out[last]`, else pushed. NOTE `y > from`: a wrapped first
    row is pushed as its own line;
  - `:77-82` the trailing run of blank (whitespace-only) lines is dropped
    (only when the LAST line is blank; interior blanks stay).
- `:86-180` **`TerminalScreen`**: `new Terminal({ cols, rows, scrollback,
  allowProposedApi: true })` (`:96`); `cols`/`rows` getters read `term.cols` /
  `term.rows`; `write(data)` → `term.write(data)` (string; async parse);
  `resize(cols, rows)` → `term.resize` in try/catch, WARN
  **`[TerminalScreen] Failed to resize emulator`** `{ cols, rows, error }`
  (`:115-119`); `settle()` = `term.write('', resolve)` (`:124-126`);
  `render()` (`:129-139`) awaits settle, `active = term.buffer.active`,
  `buffer = active.type === 'alternate' ? 'alternate' : 'normal'`, `lines =
  readLines(active, 0, active.length, buffer === 'normal')`, cols/rows.
- `:147-174` **`takeSummary()`**: settle; if alternate → `text =
  readLines(active, 0, active.length, false).join('\n')`; `!text.trim() ||
  text === lastSnapshot` → null; else `lastSnapshot = text`, `{ kind:
  'screen', text }`. Else `lastSnapshot = null`; `from = summaryMarker &&
  !isDisposed ? summaryMarker.line : 0` ("a marker scrolled out of the
  scrollback is disposed; start from the oldest line still held");
  `lines = readLines(active, from, active.length, true)`; `if (lines.length >
  0 && lastSummaryTail !== null && lines[0] === lastSummaryTail) lines.shift()`;
  dispose the old marker, `summaryMarker = term.registerMarker(0)` (the
  CURSOR row, absolute); `lines.length === 0 || !lines.join('').trim()` →
  null (the marker still moved); `lastSummaryTail = lines[last]`; `{ kind:
  'output', text: lines.join('\n') }`.
- `:176-179` `dispose()`; `:188-196` `renderTranscript(raw, cols, rows)` =
  new screen, `write(raw)`, `render()`, dispose in `finally`.
- Order observable: `takeSummary` registers the NEW marker BEFORE deciding
  null — so an all-blank window still advances the marker.

### 1.2 `lib/terminal/pty-manager.ts` (the diff `a6292f169`; file at the pin 655 lines)

- `:22` imports `TerminalScreen, type ScreenRendering`; `cleanTerminalOutput`
  import GONE (`lib/terminal/clean-output.ts` keeps NO production caller at
  the pin — `git grep cleanTerminalOutput 322a5488e -- lib app` finds only its
  own definition `:60`).
- `:36-37` `ARIEL_FLUSH_IDLE_MS = 30_000`, `ARIEL_FLUSH_MAX_AGE_MS = 120_000`
  unchanged.
- `:55-70` **`flushArielBuffer(session, reason): Promise<void>`** — clear both
  timers (unchanged); `if (!session.arielPending) return session.arielFlush;`
  `session.arielPending = false; session.arielFlush =
  session.arielFlush.then(() => this.postArielSummary(session, reason));
  return session.arielFlush;` — a per-session CHAIN; summaries post in order.
- `:72-130` **`postArielSummary(session, reason)`**: `summary = await
  session.screen.takeSummary()` in try/catch → WARN **`[PTY] Failed to render
  terminal summary`** `{ sessionId, reason, error }` → return (`:77-84`);
  DEBUG **`[PTY] Ariel summary taken`** `{ sessionId, reason, kind:
  summary?.kind ?? null, chars: summary?.text.length ?? 0 }` (`:85-90`);
  `if (!summary) return;` then `await postArielTerminalOutputAnnouncement({
  chatId, sessionId, cleanedOutput: summary.text, kind: summary.kind, reason
  })` with the unchanged `.then(posted → chat-update nudge)` /
  `.catch(WARN '[PTY] Failed to post Ariel terminal-output announcement')`.
- `:248-274` spawn: `meta` gains `cols, rows` (`:258-259`, after
  `transcriptPath`); the session gains `screen: new TerminalScreen(cols,
  rows)`, `arielPending: false`, `arielFlush: Promise.resolve()` (`:268-270`).
- `:278-312` `onData`: ring buffer (unchanged), then `session.screen.write(data);
  session.arielPending = true; this.scheduleArielFlush(session);` (`:284-286`),
  transcript write, subscriber broadcast — same order as before with the
  screen write where the buffer append was.
- `:322-415` `onExit`: stamp meta; `const finalFlush =
  this.flushArielBuffer(session, 'session-closed')` (`:329`); `exit` message
  + close subscribers; end transcript; cleanupInit; the DB update now writes
  `{ exitedAt, exitCode, cols: session.meta.cols, rows: session.meta.rows }`
  (`:381-386`; comment `:377-380` "a session orphaned by a restart keeps its
  spawn-time geometry"); **`finalFlush.then(() =>
  postArielSessionClosedAnnouncement(…)).catch(WARN '[PTY] Failed to post
  close announcement').finally(() => session.screen.dispose())`**
  (`:394-408`) — the close announcement now WAITS for the final flush; the
  screen is disposed after. INFO `[PTY] Session exited` unchanged.
- `:423-436` the create still passes `createData` = meta minus id → the
  `terminal_sessions` INSERT now carries `cols`/`rows` numbers.
- `:585-606` `resize`: `session.pty.resize(cols, rows); session.screen.resize(
  cols, rows); session.meta.cols = cols; session.meta.rows = rows;` inside
  the one try (a PTY resize failure skips the screen resize + meta update).
- `:613-617` NEW `renderScreen(id): Promise<ScreenRendering | null>`.
- `:625-646` `kickAllForChat` AWAITS `flushArielBuffer(session,
  'session-closed')` per session before kill + delete.
- `lib/terminal/types.ts:26-31` `PtySessionMeta` gains `cols: number; rows:
  number`; `:40-48` `PtySession` loses `arielFlushBuffer`, gains `screen:
  TerminalScreen`, `arielPending: boolean`, `arielFlush: Promise<void>`.

### 1.3 `lib/tools/handlers/terminal-handler.ts` + `lib/tools/terminal-read-tool.ts`

- `terminal-handler.ts:3` imports `renderTranscript, type ScreenRendering`;
  `cleanTerminalOutput` + `splitLines` GONE. Constants `:21-29`:
  `TRANSCRIPT_REPLAY_BYTES = 4 * 1024 * 1024`, `RAW_RETURN_CHARS = 64 * 1024`,
  `DEFAULT_COLS = 80`, `DEFAULT_ROWS = 24`. `MAX_RETURN_LINES = 2000`
  unchanged. `tailFile(path, maxBytes)` — the default DROPPED (`:96`).
- **`executeTerminalReadTool`** (`:125-200`): after the unchanged
  session/chat checks — `rendering: ScreenRendering | null = null; raw = ''`;
  `liveSession = ptyManager.get(id)`; **live** (`liveSession &&
  session.exitedAt == null`): `rendering = await ptyManager.renderScreen(id);
  raw = ptyManager.getRingBuffer(id) ?? ''`; **else if transcriptPath**: `raw
  = await tailFile(path, TRANSCRIPT_REPLAY_BYTES); rendering = await
  renderTranscript(raw, session.cols ?? 80, session.rows ?? 24)` inside the
  unchanged try/catch (WARN `Failed to read transcript file`, both stay
  empty/null); `allLines = rendering?.lines ?? []`; slice/tail unchanged;
  `cleanedScrollback` GONE; DEBUG **`Terminal read executed`** gains `buffer:
  rendering?.buffer ?? null` (after `truncated`, before `raw`); output:
  `buffer: rendering?.buffer ?? 'normal'` (between `truncated` and
  `scrollback`), `scrollback: result.text` (rendered), `rawScrollback =
  raw.slice(-RAW_RETURN_CHARS)` when `input.raw === true` (the raw TAIL of the
  whole stream, no longer the slice's raw twin).
- **`formatTerminalReadResults`** (`:235-255`): `screenStr = out.buffer ===
  'alternate' ? '\nScreen: a full-screen program has the terminal; these lines
  are its current screen, not scrollback.' : ''` inserted as
  `${rangeStr}${screenStr}` — i.e. AFTER the `Lines:` line, BEFORE the blank
  line and the fence. Everything else byte-identical.
- `terminal-read-tool.ts:39` the `raw` description → **`When true, the
  response also includes a rawScrollback field: the last 64 KB of the raw byte
  stream, ANSI escape codes preserved. The default scrollback field is always
  rendered text.`**; `:49` the tool description → **`Read a terminal session
  in this chat as rendered text — what the operator's terminal pane shows. A
  plain shell returns its scrollback (the last 200 lines by default). While a
  full-screen program such as an editor or Claude Code has the terminal, the
  result is that program's current screen instead, and says so. Use start/end
  to read a specific line window (0-indexed, inclusive; negative values count
  back from the last line — e.g. start=-50 means "50 lines before the last").
  Set raw=true to additionally include the raw byte stream. The terminal is
  operator-controlled — this tool only reads; it cannot write. Use
  terminal_list first to discover live session ids.`** (model-visible; the
  snapshot `tool-definitions-snapshot.test.ts.snap:2084` moved). `:70-71`
  `TerminalReadOutput.buffer: 'normal' | 'alternate'` between `truncated` and
  `scrollback`.

### 1.4 `lib/services/ariel-notifications/writer.ts`

- `:141-146` `ArielTerminalOutputAnnouncement.kind?: 'output' | 'screen'`
  (after `cleanedOutput`, before `reason`).
- `:195-198` `heading = params.kind === 'screen' ? `Screen of session
  \`${sessionLabel}\` (a full-screen program has the terminal)${reasonNote}:`
  : `Terminal output from session \`${sessionLabel}\`${reasonNote}:``;
  `:200` the content's first line is `heading`. The reason notes, the fence,
  the elide, the whitespace-only short-circuit, the `terminal-output-<reason>`
  systemKind are unchanged.
- `:213` the DEBUG log object gains `kind: params.kind ?? 'output'` after
  `reason`, before `cleanedChars`.

### 1.5 Schema + migration + prettify (the FIFTEENTH move)

- `lib/schemas/terminal.types.ts:30-32` `TerminalSessionSchema` gains
  `cols: z.number().int().nullable().optional()`, `rows: …` after
  `transcriptPath`, before `createdAt` (the JSDoc "Terminal geometry at spawn,
  updated to its final value on exit"); `:66-67` the same on
  `TerminalSessionCreateSchema`; `:44-47` **`PtySessionMetaSchema =
  TerminalSessionSchema.omit({ createdAt, updatedAt })`** — so the WS `meta`
  message (`:110-111`) and the REST `session` payloads
  (`app/api/v1/terminals/route.ts:105` `findByChatId` rows;
  `[id]/route.ts:55-69` live `ptySession.meta` or the DB row minus timestamps)
  now carry `cols` / `rows` (numbers for a live session; nullable from a row).
- `migrations/scripts/add-terminal-session-geometry-v1.ts` (74 lines): id
  `add-terminal-session-geometry-v1`, description `Add cols and rows to
  terminal_sessions (geometry for transcript replay)`, `introducedInVersion:
  '4.11.0'`, `dependsOn: ['add-terminal-sessions-table-v1']`; `shouldRun` =
  SQLite && table exists && (`!columnExists('cols') ||
  !columnExists('rows')`); `run` = `addColumnIfMissing('terminal_sessions',
  'cols', 'INTEGER DEFAULT NULL')` then `'rows'`, `added` = count; INFO
  **`Added geometry columns to terminal_sessions`** `{ context:
  'migration.add-terminal-session-geometry-v1', added }` only when `added >
  0`; result message `Added cols and rows columns to terminal_sessions`,
  `itemsAffected: added`; ERROR **`Failed to add geometry columns to
  terminal_sessions`** `{ context, error }`. NOT `resumable` → a failure is
  fatal in v4's runner.
- `migrations/scripts/index.ts:453, 927-928, 1379-1380` registered LAST,
  after `addCharacterInventoryFlagMigration`.
- `lib/startup/prettify.ts:164` label **`Measuring each of Ariel's terminal
  windows, the better to read them back`** (NO-PORT by the recorded precedent
  — `chat_informs_permanent_repair.rs` header "NO-PORT").
- `docs/developer/DDL.md:867-868, 887-888` hand-written `"cols" INTEGER
  DEFAULT NULL` inline — see §3 for why the generateDDL render differs.

### 1.6 Tests v4 shipped for bug 255 (the oracle's seed corpus)

`__tests__/unit/lib/terminal/screen.test.ts` (107 lines, **11 cases**):
`renders a plain shell as scrollback lines` (40×5; `$ echo hi\r\nhi\r\n$ ` →
`['$ echo hi','hi','$']`); `keeps lines that scrolled off the visible rows`
(40×3, five lines → all five); `applies cursor addressing the way a
full-screen program draws (bug 255)` (40×6; `ESC[?1049h ESC[H ESC[2J`,
`ESC[1;1HHello ESC[1Cworld`, `ESC[3;3H ESC[38;5;174m⏺ ESC[39m Fixing it.`,
`ESC[5;1Hstale ESC[5;1H ESC[Kfresh` → alternate, `['Hello world','','  ⏺
Fixing it.','','fresh']`); `joins soft-wrapped rows back into one line`
(10×5; `abcdefghijKLMNO\r\nnext` → `['abcdefghijKLMNO','next']`); `follows
a resize` (80×24 → 100×30); `takeSummary`: `returns shell output since the
previous summary, without repeating its last line` (`$ ls\na.txt\n$` then
`$ pwd\n/tmp\n$`), `returns null when nothing new was written`, `snapshots a
full-screen program, skipping an unchanged screen` (`Claude\n> prompt`; a
`?25l ?25h` blink → null; `ESC[2;3Hdone  ` → `Claude\n> done`), `returns to
shell output when the program leaves the alternate screen` (contains `$
claude`, not `TUI`); `renderTranscript` `replays a recorded stream at the
given geometry` (20×4 → `{ buffer:'alternate', lines:['top','',' bottom'],
cols:20, rows:4 }`). These eleven byte streams are the minimum tier-1 corpus
for the emulator twin; they exercise `?1049h/l`, `H`, `2J`, `K`, `<n>C`,
`<r>;<c>H`, SGR 256-colour, `?25l/h`, soft wrap, CRLF scrolling.

### 1.7 `1d0a31132` — the scenario sort

- `lib/scenarios/sort.ts` (NEW, 56 lines): `:17-24` `SortableScenario {
  title; filename; path? }`; `:27` `LEADING_ARTICLE = /^the[\s_-]+/i`;
  `:30-35` **`scenarioSortKey(value)`** = `trimmed = value.trim(); stripped =
  trimmed.replace(LEADING_ARTICLE, ''); stripped.length > 0 ? stripped :
  trimmed`; `:37-38` `compareText = (a,b) => a.localeCompare(b, undefined, {
  sensitivity: 'base' })`; `:41-48` **`compareScenarios(a,b)`** =
  `compareText(key(a.title), key(b.title)) || compareText(key(a.filename),
  key(b.filename)) || compareText(a.title, b.title) || (a.path ??
  '').localeCompare(b.path ?? '')` — the last leg DEFAULT options (tertiary,
  case-sensitive); `:51-56` `sortScenarios(items, toSortable)` = map to
  `{item, key}`, `.sort(compare)` (V8 stable TimSort; the comparator is total
  and never NaN — plain `sort_by` suffices, no `js_array_sort_by`), map back.
- `lib/mount-index/scenarios-common.ts:46` import; `:239-245` in
  `buildScenarioList` AFTER the default-conflict loop (which still walks the
  `relativePath.localeCompare` order and picks the alphabetically-first
  filename): `scenarios = sortScenarios(parsed, (s) => ({ title: s.name,
  filename: s.filename, path: s.path }))`, NEW DEBUG **`Scenarios: built
  list`** `{ ...logContext, count: scenarios.length }`, return `{ scenarios,
  warnings }`.
- `lib/database/repositories/vault-overlay/parsers.ts:30, 675-697` NEW
  **`parseScenarioFilesSorted(docs, characterId)`**: parse each doc with
  `parseScenarioFile`, drop nulls, `sortScenarios(parsed, ({doc, scenario}) =>
  ({ title: scenario.title, filename: doc.fileName.replace(/\.md$/i, ''),
  path: doc.relativePath }))`, map to scenarios. `read-overlay.ts:301-305`
  `hydrateOne` replaces the `.slice().sort(relativePath.localeCompare).map
  (parseScenarioFile).filter` with it (so `character.scenarios` — and the
  vault's promoted "first scenario" — change order); `vault-readers.ts:295-299`
  `readCharacterVaultScenarios` likewise.
- `components/scenario/ScenarioSelect.tsx:23, 40-43, 79-88, 98-100, 128-168`:
  `storeSortFields = (s) => ({ title: s.name, filename: s.filename, path:
  s.path })`, `characterSortFields = (s) => ({ title: s.title, filename:
  s.title, path: s.id })`; every optgroup (project, general, each group,
  character) sorted locally with `useMemo` ("the server already sends them
  sorted; sorting here too keeps an entry a surface appended locally (a fresh
  save) in its proper place").
- Test `__tests__/unit/lib/scenarios/scenario-sort.test.ts` (124 lines, **10
  cases**): `scenarioSortKey` ×3 (`'The Gilded Cage'→'Gilded Cage'`, `'  THE
  Gilded Cage '→'Gilded Cage'`, `'Theatre Night'` kept, `'the-attic'→'attic'`,
  `'Into the Woods'` kept, `'The'→'The'`); `compareScenarios` ×3 (title with
  article ignored case-insensitively; filename tie-break; total order —
  `path 'a' < 'b'`, identical → 0); `buildScenarioList — display order` ×2
  (`['The Ballroom','Casino','untitled-dock','The Zeppelin']`; the default
  conflict still resolves to filename `a` with one warning);
  `parseScenarioFilesSorted` ×1 (`['3-the-cellar','Attic','The Parlour']`).
  Help `general-scenarios.md` +2, `scenario-browser.md` +1 (the docs lane's).

### 1.8 `406dbdd5a` — the bug-133 re-ruling

- `lib/services/dangerous-content/understudy.ts:87-106` NEW
  **`isUncensoredImageProfile(profile: Pick<ImageProfile,'id'|
  'isDangerousCompatible'> | null | undefined, conciergePolicy)`**: `!profile
  → false; profile.isDangerousCompatible === true → true; deskId =
  policy.desk.imageProfileId; Boolean(deskId) && profile.id === deskId`.
- `lib/image-gen/appearance-resolution.ts:431-500`
  **`sanitizeAppearancesIfNeeded(appearances, conciergePolicy,
  imageTargetUncensored, cheapLLMSelection, userId, chatId?, mode =
  'redress')`** — SEVEN params (was eight). Rule 1 (`:441-449`):
  `imageTargetUncensored` → DEBUG **`[AppearanceResolution] Image bound for an
  uncensored profile; appearances left as written`** `{ context:
  'image-gen.appearance-resolution', chatId, conciergeState:
  conciergePolicy.state }` → unchanged, NO classifier call. Rule 2
  (`:451-460`): `!conciergePolicy.preScreen.enabled` → the unchanged DEBUG
  `[AppearanceResolution] Appearance sanitization not applicable under this
  Concierge policy` `{ context, chatId, conciergeSource }` → unchanged (the
  old `&& !routeDirect` arm REMOVED — an Unmoderated chat on a moderated
  painter is sent as written). Rule 3 classify; the classify DEBUG
  (`:490-496`) DROPS `routesDangerousToUncensored`; the old step 4 early
  return DELETED; rule 4 = the old rule 5 sanitize (INFO `[AppearanceResolution]
  Sanitizing dangerous appearance descriptions` unchanged).
- `lib/services/dangerous-content/provider-routing.service.ts:15, 173-186`
  in `resolveImageProviderForDangerousContent`, AFTER the policy gate and
  BEFORE the understudy resolve: `if (isUncensoredImageProfile(originalProfile,
  conciergePolicy))` → DEBUG **`[DangerousContent] Image profile is already
  uncensored; not rerouting`** `{ profileId, profileName }` → `{ rerouted:
  false, imageProfile: originalProfile, apiKey: originalApiKey, reason:
  `Image profile is already uncensored: ${originalProfile.name}` }`. Reaches
  every caller of that wrapper (the tool, story backgrounds, avatars,
  wardrobe item images).
- `lib/tools/handlers/image-generation-handler.ts:50, 833, 944-955, 1191`:
  `resolveAppearances` gains `effectiveImageProfile: ImageProfile` (7th
  param, before `context`); `imageTargetUncensored =
  isUncensoredImageProfile(effectiveImageProfile, conciergePolicy)` replaces
  `routesDangerousToUncensored = routeDirect && Boolean(desk.imageProfileId)`;
  the call passes `profileAfterClassification` ("the profile this image will
  actually be painted on — the chat's own, or the one steps 5a/5b rerouted
  to").
- `lib/background-jobs/handlers/story-background.ts:55, 244-260, 450-461,
  519-535`: `paintsOnUncensoredProfile = isUncensoredImageProfile(imageProfile,
  conciergePolicy)`; `routesDirectToDesk = forcedUnderstudy === null &&
  !paintsOnUncensoredProfile && isDangerousChat && hasUncensoredImageProvider
  && conciergePolicy.routeDirect`; `uncensoredImageTarget = forcedUnderstudy
  !== null || paintsOnUncensoredProfile || isDangerousChat` (the crafter's
  candid flag — now TRUE for any Unmoderated chat, desk or not); the policy
  DEBUG `[StoryBackground] Concierge policy resolved` gains
  `paintsOnUncensoredProfile, routesDirectToDesk` after
  `uncensoredImageTarget`; the sanitizer is passed `forcedUnderstudy !== null
  || paintsOnUncensoredProfile || routesDirectToDesk` (NOT
  `uncensoredImageTarget`); the primary reroute arm is `else if
  (routesDirectToDesk)`.
- Tests (seed corpus): NEW `__tests__/unit/lib/tools/image-generation-
  uncensored-target.test.ts` (203 lines, 2 cases: a Moderated chat rerouted by
  the pre-screen tells the sanitizer it is bound uncensored; a Locked chat on
  a moderated painter stays and is told so); `provider-routing.test.ts`
  `describe('an image profile that is already uncensored')` +2 (`reason`
  contains `already uncensored`, `findById`/`findAll` NOT called; the
  configured desk profile kept even when not marked compatible);
  `story-background-uncensored-target.test.ts` reshaped (14 + 5 + 8 cases:
  "crafts candidly for a Moderated chat whose own image profile is
  uncensored", "… IS the configured desk", "crafts candidly for a dangerous
  chat with NO uncensored image profile configured", "still conceals for a
  Locked chat on a moderated painter", "tells the sanitizer a Moderated chat
  on its own uncensored profile is bound uncensored"…);
  `appearance-resolution.test.ts:496-660` `sanitizeAppearancesIfNeeded`
  suites re-shaped to the new signature ("Sanitization skipped for an image
  bound for an uncensored profile"). Help `the-concierge.md` +24,
  `image-generation-profiles.md` (the docs lane's).

### 1.9 `680b71044` — `/photos` SERVER half

- `lib/photos/user-gallery-service.ts:50-65` NEW `isUserGalleryMount(mp) =
  mp.storeType !== 'character'`; `isUserGalleryLink(link, repos)` =
  `isPhotosRelativePath(link.relativePath) && (mp = await
  docMountPoints.findById(link.mountPointId)) && isUserGalleryMount(mp)`.
  `:110-117` `ListUserGalleryInput.mountPointId?: string | null`.
  `:294-311` `listUserGallery`: `mountPoints = await resolveAlbumMountPoints(
  mountPointId ?? null, repos)`; NEW DEBUG **`Listing user gallery`** `{
  mountPointId: mountPointId ?? null, galleryMounts: mountPoints.length,
  hasQuery: !!query }` (logged BEFORE the fan-out, after limit/offset
  clamping); the fan-out, dedupe, search, paging unchanged. `:447-464` NEW
  `resolveAlbumMountPoints(mountPointId, repos)`: null → `findEnabled().filter
  (isUserGalleryMount)`; else `findById(id)`; `!mp || !mp.enabled` → `throw
  new Error(`Album not found: ${mountPointId}`)`; else `[mp]`. `:484-489`
  `removeFromUserGallery`: `if (!(await isUserGalleryLink(link, repos))) throw
  new Error('Link is not a gallery entry')` (unchanged text, wider predicate
  — a vault link is now refused; the `[id]` route maps it to **400** with the
  message, `app/api/v1/photos/[id]/route.ts:46-47`). `:514-518`
  `getUserGalleryEntry`: `!(await isUserGalleryLink(…)) → null` (the `[id]`
  GET → `notFound('Gallery entry')` 404).
- `app/api/v1/photos/route.ts:16-19` imports `withCollectionActionDispatch`,
  `notFound`, `listAllPhotoAlbumOptions`; `:33` `listQuerySchema.album:
  z.string().min(1).optional()`; `:40-77` `handleListGallery` (the old GET
  body) reads `album: url.searchParams.get('album') ?? undefined` and passes
  `mountPointId: parsed.data.album`; the catch gains `if (error instanceof
  Error && error.message.startsWith('Album not found')) return
  notFound('Album')` → **404 `{ error: 'Album not found' }`** (v4
  `responses.ts:150-155` `${resource} not found`) BEFORE the unchanged ERROR
  `[Photos v1] Error listing user gallery` + 500 `Failed to list gallery`.
  `:80-84` `handleListAlbums` → `albums = await listAllPhotoAlbumOptions(
  repos)`, DEBUG **`[Photos v1] Listed albums for the gallery selector`** `{
  count }`, `successResponse({ albums })`. `:86-88` `GET =
  createContextHandler(withCollectionActionDispatch({ albums: handleListAlbums
  }, handleListGallery))` — v4 `lib/api/middleware/actions.ts:168-183`: no
  `action` → the default handler; a known action → its handler; a bare or
  unknown `?action=` → the `Unknown action` 400 envelope (v5 already carries
  that envelope — `custom_tools_routes.rs:40`, `characters_routes.rs:730`).
  `POST /photos` and the `[id]` route are UNCHANGED.
- Tests: `__tests__/unit/lib/photos/user-gallery-service.test.ts` (18 cases
  at the pin, five NEW/re-shaped: `aggregates photos/ links across every
  non-character store, deduped by sha256, newest-first`, `lists one character
  vault when that album is chosen`, `lists one document store when that album
  is chosen`, `refuses an album that does not exist or is disabled`, `accepts
  deletion of a photos/ link in any non-character store`, `refuses a
  character vault's photo — that album has its own door`). The client half
  (`PhotosView.tsx`, `lib/query/keys.ts`) is the Inventory-SPA sibling's.
  Help `photo-gallery.md` +12/−2, `docs/developer/API.md` (the docs lane's).

### 1.10 `7263105c6`'s BUG-256 half (the title-lint hunks are the keystone sibling's)

Per hunk, address-related (THIS slice) vs lint (sibling):

- `lib/doc-edit/uri-producers.ts` (whole file re-written, 160 lines):
  `selfVaultMountPointId` DELETED; `formatSelfUri` import gone;
  **`docStoreUriFor({ mountPointId, mountPointName, relativePath, heading?,
  level? })`** (`:38-65`) — no `characterId`, no self branch; name-ambiguity
  count unchanged; **`uriForResolvedPath(resolved, opts?)`** (`:73-91`) — the
  `context` parameter GONE; **`buildDocStoreUriResolver()`** (`:98-…`) — no
  parameter, no `selfVaultId`; the module note `:11-22` ("No `self` here … A
  character may still TYPE `self` for its own vault; the resolver honours it.
  Owner-only surfaces (the knowledge injector, the Prospero vault whisper) may
  still teach the shorthand").
- `lib/doc-edit/path-resolver.ts:421-430` MISSING_CONTEXT text → **`Mount
  point is required for document_store scope: a path names no store on its
  own (every character vault has a metadata.json), so say which store — by
  name, ID or qtap:// address`**; `:523-528` NOT_FOUND text → **`No document
  store "${context.mountPoint}" is reachable in this context — nothing was
  read`** (the preceding WARN line unchanged).
- `lib/services/librarian-notifications/writer.ts:34` import drops
  `formatSelfUri`; `:214-229` `librarianUri`: project/general unchanged; `if
  (!mountPoint || mountPoint.toLowerCase() === 'self')` → DEBUG
  **`[LibrarianNotification] No store name for an announced path; printing the
  bare path`** `{ context: 'librarian-notifications', mountPoint: mountPoint
  ?? null, path }` and returns **`path "${path}" in a character's vault`**
  (NOT a qtap URI); else `formatDocStoreUri({ mountPointName: mountPoint,
  mountPointId: '', path })`. `:231-236` `scopeLabel`: `document_store` +
  `self` → **`a character's vault`** (checked BEFORE the `the document store
  "<mp>"` arm).
- `lib/tools/handlers/doc-edit/shared.ts:38-52` NEW **`publishedMountPoint(
  resolved: Pick<ResolvedPath,'scope'|'mountPointName'> | null | undefined,
  requested)`** = `resolved?.scope === 'document_store' &&
  resolved.mountPointName ? resolved.mountPointName : requested`.
- `document-ui-handlers.ts:39, 88-206` `handleOpenDocument`: `let storeName =
  input.mount_point`; on the existing-file branch `storeName =
  publishedMountPoint(resolved, input.mount_point)` (`:104`); `uriForResolvedPath(
  resolved)` ×2; `chatDocuments.openDocument(... mountPoint: storeName)`
  (`:159`), the result's `mountPoint: storeName` (`:182`), the Librarian open
  announcement's `mountPoint: storeName` (`:206`). NOTE the NEW-file branch
  (`:139-150`) never sets `storeName` → stays `input.mount_point` (a typed
  `self` on a blank new document still lands in the row — measured, not a
  claim in prose).
- `file-management-handlers.ts` (move ×2 arms, copy, delete ×2, create-folder
  ×2, delete-folder ×2, move-folder ×2): every `uriForResolvedPath(x,
  context)` → `uriForResolvedPath(x)`; every announcement `mountPoint:
  input.mount_point` → `publishedMountPoint(resolvedSource|resolved,
  input.mount_point)`; `handleCopyFile`'s announcement already used
  `resolvedX.mountPointName ?? xMountPoint` (unchanged).
  `markdown-handlers.ts` (read-frontmatter, update-frontmatter, read-heading,
  update-heading) and `text-handlers.ts` (read, write ×2 announcements,
  str-replace, insert, grep `:599`, list-files `:920`) likewise; `blob-
  handlers.ts:155, 210, 263, 307` drop `characterId` / the resolver arg;
  `search-scriptorium-handler.ts:138-141` `buildDocStoreUriResolver()`.
- `lib/tools/handlers/self-inventory/builders.ts:24, 60-68` the vault files
  via `formatDocStoreUri({ mountPointName: mountPoint.name, mountPointId:
  mountPoint.id, path: rel })`; `:978-1004` `buildContextFileUri`: a `self` /
  null mount → `''`; `buildHowToReach(uri, filePath)`: `!uri` → **`unknown
  store: "${filePath}" was opened from someone's own vault before stores were
  recorded by name; find it with doc_list_files in the vault you mean`**, else
  `doc_read_file({ uri: "${uri}" })`; `:1015` call passes `d.filePath`.
  `formatters.ts:28` the vault footer → **`(To read one of these files, use
  its uri, which names this vault and so reads the same whoever follows it. In
  your own calls the reserved authority 'self' is shorthand for YOUR vault —
  qtap://self/<relativePath> — but never hand a 'self' address to anyone else:
  it would open theirs.)`**.
- SIBLING (lint, not this slice): `lib/mount-index/database-store.ts:30-52,
  156, 211, 282, 306, 487, 573` (`lintInventoryChange` hooks),
  `lib/inventory/*`, `lib/pascal/{custom-tools,tool-gate-keys}.ts`,
  `vault-overlay/inventory-writes.ts`, `components/inventory/...`. NO-PORT:
  `jest.setup.ts:6-11` (`delete process.env.QUILLTAP_TIMEZONE` — a test
  harness guard), `package.json` `test:unit` `NODE_ENV=test`, CLAUDE.md /
  README / bugs.md / the bug doc.
- Test (seed corpus): NEW `__tests__/unit/lib/doc-edit/vault-addresses-
  reader-independent.test.ts` (170 lines, **7 cases**): producers never print
  `qtap://self` (`qtap://Abigail%20Character%20Vault/metadata.json` from
  `docStoreUriFor`, `uriForResolvedPath`, `uriForMount`); the Librarian never
  announces `self` even when typed (`legacy` contains neither `qtap://self`
  nor `"self"`); an address followed by another character opens the
  publisher's file; `self` typed by a character still means her own vault;
  five vaults × `metadata.json` each read their own; a wrong id →
  `PathResolutionError` `No document store "mp-nobody" is reachable in this
  context`; a filename alone → `MISSING_CONTEXT` containing `every character
  vault has a metadata.json`. Help `document-editing-tools.md` +4/−1 (docs
  lane).

### 1.11 `5a8338b30` — the toast

`lib/toast.tsx:28-30`: `toastContainer.style.zIndex = '1000'` (was `'50'`),
the why-comment "Above every dialog overlay (z-[60]–z-[101]) so a toast raised
from inside a modal is visible; below the full-screen startup gates at
z-[9999]". One production line; README badge + CHANGELOG + stamps.

### 1.12 The CRATE question — candidates surveyed (nothing added)

What `screen.ts` relies on from `@xterm/headless` 6.0.0: (1) a normal buffer
with scrollback (5,000 lines) + an alternate buffer, `buffer.active.type`;
(2) `buffer.length` (scrollback + rows on the normal buffer; rows on the
alternate) and `getLine(y)` by ABSOLUTE index; (3) per-line `isWrapped`
(soft-wrap flag) and `translateToString(trimRight)` (trim = never-written
cells only; written spaces survive); (4) `registerMarker(0)` — an absolute
line index of the cursor row that follows scrolling and is DISPOSED when it
leaves the scrollback; (5) `resize(cols, rows)` (xterm REFLOWS wrapped lines on
the normal buffer); (6) the write queue + `write('', cb)` settle (async
parsing); (7) the escape-sequence coverage a Claude-Code / vim / less / htop
stream exercises: `?1049h/l` (alt screen with cursor save/restore + clear),
`?47h/l`, `H`/`<r>;<c>H`, `<n>A/B/C/D`, `<n>E/F/G/d`, `J`/`2J`/`3J`, `K`,
`<n>P/X/@/L/M/S/T`, `r` (scroll region), SGR incl. 256/true colour, `?25l/h`,
`?2004h`, `?1000-1006`, OSC 0/2/8/52/133, `ESC 7/8`, `ESC M`, `ESC c`, DECSC,
the `?2026` synchronized-output pair, `CSI t` window ops, DCS/APC passthrough
(ignored). Rendering never uses attributes; only text, wrap flags, buffer
type, cursor row.

| candidate | version | licence | MSRV | deps (prod) | `cargo info` reaches? | what it gives |
|---|---|---|---|---|---|---|
| **`vt100`** (doy) | 0.18.1 | MIT | 1.85 | `vte` 0.15 (no default features), `unicode-width` 0.2.2, `itoa` 1 | **yes** | `Parser::new(rows, cols, scrollback)` / `process(&[u8])` / `set_size(rows, cols)` / `screen()`; `Screen::{contents(), rows(start,width), row_wrapped(row), alternate_screen(), scrollback_len(), set_scrollback(offset), cursor_position()}`; `Callbacks::on_scroll(RowContents, alternate_screen)` for rows leaving the buffer; `decset` handles 1/6/9/25/47/1000/1002/1003/1005/1006/1049/2004 (`screen.rs:1373-1409`; 1049 = `decsc` + clear alt + enter); `ed/el/ich/dch/ech/il/dl/su/sd/decstbm/cup/cuu…/vpa/cha/ri/ris`; unhandled CSI/ESC/OSC go to `Callbacks::unhandled_*` (ignored by default); wide chars via `unicode-width`; `Row::write_contents` emits content cells only (never-written tails dropped; written spaces kept — `cell.rs:113` `has_contents` = len>0); scrollback rows are `trim()`med on push (`grid.rs:731`, DEFAULT cells only); `Row::wrapped()` set by `col_wrap` (`grid.rs:855-885`); resize does NOT reflow (`grid.rs:81`). 5,111 lines, `no_std`-capable, `#![warn(clippy::pedantic)]`. |
| `alacritty_terminal` | 0.26.0 | Apache-2.0 | 1.85.0 | `vte` 0.15 (+`ansi`), `unicode-width`, `base64`, `bitflags`, `home`, `libc`, `log`, `parking_lot`, `polling` 3, `regex-automata`, `serde` (default), unix: `rustix` 1, `rustix-openpty`, `signal-hook` 0.4; windows: `miow`, `piper`, `windows-sys` | yes | a full `Term<T: EventListener>` grid with scrollback, alt screen, damage tracking, reflow on resize (closest to xterm here), but it brings its own PTY + event-loop layer (`polling`, `rustix-openpty`, `signal-hook`) beside the `portable-pty` v5 already links, a second `vte` feature set, and a `Term` API shaped for a renderer (cells/flags/`Grid<Cell>` iteration), not a text read-back. |
| `termwiz` | 0.23.3 | MIT | unknown | `vtparse`, `anyhow`, `fancy-regex`, `finl_unicode`, `image` (opt), `pest`+`pest_derive`, `phf`, `sha2`, `terminfo`, `wezterm-bidi/-blob-leases/-color-types/-dynamic/-input-types`, `filedescriptor`, `libc`, … (25 crates) | yes | **not a screen model** — a terminal I/O + widget toolkit (parser `vtparse`, `Surface` for widgets, `terminfo`); the wezterm screen model is `wezterm-term`, which **is NOT published** (`cargo info wezterm-term` → `could not find … in registry`), so it would be a git dependency. |
| `vte` | 0.15.0 | Apache-2.0 OR MIT | 1.62.1 | `memchr`; opt `log`, `cursor-icon`, `bitflags`, `serde` | yes | parser only (`Perform` callbacks); incremental UTF-8 (`lib.rs:68-69, 113-148`). The screen model would be written here. |

None of the four is in `Cargo.lock` today (`portable-pty` is; `vte` is not —
`Cargo.lock:3728`). `vt100` reproduces the eleven seed streams by inspection
of its `decset`/`cup`/`cuf`/`el`/`ed`/wrap code; the two it does NOT
reproduce are (a) reflow on resize (xterm re-wraps the normal buffer when
`cols` changes; vt100 truncates/pads rows — v4's only resize test checks
cols/rows), and (b) the async parse (irrelevant: `process` is synchronous, so
`settle()` is a no-op twin). Recommendation: **`vt100` 0.18.1**, with v5's
`TerminalScreen` twin built on `Parser` + a v5-owned summary-marker (an
absolute line counter: `scrolled_out + cursor_row`, maintained from
`on_scroll` or from `scrollback_len()` deltas — xterm's "disposed marker"
twin = "the marker's absolute line < the oldest line still held → from 0"),
and v4's `readLines` re-implemented over `Screen::rows(0, cols)` +
`row_wrapped(y)` windows (`set_scrollback(offset)` stepping, or scrollback
capacity 0 with the v5 side holding the 5,000-line ring from `on_scroll`).
The tier-1 family measures the rest (§4.1).

---

## 2. The v5 intersection map

### 2.1 Bug 255 — v5 HAS the pre-drift shape everywhere (v4-originated defect, not a convergence)

| v4 surface | v5 today | verdict |
|---|---|---|
| `screen.ts` (emulator) | nothing; `crates/quilltap-core/src/terminal_clean.rs` (228 lines, the strip) is the whole text model | **LACKS** — NEW module (recommended `crates/quilltap-core/src/terminal_screen.rs`, core-side so the tool can render an exited transcript without the host; the host owns the live instances) |
| `pty-manager.ts` Ariel chain | `crates/quilltap-host/src/terminal/mod.rs:177-187` `SessionIo { ring_buffer, ariel_buffer, last_data_at, ariel_started_at, subscribers, transcript }`; `:567-575` `on_data` appends to `ariel_buffer`; `:814-856` `flush_ariel` takes the buffer, `clean_terminal_output`, posts; `:863-899` `ariel_timer_loop` (idle / max-age deadlines, D20 host-owned); `:599-672` `exit_sequence` flushes then posts close (already sequential — v4's new `finalFlush.then(close)` is what v5 ALREADY does, awaited); `:768-786` `kick_all_for_chat` awaits the flush; `:748-762` `resize` (PTY only); `:350-359` `get_ring_buffer` / `ariel_buffer_len` | **pre-drift**: `ariel_buffer: String` → `screen: TerminalScreen` + `ariel_pending: bool` (the chain is a tokio `Mutex`/serialised flush per session — v5's `flush_ariel` already runs on one task per session; `kick_all`/`exit` await); `resize` must also resize the screen + stamp meta; NEW `render_screen(id)`; the exit stamp writes cols/rows; spawn writes cols/rows |
| `PtySessionMeta` cols/rows | `crates/quilltap-host/src/terminal/protocol.rs:26-42` (no cols/rows); `crates/quilltap-web/src/terminal_routes.rs:63-75` `row_to_meta_json` (no cols/rows); `apps/web/src/app/terminal/terminal-protocol.ts:13-42` | **LACKS** both fields — WS `meta`, `GET /terminals` (`:178-182`), `GET /terminals/{id}` (`:195-230`), the Tauri `terminal_attach` meta (`crates/quilltap-tauri/src/terminal_ipc.rs:56-106`) |
| `terminal_sessions.cols/rows` | `crates/quilltap-core/src/db/terminal_sessions.rs:69-83` `TerminalSessionRow` (no cols/rows), `:85-86` `SELECT_COLS`, `:212-236` `create` (11 columns), `:238-304` `update` patch, `:327-358` `mark_session_exited` (exitedAt/exitCode/updatedAt only) | **LACKS** |
| the ensure `add-terminal-session-geometry-v1` | none; `host.rs` carries NO `add-terminal-sessions-table-v1` twin either (grep: zero hits in `crates/`, `harness/`) — the table comes from `fresh_schema.json:32` at provisioning and v4's `shouldRun` (table-exists gate) never runs it on a fresh instance, so no stamp exists on either side | **LACKS** — NEW `db/terminal_sessions_geometry_repair.rs` + ONE host block (§3) |
| `terminal-handler.ts` | `crates/quilltap-core/src/tools/terminal.rs:403-461` `execute_terminal_read(db, chat_id, args, full_content)` — `split_lines(full_content)` then slice/tail then `clean_terminal_output(&result.text)`; `:118-139` `TerminalReadOutput` (no `buffer`); `:497-537` `format_terminal_read` (no `Screen:` line); the seam `tools/executor.rs:285-289` `TerminalScrollbackSource::scrollback(&self, id) -> String` (`:848` call); `crates/quilltap-host/src/terminal/scrollback.rs:48-74` `PtyScrollbackSource` (live ring buffer / `tail_file(path, 1_000_000)`) | **pre-drift**: the seam must return BOTH a rendering and the raw tail — `TerminalScrollbackSource::read(&self, id) -> TerminalReadSource { rendering: Option<ScreenRendering>, raw: String }` (live: `manager.render_screen(id)` + ring; exited: `tail_file(path, 4 MiB)` + `render_transcript(raw, cols??80, rows??24)` — the DB row is already read in `scrollback.rs:51-59`); `tail_file`'s 1 MB constant → 4 MiB; `rawScrollback` = `utf16_slice_from(raw, len-65536)` (v4 `.slice(-65536)` is UTF-16 units — `jsstr::utf16_slice_from` exists, used at `mod.rs:74`) |
| the `terminal_read` definition bytes | `crates/quilltap-core/src/tools/definitions/data.rs:267` (the OLD description + `raw` description) | **pre-drift** — one row; `tool_definitions_equivalence` pins it |
| `writer.ts` heading + `kind` | `crates/quilltap-core/src/services/ariel_notifications.rs:200-207` `ArielTerminalOutputAnnouncement { chat_id, session_id, cleaned_output, reason }`; `:262-270` the heading literal; the DEBUG object — grep shows no `cleanedChars` capture site (the writer's debug line is not ported as a pin) | **pre-drift**: `kind: ArielSummaryKind { Output, Screen }` (default Output), the two headings; `ariel_writers_tier3_equivalence` extends with `kind: 'screen'` rows ×3 reasons |
| `prettify.ts` label | NO-PORT (precedent `chat_informs_permanent_repair.rs` header) | n/a |
| `scheduled_maintenance.rs:725` | a TEST-ONLY hand-rolled `terminal_sessions` DDL (`exitedAt TEXT, exitCode REAL, transcriptPath TEXT`) | add the two columns to the test table only if the lane's reads of `cols`/`rows` run there (`find_closed_before` uses `SELECT_COLS` → YES, it must) |
| Almanack `ledgers.terminalSessions` (`almanack/phase3_ledgers.rs:1075-1082`) | counts + shells only | untouched |
| SPA terminal pane | `apps/web/src/app/terminal/{terminal.ts, terminal-session.service.ts, terminal-protocol.ts}` — sends `resize` (`terminal-stream-transport.spec.ts:110-112`) | `PtySessionMeta` type gains `cols?: number | null; rows?: …`; nothing else (the pane does not read Ariel text) |
| host tests | `crates/quilltap-host/tests/terminal_pty.rs:370-446` (`ariel_buffer_len` instrumentation ×6), `:487-533` `scrollback_source_live_then_transcript`; `terminal_ariel_e2e.rs:118-135` asserts the cleaned payload in the Ariel row | both re-pinned to `ariel_pending` / the rendered text |

The lane EDITS: `crates/quilltap-host/src/terminal/{mod,scrollback,protocol}.rs`,
`crates/quilltap-host/src/host.rs` (one ensure block, after `=== end P4.160
===` `:2257` — v4 registers the migration LAST),
`crates/quilltap-core/src/{terminal_screen.rs (new), db/terminal_sessions.rs,
db/terminal_sessions_geometry_repair.rs (new), db/mod.rs (one `pub mod`),
tools/terminal.rs, tools/executor.rs (the seam), tools/definitions/data.rs
(row 267 only), services/ariel_notifications.rs (the terminal-output writer +
struct), services/scheduled_maintenance.rs (test DDL only)}`,
`crates/quilltap-web/src/terminal_routes.rs` (`row_to_meta_json` + the WS
meta), `crates/quilltap-tauri/src/terminal_ipc.rs` (meta passthrough — likely
no change if it serialises `PtySessionMeta`), `apps/web/src/app/terminal/
terminal-protocol.ts`, `Cargo.toml` (+ `vt100 = "0.18.1"` in quilltap-core),
`Cargo.lock`. It must NOT touch `services/provisioning/fresh_schema.json` (the
keystone's ONE re-dump), `terminal_clean.rs` (keep the module — v4 keeps
`clean-output.ts` with no caller; the strip loses both production callers but
its tier-1 family `terminal_clean` stays green), the Inventory / custom-tool
/ interrupts files.

### 2.2 Scenario sort — v5 HAS a pre-drift shape at three sites + a missing SPA sort

| v4 | v5 | verdict |
|---|---|---|
| `lib/scenarios/sort.ts` | nothing; `crates/quilltap-core/src/collation.rs:51` `locale_compare` (en-US tertiary = `localeCompare` default) and `:57` `locale_compare_base` (primary = `{sensitivity:'base'}`), both ICU4X | **LACKS** the comparator — NEW `crates/quilltap-core/src/scenarios/sort.rs` (`scenario_sort_key`, `compare_scenarios`, `sort_scenarios`); `LEADING_ARTICLE` = `(?i)^the[\s_-]+` with JS `\s` (Unicode White_Space incl. `\u{FEFF}` — use the existing JS-regex helpers, `jsstr`/`regex` with the `\s` class v5 already reproduces elsewhere; `trim()` = JS `String.prototype.trim` = `jsstr::js_trim`) |
| `scenarios-common.ts:239-245` `buildScenarioList` | `crates/quilltap-core/src/db/scenarios.rs:226-232` `build_scenario_list` — `docs.sort_by(locale_compare(relative_path))` BEFORE parse (path order feeds the default-conflict walk, which is right) and the list is returned in that order | **pre-drift**: keep the path sort for the conflict walk; after it, `sort_scenarios(parsed, |s| (s.name, s.filename, s.path))` + the DEBUG `Scenarios: built list` `{…logContext, count}` |
| `read-overlay.ts` `hydrateOne` scenarios | `crates/quilltap-core/src/db/vault_read_overlay.rs:398-408` (the `Scenarios/*.md` fold: `locale_compare(relative_path)` then `parse_scenario_file`) | **pre-drift**: `parse_scenario_files_sorted(docs, character_id)` twin — title from the parsed scenario, filename = `fileName.replace(/\.md$/i,'')` (the doc's `file_name` — check `VaultFolderDoc` carries it; `vault_doc(d)` has `relative_path`; the v4 key is the FILE NAME, not the relative path), path = `relative_path` |
| `vault-readers.ts` `readCharacterVaultScenarios` | the ledger names a twin in `vault_read_overlay.rs` / `vault_overlay.rs`; grep finds NO `read_character_vault_scenarios` in v5 (`vault_overlay.rs` does not exist at `5a94a9d1e`); the overlay fold above IS the one v5 reader | **HAS** (one site) — confirm by grep at lane start whether `scenarios/location.rs`'s character tier reads the vault through the overlay (P4.D295) or its own fold |
| `ScenarioSelect.tsx` optgroup sorts | `apps/web/src/app/scenario/scenario-select.ts:97-133` renders the four optgroups from the inputs with NO sort (grep `sort` → nothing) | **LACKS** — a `sortScenarios` twin in `apps/web/src/app/scenario/scenario-sort.ts` (new) with `storeSortFields` / `characterSortFields`, applied in the select's computed lists (`Intl.Collator('en-US',{sensitivity:'base'})`? NO — v4 uses `localeCompare(b, undefined, …)` = the BROWSER's default locale; the SPA twin uses the same `localeCompare` call verbatim so both run the browser's ICU) |
| `everywhere.ts` owner sort | `crates/quilltap-core/src/scenarios/everywhere.rs:68-72` `by_name` = `locale_compare_base` (unchanged by this commit) | HAS |

Families that pin these (all trip at the pin): `scenario_location_equivalence`
(the per-location `list` results + the captured line whitelist `:83` — the
NEW `Scenarios: built list` DEBUG line is NOT in the whitelist, so the capture
census is unchanged unless the lane adds it; v4's oracle `scenario-location.ts:60`
same list), `scenarios_routes_equivalence`, `chat_scenario_routes_equivalence`,
`canon_scenario_equivalence`, `vault_read_overlay_equivalence` (the hydrated
`scenarios` array order + "promote first" default), the CLI Tier R
`cli_differential.rs` (3 scenario mentions — check which list), the SPA
`scenario-select.spec.ts` / `scenario-selection.oracle.spec.ts`.

### 2.3 Bug 133 — v5 carries the OLD rule at four sites

| v4 | v5 | verdict |
|---|---|---|
| `isUncensoredImageProfile` | nothing (`dangerous_content/understudy.rs:103` has a private `is_dangerous_compatible(&Value)` for the scan) | **LACKS** — `pub fn is_uncensored_image_profile(profile: &RouteProfile /*needs an is_dangerous_compatible field*/, policy: &ResolvedConciergePolicy) -> bool`; `RouteProfile` (`provider_routing.rs:27-33`: id/name/provider/model_name/base_url) LACKS `is_dangerous_compatible` — add it (every constructor: `generate_image.rs:716-722`-style literals, `story_background_job.rs:716-724`, the avatar + wardrobe-item callers of `resolve_image_provider_for_dangerous_content`) |
| `sanitizeAppearancesIfNeeded` | `services/appearance_resolution.rs:442-454` the 12-arg port with `is_dangerous_chat`, `routes_dangerous_to_uncensored`; `:461-475` rule 1 (`!pre_screen.enabled && !route_direct`), `:477-481` rule 2 | **pre-drift**: drop both flags for `image_target_uncensored: bool`; rule 1 = the new DEBUG (target `quilltap::appearance_resolution`, `context`, `chat_id`, `concierge_state`); rule 2 = `!pre_screen.enabled` with the old DEBUG; classify DEBUG drops the key; delete the old step 4 |
| `provider-routing.service.ts` early return | `services/dangerous_content/provider_routing.rs:274-300` — the policy gate then straight to the understudy | **pre-drift**: insert the `is_uncensored_image_profile(original)` arm after the gate (`:289`) with the DEBUG + `reason: "Image profile is already uncensored: {name}"` |
| the tool | `tools/generate_image.rs:2009-2020` passes `db_ctx.is_dangerous_chat`, `tool_routes_dangerous_to_uncensored(&concierge_policy)`; `:2804-2812` that predicate; `:2825-…` `mod bug_133_tests` (unit test `tool_routes_dangerous_to_uncensored_needs_route_direct_and_a_profile`); `:1960` `effective_profile = image_profile.clone()` (after the reroute); `:1474` `LoadedProfile.is_dangerous_compatible` EXISTS | **pre-drift**: pass `is_uncensored_image_profile(&effective_profile_after_classification, policy)`; DELETE `tool_routes_dangerous_to_uncensored` + its unit test (v4 deleted the predicate) |
| story backgrounds | `services/story_background_job.rs:336-337` `is_dangerous_chat`, `has_uncensored_image_provider`; `:393-401` `uncensored_image_target` + the policy DEBUG; `:574-586` passes `uncensored_image_target` to the sanitizer; `:716` `else if uncensored_image_target` reroute; `:271-281` `image_profile` is a JSON `Value` (read `isDangerousCompatible` from it) | **pre-drift**: `paints_on_uncensored_profile`, `routes_direct_to_desk`, the new `uncensored_image_target`, two DEBUG keys, the sanitizer's `forced || paints || routes_direct_to_desk`, `else if routes_direct_to_desk` |
| `isDangerousCompatible` reader | `db/image_profiles.rs:72` (`bool`), `:102` (patch), `:297`, `:327` | HAS |

Pins (grep `bug.133|bug_133|routes_dangerous_to_uncensored|uncensored_image_target`
over `crates/quilltap-harness/tests` + `harness/oracle`): NO named carve.
`appearance_sanitize_gate_tier3_equivalence.rs` (30-case grid; `:113`
`routes_dangerous_to_uncensored` field; `:449-461` the call) ↔
`harness/oracle/cases/appearance-sanitize-gate.test.ts:283-291` ↔
`fixtures/appearance-sanitize-gate.json` (`isDangerousChat` ×
`routesDangerousToUncensored` ×2 per row, lines 96-866);
`image_generation_tier3_equivalence.rs:279-300` + `fixtures/image-generation.json:436`
`detect_only_sanitizes_appearance`; `story_background_job_tier3_equivalence.rs`
+ `oracle/cases/story-background-job.test.ts:270-320` (the candid-craft
seam keyed on `The target image provider accepts adult content`);
`tool_image_generation_equivalence`, `wardrobe_item_image_generation_tier3_equivalence`
(the reroute wrapper). The grid family is the one that MUST be re-shaped
(§4.3); the others re-pin by regen at the new baseline.

### 2.4 `/photos` — v5 HAS the old roll-up and delete; LACKS `album=` + `?action=albums`

| v4 | v5 | verdict |
|---|---|---|
| `isUserGalleryMount` / `resolveAlbumMountPoints` | `photos/user_gallery_service.rs:110-145` `list_user_gallery(conn, q, embedding, tags, limit, offset)` fans over `find_enabled_for_docedit()` (fallback-wrapped, P4.163) with no `storeType` filter | **pre-drift**: add `mount_point_id: Option<&str>`; filter `store_type != "character"` (the `EnabledStoreRow` carries `store_type` — `photo_album_options.rs` reads it); `Some(id)` → `find_by_id`, `!mp || !mp.enabled` → `Err("Album not found: {id}")`; the DEBUG `Listing user gallery` |
| `removeFromUserGallery` / `getUserGalleryEntry` vault refusal | `:395-408` (path check only), `:350-…` | **pre-drift**: `is_user_gallery_link(link)` = path && `find_by_id(mount)` && not character |
| `?album=` | `api/photos.rs:115-178` `photo_gallery_list(db, provider, user_id, q, tag, limit, offset)`; `api/types.rs:2612-2622` `PhotoGalleryList { q, tag, limit, offset }`; `crates/quilltap-web/src/photos_routes.rs:70-88` | **LACKS**: `album: Option<String>` on the Request (`z.string().min(1)` — an empty `?album=` is Zod's `Too small` 400 through the existing `zod_*` helpers), threaded to the service; `Album not found` → `ErrorKind::NotFound` with v4's `Album not found` body (BEFORE the generic 500) |
| `?action=albums` | `photos/photo_album_options.rs` (P4.D275 — `list_all_photo_album_options` twin, pinned by `photo_album_options_equivalence`); no Request variant; `photos_routes.rs` has no action dispatch | **LACKS**: `Request::PhotoAlbumsList`, `api/photos.rs` arm → `Response::PhotoGallery(json!({"albums": …}))` + DEBUG `[Photos v1] Listed albums for the gallery selector` `{count}`; the web edge dispatches `action` through the shared `Unknown action` envelope helper (`custom_tools_routes.rs:142`, `scenario_builder_routes.rs:97` precedents); a NEW `COLLAPSE_CENSUS` row (`crates/quilltap-web/tests/dispatch_wrong_type_census.rs:1777-1797` carries the photos rows — memory note `frozen-engine-seams-and-web-censuses`) |
| `DELETE /photos/{id}` on a vault link | `api/photos.rs:252-270` maps the service error; the `[id]` route is unchanged in v4 (400 `Link is not a gallery entry`) | HAS the mapping; the predicate widens |
| SPA | `apps/web/src/app/screens/photos/{photos-page.ts, photos.api.ts}` (`removeGalleryEntry` → `/api/v1/photos/{id}`; no album state) | the Inventory-SPA sibling's (client half) — the server lane exposes `album` + `albums`; a Shared contract (§5) |

Pins: `crates/quilltap-web/tests/photos_routes_equivalence.rs` (the route
family over `harness/oracle/cases/photos-routes.test.ts:211` `mod.GET(
mockRequest(`${B}/photos${qs}`))` — the fixture's `photos-{main,mount}.db`
carries character-vault `photos/` links? if so `list_default` moves at the
pin); `photo_album_options_equivalence` (unchanged).

### 2.5 Bug 256 — v5 HAS the defect; the producer census

v5 `qtap://self` PRODUCERS (must change): `doc_edit/uri_producers.rs:22-39`
(`doc_store_uri_for`'s `character_id` arm → `format_self_uri`), `:62-91`
(`uri_for_resolved_path(.., character_id, ..)`), `:96-123`
(`DocStoreUriResolver::build(main, mount, character_id)` + `uri_for_mount`'s
self arm); `services/librarian_notifications.rs:107-121` `librarian_uri`'s
`format_self_uri` arm + `scope_label` (`:124-…`); `tools/self_inventory.rs:945`
(the vault section's files), `:1941-1953` `build_context_file_uri`
(`None`/`self` → `format_self_uri`), `:1963` `how_to_reach` (one shape),
`:2440` the footer literal; `tools/doc_edit/document_ui.rs:294, 359`
(`addressing.mount_point` into the `chat_documents` row + the announcement —
`publishedMountPoint` twin: `resolved.mount_point_name` when scope is
document_store); `doc_edit/path_resolver.rs:591, 713, 1101` (the two error
texts; `:1101` is a third site of the NOT_FOUND text — v4 has ONE at `:526`;
check whether `:1101` is the same arm or a test). Call sites that lose the
`character_id` argument (v5 census, 19): `tools/send_mail.rs:182` (already
`None`), `tools/doc_edit/blob.rs:255, 329, 414, 499`, `tools/search.rs:658`,
`tools/doc_edit/file_management.rs:79`, `document_ui.rs:156, 244`,
`markdown.rs:63, 149, 221, 300`, `text.rs:101, 358, 539, 689, 760, 1171`.
STAY (v4 keeps `formatSelfUri` there): `services/knowledge_injector.rs:482`
(v4 `knowledge-injector.ts:372`), `services/prospero_notifications.rs:464`
(v4 `prospero-notifications/writer.ts:589`); `post_office/*`, `send_mail.rs`,
`suparna_mail.rs` use `qtap://self` as INPUT syntax (the resolver still
honours typed `self`) — untouched. `tools/definitions/data.rs` carries
`qtap://self` in tool DESCRIPTIONS — v4's definitions snapshot at this
commit: UNCHANGED for the doc tools (the snapshot diff in `7263105c6` is
absent from its file list), so leave them.

Pins that trip: `doc_edit_path_resolver_equivalence` (`uri-self`, `res-self`,
`uri-resolved-project` unchanged); `doc_enum_equivalence` (every `uri` of a
self-vault row — the fixture's character vault files); `doc_text_equivalence`
(one `uri` compare — the `read_qtap` INPUT `qtap://self/Notes/journal.md`
still resolves; its OUTPUT `uri` changes to the named form);
`doc_blob_equivalence` (no `uri` compare — but `handleWriteBlob`'s
announcement bytes via the Librarian family?); `post_office_librarian_equivalence`
(`ds-self` ×3 rows: `:87, 108, 180` — v4's oracle `post-office-librarian.ts:67,
78` plants `mountPoint: 'self'` → the Librarian now prints `path "<p>" in a
character's vault` + the new DEBUG line and the `a character's vault` label);
the Document Mode family (P4.6w — grep `chat_documents` dump in
`document_mode*` / `doc_ui*` tests at lane start) for the row's `mountPoint`;
`self_inventory` families (the vault section URIs + footer; P4.15/P4.D157 —
`self_inventory_*_equivalence.rs`). The resolver's MISSING_CONTEXT /
NOT_FOUND texts are pinned by `doc_edit_path_resolver_equivalence` (`:591,
713` are compared by message — `sentinelize`).

### 2.6 The toast — v5 HAS the old value

`apps/web/src/styles/qt-components/_toast.css:31-39` `.qt-toast-container {
position: fixed; bottom: 20px; right: 20px; z-index: 50; … }` ↔ v4 `toast.tsx:25-32`
(the inline-style twin). v5's ladder: `_surfaces.css:203-206` `.qt-dialog-overlay
{ @apply fixed inset-0 z-[60]; }`; raised overlays in use (grep `z-\[` over
`apps/web/src/app`): `z-[60]`×20, `z-[70]`×5, `z-[80]`×3, `z-[90]`×9,
`z-[100]`×19, `z-[101]`×1, `z-[9999]`×2 (the startup gates); `_content.css:66`
raises the page child hosting an open dialog to 60. So `z-index: 1000` sits
above 101 and below 9999 exactly as v4 intends. Lane edits `_toast.css:35` +
its why-comment; `toast-container.ts` (`:26` the class) untouched.

---

## 3. Schema / migrations / boot ensures — the FIFTEENTH move

- **The generateDDL-rendered shape** (fresh table): `TerminalSessionsRepository
  extends AbstractBaseRepository<TerminalSession>` over `TerminalSessionSchema`
  → `schema-translator.ts` `mapToSQLiteType` (`:316-333`): `'number'` → `INTEGER`
  only if `field.min` AND `field.max` are both integers (`:324-326`), else
  `REAL` (`:328`); `getNumberConstraints` (`:151-174`) reads only
  `greater_than`/`less_than`/`min`/`max` checks — `.int()` sets none. PROOF in
  the committed artifact: `fresh_schema.json:32` renders the identically-shaped
  `exitCode` as `"exitCode" REAL`. So the FIFTEENTH move's fresh shape is
  **`"cols" REAL,\n  "rows" REAL`** (no DEFAULT, nullable) in SCHEMA order —
  between `"transcriptPath" TEXT` and `"createdAt"`. (`DDL.md:867-868` is
  hand-written; the ledger §1 note `:251-268` and row `:406` repeat it. VERIFY
  at the keystone's re-dump — `harness/oracle/provision/dump-fresh-schema.ts`.)
- **The migration-appended shape** (existing table): `ALTER TABLE
  terminal_sessions ADD COLUMN "cols" INTEGER DEFAULT NULL`, then `"rows"`,
  appended LAST (after `updatedAt`). `migrations_state` row id
  `add-terminal-session-geometry-v1`, `dependsOn add-terminal-sessions-table-v1`,
  registered LAST in `index.ts:927-928`.
- **The P4.D249 precedent applies verbatim** (`crates/quilltap-core/src/db/
  chat_informs_permanent_repair.rs:1-60`: "The two v4 shapes DISAGREE here …
  Both are carried"): `fresh_schema.json` carries the generateDDL shape (the
  keystone's re-dump); a NEW `db/terminal_sessions_geometry_repair.rs`
  `ensure_terminal_sessions_geometry_columns(main)` emits the migration DDL
  verbatim for an existing table — a no-op when the table is absent (v4's
  table gate; on a v5 boot the table always exists from provisioning) or both
  columns exist; two `PRAGMA table_info` reads on the happy path. **NO
  `migrations_state` stamp**: v4's `shouldRun` is column-gated, so after v5's
  ALTER v4's runner finds both columns and never re-runs (the
  `chat_informs.permanent` class, NOT the `add-memory-tiers-v1` ping-pong
  class — nothing later drops the columns). Fatal on failure (not
  `resumable`). INFO/ERROR lines NO-PORT (the recorded precedent). Column ORDER
  harmless: v5 names columns on every read/write (`SELECT_COLS`).
- **Host registration:** `crates/quilltap-host/src/host.rs` after `=== end
  P4.160 ===` (`:2257`) — v4's order puts it after `add-character-inventory-
  flag-v1` (the keystone's 13th/14th ensures, if those land as ensures); the
  block follows the `=== P4.D249 ===` idiom (`:1670-1680`).
- **The harness artefacts:** `provisioning_equivalence` (1d requires
  `QT_FRESH_SCHEMA_LIVE` — `:21, :48, :296`) goes RED at the baseline move
  until the keystone re-dumps (the D23 tripwire, by design);
  `zero_migrations_rerun_equivalence` must add the id to the narrowed runner
  list and prove v4 re-runs NOTHING after v5's ensure on `planted` / `fresh` /
  `v4-migrated` (and the `-unstamped` twins re-run exactly what they did
  before — this ensure stamps nothing); `restore_vintage_state`'s boot-align
  list (`crates/quilltap-harness/tests/restore_vintage_state.rs:93-127`)
  gains the ensure (memory note `vintage-restore-mirror-boot-list`);
  `migration_indexes.json` UNMOVED (no index); `migration_index_backfill_
  equivalence` untouched; the P4.D315-class `BOOT_CREATED_*` carves
  untouched (nothing is boot-created here).
- **Readers of a NULL cell:** `session.cols ?? 80` / `rows ?? 24` in the
  handler only. The `Option<f64>` binding on create (REAL affinity, the
  `exitCode` precedent — `terminal_sessions.rs:212-214` doc) and `Option<i64>`
  on read (`TerminalSessionRow.exit_code`) — follow `exitCode` exactly so the
  tier-2 dump's `js_number_to_json` cells match (`80` not `80.0`).

---

## 4. The differential plan

### 4.1 Bug 255

| unit | tier | family (existing → extends) | fixture / oracle | new oracle cases |
|---|---|---|---|---|
| the ensure (two shapes, no stamp) | 2 | `zero_migrations_rerun_equivalence` (narrowed runner list +1), `provisioning_equivalence` 1b/1c/1d (after the keystone's re-dump), `restore_vintage_state` boot-align; a NEW `terminal_sessions_geometry_ensure_equivalence` (the P4.D249 `chat_informs_permanent` family's shape: v5's ensure vs v4's REAL narrowed `MigrationRunner` on {pre-column table, fresh 13-column table, one-column-present}, `table_info` + `sqlite_master` byte-equal) | the P4.D249 ensure recipe (`chat_informs_permanent_ensure_equivalence.rs` — name by grep at lane start) | 3 modes |
| `terminal_sessions` create / update / exit stamp with cols/rows | 2 | `terminal_sessions_tier2_equivalence` (`harness/oracle/fixtures/terminal-sessions-tier2.json` ops gain `cols`/`rows` cells incl. explicit `null`; the `build-terminal-sessions-fixture.ts` builder re-bakes a 13-column table at the pin — **the committed dump's rows grow two cells: a RUNTIME BREAK the order owns**) | `QT_FIXTURE_TERMINAL_SESSIONS` | +3 ops (create with geometry, create without, the exit update writing both) |
| the screen model (`TerminalScreen` twin: `render`, `take_summary`, `render_transcript`) | **1 exact** over recorded byte streams: v4's REAL `lib/terminal/screen.ts` driven by a NEW oracle case `harness/oracle/cases/terminal-screen.ts` over a committed corpus `fixtures/terminal-screen.json` (the eleven `screen.test.ts` streams + a captured Claude-Code-shaped stream + the §6 edge rows), emitting `{buffer, lines, cols, rows}` per `render` and the `takeSummary` sequence per script | NEW `terminal_screen_equivalence.rs` (no DB; the `now_constant`-style pure family) | NEW (the whole family) — requires `@xterm/headless` in the pin's `node_modules` (it IS in the pin's lockfile `:6462` → `npm ci --offline` in the pin worktree brings it; the ledger `:311` noted it "imported by nothing at the pin" — true at `e8e17b2d9`, false at `322a5488e`) |
| the Ariel writer (`kind: 'screen'` heading, the DEBUG `kind`) | 3 (structural chat_messages) | `ariel_writers_tier3_equivalence` (`harness/oracle/cases/ariel-writers-tier3.ts` + `fixtures/ariel-writers-tier3.json`; a STANDING RED since the `25b8fab6a` gate per the ledger row — MEASURE FIRST, then grow) | `QT_FIXTURE_ARIEL_MAIN` | +3 (`screen` × idle / max-age / session-closed) +1 (`kind` absent → output) |
| `terminal_read` (rendered lines, `buffer`, the `Screen:` sentence, `rawScrollback` tail) | 2 (read-only exact) | `terminal_tools_equivalence` (`cases/terminal-tools.ts`, `fixtures/terminal-tools-tier2.json`; the builder writes transcript files — at the pin the exited-session branch REPLAYS them through xterm, so the oracle's own output changes; v5's seam must feed the rendering produced by ITS emulator over the same bytes — the comparison is then emulator-vs-emulator, which the tier-1 family above underwrites) | `QT_FIXTURE_TERMTOOLS` | + rows: an exited session with `cols`/`rows` stored (20×4 alt-screen transcript), one with NULL geometry (80×24 default), `raw:true` on a >64 KB transcript (the tail), a live-session row is unreachable in the oracle (no PTY) — the host test covers it |
| the definition bytes | 1 | `tool_definitions_equivalence` (regen at the pin) | `QT_ORACLE_TOOL_DEFINITIONS` | none |
| the host driver (chain, resize → screen, exit flush order, `render_screen`) | host integration (no oracle — v4's PTY manager is not oracle-drivable) | `crates/quilltap-host/tests/terminal_pty.rs` (re-pin `ariel_buffer_len` → `ariel_pending`; add: a `?1049h` write then `render_screen` → alternate; resize then render → new cols/rows; the exit flush precedes the close row), `terminal_ariel_e2e.rs` (the Ariel row carries RENDERED text — `printf` output through the emulator equals the cleaned text for a plain line) | `QT_FIXTURE_ARIEL_MAIN` | n/a |

Runtime breaks at the pin for THIS unit's oracles: `build-terminal-sessions-
fixture.ts`, `build-terminal-tools-fixture.ts`, `build-ariel-writers-fixture.ts`
(each creates rows through `repos.terminalSessions.create` → 13 columns; any
committed `terminal_sessions` dump with 11-column rows reds on the extra
`cols`/`rows` cells — `ariel_writers_tier3_equivalence`'s table-4 dump and
`terminal_sessions_tier2`'s); the oracle `cases/terminal-tools.ts` calls
`executeTerminalReadTool` whose exited branch now needs `@xterm/headless`
(present in the pin's lockfile). Nothing in this slice imports a deleted
export.

### 4.2 Scenario sort

| unit | tier | family | new cases |
|---|---|---|---|
| `scenario_sort_key` / `compare_scenarios` / `sort_scenarios` | **1 exact** vs v4's REAL `lib/scenarios/sort.ts` | NEW rows in `scenario_pure_equivalence` (`harness/oracle/cases/scenario-pure.ts` already imports the scenarios-from-anywhere pure exports — add the three) | the 10 test cases + an ICU grid: `The`/`the`/`THE`, `the-attic`, `the_attic`, `The\u{A0}X` (NBSP — JS `\s` matches it), `Thé` (no strip), accents at `base` (`é`/`e` tie → the raw-title leg), case ties (`gala`/`Gala` → raw-title leg → `path` DEFAULT leg), empty title, only-`The`, a `path`-less pair (`'' < 'a'`), surrogate pairs |
| `build_scenario_list` order + the DEBUG line | 2 | `scenario_location_equivalence` (its captured-line whitelist `:83` — add `Scenarios: built list` ONLY if the lane wants it pinned; v4's oracle whitelist `scenario-location.ts:60` is the same list, so both sides must add it) + `scenarios_routes_equivalence` + `chat_scenario_routes_equivalence` (regen at the pin; the fixture's titles must include an article-led and a case-crossing pair or the sort is vacuously equal — add rows: `The Zeppelin`, `casino`, `Ballroom`, `untitled-dock`) | +4 docs |
| the vault fold | 2 | `vault_read_overlay_equivalence` (`cases/vault-read-overlay.ts`; the fixture's `Scenarios/` needs ≥3 files whose path order ≠ title order, and a character whose promoted default would MOVE) | +3 files |
| the SPA select | SPA spec | `apps/web/src/app/scenario/scenario-select.spec.ts` (+ a `scenario-sort.spec.ts` twin of the jest file; the browser's `localeCompare` is jsdom's ICU — same engine as v4's jest, so the ten cases run verbatim) | 10 |

### 4.3 Bug 133

| unit | tier | family | note |
|---|---|---|---|
| `is_uncensored_image_profile` | 1 | NEW rows in `appearance_sanitize_gate`'s oracle or a `dangerous_content_pure` case: `{null, compatible:true, compatible:false+desk match, compatible:false+no desk, desk '' (Boolean('') false), desk match with compatible:undefined}` | 6 |
| `sanitize_appearances_if_needed` (7-param) | 3 | **`appearance_sanitize_gate_tier3_equivalence` RE-SHAPED**: the grid axis `isDangerousChat × routesDangerousToUncensored` → `imageTargetUncensored {t,f}` (policy {off, Moderated+pre-screen, Moderated no pre-screen, Unmoderated, Locked} × target × classification); the oracle `appearance-sanitize-gate.test.ts:283-291` RUNTIME BREAK (eight positional args into a seven-param function — `c.routesDangerousToUncensored` lands in `cheapLLMSelection`, `selection` in `userId`, `spec.userId` in `chatId`, `chat-…` in `mode`); the fixture json's 30 rows re-cut (each row's two booleans → one); the new DEBUG line captured both sides; the completion-call counts re-derived (rule 1 → 0 calls, rule 2 → 0, safe → 1, sanitize → 2) | the Rust test `:113, :289-290, :456` field renames |
| the reroute early return | 2/3 | `tool_image_generation_equivalence`, `wardrobe_item_image_generation_tier3_equivalence`, `story_background_job_tier3_equivalence` (+ the avatar family — grep `resolve_image_provider_for_dangerous_content` callers' tests at lane start) | + a row whose ORIGINAL profile is `isDangerousCompatible` (and one that IS the desk) with the Concierge Unmoderated — v4 now stays put (`rerouted:false`, the reason text); previously it swapped to the understudy |
| the tool's `effective profile` | 3 | `image_generation_tier3_equivalence` (`detect_only_sanitizes_appearance` keeps measuring the production call; add a row where the pre-screen REROUTES to the desk, so the sanitizer is told "bound uncensored" — v4's new unit test's first case) | +2 |
| story backgrounds | 3 | `story_background_job_tier3_equivalence` (rows: Moderated chat on an own-uncensored profile → candid craft + NO reroute; Moderated on the desk profile; Unmoderated with NO desk → candid, no reroute; Locked on moderated → concealed; the policy DEBUG's two new keys captured) | +4 |
| `tool_routes_dangerous_to_uncensored` + `mod bug_133_tests` | — | DELETE (v4 deleted the predicate) | — |

### 4.4 `/photos`

| unit | tier | family | new cases |
|---|---|---|---|
| the roll-up filter, `mountPointId`, the DEBUG, `Album not found` | 2 | `crates/quilltap-web/tests/photos_routes_equivalence.rs` over `cases/photos-routes.test.ts` (route-level; add `?album=<vault>`, `?album=<store>`, `?album=<disabled>` → 404 `{error:'Album not found'}`, `?album=` → Zod 400, `?album=nope`) + the service-level cases if a `photo_tools`/`user_gallery` family exists (`photo_tools_equivalence` is the tools' — check); the fixture must hold a character-vault `photos/` link so `list_default` MOVES (if it does not, add one — otherwise the filter is vacuously equal, memory note `tier-r-vacuous-equal-rows`) | +5 |
| `?action=albums` | 2 | the same route family (`mod.GET(mockRequest(…?action=albums))` → `{albums}` — `photo_album_options_equivalence` already pins the array; here only the envelope + DEBUG) + `?action=bogus` → the `Unknown action` envelope + bare `?action=` | +3 |
| `DELETE` / `GET /photos/{id}` on a vault link | 2 | the same family (`delete_vault_link` → 400 `Link is not a gallery entry`; `get_vault_link` → 404 `Gallery entry not found`) | +2 |
| the census | web test | `dispatch_wrong_type_census.rs` `COLLAPSE_CENSUS` +1 row for the new verb | +1 |

### 4.5 Bug 256

| unit | tier | family | note |
|---|---|---|---|
| the three producers | 2 (read) | `doc_edit_path_resolver_equivalence` — the oracle `doc-edit-path-resolver.ts:201-235` passes `characterId: A` / `{ characterId: A }` / `buildDocStoreUriResolver(A)`: with tsx these are IGNORED extra arguments (no type-check), so the case RUNS and `uri-self` / `res-self` come back `qtap://<vault name>/Mail/x.md` (name-form; the vault's name is set by the fixture — `mountPointName: ''` is passed for `uri-self`, so it falls to the AMBIGUOUS/empty-name branch = the UUID form `qtap://<uuid>/…` — measure); the Rust side drops the `character_id` args; rename the rows to `uri-vault` / `res-vault` | the two resolver texts (`sentinelize`d messages) move |
| `doc_list_files` / `doc_grep` URIs | 2 | `doc_enum_equivalence` (regen; the fixture's self-vault rows now print the vault NAME — ambiguous-name check applies) | — |
| `doc_read_file` uri | 2 | `doc_text_equivalence` (one `uri` field) | — |
| the Librarian | 3 | `post_office_librarian_equivalence` (`ds-self` ×3 → the bare-path sentence + `a character's vault`; the new DEBUG line captured) | — |
| Document Mode's row + `publishedMountPoint` | 2 | the P4.6w Document Mode family (grep `chat_documents` dumps: `document_mode_*` / `doc_edit_ui_*` at lane start) — add a case opening `mount_point: 'self'` for an existing file (row carries the vault NAME) and a NEW blank file (row keeps `self` — v4's unchanged branch, measure) | +2 |
| self_inventory | 3 | the `self_inventory_*` families (the vault section's `uri`s named; a planted `chat_documents` row with `mountPoint:'self'` → `uri:''` + the `unknown store:` howToReach; the footer bytes) | +2 |
| the resolver texts | 2 | `doc_edit_path_resolver_equivalence` (MISSING_CONTEXT / NOT_FOUND rows) | — |

### 4.6 The toast

SPA only: `apps/web/src/app/ui/toast-container.spec.ts` (or `toast.service.spec.ts`)
— a computed-style assertion is not meaningful in jsdom; pin the CSS literal
with a source-text guard (the `_toast.css` `z-index: 1000` line) the way the
gallery host-class lint catch was pinned (`e8e17b2d9` round §3). Optional
Playwright beat: open a dialog, raise a toast, assert the toast's bounding
box is not covered (`elementFromPoint`).

---

## 5. Proposed lane cut

### Lane A — bug 255 (its OWN lane; GATED on the crate ruling; stacks on the keystone's `fresh_schema.json` re-dump only for `provisioning_equivalence` 1d)

Owns (v4 → v5): `lib/terminal/{screen,pty-manager,types}.ts` →
`crates/quilltap-core/src/terminal_screen.rs` (NEW; `TerminalScreen`,
`ScreenRendering`, `ScreenBufferKind`, `ScreenSummary`, `SummaryKind`,
`render_transcript`, `SCREEN_SCROLLBACK_LINES`), `crates/quilltap-host/src/
terminal/{mod,scrollback,protocol}.rs`; `lib/tools/{terminal-read-tool,
handlers/terminal-handler}.ts` → `crates/quilltap-core/src/tools/terminal.rs`,
`tools/executor.rs` (the seam's signature — the ONLY shared file with any
sibling: the custom-tool / interrupts lanes touch `executor.rs`? the ledger
says `edit_custom_tool` adds a tool → `tools/definitions/data.rs` + the
registry; the seam trait lives at `executor.rs:285-289` — fence it as a
Shared contract, below), `tools/definitions/data.rs:267` (ONE row — the
`edit_custom_tool` sibling ADDS rows; disjoint hunks, same file: declare
it); `lib/services/ariel-notifications/writer.ts` →
`services/ariel_notifications.rs`; the schema → `db/terminal_sessions.rs`,
`db/terminal_sessions_geometry_repair.rs` (NEW), `db/mod.rs` (one line),
`crates/quilltap-host/src/host.rs` (one fenced block), `services/
scheduled_maintenance.rs` (test DDL only); the wire → `crates/quilltap-web/
src/terminal_routes.rs`, `crates/quilltap-tauri/src/terminal_ipc.rs`,
`apps/web/src/app/terminal/terminal-protocol.ts`; `Cargo.toml` (core:
`vt100 = "0.18.1"`), `Cargo.lock`; harness: `terminal_screen_equivalence.rs`
(NEW), `terminal_tools_equivalence.rs`, `terminal_sessions_tier2_equivalence.rs`,
`ariel_writers_tier3_equivalence.rs`, `zero_migrations_rerun_equivalence.rs`
(+1 id), `restore_vintage_state.rs` (+1 boot-align), `tool_definitions_
equivalence` (regen only), `harness/oracle/cases/{terminal-screen.ts (NEW),
terminal-tools.ts, terminal-sessions-tier2.ts, ariel-writers-tier3.ts}`,
`harness/oracle/fixtures/{terminal-screen.json (NEW), terminal-sessions-
tier2.json, terminal-tools-tier2.json, ariel-writers-tier3.json, build-
terminal-*.ts, build-ariel-writers-fixture.ts}`; host tests
`terminal_pty.rs`, `terminal_ariel_e2e.rs`. Help `ariel.md` → the docs lane.
**Bumps:** core, host, web, tauri, SPA (the protocol type), harness frozen.
**Must NOT touch:** `services/provisioning/fresh_schema.json` (keystone),
`terminal_clean.rs` (stays, its family stays), anything under `pascal/`,
`inventory`, the chat spine.

Candidate Shared contracts (name-level):
- `quilltap_core::tools::executor::TerminalScrollbackSource` → renamed/
  widened to `TerminalReadSource { fn read(&self, session_id: &str) ->
  TerminalReadSnapshot }` with `TerminalReadSnapshot { rendering:
  Option<ScreenRendering>, raw: String }` (core-defined; host implements);
  `EmptyScrollback` → `EmptyTerminalRead`.
- `quilltap_core::terminal_screen::{ScreenRendering { buffer:
  ScreenBufferKind, lines: Vec<String>, cols: u16, rows: u16 },
  ScreenBufferKind::{Normal, Alternate}, ScreenSummary { kind: SummaryKind,
  text: String }, SummaryKind::{Output, Screen}, TerminalScreen::{new(cols,
  rows), write(&str), resize(cols, rows), render(), take_summary(), cols(),
  rows()}, render_transcript(raw: &str, cols: u16, rows: u16)}`.
- `ArielTerminalOutputAnnouncement.kind: SummaryKind` (serde `output` /
  `screen`; default `Output`).
- `PtySessionMeta { …, cols: Option<i64>, rows: Option<i64> }` in
  `quilltap-host::terminal::protocol` and `apps/web/src/app/terminal/
  terminal-protocol.ts` (`cols?: number | null; rows?: number | null`).
- `db::terminal_sessions::{TerminalSessionRow.cols/rows: Option<i64>,
  TsCreate.cols/rows: Option<f64>, TsUpdate.cols/rows: Option<f64>,
  mark_session_exited(conn, id, exited_at, exit_code, cols, rows,
  updated_at)}`.

### Lane B — the four smalls (ONE lane; disjoint from every sibling)

Owns:
- **Scenario sort:** `crates/quilltap-core/src/scenarios/sort.rs` (NEW) +
  `scenarios/mod.rs` (one `pub mod`), `db/scenarios.rs` (`build_scenario_list`
  only), `db/vault_read_overlay.rs` (the `Scenarios/` fold `:398-408` only),
  `apps/web/src/app/scenario/{scenario-sort.ts (NEW), scenario-select.ts}`;
  harness `scenario_pure_equivalence.rs` + `cases/scenario-pure.ts`,
  `scenario_location_equivalence.rs` + `cases/scenario-location.ts` +
  `fixtures/build-scenario-location-fixture.ts`, `vault_read_overlay_
  equivalence.rs` + its builder, `scenarios_routes` / `chat_scenario_routes`
  (regen + fixture rows). NOT `scenarios/location.rs`, `everywhere.rs`
  (unchanged by v4).
- **Bug 133:** `services/dangerous_content/{understudy.rs (one pub fn),
  provider_routing.rs (RouteProfile +1 field, the early return)}`,
  `services/appearance_resolution.rs` (the sanitizer), `tools/generate_image.rs`
  (the sanitizer call `:2009-2020`, `RouteProfile` literals, DELETE
  `tool_routes_dangerous_to_uncensored` + `bug_133_tests`),
  `services/story_background_job.rs` (`:336-401, 574-586, 695-740`), the
  avatar + wardrobe-item callers' `RouteProfile` literals (grep at lane start:
  `RouteProfile {` across core — each gains `is_dangerous_compatible`);
  harness `appearance_sanitize_gate_tier3_equivalence.rs` + `cases/
  appearance-sanitize-gate.test.ts` + `fixtures/appearance-sanitize-gate.json`,
  `story_background_job_tier3` / `image_generation_tier3` /
  `tool_image_generation` / `wardrobe_item_image_generation_tier3` (+rows).
  ⚠ `generate_image.rs` is a file the P4.D282/P4.D283 descendants own — no
  sibling in THIS round touches it (the Inventory keystone touches the chat
  spine + tools catalog, not the image tool; the interrupts lane touches the
  tool LOOP, not `generate_image.rs`) — declare the hunks.
- **`/photos`:** `photos/user_gallery_service.rs`, `api/photos.rs`,
  `api/types.rs` (`PhotoGalleryList.album` + `PhotoAlbumsList` — a Request
  enum the Inventory keystone ALSO edits for its own verbs: disjoint variants,
  same file — declare), `api/engine.rs` (one dispatch arm — same caveat),
  `crates/quilltap-web/src/photos_routes.rs`, `crates/quilltap-web/src/lib.rs`
  (no new path — `/api/v1/photos` already routed; the action dispatch lives in
  `photos_list`), `crates/quilltap-web/tests/{photos_routes_equivalence.rs,
  dispatch_wrong_type_census.rs}` (+1 row), `cases/photos-routes.test.ts`,
  the photos fixture pair (+ a vault `photos/` link).
- **Bug 256:** `doc_edit/uri_producers.rs`, `doc_edit/path_resolver.rs`
  (two texts), `services/librarian_notifications.rs`, `tools/self_inventory.rs`
  (`:945, 1941-1970, 2440`), `tools/doc_edit/{shared.rs (publishedMountPoint
  twin), document_ui.rs, file_management.rs, markdown.rs, text.rs, blob.rs}`,
  `tools/search.rs:658`, `tools/send_mail.rs:182` (signature only); harness
  `doc_edit_path_resolver_equivalence.rs` + `cases/doc-edit-path-resolver.ts`,
  `doc_enum_equivalence`, `doc_text_equivalence`, `post_office_librarian_
  equivalence` + `cases/post-office-librarian.ts`, the Document Mode + self-
  inventory families (regen + rows). NOT `pascal/roster.rs`, `db/database_
  store.rs`, anything `inventory`/`title_lint` (the keystone's lint half).
- **Toast:** `apps/web/src/styles/qt-components/_toast.css` (+ a source-text
  guard spec).
- Help (`general-scenarios.md`, `scenario-browser.md`, `the-concierge.md`,
  `image-generation-profiles.md`, `photo-gallery.md`, `document-editing-
  tools.md`, `ariel.md`) + `docs/v4/` (`API.md`, `DDL.md`) → the docs lane.
**Bumps:** core, web, SPA; harness frozen.

Candidate Shared contracts with siblings:
- `api::types::Request::PhotoGalleryList { album: Option<String>, … }`,
  `Request::PhotoAlbumsList {}` → `Response::PhotoGallery({"albums": [...]})`
  (the Inventory-SPA sibling's selector consumes `GET /api/v1/photos?action=
  albums` and `?album=` — name the query keys `album`, `action=albums`, the
  404 body `{"error":"Album not found"}`).
- `dangerous_content::understudy::is_uncensored_image_profile(profile:
  &RouteProfile, policy: &ResolvedConciergePolicy) -> bool` +
  `RouteProfile.is_dangerous_compatible: bool`.
- `scenarios::sort::{SortableScenario<'a> { title, filename, path:
  Option<&str> }, scenario_sort_key, compare_scenarios, sort_scenarios}`.
- `tools::doc_edit::shared::published_mount_point(resolved: Option<&
  ResolvedPath>, requested: Option<&str>) -> Option<String>`.

---

## 6. Traps

1. **`REAL` vs `INTEGER` (Headline 1).** Also: v4 writes JS numbers → with
   REAL affinity SQLite stores `80` as REAL `80.0`; the tier-2 dumps print
   `80` through `js_number_to_json` both sides — bind `Option<f64>` like
   `exitCode`, NOT `i64`, or the dump may differ by storage class on the
   migration-appended `INTEGER` shape vs the fresh `REAL` shape (two shapes →
   two storage classes for the SAME insert; the oracle corpus must plant BOTH
   table shapes, as P4.D249 did).
2. **xterm vs vt100 text semantics** the tier-1 corpus must include: (a) a
   wide char that does not fit at the row edge (xterm pads a never-written
   cell; `translateToString(false)` in the `continues` branch yields a space
   there; vt100 `write_contents` emits nothing past the last content cell) —
   an edge row with `字` at col 9 of 10; (b) a row filled EXACTLY to the edge
   followed by `\r\n` (xterm: `isWrapped` false on the next row; vt100:
   `col_wrap` sets `wrapped` only when the NEXT char lands — same, measure);
   (c) `ESC[3J` (xterm clears scrollback; vt100 `ed(3)` → check `unhandled`);
   (d) `ESC[?1049h` twice without `l` (xterm clears alt again? vt100
   `alternate_grid.clear()` each time — same); (e) `ESC[?47h` (no
   save/restore); (f) a scroll region `ESC[2;5r` + `ESC[M` (both handle);
   (g) `\t` into tab stops (xterm default 8; vt100 `col_tab` 8); (h) BEL /
   OSC 0 title (ignored both); (i) `ESC[?2026h` sync output (xterm 6 handles;
   vt100 → unhandled, no visible effect); (j) resize 80→40 with a 60-char
   line in scrollback (xterm REFLOWS to two wrapped rows; vt100 truncates —
   **a documented divergence class** unless the twin re-wraps; the ruling);
   (k) a lone `\r` then text (both overwrite — the bug-255 `\r` rule is gone
   with the strip); (l) backspace `\b` (both move the cursor; xterm does not
   erase — same); (m) a combining mark (`e\u{301}`) — xterm stores it in the
   cell's combined data, `translateToString` emits `é` decomposed; vt100
   `text()` width-0 path appends to the previous cell — same bytes; (n) CRLF
   past the bottom with scrollback FULL (5,001 lines: the oldest drops; the
   marker dispose rule); (o) invalid UTF-8 in the transcript (`tail_file`
   starts mid-sequence: v5's `tail_file` decodes lossy → U+FFFD; xterm gets
   the Buffer's `toString('utf8')` → U+FFFD too; vte would ALSO accept raw
   bytes — feed the lossy String's bytes, not the file bytes, to match).
3. **`takeSummary`'s marker is the CURSOR row at summary time, not the last
   written row** (`registerMarker(0)`), and the marker advances even when
   the summary is null (`:168-171` order). The `lastSummaryTail` shift
   fires only when the first line is BYTE-equal to the previous summary's
   last line (`$` after `.trimEnd()` — not `$ `).
4. **`render()` on the normal buffer reads `[0, buffer.length)`** — the
   WHOLE scrollback + every screen row, so a 5,000-line scrollback yields up
   to 5,000 + rows lines before the handler's tail/slice; `totalLines` is
   that count (not the ring-buffer line count as before) — the `Lines: N
   (showing a–b of T)` text moves for the same transcript.
5. **`rawScrollback` is the LAST 64 KB of the raw stream in UTF-16 units**
   (`raw.slice(-RAW_RETURN_CHARS)`), no longer the slice's raw twin; for a
   live session `raw` is the 256 KB ring (so the field is at most the ring's
   tail); for an exited one the 4 MiB tail's last 64 KB.
6. **The live branch requires BOTH `ptyManager.get(id)` AND
   `session.exitedAt == null`** (unchanged) — a reconciled-dead row with a
   live manager entry falls to the transcript branch; v5's seam
   (`scrollback.rs:63`) already models this.
7. **The async order in `onExit`:** the DB exit stamp is fire-and-forget and
   does NOT wait for the final flush; the close ANNOUNCEMENT does. v5's
   `exit_sequence` awaits the flush, then stamps, then posts — same observable
   row order (flush row → close row); keep it.
8. **`localeCompare` with `undefined` locale is the HOST default locale** in
   Node (v4's jest runs under the machine's locale → en-US on Friday); v5's
   `collation.rs` is pinned to `en-US`. The oracle must run with
   `LANG=en_US.UTF-8` (the standing distill-TZ-style pin) — the SPA twin
   runs under the browser's locale (both sides the same browser in Playwright).
9. **The sort's 4th leg uses DEFAULT `localeCompare`** (tertiary): `'a' <
   'A'`? ICU en-US tertiary orders lower before upper (`a < A`) — `locale_
   compare` reproduces it; do NOT use `locale_compare_base` there.
10. **`scenarioSortKey` strips only when something remains** (`'The'` →
    `'The'`, `'The '` → `'The'` after trim, `'The-'` → `'The-'`? NO: `'The-'`
    → stripped `''` → keeps `trimmed` `'The-'`); the regex needs the `[\s_-]+`
    tail — `'Theatre'` untouched.
11. **`buildScenarioList`'s default-conflict walk stays in PATH order** — the
    winner is still the alphabetically-first FILENAME; only the returned
    array is re-sorted. A naive "sort first" port changes the winner.
12. **The vault fold's `filename` key is `doc.fileName` minus `.md`, NOT the
    relative path** — for `Scenarios/x.md` they coincide; for a nested file
    they do not (v4's vault scenarios are flat, but the key is the file name).
13. **`sanitizeAppearancesIfNeeded`'s rule 1 now SKIPS the classifier** even
    when the pre-screen is on — the completion-call count for a
    target-uncensored row is 0 (was 1 under the old rule 3 + step 4).
14. **Story backgrounds: `uncensoredImageTarget` (the crafter's candid flag)
    and the sanitizer's argument are now DIFFERENT expressions** (`forced ||
    paints || isDangerousChat` vs `forced || paints || routesDirectToDesk`) —
    an Unmoderated chat with NO desk crafts candidly AND skips sanitization
    (rule 2: `!preScreen.enabled` — Unmoderated has no pre-screen) — measure
    both in the corpus.
15. **The pre-flight reroute's early return comes AFTER the policy gate** —
    a Locked / off-duty chat still answers the gate's reason, not the
    "already uncensored" one.
16. **`isUserGalleryLink` does a `findById` per link on remove/get** (one
    read) — v5's fallback wrappers (`find_by_filter_or_empty`) apply; a
    degraded mount index → `!mountPoint` → refused as "not a gallery entry"
    (v4's shape; keep).
17. **`Album not found` is matched by `startsWith`** on the message; the 404
    body is `Album not found` (the resource word), NOT the thrown
    `Album not found: <id>`.
18. **`withCollectionActionDispatch` with a DEFAULT handler**: no `action`
    → the gallery; `?action=` (bare) and unknown → the `Unknown action`
    envelope listing `availableActions: ['albums']` (v4 `actions.ts:168-183`
    — check the envelope's exact keys in v5's existing twin).
19. **`publishedMountPoint` falls back to the REQUEST outside document_store
    scope** (project/general keep `input.mount_point`, which may be
    `undefined`); the NEW-file branch of `handleOpenDocument` never calls it
    (`storeName` stays the typed value) — port the asymmetry, don't "fix" it.
20. **The Librarian's `self` arm is now reachable only from legacy rows**
    (announcements pass the resolved name) — the oracle must PLANT
    `mountPoint:'self'` (it already does, `post-office-librarian.ts:67, 78`).
21. **`buildContextFileUri` returns `''` for `self`/null, and
    `howToReach` branches on `!uri`** — a `uri: ''` field is emitted (not
    omitted) in the JSON section.
22. **The resolver's NOT_FOUND text embeds `context.mountPoint` raw** (no
    quoting beyond the literal `"…"`); MISSING_CONTEXT's text contains an em
    dash `—` (U+2014).
23. **The toast comment names v4's React tiers** — v5's are the same numbers
    (§2.6); do not "normalise" to a token.
24. **Help diffs are the docs lane's** but `ariel.md`'s new paragraphs
    describe behaviour THIS lane ships — vendor after Lane A lands.

---

## 7. Tiering suggestion

**Tier 1 (must land):** the FIFTEENTH move's ensure (two shapes) + the
`terminal_sessions` columns on create/exit/read/meta; `TerminalScreen` on
`vt100` with the eleven seed streams exact; `terminal_read` through the
emulator (live + transcript replay at stored geometry, `buffer`, the
`Screen:` sentence, the 64 KB raw tail, the definition bytes); the Ariel
chain with `kind:'screen'` snapshots + the two headings; the exit-flush-
before-close order (already v5's); `compare_scenarios` + the three
re-sorts + the SPA optgroup sort; `is_uncensored_image_profile` + the
seven-param sanitizer + the reroute early return + the tool's effective
profile + the story-background flags; the `/photos` vault exclusion +
vault-link refusals + `?album=` + `?action=albums`; every bug-256 producer +
`publishedMountPoint` + the Librarian bare path + self_inventory + the two
resolver texts; the toast.

**Tier 2 (should land):** the DEBUG lines captured both sides (`[PTY] Ariel
summary taken`, `Listing user gallery`, `[Photos v1] Listed albums…`,
`Scenarios: built list`, `[AppearanceResolution] Image bound…`,
`[DangerousContent] … already uncensored`, `[LibrarianNotification] No store
name…`, `[TerminalScreen] Failed to resize emulator`, `[PTY] Failed to render
terminal summary`); the ensure's own differential family; xterm-parity edge
rows (§6.2 a–o) with each measured divergence RECORDED as a named table (not
silenced); the host integration beats.

**Tier 3 (explicit typed refusals / recorded divergences):** reflow on
resize (`SCREEN_RESIZE_NO_REFLOW` — unless the ruling asks for it);
`?2026`/`CSI t`/DCS passthrough differences (`SCREEN_UNHANDLED_SEQUENCE`);
the prettify label (NO-PORT by precedent); `jest.setup.ts` /
`package.json test:unit` (NO-PORT); the `tool-definitions-snapshot` jest
snapshot (v4-test-only; v5's `data.rs` row is the twin).

---

## 8. Open questions / rulings owed / disagreements with the ledger

1. **The crate ruling (owed before Lane A opens):** `vt100` 0.18.1
   (recommended) vs `alacritty_terminal` 0.26.0 (reflow parity, heavier) vs a
   `wezterm-term` git dependency (not on crates.io — against the "registry
   crates only" practice this repo has kept) vs hand-rolling on `vte`.
2. **Reflow on resize:** accept `SCREEN_RESIZE_NO_REFLOW` as a recorded
   divergence (live sessions resize rarely; `renderTranscript` never
   resizes), or require the twin to re-wrap the normal buffer on `cols`
   change (a ~200-line addition over vt100's `Row`s)?
3. **Fresh shape `REAL` vs the ledger's `INTEGER`** (§3): the ledger row
   `:406`, the §1 note `:251-268` and `DDL.md:867-868` say `INTEGER DEFAULT
   NULL` for the generateDDL render; `schema-translator.ts:320-328` + the
   committed `exitCode REAL` say `REAL` with no default. The keystone's
   re-dump settles it mechanically; the order should NOT hand-write either.
4. **Is `terminal_read`'s rendered text a Tier-1 recorded-stream family or a
   documented divergence class?** Recommended: Tier 1 over the eleven seed
   streams + a captured Claude-Code transcript (the 422 KB `Friday` session
   `1bcfb8f1`'s transcript, if the human can supply a COPY — the bug doc's
   motivating case; else a synthetic alt-screen stream with ~2,900 `ESC[H`
   repaints), with the §6.2 edges each measured and named.
5. **`@xterm/headless` in the pin worktree:** the ledger `:311` records it
   as "imported by nothing at the pin" (true at `e8e17b2d9`); at `322a5488e`
   it is imported by `lib/terminal/screen.ts` and MUST resolve in the pin's
   `node_modules` for `terminal-tools.ts`, `ariel-writers-tier3.ts` (via the
   PTY manager import? — the writer case imports `lib/services/ariel-
   notifications` only; the PTY manager is not imported, so only
   `terminal-tools.ts` and the NEW `terminal-screen.ts` need it). `npm ci
   --offline` from the pin's lockfile carries 6.0.0.
6. **Ledger row `:406` says the exit-stamp "now writes `cols` / `rows`" and
   `:410` says "the client half is the Inventory-SPA sibling's"** — agreed;
   but the row's "the `raw` parameter description and the `terminal_read`
   tool DESCRIPTION both change" omits that `TerminalReadOutput.buffer` is
   ALWAYS present (`'normal'` when no rendering) — a key-order change in the
   output JSON the tier-2 read family pins.
7. **Ledger row `:404` names `scenarios/location.rs` and a `vault_overlay.rs`
   twin** — at `5a94a9d1e` there is no `db/vault_overlay.rs`; the one vault
   fold is `vault_read_overlay.rs:398-408` (+ the prompts fold `:387` and the
   wardrobe fold `:575`, both untouched by v4). Lane B edits ONE site there.
8. **Ledger row `:408` promises "both-direction bug-133 pins" by name** — the
   harness has no named carve; the trip is the grid family's INPUT shape
   (§2.3). The order should say "re-shape the grid", not "retire the carve".
9. **`path_resolver.rs:1101` carries a THIRD copy of the NOT_FOUND text**
   (v4 has one at `:526`) — measure whether it is a test or a second arm
   before changing it.
10. **`handleOpenDocument`'s new-file branch keeps the typed `self`** in the
    `chat_documents` row (§1.10) — a v4 residue to port as-is and record, or
    to file upstream? Recommended: port as-is + file (the P4.D322 class).
11. **The `/photos` fixture pair:** if `photos-{main,mount}.db` holds no
    character-vault `photos/` link, the exclusion is vacuously equal — the
    order must add one (memory note `tier-r-vacuous-equal-rows`).
