# Dogfood walk — the `a14a1811` vision round + the `0ba942b1` drift round + the standing 💸 queue — 2026-08-24

**Instance:** a COPY of Friday at `~/qt-dogfood-friday` (never the live iCloud tree).
Data refreshed 2026-08-24 09:41 straight from `~/iCloud/Quilltap/Friday/data/` (v4 not
running at copy time; sizes + mtimes verified identical to the source).
**Server:** `./target/release/quilltap-web --data-dir ~/qt-dogfood-friday --spa-dir apps/web/dist/quilltap/browser`, `RUST_BACKTRACE=1`, log in the scratchpad.
**Findings log:** `docs/developer/porting/dogfood-findings.md` — next finding number is **#101**.
**Unlock:** expected none — prior passes measured `hasUserPassphrase: false`. Confirm before Part A.

**Human's standing instruction for this pass:** *spend NanoGPT freely (there is a
subscription), but stay on open-weight models — DeepSeek, GLM, Qwen — unless the thing
under test is inherently an OpenAI/Anthropic behaviour.* Prompt caching is the one
explicit exception (NanoGPT's `promptCaching` body key exists for Claude-routed models;
its OpenAI/Gemini routes cache on their own).

## What this pass is for

Two rounds have unified since the 2026-08-22 walk, and the first of them is the
largest un-walked surface the port has: **vision**.

- **`a14a1811`** (2026-08-23, five lanes) — P4.D106 the image-transport predicate +
  the ten-literal moderation finish-reason table + the three-tier attachment anchor;
  P4.D107 NanoGPT plugin 1.1.0 (`image_url` serialisation — the fifth image-sending
  provider); P4.D108 the `describe_image` looking verb end-to-end **with the vision
  tier wired LIVE into production at unification** (⚠ one vision-LLM call per
  `describe_image` on an undescribed image, on every tool path); P4.D109 the
  attachment-failure warning toast; P4.57 tri-state decode-once.
- **`0ba942b1`** (2026-08-23) — P4.D110 the title-verdict parser (v4 bug 96: a
  misspelled key stops silencing the auto-titler) + the checkpoint-burned warn;
  P4.D111 the bug-97 OpenRouter convergence (**OpenRouter now declares the vision
  path it implements** — v5's manifest flipped, so OpenRouter vision profiles stop
  routing to the describe-fallback); P4.58 corpus-only (no v5 source change).

Plus the standing 💸 queue carried from four prior passes.

**Local advantage:** ollama is up — `qwen3.5-9b-q6:latest` and
`hf.co/unsloth/Qwen3.6-35B-A3B-GGUF:UD-Q5_K_XL` (tools), plus `nomic-embed-text`.
Ollama is a **non-transporting** provider, which makes it the natural test bed for
the describe-fallback arm.

**Primary verification channels:**
- `./target/release/quilltap db --llm-logs --data-dir ~/qt-dogfood-friday --json "…"`
  (`llm_logs`, call-type column is `type`) — but note `llm_logs.request` is a
  **pre-builder projection**: it cannot show provider body keys or the leading-system
  fold. For body bytes use `~/qt-dogfood-friday/wire-tap.py`.
- `~/qt-dogfood-friday/logs/combined.log` — P4.49's file appender, where the new
  P4.D110 warn lines and the P4.D106 moderation warn payload land.
- The server's own stderr log in the scratchpad.

## What NOT to expect to work (do not file these)

- **DeepSeek / OpenAI-Compatible / Ollama cannot send images.** v4 still cannot
  either (the fix is an unpublished `plugin-utils` change) — their drop constants are
  live and wired. An image on those providers routes to the describe-fallback, or is
  dropped with a named sentence. That is the correct behaviour this round.
- **Web search is dark on a real instance** — finding #98, the standing P4.42
  plugin-registry deferral. Only `SERPER_API_KEY` in the environment works.
- **`embeddingProfileFetchModels`** answers a named loud refusal (P4.9H2A).
- The upload-time fire-and-forget `autoDescribeChatImageAttachment` is still a named
  no-op — describe-on-upload does not happen; `describe_image` as a TOOL is what
  landed.
- The sha256-sister `?? sisters[0]` fallback arm is unreachable by natural gestures.
- The help-doc chunk backfill / section-led `help_search` (`p4.9i2`), subsystem
  backgrounds other than a project story background, `?msg=` anchors, `/photos?tag=`
  filters — named deferrals.
