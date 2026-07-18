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
 * One salon chat CH with all three characters + a user participant, so
 * `postPascalResult`/`postProsperoCustomToolError` find the chat.
 *
 * The rolls are all `min === max` (no draw) so the outcome is deterministic on
 * both sides without a shared byte source. The minted char-vault ids are written
 * to the `.meta.json` sidecar (the Rust side reads them to pass
 * `character_mount_point_id`).
 *
 * Regenerate (v4 @ d68638b4, Node 24):
 *   cd ~/source/quilltap-server
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

const CHAT = 'c1000000-0000-4000-8000-000000000001';
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
  const { UserSchema } = await import('@/lib/schemas/auth.types');
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

  await initializeDatabase();
  await ensureCollection('users', UserSchema);
  await ensureCollection('characters', CharacterSchema);
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
  await writeVaultFile(vaultA, 'metadata.json', { hasAnsibleAccess: true, clearanceLevel: 3 });

  await writeVaultFile(vaultB, 'Tools/ansible.tool.json', ANSIBLE);
  await writeVaultFile(vaultB, 'metadata.json', { faction: 'Ordo Ferrum' });

  await writeVaultFile(vaultC, 'Tools/ansible.tool.json', ANSIBLE);
  await writeVaultFile(vaultC, 'metadata.json', {});

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

  const meta = { vaultA, vaultB, vaultC };
  writeFileSync(mainOut + '.meta.json', JSON.stringify(meta));
  process.stderr.write(
    `built pascal-run-custom fixture: main=${mainOut} mount=${mountOut} ` +
      `vaultA=${vaultA} vaultB=${vaultB} vaultC=${vaultC}\n`,
  );
  process.exit(0);
}

main().catch((err) => {
  process.stderr.write(`pascal-run-custom fixture build failed: ${err?.stack ?? err}\n`);
  process.exit(1);
});
