/**
 * `HelpNavigate` — the v5 twin of v4's `useWorkspaceNavigate`
 * (`components/workspace/useWorkspaceNavigate.ts`).
 *
 * A keep-alive-safe replacement for a bare route change. Programmatic
 * navigations from the Help dialog do not go through an `<a>`, so the
 * workspace's delegated link interceptor never sees them: inside the workspace,
 * a href that maps to a tab opens/focuses that tab IN PLACE (no remount — a
 * streaming Salon survives an operator following a help link); otherwise, and
 * anywhere outside the workspace, it falls back to a normal navigation.
 *
 * v5 has no shared twin of this hook today — the two existing call sites inline
 * it (`brahma/brahma-entry.ts`, `documents/documents-rail-entry.ts`) — so it is
 * introduced here, in this family's own folder, rather than by editing theirs.
 *
 * @module help/help-navigate
 */

import { Injectable, inject } from '@angular/core';
import { Router } from '@angular/router';

import { parseHrefToIntent } from '../workspace/core/route-to-intent';
import { isWorkspaceTabsEnabled } from '../workspace/workspace-flag';
import { WorkspaceService } from '../workspace/workspace.service';

@Injectable({ providedIn: 'root' })
export class HelpNavigate {
  private readonly workspace = inject(WorkspaceService);
  private readonly router = inject(Router);

  /** Open `href` as a workspace tab when that is possible, else navigate. */
  go(href: string): void {
    // v4 gates on `pathname === '/workspace'`; `Router.url` carries the query,
    // so the path half is the equivalent read (the established mapping — see
    // `brahma/brahma-entry.ts`).
    if (isWorkspaceTabsEnabled() && this.router.url.split('?')[0] === '/workspace') {
      const intent = parseHrefToIntent(href);
      if (intent) {
        this.workspace.openTab(intent.kind, intent.payload);
        return;
      }
    }
    void this.router.navigateByUrl(href);
  }
}
