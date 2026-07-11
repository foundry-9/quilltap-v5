/**
 * Tier-1 oracle case — the SillyTavern PNG codec (v4
 * `lib/sillytavern/character.ts`: `createSTCharacterPNG` / `parseSTCharacterPNG`,
 * which internally exercise `exportSTCharacter` + `generatePlaceholderPNG` +
 * `calculateCRC32`).
 *
 * Drives v4's REAL functions over a fixed corpus and emits one NDJSON row per
 * case. The Rust port
 * (`quilltap_core::services::sillytavern::{create_st_character_png,
 * parse_st_character_png}`) must:
 *   - ENCODE / real-avatar leg: produce the byte-identical PNG (pure chunk math
 *     + the same insertion-ordered `JSON.stringify`).
 *   - ENCODE / placeholder leg: produce a PNG whose tEXt + IHDR chunks are
 *     byte-identical and whose IDAT DECODES to identical raw scanlines (the
 *     declared DEFLATE seam — v4 zlib-compresses, the port stores).
 *   - DECODE leg: return the byte-identical parsed card (or null).
 *
 * Run from the v4 server checkout:
 *   cd ~/source/quilltap-server
 *   npx tsx ~/source/quilltap-v5/harness/oracle/cases/st-png.ts \
 *     > /tmp/oracle-st-png.ndjson
 */

import {
  createSTCharacterPNG,
  parseSTCharacterPNG,
} from '@/lib/sillytavern/character';

// A committed 1×1 red PNG used as the fixed avatar for the byte-exact encode
// legs (its IHDR is a standard 13 bytes → the tEXt insertion offset is 33).
const AVATAR_PNG = Buffer.from([
  0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a, 0x00, 0x00, 0x00, 0x0d, 0x49, 0x48, 0x44, 0x52,
  0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x08, 0x02, 0x00, 0x00, 0x00, 0x90, 0x77, 0x53,
  0xde, 0x00, 0x00, 0x00, 0x0c, 0x49, 0x44, 0x41, 0x54, 0x78, 0x9c, 0x63, 0xf8, 0xcf, 0xc0, 0x00,
  0x00, 0x03, 0x01, 0x01, 0x00, 0xc9, 0xfe, 0x92, 0xef, 0x00, 0x00, 0x00, 0x00, 0x49, 0x45, 0x4e,
  0x44, 0xae, 0x42, 0x60, 0x82,
]);

// --- The character corpus (drives exportSTCharacter inside createSTCharacterPNG) ---

const richCharacter = {
  name: 'Aria Voss',
  title: 'Sky-Captain',
  description: 'A daring sky-captain of the upper aetherways.',
  personality: 'Bold, curious, wry.',
  firstMessage: 'Well then — climb aboard, and mind the ballast.',
  exampleDialogues: 'U: Hello\nA: Ahoy.',
  scenarios: [{ title: 'Default', content: 'A storm looms over the docks.' }],
  systemPrompts: [
    { name: 'Backup', isDefault: false, content: 'Alt prompt.' },
    { name: 'Explorer', isDefault: true, content: 'You are Aria, a sky-captain.' },
  ],
  // A rich passthrough object that becomes the export base.
  sillyTavernData: {
    name: 'Aria Voss',
    description: 'original description',
    personality: 'original personality',
    scenario: 'original scenario',
    first_mes: 'original greeting',
    mes_example: 'original examples',
    creator_notes: 'penned by the Scriptorium',
    tags: ['airship', 'adventure'],
    creator: 'Quilltap',
    character_version: '2.1',
    extensions: { depth_prompt: { prompt: 'stay in character', depth: 4 } },
    alternate_greetings: ['A second hello.'],
  },
};

const defaultCharacter = {
  name: 'Bartholomew',
  description: 'A meticulous clockwork butler.',
  personality: 'Impeccably polite.',
  firstMessage: 'You rang?',
  exampleDialogues: '',
  scenarios: [
    { title: 'Morning', content: 'Tea is served at dawn.' },
    { title: 'Evening', content: 'The lamps are lit at dusk.' },
  ],
  systemPrompts: [],
  // no sillyTavernData → the default baseData path.
};

const unicodeCharacter = {
  name: 'Élodie ✨',
  title: 'La Rêveuse',
  description: 'Une héroïne de café — with “smart quotes” & an em—dash.',
  personality: 'Rêveuse et curieuse. 日本語も少し。',
  firstMessage: 'Bonjour ! 🌙',
  exampleDialogues: 'U: Salut\nÉ: Enchantée 😊',
  scenarios: [{ title: 'Default', content: 'Un soir à Montmartre.' }],
  systemPrompts: [{ name: 'Default', isDefault: true, content: 'Tu es Élodie.' }],
};

