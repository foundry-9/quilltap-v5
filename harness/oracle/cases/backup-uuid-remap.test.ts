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
 *   rm -rf "$TMPO"; mkdir -p "$TMPO/cases" "$TMPO/fixtures" "$TMPO/lib"
 *   cp "$V5W/harness/oracle/cases/backup-uuid-remap.test.ts" "$TMPO/cases/"
 *   cp "$V5W/harness/oracle/lib/v4-migrations.ts" "$TMPO/lib/"
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
import { plantChatWardrobeHistoryTable } from '../lib/v4-migrations';

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
  ['chatInforms', 'chat-informs.json'],
  ['instanceSettings', 'instance-settings.json'],
  ['embeddingStatus', 'embedding-status.json'],
  ['conversationChunks', 'conversation-chunks.json'],
  ['tfidfVocabularies', 'tfidf-vocabularies.json'],
  // [P4.D293] v4 `af48f6cc2`: `remapBackupData` returns no `vectorIndexMetas`
  // and no backup writes either vector file. `vectorEntries` stays — the
  // LEGACY mirror rows an old archive carries are still remapped (so the
  // restore's adoption matches the remapped memory id) — read optionally.
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
  // P4.D264 (v4 `3ee3b1342`, #81): the wear ledger — the 39th returned key,
  // ALWAYS present (`uuid-remap.ts:629`); `data/wardrobe-wear.json` is written
  // on every backup (`backup-service.ts:722`).
  ['wardrobeWear', 'wardrobe-wear.json'],
  // P4.D317 (v4 `7953da3a0`): the outfit history — the 40th returned key,
  // ALWAYS present (`uuid-remap.ts:733`); `data/wardrobe-history.json` on
  // every backup (`backup-service.ts:709-710`).
  ['wardrobeHistory', 'wardrobe-history.json'],
];

