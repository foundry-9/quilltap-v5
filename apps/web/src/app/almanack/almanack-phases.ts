/**
 * The Almanack — phase manifest (the client-safe mirror of v4
 * `lib/tools/almanack/phases.ts`, HEAD `f7f1a956`).
 *
 * The seven named phases the report pipeline walks through, in order. In v4 this
 * module is literally shared: it is pure data with no server-only imports, so
 * the card imports the same file the pipeline publishes `phase` events from. v5
 * cannot share a file across the Rust/TS boundary, so the manifest exists twice
 * — the server constant (P4.37) and this mirror — and the two are held together
 * by the §1 contract: **the keys, labels and `estimatedShare` values here are
 * byte-equal to v4's**, and the wire `key` is what the server tags each `phase`
 * frame with.
 *
 * The labels are user-facing and therefore in house voice. `estimatedShare` is
 * the fraction of a typical run each phase occupies; the bar uses it to size
 * segments so a long phase doesn't look stalled beside a fast one. They are
 * eyeballed from a warm-cache run and sum to 1.
 */

export type AlmanackPhaseKey =
  | 'premises'
  | 'machinery'
  | 'ledgers'
  | 'scriptorium'
  | 'personae'
  | 'wire-records'
  | 'binding';

export interface AlmanackPhase {
  key: AlmanackPhaseKey;
  /** User-facing, present-continuous, house voice. */
  label: string;
  /** Fraction of a typical run. Sums to 1 across the manifest. */
  estimatedShare: number;
}

export const ALMANACK_PHASES: readonly AlmanackPhase[] = [
  { key: 'premises', label: 'Taking the measure of the premises…', estimatedShare: 0.05 },
  { key: 'machinery', label: 'Cataloguing the machinery…', estimatedShare: 0.25 },
  { key: 'ledgers', label: 'Auditing the ledgers…', estimatedShare: 0.2 },
  { key: 'scriptorium', label: 'Touring the Scriptorium…', estimatedShare: 0.2 },
  { key: 'personae', label: 'Assembling the dramatis personae…', estimatedShare: 0.12 },
  { key: 'wire-records', label: 'Reading the wire records…', estimatedShare: 0.13 },
  { key: 'binding', label: 'Binding the volume…', estimatedShare: 0.05 },
] as const;

export const ALMANACK_PHASE_COUNT = ALMANACK_PHASES.length;

/** The feature's user-facing name. Kept here so every surface spells it alike. */
export const ALMANACK_NAME = 'The Almanack';
/**
 * The name as it appears in headings and card titles. The parenthetical is
 * deliberate — it keeps the feature findable for anyone searching the old
 * "capabilities report" wording.
 */
export const ALMANACK_TITLE = 'The Almanack (System Report)';
