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
import { Icon } from '../../../../ui/icon';
import { fetchProviders } from '../wizard-api';
import { WizardStore } from '../wizard-state';

const CAPABILITY_LABELS: Array<{ key: keyof ProviderInfo['capabilities']; label: string }> = [
  { key: 'chat', label: 'Chat' },
  { key: 'embeddings', label: 'Embeddings' },
  { key: 'imageGeneration', label: 'Images' },
  { key: 'webSearch', label: 'Web Search' },
];

/** Wizard step 1 — pick providers (v4 `steps/ProviderSelectionStep.tsx`). */
@Component({
  selector: 'qt-wizard-provider-step',
  changeDetection: ChangeDetectionStrategy.OnPush,
  imports: [Icon],
  template: `
    @if (loading()) {
      <div class="flex items-center justify-center py-12">
        <div class="qt-spinner qt-spinner-sm mr-3"></div>
        <span class="qt-text-muted">Loading available providers...</span>
      </div>
    } @else if (error()) {
      <div class="qt-alert-error">
        <p>{{ error() }}</p>
        <button class="qt-button qt-button-secondary mt-2" (click)="load()">Retry</button>
      </div>
    } @else {
      <div class="space-y-6">
        <div>
          <h2 class="qt-heading-2">Select Your Providers</h2>
          <p class="qt-text-muted mt-1">
            Choose one or more AI providers to connect. You must select at least one provider with
            chat capability to continue.
          </p>
        </div>

        <div class="grid grid-cols-1 md:grid-cols-2 gap-4">
          @for (p of store.availableProviders(); track p.id) {
            <button
              type="button"
              [class]="
                'w-full p-4 text-left rounded-lg transition-all ' +
                (isSelected(p.id) ? 'qt-option-selected' : 'qt-option-unselected')
              "
              (click)="store.toggleProvider(p.id)"
            >
              <div class="flex items-start gap-3">
                <span
                  class="inline-flex items-center justify-center w-10 h-10 rounded-lg text-sm font-bold shrink-0 qt-bg-muted"
                >
                  {{ p.abbreviation }}
                </span>
                <div class="flex-1 min-w-0">
                  <div class="flex items-center gap-2">
                    <span class="font-semibold">{{ p.displayName }}</span>
                    @if (isSelected(p.id)) {
                      <qt-icon name="check" class="w-5 h-5 qt-text-primary" />
                    }
                  </div>
                  <p class="qt-text-muted qt-text-small mt-1">{{ p.description }}</p>
                  <div class="flex flex-wrap gap-1.5 mt-2">
                    @for (cap of capabilitiesOf(p); track cap) {
                      <span
                        class="qt-text-xs px-2 py-0.5 rounded-full qt-bg-muted qt-text-primary"
                        >{{ cap }}</span
                      >
                    }
                  </div>
                </div>
              </div>
            </button>
          }
        </div>

        @if (store.selectedProviders().length > 0 && !store.hasValidChatProvider()) {
          <div class="qt-alert-warning">
            None of your selected providers support chat. Please select at least one chat-capable
            provider to continue.
          </div>
        }
        @if (store.selectedProviders().length === 0) {
          <p class="qt-text-muted qt-text-small text-center">
            Select at least one provider to get started.
          </p>
        }
      </div>
    }
  `,
})
export class ProviderSelectionStep implements OnInit {
  protected readonly store = inject(WizardStore);
  private readonly core = inject(CoreClient);

  protected readonly loading = signal(false);
  protected readonly error = signal<string | null>(null);

  protected readonly hasProviders = computed(() => this.store.availableProviders().length > 0);

  ngOnInit(): void {
    if (!this.hasProviders()) {
      void this.load();
    }
  }

  protected isSelected(id: string): boolean {
    return this.store.selectedProviders().includes(id);
  }

  protected capabilitiesOf(p: ProviderInfo): string[] {
    return CAPABILITY_LABELS.filter((c) => p.capabilities[c.key]).map((c) => c.label);
  }

  protected async load(): Promise<void> {
    this.loading.set(true);
    this.error.set(null);
    try {
      const providers = await fetchProviders(this.core);
      this.store.availableProviders.set(providers);
    } catch (err) {
      this.error.set(err instanceof Error ? err.message : 'Failed to load providers');
    } finally {
      this.loading.set(false);
    }
  }
}
