/**
 * The committed in-chat Post Office / Announcer server-surface fixture builder
 * (P4.9E2A deliverable #5) — the shared test-pepper substrate for
 * `post_office_routes_equivalence` (tier 2) and `announcer_tier3_equivalence`
 * (tier 3).
 *
 * Bakes, via v4's REAL repositories + the REAL Post Office delivery helpers (all
 * ids/timestamps pinned so both differential sides read identical rows — reads
 * diff with no remap; only the mutating cases mint fresh ids, blanked by the
 * harness). Modeled on `build-courier-images-web-fixture.ts`:
 *
 *   1. The single user (FIXTURE_USER) + an api key + TWO connection profiles
 *      (CONN is the announcement-preview subject profile; CONN_B exists so the
 *      "wrong profile" arm has a second real row to point at).
 *   2. Five characters, every vault provisioned by v4's own `create` (which owns
 *      `ensureCharacterVault`):
 *        - ARIA  — `controlledBy: 'user'`, the operator's player-character:
 *                  send-mail FROM, mailbox subject, the seeded letter's
 *                  RECIPIENT (so "In reply to" returns rows).
 *        - BEA   — `controlledBy: 'user'`, a SECOND player-character (the chat
 *                  needs at least two): send-mail TO, and the seeded letter's
 *                  sender.
 *        - CLIO  — `controlledBy: 'llm'`, an LLM participant: the send-mail 400
 *                  and mailbox 403 rejection arms.
 *        - DORIAN— NOT a participant: the announcement `character` sender and
 *                  the announcement-preview subject (v4's rewriter is for an
 *                  OFF-SCENE character).
 *        - ELISE — `controlledBy: 'user'`, then UNLINKED from its vault via the
 *                  real `characters.update` (see below): the mailbox GET's
 *                  vault-write arm.
 *   3. CHAT — one salon chat with ARIA(user) + BEA(user) + CLIO(llm) +
 *      ELISE(user) + GONE(user, `removedAt` set) + GHOST(user, pointing at a
 *      character id that does not exist) participants, so every predicate of
 *      `findOperatorPlayedParticipant` has a live counterexample AND the
 *      `notFound('Character')` / `notFound('Sender character')` arms — which sit
 *      BEHIND the auth gate — are reachable at all.
 *   4. One delivered letter BEA → ARIA at the pinned `seededLetterSentAt`, via
 *      v4's REAL `deliverLetter` (pinned `sentAt` rather than a frozen clock, so
 *      the filename's epoch prefix is deterministic).
 *
 * ## Why ELISE is unlinked with a real repo call
 *
 * v4's `characters.create` owns the full vault operation, so there is no way to
 * create an unprovisioned character. `characters.update(id, {
 * characterDocumentMountPointId: null })` is a normal repo write against a normal
 * DB column — it leaves the vault's mount point and documents in place but breaks
 * the character's FK. That is exactly the state `ensureCharacterVault`'s ADOPT
 * branch exists for (`character-vault.ts:115-131`: one populated same-name
 * character vault → re-link, `created:false, adopted:true`), so the mailbox GET
 * performs a REAL write on a READ path with **no minted ids** — the write is
 * diffable without any remap machinery.
 *
 * The minted char-vault ids + the seeded letter's delivered path are echoed to a
 * JSON sidecar (QT_FIXTURE_PO_MAIN + '.meta.json') the oracle case + the Rust
 * harness both read.
 *
 * Regenerate (Node 24, from the v4 checkout) + the committed .db files land in
 * place:
 *   N=~/.nvm/versions/node/v24.13.1/bin
 *   W=<this worktree>
 *   cd ~/source/quilltap-server
 *   TZ=UTC \
 *   QT_FIXTURE_PO_MAIN=$W/crates/quilltap-web/tests/fixtures/post-office-main.db \
 *   QT_FIXTURE_PO_MOUNT=$W/crates/quilltap-web/tests/fixtures/post-office-mount.db \
 *     $N/node --import tsx $W/harness/oracle/fixtures/build-post-office-fixture.ts
 */

