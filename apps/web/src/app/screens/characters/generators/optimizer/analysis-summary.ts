import { ChangeDetectionStrategy, Component, input } from '@angular/core';

import { Icon } from '../../../../ui/icon';
import type { OptimizerAnalysis } from '../detail-generators.api';

/**
 * v4 `components/characters/optimizer/components/AnalysisSummary.tsx` — the
 * behavioral-pattern summary shown once the analysing step completes.
 */
@Component({
  selector: 'qt-analysis-summary',
  changeDetection: ChangeDetectionStrategy.OnPush,
  imports: [Icon],
  template: `
    <div class="flex flex-col gap-4">
      <div class="qt-card p-4">
        <div class="mb-2 flex items-center gap-2">
          <qt-icon name="shield" class="w-4 h-4 text-primary flex-shrink-0" />
          <h3 class="qt-section-title text-sm">Analysis Complete</h3>
          <span class="qt-badge-info ml-auto"
            >{{ memoryCount() }} {{ memoryCount() === 1 ? 'memory' : 'memories' }} consulted</span
          >
        </div>
        <p class="qt-body text-sm leading-relaxed">{{ analysis().summary }}</p>
      </div>

      @if (analysis().behavioralPatterns.length > 0) {
        <div class="flex flex-col gap-2">
          <h4 class="qt-label qt-text-secondary text-xs uppercase tracking-wider">
            Observed Behavioural Tendencies
          </h4>
          <div class="flex flex-col gap-2">
            @for (pattern of analysis().behavioralPatterns; track $index) {
              <div class="qt-card flex flex-col gap-1.5 overflow-hidden p-3">
                <div class="flex flex-wrap items-start justify-between gap-3">
                  <span class="text-sm font-semibold leading-snug text-foreground">{{
                    pattern.pattern
                  }}</span>
                  <span [class]="frequencyBadgeClass(pattern.frequency) + ' flex-shrink whitespace-normal text-left'">{{
                    pattern.frequency
                  }}</span>
                </div>
                <p class="qt-text-secondary text-xs italic leading-relaxed">{{ pattern.evidence }}</p>
              </div>
            }
          </div>
        </div>
      }
    </div>
  `,
})
export class AnalysisSummary {
  readonly analysis = input.required<OptimizerAnalysis>();
  readonly memoryCount = input.required<number>();

  /** v4 `FrequencyBadge` (`:19-36`). */
  protected frequencyBadgeClass(frequency: string): string {
    const normalized = (frequency ?? '').toLowerCase();
    if (
      normalized.includes('often') ||
      normalized.includes('frequent') ||
      normalized.includes('always') ||
      normalized.includes('consistent')
    ) {
      return 'qt-badge-info';
    }
    if (
      normalized.includes('occasion') ||
      normalized.includes('sometimes') ||
      normalized.includes('moderate')
    ) {
      return 'qt-badge-warning';
    }
    if (
      normalized.includes('rare') ||
      normalized.includes('seldom') ||
      normalized.includes('infreq')
    ) {
      return 'qt-badge-outline';
    }
    return 'qt-badge-secondary';
  }
}
