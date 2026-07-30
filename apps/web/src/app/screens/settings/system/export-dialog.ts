import { ChangeDetectionStrategy, Component, computed, inject, output, signal } from '@angular/core';

import { apiUrl } from '../../../core/api-url';
import { CoreClient } from '../../../core/core-client';
import { CoreDispatchError } from '../../../core/core-contract';
import { Icon } from '../../../ui/icon';
import { Modal } from '../../../ui/modal';
import { triggerBlobDownload } from '../../../core/download-utils';
import {
  ENTITY_TYPE_LABELS,
  EXPORTABLE_TYPES,
  type AvailableEntity,
  type ExportEntityType,
} from './import-export.types';

type Step = 'type' | 'select' | 'options' | 'exporting' | 'complete' | 'error';
type Scope = 'all' | 'selected';

/**
 * The Export Data dialog (v4 `components/tools/export-dialog.tsx` +
 * `hooks/useExportData.ts`): a 5-step wizard — pick a type, pick scope/entities,
 * (characters/chats only) an options step, then stream a `.qtap` download. The
 * entity listing rides `systemExportEntities`; the streamed export is a web-edge
 * leg (`POST /system/tools?action=export`) fetched via `apiUrl()` and consumed
 * as a Blob. v4's total-step counter is quirky BY DESIGN (Options reads "3 of 4",
 * error "1 of 5") — reproduced. Live behaviour is ACTIVATE-AT-UNIFY.
 */
