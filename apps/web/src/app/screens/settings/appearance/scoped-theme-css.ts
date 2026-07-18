/**
 * Scoped theme CSS for the preview panel (v4 `lib/themes/utils.ts:651-784`
 * `generateScopedThemeCSS`, plus the four VAR_MAPs at `:24-122`).
 *
 * The point of "scoped": a preview must repaint ONE container without touching
 * the live theme. v4 emits the theme's tokens as custom properties under a
 * unique class and puts that class on the preview div — nothing on `<html>`,
 * no stylesheet swap. v5's own {@link ThemeService.applyToDom} does the exact
 * opposite (it mutates `<html>` and swaps the single theme `<link>`), so the
 * preview cannot route through it and needs this instead.
 *
 * The re-declared component variables at the end are load-bearing, not
 * decoration: `--qt-button-primary-bg` and friends are defined elsewhere
 * against the LIVE `--color-*` values, and CSS custom properties do not
 * re-resolve just because a descendant redefines what they referenced. Without
 * re-declaring them inside the scope, `qt-button-primary` in the preview keeps
 * painting in the active theme's colours.
 *
 * DEFERRED, loudly (v4 has them; v5's bundled packs cannot feed them):
 *  - `cssOverrides` scoping (v4 `ThemePreviewPanel.tsx:40-126` — a brace-depth
 *    state machine rewriting `[data-theme="id"]` selectors). v5 ships each
 *    theme's overrides as a LINKED `styles.css`, not as text, so there is
 *    nothing to scope without fetching and re-parsing it. v4 itself degrades to
 *    token-only when `cssOverrides` is absent (`ThemePreviewPanel.tsx:162`), so
 *    this port takes that supported path.
 *  - `@font-face` emission. The active document already carries the bundled
 *    packs' faces via `ThemeService`, so the preview inherits them.
 */

/** v4 `COLOR_VAR_MAP` (`:24-55`). */
const COLOR_VAR_MAP: Record<string, string> = {
  background: '--theme-background',
  foreground: '--theme-foreground',
  primary: '--theme-primary',
  primaryForeground: '--theme-primary-foreground',
  secondary: '--theme-secondary',
  secondaryForeground: '--theme-secondary-foreground',
  muted: '--theme-muted',
  mutedForeground: '--theme-muted-foreground',
  accent: '--theme-accent',
  accentForeground: '--theme-accent-foreground',
  destructive: '--theme-destructive',
  destructiveForeground: '--theme-destructive-foreground',
  card: '--theme-card',
  cardForeground: '--theme-card-foreground',
  popover: '--theme-popover',
  popoverForeground: '--theme-popover-foreground',
  border: '--theme-border',
  input: '--theme-input',
  ring: '--theme-ring',
  success: '--theme-success',
  successForeground: '--theme-success-foreground',
  warning: '--theme-warning',
  warningForeground: '--theme-warning-foreground',
  info: '--theme-info',
  infoForeground: '--theme-info-foreground',
  chatUser: '--theme-chat-user',
  chatUserForeground: '--theme-chat-user-foreground',
};

/** v4 `TYPOGRAPHY_VAR_MAP` (`:61-83`). */
const TYPOGRAPHY_VAR_MAP: Record<string, string> = {
  fontSans: '--theme-font-sans',
  fontSerif: '--theme-font-serif',
  fontMono: '--theme-font-mono',
  fontSizeXs: '--theme-font-size-xs',
  fontSizeSm: '--theme-font-size-sm',
  fontSizeBase: '--theme-font-size-base',
  fontSizeLg: '--theme-font-size-lg',
  fontSizeXl: '--theme-font-size-xl',
  fontSize2xl: '--theme-font-size-2xl',
  fontSize3xl: '--theme-font-size-3xl',
  fontSize4xl: '--theme-font-size-4xl',
  lineHeightTight: '--theme-line-height-tight',
  lineHeightNormal: '--theme-line-height-normal',
  lineHeightRelaxed: '--theme-line-height-relaxed',
  fontWeightNormal: '--theme-font-weight-normal',
  fontWeightMedium: '--theme-font-weight-medium',
  fontWeightSemibold: '--theme-font-weight-semibold',
  fontWeightBold: '--theme-font-weight-bold',
  letterSpacingTight: '--theme-letter-spacing-tight',
  letterSpacingNormal: '--theme-letter-spacing-normal',
  letterSpacingWide: '--theme-letter-spacing-wide',
};

