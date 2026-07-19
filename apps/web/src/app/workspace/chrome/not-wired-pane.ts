/**
 * The loud not-yet-wired tab pane (the `not_available` idiom for the SPA).
 *
 * A tab whose kind cannot be hosted in-lane yet renders THIS, naming exactly
 * what is missing and where it lands. Two flavours:
 *  - **ACTIVATE-AT-UNIFY**: the v5 screen needs lane J2's contracted `input()`s
 *    (salon / terminal / document / settings / wardrobe / character-edit /
 *    character-view / custom-tools / document-standalone); the pane is replaced
 *    by the real screen at unify.
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

  protected readonly headline = computed(() =>
    this.kind() === 'brahma'
      ? 'The Brahma Console has no v5 surface yet.'
      : `This “${this.kind()}” tab is not wired in this lane yet.`,
  );

  protected readonly detail = computed(() =>
    this.kind() === 'brahma'
      ? 'It is deferred to p4.9i1 — its tab kind round-trips through the layout, but the console itself is unported.'
      : 'It lands at unification, once lane J2 delivers this screen’s tab-mode inputs. Until then, reach it via its own route.',
  );
}
