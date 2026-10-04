# Dogfood walk — 2026-10-03 — the `f6426e196` recorded-divergences round + the `e5c6bd0c0` roster/structure round

**Rounds walked:** P4.139 ∥ P4.140 ∥ P4.141 ∥ P4.142 ∥ P4.143 ∥ P4.144 ∥
P4.145 (unified 2026-10-03) and P4.D245 ∥ P4.D246 ∥ P4.D247 ∥ P4.D248
(unified 2026-10-03). Agent-driven, on a COPY of Friday
(`~/qt-dogfood-friday`, rsynced 2026-10-03 ~17:05, before v4 ran its
standing-informs migration).

**Build:** main `a3fb2c1d6` (core 0.0.1199, host 0.0.179, web 0.0.216,
SPA 0.5.797), release build + `npm run build`.

**Server:**

```
RUST_BACKTRACE=1 RUST_LOG='info,quilltap::=debug,quilltap_core::tools=debug' \
  ./target/release/quilltap-web --data-dir ~/qt-dogfood-friday \
  --spa-dir apps/web/dist/quilltap/browser
```

The copy needs no passphrase (the server boots unlocked).

## §1 Drift state at walk start

The ledger's §2 probe FAILED at walk start: v4 main had moved one commit to
`52d6e7ecd` ("Inform: standing (per-chat) informs"), the in-flight dirt the
`e5c6bd0c0` unification recorded, now committed. `/driftcheck` ran first
(`b3c31274a`): **DRIFT PENDING — 1 commit, PORT, schema-moving** (a new
`chat_informs.permanent` column). **No row in this walk touches Inform**, and
the copy predates the migration (no `add-chat-informs-permanent-v1` ledger
row; 34 inform rows, 0 pending), so no step can blame the drift.

## §2 Pre-walk measurements

| # | measured | result | consequence |
|---|---|---|---|
| M1 | the boot's structural pass | DEBUG `Structural tables verified context="startup.verify-structural-tables" checked=11`; `/health` 200 with `structure: healthy` "All structural tables verified" | A1 PASS at boot |
| M2 | the projects' `allowAnyCharacter` + roster (`projectList`) | all seven projects CLOSED (`allowAnyCharacter: false`): Voyages of the Covenant (34 on roster, 182 chats), The Estate (35/425), Malory Wave (6/29), Foundry-9 (3/17), Church (6/42), LUC Ranch (2/3), Quilltap Plans (12/233) | the roster gate has real CLOSED projects; pick an off-roster character by query |
| M3 | the Lantern | Friday's chat settings had `storyBackgroundsSettings.enabled: true` | turned OFF on the copy (`chatSettingsUpdate`) before any test chat |
| M4 | boot WARNs on the fresh copy | two `Skipping corrupted chat message` WARNs (`createdAt: Invalid ISO datetime`, `participantId: Invalid UUID`) on chats `1762da60…` / `51660a07…` | the same v4-faithful data damage the 2026-10-02 walk recorded; not a finding |

## §3 The walk

Statuses: `PENDING` → `PASS` / `FAIL(#n)` / `DEFERRED-TO-HUMAN` / `BLOCKED(reason)`.

### A — Boot structure + health (P4.D248)

