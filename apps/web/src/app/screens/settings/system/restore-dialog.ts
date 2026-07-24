import { ChangeDetectionStrategy, Component, computed, inject, output, signal } from '@angular/core';

import { apiUrl } from '../../../core/api-url';
import { CoreClient } from '../../../core/core-client';
import { CoreDispatchError } from '../../../core/core-contract';
import { Icon } from '../../../ui/icon';
import { Modal } from '../../../ui/modal';

type Step = 'source' | 'preview' | 'mode' | 'progress';
type RestoreMode = 'replace' | 'import';

/** The counts the preview/summary screens read (a subset of the server RestoreSummary). */
interface RestoreSummary {
  characters?: number;
  chats?: number;
  messages?: number;
  tags?: number;
  memories?: number;
  files?: number;
  profiles?: { connection?: number };
  warnings?: string[];
}

/**
 * The Restore Backup dialog (v4 `components/tools/restore/RestoreDialog.tsx` +
 * `hooks/useRestoreData.ts`): a 4-step wizard — pick a file, upload + preview,
 * choose a mode, restore. The upload is a raw octet-stream over XHR so the
 * `Uploading… N%` progress shows (a web-edge leg via `apiUrl()`); preview and
 * execute ride the `systemRestorePreview` / `systemRestoreExecute` verbs. The UI
 * mode `import` maps to the wire `new-account` (v4 `:180`). On completion the
 * card reloads the app (a replace-mode restore replaced everything).
 *
 * Over G1's verbs + the upload leg — live behaviour is ACTIVATE-AT-UNIFY.
 */
