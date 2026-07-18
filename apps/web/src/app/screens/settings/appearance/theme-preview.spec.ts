import { ComponentFixture, TestBed } from '@angular/core/testing';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import type { ThemeOption } from '../../../theme/theme.service';
import { getContrastingTextColor, getLuminance, getMutedTextColor } from './contrast';
import { generateScopedThemeCSS, type ThemeTokens } from './scoped-theme-css';
import { ThemePreviewModal } from './theme-preview-modal';

/**
 * The theme-preview tier (work order P4.9d tier 2). The contrast maths and the
 * scoped-CSS emission are the parts a port silently gets wrong, so both are
 * pinned against the v4 sources they were transcribed from.
 */

describe('contrast (v4 components/settings/appearance/utils/contrast.ts)', () => {
  it('computes WCAG luminance for 6-digit hex (v4 :36-47)', () => {
    expect(getLuminance('#ffffff')).toBeCloseTo(1, 10);
    expect(getLuminance('#000000')).toBeCloseTo(0, 10);
    // Mid grey: gamma-corrected, not linear.
    expect(getLuminance('#808080')).toBeCloseTo(0.21586, 4);
  });

  it('expands 3-digit hex (v4 :32-35)', () => {
    expect(getLuminance('#fff')).toBeCloseTo(getLuminance('#ffffff'), 10);
    expect(getLuminance('#000')).toBeCloseTo(getLuminance('#000000'), 10);
  });

  it('tolerates a missing leading # (v4 :28)', () => {
    expect(getLuminance('ffffff')).toBeCloseTo(1, 10);
  });

  it('approximates HSL by its LIGHTNESS, not real luminance (v4 :18-25)', () => {
    // This is v4's documented shortcut; a "correct" port would diverge visibly.
    expect(getLuminance('hsl(225 14% 97%)')).toBeCloseTo(0.97, 10);
    expect(getLuminance('hsl(225, 14%, 30%)')).toBeCloseTo(0.3, 10);
  });

  it('falls back to 0.5 for an unparseable hsl() (v4 :24)', () => {
    expect(getLuminance('hsl(garbage)')).toBe(0.5);
  });

  it('picks foreground on a STRICT > 0.5 threshold (v4 :53-56)', () => {
    expect(getContrastingTextColor('#ffffff')).toBe('#1a1a1a');
    expect(getContrastingTextColor('#000000')).toBe('#ffffff');
    // Exactly 0.5 is NOT greater than 0.5 → the dark-background pair.
    expect(getContrastingTextColor('hsl(0 0% 50%)')).toBe('#ffffff');
  });

  it('picks the muted pair on the same threshold (v4 :61-64)', () => {
    expect(getMutedTextColor('#ffffff')).toBe('#666666');
    expect(getMutedTextColor('#000000')).toBe('#a0a0a0');
  });

  it('does not throw on malformed hex — NaN falls to the dark pair (v4 :32-47)', () => {
    expect(() => getLuminance('#zzzzzz')).not.toThrow();
    expect(getContrastingTextColor('#zzzzzz')).toBe('#ffffff');
  });
});

