/**
 * Tier-1 oracle case — the sync's MANIFEST and SIDECAR modules (v4
 * `lib/mount-index/sync/manifest.ts` + `sidecar.ts`, added at `23da0b322`).
 *
 * Drives v4's REAL modules and emits one NDJSON row per probe. Four kinds:
 *
 *   - `sidecar-path` — the naming functions. `partnerPathFor` slices by the
 *     suffix's LENGTH after matching on the lower-cased string, so an
 *     upper-cased sidecar's partner keeps its own casing.
 *   - `sidecar-text` — render / parse / hash. The whitespace rows are the
 *     load-bearing ones: v4 strips with `/\s+$/`, whose class is ECMAScript's
 *     `\s`, NOT Rust's `char::is_whitespace` (they disagree on U+FEFF and
 *     U+0085), and the hash is computed on the PARSED text so an editor's
 *     newline is not an edit.
 *   - `manifest-bytes` — what `writeManifest` actually puts on disk, read back
 *     verbatim. `JSON.stringify(m, null, 2) + '\n'`, so key order and indent
 *     are part of the comparison.
 *   - `manifest-read` — `readManifest` against a directory seeded with the
 *     given bytes: the first-run `null`, the degrade-to-null warnings with
 *     their WORDING (a Zod issue message reaches the operator), and the
 *     mismatch throw with its sentence.
 *
 * Run from the v4 server checkout:
 *   cd ~/source/quilltap-server
 *   V5W=${V5W:-$HOME/source/quilltap-v5}
 *   npx tsx $V5W/harness/oracle/cases/sync-manifest-sidecar.ts > /tmp/oracle-sync-manifest-sidecar.ndjson
 */

import { mkdtempSync, rmSync, writeFileSync, readFileSync } from 'fs';
import { join } from 'path';
import { tmpdir } from 'os';

import {
  sidecarPathFor,
  isSidecarPath,
  partnerPathFor,
  renderSidecar,
  parseSidecar,
  descriptionSha256,
  descriptionsEqual,
} from '@/lib/mount-index/sync/sidecar';
import {
  ManifestMismatchError,
  readManifest,
  writeManifest,
  baseFromManifest,
  manifestPathFor,
} from '@/lib/mount-index/sync/manifest';
import { SYNC_MANIFEST_FILENAME, type SyncManifest } from '@/lib/mount-index/sync/types';

const emit = (row: unknown) => process.stdout.write(JSON.stringify(row) + '\n');

// ---------------------------------------------------------------------------
// sidecar
// ---------------------------------------------------------------------------

const SIDECAR_INPUTS: Array<{ id: string; value: string }> = [
  { id: 'plain', value: 'lore/harbour.png' },
  { id: 'nested', value: 'a/b/c/harbour.png' },
  { id: 'bare-name', value: 'harbour.png' },
  { id: 'already-a-sidecar', value: 'lore/harbour.png.description.md' },
  { id: 'sidecar-upper', value: 'lore/harbour.png.DESCRIPTION.MD' },
  { id: 'sidecar-mixed', value: 'lore/harbour.png.Description.Md' },
  { id: 'suffix-only', value: '.description.md' },
  { id: 'suffix-only-upper', value: '.DESCRIPTION.MD' },
  { id: 'almost-a-sidecar', value: 'harbour.description.markdown' },
  { id: 'description-md-as-a-name', value: 'description.md' },
  { id: 'empty', value: '' },
  { id: 'double-suffix', value: 'x.description.md.description.md' },
  { id: 'non-ascii', value: 'lore/hâfen.png' },
];

for (const { id, value } of SIDECAR_INPUTS) {
  emit({
    kind: 'sidecar-path',
    id,
    value,
    sidecarPathFor: sidecarPathFor(value),
    isSidecarPath: isSidecarPath(value),
    partnerPathFor: partnerPathFor(value),
  });
}

