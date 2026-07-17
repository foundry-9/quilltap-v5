import { ChangeDetectionStrategy, Component, computed } from '@angular/core';

import { ErrorAlert } from '../../../ui/error-alert';
import { ChatSettingsCard } from './chat-settings.api';

/**
 * The Custom Tools card (v4 `components/settings/chat-settings/
 * CustomToolsSettings.tsx`): whether Pascal offers your custom pseudo-tools to
 * models (and posts the composer button). Writes the `customTools` scalar; v4's
 * default when unset is **true** (`DEFAULT_CUSTOM_TOOLS`).
 *
 * v4's "Open Pascal's Workbench" button is OMITTED — the Workbench is P4.6bb
 * (next round). The toggle + its copy carry over verbatim.
 */
@Component({
  selector: 'qt-custom-tools-settings',
  changeDetection: ChangeDetectionStrategy.OnPush,
  imports: [ErrorAlert],
  template: `
    @if (loading()) {
      <p class="qt-text-small qt-text-muted">Loading custom-tool settings…</p>
    } @else {
      <div>
        @if (saveError(); as msg) {
          <qt-error-alert [message]="msg" class="mb-3" />
        }

        <label class="qt-settings-toggle-row">
          <input
            type="checkbox"
            class="qt-checkbox mt-1"
            [checked]="enabled()"
            [disabled]="saving()"
            (change)="onChange($any($event.target).checked)"
          />
          <div class="flex-1">
            <div class="qt-settings-section-heading">Custom tools</div>
            <div class="qt-text-small mt-1">
              Permits Pascal to lay your own contrivances upon the baize, where any model at the
              table may reach for one of its own accord, and posts the little button in the
              composer&rsquo;s gutter for when you&rsquo;d rather call the play yourself. Unchecked,
              the croupier sweeps the lot out of sight: no model is offered them, the gutter button
              retires, and your contraptions wait &mdash; undisturbed and entirely intact &mdash;
              until you see fit to invite them back.
            </div>
          </div>
        </label>
      </div>
    }
  `,
})
export class CustomToolsSettings extends ChatSettingsCard {
  /** v4 `settings.customTools ?? DEFAULT_CUSTOM_TOOLS` (true). */
  protected readonly enabled = computed(
    () => (this.settings()?.['customTools'] as boolean | undefined) ?? true,
  );

  protected async onChange(value: boolean): Promise<void> {
    await this.save({ customTools: value }, 'Failed to update custom tools setting');
  }
}
