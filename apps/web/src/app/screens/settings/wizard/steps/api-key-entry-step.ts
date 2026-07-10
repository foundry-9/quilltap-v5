import { ChangeDetectionStrategy, Component, computed, inject, signal } from '@angular/core';

import { CoreClient } from '../../../../core/core-client';
import type { ProviderInfo } from '../../../../core/core-contract';
import { Icon } from '../../../../ui/icon';
import { createApiKey, testConnection } from '../wizard-api';
import { WizardStore } from '../wizard-state';

interface FormState {
  key: string;
  baseUrl: string;
  validating: boolean;
  validated: boolean;
  error: string | null;
  apiKeyId: string | null;
}

/** Wizard step 2 — enter + validate keys (v4 `steps/ApiKeyEntryStep.tsx`). */
@Component({
  selector: 'qt-wizard-apikey-step',
  changeDetection: ChangeDetectionStrategy.OnPush,
  imports: [Icon],
  template: `
    <div class="space-y-6">
      <div>
        <h2 class="qt-heading-2">Configure API Keys</h2>
        <p class="qt-text-muted mt-1">
          Enter your API keys and validate the connection for each selected provider.
        </p>
      </div>

      <div class="space-y-6">
        @for (p of selectedProviders(); track p.id) {
          <div class="qt-card p-5">
            <div class="flex items-center gap-3 mb-4">
              <span
                class="inline-flex items-center justify-center w-8 h-8 rounded-lg text-xs font-bold qt-bg-muted"
              >
                {{ p.abbreviation }}
              </span>
              <h3 class="font-semibold text-lg">{{ p.displayName }}</h3>
              @if (form(p.id).validated) {
                <qt-icon name="check" class="w-5 h-5 qt-text-success ml-auto" />
              }
            </div>

            <div class="space-y-4">
              @if (p.configRequirements.requiresApiKey) {
                <div>
                  <label class="qt-text-label block mb-1.5">{{
                    p.configRequirements.apiKeyLabel || 'API Key'
                  }}</label>
                  <input
                    type="password"
                    class="qt-input w-full"
                    placeholder="Enter your API key"
                    [disabled]="form(p.id).validating"
                    [value]="form(p.id).key"
                    (input)="
                      patch(p.id, { key: $any($event.target).value, validated: false, error: null })
                    "
                  />
                </div>
              }
              @if (p.configRequirements.requiresBaseUrl) {
                <div>
                  <label class="qt-text-label block mb-1.5">{{
                    p.configRequirements.baseUrlLabel || 'Base URL'
                  }}</label>
                  <input
                    type="text"
                    class="qt-input w-full"
                    [placeholder]="p.configRequirements.baseUrlPlaceholder || 'https://...'"
                    [disabled]="form(p.id).validating"
                    [value]="form(p.id).baseUrl"
                    (input)="
                      patch(p.id, {
                        baseUrl: $any($event.target).value,
                        validated: false,
                        error: null,
                      })
                    "
                  />
                </div>
              }

              @if (form(p.id).error) {
                <div class="qt-alert-error">{{ form(p.id).error }}</div>
              }
              @if (form(p.id).validated) {
                <div class="qt-alert-info">Connection validated successfully.</div>
              }

              <button
                type="button"
                class="qt-button qt-button-primary"
                [disabled]="
                  form(p.id).validating ||
                  (p.configRequirements.requiresApiKey && !form(p.id).key.trim())
                "
                (click)="validate(p)"
              >
                {{
                  form(p.id).validating
                    ? 'Validating...'
                    : form(p.id).validated
                      ? 'Re-validate'
                      : 'Validate'
                }}
              </button>
            </div>
          </div>
        }
      </div>

      @if (selectedProviders().length === 0) {
        <div class="qt-alert-warning">
          No providers selected. Go back and select at least one provider.
        </div>
      }
    </div>
  `,
})
export class ApiKeyEntryStep {
  protected readonly store = inject(WizardStore);
  private readonly core = inject(CoreClient);

  private readonly forms = signal<Record<string, FormState>>({});

  protected readonly selectedProviders = computed(() =>
    this.store
      .selectedProviders()
      .map((id) => this.store.providerById(id))
      .filter((p): p is ProviderInfo => p !== undefined),
  );

  protected form(id: string): FormState {
    const existing = this.forms()[id];
    if (existing) {
      return existing;
    }
    const provider = this.store.providerById(id);
    const seed = this.store.apiKeys()[id];
    return {
      key: seed?.key ?? '',
      baseUrl: this.store.baseUrls()[id] ?? provider?.configRequirements.baseUrlDefault ?? '',
      validating: false,
      validated: seed?.validated ?? false,
      error: seed?.error ?? null,
      apiKeyId: seed?.apiKeyId ?? null,
    };
  }

  protected patch(id: string, patch: Partial<FormState>): void {
    const current = this.form(id);
    this.forms.update((m) => ({ ...m, [id]: { ...current, ...patch } }));
  }

  protected async validate(provider: ProviderInfo): Promise<void> {
    const f = this.form(provider.id);
    this.patch(provider.id, { validating: true, error: null });
    try {
      let apiKeyId = f.apiKeyId;
      const baseUrl = f.baseUrl || undefined;

      if (provider.configRequirements.requiresApiKey) {
        if (!f.key.trim()) {
          this.patch(provider.id, { validating: false, error: 'API key is required' });
          return;
        }
        const label = `${provider.displayName} (Setup Wizard)`;
        const result = await createApiKey(this.core, provider.id, label, f.key.trim());
        apiKeyId = result.id;
      }

      const test = await testConnection(this.core, provider.id, apiKeyId || '', baseUrl);
      if (test.valid) {
        this.patch(provider.id, { validating: false, validated: true, error: null, apiKeyId });
        this.store.setApiKey(provider.id, {
          label: `${provider.displayName} (Setup Wizard)`,
          key: f.key,
          validated: true,
          apiKeyId: apiKeyId || undefined,
        });
        if (baseUrl) {
          this.store.setBaseUrl(provider.id, baseUrl);
        }
      } else {
        this.patch(provider.id, {
          validating: false,
          validated: false,
          error: test.error || 'Connection test failed',
        });
      }
    } catch (err) {
      this.patch(provider.id, {
        validating: false,
        validated: false,
        error: err instanceof Error ? err.message : 'Validation failed',
      });
    }
  }
}
