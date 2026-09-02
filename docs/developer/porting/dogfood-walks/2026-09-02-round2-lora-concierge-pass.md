# Dogfood walk — the round-2 drift catch-up + the P4.D138 follow-up

**Date:** 2026-09-02 · **Instance:** `~/qt-dogfood-friday` (a COPY of Friday;
never rsynced back) · **Driver:** Claude, with a short HUMAN remainder.

**Rounds under test**

- The round-2 drift catch-up (P4.D138 ∥ P4.D139 ∥ P4.D140 ∥ P4.D141 ∥
  P4.D142 ∥ P4.66), unified 2026-09-01.
- The P4.D138 follow-up (units 5–7, the resumed LoRA-train lane), unified
  2026-09-01.
- Carry-over 💸 from the round-1 catch-up (P4.D134 ∥ P4.D135→P4.D136 ∥
  P4.D137) and the standing queue.

---

## ⚠ Drift note — read before calling anything a defect

The drift ledger was **stale at walk start**. `/driftcheck` ran first
(`28245beb`) and found **one** v4 commit past the baseline:

| sha | subject | class |
|---|---|---|
| `70505745a` | fix(images): keep absent characters out of story backgrounds | **PORT**, UNPROCESSED |

Two surfaces in this walk therefore have a pending §3 drift row. **An
apparent failure on either is the drift, not a v5 defect** — diagnose against
`git show 4622411fd:<path>`, note the row, and do not port ahead of the
catch-up round:

1. **Story-background generation.** v4 now filters chat participants on
   `isParticipantPresent` at both enqueue sites (an *Absent* or soft-removed
   character was being painted into the frame), excludes them from the
   prompt back-fill's candidate pool, and answers "No characters **present**
   in chat to generate background for." v5 still carries the pre-fix
   behaviour and the pre-fix sentence (`api/chat_media.rs:1353`). **Expected.**
2. **Project background display mode.** v4 narrowed the enum to
   `latest_chat | theme`, retiring `'project'` and `'static'` (neither ever
   produced an image) with a coercing preprocessor. v5 still offers all four
   in the project Image Generation card. **Expected.**

Regen rule for anything this walk fixes: **PIN REQUIRED** at `4622411fd`
(ledger §5.1).

---

## Pre-walk measurements (taken 2026-09-02, before the server booted)

Read-only queries against the copy. These decide what several steps can and
cannot prove (ledger §5.5 — v4 runs daily on the real instance and heals data
out from under banked proofs).

| what | measured |
|---|---|
| chats | **885** |
| `lastMessageAt` rows drifted from the character-authored recompute | **13** (deltas 3–170 ms); **0** would clear to NULL |
| `migrations_state` row `recompute-chat-last-message-at-v1` | **ALREADY PRESENT** — written by **v4** on 2026-08-30, `4.9.0-dev.103`, `itemsAffected: 608`, "Recomputed last-activity for 608 chats (0 with no character-authored messages)" |
| `migrations_state` row `widen-concierge-override-domain-v1` | present, v4, 2026-08-31, `4.9.0-dev.108` |
| `chats.conciergeOverride` | 873 `NULL` · **10 `'OFF'`** · **2 `'UNCENSORED'`** |
| image profiles | 14; **2 NANOGPT** — `FLUXNSFWunlock` (`flux-lora`) and `Klein Uncensored` (`flux-2-klein-9b`) |
| NanoGPT image `parameters` | **both already carry a v4-written `loras` array** (`{source, scale, triggerPhrase}`) |
| connection profiles with a fallback chain | **16** (`fallbackProfileId` set, `allowTierFallback = 1`) |
| projects | 8 |

**Two consequences the plan is built around.**

- **A1/A2 split.** Because v4 already wrote the recompute ledger row, v5's
  heal (completed-check FIRST, P4.D97 shape) will **SKIP** on this copy. That
  is itself the cross-app proof the whole ledger mechanism exists for — so
  A1 asserts the skip, and A2 then deletes the row on the copy (disposable)
  to force the run and watch the 13 measured rows heal.
- **C1 is a free cross-implementation proof.** v4 wrote both `loras` bags
  before the copy was taken, so v5's LoRA list editor is reading v4's own
  bytes back.

---

## What NOT to expect to work (refusal-armed / unported, per the orders)

