import { ChangeDetectionStrategy, Component, computed, input } from '@angular/core';

/** v4 `components/ui/brand-name.tsx` — the wordmark in the brand font. */
@Component({
  selector: 'qt-brand-name',
  changeDetection: ChangeDetectionStrategy.OnPush,
  template: `<span [class]="cssClass()">Quilltap</span>`,
})
export class BrandName {
  readonly klass = input<string>('', { alias: 'class' });
  protected readonly cssClass = computed(() => `qt-font-brand ${this.klass()}`.trim());
}
