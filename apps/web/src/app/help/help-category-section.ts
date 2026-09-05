/**
 * One collapsible category in the Guide (port of v4
 * `components/help-chat/HelpCategorySection.tsx`).
 *
 * v4 keeps the open state in a ref behind `useSyncExternalStore` purely to dodge
 * a setState-in-effect lint; a signal is the same thing without the scaffolding,
 * so that machinery is deliberately NOT transcribed — only its behaviour is:
 * `defaultExpanded` seeds it, `forceExpanded` (a live search) opens it and
 * scrolls it into view ONCE per search, and dropping back out re-arms that.
 *
 * The `snippets` line is the P4.D77 bank's half (b): for a topic the server
 * matched on its PROSE rather than its title, the excerpt renders under it —
 * without it the topic appears in the results with nothing explaining why.
 *
 * @module help/help-category-section
 */

import {
  ChangeDetectionStrategy,
  Component,
  ElementRef,
  computed,
  effect,
  input,
  signal,
  output,
  viewChild,
} from '@angular/core';

import { Icon } from '../ui/icon';

/** A document as the Guide lists it — `id` is the SLUG, not the database id. */
export interface HelpDocumentInfo {
  id: string;
  title: string;
  url: string;
}

@Component({
  selector: 'qt-help-category-section',
  changeDetection: ChangeDetectionStrategy.OnPush,
  imports: [Icon],
  host: { class: 'contents' },
  template: `
    <div class="qt-help-guide-category" #section>
      <button
        type="button"
        class="qt-help-guide-category-header"
        [attr.aria-expanded]="isExpanded()"
        (click)="toggle()"
      >
        <qt-icon
          name="chevron-right"
          class="qt-help-guide-category-chevron"
          [class.qt-help-guide-category-chevron-open]="isExpanded()"
        />
        <span class="qt-help-guide-category-label">{{ label() }}</span>
        <span class="qt-help-guide-category-badge">({{ documents().length }})</span>
      </button>
      @if (isExpanded()) {
        <div class="qt-help-guide-category-topics">
          @for (doc of sortedDocs(); track doc.id) {
            <button
              type="button"
              class="qt-help-guide-topic"
              [class.qt-help-guide-topic-active]="isActive(doc)"
              [class.whitespace-normal]="!!snippetFor(doc)"
              (click)="selectTopic.emit(doc.id)"
            >
              {{ doc.title }}
              @if (snippetFor(doc); as snippet) {
                <span class="block text-xs mt-0.5 truncate" style="color: var(--deco-fg-muted)">{{
                  snippet
                }}</span>
              }
            </button>
          }
        </div>
      }
    </div>
  `,
})
export class HelpCategorySection {
  readonly label = input.required<string>();
  readonly documents = input.required<HelpDocumentInfo[]>();
  readonly currentPageUrl = input.required<string>();
  readonly defaultExpanded = input(false);
  readonly forceExpanded = input(false);
  /**
   * slug → matching excerpt, for documents the current search matched on their
   * text rather than their title. Without it a topic appears in the results
   * with nothing on screen explaining why.
   */
  readonly snippets = input<Map<string, string | null> | null>(null);
  readonly selectTopic = output<string>();

  /**
   * Ref-based open state in v4 (`useSyncExternalStore` behind a ref, purely to
   * dodge a setState-in-effect lint); a signal is the same thing without the
   * scaffolding, so that machinery is not transcribed — only its behaviour is.
   */
  private readonly open = signal(false);
  private seeded = false;
  private hasScrolled = false;
  private readonly sectionEl = viewChild.required<ElementRef<HTMLDivElement>>('section');

  protected readonly isExpanded = computed(() => this.open());

  /**
   * Sorted with the page the operator is ON first, then the curated order.
   *
   * v4's comparator maps each document to -1 or 0 and subtracts, which is a
   * PARTIAL order — `Array.prototype.sort` is stable, so equal keys keep the
   * curated order. Transcribed as-is rather than "improved" into a total order,
   * because the curated order is the fallback and stability is what preserves it.
   */
  protected readonly sortedDocs = computed(() => {
    const url = this.currentPageUrl();
    return [...this.documents()].sort((a, b) => {
      const aMatch = url === a.url || url.startsWith(a.url) ? -1 : 0;
      const bMatch = url === b.url || url.startsWith(b.url) ? -1 : 0;
      return aMatch - bMatch;
    });
  });

  constructor() {
    // v4's single `useEffect` on `[forceExpanded]`: seed from `defaultExpanded`
    // on the first pass, then force open (and scroll into view ONCE) whenever a
    // search is running, re-arming the scroll when the box is cleared. v4's
    // `setOpen(true)` sticks, so a category a search opened stays open after —
    // reproduced here by only ever setting `open` true in this branch.
    effect(() => {
      const force = this.forceExpanded();
      if (!this.seeded) {
        this.seeded = true;
        this.open.set(this.defaultExpanded() || force);
      }
      if (force) {
        this.open.set(true);
        if (!this.hasScrolled) {
          this.hasScrolled = true;
          requestAnimationFrame(() => {
            this.sectionEl().nativeElement.scrollIntoView({ behavior: 'smooth', block: 'nearest' });
          });
        }
      } else {
        this.hasScrolled = false;
      }
    });
  }

  protected toggle(): void {
    this.open.update((v) => !v);
  }

  /** v4's active test appends a trailing slash — `/files` never lights `/f`. */
  protected isActive(doc: HelpDocumentInfo): boolean {
    const url = this.currentPageUrl();
    return url === doc.url || url.startsWith(doc.url + '/');
  }

  protected snippetFor(doc: HelpDocumentInfo): string | null | undefined {
    return this.snippets()?.get(doc.id);
  }
}
