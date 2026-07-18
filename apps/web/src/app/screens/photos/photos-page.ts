import {
  ChangeDetectionStrategy,
  Component,
  ElementRef,
  OnDestroy,
  computed,
  effect,
  inject,
  signal,
  viewChild,
} from '@angular/core';
import { FormsModule } from '@angular/forms';
import { RouterLink } from '@angular/router';

import { CoreClient } from '../../core/core-client';
import {
  GalleryEntry,
  PAGE_SIZE,
  cardCaption,
  cardLinkerLabel,
  detailCaption,
  entryImageUrl,
  fetchGallery,
  linkerBadge,
  removeGalleryEntry,
} from './photos.api';

/**
 * The "My Photos" screen at `/photos` (v4 `app/photos/PhotosView.tsx`) — a
 * deduped roll-up of every `photos/` folder across the instance. The same image
 * hard-linked into several albums collapses to ONE card carrying a "linked in N
 * places" badge, which the detail modal expands into the full linker list.
 *
 * ## The two staleness guards, both ported deliberately
 *
 * v4 keeps a `fetchGenerationRef` (`:77`, `:83`, `:97`) that every new search
 * bumps: an in-flight load-more from the PREVIOUS query resolves after the new
 * first page and would otherwise append the old query's rows onto the new
 * results. And it keeps a defensive linkId de-dupe on append (`:125-131`),
 * because a save landing mid-scroll shifts the underlying ordering and can
 * hand the same row to two pages. Both are carried here — they are not
 * belt-and-braces, they are the two ways this screen can visibly corrupt
 * itself.
 *
 * ## Divergences, all recorded
 *
 * - **No subsystem background.** v4 wraps the page in
 *   `useSubsystemBackgroundStyle('lantern')`. v5 has no subsystem-background
 *   machinery at all (verified by grep), and inventing one is not this lane's
 *   business — deferred loudly in the lane's status-log record.
 * - **Full-size images in the grid.** v4's cards render the full `blobUrl` with
 *   `loading="lazy"` and no thumbnail route. That is a v4 performance quirk,
 *   carried deliberately rather than "fixed", because a thumbnail route does not
 *   exist on this surface and minting one would be a v5-only behavior.
 * - **`window.confirm` for the delete gate.** v4 uses its promise-based
 *   `showConfirmation` modal; v5 has no such component and `window.confirm` is
 *   the established idiom for it here (13 call sites).
 * - **Tags are READ-ONLY.** Not an omission: v4 has no tag editing anywhere in
 *   this family.
 */
