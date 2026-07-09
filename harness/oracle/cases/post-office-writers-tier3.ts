/**
 * Tier-3 ORACLE for the Post Office persona-writer POST functions + the
 * cost/system-event writers (W4.6b).
 *
 * Drives v4's REAL `post*` writer functions (concierge / prospero / host / aurora /
 * suparna / commonplace / librarian) and the REAL `createContextSummaryEvent` /
 * `createTitleGenerationEvent` (system-events.service) against the REAL two-DB
 * fixture, then dumps the resulting `chat_messages` rows (byte-for-byte the
 * persona-message marshaling) plus the `chats` row (the token aggregate the cost
 * events bump). NO seams are mocked — the writers are self-contained (read the
 * chat, build content, insert through `repos.chats.addMessage`); this runs under
 * plain `tsx` (like the characters-create oracle), not jest.
 *
 * Each case is dispatched by `kind`; the persona `content` builders are already
 * tier-1-proven, so this verifies the ROW marshaling (systemSender / systemKind /
 * role / participantId / opaqueContent / hostEvent / targetParticipantIds /
 * summaryAnchor / attachments) the post functions assemble + persist.
 *
 * Run (Node 24, from the v4 checkout), AFTER building the fixtures:
 *   N=~/.nvm/versions/node/v24.13.1/bin
 *   cd ~/source/quilltap-server
 *   QT_FIXTURE_POW_MAIN=/tmp/qt-pow-main.db \
 *   QT_FIXTURE_POW_MOUNT=/tmp/qt-pow-mount.db \
 *     $N/npx tsx ~/source/quilltap-v5/harness/oracle/cases/post-office-writers-tier3.ts \
 *     > /tmp/oracle-pow.ndjson
 */

