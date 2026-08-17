import { ChangeDetectionStrategy, Component, computed, inject, signal } from '@angular/core';
import { injectQuery, injectQueryClient } from '@tanstack/angular-query-experimental';

import { CoreClient } from '../../../core/core-client';
import type { ApiKeyDto, ConnectionProfileDto, ProviderInfo } from '../../../core/core-contract';
import { EmptyState } from '../../../ui/empty-state';
import { ErrorAlert } from '../../../ui/error-alert';
import { Icon } from '../../../ui/icon';
import { LoadingState } from '../../../ui/loading-state';
import { getModelClass, normalizeProfileName } from './model-classes';
import { ProfileModal } from './profile-modal';

interface Badge {
  text: string;
  cls: string;
}

/**
 * The Connection Profiles card (v4 `components/settings/connection-profiles/`):
 * the sortable list (badges, provider/model, up/down reorder + Reset Sort),
 * create/edit over the profile modal, tag pills, and delete-with-confirm. Drag
 * reordering is replaced by up/down buttons (no dnd dependency); the
 * message-count heuristic is a named deferral. Copy + `qt-*` classes carry over
 * verbatim.
 *
 * Tags render from the LIST enrichment's `{ tagId, tag }` envelope, NOT a flat
 * tag — reading `name` off the envelope is what drew every pill empty in v4
 * (Bug 74's third layer, `ProfileCard.tsx:163-168` at `d123658d`). Editing them
 * is the modal's job, over the item route's own verbs.
 */
