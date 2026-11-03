/**
 * The always-on Brahma Console launcher — a port of v4
 * `components/brahma-console/BrahmaConsoleLauncher.tsx` (`99463fc56`, at
 * `236c04e35`): a round button beside the left rail, in the topmost floating
 * layer so it stays reachable over any modal. It toggles the floating console.
 * NEVER disabled — "with no connection profile the console opens to a note
 * saying so" (v4 `:9-10`).
 *
 * Mounted by `brahma-entry.ts` (the shell's one Brahma mount) and, as v4
 * portals it, moved to `document.body` after its first render: in place, the
 * rail's stacking context let a modal's overlay cover it (measured live).
 *
 * @module brahma/brahma-console-launcher
 */

import {
  ChangeDetectionStrategy,
  Component,
  DestroyRef,
  ElementRef,
  afterNextRender,
  computed,
  inject,
} from '@angular/core';

import { Icon } from '../ui/icon';
import { BrahmaConsoleService } from './brahma-console.service';

@Component({
  selector: 'qt-brahma-console-launcher',
  changeDetection: ChangeDetectionStrategy.OnPush,
  imports: [Icon],
  template: `
    <button
      type="button"
      class="qt-brahma-launcher"
      [class.qt-brahma-launcher-active]="isOpen()"
      [title]="label()"
      [attr.aria-label]="label()"
      [attr.aria-pressed]="isOpen()"
      data-qt-floating-layer=""
      (click)="toggle()"
    >
      <qt-icon name="brahma-console" class="qt-brahma-launcher-icon" />
    </button>
  `,
})
export class BrahmaConsoleLauncher {
  private readonly service = inject(BrahmaConsoleService);

  protected readonly isOpen = this.service.isOpen;
  /** v4 `:24`. */
  protected readonly label = computed(() =>
    this.isOpen() ? 'Hide the Brahma Console' : 'Open the Brahma Console',
  );

  constructor() {
    // v4 `createPortal(<button …>, document.body)`.
    const host = inject<ElementRef<HTMLElement>>(ElementRef).nativeElement;
    afterNextRender(() => {
      if (host.parentElement !== document.body) document.body.appendChild(host);
    });
    inject(DestroyRef).onDestroy(() => host.remove());
  }

  protected toggle(): void {
    this.service.toggleConsole();
  }
}
