/**
 * How a comparator is spelled on a chip (v4
 * `components/custom-tools/comparator-labels.ts`).
 *
 * Shared by the outcome table and the availability gate so a reader learns one
 * vocabulary. The keys are the format's; these are the glyphs a person reads.
 */

import type { ComparatorKey } from '../../pascal/tool-draft';

export const COMPARATOR_LABELS: Record<ComparatorKey, string> = {
  gt: '>',
  gte: '≥',
  lt: '<',
  lte: '≤',
  eq: '=',
  neq: '≠',
  contains: 'contains',
  ncontains: "doesn't contain",
};
