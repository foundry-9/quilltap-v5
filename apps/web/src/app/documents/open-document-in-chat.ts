/**
 * The one client-side choreography for opening a document *inside a chat* (v4
 * `lib/documents/open-document-in-chat.ts`, NEW at `b220999d`).
 *
 * Three steps that must always happen together:
 *
 *  1. `chatDocumentOpen` — creates (or reactivates) the `chat_documents` row and
 *     posts the Librarian's "opened" announcement.
 *  2. Inside the workspace shell, open the `document` tab for the returned
 *     `chatDocumentId` so the editor is visible immediately (`openTab` de-dupes
 *     on kind+payload identity, so the Salon's own tab reconciliation is a no-op
 *     afterwards).
 *  3. Dispatch `quilltap:document-opened` so a mounted Salon reconciles its
 *     open-document set and focuses the new pane without a manual refresh.
 *
 * In-chat opens are chat-visible **on purpose** — the Librarian announces the
 * open and every later save, exactly as for a picker-opened document. The silent
 * alternative is standalone Document Mode (`document-standalone`), which touches
 * no chat at all.
 *
 * ⚠ **v4's `standaloneTabPayload()` export is deliberately NOT ported.** It is
 * dead in `b220999d` itself — the search hook builds its standalone payload
 * inline — and inventing a caller would be inventing behaviour.
 *
 * @module documents/open-document-in-chat
 */

import type { DocumentApi } from './document-api';
import type { TabKind } from '../workspace/workspace-contract';

/** Scope of the document being opened, as the chat API understands it. */
export type ChatDocumentScope = 'document_store' | 'project' | 'general';

/**
 * The event a chat-side open dispatches so a mounted Salon can reconcile.
 *
 * v4 spells it `qtap-document-opened`; v5's window events all carry the
 * `quilltap:` prefix (`quilltap:chat-update`, `quilltap:terminal-exited`), and
 * the name is private to this app — no v4 bytes cross the wire — so the SPA's
 * own convention wins. A DOCUMENTED naming divergence, not a behavioural one.
 */
export const DOCUMENT_OPENED_EVENT = 'quilltap:document-opened';

export interface DocumentOpenedDetail {
  chatId: string;
  chatDocumentId: string;
}

export interface OpenDocumentInChatParams {
  filePath: string;
  scope: ChatDocumentScope;
  /** Store name or UUID; required for `document_store` scope. */
  mountPoint?: string | null;
  /** Split (side by side) or focus (document alone). Defaults to `split`. */
  mode?: 'split' | 'focus';
}

export interface OpenDocumentInChatDeps {
  /**
   * The workspace's `openTab`, when the caller is inside the workspace shell.
   * Omitted (or null) in the legacy shell, where the Salon renders the document
   * pane itself off the dispatched event.
   */
  openTab?:
    | ((
        kind: TabKind,
        payload?: unknown,
        opts?: { focus?: boolean; parentTabId?: string; title?: string },
      ) => string)
    | null;
  /** The Salon tab the document belongs under, when it is known. */
  parentTabId?: string;
}

/**
 * Open `params` as a document of `chatId`. Resolves once the chat row exists and
 * the UI has been told about it; rejects with the server's message when the open
 * fails, so callers can surface a toast.
 */
export async function openDocumentInChat(
  api: DocumentApi,
  chatId: string,
  params: OpenDocumentInChatParams,
  deps: OpenDocumentInChatDeps = {},
): Promise<void> {
  const data = await api.openDocument(chatId, {
    filePath: params.filePath,
    scope: params.scope,
    mountPoint: params.mountPoint ?? undefined,
    mode: params.mode ?? 'split',
  });

  deps.openTab?.(
    'document',
    {
      chatId,
      chatDocumentId: data.document.id,
      displayTitle: data.document.displayTitle,
    },
    { focus: true, parentTabId: deps.parentTabId },
  );

  if (typeof window !== 'undefined') {
    window.dispatchEvent(
      new CustomEvent<DocumentOpenedDetail>(DOCUMENT_OPENED_EVENT, {
        detail: { chatId, chatDocumentId: data.document.id },
      }),
    );
  }
}
