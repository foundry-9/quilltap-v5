import { computed, inject, type Signal } from '@angular/core';
import { injectQuery, injectQueryClient } from '@tanstack/angular-query-experimental';

import { apiUrl } from '../core/api-url';
import { CoreClient } from '../core/core-client';
import type { ChatGalleryEntry, ChatGalleryResult, ChatGallerySource } from '../core/core-contract';
import { RealtimeService } from '../core/realtime.service';
import { chatKeys } from './chat-keys';

/**
 * The chat-gallery data layer — a port of v4
 * `app/salon/[id]/hooks/useChatGallery.ts` (85 lines, P4.D176/§C.3).
 *
 * ONE query answers two questions: the grid `qt-photo-gallery-modal` draws,
 * and the Organize drawer's `Gallery (N)` count beside the button that opens
 * it. They were two answers once (v4 bug 129): the count read an action that
 * did not exist, so the button gated on it never appeared at all. v5 never
 * had bug 129's SHAPE — its Gallery entry was unconditional and unnumbered
 * (a recorded divergence, now retired) — but the underlying invariant is the
 * same one v4 fixed: the count's fetch must be a verb that EXISTS.
 *
 * Realtime rides the EXISTING `chats` topic (`realtime-topic-map.ts`'s
 * `chats` row returns `chatKeys.detail(id)` as one prefix, which already
 * covers `chatKeys.gallery(id)` — no dedicated row). The 60s interval below is
 * only the offline fallback, gated by {@link RealtimeService.refetchInterval}
 * so it stops the moment the socket is up.
 *
 * @module chat/chat-gallery.api
 */

/** v4 `EMPTY_COUNTS` — all seven sources at 0. */
const EMPTY_COUNTS: Record<ChatGallerySource, number> = {
  'story-background': 0,
  avatar: 0,
  portrait: 0,
  generated: 0,
  attachment: 0,
  kept: 0,
  inline: 0,
};

/** What {@link injectChatGallery} hands back. */
export interface ChatGallery {
  /** The chat's gallery entries, `url` already resolved through {@link apiUrl}. */
  entries: Signal<ChatGalleryEntry[]>;
  counts: Signal<Record<ChatGallerySource, number>>;
  total: Signal<number>;
  /**
   * Whether the query has EVER answered. Distinct from `total() === 0`: a
   * chat with genuinely no pictures reads `(0)`; a query still pending, or
   * answered by a server that does not yet implement `chatGallery`, reads
   * `false` — the caller then renders the entry UNNUMBERED rather than
   * claiming a count it does not have (v4's ungated post-bug-129 shape).
   */
  hasData: Signal<boolean>;
  isLoading: Signal<boolean>;
  isError: Signal<boolean>;
  /** Re-read after something this client did put a new image in the chat. */
  invalidate: () => void;
}

/**
 * Read one chat's gallery. **Must be called from an injection context** (a
 * component field initializer) — the `injectCharacterSubprompts` precedent.
 *
 * @param chatId The conversation. A falsy id disables the query.
 * @param options.enabled Defer the read (e.g. until the modal is opened).
 */
export function injectChatGallery(
  chatId: () => string | null | undefined,
  options: { enabled?: () => boolean } = {},
): ChatGallery {
  const core = inject(CoreClient);
  const queryClient = injectQueryClient();
  const realtime = inject(RealtimeService);

  const enabled = computed(() => !!chatId() && (options.enabled?.() ?? true));
  const key = computed(() => chatKeys.gallery(chatId() ?? ''));

  const query = injectQuery(() => ({
    queryKey: key(),
    queryFn: async (): Promise<ChatGalleryResult> => {
      const data = await core.dispatchData({ type: 'chatGallery', chatId: chatId()! });
      const result = data as unknown as ChatGalleryResult;
      return {
        ...result,
        entries: (result.entries ?? []).map((entry) => ({ ...entry, url: apiUrl(entry.url) })),
      };
    },
    enabled: enabled(),
    refetchInterval: realtime.refetchInterval(60_000),
  }));

  return {
    entries: computed(() => query.data()?.entries ?? []),
    counts: computed(() => query.data()?.counts ?? EMPTY_COUNTS),
    total: computed(() => query.data()?.total ?? 0),
    hasData: computed(() => query.data() !== undefined),
    isLoading: computed(() => enabled() && query.isLoading()),
    isError: computed(() => query.isError()),
    invalidate: () => {
      const id = chatId();
      if (!id) return;
      void queryClient.invalidateQueries({ queryKey: chatKeys.gallery(id) });
    },
  };
}
