import { ChangeDetectionStrategy, Component, computed, input, output } from '@angular/core';

import { formatBytes } from '../../ui/format-bytes';
import { Icon } from '../../ui/icon';
import type { DocumentStore } from './scriptorium.api';

/**
 * A document-store card (v4 `DocumentStoreCard.tsx`): name + path, the badge row
 * (contents / mount-type / scan / conversion / embedding / disabled), a 3-up
 * stat block, and the per-card actions (scan with spinner; convert OR deconvert
 * by mount type; edit; delete). Actions disable while scanning or mid-conversion.
 * Clicking the card body (not a button) opens the store.
 */
@Component({
  selector: 'qt-store-card',
  changeDetection: ChangeDetectionStrategy.OnPush,
  imports: [Icon],
  template: `
    <div
      class="qt-entity-card cursor-pointer hover:qt-border-primary/50 transition-colors"
      [class.opacity-60]="!store().enabled"
      (click)="open.emit()"
    >
      <div class="flex items-start justify-between mb-3">
        <div class="flex items-center gap-3 min-w-0">
          <div class="w-10 h-10 rounded-lg flex items-center justify-center qt-bg-muted shrink-0">
            <qt-icon name="database" class="w-5 h-5 qt-text-secondary" />
          </div>
          <div class="min-w-0">
            <h2 class="qt-section-title truncate">{{ store().name }}</h2>
            <p class="qt-text-small truncate" [title]="store().basePath || 'Database-backed — no path'">
              @if (store().basePath) {
                {{ store().basePath }}
              } @else {
                <em class="qt-text-secondary">Database-backed — no path</em>
              }
            </p>
          </div>
        </div>
      </div>

      <div class="flex flex-wrap items-center gap-2 mb-3">
        @if (store().storeType === 'character') {
          <span class="qt-badge-related inline-flex items-center" title="Character store">Character</span>
        } @else {
          <span class="qt-badge-info inline-flex items-center" title="Documents store">Documents</span>
        }

        @switch (store().mountType) {
          @case ('obsidian') {
            <span class="qt-badge-related inline-flex items-center">Obsidian</span>
          }
          @case ('database') {
            <span class="qt-badge-success inline-flex items-center">Database</span>
          }
          @default {
            <span class="qt-badge-info inline-flex items-center">Filesystem</span>
          }
        }

        @if (store().scanStatus === 'scanning') {
          <span class="qt-badge-warning inline-flex items-center gap-1">
            <span class="h-1.5 w-1.5 animate-pulse rounded-full qt-dot-warning"></span>Scanning
          </span>
        } @else if (store().scanStatus === 'error') {
          <span class="qt-badge-destructive inline-flex items-center gap-1" [title]="store().lastScanError || 'Scan error'">Error</span>
        }

        @if (store().conversionStatus === 'converting') {
          <span class="qt-badge-warning inline-flex items-center gap-1">
            <span class="h-1.5 w-1.5 animate-pulse rounded-full qt-dot-warning"></span>Converting
          </span>
        } @else if (store().conversionStatus === 'deconverting') {
          <span class="qt-badge-warning inline-flex items-center gap-1">
            <span class="h-1.5 w-1.5 animate-pulse rounded-full qt-dot-warning"></span>Deconverting
          </span>
        } @else if (store().conversionStatus === 'error') {
          <span class="qt-badge-destructive inline-flex items-center gap-1" [title]="store().conversionError || 'Conversion error'">Conversion error</span>
        }

        @if (store().chunkCount > 0) {
          @if (store().embeddedChunkCount === store().chunkCount) {
            <span class="qt-badge-success inline-flex items-center gap-1">
              <span class="h-1.5 w-1.5 rounded-full qt-dot-success"></span>Embedded
            </span>
          } @else {
            <span class="qt-badge-warning inline-flex items-center gap-1">
              <span class="h-1.5 w-1.5 animate-pulse rounded-full qt-dot-warning"></span>{{ embeddedPct() }}% embedded
            </span>
          }
        }

        @if (!store().enabled) {
          <span class="qt-badge-disabled inline-flex items-center">Disabled</span>
        }
      </div>

      <div class="grid grid-cols-3 gap-2 mb-3 text-center">
        <div class="rounded-lg qt-bg-muted/50 px-2 py-1.5">
          <div class="qt-section-title">{{ store().fileCount }}</div>
          <div class="text-xs qt-text-secondary">Files</div>
        </div>
        <div class="rounded-lg qt-bg-muted/50 px-2 py-1.5">
          <div class="qt-section-title">{{ totalSize() }}</div>
          <div class="text-xs qt-text-secondary">Total Size</div>
        </div>
        <div class="rounded-lg qt-bg-muted/50 px-2 py-1.5">
          <div class="text-xs qt-text-secondary mt-1">Scanned</div>
          <div class="qt-text-label-xs text-foreground">{{ lastScanned() }}</div>
        </div>
      </div>

      <div class="qt-entity-card-actions flex gap-2 flex-wrap">
        <button
          type="button"
          (click)="onScan($event)"
          [disabled]="actionsDisabled()"
          class="qt-button-secondary flex-1 inline-flex items-center justify-center gap-1.5"
          [class.opacity-50]="actionsDisabled()"
          [class.cursor-not-allowed]="actionsDisabled()"
          title="Scan for changes"
        >
          <qt-icon name="refresh" class="w-4 h-4" [class.animate-spin]="isScanning()" />
          {{ isScanning() ? 'Scanning...' : 'Scan' }}
        </button>

        @if (store().mountType === 'database') {
          <button
            type="button"
            (click)="onDeconvert($event)"
            [disabled]="actionsDisabled()"
            class="qt-button-secondary inline-flex items-center justify-center gap-1.5"
            [class.opacity-50]="actionsDisabled()"
            [class.cursor-not-allowed]="actionsDisabled()"
            title="Export everything out to a filesystem directory and switch to filesystem-backed"
          >
            <qt-icon name="download" class="w-4 h-4" [class.animate-pulse]="store().conversionStatus === 'deconverting'" />
            {{ store().conversionStatus === 'deconverting' ? 'Deconverting...' : 'Deconvert' }}
          </button>
        } @else {
          <button
            type="button"
            (click)="onConvert($event)"
            [disabled]="actionsDisabled()"
            class="qt-button-secondary inline-flex items-center justify-center gap-1.5"
            [class.opacity-50]="actionsDisabled()"
            [class.cursor-not-allowed]="actionsDisabled()"
            title="Move every file into the encrypted database and switch to database-backed"
          >
            <qt-icon name="upload" class="w-4 h-4" [class.animate-pulse]="store().conversionStatus === 'converting'" />
            {{ store().conversionStatus === 'converting' ? 'Converting...' : 'Convert' }}
          </button>
        }

        <button
          type="button"
          (click)="onEdit($event)"
          [disabled]="conversionInFlight()"
          class="qt-button-secondary"
          [class.opacity-50]="conversionInFlight()"
          [class.cursor-not-allowed]="conversionInFlight()"
          title="Edit document store"
        >
          <qt-icon name="pencil" class="w-4 h-4" />
        </button>
        <button
          type="button"
          (click)="onDelete($event)"
          [disabled]="conversionInFlight()"
          class="qt-button-destructive qt-shadow-sm"
          [class.opacity-50]="conversionInFlight()"
          [class.cursor-not-allowed]="conversionInFlight()"
          title="Delete document store"
        >
          <qt-icon name="trash" class="w-4 h-4" />
        </button>
      </div>
    </div>
  `,
})
export class StoreCard {
  readonly store = input.required<DocumentStore>();
  /** True while THIS card's scan request is in flight (the view's scanningIds set). */
  readonly scanning = input(false);

