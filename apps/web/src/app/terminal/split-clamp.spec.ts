import { describe, expect, it } from 'vitest';

import { clampDividerPosition, clampVerticalPosition } from './split-clamp';

/** Divider clamping transcribed from v4 `SplitLayout` / `RightPaneVerticalSplit`. */
describe('clampDividerPosition (v4 SplitLayout)', () => {
  it('applies only the absolute band with no container width', () => {
    expect(clampDividerPosition(50, 0)).toBe(50);
    expect(clampDividerPosition(5, 0)).toBe(20);
    expect(clampDividerPosition(95, 0)).toBe(80);
    expect(clampDividerPosition(33.4, 0)).toBe(33);
  });

  it('keeps 320px of chat and 360px of right pane at a known width', () => {
    // 1000px wide: min chat 32%, max chat 64%.
    expect(clampDividerPosition(10, 1000)).toBe(32);
    expect(clampDividerPosition(90, 1000)).toBe(64);
    expect(clampDividerPosition(50, 1000)).toBe(50);
  });

  it('never escapes the absolute [20, 80] band even in a huge container', () => {
    expect(clampDividerPosition(1, 5000)).toBe(20);
    expect(clampDividerPosition(99, 5000)).toBe(80);
  });
});

describe('clampVerticalPosition (v4 RightPaneVerticalSplit)', () => {
  it('applies only the absolute band with no container height', () => {
    expect(clampVerticalPosition(50, 0)).toBe(50);
    expect(clampVerticalPosition(5, 0)).toBe(20);
    expect(clampVerticalPosition(95, 0)).toBe(80);
  });

  it('keeps 200px in each stacked pane at a known height', () => {
    // 800px tall: min top 25%, max top 75%.
    expect(clampVerticalPosition(10, 800)).toBe(25);
    expect(clampVerticalPosition(90, 800)).toBe(75);
    expect(clampVerticalPosition(50, 800)).toBe(50);
  });
});
