# Dogfood walk — the four un-walked rounds (`12fe3e6f` → `4cb1035e` → `a6870c5a` → `f8973813`) + the standing 💸 queue — 2026-08-22

**Instance:** a COPY of Friday at `~/qt-dogfood-friday` (never the live iCloud tree).
**Server:** `./target/release/quilltap-web --data-dir ~/qt-dogfood-friday --spa-dir apps/web/dist/quilltap/browser`, `RUST_BACKTRACE=1`, log in the scratchpad.
**Findings log:** `docs/developer/porting/dogfood-findings.md` — next finding number is **#100**.
**Unlock:** expected none — the 2026-08-21 pass measured `unlockState = {state: "resolved", hasUserPassphrase: false}`. Confirm before Part A; if it prompts, the passphrase is the human's step.

## What this pass is for

Four rounds have unified since the 2026-08-21 walk and none has met real data:

- **`12fe3e6f`** (2026-08-22) — P4.D97 the thinking-turn evaluator + the model-aware
  DeepSeek prefill strip + the retire-prefill heal; P4.D98 the profile editor's three
  thinking-turn behaviors; P4.D99 v4 bug 84 — the tool-error sentence reaching the UI
  (this port's own finding #99 coming back fixed).
- **`4cb1035e`** (2026-08-22) — P4.D100 the honest image `list-models` verb (Fetch Models
  for image profiles); P4.D101 NanoGPT as the tenth provider (chat + images + embeddings);
  P4.D102 the SPA half.
- **`a6870c5a`** (2026-08-22) — P4.D103 standing instructions (project + group prompts in
  every system prompt), bug 88's second-person tool reinforcement, the version-stamped
  `compiledIdentityStacks` envelope; P4.D104 the shared prompt-field label + the Group
  Instructions editor; P4.55 validate-first on the merge verbs.
- **`f8973813`** (2026-08-22) — P4.D105 NanoGPT prompt caching; P4.56 the data-retention
  present-`null` 400 + the brahma-console REST edge that had 500'd on every success
  since P4.D57.

Plus the standing 💸 queue and the carry-over from 2026-08-21.

**Local advantage:** ollama is up — `qwen3.5-9b-q6:latest` (9B) and
`hf.co/unsloth/Qwen3.6-35B-A3B-GGUF:UD-Q5_K_XL` (35B, tools), plus `nomic-embed-text`.
Turn-level proofs on those are free. Paid calls stay sparing (human's standing
instruction, 2026-08-21).

**Primary verification channel:** `llm_logs.request` carries the FULL message array
including system-message content, read with
`./target/release/quilltap db --llm-logs --data-dir ~/qt-dogfood-friday --json "…"`.
(`llm_logs` is the llm-logs partition, call-type column is `type`; `memories`,
`chats`, `groups`, `projects` are on main.)

## What NOT to expect to work (do not file these)

- **NanoGPT needs a key.** No NanoGPT credential is expected on the Friday copy. Every
  NanoGPT step (chat, images, embeddings, prompt caching) is `HUMAN`-gated on a key
  being entered, and `BLOCKED(no key)` otherwise — not a defect.
- **The data-retention 400 has no screen gesture.** P4.56's Tier-3 survey measured that
  the card can neither send `null` nor omit the key, so the 400 is provoked by `curl`
  against the REST edge. That is the intended verification, not a workaround.
- **Web search is dark on a real instance** — finding #98: the `SERPER` key configured
  through Settings → API Keys is invisible to v5 (the search-provider plugin registry is
  the standing P4.42 deferral). Only `SERPER_API_KEY` in the environment works.
- The help-doc chunk backfill / section-led `help_search` (`p4.9i2`), subsystem
  backgrounds other than a project story background, `?msg=` anchors, `/photos?tag=`
  filters, the ten no-analog queue-trigger sites — all named deferrals.
- The client skip-signal twin still has no visible surface (2026-08-21 note).
- Enter in composition mode does not send — v4's contract.

---

## Part A — the `12fe3e6f` thinking-turn round

| # | Owner | Step | Gesture | Expected + how verified | Status |
|---|---|---|---|---|---|
| A1 | CLAUDE | The retire-prefill heal fires once on a Friday-vintage copy | Boot against the freshly-rsynced copy; read the boot log + the ledger | **PASS — and a free cross-implementation proof.** The copy arrived with the row ALREADY WRITTEN BY v4 (`quilltapVersion: 4.9.0-dev.43`, `completedAt: 2026-08-22T02:00:57.124Z`), so v5's boot correctly returned `AlreadyCompleted` and logged nothing — the cross-app ledger working in the v4→v5 direction on real data. To prove v5's own write direction, the row was deleted on this disposable copy and the server restarted: v5 logged `Retired the [Name] prefill on thinking connection profiles examined=1 cleared=0` and wrote `message = "Examined 1 prefill-enabled profile(s) on thinking-capable providers; turned the [Name] prefill off on 0"` — **byte-identical to v4's own message over the same 50 real profiles, same verdict** (only `quilltapVersion` differs, which the module header calls informational). The single candidate is `OLLAMA / Qwen3.5-9B` (`prefill=1`, params carry no thinking key → runs no thinking turn → correctly left alone) | PASS |
| A2 | CLAUDE | …and survives a second boot (idempotent) | Restart, re-read | **PASS.** Proven twice over: this session's FIRST boot skipped silently on v4's row, and the ledger holds exactly one row for `retire-prefill-on-thinking-profiles-v1` after the re-run. No `connection_profiles` row changed (`cleared=0`) | PASS |
| A3 | CLAUDE | The thinking-turn evaluator picks the model-shaped answer | A 13-case matrix of throwaway profile creations against the REAL built-in registry, each omitting `multiCharacterPrefill` so the create path resolves it through `profile_runs_thinking_turn`; read the stored value back | **PASS, 13/13.** The rule outranks the model habit (`thinking:"disabled"` on `deepseek-v4-flash` → prefill ON, though the model thinks by default); `disabledValues` is tested BEFORE `enabledValues`; an unknown rule value and the empty-string "(model default)" spelling both fall through to the model facts; ANTHROPIC is off regardless; GROK (no rule, no facts) is on; NanoGPT's `reasoning_effort` enum discriminates `high` (off) from `none` (on), and `anthropic/claude-sonnet-5:thinking` is off on its `thinksByDefault` alone. All 20 throwaway profiles deleted (`leftover 0`). ⓘ **A first run appeared to find two divergences and did not** — it sent `parameters` as a JSON *string*; v4's create route destructures `parameters = {}` off the raw body with no Zod and hands it straight to `profileRunsThinkingTurn`, so a string reads as no-option on BOTH sides. v5 was faithful; the test payload was wrong | PASS |
| A4 | CLAUDE | v4 bug 85's repro chat completes | A NEW 3-seat chat (Amy + Abigail + Charlie) created through the New Chat screen with BOTH AI seats moved to `DeepSeek V4 Flash Thinking` (DEEPSEEK provider, `deepseek-v4-flash`, `thinking` in its params); greeting + one user message + one multi-character turn | **PASS.** Both turns completed, no 400 anywhere in `llm_logs` or the server log. The wire proves the mechanism: the request's **last message is `role: user`** — no assistant `[Name]` prefill tail, which is precisely what bug 85's 400 fires on. 14 messages, 3 leading `system` blocks. The Green Room dressed both characters on the way in, showing all FIVE slots incl. HAIR (Amy: *Soft Low-Twist with Loose Waves*; Abigail: nothing) — the P4.D87 slot live in the preview | PASS |
| A5 | CLAUDE | The profile editor's thinking-turn behaviors | Edit `DeepSeek V4 Flash Thinking` (stored `thinking: "enabled"`, prefill off): tick the `[Name] prefill` box, then flip Thinking Mode to Disabled | **PASS, mutation-proven in both directions.** Ticking the box raises v4's amber warning verbatim — *"This model reasons before it answers, and a turn handed over already opened sits badly with that: some providers refuse the request outright, others quietly swallow the reasoning altogether…"* — because `prefillOnThinkingProfile()` needs all three of ticked + provider-default-on + `runsThinkingTurn()`. Switching **Thinking Mode → Disabled** retracts the warning **while the checkbox stays ticked**: the browser twin of the shared evaluator re-derived `runsThinkingTurn` from the rule's `thinking` key, live, and it **warns without vetoing** — the tri-state the order describes. Cancelled without saving; the row is untouched (`isDefault 0`, `prefill 0`, params unchanged). ⚠ **A stray earlier selector of mine ticked `Set as default profile` on this profile** — caught before saving by re-reading the row; the real default (`DeepSeek V4 Pro Thinking`) was never at risk | PASS |
| A6 | CLAUDE | Bug 84 — a failed tool call's real sentence reaches the UI | Force a `generate_image` failure on a chat whose seats resolve no image profile (or point a seat at a broken image profile), on a tools-capable model | The **notice** and the **toast** carry the provider's real sentence from `toolResult.error`, not the generic `Failed to generate image`. Verified on screen; cross-checked against the frame in the network tab | PENDING |

## Part B — the `4cb1035e` image `list-models` + NanoGPT round

| # | Owner | Step | Gesture | Expected + how verified | Status |
|---|---|---|---|---|---|
| B1 | CLAUDE | Image-profile **Fetch Models**, unkeyed | Settings → Images → an image profile → open the modal without selecting an API key | The built-in list shows with *"Showing the plugin's built-in model list — select an API key and Fetch Models to query the provider."* | PENDING |
| B2 | CLAUDE | Image-profile **Fetch Models**, keyed (live) | Same modal with a real key selected (whichever image provider Friday has keyed) → click Fetch Models | A real catalogue lands and the built-in hint disappears; the list order is the provider's, not the manifest's. Verified on screen + the network response | PENDING |
| B3 | CLAUDE | The OpenRouter image-discovery finding, on a real key | Fetch Models on an OpenRouter **image** profile | **Expected to FAIL, faithfully** — v4's own SDK zod strips the wire keys its discovery reads, so every keyed listing throws (`openrouter/models_live_every_signal` is the convergence tripwire). Record the observed error; do not file as a v5 defect | PENDING |
| B4 | HUMAN | A NanoGPT key exists? | Settings → API Keys → add a NanoGPT key (credential entry is the human's step) | Once present, B5–B7 and D1–D3 unblock | PENDING |
| B5 | CLAUDE | NanoGPT chat | A short chat turn on a NanoGPT connection profile with `reasoning_effort` set | The turn completes; `llm_logs` shows the NanoGPT request with the flat `reasoning_effort` key, and reasoning renders | PENDING |
| B6 | CLAUDE | NanoGPT image generation | Generate one image on a NanoGPT image profile | One image lands in `files/`; the download seam decoded it (no zero-byte file) | PENDING |
| B7 | CLAUDE | NanoGPT embeddings | Point an embedding profile at NanoGPT and embed one memory | The vector lands; `llm_logs` shows the embedding call with no doubled error prefix | PENDING |

## Part C — the `a6870c5a` prompts trio

| # | Owner | Step | Gesture | Expected + how verified | Status |
|---|---|---|---|---|---|
| C1 | CLAUDE | The Group Instructions editor round-trips | Edit Group → **Sebold Family** (chosen because its instructions were empty, so the real Constellation prompt stays untouched) → type → Save → clear → Save | **PASS, both arms.** After saving, the API reads back `"Speak plainly here; the Sebold table has no room for ceremony."`; after clearing and saving again it reads back **`null`**, not `""` — the client's `|| null` normalization, exactly as P4.D103 measured. ⓘ `groups.instructions` is a **store-overlay property, not a SQL column** — a `quilltap db` SELECT errors on it; the API is the only reader (the standing overlay rule) | PASS |
| C2 | CLAUDE | **Standing instructions on a REAL turn — the group leg** | The A4 chat: Abigail is a member of the real Friday group **Constellation**, which already carries a prompt written in v4 | **PASS, on real data with a real group prompt.** The system message carries `[STANDING INSTRUCTIONS]` with the preamble byte-identical to `standing_instructions.rs`'s `STANDING_INSTRUCTIONS_PREAMBLE`, then `## Group Instructions — Constellation` and the group's own text. Position proven by offset, not by eye: taboo=6343 < standing=7035 < tools=7806 — **between the Taboo section and the tool instructions**, exactly as the module documents | PASS |
| C3 | CLAUDE | …the project leg, and both together | **A stronger check than planned was available: v4 itself is running this feature on this instance**, so v4's own `llm_logs` rows from 03:00–04:00 today are an oracle. Extracted the whole `[STANDING INSTRUCTIONS]` section from a v4-written Abigail row and from v5's own, and compared bytes | **PASS — byte-identical, 773 bytes on both sides.** A free cross-implementation proof on real data: preamble, `## Group Instructions — Constellation` heading, and the group's own text, identical between the two implementations. All six v4-written CHAT_MESSAGE rows sampled carry the section, as v5's do. The multi-source ICU collation and the project leg were NOT exercised (only one instructed entity exists on this instance) — carried to the remainder | PASS |
| C4 | CLAUDE | A chat with NO standing instructions is byte-identical to the old layout | Any chat outside a project with a character in no instructed group | No `[STANDING INSTRUCTIONS]` substring anywhere in the system message | PENDING |
| C5 | CLAUDE | Bug 88's second-person tool reinforcement | The same A4 turn (tools present — `toolCount` set) | **PASS.** `## Tool Execution Rules (MANDATORY — overrides all other behavioral patterns)` is fixed second person throughout (*"You have access to tools. When you decide to use a tool, you MUST actually invoke it…"*); the bug-88 string `they CALLS` is absent | PASS |
| C6 | CLAUDE | The `compiledIdentityStacks` envelope invalidates correctly | Take a chat with a cached stack; confirm the stored envelope carries the version stamp; confirm a turn reuses it | The stored value is the versioned envelope (not a bare stack) and the current builder version matches, so old chats are recompiled rather than serving stale wording. Verified by `SELECT compiledIdentityStacks FROM chats WHERE id=…` | PENDING |
| C7 | CLAUDE | The shared prompt-field label + hints, across surfaces | The Group Instructions field on the Edit Group screen | **PARTIAL PASS — the group surface confirmed.** The label renders through the shared host with its single-sourced hint and the italic worked example: *"Standing instructions folded into the prompt of every member of this group, addressed to the character they reach. Written as: You have known the others here for years; you do not explain yourselves to each other."* Correct box height, no inline-host collapse (the finding-#97 class). The other six migrated surfaces were not walked this pass — see the remainder list | PASS |
| C8 | CLAUDE | P4.55 validate-first — a rejected memories config saves NOTHING | Send an invalid `memoryRecall` / memories-config value via the dispatch API | **400 `Validation error`** and the stored bag is UNCHANGED (the old behavior persisted garbage). Verified by a before/after read of `instance_settings` | PENDING |
| C9 | CLAUDE | …and the same shape reads usefully on a live screen | Trigger a rejected save from Settings where the card can express one | The error surfaces as a toast/inline message a person can act on, not a silent no-op or a raw 500. **Aesthetic half is HUMAN** | PENDING |

## Part D — the `f8973813` NanoGPT caching + settings wire

| # | Owner | Step | Gesture | Expected + how verified | Status |
|---|---|---|---|---|---|
| D1 | CLAUDE | The Prompt Caching card renders | Settings → Providers → Connection Profiles → Edit `GLM 5.2 Thinking` (NANOGPT) | **PASS.** The **Prompt Caching** group renders last, after Reasoning Effort, with the long Wodehouse-register helpText verbatim (*"NanoGPT's OpenAI- and Gemini-routed models cache repeated context on their own, gratis and automatic…"*) and the short one on the toggle. `cacheTTL` is **absent** until the box is ticked; ticking it reveals **Cache Duration** with exactly the two manifest labels — `5 minutes (1.25x write cost)` / `1 hour (2x write cost)` — defaulted to 5 minutes. The `showIf` works in both directions | PASS |
| D2 | CLAUDE | The body key is sent, strictly | A throwaway NANOGPT profile whose Base URL points at `harness/tools/wire-tap.py` (so the REAL NanoGPT builder's bytes are readable), driven by nudging a live seat — twice, with `cacheTTL: "1h"` and then with the key removed entirely | **PASS on the real wire, both arms.** Body 1: `promptCaching: {"enabled": true, "ttl": "1h"}`. Body 2 (no `cacheTTL` stored): `{"enabled": true, "ttl": "5m"}` — the collapse-to-5m the order specifies, proven live rather than by corpus. In BOTH, `enablePromptCaching` and `cacheTTL` are **absent from the body** — consumed, never forwarded as profile params. Top-level key order: `model, messages, temperature, max_tokens, top_p, stream, stream_options, tools, tool_choice, promptCaching`. ⓘ `llm_logs.request` cannot prove this — it logs a projection (`messageCount/messages/temperature/maxTokens/toolCount`), not provider body keys; the tap is the only channel that can | PASS |
| D3 | CLAUDE | 💸 The live caching smoke | A Claude-routed NanoGPT model, two turns of the same long conversation; then switch TTL to 1h and repeat | `cacheUsage` appears in the LLM Inspector and the cost display; the second turn shows a cache READ. **Real spend — small, but confirm with the human first** | PENDING |
| D4 | CLAUDE | The data-retention present-`null` 400 | `PUT /api/v1/settings/data-retention` with `{"staleChatDays": null}` | **PASS.** `400 {"error":"Validation error"}` — the P4.56 fix. Before it, this was a 200 that silently kept the stored value | PASS |
| D5 | CLAUDE | …and absent-key still keeps, value still writes | Same edge with `{}` and with a real number | **PASS.** `GET` → `{"staleChatDays":30}`; `PUT {}` → 200 with 30 **unchanged** (absent still means keep); `PUT {"staleChatDays":45}` → 200 and `GET` reads back 45. Restored to 30 afterwards. All three arms of the tri-state behave differently, which is the whole point of the `double_option` fix | PASS |
| D6 | CLAUDE | The brahma-console REST edge answers on SUCCESS | `GET` + three `PUT`s | **PASS.** `GET` → `200 {"maxAgentTurns":50}` — the edge that 500'd on every SUCCESS from P4.D57 until P4.56 now answers. `PUT {"maxAgentTurns":75}` → 200 and reads back 75; out-of-bounds `500` → `400 Validation error` (the 5–200 bounds); explicit `null` → `400` (P4.D57's `double_option` holding). ⓘ A first attempt used the wrong field name and got `200` with the entity **unchanged** — the known unknown-field trap, not a bug | PASS |
| D7 | CLAUDE | The raised budget binds on a real deep query | Run a Brahma Console query that used to exhaust at 25 agent turns, with the budget raised | The run continues past 25 turns, or salvages byte-exactly at the raised bound. Verified in the transcript + `llm_logs` call count | PENDING |

## Part E — the standing 💸 queue and the 2026-08-21 carry-over

| # | Owner | Step | Gesture | Expected + how verified | Status |
|---|---|---|---|---|---|
| E1 | CLAUDE | **v4 bug 82 is fixed** — the leading-system fold on a strict local template | Amy's seat moved to the `Qwen3.5-9B` OLLAMA profile — the exact model of finding #95 — and nudged on a **non-initial** turn (41 messages of history), the shape that used to die | **PASS behaviorally.** The turn completed in 41.9 s with real prose and `error: None`, where before P4.D93 every non-initial turn on this model failed the strict Jinja template. Two riders: `durationMs = 41930` (a second live confirmation of the #100 fix), and the wire's last message is `role: assistant` — the `[Amy]` prefill, matching A3's matrix row for an Ollama profile with no thinking key. ⓘ The byte proof of the fold itself is NOT readable from `llm_logs`: the logged projection is the **pre-builder** message array (it shows 3 leading system messages), and `collapse_leading_system_messages` runs inside the request builder, below the log. See E1b | PASS |
| E1b | CLAUDE | …and the fold proven at the byte level | The same profile's Base URL repointed at `wire-tap.py` (11435 → ollama 11434) and nudged again, so the post-builder body is readable | **PASS — and the tap holds the contrast in one file.** The **OLLAMA** body carries **1** leading `system` message of 11,874 chars containing all three originals (`## Character Identity` + `## Identity Reminder` + `## Agent Mode Instructions`) — folded. The two **NANOGPT** bodies captured earlier, same model and same conversation, carry **3** — unfolded. That is P4.D93's scoping proven in both directions at the byte level on one machine, and it also explains the 500 seen during D2: a NanoGPT-builder request to a strict local template is bug 82's exact mechanism, which is precisely why the fold is scoped to the Ollama and OAC builders | PASS |
| E2 | CLAUDE | …and hosted requests are byte-identical | The A4 DeepSeek turn | **PASS.** The DEEPSEEK request carries **three separate leading `system` messages** (Character Identity 10562 / Identity Reminder 788 / Agent Mode 1928) — unfolded, as the P4.D93 fold is scoped to the Ollama and OAC builders only | PASS |
| E3 | CLAUDE | A bearer-token OpenAI-Compatible endpoint holds a key | An OAC profile against a local server, with an API key selected (v4 bug 81's fix — `acceptsApiKey`) | The key field is present and unstarred; the request carries the bearer header. Verified with `wire-tap.py` | PENDING |
| E4 | CLAUDE | The candid story-background prompt | Trigger a `STORY_BACKGROUND_GENERATION` on a project | The crafter selects the CANDID variant per call; the concealed path is unchanged. Verified in `llm_logs.request` for the image-prompt call | PENDING |
| E5 | CLAUDE | Pascal side effects — the other three write paths | P4.D35's remaining write paths (only `agent_lambda` on the character vault was proven live 2026-08-21) | Each path commits its effect where it lives, siblings intact. Verified by reading the written row | PENDING |
| E6 | HUMAN | The memory-dedup + conversation-summaries first run | Settings → the maintenance cards | Deferred by cost in two prior passes — real batch spend across the whole Friday corpus. Human decides whether to spend it | DEFERRED-TO-HUMAN |
| E7 | CLAUDE | The connection-profile list refresh open question (2026-08-19) | Create a profile out-of-band (API) with the app open, then check the seat dropdown; then activate another tab and come back | PENDING — **one false start already ruled out.** A profile created out-of-band did not appear in the seat dropdown, which looked like the 2026-08-19 report; but `performance.getEntriesByType('navigation')` showed `"navigate"`, not `"reload"` — **the browser pane's `Cmd+Shift+R` never actually reloaded the page**, so the stale list was a stale tab, not a stale cache. After a real `location.reload()` the profile appeared. The open question needs re-testing with the tab-activation gesture specifically, and every negative in this area must first prove the page actually reloaded | PENDING |

---

## Log

(Appended as the walk runs.)
