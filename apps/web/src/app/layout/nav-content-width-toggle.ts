import { ChangeDetectionStrategy, Component, inject } from '@angular/core';

import { Icon } from '../ui/icon';
import { ContentWidthService } from './content-width.service';

/**
 * The content-width toggle (v4 `components/dashboard/nav-content-width-toggle.tsx`)
 * — the page toolbar's right-most occupant. Renders nothing while the viewport
 * is too narrow for wide mode to apply (v4's `!canApplyWide` null return; the
 * provider-absent arm collapses in v5, where the root service always exists).
 */
@Component({
  selector: 'qt-nav-content-width-toggle',
  changeDetection: ChangeDetectionStrategy.OnPush,
  imports: [Icon],
  template: `
    @if (width.canApplyWide()) {
      <button
        type="button"
        (click)="width.toggleWidth()"
        class="qt-navbar-toggle"
        [class.qt-navbar-toggle-active]="width.isWide()"
        [title]="width.isWide() ? 'Switch to narrow layout' : 'Switch to wide layout'"
        [attr.aria-label]="width.isWide() ? 'Switch to narrow layout' : 'Switch to wide layout'"
        [attr.aria-pressed]="width.isWide()"
      >
        <qt-icon [name]="width.isWide() ? 'compress' : 'expand'" class="w-5 h-5" />
      </button>
    }
  `,
})
export class NavContentWidthToggle {
  protected readonly width = inject(ContentWidthService);
}
