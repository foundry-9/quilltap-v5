/**
 * Oracle case (W4.1a sub-unit 1): the RNG pattern detector.
 *
 * Drives the REAL exported functions:
 *   detectRngPatterns / detectAndConvertRngPatterns
 *   (lib/services/chat-message/rng-pattern-detector.service.ts)
 * over a committed corpus, emitting BOTH the detected patterns and the converted
 * tool calls for a field-by-field tier-1 diff. Exercises: plain/counted dice,
 * multiple patterns, bounds rejections (d1/d1001/101d6/overflow), the
 * "flip the coin" non-match quirk, coin/bottle word boundaries, spin-bottle at
 * 0/50/51 chars, mixed case, ASCII-`\b` non-ASCII adjacency, the JS-`.`
 * line-terminator exclusion, a ReDoS adversarial string, and the empty string.
 *
 * Run from inside the server checkout:
 *   cd ~/source/quilltap-server
 *   npx tsx ~/source/quilltap-v5/harness/oracle/cases/rng-patterns.ts \
 *     > /tmp/oracle-rng-patterns.ndjson
 */

import {
  detectRngPatterns,
  detectAndConvertRngPatterns,
} from '@/lib/services/chat-message/rng-pattern-detector.service';

const cases: Array<[string, string]> = [
  // Plain dice.
  ['plain-d6', 'roll a d6'],
  ['plain-d20', 'attack with d20'],
  ['plain-d100', 'percentile d100'],
  ['plain-d1000', 'the great d1000'],
  // Counted dice.
  ['counted-2d6', 'deal 2d6 damage'],
  ['counted-3d10', 'cast 3d10'],
  ['counted-10d4', '10d4 shards'],
  ['counted-100d1000', 'the max 100d1000'],
  ['leading-zeros', 'roll 007d06 please'],
  // Multiple in one message.
  ['two-dice', 'roll d6 and 2d20'],
  ['three-dice', 'd4 d6 d8'],
  ['dice-then-plus', '3d6+2 total'],
  // Bounds rejections.
  ['reject-d1', 'a lonely d1'],
  ['reject-d0', 'the void d0'],
  ['reject-d1001', 'too many d1001'],
  ['reject-101d6', 'greedy 101d6'],
  ['reject-0d6', 'zero 0d6'],
  ['reject-1d1', 'edge 1d1'],
  ['reject-count-overflow', 'huge 99999999999999999999d6'],
  ['reject-sides-overflow', 'huge d99999999999999999999'],
  ['boundary-2d1000', 'ok 2d1000'],
  // Dice word boundaries.
  ['dice-trailing-underscore', 'token d6_extra'],
  ['dice-trailing-letter', 'the word cardd6x'],
  ['dice-uppercase', 'roll a D6'],
  ['dice-uppercase-counted', 'roll 2D20'],
  // Coin.
  ['coin-a', 'flip a coin'],
  ['coin-bare', 'flip coin'],
  ['coin-the-nomatch', 'flip the coin'],
  ['coin-uppercase', 'FLIP A COIN'],
  ['coin-mixed', 'Flip A Coin'],
  ['coin-1char', 'flipXcoin now'],
  ['coin-3char', 'flip123coin'],
  ['coin-4char-nomatch', 'flip1234coin'],
  ['coin-trailing-s-nomatch', 'flip a coins'],
  ['coin-trailing-period', 'flip a coin.'],
  ['coin-two', 'flip a coin and flip a coin'],
  ['coin-newline-nomatch', 'flip a\ncoin'],
  // Bottle.
  ['bottle-the', 'spin the bottle'],
  ['bottle-a', 'spin a bottle'],
  ['bottle-adjacent', 'spin bottle'],
  ['bottle-glued-nomatch', 'spinbottle'],
  ['bottle-uppercase', 'SPIN THE BOTTLE'],
  // Length bound: the between-span is `.{0,50}`. A leading/trailing space keeps
  // the `\bspin\b`/`\bbottle\b` boundaries; the middle fill sets the span length.
  ['bottle-50-chars', 'spin ' + 'a'.repeat(48) + ' bottle'], // span 50 → match
  ['bottle-51-chars-nomatch', 'spin ' + 'a'.repeat(49) + ' bottle'], // span 51 → no match
  ['bottle-newline-nomatch', 'spin the\nbottle'],
  ['bottle-redos', 'spin ' + 'a'.repeat(1000) + ' bottle'], // bounded, no catastrophic backtracking
  // Non-ASCII adjacency (ASCII \b quirk — derive truth from v4).
  ['nonascii-before-d6', 'éd6'],
  ['nonascii-around-coin', 'flipécoin'],
  ['nonascii-name-dice', 'José rolls 2d6'],
  // Mixed all three (order: dice, coin, bottle).
  ['mixed-all', 'flip a coin, spin the bottle, roll 2d6'],
  ['mixed-multi', 'd20 then flip a coin then spin the bottle then d4'],
  // Empty / no match.
  ['empty', ''],
  ['no-match', 'just a normal sentence'],
  ['word-with-d', 'the word android has no dice'],
];

const rows: unknown[] = [];
for (const [id, content] of cases) {
  rows.push({
    kind: 'rng',
    id,
    content,
    patterns: detectRngPatterns(content),
    toolCalls: detectAndConvertRngPatterns(content),
  });
}

for (const r of rows) process.stdout.write(JSON.stringify(r) + '\n');
