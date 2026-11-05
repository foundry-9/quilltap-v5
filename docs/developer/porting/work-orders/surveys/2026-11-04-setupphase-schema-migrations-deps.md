# Survey — the `624d0d25c` round's SCHEMA / MIGRATIONS / DRIVER / DEPENDENCY slice (the 22nd + 23rd D23 moves and the two fresh-only moves the ledger missed; the four new v4 migrations as boot ensures; the FTS5 character-only reindex; the Anthropic `2026-10-21` API move at the schema / repository / manifest level; SQLite3MC 2.4.0; `quilltap db integrity`; the SDK floors at the new pin; the IMPORT-DEAD census)

Read-only planning survey, **2026-11-04**. The round pins at v4 **`624d0d25c`**
("ci: restore the 8 GB heap for the coverage run", `4.13.0-dev.8`). Oracle
baseline **`236c04e35`**. v5 `main` **`ff1651177`** (core 0.0.1427, host
0.0.223, web 0.0.256, cli 0.0.38, harness 0.0.1110, SPA 0.5.971). Every v4
read below is `git show 624d0d25c:<path>`, `git diff 236c04e35 624d0d25c --
<path>` or one commit's hunks. v4's working tree (unrelated uncommitted
optimizer edits) was never read. Nothing was built, run or written in either
repo except this file. Port facts come from SHIPPED HUNKS, never commit prose.
One external fact was fetched: the utelle SQLite3MultipleCiphers 2.4.0 release
notes and sqlite.org's 3.53.3 / 3.53.4 change notes (§4).

**Commits in scope:**

| sha | subject | this survey's slice |
|---|---|---|
| `db6190b44` | salon: Deus Ex Machina — established facts | ONLY the 22nd D23 move: `chat_messages."isEstablishedFact"`, `add-established-fact-field-v1`, the row / event Zod fields, the carriers' plumbing, the export schema line. The behaviour (frames, reminder, job, composer) belongs to the Deus survey (§1.6 lists where it lives) |
| `d439f6dc9` | connection-profiles: cap images sent per request (default 4) | ONLY the 23rd D23 move: `connection_profiles."maxImagesPerRequest"`, `add-profile-max-images-per-request-v1` + its NULL backfill, the Zod field, the legacy seed, the route validation bytes, the export schema lines, and the SDK move riding it. The cap's behaviour (`adaptMessagesForProfile`, the Lantern walk, failover) belongs to the image-budget survey (§1.6) |
| `902c6efe9` | search: keep Staff messages and announcement bubbles out of global search | whole (FTS5 eligibility, trigger replace, the reindex migration, both search arms) |
| `544c7ab1e` | anthropic: move to API version 2026-10-21; drop temperature and top_p | whole at the server / manifest / repository / migration level; the profile-editor SPA half is listed (§3.6) |
| `8832cfd20` | deps: SQLCipher driver to better-sqlite3-multiple-ciphers 13 | the amalgamation question, the INFO line, the CLI driver under Tier R |
| `70d2859d8` | Docs: …; describe database encryption correctly; fix db integrity | ONLY the CLI `db integrity` + its help bytes |
| `9f761e157`, `f56965ef9`, `d439f6dc9`, `8f83041aa` | dependency moves | the four recorded SDKs, plugin-utils, the root SDKs, the CLI driver, at the pin's floors |
| `b09ca46bb`, `c539c82e0` | character kind; Brahma lookups | ONLY their FRESH-SCHEMA consequences (§1.3) — the ledger says "no D23 move" for both; the hunks say otherwise |
| `29c1e2690`, `600c2906f`, `134961572`, `a0ed5ecad`, `a3d53eae1`, `604a082c4` | (various) | ONLY the exports they DELETED that `harness/oracle/` imports (§7) |

Ledger rows read: `drift-ledger.md` §1 (`:17-433`), §3 rows `8832cfd20`,
`f56965ef9`, `9f761e157`, `544c7ab1e`, `902c6efe9`, `db6190b44`, `d439f6dc9`,
`8f83041aa`, `70d2859d8`, `b09ca46bb`, `c539c82e0`; §5.1. Where a hunk
disagrees with a row, the correction is marked **⚠ ledger**.

Shape follows `surveys/2026-10-29-setupphase-keystone-memory.md`.

---

## §0 Headline findings (written last)

