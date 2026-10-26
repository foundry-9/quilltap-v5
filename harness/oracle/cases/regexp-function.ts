/**
 * Oracle case: the `regexp()` SQL scalar (P4.D331, v4 `e4a60ad9e` + the
 * `a495faa26` reason strip, at the `322a5488e` pin).
 *
 * Drives the REAL code:
 *   compileSqlRegexp, registerRegexpFunction
 *     (lib/database/backends/sqlite/regexp-function.ts)
 * over an in-memory better-sqlite3 connection — the shape of v4's own
 * `log-entries-repository.test.ts:40-66` — so a `match` row's answer is what
 * SQLite hands back from v4's function, and an error row's text is the
 * statement error better-sqlite3 surfaces (the thrown Error's message).
 *
 * Row kinds:
 *   - `compile` {id, pattern, flags, ok, reason?} — `compileSqlRegexp`'s
 *     answer; `reason` is `/<pattern>/<flags>: <V8 message>` for a syntax
 *     error (V8's own prefix stripped), or v4's flags sentence;
 *   - `match` {id, sql, params, out?, error?} — the SAME SQL + bound params
 *     both sides run; `out` is the one column's value (1 / 0 / null), `error`
 *     the statement's error message.
 *
 * Params ride as JSON: a string, `null`, or a number ({"num": n} so the Rust
 * side binds an INTEGER / REAL, not text).
 *
 * Run from inside the pinned server checkout (Node 24):
 *   cd ~/source/quilltap-server
 *   TZ=UTC npx tsx <V5W>/harness/oracle/cases/regexp-function.ts \
 *     > /tmp/oracle-regexp-function.ndjson
 */

import path from 'node:path';
import { createRequire } from 'node:module';
import {
  compileSqlRegexp,
  registerRegexpFunction,
} from '@/lib/database/backends/sqlite/regexp-function';

const require = createRequire(import.meta.url);
const Database = require(path.join(process.cwd(), 'node_modules', 'better-sqlite3'));

type Param = string | null | { num: number };

const COMPILE: Array<[string, string, string]> = [
  // [id, pattern, flags]
  ['ok-alternation', 'lock(pick|smith)', ''],
  ['ok-day', '^Day \\d+', ''],
  ['unclosed', '(unclosed', ''],
  ['open-bracket', '[', ''],
  ['lone-star', '*', ''],
  ['lone-plus', '+', ''],
  ['out-of-order-quantifier', 'a{2,1}', ''],
  ['dup-group-name', '(?<n>a)(?<n>b)', ''],
  ['named-ref-undefined', '\\k<x>', ''],
  ['named-ref-undefined-u', '\\k<x>', 'u'],
  ['named-ref-undefined-with-group', '(?<y>a)\\k<x>', ''],
  ['lookbehind', '(?<=a)b', ''],
  ['lookbehind-u', '(?<=a)b', 'u'],
  ['negative-lookbehind', '(?<!a)b', ''],
  ['property-no-u', '\\p{L}', ''],
  ['property-u', '\\p{L}', 'u'],
  ['property-bad-u', '\\p{Nope}', 'u'],
  ['brace-open', 'x{', ''],
  ['brace-open-u', 'x{', 'u'],
  ['trailing-backslash', '\\', ''],
  ['unmatched-close', ')', ''],
  ['double-star', 'a**', ''],
  ['range-out-of-order', '[z-a]', ''],
  ['bare-c', '\\c', ''],
  ['bare-c-u', '\\c', 'u'],
  ['open-question', '(?', ''],
  ['inline-flag', '(?i)abc', ''],
  ['empty-min-brace', 'a{,5}', ''],
  ['empty-min-brace-u', 'a{,5}', 'u'],
  ['backref-before-group', '\\1(a)', ''],
  ['backref-too-high-u', '(a)\\2', 'u'],
  ['unterminated-group-name', '(?<a', ''],
  ['bad-group-name', '(?<1a>x)', ''],
  ['class-escape-range-u', '[\\d-z]', 'u'],
  ['class-escape-range', '[\\d-z]', ''],
  ['unicode-brace-u', '\\u{1F600}', 'u'],
  ['identity-escape-u', '\\-', 'u'],
  ['identity-escape-letter-u', '\\q', 'u'],
  ['lone-close-brace-u', '}', 'u'],
  ['lone-close-bracket-u', ']', 'u'],
  ['lone-close-brace', '}', ''],
  ['quantified-lookahead-u', '(?=a)*', 'u'],
  ['quantified-lookahead', '(?=a)*', ''],
  ['quantified-lookbehind', '(?<=a)*', ''],
  ['bad-hex-u', '\\x1', 'u'],
  ['octal-u', '\\01', 'u'],
  ['nothing-to-repeat-in-alt', 'a|*', ''],
  ['long-500', 'a'.repeat(500), ''],
  // Flags — v4's own check first, then V8's constructor.
  ['flag-g', 'x', 'g'],
  ['flag-y', 'x', 'y'],
  ['flag-d', 'x', 'd'],
  ['flag-v', 'x', 'v'],
  ['flag-upper-i', 'x', 'I'],
  ['flag-repeated', 'x', 'ii'],
  ['flag-repeated-ms', 'x', 'msm'],
  ['flag-imsu', 'x', 'imsu'],
  ['flag-mi', 'x', 'mi'],
];

