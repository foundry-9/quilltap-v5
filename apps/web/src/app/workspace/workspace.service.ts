/**
 * WorkspaceService — the tabbed-workspace client store (port of v4
 * `components/providers/workspace-provider.tsx`, baseline `b8b12695`).
 *
 * Wraps the pure {@link workspaceReducer} with signal state, uuid minting,
 * localStorage persistence (debounced), and one-shot hydration. It is the
 * concrete {@link WorkspaceHandle} the host provides under `WORKSPACE_HANDLE`.
 *
 * Keep-alive constraint (v4): this store never decides whether a view is
 * mounted — the host renders every open tab and hides inactive ones via CSS.
 *
 * v5 shape differences from v4, all documented:
 *  - Hydration is HOST-triggered (`hydrateOnce()` is called by the workspace
 *    host when it mounts) rather than run in the provider's mount effect, so the
 *    store stays inert — no chat fetch, no layout write — while the app runs in
 *    routed (flag-off) mode and nothing hosts a workspace.
 *  - The instance id is `null` for now (v5 exposes no client-visible instance
 *    id); the scoped `workspaceStorageKey(instanceId)` form stays corpus-proven.
 *    NAMED DEFERRAL (tier 3).
 *
 * @module workspace/workspace.service
 */

import { Injectable, signal } from '@angular/core';

import { CoreClient } from '../core/core-client';
import {
  createInitialState,
  tabIdentity,
  workspaceReducer,
  type WorkspaceAction,
} from './core/reducer';
import {
  hydrateWorkspaceState,
  serializeWorkspaceState,
  workspaceStorageKey,
} from './core/persistence';
import { defaultTabMeta } from './core/tab-meta';
import type {
  OpenTabOptions,
  PaneId,
  TabKind,
  WorkspaceHandle,
  WorkspaceState,
} from './workspace-contract';

/** Stable id for the default home tab (home is a singleton). */
const HOME_TAB_ID = 'home';

/** How long to wait after the last change before persisting. */
export const PERSIST_DEBOUNCE_MS = 250;

function freshId(): string {
  if (typeof crypto !== 'undefined' && typeof crypto.randomUUID === 'function') {
    return crypto.randomUUID();
  }
  // Extremely defensive fallback for environments without Web Crypto.
  return `tab-${Math.random().toString(36).slice(2)}${Math.random().toString(36).slice(2)}`;
}

@Injectable({ providedIn: 'root' })
export class WorkspaceService implements WorkspaceHandle {
  // Constructor injection (not `inject()`) so the store can be unit-constructed
  // directly with a stub core — no TestBed / Angular test env needed.
  constructor(private readonly core: CoreClient) {}

  private readonly _state = signal<WorkspaceState>(createInitialState(HOME_TAB_ID));
  /** The live workspace layout. */
  readonly state = this._state.asReadonly();

  private readonly _hydrated = signal(false);
  /** True once the layout has been hydrated from localStorage (or confirmed empty). */
  readonly hydrated = this._hydrated.asReadonly();

  // The scoped form (`workspaceStorageKey(instanceId)`) stays corpus-proven; v5
  // has no client-visible instance id yet, so we pass null. NAMED DEFERRAL.
  private readonly storageKey = workspaceStorageKey(null);

  private persistTimer: ReturnType<typeof setTimeout> | null = null;
  private hydrateStarted = false;
  private ready: Promise<void> | null = null;

  // -------------------------------------------------------------------------
  // Hydration (host-triggered, one-shot)
  // -------------------------------------------------------------------------

  /**
   * Hydrate the layout from localStorage exactly once, pruning tabs whose chat
   * no longer exists (via the CoreClient chats list — v4's `isChatValid` seam).
   * Idempotent: repeated calls return the same in-flight/settled promise. The
   * host awaits this before applying a `?open=` intent (the v4 hydration race:
   * applying the intent before the hydrate REPLACE_STATE would let the hydrate
   * clobber the just-opened tab).
   */
  hydrateOnce(): Promise<void> {
    if (this.hydrateStarted) return this.ready as Promise<void>;
    this.hydrateStarted = true;
    this.ready = this.runHydrate();
    return this.ready;
  }

