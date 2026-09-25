import { ChangeDetectionStrategy, Component, computed } from '@angular/core';

import { ErrorAlert } from '../../../ui/error-alert';
import type { ConciergePreScreenSettings } from '../chat/chat-settings.types';
import { ConciergeSettingsCard } from './concierge-settings.api';

type ScanKey = 'scanTextChat' | 'scanImagePrompts' | 'scanImageGeneration';

/** v4's three "What to scan" toggles, in v4's order, copy verbatim. */
export const SCAN_TOGGLES: readonly { key: ScanKey; heading: string; body: string }[] = [
  {
    key: 'scanTextChat',
    heading: 'Text chat messages',
    body: 'Classify your messages before they are sent to the LLM.',
  },
  {
    key: 'scanImagePrompts',
    heading: 'Image prompts',
    body: 'Classify image generation prompts before expansion.',
  },
  {
    key: 'scanImageGeneration',
    heading: 'Image generation',
    body: 'Classify the expanded prompt before it is sent to the image generator.',
  },
];

/**
 * The optional classifier (v4 `components/settings/concierge-settings/
 * PreScreeningCard.tsx`, `3b463d6b1`): pre-screening messages and image
 * prompts before they are sent, and the background summary read that may
 * switch a chat to Unmoderated. Everything here is off for new installs —
 * refusals, not guesses, are what the Concierge acts on by default.
 *
 * Only the three scan toggles wait on the pre-screen switch
 * (`scansDisabled = saving || !preScreen.enabled`); the summary read is its
 * own switch and is disabled only while saving.
 *
 * The threshold saves on `change` (release) where v4's React `onChange` fires
 * per step of the drag — the same final value, one PUT instead of several,
 * and no mid-drag disable. The custom prompt saves per keystroke, as v4's does.
 */
@Component({
  selector: 'qt-concierge-pre-screening-card',
  changeDetection: ChangeDetectionStrategy.OnPush,
  imports: [ErrorAlert],
  template: `
    <div class="space-y-6">
      @if (saveError(); as msg) {
        <qt-error-alert [message]="msg" />
      }

      <div>
        <label class="qt-settings-toggle-row">
          <input
            type="checkbox"
            class="qt-checkbox mt-1"
            [checked]="preScreen().enabled"
            [disabled]="saving()"
            (change)="updatePreScreen({ enabled: $any($event.target).checked })"
          />
          <div class="flex-1">
            <div class="qt-settings-section-heading">Pre-screen before sending</div>
            <div class="qt-text-small mt-1">
              Classify messages and image prompts before they are sent, and route anything flagged
              on a Moderated chat to the uncensored desk without waiting for a refusal. Costs a
              classification call per item.
            </div>
          </div>
        </label>
      </div>

      <div class="space-y-3">
        <div class="qt-text-label">What to scan</div>

        @for (scan of scanToggles; track scan.key) {
          <div>
            <label class="qt-settings-toggle-row">
              <input
                type="checkbox"
                class="qt-checkbox mt-1"
                [checked]="preScreen()[scan.key]"
                [disabled]="scansDisabled()"
                (change)="onScanChange(scan.key, $any($event.target).checked)"
              />
              <div class="flex-1">
                <div class="qt-settings-section-heading">{{ scan.heading }}</div>
                <div class="qt-text-small mt-1">{{ scan.body }}</div>
              </div>
            </label>
          </div>
        }
      </div>

      <div>
        <label class="qt-settings-toggle-row">
          <input
            type="checkbox"
            class="qt-checkbox mt-1"
            [checked]="preScreen().summaryClassification"
            [disabled]="saving()"
            (change)="updatePreScreen({ summaryClassification: $any($event.target).checked })"
          />
          <div class="flex-1">
            <div class="qt-settings-section-heading">
              Read each chat's summary in the background and switch it when it looks dangerous
            </div>
            <div class="qt-text-small mt-1">
              Every ten minutes the Concierge reads the summaries of Moderated chats and moves any
              that read as dangerous to Unmoderated, with an announcement. Locked chats are never
              moved.
            </div>
          </div>
        </label>
      </div>

      <div class="space-y-2">
        <label for="concierge-threshold" class="block qt-text-label">
          Detection threshold ({{ thresholdLabel() }})
        </label>
        <input
          id="concierge-threshold"
          type="range"
          min="0.1"
          max="1.0"
          step="0.1"
          class="qt-range w-full max-w-xs"
          [value]="preScreen().threshold"
          [disabled]="saving()"
          (change)="updatePreScreen({ threshold: parseThreshold($any($event.target).value) })"
        />
        <p class="qt-text-small">
          Lower values flag more content; higher values flag only strongly dangerous content.
        </p>
      </div>

      <div class="space-y-2">
        <label for="concierge-custom-classification-prompt" class="block qt-text-label">
          Custom classification prompt (optional)
        </label>
        <textarea
          id="concierge-custom-classification-prompt"
          rows="3"
          placeholder="Additional instructions for the content classifier..."
          class="qt-textarea"
          [value]="preScreen().customClassificationPrompt || ''"
          [disabled]="saving()"
          (input)="
            updatePreScreen({ customClassificationPrompt: $any($event.target).value || null })
          "
        ></textarea>
        <p class="qt-text-small">
          Appended to the classification prompt. Use it to adjust sensitivity for your use case.
        </p>
      </div>

      <div class="qt-alert-info">
        <ul class="qt-text-small space-y-1 list-disc list-inside">
          <li>
            With an OpenAI connection profile, classification uses the free OpenAI moderation
            endpoint.
          </li>
          <li>Otherwise it falls back to your cheap LLM, at a small cost per item.</li>
          <li>Classification is fail-safe: an error never blocks a message.</li>
        </ul>
      </div>
    </div>
  `,
})
export class PreScreeningCard extends ConciergeSettingsCard {
  protected readonly scanToggles = SCAN_TOGGLES;

  protected readonly preScreen = computed(() => this.concierge().preScreen);

  /** v4 `saving || !preScreen.enabled` — the three scan toggles only. */
  protected readonly scansDisabled = computed(() => this.saving() || !this.preScreen().enabled);

  /** v4 `preScreen.threshold.toFixed(1)`. */
  protected readonly thresholdLabel = computed(() => this.preScreen().threshold.toFixed(1));

  /** v4 `parseFloat(e.target.value)`. */
  protected parseThreshold(raw: string): number {
    return Number.parseFloat(raw);
  }

  protected updatePreScreen(preScreen: Partial<ConciergePreScreenSettings>): Promise<void> {
    return this.update({ preScreen });
  }

  protected onScanChange(key: ScanKey, value: boolean): Promise<void> {
    return this.updatePreScreen({ [key]: value });
  }
}
