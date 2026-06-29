/**
 * Tier-1 oracle case — the vault write-projection string leaves.
 *
 * Drives five REAL v4 functions from lib/mount-index/character-vault.ts:
 *   slugifyWardrobeTitle, buildSlugByItemIdMap, sanitizeFileName,
 *   buildSystemPromptFile (which exercises the private escapeYaml),
 *   buildScenarioFile.
 *
 * One NDJSON row per case; the Rust port (quilltap_core::vault_overlay::{
 * slugify_wardrobe_title, build_slug_by_item_id_map, sanitize_file_name,
 * build_system_prompt_file, build_scenario_file}) must match exactly. No YAML
 * library, no localeCompare — these are the decision-free vault string leaves.
 *
 * Run from the v4 server checkout:
 *   cd ~/source/quilltap-server
 *   npx tsx ~/source/quilltap-v5/harness/oracle/cases/vault-string-leaves.ts \
 *     > /tmp/oracle-vault-string-leaves.ndjson
 */

import {
  slugifyWardrobeTitle,
  buildSlugByItemIdMap,
  sanitizeFileName,
  buildSystemPromptFile,
  buildScenarioFile,
} from '@/lib/mount-index/character-vault';
import type { WardrobeItem } from '@/lib/schemas/wardrobe.types';
import type { CharacterSystemPrompt, CharacterScenario } from '@/lib/schemas/character.types';

type Row =
  | { kind: 'slug'; id: string; title: string; out: string }
  | { kind: 'slugMap'; id: string; items: Array<[string, string]>; out: Array<[string, string]> }
  | { kind: 'sanitize'; id: string; name: string; out: string }
  | { kind: 'promptFile'; id: string; name: string; isDefault: boolean; content: string; out: string }
  | { kind: 'scenarioFile'; id: string; title: string; content: string; out: string };

const rows: Row[] = [];

for (const [id, title] of [
  ['s-basic', 'Pearl Earrings'],
  ['s-spaces', '  Fancy!!  Hat  '],
  ['s-caps', 'ALL CAPS'],
  ['s-dashes', '---weird---'],
  ['s-empty', ''],
  ['s-digits', '123'],
  ['s-single', 'a'],
  ['s-unicode', 'É 123 ñ'],
  ['s-punct', "Mum's & Dad's: Stuff!"],
] as Array<[string, string]>) {
  rows.push({ kind: 'slug', id, title, out: slugifyWardrobeTitle(title) });
}

function runSlugMap(items: Array<[string, string]>): Array<[string, string]> {
  const wardrobe = items.map(
    ([id, title]) => ({ id, title }) as unknown as WardrobeItem,
  );
  const map = buildSlugByItemIdMap(wardrobe);
  return Array.from(map.entries());
}
for (const { id, items } of [
  { id: 'sm-basic', items: [['1', 'Red Hat'], ['2', 'Blue Coat']] as Array<[string, string]> },
  { id: 'sm-collide', items: [['1', 'Red Hat'], ['2', 'red hat'], ['3', 'Boots']] as Array<[string, string]> },
  { id: 'sm-empty-slug', items: [['1', '!!!'], ['2', 'Scarf']] as Array<[string, string]> },
]) {
  rows.push({ kind: 'slugMap', id, items, out: runSlugMap(items) });
}

for (const [id, name] of [
  ['fn-specials', 'a/b:c*d?e"f<g>h|i\\j'],
  ['fn-spaces', '  multi   space  '],
  ['fn-empty', '   '],
  ['fn-onlybad', ':::'],
  ['fn-plain', 'Normal Name'],
  ['fn-mixed', 'My: File / Name?'],
] as Array<[string, string]>) {
  rows.push({ kind: 'sanitize', id, name, out: sanitizeFileName(name) });
}

function runPrompt(name: string, isDefault: boolean, content: string): string {
  return buildSystemPromptFile(
    { name, isDefault, content } as unknown as CharacterSystemPrompt,
  );
}
for (const c of [
  { id: 'pf-plain', name: 'Greeting', isDefault: false, content: 'Hello there.' },
  { id: 'pf-default', name: 'Greeting', isDefault: true, content: 'Hello there.' },
  { id: 'pf-colon', name: 'Time: Now', isDefault: false, content: 'Body.' },
  { id: 'pf-quote', name: 'Say "hi"', isDefault: true, content: 'Body.' },
  { id: 'pf-hash', name: 'Tag #1', isDefault: false, content: 'Body.' },
  { id: 'pf-apos', name: "It's me", isDefault: false, content: 'Body.' },
  { id: 'pf-newline', name: 'Line1\nLine2', isDefault: false, content: 'Body.' },
]) {
  rows.push({
    kind: 'promptFile',
    id: c.id,
    name: c.name,
    isDefault: c.isDefault,
    content: c.content,
    out: runPrompt(c.name, c.isDefault, c.content),
  });
}

function runScenario(title: string, content: string): string {
  return buildScenarioFile({ title, content } as unknown as CharacterScenario);
}
for (const c of [
  { id: 'sf-basic', title: 'First Meeting', content: 'They meet at dawn.' },
  { id: 'sf-empty-body', title: 'Untitled', content: '' },
]) {
  rows.push({
    kind: 'scenarioFile',
    id: c.id,
    title: c.title,
    content: c.content,
    out: runScenario(c.title, c.content),
  });
}

for (const row of rows) {
  process.stdout.write(JSON.stringify(row) + '\n');
}