@Component({
  selector: 'qt-export-dialog',
  changeDetection: ChangeDetectionStrategy.OnPush,
  imports: [Icon, Modal],
  template: `
    <qt-modal title="Export Data" maxWidth="2xl" [closeOnBackdrop]="!exporting()" (close)="handleClose()">
      <p class="qt-dialog-description mb-4">Step {{ stepNumber() }} of {{ totalSteps() }}</p>

      @switch (step()) {
        @case ('type') {
          <div class="space-y-4">
            <p class="qt-text-small qt-text-secondary">Select the type of data you want to export.</p>
            <div class="space-y-2">
              @for (t of exportableTypes; track t) {
                <label
                  class="flex items-center p-4 border-2 rounded-lg cursor-pointer transition-colors"
                  [class]="
                    entityType() === t
                      ? 'qt-border-primary qt-bg-primary/10'
                      : 'qt-border-default bg-background hover:qt-border-primary/50'
                  "
                >
                  <input
                    type="radio"
                    name="entity-type"
                    class="w-4 h-4"
                    [value]="t"
                    [checked]="entityType() === t"
                    (change)="setEntityType(t)"
                  />
                  <span class="ml-3 font-medium text-foreground">{{ label(t) }}</span>
                </label>
              }
            </div>
          </div>
        }
        @case ('select') {
          <div class="space-y-4">
            <div class="space-y-3">
              <label class="flex items-center gap-3 cursor-pointer">
                <input type="radio" name="scope" [checked]="scope() === 'all'" (change)="setScope('all')" />
                Export All
              </label>
              <label class="flex items-center gap-3 cursor-pointer">
                <input
                  type="radio"
                  name="scope"
                  [checked]="scope() === 'selected'"
                  (change)="setScope('selected')"
                />
                Select Specific
              </label>
            </div>
            @if (scope() === 'selected') {
              <div class="space-y-3 mt-4 pt-4 border-t qt-border-default">
                <input
                  type="text"
                  class="w-full px-3 py-2 border qt-border-default rounded-lg bg-background text-foreground"
                  placeholder="Search entities..."
                  [value]="searchQuery()"
                  (input)="searchQuery.set($any($event.target).value)"
                />
                @if (loadingEntities()) {
                  <div class="flex justify-center py-6">
                    <div class="animate-spin rounded-full h-6 w-6 border-b-2 qt-border-primary"></div>
                  </div>
                } @else if (filteredEntities().length === 0) {
                  <div class="text-center py-6 qt-text-secondary">No entities found</div>
                } @else {
                  <div class="space-y-2 max-h-64 overflow-y-auto">
                    @for (e of filteredEntities(); track e.id) {
                      <label class="flex items-center gap-3 p-2 hover:qt-bg-muted/50 rounded cursor-pointer">
                        <input
                          type="checkbox"
                          class="w-4 h-4"
                          [checked]="selectedIds().includes(e.id)"
                          (change)="toggleSelection(e.id)"
                        />
                        <span class="text-foreground">{{ e.name }}</span>
                      </label>
                    }
                  </div>
                  <div class="text-sm qt-text-secondary pt-2">
                    {{ selectedIds().length }} of {{ availableEntities().length }} selected
                  </div>
                }
              </div>
            }
          </div>
        }
        @case ('options') {
          <div class="space-y-4">
            <p class="qt-text-small qt-text-secondary">Configure export options</p>
            <label class="flex items-start gap-3 p-4 border qt-border-default rounded-lg cursor-pointer hover:qt-bg-muted/50">
              <input
                type="checkbox"
                class="w-4 h-4 mt-1"
                [checked]="includeMemories()"
                (change)="includeMemories.set($any($event.target).checked)"
              />
              <div>
                <p class="font-medium text-foreground">Include associated memories</p>
                @if (memoryCount() > 0) {
                  <p class="qt-text-small qt-text-secondary mt-1">
                    {{ memoryCount() }} memories will be included
                  </p>
                }
              </div>
            </label>
          </div>
        }
        @case ('exporting') {
          <div class="flex flex-col items-center justify-center py-12">
            <div class="animate-spin rounded-full h-8 w-8 border-b-2 qt-border-primary"></div>
            <p class="mt-4 text-foreground font-medium">Creating export...</p>
          </div>
        }
        @case ('complete') {
          <div class="space-y-4 py-6 text-center">
            <div class="w-12 h-12 rounded-full qt-bg-success/10 mx-auto flex items-center justify-center">
              <qt-icon name="check" class="w-6 h-6 qt-text-success" />
            </div>
            <h3 class="qt-heading-4 text-foreground">Export Complete</h3>
            <p class="qt-text-small qt-text-secondary">
              Your data has been successfully exported and downloaded.
            </p>
          </div>
        }
        @case ('error') {
          <div class="space-y-4 py-6 text-center">
            <div class="w-12 h-12 rounded-full qt-bg-destructive/10 mx-auto flex items-center justify-center">
              <qt-icon name="close" class="w-6 h-6 qt-text-destructive" />
            </div>
            <h3 class="qt-heading-4 text-foreground">Export Failed</h3>
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
            [disabled]="step() === 'type' || exporting()"
            (click)="handleBack()"
          >
            Back
          </button>
          @if (step() !== 'exporting') {
            <div class="flex gap-3">
              <button
                type="button"
                class="qt-button qt-button-secondary"
                [disabled]="exporting()"
                (click)="handleClose()"
              >
                Cancel
              </button>
              @if (showExportButton()) {
                <button
                  type="button"
                  class="qt-button qt-button-primary"
                  [disabled]="exporting() || (step() === 'select' && scope() === 'selected' && selectedIds().length === 0)"
                  (click)="handleExport()"
                >
                  {{ exporting() ? 'Exporting...' : 'Export' }}
                </button>
              } @else {
                <button
                  type="button"
                  class="qt-button qt-button-primary"
                  [disabled]="nextDisabled()"
                  (click)="handleNext()"
                >
                  Next
                </button>
              }
            </div>
          }
        }
      </div>
    </qt-modal>
  `,
})
export class ExportDialog {
  private readonly core = inject(CoreClient);

  readonly close = output<void>();

  protected readonly exportableTypes = EXPORTABLE_TYPES;

  protected readonly step = signal<Step>('type');
  protected readonly entityType = signal<ExportEntityType | null>(null);
  protected readonly scope = signal<Scope>('all');
  protected readonly selectedIds = signal<string[]>([]);
  protected readonly availableEntities = signal<AvailableEntity[]>([]);
  protected readonly loadingEntities = signal(false);
  protected readonly includeMemories = signal(false);
  protected readonly exporting = signal(false);
  protected readonly error = signal<string | null>(null);
  protected readonly searchQuery = signal('');

  private supportsMemories(t: ExportEntityType | null): boolean {
    return t === 'characters' || t === 'chats';
  }

  /** v4 memory-count recompute (`useExportData.ts:75-99`). */
  protected readonly memoryCount = computed(() => {
    if (!this.supportsMemories(this.entityType())) return 0;
    const entities = this.availableEntities();
    if (this.scope() === 'all') {
      return entities.reduce((sum, e) => sum + (e.memoryCount ?? 0), 0);
    }
    const ids = this.selectedIds();
    return entities.filter((e) => ids.includes(e.id)).reduce((sum, e) => sum + (e.memoryCount ?? 0), 0);
  });

  protected readonly filteredEntities = computed(() => {
    const q = this.searchQuery().toLowerCase();
    return this.availableEntities().filter((e) => e.name.toLowerCase().includes(q));
  });

