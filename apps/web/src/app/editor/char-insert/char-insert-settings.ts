import { computed, inject, signal, type Signal } from '@angular/core';
import { QueryClient, injectQuery } from '@tanstack/angular-query-experimental';

import { CoreClient } from '../../core/core-client';
import {
  chatSettingsKeys,
  fetchChatSettings,
} from '../../screens/settings/chat/chat-settings.api';

/**
 * The two composer toggles, read where the feature lives.
 *
 * v4's `CharTypeaheadPlugin` runs its OWN `useQuery(queryKeys.settings.chat)`
 * rather than being handed a flag, and that shape is worth keeping: the
 * typeahead mounts in two hosts (the Salon composer and the Document-Mode pane),
 * and only one of them has a settings-passing parent. Sharing
 * `chatSettingsKeys.all` means both mounts and the sixteen settings cards still
 * dedupe into ONE GET, exactly as v4's single provider does.
 *
 * `?? true` is v4's default for both keys.
 *
 * The toggles gate the automatic `:` / `\` trigger ONLY — the toolbar pickers
 * are never gated, because an explicit button press cannot surprise anyone.
 *
 * @module editor/char-insert/char-insert-settings
 */
export interface CharInsertSettings {
  /** `chat_settings.composerEmoji ?? true` — the `:` typeahead. */
  emoji: Signal<boolean>;
  /** `chat_settings.composerUnicode ?? true` — the `\` typeahead. */
  unicode: Signal<boolean>;
}

/** Both flags ON — v4's defaults, and the answer when there is no query context. */
const DEFAULTS: CharInsertSettings = { emoji: signal(true), unicode: signal(true) };

/**
 * Must be called from an injection context (a component field initializer).
 *
 * Degrades to the defaults when no `QueryClient` is provided. That is not a
 * runtime path — the app root always provides one — but a host component under
 * test need not stand up a query stack just to render an editor, and answering
 * "both on" there is the same thing an unset settings row means.
 */
export function injectCharInsertSettings(): CharInsertSettings {
  if (!inject(QueryClient, { optional: true })) return DEFAULTS;

  const core = inject(CoreClient);
  const query = injectQuery(() => ({
    queryKey: chatSettingsKeys.all,
    queryFn: () => fetchChatSettings(core),
  }));

  return {
    emoji: computed(() => (query.data()?.['composerEmoji'] as boolean | undefined) ?? true),
    unicode: computed(() => (query.data()?.['composerUnicode'] as boolean | undefined) ?? true),
  };
}
