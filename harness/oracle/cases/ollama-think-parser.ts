/**
 * Oracle case: the Ollama inline-`<think>` stream parser (P4.D78, v4
 * `d9c5a1c7`).
 *
 * Drives the REAL classes from v4's
 * `plugins/dist/qtap-plugin-ollama/think-parser.ts`:
 *   ThinkTagStreamParser (push / flush / reasoning), extractThinkBlocks
 *
 * over the committed case table
 * (`harness/oracle/fixtures/ollama-think-parser/cases.json` — every row an
 * explicit `pushes: string[]`, so neither side has to agree about chop
 * arithmetic). Per row it records the visible text each `push` released and the
 * reasoning accumulated after it, the `flush` release, and the one-shot
 * `extractThinkBlocks` over the concatenated input.
 *
 * Run from inside the server checkout (the import is a plugin path, not lib/):
 *   cd ~/source/quilltap-server
 *   npx tsx ~/source/quilltap-v5/harness/oracle/cases/ollama-think-parser.ts \
 *     ~/source/quilltap-v5/harness/oracle/fixtures/ollama-think-parser/cases.json \
 *     > /tmp/oracle-ollama-think-parser.ndjson
 */

import { readFileSync } from 'node:fs';

import {
  ThinkTagStreamParser,
  extractThinkBlocks,
} from '@/plugins/dist/qtap-plugin-ollama/think-parser';

interface Case {
  id: string;
  pushes: string[];
}

interface Step {
  visible: string;
  reasoning: string;
}

interface Row {
  id: string;
  steps: Step[];
  flush: Step;
  oneShot: { content: string; reasoning: string };
}

const casesPath = process.argv[2];
if (!casesPath) throw new Error('usage: ollama-think-parser.ts <cases.json>');
const cases: Case[] = JSON.parse(readFileSync(casesPath, 'utf8'));

for (const c of cases) {
  const parser = new ThinkTagStreamParser();
  const steps: Step[] = c.pushes.map((delta) => {
    const visible = parser.push(delta);
    return { visible, reasoning: parser.reasoning };
  });
  const flushVisible = parser.flush();
  const row: Row = {
    id: c.id,
    steps,
    flush: { visible: flushVisible, reasoning: parser.reasoning },
    oneShot: extractThinkBlocks(c.pushes.join('')),
  };
  process.stdout.write(JSON.stringify(row) + '\n');
}
