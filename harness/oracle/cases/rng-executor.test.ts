/**
 * @jest-environment node
 *
 * ORACLE for the RNG tool executor (W4.1a sub-unit 2; v4
 * `lib/tools/handlers/rng-handler.ts` `executeRngTool` + `formatRngResults`).
 *
 * Drives v4's REAL `executeRngTool` over the committed corpus
 * (harness/oracle/fixtures/rng-executor.json) against a REAL fixture DB (the
 * spin-the-bottle path reads participants + character names through the real
 * repos), with ONLY `crypto.randomBytes` pinned — mocked to each case's committed
 * byte sequence, the same bytes the Rust `FixedBytes` source replays. Because
 * `secureRandomInt`'s rejection sampling consumes a *variable* number of bytes,
 * byte-exact algorithm fidelity is itself part of what the differential proves
 * (the Rust harness asserts each case consumed exactly the committed bytes).
 *
 * Uses the jest-real-DB pattern ([[jest-real-db-oracle]]): resetModules +
 * doMock past jest.setup's global DB mocks; `better-sqlite3` → the real cipher
 * binding by absolute path. `crypto.randomUUID` / `createHash` stay REAL (spread
 * of requireActual) so id minting + the vault overlay's SHA are genuine.
 *
 * Run from the v4 server checkout under Node 24:
 *   N=~/.nvm/versions/node/v24.13.1/bin ; V5=~/source/quilltap-v5
 *   cd ~/source/quilltap-server
 *   QT_FIXTURE_OUT=/tmp/qt-rng-main.db QT_FIXTURE_MOUNT_OUT=/tmp/qt-rng-mount.db \
 *     $N/npx tsx $V5/harness/oracle/fixtures/build-rng-executor-fixture.ts
 *   QT_FIXTURE_RNG_MAIN=/tmp/qt-rng-main.db QT_FIXTURE_RNG_MOUNT=/tmp/qt-rng-mount.db \
 *   QT_ORACLE_OUT=/tmp/oracle-rng-executor.ndjson \
 *     $N/npx jest --silent --watchman=false --roots "$PWD" --roots "$V5/harness/oracle/cases" -- rng-executor
 */

import * as fs from 'fs';
import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import { mkdtempSync, mkdirSync, copyFileSync, existsSync } from 'node:fs';
import { tmpdir } from 'node:os';

interface CaseSpec {
  name: string;
  input: unknown;
  chatId: string;
  bytes: number[];
}
interface Spec {
  testPepperBase64: string;
  userId: string;
  cases: CaseSpec[];
}

// The per-case injected byte stream (mirrors the Rust `FixedBytes`). The mocked
// `crypto.randomBytes(n)` serves `n` bytes from here and advances the cursor;
// exhaustion throws (catching an unexpected consumer / a byte-shortage).
let currentBytes: number[] = [];
let byteCursor = 0;

async function main(): Promise<void> {
  const here = dirname(fileURLToPath(import.meta.url));
  const spec = JSON.parse(
    fs.readFileSync(join(here, '..', 'fixtures', 'rng-executor.json'), 'utf8')
  ) as Spec;

  const fixtureMain = process.env.QT_FIXTURE_RNG_MAIN;
  const fixtureMount = process.env.QT_FIXTURE_RNG_MOUNT;
  if (!fixtureMain || !existsSync(fixtureMain) || !fixtureMount || !existsSync(fixtureMount)) {
    throw new Error('QT_FIXTURE_RNG_MAIN / QT_FIXTURE_RNG_MOUNT must point at the seed fixture');
  }
  const outPath = process.env.QT_ORACLE_OUT;
  if (!outPath) throw new Error('QT_ORACLE_OUT must point at the NDJSON file to write');

  const scratch = mkdtempSync(join(tmpdir(), 'qt-rng-oracle-'));
  mkdirSync(join(scratch, 'data'), { recursive: true });
  const workMain = join(scratch, 'rng-main.db');
  const workMount = join(scratch, 'rng-mount.db');
  copyFileSync(fixtureMain, workMain);
  copyFileSync(fixtureMount, workMount);

  process.env.ENCRYPTION_MASTER_PEPPER = spec.testPepperBase64;
  process.env.SQLITE_PATH = workMain;
  process.env.SQLITE_MOUNT_INDEX_PATH = workMount;
  process.env.QUILLTAP_DATA_DIR = scratch;
  delete process.env.SQLITE_WAL_MODE;
  process.env.LOG_LEVEL = 'error';

  jest.resetModules();
  const cipherDriverPath = require('node:path').join(
    process.cwd(),
    'packages/quilltap/node_modules/better-sqlite3-multiple-ciphers'
  );
  jest.doMock('better-sqlite3', () => jest.requireActual(cipherDriverPath));
  jest.doMock('@/lib/database/manager', () => jest.requireActual('@/lib/database/manager'));
  jest.doMock('@/lib/database/repositories', () =>
    jest.requireActual('@/lib/database/repositories')
  );
  jest.doMock('@/lib/repositories/factory', () => jest.requireActual('@/lib/repositories/factory'));
  // Pin ONLY crypto.randomBytes; keep randomUUID + createHash real (id minting,
  // vault SHA). The handler does `import { randomBytes } from 'crypto'`.
  // Patch ONLY randomBytes; keep every other crypto export (createHash for the
  // vault overlay's SHA, randomUUID for id minting) real. No `__esModule: true`
  // and no explicit `default` — so esModuleInterop synthesizes the default from
  // the whole patched object, keeping `import crypto from 'crypto'` intact (the
  // overlay's default import; `__esModule:true` would null its `.createHash`).
  jest.doMock('crypto', () => {
    const actual = jest.requireActual('crypto');
    return {
      ...actual,
      randomBytes: (n: number) => {
        const end = byteCursor + n;
        if (end > currentBytes.length) {
          throw new Error(
            `randomBytes mock exhausted: needed ${n} at cursor ${byteCursor} of ${currentBytes.length}`
          );
        }
        const slice = currentBytes.slice(byteCursor, end);
        byteCursor = end;
        return Buffer.from(slice);
      },
    };
  });

  const { initializeDatabase, closeDatabase } = await import('@/lib/database/manager');
  const { executeRngTool, formatRngResults } = await import('@/lib/tools/handlers/rng-handler');

  await initializeDatabase();

  const lines: string[] = [];
  for (const c of spec.cases) {
    currentBytes = c.bytes;
    byteCursor = 0;
    const output = await executeRngTool(c.input, { userId: spec.userId, chatId: c.chatId });
    const formatted = formatRngResults(output);
    if (byteCursor !== c.bytes.length) {
      throw new Error(
        `case ${c.name}: consumed ${byteCursor} bytes, committed ${c.bytes.length}`
      );
    }
    lines.push(JSON.stringify({ kind: 'case', name: c.name, output, formatted }));
  }

  await closeDatabase();
  fs.writeFileSync(outPath, lines.join('\n') + '\n');
  process.stderr.write(`rng-executor oracle wrote ${outPath} (${spec.cases.length} cases)\n`);
}

test('rng-executor oracle', async () => {
  await main();
});
