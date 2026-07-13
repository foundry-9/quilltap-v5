import { ChangeDetectionStrategy, Component, computed, inject, input } from '@angular/core';
import { DomSanitizer, type SafeHtml } from '@angular/platform-browser';

import { renderMarkdownCached } from './render/render-cache';

/**
 * Renders a message's markdown through the v4-parity pipeline
 * ({@link renderMarkdownToHtml}) into the `qt-chat-message-content` container.
 *
 * v5 always renders client-side (it dropped v4's server `renderedHtml` fast
 * path). The pipeline emits only the safe tag/class set it controls (raw HTML is
 * dropped by remark-rehype's default), so — like v4's `dangerouslySetInnerHTML`
 * — the output is trusted verbatim, which keeps `qtap://` hrefs intact (Angular's
 * URL sanitizer would otherwise rewrite them to `unsafe:`).
 */
@Component({
  selector: 'qt-message-content',
  changeDetection: ChangeDetectionStrategy.OnPush,
  template: `<div
    class="qt-chat-message-content qt-prose prose prose-sm qt-prose-auto"
    [innerHTML]="html()"
  ></div>`,
})
export class MessageContent {
  readonly content = input.required<string>();
  /**
   * The chat's blob mount point — when set, relative markdown image refs are
   * rewritten to the mount-point blob route (v4 `MessageContent blobMountPointId`).
   */
  readonly blobMountPointId = input<string | null>(null);

  private readonly sanitizer = inject(DomSanitizer);

  protected readonly html = computed<SafeHtml>(() =>
    this.sanitizer.bypassSecurityTrustHtml(
      renderMarkdownCached(this.content(), { blobMountPointId: this.blobMountPointId() }),
    ),
  );
}
