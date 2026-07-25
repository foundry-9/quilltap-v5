/**
 * A transcription of v4 `lib/format-time.ts`'s date formatters.
 *
 * This module started life at `screens/profile/format-date.ts`, whose header
 * said to "hoist to a shared module when a second screen needs it". The Post
 * Office's Compose Mail dialog is that second consumer (v4
 * `ComposeMailDialog.tsx:279` labels each "In reply to" option with
 * `formatDate(l.sentAt, { includeYear: false })`), so the module moved here and
 * grew v4's DATE-ONLY `formatDate` beside the date+time one.
 *
 * (v4's `formatMessageTime` is still transcribed separately at
 * `screens/home/format-time.ts` — a different function with its own spec, left
 * where it is.)
 */

export interface FormatDateOptions {
  /** v4 default `'short'`; the profile rows pass `'long'`. */
  monthStyle?: 'short' | 'long';
  /** v4 drops the year entirely when this is explicitly `false`. */
  includeYear?: boolean;
}

/**
 * v4 `lib/format-time.ts:47-64` — date only, no time. Branch-faithful: a falsy
 * input is `''` (NOT the string "null"), a parse failure falls back to the raw
 * string, and the locale is the ambient one (`toLocaleDateString(undefined, …)`).
 */
export function formatDate(
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
    });
  } catch {
    return String(dateString);
  }
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
