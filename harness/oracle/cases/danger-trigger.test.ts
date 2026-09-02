/**
 * @jest-environment node
 *
 * P4.D143 Tier 2 ORACLE: v4's REAL `triggerChatDangerClassification`
 * (`lib/services/chat-message/memory-trigger.service.ts`) over MOCKED repos —
 * the DB-free route-guard-oracle idiom. No fixture, no database: three
 * `jest.doMock`s stand in for the repos, the resolver and the queue service,
 * and what the oracle emits is the ENQUEUE CALLS the function made.
 *
 * v4's own `__tests__/unit/services/chat-danger-trigger.test.ts` is the corpus,
 * case for case, plus the two operator arms `c43d3b1b4` added
 * (`isClassifierOnDuty` — the missing guard that let an Uncensored chat
 * enqueue a doomed `CHAT_DANGER_CLASSIFICATION` on every turn).
 *
 * Two of v4's cases have NO v5 counterpart and are recorded, not compared:
 *   - `chatSettingsLookedUp` — v4 resolves the danger mode INSIDE the function,
 *     so it can assert an operator override bails before any settings lookup.
 *     v5's `danger_mode_off` is computed by the two producers BEFORE the call.
 *   - `settings-lookup-throws` — the same reason: there is no settings lookup
 *     in the v5 function to throw.
 * Both are emitted so the divergence is visible in the NDJSON rather than
 * argued in prose; the Rust side names them and skips the comparison.
 *
 * Run (Node 24, from the v4 checkout — cp to a /tmp mirror; jest ignores .claude/):
 *   N=~/.nvm/versions/node/v24.13.1/bin ; V5W=<this worktree>
 *   TMPO=/tmp/qt-danger-trigger-oracle
 *   rm -rf "$TMPO"; mkdir -p "$TMPO/cases"
 *   cp "$V5W/harness/oracle/cases/danger-trigger.test.ts" "$TMPO/cases/"
 *   cd ~/source/quilltap-server
 *   QT_ORACLE_OUT=/tmp/oracle-danger-trigger.ndjson \
 *     $N/npx jest --silent --watchman=false --roots "$PWD" --roots "$TMPO/cases" -- danger-trigger
 */

import * as fs from 'fs';

interface CaseSpec {
  name: string;
  /** The `chats.findById` answer. `null` = the chat-not-found arm. */
  chat: Record<string, unknown> | null;
  /** What the resolver answers. v5 passes this in as `dangerModeOff`. */
  mode: 'DETECT_ONLY' | 'AUTO_ROUTE' | 'OFF';
  /** v4's "handles errors gracefully" arm: the settings lookup rejects. */
  settingsThrows?: boolean;
}

const BASE_CHAT = {
  id: 'chat-1',
  contextSummary: 'A conversation about cats.',
  messageCount: 10,
  isDangerousChat: null,
  dangerClassifiedAt: null,
  dangerClassifiedAtMessageCount: null,
};

const CASES: CaseSpec[] = [
  // --- v4's own corpus, case for case ---
  { name: 'enqueues_when_conditions_met', chat: { ...BASE_CHAT }, mode: 'DETECT_ONLY' },
  { name: 'skips_when_mode_off', chat: { ...BASE_CHAT }, mode: 'OFF' },
  { name: 'skips_when_chat_not_found', chat: null, mode: 'DETECT_ONLY' },
  { name: 'skips_when_sticky_dangerous', chat: { ...BASE_CHAT, isDangerousChat: true }, mode: 'DETECT_ONLY' },
  {
    name: 'skips_when_already_classified_at_count',
    chat: { ...BASE_CHAT, dangerClassifiedAt: '2026-01-01T00:00:00Z', dangerClassifiedAtMessageCount: 10, messageCount: 10 },
    mode: 'DETECT_ONLY',
  },
  {
    name: 'rechecks_when_count_changed',
    chat: { ...BASE_CHAT, dangerClassifiedAt: '2026-01-01T00:00:00Z', dangerClassifiedAtMessageCount: 8, messageCount: 10 },
    mode: 'DETECT_ONLY',
  },
  { name: 'skips_when_no_context_summary', chat: { ...BASE_CHAT, contextSummary: null }, mode: 'DETECT_ONLY' },
  { name: 'skips_when_empty_context_summary', chat: { ...BASE_CHAT, contextSummary: '' }, mode: 'DETECT_ONLY' },
  // --- the two operator arms `c43d3b1b4` added. The label underneath is
  //     FALSE on purpose: the chat was scanned and found safe before the
  //     operator spoke, so no other guard would catch these.
  {
    name: 'skips_when_operator_vouched',
    chat: { ...BASE_CHAT, conciergeOverride: 'OFF', isDangerousChat: false },
    mode: 'AUTO_ROUTE',
  },
  {
    name: 'skips_when_operator_uncensored',
    chat: { ...BASE_CHAT, conciergeOverride: 'UNCENSORED', isDangerousChat: false },
    mode: 'AUTO_ROUTE',
  },
  // The Uncensored chat as production actually meets it: the resolver forces
  // AUTO_ROUTE, so mode is emphatically not OFF, and the ONLY thing between the
  // turn and the enqueue is the new on-duty guard.
  {
    name: 'skips_when_operator_uncensored_with_label',
    chat: { ...BASE_CHAT, conciergeOverride: 'UNCENSORED', isDangerousChat: true },
    mode: 'AUTO_ROUTE',
  },
  // NO v5 COUNTERPART (recorded, not compared).
  { name: 'settings_lookup_throws', chat: { ...BASE_CHAT }, mode: 'DETECT_ONLY', settingsThrows: true },
];

