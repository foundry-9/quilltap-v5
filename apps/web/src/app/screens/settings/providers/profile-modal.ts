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

import { CoreClient } from '../../../core/core-client';
import type { ApiKeyDto, ConnectionProfileDto, ProviderInfo } from '../../../core/core-contract';
import { FormActions } from '../../../ui/form-actions';
import { Modal } from '../../../ui/modal';
import { ModelSelector } from '../../../ui/model-selector';
import {
  getModelClass,
  makeUniqueProfileName,
  MODEL_CLASSES,
  normalizeProfileName,
} from './model-classes';
import {
  buildProfileRequestBody,
  initialFormState,
  loadProfileIntoForm,
  type ProfileFormData,
  type ProviderRequirements,
} from './profile-form';

const MODEL_SUGGESTIONS: Record<string, string[]> = {
  OPENAI: ['gpt-4', 'gpt-3.5-turbo', 'gpt-4-turbo'],
  ANTHROPIC: [
    'claude-sonnet-4-5-20250929',
    'claude-haiku-4-5-20251001',
    'claude-opus-4-1-20250805',
  ],
  GOOGLE: [
    'gemini-1.5-flash-latest',
    'gemini-1.5-pro-latest',
    'gemini-1.0-pro',
    'gemini-pro-vision',
  ],
  GROK: ['grok-beta', 'grok-2', 'grok-vision-beta'],
  OLLAMA: ['llama2', 'neural-chat', 'mistral'],
  OPENROUTER: ['openai/gpt-4', 'anthropic/claude-2', 'meta-llama/llama-2-70b'],
  OPENAI_COMPATIBLE: ['gpt-3.5-turbo'],
};

/**
 * The connection-profile create/edit modal (v4
 * `components/settings/connection-profiles/ProfileModal.tsx`): the transport
 * selector, provider/key/base-url, the four-button connection-testing flow
 * (Connect → Fetch Models → Test Message; Auto-Configure is a named deferral, its
 * slot disabled), the model combobox with a free-text fallback, sampling
 * parameters, the rich capability flags, and duplicate-name inline validation.
 * Copy + `qt-*` classes carry over verbatim.
 */
