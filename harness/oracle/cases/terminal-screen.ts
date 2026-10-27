/**
 * Tier-1 oracle case — the server-side terminal screen model (v4
 * `lib/terminal/screen.ts`, bug 255, v4 `a6292f169`; unmoved to the
 * `322a5488e` pin), P4.D337.
 *
 * Drives v4's REAL `TerminalScreen` / `renderTranscript` (over
 * `@xterm/headless` 6.0.0) through the shared corpus
 * `harness/oracle/fixtures/terminal-screen.json`. Every `render`,
 * `takeSummary` and `transcript` step's answer is emitted in order — not only
 * the ones v4's own `screen.test.ts` asserts.
 *
 * The corpus's `synth` step (a Claude-Code-shaped alternate-screen repaint
 * stream) is EXPANDED here from a fixed seed into plain `write` /
 * `takeSummary` steps, and the expanded step list is ECHOED as `input`, so the
 * Rust side replays THIS case's bytes and never re-generates them.
 *
 * Pure — no DB, no pepper, no jest. One NDJSON row per case:
 * `{ name, cols, rows, scrollback, input: [<steps, synth expanded>],
 * outputs: [<one per render / takeSummary / transcript step>] }`, then a
 * trailing `{ "scrollbackLines": 5000 }` row.
 *
 * Run (Node 24, from the v4 checkout — `screen.ts` arrived at `a6292f169`,
 * past the `e8e17b2d9` baseline, so regenerate from a pin at or after it
 * through the sweep driver until the baseline moves):
 *   N=~/.nvm/versions/node/v24.13.1/bin
 *   V5W=${V5W:-$HOME/source/quilltap-v5}
 *   cd ~/source/quilltap-server
 *   $N/npx tsx $V5W/harness/oracle/cases/terminal-screen.ts \
 *     > /tmp/oracle-terminal-screen.ndjson
 */

import * as fs from 'fs';
import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';

import {
  TerminalScreen,
  renderTranscript,
  SCREEN_SCROLLBACK_LINES,
} from '@/lib/terminal/screen';

type Step =
  | { write: string }
  | { resize: [number, number] }
  | { render: true }
  | { takeSummary: true }
  | { transcript: { raw: string; cols: number; rows: number } }
  | {
      synth: {
        kind: 'alt-repaint';
        frames: number;
        seed: number;
        every: number;
        cols: number;
        rows: number;
      };
    };

interface Case {
  name: string;
  cols: number;
  rows: number;
  scrollback?: number;
  steps: Step[];
}

const ESC = '\u001b';

/** mulberry32 — a fixed-seed PRNG, so the stream is reproducible. */
function prng(seed: number): () => number {
  let a = seed >>> 0;
  return () => {
    a = (a + 0x6d2b79f5) >>> 0;
    let t = a;
    t = Math.imul(t ^ (t >>> 15), t | 1);
    t ^= t + Math.imul(t ^ (t >>> 7), t | 61);
    return ((t ^ (t >>> 14)) >>> 0) / 4294967296;
  };
}

const WORDS = [
  'Reading', 'file', 'src/lib.rs', 'Fixing', 'it.', 'the', 'test', 'passes',
  'cargo', 'build', 'warning:', 'unused', 'variable', '`x`', 'Thinking…',
  'Edit', 'Bash(npm test)', '✓', 'done', 'and', 'then', 'a', 'longer', 'line',
];
const SPINNER = ['✻', '✽', '✶', '✳', '✢', '·'];

/**
 * The shape of Claude Code's output on the alternate screen: every frame homes
 * the cursor and repaints a body with absolute moves, cursor-right in place of
 * spaces, erase-to-end-of-line, 256-colour SGR, a hidden / shown cursor, the
 * `?2026` synchronized-output bracket and a box-drawn prompt.
 */
function expandSynth(s: Extract<Step, { synth: unknown }>['synth']): Step[] {
  const rnd = prng(s.seed);
  const pick = <T>(xs: T[]): T => xs[Math.floor(rnd() * xs.length)];
  const out: Step[] = [{ write: `${ESC}[?1049h${ESC}[H${ESC}[2J` }];
  const body: string[] = [];
  for (let f = 0; f < s.frames; f++) {
    if (body.length === 0 || rnd() < 0.35) {
      const n = 1 + Math.floor(rnd() * 8);
      const words: string[] = [];
      for (let i = 0; i < n; i++) words.push(pick(WORDS));
      body.push(words.join(' ').slice(0, s.cols - 4));
      while (body.length > s.rows - 6) body.shift();
    }
    let frame = `${ESC}[?2026h${ESC}[?25l${ESC}[H`;
    for (let r = 0; r < body.length; r++) {
      const line = body[r];
      const parts = line.split(' ');
      frame += `${ESC}[${r + 1};1H${ESC}[38;5;${(r * 7 + f) % 256}m⏺${ESC}[39m`;
      // Cursor-right in place of spaces.
      frame += `${ESC}[1C${parts.join(`${ESC}[1C`)}${ESC}[K`;
    }
    const top = s.rows - 4;
    frame += `${ESC}[${top};1H${ESC}[38;5;174m${pick(SPINNER)}${ESC}[39m ${pick(WORDS)}… (${f}s)${ESC}[K`;
    const w = s.cols - 2;
    frame += `${ESC}[${top + 1};1H╭${'─'.repeat(w)}╮`;
    frame += `${ESC}[${top + 2};1H│ > ${ESC}[${w - 3}C│`;
    frame += `${ESC}[${top + 3};1H╰${'─'.repeat(w)}╯`;
    frame += `${ESC}[${top + 2};5H${ESC}[?25h${ESC}[?2026l`;
    out.push({ write: frame });
    if ((f + 1) % s.every === 0) out.push({ takeSummary: true });
  }
  return out;
}

async function runCase(c: Case): Promise<unknown> {
  const input: Step[] = [];
  for (const step of c.steps) {
    if ('synth' in step) input.push(...expandSynth(step.synth));
    else input.push(step);
  }
  const screen =
    c.scrollback === undefined
      ? new TerminalScreen(c.cols, c.rows)
      : new TerminalScreen(c.cols, c.rows, c.scrollback);
  const outputs: unknown[] = [];
  try {
    for (const step of input) {
      if ('write' in step) screen.write(step.write);
      else if ('resize' in step) screen.resize(step.resize[0], step.resize[1]);
      else if ('render' in step) outputs.push(await screen.render());
      else if ('takeSummary' in step) outputs.push(await screen.takeSummary());
      else if ('transcript' in step) {
        const t = step.transcript;
        outputs.push(await renderTranscript(t.raw, t.cols, t.rows));
      }
    }
  } finally {
    screen.dispose();
  }
  return {
    name: c.name,
    cols: c.cols,
    rows: c.rows,
    scrollback: c.scrollback ?? null,
    input,
    outputs,
  };
}

async function main(): Promise<void> {
  const here = dirname(fileURLToPath(import.meta.url));
  const corpus = JSON.parse(
    fs.readFileSync(join(here, '..', 'fixtures', 'terminal-screen.json'), 'utf8'),
  ) as { cases: Case[] };
  for (const c of corpus.cases) {
    process.stdout.write(`${JSON.stringify(await runCase(c))}\n`);
  }
  process.stdout.write(`${JSON.stringify({ scrollbackLines: SCREEN_SCROLLBACK_LINES })}\n`);
}

main().catch((err) => {
  console.error(err);
  process.exit(1);
});
