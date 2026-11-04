import {
  ChangeDetectionStrategy,
  Component,
  DestroyRef,
  ElementRef,
  InjectionToken,
  afterNextRender,
  computed,
  effect,
  inject,
  input,
  signal,
  viewChild,
} from '@angular/core';
import { DomSanitizer } from '@angular/platform-browser';

import { copyPngBlobToClipboard } from '../../core/clipboard-utils';
import { Icon } from '../../ui/icon';
import { ToastService } from '../../ui/toast.service';
import {
  DiagramExportError,
  PNG_EXPORT_SCALE,
  diagramToPngBlob,
  safeExportScale,
} from './export-png';
import {
  ZOOM_STEP,
  centeredView,
  fitView,
  naturalSvgSize,
  wheelZoomFactor,
  zoomTo,
  type Size,
  type View,
} from './lightbox-geometry';
import { isInFloatingLayer } from '../../ui/floating-layer';

/**
 * The PNG pipeline the Copy PNG button drives. A token only so a spec can
 * stand mocks in, as v4's jest suite does (`DiagramLightbox.test.tsx:11-16`).
 */
export const DIAGRAM_PNG_COPY = new InjectionToken<{
  toPngBlob: typeof diagramToPngBlob;
  copyPng: typeof copyPngBlobToClipboard;
}>('DIAGRAM_PNG_COPY', {
  providedIn: 'root',
  factory: () => ({ toPngBlob: diagramToPngBlob, copyPng: copyPngBlobToClipboard }),
});

/** Pixels a press may wander and still count as a click on the backdrop. */
const CLICK_SLOP = 4;

// The Transparent choice lasts for the page's life, so a run of copies keeps it.
let rememberedTransparent = false;

type CopyState = 'idle' | 'working' | 'copied';

interface GestureLikeEvent extends Event {
  scale: number;
  clientX: number;
  clientY: number;
}

function failureMessage(err: unknown): string {
  if (err instanceof DiagramExportError) {
    if (err.reason === 'tainted')
      return 'The browser would not let this drawing be copied as a picture.';
    if (err.reason === 'too-large') return 'This drawing is too large to copy as a picture.';
    return 'The drawing could not be turned into a picture.';
  }
  return 'The picture could not be put on the clipboard.';
}

/**
 * DiagramLightbox — one diagram, full-window, to be examined and exported.
 *
 * Wheel, trackpad pinch (Chromium's ctrl-wheel and Safari's gesture events)
 * and two-finger touch pinch zoom about the point under the pointer; a drag
 * pans. **Fit** shows the whole drawing as large as the window allows and is
 * where the lightbox opens; **100%** shows it at its natural size. While Fit
 * is in force a window resize re-fits.
 *
 * **Copy PNG** puts a twice-natural-size picture on the clipboard (see
 * export-png.ts for why that is harder than it sounds), on the diagram's own
 * background or — with **Transparent** ticked — on nothing at all.
 *
 * The drawing is scaled by sizing it, not by a CSS transform, so the browser
 * re-draws the vector at every zoom and it stays sharp. Escape, the close
 * button or a click on the empty backdrop closes it; the caller (the lightbox
 * service) puts the transcript back where it was.
 *
 * v5 port of v4 `components/chat/diagrams/DiagramLightbox.tsx` (`9faed0c7e`,
 * at 236c04e35). The React refs that let gesture handlers read the latest view
 * without waiting for a render are plain fields here; the listeners React
 * could not bind non-passively are bound by hand, as v4's are. The dialog is
 * named by `aria-label` (v4's byte), not `aria-labelledby`.
 */
