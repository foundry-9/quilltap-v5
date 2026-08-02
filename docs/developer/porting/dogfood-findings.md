# Dogfood findings — the Friday smoke (started 2026-07-10)

The running log of findings from browsing a COPY of the real Friday instance
through `quilltap-web` + the SPA. Each finding is either fixed-in-place (with
its commit) or promoted into the next work order. The common thread so far:
**fresh-`generateDDL` schema vs the migration-accumulated schema of a real
instance** — the one divergence class synthetic fixtures structurally cannot
catch, since every fixture is built fresh.

| # | Finding | Class | Status |
|---|---|---|---|
| 1 | Salon list: `Invalid column type Integer … isSilentMessage` | Migration affinity — the `add-silent-message-field` migration declared INTEGER where fresh DDL says TEXT; the strictly-`String` read refused integer cells | **FIXED** `bcaa744` — `put_is_silent` reads the raw sql value (Integer/Real/Text coerced uniformly); regression tests over both table shapes; migrations audit found no other read-breaking affinity divergence |
| 2 | Chat GET: `no such column: timezone` | Never-migrated column — v4 added `chat_settings.timezone` with NO migration; its `SELECT *` reads tolerate the absence, the port's explicit column list errored | **FIXED** `bb71652` — `db::tolerant_select_list` (PRAGMA table_info → missing columns substituted `NULL AS "col"`), applied to `chat_settings::find_by_user_id`; `sidebarWidth` extraction NULL-tolerant; `settings_routes_equivalence` re-verified |
| 3 | A large Salon chat renders for 10+ s and lands stuck at the top (console: `'setTimeout' handler took 10196ms`, no errors) | TWO distinct causes — see #3a/#3b | split |
| 3a | NO chat could scroll at all (an 80-message chat reproduced it) | The scroll chain was broken for every chat: the v5 shell dropped v4 `app-layout.tsx`'s inner `flex-1 min-h-0 overflow-y-auto` scroller wrapper around the page content, and two unstyled Angular component hosts (`qt-salon-conversation`, `qt-message-list`) broke the flex/height chain React never has — `.qt-chat-messages`' own `overflow-y-auto` never got a bounded height. Fixture chats FIT the viewport and the e2e never scrolls, so it slipped through | **FIXED** — the shell scroller wrapper restored + `host:` classes on both components (`block h-full` / `flex flex-col flex-1 min-h-0`); a real scroll e2e beat lands with the long-chat fixture the virtualization deliverable needs anyway |
| 3b | The 10+ s synchronous render on a LARGE chat | No virtualization — every message renders through the full markdown pipeline in one task | **FIXED** (P4.6h) — the message list is virtualized with `@tanstack/angular-virtual` (a port of v4's own tanstack-virtual + `useAutoScroll` architecture) and markdown is memoized per `(content, renderingPatterns, dialogueDetection)`, so only the viewport + overscan rows pay the render cost. A separate ~300-message `salon-long-*.db` fixture backs the new `e2e/salon-scroll.spec.ts` (interactive < 3s, lands at bottom, windowed DOM, jump-button round-trip) |
| 4 | Clicking a character card on `/characters` does nothing unless the click hits the name/avatar exactly | Port divergence — v4's `AuroraView` card is clickable ANYWHERE (`cursor-pointer` + `handleCardClick`, which ignores clicks landing on inner buttons/links); the v5 card only linked the avatar+name row. The e2e never caught it because it clicked the name link directly | **FIXED** — the card div carries v4's whole-card click (the `closest('button')`/`closest('a')` guard preserved); a unit test proves navigate-from-body / no-navigate-from-star, and the e2e's detail-open beat now clicks the card BODY |
| 5 | The System Prompts view tab renders a prompt containing the character's name as scattered fragments with huge gaps — each name chip floats alone mid-screen | Port divergence (Angular-mechanics class, not schema) — v4 renders the body via a shared `TemplateDisplay` component inside `<pre><code>`; v5 had INLINED that markup into the tab template, and Angular preserves a template's literal whitespace inside `<pre>` elements, so every highlight segment rendered wrapped in the template's own newlines + ~20-space indentation. Fixture prompts are short one-liners, so nobody eyeballed it | **FIXED** — v4's `TemplateDisplay` ported as the shared `qt-template-display` (its own template compiles OUTSIDE any `<pre>`, so default whitespace stripping applies); both the System Prompts and Details tabs now use it (the Details tab's `div` + CSS `pre-wrap` variant wasn't affected but shared the duplicated markup); a unit test asserts the rendered `<pre><code>` textContent is BYTE-EXACT to the prompt content |
| 6 | The Default Settings tab "doesn't seem to accept edits" on Friday data | TWO port divergences, both Angular-mechanics class. (a) Error surfacing: v4 wraps every defaults save in try/catch + `showErrorToast`; v5's `save()` had `try/finally` with NO catch — a failed save would revert silently. (b) The actual cause: `<select [value]>` with async-loaded options — the value binding fires BEFORE the profiles/partner options render, the assignment finds no matching option and silently resets to `""`, and it never re-fires when options arrive (React re-renders `<select value>` after options change; Angular doesn't). So saves were in fact SUCCEEDING all along — the select just never displayed the stored value, so every render read as "the edit didn't take". Diagnosed live against the Friday copy: the character's stored `defaultConnectionProfileId` was among 36 rendered options while `select.value` was `""`; fixture characters have NO stored profile, so the fixture never exercised a non-empty value + async options | **FIXED** — (a) tab-level `qt-alert-error` with v4's fallback microcopy per control (`ba216ec`); (b) the profile/partner/prompt/scenario selects bind `[selected]` per option instead of `[value]` on the select (re-applies when options render); regression tests set the options/id inputs AFTER first render and assert the stored value displays. Verified live on the Friday copy: stored values display, an edit round-trips, no alert. ~8 more `[value]`+dynamic-options sites exist (settings modals, wizard, create screen) — audit filed in the standing notes |
| 7 | Multi-character chain: after the user sends, each OTHER character's finished response never appears — only the status notices ("consulting memories…") — until the whole chain ends and it's the user's turn again | Port divergence — the stream reducer DOES fold each chained turn's finished bubble into `ChatStreamState.messages` on intermediate `done` (and mid-turn Carina answers + Host announcements land there too), but `qt-streaming-message` renders ONLY the live buffer (status/content/reasoning/tools) and `salon-conversation.displayMessages` merges only the canonical chat + optimistic user bubble. On `turnComplete` the live buffer resets, so the finished text vanishes until the post-dispatch refetch. v4's `useSSEStreaming` appends each completed chained message to the visible list as its `done` arrives. Fixture e2e never caught it because the walks are single-responder | **FIXED** (P4.6am, unified `7c76aee`) — render-side only: `buildStreamRenderItems` shows finished chained / Carina / Host bubbles as they complete, deduped by server messageId against the canonical flow; the reducer was already faithful. Component specs are the proof (no multi-responder LLM in the e2e host) |
| 8 | No way to make Return insert a newline instead of sending | Unported v4 feature — v4 has a per-chat **composition mode** toggle in the composer (chat mode: Enter sends / Shift+Enter newline; composition mode: Enter newline / Cmd-or-Ctrl+Enter sends) plus `CompositionModeDefaultSettings` in the Chat settings tab. v5's `chat-composer` ships chat mode only (Shift+Enter DOES insert a line break today). Not named in any order's deferral list until now | **FIXED** (P4.6al, unified `7c76aee`) — composer toggle + `submitOnModEnter` in `qt-rich-editor`'s keymap, per-chat persistence via `documentEditingMode` ↔ `chatUpdate`, and the Settings→Chat "Composition Mode" default card; live e2e beats incl. the persisted-flag reload |
| 9 | Chat background images don't display | Unported surface — the aesthetic-settings data layer landed (P4.6k/P4.6c server-side; Prospero cards edit it) but the Salon never renders `backgroundImage` (v4 applies it in the salon view); no v5 salon component references it | **FIXED** (P4.6ak+P4.6am, unified `7c76aee`) — the `::before` CSS was already ported; the fix was the data wire: `chatGetBackground` (all three arms, server-differentialed) fetched once per chat open, `--story-background-url` bound on the layout root, fileId → the store-backed byte route; live e2e beat. Background GENERATION (regenerate + 30s poll + settings card) stays a loud refusal |

| 10 | In a SENT chat message, `==highlight==` renders literally (equals signs visible) while `*word*` typed literally in the composer renders italicized | **Faithfully ported v4 behavior** — the composer dialect and the message-render dialect differ IN V4: the editor supports `==`→highlight and deliberately keeps single `*` literal (roleplay `preserve*`), but the message renderer is plain `remark-parse + remark-gfm + remark-breaks` — no `==` extension, and `*word*` is standard CommonMark emphasis. Proven by running v4's REAL `lib/services/markdown-renderer.service.ts` (`renderMarkdownToHtml`, default options): `a ==highlight== test` → `<p>a ==highlight== test</p>`; `a *word* test` → `<p>a <em>word</em> test</p>`. v5's observed output is byte-identical | **NOT A BUG** (2026-07-15) — recorded so it isn't re-reported. **DECIDED 2026-07-15:** the renderer stays v4-faithful (no `==` extension); highlighting in messages is the province of roleplay-template rendering-pattern classes, not the markdown pipeline |
| 11 | `/files` has no upload control | **Faithfully ported v4 behavior** — v4's `FilesView` renders `FileBrowser` WITHOUT `showUpload` (default false), and even with it the upload input requires mount mode or a projectId (`FileBrowser.tsx:171,657,866`): the general files page has no upload UI in v4 either; v5's `files-browser.ts:48` docstring already records the parity. Uploads reach general files via the chat composer attach and the REST/API legs. (Diagnosis side-proof: `/files` is GENERAL files only — v4 `findGeneralFiles` = `projectId IS NULL` — and the display was verified COMPLETE against the Friday copy: 50 general rows of 1741 total, 46 webp + 4 markdown, exactly what renders) | **NOT A BUG** (2026-07-15) — the walk script overpromised a /files upload button; corrected here |
| 12 | Tauri shell vs the Friday copy: every image renders as a broken "?" chip (salon-list avatar stacks + chat-card images; message avatars etc.) | **P4.7 scope gap — server-supplied relative URLs.** The P4.7b §3 `apiUrl()` adoption covered every client-BUILT URL, but the server mints RELATIVE paths inside response bodies — `avatarUrl`/`filepath`/`backgroundUrl` DTO fields (`chat_enrichment.rs`, `chat_media.rs` — `/api/v1/files/{id}`, `files/proxy/…`), plus `/api/v1/files/…` links inside server-rendered markdown/courier bodies — and the client markdown store-image rewrite builds its blob URL inline (`markdown-renderer.ts:98`), bypassing `apiUrl`. Relative = same-origin in a browser (works; the wire format is v4-faithful and byte-pinned by differentials — do NOT absolutize server-side); in the Tauri webview the page origin is `tauri://localhost` (the bundled dist), so relative `/api/v1/…` 404s. Invisible on the staged M5 instance (the salon fixture has no avatar images); Friday surfaces it instantly | **CLOSED (2026-07-18) — the human visual confirmation ran on the Friday copy under the rebuilt bundle: all four checks green (salon-list avatar stacks, chat-card images, in-message/courier images, a story background), "fully functioning, no problems to report".** The fix history follows. The one-origin spike ran GREEN and was ADOPTED: the Tauri window ships on `qtap://localhost/` (the qtap handler serves the embedded dist for non-API paths and delegates `/api/*` into the reused router), so every server-relative URL — including inside pre-rendered bodies — resolves; the fallback (b) was never taken and no render seam changed. `apiUrl()` is identity on a qtap-origin page. The visual proof (avatar stacks / chat-card images / in-message images / a story background on the Friday copy) rides the combined M5 + finding-#12 human walk staged in `work-orders/p4.7c-tauri-one-origin.md`. Two candidate fixes: **(a) one-origin (recommended, spike first):** serve the SPA itself off the `qtap` origin (window URL `qtap://localhost/` — the reused router already carries `static_serve` + index fallback), making EVERY relative URL resolve correctly, including inside pre-rendered HTML bodies; spike must check WKWebView custom-scheme-page behavior (localStorage/history/devtools). **(b) render-seam normalization:** route server-supplied paths through `apiUrl()` at the chokepoints (`normalizeAvatarSrc`, the enriched-image `filepath` consumers, `markdown-renderer.ts:98`, courier-rendered hrefs) — more sites, and pre-rendered HTML is the hard case |
| 13 | The Salon's Chat Photos gallery: the thumbnail-size slider does nothing | Port divergence (Angular/CSS-mechanics class) — the slider was wired correctly all along: it moved `sizeIndex`, `thumbnailSize()` recomputed, and the server was asked for a correctly-sized thumbnail (`?action=thumbnail&size=…`). What never moved was the rendered box. v5 passed the size to the `<img>` as HTML `width`/`height` **attributes**, which are only a fallback for CSS — and the shared `.qt-chat-attachment-image` class hard-codes `width: 5rem; height: 5rem` (`_chat.css:2268`), so every thumbnail stayed pinned at 80px through all six sizes. v4 never sizes the image: `PhotoGalleryModal.tsx:272` puts an inline `style={{ width: thumbnailSize, height: thumbnailSize }}` on the **button container** and the img fills it (`w-full h-full object-cover`). The unit specs asserted the thumbnail URL (which was always right) and the e2e never touches the slider, so the split between "requested size" and "rendered size" slipped through | **FIXED** — the gallery button carries `[style.width.px]`/`[style.height.px]` bound to `thumbnailSize()` (inline style outranks the class) and its img fills the parent, matching v4; the CSS is untouched, so the class keeps its fixed 80px box for its two other consumers (message-row attachments, save-image dialog). A regression spec drives the slider to max and asserts BOTH the rendered box (200px) and the URL — the old code passed the URL leg and failed the box leg, which is the exact split reported |
| 14 | The M5 walk under the Tauri shell: Cmd+R does nothing (the deep-link reload beat unexercisable by keyboard) | **P4.7 shell scope gap — the #12 flavor (a browser affordance the shell must supply itself).** v4 is browser-hosted, so page reload comes free from the browser chrome; macOS routes key equivalents through the app menu, the shell configured NO menu (bare `tauri::Builder::default()`), and Tauri's default menu carries no Reload item — the keystroke was silently dropped before reaching the webview. The §3 index fallback (the server half of reload-survival) was in place all along; there was simply no way to trigger a reload. Invisible to the e2e (no tauri-driver on macOS) and to the sandbox (window content unverifiable headless) — the M5 recipe asserted a gesture no shell code carried | **FIXED** — `menu::build_app_menu`: Tauri's default menu with `View → Reload` (`CmdOrCtrl+R`) prepended, `on_menu_event` reloading the main webview in place; macOS-gated (on Windows/Linux an app menu is a visible menubar the bare builder never showed, and those targets are the standing unverified deferral — wire the accelerator there when they first build). Guard: the harness-free `menu_contract` suite (muda refuses menu construction off the platform main thread, so the test binary runs `harness = false`) pins View → Reload + the surviving default submenus. **Visual half CONFIRMED by the human (2026-07-18):** Cmd+R reloads and lands back on the same deep route, tested on a conversation and in Settings — the finding is fully CLOSED |
| 15 | The unlock screen is barely readable — heading, body, label, placeholder, and the Unlock button all render dim gray on near-black (reported from the Tauri M5 walk; reproduced identically in the browser deployment — NOT Tauri-specific) | Port divergence — v4's `ThemeProvider` mounts inside the ROOT layout, wrapping every page including `/unlock`, so `.dark`/`.light` + `data-theme` are stamped on `<html>` before any page paints. v5 moved that DOM-stamping into `ThemeService`, but only the post-unlock `Shell` (and the Appearance tab) inject it — so every PRE-unlock screen (unlock, setup wizard, startup/error) rendered with NO theme scope: text resolved the `:root` light-mode foreground (near-black) on the hard-coded dark auth backdrop (`--qt-auth-page-bg`), and `qt-card`'s background variable didn't resolve at all (transparent card). Invisible to the e2e (it asserts text/roles, not contrast) and to every prior human pass (the unlock beat was walked pre-P4.6e theming, before the class-scoped variables landed) | **FIXED** — the `App` root constructs `ThemeService` (one `inject()` in the constructor), stamping the theme from localStorage + the system preference before the gate screens paint — exactly v4's pre-auth behavior; the Shell keeps the server-preference reload. Guards: an app.spec case asserts `.light`/`.dark` + `data-theme` are on `<html>` when the unlock state renders; the foundation e2e asserts `html.light, html.dark` BEFORE unlock. Verified visually on the locked instance in the browser (dark mode resolves, card + button paint) |

| 16 | New-chat avatar auto-generation in a PROJECT chat: Aurora announces a new portrait "attached here, catalogued under uuid …" but no image renders in the announcement, and every newly generated avatar is a broken image (the file route 500s); a pre-existing avatar displays fine | TWO stacked port divergences, surfaced only on real data. (a) `get_project_document_store` read `project_doc_mount_links` on the MAIN connection — the table lives ONLY in the mount-index partition on a real instance (`fresh_schema.json` `/mountIndex`; synthetic fixtures never exercised the production seam, whose corpora mock the upload), so the fail-soft `.ok()?` turned `no such table` into "project has no linked store" for EVERY project. (b) The `ProjectImageUpload` seam was infallible (a flagged trait-shape gap) and buried the resulting error in a `fs-seam:error:<msg>` sentinel storageKey, so the job "succeeded": files row + `characterAvatars` + `avatarOverrides` written pointing at an unservable file, Aurora announcement posted with a dead uuid, `/api/v1/files/{id}` 500s. v4 throws from `uploadFile` and the JOB FAILS before any of that (with a remediation message) | **FIXED** `5dcb871b` — the links lookup moved to the mount partition (the lone wrong-partition site; a sweep found no other mount-table repo on a main connection), and `ProjectImageUpload` widened to `Result` so an upload failure fails the job exactly at v4's catch-wrap points in both consumers (avatar + story-background), with v4's store-less message byte-for-byte. Regression test provisions the FULL fresh schema (real-instance table placement) and pins both arms; avatar/story/files-routes/courier-images differentials re-ran green over fresh oracles. Live-walk gesture (avatar generation with a real image provider) is not e2e-able — the unit regression is the guard, per the no-provider e2e host precedent. **Human-verified LIVE on the Friday copy (2026-07-22): a regenerated avatar renders in the announcement and the participant cards** |
| 17 | New-chat Play As: choose a character, then revert to yourself — the character stays in the cast under LLM control but "did not switch to an LLM-controlled profile" | **Faithfully ported v4 behavior** — v4's `NewChatForm.handlePlayAsChange` deliberately hard-clears the reverted character to `controlledBy:'llm', connectionProfileId:''` (NOT re-seeding `defaultConnectionProfileId`), its per-character select shows "Select profile...", and the submit guard blocks creation until the user re-picks ("Please select a connection profile for: …"). v5's `applyPlayAs` + guard + payload are line-for-line equivalent | **NOT A BUG** (2026-07-22) — recorded so it isn't re-reported. If revert-should-restore-a-default is wanted, that's a product change to make in v4 first |