  /** v4 `:52-69` getTotalSteps — the quirk carried verbatim. */
  protected readonly totalSteps = computed(() => {
    const step = this.step();
    if (step === 'exporting' || step === 'complete' || step === 'error') return 5;
    const t = this.entityType();
    if (t === 'characters' || t === 'chats') return step === 'type' || step === 'select' ? 5 : 4;
    return 4;
  });

  protected readonly stepNumber = computed(() => {
    switch (this.step()) {
      case 'type':
        return 1;
      case 'select':
        return 2;
      case 'options':
        return 3;
      case 'exporting':
        return 4;
      case 'complete':
        return 5;
      default:
        return 1;
    }
  });

  /** v4 `:199` — the Export-vs-Next branch. */
  protected readonly showExportButton = computed(() => {
    const t = this.entityType();
    return (
      this.step() === 'options' ||
      (this.step() === 'select' && !!t && t !== 'characters' && t !== 'chats')
    );
  });

  protected readonly nextDisabled = computed(
    () =>
      (this.step() === 'type' && !this.entityType()) ||
      (this.step() === 'select' && this.scope() === 'selected' && this.selectedIds().length === 0),
  );

  protected label(t: string): string {
    return ENTITY_TYPE_LABELS[t] ?? t;
  }

  protected setEntityType(t: ExportEntityType): void {
    this.entityType.set(t);
    this.scope.set('all');
    this.selectedIds.set([]);
    this.error.set(null);
  }

  protected setScope(scope: Scope): void {
    this.scope.set(scope);
    if (scope === 'all') this.selectedIds.set([]);
  }

  protected toggleSelection(id: string): void {
    this.selectedIds.update((ids) => (ids.includes(id) ? ids.filter((x) => x !== id) : [...ids, id]));
  }

  protected async handleNext(): Promise<void> {
    if (this.step() === 'type') {
      if (!this.entityType()) return;
      this.step.set('select');
      await this.loadAvailableEntities(this.entityType()!);
    } else if (this.step() === 'select') {
      if (this.scope() === 'selected' && this.selectedIds().length === 0) return;
      this.step.set(this.supportsMemories(this.entityType()) ? 'options' : 'exporting');
    } else if (this.step() === 'options') {
      this.step.set('exporting');
    }
  }

  protected handleBack(): void {
    this.error.set(null);
    switch (this.step()) {
      case 'select':
        this.step.set('type');
        break;
      case 'options':
        this.step.set('select');
        break;
      case 'exporting':
      case 'error':
        this.step.set(this.supportsMemories(this.entityType()) ? 'options' : 'select');
        break;
    }
  }

  private async loadAvailableEntities(entityType: ExportEntityType): Promise<void> {
    this.loadingEntities.set(true);
    this.error.set(null);
    try {
      const data = await this.core.dispatchData({ type: 'systemExportEntities', entityType });
      this.availableEntities.set((data['entities'] as AvailableEntity[] | undefined) ?? []);
    } catch (err) {
      this.error.set(this.msg(err, 'Failed to load entities'));
    } finally {
      this.loadingEntities.set(false);
    }
  }

  protected async handleExport(): Promise<void> {
    const entityType = this.entityType();
    if (!entityType) return;
    this.step.set('exporting');
    this.exporting.set(true);
    this.error.set(null);
    try {
      const res = await fetch(apiUrl('/api/v1/system/tools?action=export'), {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({
          type: entityType,
          scope: this.scope(),
          selectedIds: this.scope() === 'selected' ? this.selectedIds() : undefined,
          includeMemories: this.includeMemories(),
        }),
      });
      if (!res.ok) {
        let detail = 'Failed to create export';
        try {
          detail = (await res.json()).error ?? detail;
        } catch {
          /* non-JSON body */
        }
        throw new Error(detail);
      }
      const blob = await res.blob();
      const filename = `quilltap-${entityType}-${new Date().toISOString().split('T')[0]}.qtap`;
      triggerBlobDownload(blob, filename);
      this.step.set('complete');
    } catch (err) {
      this.error.set(this.msg(err, 'Failed to create export'));
      this.step.set('error');
    } finally {
      this.exporting.set(false);
    }
  }

  protected handleClose(): void {
    if (this.exporting()) return;
    this.close.emit();
  }

  private msg(err: unknown, fallback: string): string {
    if (err instanceof CoreDispatchError || err instanceof Error) return err.message || fallback;
    return fallback;
  }
}
