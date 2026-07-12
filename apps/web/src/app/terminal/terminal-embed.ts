import {
  ChangeDetectionStrategy,
  Component,
  computed,
  DestroyRef,
  effect,
  inject,
  Injector,
  input,
  OnInit,
  signal,
} from '@angular/core';
import { Router } from '@angular/router';

import { Icon } from '../ui/icon';
import { TerminalApi } from './terminal-api';
import { TerminalModeController } from './terminal-mode';
import type { PtySessionMeta } from './terminal-protocol';
import { Terminal } from './terminal';
import {
  TerminalSessionService,
  type TerminalSessionHandle,
  type TerminalState,
} from './terminal-session.service';

/**
 * `qt-terminal-embed` — the inline terminal wrapper for a Salon chat bubble (v4
 * `TerminalEmbed`): a collapsible header (pop-out, kill) over the xterm surface.
 * Collapse state persists in localStorage per session. When the same session is
 * showing in the Terminal Mode pane, the body becomes a "showing in the pane"
 * note instead of a second live surface. On PTY exit it dispatches
 * `quilltap:terminal-exited` so the Salon page refetches (the new Ariel close
 * announcement) and the pane cleans up.
 */
@Component({
  selector: 'qt-terminal-embed',
  changeDetection: ChangeDetectionStrategy.OnPush,
  imports: [Icon, Terminal],
  template: `
    <div class="qt-terminal-embed">
      <div class="qt-terminal-embed-header">
        <div class="flex items-center gap-2 flex-1">
          <button
            type="button"
            class="qt-icon-button p-1"
            [attr.aria-label]="collapsed() ? 'Expand' : 'Collapse'"
            (click)="toggleCollapse()"
          >
            <qt-icon
              name="arrow-down"
              class="w-4 h-4 transition-transform"
              [class.-rotate-90]="collapsed()"
            />
          </button>
          <h3 class="text-sm font-medium">{{ title() }}</h3>
        </div>

        <div class="flex items-center gap-2">
          <button type="button" class="qt-button-icon text-sm" title="Pop out" (click)="popOut()">
            <qt-icon name="external-link" class="w-4 h-4" />
          </button>

          @if (state() !== 'exited') {
            <button
              type="button"
              class="qt-button-icon text-sm qt-text-destructive opacity-70 hover:opacity-100"
              title="Kill session"
              (click)="kill()"
            >
              <qt-icon name="close" class="w-4 h-4" />
            </button>
          }
        </div>
      </div>

      @if (!collapsed()) {
        @if (isInTerminalPane()) {
          <div class="qt-terminal-embed-footer">
            <span class="qt-text-secondary">Showing in Terminal Mode pane.</span>
            <button
              type="button"
              class="qt-button-secondary text-xs px-2 py-1"
              title="Focus the Terminal Mode pane"
              (click)="focusPane()"
            >
              Go to pane →
            </button>
          </div>
        } @else {
          <div class="qt-terminal-surface" style="height: 360px">
            <qt-terminal [sessionId]="sessionId()" class="h-full w-full" />
          </div>
        }
      }
    </div>
  `,
})
export class TerminalEmbed implements OnInit {
  readonly sessionId = input.required<string>();
  readonly chatId = input.required<string>();
  readonly label = input<string | null>(null);

  private readonly router = inject(Router);
  private readonly sessions = inject(TerminalSessionService);
  private readonly api = inject(TerminalApi);
  private readonly destroyRef = inject(DestroyRef);
  private readonly injector = inject(Injector);
  /** The conversation's controller (present when embedded in the Salon subtree). */
  private readonly terminalCtx = inject(TerminalModeController, { optional: true });

  private handle!: TerminalSessionHandle;
  protected readonly state = signal<TerminalState>('connecting');
  private readonly metaValue = signal<PtySessionMeta | null>(null);
  protected readonly collapsed = signal(false);
  private exitDispatched = false;

  protected readonly title = computed(() => {
    const label = this.label();
    if (label) return label;
    const meta = this.metaValue();
    return meta ? `Terminal — ${meta.shell}` : 'Terminal';
  });

  protected readonly isInTerminalPane = computed(() => {
    const ctx = this.terminalCtx;
    if (!ctx) return false;
    return ctx.terminalMode() !== 'normal' && ctx.activeTerminalSessionId() === this.sessionId();
  });

  ngOnInit(): void {
    if (typeof window !== 'undefined') {
      this.collapsed.set(
        localStorage.getItem(`terminalEmbed:${this.sessionId()}:collapsed`) === 'true',
      );
    }
    this.handle = this.sessions.acquire(this.sessionId());

    // Mirror the session state + meta, and dispatch terminal-exited once on exit.
    effect(
      () => {
        const s = this.handle.state();
        this.state.set(s);
        this.metaValue.set(this.handle.meta());
        if (s === 'exited' && !this.exitDispatched) {
          this.exitDispatched = true;
          if (typeof window !== 'undefined') {
            window.dispatchEvent(
              new CustomEvent('quilltap:terminal-exited', {
                detail: { chatId: this.chatId(), sessionId: this.sessionId() },
              }),
            );
          }
        }
      },
      { injector: this.injector },
    );

    this.destroyRef.onDestroy(() => this.handle?.release());
  }

  protected toggleCollapse(): void {
    const next = !this.collapsed();
    this.collapsed.set(next);
    if (typeof window !== 'undefined') {
      localStorage.setItem(`terminalEmbed:${this.sessionId()}:collapsed`, String(next));
    }
  }

  protected popOut(): void {
    void this.router.navigate(['/salon', this.chatId(), 'terminal', this.sessionId()]);
  }

  protected async kill(): Promise<void> {
    await this.api.killTerminalSession(this.sessionId()).catch(() => {
      // v4 toasts on failure; the SPA has no toast bus yet — swallow.
    });
  }

  protected focusPane(): void {
    if (typeof document === 'undefined') return;
    const pane = document.querySelector('.qt-doc-pane');
    if (pane && 'scrollIntoView' in pane) {
      try {
        (pane as HTMLElement).scrollIntoView({ behavior: 'smooth', block: 'nearest' });
      } catch {
        // ignore
      }
    }
    const textarea = pane?.querySelector('.xterm-helper-textarea') as HTMLTextAreaElement | null;
    textarea?.focus();
  }
}
