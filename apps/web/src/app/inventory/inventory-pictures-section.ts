import {
  ChangeDetectionStrategy,
  Component,
  computed,
  effect,
  inject,
  input,
  output,
  signal,
  untracked,
} from '@angular/core';

import { apiUrl } from '../core/api-url';
import { CoreClient } from '../core/core-client';
import type { InventoryImageSummary, ResolvedInventoryItem } from '../core/core-contract';
import { ToastService } from '../ui/toast.service';
import type { InventoryContainer } from './inventory-container';
import { InventoryImageViewer } from './inventory-image-viewer';
import { describeInventoryRefusal } from './inventory-refusal';
import {
  deleteInventoryItemImage,
  generateInventoryItemImage,
  listInventoryImages,
  setCurrentInventoryItemImage,
  uploadInventoryItemImage,
} from './inventory.api';

/**
 * An existing object's pictures: generate, upload, choose, delete — a port of
 * v4 `InventoryPicturesSection` (`inventory-item-editor.tsx:451-554`,
 * `322a5488e`). An instance with no picture of its own shows its archetype's
 * (`inherited`), which it can view but not choose or delete. Each action runs
 * one at a time (`busy`), then re-reads the pictures and the browser's lists
 * (v4 `refresh` — `images(itemId)` + `inventory.lists`); a refusal toasts in
 * words with `'The picture would not oblige'` as the fallback.
 *
 * `viewingChange` tells the editor when the full-screen viewer is up, so its
 * Escape stands down (the viewer owns the key then).
 */
@Component({
  selector: 'qt-inventory-pictures-section',
  changeDetection: ChangeDetectionStrategy.OnPush,
  imports: [InventoryImageViewer],
  template: `
    <section class="space-y-2 border-t qt-border-default pt-3">
      <div class="flex items-center justify-between gap-2">
        <h3 class="text-sm text-foreground">Pictures</h3>
        <div class="flex items-center gap-2">
          <label class="qt-button-ghost qt-button-sm cursor-pointer">
            Upload…
            <input type="file" accept="image/*" class="hidden" (change)="onFile($event)" />
          </label>
          <button
            type="button"
            class="qt-button-secondary qt-button-sm"
            [disabled]="busy() !== null"
            (click)="generate()"
          >
            {{ busy() === 'generate' ? 'Drawing…' : 'Generate' }}
          </button>
        </div>
      </div>
      @if (inherited()) {
        <p class="qt-text-xs qt-text-secondary">
          Showing the archetype's pictures; upload or generate one to give this object its own.
        </p>
      }
      @if (loading()) {
        <div class="qt-text-xs qt-text-secondary">Loading pictures…</div>
      } @else if (images().length === 0) {
        <div class="qt-text-xs qt-text-secondary">
          No pictures yet. A picture is for choosing the object in pickers; rows show its glyph.
        </div>
      } @else {
        <ul class="flex flex-wrap gap-2">
          @for (image of images(); track image.fileId; let index = $index) {
            <li class="flex flex-col items-center gap-1">
              <button type="button" (click)="view(index)" aria-label="View picture full screen">
                <img
                  [src]="thumb(image)"
                  alt=""
                  [attr.class]="
                    'w-20 h-20 rounded object-cover border ' +
                    (image.fileId === current() ? 'qt-border-primary' : 'qt-border-default')
                  "
                />
              </button>
              @if (!image.inherited) {
                <div class="flex gap-1">
                  <button
                    type="button"
                    class="qt-button-ghost qt-button-sm"
                    [disabled]="image.fileId === current() || busy() !== null"
                    (click)="useImage(image.fileId)"
                  >
                    {{ image.fileId === current() ? 'Current' : 'Use' }}
                  </button>
                  <button
                    type="button"
                    class="qt-button-ghost qt-button-sm qt-text-destructive"
                    [disabled]="busy() !== null"
                    (click)="deleteImage(image.fileId)"
                  >
                    Delete
                  </button>
                </div>
              }
            </li>
          }
        </ul>
      }
      @if (viewing() !== null && images()[viewing()!]; as shown) {
        <qt-inventory-image-viewer
          [itemId]="item().id"
          [itemTitle]="item().title || item().name"
          [container]="container()"
          [fileId]="shown.fileId"
          [onPrev]="walk().prev"
          [onNext]="walk().next"
          (closed)="close()"
        />
      }
    </section>
  `,
})
export class InventoryPicturesSection {
  private readonly core = inject(CoreClient);
  private readonly toasts = inject(ToastService);

  readonly container = input.required<InventoryContainer>();
  readonly item = input.required<ResolvedInventoryItem>();
  /** The host's list reload (v4 `refresh`'s `inventory.lists` half). */
  readonly refreshLists = input<() => Promise<void>>(async () => undefined);
  readonly viewingChange = output<boolean>();

  protected readonly images = signal<InventoryImageSummary[]>([]);
  protected readonly current = signal<string | null>(null);
  protected readonly loading = signal(true);
  protected readonly busy = signal<string | null>(null);
  protected readonly viewing = signal<number | null>(null);
  protected readonly inherited = computed(() => this.images().some((image) => image.inherited));

  /** Previous / Next walk the pictures modulo their count (only with more than one). */
  protected readonly walk = computed(() => {
    const count = this.images().length;
    const at = this.viewing();
    if (count <= 1 || at === null) return { prev: undefined, next: undefined };
    return {
      prev: () => this.viewing.set((at - 1 + count) % count),
      next: () => this.viewing.set((at + 1) % count),
    };
  });

  constructor() {
    effect(() => {
      this.container();
      this.item().id;
      untracked(() => void this.load());
    });
  }

  protected thumb(image: InventoryImageSummary): string {
    return apiUrl(image.thumbnailUrl);
  }

  private async load(): Promise<void> {
    try {
      const data = await listInventoryImages(this.core, this.container(), this.item().id);
      this.images.set(data.images);
      this.current.set(data.current);
    } catch {
      this.images.set([]);
      this.current.set(null);
    } finally {
      this.loading.set(false);
    }
  }

  /** v4 `refresh` — the pictures and the lists. */
  private async refresh(): Promise<void> {
    await Promise.all([this.load(), this.refreshLists()()]);
  }

  /** v4 `run(label, fn)` (`:463-472`). */
  private async run(label: string, fn: () => Promise<unknown>): Promise<void> {
    this.busy.set(label);
    try {
      await fn();
      await this.refresh();
    } catch (error) {
      this.toasts.showError(describeInventoryRefusal(error, 'The picture would not oblige'));
    } finally {
      this.busy.set(null);
    }
  }

  protected onFile(event: Event): void {
    const input = event.target as HTMLInputElement;
    const file = input.files?.[0];
    if (file) void this.run('upload', () => uploadInventoryItemImage(this.container(), this.item().id, file));
  }

  protected generate(): void {
    void this.run('generate', () => generateInventoryItemImage(this.core, this.container(), this.item().id));
  }

  protected useImage(fileId: string): void {
    void this.run('current', () => setCurrentInventoryItemImage(this.core, this.container(), this.item().id, fileId));
  }

  protected deleteImage(fileId: string): void {
    void this.run('delete', () => deleteInventoryItemImage(this.core, this.container(), this.item().id, fileId));
  }

  protected view(index: number): void {
    this.viewing.set(index);
    this.viewingChange.emit(true);
  }

  protected close(): void {
    this.viewing.set(null);
    this.viewingChange.emit(false);
  }
}
