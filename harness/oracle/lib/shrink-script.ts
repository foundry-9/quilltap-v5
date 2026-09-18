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
 * Whether `buffer` looks like an [`encodedBuffer`] product: every byte is its
 * predecessor plus one (mod 256), which `originalBuffer`'s step of 7 is not.
 * Ambiguous under 2 bytes, so callers require the length; no corpus carries a
 * one-byte image. Structural rather than rebuild-and-compare, which would
 * allocate a hundred candidates per probe for the 384 KB rungs. Mirrored by the
 * Rust `is_encoded_pattern`.
 */
export function isEncodedPattern(buffer: Buffer): boolean {
  if (buffer.length === 0) return false;
  const q0 = buffer[0];
  for (let i = 0; i < buffer.length; i += 1) {
    if (buffer[i] !== ((q0 + i) & 0xff)) return false;
  }
  return true;
}

/** One original buffer and the script that describes it. */
export interface ScriptEntry {
  bytes: Buffer;
  script: ShrinkScript;
  /** Filled in as the ladder runs, in order. */
  calls: RecordedCall[];
}

/**
 * The MULTI-script `sharp` stand-in: one mock serving several original buffers,
 * each with its own script, plus a DEFAULT for buffers no script names.
 *
 * `file_attachment_tier3` needs this shape — its corpus carries twenty-two
 * files and scripts only the bug-151 ones, so everything else must behave as
 * SMALL (the default) and take the budget's early return, which is what keeps
 * every pre-existing oracle row byte-identical.
 *
 * Which script a probe belongs to is resolved the same way on both sides: a
 * buffer that IS a registered original names its script (and becomes the
 * "current" one); a buffer that is one of the current script's own produced
 * encodes is v4's `sharp(best).metadata()` and answers that script's `final`;
 * anything else is the default.
 */
export function scriptedSharpMulti(
  entries: ScriptEntry[],
  defaultScript: ShrinkScript,
): (buffer: Buffer) => unknown {
  let current: ScriptEntry | null = null;
  let defaultCalls = 0;

  const dims = (d: { width: number | null; height: number | null }) => ({
    width: d.width ?? undefined,
    height: d.height ?? undefined,
  });

  return (buffer: Buffer) => {
    const own = Buffer.isBuffer(buffer)
      ? (entries.find((e) => e.bytes.equals(buffer)) ?? null)
      : null;
    if (own) current = own;

    return {
      async metadata() {
        if (own) {
          if (own.script.metadata === 'throws') {
            throw new Error(own.script.metadataThrowMessage ?? 'scripted metadata failure');
          }
          return dims(own.script.metadata);
        }
        if (
          current &&
          buffer.length >= 2 &&
          isEncodedPattern(buffer) &&
          current.script.steps.some((st) => 'len' in st && st.len === buffer.length)
        ) {
          return dims(current.script.final);
        }
        if (defaultScript.metadata === 'throws') {
          throw new Error(defaultScript.metadataThrowMessage ?? 'scripted metadata failure');
        }
        return dims(defaultScript.metadata);
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
        const target = own;
        return {
          webp({ quality }: { quality: number }) {
            return {
              async toBuffer() {
                const script = target ? target.script : defaultScript;
                let rung: number;
                if (target) {
                  target.calls.push({ maxEdge, quality });
                  rung = target.calls.length - 1;
                } else {
                  // An UNSCRIPTED buffer reached the ladder. For a tier-3
                  // corpus that is a finding, not a fixture detail: the default
                  // is "small", so the budget should have early-returned.
                  rung = defaultCalls;
                  defaultCalls += 1;
                }
                const step = script.steps[rung];
                if (step === undefined) {
                  throw new Error(
                    `${target ? 'the ladder' : 'an UNSCRIPTED buffer'} asked for rung ` +
                      `${rung + 1} (quality ${quality}, ${buffer.length} bytes); the script ` +
                      `carries ${script.steps.length} — a CORPUS bug, not a port one. A file ` +
                      'that reaches the codec needs its own `shrinkScript`.',
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
  // The one-script case is the multi-script one with a single entry and a
  // default that can never be reached (every probe is either the original or
  // one of its encodes). Sharing the body keeps the two bug-151 oracles from
  // drifting apart in how they resolve a probe.
  const entry: ScriptEntry = { bytes: original, script, calls };
  return scriptedSharpMulti([entry], script);
}
