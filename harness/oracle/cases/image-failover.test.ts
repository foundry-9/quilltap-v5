/**
 * @jest-environment node
 *
 * Tier-3 ORACLE for the image-failover chokepoint (P4.D225; v4
 * `lib/services/dangerous-content/image-failover.ts`, NEW at `8bd080267`, the
 * ledger at `49059fb14`).
 *
 * Drives v4's REAL `generateImageWithConciergeFailover` with the caller's
 * `attempt` closure scripted per profile (answer, or throw a typed
 * `ModerationRejectionError` / a plain `Error` with an optional `code`).
 * Everything else is real: `classifyRefusal`, the default understudy resolver
 * (`resolveUncensoredImageUnderstudy`) over the baked image profiles, the
 * Concierge writer's refusal bubbles, `recordModerationRefusal` and the
 * auto-switch — over the fixture `build-refusal-ledger-fixture.ts` builds with
 * `QT_REFUSAL_LEDGER_SPEC=image-failover.json`. API keys come from a canned
 * seam on the connections repository (the danger-routing arrangement). The
 * dialog arms supply their own CONNECTION-profile resolver, as v4's legacy
 * dialog does.
 *
 * `ModerationRejectionError` is imported INSIDE the case, after
 * `resetModules` — the class is compared by `code`, never `instanceof`, but
 * the typed arm must be the plugin contract's real class.
 *
 * Per case: the attempt calls (profile, key), the outcome (answer or error +
 * `getConciergeTrail`), and every `ConciergeImageFailover` /
 * `ConciergeRefusal` / `ConciergeRefusalLedger` / `[ConciergeNotification]`
 * line; then the `chats` + `chat_messages` dumps.
 *
 * Regen recipe: the header of
 * `crates/quilltap-harness/tests/image_failover_tier3_equivalence.rs`.
 */

