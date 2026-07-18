/**
 * Contrast helpers for theme previews (v4
 * `components/settings/appearance/utils/contrast.ts`).
 *
 * Computes a legible foreground from a theme's OWN background colour, so the
 * preview banner stays readable whatever the theme paints behind it (v4's
 * "Option B" banner-contrast approach).
 *
 * Transcribed rather than improved — including the parts that are arguably
 * wrong, because the point is to match v4's rendering:
 *  - the HSL branch does not compute luminance at all, it takes the LIGHTNESS
 *    percentage as an approximation (`:18-25`);
 *  - a malformed hex yields `NaN` channels, and `NaN > 0.5` is false, so it
 *    falls through to the light-on-dark pair rather than throwing (`:32-47`).
 */

/** v4 `getLuminance` (`:16-48`) — WCAG 2.0 relative luminance, with v4's quirks. */
export function getLuminance(hexColor: string): number {
  // v4 `:18-25`: for HSL, extract the lightness as an approximation.
  if (hexColor.startsWith('hsl')) {
    const match = hexColor.match(/hsl\(\s*\d+\s*,?\s*[\d.]+%?\s*,?\s*([\d.]+)%?\s*\)/);
    if (match) {
      return parseFloat(match[1]) / 100;
    }
    return 0.5; // v4 `:24` fallback
  }

  const hex = hexColor.replace('#', '');

  let r: number;
  let g: number;
  let b: number;
  if (hex.length === 3) {
    r = parseInt(hex[0] + hex[0], 16) / 255;
    g = parseInt(hex[1] + hex[1], 16) / 255;
    b = parseInt(hex[2] + hex[2], 16) / 255;
  } else {
    r = parseInt(hex.slice(0, 2), 16) / 255;
    g = parseInt(hex.slice(2, 4), 16) / 255;
    b = parseInt(hex.slice(4, 6), 16) / 255;
  }

  // Gamma correction (v4 `:43-45`).
  r = r <= 0.03928 ? r / 12.92 : Math.pow((r + 0.055) / 1.055, 2.4);
  g = g <= 0.03928 ? g / 12.92 : Math.pow((g + 0.055) / 1.055, 2.4);
  b = b <= 0.03928 ? b / 12.92 : Math.pow((b + 0.055) / 1.055, 2.4);

  return 0.2126 * r + 0.7152 * g + 0.0722 * b;
}

/** v4 `getContrastingTextColor` (`:53-56`) — the threshold is STRICTLY `> 0.5`. */
export function getContrastingTextColor(bgColor: string): string {
  return getLuminance(bgColor) > 0.5 ? '#1a1a1a' : '#ffffff';
}

/** v4 `getMutedTextColor` (`:61-64`). */
export function getMutedTextColor(bgColor: string): string {
  return getLuminance(bgColor) > 0.5 ? '#666666' : '#a0a0a0';
}
