import { ChangeDetectionStrategy, Component, computed } from '@angular/core';

import { ErrorAlert } from '../../../ui/error-alert';
import { ChatSettingsCard } from './chat-settings.api';
import {
  AUTOMATION_OPTIONS,
  DEFAULT_AUTO_DETECT_RNG,
  DEFAULT_CUSTOM_TOOL_TRIGGERS,
  DEFAULT_CUSTOM_TOOLS,
  type AutomationKey,
} from './chat-settings.types';
import { SettingsCard } from './settings-card';

/** One row's state: its value, and whether it is dead (v4 `AutomationRow`). */
interface AutomationRow {
  value: boolean;
  disabled: boolean;
}

/**
 * The Automation card (v4 `components/settings/chat-settings/
 * AutomationSettings.tsx`, rewritten as a row table at `362426c8f`, pinned at
 * `25b8fab6a`): one row per `AUTOMATION_OPTIONS` entry.
 *
 * - **Auto-Detect RNG Calls** — the `autoDetectRng` scalar; default **true**.
 * - **Auto-Run Custom Tools on Trigger Phrases** — the `customToolTriggers`
 *   scalar; default **false** (overhearing is a surprise the first time), and
 *   DEAD while Custom Tools is off (`customTools ?? true`) — a client rule; the
 *   server stores the value regardless.
 *
 * v4 threads each handler down from `useChatSettings` through
 * `ChatTabContent`; v5's cards are self-contained, so each row's write is one
 * `save(partial, failureText)` here — the partial body and v4's failure text
 * are the behaviour that ports (the plumbing files are NO-PORT).
 *
 * A dead row swaps `qt-hover-accent cursor-pointer` for `opacity-50
 * cursor-not-allowed` (v4 `:61-63`). Bound token by token, since Angular's
 * `[class]` re-sorts a string.
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
            @let row = rows()[option.key];
            <label
              class="flex items-start gap-3 p-4 border qt-border-default rounded transition-colors"
              [class.opacity-50]="row.disabled"
              [class.cursor-not-allowed]="row.disabled"
              [class.qt-hover-accent]="!row.disabled"
              [class.cursor-pointer]="!row.disabled"
            >
              <input
                type="checkbox"
                class="mt-1 h-4 w-4 rounded border-input text-primary focus:ring-primary"
                [checked]="row.value"
                [disabled]="saving() || row.disabled"
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

  /** v4 `rows` (`:40-50`), keyed by `AutomationKey`. */
  protected readonly rows = computed<Record<AutomationKey, AutomationRow>>(() => {
    const settings = this.settings();
    const customToolsOn =
      (settings?.['customTools'] as boolean | undefined) ?? DEFAULT_CUSTOM_TOOLS;
    return {
      autoDetectRng: {
        value: (settings?.['autoDetectRng'] as boolean | undefined) ?? DEFAULT_AUTO_DETECT_RNG,
        disabled: false,
      },
      customToolTriggers: {
        value: settings?.customToolTriggers ?? DEFAULT_CUSTOM_TOOL_TRIGGERS,
        disabled: !customToolsOn,
      },
    };
  });

  protected async onChange(key: AutomationKey, value: boolean): Promise<void> {
    switch (key) {
      case 'autoDetectRng':
        await this.save({ autoDetectRng: value }, 'Failed to update auto-detect RNG setting');
        return;
      case 'customToolTriggers':
        await this.save({ customToolTriggers: value }, 'Failed to update trigger phrase setting');
        return;
    }
  }
}
