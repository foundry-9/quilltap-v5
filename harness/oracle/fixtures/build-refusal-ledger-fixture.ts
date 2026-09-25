/**
 * Fixture builder for the P4.D225 refusal-ledger tier-3 differential (v4
 * `recordModerationRefusal` / `maybeAutoSwitchAfterRefusal`,
 * `lib/services/dangerous-content/refusal-ledger.ts`, NEW at `49059fb14`).
 *
 * Bakes one `chats` row per scenario (pinned ids + timestamps) and one
 * `chat_settings` row per user — each user carries the Concierge settings its
 * scenarios need (AUTO_ROUTE at the default threshold, DETECT_ONLY with the
 * auto-switch off, …), materialized through v4's REAL
 * `DangerousContentSettingsSchema` so the stored object is Zod-shaped at the
 * pin. One user deliberately has NO settings row (v4's `findByUserId` → null →
 * the defaults, mode OFF). Then runs v4's own `add-chat-refusal-ledger-v1`
 * migration module: `initializeDatabase` builds `chats` from the Zod schema,
 * which omits the ledger's columns.
 *
 * `QT_REFUSAL_LEDGER_SPEC` names another spec in this directory with the same
 * shape (default `refusal-ledger.json`) — the cheap-LLM refusal case
 * (`cheap-llm-refusal.json`) reuses the builder and adds `connectionProfiles`,
 * created through v4's REAL connections repository; the image-failover case
 * (`image-failover.json`) adds `imageProfiles` through the image-profiles one.
 *
 * Run (Node 24, from the v4 checkout — or a pinned worktree):
 *   N=~/.nvm/versions/node/v24.13.1/bin ; V5W=<this worktree>
 *   QT_FIXTURE_OUT=/tmp/qt-refusal-ledger.db \
 *     $N/npx tsx $V5W/harness/oracle/fixtures/build-refusal-ledger-fixture.ts
 */

import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import { mkdtempSync, mkdirSync, readFileSync, rmSync, existsSync } from 'node:fs';
import { tmpdir } from 'node:os';

interface Spec {
  testPepperBase64: string;
  seedTimestamp: string;
  characterId: string;
  chatSettings: Array<{ id: string; userId: string; dangerousContentSettings: Record<string, unknown> }>;
  chats: Array<Record<string, unknown> & { id: string; userId: string }>;
  connectionProfiles?: Array<Record<string, unknown> & { id: string; userId: string }>;
  imageProfiles?: Array<Record<string, unknown> & { id: string; userId: string }>;
}

async function main(): Promise<void> {
  const here = dirname(fileURLToPath(import.meta.url));
  const specName = process.env.QT_REFUSAL_LEDGER_SPEC ?? 'refusal-ledger.json';
  const spec = JSON.parse(readFileSync(join(here, specName), 'utf8')) as Spec;

  const out = process.env.QT_FIXTURE_OUT;
  if (!out) throw new Error('QT_FIXTURE_OUT must point at the fixture .db to write');
  for (const suffix of ['', '-journal', '-wal', '-shm']) {
    const p = out + suffix;
    if (existsSync(p)) rmSync(p);
  }

  const scratch = mkdtempSync(join(tmpdir(), 'qt-refusal-ledger-build-'));
  mkdirSync(join(scratch, 'data'), { recursive: true });

  process.env.ENCRYPTION_MASTER_PEPPER = spec.testPepperBase64;
  process.env.SQLITE_PATH = out;
  process.env.QUILLTAP_DATA_DIR = scratch;
  delete process.env.SQLITE_WAL_MODE;
  process.env.LOG_LEVEL = 'error';

  const { initializeDatabase, closeDatabase } = await import('@/lib/database/manager');
  const { getRepositories } = await import('@/lib/repositories/factory');
  const { DangerousContentSettingsSchema } = await import('@/lib/schemas/settings.types');

  await initializeDatabase();
  const repos = getRepositories();
  const ts = spec.seedTimestamp;

  for (const cs of spec.chatSettings) {
    await repos.chatSettings.create(
      {
        userId: cs.userId,
        dangerousContentSettings: DangerousContentSettingsSchema.parse(cs.dangerousContentSettings),
      } as never,
      { id: cs.id, createdAt: ts, updatedAt: ts },
    );
  }

  for (const p of spec.connectionProfiles ?? []) {
    const { id, ...data } = p;
    await repos.connections.create(data as never, { id, createdAt: ts, updatedAt: ts });
  }

  for (const p of spec.imageProfiles ?? []) {
    const { id, ...data } = p;
    await repos.imageProfiles.create(data as never, { id, createdAt: ts, updatedAt: ts });
  }

  let n = 0;
  for (const c of spec.chats) {
    const { id, ...rest } = c;
    n += 1;
    const participantId = `fd0000${String(n).padStart(2, '0')}-0000-4000-8000-000000000001`;
    await repos.chats.create(
      {
        participants: [
          { id: participantId, type: 'CHARACTER', characterId: spec.characterId, createdAt: ts, updatedAt: ts },
        ],
        ...rest,
      } as never,
      { id, createdAt: ts, updatedAt: ts },
    );
  }

  // Materialize `chat_messages` (v4 ensures it lazily on first repo use; the
  // Concierge's auto-flag bubble lands there on both sides).
  await repos.chats.getMessages(spec.chats[0].id);

  await closeDatabase();

  const { runV4Migrations, ADD_CHAT_REFUSAL_LEDGER } = await import('../lib/v4-migrations');
  const migrated = await runV4Migrations({
    dbPath: out,
    pepperBase64: spec.testPepperBase64,
    migrations: [ADD_CHAT_REFUSAL_LEDGER],
    // Skipped (and reported) at a pin before `49059fb14` — where v4 has no
    // ledger either — so a family sharing this builder still regenerates its
    // other halves at the baseline.
    allowMissing: true,
  });
  process.stderr.write(`refusal-ledger fixture migrations: ${migrated.join('; ')}\n`);
  process.stderr.write(`built ${specName} fixture: ${out} (${spec.chats.length} chats)\n`);
  process.exit(0);
}

main().catch((err) => {
  process.stderr.write(`refusal-ledger fixture build failed: ${err?.stack ?? err}\n`);
  process.exit(1);
});
