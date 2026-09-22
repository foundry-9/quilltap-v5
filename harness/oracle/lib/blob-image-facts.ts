/**
 * The D19 image comparand shared by every P4.104 family (bug 159's image half,
 * v4 `186eb09cb`): what a blob write did to a DECODABLE image, stated in the
 * terms two different encoders can agree on.
 *
 * v4 encodes through `sharp`, v5 through the `image` + `webp` crates. They can
 * never produce the same WebP bytes, so the comparand is NEVER the bytes and
 * NEVER the sha's value. It is: where the row landed (`relativePath`,
 * `fileName`), what it claims to hold (`storedMimeType`), whether the stored
 * bytes are still the input's (`shaChanged`), which way the size moved, and the
 * DECODED pixel dimensions of what is stored — so a port that "normalized" by
 * handing back a 1×1 placeholder, or a stale un-normalized PNG, cannot pass.
 *
 * `crates/quilltap-harness/tests/blob_image_facts/mod.rs` is the Rust half.
 * Keep the two in step — the field names and order are the comparand.
 *
 * Copy this file to `$TMPO/lib/` beside the staged case (jest ignores
 * `.claude/`) and import it as `../lib/blob-image-facts`.
 */

import { createHash } from 'node:crypto';

/** One stored blob row, as the case read it back (joined link + blob). */
export interface StoredBlobRow {
  relativePath: string;
  fileName: string;
  storedMimeType: string;
  data: Buffer | Uint8Array;
}

export interface BlobImageFacts {
  relativePath: string;
  fileName: string;
  storedMimeType: string;
  shaChanged: boolean;
  size: 'smaller' | 'same' | 'larger';
  width: number | null;
  height: number | null;
}

/** `sharp(buf).metadata()` width/height, or nulls when sharp cannot read it. */
export type Measure = (buf: Buffer) => Promise<{ width: number | null; height: number | null }>;

/** A `Measure` over a resolved `sharp` module. */
export function sharpMeasure(
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  sharp: any,
): Measure {
  return async (buf) => {
    try {
      const m = await sharp(buf).metadata();
      return { width: m.width ?? null, height: m.height ?? null };
    } catch {
      return { width: null, height: null };
    }
  };
}

/** The facts for each row, relative to the bytes the write was handed. */
export async function blobImageFacts(
  rows: StoredBlobRow[],
  input: Buffer,
  measure: Measure,
): Promise<BlobImageFacts[]> {
  const inputSha = createHash('sha256').update(input).digest('hex');
  const out: BlobImageFacts[] = [];
  for (const r of rows) {
    const data = Buffer.from(r.data);
    const sha = createHash('sha256').update(data).digest('hex');
    const dims = await measure(data);
    out.push({
      relativePath: r.relativePath,
      fileName: r.fileName,
      storedMimeType: r.storedMimeType,
      shaChanged: sha !== inputSha,
      size: data.length < input.length ? 'smaller' : data.length > input.length ? 'larger' : 'same',
      width: dims.width,
      height: dims.height,
    });
  }
  return out;
}

/** The joined link + blob read every family uses (`l.*` filter supplied). */
export const STORED_BLOB_SELECT =
  'SELECT l.relativePath AS relativePath, l.fileName AS fileName, ' +
  'b.storedMimeType AS storedMimeType, b.data AS data ' +
  'FROM doc_mount_file_links l JOIN doc_mount_blobs b ON b.fileId = l.fileId ';
