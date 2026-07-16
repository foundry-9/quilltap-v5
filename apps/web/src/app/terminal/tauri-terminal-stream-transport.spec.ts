/**
 * The Tauri terminal pipe (P4.7b §4): command names, argument keys, and the
 * frozen `WsServerMessage`/`WsClientMessage` payloads are written against the
 * Shared contract text — a lane-A rename surfaces here as a spec diff. The
 * round trips run the real `@tauri-apps/api` `invoke` + `Channel` over
 * `mockIPC`; frames are driven through the captured channel's public
 * `onmessage` (the mock hands the raw `Channel` instance to the handler).
 */

import { afterEach, describe, expect, it, vi } from 'vitest';
import { Channel } from '@tauri-apps/api/core';
import { clearMocks, mockIPC } from '@tauri-apps/api/mocks';

import type { WsServerMessage } from './terminal-protocol';
import { TauriTerminalStreamTransport } from './tauri-terminal-stream-transport';
import type { TerminalStreamCallbacks } from './terminal-stream-transport';

function recordingCallbacks() {
  const events: unknown[] = [];
  const callbacks: TerminalStreamCallbacks = {
    onOpen: () => events.push({ kind: 'open' }),
    onMessage: (message) => events.push({ kind: 'message', message }),
    onError: () => events.push({ kind: 'error' }),
    onClose: (info) => events.push({ kind: 'close', ...info }),
  };
  return { events, callbacks };
}

/** mockIPC shell recording every command; hands back the captured channel. */
function mockTerminalShell() {
  const calls: Array<{ cmd: string; args: Record<string, unknown> }> = [];
  let channel: Channel<WsServerMessage> | null = null;
  mockIPC((cmd, args) => {
    const record = { cmd, args: args as Record<string, unknown> };
    calls.push(record);
    if (cmd === 'terminal_attach') {
      channel = record.args['onMessage'] as Channel<WsServerMessage>;
      return null;
    }
    if (cmd === 'terminal_send' || cmd === 'terminal_detach') {
      return null;
    }
    throw new Error(`unexpected command ${cmd}`);
  });
  return { calls, channel: () => channel };
}

afterEach(() => {
  clearMocks();
  vi.restoreAllMocks();
});

describe('TauriTerminalStreamTransport', () => {
  it("attaches with {sessionId, onMessage: Channel} and opens only once attach resolves", async () => {
    const shell = mockTerminalShell();
    const rec = recordingCallbacks();
    const conn = new TauriTerminalStreamTransport().open('s-1', rec.callbacks)!;

    expect(conn.isOpen()).toBe(false);
    await vi.waitFor(() => expect(rec.events).toEqual([{ kind: 'open' }]));
    expect(conn.isOpen()).toBe(true);
    expect(shell.calls.map((c) => c.cmd)).toEqual(['terminal_attach']);
    expect(shell.calls[0].args['sessionId']).toBe('s-1');
    expect(shell.channel()).toBeInstanceOf(Channel);
    conn.close();
  });

  it('delivers the frozen WsServerMessage union verbatim through the channel', async () => {
    const shell = mockTerminalShell();
    const rec = recordingCallbacks();
    const conn = new TauriTerminalStreamTransport().open('s-1', rec.callbacks)!;
    await vi.waitFor(() => expect(rec.events).toHaveLength(1));

    shell.channel()!.onmessage({ type: 'output', data: '$ ' });
    shell.channel()!.onmessage({ type: 'pong' });
    shell.channel()!.onmessage({ type: 'exit', code: 0, signal: null });

    expect(rec.events.slice(1)).toEqual([
      { kind: 'message', message: { type: 'output', data: '$ ' } },
      { kind: 'message', message: { type: 'pong' } },
      { kind: 'message', message: { type: 'exit', code: 0, signal: null } },
    ]);
    conn.close();
  });

  it("send rides terminal_send with {sessionId, message} — and only while attached", async () => {
    const shell = mockTerminalShell();
    const rec = recordingCallbacks();
    const conn = new TauriTerminalStreamTransport().open('s-1', rec.callbacks)!;

    conn.send({ type: 'input', data: 'early' }); // pre-attach — dropped
    await vi.waitFor(() => expect(rec.events).toEqual([{ kind: 'open' }]));
    conn.send({ type: 'input', data: 'ls\r' });
    conn.send({ type: 'resize', cols: 80, rows: 24 });

    await vi.waitFor(() => {
      const sends = shell.calls.filter((c) => c.cmd === 'terminal_send');
      expect(sends.map((c) => c.args)).toEqual([
        { sessionId: 's-1', message: { type: 'input', data: 'ls\r' } },
        { sessionId: 's-1', message: { type: 'resize', cols: 80, rows: 24 } },
      ]);
    });
    conn.close();
  });

  it('close detaches and drops any frames still in flight', async () => {
    const shell = mockTerminalShell();
    const rec = recordingCallbacks();
    const conn = new TauriTerminalStreamTransport().open('s-1', rec.callbacks)!;
    await vi.waitFor(() => expect(rec.events).toEqual([{ kind: 'open' }]));

    conn.close();
    expect(conn.isOpen()).toBe(false);
    shell.channel()!.onmessage({ type: 'output', data: 'late' });

    await vi.waitFor(() =>
      expect(shell.calls.map((c) => c.cmd)).toEqual(['terminal_attach', 'terminal_detach']),
    );
    expect(shell.calls[1].args).toEqual({ sessionId: 's-1' });
    expect(rec.events).toEqual([{ kind: 'open' }]); // the late frame was dropped
    conn.close(); // idempotent — no second detach
    await Promise.resolve();
    expect(shell.calls.filter((c) => c.cmd === 'terminal_detach')).toHaveLength(1);
  });

  it('a failed attach routes into the WS failure shape: error, then a TRANSIENT close', async () => {
    mockIPC(() => {
      throw new Error('no such session');
    });
    const rec = recordingCallbacks();
    const conn = new TauriTerminalStreamTransport().open('s-1', rec.callbacks)!;
    await vi.waitFor(() =>
      expect(rec.events).toEqual([{ kind: 'error' }, { kind: 'close', transient: true }]),
    );
    expect(conn.isOpen()).toBe(false);
  });

  it('close racing the attach detaches once the attach lands, and never opens', async () => {
    let resolveAttach: (() => void) | null = null;
    const calls: string[] = [];
    mockIPC((cmd) => {
      calls.push(cmd);
      if (cmd === 'terminal_attach') {
        return new Promise<null>((resolve) => {
          resolveAttach = () => resolve(null);
        });
      }
      return null;
    });
    const rec = recordingCallbacks();
    const conn = new TauriTerminalStreamTransport().open('s-1', rec.callbacks)!;
    await vi.waitFor(() => expect(calls).toEqual(['terminal_attach']));

    conn.close();
    resolveAttach!();
    await vi.waitFor(() => expect(calls).toEqual(['terminal_attach', 'terminal_detach']));
    expect(rec.events).toEqual([]); // no onOpen after a lost race
    expect(conn.isOpen()).toBe(false);
  });
});
