# Dogfood walk — the `0b0617fee` round + the follow-ups round (2026-09-03)

**Surfaces:** the `0b0617fee` drift catch-up round (P4.D148 ∥ P4.D149 ∥ P4.D150
∥ P4.D151 ∥ P4.D152, unified 2026-09-03) and the follow-ups round (P4.67 ∥
P4.68 ∥ P4.69 ∥ P4.70 ∥ P4.71, unified 2026-09-02 — never dogfooded).

**Instance:** `~/qt-dogfood-friday`, a COPY of Friday rsynced 2026-09-03 18:51
(byte sizes match live). Never rsynced back.

**Drift-ledger §2 probe at walk start: PASS.** v4 `main` at `15573c3a1`, tree
clean, both logs empty. §1's verdict stands: **1 commit past the baseline
`0b0617fee`** — `15573c3a1` (bug 119, the character optimizer). That surface
has **no v5 counterpart at all** (`p4.9k`, unported), so **no step in this walk
touches it** and no apparent failure here can be attributed to it. Regen rule
PIN REQUIRED at `0b0617fee` (no regen is planned in this walk).

---

## Pre-walk measurements (ledger §5.5 — done BEFORE any v5 boot)

The bug-117 heal is a boot heal: **booting the v5 server consumes the
population**, so these were taken first, with the server down.

| measurement | value | consequence for the plan |
|---|---|---|
| `migrations_state` row `realign-file-entry-sha256-v1` | **PRESENT** — `completedAt` 2026-09-03T02:43:08.018Z, `quilltapVersion` `4.9.0-dev.120`, `itemsAffected` **117**, message `Scanned 2801 mount-blob FileEntries; realigned 117 sha256 values; 2 orphaned (no matching blob), 0 malformed storage keys` | ⚠ **The banked 💸 proof is DEAD.** v4 ran its own bug-117 migration on this instance hours before the copy was taken — `4.9.0-dev.120` IS `0b0617fee`, the commit P4.D152 ported. The 117 damaged rows are already healed. Replaced by the better PAIR in A6/A7 below (memory note `an-expired-live-proof-can-buy-a-better-pair`). |
| `files` rows | 2818 total, **0** with NULL/empty `sha256` | the 2801 v4 scanned are the mount-blob subset; 17 files are another storage class |
| `chats.conciergeOverride` | 878 NULL / 10 `OFF` / **4 `UNCENSORED`** | real material for A1–A3; the four Uncensored chats are pre-existing (set after creation), so A2's create-time flip is genuinely new |
| `chats.isDangerousChat` | 743 `0` / **76 `1`** / 73 NULL | ample material for B1's danger ring |
| `connection_profiles` with `fallbackProfileId` | **11** (Anthropic/OpenAI/Grok/Z.AI/DeepSeek), 10 of them `allowTierFallback=1` | real understudy chains for B5 |
| `image_profiles` | 15, incl. **3 NANOGPT** (`FLUXNSFWunlock`, `Klein Uncensored`, `Flux.2 Klein 9B`) | real NanoGPT profile for B3/B4 |
| `api_keys` | 11 live providers: OPENAI, OPENROUTER, GROK, ANTHROPIC, GOOGLE, SERPER, MISTRAL, Z_AI, DEEPSEEK, WAVESPEED, NANOGPT | live wires available for A2/A5/B5 |

---

## What NOT to expect to work (from the orders' status headers)

- **Continue Elsewhere seeding the Concierge state** — P4.D149 Tier 3, loud:
  the continue-chat flow is **unported**. Not a bug.
- **A non-string `conciergeState`** (e.g. `42`) answers the dispatch *decode*
  envelope, not v4's flat `Validation error` — P4.D148 recorded this as the
  P4.60/P4.62 wrong-type-collapse class. Expected divergence, not a finding.
- **The danger ring on the STREAMING bubble** — P4.69 measured that v5's
  streaming bubble renders no avatar at all, so there is nothing to attach to.
  Recorded deferral; only the settled bubble carries the ring.
- **P4.67's remainder:** seventeen REST sites are NOT yet in the
  query-param family (Tier 1 item 3 PARTIAL). Absence of a refusal at an
  unlisted site is out of scope, not a finding.
