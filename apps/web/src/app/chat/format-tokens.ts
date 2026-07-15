/**
 * Token/cost display formatting — a byte-faithful port of v4
 * `lib/utils/format-tokens.ts`.
 *
 * Both functions are pure and client-safe; the thresholds and the digit counts
 * ARE the user-facing copy, so they carry over exactly. `toFixed` is the same
 * operation in JS and TS — no reimplementation, just the same rounding.
 *
 * @module chat/format-tokens
 */

/**
 * v4 `formatCostForDisplay` (`:11-29`). The banding is deliberate: sub-cent
 * costs need four digits to be legible at all, whole-dollar costs read better
 * at two.
 *
 * Note the `null` arm returns `'N/A'` while an exact `0` returns `'Free'` —
 * they are distinct states (no pricing data vs. a genuinely free call), and the
 * order of the checks is what keeps them apart.
 */
export function formatCostForDisplay(costUSD: number | null): string {
  if (costUSD === null) {
    return 'N/A';
  }

  if (costUSD === 0) {
    return 'Free';
  }

  if (costUSD < 0.01) {
    return `$${costUSD.toFixed(4)}`;
  }

  if (costUSD < 1) {
    return `$${costUSD.toFixed(3)}`;
  }

  return `$${costUSD.toFixed(2)}`;
}

/** v4 `formatTokenCount` (`:34-42`): `>=1M → "2.3M"`, `>=1K → "1.5K"`, else the raw integer. */
export function formatTokenCount(tokens: number): string {
  if (tokens >= 1_000_000) {
    return `${(tokens / 1_000_000).toFixed(1)}M`;
  }
  if (tokens >= 1_000) {
    return `${(tokens / 1_000).toFixed(1)}K`;
  }
  return tokens.toString();
}
