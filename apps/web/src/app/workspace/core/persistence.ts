/**
 * Tabbed workspace — localStorage persistence (port of v4
 * `lib/workspace/workspace-persistence.ts`, baseline `b8b12695`).
 *
 * Serializes {@link WorkspaceState} to a per-instance localStorage key and
 * hydrates it back, validating the shape and pruning tabs that reference
 * entities which no longer exist (e.g. a persisted `chatId` whose chat was
 * deleted, or a terminal/document child tab whose parent Salon tab is gone).
 * No DB, no migration. Pure (no `window` access) so it can be corpus-diffed.
 *
 * v4 validates with Zod. v5 has no Zod, so the validation is hand-ported (the
 * Workbench client-safe schema-port precedent); the tier-1 corpus is the
 * equivalence proof. The two behaviours that MUST match Zod's default `.strip()`
 * — an unknown-kind or malformed tab drops only itself (never the whole saved
 * layout), and unknown keys on the state / a pane / a tab are stripped — are
 * pinned by dedicated corpus rows.
 *
 * @module workspace/core/persistence
 */

import {
  DEFAULT_SPLIT_RATIO,
  MAX_SPLIT_RATIO,
  MIN_SPLIT_RATIO,
  type PaneId,
  type PaneState,
  type TabKind,
  type WorkspaceState,
  type WorkspaceTab,
} from '../workspace-contract';
import { createInitialState } from './reducer';

/** localStorage key base. Scoped per instance by {@link workspaceStorageKey}. */
export const WORKSPACE_STORAGE_KEY_BASE = 'quilltap.workspace.layout';

/**
 * Per-instance storage key. The instance id keeps layouts from leaking between
 * instances sharing a browser origin; falls back to the unscoped base when no
 * instance id is available (the common single-instance case).
 */
export function workspaceStorageKey(instanceId?: string | null): string {
  return instanceId ? `${WORKSPACE_STORAGE_KEY_BASE}.${instanceId}` : WORKSPACE_STORAGE_KEY_BASE;
}

// Must list every `TabKind`. `satisfies` rejects typos/removed kinds. An
// unlisted kind drops only its own tab on deserialize, never the whole layout.
const TAB_KINDS = [
  'home',
  'salon',
  'salon-list',
  'terminal',
  'document',
  'aurora',
  'prospero',
  'scriptorium',
  'settings',
  'files',
  'photos',
  'scenarios',
  'brahma',
  'wardrobe',
  'profile',
  'about',
  'generate-image',
  'document-standalone',
  'character-new',
  'character-edit',
  'character-view',
  'settings-wizard',
  'custom-tools',
  // v5-only (P4.d16 tier 2): the hosted New-Chat screen. Listing it here is what
  // lets a persisted salon-new tab survive a reload.
  'salon-new',
] as const satisfies readonly TabKind[];
const TAB_KIND_SET = new Set<string>(TAB_KINDS);

export function serializeWorkspaceState(state: WorkspaceState): string {
  return JSON.stringify(state);
}

// --- hand-rolled shape validators (mirror the v4 Zod schemas exactly) -------

function isPlainObject(v: unknown): v is Record<string, unknown> {
  return typeof v === 'object' && v !== null && !Array.isArray(v);
}

/**
 * Validate + STRIP a pane (Zod `PaneStateSchema`: `order: string[]`,
 * `activeTabId: string | null`, unknown keys stripped). Returns null on any
 * violation.
 */
function validatePane(raw: unknown): PaneState | null {
  if (!isPlainObject(raw)) return null;
  const order = raw['order'];
  if (!Array.isArray(order) || !order.every((x) => typeof x === 'string')) return null;
  const activeTabId = raw['activeTabId'];
  if (!(activeTabId === null || typeof activeTabId === 'string')) return null;
  return { order: order as string[], activeTabId: activeTabId as string | null };
}

/**
 * Validate + STRIP a single tab (Zod `WorkspaceTabSchema`). Unknown keys are
 * dropped; `payload` is kept iff present (any value); `icon`/`parentTabId` are
 * kept iff present and a string. Returns null on any violation (the caller
 * drops just this tab).
 */
