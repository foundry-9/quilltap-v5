# Dogfood walk — the `97b25fc53` follow-ups round: host-zone dates, the
# upload describe, per-leg rows, mid-stream frames, the wardrobe avatar
# seam, the memory badge (2026-09-30)

**Instance:** `~/qt-dogfood-friday` — a COPY of real Friday data (refresh
time recorded at §0.5). Disposable; never rsynced back.
**Server:** `RUST_BACKTRACE=1 RUST_LOG='info,quilltap::=debug,quilltap_core::tools=debug'
./target/release/quilltap-web --data-dir ~/qt-dogfood-friday --spa-dir
apps/web/dist/quilltap/browser`.
**Log of record:** the scratchpad server log + `~/qt-dogfood-friday/logs/combined.log`.
**Instrument:** `harness/tools/refusal-server.py` on `127.0.0.1:8898` — grown
this walk by four modes: `midframe`, `midframe-late`, `midframe-uncoded`
(P4.122's mid-stream error frame, before / after content / uncoded) and
`toolcall` (one native tool call, then text — P4.121's legs, deterministic).
Smoke-tested offline before the walk. The copy's posed profiles from
2026-09-29 are GONE with the rsync; they are re-created in §3 E.
**Host zone:** CDT (`America/Chicago`) — the zone v4 renders in on this Mac.

**Round covered:** the `97b25fc53` follow-ups round (unified 2026-09-30) —
P4.119 (host-zone dates) · P4.120 (upload auto-describe + photo side effects)
· P4.121 (per-leg `CHAT_MESSAGE` rows) · P4.122 (mid-stream error frames +
the image flat body) · P4.123 (the wardrobe-tools avatar seam) · P4.125 (SPA
smalls: soft-break replacement, Concierge nits, the memory badge). P4.124 /
P4.126 carry no 💸 row of their own (P4.126's render riders ride P4.119's
rows).

## §0 Drift state

The ledger's §2 freshness probe **PASSED** at walk planning (2026-09-30): v4
`main` **AT** the baseline `97b25fc53`, `bugfix` unmoved at `1a2b2164c`,
checkout on `main`, tree clean, **§3 EMPTY**. No step in this walk may blame
drift. (Re-probe at walk start, after the human's refresh: PASS — unchanged.)

## §0.5 Pre-walk measurement (ledger §5.5 — read-only, BEFORE the first boot)

| # | population | measured | consequence for the walk |
|---|---|---|---|
| M1 | copy vintage — `migrations_state` count + newest | **199**, newest `drop-chat-rendered-markdown-v1` 2026-09-26 — the same vintage as 2026-09-29 | v5 must add no row; re-count after boot. |
| M2 | render reconcile: incomplete conversations + unembedded chunks (the scan SQL replicated read-only) | chunks **17,447, 23 unembedded**; the scan finds **ONE** chat — "Two on the Way at the Salon" (`ec7322be…`, 66 messages, 8 chunks, **6 owed / 2 embedded**), v4-rendered TODAY at 15:42Z with its `Past conversation message timestamp:` lines in **CDT** (`September 30, 2026 at 10:15 AM`). Chunk texts md5-snapshotted (scratchpad `m2-chunks-before.json`) | the A2 prediction: after P4.119, re-rendered chats' UNCHANGED interchanges enqueue nothing. |
| M3 | Friday's postbox — the 2026-08-30 14:40 CDT letter still there | verified live at B1 (the CLI's `db` has no mount-index flag) | B1's `02:40 PM` target. |
| M4 | Concierge desks — text / image / vision profile + provider | text `32053cdb…` NanoGPT DeepSeek V4 Flash; **image `479a208b…` "Flux 2 Dev" NANOGPT `flux-2-dev` — the human repointed it in v4 after #124**; vision `c19c142c…` Z.AI GLM 4.6V; default image profile GPT Image 2.5 Flare (OPENAI) | whether #124's WaveSpeed image desk was repointed on live (P4.123's roll, C-rows). |
| M5 | chats with `avatarGenerationEnabled = 1`; the avatar image profile resolution | **688** chats flag-ON, none autonomous — real subjects everywhere | P4.123's flag-ON chat — use one or flip a throwaway. |
| M6 | text-replacement rules in chat settings | `textReplacementsEnabled = 1`; five rules incl. **`teh → the`** (and `hte → the`) — no setup needed | P4.125's `teh → the` — create it through the UI if absent. |
| M7 | image describer / vision config; cheap LLM profile | describer `7d3086f6…` Grok 4 Fast Non-Reasoning (GROK); uncensored describer Z.AI GLM 4.6V; cheap LLM USER_DEFINED `32053cdb…` (NanoGPT DeepSeek) | P4.120's describe desk; P4.125's badge re-extract needs a cheap LLM. |
| M8 | a small real chat (few messages, has memories) for the badge | 12 candidates; e.g. "Versioned Hearts and System Logs" (`e068f1fa…`, 8 messages, 3 memories) | P4.125 badge row. |

## §1 What is NOT live — do not report these as bugs

From the six orders' Unification headers:

- **P4.119:** the zone VALUE is not threaded through the executor / engine /
  `build_context` / `prompt_section` — 13 ambient wrappers read the host zone
  once (same result in production). Tier 3 items 11–12 recorded only.
- **P4.120:** `invalidate_mount_point` is a measured no-op (v5 has no
  mount-chunk cache). The describe runs with v4's `chatId`-less call, so a
  **Locked** chat's upload can reach the uncensored vision desk — v4-faithful
  (candidate v4 note), not a bug.
- **P4.121:** the multi-character `name` is NOT on the wire / in the request
  hash (`StreamMessage` has no `name` slot — a named follow-up). The
  orchestrator oracle still strips `CHAT_MESSAGE`; the per-leg proof is the
  relocated loop families.
- **P4.122:** the PRE-stream arms' plugin catch line is unported (named);
  the image flat-body fix is proven on the wire family + unit pins only —
  **v5's OPENAI image URL is hard-coded to `api.openai.com`**, so no posed
  endpoint can drive it here; a real flat-body OpenAI image refusal is the
  only live proof (not posable). The OpenRouter-SDK 403 / Google 4xx
  triggers await the human's ruling. `OPENROUTER` (the raw fetch path)
  SKIPS an error frame by design — only the SDK flavours throw.
- **P4.123:** v5 can answer FAILURE with a job queued for a rolled-back
  write (post-closure trigger; recorded). The trigger's `background_jobs`
  row starts `PENDING` on v5 (v4's in-process queue flips `PROCESSING` fast).
  Autonomous chats never trigger (v4's rule).
- **P4.125:** the caret-MID-line soft-break divergence stays OPEN
  (`teh world`, caret after `teh`, Space → v5 replaces, v4 does not); the
  sidebar memory-badge variant is not ported; the badge's handler is
  duplicated across the two cards (one action home).
- Standing (earlier walks): a bare `?action=` → 400; a swipe writes no
  `llm_logs` row; the WaveSpeed npm plugin is unloadable (#124); chained
  follow-ups in an Unmoderated chat use the character's own profile (#125).

## §2 Server launch

```
RUST_BACKTRACE=1 RUST_LOG='info,quilltap::=debug,quilltap_core::tools=debug' \
  ./target/release/quilltap-web --data-dir ~/qt-dogfood-friday \
  --spa-dir apps/web/dist/quilltap/browser
```

## §3 The walk

Status: `PENDING` → `PASS` / `FAIL(#n)` / `DEFERRED-TO-HUMAN` / `BLOCKED(reason)`.

### A — Boot + the render reconcile (P4.119, near-zero spend)

| # | owner | gesture | expected + evidence | status |
|---|---|---|---|---|
| A1 | CLAUDE | boot on the copy | stale-lock reclaim or clean claim; no panic; unlocked (or the human unlocks) | **PASS** — clean boot at 16:20:30Z (the refresh removed the lock), no panic; the copy has no passphrase, so the engine unlocked at boot. |
| A2 | CLAUDE | read the boot log: `Conversation render reconciliation` count + the `EMBEDDING_GENERATE` jobs it enqueues | enqueued embeds ≤ M2's owed set — **no longer every interchange** (2026-09-29 A2: 39 for 5 owed). Query `background_jobs` by type after the drain | **PASS** ⭐ — `found incomplete conversations count=1` → `complete incomplete_chats=1 enqueued=1`; the render then embedded **exactly the 6 owed chunks** (interchanges 0, 3, 4, 5, 6, 7) and **not** 1 or 2, whose v4-embedded vectors survived. Chunk texts 1–7 are **byte-identical** to the v4 render (md5 before/after); chunk 0 differs only in the `Current time:` header (a render-time wall clock on both sides), which now reads `September 30, 2026 at 11:20 AM` (CDT, i.e. 16:20Z). **#121's re-embed churn is gone on real data.** |
| A3 | CLAUDE | `migrations_state` count after boot | = M1 | **PASS** — 199. |

### B — Host-zone dates on real data (P4.119 / P4.126, zero spend)

| # | owner | gesture | expected + evidence | status |
|---|---|---|---|---|
| B1 | CLAUDE | `chatRunTool list_mail` as Friday in a real chat | the 2026-08-30 letter reads **`August 30, 2026 at 02:40 PM`** (was `07:40 PM` UTC) | **PASS** — in "The Trustee's Head Count" (`c22d1a65…`), Friday's 7 letters, the newest `1788118818964-from-abigail.md` (= 19:40:18Z) listed as **`August 30, 2026 at 02:40 PM`**; the rest June 2026 in CDT. |
| B2 | CLAUDE | `send_mail` Amy a reply `in_reply_to` that letter; read the stored file | the persisted reply preface reads `In reply to your letter of August 30, 2026 at 02:40 PM`; Amy's `read_mail` shows the new letter's date in CDT | **PASS** — delivered as `Mail/1790785316298-from-friday.md` in Amy's vault; Amy's `read_mail`: *"A letter from Friday, posted September 30, 2026 at 11:21 AM:"* (16:21Z → CDT) then the persisted preface *"> In reply to your letter of August 30, 2026 at 02:40 PM:"* (2026-09-29 D2 persisted `07:40 PM`). The nested preface inside is v4's own bytes, already local. |
| B3 | CLAUDE | the Scriptorium badge → **click to re-render** on a v4-rendered chat (the broad gesture: the badge itself) | the chat's chunks re-render with **zero** `EMBEDDING_GENERATE` for unchanged interchanges; spot-check a chunk's `Past conversation message timestamp:` line reads the CDT wall time v4 wrote | **PASS (text) — the expectation was wrong about embeds.** Clicked the green badge on "Emerald Gown on the Covenant" (`7cb3cba1…`, 6 chunks, v4-rendered 2026-09-28): chunks 1–5 re-rendered **byte-identical** to v4's text; chunk 0 differs only in the `Current time:` header (`September 28, 2026 at 6:54 AM` → `September 30, 2026 at 11:22 AM`). But **6** `EMBEDDING_GENERATE` jobs ran — because the badge is the MANUAL re-render, which passes **`fullReembed: true`** on both sides (v4 `app/api/v1/chats/[id]/actions/render-conversation.ts:25`; the handler's `payload.fullReembed \|\| !chunk.embedding`, `conversation-render.ts:78`). v4-faithful, not a finding. The zero-embed proof belongs to a render WITHOUT `fullReembed` — A2's boot render supplies it. **The per-turn render (C3's send, `{chatId}` only) confirmed it again on a second chat:** "The Trustee's Head Count" went 15 → 17 interchanges; chunks **1–7 kept their v4 vectors**, and 0 + 8–16 re-embedded. 8–14 changed for a v4-faithful reason, not the zone: v5's context build **swept Friday's previous Aurora `core-whisper`** (`036ff2e4…`, 2026-09-29 15:58Z) when it posted today's — v4's own sweep (`context-manager.ts:2278-2297`) — so the transcript lost that message, every later `### Message N` renumbered by one, and v4's `Interchange 12 (continued 1)` sub-chunk no longer overflowed. Diffed against a read-only scratch copy of live v4's chunks; the only other deltas are the header's `Last Updated`/counts. → #130 (a candidate v4 note). |
| B4 | CLAUDE | `read_conversation` (Scriptorium tool) on a chat; the web-search / Almanack date lines if reachable | message times in CDT | **PASS (read_conversation)** — as Friday on "Two on the Way at the Salon": `Current time: September 30, 2026 at 11:24 AM` and the first message `September 30, 2026 at 10:15 AM` (= 15:15:09Z). A first try on a chat Friday is not seated in answered `Conversation not found.` with WARN `character does not participate in target chat` — v4's own rule (`read-conversation-handler.ts:94`). Web-search / Almanack date lines not driven (no web-search key is exercised in this walk; the Almanack is covered by the tier-1 `host_zone_dates` family). |

### C — The upload describe (P4.120, a few vision calls)

| # | owner | gesture | expected + evidence | status |
|---|---|---|---|---|
| C1 | CLAUDE | in a real chat, attach a real picture through the **composer's file input** (the SPA path, `POST /api/v1/chats/{id}/files`) | 201; within ~15 s `files.description` is set; an `IMAGE_DESCRIPTION` `llm_logs` row with **no chatId**; blank `linkedTo` links carry it | **PASS** ⭐ — a 640×480 canvas card (red disc left, blue square right, `PERIWINKLE 42` below) set on the composer's hidden `<input type=file>` + `change` (the SPA's own upload path) in "The Trustee's Head Count": `Transcoded blob to WebP` 19,644 → 5,780 B; **5.9 s later** `[Image Fallback] Successfully generated description … description_length=2338 profile_id=7d3086f6…` (the instance describer, GROK `grok-4.20-0309-non-reasoning`) and `auto-describe: completed file_entry_id=3748abc2… links_updated=1`. `files.description` set, and it matches the drawing (red disc left, blue square right, the text beneath). The `IMAGE_DESCRIPTION` row carries **`chatId: null`, `characterId: null`** — v4's `chatId`-less call. |
| C2 | CLAUDE | `background_jobs` after C1 | `EMBEDDING_GENERATE` jobs for the photo (the production `SaveImageSideEffects`) | **PASS** — 0.3 s after the describe: an `EMBEDDING_GENERATE` → `Mount chunk embedding generated chunk_id=f54a2cc0… mount_point_id=701e03fd…` — the production side effect that was a no-op on every photo path before P4.120. |
| C3 | CLAUDE | send a turn in that chat with the image attached | log `[Image Fallback] Reusing persisted description (no vision call)` on a non-vision seat, or the image sent natively on a vision seat — record which | **PASS, both arms in one turn.** Sent (Send button — this instance's composer takes Enter as a newline) *"Friday, a quick one: what shapes and words are on the card…"*: Friday (Z_AI `glm-5.3-flash`) took the image natively and answered *"A red circle on the left, a blue square on the right, and \"PERIWINKLE 42\" in black capitals underneath"*; the chain's Amy (DEEPSEEK `deepseek-v4-flash`, `supports_image_upload=false`) logged `[Attachment] Plugin cannot transport images; routing to describe-fallback` then **`[Image Fallback] Reusing persisted description (no vision call) file_id=3748abc2… source="stored-description"`**, and her reply engages with the card (and, fairly, notices the filename). |
| C4 | CLAUDE | upload a NON-image file | no describe fires (the `IMAGE` gate) | **PASS** — `dogfood-note-0930.txt` → `files` row `text/plain` / `ATTACHMENT`, `description` NULL, no describe line in 10 s. Removed from the composer afterwards. |

### D — Per-leg `CHAT_MESSAGE` rows (P4.121, a few LLM calls)

| # | owner | gesture | expected + evidence | status |
|---|---|---|---|---|
| D1 | CLAUDE | a posed `toolcall` seat: one Salon turn | **two** `CHAT_MESSAGE` rows for the one message — the first leg (tool call) and the native re-stream — each with its own `durationMs`, `characterId`, the leg's own `response.content` | **PASS** — "Dogfood 09-30 — posed tool legs" (`322be6dd…`, Friday on `POSED toolcall (dogfood)`): message `b1a6f944…` has **two** `CHAT_MESSAGE` rows — leg 1 `{content: "", finishReason: "tool_calls"}` and the re-stream `{content: "The posed tool ran; here is its answer, in plain text."}`, both carrying Friday's `characterId`. The pose's capture shows the re-stream request with 12 messages (the tool result in history). |
| D2 | CLAUDE | a REAL tool turn (ask Friday something that needs `list_mail` / `search`) | one `CHAT_MESSAGE` row per leg in the Inspector (LLM Inspector view in the SPA) | **PASS (rows)** — "Dogfood 09-30 — real tool legs" (`a11696fa…`, Friday on her own Z_AI `glm-5.3-flash`): message `958026a0…` → **two** rows, the tool-call leg (5,669 ms, `finishReason: tool_calls`) and the re-stream (7,165 ms). ⭐ The reply reads the letter as *"August 30, 2026, 2:40 PM"* — the host-zone date now reaches the MODEL through `list_mail` (P4.119 end to end). The rows were read from `llm_logs`; the Inspector view was not opened (pane hidden). |
| D3 | HUMAN | an autonomous room's budget charges the per-leg rows | long run + spend | DEFERRED-TO-HUMAN (→ H2) |

### E — Mid-stream error frames (P4.122, posed primary + the real desk)

Setup: `OPENAI_COMPATIBLE` profiles on `http://127.0.0.1:8898/v1`, model
names `midframe` / `midframe-late` / `midframe-uncoded` / `toolcall`, a
dummy key; each seated as the LLM character in its own **Moderated** chat.

| # | owner | gesture | expected + evidence | status |
|---|---|---|---|---|
| E1 | CLAUDE | one turn on `midframe` | `[Failover] Primary call failed`, trail `{primary, refused, moderation-refusal, provider-code}` then `{desk, via: concierge, answered}`; the refusal ledger +1 | **PASS** ⭐ — Moderated chat `a233e6ef…`: the pose answered **200** and then sent the error frame; `Classified a provider outcome for refusal refused=true evidence="provider-code"` → WARN `[Failover] Primary call failed … trigger="moderation-refusal"` → `[DangerousContent] Attempting uncensored retry … refusal_was_stated=true` → `Uncensored retry succeeded` (NANOGPT `deepseek/deepseek-v4.1-flash`) → `Moderation refusal recorded … evidence="provider-code" rerouted=true count=1`. Stored trail: `{POSED midframe, via: primary, outcome: refused, trigger: moderation-refusal, evidence: provider-code, detail: "code content_filter: …"}` then `{DeepSeek V4 Flash Latest, via: concierge, outcome: answered}`. Before P4.122 this frame took the EMPTY path. (The greeting on the same pose fell to the static line with `content_filter_hit=false` — v4's rule: only a NON-throwing `contentFilterDetected` sets it, `route.ts:940-944`, so a thrown coded refusal never reaches the desk on the greeting — see #131.) |
| E2 | CLAUDE | one turn on `midframe-late` | **no failover**; the partial `The kettle had only just begun to ` kept (or v4's mid-stream rule — record exactly) | **PASS** — `e35ab01d…`: classified `provider-code` again, then INFO `[Failover] Skipping chain: content already reached the user … trigger="moderation-refusal" partial_length=34`; Friday's message `cc6fc89f…` persisted as *"The kettle had only just begun to\n\n{{OOC: stream ended abruptly (The response was filtered …)}}"*; no trail, no ledger row; the dispatch answered `internal "primary stream failed: …"` (the send's error frame). |
| E3 | CLAUDE | one turn on `midframe-uncoded` | measured: the trigger / evidence recorded (not a refusal — no code); compare with P4.122's `primary_stream_tier3` uncoded arm | **PASS (measured)** — `c8f31d3e…`: `refused=false has_error=true` → WARN `[Failover] Primary call failed … trigger="provider-error"` → ERROR `[Failover] Fallback chain exhausted … attempts=[("POSED midframe-uncoded (dogfood)", "OPENAI_COMPATIBLE", "provider-error")]` (the posed profile has no understudy), send answered `internal`. An uncoded frame is an ordinary provider error, never a refusal — the classifier reads the code. |

### F — The wardrobe-tools avatar seam (P4.123, one image)

| # | owner | gesture | expected + evidence | status |
|---|---|---|---|---|
| F1 | CLAUDE | flag-OFF baseline: `wardrobe_wear` as a character in a chat with `avatarGenerationEnabled` off | no `CHARACTER_AVATAR_GENERATION` job | **PASS** — `chatRunTool wardrobe_wear {item_title: "Apron"}` as Friday in `a11696fa…` (flag NULL): *"Layered \"Apron\" into top, bottom…"*; no avatar job, no `[CharacterAvatar]` line. |
| F2 | CLAUDE | flag ON (the chat's avatar toggle in the SPA); the character wears something — `wardrobe_wear` via a real Salon turn if the model obliges, else `chatRunTool` | exactly **one** `CHARACTER_AVATAR_GENERATION` job, `callerContext`-tagged; the job runs and the roll lands (a new avatar-roll file) — looked at | **PASS** ⭐ — **a real Salon turn** in the already flag-ON "Trustee's Head Count" (the toggle not used: v4's toggle-ON itself enqueues a roll for every LLM seat, `toggle-avatar-generation.ts:80`, which would have muddied the count). Friday (Z_AI) called `wardrobe_list` then `wardrobe_wear` (Apron); `[CharacterAvatar] Avatar generation job enqueued … character_id=d9d0d998…` → exactly **one** job (payload `{chatId, characterId, imageProfileId: 6de59b2d…}` — `callerContext` is a log field in v4 too, not payload) → `Image call answered first time purpose="avatar" … gpt-image-2.5-flare` → `Avatar image saved file_id=2c2cb315…` (1024×1536 WebP, 172,802 B) → the Lantern's `avatar` announcement and Aurora's `outfit-change` row. **Looked at:** Friday in a navy apron with gold trim over the navy sweater and forest-green cardigan — the new outfit exactly. The chain's later turns called no tool and queued nothing. |
| F3 | CLAUDE | `wardrobe_take_off` of an unequipped slot (`appliedCount = 0`) | no job | **PASS — with a corrected expectation.** Taking off an item Friday was NOT wearing ("Apple Watch"; her accessories were the ring + pearl studs) answered *"Took \"Apple Watch\" off accessories; any other layers there stayed."* and queued a roll — **v4-faithful**: v4's `remove` arm does `appliedCount++` for every RESOLVED item, worn or not (`wardrobe-take-off-handler.ts:119-130`), so v4 would roll the same portrait → #132. The true `appliedCount = 0` arm (a take-off of a nonexistent item → `Failed: Wardrobe item not found…`) queued **no** job. |

### G — SPA smalls (P4.125, near-zero spend)

| # | owner | gesture | expected + evidence | status |
|---|---|---|---|---|
| G1 | CLAUDE | Settings → Chat → Text Replacements: add `teh → the` (if absent), then in a Salon composer type `first line`, **Shift+Enter**, `teh`, **Space** | the editor holds `first line<br>the ` | **PASS** — the rule already existed on the instance (M6). Real keystrokes in "The Trustee's Head Count" composer: the editor's DOM reads **`<p>first line<br>the </p>`** and the screen shows `first line` / `the`. Composer cleared afterwards. |
| G2 | CLAUDE | the memory badge on a small chat's card (Salon list + the character's chats tab) | the confirm text, then v4's count toast; the Inspector / `background_jobs` shows the extraction jobs; `memoryDeleteByChat` then `chatQueueMemories` on the wire in that order | **PASS (Salon list card)** — clicked the `3 memories — delete and re-extract` badge on "Versioned Hearts and System Logs" (`e068f1fa…`; `window.confirm` stubbed to record + accept, since the pane's native confirm answers false): the prompt is v4's *"This will delete all existing memories from this chat and re-extract them from the conversation. Are you sure?"*; the wire (a `fetch` wrapper) ran `memoryDeleteByChat` **then** `chatQueueMemories`; **3** `MEMORY_EXTRACTION` jobs queued at 16:43:42 (the header chip `Mem 3`); the badge went 3 → 0 → **7** as the jobs landed. A second click caught the toast *"Queued 3 memory extraction jobs"* — v4's template byte for byte (`SalonListView.tsx:136`). The character-tab card was not driven. ⚠ Instrument: a first click, aimed from `getBoundingClientRect`, opened a DIFFERENT chat — the pane's page runs at 1.479× the screenshot's frame, so page coordinates must be divided by `innerWidth/800` before a `computer` click. |
| G3 | CLAUDE | Concierge off duty → the Salon-side hint; a `conflict` refusal wording | the off-duty hint reads v4's text; restore on duty | **PASS (hint)** — unticked *The Concierge is on duty* in The Foundry's Concierge tab (`conciergeSettings.enabled` → false in the row; the tab's banner *"The Concierge is off duty. Nothing is rerouted…"*); the Salon sidebar's Chat section then showed the per-chat Concierge select **disabled** (value `moderated`) with *"The Concierge is off duty — turn him on in Settings → The Concierge."* Re-ticked; the row reads `enabled: 1`. The `kind === 'conflict'` refusal wording was not posed (no live 409 path on hand; the vitest pins it). |

### H — Human remainder

| # | owner | gesture | why human | status |
|---|---|---|---|---|
| H1 | HUMAN | the standing queue: Lantern budget, a real token-limit turn, the four planted proofs, dedup/summaries, the Brahma deep query, #101, the compression re-measure, the conceal-marker arm | spend / long runs | DEFERRED-TO-HUMAN |
| H2 | HUMAN | D3 (autonomous budget) | long run | DEFERRED-TO-HUMAN |
| H3 | HUMAN | a real flat-body OpenAI image refusal (P4.122 (B)) | not posable — the OPENAI image URL is hard-coded to `api.openai.com`; needs a prompt the real endpoint refuses | DEFERRED-TO-HUMAN |

## §4 Findings from this walk

| # | finding | class | disposition |
|---|---|---|---|
| **130** | A per-turn render re-embeds EVERY chunk after a swept whisper. When a new Aurora `core-whisper` (or Commonplace whisper) posts, the context build deletes that seat's previous one; the transcript loses the message, every later `### Message N` renumbers by one, and interchange sub-chunk boundaries move — so every later chunk's text changes and its embedding is nulled and redone (C3's send: 7 unchanged-content chunks re-embedded on "The Trustee's Head Count") | **v4-faithful** — v4's sweep (`context-manager.ts:2278-2297`) and its render do the same; v5's sweep is `build_context.rs:956` | RECORDED — candidate v4 note: the render could number messages by a stable id or skip whisper rows, so a sweep doesn't re-embed the tail. No v5 change. |
| **131** | A greeting whose provider throws a CODED refusal never reaches the uncensored desk: the ladder's `contentFilterHit` is set only from a non-throwing result's `contentFilterDetected`, so an Azure-style `content_filter` 400 (or P4.122's mid-stream frame) falls through to the static greeting with `content_filter_hit=false` | **v4-faithful** (`app/api/v1/chats/route.ts:940-944`; E1) | RECORDED — candidate v4 note: route the greeting's thrown refusal through the same classifier the Salon turn uses. No v5 change. |
| **132** | `wardrobe_take_off` of an item the character is NOT wearing reports *"Took … off accessories"* and counts as applied, so with avatar generation on it **rolls a new portrait** (an image spend for no visible change) | **v4-faithful** — v4's `remove` arm does `appliedCount++` for every resolved item, worn or not (`wardrobe-take-off-handler.ts:119-130`) | RECORDED — candidate v4 note: count an op as applied only when a slot actually changed. No v5 change. |

Non-findings recorded so nobody re-files them: the Scriptorium badge's re-render passes `fullReembed: true` on both sides (B3) — it re-embeds every chunk by design; `read_conversation` on a chat the character is not seated in answers `Conversation not found.` (v4's rule); the composer on this instance takes Enter as a newline (the user's setting).
