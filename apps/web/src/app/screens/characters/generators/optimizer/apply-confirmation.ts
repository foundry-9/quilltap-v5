import { ChangeDetectionStrategy, Component, input, output } from '@angular/core';

import { Icon } from '../../../../ui/icon';
import type { OptimizerSuggestion } from '../detail-generators.api';
import { FIELD_BADGE_CLASS, FIELD_LABELS } from './field-meta';

export interface OptimizerAcceptedChangeView {
  suggestion: OptimizerSuggestion;
  finalValue: string;
}

function truncate(text: unknown, maxLength = 120): string {
  const str = typeof text === 'string' ? text : String(text ?? '');
  if (str.length <= maxLength) return str;
  return str.slice(0, maxLength).trimEnd() + '…';
}

/** v4 `components/characters/optimizer/components/ApplyConfirmation.tsx`. */
@Component({
  selector: 'qt-apply-confirmation',
  changeDetection: ChangeDetectionStrategy.OnPush,
  imports: [Icon],
  template: `
    @if (changes().length === 0) {
      <div class="flex flex-col items-center gap-4 py-8 text-center">
        <qt-icon name="file" class="qt-text-secondary w-12 h-12" />
        <div class="flex flex-col gap-1">
          <h3 class="qt-section-title text-base">No Changes Accepted</h3>
          <p class="qt-section-subtitle text-sm">
            It appears you have declined all proposed amendments. Return to the review to reconsider,
            or close the proceedings altogether.
          </p>
        </div>
        <button type="button" class="qt-button-secondary" (click)="back.emit()">Return to Review</button>
      </div>
    } @else {
      <div class="flex flex-col gap-4">
        <div class="qt-card qt-bg-primary/5 qt-border-primary/20 p-4">
          <div class="mb-1 flex items-center gap-2">
            <qt-icon name="check-circle" class="w-4 h-4 text-primary" />
            <h3 class="qt-section-title text-sm">
              {{ changes().length }} {{ changes().length === 1 ? 'Amendment' : 'Amendments' }} Awaiting
              Commission
            </h3>
          </div>
          <p class="qt-body-sm qt-text-secondary">
            The following refinements shall be inscribed permanently into the character record. This
            act, once performed, admits of no mechanical undoing — though you may of course return and
            amend matters by hand thereafter.
          </p>
        </div>

        <div class="flex flex-col gap-2">
          @for (change of changes(); track change.suggestion.id) {
            <div class="qt-card flex flex-col gap-2 p-3">
              <div class="flex flex-wrap items-center gap-2">
                <span [class]="badgeClass(change.suggestion) + ' text-xs'">{{
                  displayLabel(change.suggestion)
                }}</span>
                @if (change.suggestion.currentValue) {
                  <span class="qt-caption">Revised</span>
                } @else {
                  <span class="qt-badge-success text-xs">Newly Added</span>
                }
              </div>
              <p class="qt-body-sm qt-text-secondary leading-relaxed">{{ truncated(change.finalValue) }}</p>
            </div>
          }
        </div>

        <div class="flex justify-between gap-3 pt-2">
          <button
            type="button"
            [disabled]="applying()"
            class="qt-button-secondary disabled:opacity-50"
            (click)="back.emit()"
          >
            <qt-icon name="arrow-left" class="w-4 h-4" />
            Back to Review
          </button>

          <button
            type="button"
            [disabled]="applying()"
            class="qt-button-primary disabled:opacity-50"
            (click)="apply.emit()"
          >
            @if (applying()) {
              <qt-icon name="refresh" class="w-4 h-4 animate-spin" />
              Applying Refinements…
            } @else {
              <qt-icon name="check" class="w-4 h-4" />
              Apply {{ changes().length }} {{ changes().length === 1 ? 'Change' : 'Changes' }}
            }
          </button>
        </div>
      </div>
    }
  `,
})
export class ApplyConfirmation {
  readonly changes = input.required<OptimizerAcceptedChangeView[]>();
  readonly applying = input(false);

  readonly apply = output<void>();
  readonly back = output<void>();

  protected displayLabel(suggestion: OptimizerSuggestion): string {
    const fieldLabel = FIELD_LABELS[suggestion.field] ?? suggestion.field;
    const newItemName = suggestion.name ?? suggestion.title;
    if (suggestion.subName) return `${fieldLabel}: ${suggestion.subName}`;
    if (newItemName) return `${fieldLabel}: ${newItemName}`;
    return fieldLabel;
  }
  protected badgeClass(suggestion: OptimizerSuggestion): string {
    return FIELD_BADGE_CLASS[suggestion.field] ?? 'qt-badge-secondary';
  }
  protected truncated(value: string): string {
    return truncate(value);
  }
}
