import { ChangeDetectionStrategy, Component, ElementRef, computed, inject } from '@angular/core';

import type { ImpersonationVoiceMode } from '../../../core/core-contract';
import { ErrorAlert } from '../../../ui/error-alert';
import { ChatSettingsCard } from './chat-settings.api';

/** v4 `MODES` (`ImpersonationVoiceSettings.tsx:11-31`), in v4's order, byte for byte. */
const MODES: ReadonlyArray<{ value: ImpersonationVoiceMode; label: string; description: string }> =
  [
    {
      value: 'off',
      label: 'Never',
      description:
        'Your line goes straight to the room, exactly as typed. No dialog, no rehearsal.',
    },
    {
      value: 'ask',
      label: 'Ask each time',
      description:
        'The dialog opens with your draft and nothing more — no model is troubled. Send it as written, ' +
        'or ask the character to restate it in their own voice.',
    },
    {
      value: 'always',
      label: 'Always restate',
      description:
        'The dialog opens and the character begins restating your draft at once. You may still send ' +
        'your own words as written.',
    },
  ];

/**
 * The Composer card's In Their Own Words mode (v4
 * `components/settings/chat-settings/ImpersonationVoiceSettings.tsx`,
 * `686954937`; three radios since `07b8f0209`): what happens to a line typed
 * while wearing a character's seat — it posts as typed (**Never**), the review
 * dialog opens on the draft with no model call (**Ask each time**), or the
 * dialog opens and the character restates it at once (**Always restate**).
 *
 * Writes the `impersonationVoiceMode` scalar; v4's default when unset is
 * **`'off'`** — a row that defaulted otherwise would quietly open a dialog on
 * every impersonated line on a fresh instance. The retired `686954937` boolean
 * is never sent.
 *
 * Sits directly after `qt-composer-unicode-settings` inside the existing Composer
 * card (v4's order, `ChatTabContent.tsx:97-106`). Copy carries over verbatim.
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

        <fieldset>
          <legend class="qt-settings-section-heading">
            Impersonated lines in the character's own words
          </legend>
          <div class="qt-text-small mt-1">
            When you have taken a character's seat with the Impersonate button, your draft may be
            handed first to that character — their own model, their own voice — and returned for
            your inspection before a syllable reaches the room. Nothing posts until you say so.
            Speaking as yourself is untouched, as are sends that carry only attachments or tool
            results.
          </div>
          <div class="mt-3 space-y-2">
            @for (m of modes; track m.value) {
              <label class="qt-settings-toggle-row">
                <input
                  type="radio"
                  name="impersonationVoiceMode"
                  class="qt-radio mt-1"
                  [value]="m.value"
                  [checked]="mode() === m.value"
                  [disabled]="saving()"
                  (change)="onChange(m.value)"
                />
                <div class="flex-1">
                  <div class="font-medium">{{ m.label }}</div>
                  <div class="qt-text-small mt-1">{{ m.description }}</div>
                </div>
              </label>
            }
          </div>
        </fieldset>
      </div>
    }
  `,
})
export class ImpersonationVoiceSettings extends ChatSettingsCard {
  private readonly host = inject<ElementRef<HTMLElement>>(ElementRef);
  protected readonly modes = MODES;

  /** v4 `settings.impersonationVoiceMode ?? 'off'` (`:38`). */
  protected readonly mode = computed<ImpersonationVoiceMode>(
    () => this.settings()?.impersonationVoiceMode ?? 'off',
  );

  /** v4 `handleImpersonationVoiceModeChange` (`useChatSettings.ts:431-442`). */
  protected async onChange(value: ImpersonationVoiceMode): Promise<void> {
    await this.save(
      { impersonationVoiceMode: value },
      'Failed to update impersonated-line voice setting',
    );
    // The radios are CONTROLLED by the stored mode, as v4's React `checked` is:
    // a refused save leaves the setting where it was, and Angular's binding only
    // writes `checked` when the mode CHANGES — so without this the clicked radio
    // would stay lit over a setting that never moved.
    const mode = this.mode();
    this.host.nativeElement
      .querySelectorAll<HTMLInputElement>('input[name="impersonationVoiceMode"]')
      .forEach((el) => (el.checked = el.value === mode));
  }
}
