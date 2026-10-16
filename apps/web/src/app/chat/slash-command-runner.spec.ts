import { QueryClient } from '@tanstack/angular-query-experimental';
import { describe, expect, it, vi } from 'vitest';

import type { CoreClient } from '../core/core-client';
import type { RngPendingResult } from './rng-request';
import { QUICK_ROLLS, buildQuickRollActions, createSlashCommandRunner } from './slash-command-runner';

/**
 * v4 `components/chat/useSlashCommandRunner.ts` + `lexical/slash-commands/
 * context.ts`'s `buildQuickRollActions` (`4877fc2da`): the immediate runs and
 * the quick rolls behind the composer's `/` commands. The bodies are the verbs'
 * requests; the toasts are v4's strings.
 */

type Answer = Record<string, unknown> | Error | unknown;

function harness(answer: Answer = { success: true }, defaultCharacterId: string | null = 'char-1') {
  const requests: Record<string, unknown>[] = [];
  const core = {
    dispatchData: vi.fn(async (req: Record<string, unknown>) => {
      // Requests travel as JSON: what an undefined key does on the wire.
      requests.push(JSON.parse(JSON.stringify(req)) as Record<string, unknown>);
      if (answer instanceof Error) throw answer;
      if (typeof answer === 'string') throw answer;
      return answer as Record<string, unknown>;
    }),
  } as unknown as CoreClient;
  const queryClient = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  const invalidate = vi.spyOn(queryClient, 'invalidateQueries');
  const toasts = { showSuccess: vi.fn(() => ''), showError: vi.fn(() => '') };
  const onRan = vi.fn();
  const runner = createSlashCommandRunner({
    core,
    queryClient,
    toasts,
    chatId: 'chat-1',
    defaultCharacterId,
    onRan,
  });
  return { requests, toasts, onRan, runner, invalidate, core };
}

describe('createSlashCommandRunner — runBuiltInNow', () => {
  it("runs Prospero's tool with no arguments, as the default character, in public", async () => {
    const h = harness();
    await h.runner.runBuiltInNow('self_inventory');
    expect(h.requests).toEqual([
      {
        type: 'chatRunTool',
        chatId: 'chat-1',
        toolName: 'self_inventory',
        arguments: {},
        characterId: 'char-1',
        private: false,
      },
    ]);
    expect(h.toasts.showSuccess).toHaveBeenCalledWith('Prospero has run self_inventory.');
    expect(h.onRan).toHaveBeenCalledTimes(1);
  });

  it.each([null, ''])('sends no characterId when the default is %j (v4 `|| undefined`)', async (id) => {
    const h = harness({ success: true }, id);
    await h.runner.runBuiltInNow('self_inventory');
    expect(h.requests[0]).not.toHaveProperty('characterId');
  });

  it("toasts the server's text verbatim on a refusal, and does not report a run", async () => {
    const h = harness({ success: false, error: 'No such tool in this chat' });
    await h.runner.runBuiltInNow('self_inventory');
    expect(h.toasts.showError).toHaveBeenCalledWith('No such tool in this chat');
    expect(h.toasts.showSuccess).not.toHaveBeenCalled();
    expect(h.onRan).not.toHaveBeenCalled();
  });

  it("falls back to v4's '<id> could not be run' on a bare throw", async () => {
    const h = harness('boom');
    await h.runner.runBuiltInNow('self_inventory');
    expect(h.toasts.showError).toHaveBeenCalledWith('self_inventory could not be run');
  });
});

