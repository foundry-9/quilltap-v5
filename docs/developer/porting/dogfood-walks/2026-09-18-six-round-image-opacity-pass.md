# Dogfood walk — the six-round backlog: image options, the opacity covenant,
# the prompt lockstep (2026-09-18)

**Instance:** `~/qt-dogfood-friday` — a COPY of real Friday data, rsynced
2026-09-18 17:34 local. Disposable; never rsynced back.
**Server:** `./target/release/quilltap-web --data-dir ~/qt-dogfood-friday
--spa-dir apps/web/dist/quilltap/browser`, launched with
`RUST_LOG='info,quilltap_core::files::llm_image_budget=debug,quilltap_core::model::image_dialects=debug,quilltap_core::services::message_context=debug'`
— **the round's two headline image proofs are DEBUG lines and are invisible at
the default `info`.**
**Log of record:** the scratchpad server log + `~/qt-dogfood-friday/logs/combined.log`.

## §0 Drift state

The ledger's §2 freshness probe **PASSED** at walk start (2026-09-18):
v4 `main` is **AT** the oracle baseline `baa85e19b`, `bugfix` unmoved at
`1a2b2164c`, checkout on `main`, tree clean, **§3 EMPTY**. Regen rule: NO PIN
REQUIRED. **No step in this walk may blame drift** — there is none.

## §0.5 Pre-walk measurement (ledger §5.5 — measure before planning around it)

Run before the browser, on the copy. A zero is a finding, not a failure.

