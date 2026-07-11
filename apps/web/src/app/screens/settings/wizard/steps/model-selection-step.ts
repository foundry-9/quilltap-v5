import {
  ChangeDetectionStrategy,
  Component,
  OnInit,
  computed,
  inject,
  signal,
} from '@angular/core';

import { CoreClient } from '../../../../core/core-client';
import type { ProviderInfo } from '../../../../core/core-contract';
import { ModelSelector } from '../../../../ui/model-selector';
import { fetchModels } from '../wizard-api';
import { WizardStore } from '../wizard-state';

/** Wizard step 3 — pick the chat + cheap models (v4 `steps/ModelSelectionStep.tsx`). */
@Component({
  selector: 'qt-wizard-model-step',
  changeDetection: ChangeDetectionStrategy.OnPush,
  imports: [ModelSelector],
  template: `
    <div class="space-y-8">
      <div>
        <h2 class="qt-heading-2">Choose Your Models</h2>
        <p class="qt-text-muted mt-1">
          Select which models to use for chat. You can always change these later in settings.
        </p>
      </div>

      <section class="space-y-4">
        <h3 class="font-semibold text-lg">Primary Chat Model</h3>
        <p class="qt-text-muted qt-text-small">
          This is the main model used for conversations with your characters.
        </p>

        @for (p of chatProviders(); track p.id) {
          <div class="qt-card p-4">
            <div class="flex items-center gap-3 mb-3">
              <span
                class="inline-flex items-center justify-center w-8 h-8 rounded-lg text-xs font-bold qt-bg-muted"
                >{{ p.abbreviation }}</span
              >
              <span class="font-medium">{{ p.displayName }}</span>
              @if (store.chatConfig().provider === p.id && store.chatConfig().model) {
                <span class="ml-auto qt-text-xs qt-text-primary font-medium">Selected</span>
              }
            </div>

            @if (loadingModels()[p.id]) {
              <div class="flex items-center gap-2 py-2">
                <span class="qt-spinner qt-spinner-sm"></span>
                <span class="qt-text-muted qt-text-small">Fetching models...</span>
              </div>
            } @else if (modelErrors()[p.id]) {
              <div class="space-y-2">
                <div class="qt-alert-error">{{ modelErrors()[p.id] }}</div>
                <button
                  type="button"
                  class="qt-button qt-button-secondary qt-text-small"
                  (click)="loadModels(p)"
                >
                  Retry
                </button>
              </div>
            } @else {
              <qt-model-selector
                [models]="store.availableModels()[p.id] || []"
                [value]="store.chatConfig().provider === p.id ? store.chatConfig().model : ''"
                [placeholder]="'Select a ' + p.displayName + ' model'"
                [disabled]="(store.availableModels()[p.id] || []).length === 0"
                (changed)="store.setChatConfig({ provider: p.id, model: $event })"
              />
              @if ((store.availableModels()[p.id] || []).length === 0) {
                <p class="qt-text-muted qt-text-xs mt-1">
                  No models available. Ensure your API key is valid.
                </p>
              }
            }
          </div>
        }

        @if (chatProviders().length === 0) {
          <div class="qt-alert-warning">
            No chat-capable providers selected. Go back and select a provider with chat capability.
          </div>
        }
      </section>

      <section class="space-y-4">
        <h3 class="font-semibold text-lg">Background Model (Cheap LLM)</h3>
        <p class="qt-text-muted qt-text-small">
          A smaller, faster model used for background tasks like summaries, memory management, and
          classification. This keeps costs down while your primary model handles conversations.
        </p>

        <div class="space-y-3">
          <label class="flex items-start gap-3 qt-card p-4 cursor-pointer">
            <input
              type="radio"
              name="cheapStrategy"
              class="mt-0.5"
              [checked]="store.cheapConfig().strategy === 'PROVIDER_CHEAPEST'"
              (change)="
                store.setCheapConfig({
                  strategy: 'PROVIDER_CHEAPEST',
                  provider: undefined,
                  model: undefined,
                })
              "
            />
            <div>
              <span class="font-medium">Auto (Recommended)</span>
              <p class="qt-text-muted qt-text-small mt-0.5">
                Automatically use the cheapest available model from your primary chat provider. This
                is the simplest option and works well for most users.
              </p>
            </div>
          </label>

          <label class="flex items-start gap-3 qt-card p-4 cursor-pointer">
            <input
              type="radio"
              name="cheapStrategy"
              class="mt-0.5"
              [checked]="store.cheapConfig().strategy === 'USER_DEFINED'"
              (change)="store.setCheapConfig({ strategy: 'USER_DEFINED' })"
            />
            <div>
              <span class="font-medium">Manual</span>
              <p class="qt-text-muted qt-text-small mt-0.5">
                Choose a specific provider and model for background tasks.
              </p>
            </div>
          </label>
        </div>

        @if (store.cheapConfig().strategy === 'USER_DEFINED') {
          <div class="ml-6 space-y-4 border-l-2 qt-border-default pl-4">
            <div>
              <label class="qt-text-label block mb-1.5">Provider</label>
              <select
                class="qt-input w-full"
                (change)="store.setCheapConfig({ provider: $any($event.target).value, model: '' })"
              >
                <option value="" [selected]="!store.cheapConfig().provider">
                  Select a provider
                </option>
                @for (p of chatProviders(); track p.id) {
                  <option [value]="p.id" [selected]="store.cheapConfig().provider === p.id">
                    {{ p.displayName }}
                  </option>
                }
              </select>
            </div>
            @if (store.cheapConfig().provider) {
              <div>
                <label class="qt-text-label block mb-1.5">Model</label>
                <qt-model-selector
                  [models]="cheapModels()"
                  [value]="store.cheapConfig().model || ''"
                  placeholder="Select a background model"
                  [disabled]="cheapModels().length === 0"
                  (changed)="store.setCheapConfig({ model: $event })"
                />
              </div>
            }
          </div>
        }
      </section>
    </div>
  `,
})
export class ModelSelectionStep implements OnInit {
  protected readonly store = inject(WizardStore);
  private readonly core = inject(CoreClient);

  protected readonly loadingModels = signal<Record<string, boolean>>({});
  protected readonly modelErrors = signal<Record<string, string>>({});

  protected readonly chatProviders = computed(() => this.store.chatProviders());

  protected readonly cheapModels = computed(() => {
    const provider = this.store.cheapConfig().provider;
    return provider ? (this.store.availableModels()[provider] ?? []) : [];
  });

  ngOnInit(): void {
    for (const provider of this.chatProviders()) {
      if (!this.store.availableModels()[provider.id] && !this.loadingModels()[provider.id]) {
        void this.loadModels(provider);
      }
    }
  }

  protected async loadModels(provider: ProviderInfo): Promise<void> {
    this.loadingModels.update((m) => ({ ...m, [provider.id]: true }));
    this.modelErrors.update((m) => {
      const next = { ...m };
      delete next[provider.id];
      return next;
    });
    try {
      const models = await fetchModels(
        this.core,
        provider.id,
        this.store.apiKeys()[provider.id]?.apiKeyId,
        this.store.baseUrls()[provider.id] || undefined,
      );
      this.store.setModels(provider.id, models);
    } catch (err) {
      this.modelErrors.update((m) => ({
        ...m,
        [provider.id]: err instanceof Error ? err.message : 'Failed to fetch models',
      }));
    } finally {
      this.loadingModels.update((m) => ({ ...m, [provider.id]: false }));
    }
  }
}
