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

- **Oracle baseline: `2f4254b42`** — "feat: character subprompts — per-chat
  optional instructions from the vault" (v4 main, 2026-09-07 07:43 -0500,
  `4.10.0-dev.5`), adopted at the `2f4254b42` character-subprompts round
  unification (2026-09-07). CLAUDE.md's Status bullet agrees. **The
  `25f534c0b` round's unification is IN PROGRESS and moves the baseline to
  `25f534c0b` when it lands** — this check was run by that unification's own
  §2 probe failing.
- **Checked:** 2026-09-09, at the start of `/unify` for the `25f534c0b`
  progressions + bug-126 round (P4.D166 ∥ P4.D167 → {P4.D168 ∥ P4.D169} ∥
  P4.D170). Every lane had already finished; two lanes (P4.D168 at
  `07eee4f4c`, P4.D169 at `c0f9232af`) saw the probe fail MID-LANE, STOPped
  and reported as the round's rules require, and resumed under their
  `25f534c0b` pins — so no unabsorbed drift reached any oracle.
- **v4 `main` HEAD at check:** `d3f0ed133` (2026-09-09 00:05 -0500,
  `4.10.0-dev.18`) — **THIRTEEN commits past the baseline: the round's four
  (ORDERED, being unified) plus NINE that arrived during the round.**
- **v4 `bugfix` tip at check:** `1a2b2164c` — UNMOVED. Its content delta
  against `main` is still the subprompts commit reversed plus everything
  below (it is simply behind); nothing unabsorbed lives on it.
- **v4 `release` tip at check:** `8fbf2afe0` ("release: 4.9.2") — UNMOVED.
- **Checkout at check:** branch **`main`**, tree CLEAN.
- **Verdict: DRIFT PENDING — 13 commits** (4 `ORDERED`, 9 `UNPROCESSED`).
  The nine new rows are three shipped features and six riders: **two
  PORT-NEW features** (`5841a8c62` the message route trail — **with a
  SCHEMA MOVE**, the first `chat_messages` column since bug 68's
  `multiCharacterPrefill` moved `chat_participants`; `86d59660c` the Salon
  chat gallery, 4,306 insertions, plus bugs 129/130), **one PORT +
  CONVERGENCE pair** (`4a9be9878` — bug 128's `memories` realtime topic is
  a PORT onto the P4.D123–D125 realtime subsystem; bug 127 is v4 ADOPTING
  P4.D170's rendering of the progressions card's unparseable-ids line), and
  six NO-PORT? riders (four docs/plans/filings, the twelve-spec
  `features/complete/` retirement whose 29 source hunks are all
  comment-path rewrites, and a version bump).
- **⚠ `5841a8c62` MOVES THE SCHEMA — a D23 re-dump is owed.** The
  `routeTrail` column on `chat_messages` arrives BOTH as a migration
  (`migrations/scripts/add-route-trail-message-column-v1.ts`, `TEXT DEFAULT
  NULL`, `introducedInVersion: '4.10.0'`) AND in the repository Zod shape
  (`lib/database/repositories/chats-messages.ops.ts:182`,
  `routeTrail: z.array(...)`) that `generateDDL` (`lib/database/
  schema-translator.ts`) reads — so `fresh_schema.json` MOVES, and the
  P4.D78 (bug 68) precedent applies: the generateDDL shape and the migration
  shape may DISAGREE, both are carried (the re-dump for fresh instances, a
  boot ensure for existing ones). No cache-version bump anywhere in the
  nine.
