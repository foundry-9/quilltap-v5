import { ChangeDetectionStrategy, Component, computed } from '@angular/core';

import { ErrorAlert } from '../../../ui/error-alert';
import { ChatSettingsCard } from './chat-settings.api';

/**
 * The Composer card's emoji toggle (v4
 * `components/settings/chat-settings/ComposerEmojiSettings.tsx`): the `:`
 * typeahead switch. Writes the `composerEmoji` scalar; v4's default when unset
 * is **true**.
 *
 * The toggle gates the automatic trigger ONLY — the toolbar's emoji button is
 * never gated, which is what the copy says out loud. Copy carries over verbatim.
 *
 * Sits directly after `qt-composer-spellcheck-settings` inside the existing
 * Composer card (v4's order).
 */
@Component({
  selector: 'qt-composer-emoji-settings',
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
            <div class="qt-settings-section-heading">Emoji shortcuts</div>
            <div class="qt-text-small mt-1">
              Type <code>:</code> and at least two letters to search emoji by name. The toolbar's
              emoji button works either way.
            </div>
          </div>
        </label>
      </div>
    }
  `,
})
export class ComposerEmojiSettings extends ChatSettingsCard {
  /** v4 `settings.composerEmoji ?? true`. */
  protected readonly enabled = computed(
    () => (this.settings()?.['composerEmoji'] as boolean | undefined) ?? true,
  );

  protected async onChange(value: boolean): Promise<void> {
    await this.save({ composerEmoji: value }, 'Failed to update emoji shortcut setting');
  }
}