@Component({
  selector: 'qt-diagram-lightbox',
  changeDetection: ChangeDetectionStrategy.OnPush,
  imports: [Icon],
  host: { class: 'qt-diagram-lightbox-root' },
  template: `
    <div
      #dialog
      class="qt-diagram-lightbox"
      role="dialog"
      aria-modal="true"
      [attr.aria-label]="label()"
      tabindex="-1"
    >
      <div class="qt-diagram-lightbox-bar" role="toolbar" aria-label="Diagram view">
        <div class="qt-diagram-lightbox-group">
          <button
            type="button"
            class="qt-diagram-lightbox-button"
            (click)="zoomBy(1 / ZOOM_STEP)"
            title="Zoom out (−)"
            aria-label="Zoom out"
          >
            <qt-icon name="zoom-out" class="w-4 h-4" />
          </button>
          <span
            class="qt-diagram-lightbox-zoom"
            aria-live="polite"
            [attr.aria-label]="'Zoom ' + percent() + '%'"
            >{{ percent() }}%</span
          >
          <button
            type="button"
            class="qt-diagram-lightbox-button"
            (click)="zoomBy(ZOOM_STEP)"
            title="Zoom in (+)"
            aria-label="Zoom in"
          >
            <qt-icon name="zoom-in" class="w-4 h-4" />
          </button>
          <button
            type="button"
            class="qt-diagram-lightbox-button"
            (click)="fit()"
            [attr.aria-pressed]="fitted()"
            title="Fit the whole drawing to the window"
          >
            <qt-icon name="compress" class="w-4 h-4" />
            <span>Fit</span>
          </button>
          <button
            type="button"
            class="qt-diagram-lightbox-button"
            (click)="actualSize()"
            [attr.aria-pressed]="!fitted() && naturalPressed()"
            title="Natural size (0)"
          >
            <span>100%</span>
          </button>
        </div>
        <div class="qt-diagram-lightbox-group">
          <label class="qt-diagram-lightbox-check" title="Copy without a background">
            <input
              type="checkbox"
              [checked]="transparent()"
              (change)="handleTransparent($any($event.target).checked)"
            />
            <span>Transparent</span>
          </label>
          <button
            type="button"
            class="qt-diagram-lightbox-button"
            (click)="handleCopyPng()"
            [disabled]="copyState() === 'working'"
            [title]="'Copy as a PNG at ' + PNG_EXPORT_SCALE + '× resolution'"
          >
            <qt-icon [name]="copyState() === 'copied' ? 'check' : 'image'" class="w-4 h-4" />
            <span>{{
              copyState() === 'copied'
                ? 'Copied'
                : copyState() === 'working'
                  ? 'Copying…'
                  : 'Copy PNG'
            }}</span>
          </button>
          <button
            type="button"
            class="qt-diagram-lightbox-button"
            (click)="close()()"
            title="Close (Esc)"
            aria-label="Close"
          >
            <qt-icon name="close" class="w-4 h-4" />
          </button>
        </div>
      </div>
      <div
        #viewport
        class="qt-diagram-lightbox-viewport"
        (pointerdown)="onPointerDown($event)"
        (pointermove)="onPointerMove($event)"
        (pointerup)="onPointerUp($event)"
        (pointercancel)="onPointerUp($event)"
        (dblclick)="onDoubleClick($event)"
      >
        <div
          #sheet
          class="qt-diagram-lightbox-sheet"
          [attr.data-transparent]="transparent() ? 'true' : null"
          [style.left.px]="natural() ? view().x : null"
          [style.top.px]="natural() ? view().y : null"
          [style.width.px]="natural() ? natural()!.width * view().scale : null"
          [style.height.px]="natural() ? natural()!.height * view().scale : null"
          [style.visibility]="natural() ? null : 'hidden'"
          [innerHTML]="markup()"
        ></div>
      </div>
      <p class="qt-diagram-lightbox-hint">Wheel or pinch to zoom · drag to pan · Esc to return</p>
    </div>
  `,
})
export class DiagramLightbox {
  /** The drawn diagram's markup, exactly as shown in the message. */
  readonly svg = input.required<string>();
  /** What the dialog announces itself as. */
  readonly label = input.required<string>();
  readonly close = input.required<() => void>();

