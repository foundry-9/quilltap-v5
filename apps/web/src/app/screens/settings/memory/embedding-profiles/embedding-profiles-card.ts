import { ChangeDetectionStrategy, Component, computed, inject, signal } from '@angular/core';
import { injectQuery, injectQueryClient } from '@tanstack/angular-query-experimental';

import { CoreClient } from '../../../../core/core-client';
import type { EmbeddingProfileDto } from '../../../../core/core-contract';
import { ErrorAlert } from '../../../../ui/error-alert';
import { LoadingState } from '../../../../ui/loading-state';
import { SectionHeader } from '../../../../ui/section-header';
import { EmbeddingProfileList } from './embedding-profile-list';
import { EmbeddingProfileModal } from './embedding-profile-modal';
import { buildProviderInfos, type EmbeddingProfilesMeta } from './embedding-profiles.types';

const PROFILE_KEYS = {
  list: ['embedding-profiles', 'list'] as const,
  meta: ['embedding-profiles', 'meta'] as const,
};

/**
 * The Embedding Profiles card (v4 `components/settings/embedding-profiles/
 * index.tsx`): the header + "New Profile" action, the profile list, and the
 * create/edit modal. Loads the profile list (fatal) and, once, the trio the
 * modal needs — API keys, the static model map, and the provider registry (each
 * non-fatal, degrading to empty; v4 `loadData`).
 *
 * DEFERRED LOUDLY (the `p4.9h2` pool's remaining slice): v4's `triggerAutoAssociate`
 * mount effect (tag machinery for template-profile pickers) has no v5 analog and
 * is NOT invented here — it lands with the prompt-library / tag slice.
 */
@Component({
  selector: 'qt-embedding-profiles-card',
  changeDetection: ChangeDetectionStrategy.OnPush,
  imports: [SectionHeader, LoadingState, ErrorAlert, EmbeddingProfileList, EmbeddingProfileModal],
  template: `
    @if (profilesQuery.isPending()) {
      <qt-loading-state message="Loading embedding profiles..." />
    } @else {
      <div class="space-y-6">
        <div>
          <qt-section-header title="Embedding Profiles" level="h2">
            <button type="button" class="qt-button-secondary qt-button-sm" (click)="openCreate()">
              New Profile
            </button>
          </qt-section-header>
          <p class="qt-text-small qt-text-secondary">
            Manage text embedding connections for semantic search (OpenAI or Ollama)
          </p>
        </div>

        @if (profilesQuery.isError()) {
          <qt-error-alert [message]="errorMessage()" [retryable]="true" (retry)="reload()" />
        }

        <qt-embedding-profile-list
          [profiles]="profiles()"
          (edit)="openEdit($event)"
          (create)="openCreate()"
          (changed)="refetchList()"
        />
      </div>
    }

    @if (modalOpen()) {
      <qt-embedding-profile-modal
        [profile]="editingProfile()"
        [apiKeys]="meta().apiKeys"
        [embeddingModels]="meta().models"
        [embeddingProviders]="meta().providers"
        (saved)="refetchList()"
        (close)="modalOpen.set(false)"
      />
    }
  `,
})
export class EmbeddingProfilesCard {
  private readonly core = inject(CoreClient);
  private readonly queryClient = injectQueryClient();

  protected readonly modalOpen = signal(false);
  protected readonly editingProfile = signal<EmbeddingProfileDto | null>(null);

  protected readonly profilesQuery = injectQuery(() => ({
    queryKey: PROFILE_KEYS.list,
    queryFn: (): Promise<EmbeddingProfileDto[]> => this.core.listEmbeddingProfiles(),
  }));

  /** The modal's trio (v4 `loadData`'s non-profile fetches); each degrades to empty. */
  private readonly metaQuery = injectQuery(() => ({
    queryKey: PROFILE_KEYS.meta,
    queryFn: async (): Promise<EmbeddingProfilesMeta> => {
      const [apiKeys, models, providerNames] = await Promise.all([
        this.core
          .dispatchExpect({ type: 'apiKeyList' }, 'apiKeys')
          .then((r) => r.data.apiKeys)
          .catch(() => []),
        this.core.listEmbeddingModels().catch(() => ({})),
        this.core.listEmbeddingProviders().catch(() => []),
      ]);
      // v4 builds the provider list from the registry, else from the model keys.
      const providers =
        providerNames.length > 0
          ? buildProviderInfos(providerNames)
          : buildProviderInfos(Object.keys(models));
      return { apiKeys, models, providers };
    },
  }));

  protected readonly profiles = computed(() => this.profilesQuery.data() ?? []);
  protected readonly meta = computed<EmbeddingProfilesMeta>(
    () => this.metaQuery.data() ?? { apiKeys: [], models: {}, providers: [] },
  );

  protected errorMessage(): string {
    const err = this.profilesQuery.error();
    return err instanceof Error && err.message ? err.message : 'Failed to fetch profiles';
  }

  protected openCreate(): void {
    this.editingProfile.set(null);
    this.modalOpen.set(true);
  }

  protected openEdit(profile: EmbeddingProfileDto): void {
    this.editingProfile.set(profile);
    this.modalOpen.set(true);
  }

  protected refetchList(): void {
    void this.queryClient.invalidateQueries({ queryKey: PROFILE_KEYS.list });
  }

  protected reload(): void {
    // v4 :79-81 — the load-error retry is a full reload.
    if (typeof window !== 'undefined') {
      window.location.reload();
    }
  }
}
