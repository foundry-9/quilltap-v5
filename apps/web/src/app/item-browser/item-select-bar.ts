import { ChangeDetectionStrategy, Component, computed, input, output } from '@angular/core';

/**
 * The select-mode bar of a folder-of-items browser: how many are selected,
 * the filing buttons the browser supplies (projected), and Cancel. A port of
 * v4 `components/item-browser/ItemSelectBar.tsx` (`4fbdad186`).
 */
@Component({
  selector: 'qt-item-select-bar',
  changeDetection: ChangeDetectionStrategy.OnPush,
  template: `
    <div [attr.class]="barClass()" [attr.data-testid]="testId()">
      <span class="qt-text-xs qt-text-secondary">{{ count() }} selected</span>
      <div class="flex items-center gap-2">
        <ng-content />
        <button type="button" class="qt-button-ghost qt-button-sm" (click)="cancelled.emit()">Cancel</button>
      </div>
    </div>
  `,
})
export class ItemSelectBar {
  readonly count = input(0);
  /** Stick to the bottom of a scrolling list. */
  readonly sticky = input(false);
  /** `data-testid`, e.g. `wardrobe-select-bar`. */
  readonly testId = input.required<string>();
  readonly cancelled = output<void>();

  /** v4 `:22` — the class string, sticky prefix included. */
  protected readonly barClass = computed(
    () =>
      `${this.sticky() ? 'sticky bottom-0' : ''} mt-2 flex items-center justify-between gap-2 rounded border qt-border-default qt-bg-default px-3 py-2 shadow-sm`,
  );
}
