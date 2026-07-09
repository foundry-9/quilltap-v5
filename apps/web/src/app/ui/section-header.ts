import { ChangeDetectionStrategy, Component, computed, input } from '@angular/core';

/** v4 `components/ui/SectionHeader.tsx` — a titled section header with an optional count. */
@Component({
  selector: 'qt-section-header',
  changeDetection: ChangeDetectionStrategy.OnPush,
  template: `
    <div class="flex items-center justify-between gap-4 mb-4">
      @if (level() === 'h2') {
        <h2 class="qt-text-section text-foreground flex-1">{{ label() }}</h2>
      } @else {
        <h3 class="qt-text-section text-foreground flex-1">{{ label() }}</h3>
      }
      <ng-content></ng-content>
    </div>
  `,
})
export class SectionHeader {
  readonly title = input.required<string>();
  readonly count = input<number | undefined>(undefined);
  readonly level = input<'h2' | 'h3'>('h3');
  protected readonly label = computed(() =>
    this.count() !== undefined ? `${this.title()} (${this.count()})` : this.title(),
  );
}
