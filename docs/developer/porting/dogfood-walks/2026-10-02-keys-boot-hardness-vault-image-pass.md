# Dogfood walk — the `ca363178d` + `f6426e196` rounds: the profile's own key,
# boot hardness, bug 173's CLI decode, the anti-committee prompts, the scene
# note, bug 174's vault image bytes, the API-key fallback lines (2026-10-02)

**Instance:** `~/qt-dogfood-friday` — a COPY of real Friday data, refreshed by
the human before the first boot (vintage recorded at M1). Disposable; never
rsynced back.
**Server:** `RUST_BACKTRACE=1 RUST_LOG='info,quilltap::=debug,quilltap_core::tools=debug'
./target/release/quilltap-web --data-dir ~/qt-dogfood-friday --spa-dir
apps/web/dist/quilltap/browser`
**Log of record:** the scratchpad server log + `~/qt-dogfood-friday/logs/combined.log`.
**Instrument:** `harness/tools/refusal-server.py` on `127.0.0.1:8898`
(capture to the scratchpad; `QT_REFUSE_KEY` for the key gate rows).
**Host zone:** CDT (`America/Chicago`).

**Rounds covered** (both unified, neither walked):

- **`ca363178d` (unified 2026-10-01):** P4.D240 (bug 173 — the CLI's raw-SQL
  reader decodes compressed text; `help/` + `docs/v4/` at `ca363178d`) ·
  P4.D241 (21 built-in prompts re-vendored, lazy seeder refresh; the five
  trust directions at eight generator sites) · P4.D242 (memory-consent
  extraction prompts, the two-arm transcript heading, `FOLD_EPISODE_PROMPT`)
  · P4.D243 (the chained-turn scene note) · P4.133 (dogfood #133 — the
  profile's OWN key on every leg) · P4.134 (dogfood #134(b) — boot steps
  warn-and-continue).
- **`f6426e196` (unified 2026-10-02):** P4.D244 (bug 174 — vault image bytes,
  not a server path, to Z.AI / NanoGPT) · P4.135 (the D184 collapse fails the
  boot on a failed PASS, logs-and-skips on a failed `shouldRun`) · P4.136
  (six API-key reads get v4's fallback lines; the engine thaw sends the
  gate's key; the test-message / models-fetch 404s) · P4.137 (an empty new
  user message takes v4's chained branch) · P4.138 (harness-only — no live
  row).

## §0 Drift state

The ledger's §2 freshness probe at walk planning (2026-10-02): v4 `main`
**AT** the baseline `f6426e196`, both logs empty, `bugfix` unmoved at
`1a2b2164c`, checkout on `main`, the tree dirty by EXACTLY the recorded
waiver (the uncommitted bug-175/176 filings — docs only). **§3 EMPTY.** No
step may blame drift.

## §0.5 Pre-walk measurement (read-only, AFTER the refresh, BEFORE the first boot)

| # | population | measured | consequence for the walk |
|---|---|---|---|
| M1 | `migrations_state` count + newest; v4's last-run version | **199**, newest `drop-chat-rendered-markdown-v1` 2026-09-26T13:01Z (`4.10.0-dev.96`) — same vintage as the 09-30 walks | v5 must add none (A2). |
| M2 | the collapse gate: ledger row + unkeyed avatar rolls | ledger row **present** (`collapse-duplicate-avatar-rolls-v1`, 2026-09-11T17:45Z, 1785 items); unkeyed rolls **0** | G1: NO plant (both conditions fail) — expect the gate's `AlreadyCompleted`. |
| M3 | built-in `prompt_templates` vs the shipped text | 27 built-in rows; the three MODERN rows last updated 2026-08-19, **none** carry `## Whose story it is` (decoded through `qt_text`) — v4 has NOT refreshed them | A3 is a REAL proof: expect refresh lines (up to 21) and MODERN General gaining the heading. |
| M4 | `api_keys` per provider; `Z_AI` / `NANOGPT` profiles | one key per provider (NANOGPT `NanoGPT QT`, Z_AI `Z.AI`); 14 NANOGPT profiles (incl. `GLM 5.3 Flash` `z-ai/glm-5.3-flash`, `NANOGPT/zai-org/glm-4.6v`), 8 Z_AI (incl. `Z.AI GLM 5.3 Flash` `glm-5.3-flash`, `Z.AI GLM 4.6V`); **no** OPENAI_COMPATIBLE profile | C/D: the posed profiles + junk-key profiles created through the API. |
| M5 | a character vault photo | `Friday Character Vault` holds 238 images; smallest: link `3fd8abb6-b4cb-4502-a9f6-54f361bc944f` (`photos/…token-usage-dashboard…webp`, 5,128 B) | D1/D2's attachment (a `fileIds` entry that is a mount LINK id → `load_mount_file_as_attachment`). |
| M6 | multi-character / persona-seat chats | create throwaways at F (a two-LLM-seat chat on the posed desk; a 1:1 with a user-controlled persona seat) | F1–F4. |
| M7 | the largest chats | `The Bridge of Ordinary Breathing` (4,460 messages), `Chat with Riya and Anjali Rajan` (1,570) | B1–B4 run the round's own query. |

