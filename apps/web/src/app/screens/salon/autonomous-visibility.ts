/**
 * The Salon list's autonomous-room visibility logic (v4 `SalonListView` +
 * `quick-hide-provider`). Pure + localStorage-backed so the effective-include
 * rule and hint gating are unit-pinned.
 *
 * PLACEMENT DIVERGENCE (recorded loudly): v4 keeps the "Show Autonomous Rooms"
 * toggle in the user menu (`nav-user-menu-quick-hide.tsx`) behind the
 * `quick-hide-provider`. v5 has neither a user menu nor that provider yet, so
 * the toggle rides the salon-list header row — but it persists to the SAME
 * localStorage key so a future user-menu port inherits the setting.
 */

/** The localStorage key v4's quick-hide provider uses (v5 shares it verbatim). */
export const AUTONOMOUS_TOGGLE_KEY = 'quilltap.quickHide.includeAutonomousRooms';

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

/** Read the toggle from localStorage (default false; `'true'` is the only truthy). */
export function readIncludeAutonomous(): boolean {
  if (typeof window === 'undefined') return false;
  try {
    return window.localStorage.getItem(AUTONOMOUS_TOGGLE_KEY) === 'true';
  } catch {
    return false;
  }
}

/** Persist the toggle to localStorage (v4 writes `'true'` / `'false'`). */
export function writeIncludeAutonomous(value: boolean): void {
  if (typeof window === 'undefined') return;
  try {
    window.localStorage.setItem(AUTONOMOUS_TOGGLE_KEY, value ? 'true' : 'false');
  } catch {
    /* storage unavailable — the toggle simply doesn't persist */
  }
}
