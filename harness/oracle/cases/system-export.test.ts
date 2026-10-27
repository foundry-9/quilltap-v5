/**
 * @jest-environment node
 *
 * P4.9G4 `.qtap` EXPORT ORACLE: drives v4's REAL `createNdjsonStream` /
 * `streamExportRecords` (`lib/export/ndjson-writer.ts`), `previewExport`
 * (`lib/export/quilltap-export-service.ts:63`), and the route-level
 * `handleExportEntities` (`app/api/v1/system/tools/route.ts:440`) over a FRESH
 * copy of the committed `system-data-*` fixture family per case, emitting the
 * NDJSON **line by line** so the Rust writer can be diffed record-for-record.
 *
 * ── WHAT IS NORMALIZED ───────────────────────────────────────────────────────
 * Only `manifest.createdAt` (wall clock) — and only on the envelope, where v4
 * stamps it. `appVersion` is NOT normalized: the Rust differential passes v4's
 * own `package.json` version (emitted here as `_meta.appVersion`) so the line is
 * byte-identical. Every other byte of every other line is exact.
 *
 * ── THE CASE MATRIX ──────────────────────────────────────────────────────────
 * All FIFTEEN entity types × `scope: all` (`7189a968`), plus `scope: selected` and
 * `includeMemories` variants where they change behaviour, plus the unknown-type
 * throw and the `previewExport` / `export-entities` surfaces.
 *
 * Run (Node 24, from the v4 checkout — cp to a /tmp mirror; jest ignores .claude/):
 *   N=~/.nvm/versions/node/v24.13.1/bin ; V5W=<this worktree>
 *   TMPO=/tmp/qt-sysexport-oracle
 *   rm -rf "$TMPO"; mkdir -p "$TMPO/cases" "$TMPO/fixtures" "$TMPO/lib"
 *   cp "$V5W/harness/oracle/cases/system-export.test.ts" "$TMPO/cases/"
 *   cp "$V5W/harness/oracle/lib/v4-migrations.ts" "$TMPO/lib/"
 *   cp "$V5W/harness/oracle/fixtures/system-data.json" "$TMPO/fixtures/"
 *   cp "$V5W/harness/oracle/fixtures/apply-memory-tiers-migration.ts" "$TMPO/fixtures/"
 *   cd ~/source/quilltap-server
 *   QT_FIXTURE_SD_MAIN=$V5W/crates/quilltap-web/tests/fixtures/system-data-main.db \
 *   QT_FIXTURE_SD_MOUNT=$V5W/crates/quilltap-web/tests/fixtures/system-data-mount.db \
 *   QT_FIXTURE_SD_LLM=$V5W/crates/quilltap-web/tests/fixtures/system-data-llmlogs.db \
 *   QT_ORACLE_OUT=/tmp/oracle-system-export.ndjson \
 *     $N/npx jest --silent --watchman=false --testTimeout=300000 \
 *       --roots "$PWD" --roots "$TMPO/cases" -- system-export
 */

import * as fs from 'fs';
import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import { mkdtempSync, mkdirSync, copyFileSync, existsSync, rmSync } from 'node:fs';
import { applyMemoryTiersMigration } from '../fixtures/apply-memory-tiers-migration';
import { tmpdir } from 'node:os';
import { plantChatWardrobeHistoryTable, plantInventoryColumns } from '../lib/v4-migrations';

interface Spec {
  testPepperBase64: string;
  userId: string;
}

const LORIAN = 'a1000000-0000-4000-8000-000000000001';
const CHAT_1 = 'c1000000-0000-4000-8000-000000000001';
const TAG_1 = 'a5000000-0000-4000-8000-000000000001';

function applyMocks(userId: string): void {
  const cipherDriverPath = require('node:path').join(
    process.cwd(),
    'packages/quilltap/node_modules/better-sqlite3-multiple-ciphers',
  );
  jest.doMock('better-sqlite3', () => jest.requireActual(cipherDriverPath));
  jest.doMock('@/lib/database/manager', () => jest.requireActual('@/lib/database/manager'));
  jest.doMock('@/lib/repositories/factory', () => jest.requireActual('@/lib/repositories/factory'));
  // [P4.D46] jest.setup.ts globally mocks the storage manager with canned
  // bytes — `streamFiles` must read REAL bytes (mount-blob keys hit the mount
  // DB; the fixture's legacy disk key has no bytes anywhere, which is the
  // `_bytesMissing` arm).
  jest.doMock('@/lib/file-storage/manager', () =>
    jest.requireActual('@/lib/file-storage/manager'),
  );
  // [P4.D46] The plugin registry is EMPTY under jest (loadPlugins never runs),
  // but production has every bundled plugin registered — and the plugin-config
  // redaction resolves manifests through it. Serve the REAL manifest.json
  // straight from plugins/dist so redaction sees production's data; anything
  // else (an npm-installed plugin, the fixture's synthetic name) resolves
  // null, the withhold-everything arm.
  jest.doMock('@/lib/plugins/registry', () => {
    const actual = jest.requireActual('@/lib/plugins/registry');
    const rfs = require('node:fs');
    const rpath = require('node:path');
    return {
      __esModule: true,
      ...actual,
      getPlugin: (name: string) => {
        const manifestPath = rpath.join(process.cwd(), 'plugins', 'dist', name, 'manifest.json');
        if (!rfs.existsSync(manifestPath)) return null;
        return { manifest: JSON.parse(rfs.readFileSync(manifestPath, 'utf8')) };
      },
    };
  });
  jest.doMock('@/lib/auth/session', () => ({
    __esModule: true,
    ...jest.requireActual('@/lib/auth/session'),
    getServerSession: async () => ({ user: { id: userId } }),
  }));
  jest.doMock('@/lib/startup/startup-state', () => {
    const actual = jest.requireActual('@/lib/startup/startup-state');
    return {
      __esModule: true,
      ...actual,
      startupState: {
        ...actual.startupState,
        isReady: () => true,
        waitForReady: async () => true,
        isPepperResolved: () => true,
        getPepperState: () => 'resolved',
        getPhase: () => 'ready',
        isLockedMode: () => false,
      },
    };
  });
}

