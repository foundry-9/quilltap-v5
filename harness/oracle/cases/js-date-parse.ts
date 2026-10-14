/**
 * Tier-1 ORACLE for `Date.parse` — the V8 13.6.233.17 parser inside Node
 * 24.13.1, the runtime v4 runs on — against v5's whole-parser port
 * (`quilltap_core::v8_date_parse`, behind `episodic::js_date_parse_ms`;
 * P4.D289, the human's 2026-10-14 ruling on the `JS_DATE_PARSE_ISO_SUBSET`
 * finding).
 *
 * The corpus is a hand-picked set (ES5 ISO incl. the expanded year and the
 * 24:00 rule; every legacy shape — month names, weekday words, am/pm, the
 * named zones, `GMT±hh[mm]`, two-digit years, parentheses, `::`, the Smi
 * bound on a zone total; the units V8 treats specially — NUL, NBSP, LS/PS,
 * U+FEFF, U+180E, full-width digits, a surrogate pair) plus a SEEDED fuzz of
 * token soup, so the comparison reaches far past what anyone would write by
 * hand. Each row is `{ s, out }`, `out` the epoch ms or `"NaN"`.
 *
 * MUST run under `TZ=UTC`: V8 reads a string without a UTC offset in the
 * process's local zone; v5 reads it as UTC (`ZONELESS_PARSE_AS_UTC`).
 *
 * Run (from a v4 checkout or the round's pin — the case imports nothing from v4):
 *   N=~/.nvm/versions/node/v24.13.1/bin ; V5W=${V5W:-$HOME/source/quilltap-v5}
 *   cd ~/source/quilltap-server
 *   TZ=UTC $N/npx tsx "$V5W/harness/oracle/cases/js-date-parse.ts" > /tmp/oracle-js-date-parse.ndjson
 */

if (Intl.DateTimeFormat().resolvedOptions().timeZone !== 'UTC') {
  throw new Error('js-date-parse.ts must run under TZ=UTC (see the header)');
}