@Component({
  selector: 'qt-connection-profiles-card',
  changeDetection: ChangeDetectionStrategy.OnPush,
  imports: [LoadingState, ErrorAlert, EmptyState, Icon, ProfileModal],
  template: `
    <div>
      @if (profilesQuery.isPending()) {
        <qt-loading-state message="Loading connection profiles..." />
      } @else {
        @if (profilesQuery.isError()) {
          <qt-error-alert
            [message]="errorMessage()"
            [retryable]="true"
            class="mb-4"
            (retry)="profilesQuery.refetch()"
          />
        }

        @if (apiKeys().length === 0) {
          <div class="qt-alert-warning mb-6">
            <p class="font-medium">No API keys found</p>
            <p class="qt-text-small">
              Add an API key in the &quot;API Keys&quot; tab before creating a connection profile.
            </p>
          </div>
        }

        <div class="mb-2">
          <div class="flex items-center justify-between gap-4 mb-4">
            <h2 class="qt-text-section text-foreground flex-1">
              Connection Profiles ({{ sortedProfiles().length }})
            </h2>
            <button type="button" class="qt-button-secondary qt-button-sm" (click)="openCreate()">
              + Add Profile
            </button>
          </div>

          @if (sortedProfiles().length > 1) {
            <div class="flex justify-end mb-3 -mt-2">
              <button type="button" class="qt-button-secondary qt-button-sm" (click)="resetSort()">
                Reset Sort Order
              </button>
            </div>
          }

          @if (sortedProfiles().length === 0) {
            <qt-empty-state
              variant="dashed"
              title="No connection profiles yet"
              description="Create one to start chatting."
              actionLabel="Create Profile"
              (action)="openCreate()"
            />
          } @else {
            <div class="space-y-3">
              @for (profile of sortedProfiles(); track profile.id; let i = $index) {
                <div class="qt-card p-4">
                  <div class="flex items-start justify-between gap-3">
                    <div class="flex items-start gap-2 min-w-0">
                      <div class="flex flex-col gap-1 pt-0.5">
                        <button
                          type="button"
                          class="qt-button-ghost qt-button-sm p-0.5"
                          aria-label="Move up"
                          [disabled]="i === 0"
                          (click)="move(i, -1)"
                        >
                          <qt-icon name="arrow-up" class="w-4 h-4" />
                        </button>
                        <button
                          type="button"
                          class="qt-button-ghost qt-button-sm p-0.5"
                          aria-label="Move down"
                          [disabled]="i === sortedProfiles().length - 1"
                          (click)="move(i, 1)"
                        >
                          <qt-icon name="arrow-down" class="w-4 h-4" />
                        </button>
                      </div>
                      <div class="min-w-0">
                        <div class="flex items-center gap-2 flex-wrap">
                          <h3 class="font-medium truncate">{{ profile.name }}</h3>
                          @for (badge of badgesFor(profile); track badge.text) {
                            <span [class]="'qt-badge ' + badge.cls">{{ badge.text }}</span>
                          }
                        </div>
                        <p class="qt-text-small qt-text-muted">
                          {{ profile.provider }} • {{ profile.modelName }}
                        </p>
                        @if (missingApiKey(profile)) {
                          <p class="qt-text-xs qt-text-warning mt-1">⚠️ No API key linked</p>
                        }
                        @if (profile.apiKey) {
                          <p class="qt-text-small">API Key: {{ profile.apiKey.label }}</p>
                        }
                        @if (profile.baseUrl) {
                          <p class="qt-text-small">Base URL: {{ profile.baseUrl }}</p>
                        }
                        <div class="qt-text-xs mt-2">
                          Temperature: {{ paramOf(profile, 'temperature', 0.7) }} • Max Tokens:
                          {{ paramOf(profile, 'max_tokens', 1000) }} • Top P:
                          {{ paramOf(profile, 'top_p', 1) }}
                        </div>
                        @if (profile.tags && profile.tags.length > 0) {
                          <div class="mt-3 flex flex-wrap gap-2">
                            @for (entry of profile.tags; track entry.tagId) {
                              <span class="qt-tag-badge qt-tag-badge-sm">{{ entry.tag.name }}</span>
                            }
                          </div>
                        }
                      </div>
                    </div>
                    <div class="flex gap-2 flex-shrink-0">
                      <button
                        type="button"
                        class="qt-button-primary qt-button-sm"
                        (click)="openEdit(profile)"
                      >
                        Edit
                      </button>
                      @if (deleteConfirmId() === profile.id) {
                        <button
                          type="button"
                          class="qt-button-destructive qt-button-sm"
                          [disabled]="deletingId() === profile.id"
                          (click)="confirmDelete(profile.id)"
                        >
                          {{ deletingId() === profile.id ? 'Deleting...' : 'Confirm' }}
                        </button>
                        <button
                          type="button"
                          class="qt-button-ghost qt-button-sm"
                          (click)="deleteConfirmId.set(null)"
                        >
                          Cancel
                        </button>
                      } @else {
                        <button
                          type="button"
                          class="qt-button-ghost qt-button-sm"
                          (click)="deleteConfirmId.set(profile.id)"
                        >
                          Delete
                        </button>
                      }
                    </div>
                  </div>
                </div>
              }
            </div>
          }
        </div>
      }

      @if (modalOpen()) {
        <qt-profile-modal
          [profile]="editingProfile()"
          [providers]="providers()"
          [apiKeys]="apiKeys()"
          [takenNames]="takenNames()"
          (close)="closeModal()"
          (saved)="onSaved()"
        />
      }
    </div>
  `,
})
export class ConnectionProfilesCard {
  private readonly core = inject(CoreClient);
  private readonly queryClient = injectQueryClient();

  protected readonly modalOpen = signal(false);
  protected readonly editingProfile = signal<ConnectionProfileDto | null>(null);
  protected readonly deleteConfirmId = signal<string | null>(null);
  protected readonly deletingId = signal<string | null>(null);

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

  private readonly providersQuery = injectQuery(() => ({
    queryKey: ['providers'],
    queryFn: async (): Promise<ProviderInfo[]> => {
      const resp = await this.core.dispatchExpect({ type: 'providerList' }, 'providers');
      return resp.data.providers;
    },
  }));

  private readonly apiKeysQuery = injectQuery(() => ({
    queryKey: ['apiKeys'],
    queryFn: async (): Promise<ApiKeyDto[]> => {
      const resp = await this.core.dispatchExpect({ type: 'apiKeyList' }, 'apiKeys');
      return resp.data.apiKeys;
    },
  }));

  protected readonly providers = computed(() => this.providersQuery.data() ?? []);
  protected readonly apiKeys = computed(() => this.apiKeysQuery.data() ?? []);

