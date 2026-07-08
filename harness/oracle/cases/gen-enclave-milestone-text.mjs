/**
 * Mechanical extractor for the autonomous-room milestone/grace text (U4.1).
 * Emits the byte-exact `enclave/milestones/milestone_text.rs` generated
 * submodule (the generated-text precedent: `gen-brahma-prompts.mjs`,
 * `memory_tasks/prompt_text.rs`, the image scene tasks).
 *
 * Unlike the gen-brahma precedent, the source constants are module-PRIVATE in
 * v4 (`lib/background-jobs/handlers/autonomous-room-turn.ts` exports only the
 * budget functions), so nothing can be imported — this script extracts the
 * constants from the SOURCE TEXT and evaluates them as JS, so the emitted
 * bytes are exactly what V8 produces from v4's own literals:
 *
 *   - `GRACE_CONTENT` / `GRACE_OPAQUE` — the grace-round bodies (single
 *     string literals, evaluated verbatim).
 *   - The `MILESTONE_BINDING_PHRASE` table — the object literal is evaluated
 *     whole; each binding's hostHalf/hostEnd/opaqueNoun/pauses is emitted as
 *     a const.
 *   - The milestone bit constants + thresholds — emitted as `SRC_*` consts a
 *     unit test asserts against the hand-doc'd Rust constants.
 *   - `COMPOSED_MILESTONE_MESSAGES` — every (binding × milestone) cell fully
 *     composed by evaluating v4's OWN template literals (extracted from the
 *     `buildMilestoneMessage` body) against the extracted phrase objects, so
 *     the Rust composition in `enclave::milestones::build_milestone_message`
 *     is unit-tested against bytes v4's templates actually produce.
 *
 * Run under tsx from the v4 checkout (no imports are needed, but keep the
 * invocation uniform with the other generators):
 *   V5=~/source/quilltap-v5
 *   cd ~/source/quilltap-server
 *   npx tsx $V5/harness/oracle/cases/gen-enclave-milestone-text.mjs \
 *     $V5/crates/quilltap-core/src/enclave/milestones/milestone_text.rs \
 *     [path/to/autonomous-room-turn.ts]
 */
import * as fs from 'node:fs';

const OUT = process.argv[2];
const SRC_PATH = process.argv[3] ?? 'lib/background-jobs/handlers/autonomous-room-turn.ts';
if (!OUT) {
  console.error('usage: tsx gen-enclave-milestone-text.mjs <out.rs> [autonomous-room-turn.ts]');
  process.exit(1);
}
const src = fs.readFileSync(SRC_PATH, 'utf8');

function fail(msg) {
  console.error(`gen-enclave-milestone-text: ${msg}`);
  process.exit(1);
}

// --- extract a `const NAME =\n  '<literal>';` string constant, evaluated as JS.
function extractStringConst(name) {
  const m = src.match(new RegExp(`const ${name} =\\s*('(?:[^'\\\\]|\\\\.)*');`));
  if (!m) fail(`cannot find string const ${name}`);
  return new Function(`return ${m[1]};`)();
}
const GRACE_CONTENT = extractStringConst('GRACE_CONTENT');
const GRACE_OPAQUE = extractStringConst('GRACE_OPAQUE');

// --- extract a `const NAME = <number>;` (trailing comments tolerated).
function extractNumberConst(name) {
  const m = src.match(new RegExp(`const ${name} = ([0-9.]+);`));
  if (!m) fail(`cannot find numeric const ${name}`);
  return m[1]; // keep the source spelling ("1", "0.9") for exact literals
}
const bits = {
  MILESTONE_HALFWAY: extractNumberConst('MILESTONE_HALFWAY'),
  MILESTONE_NEAR_END: extractNumberConst('MILESTONE_NEAR_END'),
  MILESTONE_GRACE: extractNumberConst('MILESTONE_GRACE'),
};
const thresholds = {
  HALFWAY_THRESHOLD: extractNumberConst('HALFWAY_THRESHOLD'),
  NEAR_END_THRESHOLD: extractNumberConst('NEAR_END_THRESHOLD'),
};

// --- extract the MILESTONE_BINDING_PHRASE object literal (the TS type
// annotation between the name and `=` contains no `=`; the table is the first
// `{ ... }` closing with `};` at column 0 after it) and evaluate it whole.
const tm = src.match(/const MILESTONE_BINDING_PHRASE[^=]*= (\{[\s\S]*?\n\});/);
if (!tm) fail('cannot find MILESTONE_BINDING_PHRASE table');
const table = new Function(`return ${tm[1]};`)();
const BINDINGS = Object.keys(table);
if (BINDINGS.join(',') !== 'time,turns,tokens,daily') {
  fail(`unexpected binding set/order: ${BINDINGS.join(',')}`);
}
for (const b of BINDINGS) {
  const p = table[b];
  if (typeof p.hostHalf !== 'string' || typeof p.hostEnd !== 'string'
    || typeof p.opaqueNoun !== 'string' || typeof p.pauses !== 'boolean') {
    fail(`binding '${b}' has an unexpected shape`);
  }
}

