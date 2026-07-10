import {
  ChangeDetectionStrategy,
  Component,
  ElementRef,
  HostListener,
  computed,
  inject,
  input,
  output,
  signal,
} from '@angular/core';

import { Icon } from './icon';

/**
 * The model combobox (v4 `components/settings/model-selector.tsx`). A plain
 * `<select>` for ≤10 models; a search box + dropdown for more. The value is
 * controlled — a free-text fallback lives in the caller (the profile modal's
 * datalist input), this component only picks from the provided `models`.
 */
@Component({
  selector: 'qt-model-selector',
  changeDetection: ChangeDetectionStrategy.OnPush,
  imports: [Icon],
  template: `
    @if (!searchMode()) {
      <select class="qt-select" [disabled]="disabled()" (change)="pick($any($event.target).value)">
        <option value="" [selected]="!value()">{{ placeholder() }}</option>
        @for (m of sortedModels(); track m) {
          <option [value]="m" [selected]="m === value()">{{ m }}</option>
        }
      </select>
    } @else {
      <div class="relative">
        <div class="relative">
          <input
            type="text"
            class="qt-input pr-8"
            [placeholder]="placeholder()"
            [disabled]="disabled()"
            [value]="open() ? search() : value()"
            (focus)="open.set(true)"
            (input)="search.set($any($event.target).value)"
            (keydown)="onKeydown($event)"
          />
          <button
            type="button"
            class="absolute right-2 top-1/2 -translate-y-1/2 p-1 qt-hover-accent rounded"
            [disabled]="disabled()"
            (click)="open.set(!open())"
          >
            <qt-icon
              name="arrow-down"
              [class]="'w-4 h-4 transition-transform ' + (open() ? 'rotate-180' : '')"
            />
          </button>
        </div>

        @if (open()) {
          <div
            class="absolute top-full left-0 right-0 mt-1 qt-bg-card border qt-border-default rounded-lg qt-shadow-lg z-10 max-h-64 overflow-y-auto"
          >
            @if (filtered().length > 0) {
              <ul class="py-1">
                @for (m of filtered(); track m) {
                  <li>
                    <button
                      type="button"
                      [class]="
                        'w-full text-left px-3 py-2 qt-hover-accent transition-colors ' +
                        (m === value() ? 'bg-primary text-primary-foreground' : 'text-foreground')
                      "
                      (click)="select(m)"
                    >
                      {{ m }}
                    </button>
                  </li>
                }
              </ul>
            } @else {
              <div class="px-3 py-2 qt-text-small">No models match &quot;{{ search() }}&quot;</div>
            }
          </div>
        }
      </div>
      @if (showFetchedCount() && models().length > 0) {
        <p class="qt-text-xs mt-1">Showing {{ models().length }} fetched models from provider</p>
      }
    }
  `,
})
export class ModelSelector {
  private readonly host = inject(ElementRef<HTMLElement>);

  readonly models = input<string[]>([]);
  readonly value = input<string>('');
  readonly placeholder = input<string>('Select or search a model');
  readonly disabled = input(false);
  readonly showFetchedCount = input(false);
  readonly changed = output<string>();

  protected readonly open = signal(false);
  protected readonly search = signal('');

  protected readonly sortedModels = computed(() =>
    [...this.models()].sort((a, b) => a.localeCompare(b)),
  );
  protected readonly searchMode = computed(() => this.models().length > 10);
  protected readonly filtered = computed(() => {
    const q = this.search().toLowerCase();
    return q ? this.sortedModels().filter((m) => m.toLowerCase().includes(q)) : this.sortedModels();
  });

  @HostListener('document:click', ['$event'])
  protected onDocumentClick(ev: MouseEvent): void {
    if (this.open() && !this.host.nativeElement.contains(ev.target as Node)) {
      this.open.set(false);
    }
  }

  protected pick(value: string): void {
    this.changed.emit(value);
  }

  protected select(model: string): void {
    this.changed.emit(model);
    this.open.set(false);
    this.search.set('');
  }

  protected onKeydown(ev: KeyboardEvent): void {
    if (ev.key === 'Escape') {
      this.open.set(false);
      this.search.set('');
    }
  }
}
