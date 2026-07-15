import { ChangeDetectionStrategy, Component, computed } from '@angular/core';

import { ErrorAlert } from '../../../ui/error-alert';
import { ChatSettingsCard } from './chat-settings.api';
import {
  DEFAULT_AGENT_MODE_SETTINGS,
  MAX_TURNS_OPTIONS,
  type AgentModeSettings as AgentModeBag,
} from './chat-settings.types';
import { SettingsCard } from './settings-card';

/**
 * The Agent Mode card (v4 `components/settings/chat-settings/
 * AgentModeSettings.tsx`): iterative tool use with self-correction.
 *
 * v4 wires this through TWO handlers rather than one merged update
 * (`handleAgentModeDefaultEnabledChange`, `handleAgentModeMaxTurnsChange`), each
 * PUTting the whole `agentModeSettings` bag with its own key replaced. Ported as
 * two methods for the same reason — they are separate v4 call sites with
 * separate semantics (a toggle vs a select).
 */
@Component({
  selector: 'qt-agent-mode-settings',
  changeDetection: ChangeDetectionStrategy.OnPush,
  imports: [ErrorAlert, SettingsCard],
  template: `
    @if (loading()) {
      <p class="qt-text-small qt-text-muted">Loading agent-mode settings…</p>
    } @else {
      <qt-settings-card
        title="Agent Mode"
        subtitle="Configure iterative tool use with self-correction"
      >
        @if (saveError(); as msg) {
          <qt-error-alert [message]="msg" class="mb-3" />
        }

        <div class="space-y-6">
          <!-- Default enabled -->
          <div>
            <label
              class="flex items-start gap-3 p-4 border qt-border-default rounded qt-hover-accent cursor-pointer"
            >
              <input
                type="checkbox"
                class="mt-1 h-4 w-4 rounded border-input text-primary focus:ring-primary"
                [checked]="current().defaultEnabled"
                [disabled]="saving()"
                (change)="onDefaultEnabledChange($any($event.target).checked)"
              />
              <div class="flex-1">
                <div class="font-medium text-foreground">Enable Agent Mode by Default</div>
                <div class="qt-text-small mt-1">
                  When enabled, new chats will use agent mode, allowing the AI to iteratively use
                  tools, verify results, and self-correct before delivering a final response.
                </div>
              </div>
            </label>
          </div>

          <!-- Max turns -->
          <div class="space-y-2">
            <label for="agent-max-turns" class="block font-medium text-foreground">
              Maximum Agent Turns
            </label>
            <p class="qt-text-small">
              The maximum number of tool iterations before the AI must deliver its final response.
              Higher values allow more thorough exploration but may increase response time and cost.
            </p>
            <select
              id="agent-max-turns"
              class="w-full max-w-xs rounded-lg border qt-border-default qt-bg-card px-3 py-2 text-foreground focus:qt-border-primary focus:outline-none focus:ring-1 focus:ring-primary"
              [disabled]="saving()"
              (change)="onMaxTurnsChange($any($event.target).value)"
            >
              @for (option of maxTurnsOptions; track option.value) {
                <option [value]="option.value" [selected]="current().maxTurns === option.value">
                  {{ option.label }}
                </option>
              }
            </select>
          </div>

          <!-- Info box -->
          <div class="rounded-lg border qt-border-default qt-bg-muted/50 p-4">
            <h4 class="font-medium text-foreground mb-2">How Agent Mode Works</h4>
            <ul class="qt-text-small space-y-1 list-disc list-inside">
              <li>The AI uses tools iteratively to gather information and verify results</li>
              <li>Each tool use counts as one &quot;turn&quot; toward the maximum</li>
              <li>
                When ready, the AI calls
                <code class="qt-bg-muted px-1 rounded">submit_final_response</code> to deliver its
                answer
              </li>
              <li>If the turn limit is reached, the AI is prompted to submit its best answer</li>
              <li>Agent mode can be enabled/disabled per-character, per-project, or per-chat</li>
            </ul>
          </div>
        </div>
      </qt-settings-card>
    }
  `,
})
export class AgentModeSettings extends ChatSettingsCard {
  protected readonly maxTurnsOptions = MAX_TURNS_OPTIONS;

  /**
   * v4 `settings.agentModeSettings ?? { maxTurns: 10, defaultEnabled: false }` —
   * the card inlines that literal, which is `DEFAULT_AGENT_MODE_SETTINGS`
   * (the handlers use the named constant for the same fallback).
   */
  protected readonly current = computed<AgentModeBag>(
    () =>
      (this.settings()?.['agentModeSettings'] as AgentModeBag | undefined) ??
      DEFAULT_AGENT_MODE_SETTINGS,
  );

  /** v4 `handleAgentModeDefaultEnabledChange`. */
  protected async onDefaultEnabledChange(value: boolean): Promise<void> {
    await this.save(
      { agentModeSettings: { ...this.current(), defaultEnabled: value } },
      'Failed to update agent mode settings',
    );
  }

  /** v4 `handleAgentModeMaxTurnsChange` — v4 `parseInt(e.target.value, 10)`. */
  protected async onMaxTurnsChange(raw: string): Promise<void> {
    await this.save(
      { agentModeSettings: { ...this.current(), maxTurns: Number.parseInt(raw, 10) } },
      'Failed to update agent mode settings',
    );
  }
}
