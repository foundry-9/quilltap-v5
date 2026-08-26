/**
 * Click behaviour for a **Documents** result in the global search bar (v4
 * `lib/hooks/use-open-document-from-search.ts`, NEW at `b220999d` — a React hook
 * there, an injectable service here). Where the document lands depends on what
 * the user is looking at:
 *
 * - **A Salon is focused** → open it *in that chat*, exactly as the composer's
 *   document picker would: the Librarian announces the open, and the chat sees
 *   later saves. See {@link openDocumentInChat}.
 * - **Inside the workspace, no Salon focused** → open a `document-standalone`
 *   tab in place. Standalone Document Mode touches no `chat_documents` row, so
 *   no conversation is told of the open or of any edit.
 * - **Outside the workspace shell** → navigate to the result's own URL, which is
 *   the same standalone deep link `WorkspaceIntent` consumes on arrival.
 *
 * Modified clicks (⌘/ctrl/shift/alt, middle button) are left alone so the browser
 * opens the anchor's href — the silent standalone link — in a new tab. That is
 * also what a JS-free open does, which is why the server hands out the standalone
 * URL rather than a chat one: the default can never surprise a conversation.
 *
 * @module documents/open-document-from-search
 */

import { inject, Injectable, Injector, runInInjectionContext } from '@angular/core';
import { Router } from '@angular/router';

import { ToastService } from '../ui/toast.service';
import {
  standaloneDocKey,
  type DocumentStandaloneTabPayload,
  type WorkspaceState,
} from '../workspace/workspace-contract';
import { isWorkspaceTabsEnabled } from '../workspace/workspace-flag';
import { WorkspaceService } from '../workspace/workspace.service';
import type { DocumentSearchResultItem } from '../search/search.types';
import { DocumentApi } from './document-api';
import { openDocumentInChat } from './open-document-in-chat';

/** The workspace-focused Salon, plus the tab it lives in. */
export interface ActiveSalon {
  chatId: string;
  /** The Salon tab's id, so an opened document can be parented to it. */
  tabId: string | null;
}

/**
 * Which conversation an "open in the chat" would land in, if any (v4
 * `resolveActiveSalon`).
 *
 * Follows **focus**, not mere presence: only the focused pane's active tab
 * counts, so a Salon idling in the other pane never captures the open. When
 * workspace state is supplied there is **no fall through to the pathname** —
 * a workspace with no Salon focused means "no chat", full stop. Outside the
 * workspace shell the pathname decides (`/salon/<id>`).
 *
 * Pure — the service is a thin wrapper so this stays directly testable.
 */
/**
 * The path half of `Router.url`. v4 reads `usePathname()`, which never carries
 * a query OR a fragment — `router.url` can carry both, so both are stripped
 * (a `/salon/abc#x` URL must still match the salon arm; unify §3).
 */
function pathnameOf(url: string): string {
  return url.split('?')[0].split('#')[0];
}

