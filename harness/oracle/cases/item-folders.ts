/**
 * Oracle case (P4.D324, the Inventory keystone — v4 `4fbdad186`'s LIFT): the
 * generic item-folder rules `lib/mount-index/item-folders.ts`, driven over
 * BOTH rule sets (the wardrobe's and the Inventory's), plus the REAL
 * `lib/inventory/folders.ts` delegates as a second row set so the subclass
 * messages are pinned with `noun: "inventory"`.
 *
 * Drives the REAL exports — `normalizeItemFolderPath`,
 * `normalizeItemFolderPathOrRoot`, `normalizeItemFolderName`,
 * `isSafeItemFolderPath`, `resolveItemFolder`, `isValidItemFolderPath`,
 * `folderOfItemPath`, `itemFolderStorePath`, `folderUnderItemRoot`,
 * `isItemDocumentPath` (with / without `skipFileName`, `.json` / `.md`), the
 * eight path helpers, the four error classes' messages — no mocks, no
 * reimplementation. A throwing call is emitted as `{ ok: false, name, message
 * }`; a returning one as `{ ok: true, value }`.
 *
 * The probe matrix is `wardrobe-folders.ts`'s, copied (that case pins the
 * wardrobe DELEGATES; this one pins the generic layer under both rules, so a
 * lift that moved a wardrobe byte reddens both).
 *
 * Run (Node 24, from the v4 checkout):
 *   N=~/.nvm/versions/node/v24.13.1/bin
 *   V5W=${V5W:-$HOME/source/quilltap-v5}
 *   cd ~/source/quilltap-server
 *   $N/node --import tsx $V5W/harness/oracle/cases/item-folders.ts \
 *     > /tmp/oracle-item-folders.ndjson
 */

import {
  folderCrumb,
  folderName,
  folderOfItemPath,
  folderSegments,
  folderUnderItemRoot,
  isItemDocumentPath,
  isSafeItemFolderPath,
  isSameFolder,
  isUnderFolder,
  isValidItemFolderPath,
  itemFolderStorePath,
  ItemFolderConflictError,
  ItemFolderNotFoundError,
  ItemNotInLocationError,
  joinFolder,
  normalizeItemFolderName,
  normalizeItemFolderPath,
  normalizeItemFolderPathOrRoot,
  parentFolder,
  rebaseFolder,
  resolveItemFolder,
  type ItemFolderRules,
} from '@/lib/mount-index/item-folders';
import { WARDROBE_FOLDER_RULES } from '@/lib/wardrobe/folders';
import { isWardrobeInstructionsFileName } from '@/lib/mount-index/character-vault';
import {
  INVENTORY_FOLDER,
  INVENTORY_FOLDER_MAX_DEPTH,
  INVENTORY_FOLDER_RULES,
  INVENTORY_RESERVED_FOLDER_SEGMENTS,
  InventoryFolderConflictError,
  InventoryFolderNotFoundError,
  InventoryItemNotInLocationError,
  folderOfInventoryPath,
  inventoryFolderStorePath,
  isInventoryItemDocumentPath,
  isSafeInventoryFolderPath,
  isValidInventoryFolderPath,
  normalizeInventoryFolderName,
  normalizeInventoryFolderPath,
  normalizeInventoryFolderPathOrRoot,
  resolveInventoryFolder,
} from '@/lib/inventory/folders';
import { InventoryFolderPathSchema } from '@/lib/schemas/inventory.types';

const BASELINE = 'p4.d324-item-folders';

type Outcome = { ok: true; value: unknown } | { ok: false; name: string; message: string };