  protected readonly ZOOM_STEP = ZOOM_STEP;
  protected readonly PNG_EXPORT_SCALE = PNG_EXPORT_SCALE;

  private readonly sanitizer = inject(DomSanitizer);
  private readonly toasts = inject(ToastService);
  private readonly png = inject(DIAGRAM_PNG_COPY);
  private readonly dialogRef = viewChild.required<ElementRef<HTMLDivElement>>('dialog');
  private readonly viewportRef = viewChild.required<ElementRef<HTMLDivElement>>('viewport');
  private readonly sheetRef = viewChild.required<ElementRef<HTMLDivElement>>('sheet');

  protected readonly markup = computed(() => this.sanitizer.bypassSecurityTrustHtml(this.svg()));
  protected readonly natural = signal<Size | null>(null);
  protected readonly view = signal<View>({ scale: 1, x: 0, y: 0 });
  protected readonly fitted = signal(true);
  protected readonly transparent = signal(rememberedTransparent);
  protected readonly copyState = signal<CopyState>('idle');
  protected readonly percent = computed(() => Math.round(this.view().scale * 100));
  protected readonly naturalPressed = computed(() => Math.abs(this.view().scale - 1) < 0.005);

  // Gesture handlers read the latest view without re-binding, and without
  // waiting for a render: a burst of pointer moves must compound.
  private viewNow: View = { scale: 1, x: 0, y: 0 };
  private naturalNow: Size | null = null;

  // Drag to pan; two fingers to pinch.
  private readonly pointers = new Map<number, { x: number; y: number }>();
  private press: { x: number; y: number; moved: boolean; onBackdrop: boolean } | null = null;
  private copiedTimer: ReturnType<typeof setTimeout> | null = null;

  constructor() {
    const destroyRef = inject(DestroyRef);

    afterNextRender(() => {
      // Measure the drawing once it is in the sheet, then open at Fit.
      const el = this.sheetRef().nativeElement.querySelector('svg');
      if (el) {
        const size = naturalSvgSize(el);
        // Without a viewBox, resizing an SVG grows its canvas, not its drawing
        // (svg2roughjs's sketches carry only width and height).
        if (!el.getAttribute('viewBox'))
          el.setAttribute('viewBox', `0 0 ${size.width} ${size.height}`);
        el.style.width = '100%';
        el.style.height = '100%';
        el.style.maxWidth = 'none';
        el.style.maxHeight = 'none';
        el.style.display = 'block';
        this.naturalNow = size;
        this.natural.set(size);
        this.applyView(fitView(size, this.viewportSize()), true);
      }

      // Escape closes, before anything in the Salon behind can act on it
      // (Document and Terminal Mode both leave focus mode on Escape).
      const onKey = (e: KeyboardEvent) => this.onKey(e);
      window.addEventListener('keydown', onKey, true);

      // Wheel and Safari's trackpad gestures. Bound by hand, non-passive, so
      // the page cannot scroll underneath.
      const viewport = this.viewportRef().nativeElement;
      const local = (clientX: number, clientY: number) => {
        const rect = viewport.getBoundingClientRect();
        return { x: clientX - rect.left, y: clientY - rect.top };
      };
      const onWheel = (e: WheelEvent) => {
        e.preventDefault();
        this.zoomBy(wheelZoomFactor(e.deltaY, e.deltaMode, e.ctrlKey), local(e.clientX, e.clientY));
      };
      let gestureStartScale = 1;
      const onGestureStart = (e: Event) => {
        e.preventDefault();
        gestureStartScale = this.viewNow.scale;
      };
      const onGestureChange = (e: Event) => {
        e.preventDefault();
        const g = e as GestureLikeEvent;
        this.applyView(
          zoomTo(this.viewNow, gestureStartScale * g.scale, local(g.clientX, g.clientY)),
        );
      };
      viewport.addEventListener('wheel', onWheel, { passive: false });
      viewport.addEventListener('gesturestart', onGestureStart);
      viewport.addEventListener('gesturechange', onGestureChange);
      viewport.addEventListener('gestureend', onGestureStart);

      this.dialogRef().nativeElement.focus({ preventScroll: true });

      destroyRef.onDestroy(() => {
        window.removeEventListener('keydown', onKey, true);
        viewport.removeEventListener('wheel', onWheel);
        viewport.removeEventListener('gesturestart', onGestureStart);
        viewport.removeEventListener('gesturechange', onGestureChange);
        viewport.removeEventListener('gestureend', onGestureStart);
      });
    });

    // Fit follows the window while it is in force.
    effect((onCleanup) => {
      if (!this.fitted()) return;
      const onResize = () => this.fit();
      window.addEventListener('resize', onResize);
      onCleanup(() => window.removeEventListener('resize', onResize));
    });

    destroyRef.onDestroy(() => {
      if (this.copiedTimer) clearTimeout(this.copiedTimer);
    });
  }

