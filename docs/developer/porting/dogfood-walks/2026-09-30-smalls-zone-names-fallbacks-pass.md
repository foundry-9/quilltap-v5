# Dogfood walk — the `97b25fc53` smalls round: the threaded display zone,
# the `name` hash slot + catch lines + SDK frames, the Zod smalls, the
# repository fallbacks, the SPA smalls (2026-09-30, evening)

**Instance:** `~/qt-dogfood-friday` — a COPY of real Friday data, refreshed by
the human at 19:44 CDT (migrations 199, newest 2026-09-26 — the same vintage
as this morning's walk). Disposable; never rsynced back.
**Server:** `RUST_BACKTRACE=1 RUST_LOG='info,quilltap::=debug,quilltap_core::tools=debug'
./target/release/quilltap-web --data-dir ~/qt-dogfood-friday --spa-dir
apps/web/dist/quilltap/browser` — with **NO `TZ`** in the environment (P4.127's
point), and later restarted under `TZ=UTC` for B4.
**Log of record:** the scratchpad server log + `~/qt-dogfood-friday/logs/combined.log`.
**Instrument:** `harness/tools/refusal-server.py` on `127.0.0.1:8898` — grown
this walk by two modes: `malformed` (a first `data:` frame that is not JSON —
P4.128's SDK frame semantics) and `unauthorized` (a pre-stream 401 — P4.128's
catch line).
**Host zone:** CDT (`America/Chicago`).

**Round covered:** the `97b25fc53` smalls round (unified 2026-09-30) — P4.127
(the display zone threaded) · P4.128 (the `name` hash slot, the pre-stream
catch line, SDK frame semantics) · P4.129 (orchestrator oracle — harness-only,
no live row) · P4.130 (Zod smalls: code points, ZodError bytes, the strict
trail twin) · P4.131 (document-store repository fallbacks, the bare SQLite
message) · P4.132 (SPA smalls: the end-of-text-run replacement rule, one memory
badge, the Edit Content memory pair).

## §0 Drift state

The ledger's §2 freshness probe at walk planning (2026-09-30 19:45): v4 `main`
**AT** the baseline `97b25fc53`, `bugfix` unmoved at `1a2b2164c`, checkout on
`main`, **§3 EMPTY**. One untracked file in v4's tree
(`docs/developer/features/prompt-trust-and-anti-committee.md` — a design doc,
no code, no surface this walk touches). No step may blame drift. (Strictly the
tree is "dirty" for the regen rule; this walk regenerates no oracle.)

## §0.5 Pre-walk measurement (read-only, BEFORE the first boot)

| # | population | measured | consequence for the walk |
|---|---|---|---|
| M1 | `migrations_state` | **199**, newest 2026-09-26 13:01Z | v5 must add none; re-count after boot (A2). |
| M2 | `groups` | **4** rows | E1 creates a fifth (the emoji name); E2 plants on it, never a real group. |
| M3 | `chat_messages.routeTrail` | 23 rows, **all ASSISTANT**, none TOOL — no real picture message carries a trail | E3's bad-trail plant goes on a throwaway posed chat's message; the retry-picture 404 arm has no real subject (family-covered). |
| M4 | small chats with memories | `091528a8…` "Chat with Sunny Brevity" — 10 messages, **1** memory | G2/G3's subject (re-extract, then Delete Memories (n)). |
| M5 | Friday's postbox | the 2026-08-30 19:40:18Z letter (`1788118818964-from-abigail.md`) — read live at B1 | B1/B4's `02:40 PM` / `07:40 PM` targets. |
| M6 | name-supporting providers (manifests) | DEEPSEEK, GROK, NANOGPT, OPENAI, OPENAI_COMPATIBLE, Z_AI | C1 seats a posed OPENAI_COMPATIBLE profile in a two-character chat, so the wire capture lets the hash be recomputed. |

## §1 What is NOT live — do not report these as bugs

From the six orders' Unification headers:

- **P4.127:** on a host whose `TZ` is a POSIX string (`CST6CDT`), the
  name-fed sites (every executor tool, Carina, the `send_mail` preface) fall
  to UTC while the value-fed ones keep the zone — **RULED (Option V) to be
  closed by the next smalls round's item (k)**; B5 measures it only. The two
  core zone-NAME wrappers (`autonomous_rooms`, `markdown_transcript`) remain.
- **P4.128:** the `name` is HASH-only — no plugin sends it on the wire and
  the logged `request` omits it (v4 too). A NON-streaming reqwest timeout
  logs `Connection error.` (v4 `Request timed out.`); a 2xx `{"choices":[]}`
  answers empty content where v4 throws — both recorded, unpinned. GOOGLE /
  OLLAMA / OpenRouter-raw log none of their own plugin ERROR lines (nine
  unported). Anthropic / Google malformed frames unmeasured.
- **P4.129:** Carina's consultation logs **no** `CHAT_MESSAGE` row where
  v4's funnel logs one (pinned both ways; ordered as NEXT 2(d)).
- **P4.130:** the import mask's non-chat entity families answer v5's decode
  sentence where v4 answers a ZodError tail; the restore orchestrator's serde
  arm; the trail WARN's `errors` is a first-failure stand-in.
- **P4.131:** `mount_embedding_effects`' chunks read still propagates where
  v4 falls back; `send_mail`'s overlay-backed recipient resolve fails hard
  where v4 fails soft (pinned both ways); 130 direct repo sites not
  converted.
- **P4.132:** `onDeleteMemories` has no unit coverage; the bold-run
  converse is selection-order dependent (both-ways divergence); the bare
  `Failed to …` toast is a deliberate divergence.
- Standing: a bare `?action=` → 400; a swipe writes no `llm_logs` row; the
  WaveSpeed npm plugin is unloadable (#124); #130–#132 v4-faithful.

## §2 Server launch

```
RUST_BACKTRACE=1 RUST_LOG='info,quilltap::=debug,quilltap_core::tools=debug' \
  ./target/release/quilltap-web --data-dir ~/qt-dogfood-friday \
  --spa-dir apps/web/dist/quilltap/browser
```

## §3 The walk

Status: `PENDING` → `PASS` / `FAIL(#n)` / `DEFERRED-TO-HUMAN` / `BLOCKED(reason)`.

### A — Boot (zero spend)

| # | owner | gesture | expected + evidence | status |
|---|---|---|---|---|
| A1 | CLAUDE | boot on the copy with `TZ` unset | clean claim, no panic, unlocked | **PASS** — boot at 00:49:09Z with `TZ` unset (`env -u TZ`); FTS reconcile 83,112/83,112, help docs 129 unchanged, no render-reconcile work owed, no panic; the copy has no passphrase, so the engine unlocked at boot. |
| A2 | CLAUDE | `migrations_state` after boot | = M1 (199) | **PASS** — 199. |

### B — The threaded display zone (P4.127, near-zero spend)

| # | owner | gesture | expected + evidence | status |
|---|---|---|---|---|
| B1 | CLAUDE | `list_mail` as Friday (`chatRunTool`), NO `TZ` | the 2026-08-30 letter reads **`August 30, 2026 at 02:40 PM`** | **PASS** — `chatRunTool list_mail` as Friday in "The Trustee's Head Count" (`c22d1a65…`): *"1. From Abigail — August 30, 2026 at 02:40 PM"* (19:40:18Z → CDT), with no `TZ` in the server's environment — the zone now comes from `HostConfig`'s one read, threaded as a value. |
| B2 | CLAUDE | `read_conversation` as Friday on a chat she sits in | `Current time:` and message times in CDT | **PASS** — on "Two on the Way at the Salon" (`ec7322be…`): *"Current time: September 30, 2026 at 7:49 PM"* (the server clock read 00:49Z) and *"This conversation occurred on September 29, 2026 from 9:14 AM to 11:24 AM"* — CDT throughout. |
| B3 | CLAUDE | one Scenario Builder run (a few calls) | the prompt's `## Now` line carries the CDT wall time (`llm_logs` `SCENARIO_BUILDER` request) | **PASS** — run from New Chat's *Ask the Host to set the scene* on the posed `echo` desk (zero spend; Location "the Lantern Inn at Vey's Crossing", Time "now"): the captured system prompt reads *"The current date and time is 2026-09-30T19:51:06-05:00. Use it as the reference for "now"…"* — the CDT offset, with the server clock at 00:51Z — the spine's `Zoned::now()` read retired onto the threaded zone (the §3 review's fix). The dialog delivered the scene (`ok`) and was cancelled. |
| B4 | CLAUDE | restart under `TZ=UTC`; repeat B1 | the same letter reads **`07:40 PM`**; restart without `TZ` afterwards | **PASS** — the same binary under `TZ=UTC`: *"1. From Abigail — August 30, 2026 at 07:40 PM"* (= 19:40:18Z). The zone is read once at boot from the environment; nothing else moved. |
| B5 | CLAUDE | (measure only) restart under `TZ=CST6CDT`; `list_mail` + `read_conversation` | record each surface's zone — the ruled Option-V divergence, not a finding | **MEASURED — the divergence is wider than recorded.** (1) `TZ=CST6CDT` is ALSO a legacy IANA zone name, so it is not a POSIX-only case: every surface stayed CDT (`02:40 PM`; *"Current time: September 30, 2026 at 8:10 PM"*). (2) A genuinely non-IANA POSIX rule, `TZ='XST6XDT,M3.2.0,M11.1.0'` (= CDT today): **every** surface fell to UTC — `list_mail` *"07:40 PM"*, `read_conversation` *"from 2:14 PM to 4:24 PM"* / *"Current time: October 1, 2026 at 1:10 AM"*, AND the value-fed Scenario Builder clock *"2026-10-01T01:10:42+00:00"*. The recorded scope (name-fed sites fall, value-fed ones keep the zone) is not what this host does: the VALUE itself resolves to UTC, so Option V's threading alone would not close it — item (k) must also make `HostConfig`'s one read honour a POSIX `TZ` (v4's Node/ICU does). Routed to the order (§5). |