@Component({
  selector: 'qt-profile-modal',
  changeDetection: ChangeDetectionStrategy.OnPush,
  imports: [Modal, FormActions, ModelSelector],
  template: `
    <qt-modal
      [title]="editing() ? 'Edit Connection Profile' : 'Create Connection Profile'"
      maxWidth="3xl"
      (close)="close.emit()"
    >
      <div class="space-y-4">
        <!-- Transport -->
        <div>
          <label for="qt-pf-transport" class="block qt-text-label mb-2">Transport *</label>
          <select
            id="qt-pf-transport"
            class="qt-select"
            [value]="form().transport"
            (change)="setField('transport', $any($event.target).value)"
          >
            <option value="api">API (provider-backed)</option>
            <option value="courier">The Courier (manual / clipboard)</option>
          </select>
          <p class="qt-text-xs mt-1">
            {{
              isCourier()
                ? 'Manual / clipboard mode. Quilltap will render each LLM call as Markdown for you to carry by hand to an external LLM. No API key, no tools — just copy out and paste back.'
                : 'Standard provider-backed mode. Quilltap calls the LLM directly using the API key and base URL you configure below.'
            }}
          </p>
        </div>

        <!-- Name + Provider -->
        <div [class]="isCourier() ? '' : 'grid grid-cols-2 gap-4'">
          <div>
            <label for="qt-pf-name" class="block qt-text-label mb-2">Name *</label>
            <input
              id="qt-pf-name"
              type="text"
              class="qt-input"
              [placeholder]="
                isCourier() ? 'e.g., Claude desktop courier' : 'e.g., My GPT-4 Profile'
              "
              [value]="form().name"
              (input)="setField('name', $any($event.target).value)"
            />
            @if (nameTaken()) {
              <p class="qt-text-destructive qt-text-xs mt-1">
                Another connection profile already bears this name. Names must be unique.
              </p>
            }
          </div>
          @if (!isCourier()) {
            <div>
              <label for="qt-pf-provider" class="block qt-text-label mb-2">Provider *</label>
              <select
                id="qt-pf-provider"
                class="qt-select"
                [value]="form().provider"
                (change)="onProviderChange($any($event.target).value)"
              >
                @if (chatProviders().length > 0) {
                  @for (p of chatProviders(); track p.name) {
                    <option [value]="p.name">{{ p.displayName }}</option>
                  }
                } @else {
                  <option value="OPENAI">OpenAI</option>
                  <option value="ANTHROPIC">Anthropic</option>
                  <option value="GOOGLE">Google</option>
                  <option value="GROK">Grok</option>
                  <option value="OLLAMA">Ollama</option>
                  <option value="OPENROUTER">OpenRouter</option>
                  <option value="OPENAI_COMPATIBLE">OpenAI Compatible</option>
                }
              </select>
            </div>
          }
        </div>

        @if (isCourier()) {
          <div>
            <label for="qt-pf-courier-model" class="block qt-text-label mb-2">
              Which LLM will you carry to? (informational)
            </label>
            <input
              id="qt-pf-courier-model"
              type="text"
              class="qt-input"
              placeholder="e.g., Claude Opus 4.7, ChatGPT o3, Local Llama via LM Studio"
              [value]="form().modelName"
              (input)="setField('modelName', $any($event.target).value)"
            />
            <p class="qt-text-xs mt-1">
              Free text — appears on the placeholder bubble so you remember which LLM to paste into.
              Quilltap does not validate or call it.
            </p>
          </div>

          <div class="space-y-2 pt-2">
            <label class="flex items-center gap-2">
              <input
                type="checkbox"
                class="qt-checkbox"
                [checked]="form().isDefault"
                (change)="setField('isDefault', $any($event.target).checked)"
              />
              <span class="text-sm">Set as default profile</span>
            </label>
            <label class="flex items-center gap-2">
              <input
                type="checkbox"
                class="qt-checkbox"
                [checked]="form().isCheap"
                (change)="setField('isCheap', $any($event.target).checked)"
              />
              <span class="text-sm"
                >Mark as cheap LLM (memory extraction, danger classification, etc.)</span
              >
            </label>
            <label class="flex items-start gap-2">
              <input
                type="checkbox"
                class="qt-checkbox mt-0.5"
                [checked]="form().courierDeltaMode"
                (change)="setField('courierDeltaMode', $any($event.target).checked)"
              />
              <span class="text-sm">
                <span class="block">Delta mode after first turn</span>
                <span class="qt-text-xs"
                  >After the character&apos;s first paste-back in a chat, render only what&apos;s
                  new since the last reply — the desktop LLM remembers the rest. The bubble still
                  keeps a full-context fallback you can swap to if your destination LLM has lost the
                  conversation.</span
                >
              </span>
            </label>
          </div>

          <div class="qt-text-xs">
            The Courier does not expose tools, web search, or image uploads. Memories, character
            manifestos, scene state, and wardrobe context are all still bundled into the prompt as
            normal — the external LLM just doesn&apos;t have any way to call back into Quilltap.
          </div>
        }

        <!-- API Key + Base URL (API transport only) -->
        @if (!isCourier()) {
          <div
            [class]="
              reqs().requiresApiKey && reqs().requiresBaseUrl ? 'grid grid-cols-2 gap-4' : ''
            "
          >
            @if (reqs().requiresApiKey) {
              <div>
                <label for="qt-pf-key" class="block qt-text-label mb-2">API Key *</label>
                <select
                  id="qt-pf-key"
                  class="qt-select"
                  [value]="form().apiKeyId"
                  (change)="setField('apiKeyId', $any($event.target).value)"
                >
                  <option value="">Select an API Key</option>
                  @for (key of keysForProvider(); track key.id) {
                    <option [value]="key.id">{{ key.label }}</option>
                  }
                </select>
              </div>
            }
            @if (reqs().requiresBaseUrl) {
              <div>
                <label for="qt-pf-baseurl" class="block qt-text-label mb-2">Base URL *</label>
                <input
                  id="qt-pf-baseurl"
                  type="url"
                  class="qt-input"
                  placeholder="http://localhost:11434"
                  [value]="form().baseUrl"
                  (input)="setField('baseUrl', $any($event.target).value)"
                />
              </div>
            }
          </div>

          <!-- Connection testing -->
          <div class="border qt-border-default rounded-lg p-4 qt-bg-muted/50">
            <h4 class="font-medium text-sm mb-3">Connection Testing</h4>
            <div class="flex flex-wrap gap-3 mb-3">
              <button
                type="button"
                class="qt-button-primary"
                [disabled]="connecting()"
                (click)="connect()"
              >
                {{ connecting() ? 'Connecting...' : 'Connect' }}
              </button>
              <button
                type="button"
                class="qt-button-primary"
                [disabled]="fetchDisabled()"
                (click)="fetchModels()"
              >
                {{ fetchingModels() ? 'Fetching...' : 'Fetch Models' }}
              </button>
              <button
                type="button"
                class="qt-button-primary"
                [disabled]="!isConnected() || testingMessage() || !form().modelName"
                (click)="testMessage()"
              >
                {{ testingMessage() ? 'Testing...' : 'Test Message' }}
              </button>
              <button
                type="button"
                class="qt-button-primary"
                disabled
                title="Auto-Configure arrives in a later installment."
              >
                Auto-Configure
              </button>
            </div>

            @if (connectionMessage()) {
              <div class="text-sm qt-alert-success">✓ {{ connectionMessage() }}</div>
            }
            @if (connectError()) {
              <div class="text-sm qt-alert-error">✗ {{ connectError() }}</div>
            }
            @if (modelsMessage()) {
              <div class="text-sm qt-alert-info">✓ {{ modelsMessage() }}</div>
            }
            @if (testMessageResult()) {
              <div class="text-sm qt-alert-info">✓ {{ testMessageResult() }}</div>
            }

            <p class="qt-text-xs mt-2">
              1. Click Connect to test the connection • 2. Fetch Models • 3. Test Message to verify
            </p>
          </div>

          <!-- Model -->
          <div>
            <label for="qt-pf-model" class="block qt-text-label mb-2">Model *</label>
            @if (fetchedModels().length > 0) {
              <qt-model-selector
                [models]="fetchedModels()"
                [value]="form().modelName"
                placeholder="Select or search a model"
                [showFetchedCount]="true"
                (changed)="onModelChange($event)"
              />
            } @else {
              <input
                id="qt-pf-model"
                type="text"
                class="qt-input"
                placeholder="e.g., gpt-4"
                list="qt-pf-model-suggestions"
                [value]="form().modelName"
                (input)="onModelChange($any($event.target).value)"
              />
              <datalist id="qt-pf-model-suggestions">
                @for (m of modelSuggestions(); track m) {
                  <option [value]="m"></option>
                }
              </datalist>
            }
          </div>

          <!-- Model parameters -->
          <div class="border-t qt-border-default pt-4">
            <h4 class="font-medium text-sm mb-3">Model Parameters (Optional)</h4>
            <div class="grid grid-cols-3 gap-4">
              <div>
                <label for="qt-pf-temp" class="block qt-text-label mb-2"
                  >Temperature ({{ form().temperature }})</label
                >
                <input
                  id="qt-pf-temp"
                  type="range"
                  min="0"
                  max="2"
                  step="0.1"
                  class="w-full"
                  [value]="form().temperature"
                  (input)="setField('temperature', parseNum($any($event.target).value))"
                />
                <p class="qt-text-xs mt-1">0 = deterministic, 2 = creative</p>
              </div>
              <div>
                <label for="qt-pf-maxtokens" class="block qt-text-label mb-2">Max Tokens</label>
                <input
                  id="qt-pf-maxtokens"
                  type="number"
                  min="1"
                  class="qt-input"
                  [value]="form().maxTokens"
                  (input)="setField('maxTokens', parseInt10($any($event.target).value))"
                />
                <p class="qt-text-xs mt-1">Max output tokens</p>
              </div>
              <div>
                <label for="qt-pf-topp" class="block qt-text-label mb-2"
                  >Top P ({{ form().topP }})</label
                >
                <input
                  id="qt-pf-topp"
                  type="range"
                  min="0"
                  max="1"
                  step="0.05"
                  class="w-full"
                  [value]="form().topP"
                  (input)="setField('topP', parseNum($any($event.target).value))"
                />
                <p class="qt-text-xs mt-1">Nucleus sampling (0-1)</p>
              </div>
            </div>
          </div>

          <!-- Capability flags -->
          <div class="space-y-2 pt-2">
            <label class="flex items-center gap-2">
              <input
                type="checkbox"
                class="qt-checkbox"
                [checked]="form().isDefault"
                (change)="setField('isDefault', $any($event.target).checked)"
              />
              <span class="text-sm">Set as default profile</span>
            </label>
            <label class="flex items-center gap-2">
              <input
                type="checkbox"
                class="qt-checkbox"
                [checked]="form().isCheap"
                (change)="setField('isCheap', $any($event.target).checked)"
              />
              <span class="text-sm">Mark as cheap LLM (for cost-effective tasks)</span>
            </label>
            <label class="flex items-center gap-2">
              <input
                type="checkbox"
                class="qt-checkbox"
                [checked]="form().isDangerousCompatible"
                (change)="setField('isDangerousCompatible', $any($event.target).checked)"
              />
              <span class="text-sm"
                >Uncensored-compatible (suitable for dangerous/sensitive content routing)</span
              >
            </label>
            <label class="flex items-center gap-2">
              <input
                type="checkbox"
                class="qt-checkbox"
                [checked]="form().allowToolUse"
                (change)="setField('allowToolUse', $any($event.target).checked)"
              />
              <span class="text-sm"
                >Allow tool use (overrides chat and project tool settings when disabled)</span
              >
            </label>
            @if (form().allowToolUse) {
              <div class="flex flex-col gap-1 ml-6">
                <label for="qt-pf-pseudo" class="text-sm">Tool format</label>
                <select
                  id="qt-pf-pseudo"
                  class="qt-select w-full max-w-md"
                  [value]="form().pseudoToolMode"
                  (change)="setField('pseudoToolMode', $any($event.target).value)"
                >
                  <option value="auto">Auto (recommended)</option>
                  <option value="native">Native function calling</option>
                  <option value="simple-json">Simple JSON (&lt;tool_call&gt;…)</option>
                  <option value="text-block">Text-block ([[TOOL ...]]) — legacy</option>
                </select>
                <p class="qt-text-xs mt-1">
                  Auto: native for capable models, otherwise simple JSON. Override only if your
                  model needs a particular dialect.
                </p>
              </div>
            }
            <label class="flex items-center gap-2">
              <input
                type="checkbox"
                class="qt-checkbox"
                [checked]="form().supportsImageUpload"
                (change)="setField('supportsImageUpload', $any($event.target).checked)"
              />
              <span class="text-sm">Supports image attachments (vision input)</span>
            </label>
            <label class="flex items-center gap-2">
              <input
                type="checkbox"
                class="qt-checkbox"
                [checked]="form().allowWebSearch"
                (change)="setField('allowWebSearch', $any($event.target).checked)"
              />
              <span class="text-sm">Allow web search tool</span>
            </label>
            @if (reqs().supportsWebSearch) {
              <label class="flex items-center gap-2">
                <input
                  type="checkbox"
                  class="qt-checkbox"
                  [checked]="form().useNativeWebSearch"
                  (change)="setField('useNativeWebSearch', $any($event.target).checked)"
                />
                <span class="text-sm">Use provider native web search</span>
              </label>
            }
          </div>

          <!-- Model class + max context -->
          <div class="grid grid-cols-2 gap-4">
            <div>
              <label for="qt-pf-class" class="block qt-text-label mb-2">Model Class</label>
              <select
                id="qt-pf-class"
                class="qt-select"
                [value]="form().modelClass"
                (change)="setField('modelClass', $any($event.target).value)"
              >
                <option value="">(None)</option>
                @for (mc of modelClasses; track mc.name) {
                  <option [value]="mc.name">{{ mc.name }} (Tier {{ mc.tier }})</option>
                }
              </select>
              @if (selectedModelClass(); as mc) {
                <p class="qt-text-xs mt-1">
                  Context: {{ mc.maxContext.toLocaleString() }} | Output:
                  {{ mc.maxOutput.toLocaleString() }} | Quality: {{ mc.quality }} | Tags:
                  {{ mc.tags.join(', ') }}
                </p>
              }
            </div>
            <div>
              <label for="qt-pf-maxctx" class="block qt-text-label mb-2"
                >Max Context (tokens)</label
              >
              <input
                id="qt-pf-maxctx"
                type="number"
                min="1"
                class="qt-input"
                placeholder="e.g., 128000"
                [value]="form().maxContext"
                (input)="setField('maxContext', $any($event.target).value)"
              />
              <p class="qt-text-xs mt-1">
                Override context window size. Leave blank to use provider default.
              </p>
            </div>
          </div>
        }
      </div>

      <div qt-modal-footer>
        <qt-form-actions
          [submitLabel]="saving() ? 'Saving...' : editing() ? 'Update Profile' : 'Create Profile'"
          [isLoading]="saving()"
          [isDisabled]="!isValid()"
          (cancel)="close.emit()"
          (submit)="submit()"
        />
      </div>
    </qt-modal>
  `,
})
export class ProfileModal implements OnInit {
  private readonly core = inject(CoreClient);