| # | owner | gesture | expected + how verified | status |
|---|---|---|---|---|
| A1 | CLAUDE | boot the fresh copy | `checked=11`, `structure` healthy (M1) | **PASS** (M1) |
| A2 | CLAUDE | server stopped: `ALTER TABLE doc_mount_chunks RENAME COLUMN headingContext TO headingContext_x` (mount index, `--write`); boot | boot continues; the ERROR pair; `/health` 503 `degraded` naming `table doc_mount_chunks is missing column headingContext`; the SPA still opens (dashboard renders); the Scriptorium lists files; rename back → healthy | **PASS** — planted together with F6's `embedding` rename (server stopped, `--mount-points --write`): the boot CONTINUED; ERROR `quilltap::boot: Structural table check failed; reads through this repository will come back empty … repository="docMountChunks" problem="table doc_mount_chunks is missing columns headingContext, embedding"` + ERROR `Structural tables damaged; /api/health will report degraded until repaired … checked=11 damaged=1`; `/health` **503** `{"status":"degraded", … "structure":{"status":"degraded","message":"1 damaged table; reads through it answer empty","problems":["table doc_mount_chunks is missing columns headingContext, embedding"]}}`; the SPA opened on the dashboard anyway; the Scriptorium listed its 76 stores and a vault's 14 files. Reverted → the next boot `checked=11`, `/health` 200 |
| A3 | CLAUDE | server stopped: the #134 plant (`doc_mount_file_links.relativePath → relativePath_x`); boot | the ensure-form structural problem in `/health`; boot continues; revert | **PASS** — boot continued: ERROR `quilltap::db: Failed to ensure doc_mount_file_links table in mount index database error=no such column: relativePath`, WARN `Error ensuring general state.json, continuing startup … error=no such column: l.relativePath`, the structural ERROR pair; `/health` 503 `degraded` with `problems: ["doc_mount_file_links in mount index database: no such column: relativePath"]`. The two-app cross-check (the same plant under v4 on a second copy) was not run — the identical string is the unification's §S.7 record |
| A4 | CLAUDE | the collapse gate (§5.5): ledger row + unkeyed avatar rolls | expect `AlreadyCompleted` — no plant if the ledger row exists and no unkeyed rolls | **PASS (no plant, per the measurement)** — `migrations_state` carries `collapse-duplicate-avatar-rolls-v1` (2026-09-11T17:45:57Z, 1785 items — v4's own collapse), so the gate is `AlreadyCompleted` and the trigger plant does not apply; no collapse line at any of this walk's eight boots |
| A5 | CLAUDE | Lock (dispatch) → `/health` → unlock | 423 while locked; healthy after unlock | **PASS** — `lock` → `unlockState needs-passphrase`; `/health` **423** `{"status":"locked","dbKeyState":"needs-passphrase",…}`; `unlock` (this copy has no user passphrase, `hasUserPassphrase: false`) → `resolved`; `/health` 200 with `structure` healthy again |

### B — The project roster (P4.D245 ∥ P4.D246 ∥ P4.D247)

| # | owner | gesture | expected + how verified | status |
|---|---|---|---|---|
| B1 | CLAUDE | open a real project's Characters card (click the card body from the projects list) | v4's populated roster renders — the first time on v5 (the phantom `roster` key read empty before P4.D247) | **PASS** — LUC Ranch (clicked the dashboard row body): "2 characters in roster", Friday + Charlie each "3 chats". **Found + FIXED #135 on the way**: the same page's Files card listed "4 files" as four nameless `0 B • DOCUMENT` rows (the card read `fileName`/`fileSizeBytes`, the wire carries `originalFilename`/`size`); after the fix the card reads `properties.json` 337 B, `description.md` 39 B, `instructions.md` 121 B, `state.json` 2 B, each with its type glyph |
| B2 | CLAUDE | toggle Allow Any Character both ways | both directions' strings, subtitle, explainer; the PUT answers enriched `characterRoster` objects + `_count` (network log) | **PASS** — ON: subtitle "Open to every character", "Every character may use the project files and shared wardrobe.", explainer "Any character in a project chat may read and edit its files and borrow from its wardrobe. Turn this off to choose who may."; OFF: "2 characters in roster", "Only roster characters may use the project files and shared wardrobe.", the roster cards back. The `projectUpdate` `{allowAnyCharacter:true}` response (fetch hooked in the page): `characterRoster` as enriched objects (`id`, `name`, `defaultImageId`, `defaultImage{filepath}`, `tags`, `chatCount: 3`) + `_count {chats:3, files:0, characters:2}` (`files: 0` beside a 4-file card is v4's own legacy-table count, `project-crud.ts:32`). **Found #136 on the way**: each write rewrote `properties.json` 598 → 337 bytes, dropping the eight explicit `null` keys v4 had written (see §4) |
| B3 | CLAUDE | the picker on real data | archived absent, quick-hidden absent, NPCs present; a title search; `Adding…`; toast `Character added to the roster` | **PASS** — LUC Ranch's picker listed 42 = the 44 non-archived characters minus the two on the roster; all ten archived characters absent (Tuman, an archived NPC, among them); both live NPCs present (Missouri Secretary of State Office, The Watcher). Quick-hide: the copy's one flagged tag "Secret" sat only on an archived character, so Trina was tagged "Secret" (`characterAddTag`) and the tag switched on through the user menu's QUICK HIDE TAGS section — after a refetch the picker listed 41 without her. "Mender" typed into the search left only Anjali (a TITLE match); her row read `Adding…` mid-request and the toast read `Character added to the roster`; the card went to "3 characters in roster" with "Anjali · 0 chats" |
| B4 | CLAUDE | remove; then the empty states | `Every character is already on the roster.`; `No characters match.`; `Done` clears the search | **PASS** (one string e2e-only) — `Remove Anjali from roster` → toast `Character removed from the roster`, Anjali back in the "Mender" results; with her re-added the stale "Mender" search read `No characters match.`; `Done` then `Add character` reopened with an empty search. `Every character is already on the roster.` needs all 41 candidates rostered — left to `projects-flow.spec.ts`'s Characters beat, which asserts it |
| B5 | CLAUDE | a CLOSED project, an off-roster character, a real Salon turn asking it to use `doc_list_files` / `doc_read_file scope:project` / `search_scriptorium` | no project store listed; the roster sentence verbatim on the project read; no project hits; `[ProjectRoster] Tool access check … allowed: false` in the log | **PASS** — "Chat with Laura" (`978ff334…`, LUC Ranch, Laura off-roster, Z.AI GLM 5.3 Flash), a real composer send (the composer sits in composition mode, so Send, not Enter): `doc_list_files {scope: project}` → `"No files found."`, `total: 0`; `doc_read_file {path: instructions.md, scope: project}` → `success: false`, *"Error: You are not on this project's character roster, so its files are closed to you. Ask the user to add you to the roster in the project's Characters card."* verbatim; the log: DEBUG `quilltap_core::project_roster_access: [ProjectRoster] Tool access check projectId=bf90dc1f… characterId=3b465ede… allowed=false` then INFO `quilltap_core::doc_edit::path_resolver: Project scope refused: character off project roster` (the DEBUG line needs `quilltap_core::project_roster_access=debug` in `RUST_LOG` — outside `quilltap::`). `search_scriptorium`: the model reported no tool by that name (v5's catalogue names it `search`) — not exercised |
| B6 | CLAUDE | add the same character through the card; repeat B5's ask | admitted with no restart; `allowed: true` | **PASS** — Laura added through the card's picker (toast `Character added to the roster`, "Laura · 2 chats"); the same ask in LUC Ranch's other Laura chat (`2053de26…`) → DEBUG `[ProjectRoster] Tool access check … allowed=true` at 23:47:26 and `doc_read_file` `success: true`, `File: instructions.md (1 lines, 121 bytes)` with its one line — no restart |
| B7 | CLAUDE | create a new project from the SPA, toggle untouched | opens with Allow Any Character ON ("Open to every character") | **PASS** — Projects list → Create Project (the dialog offers only Name + Description) → "DOGFOOD roster default" → the detail's Characters card reads "Open to every character" with the switch ON |
| B8 | CLAUDE | on a roster-locked project: a new chat with an off-roster character | the roster's `properties.json` bytes UNCHANGED (md5 before/after) — the auto-add is gone | **PASS** — LUC Ranch (Friday, Charlie, Anjali on the roster): `properties.json` md5 `562bfff4…` before; the project's New Chat with Laura (off-roster) created "Chat with Laura" `978ff334…` in the project; md5 `562bfff4…` after, roster unchanged, no `Character added to project` line. (A first attempt was VOID — instrument error: the workspace keeps hidden tabs mounted, and the script's `main`-scoped "Laura" match hit the Projects tab's still-open roster picker, which added her through `projectCharacterAdd` at 23:38:43; removed and redone scoped to `qt-new-chat-page`.) |
| B9 | CLAUDE | Help search "roster" | lands on the rewritten page | **PASS** — the sidebar Help → "Search topics..." → `roster` lists Getting Started, Characters, Chats, Projects (Overview / Chats / Files / **Project Characters** / Settings), Files, Appearance, Settings & System; Project Characters opens on the rewritten steps ("Make sure Allow Any Character is off · Click Add character · Search for the character by name, then click them to add them · Click Done when the guest list is complete") |
| B10 | CLAUDE | the Salon outfit dialog: equip a project-tier garment for the off-roster character | the operator bypass equips it | **PASS** — Laura taken off LUC Ranch's roster (Friday, Charlie, Anjali remain); a project-tier garment "Ranch work gloves" (`0021db72…`, accessories) created; in "Empty Ranch, Locked Door" the participant card's "Open Laura’s wardrobe" → the dialog's "Live outfit · Laura in this chat" listed the gloves under Show shared ("· shared accessories"); Wear staged it, Done flushed it (`flushStagedLiveOutfits` → `chatEquip set_all`): `chatOutfitGet` then carried `accessories: ["15458839…", "0021db72…"]` and `WARDROBE_OUTFIT_ANNOUNCEMENT` ran. (The sidebar's "Wardrobe" entry is the global dressing room, not this dialog.) |