/** v4 `SPACING_VAR_MAP` (`:89-105`). */
const SPACING_VAR_MAP: Record<string, string> = {
  radiusSm: '--theme-radius-sm',
  radiusMd: '--theme-radius-md',
  radiusLg: '--theme-radius-lg',
  radiusXl: '--theme-radius-xl',
  radiusFull: '--theme-radius-full',
  spacing1: '--theme-spacing-1',
  spacing2: '--theme-spacing-2',
  spacing3: '--theme-spacing-3',
  spacing4: '--theme-spacing-4',
  spacing5: '--theme-spacing-5',
  spacing6: '--theme-spacing-6',
  spacing8: '--theme-spacing-8',
  spacing10: '--theme-spacing-10',
  spacing12: '--theme-spacing-12',
  spacing16: '--theme-spacing-16',
};

/** v4 `EFFECTS_VAR_MAP` (`:111-122`). */
const EFFECTS_VAR_MAP: Record<string, string> = {
  shadowSm: '--theme-shadow-sm',
  shadowMd: '--theme-shadow-md',
  shadowLg: '--theme-shadow-lg',
  shadowXl: '--theme-shadow-xl',
  transitionFast: '--theme-transition-fast',
  transitionNormal: '--theme-transition-normal',
  transitionSlow: '--theme-transition-slow',
  transitionEasing: '--theme-transition-easing',
  focusRingWidth: '--theme-focus-ring-width',
  focusRingOffset: '--theme-focus-ring-offset',
};

/** The `tokens.json` shape shipped under `public/themes/<id>/`. */
export interface ThemeTokens {
  colors: {
    light: Record<string, string>;
    dark: Record<string, string>;
  };
  typography?: Record<string, string>;
  spacing?: Record<string, string>;
  effects?: Record<string, string>;
}

/** v4's map-driven emitters (`:133-144` and siblings) — absent keys are skipped. */
function emitVars(values: Record<string, string> | undefined, map: Record<string, string>): string[] {
  if (!values) return [];
  const out: string[] = [];
  for (const [key, varName] of Object.entries(map)) {
    const value = values[key];
    if (value !== undefined) {
      out.push(`${varName}: ${value};`);
    }
  }
  return out;
}

/**
 * v4 `generateScopedThemeCSS` (`:651-784`), minus the font-face and
 * cssOverrides arms noted in the module docstring.
 */
