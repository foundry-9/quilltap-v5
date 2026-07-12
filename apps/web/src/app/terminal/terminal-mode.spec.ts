import { TestBed } from '@angular/core/testing';
import { describe, expect, it, vi } from 'vitest';

import { isLiveSession, TerminalApi, type TerminalPaneState } from './terminal-api';
import { clampSplit, nextFocusMode, TerminalModeController } from './terminal-mode';
import type { PtySessionMeta } from './terminal-protocol';

function meta(over: Partial<PtySessionMeta> = {}): PtySessionMeta {
  return {
    id: 's1',
    chatId: 'c1',
    label: null,
    shell: '/bin/bash',
    cwd: '/tmp',
    startedAt: '2026-07-12T00:00:00.000Z',
    exitedAt: null,
    exitCode: null,
    transcriptPath: null,
    ...over,
  };
}

describe('terminal-mode pure helpers (v4)', () => {
  it('clampSplit clamps to [20, 80] rounded', () => {
    expect(clampSplit(50)).toBe(50);
    expect(clampSplit(5)).toBe(20);
    expect(clampSplit(95)).toBe(80);
    expect(clampSplit(49.6)).toBe(50);
  });

  it('nextFocusMode toggles focus ⇄ split', () => {
    expect(nextFocusMode('focus')).toBe('split');
    expect(nextFocusMode('split')).toBe('focus');
    // From normal (defensive), the target is focus (v4: anything !== focus → focus).
    expect(nextFocusMode('normal')).toBe('focus');
  });

  it('isLiveSession is false for null / exited, true otherwise', () => {
    expect(isLiveSession(null)).toBe(false);
    expect(isLiveSession(meta({ exitedAt: '2026-07-12T00:01:00.000Z' }))).toBe(false);
    expect(isLiveSession(meta())).toBe(true);
  });
});

/** A recording stub of the REST client. */
class FakeApi {
  sessions = new Map<string, PtySessionMeta>();
  listResult: PtySessionMeta[] = [];
  spawned: PtySessionMeta = meta({ id: 'spawned-1' });
  persists: Partial<TerminalPaneState>[] = [];
  killed: string[] = [];

  getTerminalSession = vi.fn(async (id: string) => this.sessions.get(id) ?? null);
  listTerminalSessions = vi.fn(async (_chatId: string) => this.listResult);
  spawnTerminalSession = vi.fn(async (_chatId: string) => this.spawned);
  killTerminalSession = vi.fn(async (id: string) => {
    this.killed.push(id);
  });
  persistChatTerminalState = vi.fn(async (_chatId: string, updates: Partial<TerminalPaneState>) => {
    this.persists.push(updates);
  });
}

function build(): {
  ctrl: TerminalModeController;
  api: FakeApi;
  refetch: ReturnType<typeof vi.fn>;
} {
  const api = new FakeApi();
  TestBed.configureTestingModule({
    providers: [TerminalModeController, { provide: TerminalApi, useValue: api }],
  });
  const ctrl = TestBed.inject(TerminalModeController);
  const refetch = vi.fn();
  ctrl.configure('c1', refetch);
  return { ctrl, api, refetch };
}

/** Flush enough microtasks to drain hydrate's `.catch().then()` liveness check. */
async function flush(): Promise<void> {
  for (let i = 0; i < 6; i++) await Promise.resolve();
}

describe('TerminalModeController (v4 useTerminalMode)', () => {
  it('requestOpen spawns and enters split when there are no live sessions', async () => {
    const { ctrl, api, refetch } = build();
    api.listResult = [];
    await ctrl.requestOpen();

    expect(api.spawnTerminalSession).toHaveBeenCalledWith('c1');
    expect(ctrl.terminalMode()).toBe('split');
    expect(ctrl.activeTerminalSessionId()).toBe('spawned-1');
    expect(refetch).toHaveBeenCalledTimes(1);
    expect(api.persists.at(-1)).toEqual({
      terminalMode: 'split',
      activeTerminalSessionId: 'spawned-1',
    });
  });

  it('requestOpen re-enters a still-live bound session without spawning', async () => {
    const { ctrl, api } = build();
    api.sessions.set('s1', meta({ id: 's1' }));
    ctrl.hydrate({ terminalMode: 'split', activeTerminalSessionId: 's1' });
    await flush();
    await ctrl.requestOpen();

    expect(api.spawnTerminalSession).not.toHaveBeenCalled();
    expect(ctrl.terminalMode()).toBe('split');
    expect(ctrl.activeTerminalSessionId()).toBe('s1');
  });

  it('requestOpen shows the picker when other live sessions exist', async () => {
    const { ctrl, api } = build();
    api.listResult = [meta({ id: 'a' }), meta({ id: 'b', exitedAt: '2026-07-12T00:01:00.000Z' })];
    await ctrl.requestOpen();

    expect(ctrl.showTerminalPicker()).toBe(true);
    // Only the live one is offered.
    expect(ctrl.pickerSessions().map((s) => s.id)).toEqual(['a']);
    expect(api.spawnTerminalSession).not.toHaveBeenCalled();
  });

  it('hydrate falls back to normal when the bound session has died', async () => {
    const { ctrl, api } = build();
    // No session registered → getTerminalSession returns null → dead.
    ctrl.hydrate({
      terminalMode: 'split',
      activeTerminalSessionId: 'gone',
      rightPaneVerticalSplit: 60,
    });
    expect(ctrl.rightPaneVerticalSplit()).toBe(60);
    await flush();

    expect(ctrl.terminalMode()).toBe('normal');
    expect(ctrl.activeTerminalSessionId()).toBeNull();
    expect(api.persists.at(-1)).toEqual({ terminalMode: 'normal', activeTerminalSessionId: null });
  });

  it('toggleFocusMode flips split ⇄ focus and persists', () => {
    const { ctrl, api } = build();
    ctrl.hydrate({ terminalMode: 'split', activeTerminalSessionId: null });
    ctrl.toggleFocusMode();
    expect(ctrl.terminalMode()).toBe('focus');
    ctrl.toggleFocusMode();
    expect(ctrl.terminalMode()).toBe('split');
    expect(api.persists).toContainEqual({ terminalMode: 'focus' });
  });

  it('killTerminal closes the pane and kills the PTY', async () => {
    const { ctrl, api } = build();
    api.sessions.set('s1', meta({ id: 's1' }));
    ctrl.hydrate({ terminalMode: 'split', activeTerminalSessionId: 's1' });
    await flush();
    await ctrl.killTerminal();

    expect(ctrl.terminalMode()).toBe('normal');
    expect(ctrl.activeTerminalSessionId()).toBeNull();
    expect(api.killed).toEqual(['s1']);
  });

  it('setRightPaneVerticalSplit clamps and persists', () => {
    const { ctrl, api } = build();
    ctrl.setRightPaneVerticalSplit(95);
    expect(ctrl.rightPaneVerticalSplit()).toBe(80);
    expect(api.persists.at(-1)).toEqual({ rightPaneVerticalSplit: 80 });
  });
});
