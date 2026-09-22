/**
 * Tier-2 ORACLE for the speaker-name resolver (P4.D212; v4
 * `lib/chat/speaker-names.ts`, `e7821606f`, bug 161).
 *
 * Opens a COPY of the baked fixture (build-speaker-names-fixture.ts) and, per
 * spec chat, reads the chat through v4's REAL `repos.chats.findById`, hands it
 * to v4's REAL `resolveSpeakerNames`, and labels the spec's label list plus
 * every seat of the chat (under USER and ASSISTANT) through v4's REAL
 * `speakerLabel`. Emits, per case:
 *
 *   - `repoReadReturned` — whether `repos.chats.findById` answered the chat
 *                 (false for the planted `read: raw` chat — see the spec);
 *   - `seats`   — the `(id, characterId)` projection the resolver was fed
 *                 (the Rust side asserts it read the same seats);
 *   - `names`   — the resolved Map as an ORDERED pair list (insertion order);
 *   - `reads`   — every `repos.characters.findByIdRaw` call, in order (the
 *                 container is cached, so wrapping the instance method sees
 *                 the resolver's own reads) — "reads each seat once";
 *   - `labels`  — `{ participantId, role, label }` rows.
 *
 * Plus top-level `probes`: v4's real `findByIdRaw` answer (found / none /
 * threw, and the name) for every spec character and the missing id.
 *
 * Run (Node 24, from the v4 checkout), AFTER building the fixture:
 *   N=~/.nvm/versions/node/v24.13.1/bin ; V5W=${V5W:-$HOME/source/quilltap-v5}
 *   cd ~/source/quilltap-server
 *   QT_FIXTURE_SPEAKER_NAMES_MAIN=/tmp/qt-speaker-names-main.db \
 *   QT_FIXTURE_SPEAKER_NAMES_MOUNT=/tmp/qt-speaker-names-mount.db \
 *     $N/npx tsx $V5W/harness/oracle/cases/speaker-names.ts \
 *     > /tmp/oracle-speaker-names.ndjson
 */

import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import { mkdtempSync, mkdirSync, readFileSync, copyFileSync, existsSync } from 'node:fs';
import { tmpdir } from 'node:os';

interface LabelSpec {
  participantId: string | null;
  role: string;
}
interface Spec {
  testPepperBase64: string;
  characters: Array<{ id: string }>;
  missingCharacterId: string;
  chats: Array<{ name: string; id: string; read: 'repo' | 'raw' }>;
  labels: LabelSpec[];
}

