import { ChangeDetectionStrategy, Component, computed, inject, output, signal } from '@angular/core';

import { apiUrl } from '../../../core/api-url';
import { Icon } from '../../../ui/icon';
import { Modal } from '../../../ui/modal';
import {
  ENTITY_TYPE_LABELS,
  formatBytes,
  toExportEntityType,
  type ConflictStrategy,
} from './import-export.types';

type Step = 'file' | 'preview' | 'options' | 'importing' | 'complete' | 'error';

interface ImportPreview {
  manifest?: { exportType?: string; createdAt?: string; appVersion?: string };
  entities?: Record<string, unknown>;
}

interface ImportResult {
  imported?: Record<string, number>;
  skipped?: Record<string, number>;
  warnings?: string[];
}

interface PreviewEntity {
  id: string;
  name?: string;
  title?: string;
  exists?: boolean;
}

/**
 * The Import Data dialog (v4 `components/tools/import-dialog.tsx` +
 * `hooks/useImportData.ts`): a 5-step wizard — drag/drop a `.qtap`, preview,
 * choose conflict handling, import. The file is sniffed client-side (NDJSON
 * envelope vs legacy monolith); because the client always holds the `File`, the
 * preview + execute always use the multipart web-edge legs
 * (`?action=import-preview|import-execute`), fetched via `apiUrl()`. The JSON
 * legs (`systemImportPreview`/`Execute`) are the fallback the UI never reaches.
 * Live behaviour is ACTIVATE-AT-UNIFY.
 */
