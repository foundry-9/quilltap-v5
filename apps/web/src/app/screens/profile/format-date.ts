/**
 * A transcription of v4 `lib/format-time.ts` `formatDateTime` (the Profile
 * screen's Account Information rows are its first v5 consumer — hoist to a
 * shared module when a second screen needs it, the `format-time.ts` precedent).
 * The unit spec pins the v4 branches it copies.
 */

export interface FormatDateOptions {
  /** v4 default `'short'`; the profile rows pass `'long'`. */
  monthStyle?: 'short' | 'long';
  /** v4 drops the year entirely when this is explicitly `false`. */
  includeYear?: boolean;
}

/**
 * v4 `lib/format-time.ts:70-86`, branch-faithful: a falsy input is `''` (NOT
 * the string "null"), a parse failure falls back to the raw string, and the
 * locale is the ambient one (`toLocaleDateString(undefined, …)`).
 */
export function formatDateTime(
  dateString: string | null | undefined,
  opts: FormatDateOptions = {},
): string {
  if (!dateString) {
    return '';
  }
  try {
    return new Date(dateString).toLocaleDateString(undefined, {
      year: opts.includeYear === false ? undefined : 'numeric',
      month: opts.monthStyle ?? 'short',
      day: 'numeric',
      hour: '2-digit',
      minute: '2-digit',
    });
  } catch {
    return String(dateString);
  }
}

/**
 * v4 `ProfileInfoSection.tsx:12-14` — the profile rows' wrapper, which turns
 * the empty-string result into the literal `'Never'`.
 */
export function formatProfileDate(dateString: string | null | undefined): string {
  return formatDateTime(dateString, { monthStyle: 'long' }) || 'Never';
}
