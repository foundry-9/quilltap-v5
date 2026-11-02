/**
 * WCAG 2.x contrast, computed for real: sRGB → relative luminance →
 * `(L1 + 0.05) / (L2 + 0.05)`, the lighter colour on top (P4.D357).
 *
 * This is deliberately NOT `screens/settings/appearance/contrast.ts`. That
 * file's `getLuminance` returns an `hsl()` colour's LIGHTNESS as its
 * luminance — v4's own shortcut for the preview banner, pinned by
 * `theme-preview.spec.ts` — and would make a contrast test wrong by
 * construction. The token-contrast spec (`theme-contrast.spec.ts`) reads the
 * bundled packs' `tokens.json` and the default palette through this helper,
 * against v4's release-checklist-13 pair list.
 */

/** A colour in sRGB, each channel 0–1. */
export interface Rgb {
  r: number;
  g: number;
  b: number;
}

/** An sRGB colour with its alpha, 0–1. */
export interface Rgba extends Rgb {
  a: number;
}

const HSL_SPACE = /^hsla?\(\s*([-+]?[\d.]+)(?:deg)?\s+([\d.]+)%\s+([\d.]+)%\s*(?:\/\s*([\d.]+)(%?)\s*)?\)$/i;
const HSL_COMMA = /^hsla?\(\s*([-+]?[\d.]+)(?:deg)?\s*,\s*([\d.]+)%\s*,\s*([\d.]+)%\s*(?:,\s*([\d.]+)(%?)\s*)?\)$/i;
const HEX = /^#([0-9a-f]{3}|[0-9a-f]{6})$/i;

/**
 * Parse the colour forms the bundled tokens use: space-separated
 * `hsl(H S% L%)` with an optional `/ A`, the legacy comma form (`hsl` /
 * `hsla`), `#rrggbb` / `#rgb`, and `transparent` (alpha 0). Anything else is
 * `null` — the caller decides what an unparseable token means.
 */
export function parseCssColor(value: string): Rgba | null {
  const v = value.trim();
  if (v.toLowerCase() === 'transparent') return { r: 0, g: 0, b: 0, a: 0 };
  const hsl = HSL_SPACE.exec(v) ?? HSL_COMMA.exec(v);
  if (hsl) {
    const [, h, s, l, a, pct] = hsl;
    const alpha = a === undefined ? 1 : Number(a) / (pct ? 100 : 1);
    return { ...hslToRgb(Number(h), Number(s) / 100, Number(l) / 100), a: clamp01(alpha) };
  }
  const hex = HEX.exec(v);
  if (hex) {
    const digits = hex[1].length === 3 ? [...hex[1]].map((c) => c + c).join('') : hex[1];
    return {
      r: parseInt(digits.slice(0, 2), 16) / 255,
      g: parseInt(digits.slice(2, 4), 16) / 255,
      b: parseInt(digits.slice(4, 6), 16) / 255,
      a: 1,
    };
  }
  return null;
}

/** CSS Color 4's `hsl()` → sRGB. */
function hslToRgb(h: number, s: number, l: number): Rgb {
  const hue = ((h % 360) + 360) % 360;
  const k = (n: number) => (n + hue / 30) % 12;
  const a = s * Math.min(l, 1 - l);
  const f = (n: number) => l - a * Math.max(-1, Math.min(k(n) - 3, 9 - k(n), 1));
  return { r: f(0), g: f(8), b: f(4) };
}

function clamp01(n: number): number {
  return Math.max(0, Math.min(1, n));
}

/** Source-over compositing of `fg` (with its alpha) onto an opaque `bg`. */
export function compositeOver(fg: Rgba, bg: Rgb): Rgb {
  return {
    r: fg.r * fg.a + bg.r * (1 - fg.a),
    g: fg.g * fg.a + bg.g * (1 - fg.a),
    b: fg.b * fg.a + bg.b * (1 - fg.a),
  };
}

/** WCAG 2.x relative luminance of an sRGB colour. */
export function relativeLuminance({ r, g, b }: Rgb): number {
  const lin = (c: number) => (c <= 0.04045 ? c / 12.92 : ((c + 0.055) / 1.055) ** 2.4);
  return 0.2126 * lin(r) + 0.7152 * lin(g) + 0.0722 * lin(b);
}

/** WCAG 2.x contrast ratio, 1–21; the order of the two colours does not matter. */
export function contrastRatio(fg: Rgb, bg: Rgb): number {
  const a = relativeLuminance(fg);
  const b = relativeLuminance(bg);
  return (Math.max(a, b) + 0.05) / (Math.min(a, b) + 0.05);
}