- **Message-bubble danger styling** — never ported (the round's §C was
  measured as a NO-OP with evidence). Only the sidebar/header carry the
  Concierge state; do not report a missing red bubble.
- **`imageProfileOptionsSchema` / `imageProfileLoraMetadata`** — these
  reached the SPA at P4.D139 and the server at the P4.D138 follow-up; if the
  options panel falls back to the legacy panel, that is now a defect, not a
  deferral.
- **"Summon From Lore" / AI import** — a documented disabled stub.
- **Project-generated / Static uploaded background modes** — see the drift
  note; still offered in v5, retired in v4.
- **v4's e2e `--arm`/`--recheck` scan-tick guard** — deliberately not
  reproduced; its substance is pinned at the differential.

---

## Part A — bug 112: chat activity by when a character last spoke (P4.D140)

| # | owner | step | gesture | expected + verification | status |
|---|---|---|---|---|---|
| A1 | CLAUDE | The boot heal **respects v4's ledger row** | Boot `quilltap-web` against the copy with the row present; watch the server log. | No `Recomputed last-activity…` line; `migrations_state` still has exactly one `recompute-chat-last-message-at-v1` row and it is still v4's (`4.9.0-dev.103`, 608). The 13 drifted rows are untouched. Cross-app proof of the completed-check ordering. | **PASS** — boot log carries **no** activity/recompute line at all (grep for `last-activity|recompute|chat_activity` returns nothing); the ledger row is still v4's verbatim (`2026-08-30T13:21:18.418Z`, `4.9.0-dev.103`, 608); the drift query still returns **13**. The completed-check ordering is proven **cross-app**: v5 honoured a row only v4 ever wrote. |
| A2 | CLAUDE | The heal **runs** and heals the measured 13 | Stop the server; `DELETE FROM migrations_state WHERE id='recompute-chat-last-message-at-v1'` on the copy; restart. | Log carries v4's byte-exact sentence with the plural `chats` and a count of **13**; re-running the drift query returns **0**; a fresh ledger row appears stamped with v5's version. | **PASS** — boot logged `Recomputed chat last-activity from character-authored messages updated=13 cleared=0` (exactly the measured population, including the measured `cleared=0`); drift query → **0**; new ledger row `2026-09-02T04:18:10.214Z`, `quilltapVersion: 0.0.736`, `itemsAffected: 13`, message `Recomputed last-activity for 13 chats (0 with no character-authored messages)` — the same sentence shape as v4's own 608-chat row, pluralized correctly. |
| A3 | CLAUDE | A no-drift boot writes **NO** ledger row | Delete the row again (so the completed-check cannot be what skips it) and restart with drift already at 0. | `RecomputeOutcome::NoDrift` — no log line and **no** `migrations_state` row. | **PASS** — no `Recomputed…` line; the row count for that id stayed at **0**. The cross-app hazard is closed by measurement: v5 declined to stamp a migration a later v4 boot would then skip. |
| A4 | CLAUDE | The date the UI shows walks back past non-character events | Query for the **largest** walk-back gap on real data, then find that row in the Chats list. | The list shows the character-authored date, not the newest event's. | **PASS** — the largest real gap is **224 days**: *Phaser + TypeScript Plan for Ranch Rush Clone* has its last character message at `2025-10-31T11:28:00Z` and four `type='system'` events at `2026-06-12T17:02Z`. The Chats list renders **10/31/2025** and *sorts the row there*, among the other Oct/Nov 2025 chats — so both the label and the ordering read activity, not `updatedAt`. (Its neighbour *Local sidecar proxy for Claude via OpenRouter* is the same shape, also 10/31/2025.) |

## Part B — the Concierge four-state per-chat control (P4.D141)

