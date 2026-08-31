/**
 * @jest-environment node
 *
 * P4.9G6 UUID-remap ORACLE **and corpus generator**: drives v4's REAL
 * `remapBackupData` (`lib/backup/restore/uuid-remap.ts:70`) over the
 * `UuidRemapper` from `lib/backup/uuid-remapper.ts`, with `crypto.randomUUID`
 * mocked to a counter so BOTH sides mint identical ids and the differential
 * needs **no normalization at all**.
 *
 * ── TWO ARTIFACTS, ONE INVOCATION ────────────────────────────────────────────
 *   `QT_CORPUS_OUT`  the committed corpus — the INPUTS. Rebuilt here rather
 *                    than hand-typed (the `byte-exact-static-data-transcription`
 *                    rule: ship the generator, not a blob).
 *   `QT_ORACLE_OUT`  the NDJSON — v4's outputs, one line per case.
 *
 * Every NDJSON line carries `corpusSha256`, the sha256 of the exact corpus bytes
 * this run wrote. The Rust side recomputes it from the COMMITTED file and
 * refuses to run on a mismatch, so a stale corpus can never green-light a pass.
 *
 * ── THE CASES ────────────────────────────────────────────────────────────────
 *   `wide` — the real `BackupData` v4's own `createBackup` projects out of the
 *            committed `system-data-{main,mount,llmlogs}.db` family (read back
 *            from the archive's `data/*.json`, which IS `collectUserData`'s
 *            output; the function itself is not exported). Every collection
 *            carries at least one row, so this single case exercises the whole
 *            entity table against real marshaled shapes.
 *   the rest — hand-authored edge documents, one per quirk, each named.
 *
 * ⚠ `wide` is stamped through `stabilizeWide`: the vault overlay synthesizes a
 * character's `physicalDescription` at READ time and stamps `createdAt` /
 * `updatedAt` "now", which would make the committed corpus churn on every
 * regen. Those two nested fields (and only those) are pinned to a constant.
 * `remapBackupData` never reads them — it rewrites `physicalDescription.id`,
 * which is derived, not random, and stays under diff. The same fields are
 * already normalized by `system_backup_equivalence` for the same reason.
 *
 * Run (Node 24, from the v4 checkout — cp to a /tmp mirror; jest ignores .claude/):
 *   N=~/.nvm/versions/node/v24.13.1/bin ; V5W=<this worktree>
 *   TMPO=/tmp/qt-uuidremap-oracle
 *   rm -rf "$TMPO"; mkdir -p "$TMPO/cases" "$TMPO/fixtures"
 *   cp "$V5W/harness/oracle/cases/backup-uuid-remap.test.ts" "$TMPO/cases/"
 *   cp "$V5W/harness/oracle/fixtures/system-data.json" "$TMPO/fixtures/"
 *   cd ~/source/quilltap-server
 *   QT_FIXTURE_SD_MAIN=$V5W/crates/quilltap-web/tests/fixtures/system-data-main.db \
 *   QT_FIXTURE_SD_MOUNT=$V5W/crates/quilltap-web/tests/fixtures/system-data-mount.db \
 *   QT_FIXTURE_SD_LLM=$V5W/crates/quilltap-web/tests/fixtures/system-data-llmlogs.db \
 *   QT_CORPUS_OUT=$V5W/harness/oracle/fixtures/uuid-remap-corpus.json \
 *   QT_ORACLE_OUT=/tmp/oracle-backup-uuid-remap.ndjson \
 *     $N/npx jest --silent --watchman=false --testTimeout=300000 \
 *       --roots "$PWD" --roots "$TMPO/cases" -- backup-uuid-remap
 */

