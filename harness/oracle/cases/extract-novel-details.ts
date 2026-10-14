/**
 * Oracle case #24 (Wave 5 / B15): extractNovelDetails.
 *
 * Drives the REAL detail extractor:
 *   extractNovelDetails (lib/memory/admission/novel-details.ts)
 * which scans for proper nouns, dates (4 formats), currency, numbers-with-units,
 * CamelCase, and acronyms, deduping case-insensitively and suppressing details
 * already present in existing content. The corpus distinguishes the regex
 * fidelity choices (ASCII \d/\b, JS \s including U+00A0, sentence-initial skip,
 * stop-word filtering, punctuation stripping, length>1, cross-category order).
 *
 * Re-aimed at P4.D289 (pin `83334b9a5`): v4 `8af764514` (memory refactor
 * Phase C) MOVED the function, byte-identical, from `lib/memory/memory-gate.ts`
 * (deleted) to `lib/memory/admission/novel-details.ts`.
 *
 * Run from inside a v4 checkout (or the round's pin):
 *   N=~/.nvm/versions/node/v24.13.1/bin ; V5W=${V5W:-$HOME/source/quilltap-v5}
 *   cd ~/source/quilltap-server
 *   TZ=UTC $N/npx tsx "$V5W/harness/oracle/cases/extract-novel-details.ts" > /tmp/oracle-extract-novel-details.ndjson
 */

import { extractNovelDetails } from '@/lib/memory/admission/novel-details';

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
