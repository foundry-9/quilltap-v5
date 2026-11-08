/**
 * Tier-1 oracle case — the Salon stream frames (P4.D366, v4 `604a082c4`
 * `lib/chat/stream-frames.ts` at the `624d0d25c` pin).
 *
 * Drives v4's REAL `encodeFrame` in all three encoder profiles — a plain
 * `TextEncoder` (the `frames` dialect), `legacyFrameEncoder()` and
 * `seatedFrameEncoder('p-seat')` — plus `toLegacyBag(tagFrame(frame,
 * 'p-seat'))` (the legacy dialect with a seat), over the shared corpus
 * `harness/oracle/fixtures/stream-frames.json`, which the Rust side reads too.
 * `done` rows are built through v4's REAL `encodeDoneEvent` /
 * `encodeTurnDoneEvent`, so `outcome`'s position and the producer's key order
 * are v4's own.
 *
 * Then v4's six GOLDEN streams (`__tests__/fixtures/salon-stream/*.legacy.json`,
 * recorded from the pre-4.12 encoders) are read from the v4 checkout, each bag
 * translated by v4's REAL `fromLegacyBag` (the test helper
 * `__tests__/helpers/salon-stream-golden.ts`, imported by path — v4's code,
 * not a reimplementation), and encoded in the frames and legacy profiles. The
 * case ASSERTS v4's own golden invariant (`toLegacyBag(fromLegacyBag(bag))`
 * deep-equals the recorded bag, as v4's golden test's `toEqual` does) and
 * throws if it does not hold at the pin.
 *
 * Sentinel (JSON cannot say `undefined`): `"__UNDEFINED__"` as a frame value
 * becomes a real `undefined` (the key is then dropped by `JSON.stringify`).
 *
 * Pure — no DB, no jest. One NDJSON row per (row, profile):
 * `{ section, name, profile, body }` (`body` = the bytes between `data: ` and
 * `\n\n`), and per golden bag `{ section: "golden", name, profile: "recorded",
 * body }` with the recorded bag.
 *
 * Regenerate (Node 24, from the v4 checkout — or the sweep driver's pin):
 *   V5W=${V5W:-$HOME/source/quilltap-v5}
 *   N=~/.nvm/versions/node/v24.13.1/bin
 *   rm -f /tmp/oracle-stream-frames.ndjson
 *   cd ~/source/quilltap-server
 *   $N/npx tsx $V5W/harness/oracle/cases/stream-frames.ts \
 *     > /tmp/oracle-stream-frames.ndjson
 */

import * as fs from 'fs';
import { isDeepStrictEqual } from 'node:util';
import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';

import {
  encodeFrame,
  legacyFrameEncoder,
  seatedFrameEncoder,
  tagFrame,
  toLegacyBag,
  type StreamFrame,
} from '@/lib/chat/stream-frames';
import {
  encodeDoneEvent,
  encodeTurnDoneEvent,
  type DoneEventData,
} from '@/lib/services/chat-message/streaming.service';
import { GOLDEN_STREAMS, fromLegacyBag, loadLegacyStream } from '@/__tests__/helpers/salon-stream-golden';

const UNDEFINED = '__UNDEFINED__';
const SEAT = 'p-seat';

interface FrameRow { name: string; frame: Record<string, unknown> }
interface DoneRow { name: string; data: Record<string, unknown> }
interface TurnDoneRow { name: string; streaming: Record<string, unknown>; data: Record<string, unknown> }
interface Corpus {
  frames: FrameRow[];
  done: DoneRow[];
  turnDone: TurnDoneRow[];
  tagged: FrameRow[];
  swipe: FrameRow[];
}

/** Replace the `"__UNDEFINED__"` sentinel with a real `undefined`, recursively. */
function revive(v: unknown): unknown {
  if (v === UNDEFINED) return undefined;
  if (Array.isArray(v)) return v.map(revive);
  if (v && typeof v === 'object') {
    const out: Record<string, unknown> = {};
    for (const [k, x] of Object.entries(v as Record<string, unknown>)) out[k] = revive(x);
    return out;
  }
  return v;
}

const here = dirname(fileURLToPath(import.meta.url));
const corpus = JSON.parse(
  fs.readFileSync(join(here, '..', 'fixtures', 'stream-frames.json'), 'utf8'),
) as Corpus;

