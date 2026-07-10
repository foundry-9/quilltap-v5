import { describe, expect, it } from 'vitest';

import type { ProviderInfo } from '../../../core/core-contract';
import { WizardStore } from './wizard-state';

function provider(over: Partial<ProviderInfo>): ProviderInfo {
  return {
    id: 'OPENAI',
    name: 'OPENAI',
    displayName: 'OpenAI',
    description: '',
    abbreviation: 'AI',
    type: 'llm',
    capabilities: { chat: true, imageGeneration: false, embeddings: false, webSearch: false },
    configRequirements: { requiresApiKey: true, requiresBaseUrl: false },
    ...over,
  };
}

describe('WizardStore (reducer walk)', () => {
  it('starts on providers and cannot proceed until a chat-capable provider is selected', () => {
    const store = new WizardStore();
    store.availableProviders.set([
      provider({
        id: 'OPENAI',
        capabilities: { chat: true, imageGeneration: false, embeddings: false, webSearch: false },
      }),
      provider({
        id: 'VOYAGE',
        name: 'VOYAGE',
        capabilities: { chat: false, imageGeneration: false, embeddings: true, webSearch: false },
      }),
    ]);
    expect(store.currentStep()).toBe('providers');
    expect(store.canProceed()).toBe(false);

    // Selecting an embeddings-only provider is not enough.
    store.toggleProvider('VOYAGE');
    expect(store.hasValidChatProvider()).toBe(false);
    expect(store.canProceed()).toBe(false);

    // Adding a chat-capable provider unlocks the step.
    store.toggleProvider('OPENAI');
    expect(store.hasValidChatProvider()).toBe(true);
    expect(store.canProceed()).toBe(true);
  });

  it('api-keys step needs every key-requiring provider validated', () => {
    const store = new WizardStore();
    store.availableProviders.set([
      provider({ id: 'OPENAI' }),
      provider({
        id: 'OLLAMA',
        name: 'OLLAMA',
        configRequirements: { requiresApiKey: false, requiresBaseUrl: true },
      }),
    ]);
    store.selectedProviders.set(['OPENAI', 'OLLAMA']);
    store.setStep('api-keys');
    // OLLAMA needs no key; OPENAI does and isn't validated yet.
    expect(store.allKeysValidated()).toBe(false);
    expect(store.canProceed()).toBe(false);

    store.setApiKey('OPENAI', { label: 'k', key: 'sk-x', validated: true, apiKeyId: 'id1' });
    expect(store.allKeysValidated()).toBe(true);
    expect(store.canProceed()).toBe(true);
  });

  it('models step needs a chat provider + model chosen', () => {
    const store = new WizardStore();
    store.setStep('models');
    expect(store.canProceed()).toBe(false);
    store.setChatConfig({ provider: 'OPENAI', model: 'gpt-4' });
    expect(store.canProceed()).toBe(true);
  });

  it('goNext/goBack move across the six steps and clamp at the ends', () => {
    const store = new WizardStore();
    expect(store.isFirstStep()).toBe(true);
    store.goBack();
    expect(store.currentStep()).toBe('providers'); // clamped
    for (const step of ['api-keys', 'models', 'embedding', 'image', 'confirm']) {
      store.goNext();
      expect(store.currentStep()).toBe(step);
    }
    expect(store.isLastStep()).toBe(true);
    store.goNext();
    expect(store.currentStep()).toBe('confirm'); // clamped
  });

  it('toggleProvider adds then removes a provider', () => {
    const store = new WizardStore();
    store.toggleProvider('OPENAI');
    expect(store.selectedProviders()).toEqual(['OPENAI']);
    store.toggleProvider('OPENAI');
    expect(store.selectedProviders()).toEqual([]);
  });
});
