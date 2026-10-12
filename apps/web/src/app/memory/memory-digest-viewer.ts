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

/**
 * The digest viewer (v4 `components/memory/memory-list.tsx:370-389` at
 * `01a83539d`): the digest that superseded an archived memory, shown in v4's
 * hand-written overlay over a handler-less card.
 *
 * **Transcribed as v4 writes it (P4.D276 R-H, recorded):** v4's overlay is NOT
 * a dialog component — no `role="dialog"`, no backdrop-click close, no Escape
 * handler; the footer `Close` is the only exit. v5's `qt-modal` would add all
 * three, so the markup is ported verbatim instead.
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
  imports: [MemoryCard],
  template: `
    <div class="qt-dialog-overlay p-4 z-[100]">
      <div class="qt-dialog max-w-xl max-h-[90vh] overflow-y-auto">
        <div class="qt-dialog-header">
          <h2 class="qt-dialog-title">The Digest</h2>
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
