/**
 * Rendered SVG by (engine, style, theme, source) — v4 `DiagramBlock.tsx:58-84`
 * (236c04e35), moved into its own module so a spec can clear it between cases.
 *
 * The Salon transcript is virtualized, so rows unmount and remount as they
 * scroll: a remount that finds its drawing here shows it at once, with no
 * placeholder and no change of height under the virtualizer. Bounded, oldest
 * first out.
 */

import type { DiagramEngine, DiagramStyle } from './diagram-fences';

export const RENDER_CACHE_LIMIT = 64;
const renderCache = new Map<string, string>();

export function cachePeek(key: string): string | null {
  return renderCache.get(key) ?? null;
}

export function cachePut(key: string, svg: string): void {
  // Re-inserting moves the key to the young end.
  renderCache.delete(key);
  renderCache.set(key, svg);
  while (renderCache.size > RENDER_CACHE_LIMIT) {
    const oldest = renderCache.keys().next().value;
    if (oldest === undefined) break;
    renderCache.delete(oldest);
  }
}

/** Test seam: forget every cached drawing (v4 `clearDiagramRenderCache`). */
export function cacheClear(): void {
  renderCache.clear();
}

export function cacheKeyFor(
  engine: DiagramEngine,
  style: DiagramStyle,
  themeKey: string,
  source: string,
): string {
  // Raw SVG ignores both the style and the theme.
  return engine === 'svg' ? `svg|${source}` : `${engine}|${style}|${themeKey}|${source}`;
}
