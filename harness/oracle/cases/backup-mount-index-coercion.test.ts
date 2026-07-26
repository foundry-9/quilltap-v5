/**
 * @jest-environment node
 *
 * P4.d22 MOUNT-INDEX COERCION ORACLE — a **tier-1 exact** family over v4's REAL
 * `lib/backup/restore/mount-index-coercion.ts`, the module `c1507f47` added to
 * fix restore bug 1.
 *
 * ── Why a dedicated pure family rather than leaning on the state diff ────────
 * `system_restore_state` proves the coercion's effect on the COMMITTED archives,
 * and those archives are uniform: every `enabled` / `allowEmbed` /
 * `allowCharacterRead` / `allowCharacterWrite` in them is `1`, and every pattern
 * column is well-formed JSON text. So the state diff cannot distinguish
 * "coerces INTEGER 0 to false" from "defaults everything to true", and cannot
 * see what happens to an empty string, a null, unparseable text, or text that
 * parses to a non-array. Those are exactly the arms a hand-written port gets
 * subtly wrong, and each of them silently loosens a user's stated policy.
 *
 * The corpus is therefore built here rather than sampled from a fixture: one row
 * per storage shape, driven through v4's own exported functions.
 *
 * Emits one NDJSON line per case: `{name, kind, input, output}`. `kind` is
 * `mountPoint` or `fileLink`; `output` is the WHOLE coerced row, so the diff
 * also proves the untouched columns are passed through unchanged and in v4's
 * key order.
 *
 * Run (Node 24, from the v4 checkout — cp to a /tmp mirror; jest ignores .claude/):
 *   N=~/.nvm/versions/node/v24.13.1/bin ; V5W=<this worktree>
 *   TMPO=/tmp/qt-micoerce-oracle
 *   rm -rf "$TMPO"; mkdir -p "$TMPO/cases"
 *   cp "$V5W/harness/oracle/cases/backup-mount-index-coercion.test.ts" "$TMPO/cases/"
 *   cd ~/source/quilltap-server
 *   QT_ORACLE_OUT=/tmp/oracle-mount-index-coercion.ndjson \
 *     $N/npx jest --silent --watchman=false --testTimeout=120000 \
 *       --roots "$PWD" --roots "$TMPO/cases" -- backup-mount-index-coercion
 */

import * as fs from 'fs';

/** A `doc_mount_points` row as `SELECT *` gives it up, minus the columns under test. */
const MP_BASE = {
  id: 'aff3114e-ed90-4d5b-99c1-ef3fa20203fc',
  name: 'Lorian Character Vault',
  basePath: '',
  mountType: 'database',
  storeType: 'character',
  scanStatus: 'idle',
  fileCount: 11,
};

/** A `doc_mount_file_links` row, same idea. */
const LINK_BASE = {
  id: 'bb000000-0000-4000-8000-00000000000a',
  fileId: 'cc000000-0000-4000-8000-00000000000b',
  mountPointId: MP_BASE.id,
  relativePath: 'identity.md',
  fileName: 'identity.md',
  chunkCount: 3,
};

interface Case {
  name: string;
  kind: 'mountPoint' | 'fileLink';
  input: Record<string, unknown>;
}