@Component({
  selector: 'qt-restore-dialog',
  changeDetection: ChangeDetectionStrategy.OnPush,
  imports: [Icon, Modal],
  template: `
    <qt-modal title="Restore Backup" maxWidth="2xl" [closeOnBackdrop]="!busy()" (close)="handleClose()">
      <p class="qt-dialog-description mb-4">Step {{ stepNumber() }} of 4</p>

      @switch (step()) {
        @case ('source') {
          <div class="space-y-4">
            <label class="block text-sm qt-text-primary mb-3">Select Backup File</label>
            <div
              class="border-2 border-dashed qt-border-default rounded-lg p-6 text-center cursor-pointer hover:border-input transition-colors"
              (click)="fileInput.click()"
            >
              <qt-icon name="plus" class="w-12 h-12 mx-auto qt-text-secondary mb-2" />
              <p class="qt-text-small">Click to select or drag and drop a backup file</p>
              <p class="qt-text-xs mt-1">Supports .zip backup files</p>
              @if (selectedFile(); as f) {
                <p class="text-sm qt-text-success mt-2">Selected: {{ f.name }}</p>
              }
              <input
                #fileInput
                type="file"
                accept=".json,.zip"
                class="hidden"
                (change)="onFileSelect($event)"
              />
            </div>
            @if (error(); as msg) {
              <div class="p-3 qt-bg-destructive/10 border qt-border-destructive rounded-lg">
                <p class="text-sm qt-text-destructive">{{ msg }}</p>
              </div>
            }
          </div>
        }
        @case ('preview') {
          @if (loadingPreview()) {
            <div class="text-center py-8 qt-text-secondary">Loading preview...</div>
          } @else if (preview(); as p) {
            <div class="grid grid-cols-2 gap-3">
              @for (card of previewCards(p); track card.label) {
                <div class="qt-bg-muted p-4 rounded-lg">
                  <p class="qt-heading-2 text-foreground">{{ card.value }}</p>
                  <p class="qt-text-xs mt-1">{{ card.label }}</p>
                </div>
              }
            </div>
          }
          @if (error(); as msg) {
            <div class="mt-3 p-3 qt-bg-destructive/10 border qt-border-destructive rounded-lg">
              <p class="text-sm qt-text-destructive">{{ msg }}</p>
            </div>
          }
        }
        @case ('mode') {
          <div class="space-y-4">
            <div class="space-y-3">
              <label
                class="flex items-start p-4 border-2 rounded-lg cursor-pointer transition-colors"
                [class]="
                  mode() === 'replace'
                    ? 'qt-border-destructive qt-bg-destructive/10'
                    : 'qt-border-default bg-background'
                "
              >
                <input
                  type="radio"
                  name="restore-mode"
                  value="replace"
                  class="mt-1"
                  [checked]="mode() === 'replace'"
                  (change)="setMode('replace')"
                />
                <div class="ml-3 flex-1">
                  <p class="text-sm qt-text-primary">Replace Existing Data</p>
                  <p class="qt-text-xs mt-1">Delete all your current data and replace with backup</p>
                </div>
              </label>
              <label
                class="flex items-start p-4 border-2 rounded-lg cursor-pointer transition-colors"
                [class]="
                  mode() === 'import'
                    ? 'qt-border-primary qt-bg-primary/10'
                    : 'qt-border-default bg-background'
                "
              >
                <input
                  type="radio"
                  name="restore-mode"
                  value="import"
                  class="mt-1"
                  [checked]="mode() === 'import'"
                  (change)="setMode('import')"
                />
                <div class="ml-3 flex-1">
                  <p class="text-sm qt-text-primary">Import as New Data</p>
                  <p class="qt-text-xs mt-1">
                    Keep your existing data and import backup with regenerated IDs
                  </p>
                </div>
              </label>
            </div>

            @if (mode() === 'replace') {
              <div class="qt-bg-destructive/10 border qt-border-destructive rounded-lg p-4">
                <p class="qt-text-destructive mb-3">Warning: This will DELETE all your current data!</p>
                <label class="flex items-center gap-2 cursor-pointer">
                  <input
                    type="checkbox"
                    class="w-4 h-4 qt-text-destructive rounded focus:ring-ring"
                    [checked]="confirmReplace()"
                    (change)="confirmReplace.set($any($event.target).checked)"
                  />
                  <span class="text-sm qt-text-destructive">I understand this action cannot be undone</span>
                </label>
              </div>
            }
            @if (error(); as msg) {
              <div class="p-3 qt-bg-destructive/10 border qt-border-destructive rounded-lg">
                <p class="text-sm qt-text-destructive">{{ msg }}</p>
              </div>
            }
          </div>
        }
        @case ('progress') {
          @if (restoring()) {
            <div class="text-center py-8">
              <div class="animate-spin rounded-full h-12 w-12 border-b-2 qt-border-primary mx-auto"></div>
              <p class="mt-4 qt-text-small">Restoring your backup...</p>
            </div>
          } @else if (summary(); as s) {
            <div class="space-y-4">
              <div class="qt-bg-success/20 border qt-border-success/30 rounded-lg p-4">
                <p class="qt-label qt-text-success">Backup restored successfully!</p>
              </div>
              <div class="grid grid-cols-2 gap-3">
                @for (card of summaryCards(s); track card.label) {
                  <div class="qt-bg-muted p-4 rounded-lg">
                    <p class="qt-heading-2 text-foreground">{{ card.value }}</p>
                    <p class="qt-text-xs mt-1">{{ card.label }}</p>
                  </div>
                }
              </div>
              @if (s.warnings && s.warnings.length > 0) {
                <div class="qt-bg-warning/20 border qt-border-warning/30 rounded-lg p-4">
                  <p class="qt-label qt-text-warning mb-2">Warnings ({{ s.warnings.length }}):</p>
                  <ul class="text-sm qt-text-warning space-y-1 max-h-40 overflow-y-auto">
                    @for (w of s.warnings; track $index) {
                      <li class="flex gap-2">
                        <span class="flex-shrink-0">•</span><span class="break-words">{{ w }}</span>
                      </li>
                    }
                  </ul>
                </div>
              }
            </div>
          } @else if (error(); as msg) {
            <div class="p-4 qt-bg-destructive/10 border qt-border-destructive rounded-lg">
              <p class="text-sm qt-text-destructive">{{ msg }}</p>
            </div>
          }
        }
      }

      <div qt-modal-footer class="flex gap-3 justify-between w-full">
        @if (step() === 'progress' && summary()) {
          <button type="button" class="flex-1 qt-button qt-button-primary" (click)="handleCloseAfterRestore()">
            Close
          </button>
        } @else {
          <button
            type="button"
            class="qt-button qt-button-secondary"
            [disabled]="step() === 'source' || busy()"
            (click)="handleBack()"
          >
            Back
          </button>
          @if (step() !== 'progress') {
            <div class="flex gap-3">
              <button
                type="button"
                class="qt-button qt-button-secondary"
                [disabled]="busy()"
                (click)="handleClose()"
              >
                Cancel
              </button>
              @if (step() === 'mode') {
                <button
                  type="button"
                  class="qt-button qt-button-primary"
                  [disabled]="restoring() || (mode() === 'replace' && !confirmReplace())"
                  (click)="handleStartRestore()"
                >
                  {{ restoring() ? 'Restoring...' : 'Start Restore' }}
                </button>
              } @else {
                <button
                  type="button"
                  class="qt-button qt-button-primary"
                  [disabled]="uploading() || loadingPreview() || (step() === 'source' && !selectedFile())"
                  (click)="handleNext()"
                >
                  @if (uploading()) {
                    Uploading... {{ uploadProgress() }}%
                  } @else if (loadingPreview()) {
                    Analyzing...
                  } @else {
                    Next
                  }
                </button>
              }
            </div>
          }
        }
      </div>
    </qt-modal>
  `,
})
export class RestoreDialog {
  private readonly core = inject(CoreClient);

