// Builds the committed image fixtures `normalize-blob-image/` reads
// (P4.D209 / v4 bug 159). Shipped rather than described, so the bytes can be
// rebuilt exactly (`byte-exact-static-data-transcription`): the noise is a
// fixed-seed LCG, so a rerun reproduces the same five files.
//
// The sizes are deliberate. `photo-lossless.webp` must clear
// LOSSLESS_WEBP_REENCODE_MIN_BYTES (512 KiB) or the re-encode arm is never
// taken; `icon-lossless.webp` must sit under it, or "a small lossless asset is
// left alone" is unprovable. Everything else is as small as its role allows —
// these are committed.
//
// Run from the v4 checkout (it needs the real sharp), with the script COPIED
// inside it so Node resolves `sharp` (a script in /tmp cannot):
//   N=~/.nvm/versions/node/v24.13.1/bin ; V5W=${V5W:-$HOME/source/quilltap-v5}
//   cp $V5W/harness/oracle/fixtures/build-normalize-blob-image-fixtures.mjs \
//      ~/source/quilltap-server/qt-mkfix.mjs
//   cd ~/source/quilltap-server
//   FX_OUT=$V5W/harness/oracle/fixtures/normalize-blob-image $N/node qt-mkfix.mjs
//   rm ~/source/quilltap-server/qt-mkfix.mjs
import sharp from 'sharp';
import { writeFileSync } from 'node:fs';
const OUT = process.env.FX_OUT;

// A photographic-ish gradient + noise: compresses badly losslessly (so the
// lossless WebP clears the 512 KiB floor) and well at quality 85.
const W = 620, H = 440;
const raw = Buffer.alloc(W * H * 3);
let seed = 12345;
const rnd = () => (seed = (seed * 1103515245 + 12345) & 0x7fffffff) / 0x7fffffff;
for (let y = 0; y < H; y++) {
  for (let x = 0; x < W; x++) {
    const i = (y * W + x) * 3;
    raw[i] = (x * 255 / W + rnd() * 90) & 255;
    raw[i + 1] = (y * 255 / H + rnd() * 90) & 255;
    raw[i + 2] = ((x + y) * 255 / (W + H) + rnd() * 90) & 255;
  }
}
const base = sharp(raw, { raw: { width: W, height: H, channels: 3 } });
const png = await sharp(raw, { raw: { width: W, height: H, channels: 3 } }).resize(240, 170).png().toBuffer();
const losslessWebp = await base.clone().webp({ lossless: true }).toBuffer();
const lossyWebp = await sharp(raw, { raw: { width: W, height: H, channels: 3 } }).resize(200, 142).webp({ quality: 70 }).toBuffer();
// A SMALL lossless WebP — under the floor, so it must be left alone.
const smallLossless = await sharp({
  create: { width: 40, height: 40, channels: 3, background: { r: 10, g: 200, b: 40 } },
}).webp({ lossless: true }).toBuffer();

writeFileSync(`${OUT}/photo.png`, png);
writeFileSync(`${OUT}/photo-lossless.webp`, losslessWebp);
writeFileSync(`${OUT}/photo-lossy.webp`, lossyWebp);
writeFileSync(`${OUT}/icon-lossless.webp`, smallLossless);
writeFileSync(`${OUT}/notes.txt`, Buffer.from('not an image at all, just prose.\n'));
console.log('png', png.length, 'lossless', losslessWebp.length,
            'lossy', lossyWebp.length, 'small-lossless', smallLossless.length);
