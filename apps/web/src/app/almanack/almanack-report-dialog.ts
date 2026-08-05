import {
  ChangeDetectionStrategy,
  Component,
  DestroyRef,
  computed,
  inject,
  input,
  output,
} from '@angular/core';
import { DomSanitizer, type SafeHtml } from '@angular/platform-browser';

import { apiUrl } from '../core/api-url';
import { Icon } from '../ui/icon';
import { renderAlmanackMarkdown } from './almanack-markdown';
import { ALMANACK_TITLE } from './almanack-phases';

/**
 * The Almanack report viewer — a port of v4
 * `components/tools/capabilities-report-dialog.tsx` (HEAD `f7f1a956`).
 *
 * A near-full-screen modal (v4's `fixed inset-4 md:inset-8 lg:inset-12` with a
 * `max-w-6xl` cap) holding one edition's rendered markdown, with Escape-to-close,
 * a body-scroll lock while open, and a footer that can download the same edition
 * through the web edge.
 *
 * The title is the FILENAME and the subtitle is {@link ALMANACK_TITLE} — that
 * way a person with several editions open in sequence can tell which one they
 * are reading. There are no `?section=` anchors inside the dialog: the deep link
 * is the settings card's `sectionId`, not anything here.
 *
 * The markdown goes through the Almanack's own plain GFM pipeline rather than
 * the Salon renderer — see `almanack-markdown.ts` for why a report must not be
 * read as roleplay. The HTML is trusted verbatim for the same reason
 * `MessageContent` trusts its own: the pipeline drops raw HTML, and Angular's
 * URL sanitizer would otherwise rewrite `qtap://` hrefs to `unsafe:`.
 */
@Component({
  selector: 'qt-almanack-report-dialog',
  changeDetection: ChangeDetectionStrategy.OnPush,
  imports: [Icon],
  template: `
    <button
      type="button"
      class="qt-dialog-overlay !p-0 cursor-default border-none z-40"
      aria-label="Close dialog"
      (click)="close.emit()"
    ></button>

    <div
      class="fixed inset-4 md:inset-8 lg:inset-12 z-50 pointer-events-auto flex items-center justify-center"
    >
      <div
        class="qt-dialog w-full h-full max-w-6xl flex flex-col"
        role="dialog"
        aria-modal="true"
      >
        <div class="qt-dialog-header flex-shrink-0">
          <div class="flex items-center justify-between">
            <h2 class="qt-dialog-title">{{ filename() }}</h2>
            <button
              type="button"
              class="p-1 hover:qt-bg-muted rounded transition-colors"
              aria-label="Close"
              (click)="close.emit()"
            >
              <qt-icon name="close" class="w-5 h-5" />
            </button>
          </div>
          <p class="qt-dialog-description qt-text-small">{{ almanackTitle }}</p>
        </div>

        <div class="qt-dialog-body flex-1 overflow-y-auto min-h-0">
          <div
            class="qt-almanack-report prose prose-sm qt-prose-auto max-w-none"
            [innerHTML]="html()"
          ></div>
        </div>

        <div class="qt-dialog-footer flex-shrink-0 flex justify-end gap-3">
          <a
            [href]="downloadHref()"
            [download]="filename()"
            class="qt-button qt-button-secondary"
          >
            <qt-icon name="download" class="w-4 h-4" />
            Download
          </a>
          <button type="button" class="qt-button qt-button-primary" (click)="close.emit()">
            Close
          </button>
        </div>
      </div>
    </div>
  `,
})
export class AlmanackReportDialog {
  readonly reportId = input.required<string>();
  readonly filename = input.required<string>();
  readonly content = input.required<string>();
  readonly close = output<void>();

  protected readonly almanackTitle = ALMANACK_TITLE;

  private readonly sanitizer = inject(DomSanitizer);

  protected readonly html = computed<SafeHtml>(() =>
    this.sanitizer.bypassSecurityTrustHtml(renderAlmanackMarkdown(this.content())),
  );

  /**
   * The WEB-EDGE-ONLY download leg: the same `capabilities-report-get` action,
   * with `download=true` switching the edge to a `Content-Disposition:
   * attachment` response. A plain `<a download>` rather than a dispatch call,
   * exactly as v4 does it — the browser owns the save dialog.
   */
  protected readonly downloadHref = computed(() =>
    apiUrl(
      `/api/v1/system/tools?action=capabilities-report-get&reportId=${encodeURIComponent(
        this.reportId(),
      )}&download=true`,
    ),
  );

  constructor() {
    // Escape closes; body scroll is locked while open (v4's keydown + overflow
    // effect, `capabilities-report-dialog.tsx:29-46`). The dialog is created
    // only while open, so mount/teardown IS v4's `isOpen` transition.
    const onKeyDown = (e: KeyboardEvent) => {
      if (e.key === 'Escape') this.close.emit();
    };
    if (typeof document !== 'undefined') {
      document.addEventListener('keydown', onKeyDown);
      const prevOverflow = document.body.style.overflow;
      document.body.style.overflow = 'hidden';
      inject(DestroyRef).onDestroy(() => {
        document.removeEventListener('keydown', onKeyDown);
        document.body.style.overflow = prevOverflow;
      });
    }
  }
}