import * as fs from 'fs';
import { createHash } from 'node:crypto';
import { execFileSync } from 'node:child_process';
import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import { mkdtempSync, mkdirSync, copyFileSync, existsSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';

interface Spec {
  testPepperBase64: string;
  userId: string;
}

/** The account the remap reassigns everything to. */
const TARGET_USER_ID = '11111111-2222-3333-4444-555555555555';
/** The pinned stamp `stabilizeWide` writes over the overlay's read-time "now". */
const PINNED_STAMP = '2026-01-01T00:00:00.000Z';

/**
 * Every collection `BackupData` carries, paired with the archive file
 * `createBackup` writes it to. Order is v4's `BackupData` order.
 */
const COLLECTIONS: ReadonlyArray<readonly [string, string]> = [
  ['characters', 'characters.json'],
  ['chats', 'chats.json'],
  ['tags', 'tags.json'],
  ['connectionProfiles', 'connection-profiles.json'],
  ['imageProfiles', 'image-profiles.json'],
  ['embeddingProfiles', 'embedding-profiles.json'],
  ['memories', 'memories.json'],
  ['files', 'files.json'],
  ['promptTemplates', 'prompt-templates.json'],
  ['roleplayTemplates', 'roleplay-templates.json'],
  ['providerModels', 'provider-models.json'],
  ['projects', 'projects.json'],
  ['groups', 'groups.json'],
  ['llmLogs', 'llm-logs.json'],
  ['pluginConfigs', 'plugin-configs.json'],
  ['chatSettings', 'chat-settings.json'],
  ['folders', 'folders.json'],
  ['wardrobeItems', 'wardrobe-items.json'],
  ['characterPluginData', 'character-plugin-data.json'],
  ['conversationAnnotations', 'conversation-annotations.json'],
  ['chatDocuments', 'chat-documents.json'],
  ['instanceSettings', 'instance-settings.json'],
  ['embeddingStatus', 'embedding-status.json'],
  ['conversationChunks', 'conversation-chunks.json'],
  ['tfidfVocabularies', 'tfidf-vocabularies.json'],
  ['vectorIndexMetas', 'vector-index-metas.json'],
  ['vectorEntries', 'vector-entries.json'],
  ['docMountPoints', 'doc-mount-points.json'],
  ['docMountFolders', 'doc-mount-folders.json'],
  ['docMountFiles', 'doc-mount-files.json'],
  ['docMountFileLinks', 'doc-mount-file-links.json'],
  ['docMountChunks', 'doc-mount-chunks.json'],
  ['docMountDocuments', 'doc-mount-documents.json'],
  ['docMountBlobs', 'doc-mount-blobs.json'],
  ['projectDocMountLinks', 'project-doc-mount-links.json'],
  ['groupDocMountLinks', 'group-doc-mount-links.json'],
  ['groupCharacterMembers', 'group-character-members.json'],
  ['textReplacementRules', 'text-replacement-rules.json'],
];

type Bag = Record<string, unknown[]>;

/** Every collection present and empty — v4 reads ten of them without a `|| []`. */
function emptyBag(): Bag {
  const out: Bag = {};
  for (const [key] of COLLECTIONS) out[key] = [];
  return out;
}

function bag(partial: Record<string, unknown[]>): Bag {
  return { ...emptyBag(), ...partial };
}

// ── The fixture read (phase 1: real modules) ──────────────────────────────────

function applyMocks(userId: string): void {
  const cipherDriverPath = require('node:path').join(
    process.cwd(),
    'packages/quilltap/node_modules/better-sqlite3-multiple-ciphers',
  );
  jest.doMock('better-sqlite3', () => jest.requireActual(cipherDriverPath));
  jest.doMock('@/lib/database/manager', () => jest.requireActual('@/lib/database/manager'));
  jest.doMock('@/lib/repositories/factory', () => jest.requireActual('@/lib/repositories/factory'));
  jest.doMock('@/lib/auth/session', () => ({
    __esModule: true,
    ...jest.requireActual('@/lib/auth/session'),
    getServerSession: async () => ({ user: { id: userId } }),
  }));
  jest.doMock('@/lib/startup/startup-state', () => {
    const actual = jest.requireActual('@/lib/startup/startup-state');
    return {
      __esModule: true,
      ...actual,
      startupState: {
        ...actual.startupState,
        isReady: () => true,
        waitForReady: async () => true,
        isPepperResolved: () => true,
        getPepperState: () => 'resolved',
        getPhase: () => 'ready',
        isLockedMode: () => false,
      },
    };
  });
}

/**
 * `collectUserData` is module-private in v4, so the wide case comes out of the
 * archive `createBackup` writes — the same arrays, one file each.
 */
async function collectWide(
  spec: Spec,
  scratchRoot: string,
  fixtures: { main: string; mount: string; llm: string },
): Promise<Bag> {
  jest.resetModules();
  applyMocks(spec.userId);

  const work = mkdtempSync(join(scratchRoot, 'wide-'));
  mkdirSync(join(work, 'data'), { recursive: true });
  const mainWork = join(work, 'main.db');
  const mountWork = join(work, 'mount.db');
  const llmWork = join(work, 'llm.db');
  copyFileSync(fixtures.main, mainWork);
  copyFileSync(fixtures.mount, mountWork);
  copyFileSync(fixtures.llm, llmWork);
  process.env.SQLITE_PATH = mainWork;
  process.env.SQLITE_MOUNT_INDEX_PATH = mountWork;
  process.env.SQLITE_LLM_LOGS_PATH = llmWork;
  process.env.QUILLTAP_DATA_DIR = work;

  const { initializeDatabase, closeDatabase } = await import('@/lib/database/manager');
  const { closeMountIndexSQLiteClient } = await import(
    '@/lib/database/backends/sqlite/mount-index-client'
  );
  const { closeLLMLogsSQLiteClient } = await import(
    '@/lib/database/backends/sqlite/llm-logs-client'
  );
  await initializeDatabase();

  const extractDir = mkdtempSync(join(scratchRoot, 'ex-'));
  try {
    const { createBackup } = await import('@/lib/backup/backup-service');
    const { zipPath } = await createBackup(spec.userId);
    execFileSync('unzip', ['-q', '-o', zipPath, '-d', extractDir], { maxBuffer: 64 * 1024 * 1024 });
    const roots = fs
      .readdirSync(extractDir, { withFileTypes: true })
      .filter((e) => e.isDirectory() && e.name.startsWith('quilltap-backup-'));
    if (roots.length !== 1) throw new Error(`expected one staging root, saw ${roots.length}`);
    const dataDir = join(extractDir, roots[0].name, 'data');

    const out: Bag = {};
    for (const [key, file] of COLLECTIONS) {
      const p = join(dataDir, file);
      if (!existsSync(p)) throw new Error(`archive is missing data/${file}`);
      const parsed = JSON.parse(fs.readFileSync(p, 'utf8'));
      if (!Array.isArray(parsed)) throw new Error(`data/${file} is not an array`);
      out[key] = parsed;
    }
    return out;
  } finally {
    await closeDatabase();
    closeMountIndexSQLiteClient();
    closeLLMLogsSQLiteClient();
    rmSync(extractDir, { recursive: true, force: true });
    rmSync(work, { recursive: true, force: true });
  }
}

/**
 * Pin the ONE read-time-"now" pair in the wide projection (see the header).
 * Everything else in the archive is a stored column.
 */
function stabilizeWide(wide: Bag): Bag {
  for (const c of wide.characters as Array<Record<string, unknown>>) {
    const pd = c.physicalDescription as Record<string, unknown> | null | undefined;
    if (pd && typeof pd === 'object') {
      if ('createdAt' in pd) pd.createdAt = PINNED_STAMP;
      if ('updatedAt' in pd) pd.updatedAt = PINNED_STAMP;
    }
  }
  return wide;
}

// ── The hand-authored edge documents ─────────────────────────────────────────

interface Case {
  name: string;
  note: string;
  targetUserId: string;
  data: Bag;
}

function edgeCases(): Case[] {
  const t = TARGET_USER_ID;
  return [
    {
      name: 'all_empty',
      note: 'every collection absent/empty at once — the ten v4 reads without a `|| []` still need the key present',
      targetUserId: t,
      data: emptyBag(),
    },
    {
      name: 'legacy_persona_links',
      note: 'char A: legacy personaLinks folded to partnerLinks (only partnerId+isDefault survive) then deleted — note the FOLD APPENDS partnerLinks after userId; char B: personaLinks present but partnerLinks ALSO present, so the fold is skipped and personaLinks survives verbatim; char C: personaLinks present with partnerLinks EMPTY ([] is truthy in JS, so the fold is still skipped)',
      targetUserId: t,
      data: bag({
        characters: [
          {
            id: 'char-a',
            userId: 'old-user',
            name: 'A',
            personaLinks: [
              { personaId: 'persona-1', isDefault: true, extra: 'dropped' },
              { personaId: 'persona-2' },
            ],
          },
          {
            id: 'char-b',
            name: 'B',
            partnerLinks: [{ partnerId: 'persona-1', isDefault: false }],
            personaLinks: [{ personaId: 'persona-9', isDefault: true }],
          },
          {
            id: 'char-c',
            name: 'C',
            partnerLinks: [],
            personaLinks: [{ personaId: 'persona-8', isDefault: true }],
          },
        ],
      }),
    },
    {
      name: 'partner_links',
      note: 'partnerLinks (new format): `{...link, partnerId}` keeps every other key AND partnerId\'s position; the same partnerId in two links memoizes to one new id',
      targetUserId: t,
      data: bag({
        characters: [
          {
            id: 'char-p',
            partnerLinks: [
              { partnerId: 'p-1', isDefault: true, nickname: 'kept' },
              { note: 'partnerId is APPENDED here', partnerId: 'p-1' },
            ],
          },
          { id: 'char-q', partnerLinks: null },
        ],
      }),
    },
    {
      name: 'physical_descriptions_plural',
      note: 'legacy plural array with a row collapses to the singular (and APPENDS the key, since the singular was absent) then deletes the plural; an EMPTY plural collapses to null; the singular path remaps id in place; a null singular is left alone',
      targetUserId: t,
      data: bag({
        characters: [
          {
            id: 'pd-a',
            physicalDescriptions: [
              { id: 'pd-1', height: 'tall', characterId: 'pd-a' },
              { id: 'pd-2', height: 'ignored' },
            ],
            trailing: 'proves shift_remove, not swap_remove',
          },
          { id: 'pd-b', physicalDescriptions: [], trailing: 'still last' },
          { id: 'pd-c', physicalDescription: { id: 'pd-3', height: 'short' } },
          { id: 'pd-d', physicalDescription: null },
        ],
      }),
    },
    {
      name: 'clothing_records',
      note: 'a truthy legacy clothingRecords is deleted (shift_remove — `trailing` must keep its place); a null one is NOT deleted',
      targetUserId: t,
      data: bag({
        characters: [
          { id: 'cr-a', clothingRecords: [{ id: 'x' }], trailing: 'still last' },
          { id: 'cr-b', clothingRecords: [], trailing: 'empty array is TRUTHY in JS' },
          { id: 'cr-c', clothingRecords: null, trailing: 'kept' },
        ],
      }),
    },
    {
      name: 'avatar_overrides',
      note: 'each override is REBUILT as exactly {chatId, imageId} — extra keys are dropped, chatId is remapped first',
      targetUserId: t,
      data: bag({
        characters: [
          {
            id: 'ao-a',
            avatarOverrides: [
              { imageId: 'img-1', chatId: 'chat-1', extra: 'dropped' },
              { chatId: 'chat-1', imageId: 'img-2' },
            ],
          },
          { id: 'ao-b', avatarOverrides: [] },
        ],
      }),
    },
    {
      name: 'chat_settings_bags_present',
      note: 'all three nested bags present with truthy ids — each id remapped in place, key order preserved',
      targetUserId: t,
      data: bag({
        chatSettings: [
          {
            id: 'cs-1',
            userId: 'old-user',
            imageDescriptionProfileId: 'idp-1',
            uncensoredImageDescriptionProfileId: 'uidp-1',
            defaultRoleplayTemplateId: 'drt-1',
            cheapLLMSettings: {
              enabled: true,
              userDefinedProfileId: 'cheap-1',
              defaultCheapProfileId: 'cheap-2',
              imagePromptProfileId: 'cheap-3',
            },
            dangerousContentSettings: {
              uncensoredTextProfileId: 'danger-1',
              uncensoredImageProfileId: 'danger-2',
              level: 1,
            },
            storyBackgroundsSettings: { enabled: true, defaultImageProfileId: 'story-1' },
          },
        ],
      }),
    },
    {
      name: 'chat_settings_bags_null_ids',
      note: 'the bags exist but every nested id is null/empty — the conditional spreads write NOTHING, so the nulls survive; storyBackgroundsSettings is guarded on its one id, so the bag is not rewritten at all',
      targetUserId: t,
      data: bag({
        chatSettings: [
          {
            id: 'cs-2',
            imageDescriptionProfileId: null,
            cheapLLMSettings: {
              userDefinedProfileId: null,
              defaultCheapProfileId: '',
              imagePromptProfileId: 'cheap-only',
            },
            dangerousContentSettings: { uncensoredTextProfileId: null },
            storyBackgroundsSettings: { defaultImageProfileId: null, enabled: false },
          },
        ],
      }),
    },
    {
      name: 'chat_settings_bags_absent',
      note: 'no nested bags at all (and a null one) — no key is invented',
      targetUserId: t,
      data: bag({
        chatSettings: [
          { id: 'cs-3', userId: 'old-user' },
          { id: 'cs-4', cheapLLMSettings: null, storyBackgroundsSettings: null },
        ],
      }),
    },
    {
      name: 'instance_settings',
      note: 'all three MOUNT_POINT_SETTING_KEYS remap and are REBUILT as exactly {key,value} (dropping any other column, and fixing the key order); a non-mount key passes through byte-identical; a mount key with an EMPTY value passes through unchanged',
      targetUserId: t,
      data: bag({
        instanceSettings: [
          { key: 'lanternBackgroundsMountPointId', value: 'mount-1' },
          { value: 'mount-2', key: 'userUploadsMountPointId', updatedAt: 'dropped' },
          { key: 'generalMountPointId', value: 'mount-3' },
          { key: 'somethingElse', value: 'mount-1' },
          { key: 'generalMountPointId', value: '' },
          { key: 'userUploadsMountPointId', value: null },
        ],
      }),
    },
    {
      name: 'null_and_absent_scalars',
      note: "remapFields' `typeof === 'string'` guard: null, a number, a bool and an absent key are all left exactly as they were",
      targetUserId: t,
      data: bag({
        files: [
          {
            id: 'file-1',
            projectId: null,
            linkedTo: null,
            tags: ['tag-1'],
            userId: 'old-user',
            size: 12,
          },
          { id: 'file-2', projectId: 7, linkedTo: [], tags: [] },
          { id: 'file-3' },
        ],
        folders: [
          { id: 'folder-1', parentFolderId: null, projectId: 'proj-1' },
          { parentFolderId: 'folder-1', id: 'folder-2' },
        ],
        llmLogs: [{ id: 'log-1', messageId: null, chatId: 'chat-x', characterId: false }],
      }),
    },
    {
      name: 'llm_log_profile_attribution',
      note: "P4.D49 / v4 0cde7fbc: an llm_logs row's connectionProfileId and imageProfileId must land on the SAME minted ids the profiles themselves got, or the Almanack's per-profile attribution reads every restored row as a deleted profile. The second log shares the connection profile (one mapping entry, reused), the third carries nulls (a pre-4.9 row, untouched by remapFields' string guard).",
      targetUserId: t,
      data: bag({
        connectionProfiles: [{ id: 'cp-1' }, { id: 'cp-2' }],
        imageProfiles: [{ id: 'ip-1' }],
        llmLogs: [
          {
            id: 'log-a',
            messageId: 'msg-1',
            chatId: 'chat-1',
            characterId: 'char-1',
            connectionProfileId: 'cp-1',
            imageProfileId: null,
          },
          {
            id: 'log-b',
            connectionProfileId: 'cp-1',
            imageProfileId: 'ip-1',
          },
          {
            id: 'log-c',
            connectionProfileId: null,
            imageProfileId: null,
          },
          {
            id: 'log-d',
            note: 'a profile id naming nothing in this backup still gets its own mapping entry',
            connectionProfileId: 'cp-orphan',
          },
        ],
      }),
    },
    {
      name: 'array_field_guards',
      note: "remapArrayFields' Array.isArray guard: a non-array is left alone (it is NOT routed through remapArray's dead [] branch); an empty array stays empty",
      targetUserId: t,
      data: bag({
        files: [
          { id: 'g-1', linkedTo: 'not-an-array', tags: [] },
          { id: 'g-2', linkedTo: null, tags: { nope: true } },
        ],
        memories: [{ id: 'm-1', tags: [], relatedMemoryIds: 3, chatId: 'chat-y' }],
      }),
    },
    {
      name: 'mixed_key_types',
      note: 'the keying trap: a UUID array carrying a string, a number, the SAME number as a string, null and true — v4 keys a JS Map, so "5.5" and 5.5 are DIFFERENT entries that collapse to one key in getMapping()',
      targetUserId: t,
      data: bag({
        files: [{ id: 'k-1', tags: ['a', 5.5, '5.5', null, true, 'a'], linkedTo: [] }],
      }),
    },
    {
      name: 'repeated_id_across_entities',
      note: 'memoization is the whole mechanism: chat.id, memory.chatId, chatDocument.chatId, conversationChunk.chatId and conversationAnnotation.chatId all resolve to ONE new id',
      targetUserId: t,
      data: bag({
        chats: [
          {
            id: 'shared-chat',
            userId: 'old-user',
            projectId: 'shared-project',
            tags: ['shared-tag'],
            impersonatingParticipantIds: ['part-1'],
            participants: [
              { id: 'part-1', characterId: 'shared-char', connectionProfileId: null },
              { id: 'part-2', characterId: 'shared-char', imageProfileId: 'ip-1' },
            ],
            messages: [
              { id: 'msg-1', swipeGroupId: 'swipe-1', participantId: 'part-1', attachments: [] },
              {
                id: 'msg-2',
                participantId: 'part-2',
                attachments: ['file-att-1', 'file-att-1'],
                content: 'kept',
              },
            ],
          },
        ],
        memories: [{ id: 'mem-1', chatId: 'shared-chat', characterId: 'shared-char', tags: [] }],
        projects: [{ id: 'shared-project', characterRoster: ['shared-char'] }],
        chatDocuments: [{ id: 'cd-1', chatId: 'shared-chat' }],
        conversationChunks: [
          { id: 'cc-1', chatId: 'shared-chat', messageIds: ['msg-1', 'msg-2'] },
        ],
        conversationAnnotations: [
          { id: 'ca-1', chatId: 'shared-chat', sourceMessageId: 'msg-1' },
        ],
        characterPluginData: [{ id: 'cpd-1', characterId: 'shared-char', data: '{}' }],
        wardrobeItems: [
          { id: 'wi-1', characterId: 'shared-char', componentItemIds: ['wi-2'] },
          { id: 'wi-2', characterId: 'shared-char' },
        ],
        vectorIndexMetas: [{ id: 'shared-char', characterId: 'shared-char' }],
        vectorEntries: [{ id: 'mem-1', characterId: 'shared-char', embedding: [0.5] }],
        embeddingStatus: [{ id: 'es-1', entityId: 'mem-1', profileId: 'ep-1' }],
        tfidfVocabularies: [{ id: 'tv-1', profileId: 'ep-1' }],
        embeddingProfiles: [{ id: 'ep-1', apiKeyId: 'ak-1', tags: [] }],
      }),
    },
    {
      name: 'groups_official_mount_point',
      note: 'ONLY the group id is remapped — officialMountPointId is deliberately left alone (groups.create discards it and provisions a fresh store), and groups get NO userId rewrite',
      targetUserId: t,
      data: bag({
        groups: [
          { id: 'group-1', officialMountPointId: 'mount-1', userId: 'old-user', name: 'G' },
        ],
        groupDocMountLinks: [{ id: 'gdml-1', groupId: 'group-1', mountPointId: 'mount-1' }],
        groupCharacterMembers: [{ id: 'gcm-1', groupId: 'group-1', characterId: 'char-1' }],
        docMountPoints: [{ id: 'mount-1', name: 'store' }],
      }),
    },
    {
      name: 'pass_through_collections',
      note: 'providerModels (global) and textReplacementRules (global config, nothing references rule ids) pass through untouched — ids included',
      targetUserId: t,
      data: bag({
        providerModels: [{ id: 'pm-1', provider: 'anthropic', userId: 'old-user' }],
        textReplacementRules: [{ id: 'trr-1', find: 'a', replace: 'b', userId: 'old-user' }],
      }),
    },
    {
      name: 'doc_mount_graph',
      note: 'the document-store tables: a file id shared by its document and blob rows resolves to one new id, and every FK is rewritten',
      targetUserId: t,
      data: bag({
        docMountPoints: [{ id: 'mp-1', name: 'notes' }],
        docMountFolders: [
          { id: 'fol-1', mountPointId: 'mp-1', parentId: null },
          { id: 'fol-2', mountPointId: 'mp-1', parentId: 'fol-1' },
        ],
        docMountFiles: [{ id: 'dmf-1', sha256: 'abc' }],
        docMountFileLinks: [
          { id: 'link-1', fileId: 'dmf-1', mountPointId: 'mp-1', folderId: 'fol-2' },
        ],
        docMountChunks: [{ id: 'chunk-1', linkId: 'link-1', mountPointId: 'mp-1' }],
        docMountDocuments: [{ id: 'doc-1', fileId: 'dmf-1' }],
        docMountBlobs: [{ id: 'blob-1', fileId: 'dmf-1' }],
        projectDocMountLinks: [{ id: 'pdml-1', projectId: 'proj-1', mountPointId: 'mp-1' }],
        projects: [{ id: 'proj-1', characterRoster: [] }],
        pluginConfigs: [{ id: 'pc-1', pluginId: 'plug', enabled: true }],
      }),
    },
    {
      name: 'fallback_understudy_links',
      note: "P4.D135: fallbackProfileId points at another row in the SAME table, so it rides remapFields alongside id. The memoizer is lazy and consistent, so a FORWARD reference (cp-a names cp-b, which appears after it) resolves to the same new id as cp-b's own — the trap the reconcile pass exists for on the import side. A dangling reference mints an id for a row that is not there (v4 remaps the value, it does not check it); a self-reference collapses to the row's own new id; an absent key is untouched, and so is an explicit null.",
      targetUserId: t,
      data: bag({
        connectionProfiles: [
          { id: 'cp-a', apiKeyId: 'ak-1', fallbackProfileId: 'cp-b', tags: [] },
          { id: 'cp-b', apiKeyId: null, fallbackProfileId: 'cp-missing', tags: [] },
          { id: 'cp-c', fallbackProfileId: 'cp-c', tags: [] },
          { id: 'cp-d', fallbackProfileId: null, tags: [] },
          { id: 'cp-e', tags: [] },
        ],
      }),
    },
    {
      name: 'user_id_position',
      note: 'trap 1: an EXISTING userId keeps its position with the new value; an absent one is APPENDED',
      targetUserId: t,
      data: bag({
        tags: [
          { userId: 'old-user', id: 'tag-1', name: 'first-key-was-userId' },
          { id: 'tag-2', name: 'no userId at all' },
        ],
        promptTemplates: [{ id: 'pt-1', userId: 'old-user', tags: ['tag-1'] }],
        roleplayTemplates: [{ id: 'rt-1', tags: [] }],
        connectionProfiles: [{ id: 'cp-1', apiKeyId: 'ak-1', tags: ['tag-2'] }],
        imageProfiles: [{ id: 'ip-1', apiKeyId: null, tags: [] }],
      }),
    },
  ];
}

// ── The oracle run (phase 2: crypto mocked to a counter) ─────────────────────

let counter = 0;
const nextId = (): string =>
  `00000000-0000-4000-8000-${String(++counter).padStart(12, '0')}`;

async function main(): Promise<void> {
  const here = dirname(fileURLToPath(import.meta.url));
  const spec = JSON.parse(
    fs.readFileSync(join(here, '..', 'fixtures', 'system-data.json'), 'utf8'),
  ) as Spec;

  const fixtures = {
    main: process.env.QT_FIXTURE_SD_MAIN ?? '',
    mount: process.env.QT_FIXTURE_SD_MOUNT ?? '',
    llm: process.env.QT_FIXTURE_SD_LLM ?? '',
  };
  for (const [k, v] of Object.entries(fixtures)) {
    if (!v || !existsSync(v)) throw new Error(`fixture ${k} missing: ${v}`);
  }
  const corpusPath = process.env.QT_CORPUS_OUT;
  if (!corpusPath) throw new Error('QT_CORPUS_OUT must point at the corpus JSON to write');
  const outPath = process.env.QT_ORACLE_OUT;
  if (!outPath) throw new Error('QT_ORACLE_OUT must point at the NDJSON file to write');

  const scratchRoot = mkdtempSync(join(tmpdir(), 'qt-uuidremap-oracle-'));
  process.env.ENCRYPTION_MASTER_PEPPER = spec.testPepperBase64;
  delete process.env.SQLITE_WAL_MODE;
  process.env.LOG_LEVEL = 'error';

  // Phase 1 — the wide case, out of v4's own archive.
  const wide = stabilizeWide(await collectWide(spec, scratchRoot, fixtures));
  const cases: Case[] = [
    {
      name: 'wide',
      note: "v4's own createBackup projection of the committed system-data-* fixture family — every collection carries a row",
      targetUserId: TARGET_USER_ID,
      data: bag(wide),
    },
    ...edgeCases(),
  ];

  // The committed corpus — the INPUT half of the contract.
  const corpus = {
    _meta: {
      baseline: 'e646f58b',
      generatedBy: 'harness/oracle/cases/backup-uuid-remap.test.ts',
      idSource: '00000000-0000-4000-8000-<12-digit counter, from 1>',
      widePinnedStamp: PINNED_STAMP,
      collections: COLLECTIONS.map(([k]) => k),
    },
    cases,
  };
  const corpusText = JSON.stringify(corpus, null, 2) + '\n';
  fs.writeFileSync(corpusPath, corpusText, 'utf8');
  const corpusSha256 = createHash('sha256').update(corpusText, 'utf8').digest('hex');

  // Phase 2 — v4's REAL remapBackupData, with a counting randomUUID.
  jest.resetModules();
  jest.doMock('crypto', () => ({
    ...jest.requireActual('crypto'),
    randomUUID: () => nextId(),
  }));
  const { UuidRemapper } = await import('@/lib/backup/uuid-remapper');
  const { remapBackupData } = await import('@/lib/backup/restore/uuid-remap');

  const outLines: string[] = [];
  for (const c of cases) {
    counter = 0;
    const remapper = new UuidRemapper();
    // A deep clone per case: `remapBackupData` must never mutate its input, and
    // the corpus we just wrote is the Rust side's input too.
    const input = JSON.parse(JSON.stringify(c.data));
    const output = remapBackupData(input, c.targetUserId, remapper);
    outLines.push(
      JSON.stringify({
        name: c.name,
        corpusSha256,
        output,
        mapping: remapper.getMapping(),
        size: remapper.getSize(),
        inputUnchanged: JSON.stringify(input) === JSON.stringify(c.data),
      }),
    );
  }
  fs.writeFileSync(outPath, outLines.join('\n') + '\n');
  rmSync(scratchRoot, { recursive: true, force: true });
  process.stderr.write(
    `backup-uuid-remap oracle wrote ${outPath} (${outLines.length} cases) ` +
      `and corpus ${corpusPath} (sha256 ${corpusSha256})\n`,
  );
}

test('backup-uuid-remap oracle', async () => {
  await main();
});
