/**
 * The dressing-instructions data layer (v4 `lib/hooks/use-wardrobe-instructions.ts`,
 * NEW at `b86bb1a5`).
 *
 * Loads and saves the optional `Wardrobe/instructions.md` of ONE wardrobe
 * container — a character's vault, Quilltap General, a project's store, or a
 * group's store. No tier merging happens here: the editor shows each container
 * exactly its own file; the character > group > project > general cascade is
 * applied server-side, and only when a character dresses themselves.
 *
 * **Recorded mechanism divergence.** v4 reaches these through
 * `?action=instructions` on `wardrobeCollectionUrl(container)` — the same URL
 * shape `wardrobe-container.ts`'s header already records as becoming request
 * builders here. The four containers × two directions are therefore the eight
 * verbs of the P4.D119/P4.D121 Shared contract A1, routed below exactly as
 * `containerListRequest` and friends route the CRUD verbs.
 *
 * v4 additionally keeps this out of React Query on purpose (plain `useState` +
 * `fetch`, so it composes with the dialog's manual reloads); v5's equivalent is
 * the plain signal state {@link WardrobeInstructionsSection} holds — no query
 * cache is involved on either side.
 *
 * @module wardrobe/wardrobe-instructions.api
 */

import type { CoreClient } from '../core/core-client';
import type {
  CharacterWardrobeInstructionsGetRequest,
  CharacterWardrobeInstructionsSetRequest,
  GroupWardrobeInstructionsGetRequest,
  GroupWardrobeInstructionsSetRequest,
  ProjectWardrobeInstructionsGetRequest,
  ProjectWardrobeInstructionsSetRequest,
  WardrobeInstructionsDto,
  WardrobeInstructionsGetRequest,
  WardrobeInstructionsSetRequest,
} from '../core/core-contract';
import type { WardrobeContainer } from './wardrobe-container';

/** The four read verbs (v4 `GET …?action=instructions`). */
export type WardrobeInstructionsGetVerb =
  | CharacterWardrobeInstructionsGetRequest
  | GroupWardrobeInstructionsGetRequest
  | ProjectWardrobeInstructionsGetRequest
  | WardrobeInstructionsGetRequest;

/** The four write verbs (v4 `POST …?action=instructions`). */
export type WardrobeInstructionsSetVerb =
  | CharacterWardrobeInstructionsSetRequest
  | GroupWardrobeInstructionsSetRequest
  | ProjectWardrobeInstructionsSetRequest
  | WardrobeInstructionsSetRequest;

function containerId(container: WardrobeContainer): string {
  if (!container.id) {
    throw new Error(`Wardrobe container of scope "${container.scope}" has no id`);
  }
  return container.id;
}

/** v4 `instructionsUrl(container)` read with GET. */
export function instructionsGetRequest(
  container: WardrobeContainer,
): WardrobeInstructionsGetVerb {
  switch (container.scope) {
    case 'character':
      return { type: 'characterWardrobeInstructionsGet', characterId: containerId(container) };
    case 'project':
      return { type: 'projectWardrobeInstructionsGet', projectId: containerId(container) };
    case 'group':
      return { type: 'groupWardrobeInstructionsGet', groupId: containerId(container) };
    case 'general':
      return { type: 'wardrobeInstructionsGet' };
  }
}

/**
 * v4 `instructionsUrl(container)` written with POST. `instructions` is sent on
 * every arm — REQUIRED and nullable, never omitted (contract A1: v4's schema is
 * `z.string().nullable()`, so an absent key is a validation failure, and `null`
 * is how the file is cleared).
 */
export function instructionsSetRequest(
  container: WardrobeContainer,
  instructions: string | null,
): WardrobeInstructionsSetVerb {
  switch (container.scope) {
    case 'character':
      return {
        type: 'characterWardrobeInstructionsSet',
        characterId: containerId(container),
        instructions,
      };
    case 'project':
      return {
        type: 'projectWardrobeInstructionsSet',
        projectId: containerId(container),
        instructions,
      };
    case 'group':
      return {
        type: 'groupWardrobeInstructionsSet',
        groupId: containerId(container),
        instructions,
      };
    case 'general':
      return { type: 'wardrobeInstructionsSet', instructions };
  }
}

/** Read the echoed value out of either arm's body (contract A2). */
function readEcho(data: Record<string, unknown>): string | null {
  return (data as unknown as WardrobeInstructionsDto).instructions ?? null;
}

/**
 * Load one container's instructions. Rejects on a transport/server failure —
 * the caller warns and falls back to `null`, which is v4's `catch` (`:64-67`):
 * a container whose file could not be read shows as having none rather than
 * showing another container's.
 */
export async function loadWardrobeInstructions(
  core: CoreClient,
  container: WardrobeContainer,
): Promise<string | null> {
  return readEcho(await core.dispatchData(instructionsGetRequest(container)));
}

/**
 * Write (non-blank) or clear (`null`) one container's instructions, answering
 * the server's echo — the TRIMMED string, or `null` when cleared. Rejects on
 * failure, exactly as v4's `!res.ok` throw does (`:88`).
 */
export async function saveWardrobeInstructions(
  core: CoreClient,
  container: WardrobeContainer,
  instructions: string | null,
): Promise<string | null> {
  return readEcho(await core.dispatchData(instructionsSetRequest(container, instructions)));
}
