/**
 * The D18 retirement census (P4.D387 item 15; the human's ruling R-D).
 *
 * v4 `c34986421` replaced the Scriptorium's file table and the SVAR file
 * manager with ONE document-store browser; v5 follows by retiring BOTH of its
 * old surfaces — the classic `FileTable` / `FileDetailRow` and D18's bespoke
 * file manager (`app/files/**`, the "New file manager (beta)" toggle). A lazy
 * `@defer` chunk or a stale spec could keep a deleted module alive unseen, so
 * this census holds the line over every `.ts` under `src/app` (specs
 * included) and every `e2e/*.ts`:
 *
 * - no import specifier resolving into `src/app/files/`;
 * - no import of a `file-table` / `file-detail-row` / `file-manager` module;
 * - no `qt-file-manager` / `qt-file-table` / `qt-file-detail-row` selector
 *   (comments included — they name a component that is gone);
 * - no `useFileManager`, no `New file manager (beta)` text;
 * - in e2e, no `qt-file-detail-row` / `qt-file-manager` locator.
 *
 * Measured red-first on `BROWSER` (before the deletion) — the count is in the
 * P4.D387 lane record.
 *
 * `node:fs` is loaded at RUN time (the `pascal/raw-gate-reads.census.spec.ts`
 * idiom): the spec bundle targets the browser platform and carries no Node
 * types.
 */

import { beforeAll, describe, expect, it } from 'vitest';

interface NodeFs {
  readdirSync(path: string): string[];
  readFileSync(path: string, encoding: 'utf8'): string;
  statSync(path: string): { isDirectory(): boolean };
  existsSync(path: string): boolean;
}

let fs: NodeFs;

beforeAll(async () => {
  const specifier = 'node:fs';
  fs = (await import(/* @vite-ignore */ specifier)) as NodeFs;
});

/** `apps/web`, the runner's working directory. */
function webRoot(): string {
  const cwd = (globalThis as unknown as { process: { cwd(): string } }).process.cwd();
  return cwd.replace(/\\/g, '/');
}

const SELF = 'document-browser/d18-retirement.census.spec.ts';

/** Every `.ts` file under `dir` (relative to `base`). */
function tsFiles(base: string, dir = ''): string[] {
  const out: string[] = [];
  for (const entry of fs.readdirSync(dir ? `${base}/${dir}` : base)) {
    const rel = dir ? `${dir}/${entry}` : entry;
    if (fs.statSync(`${base}/${rel}`).isDirectory()) out.push(...tsFiles(base, rel));
    else if (entry.endsWith('.ts')) out.push(rel);
  }
  return out;
}

/** Resolve a relative specifier from `fromRel` (both relative to `src/app`). */
function resolveFrom(fromRel: string, specifier: string): string {
  const parts = fromRel.split('/').slice(0, -1);
  for (const seg of specifier.split('/')) {
    if (seg === '..') parts.pop();
    else if (seg !== '.') parts.push(seg);
  }
  return parts.join('/');
}

const IMPORT = /(?:from|import\(|import)\s*['"]([^'"]+)['"]/g;
const RETIRED_MODULE = /(?:^|\/)(?:file-table|file-detail-row|file-manager)(?:\.ts)?$/;
const RETIRED_TEXT =
  /qt-file-manager|qt-file-table|qt-file-detail-row|useFileManager|New file manager \(beta\)/;
const RETIRED_E2E = /qt-file-detail-row|qt-file-manager/;

/** Every D18 site in `src/app`, as `file:line: why`. */
function appOffenders(): string[] {
  const root = `${webRoot()}/src/app`;
  const found: string[] = [];
  for (const rel of tsFiles(root)) {
    if (rel === SELF) continue;
    fs.readFileSync(`${root}/${rel}`, 'utf8')
      .split('\n')
      .forEach((line: string, i: number) => {
        for (const m of line.matchAll(IMPORT)) {
          const spec = m[1];
          if (!spec.startsWith('.')) continue;
          const target = resolveFrom(rel, spec);
          if (target === 'files' || target.startsWith('files/'))
            found.push(`${rel}:${i + 1}: imports ${spec}`);
          else if (RETIRED_MODULE.test(target)) found.push(`${rel}:${i + 1}: imports ${spec}`);
        }
        if (RETIRED_TEXT.test(line)) found.push(`${rel}:${i + 1}: ${line.trim()}`);
      });
  }
  return found;
}

/** Every D18 locator in `e2e/*.ts`. */
function e2eOffenders(): string[] {
  const root = `${webRoot()}/e2e`;
  const found: string[] = [];
  for (const rel of tsFiles(root)) {
    fs.readFileSync(`${root}/${rel}`, 'utf8')
      .split('\n')
      .forEach((line: string, i: number) => {
        if (RETIRED_E2E.test(line)) found.push(`e2e/${rel}:${i + 1}: ${line.trim()}`);
      });
  }
  return found;
}

describe('the D18 retirement census (P4.D387 item 15, ruling R-D)', () => {
  it('scans a real tree (a wrong root must not pass vacuously)', () => {
    const files = tsFiles(`${webRoot()}/src/app`);
    expect(files).toContain('document-browser/document-store-browser.ts');
    expect(files).toContain('screens/scriptorium/store-detail.ts');
    expect(files.length).toBeGreaterThan(500);
    expect(tsFiles(`${webRoot()}/e2e`).length).toBeGreaterThan(50);
  });

  it('the patterns bite', () => {
    expect(resolveFrom('screens/scriptorium/store-detail.ts', '../../files/types')).toBe(
      'files/types',
    );
    expect(resolveFrom('screens/files/file-model.ts', './file-model')).toBe(
      'screens/files/file-model',
    );
    expect(RETIRED_MODULE.test('screens/scriptorium/file-table')).toBe(true);
    expect(RETIRED_MODULE.test('screens/files/file-model')).toBe(false);
    expect(RETIRED_TEXT.test('<qt-file-manager [mountPointId]="s.id" />')).toBe(true);
    expect(RETIRED_TEXT.test('<qt-file-preview-modal />')).toBe(false);
  });

  it('nothing under src/app imports or names a retired D18 module', () => {
    expect(appOffenders()).toEqual([]);
    expect(fs.existsSync(`${webRoot()}/src/app/files`)).toBe(false);
  });

  it('no e2e spec locates a retired D18 element', () => {
    expect(e2eOffenders()).toEqual([]);
  });
});
