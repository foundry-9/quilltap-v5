import { ChangeDetectionStrategy, Component, computed } from '@angular/core';

import { ErrorAlert } from '../../../ui/error-alert';
import { ChatSettingsCard } from './chat-settings.api';

/**
 * The Composer card's symbol toggle (v4
 * `components/settings/chat-settings/ComposerUnicodeSettings.tsx`): the `\`
 * typeahead switch. Writes the `composerUnicode` scalar; v4's default when unset
 * is **true**.
 *
 * The toggle gates the automatic trigger ONLY — the toolbar's Ω button is never
 * gated. Copy carries over verbatim, math bail and all.
 *
 * Sits directly after `qt-composer-emoji-settings` inside the existing Composer
 * card (v4's order).
 */
@Component({
  selector: 'qt-composer-unicode-settings',
  changeDetection: ChangeDetectionStrategy.OnPush,
  imports: [ErrorAlert],
  template: `
    @if (loading()) {
      <p class="qt-text-small qt-text-muted">Loading composer settings…</p>
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
            <div class="qt-settings-section-heading">Symbol shortcuts</div>
            <div class="qt-text-small mt-1">
              Type <code>\\</code> and a LaTeX name (<code>\\to</code>, <code>\\phi</code>) or a code
              point (<code>\\u2192</code>) to insert a symbol. Nothing fires inside a formula, so
              <code>$$\\phi$$</code> stays as you typed it. The toolbar's <code>Ω</code> button
              works either way.
            </div>
          </div>
        </label>
      </div>
    }
  `,
})
export class ComposerUnicodeSettings extends ChatSettingsCard {
  /** v4 `settings.composerUnicode ?? true`. */
  protected readonly enabled = computed(
    () => (this.settings()?.['composerUnicode'] as boolean | undefined) ?? true,
  );

  protected async onChange(value: boolean): Promise<void> {
    await this.save({ composerUnicode: value }, 'Failed to update symbol shortcut setting');
  }
}
