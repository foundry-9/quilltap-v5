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

- **Oracle baseline: `baa85e19b`** — "fix(aurora): the star that sets a
  default prompt calls a route that exists (bug 154)" (v4 main, 2026-09-18
  07:04, `4.10.0-dev.48`), adopted at the `baa85e19b` bug-154
  default-system-prompt drift catch-up + maintenance round unification
  (P4.D201 ∥ P4.D202 ∥ P4.100 ∥ P4.101 ∥ P4.102, 2026-09-18). CLAUDE.md's
  Status bullet agrees.
- **Checked:** 2026-09-21, 18:xx — **the second standalone `/driftcheck`
  that day**, run because v4 landed the FTS5 escalation an hour after the
  first one closed. Previously checked 2026-09-21 17:xx (the nine-commit
  check), 2026-09-19 twice, 2026-09-18 (the round's `/unify`).
- **v4 `main` HEAD at check:** `f45a517a9` ("feat(search): index chat
  messages with FTS5, compress the transcript", 2026-09-21 17:25,
  `4.10.0-dev.61`) — **THIRTEEN commits past the baseline.** Ten of the
  thirteen landed on 2026-09-21 alone, between 10:14 and 17:25.
- **v4 `bugfix` tip at check:** `1a2b2164c` ("bugfix: started 4.9.2 bug
  branch") — **UNMOVED**, so the content probe stands: only pre-absorbed
  lineage. No unabsorbed bugfix work.
- **v4 `release` tip at check:** `8fbf2afe0` ("release: 4.9.2") — UNMOVED.
  Still no `release: 4.10.0` squash.
- **Checkout at check:** branch **`main`**, tree **CLEAN** — the FTS5 work
  that was uncommitted at the previous check is what `f45a517a9` shipped.
  This check classifies committed history only, as instructed.
- **Verdict: DRIFT PENDING — 13 commits, and the blocking row has CHANGED
  CLASS.** Five rows are docs-only NO-PORT? candidates. Of the rest:
  **`f45a517a9` + `186eb09cb` together are now a hard stop, not a
  scheduling problem** (see the next bullet); `e7d77bb60` (Inform) is the
  largest single feature (79 files, a new table, the only D23 re-dump
  owed); `23da0b322` (`quilltap sync`) is a whole new CLI verb plus a
  2,127-line engine with no v5 analogue; `f564b0de3` rewrites the swipe as
  a watched stream; `da9c4f34f` (bug 158) and `0c14fd61f` (bug 157) are
  defects v5 measurably reproduces; `6b0615807` moves the Zod version the
  gate pins. All thirteen rows are UNPROCESSED in §3.
- **⛔⛔ BLOCKING, ESCALATED AND NOW PROVEN BY EXPERIMENT: v5 CANNOT WRITE A
  CHAT MESSAGE to a v4-4.10 instance.** `f45a517a9` puts three triggers over
  `chat_messages`, and **two of them call `qt_text()`** — `_ai`'s INSERT body
  and `_au`'s **`WHEN` clause** (`qt_text(new."content") IS NOT
  qt_text(old."content")`). v5 registers no UDF anywhere (zero hits for
  `create_scalar_function`/`add_function`/`qt_text` across `crates/`), and
  the workspace pins `rusqlite` without the `functions` feature, so
  `qt_text` is **not merely unported — it is not currently compilable.**
  **Measured 2026-09-21 on a read-only COPY of the live Friday instance, by
  opening it with the UDF deliberately unregistered and attempting writes
  inside always-rolled-back transactions (0 rows left behind; copy and
  `.dbkey` deleted after):**
  | attempted write | result |
  |---|---|
  | INSERT USER message, content set | **FAILED — `no such function: qt_text`** |
  | INSERT ASSISTANT message, content set | **FAILED — `no such function: qt_text`** |
  | INSERT USER message, content NULL | **FAILED — `no such function: qt_text`** |
  | INSERT SYSTEM message, content set | **FAILED — `no such function: qt_text`** |
  | INSERT type=staff, content set | **FAILED — `no such function: qt_text`** |
  | UPDATE `content` on an indexed message | **FAILED — `no such function: qt_text`** |
  | UPDATE a non-content column | SUCCEEDED |
  | DELETE an indexed message | SUCCEEDED |
  ⚠ **Note what the table proves beyond the trigger's own `WHEN` guard:
  EVERY insert fails, including the SYSTEM, staff and content-NULL rows the
  `WHEN` clause would have excluded — because SQLite resolves functions when
  it COMPILES the trigger program, before any row is tested.** So the blast
  radius is not "indexed messages"; it is **every `chat_messages` INSERT**
  and **every `UPDATE OF content`**, i.e. `db/chats_messages.rs:888-903`
  (the `content`/`opaqueContent` insert), `:957-959` (`context`), `:966-970`
  (`description`) and `:545-548` (`update_message`, which is a DELETE +
  re-INSERT, so every edit re-enters through the AI trigger). **DELETE still
  succeeds — the worst asymmetry available: a v5 binary on a shared instance
  can destroy messages but not create them.**
- **The live Friday instance is ALREADY in that state. Measured 2026-09-21
  on the same copy:** all five FTS objects present (`chat_messages_fts`,
  `_fts_map`, `_fts_config`, `_fts_data`, `_fts_docsize`, `_fts_idx`) plus
  the three triggers; **142,697 messages with 78,562 in the FTS map**; and
  the transcript itself compressed — **`content` 62,913 BLOB / 25,729 text /
  54,055 NULL**, `opaqueContent` 18,021 BLOB, `description` 213 BLOB,
  `context` 2,328 BLOB (every non-NULL `context` cell). `quilltap.db` fell
  **835 MB → 615 MB** and was last written at 17:56, after the 17:25 commit.
  **Together with the previous check's `llm_logs` (5,627 BLOB) and
  `conversation_chunks` (14,962 BLOB) census, the whole of Friday is now on
  the new format. The dogfood copy must be treated as hostile to v5 —
  writes fail outright and reads of any message over 512 bytes take the
  entire `get_messages` call with them (`db/chats_messages_read.rs:283-287`
  propagates, so one cell fails the whole chat).**
- **Regen rule: PIN REQUIRED at `baa85e19b`**, absolutely and unchanged.
  Thirteen commits of divergence, four of them rewriting paths whose
  families regenerate routinely (search, the swipe, the mount-index write
  path, chat creation). Every oracle regen until the baseline moves runs
  from a detached worktree pinned at `baa85e19b` (§5.1, lane-unique path).
  **Do not reason about which families "could" be affected — pin.**
- **⚠ The workspace gate MOVES in TWO places, both by design, both measured
  red 2026-09-21 by running them:** **(a) `qtap_schema_embed_guard`** —
  `the_embedded_schema_equals_the_v4_checkouts` FAILS;
  `qtap-export.schema.json` grew 93,384 → 95,266 and the NDJSON schema
  10,890 → 10,989 at `e7d77bb60`, and that redness IS Inform's re-vendor
  obligation. **(b) `zod_version_guard`** — FAILS, `4.5.4 → 4.6.5` from
  `6b0615807`; its own failure text names the P4.D158 re-measurement and the
  regen it gates. **Green:** `public_schemas_vendor_guard`, and
  `help_tree_embed_guard` — the latter only because it compares embedded
  against v5's own DISK rather than against v4, so it cannot see the gap
  below. `help_tree_equivalence` is the only family that can.
- **Schema state: a D23 re-dump is OWED from `e7d77bb60` and from that
  commit ALONE — `f45a517a9` does NOT move `generateDDL`, and the reason
  matters.** v4 creates the FTS objects **in the migration, not in
  `ensureCollection`**, and says so: *that path only runs the Zod-derived
  `CREATE TABLE IF NOT EXISTS` and knows nothing of triggers or virtual
  tables.* Confirmed on the v5 side: the D23 dumper
  (`harness/oracle/provision/dump-fresh-schema.ts:57-73`) keeps only
  `type === 'table'` and `type === 'index'` rows, **so triggers are filtered
  out by construction**, and `fresh_schema.json` has 0 `TRIGGER` / 0
  `VIRTUAL` / 0 `fts` today. **The consequence is the opposite of a re-dump:
  a v5-provisioned fresh instance is MISSING all five objects and no D23
  mechanism can ever supply them** — the port needs a first-class
  `db/chat_message_fts.rs` carrying the DDL verbatim. That direction is
  benign (v4's boot reconciler restores them with a warn and a rebuild); the
  direction measured two bullets above is not. ⭐ **And one piece of good
  news that removes a feared blocker: `SQLITE_ENABLE_FTS5` is ALREADY
  compiled into v5's amalgamation** (`crates/quilltap-sqlite3mc-sys/
  build.rs:57`), so the FTS half needs **no build change and no pinned-crate
  risk**. The cost is entirely in the UDF and the codec.
- **`help/**` is BEHIND v4 in TEN files, re-measured 2026-09-21 after
  `f45a517a9`:** v5 has **124**, v4 has **126**. **MISSING (2):**
  `inform.md` (`e7d77bb60`), `cli-sync.md` (`23da0b322`). **DIFFERS (8):**
  `insert-announcement.md` (`e7d77bb60`), `chat-message-actions.md`
  (`f564b0de3`), `cli-docs.md` / `mount-points.md` / `scriptorium.md`
  (`23da0b322`), `dangerous-content.md` (`da9c4f34f`), `data-retention.md`
  (`186eb09cb`), `search.md` (`f45a517a9`). Every other shared file is still
  `cmp`-identical and v5 has nothing extra.
  `help_tree_embed_guard.rs:56` pins `VENDORED_FILE_COUNT = 124`, which
  moves with the re-vendor.
- **✅ RETIRED: the standing `openai 7.10.0` item is RESOLVED by
  `6b0615807`** — and its count was wrong. Measured installed-vs-declared:
  **`openai@7.20.0` against `^7.20.0` in SIX plugin dirs** (`deepseek`,
  `grok`, `nanogpt`, `openai-compatible`, `openai`, `z-ai` — not four), plus
  `@anthropic-ai/sdk@0.115.0`, `@google/genai@1.52.0`,
  `@openrouter/sdk@1.3.11`, all matching. The old bundles sat at **two
  different versions** (7.10.0 and 7.15.0). Strike the phase-4.md
  `npm install` item at the baseline move, with the correction recorded.

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
| `781e3b499` | 2026-09-19 | docs: plan for Inform, out-of-character information delivered before a character's next turn | **NO-PORT?** | **Docs-only — two files, `docs/CHANGELOG.md` (+13) and the NEW `docs/developer/features/salon-inform.md` (+314). No `lib/`, `app/`, `components/`, `packages/`, `help/`, `public/schemas/` or `migrations/` hunk; the commit body says "No code changes yet" and the file list agrees.** Ratification should be trivial, but the spec itself is **the single most useful reference the Inform port has** and v5 already mirrors this tree (`docs/v4/developer/features/`, 21 files) — so the disposition to expect is NO-PORT-RATIFIED *with a mirror*: copy `salon-inform.md` in at the version `e7d77bb60` leaves it (that commit edits it again, +14/−1, to add a status header recording that it is **implemented in 4.10-dev with live verification outstanding**, that the DDL gained an `updatedAt` the draft omitted, that the dialog width was derived from the toolbar's CSS rather than measured (**that caveat is DISCHARGED** — the human confirmed 2026-09-19 that the CSS was tested and is fine; v4's header text is simply stale on the point), and that `extractVisibleConversation` needed a record-only strip the spec never named — mirror the POST-`e7d77bb60` file, not this one). `docs/CHANGELOG.md` is v4's own changelog: NO-PORT, as every round has ratified it. | UNPROCESSED |
| `e7d77bb60` | 2026-09-19 | feat(salon): Inform — out-of-character word handed to the cast | **PORT-NEW** | **The largest single drift this port has faced: 79 files, +4,758/−38, `4.10.0-dev.50`.** A new Salon composer action (the *i* in the gutter, beside Pascal): the operator picks one, several, or every LLM-controlled seat, writes a short second-person passage, and each target receives it **verbatim** as its own system block on its next generation, then it is consumed. Nothing is added to what the operator types — no preamble, no Host voice, no rider — for transparent and opaque characters alike. **Not a convergence** (`docs/developer/bugs.md` did not move; no bug number; a v4-side feature, planned in `781e3b499` three hours earlier). **⚠ Classify the port from the hunks, not this summary or v4's commit body (§5.3).** The shape, by seam, with the v5 home each lands on — every one of them already ported: <br><br>**(1) Storage — a NEW TABLE, so D23 fires.** `lib/schemas/chat-inform.types.ts` (+72, `ChatInformSchema` + `ChatInformInputSchema` + the `PendingInformBatch` read shape) and `lib/database/repositories/chat-informs.repository.ts` (+311) over the standard `AbstractBaseRepository`, registered in `lib/database/repositories/index.ts` (four hunks: export, import, `RepositoryContainer` field, `createRepositories`). **One row per (batch × target) — the body is duplicated per target on purpose, so consumption is a single-row write with no shared array for a buffered job-child write to clobber.** Method names are chosen so the background-job child proxy classifies them without an override (reads `find*`, writes `create`/`mark`/`delete`). `migrations/scripts/add-chat-informs-table.ts` (+107) + `index.ts` (+6) + the `lib/startup/prettify.ts` label. **v5:** a new `crates/quilltap-core/src/db/chat_informs.rs` alongside its direct analogue `db/chat_documents.rs`; **`services/provisioning/fresh_schema.json` must be re-dumped from v4's live `generateDDL` (D23 — never by hand)** into the `main` partition; the migration runner stays deferred, so the existing-instance path needs the same decision earlier new tables got (`ensure_table`, cf. `db/doc_mount_blobs.rs:150`). <br><br>**(2) The prompt path.** NEW `lib/chat/context/inform-block.ts` (+117) — `buildInformBlock`, **the only reader of `chat_informs` on the prompt path, and it never writes.** The block sits **between system blocks 2 and 3** (keeps the cacheable prefix contiguous) and is **empty-is-absent**: with nothing pending it pushes nothing, so an ordinary turn assembles byte-for-byte as before, and **neither builder-version constant moves**. `lib/services/chat-message/context-builder.service.ts` (+65) wires it. **v5:** `services/build_context.rs` — the block-3 seam is at `:2204` ("Compressed-history block (system block 3)"), so the Inform block slots immediately above it; v5 has **no** `context_builder.rs` — `services/build_context.rs` and `services/message_context.rs` are the two homes v4's `context-builder.service.ts` maps onto; `services/orchestrator.rs` takes v4's `orchestrator.service.ts` (+3). <br><br>**(3) Consumption, tied to a persisted assistant message.** `message-finalizer.service.ts` (+17) and `primary-stream.service.ts` (+21, the preserved-partial path). A provider failure that saves nothing, or a "nothing to add" pass, **leaves the passage pending**. `regenerate-swipe.service.ts` (+21): a swipe passes its **whole swipe group**, re-applies exactly what that line's generation saw, is deliberately **not** given pending rows, and **never consumes**. **v5:** `services/message_finalizer.rs`, `services/primary_stream.rs`, `services/regenerate_swipe.rs`. <br><br>**(4) The record never reaches a model — three strips, one of them a genuine v4 hole.** Each post leaves a **Host** message carrying exactly what was typed: **public when every eligible seat was covered, whispered to the targets otherwise** (coverage decides, not how the operator clicked), chip reading "out of character" — `lib/services/announcer/writer.ts` (+98). Stripped in `buildMessageContext` via a named `isRecordOnlyMessage` predicate **which also absorbs the existing Commonplace strip**; in `courier-transport.service.ts` (+9, which builds its own transcript from raw events); and in `extractVisibleConversation` (`lib/chat/context-manager.ts`, +72) — **that last was the hole: it filters by ROLE, the record wears `role=ASSISTANT`, so the async pre-compression would have folded it into a summary returning as system block 3 on every later turn, permanently. It now skips by KIND, which closes the same gap for titles, story backgrounds and the rolling summary.** `buildTurnTranscript` already skipped Staff messages, so an inform never becomes memory; v4 added a test holding that. `lib/memory/cheap-llm-tasks/chat-tasks.ts` (+15/−…). **v5:** `services/announcer/writer.rs`; `services/message_context.rs` (the Commonplace strip is at `:1187`, `system_sender == Some("commonplaceBook")` — this becomes the `is_record_only_message` predicate); `services/courier_transport.rs`; **`chat_tasks.rs:73 extract_visible_conversation`** (the kind-skip — measure whether v5 reproduces the hole; it almost certainly does, and the fix widens beyond Inform); `services/turn_transcript.rs`. <br><br>**(5) The API.** `POST ?action=inform` / `GET ?action=informs` / `POST ?action=cancel-inform` under the existing dispatch — `app/api/v1/chats/[id]/actions/inform.ts` (+242), `actions/index.ts`, `handlers/get.ts` (+8/−…), `handlers/post.ts` (+6), `schemas.ts` (+16). **Cancelling before anyone has collected removes the record too; cancelling after some have collected KEEPS it and drops only the seats still waiting.** `actions/participants.ts` (+18): **removing a participant drops what it was owed.** **v5:** the chats dispatch verbs in `api/` + the census rows that count them; `db/chats_participants.rs:207 remove_participant`. <br><br>**(6) Export / import / backup — rows survive a round trip, consumed ones included, "so swipes stay honest".** `lib/export/ndjson-writer.ts` (+19), `lib/export/types.ts` (+24), `lib/import/quilltap-import-stream.ts` (+13), `quilltap-import/execute.ts` (+93), `reconcile.ts` (+80), `types.ts`; `lib/backup/backup-service.ts` (+14), `restore/{archive,delete-service,preview,restore,uuid-remap}.ts`, `backup/types.ts` (+11). **v5:** `services/backup/**` (incl. `uuid_remap.rs`/`uuid_remapper.rs`, `restore/`), `services/quilltap_import/{execute,reconcile,preview,...}.rs`, the export marshal. **Plus the vendored `public/schemas/qtap-export.schema.json` (93,384 → 95,266) and `qtap-export-ndjson.schema.json` (10,890 → 10,989) — see §1's gate note; `qtap_schema_embed_guard` goes red until re-vendored.** <br><br>**(7) Realtime + the SPA.** `lib/realtime/topic-map.ts` (+5) — pending informs ride the **existing `chats` topic**, no new polling site (`queryKeys.chats.informs(id)`); `lib/query/keys.ts` (+7). NEW `components/chat/InformDialog.tsx` (+290) and `components/chat/PendingInformChips.tsx` (+123, names who is still owed, body's first line on hover, a cross to cancel); `components/chat/ComposerGutterTools.tsx` (+22/−…); `app/salon/[id]/{SalonView.tsx,components/ChatComposer.tsx,components/ChatModals.tsx,components/system-message-labels.ts,hooks/useModalState.ts}`. **v5:** `apps/web/src/app/chat/chat-composer.ts` (the gutter, beside the Pascal/custom-tools popup), `apps/web/src/app/screens/salon/`, the realtime topic map + query-key twins. <br><br>**(8) Autonomous rooms deliver on their next chained turn; Carina does NOT** (it builds its own minimal call) — `lib/chat/__tests__/dynamic-head-distill-latency.test.ts` (+5) and the chat-tasks hunk. **Pending informs do not travel through a merge** — noted in the help. <br><br>**(9) Docs + help.** NEW `help/inform.md` (+89), `help/insert-announcement.md` (+4) → **v5's `help/` is 124 vs v4's 125; those two files are the ONLY differences (measured 2026-09-19), every other shared file `cmp`-identical.** `docs/developer/{API.md,DDL.md,PROMPT_ARCHITECTURE.md}` and `docs/developer/features/salon-inform.md` → the `docs/v4/` mirror. `docs/CHANGELOG.md`, `README.md`, `CLAUDE.md`, `.claude/commands/update-documentation.md`, `package.json`/`package-lock.json`/`packages/quilltap/package.json` stamps → NO-PORT candidates (Tier R at the pin for the CLI stamp, as every round has done). <br><br>**(10) The 21 test files (+2,500-odd lines) are the corpus sources**, per the standing rule — `inform.test.ts` (392), `chat-informs-backup.test.ts` (335), `inform-block.test.ts` (159), the import/export round-trips (272 + 245), the migration test (149), `context-management.test.ts` (+127), `InformDialog`/`PendingInformChips` specs (220 + 141), and the six `services/chat-message/*` additions. <br><br>**⚠ v4's own status header records that LIVE VERIFICATION AGAINST A REAL INSTANCE HAS NOT BEEN RUN** — the ten-step walkthrough in its Verification section. The header's *second* caveat, that the dialog width was derived from the toolbar's CSS rather than measured, is **DISCHARGED**: the human confirmed 2026-09-19 that the CSS was tested and is fine, so v4's header is stale on that point and no lane should chase it. The first caveat stands. The port inherits an oracle that is itself unproven on real data — worth a 💸 dogfood row of its own, and worth expecting v4 follow-up commits. | UNPROCESSED |
| `f564b0de3` | 2026-09-19 | feat(salon): a regeneration says so while it happens | **PORT** | **26 files, +1,226/−81, `4.10.0-dev.51`. Not a convergence** (`docs/developer/bugs.md` did not move; no bug number — a v4-side feature, landed two hours after Inform). Pressing refresh on a character's message used to do nothing visible until the new line appeared; nothing stopped a second press, or a fresh composer message, from landing on a turn already in flight. **The load-bearing change is not the narration — it is that the swipe's generation stops being a blocking non-streaming call.** By seam, with the v5 home each lands on: <br><br>**(1) The service, `lib/services/chat-message/regenerate-swipe.service.ts` (+154/−…).** The single no-tools provider call switches from `provider.sendMessage(...)` to a `for await` over **`withStallWatchdog(provider.streamMessage(...), { provider, modelName, logContext })`** — v4's own comment: *the stall watchdog is not optional on any `streamMessage` consumer (bug 141)*, because an SDK timeout stops at the response headers and a provider that answers then goes quiet would hang the call and the operator's disabled composer indefinitely. A new exported `RegenerateSwipeProgress` union (`status` / `delta` / `reasoning`) is reported through a new **optional** `onProgress` option — **`content` is a DELTA (append), `reasoning` is CUMULATIVE (replace)**, deliberately the same contract the send path's SSE events use. Five status beats in order: `gathering` → `sending` → (on the first chunk carrying content, once) `regenerating` → `saving`, each with v4's exact sentence, `characterName` and `characterId`. **With no callback the generation is identical, just silent**, and a non-streaming caller sees the same finished swipe. <br><br>**⭐ (2) This commit closes v5's OWN tracked deferral, and the port should treat that as the headline.** `crates/quilltap-core/src/services/regenerate_swipe.rs` says so in its header (lines 20–25): *"**Tracked deferral:** the new swipe's `rawResponse` / `reasoningContent` / `thoughtSignature` are set to `null`. The ported `CompletionResponse` is the cheap-LLM subset (`content` + `usage`) and does not carry the provider's raw payload / reasoning / thought signature; forwarding them awaits the richer wire-decoded response…"* — and it still writes all three as `Value::Null` at **`:529-531`**, through `send_message_with_anchor` at **`:448`** with the *"Single non-streaming generation (v4 132–153)"* comment at **`:379`**. **v4 has now moved the persist onto the chunks** (`tokenCount`/`promptTokens`/`completionTokens` from accumulated `chunk.usage`, plus `chunk.rawResponse` / `chunk.reasoningContent` / `chunk.thoughtSignature`), and says explicitly that *every field the swipe persists rides the chunks, so reading the response as a stream costs the record nothing*. Porting this commit therefore **retires the deferral rather than working around it** — and **⚠ it moves a tier-2 comparand**: three columns that v5 has been writing NULL start carrying real values, so expect the regenerate/swipe DB-diff families to go red at the baseline move **by design**, and re-record them at the target rather than "fixing" v5 back. Note also the field-name shift on v4's side (`response.raw` → `chunk.rawResponse`); confirm which the swipe row actually stores before pinning bytes. <br><br>**(3) The stall-watchdog census moves.** v5's `model/stream_watchdog.rs` exposes `watch_stream`, wrapped at **27 call sites across 11 files** today (`model/stream.rs`, `services/{recovery,primary_stream,carina_query,native_tool_loop,initial_greeting,provider_failover,text_tool_loop}.rs`, `services/brahma_console/{mod,orchestrator}.rs`, `services/help_chat/orchestrator.rs`) — and **`crates/quilltap-harness/tests/stream_watchdog_wrap_census.rs` counts them**, so `regenerate_swipe.rs` becoming a twelfth home is a census bump the lane must make deliberately, red-first. This is the P4.D189 discipline exactly: v4 has ONE `streaming.service.ts` funnel and v5 has none, so every new consumer is its own wrap site. <br><br>**(4) The SSE route.** `app/api/v1/messages/[id]/route.ts` (+124/−…): `POST ?action=swipe&stream=1` answers `text/event-stream` with `status` / content delta / cumulative reasoning / `done`+message / `error`; **without the flag the endpoint still answers 201 with the swipe as JSON.** A new shared `resolveSwipeTarget` makes both handlers *"refuse the same things for the same reasons"* (`notFound('Message')`, `badRequest('Only assistant messages can be swiped')`, `badRequest('Staff and system messages cannot be regenerated')`). **A refusal BEFORE the stream opens stays an ordinary JSON error; one after it is an `error` event, because the headers are long gone — so callers check `res.ok` first.** `lib/services/chat-message/index.ts` (+3) re-exports the SSE encoders (`encodeContentChunk` / `encodeReasoningChunk` / `encodeStatusEvent` / `encodeErrorEvent` / `safeClose` / `safeEnqueue` / `sseStreamResponse`). **v5:** `api/types.rs:279 Request::MessageSwipe`, dispatched at `api/engine.rs:1814` to `api/salon.rs:1210 message_swipe_generate` (the generate branch) / `:1258 message_swipe_switch`; the streaming mode lands on `quilltap-web`'s SSE machinery beside the send path's — **measure whether v5's boundary wants a flag on the existing verb or an Event-channel stream, since D-rule streaming is only ever on the `Event` channel.** `docs/developer/API.md` (+29) documents the contract → the `docs/v4/` mirror. <br><br>**(5) The SPA, including two bugs v4 found on the way — MEASURE whether v5 reproduces them.** NEW `app/salon/[id]/hooks/useRegeneration.ts` (+204) with `hooks/index.ts`; `MessageRow.tsx` (+72) dims/greys the line being replaced under a "Regenerating..." plate **that withdraws the moment there is prose, so there is no blank gap**; `VirtualizedMessageList.tsx`, `MessageActionBar.tsx`, `ChatComposer.tsx` (+32), `SalonView.tsx`, `useChatData.ts` (+27), `useMessageActions.ts` (−18). The three found-on-the-way defects: **(a) `MessageRow`'s memo comparator is exhaustive, so the new prop was invisible to it** — the status strip updated while the message sat untouched; **(b) after a regeneration the view stayed on the previously-selected variant** (reconciliation carries swipe selection by id, right for every refetch *except* this one — `useChatData.selectSwipeVariant` now puts the new variant on display); **(c) `ChatComposer`'s `disabled` prop was declared and destructured but wired to nothing**, so every control was gated on `sending` alone. **v5:** `apps/web/src/app/chat/{message-row,message-list,transcript-reconcile}.ts` and the Salon screen — (b) is the one most likely to reproduce (v5 has its own `transcript-reconcile.ts`), (a) is Angular-signal-shaped and probably N/A, (c) needs a look at v5's composer gating. <br><br>**(6) qt-* classes + the theme mirror.** `app/styles/qt-components/_chat.css` (+57) adds six classes — `qt-chat-message-regenerating`, `qt-chat-regenerating`, `-original`, `-plate`, `-plate-text`, `qt-chat-message-action-bar-disabled` — plus `qt-chat-response-status[data-stage="regenerating"|"saving"]`. **v5's home is `apps/web/src/styles/qt-components/_chat.css`, which today has NO `regenerat*` class** (its single match is an unrelated comment at `:3082`) — and the repo's `check-qt-classes` guard governs. v4 also mirrors the work into `packages/theme-storybook` (`src/css/qt-components.css` +201, published 1.0.71) **including the action-bar and response-status families, neither of which had ever been mirrored** — v5 vendors no `qt-components.css`, so treat the storybook package as a NO-PORT class *source*, not a port target (cf. p4.d128). <br><br>**(7) Help + stamps.** `help/chat-message-actions.md` (+36) → **v5's copy DIFFERS (measured 2026-09-19); re-vendor obligation.** `docs/CHANGELOG.md`, `README.md`, `package.json`/`package-lock.json`/`packages/quilltap/package.json`/`packages/theme-storybook/{package.json,CHANGELOG.md}` stamps → NO-PORT candidates (Tier R at the pin for the CLI stamp). <br><br>**(8) The three test files (+256) are the corpus sources:** `route.test.ts` (+121, the SSE contract), `useChatData.test.ts` (+81, the variant-selection fix), `regenerate-swipe.service.test.ts` (+54, the progress contract). | UNPROCESSED |
| `0ecc6067c` | 2026-09-21 | docs: plan for quilltap sync; file bugs 155 and 156 | **NO-PORT?** | **Docs-only — five files, +823/−0, measured from `--stat`: `docs/CHANGELOG.md` (+14), `docs/developer/bugs.md` (+2), the NEW `bugs/bug-155-byte-write-blanks-caption.md` (+135) and `bugs/bug-156-overwrite-leaves-stale-chunks.md` (+136), and the NEW `docs/developer/features/cli-document-store-sync.md` (+536). No `lib/`, `app/`, `components/`, `packages/`, `help/`, `public/schemas/` or `migrations/` hunk — the file list agrees with the body.** Ratification is trivial; the mirror obligation is not. <br><br>**(1) Mirror targets, and a path that MOVED.** v5 mirrors this tree at `docs/v4/developer/` (`bugs/fixed/` at 149 files, `features/` + `features/complete/` at 92). All three new docs are **ABSENT from the mirror today (measured)**. ⚠ **Do not mirror the paths this commit creates:** `23da0b322` renames both bug docs into `bugs/fixed/` (`git log --diff-filter=R` confirms `R083`/`R080`), and v4's `bugs.md` rows already link the `fixed/` paths — mirror `docs/v4/developer/bugs/fixed/bug-155-…md` and `…/bug-156-…md`, not the birth paths, the same rule prior rounds applied to bugs 144/150. `cli-document-store-sync.md` (+536) mirrors to `docs/v4/developer/features/` and is **the design of record for `23da0b322`** — mirror it whether or not that port is ordered this round. <br><br>**(2) Bugs 155/156 are NOT convergences and are NOT this row's port obligation.** Neither carries v5/port attribution; v4 found both while planning the sync verb ("*Neither was reported live*"), and both are fixed by `23da0b322`/`0c14fd61f`, so their PORT class sits on those rows. What this commit owes the port is the **warning** — and the measurement has since been made: **v5 reproduces both** (see `23da0b322` seams 1–2). <br><br>**(3) The spelling guard is already exempt** — `harness/tools/check_spelling.py:63` sets `ALLOWED_PREFIXES = ("docs/v4/",)`, so mirroring v4 prose verbatim cannot trip the Quilltap-spelling gate. <br><br>**(4) `docs/CHANGELOG.md` is v4's own changelog: NO-PORT**, as every round has ratified. | UNPROCESSED |
| `23da0b322` | 2026-09-21 | feat(scriptorium): quilltap sync — a store and a directory, kept in step | **PORT-NEW** (+ two embedded **PORT** bug fixes v5 measurably has) | **46 files, +5,871/−106, `4.10.0-dev.53`.** A new CLI verb `npx quilltap sync <store> <path>` mirroring a **database-backed** document store and a server-local directory **both ways**: SHA-256 first, `lastModified` second, so equal bytes with unequal clocks are a `touch` and never a copy; the side that changed wins; both changed since the last run is a **`conflict`, reported and skipped**; deletions propagate only when `.quilltap-sync.json` proves the entry was there last run, so a first run **creates, never deletes**. **Not a convergence** — `bugs.md` moves only to close bugs 155/156, which v4 filed against itself while planning this verb. ⚠ **Classify and port from the hunks, not the body (§5.3): the body's "chunks and vectors are never read or written by the sync" is false at module level** — `sync/apply-store.ts` imports and calls `reindexAfterDatabaseWrite`; what is true is that the sync issues no SQL of its own against `doc_mount_chunks`. By seam: <br><br>**⭐ (1) Bug 155 — v5 HAS IT, measured.** v4's `linkBlobContent` UPDATE branch now builds its SET clause per field (`if (input.description !== undefined)` / `extractedText` / `extractionStatus`), so an omitted metadata field means *keep*; an explicit `''` still clears; INSERT keeps its blank defaults. **v5 reproduces the pre-fix behaviour exactly**: `crates/quilltap-core/src/db/doc_mount_file_links.rs:1031-1037` computes `unwrap_or_default()`/`unwrap_or("none")` and `:1055-1077` writes **all five columns unconditionally**. The live v5 trigger is the one v4 names: `services/mount_index/file_ops.rs:392 write_dest_bytes`, whose binary branch passes `description: None, extracted_text: None, extraction_status: None` at `:440-444`, reached from four call sites (`:665, :724, :845, :1035`). ⚠ **A type change is required, and it is the P4.96/P4.98 tri-state pattern:** v4's `description?: string` carries no null so `Option<String>` already suffices, but `extractedText?: string \| null` **does** distinguish `undefined` from `null`, so `LinkBlobInput.extracted_text` must become `Option<Option<String>>` (one gate also covers `extractedTextSha256`). `services/quilltap_import/document_stores.rs:560-600` already carries an escalation comment asking for the same widening. <br><br>**⭐ (2) Bug 156 — v5 HAS IT, both halves, measured.** v4 computes `contentChanged = existingLink.fileId !== fileRow.id`, calls a new `dropChunksForLinks` (tolerant of `no such table`, rethrowing anything else), splices `chunkCount = 0` into the UPDATE, and on that flag invalidates the writer's mount **and every fanned-out sibling's**; `fanOutGroupFileId` does the same for exactly the siblings whose `fileId` moved. **v5's update branch has none of it** — `db/doc_mount_file_links.rs:809-828` writes `fileId/folderId/plainTextLength/conversionStatus/allow*/lastModified/updatedAt` and nothing else; `fan_out_group_file_id` (`:2295`) has no `moved` filter; `delete_chunks_by_link_id` **exists** at `:1917` and `link_document_content` never calls it; **v5 has no chunk-cache invalidation at all** (the only `invalidate_mount_point` is the no-op seam at `photos/save_image_to_album.rs:199`). Second half likewise: v4 added `reindexAfterDatabaseWrite` to `writeDestBytes`; **v5's `write_dest_bytes` re-chunks nowhere** (native-text branch returns at `file_ops.rs:425` under `// v4 emitDocumentWritten — the watcher deferral`). And the predicate the fix turns on is ported byte-for-byte: **`services/mount_index/scanner.rs:423`, `link.chunk_count == 0 \|\| link.conversion_status != "converted"`** — so in v5 today the rescan **provably cannot** see a repointed link, exactly as v4's filing says of v4. <br><br>**(3) The shared post-write helper.** NEW `lib/mount-index/post-write-reindex.ts` (+65) hoists the re-chunk + hard-link-group pass out of `writeDatabaseDocument` into one never-throwing, parent-process-only block. **v5 has both halves but inlined as a pair at each writer:** `services/mount_index/reindex_file.rs:91`/`:105` and `services/mount_index/link_groups.rs:73`, called from only two sites (`db/doc_mount_file_links.rs:~657/:668`, `db/database_store.rs:224/:240`). ⚠ `reindex_file.rs:82-90` records the deliberate divergence that **v5 has no `QUILLTAP_JOB_CHILD` guard** (its job runner is in-process) — v4's helper is a no-op in the child and v5's is not. That is the one place this seam needs thought rather than transcription. <br><br>**(4) Per-location timestamps — new optional inputs, NO schema change.** `LinkBlobInput`/`LinkDocumentInput` gain optional `lastModified` (honoured on INSERT **and** UPDATE) and `createdAt` (INSERT only); `fanOutGroupFileId` takes `lastModified` as a new positional; NEW `setLinkTimestamps(linkId, {lastModified, createdAt})` moves the clocks without touching bytes. **Every ordinary caller omits both and gets `now`, so this is comparand-neutral for existing corpora.** **v5:** `LinkBlobInput` (`:467-489`) and `LinkDocumentInput` (`:435-450`) have **neither field**, and `set_link_timestamps` **does not exist anywhere in the workspace** (grep of `crates/` + `apps/` is empty). <br><br>**(5) ⚠ NO DDL change — D23 does NOT fire.** `DDL.md +6` is **three prose paragraphs** inserted into the existing `doc_mount_file_links` narrative. **No new table, column or index, no `migrations/` file, no `public/schemas/` byte** (confirmed against `--stat`). v5's `fresh_schema.json` already carries `lastModified`/`createdAt` on `doc_mount_file_links`, so **no re-dump is owed here** and `qtap_schema_embed_guard` is untouched by this commit. <br><br>**(6) The engine — 9 NEW `lib/mount-index/sync/` modules, +2,127, with NO v5 analogue whatsoever** (`grep` for `mount_index/sync` in `crates/` is empty): `types.ts` (the `SyncEntry` both sides reduce to, `MTIME_TOLERANCE_MS`, `SIDECAR_SUFFIX`, `SYNC_MANIFEST_FILENAME`), `planner.ts` (+602, **PURE** — no SQLite, no `fs`, no clock: the decision table, the `CHARACTER_VAULT_KEYSTONES` refusal list mirroring `REQUIRED_VAULT_FILES` plus `wardrobe/instructions.md`, the `createdAt`-takes-the-older rule, and a `birthtimeIsSettable` guard that stops an unsettable-birthtime platform planning the same futile `touch` forever), `manifest.ts` (`ManifestMismatchError` → 409; a corrupt manifest reads as absent because first-run rules can only create), `sidecar.ts` (`<file>.description.md`, binaries only, trailing-newline hygiene both ways, `descriptionSha256` over the *parsed* text), `walk-disk.ts`/`walk-store.ts` (dot-segments invisible on BOTH sides; a store path that *is* a sidecar is a reserved-path conflict), `apply-disk.ts` (temp-then-rename, `resolveInTarget` refusing any `..` escape), `apply-store.ts` (every write through an existing chokepoint, **no transcoding** so a `.png` keeps its sha unlike a Scriptorium upload, **no metadata opinion** — which is *why* bug 155 had to go first — and compare-and-swap against the planner's sha → `StoreRaceError`), and `index.ts` (+358, the per-store `inFlight` mutex, store-type/archived-vault guards, `SyncRefusedError`). **v5 homes it would sit beside:** `services/mount_index/` and `db/database_store.rs`. **`planner.ts` + its 576-line test is the highest-value tier-1 corpus in the commit.** <br><br>**(7) The API — a NEW action on an already-ported route.** `app/api/v1/mount-points/[id]/route.ts` (+90): `POST ?action=sync` with `syncSchema` (`targetPath` min 1, `dryRun`, `direction: both\|to-disk\|to-store`, `prefer: newer\|store\|disk`, `propagateDeletes`, `useManifest`, all `.optional().default(…)`) as **the single source of truth for the CLI's flags**; two INFO lines; `SyncRefusedError` → 409 for `SYNC_IN_PROGRESS`/`CONVERSION_IN_PROGRESS`/`SCAN_IN_PROGRESS`, else 400; `ManifestMismatchError` → 409; `ENOENT` → 400 naming `quilltap docs docker-mounts`. **v5:** mount-point variants are `MountPointList` `api/types.rs:1262` … `Delete` `:1282`, dispatched `api/engine.rs:3141-3164` into `api/mount_points.rs`. **A new `Request::MountSync` variant plus its wire-decode census rows** — and since all six body keys are `.optional().default()` (plain optionals, **not** tri-states), a decode-raw pin is the cheap guard. Ported by **P4.6p unit 4** and **P4.6v ∥ P4.6w ∥ P4.6x**; the file-ops mutation surface by **P4.6y**. <br><br>**⭐ (8) The CLI — a genuinely NEW verb, and v5 has no trace of it.** `bin/quilltap.js` (+9: the main-help line after `docs`, the `SUBCOMMANDS` set, the dispatch arm), NEW `lib/sync-command.js` (+323, thin client: resolves the store name/`qtap://` URI against the local mount index read-only — the one thing it opens the DB for — then POSTs; **does not re-validate**; exit codes **0 clean / 1 error-or-failed-action / 2 unresolved conflict**) and NEW `lib/sync-report.js` (+167, **pure** renderer, fixed 8/6 column widths, `formatActionLines`/`formatSummary`/`exitCodeFor`; **the second column is the side that CHANGES**). **v5:** `crates/quilltap-cli/src/main.rs:37` (`SUBCOMMANDS`, 12 names, **no `sync`**) + the dispatch match at `:236-241` (5 of 12 implemented, the rest `not_yet_available()` at `:83`); the server leg already has its transport at `crates/quilltap-cli/src/http.rs`. **Tier R today is 223 cases** (51 `ctx.case(` + 167 `ctx.case_with(` + 5 hand-rolled, in `crates/quilltap-cli/tests/cli_differential.rs:1774`). ⚠ **FOUR existing Tier R cases go RED at the baseline move by design:** `"main help"` (`:1808`; v5's `src/help/main_help.txt:9` lists `docs` with no `sync` after it) and `"completion bash"`/`"completion zsh"`/`"completion fish"` (`:3764-3766`) — see seam 9. Growing Tier R means a new section plus a byte-captured `sync_help.txt` (**capture, don't extract** — the #119/bug-144 lesson: run the oracle BEFORE writing a user-facing string). <br><br>**⭐ (9) Shell completion IS ported in v5, so all three templates move.** v4 edits `bash.template` (+24), `zsh.template` (+36, incl. a whole new `_quilltap_sync` with an 11-entry `_arguments` spec and a `store`/`target` state machine), `fish.template` (+21), and `completion-coverage.test.js` (+1: `sync: ['lib/sync-command.js', 'printSyncHelp']`). **v5's homes:** `crates/quilltap-cli/src/help/completion/{bash,zsh,fish}.template` (13,862 / 22,226 / 24,645 bytes, `include_str!`'d at `src/completion_cmd.rs:15-17` — no `clap_complete` in the workspace), **all three containing zero occurrences of `sync`** (measured). v4's coverage test has a v5 mirror at `crates/quilltap-cli/tests/completion_behavior.rs:276`, which drives real `bash` and `zsh -n` and whose `help_sources_cover_every_dispatched_subcommand` **fails the moment `sync` is dispatched without a help source**. <br><br>**(10) Help + the docs mirror.** NEW `help/cli-sync.md` (+212), plus `help/cli-docs.md` (+13), `help/mount-points.md` (+28), `help/scriptorium.md` (+29). See §1's help paragraph for the full nine-file gap. `docs/developer/{API.md +58, CLI.md +63, DDL.md +6}` and the **renamed** `features/{ => complete}/cli-document-store-sync.md` (+153) → the `docs/v4/` mirror (`cli-document-store-sync.md` is **absent** there). `docs/CHANGELOG.md` (+65), `README.md`, `.claude/commands/update-documentation.md`, `packages/quilltap/README.md` (+27) and the stamps → **NO-PORT candidates** (Tier R at the pin for the CLI stamp). <br><br>**(11) The 7 test files (+2,094) are the corpus sources:** `sync/planner.test.ts` (576), `sync/engine.integration.test.ts` (605), `mount-points/sync-action.test.ts` (212), `doc-mount-write-metadata.integration.test.ts` (327 — **bugs 155+156's red-first arms**), `sync/manifest.test.ts` (127), `sync/sidecar.test.ts` (89), `packages/quilltap/lib/__tests__/sync-report.test.js` (158, pure — a direct tier-1 for the renderer). <br><br>**(12) ⚠ Foreseeable comparand movement.** Bug 155's fix changes what a byte-overwrite leaves in the five metadata columns; bug 156's changes `chunkCount` **and deletes `doc_mount_chunks` rows**; `writeDestBytes` now re-chunking **creates** chunk rows where none were created before. Families driving the changed functions (measured by grep over `harness/oracle/cases/`): **`doc_mount_file_links_tier2`** (the primary — drives both `linkDocumentContent` and `linkBlobContent`), **`doc_blob`**, **`mount_link_groups`**, **`mount_case_resolution`**, **`mount_case_moves`**, every family whose oracle calls `writeDatabaseDocument` (21 case files incl. `characters_scaffold`, `groups_tier2`, `projects_tier2`, the `doc_*` set, `metadata_vault_roundtrip`, `wardrobe_instructions_cascade`), plus **`mount_ops`**/**`mount_write`**. Seam 4 is **neutral** for all of them — every existing caller omits `lastModified`. | UNPROCESSED |
| `0c14fd61f` | 2026-09-21 | fix(scriptorium): a caption belongs to a path, not to the bytes (bug 157) | **PORT** | **13 files, +318/−46, `4.10.0-dev.54`. Not a convergence** — bug 157 was **reported live on v4's own `V4test` instance** on 2026-09-21 and fixed the same day; its Provenance row reads *"Original to v4, from the content/link split"*, its v5 status is "Not assessed", and nothing credits this port. A small, total fix with a **compile-breaking signature change** — and **v5 has the defect**. <br><br>**⭐ (1) The repository signature change — `linkId` goes optional → REQUIRED, and the `LIMIT 1` fallback is deleted.** `updateDescription(id, description, linkId?)` → `linkId: string` and `updateExtractedText(id, input, linkId?)` → `linkId: string`; both lose the eight-line `SELECT id FROM doc_mount_file_links WHERE fileId = ? LIMIT 1` block and its `return null` guard, and both the UPDATE and the joined read-back bind `linkId` directly. v4's new docstrings state the reason as a rule: **there is no such thing as "the" link for a blob** — content-addressing puts every byte-identical path on one `doc_mount_files` row, and a vault holds every avatar at both `photos/` and `images/history/`, so **two links is the normal case, not an edge one**. Required rather than optional is deliberate: *"the next caller that has not resolved a location fails to compile."* v4's `updatePath`'s own `LIMIT 1` is **left alone** as a documented legacy facade — do not sweep it. **v5's home: `crates/quilltap-core/src/db/doc_mount_blobs.rs`** — `update_description` at **`:404`** (`link_id: Option<&str>`, fallback at **`:419`**) and `update_extracted_text` at **`:453`** (fallback at **`:471`**), both read verbatim and re-verified at this check. <br><br>**⭐ (2) Every v5 caller, measured — THREE of six pass `None`, and all three already hold the right link id.** (a) `api/mount_files.rs:873 mount_blob_update` (the blobs-item PATCH, `Request::MountBlobUpdate`, dispatched `api/engine.rs:4123-4129`) calls `update_description(&meta.id, &desc, None)` at **`:891`**, where `meta` came from `find_by_mount_point_and_path` five lines earlier and **carries `link_id`** — and the comment at `:889` records the v4 behaviour being faithfully reproduced (*"v4's route calls updateDescription WITHOUT a linkId"*). **That comment is now stale and is the fix site.** (b) `api/chat_media.rs:1510 ensure_image_description` (called from `:1728`) — **v4's second half, and v5 has it exactly**: it *reads* `blob.description` at `:1520` off the joined view (the **attached** link's row) and *writes back* with `None` at **`:1589`**, so a multi-linked blob's cache write lands on an arbitrary sibling and **the vision model re-runs on every attach**; the comment at `:1582-1584` names the hazard and reproduces it. (c) `services/quilltap_import/document_stores.rs:637` passes `None` while `created.link_id` is already captured at `:588` and used in the adjacent raw UPDATE at `:617`. **The three already-correct callers:** `api/mount_files.rs:353 mount_file_update` (`Some(&meta.link_id)` at `:392`) and `services/mount_index/store_file.rs:426`/`:449` (`Some(&result.link_id)`) — the ones v4 explicitly leaves untouched. ⚠ Incidental, **not** this commit's business but the lane will be in the file: **v4 has THREE `updateExtractedText` sites in `store-file.ts` (success / empty / catch) where v5 has TWO** — pre-existing, worth a look. <br><br>**(3) The test-fixture type.** `__tests__/unit/lib/fixtures/mock-repositories.ts` (+4/−4) grows the third parameter in both the interface and the factory. **No v5 analogue needed** — v5's callers bind the real repository, not a mock container. <br><br>**(4) ⚠ NO DDL change and NO `public/schemas/` byte.** `DDL.md` is **not touched by this commit at all** (the +6 belongs to `23da0b322`); no `migrations/` file, no new column. **D23 does not fire.** <br><br>**(5) No SPA hunk, and none is owed** — v4's UI reaches this through the PATCH by `(mountPointId, relativePath)`; the wrong-row choice was entirely server-side. The fix is one argument at three v5 call sites. <br><br>**(6) Docs + stamps.** `bugs.md` (+10) and the NEW `bugs/fixed/bug-157-caption-lands-at-the-wrong-path.md` (+171, naming the exact live store and the shared `fileId`) → the `docs/v4/` mirror. `README.md` and the three stamps (`.53 → .54`) → NO-PORT candidates. <br><br>**(7) The regression corpus** is `doc-mount-write-metadata.integration.test.ts` (+83 on top of the +327 from `23da0b322`), shaped as exactly the thing v5's fixtures lack: *two links on one blob*, a caption written to each, and `extractedText` written to the **second-created** link. <br><br>**(8) ⚠ Comparand movement is CONDITIONAL, and that is the trap.** v4's filing says why the bug survived: *"a single-link store — everything the tests build — makes the fallback correct."* The only v5 family whose oracle drives `updateDescription` is **`attach_mount_file_equivalence`** (12 cases + 1 v5-only arm; `updateExtractedText` appears in **no** oracle case at all), and unless its fixture grows a **second link on one blob** the fix is **comparand-invisible — green before and after**. **So this port cannot be proven by re-running the existing families: the corpus must grow a two-link blob first, red-first, or the fix is unverified by construction.** Same shape for the `mount_blob_update` route family and the `.qtap` import round-trip. | UNPROCESSED |
| `5e4252898` | 2026-09-21 | docs: plan for consolidating CLI reference into package README | **NO-PORT?** | **Docs-only — three files, +61/−0, measured: `docs/CHANGELOG.md` (+7), `.claude/commands/update-documentation.md` (+1), and the NEW `docs/developer/features/cli-comprehensive-help.md` (+53). No code hunk of any kind.** The thinnest row in the batch. <br><br>**(1) The new file is a PLAN that is superseded three commits later** — `e11a51f44` **renames** it to `features/complete/cli-comprehensive-help.md` with a status header. v5's mirror already carries a 92-file `features/complete/` subtree, so **mirror the POST-`e11a51f44` file at the `complete/` path**, the same move prescribed for `salon-inform.md`. Mirroring the birth path leaves an orphan the next `/driftcheck` has to clean up. <br><br>**(2) `.claude/commands/update-documentation.md` is v4's own slash-command catalogue — NO-PORT**, the same class as v4's `CLAUDE.md` rule lines (ratified at the `bcd7e4852` round). v5 has its own `.claude/commands/` and does not track v4's. <br><br>**(3) Nothing here touches CLI behaviour**, so Tier R is unaffected by this commit specifically; the stamp check belongs on `e11a51f44` and `6b0615807`. | UNPROCESSED |
| `e11a51f44` | 2026-09-21 | docs: one comprehensive CLI reference, in the package README | **NO-PORT?** | **Docs-only — twelve files, +168/−276, measured. No `lib/`, `app/`, `components/`, `packages/quilltap/{bin,lib}/`, `help/`, `migrations/` or `public/` hunk;** the only `packages/` files are `README.md` (+99) and the version stamp. But ratification is **not** trivial, because the commit moves where the CLI's canonical prose lives and **v5's mirror cannot reach the new home**. <br><br>**(1) `docs/developer/CLI.md` is GUTTED: 28,099 → 6,542 bytes (measured), −284 lines.** Six headings survive (`# Quilltap CLI — developer notes`, Docker startup + the bind planner, "Sync: why the engine is server-side", shell-completion internals, See also). Everything else moved into `packages/quilltap/README.md`, which gains Character Archive, Locking + the five-minute heartbeat window, `qtap://` URI addressing, and Docker binds. <br><br>**(2) ⚠ v5's mirror has NO `packages/` tree — the new canonical reference is outside its reach (measured).** `docs/v4/` mirrors `CHANGELOG.md`, `developer/` and `releases/`; the only README under it is `docs/v4/developer/bugfix-sessions/README.md`. **A straight re-mirror of the gutted `CLI.md` would silently shrink the port's CLI reference by 21.5 KB.** Expect **NO-PORT-RATIFIED *with a widened mirror*:** re-mirror `CLI.md` **and** add `docs/v4/packages-quilltap-README.md` (or an agreed equivalent), or the P4.D158 §G CLI bank loses its source. <br><br>**(3) Nothing in v5 reads `CLI.md` programmatically (measured).** `ggrep -rn "CLI\.md"` outside `docs/v4/` returns only porting prose (three work orders and `status-log.md`); no `.rs`/`.ts`/`.mjs`/`.py`/`.sh` reads it. **So the mirror is reference-only and no gate moves.** <br><br>**(4) CLI behaviour does not move, so Tier R should be 223/0 at the pin** — but the commit bumps both stamps, so run it rather than asserting it. ⚠ **The added README content names four user-visible rules worth banking as Tier R / dogfood questions** (prose is not a hunk, so none is a port obligation *from this commit*): the two sync refusal cases (a second concurrent run; character-vault keystones report a conflict rather than being deleted), the link-vs-copy link-group semantics, the Docker bind planner's rules, and the caveat that `instances restore-key` does **not** re-encrypt character archive bundles. <br><br>**(5) `CLAUDE.md`, `README.md`, `docs/CHANGELOG.md`, the two `.claude/commands/` files, `DEVELOPMENT.md`, `package-lock.json` and the stamps → NO-PORT candidates**, the standing class. | UNPROCESSED |
| `6b0615807` | 2026-09-21 | chore(deps): npm update across the app, packages and plugins | **PORT** (not a chore — see (1)) | **63 files, +67,546/−18,910. ⚠ Classify from the hunks and from what is INSTALLED, not from "chore(deps)" or "No behavior changes intended" — three measurements contradict the prose.** Root: Next 16.3.4→16.3.5, React 19.2.8→19.3.0, **Zod 4.5.4→4.6.5**, **openai 7.15→7.20**, **@openrouter/sdk 1.2.106→1.3.11**, TanStack Query 5.102.8→5.103.2, Playwright 1.62.1→1.63.0, Jest 30.5.1→30.5.2. All 15 plugins patch-bumped and **rebuilt**. <br><br>**⭐ (1) THE WORKSPACE GATE MOVES: `zod_version_guard` goes RED — measured by running it.** `crates/quilltap-harness/tests/zod_version_guard.rs:46` holds `RECORDED_ZOD_VERSION = "4.5.4"` and `:73` asserts it against `$QT_V4_CHECKOUT/node_modules/zod/package.json`; **v4's installed zod is now `4.6.5`** and the test FAILS with `left: "4.6.5" / right: "4.5.4"`. The assert's own message names the obligation: repeat the **P4.D158 unit 2 item 4** measurement (diff `zod/v4/locales/en.js`'s sentences, `zod/v4/core/regexes.js`'s regexes, grep for any newly-reachable validator), then regenerate the two hand-rolled engines (`crates/quilltap-core/src/pascal/custom_tool_types.rs` + its SPA twin `apps/web/src/app/pascal/custom-tool-types.ts`), the SPA corpus, and the ~150 Zod-sentence/regex edge sites, *then* bump the constant. **That is this row's real cost, and it is far larger than the diff suggests.** Note also ~40 `4.5.4` prose claims across `crates/` doc comments (`jsstr.rs:57-62,73,87`, `api/zod_issues.rs:25,504,575`, `pascal/custom_tool_types.rs:30,1105,1217,1433`, `progressions/schema.rs:26,33`) — unenforced, but each goes stale. Sibling pin of the same class, unaffected: `enclave_cron_equivalence.rs:31` (`EXPECTED_CRONER = "10.0.1"`). <br><br>**(2) The five `.optional().prefault()` drops are TYPE-LEVEL ONLY — v4's "Runtime parsing is unchanged" HOLDS, verified from the hunks.** Each is a one-token deletion, no other change in any of the four files; Zod 4.6 changed what `.optional().prefault(x)` *infers* (`T \| undefined` where 4.5 gave `T`), which broke `tsc`, and `.prefault()` already accepts a missing key. **v5's four ported twins already encode the post-change semantics and need NO change (measured):** `isDefault` → `api/types.rs:553-554` (`#[serde(default)] is_default: bool`, handler `api/characters.rs:2321`, dispatch `api/engine.rs:2217-2234`); `count` → `api/image_profiles.rs:817-818` (a hand `match … None => 1`, wire decode at `api/types.rs:1100-1101` deliberately a plain `#[serde(default)] Option<i64>` rather than the `double_option` tri-state its six siblings use — P4.98's deliberate shape); `allowAnyCharacter` → `api/projects.rs:243-246`; `characterRoster` → `api/projects.rs:247`. The optional-but-not-nullable distinction is **already pinned** by `the_prefaulted_fields_are_not_nullable` (`api/projects.rs:2035-2042`) and `projects_routes_equivalence.rs:655`, and the `count` prefault by `quilltap-web/tests/dispatch_wrong_type_census.rs:832,843`. **The fifth site — `app/api/v1/plugins/route.ts` — has NO v5 counterpart**: v5 has no plugin runtime (standing deferral, `services/tools_inventory.rs:23`), no `/api/v1/plugins` route, no `Plugin*` Request variant. **NO-COUNTERPART, measured.** <br><br>**(3) `public/schemas/plugin-manifest.schema.json`'s regenerated email pattern is INERT for v5, twice over — measured.** (a) The file is **not vendored in v5**; the two vendor guards cover only `qtap-custom-tool`/`qtap-progression` (`public_schemas_vendor_guard.rs:46,52`) and `qtap-export` (`qtap_schema_embed_guard.rs:3`). (b) The rewrite is **semantically equivalent** — the 4.5 lookahead form `^(?!\.)(?!.*\.\.)(…)` and the 4.6 `^(?:[A-Za-z0-9_'+\-]+\.)*…` form **agreed on all 596 sampled inputs**; the `(?:X+\.)*X*` shape structurally forbids the leading dot and the `..` the lookaheads forbade. ⚠ **But the measurement banked a PRE-EXISTING finding on the way:** v5's only `z.email()` port, `is_zod_email` at `crates/quilltap-core/src/api/user_profile.rs:161-179` ("Zod v4's `z.email()` regex, transcribed"), is a loose hand-rolled rule — *any* non-`@`/non-whitespace local part — that **disagrees with zod on 204 of those 596 inputs** (it accepts `'''@b.co`, `''.@b.co`, every local ending in `.`, …). **It disagrees with 4.5.4 equally, so this commit did not cause it and it is not this row's obligation** — bank it as a v5-side finding for a follow-up (readers: `db/users.rs:52`, `api/types.rs:2538`, shape test `user_profile.rs:364`). <br><br>**⭐ (4) THE REBUILT PLUGIN BUNDLES CARRY A REAL WIRE-DECODE CHANGE, and v5's stream-fixture oracle drives exactly it.** All four SDK-bundling plugins gained a terminal-SSE **flush arm** in `iterSSEChunks` — measured by diffing `6b0615807^:plugins/dist/…/index.js` against `6b0615807:` — `if (signal.aborted) return; const pending = sseDecoder.flush(); if (pending) yield pending;`, **present in the NEW bundle and absent from the OLD one for openai, openrouter, anthropic AND google** (all four). That is openai-node 7.17.0's *"fix(streaming): emit terminal SSE events missing a trailing blank line"* (#2726); the `LineDecoder` buffer-realloc path moved with it. **`harness/oracle/providers/record-stream-fixtures.mjs` drives v4's REAL plugin `streamMessage` generator over committed wire transcripts through the SDK's own parser**, so this is not theoretical. **The at-risk rows are identified: 6 of 33 committed SSE transcripts end with `data: [DONE]\n` and NO trailing blank line** — `harness/oracle/fixtures/streams/chat_completions_sse/nanogpt-cache-{no-usage,openai-dialect,write-only,clamp,anthropic,read-zero-present}.wire`. **Reasoning says they stay neutral** (`[DONE]` is the SDK's end-of-stream sentinel either way) **— but that is reasoning, and the house rule is to measure: re-record `stream_decoders_equivalence` at the target pin and compare, red-first, rather than asserting neutrality.** **The nine `harness/oracle/providers/record-*.mjs` recorders are the full blast radius** (`record-stream-fixtures`, `-request-envelopes`, `-response-bodies`, `-tool-wire`, `-image-fixtures`, `-google-request`, `-moderation-wire`, `-web-search-wire`, `gen-provider-manifests`) — all load the real `plugins/dist/*` bundles and all must be re-run at the target. **Insulating facts, measured: no base URL, auth header, `HTTP-Referer`/`X-Title`, `anthropic-version`, `prompt_cache_key`, `safety_identifier`, `reasoning_effort`, `verbosity` or `service_tier` string moved in any of the four bundles**; `gen-provider-manifests.mjs` carries no plugin version; and `record-request-envelopes.mjs:812-814` **deliberately refuses** to record SDK-version-specific zod dumps, so the envelope corpus is insulated by construction. <br><br>**⭐ (5) THIS COMMIT RESOLVES THE STANDING `openai 7.10.0` ITEM — and the standing note's count was wrong.** See §1's retirement bullet: **six** dirs bundle openai, not four, and they were at **two different versions** (the old `qtap-plugin-openai` bundle carried `VERSION = "7.10.0"`, the old `qtap-plugin-openrouter` bundle `"7.15.0"`); everything now matches its declaration. <br><br>**(6) Non-code / stamp files → NO-PORT candidates**, the standing class: `README.md`, `docs/CHANGELOG.md`, all seven `package.json`/`package-lock.json` pairs, `packages/plugin-utils/src/version.generated.ts`, and the 15+15 plugin `manifest.json`/`package.json` bumps — **v5 pins no plugin manifest or package version (measured:** v5's manifests carry only their own `"schemaVersion": 1`, gated at `provider_manifest/mod.rs:449-450`, and the 16 v4-plugin-version mentions in `crates/` are all provenance doc comments). Tier R at the pin for the `packages/quilltap` stamp. | UNPROCESSED |
| `da9c4f34f` | 2026-09-21 | fix(salon): a scenario is not a summary, and must not greet from one (bug 158) | **PORT** | **24 files, +1,082/−30, `4.10.0-dev.55`. NOT a convergence** — `bugs/fixed/bug-158-scenario-masquerades-as-summary.md` carries **no** v5/port attribution (grepped for `quilltap-v5`/`v5 port`/`the port`/`native port`/`rust port`/`filed by`: zero hits); its Found row reads *"2026-09-21, live on the `Friday` instance, chat `6cd2068d-…`"* — a v4-side operator finding, fixed the same day. **Severity High.** <br><br>**⭐ (1) v5 REPRODUCES THE BUG — MEASURED, file:line, re-verified at this check.** `crates/quilltap-core/src/services/chat_create.rs:1419` writes `"contextSummary": resolved_scenario.clone()` into the create object, **eight lines above `:1427`'s `"scenarioText": resolved_scenario.clone()`** — the exact double write v4 deleted at `app/api/v1/chats/route.ts:1268`. v5 has **no** `scenario_seeded_summary` predicate and nothing anywhere that distinguishes a seeded column from a folded one. **So v5 is still MINTING the damage on every chat it creates.** <br><br>**⚠ (2) THE BANKED POPULATION PROOF HAS ALREADY EXPIRED — measured 2026-09-21 on a read-only copy of live Friday (since deleted).** v4's own body records *186 of 712* chats seeded and *526 real summaries* left standing after its migration. Measured now: **971 chats, 526 with a `contextSummary`, and ZERO matching the seeded predicate.** v4 ran `clear-scenario-seeded-chat-summaries-v1` on the live instance, and the 526 matches v4's sentence exactly. **§5.5 fired as designed: do not plan a dogfood row around finding the 186 — they are gone.** The replacement proof is better and is available live: **v5 still mints a seeded summary**, so create a chat on a scenario in v5 and read the greeting prompt's Recent Conversations block. <br><br>**⭐ (3) v5 REPRODUCES THE SECOND HALF TOO — the greeting block, verbatim pre-fix.** v5's `build_recent_conversations_block` lives at **`services/chat_create.rs:2977`** (NOT in `memory_recap`), and at `:3025-3035` it renders `format!("#### {title} (\`{id}\`)\n{summary}")` from the raw `contextSummary` and returns `format!("### Recent Conversations\n\n{}", …)` — **uncapped, unframed, with no empty-gist arm and no closing note**, byte-for-byte v4's pre-fix shape (read verbatim at this check). The port adds v4's three changes: the `gist.trim()` empty arm (heading only), `truncate_gist(gist)`, and the `\n\n{READ_CONVERSATION_CALL_NOTE}` tail. **Both halves already exist in v5 but in the OTHER module:** `READ_CONVERSATION_CALL_NOTE` is `pub(crate)` at `services/memory_recap/mod.rs:131` and `truncate_gist` is a **private `fn`** at `:153` (UTF-16-faithful, `js_trim`/`utf16_len`/`utf16_truncate`), already used by the per-turn sibling at `:530` as `truncate_gist(gist, 280)` — matching v4's `truncateGist(text, maxChars = 280)` default. **So the port is a visibility widening plus three lines, not a new implementation.** ⚠ v5's helper carries a RECORDED DIVERGENCE block at `:2984-3002` (P4.90's `:692` warn, re-recorded at the `2075242f9` round) and a `tracing::warn!` arm at `:3010` v4 has no twin for — **preserve both; this commit does not touch them.** <br><br>**⭐ (4) v4's new empty-gist arm is PROOF about the `$exists` filter, and it retires a v5 question.** v4 added `gist.length > 0 ? … : "#### title (id)"`, which is only reachable if `findRecentSummarizedByCharacter`'s `$exists: true` admits rows whose `contextSummary` is `''` or whitespace. v5's `db/chats_read.rs:412` renders that filter as **`contextSummary IS NOT NULL` only, with no empty-string check** (doc at `:409-411`; callers `chat_create.rs:3004` and `memory_recap/mod.rs:486`). **v5's rendering is therefore right and the new arm is genuinely reachable on both sides** — exactly the corpus row to write red-first. <br><br>**(5) The three Concierge sites — v4's "Behavior unchanged" is TRUE FOR THE INPUT TEXT and FALSE IN TWO MEASURABLE WAYS, from the hunks.** (a) `handlers/chat-danger-classification.ts` (+20) inserts an `else if (chat.scenarioText)` arm between the summary arm and the raw-messages fallback and widens `inputSource` to `'summary' \| 'scenario' \| 'messages'` → **v5 `services/dangerous_content/gatekeeper_job.rs:105`, branch `:155-180`**; (b) `scheduled-danger-scan.ts` (+18) changes the first branch to `if (chat.contextSummary \|\| chat.scenarioText)` → **v5 `services/danger_scan.rs:185`, three-way branch `:242-270`, `has_summary` at `:232-236`**; (c) `memory-trigger.service.ts` (+11) changes the gate to `if (!chat.contextSummary && !chat.scenarioText) return` → **v5 `services/message_finalizer.rs:1522`, gate `:1564-1572`**. ⚠ **The two ways "unchanged" is false:** *(i)* the reported source moves `'summary'` → `'scenario'`, but **v5 has NO `input_source` at all** (measured: zero hits across `crates/`; `gatekeeper_job.rs` has zero `tracing::` calls; the persist at `:290-321` writes only the five danger columns; `quilltap-host/src/spine.rs:2782-2814` collapses the result to `JobOutcome::Completed(None)`) — so which input was used is **unobservable in v5**, and the port must decide whether to add the discriminant or record the absence as a divergence; *(ii)* a chat with a `scenarioText` but **never** seeded — one arriving by import/restore from a fixed instance, or one the migration cleared — previously fell through to raw messages (or skipped entirely at gate (c)) and now takes the scenario arm. **That is a genuine behaviour change on real data, and it is the whole reason v4 kept the bootstrap. Port the arms; do not port the prose.** <br><br>**(6) The shared predicate and its three consumers.** NEW `lib/chat/scenario-seeded-summary.ts` (+53): `isScenarioSeededSummary` = *`scenarioText` is a non-empty string AND `contextSummary === scenarioText`* (byte equality is the whole test), and `stripScenarioSeededSummary<T>` nulls `contextSummary`, **never touching `scenarioText`**. Consumers: `import-entities.ts` (+10, **both** create sites — the `(imported)`-title duplicate branch and the preserveIds branch) and `backup/restore/restore.ts` (+8). **v5 homes:** the predicate wants a new module beside `chat_create.rs`; the import is **`services/quilltap_import/entities.rs`** — note v5 **collapsed v4's two call sites into ONE** via the `DUPLICATE_MINTS = ""` sentinel at `:695-699`, single `create_chat(…)` at `:700-707`, repo `create` at `:765-772` with `mint_or_preserve` at `:764` — so **one strip covers both of v4's sites**; the restore is **`services/backup/restore/orchestrator.rs:464`** (section 7, loop `:468`, `chats.create` at `:481-484`). <br><br>**(7) The migration is the interesting one, because v5's runner is deferred.** `migrations/scripts/clear-scenario-seeded-chat-summaries.ts` (+131) + `index.ts` (+5) + the `prettify.ts` label. Its SQL predicate is `"contextSummary" IS NOT NULL AND "scenarioText" IS NOT NULL AND "scenarioText" <> '' AND "contextSummary" = "scenarioText"`, **held as one string so the count and the UPDATE cannot disagree**; `dependsOn: ['sqlite-initial-schema-v1', 'add-chat-scenario-text-field-v1']`; `shouldRun` returns `countSeeded > 0`. v4's file comment states the rule the port must keep: **"The migration states the same rule in SQL … The two must agree — change both or neither."** v5 has no migration runner (a locked deferral), so v4 migrations are re-homed as boot repairs in `seed_built_ins` (`crates/quilltap-host/src/host.rs:1084`, called from `assemble` at `:501`). ⚠ **This is a DATA-only pass with no column or index to key on, so it takes the `migrations_state` ledger-guarded idiom** (precedent `host.rs:1222-1228`/`:1250-1254`), **not** a column-presence ensure. Closest precedents: the bug-132 placeholder heal (`host.rs:1545-1562`, `db/generated_image_placeholder_heal.rs`) and the avatar-roll collapse (`host.rs:1481-1527`, `db/avatar_rolls_collapse_heal.rs:90`). Ledger helper: `db/migrations_ledger.rs`. ⚠ **And note seam (2): on the live instance the heal will now find ZERO rows — its proof must come from rows v5 itself minted.** <br><br>**(8) The differential families — red-by-design vs green-by-luck.** **Will move (regenerate at the target, do not "fix" v5 back):** `chat_create_capstone_equivalence` (its column list at `:503` names `contextSummary`/`scenarioText` explicitly; oracle `chat-create-capstone.test.ts`); `initial_greeting_equivalence` carries `recent_conversations_block` (`:65`, `:263`) and moves **only if a case has a non-empty block** — check first, grow the corpus if not. **Green-by-luck, so GROW red-first:** `danger_trigger_equivalence` — its `skips_when_no_context_summary` (`:71`) and `skips_when_empty_context_summary` (`:72`) cases carry **no `scenarioText`**, so they still skip under v4's new gate and prove nothing; the port owes scenario-bearing arms on all three Concierge sites — plus `danger_scan_tier2_equivalence` and `danger_gatekeeper_tier3_equivalence`. **Check for incidental movement** (all carry `contextSummary`): `salon_mutations_equivalence`, `courier_images_routes_equivalence`, `context_summary_service_tier3_equivalence`, `orchestrator_tier3_equivalence`, and `salon_reads_equivalence` for the chat-GET projection. **The ingest strips need seeded rows planted in the bundle fixtures** or `qtap_import_equivalence` / `system_import_equivalence` / `system_import_state` / `system_restore_equivalence` / `system_restore_state` prove nothing. <br><br>**(9) The three test files are the corpus sources:** `route.scenario-seed.test.ts` (+297, new), `scenario-seeded-summary.test.ts` (+79, new), `memory-recap.test.ts` (+97 — v4's own body notes this file **had no coverage of the greeting block at all**). <br><br>**(10) Docs + help + stamps.** `help/dangerous-content.md` (+11, three hunks) → a re-vendor obligation, counted in §1's nine. `DDL.md` (+1/−1) annotates `chats."contextSummary"` — **a comment only, NO DDL change, D23 does NOT fire.** `bugs.md` (+1) and the NEW `bug-158-…md` (+218) → the `docs/v4/` mirror. `README.md`, `docs/CHANGELOG.md` and the three stamps → NO-PORT candidates. <br><br>**💸 The dogfood queue gains the richest row in months** — and seam (2) reshapes it: the population proof is gone, so **open a new chat on a scenario in v5 and read the greeting prompt's Recent Conversations block live** (v5 can reproduce the reported failure *today*, the one proof a canned corpus cannot give), then run the boot heal against rows v5 minted and re-count, then a `.qtap` round-trip of a pre-fix bundle to prove the strip. | UNPROCESSED |
| `186eb09cb` | 2026-09-21 | perf(storage): store images and bulky text at their real size (bugs 159, 160) | **PORT — ⛔ BLOCKING** | **45 files, +2,592/−38, `4.10.0-dev.59`. This is the most dangerous drift this port has faced, and it is dangerous in a way no previous row has been: it changes the BYTES IN COLUMNS v5 READS, and v5 breaks SILENTLY on them. Not a convergence** — `bugs.md` gains 159 and 160 *in this commit*, both "Owed" in the assessed column; neither is on the port's filing list. **⚠ The commit body is accurate but radically understates the port's exposure: "reading, searching and exporting are unchanged" is true of v4 ONLY, and false of v5 — §5.3 in its most expensive form yet.** <br><br>**⭐ (1) The codec — a new on-disk format, stated byte-for-byte (read from the file).** NEW `lib/database/text-compression.ts` (+154), modelled on `lib/embedding/float32-conversion.ts`. A value is stored as a **3-byte-header BLOB**: `[0] magic = 0x51` (`'Q'`, chosen distinct from `0xEB`, the embedding magic), `[1] version = 0x01`, `[2] codec = 0x01` (brotli), `[3..]` payload. Compression is **Node `zlib.brotliCompressSync`** with `BROTLI_PARAM_QUALITY = 5` (`TEXT_COMPRESSION_QUALITY`) and `BROTLI_PARAM_SIZE_HINT = raw.length`. **`textToBlob` returns the ORIGINAL STRING** — not a blob — when the value is under `TEXT_COMPRESSION_MIN_BYTES` (**512**) or when `compressed.length + 3 >= raw.length`, **so a column holds a mix of plaintext and BLOBs indefinitely and by design.** `isCompressedTextBlob` checks all three header bytes. `blobToText` is total: `null`/`undefined` → `null`, string → itself, non-Buffer → `String(value)`, unrecognised Buffer → `toString('utf-8')`, and a brotli failure falls back to `payload.toString('utf-8')` rather than throwing. **v5 has no counterpart module; `brotli` is in `Cargo.lock` only transitively (axum/tower-http), never as a direct dependency.** <br><br>**(2) Registration — three columns, two partitions, two different homes.** `json-columns.ts documentToRow` gains a 4th `compressedColumns` param whose branch runs **FIRST** (before the BLOB and JSON branches) because `llm_logs.request` is *both* a JSON column and a compressed one. `query-translator.ts` gains the same param plus **`rejectJsonOperator`, which THROWS on `$push`/`$pull`/`$addToSet` against a compressed column**. `backend.ts` (+60) threads a 7th ctor arg through all six write paths and — load-bearing — puts the **decode branch first in `rowToDocument`**, with a `Corrupted JSON in compressed column` warn, explicitly because a compressed value would otherwise fall into the JSON branch and, being a Buffer, **be decoded as a Float32 embedding**. Registered in exactly two places: `lib/database/manager.ts:118+` → **`conversation_chunks.content`** (main), and `llm-logs.repository.ts` → `LLM_LOG_COMPRESSED_COLUMNS = ['request','response']`, which bypasses the manager entirely. **`chat_messages` is deliberately untouched — for now (see (12)).** **No `lib/schemas/**` file moves and no column type changes (SQLite is dynamically typed; the columns stay `TEXT`), so v4's live `generateDDL` is unmoved and THIS commit owes NO D23 re-dump.** <br><br>**(3) The `qt_text()` UDF — six registration sites.** NEW `backends/sqlite/text-codec-function.ts` (+45) registers `qt_text` via `db.function(name, {deterministic:true}, blobToText)` immediately after `applySqlcipherKey` on **`client.ts`, `child-client.ts`, `llm-logs-client.ts`, `mount-index-client.ts`**, plus **`migrations/lib/database-utils.ts`** and **`packages/quilltap/lib/db-helpers.js`**. Its doc states the failure mode as a feature: a connection that misses it gets `no such function: qt_text` *"rather than silently returning wrong answers"*. Exactly **two** raw-SQL sites are rewritten: `llm-logs.repository.ts USAGE_AGGREGATE_COLUMNS` → `json_extract(qt_text("response"), '$.error')`, and `lib/startup/reconcile-conversation-rendering.ts` → three `LENGTH(qt_text(cc."content"))`. <br><br>**⛔ (4) v5's exposure — MEASURED, and it is the headline. See §1's blocking bullet for the live-data measurement.** v5 binds `rusqlite 0.37` workspace-wide with `default-features = false, features = ["buildtime_bindgen"]` (`Cargo.toml:54`) and **no crate enables `functions`** (grepped all ten declarations), **so v5 has ZERO UDF machinery and `qt_text` is not currently compilable** — that is a workspace `Cargo.toml` change before a line of Rust. `String` and `Option<String>` both reject a BLOB with `Err(InvalidColumnType)`, and **every v5 read site of the three columns binds one of the two.** `llm_logs`: **`db/llm_logs.rs:1187-1188`** (`let request_json: String = row.get(9)?; let response_json: String = row.get(10)?;`, re-read at this check) feeds all eight Inspector reads, and **both callers treat the `Err` as v4's Zod-drop** — `:648` `Some(row) => Ok(map_log_row(row).ok())` (→ **404 on every log**) and `:780` `if let Ok(log) = map_log_row(row)` (→ **every list returns `[]`**). `services/backup/collect.rs:336-355` declares both as `F::Json`, whose `marshal.rs:87-95` binds `Option<String>`, and the error is swallowed at `collect.rs:627` `.unwrap_or_default()` → **a v5-taken backup of a migrated instance silently contains ZERO llm_logs rows — data loss wearing a successful exit code.** `conversation_chunks.content`: five `String` binds — `db/conversation_chunks.rs:306` (`find_all_with_embeddings`; swallowed at `tools/search.rs:370` → **conversation search silently returns nothing**), `:446` (every `CONVERSATION_CHUNK` embed job fails), `:206` via `marshal_cc_row` (`find_by_interchange_index` is **called from inside `upsert` at `:600`, so the render WRITE path fails on its own read**), and `services/backup/collect.rs:779`. **Raw SQL needing `qt_text` in v5, six sites, two of them silent-wrong rather than erroring:** `almanack/phase6_wire_records.rs:124` and `:184` (`json_extract("response", '$.error')` — v5 carries v4's pre-fix expression verbatim), **`db/conversation_chunks.rs:537` (`trim(content) != ''`)** and **`services/conversation_render_reconcile.rs:122,139,140` (three `LENGTH(cc."content")`)** — those four would never announce themselves. BLOB-*tolerant* sites that answer wrongly: `tools/run_sql.rs:356-358` (renders `<blob: N bytes>`), `crates/quilltap-cli/src/nodefmt.rs:39-56`, and **`crates/quilltap-fixture-sanitizer/src/lib.rs:241-243,333-340` — a compressed payload passes through `scrub_blob` and is no longer scrubbed as text, i.e. a sanitizer leak.** A UDF must be registered at **three distinct open paths, none of them shared**: `db/runtime.rs:325-333 open_readonly` (after the `PRAGMA key` line — the highest-value single point, feeding all three partitions), `db/mod.rs:238-251 Writer::open_writable`, and `crates/quilltap-cli/src/dbopen.rs:14-30`. <br><br>**(5) The image half (bug 159) — normalization moves INTO the blob funnel, and v5's decliner count matches v4's exactly.** `doc-mount-file-links.repository.ts` renames its parameter to `rawInput` and calls **NEW `lib/mount-index/normalize-blob-image.ts` (+92) BEFORE the sha256 is computed**, so the stored hash describes the bytes that land; `data`, `sha256`, `storedMimeType`, `relativePath` and `fileName` are rewritten together. `LinkBlobInput.normalizeImages?: boolean` defaults true; **the only `false` in the tree is `import-document-stores.ts`** (byte fidelity for `.qtap` import and archive rehydrate). `blob-transcode.ts` (+79) adds **`isLosslessWebP`**, which walks the RIFF chunk list (`RIFF` at 0, `WEBP` at 8, then fourcc + u32 LE size padded to even, returning true on `VP8L`) rather than reading the first fourcc, because a `VP8X` file hides it behind `ICCP`/`ANIM`/`ALPH`; gains **`LOSSLESS_WEBP_REENCODE_MIN_BYTES = 512 * 1024`**; normalises the mime before the set lookup; and adds a `Transcoded blob to WebP` debug line with `reason: 'lossless-webp-reencode' \| 'bitmap-transcode'`. **v5's funnel is `db/doc_mount_file_links.rs:922 link_blob_content` / `:929 …_with_ids`** (sha recomputed `:938-942`, INSERT `:1012-1016` writing `input.data` and `input.stored_mime_type` **verbatim**); `LinkBlobInput` at `:467` has **no normalize flag** and its `data` doc still reads "Already-transcoded bytes" — v5 reproduces pre-fix v4 exactly. **v5's declining call sites number EIGHT, matching v4's claim:** `db/doc_mount_blobs.rs:308`, `tools/doc_edit/blob.rs:243`, `services/quilltap_import/document_stores.rs:573`, `photos/character_gallery_service.rs:330`, `photos/save_image_to_album.rs:399`, `photos/user_gallery_service.rs:542`, `services/image_job_storage.rs:267`, `services/mount_index/file_ops.rs:438`. ⚠ **Two existing `transcode_images: bool` flags (`store_file.rs:72`, `file_storage.rs:1119`) look like the natural `normalizeImages` landing site but are NOT: no caller sets either to `false`, their passthrough branches are dead code, and — load-bearing — the `.qtap` import and archive paths do not go through them at all, they go through `DocMountBlobsRepository::create_with_ids`, which has no flag.** v5's transcode logic is **ported twice** (`services/mount_index/blob_transcode.rs` trait `WebpTranscoder` `:35`; `services/file_storage.rs` trait `PixelCodec` `:325`) with `normalise_blob_relative_path` in **five** copies — so v4's one-chokepoint move has **five** de-duplication targets, not one. The bug-151 seam is `files/image_processing.rs:70` (`shrink_to_webp` `:117`), implemented by `quilltap-host/src/image_codec.rs:172-191`, **and `HostImageCodec` already implements four distinct image seams** (`:80`, `:108`, `:233`, `:249`) — a fifth must fit there. **v5 has ZERO WebP chunk-level logic**: `VP8L`/`VP8X`/`ICCP` return no hits, every `RIFF` hit is a test fixture, the host encoder (`image_codec.rs:71`) uses the `webp` crate's **lossy** encoder only, and **no `512 * 1024` / `524288` constant exists in the repo**. <br><br>**⭐ (6) Bug 160 — MEASURED: v5 HAS the same omission, verbatim.** v4 adds `compiledIdentityStacks = NULL` to the nightly collapse's `chats` UPDATE and to its guard disjunct. **v5's `services/collapse_stale_chat_caches.rs:98-103` is the twin and lists exactly `compressionCache` and `renderedMarkdown`** (re-read at this check) — `compiledIdentityStacks` is absent, and absent from the file's "NEVER touched" list at `:27-28` too, **so it is a plain omission rather than a ruled exclusion** (the `chat_messages` UPDATE at `:106-115` already carries all five of v4's columns). The column is a version-stamped read-through cache in v5 as in v4 — written `services/system_prompt_compiler.rs:269` via `db/chats.rs:1147-1157`, read `db/chats_read.rs:296` (column index 67) and `system_prompt_compiler.rs:215`/`:366`, with strict version equality — so nulling it is safe and one recompile. **Families that will notice:** `identity_compiler_equivalence`, `subprompts_prompt_tier2`, `subprompts_storage_tier2`, `chat_cast_routes`, `chat_admin_routes`, `chat_scenario_routes`, and the direct `collapse_stale_chat_caches_tier2`. <br><br>**(7) Three new migrations + the prettify labels.** `recompress-oversized-mount-blobs-v1` (+322, ONE-WAY: re-encodes untranscoded bitmaps and `VP8L` WebP, updating `doc_mount_blobs.{data,sha256,sizeBytes,storedMimeType}` + `doc_mount_files.{sha256,fileSizeBytes}` + **`files.sha256` in the MAIN db joined on the OLD hash**, preserving bug 117's invariant, and **deliberately leaving `relativePath` alone** so stored Markdown keeps resolving — "the extension becomes cosmetically wrong and nothing else"), `compress-llm-log-payloads-v1` (+214) and `compress-conversation-chunk-content-v1` (+149), both `BATCH_SIZE` 250 / `SAMPLE_LIMIT` 50. **v5's migration runner stays deferred — which is exactly why this row is dangerous rather than merely large: v5 never *performs* the change, but v4 running once on a shared instance performs it FOR v5, and the two compression migrations are explicitly "byte reclamation, not a correctness prerequisite" because NEW WRITES ALREADY COMPRESS. There is no migration gate to sit behind.** The image migration needs a v5 boot-heal decision of the kind earlier work got; the two text ones are pure reclamation and could be deferred indefinitely *once v5 reads the format*. <br><br>**(8) The CLI half — cheaper than it looks. MEASURED.** NEW `packages/quilltap/lib/text-codec.js` (+61) is a hand-maintained **mirror** of the codec (*"If the header or codec changes there, change it here in the same commit"*); `db-helpers.js` registers `qt_text`; `db-commands.js cmdLog` decodes before printing. **v5's CLI ships NEITHER affected verb**: `crates/quilltap-cli/src/db_cmd.rs:30` lists `"log"` in `DB_VERBS` but `:279-285` answers *"recognized but not yet available in this build"*, and `--repl` likewise at `:121-125`. **Tier R has no `db log` case at all** — the only `llm_logs` cases (`cli_differential.rs:1885`, `:1895`) run against a synthetic two-column table built at `:562-567` with no `request`/`response`. **So ZERO Tier R cases move.** The one live v5 surface is the **raw-SQL path** (`db_cmd.rs:105-111`), whose open is `dbopen.rs:14-30` — the direct home for a `qt_text` registration, and the only CLI work this commit forces. <br><br>**(9) Docs + help.** `DDL.md` (+69) gains a whole "Compressed text BLOB format" section with the byte layout, the registered-column table, the `qt_text` rule and two worked examples → the `docs/v4/` mirror. `bugs.md` (+2) and the NEW `bug-159-…md` (+125) / `bug-160-…md` (+79) → the same mirror. NEW `docs/developer/features/chat-message-fts5-and-compression.md` (+439) → `docs/v4/developer/features/`. **`help/data-retention.md` (+24/−1) — v5's copy DIFFERS, the ninth re-vendor obligation.** `README.md` and the stamps → NO-PORT candidates. <br><br>**(10) The eight test files (+481) are the corpus sources:** `text-compression.test.ts` (121), `compressed-columns.test.ts` (171), `blob-transcode-lossless-webp.test.ts` (89), `normalize-blob-image.test.ts` (89), plus four small edits. <br><br>**⚠ (11) THE FIRST MEASUREMENT THE PORT OWES, before any comparand is designed: is Node `zlib.brotliCompressSync(q=5, SIZE_HINT)` BYTE-IDENTICAL to the Rust `brotli` crate at the same parameters? NOT VERIFIED.** Both descend from the reference encoder and *should* agree, but if they do not, every tier-2 byte-diff over a compressed column goes red even when the decoded text matches — and the differ must then normalize by **DECODING** rather than comparing raw cells, which is a harness change, not a core change. **This is the cheapest experiment in the row and it gates the design.** Three more inferences to settle with it, none executed: `json_extract(<brotli blob>)` raising `malformed JSON` (BLOB args have been JSONB since SQLite 3.45 — v4's own body asserts the same); `trim(<blob>) != ''` evaluating always-true and `LENGTH(<blob>)` counting compressed bytes; and `rusqlite`'s `FromSql for String` rejecting `ValueRef::Blob`. **A three-line test settles all four together.** <br><br>**(12) A LATENT v4 GAP to carry forward rather than reproduce**, recorded in the FTS plan (§2.2) and confirmed in the hunks: **a write made inside `withTransaction` bypasses the codec entirely** — `SQLiteBackend` threads `compressedColumns` into `getCollection` but the transaction-scoped collection path does not receive it. v4 calls it latent (only `users.repository.ts` uses transactions today) and defers the one-argument fix to PR-2. **v5's write path is a partitioned applier rather than a `withTransaction` collection, so the class may be N/A — measure before assuming; if it is N/A, say so in a comment naming this sha.** <br><br>**(13) Predicted redness at the baseline move, with two measured mitigations.** Directly named: `conversation_chunks_tier2`, `llm_logs_tier2`, `llm_logs_routes`, `doc_mount_blobs_tier2`, `doc_blob`, `collapse_stale_chat_caches_tier2`; broader, 12 harness files reference `conversation_chunks` and 28 reference `doc_mount_blobs`. **Mitigation (a): `jest.setup.ts:379` mocks the LLM-logging service, so jest oracles write ZERO `llm_logs` rows** — the 168 files naming `llm_logs` are overwhelmingly whole-partition dumps of an empty table, and those comparands should be largely UNMOVED. **Mitigation (b): `sharp` is NOT mocked, so the image half's oracle side is real and `doc_mount_blobs` comparands WILL move** wherever the corpus bytes are genuinely decodable, with `sha256` propagating into `doc_mount_files.sha256` and `files.sha256`. `conversation_chunks.content` moves wherever a rendered chunk clears 512 bytes — and because `harness/oracle/lib/tier2.ts:37-40` hexes a Buffer, **the diff reads as hex-vs-prose: loud and unmistakable, which is the good news in this paragraph.** ⚠ **Finally, a REBUILD-triggered trap, not a baseline-move one: 106 committed fixture DBs sit in `crates/quilltap-web/tests/fixtures/`, and the `-mount.db` half holds real blob bytes — any lane that rebuilds a fixture pair at the new pin silently re-encodes its images.** Call it out in the round plan beside the existing ten-family fixture-vintage heal order. | UNPROCESSED |
| `80a05a4c8` | 2026-09-21 | docs: check the message FTS5 + compression plan against the code | **NO-PORT?** | **Docs-only — MEASURED, exactly two files: `docs/CHANGELOG.md` (+12) and `docs/developer/features/chat-message-fts5-and-compression.md` (+421/−173, rewritten in place). No `lib/`, `app/`, `components/`, `packages/`, `help/`, `public/schemas/`, `migrations/` or test hunk. `4.10.0-dev.59`, unmoved from `186eb09cb`.** Expect **NO-PORT-RATIFIED *with a mirror*** — v5 mirrors `docs/v4/developer/features/` and this is the design of record for the next escalation. `docs/CHANGELOG.md` is v4's own changelog: NO-PORT. <br><br>**⚠ But this row is NOT informational filler — it is the port's advance warning, and its corrections change what v4's NEXT commit will do to v5.** v4 records: **(a)** the FTS index was keyed on `chat_messages`' implicit rowid, which VACUUM may renumber and a table rebuild silently reassigns while dropping the triggers — it is now keyed through **a small id-mapping table with an explicit INTEGER PRIMARY KEY**, plus a startup guard that rebuilds a stale index (**a new table → a future D23 re-dump**); **(b)** two migrations, one per PR, with the update trigger comparing *decoded* text so the compression pass leaves the index untouched; **(c)** *"`qt_text()` is already registered on every connection; stated as fact"* — **v4 now treats the UDF as ambient infrastructure, which is precisely the assumption v5 cannot satisfy**; **(d)** today's search emits a plain LIKE, applies the 100-row cap in JS *after* fetching every match, passes every chat the user owns, **and its regex escaping makes a query containing a period silently return nothing** (an un-numbered v4 defect the port should measure against its own search); **(e)** contentless FTS5 tables offer no `snippet()`, so a **token-prefix locator in JS** is required; **(f)** two gaps added to PR-2 — **transaction-scoped collections skip the codec** (cf. `186eb09cb` seam 12) and **the CLI's `db messages`/`db message` print the columns raw**; **(g)** size figures predate the 4.10 storage work, projected landing ~1.3 GB; **(h)** `help/search.md` uses a wildcard url and has no In-Chat Navigation section. The plan's Part A is marked **BUILT, v4.10** — **this commit is the bridge between what landed and what is in v4's working tree right now, and it says plainly that `chat_messages.content` is next.** | UNPROCESSED |
| `f45a517a9` | 2026-09-21 | feat(search): index chat messages with FTS5, compress the transcript | **PORT-NEW + PORT — ⛔ BLOCKING; it ESCALATES `186eb09cb` from a silent READ failure into a hard WRITE failure** | **33 files, +2,351/−72, `4.10.0-dev.61`, v4 main HEAD at this check. This is the commit the `80a05a4c8` row warned was coming, and it is worse than that warning: `186eb09cb` made v5 read three columns wrongly; this one makes v5 unable to WRITE A SINGLE CHAT MESSAGE to a migrated instance. Not a convergence** — `docs/developer/bugs.md` **did not move** in this commit (measured: the path is absent from `--stat`), so no numbered bug closes and nothing is credited to this port. It does fix an **un-numbered** defect that **v5 reproduces and PINS WITH A PASSING TEST** (seam 6). <br><br>**⭐ (1) The FTS5 schema — five new objects — and D23 does NOT fire. Measured twice over.** NEW `lib/database/backends/sqlite/chat-message-fts.ts` (+295) is the single source of truth: `chat_messages_fts_map` (`"ftsId" INTEGER PRIMARY KEY, "messageId" TEXT NOT NULL UNIQUE`), the contentless virtual table `chat_messages_fts USING fts5(content, content='', contentless_delete=1, tokenize='unicode61 remove_diacritics 2')`, and the three triggers. **Contentless is the load-bearing choice** — the index never reads the base column, so it is indifferent to the encoding, but `snippet()`/`highlight()` are unavailable (snippets move to JS, seam 7) and `VALUES('rebuild')` is REFUSED, so the rebuild deletes and re-inserts by hand, keyset-paginated by rowid, `REBUILD_BATCH_SIZE = 500`. The map table exists because `chat_messages` is `"id" TEXT PRIMARY KEY` → implicit rowid, which `VACUUM` (i.e. `npx quilltap db optimize`) may renumber. **Why D23 does not fire, stated by v4 itself:** `migrations/scripts/create-chat-message-fts.ts` says the objects are created in the migration *"rather than in `ensureCollection`: that path only runs the Zod-derived `CREATE TABLE IF NOT EXISTS` and knows nothing of triggers or virtual tables"* — so v4's live `generateDDL` emits none of it and `fresh_schema.json` owes **no re-dump**. Confirmed on the v5 side: the D23 dumper `harness/oracle/provision/dump-fresh-schema.ts:57-73` keeps only `type === 'table'` and `type === 'index'` rows, **so triggers are filtered out by construction**, and the committed file has 0 `TRIGGER` / 0 `VIRTUAL` / 0 `fts`. **The consequence is the opposite of a re-dump: a v5-provisioned fresh instance is MISSING all five objects and no D23 mechanism can ever supply them** — the port needs a first-class `db/chat_message_fts.rs` carrying the DDL verbatim. That direction is benign (v4's reconciler restores them, seam 10). ⭐ **`SQLITE_ENABLE_FTS5` is ALREADY defined at `crates/quilltap-sqlite3mc-sys/build.rs:57`** (beside `RTREE`, `DBSTAT_VTAB`, `COLUMN_METADATA`), so v5 can create and probe the virtual table today — **the FTS half costs no build change and carries no pinned-crate risk.** <br><br>**⛔⭐ (2) THE ESCALATION, PROVEN BY EXPERIMENT ON REAL MIGRATED DATA — see §1's table for the full matrix.** `chat_messages_fts_ai`'s body is `INSERT INTO "chat_messages_fts"(rowid, content) VALUES (…, qt_text(new."content"))`; `chat_messages_fts_au`'s **`WHEN` clause itself** is `qt_text(new."content") IS NOT qt_text(old."content")`; `_ad`'s body has no `qt_text`. v4's module doc states the failure mode as a deliberate feature: *a connection that opens without it "fails any write to `chat_messages` with `no such function: qt_text`. That is deliberate: a loud, immediate failure beats silent index drift. Every connection this app opens registers it."* **v5 registers no UDF anywhere** (zero hits for `create_scalar_function`/`add_function`/`qt_text` across `crates/`; `Cargo.toml:54` pins `rusqlite` `default-features = false, features = ["buildtime_bindgen"]` with no crate enabling `functions`). **Measured on a read-only copy of live Friday with the UDF deliberately unregistered, writes wrapped in always-rolled-back transactions: EVERY INSERT into `chat_messages` fails and every `UPDATE OF content` fails; non-content UPDATEs and DELETEs succeed.** ⚠ **The experiment proved more than the trigger's `WHEN` guard predicts: the SYSTEM, staff and content-NULL inserts fail too, because SQLite resolves functions when it COMPILES the trigger program, before any row is tested.** So the blast radius is **every** `chat_messages` INSERT — `db/chats_messages.rs:888-903` (`content`/`opaqueContent`), `:957-959` (`context`), `:966-970` (`description`) — plus `:545-548` `update_message`, which is a DELETE + re-INSERT and so re-enters through the AI trigger on every edit. **DELETE still succeeding is the worst asymmetry available: v5 can destroy messages on a shared instance but not create them.** <br><br>**⭐ (3) The four compressed columns — the blast radius funnels to FOUR LINES.** `lib/database/manager.ts:125+` gains `registerCompressedColumns('chat_messages', ['content','opaqueContent','description','context'])` — the same `0x51 0x01 0x01` brotli codec as `186eb09cb`, same 512-byte plaintext floor, **so the columns hold a TEXT/BLOB mix forever**; types stay `TEXT`. v4's census: `content` 355 MB, `opaqueContent` 31 MB, `context` 6.3 MB, `description` 3.3 MB of a 932 MB main DB. **v5's read side is ONE choke point, `crates/quilltap-core/src/db/chats_messages_read.rs`, and all four binds are there:** `:169` `Value::String(row.get::<_, String>(3)?)` (**`content`, idx 3, hard `String`**), `:206` `put_opt_string(…, row.get(22)?)` (`opaqueContent`), `:233` (`context`, idx 30), `:245-250` (`description`, idx 32). `String` **and** `Option<String>` both reject a BLOB with `InvalidColumnType`. **The failure is not per-row: `:283-287` is `for r in rows { if let Some(v) = r? … }`, so ONE compressed cell fails the ENTIRE `get_messages` call** — and `get_message_count` at `:293` is `get_messages(...)?.len()`, so counts die with it. Downstream: the Salon transcript read, `services/build_context.rs`, `services/backup/collect.rs:467-470` (**whose `?` propagates to `backup/mod.rs:121`, so the whole backup fails LOUDLY — the opposite failure mode from `186eb09cb`'s `llm_logs` `.unwrap_or_default()`, and the one mercy in this row**), search-and-replace, chunk-on-write, the transcript route. **Named worst failures: (a) every chat opens empty-or-500 the moment any one of its messages exceeded 512 bytes; (b) Create Backup fails outright; (c) the boot heal aborts before the user opens anything (seam 9).** BLOB-tolerant-but-wrong sites carry over: `tools/run_sql.rs:356-358` renders `<blob: N bytes>` while `services/brahma_console/prompt_text.rs:62` still tells the model `chat_messages carries chatId, role, content…`; `crates/quilltap-cli/src/nodefmt.rs:40-57` emits a Node Buffer dump; and **`crates/quilltap-fixture-sanitizer/src/lib.rs:243-245`/`:334-340` routes a BLOB to `scrub_blob` — so a sanitized fixture's compressed rows become undecodable noise while the sub-512-byte rows in the same column sanitize into valid pseudo-text: a mixed fixture that will read as a v5 decoder bug.** <br><br>**(4) v5's raw SQL over the four — three sites, and the write pair launders the format BACKWARDS.** `db/chats_search.rs:177-182` `… AND content LIKE ? ORDER BY createdAt DESC LIMIT ?` — **`LIKE` does not coerce a BLOB, so it returns zero rows rather than erroring: every compressed message silently vanishes from search**; `:243-246` `UPDATE chat_messages SET content = ?1` binds a Rust `String`; `db/avatar_rolls_collapse_heal.rs:946-949` selects `content, opaqueContent` with the `:1008-1009` UPDATE writing both back. **All three need `qt_text()` on read and a `text_to_blob()` on write, and until they get it v5 progressively DECOMPRESSES a v4-compressed instance one edit at a time** — v4 tolerates the mix, so it is silent, but the invariant is gone. No `LENGTH(`/`trim(`/`json_extract(`/`INSTR`/`SUBSTR`/`ORDER BY` over the four exists elsewhere in `crates/` (swept; the `description` hits in `services/backup/collect.rs:161,204,217,312` belong to `folders`/`embedding_profiles`/`prompt_templates`/`roleplay_templates`/`vector_index_metas`, not to messages). <br><br>**⭐ (5) The search rewrite, and v4's own behaviour-change admission.** `chats-search.ops.ts` (+153) drops `new RegExp(escapeRegex(searchText),'i')` + the `$regex` filter for two hand-built SQL shapes: `buildFtsSearchSql` (probe `chat_messages_fts MATCH ?`, join the map, join back, `chatId IN (…)` **kept for parity**, `ORDER BY mm."createdAt" DESC LIMIT ?`) and `buildLikeSearchSql` (`qt_text(mm."content") LIKE ? ESCAPE '\'`), both wrapped by **`deferTextDecode`** — only the outer SELECT carries `qt_text`, *"SQLite puts a query's output columns into the sorter record"*, measured **1.2 s → 86 ms on 142,000 rows**. NEW `lib/database/repositories/fts-query.ts` (+113): `tokenizeLikeUnicode61` (`/[^\p{L}\p{N}]+/u`), `escapeLikePattern` (`/[\\%_]/g`, **backslash first**), and `buildFtsMatchExpression` → **one quoted phrase with a trailing prefix star**, falling back to an exact scan when there are no tokens or **every token is under `MIN_USEFUL_TOKEN_LENGTH = 2`** (the `C++` case, which as a bare prefix matches 14,260 rows). Plus a **runtime** fallback: an FTS query that throws is caught, warned (`FTS message search failed; falling back to an exact scan`) and re-run as LIKE; and a new `logger.debug('Global message search plan', { path, tokens, chatCount, reason? })`. **The user-visible change v4 states in its own body and help: whole words and word PREFIXES instead of arbitrary substrings (`walk` finds *walking*, no longer finds *sidewalk*), accents and non-ASCII case fold, punctuation is not indexed; still capped at 100, still `createdAt DESC`, NOT ranked.** **v5's home is `db/chats_search.rs`** (module doc `:15-46` documents the `$regex` → `LIKE` seam it reproduces byte-for-byte) reached through `api/ui_search.rs:298-304` ← `api/engine.rs:4482-4491` ← `quilltap-web/src/ui_search_routes.rs:21-36`. **What moves: `like_pattern`/`regex_source_escaped` (`:67-89`) are DELETED, the SQL becomes two shapes, the 100 stays where it already is** (v5 already binds `LIMIT ?` at `:194` — **ahead of v4 until now**), **and `db/like_escape.rs` already exists in the tree but is NOT used by this path** — the ready-made home for `escapeLikePattern`. Oracle families: **`chats_search_equivalence`** (`QT_ORACLE_CHSEARCH`) and **`ui_search_equivalence`** (`QT_ORACLE_UI_SEARCH`); `fts-query.ts` + its 106-line spec is a **pure tier-1 corpus — the cheapest and highest-value unit in the commit.** <br><br>**⭐ (6) THE UN-NUMBERED DEFECT — v5 HAS IT AND PINS IT WITH A PASSING TEST.** v4: *"the old path escaped the query as a regex, then translated it to LIKE with no ESCAPE clause, so `Mr. Smith` matched nothing."* **v5 reproduces it exactly and deliberately** — `db/chats_search.rs:67-79 regex_source_escaped` backslash-prefixes `[.*+?^${}()|[\]\\]`, `:85-89 like_pattern` then does `.replace(".*","%").replace('.',"_")` and wraps `%…%` with **no `ESCAPE`**, and the module doc `:22-32` records the consequence as intended fidelity. **The pin is `like_pattern_reproduces_v4_mangling` (`:~287-296`): `assert_eq!(like_pattern("a.b"), "%a\\_b%")`, `assert_eq!(like_pattern("f("), "%f\\(%")`.** So a v5 user searching `Mr. Smith`, `C++`, `foo(bar)` or `$500` gets **nothing, confidently** — a live user-facing v5 defect on the dogfood copy today. **That test must be RETIRED, not repaired, and its retirement is the red-first proof.** ⚠ The third assert, `like_pattern("50%") == "%50%%"`, records that v5 also leaves a user-typed `%`/`_` as a live wildcard — which v4's `escapeLikePattern` now fixes on the fallback path too. <br><br>**(7) `app/api/v1/ui/search/route.ts` (+63) — pure snippet shaping; NO census row moves.** `foldWithIndexMap(value)` (per-character `normalize('NFD')` + `/[̀-ͯ]/g` strip + `toLowerCase`, **keeping a `map[]` back to ORIGINAL indices** because NFD changes length) plus a rewritten `createSnippet` trying the literal phrase, then the folded phrase, then **each query token folded as a prefix**, tracking `matchLength` in original indices (`end = matchIndex + matchLength + 70`, replacing `query.length`). Reason: under token semantics *a hit need not contain the literal query*, and such results previously degraded to "the first 100 characters". **v5's twin is `api/ui_search.rs:146-171 create_snippet`** (called for messages at `:505` with `max_length: 120`), which has only the `js_index_of` literal arm and the truncate fallback — **the fold arm and `matchLength` are the whole port.** **The request shape does not change** (a GET with query params, no new `Request` variant, no new body key, no tri-state), **so `dispatch_wrong_type_census.rs` does not move and neither does the verb census.** The SPA is a pass-through (`apps/web/src/app/search/search-results.ts:108,136,194,233,263` render `result.snippet`); **no SPA twin of `createSnippet` exists.** <br><br>**(8) `SQLiteTransaction.getCollection` — the `186eb09cb` seam-12 gap is CLOSED upstream, and it IS N/A for v5.** `backend.ts` (+31) threads `collectionCompressedColumns` into the transaction ctor and on to `SQLiteCollection`. **v5 has no collection abstraction and no `withTransaction`-scoped collection**: every `chat_messages` write is hand-written SQL through the partitioned applier, so **this latent hole cannot exist in v5 — record it N/A naming this sha, per that row's instruction.** ⚠ **But the harder half: v4's chokepoint is one `documentToRow`; v5's equivalent is FIVE independent hand-written statements, so when v5 gains a codec, "a write path that bypasses it" is not a latent class — it is the DEFAULT, and every one of the five must be converted by hand with a census guarding the count.** <br><br>**⭐ (9) `collapse-duplicate-avatar-rolls-v1` — v5's heal TOUCHES TWO OF THE FOUR, AT BOOT.** v4 (+16) rewrites the SELECT to `qt_text(content)` / `qt_text(opaqueContent)` and the UPDATE to `textToBlob(...)` with explicit `=== null` guards, noting it *"was safe by migration ordering, and is now safe to replay"*. **v5 ported this as a boot heal (`db/avatar_rolls_collapse_heal.rs`) and the answer is YES: `:946-949` selects both raw and `:950-955` binds them as `Option<String>`, with the `?` at `:958` aborting the pass.** Because it is a heal rather than a user action, **this is the first thing that breaks on a migrated instance — at startup, before any chat is opened.** The write half at `:1008-1013` would re-write plaintext. <br><br>**(10) The boot reconciler, and v4's own "only silent failure mode".** NEW `lib/startup/reconcile-chat-message-fts.ts` (+108) + `instrumentation.ts` (+22, **PHASE 3.65**, `await`ed, wrapped in a `try` that warns `Chat message FTS reconciliation failed`). It reports missing objects and warns `Message search index objects were missing; recreating`, replays the `IF NOT EXISTS` DDL, then compares eligible-row and indexed counts and on a mismatch warns `Message search index is out of step with the transcript; rebuilding` then `info`s `Message search index rebuilt`. Its doc names the design's one unguarded hole: *a table rebuild of `chat_messages` "silently drops the triggers with the old table. No error is raised; the index simply stops being updated, and search quietly goes stale."* **v5's migration runner stays deferred and v4 migrations are re-homed as boot repairs in `seed_built_ins` (`crates/quilltap-host/src/host.rs:1084`, existing tenants `builtin_templates::seed_built_in_templates`, `fictional_clock_anchor_repair::anchor_fictional_clock_bases`, the P4.D63 archive columns).** **Both v4 migrations land there as one repair:** `create-chat-message-fts-v1`'s work IS the reconciler (same module, same counts — v5 gets the migration free by porting the boot guard), while `compress-chat-message-text-v1` is *"byte reclamation, not a correctness prerequisite"* (`BATCH_SIZE` 250, `SAMPLE_LIMIT` 50, `dependsOn: ['sqlite-initial-schema-v1','create-chat-message-fts-v1']`) and **can be deferred indefinitely — but only once v5 READS the format, because new writes already compress and there is no gate to sit behind.** ⚠ **The ordering is a hard constraint the port must honour in the same direction: the index must exist before anything compresses, and the `_au` trigger's decoded-text comparison is what lets 142,697 rows change encoding without retokenizing one index entry.** <br><br>**(11) The CLI half — ZERO Tier R cases move.** `db-commands.js` (+12) decodes `content` in `cmdMessages` and all four in `cmdMessage`; `db-helpers.js` (+7/−4) adds the load-bearing sentence *"It is also REQUIRED for any `--write` that touches `chat_messages`."* **v5 ships neither verb**: `crates/quilltap-cli/src/db_cmd.rs:23-35` lists `"messages"`/`"message"` in `DB_VERBS` and `:274-284` answers *"recognized but not yet available in this build"* (while `src/help/db_help.txt:17,23,98,108` advertises them). **Tier R is 223 cases and contains NO `db messages`/`db message` case — measured, zero move.** The only live CLI surface is the raw-SQL path (`db_cmd.rs:378-402` + `nodefmt.rs:47-55`), whose open is `crates/quilltap-cli/src/dbopen.rs:14-30` — **the same single registration point `186eb09cb` already named, now also needed for `--write`.** <br><br>**(12) Docs + help.** `help/search.md` (+42/−3): a new **"How the Message Search Reads Your Words"** section (six bullets in full Wodehouse register), two rewritten lines in the category and index lists, "exact strings" → "words" in Limitations, a new Limitations bullet, and a new **In-Chat Navigation** section. **v5's copy DIFFERS — the tenth help obligation, exactly as the `186eb09cb` row predicted; see §1 for the full ten.** `docs/developer/DDL.md` (+69, the `chat_messages` compressed-column row plus a `### chat_messages search index (FTS5)` section with the full DDL and the *"nothing else may spell it"* rule) and `features/chat-message-fts5-and-compression.md` (+85, the third revision) → the `docs/v4/` mirror. `docs/CHANGELOG.md`, `README.md`, `CLAUDE.md`, `.claude/commands/update-documentation.md` and the two stamps → NO-PORT candidates. <br><br>**(13) The seven test files (+1,062, plus a 41-line snapshot) are the corpus sources:** `fts-query.test.ts` (106, **pure tier-1, start here**), `chat-message-fts.test.ts` (272 + snapshot, the DDL/trigger/rebuild behaviour), `chats-search-global.test.ts` (199), `reconcile-chat-message-fts.test.ts` (145, incl. the dropped-trigger case), `migrations/chat-message-fts-and-compression.test.ts` (238), `ui/search/route.test.ts` (56, the folded snippet), `collapse-duplicate-avatar-rolls.test.ts` (+5). <br><br>**(14) Foreseen redness, and the fixture trap.** **Red by design: `chats_search_equivalence` and `ui_search_equivalence`** — the oracle switches to token semantics, so every comparand with a prefix/substring distinction (`walk`/`sidewalk`), an accent, or a punctuated query moves, **and the `.`-defect rows FLIP FROM EMPTY TO POPULATED**, which is seam 6's red-first proof. **`like_pattern_reproduces_v4_mangling` fails as a matter of intent.** **Broader tier-2 exposure, with the mitigation measured: 276 of the 469 files in `harness/oracle/cases/` call `initializeDatabase()`**, so those corpora now write compressed cells — **but only at or above the 512-byte floor, and most corpus messages are far shorter**, so movement should be narrow and, where it happens, unmistakable (`harness/oracle/lib/tier2.ts:31-40` hexes Buffers, so the diff reads hex-vs-prose). Candidates: `chats_messages_ops_tier2`, the salon-read families, `chat-dialogs-search-replace`, `markdown-transcript`, `transcript-route`, `turn-transcript`, the backup/export families. **`provisioning_equivalence` should NOT move** — `verify-v5-provisioned.ts` asserts one user, an empty chat list and the BUILTIN embedding profile, writes no message, and the dumper filters triggers out (seam 1). ⚠ **The fixture trap from `186eb09cb` applies again and harder: `crates/quilltap-web/tests/fixtures/` holds 124 entries, and any lane that REBUILDS a `*-main.db` pair at the new pin silently re-encodes every `chat_messages` row over 512 bytes AND acquires the five FTS objects — after which a v5 binary without `qt_text` cannot write to that fixture at all.** Fold this into the round plan beside the standing ten-family fixture-vintage heal order. | UNPROCESSED |

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
