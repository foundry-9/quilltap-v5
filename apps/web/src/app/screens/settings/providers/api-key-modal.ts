import {
  ChangeDetectionStrategy,
  Component,
  computed,
  inject,
  input,
  output,
  signal,
} from '@angular/core';
import { injectQuery } from '@tanstack/angular-query-experimental';

import { CoreClient } from '../../../core/core-client';
import type { ProviderInfo } from '../../../core/core-contract';
import { ErrorAlert } from '../../../ui/error-alert';
import { FormActions } from '../../../ui/form-actions';
import { Modal } from '../../../ui/modal';
import { ToastService } from '../../../ui/toast.service';
import { providerAcceptsApiKey } from './api-key-support';

interface ProviderOption {
  value: string;
  label: string;
}

/**
 * The "Add New API Key" modal (v4 `components/settings/api-keys/ApiKeyModal.tsx`):
 * a label, a provider dropdown (LLM providers that require a key), and a password
 * field. Emits `saved` on success so the list refetches. Copy is v4-verbatim.
 */
@Component({
  selector: 'qt-api-key-modal',
  changeDetection: ChangeDetectionStrategy.OnPush,
  imports: [Modal, FormActions, ErrorAlert],
  template: `
    <qt-modal title="Add New API Key" (close)="close.emit()">
      @if (error()) {
        <qt-error-alert [message]="error()!" class="mb-4" />
      }
      <div class="space-y-4">
        <div>
          <label for="qt-key-label" class="block qt-text-label mb-2">Label *</label>
          <input
            id="qt-key-label"
            type="text"
            class="qt-input"
            placeholder="e.g., My OpenAI Key"
            [value]="label()"
            (input)="label.set($any($event.target).value)"
          />
          <p class="qt-text-xs mt-1">A friendly name to identify this key</p>
        </div>

        <div>
          <label for="qt-key-provider" class="block qt-text-label mb-2">Provider *</label>
          <!--
            dogfood-#6: options come from an async query, so bind the selection
            per-option with [selected] rather than [value] on the <select> — a
            [value] bound before the options render leaves the native control
            blank (P4.6aa select-audit rider).
          -->
          <select
            id="qt-key-provider"
            class="qt-select"
            [disabled]="providersQuery.isPending()"
            (change)="provider.set($any($event.target).value)"
          >
            @if (providersQuery.isPending()) {
              <option value="">Loading providers...</option>
            } @else {
              <option value="" [selected]="!provider()">Select a provider</option>
              @for (p of providerOptions(); track p.value) {
                <option [value]="p.value" [selected]="provider() === p.value">{{ p.label }}</option>
              }
            }
          </select>
        </div>

        <div>
          <label for="qt-key-value" class="block qt-text-label mb-2">API Key *</label>
          <input
            id="qt-key-value"
            type="password"
            class="qt-input"
            placeholder="Your API key (will be encrypted)"
            [value]="apiKey()"
            (input)="apiKey.set($any($event.target).value)"
          />
          <p class="qt-text-xs mt-1">Your key is encrypted and never exposed</p>
        </div>
      </div>

      <div qt-modal-footer>
        <qt-form-actions
          [submitLabel]="saving() ? 'Creating...' : 'Create API Key'"
          [isLoading]="saving()"
          [isDisabled]="!isValid()"
          (cancel)="close.emit()"
          (submit)="submit()"
        />
      </div>
    </qt-modal>
  `,
})
export class ApiKeyModal {
  private readonly core = inject(CoreClient);
  private readonly toasts = inject(ToastService);

  readonly close = output<void>();
  readonly saved = output<void>();

  protected readonly label = signal('');
  protected readonly provider = signal('');
  protected readonly apiKey = signal('');
  protected readonly saving = signal(false);
  protected readonly error = signal<string | null>(null);

  protected readonly providersQuery = injectQuery(() => ({
    queryKey: ['providers'],
    queryFn: async (): Promise<ProviderInfo[]> => {
      const resp = await this.core.dispatchExpect({ type: 'providerList' }, 'providers');
      return resp.data.providers;
    },
  }));

  /**
   * Offered on "may this provider hold a key?", not "must it?" — the two answers
   * differ for OpenAI-Compatible, whose hosted endpoints need a bearer token its
   * local ones have no use for, and asking the stricter question left no way to
   * create such a key at all (v4 bug 81).
   *
   * **`type` is deliberately NOT filtered** (v4 `ApiKeyModal.tsx:70-71` asks
   * `providerAcceptsApiKey` and nothing else). SEARCH providers hold API keys
   * exactly like LLM providers do — a Serper key created here is the row the
   * `search_web` tool resolves per call — so an added `type === 'llm'` test
   * would make the configured search path unreachable from the UI, which is
   * dogfood finding #98. The list is sorted by displayName, so Serper Web Search
   * falls in alphabetically among the rest, as it does in v4.
   */
  protected readonly providerOptions = computed<ProviderOption[]>(() =>
    (this.providersQuery.data() ?? [])
      .filter((p) => providerAcceptsApiKey(p.configRequirements))
      .map((p) => ({ value: p.name, label: p.displayName }))
      .sort((a, b) => a.label.localeCompare(b.label)),
  );

  protected readonly isValid = computed(
    () =>
      this.label().trim().length > 0 &&
      this.provider().length > 0 &&
      this.apiKey().trim().length > 0,
  );

  protected async submit(): Promise<void> {
    if (!this.isValid()) {
      return;
    }
    this.saving.set(true);
    this.error.set(null);
    try {
      const label = this.label().trim();
      const resp = await this.core.dispatchExpect(
        {
          type: 'apiKeyCreate',
          label,
          provider: this.provider(),
          apiKey: this.apiKey().trim(),
        },
        'apiKey',
      );
      this.saved.emit();
      this.close.emit();
      // v4 `ApiKeyModal.tsx:104-112` — one 4000ms toast per auto-association.
      for (const assoc of resp.data.apiKey.associations ?? []) {
        this.toasts.showSuccess(`${assoc.profileName} linked to API key "${label}"`, 4000);
      }
    } catch (err) {
      this.error.set(err instanceof Error ? err.message : 'Failed to create API key');
    } finally {
      this.saving.set(false);
    }
  }
}