@Component({
  selector: 'qt-photos-page',
  changeDetection: ChangeDetectionStrategy.OnPush,
  imports: [FormsModule, RouterLink],
  // v4's detail modal binds Escape on `window` (`:394-400`). The listener is
  // declared on the page rather than the modal because the modal is inline
  // control flow here, not a child component with its own lifecycle.
  host: { '(document:keydown.escape)': 'onEscape()' },
  template: `
    <div class="qt-page-container text-foreground">
      <header
        class="flex flex-wrap items-end justify-between gap-6 border-b qt-border-default/60 pb-6"
      >
        <div class="max-w-2xl space-y-2">
          <h1 class="qt-page-title">My Photos</h1>
          <p class="qt-text-small">
            Every image you&rsquo;ve kept, gathered from each character&rsquo;s vault, each project
            album, and your private uploads &mdash; listed once and linked to wherever it lives.
          </p>
          <p class="qt-text-muted text-xs uppercase tracking-wide">{{ counterLabel() }}</p>
        </div>
        <a routerLink="/salon" class="qt-link text-sm whitespace-nowrap">&larr; Back to the Salon</a>
      </header>

      <form class="mt-8 flex flex-wrap gap-3" (submit)="onSearch($event)">
        <div class="relative flex-1 min-w-[16rem]">
          <span
            class="pointer-events-none absolute inset-y-0 left-3 flex items-center qt-text-muted"
            aria-hidden="true"
            >&#128269;</span
          >
          <input
            type="search"
            class="qt-input pl-9"
            aria-label="Search your photos"
            placeholder="Search by prompt, caption, scene, or tag&hellip;"
            [(ngModel)]="queryText"
            name="q"
          />
        </div>
        <button type="submit" class="qt-button-primary">Search</button>
        @if (appliedQuery()) {
          <button type="button" class="qt-button-secondary" (click)="clearSearch()">Clear</button>
        }
      </form>

      @if (error(); as message) {
        <div class="qt-alert-error mt-6" role="alert">{{ message }}</div>
      }

      @if (loading() && entries().length === 0) {
        <div class="mt-12 flex items-center justify-center">
          <div class="animate-spin rounded-full h-8 w-8 border-b-2 qt-border-primary"></div>
        </div>
      }

      @if (isEmpty()) {
        <div
          class="mt-12 rounded-2xl border border-dashed qt-border-default/70 qt-bg-card/80 px-8 py-16 text-center qt-shadow-sm"
        >
          @if (appliedQuery(); as applied) {
            <h2 class="qt-heading-3 mb-3">Nothing matches that search</h2>
            <p class="qt-text-small max-w-md mx-auto">
              No photo&rsquo;s prompt, caption, scene, or tags pair convincingly with
              &ldquo;{{ applied }}&rdquo;. Try a different phrasing or clear the search to browse
              everything.
            </p>
            <button type="button" class="qt-link mt-6 inline-block text-sm" (click)="clearSearch()">
              Clear search &#8635;
            </button>
          } @else {
            <h2 class="qt-heading-3 mb-3">Nothing pinned to the wall yet</h2>
            <p class="qt-text-small max-w-md mx-auto">
              Save an image from any chat &mdash; the &ldquo;Save image&rdquo; button on a message
              tucks it into the album of your choice, and it&rsquo;ll appear here the moment
              it&rsquo;s filed.
            </p>
            <a routerLink="/salon" class="qt-link mt-6 inline-block text-sm"
              >Take me to the Salon &rarr;</a
            >
          }
        </div>
      }

      @if (entries().length > 0) {
        <ul class="mt-8 grid grid-cols-2 gap-5 sm:grid-cols-3 md:grid-cols-4 xl:grid-cols-5">
          @for (entry of entries(); track entry.linkId) {
            <li>
              <button
                type="button"
                class="group relative flex w-full flex-col overflow-hidden rounded-2xl border qt-border-default/60 qt-bg-card/90 text-left qt-shadow-sm transition-all hover:-translate-y-0.5 hover:qt-border-primary/60 hover:qt-shadow-md focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring"
                [attr.aria-label]="caption(entry)"
                (click)="selected.set(entry)"
              >
                <div class="relative aspect-square w-full overflow-hidden qt-bg-muted/40">
                  <img
                    [src]="imageUrl(entry)"
                    [alt]="caption(entry)"
                    loading="lazy"
                    class="h-full w-full object-cover transition-transform duration-300 group-hover:scale-[1.03]"
                  />
                  @if (entry.linkSummary.count > 1) {
                    <span
                      class="absolute right-2 top-2 inline-flex items-center gap-1 rounded-full bg-background/85 px-2.5 py-1 text-xs font-medium qt-text-primary backdrop-blur-sm qt-shadow-sm"
                      [title]="'Hard-linked in ' + entry.linkSummary.count + ' places'"
                    >
                      <span aria-hidden="true">&#128279;</span>{{ entry.linkSummary.count }}
                    </span>
                  }
                </div>
                <div class="flex flex-1 flex-col gap-1.5 px-4 py-3">
                  <p
                    class="line-clamp-2 text-sm font-medium text-foreground"
                    [title]="caption(entry)"
                  >
                    {{ caption(entry) }}
                  </p>
                  <p class="qt-text-muted truncate text-xs" [title]="linkerLabel(entry)">
                    {{ linkerLabel(entry) }}
                  </p>
                </div>
              </button>
            </li>
          }
        </ul>

        <div #sentinel aria-hidden="true" class="h-1"></div>

        @if (loadingMore()) {
          <div class="mt-8 flex items-center justify-center gap-3 qt-text-muted text-sm">
            <span
              class="inline-block h-4 w-4 animate-spin rounded-full border-b-2 qt-border-primary"
            ></span>
            Loading more photos&hellip;
          </div>
        }

        @if (!hasMore() && !loadingMore() && entries().length >= PAGE_SIZE) {
          <p class="mt-10 text-center qt-text-muted text-xs uppercase tracking-wider">
            {{ endOfGalleryLabel() }}
          </p>
        }
      }

      @if (selected(); as entry) {
        <div
          class="fixed inset-0 z-50 flex items-center justify-center bg-background/80 p-4 backdrop-blur-sm"
          role="presentation"
          (click)="selected.set(null)"
        >
          <div
            class="relative max-h-[90vh] w-full max-w-5xl overflow-hidden rounded-2xl border qt-border-default qt-bg-card qt-shadow-lg"
            role="dialog"
            aria-modal="true"
            aria-labelledby="photo-detail-title"
            (click)="$event.stopPropagation()"
          >
            <header
              class="flex items-start justify-between gap-4 border-b qt-border-default/60 px-6 py-4"
            >
              <div class="min-w-0">
                <h2 id="photo-detail-title" class="qt-heading-3 truncate">
                  {{ modalCaption(entry) }}
                </h2>
                <p class="qt-text-muted mt-1 text-xs">Saved {{ savedAt(entry) }}</p>
              </div>
              <button
                type="button"
                aria-label="Close"
                class="qt-button-secondary qt-text-muted px-3 py-1 text-lg leading-none"
                (click)="selected.set(null)"
              >
                &times;
              </button>
            </header>
            <div
              class="grid max-h-[calc(90vh-9rem)] gap-6 overflow-y-auto px-6 py-5 md:grid-cols-[minmax(0,1fr)_22rem]"
            >
              <div class="flex items-start justify-center">
                <img
                  [src]="imageUrl(entry)"
                  [alt]="modalCaption(entry)"
                  class="max-h-[70vh] w-full rounded-xl object-contain qt-shadow-sm"
                />
              </div>
              <div class="space-y-5">
                @if (entry.generationPromptExcerpt) {
                  <section>
                    <h3 class="qt-text-muted mb-2 text-xs font-semibold uppercase tracking-wide">
                      Original prompt
                    </h3>
                    <p class="qt-text-small leading-relaxed">{{ entry.generationPromptExcerpt }}</p>
                  </section>
                }
                @if (entry.tags.length > 0) {
                  <section>
                    <h3 class="qt-text-muted mb-2 text-xs font-semibold uppercase tracking-wide">
                      Tags
                    </h3>
                    <div class="flex flex-wrap gap-1.5">
                      @for (tag of entry.tags; track tag) {
                        <span class="qt-badge qt-badge-muted">{{ tag }}</span>
                      }
                    </div>
                  </section>
                }
                <section>
                  <h3 class="qt-text-muted mb-2 text-xs font-semibold uppercase tracking-wide">
                    {{ linkedInLabel(entry) }}
                  </h3>
                  <ul class="space-y-2">
                    @for (linker of entry.linkSummary.linkers; track linker.linkId) {
                      <li
                        class="rounded-lg border qt-border-default/50 qt-bg-muted/40 px-3 py-2 text-sm"
                      >
                        <div class="flex items-center justify-between gap-3">
                          <span class="truncate font-medium text-foreground">{{
                            linker.linkedBy ?? linker.mountPointName
                          }}</span>
                          <span
                            class="qt-badge qt-badge-muted whitespace-nowrap text-[10px] uppercase"
                            >{{ badge(linker) }}</span
                          >
                        </div>
                        <p class="qt-text-muted mt-1 break-all font-mono text-xs">
                          {{ linker.relativePath }}
                        </p>
                      </li>
                    }
                  </ul>
                </section>
                <section>
                  <h3 class="qt-text-muted mb-2 text-xs font-semibold uppercase tracking-wide">
                    Identity
                  </h3>
                  <p class="qt-text-muted break-all font-mono text-[11px]">
                    sha256: {{ entry.sha256 }}
                  </p>
                  <p class="qt-text-muted break-all font-mono text-[11px]">
                    linkId: {{ entry.linkId }}
                  </p>
                </section>
              </div>
            </div>
            <footer
              class="flex flex-wrap items-center justify-between gap-3 border-t qt-border-default/60 px-6 py-4"
            >
              <button
                type="button"
                class="qt-button qt-button-destructive"
                [disabled]="deleting() === entry.linkId"
                (click)="onDelete(entry)"
              >
                {{ deleting() === entry.linkId ? 'Removing…' : 'Remove from this album' }}
              </button>
              <button type="button" class="qt-button-secondary" (click)="selected.set(null)">
                Close
              </button>
            </footer>
          </div>
        </div>
      }
    </div>
  `,
})
export class PhotosPage implements OnDestroy {
  private readonly core = inject(CoreClient);

