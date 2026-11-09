/**
 * @jest-environment node
 *
 * P4.D367 — tier-3 ORACLE for the image transport budget: drives v4's REAL
 * `applyImageTransportBudget` (`lib/chat/image-transport-budget.ts`,
 * `29c1e2690`, at the `624d0d25c` pin) over the committed corpus
 * `harness/oracle/fixtures/image-transport-budget.json` (`apply` rows).
 *
 * The "LLM" here is the vision describer: `@/lib/chat/file-attachment-
 * fallback`'s `processFileAttachmentFallback` (the capability arm) and
 * `generateImageDescription` (the overflow arm) are doMocked PER ROW to a
 * SCRIPTED describer answering from the row's `scripts` (a `{throw}` script
 * THROWS — v5's scripted describer answers `Err` there). Everything else is
 * real: `formatFallbackAsMessagePrefix` (`jest.requireActual`), the policy
 * (`imageTransportPolicyFor` → `resolveMaxImagesPerRequest` +
 * `profileCanReceiveAttachment`, whose plugin registry is NOT initialised in a
 * jest run, so v4's static capability table answers — asserted below so the
 * capability rows cannot pass vacuously), and the logger (captured by
 * splicing a sink into the singleton's transports at DEBUG).
 *
 * `repos` is a Proxy that THROWS on any read: the scripted describer never
 * reads it, and neither may the pass (asserted by construction).
 *
 * Emits one NDJSON line per row: `{ kind: 'apply', label, same, messages,
 * report, calls, logs }` where `messages` is the RAW rebuilt array (an
 * absent `attachments` key is `null`, an empty list `[]`; a string `data` is
 * projected to its UTF-16 length as `"<n chars>"` — pass-through bytes, not a
 * comparand worth megabytes), `report` is `JSON.stringify(report)`, `calls`
 * the describer calls in order, `logs` the `[ImageTransport]` lines.
 *
 * Run: see the recipe in crates/quilltap-harness/tests/image_transport_budget_equivalence.rs.
 */

import * as fs from 'fs';
import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';

type Script = { type?: string; text?: string; error?: string; throw?: string };
interface ApplyRow {
  label: string;
  rule: string;
  profile: Record<string, unknown>;
  messages: Array<Record<string, unknown>>;
  scripts: Record<string, { fallback?: Script; describe?: Script }>;
  policy?: { maxImages: number; maxTotalBase64: number; canReceive: string[] };
  logContext: Record<string, unknown>;
  formatted: boolean;
}

function expandData(v: unknown): unknown {
  if (Array.isArray(v)) return v.map(expandData);
  if (!v || typeof v !== 'object') return v;
  const out: Record<string, unknown> = {};
  for (const [k, val] of Object.entries(v as Record<string, unknown>)) {
    if (k === '$dataLen') out.data = 'A'.repeat(val as number);
    else out[k] = expandData(val);
  }
  return out;
}

/** A string `data` → `"<n chars>"`; an absent `attachments` → `null`. */
function project(messages: Array<Record<string, unknown>>): unknown {
  return messages.map((m) => {
    const out: Record<string, unknown> = {};
    for (const [k, v] of Object.entries(m)) {
      if (k === 'attachments') {
        out[k] = (v as unknown[]).map((a) => {
          if (!a || typeof a !== 'object' || Array.isArray(a)) return a;
          const o: Record<string, unknown> = {};
          for (const [ak, av] of Object.entries(a as Record<string, unknown>)) {
            o[ak] = ak === 'data' && typeof av === 'string' ? `<${av.length} chars>` : av;
          }
          return o;
        });
      } else {
        out[k] = v;
      }
    }
    if (!('attachments' in out)) out.attachments = null;
    return out;
  });
}

const here = dirname(fileURLToPath(import.meta.url));
const corpus = JSON.parse(
  fs.readFileSync(join(here, '..', 'fixtures', 'image-transport-budget.json'), 'utf8'),
) as { apply: ApplyRow[] };