describe('generateScopedThemeCSS (v4 lib/themes/utils.ts:651-784)', () => {
  const tokens: ThemeTokens = {
    colors: {
      light: { background: '#ffffff', foreground: '#111111', primary: '#3355ff' },
      dark: { background: '#111111', foreground: '#eeeeee', primary: '#88aaff' },
    },
    typography: { fontSans: 'Inter, sans-serif' },
    spacing: { radiusMd: '0.5rem' },
    effects: { shadowMd: '0 1px 2px black' },
  };

  it('scopes every declaration under the given class', () => {
    const css = generateScopedThemeCSS(tokens, 'theme-preview-1', 'light');
    expect(css.startsWith('.theme-preview-1 {')).toBe(true);
    // Nothing may leak to :root / html — that would repaint the live app.
    expect(css).not.toContain(':root');
    expect(css).not.toContain('html');
  });

  it('emits the requested mode’s palette (v4 :657)', () => {
    expect(generateScopedThemeCSS(tokens, 's', 'light')).toContain('--theme-background: #ffffff;');
    expect(generateScopedThemeCSS(tokens, 's', 'dark')).toContain('--theme-background: #111111;');
  });

  it('maps theme vars onto the --color-* names Tailwind reads (v4 :741-759)', () => {
    const css = generateScopedThemeCSS(tokens, 's', 'light');
    expect(css).toContain('--color-background: var(--theme-background);');
    expect(css).toContain('--color-primary: var(--theme-primary);');
  });

  it('RE-DECLARES the component variables so qt-* classes repaint (v4 :726-713)', () => {
    // Without these the preview's buttons keep the LIVE theme's colours —
    // custom properties don't re-resolve on their own.
    const css = generateScopedThemeCSS(tokens, 's', 'light');
    expect(css).toContain('--qt-button-primary-bg: var(--color-primary);');
    expect(css).toContain('--qt-card-bg: var(--color-card);');
    expect(css).toContain('--qt-input-bg: var(--color-background);');
  });

  it('swaps the badge palette per mode (v4 :715-728)', () => {
    expect(generateScopedThemeCSS(tokens, 's', 'dark')).toContain(
      '--qt-badge-character-fg: hsl(217 91% 70%);',
    );
    expect(generateScopedThemeCSS(tokens, 's', 'light')).toContain(
      '--qt-badge-character-fg: hsl(217 91% 45%);',
    );
  });

  it('emits the typography / spacing / effects sections (v4 :770-778)', () => {
    const css = generateScopedThemeCSS(tokens, 's', 'light');
    expect(css).toContain('--theme-font-sans: Inter, sans-serif;');
    expect(css).toContain('--theme-radius-md: 0.5rem;');
    expect(css).toContain('--theme-shadow-md: 0 1px 2px black;');
  });

  it('skips absent token keys rather than emitting empty values (v4 :136-141)', () => {
    const css = generateScopedThemeCSS(
      { colors: { light: { background: '#fff' }, dark: {} } },
      's',
      'light',
    );
    expect(css).toContain('--theme-background: #fff;');
    expect(css).not.toContain('--theme-foreground:');
  });

  it('paints the container itself (v4 :781-782)', () => {
    const css = generateScopedThemeCSS(tokens, 's', 'light');
    expect(css).toContain('background-color: var(--theme-background);');
    expect(css).toContain('color: var(--theme-foreground);');
  });
});

// ---------------------------------------------------------------------------

function themeOption(over: Partial<ThemeOption>): ThemeOption {
  return {
    id: 'art-deco',
    name: 'Art Deco',
    description: 'A deco theme.',
    supportsDarkMode: true,
    previewColors: { background: '#fff', primary: '#000', secondary: '#ccc' },
    ...over,
  };
}

const TOKENS: ThemeTokens = {
  colors: {
    light: { background: '#ffffff', foreground: '#111111' },
    dark: { background: '#111111', foreground: '#eeeeee' },
  },
};

async function render(opt: ThemeOption, isActive = false): Promise<ComponentFixture<ThemePreviewModal>> {
  TestBed.configureTestingModule({ imports: [ThemePreviewModal] });
  const fixture = TestBed.createComponent(ThemePreviewModal);
  fixture.componentRef.setInput('theme', opt);
  fixture.componentRef.setInput('isActive', isActive);
  fixture.detectChanges();
  for (let i = 0; i < 6; i++) {
    await new Promise((r) => setTimeout(r, 0));
    fixture.detectChanges();
  }
  return fixture;
}

