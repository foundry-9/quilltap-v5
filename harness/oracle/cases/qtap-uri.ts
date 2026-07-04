/**
 * Oracle case (W4.1d batch 3a): the `qtap://` URI codec.
 *
 * Drives the REAL exported functions from lib/doc-edit/qtap-uri.ts:
 *   isQtapUri / parseQtapUri / formatQtapUri / qtapUriToResolverInput /
 *   formatDocStoreUri / formatScopedUri / formatSelfUri
 * over a committed corpus, emitting parse parts (or the thrown QtapUriError code),
 * the re-emitted canonical URI, the resolver triple, and the producer strings for
 * a field-by-field tier-1 diff.
 *
 * Corpus mined from lib/doc-edit/__tests__/qtap-uri.test.ts plus adversarial rows
 * (percent-decode errors, encoded slash, non-ASCII, empty authority, bad levels).
 *
 * Run from inside the server checkout:
 *   cd ~/source/quilltap-server
 *   LOG_LEVEL=error npx tsx ~/source/quilltap-v5/harness/oracle/cases/qtap-uri.ts \
 *     > /tmp/oracle-qtap-uri.ndjson
 */

import {
  isQtapUri,
  parseQtapUri,
  formatQtapUri,
  qtapUriToResolverInput,
  formatDocStoreUri,
  formatScopedUri,
  formatSelfUri,
  QtapUriError,
  type QtapUriParts,
} from '@/lib/doc-edit/qtap-uri';

const rows: unknown[] = [];

// --- parse cases (each also round-trips through format + resolver on success) ---
const parseInputs: Array<[string, string]> = [
  ['self-mail', 'qtap://self/Mail/1781578632981-from-friday.md'],
  ['encoded-name-colon', 'qtap://Project%20Files%3A%20Voyages%20of%20the%20Covenant/Knowledge/rank_markings.md'],
  ['literal-colon-authority', 'qtap://Project%20Files:%20Voyages%20of%20the%20Covenant/Knowledge/rank_markings.md'],
  ['uuid-authority', 'qtap://550e8400-e29b-41d4-a716-446655440000/notes/today.md'],
  ['project-scope', 'qtap://project/Outline.md'],
  ['general-scope', 'qtap://general/Scenarios/intro.md'],
  ['fragment-heading-level', 'qtap://self/Backstory.md#Childhood:2'],
  ['store-root-slash', 'qtap://self/'],
  ['store-root-no-slash', 'qtap://self'],
  ['reserved-SELF', 'qtap://SELF/x.md'],
  ['reserved-Project', 'qtap://Project/x.md'],
  ['reserved-GENERAL', 'qtap://GENERAL/x.md'],
  ['literal-self-store-by-uuid', 'qtap://11111111-2222-3333-4444-555555555555/x.md'],
  ['mixed-scheme', 'QTAP://self/x.md'],
  ['encoded-heading-level', 'qtap://self/Backstory.md#Heading%20Text:3'],
  ['heading-no-level', 'qtap://self/Backstory.md#Childhood'],
  ['heading-encoded-colon', 'qtap://self/Backstory.md#Chapter%3A%20One'],
  ['query-two-keys', 'qtap://self/x.md?foo=bar&baz=qux%20quux'],
  ['query-single', 'qtap://self/x.md?only=value'],
  ['encoded-slash-segment', 'qtap://self/a%2Fb/c.md'],
  ['nonascii-cafe', 'qtap://Caf%C3%A9%3A%20Notes%20%26%20More/Chapter%20%231/draft%3F.md'],
  ['deep-path', 'qtap://general/a/b/c/d.md'],
  ['level-1', 'qtap://self/x.md#H:1'],
  ['level-6', 'qtap://self/x.md#H:6'],
  // Error cases.
  ['err-not-qtap', 'https://example.com'],
  ['err-not-qtap-rel', 'self/x.md'],
  ['err-empty-authority', 'qtap:///foo'],
  ['err-bad-level-0', 'qtap://self/Backstory.md#X:0'],
  ['err-bad-level-7', 'qtap://self/Backstory.md#X:7'],
  ['err-bad-level-nan', 'qtap://self/Backstory.md#X:x'],
  ['err-malformed-pct-stray', 'qtap://self/a%2'],
  ['err-malformed-pct-zz', 'qtap://self/a%zzb'],
  ['err-malformed-authority', 'qtap://%E0%A4%A/x.md'],
];

