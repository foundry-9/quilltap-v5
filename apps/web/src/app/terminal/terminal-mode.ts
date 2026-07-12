import { DestroyRef, inject, Injectable, signal } from '@angular/core';

import {
  isLiveSession,
  TerminalApi,
  type TerminalMode,
  type TerminalPaneState,
} from './terminal-api';
import type { PtySessionMeta } from './terminal-protocol';

/** The chat fields the terminal pane hydrates from (a structural subset of ChatDetail). */
export interface ChatTerminalFields {
  terminalMode?: TerminalMode | null;
  activeTerminalSessionId?: string | null;
  rightPaneVerticalSplit?: number | null;
}

/** Clamp a split/divider percentage to the [20, 80] band, rounded (v4). */
export function clampSplit(position: number): number {
  return Math.max(20, Math.min(80, Math.round(position)));
}

/** The focus toggle target (v4 `toggleFocusMode`): focus ⇄ split. */
export function nextFocusMode(mode: TerminalMode): TerminalMode {
  return mode === 'focus' ? 'split' : 'focus';
}

/**
 * Terminal Mode state for one Salon conversation (v4 `useTerminalMode` +
 * `terminalModeApi`). Provided at the conversation component so the inline embed
 * can inject the SAME instance to learn whether its session is showing in the
 * pane (v4's `TerminalModeContext`). The layout state (`terminalMode`), the bound
 * session id, and the two divider positions all live on the chat record; the
 * picker visibility is local UX.
 *
 * Not `providedIn: 'root'` — one instance per conversation screen.
 */
@Injectable()
export class TerminalModeController {
  private readonly api = inject(TerminalApi);
  private readonly destroyRef = inject(DestroyRef);

  private chatId: string | null = null;
  private refetchChat: (() => void | Promise<void>) | null = null;
  /** Guards the async liveness check in {@link hydrate} against a chat change mid-flight. */
  private hydrateToken = 0;

  readonly terminalMode = signal<TerminalMode>('normal');
  readonly activeTerminalSessionId = signal<string | null>(null);
  readonly rightPaneVerticalSplit = signal<number>(50);

  readonly showTerminalPicker = signal(false);
  readonly pickerSessions = signal<PtySessionMeta[]>([]);

  constructor() {
    // A Hide-pane → Kill happening from a message embed emits this DOM event;
    // clean the pane up too if it was showing that session (v4's listener).
    const onExited = (event: Event) => {
      const detail = (event as CustomEvent<{ chatId?: string; sessionId?: string }>).detail;
      if (!detail || detail.chatId !== this.chatId || !detail.sessionId) return;
      if (detail.sessionId !== this.activeTerminalSessionId()) return;
      this.terminalMode.set('normal');
      this.activeTerminalSessionId.set(null);
      void this.persist({ terminalMode: 'normal', activeTerminalSessionId: null });
    };
    if (typeof window !== 'undefined') {
      window.addEventListener('quilltap:terminal-exited', onExited);
      this.destroyRef.onDestroy(() =>
        window.removeEventListener('quilltap:terminal-exited', onExited),
      );
    }
  }

  /** Bind the controller to a conversation (chat id + a refetch after spawn). */
  configure(chatId: string, refetchChat: () => void | Promise<void>): void {
    this.chatId = chatId;
    this.refetchChat = refetchChat;
  }

  /**
   * Hydrate from the chat record on load / refresh (v4's hydration effect). If a
   * session was bound but has since died, fall back to normal so the user
   * doesn't return into a dead pane.
   */
  hydrate(chat: ChatTerminalFields | null): void {
    if (!chat) return;
    const incomingMode = (chat.terminalMode ?? 'normal') as TerminalMode;
    const incomingSessionId = chat.activeTerminalSessionId ?? null;
    this.rightPaneVerticalSplit.set(chat.rightPaneVerticalSplit ?? 50);

    if (incomingMode !== 'normal' && incomingSessionId) {
      const token = ++this.hydrateToken;
      void this.api
        .getTerminalSession(incomingSessionId)
        .catch(() => null)
        .then((meta) => {
          if (token !== this.hydrateToken) return;
          if (!isLiveSession(meta)) {
            this.terminalMode.set('normal');
            this.activeTerminalSessionId.set(null);
            void this.persist({ terminalMode: 'normal', activeTerminalSessionId: null });
            return;
          }
          this.terminalMode.set(incomingMode);
          this.activeTerminalSessionId.set(incomingSessionId);
        });
      return;
    }

    // A plain (already-normal or unbound) hydrate cancels any in-flight check.
    this.hydrateToken++;
    this.terminalMode.set(incomingMode);
    this.activeTerminalSessionId.set(incomingSessionId);
  }