## §1 What is NOT live — do not report these as bugs

- **P4.137:** the SPA composer's attachment-only Salon send posts `''` where
  v4 sends `'Please look at the attached file(s).'` — so on v5 an
  attachment-only post persists no row and links no file (deferred, phase-4
  NEXT; F8 RECORDS it only). The `fileProcessing` frame is absent (no
  fallback notice where v4 shows one). The text-file-prefix and
  whitespace-with-file shapes are v4-faithful.
- **P4.136:** ~20 raw `api_keys::find_by_id*` reads still skip the fallback
  homes (a census + guard is OPEN) — a corrupted key row on a path outside
  the six folds + `model_fetch` may still 500. The Scenario Builder spine's
  key is correct by inspection but unpinned (the host recorder is OPEN).
  `MainReads` pool failure omits v4's `Failed to get API keys collection`.
- **P4.135:** a failed ledger PROBE is fatal on v5 (v4 falls back to a file
  state); a failed ledger WRITE logs no `Migration threw an exception`. Both
  recorded on `CollapseError::Fatal`.
- **The ledger-gate divergence** (v4 bug 176): v5 re-runs its ensures every
  boot where v4 skips a ledgered migration — so a lazy-home failure logs on
  EVERY boot (G2 expects that).
- **P4.D244:** failed provider calls write no `llm_logs` row (v4 too, its own
  named follow-up) — the D rows read the body from the server log, not
  `llm_logs`.
- **P4.127 Option V (k):** a non-IANA POSIX `TZ` still puts every surface in
  UTC (ordered).
