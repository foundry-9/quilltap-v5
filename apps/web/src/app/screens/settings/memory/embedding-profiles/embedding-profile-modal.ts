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

import { CoreClient, coreErrorMessage } from '../../../../core/core-client';
import type {
  ApiKeyDto,
  EmbeddingModelDto,
  EmbeddingProfileDto,
  EmbeddingProviderInfoDto,
} from '../../../../core/core-contract';
import { notifyQueueChange } from '../../../../layout/queue-status.logic';
import { ErrorAlert } from '../../../../ui/error-alert';
import { FormActions } from '../../../../ui/form-actions';
import { Modal } from '../../../../ui/modal';

/**
 * The create / edit Embedding Profile modal (v4
 * `components/settings/embedding-profiles/ProfileModal.tsx`): Profile Name, a
 * Provider select over the plugin registry, a provider-conditional API Key /
 * Base URL / Model / Dimensions block, and the Default toggle. On a save that
 * NEWLY sets the profile as default, a second "Re-embed Everything?" dialog
 * offers to queue a full reindex before closing.
 *
 * PRESERVED v4 GAP (recorded loud): v4's form type carries
 * `truncateToDimensions` / `normalizeL2`, but the modal renders NO inputs for
 * them and never submits them — the whole Matryoshka-truncation matrix is
 * API-only. This port reproduces that gap faithfully (no truncation inputs); it
 * is a v4-side product item, not a port omission.
 *
 * DEFERRED LOUDLY: the `Fetch Installed Models` button's live path
 * (`embeddingProfileFetchModels` is refusal-armed server-side). The button is
 * kept, v4-faithful; pressing it surfaces the server's refusal sentence rather
 * than hiding the failure.
 */
@Component({
  selector: 'qt-embedding-profile-modal',
  changeDetection: ChangeDetectionStrategy.OnPush,
  imports: [Modal, FormActions, ErrorAlert],
  template: `
    @if (!showReembedDialog()) {
      <qt-modal [title]="title()" (close)="close.emit()">
        @if (error()) {
          <qt-error-alert [message]="error()!" class="mb-4" />
        }

        <div class="space-y-4">
          <div>
            <label class="qt-label mb-1">Profile Name *</label>
            <input
              type="text"
              class="qt-input"
              placeholder="My OpenAI Embeddings"
              [value]="name()"
              (input)="name.set($any($event.target).value)"
            />
          </div>

          <div>
            <label class="qt-label mb-1">Provider *</label>
            <select class="qt-select" [value]="provider()" (change)="onProviderChange($any($event.target).value)">
              @for (p of embeddingProviders(); track p.name) {
                <option [value]="p.name" [selected]="provider() === p.name">{{ p.displayName }}</option>
              }
            </select>
            @if (currentProviderInfo()?.description) {
              <p class="mt-1 qt-text-xs qt-text-warning">{{ currentProviderInfo()!.description }}</p>
            }
          </div>

          @if (needsApiKey()) {
            <div>
              <label class="qt-label mb-1">API Key</label>
              <select class="qt-select" [value]="apiKeyId()" (change)="apiKeyId.set($any($event.target).value)">
                <option value="" [selected]="apiKeyId() === ''">Select API Key...</option>
                @for (key of filteredApiKeys(); track key.id) {
                  <option [value]="key.id" [selected]="apiKeyId() === key.id">{{ key.label }}</option>
                }
              </select>
              @if (filteredApiKeys().length === 0) {
                <p class="mt-1 qt-text-xs qt-text-secondary">
                  No {{ provider() }} API keys found. Add one in the API Keys tab first.
                </p>
              }
            </div>
          }

          @if (needsBaseUrl()) {
            <div>
              <label class="qt-label mb-1">Base URL</label>
              <input
                type="text"
                class="qt-input"
                placeholder="http://localhost:11434"
                [value]="baseUrl()"
                (input)="baseUrl.set($any($event.target).value)"
              />
              <p class="mt-1 qt-text-xs">Leave empty for default URL</p>
            </div>
          }

          @if (!isBuiltin()) {
            <div>
              <div class="flex items-center justify-between mb-1">
                <label class="qt-label">Model *</label>
                @if (canFetchModels()) {
                  <button
                    type="button"
                    class="qt-button-ghost qt-button-sm"
                    [disabled]="fetchingModels()"
                    (click)="fetchModels()"
                  >
                    {{ fetchingModels() ? 'Fetching...' : 'Fetch Installed Models' }}
                  </button>
                }
              </div>
              @if (currentModels().length > 0) {
                <div class="space-y-2">
                  <select class="qt-select" [value]="modelName()" (change)="onModelSelect($any($event.target).value)">
                    <option value="" [selected]="modelName() === ''">Select a model...</option>
                    @for (model of currentModels(); track model.id) {
                      <option [value]="model.id" [selected]="modelName() === model.id">
                        {{ model.name }}{{ model.dimensions ? ' (' + model.dimensions + ' dims)' : '' }}
                      </option>
                    }
                  </select>
                  @if (selectedModelDescription()) {
                    <p class="qt-text-xs">{{ selectedModelDescription() }}</p>
                  }
                </div>
              } @else {
                <input
                  type="text"
                  class="qt-input"
                  [placeholder]="canFetchModels() ? 'nomic-embed-text' : 'text-embedding-3-small'"
                  [value]="modelName()"
                  (input)="modelName.set($any($event.target).value)"
                />
              }
              @if (fetchModelsError()) {
                <p class="mt-1 qt-text-xs qt-text-destructive">{{ fetchModelsError() }}</p>
              }
            </div>

            <div>
              <label class="qt-label mb-1">Dimensions (optional)</label>
              <input
                type="text"
                class="qt-input"
                placeholder="1536"
                [value]="dimensions()"
                (input)="dimensions.set($any($event.target).value)"
              />
              <p class="mt-1 qt-text-xs">Leave empty to use the model&apos;s default dimensions</p>
            </div>
          }

          <div class="flex items-center">
            <input
              type="checkbox"
              id="qt-emb-isDefault"
              class="h-4 w-4 rounded"
              [checked]="isDefault()"
              (change)="isDefault.set($any($event.target).checked)"
            />
            <label for="qt-emb-isDefault" class="ml-2 block qt-text-label text-foreground">
              Set as default embedding profile
            </label>
          </div>
        </div>

        <div qt-modal-footer>
          <qt-form-actions
            [submitLabel]="saving() ? submitLabel() + '…' : submitLabel()"
            [isLoading]="saving()"
            [isDisabled]="!isValid()"
            (cancel)="close.emit()"
            (submit)="submit()"
          />
        </div>
      </qt-modal>
    } @else {
      <qt-modal title="Re-embed Everything?" maxWidth="md" (close)="declineReembed()">
        <p class="text-foreground">
          You&apos;ve switched the default embedding profile. All existing embeddings were created
          with a different model and need to be regenerated — this includes help documentation,
          character memories, and conversation history.
        </p>
        <p class="mt-3 qt-text-secondary qt-text-small">
          Help documentation will be embedded first, followed by character memories and conversation
          history. You can track progress via the Emb badge in the header.
        </p>

        <div qt-modal-footer>
          <div class="flex gap-2 justify-end">
            <button
              type="button"
              class="qt-button-secondary"
              [disabled]="reembedLoading()"
              (click)="declineReembed()"
            >
              Skip
            </button>
            <button
              type="button"
              class="qt-button-primary"
              [disabled]="reembedLoading()"
              (click)="confirmReembed()"
            >
              {{ reembedLoading() ? 'Queuing…' : 'Re-embed Everything' }}
            </button>
          </div>
        </div>
      </qt-modal>
    }
  `,
})
export class EmbeddingProfileModal implements OnInit {
  private readonly core = inject(CoreClient);