  private async runHydrate(): Promise<void> {
    let isChatValid: (id: string) => boolean = () => true;
    try {
      const chats = await this.core.dispatchExpect({ type: 'listChats' }, 'chats');
      const ids = new Set(chats.map((c) => c.id));
      isChatValid = (id) => ids.has(id);
    } catch {
      // Chats unavailable — keep every tab (best-effort, v4-faithful default).
    }
    let raw: string | null = null;
    try {
      raw = localStorage.getItem(this.storageKey);
    } catch {
      // Storage unavailable — keep the default.
    }
    if (raw) {
      const next = hydrateWorkspaceState(raw, { isChatValid }, freshId());
      this._state.set(next);
    }
    this._hydrated.set(true);
    // Persist the (possibly pruned) hydrated layout, mirroring v4's post-hydrate
    // persist effect. No-op until hydrated, which we just flipped.
    this.schedulePersist();
  }

  // -------------------------------------------------------------------------
  // Dispatch + persistence
  // -------------------------------------------------------------------------

  private dispatch(action: WorkspaceAction): void {
    this._state.update((s) => workspaceReducer(s, action));
    this.schedulePersist();
  }

  private schedulePersist(): void {
    // Never clobber a stored layout with the pre-hydration default.
    if (!this._hydrated()) return;
    if (this.persistTimer) clearTimeout(this.persistTimer);
    this.persistTimer = setTimeout(() => this.persistNow(), PERSIST_DEBOUNCE_MS);
  }

  private persistNow(): void {
    this.persistTimer = null;
    try {
      localStorage.setItem(this.storageKey, serializeWorkspaceState(this._state()));
    } catch {
      // Storage full/unavailable — non-fatal; layout simply won't persist.
    }
  }

  // -------------------------------------------------------------------------
  // WorkspaceHandle + actions
  // -------------------------------------------------------------------------

  /**
   * Open (or focus an existing) tab. De-dupes by kind+payload identity. Returns
   * the resulting tab id (the existing one when de-duped). The reducer is the
   * source of truth for de-dupe; the id resolved here from the current state is
   * only what we hand back to the caller.
   */
  openTab(kind: TabKind, payload?: unknown, opts: OpenTabOptions = {}): string {
    const identity = tabIdentity({ kind, payload });
    const existing = Object.values(this._state().tabs).find((t) => tabIdentity(t) === identity);
    const id = existing?.id ?? freshId();
    this.dispatch({
      type: 'OPEN_TAB',
      id,
      kind,
      payload,
      title: opts.title,
      icon: opts.icon,
      parentTabId: opts.parentTabId,
      pane: opts.pane,
      focus: opts.focus,
    });
    return id;
  }

  closeTab(id: string): void {
    this.dispatch({ type: 'CLOSE_TAB', id, homeFallbackId: freshId() });
  }

  /**
   * Refresh an open tab's payload/title in place — the v4 `OPEN_TAB` focus:false
   * payload-refresh path (e.g. a blank standalone document receiving its real
   * filePath after the server names it). No activation, no focus change.
   */
  refreshTab(kind: TabKind, payload: unknown, title?: string): void {
    this.openTab(kind, payload, { focus: false, title });
  }

  moveTab(id: string, toPane: PaneId, toIndex?: number): void {
    this.dispatch({ type: 'MOVE_TAB', id, toPane, toIndex });
  }

  reorderTab(id: string, toIndex: number): void {
    const pane: PaneId = this._state().panes.left.order.includes(id) ? 'left' : 'right';
    this.dispatch({ type: 'MOVE_TAB', id, toPane: pane, toIndex });
  }

  setActive(pane: PaneId, id: string): void {
    this.dispatch({ type: 'SET_ACTIVE', pane, id });
  }

  setFocusedPane(pane: PaneId): void {
    this.dispatch({ type: 'SET_FOCUSED_PANE', pane });
  }

  /** Move a tab into the other pane, creating the split if needed. */
  splitTo(id: string, pane: PaneId): void {
    this.dispatch({ type: 'MOVE_TAB', id, toPane: pane });
  }

  unsplit(): void {
    this.dispatch({ type: 'UNSPLIT' });
  }

  setSplitRatio(ratio: number): void {
    this.dispatch({ type: 'SET_SPLIT_RATIO', ratio });
  }

  /** Default title/icon for a kind (used by the chrome for rail-opened tabs). */
  metaFor(kind: TabKind) {
    return defaultTabMeta(kind);
  }
}