const TEXT_INPUTS: Array<{ id: string; value: string }> = [
  { id: 'empty', value: '' },
  { id: 'plain', value: 'A map of the harbour.' },
  { id: 'trailing-newline', value: 'A map of the harbour.\n' },
  { id: 'many-trailing-newlines', value: 'A map.\n\n\n' },
  { id: 'trailing-spaces', value: 'A map.   ' },
  { id: 'trailing-mixed', value: 'A map. \t\r\n ' },
  { id: 'only-whitespace', value: '   \n\t ' },
  { id: 'interior-newlines-kept', value: 'line one\n\nline two' },
  { id: 'leading-whitespace-kept', value: '   indented' },
  // The two characters where ECMAScript `\s` and Rust's `char::is_whitespace`
  // disagree: U+FEFF (JS strips it, Rust does not) and U+0085 (Rust strips it,
  // JS does not).
  { id: 'trailing-bom', value: 'A map.﻿' },
  { id: 'trailing-nel', value: 'A map.' },
  { id: 'trailing-nbsp', value: 'A map. ' },
  { id: 'trailing-ideographic-space', value: 'A map.　' },
  { id: 'trailing-line-separator', value: 'A map. ' },
  { id: 'trailing-paragraph-separator', value: 'A map. ' },
  { id: 'trailing-ogham', value: 'A map. ' },
  { id: 'trailing-vertical-tab', value: 'A map.' },
  { id: 'trailing-form-feed', value: 'A map.' },
  { id: 'trailing-narrow-nbsp', value: 'A map. ' },
  { id: 'trailing-mmsp', value: 'A map. ' },
  { id: 'trailing-en-quad', value: 'A map. ' },
  { id: 'trailing-hair-space', value: 'A map. ' },
  // U+200B (zero-width space) is NOT in ECMAScript's `\s` and not in Rust's
  // either — the control that says the two lists are being compared, not a
  // blanket "strip anything invisible".
  { id: 'trailing-zwsp', value: 'A map.​' },
  { id: 'astral-tail', value: 'A map 🗺' },
];

for (const { id, value } of TEXT_INPUTS) {
  emit({
    kind: 'sidecar-text',
    id,
    value,
    renderSidecar: renderSidecar(value),
    parseSidecar: parseSidecar(value),
    descriptionSha256: descriptionSha256(value),
  });
}

const EQUAL_PAIRS: Array<{ id: string; a: string | undefined; b: string | undefined }> = [
  { id: 'both-undefined', a: undefined, b: undefined },
  { id: 'undefined-vs-empty', a: undefined, b: '' },
  { id: 'undefined-vs-whitespace', a: undefined, b: '  \n' },
  { id: 'same', a: 'a caption', b: 'a caption' },
  { id: 'newline-difference', a: 'a caption', b: 'a caption\n' },
  { id: 'interior-difference', a: 'a\ncaption', b: 'a caption' },
  { id: 'different', a: 'a caption', b: 'another caption' },
  { id: 'bom-difference', a: 'a caption', b: 'a caption﻿' },
  { id: 'nel-difference', a: 'a caption', b: 'a caption' },
];

for (const { id, a, b } of EQUAL_PAIRS) {
  emit({
    kind: 'descriptions-equal',
    id,
    a: a ?? null,
    b: b ?? null,
    out: descriptionsEqual(a, b),
  });
}

// ---------------------------------------------------------------------------
// manifest
// ---------------------------------------------------------------------------

const scratch = mkdtempSync(join(tmpdir(), 'qt-sync-manifest-'));

const MANIFESTS: Array<{ id: string; manifest: SyncManifest }> = [
  {
    id: 'empty-entries',
    manifest: {
      version: 1,
      storeId: 'store-1',
      storeName: 'Lore',
      lastSyncAt: '2026-09-20T10:00:00.000Z',
      entries: {},
    },
  },
  {
    id: 'a-file-a-folder-and-a-caption',
    manifest: {
      version: 1,
      storeId: 'store-1',
      storeName: 'Lore',
      lastSyncAt: '2026-09-20T10:00:00.000Z',
      entries: {
        // Insertion order is what `JSON.stringify` writes, so the folder comes
        // out first here even though it sorts later.
        'zz-folder': { kind: 'folder', createdAt: '2024-01-01T00:00:00.000Z' },
        'chapters/01.md': {
          kind: 'file',
          sha256: 'a'.repeat(64),
          lastModified: '2026-09-10T10:00:00.000Z',
          createdAt: '2024-01-01T00:00:00.000Z',
        },
        'lore/harbour.png': {
          kind: 'file',
          sha256: 'b'.repeat(64),
          lastModified: '2026-09-10T10:00:00.000Z',
          createdAt: null,
          descriptionSha256: descriptionSha256('A map of the harbour.'),
          descriptionUpdatedAt: '2026-09-11T10:00:00.000Z',
        },
      },
    },
  },
  {
    id: 'a-null-descriptionUpdatedAt-and-an-escaped-name',
    manifest: {
      version: 1,
      storeId: 'store-1',
      storeName: 'A name with "quotes" and a \\ backslash',
      lastSyncAt: '2026-09-20T10:00:00.000Z',
      entries: {
        'x.png': {
          kind: 'file',
          sha256: 'c'.repeat(64),
          lastModified: '2026-09-10T10:00:00.000Z',
          createdAt: '2024-01-01T00:00:00.000Z',
          descriptionSha256: descriptionSha256(''),
          descriptionUpdatedAt: null,
        },
      },
    },
  },
];

