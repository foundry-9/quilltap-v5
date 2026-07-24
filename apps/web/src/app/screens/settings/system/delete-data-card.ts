import { ChangeDetectionStrategy, Component, computed, inject, signal } from '@angular/core';

import { CoreClient } from '../../../core/core-client';
import { CoreDispatchError } from '../../../core/core-contract';
import { Icon } from '../../../ui/icon';
import { Modal } from '../../../ui/modal';

type Step = 'preview' | 'confirm' | 'deleting' | 'complete';

/**
 * The server delete summary (v4 `lib/backup/restore/delete-service.ts:191-209`).
 * NOTE the deliberate v5 correction: v4's client component (`delete-data-card.tsx`)
 * declared a required `personas` field the server never returns, so its
 * `getTotalCount` summed `undefined` → `NaN` "Total Items" (and never showed
 * `projects`). v5 uses the REAL server shape, sums only present fields, and
 * renders the `projects` row v4 dropped — a dogfood-#6-style fix, recorded.
 */
interface DeleteSummary {
  characters?: number;
  chats?: number;
  tags?: number;
  files?: number;
  memories?: number;
  apiKeys?: number;
  backups?: number;
  projects?: number;
  profiles?: { connection?: number; image?: number; embedding?: number };
  templates?: { prompt?: number; roleplay?: number };
}

/**
 * The Delete All Data card (v4 `components/tools/delete-data-card.tsx`): a
 * destructive 4-step dialog (preview → confirm → deleting → complete),
 * double-gated — the user types `DELETE` (client) AND the wire body carries
 * `{confirm:'DELETE_ALL_MY_DATA'}` (server gate, v4 `tools/route.ts:155`).
 * Preview rides `systemDeleteDataPreview`, the delete `systemDeleteData`.
 * Live behaviour + e2e beat are ACTIVATE-AT-UNIFY.
 */
@Component({
  selector: 'qt-delete-data-card',
  changeDetection: ChangeDetectionStrategy.OnPush,
  imports: [Icon, Modal],
  template: `
    <div>
      <div class="flex items-start gap-4 mb-6">
        <div class="flex-1">
          <p class="qt-text-small">
            Permanently delete all your data including characters, personas, chats, files, API keys,
            and backups. This action cannot be undone.
          </p>
        </div>
        <div class="flex-shrink-0 qt-text-destructive"><qt-icon name="trash" class="w-8 h-8" /></div>
      </div>

      <button type="button" class="qt-button qt-button-destructive" (click)="open()">
        Delete All Data
      </button>

      @if (showDialog()) {
        <qt-modal [title]="dialogTitle()" maxWidth="md" [closeOnBackdrop]="step() !== 'deleting'" (close)="close()">
          @switch (step()) {
            @case ('preview') {
              @if (loading()) {
                <div class="flex justify-center py-8">
                  <div class="animate-spin rounded-full h-8 w-8 border-b-2 qt-text-secondary border-current"></div>
                </div>
              } @else if (preview(); as p) {
                <div class="space-y-4">
                  <p class="qt-text-small">The following data will be permanently deleted:</p>
                  <div class="qt-bg-muted/50 rounded-lg p-4 space-y-2 text-sm">
                    @for (row of rows(p); track row.label) {
                      <div class="flex justify-between">
                        <span class="qt-text-secondary">{{ row.label }}</span>
                        <span class="qt-text-primary">{{ row.value }}</span>
                      </div>
                    }
                    <div class="border-t qt-border-default pt-2 mt-2">
                      <div class="flex justify-between font-semibold">
                        <span class="text-foreground">Total Items</span>
                        <span class="qt-text-destructive">{{ total(p) }}</span>
                      </div>
                    </div>
                  </div>
                  <div class="qt-bg-destructive/10 border qt-border-destructive/30 rounded-lg p-3">
                    <p class="text-sm qt-text-destructive font-medium">
                      This action cannot be undone. All your data will be permanently deleted.
                    </p>
                  </div>
                </div>
              } @else if (error(); as msg) {
                <div class="p-3 qt-bg-destructive/10 border qt-border-destructive/30 rounded-lg">
                  <p class="text-sm qt-text-destructive">{{ msg }}</p>
                </div>
              }
            }
            @case ('confirm') {
              <div class="space-y-4">
                <p class="qt-text-small">
                  To confirm deletion, please type
                  <span class="font-mono font-semibold qt-text-destructive">DELETE</span> below:
                </p>
                <input
                  type="text"
                  class="qt-input"
                  placeholder="Type DELETE to confirm"
                  [value]="confirmText()"
                  (input)="onConfirmInput($event)"
                />
                @if (error(); as msg) {
                  <p class="text-sm qt-text-destructive">{{ msg }}</p>
                }
              </div>
            }
            @case ('deleting') {
              <div class="flex flex-col items-center justify-center py-8 space-y-4">
                <div class="animate-spin rounded-full h-12 w-12 border-b-2 qt-text-destructive border-current"></div>
                <p class="qt-text-small">Deleting all data...</p>
              </div>
            }
            @case ('complete') {
              <div class="space-y-4">
                <div class="w-16 h-16 qt-bg-success/10 rounded-full mx-auto flex items-center justify-center">
                  <qt-icon name="check" class="w-8 h-8 qt-text-success" />
                </div>
                <p class="text-center qt-text-small">
                  Successfully deleted {{ completeTotal() }} items from your account.
                </p>
                <p class="text-center qt-text-xs">
                  Your account is now clean. You can start fresh or restore from a backup.
                </p>
              </div>
            }
          }

          <div qt-modal-footer class="flex gap-3 justify-end w-full">
            @switch (step()) {
              @case ('preview') {
                <button type="button" class="qt-button qt-button-secondary" (click)="close()">Cancel</button>
                <button
                  type="button"
                  class="qt-button qt-button-destructive"
                  [disabled]="loading() || !preview() || total(preview()!) === 0"
                  (click)="proceedToConfirm()"
                >
                  Continue
                </button>
              }
              @case ('confirm') {
                <button type="button" class="qt-button qt-button-secondary" (click)="step.set('preview')">
                  Back
                </button>
                <button
                  type="button"
                  class="qt-button qt-button-destructive"
                  [disabled]="confirmText() !== 'DELETE'"
                  (click)="performDelete()"
                >
                  Delete Everything
                </button>
              }
              @case ('complete') {
                <button type="button" class="qt-button qt-button-primary" (click)="close()">Done</button>
              }
            }
          </div>
        </qt-modal>
      }
    </div>
  `,
})
export class DeleteDataCard {
  private readonly core = inject(CoreClient);

