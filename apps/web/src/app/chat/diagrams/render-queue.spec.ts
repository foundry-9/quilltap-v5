// v5-authored: v4 ships `render-queue.ts` without a test (236c04e35).
import { describe, expect, it } from 'vitest';

import { enqueueRender, nextRenderId, seedFromSource } from './render-queue';

describe('render-queue', () => {
  it('v5: runs tasks one at a time, in the order they were queued', async () => {
    const log: string[] = [];
    const task = (name: string, ms: number) => () =>
      new Promise<string>((resolve) => {
        log.push(`start ${name}`);
        setTimeout(() => {
          log.push(`end ${name}`);
          resolve(name);
        }, ms);
      });
    const results = await Promise.all([
      enqueueRender(task('a', 20)),
      enqueueRender(task('b', 5)),
      enqueueRender(task('c', 1)),
    ]);
    expect(results).toEqual(['a', 'b', 'c']);
    expect(log).toEqual(['start a', 'end a', 'start b', 'end b', 'start c', 'end c']);
  });

  it('v5: a rejecting task does not jam the next', async () => {
    const failed = enqueueRender(() => Promise.reject(new Error('boom')));
    const next = enqueueRender(() => Promise.resolve('ok'));
    await expect(failed).rejects.toThrow('boom');
    await expect(next).resolves.toBe('ok');
  });

  it('v5: seeds are stable, never 0, and differ between sources', () => {
    expect(seedFromSource('graph TD; A-->B')).toBe(seedFromSource('graph TD; A-->B'));
    expect(seedFromSource('graph TD; A-->B')).not.toBe(seedFromSource('graph TD; A-->C'));
    expect(seedFromSource('')).not.toBe(0);
    expect(seedFromSource('graph TD; A-->B')).toBeGreaterThan(0);
    expect(seedFromSource('graph TD; A-->B')).toBeLessThan(2 ** 31);
  });

  it('v5: render ids carry the prefix and a rising serial', () => {
    const a = nextRenderId('qt-mermaid');
    const b = nextRenderId('qt-mermaid');
    expect(a).toMatch(/^qt-mermaid-[0-9a-z]+-\d+$/);
    expect(Number(b.split('-').pop())).toBe(Number(a.split('-').pop()) + 1);
  });
});
