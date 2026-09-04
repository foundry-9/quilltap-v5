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

- **Oracle baseline:** `0b0617fee` — "fix(images): verify a describer saw the
  image; hash the bytes we store (bugs 116-118)" (v4 main, 2026-09-02
  21:04 -0500, v4 `4.9.0-dev.120`), adopted at the `0b0617fee` drift catch-up
  round unification (P4.D148 ∥ P4.D149 ∥ P4.D150 ∥ P4.D151 ∥ P4.D152,
  2026-09-03).
- **Checked:** 2026-09-04, at the END of the follow-ups round 2 unification
  (the `/unify` §6 drift step — the round's §2 probe had PASSED at its start
  with v4 `main` at `15573c3a1`, and the checkout was found at `06658535f`
  during cleanup). Every regen of that round ran from a pinned worktree at
  `0b0617fee`, so the unified gate is unaffected; the twelve commits below
  arrived DURING the round.
- **Re-checked:** 2026-09-04, immediately after that unification merged — a
  full `/driftcheck` run from a clean `main`. It found nothing new on either
  branch (`06658535f..main` and `3a76b17df..bugfix` both empty, tree clean), so
  it adds no §3 row and moves no disposition. What it DID do is **discharge the
  hunk-level survey the verdict below had deferred to the catch-up order**: all
  twelve rows in §3 are now written from their shipped hunks and from measured
  v5 intersections, not from diffstats. **One classification moved, and it moved
  materially: `d4138b96b`, from NO-PORT?-with-a-check to PORT — see its row.**
  The other eleven kept their class but traded inference for measurement, and
  three of them now name a v5 site that **measurably reproduces the v4 defect**
  (`48f4b42ec` → `request_builder/anthropic.rs:31`; `af2023c9a` →
  `instances_cmd.rs:314`; `0506517d3`'s correction (d) →
  `brahma_console/mod.rs:359`). `bugfix` was re-measured
  by CONTENT per §4 step 2: `main..bugfix` over `lib/ app/ packages/ plugins/`
  is 681 files / +60,360 / **−173,933**, net-negative because bugfix is *behind*
  main — the squash-topology signature, nothing genuinely unabsorbed.
