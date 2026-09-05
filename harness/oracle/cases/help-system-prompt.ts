/**
 * P4.9I2A tier-1 ORACLE — v4's REAL `buildHelpChatSystemPrompt`
 * (`lib/help-chat/system-prompt-builder.ts`) over the committed corpus
 * (`fixtures/help-system-prompt.json`: named option combinations). Pure — the
 * builder's imports (`processTemplate`, `buildIdentityReinforcement`,
 * `firstActiveScenarioContent`, the logger) all load under tsx.
 *
 * Emits per case: { kind: 'prompt', name, prompt }
 *
 * Run (Node 24, from the v4 checkout):
 *   N=~/.nvm/versions/node/v24.13.1/bin
 *   V5W=${V5W:-$HOME/source/quilltap-v5}
 *   cd ~/source/quilltap-server
 *   $N/node --import tsx $V5W/harness/oracle/cases/help-system-prompt.ts > /tmp/oracle-help-system-prompt.ndjson
 */

import * as fs from 'node:fs';
import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import { buildHelpChatSystemPrompt } from '@/lib/help-chat/system-prompt-builder';

interface Case {
  name: string;
  character: Record<string, unknown>;
  userCharacter?: { name: string; description: string } | null;
  pageContext?: { title: string; content: string; url: string; matchType: string } | null;
  additionalPageContexts?: Array<{ title: string; content: string; url: string; matchType: string }>;
  otherCharacterNames?: string[];
  toolInstructions?: string;
}

const here = dirname(fileURLToPath(import.meta.url));
const spec = JSON.parse(fs.readFileSync(join(here, '..', 'fixtures', 'help-system-prompt.json'), 'utf8')) as { cases: Case[] };

for (const c of spec.cases) {
  const prompt = buildHelpChatSystemPrompt({
    character: c.character as never,
    userCharacter: c.userCharacter,
    pageContext: c.pageContext as never,
    additionalPageContexts: c.additionalPageContexts as never,
    otherCharacterNames: c.otherCharacterNames,
    toolInstructions: c.toolInstructions,
  });
  process.stdout.write(JSON.stringify({ kind: 'prompt', name: c.name, prompt }) + '\n');
}
