import { ChangeDetectionStrategy, Component, inject, output, signal } from '@angular/core';

import { apiUrl } from '../../../core/api-url';
import { CoreClient } from '../../../core/core-client';
import { Modal } from '../../../ui/modal';

/**
 * The SillyTavern import dialog (v4 `AuroraView.tsx` import form). A JSON file is
 * read client-side and dispatched as `characterImport { payload }`; a PNG posts
 * multipart to lane A's web route (`POST /api/v1/characters?action=import`) — the
 * only leg dispatch can't carry. Emits `imported` on success so the list refetches.
 */
@Component({
  selector: 'qt-character-import-dialog',
  changeDetection: ChangeDetectionStrategy.OnPush,
  imports: [Modal],
  template: `
    <qt-modal title="Import from SillyTavern" maxWidth="md" (close)="close.emit()">
      <div class="mb-4">
        <label class="mb-2 block text-sm qt-text-primary">
          Select SillyTavern character file (PNG or JSON)
        </label>
        <input
          #fileInput
          type="file"
          name="file"
          accept=".png,.json"
          class="block w-full qt-text-small file:mr-4 file:rounded-md file:border-0 file:qt-bg-primary/20 file:px-4 file:py-2 file:font-semibold file:text-primary hover:file:qt-bg-primary/30"
          (change)="onPick($any($event.target).files)"
        />
      </div>
      @if (error()) {
        <p class="qt-text-destructive qt-text-small mb-2">{{ error() }}</p>
      }

      <div qt-modal-footer class="flex gap-2 justify-end">
        <button
          type="button"
          class="qt-button qt-button-secondary"
          [disabled]="busy()"
          (click)="close.emit()"
        >
          Cancel
        </button>
        <button
          type="button"
          class="qt-button qt-button-primary"
          [disabled]="busy() || !file()"
          (click)="submit()"
        >
          {{ busy() ? 'Importing...' : 'Import' }}
        </button>
      </div>
    </qt-modal>
  `,
})
export class CharacterImportDialog {
  private readonly core = inject(CoreClient);

  readonly close = output<void>();
  readonly imported = output<void>();

  protected readonly file = signal<File | null>(null);
  protected readonly busy = signal(false);
  protected readonly error = signal<string | null>(null);

  protected onPick(files: FileList | null): void {
    this.file.set(files && files.length > 0 ? files[0] : null);
    this.error.set(null);
  }

  protected async submit(): Promise<void> {
    const file = this.file();
    if (!file) {
      return;
    }
    this.busy.set(true);
    this.error.set(null);
    try {
      if (file.name.toLowerCase().endsWith('.json')) {
        const payload = JSON.parse(await file.text());
        await this.core.dispatchData({ type: 'characterImport', payload });
      } else {
        // The PNG (tEXt-chunk) leg rides multipart — dispatch can't carry bytes.
        const form = new FormData();
        form.append('file', file);
        const res = await fetch(apiUrl('/api/v1/characters?action=import'), {
          method: 'POST',
          body: form,
        });
        if (!res.ok) {
          throw new Error(`Import failed (HTTP ${res.status})`);
        }
      }
      this.imported.emit();
      this.close.emit();
    } catch {
      this.error.set(
        "Failed to import character. Make sure it's a valid SillyTavern PNG or JSON file.",
      );
    } finally {
      this.busy.set(false);
    }
  }
}
