import { ChangeDetectionStrategy, Component, inject, input, model, signal } from '@angular/core';
import { FormsModule } from '@angular/forms';

import { CoreClient } from '../../core/core-client';
import { Icon } from '../../ui/icon';
import { browseDirectory } from './scriptorium.api';
import type { BrowseDirectoryResult } from '../../core/core-contract';

/**
 * The Directory Picker (v4 `DirectoryPicker.tsx`): a text input + a Browse
 * button that opens an inline, server-backed folder browser over
 * `systemBrowseDirectory`. Navigate into subdirectories or up to the parent,
 * then Select to write the current path back. Two-way `[(value)]`.
 */
@Component({
  selector: 'qt-directory-picker',
  changeDetection: ChangeDetectionStrategy.OnPush,
  imports: [FormsModule, Icon],
  template: `
    <div>
      <div class="flex gap-2">
        <input
          type="text"
          [attr.name]="name()"
          [ngModel]="value()"
          (ngModelChange)="value.set($event)"
          [ngModelOptions]="{ standalone: true }"
          [required]="required()"
          [placeholder]="placeholder() || '/path/to/documents'"
          class="qt-input flex-1"
        />
        <button
          type="button"
          (click)="openBrowser()"
          class="qt-button-secondary inline-flex items-center gap-1.5 shrink-0"
          title="Browse folders"
        >
          <qt-icon name="folder" class="w-4 h-4" />
          Browse
        </button>
      </div>

      @if (browserOpen()) {
        <div class="mt-2 rounded-xl border qt-border-default qt-bg-card overflow-hidden">
          <div
            class="flex items-center gap-2 px-3 py-2 border-b qt-border-default qt-bg-muted/30"
          >
            @if (result()?.parent; as parent) {
              <button
                type="button"
                (click)="navigate(parent)"
                class="p-1 rounded hover:qt-bg-muted transition-colors shrink-0"
                title="Go to parent directory"
              >
                <qt-icon name="chevron-down" class="w-4 h-4 rotate-180" />
              </button>
            }
            <span
              class="text-xs font-mono qt-text-secondary truncate flex-1"
              [title]="browsePath() || ''"
            >
              {{ browsePath() || '...' }}
            </span>
          </div>

          <div class="max-h-48 overflow-y-auto">
            @if (loading()) {
              <div class="px-3 py-4 text-center text-sm qt-text-secondary">Loading...</div>
            } @else if (browseError(); as err) {
              <div class="px-3 py-4 text-center text-sm qt-text-destructive">{{ err }}</div>
            } @else if (result()?.error; as warn) {
              <div class="px-3 py-4 text-center text-sm qt-text-warning">{{ warn }}</div>
            } @else if (directories().length === 0) {
              <div class="px-3 py-4 text-center text-sm qt-text-secondary">No subdirectories</div>
            } @else {
              @for (dir of directories(); track dir.path) {
                <button
                  type="button"
                  (click)="navigate(dir.path)"
                  class="w-full flex items-center gap-2 px-3 py-1.5 text-left text-sm text-foreground hover:qt-bg-muted/50 transition-colors"
                >
                  <qt-icon name="folder" class="w-4 h-4 qt-text-secondary shrink-0" />
                  <span class="truncate">{{ dir.name }}</span>
                </button>
              }
            }
          </div>

          <div class="flex items-center justify-between gap-2 px-3 py-2 border-t qt-border-default">
            <span class="text-xs qt-text-secondary truncate">
              {{ browsePath() ? 'Select: ' + browsePath() : 'Navigate to a folder' }}
            </span>
            <div class="flex gap-2 shrink-0">
              <button type="button" (click)="cancel()" class="qt-button-secondary text-xs px-3 py-1">
                Cancel
              </button>
              <button type="button" (click)="select()" class="qt-button-primary text-xs px-3 py-1">
                Select
              </button>
            </div>
          </div>
        </div>
      }
    </div>
  `,
})
export class DirectoryPicker {
  private readonly core = inject(CoreClient);

  readonly value = model.required<string>();
  readonly name = input<string | undefined>(undefined);
  readonly placeholder = input<string | undefined>(undefined);
  readonly required = input(false);

  protected readonly browserOpen = signal(false);
  protected readonly browsePath = signal<string | null>(null);
  protected readonly result = signal<BrowseDirectoryResult | null>(null);
  protected readonly loading = signal(false);
  protected readonly browseError = signal<string | null>(null);

  protected directories(): BrowseDirectoryResult['directories'] {
    return this.result()?.directories ?? [];
  }

  protected openBrowser(): void {
    this.browserOpen.set(true);
    // Start from the current value if set, otherwise the home directory.
    void this.fetchDirectory(this.value() || undefined);
  }

  protected navigate(dirPath: string): void {
    void this.fetchDirectory(dirPath);
  }

  protected select(): void {
    const path = this.browsePath();
    if (path) {
      this.value.set(path);
    }
    this.browserOpen.set(false);
  }

  protected cancel(): void {
    this.browserOpen.set(false);
  }

  private async fetchDirectory(dirPath?: string): Promise<void> {
    this.loading.set(true);
    this.browseError.set(null);
    try {
      const data = await browseDirectory(this.core, dirPath);
      this.result.set(data);
      this.browsePath.set(data.path);
    } catch (err) {
      this.browseError.set(err instanceof Error ? err.message : 'Failed to browse');
    } finally {
      this.loading.set(false);
    }
  }
}
