/**
 * The Guide's document reader (port of v4
 * `components/help-chat/HelpTopicReader.tsx`).
 *
 * Fetches one document by slug, renders it through {@link renderHelpDocument},
 * and delegates the two in-document link kinds the pipeline turned into buttons:
 * a sibling topic (`data-help-doc`) opens IN the Guide, an app route
 * (`data-help-page`) navigates — that second one is how the "Open this page in
 * Quilltap" callout at the top of every help document works (see
 * `help-doc-markdown.ts` for why it is the `a` branch and not v4's blockquote
 * branch that produces it).
 *
 * One delegated listener on the container rather than a listener per button,
 * because the buttons are `innerHTML` and have no component instances.
 *
 * @module help/help-topic-reader
 */

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
import { DomSanitizer, type SafeHtml } from '@angular/platform-browser';

import { Icon } from '../ui/icon';
import { renderHelpDocument } from './help-doc-markdown';
import { HelpApi } from './help-wire';

@Component({
  selector: 'qt-help-topic-reader',
  changeDetection: ChangeDetectionStrategy.OnPush,
  imports: [Icon],
  host: { class: 'contents' },
  template: `
    <div class="flex flex-col h-full">
      <button type="button" class="qt-help-guide-back" (click)="back.emit()">
        <qt-icon name="arrow-left" class="w-4 h-4" />
        {{ categoryLabel() }}
      </button>
      @if (loading()) {
        <div class="flex-1 flex items-center justify-center qt-text-secondary text-sm">
          Loading...
        </div>
      } @else if (error()) {
        <div class="flex-1 flex items-center justify-center qt-text-destructive text-sm">
          {{ error() }}
        </div>
      } @else {
        <div #reader class="qt-help-guide-reader" [innerHTML]="html()" (click)="onClick($event)"></div>
      }
    </div>
  `,
})
export class HelpTopicReader {
  readonly documentId = input.required<string>();
  readonly categoryLabel = input.required<string>();
  /** When set, scroll to this position after content loads instead of top. */
  readonly restoreScrollTop = input<number | undefined>(undefined);
  readonly back = output<void>();
  readonly navigateDoc = output<string>();
  readonly navigatePage = output<string>();
  /** The reader's live scrollTop, so the Guide can bank it before a jump. */
  readonly scrolled = output<number>();

  private readonly api = inject(HelpApi);
  private readonly sanitizer = inject(DomSanitizer);

  protected readonly content = signal<string | null>(null);
  protected readonly loading = signal(true);
  protected readonly error = signal<string | null>(null);

  private readonly readerEl = viewChild<ElementRef<HTMLDivElement>>('reader');

  /**
   * Trusted verbatim, exactly as `chat/message-content.ts` and the Almanack
   * dialog trust theirs: the pipeline drops raw HTML (remark-rehype's default)
   * and emits only the tag set it controls, so this is the same safety property
   * v4 relies on when it hands ReactMarkdown's output to React.
   */
  protected readonly html = computed<SafeHtml>(() =>
    this.sanitizer.bypassSecurityTrustHtml(renderHelpDocument(this.content() ?? '')),
  );

  constructor() {
    effect((onCleanup) => {
      const id = this.documentId();
      let cancelled = false;
      onCleanup(() => {
        cancelled = true;
      });
      this.loading.set(true);
      this.error.set(null);
      void (async () => {
        try {
          const doc = await this.api.docGet(id);
          if (cancelled) return;
          if (!doc) throw new Error('Document not found');
          this.content.set(doc.content);
        } catch (err) {
          if (cancelled) return;
          this.error.set(err instanceof Error ? err.message : 'Failed to load document');
        } finally {
          if (!cancelled) this.loading.set(false);
        }
      })();
    });

    // Scroll to the restored position, or the top, once the document lands
    // (v4's effect on `[documentId, loading, content, restoreScrollTop]`).
    effect(() => {
      const ready = !this.loading() && this.content();
      const top = this.restoreScrollTop();
      void this.documentId();
      if (!ready) return;
      queueMicrotask(() => this.readerEl()?.nativeElement.scrollTo(0, top ?? 0));
    });
  }

  /**
   * Delegate the pipeline's two button kinds. The Guide banks this reader's
   * scroll position first, so a jump to a sibling topic can come back to it.
   */
  protected onClick(event: Event): void {
    const target = (event.target as HTMLElement | null)?.closest<HTMLElement>(
      '[data-help-doc],[data-help-page]',
    );
    if (!target) return;
    event.preventDefault();
    this.scrolled.emit(this.readerEl()?.nativeElement.scrollTop ?? 0);
    const docId = target.getAttribute('data-help-doc');
    if (docId) {
      this.navigateDoc.emit(docId);
      return;
    }
    const page = target.getAttribute('data-help-page');
    if (page) this.navigatePage.emit(page);
  }
}
