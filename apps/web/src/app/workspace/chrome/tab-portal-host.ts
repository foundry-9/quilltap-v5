/**
 * Terminal / Document child-tab hosts (port of v4 `TerminalDocumentViews.tsx`,
 * baseline `b8b12695`).
 *
 * These render only a portal *host*. The live Terminal (Ariel PTY) and Document
 * (Librarian editor) panes are owned by the parent Salon view (lane J2's portal
 * SOURCE) and relocated into this host's mount node — keeping the PTY/editor
 * mounted inside the kept-alive Salon subtree while appearing in their own tab,
 * possibly in the other pane. Until a source registers, the empty copy shows.
 *
 * @module workspace/chrome/tab-portal-host
 */

import {
  ChangeDetectionStrategy,
  Component,
  ElementRef,
  computed,
  effect,
  inject,
  input,
  viewChild,
} from '@angular/core';

import { WORKSPACE_PORTAL_REGISTRY, portalKey } from '../workspace-contract';

@Component({
  selector: 'qt-tab-portal-host',
  changeDetection: ChangeDetectionStrategy.OnPush,
  template: `
    <div class="qt-tab-portal-host">
      <div class="qt-tab-portal-mount" #mount></div>
      @if (!hasSource()) {
        <div class="qt-tab-portal-empty">
          <p class="qt-text-muted text-sm">
            Open this conversation in the workspace to bring its
            {{ kind() === 'terminal' ? 'terminal' : 'document' }} to life here.
          </p>
        </div>
      }
    </div>
  `,
})
export class TabPortalHost {
  readonly kind = input.required<'terminal' | 'document'>();
  readonly chatId = input.required<string>();
  readonly docId = input<string | undefined>(undefined);

  private readonly registry = inject(WORKSPACE_PORTAL_REGISTRY, { optional: true });
  private readonly mount = viewChild.required<ElementRef<HTMLElement>>('mount');

  protected readonly key = computed(() => portalKey(this.kind(), this.chatId(), this.docId()));
  protected readonly hasSource = computed(() => (this.registry?.nodes()[this.key()] ?? null) != null);

  constructor() {
    // Register the mount node under the portal key; clear on key change / destroy.
    effect((onCleanup) => {
      const el = this.mount().nativeElement;
      const key = this.key();
      this.registry?.setNode(key, el);
      onCleanup(() => this.registry?.setNode(key, null));
    });
  }
}
