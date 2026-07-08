/**
 * Fixture builder for the P4.1d danger-scan differential (v4
 * `lib/background-jobs/scheduled-danger-scan.ts` `runScheduledDangerScan`).
 *
 * Seeds (main DB only, pinned ids/timestamps, through v4's REAL repos):
 *   - three users' `chat_settings`: userOn (mode DETECT_ONLY), userOff (OFF),
 *     userNoProfiles (DETECT_ONLY, but no connection profiles);
 *   - two connection profiles for userOn (P1 inserted first = the fallback
 *     `availableProfiles[0]`, P2 the participant-referenced one);
 *   - the per-chat gate matrix under userOn:
 *       c1 help (exempt)                          → skipped
 *       c2 conciergeOverride OFF (operator wave)   → skipped
 *       c3 sticky dangerous                        → skipped
 *       c4 safe, not grown (count == classifiedAt) → skipped
 *       c5 never classified + summary; participants [user-controlled w/ P1,
 *          LLM w/ P2]                              → CLASSIFICATION via P2
 *          (proving the controlledBy filter + participant-first resolution)
 *       c6 never classified, no summary, count 51; participant refs P1
 *                                                  → CONTEXT_SUMMARY via P1
 *       c7 never classified, no summary, count 3; participant refs a BOGUS
 *          profile id                              → CLASSIFICATION via the
 *          fallback P1 (stale participant profile)
 *       c8 safe but GROWN (classifiedAt 5 < count 10) + summary
 *                                                  → CLASSIFICATION
 *   - userOff: one never-classified chat → nothing (user skipped, not counted);
 *   - userNoProfiles: one never-classified chat → skipped per-chat (no
 *     resolvable profile), user still counted as processed.
 *
 * Expected: usersProcessed = 2, chatsEnqueued = 4.
 *
 * Run (Node 24, from the v4 checkout):
 *   N=~/.nvm/versions/node/v24.13.1/bin ; V5=~/source/quilltap-v5
 *   cd ~/source/quilltap-server
 *   QT_FIXTURE_OUT=/tmp/qt-danger-scan.db \
 *     $N/npx tsx $V5/harness/oracle/fixtures/build-danger-scan-fixture.ts
 */

import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import { mkdtempSync, mkdirSync, readFileSync, rmSync, existsSync } from 'node:fs';
import { tmpdir } from 'node:os';

interface Spec {
  testPepperBase64: string;
  seedTimestamp: string;
  userOn: string;
  userOff: string;
  userNoProfiles: string;
  characterId: string;
  profileP1: string;
  profileP2: string;
  dangerOn: Record<string, unknown>;
  dangerOff: Record<string, unknown>;
}

