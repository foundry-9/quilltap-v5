import { Injectable, computed, signal } from '@angular/core';

import type { ProviderInfo } from '../../../core/core-contract';

export type WizardStep = 'providers' | 'api-keys' | 'models' | 'embedding' | 'image' | 'confirm';

export interface ApiKeyEntry {
  label: string;
  key: string;
  validated: boolean;
  apiKeyId?: string;
  error?: string;
}

export interface ChatConfig {
  provider: string;
  model: string;
}

export interface CheapConfig {
  strategy: 'PROVIDER_CHEAPEST' | 'USER_DEFINED';
  provider?: string;
  model?: string;
}

export type TestStatus = 'pending' | 'testing' | 'success' | 'error';

export const WIZARD_STEPS: WizardStep[] = [
  'providers',
  'api-keys',
  'models',
  'embedding',
  'image',
  'confirm',
];

export const STEP_LABELS: Record<WizardStep, string> = {
  providers: 'Providers',
  'api-keys': 'API Keys',
  models: 'Models',
  embedding: 'Embeddings',
  image: 'Images',
  confirm: 'Confirm',
};

/**
 * The provider-wizard state (v4 `useProviderWizardState.ts`, ported from the
 * reducer to signals). One instance per wizard (provided at the component).
 */
@Injectable()
export class WizardStore {
  readonly currentStep = signal<WizardStep>('providers');
  readonly availableProviders = signal<ProviderInfo[]>([]);
  readonly selectedProviders = signal<string[]>([]);
  readonly apiKeys = signal<Record<string, ApiKeyEntry>>({});
  readonly baseUrls = signal<Record<string, string>>({});
  readonly availableModels = signal<Record<string, string[]>>({});
  readonly chatConfig = signal<ChatConfig>({ provider: '', model: '' });
  readonly cheapConfig = signal<CheapConfig>({ strategy: 'PROVIDER_CHEAPEST' });
  readonly testResults = signal<Record<string, TestStatus>>({});
  readonly saving = signal(false);
  readonly error = signal<string | null>(null);

  readonly currentStepIndex = computed(() => WIZARD_STEPS.indexOf(this.currentStep()));
  readonly isFirstStep = computed(() => this.currentStepIndex() === 0);
  readonly isLastStep = computed(() => this.currentStepIndex() === WIZARD_STEPS.length - 1);
  readonly canGoBack = computed(() => this.currentStepIndex() > 0);

  /** At least one selected provider is chat-capable. */
  readonly hasValidChatProvider = computed(() =>
    this.selectedProviders().some((id) => this.providerById(id)?.capabilities?.chat),
  );

  /** Every selected provider that needs a key has a validated one. */
  readonly allKeysValidated = computed(() =>
    this.selectedProviders().every((id) => {
      const p = this.providerById(id);
      if (!p?.configRequirements.requiresApiKey) return true;
      return this.apiKeys()[id]?.validated === true;
    }),
  );

  providerById(id: string): ProviderInfo | undefined {
    return this.availableProviders().find((p) => p.id === id);
  }

  /** Chat-capable selected providers, in selection order. */
  chatProviders(): ProviderInfo[] {
    return this.selectedProviders()
      .map((id) => this.providerById(id))
      .filter((p): p is ProviderInfo => p !== undefined && !!p.capabilities?.chat);
  }

  /** Step-gate for the Next button (v4 `canProceed`). */
  canProceed(): boolean {
    switch (this.currentStep()) {
      case 'providers':
        return this.selectedProviders().length > 0 && this.hasValidChatProvider();
      case 'api-keys':
        return this.allKeysValidated();
      case 'models':
        return !!this.chatConfig().provider && !!this.chatConfig().model;
      default:
        return true; // embedding / image / confirm
    }
  }

  setStep(step: WizardStep): void {
    this.currentStep.set(step);
    this.error.set(null);
  }

  goNext(): void {
    const next = this.currentStepIndex() + 1;
    if (next < WIZARD_STEPS.length) {
      this.setStep(WIZARD_STEPS[next]);
    }
  }

  goBack(): void {
    const prev = this.currentStepIndex() - 1;
    if (prev >= 0) {
      this.setStep(WIZARD_STEPS[prev]);
    }
  }

  toggleProvider(id: string): void {
    this.selectedProviders.update((ids) =>
      ids.includes(id) ? ids.filter((x) => x !== id) : [...ids, id],
    );
  }

  setApiKey(id: string, entry: ApiKeyEntry): void {
    this.apiKeys.update((m) => ({ ...m, [id]: entry }));
  }

  setBaseUrl(id: string, url: string): void {
    this.baseUrls.update((m) => ({ ...m, [id]: url }));
  }

  setModels(id: string, models: string[]): void {
    this.availableModels.update((m) => ({ ...m, [id]: models }));
  }

  setChatConfig(patch: Partial<ChatConfig>): void {
    this.chatConfig.update((c) => ({ ...c, ...patch }));
  }

  setCheapConfig(patch: Partial<CheapConfig>): void {
    this.cheapConfig.update((c) => ({ ...c, ...patch }));
  }

  setTestResult(key: string, status: TestStatus): void {
    this.testResults.update((m) => ({ ...m, [key]: status }));
  }
}