  private viewportSize(): Size {
    const el = this.viewportRef().nativeElement;
    return { width: el.clientWidth ?? 0, height: el.clientHeight ?? 0 };
  }

  private applyView(next: View, stillFitted = false): void {
    this.viewNow = next;
    this.view.set(next);
    this.fitted.set(stillFitted);
  }

  protected fit(): void {
    const size = this.naturalNow;
    if (size) this.applyView(fitView(size, this.viewportSize()), true);
  }

  protected actualSize(): void {
    const size = this.naturalNow;
    if (size) this.applyView(centeredView(size, this.viewportSize(), 1));
  }

  /** Zoom by `factor` about a viewport point (default: the middle). */
  protected zoomBy(factor: number, focus?: { x: number; y: number }): void {
    const vp = this.viewportSize();
    const current = this.viewNow;
    this.applyView(
      zoomTo(current, current.scale * factor, focus ?? { x: vp.width / 2, y: vp.height / 2 }),
    );
  }

  private onKey(e: KeyboardEvent): void {
    if (isInFloatingLayer(e.target)) return;
    if (e.key === 'Escape') {
      e.preventDefault();
      e.stopImmediatePropagation();
      this.close()();
      return;
    }
    if (e.metaKey || e.ctrlKey || e.altKey) return;
    const target = e.target as HTMLElement | null;
    if (target?.closest?.('input, textarea, select')) return;
    if (e.key === '+' || e.key === '=') {
      e.preventDefault();
      this.zoomBy(ZOOM_STEP);
    } else if (e.key === '-' || e.key === '_') {
      e.preventDefault();
      this.zoomBy(1 / ZOOM_STEP);
    } else if (e.key === '0') {
      e.preventDefault();
      this.actualSize();
    } else if (e.key === 'Tab') {
      // Keep focus inside the dialog.
      const dialog = this.dialogRef().nativeElement;
      const focusable = Array.from(
        dialog.querySelectorAll<HTMLElement>('button, input, [tabindex]:not([tabindex="-1"])'),
      ).filter((el) => !el.hasAttribute('disabled'));
      if (focusable.length === 0) return;
      const first = focusable[0];
      const last = focusable[focusable.length - 1];
      const active = document.activeElement;
      if (e.shiftKey && (active === first || !dialog.contains(active))) {
        e.preventDefault();
        last.focus();
      } else if (!e.shiftKey && active === last) {
        e.preventDefault();
        first.focus();
      }
    }
  }

  protected onPointerDown(e: PointerEvent): void {
    if (e.button !== 0 && e.pointerType === 'mouse') return;
    const viewport = e.currentTarget as HTMLElement;
    viewport.setPointerCapture?.(e.pointerId);
    this.pointers.set(e.pointerId, { x: e.clientX, y: e.clientY });
    if (this.pointers.size === 1) {
      this.press = {
        x: e.clientX,
        y: e.clientY,
        moved: false,
        onBackdrop: e.target === e.currentTarget,
      };
    } else if (this.press) {
      this.press.moved = true;
    }
  }

