/**
 * The Guide tab (port of v4 `components/help-chat/HelpGuideTab.tsx`).
 *
 * Three layers: category list → expanded topics → document reader.
 *
 * The search is v4's two-speed design and the reason it is not simpler. The
 * document index carries no prose (far too large to ship), so a text match has
 * to happen server-side; the TITLE filter therefore stays client-side and
 * narrows the list on the first keystroke rather than waiting on a round trip.
 * The server hits are TAGGED with the query that produced them, so a slow
 * response for `"mem"` can never leak into the results for `"memory"` — v4
 * tags rather than clearing on every keystroke precisely to avoid that, and to
 * keep the effect free of a synchronous state write.
 *
 * @module help/help-guide-tab
 */

import {
  ChangeDetectionStrategy,
  Component,
  computed,
  effect,
  inject,
  signal,
} from '@angular/core';

import { EXCLUDED_DOCUMENTS, HELP_CATEGORIES, getCategoryForUrl } from './help-categories';
import { HelpCategorySection, type HelpDocumentInfo } from './help-category-section';
import { HelpGuideSearch } from './help-guide-search';
import { HelpNavigate } from './help-navigate';
import { HelpTopicReader } from './help-topic-reader';
import { HelpWelcomeCard } from './help-welcome-card';
import { HelpApi, type HelpDocIndexRow } from './help-wire';
import { HelpService } from './help.service';

/** Keystroke settling time before the text search hits the server. */
export const SEARCH_DEBOUNCE_MS = 200;

/**
 * v4's index loop, lifted into a named function so the exclusion can be
 * MEASURED.
 *
 * The body is v4's verbatim; only its home moved. The reason: no slug in
 * `EXCLUDED_DOCUMENTS` appears in any `HELP_CATEGORIES` list, so an excluded
 * document has no category to render under either way — a mutation deleting the
 * filter entirely survives every rendering test (measured). The exclusion is
 * still real, belt-and-braces behaviour that a future category row could depend
 * on, so it gets a discriminating test here rather than shipping as an arm
 * nothing can see.
 */
export function buildDocumentMap(docs: readonly HelpDocIndexRow[]): Map<string, HelpDocumentInfo> {
  const docMap = new Map<string, HelpDocumentInfo>();
  for (const doc of docs) {
    if (!EXCLUDED_DOCUMENTS.includes(doc.slug)) {
      docMap.set(doc.slug, { id: doc.slug, title: doc.title, url: doc.url });
    }
  }
  return docMap;
}

/** One step of the reader's back stack (v4 `NavHistoryEntry`). */
interface NavHistoryEntry {
  docId: string;
  categoryLabel: string;
  scrollTop: number;
}

@Component({
  selector: 'qt-help-guide-tab',
  changeDetection: ChangeDetectionStrategy.OnPush,
  imports: [HelpGuideSearch, HelpWelcomeCard, HelpCategorySection, HelpTopicReader],
  host: { class: 'contents' },
  template: `
    <!-- Both views wrapped in Art Deco scoping class -->
    <div class="qt-help-guide-deco flex flex-col h-full">
      @if (activeDocId(); as docId) {
        <qt-help-topic-reader
          [documentId]="docId"
          [categoryLabel]="activeCategoryLabel()"
          [restoreScrollTop]="restoreScrollTop()"
          (back)="handleBack()"
          (navigateDoc)="handleNavigateDoc($event)"
          (navigatePage)="handleNavigatePage($event)"
          (scrolled)="lastScrollTop = $event"
        />
      } @else {
        <div class="flex-shrink-0 p-3 pb-0">
          <qt-help-guide-search
            [value]="searchQuery()"
            (valueChange)="searchQuery.set($event)"
          />
        </div>

        <div class="flex-1 overflow-y-auto p-3">
          @if (loading()) {
            <div class="flex items-center justify-center py-8" style="color: var(--deco-fg-muted)">
              Loading guide...
            </div>
          } @else {
            @if (showWelcome()) {
              <qt-help-welcome-card (openDocument)="handleOpenWelcomeDoc($event)" />
            }

            @for (cat of filteredCategories(); track cat.id) {
              <qt-help-category-section
                [label]="cat.label"
                [documents]="cat.resolvedDocs"
                [currentPageUrl]="currentPageUrl()"
                [defaultExpanded]="cat.id === contextCategoryId() && !searchQuery()"
                [forceExpanded]="!!searchQuery()"
                [snippets]="currentTextHits()"
                (selectTopic)="handleSelectTopic($event, cat.label)"
              />
            }

            @if (filteredCategories().length === 0 && searchQuery()) {
              <div class="text-center py-6" style="color: var(--deco-fg-muted)">
                No topics match &ldquo;{{ searchQuery() }}&rdquo;
              </div>
            }
          }
        </div>
      }
    </div>
  `,
})
export class HelpGuideTab {
  private readonly api = inject(HelpApi);
  private readonly help = inject(HelpService);
  private readonly navigate = inject(HelpNavigate);

  protected readonly currentPageUrl = this.help.currentPageUrl;

  /** The document index, keyed by SLUG (v4 keys categories and links by slug). */
  private readonly documents = signal<Map<string, HelpDocumentInfo>>(new Map());
  private readonly chatCount = signal<number | null>(null);
  protected readonly loading = signal(true);

