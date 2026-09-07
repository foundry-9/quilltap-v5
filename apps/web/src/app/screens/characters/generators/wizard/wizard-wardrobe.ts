/**
 * The leaf-first ordering helper for wizard-generated wardrobe items — v4
 * `lib/wardrobe/generated-items.ts`'s `orderGeneratedItemsLeafFirst`, at the
 * `f699da6f6` pin. A local copy (not owned by this lane's file set otherwise)
 * because `saveGeneratedWardrobeItems` needs it to create leaf garments before
 * the composites that reference them by title.
 *
 * @module screens/characters/generators/wizard/wizard-wardrobe
 */

/**
 * Order generated items so leaf garments precede the composites that
 * reference them (and shallower composites precede deeper ones), so a
 * one-at-a-time persistence path never writes a composite ahead of its
 * components.
 */
export function orderGeneratedItemsLeafFirst<T extends { title: string; components?: string[] }>(
  items: T[],
): T[] {
  const depth = (item: T, seen: Set<string>): number => {
    if (!item.components || item.components.length === 0) return 0;
    if (seen.has(item.title.toLowerCase())) return 0;
    seen.add(item.title.toLowerCase());
    let max = 0;
    for (const title of item.components) {
      const component = items.find((i) => i.title.toLowerCase() === title.trim().toLowerCase());
      if (component) max = Math.max(max, depth(component, seen) + 1);
    }
    return max;
  };
  return [...items].sort((a, b) => depth(a, new Set()) - depth(b, new Set()));
}
