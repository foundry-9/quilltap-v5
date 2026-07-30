import { TestBed } from '@angular/core/testing';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import { CONTENT_WIDTH_STORAGE_KEY, ContentWidthService } from './content-width.service';

type MqListener = (e: { matches: boolean }) => void;

/** A controllable matchMedia stub (jsdom has none). */
function installMatchMedia(initialMatches: boolean): {
  fire: (matches: boolean) => void;
  lastQuery: () => string | undefined;
} {
  let listener: MqListener | undefined;
  let query: string | undefined;
  const mql = {
    get matches() {
      return state.matches;
    },
    addEventListener: (_type: string, l: MqListener) => {
      listener = l;
    },
    removeEventListener: () => undefined,
  };
  const state = { matches: initialMatches };
  (window as unknown as { matchMedia: (q: string) => typeof mql }).matchMedia = (q: string) => {
    query = q;
    return mql;
  };
  return {
    fire: (matches: boolean) => {
      state.matches = matches;
      listener?.({ matches });
    },
    lastQuery: () => query,
  };
}

function reset(): void {
  localStorage.clear();
  const root = document.documentElement;
  root.removeAttribute('data-full-width');
  root.style.removeProperty('--qt-chat-message-row-max-width');
  root.style.removeProperty('--qt-page-max-width');
}

function make(): ContentWidthService {
  TestBed.resetTestingModule();
  TestBed.configureTestingModule({ providers: [ContentWidthService] });
  return TestBed.inject(ContentWidthService);
}

describe('ContentWidthService (v4 content-width-provider.tsx)', () => {
  beforeEach(reset);
  afterEach(() => {
    reset();
    vi.restoreAllMocks();
    delete (window as { matchMedia?: unknown }).matchMedia;
  });

  it('gates on the exact v4 media query (min-width: 1000px)', () => {
    const mq = installMatchMedia(true);
    make();
    expect(mq.lastQuery()).toBe('(min-width: 1000px)');
  });

  it('defaults narrow and applies the v4 narrow values (900px / 75rem)', () => {
    installMatchMedia(true);
    const svc = make();
    expect(svc.isWide()).toBe(false);
    const root = document.documentElement;
    expect(root.style.getPropertyValue('--qt-chat-message-row-max-width')).toBe('900px');
    expect(root.style.getPropertyValue('--qt-page-max-width')).toBe('75rem');
    expect(root.hasAttribute('data-full-width')).toBe(false);
  });

  it('reads the stored preference on v4\'s exact key, "true" only (v4 :42-57)', () => {
    installMatchMedia(true);
    localStorage.setItem(CONTENT_WIDTH_STORAGE_KEY, 'true');
    expect(make().isWide()).toBe(true);

    localStorage.setItem(CONTENT_WIDTH_STORAGE_KEY, 'TRUE');
    expect(make().isWide()).toBe(false);
  });

  it('toggle persists and applies wide values + data-full-width (v4 :99-116)', () => {
    installMatchMedia(true);
    const svc = make();
    svc.toggleWidth();
    expect(localStorage.getItem(CONTENT_WIDTH_STORAGE_KEY)).toBe('true');
    const root = document.documentElement;
    expect(root.style.getPropertyValue('--qt-chat-message-row-max-width')).toBe('100%');
    expect(root.style.getPropertyValue('--qt-page-max-width')).toBe('100%');
    expect(root.getAttribute('data-full-width')).toBe('true');
    svc.toggleWidth();
    expect(localStorage.getItem(CONTENT_WIDTH_STORAGE_KEY)).toBe('false');
    expect(root.hasAttribute('data-full-width')).toBe(false);
  });

  it('wide preference on a narrow viewport applies narrow (shouldApplyWide = isWide && canApplyWide)', () => {
    const mq = installMatchMedia(false);
    localStorage.setItem(CONTENT_WIDTH_STORAGE_KEY, 'true');
    const svc = make();
    expect(svc.isWide()).toBe(true);
    expect(svc.canApplyWide()).toBe(false);
    expect(document.documentElement.hasAttribute('data-full-width')).toBe(false);
    // Viewport grows past 1000px → wide applies without touching the preference.
    mq.fire(true);
    expect(document.documentElement.getAttribute('data-full-width')).toBe('true');
    expect(localStorage.getItem(CONTENT_WIDTH_STORAGE_KEY)).toBe('true');
  });

  it('cross-tab storage events update state; other keys and removals are ignored (v4 :72-81)', () => {
    installMatchMedia(true);
    const svc = make();
    window.dispatchEvent(
      new StorageEvent('storage', { key: CONTENT_WIDTH_STORAGE_KEY, newValue: 'true' }),
    );
    expect(svc.isWide()).toBe(true);
    window.dispatchEvent(new StorageEvent('storage', { key: 'other-key', newValue: 'false' }));
    expect(svc.isWide()).toBe(true);
    window.dispatchEvent(
      new StorageEvent('storage', { key: CONTENT_WIDTH_STORAGE_KEY, newValue: null }),
    );
    expect(svc.isWide()).toBe(true);
    window.dispatchEvent(
      new StorageEvent('storage', { key: CONTENT_WIDTH_STORAGE_KEY, newValue: 'false' }),
    );
    expect(svc.isWide()).toBe(false);
  });

  it('survives a missing matchMedia (canApplyWide stays false)', () => {
    const svc = make();
    expect(svc.canApplyWide()).toBe(false);
    svc.toggleWidth();
    expect(document.documentElement.hasAttribute('data-full-width')).toBe(false);
  });
});