### C — The `name` hash slot, the catch line, SDK frames (P4.128, posed + one real desk)

Setup: `OPENAI_COMPATIBLE` profiles on `http://127.0.0.1:8898/v1`, model names
`echo` / `unauthorized` / `malformed`, a dummy key.

| # | owner | gesture | expected + evidence | status |
|---|---|---|---|---|
| C1 | CLAUDE | a two-character chat, both LLM seats on the posed `echo` profile; two user turns | the second turn's `CHAT_MESSAGE` row's `historyTailHash` equals the hash recomputed from the captured wire messages **with** each message's speaker `name` — and NOT the name-less recompute (the slot is hash-only: the captured wire carries no `name`) | **PASS** ⭐ — "Chat with Friday and Amy" (`2a9e6e08…`; Charlie as the user seat, Friday and Amy both on `POSED echo`): one user turn, the chain ran Friday then Amy. The captured wire messages carry **no** `name` key (v4's plugins send none). Recomputing v4's `historyTailHash` (SHA-256/16 of the frozen non-system messages' `{content,name,role,toolCallId,toolCalls}`) over the captured messages: Friday's row `9fc8569a…` stores `8414af4321721e33` — reproduced only with each `[Speaker]`-prefixed history message's `name` set (name-less: `7c49f891…`); Amy's row `022342fc…` stores `b1e63ea88c842963` — reproduced only with the speakers' names AND her own earlier assistant message named `Amy` (name-less: `1c591de3…`; prefix-only: `1394882f…`). The name reaches the hash on real turns and nowhere else. Script: scratchpad `c1.py`. |
| C2 | CLAUDE | one turn on `unauthorized` | ERROR `OpenAICompatible API error in streamMessage` with the SDK's `401 Incorrect API key provided: dogfood.` bytes, before the failover line | **PASS** — Friday's seat switched in the Salon sidebar to `POSED unauthorized`: ERROR `quilltap::model::streaming_provider: OpenAICompatible API error in streamMessage context="OpenAICompatibleProvider.streamMessage" baseUrl=http://127.0.0.1:8898/v1 error=401 Incorrect API key provided: dogfood.` — the openai SDK's `APIError` message (`${status} ${message}`) — then WARN `[Failover] Primary call failed … trigger="auth"` and `Fallback chain exhausted` (the posed profile has no understudy). |
| C3 | CLAUDE | a NANOGPT profile on a junk key (a new `api_keys` row), one turn | NanoGPT's catch line with its 401 bytes (the real pre-stream 4xx) | **PASS — after #133.** The first try **answered** (a real DeepSeek reply on a junk key): v5's host key source sends the provider's FIRST active key, not the profile's `apiKeyId` (#133). With the real `NanoGPT QT` key briefly deactivated (`apiKeyUpdate isActive:false`, restored right after): ERROR `NanoGPT API error in streamMessage context="NanoGPTProvider.streamMessage" baseUrl=https://nano-gpt.com/api/v1 error=401 Invalid session` → `[Failover] … trigger="auth"` → chain exhausted; and the cheap-LLM calls on the same key logged the `sendMessage` twin, `NanoGPT API error in sendMessage … error=401 Invalid session` (one of P4.128's three ported `sendMessage` lines) before `Cheap-LLM call failed` / `[CheapLLM] Task failed`. NanoGPT's own 401 body read directly with the junk key: `{"error":{"message":"Invalid session",…}}` — the `Invalid session` the line carries. |
| C4 | CLAUDE | one turn on `malformed` | the stream FAILS (no longer skips the frame) with the SDK's `SyntaxError` bytes; record the trigger + whether a failover runs | **PASS** — Friday on `POSED malformed` (first frame `data: {"choices": [`): ERROR `OpenAICompatible API error in streamMessage … error=Error reading response: malformed server-sent event JSON.` → WARN `[Failover] Primary call failed … trigger="provider-error"` → chain exhausted; the send answered 500 (the failed-initial-turn shape). Before P4.128 the decoder skipped the frame and the turn ended empty. Instrument note: the first send on this seat was recorded WITHOUT a turn — the previous chain error had left the chat paused (`[Orchestrator] Chat paused — recording the user message without a reply`); resumed + *Nudge Friday* ran it. |

### E — Zod smalls (P4.130, zero spend)

| # | owner | gesture | expected + evidence | status |
|---|---|---|---|---|
| E1 | CLAUDE | create a group through the UI named 99 × `x` + `🎩`; add a character; reopen it | saved; reads back whole; its store kept (the tiered pool reads it with no `Data validation failed`) | **PASS** — Characters → *Create Group*, the name typed with real keystrokes (`xxx…x🎩`: 101 UTF-16 units, **100 code points**; the input has no `maxlength`) → toast *"Group created successfully!"*; the row stores 100 characters and an `officialMountPointId` (`038a3282…` — the store was provisioned); `groupGet` reads the whole name back ending `xx🎩`; Friday added (`groupMemberAdd`). A Scenario Builder build naming the group (`groupIds`, the posed `echo` desk) completed with **no** `Data validation failed` — the same validated read E2 then trips. Before P4.130 v5 measured UTF-16 units and refused this row (101 > 100) where v4 keeps it. |
| E2 | CLAUDE | plant (server stopped, `--write`) that group's `name` to 101 × `x`; drive a read of the group tier (a Scriptorium listing as its member) | ERROR `Data validation failed collection=groups error=[…]` — v4's `JSON.stringify(issues, null, 2)` bytes (`too_big`, `maximum: 100`) — then the fallback `Error finding entity by ID`; the listing degrades, no 500 | **PASS** — planted `name` = 101 × `x` (server stopped, `quilltap db --write`); the same build: ERROR `quilltap::db: Data validation failed collection="groups" error=[\n  {\n    "origin": "string",\n    "code": "too_big",\n    "maximum": 100,\n    "inclusive": true,\n    "path": [\n      "name"\n    ],\n    "message": "Too big: expected string to have <=100 characters"\n  }\n]` — zod's `JSON.stringify(issues, null, 2)` bytes (P4.124 had logged a v5 sentence) — then `Error finding entity by ID collection="groups" id=1a0bf735…` with the same bytes, on the home's `quilltap::db` target; twice (the route's group check, then the mount pool). The run answered `done` — the group was dropped, nothing 500'd. (Driven through the builder, not a Scriptorium listing: `doc_list_files` needs a project context or a pre-built pool.) |
| E3 | CLAUDE | plant a posed chat's assistant message `routeTrail` with a bad `via`; open the chat | WARN `Skipping corrupted chat message`; the message absent from the GET; the chat still opens | **PASS** — planted Amy's `ok` (`f7214932…`) with a one-row trail whose `via` is `bogus`: `chatGet` answered the chat with 46 messages, `f7214932…` absent and its sibling `df8bda71…` present; WARN `quilltap::db: Skipping corrupted chat message context="db.chats-messages" … messageId="f7214932…" error=routeTrail.0.via: Invalid option` — the strict `RouteAttemptSchema` twin at the READ (P4.D228's premise corrected). |

### F — Repository fallbacks (P4.131, zero spend)

| # | owner | gesture | expected + evidence | status |
|---|---|---|---|---|
| F1 | CLAUDE | plant (server stopped) `doc_mount_file_links.relativePath → relativePath_x` on the mount index; `list_mail` as Friday | v4's fallback lines with the **bare** SQLite message (`no such column: …`, not `sqlite error: …`) on the `quilltap::db` target; the tool answers rather than 500s. Rename back afterwards and confirm B1 again | **BLOCKED (#134)** — with the plant in place **v5 does not boot**: the built-in seed's link case-repair (`mount_index_case_repair::ensure_link_nocase_unique_index` → `repair_link_case_collisions`, `SELECT … relativePath …`) fails with `no such column: relativePath`, and `seed_built_ins` propagates it with `?`, so the engine never assembles and every dispatch answers 503 — where v4 wraps each boot step in try/catch and continues (`instrumentation.ts:767-797`, *"… continuing startup"*). And `/health` then answered 409 **`lock-conflict` "held by PID 69127 on this host" — the server's own PID** — with the real error in no log. The second half is fixed in place (#134); the boot-hardness half is ordered. The rename was reversed (server stopped) and B1-class reads re-run clean on the next boot. |

### G — SPA smalls (P4.132, near-zero spend)

| # | owner | gesture | expected + evidence | status |
|---|---|---|---|---|
| G1 | CLAUDE | composer: type `teh world`, caret back after `teh`, Space | **no** rewrite mid-run (v4's end-of-text-run rule — the divergence P4.125 left open, now closed); then `teh` + Space at the end of the run rewrites | **PASS** — in the Salon composer ("Chat with Friday and Amy"), real key events: typed `teh world`, **Left ×6** (caret after `teh`), **Space** → the editor reads `<p>teh  world</p>` (no rewrite mid-run — the P4.125 divergence closed); then **End**, **Space t e h Space** → `<p>teh  world the </p>` (rewritten at the end of the run). Instrument note: the browser tool's `type` action inserts text without a Space keydown, so its own spaces never trigger the rule — drive the trigger with `key`. Composer cleared. |
| G2 | CLAUDE | Salon sidebar → Edit Content → **Re-extract Memories** on M4's chat | jobs queued only (no delete); v4's count toast | **PASS** — "Chat with Sunny Brevity" (`091528a8…`) → sidebar *Edit Content* shows *Re-extract Memories* and *Delete Memories (1)*; Re-extract: confirm *"Queue memory extraction jobs for all messages in this chat? This will process the entire conversation history."* (`window.confirm` stubbed to record + accept), the wire carried **only** `chatQueueMemories` (no delete), toast *"Queued 1 memory extraction jobs"* — both byte-identical to v4 (`useMemoryActions.ts:73,92`). The job ran (`MEMORY_EXTRACTION` COMPLETED + two embeddings); the chat's memories 1 → 3. |
| G3 | CLAUDE | **Delete Memories (n)** on the same chat once the count is real | the confirm text, the memories gone (`memories` count 0), the count invalidated | **PASS** — the item now read *Delete Memories (3)*; clicking it sent `memoryCountByChat` FIRST (v4's ask-the-server probe, `useMemoryActions.ts:23-33`), then the confirm *"Delete all 3 memories created from this chat? This action cannot be undone."*, then `memoryDeleteByChat`, then two `memoryCountByChat` re-reads; toast *"Deleted 3 memories"*; `memories` for the chat → **0**. This is also the Tier-3 live arm the P4.132 review named ("the beat never proves `memoryCountByChat` went out"). |
| G4 | CLAUDE | the memory badge on a card (one `qt-memory-badge`) | renders on the Salon list card and the character's chats tab alike | **PASS** — Salon list: 917 `qt-chat-card`s, 917 `qt-memory-badge`s; Friday's character view → *Conversations*: 10 `qt-character-conversation-card`s, all 10 carrying `qt-memory-badge` with the same tooltip *"Memories — click to delete and re-extract"*. |

### H — Human remainder

| # | owner | gesture | why human | status |
|---|---|---|---|---|
| H1 | HUMAN | the standing queue: Lantern budget, a real token-limit turn, the four planted proofs, dedup/summaries, the Brahma deep query, #101, the compression re-measure, the conceal-marker arm, an autonomous room's budget, a real flat-body OpenAI image refusal | spend / long runs / not posable | DEFERRED-TO-HUMAN |

## §4 Findings from this walk

| # | finding | class | disposition |
|---|---|---|---|
| **133** | A profile's own API key is ignored: the host key source sends the provider's FIRST active key (`DbProviderKeys::key_for(provider)`), so a NANOGPT profile bound to a junk key answered with the real one (C3) | **Port divergence** — a documented host seam (`spine.rs` module header), measured live for the first time; Friday holds one key per provider today, so no real turn is misrouted yet | ORDERED-PENDING — `dogfood-findings.md` Standing notes. |
| **134** | (a) a boot failure after lock acquisition was served as `lock-conflict` "held by PID <self>", the real error in no log; (b) a damaged mount index (P4.131's column-rename plant) kills the boot, where v4 warns and continues per step (F1) | (a) **v5-only defect**; (b) **port divergence (boot hardness)** | (a) **FIXED** in place (`classify_boot_failure`, web 0.0.205, a new test in `lock_conflict_boot_status.rs`, mutation-proven); (b) ORDERED-PENDING — Standing notes. |

Non-findings recorded so nobody re-files them:

- **Two real chats' messages are skipped on every read** — WARN `Skipping
  corrupted chat message … error=createdAt: Invalid ISO datetime ("")`
  (`1762da60…`/`f2e0170b…`, `51660a07…`/`2ab36187…`): rows with an EMPTY
  `createdAt`, refused by v4's `ChatEventSchema` the same way (the P4.113
  per-row skip). Data damage on the instance, v4-faithful.
- B5's POSIX-`TZ` result is the ruled Option-V divergence, wider than
  recorded (every surface falls, value-fed ones included) — routed to item
  (k), not a new finding.
- The browser tool's `type` action inserts text without a Space keydown, so
  text replacements never fire on its spaces (G1) — an instrument property.
- After C3's failed chain the chat was paused, so C4's first send was
  recorded without a reply until resumed. Not investigated against v4 this
  walk (observed once; outside this round's surfaces) — a candidate for the
  next walk's checklist, not a filed finding.
