import { Injectable, signal, type TemplateRef } from '@angular/core';

import type { ConversationSearchSource } from '../chat/conversation-search';

/**
 * The page-toolbar slot service (v4
 * `components/providers/page-toolbar-provider.tsx` as a root signal service).
 * v4's `usePageToolbar`-throws / `usePageToolbarOptional` split collapses — a
 * root service always exists (the house divergence, same as the wardrobe
 * dialog). It carries two kinds of slot:
 *
 * **The conversation-search slot (P4.D384, v4 `4f5047241`) — HAS a consumer.**
 * The open Salon publishes a {@link ConversationSearchSource}; the toolbar's
 * `qt-search-bar` (and its dialog) searches that chat first, reveals a picked
 * match, and has the chat paint the query. v4 keeps the source per tab through
 * its `WorkspaceToolbarBridge` so it is always the FOCUSED chat's; v5 has no
 * bridge, so the Salon offers its source only while it is the focused pane's
 * active tab (outside the workspace: always) and clears it on hide / blur /
 * destroy. Because several Salons can be alive at once (kept-alive tabs), the
 * clear is OWNER-ONLY — {@link clearConversationSearch} is a no-op unless the
 * slot still holds that very source — so a Salon going to the background after
 * another took focus can never wipe the focused one's.
 *
 * **The left / right `TemplateRef` slots — NO CONSUMER YET** (deliberate — the
 * P4.9P tier-2 ruling): v4's ONE consumer is `SalonView`, whose content v5
 * already renders inline as `qt-conversation-header`, and v4's slot mechanism
 * only works in its tabbed shell through the per-tab bridge
 * (`components/workspace/tab-toolbar.tsx` — the focused tab's slots win).
 * Moving the Salon header into the global slots WITHOUT that bridge would show
 * one chat's header while another chat's tab is focused, so the inline header
 * stays (a documented divergence, recorded in the lane record +
 * `m6-screen-parity.md` is the unifier's to update); the slots land so the
 * toolbar's slot surface exists for the follow-up that ports the bridge. A page
 * that sets one MUST clear it on destroy (v4's slot content dies with the page
 * component that set it; here that is an explicit
 * `DestroyRef.onDestroy(() => svc.clearLeft/Right())`).
 */
@Injectable({ providedIn: 'root' })
export class PageToolbarService {
  private readonly _leftContent = signal<TemplateRef<unknown> | null>(null);
  private readonly _rightContent = signal<TemplateRef<unknown> | null>(null);
  private readonly _conversationSearch = signal<ConversationSearchSource | null>(null);

  readonly leftContent = this._leftContent.asReadonly();
  readonly rightContent = this._rightContent.asReadonly();
  /** The focused Salon chat's search source, or null when no chat is focused. */
  readonly conversationSearch = this._conversationSearch.asReadonly();

  setLeftContent(content: TemplateRef<unknown> | null): void {
    this._leftContent.set(content);
  }

  setRightContent(content: TemplateRef<unknown> | null): void {
    this._rightContent.set(content);
  }

  clearLeft(): void {
    this._leftContent.set(null);
  }

  clearRight(): void {
    this._rightContent.set(null);
  }

  /** Offer (or, with null, withdraw unconditionally) the conversation-search source. */
  setConversationSearch(source: ConversationSearchSource | null): void {
    this._conversationSearch.set(source);
  }

  /**
   * Withdraw `source` — but ONLY if the slot still holds it (the same object
   * for the same chat). A no-op when another Salon has since taken the slot.
   * Returns whether the slot was cleared.
   */
  clearConversationSearch(source: ConversationSearchSource): boolean {
    const current = this._conversationSearch();
    if (current === null || current !== source || current.chatId !== source.chatId) return false;
    this._conversationSearch.set(null);
    return true;
  }
}
