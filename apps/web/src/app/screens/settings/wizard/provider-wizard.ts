import {
  ChangeDetectionStrategy,
  Component,
  OnInit,
  computed,
  inject,
  input,
  output,
} from '@angular/core';

import { CoreClient } from '../../../core/core-client';
import { Icon } from '../../../ui/icon';
import { ApiKeyEntryStep } from './steps/api-key-entry-step';
import { ModelSelectionStep } from './steps/model-selection-step';
import { ProviderSelectionStep } from './steps/provider-selection-step';
import { TestConfirmStep } from './steps/test-confirm-step';
import { loadExistingConfig } from './wizard-api';
import { STEP_LABELS, WIZARD_STEPS, WizardStore, type WizardStep } from './wizard-state';

const STEP_TITLES: Record<WizardStep, { title: string; subtitle: string }> = {
  providers: {
    title: 'Choose Your Providers',
    subtitle:
      'Select the AI providers you wish to engage. Each brings its own particular talents to the enterprise.',
  },
  'api-keys': {
    title: 'Present Your Credentials',
    subtitle:
      'Every establishment requires proper identification. Enter your API keys and we shall verify them forthwith.',
  },
  models: {
    title: 'Select Your Models',
    subtitle:
      'With credentials in order, choose which models shall serve as your primary conversationalists.',
  },
  embedding: {
    title: 'The Memory Engine',
    subtitle:
      'Embeddings power the Commonplace Book — your characters’ long-term memory. This step is entirely optional.',
  },
  image: {
    title: 'The Lantern',
    subtitle:
      'Image generation brings visual atmosphere to your stories. Also entirely optional, but rather dashing.',
  },
  confirm: {
    title: 'Review & Confirm',
    subtitle:
      'A final inspection before we commit these arrangements to the ledger. Test if you like, then save.',
  },
};

/**
 * The provider setup wizard (v4 `components/setup-wizard/ProviderWizard.tsx`):
 * providers → api-keys → models → (embedding · image, skippable) → confirm, over
 * {@link WizardStore}. Settings-mode re-entry pre-populates from the list
 * variants. The embedding/image steps render as v4's skippable steps but skip
 * immediately (their route families are deferred). Victorian copy carries over.
 */
@Component({
  selector: 'qt-provider-wizard',
  changeDetection: ChangeDetectionStrategy.OnPush,
  providers: [WizardStore],
  imports: [Icon, ProviderSelectionStep, ApiKeyEntryStep, ModelSelectionStep, TestConfirmStep],
  template: `
    <div class="qt-auth-page flex items-center justify-center min-h-screen p-4">
      <div class="qt-card max-w-2xl w-full p-6 space-y-6">
        <!-- Step indicator -->
        <nav
          aria-label="Wizard progress"
          class="flex items-center justify-center gap-1 sm:gap-2 mb-6"
        >
          @for (step of steps; track step; let i = $index) {
            <div class="flex items-center">
              <div class="flex flex-col items-center">
                <div
                  [class]="
                    'flex items-center justify-center w-8 h-8 rounded-full qt-label transition-colors ' +
                    (step === store.currentStep()
                      ? 'qt-bg-muted qt-text-primary'
                      : i < store.currentStepIndex()
                        ? 'bg-primary text-primary-foreground'
                        : 'qt-bg-muted qt-text-muted')
                  "
                  [attr.aria-current]="step === store.currentStep() ? 'step' : null"
                >
                  @if (i < store.currentStepIndex()) {
                    <qt-icon name="check" class="w-4 h-4" />
                  } @else {
                    {{ i + 1 }}
                  }
                </div>
                <span
                  [class]="
                    'qt-text-xs mt-1 hidden sm:block ' +
                    (step === store.currentStep() ? 'qt-text-primary font-medium' : 'qt-text-muted')
                  "
                >
                  {{ labels[step] }}
                </span>
              </div>
              @if (i < steps.length - 1) {
                <div
                  [class]="
                    'w-6 sm:w-10 h-0.5 mx-1 ' +
                    (i < store.currentStepIndex() ? 'bg-primary' : 'qt-bg-muted')
                  "
                ></div>
              }
            </div>
          }
        </nav>

        <div>
          <h1 class="qt-heading-2">{{ stepInfo().title }}</h1>
          <p class="qt-text-muted mt-1">{{ stepInfo().subtitle }}</p>
        </div>

        <div class="min-h-[300px]">
          @if (store.error()) {
            <div class="qt-alert-error text-sm mb-4">{{ store.error() }}</div>
          }
          @switch (store.currentStep()) {
            @case ('providers') {
              <qt-wizard-provider-step />
            }
            @case ('api-keys') {
              <qt-wizard-apikey-step />
            }
            @case ('models') {
              <qt-wizard-model-step />
            }
            @case ('embedding') {
              <div class="qt-text-muted text-center py-12">
                Embedding setup arrives with the Commonplace Book vertical. Skip this step for now —
                you can configure it later in Settings.
              </div>
            }
            @case ('image') {
              <div class="qt-text-muted text-center py-12">
                Image generation setup arrives with the Images vertical. Skip this step for now —
                you can configure it later in Settings.
              </div>
            }
            @case ('confirm') {
              <qt-wizard-confirm-step (complete)="complete.emit()" />
            }
          }
        </div>

        <!-- Nav -->
        <div class="pt-4 border-t qt-border-default">
          <div class="flex items-center justify-between">
            <div>
              @if (store.canGoBack() || cancellable()) {
                <button
                  type="button"
                  class="qt-button-secondary"
                  [disabled]="store.saving()"
                  (click)="back()"
                >
                  Back
                </button>
              }
            </div>
            <div class="flex items-center gap-2">
              @if (isSkippable()) {
                <button
                  type="button"
                  class="qt-button-secondary"
                  [disabled]="store.saving()"
                  (click)="skip()"
                >
                  Skip
                </button>
              }
              @if (!store.isLastStep()) {
                <button
                  type="button"
                  class="qt-button-primary"
                  [disabled]="!store.canProceed() || store.saving()"
                  (click)="next()"
                >
                  Next
                </button>
              }
            </div>
          </div>
        </div>
      </div>
    </div>
  `,
})
export class ProviderWizard implements OnInit {
  protected readonly store = inject(WizardStore);
  private readonly core = inject(CoreClient);

