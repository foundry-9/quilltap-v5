/**
 * The SCRIPTED `sharp` stand-in shared by the two bug-151 oracles (P4.D198,
 * v4 `bcd7e4852`).
 *
 * ## Why a script rather than real sharp
 *
 * v4's own `__tests__/unit/lib/files/llm-image-budget.test.ts` runs against
 * real sharp, and it is right to: the defect was entirely about measured byte
 * sizes, and a mocked encoder that returns whatever buffer it was handed cannot
 * reproduce that. But a DIFFERENTIAL is a different job. v5's pixel work goes
 * through libwebp via the `webp` crate, and D19 says the operation is ported
 * while encoded byte parity with sharp is neither required nor achievable
 * (libwebp's rate control and sharp's premultiply differ, so the same input at
 * the same quality yields different bytes on the two sides). Comparing real
 * encoders would therefore compare nothing but the encoders.
 *
 * So the two sides run the SAME script below a deterministic encoder, and what
 * the differential proves is the DECISION: the arm order, the ceiling, which
 * rungs are asked for and in what order, which encode becomes `best`, when a
 * grown encode is discarded, and what a throw does to a partial ladder. The
 * real encoders are proven separately, each against its own contract — v4's by
 * its unit test, v5's by `HostImageCodec`'s (`crates/quilltap-host/src/
 * image_codec.rs`, which asserts 683x1024 / 1024x683 exactly as v4's does).
 *
 * `quilltap_harness::ScriptedTranscoder` is the Rust half, reading the same
 * corpus rows. Keep the two in step — the byte patterns are the comparand.
 */

/** A metadata probe's answer: dimensions (either may be null), or a rejection. */
export type ScriptMetadata = { width: number | null; height: number | null } | 'throws';

/** One ladder rung's scripted outcome. */
export type ScriptStep = { len: number } | { throw: string };

export interface ShrinkScript {
  metadata: ScriptMetadata;
  /** The `Error` message for a `metadata: 'throws'` probe. */
  metadataThrowMessage?: string;
  /** What a metadata probe of a NON-original buffer answers. */
  final: { width: number | null; height: number | null };
  steps: ScriptStep[];
}

/** One resize+encode the scripted encoder was asked for. */
export interface RecordedCall {
  maxEdge: number;
  quality: number;
}

/**
 * The input buffer a case's `originalLen` denotes: `(i * 7 + 13) & 0xff`.
 * Mirrored byte for byte by the Rust side, so `buffer` identity on an
 * unchanged arm is a real comparand rather than a length check.
 */
export function originalBuffer(len: number): Buffer {
  const out = Buffer.alloc(len);
  for (let i = 0; i < len; i += 1) out[i] = (i * 7 + 13) & 0xff;
  return out;
}

/**
 * A scripted encode's bytes: `(quality + i) & 0xff` for `len` bytes. Distinct
 * from `originalBuffer`'s pattern at i=0 for every ladder quality (78/65/55/45
 * vs 13), which is what lets a metadata probe tell an encode from the input.
 */
export function encodedBuffer(quality: number, len: number): Buffer {
  const out = Buffer.alloc(len);
  for (let i = 0; i < len; i += 1) out[i] = (quality + i) & 0xff;
  return out;
}

/**
 * Build the `sharp` module stand-in for one case's script.
 *
 * Shape: `sharp(buffer)` returns a chainable whose `.metadata()` answers from
 * the script and whose `.resize(opts).webp({quality}).toBuffer()` answers the
 * next step. `calls` accumulates every `(maxEdge, quality)` asked for, in
 * order, and is the differential's ladder comparand.
 *
 * The `resize` options are ASSERTED here rather than recorded: v4 must keep
 * asking for `{width: maxEdge, height: maxEdge, fit: 'inside',
 * withoutEnlargement: true}`, and if a future v4 changes that shape the oracle
 * regeneration is what fails, loudly, at the point where the change happened.
 * Recording them instead would put a constant on the v5 side of the diff,
 * which proves nothing (see the `a-harness-call-site-can-hard-code-a-parameter-
 * blind` note).
 */
export function scriptedSharp(
  script: ShrinkScript,
  original: Buffer,
  calls: RecordedCall[],
): (buffer: Buffer) => unknown {
  return (buffer: Buffer) => {
    const isOriginal = Buffer.isBuffer(buffer) && buffer.equals(original);
    return {
      async metadata() {
        if (isOriginal) {
          if (script.metadata === 'throws') {
            throw new Error(script.metadataThrowMessage ?? 'scripted metadata failure');
          }
          return { width: script.metadata.width ?? undefined, height: script.metadata.height ?? undefined };
        }
        return { width: script.final.width ?? undefined, height: script.final.height ?? undefined };
      },
      resize(opts: { width: number; height: number; fit: string; withoutEnlargement: boolean }) {
        if (
          typeof opts?.width !== 'number' ||
          opts.height !== opts.width ||
          opts.fit !== 'inside' ||
          opts.withoutEnlargement !== true
        ) {
          throw new Error(
            `the shrink's resize options moved: ${JSON.stringify(opts)} — the port's ` +
              '`shrink_to_webp` contract is fit-inside/never-enlarge on a square box, ' +
              'so a change here needs a matching change on the Rust seam',
          );
        }
        const maxEdge = opts.width;
        return {
          webp({ quality }: { quality: number }) {
            return {
              async toBuffer() {
                calls.push({ maxEdge, quality });
                const step = script.steps[calls.length - 1];
                if (step === undefined) {
                  throw new Error(
                    `the ladder asked for rung ${calls.length} (quality ${quality}); the ` +
                      `script carries ${script.steps.length} — a CORPUS bug, not a port one`,
                  );
                }
                if ('throw' in step) throw new Error(step.throw);
                return encodedBuffer(quality, step.len);
              },
            };
          },
        };
      },
    };
  };
}