  closeTerminalPicker(): void {
    this.showTerminalPicker.set(false);
    this.pickerSessions.set([]);
  }

  /**
   * Smart entry (v4 `requestOpen`): re-attach a still-live bound session, else
   * show the picker if other live sessions exist, else spawn-and-enter.
   */
  async requestOpen(): Promise<void> {
    const bound = this.activeTerminalSessionId();
    if (bound) {
      const meta = await this.api.getTerminalSession(bound).catch(() => null);
      if (isLiveSession(meta)) {
        await this.enterModeWithSession(bound);
        return;
      }
    }

    let liveSessions: PtySessionMeta[] = [];
    try {
      const all = await this.api.listTerminalSessions(this.chatId!);
      liveSessions = all.filter(isLiveSession);
    } catch {
      // fall through to spawning a new session
    }

    if (liveSessions.length > 0) {
      this.pickerSessions.set(liveSessions);
      this.showTerminalPicker.set(true);
      return;
    }

    await this.spawnNewSession();
  }

  async attachExistingSession(sessionId: string): Promise<void> {
    this.closeTerminalPicker();
    const meta = await this.api.getTerminalSession(sessionId).catch(() => null);
    if (!isLiveSession(meta)) {
      // v4 surfaces a toast here; the SPA has no toast bus yet — no-op refusal.
      return;
    }
    await this.enterModeWithSession(sessionId);
  }

  async spawnNewSession(): Promise<void> {
    this.closeTerminalPicker();
    const session = await this.api.spawnTerminalSession(this.chatId!);
    await this.enterModeWithSession(session.id);
    // Refetch so the Ariel session-opened announcement appears.
    await this.refetchChat?.();
  }

  /** Hide the pane but leave the PTY alive (v4 `hidePane`). */
  async hidePane(): Promise<void> {
    this.terminalMode.set('normal');
    await this.persist({ terminalMode: 'normal' });
  }

  /** Kill the PTY and exit Terminal Mode (v4 `killTerminal`). */
  async killTerminal(): Promise<void> {
    const sessionId = this.activeTerminalSessionId();
    this.terminalMode.set('normal');
    this.activeTerminalSessionId.set(null);
    await this.persist({ terminalMode: 'normal', activeTerminalSessionId: null });
    if (sessionId) {
      await this.api.killTerminalSession(sessionId).catch(() => {
        // v4 toasts; the SPA swallows (the pane is already closed).
      });
    }
  }

  toggleFocusMode(): void {
    const next = nextFocusMode(this.terminalMode());
    this.terminalMode.set(next);
    void this.persist({ terminalMode: next });
  }

  setRightPaneVerticalSplit(position: number): void {
    const clamped = clampSplit(position);
    this.rightPaneVerticalSplit.set(clamped);
    void this.persist({ rightPaneVerticalSplit: clamped });
  }

  private async enterModeWithSession(sessionId: string): Promise<void> {
    this.activeTerminalSessionId.set(sessionId);
    this.terminalMode.set('split');
    await this.persist({ terminalMode: 'split', activeTerminalSessionId: sessionId });
  }

  private async persist(updates: Partial<TerminalPaneState>): Promise<void> {
    if (!this.chatId) return;
    try {
      await this.api.persistChatTerminalState(this.chatId, updates);
    } catch (error) {
      console.error('[TerminalMode] Failed to persist chat terminal state', {
        chatId: this.chatId,
        error: error instanceof Error ? error.message : String(error),
      });
    }
  }
}