  readonly profile = input<ConnectionProfileDto | null>(null);
  readonly providers = input<ProviderInfo[]>([]);
  readonly apiKeys = input<ApiKeyDto[]>([]);
  /** Normalized names already taken by OTHER profiles (duplicate-name guard). */
  readonly takenNames = input<Set<string>>(new Set());
  readonly close = output<void>();
  readonly saved = output<void>();

  protected readonly modelClasses = MODEL_CLASSES;
  protected readonly form = signal<ProfileFormData>({ ...initialFormState });

  // Provider-action state
  protected readonly isConnected = signal(false);
  protected readonly connectionMessage = signal<string | null>(null);
  protected readonly connectError = signal<string | null>(null);
  protected readonly fetchedModels = signal<string[]>([]);
  protected readonly modelsMessage = signal<string | null>(null);
  protected readonly testMessageResult = signal<string | null>(null);
  protected readonly connecting = signal(false);
  protected readonly fetchingModels = signal(false);
  protected readonly testingMessage = signal(false);
  protected readonly saving = signal(false);

  protected readonly editing = computed(() => !!this.profile()?.id);
  protected readonly isCourier = computed(() => this.form().transport === 'courier');

  protected readonly chatProviders = computed(() =>
    this.providers().filter((p) => p.capabilities?.chat),
  );

