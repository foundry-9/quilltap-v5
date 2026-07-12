import { signal } from '@angular/core';
import { describe, expect, it, vi } from 'vitest';

import type { PtySessionMeta } from './terminal-protocol';
import {
  OutputFanout,
  TerminalSessionService,
  type ExitInfo,
  type SessionSink,
  type TerminalState,
} from './terminal-session.service';

/**
 * The session service's rendering surface can't run under `ng test` (jsdom has
 * no real terminal geometry — quirk #6), so the WS client's pure pieces are
 * proven here: the client-side replay buffer / fan-out (v4 `dispatchOutput` +
 * `onData`) and the server-frame → reactive-state mapping (v4 `ws.onmessage`).
 */
describe('OutputFanout (v4 dispatchOutput + onData)', () => {
  it('replays buffered output to a subscriber that attaches late', () => {
    const fanout = new OutputFanout();
    // Output (the server ring buffer + first prompt) arrives before xterm opens.
    fanout.dispatch('ring-buffer\r\n');
    fanout.dispatch('$ ');

    const seen: string[] = [];
    fanout.subscribe((chunk) => seen.push(chunk));
    // The whole buffer replays as a single chunk.
    expect(seen).toEqual(['ring-buffer\r\n$ ']);
  });

  it('streams subsequent chunks to a live subscriber', () => {
    const fanout = new OutputFanout();
    const seen: string[] = [];
    fanout.subscribe((chunk) => seen.push(chunk));
    fanout.dispatch('echo quilltap\r\n');
    fanout.dispatch('quilltap\r\n');
    expect(seen).toEqual(['echo quilltap\r\n', 'quilltap\r\n']);
  });

  it('stops delivering after unsubscribe', () => {
    const fanout = new OutputFanout();
    const seen: string[] = [];
    const off = fanout.subscribe((chunk) => seen.push(chunk));
    fanout.dispatch('a');
    off();
    fanout.dispatch('b');
    expect(seen).toEqual(['a']);
  });

  it('caps the replay buffer at the tail (512 KiB)', () => {
    const fanout = new OutputFanout();
    fanout.dispatch('X'.repeat(512 * 1024));
    fanout.dispatch('TAILMARK');
    const seen: string[] = [];
    fanout.subscribe((chunk) => seen.push(chunk));
    const replayed = seen.join('');
    expect(replayed.length).toBe(512 * 1024);
    expect(replayed.endsWith('TAILMARK')).toBe(true);
  });

  it('swallows a throwing replay subscriber', () => {
    const fanout = new OutputFanout();
    fanout.dispatch('data');
    const err = vi.spyOn(console, 'error').mockImplementation(() => undefined);
    expect(() =>
      fanout.subscribe(() => {
        throw new Error('boom');
      }),
    ).not.toThrow();
    err.mockRestore();
  });
});

describe('TerminalSessionService.handleServerMessage (v4 ws.onmessage)', () => {
  function makeSink(): SessionSink & {
    state: ReturnType<typeof signal<TerminalState>>;
    meta: ReturnType<typeof signal<PtySessionMeta | null>>;
    exitInfo: ReturnType<typeof signal<ExitInfo | null>>;
  } {
    return {
      fanout: new OutputFanout(),
      state: signal<TerminalState>('connecting'),
      meta: signal<PtySessionMeta | null>(null),
      exitInfo: signal<ExitInfo | null>(null),
    };
  }

  const service = new TerminalSessionService();

  it('routes output into the fan-out', () => {
    const sink = makeSink();
    const seen: string[] = [];
    sink.fanout.subscribe((c) => seen.push(c));
    service.handleServerMessage(sink, { type: 'output', data: 'hello' });
    expect(seen).toEqual(['hello']);
  });

  it('stores meta', () => {
    const sink = makeSink();
    const meta: PtySessionMeta = {
      id: 's1',
      chatId: 'c1',
      label: null,
      shell: '/bin/bash',
      cwd: '/tmp',
      startedAt: '2026-07-12T00:00:00.000Z',
      exitedAt: null,
      exitCode: null,
      transcriptPath: null,
    };
    service.handleServerMessage(sink, { type: 'meta', meta });
    expect(sink.meta()).toEqual(meta);
  });

  it('marks exited and captures the exit info (signal → undefined)', () => {
    const sink = makeSink();
    service.handleServerMessage(sink, { type: 'exit', code: 0, signal: null });
    expect(sink.state()).toBe('exited');
    expect(sink.exitInfo()).toEqual({ code: 0, signal: undefined });

    const killed = makeSink();
    service.handleServerMessage(killed, { type: 'exit', code: null, signal: 'SIGTERM' });
    expect(killed.exitInfo()).toEqual({ code: null, signal: 'SIGTERM' });
  });

  it('re-emits chat-update as a DOM event for the Salon page to refetch', () => {
    const sink = makeSink();
    const handler = vi.fn();
    window.addEventListener('quilltap:chat-update', handler);
    service.handleServerMessage(sink, {
      type: 'chat-update',
      chatId: 'c1',
      reason: 'ariel-terminal-output',
    });
    window.removeEventListener('quilltap:chat-update', handler);
    expect(handler).toHaveBeenCalledTimes(1);
    const detail = (handler.mock.calls[0][0] as CustomEvent).detail;
    expect(detail).toEqual({ chatId: 'c1', reason: 'ariel-terminal-output' });
  });

  it('ignores pong (keepalive, no state change)', () => {
    const sink = makeSink();
    service.handleServerMessage(sink, { type: 'pong' });
    expect(sink.state()).toBe('connecting');
  });
});
