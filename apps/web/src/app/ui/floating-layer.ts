/**
 * The topmost floating layer — a port of v4 `components/ui/floating-layer.ts`
 * (`99463fc56`, at `236c04e35`).
 *
 * A few surfaces (the always-on Brahma Console and its launcher button) float
 * above every modal so the operator can reach them mid-form. They are not part
 * of whatever modal is open, so a modal's "click outside to close" or "Escape
 * to close" must not fire for interactions inside them. Those surfaces carry
 * {@link FLOATING_LAYER_ATTR}; outside-click/Escape handlers ask
 * {@link isInFloatingLayer} before dismissing.
 *
 * v4's `floatingLayerProps` spread has no Angular counterpart: a template binds
 * the attribute statically (`data-qt-floating-layer=""`).
 *
 * @module ui/floating-layer
 */

/** Marks an element as part of the topmost floating layer. */
export const FLOATING_LAYER_ATTR = 'data-qt-floating-layer';

/** True when an event target sits inside a topmost floating-layer surface. */
export function isInFloatingLayer(target: EventTarget | null): boolean {
  return target instanceof Element && target.closest(`[${FLOATING_LAYER_ATTR}]`) !== null;
}
