import { describe, expect, it } from 'vitest';

import { interpretHealth } from './core-transport';

/**
 * P4.D247 item 14 — the degraded-`/health` carve-out (the Shared contract
 * P4.D248 ↔ P4.D247, ruling R1). v4 `e5c6bd0c0` answers a damaged-but-booted
 * instance with 503 + `status: "degraded"` (`app/api/health/route.ts`), and
 * v4's own UI never reads that 503 (`hooks/useHealthCheck.ts:62-87` branches
 * only on 409) — so the instance "stays reachable for a restore". v5's shell
 * DOES read the status, so the degraded body must not land on the error
 * screen; every other 503 keeps the unhealthy mapping.
 */
describe('interpretHealth', () => {
  it('reads a degraded 503 as healthy, carrying the version as the 200 arm does', () => {
    expect(
      interpretHealth(503, {
        status: 'degraded',
        version: '4.10.0-dev.108',
        services: { structure: { status: 'degraded', problems: ['table x does not exist'] } },
      }),
    ).toEqual({ kind: 'healthy', version: '4.10.0-dev.108' });
    expect(interpretHealth(503, { status: 'degraded' })).toEqual({ kind: 'healthy' });
  });

  it('keeps an unhealthy 503 unhealthy, with its error sentence', () => {
    expect(interpretHealth(503, { status: 'unhealthy', error: 'Database unavailable' })).toEqual({
      kind: 'unhealthy',
      message: 'Database unavailable',
    });
  });

  it('keeps a 503 with no body status unhealthy, with the default sentence', () => {
    expect(interpretHealth(503, {})).toEqual({
      kind: 'unhealthy',
      message: 'The server is not ready.',
    });
  });

  it('leaves the 200 arm unchanged', () => {
    expect(interpretHealth(200, { status: 'healthy', version: '0.0.1' })).toEqual({
      kind: 'healthy',
      version: '0.0.1',
    });
    expect(interpretHealth(200, {})).toEqual({ kind: 'healthy' });
  });
});
