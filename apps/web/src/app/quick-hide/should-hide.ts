/**
 * The quick-hide predicate, transcribed from v4
 * `components/providers/quick-hide-provider.tsx:183-196` (`shouldHideByIds`).
 *
 * Kept pure and separate from the service so every consumer's tag-collection
 * semantics can be unit-pinned without standing up Angular DI.
 *
 * ⚠ PORT DIVERGENCE — the dead method. v4's provider also exports
 * `shouldHideChat` (`:198-209`), which reads `chat.isDangerous`. That field
 * does not exist on any real payload (the classifier writes `isDangerousChat`),
 * so the method can never fire its dangerous arm — and NO v4 consumer calls it.
 * Every v4 consumer instead inlines
 * `shouldHideByIds(tags) || (hideDangerousChats && chat.isDangerousChat)` with
 * the REAL field name. v5 ports the inline consumer pattern and deliberately
 * does NOT reproduce the dead method (work order P4.9d, "Port hazard").
 */

/**
 * v4 `:184-193`: false when there are no ids at all; true when ANY non-empty id
 * is in the hidden set. Nullish entries are skipped, not treated as matches
 * (v4 `:189` guards with `if (tagId && …)`).
 */
export function shouldHideByIds(
  hiddenTagIds: ReadonlySet<string>,
  tagIds?: ReadonlyArray<string | null | undefined>,
): boolean {
  if (!tagIds || tagIds.length === 0) {
    return false;
  }
  for (const tagId of tagIds) {
    if (tagId && hiddenTagIds.has(tagId)) {
      return true;
    }
  }
  return false;
}
