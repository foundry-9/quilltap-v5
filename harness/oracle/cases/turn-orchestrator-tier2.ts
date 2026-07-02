/**
 * Tier-2 oracle case — the turn-orchestration decision core (v4
 * `lib/services/chat-message/turn-orchestrator.service.ts` `shouldChainNext` /
 * `persistTurnParticipantId`, and the `actions/turn.ts` `handleTurnAction`
 * mutation core).
 *
 * These read/write the DB via `getRepositories()` (no LLM anywhere — turn
 * selection is deterministic once `Math.random` is pinned), so — like the
 * memory-delete case — this runs directly under the tsx real-DB path (no jest).
 *
 * `shouldChainNext` is imported and called directly. The `handleTurnAction`
 * mutation core is reproduced INLINE here (v4 `actions/turn.ts:31–184` minus the
 * HTTP shell — auth/validation/response-envelope), driving the SAME real
 * turn-manager pure functions + repo reads/writes the route uses, so the ported
 * Rust `handle_turn_action` is diffed against v4's real behavior.
 *
 * INJECTION: `Math.random` is pinned to each op's `random01` (the sole impurity
 * inside `selectNextSpeaker`); the Rust side injects the identical value. The
 * wall clock in `shouldChainNext` is passed explicitly (`nowMs` / `chainStartTimeMs`).
 *
 * NORMALIZATION: none. The seed pins every id + timestamp; the ops never mint a
 * chat `updatedAt` (v4 `_update` preserves it), so the final `chats` dump and each
 * op's returned decision object are compared exactly.
 *
 * Run (Node 24, from the v4 checkout), AFTER building the fixture:
 *   N=~/.nvm/versions/node/v24.13.1/bin
 *   cd ~/source/quilltap-server
 *   QT_FIXTURE_TURNORCH=/tmp/qt-turnorch-main.db \
 *   QT_FIXTURE_TURNORCH_MOUNT=/tmp/qt-turnorch-mount.db \
 *     $N/npx tsx ~/source/quilltap-v5/harness/oracle/cases/turn-orchestrator-tier2.ts > /tmp/oracle-turnorch.ndjson
 */

