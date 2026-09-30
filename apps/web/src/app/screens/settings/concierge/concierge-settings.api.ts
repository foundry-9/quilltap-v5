import { computed } from '@angular/core';
import { injectQueryClient, type QueryClient } from '@tanstack/angular-query-experimental';

import type { ChatSettingsDto } from '../../../core/core-contract';
import { effectiveConcierge } from '../../../chat/concierge-duty';
import { ChatSettingsCard, chatSettingsKeys } from '../chat/chat-settings.api';
import {
  DEFAULT_CONCIERGE_SETTINGS,
  type ConciergeSettings,
  type ConciergeSettingsUpdate,
} from '../chat/chat-settings.types';

/**
 * The Concierge tab's shared read/write surface (v4 `3b463d6b1`
 * `useChatSettings.handleConciergeUpdate` + `ConciergeTabContent`'s effective
 * object).
 *
 * @module screens/settings/concierge/concierge-settings.api
 */

// The on-duty derivation lives in the chat tree's pure home (no Angular /
// TanStack / settings-card imports); re-exported so the Settings tab and its
// spec import it from where they always have.
export { effectiveConcierge, isConciergeOnDuty } from '../../../chat/concierge-duty';

/**
 * v4 `handleConciergeUpdate`'s merge: top-level fields replace, `display` and
 * `preScreen` deep-merge over defaults + current. The result is the WHOLE
 * object — the server's `ConciergeSettingsSchema.safeParse` REPLACES the
 * column, so a partial here would silently reset every sibling to its default.
 */
export function mergeConciergeUpdate(
  current: ConciergeSettings | null | undefined,
  updates: ConciergeSettingsUpdate,
): ConciergeSettings {
  const base = current || DEFAULT_CONCIERGE_SETTINGS;
  const { display, preScreen, ...topLevel } = updates;
  return {
    ...DEFAULT_CONCIERGE_SETTINGS,
    ...base,
    ...topLevel,
    display: { ...DEFAULT_CONCIERGE_SETTINGS.display, ...base.display, ...(display ?? {}) },
    preScreen: {
      ...DEFAULT_CONCIERGE_SETTINGS.preScreen,
      ...base.preScreen,
      ...(preScreen ?? {}),
    },
  };
}

/**
 * One save chain per query client: every Concierge save waits for the one
 * before it, THEN reads the cache. v4's one provider-wide `saving` flag
 * disables every card while any saves, and its `settingsRef` hands the next
 * merge the latest row; v5's `saving` is per-card (a `ChatSettingsCard` house
 * rule), so two cards CAN save at once — and since every save sends the whole
 * object, the second would otherwise overwrite the first with a stale merge.
 */
const saveChains = new WeakMap<QueryClient, Promise<void>>();

/**
 * The base every Concierge card extends: the effective object for rendering,
 * and `update()` — the one write path.
 */
export abstract class ConciergeSettingsCard extends ChatSettingsCard {
  private readonly conciergeQueryClient = injectQueryClient();

  /** The effective settings the card renders (never a merge input — see `update`). */
  protected readonly concierge = computed(() =>
    effectiveConcierge(this.settings()?.conciergeSettings),
  );

  /**
   * v4 `handleConciergeUpdate(updates)`: merge over the LATEST cached row AT
   * SEND TIME (never the render-time `concierge()` snapshot), PUT the whole
   * `conciergeSettings` object, seed the cache from the response. A failure
   * shows v4's failure message in the card's `qt-error-alert` (v5's
   * `ChatSettingsCard` rule; v4 only logs `Failed to update Concierge
   * settings` to the console).
   */
  protected update(updates: ConciergeSettingsUpdate): Promise<void> {
    const qc = this.conciergeQueryClient;
    const run = (saveChains.get(qc) ?? Promise.resolve()).then(async () => {
      const latest = qc.getQueryData<ChatSettingsDto>(chatSettingsKeys.all);
      // v4: `if (!latestSettings) return`.
      if (!latest) return;
      await this.save(
        { conciergeSettings: mergeConciergeUpdate(latest.conciergeSettings, updates) },
        "Failed to update the Concierge's settings",
      );
    });
    // The STORED link never rejects: a failed save must not poison every later
    // save on this query client (`.then` skips its callback on a rejection).
    // The caller still sees this save's own outcome through `run`.
    saveChains.set(
      qc,
      run.catch(() => undefined),
    );
    return run;
  }
}
