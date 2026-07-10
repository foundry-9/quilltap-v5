import { ChangeDetectionStrategy, Component, computed, inject, signal } from '@angular/core';
import { injectQuery, injectQueryClient } from '@tanstack/angular-query-experimental';

import { CoreClient } from '../../../core/core-client';
import type { ApiKeyDto } from '../../../core/core-contract';
import { EmptyState } from '../../../ui/empty-state';
import { ErrorAlert } from '../../../ui/error-alert';
import { LoadingState } from '../../../ui/loading-state';
import { ApiKeyModal } from './api-key-modal';

/**
 * The API Keys card (v4 `components/settings/api-keys-tab.tsx`): the masked-key
 * list (`keyPreview`), an Add modal, per-key Test → `{valid,error?}`, and
 * delete-with-confirm. Export/Import are a named deferral. Copy is v4-verbatim.
 */
@Component({
  selector: 'qt-api-keys-card',
  changeDetection: ChangeDetectionStrategy.OnPush,
  imports: [LoadingState, ErrorAlert, EmptyState, ApiKeyModal],
  template: `
    <div>
      @if (keysQuery.isPending()) {
        <qt-loading-state message="Loading API keys..." />
      } @else {
        @if (keysQuery.isError()) {
          <qt-error-alert
            [message]="errorMessage()"
            [retryable]="true"
            class="mb-4"
            (retry)="keysQuery.refetch()"
          />
        }

        <div class="mb-2">
          <div class="flex items-center justify-between gap-4 mb-4">
            <h2 class="qt-text-section text-foreground flex-1">
              Your API Keys ({{ sortedKeys().length }})
            </h2>
            <div class="flex gap-2 flex-shrink-0">
              <button
                type="button"
                class="qt-button-secondary qt-button-sm"
                (click)="modalOpen.set(true)"
              >
                + Add API Key
              </button>
            </div>
          </div>

          @if (sortedKeys().length === 0) {
            <qt-empty-state
              variant="dashed"
              title="No API keys yet"
              description="Add one to get started."
              actionLabel="Add API Key"
              (action)="modalOpen.set(true)"
            />
          } @else {
            <div class="qt-card-grid-auto">
              @for (key of sortedKeys(); track key.id) {
                <div class="qt-card p-4">
                  <div class="flex items-start justify-between gap-2">
                    <div class="min-w-0">
                      <h3 class="font-medium truncate">{{ key.label }}</h3>
                      <p class="qt-text-small qt-text-muted">
                        {{ key.provider }} • {{ key.keyPreview }}
                      </p>
                    </div>
                    <div class="flex gap-2 flex-shrink-0">
                      <button
                        type="button"
                        class="qt-button-secondary qt-button-sm"
                        [disabled]="testingId() === key.id"
                        (click)="test(key.id)"
                      >
                        {{ testingId() === key.id ? 'Testing...' : 'Test' }}
                      </button>
                      @if (deleteConfirmId() === key.id) {
                        <button
                          type="button"
                          class="qt-button-destructive qt-button-sm"
                          [disabled]="deletingId() === key.id"
                          (click)="confirmDelete(key.id)"
                        >
                          {{ deletingId() === key.id ? 'Deleting...' : 'Confirm' }}
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
                          (click)="deleteConfirmId.set(key.id)"
                        >
                          Delete
                        </button>
                      }
                    </div>
                  </div>

                  @if (key.lastUsed) {
                    <p class="qt-text-xs mt-1">Last used: {{ formatDate(key.lastUsed) }}</p>
                  }
                  @if (testResults()[key.id]; as result) {
                    <p
                      [class]="
                        'text-sm mt-2 ' +
                        (result.startsWith('✓') ? 'qt-text-success' : 'qt-text-destructive')
                      "
                    >
                      {{ result }}
                    </p>
                  }
                </div>
              }
            </div>
          }
        </div>
      }

      @if (modalOpen()) {
        <qt-api-key-modal (close)="modalOpen.set(false)" (saved)="onSaved()" />
      }
    </div>
  `,
})
export class ApiKeysCard {
  private readonly core = inject(CoreClient);
  private readonly queryClient = injectQueryClient();

  protected readonly modalOpen = signal(false);
  protected readonly testingId = signal<string | null>(null);
  protected readonly deletingId = signal<string | null>(null);
  protected readonly deleteConfirmId = signal<string | null>(null);
  protected readonly testResults = signal<Record<string, string>>({});

  protected readonly keysQuery = injectQuery(() => ({
    queryKey: ['apiKeys'],
    queryFn: async (): Promise<ApiKeyDto[]> => {
      const resp = await this.core.dispatchExpect({ type: 'apiKeyList' }, 'apiKeys');
      return resp.data.apiKeys;
    },
  }));

  protected readonly sortedKeys = computed(() =>
    [...(this.keysQuery.data() ?? [])].sort((a, b) => a.label.localeCompare(b.label)),
  );

  protected errorMessage(): string {
    const err = this.keysQuery.error();
    return err instanceof Error ? err.message : 'Failed to fetch API keys';
  }

  protected formatDate(iso: string): string {
    return new Date(iso).toLocaleDateString();
  }

  protected async test(id: string): Promise<void> {
    this.testingId.set(id);
    this.testResults.set({});
    try {
      const data = await this.core.dispatchData({ type: 'apiKeyTest', apiKeyId: id });
      const valid = data['valid'] === true;
      const error = (data['error'] as string) || 'Key is invalid';
      this.testResults.set({ [id]: valid ? '✓ Key is valid' : `✗ ${error}` });
    } catch (err) {
      this.testResults.set({
        [id]: `✗ ${err instanceof Error ? err.message : 'Connection failed'}`,
      });
    } finally {
      this.testingId.set(null);
    }
  }

  protected async confirmDelete(id: string): Promise<void> {
    this.deletingId.set(id);
    try {
      await this.core.dispatchExpect({ type: 'apiKeyDelete', apiKeyId: id }, 'ack');
      this.deleteConfirmId.set(null);
      await this.queryClient.invalidateQueries({ queryKey: ['apiKeys'] });
    } catch {
      // leave the confirm open; the load error surfaces on the next refetch
    } finally {
      this.deletingId.set(null);
    }
  }

  protected onSaved(): void {
    void this.queryClient.invalidateQueries({ queryKey: ['apiKeys'] });
  }
}