- **Vendored-artifact obligations the nine create (re-read hazards 4, 9,
  10 below):**
  - **`help/**` — v4 is at 123 files, v5's re-vendored tree (P4.D168, at
    `25f534c0b`) at 122.** `86d59660c` adds `help/chat-gallery.md` and
    edits `chat-message-actions.md`, `chat-participants.md`,
    `photo-gallery.md`; `5841a8c62` edits `chats.md`,
    `connection-profiles.md`, `dangerous-content.md`; `4a9be9878` edits
    `character-progressions.md` and `chat-participants.md`.
    `help_tree_equivalence` goes RED against any oracle regenerated past
    `25f534c0b`, by design.
  - **`public/schemas/qtap-export.schema.json` moved by +19 lines**
    (`5841a8c62`, the `routeTrail` message field). **`qtap_schema_embed_guard`
    is RED against the live checkout** (measured by P4.D169: 92,797 bytes at
    HEAD against the vendored 89,769; the vendored copy is byte-identical to
    `25f534c0b`, md5 `430bb227c7029550f773134f42a0c6eb`). The re-vendor
    rides the route-trail port — vendoring a schema field for a column v5
    does not yet carry would be wrong — so **until that round lands, every
    full workspace gate needs `QT_V4_ROOT=<a 25f534c0b pin>`** or the
    fail-fast run loses its tail to this tripwire (the P4.D169 record names
    the shape). The two SPA-served schemas (`qtap-custom-tool`,
    `qtap-progression`) are UNMOVED past `25f534c0b`; P4.D170's new
    `public_schemas_vendor_guard` stays green.
  - Unmoved: the 21-file sample-prompt source directory, and v4's installed
    `zod` at `4.5.4` (every `package-lock.json` hunk across the nine is the
    lockfile's two identity-version lines — `4.10.0-dev.9` → `-dev.18`).
- **CONVERGENCE row: bug 127 (`4a9be9878`).** Filed by v4 (`07eee4f4c`)
  from THIS PORT's P4.D170 lane finding it while transcribing
  `ProgressionsSection.tsx` — v4's `invalidIds.join('</code>, <code>')`
  inside a JSX expression is escaped by React. v4 adopted v5's
  map-with-separator shape. **No oracle compares the line**, so no pin
  trips at any baseline move; the retirement site is the divergence note in
  `apps/web/src/app/progressions/progressions-section.spec.ts` (P4.D170),
  which the absorbing lane rewrites as a convergence record. Bugs 128, 129
  and 130 were filed by v4 from its own code map — NOT convergences.
- **Regen rule in force: PIN REQUIRED at `25f534c0b`** — the round's
  catch-up target, and the baseline once this unification lands (v4 `main`
  HEAD is past both). Every oracle regen and fixture build runs from a
  detached worktree pinned at `25f534c0b` per §5.1 until a later round
  absorbs the nine.
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
  (4) **`help/**` is a VENDORED v5 ARTIFACT since P4.9I2A** — any v4 commit
  touching `help/**` is a re-vendor obligation and `help_tree_equivalence`
  goes RED the moment an oracle is regenerated past it, by design; the
  vendored COUNT is a literal in several crates (`a-vendored-count-is-hard-
  coded-in-several-crates`; P4.D168 derived one of the four); (5) **since
  `8fbf2afe0`, v4's `jest.config.ts` maps `'^@google/genai$'` →
  `__mocks__/@google/genai.ts`** (a manual mock; the SDK is ESM-only).
  Fourteen committed jest-run oracle cases load the plugin tree
  (`orchestrator-tier3`, `help-chat-orchestrator-tier3`,
  `brahma-orchestrator-tier3`, `brahma-console-tier3`, `enclave-step-tier3`,
  `embedding-provider-tier3`, `avatar-job`, `danger-routing`,
  `danger-gatekeeper`, `image-gen-leaves`, `image-profiles-routes`,
  `image-generation`, `image-generate-route`, `settings-routes`); the mock is
  never reached by the four proven at the last round's pin (those oracles mock
  `streamMessage` above the plugin layer); the `tsx`/`node` recorders are
  unaffected; (6) **the CLI-package version nit is CLOSED on `main`** — at
  `d3f0ed133` `package.json` and `packages/quilltap/package.json` both read
  `4.10.0-dev.18`; the lag survives only on `bugfix`. Still no `--version`
  comparand, Tier R unaffected; re-watch it after the next merge-back; (7)
  **the generator runners (`character_optimizer_tier3`,
  `character_wizard_tier3`, `ai_import_tier3`, `external_prompt_tier3`)
  drive v4's REAL runners with a canned `createLLMProvider`** — a v4 change
  to the provider factory's signature breaks their oracles at LINK time, not
  at diff time; (8) **the sample-prompt catalogue is a VENDORED v5 ARTIFACT
  since P4.83** (`crates/quilltap-core/src/services/
  builtin_prompt_templates.json`, the 21 `.md` files under
  `plugins/dist/qtap-plugin-default-system-prompts/prompts/` at
  `2f4254b42`) — any v4 commit touching that directory is a re-vendor
  obligation, and `builtin_prompt_templates_guard` goes RED against the
  checkout the moment one lands, by design; (9) **`public/schemas/
  qtap-export.schema.json` is a VENDORED v5 ARTIFACT since P4.86**
  (`crates/quilltap-core/src/generators/qtap-export.schema.json`, 89,769
  bytes at `25f534c0b`) — a v4 commit touching it is a re-vendor obligation
  and `qtap_schema_embed_guard` goes RED against the checkout, by design
  (**it IS red now — see above**); (10) **`public/schemas/
  qtap-custom-tool.schema.json` and `qtap-progression.schema.json` are the
  fifth and sixth vendored artifacts, served by the SPA from
  `apps/web/public/schemas/` and GUARDED since P4.D170** by
  `crates/quilltap-harness/tests/public_schemas_vendor_guard.rs` (byte
  equality against `QT_V4_ROOT`, plus a checkout-free self-consistency
  half) — the standing hazard recorded 2026-09-08 (the custom-tool schema
  sat 529 lines behind with nothing red) is DISCHARGED; (11) **the progress
  corpora freeze v4's `Date.now()` per case** (P4.D169) — the discovery,
  workbench-route and both run-tool oracles carry a `nowMs` the Rust side
  reads OFF THE ROW; a v4 change that adds a clock read at a new site
  (roster, bench, run) shows as a per-case drift, not a recipe failure.
