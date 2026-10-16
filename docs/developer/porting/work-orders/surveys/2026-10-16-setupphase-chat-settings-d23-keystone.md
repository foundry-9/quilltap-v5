# Survey — the `25b8fab6a` round's CHAT-SETTINGS KEYSTONE: the TENTH + ELEVENTH D23 moves (`chat_settings.customToolTriggers` + `chat_settings.composerSlashCommands`, ONE re-dump), the two migrations re-homed as boot ensures, the six-sites adoption, the settings PUT arms, the `help_settings` / Almanack / restore carriers, and the harness families that move

Read-only planning survey, **2026-10-16**. v4 pin **`25b8fab6a`**
(`25b8fab6aa58636c6885ba63684c7f8fbfca9c3b`, `4.11.0-dev.10`, v4 `main` HEAD,
clean — read ONLY through `git show 25b8fab6a:<path>` / `git show <sha> --
<paths>` / `git diff 83334b9a5 25b8fab6a -- <paths>`); oracle baseline
**`83334b9a5`** (what v5 has ported); v5 `main` at **`56554d98c`**
(`/Volumes/External 1/source_overflow/quilltap-v5`; `~/source/quilltap-v5`
symlinks to it). Every v4 line number below is AT THE PIN; every v5 line number
is at main `56554d98c`. Nothing was built, run or written in either repo except
this file. Port facts come from SHIPPED HUNKS (ledger §5.3), never from commit
messages. This survey builds on the ledger's §1 "Schema / D23" block
(`drift-ledger.md:143-186`) and its §3 rows for `362426c8f` and `4877fc2da`
(`:316`, `:320`) — it does not restate them; where a hunk disagrees with a row,
§9 says so.

Shape follows `surveys/2026-10-13-setupphase-memory-schema-keystone.md`, at a
depth proportional to this much smaller slice (two one-column migrations, two
boolean arms, three carriers).

---

## 0. The span for this slice, in one paragraph — and the definitive DDL answer

