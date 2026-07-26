/**
 * Differential oracle (tier 2, DB state) — the fictional-clock anchor backfill
 * (P4.d18; v4 migration `anchor-fictional-clock-base-v1`, from `e3a9654f`).
 *
 * Drives v4's REAL `anchorFictionalClockBaseMigration.run()` over a freshly
 * provisioned instance: `initializeDatabase()` builds the true schema, the
 * shared spec (`fixtures/fictional-clock-anchor-spec.json`) plants the chats
 * rows, the migration runs, and the post-migration `chats` rows are dumped.
 *
 * The Rust diff (`crates/quilltap-harness/tests/
 * fictional_clock_anchor_equivalence.rs`) reads the SAME spec, provisions a
 * fresh instance with `provision_fresh_instance` (the D23 re-dump of this very
 * schema), plants the identical rows, runs the ported
 * `db::fictional_clock_anchor_repair`, and field-diffs against this NDJSON.
 *
 * `shouldRun()` is recorded too — v5 folds that gate into the pass itself, and
 * a `false` there would mean the backfill never fires on a real instance.
 *
 * The clock is PINNED. v4 reads it two ways here (`Date.now()` for durationMs,
 * `new Date()` for the anchor of a row whose `createdAt` will not parse), so the
 * whole Date constructor is overridden: a no-arg `new Date()` returns the spec's
 * `nowMs` while every argument form keeps native behavior.
 *
 * Run from the v4 checkout (Node 24 for the native binding ABI; TZ=UTC because
 * an unparseable createdAt lands on the pinned clock and v4 formats it):
 *   N=~/.nvm/versions/node/v24.13.1/bin
 *   cd /tmp/qt-v4-pin-231be14c
 *   TZ=UTC $N/node --import tsx \
 *     ~/source/quilltap-v5/harness/oracle/cases/fictional-clock-anchor.ts \
 *     > /tmp/oracle-fictional-clock-anchor.ndjson
 */

import { mkdtempSync, mkdirSync, readFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join, dirname } from 'node:path';
import { fileURLToPath } from 'node:url';

const TEST_PEPPER = '3q2+796tvu/erb7v3q2+796tvu/erb7v3q2+796tvu8=';

interface ChatSpec {
  id: string;
  label: string;
  createdAt: string | null;
  timestampConfig: string | null;
}
interface Spec {
  nowMs: number;
  chats: ChatSpec[];
}

const HERE = dirname(fileURLToPath(import.meta.url));
const SPEC = JSON.parse(
  readFileSync(join(HERE, '..', 'fixtures', 'fictional-clock-anchor-spec.json'), 'utf8'),
) as Spec;

// Pin the wall clock before anything imports the migration.
const RealDate = Date;
class MockDate extends RealDate {
  constructor(...args: any[]) {
    if (args.length === 0) {
      super(SPEC.nowMs);
    } else {
      // @ts-expect-error variadic forward to the native Date constructor
      super(...args);
    }
  }
  static now(): number {
    return SPEC.nowMs;
  }
}
;(globalThis as any).Date = MockDate;

async function main(): Promise<void> {
  const scratch = mkdtempSync(join(tmpdir(), 'qt-fictional-clock-oracle-'));
  mkdirSync(join(scratch, 'data'), { recursive: true });

  process.env.ENCRYPTION_MASTER_PEPPER = TEST_PEPPER;
  process.env.SQLITE_PATH = join(scratch, 'quilltap.db');
  process.env.QUILLTAP_DATA_DIR = scratch;
  delete process.env.SQLITE_WAL_MODE;
  process.env.LOG_LEVEL = 'error';

  const { initializeDatabase, ensureCollection, rawQuery, closeDatabase } = await import(
    '@/lib/database/manager'
  );
  const { ChatMetadataBaseSchema } = await import('@/lib/schemas/chat.types');
  const { anchorFictionalClockBaseMigration } = await import(
    '@/migrations/scripts/anchor-fictional-clock-base'
  );

  await initializeDatabase();
  // v4 creates collections lazily; materialize `chats` from its real schema
  // (the same generateDDL output the D23 re-dump feeds v5's fresh_schema.json).
  await ensureCollection('chats', ChatMetadataBaseSchema);

  // Plant the spec rows. Only the NOT NULL columns plus the three the migration
  // reads are set; everything else takes its schema default.
  for (const chat of SPEC.chats) {
    await rawQuery(
      'INSERT INTO "chats" ("id","userId","title","createdAt","updatedAt","timestampConfig") VALUES (?,?,?,?,?,?)',
      [
        chat.id,
        'ffffffff-ffff-ffff-ffff-ffffffffffff',
        chat.label,
        chat.createdAt,
        '2024-03-04T05:06:07.000Z',
        chat.timestampConfig,
      ],
    );
  }

  const shouldRun = await anchorFictionalClockBaseMigration.shouldRun();
  const result = await anchorFictionalClockBaseMigration.run();
  // A second pass proves idempotence: v4's own needsAnchor guard is the marker.
  const shouldRunAfter = await anchorFictionalClockBaseMigration.shouldRun();
  const secondResult = await anchorFictionalClockBaseMigration.run();

  const rows = (await rawQuery(
    'SELECT "id","createdAt","timestampConfig" FROM "chats" ORDER BY "id"',
  )) as Array<Record<string, unknown>>;

  await closeDatabase();

  process.stdout.write(
    JSON.stringify({
      case: 'fictional-clock-anchor',
      shouldRun,
      shouldRunAfter,
      success: result.success,
      itemsAffected: result.itemsAffected,
      message: result.message,
      secondItemsAffected: secondResult.itemsAffected,
      secondMessage: secondResult.message,
      rows,
    }) + '\n',
  );
  process.exit(0);
}

main().catch((err) => {
  process.stderr.write(`fictional-clock-anchor oracle failed: ${err?.stack ?? err}\n`);
  process.exit(1);
});
