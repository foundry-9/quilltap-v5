import type { RealtimeService } from '../core/realtime.service';

/**
 * The refresh strategy for a write whose server path publishes a realtime
 * topic — a port of v4 `lib/query/refresh-unless-realtime.ts` at the pin
 * `624d0d25c` (NEW at `600c2906f`).
 *
 * v4's doc, carried: after this tab's own write, refresh only while the
 * realtime socket is down. While it is up, the server's publish for that write
 * refreshes the same reads, and refreshing here as well would fetch
 * everything twice. "Only for writes that really do publish: a write with no
 * topic behind it must invalidate unconditionally, or a connected tab never
 * sees its own change."
 *
 * v5 shape: the stores are signal-held (P4.D330 R-2), so v4's query keys are
 * the stores' reload functions and `RealtimeService.connected` is the gate.
 */
export function refreshUnlessRealtime(
  realtime: Pick<RealtimeService, 'connected'>,
  reloads: ReadonlyArray<() => Promise<unknown>>,
): Promise<unknown> {
  if (realtime.connected()) return Promise.resolve();
  return Promise.all(reloads.map((reload) => reload()));
}
