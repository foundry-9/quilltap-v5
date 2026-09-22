import { brotliCompressSync, constants } from 'zlib';
// Builds the COMMITTED corpus for the P4.D203 tier-1 codec family.
// Deterministic: no randomness, no clock. `v5Blobs` is filled by the Rust
// regenerator (harness test `regenerate_v5_blobs`), not here.
const FLOOR = 512;
const long = (s) => s.repeat(Math.ceil((FLOOR * 4) / s.length));
let seed = 0x1a2b3c4d;
const rnd = () => { seed ^= seed << 13; seed >>>= 0; seed ^= seed >>> 17; seed ^= seed << 5; seed >>>= 0; return seed; };
const B64 = 'ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/';
const pseudoB64 = (n) => Array.from({ length: n }, () => B64[rnd() % 64]).join('');

const texts = [];
const push = (label, text) => texts.push({ label, text });

// --- v4's own test strings (__tests__/unit/lib/database/text-compression.test.ts)
push('ascii prose', long('The djinn of Istanbul waited by the bosphorus. '));
push('unicode + emoji', long('café — naïve — 日本語 — 🜁🜂🜃🜄 — Ω≈ç√∫ '));
push('json', long('{"role":"assistant","content":"well, quite."} '));
push('markdown with newlines', long('## Interchange 4\n\n*He bowed.*\n\n'));
push('repeated whitespace', long('   \t  \n   '));
push('shrinks substantially', long('the same clause over and over again, endlessly. '));
push('legacy short string', 'already text');
push('empty string', '');

// --- the floor, measured in BYTES
push('511 ascii bytes', 'x'.repeat(FLOOR - 1));
push('512 ascii bytes (exactly the floor)', 'x'.repeat(FLOOR));
push('513 ascii bytes', 'x'.repeat(FLOOR + 1));
// 127 four-byte emoji = 508 bytes: UNDER the floor despite 254 UTF-16 units.
push('508 bytes of 4-byte emoji (under)', '🜁'.repeat(127));
// 128 four-byte emoji = 512 bytes: AT the floor.
push('512 bytes of 4-byte emoji (at)', '🜁'.repeat(128));
// v4's own "measures the floor in BYTES" case.
push('800 bytes of 4-byte emoji', '🜁'.repeat(200));
// 511 bytes ending mid-multibyte-boundary-adjacent: 169 3-byte + 4 ascii.
push('511 bytes of 3-byte chars + ascii', '日'.repeat(169) + 'abcd');
push('513 bytes of 2-byte chars + ascii', 'é'.repeat(256) + 'a');

// --- incompressible: the `total >= raw.length` arm
push('pseudo-base64 5464', pseudoB64(5464));
push('pseudo-base64 520 (near floor)', pseudoB64(520));
push('pseudo-base64 512 (at floor)', pseudoB64(512));
push('printable cycle 2000', Array.from({ length: 2000 }, (_, i) => String.fromCharCode(32 + ((i * 7919) % 95))).join(''));

// --- real-shaped transcript prose, the common production case
const clauses = [
  'The Lantern hissed once and settled into its brass cradle, throwing a coin of light across the ledger.',
  'She considered the proposition the way one considers a second helping of trifle: with appetite, and a little shame.',
  '"I should not like to be quoted," he said, "but the Concierge has been at the sherry again."',
  'Outside, the airships grumbled over the rooftops, dragging their shadows like reluctant dogs.',
  'Prospero had a theory about this, and Prospero always had a theory, which was rather the trouble.',
  'The Commonplace Book fell open at a page nobody remembered writing.',
  'Aurora adjusted her spectacles, and in doing so adjusted the entire moral weight of the room.',
  'It was the sort of afternoon that made a person believe in gaslight and disbelieve in Tuesdays.',
  'Carina counted the tokens twice, then a third time, on the principle that arithmetic improves under supervision.',
  'The Scriptorium smelled of hot metal, cold tea and the particular dust that settles only on unread things.',
];
for (let i = 0; i < 12; i++) {
  const n = 4 + (i % 9);
  const parts = [];
  for (let j = 0; j < n; j++) parts.push(clauses[(i * 3 + j) % clauses.length]);
  push(`transcript prose ${i}`, parts.join(' ') + `\n\n— interchange ${i + 1}\n`);
}
// An `llm_logs.response`-shaped JSON payload over the floor.
push('llm_logs response shape', JSON.stringify({
  error: null,
  choices: [{ index: 0, message: { role: 'assistant', content: clauses.join(' ') } }],
  usage: { prompt_tokens: 812, completion_tokens: 344, total_tokens: 1156 },
}));

