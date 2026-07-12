/**
 * P4.6w pure tier-1 oracle: `computeRenameTarget` (the Document Mode rename
 * math). Drives v4's REAL `lib/documents/operator-doc-actions.ts` over a fixed
 * corpus of (currentFilePath, newTitle) pairs and emits the exact
 * `RenameTarget` result as NDJSON. The Rust side
 * (`documents_rename_target_equivalence`) rebuilds the same JSON shape from its
 * enum and compares byte-for-byte — the extension-inheritance, directory
 * preservation, trim, and rejection arms are all under test.
 *
 * Run from inside the server checkout:
 *   cd ~/source/quilltap-server
 *   npx tsx ~/source/quilltap-v5/harness/oracle/cases/documents-rename-target.ts \
 *     > /tmp/oracle-documents-rename-target.ndjson
 */

import { computeRenameTarget } from '@/lib/documents/operator-doc-actions';

// [id, currentFilePath, newTitle]
const cases: Array<[string, string, string]> = [
  ['inherit-ext-nested', 'Notes/today.md', 'tomorrow'],
  ['inherit-ext-root', 'today.md', 'backstory'],
  ['explicit-ext-swap', 'a/b.md', 'c.txt'],
  ['no-ext-both', 'dir/README', 'LICENSE'],
  ['no-ext-both-root', 'README', 'LICENSE'],
  ['trim-spaces', 'a/b.md', '  spaced  '],
  ['trim-tabs-newlines', 'a/b.md', '\t name \n'],
  ['multi-dot-title-kept', 'a/b.md', 'notes.v2'],
  ['dotfile-title-inherits', 'a/b.md', '.hidden'],
  ['bare-extension-title', 'a/b.txt', '.md'],
  ['deeper-dir-preserved', 'x/y/z/old.json', 'new'],
  ['reject-empty', 'a.md', '   '],
  ['reject-slash', 'a.md', 'b/c'],
  ['reject-backslash', 'a.md', 'b\\c'],
  ['reject-dot', 'a.md', '.'],
  ['reject-dotdot', 'a.md', '..'],
];

for (const [id, current, title] of cases) {
  const out = computeRenameTarget(current, title);
  process.stdout.write(JSON.stringify({ id, current, title, out }) + '\n');
}