interface EnqueueCall {
  userId: string;
  chatId: string;
  connectionProfileId: string;
}

async function runCase(c: CaseSpec): Promise<Record<string, unknown>> {
  jest.resetModules();

  const enqueued: EnqueueCall[] = [];
  let chatSettingsLookedUp = false;

  jest.doMock('@/lib/logging/create-logger', () => ({
    createServiceLogger: () => ({ debug: () => {}, info: () => {}, warn: () => {}, error: () => {} }),
  }));
  jest.doMock('@/lib/memory', () => ({
    processMessageForMemoryAsync: () => {},
    processInterCharacterMemoryAsync: () => {},
  }));
  jest.doMock('@/lib/chat/context-summary', () => ({ checkAndGenerateSummaryIfNeeded: () => {} }));
  jest.doMock('@/lib/services/system-events.service', () => ({ createMemoryExtractionEvent: () => {} }));
  jest.doMock('@/lib/services/cost-estimation.service', () => ({ estimateMessageCost: () => {} }));
  jest.doMock('@/lib/services/dangerous-content/resolver.service', () => ({
    resolveDangerousContentSettings: () => ({
      settings: {
        mode: c.mode,
        threshold: 0.7,
        scanTextChat: true,
        scanImagePrompts: true,
        scanImageGeneration: false,
        displayMode: 'SHOW',
        showWarningBadges: true,
      },
      source: 'global',
    }),
  }));
  jest.doMock('@/lib/background-jobs/queue-service', () => ({
    enqueueChatDangerClassification: async (userId: string, payload: { chatId: string; connectionProfileId: string }) => {
      enqueued.push({ userId, chatId: payload.chatId, connectionProfileId: payload.connectionProfileId });
      return { jobId: 'job-1', isNew: true };
    },
  }));

  const repos = {
    chatSettings: {
      findByUserId: async () => {
        chatSettingsLookedUp = true;
        if (c.settingsThrows) throw new Error('DB error');
        return { dangerousContentSettings: { mode: 'DETECT_ONLY', threshold: 0.7 } };
      },
    },
    chats: { findById: async () => c.chat },
    connections: { findByUserId: async () => [] },
  };

  const { triggerChatDangerClassification } = await import(
    '@/lib/services/chat-message/memory-trigger.service'
  );
  await triggerChatDangerClassification(repos as never, {
    chatId: 'chat-1',
    userId: 'user-1',
    connectionProfile: { id: 'profile-1', provider: 'OPENAI', modelName: 'gpt-4o-mini' } as never,
    chatSettings: { cheapLLMSettings: { strategy: 'PROVIDER_CHEAPEST', fallbackToLocal: true } },
  } as never);

  return {
    name: c.name,
    chat: c.chat,
    mode: c.mode,
    enqueued,
    // NO v5 COUNTERPART — recorded so the divergence is visible, never compared.
    chatSettingsLookedUp,
  };
}

test('danger-trigger oracle', async () => {
  const outPath = process.env.QT_ORACLE_OUT;
  if (!outPath) throw new Error('QT_ORACLE_OUT must point at the NDJSON file to write');
  const lines: string[] = [];
  for (const c of CASES) lines.push(JSON.stringify(await runCase(c)));
  fs.writeFileSync(outPath, lines.join('\n') + '\n');
  process.stderr.write(`danger-trigger oracle wrote ${outPath} (${lines.length} cases)\n`);
});
