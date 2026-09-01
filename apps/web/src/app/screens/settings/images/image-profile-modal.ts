import {
  ChangeDetectionStrategy,
  Component,
  ElementRef,
  OnInit,
  afterRenderEffect,
  computed,
  effect,
  inject,
  input,
  output,
  signal,
  untracked,
  viewChild,
} from '@angular/core';
import { injectQuery } from '@tanstack/angular-query-experimental';

import { CoreClient } from '../../../core/core-client';
import type {
  ApiKeyDto,
  ImageLoraSpec,
  ImageLoraSupport,
  ImageProfileDto,
  ImageProviderInfo,
} from '../../../core/core-contract';
import { ErrorAlert } from '../../../ui/error-alert';
import { FormActions } from '../../../ui/form-actions';
import { Modal } from '../../../ui/modal';
import { ProviderOptionsPanel } from '../providers/provider-options-panel';
import type { ProviderOptionsSchema } from '../providers/provider-options-schema';
import { LoraListEditor } from './lora-list-editor';
import {
  availableApiKeys,
  defaultModelsFor,
  emptyImageProfileForm,
  FALLBACK_PROVIDERS,
  imageProfileToForm,
  normalizeProviderName,
  PROVIDER_SIZE_PANELS,
} from './image-profile-form';
import {
  createImageProfile,
  fetchImageModels,
  fetchImageOptionsSchema,
  fetchImageProviders,
  updateImageProfile,
} from './image-profiles.api';

/**
 * The create / edit Image Profile modal (v4 `components/image-profiles/
 * ImageProfileForm.tsx` in `ImageProfileModal`): Profile Name, a Provider select
 * over the live registry (falling back to `FALLBACK_PROVIDERS`), an API Key
 * select filtered by provider, a Model select over the provider's default
 * models, a Parameters JSON textarea, and the isDefault / isDangerousCompatible
 * toggles. Duplicate-name (409) and `Provider … is not available` (400) surface
 * verbatim. The server enforces the single-default (unsets the others).
 *
 * P4.D102 CLOSED the model-list deferral: the Model row is now v4's honest
 * Fetch Models control (`ImageProfileForm.tsx:135-175,390-444`) — an auto-load
 * on provider/key change, an explicit button, and the source label beneath.
 *
 * P4.D102 also landed the structured parameters editor's FIRST TWO cases —
 * v4's `Z_AI` and `NANOGPT` Default Size panels
 * (`ImageProfileParameters.tsx:126-181`), the two the round's drift commits
 * added.
 *
 * P4.D139 (v4 `84f33ce94`) put the SCHEMA-DRIVEN panel in front of both: a
 * provider whose plugin declares an image options schema now gets the shared,
 * model-aware `ProviderOptionsPanel` — the same renderer the connection-profile
 * editor uses — and the hand-written arms below it become the fallback for
 * plugins that have not adopted the hook yet, and for a failed schema fetch.
 * v4's own comment gives the reason to keep them: a provider whose editor
 * offers nothing at all would be worse than a slightly stale size list.
 *
 * Note the shape of that fallback differs between the apps and the difference
 * is v5's, not v4's: v4's legacy arm is `ImageProfileParameters` alone, whose
 * `default:` case renders NOTHING; v5's is the size panel OR the JSON
 * textarea, the textarea being a v5 invention. The schema arm is identical.
 *
 * STILL DEFERRED LOUDLY: the `Validate` key button (its wire pair,
 * `imageProfileValidateKey`, is refusal-armed) renders disabled-with-title —
 * v4 did not move it this round. And v4's OTHER structured cases are still
 * unported: `OPENAI` (`:28-83` — Quality, Style, Size, Response Format),
 * `GOOGLE`/`GOOGLE_IMAGEN` (`:84-125` — Aspect Ratio, Person Generation,
 * Sample Count) and `GROK` (`:183-192` — a static "minimal parameters"
 * paragraph). Those four providers get the schema panel when their plugin
 * declares one and v5's JSON textarea stand-in otherwise. Unknown keys survive
 * editing on both sides.
 */
