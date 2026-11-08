import { ChangeDetectionStrategy, Component, computed, input, output } from '@angular/core';

import { Icon } from '../ui/icon';
import { Tooltip } from '../ui/tooltip';
import type { RaisedHandStatus } from './raised-hand';

/**
 * The chip's line, and whether its × lowers the hand or dismisses a note (v4
 * `RaisedHandChip.tsx`'s `describe`, `9eab8d2da`, the five lines verbatim).
 * Exported so the spec pins the table.
 */
export function describeRaisedHand(status: RaisedHandStatus): { label: string; lowers: boolean } {
  switch (status.kind) {
    case 'raised':
      return { label: 'Hand raised — the next speaker will be asked to yield', lowers: true };
    case 'asked':
      return { label: `${status.characterName} is weighing your raised hand…`, lowers: false };
    case 'yielded':
      return { label: `${status.characterName} yields the floor — it is yours`, lowers: false };
    case 'declined':
      return { label: `${status.characterName} carried on regardless; your hand is lowered`, lowers: false };
    case 'unasked':
      return { label: 'The room fell quiet before anyone was asked — the floor is yours', lowers: false };
  }
}

/**
 * RaisedHandChip — where the operator's raised hand stands (v4
 * `app/salon/[id]/components/RaisedHandChip.tsx`, `9eab8d2da`).
 *
 * Sits in the composer's chip row beside the pending Informs. While the hand
 * is up it says so (and, once the server has taken it, who is weighing it);
 * afterwards it says how it was answered — a yield, a decline, or a room that
 * fell quiet before anyone was asked — until the operator dismisses it or
 * speaks again. The status is computed from the stream by the Salon spine
 * (P4.D384); this component only draws it. v4's `onLower` / `onDismiss`
 * callbacks are the `lower` / `dismiss` outputs.
 */
@Component({
  selector: 'qt-raised-hand-chip',
  changeDetection: ChangeDetectionStrategy.OnPush,
  imports: [Icon, Tooltip],
  template: `
    <!-- The live region stays mounted so each step is announced as it changes. -->
    <div aria-live="polite">
      @if (status(); as s) {
        <div class="qt-chat-attachment-list mb-2">
          <div class="qt-chat-tool-result-chip" [attr.data-hand-state]="s.kind">
            <qt-icon
              name="hand-raised"
              class="qt-chat-attachment-chip-icon qt-chat-attachment-chip-icon-info"
            />
            <span class="text-foreground">{{ line()!.label }}</span>
            @if (s.kind !== 'asked') {
              <qt-tooltip [content]="line()!.lowers ? 'Lower your hand' : 'Dismiss'">
                <button
                  type="button"
                  class="qt-chat-attachment-chip-remove"
                  [attr.aria-label]="line()!.lowers ? 'Lower your hand' : 'Dismiss the raised-hand note'"
                  (click)="line()!.lowers ? lower.emit() : dismiss.emit()"
                >
                  <qt-icon name="close" class="w-4 h-4" />
                </button>
              </qt-tooltip>
            }
          </div>
        </div>
      }
    </div>
  `,
})
export class RaisedHandChip {
  readonly status = input<RaisedHandStatus | null>(null);
  /** Lower a hand that is still up. */
  readonly lower = output<void>();
  /** Put away an outcome note. */
  readonly dismiss = output<void>();

  protected readonly line = computed(() => {
    const s = this.status();
    return s ? describeRaisedHand(s) : null;
  });
}