function mockRequest(url: string, method: string, body: unknown): unknown {
  return {
    method,
    url,
    nextUrl: new URL(url),
    headers: new Headers({ 'Content-Type': 'application/json' }),
    json: jest.fn().mockResolvedValue(body ?? {}),
  };
}

/** Drain a `ReadableStream<Uint8Array>` into the array of NDJSON lines. */
async function drain(stream: ReadableStream<Uint8Array>): Promise<string[]> {
  const reader = stream.getReader();
  const chunks: Uint8Array[] = [];
  for (;;) {
    const { value, done } = await reader.read();
    if (done) break;
    if (value) chunks.push(value);
  }
  const total = chunks.reduce((n, c) => n + c.byteLength, 0);
  const merged = new Uint8Array(total);
  let off = 0;
  for (const c of chunks) {
    merged.set(c, off);
    off += c.byteLength;
  }
  const text = new TextDecoder('utf-8').decode(merged);
  return text.split('\n').filter((l) => l.length > 0);
}

interface ExportOpts {
  type: string;
  scope: 'all' | 'selected';
  selectedIds?: string[];
  includeMemories?: boolean;
}

interface CaseSpec {
  name: string;
  run: () => Promise<Record<string, unknown>>;
  /** [P4.D264] Plant the wardrobe carriers' material first (`plantWardrobe`). */
  wardrobe?: boolean;
  /** [P4.D294] Rebuild the cursor table to throw on read (`plantThrowingCursors`). */
  cursorThrow?: boolean;
  /** [P4.D294 Tier 2 #19] `CHAT_1`'s stored `scenarioSource` a non-object TEXT cell. */
  scenarioTextCell?: boolean;
  /** [P4.D317] Outfit-history rows through v4's REAL `upsertRows` (`HISTORY_ROWS`). */
  outfitHistory?: boolean;
  /** [P4.D317] Do NOT plant `chat_wardrobe_history`: the per-chat read throws. */
  noHistoryTable?: boolean;
  /** [P4.D327] `INVENTORY_CELLS_SQL`: the two Inventory cells set on the copy. */
  inventoryCells?: boolean;
}

