import { ChangeDetectionStrategy, Component, input, output } from '@angular/core';

import { Icon } from '../ui/icon';

const plural = (n: number, word: string): string => `${n} ${word}${n === 1 ? '' : 's'}`;

/**
 * A tier row in a character's browser: a shared store the character borrows
 * from (a group, a project, Quilltap General), entered like a folder. A port
 * of v4 `components/item-browser/ItemTierRow.tsx` (`4fbdad186`). The badge
 * says `Shared` for General while the tier's LABEL (from `tierLabel`) says
 * `General` — both verbatim (survey trap 5).
 */
@Component({
  selector: 'qt-item-tier-row',
  changeDetection: ChangeDetectionStrategy.OnPush,
  imports: [Icon],
  template: `
    <button
      type="button"
      class="qt-card-interactive w-full py-2 px-3 flex items-center gap-2 text-left"
      (click)="opened.emit()"
      [attr.data-testid]="testId()"
    >
      <qt-icon name="folder" class="w-4 h-4 qt-text-secondary" />
      <span class="text-sm text-foreground flex-1 min-w-0 truncate">{{ label() }}</span>
      <span class="qt-badge qt-badge-wardrobe-shared">{{ badge() }}</span>
      <span class="qt-text-xs qt-text-secondary">{{ plural(itemCount(), countNoun()) }}</span>
    </button>
  `,
})
export class ItemTierRow {
  readonly label = input.required<string>();
  readonly scope = input.required<'general' | 'project' | 'group' | 'character'>();
  readonly itemCount = input(0);
  /** `data-testid`, e.g. `wardrobe-tier-row`. */
  readonly testId = input.required<string>();
  readonly countNoun = input('item');
  readonly opened = output<void>();

  protected readonly plural = plural;
  protected badge(): string {
    const scope = this.scope();
    return scope === 'general' ? 'Shared' : scope === 'project' ? 'Project' : 'Group';
  }
}
