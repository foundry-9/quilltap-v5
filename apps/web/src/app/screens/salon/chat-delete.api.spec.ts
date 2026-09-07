import { afterEach, describe, expect, it, vi } from 'vitest';

import { CoreDispatchError } from '../../core/core-contract';
import type { CoreClient } from '../../core/core-client';
import { confirmAndDeleteChat } from './chat-delete.api';

/**
 * P4.80 (dogfood finding #117) — v4 `lib/chat-utils.ts:148-159`, the shared
 * gate both chat lists call. Four behaviours, all v4's:
 *
 *  1. the confirmation SENTENCE, byte for byte;
 *  2. cancel writes nothing and answers `false` — the dispatch is never made;
 *  3. a server refusal toasts v4's FIXED `Failed to delete chat`, never the
 *     server's own text (v4 throws its own `Error` on a non-ok response before
 *     it ever reads the body);
 *  4. a transport-level rejection keeps ITS message (v4's `fetch` rejection).
 *
 * (3) and (4) are a pair on purpose: collapsing them either way is a real
 * divergence, and only one of them can be caught by looking at the code.
 */
function client(dispatchData: () => Promise<unknown>): CoreClient {
  return { dispatchData } as unknown as CoreClient;
}

describe('confirmAndDeleteChat', () => {
  afterEach(() => vi.restoreAllMocks());

  it('asks v4’s exact question', async () => {
    const confirm = vi.spyOn(window, 'confirm').mockReturnValue(true);
    await confirmAndDeleteChat(
      client(() => Promise.resolve({ success: true })),
      vi.fn(),
      'c1',
    );
    expect(confirm).toHaveBeenCalledWith('Are you sure you want to delete this chat?');
  });

  it('dispatches chatDelete and answers true when confirmed', async () => {
    vi.spyOn(window, 'confirm').mockReturnValue(true);
    const dispatchData = vi.fn().mockResolvedValue({ success: true });
    const toast = vi.fn();
    await expect(confirmAndDeleteChat(client(dispatchData), toast, 'c1')).resolves.toBe(true);
    expect(dispatchData).toHaveBeenCalledWith({ type: 'chatDelete', chatId: 'c1' });
    expect(toast).not.toHaveBeenCalled();
  });

  it('cancelling dispatches nothing and answers false', async () => {
    vi.spyOn(window, 'confirm').mockReturnValue(false);
    const dispatchData = vi.fn();
    const toast = vi.fn();
    await expect(confirmAndDeleteChat(client(dispatchData), toast, 'c1')).resolves.toBe(false);
    expect(dispatchData).not.toHaveBeenCalled();
    expect(toast).not.toHaveBeenCalled();
  });

  it('a server refusal toasts v4’s fixed sentence, not the server’s text', async () => {
    vi.spyOn(window, 'confirm').mockReturnValue(true);
    const toast = vi.fn();
    const err = new CoreDispatchError({
      kind: 'internal',
      message: 'Failed to delete chat: some very specific SQL complaint',
    } as never);
    await expect(
      confirmAndDeleteChat(
        client(() => Promise.reject(err)),
        toast,
        'c1',
      ),
    ).resolves.toBe(false);
    expect(toast).toHaveBeenCalledWith('Failed to delete chat');
  });

  it('a transport rejection keeps its own message (v4’s fetch rejection)', async () => {
    vi.spyOn(window, 'confirm').mockReturnValue(true);
    const toast = vi.fn();
    await expect(
      confirmAndDeleteChat(
        client(() => Promise.reject(new Error('Failed to fetch'))),
        toast,
        'c1',
      ),
    ).resolves.toBe(false);
    expect(toast).toHaveBeenCalledWith('Failed to fetch');
  });
});