import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import { mkdtempSync, mkdirSync, readFileSync, copyFileSync, existsSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { canonicalizeRows } from '../lib/tier2.js';

interface Usage {
  promptTokens?: number;
  completionTokens?: number;
  totalTokens?: number;
}
interface OffSceneCard {
  id: string;
  name: string;
  aliases?: string[];
  pronouns?: { subject: string; object: string; possessive: string } | null;
  identity?: string | null;
  description?: string | null;
}
interface DangerDetails {
  score: number;
  threshold: number;
  categories: Array<{ category: string; score: number; label?: string }>;
  source?: 'moderation' | 'llm';
  providerName?: string;
}
interface OutfitSlots {
  top: string[];
  bottom: string[];
  footwear: string[];
  accessories: string[];
}
interface Case {
  name: string;
  kind: string;
  details?: DangerDetails;
  manualKind?: string;
  characterName?: string;
  oldProfileLabel?: string;
  newProfileLabel?: string;
  participantRef?: string;
  characterRef?: string;
  oldStatus?: string;
  newStatus?: string;
  initialStatus?: string;
  offSceneCharacters?: OffSceneCard[];
  scenarioText?: string;
  targetRef?: string;
  content?: string;
  opaqueContent?: string;
  whisperKind?: string;
  outfit?: OutfitSlots;
  summary?: string;
  targetRefs?: string[];
  summaryAnchor?: number | null;
  displayTitle?: string;
  filePath?: string;
  scope?: string;
  mountPoint?: string;
  isNew?: boolean;
  originBy?: string;
  originCharacterName?: string;
  turnPassSource?: string;
  usage?: Usage;
  provider?: string;
  modelName?: string;
  estimatedCostUSD?: number;
}
interface CharSeed {
  id: string;
  name: string;
}
interface Spec {
  testPepperBase64: string;
  seedTimestamp: string;
  userId: string;
  chat: { id: string; title: string };
  ada: CharSeed;
  charlie: CharSeed;
  p1Id: string;
  p2Id: string;
  cases: Case[];
}

async function main(): Promise<void> {
  const here = dirname(fileURLToPath(import.meta.url));
  const spec = JSON.parse(
    readFileSync(join(here, '..', 'fixtures', 'post-office-writers-tier3.json'), 'utf8')
  ) as Spec;

  const mainFixture = process.env.QT_FIXTURE_POW_MAIN;
  const mountFixture = process.env.QT_FIXTURE_POW_MOUNT;
  if (!mainFixture || !existsSync(mainFixture) || !mountFixture || !existsSync(mountFixture)) {
    throw new Error(
      'QT_FIXTURE_POW_MAIN and QT_FIXTURE_POW_MOUNT must point at the fixtures from build-post-office-writers-fixture.ts'
    );
  }

  const scratch = mkdtempSync(join(tmpdir(), 'qt-pow-oracle-'));
  mkdirSync(join(scratch, 'data'), { recursive: true });
  const mainWork = join(scratch, 'pow-main-work.db');
  const mountWork = join(scratch, 'pow-mount-work.db');
  copyFileSync(mainFixture, mainWork);
  copyFileSync(mountFixture, mountWork);

  process.env.ENCRYPTION_MASTER_PEPPER = spec.testPepperBase64;
  process.env.SQLITE_PATH = mainWork;
  process.env.SQLITE_MOUNT_INDEX_PATH = mountWork;
  process.env.QUILLTAP_DATA_DIR = scratch;
  delete process.env.SQLITE_WAL_MODE;
  process.env.LOG_LEVEL = 'error';

  const { initializeDatabase, closeDatabase, rawQuery } = await import('@/lib/database/manager');
  const { getRepositories } = await import('@/lib/repositories/factory');
  const concierge = await import('@/lib/services/concierge-notifications/writer');
  const prospero = await import('@/lib/services/prospero-notifications/writer');
  const host = await import('@/lib/services/host-notifications/writer');
  const auroraWriter = await import('@/lib/services/aurora-notifications/writer');
  const auroraCore = await import('@/lib/services/aurora-notifications/core-whisper');
  const suparna = await import('@/lib/services/suparna-notifications/writer');
  const commonplace = await import('@/lib/services/commonplace-notifications/writer');
  const librarian = await import('@/lib/services/librarian-notifications/writer');
  const systemEvents = await import('@/lib/services/system-events.service');

  await initializeDatabase();
  const repos = getRepositories();
  const chatId = spec.chat.id;

  const participantRef = (ref?: string): string => {
    if (ref === 'p1') return spec.p1Id;
    if (ref === 'p2') return spec.p2Id;
    throw new Error(`unknown participant ref ${ref}`);
  };

  for (const c of spec.cases) {
    switch (c.kind) {
      case 'conciergeDanger':
        await concierge.postConciergeDangerAnnouncement({ chatId, details: c.details });
        break;
      case 'conciergeManual':
        await concierge.postConciergeManualAnnouncement({
          chatId,
          kind: c.manualKind as never,
        });
        break;
      case 'prosperoConnProfile':
        await prospero.postProsperoConnectionProfileChangeAnnouncement({
          chatId,
          characterName: c.characterName!,
          oldProfileLabel: c.oldProfileLabel,
          newProfileLabel: c.newProfileLabel,
        });
        break;
      case 'hostStatusChange':
        await host.postHostStatusChangeAnnouncement({
          chatId,
          characterName: c.characterName!,
          participantId: participantRef(c.participantRef),
          oldStatus: c.oldStatus as never,
          newStatus: c.newStatus as never,
        });
        break;
      case 'hostAdd': {
        const character = await repos.characters.findById(spec.ada.id);
        if (!character) throw new Error('host_add: Ada character not found');
        await host.postHostAddAnnouncement({
          chatId,
          character,
          participantId: participantRef(c.participantRef),
          initialStatus: c.initialStatus as never,
        });
        break;
      }
      case 'hostOffScene':
        await host.postHostOffSceneCharactersAnnouncement({
          chatId,
          characters: (c.offSceneCharacters ?? []) as never,
        });
        break;
      case 'hostScenario':
        await host.postHostScenarioAnnouncement({ chatId, scenarioText: c.scenarioText! });
        break;
      case 'hostTurnPass':
        await host.postHostTurnPassAnnouncement({
          chatId,
          characterName: c.characterName!,
          participantId: participantRef(c.participantRef),
          source: c.turnPassSource as never,
        });
        break;
      case 'auroraCoreWhisper':
        await auroraCore.postCoreWhisper({
          chatId,
          targetParticipantId: participantRef(c.targetRef),
          content: c.content!,
          opaqueContent: c.opaqueContent!,
        });
        break;
      case 'auroraOutfitChange':
        await auroraWriter.postOutfitChangeWhisper({
          chatId,
          characterName: c.characterName!,
          outfit: c.outfit as never,
        });
        break;
      case 'suparnaMail':
        await suparna.postSuparnaMailWhisper({
          chatId,
          targetParticipantId: participantRef(c.targetRef),
          content: c.content!,
        });
        break;
      case 'commonplaceTargeted':
        await commonplace.postCommonplaceWhisper({
          chatId,
          targetParticipantId: participantRef(c.targetRef),
          content: c.content!,
          opaqueContent: c.opaqueContent,
          kind: c.whisperKind as never,
        });
        break;
      case 'commonplacePublicNullOpaque':
        await commonplace.postCommonplaceWhisper({
          chatId,
          content: c.content!,
          kind: c.whisperKind as never,
        });
        break;
      case 'librarianSummary':
        await librarian.postLibrarianSummaryAnnouncement({
          chatId,
          summary: c.summary!,
          targetParticipantIds: (c.targetRefs ?? []).map(participantRef),
          summaryAnchor:
            c.summaryAnchor === null || c.summaryAnchor === undefined
              ? null
              : { compactionGeneration: c.summaryAnchor },
        });
        break;
      case 'librarianOpen':
        await librarian.postLibrarianOpenAnnouncement({
          chatId,
          displayTitle: c.displayTitle!,
          filePath: c.filePath!,
          scope: c.scope as never,
          mountPoint: c.mountPoint,
          isNew: c.isNew ?? false,
          origin:
            c.originBy === 'character'
              ? { kind: 'opened-by-character', characterName: c.originCharacterName! }
              : { kind: 'opened-by-user' },
        });
        break;
      case 'costContextSummary':
        await systemEvents.createContextSummaryEvent(
          chatId,
          c.usage ?? null,
          c.provider,
          c.modelName,
          c.estimatedCostUSD
        );
        break;
      case 'costTitleGeneration':
        await systemEvents.createTitleGenerationEvent(
          chatId,
          c.usage ?? null,
          c.provider,
          c.modelName,
          c.estimatedCostUSD
        );
        break;
      default:
        throw new Error(`unknown case kind ${c.kind}`);
    }
  }

  const dumpTable = async (table: string, orderBy: string) => {
    const columns = (
      (await rawQuery(`PRAGMA table_info(${table})`)) as Array<{ name: string }>
    ).map((c) => c.name);
    const rawRows = (await rawQuery(`SELECT * FROM ${table}`)) as Array<Record<string, unknown>>;
    return canonicalizeRows({ table, columns, rawRows, orderBy });
  };

  // chat_messages ordered by `content` (each persona body distinct; the SYSTEM
  // cost rows carry NULL content and tie at the front, preserving insertion order
  // — deterministic on both sides). `chats` has a single row.
  const chatMessages = await dumpTable('chat_messages', 'content');
  const chats = await dumpTable('chats', 'id');

  await closeDatabase();

  process.stdout.write(JSON.stringify({ case: 'post-office-writers-tier3', chatMessages, chats }) + '\n');
  process.exit(0);
}

main().catch((err) => {
  process.stderr.write(`post-office-writers-tier3 oracle failed: ${err?.stack ?? err}\n`);
  process.exit(1);
});
