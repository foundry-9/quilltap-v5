import { ChangeDetectionStrategy, Component, input, output } from '@angular/core';

import { MarkdownField } from '../../editor/markdown-field';
import { QuillAnimation } from '../quill-animation';

/**
 * `qt-voice-rewrite-review-panel` — the review half of a "say it in the
 * character's own voice" rehearsal: the heading, the generating state, and the
 * editable proposal (v4 `components/chat/VoiceRewriteReviewPanel.tsx`,
 * `686954937`).
 *
 * Shared by the off-scene rehearsal (Insert Announcement) and the in-scene one
 * (In Their Own Words) so the two cannot drift apart in look or behaviour.
 * Presentation only — every decision about when it appears and what becomes of
 * the text stays with the dialog.
 *
 * v4 extracted this MECHANICALLY out of `InsertAnnouncementDialog`; so does this,
 * and the announcement dialog's existing specs stay green unchanged. v4's
 * `namespace` prop has no counterpart: it names a Lexical editor instance, and
 * v5's ProseMirror-based `qt-markdown-field` needs no such key (D17).
 *
 * @module chat/impersonation-voice/voice-rewrite-review-panel
 */
@Component({
  selector: 'qt-voice-rewrite-review-panel',
  // `display: contents` so the extraction is exactly mechanical: v4 kept a bare
  // `<div>` root in the DOM, and an inline custom element interposed between
  // `.qt-dialog-body` and that div would be the #97/#107 inline-host class (the
  // `concierge-mark` / `confirmation-badge` idiom). Landed at the `f4ad2c8d1`
  // unification (§3 review).
  host: { style: 'display: contents' },
  changeDetection: ChangeDetectionStrategy.OnPush,
  imports: [MarkdownField, QuillAnimation],
  template: `
    <div>
      <label class="block text-sm qt-text-primary mb-2">
        What {{ characterName() }} will say
      </label>
      @if (generating()) {
        <div
          class="qt-border-primary border rounded p-6 flex flex-col items-center justify-center gap-3 min-h-32"
        >
          <qt-quill-animation size="lg" />
          <div class="qt-text-secondary text-sm">Generating in character…</div>
        </div>
      } @else {
        <qt-markdown-field
          [value]="value()"
          [disabled]="disabled()"
          minHeight="12rem"
          [ariaLabel]="ariaLabel()"
          (contentChange)="valueChange.emit($event)"
        />
      }
    </div>
  `,
})
export class VoiceRewriteReviewPanel {
  /** Name to put in the "What {name} will say" heading. */
  readonly characterName = input.required<string>();
  /** True while the rewrite is in flight — shows the quill in place of the editor. */
  readonly generating = input(false);
  readonly value = input('');
  readonly disabled = input(false);
  readonly ariaLabel = input.required<string>();

  readonly valueChange = output<string>();
}