@Component({
  selector: 'qt-import-dialog',
  changeDetection: ChangeDetectionStrategy.OnPush,
  imports: [Icon, Modal],
  template: `
    <qt-modal title="Import Data" maxWidth="2xl" [closeOnBackdrop]="!importing()" (close)="handleClose()">
      <p class="qt-dialog-description mb-4">Step {{ stepNumber() }} of 5</p>

      @switch (step()) {
        @case ('file') {
          <div class="space-y-4">
            <p class="qt-text-small qt-text-secondary">Select a Quilltap export file (.qtap) to import.</p>
            <div
              class="border-2 border-dashed rounded-lg p-8 text-center cursor-pointer transition-colors"
              [class]="
                dragActive()
                  ? 'qt-border-primary qt-bg-primary/10'
                  : 'qt-border-default hover:qt-border-primary/50'
              "
              (click)="fileInput.click()"
              (dragenter)="onDragEnter($event)"
              (dragleave)="onDragLeave($event)"
              (dragover)="onDragOver($event)"
              (drop)="onDrop($event)"
            >
              <input
                #fileInput
                type="file"
                accept=".qtap,.json"
                class="hidden"
                (change)="onFileSelect($event)"
              />
              <qt-icon name="upload" class="w-12 h-12 mx-auto mb-3 qt-text-secondary" />
              <p class="text-foreground font-medium">Drag and drop a .qtap file here</p>
              <p class="qt-text-secondary text-sm mt-1">or click to browse</p>
            </div>
            @if (selectedFile(); as f) {
              <div class="p-4 qt-bg-muted/50 rounded-lg">
                <p class="font-medium text-foreground truncate">{{ f.name }}</p>
                <p class="qt-text-small qt-text-secondary mt-1">{{ size(f) }}</p>
              </div>
            }
            @if (error(); as msg) {
              <div class="p-3 qt-bg-destructive/10 border qt-border-destructive rounded-lg">
                <p class="text-sm qt-text-destructive">{{ msg }}</p>
              </div>
            }
          </div>
        }
        @case ('preview') {
          @if (loadingPreview()) {
            <div class="flex justify-center py-8">
              <div class="animate-spin rounded-full h-8 w-8 border-b-2 qt-border-primary"></div>
            </div>
          } @else if (preview(); as p) {
            <div class="space-y-4">
              <div class="p-4 qt-bg-muted/50 rounded-lg space-y-2">
                <div>
                  <span class="qt-text-small qt-text-secondary">Export Type</span>
                  <div class="font-medium text-foreground">{{ p.manifest?.exportType }}</div>
                </div>
                <div>
                  <span class="qt-text-small qt-text-secondary">App Version</span>
                  <div class="font-medium text-foreground">{{ p.manifest?.appVersion }}</div>
                </div>
              </div>
              <div class="space-y-3 max-h-96 overflow-y-auto">
                @for (key of previewKeys(); track key) {
                  <div class="p-4 border qt-border-default rounded-lg space-y-2">
                    <div class="flex items-center justify-between">
                      <h4 class="font-medium text-foreground">{{ keyLabel(key) }}</h4>
                      <span class="qt-text-small qt-text-secondary">
                        {{ selectedCount(key) }} of {{ entitiesOf(key).length }}
                      </span>
                    </div>
                    <div class="space-y-1 max-h-32 overflow-y-auto">
                      @for (e of entitiesOf(key); track e.id) {
                        <label class="flex items-center gap-2 p-2 hover:qt-bg-muted/50 rounded cursor-pointer">
                          <input
                            type="checkbox"
                            class="w-4 h-4"
                            [checked]="isSelected(key, e.id)"
                            (change)="toggleSelection(key, e.id)"
                          />
                          <span class="text-foreground flex-1">{{ e.name || e.title }}</span>
                          @if (e.exists) {
                            <span class="text-xs qt-bg-warning/10 qt-text-warning px-2 py-1 rounded">Exists</span>
                          }
                        </label>
                      }
                    </div>
                  </div>
                }
              </div>
            </div>
          } @else if (error(); as msg) {
            <!--
              Dogfood #61/#63 — a DELIBERATE divergence from v4, under the
              2026-08-03 backup/restore/import/export ruling. loadPreview
              records the failure in the error signal but leaves the wizard on
              this step, and v4's ImportPreviewStep bails with a bare
              "if (!preview) return null" while its dialog renders state.error
              only on the file and error steps — so in BOTH apps a failed
              preview drew a blank step 2 with working Back/Next buttons and no
              hint of what went wrong. The v4 one-liner is queued post-5.0.
            -->
            <div class="p-3 qt-bg-destructive/10 border qt-border-destructive rounded-lg">
              <p class="text-sm qt-text-destructive">{{ msg }}</p>
            </div>
          }
        }
        @case ('options') {
          <div class="space-y-4">
            <div class="space-y-3">
              <p class="font-medium text-foreground">When an item already exists:</p>
              @for (strat of strategies; track strat.value) {
                <label class="flex items-center gap-3 cursor-pointer">
                  <input
                    type="radio"
                    name="conflict-strategy"
                    [checked]="conflictStrategy() === strat.value"
                    (change)="conflictStrategy.set(strat.value)"
                  />
                  <span class="text-foreground">{{ strat.label }}</span>
                </label>
              }
            </div>
            @if (hasMemories()) {
              <label class="flex items-start gap-3 p-4 border qt-border-default rounded-lg cursor-pointer">
                <input
                  type="checkbox"
                  class="w-4 h-4 mt-1"
                  [checked]="importMemories()"
                  (change)="importMemories.set($any($event.target).checked)"
                />
                <div>
                  <p class="font-medium text-foreground">Import associated memories</p>
                </div>
              </label>
            }
          </div>
        }
        @case ('importing') {
          <div class="flex flex-col items-center justify-center py-12">
            <div class="animate-spin rounded-full h-8 w-8 border-b-2 qt-border-primary"></div>
            <p class="mt-4 text-foreground font-medium">Importing data...</p>
          </div>
        }
        @case ('complete') {
          @if (result(); as r) {
            <div class="space-y-4">
              <div class="flex flex-col items-center justify-center py-6">
                <div class="w-12 h-12 rounded-full qt-bg-success/10 flex items-center justify-center">
                  <qt-icon name="check" class="w-6 h-6 qt-text-success" />
                </div>
                <h3 class="qt-heading-4 text-foreground mt-2">Import Complete</h3>
              </div>
              <div class="p-4 qt-bg-muted/50 rounded-lg space-y-2">
                @for (row of importedRows(r); track row.label) {
                  <div class="flex justify-between">
                    <span class="text-foreground capitalize">{{ row.label }}</span>
                    <span class="font-medium qt-text-success">+{{ row.value }}</span>
                  </div>
                }
                @for (row of skippedRows(r); track row.label) {
                  <div class="flex justify-between">
                    <span class="text-foreground capitalize">{{ row.label }} (skipped)</span>
                    <span class="font-medium qt-text-secondary">{{ row.value }}</span>
                  </div>
                }
              </div>
              @if (r.warnings && r.warnings.length > 0) {
                <div class="p-4 qt-bg-warning/10 border qt-border-warning/30 rounded-lg">
                  <h4 class="font-medium qt-text-warning mb-2">Warnings</h4>
                  <ul class="space-y-1">
                    @for (w of r.warnings; track $index) {
                      <li class="text-sm qt-text-warning">• {{ w }}</li>
                    }
                  </ul>
                </div>
              }
            </div>
          }
        }
        @case ('error') {
          <div class="space-y-4 py-6 text-center">
            <div class="w-12 h-12 rounded-full qt-bg-destructive/10 mx-auto flex items-center justify-center">
              <qt-icon name="close" class="w-6 h-6 qt-text-destructive" />
            </div>
            <h3 class="qt-heading-4 text-foreground">Import Failed</h3>
            @if (error(); as msg) {
              <div class="p-3 qt-bg-destructive/10 border qt-border-destructive rounded-lg">
                <p class="text-sm qt-text-destructive">{{ msg }}</p>
              </div>
            }
          </div>
        }
      }

      <div qt-modal-footer class="flex gap-3 justify-between w-full">
        @if (step() === 'complete') {
          <button type="button" class="flex-1 qt-button qt-button-primary" (click)="handleClose()">Close</button>
        } @else {
          <button
            type="button"
            class="qt-button qt-button-secondary"
            [disabled]="step() === 'file' || importing()"
            (click)="handleBack()"
          >
            Back
          </button>
          @if (step() !== 'importing') {
            <div class="flex gap-3">
              <button
                type="button"
                class="qt-button qt-button-secondary"
                [disabled]="importing()"
                (click)="handleClose()"
              >
                Cancel
              </button>
              @if (step() === 'options') {
                <button
                  type="button"
                  class="qt-button qt-button-primary"
                  [disabled]="importing()"
                  (click)="handleImport()"
                >
                  {{ importing() ? 'Importing...' : 'Import' }}
                </button>
              } @else {
                <button
                  type="button"
                  class="qt-button qt-button-primary"
                  [disabled]="(step() === 'file' && !exportData()) || loadingPreview()"
                  (click)="handleNext()"
                >
                  {{ loadingPreview() ? 'Loading...' : 'Next' }}
                </button>
              }
            </div>
          }
        }
      </div>
    </qt-modal>
  `,
})
export class ImportDialog {
  readonly close = output<void>();

