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
 * (`isClassifierOnDuty` — the missing guard that let an Uncensored (now Unmoderated) chat
 * enqueue a doomed `CHAT_DANGER_CLASSIFICATION` on every turn).
 *
 * P4.D227 (v4 `3b463d6b1`, #76): the gate is the chat's Concierge POLICY —
 * `resolveConciergeSettings(chatSettings, chat).summaryClassification` — read
 * INSIDE the function after the chat and its Moderated check, exactly as v5
 * now reads it (the old pre-resolved `danger_mode_off` flag is retired). The
 * resolver runs REAL; only the repositories are mocked, and each case names
 * the stored `conciergeSettings` both sides seed.
 *
 * Recorded, not compared:
 *   - `chatSettingsLookedUp` — v5 has no probe on its settings read (the
 *     ORDER is the same since #76; only the observable is missing).
 *   - `settings-lookup-throws` — v5's settings read cannot be made to throw
 *     through the fixture.
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
  /**
   * The stored `chat_settings.conciergeSettings` (v4 `3b463d6b1`, #76): the
   * REAL resolver reads it off the mocked settings row, WITH the chat. `null`
   * = a row with no `conciergeSettings` (the defaults: summary classification
   * off). Both sides seed exactly this.
   */
  concierge: Record<string, unknown> | null;
  /** v4's "handles errors gracefully" arm: the settings lookup rejects. */
  settingsThrows?: boolean;
}

const OPTED_IN = { enabled: true, preScreen: { summaryClassification: true } };

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
  { name: 'enqueues_when_conditions_met', chat: { ...BASE_CHAT }, concierge: OPTED_IN },
  { name: 'skips_when_off_duty', chat: { ...BASE_CHAT }, concierge: { enabled: false, preScreen: { summaryClassification: true } } },
  // v4 `chat-danger-trigger.test.ts` at `3b463d6b1`: the summary classifier
  // is its own opt-in, off by default.
  { name: 'skips_when_not_opted_into_summary_classifier', chat: { ...BASE_CHAT }, concierge: { enabled: true } },
  { name: 'skips_when_no_concierge_settings_stored', chat: { ...BASE_CHAT }, concierge: null },
  // Resolved WITH the chat: a moderation-exempt chat type is inert whatever
  // the operator opted into.
  { name: 'skips_when_exempt_chat_type', chat: { ...BASE_CHAT, chatType: 'help' }, concierge: OPTED_IN },
  { name: 'skips_when_chat_not_found', chat: null, concierge: OPTED_IN },
  { name: 'skips_when_sticky_dangerous', chat: { ...BASE_CHAT, isDangerousChat: true }, concierge: OPTED_IN },
  {
    name: 'skips_when_already_classified_at_count',
    chat: { ...BASE_CHAT, dangerClassifiedAt: '2026-01-01T00:00:00Z', dangerClassifiedAtMessageCount: 10, messageCount: 10 },
    concierge: OPTED_IN,
  },
  {
    name: 'rechecks_when_count_changed',
    chat: { ...BASE_CHAT, dangerClassifiedAt: '2026-01-01T00:00:00Z', dangerClassifiedAtMessageCount: 8, messageCount: 10 },
    concierge: OPTED_IN,
  },
  { name: 'skips_when_no_context_summary', chat: { ...BASE_CHAT, contextSummary: null }, concierge: OPTED_IN },
  { name: 'skips_when_empty_context_summary', chat: { ...BASE_CHAT, contextSummary: '' }, concierge: OPTED_IN },
  // --- the scenario arm `da9c4f34f` added (bug 158). The two cases above are
  //     GREEN BY LUCK without these: no chat in v4's corpus carries a
  //     `scenarioText` at all, so the widened gate `!contextSummary &&
  //     !scenarioText` is indistinguishable from the old `!contextSummary`.
  //     These three make the conjunction observable. The gate is JS
  //     truthiness on BOTH columns, so an empty scenario is no scenario —
  //     a port reading `scenarioText.is_some()` enqueues on the third.
  {
    name: 'scenario_only_summary_none',
    chat: { ...BASE_CHAT, contextSummary: null, scenarioText: 'A scene at the pool.' },
    concierge: OPTED_IN,
  },
  {
    name: 'scenario_only_summary_empty',
    chat: { ...BASE_CHAT, contextSummary: '', scenarioText: 'A scene at the pool.' },
    concierge: OPTED_IN,
  },
  {
    name: 'skips_when_scenario_is_empty_string',
    chat: { ...BASE_CHAT, contextSummary: null, scenarioText: '' },
    concierge: OPTED_IN,
  },
  // --- off the Moderated desk (v4 `4d370a90f`, #75 — the three states; the
  //     arms `c43d3b1b4` added, re-keyed). The label underneath is FALSE on
  //     purpose: the chat was scanned and found safe before the operator
  //     spoke, so no other guard would catch these.
  {
    name: 'skips_when_locked',
    chat: { ...BASE_CHAT, conciergeMode: 'locked', conciergeModeSetBy: 'operator', isDangerousChat: false },
    concierge: OPTED_IN,
  },
  {
    name: 'skips_when_unmoderated',
    chat: { ...BASE_CHAT, conciergeMode: 'unmoderated', conciergeModeSetBy: 'operator', isDangerousChat: false },
    concierge: OPTED_IN,
  },
  // The Unmoderated chat as production actually meets it: the resolver forces
  // AUTO_ROUTE, so mode is emphatically not OFF, and the ONLY thing between the
  // turn and the enqueue is the on-duty guard.
  {
    name: 'skips_when_unmoderated_by_the_concierge_with_label',
    chat: { ...BASE_CHAT, conciergeMode: 'unmoderated', conciergeModeSetBy: 'concierge', isDangerousChat: true },
    concierge: OPTED_IN,
  },
  // The legacy pair no longer takes the classifier off the case: an `OFF`
  // override on a Moderated (NULL) chat ENQUEUES.
  {
    name: 'legacy_override_is_ignored',
    chat: { ...BASE_CHAT, conciergeOverride: 'OFF', isDangerousChat: false },
    concierge: OPTED_IN,
  },
  // NO v5 COUNTERPART (recorded, not compared).
  { name: 'settings_lookup_throws', chat: { ...BASE_CHAT }, concierge: OPTED_IN, settingsThrows: true },
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
  // The resolver is pure: v4's REAL `resolveConciergeSettings` runs (P4.D227).
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
        return c.concierge === null ? {} : { conciergeSettings: c.concierge };
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
    concierge: c.concierge,
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
