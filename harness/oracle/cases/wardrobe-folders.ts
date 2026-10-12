/**
 * Oracle case (P4.D279, the wardrobe keystone — v4 `4a38222e6`): the wardrobe
 * folder rules, `lib/wardrobe/folders.ts`.
 *
 * Drives the REAL `@/lib/wardrobe/folders` exports — `normalizeWardrobeFolderPath`,
 * `normalizeWardrobeFolderPathOrRoot`, `normalizeWardrobeFolderName`,
 * `isSafeWardrobeFolderPath`, `resolveWardrobeFolder`, `isValidWardrobeFolderPath`,
 * `folderSegments`, `folderOfWardrobePath`, `wardrobeFolderStorePath`,
 * `parentFolder`, `folderName`, `joinFolder`, `isSameFolder`, `isUnderFolder`,
 * `rebaseFolder`, `folderCrumb`, the three error classes' messages and
 * `WardrobeFolderPathSchema`'s refine message (from `@/lib/schemas/wardrobe.types`)
 * — no mocks, no reimplementation. A throwing call is emitted as
 * `{ ok: false, name, message }`; a returning one as `{ ok: true, value }`.
 *
 * The corpus is v4's own `__tests__/unit/lib/wardrobe/folders.test.ts`
 * case-for-case, plus the shapes it leaves implicit: resolve over hand-made
 * spellings, a case-only match, a deeper-than-8 join across an existing
 * ancestor, `images` at the first level vs nested, `../x`, backslash paths,
 * whitespace-only input, `folderOfWardrobePath` over the item-picture folder,
 * non-ASCII segments, a segment past the 100-code-unit sanitize cap, and a
 * `rebaseFolder` whose ancestor differs only in case.
 *
 * Run (Node 24, from the v4 checkout):
 *   N=~/.nvm/versions/node/v24.13.1/bin
 *   cd ~/source/quilltap-server
 *   $N/node --import tsx <v5>/harness/oracle/cases/wardrobe-folders.ts \
 *     > /tmp/oracle-wardrobe-folders.ndjson
 */

import {
  folderCrumb,
  folderName,
  folderOfWardrobePath,
  folderSegments,
  isSameFolder,
  isSafeWardrobeFolderPath,
  isUnderFolder,
  isValidWardrobeFolderPath,
  joinFolder,
  normalizeWardrobeFolderName,
  normalizeWardrobeFolderPath,
  normalizeWardrobeFolderPathOrRoot,
  parentFolder,
  rebaseFolder,
  resolveWardrobeFolder,
  WARDROBE_FOLDER_MAX_DEPTH,
  WARDROBE_RESERVED_FOLDER_SEGMENTS,
  WardrobeFolderConflictError,
  WardrobeFolderNotFoundError,
  WardrobeItemNotInLocationError,
  wardrobeFolderStorePath,
} from '@/lib/wardrobe/folders';
import { WardrobeFolderPathSchema } from '@/lib/schemas/wardrobe.types';

const BASELINE = 'p4.d279-wardrobe-folders';

type Outcome = { ok: true; value: string } | { ok: false; name: string; message: string };

function attempt(fn: () => string): Outcome {
  try {
    return { ok: true, value: fn() };
  } catch (e) {
    const err = e as Error;
    return { ok: false, name: err.name, message: err.message };
  }
}

function emit(row: Record<string, unknown>): void {
  process.stdout.write(`${JSON.stringify(row)}\n`);
}

// A `null` on the wire stands for BOTH `null` and `undefined` — v4 reads them
// alike (`input == null`, `requested ?? ''`, `a ?? ''`). The `undefined` cases
// are driven as `undefined` here so the real branch runs, and serialised as
// `null` for the Rust side's `Option`.
type Nullish = string | null | undefined;
const wire = (s: Nullish): string | null => (s === undefined ? null : s);

emit({
  kind: 'meta',
  baseline: BASELINE,
  maxDepth: WARDROBE_FOLDER_MAX_DEPTH,
  reserved: [...WARDROBE_RESERVED_FOLDER_SEGMENTS],
});

// ── the fixed messages ──────────────────────────────────────────────────────
emit({
  kind: 'messages',
  notFound: new WardrobeFolderNotFoundError('Formal/Evening').message,
  notFoundRoot: new WardrobeFolderNotFoundError('').message,
  conflict: new WardrobeFolderConflictError('Formal').message,
  conflictCustom: new WardrobeFolderConflictError('Formal', 'Holds files that are not items').message,
  notInLocation: new WardrobeItemNotInLocationError(['i-1', 'i-2']).message,
  notInLocationOne: new WardrobeItemNotInLocationError(['i-1']).message,
  notInLocationNone: new WardrobeItemNotInLocationError([]).message,
  schema: (() => {
    const r = WardrobeFolderPathSchema.safeParse('images');
    if (r.success) throw new Error('schema accepted "images"');
    return r.error.issues[0].message;
  })(),
  schemaColon: (() => {
    const r = WardrobeFolderPathSchema.safeParse('a:b');
    if (r.success) throw new Error('schema accepted "a:b"');
    return r.error.issues[0].message;
  })(),
});