/** [id, sql, params] — the SAME statement on both sides. */
const MATCH: Array<[string, string, Param[]]> = [
  ['operator-hit', `SELECT ? REGEXP ? AS m`, ['lockpick', 'lock(pick|smith)']],
  ['operator-miss', `SELECT ? REGEXP ? AS m`, ['padlock', '^lock']],
  ['three-arg-i', `SELECT regexp(?, ?, ?) AS m`, ['^the', 'The vault', 'i']],
  ['three-arg-empty', `SELECT regexp(?, ?, ?) AS m`, ['^the', 'The vault', '']],
  ['null-value', `SELECT regexp(?, ?) AS m`, ['x', null]],
  ['null-pattern', `SELECT regexp(?, ?) AS m`, [null, 'x']],
  ['null-flags', `SELECT regexp(?, ?, ?) AS m`, ['^T', 'The', null]],
  ['number-value', `SELECT regexp(?, ?) AS m`, ['^1', { num: 12 }]],
  ['real-value', `SELECT regexp(?, ?) AS m`, ['^1\\.5$', { num: 1.5 }]],
  ['number-pattern', `SELECT regexp(?, ?) AS m`, [{ num: 12 }, 'a12b']],
  ['bad-pattern', `SELECT regexp(?, ?) AS m`, ['(', 'x']],
  ['bad-flags', `SELECT regexp(?, ?, ?) AS m`, ['x', 'x', 'g']],
  ['repeated-flags', `SELECT regexp(?, ?, ?) AS m`, ['x', 'x', 'ii']],
  ['lookbehind-hit', `SELECT regexp(?, ?) AS m`, ['(?<=a)b', 'ab']],
  ['lookbehind-miss', `SELECT regexp(?, ?) AS m`, ['(?<=a)b', 'cb']],
  ['negative-lookahead', `SELECT regexp(?, ?) AS m`, ['^a(?!b)', 'ab']],
  ['backreference', `SELECT regexp(?, ?) AS m`, ['^(a+)\\1$', 'aaaa']],
  ['named-backreference', `SELECT regexp(?, ?) AS m`, ['^(?<w>\\w+) \\k<w>$', 'lock lock']],
  ['digit-arabic-indic', `SELECT regexp(?, ?) AS m`, ['\\d', '\u0663']],
  ['digit-arabic-indic-u', `SELECT regexp(?, ?, ?) AS m`, ['\\d', '\u0663', 'u']],
  ['word-e-acute', `SELECT regexp(?, ?) AS m`, ['^\\w$', '\u00e9']],
  ['boundary-e-acute', `SELECT regexp(?, ?) AS m`, ['\\b\u00e9', ' \u00e9']],
  ['space-nbsp', `SELECT regexp(?, ?) AS m`, ['^\\s$', '\u00a0']],
  ['space-bom', `SELECT regexp(?, ?) AS m`, ['^\\s$', '\ufeff']],
  ['icase-long-s', `SELECT regexp(?, ?, ?) AS m`, ['^s$', '\u017f', 'i']],
  ['icase-long-s-u', `SELECT regexp(?, ?, ?) AS m`, ['^s$', '\u017f', 'iu']],
  ['icase-kelvin', `SELECT regexp(?, ?, ?) AS m`, ['^k$', '\u212a', 'i']],
  ['icase-kelvin-u', `SELECT regexp(?, ?, ?) AS m`, ['^k$', '\u212a', 'iu']],
  ['icase-sharp-s', `SELECT regexp(?, ?, ?) AS m`, ['^\u00df$', '\u1e9e', 'i']],
  ['icase-sigma', `SELECT regexp(?, ?, ?) AS m`, ['^\u03c3$', '\u03c2', 'i']],
  ['dot-lf', `SELECT regexp(?, ?) AS m`, ['^.$', '\n']],
  ['dot-cr', `SELECT regexp(?, ?) AS m`, ['^.$', '\r']],
  ['dot-ls', `SELECT regexp(?, ?) AS m`, ['^.$', '\u2028']],
  ['dot-ps', `SELECT regexp(?, ?) AS m`, ['^.$', '\u2029']],
  ['dot-lf-s', `SELECT regexp(?, ?, ?) AS m`, ['^.$', '\n', 's']],
  ['dot-ls-s', `SELECT regexp(?, ?, ?) AS m`, ['^.$', '\u2028', 's']],
  ['dot-astral', `SELECT regexp(?, ?) AS m`, ['^.$', '\u{1f600}']],
  ['dot-astral-u', `SELECT regexp(?, ?, ?) AS m`, ['^.$', '\u{1f600}', 'u']],
  ['two-dots-astral', `SELECT regexp(?, ?) AS m`, ['^..$', '\u{1f600}']],
  ['negated-class-astral', `SELECT regexp(?, ?) AS m`, ['^[^a]$', '\u{1f600}']],
  ['negated-class-astral-u', `SELECT regexp(?, ?, ?) AS m`, ['^[^a]$', '\u{1f600}', 'u']],
  ['caret-m-lf', `SELECT regexp(?, ?, ?) AS m`, ['^b', 'a\nb', 'm']],
  ['caret-m-ls', `SELECT regexp(?, ?, ?) AS m`, ['^b', 'a\u2028b', 'm']],
  ['caret-no-m', `SELECT regexp(?, ?) AS m`, ['^b', 'a\nb']],
  ['dollar-m-cr', `SELECT regexp(?, ?, ?) AS m`, ['a$', 'a\rb', 'm']],
  ['unicode-escape-u', `SELECT regexp(?, ?, ?) AS m`, ['^\\u{61}$', 'a', 'u']],
  ['unicode-escape-no-u', `SELECT regexp(?, ?) AS m`, ['^\\u{2}$', 'uu']],
  ['named-group', `SELECT regexp(?, ?) AS m`, ['^(?<y>\\d{4})-', '2026-10-19']],
  ['brace-literal', `SELECT regexp(?, ?) AS m`, ['x{', 'x{']],
  ['close-brace-literal', `SELECT regexp(?, ?) AS m`, ['a}', 'a}']],
  ['legacy-octal', `SELECT regexp(?, ?) AS m`, ['\\101', 'A']],
  ['empty-class', `SELECT regexp(?, ?) AS m`, ['[]', 'a']],
  ['negated-empty-class', `SELECT regexp(?, ?) AS m`, ['^[^]$', '\n']],
  ['property-letter-u', `SELECT regexp(?, ?, ?) AS m`, ['^\\p{L}+$', '\u00e9t\u00e9', 'u']],
  ['property-no-u-literal', `SELECT regexp(?, ?) AS m`, ['^\\p{L}$', 'p{L}']],
  ['lazy-quantifier', `SELECT regexp(?, ?) AS m`, ['^a+?b$', 'aaab']],
  ['astral-literal-no-u', `SELECT regexp(?, ?) AS m`, ['^\u{1f600}$', '\u{1f600}']],
  ['long-500-hit', `SELECT regexp(?, ?) AS m`, ['a'.repeat(500), 'a'.repeat(501)]],
  ['in-where', `SELECT count(*) AS m FROM (SELECT 'Lockpick broke' AS v UNION ALL SELECT 'lock') WHERE regexp(?, v, ?)`, ['lock', 'i']],
];

function bind(params: Param[]): unknown[] {
  return params.map((p) => (p !== null && typeof p === 'object' ? p.num : p));
}

const db = new Database(':memory:');
registerRegexpFunction(db);

for (const [id, pattern, flags] of COMPILE) {
  const r = compileSqlRegexp(pattern, flags);
  const row: Record<string, unknown> = { kind: 'compile', id, pattern, flags, ok: r.ok };
  if (!r.ok) row.reason = r.reason;
  process.stdout.write(JSON.stringify(row) + '\n');
}

for (const [id, sql, params] of MATCH) {
  const row: Record<string, unknown> = { kind: 'match', id, sql, params };
  try {
    const got = db.prepare(sql).get(...bind(params)) as { m: unknown };
    row.out = got.m;
  } catch (error) {
    row.error = error instanceof Error ? error.message : String(error);
  }
  process.stdout.write(JSON.stringify(row) + '\n');
}

db.close();