  protected onPointerMove(e: PointerEvent): void {
    const previous = this.pointers.get(e.pointerId);
    if (!previous) return;
    const rect = (e.currentTarget as HTMLElement).getBoundingClientRect();
    const others = Array.from(this.pointers.entries()).filter(([id]) => id !== e.pointerId);
    this.pointers.set(e.pointerId, { x: e.clientX, y: e.clientY });

    if (this.press && Math.hypot(e.clientX - this.press.x, e.clientY - this.press.y) > CLICK_SLOP) {
      this.press.moved = true;
    }

    const current = this.viewNow;
    if (others.length === 0) {
      this.applyView({
        ...current,
        x: current.x + e.clientX - previous.x,
        y: current.y + e.clientY - previous.y,
      });
      return;
    }
    // Pinch: scale by the change in finger spread, about their midpoint, and
    // follow the midpoint as it travels.
    const [, other] = others[0];
    const before = Math.hypot(previous.x - other.x, previous.y - other.y);
    const after = Math.hypot(e.clientX - other.x, e.clientY - other.y);
    if (before < 1) return;
    const midBefore = {
      x: (previous.x + other.x) / 2 - rect.left,
      y: (previous.y + other.y) / 2 - rect.top,
    };
    const midAfter = {
      x: (e.clientX + other.x) / 2 - rect.left,
      y: (e.clientY + other.y) / 2 - rect.top,
    };
    const zoomed = zoomTo(current, current.scale * (after / before), midBefore);
    this.applyView({
      ...zoomed,
      x: zoomed.x + midAfter.x - midBefore.x,
      y: zoomed.y + midAfter.y - midBefore.y,
    });
  }

  protected onPointerUp(e: PointerEvent): void {
    this.pointers.delete(e.pointerId);
    if (this.pointers.size > 0) return;
    const wasClick = this.press && !this.press.moved && this.press.onBackdrop;
    this.press = null;
    if (wasClick && e.type === 'pointerup') this.close()();
  }

  protected onDoubleClick(e: MouseEvent): void {
    if (e.target === e.currentTarget) return;
    if (this.fitted()) this.actualSize();
    else this.fit();
  }

  protected handleTransparent(checked: boolean): void {
    rememberedTransparent = checked;
    this.transparent.set(checked);
  }

  protected handleCopyPng(): void {
    const sheet = this.sheetRef().nativeElement;
    const live = sheet.querySelector('svg');
    if (!live || this.copyState() === 'working') return;
    this.copyState.set('working');
    const background =
      getComputedStyle(sheet).getPropertyValue('--qt-diagram-bg').trim() ||
      getComputedStyle(this.dialogRef().nativeElement).getPropertyValue('--color-card').trim() ||
      '#ffffff';
    const size = this.naturalNow ?? naturalSvgSize(live);
    const scale = safeExportScale(size, PNG_EXPORT_SCALE);

    // Started now, inside the click: the clipboard is claimed before the
    // drawing is painted (see copyPngBlobToClipboard).
    const png = this.png.toPngBlob(live, { transparent: this.transparent(), background, scale });
    this.png
      .copyPng(png)
      .then(() => {
        this.copyState.set('copied');
        if (scale < PNG_EXPORT_SCALE) {
          this.toasts.showInfo(
            `A large drawing — copied at ${scale.toFixed(1)}× rather than ${PNG_EXPORT_SCALE}×.`,
          );
        }
        this.copiedTimer = setTimeout(() => {
          if (this.copyState() === 'copied') this.copyState.set('idle');
        }, 2000);
      })
      .catch((err: unknown) => {
        console.error('Failed to copy diagram as PNG', {
          error: err instanceof Error ? err.message : String(err),
        });
        this.toasts.showError(failureMessage(err));
        this.copyState.set('idle');
      });
  }
}