// --- LARGE real-shaped prose: the multi-metablock encodes.
// Brotli emits a fresh meta-block roughly every 16 MiB of input at q5, but the
// encoder's *window* fills long before that: past ~64 KiB the back-reference
// distances stop being representable in the short-distance codes and the
// stream starts switching Huffman contexts mid-file. Everything measured
// before these two rows was <= 5,464 bytes, so parity was pinned only over a
// single small block. These two rows carry the measurement up to a quarter of
// a megabyte of genuinely compressible text — random bytes would prove nothing
// here, because an incompressible input takes the `total >= raw.length` arm and
// never reaches the encoder's block machinery at all.
let proseSeed = 0x2f6d1c07;
const prnd = () => {
  proseSeed ^= proseSeed << 13; proseSeed >>>= 0;
  proseSeed ^= proseSeed >>> 17;
  proseSeed ^= proseSeed << 5; proseSeed >>>= 0;
  return proseSeed;
};
const WORDS = [
  'brass', 'lantern', 'ledger', 'airship', 'sherry', 'gaslight', 'orrery',
  'trifle', 'spectacles', 'rooftop', 'shadow', 'appetite', 'proposition',
  'afternoon', 'arithmetic', 'supervision', 'scriptorium', 'commonplace',
  'interchange', 'cradle', 'coin', 'dust', 'metal', 'tea', 'theory',
  'trouble', 'page', 'room', 'weight', 'principle', 'reluctant', 'particular',
  'unread', 'hissed', 'settled', 'considered', 'adjusted', 'counted',
  'grumbled', 'dragged', 'remembered', 'believed', 'disbelieved', 'quoted',
];
/** Deterministic prose of at least `bytes` UTF-8 bytes, in sentences. */
const prose = (bytes) => {
  const out = [];
  let size = 0;
  let sentences = 0;
  while (size < bytes) {
    const n = 6 + (prnd() % 14);
    const words = [];
    for (let i = 0; i < n; i++) words.push(WORDS[prnd() % WORDS.length]);
    let s = words.join(' ');
    s = s[0].toUpperCase() + s.slice(1) + (prnd() % 8 === 0 ? '?' : '.');
    sentences += 1;
    if (sentences % 9 === 0) s += '\n\n';
    out.push(s);
    size += s.length + 1;
  }
  return out.join(' ');
};
push('real-shaped prose ~64 KiB', prose(64 * 1024));
push('real-shaped prose ~256 KiB', prose(256 * 1024));

// --- the decode-only shapes blobToText must tolerate (kind drives the oracle)
const decodeShapes = [
  { label: 'null', kind: 'null' },
  { label: 'undefined', kind: 'undefined' },
  { label: 'plain string', kind: 'string', text: 'already text' },
  { label: 'empty string', kind: 'string', text: '' },
  { label: 'uncompressed buffer as UTF-8', kind: 'blob', hex: Buffer.from('legacy plaintext row', 'utf-8').toString('hex') },
  { label: 'uncompressed buffer, multi-byte', kind: 'blob', hex: Buffer.from('café — 日本語 — 🜁', 'utf-8').toString('hex') },
  { label: 'empty buffer', kind: 'blob', hex: '' },
  { label: 'two-byte buffer (too short for a header)', kind: 'blob', hex: '5101' },
  { label: 'embedding magic blob (0xEB)', kind: 'blob', hex: 'eb010100000000' },
  { label: 'header-only, empty payload', kind: 'blob', hex: '510101' },
  { label: 'number, integer', kind: 'number', value: 42 },
  { label: 'number, real', kind: 'number', value: 1.5 },
];
// Truncated / unknown-header variants over a REAL brotli payload, so the
// corrupt-payload fallback and the "not mine" header checks are exercised on
// genuine bytes rather than on noise. Built with Node's own brotli at the
// codec's parameters — the SAME encoder v4's textToBlob uses.
const sample = long('some compressible text ');
const rawSample = Buffer.from(sample, 'utf-8');
const payload = brotliCompressSync(rawSample, {
  params: {
    [constants.BROTLI_PARAM_QUALITY]: 5,
    [constants.BROTLI_PARAM_SIZE_HINT]: rawSample.length,
  },
});
const full = Buffer.concat([Buffer.from([0x51, 0x01, 0x01]), payload]);
decodeShapes.push({ label: 'a well-formed compressed blob', kind: 'blob', hex: full.toString('hex') });
decodeShapes.push({ label: 'truncated to 8 bytes', kind: 'blob', hex: full.subarray(0, 8).toString('hex') });
decodeShapes.push({ label: 'truncated to half', kind: 'blob', hex: full.subarray(0, Math.floor(full.length / 2)).toString('hex') });
const futureVersion = Buffer.from(full); futureVersion[1] = 0x02;
decodeShapes.push({ label: 'unknown version byte', kind: 'blob', hex: futureVersion.toString('hex') });
const futureCodec = Buffer.from(full); futureCodec[2] = 0x02;
decodeShapes.push({ label: 'unknown codec byte', kind: 'blob', hex: futureCodec.toString('hex') });
const wrongMagic = Buffer.from(full); wrongMagic[0] = 0x50;
decodeShapes.push({ label: 'wrong magic byte', kind: 'blob', hex: wrongMagic.toString('hex') });

process.stdout.write(JSON.stringify({ texts, decodeShapes, v5Blobs: [] }, null, 2) + '\n');
