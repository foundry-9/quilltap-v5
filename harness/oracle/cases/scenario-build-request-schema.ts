/**
 * Oracle case: `scenarioBuildRequestSchema` — the body of `POST
 * /api/v1/scenario-builder?action=build` (v4 `d1c06cd9d`,
 * `lib/scenario-builder/request-schema.ts`).
 *
 * Drives v4's REAL schema with `safeParse` over a corpus of raw JSON bodies and
 * records, per row, either the parsed OUTPUT (`data`, key order as Zod builds
 * it — the trims and the two defaults applied) or the issue list (`issues`,
 * each object in Zod's own per-code key order). v5's twin is
 * `quilltap_core::services::scenario_builder::request_schema`; the Rust side
 * diffs both byte-for-byte after `JSON.stringify` / `serde_json` rendering.
 *
 * Every field is asked: absent / explicit null / wrong type / the trim and
 * boundary lengths ±1 / the refine's four quadrants / non-object bodies /
 * unknown keys (stripped). Bodies are JSON only — the dispatch verb carries a
 * `serde_json::Value`, so `undefined` never reaches the v5 twin as anything but
 * an ABSENT key, and the corpus asks exactly that.
 *
 * ⚠ PIN REQUIRED at the TARGET `d1c06cd9d`: the module does not exist at the
 * `00c290c9a` baseline, so a baseline-pinned run fails to IMPORT — that failure
 * is the pin verification.
 *
 *   V5W=${V5W:-$HOME/source/quilltap-v5}
 *   N=~/.nvm/versions/node/v24.13.1/bin
 *   rm -f /tmp/oracle-scenario-build-request-schema.ndjson
 *   cd ~/source/quilltap-server
 *   $N/npx tsx $V5W/harness/oracle/cases/scenario-build-request-schema.ts \
 *     > /tmp/oracle-scenario-build-request-schema.ndjson
 */

import { scenarioBuildRequestSchema } from '@/lib/scenario-builder/request-schema';

const P = '11111111-1111-4111-8111-111111111111';
const PROJ = '22222222-2222-4222-8222-222222222222';
const C1 = '33333333-3333-4333-8333-333333333333';
const C2 = '44444444-4444-4444-8444-444444444444';
const CHAT = '55555555-5555-4555-8555-555555555555';

/** The smallest body that parses. */
const base = (): Record<string, unknown> => ({
  mode: 'in-world',
  location: 'The Lantern',
  time: 'dusk',
  connectionProfileId: P,
});
const withKey = (k: string, v: unknown) => ({ ...base(), [k]: v });
const without = (k: string) => {
  const b = base();
  delete b[k];
  return b;
};

const rows: Array<[string, unknown]> = [];
const add = (id: string, body: unknown) => rows.push([id, body]);

// --- the whole-body shapes -------------------------------------------------
add('minimal', base());
add('full', {
  mode: 'real',
  location: '  Chicago, the Loop  ',
  time: ' 1927, a wet November night ',
  details: '  jazz, rain  ',
  connectionProfileId: P,
  projectId: PROJ,
  characterIds: [C1, C2],
  chatId: CHAT,
  priorDraft: '  The rain.  ',
  revision: '  Shorter.  ',
});
add('empty-object', {});
add('null-body', null);
add('array-body', []);
add('string-body', 'build');
add('number-body', 42);
add('unknown-keys-stripped', { ...base(), extra: 1, userId: 'x', nested: { a: 1 } });

// --- mode --------------------------------------------------------------------
add('mode-absent', without('mode'));
add('mode-null', withKey('mode', null));
add('mode-bad-value', withKey('mode', 'fiction'));
add('mode-wrong-type', withKey('mode', 1));
add('mode-real', withKey('mode', 'real'));

// --- location: trim, min(1), max(500) ----------------------------------------
add('location-absent', without('location'));
add('location-null', withKey('location', null));
add('location-wrong-type', withKey('location', 7));
add('location-empty', withKey('location', ''));
add('location-blank', withKey('location', '   '));
add('location-500', withKey('location', 'a'.repeat(500)));
add('location-501', withKey('location', 'a'.repeat(501)));
add('location-500-padded', withKey('location', `  ${'a'.repeat(500)}  `));
// JS `trim()` is not Rust's: it strips U+FEFF and keeps U+0085. Zod >= 4.5
// measures a string in CODE POINTS once the UTF-16 count overflows, so 251
// astral characters (502 units) PASS `.max(500)` and 501 of them do not.
add('location-bom-only', withKey('location', '\uFEFF'));
add('location-nel-kept', withKey('location', '\u0085'));
add('location-nbsp-only', withKey('location', '\u00A0\u2003'));
add('location-250-astral', withKey('location', '😀'.repeat(250)));
add('location-251-astral', withKey('location', '😀'.repeat(251)));
add('location-501-astral', withKey('location', '😀'.repeat(501)));

// --- time: trim, min(1), max(200) --------------------------------------------
add('time-absent', without('time'));
add('time-blank', withKey('time', '\t\n '));
add('time-200', withKey('time', 'b'.repeat(200)));
add('time-201', withKey('time', 'b'.repeat(201)));
add('time-wrong-type', withKey('time', false));

// --- details: trim, max(4000), default('') -----------------------------------
add('details-absent', base());
add('details-null', withKey('details', null));
add('details-blank', withKey('details', '   '));
add('details-4000', withKey('details', 'c'.repeat(4000)));
add('details-4001', withKey('details', 'c'.repeat(4001)));
add('details-4001-trims-to-4000', withKey('details', ` ${'c'.repeat(4000)}`));
add('details-wrong-type', withKey('details', ['x']));

