/**
 * Tier-2 oracle case (P4.D279 keystone, C1 §6) — v4's REAL `applyDisplacement`
 * (`lib/wardrobe/outfit-displacement.ts`, `9ecebf986`): load → the one mode →
 * primitive table → the wear ledger's chokepoint → the `[applyDisplacement]`
 * log lines.
 *
 * Runs the spec's scripted op list (`fixtures/outfit-displacement-tier2.json`)
 * over ONE copy of the seed fixture `build-outfit-displacement-fixture.ts`
 * builds, each op through `applyDisplacement(getRepositories(), chat,
 * characterId, { mode, item?, slot?, itemId?, itemsById? }, source)`: a leaf
 * worn / re-worn / layered, `add_to_slot` with a composite that covers the slot
 * and one that does not, `replace`, the replacing composite through `wear`, a
 * composite with no lookup (stored whole), the missing-item / missing-slot
 * fallbacks (a committed no-op copy), `remove_from_slot` with and without an
 * id, `clear_slot`, a re-wear after a take-off, an unresolvable composite, a
 * cyclic lookup, a depth-truncated chain, and a wear into a MISSING chat (v4's
 * no-op `update` still credits).
 *
 * After each op it dumps the returned slots, the op chat's `equippedOutfit`,
 * EVERY `wardrobe_wear_stats` row (minus the minted `id` / `createdAt` /
 * `updatedAt`), and the log lines, captured by splicing a transport into the
 * singleton logger (child loggers share its transports).
 *
 * Run (Node 24, from the v4 checkout):
 *   N=~/.nvm/versions/node/v24.13.1/bin ; V5W=${V5W:-$HOME/source/quilltap-v5}
 *   cd ~/source/quilltap-server
 *   QT_FIXTURE_OUT=/tmp/qt-outfit-displacement-main.db \
 *     $N/node --import tsx $V5W/harness/oracle/fixtures/build-outfit-displacement-fixture.ts
 *   QT_FIXTURE_OUTFIT_DISPLACEMENT=/tmp/qt-outfit-displacement-main.db \
 *     $N/node --import tsx $V5W/harness/oracle/cases/outfit-displacement-tier2.ts \
 *     > /tmp/oracle-outfit-displacement.ndjson
 */

import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import { mkdtempSync, rmSync, mkdirSync, readFileSync, copyFileSync, existsSync } from 'node:fs';
import { tmpdir } from 'node:os';

interface SpecItem {
  id: string;
  title: string;
  types: string[];
  componentItemIds: string[];
  replace?: boolean;
}

interface Op {
  name: string;
  chat: string;
  mode: 'wear' | 'replace' | 'add_to_slot' | 'remove_from_slot' | 'clear_slot';
  item?: string;
  slot?: string;
  itemId?: string;
  /** `"all"` = every spec item; a list = those ids; null = no lookup at all. */
  lookup: 'all' | string[] | null;
  source: 'ui' | 'tool';
}

interface Spec {
  testPepperBase64: string;
  characterId: string;
  items: SpecItem[];
  ops: Op[];
}

/** The messages this family compares. */
const CAPTURED = new Set([
  '[applyDisplacement] Malformed composite graph; expansion truncated',
  '[applyDisplacement] Composite resolved to no wearable parts; wearing it whole',
  '[applyDisplacement] Outfit change committed',
  'Committed equipped outfit',
  'Equipped outfit write failed; no wears credited',
]);

