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

- **Oracle baseline: `25f534c0b`** — "fix: a hostname change no longer makes
  the app kill its own database (bug 126)" (v4 main, 2026-09-08 11:29 -0500,
  `4.10.0-dev.9`), adopted at the `25f534c0b` progressions + bug-126 drift
  catch-up round unification (P4.D166 ∥ P4.D167 → {P4.D168 ∥ P4.D169} ∥
  P4.D170, 2026-09-09). CLAUDE.md's Status bullet agrees. UNMOVED by this
  check — no round has run since.
- **Checked:** 2026-09-09 (a standalone `/driftcheck` from a main checkout,
  ~09:40, hours after the `25f534c0b` unification landed). The §2 probe
  FAILED: two commits arrived on `main` after that unification's cleanup
  check, and the checkout's in-flight bug-131 edits — recorded as dirt last
  time — are now one of them.
- **v4 `main` HEAD at check:** `78b381a96` (2026-09-09 09:26 -0500,
  `4.10.0-dev.21`, "Fix bug 132: describe_image returned a generated image's
  label, not a description") — **TWELVE commits past the baseline**: the ten
  already tabled below plus `d14da3a56` (bug 131) and `78b381a96` (bug 132),
  both landed this morning, both PORT.
- **v4 `bugfix` tip at check:** `1a2b2164c` — UNMOVED (measured by content
  per §4 step 2: its delta against `main` is still the subprompts commit
  reversed plus everything since; it is simply behind, and nothing
  unabsorbed lives on it).
- **v4 `release` tip at check:** `8fbf2afe0` ("release: 4.9.2") — UNMOVED.
- **Checkout at check:** branch **`main`**, tree **CLEAN**. The 14 modified
  + 3 untracked files the last check recorded were the human mid-edit on
  bug 131; they committed as `d14da3a56` (and `bugs/bug-131-…md` moved to
  `bugs/fixed/`). The last check's prediction — "expect a bug-131 commit on
  top" — held. Nothing is in flight now.
- **Re-probed 2026-09-09 ~15:00 at `/setupphase` (the `78b381a96` round's
  ordering; the four surveys had run against a CLEAN tree ~09:50):** HEAD
  UNMOVED at `78b381a96`, `bugfix`/`release` UNMOVED — but the checkout is
  **DIRTY again: 8 modified + 1 untracked, the human mid-edit on v4 bug
  133** ("moderated chat image escalation"):
  `lib/background-jobs/handlers/story-background.ts` (+77/−?),
  `lib/image-gen/appearance-resolution.ts`,
  `lib/tools/handlers/image-generation-handler.ts`,
  `help/dangerous-content.md` (+8/−?), two unit tests, `docs/CHANGELOG.md`,
  `docs/developer/bugs.md`, and the new
  `docs/developer/bugs/fixed/bug-133-moderated-chat-image-escalation.md`.
  **This dirt is the EXPECTED state in the round's §R.2** (recorded so the
  seven lanes' probes do not re-alarm on it; any OTHER dirt, or a HEAD
  move, is a STOP). It cannot poison a lane: every regen this round runs
  from a detached worktree pinned at `78b381a96` (§5.1), and the lanes
  port the PIN. Intersection, for the NEXT round: `story-background.ts` is
  the file P4.D175's bug-132 writer edit lands in (`services/story_
  background_job.rs`), `help/dangerous-content.md` is one of P4.D175's
  eleven re-vendored files, and the two image-gen files are P4.9a/W4.7f
  surfaces (`image_generation`, the appearance resolver family) —
  **prediction: a bug-133 commit on top of `78b381a96`, class PORT,
  becomes the next round's first row** and re-opens exactly those v5
  files. Do not fold it into this round.
  **Pre-authorized exception for the seven lanes (written here at
  ordering, mirrored in every order's §R.2):** if the ONLY change a lane's
  probe finds is that predicted bug-133 commit having LANDED — tree CLEAN,
  `78b381a96..main` exactly ONE commit naming bug 133 over the recorded
  nine files — the lane proceeds on its `78b381a96` pin and records the
  sha; anything else is a STOP. The next `/driftcheck` tables it.
- **Verdict: DRIFT PENDING — 12 commits** (all `UNPROCESSED`). Six shipped
  features/fixes and six riders. The two new rows are both **PORT on
  already-ported surfaces, and v5 measurably HAS both defects** (measured
  at this check, not inferred — see the §3 rows):
  - `d14da3a56` **bug 131** — a user-driven seat's talkativeness never
    reached the speaking-order draw, because four of six map-building sites
    read the `@deprecated` LLM-only accessor. v5 carries that alias
    faithfully (`crates/quilltap-core/src/participant_filters.rs:127`) and
    calls it at `services/turn_orchestrator.rs:512` and `:739` to build the
    talkativeness map — the same blind spot, same shape.
  - `78b381a96` **bug 132** — generated images stored a caption in the
    `description` column and `describe_image` served it ahead of the
    generation prompt. v5 writes both labels
    (`services/story_background_job.rs:839`,
    `services/character_avatar_job.rs:365`) and serves
    `stored-description` first (`tools/photo.rs:970`, before the
    `generation-prompt` arm at `:989`).
- **⚠ TWO schema moves are owed, both still unabsorbed** (unchanged by this
  check): `chat_messages.routeTrail` (`5841a8c62`) and
  `chats.cycleOrderParticipantIds` (`2aca73ad6`) — each arrives BOTH as a
  migration and in the shape `generateDDL` reads, so each owes a D23 re-dump
  **and** a boot ensure, and the P4.D78 (bug 68) precedent applies: the two
  DDL shapes may disagree and both are carried. **Bug 132 adds a THIRD
  migration but NO schema move** —
  `clear-generated-image-placeholder-descriptions-v1` is a pure DATA heal
  (`files.description` → NULL where `source = 'GENERATED'` and the label
  matches; `doc_mount_file_links.description` → `''` where the MIME type is
  an image), so it wants the P4.D140/P4.D152 boot-heal + ledger-row shape,
  not a re-dump. No cache-version bump anywhere in the twelve.
- **⚠ Porting-order fact for the catch-up round: `d14da3a56` REWRITES the
  call sites `2aca73ad6` added** (the `charactersMap` build in
  `turn-orchestrator.service.ts` moves above the `if`; `?action=turn`'s
  hand-rolled loop, which `2aca73ad6` had just touched, is deleted whole).
  Port the turn-manager family from the TIP, not from `2aca73ad6` — a lane
  transcribing the intermediate lands code the very next commit removes.
  The same holds for the four chat-message services and the autonomous-room
  handler.
- **Vendored-artifact obligations the twelve create (re-read hazards 4, 9,
  10 below):**
  - **`help/**` — v4 is at 123 files, v5's re-vendored tree (P4.D168, at
    `25f534c0b`) at 122.** The count is unchanged by the two new commits
    (`78b381a96` EDITS `help/keep-image-tools.md`, adds nothing;
    `d14da3a56` touches no help file — its `chat-turn-manager.md` edits
    rode `2aca73ad6`). The file that ADDS one is still `86d59660c`
    (`help/chat-gallery.md`). Edited across the twelve:
    `chat-gallery.md` (new), `chat-message-actions.md`,
    `chat-participants.md`, `photo-gallery.md`, `chats.md`,
    `connection-profiles.md`, `dangerous-content.md`,
    `character-progressions.md`, `chat-multi-character.md`,
    `chat-turn-manager.md`, `keep-image-tools.md`.
    `help_tree_equivalence` goes RED against any oracle regenerated past
    `25f534c0b`, by design.
  - **`public/schemas/qtap-export.schema.json` moved by +19 lines**
    (`5841a8c62`, the `routeTrail` message field) and is UNMOVED by the two
    new commits. **`qtap_schema_embed_guard` is RED against the live
    checkout** (measured by P4.D169: 92,797 bytes at HEAD against the
    vendored 89,769; the vendored copy is byte-identical to `25f534c0b`,
    md5 `430bb227c7029550f773134f42a0c6eb`). The re-vendor rides the
    route-trail port — vendoring a schema field for a column v5 does not
    yet carry would be wrong — so **until that round lands, every full
    workspace gate needs `QT_V4_ROOT=<a 25f534c0b pin>`** or the fail-fast
    run loses its tail to this tripwire. The two SPA-served schemas
    (`qtap-custom-tool`, `qtap-progression`) are UNMOVED past `25f534c0b`;
    P4.D170's `public_schemas_vendor_guard` stays green.
  - Unmoved: the 21-file sample-prompt source directory, and v4's installed
    `zod` at `4.5.4` (re-measured at this check — hazard (1) stays closed).
    **The two new commits change no dependency at all**: every
    `package.json`/`package-lock.json` hunk in them is the identity-version
    lines (`4.10.0-dev.18` → `-dev.21` across the twelve).
- **CONVERGENCE row: bug 127 (`4a9be9878`).** Filed by v4 (`07eee4f4c`)
  from THIS PORT's P4.D170 lane finding it while transcribing
  `ProgressionsSection.tsx` — v4's `invalidIds.join('</code>, <code>')`
  inside a JSX expression is escaped by React. v4 adopted v5's
  map-with-separator shape. **No oracle compares the line**, so no pin
  trips at any baseline move; the retirement site is the divergence note in
  `apps/web/src/app/progressions/progressions-section.spec.ts` (P4.D170),
  which the absorbing lane rewrites as a convergence record. Bugs 128, 129
  and 130 were filed by v4 from its own code map — NOT convergences.
- **Regen rule in force: PIN REQUIRED at `25f534c0b`** — v4 `main` HEAD is
  twelve commits past the baseline (the checkout is clean, but HEAD alone
  settles it). Every oracle regen and fixture build runs from a detached
  worktree pinned at `25f534c0b` per §5.1 until a round absorbs the twelve
  and moves the baseline. **`qtap_schema_embed_guard` is RED against the
  live checkout** (hazard 9) — a full workspace gate needs
  `QT_V4_ROOT=<a 25f534c0b pin>` until the route-trail port re-vendors the
  export schema.
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
  `78b381a96` `package.json` and `packages/quilltap/package.json` both read
  `4.10.0-dev.21` (re-measured 2026-09-09); the lag survives only on
  `bugfix`. Still no `--version`
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
  one evening, and two direct-to-main bug fixes (131, 132) the next morning. §4 step 2's two-branch rule stays load-bearing — measure
  `bugfix` by CONTENT, never its commit list, and remember a content diff
  can be non-empty simply because `bugfix` is behind.
- _Superseded (2026-09-09, at the `25f534c0b` unification's cleanup check):
  DRIFT PENDING — 10 commits, checkout DIRTY on the in-flight bug 131.
  Before that (2026-09-09, the same unification's opening check): DRIFT
  PENDING — 13 commits against baseline `2f4254b42` (4 ORDERED + 9
  UNPROCESSED). Before that (2026-09-08 afternoon): DRIFT PENDING — 4
  commits, all ORDERED, PIN REQUIRED at `2f4254b42`; (2026-09-08 midday): 3
  commits; (2026-09-08 morning): CLEAR._

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
| `07eee4f4c` | 2026-09-08 | docs: file bug 127 — the progressions card escapes its own markup | NO-PORT? | Docs only (`bugs.md` + `bugs/bug-127-…md`). The filing of a defect THIS PORT found: P4.D170's transcription of `ProgressionsSection.tsx` measured v4's `invalidIds.join('</code>, <code>')` inside a JSX expression rendering raw markup for two or more unreadable ids, and recorded v5's map-with-separator rendering as a divergence (`progressions-section.spec.ts`). The fix is `4a9be9878` below. | ORDERED(P4.D175 — NO-PORT ratification; the bug-127 file mirrors at its `bugs/fixed/` path) |
| `9fc664c94` | 2026-09-08 | docs: plan for the message route trail — every model tried, in order, under the avatar | NO-PORT? | Docs only — `docs/developer/features/message-route-trail.md` (286 lines; MOVED to `features/complete/` with an "As built" section by `5841a8c62`), the changelog, the docs index. **The design of record for `5841a8c62`** — read at ordering time. Mirror candidate for `docs/v4/` at the next baseline move. | ORDERED(P4.D175 — NO-PORT ratification; design of record read by P4.D171/P4.D173/P4.D177) |
| `5fb6bedd6` | 2026-09-08 | docs: file bug 128 — the Salon's memory count goes stale and disarms its own delete button | NO-PORT? | Docs only (`bugs/bug-128-stale-chat-memory-count.md`, 231 lines — the plan `4a9be9878` carries out, incl. WHY `memories` must stay out of `REPOSITORY_TOPICS`). Filed by v4 from its own live Friday instance (chat `27961b14`: 59 rows, `(0)` on a long-lived tab) — NOT a convergence. | ORDERED(P4.D175 — NO-PORT ratification) |
| `df1a075e8` | 2026-09-08 | docs: plan for the Salon chat gallery (every image in a conversation, savable and downloadable) | NO-PORT? | Docs only — `docs/developer/features/salon-chat-gallery.md` (224 lines; MOVED to `features/complete/` by `86d59660c`), the index, the changelog, a bug-128 cross-note. **The design of record for `86d59660c`**, and the commit that FIRST FILED bugs 129 and 130 (the never-rendered Gallery button; the images route ignoring `chatId`). Mirror candidate. | ORDERED(P4.D175 — NO-PORT ratification; design of record read by P4.D174/P4.D176) |
| `c0f9232af` | 2026-09-08 | docs: retire twelve shipped feature specs to features/complete/ | NO-PORT? | 56 files, but every non-doc hunk is a comment-only `See docs/…` path rewrite (29 source files, one line each: `app/**`, `components/**`, `lib/services/home-data.service.ts`, `lib/workspace/types.ts`, `migrations/scripts/quantize-embeddings.ts`, one plugin test) plus the lockfile's two version lines — VERIFIED by grepping the non-doc hunks for any non-comment line (only the two version lines remain). **Fact the round wants: `character-progressions.md`, the design of record P4.D167 ratified, now lives at `docs/developer/features/complete/character-progressions.md`** — the unifier's `docs/v4/` mirror wire should mirror it at the path the NEW baseline (`25f534c0b`) has, `docs/v4/developer/features/character-progressions.md`, since the mirror is a snapshot at the baseline. Ratify with the file list; nothing to port. | ORDERED(P4.D175 — NO-PORT ratification with the re-run grep; the twelve MOVES are a unifier `docs/v4/` wire) |
| `4a9be9878` | 2026-09-08 | Fix bugs 127–128: stale memory count and escaped markup (#58) | PORT (bug 128) + CONVERGENCE (bug 127) | **Bug 128 — PORT onto the P4.D123–D125 realtime subsystem.** v4 adds a `memories` realtime topic: declared in `lib/schemas/realtime.types.ts` (`REALTIME_TOPICS` + `ALL_REALTIME_PREFIXES`), keyed `queryKeys.memories.chatCount(chatId)` in `lib/query/keys.ts`, mapped in `lib/realtime/topic-map.ts` (`queryKeysForTopic`; `TOPIC_ID_FIELDS.memories = []` — the emptiness is load-bearing, and `memories` is deliberately NOT in `REPOSITORY_TOPICS` because `firstIdArg` would publish a memory id under a chat-scoped topic), published from `lib/realtime/job-topics.ts`'s `topicsForCompletedJob` for the four chat-scoped memory job types (each reading `chatId` off its payload) + collection-wide for `MEMORY_HOUSEKEEPING`, and from ONE parent-side delete chokepoint (`lib/memory/memory-gate.ts` — `deleteMemoryWithUnlink` / `deleteMemoriesWithUnlinkBatch`, collection-wide; the route's own publish was REMOVED in the PR's second commit as redundant-and-wrong, since the gate stays silent for a chat with nothing to delete). Client: `useChatData.ts` subscribes `useRealtimeTopic('memories', chatId)` three lines from its fetcher (`cache: 'no-store'` added); `useMemoryActions.ts` disables the button at zero (`.qt-tool-palette-button:disabled` already styled), replaces the bare `return` with a toast, and re-reads the count from the server before the confirmation; `ChatSidebar.tsx` (+7). v5 surfaces: `crates/quilltap-core/src/realtime/{types.rs,job_topics.rs,publish_sites.rs}` + the 73-case `realtime_topics_equivalence` tier-1 family (P4.D123) — the topic ENUM, the prefix list and the job→topic map are all comparands there and MOVE; `crates/quilltap-core/src/db/memories.rs::{delete_with_unlink,delete_many_with_unlink}` (the gate twin's publish site); the SPA's `core/realtime-topic-map.ts` + `core/realtime.types.ts` (P4.D125), `chat/sidebar/edit-section.ts` (**Delete Memories (n)** — MEASURE whether v5 reads the count once at mount and gates the click on a stale zero before calling it inherited; the bugs.md v5 column says "Not yet assessed"), and `chat/memory-cascade-dialog.ts`. Also two `help/**` edits (`character-progressions.md` +6/−?, `chat-participants.md` +11) — re-vendor obligation. **Bug 127 — CONVERGENCE onto P4.D170:** `ProgressionsSection.tsx` now renders `invalidIds.map((id, i) => <span key={id}>{i > 0 && ', '}<code>{id}</code></span>)`, v5's shape; v4's two new pins in `progressions.test.tsx` (the two-id case asserts no `</code>` in the text + a `<code>` per id; the one-id sentence unchanged). Retire P4.D170's divergence note in `apps/web/src/app/progressions/progressions-section.spec.ts` to a convergence record; no oracle compares the line, nothing trips. | ORDERED(P4.D175 server ∥ P4.D177 client — bug 128; P4.D177 — bug 127 convergence record) |
| `5841a8c62` | 2026-09-08 | Message route trail: every model tried, in order, under avatar (#59) | PORT-NEW (with a SCHEMA MOVE) | **The feature, 2,112 insertions / 45 files, incl. a NEW `chat_messages.routeTrail` column (D23 re-dump + boot ensure — see §1).** Storage: nullable JSON `routeTrail` (`migrations/scripts/add-route-trail-message-column-v1.ts` + `migrations/scripts/index.ts`; the repository Zod shape in `lib/database/repositories/chats-messages.ops.ts` +26 — the source `generateDDL` reads; `docs/developer/DDL.md`; `lib/startup/prettify.ts` +1); each entry `{profileId, profileName, provider, modelName, via: primary|retry|concierge|understudy|tier-pick, outcome: answered|failed|refused, trigger?, evidence: finish-reason|inferred, detail? (≤200 chars)}`; NULL whenever nothing failed (nearly every message), no backfill; the last entry always agrees with `provider`/`modelName` (asserted by test). Recording: ONE chokepoint `lib/services/chat-message/route-trail.ts` (198 lines, NEW) is the only writer of two new `StreamingState` fields (`routeFailures`, `routeVia` — `lib/services/chat-message/types.ts` +14); `provider-failover.service.ts` (+66) calls it at every site that already logs a failure (the hard-error opener, the chain walk's key-less / thrown / empty candidates, the empty-response opener, the same-profile retry, the Concierge's uncensored reroute — recorded from `routeResult.connectionProfile`; every empty-body classification runs BEFORE `resetStreamingBuffersForSwap` clears `rawResponse`; the pre-call Concierge reroute gets no row but seeds `routeVia = 'concierge'`); `FallbackChainResult.attempts` / `summarizeFallbackAttempts` UNTOUCHED. Persistence: `message-finalizer.service.ts` (+18), `orchestrator.service.ts` (+7), `primary-stream.service.ts` (+7), `streaming.service.ts` (+5). Transport: the SSE `done` event carries it; the chat-GET projection lists it (`app/api/v1/chats/[id]/handlers/get.ts` +4); `.qtap` export carries it with the message (`public/schemas/qtap-export.schema.json` +19 — hazard (9), the re-vendor rides this port; an imported `profileId` is deliberately NOT remapped); `apply-chat-continuation.ts` does NOT copy it. Display: `lib/chat/route-trail-display.ts` (144 lines, pure — collapse adjacent same-profile rows, the ❌/🚫 marks, hover text; `lib/chat/__tests__/route-trail-display.test.ts` 139 lines is a tier-1 seed) + `components/ui/RouteTrailBadge.tsx` (replaces `ProviderModelBadge` when a trail is present; theme hook `[aria-label="Models tried for this reply"]`, no new qt-* class); `MessageRow.tsx` (memo comparator gained an O(1) identity check on `routeTrail`, normalised undefined≡null), `MessageDesktopAvatar.tsx`, `useSSEStreaming.ts` (+9), `app/salon/[id]/types.ts`; a compile-time + runtime parity assertion tying the client-safe trigger enum to the engine's `FallbackTrigger`. Help: `chats.md` (+35), `connection-profiles.md` (+7), `dangerous-content.md` (+8) — re-vendor. Twelve v4 test files as seeds (the finalizer's 160-line and the failover chain's 150-line suites carry the persistence pins). v5 surfaces: `services/provider_failover.rs` + `services/message_finalizer.rs` + `services/primary_stream.rs` + the streaming state (P4.D135 / P4.68 / P4.72 — the failover `llm_logs` rows and the `auth` chain arm landed there), the SSE `done`/chain-complete event on the `Event` channel (P4.D160's `paused` key precedent), `api/salon.rs`'s chat-GET five-field projection (P4.D51/P4.D60), `generators/**` + the import remap (P4.D46/P4.D87 — the deliberate NON-remap needs its own pin), `chat_continuation` (Continue Elsewhere), and the SPA's provider/model badge under the avatar. `IDENTITY_STACK_BUILDER_VERSION` / `PROMPT_CACHE_STRUCTURE_VERSION` untouched. The commit message's "manual V4test pass … still outstanding" is v4's own owed proof. | ORDERED(P4.D171 substrate → P4.D173 server ∥ P4.D177 SPA; help → P4.D175) |
| `86d59660c` | 2026-09-09 | Add the Salon chat gallery — every image in a conversation (#60) | PORT-NEW + PORT (bugs 129, 130) | **The feature, 4,306 insertions / 51 files — all six phases of `salon-chat-gallery.md`.** ONE server-side enumerator `lib/photos/chat-gallery.ts` (962 lines, NEW — nine sources: uploads + library links, `generate_image` output, both Generate Image entry points, `attach_image` re-shows, Librarian attaches, Lantern story backgrounds incl. superseded, Aurora avatar repaints likewise, the cast's standing portraits, Markdown-referenced images in message prose; over BOTH id species — `files.id` and `doc_mount_file_links.id`; deduped by content hash, newest first, portraits last; each entry carries source, species, is-current-background / is-worn-avatar, deletability; `__tests__/unit/lib/photos/chat-gallery.test.ts` 620 lines is the seed) — NO v5 counterpart; `GET /api/v1/chats/[id]?action=gallery` (`handlers/get.ts` +25 — entries + per-source counts + total); `POST /api/v1/chats/[id]?action=save-image` (`actions/save-image.ts` 137 lines NEW, `actions/index.ts`, `handlers/post.ts`) — the chat-scoped twin of the message-scoped save, guarded by GALLERY MEMBERSHIP rather than message attachment, sharing one Zod body schema, one attribution resolver (`lib/photos/save-attribution.ts` 93 lines NEW) and `lib/photos/save-image-to-album.ts` (+18) with `messages/[messageId]/route.ts` (−…); the `/chats/[id]/files` listing (`files/route.ts` 85 lines moved) now SHARES the enumerator's message-attachment walk instead of a second copy; **`?download=1`** on the three image byte routes (`files/[id]/actions/download.ts`, `files/proxy/[...key]/route.ts`, `mount-points/[id]/blobs/[...path]/route.ts`) answering `attachment` instead of `inline` through NEW `lib/api/content-disposition.ts` (26 lines); `lib/download-utils.ts` (+49 — `downloadImageUrl`/`downloadGalleryEntry` hand the URL, not the bytes, to `triggerUrlDownload` so Electron streams); realtime: `queryKeys.chats.gallery(id)` on the EXISTING `chats` topic (`lib/query/keys.ts` +6, `topic-map.ts` +4). **Bug 130** — `app/api/v1/images/route.ts` (+26): optional `chatId` on `generateImageSchema`, folded into `linkedTo` beside the tag ids through a `Set`. **Bug 129** — the Gallery button's `chatPhotoCount` gate read `?action=files`, an action `handleGet` never dispatched (fell through to the whole-chat 200, `data.files` undefined → 0 forever); counter DELETED, `chatPhotoCount`/`fetchChatPhotoCount` leave `useChatData`, the button loses its gate. SPA: `PhotoGalleryModal.tsx` (521-line rewrite — the grid), `ChatGalleryImageViewModal.tsx` (384-line rewrite — the detail view; the two hard-wired "first character" album buttons REMOVED for the shared album dialog, + provenance line + Jump-to-message), `SaveImageDialog.tsx` (+80), `ChatSidebar.tsx`, `ImageModal.tsx`, `useChatGallery.ts` (85 lines NEW), `ChatModals.tsx`, `SalonView.tsx`. Help: `help/chat-gallery.md` NEW (122 → **123**), `chat-message-actions.md`, `chat-participants.md`, `photo-gallery.md`; `docs/developer/API.md` (+142, both actions + the download parameter). v5 surfaces: `api/chat_media.rs` (the message-scoped save over `photos::save_image_to_album` — W4.9b/P4.6ab), `quilltap-web`'s `?action=` dispatch tables (P4.67/P4.72 — the chat GET/POST action lists are CENSUSED there and the new actions move them), the files listing family, `api/images.rs` + `images_routes.rs` (P4.73 — the `?action=generate` Zod parse gains `chatId`), the P4.D114 `Content-Disposition` header work (`inline` today — the `attachment` arm is new), the P4.D114 download surfaces + transcribed `clipboard-utils`, and the SPA's `images/photo-gallery-modal` + `chat/sidebar/organize-section.ts` — where **v5 ALREADY diverged from bug 129's shape**: its Gallery entry is ungated ("v5 has no per-chat photo count on the chat read, so the entry …" — a recorded divergence), so v5 never had bug 129 and that divergence now RETIRES to v4's post-fix shape (an ungated button whose label count comes from the gallery query). The 2026-08-25 dogfood note "`qt-image-gallery` still has no v5 host" is the same surface. **Bug 129 needs the invariant carried, not assessed away: a control gated on a fetched count needs a test that the fetch reaches an endpoint that exists.** | ORDERED(P4.D174 server ∥ P4.D176 SPA; `help/chat-gallery.md` → P4.D175) |
| `2aca73ad6` | 2026-09-09 | Draw a multi-character chat's speaking order once per cycle | PORT (with a SCHEMA MOVE) | **1,277 insertions / 38 files, landed while the `25f534c0b` unification was in its gate.** The rotation for a cycle is drawn UP FRONT — a talkativeness-weighted permutation of the present character seats, sampled without replacement — stored on the chat and followed seat by seat; previously each turn made its own weighted pick (the distribution is unchanged: successive sampling either way, so the weighted-random family's expectations should survive, the SEQUENCE machinery does not). **Schema:** NEW `chats.cycleOrderParticipantIds TEXT DEFAULT '[]'` via `migrations/scripts/add-cycle-order-column-v1.ts` (+ `index.ts`, `introducedInVersion: '4.10.0'`, `dependsOn: sqlite-initial-schema-v1`, no backfill — `'[]'` reads as "no rotation on file") AND in the schema shape (`lib/schemas/chat.types.ts` +16, `docs/developer/DDL.md` +1, `lib/startup/prettify.ts` +1) → **a second D23 re-dump + boot ensure owed** beside the route trail's. Engine: NEW `lib/chat/turn-manager/cycle-order.ts` (232 lines — `resolveCycleOrder`, the SINGLE writer, called first by every "who is next" path: the chain loop, the first-responder resolver, the message finalizer, `?action=turn`, the autonomous-room handler; `computeCycleOrderAfterMessage` at the same write chokepoints that advance `spokenThisCycleParticipantIds` — a message landing, a skipped user turn, an LLM's "nothing to add" pass; mid-cycle cast changes REPAIRED not redrawn — a departed/archived seat skipped on read, a joiner appended; a one-character chat stores no rotation; the manual queue still jumps the line; a summoned character is struck from the remaining order), `weighted-random.ts` NEW (`pickWeightedRandom` moved out of `selection.ts`, still re-exported), `selection.ts` (+83/−… — the old one-at-a-time pick kept as the FALLBACK for a chat with no rotation on file), `turn-order.ts` (+63), `state.ts` (+55), `types.ts`, `queue.ts`, `utils.ts`, `index.ts`; `lib/database/repositories/chats-messages.ops.ts` (+26 — the strike at the message-landing chokepoint), `message-finalizer.service.ts` (+11), `orchestrator.service.ts` (+11), `participant-resolver.service.ts` (+12), `turn-orchestrator.service.ts` (+6), `lib/background-jobs/handlers/autonomous-room-turn.ts` (+11), `lib/chat/apply-chat-continuation.ts` (+7 — the column is NOT copied, presumably; measure), `app/api/v1/chats/[id]/actions/turn.ts` (+31). Client: `SalonView.tsx`, `app/salon/[id]/types.ts` — the sidebar's Participants list shows the STORED order ("position 3 now means third") instead of the talkativeness-sorted guess. Help: `chat-multi-character.md`, `chat-participants.md`, `chat-turn-manager.md` (+35) — re-vendor obligation (the count stays 123). Seeds: `cycle-order.test.ts` (444 lines), `turn-order.test.ts` (+87). v5 surfaces: the whole Phase-3 turn chain (`services/turn_manager/*` — selection, turn order, state, the weighted pick), `services/message_finalizer.rs`, `services/orchestrator.rs`, `services/participant_resolver.rs`, the turn-orchestrator, `chat_activity`/`chat_continuation`, the autonomous enclave `step()`, `api/salon.rs`'s `?action=turn`, the SPA's `chat/sidebar` participants list (P4.9h1) and the seed-once `impersonationSync`; the `turn_manager_equivalence` / `orchestrator_tier3` / `enclave_step_tier3` families MOVE; the weighted-random fidelity (`Math.random` sequence) is the port hazard — v4's tests will say how the permutation is drawn. **Not a convergence** (no bug number; the in-flight bug 131 that follows it was found by v4's own inspection of this commit's call sites). | ORDERED(P4.D171 substrate → P4.D172 server, read at the TIP with `d14da3a56` ∥ P4.D177 SPA; help → P4.D175) |
| `d3f0ed133` | 2026-09-09 | doc: version update after feature changes merged in | NO-PORT? | `4.10.0-dev.16` → `4.10.0-dev.18` in the README badge, `package.json`, `packages/quilltap/package.json` and the lock's two version lines (the intermediate `-dev.10…17` steps rode the three PRs' own bumps). No ported comparand; both version fields agree on `main`, hazard (6) stays closed. | ORDERED(P4.D175 — NO-PORT ratification) |
| `d14da3a56` | 2026-09-09 | Count a user-driven seat's talkativeness in the speaking order (bug 131) | PORT | **758 insertions / 21 files; the in-flight edit the last check recorded as dirt, now committed.** Six server paths each built their own `characterId → Character` map immediately before asking who speaks next, and **four built it from `getActiveCharacterParticipants` — a `@deprecated` alias for `getActiveLLMParticipants` that returns `controlledBy === 'llm'` seats only**, under a name that reads as the general case. A seat the human drives was therefore absent from the map: its talkativeness fell through to the 0.5 default (a per-chat override still worked), an archived character on it was never dropped from the rotation, and `?action=turn` reported it as `nextSpeakerName: null` / `"Unknown"`. No error, no log line, no missing turn — only a wrong probability. **Fix:** NEW `lib/chat/turn-manager/room-characters.ts` (103 lines — `loadRoomCharacters(repos, participants, {preloaded})` over `getPresentCharacterSeats` through ONE batched `repos.characters.findByIds`; seats whose character cannot be read are simply ABSENT from the map, which is `cycleCandidates`' documented contract; two log lines — a `debug` "[Turn Manager] Room characters loaded" `{seats, requested, resolved}` and a `warn` "[Turn Manager] Room seats with no readable character" `{requested, missing}`) exported from `turn-manager/index.ts`; all six selection sites call it — `turn-orchestrator.service.ts` (`shouldChainNext`; the map moves ABOVE the `if` and now also serves the two name lookups after it, replacing two more `findById` calls), `message-finalizer.service.ts` (`preloaded: [character]` — the seat that just spoke is seeded from the copy in hand, never re-read), `participant-resolver.service.ts` (built from `llmCandidates` before — correct for the pick, wrong for the room-wide draw it feeds first; the pick stays LLM-only via `selectNextSpeaker`'s argument), `orchestrator.service.ts` (`maybePauseForUserSeatTurn` — the ONE site that was already whole-room), `app/api/v1/chats/[id]/actions/turn.ts`, `lib/background-jobs/handlers/autonomous-room-turn.ts`; plus `loadAllParticipantData` (`participant-resolver.service.ts`), which built the same map by hand for PROMPT construction — **a behaviour change beyond the map's width: it used to throw `CharacterVaultUnavailableError` out of speaker selection (and out of the read-only `?action=turn` behind the participant sidebar) on an unreadable vault; the batched list overlay logs and DROPS instead**. `cycle-order.ts` gains a docblock saying why the narrow accessor is the wrong input. Eleven cases in `room-characters.test.ts` (254 lines) are the tier-1 seed; the talkativeness case measures a DIFFERENCE (300 draws loud vs 300 quiet) because at the 0.5 default the seat already leads a fifth of cycles. **v5 measurably HAS the bug** (measured at this check): `crates/quilltap-core/src/participant_filters.rs:127` carries the deprecated alias faithfully, and `services/turn_orchestrator.rs:512` + `:739` build the talkativeness map from it; `services/message_finalizer.rs:1632` and `services/participant_resolver.rs:247` run per-seat `characters_read::find_by_id` loops; `services/orchestrator.rs:800` is v5's already-whole-room site, matching v4's one correct one. The batched seam EXISTS — `db/characters_read.rs:346 find_by_ids` (P4.65) — so the port is a consolidation, not new plumbing. v5 surfaces: `select_speaker.rs`, `turn_order.rs`, the four services, `enclave/step.rs:623` + `enclave/announce.rs:284` (two more alias readers to assess), `api/salon.rs`'s `?action=turn`; families that MOVE: `turn_manager_equivalence`, `turn_pause_filters_equivalence` (which compares BOTH accessors by name), `orchestrator_tier3`, `enclave_step_tier3`. **Not a convergence** — v4's bug doc records it Found 2026-09-09 by inspection while reviewing `2aca73ad6`'s call sites; "v5 status: Not investigated". **Stacks on `2aca73ad6` and REWRITES its hunks — port from the tip (see §1).** | ORDERED(P4.D172 — with `2aca73ad6`, from the TIP ∥ P4.D177 SPA) |
| `78b381a96` | 2026-09-09 | Fix bug 132: describe_image returned a generated image's label, not a description | PORT | **664 insertions / 17 files.** Two writers put a CAPTION in the column every reader treats as "what this picture shows": `lib/background-jobs/handlers/story-background.ts` stored ``Story background for: ${payload.sceneContext \|\| chat.title}`` on the `files` row AND passed the same string to `writeLanternBackgroundToMountStore` (the Scriptorium link), and `lib/background-jobs/handlers/character-avatar.ts` did the same with ``${character.name} — wardrobe portrait`` through `writeCharacterAvatarToVault`. `handleDescribeImage` (`lib/tools/handlers/doc-edit/photo-handlers.ts`) served `entry.description` FIRST, ahead of the generation prompt and the vision call — so a character asking what a backdrop showed was told the chat title, with `source: "stored-description"` and `Success`. **Fix, three parts:** (a) both jobs write `description: null` and OMIT it from the bridge call (the storage-write options), each with a why-comment naming bug 132; (b) `handleDescribeImage` reorders to prompt → stored → vision (`generationRevisedPrompt \|\| generationPrompt` first, matching `runGenerateImageDescription` in `lib/chat/file-attachment-fallback.ts`, which already had it that way), and when the answer is the prompt AND a stored description exists it rides along as a NEW optional output field `stored_description` with the formatted text gaining a `\n\nOn file: <stored>` tail; the log line gains `hasStoredDescription`; the `DescribeImageOutput` docblock and the source-order comments are rewritten; (c) NEW `migrations/scripts/clear-generated-image-placeholder-descriptions.ts` (199 lines, id `clear-generated-image-placeholder-descriptions-v1`, registered in `migrations/scripts/index.ts` + a `prettify.ts` PRETTY_LABEL) clears the two label shapes already on disk — `files.description` → NULL where `"source" = 'GENERATED'` AND `description LIKE 'Story background for: %' OR LIKE '% — wardrobe portrait'`; `doc_mount_file_links.description` → its `''` default under the same predicate gated on `originalMimeType LIKE 'image/%'` (the link side has no source column). **NO schema move** — a pure data heal, so the v5 shape is a boot ensure + ledger row (P4.D140/P4.D152), not a D23 re-dump. **v5 measurably HAS the bug** (measured at this check): `crates/quilltap-core/src/services/story_background_job.rs:839` formats `Story background for: {}` and `services/character_avatar_job.rs:365` formats `{character_name} — wardrobe portrait`; `tools/photo.rs` responds `stored-description` at `:970`, BEFORE the `generation-prompt` arm at `:989` and the `vision-call` arm at `:1009`. v5 surfaces: those two job modules, `tools/photo.rs`'s `handle_describe_image` + the `DescribeImageOutput` contract (P4.D106/P4.D107 — the tool's own tier-3/tool-wire families and the `describe_image` catalog entry move), `photos/auto_describe_attachment.rs`, `services/file_fallback.rs` (the sibling reader that already had the right order — verify v5's), the Scriptorium link writers, and the boot-heal ledger. `help/keep-image-tools.md` edited (+5/−3) — re-vendor obligation, count stays 123. Seeds: `doc-edit-handler-photos.test.ts` (+23) and a 202-line migration test. **Not a convergence** — v4's bug doc records it reported 2026-09-09 from v4's own Salon transcript (`story_background_1789958023065.webp` answering `"Story background for: Bite Order and Kisses"`); "v5 status: Not investigated". ⚠ **The 2026-08-24 dogfood pass proved v5's `stored-description` arm LIVE and green** — that proof was of the wrong behaviour, and the walk row should be re-read when this lands. | ORDERED(P4.D175; help → P4.D175) |

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
