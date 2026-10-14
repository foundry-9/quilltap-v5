import {
  afterNextRender,
  ChangeDetectionStrategy,
  Component,
  computed,
  DestroyRef,
  ElementRef,
  inject,
  input,
  output,
  signal,
  viewChild,
} from '@angular/core';
import { DomSanitizer, type SafeHtml } from '@angular/platform-browser';
import { injectQuery } from '@tanstack/angular-query-experimental';

import { CoreClient, coreErrorMessage } from '../core/core-client';
import type { EverywhereScenario, EverywhereSource } from '../core/core-contract';
import { renderGenerationPreviewMarkdown } from '../screens/characters/generators/wizard/generation-preview-markdown';
import { Modal } from '../ui/modal';
import {
  isInHierarchy,
  pickToSelection,
  rowKey,
  scenarioMatchesSearch,
  SCOPE_SECTIONS,
  sourceKey,
  type ScenarioHierarchy,
  type ScenarioOwnerScope,
} from './scenario-browser.logic';
import { fetchScenariosEverywhere, injectScenarioRefBody, scenarioKeys } from './scenario.api';
import type { ScenarioSelection } from './scenario.types';

/** One listed result: a scenario on its owner's shelf. */
interface BrowserRow {
  source: EverywhereSource;
  scenario: EverywhereScenario;
  key: string;
}

/** One rendered nav section — a scope with at least one owner. */
interface OwnerSection {
  scope: ScenarioOwnerScope;
  label: string;
  owners: EverywhereSource[];
}

/**
 * The scenario browser — "Every scene on every shelf" (v4
 * `components/scenario/ScenarioBrowserDialog.tsx`, NEW at `545e299bb`).
 *
 * Opened from the scenario dropdown's "Browse all scenarios…" entry, on both
 * the New Chat form and the Salon's in-chat control. Lists every scenario in
 * the instance (`scenarioEverywhere`), grouped by owner — General, projects,
 * groups, characters — searchable across every shelf, with the highlighted
 * scene's full body previewed (`scenarioReadByRef`, one body at a time, cached
 * per ref).
 *
 * A pick that lies inside the chat's own hierarchy snaps to its native kind,
 * so the dropdown shows it under its usual heading; anything else becomes a
 * `store` selection labelled `Owner › Name`, which the dropdown shows under
 * "Elsewhere" and the server resolves by ref.
 *
 * Fetched on open with `staleTime: 0` (a reopened browser refetches — under the
 * app's global 5 s a quick reopen would otherwise show a stale shelf); nothing
 * polls. MOUNTING is v4's `isOpen`: both surfaces render it under `@if`.
 *
 * ## Three Angular-port notes (P4.D302 L-E / L-F)
 *
 * **The portal.** v4's `BaseModal` renders through `createPortal(…,
 * document.body)`. `qt-modal` renders in place, and in the Salon sidebar it
 * would sit inside the overlay sidebar's `z-index: 40` stacking context and its
 * `overflow-hidden`, so the host moves itself to the body after the first
 * render (the `scenario-builder-dialog.ts` precedent).
 *
 * **Escape.** v4's `BaseModal` closes on Escape by default; `qt-modal` handles
 * none. The browser listens on `window` in the CAPTURE phase and stops the
 * event there, so Escape closes the browser ONLY — the Salon sidebar's own
 * document-level Escape (which collapses an overlay sidebar) and anything else
 * beneath never see it. (P4.D285's unification caught the shape: a nested
 * Escape closing its parent.)
 *
 * **The preview.** v4 renders the body through `<ReactMarkdown>` with NO
 * `remarkPlugins`, which is exactly {@link renderGenerationPreviewMarkdown}'s
 * bare-CommonMark pipeline (no GFM; raw HTML dropped by `remark-rehype`, so
 * the HTML is trusted verbatim as that module's other caller trusts it).
 */
