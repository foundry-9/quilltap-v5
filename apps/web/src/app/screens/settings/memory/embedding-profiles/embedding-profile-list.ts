import { NgTemplateOutlet } from '@angular/common';
import { ChangeDetectionStrategy, Component, computed, inject, input, output, signal } from '@angular/core';
import { RouterLink } from '@angular/router';

import { CoreClient, coreErrorMessage } from '../../../../core/core-client';
import type { EmbeddingProfileDto } from '../../../../core/core-contract';
import { notifyQueueChange } from '../../../../layout/queue-status.logic';
import { EmptyState } from '../../../../ui/empty-state';
import { ErrorAlert } from '../../../../ui/error-alert';
import { EmbeddingProviderBadge } from './embedding-provider-badge';

/** One metadata row for a profile card. */
interface MetaRow {
  label: string;
  value: string;
  mono?: boolean;
}

/**
 * The profile list (v4 `components/settings/embedding-profiles/ProfileList.tsx`):
 * the alphabetical card grid with provider/missing-key badges, embedded stats,
 * the maintenance actions (Refit / Re-embed Everything / Re-apply (Matryoshka) /
 * Re-embed Mismatched — all `isDefault`-gated) with their two-step confirms and
 * success banners (an inline `qt-alert-success` with a "View Tasks Queue" link,
 * NOT a toast), and the two-step delete. Every enqueueing success calls
 * `notifyQueueChange()`.
 */
