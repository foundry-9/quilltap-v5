/**
 * Builds the committed Pascal `run_custom` handler fixture (main + mount-index)
 * via v4's REAL repos + `storeMountFile`, so BOTH the jest handler oracle
 * (`pascal-run-custom-handler.test.ts`) and the Rust handler differential
 * (`pascal_run_custom_handler_equivalence.rs`) drive the same baked instance.
 *
 * Three characters, each with a provisioned vault carrying a `Tools/` roster and
 * a `metadata.json` fact sheet:
 *   - CHAR_A: ansible (metadata-gated) + coin (public) + whispered (private) tools;
 *             metadata { hasAnsibleAccess: true, clearanceLevel: 3 }.
 *   - CHAR_B: ansible; metadata { faction: "Ordo Ferrum" } (lacks the key).
 *   - CHAR_C: ansible; metadata {} (empty sheet).
 *   - CHAR_D (P4.d19): a provisioned vault with its `properties.json` keystone
 *             DELETED, so `findById` raises the vault-unavailable error.
 * The General store carries the two AVAILABILITY-GATED definitions (P4.d19).
 * One salon chat CH with all three characters + a user participant, so
 * `postPascalResult`/`postProsperoCustomToolError` find the chat.
 *
 * The rolls are all `min === max` (no draw) so the outcome is deterministic on
 * both sides without a shared byte source. The minted char-vault ids are written
 * to the `.meta.json` sidecar (the Rust side reads them to pass
 * `character_mount_point_id`).
 *
 * Regenerate (v4 @ 231be14c, Node 24; the pinned detached worktree):
 *   cd /tmp/qt-v4-pin-231be14c
 *   QT_FIXTURE_CI_MAIN=<V5W>/crates/quilltap-web/tests/fixtures/pascal-run-custom-main.db \
 *   QT_FIXTURE_CI_MOUNT=<V5W>/crates/quilltap-web/tests/fixtures/pascal-run-custom-mount.db \
 *     npx tsx <V5W>/harness/oracle/fixtures/build-pascal-run-custom-fixture.ts
 */