| # | owner | step | gesture | expected + verification | status |
|---|---|---|---|---|---|
| B1 | CLAUDE | Real `UNCENSORED` and `OFF` chats read correctly | Open one of the **2** real `UNCENSORED` chats and one of the **10** `OFF` chats. | The sidebar control shows Uncensored / Vouched respectively (`OFF` → vouched; `NULL` → Monitored). Read the select's value via `read_page`. | **PASS** — *All-or-Nothing at the East Pool* (`UNCENSORED`): header pill **Uncensored**, select `value="uncensored"` selected, all four options present under v4's two optgroups (`Monitored`/`Flagged`, `Vouched Safe`/`Uncensored`), helper text *"You have sent the Concierge away and opened the uncensored door yourself…"*. *The Ledger of Skin and Water* (`OFF`): header pill **Vouched Safe** — the `OFF` → vouched mapping on real v4-written data. |
| B2 | CLAUDE | All five manual sentences, byte-exact | On a throwaway chat, flip through the transitions that produce each of the five kinds. | Each flip posts a `systemSender='concierge'`, `systemKind='danger'` message whose `content` byte-matches `build_manual_content` and whose `opaqueContent` matches `build_manual_opaque_content` (note the U+2019 in the vouched advisory). Compare with SQL. | **PASS** — five flips from `vouched` (→ monitored → flagged → monitored → vouched → uncensored) wrote exactly **five** concierge messages, one per kind, in the expected order (resumed / flagged / safe / vouched / uncensored). **All ten strings byte-equal** to the Rust constants, U+2019 intact in the vouched advisory. ⚠ the first comparison run reported a false DIFFERS — my script had `unicode_escape`-mangled the Rust literal; the *instrument* was wrong, not the app (the standing "prove the instrument" rule). |
| B3 | CLAUDE | The classifier stands down on an operator-state chat | Send one real turn on an `UNCENSORED` chat. | No `CHAT_DANGER_CLASSIFICATION` job enqueued for it (`background_jobs` query, windowed); the turn still completes. | PENDING |
| B4 | CLAUDE | The Uncensored route is actually taken | Same turn as B3. | `llm_logs` shows the uncensored profile, not the ordinary one; and no danger-scan row. | PENDING |
| B5 | CLAUDE | The `conciergeState` PUT tri-state + guard order | `chatUpdate` over `/api/dispatch` with (a) a bogus state, (b) an explicit `null`, (c) a valid state on a missing chat, (d) both wrong at once. | (a) 400, (b) refused not silently kept, (c)+(d) **404 before 400**. Nothing written on the refusals. | **PASS** — (a) `bad-request` / `Validation error`; (b) explicit `null` also `bad-request` / `Validation error` — the present-but-null tri-state is **refused**, not silent-kept (the class three sibling lanes got wrong this round); (c) and (d) both `not-found` / `Chat not found`, so the existence gate runs **before** the body parse in both orders. After all four the chat was still `UNCENSORED` with exactly the 5 concierge messages — nothing written. |
| B6 | CLAUDE | The sidebar select is not permanently latched | Five successive picks, then change the state **externally** (curl → `monitored`) and reload. | The select follows the server, i.e. the §3 review's fixed latch stays fixed on real data. | **PASS** — every one of the five picks stuck (each wrote its own message and moved the column), and after the external change + reload the select reads **Monitored** and the header pill is gone. The permanent latch is gone. **Also observed, and v4-faithful:** on *tab re-activation* the transcript refetched (6 concierge chips) but the chat record did not, so the select/pill stayed stale until the reload — `tab-refetch.ts` leaves the `salon` kind **deliberately empty**, quoting v4's own reasoning ("live surfaces fed by SSE/PTY; a blanket invalidation risks disturbing an in-flight stream"). Not a defect. |

## Part C — the LoRA train (P4.D138 units 1–7 + P4.D139 client half)

