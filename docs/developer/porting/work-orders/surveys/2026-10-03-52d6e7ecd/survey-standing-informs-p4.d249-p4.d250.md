# Survey — v4 `52d6e7ecd` (standing informs) against v5 `main` `29c4484e5`

**Dated 2026-10-03.** v4 at `52d6e7ecd` (`4.10.0-dev.109`; the checkout on
`main`, CLEAN, both §2 logs empty). v5 at `main` `29c4484e5`. Read by P4.D249
(server) and P4.D250 (SPA). Every v4 citation below is the SHIPPED hunk
(`git show 52d6e7ecd -- <path>`), never the commit message (ledger §5.3).

## §A — v4: what the commit ships (production hunks only)

31 files; 7 are tests, 9 docs/stamps. The production hunks:

| # | v4 path | what moves |
|---|---|---|
| A1 | `migrations/scripts/add-chat-informs-permanent.ts` (NEW, 88 lines) | id `add-chat-informs-permanent-v1`, `dependsOn: ['add-chat-informs-table-v1']`, `introducedInVersion: '4.10.0'`; `shouldRun` = SQLite ∧ `sqliteTableExists('chat_informs')` ∧ `!sqliteColumnExists('chat_informs','permanent')`; `run` = `addColumnIfMissing('chat_informs','permanent','INTEGER NOT NULL DEFAULT 0')`, INFO `Added the permanent column to chat_informs` `{context:'migration.add-chat-informs-permanent', columnsAdded, durationMs}`, ERROR `Failed to add the permanent column to chat_informs`. No UPDATE (the default makes every existing row a one-shot). |
| A2 | `migrations/scripts/index.ts` | registered LAST in `migrations[]` (after `dropChatRenderedMarkdownMigration`) and in the export list. |
| A3 | `lib/startup/prettify.ts:152` | label `'add-chat-informs-permanent-v1': 'Pinning a place on the tray for the notes meant to stay put'` — v5 has no prettify analog (NO-PORT, the P4.D63/D73/D79 precedent `chats_cycle_order_repair.rs` cites). |
| A4 | `lib/schemas/chat-inform.types.ts` | `ChatInformSchema.permanent: z.boolean().default(false)` placed AFTER `recordMessageId` and BEFORE `createdAt` (schema order → `generateDDL` column order → export key order); NEW exported `isInformInForce(row) = Boolean(row.permanent) \|\| !row.consumedAt`; `PendingInformBatch.permanent: boolean` between `recordMessageId` and `pendingParticipantIds`. |
| A5 | `lib/database/repositories/chat-informs.repository.ts` | NEW `byPostingOrder` (createdAt ms delta, `id.localeCompare` tie) and `byDeliveryOrder` (`Boolean(a.permanent) !== Boolean(b.permanent)` → standing first, else `byPostingOrder`); `findPendingForParticipant` → `rows.filter(isInformInForce).sort(byDeliveryOrder)`; `findConsumedByMessages` → same consumed filter, `.sort(byDeliveryOrder)` (was posting order); `findPendingBatches` → `filter(isInformInForce).sort(byPostingOrder)` (posting order, NOT delivery order) and each batch gains `permanent: Boolean(row.permanent)` (first-seen row's value) at the key position between `recordMessageId` and `pendingParticipantIds`; `createBatch({…, permanent?})` → `const permanent = params.permanent === true`, written on every row, and the `Inform batch created` debug gains `permanent` LAST; `deletePendingByBatch` → `if (!isInformInForce(row)) continue` (a standing batch deletes WHOLE, a consumed one-shot row survives); `markConsumed` and `deletePendingForParticipant` bodies UNCHANGED (doc only — the latter goes through `findPendingForParticipant`, so it now also deletes a seat's delivered standing rows). |
| A6 | `lib/chat/context/inform-block.ts` | `inForce = findPendingForParticipant(...)` is now read on EVERY call (swipe too — a second seat read on a swipe); swipe rows = `mergeForSwipe(findConsumedByMessages(...), inForce.filter(r => r.permanent))` = standing first, then the reapplied rows whose id is not a standing id; the empty debug gains `standing: 0`; the built debug's fields become `pending: isSwipe ? 0 : rows.length - standing`, `reapplied: isSwipe ? rows.length - standing : 0`, `standing`, `passages` (in that order); `rowIds = isSwipe ? [] : rows.filter(r => !r.consumedAt).map(r => r.id)` (an already-stamped standing row is never handed back, so its first-delivery stamp never moves). |
| A7 | `app/api/v1/chats/[id]/schemas.ts:255-260` | `informSchema.permanent: z.boolean().optional().default(false)` — `null` and non-booleans are a Zod 400; absent → `false`. `cancelInformSchema` unchanged (doc only). |
| A8 | `app/api/v1/chats/[id]/actions/inform.ts` | `createBatch` gets `permanent: validated.permanent`; INFO `[Chats v1] Inform posted` gains `permanent` LAST; the 201 body becomes `{ success, batchId, targetParticipantIds, permanent, message }` (`permanent` BEFORE `message`); `handleGetInforms` code unchanged (the `permanent` key arrives via `findPendingBatches` and survives the `...batch` spread at its position); `handleCancelInform` computes `permanent = rows.some(row => row.permanent)` (over ALL the batch's rows, pre-delete) and adds it to the DEBUG `[Chats v1] Inform cancelled` between `anyConsumed` and `recordDeleted`; the response `{ success, removed, recordDeleted }` UNCHANGED. |
| A9 | `lib/import/quilltap-import/reconcile.ts:97` | `remapChatInform` writes `permanent: inform.permanent === true` between `recordMessageId` and `consumedAt` (a bundle without the key → `false`; a non-`true` truthy value → `false`). |
| A10 | `public/schemas/qtap-export.schema.json:1052-1057` | `$defs.ChatInform` gains `permanent` (`{"type":"boolean","default":false,"description":…}`) after `recordMessageId`; `consumedAt` and `consumedByMessageId` gain/changes `description`s. |
| A11 | `components/chat/InformDialog.tsx` | the checkbox (state `permanent`, default `false`, disabled while posting), the POST body sends `permanent` ALWAYS, the three-way toast, the conditional help sentence, the footer label — see §D for bytes. |
| A12 | `components/chat/PendingInformChips.tsx` | client type `permanent?: boolean`; the standing label, the standing hover title, the standing withdraw title — see §D. |
| A13 | `help/inform.md`; `docs/CHANGELOG.md`, `docs/developer/{API,DDL,PROMPT_ARCHITECTURE}.md`, `docs/developer/features/salon-inform.md` | vendored-tree moves (v5 mirrors: `help/inform.md`; `docs/v4/CHANGELOG.md`, `docs/v4/developer/{API,DDL,PROMPT_ARCHITECTURE}.md`, `docs/v4/developer/features/salon-inform.md` — all five exist in v5). |

**Backup/restore have NO v4 hunk** — v4's backup serializes whole rows, so
the column rides along by construction. v5's backup marshals a fixed column
list (§B5), so v5 DOES need a hunk there. Same for the export record (v5
reorders through `schema-key-order.json`).

**SDKs unmoved** (`git diff e5c6bd0c0 52d6e7ecd -- plugins/ package.json
packages/` moves the two version stamps only) → `provider_sdk_version_guard`
stays GREEN. **Pin markers:** `git show 52d6e7ecd:package.json` →
`4.10.0-dev.109` (baseline `4.10.0-dev.108`); `git cat-file -e
52d6e7ecd:migrations/scripts/add-chat-informs-permanent.ts` succeeds, at
`e5c6bd0c0` it fails.

v4's tests at the pin (the oracle's shapes to mirror): `__tests__/unit/lib/
database/repositories/chat-informs-standing.test.ts` (129 lines — the four
repository behaviours), `__tests__/unit/lib/chat/context/inform-block.test.ts`
(+71), `__tests__/unit/app/api/v1/chats/[id]/actions/inform.test.ts` (+30),
`__tests__/unit/lib/import/chat-informs-import.test.ts` (+8),
`__tests__/unit/migrations/add-chat-informs-permanent.test.ts` (64),
`__tests__/unit/components/chat/{InformDialog,PendingInformChips}.test.tsx`
(+23, +10).