function buildCases(spec: Spec): CaseSpec[] {
  const streamCase = (name: string, options: ExportOpts): CaseSpec => ({
    name,
    run: async () => {
      const { createNdjsonStream } = await import('@/lib/export/ndjson-writer');
      // [P4.D294] the `[ExtractionCursors]` lines, a silence leg on every case.
      // [P4.D317] + the outfit-history lines (`historyLogs`).
      // [P4.167 H-3] + the unreadable-chat lines (`findByIdLogs`).
      const { out: lines, cursorLogs, historyLogs, findByIdLogs } = await withCursorLogs(() =>
        drain(createNdjsonStream(spec.userId, options as never)),
      );
      return { kind: 'ndjson', options, lines, cursorLogs, historyLogs, findByIdLogs };
    },
  });

  const previewCase = (name: string, options: ExportOpts): CaseSpec => ({
    name,
    run: async () => {
      const { previewExport } = await import('@/lib/export/quilltap-export-service');
      // [P4.167 H-3] + the unreadable-chat lines (`findByIdLogs`).
      const { out: preview, findByIdLogs } = await withCursorLogs(() =>
        previewExport(spec.userId, options as never),
      );
      return { kind: 'preview', options, body: preview, findByIdLogs };
    },
  });

  const entitiesCase = (name: string, type: string): CaseSpec => ({
    name,
    run: async () => {
      const route = await import('@/app/api/v1/system/tools/route');
      const resp = (await (route as never as { GET: (r: unknown) => Promise<Response> }).GET(
        mockRequest(
          `http://localhost/api/v1/system/tools?action=export-entities&type=${type}`,
          'GET',
          {},
        ),
      )) as unknown as { status: number; json: () => Promise<unknown> };
      return { kind: 'entities', type, status: resp.status, body: await resp.json() };
    },
  });

  // The fifteen `7189a968` types, in v4's declaration order.
  const TYPES = [
    'characters',
    'chats',
    'roleplay-templates',
    'prompt-templates',
    'connection-profiles',
    'image-profiles',
    'embedding-profiles',
    'tags',
    'projects',
    'groups',
    'document-stores',
    'files',
    'provider-models',
    'plugin-configs',
    'instance-settings',
  ];

  const cases: CaseSpec[] = [];

  // 1. Every type, scope 'all', memories OFF.
  for (const type of TYPES) {
    cases.push(streamCase(`stream_${type}_all`, { type, scope: 'all', includeMemories: false }));
  }

  // 2. The two memory-bearing types with memories ON.
  cases.push(
    streamCase('stream_characters_all_memories', {
      type: 'characters',
      scope: 'all',
      includeMemories: true,
    }),
  );
  cases.push(
    streamCase('stream_chats_all_memories', {
      type: 'chats',
      scope: 'all',
      includeMemories: true,
    }),
  );

  // 3. scope 'selected' — a hit, an empty list, and a miss (nonexistent id).
  cases.push(
    streamCase('stream_characters_selected_one', {
      type: 'characters',
      scope: 'selected',
      selectedIds: [LORIAN],
      includeMemories: true,
    }),
  );
  cases.push(
    streamCase('stream_characters_selected_empty', {
      type: 'characters',
      scope: 'selected',
      selectedIds: [],
      includeMemories: false,
    }),
  );
  cases.push(
    streamCase('stream_tags_selected_missing', {
      type: 'tags',
      scope: 'selected',
      selectedIds: ['ffffffff-ffff-4fff-8fff-ffffffffffff', TAG_1],
      includeMemories: false,
    }),
  );
  cases.push(
    streamCase('stream_chats_selected_one', {
      type: 'chats',
      scope: 'selected',
      selectedIds: [CHAT_1],
      includeMemories: false,
    }),
  );

  // 4. The unknown-type throw (resolveExportIds :650 / streamExportRecords :714).
  cases.push({
    name: 'stream_unknown_type_throws',
    run: async () => {
      const { createNdjsonStream } = await import('@/lib/export/ndjson-writer');
      try {
        await drain(
          createNdjsonStream(spec.userId, {
            type: 'nonsense',
            scope: 'all',
            includeMemories: false,
          } as never),
        );
        return { kind: 'throw', error: null };
      } catch (err) {
        return { kind: 'throw', error: err instanceof Error ? err.message : String(err) };
      }
    },
  });

  // 5. previewExport — every type at scope 'all', plus the memory + selected arms.
  for (const type of TYPES) {
    cases.push(previewCase(`preview_${type}_all`, { type, scope: 'all', includeMemories: false }));
  }
  cases.push(
    previewCase('preview_characters_memories', {
      type: 'characters',
      scope: 'all',
      includeMemories: true,
    }),
  );
  cases.push(
    previewCase('preview_chats_memories', {
      type: 'chats',
      scope: 'all',
      includeMemories: true,
    }),
  );
  cases.push(
    previewCase('preview_characters_selected', {
      type: 'characters',
      scope: 'selected',
      selectedIds: [LORIAN],
      includeMemories: true,
    }),
  );
  cases.push({
    name: 'preview_unknown_type_throws',
    run: async () => {
      const { previewExport } = await import('@/lib/export/quilltap-export-service');
      try {
        await previewExport(spec.userId, {
          type: 'nonsense',
          scope: 'all',
          includeMemories: false,
        } as never);
        return { kind: 'throw', error: null };
      } catch (err) {
        return { kind: 'throw', error: err instanceof Error ? err.message : String(err) };
      }
    },
  });

  // 6. The ?action=export-entities route — the switch is exhaustive at
  //    `7189a968` (groups + document-stores no longer 400), plus one bogus value.
  for (const type of TYPES) {
    cases.push(entitiesCase(`entities_${type}`, type));
  }
  cases.push(entitiesCase('entities_bogus', 'nonsense'));

  // ── P4.D264 (v4 `3ee3b1342` #81 + `7c8572869` #82): the wardrobe carriers ─
  // Over the `plantWardrobe` copy: the coat ARCHIVED and pointing at its
  // picture, `portrait.png` linked to it, and three ledger rows (one for an
  // item no export carries). `characters` emits the archived coat with
  // `_imageFiles` LAST and the trailing `wardrobe_wear` records; a
  // `document-stores` export collects the coat off its `Wardrobe/*.md`
  // document; `projects` streams no store and so carries no ledger; and the
  // preview counts no `wardrobeWear` (v4's `previewExport` is untouched).
  const wardrobe = (c: CaseSpec): CaseSpec => ({ ...c, wardrobe: true });
  cases.push(
    wardrobe(
      streamCase('stream_characters_all_wardrobe', {
        type: 'characters',
        scope: 'all',
        includeMemories: false,
      }),
    ),
  );
  cases.push(
    wardrobe(
      streamCase('stream_document-stores_all_wardrobe', {
        type: 'document-stores',
        scope: 'all',
        includeMemories: false,
      }),
    ),
  );
  cases.push(
    wardrobe(streamCase('stream_projects_all_wardrobe', { type: 'projects', scope: 'all', includeMemories: false })),
  );
  cases.push(
    wardrobe(
      previewCase('preview_characters_wardrobe', {
        type: 'characters',
        scope: 'all',
        includeMemories: false,
      }),
    ),
  );

  // 8. [P4.D294] The bundle-cursor read arms, over the throwing cursor table
  //    (`plantThrowingCursors`): a multi-chat export (v4 `loadBundleCursors`,
  //    then `withBundleCursorFields` per chat when the one read fails) and a
  //    one-chat export (the per-chat read alone). Name suffix `_cursor_throw`
  //    — the Rust side reads its own planted copy.
  const cursorThrow = (c: CaseSpec): CaseSpec => ({ ...c, cursorThrow: true });
  cases.push(
    cursorThrow(streamCase('stream_chats_all_cursor_throw', { type: 'chats', scope: 'all', includeMemories: false })),
  );
  cases.push(
    cursorThrow(
      streamCase('stream_chats_selected_cursor_throw', {
        type: 'chats',
        scope: 'selected',
        selectedIds: [CHAT_1],
        includeMemories: false,
      }),
    ),
  );

  // 10. [P4.D317] The outfit-history record (v4 `7953da3a0`,
  //     `ndjson-writer.ts:423-437`): over `HISTORY_ROWS` (Lorian's two entries
  //     — a `pascal` row with `actorToolName` — and Riya's null-actor entry in
  //     CHAT_1; CHAT_2 has none) a `chats` export emits `wardrobe_history`
  //     records after CHAT_1's informs and before its annotations, by
  //     character then index, `data` in `normalizeRow` order, the footer count
  //     by first bump; a `characters` export emits no such kind; the preview
  //     carries no `wardrobeHistory`. Without the table every chat's read
  //     THROWS: the WARN per chat, and the export carries on.
  const history = (c: CaseSpec): CaseSpec => ({ ...c, outfitHistory: true });
  cases.push(
    history(streamCase('stream_chats_all_outfit_history', { type: 'chats', scope: 'all', includeMemories: false })),
  );
  cases.push(
    history(
      streamCase('stream_characters_all_outfit_history', {
        type: 'characters',
        scope: 'all',
        includeMemories: false,
      }),
    ),
  );
  cases.push(
    history(previewCase('preview_chats_outfit_history', { type: 'chats', scope: 'all', includeMemories: false })),
  );
  cases.push({
    ...streamCase('stream_chats_all_no_history_table', { type: 'chats', scope: 'all', includeMemories: false }),
    noHistoryTable: true,
  });

  // 11. [P4.D327] The two Inventory keys in their Zod slots (`INVENTORY_CELLS_SQL`):
  //     a `chats` export's CHAT_1 line carries `equippedInventory` right after
  //     `equippedOutfit`; a `characters` export's lines carry
  //     `canManageInventory` right after `canCreateOutfits`.
  const inventory = (c: CaseSpec): CaseSpec => ({ ...c, inventoryCells: true });
  cases.push(
    inventory(streamCase('stream_chats_all_inventory', { type: 'chats', scope: 'all', includeMemories: false })),
  );
  cases.push(
    inventory(
      streamCase('stream_characters_all_inventory', { type: 'characters', scope: 'all', includeMemories: false }),
    ),
  );

  // 9. [P4.D294 Tier 2 #19] A non-object TEXT `scenarioSource` cell, exported.
  cases.push({
    ...streamCase('stream_chats_selected_one_scenario_text', {
      type: 'chats',
      scope: 'selected',
      selectedIds: [CHAT_1],
      includeMemories: false,
    }),
    scenarioTextCell: true,
  });
  // [P4.167 H-3] …and previewed: v4's `previewExport` reads each selected chat
  // through the same non-strict `findById` (`quilltap-export-service.ts:154`),
  // so the chat is left out of the entity list with the same two lines.
  cases.push({
    ...previewCase('preview_chats_selected_one_scenario_text', {
      type: 'chats',
      scope: 'selected',
      selectedIds: [CHAT_1],
      includeMemories: false,
    }),
    scenarioTextCell: true,
  });

  return cases;
}