  protected readonly reqs = computed<ProviderRequirements>(() => {
    const p = this.providers().find((x) => x.name === this.form().provider);
    return {
      requiresApiKey: p?.configRequirements?.requiresApiKey ?? true,
      requiresBaseUrl: p?.configRequirements?.requiresBaseUrl ?? false,
      supportsWebSearch: p?.capabilities?.webSearch ?? false,
      supportsToolUse: p?.capabilities?.toolUse ?? false,
    };
  });

  protected readonly keysForProvider = computed(() =>
    this.apiKeys().filter((k) => k.provider === this.form().provider),
  );

  protected readonly modelSuggestions = computed(() =>
    (MODEL_SUGGESTIONS[this.form().provider] ?? ['gpt-3.5-turbo']).slice().sort(),
  );

  protected readonly nameTaken = computed(() => {
    const name = this.form().name.trim();
    return name.length > 0 && this.takenNames().has(normalizeProfileName(name));
  });

  protected readonly isValid = computed(
    () =>
      this.form().name.trim().length > 0 &&
      this.form().modelName.trim().length > 0 &&
      !this.nameTaken(),
  );

  protected readonly selectedModelClass = computed(() =>
    this.form().modelClass ? getModelClass(this.form().modelClass) : undefined,
  );

  protected readonly fetchDisabled = computed(() => {
    if (this.fetchingModels()) return true;
    if (this.reqs().requiresBaseUrl && !this.form().baseUrl) return true;
    if (this.reqs().requiresApiKey && !this.isConnected()) return true;
    return false;
  });