describe('createSlashCommandRunner — runCustomNow', () => {
  it('runs Pascal’s tool with neither parameters nor private (CUSTOM_RUN_PARAMETERS_OMITTED)', async () => {
    const h = harness({ success: true });
    await h.runner.runCustomNow({ name: 'wear_regalia', asCharacterId: 'char-1' });
    expect(h.requests).toEqual([
      { type: 'chatCustomToolRun', chatId: 'chat-1', tool: 'wear_regalia', asCharacterId: 'char-1' },
    ]);
    // The NAME, where the dialog toasts the title.
    expect(h.toasts.showSuccess).toHaveBeenCalledWith('Pascal has settled wear_regalia.');
    expect(h.onRan).toHaveBeenCalledTimes(1);
  });

  it('invalidates the chat and the roster after a run', async () => {
    const h = harness({ success: true });
    await h.runner.runCustomNow({ name: 'coin', asCharacterId: null });
    expect(h.requests[0]).not.toHaveProperty('asCharacterId');
    expect(h.invalidate.mock.calls.map(([filters]) => filters?.queryKey)).toEqual([
      ['chat', 'chat-1'],
      ['customTools', 'chat-1'],
    ]);
  });

  it("toasts the server's text verbatim on a refusal; v4's fallback on a bare throw", async () => {
    const refused = harness(new Error('Pascal has no such tool'));
    await refused.runner.runCustomNow({ name: 'coin', asCharacterId: null });
    expect(refused.toasts.showError).toHaveBeenCalledWith('Pascal has no such tool');
    expect(refused.onRan).not.toHaveBeenCalled();
    expect(refused.invalidate).not.toHaveBeenCalled();

    const bare = harness('boom');
    await bare.runner.runCustomNow({ name: 'coin', asCharacterId: null });
    expect(bare.toasts.showError).toHaveBeenCalledWith('coin could not be run');
  });
});

describe('buildQuickRollActions', () => {
  const PREVIEW = {
    success: true,
    preview: true,
    result: { formattedText: 'f', summary: 's', requestPrompt: 'p', arguments: { type: 6, rolls: 1 } },
  };

  it("carries v4's QUICK_ROLLS", () => {
    expect(QUICK_ROLLS).toEqual({
      d6: { type: 6, rolls: 1 },
      d20: { type: 20, rolls: 1 },
      coin: { type: 'flip_coin', rolls: 1 },
      bottle: { type: 'spin_the_bottle', rolls: 1 },
    });
  });

  it.each([
    ['d6', 6],
    ['d20', 20],
    ['coin', 'flip_coin'],
    ['bottle', 'spin_the_bottle'],
  ] as const)('/%s previews one roll of %j and hands the chip over', async (action, kind) => {
    const h = harness(PREVIEW);
    const chips: RngPendingResult[] = [];
    const actions = buildQuickRollActions(h.core, 'chat-1', (chip) => chips.push(chip), h.toasts);
    expect(Object.keys(actions)).toEqual(['d6', 'd20', 'coin', 'bottle']);
    actions[action]!();
    await vi.waitFor(() => expect(chips).toHaveLength(1));
    expect(h.requests).toEqual([{ type: 'chatRng', chatId: 'chat-1', kind, rolls: 1, preview: true }]);
    expect(chips[0]).toMatchObject({ tool: 'rng', displayName: 'Random Number Generator', summary: 's' });
  });

  it("toasts the failure's text, or 'The dice would not roll' when it carries none", async () => {
    const failed = harness({ success: true, message: 'rolled' });
    buildQuickRollActions(failed.core, 'chat-1', () => undefined, failed.toasts)['d6']!();
    await vi.waitFor(() =>
      expect(failed.toasts.showError).toHaveBeenCalledWith('RNG did not return a preview'),
    );

    // A message-less throw inside the request surfaces as v4's 'RNG failed'
    // (the request's own fallback) — the dice fallback answers a non-Error.
    const bare = harness('boom');
    buildQuickRollActions(bare.core, 'chat-1', () => undefined, bare.toasts)['d20']!();
    await vi.waitFor(() => expect(bare.toasts.showError).toHaveBeenCalledWith('RNG failed'));

    // A non-Error thrown past the request (here by the chip handler) takes the
    // dice fallback.
    const toasts = { showError: vi.fn(() => '') };
    const onChip = vi.fn(() => {
      throw 'chip boom';
    });
    buildQuickRollActions(
      { dispatchData: vi.fn(async () => PREVIEW) } as unknown as CoreClient,
      'chat-1',
      onChip,
      toasts,
    )['coin']!();
    await vi.waitFor(() => expect(toasts.showError).toHaveBeenCalledWith('The dice would not roll'));
  });
});
