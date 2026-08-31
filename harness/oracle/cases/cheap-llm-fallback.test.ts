/**
 * @jest-environment node
 *
 * P4.D135 — the cheap-LLM fallback chain builder (v4 `65f5021c8`,
 * `lib/memory/cheap-llm-tasks/fallback.ts`).
 *
 * Drives v4's REAL `buildCheapFallbackSelections` against a REAL fixture DB, so
 * both the profile-backed arm (walk the chain) and the profile-less arm (the
 * `allowCheapFallback` switch + a drafted `isCheap` stand-in) are compared as
 * SELECTIONS — provider, model, baseUrl, connectionProfileId, isLocal and the
 * resolved `profileParams`, which is where the two currencies actually meet.
 *
 * The fixture is the settings family's (`build-settings-fixture.ts`): userA with
 * `chat_settings`, three connection profiles (an OPENAI default, an ANTHROPIC
 * cheap one, and a Courier), and the api keys they name. The case FLIPS
 * `allowCheapFallback` between arms through v4's own repository, so the switch
 * is measured rather than assumed, and it seeds the chain wiring the same way.
 *
 * Run (Node 24, from the v4 checkout; jest ignores `.claude/` paths, so the case
 * is staged in a /tmp mirror):
 *   N=~/.nvm/versions/node/v24.13.1/bin ; V5W=${V5W:-$HOME/source/quilltap-v5}
 *   TMPO=/tmp/qt-cheapfallback-oracle
 *   rm -rf "$TMPO"; mkdir -p "$TMPO/cases" "$TMPO/fixtures"
 *   cp "$V5W/harness/oracle/cases/cheap-llm-fallback.test.ts" "$TMPO/cases/"
 *   cp "$V5W/harness/oracle/fixtures/settings.json"           "$TMPO/fixtures/"
 *   cd ~/source/quilltap-server
 *   QT_FIXTURE_SETTINGS_MAIN=/tmp/qt-cheapfallback-fixture.db \
 *     $N/node --import tsx $V5W/harness/oracle/fixtures/build-settings-fixture.ts
 *   QT_FIXTURE_CHEAP_FALLBACK=/tmp/qt-cheapfallback-fixture.db \
 *   QT_ORACLE_OUT=/tmp/oracle-cheap-llm-fallback.ndjson \
 *     $N/npx jest --silent --watchman=false --testTimeout=120000 \
 *       --roots "$PWD" --roots "$TMPO/cases" -- "cheap-llm-fallback\.test\.ts$"
 */

import * as fs from 'fs';
import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import { mkdtempSync, mkdirSync, copyFileSync } from 'node:fs';
import { tmpdir } from 'node:os';

interface Spec {
  testPepperBase64: string;
  userA: string;
  profiles: { gpt: string; claude: string; courier: string };
}

jest.setTimeout(120000);