/** [P4.88] A two-id drawn rotation — no default produces this. */
const P4D171_ROTATION =
  '["e1000000-0000-4000-8000-000000000001","e1000000-0000-4000-8000-0000000000e2"]';
/** [P4.88] A two-attempt route trail with DISTINCT `profileId`s. */
const P4D171_ROUTE_TRAIL =
  '[{"profileId":"c0000001-0000-4000-8000-000000000001","profileName":"Primary","provider":"OPENAI","modelName":"gpt-4o","via":"primary","outcome":"failed","trigger":"rate-limit","detail":"429 slow down"},{"profileId":"c0000001-0000-4000-8000-0000000000f2","profileName":"Understudy","provider":"ANTHROPIC","modelName":"claude-x","via":"understudy","outcome":"answered"}]';
const P4D171_CHAT_ID = 'c1000000-0000-4000-8000-000000000001';
const P4D171_MESSAGE_ID = 'd1000000-0000-4000-8000-000000000002';
const P4D182_GENERATION_KEY = 'a3000000-0000-4000-8000-000000000001';
const P4D182_FILE_ID = 'f0000001-0000-4000-8000-000000000001';

/**
 * P4.D182 (v4 `7fbf8a55b`): plant a NON-NULL `files.generationKey`, so the
 * export/import legs measure the carry rather than a column of nulls.
 *
 * The planted VALUE is deliberately a real UUID that this archive remaps
 * elsewhere (`PROJECT_1`'s id): the avatar cache key travels AS-IS — v4's
 * vendored export schema says so in as many words — so a remap-shaped value
 * coming back out unchanged is the only way to see the difference between
 * "carried" and "carried and rewritten".
 *
 * The other `files` rows keep NULL, which is what proves the omit-when-null
 * rule at the same time. P4.111 widened the committed `system-data-main.db`
 * in place at v4 `00c290c9a` through v4's own migration statements, so it
 * now CARRIES `files.generationKey` (+ `idx_files_generationKey`) and
 * `chats.transcriptVersion` natively (`pragma_table_info` proof: P4.117
 * lane record) — the `addColumnIfMissing`/`CREATE INDEX` guards this
 * function used to run are dead and removed; the planted VALUES are
 * unchanged.
 */
function plantP4d182Values(db: { exec(sql: string): unknown; prepare(sql: string): { run(...a: unknown[]): unknown; all(...a: unknown[]): unknown } }): void {
  db.prepare(`UPDATE "files" SET "generationKey" = ? WHERE "id" = ?`).run(
    P4D182_GENERATION_KEY,
    P4D182_FILE_ID,
  );
  // …and a bumped transcript counter, which must reach NO export record: v4
  // keeps the column out of `ChatMetadataSchema`, so nothing serializes it.
  db.prepare(`UPDATE "chats" SET "transcriptVersion" = 7 WHERE "id" = ?`).run(P4D171_CHAT_ID);
}

/**
 * [P4.88] Plant NON-DEFAULT values in the two `78b381a96` columns.
 *
 * The committed `system-data-*` fixture now carries both columns natively
 * (P4.111 widened it in place at v4 `00c290c9a`; `pragma_table_info` proof:
 * P4.117 lane record), so this only UPDATEs — values no default could
 * produce: a two-id rotation on chat 1, and a two-attempt route trail with
 * DISTINCT `profileId`s on its assistant message. Without this, every
 * export/import/restore family exercises those columns only at their Zod
 * defaults (`'[]'` and absent), which is the same bytes whether the carry
 * works or not.
 *
 * The Rust side plants the identical values through the twin
 * `plant_p4d171_values`, so the two engines provably start from the same
 * cells.
 */
