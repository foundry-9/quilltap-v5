import { ChangeDetectionStrategy, Component, computed, input, OnInit, output, signal } from '@angular/core';
import { FormsModule } from '@angular/forms';

import { Modal } from '../../ui/modal';
import { DirectoryPicker } from './directory-picker';
import type { DocumentStore, UpdateDocumentStoreData } from './scriptorium.api';

type MountType = 'filesystem' | 'obsidian' | 'database';
type StoreType = 'documents' | 'character';

function splitPatterns(raw: string): string[] {
  return raw
    .split(',')
    .map((p) => p.trim())
    .filter(Boolean);
}

/**
 * The Edit Document Store dialog (v4 `EditDocumentStoreDialog.tsx`): PATCH
 * semantics over name / storeType / mountType / basePath / enabled / patterns.
 * Database mounts submit `basePath: ''` and omit patterns (the v4 shape).
 */
@Component({
  selector: 'qt-edit-store-dialog',
  changeDetection: ChangeDetectionStrategy.OnPush,
  imports: [FormsModule, Modal, DirectoryPicker],
  template: `
    <qt-modal title="Edit Document Store" maxWidth="lg" (close)="close.emit()">
      <form id="qt-edit-store-form" (ngSubmit)="submit()">
        <div class="mb-4">
          <label class="qt-label mb-2 block" for="qt-edit-name">Name</label>
          <input
            id="qt-edit-name"
            type="text"
            [(ngModel)]="name"
            name="name"
            required
            maxlength="200"
            class="qt-input"
          />
        </div>

        <div class="mb-4">
          <label class="qt-label mb-2 block" for="qt-edit-contents">Contents</label>
          <select id="qt-edit-contents" [(ngModel)]="storeType" name="storeType" class="qt-input">
            <option value="documents">Documents — general notes, references, research</option>
            <option value="character">Character — character sheets and related material</option>
          </select>
        </div>

        @if (!isDatabaseBacked()) {
          <div class="mb-4">
            <label class="qt-label mb-2 block">Path</label>
            <qt-directory-picker [(value)]="basePath" [required]="true" placeholder="/path/to/documents" />
            <p class="mt-1 text-xs qt-text-secondary">Absolute filesystem path to the document directory</p>
          </div>
        }

        <div class="mb-4">
          <label class="qt-label mb-2 block">Type</label>
          <div class="flex flex-col gap-2">
            <label class="flex items-center gap-2 cursor-pointer">
              <input type="radio" name="mountType" value="filesystem" [(ngModel)]="mountType" class="qt-radio" />
              <span class="qt-body">Filesystem</span>
            </label>
            <label class="flex items-center gap-2 cursor-pointer">
              <input type="radio" name="mountType" value="obsidian" [(ngModel)]="mountType" class="qt-radio" />
              <span class="qt-body">Obsidian Vault</span>
            </label>
            <label class="flex items-center gap-2 cursor-pointer">
              <input type="radio" name="mountType" value="database" [(ngModel)]="mountType" class="qt-radio" />
              <span class="qt-body">Database-backed</span>
            </label>
          </div>
          @if (isDatabaseBacked()) {
            <p class="mt-2 text-xs qt-text-secondary italic">
              Database-backed stores have no filesystem path or include/exclude patterns. Switching from
              a filesystem mount type will drop the current base path.
            </p>
          }
        </div>

        <div class="mb-4">
          <label class="flex items-center gap-2 cursor-pointer">
            <input type="checkbox" [(ngModel)]="enabled" name="enabled" class="qt-checkbox" />
            <span class="qt-label">Enabled</span>
          </label>
          <p class="mt-1 text-xs qt-text-secondary">Disabled stores won't be scanned or searched</p>
        </div>

        @if (!isDatabaseBacked()) {
          <div class="mb-4">
            <label class="qt-label mb-2 block" for="qt-edit-include">Include Patterns</label>
            <input id="qt-edit-include" type="text" [(ngModel)]="includePatterns" name="includePatterns" class="qt-input" />
            <p class="mt-1 text-xs qt-text-secondary">Comma-separated glob patterns for files to include</p>
          </div>

          <div class="mb-4">
            <label class="qt-label mb-2 block" for="qt-edit-exclude">Exclude Patterns</label>
            <input id="qt-edit-exclude" type="text" [(ngModel)]="excludePatterns" name="excludePatterns" class="qt-input" />
            <p class="mt-1 text-xs qt-text-secondary">Comma-separated glob patterns for files/directories to exclude</p>
          </div>
        }
      </form>

      <div qt-modal-footer class="flex gap-2 justify-end">
        <button type="button" class="qt-button-secondary" (click)="close.emit()">Cancel</button>
        <button
          type="submit"
          form="qt-edit-store-form"
          class="qt-button-primary"
          [disabled]="!name().trim()"
        >
          Save Changes
        </button>
      </div>
    </qt-modal>
  `,
})
export class EditStoreDialog implements OnInit {
  readonly store = input.required<DocumentStore>();
  readonly close = output<void>();
  readonly submitted = output<{ id: string; data: UpdateDocumentStoreData }>();

  protected readonly name = signal('');
  protected readonly storeType = signal<StoreType>('documents');
  protected readonly mountType = signal<MountType>('filesystem');
  protected readonly basePath = signal('');
  protected readonly enabled = signal(true);
  protected readonly includePatterns = signal('');
  protected readonly excludePatterns = signal('');

  protected readonly isDatabaseBacked = computed(() => this.mountType() === 'database');

  ngOnInit(): void {
    const s = this.store();
    this.name.set(s.name);
    this.storeType.set((s.storeType as StoreType) || 'documents');
    this.mountType.set((s.mountType as MountType) || 'filesystem');
    this.basePath.set(s.basePath || '');
    this.enabled.set(s.enabled ?? true);
    this.includePatterns.set(s.includePatterns.join(', '));
    this.excludePatterns.set(s.excludePatterns.join(', '));
  }

  protected submit(): void {
    if (!this.name().trim()) {
      return;
    }
    const db = this.isDatabaseBacked();
    const data: UpdateDocumentStoreData = {
      name: this.name(),
      basePath: db ? '' : this.basePath(),
      mountType: this.mountType(),
      storeType: this.storeType(),
      enabled: this.enabled(),
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
    this.submitted.emit({ id: this.store().id, data });
  }
}
