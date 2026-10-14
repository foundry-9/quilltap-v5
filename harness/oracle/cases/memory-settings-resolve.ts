/**
 * Tier-2 oracle — v4's memory settings façade (P4.D288 unit 2, C1-5;
 * `lib/memory/settings.ts:99-130`, `e0ac7493a`, at the `83334b9a5` pin):
 * `resolveMemorySettings(userId?)` through its REAL code over a scratch
 * `instance_settings` table, per case:
 *
 *   - the resolved knobs v5's `MemorySettings` carries — `extraction.limits`,
 *     `extraction.mode`, the stored recall settings, `recall.minCosine`,
 *     `maintenance` (the settings v4 spreads in, `consolidation`,
 *     `coldRetentionDays`) and `capFor` over a probe list of character ids;
 *   - every line it logged — the DEBUG `[MemorySettings] Resolved memory
 *     settings {userId, maintenanceEnabled, perCharacterCap, capOverrides,
 *     consolidationEnabled, otherPass}` (its bytes the comparand) and each
 *     getter's WARN (`[InstanceSettings] <key> failed to parse — using
 *     defaults`, compared by message + `error` PRESENCE — V8 / Zod wording is
 *     not serde's).
 *
 * Cases: nothing stored + no user; every key stored (a maintenance value with
 * NO `phases`, an override hit, a retention window, `fold` mode, limits,
 * recall) + a user; a malformed maintenance value + the single user.
 *
 * Run (Node 24, from the pinned v4 worktree):
 *   N=~/.nvm/versions/node/v24.13.1/bin ; V5W=${V5W:-$HOME/source/quilltap-v5}
 *   rm -f /tmp/oracle-memory-settings-resolve.ndjson
 *   cd ~/source/quilltap-server
 *   $N/npx tsx $V5W/harness/oracle/cases/memory-settings-resolve.ts \
 *     > /tmp/oracle-memory-settings-resolve.ndjson
 */

import { mkdirSync, mkdtempSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';

const TEST_PEPPER = 'dGVzdHBlcHBlcnRlc3RwZXBwZXJ0ZXN0cGVwcGVyMDE=';

interface Case {
  name: string;
  userId?: string;
  rows: Record<string, string>;
  probes: string[];
}

const CASES: Case[] = [
  { name: 'defaults', rows: {}, probes: ['char-a'] },
  {
    name: 'everything-stored',
    userId: 'u1',
    rows: {
      memoryMaintenance: JSON.stringify({
        enabled: true,
        perCharacterCap: 700,
        perCharacterCapOverrides: { 'char-over': 150, 'char-two': 99999 },
      }),
      memoryConsolidation: JSON.stringify({ enabled: true, coldRetentionDays: 30, tfidfClusterThreshold: 0.4 }),
      memoryExtractionMode: JSON.stringify({ otherPass: 'fold' }),
      memoryExtractionLimits: JSON.stringify({ enabled: true, maxPerHour: 40 }),
      memoryRecall: JSON.stringify({ scopePolicy: 'exclude', expandRelated: true }),
    },
    probes: ['char-over', 'char-two', 'char-plain'],
  },
  {
    name: 'malformed-maintenance',
    userId: 'ffffffff-ffff-ffff-ffff-ffffffffffff',
    rows: { memoryMaintenance: '{bad', memoryExtractionMode: JSON.stringify({ otherPass: 'nope' }) },
    probes: ['char-a'],
  },
];

async function main(): Promise<void> {
  process.env.LOG_LEVEL = 'error';
  const scratch = mkdtempSync(join(tmpdir(), 'qt-memory-settings-resolve-'));
  process.on('exit', () => rmSync(scratch, { recursive: true, force: true }));
  mkdirSync(join(scratch, 'data'), { recursive: true });
  process.env.ENCRYPTION_MASTER_PEPPER = TEST_PEPPER;
  process.env.SQLITE_PATH = join(scratch, 'settings.db');
  process.env.QUILLTAP_DATA_DIR = scratch;
  delete process.env.SQLITE_WAL_MODE;
  const { initializeDatabase, closeDatabase, rawQuery } = await import('@/lib/database/manager');
  const { resolveMemorySettings } = await import('@/lib/memory/settings');
  const { Logger } = await import('@/lib/logger');

  let sink: Array<{ level: string; message: string; fields: Array<[string, unknown]> }> | null = null;
  for (const level of ['warn', 'debug'] as const) {
    const original = Logger.prototype[level];
    Logger.prototype[level] = function (this: unknown, message: string, context?: Record<string, unknown>, ...rest: unknown[]) {
      sink?.push({ level, message, fields: Object.entries(context ?? {}) });
      return (original as (...a: unknown[]) => void).call(this, message, context, ...rest);
    } as never;
  }

  await initializeDatabase();
  await rawQuery('CREATE TABLE IF NOT EXISTS "instance_settings" ("key" TEXT PRIMARY KEY, "value" TEXT NOT NULL)');
  for (const c of CASES) {
    await rawQuery('DELETE FROM "instance_settings"');
    for (const [key, value] of Object.entries(c.rows)) {
      await rawQuery('INSERT INTO "instance_settings" ("key", "value") VALUES (?, ?)', [key, value]);
    }
    sink = [];
    const s = await resolveMemorySettings(c.userId);
    const logs = sink;
    sink = null;
    const { capFor, consolidation, coldRetentionDays, ...maintenance } = s.maintenance;
    process.stdout.write(
      JSON.stringify({
        name: c.name,
        userId: c.userId ?? null,
        rows: c.rows,
        resolved: {
          extraction: s.extraction,
          recall: {
            scopePolicy: s.recall.scopePolicy,
            expandRelated: s.recall.expandRelated,
            perTurnConversationSummaries: s.recall.perTurnConversationSummaries,
            minCosine: s.recall.minCosine,
          },
          maintenance: {
            settings: maintenance,
            consolidation,
            coldRetentionDays,
            capFor: Object.fromEntries(c.probes.map((id) => [id, capFor(id)])),
          },
        },
        logs,
      }) + '\n',
    );
  }
  await closeDatabase();
  process.exit(0);
}

main().catch((err) => {
  process.stderr.write(`memory-settings-resolve oracle failed: ${err?.stack ?? err}\n`);
  process.exit(1);
});
