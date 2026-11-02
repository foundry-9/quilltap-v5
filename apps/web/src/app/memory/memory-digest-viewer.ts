import {
  ChangeDetectionStrategy,
  Component,
  DestroyRef,
  ElementRef,
  afterNextRender,
  inject,
  input,
  output,
} from '@angular/core';

import type { MemoryDto } from '../core/core-contract';
import { MemoryCard } from './memory-card';
import { DialogFocus, mintDialogId } from '../ui/dialog-focus';

/**
 * The digest viewer (v4 `components/memory/memory-list.tsx:370-389` at
 * `01a83539d`): the digest that superseded an archived memory, shown in v4's
 * hand-written overlay over a handler-less card.
 *
 * **Hand markup, as v4 writes it.** v4's overlay is still not a dialog
 * COMPONENT, but v4 moved at `dff76ebef`: the card carries `role="dialog"`,
 * `aria-labelledby={digestTitleId}`, `tabIndex={-1}` with `useDialogFocus`, and
 * `useEscapeKey(closeDigest, viewedDigest !== null)` — so the P4.D276 R-H "no
 * role, no Escape" record is RETIRED (P4.D358). There is still no backdrop
 * close; `qt-modal` would add one, so the markup stays hand-written.
 *
 * **The portal.** v4 `createPortal(…, document.body)` so the overlay clears the
 * workspace's stacking contexts. The host reparents to `document.body` AFTER
 * the first render (the `images/image-detail-modal.ts` idiom): every host
 * mounts this under an `@if`, and an embedded view's root nodes are inserted
 * after the view is created, so a constructor-time reparent is undone by the
 * insertion that follows it.
 */
@Component({
  selector: 'qt-memory-digest-viewer',
  changeDetection: ChangeDetectionStrategy.OnPush,
  host: { '(document:keydown.escape)': 'onEscape($event)' },
  imports: [DialogFocus, MemoryCard],
  template: `
    <div class="qt-dialog-overlay p-4 z-[100]">
      <div
        class="qt-dialog max-w-xl max-h-[90vh] overflow-y-auto"
        role="dialog"
        aria-modal="true"
        [attr.aria-labelledby]="titleId"
        tabindex="-1"
        qtDialogFocus
      >
        <div class="qt-dialog-header">
          <h2 class="qt-dialog-title" [id]="titleId">The Digest</h2>
          <p class="qt-dialog-description">The denser note that took this one’s place on the active shelf.</p>
        </div>
        <div class="p-6">
          <qt-memory-card [memory]="memory()" [showActions]="false" />
        </div>
        <div class="qt-dialog-footer flex justify-end">
          <button type="button" class="qt-button qt-button-secondary" (click)="close.emit()">
            Close
          </button>
        </div>
      </div>
    </div>
  `,
})
export class MemoryDigestViewer {
  /** One `aria-labelledby` target per instance — v4's `useId()` (C9 R1). */
  protected readonly titleId = mintDialogId();

  /** v4 `useEscapeKey(closeDigest, viewedDigest !== null)` — always, while rendered (`dff76ebef`). */
  protected onEscape(event: Event): void {
    if ((event as KeyboardEvent).defaultPrevented) return;
    this.close.emit();
  }

  readonly memory = input.required<MemoryDto>();
  readonly close = output<void>();

  constructor() {
    const host = inject<ElementRef<HTMLElement>>(ElementRef).nativeElement;
    inject(DestroyRef).onDestroy(() => host.remove());
    afterNextRender(() => {
      if (typeof document !== 'undefined') {
        document.body.appendChild(host);
      }
    });
  }
}