  protected readonly PAGE_SIZE = PAGE_SIZE;
  protected readonly entries = signal<GalleryEntry[]>([]);
  protected readonly total = signal(0);
  protected readonly hasMore = signal(false);
  protected readonly loading = signal(true);
  protected readonly loadingMore = signal(false);
  protected readonly error = signal<string | null>(null);
  protected queryText = '';
  protected readonly appliedQuery = signal('');
  protected readonly selected = signal<GalleryEntry | null>(null);
  protected readonly deleting = signal<string | null>(null);

  /**
   * v4's `fetchGenerationRef` (`:77`). Every new search bumps it; a resolved
   * fetch whose generation no longer matches is DISCARDED wholesale — result,
   * error, and the loading flag alike.
   */
  private generation = 0;
  private observer: IntersectionObserver | null = null;
  private readonly sentinel = viewChild<ElementRef<HTMLElement>>('sentinel');

  constructor() {
    void this.loadInitial();
    // The sentinel only exists once there are entries to scroll past, so the
    // observer is (re)attached whenever the element appears or is replaced.
    effect(() => {
      const el = this.sentinel()?.nativeElement;
      this.observer?.disconnect();
      this.observer = null;
      if (!el) {
        return;
      }
      // v4's `rootMargin: '600px 0px'` (`:153`) — the next page is fetched
      // before the reader reaches the bottom, so scrolling stays continuous.
      this.observer = new IntersectionObserver(
        (records) => {
          if (records.some((r) => r.isIntersecting)) {
            void this.loadMore();
          }
        },
        { rootMargin: '600px 0px' },
      );
      this.observer.observe(el);
    });
  }

