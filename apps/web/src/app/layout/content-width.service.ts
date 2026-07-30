import { DOCUMENT } from '@angular/common';
import { Injectable, computed, inject, signal } from '@angular/core';

/** v4 `content-width-provider.tsx:25-30` — the exact key + constants. */
export const CONTENT_WIDTH_STORAGE_KEY = 'quilltap.contentWidth.isWide';
const WIDE_VIEWPORT_MIN = 1000;
const NARROW_WIDTH = '900px';
const WIDE_WIDTH = '100%';
const NARROW_PAGE_WIDTH = '75rem';
const WIDE_PAGE_WIDTH = '100%';

/**
 * The content-width preference (v4 `components/providers/content-width-provider.tsx`)
 * as a root-provided signal service: narrow (900px chat rows / 75rem pages) vs
 * wide (100%), persisted on v4's exact localStorage key, gated behind a
 * `min-width: 1000px` matchMedia (wide mode is meaningless on a narrow
 * viewport), synced across tabs via the `storage` event, and applied by setting
 * `--qt-chat-message-row-max-width` + `--qt-page-max-width` on the root element
 * plus toggling `data-full-width="true"` for the CSS that keys on it
 * (`_chat.css:3377`, `_layout.css:1239/1243` — wired and waiting since P4.6ap).
 *
 * DIVERGENCES (deliberate): v4's hydrate-then-read two-phase (`storageReady`)
 * exists to dodge a React SSR hydration mismatch; v5 is a plain SPA with no
 * server render, so the localStorage read happens once at construction. And
 * where v4 applies via useEffect, v5 applies explicitly from every state
 * transition (constructor / toggle / storage event / viewport change) — same
 * observable writes, no change-detection dependence.
 *
 * CORRECTION (ordered by P4.9P): v5's static `--qt-page-max-width` default was
 * `72rem` where v4's narrow width is `75rem` — the static default in
 * `_variables.css` moved to `75rem` with this service, and the service always
 * re-applies the exact v4 values anyway.
 */
@Injectable({ providedIn: 'root' })
export class ContentWidthService {
  private readonly document = inject(DOCUMENT);

  private readonly _isWide = signal(false);
  private readonly _canApplyWide = signal(false);

  /** Whether wide mode is enabled (the stored preference). */
  readonly isWide = this._isWide.asReadonly();
  /** Whether the viewport is large enough for wide mode to apply (>= 1000px). */
  readonly canApplyWide = this._canApplyWide.asReadonly();
  /** v4: `shouldApplyWide = isWide && canApplyWide`. */
  readonly shouldApplyWide = computed(() => this._isWide() && this._canApplyWide());

  constructor() {
    // v4 :42-57 — only the literal string `'true'` enables wide mode.
    this._isWide.set(this.loadStored());

    const win = this.document.defaultView;
    if (win && typeof win.matchMedia === 'function') {
      // Viewport gate (v4 :87-96).
      const mq = win.matchMedia(`(min-width: ${WIDE_VIEWPORT_MIN}px)`);
      this._canApplyWide.set(mq.matches);
      mq.addEventListener('change', () => {
        this._canApplyWide.set(mq.matches);
        this.applyToRoot();
      });
    }
    if (win) {
      // Cross-tab sync (v4 :72-81): only this key, and a removal is ignored.
      // The storage event never fires in the tab that wrote, so no persist here.
      win.addEventListener('storage', (event: StorageEvent) => {
        if (event.key !== CONTENT_WIDTH_STORAGE_KEY || event.newValue === null) return;
        this._isWide.set(event.newValue === 'true');
        this.applyToRoot();
      });
    }

    this.applyToRoot();
  }

  /** v4 `toggleWidth`. */
  toggleWidth(): void {
    this.setWidth(!this._isWide());
  }

  /** v4 `setWidth` — persists and re-applies the root CSS state. */
  setWidth(wide: boolean): void {
    this._isWide.set(wide);
    try {
      this.document.defaultView?.localStorage.setItem(CONTENT_WIDTH_STORAGE_KEY, String(wide));
    } catch {
      // v4 warns and carries on; the preference just doesn't stick.
    }
    this.applyToRoot();
  }

  /** v4 :99-116 — the CSS variables + the `data-full-width` attribute. */
  private applyToRoot(): void {
    const root = this.document.documentElement;
    const wide = this.shouldApplyWide();
    root.style.setProperty('--qt-chat-message-row-max-width', wide ? WIDE_WIDTH : NARROW_WIDTH);
    root.style.setProperty('--qt-page-max-width', wide ? WIDE_PAGE_WIDTH : NARROW_PAGE_WIDTH);
    if (wide) {
      root.setAttribute('data-full-width', 'true');
    } else {
      root.removeAttribute('data-full-width');
    }
  }

  private loadStored(): boolean {
    try {
      return this.document.defaultView?.localStorage.getItem(CONTENT_WIDTH_STORAGE_KEY) === 'true';
    } catch {
      return false;
    }
  }
}