const decoder = new TextDecoder();
const PREFIX = 'data: ';
const SUFFIX = '\n\n';

function body(bytes: Uint8Array, what: string): string {
  const text = decoder.decode(bytes);
  if (!text.startsWith(PREFIX) || !text.endsWith(SUFFIX)) {
    throw new Error(`${what}: frame lost its framing: ${JSON.stringify(text)}`);
  }
  return text.slice(PREFIX.length, text.length - SUFFIX.length);
}

function emit(section: string, name: string, profile: string, b: string): void {
  process.stdout.write(JSON.stringify({ section, name, profile, body: b }) + '\n');
}

/** Encode one frame in the four profiles. `make` re-builds the frame each time
 * (an encoder never mutates it, but the done helpers take fresh data). */
function emitAll(section: string, name: string, make: (enc: TextEncoder) => Uint8Array, frame?: StreamFrame): void {
  emit(section, name, 'frames', body(make(new TextEncoder()), `${name}/frames`));
  emit(section, name, 'legacy', body(make(legacyFrameEncoder()), `${name}/legacy`));
  emit(section, name, 'seated', body(make(seatedFrameEncoder(SEAT)), `${name}/seated`));
  // The legacy dialect WITH a seat: `toLegacyBag(tagFrame(frame, seat))`. For
  // a done row the frame is recovered from the frames bytes (the encoder
  // renders the producer's literal unparsed, so the bytes ARE the frame).
  const f = frame ?? (JSON.parse(body(make(new TextEncoder()), `${name}/frames`)) as StreamFrame);
  emit(section, name, 'legacySeated', JSON.stringify(toLegacyBag(tagFrame(f, SEAT))));
}

for (const row of corpus.frames) {
  const frame = revive(row.frame) as StreamFrame;
  emitAll('frames', row.name, (enc) => encodeFrame(enc, frame), frame);
}

for (const row of corpus.done) {
  emitAll('done', row.name, (enc) => encodeDoneEvent(enc, revive(row.data) as DoneEventData));
}

for (const row of corpus.turnDone) {
  const s = revive(row.streaming) as Record<string, unknown>;
  // Only the five fields `encodeTurnDoneEvent` reads from the streaming state.
  const streaming = {
    usage: s.usage,
    cacheUsage: s.cacheUsage,
    attachmentResults: s.attachmentResults,
    effectiveProfile: { provider: s.provider, modelName: s.modelName },
  } as unknown as Parameters<typeof encodeTurnDoneEvent>[1];
  emitAll('turnDone', row.name, (enc) =>
    encodeTurnDoneEvent(enc, streaming, revive(row.data) as Parameters<typeof encodeTurnDoneEvent>[2]),
  );
}

for (const row of corpus.tagged) {
  const frame = revive(row.frame) as StreamFrame;
  emit('tagged', row.name, 'legacySeated', JSON.stringify(toLegacyBag(tagFrame(frame, SEAT))));
}

for (const row of corpus.swipe) {
  const frame = revive(row.frame) as StreamFrame;
  emit('swipe', row.name, 'frames', body(encodeFrame(new TextEncoder(), frame), `${row.name}/frames`));
  emit('swipe', row.name, 'legacy', body(encodeFrame(legacyFrameEncoder(), frame), `${row.name}/legacy`));
}

// v4's six golden streams, through v4's own translator.
for (const stream of GOLDEN_STREAMS) {
  const bags = loadLegacyStream(stream);
  bags.forEach((bag, i) => {
    const name = `${stream}#${i}`;
    const frame = fromLegacyBag(bag);
    const legacy = toLegacyBag(frame);
    // v4's golden invariant (`toEqual`: deep, key-order-blind).
    if (!isDeepStrictEqual(JSON.parse(JSON.stringify(legacy)), bag)) {
      throw new Error(`golden ${name}: toLegacyBag(fromLegacyBag(bag)) != recorded bag at the pin`);
    }
    emit('golden', name, 'recorded', JSON.stringify(bag));
    emit('golden', name, 'frames', body(encodeFrame(new TextEncoder(), frame), `${name}/frames`));
    emit('golden', name, 'legacy', body(encodeFrame(legacyFrameEncoder(), frame), `${name}/legacy`));
  });
}
