/**
 * Pure Commonplace Book presentation logic, transcribed verbatim from v4 so the
 * thresholds/joins are unit-provable without a DOM (the P4.6q Play-As precedent).
 *   - importance buckets: v4 `importanceBand` (`lib/memory/scoring/importance.ts`,
 *     `e0ac7493a`; ≥0.7 High, ≥0.3 Medium, else Low — was ≥0.4 before the pin)
 *   - keywords join/split: v4 `memory-editor.tsx` (comma-joined ↔ trimmed array)
 *   - page-dedupe: v4 `memory-list.tsx` (skip ids already in the list)
 */

import type { MemoryDto } from '../core/core-contract';

export type ImportanceBucket = 'High' | 'Medium' | 'Low';

/** v4 `IMPORTANCE_HIGH` — at or above this a memory is in the high band. */
export const IMPORTANCE_HIGH = 0.7;
/** v4 `IMPORTANCE_LOW` — below this a memory is in the low band. */
export const IMPORTANCE_LOW = 0.3;

export type ImportanceBand = 'high' | 'medium' | 'low';

/** v4 `importanceBand` (`lib/memory/scoring/importance.ts`): high ≥ 0.7, low < 0.3, medium between. */
export function importanceBand(value: number): ImportanceBand {
  if (value >= IMPORTANCE_HIGH) return 'high';
  if (value < IMPORTANCE_LOW) return 'low';
  return 'medium';
}

/** v4 `memory-card.tsx` / `memory-editor.tsx`: `band === 'high' ? 'High' : band === 'medium' ? 'Medium' : 'Low'`. */
export function importanceLabel(importance: number): ImportanceBucket {
  const band = importanceBand(importance);
  return band === 'high' ? 'High' : band === 'medium' ? 'Medium' : 'Low';
}

/**
 * v4 `memory-card.tsx`: high → destructive, medium → warning, else `secondary`. Returns the
 * `qt-text-*` class the card paints the importance label with. (qt-class-exception:
 * the glob is prose, not a class.)
 */
export function importanceColorClass(importance: number): string {
  const band = importanceBand(importance);
  if (band === 'high') return 'qt-text-destructive';
  if (band === 'medium') return 'qt-text-warning';
  return 'qt-text-secondary';
}

/** v4: `Math.round(importance * 100)` — the percent shown in the editor label. */
export function importancePercent(importance: number): number {
  return Math.round(importance * 100);
}

/** v4 editor: `keywords.join(', ')` for the comma-separated input value. */
export function keywordsToString(keywords: string[]): string {
  return keywords.join(', ');
}

/** v4 editor: split on comma, trim, drop empties. */
export function keywordsFromString(raw: string): string[] {
  return raw
    .split(',')
    .map((k) => k.trim())
    .filter((k) => k.length > 0);
}

/** v4 `formatDate` (memory-card footer): locale short date, or '' for empty. */
export function formatMemoryDate(dateString: string | null | undefined): string {
  if (!dateString) return '';
  try {
    return new Date(dateString).toLocaleDateString(undefined, {
      year: 'numeric',
      month: 'short',
      day: 'numeric',
    });
  } catch {
    return String(dateString);
  }
}

/**
 * v4 memory-list append path: offset pagination is unstable while the corpus is
 * mutating (a regenerate sweep), so skip any ids already present so React keys
 * (and Angular `track`) stay unique. Preserves order; appends only fresh ids.
 */
export function appendUniqueMemories(prev: MemoryDto[], incoming: MemoryDto[]): MemoryDto[] {
  const seen = new Set(prev.map((m) => m.id));
  const additions = incoming.filter((m) => !seen.has(m.id));
  return additions.length === incoming.length ? [...prev, ...incoming] : [...prev, ...additions];
}
