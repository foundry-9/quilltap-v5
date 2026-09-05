/**
 * Zod ≥ 4.5.4's string-length rules (v4 `6e1a64ea6` moved `zod` 4.4.3 → 4.5.4).
 *
 * Zod measures strings in Unicode code points, not UTF-16 units — but only
 * counts code points inside the window where the two measures can disagree
 * (`node_modules/zod/v4/core/checks.js`: a code point is one or two units).
 * Under 4.4.3 every check was plain `String.length`. The browser schema twin
 * (`custom-tool-types.ts`) reproduces the sentence and verdict the server
 * produces, so it follows the same rule; the server's twin is
 * `jsstr::zod_len_*` in `crates/quilltap-core/src/jsstr.rs`.
 *
 * NOT for the draft validators in `tool-draft.ts`: v4's client `validateDraft`
 * compares plain `.length` (`lib/pascal/tool-draft.ts:1276…`), not Zod.
 */

/** Zod's `util.codePointLength`: a surrogate pair counts once. */
export function codePointLength(str: string): number {
  let count = str.length;
  for (let i = 0; i < str.length - 1; i++) {
    if ((str.charCodeAt(i) & 0xfc00) === 0xd800 && (str.charCodeAt(i + 1) & 0xfc00) === 0xdc00) {
      count--;
      i++;
    }
  }
  return count;
}

/** `$ZodCheckMaxLength`: only an overflow in units has to be counted. */
export function zodLenMaxOk(str: string, max: number): boolean {
  const units = str.length;
  const length = units > max ? codePointLength(str) : units;
  return length <= max;
}

/** `$ZodCheckMinLength`: only `[min, 2·min)` units is in doubt. */
export function zodLenMinOk(str: string, min: number): boolean {
  const units = str.length;
  const length = units >= min && units < min * 2 ? codePointLength(str) : units;
  return length >= min;
}

/** `$ZodCheckLengthEquals`: only `[n, 2·n]` units is in doubt. */
export function zodLenEqOk(str: string, n: number): boolean {
  const units = str.length;
  const length = units >= n && units <= n * 2 ? codePointLength(str) : units;
  return length === n;
}
