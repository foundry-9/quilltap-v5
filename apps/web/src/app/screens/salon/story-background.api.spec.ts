import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import type { CoreClient } from '../../core/core-client';
import {
  ACTIVE_POLL_INTERVAL_MS,
  ACTIVE_POLL_MAX,
  PASSIVE_POLL_INTERVAL_MS,
  StoryBackgroundPoller,
  fetchChatBackgroundVar,
  regenerateChatBackground,
  storyBackgroundKeys,
} from './story-background.api';

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

describe('regenerateChatBackground (Shared contract §2)', () => {
  it('dispatches the EXISTING chatRegenerateBackground wire name and returns the result', async () => {
    // §2: the Request variant and wire name already exist (types.rs:1832-1842);
    // only the dispatch target changes. The name must NOT drift.
    const dispatchData = vi.fn(async () => ({
      message: 'Story background regeneration queued',
      queued: true,
      jobId: 'job-7',
    }));
    const core = { dispatchData } as unknown as CoreClient;

    const result = await regenerateChatBackground(core, 'chat-9');

    expect(dispatchData).toHaveBeenCalledWith({ type: 'chatRegenerateBackground', chatId: 'chat-9' });
    expect(result).toEqual({
      message: 'Story background regeneration queued',
      queued: true,
      jobId: 'job-7',
    });
  });

  it('propagates the server message (how the §2 badRequest strings reach the user)', async () => {
    const core = {
      dispatchData: vi.fn(async () => {
        throw new Error(
          'Story backgrounds are not enabled. Enable them in Settings > Chat Settings > Story Backgrounds.',
        );
      }),
    } as unknown as CoreClient;

    await expect(regenerateChatBackground(core, 'chat-9')).rejects.toThrow(
      'Story backgrounds are not enabled. Enable them in Settings > Chat Settings > Story Backgrounds.',
    );
  });
});

describe('StoryBackgroundPoller (v4 useStoryBackground startPolling, :95-128)', () => {
  beforeEach(() => vi.useFakeTimers());
  afterEach(() => vi.useRealTimers());

  /** Advance the fake clock by N poll ticks, letting each tick's await settle. */
  async function tick(n: number): Promise<void> {
    for (let i = 0; i < n; i++) {
      await vi.advanceTimersByTimeAsync(ACTIVE_POLL_INTERVAL_MS);
    }
  }

  it('polls every 5s and stops the moment the value changes, firing onChanged once', async () => {
    let current: string | null = "url('/api/v1/files/old')";
    const refetch = vi.fn(async () => current);
    const onChanged = vi.fn();
    const poller = new StoryBackgroundPoller();

    poller.start(current, refetch, onChanged);
    await tick(3);
    expect(refetch).toHaveBeenCalledTimes(3);
    expect(onChanged).not.toHaveBeenCalled();
    expect(poller.polling).toBe(true);

    current = "url('/api/v1/files/new')";
    await tick(1);

    expect(onChanged).toHaveBeenCalledTimes(1);
    expect(poller.polling).toBe(false);

    // Stopped means stopped — no further refetches.
    await tick(5);
    expect(refetch).toHaveBeenCalledTimes(4);
  });

  it('detects a background APPEARING where there was none (null → a URL)', async () => {
    let current: string | null = null;
    const onChanged = vi.fn();
    const poller = new StoryBackgroundPoller();

    poller.start(null, async () => current, onChanged);
    await tick(1);
    expect(onChanged).not.toHaveBeenCalled();

    current = "url('/api/v1/files/fresh')";
    await tick(1);
    expect(onChanged).toHaveBeenCalledTimes(1);
  });

  it('gives up after 36 polls (3 minutes) without firing onChanged', async () => {
    const refetch = vi.fn(async () => "url('/api/v1/files/same')");
    const onChanged = vi.fn();
    const poller = new StoryBackgroundPoller();

    poller.start("url('/api/v1/files/same')", refetch, onChanged);
    await tick(ACTIVE_POLL_MAX);

    expect(refetch).toHaveBeenCalledTimes(ACTIVE_POLL_MAX);
    expect(poller.polling).toBe(false);
    expect(onChanged).not.toHaveBeenCalled();

    await tick(3);
    expect(refetch).toHaveBeenCalledTimes(ACTIVE_POLL_MAX);
  });

  it('stop() halts an in-flight poll (v4 clears the interval on unmount)', async () => {
    const refetch = vi.fn(async () => "url('/api/v1/files/same')");
    const poller = new StoryBackgroundPoller();

    poller.start("url('/api/v1/files/same')", refetch, vi.fn());
    await tick(2);
    poller.stop();
    await tick(5);

    expect(refetch).toHaveBeenCalledTimes(2);
    expect(poller.polling).toBe(false);
  });

  it('re-starting mid-poll moves the baseline WITHOUT restarting the timer (v4 quirk)', async () => {
    // v4 assigns initialUrlRef BEFORE the "already polling" guard (`:97-103`), so
    // a second regenerate press re-baselines the comparison but does not reset
    // the budget or the interval. Ported exactly.
    let current = "url('/api/v1/files/a')";
    const refetch = vi.fn(async () => current);
    const onChanged = vi.fn();
    const poller = new StoryBackgroundPoller();

    poller.start(current, refetch, onChanged);
    await tick(1);

    // The backdrop moves, and the user presses regenerate again BEFORE the next
    // tick — the new value becomes the baseline, so it no longer counts as a change.
    current = "url('/api/v1/files/b')";
    poller.start(current, refetch, onChanged);
    await tick(1);
    expect(onChanged).not.toHaveBeenCalled();
    expect(poller.polling).toBe(true);

    // Only a move away from the NEW baseline settles it.
    current = "url('/api/v1/files/c')";
    await tick(1);
    expect(onChanged).toHaveBeenCalledTimes(1);
  });

  it('pins v4 cadence constants', () => {
    expect(ACTIVE_POLL_INTERVAL_MS).toBe(5000);
    expect(ACTIVE_POLL_MAX).toBe(36);
    expect(PASSIVE_POLL_INTERVAL_MS).toBe(30000);
  });
});
