# The v4 drift ledger

**The single standard record of where v4 stands relative to the oracle
baseline, what each drift commit touches, where it intersects work this port
has already landed, and what has been done about it.** Written by
`/driftcheck` (and by `/unify` when a round moves the baseline); read by
`/setupphase`, `/carryout`, `/unify`, and `/dogfood`. Those commands do NOT
re-derive drift — they run the §2 freshness probe and then trust this file.

Ownership: this file is written only from a main-checkout session
(`/driftcheck`, `/setupphase` marking rows ORDERED, `/unify` marking rows
ABSORBED, `/dogfood` when a walk changes a row's facts). **Lane agents never
write it** — a lane that finds the probe failing STOPs and reports instead.

---

## §1 Current state

_Updated only by `/driftcheck` and `/unify`. Every field here is what the §2
probe verifies against._

- **Oracle baseline: `c2232cd9a`** — "dev: started 4.10.0 development" (v4
  main, 2026-09-05 13:04 -0500, `4.10.0-dev.0`), adopted at the `p4.9i2`
  help/HelpChat round unification (P4.9I2A ∥ P4.9I2B ∥ P4.76 ∥ P4.77,
  2026-09-05). CLAUDE.md's Status bullet agrees (checked this run).
- **Checked:** 2026-09-06, a full `/driftcheck` from the main checkout.
- **v4 `main` HEAD at check:** `ba34fa367` ("merge: 4.9.1 back into main",
  2026-09-05 23:00 -0500) — **2 commits past the baseline** (`5eaf98cf1`
  "release: 4.9.1" + the merge).
- **v4 `bugfix` tip at check:** `20913d2aa` ("fix: help-chat tool results
  reach every provider; Google accepts the wardrobe tools (bugs 124, 125)",
  2026-09-06 06:44 -0500, `4.9.2-bugfix.1`) — 4 commits past the previously
  recorded `2b49f51aa`, of which **exactly one is unabsorbed by `main` by
  CONTENT**: `20913d2aa`. Measured, not inferred —
  `git diff main bugfix -- lib/ app/ packages/ plugins/ help/` is three code
  files (`lib/services/help-chat/orchestrator.service.ts`, the google plugin's
  `provider.ts`/`index.js`/`manifest.json`/`package.json`) plus the
  `packages/quilltap` version line, and nothing else.
- **v4 `release` tip at check:** `5eaf98cf1` ("release: 4.9.1").
- **Checkout at check:** branch **`bugfix`** (not `main`), tree CLEAN.
- **Verdict: DRIFT PENDING — 6 new commits, 2 of them substantive:**
  bug 123 (`fef7ce4f7`, reaching main through the 4.9.1 release squash) and
  bugs 124 + 125 (`20913d2aa`, bugfix-only). The other four are the branch
  start, the release squash, the merge back, and the bug-filing docs commit.
  **Both substantive rows land on fully-ported surfaces**; bugs 124/125 are
  **this port's own dogfood filings coming back fixed** (findings #112/#114).
- **`generateDDL` and `lib/database/schema` UNTOUCHED** since the last D23
  re-dump — measured (`git diff c2232cd9a..main -- lib/database/` and
  `git diff c2232cd9a bugfix -- lib/database/` are both empty); no re-dump
  owed.
- **Regen rule in force: PIN REQUIRED at `c2232cd9a`** — now for BOTH of
  §5.1's triggers: v4 HEAD is past the baseline *and* the checkout sits on
  `bugfix`. (The previous §1 carried pin-required for the checkout alone;
  that milder reading no longer applies — there is real code drift on both
  branches.) v4 is the human's active repo: ask, never switch it.
- **Standing hazards that SURVIVE every baseline move (re-read before any
  regen):** (1) the oracle `node_modules` resolve the LIVE dependency tree,
  never a pin's — a v4 dependency bump is a regen event for every
  Zod-transcribing family whatever sha the source pin names; since the
  `p4.9i2` round `crates/quilltap-harness/tests/zod_version_guard.rs`
  (recorded `4.5.4`) FAILS the gate when v4's installed `zod` moves; (2)
  `harness/oracle/cases/memory-injector.ts` passes a REAL
  `MemorySubjectContext` positionally (P4.D153) — a future v4 arity change
  fails SILENTLY under `tsx`; (3) the seven families `d4138b96b` took dark
  were SPLIT (P4.D157) — a regen of `cheap-model`/`model-selection`/
  `llm-errors`/`message-formatter`/`post-office-host`/`chat-timestamp`/
  `token-estimation` at any sha BEFORE `d4138b96b` would not match;
  (4) **`help/**` is a VENDORED v5 ARTIFACT since P4.9I2A** — the workspace
  root's `help/` (120 files) is embedded into `quilltap-host` by its
  `build.rs`, and `help_tree_equivalence` diffs the embedded table against
  v4's REAL `ensureHelpDocsSynced` walking the checkout. **A v4 `help/`
  change is therefore a real port obligation, not a bank**, and reddens that
  family at the baseline move. This drift moves two files (`turn-skipping.md`,
  `chat-turn-manager.md`) — v5's copies are byte-identical to the BASELINE
  and differ from `main` (measured); (5) **NEW at `20913d2aa`:
  `jest.config.ts` gained `'^@google/genai$' → __mocks__/@google/genai.ts`**
  (a manual mock, `GoogleGenAI` as a `jest.fn`), because the SDK is ESM-only.
  Every jest oracle that loads the google plugin now resolves the mock —
  re-verify the google-family oracles regenerate byte-unchanged when the
  baseline moves past it; (6) `packages/quilltap/package.json` is out of step
  with the app on both branches (`4.9.1-bugfix.1` on main against the app's
  `4.10.0-dev.0`; `4.9.2-bugfix.1` on bugfix) — a v4-side nit; no
  `--version` comparand exists, Tier R unaffected.
- **Release shape:** v4 develops on `main` at 4.10.0-dev with a live 4.9.x
  `bugfix` fork; the 4.9.1 cycle has already run once (fork `d40497411` →
  fix `fef7ce4f7` → squash `5eaf98cf1` → merge `ba34fa367`) and the branch is
  open again at `4.9.2-bugfix.1`. §4 step 2's two-branch rule is load-bearing
  — measure `bugfix` by CONTENT, never its commit list.
- _Superseded (the 2026-09-05 unification's probe): NO DRIFT at
  `c2232cd9a`, PIN REQUIRED for the `bugfix` checkout alone._

## §2 The freshness probe

What every consuming command runs before trusting §1 — four read-only
commands, no classification, no judgment:

```bash
git -C ~/source/quilltap-server branch --show-current
git -C ~/source/quilltap-server status --short
git -C ~/source/quilltap-server log --oneline <§1 main HEAD>..main
git -C ~/source/quilltap-server log --oneline <§1 bugfix tip>..bugfix
```

Pass = the branch and tree state match §1 and both logs are **empty**. Then
§1's verdict and regen rule stand; proceed on them. Any mismatch = the ledger
is stale: `/setupphase`, `/unify`, and `/dogfood` run `/driftcheck` (or its
§4 procedure) before continuing; a `/carryout` lane **STOPs and reports**.

A dirty tree matters even when HEAD hasn't moved: uncommitted `lib/`, `app/`,
`packages/`, or `plugins/` edits poison any regen run from the checkout
(§5.1's mid-lane note). Infra-only dirt (CI files, docs) still gets recorded
here so the next probe doesn't re-alarm on it.

## §3 The drift table

Classes: **PORT** (behavior change on a ported surface), **PORT-NEW** (new v4
feature to port), **CONVERGENCE** (v4 adopting a fix this port made first —
both-directions pins will trip at the baseline move *by design*; retire them
by measurement, §5.4), **NO-PORT?** (docs/infra/tests-only candidate —
needs ratification with evidence, never by subject line alone).

Dispositions: **UNPROCESSED** → **ORDERED(order-id)** (set by `/setupphase`)
→ **ABSORBED(round)** or **NO-PORT-RATIFIED(round)** (set by `/unify` when
the baseline moves past the commit). A row leaves this table for §6 only
when absorbed/ratified.

| sha | date | subject | class | intersects (already-ported work) | disposition |
|---|---|---|---|---|---|

| `15573c3a1` | 2026-09-02 | fix(optimizer): a non-array sub-step answer no longer kills the run (bug 119) | **PORT (deferred surface — no v5 counterpart today)** | **NOT a convergence** — v4's own filing, from a screenshot of the Refine-from-Memories confirmation screen showing the minified `q.filter is not a function`. **Hunks: ONE lib file, `lib/services/character-optimizer.service.ts` (+60/-4), three changes.** (a) NEW exported `coerceSuggestionArray(value: unknown): OptimizerSuggestion[]` placed after `coerceSuggestionText` — array passes through; non-object/nullish → `[]`; else the FIRST array-valued property among the ordered key list `['suggestions','items','results','data','amendments']`; else a lone object whose `field` is a `string` becomes a one-element array (`field` is called "the shape's fingerprint"); else `[]`. (b) inside the sub-step body, `parseLLMJson<OptimizerSuggestion[]>(raw)` becomes `parseLLMJson<unknown>(raw)` + `coerceSuggestionArray(...)`, and a non-array answer logs `logger.warn('[CharacterOptimizer] Sub-step answered with a non-array; coerced', {characterId, subStep: label, parsedType, recovered})` — note `parsedType` is computed as `Array.isArray(rawParsed) ? 'array' : typeof rawParsed` **inside a branch already known to be non-array**, so it can only ever emit `typeof`; the pre-existing unparseable-JSON `catch` is untouched. (c) the closure `runSubStep` is renamed `runSubStepCore` and a NEW `runSubStep` wraps it in try/catch, logging `logger.error('[CharacterOptimizer] Sub-step failed unexpectedly; continuing', {characterId, subStep: label}, err)` and emitting `onProgress({type:'substep_complete', step:'generating', partialSuggestions: []})` — so one bad pass no longer aborts the fan-out (general fields / each scenario / each system prompt / physical description / wardrobe / aliases / proposed prompts). Explicitly **NOT done** (v4 says so): Zod validation at the sub-step boundary, and moving `logLLMCall` ahead of the filter chain. → **v5 intersection: NONE.** The character optimizer has **never been ported** — `m6-screen-parity.md:546` lists `CharacterOptimizerModal` (`components/characters/optimizer/CharacterOptimizerModal.tsx`, `CharacterDetailView.tsx:373`) as **absent → MISSING → `p4.9k`**, and `phase-4.md:779` / `:5099` carry it among the tier-3 LLM-service deferrals (ai-wizard / optimizer / rename / ai-import); grep confirms no `character_optimizer` / `OptimizerSuggestion` / `runSubStep` anywhere in `crates/` or `apps/`. So there is **nothing to port now and no family to regenerate** — the obligation is that **`p4.9k` ports the POST-FIX shape**, and its work order must cite this sha so the pre-fix inline `.filter` is never transcribed. Also note the class does not transfer mechanically: v4's bug is a TypeScript *cast* (`return JSON.parse(cleaned) as T`) that JS never checks, whereas v5 has no `parseLLMJson` twin at all (`services/answer_confirmation.rs:464 extract_json` returns a `Value` and every reader is explicit) — a Rust `serde_json` deserialize into `Vec<T>` would return `Err`, landing in the equivalent of the parse `catch` rather than throwing at `.filter`. The port's live obligation is therefore the *design* lesson v4's `bugs.md` states — normalise before treating a model answer as an array, and contain a fan-out failure to the pass that caused it — carried into `p4.9k`'s order, plus the coercion table byte-for-byte (the key list is ordered and load-bearing). **NO-PORT within the commit:** `README.md`, `docs/CHANGELOG.md`, `docs/developer/bugs.md` (+ the new `bugs/fixed/bug-119-optimizer-substep-non-array.md`, 186 lines — read it when `p4.9k` runs), the 47 new lines in `__tests__/unit/lib/services/character-optimizer-helpers.test.ts`, and the version bumps (`4.9.0-dev.120` → `.121` across `package.json`, `package-lock.json`, `packages/quilltap/package.json`). | UNPROCESSED |

| `d40497411` | 2026-09-05 | bugfix: started 4.9.1 bug branch | **NO-PORT?** | Version/branch-start bookkeeping only. **Hunks: four files** — `README.md` (the version badge line), the new `docs/releases/4.9.1.md` (90 lines of release notes, written up front as v4's fork commits do), `package.json` + `package-lock.json` (`4.10.0-dev.0` → `4.9.1-bugfix.0`). **Zero `lib/`, `app/`, `packages/`, `plugins/`, `help/`.** → **v5 intersection: NONE in code.** The only follow-through is the docs mirror: `docs/v4/releases/` stops at `4.9.0.md`, so the round that absorbs this drift should copy `4.9.1.md` in alongside it (the P4.D158/P4.59 mirror-refresh habit). Ratify with the four-file list, not the subject line. | UNPROCESSED |

| `fef7ce4f7` | 2026-09-05 | fix: a paused chat no longer goes quiet without saying so; Skip is always offered (bug 123) | **PORT (three ported surfaces: the turn chain, the SSE frame contract, the Salon SPA)** | **NOT a convergence** — v4's own filing, provenance *live Friday* (chat `51d2281d` "The Telling at the Table", 2026-09-06 00:59/01:03 UTC), reported by the human as "I can't get more than one message out of anybody… like it's paused, but it doesn't claim to be." Reaches `main` through the `5eaf98cf1` release squash; this is the substantive row for that content. **Hunks: 2 `lib/` + 4 `app/` + 2 `help/` (+ 3 test files, docs, README, version).** **(A) `lib/services/chat-message/streaming.service.ts`** — `encodeChainCompleteEvent`'s `data` type gains an optional `paused?: boolean`, documented as the thing `reason` alone cannot say (an `error` stop pauses when a chained turn *threw*, not when it merely came back empty). **(B) `lib/services/chat-message/turn-orchestrator.service.ts`** — four changes: `|| initialResult.isPaused` is REMOVED from the first `executeTurnChain` guard; a dedicated paused early-return is added after the single-turn guard that logs `logger.info('[TurnOrchestrator] Chat paused, not chaining after initial turn', { chatId, userId })`, `await persistTurnParticipant(repos, chatId, null)`, and emits `chainComplete { reason: 'paused', nextSpeakerId: null, chainDepth: 0, paused: true }`; the mid-chain decision emit gains `paused: decision.reason === 'paused'`; the empty-response stop gains an explicit `paused: false` (so the client can never infer a pause from `reason: 'error'`) and the chain-error safety stop gains `paused: true`. **(C) `app/salon/[id]/hooks/useChatControls.ts`** — the pause-sync effect is re-keyed from `[chat?.isPaused]` to `[chat]` (reconcile on EVERY fetch, since Resume left the fetched object saying paused and `true → true` never fired), and `setPauseState` additionally does `setChat(prev => prev ? {...prev, isPaused: paused} : prev)`. **(D) `app/salon/[id]/hooks/useSSEStreaming.ts`** — an `isPausedRef` mirror plus `announceChainPause({reason, paused})`: returns on `!paused`, returns when the user paused it themselves (`isPausedRef.current`), returns for an all-LLM room (`isAllLLMChat`, which keeps its modal), else `showWarningToast("A character's turn failed, so auto-responses are paused. Press Resume in the sidebar to carry on.")` for `reason === 'error'` and `showInfoToast('Auto-responses are paused. Press Resume in the sidebar to let the others answer.')` otherwise; `paused: data.paused === true` is read off the frame and the callback fires AFTER `fetchChat()` at BOTH chain-complete call sites (send and continue-mode). **(E) `app/salon/[id]/SalonView.tsx` + `hooks/useTurnManagement.ts`** — the Skip banner is re-keyed from `turnSelectionResult.nextSpeakerId` to a new `speakingSeat` memo (the seat `findActiveUserParticipant` resolves through the Bug-44 impersonation overlay, i.e. whatever the composer will take words as), gated additionally on `!sending` and `hasActiveCharacters`; the wording splits — `isSeatsTurn` keeps `"<name>'s turn — type as them, or skip to let someone else respond."`, otherwise `"Speaking as <name> — type, or skip to let someone else take the floor."`, with the must-speak line unchanged; `useTurnManagement` takes a new `impersonatingParticipantIds` parameter, its guard becomes `isUserDrivenSeat(participant, impersonatingParticipantIds)` with the refusal reworded `'Only user-controlled characters can be skipped.'` → `'Only a character you are speaking as can be skipped.'`, it `await onUnpause()` first when `isPaused` (a Skip lifts a pause the way a nudge does), and the auto-continue after a skip additionally skips an impersonated next speaker (`&& !impersonatingParticipantIds.includes(nextSpeakerId)`). **(F) `help/turn-skipping.md` + `help/chat-turn-manager.md`** — rewritten Skip paragraphs plus a new `### When a Turn Fails` section. → **v5 intersection, measured:** **the server half APPLIES and v5 measurably HAS the silence** — `crates/quilltap-core/src/services/orchestrator.rs` `execute_turn_chain` returns on `|| initial.is_paused` (≈`:3796`) with no event and no log line, the empty-response stop (≈`:3886`) and the `Err` safety stop (≈`:3902`, which does `is_paused: Some(true)`) both emit a `paused`-less payload, and `crates/quilltap-core/src/services/chat_events.rs` `ChainCompletePayload` (≈`:409`) has no `paused` field. Ported in Phase 3 (the chat-orchestration spine / turn chain) with the SSE frame contract; the families that will see it are `orchestrator_tier3_equivalence` and the chat-events/SSE frame pins — ⚠ **a paused-chain case must exist in the corpus or the new frame key is unpinned** (the shape of the P4.58 blind-spot class). **The SPA Skip-banner half APPLIES**: `apps/web/src/app/chat/turn-controls.ts` `bannerText` (≈`:113-119`) has only the two pre-fix sentences, and `apps/web/src/app/screens/salon/salon-conversation.ts` (≈`:2015-2045`) gates the banner on `effectiveNextSpeakerId` + `isUserDrivenSeat` — exactly v4's pre-fix shape (ported across P4.D56/P4.D58/P4.D60, the impersonation-overlay rounds); `isAllLLM()` already exists (≈`:1408`) for the toast's all-LLM exclusion. **⚠ MEASURE BEFORE PORTING (C):** v5's SPA appears to have **no local pause flag to drift** — the templates read `chat()!.isPaused` off the fetched resource directly (≈`:353`, `:449`) and `togglePause` dispatches `chatUpdate` and relies on the refetch (≈`:2756`), so v4's `useChatControls` sync-drift defect may simply not reproduce. Do not transcribe a fix for a bug v5 does not have — measure it, then either port or record a no-counterpart. **`help/` is now a real port obligation** (§1 hazard 4): re-vendor both files or `help_tree_equivalence` reds. **NO-PORT within the commit:** the three new test files (`useTurnManagement.test.ts` +142, `useChatControls.pause-sync.test.tsx` +118, `turn-orchestrator.test.ts` +148), `docs/CHANGELOG.md`, `docs/developer/bugs.md` + the new `bugs/fixed/bug-123-silent-pause-sync-drift.md` (147 lines — read it; its "v5 status" paragraph states the two rules to port: *reconcile on every fetch, never stop a chain without saying why*), `README.md`, `.claude/commands/update-documentation.md`, and the version bumps. | UNPROCESSED |

| `5eaf98cf1` | 2026-09-05 | release: 4.9.1 | **NO-PORT? (carrier)** | The squash of the 4.9.1 bugfix cycle onto `release`, and the route by which bug 123 reaches `main`. **Its code content is exactly `fef7ce4f7` + `d40497411`** — measured: `git diff main bugfix -- lib/ app/ packages/ plugins/ help/` leaves only `20913d2aa`'s three files, so nothing in this squash is unique to it. The extra files over `fef7ce4f7` are `docs/releases/4.9.1.md` and the version lines (`4.9.1-bugfix.0` → `4.9.1`). Ratify as a carrier once `fef7ce4f7` is absorbed; the only follow-through of its own is copying `docs/releases/4.9.1.md` into the `docs/v4/releases/` mirror. | UNPROCESSED |

| `ba34fa367` | 2026-09-05 | merge: 4.9.1 back into main | **NO-PORT? (carrier)** | The merge of the release back onto `main`, which is why `main` HEAD is 2 past the baseline. Same content as `5eaf98cf1` minus `README.md`/`package.json`/`package-lock.json` (main keeps `4.10.0-dev.0`); it is the commit that left `packages/quilltap/package.json` reading `4.9.1-bugfix.1` on a `4.10.0-dev.0` tree (§1 hazard 6). Ratify as a carrier with `fef7ce4f7`. | UNPROCESSED |

| `02b77ab0f` | 2026-09-05 | docs(bugs): file bugs 124 and 125 — both UNCONFIRMED on live v4, found on the v5 port's dogfood copy | **NO-PORT?** | **Three docs files, zero code:** `docs/developer/bugs.md` (+2 index rows) and the two new `bugs/bug-124-help-tool-rows-lack-ids.md` (71 lines) / `bugs/bug-125-google-rejects-nested-additional-properties.md` (59 lines). **This is v4 accepting THIS PORT's filings** — dogfood findings #112 and #114 from the 2026-09-06 `p4.9i2` help-round walk, filed upstream at `bcf8a16c` on the v5 side. Ratify as docs; the substance arrives at `20913d2aa`. Worth reading at port time: v4's own `bugs.md` rows now carry an explicit **v5 status** for each (both "reproduces faithfully"), which is the upstream author agreeing with the walk's diagnosis. | UNPROCESSED |

| `20913d2aa` | 2026-09-06 | fix: help-chat tool results reach every provider; Google accepts the wardrobe tools (bugs 124, 125) | **PORT ×2 — this port's own filings, fixed upstream** | ⚠ **Bugfix-branch only; NOT yet on `main`** (the one content-unabsorbed commit, §1). **Not a §3 CONVERGENCE in the pin sense:** v5 never fixed either bug — it reproduced both faithfully and filed them — so there are **no both-directions divergence pins to trip** and the fix arrives *unpinned* unless the port adds coverage (see the corpus warning below). **Hunks: 1 `lib/` file + the google plugin (4 files) + jest infra + 2 test files + docs + versions.** **Bug 124 — `lib/services/help-chat/orchestrator.service.ts`:** imports `buildAssistantToolCallMessage` / `buildToolResultMessages` from `lib/services/chat-message/tool-call-threading` (the chokepoint the Salon and Brahma Console already use); a new `let lastToolResultContent: string | null = null` replaces the stuck-loop reminder's `[...conversationMessages].reverse().find(m => m.role === 'tool')` (a threaded id-less result is `[Tool Result: …]` **user** text and would never be found under role `tool`); `conversationMessages.push({role:'assistant', content: currentResponse})` becomes `push(buildAssistantToolCallMessage(toolCallsToProcess, currentResponse))`; the tool-result loop becomes `toolResult.toolMessages.map(tm => ({...tm, content: JSON.stringify({tool: tm.toolName, success: tm.success, result: tm.content})}))` → `buildToolResultMessages(threadedResults)` pushed spread, with `lastToolResultContent = resultMessages[resultMessages.length - 1]?.content ?? null`; plus a new `logger.debug('Threaded help-chat tool results into the conversation', {chatId, characterName, turn, toolNames, pairedByCallId, framedAsText})`. **Bug 125 — `plugins/dist/qtap-plugin-google/provider.ts` (+ the rebuilt `index.js`, `manifest.json`/`package.json` → 1.1.51):** `'additionalProperties'` is added at the HEAD of `UNSUPPORTED_SCHEMA_FIELDS`, and both the list and `sanitizeSchemaForGoogle` are now `export`ed so a unit pin can hold them. → **v5 intersection, measured — both surfaces are ported and both reproduce.** Bug 124: `crates/quilltap-core/src/services/help_chat/orchestrator.rs` (≈`:880-985`) pushes `msg("assistant", current_response)` and, per tool message, `msg("tool", json!({tool, success, result}).to_string())` — v4's exact pre-fix shape — and its stuck-loop reminder does the same `.rev().find(|m| m.role == "tool")`. **The chokepoint is ALREADY ported**: `crates/quilltap-core/src/services/tool_call_threading.rs` (`build_assistant_tool_call_message` `:125`, `build_tool_result_messages` `:221`), used by the Salon / Brahma / native + text tool loops — so the port is *routing the help loop through the primitive it already has*, not writing one. Family: `help_chat_orchestrator_tier3_equivalence` will red at the baseline move **by design** (the slate the follow-up stream receives changes); the per-provider id-less-tool-row DROP rule (`keeps_idless_tool_rows`, `orchestrator.rs` doc-comment `:54`, landed by the P4.9I2 §3 review) is untouched by this fix and must stay. Bug 125: `crates/quilltap-core/src/model/request_builder/google.rs:23` `UNSUPPORTED_SCHEMA_FIELDS` mirrors v4's list entry for entry and likewise lacks `additionalProperties` — a one-entry addition at the head (keep v4's order for the transcription even though the check is a `contains`). ⚠ **the google-wire / `request_builder_equivalence` corpora carry NO tool schema with a nested `items.additionalProperties`**, which is why no differential ever saw the bug (finding #114's recorded blind spot) — the port must ADD such a row or the fix ships unmeasured; the two shapes in hand are `wardrobe_wear` / `wardrobe_take_off` (`operations.items`). **Regen hazard (§1 hazard 5):** the same commit maps `^@google/genai$` to a new manual mock in `jest.config.ts`, so google-loading oracles resolve a stub SDK from this sha forward. **Unblocks:** dogfood finding #112 (a tool-needing help turn ending in silence on nine of ten providers) and finding #114 — and with it **the live proof of the P4.9I2 §3 GOOGLE-keeps-id-less-tool-rows leg**, which #114 was blocking. **NO-PORT within the commit:** `__tests__/unit/lib/services/help-chat/orchestrator.test.ts` (+93), the new `__tests__/unit/plugins/google-schema-sanitizer.test.ts` (+83), the new `__mocks__/@google/genai.ts`, `docs/CHANGELOG.md`, `docs/developer/bugs.md` + both bug docs moved to `bugs/fixed/` with FIXED paragraphs, `README.md`, versions. | UNPROCESSED |

## §4 How a full drift check runs (the `/driftcheck` procedure)

1. **Read §1** for the baseline and the previously recorded state. Never
   take the baseline from memory — CLAUDE.md's Status and this file must
   agree; if they don't, that disagreement is itself a finding to report.
2. **Both branches, always.** Since v4's 4.8.0/4.8.1 releases (2026-08-12)
   v4 develops on TWO branches: `main` (next-dev) and `bugfix` (patch
   maintenance). Release flow: a `bugfix: started X.Y bug branch` commit
   forks the branch; fixes land there; `release: X.Y` squashes back onto
   main. The topology is squash-like — bugfix commits are NOT ancestors of
   main — so `git log <baseline>..main` misses nothing but shows the release
   squash, while `git log <baseline>..bugfix` shows the whole historical
   lineage and looks alarmingly long. **Measure bugfix by CONTENT, never the
   commit list:** `git log main..bugfix --oneline -- lib/ app/ packages/`
   for candidates, then `git diff main bugfix -- <paths>` to confirm what is
   genuinely unabsorbed.
3. **Record the checkout's posture:** `git branch --show-current` and
   `git status --short`. A checkout sitting on `bugfix`, or dirty in
   `lib/`/`app/`/`packages/`/`plugins/`, silently poisons regens (it has
   happened — see §5.1) and flips the §1 regen rule to pin-required.
4. **Classify every new commit** — `git show --stat` for the file list,
   then the hunks. ⚠ **Never classify (or later port) from the commit
   message.** A v4 commit message describes the bug's shape as its author
   understood it; the shipped hunks are often narrower. The spec is
   `git show <sha>:<path>` (the post-commit file) plus `git show <sha> --
   <path>` (the hunks). When the message asserts a behavior change, find
   the hunk that makes it; if there is no such hunk, the claim is about the
   bug, not the fix (P4.D110/bug 96 is the canonical case). Carry a note of
   what a commit did NOT do into the row when the prose misleads.
5. **Delineate the intersection with ported work.** For each commit's
   files, name the v5 surface and the round/lane that ported it — grep
   `status-log.md` and CLAUDE.md's round bullets for the v4 path, feature
   name, or bug number. Rules of thumb: `lib/**` and `app/api/**` are fully
   ported (Phases 2–4); `components/**`/`app/**` client code maps to the
   SPA verticals; `packages/quilltap/**` is the CLI Tier R;
   `help/**` banks to `p4.9i2`; `.github/`, `docker/`, release scripts,
   and tests-only changes are NO-PORT candidates. Check v4's
   `docs/developer/bugs.md` for the bug number: a bug this port filed
   coming back fixed is a **CONVERGENCE** row (pins will trip; §5.4).
6. **Update §1 and §3** — never touch existing ORDERED/ABSORBED
   dispositions except to append newer facts. Commit per
   `.claude/commands/commit.md` (docs-only) and report: the verdict, the
   new rows, which ported surfaces are affected, and whether the regen rule
   changed.

## §5 Standing drift machinery — recipes and traps

_Moved into the repo from the session-memory notes, 2026-08-25. This is the
durable home; the memory notes now just point here._

### §5.1 The pinned-worktree regen recipe

A fresh oracle is only as pinned as the tree it imports. Whenever v4 HEAD is
past the baseline OR the checkout is dirty or on the wrong branch, regen from
a detached worktree pinned at the baseline. v4 is the human's active repo —
**never stash or checkout-switch it.**

```bash
PIN=/tmp/qt-v4-pin-<order>-<sha>       # LANE-UNIQUE path — never share pins
git -C ~/source/quilltap-server worktree add --detach "$PIN" <baseline-sha>
# THREE symlink classes, all untracked and absent from a bare worktree:
ln -sfn ~/source/quilltap-server/node_modules "$PIN/node_modules"
ln -sfn ~/source/quilltap-server/packages/quilltap/node_modules \
        "$PIN/packages/quilltap/node_modules"
for d in ~/source/quilltap-server/plugins/dist/*/; do
  [ -d "$d/node_modules" ] && ln -sfn "$d/node_modules" "$PIN/plugins/dist/$(basename "$d")/node_modules"
done
cd "$PIN"   # run EVERY tsx/jest oracle + fixture builder from here
# cleanup: git -C ~/source/quilltap-server worktree remove --force "$PIN"
```

- The `@/` alias resolves through the cwd's tsconfig, so cwd = the pinned
  worktree imports pinned lib code. The second symlink feeds the real-DB
  jest cases' `requireActual` of the cipher driver; the third feeds any
  oracle that loads a provider plugin (`provider-registry.ts`, the
  stream/envelope recorders, the manifest generator) — without it they die
  on `Cannot find module '@anthropic-ai/sdk'`.
- **The empty-file trap:** that failure is loud on stderr, but if stdout was
  redirected to the oracle file, the redirect already truncated it to ZERO
  bytes — the next diff then reads "DIFFERS against an empty file" exactly
  like real drift. Check the file is non-empty before believing a diff.
- **Verify the pin** by grepping the fresh NDJSON for a marker only one tree
  has (a sentence the drift added must be ABSENT from a baseline-pinned
  oracle).
- **Lane-unique paths:** a pin shared between lanes was deleted mid-run by
  whoever finished first (P4.d26) while v4 HEAD was past the baseline — the
  later regen would silently have used the wrong tree. Cheap guard:
  `git -C ~/source/quilltap-server worktree list` before each regen batch.
- **The checkout can go dirty MID-LANE** (P4.D105): clean at lane start,
  dirty two units later. Do not reason about whether the dirty files "could"
  have mattered — build the pin, re-run every regen the lane already did
  from it, and expect the re-run to change nothing. That is a cheap, total
  proof; the alternative is an argument.
- The pin is also what makes an end-of-lane sweep honest:
  `harness/tools/recipe_sweep.py --run-all --v4 "$PIN"` rewrites every
  recipe's `cd ~/source/quilltap-server`.

### §5.2 The silent-stale-pass trap (regen verification)

A fixture builder that ERRORS leaves the old fixture in place, and the
differential then passes **green on stale data** — the green comes from the
old fixture + old oracle agreeing with each other as they always had.
Discipline for every regen:

```bash
rm -f /tmp/qt-<x>.db                      # a failed build can't hide behind a stale file
... build ... 2>&1 | tail -1              # read it; a stack trace invalidates the run
grep -c '<new-field-or-id>' /tmp/oracle-<x>.ndjson   # MUST be > 0
```

Corollaries: a green regen is not coverage — grep every regenerated NDJSON
for the CHANGED bytes; when appending to a pinned-id corpus, check the WHOLE
id set for collisions, not the tail; anchor jest `--` filters
(`"case\.test\.ts$"`) because substring matches let a sibling case clobber
the same `QT_ORACLE_OUT`.

### §5.3 Commit prose misdescribes diffs

(Also in §4 step 4, because it bites at classification AND at porting.) Port
from the shipped hunks, never the message. Re-verify even when the work
order already flagged the trap — the order's paragraph is the same kind of
prose. Carry the finding into a v5-side code comment naming the sha and what
it did NOT do.

### §5.4 Convergence rows: measure, don't assume

When v4 adopts a fix this port made first, the both-directions divergence
pins trip at the baseline move **by design** — that tells you v4 MOVED, not
HOW. Regenerate the oracle and dump v4's ACTUAL post-fix output before
retiring a pin to a plain equality: v4's adoption is often partial or
differs in wording/details the planner never checked (P4.D51's bug-8 message
heads and bug-12 phantom-row surprises are the canonical cases). A partial
convergence splits one carve-out into a converged half + a still-divergent
half — expect to reshape pins, not just delete them. When a converging arm
reddens, instrument the harness to dump the actual rows (a `QT_DEBUG_*`
-gated `eprintln!`), and for restore/import read the archive's own manifest
as ground truth.

### §5.5 Drift heals real data — 💸 proofs expire

A banked live proof that depends on **damaged rows existing** can expire
because v4 (running daily on the real instance) lands its own fix and heals
the data — the orphan-reaper and poisoned-base-URL proofs both died this
way. When a round banks such a proof, record the measurement date and count;
before building a walk step around it, **measure the population first** (one
read-only query; a zero is a finding, not a failure); when it has expired,
retire it explicitly rather than carrying it forward. Planting the damage on
the disposable copy proves the mechanism but is a weaker claim — offer it,
don't silently swap it in.

## §6 History

- **The `p4.9i2` help/HelpChat round (2026-09-05, baseline `d883a5ee1` →
  `c2232cd9a`):** `6cbe2b027` NO-PORT-RATIFIED(p4.77 — the final 4.9.0
  release notes; `docs/v4/releases/4.9.0.md` refreshed from it), `b0eea4642`
  NO-PORT-RATIFIED(p4.77 — the squash onto `release`; content diff against
  the baseline EMPTY under every ported path), `f6794c840`
  NO-PORT-RATIFIED(p4.77 — the merge back; four version/doc files),
  `c2232cd9a` NO-PORT-RATIFIED(p4.77 — the 4.10.0 dev bump; the
  `package-lock.json` hunk is the two version lines). Evidence: the P4.D159
  block in `status-log.md` → "Lane record — P4.77 unit 2". `15573c3a1`
  (bug 119) stays in §3 for the unported `p4.9k`.
- **The `d883a5ee1` drift catch-up round (2026-09-05, baseline `0b0617fee` →
  `d883a5ee1`):** `d883a5ee1` ABSORBED(p4.d153 — bug 122: the memory-subject
  prefix through the three self-facing formatters at v4's template positions
  and inside the token estimate, `find_names_by_ids` on the RAW path, the
  `memory_subject` resolver with the zero-query early return, the three call
  sites; the oracle case's positional arity fixed FIRST; corpus + tier-3
  fixtures widened with targeted memories), `e288ae2ec` ABSORBED(p4.d154 —
  bug 121: the USER-side attachment walk as a fourth `message_context_leaves`
  leaf with v4's ten cases, the re-hydration before `build_context` with the
  skip-whole budget and the `unsupported`-with-error drop, the
  `load_user_attachments` seam, the orchestrator corpus widened to SEE the
  splice), `0506517d3` ABSORBED(p4.d155 corrections (a)–(e),(g) server-side +
  the Pascal classifier on both sides ∥ p4.d156 client corrections (f) ∥
  p4.d158's neutrality sweep for the other 249 files; the two
  `screens/custom-tools/**` readers landed at the unification wire),
  `bbcb318c6` ABSORBED(p4.d156 — the two `qt-checkbox` attributes),
  `48f4b42ec` ABSORBED(p4.d158 — `^claude-opus-5(-|$)` in v4's position, two
  corpus rows red-first), `af2023c9a` ABSORBED(p4.d156 — bug 120, Tier R
  214 → 216/0 vs v4's REAL launcher), `e9a9c538e` ABSORBED(p4.d156 About hunk
  ∥ p4.d158 docs half — the `docs/v4/` mirror refreshed at the pin, the
  `?action=` rows read against v5, the §G help bank), `d4138b96b`
  ABSORBED(p4.d157 — thirteen symbols, all option (ii) DELETE — not one twin
  had a production caller; seven families SPLIT, none frozen; the LoRA
  bounds pinned against their new home), `b52b996c1` + `6e1a64ea6` +
  `06658535f` NO-PORT-RATIFIED(p4.d158 — every provider corpus regenerated
  at the pin: only `x-stainless-package-version` 7.4.0 → 7.10.0 and the
  openrouter user-agent moved; manifests byte-identical) **with one
  correction at the unification: `6e1a64ea6`'s `zod` bump DID move v5 bytes**
  — Zod 4.5.4 makes a strict object's `unrecognized_keys` issue continuable
  (core `schemas.js`, invisible to the locale diff both lanes took), so a
  `when: true | {…}` union with a stray key reports the object branch alone
  and its refines fire; both engine twins (`custom_tool_types.rs` /
  `custom-tool-types.ts`) fixed at the wire, `pascal_custom_tool_definition_
  equivalence` red-first then green (260
  definitions with two new astral-title rows), the SPA's committed corpus
  refreshed (301 rows) + nine hand-captured rows re-captured — **and the
  neutrality sweep then found the bump's SECOND rule, code-point string
  lengths, in three families' astral pins (fixed as one helper per check on
  both sides; the sweep's other reds were a fixture-vintage artifact on the
  characters pair, an oracle mock lagging the collapse, and a moved LoRA
  import — none `0506517d3`'s, which measured NEUTRAL over 402 families),
  `49f66f571` + `a0e6fb42a` + `2edd823c0` NO-PORT-RATIFIED(p4.d158 — hunk
  evidence; `2edd823c0`'s four bag-key blind spots landed as restore corpus
  arms over the new committed `restore-archive-bag-keys.zip`). Round record:
  `status-log.md` → "Round record — the `d883a5ee1` drift catch-up round
  unification".

- **The `0b0617fee` drift catch-up round (2026-09-03, baseline `6d2a50382` →
  `0b0617fee`):** `303288fb4` ABSORBED(p4.d148 server ∥ p4.d149 SPA — the
  create-time `conciergeState` through the existing `apply_concierge_flip`
  chokepoint on all three branches, the greeting ladder's attempt 0 on the
  uncensored desk asked WITH the chat row, the one shared desk closure; the
  New Chat dropdown + the omit-when-monitored body rule + the gated create-time
  beat flipped live at unification; Continue Elsewhere seeding recorded as a
  NO-COUNTERPART), `02d4efa1b` ABSORBED(p4.d150 — `distill_memory_search`
  takes the latency class, the fallback interactive, pinned at the real call
  sites by a budget-recording provider since the corpus is provably blind),
  `c9faa2c74` ABSORBED(p4.d150 — the inter-character timing debug line,
  capture-pinned three arms), `b448eddd7` NO-PORT-RATIFIED(p4.d151 — docs
  only, five files, zero lib/app/packages/plugins; its measurement
  obligations discharged by p4.d151 for bugs 116/118 and p4.d152 for 117),
  `0b0617fee` ABSORBED(p4.d151 bugs 116 + 118 ∥ p4.d152 bug 117 — the
  describer arrival verdict ahead of every content check with the
  `CompletionResponse.cache_usage` widening; the manifest regen proven
  byte-identical, v5 never had bug 118; the four bug-117 legs with the
  within-tree boolean comparand and the `realign-file-entry-sha256-v1` boot
  heal in the P4.D140 ledger shape, its presence-vs-drift stamp rule a
  RECORDED both-directions divergence). `15573c3a1` deliberately NOT swept
  (still §3). Round record: `status-log.md` → "Round record — the
  `0b0617fee` drift catch-up round unification".

- **The `6d2a50382` drift catch-up round (2026-09-02, baseline `4622411fd` →
  `6d2a50382`):** `70505745a` ABSORBED(p4.d146 — the presence gate at the
  three story-background sites incl. the reworded 400, the
  `backgroundDisplayMode` normalizer at the overlay parse + the narrowed
  update schema + the GET's dead arms deleted, the SPA card; the committed
  `cost-background` pair and the story builder widened so the gates can be
  seen), `a00e18f0d` ABSORBED(p4.d147 — v5 had NO folder picker; v4's
  post-fix `FolderPicker` built fresh over the existing verbs with the live
  beat), `a5df98b3f` ABSORBED(p4.d145 — the unique-constraint predicate,
  `ensure_by_path` over the seven create sites with the two private lookups
  deleted, the collapse-then-index boot ensure in the index-guarded idiom
  with NO ledger row, the restore quiet-drop arm + a new committed archive;
  the order's provisioning-hook suggestion REFUTED by measurement),
  `f3351d54f` NO-PORT-RATIFIED(p4.d143 — three files, zero lib/app),
  `c43d3b1b4` ABSORBED(p4.d143 server ∥ p4.d144 SPA — the derived
  `conciergeState`/`dangerCategories` pair on all four list payloads, the
  predicate delegation, the per-turn enqueue guard, the `has-dangerous`
  probe v5 never had; the presentation table once in the SPA, the mark, the
  pill, `shouldHideChat` as the one rule), `6d2a50382`
  NO-PORT-RATIFIED(p4.d143 — four version files). Round record:
  `status-log.md` → "Round record — the `6d2a50382` drift catch-up round
  unification".

- **The P4.D138 follow-up (2026-09-01, baseline unchanged at `4622411fd`;
  the LoRA train's three PARTIAL rows completed):** `84f33ce94`
  ABSORBED(p4.d138 units 1–4 + unit 6 ∥ p4.d139 — the read side landed:
  `list-models` `loraSupport`, the `options-schema` action, the NanoGPT
  detailed-catalog cache with the augmentation arm the unit-1 narrowing had
  named; the tripwire fired and was deleted; the two SPA beats live),
  `648d5c8aa` ABSORBED(p4.d138 unit 5 — bug 110's family-first `apply_loras`
  with the corpus re-recorded at the tip, exactly the two predicted rows
  moving; bug 111's error-level request log + v4's debug line, both
  capture-pinned), `2ece98c90` ABSORBED(p4.d138 unit 7 ∥ p4.d139 — the
  HuggingFace lookup + repo-id twins with a 57-row differential over v4's
  real modules, the `lora-metadata` action live behind an engine gate, the
  host transport; one recorded divergence, V8's own `SyntaxError` wording).
  Round record: `status-log.md` → "Round record — the P4.D138 follow-up
  unification".

- **The round-2 drift catch-up (2026-09-01, baseline `7fb668263` →
  `4622411fd`):** `5f56f7a7d` ABSORBED(p4.d142 — `.qt-range` + tokens
  byte-identical, all twelve v5 range hosts adopted, dogfood #107's
  `qt-markdown-field` rule, the host-class guard at the ordered NARROW
  scope + the `--self-test` landed at unification), `735d9408c`
  ABSORBED(p4.d140 — bug 112 whole: the `chat_activity` chokepoint with
  its tier-1 family, both write sites red-first, the six readers, the
  restore re-derive, the ai-import twin NO-COUNTERPART, the boot recompute
  heal in the P4.D97 ledger shape with its own family, the four SPA
  display flips, the e2e seed landmine repaired; plus the out-of-mandate
  `allowCheapFallback` P4.D135 remainder), `e41fcb12e`
  NO-PORT-RATIFIED(p4.d138 — CHANGELOG + two help files, +34, zero
  lib/app/packages/plugins; help hunks banked to `p4.9i2`), `4622411fd`
  NO-PORT-RATIFIED(the unify — `docs/releases/4.9.0.md` alone, +70/−6),
  `60e3c4a0a` ABSORBED(p4.d141 — the Concierge four-state whole: the
  predicate family reshape at every call site, the resolver's operator
  arms, the flips + writer sentences byte-exact, the `conciergeState` PUT
  arm closing v5's long-named deferral, the classifier-gate corpora that
  can finally see the gate, the SPA control + single-pill badge + client
  twin, the four-state walk live; the §3 review's sidebar-latch fix and
  the broken `post_office_writers_tier3` family repaired at unification).
  The LoRA train (`84f33ce94` → `648d5c8aa` → `2ece98c90`) is PARTIAL —
  the whole client half (p4.d139) + p4.d138 units 1–4; units 5–7 stay in
  §3 as PARTIAL rows and in the order's resume list. The §3 unification
  review (five parallel readers over six lanes) fixed six finding groups
  on the unify branch — headline: the Concierge select's permanent
  optimistic latch, the optimistic-bubble echo scoped across two clocks,
  and a harness family the kind rename had silently broken. Round record:
  `status-log.md` → "Round record — the drift catch-up round 2 of 2
  unification".

- **The round-1 drift catch-up (2026-09-01, baseline `b121ac77f` →
  `7fb668263`):** `1560bd43b` ABSORBED(p4.d134 — the Lima/WSL2 retirement
  whole: env/lock/CLI, the data-dir `isVM` wire deletion with two renamed
  deletion pins, the host-rewrite two-strategy collapse, self-inventory/
  almanack retirements, the SPA About/footer/profile mirrors; the grep
  census; the unported host gateway resolver named as a follow-up order),
  `7819afb1d` + `3c3432ae9` NO-PORT-RATIFIED(p4.d134, file lists in the
  lane record), `65f5021c8` ABSORBED(p4.d135 — provider/model fallback
  chains whole: the D23 re-dump with the generateDDL-position correction,
  the pure engine tier-1 at 155→158 cases, the Salon spine both
  entrances, cheap-LLM + image-description sites, both id-remap paths,
  the delete-nulls cascade with the updatedAt stamp, the SPA mirrors +
  the live understudy round-trip beat), `97ebfb9fc`
  NO-PORT-RATIFIED(p4.d136 — both bug files' v5-relevance columns quoted
  in the lane record), `a1d88aa3a` ABSORBED(p4.d136 — bug 106 proven
  red-first in both halves + the three-spellings consolidation; bug 107's
  budget rewrite with the latency class threaded from 45 call sites, the
  timeout-only retry, five of six handler guards [scene-state deferred
  loud], the C4 dogfood row superseded per §5.5; the outfit-consult
  bound inversion measured as v4's own and reproduced), `487ae16b1`
  ABSORBED(p4.d137 — bugs 108/109 red-first: the argument guards
  byte-exact, the fold module + rebuilt per-UTF-16-unit diacritics map,
  the 5/25 replay split executable), `7fb668263` ABSORBED(p4.d134's About
  rider; README/CHANGELOG_V4 remainder NO-PORT). The §3 unification
  review fixed four finding groups on the unify branch — headline: the
  `[CheapLLM] Task failed` warn fired AFTER the chain instead of before
  it (v4 warns first; the counter bug 107 was measured from would have
  under-counted). Round record: `status-log.md` → "The drift catch-up
  round 1 of 2 unification".

- **The P4.D131 round (2026-08-27, baseline `aec86a613` → `b121ac77f`):**
  `679e450e3` ABSORBED(p4.d131 — the bug-105 CONVERGENCE retired by
  measurement per §5.4: FULL convergence, no residue; the arm — code name
  `execute_bug105_seed_abort`, the row's `import_aborts_on_non_string_
  provider` was its class description — is now a plain state-compared
  regression guard; the retirement measurably WIDENED coverage, the
  formerly-subtracted `main.image_profiles` table now discriminating),
  `0bd841394` + `1b0ce9eba` ABSORBED(p4.d132 — the Tooltip primitive +
  nine-button adoption + the ConfirmationBadge net-new + the deletion
  rider; help rows banked to `p4.9i2`; theme-storybook NO-PORT),
  `b121ac77f` ABSORBED(p4.d133 — `instances restore-key` whole, Tier R
  188 → 212; the row's ⚠ scope question resolved at ordering — the write
  proved in-sandbox via `reset_live`, the real-pepper walk banked 💸; the
  NO-PORT remainder RATIFIED: README/docs/help/package files + v4's two
  new test files, their behavior carried by the Tier R arms + unit pins).
  Round record: `status-log.md` → "The P4.D131 ∥ P4.D132 ∥ P4.D133 ∥
  P4.65 round unification".

- **The P4.D130 round (2026-08-27, baseline `8872d7efc` → `aec86a613`):**
  `aec86a613` ABSORBED(p4.d130 — the outfit pull-down + garments-only slot
  pickers, SPA-only; the composed-outfits selectors' recorded-vector corpus
  pinned at the drift commit, re-proven byte-identical after mid-lane
  drift), `b6c6d7793` NO-PORT-RATIFIED(this round — docs-only, this port's
  own bug-105 filing; file list verified `docs/developer/bugs.md` + the bug
  file, zero lib/app/packages/plugins content). Round record:
  `status-log.md` → "The P4.D130 ∥ P4.62 ∥ P4.63 ∥ P4.64 round".

- **The 4.9.0-push round (2026-08-27, baseline `f3892158d` → `8872d7efc`):**
  `914b59e13` + `805ef12bf` + `e000d6bfc` ABSORBED(p4.d126 — the full-wipe
  chokepoint, the variable-limit chunking, bug 103's legacy profile-column
  seeding; a v4 REGRESSION found in `e000d6bfc` itself — the helper sits
  outside the per-item try, one malformed profile aborts a whole v4 import —
  pinned v5-side, filed upstream), `964ffb959` + `8872d7efc` + `21f573039`
  ABSORBED(p4.d127 — bug 104, the per-task cheap-LLM budgets + failure warn,
  the coalesce-trace silence pin; the §1-predicted 💸 expiry executed: the
  Z.AI refusal-sentence proof retired, replaced by the glm-5.3 wire proof),
  `97d0b8f8e` + `57e7b1bc2` ABSORBED(p4.d128 — the qt-* utilities sweep,
  the four completion flags Tier R red-first), `8440b6391` ABSORBED(p4.d128,
  the AboutView hunk) + NO-PORT-RATIFIED(p4.d129, the docs remainder),
  `487ae57fe` `561466cfe` `7509c5cfb` `c0352fdba` NO-PORT-RATIFIED(p4.d129,
  each with evidence; `561466cfe`'s rider removed one vestigial v5 wardrobe
  twin), and `dcab791c2` NO-PORT-RATIFIED(p4.d129 — 410-family neutrality
  sweep + hunk measurements) **EXCEPT its title-cleaner second-trim collapse,
  which was measured NON-NEUTRAL (10/76 vectors) and ABSORBED at the
  unification wires** (both v5 cleaners + the `regen_title_quoted_padded_
  inside` tier-3 arm). Round record: `status-log.md` → "The 4.9.0-push
  drift catch-up round".

- **`f3892158d`-round (2026-08-26, baseline `b220999da` → `f3892158d`):**
  `664cfca84` ABSORBED(p4.d123 server ∥ p4.d125 client — the jobs/activity
  accounting whole), `f3892158d` ABSORBED(p4.d124 server ∥ p4.d125 client —
  the realtime subsystem whole, the hints riding v5's existing Event
  channel per the round's §Shared contract §B rather than a second
  WebSocket). Round record: `status-log.md` → "The `f3892158d` drift
  catch-up round".

- **`b220999d`-round (2026-08-26, baseline `8f9101370` → `b220999da`):**
  `b86bb1a58` ABSORBED(p4.d119 server ∥ p4.d121 SPA — the per-tier
  dressing instructions whole), `d25dacc1d` ABSORBED(p4.d120 ∥ p4.d121 —
  archive-instead-of-delete whole), `b220999da` ABSORBED(p4.d122 — the
  Documents-search vertical whole), `a47d3e034` + `2417cbed1`
  NO-PORT-RATIFIED(the two feature specs — docs-only, confirmed by the
  implementing lanes' hunk surveys; the search spec's defects paragraph is
  quoted in the p4.d122 order). Round record: `status-log.md` → "The
  `b220999d` drift catch-up round".

- **`8f910137`-round (2026-08-25, baseline `f6a10055d` → `8f9101370`):**
  `44a8137e9` ABSORBED(p4.d115 ∥ p4.d116 — the scenario-change feature whole),
  `8018c487a` ABSORBED(p4.d117 — bug 99, measured-then-ported),
  `309aaa97a` ABSORBED(p4.d117 — bugs 100/102 + the check-qt-classes guard),
  `6afacb187` ABSORBED(p4.d118 — bug 101, Tier R red-first),
  `8f9101370` NO-PORT-RATIFIED(p4.d118 — CI + tests-only; the +18 test lines
  absorbed by `completion_behavior.rs`). Round record: `status-log.md` →
  "The `8f910137` drift catch-up round".
- (This ledger was seeded 2026-08-25 with the baseline at `f6a10055d`. Drift
  older than that baseline is recorded in CLAUDE.md's round bullets and
  `claude-md-status-history.md`.)