## §B — v5: the server surfaces (main `29c4484e5`)

**B1 `crates/quilltap-core/src/db/chat_informs.rs`** (585 lines). Header
`:1-73` (the two-DDL rationale `:18-58`; ordering prose `:67-73`);
`CHAT_INFORMS_TABLE_DDL` `:83-95` (10 columns, no `permanent`; its doc says
byte-identical to `fresh_schema.json`); `ensure_chat_informs_table`
`:105-108`; `ChatInformRow` `:112-131` (10 fields); `PendingInformBatch`
`:136-142`; `row_to_json` `:159-187` (omits NULL optionals — the export
record and backup reuse it); `ChatInformCreate` `:193-204`; `SELECT_COLUMNS`
`:206-208` + `row_from` `:210-223` (POSITIONAL 0..9 over an explicit column
list — so an ALTER-appended `permanent` is safe as long as the list names
it); `by_created_at_then_id` `:239-251` (the only comparator; keeps v4's
NaN-means-Equal leg); `find_pending_for_participant` `:269-286` (SQL `WHERE
chatId = ?1 AND participantId = ?2`, Rust filter `consumed_at.is_none()`
`:280`); `find_consumed_by_messages` `:293-319`; `find_pending_batches`
`:332-362` (filter `:339`, sort `:343`, first-seen fold);
`find_by_chat_id` `:367-376`, `find_by_batch_id` `:380-389`; `create`
`:397-417` (INSERT, 10 cols); `create_batch(chat_id, content_markdown,
participant_ids, record_message_id)` `:433-482` (debug `Inform batch
created` `:471-479`); `mark_consumed` `:491-518`; `delete_pending_by_batch`
`:523-542` (skip `consumed_at.is_some()` `:527`); `delete_pending_for_
participant` `:546-567` (via `find_pending_for_participant` `:551`);
`delete_by_chat_id` `:575-585`.

