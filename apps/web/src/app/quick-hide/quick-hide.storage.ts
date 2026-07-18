/**
 * The quick-hide localStorage substrate (v4
 * `components/providers/quick-hide-provider.tsx:31-33,:92-129`).
 *
 * Three keys, owned here and nowhere else (work order P4.9d §4). The
 * `includeAutonomousRooms` key predates this module — `autonomous-visibility.ts`
 * shipped it with the Salon list header toggle and now re-exports these
 * primitives so the one service owns all three.
 *
 * v4 reads these AFTER mount because a lazy initializer would desync SSR
 * (`:89-91`); v5's SPA is client-only, so the service reads them eagerly in its
 * initializer — the only deliberate divergence, and a behavioural no-op.
 */

/** v4 `STORAGE_KEY` (`:31`) — a JSON array of hidden tag ids. */
export const ACTIVE_TAGS_KEY = 'quilltap.quickHide.activeTags';
/** v4 `DANGER_STORAGE_KEY` (`:32`) — `'true'` / `'false'`. */
export const HIDE_DANGEROUS_KEY = 'quilltap.quickHide.hideDangerous';
/** v4 `AUTONOMOUS_STORAGE_KEY` (`:33`) — `'true'` / `'false'`. */
export const INCLUDE_AUTONOMOUS_KEY = 'quilltap.quickHide.includeAutonomousRooms';

/**
 * v4 `:96-101`: parse the id array, keeping only string entries. A malformed
 * or absent value yields the empty set (v4 leaves its state untouched, which
 * from the default is the same thing).
 */
export function parseActiveTags(raw: string | null): Set<string> {
  if (!raw) return new Set();
  try {
    const parsed: unknown = JSON.parse(raw);
    if (Array.isArray(parsed)) {
      return new Set(parsed.filter((id): id is string => typeof id === 'string'));
    }
  } catch {
    /* v4 `:111-113` warns and falls through to the defaults */
  }
  return new Set();
}

/** Read the hidden-tag set (v4 `:95-102`). */
export function readActiveTags(): Set<string> {
  if (typeof localStorage === 'undefined') return new Set();
  try {
    return parseActiveTags(localStorage.getItem(ACTIVE_TAGS_KEY));
  } catch {
    return new Set();
  }
}

/** Read a boolean key — v4 `:104,:108` treat `'true'` as the ONLY truthy value. */
export function readBooleanKey(key: string): boolean {
  if (typeof localStorage === 'undefined') return false;
  try {
    return localStorage.getItem(key) === 'true';
  } catch {
    return false;
  }
}

/** Persist the hidden-tag set (v4 `:123`). */
export function writeActiveTags(ids: ReadonlySet<string>): void {
  writeRaw(ACTIVE_TAGS_KEY, JSON.stringify(Array.from(ids)));
}

/** Persist a boolean key (v4 `:124-125` write the literals `'true'`/`'false'`). */
export function writeBooleanKey(key: string, value: boolean): void {
  writeRaw(key, value ? 'true' : 'false');
}

function writeRaw(key: string, value: string): void {
  if (typeof localStorage === 'undefined') return;
  try {
    localStorage.setItem(key, value);
  } catch {
    /* v4 `:126-128` warns; storage unavailable just means it doesn't persist */
  }
}
