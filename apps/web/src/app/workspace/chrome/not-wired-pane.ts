/**
 * The loud not-yet-wired tab pane (the `not_available` idiom for the SPA).
 *
 * A tab whose kind cannot be hosted yet renders THIS, naming exactly what is
 * missing and where it lands. Post-unification, three kinds remain:
 *  - **`wardrobe`**: the `asTab` WardrobeView variant was ported by neither
 *    p4.9j lane (the P4.9f2 "not ported" marker stands) — a named p4.9j
 *    follow-up; the wardrobe DIALOG entry points on the character screens
 *    keep working meanwhile.
 *  - **`document-standalone`**: lane J2's tier-2 item 7, deferred whole —
 *    it needs the file-scoped (chat-less) document surface.
 *  - **permanent refusal**: `brahma` has no v5 surface at all — it stays this
 *    pane until `p4.9i1`.
 *
 * Never a silent stub: the message is user-visible and the deferral is named in
 * the status log and the final report.
 *
 * @module workspace/chrome/not-wired-pane
 */

import { ChangeDetectionStrategy, Component, computed, input } from '@angular/core';

import type { TabKind } from '../workspace-contract';

@Component({
  selector: 'qt-not-wired-pane',
  changeDetection: ChangeDetectionStrategy.OnPush,
  template: `
    <div class="qt-workspace-empty" role="status">
      <div>
        <p class="qt-workspace-empty-hint" data-not-wired [attr.data-kind]="kind()">
          {{ headline() }}
        </p>
        <p class="qt-workspace-empty-hint">{{ detail() }}</p>
      </div>
    </div>
  `,
})
export class NotWiredPane {
  readonly kind = input.required<TabKind>();

  protected readonly headline = computed(() => {
    switch (this.kind()) {
      case 'brahma':
        return 'The Brahma Console has no v5 surface yet.';
      case 'wardrobe':
        return 'The wardrobe has no tab surface yet.';
      case 'document-standalone':
        return 'Standalone documents have no tab surface yet.';
      default:
        return `This “${this.kind()}” tab is not wired yet.`;
    }
  });

  protected readonly detail = computed(() => {
    switch (this.kind()) {
      case 'brahma':
        return 'It is deferred to p4.9i1 — its tab kind round-trips through the layout, but the console itself is unported.';
      case 'wardrobe':
        return 'The in-tab wardrobe view is a named p4.9j follow-up; meanwhile the wardrobe dialog on the character screens works as ever.';
      case 'document-standalone':
        return 'The chat-less document editor is a named p4.9j follow-up; documents opened from a conversation work as ever.';
      default:
        return 'A named follow-up in the status log covers it; until then, reach it via its own route.';
    }
  });
}