  ngOnDestroy(): void {
    this.observer?.disconnect();
  }

  protected caption = cardCaption;
  protected linkerLabel = cardLinkerLabel;
  protected modalCaption = detailCaption;
  protected badge = linkerBadge;
  protected imageUrl = entryImageUrl;

  protected readonly isEmpty = computed(
    () => this.entries().length === 0 && !this.loading() && !this.error(),
  );

  /** v4 `counterLabel` (`:188-195`) — five arms, in v4's order. */
  protected readonly counterLabel = computed(() => {
    if (this.loading() && this.entries().length === 0) {
      return 'Loading…';
    }
    const applied = this.appliedQuery();
    const total = this.total();
    if (applied && total > 0) {
      return `${total} ${total === 1 ? 'match' : 'matches'} for “${applied}”`;
    }
    if (applied) {
      return `No matches for “${applied}”`;
    }
    if (total === 0) {
      return 'No photos yet';
    }
    return `${total} ${total === 1 ? 'photo' : 'photos'}`;
  });

  /** v4 `:311-313`. */
  protected readonly endOfGalleryLabel = computed(() => {
    const shown = this.entries().length;
    const total = this.total();
    return shown === total
      ? `That’s all ${total} of them.`
      : `End of the gallery — ${shown} of ${total} shown.`;
  });