Two commits past the baseline move `chat_settings`' DDL, and nothing else does.
`362426c8f` (2026-10-15, "feat(pascal): trigger phrases run custom tools when
their words are said", `4.11.0-dev.1→.3`) adds `ChatSettingsSchema.
customToolTriggers: z.boolean().default(false)` (`lib/schemas/settings.types.ts:806`)
between `customTools` (`:800`) and `compositionModeDefault` (`:808`), the
migration `add-custom-tool-triggers-field-v1` (NEW
`migrations/scripts/add-custom-tool-triggers-field.ts`, 95 lines), its
registration (`migrations/scripts/index.ts:445` import, `:909` array, `:1350`
re-export), the repository seed key (`chat-settings.repository.ts:194`), the
PUT arm (`app/api/v1/settings/chat/route.ts:54, :177-182, :406, :443`), the
`help_settings` chat-bag key (`lib/tools/handlers/help-settings-handler.ts:113`),
the Almanack triple (`lib/tools/almanack/types.ts:277`, `phase3-ledgers.ts:630,
:737`, `render.ts:727` + the snapshot line), `prettify.ts:188`, `DDL.md` (+1
`chat_settings` line, +`invokedBy`/`triggerPhrase` prose on `chat_messages.
pascalMeta`), and two tests (`__tests__/unit/app/api/v1/settings/chat/route.
custom-tool-triggers.test.ts`, 49 lines; `__tests__/unit/lib/database/migration/
registry.test.ts` +24). `4877fc2da` (2026-10-15, "feat(salon): slash commands in
the composer", `4.11.0-dev.6`) adds `composerSlashCommands: z.boolean().
default(true)` (`settings.types.ts:816`) between `composerUnicode` (`:814`) and
`impersonationVoiceMode` (`:818`), the migration `add-composer-slash-commands-
field-v1` (NEW `migrations/scripts/add-composer-slash-commands-field.ts`, 84
lines), its registration (`index.ts:447`, `:911`, `:1352` — immediately AFTER
the triggers one), the seed key (`chat-settings.repository.ts:199`), the PUT arm
(`route.ts:64, :252-257, :416, :453`), `prettify.ts:150`, `DDL.md` (+1 line).
No test file in `4877fc2da` touches the server surface (its four tests are SPA
component tests). **Definitively, from `git diff 83334b9a5 25b8fab6a -- lib/
schemas lib/database migrations/scripts docs/developer/DDL.md`:** the only
`lib/schemas/*.ts` hunks in the span are `settings.types.ts` (+8: the two
columns), `chat.types.ts` (+20/−8: `MessageEventSchema.pascalMeta`'s
`invokedBy` enum widened to `['llm','user','trigger']` + `triggerPhrase:
z.string().optional()` + comments — a JSON-column shape, NO DDL),
`memory-extraction-cursor.types.ts` (−2: the dead `EXTRACTION_CURSOR_PASSES`
export, `7417e9a1b`), `wardrobe.types.ts` (+23/−2: request-body Zod for
`move-items` / `copy-items`, `bbbcd2388`); `lib/database/schema-translator.ts`
(the generateDDL source) is UNCHANGED; the only other `migrations/scripts`
hunk is a doc-path comment in `collapse-duplicate-avatar-rolls-v1.ts`; and
`DDL.md`'s span diff is exactly the two `chat_settings` lines, the `pascalMeta`
comment, and one `generationKey` prose line. **The two `chat_settings`
columns are the ONLY DDL moves between `83334b9a5` and the pin.** One
`fresh_schema.json` re-dump from a pin at or past `4877fc2da` carries both
(`25b8fab6a` is such a pin); `migration_indexes.json` does NOT move (neither
migration makes an index).

---

## 1. v4 AT THE PIN — the hunks, file by file

### 1.1 `lib/schemas/settings.types.ts` — `ChatSettingsSchema` (`:731-882`), full key order at the pin

The Zod object's declaration order IS generateDDL's column order AND the
`JSON.stringify(zodParsed)` key order every v4 read emits (the `chat_settings.rs`
header's "nested JSON key-order discipline"). At the pin, in order (line: key):

```
731 export const ChatSettingsSchema = z.object({
732   id, 733 userId,
734   avatarDisplayMode: AvatarDisplayModeEnum.default('ALWAYS'),
735   avatarDisplayStyle: z.string().default('CIRCULAR'),
736   tagStyles: TagStyleMapSchema,
738   cheapLLMSettings: CheapLLMSettingsSchema.default({…}),
745   imageDescriptionProfileId: UUIDSchema.nullable().optional(),
747   defaultRoleplayTemplateId: UUIDSchema.nullable().optional(),
749   themePreference: ThemePreferenceSchema.default({…}),
755   sidebarWidth: z.number().min(256).max(512).default(256).optional(),
757   defaultTimestampConfig, 765 memoryCascadePreferences,
770   autonomousRoomSettings, 777 tokenDisplaySettings,
784   contextCompressionSettings, 792 llmLoggingSettings,
798   autoDetectRng: z.boolean().default(true),
800   customTools: z.boolean().default(true),
806   customToolTriggers: z.boolean().default(false),        ← TENTH move
808   compositionModeDefault: z.boolean().default(false),
810   composerSpellcheck: z.boolean().default(true),
812   composerEmoji: z.boolean().default(true),
814   composerUnicode: z.boolean().default(true),
816   composerSlashCommands: z.boolean().default(true),       ← ELEVENTH move
818   impersonationVoiceMode: ImpersonationVoiceModeEnum.default('off'),
820   textReplacementsEnabled: z.boolean().default(true),
822   autoScrollOnResponseComplete: z.boolean().default(false),
824   agentModeSettings, 829 coreWhisper, 837 thinkingDisplay,
842   answerConfirmationSettings, 846 smartTypographySettings,
852   storyBackgroundsSettings,
857   wardrobeImageSettings: WardrobeImageSettingsSchema.optional(),
859   conciergeSettings, 874 autoLockSettings,
879   timezone: z.string().nullable().optional(),
880   createdAt, 881 updatedAt,
882 });
```

The two new doc comments (verbatim, for the Rust field docs): `customToolTriggers`
— "Run a custom tool automatically, with its defaults, when a message from
anyone in a chat contains one of its declared `triggers` phrases (default:
false). Dead while `customTools` is off." (`:801-805`); `composerSlashCommands`
— "Whether `/` at the start of a word opens the slash-command menu in the Salon
composer (default: true). Document Mode never has it. The gutter buttons and the
sidebar's Run Tool are NOT gated by this — an explicit button press is never a
surprise." (`:815`).

### 1.2 What generateDDL renders for `chat_settings` at the pin — PREDICTED statement, column by column

`lib/database/schema-translator.ts` (unchanged in the span): a `ZodBoolean`
maps to `INTEGER` (`:329-330`); a field with a `.default()` gets `DEFAULT
<formatDefaultValue>` and NO `NOT NULL` (`:349-360`); a boolean default
formats as `'1'` / `'0'` (`:377-378`); columns come out in `Object.entries(
shape)` order (`:237-240`); `generateCreateTable` emits `CREATE TABLE IF NOT
EXISTS "<name>" (\n  <col>,\n…\n)` (`:401-409`). The committed dump
(`crates/quilltap-core/src/services/provisioning/fresh_schema.json:10`) stores
the `sqlite_master` text (`CREATE TABLE "chat_settings" (` — the dumper reads
the created table back, so `IF NOT EXISTS` is gone). Today's statement has 37
columns (cid 0 `id` … cid 36 `updatedAt`); the re-dump will have **39**. The
two moved lines, in their exact spelling and position (cid = `PRAGMA
table_info` ordinal):

```
  "autoDetectRng" INTEGER DEFAULT 1,                cid 16  (unchanged)
  "customTools" INTEGER DEFAULT 1,                  cid 17  (unchanged)
  "customToolTriggers" INTEGER DEFAULT 0,           cid 18  NEW
  "compositionModeDefault" INTEGER DEFAULT 0,       cid 19  (was 18)
  "composerSpellcheck" INTEGER DEFAULT 1,           cid 20  (was 19)
  "composerEmoji" INTEGER DEFAULT 1,                cid 21  (was 20)
  "composerUnicode" INTEGER DEFAULT 1,              cid 22  (was 21)
  "composerSlashCommands" INTEGER DEFAULT 1,        cid 23  NEW
  "impersonationVoiceMode" TEXT DEFAULT 'off',      cid 24  (was 22)
  "textReplacementsEnabled" INTEGER DEFAULT 1,      cid 25  (was 23)
  "autoScrollOnResponseComplete" INTEGER DEFAULT 0, cid 26  (was 24)
  … every later column +2 …
  "timezone" TEXT,                                  cid 36  (was 34)
  "createdAt" TEXT NOT NULL,                        cid 37  (was 35)
  "updatedAt" TEXT NOT NULL                         cid 38  (was 36)
```

**ONE shape this time, for both columns** (the `impersonationVoiceMode`
precedent, NOT the P4.D249 `NOT NULL` pair nor the `textShapeVersion` REAL/INTEGER
pair): generateDDL's `INTEGER DEFAULT 0` / `INTEGER DEFAULT 1` is byte-equal to
the migrations' `addColumnIfMissing(…, 'INTEGER DEFAULT 0')` / `'INTEGER DEFAULT
1'` clauses; only the POSITION differs (the migration appends). Confirm by the
re-dump (D23 — never by hand). Both DDL defaults equal the Zod defaults AND the
repository seed values, so a row written by an app that does not bind the
column reads the same value either way.

### 1.3 `migrations/scripts/add-custom-tool-triggers-field.ts` (95 lines) — verbatim facts

- `id: 'add-custom-tool-triggers-field-v1'` (`:25`); `description: 'Add
  customToolTriggers field to chat_settings table for Pascal trigger phrases'`
  (`:26`); `introducedInVersion: '4.11.0'` (`:27`); `dependsOn:
  ['add-custom-tools-field-v1']` (`:28`).
- `shouldRun()` (`:30-43`): `isSQLiteBackend()` ∧ `sqliteTableExists('chat_
  settings')` ∧ `!sqliteColumnExists('chat_settings', 'customToolTriggers')`.
  Table-gated; a `shouldRun` false is "skipped AND NOT RECORDED" by the runner
  (re-asked every boot — the model survey §1.1 step 3).
- `run()` (`:45-94`): `addColumnIfMissing('chat_settings', 'customToolTriggers',
  'INTEGER DEFAULT 0')` (`:52`) — `migrations/lib/database-utils.ts:347-360`:
  `columnExists` → `false`, else DEBUG `Adding column` `{context:
  'migrations.database-utils', table, column}` then
  `db.exec('ALTER TABLE "chat_settings" ADD COLUMN "customToolTriggers" INTEGER DEFAULT 0')`.
  No backfill. **Log lines, verbatim:** INFO `Added customToolTriggers column to
  chat_settings table` `{ context: 'migration.add-custom-tool-triggers-field' }`
  inside the `if` (`:54-56`) AND the SAME text again after it `{ context,
  columnsAdded, durationMs }` (`:61-65`) — TWICE on a real add, ONCE (the
  summary, `columnsAdded: 0`) when nothing was added; ERROR `Failed to add
  customToolTriggers column` `{ context, error: errorMessage }` (`:79-82`).
  Result `{ id, success: true, itemsAffected: columnsAdded, message: 'Added
  customToolTriggers column to chat_settings table', durationMs, timestamp }`
  (`:67-74`) / failure `message: 'Failed to add customToolTriggers column'`,
  `error` (`:84-92`). Not `resumable` → a failure stops v4's boot.
- `prettify.ts:188`: `'add-custom-tool-triggers-field-v1': 'Teaching Pascal which
  words to listen for'`.

### 1.4 `migrations/scripts/add-composer-slash-commands-field.ts` (84 lines) — verbatim facts

- `id: 'add-composer-slash-commands-field-v1'` (`:24`); `description: 'Add
  composerSlashCommands field to chat_settings table for the composer
  slash-command typeahead toggle'` (`:25-26`); `introducedInVersion: '4.11.0'`
  (`:27`); `dependsOn: ['sqlite-initial-schema-v1']` (`:28`).
- `shouldRun()` (`:30-40`): the same three gates on `composerSlashCommands`.
- `run()` (`:42-83`): `addColumnIfMissing('chat_settings', 'composerSlashCommands',
  'INTEGER DEFAULT 1')` (`:47`) → `ALTER TABLE "chat_settings" ADD COLUMN
  "composerSlashCommands" INTEGER DEFAULT 1`. **Log lines, verbatim:** INFO
  `Added composerSlashCommands column to chat_settings table` `{ context:
  'migration.add-composer-slash-commands-field' }` inside the `if` ONLY
  (`:49-51`) — this migration has NO trailing summary INFO (asymmetric with 1.3;
  the ledger's two rows are each right about their own file); ERROR `Failed to
  add composerSlashCommands column` `{ context, error }` (`:68-71`). Result
  `message: 'Added composerSlashCommands column to chat_settings table'` /
  `'Failed to add composerSlashCommands column'`.
- `prettify.ts:150`: `'add-composer-slash-commands-field-v1': 'Fitting the
  composer with a speaking-tube to the tool room'`.

### 1.5 `migrations/scripts/index.ts` — registration order at the pin

Imports `:443` `dropHousekeepingConsolidationJobsMigration`, `:445`
`addCustomToolTriggersFieldMigration` (comment `// Pascal trigger phrases:
customToolTriggers toggle column on chat_settings`), `:447`
`addComposerSlashCommandsFieldMigration` (`// Composer slash commands: …`).
The `migrations[]` array (`:455-912`) ends `…, :907
dropHousekeepingConsolidationJobsMigration, :909
addCustomToolTriggersFieldMigration, :911 addComposerSlashCommandsFieldMigration
]` — i.e. BOTH are appended after the twelve P4.D288 span migrations
(v5's `db::p4d288_span::run_span_ensures`, slot 12 = `drop-housekeeping-
consolidation-jobs-v1`), triggers FIRST, slash SECOND. Each `dependsOn` is
earlier in the array, so registration order IS run order
(`sortMigrationsByDependency`, the model survey §1.1). The re-export block
(`:1348-1352`) mirrors it. **v5's boot order must be: … `run_span_ensures` →
triggers ensure → slash ensure → the P4.160 index-family backfill.**

### 1.6 `lib/database/repositories/chat-settings.repository.ts` — the `updateForUser` create seed (`:129-230`)

`updateForUser` (`:129`): `findByUserId`; absent → `defaultSettings` literal
(`:140-…`), whose boolean block at the pin reads `:192 autoDetectRng: true,
:193 customTools: true, :194 customToolTriggers: false, :195
compositionModeDefault: false, :196 composerSpellcheck: true, :197
composerEmoji: true, :198 composerUnicode: true, :199 composerSlashCommands:
true, :200 impersonationVoiceMode: 'off', :201 textReplacementsEnabled: true,
:202 autoScrollOnResponseComplete: false`. This literal is what the dumper's
`getOrCreateSingleUser` captures into `chat_settings_seed.json` (§2.1), so the
seed gains `"customToolTriggers": 0` and `"composerSlashCommands": 1` as
captured, never by hand. `create`/`update`/`delete` (`:66/:90/:…`) are the
unchanged thin base-repo wrappers (v4's `_update` writes `$set: validated` —
the WHOLE entity — which is why a committed fixture lacking a column kills an
oracle regen, §5).

### 1.7 `app/api/v1/settings/chat/route.ts` — the PUT (`:378-473`) and `updateChatSettings` (`:38-341`)

- Signature (`:38-70`): the positional parameter list gains `customToolTriggers?:
  boolean` at `:54` (after `customTools` `:53`, before `agentModeSettings`) and
  `composerSlashCommands?: boolean` at `:64` (after `composerUnicode` `:63`,
  before `impersonationVoiceMode` `:65`).
- The arms, verbatim (`:171-182` / `:246-257`):
  ```ts
  if (typeof customToolTriggers !== 'undefined') {
    if (typeof customToolTriggers !== 'boolean') {
      throw new Error('Invalid customToolTriggers value (must be boolean)')
    }
    updateData.customToolTriggers = customToolTriggers
  }
  …
  if (typeof composerSlashCommands !== 'undefined') {
    if (typeof composerSlashCommands !== 'boolean') {
      throw new Error('Invalid composerSlashCommands value (must be boolean)')
    }
    updateData.composerSlashCommands = composerSlashCommands
  }
  ```
  The arms run in PARAMETER order (`:72-340`): a body with two bad keys gets the
  EARLIER arm's sentence — `customToolTriggers` before `agentModeSettings` (and
  after `customTools`), `composerSlashCommands` after `composerUnicode` and
  BEFORE `impersonationVoiceMode`. An explicit `null` is `typeof 'object'` →
  the arm RUNS and refuses (the P4.D251 semantics, same class).
- The PUT (`:378`): `findRetiredConciergeKeys` first (`:384-390`, the 400
  `Invalid settings: … replaced by conciergeSettings`); destructure (`:392-434`
  — `customToolTriggers` `:406`, `composerSlashCommands` `:416`); call
  (`:436-461`); `successResponse(chatSettings)` (`:463`) = the FULL row
  `updateForUser` returns; `catch` (`:465-472`): ERROR `[Settings v1] Error
  updating chat settings` `{userId}` + the error, then `status =
  errorMessage.includes('Invalid') ? 400 : 500` → `badRequest(errorMessage)`
  = `{ error: 'Invalid customToolTriggers value (must be boolean)' }` /
  `{ error: 'Invalid composerSlashCommands value (must be boolean)' }`, 400.
- The GET (`:348-375`): `findByUserId` → absent → `updateForUser(user.id, {4
  keys})` (the seed fills the rest, incl. both new keys at their defaults) →
  `successResponse(chatSettings)`; a present row echoes
  `customToolTriggers`/`composerSlashCommands` as parsed through
  `ChatSettingsSchema` (an absent column → `false` / `true`).

### 1.8 `lib/tools/handlers/help-settings-handler.ts` — `fetchCategorySettings('chat')` (`:101-119`)

The chat bag at the pin, in emit order: `tokenDisplaySettings,
contextCompressionSettings, memoryCascadePreferences, defaultTimestampConfig,
agentModeSettings, autoDetectRng, customTools, customToolTriggers (:113),
llmLoggingSettings, avatarDisplayMode, avatarDisplayStyle, timezone`. `settings`
is the Zod-parsed row, so `customToolTriggers` is `false` on an instance whose
column is absent. `composerSlashCommands` is NOT in this bag (`4877fc2da` does
not touch the handler).

### 1.9 `lib/tools/almanack/{types,phase3-ledgers,render}.ts` + the snapshot

- `types.ts:277`: `FeatureConfigInfo.customToolTriggers: boolean` right after
  `customTools: boolean` (`:276`).
- `phase3-ledgers.ts:630`: `defaultFeatureConfig()` `customToolTriggers: false`
  (after `customTools: true` `:629`); `:737`: `collectFeatureConfig`
  `customToolTriggers: chatSettings?.customToolTriggers ?? false` (after
  `customTools: chatSettings?.customTools ?? true` `:736`).
- `render.ts:725-731`: under `#### Pascal's Workbench (Custom Tools)`, the line
  `- **Auto-Run on Trigger Phrases**: ${yesNo(fc.customToolTriggers)}` (`:727`)
  BETWEEN `- **Custom tools offered to models**: …` (`:726`) and the
  italic `*(the inventory of definitions lives in the Scriptorium section …)*`
  (`:728-731`). The snapshot (`__tests__/unit/lib/tools/almanack/__snapshots__/
  render.test.ts.snap:321`) adds exactly `- **Auto-Run on Trigger Phrases**: No`.
  The Almanack has NO composer-slash row (`render.ts:749` renders only
  `composerSpellcheck` from the composer family; `4877fc2da` adds none).

### 1.10 `docs/developer/DDL.md` — the two lines (for the `docs/v4/` re-vendor)

`:1048` (after `"composerUnicode" INTEGER DEFAULT 1, …`): `"composerSlashCommands"
INTEGER DEFAULT 1, -- added in 4.11 (add-composer-slash-commands-field-v1):
governs the `/` slash-command menu in the Salon composer (never Document Mode);
the gutter buttons and the sidebar's Run Tool are NOT gated by it`; `:1057`
(after `"customTools" INTEGER DEFAULT 1, …`): `"customToolTriggers" INTEGER
DEFAULT 0, -- added in 4.11 (add-custom-tool-triggers-field-v1): when 1, a
message from anyone in a chat that contains one of a custom tool's declared
`triggers` phrases runs that tool with its defaults (pascalMeta.invokedBy =
'trigger'). Dead while customTools is 0. Off by default.` Note `DDL.md`'s
hand-written `chat_settings` block lists columns in MIGRATION order (the
composer family at `:1045-1048`, `customTools` at `:1055`) — it is prose, not
the dump. v5's mirror `docs/v4/developer/DDL.md:1047-1055` is at the
`83334b9a5` vintage (neither line present).

### 1.11 The tests `362426c8f` ships for this slice, and what they pin

- `__tests__/unit/app/api/v1/settings/chat/route.custom-tool-triggers.test.ts`
  (49 lines; repos mocked — `updateForUser` echoes `{userId, ...data}`): (i)
  `it.each([true, false])` → `PUT {customToolTriggers: v}` answers 200, the
  repo was asked to write `{customToolTriggers: v}`, the body echoes it
  (`:32-38`); (ii) `{customToolTriggers: 'yes'}` → status ≥ 400 and
  `updateForUser` NOT called (`:40-44`); (iii) `ChatSettingsSchema.shape.
  customToolTriggers.parse(undefined) === false` (`:46-48`). All three are
  subsumed by `settings_routes_equivalence` rows over v4's REAL route + repo
  (§4.2) — the mocked test itself is NO-PORT (tests-only).
- `__tests__/unit/lib/database/migration/registry.test.ts` (+24, `:69-89`): an
  acyclic-`dependsOn` guard (DFS; `expect(cycles).toEqual([])`) added because
  the triggers migration "once named itself in `dependsOn`" (a pre-commit
  slip; the shipped file says `['add-custom-tools-field-v1']`). v5 has no
  migration registry — NO-PORT; v5's analogue of "the chain is wired" is the
  host-boot order + `zero_migrations_rerun_equivalence` (§4.4).
- `4877fc2da` ships NO server test for the slash arm; its oracle coverage is
  new `settings_routes` rows (§4.2).

### 1.12 Where the two keys are READ in v4 (the sibling lanes' consumers — for §7)

`lib/pascal/trigger-runner.ts` `triggersEnabled(cs) = cs?.customTools !== false
&& cs?.customToolTriggers === true` (the ledger row's (c)) — the Pascal
trigger lane reads the parsed row. `composerSlashCommands` is read ONLY by
the SPA (`SlashCommandPlugin` through the chat-settings query; `4877fc2da`
hunk list) — no server consumer. Neither key reaches `.qtap` export/import
(v4's `.qtap` carries no chat settings; confirmed on v5 too, §2.8).

---

## 2. v5 on `main` today (`56554d98c`) — per surface

### 2.1 The D23 artifacts + provisioning

- `crates/quilltap-core/src/services/provisioning/fresh_schema.json:10` — the
  `chat_settings` statement at the `83334b9a5` vintage (37 columns; §1.2 lists
  today's cids). Dumped by `harness/oracle/provision/dump-fresh-schema.ts`
  (header recipe `:30-35`: from the v4 checkout under Node 24, `QT_SCHEMA_OUT=…`;
  the seed via `QT_SEED_OUT=…` `:192-210` — captured from v4's REAL
  `getOrCreateSingleUser` row, column order from `PRAGMA table_info`, minus
  `id/userId/createdAt/updatedAt`). The D23 register is the `provisioning/
  mod.rs` doc (`:150-234`, one paragraph per re-dump; the seed register
  `:236-261`) — append the TENTH + ELEVENTH as ONE paragraph.
- `chat_settings_seed.json` — 33 `columns` (0-based: `customTools` 15,
  `composerUnicode` 19, `impersonationVoiceMode` 20) + `values`
  (`"customTools": 1`, `"composerUnicode": 1`). After the re-dump: 35 columns —
  `customToolTriggers` inserted at 16 (value `0`), `composerSlashCommands` at 21
  (value `1`), everything after shifted. Consumed by `provisioning/mod.rs:626-
  658` `seed_chat_settings` (a raw INSERT naming the captured columns — the
  provisioned instance's `chat_settings` table MUST carry both columns, i.e. the
  schema and seed re-dumps land together) and by `db/chat_settings.rs:1766-1792`
  `default_settings_columns()` (the `updateForUser` create branch → every
  v5-created default row carries both at the seed values, through
  `tolerant_insert`, which would silently DROP a column the live table lacks —
  the write half of why the boot ensures are load-bearing).
- `migration_indexes.json` (`source.v4Commit` `83334b9a5…`): UNMOVED — neither
  migration creates an index; no `QT_REAL_BOOT_MASTER` re-dump is owed.
- `crates/quilltap-harness/tests/provisioning_equivalence.rs` — (1a) v5's table
  text vs the committed dump; (1b) column SETS vs v4's REAL first boot
  (`build-provision-oracle.ts` → `migrations-first.ts`: v4's runner over an
  empty dir — at the pin that runner RUNS both new migrations on the
  migration-text `chat_settings`, appending the columns LAST; the SET compare is
  order-blind, so (1b) stays green with no new `TABLE_COLUMN_ASYMMETRY` row; the
  existing rows `:838-851` — `v5: timezone`, `v4: dangerousContentSettings,
  uncensoredImageDescriptionProfileId` — stand); (1d) the committed dump vs a
  LIVE `dump-fresh-schema.ts` run (`QT_FRESH_SCHEMA_LIVE`, REQUIRED, `:296-305`)
  — **RED at the pin until the re-dump lands: the D23 tripwire firing as
  designed**; (2) the seed row compare (`:479-488`, `SELECT * FROM chat_settings`
  minus id/userId/timestamps) — green once both sides carry the keys.
- `provisioning/mod.rs` unit test `provisions_a_bootable_seeded_instance`
  (`:901`) lists table NAMES only — unmoved.

### 2.2 `crates/quilltap-core/src/db/chat_settings.rs` (2,493 lines) — the six sites, with today's positions

- Module doc `:62-65`: "**eight boolean columns** → INTEGER 0/1 … `autoDetectRng`,
  `customTools`, `compositionModeDefault`, `composerSpellcheck`, `composerEmoji`,
  `composerUnicode`, `textReplacementsEnabled`, `autoScrollOnResponseComplete`" →
  becomes TEN.
- `ChatSettingsCreate` (`:786-…`): `custom_tools: bool` (`:815`),
  `composition_mode_default` (`:816`), `composer_spellcheck` (`:817`),
  `composer_emoji: bool` with `#[serde(default = "smart_typography_default_true")]`
  (`:826-827`, the doc `:818-825` — "this deserialize is also restore's input,
  and an archive taken before 4.8.2 simply has no such key; v4's restore parses
  through `ChatSettingsSchema`, where `z.boolean().default(true)` fills it"),
  `composer_unicode: bool` the same way (`:830-831`),
  `impersonation_voice_mode: ImpersonationVoiceMode` with `#[serde(default)]`
  (`:846-847`). **Naming convention, confirmed:** v4 camelCase → snake_case
  (`customTools` → `custom_tools`, `composerUnicode` → `composer_unicode`,
  `cheapLLMSettings` the one explicit rename); so **`custom_tool_triggers`**
  and **`composer_slash_commands`**.
- `ChatSettingsUpdate` (`:940-966`): `custom_tools: Option<bool>` (`:953`),
  `composer_unicode: Option<bool>` (`:957`), `impersonation_voice_mode:
  Option<ImpersonationVoiceMode>` (`:958`).
- `create` (`:1014-1117`): the bool locals `:1051-1059` (`custom_tools`
  `:1052`, `composer_unicode` `:1056`); the `tolerant_insert` column list
  `:1064-1114` (`("customTools", &custom_tools)` `:1089`, `("composerUnicode",
  &composer_unicode)` `:1093`, `("impersonationVoiceMode", …)` `:1094`).
- `update` (`:1120-1255`): `custom_tools` assignment `:1192-1195`,
  `composer_unicode` `:1208-1211`, `impersonation_voice_mode` `:1212-1215`.
- `find_by_user_id` (`:1314-1660`): the tolerant SELECT list `:1360-1398`
  (`"customTools"` `:1378`, `"composerUnicode"` `:1382`,
  `"impersonationVoiceMode"` `:1383`, `"updatedAt"` last `:1397`), then the
  POSITIONAL read `r.get::<_, …>(N)`: `autoDetectRng` 16, **`customTools` 17**
  (`Option<i64>` `.is_none_or(|v| v == 1)` — absent → `true`, `:1476-1491`),
  `compositionModeDefault` 18, `composerSpellcheck` 19, `composerEmoji` 20,
  **`composerUnicode` 21** (absent → `true`, `:1510-1513`),
  `impersonationVoiceMode` 22 (absent/NULL → `'off'`, `:1514-1530`),
  `textReplacementsEnabled` 23, `autoScrollOnResponseComplete` 24,
  `agentModeSettings` 25, `coreWhisper` 26, `thinkingDisplay` 27,
  `answerConfirmationSettings` 28, `smartTypographySettings` 29,
  `storyBackgroundsSettings` 30, `wardrobeImageSettings` 31,
  `conciergeSettings` 32, `autoLockSettings` 33, `timezone` 34, `createdAt`
  35, `updatedAt` 36 (`:1540-1600`). **Inserting the two columns shifts every
  index from 18 on by +1 and from 23 on by +2** (`createdAt` → 37, `updatedAt`
  → 38) — the exact hazard `chat_settings_column_sites_guard` counts for.
  Absent-column semantics for the new keys, from v4's `SELECT *` + Zod default:
  `customToolTriggers` absent → **`false`** (`.is_some_and(|v| v == 1)` — the
  FIRST default-false tolerant boolean; `compositionModeDefault` is
  non-tolerant), `composerSlashCommands` absent → **`true`** (the
  `composerUnicode` shape).
- The whole-row validation after the read (`:1619-1640`): only
  `impersonationVoiceMode`'s enum is checked; booleans need none.
- `update_for_user` (`:1810-1878`): the update branch is a plain `UPDATE … SET
  <col> = ?` over the validated assignments (`:1830-1848` — a PUT naming a
  column the table lacks answers `500 sqlite error: no such column: …`, the
  composer-ensure finding); the create branch is `default_settings_columns()`
  + overrides through `tolerant_insert` (`:1851-1877`).
- `patch_create_return_shape` (`:1890-1909`): re-inserts
  `defaultRoleplayTemplateId: null` before `themePreference` on the create
  return — unaffected.
- Unit-test tables (`:1975-1995`, `:2080-2110`, `:2140-2160`, `:2300-2320`):
  each `CREATE TABLE chat_settings (…)` literal lists `customTools INTEGER, …
  composerUnicode INTEGER, impersonationVoiceMode TEXT` — the tolerant read
  means they keep passing without the new columns; add one test per new column
  in the `find_by_user_id_defaults_custom_tools_when_the_column_is_absent`
  (`:2078`) / `:2124` shape (absent → `false` / `true`).
- `find_all_scheduler_settings` (`:1702`, a named two-column SELECT) and
  `wardrobe_tool_images_enabled` (`:1664`) — unaffected.

### 2.3 The ensure twins (the shape to copy), and the host boot order

- `db/chat_settings_composer_repair.rs` (209 lines): a 3-column
  `COMPOSER_COLUMNS: [(&str, &str); 3]` (`:55-65`, migration clauses verbatim),
  `ensure_chat_settings_composer_columns(main)` (`:72-87`): table-gated, one
  `PRAGMA table_info`, `ALTER TABLE "chat_settings" ADD COLUMN "{col}" {decl}`
  per absent column; own `table_exists` / `column_names` helpers (`:89-103`);
  SILENT (no log line); five unit tests (`:127-208`: adds all, pre-existing row
  reads v4 defaults, half-migrated + idempotent, DDL ↔ JSON const, no table).
  Its header `:34-37` records the pretty labels NO-PORT.
- `db/chat_settings_wardrobe_image_settings_repair.rs` (163 lines) — the
  ONE-column shape: `const COLUMN` / `const DECL` (`:50-51`),
  `ensure_chat_settings_wardrobe_image_settings(main) -> Result<(), DbError>`
  (`:56-70`), silent, "No stamp (R-B)" `:31-36`, NO-PORT block `:38-43`.
- `db/chats_scenario_source_repair.rs` (51 lines) — the P4.D288 shape, through
  the SHARED probes `db/schema_probe.rs` (`pub(crate) fn table_exists :12,
  column_exists :19, column_names :24, index_exists :34, add_column_if_missing
  :44` — v4's `database-utils` in one place): `pub const MIGRATION_ID`,
  `pub const COLUMNS`, the ensure answers `Vec<&'static str>` (the columns
  added) so the HOST logs v5's ONE INFO when non-empty. **This is the shape to
  copy** (newer than the silent composer twin; the chain logs per ensure):
  `db/p4d288_span.rs:43-51` — `tracing::info!(target: "quilltap::boot",
  migrationId = MIGRATION_ID, columnsAdded = n, "Added the chat scenario source
  columns")`.
- `db/chat_settings_impersonation_voice_mode_repair.rs` (440 lines) — the
  three-step twin (ADD + translate + DROP, one transaction, `MODE_DECL`); not
  the shape here (no backfill, no drop), but its header `:32-40` is the
  "ONE shape, measured" paragraph to echo.
- Module registration: `db/mod.rs:50-55` (`chat_settings`,
  `chat_settings_composer_repair` :51, `…_impersonation_voice_mode_repair` :53,
  `…_wardrobe_image_settings_repair` :55); `:119` `chats_scenario_source_repair`;
  `:127` `p4d288_span`.
- **Host boot** `crates/quilltap-host/src/host.rs` `seed_built_ins(db, tz)`
  (`:1305`): the composer ensure `:1361-1363` (P4.D73 block `:1353-1364`), the
  voice-mode ensure `:1383-1384` (P4.D251 block `:1365-1385`), … the wear ledger
  `:2114` + `wardrobeImageSettings` `:2135` (P4.D255 block `:2101-2136`), the
  tiers ensure `:2148-2158` (P4.D265, with v5's ONE INFO `"Fitted the memory
  tiers"`), **the P4.D288 chain `:2160-2170` `quilltap_core::db::p4d288_span::
  run_span_ensures(main, &tz)?` (`:2169`)**, then the P4.160 index backfill
  `:2171-2199`. **Insert the two new ensures between `:2170` (`// === end
  P4.D288 ===`) and `:2171`** — v4's registration order (§1.5) — FATAL on
  failure (`?`), UNSTAMPED (a re-ADD is a no-op: `addColumnIfMissing`'s own
  gate + `shouldRun`'s column gate — the R-B precedent in the wardrobe twin's
  header), each logging v5's ONE INFO when it added. Two homes are open: (a)
  two sibling modules `db/chat_settings_custom_tool_triggers_repair.rs` +
  `db/chat_settings_composer_slash_commands_repair.rs` called from the host
  block, from `restore_vintage_state` and from `zero_migrations_rerun`'s
  `run_chain`; (b) ONE module `db/chat_settings_4_11_repair.rs` with both
  ensures and a `run_both(main)` so the three callers cannot drift (the
  `p4d288_span` lesson: "a hand-copied list drifted once already"). (b) is
  the safer shape; the order inside is fixed (triggers, then slash).
- `restore_vintage_state.rs` (`crates/quilltap-harness/tests/`): the mirror-boot
  list `:96-158` (`ensure_link_group_column` :103, route trail + cycle order
  :117-122, generationKey + transcriptVersion :132-137, tiers :142-146,
  `run_span_ensures` :155-158). It carries NONE of the three `chat_settings`
  column ensures today — and nothing goes red for it: the restore's chat-settings
  phase (`services/backup/restore/orchestrator.rs:1397-1437`) deserializes
  `ChatSettingsCreate` and calls `repo.create`, which is `tolerant_insert`
  (`chat_settings.rs:1064`) — an absent column is DROPPED silently, never a
  `no such column` sentence. So the memory note's red (`table X has no column
  named Y`) cannot fire here; append the new ensures anyway (a restored
  settings row on the vintage instance would otherwise silently lose the two
  keys, and the mirror-boot should mirror the boot).

### 2.4 The ensure differentials (the family to copy) + the zero-rerun proof

- `crates/quilltap-harness/tests/chat_settings_wardrobe_image_settings_ensure_
  equivalence.rs` (125 lines) + `harness/oracle/cases/chat-settings-wardrobe-
  image-settings-ensure.ts` (129 lines) — **the exact template** for a
  one-column `chat_settings` migration with no backfill: the oracle builds
  three base files AT THE PIN from v4's OWN generateDDL (`ensureCollection('chat_
  settings', ChatSettingsSchema)` in a scratch DB, the CREATE read back from
  `sqlite_master`, the ONE column line removed mechanically — `COLUMN_LINE` /
  the `PERMANENT_LINE` idiom, `:37, :71-78`): (A) pre-round + two rows → v4's
  migration RUNS (`+RAN …`), appends the clause; (B) the pin's generateDDL →
  `not needed`; (C) (A) already migrated (the `ADD` applied by hand, `:39,
  :91`) → `not needed`. v4's migration runs through `harness/oracle/lib/
  v4-migrations.ts` `runV4Migrations({dbPath, pepperBase64, migrations})` on a
  COPY (`:102-106`); the comparands per mode are `PRAGMA table_info` (cid /
  name / type / notnull / dflt_value / pk — ORDER included), the table's
  `sqlite_master.sql`, and every row's raw cell (`:109-115`); the Rust side
  (`:83-125`) runs v5's ensure on its own copy, asserts v4's report per mode
  (A `+RAN`, B/C `not needed` — so no mode is vacuous) and byte-compares the
  three. Env `QT_ORACLE_WARDROBE_IMAGE_ENSURE` + `QT_FIXTURE_WARDROBE_IMAGE_
  ENSURE_DIR`; recipe in both headers (`.rs:14-24`, `.ts:20-26`).
- `chat_settings_voice_mode_ensure_equivalence.rs` + `chat-settings-voice-mode-
  ensure.ts` — the three-step twin with mode (b) "NEITHER column — v4 runs BOTH
  migrations in registration order, v5 runs its ONE ensure" (`.ts:21-24`): the
  precedent for a **pair-order mode** (§4.1 mode D).
- `zero_migrations_rerun_equivalence.rs` (`:1-65` header; `run_chain` `:106-
  108` = `run_span_ensures`) + `harness/oracle/cases/zero-rerun.ts` (`build`
  phase plants a pre-round instance from `fresh_schema.json` with the round's
  moves UNDONE `:8-20`; the `v4` phase runs v4's REAL `MigrationRunner`
  NARROWED to `P4D288_SPAN_MIGRATIONS` `:132` — the 13-entry list in
  `harness/oracle/lib/v4-migrations.ts:72-91`, tiers + the twelve). After the
  re-dump the `fresh` / `fresh-used` bases carry both new columns, so a
  widened narrowed list (the two new refs appended, registration order) answers
  `not needed` for both and stamps nothing — the "zero re-run" claim extends
  to the TENTH + ELEVENTH for free; the `planted` base would need the two lines
  stripped as well for the planted mode to exercise them (§4.4).
- `chat_settings_column_sites_guard.rs` (192 lines): `ADOPTED_BOOLEAN_COLUMNS`
  `:31-43` (eight names, grouped by order), `ADOPTED_TEXT_COLUMNS` `:50`,
  `ADOPTED_JSON_COLUMNS` `:58`, `RETIRED_COLUMNS` `:64-68`; three regions
  anchored on `chat_settings.rs` text (`:97-119`: the INSERT list from
  `crate::db::tolerant_insert(\n            self.conn,`; the UPDATE builder from
  `if let Some(auto_detect_rng) = patch.auto_detect_rng` to `if let
  Some(timezone) = &patch.timezone`; the SELECT+read from `let cols =
  crate::db::tolerant_select_list(` to `\npub fn wardrobe_tool_images_enabled`)
  — the quoted name once in INSERT, `<col> = ?` once in UPDATE, the quoted
  name TWICE in SELECT+read (array entry + `obj.insert`). Add both names
  (`// P4.D<this round> (v4 4.11)`), red-first on unported `main`.

### 2.5 The tier-2 repo family + the routes family

- `chat_settings_tier2_equivalence.rs` (header `:1-35`): corpus
  `harness/oracle/fixtures/chat-settings-tier2.json` (three `create` payloads
  carry `composerUnicode` at `:214, :310, :427` with `impersonationVoiceMode`
  beside); the Rust `CreateData` (`:112-120`: `custom_tools: bool`,
  `composer_emoji`, `composer_unicode`, `impersonation_voice_mode`) and
  `UpdateData` (`:176-186`) mirror the struct; the fixture is built at the pin
  by `build-chat-settings-fixture.ts` (`ensureCollection('chat_settings',
  ChatSettingsSchema)` `:78` — so the table carries both new columns
  automatically) and v4's `create` writes the whole validated entity (Zod
  defaults fill absent keys). Both new columns' DDL defaults EQUAL the Zod
  defaults, so a v5 `create` that fails to bind them would STILL dump equal on
  a default-only corpus — the equal-by-default trap; grow the corpus with
  NON-default values (`customToolTriggers: true`, `composerSlashCommands:
  false`) on at least one create and one update so the diff can fail.
- `settings_routes_equivalence.rs` (header `:1-56`; per-family floors
  `:625-700` — `composer_settings >= 10` `:653`, `impersonation_voice >= 12`
  `:669`) + `harness/oracle/cases/settings-routes.test.ts` (the composer family
  `:517-600`: `s_put_composer_toggles`, the smart-typo rows,
  `s_put_composer_emoji_wrong_type` `{composerEmoji: 'yes'}`,
  `s_put_composer_unicode_wrong_type` `{composerUnicode: 1}`, the explicit-null
  bag; the voice family `:796-950` — the GET default, three positive PUTs,
  five refusal arms incl. `null` and the OLD type, the create branch on user
  B). The fixture is rebuilt at the pin by `build-settings-fixture.ts` (user A's
  row through v4's real repos; user B bare). **The rows to add are §4.2.**

### 2.6 `crates/quilltap-core/src/api/settings.rs` — the PUT today

`chat_settings_update` (`:346-395`): the retired-Concierge 400 first (`:354-
365`), `build_settings_assignments` (`:792-1190`) whose `Err(msg)` splits on
`msg.contains("Invalid")` → 400 else 500 (`:368-384`, v4 `route.ts:468`);
`bool_field(v, err_msg)` (`:779-785`: `as_bool()` else the sentence — an
explicit `null` is `Value::Null` → refused, v4's `typeof null !== 'boolean'`).
The arms walk v4's parameter order: `autoDetectRng` `:931-936`, `customTools`
`:937-942`, `agentModeSettings` `:943…`; … `compositionModeDefault` `:1039`,
`composerSpellcheck` `:1045`, `composerEmoji` `:1054`, `composerUnicode`
`:1060-1064`, `impersonationVoiceMode` `:1087-1101` (the enum arm; its
comment `:1065-1086` records the P4.D179 NO-PORT of v4's per-arm `logger.debug`
lines — carry that ruling to the two new arms: v4 adds NO debug line for
either, so nothing to not-port). **Insert `customToolTriggers` after `:942` and
`composerSlashCommands` after `:1064`**, each `bool_field(v, "Invalid
<key> value (must be boolean)")`. The dispatch wire is the raw bag
(`api/types.rs:136-138` `Request::ChatSettingsUpdate { settings: Value }`;
`engine.rs:931-936`) — no column enumeration above the handler; the web
transport and the Tauri shell pass it through (no `quilltap-web/src` or
`quilltap-cli/src` site names a chat-settings column — measured by grep).

### 2.7 `tools/help.rs` + the Almanack

- `crates/quilltap-core/src/tools/help.rs:213-221`: the chat bag's
  `obj.insert("customTools".into(), field(&s, "customTools"))` (`:215`) — add
  `customToolTriggers` right after (v4 `:113`); `field` reads the
  `find_by_user_id` row, so the tolerant default (`false`) flows through.
  Family `help_tools_equivalence` (header `:1-24`: fixture
  `build-help-tools-fixture.ts` `ensureCollection('chat_settings', …)` `:98`,
  oracle `harness/oracle/cases/help-tools.ts`); the `help_settings` output GROWS
  — red at the pin until the key lands (the P4.d6 `customTools` precedent the
  ledger names).
- `almanack/types.rs:514-526` `FeatureConfigInfo` (`#[serde(rename_all =
  "camelCase")]`, `custom_tools: bool` `:526`) → add `custom_tool_triggers:
  bool` after it (serde → `customToolTriggers`); `almanack/phase3_ledgers.rs:
  616` default `custom_tools: true` → `custom_tool_triggers: false` after;
  `:750` `custom_tools: jbool(s, &["customTools"], true)` →
  `custom_tool_triggers: jbool(s, &["customToolTriggers"], false)` after;
  `almanack/render.rs:1115-1122` the Workbench block → `push!(format!("- **
  Auto-Run on Trigger Phrases**: {}", yes_no(fc.custom_tool_triggers)))` after
  the "Custom tools offered to models" line (`:1116-1119`), before the italic.
  Families: `almanack_render_equivalence` (tier-1, byte-exact over v4's
  rendered data — the oracle `almanack-render.test.ts` re-run at the pin emits
  the new key + line; `EXPECTED_CASES` `:23-…` unchanged) and
  `almanack_tier2_equivalence` (over the committed `almanack-{main,mount}.db`
  pair — v4's `collectFeatureConfig` reads `chatSettings?.customToolTriggers ??
  false` on a fixture whose column is ABSENT → `false`; v5's `jbool(…, false)`
  → `false`; equal WITHOUT widening the pair; the `build-almanack-fixture.ts`
  plant `:505-520` sets `autoDetectRng: false` and never `customTools`, so the
  default arms are what both sides exercise).

### 2.8 The carriers

- **Backup**: `services/backup/collect.rs:691-692` collects the row through
  `find_by_user_id` → `chat-settings.json` (`staging.rs:94`) carries whatever
  the read emits — both new keys appear once the read emits them, no carrier
  edit. `manifest.rs:55` counts only.
- **Restore**: `restore/orchestrator.rs:1397-1437` — `translate_restored_chat_
  settings` (`:4192-4233`, the Concierge + voice-toggle legacy translations)
  then `serde_json::from_value::<ChatSettingsCreate>` (`:1409-1420`; a failure
  WARNs `Failed to restore chat settings` with `settingsId` + `error`) then
  `repo.create` (tolerant). So an archive predating the columns deserializes
  ONLY if the two new struct fields carry serde defaults — **`custom_tool_
  triggers` `#[serde(default)]` (false) and `composer_slash_commands`
  `#[serde(default = "smart_typography_default_true")]`** — v4's restore parses
  through `ChatSettingsSchema` where the Zod defaults fill them, so defaulting
  here is the faithful shape (the `composer_emoji` doc `:818-825`). No
  `RESTORE_TS_MESSAGES` census change (no new v4 restore line).
- **`uuid_remap.rs:921-…` `remap_chat_settings`**: scalars/ids only — the two
  booleans pass through `fields_owned` untouched; the committed
  `harness/oracle/fixtures/uuid-remap-corpus.json` carries a settings row with
  `composerUnicode` — optional: add the two keys to that row so the pure remap
  family pins the pass-through (Tier 2).
- **`.qtap`**: `services/qtap_export/` and `services/quilltap_import/` name no
  `chat_settings` / `chatSettings` (grep) — v4's `.qtap` carries no chat
  settings; nothing to do. (`362426c8f` DOES touch `public/schemas/qtap-export.
  schema.json` — for `pascalMeta.invokedBy`/`triggerPhrase`, the Pascal lane's;
  §9.)

### 2.9 The web edge + the SPA (what exists; the sibling lanes own the cards)

- `crates/quilltap-web/tests/chat_settings_composer_web_routes.rs` (header
  `:1-36`): over `common::materialize_fixture_instance()` (the `chat-send-
  {main,mount}.db` pair, `tests/common/mod.rs:96-107`), DROPs the three
  composer columns before boot (`:83-104` — "regardless of the committed
  fixture's vintage, so the ensure is genuinely under test"), boots, asserts
  the columns exist (`:113-122`), GETs the defaults (`:125-129`), PUTs, re-reads,
  refuses by wire. **Add a fourth test in this shape for the two 4.11 columns**
  (DROP both → boot → present; GET `customToolTriggers: false`,
  `composerSlashCommands: true`; PUT `{customToolTriggers: true,
  composerSlashCommands: false}` → re-read; the two refusal sentences by wire;
  an explicit `null` refused).
- SPA: `apps/web/src/app/core/core-contract.ts:3833` `ChatSettingsDto` types
  only a subset of keys (`impersonationVoiceMode?` `:3861`, no `customTools?` /
  `composerUnicode?` — the cards index the row loosely:
  `custom-tools-settings.ts` `this.settings()?.['customTools']`,
  `composer-unicode-settings.ts:77-79` `?.['composerUnicode'] ?? true`). The
  Automation card `screens/settings/chat/automation-settings.ts` renders
  `AUTOMATION_OPTIONS` (`chat-settings.types.ts:138-151`, ONE key
  `autoDetectRng`, `checked = key === 'autoDetectRng' ? … : false`) — the
  trigger lane rewrites it as v4's row table (`customToolTriggers`, disabled
  unless `customTools`); the composer card order `chat-tab.ts:117-121`
  (`qt-composer-emoji-settings`, `qt-composer-unicode-settings`, …
  `qt-impersonation-voice-settings`) — the slash lane inserts
  `qt-composer-slash-commands-settings` after `:118`; the typeahead gate read
  `editor/char-insert/char-insert-settings.ts` (`emoji` / `unicode` signals off
  `chatSettingsKeys.all`) is the slash lane's model for a `slashCommands`
  signal. This keystone delivers the WIRE KEYS (§7); the cards are theirs.

---

## 3. The proposed tier split

### Tier 1 — must land (the keystone; nothing else in the round can be gated without it)

1. **The TENTH + ELEVENTH D23 re-dump as ONE** — `fresh_schema.json` +
   `chat_settings_seed.json` from a pinned worktree at `25b8fab6a`
   (`dump-fresh-schema.ts`, `QT_SCHEMA_OUT` + `QT_SEED_OUT`), the `provisioning/
   mod.rs` D23 register paragraph + the seed register line; the predicted
   movement is EXACTLY two lines in `main`'s `chat_settings` statement and two
   `columns` entries + two `values` in the seed — anything else moving is a
   finding (§6). `migration_indexes.json` untouched.
2. **The two boot ensures** — v4's migration clauses verbatim (`INTEGER DEFAULT
   0` / `INTEGER DEFAULT 1`, appended), table-gated, UNSTAMPED, FATAL on
   failure, registered in `seed_built_ins` AFTER `run_span_ensures` and BEFORE
   the P4.160 backfill (triggers, then slash), each with v5's ONE boot INFO
   (`target: "quilltap::boot"`, `migrationId = <v4 id>`) when it added; v4's
   INFO/ERROR lines, the `migrations_state` row and the two `prettify` labels
   recorded NO-PORT (the standing class); unit tests in the composer twin's
   five shapes.
3. **The six-sites adoption** of both columns in `db/chat_settings.rs`
   (struct + serde defaults, update patch, INSERT bind + list entry, UPDATE
   assignment, tolerant SELECT entry, positional read with EVERY later index
   re-numbered; the tolerant defaults `false` / `true`); the module doc's
   "eight" → "ten"; `chat_settings_column_sites_guard`'s list grown
   (red-first).
4. **The settings PUT arms** at v4's positions with v4's sentences; the GET
   echo; the raw-bag wire untouched.
5. **`help_settings`'s chat-bag key** (after `customTools`) + the **Almanack
   triple** (type, defaults/collect, render line).
6. **Restore's `ChatSettingsCreate` serde defaults** (the archive-predates-column
   shape); `restore_vintage_state`'s mirror-boot gains the two ensures.
7. **The differentials** (§4): the new ensure oracle case + Rust family (modes
   A/B/C per column + the pair-order mode D); `settings_routes` rows + floors;
   `chat_settings_tier2` corpus rows with NON-default values; `help_tools`,
   `almanack_render`, `almanack_tier2` regenerated from the pin;
   `provisioning_equivalence` green FROM THE PIN with `QT_FRESH_SCHEMA_LIVE`
   (its (1d) is the tripwire that proves the re-dump is v4's).
8. **The web-edge test** (§2.9) — the ensure under a live boot + the wire.

### Tier 2 — should land

9. `zero_migrations_rerun_equivalence` widened: the two refs appended to the
   narrowed runner list (a NEW `CHAT_SETTINGS_4_11_MIGRATIONS` beside
   `P4D288_SPAN_MIGRATIONS`, or the span list grown — the `--p4d288-span`
   fixture-migrator flag reads the same list, so a rename ripples); the
   `planted` base strips the two lines too; `run_chain` runs the new ensures
   after the span chain — proving both apps leave the SAME instance and v4
   re-runs NOTHING after v5's boot (the `shouldRun`-false-unrecorded class, so
   no stamp is owed either way).
10. The committed-pair census: `migrate-memories-fixture-columns.ts
    --report-only --module migrations/scripts/add-custom-tool-triggers-field.ts#
    addCustomToolTriggersFieldMigration --module …/add-composer-slash-commands-
    field.ts#addComposerSlashCommandsFieldMigration` over all 49 `*-main.db`
    (the P4.D251 narrowing's recipe `:286-300`) — `WOULD RUN` is NOT the
    measure (every pair with a `chat_settings` table says so); widen ONLY the
    pairs whose oracle case WRITES `chat_settings` through v4's repo (the
    whole-entity `$set` kills the regen on a column the fixture lacks: the
    P4.52 class — `memories_routes_equivalence`'s `housekeeping_config_set`
    was the first). Candidates to measure: `salon-*` (the Salon chat spine's
    settings PUTs), `almanack-*` (reads only — no), `wardrobe-routes-*` (its
    spec `wardrobe-routes.json` plants a settings row), `images-*` /
    `courier-images-*`, `pascal-run-custom-*` (reads `customTools`),
    `chat-send-*` (the web edge drops/re-adds columns itself). Record the
    measured set in the lane record (the P4.D251 pattern: "the seventeen").
11. `uuid-remap-corpus.json`'s settings row grown with the two keys (pure
    pass-through pin).
12. The `ChatSettingsDto` keys `customToolTriggers?: boolean` /
    `composerSlashCommands?: boolean` in `core-contract.ts` with v4's doc
    comments — a HANDOFF to the two SPA lanes (they own the cards); land the
    keys here only if both lanes agree to the §7 spelling.

### Tier 3 — explicit, loud deferrals

13. **The migration runner** — the standing deferral (CLAUDE.md); the two
    migrations are re-homed as ensures, as every column move since P4.d7.
14. **v4's migration log lines, `migrations_state` rows, `PRETTY_LABELS`** —
    NO-PORT (P4.D63 / D73 / D79 / D249 / D251 / D255 / D265 / D288).
15. **`registry.test.ts`'s acyclic guard** and **`route.custom-tool-triggers.
    test.ts`** — tests-only; the former has no v5 analogue (no registry), the
    latter's three pins are carried by the routes family's rows (§4.2).
16. **`docs/v4/developer/DDL.md` + the six `help/` files** (`chat-settings.md`
    +2, `custom-tools.md` +30/+2, `pascals-workbench.md` +1, `rng-tool.md` +2/+4,
    `chat-settings-composer.md` +48 incl. its front-matter `url` →
    `/settings?tab=chat&section=composer-slash-commands`, `run-tool.md` +2),
    `API.md`, `FEATURES.md`, `features/complete/{pascal-custom-tools,
    pascal-trigger-phrases,composer-slash-commands}.md`, `composer-typeahead.md`
    — the `help/` + `docs/v4/` lane (`p4.9i2` / P4.D278 / P4.D287 shape), NOT
    this keystone; listed so nothing is orphaned.
17. **`pascalMeta.invokedBy` / `triggerPhrase`** (`chats_messages.rs`,
    `api/types.rs`, `tools/run_custom.rs`, `api/custom_tools.rs`,
    `generators/qtap-export.schema.json`) — the Pascal trigger lane's (the
    ledger row's (f)); this keystone touches no message row.

---

## 4. The differential plan — v4's REAL code → NDJSON → the Rust diff

### 4.1 The ensure family — NEW `chat_settings_4_11_columns_ensure_equivalence` (closest recipe: the wardrobe-image twin, §2.4)

ONE oracle case `harness/oracle/cases/chat-settings-4-11-columns-ensure.ts`
(the wardrobe-image case with two `COLUMN_LINE`s and two `MIGRATIONS` refs),
emitting one NDJSON line per mode, base files under `QT_FIXTURE_OUT_DIR` as
`cs411-<mode>.db`:

| mode | base (built from the pin's generateDDL CREATE read back from `sqlite_master`) | v4 runs (registration order) | expected report | v5 runs |
|---|---|---|---|---|
| **T-A** | the `"customToolTriggers" INTEGER DEFAULT 0,\n` line REMOVED; two rows | triggers only | `+RAN add-custom-tool-triggers-field-v1` | the triggers ensure |
| **T-B** | the pin's CREATE as is | triggers | `not needed` | the triggers ensure (no-op) |
| **T-C** | T-A + the migration's `ADD` applied by hand | triggers | `not needed` | the triggers ensure (no-op) |
| **S-A / S-B / S-C** | the same three for `"composerSlashCommands" INTEGER DEFAULT 1,\n` | slash only | `+RAN …slash…` / `not needed` ×2 | the slash ensure |
| **D** (the pair-order mode) | BOTH lines removed; two rows | BOTH, triggers then slash | `+RAN` ×2 | both ensures in v5's boot order |

Comparands per mode (byte-exact): `PRAGMA table_info` (order included —
mode D's `sqlite_master.sql` differs if v5 appends in the wrong order), the
table's `sqlite_master.sql`, every row's `(id, customToolTriggers,
composerSlashCommands)` cells (the DEFAULT back-fill on pre-existing rows is
the point: `0` / `1`, never NULL). The Rust side asserts the report per mode
(`+RAN` only on A and D — no vacuous mode), copies each base, runs the ensure
(s), snapshots, diffs. No `migrations_state` comparand (neither side stamps —
R-B). `runV4Migrations` through `v4-migrations.ts`'s direct `shouldRun()/run()`
is enough (no ledger handshake under test); its `runner: true` arm is there if
the lane prefers the real `MigrationRunner` (memory: oracle-runner-and-skip-
traps §1 — needed only when the handshake IS the behaviour).

Recipe lines for the headers (the driver's conventions — memory
`sweep-driver-recipe-header-rules`): `N=~/.nvm/versions/node/v24.13.1/bin ;
V5W=${V5W:-$HOME/source/quilltap-v5}` / `cd ~/source/quilltap-server` /
`rm -rf /tmp/qt-cs411-ensure` / `QT_FIXTURE_OUT_DIR=/tmp/qt-cs411-ensure $N/npx
tsx $V5W/harness/oracle/cases/chat-settings-4-11-columns-ensure.ts >
/tmp/oracle-cs411-ensure.ndjson`; run `QT_ORACLE_CS411_ENSURE=… QT_FIXTURE_CS411_
ENSURE_DIR=… cargo test -p quilltap-harness --test chat_settings_4_11_columns_
ensure_equivalence -- --nocapture`. Spell paths literally in every step; name
the binary.

### 4.2 `settings_routes_equivalence` — new rows (v4's REAL route + repo over the pin-built fixture)

Family tags `custom_tool_triggers` and `composer_slash_commands` (own floors;
or grow `composer_settings` to ≥ 16 and add a `custom_tool_triggers >= 6`
floor — either way the stale-oracle guard must bite). Rows, user A (a present
row) unless noted:

- `s_get_4_11_defaults` — GET: `customToolTriggers: false`,
  `composerSlashCommands: true` on a row whose columns are at the DDL default
  (the "defaults to off" pin of v4's test, over the REAL schema).
- `s_put_custom_tool_triggers_on` `{customToolTriggers: true}` → 200, the body
  + the refetch carry `true`; `…_off` `{customToolTriggers: false}`.
- `s_put_custom_tool_triggers_wrong_type` `{customToolTriggers: 'yes'}` → 400
  `{error: 'Invalid customToolTriggers value (must be boolean)'}`, the row
  unchanged; `…_null` `{customToolTriggers: null}` → the same 400 (the
  explicit-null class); `…_number` `{customToolTriggers: 1}` → the same.
- `s_put_composer_slash_commands_off` `{composerSlashCommands: false}` → 200;
  `…_on` back to `true`; `…_wrong_type` `{composerSlashCommands: 0}` → 400
  `{error: 'Invalid composerSlashCommands value (must be boolean)'}`; `…_null`.
- `s_put_4_11_both_bad_order` `{composerSlashCommands: 'x', customToolTriggers:
  'y'}` → 400 with the **`customToolTriggers`** sentence (parameter order,
  §1.7 — the arm-ordering pin); and `{composerSlashCommands: 'x',
  impersonationVoiceMode: 'bogus'}` → the **`composerSlashCommands`** sentence
  (it precedes the voice arm).
- `s_put_4_11_create_branch` — user B (no row): `{customToolTriggers: true,
  composerSlashCommands: false}` → 201/200 with the seed-filled row carrying
  both overrides (the `updateForUser` create branch + `patch_create_return_
  shape`).
- `s_put_4_11_alongside` `{customTools: false, customToolTriggers: true}` →
  200 — v4 writes both (the "dead while customTools is off" rule is the
  READER's, not the PUT's).

### 4.3 `chat_settings_tier2_equivalence` — corpus rows

Grow `chat-settings-tier2.json`: the three creates gain `customToolTriggers` /
`composerSlashCommands` with NON-default values on at least one (`true` /
`false`); one `update` flips each; the Rust `CreateData` / `UpdateData` gain the
two fields (`custom_tool_triggers`, `composer_slash_commands`). Regen the
fixture + oracle from the pin (the header recipe `:23-35`).

### 4.4 `zero_migrations_rerun_equivalence` (Tier 2) — widen the narrowed list

`v4-migrations.ts`: add `{ file: 'migrations/scripts/add-custom-tool-triggers-
field.ts', exportName: 'addCustomToolTriggersFieldMigration' }` and `{ file:
'migrations/scripts/add-composer-slash-commands-field.ts', exportName:
'addComposerSlashCommandsFieldMigration' }` (NOTE the file names carry NO
`-v1` suffix, unlike the twelve); `zero-rerun.ts`'s `planted` build strips the
two lines from the dump; `run_chain` (`.rs:106-108`) runs the new ensures after
`run_span_ensures`. Expected: `ran` EMPTY on every mode (both `shouldRun`
false), no ledger row for either id on either side (the unstamped class —
`STAMP_RULE_RED_FIRST`'s `-unstamped` twins are unaffected: v4 re-runs the
SAME stamped ids as before and NOT these two, which proves no stamp is owed).

### 4.5 The regen set at the pin (every family this keystone moves, by name)

`provisioning_equivalence` (+ `QT_FRESH_SCHEMA_LIVE` from the pin's tree),
`chat_settings_4_11_columns_ensure_equivalence` (new), `settings_routes_
equivalence`, `chat_settings_tier2_equivalence`, `help_tools_equivalence`,
`almanack_render_equivalence`, `almanack_tier2_equivalence`, `zero_migrations_
rerun_equivalence` (Tier 2), `chat_settings_column_sites_guard` (no oracle),
`restore_vintage_state` (no oracle; the mirror-boot line), `chat_settings_
composer_web_routes` (the web edge, no oracle). Unmoved but adjacent (run, do
not regen): `chat_settings_wardrobe_image_settings_ensure_equivalence`,
`chat_settings_voice_mode_ensure_equivalence` (their base files are built from
the pin's generateDDL with ONE line removed — at the new pin those CREATEs now
carry the two 4.11 columns, which is fine: the removed line is still unique
and the comparands are per-mode both-sides), `impersonation_voice_legacy_
equivalence`, `repository_zod_messages_equivalence`, `memories_routes_
equivalence` (its fixture carries `composerUnicode`; a regen at the pin
would exercise v4's whole-entity `$set` — Tier 2 item 10 decides).

---

## 5. Fixtures consumed / delivered

- **Delivered by this keystone:** `fresh_schema.json` + `chat_settings_seed.json`
  (committed, D23); the new ensure oracle's base files (scratch, `/tmp`,
  regenerated by recipe — never committed); the grown committed specs
  `chat-settings-tier2.json`, `settings-routes.test.ts`'s rows (+
  `build-settings-fixture.ts` is unchanged — user A's row is created through
  v4's repo at the pin, so it carries both columns at the seed values);
  optionally `uuid-remap-corpus.json`.
- **Consumed:** `harness/oracle/fixtures/chat-settings-tier2.json`'s
  `testPepperBase64` (the ensure cases' pepper, `.ts:49-52`);
  `crates/quilltap-web/tests/fixtures/chat-send-{main,mount}.db` (the web edge —
  it DROPs the columns itself, so the pair's vintage is irrelevant and it is
  NOT widened); `crates/quilltap-web/tests/fixtures/migration-vintage/*.db`
  (the mirror-boot — built by replaying v4's real migration chain at an older
  pin, `build-migration-vintage-fixture.ts:1-40`; it lacks the two columns; the
  tolerant create means no red, §2.3).
- **Who regenerates dependents:** every family in §4.5 regens from the ONE pin
  (`25b8fab6a`) in its own worktree with its own `npm ci --offline` (memory
  `unify-review-finding-shapes` §10; `tier-r-vacuous-equal-rows`'s build-copy
  note); the sweep driver with `--v5w ~/source/quilltap-v5` (memory
  `sweep-driver-spaced-path`), `--families` comma-separated, never two sweeps
  at once. The committed-pair widening (Tier 2 item 10) is this lane's if any
  pair measures as writing `chat_settings`; a sibling lane that regenerates a
  pair-reading family at the pin and dies on `no such column:
  customToolTriggers` has hit exactly that — HANDOFF here, never hand-ALTER
  (memory `rebuilt-fixture-pair-vintage`).

---

## 6. Traps and risks

1. **The positional read** (`chat_settings.rs:1540-1600`): two inserted columns
   shift 19 later indices; a missed `obj.insert` or a stale index reads the
   wrong cell SILENTLY where types coincide (every JSON bag is `Option<String>`
   at parse). The column-sites guard counts names, not indices — add a unit
   test that reads a fully-populated row and checks `createdAt`/`updatedAt`
   land (the two `String` columns at the END are the index canaries).
2. **Equal-by-default corpora** (§2.5, §4.3): both DDL defaults equal the Zod
   and seed defaults, so a differential whose every row stays at the default
   cannot see a dropped bind. Non-default values on both keys, red-first.
3. **The D23 tripwire is red by design at the pin** — `provisioning_equivalence`
   (1d) against `QT_FRESH_SCHEMA_LIVE`; do not "fix" by editing the JSON (never
   by hand; CLAUDE.md D23). Expect EXACTLY two moved lines + two seed entries;
   `main` 83 → 83 statements.
4. **Boot order** (§1.5): after the span chain, before the index backfill;
   triggers before slash. A new boot step "moves sibling tests that pin
   boot-made schema" (memory `unify-review-finding-shapes` §6) — grep
   `crates/quilltap-host` tests for `sqlite_master` / `type = 'index'` counts
   before adding (no host test names a `chat_settings` column today; the
   `host_boot_*` index counts are table-scoped elsewhere).
5. **`restore_vintage_state`'s mirror-boot** (memory `vintage-restore-mirror-
   boot-list`): append the ensures even though `tolerant_insert` hides the
   miss; a `migrations_state` plant in a host-boot test wakes OTHER ensures.
6. **The sweep driver's header rules** (memory `sweep-driver-recipe-header-
   rules`): only the literal `cd ~/source/quilltap-server` maps to `--v4`; no
   shell vars across the regen/run split; the final step filters to the compare
   test; `--self-test` after writing headers. And `--v5w ~/source/quilltap-v5`
   (the spaced real path).
7. **A gated differential that SKIPs reports `ok`** (memory `oracle-runner-
   and-skip-traps` §2/§4): env inline on every call, `--nocapture`, grep the
   test's own `OK:` line; a suspiciously fast green is a SKIP.
8. **`export`-in-one-call** does not reach the next Bash call (same note §4).
9. **Lane gate hygiene** (memory `lane-gate-hygiene`): per-unit oracle paths,
   no in-place regen or source edits mid-gate; quote heredocs.
10. **The pinned worktree's `node_modules`** (memory `unify-review-finding-
    shapes` §10): `npm ci --offline` inside the pin; the live checkout is at
    the pin and clean TODAY (`git status` empty at survey time), but a human
    pull mid-round moves it — work from the pin worktree regardless (memory
    `setupphase-reprobe-and-waiver`: a later v4 commit is an observation, not a
    STOP).
11. **Arm ORDER in `build_settings_assignments`** — a body with two bad keys
    must answer the earlier arm's sentence (§4.2's `both_bad_order` rows); an
    arm appended at the END of the function would pass every single-key row
    and fail only those.
12. **The seed's column order is `PRAGMA table_info` order** of a generateDDL
    table (`dump-fresh-schema.ts:201-205`), so the captured `columns` array
    gains the two names at schema positions — `default_settings_columns()`
    and `seed_chat_settings` both iterate it; a hand-edited seed with the keys
    appended would still provision but would differ from the capture (D23).
13. **Two different "no such column" classes**: the PUT's update branch 500s
    (the composer finding); the create branch and restore drop silently. The
    web-edge test's PUT after boot is what proves the ensure (not the GET).
14. **`help_tools_equivalence` / `almanack_render_equivalence` go red at the pin
    BEFORE any v5 edit** (the output grows on v4's side) — regenerate first,
    then port, so the red is the port's to clear (red-first).
15. **`customToolTriggers` is the FIRST default-`false` tolerant boolean**
    (`.is_some_and`, not `.is_none_or`): copying `composerUnicode`'s line
    verbatim would read an absent column as `true` and turn on trigger phrases
    on every pre-ensure instance (and Friday) — the one semantic hazard in the
    read. Pin it with the absent-column unit test (`:2078` shape).

---

## 7. Contract candidates — every name a sibling lane reads

**Rust (`quilltap_core::db::chat_settings`)** — naming confirmed from
`custom_tools` / `composer_unicode` / `composer_emoji`:

- `ChatSettingsCreate.custom_tool_triggers: bool` — `#[serde(default)]`
  (false), doc: v4 `settings.types.ts:801-806`; wire key `customToolTriggers`.
- `ChatSettingsCreate.composer_slash_commands: bool` — `#[serde(default =
  "smart_typography_default_true")]`, doc: `:815-816`; wire key
  `composerSlashCommands`.
- `ChatSettingsUpdate.custom_tool_triggers: Option<bool>`,
  `ChatSettingsUpdate.composer_slash_commands: Option<bool>`.
- `find_by_user_id` emits `"customToolTriggers": bool` (absent → `false`) right
  after `"customTools"`, and `"composerSlashCommands": bool` (absent → `true`)
  right after `"composerUnicode"` — schema key order; the Pascal trigger lane's
  gate reads this row: `triggers_enabled(row) = row["customTools"] != false &&
  row["customToolTriggers"] == true` (v4 `trigger-runner.ts`).
- The ensures: `db::chat_settings_4_11_repair::{MIGRATION_ID_TRIGGERS =
  "add-custom-tool-triggers-field-v1", MIGRATION_ID_SLASH =
  "add-composer-slash-commands-field-v1", TRIGGERS_COLUMN =
  "customToolTriggers", TRIGGERS_DECL = "INTEGER DEFAULT 0", SLASH_COLUMN =
  "composerSlashCommands", SLASH_DECL = "INTEGER DEFAULT 1",
  ensure_chat_settings_custom_tool_triggers(main) -> Result<bool, DbError>,
  ensure_chat_settings_composer_slash_commands(main) -> Result<bool, DbError>,
  run_chat_settings_4_11_ensures(main) -> Result<(), DbError>}` (the `bool` =
  "added", so the host logs on `true`) — or the two-module spelling of §2.3;
  whichever, the ORDER is triggers → slash.
- `almanack::types::FeatureConfigInfo.custom_tool_triggers: bool` (serde
  `customToolTriggers`), after `custom_tools`.
- `tools::help` chat bag: `customToolTriggers` after `customTools`.

**Wire (dispatch `chatSettings` / `chatSettingsUpdate`)** — keys
`customToolTriggers` (boolean, default `false`) and `composerSlashCommands`
(boolean, default `true`); PUT refusals, 400 `{ error }`:
`Invalid customToolTriggers value (must be boolean)` /
`Invalid composerSlashCommands value (must be boolean)`; the GET echoes both.

**SPA (`core-contract.ts` `ChatSettingsDto`)** — `customToolTriggers?: boolean`
(v4 `components/settings/chat-settings/types.ts` `DEFAULT_CUSTOM_TOOL_TRIGGERS
= false`, label `'Auto-Run Custom Tools on Trigger Phrases'`, description
`'When anyone in a chat says a phrase one of Pascal's custom tools declares, run
that tool automatically with its defaults. Requires Custom Tools.'`, the
failure toast `'Failed to update trigger phrase setting'` — the ledger row's
(h)); `composerSlashCommands?: boolean` (default `true`; the help page's
section anchor `composer-slash-commands`). The Automation card's second row is
`disabled` while `customTools` is false.

**Boot INFO wording (v5's own, one per ensure)** — proposed:
`"Fitted the custom-tool trigger toggle"` / `"Fitted the composer slash-command
toggle"` with `migrationId`, `columnAdded = true`; the lane may choose the
words, the fields are the chain's convention (`p4d288_span.rs:34-50`).

**Oracle refs** (`v4-migrations.ts`):
`{ file: 'migrations/scripts/add-custom-tool-triggers-field.ts', exportName:
'addCustomToolTriggersFieldMigration' }`, `{ file: 'migrations/scripts/
add-composer-slash-commands-field.ts', exportName:
'addComposerSlashCommandsFieldMigration' }` — NO `-v1` in either file name.

---

## 8. Touch points outside this keystone (so the round's other orders can claim them)

- **Pascal trigger lane:** `pascalMeta.invokedBy` + `triggerPhrase` in
  `db/chats_messages.rs`, `api/types.rs`, `tools/run_custom.rs`,
  `api/custom_tools.rs`; the byte-copy of `public/schemas/qtap-export.
  schema.json` → `crates/quilltap-core/src/generators/qtap-export.schema.json`
  and of `qtap-custom-tool.schema.json` → `apps/web/public/schemas/`; the SPA
  Automation card (`automation-settings.ts`, `chat-settings.types.ts:138-151`
  → v4's two-row `AUTOMATION_OPTIONS`).
- **Composer slash lane (SPA):** `qt-composer-slash-commands-settings` after
  `chat-tab.ts:118`; a `slashCommands` signal beside `char-insert-settings.ts`'s
  two; `lib/slash-commands/` (pure — a tier-1 candidate, the ledger row).
- **Help / docs lane:** the six `help/` files + `docs/v4/developer/DDL.md`
  (+2 lines at `:1048` / `:1057` of the pin's file) + `API.md` + `FEATURES.md`
  + `features/complete/*` + `composer-typeahead.md`.

---

## 9. What the ledger rows got wrong (or imprecise), measured from the hunks

1. **`362426c8f` row, "(f) … `.qtap` export schema NOT touched by this commit
   (the dirt touches it for `effects`)" — FALSE.** The commit ships
   `public/schemas/qtap-export.schema.json` (+2/−1, `:972-973`): `invokedBy`'s
   `enum` widened to `["llm", "user", "trigger"]` with a new description, and a
   NEW `triggerPhrase` string property. v5's copy is
   `crates/quilltap-core/src/generators/qtap-export.schema.json` (byte-copy
   rule) — the Pascal lane's, not this keystone's, but the row's "NOT touched"
   would let it slip.
2. **`362426c8f` row, "v5's `fresh_schema.json` `main` has `customTools` at that
   very spot, index 20 — `chat_settings_seed.json` columns 20/21" — the numbers
   are wrong.** Measured: `customTools` is cid **17** in the dump's statement
   (`"compositionModeDefault"` 18) and index **15** (0-based) in the seed's
   `columns` array; `composerUnicode` is cid 21 / seed index 19;
   `impersonationVoiceMode` cid 22 / seed index 20. The POSITIONS in §1.2 and
   §2.1 are the ones to build from.
3. **§1 "PENDING — the TENTH move": "re-homes as an UNSTAMPED table-gated boot
   ensure in the `chat_settings_*_repair.rs` shape"** — right, with one
   refinement: the newer P4.D288 shape (shared `schema_probe` helpers +
   the host's ONE INFO per ensure) is the one to copy, not the silent composer
   twin; the ledger does not say which, and the two differ in whether a boot
   line is logged.
4. **Both rows' log-line descriptions are each correct for their own file**,
   but the asymmetry is unrecorded: the triggers migration logs its INFO TWICE
   on a real add (inside the `if` and as the summary) while the slash one logs
   ONCE (no summary). Irrelevant to v5 (NO-PORT) — recorded so a reviewer does
   not "fix" one to match the other.
5. **§1 "No other DDL past the pin"** — CONFIRMED (§0), with the method on
   record: `lib/schemas/*.ts` + `migrations/scripts` + `DDL.md` across the span;
   `schema-translator.ts` unchanged. The ledger reached the right verdict from
   per-commit classification; this survey reaches it from the span diff.
6. **The `4877fc2da` row's "the closest twin" (`chat_settings_composer_repair.
   rs`)** — true for the COLUMN FAMILY (a composer gate), but that module is a
   three-column array with no `MIGRATION_ID` and no caller-visible "added"
   outcome; the one-column `chat_settings_wardrobe_image_settings_repair.rs`
   (+ its differential, the exact template) and the `chats_scenario_source_
   repair.rs` chain shape are closer for the CODE.
7. **Nothing in either row names the positional-read shift** (§2.2, §6.1) — the
   one mechanical hazard the six-sites guard cannot see. Recorded here.