function plantP4d171Values(db: { exec(sql: string): unknown; prepare(sql: string): { run(...a: unknown[]): unknown; all(...a: unknown[]): unknown } }): void {
  db.prepare(`UPDATE "chats" SET "cycleOrderParticipantIds" = ? WHERE "id" = ?`).run(
    P4D171_ROTATION,
    P4D171_CHAT_ID,
  );
  db.prepare(`UPDATE "chat_messages" SET "routeTrail" = ? WHERE "id" = ?`).run(
    P4D171_ROUTE_TRAIL,
    P4D171_MESSAGE_ID,
  );
}

/** [P4.D264] The coat's frontmatter edit (archived + a current picture). */
const COAT_EDIT_FROM = 'imagePrompt: brown coat\n';
const COAT_EDIT_TO =
  'imagePrompt: brown coat\narchived: true\narchivedAt: 2026-03-04T00:00:00.000Z\n' +
  'imageFileId: f0000001-0000-4000-8000-000000000001\n';

/**
 * [P4.D264] The wardrobe carriers' plant — plain SQL, run IDENTICALLY by the
 * Rust twin `plant_wardrobe` (no write clock reaches the export bytes): the
 * coat's document gains `archived` / `archivedAt` / `imageFileId`, the
 * `portrait.png` IMAGE row is linked to the coat, and `wardrobe_wear_stats`
 * is created through v4's REAL migration DDL with three rows inserted
 * verbatim — the coat × Lorian, the coat × unattributed, and an item no
 * export carries (which must never be emitted).
 */
const WEAR_PLANT_SQL =
  'INSERT INTO "wardrobe_wear_stats" ("id", "itemId", "wearerCharacterId", "wearCount", ' +
  '"firstWornAt", "lastWornAt", "lastWornChatId", "createdAt", "updatedAt") VALUES ' +
  "('3e0000e1-0000-4000-8000-0000000000e1', 'ac000000-0000-4000-8000-000000000001', " +
  "'a1000000-0000-4000-8000-000000000001', 3, '2026-03-02T00:00:00.000Z', " +
  "'2026-03-05T00:00:00.000Z', 'c1000000-0000-4000-8000-000000000001', " +
  "'2026-03-02T00:00:00.000Z', '2026-03-05T00:00:00.000Z'), " +
  "('3e0000e2-0000-4000-8000-0000000000e2', 'ac000000-0000-4000-8000-000000000001', NULL, 2, " +
  "'2026-03-01T00:00:00.000Z', '2026-03-03T00:00:00.000Z', NULL, " +
  "'2026-03-01T00:00:00.000Z', '2026-03-03T00:00:00.000Z'), " +
  "('3e0000e3-0000-4000-8000-0000000000e3', 'ae0000e3-0000-4000-8000-0000000000e3', " +
  "'a1000000-0000-4000-8000-000000000002', 9, '2026-03-01T00:00:00.000Z', " +
  "'2026-03-09T00:00:00.000Z', NULL, '2026-03-01T00:00:00.000Z', '2026-03-09T00:00:00.000Z')";

async function plantWardrobe(): Promise<void> {
  const { getRawDatabase } = await import('@/lib/database/backends/sqlite/client');
  const { getRawMountIndexDatabase } = await import(
    '@/lib/database/backends/sqlite/mount-index-client'
  );
  const { WARDROBE_WEAR_STATS_DDL } = await import(
    '@/lib/database/backends/sqlite/wardrobe-wear-stats-ddl'
  );
  const main = getRawDatabase()!;
  for (const statement of WARDROBE_WEAR_STATS_DDL) main.exec(statement);
  main.exec(WEAR_PLANT_SQL);
  const linked = main
    .prepare(
      `UPDATE "files" SET "linkedTo" = '["ac000000-0000-4000-8000-000000000001"]' WHERE "id" = ?`,
    )
    .run('f0000001-0000-4000-8000-000000000001') as { changes: number };
  if (linked.changes !== 1) throw new Error('plantWardrobe: portrait.png not found');
  const mount = getRawMountIndexDatabase()!;
  const edited = mount
    .prepare(
      `UPDATE "doc_mount_documents" SET "content" = replace("content", ?, ?) ` +
        `WHERE "content" LIKE '%title: Travelling Coat%'`,
    )
    .run(COAT_EDIT_FROM, COAT_EDIT_TO) as { changes: number };
  if (edited.changes !== 1) throw new Error('plantWardrobe: the coat document not found');
}

/**
 * [P4.D272] The memory-tier cells, planted on every case's copy after the
 * migration (the Rust twin `plant_memory_tiers_values` runs these SAME
 * statements, byte for byte — `MEMORY_TIERS_PLANT_SQL`): `MEM_1` becomes a
 * CONSOLIDATED hot digest of `MEM_3`, `MEM_3` its cold superseded member,
 * `CHAT_1` carries three extraction-cursor rows (P4.D294 — the chat column
 * they replaced is gone at `83334b9a5`), a chosen scenario, and `MEM_3` a
 * cold-retention clock; and the two
 * new instance settings get raw rows (Tier 2 item 13 — they travel raw:
 * `NON_PORTABLE_INSTANCE_SETTING_KEYS` does not name them). Without it every
 * export line carries the columns at their defaults, the same bytes whether
 * the key order is right or not. Each statement must touch exactly one row.
 */
