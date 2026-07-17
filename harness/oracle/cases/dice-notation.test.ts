/**
 * @jest-environment node
 *
 * ORACLE for the shared dice module (P4.d5 unit 1; v4 `lib/pascal/dice.ts`, new
 * at `a33ac8b8`).
 *
 * Drives v4's REAL `parseDiceNotation` / `scanDiceNotation` /
 * `formatDiceNotation` / `rollNotation` / `formatDiceBreakdown` / `flipCoin`
 * over a committed corpus, emitting one NDJSON row per case for a tier-1 exact
 * diff. The corpus mirrors v4's own `__tests__/unit/lib/pascal/dice.test.ts` and
 * extends it where the Rust port has fidelity traps the TS side cannot have:
 * the two regexes' whitespace disagreement, ASCII-`\b` non-ASCII adjacency, the
 * JS-`\s` set (U+FEFF is whitespace, U+0085 is NOT), and the trailing-`\b`
 * fallback that turns "3d6+2x" into a bare 3d6.
 *
 * `crypto.randomBytes` is the ONLY thing mocked (the roll cases replay a
 * committed byte sequence — the same bytes the Rust `FixedBytes` source
 * replays). `dice.ts` imports nothing else, so there is no DB and no fixture.
 *
 * Run from the v4 server checkout under Node 24 (jest ignores `.claude/` paths,
 * so a worktree lane must mirror this file to /tmp first — see the Rust test
 * header for the full recipe):
 *   N=~/.nvm/versions/node/v24.13.1/bin ; V5=~/source/quilltap-v5
 *   cd ~/source/quilltap-server
 *   QT_ORACLE_OUT=/tmp/oracle-dice-notation.ndjson \
 *     $N/npx jest --silent --watchman=false --roots "$PWD" \
 *       --roots /tmp/qt-oracle-dice -- dice-notation
 */

import * as fs from 'fs';

// The injected byte stream (mirrors the Rust `FixedBytes`). The mocked
// `crypto.randomBytes(n)` serves `n` bytes from here and advances the cursor;
// exhaustion throws (catching an unexpected consumer / a byte-shortage).
let currentBytes: number[] = [];
let byteCursor = 0;

interface Notation {
  count: number;
  sides: number;
  modifier: number;
}

/** Strict-parse cases: the whole string must be notation. */
const PARSE_CASES: Array<[string, string]> = [
  ['bare-d20', 'd20'],
  ['count-and-sides', '3d6'],
  ['positive-modifier', '3d6+2'],
  ['negative-modifier', '2d10-1'],
  // The strict form ALLOWS whitespace around the sign — the scan form does not.
  ['strict-interior-space', '  2d10 - 1 '],
  ['strict-space-plus', '3d6 + 2'],
  ['strict-leading-space', '   d6'],
  ['uppercase', '2D6+1'],
  ['not-purely-notation', 'roll 3d6 please'],
  ['trailing-word', '3d6+2 total'],
  // Bounds — skipped, never clamped.
  ['reject-d1', 'd1'],
  ['reject-d0', 'd0'],
  ['reject-d1001', 'd1001'],
  ['reject-0d6', '0d6'],
  ['reject-101d6', '101d6'],
  ['reject-modifier-1001', '1d6+1001'],
  ['reject-modifier-neg-1001', '1d6-1001'],
  ['accept-modifier-1000', '1d6+1000'],
  ['accept-modifier-neg-1000', '1d6-1000'],
  ['bounds-max', '100d1000'],
  ['bounds-min', '1d2'],
  // Overflow: parseInt gives a huge float; Number.isInteger passes but the
  // bounds reject. The Rust port parses to f64 for exactly this reason.
  ['overflow-count', '99999999999999999999d6'],
  ['overflow-sides', 'd99999999999999999999'],
  ['overflow-modifier', '1d6+99999999999999999999'],
  // JS `\s` is NOT Rust's `\s`, in BOTH directions: U+FEFF IS JS whitespace
  // (Rust's `\s` excludes it) and U+0085 NEL is NOT (Rust's `\s` includes it).
  // That is why the Rust port builds its pattern from `jsstr::JS_WS_CLASS`
  // rather than writing `\s`. Escapes, not literals — these are invisible.
  ['strict-feff-is-space', '\u{FEFF}3d6+2\u{FEFF}'],
  ['strict-feff-around-sign', '3d6\u{FEFF}+\u{FEFF}2'],
  ['strict-nel-not-space', '\u{0085}3d6'],
  ['strict-nel-around-sign', '3d6\u{0085}+\u{0085}2'],
  ['strict-nbsp-is-space', '\u{00A0}3d6\u{00A0}'],
  ['strict-ideographic-space', '\u{3000}3d6\u{3000}'],
  ['strict-vertical-tab-is-space', '\u{000B}3d6\u{000B}'],
  // Junk.
  ['empty', ''],
  ['no-sides', 'd'],
  ['sign-without-magnitude', '3d6+'],
  ['double-sign', '3d6++2'],
];