import * as fs from 'fs';
import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import { mkdtempSync, mkdirSync, copyFileSync, existsSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';

function canonValue(v: unknown): unknown {
  if (v === null || v === undefined) return null;
  if (typeof Buffer !== 'undefined' && Buffer.isBuffer(v)) return v.toString('hex');
  return v;
}

type Throw = { typed?: boolean; message: string; status?: number | null; providerReason?: string; code?: string };
type Step = { answers: string } | { throws: Throw };
interface Case {
  name: string;
  userId: string;
  chatId: string | null;
  purpose: string;
  /**
   * v4 `3b463d6b1` (#76): the stored `conciergeSettings` the policy is
   * resolved from (WITH `chat` where the case names one), as the callers do.
   */
  concierge: Record<string, unknown>;
  primary: string;
  script: Record<string, Step[]>;
  profileKind?: 'connection' | 'image';
  primaryVia?: string;
  customUnderstudy?: string | null;
  customUnderstudyNone?: boolean;
  /** P4.D226 (v4 `4d370a90f`): the chat's state when the call began (`ctx.chat`). */
  chat?: Record<string, unknown> | null;
  /** P4.D226: the operator locks the chat while the primary is thinking. */
  lockDuringAttempt?: boolean;
  /** P4.D228 (v4 `ce2f1dabf`, #77): `ctx.announceUnresolvedRefusal`. */
  announceUnresolvedRefusal?: boolean;
}
interface Spec {
  testPepperBase64: string;
  apiKeys: Record<string, string>;
  ledgerPlants: Array<{ chatId: string; count: number; lastAt: string }>;
  cases: Case[];
}

const SERVICES = new Set([
  'ConciergeImageFailover',
  'ConciergeRefusal',
  'ConciergeRefusalLedger',
  // P4.D226 (v4 `4d370a90f`): the state re-read at refusal time.
  'ConciergeCurrentState',
]);

async function main(): Promise<void> {
  const here = dirname(fileURLToPath(import.meta.url));
  const spec = JSON.parse(
    fs.readFileSync(join(here, '..', 'fixtures', 'image-failover.json'), 'utf8'),
  ) as Spec;
  const fixture = process.env.QT_FIXTURE_IMAGE_FAILOVER;
  if (!fixture || !existsSync(fixture)) {
    throw new Error('QT_FIXTURE_IMAGE_FAILOVER must point at the seed fixture');
  }
  const outPath = process.env.QT_ORACLE_OUT;
  if (!outPath) throw new Error('QT_ORACLE_OUT must point at the NDJSON file to write');

  const scratch = mkdtempSync(join(tmpdir(), 'qt-image-failover-oracle-'));
  scratchDirs.push(scratch);
  mkdirSync(join(scratch, 'data'), { recursive: true });
  const work = join(scratch, 'image-failover-work.db');
  copyFileSync(fixture, work);

  process.env.ENCRYPTION_MASTER_PEPPER = spec.testPepperBase64;
  process.env.SQLITE_PATH = work;
  process.env.QUILLTAP_DATA_DIR = scratch;
  delete process.env.SQLITE_WAL_MODE;
  delete process.env.QUILLTAP_JOB_CHILD;
  process.env.LOG_LEVEL = 'error';

  const logs: Array<{ service: string | null; level: string; message: string; bag: unknown }> = [];
  jest.resetModules();
  jest.doMock('@/lib/logger', () => {
    const recorder = (service: string | null) => {
      const record = (level: string) => (message: string, bag?: Record<string, unknown>) =>
        logs.push({ service, level, message, bag: JSON.parse(JSON.stringify(bag ?? {})) });
      const self: Record<string, unknown> = {
        debug: record('debug'),
        info: record('info'),
        warn: record('warn'),
        error: record('error'),
        trace: record('trace'),
      };
      self.child = (ctx: Record<string, unknown>) =>
        recorder(typeof ctx?.service === 'string' ? ctx.service : service);
      return self;
    };
    return {
      __esModule: true,
      LogLevel: { ERROR: 'error', WARN: 'warn', INFO: 'info', DEBUG: 'debug', TRACE: 'trace' },
      logger: recorder(null),
    };
  });
  const cipherDriverPath = require('node:path').join(
    process.cwd(),
    'packages/quilltap/node_modules/better-sqlite3-multiple-ciphers',
  );
  jest.doMock('better-sqlite3', () => jest.requireActual(cipherDriverPath));
  jest.doMock('@/lib/database/manager', () => jest.requireActual('@/lib/database/manager'));
  jest.doMock('@/lib/database/repositories', () => jest.requireActual('@/lib/database/repositories'));
  jest.doMock('@/lib/repositories/factory', () => jest.requireActual('@/lib/repositories/factory'));
  jest.doMock('@/lib/services/concierge-notifications/writer', () =>
    jest.requireActual('@/lib/services/concierge-notifications/writer'),
  );

  const { initializeDatabase, closeDatabase, rawQuery } = await import('@/lib/database/manager');
  const { getRepositories } = await import('@/lib/repositories/factory');
  const { generateImageWithConciergeFailover, getConciergeTrail } = await import(
    '@/lib/services/dangerous-content/image-failover'
  );
  const { ConciergeSettingsSchema } = await import('@/lib/schemas/settings.types');
  const { resolveConciergeSettings } = await import('@/lib/services/dangerous-content/resolver.service');
  // The plugin contract's REAL class, by path: a bare `@quilltap/plugin-types`
  // resolves against THIS file's directory, outside the v4 tree.
  const { ModerationRejectionError } = require(
    join(process.cwd(), 'packages/plugin-types/src/common/errors'),
  ) as typeof import('@quilltap/plugin-types');

  await initializeDatabase();
  const repos = getRepositories();
  // Canned key seam (the danger-routing arrangement).
  (repos.connections as any).findApiKeyByIdAndUserId = async (id: string, userId: string) => {
    const kv = spec.apiKeys[id];
    if (!kv) return null;
    return {
      id, userId, label: 'canned', provider: 'OPENAI', key_value: kv, isActive: true,
      createdAt: '2020-01-01T00:00:00.000Z', updatedAt: '2020-01-01T00:00:00.000Z',
    };
  };
  for (const plant of spec.ledgerPlants) {
    await rawQuery(
      'UPDATE chats SET "moderationRefusalCount" = ?, "lastModerationRefusalAt" = ? WHERE id = ?',
      [plant.count, plant.lastAt, plant.chatId],
    );
  }

  const makeError = (t: Throw): Error => {
    if (t.typed) return new ModerationRejectionError(t.message, t.status ?? undefined, t.providerReason);
    const e = new Error(t.message);
    if (t.code) (e as Error & { code?: string }).code = t.code;
    return e;
  };
  const findProfile = async (id: string, kind: 'connection' | 'image') =>
    kind === 'connection' ? repos.connections.findById(id) : repos.imageProfiles.findById(id);

  const lines: string[] = [];
  let typedThrown = 0;
  for (const c of spec.cases) {
    const kind = c.profileKind ?? 'image';
    const primary = await findProfile(c.primary, kind);
    if (!primary) throw new Error(`${c.name}: primary ${c.primary} not seeded`);
    const script: Record<string, Step[]> = JSON.parse(JSON.stringify(c.script));
    const calls: Array<{ profileId: string; apiKey: string }> = [];
    let lockPending = c.lockDuringAttempt === true;
    const attempt = async (profile: { id: string }, apiKey: string) => {
      calls.push({ profileId: profile.id, apiKey });
      if (lockPending) {
        lockPending = false;
        await rawQuery('UPDATE chats SET "conciergeMode" = ? WHERE id = ?', ['locked', c.chatId]);
      }
      const step = script[profile.id]?.shift();
      if (!step) throw new Error(`${c.name}: no scripted step for ${profile.id}`);
      if ('throws' in step) {
        if (step.throws.typed) typedThrown += 1;
        throw makeError(step.throws);
      }
      return step.answers;
    };
    const conciergePolicy = resolveConciergeSettings(
      { conciergeSettings: ConciergeSettingsSchema.parse(c.concierge) },
      (c.chat ?? null) as never,
    );
    const ctx: Record<string, unknown> = {
      userId: c.userId,
      chatId: c.chatId,
      purpose: c.purpose,
      conciergePolicy,
      ...(c.profileKind ? { profileKind: c.profileKind } : {}),
      ...(c.primaryVia ? { primaryVia: c.primaryVia } : {}),
      ...('chat' in c ? { chat: c.chat } : {}),
      ...(c.announceUnresolvedRefusal !== undefined
        ? { announceUnresolvedRefusal: c.announceUnresolvedRefusal }
        : {}),
    };
    if ('customUnderstudy' in c) {
      ctx.resolveUnderstudy = async (_exclude: string[]) => {
        if (c.customUnderstudyNone || !c.customUnderstudy) return null;
        const profile = await repos.connections.findById(c.customUnderstudy);
        return profile ? { profile, apiKey: spec.apiKeys[profile.apiKeyId as string] } : null;
      };
    }
    logs.length = 0;
    let outcome: Record<string, unknown>;
    try {
      const r = await generateImageWithConciergeFailover(
        { profile: primary as never, apiKey: spec.apiKeys[(primary as { apiKeyId: string }).apiKeyId] },
        attempt as never,
        ctx as never,
      );
      outcome = {
        ok: true,
        result: r.result,
        profileId: (r.profile as { id: string }).id,
        apiKey: r.apiKey,
        rerouted: r.rerouted,
        trail: r.trail,
      };
    } catch (e) {
      outcome = { ok: false, message: (e as Error).message, trail: getConciergeTrail(e) };
    }
    for (const [id, rest] of Object.entries(script)) {
      if (rest.length) throw new Error(`${c.name}: ${rest.length} unused step(s) for ${id}`);
    }
    lines.push(
      JSON.stringify({
        kind: 'case',
        name: c.name,
        calls,
        outcome,
        logs: logs
          .filter(
            (l) =>
              (l.service !== null && SERVICES.has(l.service)) ||
              (l.service === null && l.message.startsWith('[ConciergeNotification]')),
          )
          .map(({ service, level, message, bag }) => ({ service, level, message, bag })),
      }),
    );
  }
  if (typedThrown === 0) throw new Error('the corpus threw no typed ModerationRejectionError');

  const dumpTable = async (table: string, orderBy: string) => {
    const columns = ((await rawQuery(`PRAGMA table_info(${table})`)) as Array<{ name: string }>).map(
      (c) => c.name,
    );
    const rawRows = (await rawQuery(`SELECT * FROM ${table}`)) as Array<Record<string, unknown>>;
    const rows = rawRows
      .map((r) => {
        const out: Record<string, unknown> = {};
        for (const col of columns) out[col] = canonValue(r[col]);
        return out;
      })
      .sort((a, b) => {
        const av = String(a[orderBy] ?? '');
        const bv = String(b[orderBy] ?? '');
        return av < bv ? -1 : av > bv ? 1 : 0;
      });
    return { table, columns, rows };
  };
  lines.push(JSON.stringify({ kind: 'table', ...(await dumpTable('chats', 'id')) }));
  lines.push(JSON.stringify({ kind: 'table', ...(await dumpTable('chat_messages', 'chatId')) }));

  await closeDatabase();
  fs.writeFileSync(outPath, lines.join('\n') + '\n');
  process.stderr.write(
    `image-failover oracle wrote ${outPath} (${spec.cases.length} cases, ${typedThrown} typed throws)\n`,
  );
}

test('image-failover tier-3 oracle', async () => {
  await main();
});

// Remove the OS-temp scratch dir(s) once the oracle has written its NDJSON.
const scratchDirs: string[] = [];
afterAll(() => {
  for (const d of scratchDirs) rmSync(d, { recursive: true, force: true });
});
