import { afterEach, describe, expect, it, vi } from 'vitest';

import { CoreClient, parseEventData } from './core-client';
import { CoreDispatchError } from './core-contract';

function mockFetchOnce(status: number, body: unknown) {
  const res = {
    status,
    json: async () => body,
    text: async () => JSON.stringify(body),
  } as Response;
  return vi.spyOn(globalThis, 'fetch').mockResolvedValueOnce(res);
}

describe('parseEventData', () => {
  it('skips empty / [DONE] / {} sentinels', () => {
    expect(parseEventData('')).toBeNull();
    expect(parseEventData('  ')).toBeNull();
    expect(parseEventData('[DONE]')).toBeNull();
    expect(parseEventData('{}')).toBeNull();
  });

  it('parses a scope-tagged chat frame', () => {
    const frame = parseEventData('{"chatId":"chat-1","content":"hi"}');
    expect(frame).toEqual({ chatId: 'chat-1', content: 'hi' });
  });

  it('swallows chunking-artifact parse errors', () => {
    expect(parseEventData('{"chatId":"chat-1","content"')).toBeNull();
  });
});

describe('CoreClient.dispatch', () => {
  afterEach(() => vi.restoreAllMocks());

  it('returns the typed envelope on success', async () => {
    mockFetchOnce(200, { type: 'health', data: { status: 'ok', version: '1', ready: true, pepperState: 'resolved' } });
    const client = new CoreClient();
    const resp = await client.dispatch({ type: 'health' });
    expect(resp.type).toBe('health');
    if (resp.type === 'health') {
      expect(resp.data.ready).toBe(true);
      expect(resp.data.pepperState).toBe('resolved');
    }
  });

  it('returns the typed error envelope on a Locked 503 (with the setup body alongside)', async () => {
    mockFetchOnce(503, {
      type: 'error',
      data: { kind: 'locked', message: 'The database is locked.', pepperState: 'needs-passphrase' },
      error: 'Setup required',
      setupUrl: '/setup',
      pepperState: 'needs-passphrase',
    });
    const client = new CoreClient();
    const resp = await client.dispatch({ type: 'listChats' });
    expect(resp.type).toBe('error');
    if (resp.type === 'error') {
      expect(resp.data.kind).toBe('locked');
      expect(resp.data.pepperState).toBe('needs-passphrase');
    }
  });

  it('synthesises an internal error on a network failure', async () => {
    vi.spyOn(globalThis, 'fetch').mockRejectedValueOnce(new Error('ECONNREFUSED'));
    const client = new CoreClient();
    const resp = await client.dispatch({ type: 'health' });
    expect(resp.type).toBe('error');
    if (resp.type === 'error') {
      expect(resp.data.kind).toBe('internal');
      expect(resp.data.message).toContain('Connection lost');
    }
  });

  it('dispatchExpect throws a CoreDispatchError on an error envelope', async () => {
    mockFetchOnce(400, { type: 'error', data: { kind: 'bad-request', message: 'Invalid passphrase' } });
    const client = new CoreClient();
    await expect(client.dispatchExpect({ type: 'unlock', passphrase: 'nope' }, 'unlockState')).rejects.toBeInstanceOf(
      CoreDispatchError,
    );
  });
});

describe('CoreClient.fetchHealth', () => {
  afterEach(() => vi.restoreAllMocks());

  it('maps 200 → healthy', async () => {
    mockFetchOnce(200, { status: 'healthy' });
    expect(await new CoreClient().fetchHealth()).toEqual({ kind: 'healthy' });
  });

  it('maps 423 → locked with the dbKeyState', async () => {
    mockFetchOnce(423, { status: 'locked', dbKeyState: 'needs-setup' });
    expect(await new CoreClient().fetchHealth()).toEqual({ kind: 'locked', pepperState: 'needs-setup' });
  });

  it('maps 409 → lock-conflict', async () => {
    mockFetchOnce(409, { status: 'lock-conflict', lockConflict: { reason: 'held' } });
    const health = await new CoreClient().fetchHealth();
    expect(health.kind).toBe('lock-conflict');
  });

  it('maps 503 → unhealthy', async () => {
    mockFetchOnce(503, { status: 'unhealthy', error: 'still booting' });
    const health = await new CoreClient().fetchHealth();
    expect(health).toEqual({ kind: 'unhealthy', message: 'still booting' });
  });

  it('maps a network failure → unreachable', async () => {
    vi.spyOn(globalThis, 'fetch').mockRejectedValueOnce(new Error('down'));
    const health = await new CoreClient().fetchHealth();
    expect(health.kind).toBe('unreachable');
  });
});