- Enter in composition mode does not send — v4's contract.

---

## Part A — the vision tier: `describe_image` live (P4.D108)

| # | Owner | Step | Gesture | Expected + how verified | Status |
|---|---|---|---|---|---|
| A1 | CLAUDE | The tool is advertised and the catalog carries it | `/api/v1/tools` inventory + a chat's built slate | **PASS.** `GET /api/v1/tools` returns **41** tools with `Describe Image` (`id: describe_image`) present — the count matching P4.D108's `BUILT_IN_TOOLS.len() == 41` tripwire exactly | PASS |
| A2 | CLAUDE | **Tier 1 — an already-described image is free** | Ask a tools-capable seat to `describe_image` on a photo that already has a description | **PASS.** Driven through the production **Run Tool** path (`chatRunTool` — one of the four tool paths the §4 wire covers), on a real story-background image: `source: "stored-description"`, instant, and the `IMAGE_DESCRIPTION` count in `llm_logs` did not move | PASS |
| A3 | CLAUDE | **Tier 3 — the live vision call (the round's headline)** | The same seat, on an image with no description | **PASS — the vision tier's first live run on real data.** `describe_image` on the freshly uploaded, undescribed test PNG returned `source: "vision-call"` in **6,996 ms** with an accurate description (*"a solid, deep navy blue horizontal band … Three identical, solid royal blue equilateral triangles … evenly spaced"*). A real `IMAGE_DESCRIPTION` row landed — **GROK / grok-4.20-0309-non-reasoning**, the instance's configured describer — with a measured `durationMs`, and the 2,708-char description **persisted onto the file row** (`auto-describe: completed … links_updated=1`). **The three tiers were then proven by a state transition, not by assertion**: re-running the same uuid answered `source: "stored-description"` with `IMAGE_DESCRIPTION` rows 7 → 7. Before the §4 wire this path answered `(describe-failed)` everywhere | PASS |
| A4 | CLAUDE | …and the `no-bytes` starvation does NOT reproduce | Same run | **PASS.** No `no-bytes` anywhere: the photo-bytes half of the wire is live, and the §3 review's structural-unreachability catch does not reproduce on a real instance | PASS |
| A5 | CLAUDE | Tier 2 — the prompt-only arm | An image with a generation prompt but no description | **PASS.** A generated image with a prompt and no description answered `source: "generation-prompt"` with the 1,851-char crafting prompt, no vision call | PASS |

## Part B — the NanoGPT vision wire (P4.D107) + the transport predicate (P4.D106/P4.D111)

| # | Owner | Step | Gesture | Expected + how verified | Status |
|---|---|---|---|---|---|
| B1 | CLAUDE | **💸 The live NanoGPT vision send** | A real image attached to a chat message on a NanoGPT vision-capable open-weight profile (GLM-V / Qwen-VL class) | **PASS — proven twice, on the wire and in the answer.** The composer attach → upload → send path carried a purpose-drawn 2,249-byte PNG (3 blue triangles, a red circle, a yellow square, a dark band). (a) **Wire bytes**, captured with a new structural tap (`scratchpad/vision-tap.py` — `harness/tools/wire-tap.py` collapses `messages` to a count and cannot see this): the anchored user message is `content: LIST parts=[text, image_url]` with `{"type":"image_url","image_url":{"url":"data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAA…"}}`, 3,000 b64 chars — the plugin-1.1.0 serialisation. (b) **Semantics**: `zai-org/glm-4.6v` answered *"There are three triangles. They are blue. The circle is red. The square is yellow."* — every fact correct, and unavailable to a text-only send | PASS |
| B2 | CLAUDE | The manifest flip is live | The NANOGPT transport predicate on a real send | **PASS.** No describe-fallback, no drop sentence, no `attachmentResults` warning: the bytes rode as themselves. The NANOGPT manifest flip is live in production | PASS |
| B3 | CLAUDE | **The describe-fallback arm** (bug 91) | The same image on an **Ollama** profile with the vision box ticked | **PASS — proven in the log AND on the wire.** Amy's seat moved to the OLLAMA `Qwen3.6-35B` profile, whose vision box is ticked though the plugin cannot transport. The server logged bug 91's exact line — *"[Attachment] Profile claims image support but its plugin cannot transport images; routing to describe-fallback"* with `provider="OLLAMA"` — and the tapped Ollama request contains **zero `image_url` parts**; the human's turn instead reads `[Charlie] [Image: dogfood-vision-test2.png]\n\nImage Description (generated by AI): …`, a 9,456-char message carrying a GROK-produced description that is itself correct (*"dark forest green horizontal rectangle … a bright orange rectangle … Two Circles"*). ⓘ Riders: the Ollama body's top-level keys are `model, stream, think, options, keep_alive, tools` — P4.D78's thinking wire live on a real request; and the fallback describes for the send **without** persisting onto the file row, which is correct (persisting is the still-deferred upload-time `autoDescribeChatImageAttachment`) | PASS |
| B4 | CLAUDE | **💸 The attachment-failure toast** (P4.D109) | An unsupported MIME attached on a NanoGPT seat | **PASS — the toast fires with v4's exact sentence.** A 246-byte `image/bmp` (a MIME NanoGPT does not forward) was attached and sent on the NanoGPT seat. The done frame carries `attachmentResults: {failed: [{id, error: "Unsupported file type: image/bmp. NanoGPT forwards images only (image/jpeg, image/png, image/gif, image/webp)."}], sent: []}` (read by hooking `JSON.parse`, which catches the frame whatever the transport), and the toast stack showed **`An attachment was not sent to the model: Unsupported file type: image/bmp. …`** with `qt-toast qt-toast-warning`. The model duly said *"I don't see a picture attached"* — the user-facing symptom this toast exists to explain. ⚠ **Instrument lesson, the inverse of finding #99's:** a snapshot taken 11 s before the done frame showed no toast and would have read as a defect; the accumulating `MutationObserver` (whose own hit counter was checked live) is what caught it | PASS |
| B5 | CLAUDE | **The bug-97 convergence, live** | OpenRouter's transport predicate after v4's fix | **PASS — the bug-97 convergence is live in production.** A throwaway OPENROUTER profile with the vision box ticked was pointed at the tap and sent the test PNG: the request carries `content: LIST parts=[text, image_url]` with the data URL, and **no** `routing to describe-fallback` line was logged (the count stayed at the single Ollama one from B3). Before v4's `0ba942b1` fix, production read `supportsAttachments: false` and this send would have gone to the describer instead | PASS |
| B6 | CLAUDE | **💸 The whisper-tailed regenerate** (bug 95) | A chat where the human's message carries an image and a staff whisper trails it → regenerate | **NOT REACHABLE by this gesture — and the 💸 item's premise is corrected.** A regenerate WAS driven on exactly the bug-95 shape (the human's image-bearing turn with a Host whisper trailing it) and the tapped non-streaming body (`stream:false`, the `regenerate_swipe` funnel) carries **no attachments at all** — because **v4 does not re-send them either**: `regenerate-swipe.service.ts:112-132` calls `buildMessageContext` with `newUserMessage: undefined` and `[]` for the file ids, so `attachmentsToSend` is empty; `context-builder.service.ts:961` only ever stamps the anchor from `mergedAttachmentsToSend`. v5 matches. The one shape where a regenerate *does* carry attachments is a **Lantern**-bearing chat (v4 `:931` merges `lanternAttachmentsToKeep`) — that is the setup a future pass needs. The anchor placement itself stays pinned at the unit and wire tiers (P4.D106 unit 5) | BLOCKED(v4 sends none either) |

## Part C — moderation refusals + the empty-response sentence (P4.D106 bug 93)

| # | Owner | Step | Gesture | Expected + how verified | Status |
|---|---|---|---|---|---|
| C1 | CLAUDE | **💸 A real moderation refusal names itself** | Provoke a provider content filter (Z.AI is the recorded shape) | **PASS — both arms, and the mechanism corrected an assumption on the way.** Driven end-to-end on the real instance **without composing anything a provider would have to refuse**: a throwaway Z_AI profile pointed at a purpose-written stub (`scratchpad/refusal-stub.py`) that answers every `/chat/completions` with an empty stream whose final chunk carries `finish_reason`. **(a) DETECT_ONLY**: the toast reads v4's sentence byte-for-byte — *"Z_AI glm-5v-turbo refused this turn on content grounds — it reported `finish_reason: sensitive` and returned nothing. This is the provider's own moderation layer, not a Quilltap error and not a transient fault: resending the same content will be refused again. Route the chat to an uncensored provider (Concierge settings), or change what is being asked for."* — provider and model interpolated, the raw reason in backticks, no suffix. **(b) AUTO_ROUTE with the uncensored route ALSO pointed at the stub**: the same sentence plus *"An uncensored provider was tried as well and also returned empty."* The `combined.log`/server warn carries the full P4.D106 payload both times — `uncensored_retry_attempted`, `same_provider_retry_attempted`, `content_was_flagged_dangerous`, **`danger_mode`** (the §3 review's added key), **`finish_reason="sensitive"`**, **`moderation_refusal=true`**, `provider`, `model`. Also proven: the `content_filter` literal recognises identically. ⓘ **The first attempt looked like a defect and was not**: on this instance's real AUTO_ROUTE settings the uncensored reroute *succeeded*, so the turn was never empty and no sentence was owed — correct behaviour, and the reason the stub had to own both legs before the sentence could be seen | PASS |

## Part D — the `0ba942b1` round (P4.D110 bug 96)

| # | Owner | Step | Gesture | Expected + how verified | Status |
|---|---|---|---|---|---|
| D1 | CLAUDE | The auto-titler runs and titles a new chat | A fresh chat, one exchange, then the title job | **PASS, repeatedly, on real data.** The walk chat was auto-titled four times as it grew — *Triangles in Blue, Circle Red* → *Model Shifts and Memory Anchors* → *The Third Picture in the Old Format* — each from a `TITLE_GENERATION` row on the cheap LLM (NANOGPT `deepseek/deepseek-v4-flash-latest`), including one `{"needsNewTitle": false, "reason": …}` decline that correctly left the title alone | PASS |
| D2 | CLAUDE | **The near-miss key tolerance** | Drive the parser with a model that answers a misspelled/cased key — a small local model is the natural source | **PASS — bug 96's tolerant parser proven live, deterministically.** The cheap-LLM profile was pointed at a stub (`scratchpad/title-stub.py`) returning `{"needsNewTitle": true, "reason": "the stub says so", "Suggested_Title": "A Misspelled Key Still Titles"}` — a key that differs from the canonical `suggestedTitle` in BOTH case and separator, so only the fold pass can find it. The chat's `lastRenameCheckInterchange` was at 7, so one more exchange crossed the next checkpoint (10), the TITLE_UPDATE job ran, and **the chat is now titled `A Misspelled Key Still Titles`**. Pre-`0ba942b1` this verdict was silently discarded. ⓘ A first attempt used `chatRegenerateTitle` and got the raw JSON stored as the title — **not a defect**: that action runs v4's `titleChat`, a plain-text title generator with no verdict parsing (`app/api/v1/chats/[id]/actions/title.ts:11`), so a JSON body IS the title there. v5 is faithful; the test was pointed at the wrong task | PASS |
| D3 | CLAUDE | The new warn lines are readable after the fact | `combined.log` | **PASS.** `combined.log` carries the warn in v4's JSON shape with every key: `{"level":"warn","message":"[Title Verdict] Title arrived under a non-canonical key","context":{"context":"cheap-llm-tasks.title-verdict","task_label":"consider-title-update","chat_id":"…","actual_key":"Suggested_Title","expected_key":"suggestedTitle"}}` — the per-site task label included. This is the line that makes P4.D110's recovery visible after the fact instead of silent | PASS |

## Part E — the standing 💸 queue

| # | Owner | Step | Gesture | Expected + how verified | Status |
|---|---|---|---|---|---|
| E1 | CLAUDE | **💸 Bug 84 — a failed tool call's real sentence reaches the UI** (P4.D99, never run live) | Force a `generate_image` failure on a tools-capable seat | **PASS — bug 84's fix proven live on a streamed turn.** A real model-emitted `generate_image` call failed inside a live turn and the toast read **`Image generation failed: Image generation is not enabled for this chat`** — the provider's own sentence, lifted from the `toolResult.error` **sibling** of a null `result`, with the executor's `Error: ` prefix stripped. Before P4.D99 this same frame produced `Image generation failed: Unknown error`. ⓘ Two riders: the same frame shape was confirmed independently through Run Tool (`success:false, result:null, error:"Image generation is not enabled for this chat"`), and getting there needed an OOC instruction — asked in character, Amy simply **refused to call the tool** (*"No." … "you've run four refusal probes at me"*), which is its own small proof that the seat was reasoning rather than pattern-matching. The composer NOTICE (the second render site) self-clears in ~6 s and was not caught in the DOM snapshot; it reads the same resolved string as the toast by construction and is spec-pinned | PASS |
| E2 | CLAUDE | **💸 The candid story-background prompt** (P4.D94) | Trigger a `STORY_BACKGROUND_GENERATION`; read the `IMAGE_PROMPT_CRAFTING` row's prompt bytes | **PASS on the concealed arm, with an exact byte match.** A `TITLE_UPDATE` at the interchange-10 checkpoint queued a real `STORY_BACKGROUND_GENERATION`, whose `IMAGE_PROMPT_CRAFTING` row carries a system prompt of **5,114 characters** — the exact length P4.D94 recorded for the concealed variant — containing the `DEPICTING INTIMATE OR UNCLOTHED STATES` section with the drapery/framing/occlusion/lighting/pose/environment techniques. So the crafter selected, and selected the concealing variant, on a real call. The **candid** arm needs the target image profile to be dangerous-compatible and is not reachable without rerouting real image spend — carried forward | PASS |
| E3 | CLAUDE | **💸 The live NanoGPT prompt-caching smoke** (P4.D105 D3) | A Claude-routed NanoGPT model (the one sanctioned non-open-weight — caching is an Anthropic behaviour), two turns of one long conversation | **PASS on the wire and the ledger; the cache READ did not materialise, and that is NanoGPT's side of it.** A throwaway NANOGPT profile on `anthropic/claude-haiku-latest` (the sanctioned non-open-weight — caching is an Anthropic behaviour) with `enablePromptCaching: true, cacheTTL: "5m"` ran three real turns on the walk chat's ~19K-token context. Every turn came back with **`cache_creation_input_tokens` non-zero** (18,618 → 19,213 → 20,352) and `cache_read_input_tokens: 0`; `llm_logs.cacheUsage` normalised to `{"cacheCreationInputTokens": N}` with the read key **omitted** rather than zeroed — the `?? undefined` omission P4.D105 pinned — and `rawProviderUsage` carries the whole unmangled bag. **A cache is being written, which only happens when the flag reaches Anthropic**, so the P4.D105 wire is proven live. ⚠ **Worth the human's eye, and v4-faithful:** the three requests' system blocks are **byte-identical (same sha256, 3 blocks each)**, so a cache read was available and never happened — NanoGPT places the breakpoint at the end of the array, so a Quilltap chat pays the 1.25× write premium every turn and reads nothing back. v4 sends the same body, so this is not a v5 defect; it is a candidate upstream question | PASS |
| E4 | CLAUDE | **💸 Google Fetch Models on a real key** (finding #91) | Settings → Providers → a GOOGLE profile → Fetch Models | **PASS — finding #91's 💸 proof discharged.** `modelFetch` on GOOGLE with the instance's real key returns **37 models** — `gemini-2.5-flash`, `gemini-3.1-pro-preview`, `gemini-3-pro-image`, `nano-banana-pro-preview`, `gemma-4-31b-it` … — the account's live catalogue, not the 8-id `GOOGLE_FALLBACK_MODELS`. The `supportedGenerationMethods` read is correct against a real REST body | PASS |
| E5 | CLAUDE | **💸 Pascal side effects — the other three write paths** (P4.D35) | The three write paths left unit-proven only | **DEFERRED — real setup, no shortcut.** The three unproven write paths need a custom tool whose `effects` target a project / group / chat-state write, plus a seat willing to run it. The character-vault path was proven live on 2026-08-21; this needs its own focused sitting | DEFERRED |
| E6 | CLAUDE | **💸 The raised Brahma budget on a deep query** (P4.D57 D7) | A Brahma run that used to exhaust at 25 agent turns | **DEFERRED-TO-HUMAN — expensive by nature and non-deterministic.** Proving a raised budget *binds* means a Brahma run that genuinely exhausts 25 agent turns, which is a long multi-turn agent session on real spend. The wire half (read/write/bounds/explicit-null) was proven on 2026-08-22 (D6) | DEFERRED-TO-HUMAN |
| E7 | HUMAN | 💸 The memory-dedup + conversation-summaries first run | Settings → the maintenance cards | Real batch spend across the whole Friday corpus — the human's call | PENDING |
| E8 | CLAUDE | The Almanack `Free Memory: 0 B` finding (#94) | Raise it, with the measurement already done | **RAISED, with the measurement already in hand.** Finding #94 stands: `almanack_services.rs:163` hardcodes `free_memory_bytes: 0.0` and the report reads `Free Memory: 0 B`, which a reader cannot distinguish from "this box is out of memory". The module header argues no portable read exists; **that premise is false by the file's own technique** — macOS `vm_stat` (`Pages free` × page size, the same `std::process::Command` shape already used for `sysctl hw.memsize`) and Linux `MemFree` in the `/proc/meminfo` the function already parses. ~15 lines, two `#[cfg]` arms, plus a unit test. It overrides a recorded lane decision, so it wants the human's nod — see §Human remainder | DEFERRED-TO-HUMAN |

---

## The human remainder

| # | Ask | Why it needs you | Setup already done |
|---|---|---|---|
| E7 | The memory-dedup + conversation-summaries first run (Settings → the maintenance cards) | Real batch spend across the whole Friday corpus | Cards are live (P4.43); nothing to prepare |
| E6 | A Brahma query deep enough to have exhausted the old 25-turn cap, with the budget raised | Expensive and non-deterministic; the wire half is already proven | Budget setting reads/writes correctly |
| E8 | **A ruling on finding #94** — should `almanack_services.rs` compute Free Memory (macOS `vm_stat`, Linux `/proc/meminfo` `MemFree`) instead of reporting a hardcoded `0`? | It overrides a recorded lane decision, on the port's own `evidence-conditional-rulings` licence | The measurement is done and in the finding; the patch is ~15 lines + a unit test |
| E3-rider | **A view on NanoGPT prompt caching** — every turn writes a cache at 1.25× and never reads one, though the system blocks are byte-identical turn to turn | The wire is correct and v4-identical; the question is whether the feature earns its premium as NanoGPT places its breakpoint | Measured over three real turns; numbers in E3 |
| E5 | Pascal side effects on the other three write paths | Needs a purpose-built custom tool; a focused sitting rather than a walk row | The character-vault path was proven live 2026-08-21 |

## Log

- **09:41** — data refreshed from live Friday (v4 not running; sizes + mtimes verified identical).
- **09:43** — server up on `127.0.0.1:3000`, `unlockState = {resolved, hasUserPassphrase:false}`; no human unlock needed.
- **09:48–09:53** — a new chat created through the New Chat screen (Amy on `NANOGPT/zai-org/glm-4.6v`), the Green Room dressing all five slots incl. HAIR. **The greeting's first attempt hit `durationMs = 300005` — exactly P4.D42's 300 s request bound — and the automatic retry succeeded in 17 s**, an unplanned live proof of the bounded-request work.
- **09:56–10:14** — Part A + B: three `describe_image` tiers, the NanoGPT vision send (tap + answer), the Ollama describe-fallback, the OpenRouter predicate, the attachment-failure toast.
- **10:19–10:32** — Part C: the moderation-refusal sentence, both arms, via a purpose-written empty-stream stub.
- **10:34–10:42** — Part D: the title-verdict near-miss key, via a second stub.
- **10:45–10:52** — Part E: the story-background crafter, NanoGPT prompt caching, Google Fetch Models, bug 84's real sentence.
- **Throughout** — `RUST_BACKTRACE=1`, **zero panics** across ~2 hours against the real 800 MB instance. Every throwaway profile deleted; every base URL, the cheap-LLM profile, and `dangerousContentSettings` restored to their real values (verified by query).

## Tools written for this pass (kept in the scratchpad, worth promoting)

- **`vision-tap.py`** — a structural TCP tap. `harness/tools/wire-tap.py` collapses `messages` to a count, which is exactly the field a vision check needs; this one walks every message, prints each content-part shape, and elides only the base64 payload of a `data:` URL. It is what made B1/B3/B5 byte-provable.
- **`refusal-stub.py`** — an OpenAI-compatible stub that answers with an EMPTY stream whose final chunk carries a chosen `finish_reason`. It drives the bug-93 moderation path end-to-end **without composing anything a real provider would have to refuse**.
- **`title-stub.py`** — an OpenAI-compatible stub returning a canned assistant message (SSE or JSON per `stream`). It drives the bug-96 near-miss key path deterministically.

All three are general-purpose: any future walk that needs a provider to behave a particular way can point a throwaway profile at one.
