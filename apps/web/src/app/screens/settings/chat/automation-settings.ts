import { ChangeDetectionStrategy, Component, computed } from '@angular/core';

import { ErrorAlert } from '../../../ui/error-alert';
import { ChatSettingsCard } from './chat-settings.api';
import { AUTOMATION_OPTIONS, DEFAULT_AUTO_DETECT_RNG } from './chat-settings.types';
import { SettingsCard } from './settings-card';

/**
 * The Automation card (v4 `components/settings/chat-settings/
 * AutomationSettings.tsx`): auto-detection of RNG calls (dice, coin flips, spin
 * the bottle) in your messages. Writes the `autoDetectRng` scalar; v4's default
 * when unset is **true**.
 *
 * v4 renders the toggle list from `AUTOMATION_OPTIONS` even though the table has
 * exactly one entry, and hard-codes `checked={false}` for any other key — the
 * table is the extension point. Ported as v4 wrote it: the loop stays, and the
 * `autoDetectRng` arm is the only one that reads or writes.
 */
@Component({
  selector: 'qt-automation-settings',
  changeDetection: ChangeDetectionStrategy.OnPush,
  imports: [ErrorAlert, SettingsCard],
  template: `
    @if (loading()) {
      <p class="qt-text-small qt-text-muted">Loading automation settings…</p>
    } @else {
      <qt-settings-card title="Automation" subtitle="Configure automatic behavior in chats">
        @if (saveError(); as msg) {
          <qt-error-alert [message]="msg" class="mb-3" />
        }

        <div class="space-y-3">
          @for (option of options; track option.key) {
            <label
              class="flex items-start gap-3 p-4 border qt-border-default rounded qt-hover-accent cursor-pointer transition-colors"
            >
              <input
                type="checkbox"
                class="mt-1 h-4 w-4 rounded border-input text-primary focus:ring-primary"
                [checked]="option.key === 'autoDetectRng' ? autoDetectRng() : false"
                [disabled]="saving()"
                (change)="onChange(option.key, $any($event.target).checked)"
              />
              <div class="flex-1">
                <div class="font-medium">{{ option.label }}</div>
                <div class="qt-text-small">{{ option.description }}</div>
              </div>
            </label>
          }
        </div>
      </qt-settings-card>
    }
  `,
})
export class AutomationSettings extends ChatSettingsCard {
  protected readonly options = AUTOMATION_OPTIONS;

  /** v4 `settings.autoDetectRng ?? DEFAULT_AUTO_DETECT_RNG`. */
  protected readonly autoDetectRng = computed(
    () => (this.settings()?.['autoDetectRng'] as boolean | undefined) ?? DEFAULT_AUTO_DETECT_RNG,
  );

  protected async onChange(key: string, value: boolean): Promise<void> {
    if (key !== 'autoDetectRng') return;
    await this.save({ autoDetectRng: value }, 'Failed to update auto-detect RNG setting');
  }
}