for (const [id, input] of parseInputs) {
  const isUri = isQtapUri(input);
  let parse: unknown;
  try {
    const parts = parseQtapUri(input);
    parse = {
      ok: true,
      parts,
      format: formatQtapUri(parts),
      resolver: qtapUriToResolverInput(parts),
    };
  } catch (e) {
    if (e instanceof QtapUriError) {
      parse = { ok: false, code: e.code, message: e.message };
    } else {
      parse = { ok: false, code: 'UNKNOWN', message: String(e) };
    }
  }
  rows.push({ kind: 'parse', id, input, isQtapUri: isUri, parse });
}

// --- format-from-parts cases (canonical emission, incl. query + heading) ---
const formatCases: Array<[string, QtapUriParts]> = [
  ['fmt-colon-name', { scope: 'document_store', mountPoint: 'Project Files: Voyages of the Covenant', path: 'Knowledge/rank_markings.md' }],
  ['fmt-self', { scope: 'document_store', mountPoint: 'self', path: 'x.md' }],
  ['fmt-project', { scope: 'project', path: 'Outline.md' }],
  ['fmt-general', { scope: 'general', path: 'a/b.md' }],
  ['fmt-root', { scope: 'document_store', mountPoint: 'self', path: '' }],
  ['fmt-heading-level', { scope: 'document_store', mountPoint: 'self', path: 'Backstory.md', heading: 'Heading Text', level: 3 }],
  ['fmt-heading-no-level', { scope: 'document_store', mountPoint: 'self', path: 'a.md', heading: 'Intro' }],
  ['fmt-empty-heading', { scope: 'document_store', mountPoint: 'self', path: 'a.md', heading: '' }],
  ['fmt-query', { scope: 'document_store', mountPoint: 'self', path: 'x.md', query: { foo: 'bar baz' } }],
  ['fmt-nonascii', { scope: 'document_store', mountPoint: 'Café: Notes & More', path: 'Chapter #1/draft?.md' }],
];
for (const [id, parts] of formatCases) {
  rows.push({ kind: 'format', id, input: parts, format: formatQtapUri(parts) });
}

// --- producer cases ---
interface DocStoreArgs {
  mountPointName: string;
  mountPointId: string;
  path: string;
  nameIsAmbiguous?: boolean;
  heading?: string;
  level?: number;
}
const docStoreCases: Array<[string, DocStoreArgs]> = [
  ['ds-name', { mountPointName: 'My Store', mountPointId: 'uuid-1', path: 'a.md' }],
  ['ds-ambiguous', { mountPointName: 'My Store', mountPointId: 'uuid-1', path: 'a.md', nameIsAmbiguous: true }],
  ['ds-reserved-self', { mountPointName: 'self', mountPointId: 'uuid-c', path: 'a.md' }],
  ['ds-reserved-Project', { mountPointName: 'Project', mountPointId: 'uuid-c', path: 'a.md' }],
  ['ds-reserved-GENERAL', { mountPointName: 'GENERAL', mountPointId: 'uuid-c', path: 'a.md' }],
  ['ds-heading', { mountPointName: 'My Store', mountPointId: 'uuid-1', path: 'a.md', heading: 'Sec: 1', level: 2 }],
  ['ds-empty-name', { mountPointName: '', mountPointId: 'uuid-e', path: 'a.md', nameIsAmbiguous: true }],
];
for (const [id, args] of docStoreCases) {
  rows.push({ kind: 'producer', id, result: formatDocStoreUri(args) });
}
rows.push({ kind: 'producer', id: 'scoped-project', result: formatScopedUri('project', 'Outline.md') });
rows.push({ kind: 'producer', id: 'scoped-general', result: formatScopedUri('general', 'a/b.md') });
rows.push({ kind: 'producer', id: 'self-plain', result: formatSelfUri('Mail/x.md') });
rows.push({ kind: 'producer', id: 'self-heading', result: formatSelfUri('Backstory.md', { heading: 'Childhood', level: 2 }) });

for (const r of rows) process.stdout.write(JSON.stringify(r) + '\n');
