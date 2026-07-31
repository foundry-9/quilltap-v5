import { afterEach, describe, expect, it } from 'vitest';

import { readTerminalTheme } from './terminal';

/**
 * v4 `components/terminal/Terminal.tsx` → `getTerminalTheme()` at the xterm-6
 * parity commit `77ff4e2e`. The two tiers are the whole point: a required knob
 * falls back to `#000000`, an optional one must stay `undefined` so xterm derives
 * it from the themed background/foreground.
 */

const OPTIONAL_KNOBS: ReadonlyArray<readonly [string, string]> = [
  ['cursorAccent', '--qt-terminal-cursor-accent'],
  ['selectionForeground', '--qt-terminal-selection-fg'],
  ['selectionInactiveBackground', '--qt-terminal-selection-inactive'],
  ['scrollbarSliderBackground', '--qt-terminal-scrollbar'],
  ['scrollbarSliderHoverBackground', '--qt-terminal-scrollbar-hover'],
  ['scrollbarSliderActiveBackground', '--qt-terminal-scrollbar-active'],
];

afterEach(() => {
  for (const [, variable] of OPTIONAL_KNOBS) {
    document.documentElement.style.removeProperty(variable);
  }
  document.documentElement.style.removeProperty('--qt-terminal-selection');
});

describe('readTerminalTheme (v4 getTerminalTheme @ 77ff4e2e)', () => {
  it('leaves every optional knob undefined when the theme does not set it', () => {
    const theme = readTerminalTheme() as Record<string, string | undefined>;

    for (const [key] of OPTIONAL_KNOBS) {
      // NOT '#000000': the required-tier fallback would paint scrollbars and the
      // cursor accent hard black instead of letting xterm derive them.
      expect(theme[key], key).toBeUndefined();
      expect(key in theme, `${key} must be present as an explicit undefined`).toBe(true);
    }
  });

  it('reads each optional knob through when the theme sets it', () => {
    for (const [, variable] of OPTIONAL_KNOBS) {
      document.documentElement.style.setProperty(variable, 'rgb(1, 2, 3)');
    }

    const theme = readTerminalTheme() as Record<string, string | undefined>;

    for (const [key] of OPTIONAL_KNOBS) {
      expect(theme[key], key).toBe('rgb(1, 2, 3)');
    }
  });

  it('falls back to #000000 for a required knob the theme does not set', () => {
    const theme = readTerminalTheme();

    expect(theme.background).toBe('#000000');
    expect(theme.foreground).toBe('#000000');
    expect(theme.brightWhite).toBe('#000000');
  });

  it('carries the selection color on xterm 6’s selectionBackground key', () => {
    // NEWLY LIVE in v4 at 77ff4e2e: xterm 6 renamed `selection` to
    // `selectionBackground`, and v4's untyped return let the dead key survive.
    // v5 was already on the new key; this pins it.
    document.documentElement.style.setProperty('--qt-terminal-selection', 'rgba(9, 9, 9, 0.5)');

    const theme = readTerminalTheme() as Record<string, string | undefined>;

    expect(theme['selectionBackground']).toBe('rgba(9, 9, 9, 0.5)');
    expect('selection' in theme).toBe(false);
  });
});
