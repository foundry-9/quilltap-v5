/**
 * Tier-1 oracle case — the vault frontmatter READ parsers.
 *
 * Drives v4's REAL parsePromptFile + parseScenarioFile
 * (lib/database/repositories/vault-overlay/parsers.ts) over a corpus of synthetic
 * DocMountDocument-like inputs, emitting the parsed object or null. The Rust port
 * (quilltap_core::vault_overlay::{parse_prompt_file, parse_scenario_file}) must
 * match exactly — the frontmatter `name`/`isDefault`/`description` reads, the
 * body slice + trimStart, the `# heading` / filename title fallbacks, the
 * UTF-16 `.trim().slice(0, n)` caps, and the stableUuidFromString ids.
 *
 * Run from the v4 server checkout:
 *   cd ~/source/quilltap-server
 *   npx tsx ~/source/quilltap-v5/harness/oracle/cases/vault-frontmatter-parsers.ts \
 *     > /tmp/oracle-vault-frontmatter-parsers.ndjson
 */

import {
  parsePromptFile,
  parseScenarioFile,
} from '@/lib/database/repositories/vault-overlay/parsers';

type Doc = {
  content: string;
  mountPointId: string;
  relativePath: string;
  fileName: string;
  createdAt: string;
  updatedAt: string;
};

type Row =
  | { kind: 'prompt'; id: string; doc: Doc; out: unknown }
  | { kind: 'scenario'; id: string; doc: Doc; out: unknown };

const rows: Row[] = [];
const CID = 'char-1';
const T1 = '2024-01-01T00:00:00.000Z';
const T2 = '2024-01-02T00:00:00.000Z';

function doc(content: string, relativePath: string, fileName: string): Doc {
  return { content, mountPointId: 'mp-1', relativePath, fileName, createdAt: T1, updatedAt: T2 };
}

function promptCase(id: string, content: string, rp = `Prompts/${id}.md`) {
  const d = doc(content, rp, rp.split('/').pop()!);
  rows.push({ kind: 'prompt', id, doc: d, out: parsePromptFile(d as never, CID) ?? null });
}
function scenarioCase(id: string, content: string, fileName = `${id}.md`) {
  const d = doc(content, `Scenarios/${fileName}`, fileName);
  rows.push({ kind: 'scenario', id, doc: d, out: parseScenarioFile(d as never, CID) ?? null });
}

// ── parsePromptFile ──────────────────────────────────────────────────────────
promptCase('p-full', '---\nname: Hero\nisDefault: true\n---\nThe prompt body');
promptCase('p-default-false', '---\nname: Hero\n---\nBody');
promptCase('p-name-trim', '---\nname: "  Hero  "\n---\nBody');
promptCase('p-name-long', `---\nname: ${'x'.repeat(150)}\n---\nBody`);
promptCase('p-no-fm', 'No frontmatter here');
promptCase('p-no-name', '---\nisDefault: true\n---\nBody');
promptCase('p-empty-name', '---\nname: "   "\n---\nBody');
promptCase('p-name-nonstring', '---\nname: 42\n---\nBody');
promptCase('p-empty-body', '---\nname: Hero\n---\n');
promptCase('p-body-all-ws', '---\nname: Hero\n---\n   \n  ');
promptCase('p-body-leading-ws', '---\nname: Hero\n---\n\n\n  Actual body');
promptCase('p-isdefault-string', '---\nname: Hero\nisDefault: "true"\n---\nBody');
promptCase('p-multibyte', '---\nname: café\n---\nBödy text');

// ── parseScenarioFile ────────────────────────────────────────────────────────
scenarioCase('s-fm-full', '---\nname: Quest\ndescription: A quest\n---\nGo forth');
scenarioCase('s-fm-name-only', '---\nname: Quest\n---\nBody');
scenarioCase('s-desc-long', `---\nname: Q\ndescription: ${'d'.repeat(600)}\n---\nBody`);
scenarioCase('s-desc-ws', '---\nname: Q\ndescription: "   "\n---\nBody');
scenarioCase('s-heading', '# The Heading\n\nBody text');
scenarioCase('s-heading-trailing-space', '#   Spaced Title   \n\nbody');
scenarioCase('s-filename-fallback', 'Just body no heading', 'my-scenario.md');
scenarioCase('s-filename-empty', 'Body text', '.md');
scenarioCase('s-empty-body-fm', '---\nname: Q\n---\n');
scenarioCase('s-empty-body-heading', '# Title\n');
scenarioCase('s-fm-and-heading', '---\nname: FMName\n---\n# Inner Heading\n\nbody');
scenarioCase('s-no-ext-filename', 'plain body', 'plain');
scenarioCase('s-multibyte', '# Café Tïtle\n\nBödy');

for (const row of rows) {
  process.stdout.write(JSON.stringify(row) + '\n');
}