- Standing: a bare `?action=` → 400; a swipe writes no `llm_logs` row; the
  WaveSpeed npm plugin is unloadable (#124); #130–#132 v4-faithful; Carina's
  consult logs no `CHAT_MESSAGE` row (P4.129, ordered); two real chats'
  empty-`createdAt` rows are skipped on read (v4-faithful data damage).

## §2 Server launch

```
RUST_BACKTRACE=1 RUST_LOG='info,quilltap::=debug,quilltap_core::tools=debug' \
  ./target/release/quilltap-web --data-dir ~/qt-dogfood-friday \
  --spa-dir apps/web/dist/quilltap/browser
```

Posed desks: `OPENAI_COMPATIBLE` profiles on `http://127.0.0.1:8898/v1`,
model names `echo` / `refuse-code` / `toolcall`, dummy keys (created through
the API on the copy, named `POSED …`).

## §3 The walk

Status: `PENDING` → `PASS` / `FAIL(#n)` / `DEFERRED-TO-HUMAN` / `BLOCKED(reason)`.

### A — Boot (zero spend)

| # | owner | gesture | expected + evidence | status |
|---|---|---|---|---|
| A1 | CLAUDE | boot on the copy | clean claim, no panic, unlocked; `/health` ready | **PASS** — boot at 15:18:57Z; no ERROR/WARN line, no panic; FTS 84,009/84,009; help docs 129 unchanged; render reconcile 2 incomplete (1 enqueued, 1 reused); the copy has no passphrase, so the engine unlocked at boot; `/health` `healthy` (web 0.0.207). |
| A2 | CLAUDE | `migrations_state` after boot | = M1 | **PASS** — 199 after boot (= M1). |
| A3 | CLAUDE | the first template list (Settings → prompts) | M3's count of INFO `Built-in prompt template refreshed from shipped text`; MODERN General carries `## Whose story it is` | **PASS** ⭐ — the New Character form's *Import Template* modal (the SPA's one `promptTemplateList` consumer) at 15:20:18Z logged **21** INFO `quilltap::db: Built-in prompt template refreshed from shipped text template_id=… name=… source=plugin` (CLAUDE Companion … OLLAMA Romantic). All 21 rows now match `builtin_prompt_templates.json` byte-for-byte (decoded via `qt_text`); MODERN General carries `## Whose story it is` — as do exactly the 13 shipped texts that have it. The six legacy-named built-ins (`GPT-5 …`, `MISTRAL_LARGE …`) untouched. |

### B — Bug 173: the CLI decodes compressed text (P4.D240, zero spend)

| # | owner | gesture | expected + evidence | status |
|---|---|---|---|---|
| B1 | CLAUDE | `quilltap db --data-dir <copy> --json "SELECT id, content FROM chat_messages WHERE length(content) > 600 LIMIT 2"` | readable text, not a Buffer / binary | **PASS** — both rows (`580a1362…` 619 B, `aef65d62…` 3,040 B) are `blob` on disk (67,365 of the table's messages are compressed) and `--json` prints their text (`"content": "I’d like to create a web-based game…"`, `"# Ranch Rush Clone: Project Plan…"`). |
| B2 | CLAUDE | the same in table mode | readable text (the table-mode seam decided (b)) | **PASS** — table mode prints the same two as quoted text (`'I’d like to create…'`, `'# Ranch Rush Clone…'`). |
| B3 | CLAUDE | the same selecting an `embedding` column | `--json` Buffer JSON / table compact JSON (unchanged) | **PASS** — `chat_messages` has no `embedding` column on this vintage; on `memories` (whose embedding blob begins `0xEB 0x01`, the compression marker's first byte) `--json` keeps `{"type": "Buffer", "data": [235, 1, 1, 0, 4, …]}` and table mode the compact `{"type":"Buffer","data":[235,1,1,0,…]}` — not decoded as text. |
| B4 | CLAUDE | a `WHERE qt_text(content) LIKE '%…%'` query | rows match | **PASS** — `WHERE qt_text(content) LIKE '%Ranch Rush Clone: Project Plan%'` finds 2 rows (both `blob`); the bare `content LIKE` finds 0 — the paragraph's advice is the right advice. |
| B5 | CLAUDE | the SPA help → Database Protection page | the new bug-173 paragraph renders | **PASS** — Help → search *Database Protection* → the page renders the new paragraph (*"A word on the longer messages: Quilltap keeps any message of 512 bytes or more folded up in compressed form…"*) with the `qt_text()` example. |

### C — The profile's own key + the API-key fallback lines (P4.133, P4.136; posed + free 401s)

| # | owner | gesture | expected + evidence | status |
|---|---|---|---|---|
| C1 | CLAUDE | `refusal-server.py` with `QT_REFUSE_KEY=k-bound`; two `OPENAI_COMPATIBLE` keys (`k-first` created FIRST, `k-bound` second); two `echo` profiles bound one each; a turn on each | the `k-first` profile 401s (catch line + `[Failover] … trigger="auth"`); the `k-bound` profile answers; capture lines name `Bearer k-first` / `Bearer k-bound` | **PASS** ⭐ — `refusal-server.py` gated on `QT_REFUSE_KEY=k-bound`; keys created `POSED k-first` (`477ad18a…`) then `POSED k-bound` (`239a76b2…`), so the provider's FIRST active key is `k-first`. In "DOGFOOD keys" (`72e31ee6…`), Friday on `POSED echo k-first`: the greeting's three attempts and the Salon turn each captured `Bearer k-first` → ERROR `OpenAICompatible API error in streamMessage … error=401 Incorrect API key provided: dogfood.` → `[Failover] … trigger=\"auth\"` → chain exhausted. Seat switched to `POSED echo` (bound to the SECOND key): captured `Bearer k-bound`, `keyGate: true`, the turn answered (`chatSend` → `hasContent: true`). Each profile sends its own key — not the provider's first. |
| C2 | CLAUDE | a junk `NANOGPT` key added SECOND; a NanoGPT profile bound to it; one turn | the turn FAILS with NanoGPT's `401 Invalid session` (before #133 it answered on the real key) — free | **PASS** — a `JUNK NanoGPT` key added SECOND (after the real `NanoGPT QT`), profile `JUNK NanoGPT GLM 5.3 Flash` bound to it: ERROR `NanoGPT API error in streamMessage … error=401 Invalid session` → `[Failover] … trigger=\"auth\"` → chain exhausted; `chatSend` answered `primary stream failed: HTTP 401 … Invalid session`. On 2026-09-30 (#133) the same setup ANSWERED on the real key. |
| C3 | CLAUDE | "Try uncensored" on a message whose profile has an uncensored understudy on the posed desk bound to `k-bound` | the reroute's captured header is the UNDERSTUDY's key | **PASS** — the Concierge's uncensored text desk set (on the copy) to `POSED understudy k-first` (model `echo-understudy`, bound to the FIRST key); `messageRetryUncensored` on Friday's `ok` reply (`0d397c86…`, answered on the `k-bound` profile): the one captured call is `model: echo-understudy`, `Bearer k-first` — the UNDERSTUDY's own key, not the original's — and the gate's 401 surfaced as the error. |
| C4 | CLAUDE | a cheap-LLM title on a keyless profile (a profile with `apiKeyId` NULL on a key-requiring provider) | v4's throw: `No API key available for cheap LLM provider` | **PASS** — `KEYLESS NanoGPT` (NANOGPT, `apiKeyId` NULL) set as the cheap profile (`USER_DEFINED`); `chatRegenerateTitle` on "DOGFOOD keys": WARN `[CheapLLM] Task failed task_type=\"title-chat\" … error=No API key available for cheap LLM provider` → ERROR `[Chats v1] Title generation failed … error=\"No API key available for cheap LLM provider\"` → 500 with that message — v4's `serverError(result.error)` (`actions/title.ts:72-74`). |
| C5 | CLAUDE | corrupt a throwaway key row (`UPDATE api_keys SET key_value = x'00000000'`, server stopped); a cheap-LLM title on a profile bound to it | ERROR `quilltap::db Error finding API key by ID and user ID … error=Invalid column type Blob at index: 4, name: key_value` + v4's throw; no 500 | **PASS** — server stopped; `JUNK NanoGPT`'s row planted `key_value = x'00000000'` (`typeof` → `blob`); the cheap profile = `JUNK NanoGPT GLM 5.3 Flash`. `chatRegenerateTitle`: ERROR `quilltap::db: Error finding API key by ID and user ID collection=\"connection_profiles\" keyId=43cb8bb0… userId=ffffffff-… error=Invalid column type Blob at index: 4, name: key_value` → the `[CheapLLM] Task failed … No API key available for cheap LLM provider` throw → the logged title failure; no panic, no stray 500 from the read itself. |
| C6 | CLAUDE | connection test-message on a dangling `apiKeyId`, then on the corrupted row | 404 both; the logged 404 on the corrupted one | **PASS** — `connectionProfileTestMessage` with a dangling `apiKeyId` (`00000000-0000-4000-8000-…`) → `not-found` *"API key not found"* (no line); on the corrupted row → the same `not-found` with ERROR `Error finding API key by ID collection=\"connection_profiles\" keyId=43cb8bb0… error=Invalid column type Blob…` (the unscoped home). Both v4's `notFound('API key')`. (A REST `POST /api/v1/connection-profiles?action=test-message` answers 405: v5 serves this family over dispatch only — instrument note.) |
| C7 | CLAUDE | the models fetch (`POST /api/v1/models`) on the corrupted key | 404 `API key not found not found` (v4's doubled text) + the line | **PASS** — `modelFetch` {NANOGPT, the corrupted key} → `not-found` *"API key not found not found"* (v4's own doubled text) with ERROR `Error finding API key by ID and user ID … keyId=43cb8bb0… error=Invalid column type Blob…` (the SCOPED home — the unification's fifth route). (REST `POST /api/v1/models` 405s — dispatch-only family.) |
| C8 | CLAUDE | a head-and-shoulders backfill on the corrupted key | SKIPS with the line + the WARN (used to fail the job); zero spend | **PASS** ⭐ — with v4's cross-app flag `headshoulders_backfill_enqueued_v1` deleted (plant) and the cheap profile on the corrupted key, the boot-2 scan enqueued **3** `CHARACTER_HEADSHOULDERS_BACKFILL` jobs; each logged ERROR `Error finding API key by ID and user ID … error=Invalid column type Blob at index: 4, name: key_value` then WARN `[HeadShouldersBackfill] No API key for cheap LLM selection, skipping context=\"background-jobs.headshoulders-backfill\"` and `Job completed` — a skip, not a failed job (it used to fail). The flag was re-written `true`. Zero spend. |
| C9 | CLAUDE | a Scenario Builder run on a `k-bound` posed desk with the gate on | the build is answered (the gate's key sent — the engine thaw); one `api_keys` read per build if logged | **PASS** — `scenarioBuilderBuild` on `POSED echo` (bound to `k-bound`): ONE call captured, `Bearer k-bound`, `keyGate: true`, `done: true` with the scene. The contrast run on `POSED echo k-first` captured `Bearer k-first` and answered v4's `scenario_builder_failed` (*"The Host has been detained by circumstances beyond his control…"*) with the 401 in `details` — the spine sends the PROFILE's key, never a provider scan (the engine thaw's host leg, which no test pins). The "one `api_keys` read per build" count is not observable in the log (no line per read) — recorded, not claimed. |

### D — Bug 174: vault image bytes to Z.AI / NanoGPT (P4.D244, free via a junk key)

| # | owner | gesture | expected + evidence | status |
|---|---|---|---|---|
| D1 | CLAUDE | a vault photo (M5) attached to a turn on a Z.AI vision profile (`glm-5.3-flash`) bound to a junk key | the request body logged BEFORE the HTTP call carries `data:image/…;base64,`, NEVER `/api/v1/mount-points/…`; then Z.AI's 401 | **PASS** ⭐ — instrument upgraded from the order's log line (no stream path logs its body) to the real WIRE: the junk-key profile's `baseUrl` pointed at `refusal-server.py` (a profile `baseUrl` overrides every provider's vendor base), whose key gate 401s the junk key AFTER capturing the body — zero spend. Friday on `JUNK Z.AI GLM 5.3 Flash` (Z_AI, `glm-5.3-flash`). The vault link (`3fd8abb6…`, Friday's vault) sent as a `fileIds` entry is persisted on the user row but NOT loaded on that turn — the current-message path reads chat-linked `files` rows only (v4's `loadAndProcessFiles`, faithful). On the NEXT turn the bug-121 rehydration loaded it through the mount store: the captured body carries `{"type":"image_url","image_url":{"url":"data:image/webp;base64,UklGRgAU…"}}` — decoded 5,128 B, SHA-256 `688b299a…` = the vault file's own `doc_mount_files.sha256` — and `/api/v1/mount-points/` appears **nowhere** in the body. |
| D2 | CLAUDE | the same on a NanoGPT vision profile bound to the junk key | the same | **PASS** — the same on `JUNK NanoGPT GLM 5.3 Flash` (NANOGPT, `z-ai/glm-5.3-flash`): the same `data:image/webp;base64,UklGRgAU…` part, no mount path, `Bearer junk-nanogpt-dogfood` captured then 401. |
| D3 | CLAUDE | an UPLOADED (non-vault) image on both | the legacy `files` branch unmoved (bytes) | **PASS** — a generated 64×48 PNG uploaded through `POST /api/v1/chats/{id}/files` (stored as a 244-B WebP, `ec4bb14c…`) then sent as `fileIds` on both junk profiles: the current-message (`files`) branch puts its bytes on the wire (`data:image/webp;base64,UklGRuwA…` = 0xEC+8 = 244 B) on NanoGPT and on Z.AI, alongside the rehydrated vault photo; no `/api/v1/files/` path in either body. (Rehydrated images ride the CURRENT user message — v4's "anchor the usual way", `context-builder.service.ts:415`.) |
| D4 | CLAUDE | `docs/v4/developer/bugs/fixed/bug-174-vault-image-relative-url-to-provider.md` | present, matches v4 | **PASS** — `docs/v4/developer/bugs/fixed/bug-174-vault-image-relative-url-to-provider.md` byte-identical to v4 `f6426e196`'s. |

### E — The anti-committee prompts + memory consent (P4.D241, P4.D242; posed desks)

| # | owner | gesture | expected + evidence | status |
|---|---|---|---|---|
| E1 | CLAUDE | Summon From Lore on a posed `echo` desk | the `system_prompts` step's captured request shows `300-600`, TRUST, the relationships sentence | **PASS** — *Summon From Lore* is the AI import (`aiImportStream`). On a posed desk the plain `echo` reply fails the first step's JSON parse and aborts before `system_prompts`, so the instrument grew a `json` mode (a small JSON object; `refusal-server.py`). Run on `POSED json` (`k-bound`): six calls captured; the third, the `system_prompts` step, carries *"A comprehensive system prompt (300-600 words)…"*, the trust-safeguards direction, the gated companion disposition (*"Include the companion trust disposition below only when…"* / *"Because this character is {{user}}'s companion or partner…"*) and, ONE `\\n` after it in the same paragraph, *"Use the relationships array in the Prior Analysis, where one is given, as evidence for that decision."* (The run then ended on the canned JSON's shape — `(stepResults.system_prompts || []).map is not a function` — an instrument artefact, v4's own wording.) |
| E2 | CLAUDE | External Prompt within budget; then on a tiny-`maxContext` profile with `maxTokens` 20000 | runs; then refuses with the new budget number | **PASS** — Friday's *Executive Assistant* prompt: on `POSED big context` (`maxContext` 200000, `maxTokens` 4000) the run answered (`{prompt: \"ok\", tokensUsed: 501}`), the captured request carrying the TRUST direction; on `POSED tiny context` (16000, `maxTokens` 20000) it REFUSED before any call (0 captured): *"Character data is too large for the selected model's context window at the requested output size. Estimated input: ~7097 tokens, safe limit: ~1000 tokens…"* — the estimate reflects the grown `META_SYSTEM_PROMPT`. (A profile with no `maxContext` gets the small default window and also refuses at 4000: safe limit ~3372.) |
| E3 | CLAUDE | a Salon turn whose memory extraction runs (cheap desk) | the extraction request carries the ORDERED heading + the AGREEMENTS block in both bodies | **PASS** — a turn on `POSED echo` with the cheap LLM also on the posed desk (`k-bound`): the four extraction calls (the user-controlled SUBJECT pass, the SELF pass, two OTHER/observer passes) each carry `AGREEMENTS, PROPOSALS, AND CONDITIONS — read these strictly` (incl. *"…; <name> had not yet responded."*) in the SYSTEM body and *"TURN TRANSCRIPT (in the order spoken — the USER's lines came first; nothing the USER says here answers anything a character says below it):"* in the USER body. Both bodies, all four calls. |
| E4 | CLAUDE | a turn ending on a character's condition | the stored memory carries `…; <name> had not yet responded` | **PASS** (run after the human's go-ahead; real spend: one greeting + one turn on `GLM 5.3 Flash`, four extraction passes on the real cheap `DeepSeek V4 Flash Latest`) — "DOGFOOD condition" (`9a1696f4…`): Charlie asks Friday to name her condition for taking the Roadster out *"I'll answer once I've heard it"*; Friday's reply ends on it (*"My condition: you drive the whole day at civilian speed… That's it. Two clauses, both enforceable."*). The `MEMORY_EXTRACTION` job (`79f36d6a…`, 45 s) stored, about Charlie, `4a01762d…`: *"Friday set a two-clause condition on Charlie taking the Roadster to the coast on Saturday: … ; Charlie had not yet responded when the exchange ended."* — and no memory on either side records an agreement. |
| E5 | CLAUDE | a fold (Rebuild Summary / a long-enough chat) | the episode sentence in the fold request (`FOLD_EPISODE_PROMPT`) | **PASS** — `chatRebuildSummary` on "DOGFOOD keys" (job `8ccc6df1…`, CONTEXT_SUMMARY completed): three cheap calls captured — the summary update, the fold-episode pass, the title — and the fold-episode call's system prompt is **byte-identical** to the generated `FOLD_EPISODE_PROMPT` (cap 2), carrying *"An episode records what was said and done, not what was agreed: attribute proposals and conditions to their speaker, and record an agreement only where the agreeing party's own assent appears in the window."* (The `ok` reply retitled the chat `ok` via `[Auto Title] … source=\"summary-fold\"` — instrument artefact.) |
| E6 | CLAUDE | an empty-question Carina run | the request keeps `TURN TRANSCRIPT:` | **BLOCKED (no live gesture reaches it)** — an empty-question Carina run cannot be produced: `carina_parser` fires only on a line yielding a NON-empty question, and `ask_carina`'s schema has `question.minLength: 1`. The plain-heading branch (`carina_memory_extraction.rs:168`, `question.is_empty()` → `None` → `TURN TRANSCRIPT:`) stays family-proven. |

### F — The scene note + the empty-message chained branch (P4.D243, P4.137; posed)

| # | owner | gesture | expected + evidence | status |
|---|---|---|---|---|
| F1 | CLAUDE | M6's multi-character chat, seats on `echo`; Continue after a character's reply | `Scene note:` as the LAST user message in the captured request, named from the human's seat | **PASS** — "DOGFOOD scene note" (`fc78979a…`; Charlie the user seat, Friday + Amy on `POSED echo`): `chatSend continueMode:true` → the continuing responder's (Amy's) captured request carries, as its LAST user message (followed only by the `[Amy]` name prefill), *"Scene note: Charlie's most recent message is the current state of the scene. Where any other speaker's line — before or after it — conflicts with what Charlie narrated, Charlie's account is what happened. Adjust without arguing; what you do about it is yours."* — named from the human's SEAT (Charlie), not the `{{user}}` name. |
| F2 | CLAUDE | a new user message's first responder | NO scene note | **PASS** — a new user message (*"Good evening, both of you."*): the first responder (Amy) has **no** scene note anywhere in its request (its last user message is `[Charlie] Good evening…`); the chained second responder (Friday) has exactly one, as its last user message. |
| F3 | CLAUDE | M6's 1:1 chat with a persona seat; Nudge | the scene note present | **PASS** — "DOGFOOD keys" (1:1, Charlie's persona seat + Friday): the SPA's Nudge shape (`continueMode:true, respondingParticipantId, nudge:true`) → Friday's request ends on the same `Scene note: Charlie's most recent message…` (no continue gate; the persona seat counts). |
| F4 | CLAUDE | the server log | DEBUG line with `resolvedFromSeat` true when the seat name differs from `{{user}}` | **PASS** — with `chat.context.user-narration-anchor=debug` added to `RUST_LOG` (the line's own target, outside `quilltap::`): a Continue in "DOGFOOD scene note" logged DEBUG `[UserNarrationAnchor] Scene note applies to this chained turn context=\"chat.context.user-narration-anchor\" historyWindowSize=22 lastHumanIndex=9 resolvedFromSeat=false` — correct: the seat is Charlie, and `userName` is the user character's own name (v4 `userCharacter?.name || 'User'`, `context-manager.ts:2717`). In "DOGFOOD two user seats" (`afa78408…`; Charlie + Abigail both user seats, Friday on the posed desk), speaking AS Abigail and then nudging Friday after her reply: `historyWindowSize=13 lastHumanIndex=7 resolvedFromSeat=true`, and the note reads *"Scene note: Abigail's most recent message…"*. |
| F5 | CLAUDE | dispatch `chatSend` with `content: ""` + `pendingToolResults` | no empty user line in the request; the trailing note alone when it applies; the TOOL row persisted, no USER row | **PASS** — dispatch `chatSend` `content:\"\"` + one `pendingToolResults` entry (v4's shape: `tool/success/result/prompt/arguments/createdAt`) into the scene-note chat: `TOOL` 0 → 1, `USER` unchanged at 1; the captured request carries `[Tool Result: rng]\\nRolled 1d20: 17`, then the Host's time line, then the scene note as the trailing note — **no** empty user line (P4.D243-F1's chained branch). |
| F6 | CLAUDE | dispatch image-only send on a vision seat after a prior human turn; then in a chat with no human turn | the image anchored on the prior human turn in the body; then v4's `Image attachments could not be anchored …` WARN, image dropped | **PASS (expectation corrected)** — (a) image-only send (`content:\"\"`, an uploaded image) in "DOGFOOD keys" on the junk Z.AI desk: the image rides the PRIOR human turn (`[Charlie] Friday, I agreed to take you to the lighthouse…` + `image_url`), no empty user line. (b) in a fresh chat with NO human turn (`b4ea1ef1…`): the image anchored on the Host's user-role `The Host marks the time as …` line, no WARN. **v4 does the same**: `selectAttachmentAnchorIndex` (`context-builder.service.ts:730-745`) falls through to ANY `role==='user'` message, and the Host/Prospero/Aurora notices are user-role, so the `Image attachments could not be anchored` WARN (`:1273`) needs a context with no user-role message at all — unreachable in a real chat. The lane's predicted WARN was the wrong expectation. |
| F7 | CLAUDE | a whitespace-only send with a file | the whitespace row persists (v4-faithful) | **PASS** — `content:\"   \"` + a file in "DOGFOOD keys": the USER row persists with content `'   '` and `attachments [\"ec4bb14c…\"]` (v4-faithful). |
| F8 | CLAUDE | the SPA composer: attachment-only post | RECORD what it sends (`''` expected — the deferred divergence) | **RECORDED (the deferred divergence, as predicted)** — the Salon composer, a PNG injected into its file input (its own `POST /api/v1/chats/{id}/files` upload, `5f0c6181…`) and the send button clicked: the dispatch body is `{\"type\":\"chatSend\",…,\"content\":\"\",\"fileIds\":[\"5f0c6181…\"]}` — v5 sends `''` where v4 sends *"Please look at the attached file(s)."*. Result: no USER row (count unchanged at 10) and no message carries the file (it is linked to the chat only). P4.137 item 11, phase-4 NEXT. |

### G — Boot hardness (P4.134, P4.135; server stopped for each plant)

| # | owner | gesture | expected + evidence | status |
|---|---|---|---|---|
| G1 | CLAUDE | the collapse: per M2 | expect `AlreadyCompleted` (no plant); plant the `BEFORE UPDATE OF generationKey` trigger ONLY if M2 allows → the boot fails with the singular `context=migration.` line; `DROP TRIGGER` | **PASS (no plant, per M2)** — the ledger row exists and no unkeyed roll remains, so the gate answered `AlreadyCompleted` silently: no collapse line at boot, the boot completed, `migrations_state` unmoved at 199. The plant's preconditions are absent on this instance (§5.5) — the fatal arm stays family-proven (`host_boot_hardness`). |
| G2 | CLAUDE | plant `doc_mount_file_links.relativePath → relativePath_x` (mount index); boot TWICE | both boots: ERROR `quilltap::db Failed to ensure doc_mount_file_links table in mount index database error=no such column: relativePath`; `/health` ready; `list_mail` as Friday answers v4's fallback lines (bare SQLite message); rename back | **PASS** — `doc_mount_file_links.relativePath → relativePath_x` (server stopped, `--mount-points --write`), booted TWICE: both boots logged ERROR `quilltap::db: Failed to ensure doc_mount_file_links table in mount index database error=no such column: relativePath` and came up `healthy` (boot 3 15:40:22Z, boot 4 15:41:11Z — the per-boot repeat is the recorded bug-176 ledger-gate divergence). `list_mail` as Friday (`chatRunTool`) answered *"Your postbox stands empty."* with v4's fallback line ERROR `Error querying joined file links collection=\"doc_mount_file_links\" whereClause=\"WHERE l.mountPointId = ?\" error=no such column: l.relativePath` — the BARE message. Renamed back. *Observation, not a finding:* under this plant `chatGet` and `listChats` answer 500 `sqlite error: no such column: l.relativePath` — a direct repository read outside the fallback homes, the class P4.131 recorded as OPEN (its 130-site census); not compared against v4 this walk. |
| G3 | CLAUDE | plant `roleplay_templates.name` renamed; boot | `Error seeding built-in roleplay templates`; the Salon still opens; rename back | **PASS** — `roleplay_templates.name → name_x` (server stopped): boot 5 logged ERROR `quilltap::db: Error seeding built-in roleplay templates collection=\"roleplay_templates\" error=no such column: name` and came up `healthy`; `listChats` 200, `chatGet` 200, and the Salon opened "DOGFOOD scene note" with its transcript rendered. Renamed back; boot 6 clean. |
| G4 | CLAUDE | the `state.json` WARN | carries `context` + the bare message (observe if it fires under G2) | **PASS** — under G2's plant, both boots logged WARN `quilltap::boot: Error ensuring general state.json, continuing startup context=\"instrumentation.register\" error=no such column: l.relativePath` — v4's `context` and the bare message. |

### H — Human remainder

| # | owner | gesture | why human | status |
|---|---|---|---|---|
| H1 | HUMAN | the standing queue: Lantern budget, a real token-limit turn, the four planted proofs, dedup/summaries, the Brahma deep query, #101, the compression re-measure, the conceal-marker arm, an autonomous room's budget, a real flat-body OpenAI image refusal | spend / long runs / not posable | DEFERRED-TO-HUMAN |

## §4 Findings from this walk

**No v5 defect found.** All 39 rows ran to a terminal status
(37 PASS — E4 run at the human's go-ahead — one RECORDED as the predicted divergence, one BLOCKED as unreachable);
H1 (the standing spend queue) stays with the human.

Non-findings recorded so nobody re-files them:

- **A real image spend on a throwaway chat.** Friday's live chat settings have
  the Lantern ON (`storyBackgroundsSettings.enabled: true`), so creating
  "DOGFOOD keys" queued a `STORY_BACKGROUND_GENERATION` job that ran one real
  `gpt-image-2.5-flare` call at 15:25:15Z (`concierge_image_failover: Image
  call answered first time purpose="lantern"`). v4-faithful behaviour; the walk
  then turned the Lantern off ON THE COPY. **Standing walk rule: disable the
  Lantern on the copy before creating any test chat.** A few early cheap-LLM
  calls (title, memory extraction) also ran on the real cheap profile before
  the cheap LLM was pointed at the posed desk.
- `SCENE_STATE_TRACKING` jobs fail with *"recognized but its handler is not yet
  available in the native runner"* — the job (`d5938a56…`) came from v4's queue
  in the copy; the handler is a NAMED phase-4 deferral (`phase-4.md`, "Named,
  not order-sized yet"), not a regression.
- **F6's predicted WARN was the wrong expectation.** v4's anchor falls through
  to ANY user-role message, and the Host's time line and the Prospero/Aurora
  notices are user-role, so `Image attachments could not be anchored` needs a
  context with no user-role message at all — unreachable in a real chat. v5
  anchors exactly where v4 does.
- **E6 is unreachable live** (both empty-question entry points refuse an
  empty question); family-proven.
- **The current-message attachment path never loads a vault link id** (it
  reads chat-linked `files` rows only — v4's `loadAndProcessFiles`); a vault
  image reaches the wire through the bug-121 rehydration of prior user rows
  and the Lantern walk, both of which use the mount fallback. v4-faithful.
- **Under G2's plant, `chatGet` / `listChats` answer 500** (`sqlite error: no
  such column: l.relativePath`) — a direct repository read outside the
  fallback homes, the class P4.131 left OPEN (its 130-site census). The boot,
  `/health`, and `list_mail` — G2's contract — all hold. Not compared against
  v4 this walk.
- REST `POST /api/v1/models` and `POST /api/v1/connection-profiles?action=
  test-message` answer 405: v5 serves the settings family over dispatch only.
  Instrument note.
- The instrument grew a `json` mode (`refusal-server.py`) so the multi-step
  generators can reach their later steps on a posed desk, and the walk's
  bug-174 proof used a junk-key profile's `baseUrl` pointed at the posed
  server, which captures the real WIRE body (stronger than the order's
  pre-call log line, which the stream path does not emit).

## §5 Copy state left behind (disposable; the next rsync resets it)

The Lantern OFF; the cheap LLM on `POSED echo`; the Concierge's uncensored
text desk on `POSED understudy k-first`; `JUNK NanoGPT`'s key row corrupted
(`x'00000000'`); keys `POSED k-first` / `POSED k-bound` / `JUNK NanoGPT` /
`JUNK Z.AI`; posed/junk/keyless profiles; four `DOGFOOD …` chats. Every plant
(the two column renames) was reversed; `migrations_state` 199 throughout.