  readonly profile = input<EmbeddingProfileDto | null>(null);
  readonly apiKeys = input<ApiKeyDto[]>([]);
  readonly embeddingModels = input<Record<string, EmbeddingModelDto[]>>({});
  readonly embeddingProviders = input<EmbeddingProviderInfoDto[]>([]);
  /** Refetch the profile list (v4 `onSuccess`). Fired without dismissing. */
  readonly saved = output<void>();
  /** Dismiss the modal (v4 `onClose`). */
  readonly close = output<void>();

  // --- Form state (v4 `useFormState`, one signal per field) ---
  protected readonly name = signal('');
  protected readonly provider = signal('BUILTIN');
  protected readonly apiKeyId = signal('');
  protected readonly baseUrl = signal('');
  protected readonly modelName = signal('');
  /** String field (v4 keeps the dimensions input as text). */
  protected readonly dimensions = signal('');
  protected readonly isDefault = signal(false);

  protected readonly saving = signal(false);
  protected readonly error = signal<string | null>(null);
  protected readonly fetchedModels = signal<EmbeddingModelDto[]>([]);
  protected readonly fetchingModels = signal(false);
  protected readonly fetchModelsError = signal<string | null>(null);

  protected readonly showReembedDialog = signal(false);
  protected readonly reembedLoading = signal(false);
  private reembedProfileId: string | null = null;

  protected readonly title = computed(() =>
    this.profile()?.id ? 'Edit Embedding Profile' : 'Create Embedding Profile',
  );
  protected readonly submitLabel = computed(() =>
    this.profile()?.id ? 'Update Profile' : 'Create Profile',
  );

  protected readonly currentProviderInfo = computed(() =>
    this.embeddingProviders().find((p) => p.name === this.provider()),
  );
  protected readonly isBuiltin = computed(() => this.provider() === 'BUILTIN');
  protected readonly needsApiKey = computed(() => this.currentProviderInfo()?.requiresApiKey ?? false);
  protected readonly needsBaseUrl = computed(() => this.currentProviderInfo()?.requiresBaseUrl ?? false);
  /** Ollama-style providers (base-URL bearing) can fetch installed models. */
  protected readonly canFetchModels = computed(() => this.needsBaseUrl());

  protected readonly filteredApiKeys = computed(() =>
    this.apiKeys().filter((key) => key.provider === this.provider()),
  );