- **v4 `main` HEAD at check:** `06658535f` — "chore(packages): install the
  published plugin-utils 2.6.1 everywhere" (2026-09-04) — **THIRTEEN commits
  past the baseline** (the bug-119 row + twelve new, all dated 2026-09-04,
  v4's 4.9 release-checklist push). `origin/main` level with `main`.
- **v4 `bugfix` tip at check:** `3a76b17df` — **unmoved** (the bare 4.8.4 fork
  marker; `3a76b17df..bugfix` empty).
- **v4 `release` tip at check:** `8736d7042` ("release: 4.8.4") — unchanged.
- **Checkout at check:** branch `main`, **tree CLEAN**. `git worktree list`
  shows the checkout alone (the unify's pin worktree was removed at cleanup).
- **Verdict: DRIFT PENDING — 13 commits past the baseline.** §3 holds the
  bug-119 row (`p4.9k`'s, unchanged) plus twelve rows, all now surveyed at hunk
  level. Classes: **5 PORT** — `0506517d3` (seven behaviour corrections + a
  neutrality obligation over 256 files), `d4138b96b` (the vestigial-cruft class,
  **and it breaks seven committed oracle cases**), `48f4b42ec` (one regex, v5
  reproduces the bug), `af2023c9a` (CLI Tier R, v5 reproduces it and then some),
  `bbcb318c6` (two SPA attributes) — plus `e9a9c538e` **PORT** for three About
  strings on top of a large reference-mirror refresh; **3 NO-PORT? carrying a
  measurement obligation** (`b52b996c1`, `6e1a64ea6`, `06658535f` — the
  wire-neutrality claim is a claim until a corpus says otherwise); and **3
  NO-PORT?** ratifiable as they stand (`49f66f571`, measured payload-neutral;
  `a0e6fb42a` and `2edd823c0`, tests only). **No convergence rows** — every bug
  in the batch (120) is v4's own release-checklist finding, and
  `docs/developer/bugs.md` shows no port-filed bug newly fixed; the bug-104/111
  regression tests in `a0e6fb42a` pin filings this port made, but those fixes
  were absorbed rounds ago and no pin trips.
- **`generateDDL` and `lib/database/schema` are UNTOUCHED across all thirteen
  commits — no D23 re-dump is owed.** Checked mechanically over the whole batch
  (`git show --name-only` matches neither path), not inferred from subject
  lines; `0506517d3` rewrites twelve 4.9 *migration scripts* onto shared helpers
  but adds no column and emits no new DDL.
- **Regen rule in force: PIN REQUIRED** at **`0b0617fee`** (§5.1) — and the
  survey upgraded the *reason* from prudent to load-bearing. Three of the
  thirteen rebuild every bundle under `plugins/dist/` (the third symlink class),
  so a pin-free regen would import a rebuilt Anthropic plugin and the `npm
  update`d SDKs — that much was known. What the hunk survey added:
  **`d4138b96b` deletes fourteen exports that seven committed oracle cases
  import BY NAME**, so a pin-free regen of those families does not diff wrong,
  it fails to link at all; and `0506517d3` rewrote 256 files underneath a large
  fraction of the corpus. Pin every regen until a catch-up round moves the
  baseline.
- _Superseded (the 2026-09-03 `/driftcheck`): DRIFT PENDING — 1 commit past
  the baseline (`15573c3a1`), PIN REQUIRED at `0b0617fee`; the probe at the
  follow-ups round 2's start (2026-09-04) re-measured it identical._
- **Release shape:** still no `release: 4.9.0` squash and no 4.9 bugfix fork;
  v4 develops on `main` alone. Keep probing BOTH branches.
- _Superseded (the EARLIER 2026-09-03 `/driftcheck` verdict, the one that ran
  before the round unified — not the post-unification check recorded above):
  DRIFT PENDING — 6
  commits past `6d2a50382`, five ORDERED to the in-flight `0b0617fee` round
  + `15573c3a1` new and UNPROCESSED, PIN REQUIRED at `6d2a50382`. The round
  unified 2026-09-03 and moved the baseline to `0b0617fee`; the five rows are
  in §6._
- _Superseded (the 2026-09-02 verdicts): recorded in §6's round entry and in
  `status-log.md`'s round record._

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
| `49f66f571` | 2026-09-04 | refactor(api): use successResponse in two v1 route handlers (checklist item 4) | **NO-PORT? — ratify as NO-PORT (verified payload-neutral)** | **Hunks: two one-line call swaps.** `app/api/v1/connection-profiles/[id]/route.ts` (the `get-tags` GET action) and `app/api/v1/wardrobe/route.ts` (the default archetype listing) change `NextResponse.json(x)` → `successResponse(x)`, plus the import line. **The neutrality claim was MEASURED, not trusted:** `lib/api/responses.ts:63` is `successResponse<T>(data, status = 200) { return NextResponse.json(data, { status }) }` — a pass-through, so status and body bytes are identical. → **v5 intersection: none required.** The two surfaces are ported (`get-tags` at P4.D85's `resolve_editor_tags`; the wardrobe archetype listing at P4.D112) and both already answer the same payloads; nothing to change. | UNPROCESSED |
| `bbcb318c6` | 2026-09-04 | style(settings): cheap-LLM checkboxes use qt-checkbox (checklist item 7) | **PORT (SPA, two attributes)** | **Hunks: ONE component, `components/settings/chat-settings/CheapLLMSettings.tsx`, two lines** — the "Fallback to Local" and "Allow a Similar-Tier Stand-In" `<input type="checkbox">` move `className="rounded"` → `className="qt-checkbox"`. → **v5 intersection: `apps/web/src/app/screens/settings/providers/cheap-llm-card.ts:151` and `:169`** (the cheap-LLM card, ported with the Settings vertical). **Measured: v5's two inputs carry NO class attribute at all** — neither v4's pre-fix `rounded` nor the post-fix `qt-checkbox` — so this is a real divergence to close, not a rename. Note it is the same class the P4.D115 `check-qt-classes` guard now polices; `qt-checkbox` must exist in v5's sheet (it does — the guard would have reddened otherwise) before the attribute lands. | UNPROCESSED |
| `48f4b42ec` | 2026-09-04 | fix(plugins): stop sending Opus 5 the sampling params it rejects | **PORT (provider wire, one regex)** | **Hunks: `plugins/dist/qtap-plugin-anthropic/provider.ts` — ONE new entry `/^claude-opus-5(-\|$)/` added to `SAMPLING_PARAMS_REJECTED_MODELS` (after the sonnet-5 row, before opus-4-7), plus three comment corrections** ("Opus 4.7+" → naming Opus 5 explicitly). The single flag gates BOTH decisions in both compositions: sampling params (`temperature`/`top_p`/`top_k`) and the fixed-budget thinking branch, in `sendMessage` (`:379`) and `streamMessage` (`:574`). Also in the commit: the plugin bundle rebuild (`index.js`, +111/−55) and the 1.0.54 → 1.0.55 manifest/package bump. → **v5 intersection: `crates/quilltap-core/src/model/request_builder/anthropic.rs:31`**, the compiled `SAMPLING_PARAMS_REJECTED_MODELS` `LazyLock<Vec<Regex>>`. **Measured: v5's table carries the other five patterns and NOT `^claude-opus-5(-\|$)`** — so v5 reproduces the bug: any `claude-opus-5*` profile is sent temperature/top_p/top_k and a fixed thinking budget, which the API 400s. The port is one line plus the comment rewrites; the proof is a regenerated `request_envelopes` corpus carrying an opus-5 row (the family already carries anthropic vectors). ⚠ **This is the model this session runs on** — the fix matters for a real Friday profile, so it earns a 💸 live send. | UNPROCESSED |
| `a0e6fb42a` | 2026-09-04 | test(coverage): regression tests for bugs 104 and 111, and sixteen new modules | **NO-PORT? — ratify as NO-PORT (tests + a rebuild artifact)** | **Hunks: eighteen new `__tests__/` files (233 cases), `docs/CHANGELOG.md`, and `plugins/dist/qtap-plugin-anthropic/index.js` (+107/−58).** The bundle hunk is a **vendored-dependency rebuild artifact only** — inspected: it is `node_modules/standardwebhooks/dist/timing_safe_equal.js` and neighbours being re-inlined by esbuild; no provider logic moves, and it is superseded twice over by `b52b996c1` and `6e1a64ea6` rebuilding every bundle. "No source changed" verified against the file list. → **v5 intersection: none.** The two bugs it covers are already absorbed (104 at P4.D128, 111 at the P4.D138 follow-up). **Worth reading when convenient, not porting:** v4 says bugs 87/88/112/119 turned out already covered under tests that do not name their number, and that 89/90/100/102 are guarded outside jest — useful when a future lane looks for a v4-side pin and finds none under the obvious name. | UNPROCESSED |
| `0506517d3` | 2026-09-04 | refactor: release-checklist 3 — collapse the 4.9 diff's duplicates onto single owners | **PORT (seven behaviour corrections) + a NEUTRALITY OBLIGATION over 256 files** | **The largest single drift commit this port has seen: 256 files, +8,241/−6,847, a DRY/SRP/chokepoint pass over everything since 4.8.4 (v4 says 324 commits, ~1,250 files).** No file was deleted or renamed (`--diff-filter=DR` empty), and **`generateDDL` is untouched**. v4 states behaviour is unchanged *except* for seven named corrections, each "a divergence between two sites" that the collapse exposed. **All seven verified at the hunk level (§5.3 — the prose was not trusted):** **(a)** `lib/llm/cheap-llm.ts:119 selectionFromProfile` now sets `profileParameters: profileParams(profile)` and derives `isLocal` from `provider === 'OLLAMA'`, where the eight hand-built `CheapLLMSelection` literals it replaces omitted the params and hard-coded `isLocal: false` on the answer-confirmation and cheap-task selections — **so a profile's provider params now reach the priority-5 fallback path**. **(b)** the export wizard's preview count filters `(await repos.files.findAll()).filter(f => !isFileExcludedFromExport(f))`, so it no longer counts archive bundles the writer refuses. **(c)** the chat-scoped document delete 404 body: `notFound('File not found')` (which `lib/api/responses.ts:153` renders as **"File not found not found"**) becomes `notFound('File')` inside the new `lib/documents/operator-doc-http.ts:319 deleteDocumentResponse` → **"File not found"**. **(d)** API-key failure prose: Brahma one-shot's lowercase `'no API key configured for this connection profile'` and help-chat's `'No API key configured'` both become the shared `'No API key configured for this connection profile'`; the `'API key not found'` sibling is unchanged. **(e)** `lib/pascal/placeholders.ts` (new, client-safe, dependency-free) is the ONE classifier for seven readers — a bare family prefix (`{{params.}}`, `{{metadata.}}`, `{{state.}}`) is now `{kind:'unknown'}` rather than a missing name (`key.length > PREFIX.length` guards each arm), and `{{params.toString}}` no longer renders prototype function source. **(f)** three client fixes: the character edit view's "choose their opening outfit" toggle toasts on success like the detail view; the new-character wizard's physical-description failure toast prefers the server's error text; the Answer Confirmation settings row takes the shared toggle-row styling. **(g)** `lib/tools/handlers/self-inventory/builders.ts:518` drops an empty `try/catch` around `resolveStandingInstructionsSection` (which already fails soft) — behaviour-visible only if that resolver ever throws. → **v5 intersections, spot-measured:** (c) `crates/quilltap-core/src/tools/doc_edit/` carries the delete arm; (d) **`crates/quilltap-core/src/services/brahma_console/mod.rs:359` still carries v4's PRE-fix lowercase string** while `orchestrator.rs:288` already has the capital form — the exact two-site divergence v4 just collapsed, reproduced in v5; help-chat's copy is banked at `p4.9i2`. (a) is the cheap-LLM selection family (P4.D135/P4.68); (b) the export wizard (P4.D46); (e) Pascal (P4.D35 + the Workbench); (f) the Aurora edit/detail views and the Answer Confirmation settings row. **The obligation beyond the seven** is the **neutrality sweep** the P4.D128 lane established for `dcab791c2`: the claim "behaviour unchanged" over 256 files is exactly the shape whose one real exception (a 10/76 title-cleaner divergence) no family could see. Highest-risk rewrites by size: `lib/tools/almanack/render.ts` (1,060), `lib/pascal/tool-draft.ts` (342), `app/api/v1/chats/route.ts` (306), `app/api/v1/chats/[id]/actions/documents.ts` and `app/api/v1/wardrobe/transfers/route.ts` (281 each), `app/api/v1/documents/route.ts` (248), `lib/tools/handlers/doc-edit-handler.ts` (226), `lib/tools/handlers/state-handler.ts` (214), `lib/services/chat-message/message-finalizer.service.ts` and `answer-confirmation.service.ts` (144/143), plus the twelve 4.9 migration scripts collapsed onto shared `openEncryptedSqlite`/`addColumnIfMissing` helpers. v4 also records what it deliberately did NOT collapse (the NanoGPT/DeepSeek/Z.AI re-implementations of the OpenAI-compatible base, the two drifted cheap-LLM task maps in `core-execution.ts`, `executeCheapLLMTask`'s positional params, the wardrobe dialog list loaders) — read that list before "finishing" a collapse v4 stopped short of. | UNPROCESSED |
| `d4138b96b` | 2026-09-04 | chore(dead-code): v4.9 release sweep, second pass (checklist item 5) | **PORT (vestigial-cruft class) — ⚠ AND IT BREAKS SEVEN COMMITTED ORACLE CASES** | **Hunks: three modules deleted whole (`lib/database/meta.ts` — the Mongo-era `quilltap_meta` store, `lib/sillytavern/persona.ts`, `hooks/useNavbarCollapse.ts`) with their suites, plus 32 unreferenced exports removed across 23 files.** None of the three deleted modules has any v5 reference (grepped: zero hits in `crates/`, `apps/web/src`, `harness/`). **The consequential half is the 32 exports.** A mechanical cross-check of every deleted export name against `harness/oracle/` found **fourteen symbols imported BY NAME by SEVEN committed oracle cases** — an ESM named import of a now-absent export does not diff wrong, it fails to link: `cases/cheap-model.ts` (`isCheapModel`, `estimateModelCost` ← `lib/llm/cheap-llm`), `cases/model-selection.ts` (`getModelsUnderCost`, `calculateCostTier`, `calculateSavings` ← `lib/llm/pricing`), `cases/llm-errors.ts` (`handleProviderError`, `getUserFriendlyError` ← `lib/llm/errors`), `cases/message-formatter.ts` (`buildMultiCharacterContextSection` ← `lib/llm/message-formatter`), `cases/post-office-host.ts` (`buildMultiCharacterRosterContent`, `buildMultiCharacterRosterOpaqueContent` ← `lib/services/host-notifications/writer`), `cases/chat-timestamp.ts` (`formatTimestampForSystemPrompt` ← `lib/chat/timestamp-utils`), `cases/token-estimation.ts` (`getContextUsagePercent`, `getContextWarningLevel` ← `lib/tokens/token-counter`). (`DEFAULT_LORA_SCALE` also appears in the deleted set but only as a **re-export** removed from `lora-support.ts`; the symbol lives on in the new `lib/image-gen/lora-scale.ts`, and the three image cases that name it do so only in comments — no breakage.) **So the baseline move is gated on a decision this row must surface:** each of those seven families pins a v5 port of a function v4 has now proven dead in its own tree. The port's own vestigial-cruft ruling applies — the honest options are (i) keep the v5 code and re-home the oracle import against v4's pre-deletion sha, recorded as a frozen-at-`0b0617fee` family, (ii) delete the v5 twin and retire the family, or (iii) prove the v5 twin is live in v5 even though v4's is dead (several are: `is_cheap_model`/`estimate_model_cost` sit in `crates/quilltap-core/src/cheap_model.rs` and are exported from `lib.rs`) and keep it under a *unit* pin instead of a differential. **Do not simply let the regen fail and call it drift.** Also in the commit: the LoRA scale bounds move to the client-safe `lib/image-gen/lora-scale.ts` (the profile editor had a byte-identical copy) — **the unification's own row flagged the check that survives: v5's P4.D138 transcription must stay byte-equal to the bounds in their NEW home**, since the old `lora-support.ts` re-export is gone; three follow-ups recorded in `docs/developer/DEAD-CODE-REPORT.md` (orphaned `quilltap_meta` table, unwired bundle signature verification, `ErrorCode.UNAUTHORIZED`), and the `4.9.0-dev.130` bump. | UNPROCESSED |
| `b52b996c1` | 2026-09-04 | chore(packages): install published package versions consistently everywhere | **NO-PORT? — with a MEASUREMENT obligation (wire neutrality)** | **Hunks: no v4 source. Root `package.json`/lockfile move `@quilltap/plugin-utils` 2.4.0 → ^2.6.0 and `create-quilltap-theme` → ^2.0.19; all fifteen plugins declare ^2.6.0 and are patch-bumped + rebuilt.** v4 says five bundles change (anthropic, deepseek, **mcp** — whose `index.js` moves +535, the largest of the five, though MCP is not a wire v5 reproduces — nanogpt, openai-compatible) "taking up the `buildRequestBody` refactor from plugin-utils 2.6.0" and the other ten are byte-identical. **The refactor was read, not trusted:** `OpenAICompatibleProvider` gains a `buildRequestBody(params, stream)` that both `sendMessage` and `streamMessage` now call, and it carries an explicit comment that the streaming keys are **spread in position** (between `stop` and `user`) "so the serialized body stays byte-identical". That is a claim, and the port's answer to a claim is a corpus: **regenerate `request-envelopes`, `google-wire`, and the stream/response-body corpora at the new baseline and assert every pre-existing row is byte-identical** — the P4.D34 / P4.D75 / P4.D105 recipe. Plugin SDK versions were deliberately HELD here (openai 7.4.0, nanogpt 7.5.0, `@openrouter/sdk` 1.2.32); `6e1a64ea6` is what moves them. → **v5 intersection: the nine request builders and five stream decoders** (`crates/quilltap-core/src/model/request_builder/`), which hand-roll what the SDK-backed plugins emit. If a byte moves, it is a PORT; if none does, ratify NO-PORT **on the measurement**, not on v4's sentence. | UNPROCESSED |
| `2edd823c0` | 2026-09-04 | test(backup): pin every 4.9/4.10 data-model addition in the restore guard | **NO-PORT? — ratify as NO-PORT (tests only), but READ IT** | **Hunks: `__tests__/unit/lib/backup/restore-field-fidelity.test.ts` (+144) and `docs/CHANGELOG.md` only. "No production code changed" verified against the two-file stat.** → **v5 intersection: none to port — but the four gaps it names are exactly the class v5's own restore differential is blind to**, and each is worth a corpus arm rather than a code change: `chats.conciergeOverride` restoring as `'UNCENSORED'` rather than narrowing to `'OFF'` (which would re-arm the classifier on a chat the operator ruled on — the P4.D148 surface), `chat_settings.cheapLLMSettings.allowCheapFallback` (whose `false` default reads as a declined stand-in — the P4.D138 remainder), `image_profiles.parameters.loras` (the bag is unvalidated by design, so nothing downstream notices the reserved key vanishing — P4.D138), and the `memoryRecall` instance-settings row carrying `perTurnConversationSummaries` (upserted by raw SQL, not through a repository — P4.D95). v4's own framing is the lesson: *a new column announces itself with a migration, but a new key in a JSON bag or a widened enum domain is invisible to every schema check*. v4 mutation-tested each against `restore.ts`. | UNPROCESSED |
| `e9a9c538e` | 2026-09-04 | docs: documentation-freshness sweep, second pass (checklist item 13) | **PORT (one SPA file) + a large reference-mirror refresh** | **Hunks: nine docs/help files + ONE code file, `app/about/AboutView.tsx` (three `<span>` bodies).** The About bullets gain: the Lantern — "…derived from chat context**, with LoRA adapters and per-model options taken from the provider&apos;s own advertised capabilities**"; the Concierge — "…quick-hide integration**, and a four-state per-chat control (Monitored, Flagged, Vouched Safe, Uncensored) settable at creation as well as mid-conversation**"; Multi-provider support — "…and OpenAI-compatible APIs**, each profile able to name an understudy to take the call when its provider falls over**". → **v5 intersection: `apps/web/src/app/screens/about/about-page.ts:342`, `:346`, `:398`, all three carrying the PRE-fix bytes**, with `about.spec.ts:87` pinning one of them — a three-string port plus the spec update (the About mirror has been kept byte-current since P4.D127/P4.D131). **The rest is reference-mirror work, not a port:** `docs/developer/API.md` (+187/−~20 — fourteen live `?action=` values that had no entry: group-stores and outfit-summary on chats, nine on `/api/v1/memories`, semantic-search on mount-points, theme-preference on the user profile, attach-mount-file on chat files; the duplicated backup sections merged; a v4.9-dev freshness note superseding the stale v4.3 action list) — **worth mirroring into `docs/v4/` and worth reading against P4.67/P4.72's `?action=` family, which is the v5 surface those fourteen values live on**; `CLAUDE.md` gains two chokepoint rules v5 already honours (Concierge posture via `getConciergeState`/`shouldUseUncensoredRoute`/`shouldShowDangerStyling`/`isClassifierOnDuty` and every transition through `applyConciergeFlip`; provider fallback built by `lib/llm/fallback`, three attempts, no recursion, no stickiness); four `help/` files (banks to `p4.9i2` — `homepage.md` gains an In-Chat Navigation section, and three pages had `url:` values that were not routes); `docs/releases/4.9.0.md` (the defect count corrected 46 → 54, bugs 66–119). | UNPROCESSED |
| `af2023c9a` | 2026-09-04 | fix(cli): instances default --json was read as an instance name (bug 120) | **PORT (CLI Tier R) — NOT a convergence (v4's own release-checklist finding)** | **Hunks: `packages/quilltap/lib/instances-commands.js` — the `default` arm becomes `const json = rest.includes('--json'); cmdDefault(rest.filter(a => a !== '--json'), { json });` (was `cmdDefault(rest)`), plus the comment explaining why reading is not enough; the `instances --help` verb line becomes `list [--json]`; and `lib/completion/fish.template` gains a `for verb in list ls` block offering `--json`.** Two compounding mistakes in v4: the flag was never *read* (so `cmdDefault`'s JSON branch was unreachable) and never *removed* (so `args.length` was 1, and `--json` became an instance name to set). Six new cases in `packages/quilltap/lib/__tests__/instances-default-json.test.js` drive the real binary; two fail against the old dispatch. → **v5 intersection: `crates/quilltap-cli/src/instances_cmd.rs`.** **Measured: v5 has the same defect and then some** — `:48` is `"default" => cmd_default(&rest)`, and `cmd_default` (`:314`) has **no `--json` branch at all**, so v5 must port the whole post-fix shape, not just the strip. v5's `list` arm (`:32`) already reads both `--names-only` and `--json`, so only `default` is affected. Also owed: `crates/quilltap-cli/src/help/instances_help.txt:7` (`list` → `list [--json]`, byte-copied) and `help/completion/fish.template:236` (the new `list ls` `--json` block, in v4's exact placement between the `names-only` line and the `default --clear` line). Tier R should go red-first on the three affected cases. Docs half (NO-PORT for behaviour, worth mirroring): `instances restore-key` added to the CLI package README and `help/cli-instances.md`; CLI.md records `--names-only` as deliberately undocumented completion plumbing. | UNPROCESSED |
| `6e1a64ea6` | 2026-09-04 | chore(deps): npm update across app, packages, and plugins | **NO-PORT? — with a MEASUREMENT obligation (the SDK wire re-check)** | **Hunks: "dependency movement only; no source changes" — verified: no `lib/`, `app/`, or `components/` file appears. But all fifteen plugin bundles are rebuilt (+127,912/−50,475) and the SDK jumps are the largest this port has seen.** Root: **`openai` 7.4.0 → 7.10.0**, **`@openrouter/sdk` 1.2.32 → 1.2.106**, **`zod` 4.4.3 → 4.5.4**, `next` 16.3.0 → 16.3.4, `sharp` 0.35.3 → 0.35.4, `@tanstack/react-query` 5.101.4 → 5.102.8, jest 30.5.1, plus dev-tool patches. Packages: `@quilltap/plugin-utils` 2.6.1. Every plugin's own `openai` moves to ^7.10.0 and openrouter's to ^1.2.106. → **v5 intersection: two, and the second is the one to worry about.** (1) **The provider wire** — v5 hand-rolls the HTTP the SDKs emit, so this is the P4.D34/P4.D75/P4.D105 re-check: regenerate all four provider corpora at the new baseline and assert byte-identity, and re-confirm the three recorded SDK refusals still refuse (§the `v4-sdk-behavior-the-raw-wire-must-reproduce` rule — the SDK *throwing* on a non-2xx is part of the contract, and finding #104 was that lesson learned the hard way). (2) **`zod` 4.4 → 4.5** — v5 byte-copies Zod's own `ZodError.message` bodies into refusal envelopes at many edges (P4.D71's `chat_settings` arms, P4.62's `validationError` envelope, the `zod_uuid` gate transcribed from Zod 4's own regex, P4.55/P4.D122's guard-order arms). **A minor-version change to Zod's message text or its uuid regex would move bytes v5 has hard-copied, silently, at edges no SDK corpus covers** — the settings/routes families are the ones to regenerate and read. Also here: `public/schemas/plugin-manifest.schema.json` re-emits `extendsTheme` as `"type": ["string","null"]` instead of an `anyOf` pair (a zod-4.5 JSON-Schema emission difference, theme manifests only — v5's manifest generator reads plugin `manifest.json` files, not this schema). And the root `postcss` override is repaired to the self-referencing `$postcss` form, without which `npm install` fails EOVERRIDE — **relevant to anyone rebuilding v4's `node_modules` for an oracle run.** | UNPROCESSED |
| `06658535f` | 2026-09-04 | chore(packages): install the published plugin-utils 2.6.1 everywhere | **NO-PORT? — ratify with the same corpus measurement as `6e1a64ea6`** | **Hunks: no source and no bundle `index.js`.** Root `package.json`/lockfile take `@quilltap/plugin-utils` ^2.6.1 and `@quilltap/plugin-types` ^2.6.0; all fifteen plugins' `package.json` + `manifest.json` version fields follow; `packages/quilltap` and the README version stamp move to `4.9.0-dev.133`. Since no `index.js` changed, the emitted plugin code is identical to `6e1a64ea6`'s — this is the version-consistency tail of that sweep. → **v5 intersection: none beyond the corpus re-check already owed above.** ⚠ **v4 HEAD is this commit**, so it is the sha every pin-vs-tip comparison is measured against; the *baseline* stays `0b0617fee` until a round moves it. | UNPROCESSED |

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
