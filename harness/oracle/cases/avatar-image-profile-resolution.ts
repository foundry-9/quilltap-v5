/**
 * Tier-2 oracle case — v4 `resolveAvatarImageProfile` (P4.D279 §8,
 * `lib/image-gen/profile-resolution.ts:150-210`) over the REAL
 * `ImageProfilesRepository` (`findById` + `findDefault`), on a fresh copy of
 * the seed fixture built by build-avatar-image-profile-resolution-fixture.ts.
 *
 * For every row of the committed `matrix` this records the resolved profile's
 * `id` (or null) and the log lines v4 wrote while resolving — the three
 * `[ProfileResolution]` DEBUG lines and the repository fallback lines the
 * corrupt rows reach (`Error finding entity by ID`, `Error finding entity by
 * filter`; the repository's OUTER `Error finding default image profile` is in
 * the capture set so its absence is PROVEN, not assumed). Lines are captured
 * by splicing the singleton logger's transports; `LOG_LEVEL=debug` is set
 * before the logger loads so the DEBUG lines pass its level gate.
 *
 * Output: one NDJSON row per matrix entry —
 *   { case, userId, override, chat, result, lines: [{ level, message, fields }] }
 * where `fields` is the line's context minus `service` / `environment`.
 *
 * Run from the v4 server checkout under Node 24 (matches v4's `.nvmrc`), AFTER
 * building the fixture:
 *   N=~/.nvm/versions/node/v24.13.1/bin
 *   cd ~/source/quilltap-server
 *   QT_FIXTURE_AVATAR_PROFILE=/tmp/qt-avatar-profile-fixture.db \
 *     $N/npx tsx ~/source/quilltap-v5/harness/oracle/cases/avatar-image-profile-resolution.ts \
 *     > /tmp/oracle-avatar-profile.ndjson
 */

import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import { mkdtempSync, rmSync, mkdirSync, readFileSync, copyFileSync, existsSync } from 'node:fs';
import { tmpdir } from 'node:os';

interface MatrixRow {
  name: string;
  userId: string;
  override: string | null;
  chat: { imageProfileId: string | null } | null;
}

interface Spec {
  testPepperBase64: string;
  matrix: MatrixRow[];
}

const CAPTURED = new Set<string>([
  '[ProfileResolution] Avatar override profile unusable; falling back',
  '[ProfileResolution] Chat image profile unusable for avatars; falling back',
  '[ProfileResolution] No usable avatar image profile',
  'Error finding entity by ID',
  'Error finding entity by filter',
  'Error finding default image profile',
  'SQLite findOne error',
]);

async function main(): Promise<void> {
  const here = dirname(fileURLToPath(import.meta.url));
  const specPath = join(here, '..', 'fixtures', 'avatar-image-profile-resolution-tier2.json');
  const spec = JSON.parse(readFileSync(specPath, 'utf8')) as Spec;

  const fixture = process.env.QT_FIXTURE_AVATAR_PROFILE;
  if (!fixture || !existsSync(fixture)) {
    throw new Error(
      'QT_FIXTURE_AVATAR_PROFILE must point at the seed fixture from build-avatar-image-profile-resolution-fixture.ts'
    );
  }

  // Work on a fresh copy so the shared seed fixture stays pristine.
  const scratch = mkdtempSync(join(tmpdir(), 'qt-avatar-profile-oracle-'));
  process.on('exit', () => rmSync(scratch, { recursive: true, force: true }));
  mkdirSync(join(scratch, 'data'), { recursive: true });
  const work = join(scratch, 'avatar-profile-work.db');
  copyFileSync(fixture, work);

  // Env MUST be set before importing v4 config/manager/logger modules.
  process.env.ENCRYPTION_MASTER_PEPPER = spec.testPepperBase64;
  process.env.SQLITE_PATH = work;
  process.env.QUILLTAP_DATA_DIR = scratch;
  delete process.env.SQLITE_WAL_MODE;
  // DEBUG so the `[ProfileResolution]` lines pass the logger's level gate; the
  // transport splice below keeps stdout clean for the NDJSON.
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

  const { initializeDatabase, closeDatabase } = await import('@/lib/database/manager');
  const { ImageProfilesRepository } = await import(
    '@/lib/database/repositories/image-profiles.repository'
  );
  const { resolveAvatarImageProfile } = await import('@/lib/image-gen/profile-resolution');

  await initializeDatabase();
  const repos = { imageProfiles: new ImageProfilesRepository() };

  const out: string[] = [];
  for (const row of spec.matrix) {
    captured.length = 0;
    const profile = await resolveAvatarImageProfile(row.userId, repos, {
      override: row.override,
      chat: row.chat,
    });
    out.push(
      JSON.stringify({
        case: row.name,
        userId: row.userId,
        override: row.override,
        chat: row.chat,
        result: profile ? profile.id : null,
        lines: captured.slice(),
      })
    );
  }

  await closeDatabase();
  process.stdout.write(out.join('\n') + '\n');
  process.exit(0);
}

main().catch((err) => {
  process.stderr.write(`avatar-image-profile-resolution oracle failed: ${err?.stack ?? err}\n`);
  process.exit(1);
});
