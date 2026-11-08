/**
 * Record the SPA-smalls tier-1 corpus from v4's REAL modules at the pin
 * `624d0d25c` — `lib/characters/kind.ts` (`b09ca46bb`) and
 * `lib/llm/connection-profile-utils.ts` (`d439f6dc9`) — P4.D388 item 8.
 * `core/character-kind.corpus.spec.ts` and
 * `screens/settings/providers/max-images-per-request.corpus.spec.ts` replay
 * every row through the SPA's TS twins and byte-compare (`JSON.stringify`).
 *
 * Regenerate — run tsx FROM the pinned v4 worktree (its cwd resolves the `@/`
 * tsconfig aliases the modules import), with `QT_V4_ROOT` naming the same tree:
 *   cd <pin> && QT_V4_ROOT=$PWD PATH=$HOME/.nvm/versions/node/v24.13.1/bin:$PATH \
 *     npx tsx <v5>/apps/web/tooling/record-spa-smalls.mts \
 *     > <v5>/apps/web/src/testing/fixtures/spa-smalls.oracle.ndjson
 *
 * Row shape: `{kind, args, expected}`. `args` is the call's argument list as
 * JSON, with `"__undefined__"` standing for `undefined` (JSON has none),
 * `"__nan__"` for `NaN` and `"__inf__"` for `Infinity`, at any depth; an object
 * argument that omits a key stands for an absent key. `expected` is
 * `JSON.stringify(result)`.
 */

import { homedir } from 'node:os';
import { resolve } from 'node:path';
import { pathToFileURL } from 'node:url';

const V4_ROOT = process.env['QT_V4_ROOT'] ?? resolve(homedir(), 'source/quilltap-server');
const load = async <T,>(rel: string): Promise<T> =>
  (await import(pathToFileURL(resolve(V4_ROOT, rel)).href)) as T;

const kind = await load<{
  characterKindOrDefault(value: unknown): string;
  resolveCharacterKind(character: unknown): string;
  isAssistantCharacter(character: unknown): boolean;
}>('lib/characters/kind.ts');
const utils = await load<{
  resolveMaxImagesPerRequest(profile: unknown): number;
  parseMaxImagesPerRequest(value: unknown): number | null;
}>('lib/llm/connection-profile-utils.ts');

const U = '__undefined__';
const NAN = '__nan__';
const INF = '__inf__';
const decode = (v: unknown): unknown => {
  if (v === U) return undefined;
  if (v === NAN) return NaN;
  if (v === INF) return Infinity;
  if (Array.isArray(v)) return v.map(decode);
  if (v && typeof v === 'object') {
    return Object.fromEntries(Object.entries(v as object).map(([k, x]) => [k, decode(x)]));
  }
  return v;
};
const rows: string[] = [];
const emit = (k: string, args: unknown[], fn: (...a: unknown[]) => unknown): void => {
  rows.push(JSON.stringify({ kind: k, args, expected: JSON.stringify(fn(...args.map(decode))) }));
};

const KIND_VALUES: unknown[] = [
  'roleplay',
  'assistant',
  'Assistant',
  'ASSISTANT',
  ' assistant',
  '',
  null,
  U,
  1,
  true,
  {},
  ['assistant'],
  'butler',
];

for (const v of KIND_VALUES) emit('characterKindOrDefault', [v], (x) => kind.characterKindOrDefault(x));

const CHARACTERS: unknown[] = [null, U, {}, ...KIND_VALUES.map((v) => ({ kind: v }))];
for (const c of CHARACTERS) emit('resolveCharacterKind', [c], (x) => kind.resolveCharacterKind(x));
for (const c of CHARACTERS) emit('isAssistantCharacter', [c], (x) => kind.isAssistantCharacter(x));

const CAPS: unknown[] = [1, 4, 50, 0, 51, -1, 2.5, 1e1, '7', null, NAN, INF];
const PROFILES: unknown[] = [...CAPS.map((v) => ({ maxImagesPerRequest: v })), {}, null, U];
for (const p of PROFILES)
  emit('resolveMaxImagesPerRequest', [p], (x) => utils.resolveMaxImagesPerRequest(x));

const PARSE_VALUES: unknown[] = [
  '7',
  ' 7 ',
  '7.0',
  '07',
  '0x10',
  '1e1',
  '0b11',
  '+5',
  '-1',
  '0',
  '50',
  '51',
  '',
  '   ',
  'abc',
  '7px',
  7,
  7.5,
  0,
  null,
  U,
  true,
  [],
  [7],
  {},
];
for (const v of PARSE_VALUES)
  emit('parseMaxImagesPerRequest', [v], (x) => utils.parseMaxImagesPerRequest(x));

process.stdout.write(rows.join('\n') + '\n');