  protected readonly sortedProfiles = computed(() =>
    [...(this.profilesQuery.data() ?? [])].sort((a, b) => {
      const ai = a.sortIndex ?? 0;
      const bi = b.sortIndex ?? 0;
      return ai !== bi ? ai - bi : a.name.localeCompare(b.name);
    }),
  );

  protected readonly takenNames = computed(
    () =>
      new Set(
        (this.profilesQuery.data() ?? [])
          .filter((p) => p.id !== this.editingProfile()?.id)
          .map((p) => normalizeProfileName(p.name)),
      ),
  );

  protected errorMessage(): string {
    const err = this.profilesQuery.error();
    return err instanceof Error ? err.message : 'Failed to fetch profiles';
  }

  protected providerRequiresApiKey(providerName: string): boolean {
    return (
      this.providers().find((p) => p.name === providerName)?.configRequirements?.requiresApiKey ??
      true
    );
  }

  protected missingApiKey(profile: ConnectionProfileDto): boolean {
    return this.providerRequiresApiKey(profile.provider) && !profile.apiKey;
  }

  protected paramOf(profile: ConnectionProfileDto, key: string, fallback: number): number | string {
    const v = (profile.parameters ?? {})[key];
    return v === undefined || v === null ? fallback : (v as number);
  }

  protected badgesFor(profile: ConnectionProfileDto): Badge[] {
    const badges: Badge[] = [];
    if (profile.isDefault) badges.push({ text: 'Default', cls: 'qt-badge-default' });
    if (profile.isCheap) badges.push({ text: 'Cheap', cls: 'qt-badge-cheap' });
    if (profile.isDangerousCompatible)
      badges.push({ text: 'Uncensored', cls: 'qt-badge-destructive' });
    if (profile.allowToolUse === false)
      badges.push({ text: 'No Tools', cls: 'qt-badge-destructive' });
    if (profile.modelClass) {
      const mc = getModelClass(profile.modelClass);
      if (mc) badges.push({ text: `${mc.name} (Tier ${mc.tier})`, cls: 'qt-badge-default' });
    }
    return badges;
  }

  protected openCreate(): void {
    this.editingProfile.set(null);
    this.modalOpen.set(true);
  }

  protected openEdit(profile: ConnectionProfileDto): void {
    this.editingProfile.set(profile);
    this.modalOpen.set(true);
  }

  protected closeModal(): void {
    this.modalOpen.set(false);
    this.editingProfile.set(null);
  }

  protected onSaved(): void {
    void this.queryClient.invalidateQueries({ queryKey: ['connectionProfiles'] });
    void this.queryClient.invalidateQueries({ queryKey: ['apiKeys'] });
  }

  protected async move(index: number, direction: -1 | 1): Promise<void> {
    const list = [...this.sortedProfiles()];
    const target = index + direction;
    if (target < 0 || target >= list.length) {
      return;
    }
    const [moved] = list.splice(index, 1);
    list.splice(target, 0, moved);
    await this.reorder(list.map((p) => p.id));
  }

  private async reorder(orderedIds: string[]): Promise<void> {
    try {
      await this.core.dispatchExpect({ type: 'connectionProfileReorder', orderedIds }, 'ack');
      await this.queryClient.invalidateQueries({ queryKey: ['connectionProfiles'] });
    } catch {
      // leave the list as-is; the next refetch restores the server order
    }
  }

  protected async resetSort(): Promise<void> {
    try {
      await this.core.dispatchExpect({ type: 'connectionProfileResetSort' }, 'ack');
      await this.queryClient.invalidateQueries({ queryKey: ['connectionProfiles'] });
    } catch {
      // no-op
    }
  }

  protected async confirmDelete(id: string): Promise<void> {
    this.deletingId.set(id);
    try {
      await this.core.dispatchExpect({ type: 'connectionProfileDelete', profileId: id }, 'ack');
      this.deleteConfirmId.set(null);
      await this.queryClient.invalidateQueries({ queryKey: ['connectionProfiles'] });
    } catch {
      // leave confirm open
    } finally {
      this.deletingId.set(null);
    }
  }
}
