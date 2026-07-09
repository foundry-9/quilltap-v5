import { ChangeDetectionStrategy, Component, computed, input } from '@angular/core';

import { Icon } from './icon';

/** v4 `components/ui/ChevronIcon.tsx` — a `chevron-down` that rotates when open. */
@Component({
  selector: 'qt-chevron-icon',
  changeDetection: ChangeDetectionStrategy.OnPush,
  imports: [Icon],
  template: `<qt-icon name="chevron-down" [class]="cssClass()" />`,
})
export class ChevronIcon {
  readonly klass = input<string>('', { alias: 'class' });
  readonly expanded = input(false);
  protected readonly cssClass = computed(
    () => `${this.klass()} transition-transform duration-200 ${this.expanded() ? 'rotate-180' : ''}`.trim(),
  );
}
