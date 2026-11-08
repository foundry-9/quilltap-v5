/**
 * The compact header breadcrumb's two pure helpers (v4
 * `components/chat/ChatHeaderBreadcrumb.tsx:29-65`, `3c41cf3da`, pinned at
 * `624d0d25c`).
 *
 * A crowded chat has more names than the header has room for, so the project
 * and character links step down through three densities until the row fits:
 *
 *   full    — avatar + full name
 *   abbrev  — avatar + short abbreviation
 *   avatar  — avatar alone
 *
 * Transcribed, not imported: v4 keeps these beside its React row. Proven row for
 * row against v4's REAL module by `breadcrumb-density.oracle.spec.ts` (recorder
 * `apps/web/oracle/breadcrumb-density.recorder.ts`).
 */

export type BreadcrumbDensity = 'full' | 'abbrev' | 'avatar';

/** v4 `DENSITIES` — tried in this order, widest first. */
export const BREADCRUMB_DENSITIES: readonly BreadcrumbDensity[] = ['full', 'abbrev', 'avatar'];

/** Widest the title may be counted as when choosing a density; past this it truncates */
export const TITLE_MEASURE_MAX_PX = 256;

/** Words that carry no weight in an initialism ("Voyages of the Covenant" → "VC") */
export const MINOR_WORDS: ReadonlySet<string> = new Set([
  'a',
  'an',
  'and',
  'the',
  'of',
  'in',
  'on',
  'at',
  'to',
  'for',
]);

/**
 * Short form of a name for the compact header. One word keeps its first three
 * letters ("Abigail" → "Abi"); several words become their initials, minor words
 * dropped ("Baraka Ilunga" → "BI"), at most four letters.
 *
 * "Letters" are CODE POINTS (`Array.from`), as in v4: an astral letter or an
 * emoji counts once, a combining mark counts as its own point.
 */
export function abbreviateName(name: string): string {
  const words = name
    .trim()
    .split(/\s+/)
    .filter((w) => w !== '');
  if (words.length === 0) return '';
  if (words.length === 1) {
    const chars = Array.from(words[0]);
    return chars.length <= 4 ? words[0] : chars.slice(0, 3).join('');
  }
  const significant = words.filter((w) => !MINOR_WORDS.has(w.toLowerCase()));
  const source = significant.length > 0 ? significant : words;
  return source
    .slice(0, 4)
    .map((w) => Array.from(w)[0].toUpperCase())
    .join('');
}

/** First density whose natural width fits, or the most compact one */
export function chooseDensity(
  available: number,
  widths: Record<BreadcrumbDensity, number>,
): BreadcrumbDensity {
  return BREADCRUMB_DENSITIES.find((d) => widths[d] <= available) ?? 'avatar';
}