export function resolveActiveSalon(
  state: WorkspaceState | null | undefined,
  pathname: string | null,
): ActiveSalon | null {
  if (state) {
    const activeTabId = state.panes[state.focusedPane]?.activeTabId ?? null;
    const tab = activeTabId ? state.tabs[activeTabId] : undefined;
    if (tab && tab.kind === 'salon') {
      const chatId = (tab.payload as { chatId?: unknown } | undefined)?.chatId;
      if (typeof chatId === 'string' && chatId.length > 0) {
        return { chatId, tabId: tab.id };
      }
    }
    return null;
  }

  const match = pathname?.match(/^\/salon\/([^/?#]+)$/);
  if (!match) return null;
  const chatId = decodeURIComponent(match[1]);
  if (!chatId || chatId === 'new') return null;
  return { chatId, tabId: null };
}

/**
 * True when the click should be left to the browser (new tab / new window) —
 * v4 `isModifiedClick`. Note it returns WITHOUT `preventDefault`, so the anchor's
 * standalone href is what opens.
 */
export function isModifiedClick(event: MouseEvent): boolean {
  return (
    event.button !== 0 || event.metaKey || event.ctrlKey || event.shiftKey || event.altKey
  );
}

@Injectable({ providedIn: 'root' })
export class OpenDocumentFromSearch {
  private readonly workspace = inject(WorkspaceService);
  private readonly router = inject(Router);
  private readonly toasts = inject(ToastService);
  /**
   * `DocumentApi` is `@Injectable()` WITHOUT `providedIn: 'root'` — it is
   * provided by the Salon conversation that owns a chat
   * (`salon-conversation.ts:219`). Resolving it in a field initializer would
   * make merely RENDERING a search result list demand a provider the search
   * surface has no business requiring (and it did: every existing
   * `SearchResults` spec went NG0201 on the first attempt).
   *
   * ⚠ Nor can it simply be `get()` off this injector: THIS service is
   * `providedIn: 'root'`, so `inject(Injector)` hands back the ROOT injector,
   * which never sees a component's `providers`. A lazy `injector.get(DocumentApi)`
   * therefore threw NG0201 on every in-chat open — dogfood finding #105, where
   * clicking a Documents search result with a Salon focused did nothing at all.
   *
   * `DocumentApi` is a stateless wrapper over the root `CoreClient`, so the fix
   * is to build our OWN instance in the root injection context, memoized. It is
   * deliberately NOT registered globally: `document-picker.ts:335` injects
   * `DocumentApi` `{optional: true}` and relies on it being ABSENT outside a
   * chat to fall back to `StandaloneDocumentApi`.
   */
  private readonly injector = inject(Injector);
  private documentApi: DocumentApi | null = null;

  /** The chat-side document client, built on first in-chat open. */
  private chatDocumentApi(): DocumentApi {
    this.documentApi ??= runInInjectionContext(this.injector, () => new DocumentApi());
    return this.documentApi;
  }

  /**
   * v4's `inWorkspace = ws !== null && pathname === '/workspace'`. v5's
   * workspace shell is behind a flag AND a route, so both are checked — the
   * same pair `documents-rail-entry` uses.
   */
  private inWorkspace(): boolean {
    return isWorkspaceTabsEnabled() && pathnameOf(this.router.url) === '/workspace';
  }

  /** The Salon an in-chat open would land in, if any. */
  activeSalon(): ActiveSalon | null {
    const inWorkspace = this.inWorkspace();
    return resolveActiveSalon(
      inWorkspace ? this.workspace.state() : null,
      pathnameOf(this.router.url),
    );
  }

  /**
   * The result card's click handler. Decides between the in-chat and the silent
   * standalone open, and leaves modified clicks to the browser.
   */
  open(result: DocumentSearchResultItem, event: MouseEvent): void {
    if (isModifiedClick(event)) return;
    event.preventDefault();

    const inWorkspace = this.inWorkspace();
    const activeSalon = this.activeSalon();

    if (activeSalon) {
      void openDocumentInChat(
        this.chatDocumentApi(),
        activeSalon.chatId,
        {
          filePath: result.relativePath,
          scope: 'document_store',
          mountPoint: result.mountPointRef,
          mode: 'split',
        },
        {
          openTab: inWorkspace ? (k, p, o) => this.workspace.openTab(k, p, o) : null,
          parentTabId: activeSalon.tabId ?? undefined,
        },
      ).catch((error: unknown) => {
        this.toasts.showError(
          error instanceof Error ? error.message : 'Failed to open document',
        );
      });
      return;
    }

    if (inWorkspace) {
      const payload: DocumentStandaloneTabPayload = {
        docKey: standaloneDocKey('document_store', result.mountPointRef, result.relativePath),
        scope: 'document_store',
        mountPoint: result.mountPointRef,
        filePath: result.relativePath,
        displayTitle: result.name,
      };
      this.workspace.openTab('document-standalone', payload, { title: result.name });
      return;
    }

    // Legacy shell: the result's URL is the `?open=document-standalone` intent,
    // which mints the tab (and its docKey) once the workspace mounts.
    void this.router.navigateByUrl(result.url);
  }
}