// --- extract buildMilestoneMessage's six template literals, in source order:
//   [0] halfway content, [1] halfway opaque,
//   [2] pauses near-end content, [3] pauses near-end opaque,
//   [4] default near-end content, [5] default near-end opaque.
const fnm = src.match(/function buildMilestoneMessage\([\s\S]*?\n\}/);
if (!fnm) fail('cannot find buildMilestoneMessage');
const body = fnm[0];
const templates = [...body.matchAll(/`(?:[^`\\]|\\.)*`/g)].map((m) => m[0]);
if (templates.length !== 6) fail(`expected 6 template literals in buildMilestoneMessage, found ${templates.length}`);

// Sanity-check the branch structure the [0..5] ordering assumes.
const order = [
  "milestone === 'halfway'",
  "'autonomous-room-halfway'",
  templates[0],
  templates[1],
  'if (phrase.pauses)',
  "'autonomous-room-nearing-end'",
  templates[2],
  templates[3],
  templates[4],
  templates[5],
];
let cursor = -1;
for (const marker of order) {
  const idx = body.indexOf(marker, cursor + 1);
  if (idx <= cursor) fail(`buildMilestoneMessage structure changed (marker out of order: ${marker.slice(0, 40)}...)`);
  cursor = idx;
}
// The second near-end branch reuses the same systemKind literal.
if ([...body.matchAll(/'autonomous-room-nearing-end'/g)].length !== 2) {
  fail('expected exactly two autonomous-room-nearing-end systemKind sites');
}
if (!src.includes("'autonomous-room-grace'")) fail('grace systemKind literal missing at the grant site');

// --- compose every (binding × milestone) cell with v4's OWN templates.
const render = (tpl, phrase) => new Function('phrase', `return ${tpl};`)(phrase);
const composed = [];
for (const binding of BINDINGS) {
  const phrase = table[binding];
  composed.push([binding, 'halfway', 'autonomous-room-halfway',
    render(templates[0], phrase), render(templates[1], phrase)]);
  if (phrase.pauses) {
    composed.push([binding, 'near-end', 'autonomous-room-nearing-end',
      render(templates[2], phrase), render(templates[3], phrase)]);
  } else {
    composed.push([binding, 'near-end', 'autonomous-room-nearing-end',
      render(templates[4], phrase), render(templates[5], phrase)]);
  }
}

// --- emit Rust. Raw strings with a hash fence that cannot collide.
function fence(bodyText) {
  let hashes = '#';
  while (bodyText.includes(`"${hashes}`) || bodyText.includes(`${hashes}"`)) hashes += '#';
  return hashes;
}
function rstr(s) {
  const h = fence(s);
  return `r${h}"${s}"${h}`;
}

let out = `//! Generated byte-exact autonomous-room milestone/grace text (U4.1; v4
//! \`lib/background-jobs/handlers/autonomous-room-turn.ts\`, module-private
//! constants — extracted mechanically from the v4 SOURCE TEXT and evaluated
//! as JS, since v4 exports none of them). Do not hand-edit. Regenerate:
//!
//!   cd ~/source/quilltap-server
//!   npx tsx ~/source/quilltap-v5/harness/oracle/cases/gen-enclave-milestone-text.mjs \\
//!     ~/source/quilltap-v5/crates/quilltap-core/src/enclave/milestones/milestone_text.rs
//!
//! \`COMPOSED_MILESTONE_MESSAGES\` holds every (binding × milestone) cell fully
//! composed by v4's OWN template literals — the no-drift check the Rust
//! composition in \`super::build_milestone_message\` is unit-tested against.

/// v4 \`GRACE_CONTENT\` (${GRACE_CONTENT.length} UTF-16 code units).
pub const GRACE_CONTENT: &str = ${rstr(GRACE_CONTENT)};

/// v4 \`GRACE_OPAQUE\` (${GRACE_OPAQUE.length} UTF-16 code units).
pub const GRACE_OPAQUE: &str = ${rstr(GRACE_OPAQUE)};

// The milestone bit constants + thresholds as spelled in the v4 source; a unit
// test asserts the hand-documented constants in \`super\` equal these.
pub const SRC_MILESTONE_HALFWAY: i64 = ${bits.MILESTONE_HALFWAY};
pub const SRC_MILESTONE_NEAR_END: i64 = ${bits.MILESTONE_NEAR_END};
pub const SRC_MILESTONE_GRACE: i64 = ${bits.MILESTONE_GRACE};
pub const SRC_HALFWAY_THRESHOLD: f64 = ${thresholds.HALFWAY_THRESHOLD};
pub const SRC_NEAR_END_THRESHOLD: f64 = ${thresholds.NEAR_END_THRESHOLD};

`;

for (const binding of BINDINGS) {
  const p = table[binding];
  const B = binding.toUpperCase();
  out += `/// v4 \`MILESTONE_BINDING_PHRASE.${binding}.hostHalf\`.
pub const ${B}_HOST_HALF: &str = ${rstr(p.hostHalf)};
/// v4 \`MILESTONE_BINDING_PHRASE.${binding}.hostEnd\`.
pub const ${B}_HOST_END: &str = ${rstr(p.hostEnd)};
/// v4 \`MILESTONE_BINDING_PHRASE.${binding}.opaqueNoun\`.
pub const ${B}_OPAQUE_NOUN: &str = ${rstr(p.opaqueNoun)};
/// v4 \`MILESTONE_BINDING_PHRASE.${binding}.pauses\`.
pub const ${B}_PAUSES: bool = ${p.pauses};

`;
}

out += `/// Every fully-composed (binding, milestone, systemKind, content,
/// opaqueContent) cell, evaluated from v4's own \`buildMilestoneMessage\`
/// template literals by the generator.
pub const COMPOSED_MILESTONE_MESSAGES: &[(&str, &str, &str, &str, &str)] = &[
`;
for (const [binding, milestone, kind, content, opaque] of composed) {
  out += `    (\n        "${binding}",\n        "${milestone}",\n        "${kind}",\n        ${rstr(content)},\n        ${rstr(opaque)},\n    ),\n`;
}
out += `];\n`;

fs.mkdirSync(OUT.slice(0, OUT.lastIndexOf('/')), { recursive: true });
fs.writeFileSync(OUT, out);
console.error(
  `wrote ${OUT}: grace=${GRACE_CONTENT.length}/${GRACE_OPAQUE.length} bindings=${BINDINGS.length} composed=${composed.length}`,
);
