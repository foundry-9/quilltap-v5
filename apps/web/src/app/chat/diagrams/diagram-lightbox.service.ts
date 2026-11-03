import {
  ApplicationRef,
  EnvironmentInjector,
  Injectable,
  createComponent,
  inject,
  type ComponentRef,
} from '@angular/core';

import { DiagramLightbox } from './diagram-lightbox';
import { captureScrollAnchor } from './scroll-anchor';

interface OpenLightbox {
  ref: ComponentRef<DiagramLightbox>;
  root: HTMLElement;
  restore: () => void;
}

export interface OpenDiagramLightboxOptions {
  svg: string;
  label: string;
  /** The diagram on the page: where the reader's place is kept and focus returns. */
  trigger: HTMLElement;
}

/**
 * Opens and closes the diagram lightbox.
 *
 * The lightbox gets a host of its own on `<body>` rather than living in the
 * diagram's row. The Salon transcript is virtualized: a row may be unmounted
 * while its diagram is being examined (a reply streaming in below scrolls it
 * out of the overscan), and a child view would vanish with it. A root of its
 * own outlives the row, and needs nothing from the row's context — the
 * drawing arrives as finished markup and the theme is page-wide CSS.
 *
 * One lightbox at a time: opening another closes the first.
 *
 * v5 port of v4 `components/chat/diagrams/diagram-lightbox-host.tsx`
 * (`9faed0c7e`, at 236c04e35): React's `createRoot` on a body-level container
 * becomes `createComponent` into a body-level host attached to the
 * application (P4.D353 R-3; `ui/alert-modal.service.ts` is the service-owned
 * precedent).
 */
@Injectable({ providedIn: 'root' })
export class DiagramLightboxService {
  private readonly appRef = inject(ApplicationRef);
  private readonly environmentInjector = inject(EnvironmentInjector);
  private open: OpenLightbox | null = null;

  /** Close the lightbox, if one is open, and put the transcript back. */
  close(): void {
    const current = this.open;
    if (!current) return;
    this.open = null;
    // Off the page at once; the view itself is destroyed after the event that
    // closed it (often one of its own buttons) has finished dispatching.
    current.root.remove();
    current.restore();
    queueMicrotask(() => {
      this.appRef.detachView(current.ref.hostView);
      current.ref.destroy();
    });
  }

  openLightbox({ svg, label, trigger }: OpenDiagramLightboxOptions): void {
    this.close();

    const restore = captureScrollAnchor(trigger);
    const root = document.createElement('div');
    root.className = 'qt-diagram-lightbox-root';
    document.body.appendChild(root);
    const ref = createComponent(DiagramLightbox, {
      environmentInjector: this.environmentInjector,
      hostElement: root,
    });
    ref.setInput('svg', svg);
    ref.setInput('label', label);
    ref.setInput('close', () => this.close());
    this.appRef.attachView(ref.hostView);
    ref.changeDetectorRef.detectChanges();
    this.open = { ref, root, restore };
  }

  /** Whether a lightbox is up (test seam). */
  isOpen(): boolean {
    return this.open !== null;
  }
}
