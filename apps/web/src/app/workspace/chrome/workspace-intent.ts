/**
 * Transient `?open=` intent parsing (port of v4 `WorkspaceIntent.tsx`, baseline
 * `e646f58b`). The host consumes a `/workspace?open=…` intent — opening (or
 * focusing) the requested tab — then strips the params to a clean `/workspace`.
 *
 * The v4 hydration race is real and documented: the host applies the intent only
 * AFTER localStorage hydration, or the hydrate `REPLACE_STATE` clobbers the
 * just-opened tab. This module is the pure parser; the host owns the timing.
 *
 * @module workspace/chrome/workspace-intent
 */

import {
  standaloneDocKey,
  type DocumentStandaloneTabPayload,
  type TabKind,
  type WorkspaceHandle,
} from '../workspace-contract';

const OPENABLE_KINDS: ReadonlySet<TabKind> = new Set<TabKind>([
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
]);

const CHAT_KINDS: ReadonlySet<TabKind> = new Set<TabKind>(['salon', 'terminal', 'document']);

export interface OpenIntent {
  kind: TabKind;
  payload?: unknown;
  /**
   * Opened FIRST; the resulting tab id becomes this intent's `parentTabId`
   * (v4 `8d86847a`'s terminal two-step — see {@link parseOpenIntent}). Absent
   * for every other kind.
   */
  parent?: { kind: TabKind; payload?: unknown };
}

/** A minimal read-only params view (URLSearchParams or Angular's ParamMap). */
export interface ParamReader {
  get(name: string): string | null;
}

/**
 * Parse the `?open=…` intent into an openTab call, or `null` when there is no
 * openable intent (unknown kind, or a chat-bound / character-editor kind whose
 * required id is missing — the v4 skip arms).
 */
export function parseOpenIntent(params: ParamReader): OpenIntent | null {
  const open = params.get('open');
  if (!open) return null;
  const kind = open as TabKind;
  if (!OPENABLE_KINDS.has(kind)) return null;

  const chatId = params.get('chatId') || undefined;
  const tab = params.get('tab') || undefined;
  const section = params.get('section') || undefined;
  const characterId = params.get('characterId') || undefined;

  let payload: unknown;
  if (CHAT_KINDS.has(kind)) payload = chatId ? { chatId } : undefined;
  else if (kind === 'settings') payload = { tab, section };
  else if (kind === 'custom-tools') {
    const mountPointId = params.get('mount') || undefined;
    const path = params.get('path') || undefined;
    const create = params.get('new') === '1' || undefined;
    payload = mountPointId || path || create ? { mountPointId, path, create } : undefined;
  } else if (kind === 'wardrobe') payload = characterId ? { characterId } : undefined;
  else if (kind === 'character-edit' || kind === 'character-view')
    payload = characterId ? { characterId, tab } : undefined;
  else if (kind === 'prospero') {
    const projectId = params.get('projectId') || undefined;
    payload = projectId ? { projectId } : undefined;
  } else if (kind === 'scriptorium') {
    const storeId = params.get('storeId') || undefined;
    payload = storeId ? { storeId } : undefined;
  } else if (kind === 'aurora') {
    const groupId = params.get('groupId') || undefined;
    payload = groupId ? { groupId } : undefined;
  } else if (kind === 'document-standalone') {
    const scope: DocumentStandaloneTabPayload['scope'] =
      params.get('scope') === 'document_store' ? 'document_store' : 'general';
    const filePath = params.get('filePath') || undefined;
    const mountPoint = params.get('mountPoint') || undefined;
    const targetFolder = params.get('targetFolder') || undefined;
    payload = {
      docKey: standaloneDocKey(scope, mountPoint ?? null, filePath),
      scope,
      mountPoint: mountPoint ?? null,
      filePath,
      targetFolder,
    } satisfies DocumentStandaloneTabPayload;
  }

  // Chat-bound kinds need a chatId and the character editor/detail needs a
  // characterId; skip opening when the required id is missing.
  const missingChatId = CHAT_KINDS.has(kind) && !chatId;
  const missingCharacterId =
    (kind === 'character-edit' || kind === 'character-view') && !characterId;
  if (missingChatId || missingCharacterId) return null;

  if (kind === 'terminal' && chatId) {
    // A terminal tab is a portal fed by its Salon view — open (and mount) the
    // conversation first, then the terminal as its child tab.
    const sessionId = params.get('sessionId') || undefined;
    return {
      kind: 'terminal',
      payload: { chatId, sessionId },
      parent: { kind: 'salon', payload: { chatId } },
    };
  }

  return { kind, payload };
}

/**
 * Apply a parsed intent to the workspace store, honouring the optional `parent`
 * two-step (v4 `8d86847a`: the terminal deep link opens the Salon parent FIRST
 * — it is the portal source for the live PTY — then the terminal as its child).
 * Returns the id of the tab the intent targeted.
 */
export function applyOpenIntent(handle: WorkspaceHandle, intent: OpenIntent): string {
  if (!intent.parent) return handle.openTab(intent.kind, intent.payload);
  const parentTabId = handle.openTab(intent.parent.kind, intent.parent.payload);
  return handle.openTab(intent.kind, intent.payload, { parentTabId });
}