### C — The API-key read class (P4.139); server stopped for each plant

| # | owner | gesture | expected + how verified | status |
|---|---|---|---|---|
| C1 | CLAUDE | create a throwaway key; stop; `UPDATE api_keys SET key_value = x'00000000'` on it; boot; Settings → API Keys | the list renders every OTHER key (no 500); one `WARN quilltap::db API key validation failed keyId=… userId=…` | **PASS** — a throwaway key "JUNK dogfood key" (OPENAI, `63b1e2f9…`) created, then (server stopped) `key_value = x'00000000'`: `apiKeyList` 200 with the other 11 keys; WARN `quilltap::db: API key validation failed keyId=63b1e2f9… userId=ffffffff-… error=Invalid column type Blob at index: 4, name: key_value` |
| C2 | CLAUDE | edit / delete / test on the corrupted key | 404 each; `ERROR quilltap::db Error finding API key by ID collection=connection_profiles keyId=… error=Invalid column type Blob at index: 4, name: key_value` | **PASS** — `apiKeyUpdate` / `apiKeyTest` / `apiKeyDelete` each 404 `API key not found`; update + delete log `Error finding API key by ID collection="connection_profiles" keyId=63b1e2f9… error=Invalid column type Blob at index: 4, name: key_value`, the test the SCOPED `Error finding API key by ID and user ID … userId=ffffffff-…` |
| C3 | CLAUDE | an image profile bound to the corrupted key | lists with `apiKey: null`, no 500 | **PASS** — "JUNK image profile" (bound to the corrupted key) in `imageProfileList` 200 (10 profiles) with `apiKeyId: 63b1e2f9…`, `apiKey: null` |
| C4 | CLAUDE | `isActive = NULL` on a search key | web search still finds the key | **PASS** — Serper's key `isActive = NULL` (server stopped); a real turn in "Chat with Laura" `2053de26…` asking for a web search: `search_web` `success: true`, *"Found 5 search results"*, first "Lives Under Construction Boys Ranch – changing the way ..." (lucboys.org) |
| C5 | CLAUDE | a Scenario Builder run on a profile whose `name` is a BLOB | 404 + `Error finding entity by ID collection=connection_profiles` | **PASS** — "JUNK blobname profile" (`ffe3248d…`) with `name = x'4a554e4b'`: `scenarioBuilderBuild` 404 `Connection profile not found`; ERROR `quilltap::db: Error finding entity by ID collection="connection_profiles" id=ffe3248d… error=Invalid column type Blob at index: 2, name: name`. Side effect, recorded as a note (§4): the C4 turn's `TITLE_UPDATE` job FAILED on the same plant (`Job failed … error=sqlite error: Invalid column type Blob at index: 2, name: name`) |
| C6 | CLAUDE | the wizard with a keyless vision profile behind a keyed primary (posed desk) | the vision call sends the PRIMARY's key (refusal-server `QT_REFUSE_KEY` gate) | **BLOCKED (setup cost; pinned at the helper)** — the arm needs a keyed OPENAI primary on the posed Responses endpoint, an image MIME that primary refuses (so the wizard reaches for the vision profile), a keyless vision profile and the key gate restarted; the regression it guards (the unification's blocking P4.139 finding) is pinned at `profile_api_key_found_scoped` (every not-found arm `None`), and the caller-level pin is OPEN by name in P4.139's Unification paragraph. Left for a dedicated posed pass |

