import { Injectable, computed, inject, signal } from '@angular/core';

import { CoreClient } from '../core/core-client';
import { BUNDLED_THEMES, type ThemeDescriptor } from './bundled-themes';

export type ColorMode = 'light' | 'dark' | 'system';
export type ResolvedColorMode = 'light' | 'dark';

/** The server-persisted theme preference (v4 `chat_settings.themePreference`). */
export interface ThemePreference {
  activeThemeId: string | null;
  colorMode: ColorMode;
  showNavThemeSelector: boolean;
}

/** A theme option for the switcher: the bundled packs plus the built-in default. */
export interface ThemeOption {
  /** `null` = the built-in default (no pack); otherwise the pack id. */
  id: string | null;
  name: string;
  description: string;
  supportsDarkMode: boolean;
  previewColors: { background: string; primary: string; secondary: string };
}

const THEME_STORAGE_KEY = 'quilltap-theme-id';
const MODE_STORAGE_KEY = 'quilltap-color-mode';
const NAV_SELECTOR_STORAGE_KEY = 'quilltap-nav-theme-selector';
const THEME_LINK_ID = 'qt-theme-styles';
const THEME_FONTS_ID = 'qt-theme-fonts';

const DEFAULT_OPTION: ThemeOption = {
  id: null,
  name: 'Default',
  description: "Quilltap's built-in professional-neutral theme.",
  supportsDarkMode: true,
  previewColors: {
    background: 'hsl(225 14% 97%)',
    primary: 'hsl(225 65% 48%)',
    secondary: 'hsl(225 10% 92%)',
  },
};

/**
 * The client-side theming seam (D15). Applies a bundled theme pack's tokens by
 * toggling `data-theme` + the `.dark` / `.light` class on `<html>`, swapping in
 * the pack's stylesheet `<link>`, and injecting its `@font-face` rules; persists
 * the choice (localStorage this round). The read is shaped like the future
 * server themes service (`listThemes` → descriptors; `applyTheme` by id) so the
 * Settings vertical can plug the server in here later.
 */
@Injectable({ providedIn: 'root' })
export class ThemeService {
  private readonly core = inject(CoreClient);

  private readonly _activeThemeId = signal<string | null>(readStored(THEME_STORAGE_KEY));
  private readonly _colorMode = signal<ColorMode>(
    (readStored(MODE_STORAGE_KEY) as ColorMode) ?? 'system',
  );
  private readonly _showNavThemeSelector = signal<boolean>(
    readStored(NAV_SELECTOR_STORAGE_KEY) === 'true',
  );
  private readonly _systemPrefersDark = signal(prefersDark());

  /** The currently active pack id (`null` = default). */
  readonly activeThemeId = this._activeThemeId.asReadonly();
  /** The chosen color mode (may be `system`). */
  readonly colorMode = this._colorMode.asReadonly();
  /** Whether the nav-bar quick theme selector is shown (v4 `showNavThemeSelector`). */
  readonly showNavThemeSelector = this._showNavThemeSelector.asReadonly();
  /** The resolved light/dark mode after folding `system`. */
  readonly resolvedColorMode = computed<ResolvedColorMode>(() =>
    this._colorMode() === 'system'
      ? this._systemPrefersDark()
        ? 'dark'
        : 'light'
      : (this._colorMode() as ResolvedColorMode),
  );

  private readonly options: ThemeOption[] = [DEFAULT_OPTION, ...BUNDLED_THEMES.map(toOption)];

  constructor() {
    // Track the OS preference so `system` mode re-resolves live.
    if (typeof window !== 'undefined' && typeof window.matchMedia === 'function') {
      const mq = window.matchMedia('(prefers-color-scheme: dark)');
      const onChange = () => {
        this._systemPrefersDark.set(mq.matches);
        this.applyToDom();
      };
      mq.addEventListener?.('change', onChange);
    }
    this.applyToDom();
  }

  /** The available themes (the future `GET /api/v1/themes` read shape). */
  listThemes(): ThemeOption[] {
    return this.options;
  }

  /** The full descriptor list for the bundled packs (fonts, etc.). */
  bundledDescriptors(): ThemeDescriptor[] {
    return BUNDLED_THEMES;
  }

  /** The full server-persisted preference (v4 `chat_settings.themePreference`). */
  themePreference(): ThemePreference {
    return {
      activeThemeId: this._activeThemeId(),
      colorMode: this._colorMode(),
      showNavThemeSelector: this._showNavThemeSelector(),
    };
  }

  /** Apply a theme by id (`null` = default). Persists (local + server) + repaints. */
  applyTheme(id: string | null): void {
    this._activeThemeId.set(id);
    writeStored(THEME_STORAGE_KEY, id);
    this.applyToDom();
    this.persistToServer();
  }

  /** Set the light/dark/system color mode. Persists (local + server) + repaints. */
  setColorMode(mode: ColorMode): void {
    this._colorMode.set(mode);
    writeStored(MODE_STORAGE_KEY, mode);
    this.applyToDom();
    this.persistToServer();
  }

  /** Toggle the nav-bar quick theme selector. Persists (local + server). */
  setShowNavThemeSelector(show: boolean): void {
    this._showNavThemeSelector.set(show);
    writeStored(NAV_SELECTOR_STORAGE_KEY, show ? 'true' : 'false');
    this.persistToServer();
  }

