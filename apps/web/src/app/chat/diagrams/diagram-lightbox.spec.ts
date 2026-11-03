// Transcribed case for case from v4 `__tests__/unit/components/chat/diagrams/DiagramLightbox.test.tsx`
// (236c04e35); v5-authored cases are prefixed `v5:`.
/**
 * The diagram lightbox: opens at Fit, 100% and Fit on demand, wheel zoom,
 * Escape that the Salon behind never hears, and Copy PNG with and without
 * transparency. The PNG pipeline itself is covered in export-png.spec.ts.
 */

import { TestBed, type ComponentFixture } from '@angular/core/testing';
import { afterAll, beforeAll, beforeEach, describe, expect, it, vi } from 'vitest';

import { ToastService } from '../../ui/toast.service';
import { DiagramLightbox, DIAGRAM_PNG_COPY } from './diagram-lightbox';
import { DiagramExportError } from './export-png';

const exportMock = vi.fn();
const clipboardMock = vi.fn();

const SVG = '<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 400 200"><rect width="400" height="200"></rect></svg>';

// jsdom lays nothing out; give every element the viewport's size.
const realWidth = Object.getOwnPropertyDescriptor(HTMLElement.prototype, 'clientWidth');
const realHeight = Object.getOwnPropertyDescriptor(HTMLElement.prototype, 'clientHeight');
beforeAll(() => {
  Object.defineProperty(HTMLElement.prototype, 'clientWidth', { configurable: true, get: () => 1048 });
  Object.defineProperty(HTMLElement.prototype, 'clientHeight', { configurable: true, get: () => 648 });
});
afterAll(() => {
  if (realWidth) Object.defineProperty(HTMLElement.prototype, 'clientWidth', realWidth);
  if (realHeight) Object.defineProperty(HTMLElement.prototype, 'clientHeight', realHeight);
});

function render(svg: string, label: string, onClose: () => void = vi.fn()): ComponentFixture<DiagramLightbox> {
  TestBed.configureTestingModule({
    imports: [DiagramLightbox],
    providers: [
      {
        provide: DIAGRAM_PNG_COPY,
        useValue: {
          toPngBlob: (...a: unknown[]) => exportMock(...a),
          copyPng: (...a: unknown[]) => clipboardMock(...a),
        },
      },
    ],
  });
  const fixture = TestBed.createComponent(DiagramLightbox);
  fixture.componentRef.setInput('svg', svg);
  fixture.componentRef.setInput('label', label);
  fixture.componentRef.setInput('close', onClose);
  document.body.appendChild(fixture.nativeElement);
  fixture.detectChanges();
  TestBed.tick();
  fixture.detectChanges();
  return fixture;
}

const el = (f: ComponentFixture<DiagramLightbox>) => f.nativeElement as HTMLElement;
const zoomReadout = (f: ComponentFixture<DiagramLightbox>) =>
  Array.from(el(f).querySelectorAll('[aria-label]')).find((n) => /^Zoom \d+%$/.test(n.getAttribute('aria-label') ?? ''))
    ?.textContent;
function button(f: ComponentFixture<DiagramLightbox>, name: RegExp | string): HTMLButtonElement {
  const found = Array.from(el(f).querySelectorAll<HTMLButtonElement>('button')).find((b) => {
    const accessible = (b.getAttribute('aria-label') ?? b.textContent ?? '').trim();
    return typeof name === 'string' ? accessible === name : name.test(accessible);
  });
  if (!found) throw new Error(`no button ${name}`);
  return found;
}
async function settle(f: ComponentFixture<DiagramLightbox>): Promise<void> {
  for (let i = 0; i < 3; i++) {
    await new Promise((r) => setTimeout(r, 10));
    f.detectChanges();
  }
}

