import { ChangeDetectionStrategy, Component, computed, inject, input } from '@angular/core';
import { DomSanitizer, type SafeHtml } from '@angular/platform-browser';

import { SmartTypographySettings } from '../smart-typography/settings';
import { renderMarkdownCached } from './render/render-cache';
import type { DialogueDetection, RenderingPattern } from './render/roleplay-rendering';

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
  /**
   * The chat's roleplay-template patterns (v4 `MessageContent renderingPatterns`,
   * threaded from `SalonView`'s template fetch). Absent — or an EMPTY array —
   * falls back to the built-in defaults; a non-empty array REPLACES them. The
   * fallback itself lives in the renderer, where v4 puts it
   * (`MessageContent.tsx:333-338`, ported case-for-case in
   * `render/markdown-renderer.spec.ts`).
   */
  readonly renderingPatterns = input<RenderingPattern[] | undefined>(undefined);
  /**
   * The chat's paragraph-level dialogue detection (v4 `dialogueDetection`). Note
   * the deliberate asymmetry with the patterns above: this one falls back only on
   * a NULLISH value, never on an "empty-looking" object.
   */
  readonly dialogueDetection = input<DialogueDetection | null | undefined>(undefined);

  private readonly sanitizer = inject(DomSanitizer);
  /**
   * Smart typography, Part A: curl quotes on DISPLAY only. Read here rather than
   * threaded down as an input because this component is the message renderer for
   * every surface that has one — the Salon, streaming messages, thinking blocks,
   * announcements, the Brahma console — and the setting is a display preference
   * that should apply to all of them consistently (v4 `MessageContent.tsx:345`,
   * verbatim reasoning). A template that curling would break overrides it inside
   * the renderer; see `render/typography.ts`.
   */
  private readonly displayQuotes = inject(SmartTypographySettings).displayQuotes;

  protected readonly html = computed<SafeHtml>(() =>
    this.sanitizer.bypassSecurityTrustHtml(
      renderMarkdownCached(this.content(), {
        blobMountPointId: this.blobMountPointId(),
        renderingPatterns: this.renderingPatterns(),
        dialogueDetection: this.dialogueDetection(),
        displayQuotes: this.displayQuotes(),
      }),
    ),
  );
}