/** Prose-scan cases: notation embedded in text. */
const SCAN_CASES: Array<[string, string]> = [
  ['embedded', 'I roll d6'],
  ['several-in-order', 'Roll 2d6 for damage and d20 for attack'],
  ['closed-up-modifier', 'deal 3d6+2 damage'],
  ['closed-up-negative', 'deal 2d10-1 damage'],
  // THE disambiguation pair — the reason the two regexes differ.
  ['spaced-not-a-modifier', 'Rolling 2d6 - 1 apple remains'],
  ['spaced-plus-not-a-modifier', '2d6 + 1 apple'],
  ['closed-up-is-a-modifier', '2d6-1'],
  ['skips-out-of-bounds', 'a d1 and a 500d6 and a real 2d6'],
  ['no-notation', 'no dice here, just words'],
  ['empty', ''],
  // The trailing-\b fallback: "+2x" fails the boundary, so the optional group
  // is dropped rather than the whole match.
  ['trailing-letter-after-modifier', '3d6+2x'],
  ['trailing-underscore-after-modifier', '3d6+2_'],
  ['trailing-period-after-modifier', '3d6+2.'],
  ['modifier-then-digit-boundary', '3d6+2 and 1d4'],
  // ASCII-\b quirks (a non-ASCII letter is a boundary from the pattern's side).
  ['nonascii-before', 'éd6'],
  ['nonascii-name', 'José rolls 2d6+1'],
  ['trailing-letter', 'cardd6x'],
  ['uppercase', 'roll a D6+2'],
  ['leading-zeros', 'roll 007d06+02 please'],
  ['sides-then-modifier-overflow', '2d6+99999999999999999999'],
  ['newline-between', '2d6\n+1'],
  ['multiple-modifiers', '1d4+1 2d6-2 3d8+3'],
];

/** Format cases: a notation rendered back to canonical text. */
const FORMAT_CASES: Array<[string, Notation]> = [
  ['bare', { count: 3, sides: 6, modifier: 0 }],
  ['positive', { count: 3, sides: 6, modifier: 2 }],
  ['negative', { count: 2, sides: 10, modifier: -1 }],
  ['single-die', { count: 1, sides: 20, modifier: 0 }],
  ['max-modifier', { count: 1, sides: 6, modifier: 1000 }],
  ['min-modifier', { count: 1, sides: 6, modifier: -1000 }],
];

/** Roll cases: a notation + the committed byte stream it draws. */
const ROLL_CASES: Array<[string, Notation, number[]]> = [
  // d6 (bytesNeeded=1, limit=252): 3→4, 1→2, 5→6 = 12; +2 = 14.
  ['3d6+2', { count: 3, sides: 6, modifier: 2 }, [3, 1, 5]],
  ['3d6-bare', { count: 3, sides: 6, modifier: 0 }, [3, 1, 5]],
  ['2d10-1', { count: 2, sides: 10, modifier: -1 }, [4, 7]],
  ['1d20-single', { count: 1, sides: 20, modifier: 0 }, [9]],
  // Rejection sampling survives the move: 0xFF (255) >= 252 rejects, 0x05 lands.
  ['rejection-sample', { count: 1, sides: 6, modifier: 3 }, [255, 5]],
  // d1000 needs 2 bytes, little-endian: [16,0]→16→17, [232,3]→1000→1.
  ['two-byte-width', { count: 2, sides: 1000, modifier: -5 }, [16, 0, 232, 3]],
  ['max-bounds', { count: 1, sides: 1000, modifier: 1000 }, [0, 0]],
];