async function main(): Promise<void> {
  const here = dirname(fileURLToPath(import.meta.url));
  const spec = JSON.parse(readFileSync(join(here, 'danger-scan.json'), 'utf8')) as Spec;

  const out = process.env.QT_FIXTURE_OUT;
  if (!out) throw new Error('QT_FIXTURE_OUT must point at the fixture .db to write');
  for (const suffix of ['', '-journal', '-wal', '-shm']) {
    const p = out + suffix;
    if (existsSync(p)) rmSync(p);
  }

  const scratch = mkdtempSync(join(tmpdir(), 'qt-danger-scan-build-'));
  mkdirSync(join(scratch, 'data'), { recursive: true });

  process.env.ENCRYPTION_MASTER_PEPPER = spec.testPepperBase64;
  process.env.SQLITE_PATH = out;
  process.env.QUILLTAP_DATA_DIR = scratch;
  delete process.env.SQLITE_WAL_MODE;
  process.env.LOG_LEVEL = 'error';

  const { initializeDatabase, closeDatabase } = await import('@/lib/database/manager');
  const { getRepositories } = await import('@/lib/repositories/factory');

  await initializeDatabase();
  const repos = getRepositories();
  const ts = spec.seedTimestamp;

  // chat_settings rows: insertion order = the sweep's user order.
  const settings: Array<[string, string, Record<string, unknown>]> = [
    ['f0000000-0000-4000-8000-0000000000a1', spec.userOn, spec.dangerOn],
    ['f0000000-0000-4000-8000-0000000000a2', spec.userOff, spec.dangerOff],
    ['f0000000-0000-4000-8000-0000000000a3', spec.userNoProfiles, spec.dangerOn],
  ];
  for (const [id, userId, dcs] of settings) {
    await repos.chatSettings.create(
      { userId, dangerousContentSettings: dcs } as never,
      { id, createdAt: ts, updatedAt: ts },
    );
  }

  // userOn's two connection profiles (P1 first → availableProfiles[0]).
  const profile = (id: string, name: string) =>
    repos.connections.create(
      {
        userId: spec.userOn,
        name,
        provider: 'ANTHROPIC',
        modelName: 'claude-sonnet-4-5',
      } as never,
      { id, createdAt: ts, updatedAt: ts },
    );
  await profile(spec.profileP1, 'P1 Fallback');
  await profile(spec.profileP2, 'P2 Participant');

  // Participant helper (schema-required fields + the resolution-relevant ones).
  let pn = 0;
  const participant = (controlledBy: string, connectionProfileId?: string) => {
    pn += 1;
    return {
      id: `fb0000${String(pn).padStart(2, '0')}-0000-4000-8000-000000000001`,
      type: 'CHARACTER',
      characterId: spec.characterId,
      controlledBy,
      ...(connectionProfileId ? { connectionProfileId } : {}),
      createdAt: ts,
      updatedAt: ts,
    };
  };

  interface ChatSeed {
    id: string;
    user: string;
    data: Record<string, unknown>;
  }
  const chats: ChatSeed[] = [
    {
      id: 'c0000000-0000-4000-8000-0000000000c1',
      user: spec.userOn,
      data: { chatType: 'help', participants: [participant('llm', spec.profileP1)] },
    },
    {
      id: 'c0000000-0000-4000-8000-0000000000c2',
      user: spec.userOn,
      data: {
        chatType: 'salon',
        conciergeOverride: 'OFF',
        participants: [participant('llm', spec.profileP1)],
      },
    },
    {
      id: 'c0000000-0000-4000-8000-0000000000c3',
      user: spec.userOn,
      data: {
        chatType: 'salon',
        isDangerousChat: true,
        dangerClassifiedAtMessageCount: 1,
        messageCount: 100,
        participants: [participant('llm', spec.profileP1)],
      },
    },
    {
      id: 'c0000000-0000-4000-8000-0000000000c4',
      user: spec.userOn,
      data: {
        chatType: 'salon',
        isDangerousChat: false,
        dangerClassifiedAtMessageCount: 10,
        messageCount: 10,
        participants: [participant('llm', spec.profileP1)],
      },
    },
    {
      id: 'c0000000-0000-4000-8000-0000000000c5',
      user: spec.userOn,
      data: {
        chatType: 'salon',
        contextSummary: 'A perfectly reputable summary.',
        messageCount: 12,
        participants: [
          participant('user', spec.profileP1), // user-controlled → skipped
          participant('llm', spec.profileP2), // → the chosen profile
        ],
      },
    },
    {
      id: 'c0000000-0000-4000-8000-0000000000c6',
      user: spec.userOn,
      data: {
        chatType: 'salon',
        messageCount: 51,
        participants: [participant('llm', spec.profileP1)],
      },
    },
    {
      id: 'c0000000-0000-4000-8000-0000000000c7',
      user: spec.userOn,
      data: {
        chatType: 'salon',
        messageCount: 3,
        participants: [
          participant('llm', 'e0000000-0000-4000-8000-0000000000ff'), // stale
        ],
      },
    },
    {
      id: 'c0000000-0000-4000-8000-0000000000c8',
      user: spec.userOn,
      data: {
        chatType: 'salon',
        isDangerousChat: false,
        dangerClassifiedAtMessageCount: 5,
        messageCount: 10,
        contextSummary: 'Grown since the last look.',
        participants: [participant('llm', spec.profileP2)],
      },
    },
    {
      id: 'c0000000-0000-4000-8000-0000000000c9',
      user: spec.userOff,
      data: { chatType: 'salon', participants: [participant('llm', spec.profileP1)] },
    },
    {
      id: 'c0000000-0000-4000-8000-0000000000ca',
      user: spec.userNoProfiles,
      data: { chatType: 'salon', participants: [participant('llm')] },
    },
  ];

  let n = 0;
  for (const c of chats) {
    n += 1;
    await repos.chats.create(
      { userId: c.user, title: `Scan chat ${n}`, ...c.data } as never,
      { id: c.id, createdAt: ts, updatedAt: ts },
    );
  }

  // Materialize background_jobs (the sweep target) so both sides see the table.
  await repos.backgroundJobs.getStats();

  await closeDatabase();
  process.stderr.write(`built danger-scan fixture: ${out} (${chats.length} chats)\n`);
  process.exit(0);
}

main().catch((err) => {
  process.stderr.write(`danger-scan fixture build failed: ${err?.stack ?? err}\n`);
  process.exit(1);
});
