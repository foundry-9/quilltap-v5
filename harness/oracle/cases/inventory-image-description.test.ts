/**
 * @jest-environment node
 *
 * Tier-1 ORACLE for the Inventory's image phrase and where it lands in the
 * pictures (P4.D326 item 16; v4 `4fbdad186` + `d88c1ce39` at `322a5488e`),
 * ported to `quilltap_core::inventory::image_description`,
 * `services::appearance_resolution::scene_description_parts`,
 * `tools::generate_image::build_expansion_context` and
 * `services::avatar_prompt::build_character_avatar_prompt`.
 *
 * Imports v4's REAL `renderCarryingPhrase`, `sceneDescriptionParts`,
 * `buildExpansionContext` and `buildCharacterAvatarPrompt`. No seam is mocked
 * beyond `jest.setup`'s: the avatar builder is called with NO `equippedSlots`,
 * so it reads no repository (`{}` stands in, as v4's own test does).
 *
 * Emits one NDJSON line per corpus row: `{kind:"phrase", label, phrase}`,
 * `{kind:"scene", label, parts}`, `{kind:"expansion", label, clothing}` (the
 * placeholder's `clothing` key — absent when v4 omits it), `{kind:"avatar",
 * label, prompt, hasAppearance}`.
 *
 * Run (Node 24, from the v4 checkout — a /tmp mirror, jest ignores .claude/):
 *   N=~/.nvm/versions/node/v24.13.1/bin ; V5W=${V5W:-$HOME/source/quilltap-v5}
 *   TMPO=/tmp/qt-invimg-oracle
 *   rm -rf "$TMPO"; mkdir -p "$TMPO/cases" "$TMPO/fixtures"
 *   cp "$V5W/harness/oracle/cases/inventory-image-description.test.ts" "$TMPO/cases/"
 *   cp "$V5W/harness/oracle/fixtures/inventory-image-description.json" "$TMPO/fixtures/"
 *   cd ~/source/quilltap-server
 *   QT_ORACLE_OUT=/tmp/oracle-inventory-image-description.ndjson \
 *     $N/npx jest --silent --watchman=false --testTimeout=180000 \
 *       --roots "$PWD" --roots "$TMPO/cases" -- "cases/inventory-image-description\.test\.ts$"
 */

import * as fs from 'fs';
import { join } from 'node:path';

import { renderCarryingPhrase } from '@/lib/inventory/image-description';
import { sceneDescriptionParts } from '@/lib/image-gen/appearance-resolution';
import { buildExpansionContext } from '@/lib/image-gen/prompt-expansion';
import { buildCharacterAvatarPrompt } from '@/lib/image-gen/avatar-prompt';

test('inventory-image-description tier-1 oracle', async () => {
  const spec = JSON.parse(
    fs.readFileSync(join(__dirname, '..', 'fixtures', 'inventory-image-description.json'), 'utf8')
  ) as {
    phrases: Array<{ label: string; items: unknown[] }>;
    scene: Array<{
      label: string;
      physicalDescription: string;
      clothingDescription: string;
      carryingDescription?: string;
      genderPrefix: string;
    }>;
    expansion: Array<{ label: string; appearance: Record<string, unknown> }>;
    avatar: Array<{ label: string; character: Record<string, unknown>; carrying: string | null }>;
  };
  const outPath = process.env.QT_ORACLE_OUT;
  if (!outPath) throw new Error('QT_ORACLE_OUT must point at the NDJSON file to write');

  const lines: string[] = [];
  for (const p of spec.phrases) {
    lines.push(
      JSON.stringify({ kind: 'phrase', label: p.label, phrase: renderCarryingPhrase(p.items as never) })
    );
  }
  for (const s of spec.scene) {
    const { label, genderPrefix, ...resolved } = s;
    lines.push(
      JSON.stringify({ kind: 'scene', label, parts: sceneDescriptionParts(resolved as never, genderPrefix) })
    );
  }
  for (const e of spec.expansion) {
    const a = e.appearance as { characterId: string; characterName: string };
    const placeholder = `{{${a.characterName}}}`;
    const context = buildExpansionContext(
      `Paint ${placeholder}`,
      [{ placeholder, name: a.characterName, entityId: a.characterId, descriptions: [] } as never],
      'OPENAI',
      [e.appearance as never]
    );
    const ph = context.placeholders[0] as { clothing?: unknown };
    lines.push(
      JSON.stringify({
        kind: 'expansion',
        label: e.label,
        hasClothing: Object.prototype.hasOwnProperty.call(ph, 'clothing'),
        clothing: ph.clothing ?? null,
      })
    );
  }
  for (const v of spec.avatar) {
    const out = await buildCharacterAvatarPrompt({} as never, v.character as never, {
      carrying: v.carrying,
    });
    lines.push(
      JSON.stringify({
        kind: 'avatar',
        label: v.label,
        prompt: out.prompt,
        hasAppearance: out.hasAppearance,
      })
    );
  }

  fs.writeFileSync(outPath, lines.join('\n') + '\n');
  // eslint-disable-next-line no-console
  console.log(`inventory-image-description oracle wrote ${outPath} (${lines.length} lines)`);
});
