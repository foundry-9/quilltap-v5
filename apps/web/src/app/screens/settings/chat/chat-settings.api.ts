import { computed, inject, signal, type Signal } from '@angular/core';
import { injectQuery, injectQueryClient } from '@tanstack/angular-query-experimental';

import { CoreClient } from '../../../core/core-client';
import type { ChatSettingsDto } from '../../../core/core-contract';

/**
 * The Chat-tab cards' shared read/write surface over the `chatSettings` /
 * `chatSettingsUpdate` dispatch — the v5 answer to v4's
 * `components/settings/chat-settings/hooks/useChatSettings.ts` (+ its thin
 * `ChatSettingsProvider` context).
 *
 * v4 loads the settings row ONCE in the provider and hands every card the row
 * plus a handler; here the shared `chatSettingsKeys.all` query key does the same
 * job — sixteen cards mounting at once dedupe into one GET, and a save seeds the
 * cache from the PUT response (v4's `mutateSettings(updated, false)` — no
 * revalidation).
 *
 * @module screens/settings/chat/chat-settings.api
 */

export const chatSettingsKeys = {
  all: ['chatSettings'] as const,
};

export async function fetchChatSettings(core: CoreClient): Promise<ChatSettingsDto> {
  const resp = await core.dispatchExpect({ type: 'chatSettings' }, 'chatSettings');
  return resp.data;
}

/**
 * v4's `PUT /api/v1/settings/chat` with a PARTIAL body. Nested bags are
 * spread-merged CLIENT-side by the caller before landing here (v4 does the same:
 * the PUT carries the whole updated bag, never a partial nested patch — the
 * server replaces the column wholesale).
 */
export async function updateChatSettings(
  core: CoreClient,
  settings: Record<string, unknown>,
): Promise<ChatSettingsDto> {
  const resp = await core.dispatchExpect({ type: 'chatSettingsUpdate', settings }, 'chatSettings');
  return resp.data;
}

/**
 * The per-card base: the shared settings query + a save that mirrors v4's
 * handler skeleton (`setSaving(true)` → PUT → seed the cache from the response →
 * `setSaving(false)`).
 *
 * Two deliberate departures from v4, both v5 house rules:
 *  - **`saving` is per-card**, not one provider-wide flag. v4's single provider
 *    disables EVERY card's inputs while any one card saves; the v5 precedent
 *    (`composition-mode-settings`, `autonomous-room-defaults`) is per-card, and
 *    a local dispatch round-trip is too quick for the difference to be visible.
 *  - **A failed save surfaces a visible `qt-error-alert`** (the dogfood-#6 rule)
 *    where v4 only `console.error`s and silently leaves the control mid-flight.
 *    The cache is then invalidated so the control snaps back to server truth
 *    rather than lying about an unsaved value.
 */
export abstract class ChatSettingsCard {
  protected readonly core = inject(CoreClient);
  private readonly queryClient = injectQueryClient();

  private readonly settingsQuery = injectQuery(() => ({
    queryKey: chatSettingsKeys.all,
    queryFn: () => fetchChatSettings(this.core),
  }));

  protected readonly settings: Signal<ChatSettingsDto | undefined> = computed(() =>
    this.settingsQuery.data(),
  );
  protected readonly loading = computed(() => this.settingsQuery.isPending());
  protected readonly saving = signal(false);
  protected readonly saveError = signal<string | null>(null);

  /**
   * PUT a partial settings body. `failure` is the card's v4 handler message —
   * v4 logs it to the console; v5 shows it.
   */
  protected async save(settings: Record<string, unknown>, failure: string): Promise<void> {
    this.saving.set(true);
    this.saveError.set(null);
    try {
      const updated = await updateChatSettings(this.core, settings);
      // v4 `mutateSettings(updatedSettings, false)` — seed, don't revalidate.
      this.queryClient.setQueryData(chatSettingsKeys.all, updated);
    } catch {
      this.saveError.set(failure);
      await this.queryClient.invalidateQueries({ queryKey: chatSettingsKeys.all });
    } finally {
      this.saving.set(false);
    }
  }
}