@Component({
  selector: 'qt-scenario-browser-dialog',
  changeDetection: ChangeDetectionStrategy.OnPush,
  imports: [Modal],
  template: `
    <qt-modal title="Every scene on every shelf" maxWidth="4xl" (close)="closed.emit()">
      <div class="flex flex-col gap-3" (keydown)="onKeyDown($event)">
        <div class="flex flex-col sm:flex-row sm:items-center gap-2">
          <input
            #searchInput
            type="search"
            [value]="search()"
            (input)="onSearch($event)"
            placeholder="Search every shelf…"
            aria-label="Search scenarios"
            class="qt-input flex-1"
          />
          <label class="flex items-center gap-2 qt-text-secondary text-sm">
            <input
              type="checkbox"
              class="qt-checkbox"
              [checked]="showArchived()"
              (change)="onShowArchived($event)"
            />
            Show archived
          </label>
        </div>

        @if (errorMessage(); as message) {
          <p class="qt-text-destructive text-sm">The shelves could not be reached: {{ message }}</p>
        } @else if (everywhere.isLoading()) {
          <p class="qt-text-secondary text-sm">Dusting the shelves…</p>
        } @else {
          <div class="flex flex-col sm:flex-row gap-3 min-h-0">
            <nav
              aria-label="Scenario owners"
              class="sm:w-56 shrink-0 max-h-48 sm:max-h-[55vh] overflow-y-auto"
            >
              <button
                type="button"
                [attr.class]="everyShelfClass()"
                (click)="selectedSourceKey.set(null)"
              >
                Every shelf
              </button>
              @for (section of sections(); track section.scope) {
                <div class="mt-2">
                  <button
                    type="button"
                    class="w-full text-left text-xs uppercase tracking-wide qt-text-secondary px-2 py-1"
                    [attr.aria-expanded]="!collapsed()[section.scope]"
                    (click)="toggleSection(section.scope)"
                  >
                    {{ (collapsed()[section.scope] ? '▸' : '▾') + ' ' + section.label }}
                  </button>
                  @if (!collapsed()[section.scope]) {
                    <ul>
                      @for (owner of section.owners; track ownerKey(owner)) {
                        <li>
                          <button
                            type="button"
                            [attr.class]="ownerClass(owner)"
                            [attr.title]="owner.scenarios.length === 0 ? 'Nothing filed here yet.' : null"
                            (click)="selectedSourceKey.set(ownerKey(owner))"
                          >
                            <span class="truncate flex-1">{{ owner.name }}</span>
                            @if (inHierarchy(owner)) {
                              <span class="qt-badge qt-badge-outline text-xs">in this chat</span>
                            }
                            <span class="qt-text-secondary text-xs">{{ owner.scenarios.length }}</span>
                          </button>
                        </li>
                      }
                    </ul>
                  }
                </div>
              }
            </nav>

            <div class="flex-1 min-w-0 flex flex-col gap-3">
              @if (results().length === 0) {
                <p class="qt-text-secondary text-sm px-2 py-4">
                  {{ searching() ? 'Nothing on that shelf answers to the name.' : 'Nothing filed here yet.' }}
                </p>
              } @else {
                <ul
                  #resultsList
                  role="listbox"
                  aria-label="Scenarios"
                  class="max-h-60 sm:max-h-[30vh] overflow-y-auto border qt-border-default rounded-lg"
                >
                  @for (row of results(); track row.key) {
                    <li
                      [attr.data-row]="row.key"
                      role="option"
                      [attr.aria-selected]="isHighlighted(row)"
                      [attr.class]="rowClass(row)"
                      (click)="highlightRow(row)"
                      (dblclick)="pickRow(row)"
                    >
                      <div class="flex items-center gap-2 flex-wrap">
                        <span class="font-medium text-sm">{{ row.scenario.name }}</span>
                        @if (row.scenario.isDefault) {
                          <span class="qt-badge qt-badge-secondary text-xs">default</span>
                        }
                        @if (row.scenario.archived) {
                          <span class="qt-badge qt-badge-warning text-xs">archived</span>
                        }
                        @if (showAllOwners()) {
                          <span class="qt-text-secondary text-xs ml-auto">{{ row.source.name }}</span>
                        }
                      </div>
                      @if (row.scenario.description) {
                        <div class="qt-text-secondary text-xs mt-0.5">{{ row.scenario.description }}</div>
                      }
                    </li>
                  }
                </ul>
              }

              @if (highlighted(); as h) {
                <div
                  class="border qt-border-default rounded-lg qt-bg-muted/30 p-3 max-h-60 sm:max-h-[25vh] overflow-y-auto"
                >
                  <div class="qt-text-secondary text-xs mb-2">
                    {{ h.source.name + ' › ' + h.scenario.name }}
                  </div>
                  @if (preview.isLoading()) {
                    <p class="qt-text-secondary text-sm">{{ h.scenario.excerpt }}</p>
                  } @else {
                    <div
                      class="prose prose-sm qt-prose-auto max-w-none"
                      data-testid="scenario-browser-preview"
                      [innerHTML]="previewHtml()"
                    ></div>
                  }
                </div>
              }
            </div>
          </div>
        }
      </div>
      <div qt-modal-footer class="flex gap-2 justify-end">
        <button type="button" class="qt-button-secondary" (click)="closed.emit()">Cancel</button>
        <button
          type="button"
          class="qt-button-primary"
          [disabled]="!highlighted()"
          (click)="handleUse()"
        >
          Use this scenario
        </button>
      </div>
    </qt-modal>
  `,
})
export class ScenarioBrowserDialog {
  private readonly core = inject(CoreClient);
  private readonly sanitizer = inject(DomSanitizer);