const MEMORY_TIERS_PLANT_SQL: string[] = [
  "UPDATE \"memories\" SET \"source\" = 'CONSOLIDATED', \"tier\" = 'hot', \"consolidatedFrom\" = '[\"ad000000-0000-4000-8000-000000000003\"]', \"consolidatedAt\" = '2026-01-01T00:00:00.000Z' WHERE \"id\" = 'ad000000-0000-4000-8000-000000000001'",
  "UPDATE \"memories\" SET \"tier\" = 'cold', \"supersededById\" = 'ad000000-0000-4000-8000-000000000001', \"consolidatedAt\" = '2026-01-01T00:00:00.000Z' WHERE \"id\" = 'ad000000-0000-4000-8000-000000000003'",
  // [P4.D294] `ac6348c5f` moved the cursors off the chat row: `CHAT_1`'s
  // three cursor ROWS (the bundle carries them back on the chat under the
  // legacy keys, appended after every schema key), its chosen scenario
  // (`545e299bb`'s pair), and `MEM_3`'s cold-retention clock (`c2795733f`).
  "INSERT INTO \"memory_extraction_cursors\" (\"id\", \"chatId\", \"pass\", \"messageId\", \"createdAt\", \"updatedAt\") VALUES ('c1000000-0000-4000-8000-000000000001:other-fold', 'c1000000-0000-4000-8000-000000000001', 'other-fold', 'd1000000-0000-4000-8000-000000000001', '2026-01-01T00:00:00.000Z', '2026-01-01T00:00:00.000Z')",
  "INSERT INTO \"memory_extraction_cursors\" (\"id\", \"chatId\", \"pass\", \"messageId\", \"createdAt\", \"updatedAt\") VALUES ('c1000000-0000-4000-8000-000000000001:episode-fold', 'c1000000-0000-4000-8000-000000000001', 'episode-fold', 'd1000000-0000-4000-8000-000000000002', '2026-01-01T00:00:00.000Z', '2026-01-01T00:00:00.000Z')",
  "INSERT INTO \"memory_extraction_cursors\" (\"id\", \"chatId\", \"pass\", \"messageId\", \"createdAt\", \"updatedAt\") VALUES ('c1000000-0000-4000-8000-000000000001:turn', 'c1000000-0000-4000-8000-000000000001', 'turn', 'd1000000-0000-4000-8000-000000000001', '2026-01-01T00:00:00.000Z', '2026-01-01T00:00:00.000Z')",
  "UPDATE \"chats\" SET \"scenarioSource\" = '{\"mountPointId\":\"e2000000-0000-4000-8000-0000000000f3\",\"path\":\"Scenarios/Harbour.md\",\"bodySha256\":\"0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef\"}', \"scenarioNotes\" = 'Bring the lantern.' WHERE \"id\" = 'c1000000-0000-4000-8000-000000000001'",
  "UPDATE \"memories\" SET \"supersededAt\" = '2026-01-02T00:00:00.000Z' WHERE \"id\" = 'ad000000-0000-4000-8000-000000000003'",
  "INSERT INTO \"instance_settings\" (\"key\", \"value\") VALUES ('memoryConsolidation', '{\"enabled\":true,\"watermark\":25}')",
  "INSERT INTO \"instance_settings\" (\"key\", \"value\") VALUES ('memoryExtractionMode', '{\"otherPass\":\"fold\"}')",
];

function plantMemoryTiersValues(db: { prepare(sql: string): { run(...a: unknown[]): unknown } }): void {
  for (const sql of MEMORY_TIERS_PLANT_SQL) {
    const { changes } = db.prepare(sql).run() as { changes: number };
    if (changes !== 1) throw new Error(`plantMemoryTiersValues: ${changes} rows for ${sql}`);
  }
}

/**
 * [P4.D294] The throwing cursor table (memory note
 * `throwing-plant-must-fail-both-reads`, P4.D293's recipe): rebuilt with
 * `messageId` itself a VIRTUAL `json("pass")` column, so every read that names
 * it throws `malformed JSON` on both sides — but only for chats that HAVE rows
 * (`CHAT_1`'s three). v4's two bundle reads sit on FALLBACK base reads
 * (`findByFilter` / `_findAll`), so its export — which runs non-strict —
 * answers "no cursors" with no `[ExtractionCursors]` WARN; v5 runs the export
 * inside the strict-repository scope (the human's 2026-10-02 ruling, a
 * recorded divergence), so its reads fail and the WARNs fire. The NDJSON bytes
 * are the same either way; the Rust side pins both log sets.
 */
const THROWING_CURSORS_SQL = [
  `ALTER TABLE "memory_extraction_cursors" RENAME TO "qt_cursors_old"`,
  `CREATE TABLE "memory_extraction_cursors" ("id" TEXT PRIMARY KEY, "chatId" TEXT NOT NULL, "pass" TEXT NOT NULL, "createdAt" TEXT NOT NULL, "updatedAt" TEXT NOT NULL)`,
  `INSERT INTO "memory_extraction_cursors" SELECT "id", "chatId", "pass", "createdAt", "updatedAt" FROM "qt_cursors_old"`,
  `DROP TABLE "qt_cursors_old"`,
  // Added AFTER the rows: SQLite evaluates a generated column on INSERT.
  `ALTER TABLE "memory_extraction_cursors" ADD COLUMN "messageId" TEXT AS (json("pass")) VIRTUAL`,
];

/**
 * [P4.D294 Tier 2 #19] `CHAT_1`'s stored `scenarioSource` a non-object TEXT
 * cell (`"Scenarios/Harbour.md"` — valid JSON, a STRING): what v4's chats
 * repository read and C1-9's `chats_read` each make of it, exported.
 */
/**
 * [P4.D327] The two Inventory cells (v4 `4fbdad186` / `e1872c94f`): CHAT_1
 * carries an object for Lorian, Lorian `canManageInventory = 0`, Riya `1`. v4
 * has no `lib/export` hunk — the row is spread whole and `schema.parse`
 * orders it, so each key lands in its Zod slot (`equippedInventory` after
 * `equippedOutfit`, `canManageInventory` after `canCreateOutfits`). The Rust
 * side runs the identical statements on `…_inventory` cases.
 */
