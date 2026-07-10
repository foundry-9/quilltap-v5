import { ChangeDetectionStrategy, Component, input } from '@angular/core';

import { EmptyState } from '../../ui/empty-state';

/**
 * The "not yet installed" placeholder for the Settings tabs whose verticals land
 * in a later installment (Chat, Commonplace Book, Images, Templates & Prompts,
 * Data & System). Steampunk-voiced per the standing register (D11).
 */
@Component({
  selector: 'qt-settings-placeholder',
  changeDetection: ChangeDetectionStrategy.OnPush,
  imports: [EmptyState],
  template: `
    <qt-empty-state
      variant="dashed"
      [title]="title() + ' — not yet fitted out'"
      [description]="
        'The ' +
        title() +
        ' quarter of the Settings hall stands ready, but its fittings have not yet arrived by post. This section opens in a later installment.'
      "
    />
  `,
})
export class SettingsPlaceholder {
  readonly title = input.required<string>();
}
