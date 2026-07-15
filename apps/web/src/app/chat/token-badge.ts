import { ChangeDetectionStrategy, Component, computed, input } from '@angular/core';

import { formatCostForDisplay, formatTokenCount } from './format-tokens';

/**
 * The per-message token/cost badge — a port of v4
 * `components/chat/TokenBadge.tsx`. Renders `<prompt> / <completion> tokens`
 * beside a message's timestamp.
 *
 * ## The cost arm is DEAD in v4 — and is ported dead
 *
 * `showCost` and `estimatedCostUSD` exist here because they exist in v4, but the
 * cost text can never render in either codebase: v4's only caller
 * (`MessageActionBar.tsx:198-204`) passes `showCost` but never passes
 * `estimatedCostUSD`, and there is no cost field on the Message type to pass —
 * so the arm's own `estimatedCostUSD != null` guard always fails. The
 * `showPerMessageCost` setting is therefore inert end-to-end.
 *
 * This is ported AS-IS rather than "fixed": inventing a per-message cost source
 * would be a behavior change, not a port, and the settings card that writes the
 * flag is already v4-faithful. If a per-message cost source ever lands, this arm
 * is where it plugs in.
 */
@Component({
  selector: 'qt-token-badge',
  changeDetection: ChangeDetectionStrategy.OnPush,
  template: `
    @if (visible()) {
      <div class="inline-flex items-center gap-2 text-xs qt-text-secondary">
        @if (showTokens()) {
          <span class="inline-flex items-center gap-1">
            <span title="Prompt tokens">{{ promptText() }}</span>
            <span>/</span>
            <span title="Completion tokens">{{ completionText() }}</span>
            <span class="qt-text-secondary/60">tokens</span>
          </span>
        }
        @if (showCost() && estimatedCostUSD() != null) {
          <span class="qt-text-secondary/80" title="Estimated cost">{{ costText() }}</span>
        }
      </div>
    }
  `,
})
export class TokenBadge {
  readonly promptTokens = input<number | null>(null);
  readonly completionTokens = input<number | null>(null);
  readonly totalTokens = input<number | null>(null);
  /** Always absent from v4's only call site — see the class docs. */
  readonly estimatedCostUSD = input<number | null>(null);
  readonly showTokens = input(true);
  readonly showCost = input(false);

  /** v4 `promptTokens ?? 0`. */
  protected readonly prompt = computed(() => this.promptTokens() ?? 0);
  /** v4 `completionTokens ?? 0`. */
  protected readonly completion = computed(() => this.completionTokens() ?? 0);
  /** v4 `totalTokens ?? (prompt + completion)` — the sum is the FALLBACK, not the value. */
  protected readonly total = computed(() => this.totalTokens() ?? this.prompt() + this.completion());

  /**
   * v4 `:32-35` — `total === 0 || (!showTokens && !showCost)` returns null.
   * The caller gates on its own condition too; this is the component's own
   * belt-and-braces, carried over.
   */
  readonly visible = computed(() => !(this.total() === 0 || (!this.showTokens() && !this.showCost())));

  protected readonly promptText = computed(() => formatTokenCount(this.prompt()));
  protected readonly completionText = computed(() => formatTokenCount(this.completion()));
  protected readonly costText = computed(() => formatCostForDisplay(this.estimatedCostUSD()));
}
