/**
 * The WS pipe (P4.7b §4 extraction): the lifecycle mapping the pre-split
 * service performed inline, proven against a stubbed WebSocket — URL scheme
 * choice, JSON parse with the log-and-skip idiom, the 1006/1011 transient
 * classification, the readyState send guard.
 */

import { afterEach, describe, expect, it, vi } from 'vitest';

import type { TerminalStreamCallbacks } from './terminal-stream-transport';
import { WsTerminalStreamTransport } from './terminal-stream-transport';

class FakeWebSocket {
  static readonly CONNECTING = 0;
  static readonly OPEN = 1;
  static readonly CLOSING = 2;
  static readonly CLOSED = 3;
  static instances: FakeWebSocket[] = [];

  readyState = FakeWebSocket.CONNECTING;
  sent: string[] = [];
  closeCalls = 0;
  onopen: (() => void) | null = null;
  onmessage: ((event: { data: string }) => void) | null = null;
  onerror: (() => void) | null = null;
  onclose: ((event: { code: number }) => void) | null = null;

  constructor(readonly url: string) {
    FakeWebSocket.instances.push(this);
  }

  send(data: string): void {
    this.sent.push(data);
  }

  close(): void {
    this.closeCalls += 1;
  }
}

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

function openWithFake() {
  FakeWebSocket.instances = [];
  vi.stubGlobal('WebSocket', FakeWebSocket);
  const rec = recordingCallbacks();
  const conn = new WsTerminalStreamTransport().open('s-1', rec.callbacks);
  const ws = FakeWebSocket.instances[0];
  return { ...rec, conn: conn!, ws };
}

afterEach(() => {
  vi.unstubAllGlobals();
  vi.restoreAllMocks();
});

describe('WsTerminalStreamTransport', () => {
  it('connects to the frozen stream route, ws under http', () => {
    const { ws } = openWithFake();
    expect(ws.url).toBe(`ws://${window.location.host}/api/v1/terminals/s-1/stream`);
  });

  it('maps open / decoded frames / error onto the callbacks', () => {
    const { events, ws } = openWithFake();
    ws.onopen?.();
    ws.onmessage?.({ data: '{"type":"output","data":"$ "}' });
    ws.onerror?.();
    expect(events).toEqual([
      { kind: 'open' },
      { kind: 'message', message: { type: 'output', data: '$ ' } },
      { kind: 'error' },
    ]);
  });

  it('logs and skips an unparseable frame (the pre-split idiom)', () => {
    const { events, ws } = openWithFake();
    const err = vi.spyOn(console, 'error').mockImplementation(() => undefined);
    ws.onmessage?.({ data: 'not json' });
    expect(events).toEqual([]);
    expect(err).toHaveBeenCalledTimes(1);
  });

  it('classifies 1006/1011 closes as transient and every other code as final', () => {
    for (const [code, transient] of [
      [1006, true],
      [1011, true],
      [1000, false],
      [1005, false],
    ] as const) {
      const { events, ws } = openWithFake();
      ws.onclose?.({ code });
      expect(events).toEqual([{ kind: 'close', transient }]);
    }
  });

  it('send serializes the frozen client union, guarded on OPEN', () => {
    const { conn, ws } = openWithFake();
    conn.send({ type: 'input', data: 'ls\r' });
    expect(ws.sent).toEqual([]); // still CONNECTING — dropped
    ws.readyState = FakeWebSocket.OPEN;
    conn.send({ type: 'resize', cols: 80, rows: 24 });
    conn.send({ type: 'ping' });
    expect(ws.sent).toEqual(['{"type":"resize","cols":80,"rows":24}', '{"type":"ping"}']);
  });

  it('isOpen mirrors the readyState; close closes the socket', () => {
    const { conn, ws } = openWithFake();
    expect(conn.isOpen()).toBe(false);
    ws.readyState = FakeWebSocket.OPEN;
    expect(conn.isOpen()).toBe(true);
    conn.close();
    expect(ws.closeCalls).toBe(1);
  });

  it('returns null where the environment has no WebSocket (the pre-split guard)', () => {
    vi.stubGlobal('WebSocket', undefined);
    const rec = recordingCallbacks();
    expect(new WsTerminalStreamTransport().open('s-1', rec.callbacks)).toBeNull();
  });
});