async function main(): Promise<void> {
  const here = dirname(fileURLToPath(import.meta.url));
  const spec = JSON.parse(
    readFileSync(join(here, '..', 'fixtures', 'speaker-names.json'), 'utf8')
  ) as Spec;

  const mainFixture = process.env.QT_FIXTURE_SPEAKER_NAMES_MAIN;
  const mountFixture = process.env.QT_FIXTURE_SPEAKER_NAMES_MOUNT;
  if (!mainFixture || !existsSync(mainFixture) || !mountFixture || !existsSync(mountFixture)) {
    throw new Error(
      'QT_FIXTURE_SPEAKER_NAMES_MAIN and QT_FIXTURE_SPEAKER_NAMES_MOUNT must point at the fixtures from build-speaker-names-fixture.ts'
    );
  }

  const scratch = mkdtempSync(join(tmpdir(), 'qt-speaker-names-oracle-'));
  mkdirSync(join(scratch, 'data'), { recursive: true });
  const mainWork = join(scratch, 'speaker-names-main-work.db');
  const mountWork = join(scratch, 'speaker-names-mount-work.db');
  copyFileSync(mainFixture, mainWork);
  copyFileSync(mountFixture, mountWork);

  process.env.ENCRYPTION_MASTER_PEPPER = spec.testPepperBase64;
  process.env.SQLITE_PATH = mainWork;
  process.env.SQLITE_MOUNT_INDEX_PATH = mountWork;
  process.env.QUILLTAP_DATA_DIR = scratch;
  delete process.env.SQLITE_WAL_MODE;
  process.env.LOG_LEVEL = 'error';

  const { initializeDatabase, closeDatabase, rawQuery } = await import(
    '@/lib/database/manager'
  );
  const { getRepositories } = await import('@/lib/repositories/factory');
  const { resolveSpeakerNames, speakerLabel } = await import('@/lib/chat/speaker-names');

  await initializeDatabase();
  const repos = getRepositories();

  // Record every raw character read the resolver makes.
  const reads: string[] = [];
  const realFindByIdRaw = repos.characters.findByIdRaw.bind(repos.characters);
  repos.characters.findByIdRaw = (async (id: string) => {
    reads.push(id);
    return realFindByIdRaw(id);
  }) as typeof repos.characters.findByIdRaw;

  const cases: unknown[] = [];
  for (const c of spec.chats) {
    // `read: repo` — v4's real chat read. `read: raw` — the planted chat, which
    // `findById` refuses (Zod: a seat's `characterId` is a required uuid), so
    // it is read from the raw `participants` column and handed over as the
    // resolver's own `{ participants }` contract. The refusal itself is pinned.
    let chat: { participants: Array<{ id: string; characterId?: unknown }> };
    let repoReadReturned: boolean;
    if (c.read === 'repo') {
      const found = await repos.chats.findById(c.id);
      if (!found) throw new Error(`chat ${c.name} (${c.id}) not found`);
      chat = found;
      repoReadReturned = true;
    } else {
      repoReadReturned = (await repos.chats.findById(c.id)) !== null;
      const rows = (await rawQuery('SELECT participants FROM chats WHERE id = ?', [c.id])) as Array<{
        participants: string;
      }>;
      if (rows.length !== 1) throw new Error(`chat ${c.name} (${c.id}): raw row not found`);
      chat = { participants: JSON.parse(rows[0].participants) };
    }
    const seats = chat.participants.map((p) => ({
      id: p.id,
      characterId: (p as { characterId?: unknown }).characterId ?? null,
    }));

    reads.length = 0;
    const names = await resolveSpeakerNames(chat as never);
    const caseReads = [...reads];

    const labelInputs: LabelSpec[] = [...spec.labels];
    for (const p of chat.participants) {
      labelInputs.push({ participantId: p.id, role: 'USER' });
      labelInputs.push({ participantId: p.id, role: 'ASSISTANT' });
    }
    const labels = labelInputs.map((l) => ({
      participantId: l.participantId,
      role: l.role,
      label: speakerLabel(
        { participantId: l.participantId, role: l.role } as never,
        names
      ),
    }));

    cases.push({
      name: c.name,
      chatId: c.id,
      read: c.read,
      repoReadReturned,
      seats,
      names: [...names.entries()],
      reads: caseReads,
      labels,
    });
  }

  // Probes: what v4's REAL (unwrapped) `findByIdRaw` answers for every spec
  // character plus the missing id — so the resolver's input is pinned too
  // (above all the planted empty name: `found` with `name: ""`, or a throw).
  const probeIds = [...spec.characters.map((ch) => ch.id), spec.missingCharacterId];
  const probes: unknown[] = [];
  for (const id of probeIds) {
    try {
      const row = await realFindByIdRaw(id);
      probes.push({ id, outcome: row ? 'found' : 'none', name: row ? row.name : null });
    } catch (err) {
      probes.push({ id, outcome: 'threw', name: null });
    }
  }

  await closeDatabase();
  process.stdout.write(JSON.stringify({ case: 'speaker-names', probes, cases }) + '\n');
  process.exit(0);
}

main().catch((err) => {
  process.stderr.write(`speaker-names oracle failed: ${err?.stack ?? err}\n`);
  process.exit(1);
});
