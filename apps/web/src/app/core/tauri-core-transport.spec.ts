/**
 * The Tauri transport unit tier (P4.7b): every §1/§2 command name, argument
 * key, and payload shape below is written against the P4.7 Shared contract
 * text — a lane-A rename must surface HERE as a spec diff at unification, not
 * as a runtime mystery. The round trips run the REAL `@tauri-apps/api`
 * `invoke`/`listen` with `mockIPC(cb, { shouldMockEvents: true })` installed
 * beneath them (the mocks module provides the event-plugin plumbing), so the
 * transport's lazy loader and the library's own IPC path are both exercised.
 */

import { afterEach, describe, expect, it, vi } from 'vitest';
import { emit } from '@tauri-apps/api/event';
import { clearMocks, mockIPC } from '@tauri-apps/api/mocks';

import type { ScopedEvent } from './core-contract';
import {
  type ConnectionState,
  type EventStreamSink,
  HttpCoreTransport,
} from './core-transport';
import { TauriCoreTransport } from './tauri-core-transport';

/** A recording sink: state transitions, accepted frames, resync bumps. */
function recordingSink() {
  const states: ConnectionState[] = [];
  const frames: ScopedEvent[] = [];
  /** The connection state observed at the moment each frame arrived. */
  const frameStates: ConnectionState[] = [];
  let resyncs = 0;
  let connection: ConnectionState = 'idle';
  const sink: EventStreamSink = {
    connectionState: () => connection,
    setConnection: (state) => {
      connection = state;
      states.push(state);
    },
    bumpResync: () => {
      resyncs += 1;
    },
    acceptFrame: (frame) => {
      frames.push(frame);
      frameStates.push(connection);
    },
  };
  return {
    sink,
    states,
    frames,
    frameStates,
    resyncs: () => resyncs,
    connection: () => connection,
  };
}

afterEach(() => {
  clearMocks();
  vi.restoreAllMocks();
});

describe('TauriCoreTransport.dispatch (§1)', () => {
  it("invokes the 'dispatch' command with the request under the 'request' key and resolves the envelope verbatim", async () => {
    const calls: Array<{ cmd: string; payload: unknown }> = [];
    mockIPC((cmd, payload) => {
      calls.push({ cmd, payload });
      return { type: 'health', data: { status: 'ok', ready: true } };
    });
    const resp = await new TauriCoreTransport().dispatch({ type: 'health' });
    expect(calls).toEqual([{ cmd: 'dispatch', payload: { request: { type: 'health' } } }]);
    expect(resp).toEqual({ type: 'health', data: { status: 'ok', ready: true } });
  });

  it('resolves a typed error envelope as data, never as a rejection', async () => {
    mockIPC(() => ({
      type: 'error',
      data: { kind: 'locked', message: 'The database is locked.', pepperState: 'needs-passphrase' },
    }));
    const resp = await new TauriCoreTransport().dispatch({ type: 'listChats' });
    expect(resp.type).toBe('error');
    if (resp.type === 'error') {
      expect(resp.data.kind).toBe('locked');
    }
  });

  it('maps an invoke rejection to the same synthetic-internal envelope the HTTP network failure uses', async () => {
    mockIPC(() => {
      throw new Error('ipc bridge down');
    });
    const tauriResp = await new TauriCoreTransport().dispatch({ type: 'health' });

    vi.spyOn(globalThis, 'fetch').mockRejectedValueOnce(new Error('ipc bridge down'));
    const httpResp = await new HttpCoreTransport().dispatch({ type: 'health' });

    // Identical shape AND identical message — the one synthetic vocabulary.
    expect(tauriResp).toEqual(httpResp);
    if (tauriResp.type === 'error') {
      expect(tauriResp.data.kind).toBe('internal');
      expect(tauriResp.data.message).toContain('Connection lost');
    }
  });

  it('maps a non-envelope resolution to a synthetic internal error', async () => {
    mockIPC(() => 42);
    const resp = await new TauriCoreTransport().dispatch({ type: 'health' });
    expect(resp.type).toBe('error');
    if (resp.type === 'error') {
      expect(resp.data.kind).toBe('internal');
      expect(resp.data.message).toContain('unexpected response');
    }
  });
});

describe('TauriCoreTransport.fetchHealth (§1) — branch parity with the HTTP interpreter', () => {
  /** The health.rs vocabulary, one fixture per branch (+ defaults + unknown). */
  const FIXTURES: Array<{ status: number; body: Record<string, unknown> }> = [
    { status: 200, body: { status: 'healthy' } },
    { status: 423, body: { status: 'locked', dbKeyState: 'needs-setup' } },
    { status: 423, body: {} }, // dbKeyState default → 'needs-passphrase'
    { status: 409, body: { status: 'lock-conflict', lockConflict: { reason: 'held' } } },
    { status: 409, body: {} }, // lockConflict default → null
    { status: 503, body: { status: 'unhealthy', error: 'still booting' } },
    { status: 503, body: {} }, // error default → 'The server is not ready.'
    { status: 418, body: {} }, // unknown status → the loud default arm
    // P4.9c §3: the version carry must reach BOTH transports identically —
    // the Tauri health command replies with the same body health.rs builds.
    { status: 200, body: { status: 'healthy', version: '0.0.28' } },
    { status: 423, body: { status: 'locked', dbKeyState: 'needs-setup', version: '0.0.28' } },
  ];

  it('interprets every {status, body} pair exactly as the HTTP transport does', async () => {
    for (const fixture of FIXTURES) {
      mockIPC((cmd) => {
        expect(cmd).toBe('health');
        return { status: fixture.status, body: fixture.body };
      });
      const viaTauri = await new TauriCoreTransport().fetchHealth();

      vi.spyOn(globalThis, 'fetch').mockResolvedValueOnce({
        status: fixture.status,
        json: async () => fixture.body,
      } as Response);
      const viaHttp = await new HttpCoreTransport().fetchHealth();

      expect(viaTauri).toEqual(viaHttp);
      clearMocks();
      vi.restoreAllMocks();
    }
  });

  it('maps an invoke rejection to unreachable (retry, like a booting server)', async () => {
    mockIPC(() => {
      throw new Error('shell not ready');
    });
    const health = await new TauriCoreTransport().fetchHealth();
    expect(health).toEqual({ kind: 'unreachable', message: 'shell not ready' });
  });
});

