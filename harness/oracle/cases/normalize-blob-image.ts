/**
 * Tier-2 (D19-shaped) oracle case — v4's REAL `normalizeLinkBlobImage`
 * (P4.D209 / bug 159, `186eb09cb`), with REAL sharp behind it.
 *
 * **What is compared, and what deliberately is not.** v4 encodes through sharp
 * and v5 through the `image` + `webp` crates. Two different encoders cannot
 * produce byte-identical WebP, so D19 stands: the comparand is the DECISION
 * (did the bytes move?), the `storedMimeType`, the `relativePath`, the
 * `fileName`, whether the sha CHANGED, and the DECODED pixel dimensions of
 * whatever came out. Never the encoded bytes, and never the sha's value.
 *
 * The cases are v4's own six, plus the one the module doc insists on:
 *   - a PNG drags mime, path, name and hash along with the bytes;
 *   - a LARGE lossless WebP is re-encoded and NOT renamed (it is already
 *     `.webp`);
 *   - a SMALL lossless WebP is left alone — below the 512 KiB floor;
 *   - a lossy WebP is left alone — lossy→lossy is generation loss;
 *   - non-image bytes are left alone;
 *   - `normalizeImages: false` is honoured;
 *   - an OMITTED flag normalizes, because the default is `true` and a skip
 *     would be the silent reintroduction of the whole defect;
 *   - (P4.108) seven animated-input rows: two v4 keeps animated and v5
 *     DECLINES (a ruled divergence, pinned both ways on the Rust side), and
 *     five that must match (a WebP passthrough, a one-ANMF still, two still
 *     GIFs, an APNG sharp reads as a still);
 *   - (P4.112) two inputs whose SECOND frame is corrupt (a GIF, a mislabelled
 *     WebP) — a measurement of sharp, pinned as measured on the Rust side.
 *
 * Run (Node 24, from the v4 checkout):
 *   N=~/.nvm/versions/node/v24.13.1/bin ; V5W=${V5W:-$HOME/source/quilltap-v5}
 *   cd ~/source/quilltap-server        # or a worktree pinned at the baseline
 *   QT_FIXTURE_NORMALIZE_BLOB_IMAGE=$V5W/harness/oracle/fixtures/normalize-blob-image \
 *     $N/npx tsx $V5W/harness/oracle/cases/normalize-blob-image.ts \
 *     > /tmp/oracle-normalize-blob-image.ndjson
 */

import { join } from 'node:path';
import { readFileSync, existsSync } from 'node:fs';
import { createHash } from 'node:crypto';
import { createRequire } from 'node:module';

interface CaseSpec {
  name: string;
  file: string;
  relativePath: string;
  fileName: string;
  storedMimeType: string;
  normalizeImages?: boolean;
  omitFlag?: boolean;
}

