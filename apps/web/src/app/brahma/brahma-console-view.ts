/**
 * BrahmaConsoleView — the Brahma Console as a workspace tab (a port of v4
 * `components/brahma-console/BrahmaConsoleView.tsx`, `99463fc56`, at
 * `236c04e35`): the shared {@link BrahmaConsolePanel} rendered bare, with its
 * header chrome in an inline right-aligned row, on the TAB's own session
 * (`quilltap:brahma-console-last-id`) — so the tab and the floating dialog can
 * sit on two conversations at once. Always active (the tab is live whether or
 * not the floating dialog is open). The tab registry mounts THIS.
 *
 * @module brahma/brahma-console-view
 */

import { ChangeDetectionStrategy, Component, inject } from '@angular/core';

import { BrahmaConsoleHeaderActions, BrahmaConsolePanel } from './brahma-console-panel';
import { STORAGE_KEY_TAB_LAST_CHAT, createBrahmaConsoleSession } from './brahma-console-session';
import { BrahmaConsoleApi } from './brahma-wire';

@Component({
  selector: 'qt-brahma-console-view',
  changeDetection: ChangeDetectionStrategy.OnPush,
  imports: [BrahmaConsolePanel, BrahmaConsoleHeaderActions],
  template: `
    <div class="flex flex-col h-full min-h-0">
      @if (panel.isEligible()) {
        <div class="flex items-center justify-end gap-1 px-3 py-2 border-b qt-border-default/60">
          <qt-brahma-console-header-actions [panel]="panel" />
        </div>
      }
      <qt-brahma-console-panel #panel [session]="session" [active]="true" />
    </div>
  `,
})
export class BrahmaConsoleView {
  protected readonly session = createBrahmaConsoleSession(
    STORAGE_KEY_TAB_LAST_CHAT,
    inject(BrahmaConsoleApi),
  );
}
