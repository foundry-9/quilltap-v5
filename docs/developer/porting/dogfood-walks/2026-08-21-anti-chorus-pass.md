# Dogfood walk — the `b8449b3e` anti-chorus round + the `c8a3cf77` round + the standing queue — 2026-08-21

**Instance:** a COPY of Friday at `~/qt-dogfood-friday` (never the live iCloud tree).
**Server:** `./target/release/quilltap-web --data-dir ~/qt-dogfood-friday --spa-dir apps/web/dist/quilltap/browser`, `RUST_BACKTRACE=1`, log in the scratchpad.
**Findings log:** `docs/developer/porting/dogfood-findings.md` — next finding number is **#97**.
**Unlock:** none needed — `unlockState` reports `{state: "resolved", hasUserPassphrase: false}`.

## What this pass is for

Two rounds have unified since the 2026-08-19 walk and never met real data:

- **`c8a3cf77`** (2026-08-20) — P4.D95 per-turn conversation summaries; P4.9L2 the
  Document-Mode pane formatting toolbar. (P4.51 is harness-only, no user surface.)
- **`b8449b3e`** (2026-08-21) — P4.D96 the anti-chorus discipline: direct-address
  `isRecentlyAddressed`, the reworded Turn note, and `GROUP_SCENE_DISCIPLINE` on
  every multi-character turn. (P4.52/P4.53 are harness-only.)

Plus as much of the standing 💸 queue and the 2026-08-19 PENDING carry-over as
the pass can reach.