describe('DiagramLightbox', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    document.body.innerHTML = '';
    exportMock.mockResolvedValue(new Blob(['png'], { type: 'image/png' }));
    clipboardMock.mockImplementation(async (png: Promise<Blob>) => {
      await png;
    });
  });

  it('opens at Fit, and switches between 100% and Fit', () => {
    const f = render(SVG, 'Diagram, enlarged');
    expect(el(f).querySelector('[role="dialog"]')?.getAttribute('aria-label')).toBe('Diagram, enlarged');
    // (1048 − 48) / 400 = 2.5
    expect(zoomReadout(f)).toBe('250%');
    expect(button(f, /Fit/).getAttribute('aria-pressed')).toBe('true');

    button(f, '100%').click();
    f.detectChanges();
    expect(zoomReadout(f)).toBe('100%');
    expect(button(f, /Fit/).getAttribute('aria-pressed')).toBe('false');

    button(f, /Fit/).click();
    f.detectChanges();
    expect(zoomReadout(f)).toBe('250%');
  });

  it('gives a drawing with no viewBox one, so zooming scales it', () => {
    const sketch = '<svg xmlns="http://www.w3.org/2000/svg" width="544" height="325"><rect width="544" height="325"></rect></svg>';
    render(sketch, 'Diagram');
    expect(document.querySelector('.qt-diagram-lightbox-sheet svg')?.getAttribute('viewBox')).toBe('0 0 544 325');
  });

  it('zooms in on wheel-up and out with the zoom-out button', () => {
    const f = render(SVG, 'Diagram');
    button(f, '100%').click();
    f.detectChanges();
    const viewport = el(f).querySelector('.qt-diagram-lightbox-viewport') as HTMLElement;
    viewport.dispatchEvent(new WheelEvent('wheel', { deltaY: -200, bubbles: true, cancelable: true }));
    f.detectChanges();
    expect(parseInt(zoomReadout(f) ?? '0', 10)).toBeGreaterThan(100);

    button(f, '100%').click();
    button(f, 'Zoom out').click();
    f.detectChanges();
    expect(zoomReadout(f)).toBe('80%');
  });

  it('closes on Escape before anything behind it hears the key', () => {
    const onClose = vi.fn();
    const behind = vi.fn();
    document.addEventListener('keydown', behind);
    render(SVG, 'Diagram', onClose);
    (document.activeElement ?? document.body).dispatchEvent(
      new KeyboardEvent('keydown', { key: 'Escape', bubbles: true }),
    );
    expect(onClose).toHaveBeenCalledTimes(1);
    expect(behind).not.toHaveBeenCalled();
    document.removeEventListener('keydown', behind);
  });

  it('copies a 2× PNG on the diagram background by default', async () => {
    const f = render(SVG, 'Diagram');
    button(f, /Copy PNG/).click();
    await settle(f);
    expect(button(f, /Copied/)).toBeTruthy();
    expect(exportMock).toHaveBeenCalledWith(
      expect.any(SVGSVGElement),
      expect.objectContaining({ transparent: false, scale: 2 }),
    );
    expect(clipboardMock).toHaveBeenCalledWith(expect.any(Promise));
  });

  it('copies with a transparent background when Transparent is ticked', async () => {
    const f = render(SVG, 'Diagram');
    const tick = el(f).querySelector<HTMLInputElement>('.qt-diagram-lightbox-check input')!;
    tick.checked = true;
    tick.dispatchEvent(new Event('change'));
    f.detectChanges();
    button(f, /Copy PNG/).click();
    await settle(f);
    expect(exportMock).toHaveBeenCalledWith(expect.any(SVGSVGElement), expect.objectContaining({ transparent: true }));
    // Ticking it previews the transparency behind the drawing.
    expect(document.querySelector('.qt-diagram-lightbox-sheet')?.getAttribute('data-transparent')).toBe('true');
    // v5: the choice lasts for the page's life — a second lightbox opens ticked.
    f.destroy();
    TestBed.resetTestingModule();
    const again = render(SVG, 'Diagram');
    expect(again.nativeElement.querySelector('.qt-diagram-lightbox-check input').checked).toBe(true);
    const untick = again.nativeElement.querySelector('.qt-diagram-lightbox-check input') as HTMLInputElement;
    untick.checked = false;
    untick.dispatchEvent(new Event('change'));
  });

  it("v5: each export failure reason raises v4's toast and console line", async () => {
    const error = vi.spyOn(console, 'error').mockImplementation(() => undefined);
    const cases: [unknown, string][] = [
      [new DiagramExportError('tainted', 'x'), 'The browser would not let this drawing be copied as a picture.'],
      [new DiagramExportError('too-large', 'x'), 'This drawing is too large to copy as a picture.'],
      [new DiagramExportError('load-failed', 'x'), 'The drawing could not be turned into a picture.'],
      [new Error('denied'), 'The picture could not be put on the clipboard.'],
    ];
    const f = render(SVG, 'Diagram');
    for (const [err, sentence] of cases) {
      clipboardMock.mockRejectedValueOnce(err);
      button(f, /Copy PNG/).click();
      await settle(f);
      expect(TestBed.inject(ToastService).toasts().at(-1)).toMatchObject({ type: 'error', message: sentence });
      expect(error).toHaveBeenLastCalledWith('Failed to copy diagram as PNG', {
        error: (err as Error).message,
      });
      expect(button(f, /Copy PNG/).disabled).toBe(false);
    }
  });

  it('v5: the toolbar carries v4’s titles and names', () => {
    const f = render(SVG, 'Diagram');
    const bar = el(f).querySelector('[role="toolbar"]');
    expect(bar?.getAttribute('aria-label')).toBe('Diagram view');
    expect(button(f, 'Zoom out').title).toBe('Zoom out (−)');
    expect(button(f, 'Zoom in').title).toBe('Zoom in (+)');
    expect(button(f, /Fit/).title).toBe('Fit the whole drawing to the window');
    expect(button(f, '100%').title).toBe('Natural size (0)');
    expect(button(f, /Copy PNG/).title).toBe('Copy as a PNG at 2× resolution');
    expect(button(f, 'Close').title).toBe('Close (Esc)');
    expect(el(f).querySelector('.qt-diagram-lightbox-check')?.getAttribute('title')).toBe('Copy without a background');
    expect(el(f).querySelector('.qt-diagram-lightbox-hint')?.textContent).toBe(
      'Wheel or pinch to zoom · drag to pan · Esc to return',
    );
  });
});
