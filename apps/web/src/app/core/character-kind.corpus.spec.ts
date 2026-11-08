import { describe, expect, it } from 'vitest';

import corpusText from '../../testing/fixtures/spa-smalls.oracle.ndjson';
import { characterKindOrDefault, isAssistantCharacter, resolveCharacterKind } from './character-kind';

/**
 * The tier-1 corpus differential for v4 `lib/characters/kind.ts` at the pin
 * `624d0d25c` (`b09ca46bb` — P4.D388 item 8). Recorded by
 * `apps/web/tooling/record-spa-smalls.mts` against v4's REAL module (see that
 * file's header for the pinned-worktree invocation); every row replays through
 * `./character-kind` and byte-compares the `JSON.stringify`'d result. The
 * image-cap rows of the same fixture replay in
 * `screens/settings/providers/max-images-per-request.corpus.spec.ts`.
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
  characterKindOrDefault: (v) => characterKindOrDefault(v),
  resolveCharacterKind: (c) => resolveCharacterKind(c),
  isAssistantCharacter: (c) => isAssistantCharacter(c),
};
/* eslint-enable @typescript-eslint/no-explicit-any */

describe('the SPA kind module agrees with v4 row for row (624d0d25c)', () => {
  it('carries the whole recorded corpus', () => {
    // A truncated fixture would make every `it.each` below vacuously green.
    expect(ROWS).toHaveLength(85);
    expect(of('characterKindOrDefault')).toHaveLength(13);
    expect(of('resolveCharacterKind')).toHaveLength(16);
    expect(of('isAssistantCharacter')).toHaveLength(16);
  });

  it.each([...of('characterKindOrDefault'), ...of('resolveCharacterKind'), ...of('isAssistantCharacter')])(
    '$kind($args) → $expected',
    (row) => {
      expect(JSON.stringify(RUN[row.kind](...row.args.map(decode)))).toBe(row.expected);
    },
  );
});
