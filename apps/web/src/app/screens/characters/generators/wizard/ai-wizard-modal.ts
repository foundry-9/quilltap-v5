import { ChangeDetectionStrategy, Component, computed, inject, input, output } from '@angular/core';

import { Icon } from '../../../../ui/icon';
import type { GeneratedCharacterData } from '../edit-generators.api';
import { WizardDescriptionSourceStep } from './description-source-step';
import { WizardFieldSelectionStep } from './field-selection-step';
import { WizardGenerationStep } from './generation-step';
import { WizardProfileSelectionStep } from './profile-selection-step';
import { WizardState, type WizardStep } from './wizard-state';
import type { WizardCharacterData } from './wizard-types';

/** v4 `STEP_TITLES` (`AIWizardModal.tsx:20-25`). */
const STEP_TITLES: Record<WizardStep, string> = {
  1: 'Select AI Model',
  2: 'Physical Description Source',
  3: 'Select Fields',
  4: 'Generate',
};

/**
 * The AI Wizard modal — v4 `components/characters/ai-wizard/AIWizardModal.tsx`
 * (211 lines): a four-step flow (bespoke overlay, not v4's `BaseModal` — v4
 * doesn't use it here either) over {@link WizardState}, mounted from both
 * New Character and character-edit. Nothing is persisted here; `apply` emits
 * the generated data for the host form to write in, per the host's own rules
 * (New Character stages physical-description/wardrobe/scenarios/properties
 * until AFTER creation; edit applies most of them immediately).
 */
@Component({
  selector: 'qt-ai-wizard-modal',
  changeDetection: ChangeDetectionStrategy.OnPush,
  providers: [WizardState],
  imports: [
    Icon,
    WizardProfileSelectionStep,
    WizardDescriptionSourceStep,
    WizardFieldSelectionStep,
    WizardGenerationStep,
  ],
  host: {
    '(document:keydown.escape)': 'onEscape()',
  },
  template: `
    <div
      class="fixed inset-0 z-50 flex items-center justify-center qt-dialog-overlay"
      (click)="handleClose()"
    >
      <div
        class="qt-dialog w-full max-w-2xl max-h-[90vh] m-4 flex flex-col"
        (click)="$event.stopPropagation()"
      >
        <div class="qt-dialog-header flex justify-between items-center flex-shrink-0">
          <div>
            <h2 class="qt-dialog-title flex items-center gap-2">
              <qt-icon name="wand" class="w-5 h-5 text-primary" />
              AI Wizard
            </h2>
            <p class="text-sm qt-text-secondary mt-1">{{ stepTitle() }}</p>
          </div>
          <button
            type="button"
            class="qt-button-icon qt-button-ghost disabled:opacity-50"
            [disabled]="wizard.generating()"
            (click)="handleClose()"
          >
            <qt-icon name="close" class="w-5 h-5" />
          </button>
        </div>

        <div class="px-6 py-4 border-b qt-border-default flex-shrink-0">
          <div class="flex items-center justify-between">
            @for (step of steps; track step) {
              <div class="flex items-center">
                <div
                  [class]="
                    'w-8 h-8 rounded-full flex items-center justify-center qt-label transition-colors ' +
                    stepCircleClass(step)
                  "
                >
                  @if (step < wizard.currentStep()) {
                    <qt-icon name="check" class="w-4 h-4" />
                  } @else {
                    {{ step }}
                  }
                </div>
                @if (step < 4) {
                  <div
                    [class]="
                      'w-16 sm:w-24 h-0.5 mx-2 transition-colors ' +
                      (step < wizard.currentStep() ? 'bg-primary' : 'qt-bg-muted')
                    "
                  ></div>
                }
              </div>
            }
          </div>
        </div>

        <div class="flex-1 overflow-y-auto px-6 py-4">
          @switch (wizard.currentStep()) {
            @case (1) {
              <qt-wizard-profile-selection-step />
            }
            @case (2) {
              <qt-wizard-description-source-step [characterId]="characterId()" />
            }
            @case (3) {
              <qt-wizard-field-selection-step />
            }
            @case (4) {
              <qt-wizard-generation-step />
            }
          }
        </div>

        @if (wizard.currentStep() < 4) {
          <div class="qt-dialog-footer flex justify-between flex-shrink-0">
            <button
              type="button"
              class="qt-button-secondary disabled:opacity-50"
              [disabled]="!canGoBack()"
              (click)="wizard.prevStep()"
            >
              Back
            </button>
            <button
              type="button"
              class="qt-button-primary disabled:opacity-50"
              [disabled]="!wizard.canProceed()"
              (click)="wizard.nextStep()"
            >
              {{ wizard.currentStep() === 3 ? 'Review & Generate' : 'Next' }}
            </button>
          </div>
        }
      </div>
    </div>
  `,
})
export class AiWizardModal {
  protected readonly wizard = inject(WizardState);
  protected readonly steps: WizardStep[] = [1, 2, 3, 4];

  readonly characterId = input<string | undefined>(undefined);
  readonly characterName = input.required<string>();
  readonly currentData = input.required<WizardCharacterData>();

  readonly apply = output<GeneratedCharacterData>();
  readonly closeModal = output<void>();

  protected readonly stepTitle = computed(() => STEP_TITLES[this.wizard.currentStep()]);
  protected readonly canGoBack = computed(
    () => this.wizard.currentStep() > 1 && !this.wizard.generating(),
  );

  constructor() {
    this.wizard.configure({
      characterId: this.characterId,
      characterName: this.characterName,
      currentData: this.currentData,
      onApply: (data) => this.apply.emit(data),
      onClose: () => this.closeModal.emit(),
    });
    void this.wizard.fetchProfiles();
  }

  protected handleClose(): void {
    if (!this.wizard.generating()) {
      this.closeModal.emit();
    }
  }

  protected onEscape(): void {
    this.handleClose();
  }

  protected stepCircleClass(step: WizardStep): string {
    if (step < this.wizard.currentStep()) return 'bg-primary text-primary-foreground';
    if (step === this.wizard.currentStep()) {
      return 'bg-primary text-primary-foreground ring-2 ring-primary ring-offset-2 ring-offset-background';
    }
    return 'qt-bg-muted qt-text-secondary';
  }
}
