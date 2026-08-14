/**
 * Composite Wardrobe Item Expansion — a port of v4
 * `lib/wardrobe/expand-composites.ts` (`expandComposites`, widened to the
 * structural `CompositeNode` at 4.8.2 in `61574563`).
 *
 * Wardrobe items can reference other items via `componentItemIds`, building up
 * layered or themed outfits ("nice jewelry" = locket + earrings + ring; "rain
 * outfit" = raincoat + jeans + boots). Equipped state used to store the
 * composite's own ID and expand at read time; since 4.8.2 a bundle dissolves as
 * it goes ON (see `dissolve-bundles.ts`), and this walk is what finds its
 * leaves. Read-time expansion stays for outfits equipped before that change.
 *
 * Expansion is cycle-tolerant — vault files are user-editable, and a malformed
 * cycle must never break a chat. Cycles are logged and the offending branch is
 * truncated.
 *
 * NOT the same function as `outfit-store.ts`'s `expandSummaryComposites`, which
 * is the port of v4's *client-local* expander in `lib/hooks/use-outfit.ts` and
 * reports no cycles. Both exist in v4 too; keep them apart.
 */

/**
 * The only thing expansion actually reads off an item. Stated structurally so
 * the lighter client-side wardrobe summaries can be expanded too — a full
 * `WardrobeItemDto` satisfies it (v4 `expand-composites.ts:20-27`).
 */
export interface CompositeNode {
  componentItemIds?: readonly string[];
}

export interface ExpandResult {
  /**
   * Leaf wardrobe item IDs in expansion order, deduplicated. Composites
   * themselves are NOT in this list. Unknown ids (no entry in `itemsById`) are
   * emitted as leaves so callers can see them; they'll typically be filtered
   * downstream.
   */
  leafIds: string[];
  /** Cycles detected during expansion (each is the path including the repeated id). */
  cycles: string[][];
  /** True if `maxDepth` was hit on any branch — usually pathological nesting. */
  truncated: boolean;
}

export interface ExpandOptions {
  /** Max recursion depth. Defaults to 4. */
  maxDepth?: number;
}

/**
 * Default recursion bound for composite expansion (v4 `COMPOSITE_MAX_DEPTH`).
 */
export const COMPOSITE_MAX_DEPTH = 4;

const DEFAULT_MAX_DEPTH = COMPOSITE_MAX_DEPTH;

/**
 * Expand a list of wardrobe item IDs (some of which may be composites) into
 * their leaf components. Roots that are themselves leaves come back unchanged
 * (v4 `:58-118`).
 */
export function expandComposites(
  rootIds: readonly string[],
  itemsById: ReadonlyMap<string, CompositeNode>,
  options?: ExpandOptions,
): ExpandResult {
  const maxDepth = options?.maxDepth ?? DEFAULT_MAX_DEPTH;
  const leafIds: string[] = [];
  const seenLeaves = new Set<string>();
  const cycles: string[][] = [];
  let truncated = false;

  const emitLeaf = (id: string): void => {
    if (seenLeaves.has(id)) return;
    seenLeaves.add(id);
    leafIds.push(id);
  };

  const visit = (id: string, path: string[], depth: number): void => {
    const item = itemsById.get(id);
    if (!item) {
      // Unknown id — surface as a leaf so callers can decide how to handle it.
      emitLeaf(id);
      return;
    }

    if (path.includes(id)) {
      cycles.push([...path, id]);
      return;
    }

    if (depth >= maxDepth) {
      truncated = true;
      console.warn('[expandComposites] Max depth reached, treating as leaf', {
        context: 'wardrobe',
        itemId: id,
        depth,
        maxDepth,
      });
      emitLeaf(id);
      return;
    }

    const components = item.componentItemIds ?? [];
    if (components.length === 0) {
      emitLeaf(id);
      return;
    }

    const nextPath = [...path, id];
    for (const childId of components) {
      visit(childId, nextPath, depth + 1);
    }
  };

  for (const root of rootIds) {
    visit(root, [], 0);
  }

  return { leafIds, cycles, truncated };
}