| 18 | Wardrobe: "wear" on Friday's "Paris 1925 Casual" omits the top (the 1925 navy dress); the outfit editor shows fewer items than v4 does | Port divergence — the emitter-fidelity trap. v4's frontmatter emitter (eemeli/yaml `stringify`) FOLDS long plain scalars (`imagePrompt:`/`appropriateness:`) at ~80 columns onto indented continuation lines; v5's hand-rolled YAML-subset reader (`markdown::parse_yaml_subset`) treated ANY indented top-level line as a parse error, so the whole doc's frontmatter read as null → `parse_wardrobe_item_file` found no valid `types` → item silently skipped → the composite's slug-ref dropped as unknown. On the Friday copy: 33 docs invisible (30 wardrobe items across all characters incl. two composite outfits, 1 scenario, 2 specs); Friday's list read 66 of 72, the Paris outfit resolved 2 of 4 components. Fixture frontmatter is all short one-liners (the #5 class), so no corpus ever folded | **FIXED** (2026-07-22) — top-level plain scalars now fold with single-space joins (v4-exact, oracle-proven); folds broken by comments, tab indents, folded QUOTED scalars, and folds inside block sequences stay conservatively out-of-subset (a scan of all 1,764 real frontmatter docs found zero folded-quoted values — the named residual). Frontmatter corpus +7 folded cases (two verbatim from Friday data); eight families re-ran green over fresh oracles (markdown-frontmatter, vault-frontmatter-parsers, vault-legacy-wardrobe, vault-component-leaves, vault-read-overlay, vault-wardrobe-item-file, vault-wardrobe-read, wardrobe-routes). **Human-verified LIVE on the Friday copy (2026-07-22): all 72 items list and "Paris 1925 Casual" resolves all four components** |
| 19 | Sidebar "All Whispers" reveals nothing visible and doesn't survive a reload | **Faithfully ported v4 behavior**, both halves. Persistence: v4's `showAllWhispers` is `useState(false)` — in-memory only, no localStorage, no PUT; v5's `signal(false)` matches (the component even documents "the reload gap is v4's"). Reveal: the filter (`isMessageVisibleToOperator`, ported case-for-case) only HIDES whispers with non-empty `targetParticipantIds` whose sender isn't pascal/prospero and that the human isn't party to — if the open chat has none of those, the toggle changes nothing, identically in v4. The v5 chat GET was verified to deliver the whisper rows (a Friday chat with 90 hidden Commonplace whispers returns all of them) | **NOT A BUG** (2026-07-22). To see it work, toggle in a whisper-rich chat — e.g. "Relational Veracity: A Living README" (90 hidden Commonplace recall whispers) |
| 20 | No "character detail → State Editor" entry found | **Walk-script error, not a gap** — the state cascade has NO character tier in v4 or v5 (`StateEntityType = chat \| project \| group \| general`); no v4 character screen opens a state editor. All four real tiers' entry points exist in v5 (chat: the sidebar opener; project: Prospero settings card; group: the group editor; general: Settings→Chat) | **NOT A BUG** (2026-07-22) — the walk script overpromised; recorded so it isn't re-reported |
| 21 | Can't see how to make a custom tool read `$state` in the Workbench | **Faithfully ported v4 behavior** — the builder form deliberately never AUTHORS `$state` in either version (identical "Edit $state references in the raw JSON" microcopy at all three sites); authoring is the dual-mode editor's JSON mode (`{ "$state": "path", "fallback": … }` in roll fields / comparator operands / parameter defaults, `{{state.path}}` in messages) or Pascal in-chat; the proving bench's Mock-state card is for testing resolution | **NOT A BUG** (2026-07-22) — usage documented in the walk notes |

| 22 | In a PROJECT chat, asking a character to list files in a folder fails every time: `doc_list_files` returns `Error: List files requires a project context`, and the character retries the same call in a loop (both the `uri` form and the `folder`+`mount_point` form) | Port divergence — the **tool context was built with every optional field hard-coded `None`**. v4's `orchestrator.service.ts:1136–1151` passes `imageProfileId`, `chat.projectId`, `browserUserAgent` and the loaded-memories bag into `createToolContext`; v5's `process_message` passed `None` at all five optional positions, at BOTH call sites (the native loop and the text passes). The guard the user hit is faithfully ported and lives in v4 too (`text-handlers.ts:815`) — it was firing because `ctx.project_id` was always absent, never because the URI was wrong. Two more tools were silently dead the same way: `doc_grep` ("Grep requires a project context"), `project_info`, and `generate_image` (`ctx.image_profile_id` absent → "Image generation is not enabled for this chat"). Invisible to the tier-3 orchestrator differential because that harness drives the tool loops with **its own** contexts — the call sites are never executed by the corpus (the same corpus-invisible class as finding #16's `Real*` seams) | **FIXED** — both call sites thread `chat.projectId` + the resolved `image_profile_id` through a new shared `turn_tool_context` helper (extracted so the threading is unit-testable); four regression tests pin the fixed behavior AND the guards (a projectless chat still yields `None` so the refusal keeps firing where v4 fires it; v4's `|| undefined` empty-string collapse; both loops identical). `orchestrator_tier3` re-run green over a fresh oracle. **The OTHER half (the infinite retry-the-same-tool symptom) was #25's linkage loss — FIXED by P4.13 phase A (2026-07-23). CONFIRMED GONE on the P4.13 unit-9 live walk (2026-07-24): `doc_list_files` in a project chat ran ONCE and the character answered from the result on all three providers walked — no loop. #22 fully CLOSED.** |

| 23 | The memory pipeline produces nothing: `MEMORY_EXTRACTION` jobs COMPLETE in seconds with no error, write no memories, and log no LLM call | **Port divergence, systemic — every NON-STREAMING provider request in v5 sends `"stream": true`.** The request builders hard-code `.set("stream", json!(true))` (+ `stream_options` on chat-completions) and IGNORE `RequestInput.stream`, which is consulted only for Google's URL (`request_builder/chat_completions.rs:105-106`, `anthropic.rs:268`, `responses_api.rs:305,309,330`). v4 has two distinct bodies: `streamMessage` sends `stream:true` + `stream_options`, `sendMessage` sends **`stream:false` and NO `stream_options`** (`plugins/dist/qtap-plugin-deepseek/provider.ts:189` vs `:288-289`; otherwise the two bodies are identical, field for field). So the provider answers `200` with an SSE body, `TransportResponse::json()` hits the `d` of `data: ` and fails `expected value at line 1 column 1`, and `execute_completion` returns that as a `CompletionError`. **Empirically confirmed** with two live DeepSeek calls on the real key: `stream:true` → `data: {"object":"chat.completion.chunk"…`; `stream:false` → `{"choices":[{"message":{"content":"pong"…`. **Why no differential caught it:** `request_builder_equivalence` intercepts v4's REAL plugin **`streamMessage`** only (`:7`) and every corpus vector sets `stream: true` (`:126`) — the non-streaming half of the builder has never been oracle-checked. **Blast radius: the entire cheap-LLM family on every chat-completions / Anthropic / Responses provider** — memory extraction, context summary, fold-episode, title generation, scene-state tracking, answer confirmation, image-prompt crafting, outfit selection, Carina, and the llm-consult. (`DANGER_CLASSIFICATION` survives because OpenAI moderation is a different endpoint; embeddings have their own path.) The symptom is invisible because `CheapLlmTaskExecutor::log_call` only writes `llm_logs` on success, so a failed cheap call leaves no Inspector trace | **NOT FIXED — ORDER WRITTEN: `work-orders/p4.11-non-streaming-request-builders.md`** (2026-07-22). Diagnosis complete + proof recorded; the fix is small per provider but spans seven builder sites and owes the differential a non-streaming corpus leg |

| 24 | Cheap-LLM calls routed to OpenAI come back empty: the LLM Inspector shows gpt-5-nano tried first with a 0-byte response, then the uncensored fallback retrying on DeepSeek. The `llm_logs` row reads `{"content":"","contentLength":0,"error":null}` with `usage` `{"promptTokens":508,"completionTokens":335}` — the model generated 335 tokens and v5 extracted none of them | **Port divergence — the response side's phantom SDK key, the exact sibling of #23 one layer down.** `parse_responses_api` (`model/response_parse.rs:270`) took content from a **top-level `output_text` string** on the response body. v4's plugin does read `response.output_text` (`plugins/dist/qtap-plugin-openai/provider.ts:433`) — but off the object the **OpenAI Node SDK** returns, where that property is synthesized by `addOutputText` (`node_modules/openai/lib/ResponsesParser.js:164`: concat the `text` of every `output_text` content part of every `message` output item, `join('')`), applied only on the non-streaming unwrap (`resources/responses/responses.js:27-32`, gated `object === 'response'`). v5 parses the raw HTTP body, which carries **no such key** — verified against a live `POST /v1/responses` (top-level keys `id`/`object`/`created_at`/`status`/…/`output`/`usage`; the text at `output[i].content[j].text`). So content was `""` for **every non-streaming OpenAI and Grok call** (`:604` routes both), while `usage` parsed correctly off real wire keys — the fingerprint in the log row. `build_responses_raw` minted `raw.choices[0].message.content` from the same key, so tool detection above the seam was blind too. **Why no differential caught it:** `parse_responses_api` has **no oracle differential at all** — only a hand-authored unit test (`:688`) that fed a body containing `"output_text": "the answer"`, asserting the parser reads a key the test itself invented; the committed `.wire` stream fixtures carry the same invented key (they are synthetic, not captured) | **FIXED** `0d6e2c5a` — `responses_output_text` reproduces the SDK's `addOutputText` aggregation from the wire arrays, feeding both the parsed content and `raw`. **The streaming decoder deliberately keeps the phantom key** (`decoders/responses_api_sse.rs:112`): v4 opens that stream with `responses.create({stream:true})`, whose events the SDK hands over unparsed, so v4's raw carries no content there either — aggregating would diverge from the oracle. Visible streamed text was never affected (it rides `response.output_text.delta`). Guards: the invented-key unit test rewritten to the wire shape, a regression test transcribed from the live body (asserts no top-level `output_text` exists, content `"pong"`, usage 335, `raw` content), and a multi-message concatenation case pinning `join('')` with a `refusal` part ignored. **Human-verified LIVE on the Friday copy (2026-07-23):** a TITLE_GENERATION call on OPENAI/gpt-5-nano returned a 282-char parsed body (the title JSON) where the pre-fix row on the same instance and same call type read `contentLength: 0` with 335 completion tokens; no uncensored fallback retry followed |

| 25 | A character gets stuck in an agentic loop: it calls a tool, then calls the same tool again, over and over, never using the result. (Brahma Console, walked clean 2026-07-20, does NOT show this) | **Port divergence — the tool-call linkage never reaches the wire, on every provider.** The loop builds the continuation slate correctly (`native_tool_loop.rs:434-437`, results carrying `tool_call_id`, the assistant turn carrying `tool_calls` — `tool_call_threading.rs:174`), then three conversions flatten it to role+content: `native_tool_loop.rs:627`, `brahma_console/mod.rs:256`, and `streaming_provider.rs:163` (`RequestMessage::text(role, content)`). The intermediate type cannot hold it — `CompletionMessage` (`model/completion.rs:45`) is `{role, content}` and `StreamParams.messages` is a `Vec<CompletionMessage>`. The builders are correct and fully wired (`RequestMessage` carries all five fields; every formatter consumes them) — nothing populates them. Result per family: **dropped entirely** on the Responses API (`responses_api.rs:54,79`) and chat-completions (`chat_completions.rs:52`), `tool_use_id: ""` on Anthropic (`anthropic.rs:102`), filtered-then-refused on OpenRouter. `tool_calls` always empty also means the assistant's own call is never echoed. The model asks, sees no answer, asks again. **Why Brahma survives:** `build_tool_result_messages` falls back to a plain `user` message `[Tool Result: …]` when a result has NO call ID (that path works), and the console additionally re-threads prior TOOL rows as user text and re-injects data after `MAX_DUPLICATE_TOOL_CALLS` (`brahma_console/orchestrator.rs:370-373,524-536` — "the console's loop-bug fix"). The Salon has no such net. **Why no differential caught it:** `request_builder_equivalence` feeds the builders a corpus already containing well-formed tool messages; the native-tool-loop tier-3 mocks the provider and asserts the loop's internal slate; neither executes the conversion between them — the same call-site-invisible class as #16 and #22. `brahma_console/mod.rs:253-256` even documents the false belief that "the provider request builder reconstructs" the dropped fields | **FIXED — P4.13 phase A (2026-07-23, the rewrite lane): the carrying `StreamMessage` enum replaces the `[{role, content}]` flattening at every conversion site (incl. the Carina loop, a FOURTH site), the request builders consume it directly (unit 5 deleted the lossy boundary entirely), and the always-on `tool_wire_call_site` pin asserts the bytes the transport receives per family — the loss cannot be reintroduced silently.** **CLOSED (2026-07-24) — the P4.13 unit-9 live walk on the Friday copy: a Salon tool turn ran clean on OpenAI, Anthropic, AND DeepSeek (three distinct wire formats) — the character called the tool ONCE, used the result, and did not loop. The embedded P4.17 tool card rendered inline (not raw JSON). This also confirms **#22**'s retry-loop half is gone. P4.13's last open item (unit 9) is now complete — the order can close** |

| 26 | The context-summary fold never fires. On a 700-message chat at interchange 56→57 with `lastSummaryTurn: 45`, no `SUMMARIZATION` row is written, `lastSummaryTurn` never moves, and no error surfaces anywhere | **OPEN — symptom confirmed, cause not yet localized.** The gate is arithmetic (`context_summary.rs:48`): `current − lastSummaryTurn > 10` → Fold. 56−45 = 11, and `run_summary_check` is called AFTER the finalizer persists the assistant message (`orchestrator.rs:2372`), so the count is not an off-by-one. **Ruled out during diagnosis:** the interchange count (v5's `get_messages` has no filter — `chats_messages_read.rs:263` — so the SQL reproduction is exact); the `cheap_llm_settings_present` gate (the column is populated TEXT and `chat_settings.rs:793` `parse_json`s it, so the flag is true); the title-check watermark as a diagnostic (checkpoints after 10 are every 10, so `lastRenameCheckInterchange: 50` staying put through 57 is correct and proves nothing). **Leading hypothesis: the summary's cheap-LLM call fails and leaves no trace** — `lastSummaryTurn` only advances on success, and v5 writes NO `llm_logs` row for a failed cheap call (v4-faithful, the #23 unit-8 deferral). v4 additionally wraps this call site in `try/catch` + `logger.error` (`memory-trigger.service.ts:132`); **v5 has no logging at all**, so the failure is invisible by construction. Likely entangled with #27 | **OPEN — needs localization.** Next step: drive the chat to interchange 60, where the title checkpoint fires. A `TITLE_UPDATE` job there proves the function runs and the summary path alone is broken; nothing at 60 proves the whole call is unreached. **This finding is the strongest argument yet for the #23 unit-8 ruling (log failed cheap calls) and for a tracing subscriber**<br><br>**UPDATE (P4.15): every identified cause is now fixed — CLOSE at the fresh dogfood walk.** #23 (streaming, P4.11), the sort panic (P4.14), failed-cheap-call `llm_logs` error rows (P4.13's ruled divergence), and **#27 (the configured cheap-LLM config, P4.15)** — the four causes the fold could silently fail on — are all closed, and the error-row instrumentation would now surface any residual failure. Re-check on the walk with P4.13 unit 9.<br><br>**CLOSED (2026-07-24) — the fresh dogfood walk on the Friday copy.** On chat `e71847c4` (393 messages) the fold fired **three times in one session** (`SUMMARIZATION` `llm_logs` rows at 15:59 / 16:20 / 16:46, each paired with a `TITLE_GENERATION`), wrote 6 `context-summary` chat events, populated a 7022-char `contextSummary`, and advanced `lastSummaryTurn` to 30 against interchange 40 (a correct 10-turn live tail). All on the cheap `deepseek-v4-flash` (#27's live confirmation). The original "never fires / no SUMMARIZATION row / watermark never moves" symptom is gone. **Two walk-observation corrections recorded so they aren't re-reported: (a)** the summary "not covering the most recent stuff" is the intended live tail, not staleness; **(b)** the fold runs **inline** at turn-close and logs to `llm_logs` (type `SUMMARIZATION`), NOT to `background_jobs` — checking the `CONTEXT_SUMMARY` job queue returns `[]` by design and is not evidence of a missing fold (the walk script's queue-check step was wrong for this trigger). |
| 27 | Context summaries run on the responding character's own (expensive) model, never on the configured cheap LLM — even with `strategy: USER_DEFINED` and a `defaultCheapProfileId` set | **Port divergence — CONFIRMED against v4, corpus-shaped constants wired into the production path.** v4's `triggerContextSummaryCheck` (`lib/services/chat-message/memory-trigger.service.ts:104-131`) passes the REAL `options.chatSettings.cheapLLMSettings` and `availableProfiles = await repos.connections.findByUserId(userId)` — every profile the user owns. v5's `run_summary_check` (`orchestrator.rs:2717-2732`) hard-codes `strategy: "AUTO"`, `user_defined_profile_id: None`, `default_cheap_profile_id: None`, `fallback_to_local: false`, and `available_profiles = [the character's own profile]`, under a comment saying the profile "IS the effective profile **in the corpus** (single-profile chats)". With priorities 1–3 of `get_cheap_llm_provider` unable to match anything, selection falls to the current profile. On the Friday copy the user's settings name both a `userDefinedProfileId` and a `defaultCheapProfileId` that the summary check cannot see. Same class as #22 — v4's real inputs replaced by corpus-shaped constants at a call site no corpus exercises | **FIXED — P4.15 (`work-orders/p4.15-cheap-llm-config-thread.md`).** Both broken sites (`orchestrator.rs::run_summary_check` AND the twin `courier_transport.rs::run_summary_check`) now thread the REAL parsed `cheapLLMSettings` + ALL the user's connection profiles (`connection_profiles::find_by_user_id`) + the resolved danger settings, exactly as the enclave step already did; the hard-coded `AUTO`/`None`/`from_ref` block is gone. The differential trap that let this slip (every corpus was single-profile) is closed: the orchestrator + `courier_images_routes` (`resolve_cadence`) families each gained a second connection profile + a `defaultCheapProfileId` selecting it, so the fold/episode/title `llm_logs` rows carry the SELECTED profile (`OPENAI/cheap-configured-model`), not `getCheapestModel(responding.provider)`. RED reproduced pre-fix on both, green post-fix; the enclave family re-ran green untouched (no-regression). Live re-check rides the fresh dogfood walk |
| 28 | Retrospective recall never fires: two deliberately backward-looking questions in a long chat produced no `retrospective-recall` whisper and no dated mini-recap — only the usual consolidated Commonplace whisper | **The CLASSIFIER is faithfully ported v4 behavior — the finding's premise is empirically false.** Diagnosed with evidence (P4.16, 2026-07-24, bench transcript in `status-log.md`). (1) v5's own classifier DOES fire `retrospective:true` on real backward-looking prose: the Friday copy's `llm_logs` carry **20** such rows (17 in v4 production, **3 in the v5 dogfood session** — incl. the very "coherence monitor… again, I've forgotten" and "…find your own conversations" turns the finding sampled). "Zero rows carry `retrospective:true`" was a sampling artifact — it fires *inconsistently*, not never. (2) v4's REAL classifier benched (deepseek-v4-flash, the configured cheap model, 5 reps/window): the tight (proactive) window vs the diluted 12-msg fallback window is a **weak-to-null discriminator** — Q1 tight 2/5 vs wide 1/5; Q2 tight 4/5 vs wide **5/5**. By the order's pre-committed rule (port-divergence needs ≥½ tight AND ≤¼ wide) NEITHER turn qualifies. (3) The real drivers of the misses are **the model and temp-0.3 sampling noise, not a v5 code divergence**: v4/DeepSeek returns **5/5 true on the exact window** where the walk's v5/gpt-5-nano logged `false` (gpt-5-nano is markedly weaker at this classification); and verdicts flip run-to-run on byte-identical input. Prompt+parse are tier-1-proven byte-identical (`distill_search_extraction_equivalence`), so there is no classifier port bug | **NOT A BUG (classifier)** (2026-07-24). Two SEPARATE threads surfaced, neither a classifier defect: **(a)** v5 lacks v4's **proactive pre-compute path** (`pre-compute.service.ts` — a recorded spine deferral; per-character `messagesSinceLastSpoke` distill + `preSearchedMemories` + fallback suppression). Its window is only marginally tighter, so the bench says it would NOT reliably fix the symptom — a v4-fidelity/consistency port for the human to schedule, NOT ordered here. **(b)** the classifier fired `true` on turns where **no whisper surfaced** (e.g. Amy 21:44:17 `retrospective:true`), so the residual symptom is DOWNSTREAM — the two silent `return None` gates (`build_context.rs:1496` spam-guard `RETRO_SIGNATURE_TURNS=3`; `:1577` empty result) and/or the multi-character chain surfacing only one character's result. The finding's "narrowed to the classifier" is incomplete; the downstream look is a future dogfood item |

| 29 | A standalone (user-initiated) Pascal custom-tool run renders "whispered to unknown" as its whisper attribution, rather than the operator's name | **Faithfully ported v4 behavior** — v4 deliberately whispers a private user-initiated run to `ctx.user.id`, the operator's **userId**, which the comment at `app/api/v1/chats/[id]/custom-tools/route.ts:318-320` explicitly notes is "a UUID that is not a participant id" (so every character's context filter excludes it while the operator's "All Whispers" still reveals it). v4's `MessageRow.tsx:323-324` resolves each `targetParticipantId` via `participantNames?.[id] || 'unknown'`, and `participantNames` (SalonView) is keyed only by character-participant ids — so the operator's userId never resolves and **v4 renders "whispered to unknown" here too**. v5's `custom_tools.rs:482-485` (`target_participant_ids = [user_id]`) and `message-row.ts:490` (`p.character?.name || 'unknown'`) are byte-faithful to both | **NOT A BUG** (2026-07-24) — recorded so it isn't re-reported. A nicer label (showing "you"/"yourself" when the target is the operator's own userId) would be a product improvement to make in **v4 first** (same rule as #17) |

| 30 | Running a **global** custom tool (`lambda`, in the "Quilltap General" store) from the composer's Custom Tools button rolls against the wrong character's fact sheet: the operator expected it to use their own played character's `metadata` (Charlie — `toolAbilities` includes `programmable`, so the tool should succeed), but it resolved outcome #3 "API Listening Agent not installed" — the branch for `toolAbilities ncontains programmable`, which matches the OTHER (LLM) character Friday's sheet (`analyze, display, architect`, no `programmable`) | **Faithfully ported v4 behavior** — a tool that every participant resolves identically (a shared/global store) dedups to ONE **unlabelled** roster listing whose `asCharacterId` is `sightings[0]`, the **first chat participant** (v4 `route.ts:209-210` / v5 `custom_tools.rs:279-281`). The run then rolls against THAT character's `metadata` (v4 `handleRun` `metadata = asCharacterId ? perspective.metadata : {}`, byte-identical to v5 `custom_tools.rs:418-422`). In this chat Friday (controlledBy `llm`) is participant[0] and Charlie (controlledBy `user`, the operator's own character) is participant[1], so the run resolved as Friday. v4's `CustomToolsDropdown.handleRun` passes the listing's `asCharacterId` with NO "run as me" override — identical to v5's `custom-tools-popup.ts:380`. **Empirically proven from the run's stored `pascalMeta`:** `metadataTested: {toolAbilities: "analyze, display, architect"}` (Friday's sheet, NOT Charlie's), `outcomeIndex: 2`, `invokedBy: "user"`, roll `value: 1.9958` — a roll that PASSED `gte:1`, so it would have hit outcome #1 (success) had it tested Charlie's `programmable` sheet. The rule is "resolve as participant[0]", which is USUALLY the operator's own character (matching the human's prior experience) — this chat is the outlier because it was created leading with the LLM character (Friday at [0], Charlie at [1]) | **NOT A BUG** (2026-07-24) — v5 reproduces v4 exactly. That a user-initiated composer run resolves as the arbitrary-first participant (here the LLM character) rather than the operator's OWN character is a genuine UX papercut, but it is v4's behavior → a **v4-first product change** (added to the post-5.0 list below) |

| 31 | An announcement posted **as an off-scene character** (Revenant) rendered — and stayed rendered — as **Friday**, the cast member whose turn was next. The characters themselves knew it was Revenant; only the UI did not | **Port divergence, SPA-only — the view model has no `customAnnouncer.kind === 'character'` arm.** The server is correct end to end: the row carries `customAnnouncer {"kind":"character","characterId":"a3833099…"}` with `participantId` NULL and `systemSender` NULL, and `api/salon.rs:240-284` ships the announcer in `offSceneCharacters` exactly as v4's `get.ts:452-465` does. But `resolveMessageAuthor` (`chat-view-model.ts`) handled only the `custom` kind, so the row fell past it to the ROLE FALLBACK — "the first participant that is a character" — and wore whoever sorted first. v4's `getMessageAvatar` (`SalonView.tsx:1066-1090`) has the missing branch in three steps: participant lookup by `character.id`, then `offSceneCharacters`, then the literal placeholder `'Off-scene character'` for a deleted character. **Why nothing caught it:** `resolveMessageAuthor` had NO direct unit coverage at all, and the one e2e announcement beat posts a STAFF announcement — which carries a `systemSender`, collapses to a chip, and never reaches author resolution. The character arm was the only unexercised path, and it is the one the dialog's off-scene picker produces | **FIXED** `fc70009a` — the three-step branch ported verbatim; 5 unit cases over `resolveMessageAuthor` (incl. the explicit guard "does not fall through to the first cast member" and the participant-beats-off-scene precedence); a new e2e beat posts as off-scene Dax into a three-character scene and asserts the author is Dax and NOT `/Aria\|Bram\|Cleo/`. Mutation-checked at both levels — with the arm disabled the beat fails `Received: "Aria"`, the same shape as the reported "Friday". **CONFIRMED LIVE on the Friday copy 2026-07-27**: both the pre-existing announcement and a freshly generated one render under Revenant. ⚠ The live re-check first appeared to FAIL, and the cause was the browser, not the app — see the stale-tab note in the standing notes |
| 32 | After switching a new NPC (Tanya) to user-controlled and back again, the **first** render of every message sent as a different user-controlled character was attributed to **Tanya**; a moment later it re-rendered correctly | **Port divergence, SPA-only — the Speaking-As override is a permanent latch where v4 holds no unconfirmed value.** `activeSpeakerId()` is `activeSpeakerOverride() ?? chat.activeTypingParticipantId ?? null`, and `onSelectSpeaker` set the override from the CLICK and never cleared it. The server, faithfully, drops the active speaker when that participant stops being user-controlled (`chat_participants.rs`, v4 `helpers.ts:179-197`: flipping to `llm` removes the id from `impersonatingParticipantIds` and sets `activeTypingParticipantId` to `newImpersonating[0] \|\| null`) — confirmed on the dogfood copy, where the chat now reads `impersonatingParticipantIds: []`, `activeTypingParticipantId: null`. The stale override then fed `makeTempUserMessage`'s `participantId`, so the optimistic bubble wore Tanya. The correction the human saw is the server's: `chatSend` ignores a `speakingAsParticipantId` that is not user-controlled and attributes the persisted row properly (verified — every user row after the flip carries Charlie's participant id, one earlier row correctly carries Tanya's from when she WAS playable). v4 never diverges this way because every impersonation handler assigns `data.activeTypingParticipantId` from the **response** (`useImpersonation.ts:63,107,134`) and a sync effect re-reads it from the chat | **FIXED** `fc70009a` — the override is now an optimistic bridge only: `.finally()` clears it once the refetch settles, handing authority to `chat.activeTypingParticipantId` whether the server took the choice or refused it. 2 unit cases (the bridge still shows immediately; the guard that it does not survive the round trip; and that a speaker the server DID persist does not flicker away). Mutation-checked |

| 33 | A user-initiated RNG roll rendered its standalone tool card under **Amy**, the last participant to speak, though the card's own line read "You ran rng" and the following characters correctly treated the roll as the operator's | **Faithfully ported v4 bug — v5 reproduces v4 exactly, in both halves.** The persisted row is identical on both sides: v4's orchestrator writes a pending tool result with `initiatedBy:'user'` and **no `participantId`** (`orchestrator.service.ts:611-630`), and so does v5 — confirmed on the dogfood copy (`participantId` NULL, `{"tool":"rng","initiatedBy":"user",…}`). Both renderers then run the same positional borrow: a TOOL row with no participant takes the nearest preceding assistant's, stopping at a USER boundary (v4 `VirtualizedMessageList.tsx:228-247`, v5 `chat-view-model.ts::resolveToolAvatar` — a verbatim port). Because the row is written BEFORE the user's message, the nearest preceding assistant is the last character who spoke. The header markup is byte-faithful too: when a header avatar resolves, BOTH render that character's name bold and the attribution line beneath it, and v4's own conditional (`actorName !== headerAvatar.name ? "${actorName} ran " : "ran "`, `ToolMessage.tsx:438-443`) is what produces the "Amy" / "You ran rng" pair the screenshot shows. v5's `actorName` (`operatorName \|\| 'You'`) and `attributionPrefix` match line for line | **NOT A BUG (v5)** (2026-07-27) — recorded so it is not re-reported, and NOT fixed: the borrow is v4 behavior and changing it unilaterally would break the oracle comparison. The right repair is v4-first (give a user-initiated tool row the operator's identity, or suppress the borrow when `initiatedBy === 'user'`) → added to the post-5.0 v4-side list. The card is not *wrong* about who rolled — it says "You ran rng" — it is wrong about whose face to put on it |
| 34 | Four Part-B walk items have no UI at all: **regenerate title**, **bulk reattribute**, **toggle agent mode**, **merge conversations** | **Not a defect — a known unwritten SPA lane (`p4.9e3`), plus one genuinely untracked item.** All four DO have v4 UI (surveyed 2026-07-27): regenerate-title has no button of its own and fires only from `ChatRenameModal`'s "Use automatic naming" checkbox (`ChatRenameModal.tsx:52,184-192`), reached from sidebar → Organize → **Rename**; bulk reattribute is `BulkCharacterReplaceModal` behind sidebar → **Edit Content** → "Bulk Replace" (`ChatSidebar.tsx:1636-1647`) — a whole sidebar section v5 has never had; agent mode is the "Agent On/Off" badge in the Chat section (`ChatSidebar.tsx:1116-1127`); merge is "Merge In…" in Organize (`ChatSidebar.tsx:1566-1576`). P4.9E3A landed all eleven SERVER verbs on 2026-07-26 and said so ("No UI can reach it this round"); P4.9E1B, the round's SPA lane, scoped to the cast dialogs + RNG. **The one real gap in the trail: the agent-mode toggle was tracked NOWHERE** — being a badge rather than a modal it fell between `m6-screen-parity.md`'s two tables | **NOT A BUG; TRAIL CORRECTED** `25dc4823` — the agent-mode row added to m6's sidebar-controls table; the stale "unported" claims in `chat-section.ts:71` and `organize-section.ts:17-21` corrected (they blamed missing server halves that have since landed, which materially understates how cheap `p4.9e3` now is — it is UI over a live boundary for everything except **Export**, whose verb really is still missing). The walk script's error, not the app's |

| 35 | The server console warns continuously: `Job type "EMBEDDING_GENERATE" is recognized but its handler is not yet available in the native runner`. **2,088 such jobs are DEAD** on the dogfood copy, and every chunk written since v5 took over the instance is unembedded | **A KNOWN DEFERRAL whose real cost was invisible until now — no `EMBEDDING_GENERATE` handler is registered anywhere.** The runner's `KNOWN_JOB_TYPES` lists it (`job_runner.rs:143`) so it gets the loud "recognized but not available" fallback, and `tools/executor.rs:355` calls it "a tracked deferral". **Both enqueue paths are LIVE and neither can ever complete:** `services/queue_service.rs:192` (memory embeddings) and `services/mount_index/embedding_scheduler.rs:45` (mount chunks). Each job retries 3× and dies. **Measured blast radius on the Friday copy:** every established vault is 100% embedded (Friday 747/747, Charlie 572/572, Amy 467/467 — all v4-era work), while the two character vaults created during this walk have chunks and **zero** embeddings (Test2 8 chunks / 0 embedded, minutes old; Tanya 6 / 0, three hours old). So P4.6BK's chunk-on-write fix works — the chunks are written at creation, which is what Part G set out to check — but nothing embeds them, and semantic search over anything added under v5 silently finds nothing. There is no workaround: `quilltap docs embed` enqueues to the same dead handler, and `docs status` refuses. v4's handler is `lib/background-jobs/handlers/embedding-generate.ts`, 490 LOC over FOUR entity types (`MEMORY`, `CONVERSATION_CHUNK`, `HELP_DOC`, `MOUNT_CHUNK`), including a deterministic-failure classifier that exists precisely to stop this DEAD-row accumulation | ~~**OPEN — needs an order, not a dogfood commit**~~ **FIXED and LIVE-PROVEN (2026-07-28).** The order ran as **P4.6BL** (the handler, all four entity types, with the permanent-error classifier) and **P4.6BM** (the reconcile that feeds it). Live proof on the Friday copy 2026-07-28: a boot minted 3 renders → 5 `EMBEDDING_GENERATE` jobs → all COMPLETED, and the permanent-error path marked 5 oversize chunks FAILED instead of retrying them to DEAD, which is the exact accumulation this finding was about. **No new DEAD rows across two boots.** The walk that proved it also surfaced that v4 itself had a much larger bug behind the same symptom — see the 2026-07-28 walk record below and the P4.D25 round |
| 36 | Attaching any image over ~2 MB to a chat fails with `Invalid multipart body` (the composer's paperclip and the paste-image handler both; two different photos reproduced it) | **Port divergence — the TRANSPORT limit shadowed the ported APPLICATION limit.** axum 0.8 applies a **2 MB `DefaultBodyLimit`** unless overridden, and `quilltap-web` never overrode it. The body extractors enforce it, so `Multipart::from_request` failed before any handler ran and every caller got a flat 400 with a misleading message. v5 already ports v4's real cap faithfully — `MAX_CHAT_FILE_SIZE` 10 MB (`services/chat_files.rs:512`) and the 10 MB image cap (`services/file_storage.rs:73`) — but the core never saw the request, so the ported check could not decide. v4 imposes no per-route limit (its handlers stream `request.formData()`); the only ceiling it states is `next.config`'s `bodySizeLimit: '100mb'`. **Blast radius was the whole binary-upload surface** — nine `FormData::from_request` sites across `files_routes.rs` (5), `characters_routes.rs` (2, incl. ST-PNG import) and `qtap_routes.rs` (2) — plus any JSON dispatch carrying base64 bytes, since the limit is router-wide. Localized in one experiment: the same route with the same nonexistent chat id answered `404 Chat not found` at 1 KB and `400 Invalid multipart body` at 3 MB | **FIXED** (2026-07-28, commit *"Let a photograph through the door (dogfood #36)"*) — `DefaultBodyLimit::max(100 MB)` layered on the router (`quilltap-web/src/lib.rs`), so every ported per-surface cap is the one that answers, with a hard backstop matching v4's stated figure. Regression test `chat_file_upload_over_axum_default_body_limit` pins BOTH directions: 3 MB must reach the handler, and 11 MB must be refused by the ported cap in v4's words (so the fix cannot be "corrected" into an unbounded body). Mutation-verified — removing the layer reproduces the reported error exactly. **Why no test caught it:** every multipart payload in the Rust suite and every Playwright upload fixture is a few bytes |
| 37 | Attaching a document-store image via the library picker posts a Librarian announcement naming the right file, but the vision description is of a completely different image — a generic dark-haired-woman portrait for a blonde woman examining a copper inscription on a yacht in a dry-dock | **Port divergence — the non-streaming completion path drops image attachments before the wire.** Traced end to end on the Friday copy: the announcement, blob resolution (`find_by_mount_point_and_path`), and `read_data(&blob.id)` are all CORRECT — the stored bytes extracted from `doc_mount_blobs` are the right blonde-yacht PNG. But `request_input_from_params` (`model/completion_provider.rs:38`) builds each wire message from `m.content` alone and **never reads `params.attachments`**; `RequestInput`/`StreamMessage` carry no image field, and no builder anywhere emits an OpenAI `image_url` / Anthropic image source / Google `inlineData` part. So the vision call reaches Z.AI as text only ("Please describe this image…" with no image) and the model invents a generic face — the classic no-image-received signature (freckles, earrings, direct-to-camera; nothing of the boat/plaque/dry-dock). v4's image-description path puts the image on the wire as a base64 data URL — that IS the feature. **Why no test caught it:** the differential harness uses the canned completion provider, which keys its response on `canned_completion_key_with_attachments(…)` — the TEST substrate reads the attachments to pick a canned reply, so the describe differential passed green while the REAL wire path silently dropped them. Same blind-spot class as #36 and the P4.11 one-mode corpus: the substrate structurally could not see the gap | **OPEN — needs an order, not a dogfood commit** (2026-07-29). The fix is cross-provider wire serialization of image attachments (data URL / image block / inlineData per provider) plus the wire differential that was missing — order-sized and must be oracle-proven, not hacked. ⚠ **Suspected broader scope, NOT yet confirmed:** since no builder emits image parts, in-chat vision (a message to a vision-capable model) may drop the image too — the paperclip image "rendering in the bubble" only proves the SPA drew it, not that the model saw it. First step of the order is to establish whether in-chat vision is affected or just the describe path. This means the P4.9E4A describe leg, though wired live, has never actually sent an image — its header is corrected |
| 38 | Firing up an autonomous room shows its run-state badge (short chat handle + budget readout, e.g. "wFAaA 202K") in the **left-sidebar footer**, wedged above the avatar and nav icons — visually out of place | **Not a bug — a symptom of an unported top toolbar.** In v4 these live in the **top page-toolbar** (`components/layout/page-toolbar.tsx:36`, right section), whose own header comment reads *"Top toolbar for pages… Replaces the app header"*, alongside `QueueStatusBadges`, the center `SearchBar`, and `NavContentWidthToggle`. **v5 has not ported that toolbar at all** (already noted in m6 §2.6 as the unscheduled global-search/toolbar lane, but only the search dialog was named there). The badges component (`autonomous/autonomous-room-badges.ts`, whose own doc calls them "the toolbar run-state badges") had nowhere to go, so P4.6ad parked it in `shell/shell.ts:138` — the left-sidebar footer. Moving only the badges would not help: there is no top header to move them into yet | **RECORDED — needs the toolbar lane, not a dogfood commit** (2026-07-29). Porting the top page-toolbar is lane-sized (autonomous badges + queue-status badges + search bar + content-width toggle + the page-specific left/right slots — e.g. the chat project link). The m6 §2.6 toolbar-lane note is widened to name all four occupants; when that lane runs, the badges move to the toolbar and the left-footer stopgap is retired. Cosmetic and non-blocking until then |
| 39 | Clicking "Speak as <character>" on an AI character flips the card to a "You" badge, but the next message you type still lands as your existing user-controlled character, not the impersonated one | **Faithfully ported v4 quirk — v5 reproduces v4 exactly, proven three ways.** v4 offers the "Speak as" button on ANY non-user participant (`ParticipantCard.tsx:600`, `!isUserParticipant`), but message attribution goes through `findActiveUserParticipant` (`turn-manager/utils.ts:99-107`), which honours `activeTypingParticipantId` ONLY when that participant is `controlledBy === 'user'`, else falls back to the first user-controlled seat. v5 ports this in the orchestrator (`orchestrator.rs:722` reads it as `speaking_as`) and it is covered by `chat_cast_routes_equivalence`. **Confirmed on the live Friday copy, chat `ee5923d4`:** the impersonated participant is **Abigail** (`af38f265`, `controlledBy=llm`); the sole user-controlled seat is **Charlie** (`57ecc095`, `controlledBy=user`); the typed message landed as Charlie — exactly v4's fallback. The confusing "You" badge on Abigail is ALSO v4 behavior (`ParticipantCard.tsx:358` — the badge shows for user-controlled participants *or when impersonating*), so v4 mis-signals identically | **NOT A BUG (v5) — v4-faithful (2026-07-29).** Recorded so it is not re-reported; NOT fixed, because changing `findActiveUserParticipant` touches differential-verified core turn-resolution. The human's desired behavior (below) is a deliberate v5 divergence but — corrected 2026-07-29 — a **behavior change, NOT a schema change** → **post-5.0 list.** |
| 40 | The boot tick enqueues an `LLM_LOG_CLEANUP` job that fails with `Job type "LLM_LOG_CLEANUP" is recognized but its handler is not yet available in the native runner`, retries to DEAD, and repeats on every boot | **A live enqueue path with no consumer — the LAST unhandled job type in v5, and tracked NOWHERE as open.** `LLM_LOG_CLEANUP` is in `KNOWN_JOB_TYPES` (`job_runner.rs:142`) so it gets the loud fallback, and `queue_service.rs:971 run_scheduled_cleanup` is called on the daily cadence *and immediately on startup* by `quilltap-host/src/host.rs:1097` — but no handler is registered in `ProductionSpineFactory`. Every enqueue burns 3 attempts and dies. **This is the finding-#35 shape at 1/500th the rate** (4 DEAD rows so far vs #35's 2,088), which is exactly why it survived: the accumulation is too slow to notice. `SELECT DISTINCT lastError … LIKE '%not yet available%'` on the Friday copy returns this one string and nothing else. **The real cost is not the DEAD rows — it is that v5 never prunes `llm_logs` at all.** The copy's llm-logs partition is **416 MB** for a 7,559-row / 7-day window, and that window exists only because **v4** is still doing the pruning against the live instance; the oldest row is exactly `retentionDays: 7` old. The moment v5 is the only app — which is the point of the port — the partition grows without bound at ~1,080 rows/day with verbose mode on. v4's handler is `lib/background-jobs/handlers/llm-log-cleanup.ts` (73 LOC) over `llm-logs.repository.ts:368 cleanupOldLogs`, whose cutoff is **calendar-day** arithmetic (`setDate(getDate() - retentionDays)`), not `now - N×86400000` | **OPEN — needs an order, not a dogfood commit** (2026-07-31). Small but it is a **write path against the llm-logs partition**, so by this repo's discipline it owes a tier-2 DB-state differential (the P4.6bj precedent), which is more than a dogfood commit carries. Scope: the repo write (`cleanup_old_logs`, `retentionDays < 0` → 0, the local-calendar cutoff), the handler (payload `retentionDays` else settings `?? 30`; `<= 0` → return; logging-disabled → return), registration in `ProductionSpineFactory`, and a differential over a seeded llm-logs partition. See the standing note below |
| 48 | An image attached from the document store shows as **Image Deleted** in the chat's photo gallery, though the attach itself worked | **Port divergence, SPA-side — the gallery rebuilds a URL the server already gave it.** `chatFilesList` returns TWO kinds of entry: `files`-table rows (uploads / generated images) and, from v4's Librarian **announcement walk**, `mountFile` entries whose `id` is a `doc_mount_file_links` id and which carry the document-store blob route in `url`/`filepath`. The server side is correct. The SPA's `thumbSrc` keyed on `kind === 'chat'` and built `/api/v1/files/{id}?action=thumbnail` for **both**, so a `mountFile` id was looked up in the `files` table and 404'd; the `<img>` error handler then marked it missing and drew the placeholder. **Confirmed on the Friday copy:** the failing request was `GET /api/v1/files/f9644b09-…?action=thumbnail&size=120` → **404**, and that id is a `doc_mount_file_links` row, present in neither `files` nor `doc_mount_blobs` (all 2,032 `mount-blob:` storage keys resolve — nothing was actually deleted). **v4 does not do this**: `PhotoGalleryModal.tsx:250` is `let src = item.data.url \|\| item.data.filepath` for EVERY item. v5's id-keyed thumbnail route is itself a documented v5 divergence (v4 served thumbnails straight off `filepath`), and it was applied one entry-kind too wide. `ChatFileDto` had also dropped both `url` and `type`, so the SPA could not tell the kinds apart | **FIXED** — `ChatFileDto` carries the `url` and the `type` discriminator the server already sent; `thumbSrc` follows v4's expression for `mountFile` entries and keeps the v5 thumbnail route for real `files` rows. The full-size `chatFileFor` URL got the same split — it is the very next click and would have 404'd in turn. Spec **mutation-proven**. Gate: ng test 263 files / 3,174; full Playwright 164/164 zero skips |
| 47 | An unparseable character-vault `properties.json` is silently ignored on read — and then the next character edit **permanently destroys** `pronouns`, `aliases`, `title`, `firstMessage`, `talkativeness` and `canChooseOutfit` | **A faithfully ported v4 bug, and the exact hazard `dcd9440a` fixed — for the two `StoreEntity`s but NOT the character vault.** Read side is fail-soft by design (`parse_vault_properties` → `None`, overlay falls back), which is v4-faithful and why opening the character showed nothing. The damage is on the **write** side: `vault_character_update::read_current_properties` returns `None` on a parse failure and the RMW seeds `empty_properties_default()`, so the next patch projects defaults over the six fields. **v4 believes this is safe and says so in a comment at the write site** (`managed-fields.ts:236`): *"Every other field above has a DB column, so 'the caller passed nothing' safely reads as 'the value is empty'."* **That comment is STALE** — measured against the real Friday instance, `characters` has **28 columns and not one of the six**; the vault cutover moved them into `properties.json` and dropped the columns, but the safety argument was never revisited. So the file is their ONLY home, exactly like the group bag (`color`/`icon`) and the project bag (16 keys) that `dcd9440a` hardened. **Confirmed by experiment on the Friday copy**, not by analysis: the operator corrupted a vault's `properties.json`, edited the character, and lost all six values while fields living in other vault files survived | **✅ RULED 2026-07-31 (human): order it for v5, flag v4 now — ORDERED as `p4.22`, and the v4 half is on the v4-side URGENT note below (NOT post-5.0).** The fix mirrors `dcd9440a`: refuse the patch when `properties.json` is **present but unparseable**, rather than seeding defaults — genuine ABSENCE must still seed (P4.D29's Epsilon arm). It has differential exposure across the characters families, so it needs a corpus arm per `dcd9440a`'s shape, not an inline edit. **Found because the walk step was WRONG** — it named the character vault (P4.D29 hardened groups/projects) and named *opening* the character (the refusal is on the patch path). Testing the wrong entity through the wrong operation is what surfaced the one bag `dcd9440a` missed |
| 46 | Export Markdown on a chat titled `Wings Over Suparṇā's Quiet Governance` saved with the two non-ASCII characters replaced by underscores | **A faithfully ported v4 bug — the ASCII fallback should not have been used at all.** The underscores are the *intended* fallback (`filename.replace(/[^\x00-\x7F]/g, '_')`), but `filename*=UTF-8''…` should have won and delivered the real name. It didn't, because the title contains an **apostrophe**: both apps build the ext-value with `encodeURIComponent`, which leaves `'` unescaped (it is in JS's unreserved set), and in RFC 8187 the apostrophe is the **delimiter** in `charset'lang'value`. An unescaped `'` inside the value makes the parameter ungrammatical, so the browser discards `filename*` and falls back. v5's `content_disposition::build_content_disposition` is byte-identical in shape to v4's `lib/api/content-disposition.ts` (same ASCII substitution, same `encodeURIComponent` semantics), so **v4 mangles this title the same way**. Note the download is an anchor-click with a `download` attribute, but same-origin Content-Disposition still takes precedence, so the header is what names the file. Affects any title with an apostrophe **and** a non-ASCII character — common in prose titles | **✅ RULED 2026-07-31 (human): fix v5 now, queue v4 — FIXED** (`c04e0951`). A new `encode_ext_value` percent-encodes `'`, scoped to this one call because `encode_uri_component` is a faithful `encodeURIComponent` port with another consumer (`api::ui_search`) that wants JS's behaviour exactly. **The corpus blind spot is the lesson:** `markdown_transcript_equivalence` already had an apostrophe vector — `Suparṇā’s Salon 🎩` — but it uses a **CURLY** apostrophe (U+2019), which is non-ASCII and so gets percent-encoded, never reaching the delimiter. A straight ASCII `'` beside a non-ASCII character was untested, which is exactly how a byte-for-byte family stayed green over a real defect for the life of the port. New vector `ascii-apostrophe-with-non-ascii` + an `EXPECTED_DIVERGENCES` carve-out asserting the divergence in BOTH directions (v4's recorded bytes must still carry the raw delimiter; a VANISHED divergence fails loudly, so the carve-out retires itself when v4 ships), plus a coverage assertion so deleting the vector cannot silently retire the declaration. **Three mutations, all RED as designed.** v4's freshly regenerated bytes confirm the diagnosis: `filename*=UTF-8''…Supar%E1%B9%87%C4%81's%20Quiet…` — the raw `'` sitting inside the value. Two unit tests pin the header bytes and the scoping. The v4 one-liner is queued on the v4-side note below |
| 44 | The magnifier in the toolbar search box sits jammed against the typed text | **Port divergence, and wider than the report — `qt-icon` applies its class TWICE.** `class` is an *input* on `Icon` (aliased, landing on the inner span), but Angular **also** keeps the same static `class` on the `<qt-icon>` host, so a site writing `class="absolute left-3 …"` gets the offset applied at both levels: the host becomes a positioned box at `left:12px` and the span offsets another 12px inside it. v4's React `<Icon>` renders ONE span. **Measured in the browser rather than eyeballed** — wrap 393.9 → host 405.9 → glyph 417.9, right edge 433.9, against text starting at 429.9: the glyph sat **4px PAST** the text origin where v4 clears it by **8px**. Five sites carry positioning utilities (the search bar, the search dialog, the custom-tools popup, the run-tool modal, the character Conversations tab — all the same magnifier-in-an-input pattern); every one was doubled | **FIXED** — `display: contents` on the host, so it generates no box, cannot be a containing block, and its copy of `absolute`/`left-*` has nothing to act on; the span resolves against the real positioned ancestor exactly as v4's single element does. Blockification does **not** apply — confirmed empirically, not from the spec. Re-measured at **8px, v4's value**. Pinned by a measured beat in `page-toolbar-flow.spec.ts` and **mutation-proven** (reverting the host style fails it). ⚠ The same trap bit the P4.D34 chevron work earlier the same day — see the note in `announcement-group.ts` |
| 45 | Clicking outside the search dialog doesn't close it; only `Esc` does | **A faithfully ported v4 bug — NOT fixed, awaiting a ruling.** The backdrop markup, the `qt-dialog-overlay` CSS, and the `(close)` wiring are all byte-identical to v4. The cause is layout: `.qt-page-toolbar` sets `backdrop-filter: var(--qt-app-header-blur)` (`_layout.css:709`, **identical in both apps**), which makes it a **containing block for `position: fixed` descendants** — so the backdrop's `fixed inset-0` resolves against the toolbar, not the viewport. **Measured:** backdrop rect `56,0 1224×64` in a 1280×720 viewport; `elementFromPoint` at (60, 640) hits `QT-SALON-LIST`, not the backdrop. There is simply no backdrop outside the toolbar to click. Escape still works because it is a document-level key handler. **v4 is affected identically**: its `SearchBar` renders `SearchDialog` inline with **no portal**, and `<SearchBar />` sits inside `<div className="qt-page-toolbar">` | **✅ RULED 2026-07-31 (human): fix v5 now, queue v4 — FIXED.** `SearchDialog` portals its host to `document.body` in its constructor (removed on destroy), so the backdrop's containing block is the viewport; the backdrop element and its `qt-dialog-overlay` class are untouched, restoring v4's **intent**. Pinned by a beat that asserts the backdrop covers the viewport **and** performs the real gesture (a click at the bottom-left), **mutation-proven**. One unit spec moved from `fixture.nativeElement` to `document` — a legitimate consequence of the portal, not a workaround. ⚠ **This invalidates a documented premise:** `slide-over-panel.ts` records the standing "v5 renders inline" decision on the grounds that "*nothing in the chat layout carries*" a transform or filter — the P4.9P toolbar made that false for anything rendered inside it, and this was the first such surface. Any future fixed-position surface mounted in the toolbar needs the same treatment. The v4-side fix is queued post-5.0. Distinct from the previously-deferred "backdrop arbitration" item (that one is nested-dialog stacking) |
| 42 | An inline dialog error ("Connection lost. The server may still be starting.") renders in ordinary body colour — "it just looked informational" | **A v4 bug v5 inherited and then amplified.** `qt-text-danger` is referenced by markup in BOTH apps and **defined in neither** — an exhaustive search of all 25 v4 CSS files (styles, packages, theme packs) and all of v5's finds no rule, so every site wearing it inherits the body colour. v4 has 5 such sites (`StartupProgress.tsx`, `ChatCreationProgressModal.tsx`); **v5 has 14 across 13 files**, and the reason for the spread is the deeper finding below: v4 signals most dialog failures with a **toast** (`lib/toast.tsx`, used by **103** v4 files) and v5 has no toast system at all, so each ported dialog grew an inline error paragraph and reached for this name. v4's own `ReattributeMessageDialog.tsx` (the screenshotted dialog) renders no inline error whatsoever — it calls `showErrorToast` | **FIXED** — `.qt-text-danger` defined in `_utilities.css` beside `.qt-text-destructive`, both resolving `var(--color-destructive)`. Defining the utility rather than rewriting 14 markup sites is the **P4.D34 unit-3 remedy shape**, and it keeps the two genuinely-ported sites byte-identical to v4's markup. Verified present in the BUILT stylesheet (the D34 minifier-merge trap: counted `qt-text-danger{`, not a selector-text match). Recorded as a **deliberate v5 divergence**; the identical v4 one-liner is queued post-5.0. **The toast subsystem is the real gap and needs an order** — see the standing note |
| 43 | Announcement chips show a **down**-chevron while collapsed, and carry no message identity | **Port divergence — three attributes dropped from v4's `AnnouncementChip`.** v4 (`app/salon/[id]/components/AnnouncementChip.tsx`) renders `AnnouncementBarContents` with no `expanded` prop, so a chip in a group always draws `chevron-right`; v5 hard-coded `chevron-down`, which reads as an open affordance on a closed chip. v4 also sets `aria-expanded`, an `aria-label` ("Expand \<sender\> \<kind\> message"), and — with an explicit comment saying why — `id="message-<id>"` + `data-message-id` "so deep-link and delete-next-focus scroll-to-message still resolve"; v5 set none of them (`message-row.ts:54` was the only site in the SPA minting `message-` ids). **Everything else checked in this area is byte-identical to v4** and is NOT the cause of the broader complaint: the whole `.qt-chat-announcement-*` / `.qt-chat-system-bar-*` CSS block diffs clean, and `.qt-chat-message-system`'s centered-italic rule (`_chat.css:230`) is v4's own | **FIXED** — chevron follows state, the `-down` modifier composed into the icon's `class` **input** (a per-class host binding would land on the wrong element — `qt-icon` applies `class` to an inner span), plus `aria-expanded`, the state-aware label, and the identity attributes. Two specs, both **mutation-proven**. ⚠ **This does not close the human's broader report** — a systematic announcement-rendering audit is a standing note below |
| 41 | v5 writes `{"userId":…,"retentionDays":7.0}` into `background_jobs.payload` where v4 writes `…"retentionDays":7` | **Port divergence — a persisted JS number serialized as an `f64`.** `run_scheduled_cleanup` read `retentionDays` out of the untyped settings bag with `Value::as_f64` and dropped it straight into `serde_json::json!`, so a whole number rendered `7.0`; v4 carries a JS number to `JSON.stringify`, which renders `7`. Both apps read this column, and both parse either form, so nothing breaks today — but it is a byte divergence in shared persisted JSON, the exact class the `js_number_to_json` helper (`db/mod.rs:563`) exists for and that P4.6an already fixed once for `dangerousContentSettings` ("`1` not `1.0`"). **Invisible to the suite: `run_scheduled_cleanup` has no differential coverage at all** — its only caller is the host cadence, so no harness family ever built this payload. Found only because finding #40's failure row printed the payload | **FIXED** — the payload moves through `cleanup_payload()` + `js_number_to_json`; two unit tests pin the bytes (whole → `7`, fractional → `0.5` preserved), **mutation-proven** (restoring the raw `f64` fails the first test with `7.0` vs `7`). Key order is insertion order (`preserve_order`), matching v4's `{ userId, retentionDays }` |
| 54 | A **staff-signed** announcement (the Host, Suparṇā) reaches the model with no attribution — it appears in the LLM Inspector as a bare `user` turn | **NOT A BUG — the walk exercised the one announcer kind that is designed to carry none.** v4's `lib/chat/context/announcement-attribution.ts` states it in its own doc-comment: *"Messages without a `customAnnouncer` pass through untouched — Staff announcements carry their identity in their prose already."* The Insert Announcement dialog has THREE modes (`insert-announcement-dialog.ts:681-685`): `staff` writes `systemSender` and no `customAnnouncer`; only `character` (an off-scene workspace character) and `custom` (a free-text display name) produce one, and only those get the `[Name] ` prefix. The `user` role is also correct on both sides: the row is persisted `role: ASSISTANT` (v4 `announcer/writer.ts:94`, v5 `announcer/writer.rs:112`), and the context builder renders every line that is not the perspective character's own as a `user` turn | **NOT A BUG (v5)** (2026-08-02) — recorded so it is not re-reported. **⚠ The character/custom path is still UNPROVEN on real data**: its only evidence is the tier-1 `announcement_attribution_equivalence` family plus the `regenerate_swipe_tier3` wiring proof. The retest is owed — post as a character, expect `[Name] body` in the Inspector |
| 53 | A **whispered announcement** stays visible even when it was whispered to characters the operator is not playing, regardless of the All Whispers toggle — while every other whisper obeys it | **NOT A BUG — v4-faithful, and deliberately so.** `isOperatorAuthoredAnnouncement` (`systemKind === 'announcement'`) exempts these from the filter unconditionally, and they are excluded from the overheard dimming too; v5's `whisper-visibility.ts` matches v4's `app/salon/[id]/whisper-visibility.ts:60-64,90` line for line. The rationale is stated at the v4 source: a whispered announcement has **no `participantId`** to match the author against, so without the exemption it would vanish the instant the operator posted it — you would type a private aside, send it, and watch nothing appear. Note the operator is the author of every ad-hoc announcement (Insert Announcement is operator-only), so the exemption is "show me what I wrote", not a leak of someone else's whisper | **NOT A BUG (v5)** (2026-08-02) — recorded so it is not re-reported, and NOT fixed: the rule is v4's, and the sibling `OPERATOR_FACING_WHISPER_KINDS` comment records that sender-level granularity here has been wrong twice already. Whether an announcement the operator authored should nonetheless obey the toggle is a fair product question → **v4-first if wanted; not queued, awaiting the human** |
| 52 | The run dialog's "may write" sentence renders a space before its full stop: `… metadata.lastLambdaOutput . The record of what actually changed …` | **Port divergence — Angular collapses a template newline into a real space where JSX drops it.** The markup mirrors v4 element for element (`CustomToolRunDialog.tsx:700-711`), but the newline between the `@for` block's closing `}` and `. The record` is a text node; Angular's whitespace collapsing turns it into one space, while JSX discards whitespace-only lines containing a newline. Confirmed in BOTH pipelines — the JIT DOM (`… metadata.lockpick . The record …`) and the AOT bundle (`d(4," . The record…")`) — and independently in the running app by the human's own `innerHTML` dump (`</span><!----> . The record`). **The existing coverage was blind by construction:** three `toContain` assertions on fragments, none of which can see the seam between them. Note the same dump ALSO settled a second report — that no comma separates the targets — as NOT a defect: the comma is present (`<span>, <!---->`), but it is a bare text node inheriting the paragraph's dimmed `qt-text-secondary` while both targets are the brighter `qt-text`, so a 12px dim comma trailing a bright monospace token reads as absent. v4 dims it identically, so the rendering is faithful; **the human ruled 2026-08-02 to leave it** | **FIXED** `<commit>` — the `.` joins the `@for` block's closing brace, killing the text node. The three fragment assertions are replaced by an **exact** whole-sentence assertion (whitespace-normalized), which would equally catch a separator that went missing. **Mutation-proven**: restoring the newline reds it with the old spacing |
| 51 | After running a custom tool, its string parameters come back pre-filled with `[object Event]` — JavaScript's default string for an Event object | **Port divergence, and a CLASS not an instance: an Angular output named after a bubbling DOM event receives that DOM event too.** `AutoGrowTextarea` and `CustomToolParamsForm` both named their output `change`; the inner `<textarea>`'s native `change` — which fires **on blur, i.e. the moment you click Run** — bubbles to the host element and reaches the very same `(change)` binding, delivering an `Event` on top of the good string the output had already emitted. The form persists the last value, so the next open shows the Event's `toString()` in every field you had touched. **Measured, not reasoned:** a probe dispatching `new Event('change', {bubbles:true})` returned `[{"param":"label","value":"[object Event]"}]`. v4 cannot hit this — React's `onChange` is a synthetic prop carrying `e.target.value`, never a DOM listener on a component tag. **The survey found one other live instance, worse than the reported one:** `MessageRow`/`MessageList` named their output `copy`, and `copy` is a bubbling clipboard event — so **selecting text in any chat message and pressing Cmd+C called the Salon's `onCopy(ClipboardEvent)`, which does `writeText(message.content)` → `writeText(undefined)`, overwriting the user's clipboard with "undefined" and toasting "Message copied to clipboard!"**. Reproduced the same way. The wardrobe's `WardrobeItemRow.copy` shares the shape (a stray Cmd+C would open a transfer dialog on a garbage item). Five further candidates (`select` ×3, `submit` ×2, `reset` ×2) were checked and cleared: none of those components' subtrees contain an `<input>`, `<textarea>` or `<form>`, so the colliding event cannot originate inside them | **FIXED** `<commit>` — every colliding output renamed away from its DOM-event name: `valueChange` (AutoGrowTextarea), `paramChange` (CustomToolParamsForm), `copyMessage` (MessageRow/MessageList), `copyItem` (WardrobeItemRow), with all call sites moved. Three regression tests pin the leaks shut — a bubbling native `change` and a bubbling native `copy` must each emit NOTHING, plus a positive test proving the guard didn't silence ordinary typing. **All three mutation-proven** by re-adding the colliding binding: the copy test reds with `Event{isTrusted:false}`, the form tests with `[object Event]` |
| 50 | The Open-Document picker in a project chat cannot reach that project's own document store — and **"Look everywhere" doesn't help**: every OTHER project's store lists, and only the conversation's own project is missing | **Port divergence — a dropped affordance, made invisible by a server contract that depends on it.** `accessible_stores_body` (`api/documents.rs:1131`) deliberately moves the chat project's official mount OUT of `stores` and returns it as a separate `projectLibrary` field — *"surfaced separately (left-column button)"* — and it stays withheld under look-everywhere because its id is already in `seen`. v4 renders that field as a **Project library** button in the picker's left column (`DocumentPickerModal.tsx:487-501`), with two arms: an official store → browse that mount (`handleSelectScope('document_store', …)`), no official store → the legacy `project` scope. v5 built neither, so the withheld store had no path at all. The wire was never the problem: the server returns the field and `document-api.ts:103` already parsed it — only the picker never read it. **Root cause is a deferral drawn too wide:** the picker's header deferred "the project/general FileBrowser path", which is genuinely needed for the legacy arm only; the store-backed arm is an ordinary mount browse v5 has had since P4.6x | **FIXED** `<commit>` — the button renders for the store-backed arm and browses the project mount through a shared `openMount` (extracted, not duplicated); `projectLibrary` is carried across a look-everywhere refetch so the button can't flicker away. Five specs: renders, browses (asserting `listMountFiles('pm1')`), survives the toggle, and both absence guards (no project library, standalone surface). **Mutation-proven** — dropping the carry reds exactly the three positive cases. The legacy no-official-store arm stays deferred and shows no button rather than a broken one; the header comment now scopes the deferral correctly. **E2E deferred with its requirement named** — no seeded chat in either the salon or projects instance lives inside a project with an enabled official mount, so the beat needs new fixture seeding |
| 49 | A side-effect counter cannot bootstrap: an effect valued `{{state.encounters.count}} + 1` is skipped on every run of a fresh key (`· effect N skipped: expression did not evaluate: … did not resolve to a value`), and the guard forms an author reaches for — `{{state.encounters && state.encounters.count \|\| 0}}` — are not in the grammar either | **Faithfully ported v4 limitation, verified against the oracle in both halves.** The grammar is `+ - * /`, parentheses, literals and `{{ref}}` substitution and nothing else — no logical operators, no member access, no calls (v4 `lib/pascal/expressions.ts:52,121`; v5's port matches token for token). An unresolvable ref throws `{{…}} did not resolve to a value` and the effect is skipped fail-soft (v4 `expressions.ts:367-368` — the identical sentence), which v4's own feature doc states normatively: *"A `resolveRef` returning `undefined` (absent metadata key, non-primitive state value, no consult) → eval failure"* (`pascal-custom-tool-enhancements.md:174`). **The consequence is structural, not cosmetic:** the only thing that would create the key is the effect that keeps skipping, and `EffectWhen`'s subjects (`roll`/`params`/`metadata`/`llm`/`outcome` + comparators) include **no state subject**, so no "if absent, set 1" arm can be written either. Workarounds that do work today: seed the key once in the State Editor (any tier), after which the increment evaluates; or use a literal JSON number/boolean value, which is stored as-is rather than parsed as an expression (a flag, never a counter) | **NOT A BUG (v5)** (2026-08-02) — recorded so it is not re-reported, and NOT fixed: diverging unilaterally would move the oracle on a grammar whose every error sentence is corpus-pinned. The repair is a v4-first design choice (an effect-level `default`, an absent-state `when` subject, or absent refs resolving to 0) → added to the post-5.0 v4-side list |

- **The 2026-08-02 `c4d4b0de`-round walk, Parts A–B — the Pascal side-effects
  live proof COLLECTED, plus three findings (#49–#52, two fixed).**
  - **Part A (boot) — PASS on all three counts.** v5's whole boot footprint was
    eight jobs, every one COMPLETED, and **zero messages written**: ONE
    `EMBEDDING_GENERATE` (**P4.D25's live proof** — the pre-fix behavior
    re-embedded the entire cold tier on every boot at roughly $2 a restart, so
    a single row is the reconcile healing only what is genuinely missing), one
    `LLM_LOG_CLEANUP` **COMPLETED** (**P4.24's live proof** — it used to burn
    three attempts and go DEAD every boot), one `CONVERSATION_RENDER`, four
    `AUTONOMOUS_ROOM_SCHEDULE_TICK` with nothing due, and one
    `CHAT_DANGER_CLASSIFICATION`. No new FAILED/DEAD rows; the 2,351 historical
    DEAD are finding #35's known backlog, and all six dead schedule ticks read
    `Orphaned on startup — killed`, which is the stuck-job sweep working.
    **⚠ A METHOD TRAP worth carrying: `createdAt > datetime('now','-10
    minutes')` silently matches EVERYTHING FROM TODAY.** `datetime()` yields
    `2026-08-02 12:10:00` (space-separated) while `createdAt` is ISO-`T`, and
    `'T' > ' '` byte-wise, so every same-day row compares greater. The first
    reading of this walk was alarming and wrong because of it — it appeared to
    show v5 spending real money unattended on boot. Use
    `strftime('%Y-%m-%dT%H:%M:%fZ','now','-N minutes')` as the comparand.
  - **Part B (Pascal side effects) — the round's headline, PASS, and 💸 the
    P4.D35 live proof is COLLECTED.** The Workbench Side Effects card, the dry
    run (`→ target = value (would write)` with nothing actually written), the
    chip label, and the two-block bubble all behaved. **Steps 9 and 10 —
    cross-tier writes and "write where it lives" — passed on real data: the
    effects adjusted the right state at the right level, and the tier search
    dropped through the cascade deterministically.** That discharges the
    P4.D35 💸 item; only the tri-tier dressing and whispered-announcement
    proofs remain owed from this round.
  - **Step 11 (the unattributed roll) RETIRED as unreachable, and the walk
    script was wrong about it.** Attribution does not come from where a tool
    lives: `buildListing` sets `asCharacterId: perspective.characterId` on
    EVERY row (v4 `custom-tools/route.ts:353`, v5 `custom_tools.rs:335`), the
    perspective is `preferOperator(…, operatorCharacterIds(participants,
    activeTypingParticipantId))` — the character you are speaking as — and both
    dialogs forward it. `characterLabel` is the field whose absence means "runs
    as you"; `asCharacterId` is always populated and only disambiguates. Only a
    direct API call can omit it (v4's Zod has it `nullish()`), so the
    unattributed arm lives in the route differential and nowhere in a walk.
  - **Part C (whispered announcements) — the audience surface PASSES; two
    reports dispositioned NOT A BUG (#53, #54); one step retired as
    mis-specified; the 💸 proof is PARTLY collected.** The "Who hears it"
    control, the soft-removed exclusion, and posting a whisper (label flip,
    success toast, the chip's whisper tag) all behaved on real data. The
    audience-invalidation step was **my script's error, not a defect**: it
    clears the *generated in-character proposal* (`proposedMarkdown`, stage →
    compose), never text the operator typed — so "it didn't clear my typing,
    and it shouldn't" is the correct behavior, and the step only bites via
    character mode → Generate → toggle while reviewing. Still owed from Part
    C: the All-Whispers narrowing check on **Prospero's `group-context`
    whispers** (the leak this round actually closed — #53 is the announcement
    exemption, a different rule), the six-theme whisper-label legibility pass,
    and #54's character/custom attribution retest.
  - NOT walked this pass (the next pass starts here): the Part C remainder
    above, Part D (tri-tier wardrobe 💸), Part E (the editor's
    sub-list indentation + the OWED store-backed document scan), and all of
    Part F's carried-over debt.

- **The 2026-07-31 `ff12f491`-round walk, Parts B–F — coverage, and THREE owed
  live proofs COLLECTED.** Eight findings (#41–#48); seven fixed in place, two
  ordered (#40, #47), none left open.
  - **Part B (P4.D30, the Pascal canonical reader) — PASS.** A blob-stored
    definition could not be tested *by shape* (see the note below), but the
    reachable arm was proven live: a database-store definition loads, opens in
    the Workbench editor, runs on the bench, and runs again from the in-chat
    composer popup — both entrances through the refactored reader.
  - **Part C (P4.9P, the top toolbar) — PASS, with two findings.** The
    autonomous run-state badge is in the toolbar and the sidebar-footer stopgap
    is gone (**dogfood #38's fix confirmed live**); queue badges move with real
    jobs; `Cmd+K`, the type-filter chips (incl. the pre-seeded case the §3
    review fixed), and the content-width toggle all behave. Findings **#44**
    (`qt-icon` applying its class twice — the glyph 4px PAST the input text on
    all five positioned-icon sites) and **#45** (the backdrop clipped to the
    toolbar by its own `backdrop-filter`) came out of this part.
  - **Part D — PASS, with the walk's two most valuable findings.** Export
    Markdown renders a real transcript from the largest chat and produced
    **#46**; the P4.D29 store-hardening step was MIS-SPECIFIED and produced
    **#47**, the character-vault clobber `dcd9440a` missed (see its own note).
  - **Part E (P4.21, image attachments on the wire) — PASS; ⚠ P4.21's LIVE
    PROOF IS COLLECTED and dogfood #37 is CLOSED.** The library-picker describe
    leg returned a description of the ACTUAL image (the finding's exact repro,
    a blonde-yacht PNG that previously produced a generic portrait); in-chat
    vision works; **all three serializations verified on real sends — OpenAI-
    compatible, Anthropic and Google** — and confirmed **in the LLM Inspector**,
    not merely in the bubble, which is the check that distinguishes "the SPA
    drew a thumbnail" from "the model received an image". The one non-pass was
    **#48** (a store-attached image tombstoned in the chat gallery), fixed.
  - **Part F (P4.D33, the OpenRouter pricing seam) — PASS; ⚠ P4.D33's LIVE
    PROOF IS COLLECTED.** With a real OpenRouter key: a tool-using turn on a
    tool-capable model reaches the wire with a **native `tools` array** rather
    than the pseudo-tool prose loop, and cost estimates come from the live
    catalogue rather than the fallback table. That is the half that bit — the
    SDK key remap made every OpenRouter model report `supportsTools: false`,
    so `checkModelSupportsTools` silently downgraded every one of them.

  **NOT walked — the next pass starts here:** Part G (P4.d26's same-day recall
  + fresh-event boost on a non-UTC host, and P4.d27's embedding standard),
  Part H (the retrospective-recall look owed since 2026-07-24 — finding #28's
  downstream half — plus Story's Clock and the per-chat Core-whisper override),
  and Part I (**Data & System on a scratch copy — still the largest untested
  surface in the port**, and the home of P4.D31's owed restore-memory-id proof).

- **The 2026-07-31 walk, Part B — P4.D30's blob-definition arm has NO PRODUCER,
  and the step is unrunnable BY SHAPE (not skipped, retired).** Scripting the
  walk asked the operator to open a blob-stored custom-tool definition; the
  Friday copy's eight definitions were classified first, and all eight are
  `Quilltap General` **database** mount, `fileType: json`, `source: database`,
  with a `doc_mount_documents` row and **no** `doc_mount_blobs` row. That is not
  an accident of this instance — it is **structural**: discovery requires
  `*.tool.json` (`TOOL_FILE_SUFFIX`), `.json` maps to `NativeTextType::Json` →
  fileType `json` → `is_text_like`, so `read_mount_file_bytes_conn` takes the
  **document** branch for every definition the roster can find. A blob-stored
  definition requires a row hand-written with a mismatched fileType, which is
  precisely how the harness fixture makes one. The Workbench's save writes a
  database *document*, and the ingest path maps the extension the same way, so
  no UI in either app can produce one.

  **v4 never claimed otherwise, which is the part worth carrying.** Its
  `83118077` frames the change as a **deduplication** — `readToolFile`
  hand-rolled its own storage dispatch and now calls the canonical
  `readMountFileBytes` — and lists blob readability under "*two effects beyond
  the deduplication*". The stated motivation is "*found by release checklist 1
  (file provider)*", a path-safety audit. So **both** named side effects are
  unreachable in practice: P4.D30's lane already disproved the other one
  (boundary escapes, pinned by unit test as v4 pins it), and this note disposes
  of the second. The drift's real content is the deduplication plus the
  `SOURCE_NOT_FOUND` race skip. **Coverage for the blob arm is
  `pascal_definition_reader_equivalence` alone** (6 cases, with an explicit
  assertion that the blob case is present so it cannot silently vanish from the
  corpus) — and that is the right and sufficient home for it. A future walk
  should NOT script a live blob-definition step; there is nothing to click.

- **The 2026-07-31 `ff12f491`-round walk, Part A — the P4.D34 SPA riders, all
  six items closed; two findings fixed, one v4 weakness recorded, one deferral
  confirmed.** The lane's live proof is COLLECTED. Verified on the Friday copy:
  the **exited-session input disable** (item 3 — `exit` the shell, the input
  refuses keystrokes; newly live in both apps), the **shared Staff
  display-name table** (item 5), and the **xterm-6 two-tier theme read**
  (item 4) — the last one only after the re-test that matters: **both apps read
  the xterm theme once at construction and neither re-reads on a live theme
  switch**, so the pass is *switch theme → close and reopen the terminal*, at
  which point the per-pack `--qt-terminal-*` values (cursor `hsl(15 75% 62%)`
  Rains / `hsl(43 90% 60%)` Art Deco / `hsl(211 100% 65%)` Earl Grey) come
  through. A future walk should not re-report a live switch doing nothing.
  Item 1 (**the LLM Inspector hover, no visible change in Rains**) is **NOT a
  v5 bug and needs no v5 change**: `.qt-inspector-entry:hover` is byte-identical
  to v4, and it washes out in that theme because Rains sets
  `--qt-card-border: var(--stroke-strong)` at opacity **1** while the hover
  swaps in primary at **30% alpha** — the hover border is *fainter* than the
  resting one. v4 behaves identically; it is a v4 design weakness, recorded so
  it is not re-diagnosed. Items 2 and 6 produced findings **#43** and **#42**.
  The human confirmed at close: chips and chevrons now read correctly, and the
  **announcement-body formatting problem remains open by agreement** — the
  survey lane above owns it.

- **The 2026-07-28 walk, Part B — the embedding pendulum, MEASURED AND PROVEN
  on real data; and the walk that never happened, which is the point.** The
  script's first step was to *predict* what booting would cost before booting.
  That prediction is what caught v4's largest live bug of the port so far:
  **9,652 of 11,357 conversation chunks (85%) sat unembedded on the real Friday
  instance**, and the boot reconcile was about to "heal" them at ~$2 of OpenAI
  spend. Handing the diagnosis to a session in the v4 repo produced three v4
  fixes (`a0243abd`, `f7cc887b`, `a5d6cee5` — v4's own Bugs 6 and 7), which v5
  then re-ported as **P4.D25**. The cause was a **pendulum between two
  subsystems**: the stale-chat sweep deliberately cold-tiers quiet chats (NULL
  `renderedMarkdown`, NULL chunk embeddings), and the boot reconcile read that
  deliberate state as pipeline damage and re-embedded the whole cold tier on
  every boot, which the next sweep then cleared again. v4 measured 8,762 chunks
  embedded **exactly 6× each**.

  **The live proof, predicted in advance and then measured** (the prediction was
  written down *before* the boot, so it is a test rather than a rationalization):

  | | predicted | measured |
  | --- | --- | --- |
  | incomplete chats found | 598 | 598 |
  | skipped as stale | 595 | 595 (598 found − 3 enqueued, 0 unknown-activity) |
  | `CONVERSATION_RENDER` enqueued | 3 | **3** |
  | `EMBEDDING_GENERATE` calls | 5 | **5, all COMPLETED** |
  | new DEAD rows | 0 | **0** |
  | **second boot**: new renders | 0 | **0** |

  Pre-fix, the same boot would have enqueued 671 renders and ~9,609 embeddings.
  The second boot is the one that settles it: the pendulum's whole signature was
  *repeat* work, and a restart that enqueues **nothing** is the direct evidence
  that the ~$2-per-restart is gone rather than merely smaller this once.

  **The 5 chunks did NOT gain embeddings, and that is correct.** They are
  34k–117k chars — under v5's 131,072-char transport cap, over
  `text-embedding-3-large`'s 8,192-token (~31k char) context — so they fail
  deterministically, `is_permanent_embedding_error` marks them FAILED without
  retry, and the job COMPLETES. The FAILED rows could not land before (that was
  v4's Bug 7, `markAsFailed` silently no-opping), so **the first boot after the
  fix pays a one-time discovery cost and every later boot is free**: the
  reconcile's new FAILED exclusion retired those 3 chats immediately, 598 → 595.
  That self-termination is the convergence `a5d6cee5` was designed for, and it
  is why the residual is not a small pendulum of its own.

  **What the 9,656 unembedded chunks actually are** (the number that looked
  alarming and turns out to be mostly healthy): **9,098 cold-tiered by design**
  (within model context; warm on reopen, and that warmth now survives the sweep
  thanks to P4.D25 unit 2), **515 permanently stuck** over the model's context
  pending v4's renderer-side sub-chunking (a v4-side gap v4's own comment
  names), and **43 empty/over-cap**, correctly excluded by both apps.

  **Two items this left behind.** (1) The boot log is gated on
  `enqueued > 0 || failed > 0` (`quilltap-host/src/host.rs:805`), so the
  expected healthy outcome — `enqueued` ≈ 0 with `skipped_stale` large — prints
  **nothing**, and the fix's entire signature is invisible in the field. Logging
  sits outside the differential contract (P4.18), so nothing is red; it is a
  one-line rider (`|| skipped_stale > 0`, or log unconditionally when
  `incomplete_chats > 0`, which is nearer v4 — v4 logs a "found incomplete
  conversations" line before the loop and its completion line unconditionally).
  (2) The 515 oversize chunks are a **v4-side** item for the post-5.0 list:
  interchange sub-chunking, so a long interchange can embed at all.

  **The transferable lesson, for the next dogfood script:** on a real instance,
  *predict the cost of a destructive-or-expensive automatic action and check the
  prediction against the DB before triggering it.* Every number above came from
  read-only SQL run before the server ever started. Booting first would have
  spent the money, drained the backlog, and — worst — looked like a **success**
  ("the reconcile works, the backlog drained"), hiding a bug that had been
  burning ~$2 per restart in production.

- **The 2026-07-27 chat-action-round dogfood walk — Parts A–E, two fixes and two
  non-bugs.** The first pass over the chat-action-remainder round (P4.9E1A ∥
  P4.9E3A ∥ P4.9E1B ∥ P4.d22) plus the surfaces landed since 2026-07-24.
  **Part A (chat cast)** — add / create-NPC / edit / remove / rebuild all work;
  produced findings **#31** (off-scene announcement misattributed — FIXED, then
  confirmed live) and **#32** (the Speaking-As latch — FIXED). Item 7, the
  soft-removed participant reappearing in the off-scene picker, passes.
  **Part B (chat admin)** — the RNG gutter works end to end (chip → send →
  the character uses the number); **#33** (the tool card wears the last
  speaker's face) is v4-faithful and queued v4-first; **#34** — regenerate
  title / bulk reattribute / agent mode / merge have no UI at all, which is
  `p4.9e3` and was the walk script's error, not the app's.
  **Part C (Post Office)** — CLEAN. Insert Announcement with the
  generate→regenerate→edit→approve loop, Compose Mail (delivered and whispered
  correctly), and Whisper all render right.
  **Part D (the Pascal availability gate)** — **CLEAN, every item**, on real
  character sheets. This is the reassuring result of the walk: the surface was
  nine days old and a leaking gate fails silently — a withheld tool simply
  appears, and nothing announces it.
  **Part E (the Story's Clock)** — PASS, items 22–25. **This closes the two
  carry-overs owed since the 2026-07-24 walk** (Part F items 15 and 16): the
  fictional clock advances and reads its base in the story's timezone (the
  P4.d18 live proof), and the per-chat Core-whisper override persists through a
  reload and works.
  **One parity question raised and settled, recorded so it is not re-reported as
  a gap:** the fictional clock's base cannot be edited from the sidebar after a
  chat is created. That is v4's behavior — v4 renders `TimestampConfigCard` in
  exactly two places, `NewChatForm.tsx:672` and the character's Defaults tab
  (`ProfilesTab.tsx:385`), and nowhere in the chat settings tab or the sidebar.
  v5 has both of those consumers (`new-chat-form.ts`, characters
  `defaults-tab.ts`). Parity, not an omission.
  **Part G (chunk-on-write)** — the fix WORKS (both vaults created during the
  walk carry chunks written at creation), but checking it surfaced **#35**: the
  chunks are never embedded, because `EMBEDDING_GENERATE` has no handler. The
  step as scripted was also unrunnable — v5 has no semantic-search UI at all —
  so it was measured against the mount-index directly.
  **NOT walked — the next pass starts here:** **Part F** (Data & System: export
  / import execute / backup / restore both modes / delete-all / tasks queue /
  auto-lock / LLM log viewer — destructive, needs the scratch copy at
  `~/qt-dogfood-scratch`; four of these cards answered a refusal until
  2026-07-25 and are the largest untested surface in the port), and **Part H**
  (retrospective-recall live behavior — finding #28's downstream look, owed
  since 2026-07-24). Part H is unaffected by #35: memory embedding runs inline
  through `EngineAssembly.memory_embedding`, not through the dead job.

- **The 2026-07-26 `231be14c` drift-round dogfood walk — CLEAN, zero
  findings.** The round: P4.d18 (the fictional story clock) ∥ P4.d19 (the
  Pascal availability gate + tool vocabulary) ∥ P4.d20 (the Workbench gate SPA)
  ∥ P4.d21 (the in-chat two-phase dialog + the roll's outcome accent).
  VERIFIED on the Friday copy:
  - **Part A, the story clock, all seven steps.** The two rewritten card
    strings; a zone-less `datetime-local` base of `1550-07-25T10:15` rendering
    in **1550** rather than a 1970-adjacent instant (the port bug the round was
    planned around); `Europe/Istanbul` printing **+01:56**, which is the live
    confirmation of the *unpredicted* sub-minute-LMT fix that only the widened
    corpus had caught; the clock **advancing ~1:1 with the wall clock** between
    two sends; both `EVERY_MESSAGE` and `START_ONLY` read off the LLM
    Inspector; **and the boot-repair backfill proven on real migrated data** —
    the oldest existing fictional chat came back anchored and moving instead of
    frozen. P4.d18's tier-3 contingency therefore never had to fire.
  - **Part B, the availability gate, steps 8–12.** The `gated` badge (ordered
    between `disabled` and `whisper`), the proving-bench verdict against a
    mock fact sheet, both clause directions — **and step 11, the live half:**
    a withheld tool absent from the in-chat roster and unrunnable by name
    against a real character's `metadata.json`, then appearing once the sheet
    matched. That is the only end-to-end proof of the gate outside the
    Workbench's client-side evaluator.
  - **Part C, the in-chat Pascal SPA, steps 15/16 + 18–22.** The wand opening
    a **modal** (Escape / backdrop / Cancel all dismissing), the
    "Choose another tool" round trip with no state leakage, the
    "What this tool can quote" reference panel (P4.d21's ACTIVATE-AT-UNIFY
    surface, live on real definitions), the stacked params layout with the
    Workbench's `inline` consumer unchanged, and **the roll announcement
    wearing its own outcome accent + state dot** for both a success and a
    failure outcome, under two theme packs — the `.qt-pascal-result` base
    block v5 had never had at all.

  **NOT walked — three named steps, the next pass may pick them up
  cheaply:** step 13 (the gate runs BEFORE the `disabled` tombstone, so a
  gated-out definition leaves its name claimable by a farther tier — needs two
  same-named tools in different tiers, which is why it was skipped); step 14
  (a `"parameters": []` paste should read **`expected record`**, not
  `expected object` — the only live check on the third pre-existing bug this
  round fixed, and that sentence is user-visible payload returned verbatim by
  two routes); step 17 (the roster search box past six tools).

  **The recorded `TimestampConfigSchema` divergence did not bite at the SPA
  level** — step 7 saved partial configs without incident. It remains
  invisible without querying the stored JSON, so it stays recorded (see the
  round record and the P4.d18 unit-2 lane record) rather than promoted.

- **The 2026-07-24 post-rewrite dogfood walk — coverage summary.** WALKED CLEAN
  on the Friday copy: **Part A** tool use across OpenAI/Anthropic/DeepSeek (#25
  + #22 CLOSED; the embedded P4.17 card; #29/#30 surfaced as v4-faithful),
  **Part B** the context-summary fold + cheap-LLM config (#26 + #27 CLOSED —
  three fold cycles on chat `e71847c4`, all on `deepseek-v4-flash`; 66/66 AUTO
  memories carry `occurredAt`), **Part C/llm-consult** fired live (a 2937-char
  `CUSTOM_TOOL_CONSULT`), **Part E** the recall-replay CLI, and **Part F**
  items 14 (a composite outfit resolved all components) and 17 (a heavy
  character's tabs driven by card bodies + keyboard). **P4.13 unit 9 is
  complete — the provider-I/O round can close.** NOT walked this pass (the next
  pass starts here): Part D retrospective-recall live behavior (the #28
  downstream look — classifier fires but no whisper), Part F items 15 (Story's
  Clock + a narrated time jump) and 16 (per-chat Core-whisper override +
  chat-tier State Editor opener), and items 10/11 (a metadata-reading Pascal
  tool that consults an LLM — doubly blocked by #30, deferred). No
  legacy/broken-vault characters exist on this instance (item 18 N/A).
- **Part E (recall-replay CLI) walked CLEAN 2026-07-24 — the P4.d13 live
  proof, plus two working-as-designed observations recorded so they aren't
  re-reported.** `quilltap recall-replay <chatId>` ran against the Friday copy
  (chat `e71847c4`, turn 40/40), made its one cheap call, and printed the
  OLD-vs-NEW ranking comparison. **(a) NEW == OLD is EXPECTED for a query with
  no live differentiator.** The replayed turn's signals were "not retrospective
  · timeRange — · entities Charles Sebold, Quilltap Estate, Paris 1925"; the
  retrospective/window boosts correctly stay inert, and the entity-anchor union
  found no match (64/284 of that chat's memories carry `entities`, but NONE hold
  those three strings — stored entities are names like `Abigail`/`Charlie` and
  extraction fragments like `True`/`North`), so the two paths are identical. To
  see a divergence, replay a turn the classifier flags `retrospective:true` or
  one whose query entities match stored `entities`. The ranking DATA is pinned
  v4-equal by `recall_replay_equivalence` (Tier-3, both paths). **(b) The
  `———semantic` column mash is v4-faithful**, not a v5 render bug: a `None` cell
  is `DIM + '—' + RESET`, whose ANSI escape bytes exceed the column width, so v4's
  (and v5's) `padEnd` adds zero padding and the next cell butts against it
  (documented at `recall_replay_cmd.rs:130` `pad_end`).

## Standing notes for the next orders

- **A LINT RULE WOULD CLOSE THIS CLASS (finding #51, 2026-08-02) — an Angular
  `output()` must never be named after a DOM event.** Two live bugs came from
  one mistake, and one of them (a Cmd+C in the Salon writing "undefined" over
  the user's clipboard) had nothing to do with the feature that surfaced it.
  The trap is invisible on inspection: the binding `(copy)="copy.emit($event)"`
  reads exactly like every other output forward on the same element, and the
  component's own emit is correct — the second, unwanted delivery comes from
  the DOM, only at runtime, and only when the event originates *inside* the
  component's subtree. **The four fixed names were found by grepping
  `readonly (change|input|submit|select|reset|keydown|keyup|click|copy|paste|
  cut|drop|toggle|search|scroll) = output`**; that grep is the whole audit and
  it takes seconds, so the durable form is an ESLint rule over
  `output()` declarations (deny the DOM-event names outright) rather than a
  note asking future authors to remember. Until one exists, re-run the grep
  whenever a component gains an output — and note the *cleared* five are only
  cleared while their templates stay free of `<input>`/`<textarea>`/`<form>`,
  which no test enforces.

- **OWED E2E BEAT (finding #50, 2026-08-02) — the Project library button has no
  end-to-end coverage, and the reason is a fixture gap worth fixing once.**
  The fix is unit-covered and mutation-proven, but **no seeded chat in ANY e2e
  instance lives inside a project with an enabled official mount** — checked in
  both the salon fixture (`global-setup.ts` seeds no `chats.projectId`) and the
  projects instance (its beats drive project *detail* screens, never a chat
  within one). So the button cannot be reached by any existing beat. **What it
  needs:** one chat whose `projectId` names a project whose official document
  store is enabled and holds a file, then a beat that opens the picker, clicks
  `.qt-doc-project-library`, and asserts the project's file lists. That seed
  would also unlock the wider blind spot this finding exposed — **every
  project-scoped chat behavior is currently untested end to end**, which is
  precisely why a button missing since P4.6x survived until a human opened the
  picker on real data. Worth taking with the next round that already touches
  the e2e instance.

- **OWED CHECK (P4.D40's ruling, 2026-08-02) — scan the STORE-BACKED documents
  for the list-indent edge shape.** The ruling that v5 keeps its CommonMark
  list behavior is evidence-conditional, and only half the evidence has been
  gathered. The shape is a list child indented deeper than its parent but short
  of that parent's content column (`1. a` with a 2-column `- b` under it): v4
  nests it, v5 reads two sibling lists, and — the part that matters — v5 writes
  the flattened form back on the next save, so the nesting intent is lost
  permanently. That is the same failure shape v4's own `4f088e7c` existed to
  fix, which is why hits would reopen the ruling in favour of porting v4's
  `normalizeListIndentForLexical` pre-pass.

  The driver is committed: `harness/tools/list_indent_edge_scan.py`. It ran
  clean (**0 hits**) over the dogfood copy's *disk-backed* markdown, but that
  was only 3 user documents — the real corpus lives in the encrypted document
  stores, which need the real pepper. **On the next pass:** export the Friday
  copy's store-backed documents to a directory (the `quilltap` CLI, or the
  Scriptorium export) and run the scanner over it. Report the count either way;
  a clean result closes the question for good, and any hit is a port item with
  its repair already identified.

- **NEEDS AN ORDER (finding #47, 2026-07-31) — the character vault is the bag
  `dcd9440a` missed, and a corrupt `properties.json` destroys six fields on the
  next edit.** `dcd9440a` (ported as P4.D29) hardened the document-store overlay
  so a failed `properties.json` read can no longer let a patch clobber a
  settings bag — but it covered only the two `StoreEntity`s (groups, projects).
  The **character vault** carries the same shape and was left out:
  `vault_character_update::read_current_properties` returns `None` on a parse
  failure and the RMW seeds `empty_properties_default()`, projecting defaults
  over `pronouns` / `aliases` / `title` / `firstMessage` / `talkativeness` /
  `canChooseOutfit`. **Measured, not argued:** `characters` has 28 columns and
  none of the six, so the file is their only home — and the loss was reproduced
  end-to-end on the Friday copy. **v4 has the identical defect** and, worse,
  carries a comment at the write site asserting the opposite (*"Every other
  field above has a DB column"*), which the vault cutover invalidated; anyone
  auditing v4 would read that line and move on. **Scope:** mirror `dcd9440a`'s
  shape — refuse the patch when `properties.json` is **present but
  unparseable**, keep seeding on genuine absence (P4.D29's Epsilon arm), and
  prove both arms plus a "wrote nothing" post-state in the characters corpus.
  Mutation-proof it: P4.D29's own family was green on first run and needed its
  sensitivity produced. **v4-first is the natural default here (the stale
  comment is a v4-side repair too), but this is data loss on real user data, so
  it wants a ruling rather than a default.**

- **NEEDS AN ORDER (finding #42, 2026-07-31) — v5 has NO toast system, and 103
  v4 files depend on one.** v4's `lib/toast.tsx` (`showSuccessToast` /
  `showErrorToast` / `showInfoToast`) is used by **103 files** across
  `components/` and `app/`. v5 ported none of it, so every dialog that v4
  finishes with a toast either grew an improvised inline error paragraph (the
  14 `qt-text-danger` sites of finding #42) or **says nothing at all** — the
  latter is the unmeasured half and the reason this needs a survey, not just a
  component. v4's `ReattributeMessageDialog.tsx` is the worked example: it has
  no inline error markup whatsoever, only `showErrorToast`, so v5's inline
  paragraph there is an invention, not a port. **Scope:** port `lib/toast.tsx`
  and its host, then walk the 103 call sites and decide per site whether v5's
  improvised inline error is retired in favour of the toast (most) or kept
  alongside it. **Until it lands, `qt-text-danger` is defined (finding #42) so
  the improvised errors at least read as errors** — that definition is the
  stopgap, and the order should revisit whether it survives. Note the success
  and info toasts matter too: silent success is its own bug class, and nothing
  in v5 currently reports one.

- **NEEDS A SURVEY LANE (finding #43, 2026-07-31) — announcement rendering,
  systematically.** The human's report is broader than what #43 fixed: "almost
  no announcement in the UI is styled correctly. V4's announcements are way
  better and more consistent than this," and it has been true "for a while."
  **What was checked and RULED OUT as the cause**, so a lane does not re-tread
  it: the `.qt-chat-announcement-*` and `.qt-chat-system-bar-*` CSS blocks are
  byte-identical between the apps; the `--qt-chat-system-bar-*` /
  `--qt-chat-announcement-dot-*` variables are mirrored; and
  `.qt-chat-message-system`'s `text-sm italic text-center py-2` (`_chat.css:230`
  in both) is v4's own rule, so centered italic announcement bodies are
  faithful. **The one concrete lead found and NOT taken:** v5's expanded
  announcement body renders `<qt-message-content [content]="…content">` where
  v4's `MessageRow` passes `renderedHtml={message.renderedHtml}` plus
  `renderingPatterns` and `dialogueDetection` — so a v5 announcement body may be
  re-rendering from raw markdown and skipping the roleplay/dialogue pattern
  pass that every other message gets. That is the first thing to measure. The
  lane should then walk each `systemSender`/`systemKind` pair against v4
  side-by-side rather than sampling, since the complaint is about consistency
  across kinds.

- **NEEDS AN ORDER (finding #40, 2026-07-31) — `LLM_LOG_CLEANUP` has no handler,
  so v5 never prunes `llm_logs`.** It is the **last** job type in
  `KNOWN_JOB_TYPES` without a registered handler (proven by
  `SELECT DISTINCT lastError … LIKE '%not yet available%'` on the Friday copy:
  one string, this one). The enqueue path is live on the daily cadence *and*
  immediately at startup (`quilltap-host/src/host.rs:1097` →
  `queue_service::run_scheduled_cleanup`), so every boot mints a job that
  burns three attempts and dies. **Scope:** (1) `cleanup_old_logs(user_id,
  retention_days) -> count` on the llm-logs partition — v4
  `lib/database/repositories/llm-logs.repository.ts:368`: `retentionDays < 0`
  → 0 with a warn, cutoff via **calendar-day** arithmetic
  (`cutoff.setDate(cutoff.getDate() - retentionDays)`, *not*
  `now - N×86400000` — they differ across a DST boundary), delete where
  `userId` AND `createdAt < cutoff.toISOString()`; (2) the handler — v4
  `lib/background-jobs/handlers/llm-log-cleanup.ts` (73 LOC): retention from
  the payload else `chatSettings.llmLoggingSettings?.retentionDays ?? 30`,
  `<= 0` → return, settings present but logging disabled → return; (3)
  registration in `ProductionSpineFactory`; (4) **a tier-2 differential over a
  seeded llm-logs partition** — this is a partition write, and the P4.6bj
  precedent says a job handler owes one. **Why it is worth doing promptly
  rather than at leisure:** the DEAD-row trickle is cosmetic, but the retention
  window on the real instance is being maintained by **v4**, not v5 — the
  Friday copy's llm-logs partition is 416 MB for a 7-day / 7,559-row window at
  ~1,080 rows/day with verbose mode on, and the day v5 is the only app that
  number stops being bounded. Note the sibling enqueuer bug found alongside it
  (finding #41) is already fixed; `run_scheduled_cleanup` still has **no**
  differential coverage, which is how #41 survived, so the order should cover
  the enqueuer as well as the handler.

- **NEEDS AN ORDER (finding #37, 2026-07-29) — image attachments never reach
  the LLM wire.** The non-streaming completion path
  (`model/completion_provider.rs::request_input_from_params`) builds each
  message from `m.content` and drops `params.attachments`; `RequestInput` /
  `StreamMessage` have no image field, and no request builder emits any
  provider image part (no `data:image` / `image_url` / `inlineData` anywhere in
  `model/request_builder.rs`). Proven for the vision-describe path (the model
  gets text only and hallucinates a generic portrait). **Scope to establish
  FIRST:** whether in-chat vision (a user image to a vision-capable model) is
  also affected — likely, since the wire construction simply doesn't exist, but
  unconfirmed. **The order must:** (1) add an image-content representation to
  the request layer; (2) serialize it per provider (OpenAI-compatible
  `image_url` data URL, Anthropic image `source`, Google `inlineData`) —
  byte-matched to v4's `LLMMessage` attachment casting; (3) **add the wire
  differential that was missing** — the describe differential passed green
  because the canned provider keys on `canned_completion_key_with_attachments`,
  so the test substrate read the attachment while the real wire silently
  dropped it (the #36 / P4.11-one-mode blind-spot class again — a corpus that
  can't see the payload proves nothing about it). Until it lands, every image
  description on real data is a confident fabrication, which is worse than a
  visible failure because it gets memorized into the conversation.

- **RULED 2026-07-23 (human) — a failed cheap-LLM call MUST write an
  `llm_logs` error row. This is an accepted, deliberate divergence from v4**,
  which logs nothing on failure (`CheapLlmTaskExecutor::log_call` writes only
  on success; v4's `logLLMCall` is on the success path only). The evidence:
  finding #23 (a total cheap-LLM outage that presented as "jobs COMPLETED,
  nothing minted") and finding #26 (a fold that silently never happens) both
  cost hours precisely because the failure arms leave no trace anywhere — and
  v5, unlike v4, has no console logging to fall back on. **ORDER WRITTEN:
  `work-orders/p4.13-provider-io-rewrite.md` unit 6** (2026-07-23);
  the shape is: an error row per failed cheap call carrying the provider,
  model, task type, and the error text, distinguishable from a success row.
  Note v4 DOES `logger.error` at several of these call sites
  (`memory-trigger.service.ts:132`), so the divergence is narrower than it
  looks — v4 surfaces the failure to its console, v5 has no console to
  surface it to. **The related open question — whether the server should have
  a tracing subscriber at all — was RULED 2026-07-24 (human): arm (a), adopt
  `tracing` + `tracing-subscriber`, and LANDED as P4.18.** The three bins now
  init a stderr fmt subscriber env-filtered by `RUST_LOG` (default `info` —
  v4's `LOG_LEVEL` INFO analog), and the surveyed swallow sites (the job
  runner, the spine's transport-shell error frames, the host pump/seeding, the
  cheap-LLM failed-call row, the state cascade, the mount-index repair) emit
  structured events. Log output is operator output, not data — no differential
  applies (a first for this port). File-transport parity (v4's
  `combined.log`/`error.log` rotation) stays tier-3, deferred until asked.
- **SEQUENCING RULED 2026-07-23 (human): the provider-I/O rewrite lands
  FIRST, then a dedicated dogfood-fixing run, then a fresh dogfood walk.**
  The open dogfood findings are deliberately NOT being fixed piecemeal
  before then — most of them live in or adjacent to the seam the rewrite
  restructures, so fixing twice is waste. Open at the close of the
  2026-07-23 walk: **#25** (tool linkage never reaches the wire — order
  `p4.12` written, RULED 2026-07-23 to FOLD INTO the rewrite rather than land
  ahead of it; tool use stays broken until the rewrite ships; **the rewrite
  round is now PLANNED: `work-orders/p4.13-provider-io-rewrite.md` ∥
  `p4.14-memory-sort-total-order.md` ∥ the pre-existing `p4.10`**), **#26** (the fold never fires — unlocalized), **#27**
  (corpus-shaped cheap-LLM config in `run_summary_check`), **#28** (the
  retrospective classifier never returns true — needs a v4 bench
  comparison), the **memory-injector sort panic** (LOCALIZED to
  `format_dynamic_memory_head`; NOT in the seam the rewrite touches, so deferring it behind the rewrite saves no duplicated work — it crashes turns today), and the error-row logging ruled above.
- **Walk scope at the 2026-07-23 close — what was and was not covered.**
  WALKED CLEAN: title generation, AUTO memories dated to story dates (the
  P4.d14 live proof), avatar regeneration, automatic story-background
  generation (the P4.6ao live proof), the Memories tab at ~26.5k rows
  (wheel-scroll pagination + filter + sort), ChatSidebar all four sections
  in group and project chats, sidebar collapse/expand/resize/reload, and
  finding #24's live confirmation. **NOT WALKED — the next pass starts
  here:** outfit selection, Story's Clock + a narrated time jump, the
  recall-replay CLI, date-ranged memory search about another character,
  the per-chat Core-whisper override + chat-tier State Editor opener, a
  heavy character's tabs via card bodies + keyboard, and legacy/odd vault
  rows. BLOCKED until the rewrite: context summary + fold-episode (#26),
  llm-consult and in-chat Pascal and anything requiring a character to USE
  a tool result (#25).

- **PROPOSED (2026-07-23, human) — refactor the provider I/O layer as an
  accepted divergence from straight-port fidelity.** Findings #23 (every
  non-streaming request sent `stream:true`), #24 (every non-streaming
  OpenAI/Grok response parsed empty) and #25 (tool linkage never reaches the
  wire) are three total outages in one seam within two days, none caught by a
  green differential suite. The structural cause: v5 reproduced the SHAPE of
  v4's provider plugins — per-provider build/parse pairs behind a
  lowest-common-denominator interface — but that shape exists because v4 must
  load providers dynamically as JS at runtime. **v5 has no plugins**, so it
  carries the interface's costs (lossy intermediate types, duplicated per-family
  parse, a boundary no type checks) with none of its benefit. The proposal:
  restructure so provider differences are data + one typed pipeline, with
  message/response types that can represent everything the wire needs, and
  illegal states unrepresentable (a tool result without a call ID should not be
  constructible). **The invariant that must NOT move: the wire bytes stay
  byte-faithful to v4** — internal structure is free, the request/response
  corpus is the contract, and the refactor is only safe because that corpus
  exists. It needs the two missing legs first (a recorded-body response-parse
  corpus per #24; call-site pins per #25).
  **RULED 2026-07-23 (human), three parts:**
  1. **Drift risk accepted** — the human commits to keeping v4 pure for provider
     I/O unless a major upstream breakage forces a change. A restructured v5
     provider layer therefore does not owe v4 re-portability.
  2. **NOT a precedent.** The divergence is scoped to provider I/O ONLY. v4's
     general shape is retained everywhere else for the rest of the port. Where
     something else is ugly purely because it was inherited from the Node
     backend or the React frontend, that is grounds to *revisit case by case*,
     not licence to restructure. (This overrides the "state it as a precedent"
     recommendation made when the proposal was raised.)
  3. **Sequencing stands:** the verification legs land BEFORE the restructuring
     (P4.12 → the recorded-body response-parse corpus → the refactor proper).
     P4.12 unit 1's type work is the refactor's first piece either way.
- **Post-5.0 intent (human, 2026-07-23) — a thorough de-Node refactor, AFTER
  release.** 5.0 = the full working port of 4.8.0, no Node backend, no React
  frontend, the app's shape as it stands today. Once that is released and
  working in production, the intended follow-on is a deep refactor of everything
  Node was constraining: the multithreading model (removing async/await shapes
  that exist only as workarounds for a single-threaded runtime), and a broader
  move to WebSockets, among others. **Nothing in the port should be
  pre-emptively restructured for this** — it is the reason a merely-inherited
  awkwardness can be left alone now and revisited later, and the reason the
  provider-I/O divergence is deliberately a one-off rather than a first step.
- **Post-5.0 v5 DIVERGENCE (human, 2026-07-29) — multi-turn impersonation that
  actually speaks as the chosen character (finding #39).** Today "Speak as X" is
  v4-faithful: it only relabels your message if X is `controlledBy === 'user'`,
  so impersonating an AI character silently falls back to your own seat. The
  wanted behavior: flip Impersonate on a character and, for an arbitrary number
  of turns, your typed messages come out as that character; flip it off and the
  character resumes being driven by its own LLM. **The key design realization
  (human): this is a BEHAVIOR change, not a schema change.** Do NOT mutate the
  participant's `controlledBy` or `connectionProfileId` — leave both untouched.
  The impersonation flag is ALREADY persisted (`chats.impersonatingParticipantIds`
  + `chats.activeTypingParticipantId`), so nothing new needs storing and there is
  nothing to "restore" on flip-off (the original profile was never disturbed).
  Turn resolution gains one overlay check: *it's this participant's turn → send
  to their recorded LLM **unless** they are impersonated, in which case it is the
  user's turn and the message is attributed to them.* The LLM assignment is
  consulted AFTER the impersonation check rather than being the sole gate. Two
  code sites currently gate on `controlledBy === 'user'` and would instead gate
  on `controlledBy === 'user' || id ∈ impersonatingParticipantIds`: (1) message
  attribution — `findActiveUserParticipant` (v4 `turn-manager/utils.ts:99-107`,
  v5 mirror feeding `orchestrator.rs:722`); (2) who-responds resolution — the
  turn manager must exclude an impersonated character from LLM auto-response and
  treat their turn as a user turn. ⚠ This is differential-verified CORE
  turn-resolution, so the divergence must move the affected differentials
  deliberately (`chat_cast_routes_equivalence` and the turn-chain families) — not
  a quiet edit. Deferred post-5.0 by the human 2026-07-29. Whether v4-first or
  v5-only is a design-time call (v4 has no such behavior, so leaning v5-only).
- **Post-5.0 v4-side ITEM (2026-07-28) — interchange sub-chunking, so a long
  interchange can embed at all.** Found while proving P4.D25 on the Friday copy:
  **515 conversation chunks are permanently unembeddable** on that instance —
  over `text-embedding-3-large`'s 8,192-token (~31k char) context but under the
  131,072-char `EMBEDDING_MAX_CHARS` transport cap. Both apps behave correctly
  (`isPermanentEmbeddingError` / `is_permanent_embedding_error` marks them FAILED
  without retry, and since v4 `a5d6cee5` the reconcile then excludes them
  forever), so nothing retries and nothing accumulates — but those interchanges
  are silently absent from semantic search and always will be. v4's own comment
  already names the repair: *"oversized interchanges await renderer-side
  sub-chunking."* This is **v4-first** — the renderer is shared oracle surface,
  and a v5-only split would move the diff. Not urgent: it is a slow quality
  ceiling, not a failure. **v5 reproduces v4 exactly here and should keep
  doing so** until v4 moves.

- **v4-side one-liner (ruled 2026-07-31) — percent-encode `'` in the
  `Content-Disposition` ext-value (finding #46).** In
  `lib/api/content-disposition.ts`, `encodeURIComponent(filename)` leaves a
  straight apostrophe unescaped, and RFC 8187 uses `'` as the delimiter in
  `charset'language'value` — so any filename carrying both an apostrophe and a
  non-ASCII character emits a parameter browsers discard, and the download falls
  back to the underscored ASCII name. One line:
  `encodeURIComponent(filename).replace(/'/g, '%27')`. v5 already diverges here
  (ruled fix-v5-now), and `markdown_transcript_equivalence` carries an
  `EXPECTED_DIVERGENCES` entry that **fails loudly the moment v4 agrees** — so
  landing this v4 fix will turn that family red on purpose, and the carve-out
  should then be retired rather than the fix reverted. Not urgent (a cosmetic
  filename, no data at risk), but cheap.

- **⚠ v4-side URGENT — NOT post-5.0 (ruled 2026-07-31, human): a corrupt
  character-vault `properties.json` destroys six fields on the next edit
  (finding #47).** This is the one v4-side item on this page that should NOT
  wait for retirement, because **v4 runs against live Friday** and the loss is
  silent and permanent. An unparseable `properties.json` makes the RMW seed
  `empty_properties_default()`, and the next character save projects defaults
  over `pronouns` / `aliases` / `title` / `firstMessage` / `talkativeness` /
  `canChooseOutfit` — none of which has a DB column any more (28 columns on the
  real instance, not one of the six). It needs a corrupt or truncated file
  first, which is not an everyday event — but iCloud sync conflicts and
  interrupted writes are exactly how it happens.

  **Two v4 edits**, in `lib/database/repositories/vault-overlay/managed-fields.ts`:
  1. guard the RMW seed against a `properties.json` that is **present but
     unparseable** (genuine absence must still seed — that is provisioning);
  2. **delete the stale comment at ~:236** — *"Every other field above has a DB
     column, so 'the caller passed nothing' safely reads as 'the value is
     empty'"* — which was true before the vault cutover and is false now. It is
     listed second but matters nearly as much: it is a safety argument that
     tells any future auditor the hazard cannot exist.

  `dcd9440a` already fixed this exact shape for the two `StoreEntity`s
  (groups, projects); the character vault is the third bag and was missed.
  ⚠ **Landing the v4 fix MOVES THE ORACLE** for the characters families — see
  `work-orders/p4.22-character-vault-properties-clobber.md`, which reclassifies
  from "deliberate divergence" to "ordinary drift re-port" if v4 goes first.

- **Post-5.0 v4-side FIXES (human, 2026-07-24) — real v4 bugs v5 has already
  fixed, whose v4 half is queued rather than dropped.** Distinct from the
  papercut list below: these ARE bugs, and v5 does NOT reproduce them. What is
  queued is the change to v4 itself, so instances still running v4 before
  retirement get the fix too.
  > ### ✅ THE WHOLE LIST BELOW IS CLOSED — v4 fixed all four itself (2026-07-26)
  >
  > `67ffb444` (`fix(backup): restore brings back the stores, the links, and the
  > files`) and `c1507f47` (`fix(import): the blob reader waits for every chunk
  > before it signs`) landed both entries' fixes upstream: the mount-index
  > coercion, the `>= 2` gate, the files-phase move, and the sparse-array blob
  > reader. **v4 instances no longer carry any of these bugs, so nothing here is
  > owed to the v4 side any more.** The entries stay as written for history.
  >
  > Both v5 tripwires fired on the first regenerated oracle and were retired by
  > **P4.d22** (2026-07-26), which also moved the oracle baseline to `c1507f47`.
  > Two things came OUT of that convergence rather than into it, and neither is
  > v4's: a v5 gap the count-level pin had hidden (restored stores came back with
  > empty pattern arrays, and an INTEGER `0` policy flag read as `true` — fixed),
  > and an **open ordering question** about where the files phase sits, which is
  > awaiting a human ruling. See `status-log.md` → "Lane record — P4.d22 units
  > 2–3".
  - **v4 cannot re-import its own export of a document-store blob over 3 MB**
    (the sparse-array blob divergence, ruled 2026-07-24 — see
    `status-log.md`). `assembleExportFromStream`'s
    `received.every(v => typeof v === 'string')` runs over a SPARSE
    `new Array(chunkCount)`; `every` skips holes, so the blob finalizes on its
    FIRST chunk, silently truncates, and the next chunk throws
    `received without preceding doc_mount_blob` — the whole import fails. The
    fix is one line in v4 `lib/import/quilltap-import-stream.ts:283`:
    `received.filter(v => typeof v === 'string').length === accum.chunkCount`.
    v5's reader already waits for every chunk, so a v5 instance is unaffected;
    this is purely so a v4 user can restore a large-blob backup. **Queued at
    the human's request 2026-07-24** — the ruling had left it as "worth doing
    only if a v4 user hits this first", and the human (who runs v4 on Friday)
    judged that it will bite. Deliberately NOT done during the port: it moves
    the oracle baseline mid-flight.
  - **⚠ RULED 2026-07-25 — v5 DIVERGES; the v4-side fixes are QUEUED HERE (found
    2026-07-25, P4.9G5 unit 4). ⚠ MORE URGENT THAN THE SPARSE-ARRAY ENTRY ABOVE:
    that one needs a >3 MB blob to bite, this one bites EVERY modern restore. v4 cannot
    restore a modern backup's document stores, and restores no user files at
    all.** Two separate bugs, both demonstrated by running v4's REAL `restore`
    against v4's REAL backup of a modern instance (the `system-restore` oracle's
    Part 2; full evidence and warning text in the lane record):
    1. **Every `doc_mount_points` and `doc_mount_file_links` row is rejected.**
       `dumpMountIndexTable` (`backup-service.ts:72`) is a raw `SELECT *`, so
       the archive carries `includePatterns`/`excludePatterns` as JSON *text*
       and `enabled`/`allowEmbed` as INTEGER 0/1 — and `restore.ts` feeds those
       to repository `create`s whose Zod schemas demand `string[]` / `boolean`.
       The folders, file rows, documents and chunks DO restore, so the result is
       a graph with all the content and none of the stores or links that reach
       it: **every character vault, project store and group store comes back
       unreachable.** The fix is on the BACKUP side (parse the JSON columns and
       coerce the booleans in `dumpMountIndexTable`) or the restore side
       (coerce before `create`) — the human's call which.
    2. **No user file is restored.** `getFileFromExtractedBackup`
       (`archive.ts:334`) gates the `files/<storageKey>` lookup on
       `backupFormat === 2`, but a modern manifest declares `backupFormat: 4`.
       One-line fix: `backupFormat >= 2`.
    **RULED 2026-07-25 (human): "I want this work, not just fail the same way v4
    fails" — v5 diverges on both.** Full ruling: `status-log.md` → "Ruling — the two
    v4 restore bugs (2026-07-25)". Finding 1 needs no v5 change (its typed readers
    already coerce); finding 2 DOES — v5 currently reproduces the `=== 2` gate and
    must move to `>= 2`. Reader-side only: v5's writer stays byte-identical to v4's.
    **v4 itself is still unfixed**, so a real v4 restore today still loses every
    store — that is what makes this entry the more urgent of the two.
- **⚠ OPEN, added 2026-07-26 (P4.d23) — v4 loses archived link rows on every
  second-generation restore; v5 no longer does.** The four entries above are all
  closed; this one is not, and it is a NEW ruled divergence rather than a
  leftover. v4 re-ingests every user file in an archive unconditionally, so its
  replay writes into `restored/<name>` — which, for a backup taken from an
  instance that was itself restored, is exactly where the ARCHIVED link rows for
  those files already live. v4's replay gets there first (`22a-bis`) and the
  archived rows are then refused. **Measured, not reasoned** (`system_restore_state`
  → `restore_gen2_replace`, over the committed `restore-archive-gen2.zip`):

  ```text
  Failed to restore doc-store folder "restored": UNIQUE constraint failed: …
  Failed to restore doc-store file link "restored/portrait.png": UNIQUE constraint failed: …
  Failed to restore doc-store file link "restored/ledger.txt": UNIQUE constraint failed: …
  ```

  The bytes survive (the replay wrote its own copy) but the archived link IDS are
  lost, and the store rows duplicate again on every restore generation. **v5
  restores that same archive with zero warnings**, because it now recognises that
  the archive already carries the store rows for a file and skips re-ingesting it
  (`orchestrator.rs` → `carried_store_rows`; ruled 2026-07-26, `status-log.md` →
  "Ruling — the restore file-replay dedupe"). v4 names this repair itself and
  puts it out of scope (`found-bugs.md:400-402`) — **the v5 implementation is the
  evidence that it is worth taking there**: it is a small check, it needs no
  phase-order change, and it removes a data loss v4 currently logs as three
  warnings and carries on past. Not done during the port: editing v4 moves the
  oracle baseline mid-flight.
- **⚠ THE `EMBEDDING_GENERATE` HANDLER IS UNPORTED, AND IT IS COSTING DATA NOW
  (finding #35, 2026-07-27) — this is the strongest candidate for the next
  round.** It was a known, recorded deferral; what the dogfood walk added is the
  measurement, and the measurement changes its priority.
  **What is broken:** both enqueue paths are live (`queue_service.rs:192` for
  memories, `mount_index/embedding_scheduler.rs:45` for mount chunks); no
  handler is registered; every job retries three times and dies. On the Friday
  copy that is **2,088 DEAD rows and a console warning every few seconds.**
  **Why it is worse than a missing feature:** the damage accrues silently and
  does not heal. Every v4-era vault is 100% embedded; every chunk written since
  v5 took over has zero embeddings, so semantic search over new content returns
  nothing rather than erroring. A user adding a character today gets a vault
  their characters cannot search, with no indication anything is wrong. And
  there is no manual repair — `quilltap docs embed` enqueues to the same dead
  handler, `docs status` refuses, and `EMBEDDING_REFIT` (which IS registered)
  refits existing vectors rather than making absent ones.
  **Sizing:** v4's `lib/background-jobs/handlers/embedding-generate.ts` is 490
  LOC over four entity types — `MEMORY`, `CONVERSATION_CHUNK`, `HELP_DOC`,
  `MOUNT_CHUNK` — plus a vector-store write and a mount-chunk cache
  invalidation. **The embedding provider seam already exists and is wired live**
  (`EngineAssembly.memory_embedding`, since the P4.6s round), so this is a
  handler port over an available capability, in the shape P4.6bj already
  established for `MEMORY_EXTRACTION` / `CONTEXT_SUMMARY`.
  **Port v4's `isPermanentEmbeddingError` with it, deliberately.** v4's own
  comment explains it was added because "tens of thousands of DEAD
  EMBEDDING_GENERATE rows had accumulated" from deterministic failures being
  retried forever. A port that omits it inherits that bug on day one — and v5
  has already demonstrated it can produce 2,088 of them in a single day.
  **Also owed with it:** a decision about the DEAD backlog on instances that ran
  v5 before the handler landed. Those chunks are not re-enqueued by anything;
  something has to sweep them, or they stay unsearchable forever.
  **UPDATE (2026-07-27, lane P4.6BL): FIXED ON THE LANE BRANCH — the finding
  CLOSES at the next dogfood walk's live proof, not before.** The handler is
  ported over all four entity types (`services/embedding_generate_job.rs`,
  incl. `isPermanentEmbeddingError` + the empty/oversize guards) and registered
  live in the spine bundle; a tier-3 differential drives v4's REAL handler +
  REAL queue claim/retry/DEAD machinery (18 processed steps, 8 tables). The
  backlog heals three ways: a **boot repair pass** re-enqueues every
  recoverable un-embedded conversation chunk on startup (a deliberate v5-only
  repair — v4's own reconcile enqueues CONVERSATION_RENDER, whose handler v5
  still lacks); mount chunks re-enqueue on the existing mount-refresh sweep;
  memories via the backfill route plus the newly un-refused
  `memoryGenerateEmbeddings` / `memoryRebuildIndex` repair verbs (tier 2 of
  the same lane). The DEAD rows themselves stay (dedup ignores them; they are
  visible and deletable in the Tasks Queue) — v4's mass-cancel shape lives in
  the still-deferred EMBEDDING_REINDEX_ALL. The e2e instance has no API keys
  by design, so the live proof (real embeds on the Friday copy) is the walk's.

- **⚠ v4-SIDE (post-5.0), added 2026-07-27 (P4.9E4B, rider C) — the "Tools
  disabled by connection profile" warning box is DEAD CODE IN v4 ITSELF.** v4's
  `ChatModals.tsx:392` renders the box when any LLM participant's connection
  profile has `allowToolUse === false`, but `getConnectionProfile`
  (`lib/services/chat-enrichment.service.ts:354-379`) projects exactly
  `{id, name, provider, modelName, apiKey}` — it never carries `allowToolUse`.
  The condition therefore compares `undefined === false` and can never be true,
  so **no v4 user has ever seen this warning**, and a chat whose profile really
  does forbid tools looks, in the tool settings dialog, exactly like one that
  allows them.
  This closes a P4.9E3C escalation the other way round: v5's chat read does not
  project the field either, so v5 not rendering the box is **v4-faithful by
  outcome**, not a reduction, and no v5 server change is owed. v5 keeps the
  `profileToolsDisabled` input and the gated box, so one binding turns it on if
  v4 ever grows the projection.
  The upstream choice is v4's: either add `allowToolUse` to the enrichment
  projection (and the warning starts working) or delete the box. Same family as
  the unreachable `AllLLMPauseModal` — a control wired to something that cannot
  reach it. Deliberately NOT changed during the port: touching v4 moves the
  oracle baseline mid-flight.

- **⚠ v4-SIDE (post-5.0), added 2026-07-27 (the picker round's unification
  review) — the library picker lists a store's markdown documents, but
  attaching one 404s in BOTH apps.** A `.md`/`.txt`/`.json` PUT into a
  database store takes the native-text DOCUMENT branch
  (`lib/mount-index/store-file.ts:202` — `writeDatabaseDocument`, no
  `doc_mount_blobs` row), and `handleAttachMountFile` requires a blob
  (`files/route.ts:271-279` — `notFound('Mount-point file blob')`). So the
  picker's browse panel happily shows a store's markdown documents and every
  pick answers "Mount-point file blob not found" — in v4 and, faithfully, in
  v5. Found when the round's ACTIVATE-AT-UNIFY beat seeded a markdown file
  and hit the 404 live. The upstream choice is v4's: filter native-text
  documents out of the picker's store browse, or teach attach-mount-file to
  hand the Librarian a document (it has extractedText — the description
  ladder's first rung already reads it for photos). Deliberately NOT changed
  during the port.

- **⚠ v4-SIDE (post-5.0), added 2026-07-30 (P4.d27, the 5cc76688-round
  unification) — v4's boot dimension-reconcile mount-chunk count is DEAD
  CODE.** `reconcile-embedding-dimensions.ts`'s `countNonconformingMountChunks`
  opens with `tableExists(mainDb, 'doc_mount_points')` and its comment claims
  "mount point config lives in the main DB" — it does not (`doc_mount_points`
  is a mount-index table; v4's own repository log line and `fresh_schema.json`
  both say so), so on every real instance the guard is false and the count
  returns 0 before the mount-index handle is ever opened. v4's own unit test
  misses it because it creates the table in its *main* test DB. Established
  empirically: v4's REAL `reconcileEmbeddingDimensions()` reports
  `mountChunks: 0` over a corpus with non-conforming chunks on an ENABLED
  mount. **The fix is a one-liner** (read `doc_mount_points` from the
  mount-index handle). Consequence meanwhile: the reconcile never enqueues a
  reindex *for mount chunks alone* — they are not stranded, since the reindex
  handler's phase 4 reads mount points correctly and heals them whenever a
  reindex runs for any other reason. v5 reproduces the dead count faithfully
  behind a TRIPWIRE (`embedding_dimension_reconcile.rs`'s module-doc ⚠, a
  both-placements unit test, and the differential's `mountChunks == 0`
  assertion, which goes RED if anyone "fixes" v5 first). **v5 follows when v4
  moves — do not fix v5 unilaterally.**

- **A dogfood re-check after an SPA fix needs a HARD RELOAD, not a server
  restart (2026-07-27 — it cost a full round trip).** Finding #31's fix was
  reported as still broken after the human rebuilt the bundle *and* restarted the
  server. It wasn't: the served bundle was verified byte-identical to the repo
  dist and did contain the fix, the wire payload was correct, and the cast held
  the announcer. **The stale copy was the browser tab.** An Angular SPA keeps its
  loaded chunks in the tab for the life of that tab — restarting `quilltap-web`
  underneath it changes nothing, and content-hashed filenames don't help when the
  old JS is already in memory and never re-fetches. `Cmd+Shift+R`, and the render
  was correct on both the old announcement and a new one.
  **How to check this in one step next time, before diagnosing anything:** curl a
  chunk straight out of the running server and grep it for a string only the fix
  introduces —
  `curl -s http://127.0.0.1:3000/<chunk>.js | grep -c '<new string>'` — then
  `cmp` it against the file in `apps/web/dist/quilltap/browser/`. If the server is
  serving the fix, the remaining variable is the tab, and no amount of source
  reading will find the bug. **A render-only fix has a free test that needs no new
  data at all: hard-reload and look at the ALREADY-BROKEN row** — it should heal
  in place, since nothing about what was written changed.

- **`p4.9e3` is now UI-ONLY, and cheaper than its m6 row suggests (dogfood walk
  2026-07-27).** The 2026-07-27 walk went looking for four chat actions and found
  no UI for any of them — regenerate title, bulk reattribute, agent mode, merge
  conversations (finding #34). All four have v4 UI; none has v5 UI. **What
  changed since the m6 row was written is that P4.9E3A (2026-07-26) landed every
  server verb they need**, so the lane is now building dialogs over a live,
  differential-proven boundary rather than porting a service. Three specifics
  worth carrying into whoever scopes it:
  - **The agent-mode toggle is the cheapest item in the whole lane** — a badge in
    the Chat section over `ChatToggleAgentMode`, no dialog at all. It had been
    tracked in NEITHER of `m6-screen-parity.md`'s tables (it is not a modal); a
    row was added 2026-07-27.
  - **`regenerate-title` is unreachable without `ChatRenameModal`.** v4 has no
    button by that name: the route fires as a side effect of ticking "Use
    automatic naming" in the Rename dialog (`ChatRenameModal.tsx:52,184-192`).
    v5's `ChatRegenerateTitle` verb is live and has no way to be called from the
    browser until that dialog lands — which also means the ⚠ real-spend warning
    on that verb is currently unreachable, not merely unwarned.
  - **Bulk Replace needs a sidebar SECTION v5 has never had.** v4 puts it in
    **Edit Content** (`ChatSidebar.tsx:552,1615`), alongside Search & Replace,
    Re-extract Memories and Delete Chat Memories. v5's sidebar has four sections
    (Participants / Chat / Visibility / Organize) and no fifth, so the lane owes
    the section, not just the modal.
  **Only `Export` in the Organize section still lacks a server half** — every
  other deferral recorded at `organize-section.ts:17-21` and `chat-section.ts:71`
  was blaming a missing verb that has since landed. Those comments were corrected
  in place 2026-07-27; do not re-derive the lane's size from an older reading.

- **Post-5.0 product improvements (v4-first) — the running list of dogfood-surfaced
  UX papercuts that are v4-faithful today and therefore must change in v4 FIRST,
  then port.** These are NOT bugs (v5 reproduces v4 exactly) and NOT for the port
  itself (D22 — parity first); they are queued for after the 5.0 port lands.
  - **Whisper attribution shows "whispered to unknown" for a user-initiated
    private run** (finding #29). The whisper targets the operator's `userId`,
    which is deliberately not a participant id, so neither v4 nor v5 can resolve
    it to a name. The fix: when a `targetParticipantId` equals the operator's own
    userId, render "you" / "yourself" instead of falling through to "unknown".
    Touches v4 `MessageRow.tsx:323-324` + `participantNames` (SalonView), then the
    v5 mirror `message-row.ts:490` (`whisperTargets`). Requested by the human
    2026-07-24.
  - **A user-initiated tool card wears the last speaker's face** (finding #33).
    A pending tool result is persisted with no `participantId` (both sides), and
    both renderers then borrow the nearest preceding assistant's participant by
    POSITION — which, because the tool row is written before the user's message,
    is whichever character spoke last. The card's own text is already correct
    ("You ran rng"); only the avatar and the bold name above it are borrowed, so
    a roll the operator made is headed by an unrelated character. The fix:
    suppress the positional borrow when `initiatedBy === 'user'` and head the row
    with the operator instead. Touches v4 `VirtualizedMessageList.tsx:228-247`
    (the borrow) and `ToolMessage.tsx:428-443` (the name block), then the v5
    mirrors `chat-view-model.ts::resolveToolAvatar` + `tool-message.ts`. Note the
    two sides must move together: v5's borrow is a verbatim port, and changing it
    alone would put the Salon out of step with the oracle. Surfaced by the human
    2026-07-27.
  - **New-chat Play As revert doesn't restore a default profile** (finding #17) —
    if revert-should-restore-a-default is wanted, it's a v4-first change to
    `NewChatForm.handlePlayAsChange`.
  - **A side-effect counter cannot bootstrap itself** (finding #49). The most
    obvious use of the whole feature — "increment a counter when this outcome
    lands" — cannot be authored, because the incrementing expression references a
    key that only that same effect would create, and the eval-failure rule skips
    it forever. There is no way to write around it inside a definition: the
    expression grammar has no logical operators and no defaulting, and
    `EffectWhen` has no state subject, so neither `{{state.x \|\| 0}}` nor an
    "if absent" arm can be expressed. The operator must seed the key by hand in
    the State Editor first, which is exactly the manual step the feature exists
    to remove. Three candidate fixes, all v4-side: an effect-level `default` for
    unresolved refs; a `state` subject on `EffectWhen` with an existence
    comparator; or absent state refs resolving to `0` in expression context
    (narrowest, but changes an error sentence the corpus pins). Touches v4
    `lib/pascal/expressions.ts` (eval) and/or `lib/pascal/side-effects.ts` +
    the `EffectWhen` schema, then the v5 mirrors `pascal/expressions.rs` /
    `pascal/side_effects.rs` **and** the browser twin under
    `apps/web/src/app/pascal/`. Surfaced by the human 2026-08-02.
  - **A composer-initiated custom-tool run resolves as the arbitrary-first chat
    participant, not the operator's own played character** (finding #30). When
    the operator runs a global/shared tool from the composer Custom Tools button,
    it rolls against `sightings[0]`'s `metadata` — participant[0], which is
    USUALLY the operator's own character (so it usually looks right) but is an
    LLM character the operator isn't playing whenever the chat was created
    leading with that character (its `metadata.json` differs, so outcome branches
    flip — proven on "Chat with Friday" via the run's `pascalMeta.metadataTested`).
    The intuitive behavior: a user-initiated composer run should prefer
    the operator's own (controlledBy `user`) character participant as the
    `asCharacterId`. Touches v4 `CustomToolsDropdown.handleRun` /
    `route.ts` `buildListing` perspective selection, then the v5 mirrors
    (`custom-tools-popup.ts`, `api/custom_tools.rs` dedup). Requested-adjacent
    by the human 2026-07-24 (they expected their own character to be used).
    **✅ CLOSED on BOTH sides 2026-07-27 — the v4-faithful verdict is moot.**
    v4 fixed it in `e8a49597` (`found-bugs.md` Bug 5: `operatorCharacterIds` +
    `preferOperator` at the single-variant listing arm and the run's
    `asCharacterId`-less fallback, plus a `characterLabel` when none of the
    operator's characters is a candidate — an all-LLM room, or a gate their
    character did not pass). v5 mirrored it the same day as lane **P4.D24**
    (`api/custom_tools.rs`; `pascal_custom_tools_route_equivalence` 13 → 20
    cases over five new perspective rooms in the pascal fixture). No SPA change
    was needed on either side — the label already renders. ⚠ The v5 differential
    could NOT see the change until the fixture moved: its one chat seated the
    operator's character first in stored order, so old and new selection agreed
    on every row. **The 2026-07-24 walk's items 10/11, blocked on #30, unblock
    at the next dogfood pass** — and that pass should confirm the fix on real
    data (a chat created leading with an LLM character, a metadata-gated tool
    run from the composer, `pascalMeta.metadataTested` naming the operator's own
    sheet).
- **Carried out of finding #24 — the non-streaming response parsers have NO
  oracle differential.** `request_builder_equivalence` covers the request side
  (both modes, all eight providers, since P4.11); nothing covers
  `model/response_parse.rs` at all. Its five families
  (chat-completions × 4 flavors, responses-API, anthropic, google, ollama) are
  pinned only by hand-authored unit tests, and #24 is proof that a hand-authored
  fixture can encode the same wrong assumption the code makes. **Two of the five
  read SDK-normalized shapes** (responses-API `output_text`, google
  `response.text`) where the wire carries arrays — google reproduces the
  aggregation correctly (`:462`, reads `candidates[].content.parts`), the
  responses family did not. **CLOSED — P4.13 unit 4 (2026-07-23):** the committed
  `response-bodies.recorded.ndjson` corpus (29 cases, all nine families,
  recorded by running v4's real plugin `sendMessage` with fetch mocked
  UNDERNEATH the SDK) + the always-on `response_parse_equivalence`
  differential. Its first run caught and fixed TWO more #24-class bugs
  (OpenRouter usage parsed to zeros off the snake_case wire; google raw's
  `functionCalls` read a getter-only key). ⚠ Every corpus body is still
  **doc-derived (`synthetic: true`)** — the diff proves v4/v5 parse
  agreement, not wire shape; upgrading a family needs a real capture (the
  recorder header documents the path), and until then the committed `.wire`
  stream fixtures likewise stay untrusted as wire-shape evidence
  (`openai-basic`/`grok-basic`/`openai-reasoning` carry a top-level
  `output_text` no real response has).
- **FIXED (2026-07-23, work order P4.14 — arm (a), the non-validating stable
  merge sort) — the memory-injector sort comparator panics on real data, killing the turn.**
  `crates/quilltap-core/src/stable_sort.rs`'s `stable_sort_by_unchecked` now
  runs both injector comparators (and, from the same audit sweep, the Post
  Office's `sort_newest_first`, which has the identical defect via v4's
  `Number.isFinite` guard — reachable only on malformed `sentAt` frontmatter).
  The comparators are byte-unchanged; only the sort driving them moved. All
  five affected differential families re-ran byte-green over oracles
  regenerated fresh at `e646f58b`, so no committed slate reaches the
  contradictory region where this sort may diverge from V8. **#26's re-check is
  unblocked from the panic side** — a build_context panic can no longer kill
  `run_summary_check` before the fold gate is evaluated — but its cheap-LLM
  side still waits on the P4.13 rewrite; re-check #26 in the post-rewrite
  dogfood run. Two refinements to the diagnosis below, learned while landing:
  the trigger is **~20** memories, not 50 (driftsort's insertion-sort fast
  path), AND the slate must not already read as one detected run — an epsilon
  ladder fed in ladder order is skipped entirely, so only a shuffled slate (a
  real cosine-ordered recall slate) actually sorts. The original localization
  follows.
- **(the localization, 2026-07-23, server stderr backtrace) — the memory-injector sort comparator panics on real data, killing the turn.**
  `thread '<unnamed>' panicked … user-provided comparison function does not
  correctly implement a total order`, during a Salon send on the Friday copy;
  the character could not respond, and it did not reproduce on retry. **Not
  NaN.** The two epsilon-threshold comparators in `memory_injector.rs`
  (`:508` and `:865` — "if the weight gap exceeds 0.05 compare weights, else
  compare scores") are non-transitive on ordinary finite values: weights
  0.00/0.04/0.08 give a=b, b=c, a<c. A standalone repro panics reliably at
  **n ≥ 50** memories and never at n = 20 — fixture slates are small, real
  recall slates are not. v4 has the identical comparator; V8's TimSort does not
  validate, Rust's driftsort does. **The site is now CONFIRMED**: the backtrace names
  `quilltap_core::memory_injector::format_dynamic_memory_head` (the `:865`
  comparator) under `build_context` -> `process_message` -> `spine::run_send`.
  That comparator carries a SECOND intransitivity source beyond the epsilon
  rule: the `blended_after` branch (`:872-876`) returns early only when BOTH
  sides carry a value, so different pairs are ordered by different criteria.
  **This is very likely the cause of finding #26**: `format_dynamic_memory_head`
  runs inside `build_context`, which runs inside `process_message`, and
  `run_summary_check` is called at the END of `process_message`
  (`orchestrator.rs:2372`) -- a panic in build_context kills the turn before the
  fold gate is ever evaluated. Two panics were observed on the same evening the
  fold never fired. Check this link FIRST when picking up #26. The fix needs a ruling:
  a non-validating stable merge sort (identical to v4 wherever the comparator
  is self-consistent; may differ from V8 in the contradictory region) versus
  porting V8's TimSort (exactly faithful, several hundred lines).
  **ORDER WRITTEN: `work-orders/p4.14-memory-sort-total-order.md`**
  (2026-07-23) — **RULED same day (human): the non-validating stable merge
  sort.** The lane is cleared to dispatch.

- **Carried out of finding #24 — the non-streaming response parsers have NO
  oracle differential.** `request_builder_equivalence` covers the request side
  (both modes, all eight providers, since P4.11); nothing covers
  `model/response_parse.rs` at all. Its five families
  (chat-completions × 4 flavors, responses-API, anthropic, google, ollama) are
  pinned only by hand-authored unit tests, and #24 is proof that a hand-authored
  fixture can encode the same wrong assumption the code makes. **Two of the five
  read SDK-normalized shapes** (responses-API `output_text`, google
  `response.text`) where the wire carries arrays — google reproduces the
  aggregation correctly (`:462`, reads `candidates[].content.parts`), the
  responses family did not. **CLOSED — P4.13 unit 4 (2026-07-23):** the committed
  `response-bodies.recorded.ndjson` corpus (29 cases, all nine families,
  recorded by running v4's real plugin `sendMessage` with fetch mocked
  UNDERNEATH the SDK) + the always-on `response_parse_equivalence`
  differential. Its first run caught and fixed TWO more #24-class bugs
  (OpenRouter usage parsed to zeros off the snake_case wire; google raw's
  `functionCalls` read a getter-only key). ⚠ Every corpus body is still
  **doc-derived (`synthetic: true`)** — the diff proves v4/v5 parse
  agreement, not wire shape; upgrading a family needs a real capture (the
  recorder header documents the path), and until then the committed `.wire`
  stream fixtures likewise stay untrusted as wire-shape evidence
  (`openai-basic`/`grok-basic`/`openai-reasoning` carry a top-level
  `output_text` no real response has).
- **Carried out of finding #24 — v4 streaming bug, ported deliberately.** On
  every real OpenAI/Grok stream, v4's `buildRawResponse` reads
  `finalResponse.output_text` = `undefined`, so its `raw.choices[0].message`
  has no `content` key. v5 matches (emitting `null` where v4 omits the key — a
  pre-existing shape nit the synthetic fixtures hide). Anything that comes to
  depend on raw content for the streaming responses-API path needs this fixed
  **in v4 first**.

- **Carried out of finding #22 — `loadedMemories` DONE (P4.15), one field
  still unthreaded (loud here, not in code).** `turn_tool_context` now passes
  `project_id`, `image_profile_id`, AND `loadedMemories`:
  - **`loadedMemories`** — ✅ **DONE (P4.15).** The orchestrator converts
    `builtContext.{debugMemories,debugInterCharacterMemories,debugMemoryRecap}`
    into `LoadedMemoriesContext` (`loaded_memories_from_debug`, narrowing each
    semantic entry to the four keys the `self_inventory` builder consumes) and
    threads it into BOTH loops' tool contexts, so `self_inventory` reports the
    real memory slate the LLM saw (`available: true`) instead of the
    `Unavailable` arm. Proven by the existing `self_inventory` differential
    (`full_sections` asserts the populated section; `loaded_memories_absent`
    the Unavailable arm) plus orchestrator unit tests for the conversion +
    threading.
  - **`browserUserAgent`** — v5's request path carries no User-Agent at all;
    nothing to thread yet. Genuinely unported input, not a wiring slip. Still
    commented at `_image_profile_id` in `orchestrator.rs`.

- **✅ Also observed at the 2026-07-22 pass — FIXED (P4.17, SPA-only lane):
  v5 had no tool-result hide/show control.** Every tool result was whispered
  into the Salon as a `Private whisper` bubble carrying the raw JSON envelope
  (`{"toolName":…,"success":…,"arguments":…,"callId":…}`), where v4 has a
  proper show/hide affordance for tool output. **P4.17 ported v4's
  `ToolMessage.tsx` as `qt-tool-message`**: a `role:'TOOL'` message now renders
  as a collapsible Tool Request / Tool Response card (both default-collapsed,
  v4's `▶`/`▼` glyphs, 80-char previews) with a Success/Failed badge, tool-icon
  header, and attribution line, in both the embedded (character-initiated runs
  folded into the calling bubble via `groupToolMessagesIntoAssistants`) and
  standalone (user-initiated Prospero runs — a collapsed announcement chip that
  expands to the card) layouts. The other half of the wording complaint went
  with it: the message row's hardcoded `Private whisper` became v4's dynamic
  `whispered to <names>`. It was a rendering/affordance gap, not a dispatch one:
  the whisper-gate sets (`ALWAYS_PRIVATE_TOOLS` / `VAULT_READ_TOOLS` in
  `tool_execution.rs`) and the client whisper filter were already faithful and
  did not change. `generate_image` result thumbnails stay a loud deferral (v5
  renders chat images through the assistant bubble, P4.6ac). Verified by a
  19-case component spec, the ported grouping test (11 cases), 5 render-item
  cases, and a live Playwright walk over two seeded TOOL rows.

- **✅ FINDING #23 — FIXED (P4.11, unified on main 2026-07-23):
  `work-orders/p4.11-non-streaming-request-builders.md`** (single lane, nine
  units; the lane record is in `status-log.md`). Every builder honours
  `RequestInput.stream`; the corpus now records BOTH modes for all eight
  providers (34 → 93 lines, the streaming half byte-identical); the unit-9
  live proof on the Friday copy minted the first cheap-LLM rows v5 has ever
  completed (24 MEMORY_EXTRACTION + 1 TITLE_GENERATION in `llm_logs`, fresh
  AUTO memories with `occurredAt`), which is also P4.6bj's and the episodic
  campaign's owed live proof. Still open from the lane record: the extraction
  cadence is pinned by no differential; a failed cheap call still writes no
  `llm_logs` row (v4-faithful — the divergence candidate awaits a human
  ruling). **CORRECTION (P4.15): the "v5 never writes
  `chat_messages.debugMemoryLogs`" claim was STALE — no gap exists.** Both v5
  extraction handlers already write it, byte-matching v4:
  `services/memory_extraction_job.rs:338` and
  `services/carina_memory_extraction.rs:257` (v4
  `memory-extraction.ts:203-207` / `carina-memory-extraction.ts:164-169`;
  a JSON array of strings, full replace). The original finding text follows.
  The single highest-value item in the backlog: **no
  non-streaming LLM call in v5 has ever worked in production.** The order
  carries the full survey; the scoping notes that produced it follow.
  1. **Five builder sites** must honour `RequestInput.stream` instead of
     hard-coding `true`: `chat_completions.rs:105-106` (`base_body` — also
     `:311`, `:368`), `anthropic.rs:268`, `responses_api.rs:305,309,330`.
     Key ORDER must not move: v4's `sendMessage` keeps `stream` in the same
     slot and simply omits `stream_options`, so the Rust shape is
     `.set("stream", json!(input.stream))` followed by a conditional
     `stream_options` — that alone is byte-faithful for DeepSeek (verified by
     diffing v4's two bodies field for field; the rest — stop, tools,
     response_format, `user_id` from `cacheKey`, profile params, the
     thinking-strip — is shared).
  2. **v4's `sendMessage` must be read for EACH provider**, not assumed to
     differ only by that flag — DeepSeek was verified, ANTHROPIC / OPENAI
     (Responses) / GROK / GOOGLE / OPENAI_COMPATIBLE were not.
  3. **The differential owes a non-streaming corpus.**
     `request_builder_equivalence` intercepts only `streamMessage` (`:7`) and
     every vector sets `stream: true` (`:126`). The oracle must grow a
     `sendMessage`-intercepting leg with matching vectors per provider, or the
     same class of bug can recur silently. This is the real reason the lane is
     a lane.
  4. **Rider worth taking with it:** `CheapLlmTaskExecutor::log_call` writes
     `llm_logs` only on success, so a failed cheap-LLM call is invisible in the
     Inspector — which is why this read as "extraction never ran". Check
     whether v4 logs failures on that path (v4 `sendToProvider`) and match it;
     a failure row would have made this a five-minute diagnosis.
  Until it lands, treat every P4.6bj / episodic-campaign "live" claim as
  **unproven on real data** — the pipeline is wired correctly and dies at the
  provider call.
  Reproduction recipe (no server needed): a `quilltap-host` example
  constructing `WireCompletionProvider::new(DbProviderKeys(db), …)` against the
  Friday copy and calling `send_message` with the cheap params
  (`strict_max_tokens: true`, `max_tokens: 2048`, `temperature: 0.3`)
  reproduces it in one run; `build_request("DEEPSEEK", &input)` prints the
  offending body without any network call at all.

- **Resolved-by-#23: the 2026-07-22 memory-pipeline silence** (P4.6bj's owed
  live proof). Everything upstream of the provider call was verified healthy on
  the Friday copy: the job IS enqueued on turn close, the runner DOES execute
  it, both handlers ARE registered (`spine.rs:2578,2585`), and the
  finalizer's `&& settings.cheap_llm_settings_present` gate is v4-faithful (v4
  gates one frame deeper, `memory-trigger.service.ts:65`). Note the gate reads
  **global** settings, not per-chat: `chat_settings` is keyed by `userId` (v4
  `repos.chatSettings.findByUserId`), so `cheapLLMSettings` is the one global
  cheap-LLM config — the walk script's "a chat with a cheap-LLM profile"
  phrasing was wrong and is corrected here. **The diagnostic that cracked it:**
  the handler persists its own reasoning to `chat_messages.debugMemoryLogs` on
  the turn's last assistant message — read that column before instrumenting
  anything. It said, verbatim, `[Memory] SELF extraction failed for <name>:
  response parse: expected value at line 1 column 1` ×4.

- **Reading a running dogfood instance: the console now speaks (P4.18), and
  `quilltap db` reads the tables.** As of P4.18 (2026-07-24) `quilltap-web`,
  `quilltap-tauri`, and `quilltap-cli` init a `tracing` stderr subscriber, so
  the server is no longer silent after the banner: the job runner narrates its
  lifecycle (Dispatching / Job completed / Job failed), and the swallow sites
  that hid findings #23/#26 (the cheap-LLM failed-call, the spine error frames,
  the context-fold/pump paths) now emit structured events. Filter with
  `RUST_LOG` (default `info`; `RUST_LOG=debug` adds the `tower-http` per-request
  line; `RUST_LOG=quilltap::jobs=debug,quilltap::cheap_llm=debug` narrows to the
  job/cheap-LLM targets). Events go to **stderr** — the CLI's stdout stays clean
  piped table output. The `logs/` dir in a Friday copy is still **v4's** winston
  output, not v5's (v5's file-transport parity is the deferred tier-3 item).
  For the durable record, `quilltap db` still reads the tables directly:
  `./target/release/quilltap db --data-dir <instance> --json "<sql>"` takes
  arbitrary SQL, opens READ-ONLY, needs no passphrase on this instance, and is
  safe alongside the running server (`--llm-logs` / `--mount-points` switch
  partitions). The queue triage query is
  `SELECT type,status,attempts,lastError,createdAt,completedAt FROM
  background_jobs WHERE type='…' ORDER BY createdAt DESC`.

- **The 2026-07-22 pass, part 2 (P4.6bj ∥ P4.d13/14/15 ∥ P4.d10/be ∥ P4.6bg)
  — what walked CLEAN.** The **state cascade is fully walked and clean**: all
  four tiers round-tripped (chat via the sidebar opener, group via the group
  editor, project via the Prospero card, general via Settings→Chat — the
  general tier verified to survive a **server restart**, which is the one that
  exercises the mount-document write). Workbench mock-state and the
  cross-tier cascade precedence were **deferred by the human, not walked** —
  they are the remaining state-cascade gap. The **fs-documents surface walked
  clean** apart from finding #22: the general-scope "New blank document"
  round-tripped (created, saved to real disk, reopened, edited, saved again —
  the mtime-conflict path), the file was confirmed on disk, an external write
  was picked up on reopen, and a PNG uploaded to a store came back **WebP**
  (the P4.6bg unit-6 transcode wire, live on real data for the first time).
  NOT walked at all this pass: everything in the section-B episodic list
  below, plus the 💸 items.

- **Requested during the pass, already a named seam: automatic pickup of
  external file edits.** External writes are seen on *reopen* but nothing
  watches the filesystem — that is the chokidar-equivalent fs watcher (+ the
  db-store-event emitter chain), an existing loud refusing seam from P4.6y,
  listed in `m6-screen-parity.md` §5.3. The human's read ("maybe post-v5") is
  consistent with where it sits; no new order needed, but if it gets promoted
  it belongs with the store-event chain, not on its own.

- **The 2026-07-22 dogfood pass — coverage so far (walk paused here).**
  WALKED: the New-Chat picker + outfit seed (episodic round 1 — findings
  #16/#17), the ChatSidebar + Story's Clock (P4.9H1 — finding #19), the
  wardrobe dialog + tiers (P4.9f1/f2 — finding #18) + the avatar Preview
  (P4.6bf), the image-detail modal family (P4.9a2), and My Photos (P4.d8).
  The state-cascade surface was touched enough to yield #20/#21 but not
  walked to completion. **NOT yet walked — the next pass starts here:**
  episodic retrieval live behavior (retro questions + the
  retrospective-recall whisper + spam guard + Commonplace episodic columns
  + the 💸 `recall-replay` CLI), the rest of the state-cascade walk
  (Group/General State round-trips, workbench mock-state + $state proving),
  the 💸 llm-consult entrances (P4.6bd), the fs-documents surface (P4.6bg —
  general-scope "New blank document" round-trip, fs-mount on-disk edits,
  scriptorium WebP upload), KaTeX/single-dollar math (P4.d9/d11), the
  2026-07-15 leftovers (composition mode, drafts, composer attach +
  conflict, delete-with-associations, 💸 imageProfileGenerate), and the
  memory pipeline (P4.6bj — **still awaiting /unify onto main**; dormant
  until then). ~~Two re-checks owed on the rebuilt binaries~~ **BOTH VERIFIED live by the human (2026-07-22)** (both fixes
  landed mid-walk): #16 — regenerate an avatar in a project chat; the
  announcement should render its thumbnail and participant avatars display;
  #18 — wear "Paris 1925 Casual" and reopen the outfit editor; Friday
  lists 72 items and the outfit resolves all four components.

- **v4 drift observed at the 2026-07-22 dogfood pass** (drift-check before
  the finding-#16 oracle regen): v4 HEAD moved `deab0e5d` → `e646f58b`.
  `8d86847a` ("deep links that escaped the tabbed workspace now open as
  tabs") touches the PORTED `lib/workspace` + `lib/navigation/
  route-to-intent` (42 lines) — **a P4.d-style catch-up on the workspace
  core is owed** (the P4.9J1 captured-corpus tier-1 covers it; regen the
  corpus when porting). `e646f58b` is a lint chore (comment/identifier
  spelling flags; behavior-free). The finding-#16 families were verified
  untouched by the drift, so their oracles regenerated straight from the
  checkout.

- **The 2026-07-20 dogfood pass (the workspace-tabs remainder round) —
  CLEAN, zero findings.** VERIFIED on the Friday copy: the Brahma Console
  end-to-end in BOTH modes (floating dialog + workspace tab — live
  multi-turn sends with SQL tool calls over the real Friday schema,
  transcript persistence + reload, model-picker profile switch, new/delete
  conversation, salon-list exclusion), the standalone Document Mode surface
  (rail entry, picker, real store documents, the open→edit→save→edit→save
  round-trip that the mtime fix guards, the loud general-scope
  "New blank document" refusal), the Wardrobe tab + roster riders (asTab
  hosting, card Chat → `/salon/new`, header Start Chat, Create-Character
  in-tab), workspace chrome stress (multi-kind tabs, HTML5 drag-split,
  divider resize, reload persistence, cross-theme accent), AND the
  **Text Replacements** card + a rule firing in the composer (the
  migration-vintage `text_replacement_rules` table on real data — the
  2026-07-15 note's highest-value remaining check, now closed). One
  non-finding during setup: a stale v4 browser tab on port 3000 (v5's
  deliberate default) surfaced v4-client parse errors against the v5
  server — recorded here so it isn't re-reported; hard-reload resolves it.

- **The 2026-07-15 dogfood pass (the P4.6ae→am arc) — covered vs deferred.**
  VERIFIED on the Friday copy: composer on-type marks + send fidelity, the
  big-slab paste round-trip, the character-edit field round-trip
  (qt-markdown-field over real prose), chat backgrounds (#9 fix), the
  chained-response render (#7 fix), and the /files general listing (complete
  + correct). Findings #10/#11 recorded NOT-A-BUG (v4-faithful). **NOT yet
  walked — the next /dogfood pass starts here:** ~~the Text Replacements
  settings card + a rule firing in the composer~~ (walked CLEAN 2026-07-20,
  above), composition mode (toggle + per-chat persistence + the Settings
  default card), draft persistence, delete-with-associations on a linked
  file, the composer file-attach + duplicate-conflict flow, and
  imageProfileGenerate (real provider spend — ask first).

- Finding #3 made **virtualization + post-render scroll-to-bottom** the first
  deliverable of the next Salon slice — it blocked dogfooding long-running
  chats outright. (Closed: P4.6h — `p4.6h-salon-virtualization.md`.)
- If findings of class #1/#2 keep appearing, the systematic close-out is a
  **migration-vintage fixture**: a test DB built by replaying v4's actual
  migration chain (instead of fresh `generateDDL`) so the differential harness
  can exercise real-instance shapes. Write it as its own small order if a
  third schema-divergence finding lands.
- ~~**Finding #12 sets the next Tauri deliverable** (a P4.7 rider)~~
  **CLOSED** (P4.7c one-origin, unified 2026-07-16; human-confirmed
  2026-07-18): the window ships on `qtap://localhost/`, so server-supplied
  relative URLs resolve through the qtap handler into the reused router.
  **The combined M5 + finding-#12 human walk is COMPLETE (2026-07-18)** —
  Part A (M5 beats on the staged instance) surfaced findings #14 (Cmd+R)
  and #15 (unthemed gate screens), both fixed in place same-day; Part B
  (the Friday-copy image quartet) ran clean. One standing residue:
  Windows/Linux one-origin behavior (localStorage persistence,
  custom-scheme quirks) is macOS-verified only — re-run the spike checks
  when those targets first build.
- `db::tolerant_select_list` is the reusable fix for any further
  `no such column` hits — apply it to the failing reader + add the
  migration-vintage regression test (the `chat_settings` pattern).
- ~~**Audit the remaining `<select [value]>` + dynamic-options sites**~~
  **CLOSED** across P4.6l ("dogfood-#6 select audit" riders), P4.6aa, and
  P4.6am (the last dynamic-options site converted; the remaining
  static-options sites documented proven-safe). The standing rule for NEW
  selects stands: never bind `[value]` on a `<select>` whose options load
  async — bind `[selected]` per option.
- **The Scriptorium status badge + manual re-render has no v5 caller**
  (found 2026-07-27 while scripting the walk for the library-picker +
  embedding round, not by browsing). v4's `ChatCard.tsx:255-273` shows a
  three-state badge (`none`/`rendered`/`embedded`) whose click re-renders,
  on both the salon list and the character Conversations tab; v5's
  `chat-card.ts` renders nothing, though `scriptoriumStatus` already
  arrives on the wire and `chatRenderConversation` is live. Recorded as
  **m6 §1.2 + backlog row 17 (`p4.9o`, rider-sized)**. Note the
  sequencing: before P4.6BM landed the handler, this button would have
  minted dead jobs — so it was right to be missing, and is now merely
  missing.
- **A stale list is the failure mode of any "refusing seams" inventory.**
  The m6 §5.3 row was re-verified against the code on 2026-07-27 and five
  of its names had gone live (attach-mount-file,
  `memoryGenerateEmbeddings`/`RebuildIndex`, `chatQueueMemories`,
  `EMBEDDING_GENERATE`, the WebP codec) with two more never armed at all.
  Nothing fails when a seam goes live and its name stays listed, so it
  silently accumulates work that is already done. **Re-verify that row by
  grepping the refusal strings whenever a round un-refuses anything** —
  the sources are now cited per-seam in the row itself, which makes the
  check mechanical.