function cases(): Case[] {
  const out: Case[] = [];
  const mp = (name: string, extra: Record<string, unknown>): void => {
    out.push({ name, kind: 'mountPoint', input: { ...MP_BASE, ...extra } });
  };
  const link = (name: string, extra: Record<string, unknown>): void => {
    out.push({ name, kind: 'fileLink', input: { ...LINK_BASE, ...extra } });
  };

  // ── doc_mount_points: the pattern columns ─────────────────────────────────
  // What a real modern archive actually carries: JSON TEXT.
  mp('mp_patterns_json_text', {
    includePatterns: '["*.md","*.txt","*.pdf","*.docx"]',
    excludePatterns: '[".git","node_modules",".obsidian",".trash"]',
    enabled: 1,
  });
  // The project/group stores archive an EMPTY list as the text "[]" — it must
  // stay empty rather than picking up the four-extension default.
  mp('mp_patterns_empty_json_text', {
    includePatterns: '[]',
    excludePatterns: '[]',
    enabled: 1,
  });
  // Already-correct input (a row that never went through SQLite).
  mp('mp_patterns_real_arrays', {
    includePatterns: ['*.org'],
    excludePatterns: [],
    enabled: true,
  });
  // A mixed array loses its non-strings — but is NOT replaced by the default.
  mp('mp_patterns_mixed_array', {
    includePatterns: ['*.md', 7, null, '*.txt'],
    excludePatterns: [{ a: 1 }],
    enabled: 1,
  });
  // Unusable shapes → the caller's default, per column.
  mp('mp_patterns_empty_string', { includePatterns: '', excludePatterns: '', enabled: 1 });
  mp('mp_patterns_unparseable', {
    includePatterns: 'not json',
    excludePatterns: '*.md,*.txt',
    enabled: 1,
  });
  mp('mp_patterns_parses_to_non_array', {
    includePatterns: '{"a":1}',
    excludePatterns: '"just a string"',
    enabled: 1,
  });
  mp('mp_patterns_null', { includePatterns: null, excludePatterns: null, enabled: 1 });
  mp('mp_patterns_absent', { enabled: 1 });

  // ── doc_mount_points: `enabled` ───────────────────────────────────────────
  // The arm no committed archive reaches: a store the user DISABLED.
  mp('mp_enabled_integer_zero', {
    includePatterns: '["*.md"]',
    excludePatterns: '[]',
    enabled: 0,
  });
  mp('mp_enabled_bool_false', {
    includePatterns: '["*.md"]',
    excludePatterns: '[]',
    enabled: false,
  });
  mp('mp_enabled_absent', { includePatterns: '["*.md"]', excludePatterns: '[]' });
  mp('mp_enabled_null', { includePatterns: '["*.md"]', excludePatterns: '[]', enabled: null });
  // A string is NOT a number and NOT a boolean → the default, even though "0"
  // is truthy in JS. Pinned because it is the one place a naive port inverts.
  mp('mp_enabled_string_zero', {
    includePatterns: '["*.md"]',
    excludePatterns: '[]',
    enabled: '0',
  });

  // ── doc_mount_file_links: the three policy flags ──────────────────────────
  link('link_flags_all_one', {
    allowEmbed: 1,
    allowCharacterRead: 1,
    allowCharacterWrite: 1,
  });
  // The arm no committed archive reaches: a document the user made read-only,
  // or withheld from the embedder.
  link('link_flags_all_zero', {
    allowEmbed: 0,
    allowCharacterRead: 0,
    allowCharacterWrite: 0,
  });
  link('link_flags_mixed', {
    allowEmbed: 0,
    allowCharacterRead: 1,
    allowCharacterWrite: 0,
  });
  link('link_flags_real_booleans', {
    allowEmbed: false,
    allowCharacterRead: true,
    allowCharacterWrite: false,
  });
  link('link_flags_absent', {});
  link('link_flags_null', {
    allowEmbed: null,
    allowCharacterRead: null,
    allowCharacterWrite: null,
  });

  return out;
}

async function main(): Promise<void> {
  const outPath = process.env.QT_ORACLE_OUT;
  if (!outPath) throw new Error('QT_ORACLE_OUT must point at the NDJSON file to write');

  const { coerceDocMountPointRow, coerceDocMountFileLinkRow } = await import(
    '@/lib/backup/restore/mount-index-coercion'
  );

  const lines = cases().map((c) => {
    const output =
      c.kind === 'mountPoint'
        ? coerceDocMountPointRow(c.input as never)
        : coerceDocMountFileLinkRow(c.input as never);
    return JSON.stringify({ name: c.name, kind: c.kind, input: c.input, output });
  });

  fs.writeFileSync(outPath, lines.join('\n') + '\n');
  process.stderr.write(`mount-index-coercion oracle wrote ${outPath} (${lines.length} cases)\n`);
}

test('mount-index-coercion oracle', async () => {
  await main();
});
