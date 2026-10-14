/**
 * Tier-1 oracle — v4's two memory-programme instance-settings schemas (P4.D265,
 * v4 `d58548051`): `MemoryConsolidationSettingsSchema` and
 * `MemoryExtractionModeSettingsSchema` (`lib/schemas/settings.types.ts`),
 * through their REAL `safeParse`, plus the REAL `setMemoryConsolidationSettings`
 * / `setMemoryExtractionModeSettings` merge-then-write over a scratch
 * `instance_settings` row (`lib/instance-settings/index.ts`).
 *
 * The parse rows walk every bound on both sides (`min` / `max` inclusive, the
 * `.int()` gate, `.positive()`'s EXCLUSIVE zero, `.nullable()`), the enum, a
 * wrong type per field, the non-object roots, an unknown key (stripped, never
 * an issue), and multi-issue values — so the order and continue/abort rules of
 * Zod 4's issue list are recorded, not reasoned. Each row is `{schema, input,
 * success, data | issues}`; `issues` is `ZodError.issues` (the v5 side compares
 * it byte-for-byte through `serde_json::to_string`).
 *
 * The write rows plant a stored value (or none), call the REAL setter with a
 * patch, and record the stored TEXT afterwards plus the setter's answer (or the
 * thrown `ZodError.message` when the merged value fails — reachable only by a
 * direct call; v4's route pre-parses the patch).
 *
 * P4.D288 (C1-5, the `83334b9a5` pin) grows it: `tfidfClusterThreshold` on
 * the consolidation rows; `MemoryMaintenanceSettingsSchema` (bounds 100 /
 * 100_000 both ways, `.int()`, string caps refused, the override record incl.
 * integer-like keys whose V8 own-key order the output carries — O-11 —, a
 * stored value with `phases` absent / `null` / partial); the typed
 * `MemoryExtractionLimitsSchema` (`maxPerHour ≤ 10000`); the bug-244 PATCH
 * schemas — v4's REAL `patchSchemaOf` over the consolidation, extraction-mode,
 * extraction-limits and toggles schemas plus the route's
 * `maintenanceConfigSchema`, rebuilt here from the SAME exported pieces (it is
 * route-local at the pin) — incl. every `patch-schema.test.ts` row; and the
 * REAL `setMemoryMaintenanceSettings` writes.
 *
 * Output: NDJSON — one `{kind: "parse", …}` line per parse row, then one
 * `{kind: "write", …}` line per write row.
 *
 * Run (Node 24, from the v4 checkout or the round's pin):
 *   N=~/.nvm/versions/node/v24.13.1/bin ; V5W=${V5W:-$HOME/source/quilltap-v5}
 *   rm -f /tmp/oracle-memory-settings-schemas.ndjson
 *   cd ~/source/quilltap-server
 *   $N/npx tsx $V5W/harness/oracle/cases/memory-settings-schemas.ts \
 *     > /tmp/oracle-memory-settings-schemas.ndjson
 */

