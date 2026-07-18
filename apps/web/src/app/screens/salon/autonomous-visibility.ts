/**
 * The Salon list's autonomous-room visibility MATH (v4 `SalonListView`). Pure,
 * so the effective-include rule and hint gating stay unit-pinned.
 *
 * PLACEMENT DIVERGENCE (recorded loudly, and it STANDS): v4 keeps the "Show
 * Autonomous Rooms" toggle in the user menu (`nav-user-menu-quick-hide.tsx`)
 * behind the `quick-hide-provider`. v5 keeps it on the salon-list header row.
 *
 * As of P4.9d the STATE no longer lives here: `QuickHideService` owns all three
 * quick-hide keys (work order §4), and the salon-list header toggle binds its
 * `includeAutonomousRooms` signal — so the header toggle and the user-menu
 * section are two views of one value. The key + storage primitives below are
 * re-exports from that service's storage module, kept so this file's spec and
 * any pre-P4.9d import site stay valid.
 */

import {
  INCLUDE_AUTONOMOUS_KEY,
  readBooleanKey,
  writeBooleanKey,
} from '../../quick-hide/quick-hide.storage';

/** The localStorage key v4's quick-hide provider uses (v5 shares it verbatim). */
export const AUTONOMOUS_TOGGLE_KEY = INCLUDE_AUTONOMOUS_KEY;

/**
 * v4 `wantsAutonomousByDefault`: a user whose room-visibility default is not
 * `owner_only` (i.e. `household` / `open`) wants autonomous rooms shown without
 * flipping the toggle. Undefined defaults to `owner_only`.
 */
export function wantsAutonomousByDefault(visibilityDefault: string | undefined): boolean {
  return (visibilityDefault ?? 'owner_only') !== 'owner_only';
}

/**
 * v4 `effectiveIncludeAutonomous = wantsAutonomousByDefault || includeAutonomousRooms`:
 * the visibility default OR the explicit toggle includes autonomous rooms.
 */
export function effectiveInclude(visibilityDefault: string | undefined, toggle: boolean): boolean {
  return wantsAutonomousByDefault(visibilityDefault) || toggle;
}

/**
 * v4 `hasHiddenAutonomous`: the hint renders only when rooms are actually
 * excluded AND the user owns at least one (the probe runs only when excluding).
 */
export function hasHiddenAutonomous(effectiveInclude: boolean, roomCount: number): boolean {
  return !effectiveInclude && roomCount > 0;
}

/**
 * Read the toggle from localStorage (default false; `'true'` is the only truthy).
 * Delegates to the quick-hide storage module, which now owns the key.
 */
export function readIncludeAutonomous(): boolean {
  return readBooleanKey(INCLUDE_AUTONOMOUS_KEY);
}

/** Persist the toggle to localStorage (v4 writes `'true'` / `'false'`). */
export function writeIncludeAutonomous(value: boolean): void {
  writeBooleanKey(INCLUDE_AUTONOMOUS_KEY, value);
}