**B2 `crates/quilltap-core/src/api/chat_informs.rs`** (888 lines).
`parse_inform` `:76-175` walks two keys; `parse_cancel` `:182-204`;
`chat_inform` `:213-385` (`create_batch` `:331-347`, INFO `[Chats v1] Inform
posted` `:367-374`, 201 body `{success,batchId,targetParticipantIds,message}`
`:376-384`); `chat_informs_list` `:396-456` (per-batch JSON `:432-444`, debug
`:448-453`); `chat_inform_cancel` `:465-~580` (`find_by_batch_id` `:483`,
`delete_pending_by_batch` `:516`, **INFO** `[Chats v1] Inform cancelled`
`:569-573` — v4's line is `logger.debug`: measure the level before touching
it; a level divergence predating this commit is recorded, not silently
"fixed"); unit tests from `:593`.

**B3 The wire.** `crates/quilltap-core/src/api/types.rs`:
`Request::ChatInform{chat_id, content_markdown, target_participant_ids}`
(both `double_option`) `:3945-3951`; `ChatInformsList` `:3955-3957`;
`ChatInformCancel` `:3962-3966`; fence `// === P4.D205` `:3930-3967`;
Response variants `:4560-4568`. Engine `api/engine.rs:5800-5823`. REST edge
`crates/quilltap-web/src/wardrobe_routes.rs`: GET `informs` `:333-340`; POST
`inform` `:518-538` with `request_envelope("chatInform", …,
&["contentMarkdown","targetParticipantIds"], …)` `:521-525`; `cancel-inform`
`:539-552`; action lists `:407`, `:463-464`; response fan-out `:54-56`. **The
SPA posts through `/api/dispatch`, never the REST route.** So adding
`permanent` to the request is a `types.rs` variant field + the envelope key
list + the parse walk — `api/types.rs` was FROZEN last round and is OPEN to
P4.D249 ALONE this round. **Census note:** `dispatch_wrong_type_census.rs`
asserts no literal `441` — its asserted constant is
`EXCLUDED_BY_THE_ROUTE_IDENTIFIER_RULE: usize = 451` (`:2605`, checked
`:2638`); "441" is history prose (`:346`, `:2533-2567`). A boolean field adds
no `*_id` — predict UNMOVED and measure. `tri_state_edges_share_the_decoder.rs`
(`:550-553`, the informs GET count) and the P4.D202 `request_envelope`
tri-state census guard must be run.

**B4 `crates/quilltap-core/src/services/inform_block.rs`** (158 lines).
`InformBlock{content,row_ids,passage_count}` `:39-48`;
`build_inform_block(db, chat_id, participant_id, regeneration_of_message_ids)`
`:65-112` (swipe vs pending arm `:79-83`); debug lines target
`quilltap::inform`: `[Inform] No inform block for this turn` `:88-95`
(`pending=0, reapplied=0`), `[Inform] Built inform block` `:101-109`;
`is_swipe_request` `:118-120`; `assemble_inform_block(rows, is_swipe)`
`:132-158` (`row_ids` empty on a swipe else every row `:151-155` — NO
already-stamped exclusion). Caller `services/build_context.rs:3044-3053`
(input `:590-598`; `inform_row_ids` set `:3894`, field `:331-340`).