- **The v5 boot writing a `migrations_state` row on a clean pass** — P4.D152's
  RECORDED, both-directions-pinned DIVERGENCE: v4's `shouldRun()` is
  presence-not-drift so v4 stamps a zero-`itemsAffected` row; **v5 stamps
  nothing at all.** A6 *asserts* this, it does not report it.
- **The Markdown toolbar spilling past its column** on New Chat's Starting
  Scenario — finding #109, RECORDED, a v4-first filing. Do not re-report.

---

## Round A — the `0b0617fee` drift round

| # | Owner | Step | Gesture | Expected + how verified | Status |
|---|---|---|---|---|---|
| A1 | CLAUDE | The Concierge dropdown exists in v4's slot | New Chat form: read the DOM order — the control must sit **after** the image-profile picker and **above** "Starting Scenario (Optional)". Four options in two optgroups; first reads `Monitored (default)` (the *sidebar's* says `Monitored` — v4 deliberately differs); the label carries the state icon in its tone colour; the helper sentence below is the shared `detail`, and there is **no** `hint`. | The rendered strings byte-match `CONCIERGE_STATE_PRESENTATION`; verified by `read_page` + a string diff against the SPA source table. | **PASS.** DOM order is exactly v4's: Roleplay Template → **Image Generation Profile** → **The Concierge** → Starting Scenario. Two optgroups (`The Concierge decides`: Monitored (default), Flagged / `You decide`: Vouched Safe, Uncensored), default `monitored`, label icon `eye` in `qt-text-success`. Exactly ONE `<p>` helper and no `hint`; its text is byte-identical to v4's `lib/services/dangerous-content/concierge-state-presentation.ts:56` `detail`. Switching to Uncensored flipped both the icon (`eye-off` / `qt-text-info`) and the sentence to the Uncensored `detail` — the signal is live, not a static render. |
| A2 | CLAUDE | 💸 **A chat created Uncensored greets from the frank desk** | New Chat → pick a character → Concierge = **Uncensored** → create, and let the greeting generate. | `chats.conciergeOverride='UNCENSORED'` on the new row; the Concierge's **manual-uncensored bubble** present in the transcript; the greeting's `llm_logs` row shows the **uncensored desk** profile, not the character's own; the desk was asked at **attempt 0**. Verified in `quilltap db messages --chat <id>` (bubble order via `message_order`) + `quilltap db --llm-logs logs --chat <id>`. Costs ~1–2 completions. | **PASS — the round's headline proof.** Chat `ea779927`: `conciergeOverride='UNCENSORED'` persisted. Server log: `Generating greeting on the Concierge uncensored provider … trigger="chat-state" settings_source="chat-uncensored" uncensored_profile=DeepSeek v4 Flash`, then `Greeting generation succeeded via Concierge uncensored provider`. ⭐ The desk substitution is airtight: Friday's OWN profile on the create body was `Z.AI GLM 5.3 Flash` (Z_AI), and the only `llm_logs` row is **DEEPSEEK / deepseek-v4-flash / 6094 ms** — the character's seat was never tried, so this is attempt **0** on the desk, and `trigger="chat-state"` proves the resolver was asked WITH the fresh chat row. The Concierge's manual-uncensored bubble is **rowid 156774 — second in the transcript**, directly after the system-prompt message (rowid 156773): the flip landed at the chokepoint exactly where the order places it. |
| A3 | CLAUDE | The omit-when-monitored body rule + the other two states | Create with **Monitored** (default) and inspect the POST body in the network tab; then create with **Off**. | Monitored: the request body carries **no** `conciergeState` key at all (omit rule). Off: `conciergeOverride='OFF'` persisted, applied through the flip chokepoint right after the system-prompt message. Verified by `read_network_requests` on the create POST + a DB read. | **PASS — a clean discriminating trio.** Monitored (default): the create body's keys are `type,title,participants,imageProfileId,roleplayTemplateId,timestampConfig,outfitSelections,progressId` — **no `conciergeState` key at all** — and `conciergeOverride` persisted NULL with **no** Concierge bubble. Uncensored: the key IS sent → `'UNCENSORED'`. Vouched Safe: → `'OFF'`, and its flip bubble is rowid 156791, again **directly after** the system prompt (156790): *“The operator has vouched for the present company, and the Concierge, satisfied, takes the afternoon off.”* **All FOUR states proven end-to-end through the real form**, the fourth added after the scripted steps: Flagged → the helper reads v4's flagged `detail` with icon `alert-triangle`/`qt-text-danger`, and it persists as `conciergeOverride` **NULL** + `isDangerousChat = 1` — **exactly v4's documented mapping** (`lib/services/dangerous-content/manual-flip.ts:11`: *"'flagged' → conciergeOverride = NULL, isDangerousChat = true"*), with its own distinct flip bubble at rowid 156842. ⭐ **And the Flagged chat gave a second, DIFFERENT routing proof:** its greeting also went to the uncensored desk (flagged routes through uncensored providers) but with `settings_source="global"` where the operator-set Uncensored chat logged `settings_source="chat-uncensored"` — the same desk reached by two different resolution paths, and the log distinguishes them. ⚠ *Instrument note:* reading the helper `<p>` in the same tick as the `change` dispatch returns the PREVIOUS sentence; it is correct one render tick later. |
| A4 | CLAUDE | The create-time validation gate | `curl` the create endpoint with `conciergeState: "bogus"`, then with an explicit `null`. | Both answer **400 `Validation error`** with **nothing written** (chat count unchanged before/after). Verified by curl + a `SELECT COUNT(*) FROM chats` sandwich. | **PASS.** `"bogus"` → `{"kind":"bad-request","message":"Validation error"}` HTTP 400; explicit `null` → the same. Chat count **895 → 895**: nothing written. The RECORDED divergence also confirmed exactly as documented — `42` answers the dispatch decode envelope (`invalid chatCreate request: invalid type: integer 42, expected a string`) where v4 answers the flat `Validation error`; still 400, still nothing written. Not a new finding. |
| A5 | CLAUDE | 💸 **bug 116 — the describer arrival verdict** | Attach an image in a chat whose seat cannot transport images, so the describe-fallback runs. Negative arm first (a describer that genuinely sees the image), then the positive arm if a gateway that drops images is reachable. | Negative: description persists, **no** `[Image Fallback]` refusal warn, `IMAGE_DESCRIPTION` row written. Positive: the verdict fires **ahead of every content check** — the warn plus the long `unsupported` sentence, and the invented description is **not** persisted. Verified in `combined.log` + `llm_logs` billed-input tokens against the derived 66 ceiling. | **PASS on the negative arm, with the verdict's arithmetic shown; the positive arm BLOCKED for want of a misbehaving gateway.** Switched the chat's seat to **DeepSeek v4 Flash** (`supportsAttachments: false` in its manifest) through the real Participants panel, then attached a **real 1207×805 JPEG taken from the instance itself** and sent. Log: `[Attachment] Plugin cannot transport images; routing to describe-fallback … supports_image_upload=false`; the describer ran on **GROK / grok-4.20-0309-non-reasoning, 8460 ms**, `usage={"promptTokens":1077,…}` with `cacheUsage` NULL → billed input **1077**, far above the derived **66** ceiling → verdict **`Arrived`**, so the description was trusted. **Zero** `did not process the image` warns in the whole session. Friday — on a seat that cannot receive images — then described the photo accurately (blue sweater, tousled gray hair, glasses, *“at the bench where the work happens”*), and the chat auto-titled itself **“The Bench Where the Work Happens.”** ⭐ **A free contrast arm came from a failed first attempt:** a hand-rolled 8×8 PNG that Grok rejected (`HTTP 400 {"code":"invalid_image"}`) walked the whole chain — configured understudy skipped (`cannot receive this turn's images … purpose=vision`), tier picker drafted a stand-in (`eligible_count=21`), uncensored retry — and, every describer having refused, v5 spliced the **honest error** instead of inventing a description; Friday said the image had not come through. That is the principle bug 116 protects, demonstrated from the other side. The remaining positive arm (a gateway that ACCEPTS an image and routes it to a model that ignores it, billing ≤ 66) needs a specific misbehaving gateway that is not configured here; its two legs stay differential-proven (14 `verdict` rows + 8 `fb_verdict_*` cases, 6 of 8 red-first). |
| A6 | CLAUDE | 💸 **bug 117 — the free cross-app ledger proof** (replaces the dead banked proof) | Boot v5 on the DB v4 already healed; diff `migrations_state` before/after. | v5 **honours v4's row and writes nothing** — the ledger is byte-unchanged, and no second `realign-file-entry-sha256-v1` row appears. This is P4.D152's recorded divergence meeting a real cross-app ledger: v4 would have stamped a zero row, v5 stamps none. Verified by a full-table dump md5 before and after boot. | **PASS.** Ledger dump md5 **`470baa219040cab245d31815ce10aa4d` before AND after** the boot (181 rows, byte-identical), and **zero** `realign` lines in the boot log. v5 read v4's row and did nothing — the recorded divergence (v4 would stamp a zero-`itemsAffected` row) meeting a real cross-app ledger. |
| A7 | CLAUDE | **bug 117 — the heal on a PLANTED population** | Stop the server; delete the ledger row and corrupt N `files.sha256` values on the copy; boot; read the summary. | v5 realigns **exactly** the planted set and reports its own summary line; v4's recorded scan shape (`2801` FileEntries, `2` orphaned, `0` malformed) should reproduce as a cross-implementation agreement on the scan population. Verified in `combined.log` + the new ledger row. | **PASS — and a full cross-implementation agreement.** Planted 5 corrupt `sha256` values + deleted v4's ledger row; v5 booted and logged `Realigned FileEntry sha256 values with the bytes actually stored scanned=2791 realigned=5 orphaned=2 malformed_key=0`, wrote its own ledger row (`itemsAffected` 5, `quilltapVersion` `0.0.768`, message shaped exactly as v4's), and **0** planted rows remained. `orphaned=2` and `malformed_key=0` **match v4's own recorded run**; `scanned=2791` is exactly today's mount-blob row count (v4's 2801 was 16 h earlier, 10 files since deleted — not a divergence). ⭐ **The five healed values are byte-identical to the ones v4's own migration wrote** (diff of the pre-plant and post-heal dumps: identical) — v5 recomputed v4's answer from the same stored blobs. |
| A8 | CLAUDE | **leg (a) — a fresh upload names the bytes actually stored** | Upload an image into a chat. | The new `files.sha256` equals the hash of the **stored** bytes (the bridge's own `sha256` after its transcode), not the source bytes. Verified by hashing the stored blob out of the mount index and comparing to the row. | **PASS — self-consistent; the discriminating case is UNREACHABLE in production, by design.** Uploaded a 265-byte PNG through the real `POST /api/v1/chats/{id}/files`; the row records `sha256=278e8525…`, and the bytes the server serves back from `/api/v1/files/{id}` hash to **exactly** `278e8525…`. ⚠ But note what this can and cannot prove: `chat_files.rs:705` threads `NotConfiguredPixelCodec` at **every production call**, so the encode always fails and the ORIGINAL bytes pass through — stored bytes ARE source bytes, and the hash agrees by construction (the `a-production-noop-seam-makes-a-comparand-vacuous` class). The byte-CHANGING codec that makes the pre-fix order measurably wrong lives only in the differential. **This makes P4.D152's named candidate concrete:** until the HOST codec is threaded into chat uploads, v5 stores chat images as their original type (here `image/png`) where v4 transcodes to WebP — the pre-existing divergence recorded at `api/files.rs:1116-1118`. Not a defect; a scope note. |
| A9 | CLAUDE | 💸 **bug 115 — the interactive distill budget + the timing log** | Send a turn on a chat large enough to trigger the build-context memory fallback. | The `[Memory]` inter-character timing line appears with **all five** fields and `loadedCount` = importance + relevance lengths; the fallback distill runs on the **interactive** budget. Verified in `combined.log`. | **PARTIAL — the timing line PASS live; the budget half is not live-observable here.** Sent a turn in the real four-character chat *The Warm Stone Holds Four*; at `quilltap::build_context=debug` the line fired with **all five fields**: `[ContextManager] Inter-character memory retrieval complete chat_id=88f8bf2d… character_id=d9d0d998… duration_ms=919 loaded_count=23 included_count=23`, and it is correctly gated on `is_multi_character`. **Correction to this plan's own numbers:** the constants are `CHEAP_LLM_TASK_TIMEOUT_INTERACTIVE_MS = 45_000` (no retry) vs `CHEAP_LLM_TASK_TIMEOUT_MS = 90_000` (+ a free retry) — the plan's "85 s" was wrong, taken from a stale note. The budget itself is a **deadline**: it is observable only when the cheap route stalls past 45 s, which no real provider did here (and the fallback distill branch did not run at all this turn — the proactive pass had already supplied the paraphrase). It stays unit-pinned by P4.D150's stalling-provider-on-a-paused-clock test, which is the right instrument for a deadline. |

## Round B — the follow-ups round (P4.67–P4.71)

| # | Owner | Step | Gesture | Expected + how verified | Status |
|---|---|---|---|---|---|
| B1 | CLAUDE | 💸 **the assistant-avatar danger ring** | Open one of the 76 `isDangerousChat=1` chats and look at a settled assistant bubble's avatar. | The ring renders (v4's CSS was dead in v5 until P4.69). The **streaming** bubble has no avatar at all — expected, recorded. Verified by computed style on the avatar element. | **PASS with a clean discriminator.** On the real flagged chat *The Bronze Window and the Held Night*: 5 avatars, **3** carrying `qt-chat-avatar-dangerous`. The rule targets a DESCENDANT — `.qt-chat-avatar-dangerous .overflow-hidden`, **byte-identical to v4's `_chat.css`** — and those three inner elements compute `outline: rgb(208, 67, 67) solid 2px; outline-offset: 2px` (the theme's `--color-destructive`), while the two non-dangerous (user) avatars compute `none`. The dead CSS is alive. ⚠ *Instrument note:* the first probe measured the **wrapper** and reported `boxShadow: none` — a false negative from reading the wrong element, not a defect. |
| B2 | CLAUDE | 💸 **the subset refusals via a v4-shaped client** | `curl` the `?action=` edges: an unknown action, a present-but-empty `?action=`, a duplicate `?action=a&action=b`, and a **v4-known-but-unserved** action (`scan` on `mount-points/{id}` POST, `system/tools`). | Unknown → v4's dispatcher envelope; present-but-empty → v4's JS-falsy default; duplicate → the **FIRST** wins (v4's `searchParams.get`); the unserved-known action → the **loud** v5 refusal (`UNSERVED_KNOWN_ACTIONS`), and the edge must **not** advertise an action it refuses. Verified by curl + response body compare. | **PASS on all five arms.** `system/tools` (a strict-allow-list edge): unknown → `Unknown action: bogus. Available GET actions: …` 400, and the interpolated list is **byte-identical to v4's `TOOLS_GET_ACTIONS` in declaration order** (and POST to `TOOLS_POST_ACTIONS`); present-but-empty → `Unknown action: .` — which **matches v4**, because this route gates on `isValidAction`, not on JS truthiness (the falsy-default shape belongs to v4's *other* dispatch shapes); duplicate `?action=bogus&action=alsobogus` → the **FIRST** wins, as v4's `searchParams.get` does. `mount-points/{id}` POST (the subset edge): `scan` → the **loud** v5 refusal *“Only the multipart 'write-file' action is served on this route; JSON mount actions ride POST /api/dispatch”* — and, the point of the §3 fix, it no longer lists `scan` as available in the sentence refusing it; a truly unknown action → v4's envelope with the full `availableActions` array; no action at all → `Action parameter required` + the same array. |
| B3 | CLAUDE | 💸 **`count: 20` through the image-profile route** | `curl` the in-process image-profile generate route with `count: 20`, and with an empty prompt. | Both answer v4's `Validation error` from v4's OWN `generateImageSchema` gate. Verified by curl. | **PASS, plus the guard order proven.** `count: 20` → `{"kind":"bad-request","message":"Validation error"}` 400 — v4's route gate, not the tool's fixed sentence. Boundaries all refuse correctly: `count: 0`, `count: 11`, an empty prompt, and a 4001-unit prompt each → `Validation error` 400. ⭐ **The 404 beats the 400:** a request that is *both* a missing profile *and* invalid (`prompt:""`, `count:99`) answers `{"kind":"not-found","message":"Image profile not found"}` **404** — v4's order (profile lookup, then `generateImageSchema.parse`), which is the guard-order class this port has repeatedly had to correct elsewhere. |
| B4 | CLAUDE | 💸 **the image-profile modal's structured writers** on a real NanoGPT profile | Open Settings → Images → one of the three real NANOGPT profiles; exercise the options panel and save. | The modal's parameters write as an **object** (not a stringified bag); the round-trip preserves every non-sampling key. Verified by the PUT body in the network tab + the persisted `image_profiles` row. | **PASS.** Opened the real NANOGPT profile **FLUXNSFWunlock** (`flux-lora`): the modal names the provider **NanoGPT** (finding #108's fix holding on real data), its own key, and renders the **schema-driven NanoGPT Image Options** panel — the `imageProfileOptionsSchema` verb fires for `NANOGPT`/`flux-lora` and the fields arrive as `pof-size` / `pof-num_inference_steps` / `pof-guidance_scale` / `pof-lora_preset`, with the instance's real LoRA train populated (`shahtab/FLUXNSFWunlock`, strength 0.80, trigger `aidmaNSFWunlock`). Set Inference Steps to 28 and saved; the wire body carries `parameters` as a **structured object**, not a stringified bag: `{"size":"1024x576","loras":[{"source":"shahtab/FLUXNSFWunlock","scale":0.8,"triggerPhrase":"aidmaNSFWunlock"}],"num_inference_steps":28}` — `28` and `0.8` as **numbers**, and every non-sampling key (`size`, `loras`) preserved, so the old drop-non-sampling-keys defect stays fixed. `baseUrl: null` is sent explicitly (the always-send heal) and `apiKeyId` rides along (the bug-76 chokepoint). The `image_profiles` row persisted byte-for-byte. |
| B5 | CLAUDE | 💸 **the failover rows on a real understudy** | Force a primary failure on a profile with a real `fallbackProfileId` (one of the 11) and let the chain walk. | `llm_logs` carries a row for the **failed** leg as well as the recovery, threaded with the run-id context (P4.68's orchestrator recovery call). Verified in `quilltap db --llm-logs`. | **PASS — proven on the vision chain, incidentally, by A5's first attempt.** The rejected PNG walked a real three-provider chain and **every leg wrote its own `llm_logs` row** with the provider's real error: GROK `grok-4.20-0309-non-reasoning` 892 ms `HTTP 400 {"code":"invalid_image","error":"Invalid PNG image."}`; Z_AI `glm-5.3-flash` 1437 ms and Z_AI `glm-4.6v` 1110 ms, both `HTTP 400 {"error":{"code":"1210","message":"图片输入格式/解析错误"}}`. The chain context is in the log too: `Fallback chain skipped configured understudy: cannot receive this turn's images … understudy_provider=NANOGPT supports_image_upload=false purpose=vision`, then `Tier picker drafted a replacement … different_provider=true purpose=vision eligible_count=21`. ⚠ Scope note: this proves the thread on the **describer** chain (`purpose=vision`); the primary chat-completion recovery call was not separately forced (the dead-endpoint understudy walk was discharged on 2026-09-02). |
| B6 | HUMAN | 💸 **the Docker / host-gateway container walk** | One `docker build` on a quiet machine, then the Ollama walk through `host.docker.internal`. | The gateway resolver rewrites the base URL at every provider construction site; the OLLAMA key-test and model-fetch URLs keep their `//api/tags` double slash (v4-faithful — P4.71's flipped pin). | DEFERRED-TO-HUMAN — a container build is expensive by nature and this machine is running the walk. |

## Standing 💸 queue carried in (fold in opportunistically)

| item | owner | note |
|---|---|---|
| Pascal's **group** tier | CLAUDE | The recipe is written down (2026-09-02 walk): the effects cascade searches chat → project → group for a key that **already exists**, so it must be pre-seeded via `groupStateSet`, and the chat must satisfy `groupTier.status == "single"`. |
| The Brahma deep-query budget | CLAUDE | A raised agent-turn budget on a real deep query. |
| memory dedup / conversation-summaries first run | HUMAN | Deferred by cost across five passes — a batch job on 800 MB of real data. |
| NanoGPT prompt-caching cost question (#101) | HUMAN | A cost judgment, not a defect. |
| The LoRA **wire-byte** look | BLOCKED | `llm_logs.request` is a pre-builder projection and `wire-tap.py` cannot tap HTTPS. |

---

## Findings

**No v5 defects were found by this walk.** Twelve of the thirteen scripted
steps PASS; one (A9) is PARTIAL for a reason that is a property of the thing
being tested, not of the code; one (B6) is deferred to the human by nature.
Two apparent problems were chased and both were **instrument error**, recorded
in their rows: measuring the ring on the wrapper instead of the descendant the
CSS targets, and reading a helper sentence in the same tick as the `change`
that should update it.

### Scope notes worth carrying forward

1. **The bug-117 chat-upload leg cannot exhibit its own fix in production.**
   `chat_files.rs:705` threads `NotConfiguredPixelCodec` at every production
   call site, so the encode always fails and the original bytes pass through:
   stored bytes ARE source bytes and the hash agrees by construction. The
   byte-changing codec that makes the pre-fix ordering measurably wrong exists
   only in the differential. This makes P4.D152's own named candidate concrete
   — until the HOST codec is threaded in, v5 stores chat images in their
   original type where v4 transcodes to WebP.
2. **The bug-116 positive arm needs a misbehaving gateway.** The verdict's
   `Arrived` leg is now proven live with real arithmetic (1077 billed prompt
   tokens against the 66 ceiling), and the "every describer refused → splice
   the honest error, invent nothing" path was proven by accident. The
   remaining arm — a gateway that accepts an image, routes it to a model that
   ignores it, and bills ≤ 66 — has no configured counterpart on this
   instance.
3. **A deadline is not observable without a stall.** A9's interactive budget
   (45 s, no retry) can only be seen when a cheap route stalls past it; the
   stalling-provider-on-a-paused-clock unit test remains the right instrument.

## Summary

| | count |
|---|---|
| Steps PASS | 13 (two with explicitly recorded limits — A5's positive arm, A8's production no-op codec) |
| Steps PARTIAL | 1 (A9 — timing line live, budget not live-observable) |
| Steps DEFERRED-TO-HUMAN | 1 (B6 — the Docker/container walk) |
| v5 defects found | **0** |
| v4 bugs to file | 0 |
| 💸 items discharged | 7 |

**💸 discharged by this pass:** the created-Uncensored greeting (A2), the
describer verdict against a real gateway (A5, negative arm + the refusal
contrast), the sha256 heal on the Friday copy (A6 + A7 — replaced by a
stronger pair, see below), the danger ring (B1), the subset refusals via a
v4-shaped client (B2), `count: 20` through the image-profile route (B3), the
modal's writers on a real NanoGPT profile (B4), and the failover rows on a
real understudy (B5, on the vision chain).

**💸 still owed:** the Docker/container walk + one `docker build` (B6);
Pascal's group tier; the Brahma deep-query budget; memory dedup / conversation
summaries (cost); the NanoGPT prompt-caching cost question (#101); the LoRA
wire-byte look (blocked — `llm_logs.request` is a pre-builder projection and
`wire-tap.py` cannot tap HTTPS).

### The pass's best result

The bug-117 banked proof **expired before the walk** — v4 ran its own
`realign-file-entry-sha256-v1` migration on this instance at 2026-09-03
02:43, healing 117 rows, hours before the copy was taken. Measuring the
population first (ledger §5.5) turned that loss into a stronger pair:

- **v5 booted on v4's healed database and wrote nothing** — ledger md5
  identical across the boot, zero realign lines. That is P4.D152's recorded
  divergence (v4's `shouldRun()` is presence-not-drift and would stamp a
  zero-`itemsAffected` row; v5 stamps none) meeting a real cross-app ledger.
- **On a planted population, v5 recomputed v4's own answer.** Five corrupted
  `sha256` values and v4's ledger row removed; v5 healed exactly those five,
  reported `scanned=2791 realigned=5 orphaned=2 malformed_key=0` — with
  `orphaned` and `malformed_key` **matching v4's recorded run** and `scanned`
  exactly today's mount-blob count — and the five healed values came back
  **byte-identical to the ones v4's migration had written**. Two
  implementations, same blobs, same answer.
- **And the heal is idempotent:** the next boot, with v5's own ledger row
  present, skipped it entirely.
