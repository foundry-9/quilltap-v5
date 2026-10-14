/**
 * Tier-1 oracle case — the chat stream's ERROR frame (v4
 * `lib/services/chat-message/streaming.service.ts` `encodeErrorEvent` +
 * `ErrorEventExtras`), P4.D301 / v4 `93c5af6ad` (bug 241).
 *
 * Drives v4's REAL `encodeErrorEvent` over the shared corpus
 * `harness/oracle/fixtures/chat-error-frame.json`, which the Rust side reads
 * too, and emits the frame's JSON BODY — the bytes between `data: ` and the
 * closing `\n\n` — so the Rust side can compare `serde_json::to_string` of the
 * `ChatErrorPayload` the spine builds byte for byte. The frame is v4's
 * `` `data: ${JSON.stringify({ error, errorType, details, ...extras })}\n\n` ``:
 * key order `error, errorType, details, userMessagePersisted,
 * pendingToolResultsPersisted`, and an absent / `undefined` extra is no key
 * (the three-key frame every other caller still sends).
 *
 * Sentinels (JSON cannot say `undefined`): `"__ABSENT__"` omits the `extras`
 * argument, `"__UNDEFINED__"` passes / sets a real `undefined`.
 *
 * Pure — no DB, no jest. One NDJSON row per case: `{ name, prefix, suffix, body }`
 * (`prefix` / `suffix` pin the `data: ` / `\n\n` framing the body sits in).
 *
 * Regenerate (Node 24, from the v4 checkout — or the sweep driver's pin):
 *   V5W=${V5W:-$HOME/source/quilltap-v5}
 *   N=~/.nvm/versions/node/v24.13.1/bin
 *   rm -f /tmp/oracle-chat-error-frame.ndjson
 *   cd ~/source/quilltap-server
 *   $N/npx tsx $V5W/harness/oracle/cases/encode-error-event.ts \
 *     > /tmp/oracle-chat-error-frame.ndjson
 */

import * as fs from 'fs';
import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';

import { encodeErrorEvent, type ErrorEventExtras } from '@/lib/services/chat-message/streaming.service';

const UNDEFINED = '__UNDEFINED__';
const ABSENT = '__ABSENT__';

interface Case {
  name: string;
  error: string;
  errorType: string;
  details: string;
  extras: string | Record<string, unknown>;
}

function toExtras(raw: Record<string, unknown>): ErrorEventExtras {
  const out: Record<string, unknown> = {};
  for (const [k, v] of Object.entries(raw)) {
    out[k] = v === UNDEFINED ? undefined : v;
  }
  return out as ErrorEventExtras;
}

const here = dirname(fileURLToPath(import.meta.url));
const corpus = JSON.parse(
  fs.readFileSync(join(here, '..', 'fixtures', 'chat-error-frame.json'), 'utf8'),
) as { cases: Case[] };

const encoder = new TextEncoder();
const decoder = new TextDecoder();
const PREFIX = 'data: ';
const SUFFIX = '\n\n';

for (const c of corpus.cases) {
  let bytes: Uint8Array;
  if (c.extras === ABSENT) {
    bytes = encodeErrorEvent(encoder, c.error, c.errorType, c.details);
  } else if (c.extras === UNDEFINED) {
    bytes = encodeErrorEvent(encoder, c.error, c.errorType, c.details, undefined);
  } else if (typeof c.extras === 'object') {
    bytes = encodeErrorEvent(encoder, c.error, c.errorType, c.details, toExtras(c.extras));
  } else {
    throw new Error(`case ${c.name}: unknown extras sentinel ${String(c.extras)}`);
  }
  const text = decoder.decode(bytes);
  if (!text.startsWith(PREFIX) || !text.endsWith(SUFFIX)) {
    throw new Error(`case ${c.name}: frame lost its framing: ${JSON.stringify(text)}`);
  }
  const body = text.slice(PREFIX.length, text.length - SUFFIX.length);
  process.stdout.write(
    JSON.stringify({ name: c.name, prefix: PREFIX, suffix: SUFFIX, body }) + '\n',
  );
}