**B5 Call sites + carriers.** Finalizer consume
`services/message_finalizer.rs:1042-1093` (`mark_consumed` `:1062-1067`);
preserved-partial consume `services/primary_stream.rs:820-847`; swipe ids
`services/regenerate_swipe.rs:587-627` (fenced `P4.D205 OUT-OF-MANDATE —
P4.D207 preserves`) + `:645-647` (census `chat_informs_swipe_handle.rs`);
seat removal `api/chat_cast.rs:642-670`; chat delete `db/chats.rs:1396-1418`;
delete-all `services/delete_all.rs:208-211`. **Import:**
`services/quilltap_import/reconcile.rs` `ChatInformRemap` `:811-827`,
`remap_chat_inform` `:858-911` (builds `ChatInformCreate` `:901-911`, no
`permanent`); loop `quilltap_import/mod.rs:1513-1642` (`create` `:1626`).
**Export:** `services/qtap_export/records.rs:255-285` (`row_to_json` +
`reorder("chat_inform", …)`); key order `services/qtap_export/
schema-key-order.json:196-207` (10 keys) — regenerate with
`harness/oracle/fixtures/dump-export-key-order.ts` from the pin (the P4.D235
recipe, its order `:303-308`). **Backup:** `services/backup/collect.rs`
`CHAT_INFORMS` `:270-281` (the `F` enum `backup/marshal.rs:31` already has
`Bool`/`BoolOpt`), collected `:565-570`; restore insert
`backup/restore/orchestrator.rs:1106-1140` (`ChatInformCreate`
`:1123-1134`); `uuid_remap.rs:282-290,418`; `archive.rs:177-179`;
`preview.rs:52`; `manifest.rs:70-75` (a count — no change). **Boot:**
`crates/quilltap-host/src/host.rs:1563-1581` (`ensure_chat_informs_table`
at `:1580`).

## §C — v5: schema adoption mechanics

- **D23** (`phase-4.md:330-368`): re-dump `crates/quilltap-core/src/services/
  provisioning/fresh_schema.json` from v4's live `generateDDL` with
  `harness/oracle/provision/dump-fresh-schema.ts` (recipe `:28-33`,
  `QT_SCHEMA_OUT`, run from the PIN), record a dated note in
  `services/provisioning/mod.rs` (~`:90-130`, the P4.D235 entry near
  `:127`), prove `provisioning_equivalence` (`QT_ORACLE_PROVISION`,
  `QT_FIXTURE_V4_FRESH`, `QT_V5_PROVISION_OUT`, `QT_DBKEY_V4_FIXTURE`, header
  `:16-32`) red-first. `chat_informs` sits at `fresh_schema.json` line 8
  (CREATE) and line 45 (index). Expect `permanent INTEGER NOT NULL DEFAULT 0`
  BETWEEN `recordMessageId` and `createdAt` in the fresh CREATE (schema
  order) — measure, do not assume the exact type spelling `generateDDL`
  emits for a `z.boolean().default(false)` (it may differ from the
  migration's `INTEGER NOT NULL DEFAULT 0` — the `routeTrail` two-shape
  precedent, `chat_messages_route_trail_repair.rs`).
