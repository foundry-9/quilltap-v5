import { DestroyRef, Injectable, Injector, inject, signal } from '@angular/core';
import { QueryClient } from '@tanstack/query-core';

import { CoreClient } from '../../core/core-client';
import type { ChatSettingsDto } from '../../core/core-contract';
import {
  chatSettingsKeys,
  fetchChatSettings,
} from '../../screens/settings/chat/chat-settings.api';

/**
 * `smartTypographySettings.displayQuotes`, for the ONE message renderer.
 *
 * v4 reads this setting inside `MessageContent.tsx` itself rather than
 * threading it down as a prop, and says why: that component is the message
 * renderer for every surface that has one — the Salon, streaming messages,
 * thinking blocks, the help chat, the Brahma console — and quote curling is a
 * display preference that should apply to all of them consistently. v5 has the
 * same shape (`qt-message-content` is the single renderer), so it reads the
 * setting the same way.
 *
 * What differs is the plumbing. v4 leans on TanStack Query deduping ONE
 * `useQuery` across every mounted message; an Angular component doing that would
 * open a query per row and would hard-require a `QueryClient` in every spec that
 * renders a message. So the read lives here instead: one root singleton, one
 * cache subscription, one signal every renderer reads.
 *
 * **Absent a `QueryClient` the service is inert and the signal stays `false`** —
 * which is the correct answer for a bare component spec (v4's default when the
 * setting is unset is `false` too), and keeps `qt-message-content` mountable
 * with no query layer at all, exactly as it was before Layer 1.6.
 *
 * @module chat/render/display-quotes
 */
@Injectable({ providedIn: 'root' })
export class DisplayQuotesSetting {
  /** `null` when nothing provided TanStack Query (bare specs) — see the note. */
  private readonly queryClient = inject(QueryClient, { optional: true });
  private readonly injector = inject(Injector);
  private readonly destroyRef = inject(DestroyRef);

  /** Whether messages should render with curled quotes. v4's default: `false`. */
  readonly displayQuotes = signal(false);

  constructor() {
    const client = this.queryClient;
    if (!client) return;

    const read = (): void => {
      const row = client.getQueryData<ChatSettingsDto>(chatSettingsKeys.all);
      this.displayQuotes.set(readDisplayQuotes(row));
    };

    read();
    const unsubscribe = client.getQueryCache().subscribe((event) => {
      if (event.query.queryKey[0] === chatSettingsKeys.all[0]) read();
    });
    this.destroyRef.onDestroy(unsubscribe);

    // Nobody else may have asked for the row: the Brahma console and the help
    // chat render messages without ever mounting a settings card or the Salon.
    // `ensureQueryData` is a no-op when the row is already cached or in flight.
    void client
      .ensureQueryData({
        queryKey: chatSettingsKeys.all,
        queryFn: () => fetchChatSettings(this.injector.get(CoreClient)),
      })
      .catch(() => {
        // A settings fetch that fails leaves the display preference at its
        // default — a renderer must never surface a settings error.
      });
  }
}

/** The tri-level read v4 performs inline: row → bag → key, defaulting `false`. */
export function readDisplayQuotes(row: ChatSettingsDto | undefined | null): boolean {
  const bag = row?.['smartTypographySettings'] as { displayQuotes?: boolean } | undefined | null;
  return bag?.displayQuotes ?? false;
}
