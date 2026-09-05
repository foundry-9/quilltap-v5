import { ChangeDetectionStrategy, Component, computed } from '@angular/core';

import { ErrorAlert } from '../../../ui/error-alert';
import { ChatSettingsCard } from './chat-settings.api';
import {
  DEFAULT_ANSWER_CONFIRMATION_SETTINGS,
  type AnswerConfirmationSettings as AnswerConfirmationSettingsBag,
} from './chat-settings.types';
import { SettingsCard } from './settings-card';

/**
 * The Answer Confirmation card (v4 `components/settings/chat-settings/
 * AnswerConfirmationSettings.tsx`): the global default for the Salon's
 * answer-confirmation check. When a character leans on their recollections or a
 * lookup this turn, a quiet second reader vets the reply against that material
 * before it lands — and, if something looks off, the character is given the
 * chance to stand by it or set it right. Off by default; a whole project or a
 * single chat may overrule this.
 *
 * The engine side is ported (the P4.d answer-confirmation re-port); this is the
 * global dial. v4's handler merges over BOTH the defaults and the stored bag
 * before PUTting the whole `answerConfirmationSettings` object — reproduced
 * exactly in {@link onUpdate}.
 */
@Component({
  selector: 'qt-answer-confirmation-settings',
  changeDetection: ChangeDetectionStrategy.OnPush,
  imports: [ErrorAlert, SettingsCard],
  template: `
    @if (loading()) {
      <p class="qt-text-small qt-text-muted">Loading answer-confirmation setting…</p>
    } @else {
      <qt-settings-card
        title="Answer Confirmation"
        subtitle="Vet a character's looked-up answers against what they actually knew this turn"
      >
        @if (saveError(); as msg) {
          <qt-error-alert [message]="msg" class="mb-3" />
        }

        <div class="space-y-3">
          <div>
            <label class="qt-settings-toggle-row">
              <input
                type="checkbox"
                class="qt-checkbox mt-1"
                [checked]="enabled()"
                [disabled]="saving()"
                (change)="onUpdate({ enabled: $any($event.target).checked })"
              />
              <div class="flex-1">
                <div class="qt-settings-section-heading">
                  Confirm looked-up answers by default
                </div>
                <div class="qt-text-small mt-1">
                  When a character's reply rests on their recollections or a lookup (a web search,
                  a peek back through the conversation, or a document read), a swift second reader
                  checks the reply for contradictions before it lands. Should something ring false,
                  the character is asked to stand by their words or amend them — and every checked
                  reply wears a small mark you can hover for the particulars. This adds a
                  round-trip or two per qualifying turn, so it arrives switched off; enable it
                  here, or for a particular project or chat.
                </div>
              </div>
            </label>
          </div>
        </div>
      </qt-settings-card>
    }
  `,
})
export class AnswerConfirmationSettings extends ChatSettingsCard {
  private readonly bag = computed(
    () => (this.settings()?.['answerConfirmationSettings'] ?? {}) as AnswerConfirmationSettingsBag,
  );

  /** v4 `settings.answerConfirmationSettings?.enabled ?? DEFAULT.enabled ?? false`. */
  protected readonly enabled = computed(
    () => this.bag().enabled ?? DEFAULT_ANSWER_CONFIRMATION_SETTINGS.enabled ?? false,
  );

  /** v4 `{ ...DEFAULT_ANSWER_CONFIRMATION_SETTINGS, ...(settings.answerConfirmationSettings ?? {}), ...updates }`. */
  protected async onUpdate(updates: Partial<AnswerConfirmationSettingsBag>): Promise<void> {
    const merged: AnswerConfirmationSettingsBag = {
      ...DEFAULT_ANSWER_CONFIRMATION_SETTINGS,
      ...this.bag(),
      ...updates,
    };
    await this.save(
      { answerConfirmationSettings: merged },
      'Failed to update answer-confirmation settings',
    );
  }
}