/** [P4.D293] Files a pin backup no longer writes but the remap still reads. */
const OPTIONAL_FILES = new Set(['vector-entries.json']);

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
  // [P4.D317] v4's `createBackup` reads `chat_wardrobe_history` UNGUARDED at
  // the `e8e17b2d9` pin, so the `wide` copy plants the table first (the round's
  // shared plant — v4's REAL DDL; never the seed migration). Empty: `wide`
  // carries `wardrobeHistory: []`.
  await plantChatWardrobeHistoryTable();

  const extractDir = mkdtempSync(join(scratchRoot, 'ex-'));
  try {
    const { createBackup } = await import('@/lib/backup/backup-service');
    const { zipPath } = await createBackup(spec.userId);
    execFileSync('unzip', ['-q', '-o', zipPath, '-d', extractDir], { maxBuffer: 64 * 1024 * 1024 });
    // createBackup leaves its zip in a private mkdtemp dir the caller owns;
    // remove it as v4's download handler does (`system/backup/[id]/route.ts`).
    rmSync(dirname(zipPath), { recursive: true, force: true });
    const roots = fs
      .readdirSync(extractDir, { withFileTypes: true })
      .filter((e) => e.isDirectory() && e.name.startsWith('quilltap-backup-'));
    if (roots.length !== 1) throw new Error(`expected one staging root, saw ${roots.length}`);
    const dataDir = join(extractDir, roots[0].name, 'data');

    const out: Bag = {};
    for (const [key, file] of COLLECTIONS) {
      const p = join(dataDir, file);
      if (!existsSync(p)) {
        if (OPTIONAL_FILES.has(file)) {
          out[key] = [];
          continue;
        }
        throw new Error(`archive is missing data/${file}`);
      }
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
  /**
   * P4.D264 (v4 `7c8572869`, `restore.ts:98-104`): run the restore's ORDER —
   * `planWardrobeImagePointerFixes(original, remapper)` FIRST, then
   * `remapBackupData` with the SAME remapper — and emit the planned fixes. The
   * plan primes the remapper (mount, legacy-item and pointer ids), so every
   * later minted id lands where the restore path puts it.
   */
  restorePath?: boolean;
}

/**
 * P4.D264: a path-derived wardrobe item id, through v4's REAL parser (the
 * corpus stores it as data, so the Rust side reads the same bytes).
 */
function pathDerivedItemId(mountPointId: string, relativePath: string, content: string): string {
  const { wardrobeItemIdForDocument } = jest.requireActual(
    '@/lib/database/repositories/vault-overlay/parsers',
  ) as typeof import('@/lib/database/repositories/vault-overlay/parsers');
  return wardrobeItemIdForDocument({ mountPointId, relativePath, content });
}

function edgeCases(): Case[] {
  const t = TARGET_USER_ID;
  const hatContent = '---\ntitle: Hat\ntypes: [accessories]\n---\n';
  const hatPicContent = '---\ntitle: Hat\ntypes: [accessories]\nimageFileId: file-hat\n---\n';
  const hatItemId = pathDerivedItemId('mount-hat', 'Wardrobe/Hat.md', hatContent);
  const hatPicItemId = pathDerivedItemId('mount-old', 'Wardrobe/Hat.md', hatPicContent);
  // P4.D284 (v4 `4a38222e6`): the item-path test is RECURSIVE, so a nested
  // id-less item is recomputed against the remapped mount like a root one;
  // pictures (`Wardrobe/images/**`) and `instructions.md` at any depth are not
  // items, and their ledger rows pass through unchanged.
  const nestedContent = '---\ntitle: Old Coat\ntypes: [top]\n---\n';
  const nestedItemId = pathDerivedItemId('mount-hat', 'Wardrobe/Old/Coat.md', nestedContent);
  const picDocContent = '---\ntitle: Pictured Coat\ntypes: [top]\n---\n';
  const picDocItemId = pathDerivedItemId('mount-hat', 'Wardrobe/images/hat-item/Coat.md', picDocContent);
  const nestedInstrItemId = pathDerivedItemId('mount-hat', 'Wardrobe/Formal/instructions.md', 'Formal rules.');
  const eveningContent = '---\ntitle: Evening Coat\ntypes: [top]\nimageFileId: file-evening\n---\n';
  const eveningItemId = pathDerivedItemId('mount-evening', 'Wardrobe/Formal/Evening/Coat.md', eveningContent);
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
    // P4.D227 (v4 `3b463d6b1`, #76): the Concierge's own settings. A 4.10
    // archive carries `conciergeSettings` — the four desk ids remap in place;
    // the legacy-shaped rows above now TRANSLATE (add-concierge-settings-v1's
    // mapping) and the retired keys are left for the schema to strip.
    {
      name: 'chat_settings_concierge_desk_ids',
      note: 'a 4.10 archive: conciergeSettings present, its four desk ids remapped in place, every other key kept',
      targetUserId: t,
      data: bag({
        chatSettings: [
          {
            id: 'cs-5',
            userId: 'old-user',
            conciergeSettings: {
              enabled: true,
              uncensoredTextProfileId: 'desk-text',
              uncensoredImageProfileId: 'desk-image',
              uncensoredVisionProfileId: 'desk-vision',
              imagePromptProfileId: 'desk-crafter',
              autoSwitchAfterRefusals: 3,
              newChatsStartAs: 'unmoderated',
              display: { mode: 'BLUR', showWarningBadges: false },
              preScreen: { enabled: true, threshold: 0.4, summaryClassification: true },
            },
          },
        ],
      }),
    },
    {
      name: 'chat_settings_legacy_with_unmoderated_chat',
      note: 'a pre-4.10 archive with the Concierge OFF but an Unmoderated chat in the same backup: the translation keeps him on duty (pre-screen off)',
      targetUserId: t,
      data: bag({
        chats: [{ id: 'chat-u', userId: 'old-user', title: 'frank', conciergeOverride: 'UNCENSORED', participants: [], messages: [] }],
        chatSettings: [
          {
            id: 'cs-6',
            userId: 'old-user',
            uncensoredImageDescriptionProfileId: 'uidp-6',
            cheapLLMSettings: { imagePromptProfileId: 'cheap-6' },
            dangerousContentSettings: { mode: 'OFF', uncensoredTextProfileId: 'danger-6', threshold: 0.3 },
          },
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
      name: 'route_trail_profile_id_not_remapped',
      note: "P4.D171 (v4 5841a8c62): a message's routeTrail is NOT in messages' remapFields list (only id/swipeGroupId/participantId are), so a routeTrail[].profileId that IS a real connection-profile id elsewhere in the same backup must come out of remapBackupData UNCHANGED inside the trail even though that SAME id, read off connectionProfiles[0].id or the participant's connectionProfileId, gets minted a fresh one. v4's design of record (docs/developer/features/complete/message-route-trail.md) states this is deliberate: profileId is a historical reference, never remapped on import.",
      targetUserId: t,
      data: bag({
        connectionProfiles: [{ id: 'rt-profile-1', name: 'Primary' }],
        chats: [
          {
            id: 'rt-chat-1',
            userId: 'old-user',
            participants: [{ id: 'rt-part-1', connectionProfileId: 'rt-profile-1' }],
            messages: [
              {
                id: 'rt-msg-1',
                participantId: 'rt-part-1',
                routeTrail: [
                  {
                    profileId: 'rt-profile-1',
                    profileName: 'Primary',
                    provider: 'anthropic',
                    modelName: 'claude-opus-4-8',
                    via: 'primary',
                    outcome: 'answered',
                  },
                ],
              },
            ],
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
        vectorEntries: [{ id: 'mem-1', characterId: 'shared-char', embedding: [0.5] }],
        embeddingStatus: [{ id: 'es-1', entityId: 'mem-1', profileId: 'ep-1' }],
        tfidfVocabularies: [{ id: 'tv-1', profileId: 'ep-1' }],
        embeddingProfiles: [{ id: 'ep-1', apiKeyId: 'ak-1', tags: [] }],
      }),
    },
    {
      name: 'chat_informs_six_field_remap',
      note: 'P4.D205 (v4 e7d77bb60): all SIX id fields move — id, chatId, batchId, participantId, recordMessageId, consumedByMessageId. batchId is a row nowhere, and is remapped anyway so the whole batch travels together and no source id survives. A row with the two nullable message pointers ABSENT is the other half: remapFields only touches strings, so they stay absent.',
      targetUserId: t,
      data: bag({
        chats: [
          {
            id: 'inf-chat',
            userId: 'old-user',
            participants: [{ id: 'inf-part', characterId: 'inf-char' }],
            messages: [
              { id: 'inf-record-msg', participantId: 'inf-part', attachments: [] },
              { id: 'inf-consumed-msg', participantId: 'inf-part', attachments: [] },
            ],
          },
        ],
        chatInforms: [
          {
            id: 'inf-1',
            chatId: 'inf-chat',
            batchId: 'inf-batch',
            participantId: 'inf-part',
            contentMarkdown: 'You notice the clock has stopped.',
            recordMessageId: 'inf-record-msg',
            // P4.D249 (v4 `52d6e7ecd`): a STANDING row — a boolean the remap
            // must carry through untouched (it remaps id fields only).
            permanent: true,
            consumedAt: '2026-01-01T00:00:00.000Z',
            consumedByMessageId: 'inf-consumed-msg',
          },
          {
            id: 'inf-2',
            chatId: 'inf-chat',
            batchId: 'inf-batch',
            participantId: 'inf-part',
            contentMarkdown: 'A second passage in the same batch.',
          },
        ],
      }),
    },
    {
      name: 'groups_official_mount_point',
      note: 'the group id AND officialMountPointId are remapped (v4 `ed8b15b50` bug 185 — in lockstep with the store row, so the restore binds the group to its archived store); groups get NO userId rewrite',
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
    // ── P4.D273: v4 `ed8b15b50` (bug 185) + `d58548051` remap moves ─────────
    {
      name: 'store_pointers_lockstep',
      note: 'a character vault pointer and a project / group official store pointer each equal the NEW id of the store row they name (v4 jest `uuid-remapper.test.ts:479-496`); the three pointers are minted at the entity passes and REUSED by the docMountPoints pass',
      targetUserId: t,
      data: bag({
        characters: [{ id: 'char-old', name: 'Friday', characterDocumentMountPointId: 'vault-old' }],
        projects: [{ id: 'proj-old', name: 'P', officialMountPointId: 'pstore-old' }],
        groups: [{ id: 'group-old', name: 'G', officialMountPointId: 'gstore-old' }],
        docMountPoints: [
          { id: 'vault-old', name: 'Friday Character Vault', storeType: 'character' },
          { id: 'pstore-old', name: 'Project Files: P', storeType: 'documents' },
          { id: 'gstore-old', name: 'Group Files: G', storeType: 'documents' },
        ],
      }),
    },
    {
      name: 'consolidation_references',
      note: 'supersededById and consolidatedFrom are memory-id references remapped in lockstep with id (v4 `d58548051`); m1 is listed BEFORE its digest, so its supersededById mints the digest id the digest row then reuses',
      targetUserId: t,
      data: bag({
        memories: [
          { id: 'm1-old', characterId: 'c', tier: 'cold', supersededById: 'digest-old', consolidatedFrom: [] },
          {
            id: 'digest-old',
            characterId: 'c',
            source: 'CONSOLIDATED',
            tier: 'hot',
            consolidatedFrom: ['m1-old', 'm2-old'],
            supersededById: null,
          },
          { id: 'm2-old', characterId: 'c', tier: 'cold', supersededById: 'digest-old', consolidatedFrom: [] },
        ],
      }),
    },
    {
      name: 'pre_tier_memory_untouched',
      note: 'a memory from before the tier columns gains NO supersededById / consolidatedFrom key (remapFields / remapArrayFields skip absent values)',
      targetUserId: t,
      data: bag({
        memories: [{ id: 'old', characterId: 'c' }],
      }),
    },
    {
      name: 'chat_other_watermark',
      note: "a chat's otherExtractionWatermarkMessageId is minted at the chat's scalar pass (LAST in the list) and REUSED by its message (v4 `d58548051`)",
      targetUserId: t,
      data: bag({
        chats: [
          {
            id: 'chat-old',
            participants: [],
            tags: [],
            otherExtractionWatermarkMessageId: 'msg-old',
            messages: [{ id: 'msg-old', type: 'message' }],
          },
        ],
      }),
    },
    // [P4.D293] v4 `c2795733f` / `64743ee13`: the episode and turn cursor keys
    // follow their messages too (LAST in the scalar list, BUNDLE_CURSOR_FIELDS
    // order). A cursor naming a message OUTSIDE the archive still mints.
    {
      name: 'chat_episode_watermark',
      note: "[P4.D293] episodeExtractionWatermarkMessageId is minted at the chat's scalar pass (after the other key) and REUSED by its message (v4 c2795733f)",
      targetUserId: t,
      data: bag({
        chats: [
          {
            id: 'chat-old',
            participants: [],
            tags: [],
            otherExtractionWatermarkMessageId: 'msg-a',
            episodeExtractionWatermarkMessageId: 'msg-b',
            messages: [
              { id: 'msg-a', type: 'message' },
              { id: 'msg-b', type: 'message' },
            ],
          },
        ],
      }),
    },
    {
      name: 'chat_turn_watermark',
      note: "[P4.D293] turnExtractionWatermarkMessageId (v4 64743ee13) minted LAST; one naming a message outside the archive still gets a minted id",
      targetUserId: t,
      data: bag({
        chats: [
          {
            id: 'chat-old',
            participants: [],
            tags: [],
            otherExtractionWatermarkMessageId: 'msg-a',
            episodeExtractionWatermarkMessageId: 'msg-b',
            turnExtractionWatermarkMessageId: 'msg-not-in-archive',
            messages: [
              { id: 'msg-a', type: 'message' },
              { id: 'msg-b', type: 'message' },
            ],
          },
        ],
      }),
    },
    // [P4.D293] v4 545e299bb: the scenario source follows its store through the
    // shared remapper — after the chat scalars + userId, before participants.
    {
      name: 'chat_scenario_source',
      note: '[P4.D293] chat-1 names a store first seen at a character (minted there, reused); chat-2 names one first seen at the chat (minted between the chat scalars and its participants); docMountPoints then reuses both. Unknown keys are stripped (Zod), the key keeps its place',
      targetUserId: t,
      data: bag({
        characters: [
          { id: 'char-1', characterDocumentMountPointId: '5ce00000-0000-4000-8000-000000000001' },
        ],
        chats: [
          {
            id: 'chat-1',
            participants: [{ id: 'p-1', characterId: 'char-1' }],
            tags: [],
            scenarioSource: {
              mountPointId: '5ce00000-0000-4000-8000-000000000001',
              path: 'Scenarios/The Road.md',
              bodySha256: 'abababababababababababababababababababababababababababababababab',
              extra: 'stripped',
            },
            scenarioNotes: 'Notes ride along.',
            title: 'after the source',
            messages: [],
          },
          {
            id: 'chat-2',
            participants: [{ id: 'p-2', characterId: 'char-1' }],
            tags: [],
            scenarioSource: {
              mountPointId: '5ce00000-0000-4000-8000-000000000002',
              path: 'Scenarios/The Sea.md',
              bodySha256: 'abababababababababababababababababababababababababababababababab',
            },
            messages: [],
          },
        ],
        docMountPoints: [
          { id: '5ce00000-0000-4000-8000-000000000001' },
          { id: '5ce00000-0000-4000-8000-000000000002' },
        ],
      }),
    },
    {
      name: 'chat_scenario_source_json_string',
      note: '[P4.D293] a source carried as a JSON STRING (a raw row) is parsed, re-pointed, and lands as an OBJECT; a null / empty-string source is left as it came',
      targetUserId: t,
      data: bag({
        chats: [
          {
            id: 'chat-1',
            participants: [],
            tags: [],
            scenarioSource: JSON.stringify({
              mountPointId: '5ce00000-0000-4000-8000-000000000003',
              path: 'Scenarios/A String.md',
              bodySha256: 'abababababababababababababababababababababababababababababababab',
            }),
            messages: [],
          },
          { id: 'chat-2', participants: [], tags: [], scenarioSource: null, messages: [] },
          { id: 'chat-3', participants: [], tags: [], scenarioSource: '', messages: [] },
        ],
      }),
    },
    {
      name: 'chat_scenario_source_invalid',
      note: '[P4.D293] a source the schema refuses (a bad uuid; a short sha; unparseable JSON text) becomes null and mints nothing',
      targetUserId: t,
      data: bag({
        chats: [
          {
            id: 'chat-1',
            participants: [],
            tags: [],
            scenarioSource: { mountPointId: 'not-a-uuid', path: 'Scenarios/x.md', bodySha256: 'abababababababababababababababababababababababababababababababab' },
            messages: [],
          },
          {
            id: 'chat-2',
            participants: [],
            tags: [],
            scenarioSource: {
              mountPointId: '5ce00000-0000-4000-8000-000000000004',
              path: 'Scenarios/x.md',
              bodySha256: 'abc',
            },
            messages: [],
          },
          { id: 'chat-3', participants: [], tags: [], scenarioSource: '{not json', messages: [] },
        ],
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
        // P4.D264: `relativePath` is a NOT NULL column on every real link —
        // v4 `f5e953a3f`'s `buildWardrobeItemIdRemap` calls
        // `isWardrobeItemDocumentPath(link.relativePath)` on every link and
        // THROWS on an absent one, so this hand-built row now carries one.
        docMountFileLinks: [
          { id: 'link-1', fileId: 'dmf-1', mountPointId: 'mp-1', folderId: 'fol-2', relativePath: 'notes/a.md' },
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
    // ── P4.D264 (v4 `3ee3b1342` #81 + `7c8572869` #82) — the wardrobe shapes of
    // v4's own `uuid-remapper.test.ts:652-837`, widened. Wardrobe item ids
    // are NOT remapper keys: a frontmatter uuid maps to itself, a path-derived
    // id is recomputed against the remapped mount, a legacy row follows the
    // remapper; `buildWardrobeItemIdRemap` runs BEFORE `files` and mints the
    // Wardrobe-holding mount's id at that call.
    {
      name: 'wardrobe_wear_frontmatter_item',
      note: "a frontmatter item id is kept (the document is not rewritten); the row's id / wearer / chat are remapped and the wearer lands on the restored character's id; an unattributed row keeps its nulls; an itemId the backup carries no document for passes through UNCHANGED (never minted)",
      targetUserId: t,
      data: bag({
        characters: [{ id: 'char-old', userId: 'old-user' }],
        docMountFileLinks: [
          { id: 'wlink-1', fileId: 'wfile-1', mountPointId: '22222222-2222-4222-8222-222222222222', relativePath: 'Wardrobe/Coat.md' },
        ],
        docMountDocuments: [
          { id: 'wdoc-1', fileId: 'wfile-1', content: '---\nid: 11111111-1111-4111-8111-111111111111\ntitle: Coat\ntypes: [top]\n---\n' },
        ],
        wardrobeWear: [
          {
            id: 'row-old',
            itemId: '11111111-1111-4111-8111-111111111111',
            wearerCharacterId: 'char-old',
            wearCount: 4,
            firstWornAt: '2026-01-01T00:00:00.000Z',
            lastWornAt: '2026-02-01T00:00:00.000Z',
            lastWornChatId: 'chat-old',
            createdAt: '2026-01-01T00:00:00.000Z',
            updatedAt: '2026-02-01T00:00:00.000Z',
          },
          {
            id: 'row-unattributed',
            itemId: '11111111-1111-4111-8111-111111111111',
            wearerCharacterId: null,
            wearCount: 2,
            firstWornAt: '2026-01-02T00:00:00.000Z',
            lastWornAt: '2026-01-03T00:00:00.000Z',
            lastWornChatId: null,
            createdAt: '2026-01-02T00:00:00.000Z',
            updatedAt: '2026-01-03T00:00:00.000Z',
          },
          {
            id: 'row-orphan',
            itemId: 'not-a-carried-item',
            wearerCharacterId: 'char-departed',
            wearCount: 1,
            firstWornAt: '2026-01-04T00:00:00.000Z',
            lastWornAt: '2026-01-04T00:00:00.000Z',
            lastWornChatId: 'chat-old',
            createdAt: '2026-01-04T00:00:00.000Z',
            updatedAt: '2026-01-04T00:00:00.000Z',
          },
        ],
      }),
    },
    {
      name: 'wardrobe_wear_path_derived',
      note: "a path-derived item id (no frontmatter id, or one that is not uuid-shaped) is recomputed against the REMAPPED mount; the item-path test is case-insensitive and RECURSIVE (v4 4a38222e6) — a nested Wardrobe/Old/Coat.md is an item and its ledger row follows the remapped mount; instructions.md at any depth, anything under Wardrobe/images/, a Notes/*.md and a link whose document is missing are not items (their ledger rows pass through unchanged)",
      targetUserId: t,
      data: bag({
        docMountPoints: [{ id: 'mount-hat' }],
        docMountFileLinks: [
          { id: 'wl-hat', fileId: 'wf-hat', mountPointId: 'mount-hat', relativePath: 'Wardrobe/Hat.md' },
          { id: 'wl-scarf', fileId: 'wf-scarf', mountPointId: 'mount-hat', relativePath: 'WARDROBE/Scarf.MD' },
          { id: 'wl-instr', fileId: 'wf-instr', mountPointId: 'mount-hat', relativePath: 'Wardrobe/instructions.md' },
          { id: 'wl-nested', fileId: 'wf-nested', mountPointId: 'mount-hat', relativePath: 'Wardrobe/Old/Coat.md' },
          { id: 'wl-picdoc', fileId: 'wf-picdoc', mountPointId: 'mount-hat', relativePath: 'Wardrobe/images/hat-item/Coat.md' },
          { id: 'wl-ninstr', fileId: 'wf-ninstr', mountPointId: 'mount-hat', relativePath: 'Wardrobe/Formal/instructions.md' },
          { id: 'wl-notes', fileId: 'wf-notes', mountPointId: 'mount-hat', relativePath: 'Notes/Hat.md' },
          { id: 'wl-nodoc', fileId: 'wf-nodoc', mountPointId: 'mount-other', relativePath: 'Wardrobe/Ghost.md' },
        ],
        docMountDocuments: [
          { id: 'wd-hat', fileId: 'wf-hat', content: hatContent },
          { id: 'wd-scarf', fileId: 'wf-scarf', content: '---\nid: not-a-uuid\ntitle: Scarf\ntypes: [accessories]\n---\n' },
          { id: 'wd-instr', fileId: 'wf-instr', content: 'Dress for the weather.' },
          { id: 'wd-nested', fileId: 'wf-nested', content: nestedContent },
          { id: 'wd-picdoc', fileId: 'wf-picdoc', content: picDocContent },
          { id: 'wd-ninstr', fileId: 'wf-ninstr', content: 'Formal rules.' },
          { id: 'wd-notes', fileId: 'wf-notes', content: '---\ntitle: Notes Hat\n---\n' },
        ],
        wardrobeWear: [
          {
            id: 'row-hat',
            itemId: hatItemId,
            wearerCharacterId: null,
            wearCount: 3,
            firstWornAt: '2026-01-01T00:00:00.000Z',
            lastWornAt: '2026-03-01T00:00:00.000Z',
            lastWornChatId: null,
            createdAt: '2026-01-01T00:00:00.000Z',
            updatedAt: '2026-03-01T00:00:00.000Z',
          },
          {
            id: 'row-nested',
            itemId: nestedItemId,
            wearerCharacterId: null,
            wearCount: 2,
            firstWornAt: '2026-01-01T00:00:00.000Z',
            lastWornAt: '2026-03-01T00:00:00.000Z',
            lastWornChatId: null,
            createdAt: '2026-01-01T00:00:00.000Z',
            updatedAt: '2026-03-01T00:00:00.000Z',
          },
          {
            id: 'row-picdoc',
            itemId: picDocItemId,
            wearerCharacterId: null,
            wearCount: 1,
            firstWornAt: '2026-01-01T00:00:00.000Z',
            lastWornAt: '2026-03-01T00:00:00.000Z',
            lastWornChatId: null,
            createdAt: '2026-01-01T00:00:00.000Z',
            updatedAt: '2026-03-01T00:00:00.000Z',
          },
          {
            id: 'row-ninstr',
            itemId: nestedInstrItemId,
            wearerCharacterId: null,
            wearCount: 1,
            firstWornAt: '2026-01-01T00:00:00.000Z',
            lastWornAt: '2026-03-01T00:00:00.000Z',
            lastWornChatId: null,
            createdAt: '2026-01-01T00:00:00.000Z',
            updatedAt: '2026-03-01T00:00:00.000Z',
          },
        ],
      }),
    },
    {
      name: 'wardrobe_wear_legacy_row',
      note: "a legacy wardrobe_items row follows the remapper and its ledger row follows the row; the legacy row's imageFileId is now in remapFields' list and lands on the SAME minted id as the files row it names",
      targetUserId: t,
      data: bag({
        files: [{ id: 'file-legacy', linkedTo: ['legacy-item'], tags: ['legacy-item'] }],
        wardrobeItems: [
          { id: 'legacy-item', characterId: 'char-old', imageFileId: 'file-legacy', componentItemIds: [] },
          { id: 'legacy-nopic', characterId: 'char-old', imageFileId: null, componentItemIds: ['legacy-item'] },
        ],
        wardrobeWear: [
          {
            id: 'row-legacy',
            itemId: 'legacy-item',
            wearerCharacterId: 'char-old',
            wearCount: 7,
            firstWornAt: '2026-01-01T00:00:00.000Z',
            lastWornAt: '2026-02-01T00:00:00.000Z',
            lastWornChatId: 'chat-old',
            createdAt: '2026-01-01T00:00:00.000Z',
            updatedAt: '2026-02-01T00:00:00.000Z',
          },
        ],
      }),
    },
    {
      name: 'wardrobe_picture_links_restore_path',
      note: "the restore order (plan first, same remapper): a picture's files row keeps its item link on the item's unchanged id and remaps only the other links, element by element and key positions unchanged (a path-derived item id in linkedTo follows the RECOMPUTED id; a non-string element still goes through the remapper); exactly one pointer fix, onto the picture's new files id against the remapped mount",
      targetUserId: t,
      restorePath: true,
      data: bag({
        files: [
          // `7.5`, not `7`: an integer-like key ENUMERATES FIRST in the JS
          // object `getMapping()` folds into, an ordering v5's
          // `mapping_object` does not model (P4.D264 lane record) — it touches
          // no restored row, only this family's memo comparand.
          { id: 'file-old', linkedTo: ['11111111-1111-4111-8111-111111111111', 'chat-old'], tags: ['11111111-1111-4111-8111-111111111111'], userId: 'old-user' },
          { id: 'file-hat', linkedTo: [hatPicItemId, 7.5, null], tags: [], userId: 'old-user' },
        ],
        docMountPoints: [{ id: 'mount-old' }],
        docMountFileLinks: [
          { id: 'link-1', mountPointId: 'mount-old', fileId: 'docfile-1', relativePath: 'Wardrobe/Coat.md' },
          { id: 'link-2', mountPointId: 'mount-old', fileId: 'docfile-2', relativePath: 'Wardrobe/Hat.md' },
        ],
        docMountDocuments: [
          {
            id: 'doc-1',
            fileId: 'docfile-1',
            content: '---\nid: 11111111-1111-4111-8111-111111111111\ntitle: Coat\ntypes:\n  - top\nimageFileId: file-old\n---\nA coat.',
          },
          { id: 'doc-2', fileId: 'docfile-2', content: hatPicContent },
        ],
      }),
    },
    {
      name: 'wardrobe_picture_pointer_unplanned',
      note: "the restore order: a pointer naming a file the backup does not carry, a non-string pointer and an item with no pointer plan NOTHING; the plan still mints the Wardrobe-holding mount's id first",
      targetUserId: t,
      restorePath: true,
      data: bag({
        files: [{ id: 'file-other', linkedTo: [], tags: [] }],
        docMountFileLinks: [
          { id: 'link-a', mountPointId: 'mount-a', fileId: 'df-a', relativePath: 'Wardrobe/A.md' },
          { id: 'link-b', mountPointId: 'mount-a', fileId: 'df-b', relativePath: 'Wardrobe/B.md' },
          { id: 'link-c', mountPointId: 'mount-a', fileId: 'df-c', relativePath: 'Wardrobe/C.md' },
        ],
        docMountDocuments: [
          { id: 'doc-a', fileId: 'df-a', content: '---\ntitle: A\ntypes: [top]\nimageFileId: file-missing\n---\n' },
          { id: 'doc-b', fileId: 'df-b', content: '---\ntitle: B\ntypes: [top]\nimageFileId: 42\n---\n' },
          { id: 'doc-c', fileId: 'df-c', content: '---\ntitle: C\ntypes: [top]\n---\n' },
        ],
      }),
    },
    {
      name: 'wardrobe_items_filed_in_subfolders',
      note: "P4.D284 (v4 `4a38222e6`, jest `uuid-remapper.test.ts:940-1008`): an id-less item filed at Wardrobe/Formal/Evening/Coat.md is an item — its ledger row follows the id recomputed against the remapped mount, and its picture pointer is PLANNED (one fix); a document under Wardrobe/images/ beside it is not an item and plans nothing",
      targetUserId: t,
      restorePath: true,
      data: bag({
        files: [{ id: 'file-evening', linkedTo: [], tags: [] }],
        docMountPoints: [{ id: 'mount-evening' }],
        docMountFileLinks: [
          { id: 'link-ev', mountPointId: 'mount-evening', fileId: 'df-ev', relativePath: 'Wardrobe/Formal/Evening/Coat.md' },
          { id: 'link-evpic', mountPointId: 'mount-evening', fileId: 'df-evpic', relativePath: 'Wardrobe/images/ev-item/Note.md' },
        ],
        docMountDocuments: [
          { id: 'doc-ev', fileId: 'df-ev', content: eveningContent },
          { id: 'doc-evpic', fileId: 'df-evpic', content: '---\ntitle: Note\ntypes: [top]\nimageFileId: file-evening\n---\n' },
        ],
        wardrobeWear: [
          {
            id: 'row-evening',
            itemId: eveningItemId,
            wearerCharacterId: null,
            wearCount: 4,
            firstWornAt: '2026-01-01T00:00:00.000Z',
            lastWornAt: '2026-03-01T00:00:00.000Z',
            lastWornChatId: null,
            createdAt: '2026-01-01T00:00:00.000Z',
            updatedAt: '2026-03-01T00:00:00.000Z',
          },
        ],
      }),
    },
    {
      name: 'chat_settings_wardrobe_image_settings',
      note: "v4 `7c8572869` `uuid-remap.ts:440-445`: wardrobeImageSettings is guarded on its ONE id (truthy imageProfileId), spread-rewritten in place after storyBackgroundsSettings; a null id leaves the bag alone, an absent bag stays absent",
      targetUserId: t,
      data: bag({
        chatSettings: [
          {
            id: 'cs-w1',
            userId: 'old-user',
            storyBackgroundsSettings: { enabled: true, defaultImageProfileId: 'story-w' },
            wardrobeImageSettings: { imageProfileId: 'wardrobe-ip', generateFromTools: true },
          },
          { id: 'cs-w2', wardrobeImageSettings: { generateFromTools: false, imageProfileId: null } },
          { id: 'cs-w3', wardrobeImageSettings: { imageProfileId: '' } },
          { id: 'cs-w4' },
        ],
      }),
    },
    // [P4.D312] v4 `49f8906c3` (`uuid-remap.ts:174-198`, `:352-361`, `:373-381`):
    // the three turn-manager columns are JSON STRINGS naming seats, remapped by
    // `remapJsonIdList` AFTER the scenario-source arm and BEFORE the
    // participants (mint order is observable through the counter ids); a
    // message's `targetParticipantIds` (a whisper's audience) joins
    // `attachments` as an array field.
    {
      name: 'chat_turn_state_json_ids',
      note: "[P4.D312] v4's own uuid-remapper test 1 verbatim: turnQueue names seat-b FIRST, so seat-b is minted before seat-a (the participants' order) — the mint-order proof a loop placed after the participants fails; the whisper audience follows its seat",
      targetUserId: t,
      data: bag({
        chats: [
          {
            id: 'chat-old',
            participants: [{ id: 'seat-a' }, { id: 'seat-b' }],
            turnQueue: JSON.stringify(['seat-b']),
            spokenThisCycleParticipantIds: JSON.stringify(['seat-a']),
            cycleOrderParticipantIds: JSON.stringify(['seat-b', 'seat-a']),
            messages: [{ id: 'msg-old', participantId: 'seat-a', targetParticipantIds: ['seat-b'] }],
          },
        ],
      }),
    },
    {
      name: 'chat_turn_state_json_edges',
      note: '[P4.D312] one chat per edge: a non-JSON / empty / JSON-null / object / number-array / mixed-array string is returned UNTOUCHED (byte for byte); a pretty-printed list comes back COMPACT; a duplicate mints once; a non-seat id is minted anyway; a list naming a seat before the participants mints it first; a non-string value (null, a number, a REAL array) is untouched; an absent key stays absent',
      targetUserId: t,
      data: bag({
        chats: [
          { id: 'edge-not-json', participants: [], messages: [], turnQueue: 'not json' },
          { id: 'edge-empty-string', participants: [], messages: [], spokenThisCycleParticipantIds: '' },
          { id: 'edge-json-null', participants: [], messages: [], cycleOrderParticipantIds: 'null' },
          { id: 'edge-object', participants: [], messages: [], turnQueue: '{"a":"b"}' },
          { id: 'edge-numbers', participants: [], messages: [], turnQueue: '[1,2]' },
          { id: 'edge-mixed', participants: [{ id: 'seat-m' }], messages: [], turnQueue: '["seat-m",1]' },
          {
            id: 'edge-pretty',
            participants: [{ id: 'seat-p1' }, { id: 'seat-p2' }],
            messages: [],
            turnQueue: '[ "seat-p1" , "seat-p2" ]',
          },
          { id: 'edge-duplicate', participants: [], messages: [], spokenThisCycleParticipantIds: '["x","x"]' },
          {
            id: 'edge-not-a-seat',
            participants: [{ id: 'seat-n' }],
            messages: [],
            cycleOrderParticipantIds: '["ghost-seat","seat-n"]',
          },
          {
            id: 'edge-mint-order',
            participants: [{ id: 'seat-o1' }, { id: 'seat-o2' }, { id: 'seat-o3' }],
            messages: [],
            turnQueue: '["seat-o3"]',
            spokenThisCycleParticipantIds: '["seat-o2"]',
            cycleOrderParticipantIds: '["seat-o3","seat-o1"]',
          },
          { id: 'edge-null-value', participants: [], messages: [], turnQueue: null },
          { id: 'edge-number-value', participants: [], messages: [], turnQueue: 5 },
          { id: 'edge-real-array', participants: [{ id: 'seat-r' }], messages: [], turnQueue: ['seat-r'] },
          { id: 'edge-absent', participants: [{ id: 'seat-q' }], messages: [] },
        ],
      }),
    },
    {
      name: 'message_whisper_audience',
      note: "[P4.D312] targetParticipantIds is an ARRAY field beside attachments: a seat, an empty list, an unknown id (minted), null / a string / absent (untouched); one message with BOTH lists mints its attachment ids BEFORE its audience ids (`['attachments','targetParticipantIds']` order, after the scalars)",
      targetUserId: t,
      data: bag({
        chats: [
          {
            id: 'whisper-chat',
            participants: [{ id: 'seat-w1' }, { id: 'seat-w2' }],
            messages: [
              { id: 'w-msg-1', participantId: 'seat-w1', targetParticipantIds: ['seat-w2'] },
              { id: 'w-msg-2', participantId: 'seat-w1', targetParticipantIds: [] },
              { id: 'w-msg-3', participantId: 'seat-w1', targetParticipantIds: ['unknown-seat'] },
              { id: 'w-msg-4', participantId: 'seat-w2', targetParticipantIds: null },
              { id: 'w-msg-5', participantId: 'seat-w2', targetParticipantIds: 'seat-w2' },
              { id: 'w-msg-6', participantId: 'seat-w2' },
              {
                id: 'w-msg-7',
                participantId: 'seat-w1',
                attachments: ['att-new-1', 'att-new-2'],
                targetParticipantIds: ['aud-new-1'],
              },
            ],
          },
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
    ...outfitHistoryAndBug252Cases(),
  ];
}

/**
 * P4.D317 — the outfit-history arm (`7953da3a0`, `uuid-remap.ts:664-675`) and
 * bug 252's character-keyed chat pass (`e8e17b2d9`, `:331`, `:360-380`). v4's
 * own `uuid-remapper.test.ts` rows (`:1126`, `:1179`, `:1292`) plus the named
 * hand cases of the carriers survey §4.2. `historyRow` hashes through v4's REAL
 * `hashEquippedSlots`; path-derived item ids through v4's REAL parser.
 */
function outfitHistoryAndBug252Cases(): Case[] {
  const t = TARGET_USER_ID;
  const { hashEquippedSlots } = jest.requireActual('@/lib/wardrobe/outfit-hash') as {
    hashEquippedSlots: (slots: unknown) => string;
  };
  const empty = { top: [], bottom: [], footwear: [], accessories: [], hair: [] };
  const historyRow = (over: Record<string, unknown>): Record<string, unknown> => {
    const slots = (over.slots as Record<string, unknown>) ?? empty;
    return {
      id: 'hist-x',
      chatId: 'chat-old',
      characterId: 'char-a',
      entryIndex: 0,
      slots,
      slotsHash: hashEquippedSlots(slots),
      source: 'character',
      actorCharacterId: 'char-a',
      actorToolName: null,
      isCurrent: true,
      createdAt: '2026-10-16T00:00:00.000Z',
      ...over,
    };
  };
  const FRONTMATTER_ITEM = '33333333-3333-4333-8333-333333333333';
  const OLD_MOUNT = '44444444-4444-4444-8444-444444444444';
  const spatsContent = '---\ntitle: Spats\ntypes: [footwear]\n---\n';
  const oldSpats = pathDerivedItemId(OLD_MOUNT, 'Wardrobe/Spats.md', spatsContent);
  const wardrobeDocs = {
    docMountFileLinks: [
      { id: 'link-1', fileId: 'file-1', mountPointId: OLD_MOUNT, relativePath: 'Wardrobe/Coat.md' },
      { id: 'link-2', fileId: 'file-2', mountPointId: OLD_MOUNT, relativePath: 'Wardrobe/Spats.md' },
    ],
    docMountDocuments: [
      { id: 'doc-1', fileId: 'file-1', content: `---\nid: ${FRONTMATTER_ITEM}\ntitle: Coat\ntypes: [top]\n---\n` },
      { id: 'doc-2', fileId: 'file-2', content: spatsContent },
    ],
  };
  return [
    // ── v4's own rows ──
    {
      name: 'v4_outfit_history_five_entry_timeline',
      note: "v4 uuid-remapper.test.ts:1133 — a five-entry timeline under remapped ids: indices, cursor, the null first actor kept, hashes recomputed (equal: the items carry no document); the rows carry NO actorToolName key and the remap does not add one",
      targetUserId: t,
      data: bag({
        characters: [{ id: 'char-old', name: 'Bertie' }],
        chats: [{ id: 'chat-old', participants: [], messages: [] }],
        wardrobeHistory: [0, 1, 2, 3, 4].map((entryIndex) => {
          const row = historyRow({
            id: `hist-${entryIndex}`,
            characterId: 'char-old',
            entryIndex,
            slots: { ...empty, top: [`item-${entryIndex}`] },
            source: entryIndex === 0 ? 'chat-start' : 'character',
            actorCharacterId: entryIndex === 0 ? null : 'char-old',
            isCurrent: entryIndex === 2,
            createdAt: `2026-10-16T0${entryIndex}:00:00.000Z`,
          });
          delete row.actorToolName;
          return row;
        }),
      }),
    },
    {
      name: 'v4_bug252_re_keys_every_character',
      note: 'v4 uuid-remapper.test.ts:1207 — both carried characters re-keyed; an item with no document passes through; the outfit becomes a full five-slot bag',
      targetUserId: t,
      data: bag({
        characters: [{ id: 'char-a', name: 'Bertie' }, { id: 'char-b', name: 'Jeeves' }],
        chats: [{
          id: 'chat-old', participants: [], messages: [],
          equippedOutfit: { 'char-a': { ...empty, top: ['item-unknown'] }, 'char-b': { ...empty, footwear: ['item-other'] } },
        }],
      }),
    },
    {
      name: 'v4_bug252_moves_slot_item_ids',
      note: 'v4 uuid-remapper.test.ts:1227 — a frontmatter item kept, a path-derived item recomputed against the remapped mount',
      targetUserId: t,
      data: bag({
        characters: [{ id: 'char-a', name: 'Bertie' }, { id: 'char-b', name: 'Jeeves' }],
        chats: [{
          id: 'chat-old', participants: [], messages: [],
          equippedOutfit: { 'char-a': { ...empty, top: [FRONTMATTER_ITEM], footwear: [oldSpats] } },
        }],
        ...wardrobeDocs,
      }),
    },
    {
      name: 'v4_bug252_agrees_with_the_history',
      note: 'v4 uuid-remapper.test.ts:1262 — the chat outfit and the history row land on the SAME new character id and the same slots',
      targetUserId: t,
      data: bag({
        characters: [{ id: 'char-a', name: 'Bertie' }, { id: 'char-b', name: 'Jeeves' }],
        chats: [{ id: 'chat-old', participants: [], messages: [], equippedOutfit: { 'char-a': { ...empty, top: ['item-1'] } } }],
        wardrobeHistory: [(() => {
          const r = historyRow({ id: 'hist-0', slots: { ...empty, top: ['item-1'] }, source: 'chat-start', actorCharacterId: null });
          delete r.actorToolName;
          return r;
        })()],
      }),
    },
    {
      name: 'v4_bug252_leaves_a_chat_with_no_outfit_alone',
      note: 'v4 uuid-remapper.test.ts:1281 — a null equippedOutfit stays null, an absent one stays absent; no DEBUG for either',
      targetUserId: t,
      data: bag({
        characters: [{ id: 'char-a', name: 'Bertie' }],
        chats: [
          { id: 'chat-null', participants: [], messages: [], equippedOutfit: null },
          { id: 'chat-absent', participants: [], messages: [] },
        ],
      }),
    },
    {
      name: 'v4_bug252_other_columns',
      note: "v4 uuid-remapper.test.ts:1307-1360, one chat per `it`: the avatar follows its file (already minted by the files pass — no new mint), the Courier checkpoint follows its message (MINTED at the chat, reused by the message), a non-carried character's entry is DROPPED (droppedEntries), a queued notice re-keyed",
      targetUserId: t,
      data: bag({
        characters: [{ id: 'char-a', name: 'Bertie' }],
        files: [{ id: 'file-old' }],
        chats: [
          { id: 'chat-avatar', participants: [], messages: [{ id: 'msg-a', type: 'message' }], characterAvatars: { 'char-a': { imageId: 'file-old', generatedAt: 't', afterMessageCount: 2 } } },
          { id: 'chat-courier', participants: [], messages: [{ id: 'msg-old', type: 'message' }], courierCheckpoints: { 'char-a': { lastResolvedMessageId: 'msg-old', resolvedAt: 't' } } },
          { id: 'chat-drop', participants: [], messages: [], equippedOutfit: { 'char-a': { top: ['x'] }, 'char-deleted': { top: ['y'] } } },
          { id: 'chat-notice', participants: [], messages: [], pendingOutfitNotifications: { 'char-a': { text: 'changed' } } },
        ],
      }),
    },
    // ── the carriers survey §4.2 hand cases ──
    {
      name: 'outfit_history_timeline',
      note: 'five entries, the cursor mid-timeline, a null actor, an actor, a pascal row WITH actorToolName (copied), a pre-4ae20977f row with NO actorToolName key (absent in the output); a stale stored hash recomputed',
      targetUserId: t,
      data: bag({
        characters: [{ id: 'char-a' }, { id: 'char-b' }],
        chats: [{ id: 'chat-old', participants: [], messages: [] }],
        wardrobeHistory: [
          historyRow({ id: 'h-0', entryIndex: 0, source: 'chat-start', actorCharacterId: null, isCurrent: false }),
          historyRow({ id: 'h-1', entryIndex: 1, source: 'operator', actorCharacterId: 'char-b', isCurrent: false, slots: { ...empty, top: ['item-1'] } }),
          historyRow({ id: 'h-2', entryIndex: 2, source: 'pascal', actorCharacterId: 'char-a', actorToolName: 'dress_for_dinner', isCurrent: true, slotsHash: 'stalehash0000000' }),
          (() => { const r = historyRow({ id: 'h-3', entryIndex: 3, isCurrent: false }); delete r.actorToolName; return r; })(),
          historyRow({ id: 'h-4', entryIndex: 4, source: 'seeded', actorCharacterId: null, isCurrent: false }),
        ],
      }),
    },
    {
      name: 'outfit_history_items',
      note: 'slot item ids through the wear ledger\'s item remap: a frontmatter item kept, a path-derived item recomputed against the remapped mount, an unknown item passing through',
      targetUserId: t,
      data: bag({
        characters: [{ id: 'char-a' }],
        chats: [{ id: 'chat-old', participants: [], messages: [] }],
        ...wardrobeDocs,
        wardrobeHistory: [historyRow({ id: 'h-items', slots: { ...empty, top: [FRONTMATTER_ITEM], footwear: [oldSpats], accessories: ['item-unknown'] } })],
      }),
    },
    {
      name: 'outfit_history_phantom_character',
      note: "R-9 (a), v4-faithful: a characterId (and actor) the backup does not carry is MINTED by remapFields — after the row's id and chatId — where bug 252's chat pass DROPS the same character's entry",
      targetUserId: t,
      data: bag({
        characters: [{ id: 'char-a' }],
        chats: [{ id: 'chat-old', participants: [], messages: [], equippedOutfit: { 'char-gone': { ...empty } } }],
        wardrobeHistory: [historyRow({ id: 'h-phantom', characterId: 'char-gone', actorCharacterId: 'char-gone-too' })],
      }),
    },
    {
      name: 'outfit_history_slot_shapes',
      note: 'NO normalize in the remap: a missing slot and a null slot read [], a four-slot pre-hair bag gains hair, a null / string / array `slots` reads every slot [], a non-string element passes through and is hashed as given',
      targetUserId: t,
      data: bag({
        characters: [{ id: 'char-a' }],
        chats: [{ id: 'chat-old', participants: [], messages: [] }],
        wardrobeHistory: [
          historyRow({ id: 'h-missing', entryIndex: 0, isCurrent: false, slots: { top: ['item-1'], footwear: null } }),
          historyRow({ id: 'h-four', entryIndex: 1, isCurrent: false, slots: { top: [], bottom: ['item-2'], footwear: [], accessories: [] } }),
          historyRow({ id: 'h-null', entryIndex: 2, isCurrent: false, slots: null }),
          historyRow({ id: 'h-string', entryIndex: 3, isCurrent: false, slots: 'not an object' }),
          historyRow({ id: 'h-array', entryIndex: 4, isCurrent: false, slots: [['item-3']] }),
          historyRow({ id: 'h-elements', entryIndex: 5, isCurrent: true, slots: { ...empty, top: ['item-1', 7, null, { k: 1 }] } }),
        ],
      }),
    },
    {
      name: 'outfit_history_non_array_slot_throws',
      note: "R-9 (b), v4-faithful: a non-array slot value makes the arm's `.map` THROW, aborting the WHOLE remapBackupData (a new-account restore of such an archive fails before any write); the mapping minted before the throw is recorded",
      targetUserId: t,
      data: bag({
        characters: [{ id: 'char-a' }],
        chats: [{ id: 'chat-old', participants: [], messages: [] }],
        wardrobeHistory: [historyRow({ id: 'h-ok', entryIndex: 0, isCurrent: false }), historyRow({ id: 'h-bad', entryIndex: 1, slots: { ...empty, top: 'item-1' } })],
      }),
    },
    {
      name: 'outfit_history_null_row_throws',
      note: 'a null history row throws (`row.slots` on null) — the same whole-remap refusal',
      targetUserId: t,
      data: bag({
        characters: [{ id: 'char-a' }],
        chats: [{ id: 'chat-old', participants: [], messages: [] }],
        wardrobeHistory: [null],
      }),
    },
    {
      name: 'chat_keyed_bug252',
      note: "all four columns on one chat: a carried character re-keyed, a NON-carried one DROPPED (droppedEntries), an avatar imageId for a carried file (no mint) and for an unseen file (MINTS at the chat), a Courier lastResolvedMessageId naming one of the chat's own messages (MINTS before the participants — the mint-order proof; the message reuses it), a four-slot outfit gaining hair, a malformed slot salvaged",
      targetUserId: t,
      data: bag({
        characters: [{ id: 'char-a' }, { id: 'char-b' }],
        files: [{ id: 'file-carried' }],
        chats: [{
          id: 'chat-old',
          participants: [{ id: 'seat-a', characterId: 'char-a' }, { id: 'seat-b', characterId: 'char-b' }],
          messages: [{ id: 'msg-1', participantId: 'seat-a' }, { id: 'msg-2', participantId: 'seat-b' }],
          equippedOutfit: {
            'char-a': { top: ['item-1'], bottom: [], footwear: [], accessories: [] },
            'char-gone': { ...empty, top: ['item-9'] },
            'char-b': { ...empty, top: 'item-2', hair: ['item-3', 5] },
          },
          characterAvatars: {
            'char-b': { imageId: 'file-unseen', generatedAt: 'g2', afterMessageCount: 4 },
            'char-a': { imageId: 'file-carried', generatedAt: 'g1', afterMessageCount: 1 },
            'char-gone': { imageId: 'file-gone' },
          },
          courierCheckpoints: { 'char-b': { lastResolvedMessageId: 'msg-2', resolvedAt: 'r' } },
          pendingOutfitNotifications: { 'char-a': { text: 'changed' }, 'char-gone': 'x' },
        }],
      }),
    },
    {
      name: 'chat_keyed_edges',
      note: "one chat per edge of the pure pass: a column as a JSON string (parsed); 'null' as a string, a number, an array, true (each UNREADABLE — the WARN, the column left as given); {} (patch {}, the DEBUG fires); null and absent (skipped); an outfit entry that is a JSON string (five empty slots); an avatar entry that is a JSON string with a string imageId (an OBJECT, minted); an avatar imageId that is a number (kept)",
      targetUserId: t,
      data: bag({
        characters: [{ id: 'char-a' }],
        chats: [
          { id: 'edge-json-string', participants: [], messages: [], pendingOutfitNotifications: JSON.stringify({ 'char-a': 'n' }) },
          { id: 'edge-null-string', participants: [], messages: [], characterAvatars: 'null' },
          { id: 'edge-unreadable-many', participants: [], messages: [], equippedOutfit: 5, characterAvatars: [], courierCheckpoints: true },
          { id: 'edge-empty-object', participants: [], messages: [], equippedOutfit: {} },
          { id: 'edge-null-and-absent', participants: [], messages: [], equippedOutfit: null },
          { id: 'edge-outfit-entry-string', participants: [], messages: [], equippedOutfit: { 'char-a': JSON.stringify({ ...empty, top: ['item-1'] }) } },
          { id: 'edge-avatar-entry-string', participants: [], messages: [], characterAvatars: { 'char-a': JSON.stringify({ imageId: 'file-s', generatedAt: 'g' }) } },
          { id: 'edge-avatar-number', participants: [], messages: [], characterAvatars: { 'char-a': { imageId: 42 } } },
        ],
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

  // [P4.D312] Self-check the planted turn-state strings (memory note
  // `tier-r-vacuous-equal-rows`): a malformed plant that both sides leave alone
  // would pass vacuously, so assert each edge really is the shape it claims.
  {
    const byName = (n: string) => cases.find((c) => c.name === n)!;
    const chatOf = (c: Case, id: string) =>
      (c.data.chats as Array<Record<string, unknown>>).find((x) => x.id === id)!;
    const edges = byName('chat_turn_state_json_edges');
    const isJson = (v: string): boolean => {
      try {
        JSON.parse(v);
        return true;
      } catch {
        return false;
      }
    };
    const check = (ok: boolean, what: string) => {
      if (!ok) throw new Error(`P4.D312 corpus self-check failed: ${what}`);
    };
    check(!isJson(chatOf(edges, 'edge-not-json').turnQueue as string), "'not json' parses");
    check(chatOf(edges, 'edge-empty-string').spokenThisCycleParticipantIds === '', "'' is not empty");
    check(JSON.parse(chatOf(edges, 'edge-json-null').cycleOrderParticipantIds as string) === null, "'null' is not JSON null");
    check(!Array.isArray(JSON.parse(chatOf(edges, 'edge-object').turnQueue as string)), 'the object row is an array');
    check((JSON.parse(chatOf(edges, 'edge-numbers').turnQueue as string) as unknown[]).every((x) => typeof x === 'number'), 'the number row holds a non-number');
    const pretty = chatOf(edges, 'edge-pretty').turnQueue as string;
    check(pretty.includes(' ') && pretty !== JSON.stringify(JSON.parse(pretty)), 'the pretty row is already compact');
    check(Array.isArray(chatOf(edges, 'edge-real-array').turnQueue), 'the real-array row is not an array');
    check(!('turnQueue' in chatOf(edges, 'edge-absent')), 'the absent row carries turnQueue');
    const lone = byName('chat_turn_state_json_ids');
    check(JSON.parse(chatOf(lone, 'chat-old').turnQueue as string)[0] === 'seat-b', 'test 1 no longer names seat-b first');
  }

  // The committed corpus — the INPUT half of the contract.
  const corpus = {
    _meta: {
      baseline: 'f5e953a3f',
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
  const { remapBackupData, planWardrobeImagePointerFixes } = await import(
    '@/lib/backup/restore/uuid-remap'
  );

  // [P4.D317] The module's two `backup:uuid-remap` lines (`e8e17b2d9`), any
  // level, recorded per case (`logs`) for the Rust side's capture compare.
  const UUID_REMAP_LOGGED = new Set([
    'Chat character-keyed columns unreadable; left as given',
    'Remapped character-keyed chat fields',
  ]);
  const { Logger } = await import('@/lib/logger');
  const levels = ['error', 'warn', 'info', 'debug'] as const;
  const originals = Object.fromEntries(levels.map((l) => [l, Logger.prototype[l]]));
  let caseLogs: Array<Record<string, unknown>> = [];
  for (const level of levels) {
    const original = originals[level];
    Logger.prototype[level] = function (this: unknown, message: string, context?: Record<string, unknown>, ...rest: unknown[]) {
      if (UUID_REMAP_LOGGED.has(message)) caseLogs.push({ level, message, ...(context ?? {}) });
      return (original as (...a: unknown[]) => void).call(this, message, context, ...rest);
    } as never;
  }

  const outLines: string[] = [];
  for (const c of cases) {
    counter = 0;
    caseLogs = [];
    const remapper = new UuidRemapper();
    // A deep clone per case: `remapBackupData` must never mutate its input, and
    // the corpus we just wrote is the Rust side's input too.
    const input = JSON.parse(JSON.stringify(c.data));
    // P4.D264: the restore's order — plan on the ORIGINAL data first, same
    // remapper (`restore.ts:98-104`).
    const fixes = c.restorePath ? planWardrobeImagePointerFixes(input, remapper) : undefined;
    // [P4.D317 R-9 (b)] the outfit-history arm can THROW: recorded as `error`
    // (no `output`), with the mapping minted before the throw.
    let output: unknown;
    let error: string | undefined;
    try {
      output = remapBackupData(input, c.targetUserId, remapper);
    } catch (e) {
      error = e instanceof Error ? e.message : String(e);
    }
    outLines.push(
      JSON.stringify({
        name: c.name,
        corpusSha256,
        ...(fixes !== undefined ? { fixes } : {}),
        ...(error !== undefined ? { error } : { output }),
        mapping: remapper.getMapping(),
        size: remapper.getSize(),
        inputUnchanged: JSON.stringify(input) === JSON.stringify(c.data),
        logs: caseLogs,
      }),
    );
  }
  for (const level of levels) Logger.prototype[level] = originals[level] as never;
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
