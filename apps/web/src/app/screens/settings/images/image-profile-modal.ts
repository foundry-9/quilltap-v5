import {
  ChangeDetectionStrategy,
  Component,
  OnInit,
  computed,
  inject,
  input,
  output,
  signal,
} from '@angular/core';
import { injectQuery } from '@tanstack/angular-query-experimental';

import { CoreClient } from '../../../core/core-client';
import type { ApiKeyDto, ImageProfileDto, ImageProviderInfo } from '../../../core/core-contract';
import { ErrorAlert } from '../../../ui/error-alert';
import { FormActions } from '../../../ui/form-actions';
import { Modal } from '../../../ui/modal';
import {
  availableApiKeys,
  defaultModelsFor,
  emptyImageProfileForm,
  FALLBACK_PROVIDERS,
  imageProfileToForm,
} from './image-profile-form';
import { createImageProfile, fetchImageProviders, updateImageProfile } from './image-profiles.api';

/**
 * The create / edit Image Profile modal (v4 `components/image-profiles/
 * ImageProfileForm.tsx` in `ImageProfileModal`): Profile Name, a Provider select
 * over the live registry (falling back to `FALLBACK_PROVIDERS`), an API Key
 * select filtered by provider, a Model select over the provider's default
 * models, a Parameters JSON textarea, and the isDefault / isDangerousCompatible
 * toggles. Duplicate-name (409) and `Provider … is not available` (400) surface
 * verbatim. The server enforces the single-default (unsets the others).
 *
 * DEFERRED LOUDLY this round: the `Validate` key button (its wire pair,
 * `imageProfileValidateKey`, is refusal-armed) renders disabled-with-title; the
 * Model list is populated from the provider `defaultModels` only (live
 * `imageProfileListModels` is likewise refusal-armed). The structured
 * per-provider parameters editor is a named deferral — a JSON textarea stands in
 * (key order preserved).
 */
@Component({
  selector: 'qt-image-profile-modal',
  changeDetection: ChangeDetectionStrategy.OnPush,
  imports: [Modal, FormActions, ErrorAlert],
  template: `
    <qt-modal [title]="title()" maxWidth="2xl" (close)="close.emit()">
      @if (error()) {
        <qt-error-alert [message]="error()!" class="mb-4" />
      }

      <div class="space-y-4">
        <div>
          <label class="block qt-text-label mb-1">Profile Name</label>
          <input
            type="text"
            class="qt-input"
            placeholder="e.g., DALL-E 3 HD"
            [value]="name()"
            (input)="name.set($any($event.target).value)"
          />
        </div>

        <div>
          <label class="block qt-text-label mb-1">Provider</label>
          <select
            class="qt-select"
            [disabled]="providersQuery.isPending()"
            [value]="provider()"
            (change)="onProviderChange($any($event.target).value)"
          >
            @for (p of providers(); track p.value) {
              <option [value]="p.value">{{ p.label }}</option>
            }
          </select>
        </div>

        <div>
          <label class="block qt-text-label mb-1">API Key</label>
          <div class="flex gap-2">
            <select
              class="qt-select flex-1"
              [value]="apiKeyId()"
              (change)="apiKeyId.set($any($event.target).value)"
            >
              <option value="" [selected]="apiKeyId() === ''">Select an API key</option>
              @for (k of eligibleApiKeys(); track k.id) {
                <option [value]="k.id" [selected]="apiKeyId() === k.id">
                  {{ k.label }} ({{ k.provider }})
                </option>
              }
            </select>
            <button
              type="button"
              class="qt-button-primary qt-button-sm flex-shrink-0"
              title="Key validation is not yet available"
              disabled
            >
              Validate
            </button>
          </div>
        </div>

        <div>
          <label class="block qt-text-label mb-1">Model</label>
          <select
            class="qt-select"
            [value]="modelName()"
            (change)="modelName.set($any($event.target).value)"
          >
            @for (m of models(); track m) {
              <option [value]="m" [selected]="modelName() === m">{{ m }}</option>
            }
            @if (models().length === 0) {
              <option [value]="modelName()">{{ modelName() || 'No models available' }}</option>
            }
          </select>
        </div>

        <div>
          <label class="block qt-text-label mb-1">Parameters (JSON)</label>
          <textarea
            class="qt-input font-mono"
            rows="4"
            placeholder="{}"
            [value]="parametersText()"
            (input)="parametersText.set($any($event.target).value)"
          ></textarea>
          @if (parametersError()) {
            <p class="qt-text-xs qt-text-destructive mt-1">{{ parametersError() }}</p>
          }
        </div>

        <label class="flex items-center gap-2 qt-text-small">
          <input
            type="checkbox"
            class="h-4 w-4 rounded"
            [checked]="isDefault()"
            (change)="isDefault.set($any($event.target).checked)"
          />
          Set as default profile for image generation
        </label>

        <label class="flex items-center gap-2 qt-text-small">
          <input
            type="checkbox"
            class="h-4 w-4 rounded"
            [checked]="isDangerousCompatible()"
            (change)="isDangerousCompatible.set($any($event.target).checked)"
          />
          Uncensored-compatible (suitable for dangerous/sensitive content routing)
        </label>
      </div>

      <div qt-modal-footer>
        <qt-form-actions
          [submitLabel]="saving() ? 'Saving...' : submitLabel()"
          [isLoading]="saving()"
          [isDisabled]="!isValid()"
          (cancel)="close.emit()"
          (submit)="submit()"
        />
      </div>
    </qt-modal>
  `,
})
export class ImageProfileModal implements OnInit {
  private readonly core = inject(CoreClient);

