/**
 * The raw-clause-read census — the SPA twin of v4's own guard (`a53024d3c`):
 * once a gate, an outcome `when` and an effect `when` may each be an any-of
 * ARRAY, a reader that reaches straight into a clause's subjects
 * (`availableWhen.metadata`, `outcome.when.llm`, `effect.when?.progress`) reads
 * `undefined` off an array and silently tests nothing. Every reader goes
 * through the accessors instead — `gateGroups`, `whenGroups`,
 * `effectWhenGroups` in `./custom-tool-types` — and this census holds the
 * line.
 *
 * The pattern is v4's VERBATIM. Exempt: the schema module itself (it builds
 * the clauses) and `./tool-draft` (the draft layer's group-aware re-port is
 * P4.D334's — the unifier drops the exemption when that lane lands, if the
 * draft layer no longer needs it).
 *
 * Measured red-first on the tree before P4.D333: with the two exemptions
 * removed, the census names `pascal/tool-draft.ts` twice (`effect.when.outcome`
 * at the outcome-state reader, `effect.when.when.llm` in the draft audit);
 * nothing outside those two files matches.
 */

import { beforeAll, describe, expect, it } from 'vitest';

/**
 * The slice of `node:fs` this census reads. The Angular unit-test builder
 * bundles specs for the browser platform and the spec tsconfig carries no Node
 * types, so `node:fs` is loaded at RUN time (vitest executes in Node) through
 * a non-literal specifier the bundler leaves alone, and typed here by hand.
 */
interface NodeFs {
  readdirSync(path: string): string[];
  readFileSync(path: string, encoding: 'utf8'): string;
  statSync(path: string): { isDirectory(): boolean };
}

let fs: NodeFs;

beforeAll(async () => {
  const specifier = 'node:fs';
  fs = (await import(/* @vite-ignore */ specifier)) as NodeFs;
});

/** v4's pattern, verbatim. */
const RAW_CLAUSE_READ =
  /(?:availableWhen|withheldWhen|\.when)\??\.(?:metadata|progress|inventory|inventoryCount|inventoryAggregate|params|roll|llm|outcome)\b/;

/** The files that BUILD clauses, and so read them raw by construction. */
const EXEMPT: ReadonlySet<string> = new Set(['pascal/custom-tool-types.ts', 'pascal/tool-draft.ts']);

/** `apps/web/src/app`, from the runner's working directory (`apps/web`). */
function appRoot(): string {
  const cwd = (globalThis as unknown as { process: { cwd(): string } }).process.cwd();
  return `${cwd.replace(/\\/g, '/')}/src/app`;
}

/** Every non-spec `.ts` file under `dir`, as a path relative to `src/app`. */
function sourceFiles(dir = '', root = appRoot()): string[] {
  const out: string[] = [];
  for (const entry of fs.readdirSync(dir ? `${root}/${dir}` : root)) {
    const rel = dir ? `${dir}/${entry}` : entry;
    if (fs.statSync(`${root}/${rel}`).isDirectory()) {
      out.push(...sourceFiles(rel, root));
    } else if (entry.endsWith('.ts') && !entry.endsWith('.spec.ts')) {
      out.push(rel);
    }
  }
  return out;
}

/** Every `file:line: text` the pattern matches, relative to `src/app`. */
function offenders(exempt: ReadonlySet<string>): string[] {
  const root = appRoot();
  const found: string[] = [];
  for (const rel of sourceFiles()) {
    if (exempt.has(rel)) continue;
    fs.readFileSync(`${root}/${rel}`, 'utf8')
      .split('\n')
      .forEach((line: string, i: number) => {
        if (RAW_CLAUSE_READ.test(line)) found.push(`${rel}:${i + 1}: ${line.trim()}`);
      });
  }
  return found;
}

describe('the raw clause-read census (v4 a53024d3c)', () => {
  it('scans a real tree (a wrong root must not pass vacuously)', () => {
    const files = sourceFiles();
    expect(files).toContain('pascal/custom-tool-types.ts');
    expect(files).toContain('pascal/tool-gate.ts');
    expect(files.length).toBeGreaterThan(50);
  });

  it('the pattern bites — it finds the raw reads in an exempt file when the exemption is lifted', () => {
    // The teeth: without this, a regex that matched nothing would pass forever.
    expect(offenders(new Set()).some((o) => o.startsWith('pascal/tool-draft.ts:'))).toBe(true);
    expect(RAW_CLAUSE_READ.test('if (definition.availableWhen?.metadata) {')).toBe(true);
    expect(RAW_CLAUSE_READ.test('const llm = outcome.when.llm;')).toBe(true);
    expect(RAW_CLAUSE_READ.test('gateGroups(definition.availableWhen)')).toBe(false);
    expect(RAW_CLAUSE_READ.test('when.metadata')).toBe(false);
  });

  it('no reader outside the exemptions reaches into a clause raw', () => {
    expect(offenders(EXEMPT)).toEqual([]);
  });

  it('every exemption names a file that exists', () => {
    for (const rel of EXEMPT) {
      expect(() => fs.statSync(`${appRoot()}/${rel}`), rel).not.toThrow();
    }
  });
});
