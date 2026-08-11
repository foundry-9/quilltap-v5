import { spawnSync } from 'node:child_process';

import { E2E_PASSPHRASE, INSTANCE_DIR, SINGLE_USER_ID } from './env';

/**
 * The P4.D64 wire: seed a tombstoned character — plus a group and a chat that
 * hold it — into the SHARED e2e instance, so the archive's READ surfaces
 * (roster toggle / badges / read-only detail / group members line / participant
 * badge) have something real to walk. The archive ACTIONS are round 2's; nothing
 * here needs them.
 *
 * ## Why a NEW character, and not a fixture one
 *
 * The order's seeding note suggests archiving Dax as "the least-entangled
 * fixture character". A survey at lane start says otherwise: Dax appears in
 * SEVEN places in `salon-post-office-flow.spec.ts` (the off-scene recipient, the
 * mail author, the not-borrowed-from-the-cast guard), and Aria / Bram / Cleo are
 * that spec's cast. Archiving ANY of the four removes them from every picker —
 * v4's list chokepoint excludes archived characters by default — so a sibling
 * spec would go red for a reason that has nothing to do with it. This seeds
 * `Marchpane` instead: a character no beat has ever mentioned.
 *
 * ## Why the rows are COPIED rather than written out
 *
 * `characters` and `chats` have wide, moving schemas (P4.D63 is adding three
 * columns to `characters` in this very round). A hand-written INSERT would rot.
 * So each row is read back with `SELECT *` (`--json`), then re-inserted with a
 * handful of keys overridden — which picks up new columns for free and cannot
 * drift from the schema the instance actually has.
 *
 * ## Why it is INERT until P4.D63 lands
 *
 * `characters.archivedAt` does not exist until P4.D63's schema re-dump + boot
 * ensure. The seeder PROBES for the column and returns false when it is absent,
 * seeding nothing — because a Marchpane who exists but is not archived would
 * appear in the roster and in every picker, which is exactly the sibling-spec
 * breakage described above. The e2e specs gate their tombstone beats on
 * `ARCHIVE_TOMBSTONE_SEEDED`, which the unifier flips once this returns true.
 *
 * Everything it writes is additive and keyed `d64…` — well outside every
 * fixture family's id scheme — and nothing existing is mutated.
 */

/** Ids are e2e-only (`d64…`), so they cannot collide with a fixture family. */
const CHARACTER_ID = 'd6400000-0000-4000-8000-0000000000c1';
const CHAT_ID = 'd6400000-0000-4000-8000-0000000000a1';
const GROUP_ID = 'd6400000-0000-4000-8000-0000000000b1';
const MEMBER_ARCHIVED = 'd6400000-0000-4000-8000-0000000000e1';
const MEMBER_LIVE = 'd6400000-0000-4000-8000-0000000000e2';
const PARTICIPANT_ID = 'd6400000-0000-4000-8000-0000000000f1';

/** The names the beats locate by — deliberately unlike anything else seeded. */
export const ARCHIVED_CHARACTER_NAME = 'Marchpane';
export const ARCHIVED_GROUP_NAME = 'The Dust Sheets';
export const ARCHIVED_CHAT_TITLE = 'The Shuttered Wing';
/** The tombstone stamp, fixed so a badge tooltip is predictable. */
export const ARCHIVED_AT = '2026-08-01T00:00:00.000Z';

const TS = '2026-08-01T00:00:00.000Z';

function run(
  cli: string,
  statement: string,
  opts: { mount?: boolean; json?: boolean; allowFail?: boolean } = {},
): { ok: boolean; stdout: string } {
  const args = [
    'db',
    '--data-dir',
    INSTANCE_DIR,
    ...(opts.mount ? ['--mount-points'] : []),
    ...(opts.json ? ['--json'] : []),
    ...(opts.json ? [] : ['--write']),
    statement,
  ];
  const res = spawnSync(cli, args, {
    env: { ...process.env, QUILLTAP_DB_PASSPHRASE: E2E_PASSPHRASE, QUILLTAP_QUIET_HINTS: '1' },
    encoding: 'utf8',
    maxBuffer: 32 * 1024 * 1024,
  });
  if (res.status !== 0 && !opts.allowFail) {
    throw new Error(`archive-fixture failed (${statement}):\n${res.stdout}\n${res.stderr}`);
  }
  return { ok: res.status === 0, stdout: res.stdout };
}

function read(cli: string, statement: string, mount = false): Array<Record<string, unknown>> {
  const { stdout } = run(cli, statement, { json: true, mount });
  return JSON.parse(stdout) as Array<Record<string, unknown>>;
}

function sqlLiteral(value: unknown): string {
  if (value === null || value === undefined) return 'NULL';
  if (typeof value === 'number') return String(value);
  if (typeof value === 'boolean') return value ? '1' : '0';
  return `'${String(value).replace(/'/g, "''")}'`;
}

