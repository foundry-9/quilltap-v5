import { describe, expect, it, vi } from 'vitest';

import type { CoreClient } from '../../core/core-client';
import { fetchChatBackgroundVar, storyBackgroundKeys } from './story-background.api';

/**
 * The story-background resolver (dogfood finding #9). It maps the §1
 * `chatGetBackground` body to the CSS `url(...)` value the Salon binds on
 * `--story-background-url`, preferring the file id over v4's path string.
 */
describe('fetchChatBackgroundVar', () => {
  function coreReturning(data: Record<string, unknown>): { core: CoreClient; dispatchData: ReturnType<typeof vi.fn> } {
    const dispatchData = vi.fn(async () => data);
    return { core: { dispatchData } as unknown as CoreClient, dispatchData };
  }

  it('dispatches chatGetBackground for the chat and resolves the id-keyed byte URL', async () => {
    const { core, dispatchData } = coreReturning({
      backgroundUrl: '/v4/path/bg.webp',
      fileId: 'file-42',
      filename: 'bg.webp',
      sha256: 'abc',
      linkSummary: null,
    });
    const value = await fetchChatBackgroundVar(core, 'chat-9');
    expect(dispatchData).toHaveBeenCalledWith({ type: 'chatGetBackground', chatId: 'chat-9' });
    // Prefer the file id through the store-backed byte route, NOT v4's path string.
    expect(value).toBe("url('/api/v1/files/file-42')");
  });

  it('returns null when the chat has no background (all-null body)', async () => {
    const { core } = coreReturning({
      backgroundUrl: null,
      fileId: null,
      filename: null,
      sha256: null,
      linkSummary: null,
    });
    expect(await fetchChatBackgroundVar(core, 'chat-9')).toBeNull();
  });

  it('keys the query as a sibling of the chat detail key', () => {
    expect(storyBackgroundKeys.background('chat-9')).toEqual(['chat', 'chat-9', 'background']);
  });
});