/** A theme mode's colour tokens, keyed by `tokens.json`'s camelCase names. */
export type ThemeColors = Readonly<Record<string, string>>;

/**
 * v4's release-checklist-13 token pairs (`.claude/commands/release-checklist-13-ui.md`
 * step 1 at `236c04e35`), as `[fg, bg, minimum]`:
 *
 * - every `X` / `XForeground` pair (`primary`, `secondary`, `muted`, `accent`,
 *   `destructive`, `card`, `popover`, `success`, `warning`, `info`, `chatUser`)
 *   — ≥ 4.5:1 (SC 1.4.3);
 * - `foreground` and `mutedForeground` on `background`, `card`, `muted` and
 *   `popover`;
 * - `border` / `input` against `background` and `card`, and `ring` against
 *   everything a focused control can sit on — ≥ 3:1 (SC 1.4.11, 2.4.7);
 * - status colours used as text (`destructive`, `success`, `warning`, `info`
 *   on `background`).
 *
 * `mutedForeground on muted` sits in both the first and second bullets; it is
 * one pair and is measured once — 29 distinct pairs per theme mode.
 */
export const CHECKLIST_PAIRS: readonly (readonly [string, string, number])[] = dedupe([
  ...['primary', 'secondary', 'muted', 'accent', 'destructive', 'card', 'popover', 'success', 'warning', 'info', 'chatUser'].map(
    (x) => [`${x}Foreground`, x, 4.5] as const,
  ),
  ...['foreground', 'mutedForeground'].flatMap((fg) =>
    ['background', 'card', 'muted', 'popover'].map((bg) => [fg, bg, 4.5] as const),
  ),
  ...['border', 'input'].flatMap((fg) => ['background', 'card'].map((bg) => [fg, bg, 3] as const)),
  ...['background', 'card', 'input'].map((bg) => ['ring', bg, 3] as const),
  ...['destructive', 'success', 'warning', 'info'].map((fg) => [fg, 'background', 4.5] as const),
]);

function dedupe<T extends readonly [string, string, number]>(pairs: T[]): T[] {
  const seen = new Set<string>();
  return pairs.filter(([fg, bg]) => !seen.has(`${fg} on ${bg}`) && !!seen.add(`${fg} on ${bg}`));
}

/** One pair's outcome. `measured` is `null` when the pair cannot be computed. */
export interface PairResult {
  pair: string;
  min: number;
  measured: number | null;
  /** Why `measured` is `null`: a `transparent` background, or a value that does not parse. */
  reason?: 'NOT_A_FILL' | 'UNPARSEABLE';
}

/**
 * Every checklist pair for one theme mode. A colour with alpha < 1 is
 * composited before the ratio — the pair's background over the theme's
 * `background`, then the foreground over that — since WCAG's ratio is defined
 * on the rendered colours. A fully `transparent` background is NOT_A_FILL
 * (Earl Grey's user bubble sits on the page by design): recorded, not computed.
 */
export function evaluatePairs(colors: ThemeColors): PairResult[] {
  const page = parseCssColor(colors['background'] ?? '');
  return CHECKLIST_PAIRS.map(([fgKey, bgKey, min]) => {
    const pair = `${fgKey} on ${bgKey}`;
    const fg = parseCssColor(colors[fgKey] ?? '');
    const bg = parseCssColor(colors[bgKey] ?? '');
    if (!fg || !bg || !page) return { pair, min, measured: null, reason: 'UNPARSEABLE' as const };
    if (bg.a === 0) return { pair, min, measured: null, reason: 'NOT_A_FILL' as const };
    const bgEff = compositeOver(bg, page);
    const fgEff = compositeOver(fg, bgEff);
    return { pair, min, measured: contrastRatio(fgEff, bgEff) };
  });
}

/**
 * The colour tokens of one `--theme-*` block of the default stylesheet
 * (`src/styles.css`'s `:root` / `.dark`), renamed to `tokens.json`'s keys:
 * `--theme-muted-foreground` → `mutedForeground`, `--theme-chat-user` →
 * `chatUser`. Sidebar, radius and font tokens are carried but no pair reads them.
 */
export function cssBlockColors(css: string, selector: ':root' | '.dark'): ThemeColors {
  const start = css.indexOf(`\n${selector} {`);
  if (start < 0) throw new Error(`no ${selector} block`);
  const body = css.slice(start, css.indexOf('\n}', start));
  const out: Record<string, string> = {};
  for (const m of body.matchAll(/^\s*--theme-([a-z-]+):\s*([^;]+);/gm)) {
    out[m[1].replace(/-([a-z])/g, (_, c: string) => c.toUpperCase())] = m[2].trim();
  }
  return out;
}