export function generateScopedThemeCSS(
  tokens: ThemeTokens,
  scopeClass: string,
  mode: 'light' | 'dark',
): string {
  const colors = mode === 'light' ? tokens.colors.light : tokens.colors.dark;
  const colorVars = emitVars(colors, COLOR_VAR_MAP);
  const typographyVars = emitVars(tokens.typography, TYPOGRAPHY_VAR_MAP);
  const spacingVars = emitVars(tokens.spacing, SPACING_VAR_MAP);
  const effectsVars = emitVars(tokens.effects, EFFECTS_VAR_MAP);

  // v4 `:726-713` — re-declared so they resolve against the SCOPED --color-*.
  const componentVars = `
  --qt-button-primary-bg: var(--color-primary);
  --qt-button-primary-fg: var(--color-primary-foreground);
  --qt-button-primary-hover-bg: color-mix(in srgb, var(--color-primary) 90%, black);
  --qt-button-secondary-bg: var(--color-secondary);
  --qt-button-secondary-fg: var(--color-secondary-foreground);
  --qt-button-secondary-border: var(--color-border);
  --qt-button-secondary-hover-bg: color-mix(in srgb, var(--color-secondary) 80%, black);
  --qt-button-ghost-bg: transparent;
  --qt-button-ghost-fg: var(--color-foreground);
  --qt-button-ghost-hover-bg: var(--color-muted);

  --qt-input-bg: var(--color-background);
  --qt-input-fg: var(--color-foreground);
  --qt-input-border: var(--color-input);
  --qt-input-focus-ring: var(--color-ring);

  --qt-card-bg: var(--color-card);
  --qt-card-fg: var(--color-card-foreground);
  --qt-card-border: var(--color-border);

  --qt-badge-primary-bg: var(--color-primary);
  --qt-badge-primary-fg: var(--color-primary-foreground);
  --qt-badge-secondary-bg: var(--color-secondary);
  --qt-badge-secondary-fg: var(--color-secondary-foreground);

  --qt-text-secondary-fg: var(--color-muted-foreground);
  --qt-heading-font: var(--theme-font-sans, var(--font-sans));

  --qt-status-success-bg: color-mix(in srgb, var(--color-success, hsl(142 76% 36%)) 85%, transparent);
  --qt-status-success-fg: var(--color-success-foreground, hsl(142 76% 30%));
  --qt-status-success-border: var(--color-success, hsl(142 76% 36%));`;

  // v4 `:715-728` — the badge palette differs per mode.
  const modeVars =
    mode === 'dark'
      ? `
  --qt-badge-character-bg: hsl(217 91% 60% / 0.3);
  --qt-badge-character-fg: hsl(217 91% 70%);
  --qt-badge-chat-bg: hsl(142 76% 36% / 0.3);
  --qt-badge-chat-fg: hsl(142 76% 65%);
  --qt-status-success-bg: color-mix(in srgb, var(--color-success, hsl(142 76% 36%)) 30%, transparent);
  --qt-status-success-fg: hsl(142 76% 65%);`
      : `
  --qt-badge-character-bg: hsl(217 91% 60% / 0.15);
  --qt-badge-character-fg: hsl(217 91% 45%);
  --qt-badge-chat-bg: hsl(142 76% 36% / 0.15);
  --qt-badge-chat-fg: hsl(142 76% 30%);`;

  return `
.${scopeClass} {
  ${colorVars.join('\n  ')}

  --color-background: var(--theme-background);
  --color-foreground: var(--theme-foreground);
  --color-primary: var(--theme-primary);
  --color-primary-foreground: var(--theme-primary-foreground);
  --color-secondary: var(--theme-secondary);
  --color-secondary-foreground: var(--theme-secondary-foreground);
  --color-muted: var(--theme-muted);
  --color-muted-foreground: var(--theme-muted-foreground);
  --color-accent: var(--theme-accent);
  --color-accent-foreground: var(--theme-accent-foreground);
  --color-destructive: var(--theme-destructive);
  --color-destructive-foreground: var(--theme-destructive-foreground);
  --color-card: var(--theme-card);
  --color-card-foreground: var(--theme-card-foreground);
  --color-popover: var(--theme-popover);
  --color-popover-foreground: var(--theme-popover-foreground);
  --color-border: var(--theme-border);
  --color-input: var(--theme-input);
  --color-ring: var(--theme-ring);

  --radius-sm: var(--theme-radius-sm, 0.25rem);
  --radius-md: var(--theme-radius-md, 0.375rem);
  --radius-lg: var(--theme-radius-lg, 0.5rem);

  --font-sans: var(--theme-font-sans, ui-sans-serif, system-ui, sans-serif);
  --font-serif: var(--theme-font-serif, ui-serif, Georgia, serif);
  --font-mono: var(--theme-font-mono, ui-monospace, monospace);

  ${typographyVars.join('\n  ')}

  ${spacingVars.join('\n  ')}

  ${effectsVars.join('\n  ')}
${componentVars}
${modeVars}

  background-color: var(--theme-background);
  color: var(--theme-foreground);
}
`.trim();
}
