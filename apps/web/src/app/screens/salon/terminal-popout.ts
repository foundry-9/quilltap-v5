import {
  ChangeDetectionStrategy,
  Component,
  computed,
  DestroyRef,
  effect,
  inject,
  Injector,
  OnInit,
  signal,
} from '@angular/core';
import { toSignal } from '@angular/core/rxjs-interop';
import { ActivatedRoute, Router, RouterLink } from '@angular/router';

import { Icon } from '../../ui/icon';
import { TerminalApi } from '../../terminal/terminal-api';
import { Terminal } from '../../terminal/terminal';
import {
  TerminalSessionService,
  type TerminalSessionHandle,
  type TerminalState,
} from '../../terminal/terminal-session.service';
import type { PtySessionMeta } from '../../terminal/terminal-protocol';

/**
 * The full-page terminal pop-out (v4 `app/salon/[id]/terminal/[sessionId]/page`)
 * — one terminal filling the page, the pop-out target of the inline embed. A
 * header with Back / a breadcrumb to the chat / a Kill button.
 */
@Component({
  selector: 'qt-terminal-popout',
  changeDetection: ChangeDetectionStrategy.OnPush,
  host: { class: 'block h-full' },
  imports: [Icon, RouterLink, Terminal],
  template: `
    <div class="qt-terminal-popout-page h-full flex flex-col">
      <div class="qt-terminal-popout-header">
        <div class="flex items-center gap-3">
          <button type="button" class="qt-icon-button" title="Back to chat" (click)="back()">
            <qt-icon name="chevron-left" class="w-5 h-5" />
          </button>
          <a [routerLink]="['/salon', chatId()]" class="qt-terminal-popout-link text-sm">Chat</a>
          <span class="qt-terminal-popout-separator">/</span>
          <h1 class="qt-terminal-popout-title">{{ title() }}</h1>
        </div>

        @if (state() !== 'exited') {
          <button type="button" class="qt-button-destructive text-sm" (click)="kill()">
            Kill Session
          </button>
        }
      </div>

      <div class="flex-1 overflow-hidden">
        <qt-terminal [sessionId]="sessionId()" class="h-full w-full" />
      </div>
    </div>
  `,
})
export class TerminalPopout implements OnInit {
  private readonly route = inject(ActivatedRoute);
  private readonly router = inject(Router);
  private readonly sessions = inject(TerminalSessionService);
  private readonly api = inject(TerminalApi);
  private readonly destroyRef = inject(DestroyRef);
  private readonly injector = inject(Injector);

  private readonly params = toSignal(this.route.paramMap, { requireSync: true });
  protected readonly chatId = computed(() => this.params().get('id') ?? '');
  protected readonly sessionId = computed(() => this.params().get('sessionId') ?? '');

  private handle!: TerminalSessionHandle;
  protected readonly state = signal<TerminalState>('connecting');
  private readonly metaValue = signal<PtySessionMeta | null>(null);

  protected readonly title = computed(() => {
    const meta = this.metaValue();
    return meta ? (meta.label ?? `Terminal — ${meta.shell}`) : 'Terminal';
  });

  ngOnInit(): void {
    this.handle = this.sessions.acquire(this.sessionId());
    effect(
      () => {
        this.state.set(this.handle.state());
        this.metaValue.set(this.handle.meta());
      },
      { injector: this.injector },
    );
    this.destroyRef.onDestroy(() => this.handle?.release());
  }

  protected back(): void {
    // Prefer history back; fall back to the chat route.
    if (typeof window !== 'undefined' && window.history.length > 1) {
      window.history.back();
      return;
    }
    void this.router.navigate(['/salon', this.chatId()]);
  }

  protected async kill(): Promise<void> {
    await this.api.killTerminalSession(this.sessionId()).catch(() => {
      // v4 toasts on failure; the SPA swallows.
    });
  }
}