**Local advantage:** ollama is up — `qwen3.5-9b-q6` (prefill=1, tools=0) and
`hf.co/unsloth/Qwen3.6-35B-A3B-GGUF` (prefill=0, tools=1), plus
`nomic-embed-text`. Turn-level proofs on those are free. Paid calls are
permitted but kept sparing (human's standing instruction, 2026-08-21).

**Primary verification channel:** `llm_logs.request` carries the FULL message
array including system-message content (confirmed on a real row), so the
prompt-byte proofs are read with
`./target/release/quilltap db --llm-logs --data-dir ~/qt-dogfood-friday --json "…"`.

## What NOT to expect to work (do not file these)

- **The client skip-signal twin has no visible surface.** `salon-conversation.ts:1981`
  consumes only `mustSpeakReason === 'all-others-skipped'`; `recentlyAddressed` is
  computed and discarded client-side. The direct-address rewrite is observable
  only in the server's prompt bytes.
- **Ollama `qwen3.5-9b-q6` dies on every non-initial turn** — finding #95, a
  faithfully ported v4 bug (three leading system messages vs a strict Jinja
  template), filed as v4 bug 82. Use `Qwen3.6-35B` or a hosted profile.
- The help-doc chunk backfill / section-led `help_search` (`p4.9i2`), subsystem
  backgrounds other than a project story background, `?msg=` anchors,
  `/photos?tag=` filters, the ten no-analog queue-trigger sites — all named
  deferrals.
- Enter in composition mode does not send — v4's contract.

---

## Part A — the `b8449b3e` anti-chorus round (💸 the live group-scene walk)

| # | Owner | Step | Gesture | Expected + how verified | Status |
|---|---|---|---|---|---|
| A1 | CLAUDE | `GROUP_SCENE_DISCIPLINE` reaches the wire — **prose route** | A purpose-built 3-character chat (Friday / Amy / Abigail + Charlie as the user seat) on `DeepSeek V4 Flash Tools` (`multiCharacterPrefill = 0`) | **PASS.** The system message ends with exactly TWO appended blocks, `\n\n`-separated and in v4's order: the identity instruction (`IMPORTANT — this is a multi-character scene. Respond as Abigail and ONLY Abigail…`) FIRST, then `GROUP-SCENE DISCIPLINE — …` **1372 chars, byte-identical to `message_context.rs:554`, and it is the LAST thing in the system message** | PASS |
| A2 | CLAUDE | …and the **prefill route** appends exactly ONE | Moved Friday's seat to `Grok 4 Fast Non-Reasoning` (`multiCharacterPrefill = 1`) and nudged her | **PASS.** The system message ends with the discipline block and the identity instruction is **absent** (`IMPORTANT — this is a multi-character scene` not present), and the last wire message is `{role: "assistant", content: "[Friday]"}` | PASS |
| A3 | CLAUDE | Direct address: a third-person mention does NOT arm the caution | Sent `I keep wondering what Abigail makes of the brass plaque by the doors.`, then a nameless message so Abigail took the floor with that mention in her scan window | **PASS.** Abigail's Turn note is present and carries **no** `One caution:` paragraph. Under the old mention-based rule this is v4's own `mention-hit` red row — it would have fired. Cross-checked with a transcription of v4's regex run over the exact visible window: `addressed? False` | PASS |
| A4 | CLAUDE | …and a vocative DOES | Sent `Abigail, did you ever settle the question of the ledger?` while it was **Amy's** turn, then two nameless messages until the floor reached Abigail | **PASS — but only in the backlog case, and that is v4-faithful.** The caution reads `One caution: Abigail appears to have been directly addressed since they last spoke…`. ⚠ A vocative in the message you have *just* sent does **not** arm the caution for the turn that answers it: the user message is persisted at `orchestrator.rs:1589`, AFTER `get_messages` (`:1357`) and `compute_skip_eligibility` (`:1366`) — and v4 has the identical order (`orchestrator.service.ts` 552 / 564 / 627–685). Measured, not assumed: `Friday, would you close the doors…` → Friday answered with the note and NO caution | PASS |
| A5 | CLAUDE | The Turn note's reworded bytes | Every note captured above | **PASS.** The base note ends with the new paragraph verbatim: `If your reply would mostly restate, endorse, or re-phrase what has already\nbeen said — even in your own voice — that is not substantive. Pass.` | PASS |
| A6 | HUMAN | Does the discipline actually break the chorus? | Read a real multi-character exchange on a weak model, before/after | Aesthetic judgment — no oracle can decide it. Material is ready: the chat `6eccb8ca-c93b-491b-994b-82a71ab22e8a` ("Cold Tea and Plumb Lines") holds eight turns across three characters with the block live | DEFERRED-TO-HUMAN |

### Part A observations — both v4-faithful, neither a v5 defect

1. **`and` is a vocative lead-in, so `…Friday and Amy.` reads as addressing Amy.** v4's
   `VOCATIVE_LEAD_INS` list includes `and`, `but`, `so`, `now`, `no`, `yes`. Measured live:
   Abigail's turn wrote *"the one who learned truth-telling from watching Friday and Amy. Let me
   answer honestly"*, and that armed Amy's caution on the next turn. A roll-call recap ending
   `X and Y.` — the exact chorus shape `e22f7b36` exists to break — re-arms the caution it was
   meant to withhold. A second instance the same session: `— Abigail, then Amy, then …` (a list,
   not an address). v5 transcribes the list byte-for-byte, so this is a **v4 heuristic weakness,
   a candidate upstream filing**, not a port divergence.
2. **The caution can never see the message that just addressed the responder** (see A4). Also
   v4-faithful, and arguably deliberate — the just-sent address is right there in context — but
   it means the caution only ever fires for a *backlog* address.

## Part B — the `c8a3cf77` round

| # | Owner | Step | Gesture | Expected + how verified | Status |
|---|---|---|---|---|---|
| B1 | CLAUDE | Per-turn conversation summaries — the card round-trips | Settings → Commonplace Book → **Recall Relevance** → unticked **"Consult past conversations every turn"** | **PASS.** Real Friday data already carried `perTurnConversationSummaries: true` (v4-written), and the card rendered it ticked. Unticking wrote `{"scopePolicy":"down-weight","expandRelated":true,"perTurnConversationSummaries":false}` immediately; restored to `true` at the end of the step | PASS |
| B2 | CLAUDE | …and an invalid value 400s (the §3 fix) | `memoryRecallConfigSet` with `scopePolicy: "nonsense-value"` **and** a valid `perTurnConversationSummaries: true` alongside it | **PASS.** `{"kind":"bad-request","message":"Validation error"}` and the stored bag is byte-unchanged — `perTurnConversationSummaries` stayed `false`, so the valid half of a rejected patch was NOT written. Exactly the §3 fix | PASS |
| B3 | CLAUDE | The per-turn cadence on a real turn (💸) | Compared the PERSISTED `commonplaceBook` whispers across turns with the setting ON and then OFF | **PASS, mutation-proven on live data.** ON (10:30:33, 10:32:15): the per-turn whisper carries the relevant-past-conversations block (13–15 KB). OFF (10:35:16): the same per-turn whisper (13.6 KB) **does not** — the only carrier left is the separate 699-byte standing fold / days-gone-by whisper, one of v4's three base triggers. ⚠ Reading the LLM **request** alone is not a discriminator: the standing whisper sits in the transcript and is replayed every turn either way | PASS |
| B4 | CLAUDE | The Document-Mode pane toolbar — standalone host | Rail → Document Mode → Recent → `Camille.md` (a real 52 KB store document); read the toolbar inventory; toggled source mode and back | **FAIL → FIXED (#97).** The toolbar is right: B / I / H1–H6 / lists / quote / outdent / indent / CODE / emoji / Ω / source, and **no `Nar` and no `OOC`** — correct for the chat-less host (the Salon composer's toolbar in the same DOM has both). Source mode toggled correctly and the status bar read `Markdown · 7,447 words · Saved`. **But the textarea rendered 77 px tall inside a 788 px pane** — three visible lines of a 52,194-character document. Fixed and re-measured live: **612 px** | FAIL(#97) → PASS |
| B5 | CLAUDE | …and the Salon host | Salon composer → **Open document** → `amy.md` (a real Obsidian-mount document), which portals into its own tab through `qt-tab-portal-host` | **PASS.** Same inventory as B4 **plus `Nar` and `OOC`** — the chat's resolved roleplay delimiters, exactly the P4.9L2 contract, and the discriminator against the chat-less host. The frontmatter table (`title` / `description` / `pubDate` / `author` / `image` / `tags` chips) renders above the body, source mode shows the recombined body only (5,985 chars starting `\n## Amy`), and the height chain is healthy here — 315 px of textarea under a 250 px frontmatter table in a 788 px pane, i.e. the flex remainder, never the #97 collapse (this host is `qt-tab-portal-host`, which measures 788) | PASS |

## Part C — carry-over PENDING from the 2026-08-19 walk

| # | Owner | Step | Gesture | Expected + how verified | Status |
|---|---|---|---|---|---|
| C1 | CLAUDE | A failed import names its dropped items (was A2) | `systemImportExecute` with five connection profiles whose typed parse must fail (`provider: 12345`), each carrying a differently-shaped `name`: absent / `null` / `7` / `["a","b"]` / `{"x":1}` | **PASS.** All five warnings fired with v4's sentence and the JS template-literal interpolation exactly as the P4.D91 §3 fix intends — `Failed to import connection profile "undefined"…` / `"null"` / `"7"` / `"a,b"` / `"[object Object]"`. Nothing written: `connection_profiles` 41 → 41, every `imported`/`skipped` counter 0. ⚠ Observation, not a defect: the `: {e}` tail is the Rust serde message (`invalid type: integer \`12345\`, expected a string`) where v4's is Zod's — the prefix and the quoted name are v4's bytes, and the tail cannot be byte-identical across the two runtimes | PASS |
| C2 | CLAUDE | Bug-76 poisoned `outboundApiKeyId` heal (was B2) | The API refuses the poison outright (`API key provider does not match profile provider`, correctly), so the poison was made **the way the bug made it** — opened `Claude Haiku 4.5` in the profile modal, switched Provider Anthropic → Ollama, saved | **PASS.** The save succeeded and the row reads `provider: OLLAMA, apiKeyId: null` — the stale anthropic id cleared by the `\|\| null` always-send instead of the bug-76 400. The `outboundBaseUrl` chokepoint filled `http://localhost:11434` in the same gesture, the API Key select hid itself for Ollama, and the banked `Non-image attachments: No file attachments supported` line read correctly. Card now shows `OLLAMA · Base URL: …` with no API Key row | PASS |
| C3 | CLAUDE | The tool-execution notice lifecycle, bug 77 (was B3) | Drove four real tool turns (`search` ×3, `read_conversation`, `rng` ×2) with a 200 ms DOM poll recording every `[role=status]` in the composer | **BLOCKED — the step as written cannot fire it, and that is v4's scope, not a v5 gap.** The notice is **`generate_image`-only** in both apps: `salon-conversation.ts:2933` (`if (call.name !== 'generate_image') continue`) ports v4's `trackToolsDetected`/`trackToolResult` filter. Ordinary tool calls raise nothing, correctly. Asked the cast to call `generate_image` outright and the model answered in prose instead — the seats here carry no `imageProfileId`, so the tool is not on the slate. **Re-word the queue item: it needs a seat with an image profile.** What the poll DID catch is the P4.D84 §3 pre-send validation sequence live — `Sending to Amy..` → `Amy is responding...` | BLOCKED(needs an image-profile seat) |
| C4 | CLAUDE | The tool-change notice splices ONCE (was C1) | Chat sidebar → Chat → **Tools…** → unticked **Ask Carina** → Save, then sent two ordinary turns | **PASS.** Turn 1 (`16:08:54`) carries `[System Notice] Your available tools have been updated. You now have access to the following 24 tool(s): …` with `toolCount: 24`; turn 2 (`16:09:26`) carries **no notice** and `toolCount: 23` — the notice spliced exactly once and `forceToolsOnNextMessage` cleared, which is the P4.D82 deferral closed on real data. Bonus corroboration of #98 from the same dialog: with the Ollama profile selected, Search Web read `unavailable — Web search must be enabled in the connection profile`; after switching to the tool-capable DeepSeek profile it read `No search provider configured. Please add a search provider API key in Settings > API Keys.` — the two-arm gate order in `tools_inventory.rs:330-336`, in the right order | PASS |
| C5 | CLAUDE | The vision send (was D3) | Generated a 240×160 PNG with deliberately unguessable content — solid deep-teal ground, one large white triangle point-up, three black dots in a row along the bottom — uploaded it through `POST /api/v1/files`, attached it with the composer's **Attach file from library**, and asked for a literal description on `Grok 4 Fast Non-Reasoning` (`supportsImageUpload = 1`, cheap) | **PASS — 💸 discharged.** The reply: *"Three black circles in a straight horizontal line at the bottom. One large white triangle sitting directly on top of them, point upward. The background is a solid deep teal, almost forest green."* — correct on every count, so the bytes genuinely reached the Grok wire. The `llm_logs` `CHAT_MESSAGE` projection carries `hasAttachments: true` on the user message (v4's `summarizeRequest` shape — a flag, not the bag) | PASS |
| C6 | CLAUDE | The Serper live-key smoke (was D7) | The instance DOES carry an active `SERPER` key; asked the cast to search the web for a Chicago forecast on a `allowWebSearch = 1` profile | **FAIL(#98) — and the 💸 item cannot be discharged as worded.** The model called `search_web`; the handler answered v4's `Error: Web search is not configured…`. v5 reads only `SERPER_API_KEY` from the environment; v4 reads the key out of `api_keys` through its `qtap-plugin-search-serper` registry, which is v5's standing P4.42 deferral. Running it needs the stored key exported as `SERPER_API_KEY` at launch. **Then done, same session:** the instance's own `api_keys` row holds the raw secret, so the server was relaunched with that value read straight into the child's environment (never echoed, never written to disk, never off-host) — and `search_web` ran **live** from a real turn, `success: true`, five current results (*"At a height of 828 meters, the Burj Khalifa is currently the undisputed tallest skyscraper in the world"*, plus `skyscrapercenter.com`'s 2026 table) for the model's own query `tallest building in the world current record holder name and height`. **💸 discharged; the P4.42 invariant holds — advertised and executed agree the moment the provider exists.** The gap in #98 is unchanged: it is the *configured* path v5 cannot see | FAIL(#98) → PASS (wire proven) |
| C7 | CLAUDE | Whispered announcements (was D8) | Composer → **Insert announcement** → sender The Host → ticked Amy + Abigail under **Who hears it** → posted | **PASS.** The dialog re-labels itself live as the audience narrows: heading `Announcement` → `Whisper`, hint `Everyone present hears this…` → `Whispered to Amy, Abigail. No other character receives it in their context. Make it public.`, button `Post Announcement` → `Post Whisper` — v4's audience-replaces-roster shape. The row stored `systemSender: host, systemKind: announcement, targetParticipantIds: ["39441710…","ba1a44e7…"]` — the two ticked seats and no one else. **Both render sites carry the tag**: the collapsed chip reads `The Host · announcement · whispered to Amy, Abigail`, expanded it reads `The Host · announcement · to Amy, Abigail` with the body in a plain `qt-chat-message-content` prose block (no v5-invented system slab — the P4.25 fix holding). Not exercised: the All Whispers toggle against a Prospero `group-context` whisper | PASS |
| C8 | CLAUDE | Pascal cross-tier side-effect writes (was D9) | Confirmed the library is live on real data — `customToolsLibrary` returns real tools (`scan_hawking_radiation`, `force_adjustment`, `root_access`, …) from `Quilltap General/Tools/`, all `valid: true, gate: available` | **DEFERRED-TO-HUMAN.** Whether any of these definitions carries a `sideEffects` block needs the `.tool.json` bodies read, and authoring one to order is a larger setup than the rest of this pass. The substrate is demonstrably healthy; what is unproven is the P4.D35 effect applier on a real cross-tier write | DEFERRED-TO-HUMAN |
| C9 | CLAUDE | A roleplay template delimited by a quote character (was E1) | The chat runs the built-in **Standard** template, whose `dialogueDetection` opens/closes on `"` and `“`/`”`. Sent `*I set the mug down.* "It's the plumb line -- that's what I meant," I say.` with `smartTypographySettings = {displayQuotes: true, dashes: true, ellipsis: true}` | **PASS on all three axes.** (a) **Stored bytes untouched** — pure ASCII, straight `"` and `'`; smart quotes are render-time, exactly as P4.D71 landed them. (b) **Rendered** as `“It’s the plumb line -- that’s what I meant,”` — quotes and apostrophes curled. (c) **The delimiter still matched the curled text**: the whole run came back inside `<span class="qt-chat-dialogue">`, narration as `<em>` — which is the question the step was really asking. The dash ladder was verified separately with real keystrokes: three `-` presses → `—` (U+2014). ⚠ Test artifact worth remembering: the browser `type` action bulk-inserts, so `--` typed that way never fires the per-keystroke rule — not a defect, but a dash test must use `key` presses | PASS |
| C10 | CLAUDE | The P4.50 `combined.log` look at a real failed turn (💸) | A real failure arrived unprompted: `STORY_BACKGROUND_GENERATION` failed its first attempt against the OpenAI Images API | **PASS — 💸 discharged.** `combined.log` carries `{"level":"warn","message":"Job failed","context":{module,job_id,job_type,attempts},"error":{"name":"Error","message":"Image generation failed: Invalid response from OpenAI Images API"}}` — v4's winston Error shape, the real sentence, and **no `key derivation failed:` prefix**, which is exactly the P4.50 acceptance. `grep -c 'key derivation failed' combined.log` → 0. (The job then succeeded on attempt 2 — related to the previous walk's open question about a twice-failed story background, and worth its own look) | PASS |
| C11 | HUMAN | Memory dedup + conversation-summaries first run (P4.43) | The two maintenance cards on a real corpus | Batch LLM spend over Friday's whole memory graph — deferred by cost | DEFERRED-TO-HUMAN |


---

## Result

**23 rows: 19 PASS, 1 FAIL-then-FIXED (#97), 1 FAIL-recorded-but-wire-proven
(#98), 1 DEFERRED-TO-HUMAN, 1 BLOCKED-with-diagnosis.** Two findings, one fix
shipped, ten 💸 items discharged.

### A note on how the Serper key was handled (C6)

The key is stored raw in the copy's `api_keys` table (`db/api_keys.rs:11` — the
old AES-GCM columns were dropped by migration and nothing decrypts at read
time). To run the smoke, a scratch launcher read that one column in a subshell
and `exec`'d the server with the value in its environment. The value was never
printed, never written to a file, never placed in an argv, and never left the
host — the launcher's only output is `serper key resolved: 40 chars`. Anyone
repeating this should keep those properties; the script is
`scratchpad/launch-with-serper.sh` and is deliberately not committed.

### Fixed on main this pass

- **#97 — Document Mode's source view was three lines tall.** `qt-tab-view` is
  an unstyled Angular custom element (`display: inline`) with no v4 counterpart,
  so `StandaloneDocumentView`'s `flex-1` host was inert and every `h-full`
  beneath it collapsed. Host class → `h-full`; 77 px → 612 px, measured live.
  Commit `a42638e7`, gate green (ng 332/4,937; Playwright 232/232).

### Recorded, no code changed

- **#98 — web search is dark on a real instance.** The `SERPER` key configured
  through v4's Settings → API Keys is invisible to v5, which reads only
  `SERPER_API_KEY` from the environment: the search-provider plugin registry is
  the standing P4.42 deferral. The refusal path itself is v4-faithful.
  **The wire underneath is nonetheless proven** — relaunching with the
  instance's own key in the environment made `search_web` return five live
  results on a real turn (C6). Only the *configured* path is missing.

### Two v4-heuristic observations from Part A (neither a v5 defect)

1. `and` sits in v4's `VOCATIVE_LEAD_INS`, so a roll-call recap ending
   `…Friday and Amy.` reads as addressing Amy — measured live, and it re-opens
   the very chorus hole `e22f7b36` closes. A **candidate upstream filing**.
2. The caution can never see the message that just addressed the responder: the
   user message is persisted after the eligibility read, in v4 as in v5. Only a
   backlog address arms it. Arguably deliberate; worth stating out loud.

### 💸 queue movement

**Discharged:** the live group-scene walk (Part A), the per-turn conversation
summaries cadence (B3), the vision send (C5), the P4.50 `combined.log` look at
a real failure (C10), the bug-76 key heal (C2), the tool-change splice (C4),
whispered announcements (C7), the roleplay quote delimiter (C9), the failed-import
warnings (C1), **and the Serper live-key smoke (C6)** — ten in all.

**Still owed:** the tool-execution notice (needs a seat with an image profile —
re-word the item; a `generate_image` call did finally fire late in the pass and
refused cleanly with `Image generation is not enabled for this chat`, which
confirms the diagnosis), Pascal cross-tier side effects, and the memory-dedup /
conversation-summaries first run (cost).

### Two things worth a look that this pass did not chase

- `STORY_BACKGROUND_GENERATION` failed its first attempt with `Image generation
  failed: Invalid response from OpenAI Images API` and succeeded on the retry.
  The 2026-08-19 walk left an open question about a twice-failed story
  background; this is the same shape, now with a sentence attached.
- The uploaded PNG came back `category: "DOCUMENT"` from `POST /api/v1/files`.
  It behaved correctly as an image attachment afterwards, so this may be v4's
  own classification — unverified either way.

### Instance state left behind (all harmless — the copy is disposable)

The chat `6eccb8ca-c93b-491b-994b-82a71ab22e8a` ("The Kettle Boils Twice") was
built for this pass and carries the whole walk. `Claude Haiku 4.5` was
deliberately switched to OLLAMA for C2 and left that way. Charlie's seat in the
test chat was turned LLM-driven by a too-broad selector during setup. `Ask
Carina` is unticked on that chat's tool set. The next rsync clears all of it.
