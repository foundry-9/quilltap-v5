import { ChangeDetectionStrategy, Component, computed, inject, signal } from '@angular/core';
import { injectQuery, injectQueryClient } from '@tanstack/angular-query-experimental';

import { CoreClient } from '../../../core/core-client';
import type { ImageProfileDto } from '../../../core/core-contract';
import { EmptyState } from '../../../ui/empty-state';
import { ErrorAlert } from '../../../ui/error-alert';
import { LoadingState } from '../../../ui/loading-state';
import { ImageProfileModal } from './image-profile-modal';
import { deleteImageProfile, fetchImageProfiles, imageProfileKeys } from './image-profiles.api';

/** One `key: value` parameter row for display. */
interface ParamRow {
  key: string;
  value: string;
}

/**
 * The Image Profiles card (v4 `components/settings/image-profiles-tab.tsx`): the
 * profile grid (Default / Uncensored badges, model + API-key metadata, the
 * parameters summary), an Edit action, delete-with-inline-confirm, and the
 * create/edit modal launch. The card sorts alphabetically by name (v4). Copy is
 * v4-verbatim.
 */
@Component({
  selector: 'qt-image-profiles-card',
  changeDetection: ChangeDetectionStrategy.OnPush,
  imports: [LoadingState, ErrorAlert, EmptyState, ImageProfileModal],
  template: `
    @if (profilesQuery.isPending()) {
      <qt-loading-state message="Loading image profiles..." />
    } @else {
      @if (profilesQuery.isError()) {
        <qt-error-alert
          [message]="errorMessage()"
          [retryable]="true"
          class="mb-4"
          (retry)="profilesQuery.refetch()"
        />
      }
      @if (banner(); as msg) {
        <div class="qt-alert-error mb-4">{{ msg }}</div>
      }

      <div class="flex items-center justify-between gap-4 mb-4">
        <h2 class="qt-text-section text-foreground flex-1">
          Image Generation Profiles ({{ sortedProfiles().length }})
        </h2>
        <button type="button" class="qt-button-secondary qt-button-sm" (click)="openCreate()">
          New Profile
        </button>
      </div>
      <p class="qt-text-small qt-text-muted mb-4">
        Manage profiles for different image generation providers
      </p>

      @if (sortedProfiles().length === 0) {
        <qt-empty-state
          variant="dashed"
          title="No image profiles yet"
          description="Create a profile to start generating images with AI"
          actionLabel="Create First Profile"
          (action)="openCreate()"
        />
      } @else {
        <div class="qt-card-grid-auto">
          @for (profile of sortedProfiles(); track profile.id) {
            <div class="qt-card p-4">
              <div class="flex items-start justify-between gap-2">
                <h3 class="font-medium truncate">{{ profile.name }}</h3>
                <div class="flex gap-1 flex-shrink-0">
                  @if (profile.isDefault) {
                    <span class="qt-badge-success px-2 py-0.5 rounded qt-text-label-xs">Default</span>
                  }
                  @if (profile.isDangerousCompatible) {
                    <span class="qt-badge-destructive px-2 py-0.5 rounded qt-text-label-xs"
                      >Uncensored</span
                    >
                  }
                </div>
              </div>

              <dl class="mt-2 space-y-1 qt-text-xs">
                <div class="flex gap-2">
                  <dt class="qt-text-muted">Model</dt>
                  <dd class="font-mono truncate">{{ profile.modelName }}</dd>
                </div>
                @if (profile.apiKey) {
                  <div class="flex gap-2">
                    <dt class="qt-text-muted">API Key</dt>
                    <dd class="truncate">{{ profile.apiKey.label }}</dd>
                  </div>
                }
                <div class="flex gap-2">
                  <dt class="qt-text-muted">Provider</dt>
                  <dd>{{ profile.provider }}</dd>
                </div>
              </dl>

              @if (!profile.apiKey) {
                <p class="qt-text-xs qt-text-warning mt-2">⚠️ No API Key</p>
              }

              @if (paramRows(profile).length > 0) {
                <div class="mt-2">
                  <p class="qt-text-label-xs uppercase qt-text-muted mb-1">Parameters</p>
                  <ul class="qt-text-xs space-y-0.5">
                    @for (row of paramRows(profile); track row.key) {
                      <li><span class="font-mono">{{ row.key }}</span>: {{ row.value }}</li>
                    }
                  </ul>
                </div>
              }

              <div class="flex gap-2 mt-3">
                <button
                  type="button"
                  class="qt-button-secondary qt-button-sm"
                  (click)="openEdit(profile)"
                >
                  Edit
                </button>
                @if (deleteConfirmId() === profile.id) {
                  <button
                    type="button"
                    class="qt-button-destructive qt-button-sm"
                    [disabled]="deletingId() === profile.id"
                    (click)="doDelete(profile.id)"
                  >
                    {{ deletingId() === profile.id ? 'Deleting...' : 'Delete this profile?' }}
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
                    class="qt-button-ghost qt-button-sm qt-text-destructive"
                    (click)="deleteConfirmId.set(profile.id)"
                  >
                    Delete
                  </button>
                }
              </div>
            </div>
          }
        </div>
      }
    }

    @if (modalOpen()) {
      <qt-image-profile-modal
        [profile]="editingProfile()"
        (close)="modalOpen.set(false)"
        (saved)="onSaved()"
      />
    }
  `,
})
export class ImageProfilesCard {
  private readonly core = inject(CoreClient);
  private readonly queryClient = injectQueryClient();

  protected readonly modalOpen = signal(false);
  protected readonly editingProfile = signal<ImageProfileDto | null>(null);
  protected readonly deleteConfirmId = signal<string | null>(null);
  protected readonly deletingId = signal<string | null>(null);
  protected readonly banner = signal<string | null>(null);

  protected readonly profilesQuery = injectQuery(() => ({
    queryKey: imageProfileKeys.list(),
    queryFn: (): Promise<ImageProfileDto[]> => fetchImageProfiles(this.core),
  }));

  /** v4's card re-sorts alphabetically by name (the server order is default-first). */
  protected readonly sortedProfiles = computed(() =>
    [...(this.profilesQuery.data() ?? [])].sort((a, b) => a.name.localeCompare(b.name)),
  );

  protected errorMessage(): string {
    const err = this.profilesQuery.error();
    return err instanceof Error ? err.message : 'Failed to load image profiles';
  }

  protected paramRows(profile: ImageProfileDto): ParamRow[] {
    return Object.entries(profile.parameters ?? {}).map(([key, value]) => ({
      key,
      value: typeof value === 'string' ? value : JSON.stringify(value),
    }));
  }

  protected openCreate(): void {
    this.editingProfile.set(null);
    this.modalOpen.set(true);
  }

  protected openEdit(profile: ImageProfileDto): void {
    this.editingProfile.set(profile);
    this.modalOpen.set(true);
  }

  protected onSaved(): void {
    this.modalOpen.set(false);
    void this.queryClient.invalidateQueries({ queryKey: imageProfileKeys.list() });
  }

  protected async doDelete(id: string): Promise<void> {
    this.deletingId.set(id);
    this.banner.set(null);
    try {
      await deleteImageProfile(this.core, id);
      this.deleteConfirmId.set(null);
      await this.queryClient.invalidateQueries({ queryKey: imageProfileKeys.list() });
    } catch (err) {
      this.banner.set(err instanceof Error && err.message ? err.message : 'An error occurred');
    } finally {
      this.deletingId.set(null);
    }
  }
}