  readonly profile = input<ImageProfileDto | null>(null);
  readonly close = output<void>();
  readonly saved = output<void>();

  protected readonly name = signal('');
  protected readonly provider = signal('OPENAI');
  protected readonly apiKeyId = signal('');
  protected readonly modelName = signal('dall-e-3');
  protected readonly parametersText = signal('{}');
  protected readonly isDefault = signal(false);
  protected readonly isDangerousCompatible = signal(false);

  protected readonly saving = signal(false);
  protected readonly error = signal<string | null>(null);
  protected readonly parametersError = signal<string | null>(null);

  protected readonly providersQuery = injectQuery(() => ({
    queryKey: ['image-profiles', 'providers'],
    queryFn: async (): Promise<ImageProviderInfo[]> => {
      const list = await fetchImageProviders(this.core);
      return list.length > 0 ? list : FALLBACK_PROVIDERS;
    },
  }));

  private readonly apiKeysQuery = injectQuery(() => ({
    queryKey: ['apiKeys'],
    queryFn: async (): Promise<ApiKeyDto[]> => {
      const resp = await this.core.dispatchExpect({ type: 'apiKeyList' }, 'apiKeys');
      return resp.data.apiKeys;
    },
  }));

  protected readonly providers = computed(() => this.providersQuery.data() ?? FALLBACK_PROVIDERS);

  protected readonly eligibleApiKeys = computed(() =>
    availableApiKeys(this.apiKeysQuery.data() ?? [], this.provider(), this.providers()),
  );

  protected readonly models = computed(() => defaultModelsFor(this.provider(), this.providers()));

  private readonly editingId = computed(() => this.profile()?.id ?? null);
  protected readonly title = computed(() =>
    this.editingId() ? 'Edit Image Profile' : 'New Image Profile',
  );
  protected readonly submitLabel = computed(() => (this.editingId() ? 'Update' : 'Create'));

  protected readonly isValid = computed(
    () =>
      this.name().trim().length > 0 &&
      this.provider().length > 0 &&
      this.modelName().trim().length > 0 &&
      this.apiKeyId().length > 0,
  );

  ngOnInit(): void {
    const p = this.profile();
    const form = p ? imageProfileToForm(p) : emptyImageProfileForm();
    this.name.set(form.name);
    this.provider.set(form.provider);
    this.apiKeyId.set(form.apiKeyId);
    this.modelName.set(form.modelName);
    this.parametersText.set(JSON.stringify(form.parameters ?? {}, null, 2));
    this.isDefault.set(form.isDefault);
    this.isDangerousCompatible.set(form.isDangerousCompatible);
  }

  /** v4 `handleProviderChange`: reset model to the provider's first default,
   *  clear the API key, and reset parameters. */
  protected onProviderChange(value: string): void {
    this.provider.set(value);
    this.modelName.set(defaultModelsFor(value, this.providers())[0] ?? '');
    this.apiKeyId.set('');
    this.parametersText.set('{}');
    this.parametersError.set(null);
  }

  private parseParameters(): Record<string, unknown> | null {
    const text = this.parametersText().trim();
    if (!text) {
      return {};
    }
    try {
      const parsed = JSON.parse(text) as unknown;
      if (typeof parsed !== 'object' || parsed === null || Array.isArray(parsed)) {
        this.parametersError.set('Parameters must be a JSON object');
        return null;
      }
      return parsed as Record<string, unknown>;
    } catch {
      this.parametersError.set('Parameters must be valid JSON');
      return null;
    }
  }

  protected async submit(): Promise<void> {
    if (!this.isValid()) {
      return;
    }
    const parameters = this.parseParameters();
    if (parameters === null) {
      return;
    }
    this.parametersError.set(null);
    this.saving.set(true);
    this.error.set(null);
    const body = {
      name: this.name().trim(),
      provider: this.provider(),
      apiKeyId: this.apiKeyId() || null,
      baseUrl: null,
      modelName: this.modelName(),
      parameters,
      isDefault: this.isDefault(),
      isDangerousCompatible: this.isDangerousCompatible(),
    };
    try {
      const id = this.editingId();
      if (id) {
        await updateImageProfile(this.core, id, body);
      } else {
        await createImageProfile(this.core, body);
      }
      this.saved.emit();
      this.close.emit();
    } catch (err) {
      this.error.set(
        err instanceof Error && err.message ? err.message : 'Failed to save image profile',
      );
    } finally {
      this.saving.set(false);
    }
  }
}
