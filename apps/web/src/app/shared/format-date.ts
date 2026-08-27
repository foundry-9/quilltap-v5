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

// ---------------------------------------------------------------------------
// The relative formatters (P4.D125 — v4 `f3892158d`'s `nowMs` parameterization)
//
// v4 keeps these in the SAME `lib/format-time.ts` as the two above; v5's twins
// had been transcribed where their first consumer lived (`formatRelativeDate`
// in `screens/settings/system/tasks-queue.api.ts`, `formatChatListDate` in
// `screens/characters/view/tabs/character-conversation-card.ts`). They move
// here for v4's own reason: `f3892158d` consolidated StartupProgress's private
// `formatRelativeAge` into that one module "now that the shared clock gave both
// readouts one home", and the shared clock is the point of the `nowMs` argument
// all three now take.
// ---------------------------------------------------------------------------

/**
 * v4 `lib/format-time.ts:93-116` — a relative timestamp ("Just now", "12m ago",
 * "3h ago") for the first day, then a short date+time. Falls back to the raw
 * string if parsing fails, and returns `''` for null/undefined.
 *
 * Pass `nowMs` from `NowService.now(60_000)` in a component so the readout
 * actually advances; the `Date.now()` default keeps non-reactive callers
 * working (v4's own wording).
 */
export function formatRelativeDate(
  dateString: string | null | undefined,
  nowMs: number = Date.now(),
): string {
  if (!dateString) return '';
  try {
    const date = new Date(dateString);
    // v4 declares a `now` here and never reads it — its tail branch carries NO
    // conditional `year` key (that belongs to `formatChatListDate` alone; the
    // §3 unification review caught a `year:` invented here during the hoist,
    // likely invited by that unused v4 binding, which v5 drops).
    const diffMs = nowMs - date.getTime();
    const diffMins = Math.floor(diffMs / 60000);
    if (diffMins < 1) return 'Just now';
    if (diffMins < 60) return `${diffMins}m ago`;
    if (diffMins < 1440) return `${Math.floor(diffMins / 60)}h ago`;
    return date.toLocaleDateString(undefined, {
      month: 'short',
      day: 'numeric',
      hour: '2-digit',
      minute: '2-digit',
    });
  } catch {
    return String(dateString);
  }
}

/**
 * v4 `lib/format-time.ts:121-149` — chat-list date: today→time,
 * yesterday→'Yesterday', <7d→weekday, else→date (year only when different from
 * now).
 *
 * **Pre-existing v5 narrowing, carried:** v4's signature is
 * `(dateString, useRelative, nowMs)`; v5's only consumer always passes
 * `useRelative` true, so the flag was never transcribed and the plain
 * `toLocaleDateString()` branch has no v5 analogue. The `nowMs` parameter is
 * the one thing `f3892158d` adds here.
 *
 * Pass `nowMs` from `NowService.now(DAY_GRANULARITY_MS)` so the
 * "Yesterday"/weekday rollover happens at midnight rather than whenever the
 * card next re-renders.
 */
export function formatChatListDate(dateString: string, nowMs: number = Date.now()): string {
  const date = new Date(dateString);
  if (Number.isNaN(date.getTime())) {
    return String(dateString);
  }
  const now = new Date(nowMs);
  const diffMs = nowMs - date.getTime();
  const diffDays = Math.floor(diffMs / (1000 * 60 * 60 * 24));

  if (diffDays === 0) {
    return date.toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' });
  }
  if (diffDays === 1) {
    return 'Yesterday';
  }
  if (diffDays < 7) {
    // v4 renders the LONG weekday ("Monday", `format-time.ts:144`); the §3
    // unification review caught a 'short' introduced during the hoist (main's
    // pre-round transcription was faithful).
    return date.toLocaleDateString([], { weekday: 'long' });
  }
  return date.toLocaleDateString([], {
    month: 'short',
    day: 'numeric',
    year: date.getFullYear() !== now.getFullYear() ? 'numeric' : undefined,
  });
}

/**
 * v4 `lib/format-time.ts:161-167` — second-resolution relative age ("just now",
 * "42s ago", "3m ago") for a raw epoch-millisecond timestamp.
 *
 * Distinct from {@link formatRelativeDate}, which takes a date *string* and only
 * resolves to the minute — too coarse for the startup screen, where the whole
 * point is watching each step land. This lived as a private helper in v4's
 * `StartupProgress` until the shared clock gave both readouts one home.
 *
 * ⚠ **No v5 consumer yet.** v5's `qt-startup-screen` is the "just getting our
 * bearings" card, not v4's per-step event list with ages — that list has never
 * been ported (see the P4.D125 lane record). The function lands here anyway
 * because it is the module's contract in v4 and the pair of formatters above
 * now share its `nowMs` shape; its parity spec drives it directly.
 */
export function formatRelativeAge(ts: number, nowMs: number = Date.now()): string {
  const seconds = Math.max(0, Math.round((nowMs - ts) / 1000));
  if (seconds < 2) return 'just now';
  if (seconds < 60) return `${seconds}s ago`;
  const minutes = Math.floor(seconds / 60);
  return `${minutes}m ago`;
}
