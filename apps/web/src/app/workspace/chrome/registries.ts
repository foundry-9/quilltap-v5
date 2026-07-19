/**
 * The two per-host registries (v4 `workspace-tab-context.tsx` portal registry +
 * `workspace-backdrop.tsx` backdrop registry). Provided at the workspace-host
 * level under the contract tokens; signal-backed so the host repaints reactively.
 *
 * @module workspace/chrome/registries
 */

import { Injectable, signal } from '@angular/core';

import type {
  WorkspaceBackdropEntry,
  WorkspaceBackdropRegistry,
  WorkspacePortalRegistry,
} from '../workspace-contract';

/**
 * Cross-tab portal registry: a child tab (terminal/document) registers its DOM
 * host node under `portalKey(...)`; the owning Salon reads the node and
 * relocates its live pane element into it (the DOM move preserves PTY/editor
 * state). v4 `WorkspacePortalRegistryProvider`.
 */
@Injectable()
export class WorkspacePortalRegistryImpl implements WorkspacePortalRegistry {
  private readonly _nodes = signal<Readonly<Record<string, HTMLElement | null>>>({});
  readonly nodes = this._nodes.asReadonly();

  setNode(key: string, node: HTMLElement | null): void {
    this._nodes.update((prev) => {
      if (prev[key] === node) return prev;
      const next = { ...prev };
      if (node) next[key] = node;
      else delete next[key];
      return next;
    });
  }
}

/**
 * Backdrop registry: views report their story/subsystem background; the host
 * paints one arbitrated backdrop. v4 `WorkspaceBackdropProvider`. The `entries`
 * signal is beyond the contract interface (the host component reads it).
 */
@Injectable()
export class WorkspaceBackdropRegistryImpl implements WorkspaceBackdropRegistry {
  private readonly _entries = signal<Readonly<Record<string, WorkspaceBackdropEntry>>>({});
  readonly entries = this._entries.asReadonly();

  report(tabId: string, entry: WorkspaceBackdropEntry): void {
    this._entries.update((prev) => {
      const existing = prev[tabId];
      if (existing && existing.url === entry.url && existing.isSalon === entry.isSalon) return prev;
      return { ...prev, [tabId]: entry };
    });
  }

  clear(tabId: string): void {
    this._entries.update((prev) => {
      if (!(tabId in prev)) return prev;
      const next = { ...prev };
      delete next[tabId];
      return next;
    });
  }
}
