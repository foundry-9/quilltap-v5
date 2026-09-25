import { ChangeDetectionStrategy, Component, computed } from '@angular/core';

import { ErrorAlert } from '../../../ui/error-alert';
import type { ConciergeSettings } from '../chat/chat-settings.types';
import { ConciergeSettingsCard } from './concierge-settings.api';

/**
 * v4 `NEW_CHAT_STATE_OPTIONS`. Locked is NOT offered — a new chat never starts
 * Locked (v4 `ConciergeNewChatStateEnum` is moderated | unmoderated).
 */
export const NEW_CHAT_STATE_OPTIONS: readonly {
  value: ConciergeSettings['newChatsStartAs'];
  label: string;
  description: string;
}[] = [
  {
    value: 'moderated',
    label: 'Moderated',
    description:
      'New chats go to their own provider first; the Concierge steps in only when it refuses.',
  },
  {
    value: 'unmoderated',
    label: 'Unmoderated',
    description: 'New chats go straight to the uncensored desk from the first message.',
  },
];

/**
 * v4's refusal-count input handler: `parseInt(v, 10)`, a NaN ignored (no
 * save), otherwise clamped to 0–10. Returns `null` for "do not save".
 */
export function parseRefusalCount(raw: string): number | null {
  const parsed = Number.parseInt(raw, 10);
  if (Number.isNaN(parsed)) return null;
  return Math.min(10, Math.max(0, parsed));
}

/**
 * What the Concierge does after refusals, and the state a new chat starts in
 * when the New Chat form names none (v4 `components/settings/
 * concierge-settings/RefusalsCard.tsx`, `3b463d6b1`). This is the ONLY home of
 * the refusal-count input: #74's first version lived in the Dangerous Content
 * card #76 deleted, and was never ported.
 *
 * The count saves per keystroke (v4's `onChange`), clamped — each keystroke is
 * one whole-object PUT through the shared save chain.
 */
@Component({
  selector: 'qt-concierge-refusals-card',
  changeDetection: ChangeDetectionStrategy.OnPush,
  imports: [ErrorAlert],
  template: `
    <div class="space-y-6">
      @if (saveError(); as msg) {
        <qt-error-alert [message]="msg" />
      }

      <div class="space-y-2">
        <label for="concierge-auto-switch-after-refusals" class="block qt-text-label">
          Switch a chat to Unmoderated after this many refusals (0 = never)
        </label>
        <input
          id="concierge-auto-switch-after-refusals"
          type="number"
          min="0"
          max="10"
          step="1"
          class="qt-input w-24"
          [value]="concierge().autoSwitchAfterRefusals"
          [disabled]="saving()"
          (input)="onCountInput($any($event.target).value)"
        />
        <p class="qt-text-small">
          Counts only refusals a provider actually states on a Moderated chat. When the tally is
          reached the Concierge moves the whole chat to Unmoderated and says so; returning the chat
          to Moderated clears it. Locked chats are never switched.
        </p>
      </div>

      <div class="space-y-2">
        <label for="concierge-new-chats-start-as" class="block qt-text-label">
          New chats start as
        </label>
        <select
          id="concierge-new-chats-start-as"
          class="qt-select"
          [disabled]="saving()"
          (change)="update({ newChatsStartAs: $any($event.target).value })"
        >
          @for (option of newChatStateOptions; track option.value) {
            <option [value]="option.value" [selected]="concierge().newChatsStartAs === option.value">
              {{ option.label }}
            </option>
          }
        </select>
        @if (selectedState(); as state) {
          <p class="qt-text-small">{{ state.description }}</p>
        }
      </div>
    </div>
  `,
})
export class RefusalsCard extends ConciergeSettingsCard {
  protected readonly newChatStateOptions = NEW_CHAT_STATE_OPTIONS;

  protected readonly selectedState = computed(() =>
    NEW_CHAT_STATE_OPTIONS.find((o) => o.value === this.concierge().newChatsStartAs),
  );

  protected async onCountInput(raw: string): Promise<void> {
    const count = parseRefusalCount(raw);
    if (count === null) return;
    await this.update({ autoSwitchAfterRefusals: count });
  }
}
