/**
 * Tier-1 oracle case — v4's `resolveDefaultSystemPrompt` / `resolveDefaultSystemPromptId`
 * (v4 `baa85e19b`, bug 154 — `lib/characters/default-system-prompt.ts`).
 *
 * Drives v4's REAL functions over the committed
 * `harness/oracle/fixtures/default-system-prompt.json` corpus and emits, per row,
 * the resolved prompt OBJECT (or null), its id (or null), and the CONTENT form.
 *
 * ⚠ The import is the DEFINING FILE, not a barrel: an entry script living in the
 * v5 repo with cwd inside the v4 worktree gets a barrel transpiled as CJS and a
 * named import off it fails outright (`tsx-oracle-cannot-named-import-a-v4-barrel`).
 * There is no barrel for this module anyway — v4's own two consumers import the
 * file path.
 *
 * ⚠ The `content` field is v4 `lib/chat/initialize.ts:146` TRANSCRIBED
 * (`resolveDefaultSystemPrompt(character)?.content ?? ''`), not driven:
 * `getDefaultSystemPrompt` is a module-private function with no export, so it has
 * no tier-1 path. Its DRIVING proof is `chat_context_init_equivalence`'s
 * `empty_content_default` case, which reaches it through `buildChatContext`.
 *
 * ⚠ PIN REQUIRED at the TARGET `baa85e19b`: the module does not exist at the
 * `89fcc3c0d` baseline, so a baseline-pinned run fails to resolve the import —
 * that failure IS the pin verification.
 *
 * Regenerate (Node 24):
 *   V5W=${V5W:-$HOME/source/quilltap-v5}
 *   N=~/.nvm/versions/node/v24.13.1/bin
 *   rm -f /tmp/oracle-default-system-prompt.ndjson
 *   cd ~/source/quilltap-server
 *   $N/npx tsx $V5W/harness/oracle/cases/default-system-prompt.ts \
 *     > /tmp/oracle-default-system-prompt.ndjson
 */

import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import { readFileSync } from 'node:fs';

import {
  resolveDefaultSystemPrompt,
  resolveDefaultSystemPromptId,
} from '@/lib/characters/default-system-prompt';

interface Case {
  id: string;
  note?: string;
  character: Record<string, unknown>;
}

function main(): void {
  const here = dirname(fileURLToPath(import.meta.url));
  const specPath = join(here, '..', 'fixtures', 'default-system-prompt.json');
  const spec = JSON.parse(readFileSync(specPath, 'utf8')) as { cases: Case[] };

  for (const c of spec.cases) {
    // The corpus deliberately carries JS-truthiness shapes the declared types
    // forbid (a numeric column, a string flag) — the resolver's own tests are
    // `if (x)` / `find(p => p.isDefault)`, so those shapes are the contract.
    const character = c.character as never;
    const prompt = resolveDefaultSystemPrompt(character);
    const id = resolveDefaultSystemPromptId(character);
    // v4 `lib/chat/initialize.ts:146`, transcribed — see the header.
    const content = (prompt as { content?: unknown } | null)?.content ?? '';

    process.stdout.write(
      JSON.stringify({ case: c.id, prompt: prompt ?? null, id: id ?? null, content }) + '\n'
    );
  }
  process.exit(0);
}

main();
