import {
  AfterViewInit,
  ChangeDetectionStrategy,
  Component,
  DestroyRef,
  ElementRef,
  Injector,
  OnInit,
  effect,
  inject,
  input,
  signal,
  viewChild,
} from '@angular/core';

import { TerminalSessionService, type TerminalSessionHandle } from './terminal-session.service';

/**
 * `qt-terminal` — the xterm.js surface (v4 `components/terminal/Terminal.tsx`).
 * Lazily imports `@xterm/xterm` + `@xterm/addon-fit` (keeps the ~250 KB terminal
 * bundle out of the initial chunk), applies the theme from the `--qt-terminal-*`
 * CSS variables, wires xterm input → the session service and session output →
 * xterm, and refits on container resize via a `ResizeObserver`.
 *
 * D18 locked xterm.js; this lane adds only `@xterm/xterm` + `@xterm/addon-fit`
 * (v4's optional web-links / serialize / canvas addons are progressive
 * enhancements omitted here — a tracked, no-behaviour-change scoping).
 */
@Component({
  selector: 'qt-terminal',
  changeDetection: ChangeDetectionStrategy.OnPush,
  host: { class: 'block h-full w-full' },
  template: `
    <div class="relative qt-terminal-surface h-full w-full">
      <div #container class="w-full h-full" data-testid="terminal-container"></div>
    </div>
  `,
})
export class Terminal implements OnInit, AfterViewInit {
  readonly sessionId = input.required<string>();
  readonly rows = input(24);
  readonly cols = input(80);
  readonly fontSize = input(13);

  private readonly container = viewChild.required<ElementRef<HTMLDivElement>>('container');
  private readonly sessions = inject(TerminalSessionService);
  private readonly destroyRef = inject(DestroyRef);
  private readonly injector = inject(Injector);

  /** The bound session (WS opens on acquire — mirrors v4's connect-on-mount). */
  private handle!: TerminalSessionHandle;

  /** The xterm instance + fit addon (untyped — the modules are dynamically imported). */
  private term: {
    write(d: string): void;
    focus(): void;
    dispose(): void;
    cols: number;
    rows: number;
    onData(cb: (d: string) => void): { dispose(): void };
  } | null = null;
  private fitAddon: { fit(): void } | null = null;
  private observer: ResizeObserver | null = null;
  private outputUnsub: (() => void) | null = null;
  private inputDisposable: { dispose(): void } | null = null;
  private exitLineWritten = false;

  /** True once xterm has opened — gates the state-driven effects (v4 `initialized`). */
  private readonly initialized = signal(false);

  constructor() {
    this.destroyRef.onDestroy(() => this.teardown());
  }

  ngOnInit(): void {
    // WS opens on acquire (v4's connect-on-mount). Read the required input here,
    // not in the constructor, where signal inputs are not yet bound.
    this.handle = this.sessions.acquire(this.sessionId());

    // Refocus once live so the user can type without an extra click, and write
    // the closing line once on exit (v4's two state effects). Created with the
    // injector because it's outside the constructor's injection context.
    effect(
      () => {
        const state = this.handle.state();
        if (!this.initialized() || !this.term) return;
        if (state === 'live') {
          try {
            this.term.focus();
          } catch {
            /* noop */
          }
        } else if (state === 'exited' && !this.exitLineWritten) {
          this.exitLineWritten = true;
          const info = this.handle.exitInfo();
          const line = info?.signal
            ? `\r\n[session ended — signal ${info.signal}]\r\n`
            : `\r\n[session ended — exit code ${info?.code ?? 'unknown'}]\r\n`;
          this.term.write(line);
        }
      },
      { injector: this.injector },
    );
  }

  async ngAfterViewInit(): Promise<void> {
    const el = this.container().nativeElement;
    let cancelled = false;
    this.destroyRef.onDestroy(() => {
      cancelled = true;
    });

    const { Terminal: XTermTerminal } = await import('@xterm/xterm');
    const { FitAddon } = await import('@xterm/addon-fit');
    if (cancelled) return;

    const term = new XTermTerminal({
      rows: this.rows(),
      cols: this.cols(),
      fontSize: this.fontSize(),
      theme: readTerminalTheme(),
      scrollback: 1000,
      rightClickSelectsWord: true,
    });
    const fitAddon = new FitAddon();
    term.loadAddon(fitAddon);

    term.open(el);
    try {
      fitAddon.fit();
    } catch {
      /* noop */
    }
    try {
      term.focus();
    } catch {
      /* noop */
    }

    this.term = term as unknown as typeof this.term;
    this.fitAddon = fitAddon;

    // xterm output ← session; session input ← xterm.
    this.outputUnsub = this.handle.onData((chunk) => this.term?.write(chunk));
    this.inputDisposable = term.onData((data: string) => this.handle.send(data));

    // Refit + report the new size on any container resize (v4's ResizeObserver).
    if (typeof ResizeObserver !== 'undefined') {
      this.observer = new ResizeObserver(() => {
        if (!this.fitAddon || !this.term) return;
        try {
          this.fitAddon.fit();
          this.handle.resize(this.term.cols, this.term.rows);
        } catch {
          /* ignore layout-thrash resize errors */
        }
      });
      this.observer.observe(el);
    }

    this.initialized.set(true);
  }

  private teardown(): void {
    this.observer?.disconnect();
    this.observer = null;
    this.outputUnsub?.();
    this.outputUnsub = null;
    try {
      this.inputDisposable?.dispose();
    } catch {
      /* noop */
    }
    this.inputDisposable = null;
    try {
      this.term?.dispose();
    } catch {
      /* noop */
    }
    this.term = null;
    this.handle?.release();
  }
}

/**
 * Read the xterm theme from the `--qt-terminal-*` CSS variables (v4
 * `getTerminalTheme`). Returns `{}` where `document` is absent (unit env).
 */
function readTerminalTheme(): Record<string, string> {
  if (typeof document === 'undefined') return {};
  const style = getComputedStyle(document.documentElement);
  const get = (variable: string): string => style.getPropertyValue(variable).trim() || '#000000';
  return {
    background: get('--qt-terminal-bg'),
    foreground: get('--qt-terminal-fg'),
    cursor: get('--qt-terminal-cursor'),
    selectionBackground: get('--qt-terminal-selection'),
    black: get('--qt-terminal-ansi-black'),
    red: get('--qt-terminal-ansi-red'),
    green: get('--qt-terminal-ansi-green'),
    yellow: get('--qt-terminal-ansi-yellow'),
    blue: get('--qt-terminal-ansi-blue'),
    magenta: get('--qt-terminal-ansi-magenta'),
    cyan: get('--qt-terminal-ansi-cyan'),
    white: get('--qt-terminal-ansi-white'),
    brightBlack: get('--qt-terminal-ansi-bright-black'),
    brightRed: get('--qt-terminal-ansi-bright-red'),
    brightGreen: get('--qt-terminal-ansi-bright-green'),
    brightYellow: get('--qt-terminal-ansi-bright-yellow'),
    brightBlue: get('--qt-terminal-ansi-bright-blue'),
    brightMagenta: get('--qt-terminal-ansi-bright-magenta'),
    brightCyan: get('--qt-terminal-ansi-bright-cyan'),
    brightWhite: get('--qt-terminal-ansi-bright-white'),
  };
}
