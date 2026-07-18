import type { CoreClient } from '../../../core/core-client';

/**
 * The global tags surface consumed by the Settings → Appearance Tags card
 * (v4 `components/settings/tags-tab.tsx` + `lib/tags/styles.ts`).
 *
 * Types are LOCAL by the round's §3 convention (the `home.api.ts` pattern): the
 * dispatch verbs `tagList` / `tagUpdate` / `tagDelete` already exist and are
 * differential-pinned, so this lane adds no wire surface.
 *
 * ⚠ THE SHARED CONTRACT'S `TagUpdateRequest` IS WRONG, on two counts. It had no
 * caller before this file, so nothing had exercised it:
 *
 *   1. SHAPE. It declares the patch fields FLAT (`{type, tagId, name?,
 *      visualStyle?, quickHide?}`), but the engine variant is
 *      `TagUpdate { tag_id, tag: Value }` under
 *      `#[serde(tag = "type", rename_all = "camelCase")]`
 *      (`api/types.rs:57-58,:604-608`), so the wire is
 *      `{type, tagId, tag: {...}}` — the patch is NESTED.
 *   2. TYPE. Both it and `TagDto` call `visualStyle` a string; the wire carries
 *      the OBJECT modelled below (`api/characters.rs:2242` passes the stored
 *      JSON through, and the patch is parsed by `parse_visual_style` into
 *      `db::tags::TagVisualStyle`).
 *
 * The calls here send the shape the engine actually accepts, casting past the
 * bad type. Correcting `core-contract.ts` is deliberately NOT done in this lane
 * — it is a shared file this round and no sibling owns it either; it is
 * reported for the unifier.
 */

/** v4 `TagVisualStyle` (`lib/schemas/common.types.ts:80-88`). */
export interface TagVisualStyle {
  emoji: string | null;
  foregroundColor: string;
  backgroundColor: string;
  emojiOnly: boolean;
  bold: boolean;
  italic: boolean;
  strikethrough: boolean;
}

/** v4 `DEFAULT_TAG_STYLE` (`lib/tags/styles.ts:3-11`). */
export const DEFAULT_TAG_STYLE: TagVisualStyle = {
  emoji: null,
  foregroundColor: '#1f2937',
  backgroundColor: '#e5e7eb',
  emojiOnly: false,
  bold: false,
  italic: false,
  strikethrough: false,
};

/**
 * v4 `mergeWithDefaultTagStyle` (`lib/tags/styles.ts:13-27`), transcribed
 * including its asymmetry: `emoji` survives only as a NON-EMPTY string (an
 * empty string becomes null), the colors fall back with `||` (so an empty
 * string also falls back), and the booleans fall back with `??` (so an explicit
 * `false` is kept).
 */
export function mergeWithDefaultTagStyle(
  style?: Partial<TagVisualStyle> | null,
): TagVisualStyle {
  if (!style) {
    return { ...DEFAULT_TAG_STYLE };
  }
  return {
    emoji: typeof style.emoji === 'string' && style.emoji.length > 0 ? style.emoji : null,
    foregroundColor: style.foregroundColor || DEFAULT_TAG_STYLE.foregroundColor,
    backgroundColor: style.backgroundColor || DEFAULT_TAG_STYLE.backgroundColor,
    emojiOnly: style.emojiOnly ?? DEFAULT_TAG_STYLE.emojiOnly,
    bold: style.bold ?? DEFAULT_TAG_STYLE.bold,
    italic: style.italic ?? DEFAULT_TAG_STYLE.italic,
    strikethrough: style.strikethrough ?? DEFAULT_TAG_STYLE.strikethrough,
  };
}

/** One row of the settings tag list (v4 `TagOption` + the route's usage fields). */
export interface SettingsTag {
  id: string;
  name: string;
  quickHide?: boolean;
  visualStyle?: Partial<TagVisualStyle> | null;
  /** v4 `route.ts:61-77` — the summed per-entity usage the Management rows show. */
  totalUsage?: number;
}

/** TanStack key for the settings tag list. */
export const settingsTagKeys = {
  list: ['tags', 'list'] as const,
};

/**
 * v4 `GET /api/v1/tags` → `{ tags, count }`. The server already sorts by
 * `localeCompare` (`api/characters.rs:2265-2269`), so no client sort is needed
 * for the picker or the style grid — v4 renders both in server order.
 */
export async function fetchSettingsTags(core: CoreClient): Promise<SettingsTag[]> {
  const data = await core.dispatchData({ type: 'tagList' });
  return (data['tags'] as SettingsTag[]) ?? [];
}

/**
 * v4 `PUT /api/v1/tags/{id}` with `{ visualStyle }` (`tags-tab.tsx:62-73`).
 * `null` removes the style.
 */
export async function updateTagVisualStyle(
  core: CoreClient,
  tagId: string,
  visualStyle: TagVisualStyle | null,
): Promise<SettingsTag> {
  const data = await core.dispatchData({
    type: 'tagUpdate',
    tagId,
    tag: { visualStyle },
  } as never);
  return data['tag'] as SettingsTag;
}

/** v4 `PUT /api/v1/tags/{id}` with `{ quickHide }` (`tags-tab.tsx:172-182`). */
export async function updateTagQuickHide(
  core: CoreClient,
  tagId: string,
  quickHide: boolean,
): Promise<SettingsTag> {
  const data = await core.dispatchData({
    type: 'tagUpdate',
    tagId,
    tag: { quickHide },
  } as never);
  return data['tag'] as SettingsTag;
}

/**
 * v4 `DELETE /api/v1/tags/{id}` (`tags-tab.tsx:203-207`). The server cascades
 * the tag off every entity that carries it first.
 */
export async function deleteTag(core: CoreClient, tagId: string): Promise<void> {
  await core.dispatchData({ type: 'tagDelete', tagId } as never);
}
