import { ChangeDetectionStrategy, Component, input } from '@angular/core';

import { Icon } from '../ui/icon';

/**
 * The "this is hidden" stand-in (v4
 * `components/quick-hide/hidden-placeholder.tsx`) — shown in place of a detail
 * screen whose subject carries a hidden tag, so a quick-hidden character can't
 * be read by deep-linking straight to it.
 *
 * Copy is v4-verbatim: the heading "Hidden", with an optional label beneath.
 */
@Component({
  selector: 'qt-hidden-placeholder',
  changeDetection: ChangeDetectionStrategy.OnPush,
  imports: [Icon],
  template: `
    <div class="flex items-center justify-center min-h-[200px]">
      <div
        class="flex flex-col items-center gap-3 rounded-lg border-2 border-dashed qt-border-default qt-bg-muted/30 px-6 py-8 text-center"
      >
        <qt-icon name="eye-off" class="h-12 w-12 qt-text-secondary" />
        <div>
          <p class="qt-heading-4 text-foreground">Hidden</p>
          @if (label()) {
            <p class="text-sm qt-text-secondary">{{ label() }}</p>
          }
        </div>
      </div>
    </div>
  `,
})
export class HiddenPlaceholder {
  readonly label = input<string | undefined>(undefined);
}
