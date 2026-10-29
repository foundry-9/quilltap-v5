# Survey — the `236c04e35` KEYSTONE (P4.D341: the ONE `fresh_schema.json` re-dump carrying the 17th–21st D23 moves + the five boot ensures + keystone memories SERVER) and the memory lane (P4.D342: the importance rubric + the group inner circle)

Read-only planning survey, **2026-10-29**. v4 pin **`236c04e35`** ("dev:
started 4.12.0 development", 2026-10-28, `4.12.0-dev.0`, code-identical to
`7789ebed6` / the 4.11.0 release). Oracle baseline **`322a5488e`**. v5 `main`
**`f6cdceedd`** (core 0.0.1379, host 0.0.217, web 0.0.251, cli 0.0.35, SPA
0.5.907). Every v4 read below is `git show 236c04e35:<path>` or one commit's
hunks; nothing was built, run or written in either repo except this file.
Port facts come from SHIPPED HUNKS, never commit prose.

**Commits in scope:**

| sha | subject | this survey's slice |
|---|---|---|
| `8cb584eff` | Feature: keystone memories | whole (server + CLI note + the D23 move); the SPA third is P4.D359's (named in §6) |
| `6fd2d5ffb` | Fix: memory importance scale no longer rates work commitments with family facts | whole (model-visible text only) |
| `a5efdbb97` | Feature: group members get reserved full-text slots in recall | whole |
| `b15fe9404` | Feature: Storyteller mode and declared tool effects | ONLY the schema halves: `add-storyteller-mode-v1` + `add-llm-logs-storyteller-round-id-column-v1`, the Zod fields they render from, the row-struct plumbing |
| `839575d89` | Feature: operator intervention in Pascal custom tools | ONLY `add-custom-tool-intervention-field-v1` + `ChatSettingsSchema.customToolIntervention` |
| `778b97b2b` | Feature: Novel roleplay template | ONLY `add-roleplay-template-unmarked-fields-v1`, the two Zod fields, the narration storage codec + the every-boot seed (incl. the Novel row's BYTES via re-dump) |

**Lanes informed:** P4.D341 (KEYSTONE, wave 0), P4.D342 (memory, wave 1 — stacks on `KEYSTONE`), and the three column consumers P4.D343 (Storyteller server), P4.D345 (intervention server), P4.D351 (Novel template server) through the shared contracts in §7; P4.D359 (the memory keystone UI) through §6.

Ledger rows read: `drift-ledger.md:553` (`6fd2d5ffb`), `:554` (`a5efdbb97`), `:555` (`8cb584eff`), `:557` (`b15fe9404`), `:560` (`839575d89`), `:562` (`778b97b2b`), `:593` (`539bb4012`). Where a hunk disagrees with a row, the correction is marked **⚠ ledger**.

Shape follows `surveys/2026-10-20-setupphase-inventory-keystone-server.md` and `surveys/2026-10-13-setupphase-memory-schema-keystone.md`.

---

## Headline findings (written last)

1. **⚠ ledger — the "R-AL" cite is wrong.** The ruling that keeps the CLI `memories` family OUT of v5 is **R-P** of the `83334b9a5` round (`work-orders/p4.d299-instance-lock-port-bug-239-cli-flags-help-completions.md:186`: "the CLI `memories` family, `memory-diff` and `db memories --tier` stay OUT (their loud refusals stand); only the flags, help texts, completions and the two ported tools move"; the same text at `p4.d288-…:2114`, `p4.d298-…:1502`). **R-AL** of that round is "`9ec4fa2e9` is P4.D303's NO-PORT ratification" (`p4.d299-…:188`) — the ledger rows `:555` and `:593` both cite it for the CLI verb. Under R-P the `keystone <id> [--off] [--write]` verb is NO-PORT (the loud `not yet available` refusal at `crates/quilltap-cli/src/main.rs:111` stands), while the three completion templates' captured bytes DO move (§1.1.9; pinned by `crates/quilltap-cli/tests/completion_behavior.rs:26-27` `include_str!`).
2. **The 17th move is TWO-SHAPE, as the ledger guessed:** generateDDL renders `MemorySchema.keystone: z.boolean().default(false)` as `"keystone" INTEGER DEFAULT 0` **without `NOT NULL`** (`schema-translator.ts:349-359` at the pin: `NOT NULL` only when no default), placed in schema order between `consolidatedAt` and `createdAt`; the migration appends `INTEGER NOT NULL DEFAULT 0` (`add-memory-keystone-v1.ts:53`). `DDL.md:1486` hand-writes the migration shape. The P4.D249 / P4.D304 two-shape class; the ensure uses the migration clause, the dump carries the fresh one.
3. **The partial index MUST be made by the keystone ensure itself, not left to the P4.160 backfill.** v4's `shouldRun` is `!column || !index` (`add-memory-keystone-v1.ts:46`), and `zero_migrations_rerun_equivalence`'s `run_chain` (`:153-170`) runs the ensure chains WITHOUT `ensure_migration_index_family` — so an ensure that adds only the column leaves v4's narrowed runner re-running the migration on the `planted` base (a ping-pong red by construction). The precedent is `db/memories_superseded_by_index.rs` (column-gated, v4's one statement byte for byte, the `Index was not created` re-check), and `migration_indexes.json` carries the SAME statement for fresh provisioning (SQLite stores both as the same `sqlite_master.sql` — measured there). UNSTAMPED; the ensure must run BEFORE P4.160 (a `WHERE "keystone" = 1` index on a table without the column fails — the tiers precedent at `host.rs:2170-2176`).
4. **The intervention migration is registered in the MIDDLE of an existing chain.** `migrations/scripts/index.ts:921-925` at the pin: `addCustomToolTriggersFieldMigration`, **then** `addCustomToolInterventionFieldMigration`, then `addComposerSlashCommandsFieldMigration`. v5's `db/chat_settings_4_11_repair.rs::run_chat_settings_4_11_ensures` (`:107-125`) runs triggers → slash; the intervention ensure is the NEW MIDDLE STEP of that file (and of `harness/oracle/lib/v4-migrations.ts` `CHAT_SETTINGS_4_11_MIGRATIONS:101-104`), not a fourth chain. The migration's INFO fires TWICE on a real add (`:55`, `:62`) — the triggers asymmetry, NO-PORT per the module's precedent.
5. **The 19th move is the FIRST llm-logs column ensure v5 has ever needed.** v5 carries no `autonomousRunId` ensure (`db/llm_logs.rs` only LISTS the column — `LOG_COLUMNS:426`, `row.get(6)` at `:1220`); a v5 instance provisioned before this round has an `llm_logs` table WITHOUT `storytellerRoundId`, and `db/table_shape.rs` `STRUCTURAL_TABLES` (`:79-104`, the llm_logs field list, GENERATED by `table_shape_equivalence` from v4's REAL schema) will name it as a MISSING column → the PHASE 3.1 pass reports the partition DEGRADED on every boot until an ensure adds it. The ensure needs the llm-logs writer (`ws.llm_logs()` — may be absent/degraded: skip, as v4's `shouldRun` returns false when the file is absent) and must run before `verify_structural_tables_at_boot` (`host.rs:616`, i.e. inside `seed_built_ins`' chain). `idx_llm_logs_storytellerRoundId` rides the `migration_indexes.json` re-dump's `llmLogs` part (the `idx_llm_logs_autonomousRunId` precedent).
6. **`storyteller_rounds` IS a generateDDL table** — `StorytellerRoundsRepository extends AbstractBaseRepository` (`storyteller-rounds.repository.ts:28-31`) and the dumper drives every repository's `count()` (`dump-fresh-schema.ts:96-120`), so the re-dump captures it in schema order: `id TEXT PRIMARY KEY NOT NULL, chatId TEXT NOT NULL, triggerMessageId TEXT, status TEXT NOT NULL, bids TEXT DEFAULT '[]', selection TEXT, closedReason TEXT, createdAt TEXT NOT NULL, updatedAt TEXT NOT NULL, closedAt TEXT` + `idx_storyteller_rounds_createdAt`. The migration's table (`add-storyteller-mode-v1.ts:70-86`) has the SAME column order, differs by `PRIMARY KEY` (no NOT NULL), three `DEFAULT NULL`s, the `FOREIGN KEY ("chatId") REFERENCES "chats"("id") ON DELETE CASCADE`, and `idx_storyteller_rounds_chat (chatId, status)` — the chat index lands in `migration_indexes.json` (its table IS in `fresh_schema.json`, so the dumper keeps it); the FK is the standing R-A table-text asymmetry (v5-provisioned instances have NO cascade → **P4.D343 must delete rounds explicitly on chat delete**, §7 contract C-4). The pre-round boot ensure's table SHAPE is a ruling (§10 Q2; recommend v4's migration DDL verbatim + both indexes, the P4.D315 pattern).
7. **⚠ ledger (`:562`) — `narrationDelimiters`' own line does NOT move, and `DDL.md` is wrong about its default.** `NarrationDelimitersSchema.nullable().default('*')` is a `ZodUnion` of a string and a tuple → `getBaseType` answers `'unknown'` (`schema-translator.ts:82-89`) → `formatDefaultValue`'s default arm → `"narrationDelimiters" TEXT DEFAULT '*'` — exactly what v5's committed dump holds today (`fresh_schema.json`, the `roleplay_templates` statement). `DDL.md:1545` writes `DEFAULT '"*"'` (hand-edited, and already so at `322a5488e` — a context line in the span diff). The two new columns render `"unmarkedStyle" TEXT` and `"markdownAsterisks" INTEGER DEFAULT 1` BEFORE `createdAt` in the fresh shape; the migration appends them after `updatedAt` in the same clauses (`add-roleplay-template-unmarked-fields-v1.ts:51,55`) — two POSITIONS, one text each.
8. **The seed rewrite is a seeder change, not a migration, and it is a RE-DUMP job.** v4's `seedBuiltInTemplates` now INSERTs `this.encoded(validated)` and `$set`s `this.encoded(updateData)` (`roleplay-templates.repository.ts:218-246` at the pin) — `narrationDelimiters` stored as JSON text (`"*"`, `["[","]"]`, `null`), with `unmarkedStyle` / `markdownAsterisks` in the UPDATE's `$set`, and a THIRD built-in, `NOVEL_TEMPLATE`, whose `renderingPatterns` are COMPUTED by `generateTemplateRenderingPatterns` (`novel-template.ts:98-103`). `harness/oracle/provision/dump-builtin-templates.ts` drives v4's REAL seeder twice and dumps the rows, so a re-run at the pin yields all three rows' bytes (incl. Novel's computed patterns and the encoded narration) with NO annotations port — v5's `services/builtin_templates.json` holds 2 rows with a BARE `*` today. Consequence: on EVERY existing v5 instance the first boot after this lands rewrites the two built-ins' `narrationDelimiters` from `*` to `"*"` (v4 does the same on its first boot at the pin — `$set` every boot); and `db/roleplay_templates.rs::narration_storage` (`:263-275`, bare string / JSON pair) must become v4's codec (encode `JSON.stringify` on every write incl. `update`'s merged row, lenient decode on every read: `null` → `null`, JSON string / pair → value, non-JSON → the raw legacy string, NULL/empty → `'*'`). This is the one place the keystone and P4.D351 would collide — §7 gives the repository WHOLE to the keystone.
9. **`keystone` appears in EVERY v4-parsed `Memory` object** (Zod default `false`, key position between `consolidatedAt` and `createdAt`) — so beyond the five keystone surfaces, every byte pin of a memory row moves: `db/memories_read.rs` `COLS:81-86` + `marshal_row`'s positional `row.get(28)`/`(29)` (`:313-316`), `schema_keys()`, `api/zod_issues.rs::zod_memory_issues` (`:1190-1251`), `services/qtap_export/schema-key-order.json` `memory`, the backup's `collect.rs` memory rows (`:548-640`, built from `memories_read`), the `.qtap` import (`quilltap_import/memories.rs:211-244`), and every oracle that emits a Memory (§5). The 43-pair widening mechanism is `harness/oracle/fixtures/migrate-memories-fixture-columns.ts --module migrations/scripts/add-memory-keystone-v1.ts#addMemoryKeystoneMigration` from the pinned worktree (the P4.D265 recipe at `:341-349`); 21 committed pairs carry a `memories` table (the 19 + 2 named at `:355-361`).
10. **`frozen_archive_tier2`'s oracle is RUNTIME-DEAD at the pin, not import-dead:** `harness/oracle/cases/frozen-archive-tier2.test.ts:165-171` does `archive.map((m) => m.id)` and `getOrComputeFrozenArchive` now returns `{anchors, circle}` (`archive.ts:155-161`, `:193-203`) — `TypeError: archive.map is not a function` on the first `get` op, a crash, not a well-formed wrong row. The census missed it because the import resolves. `recall-tuning.ts:35` (the `default` row), `:130` (the strict schema), `:136` (`describeRecallTuning`) REDDEN on the two new knobs; `recall-replay`'s result gains `warnings`; the recall-assemble trace's `archive` stage gains `circleHeld` / `circleRendered` / `circleMembers` / `circleTokenBudget` (`assemble.ts:441-454`).
11. **A v4 quirk to carry, not fix:** `POST /api/v1/memories` with `keystone: true` returns **400** AFTER the memory was written when the post-admission mark is refused (`memories/route.ts:637-654`): `The memory was saved, but not as a keystone: <msg>` or `The memory was saved, but could not be marked a keystone.` — a 400 with a durable write. Record it as a recorded divergence candidate for the human only if the lane prefers; the port is the bytes.
12. **Lane cut:** P4.D341 and P4.D342 both want `services/frozen_archive.rs`, `services/recall/render.rs`, `services/recall/types.rs` and `services/recall/assemble.rs` (keystone LIFTING into the circle + `renderArchive` / `renderHead` full-text vs the inner circle itself). §7 gives the WHOLE recall placement of keystones to **P4.D342** (it stacks on `KEYSTONE` and consumes `find_keystones` / `count_keystones` as a contract); the keystone keeps repository, admission, maintenance protection, API, carriers, CLI templates. The `[FrozenArchive] Composed archive` DEBUG then changes ONCE (both commits' keys), in one lane.

---

## 1. Per commit, the shipped hunks

### 1.1 `8cb584eff` — keystone memories (prod 32 files; the 17th D23 move)

#### 1.1.1 Schema + migration

- `lib/schemas/memory.types.ts:150-158` — `keystone: z.boolean().default(false)` after `consolidatedAt`, before `createdAt`; `:165` `export const KEYSTONES_PER_SUBJECT_MAX = 5`; re-exported at `lib/schemas/types.ts:247`.
- `migrations/scripts/add-memory-keystone-v1.ts` (whole, 86 lines; §3.1 for the DDL). `index.ts:456` import, `:938-939` registration (after `addTerminalSessionGeometryMigration`), `:1400-1401` export.
- `lib/startup/prettify.ts:165` — `'add-memory-keystone-v1': 'Setting keystones in the Commonplace Book, that certain facts never slip'`.
- `docs/developer/DDL.md:1485-1486`, `:1497` (the migration shape + the index line).

#### 1.1.2 Repository — `lib/database/repositories/memories.repository.ts`

- `:27-28` `MemoryCreateInput` omits/partials `keystone`.
- `:247-248`, `:254`, `:266-268` — `findByCharacterIdPaginated` option `keystone?: boolean`; when true the filter gains `keystone = true` (an exact `"keystone" = 1` through `findByFilter`; the count path shares it).
- `:462-465` — `hydrateRawRows`: BEFORE `MemorySchema.safeParse`, `if (typeof row.keystone === 'number' || typeof row.keystone === 'bigint' || row.keystone === null) row.keystone = Number(row.keystone ?? 0) === 1`. (A NULL cell → `false`; a string cell is left alone and fails Zod → the row is DROPPED with the WARN — v5's `hydrate_raw_rows` / `zod_memory_issues` must mirror exactly that: number/bigint/null coerced, anything else `invalid_type` boolean.)
- `:552` — `create` → `_create({ ...data, tier: data.tier ?? 'hot', consolidatedFrom: data.consolidatedFrom ?? [], keystone: data.keystone ?? false })`.
- `≈:1147-1176` NEW `findKeystones(characterId, subjects: readonly string[] | 'self')`: `safeQuery`; `'self'` → `(aboutCharacterId = ? OR aboutCharacterId IS NULL)`; else `ids = [...new Set(subjects.filter(id => id && id !== characterId))]`, empty → `[]`, `aboutCharacterId IN (?,…)`; SQL `SELECT * FROM memories WHERE characterId = ? AND keystone = 1 AND ${HOT_TIER_SQL} AND ${subjectSql} ORDER BY createdAt ASC, id ASC` → `hydrateRawRows(rows, 'keystone fetch')`; the safeQuery error text `Error finding keystones` `{characterId, subjects: 'self' | subjects.length}`, fallback `[]`.
- `≈:1178-1206` NEW `countKeystones(characterId, aboutCharacterId: string | null, excludeId?)`: `self = !aboutCharacterId || aboutCharacterId === characterId`; `self` → `(aboutCharacterId = ? OR aboutCharacterId IS NULL)` bound to `characterId`, else `aboutCharacterId = ?`; `AND id <> ?` when `excludeId`; `SELECT COUNT(*) AS n … WHERE characterId = ? AND keystone = 1 AND ${HOT_TIER_SQL} AND …` → `Number(rows[0]?.n ?? 0)`; error text `Error counting keystones` `{characterId, aboutCharacterId}`, fallback `0`.

#### 1.1.3 Admission — `lib/memory/admission/update-memory.ts`, `fold.ts`

- `update-memory.ts:56-57` `MemoryEditPatch.keystone?: boolean`; `:63-70` `UpdateMemoryResult` gains `{ ok: false; reason: 'keystone-refused'; message: string }` (between `invalid-related` and `embedding-failed`).
- `:81-96` NEW `checkKeystone(characterId, existing, patch)`: `willBe = patch.keystone ?? !!existing.keystone`; not willBe → null; `becoming = !existing.keystone`; `subject = patch.aboutCharacterId !== undefined ? patch.aboutCharacterId : existing.aboutCharacterId ?? null`; `moving = patch.aboutCharacterId !== undefined && (patch.aboutCharacterId ?? null) !== (existing.aboutCharacterId ?? null)`; `!becoming && !moving` → null; `(existing.tier ?? 'hot') !== 'hot'` → `'An archived memory cannot be a keystone; it belongs to the digest that replaced it.'`; `countKeystones(characterId, subject, existing.id) >= 5` → `` `A character may keep at most ${KEYSTONES_PER_SUBJECT_MAX} keystones about one subject; unmark one first.` ``; else null.
- `:119-124` — the check runs FIRST in `updateMemory` (before the importance → reinforcedImportance step): refusal → `log.debug('Refusing memory edit: keystone rule', { characterId, memoryId, reason: refusal })` → `{ ok: false, reason: 'keystone-refused', message }`. The patch's `keystone` rides `update` (the `{ ...patch }` spread minus `relatedMemoryIds`) into the row write.
- `fold.ts:62-65` `FoldSurvivor & Partial<Pick<Memory, 'keystone'>>`; `:105-126` NEW `anchorPatch(survivor, absorbed, caps)` (the earlier inline occurredAt / narrativeTime / entities logic, extracted verbatim); `:135-137` `const keystone = !!survivor.keystone`; `:142` the footnote loop iterates `keystone ? [] : absorbed`; `:164` `patch = keystone ? {} : anchorPatch(…)`. Links and the reinforcement delta / `reinforcedAt` are UNCHANGED for a keystone survivor (a fold into one still counts and links).

#### 1.1.4 Maintenance protection

- `lib/memory/maintenance/consolidate.ts:96-98` — `bucketCandidates`: `if (memory.keystone) continue` right after `rowIndex.set(...)` / `isHot` (AFTER `stats.rowsLoaded++`, BEFORE the `CONSOLIDATED` branch) — a keystone is neither seed nor member, and is counted in `rowsLoaded`.
- `demote.ts:28-30` — `isDemotable = tierOf(memory) === 'hot' && memory.source === 'AUTO' && !memory.keystone`; `types.ts:117` the `protected` doc (keystones counted as protected).
- `lib/memory/policy-protection.ts:21` `PolicyProtection = 'keystone' | 'manual' | 'digest' | 'cold'`; `:24-31` `policyProtectionOf` takes `& Partial<Pick<Memory,'keystone'>>`, `if (memory.keystone) return 'keystone'` FIRST; `:35-41` `sparedBy` initial `{ keystone: 0, manual: 0, digest: 0, cold: 0 }` — **key order moves** (`keystone` first) in every `sparedBy` JSON (chat wipe response, `MEMORY_REGENERATE_CHAT`, the fold tool's report, retire reports).
- `lib/memory/retire.ts:105-109` `RetireRow.keystone: number | null`; `:124` `emptyReport.sparedBy` order; `:146-149` `resolveRows` SELECT gains `, keystone`; `:197-201` `applyKeepManual` passes `keystone: !!row.keystone` to `policyProtectionOf`; `:241-273` `cascadeEmptiedDigests`: a `keptKeystones` set; candidates exclude kept keystones; for each emptied digest row: keystone → `keptKeystones.add`, `log.debug('Keystone digest kept though its last member retires', { digestId, characterId })`, `continue`; else `doomed.set`, `added++`, `grew = true`, the existing `Digest retires with its last member` DEBUG; **`added` now counts only rows actually doomed** (was `+= emptied.length`) and the pass loop breaks when nothing grew (`if (!grew) break`) — a behaviour change beyond keystones only when a kept keystone digest is the sole emptied candidate.
- `lib/memory/scoring/protection.ts:21-22` `ProtectionInput` gains `keystone`; `:187` `protection()`: `MANUAL || CONSOLIDATED || keystone` → `{ score: 1, protected: true, protectedBySource: true }`.

#### 1.1.5 Recall placement (the hunks P4.D342 ports — §7)

- `lib/memory/recall/archive.ts:6-18` composition comment; `:81-89` `FrozenArchive.circle` doc (lifted keystones `tier -1`); `:232-240` `usable()`, `take()` rewritten; `:241-252` present keystones: `groupBySubject(await repos.memories.findKeystones(characterId, presentCharacterIds))`, per present character in `presentCharacterIds` order: each keystone (if `chosen.size < size` and usable) → `chosen.set`, `lifted.push({ memory, characterId: otherId, tier: -1, rank: rank++, order, keystone: true })`, THEN that character's digests via `take(findHotDigests(…, FROZEN_ARCHIVE_DIGESTS_PER_PRESENT))`; `:254-256` `take(await repos.memories.findKeystones(characterId, 'self'))` then self digests; `digestCount = chosen.size` (includes keystones); `:259-263` the circle `cap` unchanged, `drawn = drawInnerCircle(…)`, `circle = [...lifted, ...drawn]`; `:282-291` the DEBUG gains `presentKeystones`, `circleKeystones`, `circle: drawn.length`, `filled: anchors.length + lifted.length - digestCount`; `:295-302` `groupBySubject` (skips `aboutCharacterId` null); `:321-331` `drawInnerCircle`: `usable()`; `keystones = groupBySubject(findKeystones(characterId, members.map(m => m.characterId)))` → `keystoneEntries` per member in circle order (`keystone: true`); `:336-340` the digest tier-boundary break now tests `keystoneEntries.length + digestEntries.length >= cap`; `:346-352` `digestEntries` (`keystone: false`); `:355-358` `drawn = [...keystoneEntries, ...digestEntries].sort((a,b) => a.order - b.order || Number(b.keystone) - Number(a.keystone) || a.rank - b.rank)`; the cap trim through `innerCircleTrimOrder`.
- `recall/render.ts:146-149` `renderArchive`: body `memory.keystone ? content.trim() || summary.trim() : summary.trim() || content.trim()`; `:244-247` the same in `renderHead`. (`renderInnerCircle` already rendered `content || summary` — unchanged.)
- `recall/types.ts:120-124` `InnerCircleEntry.keystone: boolean`; `:131-135` `innerCircleTrimOrder` sorts `Number(b.keystone) - Number(a.keystone) || tier || rank || order`.
- `recall/inner-circle.ts:21` comment only.

#### 1.1.6 Routes — `app/api/v1/memories/route.ts`, `[id]/route.ts`

- `[id]/route.ts:29-32` `updateMemorySchema.keystone: z.boolean().optional()` (after `relatedMemoryIds`); `:119-121` `result.reason === 'keystone-refused'` → `badRequest(result.message)` — placed BEFORE the `invalid-related` arm, AFTER the `embedding-failed` 502 arm.
- `route.ts:112-113` `createMemorySchema.keystone: z.boolean().optional()` (after `entities`); `:321` `keystoneOnly = searchParams.get('keystone') === 'true'`; `:345` the paginated repository call gets `keystone: keystoneOnly || undefined`; `:389-391` the legacy (no-limit) path filters `m.keystone === true`; `:559-569` create pre-check: `if (validatedData.keystone)` → `countKeystones(characterId, aboutCharacterId ?? null) >= 5` → `badRequest('A character may keep at most 5 keystones about one subject; unmark one first.')` BEFORE anything is written; `:637-654` after admission: `if (validatedData.keystone && !memory.keystone)` → `updateMemory(memory.characterId, memory.id, { keystone: true }, { userId })`; `!marked.ok` → `logger.debug('[Memories API] Keystone mark refused after manual create', { characterId, memoryId, reason })` and `badRequest(marked.reason === 'keystone-refused' ? `The memory was saved, but not as a keystone: ${marked.message}` : 'The memory was saved, but could not be marked a keystone.')` (the write stands — headline 11); success → `memory = marked.memory` (the 201/200 body carries the marked row).

#### 1.1.7 Carriers

- `lib/import/quilltap-import/import-entities.ts:577-578` `importMemories`: `keystone: memory.keystone ?? false` (between `consolidatedAt` and `characterId` in the create input — the create's own key order is the repository's, §1.1.2).
- `public/schemas/qtap-export.schema.json:1051-1054` memory record `keystone: { type: boolean, description: "Operator-marked standing fact: … Absent reads as false." }` after `consolidatedAt`.
- Backup + `.qtap` export need NO hunk: both read `repos.memories.findByCharacterId` (`backup-service.ts:212`, `ndjson-writer.ts:320,471`, `quilltap-export-service.ts:31,48`) — the parsed Memory now carries `keystone` in schema position. The restore creates through `globalRepos.memories.create(cleanMemoryData, { id })` (`restore.ts:370`) → `keystone ?? false`.

#### 1.1.8 Extraction prompt — `lib/memory/extraction/prompts/shared.ts:166` comment path only (`features/complete/`). No model-visible change in THIS commit.

#### 1.1.9 CLI (NO-PORT under R-P, except the captured completion bytes)

- `packages/quilltap/lib/completion/bash.template:324` `mem_verbs` gains `keystone` (before `maintain`); `:329` flags gain `--keystone --off` (after `--write`).
- `fish.template:276` `complete … -a 'keystone' -d 'Mark or unmark a keystone'` (between `validate` and `maintain`); `:288` `--write` description becomes `'Validate --repair / keystone: apply the change'`; `:289-290` `--keystone` `'Only keystones'`, `--off` `'Keystone: unmark instead of mark'`.
- `zsh.template:542` `'keystone:Mark or unmark a keystone (standing fact) via the running server'`; `:560-562` the same three flag lines.
- `memories-commands.js` (+109: `--keystone` filter via `hasMemoryColumn`, `selectBase` `0 AS keystone`, `◆ ` table prefix, the `Keystone:` show line, `cmdKeystone`, help text), `memories-server.js` (+59: `requestMemoriesApi`, `putMemory`): NO-PORT (R-P). `help/cli-memories.md` + `docs/developer/CLI.md`: the vendor lane's.

#### 1.1.10 SPA (P4.D359's; the files + bytes in §6)

`components/memory/memory-card.tsx` (+10), `memory-editor.tsx` (+24), `memory-list.tsx` (+14).

#### 1.1.11 Help (vendor lane): `help/memory-recall-relevance.md` +15 (`## Keystones: Facts Set in Stone`), `help/memory-maintenance.md` +5, `help/cli-memories.md` +16.

### 1.2 `6fd2d5ffb` — the importance rubric (prod 5 files; model-visible bytes only)

Every byte below is model-visible and must land verbatim.

- `lib/memory/extraction/prompts/shared.ts:166-187` NEW:
  ```
  export const WORK_IMPORTANCE_CEILING = 0.65

  export const IMPORTANCE_WORK_CEILING_NOTE = `Work, project, and tooling agreements — building, filing, fixing, or
  ruling on something — never score above ${WORK_IMPORTANCE_CEILING.toFixed(2)}, however firmly they
  were spoken. The top of the scale is for what changes who someone is or
  what they are to each other.`

  export const IMPORTANCE_FACT_NOT_TELLING_NOTE = `Score the fact, not the telling. A pregnancy, marriage, birth, death,
  change of body, or someone joining or leaving the family is a 0.95 fact
  however it arrives — announced, revealed, confessed, or overheard. The
  0.85 band is for revelations whose content is not itself such a fact.`
  ```
  (`toFixed(2)` → `0.65`; the rendered note reads `never score above 0.65, however firmly they`.)
- `prompts/self.ts:92-112` — the SELF ladder becomes:
  ```
  IMPORTANCE — calibrate to these anchors
    0.95  A fact that redefines the subject's life or bonds: marriage,
          vows, pregnancy, birth, death, a change of body, joining or
          leaving the family.
    0.85  The subject had a self-revelation that changes how they
          understand themselves, or made a personal commitment
          (one the subject spoke themselves) to another person about
          their relationship.
    0.65  The subject formed a substantive new opinion, plan, or
          position — including a commitment to build, file, fix, or
          rule on something.
    0.45  The subject made a work decision, ruling, or task agreement
          in passing.
    0.40  The subject expressed a fresh preference, reaction, or novel
          gesture in passing.
    0.20  The subject acted in a way consistent with established identity
          but worth a single note.
    < 0.20  Do not extract.
  ${IMPORTANCE_WORK_CEILING_NOTE}

  ${IMPORTANCE_FACT_NOT_TELLING_NOTE}
  ```
  (the ceiling note follows `< 0.20  Do not extract.` with NO blank line; one blank line before the fact note; the blank line before `OUTPUT —` stands). Examples: the summarizer example `"importance": 0.85` → `0.65` (`:130`); a NEW second good example appended (`:133-141`): `"content": "I told Charlie and Amy at breakfast that I am pregnant with Charlie's daughter, and that I have known since August."`, `"summary": "told charlie and amy i am pregnant"`, `"keywords": ["pregnancy", "daughter", "family"]`, `"importance": 0.95`, `"temporal": "present"`, `"scope": "wide"`, `"context": "relationships"`.
- `prompts/other.ts:119-139` — the OTHER ladder:
  ```
  IMPORTANCE — calibrate to these anchors
    0.95  A fact that redefines the subject's life or bond to the
          observer: marriage, vows, pregnancy, birth, death, a change of
          body, joining or leaving the family.
    0.85  A revelation, or a personal commitment the subject themselves
          spoke, that changes how the observer relates to the subject.
          A proposal made TO the subject is not the subject's
          commitment.
    0.60  A new substantive fact about the subject's background, plans,
          or skills — including a commitment to build, file, fix, or
          rule on something.
    0.55  A proposal, condition, or demand the subject stated, not yet
          answered.
    0.45  A work decision, ruling, or task agreement made in passing.
    0.40  A new preference, trait, or novel gesture expressed in passing.
    0.20  A specific event occurred with the subject present, no new
          information.
    < 0.20  Do not extract.
  ${IMPORTANCE_WORK_CEILING_NOTE}

  ${IMPORTANCE_FACT_NOT_TELLING_NOTE}
  ```
  Examples: cache-layout `0.85` → `0.55` (`:163`), rename-deferral `0.65` → `0.45` (`:173`); a NEW fourth good item (`:188-197`): `"subjectIndex": 1`, `"content": "Amy told Friday at the piano bench that she is nine weeks pregnant with Charlie's child, laying his hand against her stomach."`, `"summary": "amy nine weeks pregnant with charlies child"`, `"keywords": ["pregnancy", "charlie", "family"]`, `"importance": 0.95`, `"temporal": "present"`, `"scope": "wide"`, `"context": "relationships"`.
- `prompts/episode.ts:34-41` — the `importance` field text becomes:
  ```
    importance    0.20–1.00 (0.95 = a day that changed the family: a
                  wedding, a birth, a death, a pregnancy revealed;
                  0.85 = a day the participants will retell for years;
                  0.5 = a pleasant but ordinary outing; a working session
                  is 0.45 or less)

  ${IMPORTANCE_WORK_CEILING_NOTE}

  ${IMPORTANCE_FACT_NOT_TELLING_NOTE}
  ```
  (one blank line before each note here — unlike SELF / OTHER), then the existing `An episode records what was said and done…` paragraph.
- `lib/memory/cheap-llm-tasks/consolidation-tasks.ts:247-256` — `CONSOLIDATION_SYSTEM_PROMPT`'s tail:
  ```
  IMPORTANCE — weigh the bound entry as a whole: 0.95 for a fact that
  redefines the subject's life or bond to the holder (marriage, vows,
  pregnancy, birth, death, a change of body, joining or leaving the family);
  0.85 for a revelation or personal commitment that changes how the holder
  relates to the subject; 0.60 for a substantive standing fact; 0.45 for
  work decisions, rulings, or task agreements; 0.40 for a preference or
  pattern; 0.20 for a minor note worth keeping.
  ${IMPORTANCE_WORK_CEILING_NOTE}

  ${IMPORTANCE_FACT_NOT_TELLING_NOTE}`
  ```
- `help/memory-recall-relevance.md:76-91` `### How the Book Rates a New Note` (vendor lane). No route, no log line, no schema.
- **⚠ ledger (`:553`) confirmed:** only SELF and OTHER gained a 0.95 example; episode and consolidation got ladder text only. Existing rows are not rescored.

### 1.3 `a5efdbb97` — the group inner circle in recall (prod 11 files)

- `lib/database/repositories/characters.repository.ts:204-218` — `findNamesByIds(ids, options: { excludeArchived?: boolean } = {})`; `if (options.excludeArchived && row.archivedAt) continue`.
- NEW `lib/memory/recall/inner-circle.ts` (124 lines): `InnerCircleMember { characterId, name, tier, groupSize, groupName }`; `resolveInnerCircle(holderId, { excludeIds })` (`:51-105`): `repos.groupCharacterMembers.findByCharacterId(holderId)` → `[]` when none; distinct `groupIds`; per group `repos.groups.findByIdRaw(groupId)` (missing → skipped: "a membership row left behind by a deleted group"), `roster = [...new Set(findByGroupId(groupId).map(m => m.characterId))]`, `groupName = (group.name ?? '').trim()`, `size = roster.length`; `sizes` = distinct sizes ascending, `tierOf(size) = sizes.indexOf(size)`; per candidate (not holder, not excluded) the tightest group by `compareGroups` (`a.size - b.size || groupName.localeCompare || groupId.localeCompare`); `names = repos.characters.findNamesByIds([...tightest.keys()], { excludeArchived: true })`, a nameless id is dropped; sort `tier`, `groupName.localeCompare`, `name.localeCompare`, `characterId.localeCompare`; DEBUG `[InnerCircle] Resolved inner circle` `{ holderId, groups, tiers, excluded, members }` (NOT emitted on the two early returns). `innerCircleSignature(circle)` = `circle.map(m => `${m.characterId}:${m.tier}`).join(',')`; `innerCircleAllowanceFor(tier, allowance)` = `allowance.length === 0 ? 0 : Math.max(0, allowance[Math.min(tier, allowance.length - 1)] ?? 0)`.
- `recall/tags.ts:266-276` `INNER_CIRCLE_ARCHIVE_SHARE = 1 / 3`, `INNER_CIRCLE_ALLOWANCE = Object.freeze([3, 2, 1])`; `:298-299` `RECALL_TUNING_DEFAULTS.innerCircleShare` / `innerCircleAllowance` appended (after `backgroundReserve`).
- `recall/tuning.ts:65-69` `ResolvedRecallTuning.innerCircleShare: number`, `innerCircleAllowance: readonly number[]`; `:101-103` the strict `RecallTuningInputSchema` gains `innerCircleShare: z.number().min(0).max(1).optional().describe('Share of the frozen archive the inner circle may take.')` and `innerCircleAllowance: z.array(z.number().int().min(0).max(20)).min(1).max(10).optional().describe('Digests per inner-circle member by tier, tightest first; the last entry covers every looser tier.')`; `:159-160` `resolveRecallTuning` fills both from the defaults; `:213-216` `describeRecallTuning` appends `innerCircleShare=${n}` and `innerCircleAllowance=[${join(',')}]` when non-default (compared by `join(',')`).
- `recall/types.ts:111-131` `InnerCircleEntry { memory, characterId, tier, rank, order }` + `innerCircleTrimOrder` (`tier || rank || order`) — both re-widened by `8cb584eff` (§1.1.5).
- `recall/archive.ts` (§1.1.5 shows the post-keystone text): `FrozenArchive { anchors, circle }`; `InnerCircleOptions { share, allowance }`; `CacheEntry.circle: string`; `getOrComputeFrozenArchive(…, options.innerCircle?)` → `Promise<FrozenArchive>`: `members = await resolveInnerCircle(characterId, { excludeIds: new Set(present) })` resolved on EVERY call (before the `exclude` early return and before the cache check); `circle = `${knobs.share}|${knobs.allowance.join(',')}|${innerCircleSignature(members)}``; the hit test adds `cached.circle === circle`; the post-compute DEBUG (the cache-miss line, v5 `frozen_archive.rs:238-248` `[FrozenArchive] Computed archive`) replaces `returned` by `circleMembers`, `anchors`, `circle`; `computeFrozenArchive` step 3: `cap = Math.min(Math.floor(size * knobs.share), size - chosen.size)`, `drawInnerCircle`; anchors = chosen minus circle ids, `sort(a.id.localeCompare(b.id))`; `drawInnerCircle` reads `findHotDigests(characterId, member.characterId, limit)` per member in circle order, stops reading a NEW tier once `drawn.length >= cap`, trims by `innerCircleTrimOrder(...).slice(0, cap)`.
- `recall/render.ts:161-204` NEW `renderInnerCircle(entries, maxTokens, provider, subject)`: header `## Those Closest to You`; per entry `body = content?.trim() || summary?.trim() || ''` (empty → skipped), `line = `- ${formatMemorySubjectPrefix(aboutCharacterId, subject)}${body}${formatDigestProvenance(memory)}${formatKeywordsTag(keywords)}`` (NO age tag); token fitting walks `innerCircleTrimOrder(built)` and BREAKS at the first line that does not fit; nothing kept → `emptySection()`; output keeps CIRCLE order; `debugMemories` entries carry `score: 0, salience: 0`.
- `recall/assemble.ts:412-454`: `tuning = plan.replay?.tuning ?? DEFAULT_RECALL_TUNING`; passes `innerCircle: { share: tuning.innerCircleShare, allowance: tuning.innerCircleAllowance }`; `archiveIds` = circle ids then anchors; the subject context is built over `[...circleMemories, ...archive.anchors, ...pool]`; `circleBudget = Math.floor(ordinarySizing.archiveTokenBudget * tuning.innerCircleShare)`; `circle = renderInnerCircle(archive.circle, circleBudget, …)`; `anchors = renderArchive(archive.anchors, ordinarySizing.archiveTokenBudget - circle.tokenCount, …)`; `archiveContent = [circle.content, anchors.content].filter(Boolean).join('\n\n')`; `archiveTokens` summed; `bundle.lines.archive = [...circle.lines, ...anchors.lines]`; `pools.archive` from that; the trace `archive` stage detail: `{ size, held: anchors+circle, circleHeld, circleRendered, circleMembers (distinct), circleTokenBudget, rendered: anchors.lines.length, cutByBudget: anchors.length - anchors.lines.length, tokenBudget }` (key order as listed).
- `recall/replay.ts:123-124` `RecallReplayResult.warnings: string[]` (after `pools`, before `trace`); `:405` filled from `bundle.warnings`.
- `packages/quilltap/lib/recall-replay-command.js:64-66` help: `anchorOrder, backgroundReserve,` / `innerCircleShare, innerCircleAllowance` / `([3,2,1] = digests per member by tier).` — v5's captured `crates/quilltap-cli/src/help/recall_replay_help.txt:37` holds the OLD line (`anchorOrder, backgroundReserve.`) → re-capture.
- `help/groups.md:156-163`, `help/memory-recall-relevance.md:116-125` (`## Those Closest to You`) — vendor lane.
- **⚠ ledger (`:554`)** holds; one precision: `findNamesByIds` is called with the NEW option only from `inner-circle.ts` — every other caller (v5: `tools/log_read.rs`, `services/memory_subject.rs`) keeps the archived rows.

### 1.4 `b15fe9404` — ONLY the schema halves

- `migrations/scripts/add-storyteller-mode-v1.ts` (112 lines, §3.3), `add-llm-logs-storyteller-round-id-column.ts` (135 lines — the FILE has no `-v1`; the id does, §3.4); `index.ts:457-458`, `:940-943`, `:1402-1405`.
- `lib/schemas/storyteller.types.ts` (NEW, 152 lines; `ChatTurnModeSchema = z.enum(['rotation','storyteller'])` `:21`; `StorytellerRoundSchema` `:110-123`; `StorytellerBidRecordSchema { roundId, tier, plan, toolsNamed: [].default }` `:131-136`).
- `lib/schemas/chat.types.ts:489` `storytellerBid: StorytellerBidRecordSchema.nullable().optional()` (ChatMessage, between `carinaMeta` and `pascalMeta`); `:974` `turnMode: ChatTurnModeSchema.nullable().optional()` + `:979` `storytellerInviteFilter: z.boolean().default(false)` (ChatMetadataSchema, between `isPaused` and `isManuallyRenamed`); `:1386`, `:1388` the same on the second chat schema.
- `lib/schemas/llm-log.types.ts:170` `storytellerRoundId: UUIDSchema.nullable().optional()` after `autonomousRunId`.
- `lib/startup/prettify.ts:166-167` `'add-storyteller-mode-v1': "Clearing a dais in the Salon for the Storyteller, and a slate on which the company may bid"`, `'add-llm-logs-storyteller-round-id-column-v1': 'Teaching the wire ledger to bundle each storyteller round under one ribbon…'`.
- `lib/database/repositories/storyteller-rounds.repository.ts` (120 lines; the dumper drives its `count()`); `repositories/index.ts` +4 (container key `storytellerRounds`); `chats.repository.ts` +28 (`setTurnMode` patch-only — P4.D343's); `chats-messages.ops.ts` +5; `lib/backup/restore/delete-service.ts` +3 (the replace restore truncates `storyteller_rounds`).
- `public/schemas/qtap-export.schema.json`: chat `turnMode: { enum: ["rotation","storyteller",null] }`, `storytellerInviteFilter: { type: ["boolean","null"] }`, message `storytellerBid` object (`required: roundId, tier, plan`) — P4.D343 / the schema vendor.
- `DDL.md:639-640`, `:806-833`, `:1034`, `:1911`, `:1923`, `:1936-1937`.

### 1.5 `839575d89` — ONLY the 20th move

- `migrations/scripts/add-custom-tool-intervention-field.ts` (96 lines, §3.5); `index.ts:446-447`, `:923-924` (BETWEEN triggers and slash), `:1385-1386`.
- `lib/schemas/settings.types.ts:808-815` `customToolIntervention: z.boolean().default(true)` after `customToolTriggers`.
- `lib/startup/prettify.ts:198` `'add-custom-tool-intervention-field-v1': 'Teaching Pascal to wait for your nod'`.
- `lib/database/repositories/chat-settings.repository.ts` +1 (the seed/default bag — P4.D345's create binding, or the keystone's if v5 binds the column in `create`; v5 `db/chat_settings.rs:1164-1208` binds `customToolTriggers` / `composerSlashCommands` explicitly → the keystone adds the binding, §7).
- `DDL.md:1144`.

### 1.6 `778b97b2b` — ONLY the 21st move + the codec + the seed

- `migrations/scripts/add-roleplay-template-unmarked-fields-v1.ts` (94 lines, §3.6); `index.ts:459-460`, `:944-945`, `:1406`.
- `lib/schemas/template.types.ts:301-313` `narrationDelimiters: NarrationDelimitersSchema.nullable().default('*')` (was non-nullable), `unmarkedStyle: UnmarkedStyleSchema.nullable().optional()`, `markdownAsterisks: z.boolean().default(true)`; `UnmarkedStyleSchema { style: z.string().min(1).max(50), addOns: DelimiterAddOnsSchema.optional() }` `:277-280`; `RenderingPatternSchema.scope` enum `['inline','line','unmarked','literal','strip']` + the `.refine(p => p.scope === 'literal' || p.scope === 'strip' || p.className.length > 0, { message: 'className is required for span-styling patterns', path: ['className'] })` `:195-226` (a Zod-level change every template read/write validates through — P4.D351's routes, but the keystone's `RtRow` parse must accept the three new scopes and the empty `className`).
- NEW `lib/database/repositories/roleplay-template-narration-codec.ts` (50 lines): `encodeNarrationDelimiters(value) = JSON.stringify(value)`; `decodeNarrationDelimiters(stored)`: `null` → `null`; a `[string,string]` (both non-empty) → itself; not a non-empty string → `'*'`; `JSON.parse` → `null` → `null`, non-empty string → it, a pair → it; parse failure or any other parsed value → the RAW stored string.
- `roleplay-templates.repository.ts:14-20` imports; `:72-73`, `:111-112` the two built-ins gain `unmarkedStyle: null, markdownAsterisks: true`; `:114-115` `NOVEL_TEMPLATE` appended as the THIRD built-in; `:130-174` the codec: `decoded()` / `encoded()` (only when `'narrationDelimiters' in template`; `?? null` encoded), `toPersistedRow` (encode), `_findById`, `_findAll`, `findByFilter`, `findOneByFilter` (decode); `:218-221` seed INSERT `collection.insertOne(this.encoded(validated))`; `:235-246` seed UPDATE `$set` gains `unmarkedStyle`, `markdownAsterisks`, and is `this.encoded(updateData)`; `:384-395` `update`: `narrationDelimiters = 'narrationDelimiters' in data ? data.narrationDelimiters ?? null : existing.narrationDelimiters`, `_update(id, this.encoded({ ...data, narrationDelimiters }))`, returns `decoded(updated)`.
- NEW `lib/chat/novel-template.ts` (119 lines): `NOVEL_DELIMITERS` (Dialogue `"` wrap with `matchCurlyQuotes: true, closeOptional: true`, style `qt-chat-dialogue`; Emphasis `*` wrap, `hideDelimiter: true`, style `qt-roleplay-plain`, addOns bold+italic; OOC `['((', '))']` style `qt-chat-ooc`), `NOVEL_UNMARKED_STYLE { style: 'qt-roleplay-plain', addOns: { bold: false, italic: true, reverse: false, underline: 'none', border: 'none', font: '' } }`, `NOVEL_SYSTEM_PROMPT` (model-visible, `[SYSTEM INSTRUCTION: NOVEL FORMATTING]`, seven numbered rules, `EXAMPLE OF A CORRECTLY FORMATTED REPLY:` + the Marguerite / Theo passage; note the `“…”` escapes in rule 2 are TS escapes — the shipped bytes are the curly quotes), `NOVEL_TEMPLATE { userId: null, name: 'Novel', description: 'Prose as a novel prints it: plain narration, "dialogue" in quotes, *emphasis* in single asterisks, and ((out of character)) in double parentheses.', systemPrompt, isBuiltIn: true, tags: [], delimiters, renderingPatterns: generateTemplateRenderingPatterns({ delimiters, narrationDelimiters: null, unmarkedStyle, markdownAsterisks: false }), dialogueDetection: { openingChars: ['"','“'], closingChars: ['"','”'], className: 'qt-chat-dialogue' }, narrationDelimiters: null, unmarkedStyle, markdownAsterisks: false }`. The seed's bytes (incl. the computed patterns) come from the re-dump (§9), so the keystone ports NO annotations.
- `lib/startup/prettify.ts:308` `'add-roleplay-template-unmarked-fields-v1': 'Setting aside a shelf for the unadorned novel'`.
- `DDL.md:1544-1549`, `:1569-1578`.
- `public/schemas/qtap-export.schema.json`: `narrationDelimiters` anyOf gains `{ type: null }` (description `…, or null for unmarked plain-prose narration`), `unmarkedStyle` anyOf object/null, `markdownAsterisks` boolean, `DelimiterAddOns`-side `matchCurlyQuotes` / `closeOptional` — the schema vendor's.

---

## 2. The v5 surface map (CURRENT shape at `f6cdceedd`)

| v4 file (pin) | v5 counterpart(s) | what v5 does TODAY |
|---|---|---|
| `lib/schemas/memory.types.ts` (keystone) | `crates/quilltap-core/src/db/memories_read.rs:81-86` `COLS` (30 names), `:94` `schema_keys()`, `:296-316` `marshal_row` positional (`consolidatedAt` = `row.get(27)`, `createdAt` 28, `updatedAt` 29), `:340-368` `hydrate_raw_rows`; `db/memories.rs:53-96` `MemCreate` (`consolidated_at` last), `:278-313` `MemUpdate`, `:484-562` `create` INSERT (column list `:530`), `:581-685` `update`; `api/zod_issues.rs:1190-1251` `zod_memory_issues` (no `keystone`) | no `keystone` anywhere; the `alignment_census` tests in `memories_read.rs` pin `COLS` against `fresh_schema.json` and go red at the re-dump (the designed tripwire) |
| `memories.repository.ts` `findKeystones` / `countKeystones` / list filter | `db/memories_read.rs` — NEW fns beside `find_hot_digests` (`:1213`) and `count_unconsidered_hot` (`:1171`, the `HOT_TIER_SQL` precedent); `find_by_character_id_paginated` (`:674`) gains the filter | NO-COUNTERPART (new) |
| `admission/update-memory.ts` | `services/admission/update_memory.rs:56-92` `MemoryEditPatch` (no `keystone`), `UpdateMemoryResult` (no `keystone-refused`) | the embedding-failed arm's WARN at `:354` is the sibling pattern |
| `admission/fold.ts` | `admission/fold.rs:67-94` `FoldSource`, `:136-162` `FoldPatch`, `:251-354` `fold_into` (footnote loop `:266-274`) | no keystone survivor |
| `maintenance/consolidate.ts` `bucketCandidates` | `services/memory_maintenance/consolidate.rs:97-190` `bucket_candidates` | no skip |
| `maintenance/demote.ts` `isDemotable` | `services/memory_maintenance/demote.rs:71` `is_demotable` | `hot && AUTO` |
| `policy-protection.ts` | `policy_protection.rs:16-62` (`Manual \| Digest \| Cold`, `SparedBy { manual, digest, cold }`, `policy_protection_of(source, tier)`) | key order `manual, digest, cold` |
| `retire.ts` | `services/memory_retire.rs:320-362` `cascade_emptied_digests` (the `Digest retires with its last member` DEBUG at `:358`), `find_retire_rows` (`db/memories_read.rs:1994`, `RetireRow` has no `keystone`), `:500-517` the spared/cascade fill | no kept-keystone set |
| `scoring/protection.ts` | `memory_scoring/protection.rs:93-196` (`ProtectionInput.source`; `:196` the MANUAL/CONSOLIDATED arm) | no keystone |
| `app/api/v1/memories/route.ts` + `[id]/route.ts` | `api/memories.rs:135-290` `memory_list` (filters `tier` `:184-218`, sort), the create (`:649`, `:879-907`), the PUT (`:1094-1203`); `api/types.rs:1671-1731` `MemoryList { … tier, sort_by, … }`, `MemoryCreate { memory }`, `MemoryUpdate { memory_id, memory }` | no `keystone` query / body key |
| `import-entities.ts` `importMemories` | `services/quilltap_import/memories.rs:211-244` (`consolidatedAt ?? null` at `:244`) | no `keystone ?? false` |
| backup / export of memories | `services/backup/collect.rs:548-640` (rows via `memories_read`), `services/qtap_export/schema-key-order.json` `memory` (30 keys), `generators/qtap-export.schema.json` + `generators/qtap_schema.rs` (the vendored schema — P4.D313's embed guard `qtap_schema_embed_guard`) | no `keystone` key |
| `recall/archive.ts` | `services/frozen_archive.rs:124` `cache_key`, `:154-250` the cache (`CacheEntry { generation, size, presence, memories }`), `:305-330` `get_or_compute_frozen_archive(db, character_id, chat_id, generation, size, present, now_ms, exclude) -> Vec<Value>`, `:452-565` `compute` / `compose_frozen_archive` (steps 1, 2, 4; the DEBUG at `:549-557` `size, presentCharacters, digests, filled`) | returns a flat `Vec<Value>`; no circle, no keystones |
| `recall/inner-circle.ts` | NO-COUNTERPART (new module) — readers exist: `db/group_character_members.rs:150` `find_group_ids_by_character_id`, `:166` `find_character_ids_by_group_id`; `db/groups.rs:450` `find_by_id_raw_or_none`; `db/characters_read.rs:423` `find_names_by_ids(main, ids) -> HashMap` (no archived option) | — |
| `recall/render.ts` | `services/recall/render.rs:317-375` `render_archive` (`## Memory Anchors`), `:398-468` `render_head`, body rule at `:290` (`content.trim() \|\| summary` note) | no `render_inner_circle`; no keystone body rule |
| `recall/assemble.ts` | `services/recall/assemble.rs:481-560` step 2 (`get_or_compute_frozen_archive(… Some(ordinary_sizing.archive_size), &others …)`, `render_archive`, the trace detail `size, held, rendered, cutByBudget, tokenBudget` `:538-556`) | one section |
| `recall/types.ts` | `services/recall/types.rs` (no `InnerCircleEntry`) | — |
| `recall/tags.ts`, `recall/tuning.ts` | `recall_tags.rs:289-297` (consts), `recall_tuning.rs:59-108` `ResolvedRecallTuning` / `DEFAULT_RECALL_TUNING`, `:112-124` `RecallTuningInput`, `:156` `resolve_recall_tuning`, `:242-300` `describe_recall_tuning`, `:446` `validate_recall_tuning_input` (strict) | no inner-circle knobs |
| `recall/replay.ts` | `services/recall_replay.rs:186-200` `RecallReplayResult` | no `warnings` |
| `characters.repository.ts` `findNamesByIds` | `db/characters_read.rs:423` | no `exclude_archived` |
| `extraction/prompts/{shared,self,other,episode}.ts` | `memory_extraction/prompts/prompt_text.rs:91-101` (SELF ladder), `:119` (0.85 example), `:257-269` (OTHER ladder), `:293`, `:303` (examples), `:401-402` (episode `importance`); `prompts/shared.rs` (no ceiling consts), `self_turn.rs:94`, `other.rs:115,136`, `episode.rs:20,31` (the builders) | the OLD ladders byte-for-byte |
| `cheap-llm-tasks/consolidation-tasks.ts` | `consolidation_tasks/prompt_text.rs:14` `CONSOLIDATION_SYSTEM_PROMPT` (`:81-84` the IMPORTANCE paragraph) | the OLD paragraph |
| `packages/quilltap/lib/recall-replay-command.js` help | `crates/quilltap-cli/src/help/recall_replay_help.txt:37` | `anchorOrder, backgroundReserve.` |
| `packages/quilltap/lib/completion/*.template` | `crates/quilltap-cli/src/help/completion/{bash,zsh,fish}.template` (bash `:323` `mem_verbs`, fish `:268-290`), pinned by `crates/quilltap-cli/tests/completion_behavior.rs:26-27` | the pre-keystone verbs/flags |
| `memories-commands.js`, `memories-server.js` | NO-COUNTERPART — R-P (`p4.d299-…:186`); `crates/quilltap-cli/src/main.rs:62,111` the loud refusal | stands |
| `add-memory-keystone-v1.ts` | NEW `db/memory_keystone_repair.rs` (name per the `*_repair.rs` convention), called from `host.rs` after `:2271` (the Inventory chain) / `:2293` (geometry) — v4 registers it AFTER geometry (`index.ts:938`) — and BEFORE P4.160 (`:2318`) | — |
| `add-storyteller-mode-v1.ts` | NEW `db/storyteller_mode_repair.rs` (three column ensures + the table ensure) after the keystone's | — |
| `add-llm-logs-storyteller-round-id-column.ts` | NEW `db/llm_logs_storyteller_round_id_repair.rs` taking the llm-logs connection | — |
| `add-custom-tool-intervention-field.ts` | `db/chat_settings_4_11_repair.rs` — the NEW MIDDLE step (`:107-125` `run_chat_settings_4_11_ensures`) | triggers → slash |
| `add-roleplay-template-unmarked-fields-v1.ts` | NEW `db/roleplay_templates_unmarked_repair.rs` | — |
| `schema-translator.ts` (the dump) | `services/provisioning/fresh_schema.json` (83 main / 31 mountIndex / 7 llmLogs statements), `migration_indexes.json` (`source.v4Commit` **`83334b9a5`** — the artifact was NOT re-dumped at `322a5488e`, measured), `provisioning/mod.rs` (embeds both) | at the OLD moves |
| `lib/database/table-shape.ts` | `db/table_shape.rs:79-104` `STRUCTURAL_TABLES` llm_logs fields (GENERATED, `table_shape_equivalence` regenerates) | no `storytellerRoundId` |
| `roleplay-template-narration-codec.ts` + the repository | `db/roleplay_templates.rs:140-157` `StringOrPair::storage`, `:263-275` `narration_storage` (bare string / JSON pair), `:295-297` `RtCreate.narration_delimiters: StringOrPair`, `:319` `RtUpdate.narration_delimiters: Option<StringOrPair>`, `:337-368` `create`, `:387-450` `update`, `:499-520` `seed_update`, `:549` the column list, `:556-595` the read (`narrationDelimiters` RAW passthrough — "NOT a registered JSON column"), `:616-701` the readers; `services/builtin_templates.rs:60-70,139,163` + `services/builtin_templates.json` (2 rows, `narrationDelimiters: "*"` / `["[","]"]`) | bare `*` stored; pair stored as JSON text; no null; no `unmarkedStyle` / `markdownAsterisks` |
| `lib/chat/novel-template.ts` | NO-COUNTERPART (its ROW bytes arrive through `builtin_templates.json`; its CODE — `generateTemplateRenderingPatterns` with the new options — is P4.D351's `services/annotations.rs`) | — |

---

## 3. Schema / D23 shapes

Translator rules at the pin (`lib/database/schema-translator.ts`): `unwrapType` strips Optional / Nullable / Default (`:33-58`); `ZodBoolean` → `INTEGER`, `ZodEnum` → `TEXT`, object/array → `TEXT`, a mixed `ZodUnion` → `'unknown'` → `TEXT` (`:64-110`, `:317-339`); `NOT NULL` only when `!optional && !nullable && defaultValue === undefined` (`:349-351`); `DEFAULT` from `formatDefaultValue` — boolean `1`/`0`, string `'…'`, object/array `'<JSON>'`, unknown `'${String(value)}'` (`:368-389`); `id` → `PRIMARY KEY` + implicit `NOT NULL` (the dump shows `"id" TEXT PRIMARY KEY NOT NULL`); the ONLY generated indexes are `idx_<table>_userId` / `idx_<table>_createdAt DESC` (`:414-431`, `:265-284`).

**The ONE re-dump** (the keystone's, from the pinned worktree, Node 24): `dump-fresh-schema.ts` (`QT_SCHEMA_OUT`) + `dump-migration-indexes.ts` (`QT_FRESH_SCHEMA`, `QT_MIGRATION_INDEXES_OUT`, `QT_REAL_BOOT_MASTER` REQUIRED — a real `tsx server.ts` first boot's `sqlite_master`, P4.160 R-F) + `dump-builtin-templates.ts` (§3.6). Expected deltas, all five moves at once:

| part | table | expected fresh text (position) | migration clause (position) | index |
|---|---|---|---|---|
| main | `memories` | `"keystone" INTEGER DEFAULT 0` between `consolidatedAt` and `createdAt` | `INTEGER NOT NULL DEFAULT 0` appended | `idx_memories_keystone` partial → `migration_indexes.json` main (`CREATE INDEX "idx_memories_keystone" ON "memories" ("characterId", "aboutCharacterId") WHERE "keystone" = 1`, quoted, `IF NOT EXISTS` stripped as SQLite stores it) |
| main | `chats` | `"turnMode" TEXT`, `"storytellerInviteFilter" INTEGER DEFAULT 0` between `isPaused` and `isManuallyRenamed` (cids shift by 2 from `isManuallyRenamed` on) | `TEXT DEFAULT NULL`, `INTEGER DEFAULT 0` appended | — |
| main | `chat_messages` | `"storytellerBid" TEXT` between `carinaMeta` and `pascalMeta` | `TEXT DEFAULT NULL` appended | — |
| main | `storyteller_rounds` NEW | §0 headline 6 (schema order = migration order) + `idx_storyteller_rounds_createdAt` | the migration's CREATE TABLE (FK, `DEFAULT NULL`s, bare `PRIMARY KEY`) | `idx_storyteller_rounds_chat ("chatId", "status")` → `migration_indexes.json` main |
| main | `chat_settings` | `"customToolIntervention" INTEGER DEFAULT 1` right after `customToolTriggers` (cid 20; `composerSlashCommands` → 25, `smartTypographySettings` → 33) | byte-equal clause appended | — |
| main | `roleplay_templates` | `"unmarkedStyle" TEXT`, `"markdownAsterisks" INTEGER DEFAULT 1` between `narrationDelimiters` and `createdAt`; `narrationDelimiters TEXT DEFAULT '*'` UNCHANGED | `TEXT`, `INTEGER DEFAULT 1` appended | — |
| llmLogs | `llm_logs` | `"storytellerRoundId" TEXT` after `autonomousRunId` | `TEXT DEFAULT NULL` appended | `idx_llm_logs_storytellerRoundId ("storytellerRoundId")` → `migration_indexes.json` llmLogs |

**Caution for the dump:** the dumper's `secondary` list (`dump-fresh-schema.ts:122-130`) is unchanged — `storytellerRounds` is a primary repository with `count()`. `verify-v5-provisioned.ts` / `build-provision-oracle.ts` then prove v4 opens the v5-provisioned instance. `provisioning_equivalence` (1a–1d) requires `QT_FRESH_SCHEMA_LIVE` (D23 tripwire) and goes red on main until the dump lands.

### 3.1 `add-memory-keystone-v1` (main)

- `dependsOn: ['add-memory-tiers-v1']`, `introducedInVersion '4.11.0'`, description `Add keystone to memories (+ partial (characterId, aboutCharacterId) index over keystones)`, prettify `Setting keystones in the Commonplace Book, that certain facts never slip`.
- `shouldRun`: `isSQLiteBackend() && sqliteTableExists('memories') && (!sqliteColumnExists('memories','keystone') || !hasIndex())`.
- `run`: `addColumnIfMissing('memories', 'keystone', 'INTEGER NOT NULL DEFAULT 0')` → INFO `Added memories.keystone column` `{context: 'migration.add-memory-keystone-v1'}`; `!hasIndex()` → `CREATE INDEX IF NOT EXISTS "idx_memories_keystone" ON "memories" ("characterId", "aboutCharacterId") WHERE "keystone" = 1` → INFO `Created idx_memories_keystone`; result `Keystone memories in place (${itemsAffected} changes)`; ERROR `Keystone-memory migration failed` `{context, error}`. No data rewrite (`DEFAULT 0` stamps every row).
- **v5 ensure:** column (migration clause, appended) + the index statement verbatim + the existence re-check (the `memories_superseded_by_index.rs` shape, `:37-45`), table-gated. **UNSTAMPED** — ping-pong argument: v4's `shouldRun` is purely structural (column ∧ index present → false, "skipped and NOT recorded", re-asked each boot); after v5's ensure both exist, so v4 never re-runs; a stamp would be a row v4 never writes on a fresh v5 instance either (v4 writes it only when it RAN). `zero_migrations_rerun_equivalence`: `planted` base strips column + index; after `run_chain` (which must call the new ensure) v4's narrowed runner must record no `+RAN`; the `-unstamped` twin is vacuous (nothing stamped). Order: after the terminal geometry (`host.rs:2293-2305`), before P4.160.

### 3.2 (the keystone's `migration_indexes.json` half) — regenerated whole; the three new statements above join; the `source.v4Commit` moves to `236c04e35`. `migration_index_backfill_equivalence` + `host_boot_backfilled_indexes` re-measure the "56 absent names" counts (now 59 + the UNIQUE `mp_path`; the module doc at `migration_index_family_repair.rs:9-13` and the `host_boot_fresh_indexes` expectations carry literal counts).

### 3.3 `add-storyteller-mode-v1` (main)

- `dependsOn: ['sqlite-initial-schema-v1']`; prettify `Clearing a dais in the Salon for the Storyteller, and a slate on which the company may bid`.
- `shouldRun`: `isSQLiteBackend() && tableExists('chats') && tableExists('chat_messages') && (!col(chats.turnMode) || !col(chats.storytellerInviteFilter) || !col(chat_messages.storytellerBid) || !tableExists('storyteller_rounds'))`.
- `run`: `addColumnIfMissing('chats','turnMode','TEXT DEFAULT NULL')` INFO `Added chats.turnMode column`; `('chats','storytellerInviteFilter','INTEGER DEFAULT 0')` INFO `Added chats.storytellerInviteFilter column`; `('chat_messages','storytellerBid','TEXT DEFAULT NULL')` INFO `Added chat_messages.storytellerBid column`; `!tableExists('storyteller_rounds')` → the CREATE TABLE (§0 h6) + `CREATE INDEX IF NOT EXISTS "idx_storyteller_rounds_chat" ON "storyteller_rounds" ("chatId", "status")` INFO `Created storyteller_rounds table`; result `Storyteller mode storage in place (${n} changes)`; ERROR `Storyteller-mode migration failed`. Every log `context: 'migration.add-storyteller-mode-v1'`.
- **v5 ensure:** ONE chain of four structure-gated steps, UNSTAMPED (all gates structural; a re-ADD / re-CREATE is a no-op both sides). The table step's SHAPE is §10 Q2. Both `storyteller_rounds` indexes must exist after the chain on EVERY path (fresh provisioning: `fresh_schema.json` + `migration_indexes.json`; pre-round boot: the ensure makes the chat index like v4, and P4.160 finds it present; `idx_storyteller_rounds_createdAt` — v4's `ensureCollection` makes it on first access of a real instance; v5 has no lazy ensure → the boot ensure makes it too, else `provisioning_equivalence`'s index-set leg and a `host_boot_*` pin differ by path).

### 3.4 `add-llm-logs-storyteller-round-id-column-v1` (llm-logs)

- `dependsOn: ['add-llm-logs-autonomous-run-id-column-v1']`; prettify `Teaching the wire ledger to bundle each storyteller round under one ribbon…`.
- `shouldRun`: `isSQLiteBackend()`; the logs FILE absent (`SQLITE_LLM_LOGS_PATH || <data>/quilltap-llm-logs.db`) → false; opens it directly (key pragma, `journal_mode = WAL`, `busy_timeout = 5000` — v4's own opener, not the pool), `llm_logs` absent → false; else `!column`.
- `run`: table absent → success `llm_logs table does not exist; nothing to do` (0); `ALTER TABLE "llm_logs" ADD COLUMN "storytellerRoundId" TEXT DEFAULT NULL` INFO `Added storytellerRoundId column to llm_logs` `{context: 'migration.add-llm-logs-storyteller-round-id-column'}` (NO `-v1` in the context); then UNCONDITIONALLY `CREATE INDEX IF NOT EXISTS "idx_llm_logs_storytellerRoundId" ON "llm_logs" ("storytellerRoundId")`; result `Added storytellerRoundId column + index to llm_logs` (itemsAffected 1); ERROR `Failed to add storytellerRoundId column to llm_logs`. The index comment `per-run budget sum` is copy-paste (ledger `:557` — confirmed).
- **v5 ensure:** on the llm-logs writer's connection when present (Absent / Degraded → skip, as v4 skips an absent file); column-gated; make the index in the same step (v4 does; P4.160 covers the fresh path). UNSTAMPED (column-gated `shouldRun`). Must precede `verify_structural_tables_at_boot` (`host.rs:616`) — headline 5. **No v5 precedent opens the llm-logs partition inside `seed_built_ins`'s `Db::write` on main**: the chain receives `ws.llm_logs().map(|w| w.connection())` as P4.160 does (`host.rs:2320`) — the lane must check the writer-thread discipline (the connection belongs to the llm-logs writer; P4.160 already takes it there, so the seam exists).

### 3.5 `add-custom-tool-intervention-field-v1` (main)

- `dependsOn: ['add-custom-tool-triggers-field-v1']`; prettify `Teaching Pascal to wait for your nod`; `shouldRun` table ∧ `!column`; `run`: `addColumnIfMissing('chat_settings','customToolIntervention','INTEGER DEFAULT 1')` → INFO `Added customToolIntervention column to chat_settings table` `{context: 'migration.add-custom-tool-intervention-field'}` inside the `if`, and the SAME text again as the summary with `{context, columnsAdded, durationMs}`; result message `Added customToolIntervention column to chat_settings table`; ERROR `Failed to add customToolIntervention column`.
- **v5 ensure:** the middle step of `chat_settings_4_11_repair.rs` (`MIGRATION_ID_INTERVENTION`, `INTERVENTION_COLUMN`, `INTERVENTION_DECL = "INTEGER DEFAULT 1"`, ONE INFO `Fitted the custom-tool intervention toggle` — the sibling wording), run order triggers → intervention → slash; UNSTAMPED (column-gated). Generated fresh text is byte-equal to the clause (position only). `CHAT_SETTINGS_4_11_MIGRATIONS` gains `{ file: 'migrations/scripts/add-custom-tool-intervention-field.ts', exportName: 'addCustomToolInterventionFieldMigration' }` in the middle; `zero-rerun.ts`'s planted base strips the column too; `chat_settings_4_11_columns_ensure_equivalence` + `host_boot_p4d304_chat_settings_4_11` grow.
- The seed row: v4's `getOrCreateSingleUser` writes the Zod-defaulted settings row — `customToolIntervention: true` binds as `1`; v5's `db/chat_settings.rs::create` binds columns explicitly (`:1164-1208`), so the keystone adds `("customToolIntervention", &custom_tool_intervention)` and the typed field `custom_tool_intervention: bool` (`:913` neighbourhood) + the `Option<bool>` on the update struct (`:1061-1068`); the provisioning seed capture (`QT_SEED_OUT`) re-pins the row.

### 3.6 `add-roleplay-template-unmarked-fields-v1` (main) + the seed

- `dependsOn: ['add-narration-delimiters-field-v1']`; prettify `Setting aside a shelf for the unadorned novel`; `shouldRun` table ∧ (`!unmarkedStyle || !markdownAsterisks`); `run`: `addColumnIfMissing('roleplay_templates','unmarkedStyle','TEXT')`, `('roleplay_templates','markdownAsterisks','INTEGER DEFAULT 1')` (no per-column INFO), ONE INFO `Added unmarked-text columns to roleplay_templates table` `{context: 'migration.add-roleplay-template-unmarked-fields', columnsAdded, durationMs}`; result `Added ${columnsAdded} column(s) to roleplay_templates`; ERROR `Failed to add unmarked-text columns to roleplay_templates`.
- **v5 ensure:** two column-gated appends, UNSTAMPED; v5's ONE boot INFO when it added anything.
- **The seed (NOT a migration; every boot):** re-run `dump-builtin-templates.ts` at the pin → `builtin_templates.json` with THREE rows; the seeder (`services/builtin_templates.rs`) INSERTs the Novel row where absent (v4 `:201-229`: `findOneByFilter({ name, isBuiltIn: true })` absent → insert; the INFO `Built-in roleplay template seeded`), and on EVERY boot `$set`s `systemPrompt, description, delimiters, renderingPatterns, dialogueDetection, narrationDelimiters (ENCODED), unmarkedStyle, markdownAsterisks, updatedAt` on each existing built-in (v4 `:231-246`) — `seed_update` (`db/roleplay_templates.rs:508-520`) grows two columns and its `narration_delimiters: String` becomes the encoded JSON text. **The dumper's two-pass note stands** (`dump-builtin-templates.ts:11-20`: INSERT → schema key order, UPDATE → seed-literal order for `delimiters`); the Novel row's `delimiters` carry the NEW keys `matchCurlyQuotes` / `closeOptional` and `unmarkedStyle` / `markdownAsterisks` ride the row. `builtin_templates_equivalence` re-proves every byte; `roleplay_templates_tier2_equivalence` sees `narrationDelimiters` DECODED on reads (Quilltap RP now reads as the pair `["[","]"]` — v4's own bug fix by the codec, headline 8).
- No `migrations_state` stamp anywhere in this slice; no data rewrite rides a migration.

---

## 4. Log lines (v4 text + keys + level → v5 home)

| commit | level | text | keys | v5 home |
|---|---|---|---|---|
| `8cb584eff` | DEBUG | `Refusing memory edit: keystone rule` | `characterId, memoryId, reason` | NEW in `services/admission/update_memory.rs` (beside the `:354` WARN) — capture-pin |
| `8cb584eff` | DEBUG | `[Memories API] Keystone mark refused after manual create` | `characterId, memoryId, reason` | NEW in `api/memories.rs` create path |
| `8cb584eff` | DEBUG | `Keystone digest kept though its last member retires` | `digestId, characterId` | NEW in `services/memory_retire.rs::cascade_emptied_digests` (beside `:358`) |
| `8cb584eff` | ERROR (safeQuery) | `Error finding keystones` / `Error counting keystones` | `{characterId, subjects}` / `{characterId, aboutCharacterId}` | NEW — the repository-fallback class (P4.139 / P4.149 / P4.163 homes); whether v5 answers the fallback (`[]` / `0`) or rethrows follows that class's rulings per read |
| `8cb584eff` | WARN | `Memory failed validation in keystone fetch` | `memoryId, error` | `db/memories_read.rs:360-366` with the NEW context string |
| `8cb584eff` | INFO / ERROR | the migration's four lines (§3.1) | — | NO-PORT (migration runner); v5's ONE boot INFO |
| `a5efdbb97` | DEBUG | `[InnerCircle] Resolved inner circle` | `holderId, groups, tiers, excluded, members` | NEW `services/inner_circle.rs` (P4.D342) |
| `a5efdbb97` + `8cb584eff` | DEBUG | `[FrozenArchive] Composed archive` | `characterId, size, presentCharacters, presentKeystones, digests, circleMembers, circle, circleKeystones, circleCap, filled` (FINAL order at the pin) | `services/frozen_archive.rs:549-557` |
| `a5efdbb97` | DEBUG | the cache-miss line (v5 `[FrozenArchive] Computed archive`) | `… presentCharacters, circleMembers, anchors, circle, cacheEntries` (replaces `returned`) | `frozen_archive.rs:238-248` |
| `b15fe9404` | INFO/ERROR | the two migrations' lines (§3.3, §3.4) | — | NO-PORT; v5's ONE INFO per step |
| `b15fe9404` | DEBUG | `Storyteller bid dropped — round not open` (`storyteller-rounds.repository.ts:86-91`) | `context, roundId, participantId, status` | P4.D343 (the repository is theirs) |
| `839575d89` | INFO ×2 / ERROR | §3.5 | — | NO-PORT (the triggers precedent, `chat_settings_4_11_repair.rs:47-62`) |
| `778b97b2b` | INFO / ERROR | §3.6 | — | NO-PORT |
| `778b97b2b` | INFO | `Built-in roleplay template seeded` `{templateId, name}`; `Roleplay template updated successfully` `{templateId}` | existing lines, unchanged text | `services/builtin_templates.rs`, `db/roleplay_templates.rs` (unchanged) |

`6fd2d5ffb` adds no log line.

---

## 5. Harness / oracle impact

### 5.1 Provisioning / schema families (the keystone's)

| family (crate test) | oracle | effect | fixtures |
|---|---|---|---|
| `provisioning_equivalence` | `provision/build-provision-oracle.ts`, `dump-fresh-schema.ts` (`QT_FRESH_SCHEMA_LIVE`), `verify-v5-provisioned.ts` | RED until the re-dump (1d tripwire); then (b) column sets and (c) index sets must hold — `storyteller_rounds`' FK is table text, not compared | `fresh_schema.json`, `migration_indexes.json`, the `QT_SEED_OUT` chat_settings seed |
| `migration_index_backfill_equivalence`, `host_boot_backfilled_indexes`, `host_boot_fresh_indexes` | `cases/migration-index-backfill.ts` | counts move (three new names); the module doc's literal counts | `migration_indexes.json` |
| `table_shape_equivalence` | `cases/table-shape.ts` | REDDEN on `llm_logs` (prints the regenerated literal) | — |
| `zero_migrations_rerun_equivalence` | `cases/zero-rerun.ts` + `lib/v4-migrations.ts` | grows: a `KEYSTONE_236_MIGRATIONS` list (keystone, storyteller, llm-logs round id, unmarked — in registration order) beside the existing ones, the intervention ref inserted into `CHAT_SETTINGS_4_11_MIGRATIONS`, the planted base strips all five moves (the `strip()` helper refuses a line it cannot find exactly once), `TOUCHED` gains `roleplay_templates`, `storyteller_rounds`, `llm_logs`? — the llm-logs partition is a SECOND file: the oracle inspects `planted.db` only; the llm-logs move needs its own planted sibling or a documented carve | `/tmp/qt-zero-rerun/*` (regenerated) |
| `restore_vintage_state` | — | the boot-align list (`:143-214`) gains the new chains (`vintage-restore-mirror-boot-list`); the restore's memory INSERT binds `keystone`, the chat INSERT `turnMode` / `storytellerInviteFilter`, the message INSERT `storytellerBid`, the template INSERT the two columns | `migration-vintage/` trio (own pepper; healed on its scratch copy) |
| `host_boot_p4d304_chat_settings_4_11`, `chat_settings_4_11_columns_ensure_equivalence` | `cases/chat-settings-4-11-columns-ensure.ts` | grow by the middle column | — |
| NEW `host_boot_<round>_keystone_schema` (+ one `*_ensure_equivalence` per chain, the `memory_tiers_ensure_equivalence` / `terminal_sessions_geometry_ensure_equivalence` shape: v4's REAL migration module vs v5's ensure, `table_info` + `sqlite_master` byte-equal, both shapes) | NEW `cases/<name>-ensure.ts` each | — | — |
| `builtin_templates_equivalence` | `cases/builtin-templates.ts` + `provision/dump-builtin-templates.ts` | REDDEN (3 rows, encoded narration, two new columns) | `builtin_templates.json` |
| `memory_row_schema_refusal_lines` | — | the Zod twin gains `keystone` (boolean; number/bigint/null coerced first on the raw path) | — |

### 5.2 Memory families (keystone server — P4.D341)

Every oracle that emits a parsed Memory now carries `keystone: false` between `consolidatedAt` and `createdAt`; v5's marshal must match or every row diff reddens: `memories_read_equivalence` (`cases/memories-read.ts`), `memories_tier2_equivalence` (`memories-tier2.ts`), `memories_routes_equivalence` (`memories-routes.test.ts` — list `?keystone=true`, PUT `keystone`, create `keystone` + the two 400 texts + the post-write 400), `update_memory_tier2_equivalence` (`update-memory-tier2.test.ts` — `keystone-refused` both texts, the subject-move arm), `admission_tier3_equivalence` / `admission_fold_plan_equivalence` (`admission-fold-plan.ts` — a keystone survivor: no footnotes, empty patch, links + delta kept), `memory_scoring_equivalence` (`memory-scoring.ts` — `protection()` keystone), `memory_retire_tier2_equivalence` / `memory_delete_tier2_equivalence` / `memory_cascade_tier2_equivalence` (`sparedBy` key order `{keystone, manual, digest, cold}`; the kept keystone digest; `added` semantics), `memory_maintenance_planning_equivalence` (`is_demotable`, `bucket_candidates` skip; `DemotePhaseReport.protected` counts), `memory_maintenance_tier2_equivalence` / `memory_maintenance_consolidate_tier3_equivalence` (digest rows), `memory_graph_repair_tier2`, `canon-commonplace` (digests), `memory_extraction_cursors_tier2` (`memory-keystone-tier2.json` is P4.D288's cursor corpus — a NAME collision only; grown rows there must keep the `_comment` rule: "siblings may READ it, never change it"), `system_backup_equivalence` / `system_restore_equivalence` / `system_export_equivalence` / `system_import_equivalence` / `qtap_import_equivalence` / `qtap_schema_validate_equivalence` / `qtap_schema_embed_guard` (the memory record's key; `importMemories` `keystone ?? false`; the schema file re-vendor is P4.D360's unless the embed guard forces the keystone to carry it — name the owner in the order).

**Builders that bake the `memories` table** (and therefore the column, through v4's `ensureCollection` at the pin): `build-memories-fixture.ts`, `build-memories-read-fixture.ts`, `build-memories-web-fixture.ts`, `build-admission-fixture.ts`, `build-consolidation-fixture.ts`, `build-maintenance{,-ops,-sweep}-fixture.ts`, `build-memory-{cascade,delete,pipeline-jobs,processor}-fixture.ts`, `build-recall-assemble-fixture.ts`, `build-episodic-recall-fixture.ts`, `build-canon-commonplace-fixture.ts`, `build-carina-memory-extraction-fixture.ts`, plus every Salon/orchestrator builder that creates characters. A REBUILD is NOT needed (the `rebuilt-fixture-pair-vintage` warning: a rebuild drops migration-only columns); the committed pairs are WIDENED in place with `migrate-memories-fixture-columns.ts --module …#addMemoryKeystoneMigration` from the pin over the 21 pairs that carry `memories` (`:355-361` names them: `almanack, character-archive, character-generators, characters, chat-admin, chat-delete, chat-dialogs, chat-send, embedding-remainder, episodic-recall, headshoulders, home, in-scene-voiced, memories, photos, post-office, salon-long, salon, system-data` + `embedding-generate, embedding-profiles`) — run `--report-only` first over all 49 `*-main.db`; `.meta.json` sidecars do not move. The OTHER four moves widen pairs too: `chats` (43 pairs carry `chats` per the P4.D265 census), `chat_messages`, `chat_settings`, `roleplay_templates`, and the `*-llmlogs.db` / `*-llm-logs.db` siblings (`courier-images`, `salon`, `attach-file`, `chat-delete` …) for `llm_logs` — the migrator's `--module` arm takes one module per invocation (five passes per pair) or the `--runner` arm with a new list; measure with `--report-only` and record per pair as P4.D265 / P4.D288 did.

### 5.3 Recall / prompt families (P4.D342)

| family | oracle | effect |
|---|---|---|
| `frozen_archive_tier2_equivalence` | `cases/frozen-archive-tier2.test.ts:165-171` | **RUNTIME-DEAD** (`archive.map` on `{anchors, circle}` throws) — re-aim to emit `{ anchors: ids, circle: [{id, characterId, tier, rank, order, keystone}] }` (or `returned` = circle ids then anchor ids, plus a `circle` key); grow the `memories-tier2`-style fixture with groups (the mount-index partition — `groups` / `group_character_members`) and keystone rows; the fixture builder `build-memories-fixture.ts` / its `.json` gains a mount pair (today the family reads a main pair only — measure) |
| `recall_render_equivalence` | `cases/recall-render.ts` | REDDEN once keystone rows join the corpus (`renderArchive` / `renderHead` body rule); NEW `renderInnerCircle` rows (header, no age tag, the trim-order budget cut, circle order kept) |
| `recall_assemble_tier3_equivalence` | `cases/recall-assemble-tier3.test.ts` | REDDEN on the trace detail keys (`circleHeld` … even with an empty circle: `circleHeld: 0, circleRendered: 0, circleMembers: 0, circleTokenBudget: floor(budget/3)`) and `held` semantics; `## Those Closest to You` once a group fixture exists; `archive_still_on_retro_turn` holds |
| `recall_tuning_equivalence` | `cases/recall-tuning.ts:35,130,136,173` | REDDEN: the `default` row gains both knobs; the strict schema accepts/refuses the two new keys (bounds, `.min(1).max(10)` array, `.int()`); `describeRecallTuning` tails |
| `recall_replay_equivalence` | `cases/recall-replay.test.ts` | REDDEN if the whole result is compared (`warnings` after `pools`); the CLI help capture |
| `recall_deliver_equivalence`, `recall_history_equivalence` | — | the circle rows are archive-pool rows for delivery (`bundle.lines.archive` = circle then anchors; `pools.archive` ids) — the delivery stamp set widens by circle ids |
| `characters_reads_equivalence` | `cases/characters-reads*.ts` | additive option; a row for `excludeArchived: true` |
| `memory_tasks_equivalence` (the four extraction prompts, tier 1) | `cases/extraction-pure-tier1.ts` over `fixtures/extraction-pure-tier1.json` | REDDEN on every SELF / OTHER / episode `system` message (the ladders + notes + examples) — regenerate; the committed corpus's inputs stand |
| `consolidation_tasks_equivalence` | `cases/consolidation-tasks.ts:57` (`constants` row carries `CONSOLIDATION_SYSTEM_PROMPT`) | REDDEN — the new tail |
| the tier-3 extraction/consolidation families (`memory_processor_tier3`, `carina_memory_extraction_tier3`, `memory_pipeline_jobs_tier3`, `memory_maintenance_consolidate_tier3`, `memory_regenerate_jobs_tier3`) | jest oracles with canned answers | REDDEN only where the family pins the REQUEST bytes (the `llm_logs.request` row or a captured prompt) — measure each by grepping its Rust test for `request` / `systemPrompt`; the writes do not move |
| `recall_query_tier3`, `recall_rank`, `recall_tags` | — | unchanged (no hunk) |

### 5.4 Import-dead census (this area)

None of this area's cases is import-dead at the pin: `extraction-pure-tier1.ts` (`buildSelfTurnMessages` … exports stand), `consolidation-tasks.ts`, `recall-*.ts`, `memories-*.ts`, `frozen-archive-tier2.test.ts` (import-ALIVE, runtime-dead — headline 10), `update-memory-tier2.test.ts`. The brief's listed import-dead cases are other lanes'. One more property-access case to check in-lane: `recall-assemble-tier3.test.ts` reads `b.trace` / `out.trace` only (`:319-359`) — alive.

---

## 6. SPA (P4.D359's half — files + wire)

| v4 | v5 Angular | the change | wire |
|---|---|---|---|
| `components/memory/memory-card.tsx:30-31, 79-86` | `apps/web/src/app/memory/memory-card.ts` (`:41-61` header row; `:165-177` the `archived` / `sourceClass` computeds) + `memory-card.spec.ts` | a `Keystone` pill after the importance label, before the archived pill: class `text-xs px-2 py-0.5 rounded-full qt-bg-primary/10 qt-text-favorite`, `title="Keystone: a standing fact, always remembered in full and never tidied away"`, text `Keystone` | `MemoryDto.keystone?: boolean` (`core/core-contract.ts:6004-6040`, after `consolidatedAt`) |
| `memory-editor.tsx:27, 47, 68, 206-225` | `memory/memory-editor.ts` (`:84-103` the importance slider; `:138-177` the form signals + the save body) + spec | a checkbox block after the importance block: `id="keystone" name="keystone" class="qt-checkbox mt-0.5"`, label `Keystone — always remembered` (`qt-text-label`) + the `qt-text-xs` sentence `A standing fact, set in stone until you yourself chip it out. Whenever its subject is in the room, in the character's own circle, or is the character themself, it is recalled in full, and no tidying of the Book will ever fold, digest or shelve it. Five to a subject, at most.`; seeded `memory?.keystone ?? false`; sent as `keystone` on BOTH the create POST and the edit PUT body | `MemoryCreate.memory.keystone?: boolean`, `MemoryUpdate.memory.keystone?: boolean` — the server refuses with 400 `A character may keep at most 5 keystones about one subject; unmark one first.` / `An archived memory cannot be a keystone; …` / `The memory was saved, but not as a keystone: …` (the editor shows `error`) |
| `memory-list.tsx:32, 68, 95, 134, 315-323` | `memory/memory-list.ts` (`:117-125` the tier select; `:251-252` the filter signals; `:266` the params) + spec; `memory/memory.api.ts:59` `MemoryListPage` / the list params | a checkbox after the tier select: `title="Show only the standing facts you have set in stone"`, class `qt-checkbox`, label text `Keystones only`; when on, `params.set('keystone','true')`; the list reload key includes it | `MemoryList.keystone: Option<bool>` (query `keystone=true`; the Request variant at `api/types.rs:1671`) — the keystone lane adds the field, the SPA sends it |
| — | `apps/web/e2e/settings-memory-flow.spec.ts` / `salon-memories-realtime-flow.spec.ts` | one gated beat: mark → badge → `Keystones only` → unmark; a sixth refused with the 400 text | — |

No realtime / Event change: the existing `memories` publish covers the edit (v4 `updateMemory` publishes as before).

---

## 7. The lane split, by FILE OWNERSHIP

### 7.1 P4.D341 — KEYSTONE (wave 0, from `main`)

Creates / edits:

- `crates/quilltap-core/src/services/provisioning/fresh_schema.json`, `migration_indexes.json` (both re-dumped), `provisioning/mod.rs` (counts / docs if any literal), `services/builtin_templates.json` (re-dumped, 3 rows), `services/builtin_templates.rs` (two new columns through `seed_update`; encoded narration on both paths).
- NEW `db/memory_keystone_repair.rs`, `db/storyteller_mode_repair.rs`, `db/llm_logs_storyteller_round_id_repair.rs`, `db/roleplay_templates_unmarked_repair.rs`; EDIT `db/chat_settings_4_11_repair.rs` (middle step); `db/mod.rs`; `crates/quilltap-host/src/host.rs` (the chain after `:2305`, before `:2318`; the llm-logs step takes `ws.llm_logs()`); `crates/quilltap-core/src/test_support.rs` (`ensure_*_on` helpers for the new chains).
- Column plumbing: `db/memories_read.rs` (COLS, `schema_keys`, `marshal_row` index 28 + shift, `hydrate_raw_rows`' coercion, NEW `find_keystones` / `count_keystones`, the paginated filter), `db/memories.rs` (`MemCreate.keystone: bool` default false, `MemUpdate.keystone: Option<bool>`, the INSERT list + bindings, `update`'s `set_col!`), `db/memories_read.rs::find_retire_rows` + `RetireRow.keystone`; `api/zod_issues.rs` (`zod_memory_issues` keystone); `db/chats.rs` (`ChatRow.turn_mode: Option<String>`, `storyteller_invite_filter: bool`; the create INSERT; `ChatUpdate` fields), `db/chats_read.rs` (`ALL_COLUMNS` + the key list `:323-327` + positional shift), `db/chats_messages.rs` (`storyteller_bid: Option<Value>` in the write struct + INSERT list `:1248`), `db/chats_messages_read.rs` (`COLUMNS` `:86-91`, emit `storytellerBid` between `carinaMeta` and `pascalMeta` `:228-230`, positional shift), `db/llm_logs.rs` (`LOG_COLUMNS`, the write column list `:552-566`, `LlmLogRow.storyteller_round_id: Option<String>` `:361-373`, `row.get(6)` → shift, `map_log_row`), `db/table_shape.rs` (regenerated literal), `db/chat_settings.rs` (`custom_tool_intervention: bool` + `Option<bool>`, create binding, update arm?), `db/roleplay_templates.rs` WHOLE (the codec, `RtCreate` / `RtUpdate` / `SeedUpdate` + the `unmarked_style: Option<UnmarkedStyle>` / `markdown_asterisks: bool` fields, column lists, the typed `RenderingPattern.scope` enum + optional-empty `className` tolerance in `delimiters_from_value` / the pattern parse), `services/qtap_export/schema-key-order.json` (`memory` + `chat` + `chat_message` + `roleplay_template` key lists), `services/backup/collect.rs` (memory rows — follows `memories_read`; chat / message / template rows if column-listed — measure), `services/backup/restore/rows.rs` + `orchestrator.rs` (the memory / chat / message / template INSERT bindings), `services/quilltap_import/memories.rs` (`keystone ?? false`), `services/quilltap_import/*` for chats / messages / templates if they bind column lists (measure).
- Keystone memories server: `services/admission/update_memory.rs` (patch field, `check_keystone`, the result variant + DEBUG), `admission/fold.rs` (`FoldSurvivor.keystone`, `anchor_patch` extraction, the two `keystone ?` arms), `services/memory_maintenance/{consolidate,demote}.rs`, `policy_protection.rs` (variant FIRST, `SparedBy.keystone` FIRST, the `to_json` order), `services/memory_retire.rs` (`cascade_emptied_digests` kept set + `grew` + `added`), `memory_scoring/protection.rs` (+ `ProtectionInput.keystone`), `services/memory_maintenance/fold_near_duplicates.rs` (`is_fold_eligible` through `policy_protection_of` — add the keystone argument), every `policy_protection_of(source, tier)` caller (grep: `fold_near_duplicates.rs:81`, `memory_retire.rs`, the chat-wipe route, `MEMORY_REGENERATE_CHAT`), `api/memories.rs` (list filter, create pre-check + post-mark, PUT arm), `api/types.rs` (`MemoryList.keystone`), `api/zod_issues.rs` (the create/update request twins: `keystone: z.boolean().optional()`).
- CLI: `crates/quilltap-cli/src/help/completion/{bash,zsh,fish}.template` (captured bytes), `tests/completion_behavior.rs` (if a literal lists verbs).
- Harness: `harness/oracle/lib/v4-migrations.ts` (the lists), `cases/zero-rerun.ts`, NEW `cases/*-ensure.ts` ×4 + the intervention row in `chat-settings-4-11-columns-ensure.ts`, `cases/memories-read.ts` / `memories-tier2.ts` / `memories-routes.test.ts` / `update-memory-tier2.test.ts` / `admission-fold-plan.ts` / `memory-scoring.ts` / `memory-retire-tier2.ts` / `memory-delete-tier2.ts` / `memory-cascade-tier2.ts` / `memory-maintenance-planning.test.ts` (grown rows), `fixtures/migrate-memories-fixture-columns.ts` (recipe header), the widened pairs under `crates/quilltap-web/tests/fixtures/`, `crates/quilltap-harness/tests/{restore_vintage_state,zero_migrations_rerun_equivalence,table_shape_equivalence,provisioning_equivalence,migration_index_backfill_equivalence,chat_settings_4_11_columns_ensure_equivalence,memory_row_schema_refusal_lines,builtin_templates_equivalence}.rs` + NEW `*_ensure_equivalence.rs` ×4, `crates/quilltap-host/tests/host_boot_p4d304_chat_settings_4_11.rs` + NEW `host_boot_p4d341_*.rs`.
- Docs: `docs/developer/porting/status-log.md` (its record), `drift-ledger.md` is `/unify`'s.

### 7.2 P4.D342 — memory (wave 1, from `KEYSTONE`)

Creates / edits (NONE of the above files):

- NEW `services/inner_circle.rs` (`resolve_inner_circle(db, holder_id, exclude: &HashSet<String>) -> Vec<InnerCircleMember>`, `inner_circle_signature`, `inner_circle_allowance_for`); `db/characters_read.rs` (`find_names_by_ids_with(main, ids, exclude_archived: bool)` — additive; `tools/log_read.rs` / `services/memory_subject.rs` untouched); `recall_tags.rs` (`INNER_CIRCLE_ARCHIVE_SHARE`, `INNER_CIRCLE_ALLOWANCE`), `recall_tuning.rs` (two fields on `ResolvedRecallTuning` / `DEFAULT_RECALL_TUNING` / `RecallTuningInput`, `resolve`, `describe`, the strict validator + the array bounds), `services/frozen_archive.rs` WHOLE (the `FrozenArchive { anchors, circle }` return, the circle signature in the cache key, steps 1–3 incl. BOTH commits' keystone lifting and `find_keystones` calls, the two DEBUG lines), `services/recall/{render,assemble,types}.rs` (`render_inner_circle`, the keystone body rule in `render_archive` / `render_head`, `InnerCircleEntry`, `inner_circle_trim_order`, the step-2 rewrite + trace keys, `RecallBundle.warnings` already exists? — measure), `services/recall_replay.rs` (`warnings`), `api/recall_replay.rs` (the response key), `crates/quilltap-cli/src/help/recall_replay_help.txt`, `memory_extraction/prompts/{prompt_text,shared}.rs` (the ladders, the two notes as consts rendered once — `WORK_IMPORTANCE_CEILING: f64 = 0.65` formatted `0.65`), `consolidation_tasks/prompt_text.rs`.
- Harness: `cases/frozen-archive-tier2.test.ts` (re-aim + groups/keystone rows in its fixture — the builder is `fixtures/build-memories-fixture.ts`? measure which `.json` the case reads), `cases/recall-render.ts`, `recall-assemble-tier3.test.ts` + `fixtures/build-recall-assemble-fixture.ts` / `recall-assemble.json`, `recall-tuning.ts`, `recall-replay.test.ts`, `extraction-pure-tier1.ts` (regen), `consolidation-tasks.ts` (regen), `characters-reads*.ts`; the matching `crates/quilltap-harness/tests/*.rs`.

### 7.3 Named shared contracts

- **C-1 (keystone → P4.D342):** `db::memories_read::find_keystones(conn, character_id: &str, subjects: KeystoneSubjects) -> Result<Vec<Value>, DbError>` with `enum KeystoneSubjects<'a> { SelfSubject, About(&'a [String]) }` (v4's `'self' | readonly string[]`; the `About` arm dedups and drops the holder's own id, empty → `Ok(vec![])`), rows hydrated (`'keystone fetch'`), `ORDER BY createdAt ASC, id ASC`; and `count_keystones(conn, character_id, about_character_id: Option<&str>, exclude_id: Option<&str>) -> Result<i64, DbError>`. The keystone lane lands both with unit tests in `KEYSTONE`; P4.D342 consumes them unchanged. The `Memory` Value carries `"keystone": bool` (never a number) after hydration — P4.D342 reads `m["keystone"].as_bool().unwrap_or(false)`.
- **C-2 (keystone → P4.D343 / P4.D348 / P4.D355):** `db::chats::ChatRow { …, is_paused: bool, turn_mode: Option<String> /* NULL ⇒ "rotation" at the READER, never written here */, storyteller_invite_filter: bool, is_manually_renamed: bool, … }`; the chat JSON wire emits `"turnMode": null | "rotation" | "storyteller"` and `"storytellerInviteFilter": bool` between `isPaused` and `isManuallyRenamed` (the chat GET body is the parsed Chat in schema order); `ChatUpdate.turn_mode: Option<Option<String>>` (patch-only `set_turn_mode` is P4.D343's — the keystone binds the column in create as NULL and in the generic update ONLY if v4's `_update` whole-row path would — v4 says patch-only, so the keystone does NOT add it to the whole-row update; P4.D343 adds `set_turn_mode`); `db::chats_messages::MessageIn.storyteller_bid: Option<Value>` written as JSON text, read back as `"storytellerBid"` (parsed object or null) between `carinaMeta` and `pascalMeta` in `chats_messages_read`'s emitted key order — P4.D343 types it later if it wishes; `db::llm_logs::{LlmLogCreate, LlmLogRow}.storyteller_round_id: Option<String>` (bound / read by name; NULL default); the `storyteller_rounds` TABLE exists on every instance after boot (fresh: the dump; pre-round: the ensure) with BOTH indexes — **P4.D343 owns `db/storyteller_rounds.rs` (the repository) and must delete rounds on chat delete (no FK on v5-provisioned instances) and truncate them on a replace restore (`delete-service.ts` +3 — P4.D355's delete-all list or P4.D343's; name one)**.
- **C-3 (keystone → P4.D345):** `db::chat_settings::ChatSettingsRow.custom_tool_intervention: bool` (+ `ChatSettingsUpdate.custom_tool_intervention: Option<bool>`, the create binding, the seed row); the PUT arm / Almanack line / `help_settings` are P4.D345's (the P4.D304 pattern).
- **C-4 (keystone → P4.D351):** `db::roleplay_templates::{RtRow, RtCreate, RtUpdate, SeedUpdate}` gain `narration_delimiters: Option<StringOrPair>` (None ⇔ stored JSON `null`; the lenient decoder on every read; `JSON.stringify` on every write incl. `update`'s merged row), `unmarked_style: Option<UnmarkedStyle { style: String, add_ons: Option<DelimiterAddOns> }>`, `markdown_asterisks: bool` (default true); `RenderingPattern.scope` accepts `unmarked | literal | strip` with an empty `className`; `TemplateDelimiter` (wrap) accepts `match_curly_quotes: Option<bool>`, `close_optional: Option<bool>` (the keystone must parse the Novel row's delimiters to seed it — `delimiters_from_value` `:674`). P4.D351 owns `services/annotations.rs` (`generate_template_rendering_patterns` with the new options), `template_prompt_hint.rs`, the routes (`api/roleplay_templates*.rs` — POST null narration, `Narration delimiters are required (null for unmarked prose)`, `Invalid unmarked style: …`, `markdownAsterisks must be a boolean`, the PUT regenerate-on-four-fields, the GET regenerate-empty), `markdown_postprocess`, `typography`. The keystone does NOT regenerate patterns anywhere; the seed's patterns are the dump's bytes.
- **Stacking:** P4.D342 branches from `KEYSTONE` (needs C-1 and the `keystone` key in every row). P4.D343 / P4.D345 / P4.D351 branch from `KEYSTONE` (C-2 / C-3 / C-4). P4.D359's memory UI needs only the wire shape (§6) and may build blind in wave 0 against the spec, flipping its beats live after unification.
- **No two lanes edit one file** under the split above, provided P4.D351 keeps out of `db/roleplay_templates.rs` and `services/builtin_templates.*`, and P4.D343 keeps out of `db/chats.rs` / `db/chats_read.rs` / `db/chats_messages*.rs` / `db/llm_logs.rs` column lists (it adds METHODS in new files or appends functions — if it must touch those files, it rebases onto `KEYSTONE` and the append is at the file's tail). The P4.D331 precedent for `find_names_by_ids` (an additive sibling function, same file) is the shape for P4.D342's option.

---

## 8. Tiering

**P4.D341 — Tier 1 (must land):** the re-dump (both JSONs) + the seed JSON; the five ensures (four chains + the middle step) with their ensure differentials, the vintage mirror-boot and zero-rerun growth, `table_shape` regen; every column's row-struct / reader / writer plumbing so dependents compile and every existing family stays green (the `alignment_census` tripwires, the positional shifts); `find_keystones` / `count_keystones`; the keystone key in `hydrate_raw_rows` / `zod_memory_issues` / `schema_keys` / `schema-key-order.json` / backup / import; the pair widening; admission `check_keystone` + `keystone-refused`; `fold_into`'s keystone arm; `policy_protection` (variant + key order) through every caller; `is_demotable`, `bucket_candidates`; `protection()`; `cascade_emptied_digests`; the three API arms with v4's bytes (incl. the post-write 400); the completion templates. **Tier 2:** the seed's narration ENCODING on existing instances (first-boot rewrite) and the lenient decoder — must land for the codec, but the Novel row's INSERT may be ruled with C-4's parse (if the lane cannot parse `matchCurlyQuotes` / the `unmarked` scope it must NOT seed a half row — seed all three or refuse loudly). **Tier 3 (loud refusals):** none owed — every surface above is server-side and complete; the CLI verb stays the EXISTING `not yet available` refusal (R-P).

**P4.D342 — Tier 1:** the four prompts' bytes + `consolidation` (regen the two tier-1 families); `resolve_inner_circle` + signature + allowance; the frozen archive's `{anchors, circle}` with the cache key, steps 1–4 as at the pin INCLUDING the keystone lifting (one lane, one DEBUG); `render_inner_circle`; assemble's step 2 + trace keys; the tuning knobs end to end (strict schema, describe, defaults); the frozen-archive oracle re-aim + a group fixture; `recall_render` / `recall_assemble` / `recall_tuning` regens. **Tier 2:** `RecallReplayResult.warnings` + the CLI help capture; `find_names_by_ids` `exclude_archived`. **Tier 3:** none.

---

## 9. Fixtures

| fixture | consumer | who delivers |
|---|---|---|
| `services/provisioning/fresh_schema.json`, `migration_indexes.json` | provisioning, backfill, host boot, zero-rerun's planted base | P4.D341 re-dumps from the pin (`QT_REAL_BOOT_MASTER` from a real `tsx server.ts` first boot at `236c04e35` — recipe `surveys/2026-10-06-p4.153-v4-first-boot-indexes.md`) |
| `services/builtin_templates.json` | `builtin_templates_equivalence`, every boot's seed | P4.D341 re-dumps (`dump-builtin-templates.ts`, `QT_BUILTIN_OUT`) |
| the 21 `memories`-bearing pairs (+ the `chats` / `chat_messages` / `chat_settings` / `roleplay_templates` / `llm_logs` widenings across up to 49 main + the llm-logs siblings) | every real-DB family | P4.D341 widens in place (`--report-only` first; per-pair record); dependent oracles regenerate from the pin by their recipes — a shared `/tmp` recipe path per family (memory note `parallel-lane-shared-resources`) |
| `harness/oracle/fixtures/extraction-pure-tier1.json` (inputs unchanged; the NDJSON regenerates) | `memory_tasks_equivalence` | P4.D342 regenerates the oracle only |
| `fixtures/recall-assemble.json` + `build-recall-assemble-fixture.ts` | `recall_assemble_tier3_equivalence` | P4.D342 grows (a group of two in the mount partition; a keystone row; the three-circle probe) — rebuilt pair → `rebuilt-fixture-pair-vintage`: diff `sqlite_master` old/new and close gaps in the BUILDER via `runV4Migrations` |
| the frozen-archive family's fixture (measure which — `cases/frozen-archive-tier2.test.ts` reads a spec `.json`) | `frozen_archive_tier2_equivalence` | P4.D342 |
| `memory-keystone-tier2.json` (P4.D288's cursor corpus; NAME only) | `memory_extraction_cursors_tier2` | untouched — do not confuse it with this round's keystone |
| `crates/quilltap-cli/src/help/recall_replay_help.txt`, `help/completion/*.template` | `cli_differential` Tier R, `completion_behavior` | P4.D342 (help) / P4.D341 (templates), re-captured from the pin's `packages/quilltap` |
| `migration-vintage/` trio | `restore_vintage_state` | never rewritten; the mirror boot heals it |

---

## 10. Rulings owed to the human

1. **Q1 — the partial index's home.** Recommendation: the keystone ensure creates `idx_memories_keystone` itself (v4's `run` shape; the `memories_superseded_by_index.rs` precedent) AND the re-dumped `migration_indexes.json` carries it for fresh provisioning — not "P4.160 only" (headline 3). Default: yes, no ask needed unless the planner prefers otherwise.
2. **Q2 — `storyteller_rounds` on a pre-round v5 instance:** create it in v4's MIGRATION shape (FK + `DEFAULT NULL`s + `idx_storyteller_rounds_chat`) plus generateDDL's `idx_storyteller_rounds_createdAt`, or in generateDDL's shape (from the dump, the P4.D288 cursor-table "first-boot shape" ruling R-C) plus both indexes? Either leaves provisioned-vs-migrated table TEXT different (the standing R-A asymmetry); the FK decides whether a v5-provisioned-before-this-round instance cascades rounds on chat delete. Recommendation: the migration's DDL verbatim (what v4 does to an EXISTING instance; the P4.D315 pattern) + the createdAt index, with P4.D343 deleting rounds explicitly regardless (C-2).
3. **Q3 — the post-write 400** (`The memory was saved, but not as a keystone: …`): port the bytes (recommended; it is v4's contract) or file a v4 issue (`foundry-9/quilltap-server`, the human's go-ahead first) and still port the bytes.
4. **Q4 — `cascadeEmptiedDigests`' changed `added` count** (now only rows actually doomed): port as shipped (recommended — it only differs when a keystone digest is the sole emptied candidate; `digestsRetiredWithLastMember` becomes exact).
5. **Q5 — the every-boot narration rewrite on existing v5 instances** (two built-ins' `narrationDelimiters` `*` → `"*"` on the first boot after this lands; v4 does exactly this at the pin) — a dogfood note, not a divergence; flagged because it is a silent write to Friday's copy on first boot.
6. **Q6 — the `.qtap` schema file** (`generators/qtap-export.schema.json` + `qtap_schema_embed_guard`): does the keystone carry the memory / chat / message / template keys of its five moves, or does P4.D360 vendor the whole file in wave 0 (then the keystone's `qtap_schema_validate` rows must not depend on it)? Recommendation: the keystone vendors the file WHOLE at the pin (one byte copy; the guard pins it) and P4.D360 leaves it alone.
7. **Q7 — llm-logs zero-rerun coverage:** the `zero-rerun.ts` oracle plants ONE main file; the llm-logs move needs a planted logs sibling (recommended: add `planted-llm-logs.db` beside it and inspect both) or a recorded carve.
8. No new dependency in this area.

---

## 11. Dogfood rows (on a Friday COPY at any v4 point — the standing note)

1. First boot of the copy: every new ensure a NO-OP on a v4-migrated instance (Friday has all five migrations) — the boot log shows none of the five `Fitted …` lines; `/health` not degraded; `PRAGMA table_info(llm_logs)` has `storytellerRoundId`.
2. A copy taken BEFORE v4 ran `add-memory-keystone-v1` (or a v5 instance provisioned at `322a5488e`): the five ensures run once (five INFO lines), `idx_memories_keystone` + the two storyteller indexes + the llm-logs index present, second boot silent; `zero-rerun`'s claim in the wild: open the same copy with v4 at the pin afterwards — `migrations_state` gains NO new rows.
3. The built-ins after the first boot: `SELECT name, narrationDelimiters, unmarkedStyle, markdownAsterisks FROM roleplay_templates WHERE isBuiltIn = 1` → Standard `"*"` / NULL / 1, Quilltap RP `["[","]"]` / NULL / 1, Novel `null` / `{"style":"qt-roleplay-plain",…}` / 0; a chat using Quilltap RP still renders bracket narration (the decoded pair).
4. Mark a memory a keystone in the Commonplace Book (SPA, P4.D359) or via `PUT /api/v1/memories/<id> {"keystone": true}` on the running copy; `Keystones only` lists it; a sixth about the same subject answers 400 with v4's sentence; marking an archived (cold) row answers the archived sentence.
5. 💸 A Salon turn with the keystone's subject present: the request's context shows the keystone in FULL under `## Those Closest to You` ahead of the digests; with the subject absent but in a shared group, under the same heading in circle order; with the subject the character themself, among `## Memory Anchors` in full. The `[FrozenArchive] Composed archive` DEBUG carries `presentKeystones` / `circleKeystones`.
6. 💸 Two characters in a group of two + one in a group of forty: the `[InnerCircle] Resolved inner circle` DEBUG shows `tiers: 2`; the circle section lists the household member's three digests before the club member's one; archive the club member → the next turn drops them without a restart (the signature).
7. Maintenance on the keystone's holder (`quilltap maintenance` / the card): the keystone is never folded, never demoted (`protected` counts it), never a digest member; delete the last member of a keystone digest → the digest stays (`Keystone digest kept though its last member retires`).
8. Chat wipe (`DELETE /memories?chatId=`) on a chat whose extraction wrote a keystone: `sparedBy` = `{"keystone":1,"manual":…,"digest":…,"cold":…}` in that order.
9. 💸 Extraction after this round: a work agreement scores ≤ 0.65 and a family fact 0.95 (compare the `[Extraction]` rows' importance against the old ladder on the same transcript — a copy-level A/B, no fix implied).
10. A full backup → `replace` restore, and a `.qtap` export → import: every keystone survives as a keystone; the import of a pre-round bundle reads `false`.
11. `quilltap recall-replay --tuning '{"innerCircleShare":0.5,"innerCircleAllowance":[2]}'`: the description tail `innerCircleShare=0.5, innerCircleAllowance=[2]`; an unknown key still refused.
12. The CLI: `quilltap memories keystone …` answers the standing `not yet available` refusal (R-P); bash completion offers `keystone` and `--keystone` / `--off` (the captured bytes).
