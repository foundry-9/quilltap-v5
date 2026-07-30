import { Injectable, signal, type TemplateRef } from '@angular/core';

/**
 * The page-toolbar slot service (v4
 * `components/providers/page-toolbar-provider.tsx` as a root signal service):
 * a page registers a `TemplateRef` for the toolbar's left and/or right
 * section; the toolbar renders whatever is registered. v4's
 * `usePageToolbar`-throws / `usePageToolbarOptional` split collapses — a root
 * service always exists (the house divergence, same as the wardrobe dialog).
 *
 * A page that sets a slot MUST clear it on destroy (v4's slot content dies
 * with the page component that set it; here that is an explicit
 * `DestroyRef.onDestroy(() => svc.clearLeft/Right())`).
 *
 * ⚠ NO CONSUMER YET (deliberate — the P4.9P tier-2 ruling): v4's ONE slot
 * consumer is `SalonView`, whose content v5 already renders inline as
 * `qt-conversation-header`, and v4's slot mechanism only works in its tabbed
 * shell through a per-tab bridge (`components/workspace/tab-toolbar.tsx` —
 * the focused tab's slots win) that v5 has not ported. Moving the Salon
 * header into the global slots WITHOUT that bridge would show one chat's
 * header while another chat's tab is focused, so the inline header stays (a
 * documented divergence, recorded in the lane record + `m6-screen-parity.md`
 * is the unifier's to update); the service lands so the toolbar's slot
 * surface exists for the follow-up that ports the bridge.
 */
@Injectable({ providedIn: 'root' })
export class PageToolbarService {
  private readonly _leftContent = signal<TemplateRef<unknown> | null>(null);
  private readonly _rightContent = signal<TemplateRef<unknown> | null>(null);

  readonly leftContent = this._leftContent.asReadonly();
  readonly rightContent = this._rightContent.asReadonly();

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
}