  readonly open = output<void>();
  readonly edit = output<void>();
  readonly delete = output<void>();
  readonly scan = output<void>();
  readonly convert = output<void>();
  readonly deconvert = output<void>();

  protected readonly conversionInFlight = computed(
    () =>
      this.store().conversionStatus === 'converting' ||
      this.store().conversionStatus === 'deconverting',
  );
  protected readonly isScanning = computed(
    () => this.scanning() || this.store().scanStatus === 'scanning',
  );
  protected readonly actionsDisabled = computed(() => this.isScanning() || this.conversionInFlight());

  protected readonly totalSize = computed(() => formatBytes(this.store().totalSizeBytes));
  protected readonly embeddedPct = computed(() =>
    Math.round((this.store().embeddedChunkCount / this.store().chunkCount) * 100),
  );
  protected readonly lastScanned = computed(() => {
    const at = this.store().lastScannedAt;
    if (!at) {
      return 'Never';
    }
    return new Date(at).toLocaleDateString(undefined, {
      month: 'short',
      day: 'numeric',
      hour: '2-digit',
      minute: '2-digit',
    });
  });

  protected onScan(e: Event): void {
    e.stopPropagation();
    this.scan.emit();
  }
  protected onEdit(e: Event): void {
    e.stopPropagation();
    this.edit.emit();
  }
  protected onDelete(e: Event): void {
    e.stopPropagation();
    this.delete.emit();
  }
  protected onConvert(e: Event): void {
    e.stopPropagation();
    this.convert.emit();
  }
  protected onDeconvert(e: Event): void {
    e.stopPropagation();
    this.deconvert.emit();
  }
}