describe('TauriCoreTransport events (§2)', () => {
  /**
   * Install the event-mocking IPC with an `events_attach` handler that replays
   * a backlog frame BEFORE resolving — the Green-Room `active_snapshot` rule.
   * A transport that attaches before its listener is live drops the frame.
   */
  function mockEventShell() {
    const attachCalls: string[] = [];
    mockIPC(
      async (cmd) => {
        attachCalls.push(cmd);
        if (cmd === 'events_attach') {
          await emit('quilltap://event', { chatId: 'chat-1', content: 'backlog' });
          return null;
        }
        throw new Error(`unexpected command ${cmd}`);
      },
      { shouldMockEvents: true },
    );
    return { attachCalls };
  }

  it("listens BEFORE 'events_attach', so the backlog replay is not dropped, and opens only after attach resolves", async () => {
    const { attachCalls } = mockEventShell();
    const rec = recordingSink();
    const transport = new TauriCoreTransport();

    transport.connect(rec.sink);
    expect(rec.connection()).toBe('connecting');

    await vi.waitFor(() => expect(rec.connection()).toBe('open'));
    expect(attachCalls).toEqual(['events_attach']);
    // The backlog frame arrived — and arrived while still 'connecting'
    // (attach had not resolved yet when the shell replayed it).
    expect(rec.frames).toEqual([{ chatId: 'chat-1', content: 'backlog' }]);
    expect(rec.frameStates).toEqual(['connecting']);
    expect(rec.states).toEqual(['connecting', 'open']);

    transport.disconnect();
  });

  it('accepts live frames pre-parsed and skips non-object payloads', async () => {
    mockEventShell();
    const rec = recordingSink();
    const transport = new TauriCoreTransport();
    transport.connect(rec.sink);
    await vi.waitFor(() => expect(rec.connection()).toBe('open'));

    await emit('quilltap://event', { chatId: 'chat-1', content: 'live' });
    await emit('quilltap://event', '[DONE]');
    await emit('quilltap://event', null);

    expect(rec.frames).toEqual([
      { chatId: 'chat-1', content: 'backlog' },
      { chatId: 'chat-1', content: 'live' },
    ]);
    transport.disconnect();
  });

  it("bumps the resync counter on 'quilltap://resync' (payload ignored)", async () => {
    mockEventShell();
    const rec = recordingSink();
    const transport = new TauriCoreTransport();
    transport.connect(rec.sink);
    await vi.waitFor(() => expect(rec.connection()).toBe('open'));

    await emit('quilltap://resync', null);
    await emit('quilltap://resync', { ignored: true });

    expect(rec.resyncs()).toBe(2);
    // A resync is not a state transition and not a frame.
    expect(rec.states).toEqual(['connecting', 'open']);
    expect(rec.frames).toHaveLength(1);
    transport.disconnect();
  });

  it('connect is idempotent while the stream is up', async () => {
    const { attachCalls } = mockEventShell();
    const rec = recordingSink();
    const transport = new TauriCoreTransport();
    transport.connect(rec.sink);
    transport.connect(rec.sink);
    await vi.waitFor(() => expect(rec.connection()).toBe('open'));
    expect(attachCalls).toEqual(['events_attach']);
    transport.disconnect();
  });

  it('disconnect unlistens: frames and resyncs after it are dropped', async () => {
    mockEventShell();
    const rec = recordingSink();
    const transport = new TauriCoreTransport();
    transport.connect(rec.sink);
    await vi.waitFor(() => expect(rec.connection()).toBe('open'));

    transport.disconnect();
    await emit('quilltap://event', { chatId: 'chat-1', content: 'after-close' });
    await emit('quilltap://resync', null);

    expect(rec.frames).toEqual([{ chatId: 'chat-1', content: 'backlog' }]);
    expect(rec.resyncs()).toBe(0);
  });

  it("a failed attach surfaces as 'reconnecting' (the shared stream-gap vocabulary)", async () => {
    mockIPC(
      () => {
        throw new Error('forwarder failed');
      },
      { shouldMockEvents: true },
    );
    const rec = recordingSink();
    const transport = new TauriCoreTransport();
    transport.connect(rec.sink);
    await vi.waitFor(() => expect(rec.connection()).toBe('reconnecting'));
    expect(rec.states).toEqual(['connecting', 'reconnecting']);
    transport.disconnect();
  });
});
