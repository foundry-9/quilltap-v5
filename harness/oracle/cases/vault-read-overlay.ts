/**
 * Read-differential oracle — the character vault read overlay.
 *
 * Opens the pre-seeded mount-index fixture and drives v4's REAL
 * `applyDocumentStoreOverlay` (lib/database/repositories/vault-overlay/read-overlay)
 * over the spec's input characters, emitting the resulting (hydrated / dropped)
 * character list. The Rust port (db::vault_read_overlay::apply_document_store_overlay)
 * reads the same fixture and must produce the same list — exactly, except the
 * physicalDescription mint branch whose createdAt/updatedAt are placeholdered.
 *
 * Run (Node 24, from the v4 checkout):
 *   N=~/.nvm/versions/node/v24.13.1/bin
 *   cd ~/source/quilltap-server
 *   QT_FIXTURE_VAULT_READ_OVERLAY=/tmp/qt-vault-read-overlay-fixture.db \
 *     $N/npx tsx ~/source/quilltap-v5/harness/oracle/cases/vault-read-overlay.ts \
 *     > /tmp/oracle-vault-read-overlay.ndjson
 */

import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import { readFileSync, existsSync, mkdtempSync, mkdirSync, copyFileSync } from 'node:fs';
import { tmpdir } from 'node:os';

interface Spec {
  testPepperBase64: string;
  characters: Array<Record<string, unknown>>;
}

async function main(): Promise<void> {
  const here = dirname(fileURLToPath(import.meta.url));
  const specPath = join(here, '..', 'fixtures', 'vault-read-overlay-tier2.json');
  const spec = JSON.parse(readFileSync(specPath, 'utf8')) as Spec;

  const fixture = process.env.QT_FIXTURE_VAULT_READ_OVERLAY;
  if (!fixture || !existsSync(fixture)) {
    throw new Error('QT_FIXTURE_VAULT_READ_OVERLAY must point at the seeded fixture .db');
  }

  const scratch = mkdtempSync(join(tmpdir(), 'qt-vault-read-overlay-oracle-'));
  mkdirSync(join(scratch, 'data'), { recursive: true });
  const work = join(scratch, 'vault-read-overlay-mount-index-work.db');
  copyFileSync(fixture, work);

  process.env.ENCRYPTION_MASTER_PEPPER = spec.testPepperBase64;
  process.env.SQLITE_PATH = join(scratch, 'data', 'main.db');
  process.env.SQLITE_MOUNT_INDEX_PATH = work;
  process.env.QUILLTAP_DATA_DIR = scratch;
  delete process.env.SQLITE_WAL_MODE;
  process.env.LOG_LEVEL = 'error';

  const { initializeDatabase, closeDatabase } = await import('@/lib/database/manager');
  const { applyDocumentStoreOverlay } = await import(
    '@/lib/database/repositories/vault-overlay/read-overlay'
  );

  await initializeDatabase();

  const result = await applyDocumentStoreOverlay(spec.characters as never);

  // ── P4.142 — the RENAME plant: `doc_mount_file_links.relativePath` renamed on
  // this per-run work copy (the shared seed fixture stays pristine), then the
  // same two overlays again. v4's two batch reads are fallback `withRawDb([])`s
  // (`doc-mount-documents.repository.ts:142-220`), so the batch overlay DROPS
  // every vaulted character (`properties.json missing`) and the single overlay
  // THROWS `CharacterVaultUnavailableError` — v4's own `read-overlay.ts:66-70`
  // comment ("does NOT swallow read failures") is false against its code. The
  // repositories are already ensured by the healthy run above, so the renamed
  // column fails the query itself (a RENAME fails either way; P4.131). Every
  // ERROR/WARN is recorded off the `Logger` prototype (the `mail-tools.test.ts`
  // plant recipe): `{level, message, fields}` with the context keys in v4's
  // order, `module` and `error` omitted (the error tail is compared by neither
  // side — P4.131 finding 3).
  const { getRawMountIndexDatabase } = await import(
    '@/lib/database/backends/sqlite/mount-index-client'
  );
  const { applyDocumentStoreOverlayOne } = await import(
    '@/lib/database/repositories/vault-overlay/read-overlay'
  );
  const { Logger } = await import('@/lib/logger');
  let current: Array<Record<string, unknown>> | null = null;
  for (const level of ['error', 'warn'] as const) {
    const original = Logger.prototype[level];
    Logger.prototype[level] = function (
      this: unknown,
      message: string,
      context?: Record<string, unknown>,
      ...rest: unknown[]
    ) {
      if (current) {
        const fields: Array<[string, string]> = [];
        for (const [k, v] of Object.entries(context ?? {})) {
          if (k === 'module' || k === 'error') continue;
          if (typeof v === 'string' || typeof v === 'number' || typeof v === 'boolean') {
            fields.push([k, String(v)]);
          }
        }
        current.push({ level, message, fields });
      }
      return (original as (...a: unknown[]) => void).call(this, message, context, ...rest);
    } as never;
  }
  const mountDb = getRawMountIndexDatabase();
  if (!mountDb) throw new Error('raw mount-index handle unavailable');
  mountDb.exec('ALTER TABLE doc_mount_file_links RENAME COLUMN relativePath TO relativePath_x');

  current = [];
  const plantCharacters = await applyDocumentStoreOverlay(spec.characters as never);
  const plantLogs = current;
  current = [];
  const ada = spec.characters.find((c) => c.name === 'Ada');
  let one: Record<string, unknown>;
  try {
    const got = await applyDocumentStoreOverlayOne(ada as never);
    one = { threw: null, returned: got };
  } catch (err) {
    one = { threw: (err as Error).name, message: (err as Error).message };
  }
  one.logs = current;
  current = null;

  await closeDatabase();

  process.stdout.write(
    JSON.stringify({
      characters: result,
      plant: { characters: plantCharacters, logs: plantLogs, one },
    }) + '\n',
  );
  process.exit(0);
}

main().catch((err) => {
  process.stderr.write(`oracle failed: ${err?.stack ?? err}\n`);
  process.exit(1);
});