  // --- navigation state ---
  protected readonly activeDocId = signal<string | null>(null);
  protected readonly activeCategoryLabel = signal('');
  protected readonly searchQuery = signal('');
  protected readonly restoreScrollTop = signal<number | undefined>(undefined);

  /**
   * Text-search hits, tagged with the query that produced them. Tagging (rather
   * than clearing on every keystroke) keeps stale results from a previous query
   * from leaking into the current filter.
   */
  private readonly contentMatches = signal<{
    query: string;
    bySlug: Map<string, string | null>;
  } | null>(null);

  /** The reader's back stack, and its live scroll position. */
  private navHistory: NavHistoryEntry[] = [];
  protected lastScrollTop = 0;

  /** Which category to auto-expand for the page the operator is on. */
  protected readonly contextCategoryId = computed(() => getCategoryForUrl(this.currentPageUrl()));

  /** v4: the welcome card shows only below three chats, and never while searching. */
  protected readonly showWelcome = computed(() => {
    const count = this.chatCount();
    return count !== null && count < 3 && !this.searchQuery();
  });

  /** Categories with their documents resolved; a slug with no document drops out. */
  private readonly categories = computed(() => {
    const docs = this.documents();
    return HELP_CATEGORIES.map((cat) => ({
      ...cat,
      resolvedDocs: cat.documents
        .map((docId) => docs.get(docId))
        .filter((d): d is HelpDocumentInfo => d !== undefined),
    }));
  });

  /** Text hits for the query currently in the box, or null if they're stale. */
  protected readonly currentTextHits = computed(() => {
    const matches = this.contentMatches();
    return matches?.query === this.searchQuery().trim() ? matches.bySlug : null;
  });

  /** Title match, plus any document whose text the server matched. */
  protected readonly filteredCategories = computed(() => {
    if (!this.searchQuery().trim()) return this.categories();
    const query = this.searchQuery().toLowerCase();
    const hits = this.currentTextHits();
    return this.categories()
      .map((cat) => ({
        ...cat,
        resolvedDocs: cat.resolvedDocs.filter(
          (doc) => doc.title.toLowerCase().includes(query) || hits?.has(doc.id),
        ),
      }))
      .filter((cat) => cat.resolvedDocs.length > 0);
  });

  constructor() {
    void this.fetchIndex();

    // The debounced server-side text search (v4's effect on `searchQuery`).
    effect((onCleanup) => {
      const query = this.searchQuery().trim();
      if (query.length < 2) return;

      let cancelled = false;
      const timer = setTimeout(() => {
        void (async () => {
          try {
            const matches = await this.api.docsSearch(query);
            if (cancelled) return;
            const bySlug = new Map<string, string | null>();
            for (const match of matches) bySlug.set(match.slug, match.snippet ?? null);
            this.contentMatches.set({ query, bySlug });
          } catch {
            // Leave the title-only filter in place; the box still works.
          }
        })();
      }, SEARCH_DEBOUNCE_MS);

      onCleanup(() => {
        cancelled = true;
        clearTimeout(timer);
      });
    });
  }

  /** Fetch the document index and the chat count together (v4's mount effect). */
  private async fetchIndex(): Promise<void> {
    try {
      const [docs, count] = await Promise.all([this.api.docsList(), this.api.docsChatCount()]);
      this.documents.set(buildDocumentMap(docs));
      this.chatCount.set(count);
    } catch {
      /* v4 swallows: the Guide renders empty rather than erroring. */
    } finally {
      this.loading.set(false);
    }
  }

  // --- the three-layer navigation (v4's handlers) ----------------------------

  /** Opening a topic from the LIST clears the back stack (v4). */
  protected handleSelectTopic(docId: string, categoryLabel: string): void {
    this.navHistory = [];
    this.restoreScrollTop.set(undefined);
    this.activeDocId.set(docId);
    this.activeCategoryLabel.set(categoryLabel);
  }

  /** Back pops the stack — or leaves the reader entirely when it is empty. */
  protected handleBack(): void {
    const entry = this.navHistory.pop();
    if (entry) {
      // Go back to previous document and restore its scroll position
      this.restoreScrollTop.set(entry.scrollTop);
      this.activeDocId.set(entry.docId);
      this.activeCategoryLabel.set(entry.categoryLabel);
    } else {
      // No history — go back to category list
      this.restoreScrollTop.set(undefined);
      this.activeDocId.set(null);
      this.activeCategoryLabel.set('');
    }
  }

  /** A sibling `*.md` link: push the current topic, then open the new one. */
  protected handleNavigateDoc(docId: string): void {
    const current = this.activeDocId();
    if (current) {
      this.navHistory.push({
        docId: current,
        categoryLabel: this.activeCategoryLabel(),
        scrollTop: this.lastScrollTop,
      });
    }
    const cat = HELP_CATEGORIES.find((c) => c.documents.includes(docId));
    this.restoreScrollTop.set(undefined);
    this.activeDocId.set(docId);
    this.activeCategoryLabel.set(cat?.label || '');
  }

  /** An app route: leave the Guide (keep-alive-safe inside the workspace). */
  protected handleNavigatePage(url: string): void {
    this.navigate.go(url);
  }

  /** The welcome card's four links — v4 does NOT clear the back stack here. */
  protected handleOpenWelcomeDoc(docId: string): void {
    const cat = HELP_CATEGORIES.find((c) => c.documents.includes(docId));
    this.activeDocId.set(docId);
    this.activeCategoryLabel.set(cat?.label || '');
  }
}
