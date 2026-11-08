import { ChangeDetectionStrategy, Component, input, output } from '@angular/core';

/**
 * The footer of a "Move / Copy to another tier" dialog — the wardrobe's and
 * the Inventory's: Cancel, and the submit button that reads *Moving…* /
 * *Copying…* while the transfer is out. A port of v4
 * `components/item-browser/ItemTransferFooter.tsx` at the pin `624d0d25c`
 * (NEW at `600c2906f`). The bodies (and the request each kind sends) stay the
 * kinds' own; the transfer services differ on purpose.
 *
 * Project it into a `qt-modal` footer (`<qt-item-transfer-footer qt-modal-footer … />`).
 */
@Component({
  selector: 'qt-item-transfer-footer',
  changeDetection: ChangeDetectionStrategy.OnPush,
  template: `
    <div class="flex justify-end gap-2">
      <button type="button" class="qt-button-secondary qt-button-sm" (click)="cancelled.emit()" [disabled]="working()">
        Cancel
      </button>
      <button
        type="button"
        class="qt-button-primary qt-button-sm"
        (click)="submitted.emit()"
        [disabled]="working() || submitDisabled()"
      >
        {{ working() ? (mode() === 'move' ? 'Moving…' : 'Copying…') : (submitLabel() ?? (mode() === 'move' ? 'Move' : 'Copy')) }}
      </button>
    </div>
  `,
})
export class ItemTransferFooter {
  readonly mode = input.required<'move' | 'copy'>();
  readonly working = input(false);
  /** The submit button's label at rest; defaults to `Move` / `Copy`. */
  readonly submitLabel = input<string | undefined>(undefined);
  /** Disable the submit button (nothing chosen yet). */
  readonly submitDisabled = input(false);
  readonly cancelled = output<void>();
  /** v4 `onSubmit` (`submit` is a native DOM event name). */
  readonly submitted = output<void>();
}
