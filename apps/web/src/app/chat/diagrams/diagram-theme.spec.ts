// v5-authored: v4 ships `diagram-theme.ts` without a test (236c04e35).
import { afterEach, describe, expect, it, vi } from 'vitest';

import { readDiagramPalette, resetDiagramPaletteProbe } from './diagram-theme';

function host(vars: Record<string, string>): HTMLElement {
  const el = document.createElement('div');
  for (const [k, v] of Object.entries(vars)) el.style.setProperty(k, v);
  document.body.appendChild(el);
  return el;
}

describe('readDiagramPalette', () => {
  afterEach(() => {
    document.body.innerHTML = '';
    vi.restoreAllMocks();
    resetDiagramPaletteProbe();
  });

  it('v5: darkMode flips when the background luminance falls below 0.5', () => {
    const painted: Record<string, number[]> = {
      'rgb(20, 20, 30)': [20, 20, 30, 255],
      'rgb(240, 240, 240)': [240, 240, 240, 255],
    };
    let fill = '';
    const ctx = {
      clearRect: () => undefined,
      fillRect: () => undefined,
      set fillStyle(v: string) {
        fill = v;
      },
      get fillStyle() {
        return fill;
      },
      getImageData: () => ({ data: painted[fill] ?? [0, 0, 0, 0] }),
    };
    const spy = vi
      .spyOn(HTMLCanvasElement.prototype, 'getContext')
      .mockReturnValue(ctx as unknown as CanvasRenderingContext2D);
    const dark = readDiagramPalette(
      host({ '--color-background': 'rgb(20, 20, 30)', '--color-card': 'rgb(240, 240, 240)' }),
    );
    expect(dark.background).toBe('#14141e');
    expect(dark.card).toBe('#f0f0f0');
    expect(dark.darkMode).toBe(true);
    const light = readDiagramPalette(host({ '--color-background': 'rgb(240, 240, 240)' }));
    expect(light.darkMode).toBe(false);
    // An unpaintable token (alpha 0) reads as its fallback.
    expect(readDiagramPalette(host({ '--color-primary': 'nonsense' })).primary).toBe('#2a4fc4');
    spy.mockRestore();
  });

  it('v5: with no 2d context (getContext → null) every token reads as its fallback', () => {
    vi.spyOn(HTMLCanvasElement.prototype, 'getContext').mockReturnValue(null);
    const palette = readDiagramPalette(host({ '--color-background': 'rgb(20, 20, 30)' }));
    expect(palette.background).toBe('#f6f7f9');
    expect(palette.darkMode).toBe(false);
  });

  it('v5: falls back to the light palette when the tokens are empty', () => {
    const palette = readDiagramPalette(host({}));
    expect(palette).toMatchObject({
      background: '#f6f7f9',
      foreground: '#1c1f26',
      card: '#ffffff',
      primary: '#2a4fc4',
      border: '#d8dbe2',
      darkMode: false,
    });
  });

  it('v5: the two font variables default as quoted', () => {
    const palette = readDiagramPalette(host({}));
    expect(palette.fontFamily).toBe('ui-sans-serif, system-ui, sans-serif');
    expect(palette.handFontFamily).toBe(
      '"Segoe Print", "Bradley Hand", "Chalkboard SE", "Marker Felt", cursive',
    );
  });

  it('v5: the font variables are read when a theme sets them', () => {
    const palette = readDiagramPalette(
      host({ '--qt-diagram-font': 'Inter, sans-serif', '--qt-diagram-hand-font': '"Caveat", cursive' }),
    );
    expect(palette.fontFamily).toBe('Inter, sans-serif');
    expect(palette.handFontFamily).toBe('"Caveat", cursive');
  });
});
