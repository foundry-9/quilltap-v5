/**
 * Wardrobe item drafts — the one form-state shape for creating or editing a
 * wardrobe item on the client, and the one way it becomes a request body.
 *
 * The item editor, Duplicate, and Save-as-outfit all start from a draft and
 * send `draftToPayload(draft)`, so a create body is spelled once. Validation
 * mirrors `wardrobeItemFieldsSchema` — the same schema the routes parse the
 * body with — so the client can't accept what the server will refuse.
 *
 * A port of v4 `lib/wardrobe/item-draft.ts` (`9ecebf986`; `folder` and
 * `payloadForEdit` from `4a38222e6`, read at that pin), pinned by
 * `item-draft.spec.ts` (v4's five cases).
 *
 * **Recorded mechanism divergence.** The SPA has no zod, so `validateDraft`
 * checks the three rules of `wardrobeItemFieldsSchema` a draft can break, in
 * the schema's key order (zod reports `issues[0]`): `title` `min(1, 'Title is
 * required')`, `types` `min(1, 'At least one type is required')`, and
 * `folder` `max(900)` with zod 4.6.5's own default sentence (measured at the
 * pin). The other keys are typed so a draft cannot break them.
 *
 * @module wardrobe/item-draft
 */

import type { WardrobeItemDto, WardrobeSlotType } from '../core/core-contract';
import { isComposite } from './composite-types';

/** Editable fields of a wardrobe item, as a form holds them (strings, never null). */
export interface WardrobeItemDraft {
  title: string;
  description: string;
  imagePrompt: string;
  appropriateness: string;
  isDefault: boolean;
  types: WardrobeSlotType[];
  /** Non-empty makes the item a composite (an outfit). */
  componentItemIds: string[];
  /** Composite-only: clear the designated slots on equip instead of layering. */
  replace: boolean;
  /** Folder under `Wardrobe/` (`''` = root). Filing only. */
  folder: string;
}

/** The create/update body the wardrobe item routes accept. */
export interface WardrobeItemPayload {
  title: string;
  description: string | null;
  imagePrompt: string | null;
  types: WardrobeSlotType[];
  appropriateness: string | null;
  isDefault: boolean;
  componentItemIds: string[];
  replace: boolean;
  /** Omitted on an edit that leaves the folder alone (see `payloadForEdit`). */
  folder?: string;
}

/** A blank draft, optionally pre-filled. */
export function emptyDraft(overrides?: Partial<WardrobeItemDraft>): WardrobeItemDraft {
  return {
    title: '',
    description: '',
    imagePrompt: '',
    appropriateness: '',
    isDefault: false,
    types: [],
    componentItemIds: [],
    replace: false,
    folder: '',
    ...overrides,
  };
}

/** A draft holding an existing item's editable fields. */
export function draftFromItem(item: WardrobeItemDto): WardrobeItemDraft {
  return {
    title: item.title,
    description: item.description ?? '',
    imagePrompt: item.imagePrompt ?? '',
    appropriateness: item.appropriateness ?? '',
    isDefault: item.isDefault ?? false,
    types: [...item.types],
    componentItemIds: [...(item.componentItemIds ?? [])],
    replace: item.replace ?? false,
    folder: item.folder ?? '',
  };
}

/**
 * The request body for a draft. Optional text fields left blank go as null;
 * `replace` is sent only for a composite (a leaf always replaces its slots).
 */
export function draftToPayload(draft: WardrobeItemDraft): WardrobeItemPayload {
  const blankToNull = (value: string): string | null => (value.trim() ? value : null);
  return {
    title: draft.title.trim(),
    description: blankToNull(draft.description),
    imagePrompt: blankToNull(draft.imagePrompt),
    types: [...draft.types],
    appropriateness: blankToNull(draft.appropriateness),
    isDefault: draft.isDefault,
    componentItemIds: [...draft.componentItemIds],
    replace: isComposite(draft) ? draft.replace : false,
    folder: draft.folder,
  };
}

/**
 * The body for an edit: `payload` without `folder` when the item stays where
 * it is. A hand-made Scriptorium folder (`Hats: Summer`) is kept exactly as
 * stored only if an unchanged folder is never re-sent for resolving.
 */
export function payloadForEdit(
  payload: WardrobeItemPayload,
  originalFolder: string,
): WardrobeItemPayload {
  if ((payload.folder ?? '').toLowerCase() !== originalFolder.toLowerCase()) return payload;
  const rest = { ...payload };
  delete rest.folder;
  return rest;
}

export type DraftValidation =
  | { ok: true; payload: WardrobeItemPayload }
  | { ok: false; error: string };

/** v4 `WardrobeFolderInputSchema` — `z.string().max(900)`. */
const FOLDER_MAX_LENGTH = 900;

/**
 * Validate a draft against the routes' own body schema. Returns the payload
 * to send, or the first problem as a sentence for a toast.
 */
export function validateDraft(draft: WardrobeItemDraft): DraftValidation {
  const payload = draftToPayload(draft);
  if (payload.title.length < 1) return { ok: false, error: 'Title is required' };
  if (payload.types.length < 1) return { ok: false, error: 'At least one type is required' };
  if ((payload.folder ?? '').length > FOLDER_MAX_LENGTH) {
    return {
      ok: false,
      error: `Too big: expected string to have <=${FOLDER_MAX_LENGTH} characters`,
    };
  }
  return { ok: true, payload };
}
