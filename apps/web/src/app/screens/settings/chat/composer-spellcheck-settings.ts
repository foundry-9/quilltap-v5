import { ChangeDetectionStrategy, Component, computed } from '@angular/core';

import { ErrorAlert } from '../../../ui/error-alert';
import { ChatSettingsCard } from './chat-settings.api';

/**
 * The Composer card (v4 `components/settings/chat-settings/
 * ComposerSpellcheckSettings.tsx`): the browser-spellcheck switch for the Salon
 * composer and the rich-text Document Mode editor. Writes the
 * `composerSpellcheck` scalar; v4's default when unset is **true**.
 *
 * The consumer is the `qt-rich-editor` contenteditable's `spellcheck` attribute
 * (v4 binds the same flag at `LexicalComposerWrapper.tsx:107`). Copy carries over
 * verbatim.
 */
@Component({
  selector: 'qt-composer-spellcheck-settings',
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
            <div class="qt-settings-section-heading">Spellcheck in the composer</div>
            <div class="qt-text-small mt-1">
              Underlines misspelled words in the Salon composer and the Document Mode editor. In the
              Quilltap desktop app, right-click a flagged word to see suggestions and add it to your
              dictionary. Source-mode editors (raw Markdown, plain text) stay unsquiggled
              regardless.
            </div>
          </div>
        </label>
      </div>
    }
  `,
})
export class ComposerSpellcheckSettings extends ChatSettingsCard {
  /** v4 `settings.composerSpellcheck ?? true`. */
  protected readonly enabled = computed(
    () => (this.settings()?.['composerSpellcheck'] as boolean | undefined) ?? true,
  );

  protected async onChange(value: boolean): Promise<void> {
    await this.save({ composerSpellcheck: value }, 'Failed to update composer spellcheck setting');
  }
}
