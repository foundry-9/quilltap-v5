import { ChangeDetectionStrategy, Component, computed } from '@angular/core';

import { ErrorAlert } from '../../../ui/error-alert';
import { ChatSettingsCard } from './chat-settings.api';
import {
  DEFAULT_MEMORY_CASCADE_PREFERENCES,
  MEMORY_CASCADE_ACTIONS,
  type MemoryCascadeAction,
  type MemoryCascadePreferences,
} from './chat-settings.types';
import { SettingsCard } from './settings-card';

/**
 * The Memory Cascade card (v4 `components/settings/chat-settings/
 * MemoryCascadeSettings.tsx`): what becomes of auto-extracted memories when a
 * message is deleted or a response regenerated. Each select PUTs the WHOLE
 * `memoryCascadePreferences` bag with the one key replaced.
 *
 * v4's asymmetry, ported as-is: the delete select offers all four actions, but
 * the swipe/regenerate select FILTERS OUT `ASK_EVERY_TIME` — there is no
 * confirmation dialog on the regenerate path to ask through.
 */
@Component({
  selector: 'qt-memory-cascade-settings',
  changeDetection: ChangeDetectionStrategy.OnPush,
  imports: [ErrorAlert, SettingsCard],
  template: `
    @if (loading()) {
      <p class="qt-text-small qt-text-muted">Loading memory-cascade preferences…</p>
    } @else {
      <qt-settings-card
        title="Memory Cascade Behavior"
        subtitle="Control what happens to auto-extracted memories when you delete or regenerate messages."
      >
        @if (saveError(); as msg) {
          <qt-error-alert [message]="msg" class="mb-3" />
        }

        <div class="space-y-6">
          <!-- On message delete -->
          <div>
            <label for="cascade-delete" class="qt-text-label block mb-2">
              When deleting a message:
            </label>
            <select
              id="cascade-delete"
              class="qt-select w-full"
              [disabled]="saving()"
              (change)="onUpdate({ onMessageDelete: $any($event.target).value })"
            >
              @for (action of deleteActions; track action.value) {
                <option [value]="action.value" [selected]="current().onMessageDelete === action.value">
                  {{ action.label }}
                </option>
              }
            </select>
            <p class="qt-text-xs qt-text-secondary mt-1">{{ deleteDescription() }}</p>
          </div>

          <!-- On swipe / regenerate -->
          <div>
            <label for="cascade-swipe" class="qt-text-label block mb-2">
              When regenerating a response (swipe):
            </label>
            <select
              id="cascade-swipe"
              class="qt-select w-full"
              [disabled]="saving()"
              (change)="onUpdate({ onSwipeRegenerate: $any($event.target).value })"
            >
              @for (action of swipeActions; track action.value) {
                <option
                  [value]="action.value"
                  [selected]="current().onSwipeRegenerate === action.value"
                >
                  {{ action.label }}
                </option>
              }
            </select>
            <p class="qt-text-xs qt-text-secondary mt-1">{{ swipeDescription() }}</p>
          </div>
        </div>
      </qt-settings-card>
    }
  `,
})
export class MemoryCascadeSettings extends ChatSettingsCard {
  protected readonly deleteActions = MEMORY_CASCADE_ACTIONS;
  /** v4 filters ASK_EVERY_TIME out of the swipe select — nothing asks there. */
  protected readonly swipeActions = MEMORY_CASCADE_ACTIONS.filter(
    (a) => a.value !== 'ASK_EVERY_TIME',
  );

  /** v4 `settings.memoryCascadePreferences || DEFAULT_MEMORY_CASCADE_PREFERENCES`. */
  protected readonly current = computed<MemoryCascadePreferences>(
    () =>
      (this.settings()?.['memoryCascadePreferences'] as MemoryCascadePreferences | undefined) ??
      DEFAULT_MEMORY_CASCADE_PREFERENCES,
  );

  protected readonly deleteDescription = computed(
    () => MEMORY_CASCADE_ACTIONS.find((a) => a.value === this.current().onMessageDelete)?.description,
  );
  protected readonly swipeDescription = computed(
    () =>
      MEMORY_CASCADE_ACTIONS.find((a) => a.value === this.current().onSwipeRegenerate)?.description,
  );

  /** v4 `{ memoryCascadePreferences: { ...currentPrefs, ...updates } }`. */
  protected async onUpdate(updates: Partial<Record<string, MemoryCascadeAction>>): Promise<void> {
    await this.save(
      { memoryCascadePreferences: { ...this.current(), ...updates } },
      'Failed to update memory cascade preferences',
    );
  }
}
