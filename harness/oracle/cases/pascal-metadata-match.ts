/**
 * Oracle case: the fail-soft METADATA comparison table, driven directly
 * (P4.D333, v4 `a53024d3c`).
 *
 * Drives the REAL code:
 *   metadataComparatorHolds  (lib/pascal/metadata-match.ts)
 *
 * ## Why a direct drive, beside the entrance rows
 *
 * `a53024d3c` taught the table a LIST rule: a key holding an array of
 * primitives compares entry by entry, and the four ordering keys decline on it
 * BEFORE any operand resolves — where the scalar walk resolves each operand
 * first. That ORDER is invisible through `matchesWhen` and the gate (their
 * resolvers never fail on a loadable definition); only a resolver that records
 * what it was asked, and throws on a scripted key, can tell the two walks
 * apart. So every row records:
 *
 *   - `asked`    — the comparator keys `resolveOperand` was called with, in order;
 *   - `declined` — the reasons `onDecline` received, in order;
 *   - `held`     — the verdict, or `null` when the resolver threw;
 *   - `threw`    — the scripted key the resolver threw on, or `null`.
 *
 * The resolver answers a key from the row's `operands` script, else with the
 * comparator's own (literal) operand — the gate's identity resolver.
 *
 * Run from inside the server checkout (v4 @ 322a5488e, Node 24
 * `~/.nvm/versions/node/v24.13.1/bin`):
 *   cd ~/source/quilltap-server
 *   TZ=UTC npx tsx \
 *     <V5W>/harness/oracle/cases/pascal-metadata-match.ts \
 *     > /tmp/oracle-pascal-metadata-match.ndjson
 */

import { metadataComparatorHolds } from '@/lib/pascal/metadata-match';
import type { MetadataComparator } from '@/lib/pascal/custom-tool.types';

type Primitive = number | string | boolean;
interface Script {
  operands?: Record<string, Primitive>;
  throws?: string;
}

const SHEET: Record<string, unknown> = {
  clearance: 5,
  house: 'Aurum',
  trusted: true,
  label: '7',
  nothing: null,
  nested: { a: 1 },
  knownLanguages: ['Trade Cant', 7],
  numbers: [1, 2, 3],
  emptyList: [],
  mixed: [1, 'x', true],
  withNull: [1, null],
  sealedOrders: [{ seal: 'red' }],
};

const P_NUM = { $param: 'difficulty' };
const P_STR = { $param: 'material' };

const cases: Array<[string, Record<string, unknown>, string, Script?]> = [
  // ---- the scalar walk: a control for the entrance rows' semantics --------
  ['scalar-eq-holds', { eq: 'Aurum' }, 'house'],
  ['scalar-eq-misses', { eq: 'Ferro' }, 'house'],
  ['scalar-eq-strict-type', { eq: 7 }, 'label'],
  ['scalar-gte-holds', { gte: 3 }, 'clearance'],
  ['scalar-gte-and-lt-anded', { gte: 3, lt: 5 }, 'clearance'],
  ['scalar-orders-a-string-declines', { gt: 1 }, 'house'],
  ['scalar-contains-holds', { contains: 'Aur' }, 'house'],
  ['scalar-searches-a-number-declines', { contains: 'x' }, 'clearance'],
  ['scalar-ncontains-holds', { ncontains: 'Ferro' }, 'house'],
  ['scalar-neq-holds', { neq: 'Ferro' }, 'house'],
  ['scalar-absent-key-declines', { eq: 1 }, 'missing'],
  ['scalar-null-declines', { eq: 1 }, 'nothing'],
  ['scalar-object-declines', { eq: 1 }, 'nested'],
  ['scalar-boolean-eq', { eq: true }, 'trusted'],
  // ---- the list rule --------------------------------------------------------
  ['list-eq-holds', { eq: 'Trade Cant' }, 'knownLanguages'],
  ['list-eq-misses', { eq: 'Trade' }, 'knownLanguages'],
  ['list-eq-number-entry', { eq: 7 }, 'knownLanguages'],
  ['list-neq-holds', { neq: 'Latin' }, 'knownLanguages'],
  ['list-neq-misses', { neq: 'Trade Cant' }, 'knownLanguages'],
  ['list-contains-holds', { contains: 'Cant' }, 'knownLanguages'],
  ['list-ncontains-holds', { ncontains: 'Latin' }, 'knownLanguages'],
  ['list-ncontains-misses', { ncontains: 'Trade' }, 'knownLanguages'],
  ['list-contains-skips-non-string-entries', { contains: '7' }, 'knownLanguages'],
  ['list-contains-non-string-operand-declines', { contains: P_NUM }, 'knownLanguages', { operands: { contains: 7 } }],
  ['list-ordering-declines', { gt: 1 }, 'numbers'],
  ['list-lte-declines', { lte: 9 }, 'numbers'],
  ['list-empty-eq-misses', { eq: 1 }, 'emptyList'],
  ['list-empty-neq-holds', { neq: 1 }, 'emptyList'],
  ['list-empty-ordering-declines', { gte: 0 }, 'emptyList'],
  ['list-mixed-eq-number', { eq: 1 }, 'mixed'],
  ['list-mixed-eq-boolean', { eq: true }, 'mixed'],
  ['list-all-primitives-with-null-entry', { eq: 1 }, 'withNull'],
  ['list-of-objects-declines', { eq: 'red' }, 'sealedOrders'],
  ['list-eq-and-contains-anded', { eq: 'Trade Cant', contains: 'Latin' }, 'knownLanguages'],
  // ---- the operand ORDER, told apart by a throwing resolver ---------------
  ['list-orders-declines-before-resolving', { gt: P_NUM }, 'numbers', { throws: 'gt' }],
  ['scalar-orders-resolves-first', { gt: P_NUM }, 'clearance', { throws: 'gt' }],
  ['list-eq-then-neq-order', { eq: P_STR, neq: P_STR }, 'knownLanguages', { operands: { eq: 'Trade Cant', neq: 'Latin' } }],
  ['scalar-walk-order', { contains: 'A', neq: 'B', eq: 'Aurum', gte: P_NUM }, 'house', { operands: { gte: 1 } }],
  ['list-throw-on-neq-after-eq', { eq: 'Trade Cant', neq: P_STR }, 'knownLanguages', { throws: 'neq' }],
  ['list-orders-beside-eq-declines-first', { eq: P_STR, lt: 9 }, 'numbers', { throws: 'eq' }],
];

const rows: unknown[] = [];
for (const [id, comparator, key, script = {}] of cases) {
  const asked: string[] = [];
  const declined: string[] = [];
  let held: boolean | null = null;
  let threw: string | null = null;
  try {
    held = metadataComparatorHolds(comparator as unknown as MetadataComparator, key, SHEET, {
      resolveOperand: (comparatorKey) => {
        asked.push(comparatorKey);
        if (script.throws === comparatorKey) throw new Error(comparatorKey);
        if (script.operands && comparatorKey in script.operands) return script.operands[comparatorKey];
        return comparator[comparatorKey] as Primitive;
      },
      onDecline: (reason) => declined.push(reason),
    });
  } catch (e) {
    threw = (e as Error).message;
  }
  rows.push({
    kind: 'match',
    id,
    comparatorJson: JSON.stringify(comparator),
    key,
    sheetJson: JSON.stringify(SHEET),
    scriptJson: JSON.stringify(script),
    resultJson: JSON.stringify({ held, asked, declined, threw }),
  });
}

for (const r of rows) process.stdout.write(JSON.stringify(r) + '\n');
