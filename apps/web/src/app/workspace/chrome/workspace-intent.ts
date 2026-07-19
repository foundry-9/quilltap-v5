/**
 * Transient `?open=` intent parsing (port of v4 `WorkspaceIntent.tsx`, baseline
 * `b8b12695`). The host consumes a `/workspace?open=…` intent — opening (or
 * focusing) the requested tab — then strips the params to a clean `/workspace`.
 *
 * The v4 hydration race is real and documented: the host applies the intent only
 * AFTER localStorage hydration, or the hydrate `REPLACE_STATE` clobbers the
 * just-opened tab. This module is the pure parser; the host owns the timing.
 *
 * @module workspace/chrome/workspace-intent
 */

import { standaloneDocKey, type DocumentStandaloneTabPayload, type TabKind } from '../workspace-contract';

const OPENABLE_KINDS: ReadonlySet<TabKind> = new Set<TabKind>([
  'home',
  'salon',
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
  'settings-wizard',
  'custom-tools',
]);

const CHAT_KINDS: ReadonlySet<TabKind> = new Set<TabKind>(['salon', 'terminal', 'document']);

export interface OpenIntent {
  kind: TabKind;
  payload?: unknown;
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
  else if (kind === 'character-edit') payload = characterId ? { characterId, tab } : undefined;
  else if (kind === 'document-standalone') {
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

  // Chat-bound kinds need a chatId and the character editor needs a characterId.
  const missingChatId = CHAT_KINDS.has(kind) && !chatId;
  const missingCharacterId = kind === 'character-edit' && !characterId;
  if (missingChatId || missingCharacterId) return null;

  return { kind, payload };
}