  private readonly staticModels = computed(() => this.embeddingModels()[this.provider()] ?? []);

  /** Merge fetched (installed) models ahead of static suggestions (v4 :187-193). */
  protected readonly currentModels = computed<EmbeddingModelDto[]>(() => {
    const fetched = this.fetchedModels();
    if (fetched.length === 0) return this.staticModels();
    const fetchedIds = new Set(fetched.map((f) => f.id));
    return [...fetched, ...this.staticModels().filter((s) => !fetchedIds.has(s.id))];
  });

  protected readonly selectedModelDescription = computed(
    () => this.currentModels().find((m) => m.id === this.modelName())?.description ?? '',
  );

  /** v4 :196 — name required; a model required for every non-BUILTIN provider. */
  protected readonly isValid = computed(
    () => this.name().trim().length > 0 && (this.isBuiltin() || this.modelName().trim().length > 0),
  );

  ngOnInit(): void {
    const p = this.profile();
    // Default to the first available provider, else BUILTIN (v4 :47).
    const defaultProvider = this.embeddingProviders()[0]?.name ?? 'BUILTIN';
    this.name.set(p?.name ?? '');
    this.provider.set(p?.provider ?? defaultProvider);
    this.apiKeyId.set(p?.apiKeyId ?? '');
    this.baseUrl.set(p?.baseUrl ?? '');
    this.modelName.set(p?.modelName ?? '');
    this.dimensions.set(p?.dimensions != null ? String(p.dimensions) : '');
    this.isDefault.set(p?.isDefault ?? false);
  }

  /** v4 `handleModelSelect`: pick + auto-fill dimensions from the model. */
  protected onModelSelect(modelId: string): void {
    this.modelName.set(modelId);
    const dims = this.currentModels().find((m) => m.id === modelId)?.dimensions;
    if (dims) {
      this.dimensions.set(String(dims));
    }
  }

  /** v4 `handleProviderChange`: reset key/model/dimensions and drop fetched models. */
  protected onProviderChange(value: string): void {
    this.provider.set(value);
    this.apiKeyId.set('');
    this.modelName.set('');
    this.dimensions.set('');
    this.fetchedModels.set([]);
    this.fetchModelsError.set(null);
  }

  protected async fetchModels(): Promise<void> {
    this.fetchingModels.set(true);
    this.fetchModelsError.set(null);
    try {
      const models = await this.core.fetchEmbeddingModels(
        this.provider(),
        this.baseUrl() || undefined,
      );
      this.fetchedModels.set(models);
    } catch (err) {
      // The live path is refusal-armed; surface the server's own sentence
      // rather than hiding the button's failure (v4-faithful honesty).
      this.fetchModelsError.set(coreErrorMessage(err, 'Failed to fetch installed models'));
    } finally {
      this.fetchingModels.set(false);
    }
  }

  protected async submit(): Promise<void> {
    if (!this.isValid()) return;
    // Track whether this save is NEWLY setting the profile as default (v4 :74).
    const isNewlyDefault = !this.profile()?.isDefault && this.isDefault();

    this.saving.set(true);
    this.error.set(null);
    try {
      // BUILTIN pins the model name (v4 :78-80).
      const modelName = this.isBuiltin() ? 'tfidf-bm25-v1' : this.modelName();
      const dims = this.dimensions().trim() ? Number.parseInt(this.dimensions(), 10) : undefined;

      const id = this.profile()?.id;
      let saved: EmbeddingProfileDto;
      if (id) {
        saved = await this.core.updateEmbeddingProfile({
          profileId: id,
          name: this.name(),
          provider: this.provider(),
          apiKeyId: this.apiKeyId() || undefined,
          baseUrl: this.baseUrl() || undefined,
          modelName,
          dimensions: dims,
          isDefault: this.isDefault(),
        });
      } else {
        saved = await this.core.createEmbeddingProfile({
          name: this.name(),
          provider: this.provider(),
          apiKeyId: this.apiKeyId() || undefined,
          baseUrl: this.baseUrl() || undefined,
          modelName,
          dimensions: dims,
          isDefault: this.isDefault(),
        });
      }

      this.saved.emit();

      // If newly set as default, offer the full re-embed before dismissing.
      if (isNewlyDefault && saved?.id) {
        this.reembedProfileId = saved.id;
        this.showReembedDialog.set(true);
      } else {
        this.close.emit();
      }
    } catch (err) {
      this.error.set(coreErrorMessage(err, 'Failed to save profile'));
    } finally {
      this.saving.set(false);
    }
  }

  protected async confirmReembed(): Promise<void> {
    if (!this.reembedProfileId) return;
    this.reembedLoading.set(true);
    try {
      // v4 :121-123 — the legacy no-body reindex call (scope absent = 'all').
      await this.core.reindexEmbeddingProfile(this.reembedProfileId);
      notifyQueueChange();
    } finally {
      this.reembedLoading.set(false);
      this.showReembedDialog.set(false);
      this.reembedProfileId = null;
      this.close.emit();
    }
  }

  protected declineReembed(): void {
    this.showReembedDialog.set(false);
    this.reembedProfileId = null;
    this.close.emit();
  }
}
