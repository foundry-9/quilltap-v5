import { ChangeDetectionStrategy, Component, computed, inject, signal } from '@angular/core';
import { injectQuery, injectQueryClient } from '@tanstack/angular-query-experimental';

import { CoreClient } from '../../../core/core-client';
import type { ChatSettingsDto, ConnectionProfileDto } from '../../../core/core-contract';
import { LoadingState } from '../../../ui/loading-state';

type CheapLLMStrategy = 'USER_DEFINED' | 'PROVIDER_CHEAPEST' | 'LOCAL_FIRST';
type EmbeddingProvider = 'SAME_PROVIDER' | 'OPENAI' | 'LOCAL';

interface CheapLLMSettings {
  strategy: CheapLLMStrategy;
  userDefinedProfileId?: string | null;
  defaultCheapProfileId?: string | null;
  fallbackToLocal: boolean;
  /**
   * Whether a cheap route with no connection profile behind it may have a
   * stand-in drafted from the user's `isCheap` profiles when it fails (v4
   * `65f5021c8`). Routes that DO have a profile take their chain from that
   * profile's own Fallback section instead. Optional: a settings bag written
   * before 4.10 does not carry it, and absent reads as off.
   */
  allowCheapFallback?: boolean;
  embeddingProvider: EmbeddingProvider;
}

const DEFAULT_CHEAP: CheapLLMSettings = {
  strategy: 'PROVIDER_CHEAPEST',
  fallbackToLocal: true,
  embeddingProvider: 'SAME_PROVIDER',
};

const STRATEGIES: Array<{ value: CheapLLMStrategy; label: string; description: string }> = [
  { value: 'USER_DEFINED', label: 'User Defined', description: 'Use the profile you select below' },
  {
    value: 'PROVIDER_CHEAPEST',
    label: 'Provider Cheapest',
    description: 'Automatically use the cheapest model from current provider',
  },
  {
    value: 'LOCAL_FIRST',
    label: 'Local First',
    description: 'Prefer local/Ollama models if available',
  },
];

const EMBEDDING_PROVIDERS: Array<{ value: EmbeddingProvider; label: string; description: string }> =
  [
    {
      value: 'SAME_PROVIDER',
      label: 'Same Provider',
      description: 'Use embeddings from the same provider as the cheap LLM',
    },
    { value: 'OPENAI', label: 'OpenAI', description: 'Use OpenAI for embeddings' },
    { value: 'LOCAL', label: 'Local', description: 'Use local Ollama embeddings' },
  ];

/**
 * The Cheap LLM card (v4 `chat-settings/CheapLLMSettings.tsx`): the strategy
 * radios, the user-defined + global-default profile selectors, the fallback
 * toggle, and the embedding-provider radios — all through `chatSettingsUpdate`
 * PUT-merge of `cheapLLMSettings`. Copy carries over verbatim.
 */
