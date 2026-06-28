/**
 * Oracle case #24 (Wave 5 / B15): extractNovelDetails.
 *
 * Drives the REAL detail extractor:
 *   extractNovelDetails (lib/memory/memory-gate.ts)
 * which scans for proper nouns, dates (4 formats), currency, numbers-with-units,
 * CamelCase, and acronyms, deduping case-insensitively and suppressing details
 * already present in existing content. The corpus distinguishes the regex
 * fidelity choices (ASCII \d/\b, JS \s including U+00A0, sentence-initial skip,
 * stop-word filtering, punctuation stripping, length>1, cross-category order).
 *
 * Run from inside the server checkout:
 *   cd ~/source/quilltap-server
 *   npx tsx ~/source/quilltap-v5/harness/oracle/cases/extract-novel-details.ts \
 *     > /tmp/oracle-extract-novel-details.ndjson
 */

import { extractNovelDetails } from '@/lib/memory/memory-gate';

const rows: unknown[] = [];

const NBSP = ' ';
const cases: Array<[string, string, string]> = [
  ['proper-nouns-skip-initial', 'I met Alice and Bob today. Charlie waved.', ''],
  ['stop-words-filtered', 'We saw The Great Wall.', ''],
  ['date-iso', 'Born on 2024-01-15 exactly.', ''],
  ['date-slash', 'Due 1/15/2024 or 12/3/99.', ''],
  ['date-month-forms', 'Meeting January 15, 2024 here. Also 3 March 2025.', ''],
  ['currency', 'It cost $1,234.56 and $100 total.', ''],
  ['numbers-units', 'He is 25 years old and ran 5km in 30 minutes, 100% sure.', ''],
  ['camel-and-acronyms', 'The API uses CamelCase and HTTP and NASA.', ''],
  ['existing-suppression', 'Alice and Bob met', 'alice was there'],
  ['dedup-case-insensitive', 'Bob and Bob and BOB', ''],
  ['nbsp-whitespace', `Hello${NBSP}World weighs 70${NBSP}kg`, ''],
  ['length-filter', 'I saw X and Ab.', ''],
  ['punctuation-strip', 'We love (Paris), and "Tokyo"!', ''],
  ['empty', '', ''],
  ['acronym-stopword', 'The NO and the YES votes', ''],
];

for (const [id, candidate, existing] of cases) {
  rows.push({
    kind: 'novel',
    id,
    candidate,
    existing,
    out: extractNovelDetails(candidate, existing),
  });
}

for (const r of rows) process.stdout.write(JSON.stringify(r) + '\n');
