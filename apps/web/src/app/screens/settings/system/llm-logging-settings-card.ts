import { ChangeDetectionStrategy, Component, computed } from '@angular/core';

import { ErrorAlert } from '../../../ui/error-alert';
import { ChatSettingsCard } from '../chat/chat-settings.api';
import { SettingsCard } from '../chat/settings-card';

/** v4 `LLMLoggingSettings` (`types.ts:437-441`); default `{true, false, 30}`. */
interface LlmLoggingBag {
  enabled: boolean;
  verboseMode: boolean;
  retentionDays: number;
}

const DEFAULT_LLM_LOGGING: LlmLoggingBag = { enabled: true, verboseMode: false, retentionDays: 30 };

/**
 * The LLM Logging card (v4 `components/settings/chat-settings/LLMLoggingSettings.tsx`):
 * store LLM request/response data for debugging. Three controls on
 * `chatSettings.llmLoggingSettings` — an enable toggle, a verbose toggle, and a
 * retention-days number (v4 `:34/:52/:79`). The bag already round-trips through
 * `chatSettingsUpdate` (`db/chat_settings.rs:242`).
 *
 * v4's `handleLLMLoggingChange(key, value)` (`useChatSettings.ts:426-449`) merges
 * the ONE changed key onto the whole current bag and PUTs it; v5's
 * {@link ChatSettingsCard.save} does the same (whole-bag replace).
 */
@Component({
  selector: 'qt-llm-logging-settings-card',
  changeDetection: ChangeDetectionStrategy.OnPush,
  imports: [ErrorAlert, SettingsCard],
  template: `
    @if (loading()) {
      <div class="flex items-center justify-center py-8">
        <div class="qt-text-secondary">Loading settings...</div>
      </div>
    } @else {
      <qt-settings-card
        title="LLM Request Logging"
        subtitle="Log LLM API requests and responses for debugging and monitoring. Logs can be viewed per message in chats or on the Tools page."
      >
        @if (saveError(); as msg) {
          <qt-error-alert [message]="msg" class="mb-3" />
        }

        <div class="space-y-3">
          <label
            class="flex items-start gap-3 p-4 border qt-border-default rounded qt-hover-accent cursor-pointer transition-colors"
          >
            <input
              type="checkbox"
              class="mt-1 h-4 w-4 rounded border-input text-primary focus:ring-primary"
              [checked]="bag().enabled"
              [disabled]="saving()"
              (change)="onChange('enabled', $any($event.target).checked)"
            />
            <div class="flex-1">
              <div class="font-medium">Enable Logging</div>
              <div class="qt-text-small">
                Store LLM request/response data for each message. Useful for debugging and
                monitoring API usage.
              </div>
            </div>
          </label>

          <label
            class="flex items-start gap-3 p-4 border qt-border-default rounded qt-hover-accent cursor-pointer transition-colors"
            [class.opacity-50]="!bag().enabled"
          >
            <input
              type="checkbox"
              class="mt-1 h-4 w-4 rounded border-input text-primary focus:ring-primary"
              [checked]="bag().verboseMode"
              [disabled]="saving() || !bag().enabled"
              (change)="onChange('verboseMode', $any($event.target).checked)"
            />
            <div class="flex-1">
              <div class="font-medium">Verbose Mode</div>
              <div class="qt-text-small">
                Store full message content in logs (requires more storage). When disabled, only
                summaries are stored.
              </div>
            </div>
          </label>

          <div class="p-4 border qt-border-default rounded" [class.opacity-50]="!bag().enabled">
            <div class="flex items-center justify-between">
              <div class="flex-1">
                <div class="font-medium">Log Retention</div>
                <div class="qt-text-small">
                  Automatically delete logs older than this many days. Set to 0 for unlimited
                  retention.
                </div>
              </div>
              <div class="flex items-center gap-2 ml-4">
                <input
                  type="number"
                  min="0"
                  max="365"
                  class="w-20 px-2 py-1 border qt-border-default rounded text-center"
                  [value]="bag().retentionDays"
                  [disabled]="saving() || !bag().enabled"
                  (change)="onRetentionChange($event)"
                />
                <span class="qt-text-secondary">days</span>
              </div>
            </div>
          </div>
        </div>

        <div class="mt-4 p-4 border qt-border-default rounded qt-bg-muted/50">
          <p class="qt-text-small qt-text-secondary">
            <strong>Privacy Note:</strong> LLM logs contain your conversations and API responses.
            They are stored locally in your database and included in backups. Logs are automatically
            cleaned up based on your retention settings.
          </p>
        </div>
      </qt-settings-card>
    }
  `,
})
export class LlmLoggingSettingsCard extends ChatSettingsCard {
  /** v4 `settings.llmLoggingSettings || DEFAULT_LLM_LOGGING_SETTINGS` (`:22`). */
  protected readonly bag = computed<LlmLoggingBag>(() => {
    const stored = this.settings()?.['llmLoggingSettings'] as Partial<LlmLoggingBag> | undefined;
    return { ...DEFAULT_LLM_LOGGING, ...(stored ?? {}) };
  });

  /** v4 `retentionDays` — `parseInt(value, 10) || 0` (empty/NaN → 0), clamped by the input. */
  protected onRetentionChange(event: Event): void {
    const value = parseInt((event.target as HTMLInputElement).value, 10) || 0;
    void this.onChange('retentionDays', value);
  }

  /** v4 `handleLLMLoggingChange(key, value)` — merge the one key onto the whole bag. */
  protected async onChange(key: keyof LlmLoggingBag, value: boolean | number): Promise<void> {
    const merged: LlmLoggingBag = { ...this.bag(), [key]: value };
    await this.save({ llmLoggingSettings: merged }, 'Failed to update LLM logging settings');
  }
}
