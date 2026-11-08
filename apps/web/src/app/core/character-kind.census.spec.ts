/**
 * The character-kind chokepoint fence — the SPA twin of v4
 * `__tests__/unit/lib/characters/kind-chokepoint.test.ts` at the pin
 * `624d0d25c` (R-K6).
 *
 * A character's kind (roleplay / assistant) is read and compared only in
 * `core/character-kind.ts`. Everywhere else asks `resolveCharacterKind` or
 * `isAssistantCharacter`. A hand comparison against a kind literal is how a
 * default drifts: one site treats an absent value as roleplay, another as
 * assistant, and a third kind is missed in half the places that branch.
 *
 * The regex and the five self-test assertions are v4's VERBATIM. v4 scans
 * `lib`, `app`, `components`, `hooks` and skips `node_modules` / `__tests__`;
 * v5's SPA is `src/app` and its tests are colocated `*.spec.ts`, skipped by
 * name — the same rule.
 *
 * v4's pattern also matches a `StreamMessage`'s `kind` discriminant, which
 * shares the literal `'assistant'` with a character kind but is not one. Those
 * sites are a NAMED allowlist (`STREAM_MESSAGE_KIND_SITES`, P4.D388 PR-4),
 * matched by exact trimmed source TEXT, never a line number; an entry that
 * matches no line FAILS the census, so it cannot outlive its site.
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

/** `kind === 'assistant'`, `'roleplay' !== x.kind`, and so on. v4's, verbatim. */
const KIND_COMPARISON =
  /\bkind\s*[!=]==?\s*['"](?:roleplay|assistant)['"]|['"](?:roleplay|assistant)['"]\s*[!=]==?\s*[\w.?]*\bkind\b/;

/** The one module allowed to compare a kind (v4 `lib/characters/kind.ts`). */
const CHOKEPOINT = 'core/character-kind.ts';

/** `StreamMessage.kind` discriminants — not character kinds (PR-4). */
const STREAM_MESSAGE_KIND_SITES: ReadonlyArray<{ file: string; line: string }> = [
  { file: 'core/chat-stream.reducer.ts', line: "m.kind === 'assistant' && m.id === result.messageId" },
  { file: 'chat/message-list.ts', line: "if (sm.kind !== 'assistant') {" },
];

/** `apps/web/src/app`, from the runner's working directory (`apps/web`). */
function appRoot(): string {
  const cwd = (globalThis as unknown as { process: { cwd(): string } }).process.cwd();
  return `${cwd.replace(/\\/g, '/')}/src/app`;
}

/** Every non-spec `.ts` file under `dir`, as a path relative to `src/app`. */
function sourceFiles(dir = '', root = appRoot()): string[] {
  const out: string[] = [];
  for (const entry of fs.readdirSync(dir ? `${root}/${dir}` : root)) {
    if (entry === 'node_modules') continue;
    const rel = dir ? `${dir}/${entry}` : entry;
    if (fs.statSync(`${root}/${rel}`).isDirectory()) {
      out.push(...sourceFiles(rel, root));
    } else if (entry.endsWith('.ts') && !entry.endsWith('.spec.ts')) {
      out.push(rel);
    }
  }
  return out;
}

interface Hit {
  file: string;
  lineNo: number;
  text: string;
}

function hits(): Hit[] {
  const root = appRoot();
  const found: Hit[] = [];
  for (const rel of sourceFiles()) {
    if (rel === CHOKEPOINT) continue;
    fs.readFileSync(`${root}/${rel}`, 'utf8')
      .split('\n')
      .forEach((line: string, i: number) => {
        if (KIND_COMPARISON.test(line)) found.push({ file: rel, lineNo: i + 1, text: line.trim() });
      });
  }
  return found;
}

const allowed = (h: Hit, sites: ReadonlyArray<{ file: string; line: string }>): boolean =>
  sites.some((s) => s.file === h.file && s.line === h.text);

function offenders(sites: ReadonlyArray<{ file: string; line: string }>): string[] {
  return hits()
    .filter((h) => !allowed(h, sites))
    .map((h) => `${h.file}:${h.lineNo}: ${h.text}`);
}

describe('character kind chokepoint', () => {
  it('scans a real tree (a wrong root must not pass vacuously)', () => {
    const files = sourceFiles();
    expect(files).toContain(CHOKEPOINT);
    expect(files).toContain('core/core-contract.ts');
    expect(files.length).toBeGreaterThan(100);
  });

  it('compares a kind literal only in core/character-kind.ts', () => {
    expect(offenders(STREAM_MESSAGE_KIND_SITES)).toEqual([]);
  });

  it('the allowlist bites: without it the census names exactly the two StreamMessage sites', () => {
    expect(offenders([]).map((o) => o.replace(/:\d+:/, ':'))).toEqual([
      "chat/message-list.ts: if (sm.kind !== 'assistant') {",
      "core/chat-stream.reducer.ts: m.kind === 'assistant' && m.id === result.messageId",
    ]);
  });

  it('every allowlist entry still matches a line (no stale entry)', () => {
    const found = hits();
    for (const site of STREAM_MESSAGE_KIND_SITES) {
      expect(
        found.some((h) => h.file === site.file && h.text === site.line),
        `${site.file}: ${site.line}`,
      ).toBe(true);
    }
  });

  it('the fence pattern catches the comparisons it is meant to', () => {
    expect(KIND_COMPARISON.test("if (character.kind === 'assistant') {")).toBe(true);
    expect(KIND_COMPARISON.test("c.kind !== 'roleplay'")).toBe(true);
    expect(KIND_COMPARISON.test("'assistant' === character?.kind")).toBe(true);
    expect(KIND_COMPARISON.test("descriptor.kind === 'markdown'")).toBe(false);
    expect(KIND_COMPARISON.test("message.role === 'assistant'")).toBe(false);
  });
});
