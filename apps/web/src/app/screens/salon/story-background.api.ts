/**
 * The story-background client api (dogfood finding #9). Resolves a chat's
 * background image into the CSS `--story-background-url` value the Salon layout
 * consumes. v5 prefers the returned `fileId` mapped through the store-backed
 * byte route (`/api/v1/files/{id}` — the P4.6ac idiom) over v4's `backgroundUrl`
 * path string, which is a v4-server path.
 *
 * Fetched once per chat open — there is NO 30s regeneration poll. v4's poll
 * (`useStoryBackground.ts` `enablePassivePolling`) gates background *generation*,
 * which is an unported subsystem (a named lane-C tier-3 deferral; lane A
 * refusal-arms `regenerate-background`), not display. Display is unconditional.
 */

import type { CoreClient } from '../../core/core-client';
import type { ChatBackgroundDto, ChatGetBackgroundRequest } from '../../core/core-contract';
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