/** Re-insert a `SELECT *` row with some columns overridden (schema-agnostic). */
function insertCopy(
  cli: string,
  table: string,
  row: Record<string, unknown>,
  overrides: Record<string, unknown>,
  mount = false,
): void {
  const merged = { ...row, ...overrides };
  const columns = Object.keys(merged)
    .map((c) => `"${c}"`)
    .join(', ');
  const values = Object.keys(merged)
    .map((c) => sqlLiteral(merged[c]))
    .join(', ');
  run(cli, `INSERT INTO "${table}" (${columns}) VALUES (${values})`, { mount });
}

/**
 * Seed the archive island. Returns false (having written nothing) when the
 * `characters.archivedAt` column is absent — i.e. before P4.D63 lands.
 */
export function seedArchivedCharacter(cli: string): boolean {
  // The probe. A read of the column itself, so this is a fact about the
  // instance rather than a guess about which lanes have landed.
  const probe = run(cli, 'SELECT archivedAt FROM characters LIMIT 1', {
    json: true,
    allowFail: true,
  });
  if (!probe.ok) {
    return false;
  }

  // Idempotence: a re-run against an already-seeded instance is a no-op.
  const existing = read(
    cli,
    `SELECT id FROM characters WHERE id = ${sqlLiteral(CHARACTER_ID)}`,
  );
  if (existing.length > 0) {
    return true;
  }

  // --- The character: a copy of an existing row, renamed and tombstoned.
  const template = read(cli, 'SELECT * FROM characters ORDER BY createdAt LIMIT 1');
  if (template.length === 0) {
    throw new Error('archive-fixture: the shared instance has no character to copy');
  }
  insertCopy(cli, 'characters', template[0], {
    id: CHARACTER_ID,
    userId: SINGLE_USER_ID,
    name: ARCHIVED_CHARACTER_NAME,
    title: 'Keeper of the Shuttered Wing',
    description: 'A cataloguer of things put away.',
    isFavorite: 0,
    npc: 0,
    // The tombstone. `archiveFileId` stays NULL — a pre-bundle tombstone, which
    // is v4-legal and keeps this seeder clear of the ARCHIVE files row (whose
    // bytes are round 2's business).
    archivedAt: ARCHIVED_AT,
    archiveFileId: null,
    createdAt: TS,
    updatedAt: TS,
  });

  // --- The group: two members, one of them the tombstone, so the Members card
  // reads "2 members / 1 can speak (1 archived)".
  const live = read(cli, `SELECT id FROM characters WHERE id <> ${sqlLiteral(CHARACTER_ID)} LIMIT 1`);
  run(
    cli,
    `INSERT INTO "groups" ("id", "name", "description", "createdAt", "updatedAt") VALUES (` +
      `${sqlLiteral(GROUP_ID)}, ${sqlLiteral(ARCHIVED_GROUP_NAME)}, ` +
      `'Whatever has been tucked away.', ${sqlLiteral(TS)}, ${sqlLiteral(TS)})`,
  );
  const members: Array<[string, string]> = [[MEMBER_ARCHIVED, CHARACTER_ID]];
  if (live[0]?.['id']) {
    members.push([MEMBER_LIVE, String(live[0]['id'])]);
  }
  for (const [id, characterId] of members) {
    run(
      cli,
      `INSERT INTO "group_character_members" ("id", "groupId", "characterId", "createdAt", "updatedAt") ` +
        `VALUES (${sqlLiteral(id)}, ${sqlLiteral(GROUP_ID)}, ${sqlLiteral(characterId)}, ` +
        `${sqlLiteral(TS)}, ${sqlLiteral(TS)})`,
      { mount: true },
    );
  }

  // --- The chat: its own conversation (never an existing one — the post-office
  // beats assert exact cast option lists), holding the live seat from its
  // template plus the tombstone as an ABSENT participant, which is what an
  // archived seat looks like in practice and lets the card show BOTH badges.
  const chatTemplate = read(cli, 'SELECT * FROM chats ORDER BY createdAt LIMIT 1');
  if (chatTemplate.length === 0) {
    throw new Error('archive-fixture: the shared instance has no chat to copy');
  }
  const parsed = JSON.parse(String(chatTemplate[0]['participants'] ?? '[]')) as Array<
    Record<string, unknown>
  >;
  const seat = parsed[0];
  if (!seat) {
    throw new Error('archive-fixture: the template chat has no participant to model');
  }
  const participants = [
    { ...seat, displayOrder: 0 },
    {
      ...seat,
      id: PARTICIPANT_ID,
      characterId: CHARACTER_ID,
      displayOrder: 1,
      isActive: true,
      status: 'absent',
    },
  ];
  insertCopy(cli, 'chats', chatTemplate[0], {
    id: CHAT_ID,
    userId: SINGLE_USER_ID,
    title: ARCHIVED_CHAT_TITLE,
    participants: JSON.stringify(participants),
    messageCount: 0,
    lastMessageAt: null,
    createdAt: TS,
    updatedAt: TS,
  });

  return true;
}
