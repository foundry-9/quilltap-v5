import { ChangeDetectionStrategy, Component, computed } from '@angular/core';

import { ErrorAlert } from '../../../ui/error-alert';
import { ChatSettingsCard } from './chat-settings.api';

/**
 * The Slash Commands card's toggle (v4
 * `components/settings/chat-settings/ComposerSlashCommandsSettings.tsx`,
 * `4877fc2da`): whether `/` opens the command menu in the Salon composer.
 * Writes the `composerSlashCommands` scalar; v4's default when unset is
 * **true**. Copy carries over verbatim.
 *
 * The toggle gates the menu ONLY — the gutter buttons and the sidebar are
 * never affected. v4 gives it its OWN card between Composer and Auto-Scroll
 * (`ChatTabContent.tsx:112-118`), not a row inside the Composer card.
 *
 * v4's `useChatSettings.handleComposerSlashCommandsChange` + the tab's prop
 * plumbing have no file here: a v5 card is self-contained, so the partial body
 * and the failure text live in `onChange` below. The key is read through
 * `ChatSettingsDto`'s index signature (its declaration is a sibling lane's).
 */
@Component({
  selector: 'qt-composer-slash-commands-settings',
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
            <div class="qt-settings-section-heading">Slash commands</div>
            <div class="qt-text-small mt-1">
              Type <code>/</code> at the start of a word to run a tool or open a composer action. The
              gutter buttons and the sidebar are never affected.
            </div>
          </div>
        </label>
      </div>
    }
  `,
})
export class ComposerSlashCommandsSettings extends ChatSettingsCard {
  /** v4 `settings.composerSlashCommands ?? true`. */
  protected readonly enabled = computed(
    () => (this.settings()?.['composerSlashCommands'] as boolean | undefined) ?? true,
  );

  protected async onChange(value: boolean): Promise<void> {
    await this.save(
      { composerSlashCommands: value },
      'Failed to update composer slash-commands setting',
    );
  }
}
