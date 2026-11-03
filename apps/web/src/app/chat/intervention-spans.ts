/**
 * Operator intervention — what a roll or value could have been. A port of v4
 * `lib/pascal/intervention/spans.ts` (`8ae8a127c`), the client-safe half the
 * hold dialog prints beside the roll and the value. The engine reports a
 * {@link RunSpan} for the raw draw and for the value after modifiers; one
 * phrasing for both, here. Byte-pinned against v4's REAL `describeSpan` by
 * `system-message-labels.intervention.oracle.spec.ts`
 * (`intervention-summary.oracle.ndjson`, the `describeSpan` rows).
 */

import type { RunSpan } from '../core/core-contract';

export type { RunSpan };

/** Integers plain, floats to 4 significant digits — the way Pascal prints numbers. */
function formatSpanNumber(n: number): string {
  return Number.isInteger(n) ? String(n) : String(Number(n.toPrecision(4)));
}

/**
 * A span in words: "exactly 5", "1 to 20", "1 up to (not including) 21",
 * "more than -42, up to -2".
 */
export function describeSpan(span: RunSpan): string {
  const min = formatSpanNumber(span.min);
  const max = formatSpanNumber(span.max);
  if (span.min === span.max && !span.minExclusive && !span.maxExclusive) return `exactly ${min}`;
  if (span.minExclusive) return `more than ${min}, up to ${span.maxExclusive ? `(not including) ${max}` : max}`;
  if (span.maxExclusive) return `${min} up to (not including) ${max}`;
  return `${min} to ${max}`;
}