// ── normalizeWardrobeFolderPath ─────────────────────────────────────────────
const eight = Array.from({ length: 8 }, (_, i) => `L${i}`).join('/');
const normalizeCases: Array<[string, Nullish]> = [
  // folders.test.ts — "reads empty, whitespace and null as the root"
  ['empty', ''],
  ['whitespace', '   '],
  ['lone-slash', '/'],
  ['null', null],
  ['undefined', undefined],
  // "trims segments and drops empty ones and stray slashes"
  ['stray-slashes', ' /Formal// Evening / '],
  ['backslash', 'Formal\\Evening'],
  // "sanitises each segment with the file-name rule"
  ['sanitize-punct', 'Rainy: days?'],
  ['sanitize-cap-150', 'a'.repeat(150)],
  // "refuses . and .."
  ['dotdot-tail', 'Formal/..'],
  ['dot-head', './Formal'],
  // "refuses the reserved images segment at any depth, in any casing"
  ['images-root', 'images'],
  ['images-nested-upper', 'Formal/IMAGES'],
  ['images-of-me', 'Images of me'],
  // "caps nesting at 8 segments"
  ['eight-deep', eight],
  ['nine-deep', `${eight}/L8`],
  // the implicit shapes
  ['images-title-case', 'Images'],
  ['images-nested-lower', 'Formal/images'],
  ['dotdot-x', '../x'],
  ['dot-alone', '.'],
  ['backslash-mixed', 'a\\b/c\\\\d'],
  ['whitespace-tabs', '\t  '],
  ['nbsp-segments', ' Formal  / Evening '],
  ['non-ascii', 'Été/Robe'],
  ['non-ascii-nfd', 'Été/Robe'],
  ['emoji-segment', '🎩 Hats/🧥'],
  ['long-non-ascii-150', 'é'.repeat(150)],
  ['long-astral-60', '🎩'.repeat(60)],
  ['inner-whitespace-run', 'Hats   and    Coats'],
  ['all-punct', ':*?"<>|'],
  ['quotes-and-pipes', 'Formal | "Evening"'],
  ['trailing-dots-kept', 'Formal.../Evening..'],
  ['images-with-ws', ' images '],
  ['images-sanitised-into', 'images?'],
];
for (const [id, input] of normalizeCases) {
  emit({ kind: 'normalize', id, input: wire(input), out: attempt(() => normalizeWardrobeFolderPath(input)) });
}

// ── normalizeWardrobeFolderPathOrRoot ───────────────────────────────────────
const orRootCases: Array<[string, Nullish]> = [
  ['normal', 'Formal/Evening'],
  ['images-nested', 'Formal/images'],
  ['dotdot', '../x'],
  ['nine-deep', `${eight}/L8`],
  ['null', null],
  ['hand-made', 'Hats: Summer'],
];
for (const [id, input] of orRootCases) {
  emit({ kind: 'normalize_or_root', id, input: wire(input), out: normalizeWardrobeFolderPathOrRoot(input) });
}

// ── normalizeWardrobeFolderName ─────────────────────────────────────────────
const nameCases: Array<[string, Nullish]> = [
  ['one-segment', ' Dress '],
  ['two-segments', 'A/B'],
  ['whitespace', '   '],
  ['empty', ''],
  ['null', null],
  ['undefined', undefined],
  ['backslash', 'A\\B'],
  ['images', 'images'],
  ['dotdot', '..'],
  ['sanitised', 'Rainy: days?'],
  ['non-ascii', 'Été'],
];
for (const [id, input] of nameCases) {
  emit({ kind: 'normalize_name', id, input: wire(input), out: attempt(() => normalizeWardrobeFolderName(input)) });
}

// ── isSafeWardrobeFolderPath ────────────────────────────────────────────────
const safeCases: Array<[string, string]> = [
  ['root', ''],
  ['hand-made', 'Hats: Summer'],
  ['images-nested', 'Formal/images'],
  ['climb', '../escape'],
  ['double-slash', 'Formal//Evening'],
  ['leading-slash', '/Formal'],
  ['images-first', 'images/abc'],
  ['backslash', 'A\\B'],
  ['images-upper-first', 'IMAGES/abc'],
  ['images-alone', 'images'],
  ['trailing-slash', 'Formal/'],
  ['untrimmed', 'a/ b'],
  ['dot', 'Formal/.'],
  ['dotdot-mid', 'Formal/../x'],
  ['non-ascii', 'Été/Robe'],
  ['exactly-900', 'a'.repeat(900)],
  ['over-900', 'a'.repeat(901)],
  ['astral-451', '🎩'.repeat(451)],
  ['nbsp-untrimmed', 'Formal '],
];
for (const [id, path] of safeCases) {
  emit({ kind: 'safe', id, path, out: isSafeWardrobeFolderPath(path) });
}