  readonly mode = input<'setup' | 'settings'>('setup');
  readonly complete = output<void>();
  readonly cancel = output<void>();

  protected readonly steps = WIZARD_STEPS;
  protected readonly labels = STEP_LABELS;

  protected readonly stepInfo = computed(
    () => STEP_TITLES[this.store.currentStep()] ?? { title: '', subtitle: '' },
  );
  protected readonly isSkippable = computed(
    () => this.store.currentStep() === 'embedding' || this.store.currentStep() === 'image',
  );
  protected readonly cancellable = computed(() => this.store.isFirstStep());

  ngOnInit(): void {
    if (this.mode() === 'settings') {
      void this.loadExisting();
    }
  }

  private async loadExisting(): Promise<void> {
    try {
      const config = await loadExistingConfig(this.core);
      const existingProviders = [...new Set(config.apiKeys.map((k) => k.provider))];
      const apiKeys: Record<
        string,
        { label: string; key: string; validated: boolean; apiKeyId?: string }
      > = {};
      for (const key of config.apiKeys) {
        if (!apiKeys[key.provider]) {
          apiKeys[key.provider] = {
            label: key.label,
            key: '********',
            validated: true,
            apiKeyId: key.id,
          };
        }
      }
      const defaultProfile = config.connectionProfiles.find((p) => p.isDefault);
      const cheapProfile = config.connectionProfiles.find((p) => p.isCheap);

      this.store.selectedProviders.set(existingProviders);
      this.store.apiKeys.set(apiKeys);
      if (defaultProfile) {
        this.store.chatConfig.set({
          provider: defaultProfile.provider,
          model: defaultProfile.modelName,
        });
      }
      if (cheapProfile) {
        this.store.cheapConfig.set({
          strategy: 'USER_DEFINED',
          provider: cheapProfile.provider,
          model: cheapProfile.modelName,
        });
      } else if (config.cheapLLMStrategy === 'PROVIDER_CHEAPEST') {
        this.store.cheapConfig.set({ strategy: 'PROVIDER_CHEAPEST' });
      }
    } catch {
      // Non-fatal — the wizard still works from scratch.
    }
  }

  protected next(): void {
    if (this.store.canProceed()) {
      this.store.goNext();
    }
  }

  protected back(): void {
    if (this.store.canGoBack()) {
      this.store.goBack();
    } else {
      this.cancel.emit();
    }
  }

  protected skip(): void {
    this.store.goNext();
  }
}
