/**
 * The stream-frames chokepoint census (P4.D384 Tier 2 item 13) — the SPA
 * mirror of P4.D366's server chokepoint: every Salon reader of a chat-scoped
 * frame types it through `parseFrameValue` (`./stream-frames`) before folding,
 * the legacy-bag fold has exactly ONE non-spec caller (the Brahma console,
 * ruling P-2), and the legacy bag's keys are read only by the two legacy
 * readers (the legacy fold and the Help reducer).
 *
 * Measured red-first by construction: before P4.D384 `reduceChatFrame` was
 * fed the raw `ScopedEvent` by both the Salon and the Brahma console, and
 * `parseFrameValue` did not exist.
 */

import { beforeAll, describe, expect, it } from 'vitest';

/** The slice of `node:fs` this census reads (loaded at run time — see `pascal/raw-gate-reads.census.spec.ts`). */
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

function appRoot(): string {
  const cwd = (globalThis as unknown as { process: { cwd(): string } }).process.cwd();
  return `${cwd.replace(/\\/g, '/')}/src/app`;
}

function sourceFiles(dir = '', root = appRoot()): string[] {
  const out: string[] = [];
  for (const entry of fs.readdirSync(dir ? `${root}/${dir}` : root)) {
    const rel = dir ? `${dir}/${entry}` : entry;
    if (fs.statSync(`${root}/${rel}`).isDirectory()) {
      out.push(...sourceFiles(rel, root));
    } else if (entry.endsWith('.ts') && !entry.endsWith('.spec.ts') && !entry.endsWith('.testing.ts')) {
      out.push(rel);
    }
  }
  return out;
}

function read(rel: string): string {
  return fs.readFileSync(`${appRoot()}/${rel}`, 'utf8');
}

/** Files (other than the reducer module itself) whose code CALLS `fn(`. */
function callers(fn: string): string[] {
  const call = new RegExp(`\\b${fn}\\(`);
  return sourceFiles()
    .filter((rel) => rel !== 'core/chat-stream.reducer.ts')
    .filter((rel) => read(rel).split('\n').some((line) => call.test(line) && !line.trim().startsWith('*')))
    .sort();
}

const LEGACY_KEY_READ = /frame\.(concurrent|carinaAnswer|hostAnnouncement|pascalResult|toolNames)\b/;

describe('the stream-frames chokepoint census (P4.D384)', () => {
  it('scans a real tree (a wrong root must not pass vacuously)', () => {
    const files = sourceFiles();
    expect(files).toContain('core/stream-frames.ts');
    expect(files).toContain('screens/salon/salon-conversation.ts');
    expect(files.length).toBeGreaterThan(100);
  });

  it('the Salon fold is the only non-spec caller of reduceChatFrame, and it parses first', () => {
    expect(callers('reduceChatFrame')).toEqual(['screens/salon/salon-conversation.ts']);
    const salon = read('screens/salon/salon-conversation.ts');
    expect(salon).toMatch(/const frame = parseFrameValue\(ev\);\s+if \(!frame\) return;[\s\S]{0,80}reduceChatFrame\(state, frame\)/);
  });

  it('the legacy fold has exactly ONE non-spec caller: the Brahma console (ruling P-2)', () => {
    expect(callers('reduceLegacyChatFrame')).toEqual(['brahma/brahma-console-panel.ts']);
  });

  it('every Salon frame reader goes through parseFrameValue (the fold, Document Mode, the swipe)', () => {
    expect(callers('parseFrameValue').filter((f) => f !== 'core/stream-frames.ts')).toEqual([
      'chat/regeneration.state.ts',
      'screens/salon/salon-conversation.ts',
    ]);
    expect(read('screens/salon/salon-conversation.ts').match(/parseFrameValue\(ev\)/g)).toHaveLength(2);
    expect(read('chat/regeneration.state.ts')).toMatch(/parseFrameValue\(frame\.frame\)/);
  });

  // The order's census names the Help reducer too; measured, `help/help-stream.ts`
  // reads none of these five keys (its reducer reads `content` / `toolResult` /
  // `done` …), so the legacy fold is the only reader.
  it('the legacy bag keys are read only by the legacy fold', () => {
    const readers = sourceFiles()
      .filter((rel) => read(rel).split('\n').some((line) => LEGACY_KEY_READ.test(line)))
      .sort();
    expect(readers).toEqual(['core/chat-stream.reducer.ts']);
    // …and inside the reducer module, only below the legacy fence.
    const reducer = read('core/chat-stream.reducer.ts');
    const fence = reducer.indexOf('// === P4.D384 === the legacy bag fold');
    expect(fence).toBeGreaterThan(0);
    const firstRead = reducer.split('\n').findIndex((line) => LEGACY_KEY_READ.test(line));
    expect(reducer.split('\n').slice(0, firstRead).join('\n').length).toBeGreaterThan(fence);
  });
});
