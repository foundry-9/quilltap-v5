/**
 * Generator: tool-definition catalog data module (wave 4 / W4.1b.3).
 *
 * Reads the NDJSON dump produced by
 *   harness/oracle/cases/tool-definitions.ts
 * (each line `{ key, name, defJson }`, where `defJson` is the byte-exact
 * `JSON.stringify({ name, description, parameters })` from v4) and writes the
 * generated Rust data module
 *   crates/quilltap-core/src/tools/definitions/data.rs
 * holding one byte-exact raw-string constant per tool.
 *
 * This is intentionally mechanical: the stored bytes ARE the v4 output, so the
 * differential proves serde's round-trip reproduces JS `JSON.stringify` rather
 * than re-implementing the Zod→JSON-Schema emitter.
 *
 * Regen recipe (after a v4 tool-definition drift):
 *   cd ~/source/quilltap-server
 *   npx tsx ~/source/quilltap-v5/harness/oracle/cases/tool-definitions.ts \
 *     > /tmp/oracle-tool-definitions.ndjson
 *   node ~/source/quilltap-v5/harness/oracle/tools/gen-tool-catalog.mjs \
 *     /tmp/oracle-tool-definitions.ndjson \
 *     ~/source/quilltap-v5/crates/quilltap-core/src/tools/definitions/data.rs
 *   (then re-run the differential to confirm)
 */

import { readFileSync, writeFileSync } from 'node:fs';

const [, , ndjsonPath, outPath] = process.argv;
if (!ndjsonPath || !outPath) {
  console.error('usage: gen-tool-catalog.mjs <ndjson> <out.rs>');
  process.exit(1);
}

/** Emit `s` as a Rust raw string with the minimal hash level that is safe. */
function rawString(s) {
  let n = 1;
  while (s.includes('"' + '#'.repeat(n))) n += 1;
  const h = '#'.repeat(n);
  return `r${h}"${s}"${h}`;
}

const lines = readFileSync(ndjsonPath, 'utf8').split('\n').filter((l) => l.trim().length > 0);

const rows = lines.map((l) => JSON.parse(l));

const entries = rows
  .map(
    (r) =>
      `    ToolDef {\n        key: ${JSON.stringify(r.key)},\n        name: ${JSON.stringify(
        r.name
      )},\n        json: ${rawString(r.defJson)},\n    },`
  )
  .join('\n');

const out = `//! GENERATED — do not edit by hand.
//!
//! The byte-exact tool-definition catalog (v4 \`lib/tools/*-tool.ts\`). Each
//! \`json\` is \`JSON.stringify({ name, description, parameters })\` from v4,
//! transcribed verbatim (the parameters are static data computed from static Zod
//! schemas via Zod 4's \`z.toJSONSchema\`; the faithful port is the output bytes,
//! not the emitter). Regenerate via
//! \`harness/oracle/tools/gen-tool-catalog.mjs\` — see that script's header.

use super::ToolDef;

/// Every tool definition, in v4 \`ALL_TOOLS\` order. ${rows.length} entries.
pub static TOOL_DEFINITIONS: &[ToolDef] = &[
${entries}
];
`;

writeFileSync(outPath, out);
console.error(`wrote ${rows.length} definitions to ${outPath}`);
