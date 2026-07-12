import {
  ChangeDetectionStrategy,
  Component,
  DestroyRef,
  effect,
  inject,
  Injector,
  input,
  OnInit,
  output,
  signal,
} from '@angular/core';

import { Icon } from '../ui/icon';
import { Terminal } from './terminal';
import { TerminalSessionService, type TerminalSessionHandle } from './terminal-session.service';
import type { TerminalMode } from './terminal-api';

/**
 * `qt-terminal-pane` — the right-side wrapper for Terminal Mode (v4
 * `TerminalPane`, shape mirrors DocumentPane): a header with focus-toggle /
 * hide-pane / kill, and the xterm body sized to fill the pane. The kill button
 * confirms on a second click (auto-cancelling after 4 s).
 */
@Component({
  selector: 'qt-terminal-pane',
  changeDetection: ChangeDetectionStrategy.OnPush,
  host: { class: 'block h-full' },
  imports: [Icon, Terminal],
  template: `
    <div class="qt-doc-pane-inner flex flex-col h-full overflow-hidden qt-bg-surface">
      <div
        class="qt-doc-header flex items-center justify-between gap-2 px-3 py-2 border-b qt-border-default"
      >
        <div class="qt-doc-title min-w-0 truncate text-sm font-medium">{{ title() }}</div>
        <div class="flex items-center gap-1">
          <button
            type="button"
            class="qt-icon-button p-1"
            [title]="mode() === 'focus' ? 'Restore split layout' : 'Maximize terminal'"
            [attr.aria-label]="mode() === 'focus' ? 'Restore split layout' : 'Maximize terminal'"
            (click)="toggleFocusMode.emit()"
          >
            <qt-icon [name]="mode() === 'focus' ? 'compress' : 'expand'" class="w-4 h-4" />
          </button>

          <button
            type="button"
            class="qt-icon-button p-1"
            title="Close pane (terminal stays alive)"
            aria-label="Close terminal pane"
            (click)="hidePane.emit()"
          >
            <qt-icon name="minus" class="w-4 h-4" />
          </button>

          <button
            type="button"
            class="qt-icon-button p-1 qt-text-destructive"
            [class.opacity-70]="!confirmKill()"
            [title]="
              confirmKill()
                ? 'Click again to confirm: kill terminal and close pane'
                : 'Kill terminal and close pane'
            "
            aria-label="Kill terminal and close pane"
            (click)="onKillClick()"
          >
            <qt-icon name="close" class="w-4 h-4" />
          </button>
        </div>
      </div>

      <div class="flex-1 min-h-0 qt-terminal-surface">
        <qt-terminal [sessionId]="sessionId()" />
      </div>
    </div>
  `,
})
export class TerminalPane implements OnInit {
  readonly sessionId = input.required<string>();
  readonly mode = input.required<TerminalMode>();

  readonly toggleFocusMode = output<void>();
  /** Hide the pane but leave the PTY alive (the inline embed becomes interactive again). */
  readonly hidePane = output<void>();
  /** Kill the PTY and close the pane. */
  readonly kill = output<void>();

  private readonly sessions = inject(TerminalSessionService);
  private readonly destroyRef = inject(DestroyRef);
  private readonly injector = inject(Injector);

  /** A second handle (ref-counted, shares the WS) just to read meta for the title. */
  private handle!: TerminalSessionHandle;
  protected readonly title = signal('Terminal');
  protected readonly confirmKill = signal(false);
  private confirmTimer: ReturnType<typeof setTimeout> | null = null;

  ngOnInit(): void {
    this.handle = this.sessions.acquire(this.sessionId());
    effect(
      () => {
        const meta = this.handle.meta();
        this.title.set(
          meta ? `Terminal — ${meta.shell}${meta.label ? ` (${meta.label})` : ''}` : 'Terminal',
        );
      },
      { injector: this.injector },
    );
    this.destroyRef.onDestroy(() => {
      if (this.confirmTimer) clearTimeout(this.confirmTimer);
      this.handle?.release();
    });
  }

  protected onKillClick(): void {
    if (!this.confirmKill()) {
      this.confirmKill.set(true);
      // Auto-cancel the confirm after a few seconds so it doesn't linger (v4).
      this.confirmTimer = setTimeout(() => this.confirmKill.set(false), 4000);
      return;
    }
    if (this.confirmTimer) clearTimeout(this.confirmTimer);
    this.kill.emit();
  }
}