const INVENTORY_CELLS_SQL = [
  `UPDATE "chats" SET "equippedInventory" = '{"a1000000-0000-4000-8000-000000000001":["1e000000-0000-4000-8000-0000000000c1"]}' WHERE "id" = 'c1000000-0000-4000-8000-000000000001'`,
  `UPDATE "characters" SET "canManageInventory" = 0 WHERE "id" = 'a1000000-0000-4000-8000-000000000001'`,
  `UPDATE "characters" SET "canManageInventory" = 1 WHERE "id" = 'a1000000-0000-4000-8000-000000000002'`,
];

const SCENARIO_TEXT_CELL_SQL = `UPDATE "chats" SET "scenarioSource" = '"Scenarios/Harbour.md"' WHERE "id" = 'c1000000-0000-4000-8000-000000000001'`;

/**
 * [P4.D317] The export's outfit-history plant (`stream_*_outfit_history`),
 * through v4's REAL `upsertRows` (fixed ids and stamps; the hash recomputed):
 * Lorian's entry 0 (`seeded`) and entry 1 (`pascal`, `actorToolName`, the
 * cursor), and Riya's null-actor entry in CHAT_1 — so `findByChatId`'s
 * `characterId` order shows. The Rust family writes the same cells through
 * `upsert_raw_rows`.
 */
const EMPTY_SLOTS = { top: [], bottom: [], footwear: [], accessories: [], hair: [] };
const HISTORY_ROWS = [
  {
    id: '7e0000e1-0000-4000-8000-0000000000e1', chatId: CHAT_1, characterId: 'a1000000-0000-4000-8000-000000000002',
    entryIndex: 0, slots: EMPTY_SLOTS, slotsHash: '', source: 'chat-start', actorCharacterId: null,
    actorToolName: null, isCurrent: true, createdAt: '2026-03-01T00:00:00.000Z',
  },
  {
    id: '7e0000e2-0000-4000-8000-0000000000e2', chatId: CHAT_1, characterId: LORIAN,
    entryIndex: 1, slots: { ...EMPTY_SLOTS, top: ['ac000000-0000-4000-8000-000000000001'] }, slotsHash: '',
    source: 'pascal', actorCharacterId: LORIAN, actorToolName: 'dress_for_dinner', isCurrent: true,
    createdAt: '2026-03-03T00:00:00.000Z',
  },
  {
    id: '7e0000e3-0000-4000-8000-0000000000e3', chatId: CHAT_1, characterId: LORIAN,
    entryIndex: 0, slots: EMPTY_SLOTS, slotsHash: '', source: 'seeded', actorCharacterId: null,
    actorToolName: null, isCurrent: false, createdAt: '2026-03-02T00:00:00.000Z',
  },
];

async function plantOutfitHistory(): Promise<void> {
  const { getRepositories } = await import('@/lib/repositories/factory');
  const written = await getRepositories().wardrobeHistory.upsertRows(HISTORY_ROWS as never);
  if (written !== HISTORY_ROWS.length) throw new Error(`plantOutfitHistory: wrote ${written}`);
}

function plantThrowingCursors(db: { exec(sql: string): unknown }): void {
  for (const sql of THROWING_CURSORS_SQL) db.exec(sql);
}

/**
 * [P4.D294] Every `[ExtractionCursors]` line v4 logs during a case, as
 * `"<level> <message> k=v …"` (`module` dropped; an Error as its message) —
 * `cursorLogs` on every stream case, so a case where none fires is a silence
 * leg. The singleton and every child logger share the prototype.
 */
async function withCursorLogs<T>(
  body: () => Promise<T>,
): Promise<{ out: T; cursorLogs: string[]; historyLogs: string[]; findByIdLogs: string[] }> {
  const { Logger } = await import('@/lib/logger');
  const cursorLogs: string[] = [];
  // [P4.D317] `ndjson-writer.ts`'s two outfit-history lines, same rendering.
  const historyLogs: string[] = [];
  const HISTORY = new Set(['Exported outfit history', 'Failed to load outfit history for export']);
  // [P4.167 H-3] The base repository's validate + `_findById` lines: an
  // unreadable chat row read through the non-strict `findById` logs ERROR
  // `Data validation failed {collection, error}` then ERROR `Error finding
  // entity by ID {id, error}` and answers `null` (the chat is skipped).
  const findByIdLogs: string[] = [];
  const FIND_BY_ID = new Set(['Data validation failed', 'Error finding entity by ID']);
  const originals = {
    error: Logger.prototype.error,
    warn: Logger.prototype.warn,
    info: Logger.prototype.info,
    debug: Logger.prototype.debug,
  };
  for (const level of ['error', 'warn', 'info', 'debug'] as const) {
    const original = originals[level];
    Logger.prototype[level] = function (
      this: unknown,
      message: string,
      context?: Record<string, unknown>,
      ...rest: unknown[]
    ) {
      if (typeof message === 'string' && message.startsWith('[ExtractionCursors]')) {
        let line = `${level} ${message}`;
        for (const [k, v] of Object.entries(context ?? {})) {
          if (k === 'module' || v === undefined) continue;
          line += ` ${k}=${v instanceof Error ? v.message : String(v)}`;
        }
        cursorLogs.push(line);
      }
      if (HISTORY.has(message)) {
        let line = `${level} ${message}`;
        for (const [k, v] of Object.entries(context ?? {})) {
          if (k === 'module' || v === undefined) continue;
          line += ` ${k}=${v instanceof Error ? v.message : String(v)}`;
        }
        historyLogs.push(line);
      }
      if (FIND_BY_ID.has(message)) {
        let line = `${level} ${message}`;
        for (const [k, v] of Object.entries(context ?? {})) {
          if (k === 'module' || v === undefined) continue;
          line += ` ${k}=${v instanceof Error ? v.message : String(v)}`;
        }
        findByIdLogs.push(line);
      }
      return (original as (...a: unknown[]) => void).call(this, message, context, ...rest);
    } as never;
  }
  try {
    return { out: await body(), cursorLogs, historyLogs, findByIdLogs };
  } finally {
    Object.assign(Logger.prototype, originals);
  }
}

