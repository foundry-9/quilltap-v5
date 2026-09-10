import { ChangeDetectionStrategy, Component, computed, input } from '@angular/core';

import type { RouteAttempt } from '../core/core-contract';
import { ProviderModelBadge } from './sidebar/provider-model-badge';
import {
  ROUTE_OUTCOME_GLYPH,
  collapseRouteTrail,
  describeRouteAttempt,
  routeOutcomeLabel,
  type RouteTrailRow,
} from './route-trail-display';

/**
 * The call sheet under an assistant avatar: every connection profile tried
 * for the turn, first asked at the top, the one that answered at the bottom
 * (v4 `components/ui/RouteTrailBadge.tsx`).
 *
 * A row that fell over on its own is struck through and marked ❌; one that
 * declined on content grounds is struck through and marked 🚫. The row that
 * answered wears neither, so a one-row trail is indistinguishable from the
 * plain `qt-provider-model-badge` it replaces (v4's own acceptance criterion
 * — see `message-row.ts`'s template, which mounts this ONLY when
 * `message().routeTrail` is a non-empty array and falls back to the plain
 * badge otherwise).
 *
 * Theme authors: this list has no `qt-*` hook of its own — target it through
 * `[aria-label="Models tried for this reply"]` (v4's own only hook).
 */
@Component({
  selector: 'qt-route-trail-badge',
  changeDetection: ChangeDetectionStrategy.OnPush,
  imports: [ProviderModelBadge],
  template: `
    @if (rows().length) {
    <ul class="flex flex-col items-center gap-0.5" aria-label="Models tried for this reply">
      @for (row of rows(); track row.profileId + '-' + $index) {
        <li class="inline-flex items-center gap-1">
          @if (row.outcome !== 'answered') {
            <span role="img" [attr.aria-label]="outcomeLabel(row.outcome)" class="text-[10px] leading-none">{{
              glyph(row.outcome)
            }}</span>
            <s class="inline-flex items-center">
              <qt-provider-model-badge
                [provider]="row.provider"
                [modelName]="row.modelName"
                [size]="size()"
                [titleOverride]="hoverText(row)"
              />
            </s>
          } @else {
            <qt-provider-model-badge
              [provider]="row.provider"
              [modelName]="row.modelName"
              [size]="size()"
              [titleOverride]="hoverText(row)"
            />
          }
        </li>
      }
    </ul>
    }
  `,
})
export class RouteTrailBadge {
  /** The turn's route trail, oldest attempt first. Never empty — callers fall
   *  back to a plain `qt-provider-model-badge` when there is no trail. */
  readonly routeTrail = input.required<RouteAttempt[]>();
  /** Badge size, passed straight through to each row's `qt-provider-model-badge`. */
  readonly size = input<'xs' | 'sm'>('xs');

  protected readonly rows = computed(() => collapseRouteTrail(this.routeTrail()));

  protected hoverText(row: RouteTrailRow): string {
    return describeRouteAttempt(row);
  }

  protected glyph(outcome: 'failed' | 'refused'): string {
    return ROUTE_OUTCOME_GLYPH[outcome];
  }

  protected outcomeLabel(outcome: 'failed' | 'refused' | 'answered'): string {
    return routeOutcomeLabel(outcome);
  }
}
