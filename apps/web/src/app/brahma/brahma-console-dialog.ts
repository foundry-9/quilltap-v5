/**
 * BrahmaConsoleDialog — the always-on floating Brahma Console (a port of v4
 * `components/brahma-console/BrahmaConsoleDialog.tsx`, `99463fc56`, at
 * `236c04e35`): the shared {@link BrahmaConsolePanel} inside a topmost
 * {@link FloatingDialog}. It is toggled by the launcher button and sits above
 * every modal, so the operator can consult the engine in the middle of
 * filling out a form.
 *
 * The panel is projected into the dialog, so it — its session, and any reply
 * still streaming — survives the dialog being hidden; reopening returns to the
 * same conversation. The floating session keeps its own remembered
 * conversation (`quilltap:brahma-floating-last-id`), apart from the workspace
 * tab's.
 *
 * **Recorded divergence `CONSOLE_FIRST_MOUNT_DEFERRED`:** v4 mounts the dialog
 * with the layout; v5's `brahma-entry.ts` defers its FIRST mount to the first
 * open (the markdown render stack stays out of the initial bundle). From then
 * on it stays mounted, v4's shape — a reply streaming before the console was
 * ever opened cannot exist.
 *
 * @module brahma/brahma-console-dialog
 */

import { ChangeDetectionStrategy, Component, inject } from '@angular/core';

import { FloatingDialog } from '../ui/floating-dialog';
import { BrahmaConsoleHeaderActions, BrahmaConsolePanel } from './brahma-console-panel';
import {
  STORAGE_KEY_FLOATING_LAST_CHAT,
  createBrahmaConsoleSession,
} from './brahma-console-session';
import { BrahmaConsoleService } from './brahma-console.service';
import { BrahmaConsoleApi } from './brahma-wire';

@Component({
  selector: 'qt-brahma-console-dialog',
  changeDetection: ChangeDetectionStrategy.OnPush,
  imports: [FloatingDialog, BrahmaConsolePanel, BrahmaConsoleHeaderActions],
  template: `
    <qt-floating-dialog
      [open]="isOpen()"
      title="Brahma Console"
      storageKey="quilltap:brahma-console-geometry"
      [initialGeometry]="initialGeometry"
      [minWidth]="480"
      [minHeight]="400"
      topmost
      (close)="close()"
    >
      <qt-brahma-console-header-actions qt-floating-dialog-actions [panel]="panel" />
      <qt-brahma-console-panel #panel [session]="session" [active]="isOpen()" />
    </qt-floating-dialog>
  `,
})
export class BrahmaConsoleDialog {
  private readonly service = inject(BrahmaConsoleService);

  protected readonly isOpen = this.service.isOpen;
  /** v4 `initialGeometry={{ width: 560 }}`. */
  protected readonly initialGeometry = { width: 560 };
  /** The floating surface's own conversation (v4 `useBrahmaConsoleSession(…FLOATING…)`). */
  protected readonly session = createBrahmaConsoleSession(
    STORAGE_KEY_FLOATING_LAST_CHAT,
    inject(BrahmaConsoleApi),
  );

  protected close(): void {
    this.service.closeConsole();
  }
}