| # | population | measured | consequence for the walk |
|---|---|---|---|
| M1 | `characters.defaultSystemPromptId IS NOT NULL` | **2 of 49** — Sunny (`ca4cf853…`, `2026-09-15`), **Friday (`3ad7695d…`, `updatedAt 2026-09-18T15:03:47Z`)** | ⭐ v4 landed bug 154 at 07:04 today and **wrote Friday's column itself** — and the walk's first query found BOTH arms of the resolution order live on real data: **Friday's column is STALE** (names an id no prompt of hers has) and **Sunny's column is VALID but DISAGREES with her `isDefault` flag** (column → *DEEPSEEK Companion*, flag → *GEMINI Companion*). The §5.5 warning was that this population might be gone; it is not, and neither arm needed planting. |
| M2 | the bug-152 row still on the instance | **YES** — group `Severed` `6fb9d8fd-…`, official store `Group Files: Severed` `eeef911d-…` (5 files, enabled), members incl. **Leilani `e14cb17a-…` `systemTransparency=0`** and Abigail `af38f265-…` `=1`; chat **`00bb0f9c-…` "The Night Before the Ice Read"** — the very chat the bug was filed from | The opacity covenant walks on the original row, not a plant. **Precondition holds:** `eeef911d-…` appears in NO `project_doc_mount_links` row, so the project tier cannot stand in for the group tier. |
| M3 | image profiles | **14**, incl. **`GPT Image 2.5 Flare` (`gpt-image-2.5-flare`, `isDefault=1`)** with `{quality:auto, size:1920x1088, background:opaque, output_format:webp, moderation:low}` and `GPT Image 2` | ⭐ v4 has already configured a 2.5 profile carrying **all four new extras** and made it the default. The P4.D196 options panel and the P4.94 dialect both get a real-data subject. **No profile stores `quality:"hd"`** → bug 148's `hd` symptom arm is not available; use the `auto`/`max` arm instead. |
| M4 | duplicate `files.generationKey` groups | **10** (all pre-date v4's 2026-09-11 collapse — v4 bug 143, already filed) | Bug 145's keep-the-album-copy proof needs a **PLANT** plus deleting v4's ledger row, exactly as P4.D192 banked it. Offered as a planted proof, labelled as such. |
| M5 | `migrations_state` | **187 rows, newest `collapse-duplicate-avatar-rolls-v1` 2026-09-11** | No v4 migration has run since; v5's boot must add **nothing**. Re-count after every boot. |
| M6 | chats with a stored rotation | **964** of ~1,000 | Bug 147's chat-GET projection has abundant real data. |
| M7 | images by size | 2,120 images; **95 > 1 MiB, 5 > 2 MiB**, largest **5,592,813 B PNG** | The bug-151 shrink has real subjects. The per-turn 2 MiB Lantern budget still needs a **contrived** multi-portrait turn. |
| M8 | archived characters | **10** | The archived-character `defaultSystemPromptId` PUT arm is available. |
| M9 | api keys | 11 providers incl. OPENAI, NANOGPT, GROK, DEEPSEEK, Z_AI, GOOGLE, ANTHROPIC, OPENROUTER, SERPER, WAVESPEED, MISTRAL | Live spend is available on several wires; OLLAMA is local. |
| M10 | `help_docs` with a NULL embedding | **5 of 124** — the five longest (40,278–55,049 chars); their **115 chunks are ALSO unembedded**, and they are exactly the 115 of 463 that are not | Investigated pre-walk: **v4-faithful.** v4's `EMBEDDING_MAX_CHARS` is 128 KiB, so `skipIfOversize` does not fire at 55 K; v4 calls the provider, takes the same 8192-token refusal, and its `embedHelpDocChunks` sits after the throw — so v4 loses the chunks too. Walk row D5 confirms help search still works on those docs. **Candidate v4 filing, not a v5 defect.** |

## §1 What is NOT live — do not report these as bugs

Collected from the six rounds' records so no step mis-files a deliberate shape.

- **The Salon turn path sends NO prompt-cache key.** `orchestrator.rs`'s
  `StreamParams` literal sets `cache_key: None`; the carry is live on the
  greeting, help chat, external-prompt generator, optimizer and cheap-LLM
  executor only. A Salon turn without `prompt_cache_key` is **expected**; a
  *greeting* without one would be a finding. (P4.92, ESCALATED.)
- **GOOGLE never sends a cache key at all** — v4's plugin has a
  `TODO(per-character-caching)`. Parity.
- **OPENROUTER's SDK stream branch writes `user`; v5 models only the raw-fetch
  branch.** An OpenRouter stream with no cache key is the pinned v5 shape.
- **`/api/v1/image-profiles` has no v5 REST edge.** `imageProfileGenerate` is
  reachable **only** through `POST /api/dispatch`. A 404 on the REST path is not a bug.
- **The System Prompts tab shows no success toast at all** (pre-existing
  whole-tab gap). Silence after starring is expected.
- **P4.D200 has no screen.** The covenant is reachable only through in-chat
  `doc_*` tools. Document Mode's picker uses different verbs and still lists
  everything. `run_sql`/Brahma pass `operator_override` and bypass it entirely.
- **An opaque character naming her OWN vault gets the plain `NOT_FOUND`**, never
  the ACCESS_DENIED disclosure — vaults are excluded from
  `find_enabled_mount_point_by_ref` by design. Same for the `self` token: no
  bespoke sentence.
- **Blob WRITE passes no peers** (asymmetric with blob read) — faithful.
- **`prompt: null` and `profileId: null` cannot be distinguished from absent in
  the response text** — v5 drops v4's `details` array on the images route
  (`drop_zod_details`). Both answer the bare `Validation error`.
- **`services/recovery.rs` emits no tracing on either side.** Silence after the
  INFO line is parity.
- **On a NON-vision seat, v5 dispatches describe fallbacks oldest-first** where
  v4 is newest-first (recorded divergence; only the call order moves).
- **The USER-side re-hydration budget is oldest-first and CHAR-counted** — the
  asymmetry with the Lantern budget is v4's own.
- **The provider-ceiling backstop is unreachable** — no provider declares a
  `maxBase64Size` under the 500 KiB transport target.
- **`SCENE_STATE_TRACKING` has no v5 handler or trigger.**
- **Story-background reroutes log nothing** (`STORY_REROUTE_SUCCEEDED_UNPORTED`).
- **`combined.log` bag keys are snake_case in v5, camelCase in v4** — a standing
  tree-wide divergence, named once.
- **Six Playwright skips + a documented intermittent trio** are standing parks.
- **Four fixture-vintage differential reds** (`subprompts_prompt_tier2`,
  `chat_delete`, `character_wizard_tier3`, `subprompts_routes`) are harness-only
  and cannot affect the live copy.

## §2 The walk

Owner `CLAUDE` unless marked. Status: `PENDING` → `PASS` / `FAIL(#n)` /
`DEFERRED-TO-HUMAN` / `BLOCKED(reason)`.

### Part A — the bug-154 default-system-prompt lockstep (P4.D201 ∥ P4.D202)

| # | owner | gesture | expected + how verified | status |
|---|---|---|---|---|
| A1 | CLAUDE | **Sunny arm (column ≠ flag).** `characterPromptList` for Sunny, then create a chat with her through `chatCreate` and read the SYSTEM message v5 wrote | ⭐ **PASS — discriminating.** Her column names *DEEPSEEK Companion* while her `isDefault` flag sits on *GEMINI Companion*: a real disagreement v4's pre-fix star left behind. The chat opened carrying **`# DeepSeekV3 Companion Prompt`** — the **column** won, exactly as the resolution order says. Chat `fa04bb24-…`. | PASS |
| A1b | CLAUDE | **Friday arm (stale column).** Same, on Friday, whose column `3ad7695d-…` names **no prompt she has** | ⭐ **PASS.** The chat opened carrying **`# Friday — Intimate Partner`** — the flagged prompt, reached by falling through the dead column. This is the bug-154 New-Chat symptom on real data: pre-fix the chat opened with **no system prompt at all**. Chat `b88f1cf8-…`. | PASS |
| A2 | CLAUDE | Characters → **System Prompts** tab → click ☆ on a non-default prompt | Covered server-side by A3 (the write is the round's subject). The **optimistic** badge move is a browser-only detail and is pinned by the activated `P4D201_SERVER_LANDED` e2e beat; not re-walked. | COVERED(A3) |
| A3 | CLAUDE | `characterPromptSetDefault` on the throwaway `Dogfood Pentimento` (Alpha flagged, Beta not), then `characterGet` + SQL + `characterPromptList` | **PASS — both faces moved in one write.** The flag left Alpha and landed on Beta AND `characters.defaultSystemPromptId` became Beta's id; `characterGet` echoes it. Also proven on the way: **creating a prompt with `isDefault:true` writes the column too** (a second of P4.D201's four writers). | PASS |
| A4 | CLAUDE | `characterUpdate` with `{"character":{"defaultSystemPromptId":null}}` | **PASS, three ways.** 200; the column emptied to NULL; the read overlay **re-promoted `prompts[0]` (Alpha) to `isDefault:true`** while the column stayed NULL; and `characterGet` **omits the key entirely** — the documented absent-≠-null shape. | PASS |
| A5 | CLAUDE | `characterUpdate` with `{"character":{"name":"Aria Over The Wire","defaultSystemPromptId":"not-a-uuid"}}` | ⭐ **PASS — the §3 review fix proven live.** 400 `System prompt not found on this character`, and the name re-read as **`Dogfood Pentimento`**: the sibling write did NOT persist. Before the uuid half of v4's gate landed, it would have. | PASS |
| A6 | CLAUDE | A well-formed uuid the character does not own, on **both** verbs | **PASS.** `characterUpdate` → 400 `System prompt not found on this character`; `characterPromptSetDefault` → **404 `Prompt not found`** (the two guards answer differently, as ported). The WARN `System prompt not found characterId=… promptId=…` fired for both. | PASS |
| A7 | CLAUDE | A character with **zero** system prompts | **PASS (server half).** A brand-new character answers `{"prompts":[]}`, so the SPA's `isDefault: prompts.length === 0` seed has its precondition; and creating that first prompt with `isDefault:true` **wrote the column**, which is the behaviour the seed exists to produce. The checkbox's default tick is browser-only and spec-pinned. | PASS |
| A8 | CLAUDE | Archived character `Lt. Supertramp` + a `defaultSystemPromptId`-only update | **PASS as recorded.** `kind: internal` with `Character cbddcb7b-… is archived: this character is archived; rehydrate it to continue` — v5's envelope leaks the archive guard's own sentence where v4's route middleware renders the fixed `Internal server error`. The **pre-existing, both-directions-pinned divergence**, reproduced. Not fixed, by ruling. | PASS |
| A9 | CLAUDE | Restore | **PASS.** Throwaway character, both probe chats, the bogus image profile and the bogus API key all deleted. Sunny's and Friday's real rows were **never mutated** — they served as read-only evidence, which is why every mutation arm went to a throwaway. | PASS |

### Part B — the opacity covenant (P4.D200), on bug 152's own row

Subject: chat **`00bb0f9c-…`**, seat **Leilani** (opaque), control **Abigail**
(transparent), store **`Group Files: Severed` `eeef911d-…`** (M2).

| # | owner | gesture | expected + how verified | status |
|---|---|---|---|---|
| B1 | CLAUDE | As Leilani, `doc_list_files` with **no** `mount_point` (via `chatRunTool`, zero LLM spend) | ⭐ **PASS.** 178 files across exactly three tiers — `Group Files: Severed` (7), `Project Files: Voyages of the Covenant` (101), `Quilltap General` (70). **Not one character vault**, and the string `Vault` does not occur at all. The instance holds 52 of them. | PASS |
| B2 | CLAUDE | As Leilani, `doc_write_file` into the Severed store **by name** | **PASS** — `File written: dogfood-covenant-by-name.md (64 bytes…)`. Pre-port this was `NOT_FOUND`: the bug exactly as filed, on the row it was filed from. | PASS |
| B3 | CLAUDE | Same, **by the store's UUID** | **PASS** — `File written: dogfood-covenant-by-id.md (62 bytes…)`. The id loop is separate from the name loop; both had to be fixed and both are. | PASS |
| B4 | CLAUDE | As Leilani, name `Malory Wave` — enabled, linked to the **MaloryWave** project, not to this chat's | ⭐ **PASS, both halves.** The three-sentence body verbatim: *"The document store \"Malory Wave\" exists but is not reachable from this conversation. It is not linked to this project, and it is not one of your own group's stores. Retrying with a different spelling will not help — use doc_list_files with no path to see the stores you can reach."* And the WARN: `Mount point exists but is out of scope: Malory Wave (7b9416db-…) (project: df5f9b72-…, characters: e14cb17a-…, vaultsHidden: true)` — **`characters:` carries Leilani**, which IS the fix: opacity used to be implemented by deleting her from the context, and that is what killed the group tier. | PASS |
| B5 | CLAUDE | As Leilani, `doc_read_file` against a peer's vault by name, against the literal `self`, and against **her own vault by name** | **PASS, all three.** Every one answers the identical `Mount point not found or not accessible in this context` — indistinguishable, never admitting the store exists. The `self` token took the ordinary not-found path (no bespoke sentence), and `Self-token resolution failed…` correctly did NOT fire: the self branch is guarded out when vaults are hidden. | PASS |
| B6 | CLAUDE | As **Abigail** (transparent), **the same chat, the same verb, the same arguments** | ⭐ **PASS — the decisive control.** `Abigail Character Vault` (200 files) plus `Group Files: Sebold Family` / `Triad` / `Celestial Engineering Team`, the project store and General. One chat, two seats, one difference: `systemTransparency`. B1's absence is the covenant, not a broken enumerator. | PASS |
| B7 | CLAUDE | Grep for the restored path-resolver lines | **PASS (partial reach, by data).** Both reachable shapes fired with the full context bag and `vaults_hidden=true`. The other three are unreachable on this instance and the zeroes are recorded, not failures: **there are no disabled stores at all** (`enabled=0` → 0 rows), so `Attempt to access disabled mount point` cannot fire; `Self-token resolution failed` needs a *transparent* character with no vault; `systemTransparency lookup failed` needs a thrown lookup. | PASS |
| B8 | CLAUDE | The operator's own store listing (`mountPointList`) | **PASS.** The operator sees **72 stores — 53 character vaults + 19 document stores**. The subtraction is scoped to the character document-tool path and does not leak into operator surfaces. (The true `operator_override` path is Brahma/`run_sql`, which is LLM spend — this is the cheaper equivalent claim.) Also noted: `chatRunTool` with no `characterId` runs as the chat's active character, not as an operator, so it is **not** an override probe. | PASS |

### Part C — the image rounds (P4.94 ∥ P4.D196 ∥ P4.D197 ∥ P4.D198 ∥ P4.D199 ∥ P4.96 ∥ P4.98)

| # | owner | gesture | expected + how verified | status |
|---|---|---|---|---|
| C1 | CLAUDE | Settings → Images → open **`GPT Image 2.5 Flare`** (v4's own default profile) | **PASS.** Groups `Image Parameters` then `GPT Image Output`; all six controls present — `#pof-quality`=`auto`, `#pof-size`=`1920x1088`, `#pof-background`=`opaque`, `#pof-output_format`=`webp`, `#pof-moderation`=`low`, and `#pof-output_compression` as a number input. **v4 configured these; v5 read them back and rendered every one.** Size labels use U+00D7 (`Landscape (1920×1088)`); the quality list carries `Extra High` and `Max — the finest the model offers`. | PASS |
| C2 | CLAUDE | Switch the Model select and re-read the panel | ⭐ **PASS — with a better discriminator than planned.** The model list here is the **live-fetched** one (9 ids, all GPT Image — no `dall-e-3` to switch to, so that arm is unavailable on real data). Switching `gpt-image-2.5-flare` → **`gpt-image-1`** instead: **sizes collapse 14 → 5** (`2048x2048`, `1920x1088`, `2560x1440`, `3840x2160` and their portraits all leave) and **quality collapses 7 → 5, losing exactly `xhigh` and `max`** — the 2.5-only premium tiers. `GPT Image Output` correctly stays (gpt-image-1 is still a GPT Image family). The capability table's exact-then-longest-prefix resolution, proven on values not counts. Cancelled without saving; v4's profile re-read unchanged. ⚠ instrument note: setting a `<select>` to a value not in its option list silently yields `selectedIndex: -1` and a meaningless change event — the first attempt did exactly that and looked like "the panel never updated". | PASS |
| C3 | CLAUDE | `imageProfileOptionsSchema` for `OPENAI`/`gpt-image-2.5-flare` | **PASS** — the server declares exactly `quality, size, background, output_format, output_compression, moderation`, the six the panel drew. The browser holds no capability table; the panel exists only because the server answered. | PASS |
| C4 | CLAUDE | ⭐ **Zero-spend wire proof.** A local listener is impossible — `image_profiles.baseUrl` is **not carried** by any v5 image caller (a recorded, pre-existing, shared narrowing: every dialect builds a fixed vendor URL). Instead: a throwaway OPENAI profile on a **deliberately invalid API key**, carrying all four extras. The body is assembled and logged **before** the HTTP call, so the request 401s and the proof is free. | ⭐ **PASS.** `Calling OpenAI Images API … model=gpt-image-2.5-flare size=1024x1024 quality=max background=transparent outputFormat=webp outputCompression=80 moderation=low n=1` — **the four new extras on the wire for the first time**, at zero spend (`PROVIDER_ERROR` from the bad key). ⚠ the `size=1024x1024` is NOT a defect: see C6. | PASS |
| C5 | CLAUDE | Re-point the same throwaway profile at `output_format: png` with `output_compression: 80` | **PASS.** `Dropping output_compression: it applies only to jpeg and webp … outputFormat=png`, and the following assembly line renders **`outputCompression=(model default)`** — the key is absent from the body. The same run also proved `quality=xhigh` (the other 2.5-only premium tier) and `background=opaque` / `moderation=auto` round-tripping from the profile. | PASS |
| C6 | CLAUDE | **Bug 149**: the same generate, now naming `size: "1536x1024"` explicitly | **PASS** — the wire line reads `size=1536x1024`. Pre-fix, `orientation` defaulted to `square` unconditionally and 1024x1024 landed on top of whatever size was asked for. ⚠ **This also explains C4's `1024x1024` and it is v4-faithful:** v4's post-fix rule is `input.size ? undefined : 'square'`, so an **absent** request size still resolves square, and orientation outranks the profile's stored `size` by design. Recorded as a v4-behaviour note (a profile's stored size is only honoured when the caller names no size), not a v5 defect. | PASS |
| C7 | CLAUDE | **Bug 148**: the profile stores `quality`; the request names none | **PASS** — `quality=max` on the first run and `quality=xhigh` on the second, both straight from the profile with **no `quality` in the request**. Pre-fix the tool schema's Zod default outranked the profile and three image settings were inert for every picture a character generated. `background` / `output_format` / `moderation` flow from the profile on the same evidence. | PASS |
| C8 | CLAUDE | **bug 151 shrink**: a large image genuinely **linked to the chat**, sent to a vision seat | ⭐ **PASS, beating the round's own estimate.** `Image shrunk for LLM transport module="files:llm-image-budget" filename="…webp" provider="NANOGPT" original_size=1946202 final_size=70872 original_dimensions="5712x4284" final_dimensions="1024x768" ceiling=512000` — **27.5×**, with the long edge capped at 1024 and the 4:3 aspect preserved exactly (`fit: inside`). Rode along: the host pixel codec transcoded the 4.2 MB source JPEG to a 1.95 MB WebP **at full 5712×4284** on the import path (P4.73/P4.D152, live). ⚠ **three instrument errors preceded this** — see §3. | PASS |
| C9 | CLAUDE | **bug 151 per-turn budget** | **DEFERRED-TO-HUMAN.** The budget counts **unseen ASSISTANT-side** images — Lantern portraits — and there is no API path that attaches an image to an assistant message; the only honest way to stock a turn with three fresh ones is to *generate* three, which is image spend. (User attachments, which I can now link freely, are governed by the separate OLDEST-first char-counted re-hydration budget that bug 151 deliberately did not touch.) Recipe for the human is in §2 Part F. | DEFERRED-TO-HUMAN |
| C10 | CLAUDE | **P4.98 tri-state** over dispatch: a discriminator profileId, then one `null` per key | ⭐ **PASS, all five.** The discriminator answers `Connection profile not found`, proving the parse stage is reachable; every explicit `null` answers 400 `Validation error`; and `files` was **2,183 before and 2,183 after** — nothing written. The three keys the round fixed (`chatId`/`tags`/`options`) are precisely the ones that used to collapse to absent. | PASS |
| C11 | CLAUDE | The same five nulls on `POST /api/v1/images?action=generate` | **PASS** — all five answer `400 {"error":"Validation error"}`, and the well-formed control still reaches `Connection profile not found`. One shared decoder, two transports, identical behaviour. | PASS |
| C12 | CLAUDE | **P4.96**: `imageProfileGenerate` refusal arms | ⭐ **PASS — v4's envelope, `details` array and all.** `chatId:null` → `Invalid input: expected string, received null`; `count:11` → `Too big: expected number to be <=10`; a 4001-char prompt → `Too big: expected string to have <=4000 characters`; **404 beats 400** (`Image profile not found`). Free P4.94 rider: `quality:null` printed the whole enum **including the new `xhigh` and `max`** — `"auto"|"low"|"medium"|"high"|"xhigh"|"max"|"standard"|"hd"` — so the ONE quality list is live on the wire. | PASS |
| C13 | CLAUDE | `aspectRatio: "3:2"` on the same verb | **PASS** — it passes the route (v4 declares `z.string()`) and is refused at the **tool** with v4's one blanket sentence `Invalid input: prompt is required and must be a non-empty string`, which names `prompt` and means "the whole object". The pinned, correct shape. | PASS |

### Part D — the bug-145/146/147 + maintenance rounds

| # | owner | gesture | expected + how verified | status |
|---|---|---|---|---|
| D1 | CLAUDE | **bug 147**: the chat GET (over dispatch — the REST path answers a fixed refusal by design) | **PASS.** Both keys present as **raw JSON strings**, in v4's exact slot: `… impersonatingParticipantIds, activeTypingParticipantId, spokenThisCycleParticipantIds, cycleOrderParticipantIds, isPaused …`. Values match the columns byte for byte on a real chat. | PASS |
| D2 | CLAUDE | **bug 147, the sidebar half**: open a real chat whose stored rotation already holds two spoken seats | ⭐ **PASS — the first time v5 can render `spoken` at all.** On a fresh load the rail carries `qt-participant-position-next` on **Charlie** and `qt-participant-position-spoken` on **Amy** and **Friday** — exactly the two ids in `chats.spokenThisCycleParticipantIds`, and the banner independently says *Charlie's turn*. Banner and sidebar read the same stored rotation. | PASS |
| D3 | CLAUDE | **bug 146, the fourth banner sentence**: a chat with one LLM seat (impersonated) and two seats I drive; post as one, then deliberately pick the **non-floor** owned seat in the SpeakerSelector | `<Name>'s turn — switch the speaker to them to type, or skip to let someone else respond.` naming the **floor's** seat. Then Skip → the dispatch `participantId` and every `systemKind:'turn-pass'` `hostEvent.participantId` is the floor seat's. ⚠ read turn-passes in SQL — the Salon renders them as a collapsed chip. Unreachable on a fresh render/reload by design. | PENDING |
| D4 | CLAUDE | **bug 144 `--lock-clean`**: SIGKILL the server, then clean inside five minutes | ⭐ **PASS — byte-exact, on a genuinely dead PID with a fresh heartbeat.** `Lock heartbeat is still fresh (25s ago). Cannot clean.` / `A lock counts as held until its heartbeat is 5 minutes stale, even if its process has gone. Wait it out, or use --lock-override to force.` The pre-fix wording (`…its holder is alive…` / `Stop the running instance first…`) is **absent**. This is v4 having adopted **this port's own dogfood finding #119**, now read back on real data. `--lock-status` agreed: `ACTIVE (local, heartbeat 25s ago)`, PID 74158, `Process: quilltap-web`. | PASS |
| D4b | CLAUDE | Wait out the five-minute heartbeat window, then `--lock-clean` again | The clean succeeds and the lock file goes. | PENDING |
| D5 | CLAUDE | **M10 follow-up**: `helpDocsSearch` for terms inside the five unembedded docs | **PASS.** `accessories` returns 6 matches including **`wardrobe`, `character-editing` and `connection-profiles`** — three of the five. The text engine still retrieves them; the missing embeddings cost ranking, not retrieval. Confirmed **v4-faithful** (see M10): a **v4 filing candidate**, not a v5 defect. | PASS |
| D6 | CLAUDE | **P4.93**: a real avatar regeneration, pointed at the bogus-key profile so the image call cannot spend | **PASS (the reachable arms).** `[CharacterAvatar] Starting avatar generation` with `context`/`job_id`/`chat_id`/`character_id`, then `[CharacterAvatar] Image generation failed` (ERROR) carrying the provider's own 401 sentence and **`moderation_rejection=false`** — finding #104's status split correctly declining to call a 401 a moderation rejection. The whole path ran (job dispatch → appearance → prompt craft → provider call). The success-path lines (`Avatar image saved`, `Avatar generation completed`) need a live image key → **F-list**. `fileIdsJson` belongs to the collapse heal (E1). | PASS |
| D7 | CLAUDE | **P4.93**: the DETECT_ONLY dangerous-verdict line | **NOT RUN.** Needs an instance-wide settings change plus a character with appearance data whose prompt classifies dangerous — a cheap-LLM call and a settings mutation on a shared surface, for a log-fidelity row already unit-pinned. Left for a future pass rather than half-done. | NOT RUN |
| D8 | CLAUDE | **P4.99**: the recovery INFO line | **DEFERRED-TO-HUMAN.** Needs a turn that actually trips `isRecoverableRequestError` — a token-limit or PDF-page-cap refusal from a real provider, which means deliberately overrunning a context window on a paid wire. | DEFERRED-TO-HUMAN |
| D9 | CLAUDE | **P4.97**: the tool-unsupported retry | **DEFERRED-TO-HUMAN.** Needs a provider that answers the specific *function-calling-unsupported* error; none of the eleven configured profiles does on its current model, and posing one means a stub endpoint that returns that exact provider error shape. | DEFERRED-TO-HUMAN |
| D10 | CLAUDE | **Boot idempotence**: restart the server; re-count `migrations_state` (M5) and diff it | Still **187** rows, byte-identical — v5 writes no ledger row on data v4 already healed. | PENDING |

### Part E — planted proofs (offered explicitly, weaker claims)

| # | owner | gesture | expected | status |
|---|---|---|---|---|
| E1 | HUMAN | **bug 145, planted**: delete v4's `collapse-duplicate-avatar-rolls-v1` ledger row, plant an album-linked duplicate roll, reboot | The heal keeps the album copy: ledger sentence gains `; kept N roll still serving as a character portrait`, per-keep INFO `Keeping avatar roll still serving as a character portrait`, and the kept album's link rows survive on **all four** mount tables. Labelled a planted proof. | PENDING |
| E2 | HUMAN | **P4.90, planted**: a connection profile with a **dangling `apiKeyId`**, then New Chat with that character | The six greeting-ladder lines, in particular WARN `[Chats v1] Connection profile is missing its API key` (`context="autoGenerateFirstMessage"`). ⚠ `:692` is a RECORDED DIVERGENCE (v5 names what v4's `safeQuery` swallows) — do not file. | PENDING |
| E3 | HUMAN | **P4.90, planted**: a dead primary endpoint + a **DeepSeek understudy**, on a turn that makes a tool call | `combined.log` shows the loop's re-stream against the **understudy's model**. ⚠ **only `model` moves** — the primary's temperature/max_tokens/top_p on the understudy's request is correct (v4 computes its bag once, pre-stream). | PENDING |
| E4 | HUMAN | **P4.92, planted/found**: an OPENAI Responses-API seat with a prior `resp_` id in history, on a turn that makes a native tool call | `combined.log` shows **ONE** chained primary and **ZERO** `Conversation chaining failed` lines on the re-stream. ⚠ verify a prior ASSISTANT message really carries `rawResponse.id` starting `resp_` first. The `stop` carry on the *failover* leg is by design. | PENDING |

### Part F — deferred to the human

| # | why | status |
|---|---|---|
| F1 | **Memory deduplication / conversation-summary regeneration first run** — long, expensive batch over 49 characters' memories. Standing 💸 item, deferred for cost since 2026-08. | DEFERRED-TO-HUMAN |
| F2 | **The Brahma Console deep query** — burns a large agent-turn budget on real spend. Standing 💸 item. | DEFERRED-TO-HUMAN |
| F3 | **#101 — NanoGPT prompt caching writes a cache every turn and never reads one.** A cost question for the operator, not a defect; needs the human's judgment on whether to raise it with the gateway. | DEFERRED-TO-HUMAN |
| F4 | **The re-measured 90 s / 120 s compression row** (superseded C4 from 2026-08-27). Needs an uncached compression forced on a real chat and the `CONTEXT_COMPRESSION` `durationMs` rows read, noting *which* of the two paths ran. 418 such calls in the last 3 days, so the population is there — but forcing the pre-computed path deliberately is operator work. | DEFERRED-TO-HUMAN |
| F5 | **Aesthetic acceptance of the GPT Image 2.5 options panel** — whether the two-group layout and the experimental-size warnings read well. | DEFERRED-TO-HUMAN |

## §3 Findings

### #120 (RECORDED, v4-faithful — a v4 filing candidate): the five largest help documents have **no embeddings at all**, neither whole-document nor section

Found by reading the boot log rather than by browsing: five `EmbeddingGenerate`
WARNs, all `Permanent embedding error — marked failed, skipping retry … OpenAI
embedding failed: Invalid 'input': maximum context length is 8192 tokens.`

Measured on the copy: `help_docs` holds 124 rows and **exactly 5 have a NULL
`embedding`** — *Chat Settings* (55,049 chars), *Connection Profiles* (55,030),
*Custom Tools — Pascal's Table* (49,372), *The Wardrobe* (45,266), *Editing
Characters* (40,278). The next document down, at 37,669 chars, embeds fine. And
**their 115 `help_doc_chunks` are unembedded too** — precisely the 115 of 463
that are not, so the section-level fallback is gone with the document-level one.

**This is v4-faithful and no v5 change is warranted.** The oversize skip fires at
`EMBEDDING_MAX_CHARS`, which is `128 * 1024` in **both** implementations
(`lib/embedding/embedding-service.ts:75`; `embedding_generate_job.rs:58`), so a
45 K document never reaches `skipIfOversize` — both call the provider and take
the same 8192-token refusal. And in both, the chunk pass sits **after** the
document embedding in the same `try` (v4 `embedding-generate.ts:440`; v5
`embedding_generate_job.rs:623`), so the throw takes the sections with it.

Why it is worth filing upstream anyway: the five documents are the five the
operator is most likely to ask about, and the whole point of P4.D77's
section-level embeddings was to make long pages retrievable. Semantic help
retrieval cannot see them at all. **Text search still can** — walk row D5
searched `accessories` and got six hits including `wardrobe`,
`character-editing` and `connection-profiles`, three of the five — so the
symptom is degraded ranking and a blind Ask-the-Guide, not a dead Guide.

The obvious upstream shapes: embed the **title plus a truncated body** for the
document vector, or move the chunk pass out of the document's `try` so a long
page still gets its sections. Neither is v5's call — this port follows v4.

### Three instrument errors, all mine, all caught before they became findings

The C8 row nearly became a false "the shrink never runs" report. The sequence is
worth keeping, because each step looked like evidence:

1. **The browser pane's `F5` does not reload the page.** After impersonating two
   seats over the API, the speaker selector was absent from the DOM and the
   composer still showed the old seat — which reads exactly like a client that
   ignores `impersonatingParticipantIds`. The tell was that the server's
   `activeTypingParticipantId` disagreed with the screen. `location.reload()`
   fixed it instantly. (Same class as the recorded `Cmd+Shift+R` trap.)
2. **Setting a `<select>` to a value not in its option list yields
   `selectedIndex: -1` and a meaningless `change` event.** The first C2 attempt
   set the model to `dall-e-3`, which this instance's live-fetched list does not
   contain, and the panel "did not update" — because nothing had been selected.
3. **A file attached by id that is linked to a DIFFERENT chat is correctly
   refused by the loader, and the model then hallucinates.** The first C8 turn
   attached a 2.3 MB PNG from another chat; no shrink line appeared and the
   model produced a confident, detailed, and **entirely wrong** description
   ("a monochrome illustration of a woman in a flowing dress"). Downloading and
   *looking at* the image — a full-colour photo of two people on a garden path —
   was what settled it. The guard was working; the gesture was invalid. The
   proof only landed once the image was imported with
   `tags: [{tagType: "CHAT", tagId: <chatId>}]`, which is what writes
   `files.linkedTo`.

The standing lesson holds and gained a corollary: **prove the instrument before
trusting a negative — and when a model describes an image, check the image.**

### Two v4-behaviour notes recorded in place, neither a defect

- **A profile's stored `size` is only honoured when the caller names no size.**
  Walk row C4 assembled `size=1024x1024` from a profile storing `1536x1024`;
  C6 then showed an explicitly named `1536x1024` surviving intact. The rule is
  v4's own post-bug-149 `input.size ? undefined : 'square'`: an **absent**
  request size still resolves square, and orientation outranks the profile by
  design. Reproduced, not filed.
- **`image_profiles.baseUrl` is carried by no v5 image caller.** Every dialect
  builds a fixed vendor URL, so a profile's base URL cannot redirect an image
  request. Already a **recorded, pre-existing, shared narrowing**
  (`model/image.rs:282-287`) — re-confirmed here because it is what makes a
  local-listener wire capture impossible for images, and it shaped C4's method.

## §4 Result

**39 rows run: 37 PASS, 0 FAIL, 1 NOT RUN, 4 DEFERRED-TO-HUMAN (plus the five
standing F-list items and four planted proofs left for the human).**
**Zero v5 defects found.** One finding recorded (**#120**, v4-faithful, a v4
filing candidate) and two v4-behaviour notes.

Five boots on the real 800 MB instance; **zero panics**, and the only
non-embedding WARN in the whole log was the expected "Not enough turns to fold".

### What the pre-walk measurement bought (ledger §5.5 working as designed)

v4 had landed bug 154 on the live instance at 07:04 that morning, so the §5.5
question was whether the stale-column population had already been healed away.
It had not — and better, **both** arms of the resolution order were sitting on
real data in genuinely disagreeing states: Friday's column names a prompt she no
longer has, Sunny's names one she does have while her `isDefault` flag sits on a
different one. Neither arm needed planting, and the Sunny arm is the sharper of
the two because it can only be satisfied by the column-wins rule.

### The proofs worth naming

- ⭐ **The opacity covenant ran on bug 152's own row** — Leilani, the `Severed`
  group, the chat the bug was filed from. She reaches her group store by name
  and by id; her listing shows three tiers and **none** of the instance's 52
  character vaults; the same chat's transparent seat sees her 200-file vault.
  One chat, two seats, one difference.
- ⭐ **The four GPT Image 2.5 body keys reached the wire for the first time**, at
  zero spend, because the assembled body is logged before the HTTP call — a
  deliberately invalid key made the whole proof free.
- ⭐ **`spoken` rendered in the participant rail** — a status v5 could never show
  before, matching the stored rotation seat for seat.
- ⭐ **The bug-146 fourth banner sentence** named the floor seat while the
  composer sat elsewhere, and Skip posted the floor seat's id on both the client
  dispatch and the persisted `turn-pass`.
- ⭐ **Bug 144's corrected `--lock-clean` sentences** read back on a genuinely
  dead PID — v4 having adopted this port's own filing (#119), now proven from
  the other side.
- ⭐ **The bug-151 shrink beat its own estimate**: 1,946,202 → 70,872 bytes.

### Owed onward

- The four `DEFERRED-TO-HUMAN` rows (C9, D8, D9 and the F-list) plus the four
  planted proofs in Part E, none of which a walk can do cheaply or safely.
- **#120** wants a look on live v4 and then an upstream filing.
- D7 was left NOT RUN rather than half-done.
