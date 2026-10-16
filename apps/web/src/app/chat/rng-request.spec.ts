import { describe, expect, it, vi } from 'vitest';

import type { CoreClient } from '../core/core-client';
import { requestRngPendingResult } from './rng-request';

/**
 * v4 `components/chat/rng-request.ts` (`4877fc2da`): the one request behind the
 * dice button and the composer's `/d6`, `/d20`, `/coin`, `/bottle` — one
 * request, one chip.
 */

/** v4's preview body (`actions/rng.ts:91-107`) for a 1d20 that came up 17. */
const D20_PREVIEW = {
  success: true,
  preview: true,
  result: {
    type: 20,
    rollCount: 1,
    results: [17],
    sum: 17,
    formattedText: '🎲 Rolled 1d20: **17**',
    summary: 'd20: 17',
    requestPrompt: 'Roll a d20',
    arguments: { type: 20, rolls: 1 },
  },
};

function client(answer: Record<string, unknown> | Error): {
  core: CoreClient;
  calls: Record<string, unknown>[];
} {
  const calls: Record<string, unknown>[] = [];
  const core = {
    dispatchData: vi.fn(async (req: Record<string, unknown>) => {
      calls.push(req);
      if (answer instanceof Error) throw answer;
      return answer;
    }),
  } as unknown as CoreClient;
  return { core, calls };
}

describe('requestRngPendingResult', () => {
  it("rolls in PREVIEW mode on the chatRng verb and shapes v4's chip, key for key", async () => {
    const { core, calls } = client(D20_PREVIEW);
    const chip = await requestRngPendingResult(core, 'chat-1', { type: 20, rolls: 1 });
    expect(calls).toEqual([{ type: 'chatRng', chatId: 'chat-1', kind: 20, rolls: 1, preview: true }]);
    // Key order is part of the value (v4's literal).
    expect(JSON.stringify(chip)).toBe(
      JSON.stringify({
        tool: 'rng',
        displayName: 'Random Number Generator',
        icon: '🎲',
        summary: 'd20: 17',
        formattedResult: '🎲 Rolled 1d20: **17**',
        requestPrompt: 'Roll a d20',
        arguments: { type: 20, rolls: 1 },
        success: true,
      }),
    );
  });

  it.each([
    ['flip_coin', { type: 'flip_coin' as const, rolls: 1 }],
    ['spin_the_bottle', { type: 'spin_the_bottle' as const, rolls: 1 }],
  ])('sends %s as the kind', async (kind, args) => {
    const { core, calls } = client(D20_PREVIEW);
    await requestRngPendingResult(core, 'chat-1', args);
    expect(calls[0]).toMatchObject({ kind, rolls: 1, preview: true });
  });

  it("throws v4's 'RNG did not return a preview' when the reply is not a preview", async () => {
    const { core } = client({ success: true, message: 'rolled', result: { formattedText: 'x' } });
    await expect(requestRngPendingResult(core, 'chat-1', { type: 6, rolls: 1 })).rejects.toThrow(
      'RNG did not return a preview',
    );
  });

  it("surfaces a refused dispatch as the server's text", async () => {
    const { core } = client(new Error('Rolls must be between 1 and 100'));
    await expect(requestRngPendingResult(core, 'chat-1', { type: 6, rolls: 500 })).rejects.toThrow(
      'Rolls must be between 1 and 100',
    );
  });

  it("falls back to v4's 'RNG failed' for a throw that carries no message", async () => {
    const core = {
      dispatchData: vi.fn(async () => {
        throw 'boom';
      }),
    } as unknown as CoreClient;
    await expect(requestRngPendingResult(core, 'chat-1', { type: 6, rolls: 1 })).rejects.toThrow(
      'RNG failed',
    );
  });
});
