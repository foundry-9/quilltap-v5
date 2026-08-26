/**
 * The story-background client api (dogfood finding #9) — the resolver plus the
 * two polls, ported from v4 `hooks/useStoryBackground.ts`.
 *
 * The resolver maps a chat's background image into the CSS
 * `--story-background-url` value the Salon layout consumes. v5 prefers the
 * returned `fileId` mapped through the store-backed byte route
 * (`/api/v1/files/{id}` — the P4.6ac idiom) over v4's `backgroundUrl` path
 * string, which is a v4-server path.
 *
 * **Display is unconditional; the flag gates only POLLING** (the P4.6am survey
 * rule, and v4's own shape: `enablePassivePolling` is a separate argument from
 * the query's `enabled`). A chat that has a background shows it whether or not
 * story-background generation is switched on.
 *
 * @module screens/salon/story-background.api
 */

import type { CoreClient } from '../../core/core-client';
import type {
  ChatBackgroundDto,
  ChatGetBackgroundRequest,
  ChatRegenerateBackgroundRequest,
} from '../../core/core-contract';

export type { ChatRegenerateBackgroundRequest } from '../../core/core-contract';
import { fileUrl } from '../../images/image-urls';

export const storyBackgroundKeys = {
  /** Per-chat background query key (a sibling of the `['chat', id]` detail key). */
  background: (chatId: string) => ['chat', chatId, 'background'] as const,
};

/**
 * Fetch the chat's story background and return the CSS `url('…')` value to bind
 * on `--story-background-url`, or null when the chat has no background.
 */
export async function fetchChatBackgroundVar(
  core: CoreClient,
  chatId: string,
): Promise<string | null> {
  const req: ChatGetBackgroundRequest = { type: 'chatGetBackground', chatId };
  const data = (await core.dispatchData(req)) as unknown as ChatBackgroundDto;
  const fileId = data.fileId ?? null;
  return fileId ? `url('${fileUrl(fileId)}')` : null;
}

// ---------------------------------------------------------------------------
// Regeneration (the P4.6ap round's Shared contract §2)
// ---------------------------------------------------------------------------

/**
 * v4's successResponse data for both arms (§2). `queued` is true even for the
 * "already in progress" arm — the request was accepted either way, so the
 * client treats both as success and shows the returned message.
 */
export interface RegenerateBackgroundResult {
  message: string;
  queued: boolean;
  jobId: string;
}

/**
 * POST the regeneration request (v4 `?action=regenerate-background`,
 * `useChatControls.ts:397-416`). A rejection carries the server's own message —
 * v4 throws `errorData.error` and shows it verbatim, which is what makes the §2
 * badRequest strings ("Story backgrounds are not enabled. …") reach the user.
 */
export async function regenerateChatBackground(
  core: CoreClient,
  chatId: string,
): Promise<RegenerateBackgroundResult> {
  const req: ChatRegenerateBackgroundRequest = { type: 'chatRegenerateBackground', chatId };
  const data = await core.dispatchData(req);
  return data as unknown as RegenerateBackgroundResult;
}

// ---------------------------------------------------------------------------
// The two polls (v4 `hooks/useStoryBackground.ts`)
// ---------------------------------------------------------------------------

/** v4 `refetchInterval: enablePassivePolling ? 30000 : false` (`:68`). */
export const PASSIVE_POLL_INTERVAL_MS = 30_000;

/** v4 `setInterval(…, 5000)` (`:128`). */
export const ACTIVE_POLL_INTERVAL_MS = 5_000;

/** v4 `const maxPolls = 36 // 3 minutes at 5-second intervals` (`:105`). */
export const ACTIVE_POLL_MAX = 36;

/**
 * The ACTIVE regeneration poll (v4 `startPolling`, `:95-128`): after a
 * regenerate is queued, re-fetch the background every 5s until the resolved
 * value CHANGES from the one captured at start, or 36 polls (3 minutes) run out.
 *
 * A generated background is not pushed to the client — this poll is how the new
 * backdrop ever appears without a reload.
 *
 * v4's re-entrancy shape is reproduced exactly, quirk and all: `start()`
 * re-captures the baseline value BEFORE the "already polling" guard, so calling
 * it again mid-poll moves the comparison baseline WITHOUT restarting the timer
 * or resetting the budget. That ordering is v4's (`:97-103`) and is load-bearing
 * for a second regenerate press: the in-flight poll then compares against the
 * value seen at the second press.
 */
export class StoryBackgroundPoller {
  private handle: ReturnType<typeof setInterval> | null = null;
  private initial: string | null = null;
  private count = 0;

  /** True while the interval is live (v4's `polling` state). */
  get polling(): boolean {
    return this.handle !== null;
  }

  /**
   * @param current  The resolved value now — the baseline a change is measured against.
   * @param refetch  Re-fetch and resolve to the CURRENT value.
   *
   * v4 `f3892158d` moved the change CALLBACK out of this loop and into the
   * caller's shared transition effect, "which sees the same transition whether
   * it arrived by poll or by push": with the `chats:<id>` hint invalidating the
   * background key, the new value can land without this loop being the one that
   * noticed. The loop still stops itself on a change, and still gives up after
   * {@link ACTIVE_POLL_MAX} tries.
   */
  start(current: string | null, refetch: () => Promise<string | null>): void {
    // v4 stores the baseline BEFORE the already-polling guard — see the class docs.
    this.initial = current;
    if (this.handle !== null) return;

    this.count = 0;
    this.handle = setInterval(() => {
      void (async () => {
        this.count++;
        const next = await refetch();
        if (next !== this.initial) {
          this.stop();
          return;
        }
        if (this.count >= ACTIVE_POLL_MAX) {
          this.stop();
        }
      })();
    }, ACTIVE_POLL_INTERVAL_MS);
  }

  /** v4 `stopPolling` — clears the interval and drops the baseline. */
  stop(): void {
    if (this.handle !== null) {
      clearInterval(this.handle);
      this.handle = null;
    }
    this.initial = null;
    this.count = 0;
  }
}
