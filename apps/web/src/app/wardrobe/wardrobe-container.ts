/**
 * Wardrobe containers — the four places a wardrobe item or outfit can live.
 *
 * A *container* is a browsable wardrobe location: a character's personal vault,
 * the singleton Quilltap General library, a project's document store, or a
 * group's document store. The wardrobe dialog's top selector picks one, and
 * every scoped mutation (create / edit / duplicate / star / delete) routes to
 * the container's own endpoints via the router in `wardrobe.api.ts`.
 *
 * Client-safe: no server-only imports. Shared by the dialog, the item editor,
 * and the transfer dialog so the scope encoding can't drift between them.
 *
 * A 1:1 transcription of v4 `lib/wardrobe/wardrobe-container.ts` (`d7263f39`),
 * pinned character-for-character by `wardrobe-container.spec.ts`.
 *
 * **Recorded mechanism divergence.** v4's module also exports
 * `wardrobeCollectionUrl` / `wardrobeItemUrl`, which return the four REST URL
 * families. v5's mutations ride dispatch verbs, so those two helpers land in
 * `wardrobe.api.ts` as `containerListRequest` / `containerCreateRequest` /
 * `containerGetRequest` / `containerUpdateRequest` / `containerDeleteRequest`
 * — the same container → endpoint routing, expressed in the transport this
 * app actually has. The scope strings, the encoding, and the null-id rule are
 * v4's verbatim, because the transfers wire (`source.scope`) shares them.
 *
 * @module wardrobe/wardrobe-container
 */

/** v4 `wardrobe-container.ts:16`. */
export type WardrobeContainerScope = 'character' | 'general' | 'project' | 'group';

/** v4 `wardrobe-container.ts:18-22`. */
export interface WardrobeContainer {
  scope: WardrobeContainerScope;
  /** Owning entity id — null only for the singleton `general` scope. */
  id: string | null;
}

/** v4 `wardrobe-container.ts:24`. */
export const GENERAL_CONTAINER: WardrobeContainer = { scope: 'general', id: null };

/** Serialize a container for use as a `<select>` option value (`scope:id`). */
export function encodeWardrobeContainer(container: WardrobeContainer): string {
  return `${container.scope}:${container.id ?? ''}`;
}

/** Parse a `<select>` option value back into a container, or null if mangled. */
export function decodeWardrobeContainer(value: string): WardrobeContainer | null {
  const [scopeRaw, idRaw] = value.split(':', 2);
  if (
    scopeRaw !== 'character' &&
    scopeRaw !== 'general' &&
    scopeRaw !== 'project' &&
    scopeRaw !== 'group'
  ) {
    return null;
  }
  const id = idRaw && idRaw.length > 0 ? idRaw : null;
  if (scopeRaw !== 'general' && !id) return null;
  return { scope: scopeRaw, id };
}

/** True when two containers name the same place. */
export function sameWardrobeContainer(
  a: WardrobeContainer | null | undefined,
  b: WardrobeContainer | null | undefined,
): boolean {
  if (!a || !b) return false;
  return a.scope === b.scope && (a.id ?? null) === (b.id ?? null);
}