- **Release shape:** v4 develops on `main` at 4.10.0-dev with a live 4.9.x
  `bugfix` fork; the fork → fix → `release: X` squash → merge-back cycle has
  run twice. `bugfix` is currently idle at its branch-start commit while main
  takes feature work — three merged PRs (#58, #59, #60) landed on `main` in
  one evening. §4 step 2's two-branch rule stays load-bearing — measure
  `bugfix` by CONTENT, never its commit list, and remember a content diff
  can be non-empty simply because `bugfix` is behind.
- _Superseded (2026-09-08 afternoon): DRIFT PENDING — 4 commits, all
  ORDERED, PIN REQUIRED at `2f4254b42`. Before that (2026-09-08 midday):
  DRIFT PENDING — 3 commits (the same minus bug 126). Before that
  (2026-09-08 morning): CLEAR._

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
| `d307a4164` | 2026-09-08 | docs: plan for character progressions (timed properties reported per turn) | NO-PORT? | Docs only — `docs/developer/features/character-progressions.md` (375 lines), the changelog, and one v4-side `.claude/` command doc. No shipped code. **Not a throwaway row: this file is the design of record for `0587d1e96`** and should be read at ordering time — it carries the phasing, the cadence grammar's rationale, and the "prompt path performs no writes" constraint. Mirror candidate for `docs/v4/` at the next baseline move. | ORDERED(P4.D167 — NO-PORT ratification + the `docs/v4/developer/features/character-progressions.md` mirror as a unifier wire) |
| `0587d1e96` | 2026-09-08 | Add character progressions — timed conditions reported per turn (#57) | PORT-NEW | **The feature, ~6,700 insertions / 55 files, five phases squashed into one PR.** New v4 modules `lib/progressions/{schema,engine,prompt-section}.ts` — pure, client-safe, injected clock — have NO v5 counterpart (tier-1 territory: `deriveProgression`, `formatSpan`, `shouldReportProgression`, `renderProgressionReport`, `flattenProgressions`, `inferIncrement`, and a fail-soft `parseProgressions` that drops one bad entry and keeps its siblings). Ported v5 surfaces it reaches: **`build_context`** (`lib/chat/context-manager.ts` — the section is wired after Suparna mail, before the turn-skip note, skipped in continue mode; last touched by P4.D95 / P4.D103 / P4.D163), **`core_whisper.rs`** (`findLastOwnTurnMs` joins `shouldFireCoreWhisper` in `core-whisper-trigger.ts`, and the two cadences now share ONE memoised `getMessages` per turn — a read-count change on a ported path), the **greeting builder** (`lib/chat/initialize.ts`) and **Carina** (both forced, cadence bypassed), the **whole Pascal family** (`custom_tool_types` / `custom_tools` / `placeholders` / `side_effects` / `tool_gate` / `tool_vocabulary` in `quilltap-core::pascal`, plus the SPA's `pascal/tool-draft.ts` twin — ported across P4.D35/P4.D36, the Workbench lanes P4.6BB/P4.D20, P4.D43, and the archived custom-tools-end-to-end round), the **tool subsystem** (`run-custom-tool.ts`, `run-custom-handler.ts`), the **character vault types** (`lib/schemas/character.types.ts` → the store-backed entity slice), **two REST edges** in `quilltap-web` (`/api/v1/custom-tools`, `/api/v1/chats/[id]/custom-tools`), and the **SPA** (a new `components/characters/progressions/` trio — editor modal, section, hook — plus the system-prompts-editor host and four `components/custom-tools/` panels + `CustomToolRunDialog`). Also a `help/**` re-vendor (121 → 122) and two `public/schemas/` files. **No table-schema change and no cache-version bump** (see §1). | ORDERED(P4.D167 → {P4.D168 ∥ P4.D169} ∥ P4.D170) |
| `4097626c6` | 2026-09-08 | doc: Update version | NO-PORT? | `4.10.0-dev.5` → `4.10.0-dev.8` in the README badge, `package.json`, `packages/quilltap/package.json` and the lock's two version lines. No ported comparand (no `--version` assertion in Tier R); the two version fields still agree on `main`, so standing hazard (6) stays closed there. | ORDERED(P4.D166 — NO-PORT ratification, file-list evidence) |
| `25f534c0b` | 2026-09-08 | fix: a hostname change no longer makes the app kill its own database (bug 126) | PORT | **v5 measurably HAS this, critically — see §1.** v4's fix, arm for arm: ownership becomes a snapshot written with the lock (**PID + `startedAt`**, compared by `isStillOurLock`), hostname demoted to a label refreshed each tick; acquisition stops claiming a lock merely because the recorded hostname differs, deciding liveness by **heartbeat freshness for every environment** rather than only for Docker (this closes a fail-OPEN case where two processes on one machine could both open the same DB); a renamed process releases its own lock instead of orphaning it; a lock taken by manual override starts a heartbeat; and the lock-loss teardown is registered **inward** by `client.ts` (v4's dynamic `require('./client')` did not resolve in the bundled standalone server — it threw and left the WAL unmerged). v5 surfaces: `quilltap-host/src/lock.rs` (`heartbeat_tick` `:527`, `release_instance_lock` `:543`, the acquire cascade `:425`/`:454`, `classify_lock_status` `:308`) and its caller `host.rs:1584`, all from the P4.D68 → P4.D75 lock work; the CLI's `db --lock-status` / `--lock-clean` (v4 `packages/quilltap/bin/quilltap.js`, 103 lines, gains a shared `assessLock` — **Tier R comparands move**). Porting notes the hunks make explicit: v4 **inverted two existing tests that had been asserting the buggy behavior**, and its suite mocked `os.hostname()` as a constant, which is why it never surfaced in test — v5 has no hostname seam at all, so the port needs one (or, following v4, no hostname comparison left to seam). Also a `help/database-protection.md` re-vendor. v4's own bugs.md guidance: "Carry the invariant, not the code: a hostname is a label, not an identity." | ORDERED(P4.D166) |
| `07eee4f4c` | 2026-09-08 | docs: file bug 127 — the progressions card escapes its own markup | NO-PORT? | Docs only (`bugs.md` + `bugs/bug-127-…md`). The filing of a defect THIS PORT found: P4.D170's transcription of `ProgressionsSection.tsx` measured v4's `invalidIds.join('</code>, <code>')` inside a JSX expression rendering raw markup for two or more unreadable ids, and recorded v5's map-with-separator rendering as a divergence (`progressions-section.spec.ts`). The fix is `4a9be9878` below. | UNPROCESSED |
| `9fc664c94` | 2026-09-08 | docs: plan for the message route trail — every model tried, in order, under the avatar | NO-PORT? | Docs only — `docs/developer/features/message-route-trail.md` (286 lines; MOVED to `features/complete/` with an "As built" section by `5841a8c62`), the changelog, the docs index. **The design of record for `5841a8c62`** — read at ordering time. Mirror candidate for `docs/v4/` at the next baseline move. | UNPROCESSED |
| `5fb6bedd6` | 2026-09-08 | docs: file bug 128 — the Salon's memory count goes stale and disarms its own delete button | NO-PORT? | Docs only (`bugs/bug-128-stale-chat-memory-count.md`, 231 lines — the plan `4a9be9878` carries out, incl. WHY `memories` must stay out of `REPOSITORY_TOPICS`). Filed by v4 from its own live Friday instance (chat `27961b14`: 59 rows, `(0)` on a long-lived tab) — NOT a convergence. | UNPROCESSED |
| `df1a075e8` | 2026-09-08 | docs: plan for the Salon chat gallery (every image in a conversation, savable and downloadable) | NO-PORT? | Docs only — `docs/developer/features/salon-chat-gallery.md` (224 lines; MOVED to `features/complete/` by `86d59660c`), the index, the changelog, a bug-128 cross-note. **The design of record for `86d59660c`**, and the commit that FIRST FILED bugs 129 and 130 (the never-rendered Gallery button; the images route ignoring `chatId`). Mirror candidate. | UNPROCESSED |
| `c0f9232af` | 2026-09-08 | docs: retire twelve shipped feature specs to features/complete/ | NO-PORT? | 56 files, but every non-doc hunk is a comment-only `See docs/…` path rewrite (29 source files, one line each: `app/**`, `components/**`, `lib/services/home-data.service.ts`, `lib/workspace/types.ts`, `migrations/scripts/quantize-embeddings.ts`, one plugin test) plus the lockfile's two version lines — VERIFIED by grepping the non-doc hunks for any non-comment line (only the two version lines remain). **Fact the round wants: `character-progressions.md`, the design of record P4.D167 ratified, now lives at `docs/developer/features/complete/character-progressions.md`** — the unifier's `docs/v4/` mirror wire should mirror it at the path the NEW baseline (`25f534c0b`) has, `docs/v4/developer/features/character-progressions.md`, since the mirror is a snapshot at the baseline. Ratify with the file list; nothing to port. | UNPROCESSED |
| `4a9be9878` | 2026-09-08 | Fix bugs 127–128: stale memory count and escaped markup (#58) | PORT (bug 128) + CONVERGENCE (bug 127) | **Bug 128 — PORT onto the P4.D123–D125 realtime subsystem.** v4 adds a `memories` realtime topic: declared in `lib/schemas/realtime.types.ts` (`REALTIME_TOPICS` + `ALL_REALTIME_PREFIXES`), keyed `queryKeys.memories.chatCount(chatId)` in `lib/query/keys.ts`, mapped in `lib/realtime/topic-map.ts` (`queryKeysForTopic`; `TOPIC_ID_FIELDS.memories = []` — the emptiness is load-bearing, and `memories` is deliberately NOT in `REPOSITORY_TOPICS` because `firstIdArg` would publish a memory id under a chat-scoped topic), published from `lib/realtime/job-topics.ts`'s `topicsForCompletedJob` for the four chat-scoped memory job types (each reading `chatId` off its payload) + collection-wide for `MEMORY_HOUSEKEEPING`, and from ONE parent-side delete chokepoint (`lib/memory/memory-gate.ts` — `deleteMemoryWithUnlink` / `deleteMemoriesWithUnlinkBatch`, collection-wide; the route's own publish was REMOVED in the PR's second commit as redundant-and-wrong, since the gate stays silent for a chat with nothing to delete). Client: `useChatData.ts` subscribes `useRealtimeTopic('memories', chatId)` three lines from its fetcher (`cache: 'no-store'` added); `useMemoryActions.ts` disables the button at zero (`.qt-tool-palette-button:disabled` already styled), replaces the bare `return` with a toast, and re-reads the count from the server before the confirmation; `ChatSidebar.tsx` (+7). v5 surfaces: `crates/quilltap-core/src/realtime/{types.rs,job_topics.rs,publish_sites.rs}` + the 73-case `realtime_topics_equivalence` tier-1 family (P4.D123) — the topic ENUM, the prefix list and the job→topic map are all comparands there and MOVE; `crates/quilltap-core/src/db/memories.rs::{delete_with_unlink,delete_many_with_unlink}` (the gate twin's publish site); the SPA's `core/realtime-topic-map.ts` + `core/realtime.types.ts` (P4.D125), `chat/sidebar/edit-section.ts` (**Delete Memories (n)** — MEASURE whether v5 reads the count once at mount and gates the click on a stale zero before calling it inherited; the bugs.md v5 column says "Not yet assessed"), and `chat/memory-cascade-dialog.ts`. Also two `help/**` edits (`character-progressions.md` +6/−?, `chat-participants.md` +11) — re-vendor obligation. **Bug 127 — CONVERGENCE onto P4.D170:** `ProgressionsSection.tsx` now renders `invalidIds.map((id, i) => <span key={id}>{i > 0 && ', '}<code>{id}</code></span>)`, v5's shape; v4's two new pins in `progressions.test.tsx` (the two-id case asserts no `</code>` in the text + a `<code>` per id; the one-id sentence unchanged). Retire P4.D170's divergence note in `apps/web/src/app/progressions/progressions-section.spec.ts` to a convergence record; no oracle compares the line, nothing trips. | UNPROCESSED |
| `5841a8c62` | 2026-09-08 | Message route trail: every model tried, in order, under avatar (#59) | PORT-NEW (with a SCHEMA MOVE) | **The feature, 2,112 insertions / 45 files, incl. a NEW `chat_messages.routeTrail` column (D23 re-dump + boot ensure — see §1).** Storage: nullable JSON `routeTrail` (`migrations/scripts/add-route-trail-message-column-v1.ts` + `migrations/scripts/index.ts`; the repository Zod shape in `lib/database/repositories/chats-messages.ops.ts` +26 — the source `generateDDL` reads; `docs/developer/DDL.md`; `lib/startup/prettify.ts` +1); each entry `{profileId, profileName, provider, modelName, via: primary|retry|concierge|understudy|tier-pick, outcome: answered|failed|refused, trigger?, evidence: finish-reason|inferred, detail? (≤200 chars)}`; NULL whenever nothing failed (nearly every message), no backfill; the last entry always agrees with `provider`/`modelName` (asserted by test). Recording: ONE chokepoint `lib/services/chat-message/route-trail.ts` (198 lines, NEW) is the only writer of two new `StreamingState` fields (`routeFailures`, `routeVia` — `lib/services/chat-message/types.ts` +14); `provider-failover.service.ts` (+66) calls it at every site that already logs a failure (the hard-error opener, the chain walk's key-less / thrown / empty candidates, the empty-response opener, the same-profile retry, the Concierge's uncensored reroute — recorded from `routeResult.connectionProfile`; every empty-body classification runs BEFORE `resetStreamingBuffersForSwap` clears `rawResponse`; the pre-call Concierge reroute gets no row but seeds `routeVia = 'concierge'`); `FallbackChainResult.attempts` / `summarizeFallbackAttempts` UNTOUCHED. Persistence: `message-finalizer.service.ts` (+18), `orchestrator.service.ts` (+7), `primary-stream.service.ts` (+7), `streaming.service.ts` (+5). Transport: the SSE `done` event carries it; the chat-GET projection lists it (`app/api/v1/chats/[id]/handlers/get.ts` +4); `.qtap` export carries it with the message (`public/schemas/qtap-export.schema.json` +19 — hazard (9), the re-vendor rides this port; an imported `profileId` is deliberately NOT remapped); `apply-chat-continuation.ts` does NOT copy it. Display: `lib/chat/route-trail-display.ts` (144 lines, pure — collapse adjacent same-profile rows, the ❌/🚫 marks, hover text; `lib/chat/__tests__/route-trail-display.test.ts` 139 lines is a tier-1 seed) + `components/ui/RouteTrailBadge.tsx` (replaces `ProviderModelBadge` when a trail is present; theme hook `[aria-label="Models tried for this reply"]`, no new qt-* class); `MessageRow.tsx` (memo comparator gained an O(1) identity check on `routeTrail`, normalised undefined≡null), `MessageDesktopAvatar.tsx`, `useSSEStreaming.ts` (+9), `app/salon/[id]/types.ts`; a compile-time + runtime parity assertion tying the client-safe trigger enum to the engine's `FallbackTrigger`. Help: `chats.md` (+35), `connection-profiles.md` (+7), `dangerous-content.md` (+8) — re-vendor. Twelve v4 test files as seeds (the finalizer's 160-line and the failover chain's 150-line suites carry the persistence pins). v5 surfaces: `services/provider_failover.rs` + `services/message_finalizer.rs` + `services/primary_stream.rs` + the streaming state (P4.D135 / P4.68 / P4.72 — the failover `llm_logs` rows and the `auth` chain arm landed there), the SSE `done`/chain-complete event on the `Event` channel (P4.D160's `paused` key precedent), `api/salon.rs`'s chat-GET five-field projection (P4.D51/P4.D60), `generators/**` + the import remap (P4.D46/P4.D87 — the deliberate NON-remap needs its own pin), `chat_continuation` (Continue Elsewhere), and the SPA's provider/model badge under the avatar. `IDENTITY_STACK_BUILDER_VERSION` / `PROMPT_CACHE_STRUCTURE_VERSION` untouched. The commit message's "manual V4test pass … still outstanding" is v4's own owed proof. | UNPROCESSED |
| `86d59660c` | 2026-09-09 | Add the Salon chat gallery — every image in a conversation (#60) | PORT-NEW + PORT (bugs 129, 130) | **The feature, 4,306 insertions / 51 files — all six phases of `salon-chat-gallery.md`.** ONE server-side enumerator `lib/photos/chat-gallery.ts` (962 lines, NEW — nine sources: uploads + library links, `generate_image` output, both Generate Image entry points, `attach_image` re-shows, Librarian attaches, Lantern story backgrounds incl. superseded, Aurora avatar repaints likewise, the cast's standing portraits, Markdown-referenced images in message prose; over BOTH id species — `files.id` and `doc_mount_file_links.id`; deduped by content hash, newest first, portraits last; each entry carries source, species, is-current-background / is-worn-avatar, deletability; `__tests__/unit/lib/photos/chat-gallery.test.ts` 620 lines is the seed) — NO v5 counterpart; `GET /api/v1/chats/[id]?action=gallery` (`handlers/get.ts` +25 — entries + per-source counts + total); `POST /api/v1/chats/[id]?action=save-image` (`actions/save-image.ts` 137 lines NEW, `actions/index.ts`, `handlers/post.ts`) — the chat-scoped twin of the message-scoped save, guarded by GALLERY MEMBERSHIP rather than message attachment, sharing one Zod body schema, one attribution resolver (`lib/photos/save-attribution.ts` 93 lines NEW) and `lib/photos/save-image-to-album.ts` (+18) with `messages/[messageId]/route.ts` (−…); the `/chats/[id]/files` listing (`files/route.ts` 85 lines moved) now SHARES the enumerator's message-attachment walk instead of a second copy; **`?download=1`** on the three image byte routes (`files/[id]/actions/download.ts`, `files/proxy/[...key]/route.ts`, `mount-points/[id]/blobs/[...path]/route.ts`) answering `attachment` instead of `inline` through NEW `lib/api/content-disposition.ts` (26 lines); `lib/download-utils.ts` (+49 — `downloadImageUrl`/`downloadGalleryEntry` hand the URL, not the bytes, to `triggerUrlDownload` so Electron streams); realtime: `queryKeys.chats.gallery(id)` on the EXISTING `chats` topic (`lib/query/keys.ts` +6, `topic-map.ts` +4). **Bug 130** — `app/api/v1/images/route.ts` (+26): optional `chatId` on `generateImageSchema`, folded into `linkedTo` beside the tag ids through a `Set`. **Bug 129** — the Gallery button's `chatPhotoCount` gate read `?action=files`, an action `handleGet` never dispatched (fell through to the whole-chat 200, `data.files` undefined → 0 forever); counter DELETED, `chatPhotoCount`/`fetchChatPhotoCount` leave `useChatData`, the button loses its gate. SPA: `PhotoGalleryModal.tsx` (521-line rewrite — the grid), `ChatGalleryImageViewModal.tsx` (384-line rewrite — the detail view; the two hard-wired "first character" album buttons REMOVED for the shared album dialog, + provenance line + Jump-to-message), `SaveImageDialog.tsx` (+80), `ChatSidebar.tsx`, `ImageModal.tsx`, `useChatGallery.ts` (85 lines NEW), `ChatModals.tsx`, `SalonView.tsx`. Help: `help/chat-gallery.md` NEW (122 → **123**), `chat-message-actions.md`, `chat-participants.md`, `photo-gallery.md`; `docs/developer/API.md` (+142, both actions + the download parameter). v5 surfaces: `api/chat_media.rs` (the message-scoped save over `photos::save_image_to_album` — W4.9b/P4.6ab), `quilltap-web`'s `?action=` dispatch tables (P4.67/P4.72 — the chat GET/POST action lists are CENSUSED there and the new actions move them), the files listing family, `api/images.rs` + `images_routes.rs` (P4.73 — the `?action=generate` Zod parse gains `chatId`), the P4.D114 `Content-Disposition` header work (`inline` today — the `attachment` arm is new), the P4.D114 download surfaces + transcribed `clipboard-utils`, and the SPA's `images/photo-gallery-modal` + `chat/sidebar/organize-section.ts` — where **v5 ALREADY diverged from bug 129's shape**: its Gallery entry is ungated ("v5 has no per-chat photo count on the chat read, so the entry …" — a recorded divergence), so v5 never had bug 129 and that divergence now RETIRES to v4's post-fix shape (an ungated button whose label count comes from the gallery query). The 2026-08-25 dogfood note "`qt-image-gallery` still has no v5 host" is the same surface. **Bug 129 needs the invariant carried, not assessed away: a control gated on a fetched count needs a test that the fetch reaches an endpoint that exists.** | UNPROCESSED |
| `d3f0ed133` | 2026-09-09 | doc: version update after feature changes merged in | NO-PORT? | `4.10.0-dev.16` → `4.10.0-dev.18` in the README badge, `package.json`, `packages/quilltap/package.json` and the lock's two version lines (the intermediate `-dev.10…17` steps rode the three PRs' own bumps). No ported comparand; both version fields agree on `main`, hazard (6) stays closed. | UNPROCESSED |

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
