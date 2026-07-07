/**
 * Tier-2 oracle case — the `api_keys` repo (W4.7d, the last unported repo).
 *
 * Drives v4's REAL api-key methods (hosted on `ConnectionProfilesRepository`)
 * over a fixed op sequence — create / recordUsage / update / delete + the reads
 * (`getApiKeysByUserId` incl. the malformed-row DROP, `findApiKeyById`,
 * `findApiKeyByIdAndUserId`) — and dumps the resulting `api_keys` table so the
 * Rust port can be diffed against it.
 *
 * NORMALIZATION: minted-values remap. `createApiKey` mints id + timestamps (no
 * CreateOptions), so the harness remaps ids (first-seen in `label` order) and
 * placeholders the minted timestamp columns (`createdAt` / `updatedAt` /
 * `lastUsed`). Read outcomes are pinned in the spec (`expect*`) and checked on
 * BOTH ports (a mismatch aborts) — the dump is the byte differential.
 *
 * SAFETY: every key_value is a SYNTHETIC placeholder — never a real API key.
 *
 * Run from the v4 server checkout under Node 24, AFTER building the fixture:
 *   N=~/.nvm/versions/node/v24.13.1/bin
 *   cd ~/source/quilltap-server
 *   QT_FIXTURE_API_KEYS=/tmp/qt-api-keys-fixture.db \
 *     $N/npx tsx ~/source/quilltap-v5/harness/oracle/cases/api-keys.ts \
 *     > /tmp/oracle-api-keys.ndjson
 */

import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import {
  mkdtempSync,
  mkdirSync,
  readFileSync,
  copyFileSync,
  existsSync,
} from 'node:fs';
import { tmpdir } from 'node:os';
import { canonicalizeRows } from '../lib/tier2.js';

interface Op {
  kind:
    | 'create'
    | 'recordUsage'
    | 'update'
    | 'delete'
    | 'getByUser'
    | 'findById'
    | 'findByIdAndUser';
  idFromOp?: number;
  userId?: string;
  data?: {
    userId?: string;
    label?: string;
    provider?: string;
    keyValue?: string;
    isActive?: boolean;
  };
  expectDeleted?: boolean;
  expectLabels?: string[];
  expectFound?: boolean;
}

interface Spec {
  testPepperBase64: string;
  ops: Op[];
}

async function main(): Promise<void> {
  const here = dirname(fileURLToPath(import.meta.url));
  const specPath = join(here, '..', 'fixtures', 'api-keys-tier2.json');
  const spec = JSON.parse(readFileSync(specPath, 'utf8')) as Spec;

  const fixture = process.env.QT_FIXTURE_API_KEYS;
  if (!fixture || !existsSync(fixture)) {
    throw new Error(
      'QT_FIXTURE_API_KEYS must point at the seed fixture from build-api-keys-fixture.ts'
    );
  }

  const scratch = mkdtempSync(join(tmpdir(), 'qt-api-keys-oracle-'));
  mkdirSync(join(scratch, 'data'), { recursive: true });
  const work = join(scratch, 'api-keys-work.db');
  copyFileSync(fixture, work);

  process.env.ENCRYPTION_MASTER_PEPPER = spec.testPepperBase64;
  process.env.SQLITE_PATH = work;
  process.env.QUILLTAP_DATA_DIR = scratch;
  delete process.env.SQLITE_WAL_MODE;
  process.env.LOG_LEVEL = 'error';

  const { initializeDatabase, closeDatabase, rawQuery } = await import(
    '@/lib/database/manager'
  );
  const { ConnectionProfilesRepository } = await import(
    '@/lib/database/repositories/connection-profiles.repository'
  );

  await initializeDatabase();
  const repo = new ConnectionProfilesRepository();

  // Minted key id per CREATE op index (idFromOp resolves against this).
  const mintedByOp: (string | undefined)[] = [];
  const resolveId = (op: Op): string => {
    const n = op.idFromOp;
    if (n === undefined || mintedByOp[n] === undefined) {
      throw new Error(`op references idFromOp ${n}, which is not a create result`);
    }
    return mintedByOp[n] as string;
  };

  for (let i = 0; i < spec.ops.length; i++) {
    const op = spec.ops[i];
    switch (op.kind) {
      case 'create': {
        const d = op.data as NonNullable<Op['data']>;
        const created = await repo.createApiKey({
          userId: d.userId as string,
          label: d.label as string,
          provider: d.provider as string,
          key_value: d.keyValue as string,
          ...(d.isActive !== undefined ? { isActive: d.isActive } : {}),
        } as never);
        mintedByOp[i] = created.id;
        break;
      }
      case 'recordUsage': {
        const res = await repo.recordApiKeyUsage(resolveId(op));
        if (!res) throw new Error(`recordApiKeyUsage returned null for op ${i}`);
        break;
      }
      case 'update': {
        const d = op.data as NonNullable<Op['data']>;
        const patch: Record<string, unknown> = {};
        if (d.label !== undefined) patch.label = d.label;
        if (d.provider !== undefined) patch.provider = d.provider;
        if (d.keyValue !== undefined) patch.key_value = d.keyValue;
        if (d.isActive !== undefined) patch.isActive = d.isActive;
        const res = await repo.updateApiKey(resolveId(op), patch as never);
        if (!res) throw new Error(`updateApiKey returned null for op ${i}`);
        break;
      }
      case 'delete': {
        const deleted = await repo.deleteApiKey(resolveId(op));
        if (op.expectDeleted !== undefined && deleted !== op.expectDeleted) {
          throw new Error(
            `deleteApiKey returned ${deleted}, expected ${op.expectDeleted} (op ${i})`
          );
        }
        break;
      }
      case 'getByUser': {
        const keys = await repo.getApiKeysByUserId(op.userId as string);
        const labels = keys.map((k) => k.label).sort();
        const expected = [...(op.expectLabels ?? [])].sort();
        if (JSON.stringify(labels) !== JSON.stringify(expected)) {
          throw new Error(
            `getApiKeysByUserId labels ${JSON.stringify(labels)}, expected ${JSON.stringify(expected)} (op ${i})`
          );
        }
        break;
      }
      case 'findById': {
        const key = await repo.findApiKeyById(resolveId(op));
        const found = key !== null;
        if (op.expectFound !== undefined && found !== op.expectFound) {
          throw new Error(
            `findApiKeyById found=${found}, expected ${op.expectFound} (op ${i})`
          );
        }
        break;
      }
      case 'findByIdAndUser': {
        const key = await repo.findApiKeyByIdAndUserId(
          resolveId(op),
          op.userId as string
        );
        const found = key !== null;
        if (op.expectFound !== undefined && found !== op.expectFound) {
          throw new Error(
            `findApiKeyByIdAndUserId found=${found}, expected ${op.expectFound} (op ${i})`
          );
        }
        break;
      }
      default:
        throw new Error(`unknown op kind: ${(op as { kind: string }).kind}`);
    }
  }

  const columns = (
    (await rawQuery('PRAGMA table_info(api_keys)')) as Array<{ name: string }>
  ).map((c) => c.name);
  const rawRows = (await rawQuery('SELECT * FROM api_keys')) as Array<
    Record<string, unknown>
  >;

  await closeDatabase();

  const dump = canonicalizeRows({
    table: 'api_keys',
    columns,
    rawRows,
    orderBy: 'label',
  });

  process.stdout.write(JSON.stringify({ case: 'api-keys-tier2', ...dump }) + '\n');
  process.exit(0);
}

main().catch((err) => {
  process.stderr.write(`oracle case failed: ${err?.stack ?? err}\n`);
  process.exit(1);
});