function attempt(fn: () => unknown): Outcome {
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

type Nullish = string | null | undefined;
const wire = (s: Nullish): string | null => (s === undefined ? null : s);

const RULES: Array<[string, ItemFolderRules]> = [
  ['wardrobe', WARDROBE_FOLDER_RULES],
  ['inventory', INVENTORY_FOLDER_RULES],
];

emit({
  kind: 'meta',
  baseline: BASELINE,
  rules: Object.fromEntries(RULES.map(([k, r]) => [k, { root: r.root, reserved: [...r.reserved], maxDepth: r.maxDepth, noun: r.noun }])),
  inventoryFolder: INVENTORY_FOLDER,
  inventoryMaxDepth: INVENTORY_FOLDER_MAX_DEPTH,
  inventoryReserved: [...INVENTORY_RESERVED_FOLDER_SEGMENTS],
});

// ── the error classes ──────────────────────────────────────────────────────
for (const [noun] of [['wardrobe'], ['inventory'], ['item']]) {
  emit({
    kind: 'messages',
    noun,
    notFound: new ItemFolderNotFoundError('Formal/Evening', noun).message,
    notFoundRoot: new ItemFolderNotFoundError('', noun).message,
    conflict: new ItemFolderConflictError('Formal', undefined, noun).message,
    conflictCustom: new ItemFolderConflictError('Formal', 'Holds files that are not items', noun).message,
    notInLocation: new ItemNotInLocationError(['i-1', 'i-2'], noun).message,
    notInLocationNone: new ItemNotInLocationError([], noun).message,
  });
}
emit({
  kind: 'inventory_messages',
  notFound: new InventoryFolderNotFoundError('Satchel/Inner').message,
  conflict: new InventoryFolderConflictError('Satchel').message,
  conflictCustom: new InventoryFolderConflictError('Satchel', 'custom sentence').message,
  notInLocation: new InventoryItemNotInLocationError(['a', 'b']).message,
  schema: (() => {
    const r = InventoryFolderPathSchema.safeParse('images');
    if (r.success) throw new Error('schema accepted "images"');
    return r.error.issues[0].message;
  })(),
});

const eight = Array.from({ length: 8 }, (_, i) => `L${i}`).join('/');
const deep7 = Array.from({ length: 7 }, (_, i) => `L${i}`).join('/');

const normalizeCases: Array<[string, Nullish]> = [
  ['empty', ''], ['whitespace', '   '], ['lone-slash', '/'], ['null', null], ['undefined', undefined],
  ['stray-slashes', ' /Formal// Evening / '], ['backslash', 'Formal\\Evening'],
  ['sanitize-punct', 'Rainy: days?'], ['sanitize-cap-150', 'a'.repeat(150)],
  ['dotdot-tail', 'Formal/..'], ['dot-head', './Formal'], ['images-root', 'images'],
  ['images-nested-upper', 'Formal/IMAGES'], ['images-of-me', 'Images of me'], ['eight-deep', eight],
  ['nine-deep', `${eight}/L8`], ['images-title-case', 'Images'], ['images-nested-lower', 'Formal/images'],
  ['dotdot-x', '../x'], ['dot-alone', '.'], ['backslash-mixed', 'a\\b/c\\\\d'], ['whitespace-tabs', '\t  '],
  ['nbsp-segments', ' Formal  / Evening '], ['non-ascii', 'Été/Robe'],
  ['non-ascii-nfd', 'Été/Robe'], ['emoji-segment', '🎩 Hats/🧥'],
  ['long-non-ascii-150', 'é'.repeat(150)], ['long-astral-60', '🎩'.repeat(60)],
  ['inner-whitespace-run', 'Hats   and    Coats'], ['all-punct', ':*?"<>|'],
  ['quotes-and-pipes', 'Formal | "Evening"'], ['trailing-dots-kept', 'Formal.../Evening..'],
  ['images-with-ws', ' images '], ['images-sanitised-into', 'images?'],
];
const orRootCases: Array<[string, Nullish]> = [
  ['normal', 'Formal/Evening'], ['images-nested', 'Formal/images'], ['dotdot', '../x'],
  ['nine-deep', `${eight}/L8`], ['null', null], ['hand-made', 'Hats: Summer'],
];
const nameCases: Array<[string, Nullish]> = [
  ['one-segment', ' Dress '], ['two-segments', 'A/B'], ['whitespace', '   '], ['empty', ''], ['null', null],
  ['undefined', undefined], ['backslash', 'A\\B'], ['images', 'images'], ['dotdot', '..'],
  ['sanitised', 'Rainy: days?'], ['non-ascii', 'Été'],
];
const safeCases: Array<[string, string]> = [
  ['root', ''], ['hand-made', 'Hats: Summer'], ['images-nested', 'Formal/images'], ['climb', '../escape'],
  ['double-slash', 'Formal//Evening'], ['leading-slash', '/Formal'], ['images-first', 'images/abc'],
  ['backslash', 'A\\B'], ['images-upper-first', 'IMAGES/abc'], ['images-alone', 'images'],
  ['trailing-slash', 'Formal/'], ['untrimmed', 'a/ b'], ['dot', 'Formal/.'], ['dotdot-mid', 'Formal/../x'],
  ['non-ascii', 'Été/Robe'], ['exactly-900', 'a'.repeat(900)], ['over-900', 'a'.repeat(901)],
  ['astral-451', '🎩'.repeat(451)], ['nbsp-untrimmed', 'Formal '],
];
const validCases: Array<[string, string]> = [
  ['root', ''], ['normal', 'Formal/Evening'], ['leading-slash', '/Formal'], ['trailing-slash', 'Formal/'],
  ['images', 'images'], ['colon', 'a:b'], ['hand-made', 'Hats: Summer'], ['eight-deep', eight],
  ['nine-deep', `${eight}/L8`], ['non-ascii', 'Été/Robe'], ['backslash', 'a\\b'], ['inner-run', 'a  b'],
];
const EXISTING = ['Formal', 'Formal/Evening', 'Hats: Summer', 'Accessories/images'];
const resolveCases: Array<[string, readonly string[], Nullish]> = [
  ['stored-lower', EXISTING, 'formal/evening'], ['stored-hand-made', EXISTING, 'hats: summer'],
  ['stored-images-nested', EXISTING, '/accessories/images/'],
  ['ancestor-kept-rest-normalised', EXISTING, 'Hats: Summer/Straw: boaters'],
  ['ancestor-then-images', EXISTING, 'Formal/images'], ['wholly-new', EXISTING, ' Rainy: days '],
  ['empty', EXISTING, ''], ['images', EXISTING, 'images'], ['deep-seven-plus-one', [deep7], `${deep7}/L7`],
  ['deep-seven-plus-two', [deep7], `${deep7}/L7/L8`], ['null', EXISTING, null], ['undefined', EXISTING, undefined],
  ['no-existing', [], 'Formal/Evening: Late'], ['case-only-match', ['Formal'], 'FORMAL'],
  ['case-only-ancestor', ['Formal'], 'formal/Evening'],
  ['duplicate-keys-later-wins', ['Formal', 'FORMAL'], 'formal'],
  ['duplicate-keys-later-wins-ancestor', ['Formal', 'FORMAL'], 'formal/x'],
  ['deep-eight-stored-plus-one', [eight], `${eight}/L8`], ['backslash-request', EXISTING, 'formal\\evening'],
  ['dotdot-rest', EXISTING, 'Formal/../x'], ['dotdot-request', EXISTING, '../x'],
  ['non-ascii-stored', ['Été'], 'été/Robe: soie'], ['long-rest-truncated', EXISTING, `Formal/${'b'.repeat(150)}`],
];
const folderOfCases: Array<[string, string]> = [
  ['root-item-md', 'Wardrobe/Coat.md'], ['two-deep-md', 'Wardrobe/Formal/Evening/Coat.md'],
  ['lower-prefix-md', 'wardrobe/Formal/Coat.md'], ['inv-root', 'Inventory/Sword.json'],
  ['inv-two-deep', 'Inventory/Satchel/Inner/Key.json'], ['inv-lower', 'inventory/Satchel/x.json'],
  ['outside', 'Notes/Coat.md'], ['picture-w', 'Wardrobe/images/x/pic.webp'],
  ['picture-i', 'Inventory/images/x/pic.webp'], ['bare-w', 'Wardrobe'], ['bare-i', 'Inventory'],
  ['slash-only-i', 'Inventory/'], ['prefix-no-slash', 'InventoryX/y.json'], ['upper-i', 'INVENTORY/Été/x.json'],
  ['no-file-i', 'Inventory/A/B/'],
];
const underRootCases: Array<[string, string]> = [
  ['root-w', 'Wardrobe'], ['root-i', 'Inventory'], ['root-slash-i', 'Inventory/'], ['one-i', 'Inventory/Satchel'],
  ['two-i', 'inventory/Satchel/Inner'], ['images-i', 'Inventory/images'], ['images-sub-i', 'Inventory/Images/abc'],
  ['images-nested-i', 'Inventory/Satchel/images'], ['one-w', 'Wardrobe/Formal'], ['outside', 'Notes/x'],
  ['non-ascii', 'Inventory/Été'],
];
const docPathCases: Array<[string, string]> = [
  ['json-root', 'Inventory/Sword.json'], ['json-upper', 'INVENTORY/Sub/SWORD.JSON'],
  ['json-images-file', 'Inventory/images/x/a.json'], ['json-images-root-file', 'Inventory/images.json'],
  ['md-in-inventory', 'Inventory/notes.md'], ['double-slash', 'Inventory//a.json'],
  ['empty-rest', 'Inventory/'], ['ext-only', 'Inventory/.json'], ['outside', 'Notes/a.json'],
  ['md-root', 'Wardrobe/Coat.md'], ['md-images', 'Wardrobe/images/x/a.md'],
  ['md-instructions-root', 'Wardrobe/instructions.md'], ['md-instructions-nested', 'Wardrobe/Formal/INSTRUCTIONS.md'],
  ['md-sub', 'wardrobe/Formal/Coat.MD'], ['trailing-slash', 'Inventory/Sub/'],
];
const viewCases: Array<[string, string]> = [
  ['root', ''], ['one', 'Formal'], ['two', 'Formal/Evening'], ['hand-made', 'Hats: Summer/Straw'],
  ['non-ascii', 'Été/Robe'], ['eight', eight],
];

for (const [rk, rules] of RULES) {
  for (const [id, input] of normalizeCases) {
    emit({ kind: 'normalize', rules: rk, id, input: wire(input), out: attempt(() => normalizeItemFolderPath(rules, input)) });
  }
  for (const [id, input] of orRootCases) {
    emit({ kind: 'normalize_or_root', rules: rk, id, input: wire(input), out: normalizeItemFolderPathOrRoot(rules, input) });
  }
  for (const [id, input] of nameCases) {
    emit({ kind: 'normalize_name', rules: rk, id, input: wire(input), out: attempt(() => normalizeItemFolderName(rules, input)) });
  }
  for (const [id, path] of safeCases) {
    emit({ kind: 'safe', rules: rk, id, path, out: isSafeItemFolderPath(rules, path) });
  }
  for (const [id, path] of validCases) {
    emit({ kind: 'valid', rules: rk, id, path, out: isValidItemFolderPath(rules, path) });
  }
  for (const [id, existing, requested] of resolveCases) {
    emit({
      kind: 'resolve', rules: rk, id, existing: [...existing], requested: wire(requested),
      out: attempt(() => resolveItemFolder(rules, existing, requested)),
    });
  }
  for (const [id, path] of folderOfCases) {
    emit({ kind: 'folder_of', rules: rk, id, path, out: folderOfItemPath(rules, path) });
  }
  for (const [id, folder] of viewCases) {
    emit({ kind: 'store_path', rules: rk, id, folder, out: itemFolderStorePath(rules, folder) });
  }
  for (const [id, path] of underRootCases) {
    emit({ kind: 'under_root', rules: rk, id, path, out: folderUnderItemRoot(rules, path) });
  }
  for (const [id, path] of docPathCases) {
    for (const ext of ['.json', '.md']) {
      emit({ kind: 'doc_path', rules: rk, id, path, ext, skip: false, out: isItemDocumentPath(rules, path, ext) });
      emit({
        kind: 'doc_path', rules: rk, id, path, ext, skip: true,
        out: isItemDocumentPath(rules, path, ext, isWardrobeInstructionsFileName),
      });
    }
  }
}

// ── the rule-free helpers ───────────────────────────────────────────────────
for (const [id, folder] of viewCases) {
  emit({
    kind: 'views', id, folder, segments: folderSegments(folder), parent: parentFolder(folder),
    name: folderName(folder), crumb: folderCrumb(folder),
  });
}
for (const [id, parts] of [
  ['root-file', ['', 'Coat.md']], ['folder-file', ['Formal', 'Coat.md']], ['none', []], ['all-empty', ['', '']],
  ['three', ['Formal', 'Evening', 'Coat.md']], ['empty-middle', ['Formal', '', 'Coat.md']],
] as Array<[string, string[]]>) {
  emit({ kind: 'join', id, parts, out: joinFolder(...parts) });
}
for (const [id, a, b] of [
  ['case', 'Formal', 'formal'], ['empty-null', '', null], ['parent-child', 'Formal', 'Formal/Evening'],
  ['both-null', null, null], ['undefined-folder', undefined, 'Formal'], ['non-ascii-case', 'Été', 'été'],
] as Array<[string, Nullish, Nullish]>) {
  emit({ kind: 'same', id, a: wire(a), b: wire(b), out: isSameFolder(a, b) });
}
for (const [id, folder, ancestor] of [
  ['child-case', 'Formal/Evening', 'formal'], ['self', 'Formal', 'Formal'], ['prefix-not-ancestor', 'Formalwear', 'Formal'],
  ['root-ancestor', 'Anything', ''], ['root-under-folder', '', 'Formal'], ['non-ascii-case', 'Été/Robe', 'été'],
] as Array<[string, string, string]>) {
  emit({ kind: 'under', id, folder, ancestor, out: isUnderFolder(folder, ancestor) });
}
for (const [id, folder, from, to] of [
  ['child-to-parent', 'Formal/Evening/Gala', 'Formal/Evening', 'Formal'], ['to-root', 'Formal', 'Formal', ''],
  ['not-under', 'Casual', 'Formal', ''], ['case-different-ancestor', 'Formal/Evening', 'formal', 'Dress'],
  ['from-root', 'Casual/Beach', '', 'Summer'], ['non-ascii', 'Été/Robe', 'été', 'Summer'],
] as Array<[string, string, string, string]>) {
  emit({ kind: 'rebase', id, folder, from, to, out: rebaseFolder(folder, from, to) });
}

// ── the Inventory delegates (lib/inventory/folders.ts) ──────────────────────
for (const [id, input] of normalizeCases) {
  emit({ kind: 'inv_normalize', id, input: wire(input), out: attempt(() => normalizeInventoryFolderPath(input)) });
  emit({ kind: 'inv_normalize_or_root', id, input: wire(input), out: normalizeInventoryFolderPathOrRoot(input) });
}
for (const [id, input] of nameCases) {
  emit({ kind: 'inv_normalize_name', id, input: wire(input), out: attempt(() => normalizeInventoryFolderName(input)) });
}
for (const [id, path] of safeCases) {
  emit({ kind: 'inv_safe', id, path, out: isSafeInventoryFolderPath(path) });
  emit({ kind: 'inv_valid', id, path, out: isValidInventoryFolderPath(path) });
}
for (const [id, existing, requested] of resolveCases) {
  emit({
    kind: 'inv_resolve', id, existing: [...existing], requested: wire(requested),
    out: attempt(() => resolveInventoryFolder(existing, requested)),
  });
}
for (const [id, path] of [...folderOfCases, ...docPathCases]) {
  emit({ kind: 'inv_paths', id, path, folderOf: folderOfInventoryPath(path), isDoc: isInventoryItemDocumentPath(path) });
}
for (const [id, folder] of viewCases) {
  emit({ kind: 'inv_store_path', id, folder, out: inventoryFolderStorePath(folder) });
}
