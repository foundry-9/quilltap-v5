/**
 * How a Staff member is named in prose — a port of v4 `lib/chat/staff-display-names.ts`
 * (extracted there by `0246c6c8`).
 *
 * The single source of truth for the display name behind a `systemSender`. Every
 * surface that spells one out reads it here: the Salon's announcement chip and
 * anything that comes after. Two copies of this table drift the moment a member
 * is added — and adding one already means touching the `systemSender` union, the
 * `chat_messages` column and the message avatar, so it has no business also
 * being a hunt for scattered name maps.
 *
 * A Carina answer is the exception the table cannot express: it renders under
 * the ANSWERER character's own name (and avatar), and falls back to 'Carina'
 * only when that character cannot be resolved. Callers handle that before
 * reaching for this map.
 *
 * NOT the same table as `post-office/post-office.api.ts`'s `STAFF_OPTIONS`: that
 * is v4's InsertAnnouncementDialog picker roster — its own display order, no
 * Carina, and Pascal under his full billing. v4 keeps the two apart too;
 * `0246c6c8` deliberately left it alone.
 */

import type { SystemSender } from '../core/core-contract';

export const STAFF_DISPLAY_NAMES: Record<NonNullable<SystemSender>, string> = {
  lantern: 'The Lantern',
  aurora: 'Aurora',
  librarian: 'The Librarian',
  concierge: 'The Concierge',
  prospero: 'Prospero',
  host: 'The Host',
  commonplaceBook: 'The Commonplace Book',
  ariel: 'Ariel',
  carina: 'Carina',
  suparna: 'Suparṇā',
  pascal: 'Pascal',
};

/**
 * The display name for a `systemSender`, or `''` when there is none (an ordinary
 * participant message). An unrecognised sender — a row written by a newer build
 * — falls back to the raw tag rather than vanishing.
 */
export function staffDisplayName(sender: string | null | undefined): string {
  if (!sender) return '';
  return STAFF_DISPLAY_NAMES[sender as NonNullable<SystemSender>] ?? sender;
}
