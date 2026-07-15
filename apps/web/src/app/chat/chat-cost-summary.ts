import { ChangeDetectionStrategy, Component, computed, inject, input } from '@angular/core';
import { injectQuery } from '@tanstack/angular-query-experimental';

import { CoreClient } from '../core/core-client';
import { Icon } from '../ui/icon';
import { chatCostKeys, fetchChatCost } from './chat-cost.api';
import { formatCostForDisplay, formatTokenCount } from './format-tokens';

/**
 * The chat-level token/cost aggregate shown in the conversation header — a port
 * of v4 `components/chat/ChatCostSummary.tsx`, COMPACT variant only.
 *
 * v4's Salon is the only caller and always passes `variant="compact"`
 * (`SalonView.tsx:990-1027`); the default (boxed) variant and the `detailed=true`
 * arm have no caller anywhere in v4 and are named deferrals of this order, not
 * omissions.
 *
 * ## The suppression rules are load-bearing
 *
 * v4 renders NOTHING (`:86-103`) when: `!show`; while loading (compact only —
 * the boxed variant shows a pulse, which is why the loading check is
 * variant-aware); or when `totalTokens === 0`. A fetch failure also renders
 * nothing: v4 catches, `console.warn`s, and leaves `costData` null on purpose —
 * a missing cost breakdown is a UI degradation, not something to alarm the user
 * about on every render. TanStack's error state gives us the same outcome for
 * free (`data()` stays undefined), so there is deliberately no error branch here.
 */
@Component({
  selector: 'qt-chat-cost-summary',
  changeDetection: ChangeDetectionStrategy.OnPush,
  template: `
    @if (cost(); as data) {
      <div class="flex items-center gap-2 text-xs qt-text-secondary">
        <!-- v4 shows the "tokens" word only from the md breakpoint up; below it,
             the bare number. Two spans, not one — the header is tight on mobile. -->
        <span class="hidden md:inline">{{ totalText() }} tokens</span>
        <span class="md:hidden">{{ totalText() }}</span>
        @if (data.estimatedCostUSD !== null) {
          <span class="qt-text-secondary/50">•</span>
          <span>{{ costText() }}</span>
          @if (data.priceSource === 'openrouter-estimate') {
            <span
              class="qt-text-warning cursor-help"
              title="Cost estimated using OpenRouter pricing data. Actual cost may vary."
            >
              <qt-icon name="info" class="w-3.5 h-3.5" />
            </span>
          }
          @if (data.priceSource === 'unavailable') {
            <span class="qt-text-secondary/50" title="Pricing data unavailable">*</span>
          }
        }
      </div>
    }
  `,
  imports: [Icon],
})
export class ChatCostSummary {
  private readonly core = inject(CoreClient);

  readonly chatId = input.required<string>();
  /** v4's `show` prop — `tokenDisplaySettings.showChatTotals`. Gates the FETCH too. */
  readonly show = input(true);
  /**
   * v4's `refreshKey` prop; the Salon passes `messages.length`. A change
   * re-fetches — here by being part of the query key.
   */
  readonly refreshKey = input<number | string>(0);

  private readonly costQuery = injectQuery(() => ({
    queryKey: chatCostKeys.cost(this.chatId(), this.refreshKey()),
    // v4 skips the fetch entirely when hidden (`:44-48` returns before fetching).
    enabled: this.show(),
    queryFn: () => fetchChatCost(this.core, this.chatId()),
  }));

  /**
   * The data, or undefined in every case v4 renders null for: hidden, loading
   * (compact shows no loading state), errored, or a zero-token chat.
   */
  protected readonly cost = computed(() => {
    if (!this.show()) return undefined;
    const data = this.costQuery.data();
    if (!data || data.totalTokens === 0) return undefined;
    return data;
  });

  protected readonly totalText = computed(() => formatTokenCount(this.cost()?.totalTokens ?? 0));
  protected readonly costText = computed(() =>
    formatCostForDisplay(this.cost()?.estimatedCostUSD ?? null),
  );
}
