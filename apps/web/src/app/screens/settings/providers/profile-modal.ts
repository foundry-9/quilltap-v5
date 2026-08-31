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
import type {
  ApiKeyDto,
  ConnectionProfileDto,
  ModelInfo,
  ProviderInfo,
} from '../../../core/core-contract';
import { FormActions } from '../../../ui/form-actions';
import { Modal } from '../../../ui/modal';
import { ModelSelector } from '../../../ui/model-selector';
import { providerAcceptsApiKey, providerRequiresApiKey } from './api-key-support';
import { getAttachmentSupportDescription, supportsMimeType } from './attachment-support';
import {
  getModelClass,
  makeUniqueProfileName,
  MODEL_CLASSES,
  normalizeProfileName,
} from './model-classes';
import { defaultMultiCharacterPrefill } from './multi-character-prefill';
import {
  buildProfileRequestBody,
  initialFormState,
  loadProfileIntoForm,
  outboundApiKeyId,
  outboundBaseUrl,
  type ProfileFormData,
  type ProviderRequirements,
} from './profile-form';
import { ProfileTagEditor } from './profile-tag-editor';
import { ProviderOptionsPanel } from './provider-options-panel';
import type { ProviderOptionsSchema } from './provider-options-schema';
import { evaluateThinkingTurn } from './thinking-turn';

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
 *
 * **Provider options are schema-driven, as in v4 (P4.D84).** The per-provider
 * rows come from the active plugin's `getProviderOptionsSchema()`, carried on
 * the providers listing as `optionsSchema` and rendered by
 * {@link ProviderOptionsPanel}. This RETIRES the P4.D81 divergence — the
 * hardcoded Ollama Enable Thinking row is gone, and Ollama's schema draws it
 * (along with thinking effort, keep-alive, the request timeout, and the whole
 * Sampling group) from the wire instead. Keys the schema declares no control
 * for still ride the bag untouched on save, exactly as before.
 *
 * v4 passes the panel no directive callback; the one `affects: 'modelInput'`
 * field in the bundled schemas (OpenRouter's `useCustomModel`) is read straight
 * off the parameters bag to swap the model input between selector and free
 * text (`ProfileModal.tsx:198-203`). v5 derives it the same way.
 */
@Component({
  selector: 'qt-profile-modal',
  changeDetection: ChangeDetectionStrategy.OnPush,
  imports: [Modal, FormActions, ModelSelector, ProviderOptionsPanel, ProfileTagEditor],
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
                (change)="onProviderChange($any($event.target).value)"
              >
                @if (chatProviders().length > 0) {
                  @for (p of chatProviders(); track p.name) {
                    <option [value]="p.name" [selected]="form().provider === p.name">
                      {{ p.displayName }}
                    </option>
                  }
                } @else {
                  <option value="OPENAI" [selected]="form().provider === 'OPENAI'">OpenAI</option>
                  <option value="ANTHROPIC" [selected]="form().provider === 'ANTHROPIC'">
                    Anthropic
                  </option>
                  <option value="GOOGLE" [selected]="form().provider === 'GOOGLE'">Google</option>
                  <option value="GROK" [selected]="form().provider === 'GROK'">Grok</option>
                  <option value="OLLAMA" [selected]="form().provider === 'OLLAMA'">Ollama</option>
                  <option value="OPENROUTER" [selected]="form().provider === 'OPENROUTER'">
                    OpenRouter
                  </option>
                  <option
                    value="OPENAI_COMPATIBLE"
                    [selected]="form().provider === 'OPENAI_COMPATIBLE'"
                  >
                    OpenAI Compatible
                  </option>
                }
              </select>
              <!-- v4 ProfileModal.tsx:379-381. The baseUrl argument v4 passes
                   is unread by the static table on both sides. -->
              <p class="qt-text-xs mt-1">Non-image attachments: {{ attachmentSupport() }}</p>
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

          <!-- Tag editor - editing only (v4 ProfileModal.tsx:451-455). -->
          @if (profile()?.id; as profileId) {
            <div class="pt-4">
              <qt-profile-tag-editor [profileId]="profileId" />
            </div>
          }
        }

        <!-- API Key + Base URL (API transport only) -->
        @if (!isCourier()) {
          <!--
            Shown when the provider *takes* a key; starred only when it demands
            one. OpenAI-Compatible is the provider where those differ — a hosted
            endpoint needs a bearer token, a local llama.cpp has nowhere to put
            one — so its field is offered and optional rather than absent (v4
            bug 81).
          -->
          <div
            [class]="reqs().acceptsApiKey && reqs().requiresBaseUrl ? 'grid grid-cols-2 gap-4' : ''"
          >
            @if (reqs().acceptsApiKey) {
              <div>
                <label for="qt-pf-key" class="block qt-text-label mb-2">{{
                  reqs().requiresApiKey ? 'API Key *' : 'API Key'
                }}</label>
                <select
                  id="qt-pf-key"
                  class="qt-select"
                  (change)="setField('apiKeyId', $any($event.target).value)"
                >
                  <option value="" [selected]="!form().apiKeyId">
                    {{
                      reqs().requiresApiKey
                        ? 'Select an API Key'
                        : 'None — the endpoint needs no key'
                    }}
                  </option>
                  @for (key of keysForProvider(); track key.id) {
                    <option [value]="key.id" [selected]="form().apiKeyId === key.id">
                      {{ key.label }}
                    </option>
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

          <!-- Model. v4 ProfileModal.tsx:577-625: the modelInput directive
               branch first (free text, datalist from the fetched models when
               there are any), then the selector, then the plain fallback. -->
          <div>
            <label for="qt-pf-model" class="block qt-text-label mb-2">Model *</label>
            @if (useCustomModelDirective()) {
              <input
                id="qt-pf-model"
                type="text"
                class="qt-input"
                placeholder="e.g., openai/gpt-4-turbo"
                list="qt-pf-model-suggestions"
                [value]="form().modelName"
                (input)="onModelChange($any($event.target).value)"
              />
              <datalist id="qt-pf-model-suggestions">
                @for (m of customModelSuggestions(); track m) {
                  <option [value]="m"></option>
                }
              </datalist>
            } @else if (fetchedModels().length > 0) {
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
            <!-- The seed-never-clamp hint (v4 ProfileModal.tsx:742-748, bug
                 71): the box above stays editable whatever the capability
                 says, so a provider that advertises no tool support explains
                 why it starts off rather than locking anything. -->
            @if (!reqs().supportsToolUse) {
              <p class="qt-text-xs ml-6">
                This provider does not advertise tool support, so new profiles start with it off. If
                your endpoint does speak native function calling — llama-server with
                <code>--jinja</code>, vLLM, LM Studio — you may turn it on regardless.
              </p>
            }
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
            <!-- The multi-character turn anchor (v4 ProfileModal.tsx:764-793,
                 23af7146): v4's slot exactly — after the pseudo-tool block,
                 before Supports image attachments. -->
            <div class="flex flex-col gap-1">
              <label class="flex items-center gap-2">
                <input
                  type="checkbox"
                  class="qt-checkbox"
                  [checked]="form().multiCharacterPrefill"
                  (change)="setField('multiCharacterPrefill', $any($event.target).checked)"
                />
                <span class="text-sm"
                  >Announce the speaker in multi-character scenes ([Name] prefill)</span
                >
              </label>
              <p class="qt-text-xs ml-6">
                Ticked, a multi-character turn is handed to the model already opened with
                <code>[Name]</code>, so it can only continue that character&apos;s line. Unticked,
                the same instruction is given in prose and the model is left to begin the turn
                itself. Untick it for models that refuse an opened turn outright, for local thinking
                models whose reasoning never appears (an opened turn closes that door), and for any
                model that spends its reply wondering whether the name was addressed to it.
              </p>
              @if (prefillAgainstProviderDefault()) {
                <p class="qt-text-xs qt-text-warning ml-6">
                  Anthropic&apos;s recent models reject a request handed over mid-turn and will
                  return an error on every multi-character reply. Leave this unticked unless you
                  know your model tolerates it.
                </p>
              }
              @if (prefillOnThinkingProfile()) {
                <p class="qt-text-xs qt-text-warning ml-6">
                  This model reasons before it answers, and a turn handed over already opened sits
                  badly with that: some providers refuse the request outright, others quietly
                  swallow the reasoning altogether. A model that deliberates over whose turn it is
                  rarely needs the name put in its mouth — leave this unticked unless you have
                  watched yours cope.
                </p>
              }
            </div>
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

          <!-- Fallback — the understudies. API transport only: a Courier
               request is carried by hand, so nothing about it can stand in
               automatically. Sits next to Model Class because the tier toggle
               below is only as good as that field (v4
               ProfileModal.tsx:1030-1097). -->
          @if (!isCourier()) {
            <div class="qt-card p-4 space-y-3">
              <div>
                <h4 class="qt-text-label">Fallback</h4>
                <p class="qt-text-xs mt-1">
                  Should this connection be indisposed — a refused key, a rate limit, a
                  provider gone dark — the show need not come down. Name an understudy and
                  the call is passed along without a word to anyone.
                </p>
              </div>

              <div>
                <label for="qt-pf-fallback" class="block qt-text-label mb-2">Understudy</label>
                <select
                  id="qt-pf-fallback"
                  name="fallbackProfileId"
                  class="qt-select"
                  [value]="form().fallbackProfileId"
                  (change)="setField('fallbackProfileId', $any($event.target).value)"
                >
                  <option value="" [selected]="!form().fallbackProfileId">
                    (None — the call simply fails)
                  </option>
                  @for (p of fallbackCandidates(); track p.id) {
                    <option [value]="p.id" [selected]="form().fallbackProfileId === p.id">
                      {{ p.name }} — {{ p.provider }} {{ p.modelName }}
                    </option>
                  }
                </select>
                @if (selectedFallbackProfile(); as target) {
                  @if (!fallbackTargetHasKey()) {
                    <p class="qt-text-xs qt-text-warning mt-1">
                      {{ target.name }} has no API key attached yet. The understudy will be
                      skipped until one arrives.
                    </p>
                  }
                }
              </div>

              <div class="flex items-center gap-2">
                <input
                  type="checkbox"
                  id="qt-pf-tier"
                  name="allowTierFallback"
                  class="qt-checkbox"
                  [checked]="form().allowTierFallback"
                  (change)="setField('allowTierFallback', $any($event.target).checked)"
                />
                <label for="qt-pf-tier" class="text-sm">
                  Draft a stand-in from the company when both named players are indisposed
                </label>
              </div>
              <p class="qt-text-xs">
                One further attempt, and one only: a connection of the same standing or
                better, preferring a different provider than the one that just failed.
                @if (!form().modelClass) {
                  Set a Model Class above and the choice will be a great deal better
                  informed — without one, only your other unclassified connections are
                  eligible.
                }
              </p>
            </div>
          }

          <!-- Provider-specific options — schema-driven, supplied by the active
               plugin. v4's slot exactly (ProfileModal.tsx:899-908): after Max
               Context, before the tag editor. -->
          @if (optionsSchema(); as schema) {
            <qt-provider-options-panel
              [schema]="schema"
              [parameters]="form().parameters"
              [fetchedModels]="fetchedModels()"
              [modelName]="form().modelName"
              (setParameter)="setParameter($event.key, $event.value)"
            />
          }

          <!-- Tag editor - editing only, API path. The courier path renders
               its own above (v4 ProfileModal.tsx:935-940). -->
          @if (profile()?.id; as profileId) {
            <div class="pt-4">
              <qt-profile-tag-editor [profileId]="profileId" />
            </div>
          }
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
  /**
   * Every profile the user owns — the candidate pool for the Fallback dropdown
   * (v4 `ProfileModal.tsx:27-32`). A modal rendered without it simply offers no
   * understudies, which is the right answer rather than a crash.
   */
  readonly allProfiles = input<ConnectionProfileDto[]>([]);
  readonly close = output<void>();
  readonly saved = output<void>();

  protected readonly modelClasses = MODEL_CLASSES;
  protected readonly form = signal<ProfileFormData>({ ...initialFormState });

  // Provider-action state
  protected readonly isConnected = signal(false);
  protected readonly connectionMessage = signal<string | null>(null);
  protected readonly connectError = signal<string | null>(null);
  protected readonly fetchedModels = signal<string[]>([]);
  /**
   * The same fetch's per-model info rows (v4's `fetchedModelsWithInfo`,
   * `ProfileModal.tsx:66`): the selected model's static facts feed
   * {@link runsThinkingTurn}, and the list landing is the one event that can
   * correct a stored-null prefill row ({@link correctStoredNullPrefill}).
   */
  protected readonly fetchedModelsWithInfo = signal<ModelInfo[]>([]);
  protected readonly modelsMessage = signal<string | null>(null);
  protected readonly testMessageResult = signal<string | null>(null);
  protected readonly connecting = signal(false);
  protected readonly fetchingModels = signal(false);
  protected readonly testingMessage = signal(false);
  protected readonly saving = signal(false);

  protected readonly editing = computed(() => !!this.profile()?.id);
  protected readonly isCourier = computed(() => this.form().transport === 'courier');

  /**
   * Eligible understudies: anyone but this profile and the Courier profiles.
   * A cycle (A names B, B names A) is deliberately allowed — chains never
   * recurse, so B's own understudy is not followed and the cycle simply stops.
   *
   * NOTE the options bind `[selected]`, not the select's
   * `[value]`: an Angular select cannot mirror React's controlled value, and
   * every sibling select in this modal binds the same way.
   */
  protected readonly fallbackCandidates = computed(() =>
    this.allProfiles().filter(
      (p) => p.id !== this.profile()?.id && p.transport !== 'courier',
    ),
  );

  protected readonly selectedFallbackProfile = computed(() => {
    const id = this.form().fallbackProfileId;
    if (!id) return null;
    return this.allProfiles().find((p) => p.id === id) ?? null;
  });

  /**
   * A soft warning, never a block: keys can arrive later, and a provider that
   * takes none at all is perfectly ready as it stands.
   */
  protected readonly fallbackTargetHasKey = computed(() => {
    const target = this.selectedFallbackProfile();
    if (!target) return true;
    if (target.apiKeyId) return true;
    const cfg = this.providers().find((p) => p.name === target.provider)?.configRequirements;
    return !providerAcceptsApiKey(cfg);
  });

  protected readonly chatProviders = computed(() =>
    this.providers().filter((p) => p.capabilities?.chat),
  );

  protected readonly reqs = computed<ProviderRequirements>(() => {
    const p = this.providers().find((x) => x.name === this.form().provider);
    return {
      requiresApiKey: providerRequiresApiKey(p?.configRequirements),
      acceptsApiKey: providerAcceptsApiKey(p?.configRequirements),
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

  /**
   * The active provider's options schema (v4 `ProfileModal.tsx:194-196`). The
   * wire contract types this `unknown | null`
   * (`core-contract.ts` `ProviderInfo.optionsSchema`); the shape is Contract
   * B's, narrowed here rather than validated — v4 hands the plugin's object
   * straight to the renderer too.
   */
  protected readonly optionsSchema = computed<ProviderOptionsSchema | null>(() => {
    const cfg = this.providers().find((p) => p.name === this.form().provider);
    return (cfg?.optionsSchema ?? null) as ProviderOptionsSchema | null;
  });

  /**
   * The one `affects: 'modelInput'` directive in the bundled schemas
   * (OpenRouter's `useCustomModel`). v4 derives it straight from the parameters
   * map rather than plumbing the panel's directive callback, "so deriving
   * directly from the parameter map keeps the model input in sync without an
   * extra useState/useEffect" (`ProfileModal.tsx:198-203`).
   */
  protected readonly useCustomModelDirective = computed(
    () => this.form().parameters['useCustomModel'] === true,
  );

  /** v4 `:590-594`: the free-text datalist prefers the fetched models. */
  protected readonly customModelSuggestions = computed(() =>
    this.fetchedModels().length > 0 ? this.fetchedModels() : this.modelSuggestions(),
  );

  /**
   * The prefill box is ticked on a provider whose default is OFF (v4
   * `ProfileModal.tsx:786-787`) — today that is Anthropic alone, and the
   * warning names it. Permitted, but warned about: some Anthropic-compatible
   * endpoints do tolerate an assistant tail.
   */
  protected readonly prefillAgainstProviderDefault = computed(
    () => this.form().multiCharacterPrefill && !defaultMultiCharacterPrefill(this.form().provider),
  );

  /** v4 `getSelectedModelInfo` (`ProfileModal.tsx:203-206`). */
  protected readonly selectedModelInfo = computed<ModelInfo | null>(() => {
    if (!this.form().modelName || this.fetchedModelsWithInfo().length === 0) return null;
    return this.fetchedModelsWithInfo().find((m) => m.id === this.form().modelName) ?? null;
  });

  /**
   * Will this profile run a thinking turn? Same evaluator the server runs, fed
   * the plugin's declared rule and the selected model's static facts — which
   * is why the rule is declarative rather than a plugin closure (bug 85). It
   * decides the multi-character prefill default and the warning that goes with
   * it. On a wire that omits the rule and the facts it answers `false` and
   * every seed degrades to the provider rule alone, exactly as v4's client
   * does.
   */
  protected readonly runsThinkingTurn = computed(() => {
    const cfg = this.providers().find((p) => p.name === this.form().provider);
    return evaluateThinkingTurn({
      rule: cfg?.thinkingTurnRule ?? null,
      parameters: this.form().parameters,
      model: this.selectedModelInfo(),
    });
  });

  /**
   * The thinking-turn warning (v4 `ProfileModal.tsx:900-910`): the box is
   * TICKED, the provider's own default is prefill-ON (a default-off provider
   * already gets the Anthropic warning above), and the profile will run a
   * thinking turn. Warned about, never vetoed — the tri-state exists so the
   * user may overrule us.
   */
  protected readonly prefillOnThinkingProfile = computed(
    () =>
      this.form().multiCharacterPrefill &&
      defaultMultiCharacterPrefill(this.form().provider) &&
      this.runsThinkingTurn(),
  );

  /** v4 `ProfileModal.tsx:379-381` — the line under the provider select. */
  protected readonly attachmentSupport = computed(() =>
    getAttachmentSupportDescription(this.form().provider),
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

  /** {@link correctStoredNullPrefill} has fired; it never fires twice. */
  private storedPrefillCorrected = false;

  /**
   * A stored row that never chose (null — pre-4.9, or an import) shows the
   * provider default until the model list arrives, because the thinking half
   * of the answer needs the model's static facts. Correct it once, so the box
   * matches what the server will actually do and a save cannot freeze the
   * wrong answer into the column (bug 85). Only ever fires for a null row, so
   * it can never overwrite a choice the user made.
   *
   * v4 spells this as a `useEffect` keyed on the model list landing
   * (`ProfileModal.tsx:237-254` — "Re-running on every keystroke would fight
   * the checkbox; the model list landing is the one event that changes the
   * answer"). v5 spells the same requirement as an explicit call from the two
   * model-list-landing sites plus a fired-once latch — a spelling that CANNOT
   * re-fire on keystrokes, or at all after the one correction.
   */
  private correctStoredNullPrefill(): void {
    if (this.storedPrefillCorrected) return;
    const p = this.profile();
    if (p?.id === undefined || (p.multiCharacterPrefill ?? null) !== null) return;
    if (this.fetchedModelsWithInfo().length === 0) return;
    this.storedPrefillCorrected = true;
    this.setField(
      'multiCharacterPrefill',
      defaultMultiCharacterPrefill(this.form().provider, this.runsThinkingTurn()),
    );
  }

  /**
   * Write one provider-option key into the `parameters` bag (v4's
   * `setParameter`, `ProfileModal.tsx:205-216`). Every other key in the bag
   * rides along untouched — including the ones the schema declares no control
   * for. `undefined` DELETES the key rather than storing a hole, which is how
   * a cleared number field removes itself (v4 `:208-210`).
   */
  protected setParameter(key: string, value: unknown): void {
    this.form.update((f) => {
      const next = { ...f.parameters };
      if (value === undefined) {
        delete next[key];
      } else {
        next[key] = value;
      }
      return { ...f, parameters: next };
    });
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
    // New profiles: default allowToolUse and supportsImageUpload from the new
    // provider's capabilities, and re-seed the turn anchor from its default (v4
    // `handleProviderChange`, `:228-243` — all three inside the same
    // new-profile guard, so a saved choice is never clobbered on an existing
    // profile).
    //
    // `toolUse` is a SEED, never a clamp: the checkbox stays editable whatever
    // the capability says. OPENAI_COMPATIBLE declares `false` because an
    // arbitrary endpoint is the conservative case, but a user pointing at
    // llama-server --jinja knows better than we do (v4 bug 71) — hence the hint
    // rendered under the box.
    if (!this.profile()?.id) {
      this.setField('allowToolUse', cfg?.capabilities?.toolUse ?? false);
      // v4 passes the pre-change baseUrl here; `supportsMimeType` never reads
      // it (the table is static), so the argument is dropped rather than
      // faked.
      this.setField('supportsImageUpload', supportsMimeType(provider, 'image/jpeg'));
      // No model is chosen yet at this point, so only the provider rule can
      // speak; `onModelChange` re-seeds once the model is known and its
      // thinking habit can be read.
      this.setField('multiCharacterPrefill', defaultMultiCharacterPrefill(provider));
    }
  }

  protected onModelChange(modelName: string): void {
    this.setField('modelName', modelName);

    // Re-seed the multi-character turn anchor now the model is known: a model
    // that reasons unasked (deepseek-v4-flash) breaks on a `[Name]` prefill,
    // and the user never opted into thinking so nothing else would warn them
    // (bug 85). A seed, never a clamp — the box stays editable.
    if (!this.profile()?.id) {
      const cfg = this.providers().find((p) => p.name === this.form().provider);
      const modelInfo = this.fetchedModelsWithInfo().find((m) => m.id === modelName) || null;
      this.setField(
        'multiCharacterPrefill',
        defaultMultiCharacterPrefill(
          this.form().provider,
          evaluateThinkingTurn({
            rule: cfg?.thinkingTurnRule ?? null,
            parameters: this.form().parameters,
            model: modelInfo,
          }),
        ),
      );
    }

    // Auto-fill the name for a NEW profile when it's empty (suffixed until unique).
    if (!this.profile()?.id && !this.form().name.trim() && modelName.trim()) {
      const suggested = makeUniqueProfileName(
        `${this.form().provider}/${modelName}`,
        this.takenNames(),
      );
      this.setField('name', suggested);
    }
  }

  /**
   * The base URL as it is allowed to leave the FORM (v4's `outboundBaseUrl`,
   * Bug 73). Every form-driven outbound site reads this rather than
   * `form().baseUrl`; the edit-time model fetch reads the SAVED profile and so
   * resolves its own requirement in {@link autoFetchModelsForEdit}.
   */
  private outbound(): string {
    return outboundBaseUrl(this.providers(), this.form().provider, this.form().baseUrl);
  }

  /**
   * The api key as it is allowed to leave the FORM (v4's `outboundApiKeyId`,
   * Bug 76). Every form-driven outbound site reads this rather than
   * `form().apiKeyId`; the edit-time model fetch reads the SAVED profile and so
   * resolves its own requirement in {@link autoFetchModelsForEdit}.
   */
  private outboundKey(): string {
    return outboundApiKeyId(
      this.providers(),
      this.apiKeys(),
      this.form().provider,
      this.form().apiKeyId,
    );
  }

  private profilePayloadForActions(): Record<string, unknown> {
    const f = this.form();
    return {
      provider: f.provider,
      apiKeyId: this.outboundKey() || undefined,
      baseUrl: this.outbound() || undefined,
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
    // Judged on what may leave, not on what is held: a key the select cannot
    // show is not a key the user chose for this provider, and "API Key is
    // required" is the honest thing to say about a blank control (v4 Bug 76).
    if (this.reqs().requiresApiKey && !this.outboundKey()) {
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
          apiKeyId: this.outboundKey() || undefined,
          baseUrl: this.outbound() || undefined,
        },
        'models',
      );
      this.fetchedModels.set(resp.data.models ?? []);
      this.fetchedModelsWithInfo.set(resp.data.modelsWithInfo ?? []);
      this.modelsMessage.set(`Found ${resp.data.models?.length ?? 0} models`);
      this.correctStoredNullPrefill();
    } catch (err) {
      this.fetchedModels.set([]);
      this.fetchedModelsWithInfo.set([]);
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
          apiKeyId: this.outboundKey() || undefined,
          baseUrl: this.outbound() || undefined,
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

  /**
   * A stored row can carry a base URL its provider does not take — every
   * profile saved before Bug 73 was fixed, and any import. Reading the
   * requirement here rather than the row's truthiness keeps the edit-time model
   * fetch off the wrong endpoint; the next save clears the row. A provider the
   * list does not know about (not loaded, or the fetch failed) keeps its stored
   * URL — absence is not evidence (v4 `ProfileModal.tsx:73-80`).
   */
  private async autoFetchModelsForEdit(p: ConnectionProfileDto): Promise<void> {
    const saved = this.providers().find((cfg) => cfg.name === p.provider);
    const savedTakesBaseUrl = !saved || (saved.configRequirements?.requiresBaseUrl ?? false);
    // The same reading for the api key (v4 Bug 76, `ProfileModal.tsx:84-88`): a
    // row written before that fix — or by import — can carry a key its provider
    // does not take, and probing a keyless endpoint with one is how the
    // mismatch stays invisible. NOTE the default is `?? true` where the base-URL
    // twin defaults `?? false` — keys are the common case.
    const savedTakesApiKey = !saved || providerAcceptsApiKey(saved.configRequirements);
    try {
      const resp = await this.core.dispatchExpect(
        {
          type: 'modelFetch',
          provider: p.provider,
          apiKeyId: (savedTakesApiKey && p.apiKeyId) || undefined,
          baseUrl: (savedTakesBaseUrl && p.baseUrl) || undefined,
        },
        'models',
      );
      this.fetchedModels.set(resp.data.models ?? []);
      this.fetchedModelsWithInfo.set(resp.data.modelsWithInfo ?? []);
      this.modelsMessage.set(`Found ${resp.data.models?.length ?? 0} models`);
      this.correctStoredNullPrefill();
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
    const body = buildProfileRequestBody(this.form(), this.providers(), this.apiKeys());
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