// --- connectionProfileId: UUIDSchema ------------------------------------------
add('profile-absent', without('connectionProfileId'));
add('profile-null', withKey('connectionProfileId', null));
add('profile-not-uuid', withKey('connectionProfileId', 'not-a-uuid'));
add('profile-empty', withKey('connectionProfileId', ''));
add('profile-wrong-type', withKey('connectionProfileId', 12));
add('profile-nil-uuid', withKey('connectionProfileId', '00000000-0000-0000-0000-000000000000'));
add('profile-uppercase', withKey('connectionProfileId', P.toUpperCase()));

// --- projectId: UUIDSchema.nullish() ------------------------------------------
add('project-null', withKey('projectId', null));
add('project-uuid', withKey('projectId', PROJ));
add('project-bad', withKey('projectId', 'proj'));
add('project-wrong-type', withKey('projectId', {}));

// --- characterIds: z.array(UUIDSchema).max(32).default([]) --------------------
add('cast-null', withKey('characterIds', null));
add('cast-string', withKey('characterIds', 'x'));
add('cast-empty', withKey('characterIds', []));
add('cast-bad-entry', withKey('characterIds', [C1, 'nope', 5]));
add('cast-duplicates-kept', withKey('characterIds', [C1, C1]));
add('cast-32', withKey('characterIds', Array.from({ length: 32 }, () => C1)));
add('cast-33', withKey('characterIds', Array.from({ length: 33 }, () => C1)));
add('cast-33-with-bad', withKey('characterIds', [...Array.from({ length: 32 }, () => C1), 'bad']));

// --- chatId: UUIDSchema.nullish() ---------------------------------------------
add('chat-null', withKey('chatId', null));
add('chat-uuid', withKey('chatId', CHAT));
add('chat-bad', withKey('chatId', 'c'));

// --- priorDraft / revision + the refine ----------------------------------------
add('refine-neither', base());
add('refine-both', { ...base(), priorDraft: 'Draft.', revision: 'More rain.' });
add('refine-draft-only', { ...base(), priorDraft: 'Draft.' });
add('refine-revision-only', { ...base(), revision: 'More rain.' });
add('refine-both-null', { ...base(), priorDraft: null, revision: null });
add('refine-draft-null-revision-set', { ...base(), priorDraft: null, revision: 'x' });
add('refine-empty-strings-travel', { ...base(), priorDraft: '', revision: '   ' });
add('draft-20000', { ...base(), priorDraft: 'd'.repeat(20000), revision: 'r' });
add('draft-20001', { ...base(), priorDraft: 'd'.repeat(20001), revision: 'r' });
add('draft-untrimmed-kept', { ...base(), priorDraft: '  padded draft  ', revision: 'r' });
add('revision-2000', { ...base(), priorDraft: 'd', revision: 'r'.repeat(2000) });
add('revision-2001', { ...base(), priorDraft: 'd', revision: 'r'.repeat(2001) });
add('draft-wrong-type', { ...base(), priorDraft: 3, revision: 'r' });
// A refine does NOT run when a field issue already failed the object (Zod 4
// aborts refinements on a dirty object) — asked, not assumed.
add('refine-draft-only-plus-field-issue', { ...base(), location: '', priorDraft: 'Draft.' });
// Which field issues ABORT the refine (Zod 4: a non-continuable issue skips
// refinements; a continuable one does not) — one row per issue code.
add('refine-fails-plus-invalid-value', { ...base(), mode: 'x', priorDraft: 'Draft.' });
add('refine-fails-plus-invalid-type', { ...base(), location: 5, priorDraft: 'Draft.' });
add('refine-fails-plus-absent-required', { ...without('time'), priorDraft: 'Draft.' });
add('refine-fails-plus-too-big', { ...base(), time: 't'.repeat(201), priorDraft: 'Draft.' });
add('refine-fails-plus-invalid-format', { ...base(), connectionProfileId: 'p', priorDraft: 'Draft.' });
add('refine-fails-plus-element-format', { ...base(), characterIds: ['r'], priorDraft: 'Draft.' });
add('refine-fails-plus-element-type', { ...base(), characterIds: [1], priorDraft: 'Draft.' });
add('refine-fails-plus-array-too-big', {
  ...base(),
  characterIds: Array.from({ length: 33 }, () => C1),
  priorDraft: 'Draft.',
});
add('refine-fails-plus-array-type', { ...base(), characterIds: 'x', priorDraft: 'Draft.' });
add('refine-fails-plus-nullish-format', { ...base(), chatId: 'c', revision: 'r' });
// The refine reads the RAW value of a field that itself failed.
add('refine-draft-wrong-type-revision-absent', { ...base(), priorDraft: 7 });
add('refine-draft-too-big-revision-absent', { ...base(), priorDraft: 'd'.repeat(20001) });
add('refine-revision-too-big-draft-absent', { ...base(), revision: 'r'.repeat(2001) });
add('refine-draft-null-revision-wrong-type', { ...base(), priorDraft: null, revision: 3 });
add('non-object-with-nothing-else', true);

// --- multi-fault: issue ORDER is the schema's key order -----------------------
add('multi-fault', {
  mode: 'x',
  location: '',
  time: 't'.repeat(201),
  details: 9,
  connectionProfileId: 'p',
  projectId: 'q',
  characterIds: ['r'],
  chatId: 's',
  priorDraft: 1,
  revision: 2,
});

for (const [id, body] of rows) {
  const r = scenarioBuildRequestSchema.safeParse(body);
  const out = r.success ? { id, body, data: r.data } : { id, body, issues: r.error.issues };
  process.stdout.write(JSON.stringify(out) + '\n');
}