async function main(): Promise<void> {
  for (const { id, manifest } of MANIFESTS) {
    const dir = mkdtempSync(join(scratch, 'w-'));
    await writeManifest(dir, manifest);
    const bytes = readFileSync(manifestPathFor(dir), 'utf-8');
    const warnings: string[] = [];
    const readBack = await readManifest(dir, manifest.storeId, warnings);
    emit({
      kind: 'manifest-bytes',
      id,
      manifest,
      bytes,
      readBackWarnings: warnings,
      // The base is the planner's view of the manifest: lower-cased keys, in
      // insertion order.
      base: Object.fromEntries(baseFromManifest(readBack)),
    });
  }

  const READS: Array<{ id: string; write?: string; storeId: string }> = [
    { id: 'absent', storeId: 'store-1' },
    { id: 'not-json', write: '{ not json', storeId: 'store-1' },
    { id: 'empty-file', write: '', storeId: 'store-1' },
    { id: 'json-but-not-an-object', write: '[]', storeId: 'store-1' },
    { id: 'json-null', write: 'null', storeId: 'store-1' },
    {
      id: 'wrong-version',
      write: JSON.stringify({
        version: 2,
        storeId: 'store-1',
        storeName: 'L',
        lastSyncAt: 'x',
        entries: {},
      }),
      storeId: 'store-1',
    },
    {
      id: 'missing-entries',
      write: JSON.stringify({
        version: 1,
        storeId: 'store-1',
        storeName: 'L',
        lastSyncAt: 'x',
      }),
      storeId: 'store-1',
    },
    {
      id: 'entries-not-an-object',
      write: JSON.stringify({
        version: 1,
        storeId: 'store-1',
        storeName: 'L',
        lastSyncAt: 'x',
        entries: 7,
      }),
      storeId: 'store-1',
    },
    {
      id: 'missing-storeName',
      write: JSON.stringify({ version: 1, storeId: 'store-1', lastSyncAt: 'x', entries: {} }),
      storeId: 'store-1',
    },
    {
      id: 'missing-lastSyncAt',
      write: JSON.stringify({ version: 1, storeId: 'store-1', storeName: 'L', entries: {} }),
      storeId: 'store-1',
    },
    {
      id: 'empty-storeId',
      write: JSON.stringify({
        version: 1,
        storeId: '',
        storeName: 'L',
        lastSyncAt: 'x',
        entries: {},
      }),
      storeId: '',
    },
    {
      id: 'bad-entry-kind',
      write: JSON.stringify({
        version: 1,
        storeId: 'store-1',
        storeName: 'L',
        lastSyncAt: 'x',
        entries: { 'a.md': { kind: 'sideways' } },
      }),
      storeId: 'store-1',
    },
    {
      id: 'entry-is-not-an-object',
      write: JSON.stringify({
        version: 1,
        storeId: 'store-1',
        storeName: 'L',
        lastSyncAt: 'x',
        entries: { 'a.md': 3 },
      }),
      storeId: 'store-1',
    },
    {
      id: 'mismatched-store',
      write: JSON.stringify({
        version: 1,
        storeId: 'store-2',
        storeName: 'L',
        lastSyncAt: 'x',
        entries: {},
      }),
      storeId: 'store-1',
    },
    // The store check runs BEFORE schema validation, so a manifest that is both
    // another store's AND malformed is named as the former.
    {
      id: 'mismatched-and-malformed',
      write: JSON.stringify({ storeId: 'store-2', entries: 'nope' }),
      storeId: 'store-1',
    },
    {
      id: 'storeId-not-a-string',
      write: JSON.stringify({
        version: 1,
        storeId: 7,
        storeName: 'L',
        lastSyncAt: 'x',
        entries: {},
      }),
      storeId: 'store-1',
    },
    {
      id: 'version-missing',
      write: JSON.stringify({ storeId: 'store-1', storeName: 'L', lastSyncAt: 'x', entries: {} }),
      storeId: 'store-1',
    },
    {
      id: 'version-as-a-string',
      write: JSON.stringify({
        version: '1',
        storeId: 'store-1',
        storeName: 'L',
        lastSyncAt: 'x',
        entries: {},
      }),
      storeId: 'store-1',
    },
    {
      id: 'storeName-not-a-string',
      write: JSON.stringify({
        version: 1,
        storeId: 'store-1',
        storeName: 7,
        lastSyncAt: 'x',
        entries: {},
      }),
      storeId: 'store-1',
    },
    {
      id: 'entry-kind-missing',
      write: JSON.stringify({
        version: 1,
        storeId: 'store-1',
        storeName: 'L',
        lastSyncAt: 'x',
        entries: { 'a.md': { sha256: 'e'.repeat(64) } },
      }),
      storeId: 'store-1',
    },
    {
      id: 'entry-sha256-not-a-string',
      write: JSON.stringify({
        version: 1,
        storeId: 'store-1',
        storeName: 'L',
        lastSyncAt: 'x',
        entries: { 'a.md': { kind: 'file', sha256: 7 } },
      }),
      storeId: 'store-1',
    },
    {
      id: 'entry-createdAt-not-a-string',
      write: JSON.stringify({
        version: 1,
        storeId: 'store-1',
        storeName: 'L',
        lastSyncAt: 'x',
        entries: { 'a.md': { kind: 'file', createdAt: 7 } },
      }),
      storeId: 'store-1',
    },
    {
      // `.nullable().optional()`: an explicit null is VALID and distinct from
      // an absent key, which is what the port's nested Option carries.
      id: 'entry-createdAt-null-is-valid',
      write: JSON.stringify({
        version: 1,
        storeId: 'store-1',
        storeName: 'L',
        lastSyncAt: 'x',
        entries: { 'a.md': { kind: 'file', sha256: 'e'.repeat(64), createdAt: null } },
      }),
      storeId: 'store-1',
    },
    {
      id: 'a-second-issue-is-not-the-one-reported',
      write: JSON.stringify({
        version: 2,
        storeId: 7,
        storeName: 'L',
        lastSyncAt: 'x',
        entries: {},
      }),
      storeId: 'store-1',
    },
    {
      id: 'uppercase-keys-lowercase-in-the-base',
      write: JSON.stringify({
        version: 1,
        storeId: 'store-1',
        storeName: 'L',
        lastSyncAt: 'x',
        entries: { 'Lore/Harbour.PNG': { kind: 'file', sha256: 'd'.repeat(64) } },
      }),
      storeId: 'store-1',
    },
  ];

  for (const { id, write, storeId } of READS) {
    const dir = mkdtempSync(join(scratch, 'r-'));
    if (write !== undefined) writeFileSync(join(dir, SYNC_MANIFEST_FILENAME), write, 'utf-8');
    const warnings: string[] = [];
    let manifest: unknown = null;
    let error: string | null = null;
    let errorName: string | null = null;
    try {
      manifest = await readManifest(dir, storeId, warnings);
    } catch (err) {
      error = err instanceof Error ? err.message : String(err);
      errorName = err instanceof ManifestMismatchError ? 'ManifestMismatchError' : 'Error';
    }
    emit({
      kind: 'manifest-read',
      id,
      write: write ?? null,
      storeId,
      manifest,
      warnings,
      error,
      errorName,
      base:
        manifest === null ? {} : Object.fromEntries(baseFromManifest(manifest as SyncManifest)),
    });
  }

  rmSync(scratch, { recursive: true, force: true });
}

main().catch((err) => {
  process.stderr.write(`sync-manifest-sidecar oracle failed: ${err?.stack ?? err}\n`);
  process.exit(1);
});
