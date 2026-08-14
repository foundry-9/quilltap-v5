import {
  ChangeDetectionStrategy,
  Component,
  ElementRef,
  computed,
  effect,
  inject,
  input,
  output,
  signal,
  viewChild,
} from '@angular/core';

import { readRecents } from './recents-storage';
import { findByCodePoint, searchChars } from './search';
import type { CharEntry, CharIndex, CharProfile } from './types';

/** Cells per row. Arrow-key navigation is computed from this, so keep them in step. */
const COLUMNS = 8;

/** Results shown for a non-empty query. Generous — this surface is for browsing. */
const SEARCH_LIMIT = 64;

/**
 * One grid cell, carrying its index in the panel's visual coordinate space —
 * assigned while the sections are built (v4's inline `cellIndex += 1`). Deriving
 * it in the template instead would be a linear scan per cell over ~1,900 of
 * them.
 */
interface CharCell {
  entry: CharEntry;
  position: number;
}

interface CharSection {
  key: string;
  label: string;
  rows: CharCell[][];
}

function chunk<T>(items: T[], size: number): T[][] {
  const rows: T[][] = [];
  for (let i = 0; i < items.length; i += size) rows.push(items.slice(i, i + size));
  return rows;
}

/** Number the entries from `start` and chunk them into rows. */
function cells(entries: CharEntry[], start: number): CharCell[][] {
  return chunk(
    entries.map((entry, i) => ({ entry, position: start + i })),
    COLUMNS,
  );
}

/**
 * `qt-char-picker-panel` — the browsable half of the character-insertion
 * feature, shared by both profiles (v4
 * `components/chat/char-insert/CharPickerPanel.tsx`): a search field over the
 * same Tier B engine the inline typeahead uses, a Recents row, and a grid
 * grouped by the dataset's own categories (emoji groups, or Unicode blocks).
 *
 * `qt-emoji-picker-popover` and `qt-unicode-picker-popover` are thin wrappers
 * that supply a profile. Everything here is profile-agnostic — a third dataset
 * would need no changes.
 *
 * Rendered ONLY while open (the caller gates it with `@if`), which is what lets
 * "what to show when the picker opens" be initial state rather than an effect on
 * an `isOpen` flip. Dismissal follows v5's `RngDropdown` (a document `mousedown`
 * listener, plus Escape) and the surface is `qt-popover`, matching v4's own
 * choice to take one precedent from each.
 *
 * NOTE: the toolbar this hangs from renders in composition / document-editing
 * mode, so the buttons that open it are a convenience, not the feature. The
 * inline triggers are the primary surface and are always available.
 *
 * @module editor/char-insert/char-picker-panel
 */
@Component({
  selector: 'qt-char-picker-panel',
  changeDetection: ChangeDetectionStrategy.OnPush,
  host: {
    class: 'absolute bottom-full left-0 z-50 mb-1',
    '(document:mousedown)': 'onDocumentMouseDown($event)',
    '(document:keydown.escape)': 'close.emit()',
  },
  template: `
    <div class="qt-popover qt-emoji-picker">
      <input
        #search
        type="text"
        class="qt-input qt-emoji-picker-search"
        [value]="query()"
        [placeholder]="profile().ui.searchPlaceholder"
        [attr.aria-label]="profile().ui.searchAriaLabel"
        (input)="onQuery($any($event.target).value)"
        (keydown)="onSearchKeyDown($event)"
      />

      @if (failed()) {
        <div class="qt-emoji-picker-message">{{ profile().ui.failedLabel }}</div>
      } @else if (!index()) {
        <div class="qt-emoji-picker-message">{{ profile().ui.loadingLabel }}</div>
      } @else if (sections().length === 0) {
        <div class="qt-emoji-picker-message">{{ profile().ui.emptyLabel }}</div>
      } @else {
        <div class="qt-emoji-picker-grid" role="grid" [attr.aria-label]="profile().ui.gridLabel">
          @for (section of sections(); track section.key) {
            <div role="rowgroup">
              <div class="qt-emoji-picker-group-header" role="presentation">
                {{ section.label }}
              </div>
              @for (row of section.rows; track row[0].entry.char) {
                <div role="row" class="qt-emoji-picker-row">
                  @for (cell of row; track cell.entry.char) {
                    <button
                      type="button"
                      role="gridcell"
                      class="qt-emoji-picker-cell"
                      [attr.data-cell]="cell.position"
                      [tabIndex]="cell.position === activeIndex() ? 0 : -1"
                      [attr.aria-label]="cell.entry.name"
                      [title]="cell.entry.name"
                      (mousedown)="$event.preventDefault()"
                      (click)="commit(cell.entry)"
                      (focus)="activeIndex.set(cell.position)"
                      (keydown)="onGridKeyDown($event, cell.position)"
                    >
                      {{ cell.entry.char }}
                    </button>
                  }
                </div>
              }
            </div>
          }
        </div>
      }
    </div>
  `,
})
export class CharPickerPanel {
  /** Which dataset to browse. */
  readonly profile = input.required<CharProfile>();