const HAND: string[] = [
  // ES5 ISO
  '2026', '2026-07', '2026-07-14', '2026-07-14T09:30', '2026-07-14T09:30:00', '2026-07-14T09:30:00.5Z',
  '2026-07-14T09:30:00.123456789Z', '2026-07-14T09:30:00.1234567891Z', '2026-07-14T09:30Z', '2026-07-14T09:30+05:30',
  '2026-07-14T09:30-0530', '2026-07-14T09:30+24:00', '2026-07-14T24:00Z', '2026-07-14T24:00:00.000Z',
  '2026-07-14T24:01Z', '2026-07-14T24:00:01Z', '2026-07-14T24:00:00.001Z', '2026-07-14t09:30z', '2026-07-14T9:30',
  '2026-07-14T23:60Z', '2026-02-31', '2026-13-01', '2026-00-10', '2026-07-00', '2026-07-32', '+010000-01-01T00:00:00Z',
  '-000001-01-01T00:00:00Z', '-000000-01-01T00:00:00Z', '+275760-09-13T00:00:00Z', '+275760-09-13T00:00:00.001Z',
  '-271821-04-20T00:00:00Z', '-271821-04-19T23:59:59.999Z', '0000-01-01', '0099-01-01', '9999-12-31T23:59:59.999Z',
  '2026-07-14T09:30:00.000+01:00', '2026-07-14 09:30', '2026-07-14 12:00', '2026-07-14 12:00Z', '2026-07-14T12:00 Z',
  // legacy
  'July 14, 2026', 'july 14 2026', 'JULY 14 2026', '14 July 2026', 'Jul 14', 'July 14', '14 jul', 'Tue Jul 14 2026',
  'Tue, 14 Jul 2026 09:30:00 GMT', 'Tue Jul 14 2026 09:30:00 GMT+0200 (Central European Summer Time)',
  '2026/07/14', '07/14/2026', '7/14/26', '7/14/49', '7/14/50', '7/14/99', '7/14/100', '14/7/2026', '2026-7-14', '2026-7-4',
  '2026.07.14', 'Sept 3 2026', 'Septembre 3 2026', 'Mayday 3 2026', 'Ma 3 2026', 'Junk 3 2026', 'June 31 2026',
  '3 2026 Oct', '2026 Oct 3', 'Oct-3-2026', '3-Oct-2026', '10:00 July 14 2026', 'July 14 2026 10:00 PM', 'July 14 2026 12:00 am',
  'July 14 2026 12:00 pm', 'July 14 2026 13:00 pm', 'July 14 2026 10 pm', 'July 14 2026 10:30:15.25', 'July 14 2026 10::',
  'July 14 2026 10:30:15.', 'July 14 2026 10:30 EST', 'July 14 2026 10:30 edt', 'July 14 2026 10:30 PST', 'July 14 2026 10:30 UT',
  'July 14 2026 10:30 UTC', 'July 14 2026 10:30 Z', 'July 14 2026 10:30 GMT-8', 'July 14 2026 10:30 GMT-0800', 'July 14 2026 10:30 GMT+5:30',
  'July 14 2026 10:30 GMT-12345', 'July 14 2026 10:30 +0100', 'July 14 2026 10:30 -01:00', 'July 14 2026 10:30 GMT+',
  'Jan 1 2026 10:00 GMT+596523:', 'Jan 1 2026 10:00 GMT+596524:', 'Jan 1 2026 10:00 GMT+1193047:', 'Jan 1 2026 10:00 GMT+999999999:',
  'July 14 2026 (a comment) 10:30', 'July 14 2026 (nested (deep)) 10:30', 'July 14 2026 (unclosed', 'July 14 2026)', '(x) July 14 2026',
  'July 14 2026 +', 'garbage', 'garbage 14', 'garbage14 July 2026', 'on July 14 2026', 'yesterday', 'tomorrow', 'now', '', ' ', '   July 14 2026  ',
  'July 14 2026', 'July​14 2026', 'July  14 2026', 'July 14  2026', '﻿July 14 2026', 'July᠎14 2026',
  'July 14\u0000 2026', '\u0000July 14 2026', '２０２６-07-14', '٢٠٢٦-07-14', '2026-07-14😀', '😀 July 14 2026',
  '1 2 3', '1 2 3 4', '12:00', '12:00 1/2', '1/2 12:00:00:00', '1:2:3:4:5', '99', '1 1 1', '0 0 0', '32 1 2026', '1 32 2026',
  'T10:00', 'tue', 'Z', '2026 Z', '2026-07-14 Z', '2026-07-14 utc', '2026-07-14 utc+1', '2026-07-14 10:00 utc+1', '10:00 2026-07-14',
  '2026-07-14T10:00:00.000Zjunk', '2026-07-14junk', '2026-07-14 junk', 'July 14 2026 junk', 'junk July 14 2026', 'am 10:00 July 14 2026',
  '20261-07-14', '+2026-07-14', '-2026-07-14', '+002026-07-14', '-002026-07-14', '+0020261-07-14', '2026-07-14T10:00:00,5Z',
];

// A seeded PRNG (mulberry32) so the fuzz corpus is identical on every run.
function mulberry32(seed: number): () => number {
  let a = seed >>> 0;
  return () => {
    a = (a + 0x6d2b79f5) >>> 0;
    let t = a;
    t = Math.imul(t ^ (t >>> 15), t | 1);
    t ^= t + Math.imul(t ^ (t >>> 7), t | 61);
    return ((t ^ (t >>> 14)) >>> 0) / 4294967296;
  };
}
const rand = mulberry32(0x5eed2899);
const pick = <T>(xs: readonly T[]): T => xs[Math.floor(rand() * xs.length)];
const ATOMS = [
  '2026', '1999', '07', '7', '14', '31', '32', '00', '0', '1', '12', '13', '24', '59', '60', '99', '100', '123', '1234', '12345', '000123',
  '-', '+', ':', '::', '.', '/', ',', ' ', '  ', '\t', ' ', '(', ')', '(x)', 'T', 't', 'Z', 'z', 'UTC', 'GMT', 'UT', 'EST', 'PDT', 'cst',
  'am', 'PM', 'Jan', 'july', 'Sept', 'December', 'mayo', 'Tue', 'Wednesday', 'of', 'the', 'ago', 'x', 'é', '２', ' ', '﻿', '+010000',
];
const FUZZ: string[] = [];
for (let i = 0; i < 20000; i++) {
  const n = 1 + Math.floor(rand() * 9);
  let s = '';
  for (let j = 0; j < n; j++) s += pick(ATOMS);
  FUZZ.push(s);
}

const num = (x: number): number | string => (Number.isFinite(x) ? x : String(x));
for (const s of HAND) process.stdout.write(JSON.stringify({ kind: 'hand', s, out: num(Date.parse(s)) }) + '\n');
for (const s of FUZZ) process.stdout.write(JSON.stringify({ kind: 'fuzz', s, out: num(Date.parse(s)) }) + '\n');
