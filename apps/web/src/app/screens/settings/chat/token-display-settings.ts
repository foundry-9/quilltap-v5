import { ChangeDetectionStrategy, Component, computed } from '@angular/core';

import { ErrorAlert } from '../../../ui/error-alert';
import { ChatSettingsCard } from './chat-settings.api';
import {
  DEFAULT_TOKEN_DISPLAY_SETTINGS,
  TOKEN_DISPLAY_OPTIONS,
  type TokenDisplaySettings as TokenDisplayBag,
} from './chat-settings.types';
import { SettingsCard } from './settings-card';

/**
 * The Token Display card (v4 `components/settings/chat-settings/
 * TokenDisplaySettings.tsx`): the four visibility toggles for token counts and
 * cost estimates in chats. Each toggle PUTs the WHOLE `tokenDisplaySettings` bag
 * with the one key replaced (v4 `{ ...currentSettings, [key]: value }`).
 *
 * NOTE — the setting is stored and honored, but the v5 Salon does not yet RENDER
 * token counts or costs: that rendering is a Salon slice (v4 consumes
 * `tokenDisplaySettings` in `MessageRow.tsx` / `MessageActionBar.tsx`) and is a
 * NAMED DEFERRAL of the P4.6an order, not part of this card. The card is
 * v4-faithful regardless — v4's own note copy carries over unchanged.
 */
@Component({
  selector: 'qt-token-display-settings',
  changeDetection: ChangeDetectionStrategy.OnPush,
  imports: [ErrorAlert, SettingsCard],
  template: `
    @if (loading()) {
      <p class="qt-text-small qt-text-muted">Loading token-display settings…</p>
    } @else {
      <qt-settings-card
        title="Token & Cost Display"
        subtitle="Control the visibility of token usage and cost information in chats. Enabling these options helps you track your API usage and costs."
      >
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
                [checked]="current()[option.key]"
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

        <div class="mt-4 p-4 border qt-border-default rounded qt-bg-muted/50">
          <p class="qt-text-small qt-text-secondary">
            <strong>Note:</strong> Cost estimates are based on pricing data from OpenRouter when
            available, or fallback pricing for other providers. Actual costs may vary slightly.
          </p>
        </div>
      </qt-settings-card>
    }
  `,
})
export class TokenDisplaySettings extends ChatSettingsCard {
  protected readonly options = TOKEN_DISPLAY_OPTIONS;

  /** v4 `settings.tokenDisplaySettings || DEFAULT_TOKEN_DISPLAY_SETTINGS`. */
  protected readonly current = computed<TokenDisplayBag>(
    () =>
      (this.settings()?.['tokenDisplaySettings'] as TokenDisplayBag | undefined) ??
      DEFAULT_TOKEN_DISPLAY_SETTINGS,
  );

  /** v4 `{ tokenDisplaySettings: { ...currentSettings, [key]: value } }`. */
  protected async onChange(key: keyof TokenDisplayBag, value: boolean): Promise<void> {
    await this.save(
      { tokenDisplaySettings: { ...this.current(), [key]: value } },
      'Failed to update token display settings',
    );
  }
}