  /** The chosen character — the host inserts it into its editor. */
  readonly pick = output<string>();
  readonly close = output<void>();

  private readonly host: ElementRef<HTMLElement> = inject(ElementRef);
  private readonly searchField = viewChild<ElementRef<HTMLInputElement>>('search');

  protected readonly query = signal('');
  protected readonly activeIndex = signal(0);
  protected readonly failed = signal(false);
  /** Read once, on mount — i.e. when the picker opens. */
  protected readonly index = signal<CharIndex | null>(null);
  private readonly recents = signal<string[]>([]);

  constructor() {
    effect(() => {
      const profile = this.profile();
      this.index.set(profile.loader.getLoaded());
      this.recents.set(readRecents(profile));

      // Fetch on first open only — an instance that never opens the picker and
      // never types the opener never pays for the dataset at all.
      if (profile.loader.getLoaded()) return;
      void profile.loader.load().then((loaded) => {
        if (loaded) this.index.set(loaded);
        else this.failed.set(true);
      });
    });

    // The search field is where a picker opens; focus it (v4's mount effect).
    effect(() => this.searchField()?.nativeElement.focus());
  }

  private readonly byChar = computed(() => {
    const index = this.index();
    if (!index) return new Map<string, CharEntry>();
    return new Map(index.entries.map((entry) => [entry.char, entry]));
  });

  protected readonly sections = computed<CharSection[]>(() => {
    const index = this.index();
    if (!index) return [];
    const profile = this.profile();

    const trimmed = this.query().trim();
    if (trimmed.length > 0) {
      const found = searchChars(index, trimmed, SEARCH_LIMIT);

      // `u+2192` typed into the search field resolves arithmetically, so the
      // picker reaches characters the curation dropped just as the typeahead
      // does.
      const direct = profile.codePointQueries ? findByCodePoint(index, trimmed) : null;
      const entries = direct
        ? [direct, ...found.filter((entry) => entry.char !== direct.char)]
        : found;

      return entries.length > 0
        ? [{ key: 'results', label: 'Results', rows: cells(entries, 0) }]
        : [];
    }

    const result: CharSection[] = [];
    let position = 0;

    const recentEntries = this.recents()
      .map((char) => this.byChar().get(char))
      .filter((entry): entry is CharEntry => entry !== undefined);
    if (recentEntries.length > 0) {
      result.push({ key: 'recents', label: 'Recently used', rows: cells(recentEntries, position) });
      position += recentEntries.length;
    }

    for (const slug of index.groups) {
      const entries = index.entries.filter((entry) => entry.group === slug);
      if (entries.length > 0) {
        result.push({
          key: slug,
          label: profile.groupLabels[slug] ?? slug,
          rows: cells(entries, position),
        });
        position += entries.length;
      }
    }

    return result;
  });

  /** Every cell in visual order — the coordinate space arrow keys move through. */
  private readonly flatEntries = computed(() =>
    this.sections().flatMap((section) => section.rows.flat().map((cell) => cell.entry)),
  );

  protected onQuery(value: string): void {
    this.query.set(value);
    this.activeIndex.set(0);
  }

  protected commit(entry: CharEntry): void {
    this.pick.emit(entry.char);
    this.close.emit();
  }

  protected onDocumentMouseDown(event: Event): void {
    // The toggling button lives outside this component; the toolbar closes on
    // its own click, so anything outside this panel dismisses.
    if (this.host.nativeElement.contains(event.target as Node)) return;
    this.close.emit();
  }

  protected onSearchKeyDown(event: KeyboardEvent): void {
    const entries = this.flatEntries();
    if (event.key === 'ArrowDown' && entries.length > 0) {
      event.preventDefault();
      this.focusCell(0);
      return;
    }
    if (event.key === 'Enter' && entries.length > 0) {
      event.preventDefault();
      this.commit(entries[0]);
    }
  }

  protected onGridKeyDown(event: KeyboardEvent, current: number): void {
    const last = this.flatEntries().length - 1;
    let next: number | null = null;

    if (event.key === 'ArrowRight') next = Math.min(current + 1, last);
    else if (event.key === 'ArrowLeft') next = Math.max(current - 1, 0);
    else if (event.key === 'ArrowDown') next = Math.min(current + COLUMNS, last);
    else if (event.key === 'ArrowUp') {
      // Stepping off the top row returns to the search field rather than
      // trapping the keyboard inside the grid.
      if (current - COLUMNS < 0) {
        event.preventDefault();
        this.searchField()?.nativeElement.focus();
        return;
      }
      next = current - COLUMNS;
    } else if (event.key === 'Home') next = 0;
    else if (event.key === 'End') next = last;

    if (next === null) return;
    event.preventDefault();
    this.focusCell(next);
  }

  private focusCell(next: number): void {
    this.activeIndex.set(next);
    this.host.nativeElement
      .querySelector<HTMLButtonElement>(`.qt-emoji-picker-cell[data-cell="${next}"]`)
      ?.focus();
  }
}
