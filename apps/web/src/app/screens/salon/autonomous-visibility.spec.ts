import { afterEach, describe, expect, it, vi } from 'vitest';

import {
  AUTONOMOUS_TOGGLE_KEY,
  effectiveInclude,
  hasHiddenAutonomous,
  readIncludeAutonomous,
  wantsAutonomousByDefault,
  writeIncludeAutonomous,
} from './autonomous-visibility';

describe('wantsAutonomousByDefault', () => {
  it('is false for owner_only (and undefined default)', () => {
    expect(wantsAutonomousByDefault('owner_only')).toBe(false);
    expect(wantsAutonomousByDefault(undefined)).toBe(false);
  });

  it('is true for household / open', () => {
    expect(wantsAutonomousByDefault('household')).toBe(true);
    expect(wantsAutonomousByDefault('open')).toBe(true);
  });
});

describe('effectiveInclude', () => {
  it('is the visibility default OR the toggle', () => {
    // owner_only + toggle off → excluded
    expect(effectiveInclude('owner_only', false)).toBe(false);
    // owner_only + toggle on → included (the toggle wins)
    expect(effectiveInclude('owner_only', true)).toBe(true);
    // household default → included regardless of the toggle
    expect(effectiveInclude('household', false)).toBe(true);
    expect(effectiveInclude('open', false)).toBe(true);
  });
});

describe('hasHiddenAutonomous', () => {
  it('renders the hint only when excluding AND rooms exist', () => {
    expect(hasHiddenAutonomous(false, 2)).toBe(true);
    // Not excluding → no hint even with rooms.
    expect(hasHiddenAutonomous(true, 2)).toBe(false);
    // Excluding but no rooms → no hint.
    expect(hasHiddenAutonomous(false, 0)).toBe(false);
  });
});

describe('localStorage round-trip', () => {
  afterEach(() => vi.unstubAllGlobals());

  it('reads default false, and round-trips the shared quick-hide key', () => {
    const store = new Map<string, string>();
    vi.stubGlobal('localStorage', {
      getItem: (k: string) => store.get(k) ?? null,
      setItem: (k: string, v: string) => void store.set(k, v),
    });
    expect(readIncludeAutonomous()).toBe(false);
    writeIncludeAutonomous(true);
    expect(store.get(AUTONOMOUS_TOGGLE_KEY)).toBe('true');
    expect(readIncludeAutonomous()).toBe(true);
    writeIncludeAutonomous(false);
    expect(store.get(AUTONOMOUS_TOGGLE_KEY)).toBe('false');
    expect(readIncludeAutonomous()).toBe(false);
  });
});