async function main(): Promise<void> {
  const here = dirname(fileURLToPath(import.meta.url));
  const spec = JSON.parse(
    readFileSync(join(here, '..', 'fixtures', 'outfit-displacement-tier2.json'), 'utf8'),
  ) as Spec;

  const fixture = process.env.QT_FIXTURE_OUTFIT_DISPLACEMENT;
  if (!fixture || !existsSync(fixture)) {
    throw new Error('QT_FIXTURE_OUTFIT_DISPLACEMENT must point at the seed fixture .db (see header)');
  }

  const scratch = mkdtempSync(join(tmpdir(), 'qt-outfit-displacement-oracle-'));
  process.on('exit', () => rmSync(scratch, { recursive: true, force: true }));
  mkdirSync(join(scratch, 'data'), { recursive: true });
  const work = join(scratch, 'outfit-displacement-main.db');
  copyFileSync(fixture, work);

  process.env.ENCRYPTION_MASTER_PEPPER = spec.testPepperBase64;
  process.env.SQLITE_PATH = work;
  process.env.QUILLTAP_DATA_DIR = scratch;
  delete process.env.SQLITE_WAL_MODE;
  // DEBUG so the committed lines reach the capture; the console transport is
  // spliced out below, so nothing reaches stdout.
  process.env.LOG_LEVEL = 'debug';

  const { logger } = await import('@/lib/logger');
  const captured: Array<{ level: string; message: string; fields: Record<string, unknown> }> = [];
  const internal = logger as unknown as { transports: Array<{ write: (d: unknown) => void }> };
  internal.transports.splice(0, internal.transports.length, {
    write: (d: unknown) => {
      const data = d as {
        level: string;
        message: string;
        context?: Record<string, unknown>;
        error?: { message: string };
      };
      if (!CAPTURED.has(data.message)) return;
      const { service: _s, environment: _e, ...fields } = data.context ?? {};
      if (data.error) fields.error = data.error.message;
      captured.push({ level: data.level, message: data.message, fields });
    },
  });

  const { initializeDatabase, closeDatabase, rawQuery } = await import('@/lib/database/manager');
  const { getRepositories } = await import('@/lib/repositories/factory');
  const { applyDisplacement } = await import('@/lib/wardrobe/outfit-displacement');

  await initializeDatabase();
  const repos = getRepositories();

  const byId = new Map(spec.items.map((i) => [i.id, i]));
  const node = (id: string): SpecItem => {
    const found = byId.get(id);
    if (!found) throw new Error(`unknown spec item: ${id}`);
    return found;
  };
  const lookupOf = (lookup: Op['lookup']): Map<string, SpecItem> | undefined => {
    if (lookup === null) return undefined;
    if (lookup === 'all') return new Map(byId);
    return new Map(lookup.map((id) => [id, node(id)]));
  };

  const lines: string[] = [];
  for (const op of spec.ops) {
    captured.length = 0;
    let result: unknown;
    try {
      const slots = await applyDisplacement(
        repos,
        op.chat,
        spec.characterId,
        {
          mode: op.mode,
          ...(op.item ? { item: node(op.item) } : {}),
          ...(op.slot ? { slot: op.slot as never } : {}),
          ...(op.itemId ? { itemId: op.itemId } : {}),
          ...(op.lookup === null ? {} : { itemsById: lookupOf(op.lookup) }),
        },
        op.source,
      );
      result = { slots };
    } catch (error) {
      result = { error: error instanceof Error ? error.message : String(error) };
    }
    const equippedOutfit = (await repos.chats.getEquippedOutfit(op.chat)) ?? null;
    const ledger = (await rawQuery(
      `SELECT "itemId", "wearerCharacterId", "wearCount", "firstWornAt", "lastWornAt", "lastWornChatId"
         FROM "wardrobe_wear_stats" ORDER BY "itemId", COALESCE("wearerCharacterId", '')`,
    )) as unknown[];
    lines.push(
      JSON.stringify({
        case: 'outfit-displacement',
        name: op.name,
        mode: op.mode,
        result,
        equippedOutfit,
        ledger,
        logs: captured.map((c) => ({ ...c })),
      }),
    );
  }

  await closeDatabase();
  process.stdout.write(lines.join('\n') + '\n');
  process.exit(0);
}

main().catch((err) => {
  process.stderr.write(`outfit-displacement oracle failed: ${err?.stack ?? err}\n`);
  process.exit(1);
});