  protected readonly strategies: { value: ConflictStrategy; label: string }[] = [
    { value: 'skip', label: 'Skip (keep mine)' },
    { value: 'overwrite', label: 'Overwrite with imported' },
    { value: 'duplicate', label: 'Import as a duplicate' },
  ];

  protected readonly step = signal<Step>('file');
  protected readonly selectedFile = signal<File | null>(null);
  protected readonly exportData = signal<unknown | null>(null);
  protected readonly preview = signal<ImportPreview | null>(null);
  protected readonly loadingPreview = signal(false);
  protected readonly conflictStrategy = signal<ConflictStrategy>('skip');
  protected readonly importMemories = signal(false);
  protected readonly selectedEntityIds = signal<Record<string, string[]>>({});
  protected readonly importing = signal(false);
  protected readonly result = signal<ImportResult | null>(null);
  protected readonly error = signal<string | null>(null);
  protected readonly dragActive = signal(false);

  protected readonly stepNumber = computed(() => {
    switch (this.step()) {
      case 'file':
        return 1;
      case 'preview':
        return 2;
      case 'options':
        return 3;
      case 'importing':
        return 4;
      case 'complete':
        return 5;
      default:
        return 1;
    }
  });

  protected readonly hasMemories = computed(() => {
    const mem = this.preview()?.entities?.['memories'] as { count?: number } | undefined;
    return !!mem && !!mem.count && mem.count > 0;
  });

  /** v4 `getEntityKeysInPreview` (`ImportPreviewStep.tsx:41-47`). */
  protected readonly previewKeys = computed(() => {
    const entities = this.preview()?.entities ?? {};
    return Object.keys(entities).filter((k) => k !== 'memories' && !!entities[k]);
  });

  protected entitiesOf(key: string): PreviewEntity[] {
    return (this.preview()?.entities?.[key] as PreviewEntity[] | undefined) ?? [];
  }
  protected selectedCount(key: string): number {
    return (this.selectedEntityIds()[key] ?? []).length;
  }
  protected isSelected(key: string, id: string): boolean {
    return (this.selectedEntityIds()[key] ?? []).includes(id);
  }
  protected keyLabel(key: string): string {
    return ENTITY_TYPE_LABELS[toExportEntityType(key)] ?? key;
  }
  protected size(f: File): string {
    return formatBytes(f.size);
  }

  protected importedRows(r: ImportResult): { label: string; value: number }[] {
    return Object.entries(r.imported ?? {})
      .filter(([, v]) => v !== 0)
      .map(([k, v]) => ({ label: this.spaced(k), value: v }));
  }
  protected skippedRows(r: ImportResult): { label: string; value: number }[] {
    return Object.entries(r.skipped ?? {})
      .filter(([, v]) => v !== 0)
      .map(([k, v]) => ({ label: this.spaced(k), value: v }));
  }
  private spaced(key: string): string {
    return key.replace(/([A-Z])/g, ' $1').trim();
  }

  protected toggleSelection(key: string, id: string): void {
    this.selectedEntityIds.update((map) => {
      const cur = map[key] ?? [];
      const next = cur.includes(id) ? cur.filter((x) => x !== id) : [...cur, id];
      return { ...map, [key]: next };
    });
  }

  // --- File handling -------------------------------------------------------

