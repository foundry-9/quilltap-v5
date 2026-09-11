import { ChangeDetectionStrategy, Component, computed } from '@angular/core';

import { ErrorAlert } from '../../../ui/error-alert';
import { ChatSettingsCard } from './chat-settings.api';

/**
 * The Composer card's In Their Own Words toggle (v4
 * `components/settings/chat-settings/ImpersonationVoiceSettings.tsx`,
 * `686954937`): whether a line typed while wearing a character's seat is first
 * restated by that character's own model, for review, before it posts.
 *
 * Writes the `impersonationVoiceRewrite` scalar; v4's default when unset is
 * **false** — the feature spends a model call on every impersonated line, so it
 * arrives off and stays off until asked for.
 *
 * Sits directly after `qt-composer-unicode-settings` inside the existing Composer
 * card (v4's order, `ChatTabContent.tsx:107-111`). Copy carries over verbatim.
 *
 * @module screens/settings/chat/impersonation-voice-settings
 */
@Component({
  selector: 'qt-impersonation-voice-settings',
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
            <div class="qt-settings-section-heading">
              Impersonated lines in the character's own words
            </div>
            <div class="qt-text-small mt-1">
              When you have taken a character's seat with the Impersonate button, your draft is
              handed first to that character — their own model, their own voice — and returned for
              your inspection before a syllable reaches the room. Send the restatement, have it
              attempted afresh, retire to the composer and rewrite, or send your own words exactly
              as typed. Nothing posts until you say so. Speaking as yourself is untouched, as are
              sends that carry only attachments or tool results.
            </div>
          </div>
        </label>
      </div>
    }
  `,
})
export class ImpersonationVoiceSettings extends ChatSettingsCard {
  /** v4 `settings.impersonationVoiceRewrite ?? false`. */
  protected readonly enabled = computed(
    () => (this.settings()?.['impersonationVoiceRewrite'] as boolean | undefined) ?? false,
  );

  protected async onChange(value: boolean): Promise<void> {
    await this.save(
      { impersonationVoiceRewrite: value },
      'Failed to update impersonated-line voice setting',
    );
  }
}