import { readFileSync, writeFileSync, existsSync, rmSync, mkdtempSync, mkdirSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import { tmpdir } from 'node:os';

interface Spec {
  testPepperBase64: string;
  userId: string;
  seedTimestamp: string;
}

const CHAR_A = 'a1000000-0000-4000-8000-00000000000a';
const CHAR_B = 'a1000000-0000-4000-8000-00000000000b';
const CHAR_C = 'a1000000-0000-4000-8000-00000000000c';
/// P4.d19: a character whose vault is BROKEN (its `properties.json` keystone is
/// deleted after provisioning), so `findById` raises the vault-unavailable
/// error the run_custom handler now meets BEFORE it resolves a roster.
const CHAR_D = 'a1000000-0000-4000-8000-00000000000d';

const CHAT = 'c1000000-0000-4000-8000-000000000001';
const GROUP = 'a2000000-0000-4000-8000-0000000000aa';
const GENERAL_MP = '93000000-0000-4000-8000-0000000000aa';
const P_A = 'e1000000-0000-4000-8000-00000000000a';
const P_B = 'e1000000-0000-4000-8000-00000000000b';
const P_C = 'e1000000-0000-4000-8000-00000000000c';
const P_USER = 'e1000000-0000-4000-8000-0000000000ff';

const ANSIBLE = {
  name: 'ansible',
  description: 'Reach for the ansible.',
  roll: { min: 0.7, max: 0.7 },
  outcomes: [
    {
      when: { gt: 0.6, metadata: { hasAnsibleAccess: { eq: true } } },
      message: 'The ansible flickers to life.',
      state: 'success',
    },
    { when: true, message: 'The panel stays dark.', state: 'failure' },
  ],
};

const COIN = {
  name: 'coin',
  title: 'Flip A Coin',
  description: 'Flip a coin.',
  roll: { min: 0.7, max: 0.7 },
  outcomes: [{ when: true, message: 'Heads.', state: 'success' }],
};

const WHISPERED = {
  name: 'whispered',
  description: 'A private aside.',
  defaultVisibility: 'whisper',
  roll: { min: 0.5, max: 0.5 },
  outcomes: [{ when: true, message: 'psst — for you alone.', state: 'info' }],
};

/**
 * The 616930db consult tool. This fixture carries NO connection profiles, which
 * is deliberate: the consult therefore takes the `profiles.length === 0` arm on
 * BOTH sides and fails soft with a fixed reason, so the whole seam — invoker
 * construction, the fresh resolution, the failure→errorMessage translation, the
 * `pascalMeta.llm` record, and the table branching on it — is exercised
 * end-to-end without mocking a provider or spending a call.
 *
 * The table branches on `ok`, so the outcome index itself proves the consult
 * ran and reported failure rather than being skipped.
 */
/**
 * The P4.d10 `$state` tool: a $state roll bound (min == max → deterministic, no
 * draw), a $state comparator operand, and `{{state.path}}` templates. The
 * cascade for CHAR_A merges chat {difficulty:6, banner:'chat'} over group
 * {gscore:4} over general {gen_tier:true, difficulty:9} → roll 6, 6 >= 4 →
 * success. CHAR_B has no group, so gscore falls back to 99 → failure — the
 * per-entrance character scoping made visible in the outcome index.
 */
const STATEFUL = {
  name: 'stateful',
  description: 'Roll against the table stakes.',
  roll: {
    min: { $state: 'difficulty', fallback: 5 },
    max: { $state: 'difficulty', fallback: 5 },
  },
  outcomes: [
    {
      when: { gte: { $state: 'gscore', fallback: 99 } },
      message: 'Cleared the stakes ({{state.banner}}, g {{state.gscore}}, gen {{state.gen_tier}}).',
      state: 'success',
    },
    { when: true, message: 'Short of the stakes.', state: 'failure' },
  ],
};

const ORACLE = {
  name: 'oracle',
  title: 'Consult The Oracle',
  description: 'Ask the oracle.',
  roll: { min: 0.7, max: 0.7 },
  llm: {
    prompt: 'The draw was {{value}}. Answer YES or NO: is it auspicious?',
    errorMessage: 'The wire went dead, and the oracle with it.',
    maxOutput: 120,
  },
  outcomes: [
    { when: { llm: { ok: true, eq: 'YES' } }, message: 'The oracle says {{llm}}.', state: 'success' },
    { when: { llm: { ok: false } }, message: 'Silence: {{llm}}', state: 'failure' },
    { when: true, message: 'The oracle demurs.', state: 'info' },
  ],
};

/**
 * P4.d19 (v4 `6864bf0e`): the two AVAILABILITY-GATED tools, in the GENERAL store
 * so every character sees the same file and only their fact sheet decides.
 *
 * `secure_line` gates POSITIVELY: only CHAR_A's `clearanceLevel: 3` qualifies —
 * CHAR_B lacks the key and CHAR_C's sheet is empty, and both fail CLOSED.
 * `novice_aid` gates NEGATIVELY over the same three sheets, and lands the other
 * way: only CHAR_B's `faction` matches, so A and C (neither of whom has the key)
 * are OFFERED it. One sheet, two clauses, opposite answers — the asymmetry that
 * is the whole reason both keys exist.
 */
const SECURE_LINE = {
  name: 'secure_line',
  description: 'Open the secure line.',
  availableWhen: { metadata: { clearanceLevel: { gte: 3 } } },
  roll: { min: 0.7, max: 0.7 },
  outcomes: [{ when: true, message: 'The line hums, scrambled.', state: 'success' }],
};

const NOVICE_AID = {
  name: 'novice_aid',
  description: 'Ask the house for a hint.',
  defaultVisibility: 'whisper',
  withheldWhen: { metadata: { faction: { contains: 'Ferrum' } } },
  roll: { min: 0.5, max: 0.5 },
  outcomes: [{ when: true, message: 'The house obliges, quietly.', state: 'info' }],
};

async function main(): Promise<void> {
  const here = dirname(fileURLToPath(import.meta.url));
  const spec = JSON.parse(readFileSync(join(here, 'pascal-run-custom.json'), 'utf8')) as Spec;
  const TS = spec.seedTimestamp;

  const mainOut = process.env.QT_FIXTURE_CI_MAIN;
  const mountOut = process.env.QT_FIXTURE_CI_MOUNT;
  if (!mainOut || !mountOut) {
    throw new Error('QT_FIXTURE_CI_MAIN and QT_FIXTURE_CI_MOUNT must point at the .db files');
  }
  for (const out of [mainOut, mountOut]) {
    for (const suffix of ['', '-journal', '-wal', '-shm']) {
      const p = out + suffix;
      if (existsSync(p)) rmSync(p);
    }
  }
  if (existsSync(mainOut + '.meta.json')) rmSync(mainOut + '.meta.json');

  const scratch = mkdtempSync(join(tmpdir(), 'qt-pascal-fixture-'));
  mkdirSync(join(scratch, 'data'), { recursive: true });

  process.env.ENCRYPTION_MASTER_PEPPER = spec.testPepperBase64;
  process.env.SQLITE_PATH = mainOut;
  process.env.SQLITE_MOUNT_INDEX_PATH = mountOut;
  process.env.QUILLTAP_DATA_DIR = scratch;
  delete process.env.SQLITE_WAL_MODE;
  process.env.LOG_LEVEL = 'error';

  const { initializeDatabase, ensureCollection, closeDatabase } = await import(
    '@/lib/database/manager'
  );
  const { getRepositories } = await import('@/lib/repositories/factory');
  const { getRawMountIndexDatabase, closeMountIndexSQLiteClient } = await import(
    '@/lib/database/backends/sqlite/mount-index-client'
  );
  const { CharacterSchema, ChatMetadataSchema } = await import('@/lib/schemas/types');
  const { GroupSchema } = await import('@/lib/schemas/group.types');
  const { getRawDatabase } = await import('@/lib/database/backends/sqlite/client');
  const { UserSchema } = await import('@/lib/schemas/auth.types');
  const { generateDDL } = await import('@/lib/database/schema-translator');
  const {
    DocMountPointSchema,
    DocMountFileSchema,
    DocMountDocumentSchema,
    DocMountFolderSchema,
    DocMountFileLinkSchema,
    DocMountChunkSchema,
    GroupCharacterMemberSchema,
    GroupDocMountLinkSchema,
    ProjectDocMountLinkSchema,
  } = await import('@/lib/schemas/mount-index.types');

  await initializeDatabase();
  await ensureCollection('users', UserSchema);
  await ensureCollection('characters', CharacterSchema);
  await ensureCollection('groups', GroupSchema);
  await ensureCollection('chats', ChatMetadataSchema);

  const midb = getRawMountIndexDatabase();
  if (!midb) throw new Error('mount-index DB handle unavailable');
  const ddl: Array<[string, unknown]> = [
    ['doc_mount_points', DocMountPointSchema],
    ['doc_mount_files', DocMountFileSchema],
    ['doc_mount_documents', DocMountDocumentSchema],
    ['doc_mount_folders', DocMountFolderSchema],
    ['doc_mount_file_links', DocMountFileLinkSchema],
    ['doc_mount_chunks', DocMountChunkSchema],
    ['group_character_members', GroupCharacterMemberSchema],
    ['group_doc_mount_links', GroupDocMountLinkSchema],
    ['project_doc_mount_links', ProjectDocMountLinkSchema],
  ];
  for (const [name, schema] of ddl) {
    for (const sql of generateDDL(name, schema as never)) midb.exec(sql);
  }
  const repos = getRepositories();
  await repos.docMountBlobs.listByMountPoint('00000000-0000-4000-8000-000000000000');
  // Touch the two tables the 616930db LLM consult reads, so they EXIST (empty)
  // in the fixture. v4's backend creates a collection lazily on first access,
  // so it tolerates their absence; v5 reads a provisioned schema and would fail
  // the read instead of reaching the `no connection profiles are configured`
  // arm. A real instance always provisions both, so this is the fixture
  // catching up to reality — not a divergence either side gets to keep.
  await repos.chatSettings.findByUserId(spec.userId);
  await repos.connections.findByUserId(spec.userId);

  await repos.users.create(
    { username: 'friday', email: null, name: 'Friday' } as never,
    { id: spec.userId, createdAt: TS, updatedAt: TS } as never,
  );

  const mkChar = async (id: string, name: string): Promise<string> => {
    await repos.characters.create(
      { name, userId: spec.userId, controlledBy: 'llm' } as never,
      { id, createdAt: TS, updatedAt: TS } as never,
    );
    const raw = await repos.characters.findByIdRaw(id);
    const vault = raw?.characterDocumentMountPointId as string | null;
    if (!vault) throw new Error(`character ${name} vault not minted`);
    return vault;
  };

  const vaultA = await mkChar(CHAR_A, 'Bertie');
  const vaultB = await mkChar(CHAR_B, 'Jeeves');
  const vaultC = await mkChar(CHAR_C, 'Aunt Agatha');
  const vaultD = await mkChar(CHAR_D, 'Spode');

  const { storeMountFile } = await import('@/lib/mount-index/store-file');
  const writeVaultFile = async (
    mountPointId: string,
    relativePath: string,
    body: unknown,
  ): Promise<void> => {
    await storeMountFile({
      mountPointId,
      relativePath,
      data: Buffer.from(JSON.stringify(body, null, 2), 'utf8'),
      originalMimeType: 'application/json',
      transcodeImages: false,
      extractText: false,
      enqueueEmbedding: false,
      // metadata.json is scaffold-seeded ({}) by characters.create; overwrite it.
      force: true,
    } as never);
  };

  await writeVaultFile(vaultA, 'Tools/ansible.tool.json', ANSIBLE);
  await writeVaultFile(vaultA, 'Tools/coin.tool.json', COIN);
  await writeVaultFile(vaultA, 'Tools/whispered.tool.json', WHISPERED);
  await writeVaultFile(vaultA, 'Tools/oracle.tool.json', ORACLE);
  await writeVaultFile(vaultA, 'Tools/stateful.tool.json', STATEFUL);
  await writeVaultFile(vaultA, 'metadata.json', { hasAnsibleAccess: true, clearanceLevel: 3 });

  await writeVaultFile(vaultB, 'Tools/ansible.tool.json', ANSIBLE);
  await writeVaultFile(vaultB, 'Tools/stateful.tool.json', STATEFUL);
  await writeVaultFile(vaultB, 'metadata.json', { faction: 'Ordo Ferrum' });

  await writeVaultFile(vaultC, 'Tools/ansible.tool.json', ANSIBLE);
  await writeVaultFile(vaultC, 'metadata.json', {});

  // P4.d19: break CHAR_D's vault by removing the `properties.json` keystone the
  // overlay demands, so `findById` raises the vault-unavailable error. A
  // provisioned-then-broken vault is the honest shape — a character that never
  // had one takes the `hasLinkedVault` early return and never throws.
  // The path lives on `doc_mount_file_links` (content is shared by sha256
  // through `doc_mount_files`/`doc_mount_documents`, and the scaffold's
  // properties.json is byte-identical across every fresh vault — so deleting
  // the LINK is the only delete that can be scoped to one vault).
  midb
    .prepare('DELETE FROM "doc_mount_file_links" WHERE "mountPointId" = ? AND "relativePath" = ?')
    .run(vaultD, 'properties.json');

  // P4.d10: the state-cascade seeds. A group for CHAR_A (store-backed state via
  // the overlay), the singleton "Quilltap General" mount with its root
  // state.json, and chat-tier state on the salon chat below.
  const group = await repos.groups.create(
    { name: 'Table Stakes', description: null, color: null, icon: null, state: {} } as never,
    { id: GROUP, createdAt: TS, updatedAt: TS } as never,
  );
  if ((group as { id: string }).id !== GROUP) throw new Error('group id drift');
  await repos.groups.update(GROUP, { state: { gscore: 4 } } as never);
  await repos.groupCharacterMembers.addMember(GROUP, CHAR_A);

  await repos.docMountPoints.create(
    {
      name: 'Quilltap General',
      basePath: '',
      mountType: 'database',
      storeType: 'documents',
      includePatterns: [],
      excludePatterns: [],
      enabled: true,
      lastScannedAt: null,
      scanStatus: 'idle',
      lastScanError: null,
      conversionStatus: 'idle',
      conversionError: null,
      fileCount: 0,
      chunkCount: 0,
      totalSizeBytes: 0,
    } as never,
    { id: GENERAL_MP, createdAt: TS, updatedAt: TS } as never,
  );
  const mainDbRaw = getRawDatabase();
  if (!mainDbRaw) throw new Error('main DB handle unavailable');
  mainDbRaw.exec(
    'CREATE TABLE IF NOT EXISTS "instance_settings" ("key" TEXT PRIMARY KEY, "value" TEXT NOT NULL)',
  );
  mainDbRaw
    .prepare('INSERT OR REPLACE INTO "instance_settings" ("key", "value") VALUES (?, ?)')
    .run('generalMountPointId', GENERAL_MP);
  const { writeDatabaseDocument } = await import('@/lib/mount-index/database-store');
  await writeDatabaseDocument(
    GENERAL_MP,
    'state.json',
    JSON.stringify({ gen_tier: true, difficulty: 9 }, null, 2),
  );
  // P4.d19: the two gated definitions live in the GENERAL store (the farthest
  // tier), so every character reaches the same file and only the gate decides.
  await writeVaultFile(GENERAL_MP, 'Tools/secure_line.tool.json', SECURE_LINE);
  await writeVaultFile(GENERAL_MP, 'Tools/novice_aid.tool.json', NOVICE_AID);

  const mkParticipant = (id: string, characterId: string, controlledBy: string) => ({
    id,
    type: 'CHARACTER',
    characterId,
    controlledBy,
    status: 'active',
    connectionProfileId: null,
    createdAt: TS,
    updatedAt: TS,
  });

  await repos.chats.create(
    {
      userId: spec.userId,
      title: 'Pascal Custom Tools',
      chatType: 'salon',
      contextSummary: null,
      participants: [
        mkParticipant(P_A, CHAR_A, 'llm'),
        mkParticipant(P_B, CHAR_B, 'llm'),
        mkParticipant(P_C, CHAR_C, 'llm'),
        mkParticipant(P_USER, CHAR_A, 'user'),
      ],
      // P4.d10: the chat tier of the cascade (narrowest — wins on collision).
      state: { difficulty: 6, banner: 'chat' },
      tags: [],
    } as never,
    { id: CHAT, createdAt: TS, updatedAt: TS } as never,
  );
  // One pinned seed message so the `chat_messages` table exists in the committed
  // fixture (v4 creates it lazily on first insert; without it v5's post would
  // find no table). It is a plain USER row — `systemSender` null — so the
  // handler differential's positional remap of the posted system rows ignores it.
  await repos.chats.addMessages(CHAT, [
    {
      id: 'd1000000-0000-4000-8000-000000000001',
      type: 'message',
      role: 'USER',
      content: 'Shall we begin?',
      participantId: P_USER,
      createdAt: TS,
      attachments: [],
    },
  ] as never);
  await repos.chats.update(CHAT, { updatedAt: TS } as never);

  closeMountIndexSQLiteClient();
  await closeDatabase();

  const meta = { vaultA, vaultB, vaultC, vaultD };
  writeFileSync(mainOut + '.meta.json', JSON.stringify(meta));
  process.stderr.write(
    `built pascal-run-custom fixture: main=${mainOut} mount=${mountOut} ` +
      `vaultA=${vaultA} vaultB=${vaultB} vaultC=${vaultC} vaultD=${vaultD}\n`,
  );
  process.exit(0);
}

main().catch((err) => {
  process.stderr.write(`pascal-run-custom fixture build failed: ${err?.stack ?? err}\n`);
  process.exit(1);
});