@Component({
  selector: 'qt-cheap-llm-card',
  changeDetection: ChangeDetectionStrategy.OnPush,
  imports: [LoadingState],
  template: `
    @if (settingsQuery.isPending()) {
      <qt-loading-state message="Loading settings..." />
    } @else {
      <div class="space-y-4">
        <!-- Strategy -->
        <div>
          <label class="block qt-text-label mb-2">Strategy</label>
          <div class="space-y-2">
            @for (s of strategies; track s.value) {
              <label
                class="flex items-start gap-3 p-3 border qt-border-default rounded qt-hover-accent cursor-pointer transition-colors"
              >
                <input
                  type="radio"
                  name="cheapLLMStrategy"
                  class="mt-1"
                  [checked]="cheap().strategy === s.value"
                  [disabled]="saving()"
                  (change)="update({ strategy: s.value })"
                />
                <div class="flex-1">
                  <div class="font-medium text-sm">{{ s.label }}</div>
                  <div class="qt-text-xs">{{ s.description }}</div>
                </div>
              </label>
            }
          </div>
        </div>

        <!-- User-defined profile -->
        @if (cheap().strategy === 'USER_DEFINED') {
          <div>
            <label class="block qt-text-label mb-2">Select Cheap LLM Profile</label>
            <select
              class="qt-select"
              [disabled]="saving() || profilesQuery.isPending()"
              (change)="update({ userDefinedProfileId: $any($event.target).value || null })"
            >
              <option value="" [selected]="!cheap().userDefinedProfileId">
                Select a profile...
              </option>
              @for (p of profiles(); track p.id) {
                <option [value]="p.id" [selected]="cheap().userDefinedProfileId === p.id">
                  {{ profileLabel(p) }}
                </option>
              }
            </select>
            @if (profiles().length === 0 && !profilesQuery.isPending()) {
              <p class="mt-1 qt-text-xs qt-text-warning">
                No connection profiles found. Create one in the Connection Profiles tab first.
              </p>
            }
          </div>
        }

        <!-- Global default override -->
        <div>
          <label class="block qt-text-label mb-2"
            >Global Default Cheap LLM (Optional Override)</label
          >
          <p class="qt-text-xs mb-2">
            If set, this profile will always be used regardless of strategy
          </p>
          <select
            class="qt-select"
            [disabled]="saving() || profilesQuery.isPending()"
            (change)="update({ defaultCheapProfileId: $any($event.target).value || null })"
          >
            <option value="" [selected]="!cheap().defaultCheapProfileId">Not set</option>
            @for (p of profiles(); track p.id) {
              <option [value]="p.id" [selected]="cheap().defaultCheapProfileId === p.id">
                {{ profileLabel(p) }}
              </option>
            }
          </select>
        </div>

        <!-- Fallback to local -->
        <label
          class="flex items-center gap-3 p-3 border qt-border-default rounded qt-hover-accent cursor-pointer transition-colors"
        >
          <input
            type="checkbox"
            [checked]="cheap().fallbackToLocal"
            [disabled]="saving()"
            (change)="update({ fallbackToLocal: $any($event.target).checked })"
          />
          <div class="flex-1">
            <div class="font-medium text-sm">Fallback to Local</div>
            <div class="qt-text-xs">
              Use local Ollama models as fallback if configured strategy is unavailable
            </div>
          </div>
        </label>

        <!-- Allow a drafted stand-in for profile-less cheap routes -->
        <label
          class="flex items-center gap-3 p-3 border qt-border-default rounded qt-hover-accent cursor-pointer transition-colors"
        >
          <input
            type="checkbox"
            [checked]="cheap().allowCheapFallback ?? false"
            [disabled]="saving()"
            (change)="update({ allowCheapFallback: $any($event.target).checked })"
          />
          <div class="flex-1">
            <div class="font-medium text-sm">Allow a Similar-Tier Stand-In</div>
            <div class="qt-text-xs">
              When the cheap route fails and no connection profile stands behind it — a local
              model, or a route assembled on the spot — permit one understudy to be drafted
              from your inexpensive connections. Routes that <em>do</em> have a profile take
              their understudy from that profile&rsquo;s own Fallback section instead.
            </div>
          </div>
        </label>

        <!-- Embedding provider -->
        <div>
          <label class="block qt-text-label mb-2">Embedding Provider</label>
          <div class="space-y-2">
            @for (e of embeddingProviders; track e.value) {
              <label
                class="flex items-start gap-3 p-3 border qt-border-default rounded qt-hover-accent cursor-pointer transition-colors"
              >
                <input
                  type="radio"
                  name="embeddingProvider"
                  class="mt-1"
                  [checked]="cheap().embeddingProvider === e.value"
                  [disabled]="saving()"
                  (change)="update({ embeddingProvider: e.value })"
                />
                <div class="flex-1">
                  <div class="font-medium text-sm">{{ e.label }}</div>
                  <div class="qt-text-xs">{{ e.description }}</div>
                </div>
              </label>
            }
          </div>
        </div>
      </div>
    }
  `,
})
export class CheapLlmCard {
  private readonly core = inject(CoreClient);
  private readonly queryClient = injectQueryClient();

  protected readonly strategies = STRATEGIES;
  protected readonly embeddingProviders = EMBEDDING_PROVIDERS;
  protected readonly saving = signal(false);

  protected readonly settingsQuery = injectQuery(() => ({
    queryKey: ['chatSettings'],
    queryFn: async (): Promise<ChatSettingsDto> => {
      const resp = await this.core.dispatchExpect({ type: 'chatSettings' }, 'chatSettings');
      return resp.data;
    },
  }));

  protected readonly profilesQuery = injectQuery(() => ({
    queryKey: ['connectionProfiles'],
    queryFn: async (): Promise<ConnectionProfileDto[]> => {
      const resp = await this.core.dispatchExpect(
        { type: 'connectionProfileList' },
        'connectionProfiles',
      );
      return resp.data.profiles;
    },
  }));

  protected readonly profiles = computed(() => this.profilesQuery.data() ?? []);

  protected readonly cheap = computed<CheapLLMSettings>(() => {
    const raw = this.settingsQuery.data()?.['cheapLLMSettings'] as
      Partial<CheapLLMSettings> | undefined;
    return { ...DEFAULT_CHEAP, ...(raw ?? {}) };
  });

  protected profileLabel(p: ConnectionProfileDto): string {
    const warn = p.apiKey ? '' : ' ⚠️ No API Key';
    return `${p.name} (${p.provider} • ${p.modelName})${warn}`;
  }

  protected async update(patch: Partial<CheapLLMSettings>): Promise<void> {
    this.saving.set(true);
    const merged = { ...this.cheap(), ...patch };
    try {
      await this.core.dispatchExpect(
        { type: 'chatSettingsUpdate', settings: { cheapLLMSettings: merged } },
        'chatSettings',
      );
      await this.queryClient.invalidateQueries({ queryKey: ['chatSettings'] });
    } catch {
      // leave the current settings; the next refetch restores server truth
    } finally {
      this.saving.set(false);
    }
  }
}