  /** The owners the chat's picker already offers without browsing. */
  readonly hierarchy = input.required<ScenarioHierarchy>();
  /** v4 `onClose` — Cancel, the ✕, the backdrop, Escape, and after a pick. */
  readonly closed = output<void>();
  /** v4 `onPick` — the selection the dropdown should now hold. */
  readonly pick = output<ScenarioSelection>();

  protected readonly search = signal('');
  protected readonly showArchived = signal(false);
  protected readonly selectedSourceKey = signal<string | null>(null);
  protected readonly collapsed = signal<Record<ScenarioOwnerScope, boolean>>({
    general: false,
    project: false,
    group: false,
    character: false,
  });
  protected readonly highlight = signal<{ mountPointId: string; path: string } | null>(null);

  private readonly searchInput = viewChild<ElementRef<HTMLInputElement>>('searchInput');
  private readonly resultsList = viewChild<ElementRef<HTMLUListElement>>('resultsList');

  protected readonly everywhere = injectQuery(() => ({
    queryKey: scenarioKeys.everywhere(this.showArchived()),
    queryFn: () => fetchScenariosEverywhere(this.core, this.showArchived()),
    // v4 `:174` — a reopened browser refetches (L-G: explicit against the app's 5 s).
    staleTime: 0,
  }));

  private readonly sources = computed<EverywhereSource[]>(
    () => this.everywhere.data()?.sources ?? [],
  );

  protected readonly errorMessage = computed<string | null>(() => {
    const error = this.everywhere.error();
    return error ? coreErrorMessage(error, String(error)) : null;
  });

  protected readonly searching = computed(() => this.search().trim().length > 0);

  protected readonly results = computed<BrowserRow[]>(() => {
    const searching = this.searching();
    const query = this.search();
    const selected = this.selectedSourceKey();
    const rows: BrowserRow[] = [];
    for (const source of this.sources()) {
      if (!searching && selected && sourceKey(source) !== selected) continue;
      for (const scenario of source.scenarios) {
        if (scenarioMatchesSearch(source, scenario, query)) {
          rows.push({ source, scenario, key: rowKey(source.mountPointId, scenario.path) });
        }
      }
    }
    return rows;
  });

  /**
   * Only a highlight that is actually listed counts; one filtered away by the
   * search reads as nothing highlighted (v4 `:192-196`).
   */
  protected readonly highlighted = computed<BrowserRow | null>(() => {
    const h = this.highlight();
    if (!h) return null;
    return (
      this.results().find(
        (r) => r.source.mountPointId === h.mountPointId && r.scenario.path === h.path,
      ) ?? null
    );
  });

  protected readonly preview = injectScenarioRefBody(
    () => this.highlight(),
    () => this.highlighted() !== null,
  );

  protected readonly previewHtml = computed<SafeHtml>(() => {
    const h = this.highlighted();
    const body = this.preview.data()?.scenario.body ?? h?.scenario.excerpt ?? '';
    return this.sanitizer.bypassSecurityTrustHtml(renderGenerationPreviewMarkdown(body));
  });

  protected readonly showAllOwners = computed(
    () => this.searching() || this.selectedSourceKey() === null,
  );

  protected readonly sections = computed<OwnerSection[]>(() =>
    SCOPE_SECTIONS.map(({ scope, label }) => ({
      scope,
      label,
      owners: this.sources().filter((s) => s.scope === scope),
    })).filter((section) => section.owners.length > 0),
  );

  protected readonly everyShelfClass = computed(
    () =>
      `w-full text-left text-sm px-2 py-1 rounded ${this.selectedSourceKey() === null ? 'qt-bg-surface-hover font-medium' : ''}`,
  );