1. **⚠ ledger — the 22nd move's FRESH shape is `"isEstablishedFact" TEXT`,
   not `INTEGER DEFAULT NULL`.** generateDDL renders `chat_messages` from
   `ChatMessageRowSchema` (`chats-messages.ops.ts:77`), where the field is
   `z.union([z.boolean(), z.number().transform(v => v === 1)]).nullable().optional()`
   — a union of two different base types → `getBaseType` answers `'unknown'`
   (`lib/database/schema-translator.ts:83-89`) → `TEXT`, no `DEFAULT`, no
   `NOT NULL` (`:318-331`, `:340-341`). This is EXACTLY the `isSilentMessage`
   seam: v5's committed `fresh_schema.json` already reads `"isSilentMessage"
   TEXT` at that slot. The migration appends `INTEGER DEFAULT NULL`
   (`add-established-fact-field-v1.ts:42`). `DDL.md:1017` hand-writes the
   migration shape. Consequence: on a v5-PROVISIONED instance v4 stores a
   posted fact as TEXT `"1.0"` (a JS number bound as REAL into a TEXT-affinity
   column); on a migrated one, INTEGER `1`. v5 must reuse the `isSilentMessage`
   codec on BOTH sides (`db/chats_messages.rs:1162-1175` `is_silent_stored`,
   `db/chats_messages_read.rs:142-155` `put_is_silent`) — generalised by key
   name, not copied.
2. **⚠ ledger — TWO more fresh-schema moves ride this pin, both with NO
   migration (the `canDrawDiagrams` class).** The ledger says `b09ca46bb`
   ("NO migration, NO D23 move") and `c539c82e0` ("a NEW VAULT KEY, NOT a D23
   move"). But `CharactersRepository` is `super('characters', CharacterSchema)`
   (`characters.repository.ts:82`) and `ProjectsRepository` is
   `super('projects', ProjectSchema)` (`projects.repository.ts:40`, which
   spreads `ProjectPropertiesSchema.shape`, `project.types.ts:170`), so the
   live dump at the pin will carry:
   - `characters."kind" TEXT DEFAULT 'roleplay'` between `"diagramStyle"` and
     `"systemTransparency"` (`character.types.ts:273`);
   - `projects."brahmaConnectionProfileId" TEXT` between
     `"answerConfirmationOverride"` and `"storyBackgroundsEnabled"`
     (`project.types.ts:109`).
   Neither has a v4 migration, so a MIGRATED v4 instance never has either
   column; only a fresh one does. The precedent is `canDrawDiagrams` /
   `diagramStyle` (vault-only, "not a DB column" per v4's own comment, yet
   present in v5's committed dump since P4.D341). v5 needs NO ensure for
   them, and no v5 SQL write may name them (they are vault-managed — v5's
   `db/characters.rs:105` `MANAGED_FIELDS` must gain `kind`, which is the
   character-kind survey's). The re-dump carries them because D23 says the
   dump is never hand-edited. Predicted totals (to be confirmed by the
   re-dump): FOUR `main` statements move, `mountIndex` / `llmLogs`
   byte-equal.
3. **The four new migrations are FOUR different ensure classes**
   (registration `migrations/scripts/index.ts:954-960`, all after
   `addRoleplayTemplateUnmarkedFieldsMigration`):
   - `strip-anthropic-sampling-parameters-v1` — `shouldRun` is true whenever
     `connection_profiles.parameters` exists (`:62-71`), so v4 runs it ONCE on
     every instance (fresh included) and stamps it → a **STAMPED data
     rewrite** (the P4.D288 R-E class, `db/memory_rewrite_heals.rs`); a
     **frozen provider list `['ANTHROPIC']`** (`:36`) matched by
     `upper("provider") IN (…)`, not the manifest.
   - `reindex-chat-message-fts-character-only-v1` — `shouldRun` is
     STRUCTURAL + COUNT-based (missing objects ∨ stale triggers ∨ eligible ≠
     indexed, `:52-60`); v4 skips WITHOUT a stamp when false
     (`migrations/index.ts:164-167`) and stamps after it runs. Must run
     **before** v5's PHASE 3.65 reconcile (`host.rs:1727`), not in the
     236-chain slot, or the reconcile rebuilds first with its WARN.
   - `add-established-fact-field-v1` — column-gated, UNSTAMPED (R-9 class),
     one INFO only on a real add.
   - `add-profile-max-images-per-request-v1` — column-gated, UNSTAMPED, but
     its INFO fires with `{added, backfilled}` and the backfill is an
     `UPDATE … WHERE NULL` that is a no-op after `ADD COLUMN … DEFAULT 4`.
4. **The FTS change breaks every reduced `chat_messages` fixture that inserts
   through the triggers.** The new INSERT trigger's `WHEN` names
   `new."systemSender"` and `new."customAnnouncer"`
   (`chat-message-fts.ts:101-107`, `:130`); SQLite creates a trigger naming a
   missing column without complaint and then fails EVERY insert (v4's own
   comment, `create-chat-message-fts.ts:47-50`). v5's
   `db/chat_message_fts.rs` unit fixture `REDUCED_DDL` (`:363-365`) and the
   `chat-message-fts.json` / `chat-message-fts-reconcile.json` corpora carry
   neither column → red until widened. The ledger's ping-pong hazard is
   real and is closed by porting the eligibility fragment once
   (`chat_message_fts.rs:118-127`), since the counts, the trigger, the
   rebuild AND v5's LIKE fallback (`chats_search.rs:137-153`) all read it.
5. **The SDK floors at the new pin move ALL FOUR recorded SDKs** — ⚠ ledger
   (`9f761e157` is "NO-PORT? … inert type-only imports"; true only at the old
   pin). At `624d0d25c` the plugin dirs declare `@anthropic-ai/sdk` **^1.4.0**
   (from 0.115.0 — a MAJOR), `@google/genai` **^2.32.0** (from 1.52.0 — a
   MAJOR), `openai` **^7.47.0** (from 7.23.0), `@openrouter/sdk` **^1.5.34**
   (from 1.3.28), `@quilltap/plugin-utils` **^2.7.3** (from 2.7.1). The CLI
   declares the **13.0.3** driver. `.claude/pin-plugin-sdks.sh` installs those
   floors, so `provider_sdk_version_guard`'s FIVE recorded constants
   (`:116-145`) all move and every recorded provider corpus is a re-record.
   The genai major is the one to MEASURE first: `request_builder_google_wire_
   equivalence` pins the wire framing. Root: `openai` 7.47.0, `@openrouter/sdk`
   1.5.34 (`INSTALLED_ROOT_*`, `:128-138`).
6. **The Anthropic API version is a manifest constant in v5, and the
   SDK default is `2026-10-21`** — measured in the pin's tracked bundle
   (`plugins/dist/qtap-plugin-anthropic/index.js` sets `"anthropic-version":
   "2026-10-21"` in the SDK's default headers; `VERSION = "1.4.0"`). ⚠ ledger
   detail: the explicit `2023-06-01` pin was ADDED by `9f761e157` (when the
   SDK went 0.115 → 1.3) and DROPPED by `544c7ab1e`; v5's
   `provider_manifest/manifests/anthropic.json:17` (generated by
   `gen-provider-manifests.mjs:90`) moves to `2026-10-21`. The builder drops
   sampling params on EVERY model (the regex list survives only for the
   thinking shape, renamed `BUDGET_THINKING_REJECTED_MODELS`) and relocates
   assistant-turn documents into a preceding user turn.
7. **SQLite3MC 2.4.0 changes nothing v5 touches on disk** (no default-cipher
   change, no format change; the WAL-rekey fix and the VLE functions do not
   reach a TRUNCATE-journal, never-rekeyed v5). SQLite 3.53.3 / 3.53.4 are
   bugfix releases. **Recommend BUMP** (R-1): CLAUDE.md's rule is "version
   matching what v4 bundles", the cost is one cold ~4-min recompile per
   target, and the sys crate's own comment says a source change is the one
   legitimate bump.
8. **`quilltap db integrity` is still REFUSED in v5** (`db_cmd.rs:42`,
   `cli_differential.rs:2829`), so `70d2859d8` moves only the captured
   `db_help.txt` bytes (`:60-64`, `:108`). A ruling (R-3) on porting the
   now-correct verb.
9. **IMPORT-DEAD at `624d0d25c`: SEVEN harness sites across nine files, not
   three** (§7), and one of the ledger's three is a false positive
   (`cases/inventory-types.ts` imports `renameInventoryFolderSchema`, which
   still exists at `lib/schemas/inventory.types.ts:263`). New: `computeRenameTarget`
   moved (`a3d53eae1`), `tagSseChunk` deleted (`604a082c4`), the wardrobe
   transfers `GET` deleted (`a0ed5ecad` — three cases, RUNTIME-dead:
   `TypeError: GET is not a function`), and a MOCK-INERT class (eight doc
   cases neutralise a `triggerReindexIfNeeded` that no longer exists).
10. **`qtap-export.schema.json` and `schema-key-order.json` are touched by
    FOUR commits across three surveys** (`db6190b44` `:900`, `d439f6dc9`
    `:1333-1346` — which also adds the missing `supportsImageUpload`,
    `b09ca46bb`, `c539c82e0`). ONE owner re-vendors / re-dumps each whole
    (the P4.D341 rule).

---

## §1 The 22nd + 23rd D23 moves (and the two fresh-only moves)

### 1.1 `chat_messages."isEstablishedFact"` (`db6190b44`, the 22nd)

**v4 at the pin:**

- `lib/schemas/chat.types.ts:387` — `MessageEventSchema.isEstablishedFact:
  z.boolean().nullable().optional()`, between `isSilentMessage` and
  `systemSender`.
- `lib/database/repositories/chats-messages.ops.ts:77` —
  `ChatMessageRowSchema.isEstablishedFact: z.union([z.boolean(),
  z.number().transform(v => v === 1)]).nullable().optional()` — the ROW schema
  generateDDL renders (→ `TEXT`, §0.1).
- `migrations/scripts/add-established-fact-field-v1.ts` (whole, 68 lines):
  `dependsOn: ['sqlite-initial-schema-v1']`; `shouldRun` = SQLite ∧ table ∧
  ¬column (`:33-37`); `addColumnIfMissing('chat_messages',
  'isEstablishedFact', 'INTEGER DEFAULT NULL')` (`:42`); INFO `Added
  chat_messages.isEstablishedFact column` `{context}` only when added
  (`:43-45`); ERROR `Failed to add isEstablishedFact column` (`:57`). No
  backfill.
- Registration `migrations/scripts/index.ts:465` (import), `:958`
  (array), `:1424` (export). `lib/startup/prettify.ts:132` label (NO-PORT, the
  standing rule).
- `lib/chat/apply-chat-continuation.ts:238` — a continuation copies
  `isEstablishedFact: source.isEstablishedFact ?? null`;
  `lib/chat/transcript-projection.ts:239` — `event.isEstablishedFact || null`.
- `public/schemas/qtap-export.schema.json:900` — the message record's
  `isEstablishedFact` (between `isSilentMessage` and `systemSender`); `:897`
  `systemKind`'s description gains the `'established-fact'` sentence.
- Carriers: NO backup / restore / import hunk. v4's tests
  (`restore-field-fidelity.test.ts`, `quilltap-import-service.test.ts`) prove
  the flag rides the generic message copy (`addMessage(…)` receives
  `isEstablishedFact: true`).
- No SQL anywhere filters on the column (`git grep` at the pin: every read is
  a JS truthiness test through `lib/chat/established-fact.ts:68-91`).

**v5 surfaces (at `ff1651177`):**

- `crates/quilltap-core/src/db/chats_messages.rs:88-175` `MessageEventInput`
  (serde, NO `deny_unknown_fields` → today a `.qtap` / backup row carrying
  the flag is silently DROPPED) — add `is_established_fact: Option<bool>`
  after `is_silent_message` (`:151`); the member-columns list (`:1070-1127`,
  bind through the generalised `is_silent_stored` → REAL `1.0` / `0.0` /
  NULL); `PRE_P4105_MESSAGE_INSERT` pin (`:1259-1269`) grows.
- `db/chats_messages_read.rs:86-93` `COLUMNS` — APPEND (positional rule,
  `:79-84`) at index 47; emit through the generalised `put_is_silent` right
  after `isSilentMessage` (`:243`) — the emitted position must be measured by
  `chats_messages_read_equivalence` with a planted row (both storage classes:
  INTEGER `1` and TEXT `"1.0"`).
- `services/qtap_export/key_order.rs:230` + `schema-key-order.json`
  `chat_message` (re-dump, §1.5).
- `services/chat_continuation.rs` (the continuation copy — v4's `?? null`
  writes SQL NULL, i.e. no observable key).
- Ported by: the message store P2 / P4.105 (member columns), P4.D171 (the
  `routeTrail` two-shape precedent, `db/chat_messages_route_trail_repair.rs`),
  P4.D341 (`storytellerBid`, the most recent append at 46).

### 1.2 `connection_profiles."maxImagesPerRequest"` (`d439f6dc9`, the 23rd)

**v4 at the pin:**

- `lib/schemas/profile.types.ts:43` `DEFAULT_MAX_IMAGES_PER_REQUEST = 4`,
  `:45` `MAX_IMAGES_PER_REQUEST_CEILING = 50`, `:169`
  `maxImagesPerRequest: z.number().int().min(1).max(50).default(4)` between
  `supportsImageUpload` and `tags`. Integer min/max → `mapToSQLiteType`
  answers `INTEGER` (`schema-translator.ts:322-328`) → fresh shape
  `"maxImagesPerRequest" INTEGER DEFAULT 4` (predicted; the re-dump decides).
- `migrations/scripts/add-profile-max-images-per-request-v1.ts` (whole, 77):
  `dependsOn: ['add-profile-supports-image-upload-field-v1']`; `shouldRun` =
  SQLite ∧ table ∧ ¬column; `addColumnIfMissing(…, 'INTEGER DEFAULT 4')`
  (`:43`) THEN `UPDATE "connection_profiles" SET "maxImagesPerRequest" = 4
  WHERE "maxImagesPerRequest" IS NULL` (`:44-46`) → INFO `Added
  connection_profiles.maxImagesPerRequest column` `{context, added,
  backfilled}` (`:47-51` — fires on every run, not only on an add);
  `itemsAffected: backfilled`.
- Registration `index.ts:467` / `:960` / `:1425`; prettify `:133`.
- `lib/llm/connection-profile-utils.ts:154` `resolveMaxImagesPerRequest`
  (the ONE reader; junk → 4) and `:169` `parseMaxImagesPerRequest` (integer
  or numeric string, 1..50, else `null`).
- `lib/llm/connection-profile-legacy-fields.ts:96-98` — the restore / import
  seed: `undefined` → 4 (a `null` passes through and Zod then refuses it — a
  corpus row to add).
- Routes: `app/api/v1/connection-profiles/route.ts:187-194` (create: absent /
  `null` → 4; else parse; refusal `maxImagesPerRequest must be a whole number
  from 1 to 50`), `:339` (written); `[id]/route.ts:385-392` (PUT: only when
  `!== undefined`, same sentence).
- `public/schemas/qtap-export.schema.json:1333-1346` — the profile record
  gains `supportsImageUpload` (never there before) AND `maxImagesPerRequest`.
- `docs/developer/DDL.md:1238-1246` (lists it LAST — the migration shape).

**v5 surfaces:**

- `db/connection_profiles.rs` — `CpCreate` (`:128-183`, add after
  `supports_image_upload` `:176`), `CpUpdate` (`:246`), the INSERT
  (`:284-334`), `update`'s assignments (`:451-454` neighbourhood), the SELECT
  (`CP_COLUMNS_TAIL`, `:591-593`; positional reads `:765-780`), the
  marshal key order (`:648` `cp_schema_key_order`), `create_return_shape`
  (`:815-880`, the `for key in [...]` at `:870`). Recommend the tolerant
  select (`cp_select_columns`, the `multiCharacterPrefill` precedent): a
  `NULL` literal when the column is absent, materialised as `4` (v4's Zod
  default) — every committed fixture predates the column.
- `api/settings.rs` — create (`:1565` neighbourhood, the
  `supportsImageUpload` resolution) and PUT (`:2132-2137` neighbourhood)
  arms with v4's sentence.
- `services/connection_profile_legacy_fields.rs:79-133` (`SeededLegacyFields`
  + the seed), `services/quilltap_import/profiles.rs:112`/`:271-314`,
  `services/backup/restore/orchestrator.rs:471`.
- `services/qtap_export/schema-key-order.json` `connection_profile`;
  `generators/qtap-export.schema.json`.
- Ported by: P4.6e (Settings profiles), P4.D126 (bug 103 legacy seed,
  `c6c625d31`), P4.D135 (the fallback columns — the profile two-shape ensure
  precedent, `db/connection_profiles_fallback_repair.rs`).

### 1.3 The two fresh-only moves (⚠ ledger)

| table | column | fresh position | migration | v5 action |
|---|---|---|---|---|
| `characters` | `"kind" TEXT DEFAULT 'roleplay'` | after `"diagramStyle"`, before `"systemTransparency"` | NONE | re-dump only; `kind` is vault-managed (`vault-overlay/managed-fields.ts` +`'kind'`) — the character-kind survey owns `db/characters.rs:105` |
| `projects` | `"brahmaConnectionProfileId" TEXT` | after `"answerConfirmationOverride"`, before `"storyBackgroundsEnabled"` | NONE | re-dump only; project store key (`PROJECT_STORE_MANAGED_FIELDS`, `project.types.ts:205`) — the Brahma survey owns the reader / writer |

Both are UUID / enum strings → `TEXT`; `kind` has a Zod default → `DEFAULT
'roleplay'`; `brahmaConnectionProfileId` is nullable-optional → bare `TEXT`.
No ensure (v4 has no migration to mirror; a v4-migrated instance never gains
them, and nothing reads the column).

### 1.4 The ensures (two-shape, UNSTAMPED)

- NEW `db/chat_messages_established_fact_repair.rs` — column-gated; the
  migration's clause verbatim; `MIGRATION_ID = "add-established-fact-field-v1"`.
  Template: `db/chat_messages_route_trail_repair.rs`.
- NEW `db/connection_profiles_max_images_repair.rs` — column-gated; ADD
  COLUMN then the backfill UPDATE (both v4 statements, inside one
  SAVEPOINT); report `{added, backfilled}`. Template:
  `db/connection_profiles_fallback_repair.rs`.
- Host wiring (`quilltap-host/src/host.rs`), after the unmarked ensure
  (`:2378-2391`) and before the P4.134 template seed / P4.160 (`:2421`), in
  v4's registration order (strip → established fact → max images; the reindex
  sits elsewhere — §2.4). One v5 INFO per step that changed anything (the
  P4.D341 convention); v4's migration lines and prettify labels NO-PORT.
- `restore_vintage_state` mirror-boot (`quilltap-harness/tests/
  restore_vintage_state.rs:216-241` — a new `=== P4.Dxxx ===` block after
  P4.D341's): BOTH column ensures are REQUIRED — the restore's message and
  profile INSERTs will name the new columns (the memory note
  `vintage-restore-mirror-boot-list`).
- `zero_migrations_rerun_equivalence.rs:162-215` + `harness/oracle/cases/
  zero-rerun.ts` + `harness/oracle/lib/v4-migrations.ts` (a NEW
  `PIN_624_MIGRATIONS` array after `KEYSTONE_236_MIGRATIONS` `:173`): +4 ids.
  Expected today (red-first): v4's narrowed runner RUNS all four on `planted`;
  after v5's chain it runs NONE; v5 stamps exactly `strip-anthropic-…` (and
  the reindex only on the base where it ran).
- `test_support.rs` twin (`ensure_keystone_schema_on`, `:306-340`, the
  P4.D341 shape) + an oracle plant helper (`plantKeystoneColumns`'s shape,
  `v4-migrations.ts:280-330`) running v4's REAL modules on a per-run copy.
- A host-boot test (`quilltap-host/tests/host_boot_p4d341_keystone_schema.rs`
  is the template).
- Per-ensure differentials (`memory_keystone_ensure_equivalence.rs` /
  `cases/memory-keystone-ensure.ts` are the closest recipe: v4's REAL
  migration vs v5's ensure, `table_info` + `sqlite_master` byte-equal).

### 1.5 The provisioning + carrier artifacts (ONE re-dump each)

- `services/provisioning/fresh_schema.json` — `dump-fresh-schema.ts` from the
  pin (P4.D341 Tier 1 item 2 recipe, `p4.d341-…md:628-633`). Expected `main`
  deltas: EXACTLY `chat_messages`, `connection_profiles`, `characters`,
  `projects` (§1.1-1.3). ANY other delta → STOP.
- `migration_indexes.json` — `dump-migration-indexes.ts` against a REAL first
  boot at `624d0d25c` (the `surveys/2026-10-06-p4.153-…` recipe). Expected:
  NO new index (none of the four migrations creates one); only
  `source.v4Commit` moves.
- `chat_settings_seed.json` — expected UNCHANGED (`brahmaConsole.
  inlineConnectionProfileId` is nullable-optional with no default); confirm.
- `services/qtap_export/schema-key-order.json` — `dump-export-key-order.ts`
  WHOLE: `chat_message` +`isEstablishedFact`, `connection_profile`
  +`maxImagesPerRequest` (after `supportsImageUpload`), `character` +`kind`,
  `project` +`brahmaConnectionProfileId`. Counts pinned in `key_order.rs`
  tests (`:200-240`).
- `generators/qtap-export.schema.json` — re-vendor WHOLE from the pin (four
  commits' lines).

### 1.6 Where the behaviour lives (NOT this survey's)

- Established facts: `lib/chat/established-fact.ts`, `lib/chat/context/
  established-fact-reminder.ts`, `context-builder.service.ts:944-1154`,
  `orchestrator.service.ts:132-146` / `:647` / `:777`, `operator-post.service.ts:
  54-98`, `turn-transcript.ts:303`, `courier-transport.service.ts:253`,
  `in-scene-voiced.ts:155-170`, `message-attribution.ts:77` / `:178`,
  `handlers/established-fact-memories.ts`, `job.types.ts`
  `ESTABLISHED_FACT_MEMORIES`, `app/api/v1/chats/[id]/schemas.ts`, the four
  extraction prompts, the SPA composer / `MessageRow`.
- The image cap: `adaptMessagesForProfile` (deleted again by `29c1e2690` →
  `lib/chat/image-transport-budget.ts`), the Lantern walk, `provider-failover.
  service.ts`, `ProfileModal.tsx` / `useProfileForm.ts` / `types.ts`.

---

## §2 `902c6efe9` — the FTS5 character-only reindex

### 2.1 v4 at the pin

- `lib/database/backends/sqlite/chat-message-fts.ts:101-107` — the
  eligibility fragment gains `AND ${p}"systemSender" IS NULL AND
  ${p}"customAnnouncer" IS NULL` (mirror of `isCharacterAuthoredMessage`,
  `lib/chat/chat-activity.ts:41-48`).
- `:130` — the INSERT trigger's `WHEN` is now `${chatMessageFtsEligibilitySql
  ('new')}` (the text changes from three to five conjuncts).
- `:168-176` `expectedTriggerSql()` (the statement minus `IF NOT EXISTS `);
  `:184-192` NEW `staleChatMessageFtsTriggers(db)` (compares
  `sqlite_master.sql` per trigger name); `:204-215`
  `ensureChatMessageFtsSchema` DROPs every stale trigger first, DEBUG
  `Replacing outdated chat message FTS triggers` `{stale}` (`:207`).
- `lib/database/repositories/chats-search.ops.ts:89` — the FTS probe
  re-applies the fragment on the joined row (`mm`); the non-SQLite arm uses
  `isCharacterAuthoredMessage(msg)`. (`:28-60`, `:259` — `participantId` on
  every hit — is `c539c82e0`'s, the Brahma survey's.)
- `migrations/scripts/reindex-chat-message-fts-character-only.ts` (whole, 98;
  the FILE carries no `-v1`, the id does): `dependsOn` the FTS create +
  `add-system-sender-field-v1` + `add-custom-announcer-field-v1`; `shouldRun`
  `:52-60`; `run` `:62-95`: stale list, count indexed before,
  `ensureChatMessageFtsSchema`, INFO `Rebuilding chat message search index
  without Staff messages` `{context, replacedTriggers, indexedBefore}`,
  rebuild with progress, INFO `Chat message search index rebuilt` `{context,
  scanned, indexed, total, dropped}`; message `Removed N Staff message(s) from
  the search index` / `No Staff messages were in the search index`.
- `migrations/scripts/create-chat-message-fts.ts:47-50` — `dependsOn` grows
  the two column migrations (ordering only; inert for v5).
- Registration `index.ts:463` / `:956` / `:1423`; prettify `:238`.
- The startup reconcile is UNCHANGED in code; it reads the new fragment.

### 2.2 v5 surfaces

- `db/chat_message_fts.rs:105-127` (the fragment + docblock), `:143-150` (the
  trigger text — RECORDED, not transcribed: regenerate
  `harness/oracle/fixtures/chat-message-fts-ddl.json` from v4's module and
  re-pin), `:186-195` `ensure_chat_message_fts_schema` (+ stale replace +
  DEBUG with `staleJson`), a NEW `stale_chat_message_fts_triggers`.
- `db/chat_message_fts_reconcile.rs:144` — no code change once the fragment
  moves (the ledger's ping-pong closes there).
- `db/chats_search.rs:110-127` — `build_fts_search_sql` gains `AND
  {eligibility('mm')}` (v4 `:89`); the LIKE fallback (`:137-153`) already
  reads the fragment.
- `quilltap-host/src/host.rs:1700-1729` (the P4.D204 reconcile block).
- Ported by P4.D204 (`p4.d204-fts5-message-search.md`, `eb7b75e07`) on
  P4.D203's codec.

### 2.3 Fixture fallout (red-first measurement)

`chat_message_fts.rs:363-365` `REDUCED_DDL`, `chats_search.rs:494-512`'s
fixture, and the `chat-message-fts.json` / `chat-message-fts-reconcile.json`
/ `chat-message-update-fts.json` corpora lack `systemSender` /
`customAnnouncer` → every seed INSERT fails once the new trigger exists.
Widen them with the two columns (and add Staff + announcer rows so the
eligibility change is DISCRIMINATED, not merely survived).

### 2.4 The ensure — slot and stamp (R-6)

Port the migration as NEW `db/chat_message_fts_character_only_reindex.rs`,
run IMMEDIATELY BEFORE `reconcile_chat_message_fts` (`host.rs:1727`) — v4's
PHASE 1 precedes PHASE 3.65. Semantics = v4's runner: ledger row present →
skip; `shouldRun` false → skip WITHOUT a stamp; true → run + stamp v4's exact
row (`stamp_completed`, `db/migrations_ledger.rs:57`). On a pre-round v5
instance the first boot then logs v4's two INFO lines and the reconcile stays
quiet. Its two INFO lines are boot-visible on real data — recommend PORTING
them (unlike the column ensures), since they replace the reconcile WARN the
human would otherwise see; the progress callback NO-PORT.

### 2.5 Families that move (v4 drift, not v5 bugs)

`chat_message_fts_equivalence` (DDL fixture + `sqlite_master` text),
`chat_message_fts_reconcile_equivalence` (eligible counts),
`chat_message_update_fts_tier2_equivalence`, `chats_search_equivalence`
(Staff hits vanish; and `participantId` from `c539c82e0`), `ui_search_
equivalence`, `host_boot_chat_message_fts`. Recipe for the new ensure:
`cases/chat-message-fts-reconcile.test.ts` + the per-ensure pattern of §1.4.

---

## §3 `544c7ab1e` — Anthropic `2026-10-21`, no sampling parameters

### 3.1 Plugin (the request builder)

- `plugins/dist/qtap-plugin-anthropic/provider.ts:12-27` — no
  `anthropic-version` in `clientHeaders()` (only `User-Agent`); the SDK
  1.4.0's default `2026-10-21` applies (measured in the tracked bundle).
- `:102-113` `BUDGET_THINKING_REJECTED_MODELS` (the SAME seven regexes,
  renamed) now gates ONLY adaptive vs `budget_tokens` thinking.
- `:346-361` — an assistant turn's `document` blocks move into a user turn
  pushed just before it, DEBUG `Moving assistant-message documents into a
  preceding user turn` `{context, documentCount}`; images stay.
  `:373-377` — an assistant turn left with no blocks and no text is SKIPPED.
- The two `temperature` / `top_p` / `1.0` blocks (send `:471-488`, stream
  `:655-668` at the base) are DELETED — no sampling key on any model.
- `index.ts:60` `capabilities.samplingParameters: false`; `:137` the
  thinking field's `helpText` loses "temperature and top_p are left at their
  defaults (extended thinking requires this), and".
- `packages/plugin-types/src/plugins/provider.ts:158`
  `ProviderCapabilities.samplingParameters?: boolean` (plugin-types 2.10.0).

v5: `model/request_builder/anthropic.rs:8-23` (module doc), `:33-58` (the
list → rename, thinking-only), `:370-373`, `:444-453` (DELETE the sampling
block), `format_messages` (`:119-260`, the relocation + the empty-assistant
skip); `provider_manifest/manifests/anthropic.json:17` (`2026-10-21`), `:28`
(`"samplingParameters": false`), `:91` (helpText);
`harness/oracle/providers/gen-provider-manifests.mjs:90` (the hard-coded
extra header) and `:417-423` (carry `samplingParameters`);
`provider_manifest/mod.rs:148-157` `Capabilities` gains `#[serde(default)]
sampling_parameters: Option<bool>` + an `accepts_sampling_parameters()`
accessor (absent → true; unknown provider → true, v4
`provider-validation.ts:101-103`); `model/provider_auth.rs:27` doc;
`api/provider_actions.rs:777` (asserts `["2023-06-01"]`). Ported by W4.7c /
P4.11 / P4.13 (builders), P4.D158 (Opus 5 on the list).

### 3.2 Server

- `lib/llm/sampling-params.ts:71` `SAMPLING_PARAMETER_KEYS = ['temperature',
  'top_p', 'topP', 'top_k', 'topK']`, `:80-92` `withoutSamplingParameters`
  (shallow copy; `removed` in KEY-LIST order, only keys present via `in`).
  v5: `sampling_params.rs` (P4.D83) — tier-1 rows in
  `cases/sampling-params.ts` / `sampling_params_equivalence`.
- `lib/plugins/provider-validation.ts:101` `acceptsSamplingParameters`.
- `lib/database/repositories/connection-profiles.repository.ts:28-44`
  `sanitizeParametersForProvider` + DEBUG `Stripped sampling parameters the
  provider does not accept` `{context: 'ConnectionProfilesRepository',
  profileId, provider, removed}`; `create` `:99-104` (only when
  `data.parameters` is truthy; `profileId` UNDEFINED on create — the key is
  absent from the line); `update` `:122-142` (when `parameters` OR `provider`
  is in the patch; a `findById` read only when one of the two is missing; the
  patch replaced only when the blob CHANGED identity). v5:
  `db/connection_profiles.rs:270` / `:340` — the ONE chokepoint for editor,
  auto-configure, `.qtap` import, restore, delete-all reseed.

### 3.3 The migration (R-7)

`migrations/scripts/strip-anthropic-sampling-parameters-v1.ts` (whole, 144):
`dependsOn: []`; `shouldRun` `:62-71`; `run` `:76-127` — SELECT `"id",
"parameters" … WHERE upper("provider") IN (?)` bound `'ANTHROPIC'`; a
lenient parse (`null` / non-object / bad JSON → skipped); rewrite with
`JSON.stringify(parameters)` (key order of the original minus the removed);
DEBUG per row `Stripped sampling parameters from Anthropic profile`
`{context: 'migration.strip-anthropic-sampling-parameters', profileId,
removed}`; INFO `Stripped sampling parameters from Anthropic connection
profiles` `{context, examined, cleaned}`; message `Examined N Anthropic
profile(s); removed sampling parameters from M`. NEW
`db/connection_profiles_sampling_strip_heal.rs`, STAMPED (template
`db/memory_rewrite_heals.rs`, R-E), slot 1 of §1.4's chain. The FROZEN list
stays frozen (v4's comment, `:15-19`) — do NOT read the manifest.

### 3.4 Live-data note

Until ported, a v5 write of an Anthropic profile re-adds the keys (the
ledger's ping-pong) — the repository strip closes it. A v5 that has moved the
header but not the builder 400s on every Anthropic turn: the header, the
builder and the strip must land in ONE lane.

### 3.5 Families that move

`request_builder_equivalence` (every anthropic row: no sampling keys; header
`anthropic-version: 2026-10-21`; Stainless `1.4.0`; the document relocation
needs NEW rows — `record-request-envelopes.mjs:300` is the recorder),
`stream_decoders_equivalence` / the stream fixtures (re-record),
`providers_listing_equivalence` (capabilities), `sync_manifest_sidecar_
equivalence` (helpText / options-schema bytes),
`connection_profiles_tier2_equivalence` (`cases/connection-profiles-tier2.ts`
— the strip on create / update / provider change), `profile_routes_
equivalence`, `settings_routes_equivalence`, `sampling_params_equivalence`
(new fn), `system_restore_equivalence` / `quilltap_import` families (an
Anthropic profile with sampling keys restores stripped), and a NEW
strip-ensure differential (v4's real migration vs v5's heal).

### 3.6 SPA half (the settings SPA lane's — listed)

v4 `components/settings/connection-profiles/types.ts:60`, `ProfileList.tsx:
66-72` / `:167`, `ProfileCard.tsx:17-18` / `:160-169` (Max Tokens only),
`ProfileModal.tsx:749-813` (the sentence `This provider chooses its own
temperature and Top P; only the token ceiling is yours to set.`, one-column
grid), `hooks/useProfileForm.ts:41` / `:46-60` (`outboundSamplingParameters`
on save AND test-message). v5: `apps/web/src/app/screens/settings/providers/
profile-modal.ts`, `profile-form.ts:16-92` (`TOP_LEVEL_PARAMETER_KEYS`),
`connection-profiles-card.ts:131-132`, `core/core-contract.ts:4251`
(`ProviderInfo.capabilities.samplingParameters?`). Collides with the
image-cap survey's profile-editor field (`ProfileModal.tsx` / `useProfileForm.
ts` / `types.ts`) — one SPA lane.

---

## §4 `8832cfd20` — SQLite3MC 2.4.0 / SQLite 3.53.4

- v4 at the pin: root `better-sqlite3` = `npm:better-sqlite3-multiple-ciphers@
  ^13.0.3` (lock 13.0.3); `packages/quilltap/package.json:40` the same.
  SQLite3MC **2.4.0** / SQLite **3.53.4** (ledger-measured from the
  installed tree); `deps/defines.gypi` byte-identical to 12.11.1 (STAT4 /
  DQS=0 notes stand).
- 2.4.0's notes (utelle release): based on SQLite 3.53.4; regenerated build
  files; WAL-mode rekey now keeps the key salt (previously could corrupt);
  NEW VLE SQL functions `sqlite3mc_vle_key` / `_encrypt` / `_decrypt`. NO
  default-cipher change, no listed format change, no removed feature.
  SQLite 3.53.3 (2026-06-26) and 3.53.4 (2026-07-24): bugfix releases for
  3.53.0-.2.
- v5: `crates/quilltap-sqlite3mc-sys/vendor/` (2.3.5 / 3.53.2, `SQLITE_SOURCE_ID
  2026-06-03 …`), `Cargo.toml:9` (`0.1.0`, PINNED), `:15` and `build.rs:21`
  / `:39-44` ("2.3.x … @12.11.1"). Mentions to update: `CLAUDE.md` (cipher
  paragraph), `.claude/commands/commit.md:30`, `docs/developer/porting/
  phase-0.md`, `db/chat_settings_impersonation_voice_mode_repair.rs:48` /
  `:431` (comments). Nothing in v5 asserts `sqlite_version()`.
- **What a bump takes:** drop the 2.4.0 amalgamation `.c` / `.h` into
  `vendor/`; bump the sys crate to `0.2.0` (the fingerprint needs it — its own
  rule); update the docblocks; one ~4-min cold compile per target (main's
  `target/` via `/cleanup`'s cycle; lanes clone it). No flag change.
- **Risk of NOT bumping:** low. v5 never uses WAL (TRUNCATE) or rekey; both
  libraries open each other's files (same sqleet defaults). The cost is a
  standing, documented build difference beside STAT4 / DQS and a CLAUDE.md
  rule that is false as written.
- **The INFO line** (`lib/database/backends/sqlite/client.ts:103`, `:115-133`):
  `SQLCipher driver loaded` `{driverVersion, sqliteVersion}` after `SQLite
  database connection established` on the MAIN client only; DEBUG `Could not
  read SQLCipher driver versions` `{error}`. v5 emits no main-client
  "connection established" line at all (only mount index `db/runtime.rs:311`
  and llm-logs `:361`, pinned in `host_boot_hardness.rs:1793-1799`). R-2.
- **Tier R:** the pin's CLI floor install brings the 13.0.3 driver (prebuilt
  Node-API binaries; `native-modules.js` now LOADS and classifies instead of
  rebuilding); `bin/quilltap.js` stderr warnings only on a load failure — no
  byte move on a healthy run. Measure once (the script prints `cli driver …
  | sqlite … | cipher …`).

---

## §5 `70d2859d8` — `quilltap db integrity`

v4 `packages/quilltap/lib/db-commands.js:1730-1787`: `integrity_check` only;
a thrown page read → the single line `page read failed (<code|error>):
<message>`; output `  integrity_check: ok` (one space, was eight);
`{target, ok, openable, integrityCheck, issues, durationMs}` —
`cipherIntegrityCheck` GONE. Help: `bin/quilltap.js:806-809`, `:855`.
v5: the verb is REFUSED (`crates/quilltap-cli/src/db_cmd.rs:26-43`; pinned
`tests/cli_differential.rs:2826-2840`); `src/help/db_help.txt:60-64`, `:108`
are captured bytes (Tier R's `db --help` row) → re-capture. R-3.

---

## §6 SDK moves and the pin floors

| package | location | at `236c04e35` | at `624d0d25c` | moved by | guard constant |
|---|---|---|---|---|---|
| `openai` | 6 plugin dirs | ^7.23.0 | **^7.47.0** | `9f761e157` (7.45.1), `8f83041aa` | `RECORDED_OPENAI_SDK` `:116` |
| `@anthropic-ai/sdk` | anthropic | ^0.115.0 | **^1.4.0** | `9f761e157` (1.3.0), `8f83041aa` | `RECORDED_ANTHROPIC_SDK` `:118` |
| `@google/genai` | google | ^1.52.0 | **^2.32.0** | `9f761e157` | `RECORDED_GOOGLE_GENAI_SDK` `:120` |
| `@openrouter/sdk` | openrouter | ^1.3.28 | **^1.5.34** | `9f761e157` (1.5.21), `8f83041aa` | `RECORDED_OPENROUTER_SDK` `:122` |
| `@quilltap/plugin-utils` | 14 plugin dirs | ^2.7.1 | **^2.7.3** | `9f761e157` (2.7.2), `8f83041aa` | `RECORDED_PLUGIN_UTILS` `:145` |
| `openai` | root (lock) | 7.45.0 | **7.47.0** | `f56965ef9`, `d439f6dc9` | `INSTALLED_ROOT_OPENAI_SDK` `:132` |
| `@openrouter/sdk` | root (lock) | 1.5.9 | **1.5.34** | `f56965ef9`, `d439f6dc9`, `8f83041aa` | `INSTALLED_ROOT_OPENROUTER_SDK` `:138` |
| `better-sqlite3-multiple-ciphers` | CLI | ^12.11.1 | **^13.0.3** | `8832cfd20` | — (the script's print) |
| `@quilltap/plugin-types` | root + plugins | 2.9.0 / ^2.8.0 | 2.10.0 | `9f761e157`, `544c7ab1e` | type-only |
| `zod` | root | 4.6.7 | 4.6.7 | — | `zod_version_guard` unmoved (the MCP plugin moves to ^4.6.7 — no v5 twin) |
| `pdfjs-dist` | root | 6.4.299 | 6.5.438 | `d439f6dc9` | none (the PDF-stack oracles; R-C stands) |

`packages/plugin-utils/src` changed ONLY its version stamp (2.7.1 → 2.7.4)
in the span, and no plugin `.ts` except anthropic's changed — so the
openai / openrouter / google re-records should move STAMPS only, except
whatever genai 2.x changed on the wire (measure first).
`.claude/pin-plugin-sdks.sh` needs no code change; its header comment
(`:5-10`) names the old pin, and the printed floors must be re-checked
against the guard at the new pin (its own instruction, `:18-19`). Re-record
recipes: `harness/oracle/providers/regenerate-{request-envelopes,stream-
fixtures,response-bodies,text-errors,tool-wire,google-wire,image-fixtures}.sh`
+ `record-moderation-wire.mjs` / `record-web-search-wire.mjs`.

---

## §7 The IMPORT-DEAD census at `624d0d25c`

Method: every `@/…` specifier in `harness/oracle/**` (990 unique) resolved
against `git ls-tree 624d0d25c` (two dead modules), plus every `export`
REMOVED in `236c04e35..624d0d25c` (diff of `-export` lines not re-added in the
same file) grepped over `harness/oracle/**` (static, dynamic and mock
factories), each hit then checked for a surviving export at the pin.

| # | consumer | dead import | killed by | class | re-aim (stage-0, harness-only) | family |
|---|---|---|---|---|---|---|
| 1 | `cases/file-attachment-tier3.test.ts:444`, `:486`, `:518` | `@/lib/chat/message-attachment-adapter` (`adaptMessagesForProfile`, `collectAttachmentMimeTypes`) | `29c1e2690` (DELETED) | IMPORT-DEAD (dynamic — dies at section E) | `collectAttachmentMimeTypes` → `@/lib/chat/image-transport-budget` (`:170`, body identical); `adaptMessagesForProfile(m, p, repos, userId, logCtx, opts)` → `applyImageTransportBudget(m, p, {repos, userId, logContext, describeOptions})` (`:359`), project `.messages`; the moved rows are the image-budget survey's drift reds | `file_attachment_tier3_equivalence` |
| 2 | `cases/item-mutations-tier2.ts:146`, `:234-244`; `fixtures/build-item-mutations-fixture.ts` (doc only) | `@/lib/wardrobe/folder-mutations` (`create/rename/deleteWardrobeFolder`) | `600c2906f` (DELETED) | IMPORT-DEAD (dynamic) | `createItemFolder(WARDROBE_FOLDER_KIND, location, path)` / `renameItemFolder(kind, location, path, name)` / `deleteItemFolder(kind, location, path)` from `@/lib/mount-index/item-folder-location` (`:96-135`), kind from `@/lib/wardrobe/item-mutations:234` | `item_mutations_tier2_equivalence` |
| 3 | `cases/inventory-keystone-tier2.ts:266-272` | `createInventoryFolder` / `renameInventoryFolder` / `deleteInventoryFolder` (`@/lib/inventory/item-mutations`) | `134961572` | RUNTIME-dead (namespace import → `undefined`) | the same three generic fns with `INVENTORY_FOLDER_KIND` (`@/lib/inventory/item-mutations:617`) | `inventory_keystone_tier2_equivalence` |
| 4 | `cases/item-folders.ts:68`, `:309` | `normalizeInventoryFolderName` (`@/lib/inventory/folders`) | `134961572` | RUNTIME-dead (static named import, but tsx runs v4 as CommonJS — the binding is `undefined`, and the case wraps the call in `attempt()`, so it EMITS a `TypeError` row instead of crashing: a silent wrong row) | `normalizeItemFolderName(INVENTORY_FOLDER_RULES, input, InventoryFolderPathError-ctor)` (`@/lib/mount-index/item-folders:141`); check the error-ctor argument against `:141-158` | `inventory_folders_equivalence` |
| 5 | `cases/documents-rename-target.ts:16` | `computeRenameTarget` / `RenameTarget` (`@/lib/documents/operator-doc-actions`) | `a3d53eae1` (MOVED) | RUNTIME-dead under tsx-CJS (`computeRenameTarget` is `undefined` → TypeError at the first row) | `@/lib/documents/core` (re-exported at `core/index.ts:24`, `:28`; defined `core/move.ts:86`) — pure re-path; byte drift, if any, is the document survey's | `documents_rename_target_equivalence` |
| 6 | `cases/storyteller-pure.ts:37`, `:119-125`; `fixtures/storyteller-pure.json` | `tagSseChunk` (`@/lib/storyteller/sse-tag`) | `604a082c4` (DELETED with `taggingController`) | RUNTIME-dead under tsx-CJS (TypeError at the first `tagSseChunk` row) | remove the `tagSseChunk` section + its corpus rows (the Salon-frames survey owns the replacement and deletes v5's twin); keep `sinkController` | `storyteller_pure_equivalence` |
| 7 | `cases/wardrobe-transfers.test.ts:203-214`, `cases/wardrobe-routes.test.ts:332-335`, `cases/degraded-mount-routes.test.ts:639-651` | `GET` of `@/app/api/v1/wardrobe/transfers/route` | `a0ed5ecad` (route keeps `POST` only) | RUNTIME-dead (`GET` undefined → TypeError) | drop the `__destinations` synthetic row, the routes `GET` case and the two `transfers_destinations*` rows; v5 still serves the GET (the wardrobe survey removes it) | `wardrobe_transfers_tier2_equivalence`, `group_wardrobe_routes_equivalence`/wardrobe routes, `degraded_mount_routes_equivalence` |
| 8 | `cases/doc-{enum,ui,blob,fs,fm,opacity,text}.test.ts`, `cases/project-roster-access.test.ts:173` | `triggerReindexIfNeeded` (a jest mock-factory OVERRIDE of `@/lib/tools/handlers/doc-edit/shared`) | `a3d53eae1` | MOCK-INERT (loads; the neutralisation no longer reaches anything — the reindex moved into the document core, so an un-awaited tail may now escape) | find the new trigger in `lib/documents/core/*` at the pin and mock THAT; until then record it, do not delete | 8 doc families |

**⚠ ledger corrections:** `cases/inventory-types.ts` is NOT dead (its
`renameInventoryFolderSchema` lives at `lib/schemas/inventory.types.ts:263`);
rows 5-8 were missing. Not dead but moved by the pin: `harness/oracle/
provision/dump-prompt-templates.ts:17-18` documents `MODERN_GENERAL.md`,
which `d0201575a` renamed to `MODERN_ROLEPLAY.md` (the dump's OUTPUT moves —
the prompts survey's; no import breaks). The `inventory|wardrobe/[itemId]/
images` routes still export `GET` / `POST` (now `createItemImagesRoute`).

---

## §8 Families this slice moves (summary)

- Schema: `provisioning_equivalence` (1a-1d; `QT_FRESH_SCHEMA_LIVE` makes 1d
  the tripwire), `migration_index_backfill_equivalence` +
  `host_boot_fresh_indexes` / `host_boot_backfilled_indexes` (source commit
  only), `zero_migrations_rerun_equivalence` (+4 RAN on `planted`),
  `restore_vintage_state`, `system_export_equivalence` (four key-order kinds),
  `qtap_schema_validate_equivalence` / `qtap_schema_embed_guard`,
  `connection_profiles_tier2_equivalence` / `profile_routes_equivalence` /
  `settings_routes_equivalence` (`maxImagesPerRequest: 4` on EVERY profile
  read, schema position), `connection_profile_legacy_fields_equivalence`,
  `system_backup_equivalence` / `system_restore_equivalence` (profiles),
  every family emitting a profile DTO (`fallback_engine`, `orchestrator_tier3`,
  `vision_request_tier3`, `web_search_runner_wire`, `pascal_*_route`),
  `chats_messages_read_equivalence` / `chats_messages_tier2_equivalence`
  (only with a planted flag row — add one, both storage classes).
- FTS: §2.5. Anthropic: §3.5. SDK: every provider corpus + the guard (§6).
  CLI: `cli_differential` `db --help`. Harness: §7's eight.
- Closest recipes: `cases/memory-keystone-ensure.ts` (+ its Rust twin) for
  each ensure; `cases/connection-profiles-tier2.ts` for the strip;
  `cases/chat-message-fts-reconcile.test.ts` for the reindex;
  `cases/sampling-params.ts` for `withoutSamplingParameters`;
  `cases/connection-profile-legacy-fields.ts` for the seed;
  `cases/profile-routes.test.ts` for the 1..50 refusals.

---

## §9 Lane-cut proposal

**L0 — REAIM (stage 0, harness-only, from `main`).** Owns only §7's files:
`cases/file-attachment-tier3.test.ts`, `cases/item-mutations-tier2.ts`,
`cases/inventory-keystone-tier2.ts`, `cases/item-folders.ts`,
`cases/documents-rename-target.ts`, `cases/storyteller-pure.ts` (+
`fixtures/storyteller-pure.json`), `cases/wardrobe-transfers.test.ts`,
`cases/wardrobe-routes.test.ts`, `cases/degraded-mount-routes.test.ts`
(+ the paired Rust tests' expected-row lists where a row is removed). Row 8
is RECORD-ONLY. Precedent: P4.D314. Each re-aim proven by the oracle LOADING
at the pin; the drift reds it exposes are recorded, not fixed.

**K — SCHEMA KEYSTONE (wave 0, the stacking root).** Owns:
`services/provisioning/{fresh_schema,migration_indexes,chat_settings_seed}.json`
+ `provisioning/mod.rs`; `services/qtap_export/schema-key-order.json` +
`key_order.rs` tests; `generators/qtap-export.schema.json`; the two column
ensures (§1.4) + the STAMPED strip heal (§3.3) + `sampling_params.rs`'s
`without_sampling_parameters` / `SAMPLING_PARAMETER_KEYS`; `host.rs`'s
236-chain neighbourhood (`:2320-2420`); `restore_vintage_state.rs`;
`zero_migrations_rerun_equivalence.rs` + `cases/zero-rerun.ts` +
`lib/v4-migrations.ts`; `test_support.rs` twins; the column plumbing in
`db/chats_messages.rs`, `db/chats_messages_read.rs`,
`db/connection_profiles.rs` (incl. the repository STRIP hunk — it needs only
the manifest accessor below), `services/connection_profile_legacy_fields.rs`,
`services/quilltap_import/profiles.rs`, `services/backup/restore/
orchestrator.rs` (profile seed), `services/chat_continuation.rs` (the copy);
`api/settings.rs` (the `maxImagesPerRequest` validation arms only);
`provider_manifest/mod.rs` (`Capabilities.sampling_parameters` + accessor)
and `manifests/anthropic.json`'s `capabilities` key ONLY.
Ends Tier 1 in one `KEYSTONE:` commit.

**F — FTS (wave 1, stacks on K).** Owns `db/chat_message_fts.rs`,
`db/chat_message_fts_reconcile.rs` (docs), NEW `db/chat_message_fts_
character_only_reindex.rs`, `host.rs:1700-1729` (its own hunk, disjoint from
K's), `db/chats_search.rs` (the eligibility line ONLY), the FTS fixtures and
oracle cases of §2.3 / §2.5, and appends its id to K's zero-rerun (sequential
after K, so no conflict).

**P — PROVIDERS (wave 1, stacks on K).** Owns `model/request_builder/
anthropic.rs`, `manifests/anthropic.json` (header + helpText),
`gen-provider-manifests.mjs`, `model/provider_auth.rs` (doc),
`api/provider_actions.rs` (test), `provider_sdk_version_guard.rs`, every
recorded provider corpus + its recorder / `regenerate-*.sh`, the
`.claude/pin-plugin-sdks.sh` header. The genai 2.x measurement is its FIRST
step. ⚠ Collides with the image-budget survey (`29c1e2690`'s `streamMessage`
tag strip and failover re-adapt touch the same request path) — order P
before or after that lane, never parallel on `request_builder/*`.

**D — DRIVER + CLI (wave 0, independent).** Owns
`crates/quilltap-sqlite3mc-sys/**` (R-1), `.claude/commands/commit.md:30`,
the cipher paragraph of `CLAUDE.md` (the unifier merges it with the status
edits), `phase-0.md`'s version note, `crates/quilltap-cli/src/help/
db_help.txt`, `src/db_cmd.rs` + `tests/cli_differential.rs` (only if R-3
ports the verb), and — only if R-2 says PORT — `db/runtime.rs` +
`host_boot_hardness.rs`.

**Collisions with other areas:**
- `qtap-export.schema.json` / `schema-key-order.json`: K owns both WHOLE;
  the character-kind and Brahma lanes CONSUME them (the P4.D349 pattern —
  a transient `system_export_equivalence` red until the union).
- `db/chats_search.rs`: F owns the eligibility line; the Brahma lane owns
  `participantId` on hits (`c539c82e0`) — stack Brahma after F or give the
  whole file to one of them.
- `db/characters.rs` `MANAGED_FIELDS` (`kind`) and the project store key are
  the character-kind / Brahma lanes'; K only re-dumps.
- `db/connection_profiles.rs`: K's maxImages + strip vs the image-budget
  lane's `resolveMaxImagesPerRequest` reader — the reader goes in a new
  helper the image lane owns; K only carries the column.
- `host.rs`: K (`:2320-2420`), F (`:1700-1729`), and any behaviour lane that
  adds a boot step — disjoint hunks, named in each order.
- The settings SPA (`profile-modal.ts`, `profile-form.ts`,
  `connection-profiles-card.ts`): §3.6 + the image-cap field → ONE SPA lane.
- `help/` (`connection-profiles.md`, `search.md`, `deus-ex-machina.md`) and
  `docs/v4/` (`DDL.md`, `DATABASE_ENCRYPTION.md`, `CHANGELOG_V4.md`): the
  vendor lane's.

---

## §10 Rulings for the human (recommendation first)

- **R-1 — SQLite3MC 2.4.0.** (A, recommended) BUMP: vendor the 2.4.0
  amalgamation, sys crate `0.1.0` → `0.2.0`, docblocks + CLAUDE.md +
  `commit.md` to 2.4.0 / 3.53.4, one cold compile; prove by a real-instance
  open of a post-`8832cfd20` Friday COPY in the owed dogfood pass. (B) HOLD
  at 2.3.5 and record `SQLITE3MC_2_3_5_HELD` beside STAT4 / DQS, correcting
  CLAUDE.md's "matching what v4 bundles".
- **R-2 — the `SQLCipher driver loaded` INFO.** (A, recommended) NO-PORT,
  recorded: v5 has no Node driver and emits no main-client "connection
  established" line for it to follow. (B) a v5-shaped line (`driverVersion`
  = the amalgamation's `SQLITE3MC_VERSION_STRING`) beside the two partition
  lines.
- **R-3 — `quilltap db integrity`.** (A, recommended) PORT it now in lane D
  (≈45 lines of v4, read-only, Tier R diffable, and the upstream bug that
  justified waiting is fixed); (B) keep it refused and re-capture only the
  help bytes.
- **R-4 — the two fresh-only moves (`characters.kind`,
  `projects.brahmaConnectionProfileId`).** (A, recommended) carry them in the
  ONE re-dump (D23 — never hand-edit) with NO ensure, as `canDrawDiagrams`
  was; (B) none — leaving them out would make 1d red forever.
- **R-5 — `isEstablishedFact` storage.** (A, recommended) byte parity with
  v4: REAL `1.0` / `0.0` into the column on both shapes (TEXT `"1.0"` on a
  fresh instance, INTEGER-affinity `1` on a migrated one) through the
  generalised `isSilentMessage` codec.
- **R-6 — the reindex ensure.** (A, recommended) v4's runner semantics (skip
  if stamped; skip unstamped when `shouldRun` is false; run + stamp when
  true), slotted BEFORE the reconcile, with v4's two INFO lines; (B)
  UNSTAMPED, every boot (log divergence on a later rebuild); (C) no ensure,
  reconcile only (the first boot logs the reconcile WARN instead).
- **R-7 — the strip migration.** (A, recommended) STAMPED heal (R-E class),
  runs on fresh instances too (examined 0) so v4 never re-runs it; the
  frozen `['ANTHROPIC']` list kept.
- **R-8 — the provider re-record.** (A, recommended) re-record EVERY corpus
  at the pin floors in lane P (unavoidable once the pin's floors are
  installed); measure genai 2.x first and stop if the wire framing moved
  beyond stamps. (B) a split: stamps-only re-record now, genai framing as a
  follow-up order.
- **R-9 — absent-column reads on committed fixtures.** (A, recommended)
  tolerant SELECT for `connection_profiles` (`NULL` literal → `4`, the
  `multiCharacterPrefill` precedent — profiles appear in nearly every
  fixture) and the P4.D341 per-case plant for `chat_messages`; (B) widen
  every committed pair through v4's real migrations (the P4.D341 Tier 1
  path — heavier).
