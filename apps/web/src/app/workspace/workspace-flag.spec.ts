/**
 * isWorkspaceTabsEnabled — the localStorage feature flag (v4 feature-flags.ts).
 */

import { beforeEach, describe, expect, it } from 'vitest';

import {
  isWorkspaceTabsEnabled,
  resetWorkspaceTabsFlagCache,
  WORKSPACE_TABS_KEY,
} from './workspace-flag';

describe('isWorkspaceTabsEnabled', () => {
  beforeEach(() => {
    localStorage.clear();
    resetWorkspaceTabsFlagCache();
  });

  it('defaults ON when the key is unset', () => {
    expect(isWorkspaceTabsEnabled()).toBe(true);
  });

  it('is OFF only for the exact opt-out value "0"', () => {
    localStorage.setItem(WORKSPACE_TABS_KEY, '0');
    resetWorkspaceTabsFlagCache();
    expect(isWorkspaceTabsEnabled()).toBe(false);
  });

  it('is ON for any other value', () => {
    localStorage.setItem(WORKSPACE_TABS_KEY, '1');
    resetWorkspaceTabsFlagCache();
    expect(isWorkspaceTabsEnabled()).toBe(true);
  });

  it('reads once and caches (no mid-session drift)', () => {
    localStorage.setItem(WORKSPACE_TABS_KEY, '0');
    resetWorkspaceTabsFlagCache();
    expect(isWorkspaceTabsEnabled()).toBe(false);
    localStorage.setItem(WORKSPACE_TABS_KEY, '1'); // changed after the first read
    expect(isWorkspaceTabsEnabled()).toBe(false); // still cached
  });
});
