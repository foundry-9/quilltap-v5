import { ChangeDetectionStrategy, Component, computed } from '@angular/core';

import { ErrorAlert } from '../../../ui/error-alert';
import type { ConciergeDisplaySettings } from '../chat/chat-settings.types';
import { ConciergeSettingsCard } from './concierge-settings.api';

/** v4 `DISPLAY_MODE_OPTIONS` (#76's descriptions — not the retired card's). */
export const DISPLAY_MODE_OPTIONS: readonly {
  value: ConciergeDisplaySettings['mode'];
  label: string;
  description: string;
}[] = [
  { value: 'SHOW', label: 'Show', description: 'Flagged content is shown normally.' },
  {
    value: 'BLUR',
    label: 'Blur',
    description: 'Flagged content is blurred until you click to reveal it.',
  },
  {
    value: 'COLLAPSE',
    label: 'Collapse',
    description: 'Flagged content is folded away behind a placeholder.',
  },
];

/**
 * How flagged or Unmoderated content looks in the Salon (v4 `components/
 * settings/concierge-settings/DisplayCard.tsx`, `3b463d6b1`). Only consulted
 * while the Concierge is on duty; off duty, everything is shown plainly.
 *
 * **Write-only in v5 — a recorded divergence (P4.D230, ruling E.1).** v5 has
 * no message danger-flag UI (no badges, no blur, no collapse, no "Not
 * Dangerous"), so nothing reads `display.mode` or `display.showWarningBadges`
 * yet. The card ships exactly as v4 renders it; the settings are stored and
 * wait for that vertical.
 */
@Component({
  selector: 'qt-concierge-display-card',
  changeDetection: ChangeDetectionStrategy.OnPush,
  imports: [ErrorAlert],
  template: `
    <div class="space-y-6">
      @if (saveError(); as msg) {
        <qt-error-alert [message]="msg" />
      }

      <div class="space-y-2">
        <label for="concierge-display-mode" class="block qt-text-label">Flagged content</label>
        <select
          id="concierge-display-mode"
          class="qt-select"
          [disabled]="saving()"
          (change)="update({ display: { mode: $any($event.target).value } })"
        >
          @for (option of displayModeOptions; track option.value) {
            <option [value]="option.value" [selected]="concierge().display.mode === option.value">
              {{ option.label }}
            </option>
          }
        </select>
        @if (selectedMode(); as mode) {
          <p class="qt-text-small">{{ mode.description }}</p>
        }
      </div>

      <div>
        <label class="qt-settings-toggle-row">
          <input
            type="checkbox"
            class="qt-checkbox mt-1"
            [checked]="concierge().display.showWarningBadges"
            [disabled]="saving()"
            (change)="update({ display: { showWarningBadges: $any($event.target).checked } })"
          />
          <div class="flex-1">
            <div class="qt-settings-section-heading">Show warning badges</div>
            <div class="qt-text-small mt-1">Display category badges on flagged messages.</div>
          </div>
        </label>
      </div>
    </div>
  `,
})
export class DisplayCard extends ConciergeSettingsCard {
  protected readonly displayModeOptions = DISPLAY_MODE_OPTIONS;

  protected readonly selectedMode = computed(() =>
    DISPLAY_MODE_OPTIONS.find((o) => o.value === this.concierge().display.mode),
  );
}