  /** v4 `:461` — the section heading pluralizes on the link count. */
  protected linkedInLabel(entry: GalleryEntry): string {
    const count = entry.linkSummary.count;
    return `Linked in ${count} ${count === 1 ? 'place' : 'places'}`;
  }

  /** v4 `:421` — `new Date(keptAt).toLocaleString()`. */
  protected savedAt(entry: GalleryEntry): string {
    return new Date(entry.keptAt).toLocaleString();
  }

  protected onSearch(event: Event): void {
    event.preventDefault();
    this.appliedQuery.set(this.queryText);
    void this.loadInitial();
  }

  protected clearSearch(): void {
    this.queryText = '';
    this.appliedQuery.set('');
    void this.loadInitial();
  }

  /** v4's `fetchInitial` effect (`:79-105`). */
  private async loadInitial(): Promise<void> {
    const generation = ++this.generation;
    this.loading.set(true);
    this.error.set(null);
    try {
      const result = await fetchGallery(this.core, {
        query: this.appliedQuery(),
        offset: 0,
      });
      if (generation !== this.generation) {
        return;
      }
      this.entries.set(result.entries ?? []);
      this.total.set(result.total ?? result.entries?.length ?? 0);
      this.hasMore.set(Boolean(result.hasMore));
    } catch (err) {
      if (generation !== this.generation) {
        return;
      }
      this.error.set(messageOf(err, 'Failed to load gallery'));
    } finally {
      if (generation === this.generation) {
        this.loading.set(false);
      }
    }
  }

  /** v4's `loadMore` (`:107-139`). */
  private async loadMore(): Promise<void> {
    if (this.loadingMore() || this.loading() || !this.hasMore()) {
      return;
    }
    const generation = this.generation;
    this.loadingMore.set(true);
    try {
      const result = await fetchGallery(this.core, {
        query: this.appliedQuery(),
        offset: this.entries().length,
      });
      // A load-more that outlived its query must NOT append onto the new one.
      if (generation !== this.generation) {
        return;
      }
      const incoming = result.entries ?? [];
      this.entries.update((prev) => {
        // v4's defensive de-dupe (`:125-131`): a save landing mid-scroll shifts
        // the ordering, so the same linkId can arrive on two pages.
        const seen = new Set(prev.map((e) => e.linkId));
        return [...prev, ...incoming.filter((e) => !seen.has(e.linkId))];
      });
      this.hasMore.set(Boolean(result.hasMore));
      if (typeof result.total === 'number') {
        this.total.set(result.total);
      }
    } catch (err) {
      this.error.set(messageOf(err, 'Failed to load more'));
    } finally {
      this.loadingMore.set(false);
    }
  }

  /** v4's `handleDelete` (`:166-186`) — confirm-gated, optimistic. */
  protected async onDelete(entry: GalleryEntry): Promise<void> {
    if (
      !window.confirm(
        'Remove this photo from this album? Other albums that link the same image will keep their copy.',
      )
    ) {
      return;
    }
    this.deleting.set(entry.linkId);
    try {
      await removeGalleryEntry(this.core, entry.linkId);
      this.entries.update((prev) => prev.filter((e) => e.linkId !== entry.linkId));
      this.total.update((prev) => Math.max(0, prev - 1));
      if (this.selected()?.linkId === entry.linkId) {
        this.selected.set(null);
      }
    } catch (err) {
      this.error.set(messageOf(err, 'Failed to delete'));
    } finally {
      this.deleting.set(null);
    }
  }

  /** v4's modal closes on Escape (`:394-400`). */
  protected onEscape(): void {
    this.selected.set(null);
  }
}

function messageOf(err: unknown, fallback: string): string {
  return err instanceof Error && err.message ? err.message : fallback;
}