import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import { mkdtempSync, mkdirSync, readFileSync, copyFileSync, existsSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { canonicalizeRows } from '../lib/tier2.js';

interface Op {
  kind: 'shouldChainNext' | 'turnAction';
  chatId: string;
  // shouldChainNext
  userParticipantId?: string | null;
  chainDepth?: number;
  nowMs?: number;
  chainStartTimeMs?: number;
  // turnAction
  action?: 'nudge' | 'queue' | 'dequeue' | 'skipUserTurn' | 'query';
  participantId?: string | null;
  random01: number;
  note?: string;
}
interface Spec {
  testPepperBase64: string;
  seedTimestamp: string;
  ops: Op[];
}

async function main(): Promise<void> {
  const here = dirname(fileURLToPath(import.meta.url));
  const specPath = join(here, '..', 'fixtures', 'turn-orchestrator-tier2.json');
  const spec = JSON.parse(readFileSync(specPath, 'utf8')) as Spec;

  const fixture = process.env.QT_FIXTURE_TURNORCH;
  const fixtureMount = process.env.QT_FIXTURE_TURNORCH_MOUNT;
  if (!fixture || !existsSync(fixture) || !fixtureMount || !existsSync(fixtureMount)) {
    throw new Error(
      'QT_FIXTURE_TURNORCH and QT_FIXTURE_TURNORCH_MOUNT must point at the seed fixtures',
    );
  }

  const scratch = mkdtempSync(join(tmpdir(), 'qt-turnorch-oracle-'));
  mkdirSync(join(scratch, 'data'), { recursive: true });
  const work = join(scratch, 'turnorch-main.db');
  const workMount = join(scratch, 'turnorch-mount.db');
  copyFileSync(fixture, work);
  copyFileSync(fixtureMount, workMount);

  process.env.ENCRYPTION_MASTER_PEPPER = spec.testPepperBase64;
  process.env.SQLITE_PATH = work;
  process.env.SQLITE_MOUNT_INDEX_PATH = workMount;
  process.env.QUILLTAP_DATA_DIR = scratch;
  delete process.env.SQLITE_WAL_MODE;
  process.env.LOG_LEVEL = 'error';

  const { initializeDatabase, closeDatabase, rawQuery } = await import('@/lib/database/manager');
  const { closeMountIndexSQLiteClient } = await import(
    '@/lib/database/backends/sqlite/mount-index-client'
  );
  const { getRepositories } = await import('@/lib/repositories/factory');
  const { shouldChainNext } = await import(
    '@/lib/services/chat-message/turn-orchestrator.service'
  );
  const {
    selectNextSpeaker,
    calculateTurnStateFromHistory,
    nudgeParticipant,
    addToQueue,
    removeFromQueue,
    getQueuePosition,
    getActiveCharacterParticipants,
    findUserParticipant,
    getSelectionExplanation,
    computeSpokenThisCycleAfterSkip,
    isUsersTurn,
  } = await import('@/lib/chat/turn-manager');

  await initializeDatabase();
  const repos = getRepositories();

  const results: Array<Record<string, unknown>> = [];
  const origRandom = Math.random;

  // Inline reproduction of `handleTurnAction`'s decision/mutation core
  // (actions/turn.ts:31–184 minus the HTTP shell).
  async function runTurnAction(
    chatId: string,
    turnAction: 'nudge' | 'queue' | 'dequeue' | 'skipUserTurn' | 'query',
    participantId: string | null,
  ): Promise<Record<string, unknown>> {
    const chat = await repos.chats.findById(chatId);
    if (!chat) throw new Error(`chat not found: ${chatId}`);

    const userParticipant = findUserParticipant(chat.participants);
    const userParticipantId = userParticipant?.id ?? null;

    const messages = await repos.chats.getMessages(chatId);
    const messageEvents = messages.filter((m: { type: string }) => m.type === 'message');

    let turnState = calculateTurnStateFromHistory({
      messages: messageEvents,
      participants: chat.participants,
      userParticipantId,
      spokenThisCycleParticipantIds: chat.spokenThisCycleParticipantIds,
    } as never);

    let skipCycleUpdate: string | null | undefined = undefined;

    switch (turnAction) {
      case 'nudge':
        turnState = nudgeParticipant(turnState, participantId as string);
        break;
      case 'queue':
        turnState = addToQueue(turnState, participantId as string);
        break;
      case 'dequeue':
        turnState = removeFromQueue(turnState, participantId as string);
        break;
      case 'query':
        break;
      case 'skipUserTurn': {
        skipCycleUpdate = computeSpokenThisCycleAfterSkip(
          participantId as string,
          chat.participants,
          chat.spokenThisCycleParticipantIds,
        );
        if (skipCycleUpdate !== null) {
          try {
            const parsed = JSON.parse(skipCycleUpdate);
            if (Array.isArray(parsed)) {
              turnState = {
                ...turnState,
                spokenSinceUserTurn: parsed.filter((id: unknown): id is string => typeof id === 'string'),
              };
            }
          } catch {
            if (!turnState.spokenSinceUserTurn.includes(participantId as string)) {
              turnState = {
                ...turnState,
                spokenSinceUserTurn: [...turnState.spokenSinceUserTurn, participantId as string],
              };
            }
          }
        }
        turnState = { ...turnState, lastSpeakerId: participantId as string };
        break;
      }
    }

    const activeCharacterParticipants = getActiveCharacterParticipants(chat.participants);
    const charactersMap = new Map<string, unknown>();
    for (const p of activeCharacterParticipants) {
      if (p.characterId) {
        const char = await repos.characters.findById(p.characterId);
        if (char) charactersMap.set(p.characterId, char);
      }
    }

    const nextSpeakerResult = selectNextSpeaker(
      chat.participants,
      charactersMap as never,
      turnState,
      userParticipantId,
    );

    if (turnAction !== 'query') {
      const updatePayload: Record<string, unknown> = {
        turnQueue: JSON.stringify(turnState.queue),
        lastTurnParticipantId: nextSpeakerResult.nextSpeakerId ?? null,
      };
      if (turnAction === 'skipUserTurn' && skipCycleUpdate !== null && skipCycleUpdate !== undefined) {
        updatePayload.spokenThisCycleParticipantIds = skipCycleUpdate;
      }
      await repos.chats.update(chatId, updatePayload);
    }

    const nextSpeakerParticipant = nextSpeakerResult.nextSpeakerId
      ? chat.participants.find((p: { id: string }) => p.id === nextSpeakerResult.nextSpeakerId)
      : null;
    const nextSpeakerCharacter = nextSpeakerParticipant?.characterId
      ? (charactersMap.get(nextSpeakerParticipant.characterId) as { name?: string } | undefined)
      : null;

    return {
      action: turnAction,
      nextSpeakerId: nextSpeakerResult.nextSpeakerId ?? null,
      nextSpeakerName: nextSpeakerCharacter?.name ?? null,
      nextSpeakerControlledBy: nextSpeakerParticipant?.controlledBy ?? null,
      reason: nextSpeakerResult.reason,
      explanation: getSelectionExplanation(nextSpeakerResult),
      cycleComplete: nextSpeakerResult.cycleComplete,
      isUsersTurn: isUsersTurn(nextSpeakerResult),
      queue: turnState.queue,
      queuePosition: participantId ? getQueuePosition(turnState, participantId) : null,
    };
  }

  for (const op of spec.ops) {
    Math.random = () => op.random01;
    try {
      if (op.kind === 'shouldChainNext') {
        // Inject the wall clock via chainStartTime; Date.now is pinned to nowMs.
        const realNow = Date.now;
        Date.now = () => op.nowMs as number;
        const decision = await shouldChainNext(
          repos,
          op.chatId,
          op.userParticipantId ?? null,
          op.chainDepth as number,
          op.chainStartTimeMs as number,
        );
        Date.now = realNow;
        results.push({
          op: 'shouldChainNext',
          chatId: op.chatId,
          chain: decision.chain,
          participantId: decision.participantId ?? null,
          characterName: decision.characterName ?? null,
          reason: decision.reason,
        });
      } else {
        const r = await runTurnAction(op.chatId, op.action as never, op.participantId ?? null);
        results.push({ op: 'turnAction', chatId: op.chatId, ...r });
      }
    } finally {
      Math.random = origRandom;
    }
  }

  const columns = (
    (await rawQuery('PRAGMA table_info(chats)')) as Array<{ name: string }>
  ).map((c) => c.name);
  const rawRows = (await rawQuery('SELECT * FROM chats')) as Array<Record<string, unknown>>;

  await closeDatabase();
  await closeMountIndexSQLiteClient();

  const dump = canonicalizeRows({ table: 'chats', columns, rawRows, orderBy: 'id' });
  process.stdout.write(
    JSON.stringify({ case: 'turn-orchestrator-tier2', results, dump }) + '\n',
  );
  process.exit(0);
}

main().catch((err) => {
  process.stderr.write(`turn-orchestrator oracle failed: ${err?.stack ?? err}\n`);
  process.exit(1);
});
