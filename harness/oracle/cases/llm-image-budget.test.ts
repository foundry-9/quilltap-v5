/**
 * @jest-environment node
 *
 * Differential ORACLE for v4's REAL `shrinkImageForLlmTransport`
 * (`lib/files/llm-image-budget.ts`, NEW at `bcd7e4852`, bug 151). Tier 1:
 * pure decision logic over a SCRIPTED encoder, exact on every field.
 *
 * Jest rather than tsx, for one reason: `sharp` must be replaced BELOW v4's
 * real function, and only `jest.doMock` can do that — a tsx oracle cannot
 * intercept a module import. Registered before the first import of
 * `@/lib/files/llm-image-budget`, per the `jest-domock-survives-resetmodules`
 * note. No database is opened.
 *
 * The provider registry is REAL, deliberately unlike v4's own unit test (which
 * mocks `getAttachmentSupport` to `undefined`): the `min(target, provider)` arm
 * must read the same eleven manifests the port reads. MEASURED 2026-09-17 —
 * v4's plugin registry and v5's manifests agree value for value, and no
 * provider declares a ceiling under 500 KiB, so the corpus covers both registry
 * branches (a declared ANTHROPIC ceiling, an absent OPENROUTER one) plus the
 * `provider: undefined` path and records that the lower-ceiling arm is
 * unreachable through either real registry.
 *
 * `@/lib/logger` is mocked and its two calls RECORDED, so `ceiling` — which
 * appears nowhere in the returned result — and v4's `${w}x${h}` dimension
 * strings become comparands rather than transcriptions.
 *
 * Emits one NDJSON line per case:
 *   {kind:"case", label, result:{wasShrunk, mimeType, originalSize, finalSize,
 *    width, height, bufferSha256, bufferLen, bufferBase64|null},
 *    calls:[{maxEdge,quality}], log:{level,message,bag}|null}
 *
 * `bufferSha256` is the byte comparand rather than the full base64: four cases
 * carry ~400 KB originals (the ceiling is 500 KiB of base64, so nothing smaller
 * can exceed it), and their base64 would add megabytes to a file both sides
 * read whole. `bufferBase64` is emitted verbatim for buffers at or under 4096
 * bytes, which is every unchanged/identity arm.
 *
 * Run (Node 24, from the v4 checkout; stage the case OUTSIDE any .claude path —
 * v4's jest ignores `/\.claude/`). While v4 HEAD is past the oracle baseline
 * this needs a PINNED worktree; that is the sweep driver's `--v4`, never a path
 * in this header.
 *   N=~/.nvm/versions/node/v24.13.1/bin ; V5W=${V5W:-$HOME/source/quilltap-v5} ; TMPO=/tmp/qt-oracle-run-llmib
 *   cd ~/source/quilltap-server
 *   rm -rf "$TMPO"; mkdir -p "$TMPO/cases" "$TMPO/fixtures" "$TMPO/lib"
 *   cp "$V5W/harness/oracle/cases/llm-image-budget.test.ts" "$TMPO/cases/"
 *   cp "$V5W/harness/oracle/fixtures/llm-image-budget.json" "$TMPO/fixtures/"
 *   cp "$V5W/harness/oracle/lib/shrink-script.ts" "$TMPO/lib/"
 *   QT_ORACLE_OUT=/tmp/oracle-llm-image-budget.ndjson \
 *     $N/npx jest --silent --watchman=false --testTimeout=120000 \
 *       --roots "$PWD" --roots "$TMPO/cases" -- cases/llm-image-budget
 * Run:
 *   QT_ORACLE_LLM_IMAGE_BUDGET=/tmp/oracle-llm-image-budget.ndjson \
 *     cargo test -p quilltap-harness --test llm_image_budget_equivalence -- --nocapture
 */

import * as fs from 'fs';
import { createHash } from 'node:crypto';
import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';

import {
  encodedBuffer,
  originalBuffer,
  scriptedSharp,
  type RecordedCall,
  type ShrinkScript,
} from '../lib/shrink-script';

interface Case {
  label: string;
  note: string;
  mimeType: string;
  provider: string | null;
  filename: string | null;
  originalLen: number;
  script: ShrinkScript;
  v5Calls?: RecordedCall[];
  v5CallsWhy?: string;
}
interface Spec {
  cases: Case[];
}

/** One recorded `logger.debug`/`logger.warn`. */
interface RecordedLog {
  level: string;
  message: string;
  bag: Record<string, unknown>;
}

