import { ChangeDetectionStrategy, Component, computed, inject } from '@angular/core';

import { Icon } from '../../../../ui/icon';
import { WizardState } from './wizard-state';

/**
 * Wizard step 1 — v4 `ai-wizard/steps/ProfileSelectionStep.tsx` (143 lines).
 * Injects {@link WizardState} directly (a sibling component-provided service)
 * rather than v4's prop-drilled `ProfileSelectionStepProps`.
 */
@Component({
  selector: 'qt-wizard-profile-selection-step',
  changeDetection: ChangeDetectionStrategy.OnPush,
  imports: [Icon],
  template: `
    @if (wizard.loadingProfiles()) {
      <div class="flex items-center justify-center py-12">
        <div class="flex items-center gap-3 qt-text-secondary">
          <svg class="w-5 h-5 animate-spin" fill="none" viewBox="0 0 24 24">
            <circle
              class="opacity-25"
              cx="12"
              cy="12"
              r="10"
              stroke="currentColor"
              stroke-width="4"
            />
            <path
              class="opacity-75"
              fill="currentColor"
              d="M4 12a8 8 0 018-8V0C5.373 0 0 5.373 0 12h4zm2 5.291A7.962 7.962 0 014 12H0c0 3.042 1.135 5.824 3 7.938l3-2.647z"
            />
          </svg>
          <span>Loading connection profiles...</span>
        </div>
      </div>
    } @else if (wizard.error()) {
      <div class="qt-alert-error">{{ wizard.error() }}</div>
    } @else if (wizard.profiles().length === 0) {
      <div class="qt-alert-error">
        <p class="font-medium">No connection profiles available</p>
        <p class="text-sm mt-1">
          Please create a connection profile in Settings before using the AI Wizard.
        </p>
      </div>
    } @else {
      <div class="space-y-6">
        <div>
          <h3 class="qt-heading-4 text-foreground mb-2">Select AI Model</h3>
          <p class="text-sm qt-text-secondary">
            Choose the connection profile that will generate your character's details.
          </p>
        </div>

        <div>
          <label for="primaryProfile" class="qt-label">Connection Profile *</label>
          <select
            id="primaryProfile"
            class="qt-select"
            [value]="wizard.primaryProfileId()"
            (change)="wizard.setPrimaryProfileId($any($event.target).value)"
          >
            <option value="">Select a profile...</option>
            @for (profile of wizard.profiles(); track profile.id) {
              <option [value]="profile.id" [selected]="wizard.primaryProfileId() === profile.id">
                {{ profile.name }} ({{ profile.provider }} - {{ profile.modelName }}){{
                  profile.isDefault ? ' (Default)' : ''
                }}
              </option>
            }
          </select>
        </div>

        @if (selectedProfile(); as profile) {
          <div class="p-4 rounded-lg border qt-border-default qt-bg-muted/30">
            <h4 class="font-medium text-foreground mb-2">Selected Profile</h4>
            <dl class="grid grid-cols-2 gap-2 text-sm">
              <dt class="qt-text-secondary">Provider:</dt>
              <dd class="text-foreground">{{ profile.provider }}</dd>
              <dt class="qt-text-secondary">Model:</dt>
              <dd class="text-foreground">{{ profile.modelName }}</dd>
              @if (profile.isCheap) {
                <dt class="qt-text-secondary">Type:</dt>
                <dd class="text-foreground">
                  <span
                    class="inline-flex items-center px-2 py-0.5 rounded qt-text-label-xs qt-bg-success/10 qt-text-success"
                  >
                    Cost-efficient
                  </span>
                </dd>
              }
            </dl>
          </div>
        }

        <div class="p-4 rounded-lg border qt-border-default qt-bg-muted/20">
          <h4 class="font-medium text-foreground mb-2 flex items-center gap-2">
            <qt-icon name="info" class="w-4 h-4 qt-text-info" />
            Tip
          </h4>
          <p class="text-sm qt-text-secondary">
            For best results, choose a capable model like GPT-4, Claude Opus, or Gemini Pro.
            Cost-efficient models may produce simpler results.
          </p>
        </div>
      </div>
    }
  `,
})
export class WizardProfileSelectionStep {
  protected readonly wizard = inject(WizardState);

  protected readonly selectedProfile = computed(() =>
    this.wizard.profiles().find((p) => p.id === this.wizard.primaryProfileId()),
  );
}