@Component({
  selector: 'qt-embedding-profile-list',
  changeDetection: ChangeDetectionStrategy.OnPush,
  imports: [NgTemplateOutlet, RouterLink, EmptyState, ErrorAlert, EmbeddingProviderBadge],
  template: `
    @if (sortedProfiles().length === 0) {
      <qt-empty-state
        title="No embedding profiles yet"
        description="Embedding profiles are used for semantic search in memories"
        actionLabel="Create First Profile"
        (action)="create.emit()"
      />
    } @else {
      <div class="space-y-4">
        @if (deleteError()) {
          <qt-error-alert [message]="deleteError()!" [retryable]="true" (retry)="deleteError.set(null)" />
        }
        @if (refitError()) {
          <qt-error-alert [message]="refitError()!" [retryable]="true" (retry)="refitError.set(null)" />
        }
        @if (reapplyError()) {
          <qt-error-alert [message]="reapplyError()!" [retryable]="true" (retry)="reapplyError.set(null)" />
        }
        @if (reindexMismatchedError()) {
          <qt-error-alert
            [message]="reindexMismatchedError()!"
            [retryable]="true"
            (retry)="reindexMismatchedError.set(null)"
          />
        }

        @if (reindexMismatchedSuccess()) {
          <ng-container
            [ngTemplateOutlet]="successBanner"
            [ngTemplateOutletContext]="{ msg: reindexMismatchedSuccess(), clear: clearReindexMismatched }"
          />
        }
        @if (reapplySuccess()) {
          <ng-container
            [ngTemplateOutlet]="successBanner"
            [ngTemplateOutletContext]="{ msg: reapplySuccess(), clear: clearReapply }"
          />
        }
        @if (refitSuccess()) {
          <ng-container
            [ngTemplateOutlet]="successBanner"
            [ngTemplateOutletContext]="{ msg: refitSuccess(), clear: clearRefit }"
          />
        }

        <div class="qt-card-grid-auto">
          @for (profile of sortedProfiles(); track profile.id) {
            <div class="qt-card p-4">
              <div class="flex items-start justify-between gap-2">
                <h3 class="font-medium truncate">{{ profile.name }}</h3>
                @if (profile.isDefault) {
                  <span class="qt-badge-success px-2 py-0.5 rounded qt-text-label-xs flex-shrink-0">Default</span>
                }
              </div>

              <div class="flex flex-col gap-2 mt-1 mb-2">
                <div class="flex items-center gap-2">
                  <qt-embedding-provider-badge [provider]="profile.provider" />
                  @if (needsApiKey(profile.provider) && !profile.apiKey) {
                    <span class="qt-badge-warning">⚠️ No API Key</span>
                  }
                </div>

                @if (profile.provider === 'BUILTIN' && !profile.vocabularyStats) {
                  <p class="qt-text-xs qt-text-secondary">
                    Vocabulary not yet fitted. Add some memories to get started.
                  </p>
                }

                @if (profile.embeddingStats && profile.embeddingStats.total > 0) {
                  <div class="qt-text-xs qt-text-secondary">
                    Embedded: {{ profile.embeddingStats.embedded }}/{{ profile.embeddingStats.total }}
                    @if (profile.embeddingStats.pending > 0) {
                      <span class="qt-text-warning ml-2">({{ profile.embeddingStats.pending }} pending)</span>
                    }
                    @if (profile.embeddingStats.failed > 0) {
                      <span class="qt-text-destructive ml-2">({{ profile.embeddingStats.failed }} failed)</span>
                    }
                  </div>
                }
              </div>

              @if (metadataRows(profile).length > 0) {
                <dl class="mt-2 space-y-1 qt-text-xs">
                  @for (row of metadataRows(profile); track row.label) {
                    <div class="flex gap-2">
                      <dt class="qt-text-muted">{{ row.label }}</dt>
                      <dd [class]="row.mono ? 'font-mono truncate' : 'truncate'">{{ row.value }}</dd>
                    </div>
                  }
                </dl>
              }

              <div class="flex flex-wrap gap-2 mt-3">
                <button type="button" class="qt-button-secondary qt-button-sm" (click)="edit.emit(profile)">
                  Edit
                </button>

                @if (profile.isDefault) {
                  <button
                    type="button"
                    class="qt-button-secondary qt-button-sm"
                    [disabled]="refitLoading()"
                    (click)="refit(profile)"
                  >
                    {{ profile.provider === 'BUILTIN' ? 'Refit Vocabulary' : 'Re-embed Everything' }}
                  </button>
                }

                @if (profile.isDefault && profile.truncateToDimensions) {
                  <button
                    type="button"
                    class="qt-button-secondary qt-button-sm"
                    (click)="reapplyClick(profile)"
                  >
                    @if (reapplyConfirmingId() === profile.id) {
                      {{ reapplyLoading() ? 'Queuing…' : 'Confirm: slice to ' + profile.truncateToDimensions + 'd' }}
                    } @else {
                      Re-apply (Matryoshka)
                    }
                  </button>
                }

                @if (canReindexMismatched(profile)) {
                  <button
                    type="button"
                    class="qt-button-secondary qt-button-sm"
                    (click)="reindexMismatchedClick(profile)"
                  >
                    @if (reindexMismatchedConfirmingId() === profile.id) {
                      {{ reindexMismatchedLoading() ? 'Queuing…' : 'Confirm: re-embed misfits at ' + mismatchedTargetDim(profile) + 'd' }}
                    } @else {
                      Re-embed Mismatched
                    }
                  </button>
                }

                @if (deleteConfirmingId() === profile.id) {
                  <button
                    type="button"
                    class="qt-button-destructive qt-button-sm"
                    [disabled]="deleteLoading()"
                    (click)="doDelete(profile.id)"
                  >
                    {{ deleteLoading() ? 'Deleting…' : 'Delete this profile?' }}
                  </button>
                  <button
                    type="button"
                    class="qt-button-ghost qt-button-sm"
                    (click)="deleteConfirmingId.set(null)"
                  >
                    Cancel
                  </button>
                } @else {
                  <button
                    type="button"
                    class="qt-button-ghost qt-button-sm qt-text-destructive"
                    (click)="deleteConfirmingId.set(profile.id)"
                  >
                    Delete
                  </button>
                }
              </div>
            </div>
          }
        </div>
      </div>
    }

    <ng-template #successBanner let-msg="msg" let-clear="clear">
      <div class="qt-alert-success">
        <div class="flex items-center justify-between gap-4">
          <p class="qt-label flex-1">{{ msg }}</p>
          <div class="flex items-center gap-2 flex-shrink-0">
            <a [routerLink]="['/settings']" [queryParams]="{ tab: 'system' }" class="qt-button-ghost qt-button-sm">
              View Tasks Queue
            </a>
            <button type="button" class="qt-button-ghost qt-button-sm" (click)="clear()">Dismiss</button>
          </div>
        </div>
      </div>
    </ng-template>
  `,
})
export class EmbeddingProfileList {
  private readonly core = inject(CoreClient);

