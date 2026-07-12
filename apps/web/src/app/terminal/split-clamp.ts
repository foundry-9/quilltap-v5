/**
 * Divider-clamp math for the Salon split panes (v4 `SplitLayout` /
 * `RightPaneVerticalSplit`), extracted so the width/height-aware clamping is
 * unit-testable without DOM geometry.
 */

/**
 * Clamp the horizontal (chat|right-pane) divider to a percentage that keeps at
 * least 320 px of chat and 360 px of right pane, then to the absolute [20, 80]
 * band, rounded (v4 `clampDividerPosition`). With no container width yet, only
 * the absolute band applies.
 */
export function clampDividerPosition(rawPercentage: number, containerWidth: number): number {
  if (containerWidth <= 0) {
    return Math.max(20, Math.min(80, Math.round(rawPercentage)));
  }
  const minChatPercent = (320 / containerWidth) * 100;
  const maxChatPercent = 100 - (360 / containerWidth) * 100;
  const clamped = Math.max(minChatPercent, Math.min(maxChatPercent, rawPercentage));
  return Math.max(20, Math.min(80, Math.round(clamped)));
}

const MIN_PANE_PX = 200;
const ABSOLUTE_MIN_PCT = 20;
const ABSOLUTE_MAX_PCT = 80;

/**
 * Clamp the vertical (document/terminal) divider to a percentage that keeps at
 * least 200 px in each stacked pane, then to [20, 80], rounded (v4
 * `RightPaneVerticalSplit.clampPercentage`).
 */
export function clampVerticalPosition(rawPercentage: number, containerHeight: number): number {
  if (containerHeight <= 0) {
    return Math.max(ABSOLUTE_MIN_PCT, Math.min(ABSOLUTE_MAX_PCT, Math.round(rawPercentage)));
  }
  const minTopPct = (MIN_PANE_PX / containerHeight) * 100;
  const maxTopPct = 100 - (MIN_PANE_PX / containerHeight) * 100;
  const lower = Math.max(ABSOLUTE_MIN_PCT, minTopPct);
  const upper = Math.min(ABSOLUTE_MAX_PCT, maxTopPct);
  const clamped = Math.max(lower, Math.min(upper, rawPercentage));
  return Math.round(clamped);
}