import { mkdirSync, mkdtempSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { pathToFileURL } from 'node:url';

const TEST_PEPPER = 'dGVzdHBlcHBlcnRlc3RwZXBwZXJ0ZXN0cGVwcGVyMDE=';

const CONSOLIDATION_INPUTS: unknown[] = [
  {},
  {
    enabled: true,
    connectionProfileId: 'profile-1',
    clusterThreshold: 0.5,
    minClusterSize: 2,
    maxClusterSize: 10,
    matureAfterDays: 0,
    maxClustersPerRun: 1,
    watermark: 1,
    coldRetentionDays: 30,
  },
  { enabled: 'yes' },
  { enabled: null },
  { connectionProfileId: null },
  { connectionProfileId: 5 },
  { clusterThreshold: -0.1 },
  { clusterThreshold: 1.1 },
  { clusterThreshold: 0 },
  { clusterThreshold: 1 },
  { clusterThreshold: 'x' },
  { minClusterSize: 1 },
  { minClusterSize: 2 },
  { minClusterSize: 1.5 },
  { minClusterSize: '3' },
  { minClusterSize: 9007199254740993 },
  { maxClusterSize: 1 },
  { maxClusterSize: 2.5 },
  { matureAfterDays: -1 },
  { matureAfterDays: 0.5 },
  { maxClustersPerRun: 0 },
  { maxClustersPerRun: -1 },
  { maxClustersPerRun: 1.5 },
  { maxClustersPerRun: 1 },
  { watermark: 0 },
  { watermark: 2.5 },
  { watermark: null },
  { coldRetentionDays: null },
  { coldRetentionDays: 0 },
  { coldRetentionDays: 5 },
  { coldRetentionDays: 1.5 },
  { coldRetentionDays: -2 },
  { coldRetentionDays: '7' },
  { foo: 1, enabled: true },
  { minClusterSize: 1.5, watermark: 0, clusterThreshold: 2, enabled: 'no' },
  { minClusterSize: 0.5, maxClusterSize: -3.5, coldRetentionDays: -0.5 },
  null,
  [],
  'str',
  5,
];

const MAINTENANCE_INPUTS: unknown[] = [
  {},
  {
    enabled: true,
    perCharacterCap: 500,
    perCharacterCapOverrides: { 'a0eebc99-9c0b-4ef8-bb6d-6bb9bd380a11': 150 },
    phases: { foldNearDuplicates: false, demote: true, retire: false },
  },
  { enabled: 'yes' },
  { perCharacterCap: 99 },
  { perCharacterCap: 100 },
  { perCharacterCap: 100000 },
  { perCharacterCap: 100001 },
  { perCharacterCap: 100.5 },
  { perCharacterCap: '2000' },
  { perCharacterCap: null },
  { perCharacterCap: 9007199254740993 },
  { perCharacterCapOverrides: { a: 150 } },
  { perCharacterCapOverrides: { a: '200' } },
  { perCharacterCapOverrides: { a: 50, b: 200000, c: 150.5 } },
  { perCharacterCapOverrides: [] },
  { perCharacterCapOverrides: [150] },
  { perCharacterCapOverrides: null },
  { perCharacterCapOverrides: 'x' },
  { perCharacterCapOverrides: { b: 300, '10': 200, '2': 250, '1': 400, '01': 500 } },
  { phases: null },
  { phases: {} },
  { phases: { demote: false } },
  { phases: { demote: 'no', retire: null } },
  { phases: [] },
  { phases: 5 },
  { phases: { extra: 1 } },
  { autoMergeSimilarThreshold: 0.9, mergeSimilar: true, enabled: true },
  { enabled: 1, perCharacterCap: 50, perCharacterCapOverrides: { x: 1 }, phases: { retire: 'y' } },
  null,
  [],
  'str',
];

const LIMITS_INPUTS: unknown[] = [
  {},
  { enabled: true, maxPerHour: 10000, softStartFraction: 0, softFloor: 1 },
  { maxPerHour: 10001 },
  { maxPerHour: 0 },
  { maxPerHour: 1.5 },
  { maxPerHour: '20' },
  { maxPerHour: 9007199254740993 },
  { softStartFraction: 1.5, softFloor: -0.1 },
  { enabled: null },
  null,
];

/** The PATCH rows: `{schema, input}` through v4's REAL `patchSchemaOf`. */
const PATCH_INPUTS: Array<[string, unknown]> = [
  // patch-schema.test.ts, row for row.
  ['patch:memoryConsolidation', { watermark: 200 }],
  ['patch:memoryExtractionMode', { otherPass: 'turn' }],
  ['patch:memoryMaintenancePhaseToggles', { demote: false }],
  ['patch:memoryConsolidation', { clusterThreshold: 2 }],
  ['patch:memoryConsolidation', { coldRetentionDays: null }],
  ['patch:memoryConsolidation', { coldRetentionDays: 0 }],
  ['patch:memoryExtractionMode', { otherPass: 'never' }],
  // Every consolidation field present, in REVERSED order (the output is
  // schema-ordered), plus a stray key.
  [
    'patch:memoryConsolidation',
    {
      stray: 1,
      coldRetentionDays: 5,
      watermark: 9,
      maxClustersPerRun: 2,
      matureAfterDays: 1.5,
      maxClusterSize: 4,
      minClusterSize: 2,
      tfidfClusterThreshold: 0.25,
      clusterThreshold: 0.5,
      connectionProfileId: null,
      enabled: true,
    },
  ],
  ['patch:memoryConsolidation', {}],
  ['patch:memoryConsolidation', { connectionProfileId: 5, minClusterSize: 1.5, tfidfClusterThreshold: 2 }],
  ['patch:memoryConsolidation', null],
  ['patch:memoryExtractionMode', { foldCandidatesPerSubject: 0, perTurnOtherFloor: 0.25 }],
  ['patch:memoryExtractionMode', []],
  ['patch:memoryExtractionLimits', { maxPerHour: 10000 }],
  ['patch:memoryExtractionLimits', { maxPerHour: 10001, enabled: 'x' }],
  ['patch:memoryExtractionLimits', { softFloor: 0.5, softStartFraction: 0.25, enabled: false }],
  ['patch:memoryMaintenancePhaseToggles', {}],
  ['patch:memoryMaintenancePhaseToggles', { retire: 'no' }],
  ['patch:memoryMaintenanceConfig', { phases: { demote: false } }],
  ['patch:memoryMaintenanceConfig', { phases: {} }],
  ['patch:memoryMaintenanceConfig', { phases: null }],
  ['patch:memoryMaintenanceConfig', { enabled: true, perCharacterCap: 99 }],
  ['patch:memoryMaintenanceConfig', { perCharacterCapOverrides: { b: 150, '3': 200 }, perCharacterCap: 300 }],
  ['patch:memoryMaintenanceConfig', { autoMergeSimilarThreshold: 0.9, mergeSimilar: true }],
  ['patch:memoryMaintenanceConfig', { perCharacterCapOverrides: { a: 1 }, phases: { demote: 1 }, enabled: 'y' }],
  ['patch:memoryMaintenanceConfig', 'str'],
];

const MODE_INPUTS: unknown[] = [
  {},
  { otherPass: 'turn', perTurnOtherFloor: 0.5, foldCandidatesPerSubject: 1 },
  { otherPass: 'fold' },
  { otherPass: 'bogus' },
  { otherPass: 5 },
  { otherPass: null },
  { perTurnOtherFloor: -0.5 },
  { perTurnOtherFloor: 1.5 },
  { perTurnOtherFloor: 0 },
  { perTurnOtherFloor: 1 },
  { perTurnOtherFloor: 'x' },
  { foldCandidatesPerSubject: 0 },
  { foldCandidatesPerSubject: 1.5 },
  { foldCandidatesPerSubject: 2 },
  { foldCandidatesPerSubject: -1 },
  { otherPass: 'x', perTurnOtherFloor: 2, foldCandidatesPerSubject: 0 },
  null,
  [],
  true,
];

async function main(): Promise<void> {
  // v4's logger writes to STDOUT and reads LOG_LEVEL when first imported — set
  // it before ANY v4 import so the setters' WARN never lands in the NDJSON.
  process.env.LOG_LEVEL = 'error';
  const {
    MemoryConsolidationSettingsSchema,
    MemoryExtractionModeSettingsSchema,
    MemoryMaintenanceSettingsSchema,
    MemoryMaintenancePhaseTogglesSchema,
    MemoryMaintenanceCapSchema,
    MemoryMaintenanceCapOverridesSchema,
    MemoryExtractionLimitsSchema,
  } = await import('@/lib/schemas/settings.types');
  const { patchSchemaOf } = await import('@/lib/schemas/patch-schema');
  // v4's OWN zod (the ESM entry its schemas load — one instance), by path: a
  // bare `'zod'` cannot resolve from the v5 tree.
  const { z } = (await import(
    pathToFileURL(join(process.cwd(), 'node_modules/zod/index.js')).href
  )) as typeof import('zod');
  const emit = (row: unknown) => process.stdout.write(JSON.stringify(row) + '\n');
  const parseRow = (schema: string, z: { safeParse: (v: unknown) => any }, input: unknown) => {
    const r = z.safeParse(input);
    emit(
      r.success
        ? { kind: 'parse', schema, input, success: true, data: r.data }
        : { kind: 'parse', schema, input, success: false, issues: r.error.issues },
    );
  };
  for (const input of CONSOLIDATION_INPUTS) {
    parseRow('memoryConsolidation', MemoryConsolidationSettingsSchema, input);
  }
  for (const input of MODE_INPUTS) parseRow('memoryExtractionMode', MemoryExtractionModeSettingsSchema, input);
  for (const input of MAINTENANCE_INPUTS) {
    parseRow('memoryMaintenance', MemoryMaintenanceSettingsSchema, input);
  }
  for (const input of LIMITS_INPUTS) parseRow('memoryExtractionLimits', MemoryExtractionLimitsSchema, input);
  // `app/api/v1/memories/route.ts:163-168` (route-local at the pin), from the
  // same exported pieces.
  const maintenanceConfigSchema = z.object({
    enabled: z.boolean().optional(),
    phases: patchSchemaOf(MemoryMaintenancePhaseTogglesSchema).optional(),
    perCharacterCap: MemoryMaintenanceCapSchema.optional(),
    perCharacterCapOverrides: MemoryMaintenanceCapOverridesSchema.optional(),
  });
  const patchSchemas: Record<string, { safeParse: (v: unknown) => any }> = {
    'patch:memoryConsolidation': patchSchemaOf(MemoryConsolidationSettingsSchema),
    'patch:memoryExtractionMode': patchSchemaOf(MemoryExtractionModeSettingsSchema),
    'patch:memoryExtractionLimits': patchSchemaOf(MemoryExtractionLimitsSchema),
    'patch:memoryMaintenancePhaseToggles': patchSchemaOf(MemoryMaintenancePhaseTogglesSchema),
    'patch:memoryMaintenanceConfig': maintenanceConfigSchema,
  };
  for (const [schema, input] of PATCH_INPUTS) parseRow(schema, patchSchemas[schema], input);

  // The REAL setters over a scratch instance_settings table.
  const scratch = mkdtempSync(join(tmpdir(), 'qt-memory-settings-'));
  process.on('exit', () => rmSync(scratch, { recursive: true, force: true }));
  mkdirSync(join(scratch, 'data'), { recursive: true });
  process.env.ENCRYPTION_MASTER_PEPPER = TEST_PEPPER;
  process.env.SQLITE_PATH = join(scratch, 'settings.db');
  process.env.QUILLTAP_DATA_DIR = scratch;
  delete process.env.SQLITE_WAL_MODE;
  const { initializeDatabase, closeDatabase, rawQuery } = await import('@/lib/database/manager');
  const settings = await import('@/lib/instance-settings');
  await initializeDatabase();
  await rawQuery(
    'CREATE TABLE IF NOT EXISTS "instance_settings" ("key" TEXT PRIMARY KEY, "value" TEXT NOT NULL)',
  );

  const writes: Array<{ key: string; stored: string | null; patch: Record<string, unknown> }> = [
    { key: 'memoryConsolidation', stored: null, patch: { enabled: true } },
    {
      key: 'memoryConsolidation',
      stored: JSON.stringify({ watermark: 300, clusterThreshold: 0.8, coldRetentionDays: 14 }),
      patch: { minClusterSize: 4 },
    },
    {
      key: 'memoryConsolidation',
      stored: JSON.stringify({ enabled: true, coldRetentionDays: 14 }),
      patch: { coldRetentionDays: null, connectionProfileId: 'p' },
    },
    { key: 'memoryConsolidation', stored: '{not json', patch: { watermark: 5 } },
    { key: 'memoryConsolidation', stored: null, patch: { watermark: 0 } },
    { key: 'memoryExtractionMode', stored: null, patch: { otherPass: 'fold' } },
    {
      key: 'memoryExtractionMode',
      stored: JSON.stringify({ perTurnOtherFloor: 0.9 }),
      patch: { foldCandidatesPerSubject: 5 },
    },
    { key: 'memoryExtractionMode', stored: null, patch: { perTurnOtherFloor: 2 } },
    // P4.D288: the maintenance setter — a stored value without `phases` reads
    // every phase on; a sent map REPLACES the stored one; a refused merge.
    { key: 'memoryMaintenance', stored: null, patch: { enabled: true } },
    {
      key: 'memoryMaintenance',
      stored: JSON.stringify({ enabled: true, perCharacterCap: 400, perCharacterCapOverrides: { a: 150 } }),
      patch: { perCharacterCap: 500 },
    },
    {
      key: 'memoryMaintenance',
      stored: JSON.stringify({ perCharacterCapOverrides: { a: 150, b: 300 } }),
      patch: { perCharacterCapOverrides: { c: 900 } },
    },
    {
      key: 'memoryMaintenance',
      stored: JSON.stringify({ phases: { demote: false } }),
      patch: { phases: { foldNearDuplicates: false, demote: true, retire: true } },
    },
    { key: 'memoryMaintenance', stored: '{not json', patch: { enabled: false } },
    { key: 'memoryMaintenance', stored: null, patch: { perCharacterCap: 50 } },
  ];
  for (const w of writes) {
    await rawQuery('DELETE FROM "instance_settings" WHERE "key" = ?', [w.key]);
    if (w.stored !== null) {
      await rawQuery('INSERT INTO "instance_settings" ("key", "value") VALUES (?, ?)', [w.key, w.stored]);
    }
    let answer: unknown = null;
    let error: string | null = null;
    try {
      answer =
        w.key === 'memoryConsolidation'
          ? await settings.setMemoryConsolidationSettings(w.patch as never)
          : w.key === 'memoryMaintenance'
            ? await settings.setMemoryMaintenanceSettings(w.patch as never)
            : await settings.setMemoryExtractionModeSettings(w.patch as never);
    } catch (e) {
      error = e instanceof Error ? e.message : String(e);
    }
    const after = (await rawQuery('SELECT "value" FROM "instance_settings" WHERE "key" = ?', [
      w.key,
    ])) as Array<{ value: string }>;
    emit({ kind: 'write', ...w, answer, error, after: after[0]?.value ?? null });
  }
  await closeDatabase();
  process.exit(0);
}

main().catch((err) => {
  process.stderr.write(`memory-settings-schemas oracle failed: ${err?.stack ?? err}\n`);
  process.exit(1);
});