const CASES: CaseSpec[] = [
  {
    name: 'png_drags_mime_path_name_hash',
    file: 'photo.png',
    relativePath: 'art/photo.png',
    fileName: 'photo.png',
    storedMimeType: 'image/png',
    normalizeImages: true,
  },
  {
    name: 'large_lossless_reencoded_not_renamed',
    file: 'photo-lossless.webp',
    relativePath: 'art/plate.webp',
    fileName: 'plate.webp',
    storedMimeType: 'image/webp',
    normalizeImages: true,
  },
  {
    name: 'small_lossless_under_the_floor_untouched',
    file: 'icon-lossless.webp',
    relativePath: 'art/icon.webp',
    fileName: 'icon.webp',
    storedMimeType: 'image/webp',
    normalizeImages: true,
  },
  {
    name: 'lossy_untouched',
    file: 'photo-lossy.webp',
    relativePath: 'art/snap.webp',
    fileName: 'snap.webp',
    storedMimeType: 'image/webp',
    normalizeImages: true,
  },
  {
    name: 'non_image_untouched',
    file: 'notes.txt',
    relativePath: 'docs/notes.txt',
    fileName: 'notes.txt',
    storedMimeType: 'text/plain',
    normalizeImages: true,
  },
  {
    name: 'flag_false_honoured',
    file: 'photo.png',
    relativePath: 'art/photo.png',
    fileName: 'photo.png',
    storedMimeType: 'image/png',
    normalizeImages: false,
  },
  {
    name: 'omitted_flag_still_normalizes',
    file: 'photo.png',
    relativePath: 'art/photo.png',
    fileName: 'photo.png',
    storedMimeType: 'image/png',
    omitFlag: true,
  },
  {
    name: 'mime_with_parameters_and_case',
    file: 'photo.png',
    relativePath: 'art/photo.png',
    fileName: 'photo.png',
    storedMimeType: 'Image/PNG; charset=binary',
    normalizeImages: true,
  },
  {
    name: 'extensionless_path_gains_webp',
    file: 'photo.png',
    relativePath: 'art/plate',
    fileName: 'plate',
    storedMimeType: 'image/png',
    normalizeImages: true,
  },
  // P4.108 — the animated-input decline (ruled by the human, 2026-09-23).
  // v4's `transcodeToWebP` is the ONE sharp call that passes `{ animated:
  // true }`, so v4 keeps every frame of these; v5's host encoder is
  // single-frame and DECLINES them instead. The two `_declined` rows are a
  // RULED DIVERGENCE the Rust side pins in both directions; every other row
  // here must MATCH. Fixtures + their generator: `fixtures/normalize-blob-image/
  // generate.py`.
  {
    name: 'anim_gif_declined',
    file: 'anim-2frame.gif',
    relativePath: 'art/loop.gif',
    fileName: 'loop.gif',
    storedMimeType: 'image/gif',
    normalizeImages: true,
  },
  {
    // An animated WebP reaches the encoder ONLY under a mislabelled mime:
    // `image/webp` is not in the transcodable set, and the lossless check
    // reads top-level chunks only (the bitstream sits inside `ANMF`).
    name: 'anim_webp_mislabelled_declined',
    file: 'anim-2frame.webp',
    relativePath: 'art/loop.gif',
    fileName: 'loop.gif',
    storedMimeType: 'image/gif',
    normalizeImages: true,
  },
  {
    name: 'anim_webp_passthrough',
    file: 'anim-2frame.webp',
    relativePath: 'art/loop.webp',
    fileName: 'loop.webp',
    storedMimeType: 'image/webp',
    normalizeImages: true,
  },
  {
    // The VP8X animation bit with ONE `ANMF` frame: sharp reads one page,
    // so no frame is at stake and v5 must NOT decline it (frame count, not
    // the bit — the order's refinement of the ruling's wording).
    name: 'one_anmf_still',
    file: 'one-anmf.webp',
    relativePath: 'art/still.gif',
    fileName: 'still.gif',
    storedMimeType: 'image/gif',
    normalizeImages: true,
  },
  {
    name: 'still_gif_transcoded',
    file: 'still-large.gif',
    relativePath: 'art/stripes.gif',
    fileName: 'stripes.gif',
    storedMimeType: 'image/gif',
    normalizeImages: true,
  },
  {
    // A one-frame GIF behind a comment full of 0x2C (the image-descriptor
    // introducer) — a naive byte scan would count frames that are not there.
    name: 'commas_gif_transcoded',
    file: 'still-with-commas.gif',
    relativePath: 'art/commas.gif',
    fileName: 'commas.gif',
    storedMimeType: 'image/gif',
    normalizeImages: true,
  },
  {
    // sharp does not decode APNG animation (format=png, a still): v4 loses
    // the frames here too, so v5 must never decline an APNG.
    name: 'apng_still',
    file: 'anim-2frame.apng',
    relativePath: 'art/loop.png',
    fileName: 'loop.png',
    storedMimeType: 'image/png',
    normalizeImages: true,
  },
  // P4.112 — MEASURE what sharp does with a two-frame input whose SECOND
  // frame cannot decode (P4.108's recorded nit: v5's frame counter stops at
  // the first bad frame, counts 1, and encodes the first frame). The Rust
  // side pins whatever this records. Fixtures: `generate.py`.
  {
    name: 'corrupt_second_frame_gif',
    file: 'anim-corrupt2.gif',
    relativePath: 'art/broken.gif',
    fileName: 'broken.gif',
    storedMimeType: 'image/gif',
    normalizeImages: true,
  },
  {
    name: 'corrupt_second_frame_webp_mislabelled',
    file: 'anim-corrupt2.webp',
    relativePath: 'art/broken.gif',
    fileName: 'broken.gif',
    storedMimeType: 'image/gif',
    normalizeImages: true,
  },
];