  readonly profiles = input<EmbeddingProfileDto[]>([]);
  readonly edit = output<EmbeddingProfileDto>();
  readonly create = output<void>();
  /** Refetch the list (after a delete). */
  readonly changed = output<void>();

  /** v4 sorts the grid alphabetically by name. */
  protected readonly sortedProfiles = computed(() =>
    [...this.profiles()].sort((a, b) => a.name.localeCompare(b.name)),
  );

  protected readonly deleteConfirmingId = signal<string | null>(null);
  protected readonly deleteLoading = signal(false);
  protected readonly deleteError = signal<string | null>(null);

  protected readonly refitLoading = signal(false);
  protected readonly refitError = signal<string | null>(null);
  protected readonly refitSuccess = signal<string | null>(null);

  protected readonly reapplyConfirmingId = signal<string | null>(null);
  protected readonly reapplyLoading = signal(false);
  protected readonly reapplyError = signal<string | null>(null);
  protected readonly reapplySuccess = signal<string | null>(null);

  protected readonly reindexMismatchedConfirmingId = signal<string | null>(null);
  protected readonly reindexMismatchedLoading = signal(false);
  protected readonly reindexMismatchedError = signal<string | null>(null);
  protected readonly reindexMismatchedSuccess = signal<string | null>(null);

  // Bound clearers for the shared success-banner template.
  protected readonly clearRefit = (): void => this.refitSuccess.set(null);
  protected readonly clearReapply = (): void => this.reapplySuccess.set(null);
  protected readonly clearReindexMismatched = (): void => this.reindexMismatchedSuccess.set(null);

  /** v4 :152 — only OPENAI / OPENROUTER need an API key. */
  protected needsApiKey(provider: string): boolean {
    return provider === 'OPENAI' || provider === 'OPENROUTER';
  }

  /** v4 :323-330 — the partial-reindex target dim (truncate ?? dimensions). */
  protected mismatchedTargetDim(profile: EmbeddingProfileDto): number {
    return profile.truncateToDimensions ?? profile.dimensions ?? 0;
  }

  protected canReindexMismatched(profile: EmbeddingProfileDto): boolean {
    const dim = this.mismatchedTargetDim(profile);
    return profile.isDefault && profile.provider !== 'BUILTIN' && dim > 0;
  }

  protected metadataRows(profile: EmbeddingProfileDto): MetaRow[] {
    const rows: MetaRow[] = [];
    if (profile.provider !== 'BUILTIN') {
      rows.push({ label: 'Model', value: profile.modelName, mono: true });
    }
    if (profile.dimensions) {
      rows.push({ label: 'Dimensions', value: String(profile.dimensions) });
    }
    if (profile.truncateToDimensions) {
      rows.push({ label: 'Truncate to', value: `${profile.truncateToDimensions} (Matryoshka)` });
    }
    if (profile.apiKey) {
      rows.push({ label: 'API Key', value: profile.apiKey.label });
    }
    if (profile.baseUrl) {
      rows.push({ label: 'Base URL', value: profile.baseUrl });
    }
    if (profile.provider === 'BUILTIN' && profile.vocabularyStats) {
      rows.push({
        label: 'Vocabulary',
        value: `${profile.vocabularyStats.vocabularySize.toLocaleString()} terms`,
      });
      rows.push({ label: 'Last Fit', value: new Date(profile.vocabularyStats.fittedAt).toLocaleString() });
    }
    return rows;
  }