async function runCase(
  spec: Spec,
  c: CaseSpec,
  scratch: string,
  fixtures: { main: string; mount: string; llm: string },
): Promise<Record<string, unknown>> {
  jest.resetModules();
  applyMocks(spec.userId);

  const work = mkdtempSync(join(scratch, 'sd-'));
  const mainWork = join(work, 'main.db');
  const mountWork = join(work, 'mount.db');
  const llmWork = join(work, 'llm.db');
  copyFileSync(fixtures.main, mainWork);
  copyFileSync(fixtures.mount, mountWork);
  copyFileSync(fixtures.llm, llmWork);
  // [P4.D272 R-B] tier-MIGRATED through v4's REAL `add-memory-tiers-v1` (see
  // `system-import-execute.test.ts`'s `runCase`); a no-op on the widened pair.
  await applyMemoryTiersMigration(mainWork, { pepperBase64: spec.testPepperBase64, ledger: false });
  process.env.SQLITE_PATH = mainWork;
  process.env.SQLITE_MOUNT_INDEX_PATH = mountWork;
  process.env.SQLITE_LLM_LOGS_PATH = llmWork;

  const { initializeDatabase, closeDatabase } = await import('@/lib/database/manager');
  const { closeMountIndexSQLiteClient } = await import(
    '@/lib/database/backends/sqlite/mount-index-client'
  );
  await initializeDatabase();
  {
    const { getRawDatabase } = await import('@/lib/database/backends/sqlite/client');
    plantP4d171Values(getRawDatabase()!);
    plantP4d182Values(getRawDatabase()!);
    plantMemoryTiersValues(getRawDatabase()!);
    // [P4.D317] The outfit-history table on every copy (the round's shared
    // plant — v4's REAL DDL, never the seed migration): v4's `streamChats`
    // reads `chat_wardrobe_history` per chat at the `e8e17b2d9` pin through a
    // raw `rawQuery`, which THROWS on an absent table (a per-chat WARN + the
    // backend's `Raw query failed` ERROR). The Rust side runs
    // `ensure_chat_wardrobe_history_on` on its copy.
    if (!c.noHistoryTable) await plantChatWardrobeHistoryTable(getRawDatabase()!);
    // [P4.D327] The two Inventory columns on EVERY copy (the history table's
    // opt-out does not apply) — the round's shared plant.
    await plantInventoryColumns(getRawDatabase()!);
  }
  if (c.outfitHistory) await plantOutfitHistory();
  if (c.wardrobe) await plantWardrobe();
  if (c.cursorThrow) {
    const { getRawDatabase } = await import('@/lib/database/backends/sqlite/client');
    plantThrowingCursors(getRawDatabase()!);
  }
  if (c.inventoryCells) {
    const { getRawDatabase } = await import('@/lib/database/backends/sqlite/client');
    for (const sql of INVENTORY_CELLS_SQL) {
      if (getRawDatabase()!.prepare(sql).run().changes !== 1) throw new Error(`inventory cell plant: ${sql}`);
    }
  }
  if (c.scenarioTextCell) {
    const { getRawDatabase } = await import('@/lib/database/backends/sqlite/client');
    getRawDatabase()!.exec(SCENARIO_TEXT_CELL_SQL);
  }

  try {
    const out = await c.run();
    return { name: c.name, ...out };
  } finally {
    await closeDatabase();
    closeMountIndexSQLiteClient();
    rmSync(work, { recursive: true, force: true });
  }
}

async function main(): Promise<void> {
  const here = dirname(fileURLToPath(import.meta.url));
  const spec = JSON.parse(
    fs.readFileSync(join(here, '..', 'fixtures', 'system-data.json'), 'utf8'),
  ) as Spec;

  const fixtures = {
    main: process.env.QT_FIXTURE_SD_MAIN ?? '',
    mount: process.env.QT_FIXTURE_SD_MOUNT ?? '',
    llm: process.env.QT_FIXTURE_SD_LLM ?? '',
  };
  for (const [k, v] of Object.entries(fixtures)) {
    if (!v || !existsSync(v)) throw new Error(`fixture ${k} missing: ${v}`);
  }
  const outPath = process.env.QT_ORACLE_OUT;
  if (!outPath) throw new Error('QT_ORACLE_OUT must point at the NDJSON file to write');

  const scratch = mkdtempSync(join(tmpdir(), 'qt-sysexport-oracle-'));
  scratchDirs.push(scratch);
  mkdirSync(join(scratch, 'data'), { recursive: true });
  process.env.ENCRYPTION_MASTER_PEPPER = spec.testPepperBase64;
  process.env.QUILLTAP_DATA_DIR = scratch;
  delete process.env.SQLITE_WAL_MODE;
  process.env.LOG_LEVEL = 'error';

  const appVersion = (
    JSON.parse(fs.readFileSync(join(process.cwd(), 'package.json'), 'utf8')) as {
      version: string;
    }
  ).version;

  const outLines: string[] = [
    JSON.stringify({ name: '_meta', kind: 'meta', appVersion, userId: spec.userId }),
  ];
  for (const c of buildCases(spec)) {
    const payload = await runCase(spec, c, scratch, fixtures);
    outLines.push(JSON.stringify(payload));
  }
  fs.writeFileSync(outPath, outLines.join('\n') + '\n');
  process.stderr.write(
    `system-export oracle wrote ${outPath} (${outLines.length - 1} cases, appVersion=${appVersion})\n`,
  );
}

test('system-export oracle', async () => {
  await main();
});

// Remove the OS-temp scratch dir(s) once the oracle has written its NDJSON.
const scratchDirs: string[] = [];
afterAll(() => {
  for (const d of scratchDirs) rmSync(d, { recursive: true, force: true });
});