describe('ThemePreviewModal', () => {
  beforeEach(() => {
    TestBed.resetTestingModule();
    vi.stubGlobal(
      'fetch',
      vi.fn(async (url: string) =>
        url.includes('tokens.json')
          ? { ok: true, json: async () => TOKENS }
          : { ok: false, status: 404 },
      ),
    );
  });

  afterEach(() => {
    vi.unstubAllGlobals();
    document.querySelectorAll('[data-qt-theme-preview]').forEach((el) => el.remove());
  });

  it('loads the pack’s tokens.json rather than a server verb', async () => {
    await render(themeOption({ id: 'art-deco' }));
    expect(fetch).toHaveBeenCalledWith('/themes/art-deco/tokens.json');
  });

  it('paints the banner from the theme’s own background with a contrast foreground', async () => {
    const fixture = await render(themeOption({}));
    const banner = (fixture.nativeElement as HTMLElement).querySelector(
      '[style*="background"]',
    ) as HTMLElement;
    // Light background → the dark foreground of the pair.
    expect(banner.style.color).toBe('rgb(26, 26, 26)');
  });

  it('injects its CSS scoped to a unique class, never to :root', async () => {
    await render(themeOption({}));
    const styleEl = document.querySelector('[data-qt-theme-preview]');
    expect(styleEl).not.toBeNull();
    expect(styleEl?.textContent).toContain('.theme-preview-');
    expect(styleEl?.textContent).not.toContain(':root');
  });

  it('removes its style element on destroy, leaving the document clean', async () => {
    const fixture = await render(themeOption({}));
    expect(document.querySelector('[data-qt-theme-preview]')).not.toBeNull();
    fixture.destroy();
    expect(document.querySelector('[data-qt-theme-preview]')).toBeNull();
  });

  it('offers the Light/Dark toggle and repaints on switch (v4 :135-165)', async () => {
    const fixture = await render(themeOption({ supportsDarkMode: true }));
    const buttons = Array.from(
      (fixture.nativeElement as HTMLElement).querySelectorAll('button'),
    ) as HTMLButtonElement[];
    const dark = buttons.find((b) => b.textContent?.trim() === 'Dark')!;
    expect(dark).toBeDefined();

    dark.click();
    fixture.detectChanges();
    await new Promise((r) => setTimeout(r, 0));
    fixture.detectChanges();

    expect(document.querySelector('[data-qt-theme-preview]')?.textContent).toContain(
      '--theme-background: #111111;',
    );
  });

  it('a single-mode pack shows no toggle and renders in v5’s forced mode', async () => {
    // ⚠ v4 forces LIGHT here; v5's ThemeService forces DARK for the one pack
    // with supportsDarkMode:false, and this port follows v5 so the preview
    // matches what applying actually does. See the component docstring.
    const fixture = await render(themeOption({ supportsDarkMode: false }));
    const labels = Array.from(
      (fixture.nativeElement as HTMLElement).querySelectorAll('button'),
    ).map((b) => b.textContent?.trim());
    expect(labels).not.toContain('Light');
    expect(labels).not.toContain('Dark');
    expect(document.querySelector('[data-qt-theme-preview]')?.textContent).toContain(
      '--theme-background: #111111;',
    );
  });

  it('offers Apply when inactive and an Active chip when active (v4 :196-208)', async () => {
    const inactive = await render(themeOption({}), false);
    expect(inactive.nativeElement.textContent).toContain('Apply');

    TestBed.resetTestingModule();
    const active = await render(themeOption({}), true);
    expect(active.nativeElement.textContent).toContain('Active');
    expect(active.nativeElement.textContent).not.toContain('Apply');
  });

  it('refuses the built-in default loudly — it has no token bundle', async () => {
    const fixture = await render(themeOption({ id: null, name: 'Default' }));
    expect(fixture.nativeElement.textContent).toContain(
      'The built-in theme has no bundled tokens to preview.',
    );
  });

  it('surfaces a failed token fetch (v4 :223-224)', async () => {
    vi.stubGlobal(
      'fetch',
      vi.fn(async () => ({ ok: false, status: 404 })),
    );
    const fixture = await render(themeOption({}));
    expect(fixture.nativeElement.textContent).toContain('Failed to load theme preview');
  });
});
