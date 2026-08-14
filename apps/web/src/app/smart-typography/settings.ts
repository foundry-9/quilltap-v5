import { DestroyRef, Injectable, Injector, inject, signal, untracked } from '@angular/core';
import { QueryClient } from '@tanstack/query-core';

import { CoreClient } from '../core/core-client';
import type { ChatSettingsDto } from '../core/core-contract';
import { chatSettingsKeys, fetchChatSettings } from '../screens/settings/chat/chat-settings.api';
import type { SmartTypographyOptions } from './engine';

/**
 * `chat_settings.smartTypographySettings`, live, for the two consumers that
 * cannot be handed it as an input.
 *
 * v4 reads this setting twice, in both cases from inside the feature rather
 * than from a prop, and says why each time:
 *
 *  - `MessageContent.tsx:345` — that component is the message renderer for
 *    every surface that has one (the Salon, streaming messages, thinking
 *    blocks, the help chat, the Brahma console), and quote curling is a display
 *    preference that should apply to all of them consistently.
 *  - `SmartTypographyPlugin.tsx:109` — the composer plugin runs its own
 *    `useQuery` and keeps the result in a ref, so the registered command handler
 *    never needs re-registering when the settings change.
 *
 * v5 has the same two consumers and reads the row once, here. The Angular shape
 * differs from v4's `useQuery`-per-consumer: an Angular component doing that
 * would open a query per mounted message row AND would hard-require a
 * `QueryClient` in every spec that renders a message. This is one root
 * singleton, one cache subscription, two signals.
 *
 * **Absent a `QueryClient` the service is inert** and both signals keep their
 * defaults — which are v4's own defaults for an unset bag (`displayQuotes`
 * false, `dashes`/`ellipsis` true), and which keep `qt-message-content` and
 * `qt-rich-editor` mountable with no query layer at all.
 *
 * @module smart-typography/settings
 */
@Injectable({ providedIn: 'root' })
export class SmartTypographySettings {
  /** `null` when nothing provided TanStack Query (bare specs) — see the note. */
  private readonly queryClient = inject(QueryClient, { optional: true });
  private readonly injector = inject(Injector);
  private readonly destroyRef = inject(DestroyRef);

  /** Whatever is already cached, or v4's defaults. See the note on writes below. */
  private readonly seed = this.queryClient?.getQueryData<ChatSettingsDto>(chatSettingsKeys.all);

  /** Part A: render messages with curled quotes. v4's default: `false`. */
  readonly displayQuotes = signal(readDisplayQuotes(this.seed));

  /** Part B: the type-time rules. v4's default for each: `true`. */
  readonly typing = signal<SmartTypographyOptions>(readTypingOptions(this.seed));

  constructor() {
    const client = this.queryClient;
    if (!client) return;

    /**
     * ⚠ Every write here is `untracked`, and the INITIAL values are seeded in
     * the field initializers above rather than written in this constructor.
     *
     * A root service is constructed lazily, at its first `inject()` — which
     * for this one happens inside `qt-message-content`'s constructor, i.e.
     * WHILE the parent template is executing. A template is a reactive
     * consumer, so a signal write in that window is `NG0600` ("writing to
     * signals is not allowed in a computed or an effect"), the whole render
     * unwinds, and the surrounding screen comes up EMPTY. The cache callback
     * has the same exposure: `setQueryData` may be called from anywhere,
     * including inside a reactive context.
     */
    const read = (): void => {
      const row = client.getQueryData<ChatSettingsDto>(chatSettingsKeys.all);
      untracked(() => {
        this.displayQuotes.set(readDisplayQuotes(row));
        const next = readTypingOptions(row);
        const current = this.typing();
        // Keep the reference stable while the values are unchanged: the plugin
        // reads this through a closure on every keystroke.
        if (next.dashes !== current.dashes || next.ellipsis !== current.ellipsis) {
          this.typing.set(next);
        }
      });
    };

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
        // A settings fetch that fails leaves both preferences at their
        // defaults — neither a renderer nor a composer may surface a settings
        // error.
      });
  }
}

/** The bag as stored, with every key optional (v4 `SmartTypographySettings`). */
interface StoredBag {
  displayQuotes?: boolean;
  dashes?: boolean;
  ellipsis?: boolean;
}

function bagOf(row: ChatSettingsDto | undefined | null): StoredBag | undefined | null {
  return row?.['smartTypographySettings'] as StoredBag | undefined | null;
}

/** The tri-level read v4 performs inline: row → bag → key, defaulting `false`. */
export function readDisplayQuotes(row: ChatSettingsDto | undefined | null): boolean {
  return bagOf(row)?.displayQuotes ?? false;
}

/** v4 `SmartTypographyPlugin` :121-122 — each rule defaults to ON when unset. */
export function readTypingOptions(row: ChatSettingsDto | undefined | null): SmartTypographyOptions {
  const bag = bagOf(row);
  return { dashes: bag?.dashes ?? true, ellipsis: bag?.ellipsis ?? true };
}