// --- Emit encode rows (real-avatar byte-exact + placeholder decoded) ---

async function emitEncode(
  id: string,
  character: unknown,
  mode: 'exact' | 'placeholder'
) {
  const avatar = mode === 'exact' ? AVATAR_PNG : undefined;
  const png = await createSTCharacterPNG(character, avatar);
  process.stdout.write(
    JSON.stringify({
      id,
      kind: 'encode',
      mode,
      character,
      avatar: avatar ? avatar.toString('base64') : null,
      out: Buffer.from(png).toString('base64'),
    }) + '\n'
  );
}

// --- Build arbitrary tEXt PNGs for the decode corpus (scaffolding — NOT the
// function under test; parseSTCharacterPNG only walks chunk framing). ---

function crc32(buf: Buffer): number {
  let crc = 0xffffffff;
  for (let i = 0; i < buf.length; i++) {
    crc ^= buf[i];
    for (let j = 0; j < 8; j++) crc = crc & 1 ? (crc >>> 1) ^ 0xedb88320 : crc >>> 1;
  }
  return (crc ^ 0xffffffff) >>> 0;
}
function pngChunk(type: string, data: Buffer): Buffer {
  const len = Buffer.alloc(4);
  len.writeUInt32BE(data.length);
  const t = Buffer.from(type, 'ascii');
  const crc = Buffer.alloc(4);
  crc.writeUInt32BE(crc32(Buffer.concat([new Uint8Array(t), new Uint8Array(data)])));
  return Buffer.concat([
    new Uint8Array(len),
    new Uint8Array(t),
    new Uint8Array(data),
    new Uint8Array(crc),
  ]);
}
const SIG = Buffer.from([137, 80, 78, 71, 13, 10, 26, 10]);
const IHDR = pngChunk(
  'IHDR',
  Buffer.from([0, 0, 0, 1, 0, 0, 0, 1, 8, 2, 0, 0, 0])
);
const IEND = pngChunk('IEND', Buffer.alloc(0));
function textPng(keyword: string, payload: string): Buffer {
  const data = Buffer.concat([
    new Uint8Array(Buffer.from(keyword, 'utf8')),
    new Uint8Array(Buffer.from([0])),
    new Uint8Array(Buffer.from(payload, 'utf8')),
  ]);
  return Buffer.concat([
    new Uint8Array(SIG),
    new Uint8Array(IHDR),
    new Uint8Array(pngChunk('tEXt', data)),
    new Uint8Array(IEND),
  ]);
}

async function emitDecode(id: string, input: Buffer) {
  const parsed = await parseSTCharacterPNG(input);
  process.stdout.write(
    JSON.stringify({
      id,
      kind: 'decode',
      input: input.toString('base64'),
      out: parsed ?? null,
    }) + '\n'
  );
}

async function main() {
  // Encode — real-avatar (byte-exact).
  await emitEncode('enc-rich', richCharacter, 'exact');
  await emitEncode('enc-default', defaultCharacter, 'exact');
  await emitEncode('enc-unicode', unicodeCharacter, 'exact');
  // Encode — placeholder (decoded comparison; unicode name exercises the
  // UTF-16 name-hash).
  await emitEncode('enc-placeholder-ascii', defaultCharacter, 'placeholder');
  await emitEncode('enc-placeholder-unicode', unicodeCharacter, 'placeholder');

  // Decode — a real chara_card_v2 PNG (round-trip of the rich encode) → .data.
  const richPng = await createSTCharacterPNG(richCharacter, AVATAR_PNG);
  await emitDecode('dec-card', Buffer.from(richPng));
  // Decode — ccv2 keyword accepted; bare data with a name; the various nulls.
  await emitDecode(
    'dec-ccv2',
    textPng('ccv2', JSON.stringify({ spec: 'chara_card_v2', data: { name: 'CCV2 Kid', description: 'via ccv2' } }))
  );
  await emitDecode(
    'dec-bare',
    textPng('chara', JSON.stringify({ name: 'Bare Bones', description: 'no spec wrapper' }))
  );
  await emitDecode(
    'dec-spec-no-data',
    textPng('chara', JSON.stringify({ spec: 'chara_card_v2', name: 'Fallback Name' }))
  );
  await emitDecode('dec-bad-sig', Buffer.from('not a png at all', 'utf8'));
  await emitDecode('dec-bad-json', textPng('chara', '{not valid json'));
  await emitDecode('dec-other-keyword', textPng('Comment', 'just a comment, no card'));
  await emitDecode('dec-no-text', Buffer.concat([new Uint8Array(SIG), new Uint8Array(IHDR), new Uint8Array(IEND)]));
}

main();