// ── isValidWardrobeFolderPath ───────────────────────────────────────────────
const validCases: Array<[string, string]> = [
  ['root', ''],
  ['normal', 'Formal/Evening'],
  ['leading-slash', '/Formal'],
  ['trailing-slash', 'Formal/'],
  ['images', 'images'],
  ['colon', 'a:b'],
  ['hand-made', 'Hats: Summer'],
  ['eight-deep', eight],
  ['nine-deep', `${eight}/L8`],
  ['non-ascii', 'Été/Robe'],
  ['backslash', 'a\\b'],
  ['inner-run', 'a  b'],
];
for (const [id, path] of validCases) {
  emit({ kind: 'valid', id, path, out: isValidWardrobeFolderPath(path) });
}

// ── resolveWardrobeFolder ───────────────────────────────────────────────────
const EXISTING = ['Formal', 'Formal/Evening', 'Hats: Summer', 'Accessories/images'];
const deep7 = Array.from({ length: 7 }, (_, i) => `L${i}`).join('/');
const resolveCases: Array<[string, readonly string[], Nullish]> = [
  // folders.test.ts
  ['stored-lower', EXISTING, 'formal/evening'],
  ['stored-hand-made', EXISTING, 'hats: summer'],
  ['stored-images-nested', EXISTING, '/accessories/images/'],
  ['ancestor-kept-rest-normalised', EXISTING, 'Hats: Summer/Straw: boaters'],
  ['ancestor-then-images', EXISTING, 'Formal/images'],
  ['wholly-new', EXISTING, ' Rainy: days '],
  ['empty', EXISTING, ''],
  ['images', EXISTING, 'images'],
  ['deep-seven-plus-one', [deep7], `${deep7}/L7`],
  ['deep-seven-plus-two', [deep7], `${deep7}/L7/L8`],
  // the implicit shapes
  ['null', EXISTING, null],
  ['undefined', EXISTING, undefined],
  ['no-existing', [], 'Formal/Evening: Late'],
  ['case-only-match', ['Formal'], 'FORMAL'],
  ['case-only-ancestor', ['Formal'], 'formal/Evening'],
  ['hand-made-existing-plain-request', ['Hats: Summer', 'Formal'], 'Hats_ Summer'],
  ['hand-made-ancestor-deeper', ['Hats: Summer', 'Formal'], 'hats: summer / Straw / Boaters: wide'],
  ['duplicate-keys-later-wins', ['Formal', 'FORMAL'], 'formal'],
  ['duplicate-keys-later-wins-ancestor', ['Formal', 'FORMAL'], 'formal/x'],
  ['deep-eight-stored-plus-one', [eight], `${eight}/L8`],
  ['deep-stored-ancestor-trimmed', [deep7], `${deep7}/ L7 `],
  ['backslash-request', EXISTING, 'formal\\evening'],
  ['backslash-new-rest', EXISTING, 'Formal\\Night: Late'],
  ['dotdot-rest', EXISTING, 'Formal/../x'],
  ['dotdot-request', EXISTING, '../x'],
  ['whitespace-only', EXISTING, ' \t '],
  ['non-ascii-stored', ['Été'], 'été/Robe: soie'],
  ['long-rest-truncated', EXISTING, `Formal/${'b'.repeat(150)}`],
  ['stored-images-nested-deeper', EXISTING, 'accessories/images/x'],
];
for (const [id, existing, requested] of resolveCases) {
  emit({
    kind: 'resolve',
    id,
    existing: [...existing],
    requested: wire(requested),
    out: attempt(() => resolveWardrobeFolder(existing, requested)),
  });
}

// ── folderSegments ──────────────────────────────────────────────────────────
for (const [id, folder] of [
  ['root', ''],
  ['one', 'Formal'],
  ['two', 'Formal/Evening'],
  ['eight', eight],
  ['hand-made', 'Hats: Summer/Straw'],
  ['non-ascii', 'Été/Robe'],
] as Array<[string, string]>) {
  emit({ kind: 'segments', id, folder, out: folderSegments(folder) });
}