import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import { mkdtempSync, mkdirSync, readFileSync, writeFileSync, rmSync, existsSync } from 'node:fs';
import { tmpdir } from 'node:os';

interface Spec {
  testPepperBase64: string;
  userId: string;
  seedTimestamp: string;
  seededLetterSentAt: string;
}

// ── Pinned entity ids (shared verbatim with the oracle case + the Rust tests) ──
const APIKEY = 'a0000001-0000-4000-8000-000000000001';
const CONN = 'c0000001-0000-4000-8000-000000000001';
const CONN_B = 'c0000002-0000-4000-8000-000000000002';

const ARIA = 'a1000000-0000-4000-8000-000000000001'; // player-character (mailbox subject)
const BEA = 'a1000000-0000-4000-8000-000000000002'; // second player-character
const CLIO = 'a1000000-0000-4000-8000-000000000003'; // llm participant (rejection arms)
const DORIAN = 'a1000000-0000-4000-8000-000000000004'; // NOT in the chat (announcer subject)
const ELISE = 'a1000000-0000-4000-8000-000000000005'; // unlinked vault (the adopt arm)
const GONE = 'a1000000-0000-4000-8000-000000000006'; // removed participant

const CHAT = 'c1000000-0000-4000-8000-000000000001';
const P_ARIA = 'e1000000-0000-4000-8000-000000000001';
const P_BEA = 'e1000000-0000-4000-8000-000000000002';
const P_CLIO = 'e1000000-0000-4000-8000-000000000003';
const P_ELISE = 'e1000000-0000-4000-8000-000000000004';
const P_GONE = 'e1000000-0000-4000-8000-000000000005';
// A played participant whose CHARACTER ROW does not exist — the only way to reach
// `notFound('Character')` / `notFound('Sender character')` past the auth gate.
const P_GHOST = 'e1000000-0000-4000-8000-000000000006';
const GHOST_CHARACTER = '99999999-9999-4999-8999-999999999999';
const MSG_SEED = 'd1000000-0000-4000-8000-000000000001';