describe('image transport budget oracle (P4.D367)', () => {
  it('drives the real applyImageTransportBudget over the corpus', async () => {
    const outPath = process.env.QT_ORACLE_OUT;
    if (!outPath) throw new Error('QT_ORACLE_OUT must point at the NDJSON file to write');
    process.env.LOG_LEVEL = 'debug';
    const lines: string[] = [];

    for (const row of corpus.apply) {
      jest.resetModules();
      const calls: Array<Record<string, unknown>> = [];
      const answer = (id: string, script: Script | undefined, which: 'fallback' | 'describe', file: Record<string, unknown>) => {
        const s: Script = script ?? { type: 'image_description', text: `A description of ${id}.` };
        if (s.throw !== undefined) throw new Error(s.throw);
        const meta = { originalFilename: file.filename, originalMimeType: file.mimeType };
        if (s.type === 'image_description') return { type: 'image_description', imageDescription: s.text, processingMetadata: meta };
        if (s.type === 'text') return { type: 'text', textContent: s.text, processingMetadata: meta };
        return { type: 'unsupported', ...(s.error !== undefined ? { error: s.error } : {}), processingMetadata: meta };
      };
      jest.doMock('@/lib/chat/file-attachment-fallback', () => {
        const actual = jest.requireActual('@/lib/chat/file-attachment-fallback');
        return {
          ...actual,
          processFileAttachmentFallback: async (fileMetadata: Record<string, unknown>, fileAttachment: Record<string, unknown>) => {
            calls.push({ fn: 'fallback', id: fileAttachment.id, hasTransport: 'transport' in fileAttachment || 'transport' in fileMetadata });
            return answer(fileAttachment.id as string, row.scripts[fileAttachment.id as string]?.fallback, 'fallback', fileAttachment);
          },
          generateImageDescription: async (fileAttachment: Record<string, unknown>) => {
            calls.push({ fn: 'describe', id: fileAttachment.id, hasTransport: 'transport' in fileAttachment });
            return answer(fileAttachment.id as string, row.scripts[fileAttachment.id as string]?.describe, 'describe', fileAttachment);
          },
        };
      });
      jest.doMock('@/lib/logger', () => jest.requireActual('@/lib/logger'));

      const { logger } = await import('@/lib/logger');
      const logs: unknown[] = [];
      const internal = logger as unknown as { transports: Array<{ write: (d: unknown) => void }> };
      internal.transports.splice(0, internal.transports.length, {
        write: (d: unknown) => {
          const data = d as { level: string; message: string; context?: Record<string, unknown>; error?: { message: string } };
          if (!data.message.startsWith('[ImageTransport]')) return;
          const { service: _s, module: _m, environment: _e, ...fields } = (data.context ?? {}) as Record<string, unknown>;
          if (data.error) fields.error = data.error.message;
          logs.push(JSON.parse(JSON.stringify({ level: data.level, message: data.message, fields })));
        },
      });

      const { profileCanReceiveAttachment } = await import('@/lib/llm/image-transport');
      // Not vacuous: the static table answers, and OLLAMA cannot transport.
      if (profileCanReceiveAttachment({ provider: 'OLLAMA', supportsImageUpload: true } as never, 'image/png') !== false) {
        throw new Error('OLLAMA must not receive images here — the capability rows would be vacuous');
      }
      if (profileCanReceiveAttachment({ provider: 'OPENAI', supportsImageUpload: true } as never, 'image/png') !== true) {
        throw new Error('OPENAI with the vision tick must receive images');
      }

      const budget = await import('@/lib/chat/image-transport-budget');
      const repos = new Proxy({}, { get: (_t, k) => { throw new Error(`repos.${String(k)} read`); } });
      const messages = expandData(row.messages) as Array<Record<string, unknown>>;
      const policy = row.policy
        ? {
            maxImages: row.policy.maxImages,
            maxTotalBase64: row.policy.maxTotalBase64,
            canReceive: ((set) => (m: string) => set.has(m))(new Set(row.policy.canReceive)),
          }
        : undefined;
      const { messages: result, report } = await budget.applyImageTransportBudget(
        messages as never,
        row.profile as never,
        { repos, userId: 'user-1', logContext: row.logContext, describeOptions: { chatId: 'chat-1' } },
        policy,
      );
      lines.push(
        JSON.stringify({
          kind: 'apply',
          label: row.label,
          same: result === (messages as unknown),
          messages: project(result as Array<Record<string, unknown>>),
          report: JSON.stringify(report),
          calls,
          logs,
        }),
      );
    }

    fs.writeFileSync(outPath, lines.join('\n') + '\n');
    // eslint-disable-next-line no-console
    console.log(`image-transport-budget oracle wrote ${outPath} (${lines.length} lines)`);
  });
});