- **The boot column ensure** (v5's migration runner stays deferred): the
  `*_repair.rs` shape — `table_exists` → `PRAGMA table_info` → `ALTER TABLE …
  ADD COLUMN`. The NOT NULL DEFAULT 0 precedent is
  `db/chats_moderation_refusal_ledger_repair.rs:53-56` (header `:1-45`), host
  call `host.rs:1401-1402`; the closest one-column twin is
  `db/chats_cycle_order_repair.rs` (whole file — its "two v4 shapes AGREE"
  section is the template for recording whether the migration DDL and
  `generateDDL` agree). The ensure must run AFTER `ensure_chat_informs_table`
  (`host.rs:1580`) — v4's `dependsOn` — and `CHAT_INFORMS_TABLE_DDL` must
  carry the column in generateDDL's shape so a fresh-created table and an
  ALTERed one are both readable and a fresh one matches `fresh_schema.json`
  byte for byte.
- **`db/table_shape.rs`** (P4.D248) `STRUCTURAL_TABLES` `:76-~255` = 11
  tables; **`chat_informs` is NOT among them**, so the ledger's "moves what
  v5's boot creates for an absent structural table" does NOT apply to this
  table directly — but `fresh_ddl_for` (`:388-411`) reads the embedded
  `fresh_schema.json` (`:371`), so the re-dump moves its INPUT; P4.D249 runs
  `table_shape_equivalence` + the host structural arms to prove neutrality.
- **Hand-written DDL mirrors** (memory `d23-redump-is-not-only-fresh-schema`):
  grep `CREATE TABLE.*chat_informs` across `crates/` and `harness/` outside
  `src/db/` and widen every copy, or a tier-3 family goes silently empty.
- **`build_context_tier3_equivalence.rs:421`** counts seat reads by the SQL
  literal `FROM chat_informs WHERE chatId = ?1 AND participantId = ?2` —
  A6 adds a second seat read on a swipe, so its count moves by DESIGN on
  swipe cases (re-recorded from the pin, never hand-edited).
- **Committed fixtures:** 106 encrypted `crates/quilltap-web/tests/fixtures/
  *.db`; the widening tool `harness/oracle/fixtures/migrate-memories-fixture-
  columns.ts` (`--report-only`; `--module <file>#<export>` runs v4's real
  migration, doc `:283-306`, impl `:548+`); the P4.111 method
  (`p4.111-fixture-vintage-heal-ten-mains.md:68-80`). **Does any committed
  pair carry `chat_informs`?** `system_import_state.rs:213-217` records the
  committed triple PREDATES the table. Since the v5 read names `permanent`
  explicitly, a pair that HAS `chat_informs` without `permanent` will fail
  every inform read in a family that opens it without booting the ensure —
  P4.D249 runs `migrate-memories-fixture-columns.ts --report-only` over all
  106 first and widens ONLY the pairs it names, through `--module
  migrations/scripts/add-chat-informs-permanent.ts#addChatInformsPermanentMigration`.

## §D — v5: the SPA surfaces (`apps/web`)

- Dialog `src/app/chat/inform-dialog.ts` (279 lines): selector
  `qt-inform-dialog` `:70-71`; audience group `:87-128`; the help paragraph
  ending `…is gone, like a note fed to the fire.` `:135-137`; footer
  `<div qt-modal-footer class="flex items-center justify-end gap-3">` `:154`;
  `onPost` `:241-262` dispatches `{type:'chatInform', chatId,
  contentMarkdown, targetParticipantIds}` `:251-256`; the toast `everyone ?
  'The company has been informed' : \`Informed ${names.join(', ')}\`` `:258`.
  Spec `src/app/chat/inform-dialog.spec.ts` (294 lines).
- Chips `src/app/chat/pending-inform-chips.ts` (140 lines): template
  `:43-60` (`[title]="chip.hover"` `:46`; `title="Withdraw this inform"`
  `:55`; aria-label `:56`); list query `:80-91`; `chips` computed `:98-114`
  (label `Informing ${joined} before their next turn` `:111`); cancel
  `:118-130`; `firstLine` helper `:14`. Spec `pending-inform-chips.spec.ts`
  (179 lines).
- Contract `src/app/core/core-contract.ts`: union members `:2655-2659`;
  `PendingInformBatch` `:8205-8212`; `ChatInformRequest` `:8224-8229`;
  `ChatInformResult` `:8232-8239`; list `:8242-8249`; cancel `:8257-8268`.
- Wiring (no change predicted): `chat-keys.ts:46-55`,
  `realtime-topic-map.ts:94-101`, `chat-composer.ts`, `screens/salon/
  salon-conversation.ts`.
- Playwright `apps/web/e2e/salon-inform-flow.spec.ts` (226 lines; gate
  `P4D205_SERVER_LANDED = true` `:36`; beats `:78,117,153,181`).

**v4's exact SPA bytes at the pin** (A11/A12 — copy from `git show
52d6e7ecd:components/chat/InformDialog.tsx` and `…PendingInformChips.tsx`,
never from this table):
- checkbox label `Keep it standing in this chat`; hint (id
  `inform-permanent-hint`) `Every turn they take here, until you withdraw it.
  This chat only — it follows no one anywhere else.`; the label sits FIRST in
  the footer with `mr-auto`; input `qt-checkbox mt-0.5`, `disabled` while
  posting, `aria-describedby="inform-permanent-hint"`.
- the help sentence's tail: unchecked → `once they have had their turn it is
  gone, like a note fed to the fire.`; checked → `it stays at their elbow for
  every turn they take in this chat, until you withdraw it.` (preceded by
  `It is never spoken aloud, and `).
- toast: checked → `` `A standing note for ${whom}, for the rest of this chat` ``
  where `whom = everyone ? 'the company' : selectedNames.join(', ')`; else the
  two existing strings.
- POST body always carries `permanent` (boolean).
- chip label: standing → `` `Informing ${names} on every turn in this chat` ``;
  hover title: standing → `` `Standing in this chat until withdrawn — ${firstLine(...)}` ``;
  withdraw button title: standing → `Withdraw this standing inform`; the
  aria-label unchanged.

## §E — The harness families that touch Inform

Dedicated (`crates/quilltap-harness/tests/`):

| family | tier | recipe header | env vars | case / fixture |
|---|---|---|---|---|
| `chat_informs_tier2_equivalence.rs` | 2 | `:39-51` | `QT_ORACLE_CHAT_INFORMS`, `QT_FIXTURE_CHAT_INFORMS` (builder `QT_FIXTURE_OUT`) | `harness/oracle/cases/chat-informs-tier2.ts`; spec `harness/oracle/fixtures/chat-informs-tier2.json`; builder `build-chat-informs-fixture.ts` |
| `chat_informs_routes_equivalence.rs` | route | `:34-50` (jest via a /tmp mirror, `TZ=UTC`) | `QT_ORACLE_CHAT_INFORMS_ROUTES` (jest `QT_ORACLE_OUT`), `QT_FIXTURE_CHAT_INFORMS` | `cases/chat-informs-routes.test.ts`; same builder |
| `inform_block_equivalence.rs` | 1 | `:23-31` | `QT_ORACLE_INFORM_BLOCK` | `cases/inform-block.ts` (comparands `content`/`rowIds`/`calledMethod`/`writeCalls`) |
| `chat_informs_remap_equivalence.rs` | 1 | `:22-30` | `QT_ORACLE_CHAT_INFORMS_REMAP` | `cases/chat-informs-remap.ts` |
| `chat_informs_swipe_handle.rs` | source census | — | — | — |
| `inform_drop_lives_on_the_action.rs` | committed `chat-cast-{main,mount}.db` | — | — | — |

Families that plant or consume informs: `salon_mutations_equivalence`
(`QT_ORACLE_SALON_MUTATIONS`, plants `:186-192`); `chat_export_equivalence`
(`QT_ORACLE_CHAT_EXPORT`, `plantInform` `:97-100`); `system_backup_
equivalence` (`QT_ORACLE_SYSTEM_BACKUP`, `QT_BACKUP_DUMP`, `plant_informs`
`:220-227`); `system_restore_state` (`QT_ORACLE_SYSTEM_RESTORE`, the
pre-4.10 no-informs arm `:1297,1694`); `system_import_state`
(`QT_ORACLE_SYSTEM_IMPORT_EXECUTE`; `:213-217` — the committed triple
predates Inform); `backup_uuid_remap_equivalence` (`QT_ORACLE_UUID_REMAP`;
`:52,76,146,193`); `regenerate_swipe_tier3_equivalence` (`QT_ORACLE_REGEN`,
`QT_FIXTURE_REGEN_MAIN/MOUNT`; informs dump `:644-679`); `primary_stream_
tier3_equivalence` (`QT_ORACLE_PRIMARY_STREAM`, `QT_FIXTURE_PRIMARY_STREAM`,
`QT_ORACLE_OPENAI_FALLBACK`; `:249-252,1189`); `orchestrator_tier3_
equivalence` (`QT_ORACLE_ORCHESTRATOR`, `QT_FIXTURE_ORCH_MAIN/MOUNT`;
consumption comparand `:2655-2660`); `build_context_tier3_equivalence`
(`QT_ORACLE_BUILD_CONTEXT`, `QT_FIXTURE_BC_MAIN/MOUNT`; the seat-read SQL
counter `:421`); `qtap_schema_embed_guard` (`QT_V4_ROOT`). Builders that
seed `chat_informs`: `build-{chat-informs,regenerate-swipe,primary-stream,
orchestrator}-fixture.ts`, `build-context-tier3-fixture.ts`,
`uuid-remap-corpus.json`, `dump-export-key-order.ts`. Web
(`crates/quilltap-web/tests/`): `chat_informs_rest_routes.rs`,
`chat_informs_dispatch_wire.rs`, `tri_state_edges_share_the_decoder.rs`
(`:550-553`, the informs GET count), `dispatch_wrong_type_census.rs`
(P4.D205 notes `:2555-2567`; asserted constant
`EXCLUDED_BY_THE_ROUTE_IDENTIFIER_RULE = 451` `:2605`).
