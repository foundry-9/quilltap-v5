import { signal } from '@angular/core';
import { describe, expect, it, vi } from 'vitest';

import { refreshUnlessRealtime } from './refresh-unless-realtime';

/**
 * v4 `lib/query/refresh-unless-realtime.ts` at the pin `624d0d25c` (NEW at
 * `600c2906f`): after a write whose server path publishes a topic, refresh
 * only while the socket is NOT connected.
 */
describe('refreshUnlessRealtime', () => {
  it('connected: resolves at once and runs no reload (the publish does it)', async () => {
    const reload = vi.fn(async () => 'x');
    await expect(refreshUnlessRealtime({ connected: signal(true) }, [reload, reload])).resolves.toBeUndefined();
    expect(reload).not.toHaveBeenCalled();
  });

  it('disconnected: runs every reload and resolves when they all have', async () => {
    const order: string[] = [];
    const a = vi.fn(async () => order.push('a'));
    const b = vi.fn(async () => order.push('b'));
    await refreshUnlessRealtime({ connected: signal(false) }, [a, b]);
    expect(a).toHaveBeenCalledTimes(1);
    expect(b).toHaveBeenCalledTimes(1);
    expect(order.sort()).toEqual(['a', 'b']);
  });

  it('disconnected with no reloads resolves', async () => {
    await expect(refreshUnlessRealtime({ connected: signal(false) }, [])).resolves.toEqual([]);
  });
});
