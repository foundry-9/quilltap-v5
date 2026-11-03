/**
 * The Brahma Console rail entry (port of v4
 * `left-sidebar/sidebar-footer.tsx:213-226`).
 *
 * A self-contained sidebar-footer button that opens the console, plus the
 * floating console dialog and the always-on launcher (v4 `99463fc56`) it hosts
 * (so the shell mounts ONE component at the p4.9i1 unification — §W.2). The
 * rail button stays eligibility-gated (disabled without a connection profile —
 * still v4's rule at `236c04e35`, `sidebar-footer.tsx:214-226`); the launcher
 * never is. In the workspace it opens a `brahma` tab; otherwise it opens the
 * floating dialog (v4's `inWorkspace ? openTab('brahma') : openConsole()`).
 *
 * @module brahma/brahma-entry
 */

import { ChangeDetectionStrategy, Component, inject } from '@angular/core';
import { Router } from '@angular/router';

import { Icon } from '../ui/icon';
import { WorkspaceService } from '../workspace/workspace.service';
import { isWorkspaceTabsEnabled } from '../workspace/workspace-flag';
import { BrahmaConsoleDialog } from './brahma-console-dialog';
import { BrahmaConsoleLauncher } from './brahma-console-launcher';
import { BrahmaConsoleService } from './brahma-console.service';

@Component({
  selector: 'qt-brahma-entry',
  changeDetection: ChangeDetectionStrategy.OnPush,
  imports: [Icon, BrahmaConsoleDialog, BrahmaConsoleLauncher],
  template: `
    <button
      type="button"
      class="qt-collapsed-nav-button"
      [class.opacity-40]="!isEligible()"
      [class.cursor-not-allowed]="!isEligible()"
      [disabled]="!isEligible()"
      [title]="
        isEligible() ? 'Brahma Console' : 'Brahma Console (requires a connection profile)'
      "
      aria-label="Brahma Console"
      (click)="open()"
    >
      <qt-icon name="brahma-console" class="w-7 h-7" />
    </button>

    <!-- The floating console dialog (renders nothing while closed). Deferred:
         the dialog's message list pulls in the whole markdown render stack
         (unified + remark/rehype + katex + highlight.js, ~646 kB), and this
         entry is mounted eagerly by the shell — so a static reference put all
         of it in the INITIAL bundle. The console's own effects are already
         no-ops until isOpen flips, so gating the mount on the same signal
         costs nothing and defers the chunk to first open. -->
    @defer (when isConsoleOpen()) {
      <qt-brahma-console-dialog />
    }

    <!-- The always-on launcher (v4 99463fc56, app-layout.tsx:141-142: the
         dialog, then the launcher). Mounted here so the shell keeps its one
         Brahma mount; it is NOT deferred — it must be on screen from the start. -->
    <qt-brahma-console-launcher />
  `,
})
export class BrahmaEntry {
  private readonly service = inject(BrahmaConsoleService);
  private readonly workspace = inject(WorkspaceService);
  private readonly router = inject(Router);

  protected readonly isEligible = this.service.isEligible;
  /** The `@defer` trigger for the floating dialog (see the template). */
  protected readonly isConsoleOpen = this.service.isOpen;

  protected open(): void {
    if (isWorkspaceTabsEnabled() && this.router.url.split('?')[0] === '/workspace') {
      this.workspace.openTab('brahma');
      return;
    }
    this.service.openConsole();
  }
}
