import { ChangeDetectionStrategy, Component, input, output } from '@angular/core';

import { Icon } from '../ui/icon';
import { Modal } from '../ui/modal';
import type { PtySessionMeta } from './terminal-protocol';

/**
 * `qt-terminal-session-picker` — pick a live terminal session to bind to
 * Terminal Mode, or spawn a new one (v4 `TerminalSessionPicker`). Shown only
 * when there are existing live sessions for the chat; otherwise the Salon spawns
 * directly.
 */
@Component({
  selector: 'qt-terminal-session-picker',
  changeDetection: ChangeDetectionStrategy.OnPush,
  imports: [Modal, Icon],
  template: `
    <qt-modal title="Open Terminal Mode" maxWidth="md" (close)="close.emit()">
      <div class="flex flex-col gap-3">
        <p class="text-sm qt-text-secondary">
          Pick a running terminal to bring into Terminal Mode, or fire up a fresh one.
        </p>

        <ul class="flex flex-col gap-2">
          @for (session of sessions(); track session.id) {
            <li>
              <button
                type="button"
                class="qt-list-row w-full text-left flex flex-col gap-0.5 px-3 py-2 rounded border qt-border-default hover:qt-bg-muted"
                (click)="attach.emit(session.id)"
              >
                <span class="text-sm font-medium truncate">
                  {{ session.label || 'Terminal — ' + session.shell }}
                </span>
                <span class="text-xs qt-text-secondary truncate">{{ session.cwd }}</span>
                <span class="text-xs qt-text-secondary">
                  Started {{ formatStartedAt(session.startedAt) }}
                </span>
              </button>
            </li>
          }

          <li>
            <button
              type="button"
              class="qt-list-row w-full text-left flex items-center gap-2 px-3 py-2 rounded border qt-border-default hover:qt-bg-muted"
              (click)="spawnNew.emit()"
            >
              <qt-icon name="plus" class="w-4 h-4" />
              <span class="text-sm font-medium">New terminal</span>
            </button>
          </li>
        </ul>
      </div>
    </qt-modal>
  `,
})
export class TerminalSessionPicker {
  readonly sessions = input.required<PtySessionMeta[]>();

  readonly attach = output<string>();
  readonly spawnNew = output<void>();
  readonly close = output<void>();

  protected formatStartedAt(iso: string): string {
    const date = new Date(iso);
    return Number.isNaN(date.getTime()) ? iso : date.toLocaleString();
  }
}
