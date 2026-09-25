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

- **Oracle baseline: `b0b6656b5`**: "Fix bug 169: narrow-pane chat sidebar
  no longer closes dialogs it opens (#71)" (v4 main, 2026-09-24,
  `4.10.0-dev.78`), adopted when the `b0b6656b5` ten-commit drift catch-up
  round was unified (P4.D220 ∥ P4.D221 ∥ P4.D222 ∥ P4.D223 ∥ P4.D224,
  2026-09-25). CLAUDE.md's Status bullet agrees.
- **Checked:** 2026-09-25, third pass of the day (`/driftcheck`,
  main-checkout session, after `git fetch` of the v4 checkout; local `main`
  equals `origin/main`). This pass runs AFTER the `08c49319d` round was
  ordered (`fe310e7d5`: P4.D225 → P4.D226 → P4.D227 → P4.D228 ∥ P4.D229 ∥
  P4.D230 ∥ P4.D231 ∥ P4.D232, round target `08c49319d`).
- **v4 `main` HEAD at check: `acadcc7cd`** ("Name who stayed behind on
  Continue Elsewhere; persona absent in autonomous rooms (bugs 171, 172)",
  2026-09-25 14:23, `4.10.0-dev.93`). **TEN non-merge commits past the
  baseline** (§3):
  - `83d0c969b` (bug 170);
  - `a8292547a` (the overhaul specs);
  - the five Concierge-overhaul PRs (#73–#77, through `ce2f1dabf`);
  - `6d0f88d65` (a dependency sweep);
  - `08c49319d` (Scenario Builder shelves, **the ordered round's target**);
  - **`acadcc7cd`**: new, ONE past the round target.
- **v4 `bugfix` tip at check:** `1a2b2164c` ("bugfix: started 4.9.2 bug
  branch"), UNMOVED on both local and `origin/bugfix`; `1a2b2164c..bugfix` is
  empty.
- **v4 `release` tip at check:** `8fbf2afe0` ("release: 4.9.2"), UNMOVED.
  There is still no `release: 4.10.0` squash.
- **Checkout at check:** branch **`main`**, tree **CLEAN**. §2's probe runs
  against `acadcc7cd`.
  - **The `/setupphase` probe's dirt (~14:20) was bugs 171/172 in flight,
    and it is now COMMITTED as `acadcc7cd`.** Its file list is the dirty set
    that probe recorded, plus `docs/`, `help/` and the version stamps. The
    dirty-tree blocker is gone, and a HEAD-moved blocker replaces it.
  - **Consequence for the ordered round — RESOLVED by RE-ORDER (2026-09-25,
    the same day):** the round target is now `acadcc7cd`; every order's
    §R.2 probe expects HEAD `acadcc7cd`; `acadcc7cd` rides a NEW lane
    (P4.D233) and P4.D228's help tree. No waiver was needed.
  - Pinned regens at `b0b6656b5` / the chain's per-order pins are unaffected
    (§5.1).
- **v4's installed `node_modules` now match HEAD, so the morning's
  plugin-types warning is RESOLVED.** The human ran `npm install`. Measured
  at this check:
  - root `@quilltap/plugin-types` **2.8.0**, `@quilltap/plugin-utils` 2.6.2,
    `openai` **7.23.0**, `next` 16.3.6, `zod` 4.6.5 (unmoved);
  - every `plugins/dist/*/node_modules/@quilltap/plugin-types` at 2.8.0;
  - the six SDK-bundling plugin dirs at `openai` **7.23.0**,
    `qtap-plugin-openrouter` at `@openrouter/sdk` **1.3.28**,
    `@anthropic-ai/sdk` 0.115.0 and `@google/genai` 1.52.0 (both unmoved),
    and `qtap-plugin-mcp` at `@modelcontextprotocol/sdk` 1.30.1.

  ⚠ **The flip side: `provider_sdk_version_guard` is now RED by design.** It
  records `openai` 7.20.0 and `@openrouter/sdk` 1.3.11, and asserts every
  installed location. ⚠ **A baseline-pinned regen is no longer
  SDK-faithful to the baseline**, because §5.1's recipe symlinks the live
  checkout's `node_modules`, so a `b0b6656b5` pin now runs openai 7.23.0 /
  openrouter 1.3.28 under baseline `lib/`. Any provider-wire family
  regenerated before the catch-up absorbs `6d0f88d65` will carry the NEW
  stamps: `request-envelopes`, `image-dialects`, `openrouter_sdk_pricing`,
  and every stream/envelope recorder. This is the SDK-bump regen event the
  guard exists to announce (`6d0f88d65`'s row).
- **Verdict: DRIFT PENDING — 10 commits, ALL TEN ORDERED** (§3; the `08c49319d` round was RE-ORDERED the same day, `fe310e7d5` → the re-order commit, to fold `acadcc7cd` in: the round target MOVED to `acadcc7cd`, P4.D233 added, P4.D228's help tree re-vendors at `acadcc7cd`, P4.D231 pins its own commit):
  - 2 NO-PORT? rows: `83d0c969b` and `a8292547a`, to be ratified on their
    file lists.
  - **The five Concierge-overhaul rows**, each a large PORT, in a strict
    chain #73 → #74 → #75 → #76 → #77.
    - They **REPLACE v5's ported four-state Concierge** (P4.D141 /
      P4.D143–D144 / P4.D148–D149) and its `dangerousContentSettings` policy
      (P4.6a/6b, P4.6an, P4.71, P4.D37/38/48/201).
    - They rewrite the image-moderation reroute (W4.2, P4.D178 / P4.88)
      around a refusal classifier + understudy + image-failover chokepoint.
    - They widen the `routeTrail` JSON (P4.D171/D173/D177).
  - `6d0f88d65`: NO-PORT? for code, but a **REGEN EVENT** (the SDK bumps).
    It must land with a re-record of every provider corpus and the guard's
    constants moved.
  - `08c49319d`: a small PORT on the P4.D216/D217 Scenario Builder surface,
    independent of the Concierge chain.
  - **`acadcc7cd` (bugs 171 + 172): PORT, ORDERED(P4.D233).** v5 has BOTH
    bugs verbatim, measured:
    - `services/off_scene.rs:284` excludes the persona by name
      unconditionally (bug 172);
    - `services/chat_continuation.rs` names no one left behind (bug 171).

    It is small and independent of the Concierge chain. It touches
    `lib/schemas/chat.types.ts`, a P4.D226 file, but only to ADD the pure
    `operatorSpeaksWithoutSeat`.
  - No CONVERGENCE rows. v4's `bugs.md` has gained bugs 170, 171 and 172
    since the baseline. All three are v4-found (170 and 171/172 on Friday),
    none filed by the port, and v4's "v5 status" column reads "Unchecked" for
    all three.

- **Regen rule: PIN REQUIRED at `b0b6656b5`** because HEAD is ten commits
  past the baseline (and one past the ordered round's target: a lane
  regenerating "at the target" pins `08c49319d`, never HEAD). Every regen runs from a detached `b0b6656b5` worktree
  per §5.1. **Caveat, new at this check:** the pin's symlinked
  `node_modules` are HEAD's (the SDKs above), so a pinned provider-wire
  regen is baseline `lib/` under TARGET SDKs. Don't regen a provider-wire
  family at the baseline pin and call it baseline-faithful. Either the
  catch-up absorbs `6d0f88d65` first, or the lane records the SDK stamps it
  actually got.
- **The workspace gate at the baseline** is unchanged since the unification:
  the round record in `status-log.md` has the counts.
  - GREEN: `qtap_schema_embed_guard`, `zod_version_guard` (4.6.5),
    `provider_sdk_version_guard` and `public_schemas_vendor_guard`.
  - `help_tree_embed_guard` is GREEN at **129** (`host_help_docs_boot`
    agrees).
  - `dispatch_wrong_type_census` is at **449**.
  - The `web_edge_action_sites_census`, `blob_write_sites_census` (12 / 12 +
    one EXEMPT), `compressed_column_write_sites_census` (14),
    `get_messages_caller_census` (76, 7) and `web_edge_body_parse_guard`
    readings are as recorded at unification.
  - Tier R is 266/0 at the pin. The tool catalog is 59.
  - **What the drift will move** (to measure in the catch-up, not predicted
    here):
    - the `helpSettings` / `helpNavigate` tool-definition bytes (#76; the
      count stays 59);
    - both action lists and every action census (#77's
      `retry-image-uncensored` chat action + `retry-uncensored` message
      action);
    - `dispatch_wrong_type_census` (#75's three-value `conciergeState` enum;
      #76's retired-key 400 on the settings PUT);
    - the help tree's FILE SET, while the count stays 129;
    - **`provider_sdk_version_guard` (RED now, from the installed
      checkout; `6d0f88d65`).**
- **Schema state: v4 MOVED DDL.** The D23 re-dump from `e7d77bb60` still
  stands at the baseline. Past it, four new migrations are appended in this
  order in `migrations/scripts/index.ts`:
  1. `add-chat-refusal-ledger-v1` (#74) adds `chats.moderationRefusalCount
     INTEGER NOT NULL DEFAULT 0` and `lastModerationRefusalAt`. Both are
     deliberately absent from `ChatMetadataSchema`, like `transcriptVersion`.
  2. `add-chat-concierge-mode-v1` (#75) adds `conciergeMode` (default
     `'moderated'`), `conciergeModeSetBy` and `conciergeModeReason`,
     backfilled from the legacy `conciergeOverride` / `isDangerousChat` pair.
  3. `add-concierge-settings-v1` (#76) adds `chat_settings.conciergeSettings`
     JSON, backfilled from `dangerousContentSettings` +
     `uncensoredImageDescriptionProfileId` +
     `cheapLLMSettings.imagePromptProfileId`. Those source columns STAY in
     the DDL as deprecated and unread.
  4. **`drop-chat-concierge-override-v1` (#76) runs `ALTER TABLE chats DROP
     COLUMN conciergeOverride`.**

  Two open questions for the catch-up:
  - One classifier believes `generateDDL` walks the Zod schemas, so a D23
    re-dump would NOT carry #74's two schema-absent columns. If so, they need
    a boot ensure like `chats_transcript_version_repair.rs` (P4.D182). **This
    is unmeasured; the catch-up measures it with v4's real `generateDDL`
    before deciding.**
  - ⚠ **Live-instance consequence:** once the human's v4 runs `4.10.0-dev.88`
    or later against Friday, the drop migration removes a column v5 still
    binds (`conciergeOverride` has 107 references across core / web / host /
    harness / SPA, including `fresh_schema.json` and the export schema).
    **A fresh Friday copy taken after that point can be expected to break
    v5's chat reads/writes until #75/#76 are ported.** Dogfood on a copy
    taken before v4's upgrade, or measure the copy's `chats` columns first.
- **`help/**` vs v4 HEAD `acadcc7cd`:** the count is still **129**, but the
  tree DIFFERS in 20 files vs the baseline:
  - `acadcc7cd` modifies `chats.md` (already among the overhaul's 13) and
    `salon-host-introductions.md` (new to the list). So P4.D228's whole-tree
    re-vendor "at `08c49319d`" is one commit short of HEAD: 19 files at the
    round target, 20 at HEAD.
  - The 19 at the round target are the 15 below from the Concierge overhaul
    plus
  four modified by `08c49319d` (`general-scenarios.md`, `groups.md`,
  `project-scenarios.md`, `scenario-builder.md`). The overhaul's 15:
  - `dangerous-content.md` DELETED (#76);
  - `the-concierge.md` ADDED (#76, grown in #77);
  - 13 pages modified across #73–#77.

  The 129 literals therefore do not move. `help_tree_equivalence`, the
  embedded tree and the SPA's `help-categories.ts` capture (a slug swap + a
  `/settings?tab=concierge` URL entry, #76) all do.
- **The three text-compression migrations and the image re-encode migration
  stay DEFERRED as reclamation** (named in `db/text_compression.rs`, the
  P4.D209 record, and P4.104's module doc). The animated-input ruling is
  LANDED (P4.108); the corrupt-second-frame ruling (2026-09-23) keeps v5's
  first-frame still.
- **`docs/v4/developer/bugs/`** is identical to v4's at `b0b6656b5`. v4 has
  since added `fixed/bug-170-*` + its `bugs.md` row (`83d0c969b`; "v5 status:
  Unchecked", and the answer is **not affected**, see its row). The
  `docs/v4/developer/features/` mirror owes the six overhaul specs from
  `a8292547a`, as later edited by #73–#77. Both refreshes ride the catch-up's
  unification. `docs/v4/CHANGELOG.md`'s older lag stays a named housekeeping
  item.

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
| `83d0c969b` | 2026-09-24 | Fix bug 170: Salon speaker switch sends POST, not PUT | NO-PORT? (v5 never had it) | **Hunks:** ONE line in `app/salon/[id]/hooks/useImpersonation.ts:153` (`handleSetActiveSpeaker`'s `method: 'PUT'` → `'POST'`), a NEW jest test (`__tests__/unit/app/salon/hooks/useImpersonation.set-active-speaker.test.tsx`), `bugs.md` + NEW `bugs/fixed/bug-170-*.md`, the `4.10.0-dev.79` stamp (three `package*.json` + README), CHANGELOG. No `lib/`, `app/api/`, `help/`, DDL or oracle-import hunk. **v5 is NOT affected — measured, not assumed:** the SPA's speaker pick (`apps/web/src/app/screens/salon/salon-conversation.ts:2546` `onSelectSpeaker`) sends `chatSetActiveSpeaker` over the dispatch client — no HTTP method exists to get wrong — and the verb runs the real write (`api/salon.rs:1954`, `Request::ChatSetActiveSpeaker`; covered by `salon_mutations_equivalence` + `salon-conversation.spec.ts` / `salon-turn-controls.spec.ts`). v5's REST `/api/v1/chats/{id}` registers no PUT leg and its POST leg serves only `equip`/`regenerate-avatar` (`quilltap-web/src/lib.rs:436`), so neither v4's pre-`ad1c4c37f` silent no-op nor its post-dispatcher 400 toast is reachable on v5. v4's bug file asks "v5 status: Unchecked" — the answer is **not affected** (a candidate one-line upstream note). The bug-file mirror rides the catch-up round's unification with bugs 167–169. Ratify NO-PORT on this file list; it can fold into P4.D221's ratification list. **Newer fact (2026-09-24): the human WAIVED this commit for the P4.D220–P4.D224 lanes' §R.2 probe (§1); the row stays unordered and the round's target stays `b0b6656b5`.** | ORDERED(P4.D232 — NO-PORT ratification on the file list; v5 NOT AFFECTED, measured: the speaker pick is a dispatch verb with no HTTP method; the bug-file mirror pre-listed for the unifier) |
| `a8292547a` | 2026-09-24 | Add Concierge overhaul feature specs (overview + five phases) (#72) | NO-PORT? | `origin/main` ONLY at the unification's closing probe (local `main` at `83d0c969b`). Nine files, +1,008, all docs: `docs/developer/features/concierge-overhaul.md` + five `concierge-overhaul-phase-{1..5}-*.md` (refusal failover, a refusal ledger, three states, a Concierge tab, Salon polish — a ROADMAP for future v4 work, nothing shipped), `docs/developer/features/ROADMAP.md` (+1), `.claude/commands/update-documentation.md` (+6), CHANGELOG. No `lib/`, `app/`, `help/`, `packages/`, `plugins/` path. Ratify NO-PORT on the file list; the `docs/v4/developer/features/` mirror takes the six specs + the ROADMAP line then. **Watch:** when the phases LAND as code they will be PORT rows of their own — the Concierge's four states, list marks and choice-at-creation are ported surfaces (the `dcd9440a`–`31436bae4` arc). **Newer fact (2026-09-25): the watch FIRED — all five phases landed as code the next day (#73–#77, rows below), and the six specs were edited in those PRs; the mirror takes their final text.** | ORDERED(P4.D232 — NO-PORT ratification on the file list; the six specs + the ROADMAP line pre-listed for the unifier's `docs/v4/developer/features/` mirror at the ROUND target's bytes) |
| `8bd080267` | 2026-09-24 | Concierge overhaul phase 1: refusal-driven failover everywhere (#73) | PORT + PORT-NEW (the overhaul's substrate; ports FIRST) | **Hunks** (87 files, `4.10.0-dev.81`). **New modules:** `dangerous-content/refusal.ts` (`classifyRefusal`, `isModerationRefusal`, `MODERATION_REJECTION_CODE`, evidence ranked typed-error > provider-code > finish-reason > message-pattern > inferred), `understudy.ts` (`resolveUncensoredText/ImageUnderstudy`), `image-failover.ts` (`generateImageWithConciergeFailover`, `getConciergeTrail`). **Removed:** `provider-routing.service.ts` loses `isImageModerationError`, `resolveUncensoredImageProfileForReroute` and its key-decrypt helpers. **Changed:** `provider-failover.service.ts` gains `attemptUncensoredRetry` (a thrown refusal is retried on the understudy before the chain). `fallback/engine.ts` → `moderation-refusal` via `classifyRefusal`. `extract-finish-reason.ts` also reads camelCase `finishReason` + Google `promptFeedback.blockReason`. The Concierge writer gains `postConciergeRefusalAnnouncement` (rerouted / no-understudy / not-permitted). The avatar, story-background and image-tool handlers are rewritten round the chokepoint, and **the Lantern's bug-133 gate is REMOVED**. `app/api/v1/images/route.ts` generate goes through the chokepoint with a CONNECTION-profile understudy. **Wire:** the `routeTrail` JSON widens (`evidence` 2 → 5 values, `profileKind?`) in `chat.types`, the `chats-messages.ops` zod and `qtap-export.schema.json`, with no DDL column. TOOL rows and Lantern rows now carry trails. **SPA:** a `ToolMessage` "Tried:" badge and the `refusal` system label. **Plugins:** `@quilltap/plugin-types` 2.8.0 `ModerationRejectionError`. Five plugins' image providers throw it (OpenAI `moderation_blocked`, Z.AI 1301, Google `IMAGE_SAFETY`), and their text providers report the REAL finish reason (it had been hard-coded `'stop'`); every `index.js` bundle is regenerated. **v5 surfaces hit:** `services/route_trail.rs` (P4.D171/D173/D177: two-value evidence, no `profile_kind`), `llm_fallback/`, `services/provider_failover.rs` (P4.D135, P4.87/P4.90), `services/dangerous_content/provider_routing.rs` + its callers `tools/generate_image.rs`, `services/image_job_common.rs`, `model/image.rs`, `model/image_dialects.rs` (W4.2; bug 133 = P4.D178/P4.88), `character_avatar_job.rs`, `story_background_job.rs`, `concierge_notifications.rs`, the Lantern writer, `finish_reason.rs`, `quilltap-web/src/images_routes.rs` + `quilltap-host/src/images_generate.rs`, and the SPA's `tool-message.ts` / `route-trail-badge.ts` / `system-message-labels.ts`. **Traps:** v5's dialect layer is built on the keyword verdict v4 just deleted (`image_dialects.rs:35`, "never widen the keyword set"), so for v5 the typed error is a REDESIGN of the signal, not a rename. The plugin bundle regen moves the recorded stream corpora (the finish reasons). The export-schema enum widens, so import/restore validators must accept it. Needs v4's `npm install` (§1) before any target-pin regen. Two review fixes v4 deferred to phase 2's spec. | ORDERED(P4.D225 — the server whole at pin `49059fb14`; the client hunks P4.D229; the `help/` half P4.D228) |
| `49059fb14` | 2026-09-25 | Implement Concierge refusal ledger and auto-switch (phase 2) (#74) | PORT (after #73) | **Hunks** (38 files, `-dev.83`). **Migration** `add-chat-refusal-ledger-v1`, appended last: `chats.moderationRefusalCount INTEGER NOT NULL DEFAULT 0` + `lastModerationRefusalAt`, no backfill, deliberately absent from `ChatMetadataSchema` and from `.qtap` exports. **`ChatsRepository`:** `incrementModerationRefusalCount` (atomic `$inc`, read-back count), `getModerationRefusalLedger`, `resetModerationRefusalLedger`. **New `refusal-ledger.ts`:** `recordModerationRefusal` counts STATED evidence only, never `inferred`; it never throws and only buffers in the job child. `maybeAutoSwitchAfterRefusal` is parent-only, chained per chat, re-reads before flipping, and has threshold 0 = off. **Other changes:** `manual-flip` gains options + `dangerCategories: ['moderation-refusals']` on a Concierge flip, and the ledger reset in the `monitored` arm. `job-dispatcher`'s post-commit `runRefusalLedgerChecks`. The writer's `auto-flagged-refusals` kind (count wording), and the refusal-rerouted text dropping "The result is attached above." Cheap-LLM `core-execution` carries `finishReason`. Text failover records once per turn, image failover on all four exits. **Settings:** `autoSwitchAfterRefusals` int 0–10, default 2. The SPA number input and `help/dangerous-content.md`. **v5 surfaces hit:** `db/chats.rs` (raw SQL, kept OUT of `chats_read.rs` `ALL_COLUMNS` and `ChatUpdate`), `db/chat_settings.rs`, `provisioning/chat_settings_seed.json` (re-dump), `services/dangerous_content/{resolver,manual_flip}.rs` (P4.d5/W4.2, P4.D141), `concierge_notifications.rs`, `write_apply.rs` (the post-commit hook; no production consumer yet), `cheap_llm_exec.rs`, `provider_failover.rs`, and the SPA's `dangerous-content-settings.ts`. **Nothing in v5 matches the ledger.** **To MEASURE, not assume:** whether v4's `generateDDL` emits the two schema-absent columns. If it does not, they need a boot ensure on the P4.D182 `chats_transcript_version_repair.rs` model and must NOT be hand-added to `fresh_schema.json`. **Largely SUPERSEDED in shape by #75/#76** (`flagged` → `unmoderated`, the settings move into `conciergeSettings`). Port its mechanism, not its intermediate vocabulary, unless the round deliberately lands per-phase. | ORDERED(P4.D225 — the server whole at pin `49059fb14`, incl. the ledger columns as a BOOT ENSURE (measured: `generateDDL` walks the Zod schema and cannot emit them); the settings input's client hunk is DEAD at #76 (P4.D230 records); the `help/` half P4.D228) |
| `4d370a90f` | 2026-09-25 | Concierge overhaul phase 3: three states (Moderated, Unmoderated, Locked) (#75) | PORT (after #74; REPLACES v5's four-state Concierge) | **Hunks** (108 files, `-dev.86`). **Migration** `add-chat-concierge-mode-v1` (depends on the ledger): `chats.conciergeMode` (default `'moderated'`), `conciergeModeSetBy`, `conciergeModeReason`. The backfill maps `UNCENSORED` → unmoderated/operator/migration, `OFF` → locked/operator/migration, NULL+`isDangerousChat` → unmoderated/concierge/classifier, else moderated. `conciergeOverride` is kept but no longer written. **Schema:** `chat.types` gains the three enums on `ChatMetadataSchema`. **Repository:** `base.repository.patchOnlyFields()` (a whole-row `_update` omits them), `chats.setConciergeMode` (compare-and-set vs `expected`, NULL = moderated) and `setDangerClassification` (never moves state). **Services:** `chat-override` (`getConciergeState/Provenance/Reason`, `mayFailOver`, `withConciergeModeFromLegacy`), and `manual-flip` writing only the new columns (announcements `set-moderated/-unmoderated/-locked`, `auto-unmoderated`; the Concierge may only move Moderated → Unmoderated, never from the child). New `classifier-switch.ts` + `current-state.ts`. Resolver sources `chat-locked`/`chat-unmoderated`, `LOCKED` replacing `VOUCHED_SAFE`. The three-row presentation table. A refusal on Locked → `refusal-not-permitted`. The job-dispatcher's post-commit classifier switch. **Wire:** PUT/POST `conciergeState` takes `moderated\|unmoderated\|locked` (old values → 400). The chat GET drops `conciergeOverride` and adds `conciergeState/SetBy/Reason/RefusalCount`. The create response carries the post-flip columns. List payloads (enrichment, `chat-utils`, home-data) gain setBy/reason. The greeting's content-filter fallback is skipped on Locked. Restore + `.qtap` import run `withConciergeModeFromLegacy`, and the export schema marks `conciergeOverride` deprecated. **SPA:** three-option selects (sidebar, New Chat), `ConciergeMark`, `ChatCard`, quick-hide. Help rewritten. **v5 surfaces hit (all REPLACED):** P4.D141 four states, P4.D143/D144 list marks, P4.D148/D149 choice-at-creation. `services/dangerous_content/{chat_override,manual_flip,resolver,gatekeeper_job}.rs`, `services/{chat_create,chat_enrichment,concierge_notifications,danger_scan}.rs`, `db/chats.rs` + `chats_read.rs`, `api/{salon,types}.rs`, `services/backup/restore/orchestrator.rs`, `services/quilltap_import/entities.rs`, `generators/qtap-export.schema.json`, `qtap_export/schema-key-order.json`, and the SPA's `chat/concierge-state*.ts` / `concierge-mark.ts` / `chat-sidebar.ts` / `conversation-header.ts` / `new-chat.types.ts` / `recent-chat-item.ts` / `salon-list.ts` / `project-chats-section.ts` + `e2e/salon-concierge-four-state-flow.spec.ts`. **Fixtures/families to widen:** `danger_resolver`, `danger_trigger`, `chat_create_capstone`, `salon_mutations`/`salon_reads`, `projects_routes`, `characters_reads`, `system_restore_state`. The old enum → 400 moves `dispatch_wrong_type_census` and the P4.D141 refusal arms. Pre-phase-3 transcripts carry retired announcement kinds that must still render. | ORDERED(P4.D226 — the server whole at pin `4d370a90f`, stacked on P4.D225, incl. the D23 re-dump + the widening of all 42 chats-bearing pairs through v4's real migration modules; the client hunks P4.D229; the `help/` half P4.D228) |
| `3b463d6b1` | 2026-09-25 | Concierge overhaul phase 4: the Concierge's own Settings tab (#76) | PORT (large; after #75) | **Hunks** (183 files, `-dev.88`). **Migrations:** `add-concierge-settings-v1` adds `chat_settings.conciergeSettings` JSON `{enabled, uncensoredText/Image/VisionProfileId, imagePromptProfileId, autoSwitchAfterRefusals, newChatsStartAs, display{}, preScreen{}}`. Its backfill (`mapLegacyConciergeSettings`): enabled = mode≠OFF OR the user has an Unmoderated chat; preScreen + summaryClassification = mode≠OFF. It drops NO old column; `dangerousContentSettings` / `uncensoredImageDescriptionProfileId` stay in the DDL, deprecated and unread. **`drop-chat-concierge-override-v1`: `ALTER TABLE chats DROP COLUMN conciergeOverride`** (§1's live-instance warning). **Schema:** `settings.types` removes `DangerousContentSettingsSchema` / `DangerousContentModeEnum` and adds `ConciergeSettingsSchema`. `ChatSettingsSchema` loses both legacy keys, `CheapLLMSettings` loses `imagePromptProfileId`, and `chat.types` loses `conciergeOverride`. **Services:** `resolveConciergeSettings`/`readConciergeSettings` (a named-question policy) replace mode checks in ~40 consumers: the background jobs, cheap-llm, memory, Pascal `llm-consult`, photos auto-describe, appearance resolution, `file-attachment-fallback`, the Almanack, image-gen, failover. New `legacy-concierge-settings.ts`. **Routes:** the settings/chat PUT answers **400 on any retired key**. Restore + `uuid-remap` translate the old settings and remap the four desk ids. **Tools:** `help_settings` gains a `concierge` category, so BOTH tool-definition snapshots move (the `helpSettings` enum + description, the `helpNavigate` example URL; the count stays 59). **Help:** `dangerous-content.md` DELETED, `the-concierge.md` ADDED (129 stays 129). `lib/help-guide/categories.ts` swaps the slug and adds `/settings?tab=concierge`. **SPA:** `ConciergeTabContent` + five cards, `DangerousContentSettings` deleted, the `qt-components` CSS drops the `-info` tone, `theme-storybook` 1.0.73. **v5 surfaces hit:** `services/dangerous_content/resolver.rs` (P4.6a/6b, P4.71, P4.D37/38/48/201), `db/chat_settings.rs` (P4.D73, P4.76, P4.82), `api/settings.rs` (`zod_dangerous_content_settings`; P4.57, P4.6an), `tools/help.rs` + `tools/definitions/data.rs`, `services/backup/uuid_remap.rs` (P4.9, P4.D49), `almanack/phase{2,3}_*.rs`, `danger_scan.rs`, `file_fallback.rs`, `generate_image.rs`, `story_background_job.rs`, `fresh_schema.json` + `chat_settings_seed.json`, the SPA's `settings/chat/dangerous-content-settings.ts` + `chat-tab.ts` (no Concierge tab exists), `help/help-categories.ts` (P4.9I2B) + its `help-guide-tables.json` / `label-from-url-vectors.json` fixtures. **Fixtures carrying `dangerousContentSettings`:** `chat_settings_tier2`, `settings_routes`, `danger_resolver`, `orchestrator_tier3`, `images_generate_route`, `image_generation_tier3`, `story_background_job_tier3`, `host_cadence`, `host_llm_log_cleanup`, and the SPA's `async-select-cards` / `connection-profiles-shared-entry` / `core-contract.ts`. | ORDERED(P4.D227 — the server whole at pin `3b463d6b1`, stacked on P4.D226, incl. the consumer sweep, both migrations, the second re-dump and the widening of the 42 + 23 pairs; the client hunks + `lib/help-guide/categories.ts` P4.D230; the `help/` half P4.D228) |
| `ce2f1dabf` | 2026-09-25 | Concierge overhaul phase 5: Salon polish — "Try uncensored" (#77) | PORT-NEW (after #76) | **Hunks** (53 files, `-dev.90`). **New chat action `retry-image-uncensored`** (in `post.ts`'s map right after `regenerate-background`). Its body is `{toolMessageId}` or `{kind:'background'}` (else 400). It re-runs a `generate_image` TOOL message on the image understudy, filed +1 ms with a `via:'concierge'` trail, with `refusal-rerouted` (`purpose:'tool'`) when the original was refused → 200 `{toolMessageId, images, routeTrail}`. The error arms are 404 / 400 / 502, and 409 bare `{error:'locked'\|'no-understudy'}`. The background arm calls `handleRegenerateBackground(…,{forceUncensored:true})`. **New message action `retry-uncensored`** (5th in `chats/[id]/messages/[messageId]`'s `withActionDispatch`): a swipe regenerated on the text understudy → 201 `{message}` or SSE on `&stream=1`, plus 404/400/409. **Refactors and service changes:** `messages/[id]`'s SSE body is extracted to `regenerate-swipe-stream.ts` (a refactor, no wire change). `regenerate-swipe.service` gains `profileOverride` + `routeTrail`. New `retry-uncensored.ts`: Locked blocks, off-duty does not, it excludes trail profiles and same provider+model, and uses `resolveConfiguredConciergeDesk` + `composeRetryRouteTrail`. **danger-orchestrator:** Unmoderated chats no longer get synthesized `dangerFlags` (new `routedDirect`). **Smaller changes:** `saveToolMessages` `options.createdAt`, the queue payload's `forceUncensored`, `image-failover`'s `announceUnresolvedRefusal`. The Lantern writer's `postLanternRefusalNotification` (`systemKind:'background-refused'`). The story-background job now COMPLETES with a refusal bubble instead of failing. **SPA:** "Try uncensored" in MessageActionBar / ToolMessage / the refusal bubble (hidden on Locked), `useConciergeRetry` + `concierge-retry.ts`, MessageRow's "Not Dangerous" lifting blur/collapse, the `background-refused` label. Three help pages, API.md. **v5 surfaces hit:** `CHAT_POST_ACTIONS` (`quilltap-web/src/wardrobe_routes.rs`) and the P4.D220 action censuses. The message edge is RPC-only on v5 (`api/types.rs` `Message*` variants, P4.6ab), with no REST `chats/{id}/messages/{messageId}` leg in `quilltap-web/src/lib.rs`. `messages_swipe_routes.rs` + `services/regenerate_swipe.rs` (P4.D205/D207) and the SPA's `regeneration.state.ts` (P4.D206). `lantern_notifications.rs`, `story_background_job.rs` (P4.D92/D94/D178), `tool_execution.rs`, `queue_service.rs`, `services/orchestrator.rs` (the synthesized flags), the SPA's `message-row.ts` / `tool-message.ts` / `system-message-labels.ts`. **Traps:** the v4 order of the message edge's action list starts with `override-danger-flag`, which v5 never ported, so the list's order can't be matched without it. The "Not Dangerous" blur has no v5 counterpart. The orchestrator's `dangerFlags` change is the one independently portable hunk. | ORDERED(P4.D228 — the server whole at the round target, stacked on P4.D227, + the WHOLE `help/` tree re-vendored at `08c49319d`; the client hunks P4.D229) |
| `6d0f88d65` | 2026-09-25 | chore(deps): update dependencies across app, packages and plugins | NO-PORT? (code) + **REGEN EVENT** (SDK bumps) | **Hunks** (51 files, `-dev.91`). Dependency manifests, lockfiles and rebuilt bundles only. **Root:** `openai` ^7.20.0 → **^7.23.0**, `@openrouter/sdk` ^1.3.11 → **^1.3.28**, `next`/`eslint-config-next` 16.3.5 → 16.3.6, `katex` 0.18.7 → 0.18.9, `@quilltap/plugin-utils` ^2.6.1 → ^2.6.2, `create-quilltap-theme` ^2.0.20. **`packages/plugin-utils`** 2.6.3 (`version.generated.ts`; its own dep on plugin-types moves to ^2.8.0). **All 15 plugins are patch-bumped** (manifest + package.json), and 12 `index.js` bundles are rebuilt (~3,067 lines each). The deps move to plugin-types ^2.8.0 / plugin-utils ^2.6.2, `openai` ^7.23.0 in the six SDK-bundling plugins, `@openrouter/sdk` ^1.3.28, and `@modelcontextprotocol/sdk` ^1.30.1. **No `lib/`, `app/`, `help/`, DDL or `zod` hunk.** **v5 surfaces hit:** no source. **`crates/quilltap-harness/tests/provider_sdk_version_guard.rs`** (P4.106 item 9) records `openai` 7.20.0 / `@openrouter/sdk` 1.3.11. The checkout now has 7.23.0 / 1.3.28 installed at the root and in every SDK-bundling plugin dir (measured at this check), so the guard is **RED by design**. Its two halves force the corpora (`request-envelopes.recorded.ndjson` ×216 openai stamps + ×14 OpenRouter UA, and `image-dialects.recorded.ndjson` ×8 + ×3) to be re-recorded together with the constants. Also moving: `openrouter_sdk_pricing_equivalence` (the SDK's own pricing tables at 1.3.28) and any stream-decoder/request-builder family whose bytes the new SDKs change. A re-record may show wire movement beyond the stamps; measure it, don't assume stamp-only. `zod` is untouched (guard stays green). The `@modelcontextprotocol/sdk` bump is unguarded; v5's MCP client (if any family records it) should be checked. **To do:** ratify NO-PORT for code, and ORDER the provider-corpus re-record + guard constant move as a row of its own (a pin-independent chore, since `node_modules` are shared by every pin, §1). | ORDERED(P4.D232 — the `request-envelopes` re-record + the guard constants; `image-dialects`/`response-bodies`/the four #73 stream files are P4.D225's; NO-PORT ratification for code) |
| `08c49319d` | 2026-09-25 | Scenario Builder on the scenario shelves | PORT (small; independent of the Concierge chain) | **Hunks** (27 files, `-dev.92`). **Request:** the build request gains `groupIds: z.array(UUIDSchema).max(32).default([])` (`lib/scenario-builder/request-schema.ts`). **Route** (`app/api/v1/scenario-builder/route.ts` `handleBuild`): dedups `body.groupIds`, keeps only ids `repos.groups.findByIdRaw` finds, and on a throw logs WARN `Scenario Builder dropped an unreadable group id` `{groupId, error}`. It adds `groupCount` to the route's log fields and passes `groupIds` to the service. **Service:** the input gains `groupIds` and logs `namedGroupCount`. **Mount pool** (`lib/scenario-builder/mount-pool.ts`): named groups' stores join the group tier FIRST, then the cast's union, deduped, and it logs `namedGroupCount`. **`lib/mount-index/tiered-mount-pool.ts`:** new exported `resolveMountPointIdsForGroup` (official + linked stores, fails soft with `[]`). `resolveGroupMountPointIdsForCharacter` now calls it. **Its warn CHANGES:** `Group store lookup failed for membership {groupId, characterId, error}` → `Group store lookup failed {groupId, error}`. **Client:** `ScenariosManager` takes a `shelf` prop that renders the Host button, used on the General Scenarios page, the project Scenarios card and a NEW `GroupScenariosCard` on the group page. Launched from a shelf, the dialog has no "Use this scene"; Save offers every home with the shelf's home preselected; save-dialog target keys become `project:<id>` in every mode; the shelf refreshes after a save; `queryKeys.groups.list`. **Help:** four pages (`general-scenarios`, `groups`, `project-scenarios`, `scenario-builder`; count unchanged). **v5 surfaces hit:** the Scenario Builder ported whole at P4.D216 → P4.D217 (+ P4.113–P4.117 smalls): `crates/quilltap-core/src/api/scenario_builder.rs` + `api/types.rs` (the request's typed shape; a new field moves the `dispatch_wrong_type_census` if it is an `*_ids` array, memory `a-new-verb-moves-the-dispatch-wrong-type-census`), `services/scenario_builder/{mod,mount_pool}.rs`, `db/tiered_mount_pool.rs:213` `resolve_group_mount_point_ids_for_character`. That last one's per-membership closure is SILENT (`let _ = per_membership();`), so v5 lacks v4's warn in both its old and new wording, a pre-existing absent line to restore with a capture pin. The families are `scenario_builder_mount_pool_equivalence`, `tiered_mount_pool_equivalence`, `scenario_builder_tier3_equivalence` and `scenario_builder_capture`. SPA: `apps/web/src/app/scenario-builder/{scenario-builder-dialog,save-scenario-dialog,scenario-builder-run.state}.ts` + its oracle fixture, `screens/scenarios/shared/scenarios-manager.ts`, `screens/scenarios/scenarios-page.ts`, `screens/prospero/cards/`, `screens/groups/` (no group Scenarios card exists), `core-contract.ts`. **Traps:** the target-key change (`project:<id>` in every mode) is a save-dialog wire detail the recorded SPA oracle fixture will show. The dedup precedes the existence check, and an unreadable id is dropped with a WARN while an absent one is dropped silently. | ORDERED(P4.D231 — server + SPA at the round target; the `help/` half P4.D228; measured: the dispatch census does NOT move — the build body is a raw `Value`) |
| `acadcc7cd` | 2026-09-25 | Name who stayed behind on Continue Elsewhere; persona absent in autonomous rooms (bugs 171, 172) | PORT (small; v5 HAS both bugs; independent of the Concierge chain) | **Hunks** (19 files, `-dev.93`). This is the dirty set the `/setupphase` probe recorded at ~14:20, now committed. **`lib/schemas/chat.types.ts`:** NEW pure `operatorSpeaksWithoutSeat(chatType)` = `chatType !== 'autonomous'`. That is its only hunk; nothing Concierge. **`user-identity-resolver.service.ts`:** NEW `isUserPersonaInRoom(chat, identity)`: no `characterId` → false, `source === 'chat-participant'` → true, else `operatorSpeaksWithoutSeat`, with DEBUG `Unseated persona presence resolved {characterId, chatType, inRoom}`. **`context-manager.ts` (bug 172):** the off-scene scan's persona-by-name exclusion is now GATED on `operatorSpeaksWithoutSeat(chat.chatType)` (a seated persona is still excluded by id), with a new DEBUG `[ContextManager] Off-scene persona exclusion {chatId, chatType, excludesPersonaByName}`. **`apply-chat-continuation.ts` (bug 171):** new step 2b after the replay. `findLeftBehindCharacters` takes the source chat's CHARACTER participants (not `removed`) who are not seated in the new chat, deduped, minus the persona when `isUserPersonaInRoom` (DEBUG `…Persona stays in the room unseated…`), `characters.findById` each (missing → skip; throw → WARN `[ChatContinuation] Could not load a left-behind character; not naming them`). It posts `postHostOffSceneCharactersAnnouncement({…, reason: 'left-behind'})`, stamps the ids only when the notice posted, logs DEBUG `Left-behind check complete`, and the whole step sits in a try with ERROR `Failed to name left-behind characters`. The result gains `leftBehindCharacterIds` on ALL three returns. **`host-notifications/writer.ts`:** `OffSceneCharacterCard` exported, new `OffSceneIntroductionReason = 'mentioned' \| 'left-behind'` param (default `'mentioned'`) on both the content and opaque builders, with FOUR new intro sentences (singular/plural × visible/opaque; the old two stay for `'mentioned'`). **Help:** `chats.md`, `salon-host-introductions.md`. Also `bugs.md` + two `bugs/fixed/` files. **v5 surfaces hit:** `crates/quilltap-core/src/services/chat_continuation.rs:271` `apply_chat_continuation` (Phase-3 sub-unit 6; `chat_continuation_tier2_equivalence`, `route_trail_continuation_guard`). `services/off_scene.rs:284` (the UNCONDITIONAL `user_name_lower` exclusion, so v5 has bug 172 verbatim) and its caller `services/build_context.rs:~1956` (the W4.6b off-scene seam; `context_feeders_leaves_equivalence`). `services/user_identity_resolver.rs` (`user_identity_resolver_equivalence`). `services/host_notifications.rs:1137` `post_host_off_scene_characters_announcement` (`post_office_writers_tier3_equivalence`). The chat-types home for the pure predicate. **Not a CONVERGENCE:** both bugs were v4-found on Friday; v4's "v5 status" reads Unchecked, and the answer is **affected, both**. **Traps:** the ids are stamped only on a POSTED notice, so a failed post must leave the per-turn scan free to introduce them. The persona check uses the NEW chat's identity. `removed` participants are skipped but other non-present statuses are not. The two new tests' fixtures need the continuation family's corpus widened with a left-behind arm and an autonomous-room arm. **Round fit:** it lands after the `08c49319d` target. Either waive it for the current round (the next catch-up's first row) or fold it into a lane touching none of the Concierge files, e.g. P4.D231/P4.D232's scale; see §1. | ORDERED(P4.D233 — the server whole from `main` (a NEW ninth lane added at the same-day re-order that moved the round target to `acadcc7cd`; ONE pre-declared hunk into `build_context.rs`); the two `help/` pages P4.D228; the bug-file mirror pre-listed for the unifier) |

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

- **The `b0b6656b5` ten-commit drift catch-up round (2026-09-25, baseline
  `d1c06cd9d` → `b0b6656b5`; P4.D220 ∥ P4.D221 ∥ P4.D222 ∥ P4.D223 ∥
  P4.D224):** `ad1c4c37f` (the ONE `dispatchAction` primitive) ABSORBED(P4.D220
  the web edges + core dispatch — `query::dispatch_action` /
  `dispatch_required_action`, the folding `query::action()` DELETED, every
  consumer's FULL v4 list in v4's order, the four data-affecting arms
  red-first [a bare `?action=` had DELETED a chat, RUN a restore, UPLOADED a
  file, CREATED a rule], the unlock edge's awaited catch, the two WARNs at
  v4's field names, `query_param_semantics` re-recorded with `fold` false
  everywhere; P4.D221 the `lib/` riders — the two mount-index partition keys
  red-first, the Brahma `qt_text()` sentence regenerated, `escapeLikeLiteral`
  NEUTRAL, six dead v5 twins retired with their oracle rows). `68da64d9b` (the
  chat GET's fourteen keys) ABSORBED(P4.D220 — gated BEFORE any chat read;
  the absent arm's pointer pinned both ways). `944127d9a` (the
  `?action=has-dangerous` removal) ABSORBED(P4.D220 the server half —
  `Request::ChatsHasDangerous` retired end to end; P4.D223 the client half —
  the probe, the three-way gate and the contract member deleted, the section
  ALWAYS offered). `492771aff` (bugs 167/168) ABSORBED(P4.D222 — the HELP_DOC
  job by SECTION with `average_embeddings` [a NEW tier-1 family, f32
  bit-exact], `count_by_doc`, the sync shape by id, `reconcile_help_docs` +
  the once-per-boot `HelpDocReconcileGate`, the boot order 3.66 before 3.7,
  `help/` 127 → 129 whole; **bug 168 IS dogfood #120 — CONVERGED by v4's
  mechanism**). `e3937d7aa` (the Salon Images quick-hide) ABSORBED(P4.D223 —
  `hideSalonImages` + the Salon-scoped `IMAGES_HIDDEN` token + v4's two
  stand-ins at every Salon image site; a v5-only XSS in the inline stand-in's
  string swap found and fixed at unification). `3376b3dfa` (the `@` mention
  typeahead) ABSORBED(P4.D224 — the pure module recorded against v4's real
  one [116 rows], the ProseMirror plugin, the composer wiring + §S.2; the char
  typeahead's soft-break defect found and fixed with it; P4.D222 the `help/`
  half; P4.D221 the Rust Carina neutrality re-run, 67/67 byte-identical).
  `b0b6656b5` (bug 169) CONVERGENCE-RETIRED(P4.D223 — v5's predicate made
  v4's exact twin, every "deliberate divergence" wording retired).
  `7ebb74143` (the release script) and `8aafd595d` (the dedicated-db
  repository base — its item (1) taken as P4.D221's Tier-2 small, the joined
  read's inner ERROR) NO-PORT-RATIFIED(P4.D221, on the file lists).
  `94e946728` (this port's bug-169 filing) NO-PORT-RATIFIED(P4.D223, docs
  only). Round record: `status-log.md` → "Round record — the `b0b6656b5`
  ten-commit drift catch-up round unification".
- **The `d1c06cd9d` Scenario Builder drift catch-up + maintenance round
  (2026-09-23, baseline `00c290c9a` → `d1c06cd9d`):** `d1c06cd9d` (the
  Scenario Builder) ABSORBED(P4.D216 the substrate — the shared
  `run_one_shot_tool_loop` with v4's five new debug lines, `DocToolsMode` +
  the extras bag + the third `search` variant, the stream call's `log_type` +
  `SCENARIO_BUILDER`, the pre-built mount pool through the executor / search /
  doc-edit / path resolver, `groupList { characterIds }`; P4.D217 the service,
  the three verbs + `scenarioBuilderProgress` frames, the host driver, the SSE
  edge, `help/` 126 → 127; P4.D218 the SPA dialog + save dialog + both entry
  points + the log labels, beats LIVE at unification; P4.D219 bug 165 — v5 had
  it verbatim; bug 166 NO-COUNTERPART, the modal never ported). `dff00e98d`
  (the handoff spec) NO-PORT-RATIFIED(the same round — docs-only by `git show
  --stat`; its file is mirrored in `d1c06cd9d`'s final text). Recorded
  divergence: `curlConfigured` always false (v5 builds no plugin tools).
  Round record: `status-log.md`.
- **The `00c290c9a` bug-163/164 drift catch-up + maintenance round
  (2026-09-23, baseline `a2db63da7` → `00c290c9a`):** `00c290c9a` (bugs
  163/164, the auto-title chokepoint) ABSORBED(P4.D215 — NEW
  `services/auto_title.rs` with v4's four outcomes, the post-LLM re-read, the
  refused arms' `extraPatch`-only write, the enqueue MOVED in with its lines
  re-prefixed; under the fold [its hand-renamed early return + its catch], the
  title-update job [the removed `Updated title` line; an unchanged title queues
  nothing] and regenerate-title [`clearManualRename`; v4's one outer catch over
  every read, fixed at unification]; `help/story-backgrounds.md`;
  `chat-admin-main.db` widened; the two bug mirrors + `bugs.md` and the
  bug-146–154 mirror lag by the unifier). Round record: `status-log.md` →
  "Round record — the `00c290c9a` bug-163/164 drift catch-up + maintenance
  round unification".

- **The `a2db63da7` bug-161/162 drift catch-up + maintenance round
  (2026-09-23, baseline `f45a517a9` → `a2db63da7`):** `4e1a8e061` (docs:
  bugs 161/162 filed + the summarizer-name design) NO-PORT-RATIFIED(P4.D212,
  mirrors — `context-summary-speaker-names.md` at its FINAL path, both bug
  files, `API.md`, `bugs.md`); `e7821606f` (bug 161 + the PORT-NEW
  rebuild-summary) ABSORBED(P4.D212 ∥ P4.D213 — ONE `speaker_names`
  resolver shared by the fold and the episode pass, the fold turn's required
  `speaker`, `FOLD_SUMMARY_PROMPT` regenerated mechanically, the debug line,
  `Request::ChatRebuildSummary` with v4's 409/400 order, the single update,
  the enqueue at priority 0, the `chats` publish, `help/chats.md`; the SPA's
  Organize entry, confirm and toasts, the beat run LIVE at unification —
  `orchestrator_tier3`'s `lastTurnParticipantId` CONVERGENCE pin retired:
  v4 persists the write from this commit on); `a2db63da7` (bug 162)
  ABSORBED(P4.D214 — the ONE CLI opener with v4's per-target strings, Tier R
  244 → 266 at both pins, P4.D210's live rows as canned-stub rows, the
  `packages/quilltap` README mirror). Round record: `status-log.md` →
  "Round record — the `a2db63da7` bug-161/162 drift catch-up + maintenance
  round unification".

- **The `f45a517a9` thirteen-commit drift catch-up round (2026-09-22,
  baseline `baa85e19b` → `f45a517a9`):** `186eb09cb` (bugs 159/160, the
  codec) ABSORBED(P4.D203 ∥ P4.D209 — the brotli codec + `qt_text` UDF on
  every connection, byte-parity with v4's real module; bug 160's third
  column; bug 159's image half as a seam wired ONLY into the sync applier —
  the un-wired sites are OPEN by name); `f45a517a9` (FTS5 + the four
  compressed columns) ABSORBED(P4.D203 → P4.D204 — the five FTS objects
  verbatim, the boot reconciler, `fts_query`, the two SQL shapes, the
  snippet fold, the `.`-defect retired); `80a05a4c8` NO-PORT-RATIFIED(P4.D203,
  mirror); `781e3b499` NO-PORT-RATIFIED(P4.D205, mirror); `e7d77bb60` (Inform)
  ABSORBED(P4.D205 ∥ P4.D206 — the table + D23 + repository + prompt block +
  consumption + the three verbs + export/import/backup/restore; the SPA
  dialog/chips/gutter; seven server items OPEN by name in the order header);
  `f564b0de3` (the streamed swipe) ABSORBED(P4.D207 ∥ P4.D206 — the watched
  stream, four beats, the frames verbatim incl. `kind`, the SSE edge; the
  SPA plate and bug-(b)/(c) fixes, the (b) fix repaired at unification);
  `0ecc6067c` NO-PORT-RATIFIED(P4.D210, mirrors); `23da0b322` (`quilltap
  sync` + bugs 155/156) ABSORBED(P4.D209 → P4.D210 — the repo-layer
  contract T; the nine engine modules, `Request::MountSync`, the CLI verb,
  Tier R 223 → 244; the live Tier R rows OPEN by name); `0c14fd61f` (bug
  157) ABSORBED(P4.D209 — `link_id` required, every caller); `5e4252898` +
  `e11a51f44` NO-PORT-RATIFIED(P4.D211, mirrors incl. the widened
  `docs/v4/packages-quilltap-README.md`); `6b0615807` (the npm update)
  ABSORBED(P4.D211 — Zod 4.6.5 measured neutral, `is_zod_email` converged
  at tier 2, the nine recorders re-recorded: two SDK stamps moved and
  nothing else); `da9c4f34f` (bug 158) ABSORBED(P4.D208 — the seed
  deleted, ONE predicate, the greeting block, the Concierge arms, the boot
  heal). Round record: `status-log.md` → "Round record — the `f45a517a9`
  thirteen-commit drift catch-up round unification".

- **The `baa85e19b` bug-154 default-system-prompt drift catch-up +
  maintenance round (2026-09-18, baseline `89fcc3c0d` → `baa85e19b`):**
  `baa85e19b` (bug 154) ABSORBED(P4.D201 ∥ P4.D202 — server: the
  `quilltap_core::default_system_prompt` resolver home (tier-1 exact over a
  22-case committed corpus against v4's REAL module) folded into the chat
  initializer (the `??`-on-content change) and the impersonation voice
  preview (which had reproduced v4's PRE-fix stale-column chain verbatim);
  `systemPromptsPatch`'s twin `project_system_prompts` so all four
  system-prompt writers move the `isDefault` flags AND the
  `defaultSystemPromptId` column in ONE patch, with v4's transient-id null
  rule; `set_default_system_prompt(Option<&str>)`'s clear arm; the
  `characterUpdate` chokepoint route (the pull-out, the empty-payload rule,
  v4's 400 after the generic write — and, at unification, the uuid half of
  v4's `z.uuid()` gate, red-first); the arrays family's per-op
  `defaultColumnTrail`; seven `characters_mutations` arms; the widened
  `characters-{main,mount}.db`; `help/character-system-prompts.md` at 124
  files. SPA: the client-safe twin over v4's five vectors verbatim, the two
  broken New-Chat seeds red-first, the announcement dialog's neutral fold,
  the star's optimistic cache write with rollback-then-refetch, the 4xl
  dialog, the live star beat (`P4D201_SERVER_LANDED` flipped at
  unification). The non-lib files: the two new test files + the PUT route
  test → the corpus; the help page re-vendored; README/CLAUDE/bugs/stamps
  NO-PORT.) Round record: `status-log.md` → "Round record — the
  `baa85e19b` bug-154 default-system-prompt drift catch-up + maintenance
  round unification".

- **The `89fcc3c0d` opacity-covenant drift catch-up + follow-ups round
  (2026-09-18, baseline `bcd7e4852` → `89fcc3c0d`):** `1065a1f53` (bug 152)
  + `89fcc3c0d` (bug 153) ABSORBED(P4.D200 — the `hide_character_vaults`
  flag on `PathResolutionContext` set by both builders which keep
  `character_id`; the tiered pool's `FlattenOptions { include_character_tier }`;
  the collector's two `vaults_visible` uses; the self-token gate's new
  conjunct; `find_enabled_mount_point_by_ref` + the NOT_FOUND → ACCESS_DENIED
  split with v4's three-sentence message and the two `vaultsHidden` warns +
  `describe_characters`; `AccessibleMountPointsQuery` on
  `get_accessible_mount_points` derived at all four enumeration call sites;
  the five pre-existing absent path-resolver/opacity-helper warn lines; the
  NEW real-DB `doc_opacity_equivalence` family over `build-doc-opacity-
  fixture.ts` mirroring v4's 13 + 15 regression cases, red-first 21/44;
  `help/character-system-transparency.md` re-vendored; the non-lib files
  ratified on P4.D200's list — `lib/doc-edit/index.ts`'s two type re-exports
  NO-PORT (v5's items are `pub`), the two `__tests__` files → the corpus,
  README/package stamps NO-PORT, the four docs mirrored under `docs/v4/`).
  Round record: `status-log.md` → "Round record — the `89fcc3c0d`
  opacity-covenant drift catch-up + follow-ups round unification".

- **The `bcd7e4852` bug-151 drift catch-up + follow-ups round (2026-09-17,
  baseline `5f0a57dc4` → `bcd7e4852`):** `bcd7e4852` ABSORBED(P4.D198 — the
  loader half: `files/llm_image_budget.rs`, the fallible `ImageTranscoder::
  shrink_to_webp` + `HostImageCodec`'s impl, the shrink ahead of the backstop
  at both loaders in `services/chat_files.rs`, the NEW `llm_image_budget_
  equivalence` tier-1 family with `sharp` scripted below v4's real function,
  `file_attachment_tier3` grown; P4.D199 — the walk half: the per-turn budget
  in `services/message_context.rs` section K over the seam's `LanternLoad`,
  five `orchestrator_tier3` arms, `help/connection-profiles.md` re-vendored,
  the commit's non-lib files ratified on P4.D199's list — v4's CLAUDE.md rule
  line NO-PORT, the README/package stamps NO-PORT with Tier R at the pin, the
  three docs mirrored under `docs/v4/`, the unit test → P4.D198's corpus).
  Round record: `status-log.md` → "Round record — the `bcd7e4852` bug-151
  drift catch-up + follow-ups round unification".

- **The `53294163f` GPT-Image-2.5 drift catch-up + maintenance round
  (2026-09-17, baseline `1fefadb9a` → `5f0a57dc4`):** `d8d2890ee`
  ABSORBED(P4.D196 — the OpenAI image capability table as one module
  [`model/openai_image_models.rs`, eight families, exact-then-longest-prefix],
  the OPENAI dialect rewritten over it red-first against the `image-dialects`
  corpus re-recorded at the target [97 → 150 rows; exactly the seven predicted
  pre-existing rows moved], the per-model options schema [`model/
  openai_image_options.rs`] through the existing `options-schema` action, v4
  bugs 148 + 149 in `generate_image.rs` [v5 measurably had both], the tool
  definition's bytes, the ONE quality list [`image_gen/quality.rs`] at the tool
  schema and the `?action=generate` arm, the eight-model manifest, the
  openai-SDK wire re-check [every other recorded corpus byte-identical] ∥
  P4.D197 — the offline fallback list in v4's CLIENT order, the modal header's
  recorded divergence, a recorded copy of v4's REAL `getOpenAIImageOptionsSchema`
  rendered in a 28-test spec + the live beat, the two `help/` pages
  byte-copied [124 stays 124]); `53294163f` and `5f0a57dc4`
  NO-PORT-RATIFIED(P4.D197 — both `--name-status` lists in the lane record;
  bug 150's client fix lands on the image-UPLOAD dialog v5 never ported, and
  v5's two in-chat generate dialogs post through the dispatch client, never a
  REST path, so the class is unreachable by construction). Round record:
  `status-log.md` → "Round record — the `53294163f` GPT-Image-2.5 drift
  catch-up + maintenance round unification".
- **The `1fefadb9a` bug-147 drift catch-up + maintenance round (2026-09-16,
  baseline `2075242f9` → `1fefadb9a`):** `1fefadb9a` ABSORBED(P4.D195 — the
  chat GET projects `spokenThisCycleParticipantIds` + `cycleOrderParticipantIds`
  as RAW JSON strings at v4's position with v4's `?? '[]'` shape, the P4.D171
  both-directions omission pin tripped at the target on all five `get_*`
  cases and retired, the corpus grown with the raw-`''` and raw-NULL arms;
  the SPA's dormant chat-GET seed made LIVE and grown its spoken half [the
  sidebar's `spoken` status was unreachable in v5 exactly as v4's filing says
  of v4], the P4.D187 refetch interplay and the busy-flip re-seed both pinned;
  the `state.cycleOrder` client read MEASURED as a convergence over a
  four-row table [v5 has read it since P4.D177]; v4's client/server agreement
  room as tier-1 corpus rows over the REAL `selectNextSpeaker` incl. the
  post-post half, which also opened and closed a blind spot — nothing had
  ever driven the rotation argument of `selectNextSpeaker` /
  `calculateTurnStateFromHistory` before; `help/chat-turn-manager.md`
  byte-copied, 124 stays 124; the non-lib files ratified NO-PORT on the
  `--name-status` list in the lane record). Round record: `status-log.md` →
  "Round record — the `1fefadb9a` bug-147 drift catch-up + maintenance round
  unification".
- **The `2075242f9` bug-145/146 drift catch-up + maintenance round
  (2026-09-16, baseline `ffb6b3119` → `2075242f9`):** `23abc1ba1`
  ABSORBED(P4.D192 — bug 145's migration hunk: `drop_victim_roll_link` over
  the three existing chokepoints, `gc_orphaned_file_row` widened to v4's
  per-table counts, the in-pass `protectedKept` census + `keptClause`, the
  family 17 → 24 scenarios with the album cases RED-FIRST [v5 measurably had
  the bug] ∥ P4.D194 — the bug-144 CONVERGENCE measured then retired
  [FOUR Tier R cases moved, not five; BOTH lines moved], the
  `help/database-protection.md` re-vendor, the non-lib files ratified
  NO-PORT on `--name-status` lists), `2075242f9` ABSORBED(P4.D193 —
  `resolveFloorSeatId` on both sides under the NEW tier-1
  `floor_seat_equivalence` [52 → 54 rows], the Salon banner re-keyed on the
  floor with v4's fourth sentence, a live two-user-seat beat ∥ P4.D194 —
  `help/chat-turn-manager.md`), `064ba85df` NO-PORT-RATIFIED(P4.D194 — bug
  143 filed, docs-only), `81e02f7a2` NO-PORT-RATIFIED(P4.D194 — bug 144
  filed, docs-only). Round record: `status-log.md` → "Round record — the
  `2075242f9` bug-145/146 drift catch-up + maintenance round unification".
- **The `ffb6b3119` bug-141 + bug-142 drift catch-up round (2026-09-15,
  baseline `31436bae4` → `ffb6b3119`):** `f90144ac4` ABSORBED(P4.D189 — the
  stream watchdog as a substrate commit [`model/stream_watchdog.rs` +
  `StreamError`'s `Stalled` kind with v4's message bytes], the TEN Salon-side
  wrap sites held by a per-file census [v4 has one funnel, v5 none], the
  `LLMStreamStalledError` → `network` classifier arm through all four classify
  sites with red-first `fallback_engine` rows + five `primary_stream_tier3`
  stall arms, the neutrality sweep at both pins ∥ P4.D190 — the greeting
  wrapped at 90 s / 60 s, the ladder's own-profile gate at v4's three
  positions with the desk scoped out, v4's three attempt warns + the
  exhaustion line the v5 arms never carried, `initial_greeting` +
  `chat_create_capstone` red-first over the ordered `stream_calls` comparand
  [the jest `instanceof`-across-`resetModules` trap found and fixed on both
  oracles] ∥ P4.D191 — the two `help/` files byte-copied, 124 stays 124);
  `ffb6b3119` ABSORBED(P4.D191 — the CONVERGENCE measured TOTAL [1 of 1,513
  cells moved], `DELETE_MISS_DIVERGENCE` retired to a plain equality
  red-first in both directions with zero v5 source change; the §3 review
  added a presence pin for the converged chat); `85813ddd2` and `364b04ac4`
  NO-PORT-RATIFIED(P4.D191 — file lists in the lane record; Tier R 223/0 at
  the pin; the main checkout's binding measured ABI-matched to Node 24).
  Round record: `status-log.md` → "Round record — the `ffb6b3119` bug-141 +
  bug-142 drift catch-up round unification".
- **The `31436bae4` drift catch-up round (2026-09-15, baseline `f4ad2c8d1`
  → `31436bae4`):** `5029075bb` ABSORBED(P4.D182 substrate — the
  `chats.transcriptVersion` boot ensure, the eight-file `help/**` re-vendor ∥
  P4.D183 server — the funnel's one announce point at v4's six conditions,
  the projection extraction proven neutral, the `chatTranscript` verb + the
  `/api/v1/messages` GET edge, the chat-GET key; a v4 bug found and pinned
  both directions, since filed and fixed as bug 142 ∥ P4.D187 SPA —
  `reconcileTranscript` over a recorded corpus, the subscribed read, the
  bubble inside the array [dogfood #106's ground]); `7fbf8a55b`
  ABSORBED(P4.D182 substrate — `files.generationKey` through the D23 re-dump +
  the column/index ensure, the carry through every read/write/export/import
  surface, the export-schema re-vendor ∥ P4.D184 server — the key-derivation +
  lookup chokepoint over a new tier-1 corpus, lookup-before-spend, `force`,
  vault-always, the collapse heal under v4's ledger id over a new tier-2
  family; the §3 review landed the five unpinned log lines); `4dcbe0d21`
  ABSORBED(P4.D185 server — the album predicate, the rolls service, three
  verbs + two REST sub-routes, a new committed `avatar-rolls-*` pair; the §3
  review caught a swap-remove reaching `chats.characterAvatars` ∥ P4.D188 SPA
  — the Avatar Rolls section in the gallery tab); `055cac45a`
  ABSORBED(P4.D188 — the "Show shared" tickbox); `8275b3642`
  ABSORBED(P4.D187 — bug 136's two sentences at v5's measured sites; bug 135
  NO-COUNTERPART, measured); `31436bae4` ABSORBED(P4.D186 server — the
  `paused_hold` predicate consulted once at the record → prepare-turn seam,
  the three `!hold` conjuncts each pinned with the others open,
  `finish_held_user_turn`, `heldUserTurn` on the frame ∥ P4.D187 client — the
  unpause-first legs deleted, the once-per-pause toast, bug 139's resume-then-
  ask, bugs 138/140 measured); `4dc48283d` and `aecf9de0b`
  NO-PORT-RATIFIED(P4.D182 — two `docs/` files; four version markers; the
  evidence in the lane record). Round record: `status-log.md` → "Round record
  — the `31436bae4` drift catch-up round unification".
- **The `f4ad2c8d1` In-Their-Own-Words drift catch-up round (2026-09-11,
  baseline `cc65d6bfc` → `f4ad2c8d1`):** `686954937` ABSORBED(P4.D179 the
  `chat_settings."impersonationVoiceRewrite"` column through the D23 re-dump
  + a boot ensure + the route arm, the `VOICE_REWRITE` log type + both
  `mapTaskTypeToLogType` arms — v5 had filed `announcement-rewrite` as
  `SUMMARIZATION` since it was ported — the Almanack row, `help/**` 123 →
  124 ∥ P4.D180 the `voice_rewrite_core` extraction proven neutral at both
  pins, the new `in_scene_voiced` service, the `chatImpersonationVoicePreview`
  verb, the LIVE host wire, a new committed pair + 27-case tier-3 family ∥
  P4.D181 the whole SPA half); `f4ad2c8d1` ABSORBED(P4.D181 — bug 134
  measured then ported: v5 never had the mount-only snapshot, the live-read
  facts pinned structurally, the memory-cascade remember arm + its
  invalidation landed, the `types.ts` consolidation NO-COUNTERPART; its
  `help/settings.md` hunk rode P4.D179's re-vendor). Round record:
  `status-log.md` → "Round record — the `f4ad2c8d1` In-Their-Own-Words drift
  catch-up round unification".
- **The `cc65d6bfc` bug-133 catch-up + `78b381a96`-round remainders round
  (2026-09-10, baseline `78b381a96` → `cc65d6bfc`):** `cc65d6bfc`
  ABSORBED(P4.D178 — bug 133 whole: the sanitizer's fourth parameter
  re-meant as "does THIS scene route uncensored", the story reroute barred
  for a moderated chat with the candid re-craft and v5's `RerouteRecraft`
  seam deleted, the six reroute-path log lines both handlers were missing,
  the story corpus's two moderated reroute rows red-first + two new arms, the
  NEW `appearance_sanitize_gate_tier3_equivalence` family over v4's real
  sanitizer, `image_generation_tier3` widened with a DETECT_ONLY case,
  `help/dangerous-content.md` re-vendored). Round record: `status-log.md` →
  "Round record — the `cc65d6bfc` bug-133 catch-up + `78b381a96`-round
  remainders round unification".
- **The `78b381a96` twelve-commit drift catch-up round (2026-09-10, baseline
  `25f534c0b` → `78b381a96`):** `5841a8c62` ABSORBED(P4.D171 substrate →
  P4.D173 server ∥ P4.D177 SPA — the message route trail: the column through
  every surface, the ONE recording chokepoint + twelve record sites + the
  three empty-response arms v5 lacked, persistence + the `done` frame, the
  64-row compose family, the badge under the avatar); `2aca73ad6` +
  `d14da3a56` ABSORBED(P4.D171 substrate → P4.D172 server ∥ P4.D177 SPA — the
  cycle's drawn rotation as an ORDERED draw source, the six selection sites
  over the whole-room batched map [bug 131 — v5 measurably had it], the
  strike, `?action=turn`'s `state.cycleOrder`, the participants-list
  rotation; the §3 review fixed the finalizer's `{id,name}` preloaded stub);
  `86d59660c` ABSORBED(P4.D174 server ∥ P4.D176 SPA — the Salon chat gallery
  whole, `?download=1`, bugs 129/130; the §3 review flattened the 409's
  riders); `4a9be9878` ABSORBED(P4.D175 server ∥ P4.D177 client — bug 128's
  `memories` topic; bug 127 a convergence record); `78b381a96`
  ABSORBED(P4.D175 — bug 132's writers, the prompt-first ladder, the boot
  heal + ledger row, `help/**` at 123); `07eee4f4c`, `9fc664c94`,
  `5fb6bedd6`, `df1a075e8`, `c0f9232af`, `d3f0ed133` NO-PORT-RATIFIED(P4.D175
  — docs/version-only, with the evidence in the lane record; the twelve
  retired specs MOVED in the `docs/v4/` mirror at unification). Round
  record: `status-log.md` → "Round record — the `78b381a96` twelve-commit
  drift catch-up round unification". The mid-round `cc65d6bfc` (bug 133)
  stays in §3 UNPROCESSED.
- **The `25f534c0b` progressions + bug-126 drift catch-up round (2026-09-09,
  baseline `2f4254b42` → `25f534c0b`):** `0587d1e96` ABSORBED(p4.d167 +
  p4.d168 + p4.d169 + p4.d170 — the whole character-progressions feature:
  the pure engine tier-1 exact over a 555-row committed corpus [the U+202F
  prediction refuted, `toFixed` half-up pinned, the abort-suppresses-refine
  rule measured and fixed at unification]; the prompt path — the chokepoint,
  `find_last_own_turn_ms`, the ONE memoised cadence read, the trailing section
  after Suparṇā's mail and before the turn-skip note, the forced greeting and
  Carina reports, the negative cache guarantee, the seven `help/` files
  re-vendored [121 → 122]; the Pascal `progress` family end to end — read
  subject, `{{now}}`, the effect target with create-on-write / normalise /
  post-validation rollback, the vocabulary's three keys, one clock per run,
  the NEW `pascal_side_effects_equivalence` family, six corpora widened from
  zero, the committed `pascal-run-custom-*` pair rebuilt [`shift_remove` at
  three applier sites landed at unification — key order reaches disk]; the
  SPA half whole — the client-safe twins over an extracted Zod shim, the
  Progressions card + editor modal, the Workbench affordances, the run popup,
  both `public/schemas/` vendors GUARDED, both gated beats flipped LIVE);
  `25f534c0b` ABSORBED(p4.d166 — bug 126: the ownership snapshot [PID +
  `startedAt`, keyed by lock path], the heartbeat-freshness cascade for every
  environment, the renamed-process release, the loss teardown made ordered,
  the CLI's shared `assess_lock` with Tier R 216 → 223/0; `lock-helpers.js`
  UNTOUCHED by v4, so the write lock and the launcher's classifier keep the
  hostname comparison); `d307a4164` NO-PORT-RATIFIED(p4.d167 — the design of
  record, mirrored to `docs/v4/developer/features/character-progressions.md`
  at unification) and `4097626c6` NO-PORT-RATIFIED(p4.d166 — version bump,
  four files, no comparand). Round record: `status-log.md` → "Round record —
  the `25f534c0b` progressions + bug-126 drift catch-up round unification".
- **The `2f4254b42` character-subprompts round (2026-09-07, baseline
  `f699da6f6` → `2f4254b42`):** `2f4254b42` ABSORBED(p4.d163 + p4.d164 +
  p4.d165 — the whole feature: the participant `selectedSubpromptIds` carry
  [a measured v5 data-loss fix landed first], the vault-backed `subprompts`
  module + fan-out, the five verbs + REST edges + realtime, the four `help/`
  files re-vendored, the `## Additional Instructions` block in the identity
  stack with NO builder-version bump, the compiler bake, the `build_context`
  fallback, the greeting, the green room at both entrances, and the whole SPA
  half with its walk live); `15573c3a1` ABSORBED(p4.9k1-resumed — bug 119's
  runner half: `run_sub_step` / `run_sub_step_core` containment + both log
  lines, capture-pinned). Round record: `status-log.md` → "Round record — the
  `2f4254b42` character-subprompts round unification".
- **The `f699da6f6` 4.9.x drift catch-up round (2026-09-06, baseline
  `c2232cd9a` → `f699da6f6`):** `fef7ce4f7` ABSORBED(p4.d160 + p4.d161 — bug
  123: the per-emit optional `paused` chain-complete key, the paused early-
  return with v4's info line, the two re-vendored help pages; the SPA's
  seat-keyed Skip banner, overlay-aware Skip with a silent unpause-first, the
  pause-you-did-not-cause toasts, v4's pause-sync drift a mutation-proven
  NO-COUNTERPART), `20913d2aa` ABSORBED(p4.d162 — bugs 124/125, on `main` by
  content through the 4.9.2 squash: the help loop through the tool-call
  threading primitive with the family's FULL-slate comparand and an id-less
  case; `additionalProperties` at the head of Google's strip list with the
  real wardrobe schemas in the recorded corpus; a GOOGLE seat in the help-chat
  fixture), `d40497411` / `5eaf98cf1` / `ba34fa367` / `02b77ab0f` /
  `8fbf2afe0` / `d489b04a3` / `f699da6f6` / `1a2b2164c` NO-PORT-RATIFIED(p4.d160
  — the two release cycles' branch starts, squashes, merge-backs, the bug-filing
  docs commit and the CHANGELOG → `CHANGELOG_V4.md` move; file lists in the
  P4.D160 lane record's "NO-PORT ratification evidence"; `docs/v4/` refreshed
  to byte-identity with `f699da6f6:docs/` on every shared path). Round record:
  `status-log.md` → "Round record — the `f699da6f6` 4.9.x drift catch-up round
  unification". `15573c3a1` (bug 119) stays in §3 for the unported `p4.9k`.
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