  /**
   * v4 `handleRefit`: BUILTIN → `?action=refit`, else `?action=reindex` (no
   * scope). Single click, no confirm; a local success banner (not a toast).
   */
  protected async refit(profile: EmbeddingProfileDto): Promise<void> {
    this.refitSuccess.set(null);
    this.refitError.set(null);
    this.refitLoading.set(true);
    try {
      if (profile.provider === 'BUILTIN') {
        await this.core.refitEmbeddingProfile(profile.id);
      } else {
        await this.core.reindexEmbeddingProfile(profile.id);
      }
      notifyQueueChange();
      this.refitSuccess.set(
        profile.provider === 'BUILTIN'
          ? 'Vocabulary refit job queued. Help docs, memories, and conversations will be re-embedded. Track progress via the Emb badge.'
          : 'Re-embedding everything — help docs, memories, and conversations. Track progress via the Emb badge.',
      );
    } catch (err) {
      this.refitError.set(
        coreErrorMessage(err, `Failed to trigger ${profile.provider === 'BUILTIN' ? 'refit' : 'reindex'}`),
      );
    } finally {
      this.refitLoading.set(false);
    }
  }

  /** Two-step: first click arms the confirm, second runs the Matryoshka re-apply. */
  protected reapplyClick(profile: EmbeddingProfileDto): void {
    if (this.reapplyConfirmingId() === profile.id) {
      void this.reapply(profile);
    } else {
      this.reapplyConfirmingId.set(profile.id);
    }
  }

  private async reapply(profile: EmbeddingProfileDto): Promise<void> {
    this.reapplySuccess.set(null);
    this.reapplyError.set(null);
    this.reapplyLoading.set(true);
    try {
      await this.core.reapplyEmbeddingProfile(profile.id);
      notifyQueueChange();
      this.reapplyConfirmingId.set(null);
      this.reapplySuccess.set(
        `Matryoshka re-apply queued. Stored vectors will be sliced to ${profile.truncateToDimensions}d and renormalized. ` +
          'A backup of each affected database is taken before any rewrite. Track progress via the Emb badge.',
      );
    } catch (err) {
      this.reapplyError.set(coreErrorMessage(err, 'Failed to trigger re-apply'));
    } finally {
      this.reapplyLoading.set(false);
    }
  }

  /** Two-step: first click arms, second runs the mismatched-dim reindex. */
  protected reindexMismatchedClick(profile: EmbeddingProfileDto): void {
    if (this.reindexMismatchedConfirmingId() === profile.id) {
      void this.reindexMismatched(profile);
    } else {
      this.reindexMismatchedConfirmingId.set(profile.id);
    }
  }

  private async reindexMismatched(profile: EmbeddingProfileDto): Promise<void> {
    this.reindexMismatchedSuccess.set(null);
    this.reindexMismatchedError.set(null);
    const targetDim = this.mismatchedTargetDim(profile);
    this.reindexMismatchedLoading.set(true);
    try {
      await this.core.reindexEmbeddingProfile(profile.id, 'mismatched-dim');
      notifyQueueChange();
      this.reindexMismatchedConfirmingId.set(null);
      this.reindexMismatchedSuccess.set(
        `Partial re-embedding queued. Only rows whose stored vector dim differs from ${targetDim}d will be re-embedded against this profile. Track progress via the Emb badge.`,
      );
    } catch (err) {
      this.reindexMismatchedError.set(coreErrorMessage(err, 'Failed to trigger partial reindex'));
    } finally {
      this.reindexMismatchedLoading.set(false);
    }
  }

  protected async doDelete(id: string): Promise<void> {
    this.deleteError.set(null);
    this.deleteLoading.set(true);
    try {
      await this.core.deleteEmbeddingProfile(id);
      this.deleteConfirmingId.set(null);
      this.changed.emit();
    } catch (err) {
      this.deleteError.set(coreErrorMessage(err, 'Failed to delete profile'));
    } finally {
      this.deleteLoading.set(false);
    }
  }
}