  /** Emitted on plain close (Cancel / ✕). */
  readonly close = output<void>();
  /** Emitted after a completed restore (the card reloads the app). */
  readonly restoreComplete = output<void>();

  protected readonly step = signal<Step>('source');
  protected readonly selectedFile = signal<File | null>(null);
  protected readonly preview = signal<RestoreSummary | null>(null);
  protected readonly loadingPreview = signal(false);
  protected readonly mode = signal<RestoreMode>('import');
  protected readonly confirmReplace = signal(false);
  protected readonly restoring = signal(false);
  protected readonly summary = signal<RestoreSummary | null>(null);
  protected readonly error = signal<string | null>(null);
  protected readonly uploading = signal(false);
  protected readonly uploadProgress = signal(0);
  private uploadId: string | null = null;

  protected readonly busy = computed(() => this.restoring() || this.uploading());

  protected readonly stepNumber = computed(() => {
    switch (this.step()) {
      case 'source':
        return 1;
      case 'preview':
        return 2;
      case 'mode':
        return 3;
      case 'progress':
        return 4;
    }
  });

  protected previewCards(p: RestoreSummary): { label: string; value: number }[] {
    return [
      { label: 'Characters', value: p.characters ?? 0 },
      { label: 'Chats', value: p.chats ?? 0 },
      { label: 'Messages', value: p.messages ?? 0 },
      { label: 'Tags', value: p.tags ?? 0 },
      { label: 'Memories', value: p.memories ?? 0 },
    ];
  }

  protected summaryCards(s: RestoreSummary): { label: string; value: number }[] {
    return [
      { label: 'Characters', value: s.characters ?? 0 },
      { label: 'Chats', value: s.chats ?? 0 },
      { label: 'Messages', value: s.messages ?? 0 },
      { label: 'API Keys', value: s.profiles?.connection ?? 0 },
      { label: 'Files', value: s.files ?? 0 },
    ];
  }

  protected onFileSelect(event: Event): void {
    const file = (event.target as HTMLInputElement).files?.[0] ?? null;
    this.selectedFile.set(file);
    this.error.set(null);
    // Re-selecting forces a fresh upload (v4 `:34-37`).
    this.uploadId = null;
    this.uploadProgress.set(0);
  }

  protected setMode(mode: RestoreMode): void {
    // v4 `:240-242` — changing mode clears the confirm checkbox.
    this.mode.set(mode);
    this.confirmReplace.set(false);
  }