async function main(): Promise<void> {
  // Kept in step with the oracle case (which formats dates): build under TZ=UTC.
  const offset = new Date().getTimezoneOffset();
  if (offset !== 0) {
    throw new Error(`post-office fixture must be built under TZ=UTC (getTimezoneOffset=${offset})`);
  }
  const here = dirname(fileURLToPath(import.meta.url));
  const spec = JSON.parse(readFileSync(join(here, 'post-office-web.json'), 'utf8')) as Spec;
  const TS = spec.seedTimestamp;

  const mainOut = process.env.QT_FIXTURE_PO_MAIN;
  const mountOut = process.env.QT_FIXTURE_PO_MOUNT;
  if (!mainOut || !mountOut) {
    throw new Error('QT_FIXTURE_PO_MAIN and QT_FIXTURE_PO_MOUNT must point at the .db files');
  }
  for (const out of [mainOut, mountOut]) {
    for (const suffix of ['', '-journal', '-wal', '-shm']) {
      const p = out + suffix;
      if (existsSync(p)) rmSync(p);
    }
  }
  if (existsSync(mainOut + '.meta.json')) rmSync(mainOut + '.meta.json');

  const scratch = mkdtempSync(join(tmpdir(), 'qt-po-fixture-'));
  mkdirSync(join(scratch, 'data'), { recursive: true });

  process.env.ENCRYPTION_MASTER_PEPPER = spec.testPepperBase64;
  process.env.SQLITE_PATH = mainOut;
  process.env.SQLITE_MOUNT_INDEX_PATH = mountOut;
  process.env.QUILLTAP_DATA_DIR = scratch;
  delete process.env.SQLITE_WAL_MODE;
  process.env.LOG_LEVEL = 'error';

  const { initializeDatabase, ensureCollection, getCollection, closeDatabase } = await import(
    '@/lib/database/manager'
  );
  const { getRepositories } = await import('@/lib/repositories/factory');
  const { getRawMountIndexDatabase, closeMountIndexSQLiteClient } = await import(
    '@/lib/database/backends/sqlite/mount-index-client'
  );
  const { CharacterSchema, ChatMetadataSchema, FileEntrySchema } = await import(
    '@/lib/schemas/types'
  );
  const { UserSchema } = await import('@/lib/schemas/auth.types');
  const { ApiKeySchema } = await import('@/lib/schemas/profile.types');
  const { generateDDL } = await import('@/lib/database/schema-translator');
  const {
    DocMountPointSchema,
    DocMountFileSchema,
    DocMountDocumentSchema,
    DocMountFolderSchema,
    DocMountFileLinkSchema,
    DocMountChunkSchema,
    ProjectDocMountLinkSchema,
  } = await import('@/lib/schemas/mount-index.types');
  const { deliverLetter } = await import('@/lib/post-office/mailbox');

  await initializeDatabase();
  await ensureCollection('users', UserSchema);
  await ensureCollection('characters', CharacterSchema);
  await ensureCollection('chats', ChatMetadataSchema);
  await ensureCollection('files', FileEntrySchema);
  await ensureCollection('api_keys', ApiKeySchema);

  const midb = getRawMountIndexDatabase();
  if (!midb) throw new Error('mount-index DB handle unavailable');
  const ddl: Array<[string, unknown]> = [
    ['doc_mount_points', DocMountPointSchema],
    ['doc_mount_files', DocMountFileSchema],
    ['doc_mount_documents', DocMountDocumentSchema],
    ['doc_mount_folders', DocMountFolderSchema],
    ['doc_mount_file_links', DocMountFileLinkSchema],
    ['doc_mount_chunks', DocMountChunkSchema],
    ['project_doc_mount_links', ProjectDocMountLinkSchema],
  ];
  for (const [name, schema] of ddl) {
    for (const sql of generateDDL(name, schema as never)) midb.exec(sql);
  }
  const repos = getRepositories();
  // doc_mount_blobs has hand-written DDL (BLOB column) — trigger the CREATE via a read.
  await repos.docMountBlobs.listByMountPoint('00000000-0000-4000-8000-000000000000');

  // 1. User + api key + two connection profiles.
  await repos.users.create(
    { username: 'friday', email: null, name: 'Friday' } as never,
    { id: spec.userId, createdAt: TS, updatedAt: TS } as never,
  );
  const apiKeyCol = await getCollection('api_keys');
  await apiKeyCol.insertOne(
    ApiKeySchema.parse({
      id: APIKEY,
      userId: spec.userId,
      label: 'mock-key',
      provider: 'OPENAI',
      key_value: 'sk-synthetic-mock-key',
      isActive: true,
      lastUsed: null,
      createdAt: TS,
      updatedAt: TS,
    }) as never,
  );
  await repos.connections.create(
    {
      userId: spec.userId,
      name: 'Local Mock',
      provider: 'OPENAI_COMPATIBLE',
      modelName: 'mock-model',
      baseUrl: 'http://127.0.0.1:1/v1',
      apiKeyId: APIKEY,
    } as never,
    { id: CONN, createdAt: TS, updatedAt: TS } as never,
  );
  await repos.connections.create(
    { userId: spec.userId, name: 'Second Desk', provider: 'OPENAI', modelName: 'gpt-4o-mini' } as never,
    { id: CONN_B, createdAt: TS, updatedAt: TS } as never,
  );

  // 2. Characters (create owns vault provisioning).
  const mkChar = async (id: string, name: string, controlledBy: string, extra: object = {}) => {
    await repos.characters.create(
      { name, userId: spec.userId, controlledBy, ...extra } as never,
      { id, createdAt: TS, updatedAt: TS } as never,
    );
    const raw = await repos.characters.findByIdRaw(id);
    const vault = raw?.characterDocumentMountPointId as string | null;
    if (!vault) throw new Error(`vault not minted for ${name}`);
    return vault;
  };
  const ariaVault = await mkChar(ARIA, 'Aria', 'user', {
    description: 'The operator’s letter-writing persona.',
    personality: 'Precise, wry, and fond of a well-turned postscript.',
  });
  const beaVault = await mkChar(BEA, 'Bea', 'user');
  const clioVault = await mkChar(CLIO, 'Clio', 'llm');
  const dorianVault = await mkChar(DORIAN, 'Dorian', 'llm', {
    description: 'A lamplighter who keeps to the far side of the river.',
    personality: 'Formal to a fault; speaks in short, weighted sentences.',
    identity: 'Dorian, night lamplighter of the eastern quays.',
  });
  const eliseVault = await mkChar(ELISE, 'Elise', 'user');
  const goneVault = await mkChar(GONE, 'Gone', 'user');

  // 3. The chat. Every `findOperatorPlayedParticipant` predicate gets a live
  //    counterexample: CLIO is `controlledBy:'llm'`, GONE carries `removedAt`.
  const mkParticipant = (
    id: string,
    characterId: string,
    controlledBy: string,
    extra: object = {},
  ) => ({
    id,
    type: 'CHARACTER',
    characterId,
    controlledBy,
    status: 'active',
    connectionProfileId: null,
    createdAt: TS,
    updatedAt: TS,
    ...extra,
  });
  await repos.chats.create(
    {
      userId: spec.userId,
      title: 'The Post Office',
      chatType: 'salon',
      contextSummary: null,
      participants: [
        mkParticipant(P_ARIA, ARIA, 'user'),
        mkParticipant(P_BEA, BEA, 'user'),
        mkParticipant(P_CLIO, CLIO, 'llm'),
        mkParticipant(P_ELISE, ELISE, 'user'),
        mkParticipant(P_GONE, GONE, 'user', { status: 'removed', removedAt: TS }),
        mkParticipant(P_GHOST, GHOST_CHARACTER, 'user'),
      ],
      tags: [],
    } as never,
    { id: CHAT, createdAt: TS, updatedAt: TS } as never,
  );
  await repos.chats.addMessages(CHAT, [
    {
      id: MSG_SEED,
      type: 'message',
      role: 'USER',
      content: 'The evening post is late again.',
      participantId: P_ARIA,
      createdAt: '2026-05-02T00:00:00.000Z',
      attachments: [],
    },
  ] as never);
  await repos.chats.update(CHAT, { updatedAt: TS } as never);

  // 4. One delivered letter, BEA → ARIA, at the pinned sentAt (v4's REAL
  //    deliverLetter — pinned rather than clock-frozen so the filename's epoch
  //    prefix is deterministic). This is the "In reply to" row the mailbox list
  //    returns and the send-mail reply case quotes.
  const { path: seededLetterPath } = await deliverLetter({
    recipientVaultId: ariaVault,
    fromName: 'Bea',
    fromCharacterId: BEA,
    sentAt: spec.seededLetterSentAt,
    body: 'Aria — the eastern quay lamps went dark at eleven. Ask Dorian why.\n\nYours, Bea',
    inReplyTo: null,
  });

  // 5. ELISE loses her vault FK (a plain column write through the real repo).
  //    The vault mount point + its documents stay, so the mailbox GET's
  //    `ensureCharacterVault` takes the ADOPT branch — a real write on a read
  //    path, with no minted ids.
  await repos.characters.update(ELISE, { characterDocumentMountPointId: null } as never);
  const eliseAfter = await repos.characters.findByIdRaw(ELISE);
  if (eliseAfter?.characterDocumentMountPointId) {
    throw new Error('Elise vault FK was not cleared');
  }

  // Materialize the empty background-jobs table so both sides' fresh copies read
  // identical schemas.
  await repos.backgroundJobs.findByUserId(spec.userId, 'PENDING');

  closeMountIndexSQLiteClient();
  await closeDatabase();

  const meta = {
    ariaVault,
    beaVault,
    clioVault,
    dorianVault,
    eliseVault,
    goneVault,
    seededLetterPath,
  };
  writeFileSync(mainOut + '.meta.json', JSON.stringify(meta));
  process.stderr.write(
    `built post-office fixture: main=${mainOut} mount=${mountOut} ` +
      `ariaVault=${ariaVault} eliseVault=${eliseVault} letter=${seededLetterPath}\n`,
  );
  process.exit(0);
}

main().catch((err) => {
  process.stderr.write(`post-office fixture build failed: ${err?.stack ?? err}\n`);
  process.exit(1);
});