function validateTab(raw: unknown): WorkspaceTab | null {
  if (!isPlainObject(raw)) return null;
  const id = raw['id'];
  if (typeof id !== 'string' || id.length < 1) return null;
  const kind = raw['kind'];
  if (typeof kind !== 'string' || !TAB_KIND_SET.has(kind)) return null;
  const title = raw['title'];
  if (typeof title !== 'string') return null;
  // Optional string fields: present ⇒ must be a string.
  if ('icon' in raw && typeof raw['icon'] !== 'string') return null;
  if ('parentTabId' in raw && typeof raw['parentTabId'] !== 'string') return null;

  const tab: WorkspaceTab = { id, kind: kind as TabKind, title };
  if ('payload' in raw) tab.payload = raw['payload'];
  if ('icon' in raw) tab.icon = raw['icon'] as string;
  if ('parentTabId' in raw) tab.parentTabId = raw['parentTabId'] as string;
  return tab;
}

/**
 * Parse + shape-validate. Returns `null` only when the top-level structure is
 * malformed; individual tabs that fail validation (e.g. an unknown `kind` from
 * a newer or older build) are dropped rather than discarding the whole state.
 */
export function deserializeWorkspaceState(raw: string | null | undefined): WorkspaceState | null {
  if (!raw) return null;
  let parsed: unknown;
  try {
    parsed = JSON.parse(raw);
  } catch {
    return null;
  }
  if (!isPlainObject(parsed)) return null;

  // Outer shape (Zod `WorkspaceStateSchema`, unknown keys stripped).
  const rawTabs = parsed['tabs'];
  if (!isPlainObject(rawTabs)) return null;
  const rawPanes = parsed['panes'];
  if (!isPlainObject(rawPanes)) return null;
  const left = validatePane(rawPanes['left']);
  if (!left) return null;
  const rawRight = rawPanes['right'];
  let right: PaneState | null;
  if (rawRight === null) {
    right = null;
  } else {
    right = validatePane(rawRight);
    if (!right) return null;
  }
  const focusedPane = parsed['focusedPane'];
  if (focusedPane !== 'left' && focusedPane !== 'right') return null;
  const splitRatio = parsed['splitRatio'];
  // Zod `z.number()` accepts finite numbers and ±Infinity but rejects NaN.
  if (typeof splitRatio !== 'number' || Number.isNaN(splitRatio)) return null;

  const tabs: Record<string, WorkspaceTab> = {};
  for (const [id, rawTab] of Object.entries(rawTabs)) {
    const tab = validateTab(rawTab);
    if (tab) tabs[id] = tab;
  }

  return {
    tabs,
    panes: { left, right },
    focusedPane: focusedPane as PaneId,
    splitRatio,
  };
}

function clampRatio(ratio: number): number {
  if (!Number.isFinite(ratio)) return DEFAULT_SPLIT_RATIO;
  return Math.min(MAX_SPLIT_RATIO, Math.max(MIN_SPLIT_RATIO, ratio));
}

export interface PruneOptions {
  /** Returns whether a referenced chat still exists. Default: everything valid. */
  isChatValid?: (chatId: string) => boolean;
}

/**
 * Drop dead tabs and re-normalize the layout:
 * - Salon/terminal/document tabs whose chat is invalid are removed.
 * - Child tabs (terminal/document) orphaned by a removed parent are removed.
 * - Pane orders are filtered to surviving tabs; active references repaired.
 * - An emptied right pane collapses; an emptied left pane is filled from the
 *   right; if everything is gone, falls back to a fresh home tab.
 */