  constructor() {
    // The portal (L-E) — after the first render, never in the constructor:
    // both surfaces mount this under `@if`, and an embedded view's insertion
    // would put a constructor-moved host straight back.
    const host = inject<ElementRef<HTMLElement>>(ElementRef).nativeElement;
    const destroyRef = inject(DestroyRef);
    destroyRef.onDestroy(() => host.remove());
    afterNextRender(() => {
      if (typeof document !== 'undefined') document.body.appendChild(host);
      // v4 `autoFocus` on the search box.
      this.searchInput()?.nativeElement.focus();
    });

    // Escape closes the BROWSER only (v4 BaseModal `closeOnEscape`, default
    // true). Window + capture runs before every document / element listener,
    // and stopping it there keeps a parent's Escape from firing too.
    if (typeof window !== 'undefined') {
      const onEscape = (event: KeyboardEvent) => {
        if (event.key !== 'Escape') return;
        event.stopPropagation();
        event.preventDefault();
        this.closed.emit();
      };
      window.addEventListener('keydown', onEscape, true);
      destroyRef.onDestroy(() => window.removeEventListener('keydown', onEscape, true));
    }
  }

  protected ownerKey(owner: EverywhereSource): string {
    return sourceKey(owner);
  }

  protected inHierarchy(owner: EverywhereSource): boolean {
    return isInHierarchy(owner, this.hierarchy());
  }

  protected ownerClass(owner: EverywhereSource): string {
    const key = sourceKey(owner);
    const empty = owner.scenarios.length === 0;
    return `w-full text-left text-sm px-2 py-1 rounded flex items-center gap-1 ${this.selectedSourceKey() === key ? 'qt-bg-surface-hover font-medium' : ''} ${empty ? 'qt-text-secondary opacity-60' : ''}`;
  }

  protected isHighlighted(row: BrowserRow): boolean {
    const h = this.highlighted();
    return (
      h !== null &&
      h.source.mountPointId === row.source.mountPointId &&
      h.scenario.path === row.scenario.path
    );
  }

  protected rowClass(row: BrowserRow): string {
    return `px-3 py-2 cursor-pointer border-b qt-border-default last:border-b-0 ${this.isHighlighted(row) ? 'qt-bg-surface-hover' : ''}`;
  }

  protected toggleSection(scope: ScenarioOwnerScope): void {
    this.collapsed.update((c) => ({ ...c, [scope]: !c[scope] }));
  }

  protected onSearch(event: Event): void {
    this.search.set((event.target as HTMLInputElement).value);
  }

  protected onShowArchived(event: Event): void {
    this.showArchived.set((event.target as HTMLInputElement).checked);
  }

  protected highlightRow(row: BrowserRow): void {
    this.highlight.set({ mountPointId: row.source.mountPointId, path: row.scenario.path });
  }

  /** v4 `onDoubleClick` — pick straight from the row. */
  protected pickRow(row: BrowserRow): void {
    this.pick.emit(pickToSelection(row.source, row.scenario, this.hierarchy()));
    this.closed.emit();
  }

  /** v4 `handleUse`. */
  protected handleUse(): void {
    const h = this.highlighted();
    if (!h) return;
    this.pick.emit(pickToSelection(h.source, h.scenario, this.hierarchy()));
    this.closed.emit();
  }

  /**
   * v4 `moveHighlight` — clamped, scrolled into view. The listed highlight's
   * index is read BEFORE the new highlight is set (P4.D302 L-H(2), memory
   * `react-closure-reads-in-signal-ports`): v4's closure saw the render's
   * `highlighted`, and a signal read after `.set` would see the new one.
   */
  private moveHighlight(delta: number): void {
    const results = this.results();
    if (results.length === 0) return;
    const current = this.highlighted();
    const index = current ? results.indexOf(current) : -1;
    const next = results[Math.min(results.length - 1, Math.max(0, index + delta))];
    this.highlight.set({ mountPointId: next.source.mountPointId, path: next.scenario.path });
    // v4 finds the row by `[data-row="${CSS.escape(key)}"]`; matching the
    // dataset directly is the same lookup without the escaping.
    const list = this.resultsList()?.nativeElement;
    const el = list
      ? [...list.querySelectorAll<HTMLElement>('[data-row]')].find(
          (node) => node.dataset['row'] === next.key,
        )
      : undefined;
    if (el && typeof el.scrollIntoView === 'function') el.scrollIntoView({ block: 'nearest' });
  }

  protected onKeyDown(event: KeyboardEvent): void {
    if (event.key === 'ArrowDown') {
      event.preventDefault();
      this.moveHighlight(1);
    } else if (event.key === 'ArrowUp') {
      event.preventDefault();
      this.moveHighlight(-1);
    } else if (event.key === 'Enter' && this.highlighted()) {
      event.preventDefault();
      this.handleUse();
    }
  }
}