  ngOnInit(): void {
    const p = this.profile();
    if (p) {
      this.form.set(loadProfileIntoForm(p));
      void this.autoFetchModelsForEdit(p);
    }
  }

  protected setField<K extends keyof ProfileFormData>(key: K, value: ProfileFormData[K]): void {
    this.form.update((f) => ({ ...f, [key]: value }));
  }

  protected parseNum(v: string): number {
    return parseFloat(v);
  }
  protected parseInt10(v: string): number {
    return parseInt(v, 10);
  }

  protected onProviderChange(provider: string): void {
    this.form.update((f) => ({ ...f, provider }));
    const cfg = this.providers().find((p) => p.name === provider);
    if (cfg?.configRequirements?.baseUrlDefault && !this.form().baseUrl) {
      this.setField('baseUrl', cfg.configRequirements.baseUrlDefault);
    }
    // New profiles: default allowToolUse from the provider's capability.
    if (!this.profile()?.id) {
      this.setField('allowToolUse', cfg?.capabilities?.toolUse ?? false);
    }
  }

  protected onModelChange(modelName: string): void {
    this.setField('modelName', modelName);
    // Auto-fill the name for a NEW profile when it's empty (suffixed until unique).
    if (!this.profile()?.id && !this.form().name.trim() && modelName.trim()) {
      const suggested = makeUniqueProfileName(
        `${this.form().provider}/${modelName}`,
        this.takenNames(),
      );
      this.setField('name', suggested);
    }
  }

  private profilePayloadForActions(): Record<string, unknown> {
    const f = this.form();
    return {
      provider: f.provider,
      apiKeyId: f.apiKeyId || undefined,
      baseUrl: f.baseUrl || undefined,
    };
  }

