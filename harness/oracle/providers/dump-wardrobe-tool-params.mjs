/**
 * Dump the `parameters` object v4's REAL `zodToOpenAISchema` produces for the
 * two wardrobe tools — the shapes bug 125 is about.
 *
 * Bug 125: Google's function-calling API refuses `additionalProperties`
 * anywhere inside a declaration. The TOP-LEVEL one never reaches the wire (the
 * declaration builder forwards `properties` + `required` only), but the one Zod
 * emits on an array's `items` object — the wardrobe tools' `operations` — did,
 * and 400'd every tool-enabled turn whose slate carried them. No recorded
 * google corpus row carried such a schema, which is why no differential ever
 * saw the bug (v5 dogfood finding #114 → v4 bug 125).
 *
 * Run FROM the v4 checkout ROOT (the `@/` alias resolves through its tsconfig;
 * the plugin dir's own tsconfig does not carry it), Node 24, tsx:
 *   cd <v4-pin>
 *   npx tsx <V5>/harness/oracle/providers/dump-wardrobe-tool-params.mjs \
 *     --out /tmp/wardrobe-tool-params.json
 *
 * Feed the result to `record-google-request.mjs --wardrobe-params <file>`.
 *
 * Line shape: { wear: <parameters>, takeOff: <parameters> }.
 */

import { writeFileSync } from 'node:fs';

function parseArgs() {
  const args = process.argv.slice(2);
  const out = {};
  for (let i = 0; i < args.length; i += 2) out[args[i].replace(/^--/, '')] = args[i + 1];
  return out;
}

async function main() {
  const { out } = parseArgs();
  if (!out) {
    console.error('usage: --out <json>');
    process.exit(1);
  }
  const { wardrobeWearToolInputSchema } = await import('@/lib/tools/wardrobe-wear-tool');
  const { wardrobeTakeOffToolInputSchema } = await import('@/lib/tools/wardrobe-take-off-tool');
  const { zodToOpenAISchema } = await import('@/lib/tools/zod-to-openai-schema');

  const wear = zodToOpenAISchema(wardrobeWearToolInputSchema);
  const takeOff = zodToOpenAISchema(wardrobeTakeOffToolInputSchema);

  // The premise the google corpus rows rest on — assert it here so a future v4
  // converter change is a loud failure at record time, not a silent green.
  for (const [name, params] of [['wardrobe_wear', wear], ['wardrobe_take_off', takeOff]]) {
    const items = params?.properties?.operations?.items;
    if (!items || items.additionalProperties !== false) {
      console.error(`${name}: expected operations.items.additionalProperties === false`);
      process.exit(1);
    }
  }

  writeFileSync(out, JSON.stringify({ wear, takeOff }) + '\n');
  console.error(`wrote wardrobe tool parameters → ${out}`);
}

main().catch((e) => {
  console.error(e);
  process.exit(1);
});