  protected async handleNext(): Promise<void> {
    if (this.step() === 'source') {
      if (!this.selectedFile()) {
        this.error.set('Please select a backup file');
        return;
      }
      await this.uploadAndPreview();
    } else if (this.step() === 'preview') {
      this.step.set('mode');
    }
  }

  protected handleBack(): void {
    this.error.set(null);
    switch (this.step()) {
      case 'preview':
        this.step.set('source');
        this.preview.set(null);
        break;
      case 'mode':
        this.step.set('preview');
        break;
      case 'progress':
        this.step.set('mode');
        this.summary.set(null);
        break;
    }
  }

  private async uploadAndPreview(): Promise<void> {
    const file = this.selectedFile();
    if (!file) return;
    this.uploading.set(true);
    this.uploadProgress.set(0);
    this.error.set(null);
    try {
      const uploadId = await this.uploadFile(file);
      this.uploading.set(false);
      this.uploadId = uploadId;
      this.loadingPreview.set(true);

      const data = await this.core.dispatchData({ type: 'systemRestorePreview', uploadId });
      this.preview.set((data['preview'] ?? {}) as RestoreSummary);
      this.step.set('preview');
    } catch (err) {
      this.error.set(this.msg(err, 'Failed to upload backup'));
      this.uploading.set(false);
    } finally {
      this.loadingPreview.set(false);
    }
  }

  /** v4 `:51-89` — the raw octet-stream XHR upload, driving the percent. */
  private uploadFile(file: File): Promise<string> {
    return new Promise<string>((resolve, reject) => {
      const xhr = new XMLHttpRequest();
      xhr.open('POST', apiUrl('/api/v1/system/restore?action=upload'));
      xhr.setRequestHeader('Content-Type', 'application/octet-stream');
      xhr.upload.onprogress = (event) => {
        if (event.lengthComputable) {
          this.uploadProgress.set(Math.round((event.loaded / event.total) * 100));
        }
      };
      xhr.onload = () => {
        if (xhr.status >= 200 && xhr.status < 300) {
          try {
            const data = JSON.parse(xhr.responseText);
            if (data.success && data.uploadId) resolve(data.uploadId);
            else reject(new Error(data.error || 'Upload failed'));
          } catch {
            reject(new Error('Invalid response from server'));
          }
        } else {
          try {
            const data = JSON.parse(xhr.responseText);
            reject(new Error(data.error || `Upload failed (${xhr.status})`));
          } catch {
            reject(new Error(`Upload failed (${xhr.status})`));
          }
        }
      };
      xhr.onerror = () => reject(new Error('Network error during upload'));
      xhr.onabort = () => reject(new Error('Upload cancelled'));
      xhr.send(file);
    });
  }

  protected async handleStartRestore(): Promise<void> {
    if (!this.uploadId) return;
    if (this.mode() === 'replace' && !this.confirmReplace()) {
      this.error.set('Please confirm that you want to delete your existing data');
      return;
    }
    this.restoring.set(true);
    this.error.set(null);
    this.step.set('progress');
    try {
      const data = await this.core.dispatchData({
        type: 'systemRestoreExecute',
        uploadId: this.uploadId,
        mode: this.mode() === 'replace' ? 'replace' : 'new-account',
      });
      this.summary.set((data['summary'] ?? {}) as RestoreSummary);
    } catch (err) {
      // v4 `:200` — bounce back to the mode step on error.
      this.error.set(this.msg(err, 'Failed to restore backup'));
      this.step.set('mode');
    } finally {
      this.restoring.set(false);
    }
  }

  protected handleClose(): void {
    if (this.busy()) return;
    this.close.emit();
  }

  protected handleCloseAfterRestore(): void {
    this.restoreComplete.emit();
  }

  private msg(err: unknown, fallback: string): string {
    if (err instanceof CoreDispatchError || err instanceof Error) return err.message || fallback;
    return fallback;
  }
}
