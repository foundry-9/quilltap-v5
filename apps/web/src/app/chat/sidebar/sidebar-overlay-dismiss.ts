/**
 * Narrow-pane sidebar overlay: which pointer-downs dismiss it — a port of v4
 * `components/chat/sidebar-overlay-dismiss.ts` (`b0b6656b5`, v4's fix for bug
 * 169, which this port filed after fixing it first in `af6ed7e7`).
 *
 * The overlay collapses on a click outside its panel. Dialogs opened from inside
 * the sidebar (the Scenario Builder, Save as scenario…) portal to `<body>`, so
 * the DOM `contains` check alone calls every click in them "outside" —
 * collapsing the sidebar unmounts the dialog's owner and aborts any run in
 * flight. A click inside a `.qt-dialog-overlay` belongs to that dialog, not to
 * the page behind the sidebar.
 *
 * Faithful since v4 `b0b6656b5`, including the two exits v5's first fix lacked:
 * a non-`Node` (or null) target never dismisses, and a non-`Element` node (a
 * text node) is judged by its `parentElement`.
 */

/** The dialog primitive's outermost element — covers the dialog and its backdrop. */
export const DIALOG_OVERLAY_SELECTOR = '.qt-dialog-overlay';

export function shouldDismissSidebarOverlay(
  panel: Node | null,
  target: EventTarget | null,
): boolean {
  if (!panel || !(target instanceof Node)) return false;
  if (panel.contains(target)) return false;
  const element = target instanceof Element ? target : target.parentElement;
  return !element?.closest(DIALOG_OVERLAY_SELECTOR);
}
