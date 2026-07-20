/**
 * BrahmaConsoleView — the Brahma Console as a workspace tab (port of v4
 * `components/brahma-console/BrahmaConsoleView.tsx`).
 *
 * Reuses the dialog body via {@link BrahmaConsoleDialog}'s `asTab` mode, so the
 * console logic lives in one place and the floating-dialog path is unchanged.
 * The tab registry mounts THIS at the p4.9i1 unification (§W.2).
 *
 * @module brahma/brahma-console-view
 */

import { ChangeDetectionStrategy, Component } from '@angular/core';

import { BrahmaConsoleDialog } from './brahma-console-dialog';

@Component({
  selector: 'qt-brahma-console-view',
  changeDetection: ChangeDetectionStrategy.OnPush,
  imports: [BrahmaConsoleDialog],
  template: `<qt-brahma-console-dialog [asTab]="true" />`,
})
export class BrahmaConsoleView {}