  protected async connect(): Promise<void> {
    if (!this.form().provider) {
      this.connectError.set('Provider is required');
      return;
    }
    if (this.reqs().requiresBaseUrl && !this.form().baseUrl) {
      this.connectError.set('Base URL is required for this provider');
      return;
    }
    if (this.reqs().requiresApiKey && !this.form().apiKeyId) {
      this.connectError.set('API Key is required for this provider');
      return;
    }
    this.connecting.set(true);
    this.connectError.set(null);
    this.connectionMessage.set(null);
    try {
      const data = await this.core.dispatchData({
        type: 'connectionProfileTest',
        profile: this.profilePayloadForActions(),
      });
      if (data['valid'] === true) {
        this.isConnected.set(true);
        this.connectionMessage.set((data['message'] as string) || 'Connection successful!');
      } else {
        this.isConnected.set(false);
        this.connectError.set((data['error'] as string) || 'Connection test failed');
      }
    } catch (err) {
      this.isConnected.set(false);
      this.connectError.set(err instanceof Error ? err.message : 'Connection test failed');
    } finally {
      this.connecting.set(false);
    }
  }

  protected async fetchModels(): Promise<void> {
    if (this.reqs().requiresBaseUrl && !this.form().baseUrl) {
      this.connectError.set('Base URL is required for this provider');
      return;
    }
    this.fetchingModels.set(true);
    try {
      const resp = await this.core.dispatchExpect(
        {
          type: 'modelFetch',
          provider: this.form().provider,
          apiKeyId: this.form().apiKeyId || undefined,
          baseUrl: this.form().baseUrl || undefined,
        },
        'models',
      );
      this.fetchedModels.set(resp.data.models ?? []);
      this.modelsMessage.set(`Found ${resp.data.models?.length ?? 0} models`);
    } catch (err) {
      this.fetchedModels.set([]);
      this.modelsMessage.set(null);
      this.connectError.set(err instanceof Error ? err.message : 'Failed to fetch models');
    } finally {
      this.fetchingModels.set(false);
    }
  }

  protected async testMessage(): Promise<void> {
    if (!this.form().modelName) {
      return;
    }
    this.testingMessage.set(true);
    this.testMessageResult.set(null);
    const f = this.form();
    try {
      const data = await this.core.dispatchData({
        type: 'connectionProfileTestMessage',
        profile: {
          provider: f.provider,
          apiKeyId: f.apiKeyId || undefined,
          baseUrl: f.baseUrl || undefined,
          modelName: f.modelName,
          parameters: {
            temperature: parseFloat(String(f.temperature)),
            max_tokens: parseInt(String(f.maxTokens), 10),
            top_p: parseFloat(String(f.topP)),
          },
        },
      });
      if (data['success'] === true) {
        this.testMessageResult.set(
          (data['message'] as string) || 'Test message sent successfully!',
        );
      } else {
        this.connectError.set((data['error'] as string) || 'Test message failed');
      }
    } catch (err) {
      this.connectError.set(err instanceof Error ? err.message : 'Test message failed');
    } finally {
      this.testingMessage.set(false);
    }
  }

  private async autoFetchModelsForEdit(p: ConnectionProfileDto): Promise<void> {
    try {
      const resp = await this.core.dispatchExpect(
        {
          type: 'modelFetch',
          provider: p.provider,
          apiKeyId: p.apiKeyId || undefined,
          baseUrl: p.baseUrl || undefined,
        },
        'models',
      );
      this.fetchedModels.set(resp.data.models ?? []);
      this.modelsMessage.set(`Found ${resp.data.models?.length ?? 0} models`);
    } catch {
      // silently ignore — the free-text model input still works
    }
  }

  protected async submit(): Promise<void> {
    if (!this.isValid()) {
      return;
    }
    this.saving.set(true);
    this.connectError.set(null);
    const body = buildProfileRequestBody(this.form());
    const id = this.profile()?.id;
    try {
      if (id) {
        await this.core.dispatchExpect(
          { type: 'connectionProfileUpdate', profileId: id, profile: body },
          'connectionProfile',
        );
      } else {
        await this.core.dispatchExpect(
          { type: 'connectionProfileCreate', profile: body },
          'connectionProfile',
        );
      }
      this.saved.emit();
      this.close.emit();
    } catch (err) {
      this.connectError.set(err instanceof Error ? err.message : 'Failed to save profile');
    } finally {
      this.saving.set(false);
    }
  }
}