  protected onDragEnter(e: DragEvent): void {
    e.preventDefault();
    e.stopPropagation();
    this.dragActive.set(true);
  }
  protected onDragLeave(e: DragEvent): void {
    e.preventDefault();
    e.stopPropagation();
    this.dragActive.set(false);
  }
  protected onDragOver(e: DragEvent): void {
    e.preventDefault();
    e.stopPropagation();
  }
  protected onDrop(e: DragEvent): void {
    e.preventDefault();
    e.stopPropagation();
    this.dragActive.set(false);
    const file = e.dataTransfer?.files?.[0];
    if (file) void this.acceptFile(file);
  }
  protected onFileSelect(event: Event): void {
    const file = (event.target as HTMLInputElement).files?.[0];
    if (file) void this.acceptFile(file);
  }

  private async acceptFile(file: File): Promise<void> {
    try {
      const parsed = await this.parseExportFile(file);
      this.selectedFile.set(file);
      this.exportData.set(parsed);
      this.error.set(null);
    } catch (err) {
      this.error.set(err instanceof Error ? err.message : 'Failed to parse file');
    }
  }

  /** v4 `parseExportFile` (`useImportData.ts:53-103`) — NDJSON envelope vs legacy. */
  private async parseExportFile(file: File): Promise<unknown> {
    const PEEK = 256 * 1024;
    try {
      const prefix = await file.slice(0, PEEK).text();
      if (/"format"\s*:\s*"qtap-ndjson"/.test(prefix)) {
        const nl = prefix.indexOf('\n');
        if (nl === -1) throw new Error('Invalid export file format. NDJSON envelope line is missing a newline.');
        const envelope = JSON.parse(prefix.slice(0, nl));
        if (envelope.format !== 'qtap-ndjson' || !envelope.manifest) {
          throw new Error('Invalid NDJSON envelope in export file.');
        }
        return { manifest: envelope.manifest, data: {} };
      }
      const text = await file.text();
      const data = JSON.parse(text);
      if (!data.manifest || data.manifest.format !== 'quilltap-export') {
        throw new Error('Invalid export file format. Please select a valid Quilltap export file (.qtap).');
      }
      return data;
    } catch (err) {
      const message = err instanceof Error ? err.message : '';
      if (message.includes('Invalid') || message.includes('envelope')) throw err;
      throw new Error('Failed to parse file. Please ensure it is a valid Quilltap export file.');
    }
  }

  // --- Steps ---------------------------------------------------------------

  protected async handleNext(): Promise<void> {
    if (this.step() === 'file') {
      if (!this.exportData()) return;
      this.step.set('preview');
      await this.loadPreview();
    } else if (this.step() === 'preview') {
      this.step.set('options');
    }
  }

  protected handleBack(): void {
    this.error.set(null);
    switch (this.step()) {
      case 'preview':
        this.step.set('file');
        break;
      case 'options':
        this.step.set('preview');
        break;
      case 'error':
        this.step.set('options');
        break;
    }
  }

  private async loadPreview(): Promise<void> {
    const file = this.selectedFile();
    if (!file) return;
    this.loadingPreview.set(true);
    this.error.set(null);
    try {
      const form = new FormData();
      form.append('file', file);
      const res = await fetch(apiUrl('/api/v1/system/tools?action=import-preview'), {
        method: 'POST',
        body: form,
      });
      if (!res.ok) {
        throw new Error((await res.json().catch(() => ({}))).error || 'Failed to load preview');
      }
      const p = (await res.json()) as ImportPreview;
      this.preview.set(p);
      // v4 `:222-230` — auto-enable memories when the export carries any.
      const mem = p.entities?.['memories'] as { count?: number } | undefined;
      if (mem?.count && mem.count > 0) this.importMemories.set(true);
    } catch (err) {
      this.error.set(err instanceof Error ? err.message : 'Failed to load preview');
    } finally {
      this.loadingPreview.set(false);
    }
  }

  protected async handleImport(): Promise<void> {
    const file = this.selectedFile();
    if (!this.exportData() || !file) return;
    this.importing.set(true);
    this.error.set(null);
    this.step.set('importing');
    try {
      const form = new FormData();
      form.append('file', file);
      form.append(
        'options',
        JSON.stringify({
          selectedIds: this.selectedEntityIds(),
          conflictStrategy: this.conflictStrategy(),
          importMemories: this.importMemories(),
        }),
      );
      const res = await fetch(apiUrl('/api/v1/system/tools?action=import-execute'), {
        method: 'POST',
        body: form,
      });
      if (!res.ok) {
        throw new Error((await res.json().catch(() => ({}))).error || 'Failed to import data');
      }
      this.result.set((await res.json()) as ImportResult);
      this.step.set('complete');
    } catch (err) {
      this.error.set(err instanceof Error ? err.message : 'Failed to import data');
      this.step.set('error');
    } finally {
      this.importing.set(false);
    }
  }

  protected handleClose(): void {
    if (this.importing()) return;
    this.close.emit();
  }
}
