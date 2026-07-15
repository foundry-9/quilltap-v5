import { ChangeDetectionStrategy, Component, computed } from '@angular/core';

import { ErrorAlert } from '../../../ui/error-alert';
import { ChatSettingsCard } from './chat-settings.api';
import {
  DEFAULT_THINKING_DISPLAY_SETTINGS,
  type ThinkingDisplaySettings as ThinkingDisplayBag,
} from './chat-settings.types';
import { SettingsCard } from './settings-card';

/**
 * The Thinking / Reasoning card (v4 `components/settings/chat-settings/
 * ThinkingDisplaySettings.tsx`): the global defaults for showing reasoning
 * models' chain-of-thought in the Salon.
 *
 * DISPLAY ONLY — these only govern whether captured reasoning is SHOWN.
 * Reasoning is always captured and stored regardless, and is never fed back to
 * any model. Per-chat overrides live in the Salon's Visibility sidebar.
 *
 * `defaultCollapsed` is gated on `defaultVisible` (there is nothing to collapse
 * when nothing is shown): v4 disables the second toggle and dims its row, both
 * ported. `thinkingDisplay.defaultCollapsed` is already consumed in v5 at
 * `chat/message-row.ts:329`.
 */
@Component({
  selector: 'qt-thinking-display-settings',
  changeDetection: ChangeDetectionStrategy.OnPush,
  imports: [ErrorAlert, SettingsCard],
  template: `
    @if (loading()) {
      <p class="qt-text-small qt-text-muted">Loading thinking-display settings…</p>
    } @else {
      <qt-settings-card
        title="Thinking / Reasoning"
        subtitle="Whether new chats show reasoning models' chain-of-thought"
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
              class="qt-checkbox mt-1"
              [checked]="defaultVisible()"
              [disabled]="saving()"
              (change)="onUpdate({ defaultVisible: $any($event.target).checked })"
            />
            <div class="flex-1">
              <div class="font-medium">Show thinking by default</div>
              <div class="qt-text-small">
                New chats reveal a thinking model's reasoning in the bubble. Reasoning is always
                captured and stored either way — this only governs whether it is shown, and it is
                never sent back to any model.
              </div>
            </div>
          </label>

          <!-- v4 dims + un-clicks this row while thinking is hidden. -->
          <label
            class="flex items-start gap-3 p-4 border qt-border-default rounded transition-colors"
            [class.qt-hover-accent]="defaultVisible()"
            [class.cursor-pointer]="defaultVisible()"
            [class.opacity-50]="!defaultVisible()"
            [class.cursor-not-allowed]="!defaultVisible()"
          >
            <input
              type="checkbox"
              class="qt-checkbox mt-1"
              [checked]="defaultCollapsed()"
              [disabled]="saving() || !defaultVisible()"
              (change)="onUpdate({ defaultCollapsed: $any($event.target).checked })"
            />
            <div class="flex-1">
              <div class="font-medium">Start collapsed</div>
              <div class="qt-text-small">
                When thinking is shown, the block begins folded away — one click expands it. Turn
                this off to show the full reasoning unfurled.
              </div>
            </div>
          </label>
        </div>
      </qt-settings-card>
    }
  `,
})
export class ThinkingDisplaySettings extends ChatSettingsCard {
  private readonly bag = computed(
    () => (this.settings()?.['thinkingDisplay'] ?? {}) as ThinkingDisplayBag,
  );

  /** v4 `settings.thinkingDisplay?.defaultVisible ?? DEFAULT.defaultVisible ?? true`. */
  protected readonly defaultVisible = computed(
    () => this.bag().defaultVisible ?? DEFAULT_THINKING_DISPLAY_SETTINGS.defaultVisible ?? true,
  );
  /** v4 `settings.thinkingDisplay?.defaultCollapsed ?? DEFAULT.defaultCollapsed ?? true`. */
  protected readonly defaultCollapsed = computed(
    () => this.bag().defaultCollapsed ?? DEFAULT_THINKING_DISPLAY_SETTINGS.defaultCollapsed ?? true,
  );

  /** v4 `{ ...DEFAULT_THINKING_DISPLAY_SETTINGS, ...(settings.thinkingDisplay ?? {}), ...updates }`. */
  protected async onUpdate(updates: Partial<ThinkingDisplayBag>): Promise<void> {
    const merged: ThinkingDisplayBag = {
      ...DEFAULT_THINKING_DISPLAY_SETTINGS,
      ...this.bag(),
      ...updates,
    };
    await this.save({ thinkingDisplay: merged }, 'Failed to update thinking-display settings');
  }
}
