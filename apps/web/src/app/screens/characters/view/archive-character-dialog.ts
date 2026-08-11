import { ChangeDetectionStrategy, Component, input, output } from '@angular/core';

import { Modal } from '../../../ui/modal';

/**
 * The archive confirmation dialog (v4
 * `app/aurora/[id]/view/components/ArchiveCharacterDialog.tsx`) — it spells out
 * precisely what archiving packs away and what stays behind before the deed is
 * done (v4's character-archive spec §5.2). Every sentence is v4-verbatim; the
 * two boxes exist because "nothing is lost, it is merely put away" is only
 * believable if the operator can see the inventory.
 *
 * CHROME DIVERGENCE (deliberate): v4 hand-rolls a fixed overlay whose header
 * carries a folder icon and which offers no ✕ and no outside-dismiss. v5 uses
 * the shared `qt-modal`, so the header is title-only and both the ✕ and the
 * backdrop act as "Leave Them Be" — the safe arm either way. Blocked while the
 * archive is in flight, as v4 blocks both its buttons.
 */
@Component({
  selector: 'qt-archive-character-dialog',
  changeDetection: ChangeDetectionStrategy.OnPush,
  imports: [Modal],
  template: `
    <qt-modal
      [title]="'Set ' + characterName() + ' resting in the archive?'"
      maxWidth="lg"
      [closeOnBackdrop]="!working()"
      (close)="onCancel()"
    >
      <div class="space-y-4">
        <p class="qt-text-small">
          Archiving packs the whole of {{ characterName() }} — every last letter and photograph —
          into a single sealed bundle on the archive shelf, then clears the heavier effects from the
          working rooms. Nothing is lost; it is merely put away, and rehydrating unpacks it all
          again.
        </p>

        <div class="rounded-lg border qt-border-default qt-bg-muted/50 p-4 space-y-1.5">
          <p class="qt-text-label">Packed into the bundle and cleared away:</p>
          <ul class="qt-text-small qt-text-secondary list-disc pl-5 space-y-0.5">
            <li>Their memories (the Commonplace Book falls silent)</li>
            <li>Their correspondence — the whole of the mail folder</li>
            <li>Every photograph beyond the portrait itself</li>
            <li>Their conversation summaries</li>
          </ul>
        </div>

        <div class="rounded-lg border qt-border-default qt-bg-muted/50 p-4 space-y-1.5">
          <p class="qt-text-label">Kept in place, exactly as it stands:</p>
          <ul class="qt-text-small qt-text-secondary list-disc pl-5 space-y-0.5">
            <li>Who they are — every character field, still readable on their page</li>
            <li>Their portrait, so old conversations keep their face</li>
            <li>Their wardrobe</li>
            <li>Every chat they took part in, word for word</li>
            <li>
              What <em>other</em> characters remember about them — archiving silences the character,
              not everyone's memory of them
            </li>
          </ul>
        </div>

        <p class="qt-text-small qt-text-secondary">
          While archived they take no turns, receive no letters, and answer no queries. The bundle is
          sealed with your passphrase and rests at
          <span class="font-mono text-xs">files/&lt;id&gt;/character-archive.qtap</span>.
        </p>
      </div>

      <div qt-modal-footer class="flex gap-3 justify-end w-full">
        <button
          type="button"
          class="qt-button qt-button-secondary"
          [disabled]="working()"
          (click)="onCancel()"
        >
          Leave Them Be
        </button>
        <button
          type="button"
          class="qt-button qt-button-primary"
          [disabled]="working()"
          (click)="onConfirm()"
        >
          {{ working() ? 'Packing the bundle…' : 'Archive' }}
        </button>
      </div>
    </qt-modal>
  `,
})
export class ArchiveCharacterDialog {
  readonly characterName = input.required<string>();

  readonly confirm = output<void>();
  readonly cancel = output<void>();

  /**
   * v4 holds `working` in the dialog and clears it in a `finally`, so the buttons
   * come back even when the archive fails and the dialog stays open; the PARENT
   * closes the dialog on success (v4 `:281`). v5 lifts the flag to an input
   * because the parent already owns the in-flight state — same observable
   * behavior, one source of truth.
   */
  readonly working = input(false);

  protected onConfirm(): void {
    if (this.working()) {
      return;
    }
    this.confirm.emit();
  }

  protected onCancel(): void {
    if (this.working()) {
      return;
    }
    this.cancel.emit();
  }
}
