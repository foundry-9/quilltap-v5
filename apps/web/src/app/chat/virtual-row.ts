import { Directive, DestroyRef, ElementRef, afterNextRender, inject, input } from '@angular/core';
import type { AngularVirtualizer } from '@tanstack/angular-virtual';

/**
 * Registers a virtualized row element for dynamic measurement — the Angular
 * equivalent of v4's `ref={virtualizer.measureElement}`
 * (`VirtualizedMessageList.tsx`). The virtual-core `measureElement(node)` reads
 * the row's `data-index`, caches its size, and (in the browser) observes it with
 * a `ResizeObserver` so height changes after mount — images loading, an edit
 * textarea opening — re-measure automatically.
 *
 * The host element must carry `[attr.data-index]` (set by the message-list
 * template). On destroy we call `measureElement(null)`, virtual-core's prune of
 * disconnected nodes, matching React's `ref(null)` on unmount.
 */
@Directive({ selector: '[qtVirtualRow]' })
export class VirtualRow {
  private readonly el = inject<ElementRef<HTMLElement>>(ElementRef);

  /** The virtualizer instance (the `injectVirtualizer` signal-proxy). */
  readonly virtualizer = input.required<AngularVirtualizer<HTMLElement, Element>>({
    alias: 'qtVirtualRow',
  });

  constructor() {
    // Measure after the row's content has rendered (markdown is synchronous, so
    // the height is stable by the next render; the ResizeObserver catches later
    // async growth). afterNextRender is browser-only — SSR skips measurement.
    afterNextRender(() => this.virtualizer().measureElement(this.el.nativeElement));
    inject(DestroyRef).onDestroy(() => this.virtualizer().measureElement(null));
  }
}
