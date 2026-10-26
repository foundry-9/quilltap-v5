import { NgTemplateOutlet } from '@angular/common';
import {
  ChangeDetectionStrategy,
  Component,
  computed,
  input,
  output,
  type TemplateRef,
} from '@angular/core';

import { groupsOf, type ConditionJoiner } from '../../pascal/tool-draft';

/**
 * ConditionGroups — the AND / OR layout shared by the gate and the outcome
 * table (v4 `components/custom-tools/ConditionGroups.tsx`, `a53024d3c`).
 *
 * A chip list is flat in the draft, each chip carrying how it joins the one
 * before it. Here it is drawn as lines: the chips of one group share a line, an
 * OR starts the next, and a hairline separates them — so "a AND b OR a AND c"
 * reads as two lines without a parenthesis in sight. The joiner between two
 * chips is a two-state toggle; flipping it splits or merges lines.
 */

/** The AND / OR between two chips (v4 `JoinerToggle`). */
@Component({
  selector: 'qt-joiner-toggle',
  changeDetection: ChangeDetectionStrategy.OnPush,
  template: `
    <button
      type="button"
      class="qt-button qt-button-ghost qt-button-sm font-mono text-xs"
      (click)="joinerChange.emit(joiner() === 'or' ? 'and' : 'or')"
      [disabled]="disabled()"
      [attr.aria-label]="joiner() === 'or' ? 'Joined by OR — switch to AND' : 'Joined by AND — switch to OR'"
      [attr.title]="
        joiner() === 'or'
          ? 'OR starts a new line: any line that holds will do. Click to fold it into the line above.'
          : 'AND keeps this test on the same line: every test on a line must hold. Click to start a new line.'
      "
    >
      {{ joiner() === 'or' ? 'OR' : 'AND' }}
    </button>
  `,
})
export class JoinerToggle {
  readonly joiner = input.required<ConditionJoiner>();
  readonly disabled = input(false);

  readonly joinerChange = output<ConditionJoiner>();
}

/**
 * Draw a chip list as its lines, with a toggle before every chip but the first.
 * The chip itself is the caller's template (v4's `renderChip`); the lines are
 * re-grouped from the input by a plain `computed` — no `linkedSignal`, nothing
 * here is local state seeded from an input.
 */
@Component({
  selector: 'qt-condition-groups',
  changeDetection: ChangeDetectionStrategy.OnPush,
  imports: [JoinerToggle, NgTemplateOutlet],
  template: `
    @for (group of groups(); track group[0].id; let g = $index) {
      <div [attr.class]="g > 0 ? 'border-t qt-border-default pt-1 mt-1' : null">
        <div class="flex items-center gap-2 flex-wrap">
          @for (condition of group; track condition.id; let i = $index) {
            @if (g > 0 || i > 0) {
              <qt-joiner-toggle
                [joiner]="g > 0 && i === 0 ? 'or' : 'and'"
                [disabled]="disabled()"
                (joinerChange)="joinerChange.emit({ id: condition.id, joiner: $event })"
              />
            }
            <ng-container *ngTemplateOutlet="chip(); context: { $implicit: condition }" />
          }
        </div>
      </div>
    }
  `,
})
// eslint-disable-next-line @typescript-eslint/no-explicit-any
export class ConditionGroups<C extends { id: string; joiner?: ConditionJoiner } = any> {
  readonly conditions = input.required<readonly C[]>();
  readonly disabled = input(false);
  /** Renders one chip: the caller's `<ng-template let-condition>`. */
  readonly chip = input.required<TemplateRef<{ $implicit: C }>>();

  /** Set one chip's joiner. */
  readonly joinerChange = output<{ id: string; joiner: ConditionJoiner }>();

  protected readonly groups = computed(() => groupsOf(this.conditions()));
}