| # | owner | step | gesture | expected + verification | status |
|---|---|---|---|---|---|
| C1 | CLAUDE | v4's own LoRA bags render | Settings → Images → edit `FLUXNSFWunlock`. | The LoRA list editor shows `shahtab/FLUXNSFWunlock`, scale **0.8**, trigger `aidmaNSFWunlock`; `Klein Uncensored` shows its own row. Free cross-implementation proof. | **PASS** — Adapter 1 renders v4's bytes exactly: source `shahtab/FLUXNSFWunlock`, *Strength — 0.80* on a themed `qt-range`, trigger `aidmaNSFWunlock`, with `1 of 1` and "This model accepts a single adapter." resolved from the server's per-model cap. |
| C2 | CLAUDE | The schema-driven options panel, not the legacy fallback | Same modal. | The panel is rendered from the server's `options-schema` action (network tab shows a 200, not a 400 → legacy fallback). | **PASS** — a **NanoGPT Image Options** panel with Default Size (`Wide (1024x576)`), Inference Steps, Guidance Scale and LoRA Preset, each with v4's help sentence, plus `✓ 226 image models fetched from the provider` from a live `list-models`. The legacy hand-written arm did not run. |
| C3 | CLAUDE | **The HuggingFace lookup — the one arm no test may exercise** | Enter a real repo id and hit Query. | A live HF response populates the result panel; the host transport's status-before-body order holds. | **PASS — the arm no test can exercise, run live.** *HuggingFace says* → Trained on `black-forest-labs/FLUX.1-dev` · Nature "Tagged a LoRA adapter." · Pipeline text-to-image · Weights `aidmaNSFWunlock-FLUX-V0.2.safetensors` · Standing `2 likes · 6,649 downloads` · Declared trigger phrase `aidmaNSWFunlock` with a **Use it** action (the `NSWF` transposition is HuggingFace's own card, not ours). The verb was then driven directly on a **base model** (`black-forest-labs/FLUX.1-dev`) and answered the other arm honestly: `isLora: false`, `isAdapter: false`, `gated: "auto"`, the real weight-file list. |
| C4 | CLAUDE | LoRA round-trip | Edit a scale, save, reopen; then check SQL. | The `parameters` bag round-trips with the edit and every sibling key intact (`size` preserved). | **PASS** — clicked **Use it** (which relabelled itself *"— already in place"*), set Strength 0.80 → 1.20, Update. DB: `{"size":"1024x576","loras":[{"source":"shahtab/FLUXNSFWunlock","scale":1.2,"triggerPhrase":"aidmaNSWFunlock"}]}` — sibling key and key order preserved. **And the control question for finding #108: `provider` stayed `NANOGPT`**, not clobbered to the OpenAI the select was displaying. |
| C5 | CLAUDE | `apply_loras` is family-first on the wire | Generate one image on a LoRA profile; tap the wire. | The request carries the LoRA in the NanoGPT dialect bug 110 settled (family-first), and bug 111's error-level log line fires only on the arms v4 fires it on. ⚠ one real image ≈ small spend. | PENDING |
| C6 | CLAUDE | Over-cap refusal | Add more LoRAs than the cap allows. | v4's Zod **envelope** through `CoreError.details`, not a flat sentence. | PENDING |

## Part D — `qt-range`, the mid-turn bubble, and finding #107

| # | owner | step | gesture | expected + verification | status |
|---|---|---|---|---|---|
| D1 | CLAUDE | Themed sliders across the twelve hosts | Visit the range hosts: memory editor, housekeeping dialog, participant card, memory-dedup card, context-compression settings, dangerous-content settings, LoRA list editor, provider profile modal, tasks-queue card. | Every `input[type=range]` carries `.qt-range` and paints with the theme (not the browser default). | **PASS** — a mechanical census over the SPA source finds **13** `type="range"` inputs across 9 files and **0** missing `qt-range`. Live: `accent-color` computes to `rgb(129, 151, 218)` (the resolved `--color-primary`), not `auto`, with `cursor: pointer`. The `.qt-range` rule and both tokens (`--qt-range-accent: var(--color-primary)`, `--qt-range-focus-ring: var(--color-ring)`) are **byte-identical to v4's** `_interactive.css` / `_variables.css`. Seen in place on the sidebar Talkativeness sliders, the LoRA Strength slider and the three Context-Compression sliders. |
| D2 | CLAUDE | The mid-turn optimistic bubble does not duplicate | Send a message into a **multi-character** chat and watch mid-turn (the defect was strictly mid-turn — the whole Playwright suite was green through it). | The user's message appears **once** for the whole turn, including across the mid-turn refetches the realtime hints trigger. This is finding #106's fix on real data with a real clock. | PENDING — needs a live multi-character turn; grouped with B3/B4. |
| D3 | CLAUDE | Finding #107's `qt-markdown-field` rule | New Chat → the scenario field's Markdown toolbar. | The toolbar no longer overflows its column (the inline-host family: the host class now has a rule). | **PARTIAL → FAIL(#109)** — the *cause* is fixed: the `qt-markdown-field` host is now a block frame flush with its card (both 197 → 711 px), where before it was `display: inline` and framed nothing. The *symptom* is not: the toolbar's seventeen buttons still run 135 → 773 px, **62.9 px past the card on each side**. `.qt-formatting-toolbar` is byte-identical to v4's (`justify-center`, no wrap, no scroller), so v4's row overhangs too — v4 merely hides it with the frame's `overflow-hidden`, which v5 deliberately omits because it also clips the emoji/unicode pickers. Recorded as **#109**, a v4-first filing; not fixed here, since the file is byte-shared with v4. |
| D4 | CLAUDE | The host-class guard is live | `npm run lint` / the guard script. | Reports zero unresolved `host: { class: 'qt-…' }` names at its narrow scope. | **PASS** — `npm run lint` runs the guard's own self-test first (**5/5**, including the one-line `@Component` header and the conditional `[class.qt-…]` binding arms) then the real pass: *945 qt-* classes defined, every guarded reference resolves*. |