async function main(): Promise<void> {
  const here = dirname(fileURLToPath(import.meta.url));
  const spec = JSON.parse(
    fs.readFileSync(join(here, '..', 'fixtures', 'llm-image-budget.json'), 'utf8'),
  ) as Spec;

  const outPath = process.env.QT_ORACLE_OUT;
  if (!outPath) throw new Error('QT_ORACLE_OUT must point at the NDJSON file to write');

  // `sharp` is a bare specifier with no `moduleNameMapper` entry, and this case
  // is STAGED under /tmp (v4's jest ignores `/\.claude/`), so jest cannot
  // resolve it from here the way it resolves `better-sqlite3` (which the config
  // maps to a `<rootDir>/__mocks__` file). Mock the RESOLVED path under the v4
  // checkout instead: `import sharp from 'sharp'` inside `lib/` resolves to the
  // same module id, so the explicit mock intercepts it. Same trick as the
  // file-attachment oracle's `cipherDriverPath`.
  const sharpPath = join(process.cwd(), 'node_modules', 'sharp');

  const lines: string[] = [];

  for (const c of spec.cases) {
    const original = originalBuffer(c.originalLen);
    const calls: RecordedCall[] = [];
    const logs: RecordedLog[] = [];

    jest.resetModules();

    // The scripted encoder, BELOW v4's real function.
    const sharpFn = scriptedSharp(c.script, original, calls);
    jest.doMock(sharpPath, () => ({ __esModule: true, default: sharpFn }));

    // The logger, recorded: `ceiling` and the `${w}x${h}` strings live nowhere
    // else, so this is the only way they become comparands instead of
    // transcriptions.
    jest.doMock('@/lib/logger', () => {
      const record =
        (level: string) => (message: string, bag: Record<string, unknown>) =>
          logs.push({ level, message, bag: bag ?? {} });
      // `logger.child(...)` must exist and must be SILENT: importing
      // `llm-image-budget` pulls `image-processing` -> `provider-registry` ->
      // `host-rewrite`, which builds a child logger at module scope. The budget
      // module itself logs through the TOP-LEVEL logger, so a silent child
      // keeps `logs` to exactly the two lines under test.
      const silent = {
        debug: () => {},
        info: () => {},
        warn: () => {},
        error: () => {},
        trace: () => {},
        child: () => silent,
      };
      return {
        __esModule: true,
        LogLevel: { ERROR: 'error', WARN: 'warn', INFO: 'info', DEBUG: 'debug', TRACE: 'trace' },
        logger: {
          debug: record('debug'),
          info: record('info'),
          warn: record('warn'),
          error: record('error'),
          trace: record('trace'),
          child: () => silent,
        },
      };
    });

    const { shrinkImageForLlmTransport } = await import('@/lib/files/llm-image-budget');

    const result = await shrinkImageForLlmTransport({
      buffer: original,
      mimeType: c.mimeType,
      ...(c.provider === null ? {} : { provider: c.provider as never }),
      ...(c.filename === null ? {} : { filename: c.filename }),
    });

    // v4's identity contract on every unchanged arm: the SAME Buffer object
    // comes back. Asserted here (it cannot cross the wire) and pinned on the
    // Rust side as byte equality with the input.
    if (!result.wasShrunk && result.buffer !== original) {
      throw new Error(`${c.label}: an unchanged result must return the input buffer by identity`);
    }

    if (logs.length > 1) {
      throw new Error(`${c.label}: expected at most one log line, got ${JSON.stringify(logs)}`);
    }

    const buf = result.buffer;
    lines.push(
      JSON.stringify({
        kind: 'case',
        label: c.label,
        result: {
          wasShrunk: result.wasShrunk,
          mimeType: result.mimeType,
          originalSize: result.originalSize,
          finalSize: result.finalSize,
          width: result.width ?? null,
          height: result.height ?? null,
          bufferSha256: createHash('sha256').update(buf).digest('hex'),
          bufferLen: buf.length,
          bufferBase64: buf.length <= 4096 ? buf.toString('base64') : null,
        },
        calls,
        log: logs[0] ?? null,
      }),
    );
  }

  // A self-check on the two byte generators: the patterns the Rust side
  // mirrors. If either drifts, every `bufferSha256` silently stops meaning
  // anything, so pin them here where a regeneration reads them.
  const probe = originalBuffer(5);
  if (Buffer.compare(probe, Buffer.from([13, 20, 27, 34, 41])) !== 0) {
    throw new Error(`originalBuffer pattern moved: ${probe.toString('hex')}`);
  }
  const probe2 = encodedBuffer(78, 4);
  if (Buffer.compare(probe2, Buffer.from([78, 79, 80, 81])) !== 0) {
    throw new Error(`encodedBuffer pattern moved: ${probe2.toString('hex')}`);
  }

  fs.writeFileSync(outPath, lines.join('\n') + '\n');
  process.stderr.write(`llm-image-budget oracle wrote ${outPath} (${lines.length} cases)\n`);
}

test('llm-image-budget oracle', async () => {
  await main();
});