export function pruneWorkspaceState(
  state: WorkspaceState,
  opts: PruneOptions,
  homeFallbackId: string,
): WorkspaceState {
  const isChatValid = opts.isChatValid ?? (() => true);

  // 1. Tabs that survive on their own merits (chat existence).
  const surviving = new Set<string>();
  for (const tab of Object.values(state.tabs)) {
    let keep = true;
    if (tab.kind === 'salon' || tab.kind === 'terminal' || tab.kind === 'document') {
      const chatId = (tab.payload as { chatId?: string } | undefined)?.chatId;
      keep = Boolean(chatId) && isChatValid(chatId as string);
    }
    // Document tabs are keyed by an open document id; drop pre-multi-doc
    // document tabs that lack one (the Salon re-opens proper per-document tabs
    // from the server on mount).
    if (keep && tab.kind === 'document') {
      const chatDocumentId = (tab.payload as { chatDocumentId?: string } | undefined)
        ?.chatDocumentId;
      keep = Boolean(chatDocumentId);
    }
    // Standalone document tabs need a resolved file to reopen. A payload still
    // missing its filePath (a blank doc whose payload refresh never landed)
    // would mint a fresh untitled document on every reload — drop it instead.
    if (keep && tab.kind === 'document-standalone') {
      const payload = tab.payload as { docKey?: string; filePath?: string } | undefined;
      keep = Boolean(payload?.docKey && payload?.filePath);
    }
    if (keep) surviving.add(tab.id);
  }

  // 2. Drop child tabs whose parent did not survive (fixpoint for chains).
  let changed = true;
  while (changed) {
    changed = false;
    for (const tab of Object.values(state.tabs)) {
      if (!surviving.has(tab.id)) continue;
      if (tab.parentTabId && !surviving.has(tab.parentTabId)) {
        surviving.delete(tab.id);
        changed = true;
      }
    }
  }

  // 3. Rebuild tabs map (surviving + real objects only).
  const tabs: Record<string, WorkspaceTab> = {};
  for (const id of surviving) {
    if (state.tabs[id]) tabs[id] = state.tabs[id];
  }

  // 4. Rebuild pane orders, keeping original order, dropping non-survivors and
  //    ids with no backing tab.
  const leftOrder = state.panes.left.order.filter((id) => tabs[id]);
  const rightOrder = state.panes.right
    ? state.panes.right.order.filter((id) => tabs[id])
    : null;

  // Any surviving tab not placed in a pane (corruption) → append to left.
  const placed = new Set<string>([...leftOrder, ...(rightOrder ?? [])]);
  for (const id of Object.keys(tabs)) {
    if (!placed.has(id)) leftOrder.push(id);
  }

  const leftActive =
    state.panes.left.activeTabId && leftOrder.includes(state.panes.left.activeTabId)
      ? state.panes.left.activeTabId
      : (leftOrder[0] ?? null);
  let left: PaneState = { order: leftOrder, activeTabId: leftActive };

  let right: PaneState | null =
    rightOrder && rightOrder.length > 0
      ? {
          order: rightOrder,
          activeTabId:
            state.panes.right &&
            state.panes.right.activeTabId &&
            rightOrder.includes(state.panes.right.activeTabId)
              ? state.panes.right.activeTabId
              : (rightOrder[0] ?? null),
        }
      : null;

  let focusedPane: PaneId = state.focusedPane;
  if (focusedPane === 'right' && !right) focusedPane = 'left';

  // Promote right→left if the left pane emptied.
  if (left.order.length === 0 && right && right.order.length > 0) {
    left = right;
    right = null;
    focusedPane = 'left';
  }

  // Everything gone — home fallback.
  if (left.order.length === 0 && (!right || right.order.length === 0)) {
    return createInitialState(homeFallbackId);
  }

  return {
    tabs,
    panes: { left, right },
    focusedPane,
    splitRatio: clampRatio(state.splitRatio),
  };
}

/**
 * One-shot hydrate: deserialize, prune dead tabs, and fall back to a fresh home
 * tab when there is nothing usable. `homeFallbackId` must be a fresh uuid.
 */
export function hydrateWorkspaceState(
  raw: string | null | undefined,
  opts: PruneOptions,
  homeFallbackId: string,
): WorkspaceState {
  const parsed = deserializeWorkspaceState(raw);
  if (!parsed) return createInitialState(homeFallbackId);
  return pruneWorkspaceState(parsed, opts, homeFallbackId);
}
