/**
 * Dice notation — the pure half of the dice module: the `NdS±M` grammar, its
 * parser and formatters, and the bounds they enforce.
 *
 * A faithful port of v4 `lib/pascal/dice-notation.ts` (162 lines, pure). v4
 * split this out of `./dice` at `d68638b4` precisely so that
 * `custom-tool.types.ts` — which validates dice notation inside the schema —
 * can travel into a client bundle. Pascal's Workbench validates a draft in the
 * browser with the very same grammar the roster loader runs on the server; this
 * split is what makes that possible. The roller half stays server-side (v5:
 * `quilltap_core::pascal::dice`).
 */

/** Smallest legal die. A one-sided die is not a die. */
export const MIN_DIE_SIDES = 2;
/** Largest legal die. Mirrors the `rng` tool's long-standing bound. */
export const MAX_DIE_SIDES = 1000;
/** Fewest dice in a roll. */
export const MIN_DICE_COUNT = 1;
/** Most dice in a roll. Mirrors the `rng` tool's long-standing bound. */
export const MAX_DICE_COUNT = 100;
/** Bound on the flat modifier, kept symmetric and well clear of precision trouble. */
export const MAX_DICE_MODIFIER = 1000;

/** A parsed `NdS±M` roll specification. */
export interface DiceNotation {
  /** How many dice. `d20` implies 1. */
  count: number;
  /** Sides per die. */
  sides: number;
  /** Flat modifier applied to the total. 0 when the notation carried none. */
  modifier: number;
}

/** Anchored form for parsing a string that must be notation and nothing else. */
const DICE_NOTATION_STRICT = /^\s*(\d+)?d(\d+)(?:\s*([+-])\s*(\d+))?\s*$/i;

/** True when `count`/`sides`/`modifier` are all within bounds. */
function withinBounds(count: number, sides: number, modifier: number): boolean {
  return (
    Number.isInteger(count) &&
    Number.isInteger(sides) &&
    Number.isInteger(modifier) &&
    count >= MIN_DICE_COUNT &&
    count <= MAX_DICE_COUNT &&
    sides >= MIN_DIE_SIDES &&
    sides <= MAX_DIE_SIDES &&
    Math.abs(modifier) <= MAX_DICE_MODIFIER
  );
}

/** Build a {@link DiceNotation} from regex captures, or null when out of bounds. */
function fromCaptures(
  rawCount: string | undefined,
  rawSides: string,
  sign: string | undefined,
  rawModifier: string | undefined,
): DiceNotation | null {
  const count = rawCount ? parseInt(rawCount, 10) : 1;
  const sides = parseInt(rawSides, 10);
  const magnitude = rawModifier ? parseInt(rawModifier, 10) : 0;
  const modifier = sign === '-' ? -magnitude : magnitude;

  if (!withinBounds(count, sides, modifier)) return null;
  return { count, sides, modifier };
}

/**
 * Parse a complete dice-notation string ("3d6+2"). Returns null when the string
 * is not notation, or when its numbers fall outside the supported bounds.
 *
 * Strict: the whole string must be the notation.
 */
export function parseDiceNotation(notation: string): DiceNotation | null {
  const match = DICE_NOTATION_STRICT.exec(notation);
  if (!match) return null;
  return fromCaptures(match[1], match[2], match[3], match[4]);
}

/** Render a notation back to its canonical string ("3d6+2"). */
export function formatDiceNotation({ count, sides, modifier }: DiceNotation): string {
  const base = `${count}d${sides}`;
  if (modifier === 0) return base;
  return `${base}${modifier > 0 ? '+' : '-'}${Math.abs(modifier)}`;
}