## Part E — carry-over 💸 from round 1 (P4.D134–P4.D137)

| # | owner | step | gesture | expected + verification | status |
|---|---|---|---|---|---|
| E1 | CLAUDE | The **dead-endpoint understudy walk** | Point a throwaway connection profile at a dead endpoint, give it a fallback (16 chains already exist), send a turn. | The chain fails over; `llm_logs` shows the stand-in; the **failing-over toast** names the stand-in, and re-fires when a *second* stand-in's name is news. | PENDING |
| E2 | CLAUDE | The `[CheapLLM] Task failed` warn precedes the chain | Same walk on a cheap-LLM path if reachable. | The warn is logged **before** the recovery, not after (the §3 review's headline fix). | PENDING |
| E3 | CLAUDE | The live curly-quote resolve | In the Scriptorium, run a doc-edit `str_replace` whose needle differs from the document only by typographic characters (curly vs straight quotes, en/em dashes). | The 25-entry fold table resolves it; the pre-fix behaviour (silent delete / `"undefined"` splice) does not appear. | PENDING |
| E4 | HUMAN | Reroute-with-an-image + a re-measured compression row | A dangerous-chat turn carrying an image, on a chat under real context pressure. | The reroute re-crafts and the image survives; the 75 s compression numbers from the 2026-08-27 C4 row are re-measured (they are marked SUPERSEDED). Deferred: needs a chat under genuine pressure and real spend. | PENDING |

## Part F — the standing 💸 remainder (deferred by cost or judgment)

| # | owner | step | why deferred | status |
|---|---|---|---|---|
| F1 | HUMAN | Pascal side effects — the **group** tier | Needs a single-group chat set up deliberately; the other three write paths are already proven. | PENDING |
| F2 | HUMAN | The Brahma budget on a real deep query | Long agent-turn loop, real spend. | PENDING |
| F3 | HUMAN | Memory dedup + conversation-summaries first run | Batch jobs over 885 chats — real cost. | PENDING |
| F4 | HUMAN | NanoGPT prompt-caching cost question (finding #101) | A cost/economics judgment about where the gateway puts its breakpoint, not a correctness check. | PENDING |

---

## Findings

_(numbered rows also land in `docs/developer/porting/dogfood-findings.md`)_

### #108 — the image-profile editor names the wrong provider (FIXED)

Found at step C1. Editing `FLUXNSFWunlock` (a real NanoGPT profile **v4 wrote**)
showed **Provider: OpenAI** while the same dialog showed the NanoGPT API key, the
`flux-lora` model and a NanoGPT options panel — the screen contradicting itself.
Reproduced on `Grok Imagine 2`; deterministic on a re-open with the provider list
already cached, so not a first-paint race. On this instance it is **11 of 14
profiles** — every one not on `OPENAI`.

Cause: the Provider select's rows come from `@for (p of providers())` over an
async query while the value was bound `[value]="provider()"`. Angular applies the
property binding before the option views exist, so the assignment matches
nothing and the browser settles on row 0; the binding never re-runs because
`provider()` never changed. v4's React controlled select re-applies `value` on
the render that fills the list, so v4 was never affected. **The same file already
carried the fix twice** — the Model and Size selects use the `afterRenderEffect`
post-render assignment and a comment describing this exact hazard.

Display-only: `provider()` held `NANOGPT` throughout, proven by C4's round trip
writing `NANOGPT` back.

Fixed with a third `afterRenderEffect` keyed on `providers()`, four specs
(mutation-proven: the naive binding reds exactly the two non-first-row arms), and
the live LoRA e2e beat — which already re-opens a NanoGPT profile's editor after
a reload — extended with the assertion that was missing.