/** Coin cases: `flipCoin` returns bare 'heads'/'tails' strings. */
const COIN_CASES: Array<[string, number, number[]]> = [
  ['single-heads', 1, [0]],
  ['single-tails', 1, [1]],
  ['multi', 4, [0, 1, 2, 3]],
  ['zero', 0, []],
];

async function main(): Promise<void> {
  const outPath = process.env.QT_ORACLE_OUT;
  if (!outPath) throw new Error('QT_ORACLE_OUT must point at the NDJSON file to write');

  jest.resetModules();
  // Patch ONLY randomBytes; keep every other crypto export real. No
  // `__esModule: true` and no explicit `default` (the [[jest-crypto-randombytes-mock]]
  // rule) so esModuleInterop still synthesizes the default export.
  jest.doMock('crypto', () => {
    const actual = jest.requireActual('crypto');
    return {
      ...actual,
      randomBytes: (n: number) => {
        const end = byteCursor + n;
        if (end > currentBytes.length) {
          throw new Error(
            `randomBytes mock exhausted: needed ${n} at cursor ${byteCursor} of ${currentBytes.length}`
          );
        }
        const slice = currentBytes.slice(byteCursor, end);
        byteCursor = end;
        return Buffer.from(slice);
      },
    };
  });

  const {
    parseDiceNotation,
    scanDiceNotation,
    formatDiceNotation,
    formatDiceBreakdown,
    rollNotation,
    flipCoin,
    MIN_DIE_SIDES,
    MAX_DIE_SIDES,
    MIN_DICE_COUNT,
    MAX_DICE_COUNT,
    MAX_DICE_MODIFIER,
  } = await import('@/lib/pascal/dice');

  const lines: string[] = [];
  const emit = (row: unknown) => lines.push(JSON.stringify(row));

  // The bounds constants are part of the binding surface — pin them too.
  emit({
    kind: 'bounds',
    minDieSides: MIN_DIE_SIDES,
    maxDieSides: MAX_DIE_SIDES,
    minDiceCount: MIN_DICE_COUNT,
    maxDiceCount: MAX_DICE_COUNT,
    maxDiceModifier: MAX_DICE_MODIFIER,
  });

  for (const [id, input] of PARSE_CASES) {
    emit({ kind: 'parse', id, input, result: parseDiceNotation(input) });
  }

  for (const [id, input] of SCAN_CASES) {
    emit({ kind: 'scan', id, input, results: scanDiceNotation(input) });
  }

  for (const [id, notation] of FORMAT_CASES) {
    emit({ kind: 'format', id, notation, result: formatDiceNotation(notation) });
  }

  for (const [id, notation, bytes] of ROLL_CASES) {
    currentBytes = bytes;
    byteCursor = 0;
    const result = rollNotation(notation);
    const breakdown = formatDiceBreakdown(result);
    if (byteCursor !== bytes.length) {
      throw new Error(`roll case ${id}: consumed ${byteCursor} bytes, committed ${bytes.length}`);
    }
    emit({ kind: 'roll', id, notation, bytes, result, breakdown });
  }

  for (const [id, count, bytes] of COIN_CASES) {
    currentBytes = bytes;
    byteCursor = 0;
    const results = flipCoin(count);
    if (byteCursor !== bytes.length) {
      throw new Error(`coin case ${id}: consumed ${byteCursor} bytes, committed ${bytes.length}`);
    }
    emit({ kind: 'coin', id, count, bytes, results });
  }

  fs.writeFileSync(outPath, lines.join('\n') + '\n');
  process.stderr.write(`dice-notation oracle wrote ${outPath} (${lines.length} rows)\n`);
}

test('dice-notation oracle', async () => {
  await main();
});