async function main(): Promise<void> {
  const dir = process.env.QT_FIXTURE_NORMALIZE_BLOB_IMAGE;
  if (!dir || !existsSync(dir)) {
    throw new Error(
      'QT_FIXTURE_NORMALIZE_BLOB_IMAGE must point at harness/oracle/fixtures/normalize-blob-image'
    );
  }
  process.env.LOG_LEVEL = 'error';

  const { normalizeLinkBlobImage } = await import('@/lib/mount-index/normalize-blob-image');
  // ⚠ This case FILE lives outside the v4 checkout, so a bare `import('sharp')`
  // resolves from the file's own directory and fails. `@/lib/...` works because
  // tsconfig paths resolve through the CWD; a package name does not. Resolve it
  // from the v4 tree explicitly instead — the same shape the real-DB cases use
  // for the cipher driver.
  const requireFromV4 = createRequire(join(process.cwd(), 'package.json'));
  const sharp = requireFromV4('sharp');

  const results = [];
  for (const c of CASES) {
    const data = readFileSync(join(dir, c.file));
    const sha = createHash('sha256').update(data).digest('hex');
    const input: Record<string, unknown> = {
      relativePath: c.relativePath,
      fileName: c.fileName,
      originalMimeType: c.storedMimeType,
      storedMimeType: c.storedMimeType,
      sha256: sha,
      data,
    };
    if (!c.omitFlag) input.normalizeImages = c.normalizeImages;

    const out = (await normalizeLinkBlobImage(input as never)) as {
      relativePath: string;
      fileName: string;
      storedMimeType: string;
      sha256: string;
      data: Buffer;
    };

    // Decoded dimensions of whatever came out — the one pixel-level fact two
    // different encoders must agree on.
    let width: number | null = null;
    let height: number | null = null;
    // P4.108: the page count of what came out (`undefined` → null for a
    // single-page format). Read by the Rust side ONLY for the ruled
    // animated rows — v4 must have kept more than one frame there — and
    // kept out of the whole-row comparand.
    let pages: number | null = null;
    try {
      const meta = await sharp(out.data).metadata();
      width = meta.width ?? null;
      height = meta.height ?? null;
      pages = meta.pages ?? null;
    } catch {
      // non-image bytes: no dimensions, on both sides
    }

    results.push({
      name: c.name,
      changed: out.sha256 !== sha,
      storedMimeType: out.storedMimeType,
      relativePath: out.relativePath,
      fileName: out.fileName,
      shaChanged: out.sha256 !== sha,
      bytesGrewOrShrank:
        out.data.length === data.length ? 'same' : out.data.length < data.length ? 'smaller' : 'larger',
      width,
      height,
      pages,
    });
  }

  process.stdout.write(
    JSON.stringify({ case: 'normalize-blob-image', results }) + '\n'
  );
  process.exit(0);
}

main().catch((err) => {
  process.stderr.write(`normalize-blob-image oracle failed: ${err?.stack ?? err}\n`);
  process.exit(1);
});
