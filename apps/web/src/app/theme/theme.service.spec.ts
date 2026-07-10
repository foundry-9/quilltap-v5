import { TestBed } from '@angular/core/testing';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import { CoreClient } from '../core/core-client';
import type { CoreResponse } from '../core/core-contract';
import { ThemeService } from './theme.service';

function reset() {
  localStorage.clear();
  const root = document.documentElement;
  delete root.dataset['theme'];
  delete root.dataset['colorMode'];
  root.classList.remove('dark', 'light');
  document.getElementById('qt-theme-styles')?.remove();
  document.getElementById('qt-theme-fonts')?.remove();
}

/** A no-op CoreClient stub (the service persists best-effort, fire-and-forget). */
function stubClient(): Partial<CoreClient> {
  return { dispatch: vi.fn(async (): Promise<CoreResponse> => ({ type: 'ack', data: {} })) };
}

/** Build a fresh ThemeService in a TestBed injection context. */
function make(client: Partial<CoreClient> = stubClient()): ThemeService {
  TestBed.resetTestingModule();
  TestBed.configureTestingModule({
    providers: [ThemeService, { provide: CoreClient, useValue: client }],
  });
  return TestBed.inject(ThemeService);
}

describe('ThemeService', () => {
  beforeEach(reset);
  afterEach(reset);

  it('lists the default option plus the six bundled packs', () => {
    const svc = make();
    const ids = svc.listThemes().map((t) => t.id);
    expect(ids[0]).toBeNull(); // Default first
    expect(ids).toContain('art-deco');
    expect(ids).toContain('rains');
    expect(svc.listThemes()).toHaveLength(7);
  });

  it('applies a pack: sets data-theme, swaps the stylesheet link, injects @font-face', () => {
    const svc = make();
    svc.applyTheme('art-deco');
    expect(document.documentElement.dataset['theme']).toBe('art-deco');
    const link = document.getElementById('qt-theme-styles') as HTMLLinkElement;
    expect(link.getAttribute('href')).toBe('/themes/art-deco/styles.css');
    const fonts = document.getElementById('qt-theme-fonts') as HTMLStyleElement;
    expect(fonts.textContent).toContain('@font-face');
    expect(fonts.textContent).toContain('/themes/art-deco/fonts/Raleway-Regular.woff2');
    expect(localStorage.getItem('quilltap-theme-id')).toBe('art-deco');
  });

  it('returning to the default removes the pack stylesheet + fonts', () => {
    const svc = make();
    svc.applyTheme('rains');
    svc.applyTheme(null);
    expect(document.documentElement.dataset['theme']).toBe('default');
    expect(document.getElementById('qt-theme-styles')).toBeNull();
    expect((document.getElementById('qt-theme-fonts') as HTMLStyleElement).textContent).toBe('');
  });

  it('toggles the dark/light class from the color mode', () => {
    const svc = make();
    svc.setColorMode('dark');
    expect(document.documentElement.classList.contains('dark')).toBe(true);
    expect(document.documentElement.classList.contains('light')).toBe(false);
    svc.setColorMode('light');
    expect(document.documentElement.classList.contains('light')).toBe(true);
    expect(document.documentElement.classList.contains('dark')).toBe(false);
  });

  it("forces dark for a single-mode pack (Madman's Box) regardless of color mode", () => {
    const svc = make();
    svc.setColorMode('light');
    svc.applyTheme('madmans-box');
    expect(document.documentElement.classList.contains('dark')).toBe(true);
    expect(document.documentElement.dataset['colorMode']).toBe('dark');
  });

  it('persists a change to the server (best-effort) via chatSettingsUpdate', () => {
    const dispatch = vi.fn(async (): Promise<CoreResponse> => ({ type: 'ack', data: {} }));
    const svc = make({ dispatch });
    svc.applyTheme('rains');
    expect(dispatch).toHaveBeenCalledWith(expect.objectContaining({ type: 'chatSettingsUpdate' }));
  });

  it('loadFromServer applies the server preference to the DOM without writing back', async () => {
    const dispatch = vi.fn(async (): Promise<CoreResponse> => ({
      type: 'chatSettings',
      data: {
        avatarDisplayMode: 'ALWAYS',
        avatarDisplayStyle: 'CIRCULAR',
        themePreference: {
          activeThemeId: 'art-deco',
          colorMode: 'dark',
          showNavThemeSelector: true,
        },
      },
    }));
    const svc = make({ dispatch });
    await svc.loadFromServer();
    expect(svc.activeThemeId()).toBe('art-deco');
    expect(svc.colorMode()).toBe('dark');
    expect(svc.showNavThemeSelector()).toBe(true);
    // The read fired, but no chatSettingsUpdate write followed.
    expect(dispatch).not.toHaveBeenCalledWith(
      expect.objectContaining({ type: 'chatSettingsUpdate' }),
    );
  });
});
