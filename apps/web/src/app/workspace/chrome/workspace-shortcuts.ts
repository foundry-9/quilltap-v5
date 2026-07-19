/**
 * Keyboard shortcuts for the tabbed workspace (port of v4
 * `useWorkspaceShortcuts.ts`, baseline `b8b12695`).
 *
 * All shortcuts are namespaced under **Ctrl/Cmd + Alt** so they don't collide
 * with the browser/shell's own bindings, and are inert while the user is typing
 * in a field so the Salon composer is never hijacked.
 *
 *  - Ctrl/Cmd+Alt+→ / ←   — next / previous tab in the focused pane (wraps)
 *  - Ctrl/Cmd+Alt+1 … 9   — jump to the nth tab in the focused pane
 *  - Ctrl/Cmd+Alt+W       — close the focused pane's active tab
 *  - Ctrl/Cmd+Alt+\       — toggle split (split off the active tab / rejoin)
 *
 * The v4 hook attaches once and reads the latest store through a ref; in Angular
 * the host attaches a window listener that calls this pure applier with the live
 * store — the store's signal read is always current, so there is no re-bind.
 *
 * @module workspace/chrome/workspace-shortcuts
 */

import { getPaneState, isSplit } from '../core/reducer';
import type { WorkspaceState } from '../workspace-contract';

/** The store surface the applier needs (WorkspaceService satisfies it). */
export interface ShortcutTarget {
  state(): WorkspaceState;
  setActive(pane: 'left' | 'right', id: string): void;
  closeTab(id: string): void;
  unsplit(): void;
  splitTo(id: string, pane: 'left' | 'right'): void;
}

export function isEditableTarget(target: EventTarget | null): boolean {
  if (!(target instanceof HTMLElement)) return false;
  const tag = target.tagName;
  return tag === 'INPUT' || tag === 'TEXTAREA' || tag === 'SELECT' || target.isContentEditable;
}

export function applyWorkspaceShortcut(e: KeyboardEvent, ws: ShortcutTarget): void {
  // Namespace: require Alt AND (Ctrl or Cmd). Bail otherwise so plain typing and
  // single-modifier chords are untouched.
  if (!e.altKey || !(e.ctrlKey || e.metaKey)) return;
  if (isEditableTarget(e.target)) return;

  const state = ws.state();
  const pane = state.focusedPane;
  const ps = getPaneState(state, pane);
  if (!ps) return;
  const { order, activeTabId } = ps;
  const activeIndex = activeTabId ? order.indexOf(activeTabId) : -1;

  switch (e.key) {
    case 'ArrowRight':
    case 'ArrowLeft': {
      if (order.length < 2) return;
      e.preventDefault();
      const dir = e.key === 'ArrowRight' ? 1 : -1;
      const base = activeIndex < 0 ? 0 : activeIndex;
      const next = (base + dir + order.length) % order.length;
      ws.setActive(pane, order[next]);
      return;
    }
    case 'w':
    case 'W': {
      if (!activeTabId) return;
      e.preventDefault();
      ws.closeTab(activeTabId);
      return;
    }
    case '\\': {
      e.preventDefault();
      if (isSplit(state)) {
        ws.unsplit();
      } else if (activeTabId && order.length >= 2) {
        // Only split off a tab when one would remain behind in this pane.
        ws.splitTo(activeTabId, 'right');
      }
      return;
    }
    default: {
      if (e.key >= '1' && e.key <= '9') {
        const n = Number(e.key) - 1;
        if (n < order.length) {
          e.preventDefault();
          ws.setActive(pane, order[n]);
        }
      }
    }
  }
}