### D — The Salon spine (P4.140) + the attachment send (P4.145)

| # | owner | gesture | expected + how verified | status |
|---|---|---|---|---|
| D1 | CLAUDE | restart under `TZ='XST6XDT,M3.2.0,M11.1.0'` | the boot WARN's new wording; then `list_mail` / tool dates / the Scenario Builder clock in CDT (`-05:00`), the Host's `Current time:` + progressions in UTC (the recorded split) | **PASS (the recorded split, as predicted)** — booted with `TZ='XST6XDT,M3.2.0,M11.1.0'`: WARN `quilltap_web: refusing a timezone that is not an IANA zone name … a POSIX rule there (e.g. CST6CDT,M3.2.0,M11.1.0) still sets the displayed dates, but scheduled rooms, daily budget rollover and same-day recall run on UTC … value=XST6XDT,M3.2.0,M11.1.0 source=TZ` (item 13's wording). Under it: the markdown export's headings read CDT ("Laura — October 3, 2026 at 6:40 PM" for 23:40Z); a real turn's `read_conversation` read "Created: October 3, 2026 at 6:40 PM … Current time: October 3, 2026 at 7:04 PM" (CDT); the Host's line in the same turn read **UTC** — "The Host marks the time as October 4, 2026 at 12:04 AM" — P4.140's recorded Tier 3 item. `list_mail` answered *"Your postbox stands empty."* (no date to read) |
| D2 | CLAUDE | an attachment-only send (text file, empty editor) | request's user turn = the inlined file + `Please look at the attached file(s).`; one user row with the file; `[Attached: <name>]` bubble hands off | **PASS** — `dogfood-note.txt` attached through the visible composer's file input, Send with an empty editor: the bubble read `[Attached: dogfood-note.txt]`, then the persisted row "Please look at the attached file(s)."; the row's `attachments: ["7dadb8a5…"]`, the file linked to the chat; the request's user turn: `[Charlie] [User attached text file: dogfood-note.txt]` + the body + `[End of attached file]` + `Please look at the attached file(s).` |
| D3 | CLAUDE | an image on a NON-vision seat (posed describer) | the `fileProcessing` frame on the event stream with `"type":"image_description"`; the SPA shows nothing new | **PASS (the native arm; `image_description` not exercised)** — a 64×64 PNG + a question on Laura's seat, an `EventSource('/api/events')` open in the page: `status processing_files` → ONE `{"chatId":"978ff334…","fileProcessing":[{"filename":"dogfood-square.png","type":"unsupported","usedImageDescriptionLLM":false}]}` → `status validating`; the SPA rendered nothing for it. `unsupported` WITHOUT an error is v4's native-send arm (`context-builder.service.ts:217-226`) — Z.AI GLM 5.3 Flash takes images (Laura answered "a red background with a white square"), so the seat was not the non-vision one the order wanted. P4.120's background describe ran at upload anyway (`[Image Fallback] Successfully generated description … description_length=1817`, two seconds after the turn was sent) |
| D4 | CLAUDE | an `@Carina` consult | one `CHAT_MESSAGE` `llm_logs` row, `characterId` = the answerer, NULL `messageId` | **PASS** — `@Friday: in one sentence, what is a well pump pressure tank for?` in Laura's chat (the markup needs the colon; a bare `@Friday …` is an ordinary turn): `llm_logs` `CHAT_MESSAGE` at 00:11:39 with `characterId = d9d0d998…` (Friday, the answerer) and `messageId` NULL, then `CARINA_MEMORY_EXTRACTION` for her; Laura's own turn rows keep their `messageId`. The `ask_carina` tool arm not run (same funnel) |

### E — The model layer (P4.141), posed desks

| # | owner | gesture | expected + how verified | status |
|---|---|---|---|---|
| E1 | CLAUDE | cheap desk on `hang` (a title) | `OpenAICompatible API error in sendMessage … error=Request timed out.`; trigger `network`; the background cheap task RETRIES once (`is_timeout_failure` by kind) | **PASS** — the cheap desk on "POSED hang"; a real turn in `2053de26…` enqueued `memory-keyword-extraction` (a background cheap task, `budget_ms=90000`): ERROR `OpenAICompatible API error in sendMessage context="OpenAICompatibleProvider.sendMessage" baseUrl=http://127.0.0.1:8898/v1 error=Request timed out.` (v4's thrown text) → WARN `[CheapLLM] Attempt timed out; retrying the same route once … budget_ms=90000` → the second timeout → `[CheapLLM] Task failed` → the fallback chain consulted (`chain=[]`, nothing to fail over to, so no `[Failover]` line to show `network`). The cheap task's own `error=` carries reqwest's text — the 2026-07-23 ruling. Cheap desk restored afterwards |
| E2 | CLAUDE | cheap desk on `empty-choices` | `error=Cannot read properties of undefined (reading 'message')`; the chain steps on | **PASS (no chain to step to)** — the cheap LLM pointed at "POSED empty-choices" (OPENAI_COMPATIBLE, `http://127.0.0.1:8898/v1`), `chatRegenerateTitle` on `978ff334…`: ERROR `quilltap::model::completion_provider: OpenAICompatible API error in sendMessage context="OpenAICompatibleProvider.sendMessage" baseUrl=http://127.0.0.1:8898/v1 error=Cannot read properties of undefined (reading 'message')`, the cheap task's ERROR + WARN `[CheapLLM] Task failed`, then the fallback chain consulted (`Fallback chain built … purpose=cheap … chain=[]` — no stand-in enabled on this profile) and v4's `[Chats v1] Title generation failed` → 500 with v4's text. No blank title written |
| E3 | CLAUDE | a Salon turn on OLLAMA at `http://127.0.0.1:1` | `Ollama streamMessage failed … error=fetch failed`; `[Failover]` trigger `network` | **PASS** — "DEAD ollama" (OLLAMA, `http://127.0.0.1:1`), chat "DOGFOOD dead ollama" `a91c9742…`: the greeting ladder's three attempts each logged ERROR `Ollama streamMessage failed context="OllamaProvider.streamMessage" baseUrl=http://127.0.0.1:1 error=fetch failed` and fell back to the static greeting; a Salon turn → the same catch line, WARN `[Failover] Primary call failed; walking the fallback chain … trigger="network" purpose=chat`, the route-trail failure recorded with `trigger="network"`, `Fallback chain exhausted` (no stand-in) → 500. The ladder's WARNs and the Failover line's `error=` carry reqwest's `error sending request for url (…)` where Node would say `fetch failed` — the 2026-07-23 provider-I/O ruling (transport error text recorded, not compared), not a finding |
| E4 | CLAUDE | a Google turn on a junk key | `Error streaming from Google Gemini API context=GoogleProvider.streamMessage model=… error=…` | **PASS** — "JUNK gemini" (GOOGLE, `gemini-2.5-flash`, a junk key), chat "DOGFOOD junk gemini": ERROR `quilltap::model::streaming_provider: Error streaming from Google Gemini API context="GoogleProvider.streamMessage" model=gemini-2.5-flash error={"error":{"code":400,"message":"API key not valid. Please pass a valid API key.","status":"INVALID_ARGUMENT",…}}` on the greeting attempts and the turn |
| E5 | CLAUDE | an OpenRouter image send on a junk key | the plugin's ERROR line #7 alone | **PASS** — line #7 is OpenRouter's non-streaming send WITH image attachments; driven free through the describer: `imageDescriptionProfileId` → "JUNK openrouter vision" (OPENROUTER, a junk key), a PNG uploaded in the Salon (P4.120's auto-describe): ERROR `quilltap::model::completion_provider: OpenRouter API error context="OpenRouterProvider.sendViaChatCompletions" status=401 error={"error":{"message":"User not found.","code":401}}` and NO second catch line; then `[Image Fallback] Error generating description` and the retry on the uncensored stand-in. The describer setting restored afterwards |

### F — Repository fallbacks + strict wraps (P4.142); server stopped for each plant

| # | owner | gesture | expected + how verified | status |
|---|---|---|---|---|
| F1 | CLAUDE | plant `doc_mount_file_links.relativePath → relativePath_x`; `listChats` | 200, vaulted participants unresolved; 9 + 2 batch-read ERRORs, `Dropping character from list — vault unavailable` per character, the summary WARN, the project drop pair | **PASS** — `listChats` **200**; the lines, counted by message: 13 × `Error finding documents by mount point IDs and path` (the character overlay's 9 + the project overlay's 4, each naming its `relativePath`), 2 × `… and folder` (`Prompts`, `Scenarios`), 54 × `Dropping character from list — vault unavailable` + WARN `applyDocumentStoreOverlay dropped characters with unavailable vaults dropped=54 total=54`, 7 × `Dropping project from list — document store unavailable` + WARN `applyProjectStoreOverlay dropped projects with unavailable stores dropped=7 of=7` |
| F2 | CLAUDE | same plant: `chatGet` | 500 `Failed to fetch chat` + `[Chats v1] Error fetching chat chatId=…` | **PASS** — `{"kind":"internal","message":"Failed to fetch chat"}` HTTP 500; ERROR `quilltap_core::api::salon: [Chats v1] Error fetching chat chatId=978ff334… error=applyDocumentStoreOverlayOne: vault unavailable for character 3b465ede…` |
| F3 | CLAUDE | same plant: `send_mail` | `No soul by that name keeps a postbox here.`, no catch line | **BLOCKED (unreachable live on this data)** — every character on the copy is vault-backed, so under this plant no Salon turn reaches a tool: `chatSend` answered 500 `participant resolution failed: Db(StoreUnavailable { … vault unavailable for character 3b465ede… })` and ERROR `quilltap::chat: Chat send failed while streaming the initial turn`. The `send_mail` arm stays family-proven (P4.142) |
| F4 | CLAUDE | same plant: a backup and a `.qtap` export | both FAIL (the strict wraps — no silent drop of vaulted characters) | **PASS (+ #137)** — `systemBackupCreate` 500 `Failed to create backup`; `POST /api/v1/system/tools?action=export {type: characters, scope: all}` 500 `{"error":"Failed to create export"}` — neither wrote a vault-less bundle. **#137**: the backup's line reads ERROR `quilltap_core::api::system_backup: createBackup failed error=sqlite error: no such column: l.relativePath` where v4 logs `[System Backup v1] Error creating backup` with the bare message, and the export logged NOTHING where v4 logs `[System Tools v1] Export failed {userId}` |
| F5 | CLAUDE | revert; the Scriptorium search box | the chunk search logs nothing | **PASS** — reverted and rebooted (`/health` 200); the global search box (⌘K) for "boys ranch": DEBUG `Global message search plan … path="fts" tokens=2 chatCount=1042` and `Document text search completed … stores=66 archived_vaults_excluded=10 … documents=1`; no ERROR, no WARN |
| F6 | CLAUDE | plant `doc_mount_chunks.embedding` renamed; mount-points list | 200, zero embedded counts, `Error counting embedded chunks by mount point IDs` | **PASS** — (planted with A2) `mountPointList` 200, 76 stores, every `embeddedChunkCount` 0 (file and chunk counts intact); ERROR `quilltap::db: Error counting embedded chunks by mount point IDs collection="doc_mount_chunks" mountPointIdCount=76 error=no such column: embedding`; the store detail read "0/9 chunks embedded" |

### G — Data/zod smalls (P4.143)

| # | owner | gesture | expected + how verified | status |
|---|---|---|---|---|
| G1 | CLAUDE | a `.qtap` with one tag `visualStyle: "x"` | the import names it with v5's serde sentence and carries on | **PASS** — a `.qtap` (the tags export's envelope + a bad tag + a good tag) through `POST /api/v1/system/tools?action=import-execute` (multipart, `skip`): `success: true`, `imported.tags: 1`, `warnings: ["Failed to import tag \"DOGFOOD bad style\": invalid type: string \"x\", expected struct TagVisualStyle"]` (the recorded serde divergence) and WARN `quilltap_import::entities: Failed to import tag tagId=0d0f…b1 error=…`; the good tag imported |
| G2 | CLAUDE | `duplicate` re-import of an image profile with `provider: 42`; an embedding profile `provider: "BOGUS"` | the first NAMED; the second refused with v4's `invalid_value` bytes | **PASS** — the "Imagen 4" image profile re-imported under `duplicate` with `provider: 42` → `warnings: ["Failed to import image profile \"Imagen 4\": invalid type: integer \`42\`, expected a string"]` (named; it vanished before P4.143); an embedding profile with `provider: "BOGUS"` → refused with v4's `invalid_value` Zod bytes (`values: OPENAI, OLLAMA, OPENROUTER, NANOGPT, BUILTIN`, `path: ["provider"]`, *"Invalid option: expected one of …"*) |
| G3 | CLAUDE | a backup ZIP with `scenarioText: 5` restored | `Failed to restore chat "…": invalid type: integer \`5\`, expected a string` + THREE `quilltap::db` ERRORs | **BLOCKED (cost)** — the arm needs a backup ZIP of the copy (the full Friday instance) edited and RESTORED, which rewrites the copy under the rest of the walk; the derived-archive arm is family-proven (P4.143 item 14) |
| G4 | CLAUDE | `routeTrail` planted `[{"via":"sideways",…}]`; then a non-uuid `profileId` | skipped with `errorsJson=[": Invalid input"]`; then `["routeTrail.0.profileId: Invalid UUID"]` | **PASS** — two Laura replies in `978ff334…` planted (`[{"via":"sideways"}]` on `bb10465b…`; a whole attempt with `profileId: "not-a-uuid"` on `1bb5238f…`): `chatGet` 200 with 16 messages, neither planted id present; WARN `Skipping corrupted chat message … messageId="bb10465b…" … errorsJson=[": Invalid input"]` and `… messageId="1bb5238f…" … errorsJson=["routeTrail.0.profileId: Invalid UUID"]`; the file log carries them as `"errors": [...]` |

### H — The memory smalls (P4.144)

| # | owner | gesture | expected + how verified | status |
|---|---|---|---|---|
| H1 | CLAUDE | a real fold whose episode pass writes | one `[FoldEpisodePass] Episode pass complete … memoriesWritten=…` equal to the episodic rows added | **PASS** — Rebuild Summary (`chatRebuildSummary`) on "Pulled Pork and Produce Aisles" (`a886dbb6…`, 60 messages) on the real cheap desk: INFO `quilltap_core::services::fold_episode_pass: [FoldEpisodePass] Episode pass complete chatId=a886dbb6… episodesExtracted=1 memoriesWritten=2 fragmentsLinked=13`; the chat's `kind='episodic'` memories 1 → 3 (+2 = `memoriesWritten`). Two non-firing arms on the way, both v4-faithful: on the test chat the episode call ran and answered `[]`, so no INFO (it fires only on a non-empty extraction); a 24-message chat answered `[ContextSummary] Summary update did not run … error="Not enough turns to fold"` |
| H2 | CLAUDE | a fold on a junk-key cheap profile | one `WARN … Episode extraction failed`, no episodic rows | **BLOCKED (not posable with one desk)** — the fold's summary call and its episode call both ride the cheap desk, so pointing it at "POSED empty-choices" failed the SUMMARY first (`Cheap-LLM call failed task_type="fold-chat-summary" … error=Cannot read properties of undefined (reading 'message')`, `[ContextSummary] Summary update did not run`) and the episode pass was never reached. The WARN arm stays family-proven (P4.144) |

### What NOT to expect to work

- **Standing informs** (v4 `52d6e7ecd`): not ported; the Inform dialog has no
  "Keep it standing" checkbox. Drift, not a defect.
- `SCENE_STATE_TRACKING` jobs from v4's queue fail with "handler is not yet
  available in the native runner" — a named phase-4 deferral.
- WaveSpeed image desks (ruled never-supported, #124).
- REST `POST /api/v1/models` and `?action=test-message` answer 405 — the
  settings family is dispatch-only.

## §4 Findings and non-findings

**Findings (in `dogfood-findings.md`):**

- **#135 — FIXED in place (SPA).** Every project's Files card listed its
  files nameless at `0 B`: the card (P4.6l, 2026-07-11) read `fileName` /
  `fileSizeBytes`, the wire (v4's and v5's) carries `originalFilename` /
  `size`; its thumbnails read two more absent keys, and v4's `+N more files`
  line was missing. The unit spec's fixture had frozen the wrong shape.
- **#136 — OPEN, a proposed order.** A project's `properties.json` loses
  every explicit `null` v4 wrote, on the first v5 write (598 → 337 bytes on
  LUC Ranch), and the read wire omits those keys — the `Option<T>` +
  `skip_serializing_if` seam the struct's own doc records.
- **#137 — FIXED in place (core).** A failed backup logged v5's own
  `createBackup failed` with a `sqlite error:` prefix; a failed `.qtap`
  export logged nothing. Both now log v4's line with the bare message.

**Non-findings (recorded so nobody re-files them):**

- `Skipping corrupted chat message` WARNs on two real chats at every boot —
  the same v4-faithful data damage the 2026-10-02 walk recorded.
- `Group state.json unparseable; defaulting to {}` on one group store at each
  turn — v4's own WARN over a real damaged file.
- `_count.files: 0` beside a 4-file store-backed Files card — v4 counts the
  legacy `files` table (`project-crud.ts:32`).
- The greeting ladder's WARNs and the `[Failover]` line's `error=` carry
  reqwest's `error sending request for url (…)` where Node says `fetch failed`
  — the 2026-07-23 provider-I/O ruling.
- A bare `@Friday …` is an ordinary turn — Carina's markup needs the colon
  (`@Friday: …`), v4's parser.
- The Salon composer on this copy sits in COMPOSITION mode (Enter adds a
  newline; Send submits) — the user's saved preference, not a defect.
- Under C5's plant the real turn's `TITLE_UPDATE` job FAILED on the
  BLOB-named profile — a plant artifact, noted for the next `/setupphase`
  (a list read in the fallback class), not filed.

**Instrument notes:**

- The workspace keeps every open tab MOUNTED. A script that scopes to
  `main` matches hidden tabs too — the first B8 attempt clicked the hidden
  Projects tab's still-open roster picker and added Laura to the roster.
  Scope DOM scripts to the active panel's component (`qt-new-chat-page`,
  `qt-project-characters-card`).
- The pane's screenshot frame (800×777) is a scaled copy of a 1180×1147
  viewport; `ref` clicks land at the unscaled CSS coordinates and miss.
  Click by screenshot coordinates, or drive the element directly.
- The `[ProjectRoster] Tool access check` DEBUG line is under
  `quilltap_core::project_roster_access`, outside the standing
  `quilltap::=debug` filter — add it to `RUST_LOG` for a roster walk.

## §5 Copy state left behind (disposable; the next rsync resets it)

The Lantern OFF; the cheap LLM and the image describer restored to Friday's
own profiles. Every plant reversed (the two `doc_mount_chunks` renames, the
links rename, the Serper `isActive`, the BLOB profile name, both `routeTrail`
plants); the throwaway key and both JUNK profiles from C1–C5 deleted. Left:
keys `POSED key`, `JUNK Google`, `JUNK OpenRouter`; profiles `POSED hang`,
`POSED empty-choices`, `DEAD ollama`, `JUNK gemini`, `JUNK openrouter
vision`; Trina tagged "Secret" (and the quick-hide switched on in the pane's
localStorage); LUC Ranch's roster now Friday, Charlie, Anjali (Laura removed
again) with a project garment "Ranch work gloves" equipped on Laura in
`978ff334…`; project "DOGFOOD roster default"; chats "Chat with Laura" ×2
(one retitled "Empty Ranch, Locked Door"), "DOGFOOD dead ollama", "DOGFOOD
junk gemini"; the imported tag "DOGFOOD good tag"; two summary rebuilds
(+2 episodic memories on "Pulled Pork and Produce Aisles").
