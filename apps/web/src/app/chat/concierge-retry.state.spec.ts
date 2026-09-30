import { describe, expect, it, vi } from 'vitest';

import { CoreDispatchError } from '../core/core-contract';
import { QUEUE_CHANGE_EVENT } from '../layout/queue-status.logic';
import { ConciergeRetryController, type ConciergeRetryHost } from './concierge-retry.state';

/**
 * The picture + backdrop retries (v4 `app/salon/[id]/hooks/useConciergeRetry.ts`
 * at `ce2f1dabf`, #77). v4 ships no unit test for the hook (its coverage is the
 * route's and `MessageRow.concierge`); these pins are written against the
 * hook's own source, arm by arm: the in-flight Set per tool id, the info toast
 * BEFORE the request, the refetch then the success toast, the 409 words, the
 * fallbacks, and the backdrop's queue notify + poll.
 */

/** v4 words the refusal only on a 409 — `kind: 'conflict'` over v5's dispatch. */
function refusal(token: string): Error {
  return token === 'no-understudy' || token === 'locked'
    ? new CoreDispatchError({ kind: 'conflict', message: token })
    : new Error(token);
}

interface Toast {
  type: string;
  message: string;
}

function host(answer: (req: Record<string, unknown>) => Promise<unknown>) {
  const toasts: Toast[] = [];
  const order: string[] = [];
  const calls: Record<string, unknown>[] = [];
  const h: ConciergeRetryHost = {
    core: {
      dispatchData: vi.fn(async (req: Record<string, unknown>) => {
        order.push('dispatch');
        calls.push(req);
        return (await answer(req)) as Record<string, unknown>;
      }) as unknown as ConciergeRetryHost['core']['dispatchData'],
    },
    toasts: {
      showInfo: (m: string) => (order.push('info'), toasts.push({ type: 'info', message: m }), ''),
      showSuccess: (m: string) => (order.push('success'), toasts.push({ type: 'success', message: m }), ''),
      showError: (m: string) => (order.push('error'), toasts.push({ type: 'error', message: m }), ''),
    },
    chatId: () => 'chat-1',
    refetchChat: vi.fn(async () => {
      order.push('refetch');
    }),
    startBackgroundPolling: vi.fn(() => {
      order.push('poll');
    }),
  };
  return { h, toasts, order, calls };
}

describe('ConciergeRetryController.retryPicture (v4 useConciergeRetry @ ce2f1dabf)', () => {
  it('announces the commission BEFORE the request, then refetches and says it is delivered', async () => {
    const { h, toasts, order, calls } = host(async () => ({ toolMessageId: 't1', images: [], routeTrail: [] }));
    await new ConciergeRetryController(h).retryPicture('t1');
    expect(calls).toEqual([{ type: 'chatRetryImageUncensored', chatId: 'chat-1', body: { toolMessageId: 't1' } }]);
    expect(order).toEqual(['info', 'dispatch', 'refetch', 'success']);
    expect(toasts).toEqual([
      { type: 'info', message: 'The Concierge has taken the commission across the street…' },
      { type: 'success', message: 'The uncensored desk has delivered the picture' },
    ]);
  });

  it('loses a second press on the same picture while the first paints', async () => {
    const releases: Array<() => void> = [];
    const { h, calls } = host(() => new Promise((r) => releases.push(() => r({}))));
    const c = new ConciergeRetryController(h);
    const first = c.retryPicture('t1');
    await c.retryPicture('t1');
    expect(calls).toHaveLength(1);
    // A DIFFERENT picture is not blocked.
    void c.retryPicture('t2');
    expect(calls).toHaveLength(2);
    releases[0]();
    await first;
    // …and the first is released once it settles.
    void c.retryPicture('t1');
    expect(calls).toHaveLength(3);
  });

  it.each([
    [
      'no-understudy',
      'There is no uncensored desk to send this to — appoint one under Settings → The Concierge.',
    ],
    [
      'locked',
      'This conversation is Locked to the usual desks; set it to Moderated should you wish the Concierge to take things elsewhere.',
    ],
    ['Tool message not found', 'Tool message not found'],
    ['', 'The uncensored desk could not produce the picture'],
  ])('words a refusal "%s" as "%s", and neither refetches nor claims delivery', async (token, sentence) => {
    const { h, toasts, order } = host(async () => {
      throw refusal(token);
    });
    await new ConciergeRetryController(h).retryPicture('t1');
    expect(toasts.at(-1)).toEqual({ type: 'error', message: sentence });
    expect(order).not.toContain('refetch');
    expect(order).not.toContain('success');
  });
});

describe('the refusal wording is gated on the 409 (v4 useConciergeRetry.ts:36 at 97b25fc53)', () => {
  it.each(['retryPicture', 'retryBackground'] as const)(
    '%s does NOT reword a non-conflict error whose message reads no-understudy',
    async (verb) => {
      const { h, toasts } = host(async () => {
        throw new CoreDispatchError({ kind: 'internal', message: 'no-understudy' });
      });
      const c = new ConciergeRetryController(h);
      await (verb === 'retryPicture' ? c.retryPicture('t1') : c.retryBackground());
      expect(toasts.at(-1)).toEqual({ type: 'error', message: 'no-understudy' });
    },
  );
});

describe('ConciergeRetryController.retryBackground (v4 useConciergeRetry @ ce2f1dabf)', () => {
  it('commissions the backdrop, wakes the queue badges and starts the poll', async () => {
    const { h, toasts, order, calls } = host(async () => ({ message: 'queued', queued: true, jobId: 'j' }));
    const queued = vi.fn();
    window.addEventListener(QUEUE_CHANGE_EVENT, queued);
    try {
      await new ConciergeRetryController(h).retryBackground();
    } finally {
      window.removeEventListener(QUEUE_CHANGE_EVENT, queued);
    }
    expect(calls).toEqual([{ type: 'chatRetryImageUncensored', chatId: 'chat-1', body: { kind: 'background' } }]);
    expect(toasts).toEqual([{ type: 'success', message: 'Backdrop commissioned from the uncensored desk' }]);
    expect(queued).toHaveBeenCalledTimes(1);
    expect(order).toEqual(['dispatch', 'success', 'poll']);
  });

  it('has no in-flight guard (v4 has none)', async () => {
    const { h, calls } = host(() => new Promise(() => undefined));
    const c = new ConciergeRetryController(h);
    void c.retryBackground();
    void c.retryBackground();
    await Promise.resolve();
    expect(calls).toHaveLength(2);
  });

  it.each([
    [
      'no-understudy',
      'There is no uncensored desk to send this to — appoint one under Settings → The Concierge.',
    ],
    ['Story backgrounds are not enabled.', 'Story backgrounds are not enabled.'],
    ['', 'Failed to queue the backdrop'],
  ])('words a refusal "%s" as "%s", and starts no poll', async (token, sentence) => {
    const { h, toasts, order } = host(async () => {
      throw refusal(token);
    });
    await new ConciergeRetryController(h).retryBackground();
    expect(toasts).toEqual([{ type: 'error', message: sentence }]);
    expect(order).not.toContain('poll');
  });
});