@Component({
  selector: 'qt-image-profile-modal',
  changeDetection: ChangeDetectionStrategy.OnPush,
  imports: [Modal, FormActions, ErrorAlert, ProviderOptionsPanel, LoraListEditor],
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
          <div class="flex gap-2">
            <select
              #modelSelect
              class="qt-select flex-1"
              [disabled]="isFetchingModels()"
              [attr.data-qt-value]="modelName()"
              (change)="modelName.set($any($event.target).value)"
            >
              @for (m of modelOptions(); track m) {
                <option [value]="m">{{ m }}</option>
              }
            </select>
            <button
              type="button"
              class="qt-button-primary qt-button-sm flex-shrink-0"
              [disabled]="!apiKeyId() || isFetchingModels()"
              [title]="
                apiKeyId() ? 'Query the provider for its image models' : 'Select an API key first'
              "
              (click)="fetchModels()"
            >
              {{ isFetchingModels() ? 'Fetching...' : 'Fetch Models' }}
            </button>
          </div>
          @if (!isFetchingModels() && modelsSource() === 'provider') {
            <p class="qt-text-success text-sm mt-1">{{ providerSourceLabel() }}</p>
          }
          @if (!isFetchingModels() && modelsSource() === 'builtin') {
            <p class="qt-text-xs mt-1">{{ builtinSourceLabel() }}</p>
          }
        </div>

        <!-- Provider-Specific Parameters.
             A plugin that declares an image options schema gets the shared,
             model-aware renderer — the same one the connection-profile editor
             uses. The hand-written arms below are only for providers whose
             plugins have not adopted the hook yet, and for the case where the
             schema fetch itself fails: a provider whose editor offers nothing
             at all would be worse than a slightly stale size list. -->
        @if (optionsSchema(); as schema) {
          <qt-provider-options-panel
            [schema]="schema"
            [parameters]="parametersBag()"
            [fetchedModels]="availableModels()"
            [modelName]="modelName()"
            (setParameter)="setParameter($event)"
          />
        } @else if (sizePanel(); as panel) {
          <div class="space-y-4 border-t qt-border-default pt-4">
            <h3 class="text-sm qt-text-primary">Image Parameters (Optional)</h3>
            <div>
              <label class="block qt-text-label mb-1">Default Size</label>
              <select
                #sizeSelect
                class="qt-select"
                [attr.data-qt-value]="sizeValue()"
                (change)="setSize($any($event.target).value)"
              >
                @for (option of panel.options; track option.value) {
                  <option [value]="option.value">{{ option.label }}</option>
                }
              </select>
              <p class="qt-text-xs mt-1">{{ panel.footnote }}</p>
            </div>
          </div>
        } @else {
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
        }

        <!-- LoRA adapters — shown only when this provider/model declares support -->
        <qt-lora-list-editor
          [support]="loraSupport()"
          [loras]="currentLoras()"
          [hfToken]="hfToken()"
          (lorasChange)="setLoras($event)"
        />

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

  /** v4 `:92-93,90-91` — the fetched list and where it came from. */
  protected readonly availableModels = signal<string[]>([]);
  protected readonly modelsSource = signal<'provider' | 'builtin' | null>(null);
  protected readonly modelsFetchError = signal<string | null>(null);
  protected readonly isFetchingModels = signal(false);

  protected readonly saving = signal(false);
  protected readonly error = signal<string | null>(null);
  protected readonly parametersError = signal<string | null>(null);

  /**
   * Per-model options schema and LoRA support, resolved server-side (v4
   * `ImageProfileForm.tsx:101-111`). The schema is the plugin's answer for
   * *this* model — image gateways route to hundreds of models with different
   * legal sizes — so both refetch whenever the provider or the model changes.
   */
  protected readonly optionsSchema = signal<ProviderOptionsSchema | null>(null);
  protected readonly loraSupport = signal<ImageLoraSupport | null>(null);
  /**
   * Bumped whenever a live model fetch succeeds. A plugin may build its
   * options schema from a catalog it can only load with an API key, so the
   * first schema fetch on a cold cache gets the generic answer; this makes
   * the editor ask again once that catalog exists.
   */
  private readonly catalogVersion = signal(0);

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

  /**
   * The NORMALIZED provider key — v4 `ImageProfileForm.tsx:194,231`'s
   * `providerKey`, the string its options-schema effect depends on. A string
   * computed notifies only when its VALUE changes, so the providers query
   * settling (a new list object, same normalized name) does not refire the
   * fetch — reading `providers()` directly inside the effect did (the
   * unification review's catch: two identical dispatches per modal open).
   */
  private readonly providerKey = computed(() => normalizeProviderName(this.provider(), this.providers()));

  protected readonly eligibleApiKeys = computed(() =>
    availableApiKeys(this.apiKeysQuery.data() ?? [], this.provider(), this.providers()),
  );

  /**
   * v4 `:400-413` — the option list. `listed` is the fetched list when there is
   * one, else the provider registry's `defaultModels`; note v4 looks that up by
   * the RAW `formData.provider` (not the normalized name), so a legacy value
   * yields an empty list until the normalize effect below rewrites it. A saved
   * `modelName` the list omits is PREPENDED so it stays selectable.
   */
  protected readonly modelOptions = computed(() => {
    const fetched = this.availableModels();
    const listed =
      fetched.length > 0
        ? fetched
        : (this.providers().find((p) => p.value === this.provider())?.defaultModels ?? []);
    const name = this.modelName();
    return name && !listed.includes(name) ? [name, ...listed] : listed;
  });

  /** v4 `:427-431`, the ✓ tally — singular/plural on the FETCHED count. */
  protected readonly providerSourceLabel = computed(() => {
    const n = this.availableModels().length;
    return `✓ ${n} image ${n === 1 ? 'model' : 'models'} fetched from the provider`;
  });

  /**
   * v4 `:432-440`, the three-way built-in sentence. The `Couldn't` arm fires
   * only when the SERVER answered ok and named a live-fetch failure; a hard
   * request failure leaves `modelsFetchError` null and reads as one of the
   * other two, exactly as v4's fallback branches do.
   */
  protected readonly builtinSourceLabel = computed(() => {
    const err = this.modelsFetchError();
    if (err) {
      return `Couldn't fetch from the provider (${err}) — showing the plugin's built-in list.`;
    }
    return this.apiKeyId()
      ? "Showing the plugin's built-in model list."
      : "Showing the plugin's built-in model list — select an API key and Fetch Models to query the provider.";
  });

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

  private readonly modelSelect = viewChild<ElementRef<HTMLSelectElement>>('modelSelect');
  private readonly sizeSelect = viewChild<ElementRef<HTMLSelectElement>>('sizeSelect');

  /**
   * The structured Default Size panel for this provider, or null to fall back
   * to the JSON textarea (v4 `ImageProfileParameters.tsx` switches on the RAW
   * provider, so `GOOGLE_IMAGEN` gets its own case there rather than GOOGLE's).
   */
  protected readonly sizePanel = computed(() => PROVIDER_SIZE_PANELS[this.provider()] ?? null);

  /** v4 `:137` / `:166` — `parameters.size || '1024x1024'`. */
  protected readonly sizeValue = computed(() => {
    const stored = this.parametersBag()['size'];
    return typeof stored === 'string' && stored ? stored : '1024x1024';
  });

  /**
   * The parameters bag as an object. Lenient on purpose: the textarea can hold
   * mid-edit garbage, and neither the size panel nor the schema-driven one
   * must be the thing that reports it (submit still refuses, through
   * `parseParameters`).
   *
   * This is v5's `formData.parameters`. v4 holds an object and renders the raw
   * JSON nowhere; v5's legacy arm IS a textarea, so the string is the source of
   * truth and every structured write round-trips through here — the same
   * spelling `setSize` has used since P4.D102.
   */
  protected readonly parametersBag = computed<Record<string, unknown>>(() => {
    try {
      const parsed = JSON.parse(this.parametersText() || '{}') as unknown;
      return typeof parsed === 'object' && parsed !== null && !Array.isArray(parsed)
        ? (parsed as Record<string, unknown>)
        : {};
    } catch {
      return {};
    }
  });

  /**
   * v4 `currentLoras` (`:329-331`) — only an ARRAY under the reserved key is a
   * LoRA list. A bag holding something else there reads as no adapters, which
   * is also what the editor shows when the model declares no support.
   */
  protected readonly currentLoras = computed<ImageLoraSpec[]>(() => {
    const raw = this.parametersBag()['loras'];
    return Array.isArray(raw) ? (raw as ImageLoraSpec[]) : [];
  });

  /**
   * v4 `:564-568` — the profile's configured `hf_api_token`, when it has one
   * and it is a string. It rides the lookup's request body so gated weights
   * resolve for the people entitled to see them.
   */
  protected readonly hfToken = computed<string | undefined>(() => {
    const raw = this.parametersBag()['hf_api_token'];
    return typeof raw === 'string' ? raw : undefined;
  });

  /**
   * v4 `handleChange` (`:14-19`) — `{...parameters, [key]: value}`. The spread
   * is the point: a parameter this panel does not render must survive being
   * edited, exactly as it survives v4's structured cases.
   */
  protected setSize(value: string): void {
    this.parametersText.set(JSON.stringify({ ...this.parametersBag(), size: value }, null, 2));
  }

  /** v4 `handleSetParameter` (`:317-329`). */
  protected setParameter(write: { key: string; value: unknown }): void {
    const next = { ...this.parametersBag() };
    // An empty box means "unset": storing '' would send a blank string to a
    // provider that reads the key's presence, not its truthiness.
    if (write.value === undefined || write.value === '') {
      delete next[write.key];
    } else {
      next[write.key] = write.value;
    }
    this.parametersText.set(JSON.stringify(next, null, 2));
  }

  /** v4 `handleLorasChange` (`:333-343`) — an empty list DELETES the key. */
  protected setLoras(loras: ImageLoraSpec[]): void {
    const next = { ...this.parametersBag() };
    if (loras.length === 0) {
      delete next['loras'];
    } else {
      next['loras'] = loras;
    }
    this.parametersText.set(JSON.stringify(next, null, 2));
  }

  constructor() {
    /**
     * v4 `:121-130` — once the registry has loaded, rewrite a legacy provider
     * value to its canonical name (`GOOGLE_IMAGEN` → `GOOGLE`). Without this the
     * raw-value option lookup above can never match and the Model select stays
     * empty for a legacy profile.
     */
    effect(() => {
      const providers = this.providers();
      const current = this.provider();
      if (this.providersQuery.isPending() || !current) {
        return;
      }
      const normalized = normalizeProviderName(current, providers);
      if (normalized !== current) {
        untracked(() => this.provider.set(normalized));
      }
    });

    /**
     * v4 `:172-175` — the auto-load. v4's `fetchModels` useCallback closes over
     * `[formData.provider, formData.apiKeyId, imageProviders]` and the effect
     * depends on the callback, so the list re-fetches whenever any of the three
     * changes; reading all three here reproduces that dependency set.
     */
    effect(() => {
      this.provider();
      this.apiKeyId();
      this.providers();
      untracked(() => void this.fetchModels());
    });

    /**
     * v4 `:194-231` — ask the provider's plugin what this model's options look
     * like, and whether it takes LoRA adapters. v4's effect deps are
     * `[providerKey, modelKey, catalogVersion]`, where `providerKey` is the
     * NORMALIZED provider string — read through the `providerKey` computed so
     * the dependency set is exactly v4's three VALUES, not the providers list.
     *
     * The cancelled flag is v4's: a slower answer for a provider the user has
     * already left must not land on top of a newer one.
     */
    effect((onCleanup) => {
      const providerKey = this.providerKey();
      const modelKey = this.modelName();
      this.catalogVersion();
      let cancelled = false;
      onCleanup(() => {
        cancelled = true;
      });
      void (async () => {
        try {
          const answer = await fetchImageOptionsSchema(this.core, providerKey, modelKey || undefined);
          if (cancelled) return;
          this.optionsSchema.set(answer.optionsSchema);
          this.loraSupport.set(answer.loraSupport);
        } catch {
          if (cancelled) return;
          // Fall back to the legacy panel and no LoRA editor rather than
          // leaving a stale schema from the previous provider on screen.
          this.optionsSchema.set(null);
          this.loraSupport.set(null);
        }
      })();
    });

    /**
     * The Model select's value is assigned POST-render, not bound. A bound
     * `[value]` lands before `@for` fills the options and is lost, and
     * `[selected]` makes the browser fall back to row 0 when the stored value
     * matches nothing — where React leaves `selectedIndex === -1` and the
     * control blank. That case is reachable here: `onProviderChange` sets an
     * empty `modelName` when the provider has no default models.
     */
    afterRenderEffect(() => {
      this.modelOptions();
      const select = this.modelSelect()?.nativeElement;
      if (select) {
        select.value = this.modelName();
      }
    });

    // Same reasoning for the size select: a stored size outside the provider's
    // list leaves the control blank rather than snapping to 1024x1024.
    afterRenderEffect(() => {
      this.sizePanel();
      const select = this.sizeSelect()?.nativeElement;
      if (select) {
        select.value = this.sizeValue();
      }
    });
  }

  /**
   * v4 `fetchModels` (`:135-171`). The provider is normalized before the call
   * (v4 `:141`); on any failure the list falls back to the registry's
   * `defaultModels` and the source reads `builtin` WITHOUT a `fetchError` —
   * v4's two fallback branches set only the models and the source, so a hard
   * failure shows the plain built-in sentence, not the `Couldn't fetch` one.
   */
  protected async fetchModels(): Promise<void> {
    const providers = this.providers();
    const raw = this.provider();
    const keyId = this.apiKeyId();
    const normalized = normalizeProviderName(raw, providers);

    this.isFetchingModels.set(true);
    this.modelsFetchError.set(null);
    try {
      const listing = await fetchImageModels(this.core, normalized, keyId || undefined);
      this.availableModels.set(listing.models);
      this.modelsSource.set(listing.source);
      this.modelsFetchError.set(listing.fetchError ?? null);
      if (listing.source === 'provider') {
        // v4 `:168-170` — the catalog now exists, so re-ask for the schema.
        this.catalogVersion.update((v) => v + 1);
      }
    } catch {
      const info = providers.find((p) => p.value === normalized || p.value === raw);
      this.availableModels.set(info?.defaultModels ?? []);
      this.modelsSource.set('builtin');
    } finally {
      this.isFetchingModels.set(false);
    }
  }

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
    // The previous provider's fetched list must not survive the switch; the
    // auto-load effect refills it (v4 re-runs `fetchModels` for the same reason).
    this.availableModels.set([]);
    this.modelsSource.set(null);
    this.modelsFetchError.set(null);
    this.apiKeyId.set('');
    this.parametersText.set('{}');
    this.parametersError.set(null);
    // v4 `:268-272` — the old provider's schema and LoRA cap describe a
    // provider we have just left; clear them rather than render them against
    // the new one until the refetch lands.
    this.optionsSchema.set(null);
    this.loraSupport.set(null);
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
