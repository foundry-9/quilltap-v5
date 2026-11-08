import { describe, expect, it } from 'vitest';

import corpusText from '../../../../testing/fixtures/spa-smalls.oracle.ndjson';
import { parseMaxImagesPerRequest, resolveMaxImagesPerRequest } from './max-images-per-request';

/**
 * The tier-1 corpus differential for v4 `lib/llm/connection-profile-utils.ts`
 * `resolveMaxImagesPerRequest` / `parseMaxImagesPerRequest` at the pin
 * `624d0d25c` (`d439f6dc9` — P4.D388 item 8). Recorded by
 * `apps/web/tooling/record-spa-smalls.mts` against v4's REAL module; every
 * row replays through `./max-images-per-request` and byte-compares. JS
 * `Number()`'s coercions are the point (`'0x10'` → 16, `'0b11'` → 3,
 * `[7]` refused because it is not a string).
 */

interface Row {
  kind: string;
  args: unknown[];
  expected: string;
}

const ROWS: Row[] = (corpusText as unknown as string)
  .split('\n')
  .filter((line) => line.trim() !== '')
  .map((line) => JSON.parse(line) as Row);

/** The recorder's encodings (`"__undefined__"`, `"__nan__"`, `"__inf__"`), undone at any depth. */
const decode = (v: unknown): unknown =>
  v === '__undefined__'
    ? undefined
    : v === '__nan__'
      ? NaN
      : v === '__inf__'
        ? Infinity
        : Array.isArray(v)
          ? v.map(decode)
          : v && typeof v === 'object'
            ? Object.fromEntries(Object.entries(v).map(([k, x]) => [k, decode(x)]))
            : v;

const of = (kind: Row['kind']) => ROWS.filter((r) => r.kind === kind);

/* eslint-disable @typescript-eslint/no-explicit-any */
const RUN: Record<string, (...a: any[]) => unknown> = {
  resolveMaxImagesPerRequest: (p) => resolveMaxImagesPerRequest(p),
  parseMaxImagesPerRequest: (v) => parseMaxImagesPerRequest(v),
};
/* eslint-enable @typescript-eslint/no-explicit-any */

describe('the image-cap twins agree with v4 row for row (624d0d25c)', () => {
  it('carries the whole recorded corpus', () => {
    // A truncated fixture would make every `it.each` below vacuously green.
    expect(ROWS).toHaveLength(85);
    expect(of('resolveMaxImagesPerRequest')).toHaveLength(15);
    expect(of('parseMaxImagesPerRequest')).toHaveLength(25);
  });

  it.each([...of('resolveMaxImagesPerRequest'), ...of('parseMaxImagesPerRequest')])(
    '$kind($args) → $expected',
    (row) => {
      expect(JSON.stringify(RUN[row.kind](...row.args.map(decode)))).toBe(row.expected);
    },
  );
});