it('cheap-llm fallback selections', async () => {
  const here = dirname(fileURLToPath(import.meta.url));
  const spec = JSON.parse(
    fs.readFileSync(join(here, '..', 'fixtures', 'settings.json'), 'utf8'),
  ) as Spec;

  const src = process.env.QT_FIXTURE_CHEAP_FALLBACK;
  const out = process.env.QT_ORACLE_OUT;
  if (!src || !out) throw new Error('set QT_FIXTURE_CHEAP_FALLBACK and QT_ORACLE_OUT');

  const scratch = mkdtempSync(join(tmpdir(), 'qt-cheap-fallback-'));
  mkdirSync(join(scratch, 'data'), { recursive: true });
  const work = join(scratch, 'data', 'quilltap.db');
  copyFileSync(src, work);

  process.env.ENCRYPTION_MASTER_PEPPER = spec.testPepperBase64;
  process.env.SQLITE_PATH = work;
  process.env.QUILLTAP_DATA_DIR = scratch;
  delete process.env.SQLITE_WAL_MODE;
  process.env.LOG_LEVEL = 'error';

  // `jest.setup.ts` replaces the whole DB stack with bare `jest.fn()`s — the
  // manager, the repositories, the factory. They all have to be restored before
  // anything asks for a repo (the `settings-routes` idiom), and `safeQuery`
  // SWALLOWS the failure otherwise: a mocked `getDatabaseAsync` returns
  // undefined, every write quietly no-ops, and the case goes green having
  // measured a world it never built. Same for the provider validation the tier
  // picker's credential gate reads.
  const cipherDriverPath = join(
    process.cwd(),
    'packages/quilltap/node_modules/better-sqlite3-multiple-ciphers',
  );
  jest.doMock('better-sqlite3', () => jest.requireActual(cipherDriverPath));
  jest.doMock('@/lib/database/manager', () => jest.requireActual('@/lib/database/manager'));
  jest.doMock('@/lib/database/repositories', () =>
    jest.requireActual('@/lib/database/repositories'),
  );
  jest.doMock('@/lib/repositories/factory', () =>
    jest.requireActual('@/lib/repositories/factory'),
  );
  jest.doMock('@/lib/plugins/provider-validation', () =>
    jest.requireActual('@/lib/plugins/provider-validation'),
  );
  const { initializeDatabase, closeDatabase } = await import('@/lib/database/manager');
  const { getRepositories } = await import('@/lib/repositories/factory');
  const { buildCheapFallbackSelections } = await import(
    '@/lib/memory/cheap-llm-tasks/fallback'
  );

  await initializeDatabase();
  const repos = getRepositories();

  // ── Wiring the arms need, applied through v4's own repositories ──────────
  //
  // The committed fixture has no chain and no cheap profile; both are set here
  // so the SAME statements shape both sides' world (the Rust harness replays
  // them before it runs).
  await repos.connections.update(spec.profiles.gpt, {
    fallbackProfileId: spec.profiles.claude,
    allowTierFallback: true,
    modelClass: 'Standard',
  } as never);
  await repos.connections.update(spec.profiles.claude, {
    isCheap: true,
    modelClass: 'Standard',
  } as never);

  const lines: string[] = [];

  const setAllowCheapFallback = async (value: boolean) => {
    // `findOrCreateByUserId` is what the read side uses, so the row this writes
    // is the row `buildCheapFallbackSelections` will read back.
    const current =
      (await repos.chatSettings.findByUserId(spec.userA)) ??
      (await repos.chatSettings.create({ userId: spec.userA } as never));
    await repos.chatSettings.update(current.id, {
      cheapLLMSettings: { ...current.cheapLLMSettings, allowCheapFallback: value },
    } as never);
  };

  const run = async (
    name: string,
    selection: Record<string, unknown>,
    opts: { dangerous?: boolean; alreadyTried?: string[]; allowCheapFallback?: boolean },
  ) => {
    if (opts.allowCheapFallback !== undefined) {
      await setAllowCheapFallback(opts.allowCheapFallback);
    }
    const selections = await buildCheapFallbackSelections({
      selection: selection as never,
      userId: spec.userA,
      dangerous: !!opts.dangerous,
      alreadyTried: opts.alreadyTried ?? [],
      taskType: 'oracle',
    });
    lines.push(
      JSON.stringify({
        name,
        selection,
        dangerous: !!opts.dangerous,
        alreadyTried: opts.alreadyTried ?? [],
        allowCheapFallback: opts.allowCheapFallback ?? null,
        selections,
      }),
    );
  };

  const profileBacked = {
    provider: 'OPENAI',
    modelName: 'gpt-4o',
    baseUrl: undefined,
    connectionProfileId: spec.profiles.gpt,
    isLocal: false,
  };
  const profileLess = {
    provider: 'OLLAMA',
    modelName: 'llama3',
    baseUrl: 'http://localhost:11434',
    connectionProfileId: undefined,
    isLocal: true,
  };

  // Profile-backed: the chain, minus the primary itself.
  await run('backed_chain', profileBacked, { allowCheapFallback: false });
  // …and with the understudy already spent, so only a tier pick can answer.
  await run('backed_already_tried_understudy', profileBacked, {
    alreadyTried: [spec.profiles.claude],
  });
  // Dangerous: an auto-picked tier candidate must be cleared for the content,
  // and neither fixture profile is — so only the NAMED understudy survives.
  await run('backed_dangerous', profileBacked, { dangerous: true });
  // A selection naming a profile that is not there has no chain at all.
  await run(
    'backed_missing_profile',
    { ...profileBacked, connectionProfileId: '5e4f0000-0000-4000-8000-0000000000ff' },
    {},
  );

  // Profile-less: the switch governs, in both positions.
  //
  // ⚠ The cheap profile's `modelClass` has to come OFF first, or the ON arm is
  // vacuous. A profile-less route stands in a SYNTHETIC failed profile with no
  // model class, and `tierMatches` reads unknown-vs-KNOWN as a non-match in
  // both directions — so a classified cheap profile is never drafted and the
  // switch's two positions would both answer `[]`. Measured, not assumed: that
  // is exactly what the first run of this case reported.
  await repos.connections.update(spec.profiles.claude, { modelClass: null } as never);

  await run('profileless_switch_off', profileLess, { allowCheapFallback: false });
  await run('profileless_switch_on', profileLess, { allowCheapFallback: true });
  // …and the drafted stand-in is still filtered on danger.
  await run('profileless_dangerous', profileLess, { dangerous: true });

  await closeDatabase();
  fs.writeFileSync(out, lines.map((l) => l + '\n').join(''));
  process.stderr.write(`cheap-llm-fallback oracle wrote ${out} (${lines.length} cases)\n`);
  expect(lines.length).toBeGreaterThan(0);
});