  /**
   * Read the server's `themePreference` and apply it as the source of truth
   * (localStorage is the offline fallback). Best-effort — a locked/erroring
   * server leaves the local preference in place. Does NOT write back.
   */
  async loadFromServer(): Promise<void> {
    try {
      const resp = await this.core.dispatch({ type: 'chatSettings' });
      if (resp.type !== 'chatSettings') {
        return;
      }
      const pref = resp.data['themePreference'] as Partial<ThemePreference> | undefined;
      if (!pref) {
        return;
      }
      this.applyServerPreference(pref);
    } catch {
      // Offline / locked — keep the local preference.
    }
  }

  /** Apply a server preference locally (+ localStorage) WITHOUT persisting back. */
  private applyServerPreference(pref: Partial<ThemePreference>): void {
    if (pref.activeThemeId !== undefined) {
      this._activeThemeId.set(pref.activeThemeId);
      writeStored(THEME_STORAGE_KEY, pref.activeThemeId);
    }
    if (pref.colorMode !== undefined) {
      this._colorMode.set(pref.colorMode);
      writeStored(MODE_STORAGE_KEY, pref.colorMode);
    }
    if (pref.showNavThemeSelector !== undefined) {
      this._showNavThemeSelector.set(pref.showNavThemeSelector);
      writeStored(NAV_SELECTOR_STORAGE_KEY, pref.showNavThemeSelector ? 'true' : 'false');
    }
    this.applyToDom();
  }

  /** Fire-and-forget persist of the full preference to `chat_settings` (v4 store). */
  private persistToServer(): void {
    void this.core
      .dispatch({
        type: 'chatSettingsUpdate',
        settings: { themePreference: this.themePreference() },
      })
      .catch(() => {
        // Best-effort — localStorage already holds the value.
      });
  }

  private descriptor(id: string | null): ThemeDescriptor | null {
    return id ? (BUNDLED_THEMES.find((t) => t.id === id) ?? null) : null;
  }

  /** Repaint `<html>` + swap the pack's stylesheet + fonts. */
  private applyToDom(): void {
    if (typeof document === 'undefined') {
      return;
    }
    const root = document.documentElement;
    const id = this._activeThemeId();
    const desc = this.descriptor(id);
    // A single-mode pack (e.g. Madman's Box) is dark by decree — honor its lack
    // of a light variant by forcing dark; otherwise use the resolved mode.
    const resolved: ResolvedColorMode =
      desc && !desc.supportsDarkMode ? 'dark' : this.resolvedColorMode();

    root.dataset['theme'] = id ?? 'default';
    root.dataset['colorMode'] = resolved;
    root.classList.toggle('dark', resolved === 'dark');
    root.classList.toggle('light', resolved === 'light');

    this.applyStylesheet(id);
    this.applyFonts(desc);
  }

  /** Ensure exactly the active pack's stylesheet `<link>` is present. */
  private applyStylesheet(id: string | null): void {
    const head = document.head;
    let link = document.getElementById(THEME_LINK_ID) as HTMLLinkElement | null;
    if (!id) {
      link?.remove();
      return;
    }
    const href = `/themes/${id}/styles.css`;
    if (!link) {
      link = document.createElement('link');
      link.id = THEME_LINK_ID;
      link.rel = 'stylesheet';
      head.appendChild(link);
    }
    if (link.getAttribute('href') !== href) {
      link.setAttribute('href', href);
    }
  }

  /** Inject the active pack's `@font-face` rules (empty for the default). */
  private applyFonts(desc: ThemeDescriptor | null): void {
    let style = document.getElementById(THEME_FONTS_ID) as HTMLStyleElement | null;
    if (!style) {
      style = document.createElement('style');
      style.id = THEME_FONTS_ID;
      document.head.appendChild(style);
    }
    if (!desc) {
      style.textContent = '';
      return;
    }
    style.textContent = desc.fonts
      .map(
        (f) =>
          `@font-face{font-family:${cssFamily(f.family)};src:url("/themes/${desc.id}/${f.src}") format("woff2");` +
          `font-weight:${f.weight};font-style:${f.style};font-display:${f.display};}`,
      )
      .join('\n');
  }
}

function toOption(t: ThemeDescriptor): ThemeOption {
  return {
    id: t.id,
    name: t.name,
    description: t.description,
    supportsDarkMode: t.supportsDarkMode,
    previewColors: t.previewColors,
  };
}

function cssFamily(family: string): string {
  // Quote multi-word family names for the @font-face declaration.
  return /\s/.test(family) ? `"${family}"` : family;
}

function prefersDark(): boolean {
  return (
    typeof window !== 'undefined' &&
    typeof window.matchMedia === 'function' &&
    window.matchMedia('(prefers-color-scheme: dark)').matches
  );
}

function readStored(key: string): string | null {
  if (typeof localStorage === 'undefined') {
    return null;
  }
  try {
    return localStorage.getItem(key);
  } catch {
    return null;
  }
}

function writeStored(key: string, value: string | null): void {
  if (typeof localStorage === 'undefined') {
    return;
  }
  try {
    if (value === null) {
      localStorage.removeItem(key);
    } else {
      localStorage.setItem(key, value);
    }
  } catch {
    // storage may be unavailable (private mode); the DOM apply still works.
  }
}