// ── folderOfWardrobePath ────────────────────────────────────────────────────
for (const [id, path] of [
  ['root-item', 'Wardrobe/Coat.md'],
  ['two-deep', 'Wardrobe/Formal/Evening/Coat.md'],
  ['lower-prefix', 'wardrobe/Formal/Coat.md'],
  ['outside', 'Notes/Coat.md'],
  ['a-b', 'Wardrobe/A/B/x.md'],
  ['x', 'Wardrobe/x.md'],
  ['lower-a', 'wardrobe/A/x.md'],
  ['picture', 'Wardrobe/images/x/pic.webp'],
  ['bare-root', 'Wardrobe'],
  ['root-slash-only', 'Wardrobe/'],
  ['prefix-only-no-slash', 'WardrobeX/y.md'],
  ['upper-prefix', 'WARDROBE/Été/Robe/x.md'],
  ['no-file', 'Wardrobe/A/B/'],
  ['non-ascii-before-prefix', 'Été/Wardrobe/x.md'],
] as Array<[string, string]>) {
  emit({ kind: 'folder_of', id, path, out: folderOfWardrobePath(path) });
}

// ── wardrobeFolderStorePath / parentFolder / folderName / folderCrumb ────────
for (const [id, folder] of [
  ['root', ''],
  ['one', 'Formal'],
  ['two', 'Formal/Evening'],
  ['a-b', 'A/B'],
  ['hand-made', 'Hats: Summer/Straw'],
  ['non-ascii', 'Été/Robe'],
  ['eight', eight],
] as Array<[string, string]>) {
  emit({
    kind: 'folder_views',
    id,
    folder,
    storePath: wardrobeFolderStorePath(folder),
    parent: parentFolder(folder),
    name: folderName(folder),
    crumb: folderCrumb(folder),
  });
}

// ── joinFolder ──────────────────────────────────────────────────────────────
for (const [id, parts] of [
  ['root-file', ['', 'Coat.md']],
  ['folder-file', ['Formal', 'Coat.md']],
  ['none', []],
  ['all-empty', ['', '']],
  ['three', ['Formal', 'Evening', 'Coat.md']],
  ['empty-middle', ['Formal', '', 'Coat.md']],
  ['one', ['Formal']],
  ['deeper-than-8', [eight, 'L8', 'L9']],
] as Array<[string, string[]]>) {
  emit({ kind: 'join', id, parts, out: joinFolder(...parts) });
}

// ── isSameFolder ────────────────────────────────────────────────────────────
for (const [id, a, b] of [
  ['case', 'Formal', 'formal'],
  ['empty-null', '', null],
  ['parent-child', 'Formal', 'Formal/Evening'],
  ['both-null', null, null],
  ['null-undefined', null, undefined],
  ['undefined-folder', undefined, 'Formal'],
  ['non-ascii-case', 'Été', 'été'],
  ['hand-made', 'Hats: Summer', 'hats: summer'],
] as Array<[string, Nullish, Nullish]>) {
  emit({ kind: 'same', id, a: wire(a), b: wire(b), out: isSameFolder(a, b) });
}

// ── isUnderFolder ───────────────────────────────────────────────────────────
for (const [id, folder, ancestor] of [
  ['child-case', 'Formal/Evening', 'formal'],
  ['self', 'Formal', 'Formal'],
  ['prefix-not-ancestor', 'Formalwear', 'Formal'],
  ['root-ancestor', 'Anything', ''],
  ['root-under-root', '', ''],
  ['root-under-folder', '', 'Formal'],
  ['grandchild', 'Formal/Evening/Gala', 'FORMAL/evening'],
  ['sibling', 'Casual', 'Formal'],
  ['non-ascii-case', 'Été/Robe', 'été'],
] as Array<[string, string, string]>) {
  emit({ kind: 'under', id, folder, ancestor, out: isUnderFolder(folder, ancestor) });
}

// ── rebaseFolder ────────────────────────────────────────────────────────────
for (const [id, folder, from, to] of [
  ['self-to-parent', 'Formal/Evening', 'Formal/Evening', 'Formal'],
  ['child-to-parent', 'Formal/Evening/Gala', 'Formal/Evening', 'Formal'],
  ['to-root', 'Formal', 'Formal', ''],
  ['not-under', 'Casual', 'Formal', ''],
  ['rename', 'Formal/Evening', 'Formal', 'Dress'],
  ['case-different-ancestor', 'Formal/Evening', 'formal', 'Dress'],
  ['case-different-ancestor-upper', 'formal/Evening/Gala', 'FORMAL/EVENING', 'Dress'],
  ['from-root', 'Casual/Beach', '', 'Summer'],
  ['root-from-root', '', '', 'Summer'],
  ['non-ascii', 'Été/Robe', 'été', 'Summer'],
  ['deeper-target', 'Formal', 'Formal', 'A/B/C'],
] as Array<[string, string, string, string]>) {
  emit({ kind: 'rebase', id, folder, from, to, out: rebaseFolder(folder, from, to) });
}
