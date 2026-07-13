import { ChangeDetectionStrategy, Component, computed, output, signal } from '@angular/core';
import { FormsModule } from '@angular/forms';

import { Modal } from '../../ui/modal';
import { DirectoryPicker } from './directory-picker';
import type { CreateDocumentStoreData } from './scriptorium.api';

type MountType = 'filesystem' | 'obsidian' | 'database';
type StoreType = 'documents' | 'character';

/** Split a comma-separated pattern string into trimmed, non-empty tokens. */
function splitPatterns(raw: string): string[] {
  return raw
    .split(',')
    .map((p) => p.trim())
    .filter(Boolean);
}

/**
 * The Add Document Store dialog (v4 `CreateDocumentStoreDialog.tsx`): name +
 * Contents (storeType) + Type (mountType radios: filesystem/obsidian/database)
 * + a DirectoryPicker basePath (skipped for database) + comma-separated
 * include/exclude patterns. Database mounts submit `basePath: ''` and omit
 * patterns (the v4 shape).
 */
@Component({
  selector: 'qt-create-store-dialog',
  changeDetection: ChangeDetectionStrategy.OnPush,
  imports: [FormsModule, Modal, DirectoryPicker],
  template: `
    <qt-modal title="Add Document Store" maxWidth="lg" (close)="close.emit()">
      <form id="qt-create-store-form" (ngSubmit)="submit()">
        <div class="mb-4">
          <label class="qt-label mb-2 block" for="qt-store-name">Name</label>
          <input
            id="qt-store-name"
            type="text"
            [(ngModel)]="name"
            name="name"
            required
            maxlength="200"
            placeholder="My Obsidian Vault"
            class="qt-input"
          />
        </div>

        <div class="mb-4">
          <label class="qt-label mb-2 block" for="qt-store-contents">Contents</label>
          <select id="qt-store-contents" [(ngModel)]="storeType" name="storeType" class="qt-input">
            <option value="documents">Documents — general notes, references, research</option>
            <option value="character">Character — character sheets and related material</option>
          </select>
        </div>

        <div class="mb-4">
          <label class="qt-label mb-2 block">Type</label>
          <div class="flex flex-col gap-2">
            <label class="flex items-center gap-2 cursor-pointer">
              <input type="radio" name="mountType" value="filesystem" [(ngModel)]="mountType" class="qt-radio" />
              <span class="qt-body">Filesystem — read/write documents on disk</span>
            </label>
            <label class="flex items-center gap-2 cursor-pointer">
              <input type="radio" name="mountType" value="obsidian" [(ngModel)]="mountType" class="qt-radio" />
              <span class="qt-body">Obsidian Vault — filesystem with Obsidian-friendly defaults</span>
            </label>
            <label class="flex items-center gap-2 cursor-pointer">
              <input type="radio" name="mountType" value="database" [(ngModel)]="mountType" class="qt-radio" />
              <span class="qt-body">Database-backed — everything stays encrypted inside Quilltap</span>
            </label>
          </div>
        </div>

        @if (!isDatabaseBacked()) {
          <div class="mb-4">
            <label class="qt-label mb-2 block">Path</label>
            <qt-directory-picker [(value)]="basePath" [required]="true" placeholder="/path/to/documents" />
            <p class="mt-1 text-xs qt-text-secondary">Absolute filesystem path to the document directory</p>
          </div>

          <div class="mb-4">
            <label class="qt-label mb-2 block" for="qt-include">Include Patterns (optional)</label>
            <input
              id="qt-include"
              type="text"
              [(ngModel)]="includePatterns"
              name="includePatterns"
              placeholder="*.md, *.txt, *.pdf, *.docx"
              class="qt-input"
            />
            <p class="mt-1 text-xs qt-text-secondary">Comma-separated glob patterns for files to include</p>
          </div>

          <div class="mb-4">
            <label class="qt-label mb-2 block" for="qt-exclude">Exclude Patterns (optional)</label>
            <input
              id="qt-exclude"
              type="text"
              [(ngModel)]="excludePatterns"
              name="excludePatterns"
              placeholder=".git, node_modules, .obsidian, .trash"
              class="qt-input"
            />
            <p class="mt-1 text-xs qt-text-secondary">Comma-separated glob patterns for files/directories to exclude</p>
          </div>
        } @else {
          <div class="mb-4 qt-alert-info">
            <p class="qt-body">
              Database-backed stores live entirely inside Quilltap's encrypted mount-index database — no
              filesystem path required. Documents and blobs you upload are included in the standard
              Quilltap backups.
            </p>
          </div>
        }
      </form>

      <div qt-modal-footer class="flex gap-2 justify-end">
        <button type="button" class="qt-button-secondary" (click)="close.emit()">Cancel</button>
        <button
          type="submit"
          form="qt-create-store-form"
          class="qt-button-primary"
          [disabled]="!canSubmit()"
        >
          Add Store
        </button>
      </div>
    </qt-modal>
  `,
})
export class CreateStoreDialog {
  readonly close = output<void>();
  readonly submitted = output<CreateDocumentStoreData>();

  protected readonly name = signal('');
  protected readonly storeType = signal<StoreType>('documents');
  protected readonly mountType = signal<MountType>('filesystem');
  protected readonly basePath = signal('');
  protected readonly includePatterns = signal('*.md, *.txt, *.pdf, *.docx');
  protected readonly excludePatterns = signal('.git, node_modules, .obsidian, .trash');

  protected readonly isDatabaseBacked = computed(() => this.mountType() === 'database');
  protected readonly canSubmit = computed(
    () => this.name().trim().length > 0 && (this.isDatabaseBacked() || this.basePath().trim().length > 0),
  );

  protected submit(): void {
    if (!this.canSubmit()) {
      return;
    }
    const db = this.isDatabaseBacked();
    const data: CreateDocumentStoreData = {
      name: this.name(),
      basePath: db ? '' : this.basePath(),
      mountType: this.mountType(),
      storeType: this.storeType(),
    };
    if (!db) {
      const include = splitPatterns(this.includePatterns());
      const exclude = splitPatterns(this.excludePatterns());
      if (include.length > 0) {
        data.includePatterns = include;
      }
      if (exclude.length > 0) {
        data.excludePatterns = exclude;
      }
    }
    this.submitted.emit(data);
  }
}