  protected readonly showDialog = signal(false);
  protected readonly step = signal<Step>('preview');
  protected readonly preview = signal<DeleteSummary | null>(null);
  protected readonly deleteSummary = signal<DeleteSummary | null>(null);
  protected readonly loading = signal(false);
  protected readonly confirmText = signal('');
  protected readonly error = signal<string | null>(null);

  protected readonly dialogTitle = computed(() =>
    this.step() === 'complete' ? 'Deletion Complete' : 'Delete All Data',
  );
  protected readonly completeTotal = computed(() => {
    const s = this.deleteSummary();
    return s ? this.total(s) : 0;
  });

  protected rows(p: DeleteSummary): { label: string; value: number }[] {
    const base = [
      { label: 'Characters', value: p.characters ?? 0 },
      { label: 'Chats', value: p.chats ?? 0 },
      { label: 'Tags', value: p.tags ?? 0 },
      { label: 'Files', value: p.files ?? 0 },
      { label: 'Memories', value: p.memories ?? 0 },
      { label: 'API Keys', value: p.apiKeys ?? 0 },
      { label: 'Backups', value: p.backups ?? 0 },
      { label: 'Projects', value: p.projects ?? 0 },
      { label: 'Connection Profiles', value: p.profiles?.connection ?? 0 },
      { label: 'Image Profiles', value: p.profiles?.image ?? 0 },
      { label: 'Embedding Profiles', value: p.profiles?.embedding ?? 0 },
    ];
    // v4's conditional templates block.
    if (p.templates && ((p.templates.prompt ?? 0) > 0 || (p.templates.roleplay ?? 0) > 0)) {
      base.push({ label: 'Prompt Templates', value: p.templates.prompt ?? 0 });
      base.push({ label: 'Roleplay Templates', value: p.templates.roleplay ?? 0 });
    }
    return base;
  }

  /** v4 `getTotalCount` — corrected to sum only server-returned fields. */
  protected total(s: DeleteSummary): number {
    return (
      (s.characters ?? 0) +
      (s.chats ?? 0) +
      (s.tags ?? 0) +
      (s.files ?? 0) +
      (s.memories ?? 0) +
      (s.apiKeys ?? 0) +
      (s.backups ?? 0) +
      (s.projects ?? 0) +
      (s.profiles?.connection ?? 0) +
      (s.profiles?.image ?? 0) +
      (s.profiles?.embedding ?? 0) +
      (s.templates?.prompt ?? 0) +
      (s.templates?.roleplay ?? 0)
    );
  }

  protected async open(): Promise<void> {
    this.showDialog.set(true);
    this.step.set('preview');
    this.preview.set(null);
    this.deleteSummary.set(null);
    this.confirmText.set('');
    this.error.set(null);
    this.loading.set(true);
    try {
      const data = await this.core.dispatchData({ type: 'systemDeleteDataPreview' });
      this.preview.set((data['summary'] ?? {}) as DeleteSummary);
    } catch (err) {
      this.error.set(this.msg(err, 'Failed to load preview'));
    } finally {
      this.loading.set(false);
    }
  }

  protected close(): void {
    this.showDialog.set(false);
    this.step.set('preview');
    this.preview.set(null);
    this.deleteSummary.set(null);
    this.confirmText.set('');
    this.error.set(null);
  }

  protected proceedToConfirm(): void {
    this.step.set('confirm');
    this.confirmText.set('');
  }

  protected onConfirmInput(event: Event): void {
    // v4 `.toUpperCase()` on every keystroke.
    this.confirmText.set((event.target as HTMLInputElement).value.toUpperCase());
  }

  protected async performDelete(): Promise<void> {
    if (this.confirmText() !== 'DELETE') {
      this.error.set('Please type DELETE to confirm');
      return;
    }
    this.step.set('deleting');
    this.error.set(null);
    try {
      const data = await this.core.dispatchData({
        type: 'systemDeleteData',
        confirm: 'DELETE_ALL_MY_DATA',
      });
      this.deleteSummary.set((data['summary'] ?? {}) as DeleteSummary);
      this.step.set('complete');
    } catch (err) {
      this.error.set(this.msg(err, 'Failed to delete data'));
      this.step.set('confirm');
    }
  }

  private msg(err: unknown, fallback: string): string {
    if (err instanceof CoreDispatchError || err instanceof Error) return err.message || fallback;
    return fallback;
  }
}
