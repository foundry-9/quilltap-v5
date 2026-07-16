/**
 * The service-side stream logic over the P4.7b §4 seam: ping cadence,
 * reconnect/backoff, and state transitions live in `TerminalSessionService`
 * and are transport-agnostic — proven here with a fake pipe. Both real pipes
 * feed this same path: the WS close handler classifies 1006/1011 as
 * transient, the Tauri pipe classifies an attach failure the same way.
 */

import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import type { WsClientMessage } from './terminal-protocol';
import { TerminalSessionService } from './terminal-session.service';
import type {
  TerminalStreamCallbacks,
  TerminalStreamConnection,
  TerminalStreamTransport,
} from './terminal-stream-transport';

class FakeConnection implements TerminalStreamConnection {
  open = false;
  sent: WsClientMessage[] = [];
  closeCalls = 0;
  isOpen(): boolean {
    return this.open;
  }
  send(message: WsClientMessage): void {
    if (this.open) {
      this.sent.push(message);
    }
  }
  close(): void {
    this.closeCalls += 1;
    this.open = false;
  }
}

class FakeTransport implements TerminalStreamTransport {
  readonly opens: Array<{
    sessionId: string;
    callbacks: TerminalStreamCallbacks;
    conn: FakeConnection;
  }> = [];
  open(sessionId: string, callbacks: TerminalStreamCallbacks): TerminalStreamConnection {
    const conn = new FakeConnection();
    this.opens.push({ sessionId, callbacks, conn });
    return conn;
  }
  last() {
    return this.opens[this.opens.length - 1];
  }
  /** The pipe reports live (WS onopen / attach resolved). */
  openLast(): void {
    this.last().conn.open = true;
    this.last().callbacks.onOpen();
  }
  /** The pipe died (a real pipe is not open by the time onClose fires). */
  closeLast(info: { transient: boolean }): void {
    this.last().conn.open = false;
    this.last().callbacks.onClose(info);
  }
}

class TestService extends TerminalSessionService {
  constructor(fake: TerminalStreamTransport) {
    super();
    this.transport = fake;
  }
}

describe('TerminalSessionService over the stream-transport seam', () => {
  let fake: FakeTransport;
  let service: TestService;

  beforeEach(() => {
    vi.useFakeTimers();
    fake = new FakeTransport();
    service = new TestService(fake);
  });

  afterEach(() => {
    vi.useRealTimers();
  });

  it('acquire opens the pipe for the session id and goes connecting → live', () => {
    const handle = service.acquire('s-1');
    expect(fake.opens).toHaveLength(1);
    expect(fake.last().sessionId).toBe('s-1');
    expect(handle.state()).toBe('connecting');
    fake.openLast();
    expect(handle.state()).toBe('live');
    handle.release();
  });

  it('pings on the 30s cadence while live', () => {
    const handle = service.acquire('s-1');
    fake.openLast();
    vi.advanceTimersByTime(30_000);
    vi.advanceTimersByTime(30_000);
    expect(fake.last().conn.sent).toEqual([{ type: 'ping' }, { type: 'ping' }]);
    handle.release();
  });

  it('send/resize ride the frozen client union', () => {
    const handle = service.acquire('s-1');
    fake.openLast();
    handle.send('ls\r');
    handle.resize(120, 40);
    expect(fake.last().conn.sent).toEqual([
      { type: 'input', data: 'ls\r' },
      { type: 'resize', cols: 120, rows: 40 },
    ]);
    handle.release();
  });

  it('a transient close reconnects with linear backoff and resets the count on open', () => {
    const handle = service.acquire('s-1');
    fake.openLast();

    fake.closeLast({ transient: true });
    expect(fake.opens).toHaveLength(1); // not yet — backoff pending
    vi.advanceTimersByTime(2_000);
    expect(fake.opens).toHaveLength(2);
    expect(handle.state()).toBe('connecting');

    fake.closeLast({ transient: true });
    vi.advanceTimersByTime(2_000); // second retry backs off 4s, not 2s
    expect(fake.opens).toHaveLength(2);
    vi.advanceTimersByTime(2_000);
    expect(fake.opens).toHaveLength(3);

    // A successful open resets the counter: the next drop backs off 2s again.
    fake.openLast();
    expect(handle.state()).toBe('live');
    fake.closeLast({ transient: true });
    vi.advanceTimersByTime(2_000);
    expect(fake.opens).toHaveLength(4);
    handle.release();
  });

  it('gives up after MAX_RECONNECTS transient closes without an open', () => {
    const handle = service.acquire('s-1');
    fake.openLast();
    for (const backoff of [2_000, 4_000, 6_000]) {
      fake.closeLast({ transient: true });
      vi.advanceTimersByTime(backoff);
    }
    expect(fake.opens).toHaveLength(4); // initial + 3 retries
    fake.closeLast({ transient: true });
    vi.advanceTimersByTime(60_000);
    expect(fake.opens).toHaveLength(4); // no fourth retry
    expect(handle.state()).toBe('error');
    handle.release();
  });

  it('a non-transient close is final: error state, no retry', () => {
    const handle = service.acquire('s-1');
    fake.openLast();
    fake.closeLast({ transient: false });
    vi.advanceTimersByTime(60_000);
    expect(fake.opens).toHaveLength(1);
    expect(handle.state()).toBe('error');
    handle.release();
  });

  it('release closes the pipe and a late close event schedules nothing', () => {
    const handle = service.acquire('s-1');
    fake.openLast();
    handle.release();
    expect(fake.last().conn.closeCalls).toBe(1);
    fake.closeLast({ transient: true });
    vi.advanceTimersByTime(60_000);
    expect(fake.opens).toHaveLength(1); // disposed — no reconnect
  });

  it('ref-counting: the pipe closes only when the last handle releases', () => {
    const first = service.acquire('s-1');
    const second = service.acquire('s-1');
    expect(fake.opens).toHaveLength(1); // one pipe, two handles
    fake.openLast();
    first.release();
    expect(fake.last().conn.closeCalls).toBe(0);
    second.release();
    expect(fake.last().conn.closeCalls).toBe(1);
  });
});
