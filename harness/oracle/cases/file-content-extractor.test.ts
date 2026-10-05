/**
 * @jest-environment node
 *
 * Tier-1 ORACLE for v4 `lib/services/file-content-extractor.ts`
 * `extractFileContent` (Rust `quilltap_core::generators::file_content::
 * extract_file_content`) — P4.D253, the FIRST family over that function.
 *
 * Before it, the wizard / ai-import tier-3 families reached the extractor only
 * through v4's jest storage stub (every `downloadFile` → 'mock file content'),
 * so the PDF arm had never been compared. This drives v4's REAL function over
 * the corpus beside this file (`file-content-extractor-corpus.json`), with the
 * four mocks of v4's own `file-content-extractor-pdf.test.ts` (`a434c715b`):
 *
 *   - `@/lib/logger` — a recorder: every `debug`/`info`/`warn`/`error` call on
 *     the root logger AND on the `child` the extractor binds (`via: 'child'`,
 *     with the bindings) is recorded as `{ level, message, fields, via }`.
 *   - `@/lib/logging/create-logger` — `createServiceLogger(service)` answers a
 *     recorder tagged with the service (`MountIndex:PdfConverter` — the
 *     converter's two WARNs land in `converterLines`, recorded and pinned by
 *     the Rust side as v5's named non-port, P4.D253 Tier 3 item 10).
 *   - `@/lib/file-storage/manager` — `downloadFile` resolves the case's bytes
 *     (`bytesBase64`, or `repeat`), or REJECTS for `missing: true`.
 *   - `pdf-parse` — the 2.x class (`PDFParse` → `{ getText, destroy }`), mocked
 *     at its RESOLVED path from the checkout (a bare specifier does not resolve
 *     from this /tmp-staged case; `images-routes.test.ts`'s `node-fetch`
 *     precedent). `getText` resolves `{ text }` from the case's `pdfParse.text`,
 *     rejects a `ReferenceError` for `pdfParse.throw`, and rejects for an absent
 *     script. `parserCalls` counts the constructor so a mock that never fired
 *     is visible in the NDJSON.
 *
 * Emits one NDJSON line per case: `{ name, result, lines, converterLines,
 * parserCalls }` (`result` is v4's `ExtractedContent` through JSON — absent
 * keys stay absent).
 *
 * At the baseline `52d6e7ecd` the PDF arm still called `require('pdf-parse')`
 * as the 1.x function; under the same mock that path throws `pdfParse is not a
 * function` into its catch — the red-first "before" this case records there.
 *
 * Run (Node 24, from the v4 checkout; stage OUTSIDE any .claude path):
 *   N=~/.nvm/versions/node/v24.13.1/bin ; V5W=${V5W:-$HOME/source/quilltap-v5}
 *   cd ~/source/quilltap-server
 *   TMPO=/tmp/qt-file-content-extractor-oracle; rm -rf "$TMPO"; mkdir -p "$TMPO/cases"
 *   cp $V5W/harness/oracle/cases/file-content-extractor.test.ts "$TMPO/cases/"
 *   cp $V5W/harness/oracle/cases/file-content-extractor-corpus.json "$TMPO/cases/"
 *   rm -f /tmp/oracle-file-content-extractor.ndjson
 *   QT_ORACLE_OUT=/tmp/oracle-file-content-extractor.ndjson \
 *     PATH=$N:$PATH $N/npx jest --silent --watchman=false --testTimeout=120000 \
 *       --roots "$PWD" --roots "$TMPO/cases" -- 'cases/file-content-extractor\.test\.ts$'
 */

import * as fs from 'fs';
import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';

interface CaseSpec {
  name: string;
  file: {
    originalFilename: string;
    mimeType: string;
    size: number;
    storageKey: string | null;
    width?: number;
    height?: number;
    description?: string;
  };
  bytesBase64?: string;
  repeat?: { unitBase64: string; count: number };
  missing?: boolean;
  pdfParse?: { text?: string; throw?: string };
}

interface LogLine {
  level: string;
  message: string;
  fields?: Record<string, unknown>;
  via: string;
  bindings?: Record<string, unknown>;
}

function bytesOf(c: CaseSpec): Buffer {
  if (c.repeat) {
    const unit = Buffer.from(c.repeat.unitBase64, 'base64');
    return Buffer.concat(Array.from({ length: c.repeat.count }, () => unit));
  }
  return Buffer.from(c.bytesBase64 ?? '', 'base64');
}

async function main(): Promise<void> {
  const here = dirname(fileURLToPath(import.meta.url));
  const corpus = JSON.parse(
    fs.readFileSync(join(here, 'file-content-extractor-corpus.json'), 'utf8'),
  ) as { cases: CaseSpec[] };

  const outPath = process.env.QT_ORACLE_OUT;
  if (!outPath) throw new Error('QT_ORACLE_OUT must point at the NDJSON file to write');

  // Per-case state the (registered-once) mocks read.
  let current: CaseSpec | null = null;
  let lines: LogLine[] = [];
  let converterLines: LogLine[] = [];
  let parserCalls = 0;

  const recorder = (
    sink: () => LogLine[],
    via: string,
    bindings?: Record<string, unknown>,
  ): Record<string, unknown> => {
    const rec = (level: string) => (message: string, fields?: Record<string, unknown>) => {
      const line: LogLine = { level, message, via };
      if (fields !== undefined) line.fields = fields;
      if (bindings !== undefined) line.bindings = bindings;
      sink().push(line);
    };
    return {
      debug: rec('debug'),
      info: rec('info'),
      warn: rec('warn'),
      error: rec('error'),
      child: (b: Record<string, unknown>) => recorder(sink, 'child', b),
    };
  };

  jest.resetModules();

  // Self-contained — never requireActual the logger inside its own mock
  // (the `a-logger-domock-must-not-requireactual-itself` trap).
  jest.doMock('@/lib/logger', () => ({
    __esModule: true,
    LogLevel: { DEBUG: 'debug', INFO: 'info', WARN: 'warn', ERROR: 'error' },
    Logger: class {},
    installChildLoggerTransport: () => undefined,
    logger: recorder(() => lines, 'root'),
  }));
  jest.doMock('@/lib/logging/create-logger', () => ({
    __esModule: true,
    createServiceLogger: (service: string) =>
      recorder(() => converterLines, 'service', { service }),
  }));
  jest.doMock('@/lib/file-storage/manager', () => ({
    __esModule: true,
    fileStorageManager: {
      downloadFile: async () => {
        const c = current!;
        if (c.missing) throw new Error(`no object at ${c.file.storageKey}`);
        return bytesOf(c);
      },
    },
  }));
  const pdfParsePath = require.resolve('pdf-parse', { paths: [process.cwd()] });
  const pdfParseMock = () => ({
    __esModule: true,
    PDFParse: function PDFParse() {
      parserCalls += 1;
      return {
        getText: async () => {
          const script = current!.pdfParse;
          if (script?.throw !== undefined) throw new ReferenceError(script.throw);
          if (script?.text !== undefined) return { text: script.text, total: 1 };
          throw new Error(`case ${current!.name} reached pdf-parse with no script`);
        },
        destroy: async () => undefined,
      };
    },
  });
  jest.doMock(pdfParsePath, pdfParseMock);
  // The bare specifier too, for a jest resolver that keys the dynamic import
  // under the package name rather than the resolved file.
  jest.doMock('pdf-parse', pdfParseMock, { virtual: true });

  const { extractFileContent } = require('@/lib/services/file-content-extractor') as {
    extractFileContent: (f: unknown) => Promise<unknown>;
  };

  const out: string[] = [];
  for (const c of corpus.cases) {
    current = c;
    lines = [];
    converterLines = [];
    parserCalls = 0;
    const file = {
      id: `file-${c.name}`,
      userId: 'user-1',
      ...c.file,
    };
    const result = await extractFileContent(file);
    out.push(JSON.stringify({ name: c.name, result, lines, converterLines, parserCalls }));
  }
  current = null;

  fs.writeFileSync(outPath, out.join('\n') + '\n');
  // eslint-disable-next-line no-console
  console.log(`file-content-extractor oracle wrote ${outPath} (${corpus.cases.length} cases)`);
}

it('emits the file content extractor oracle', async () => {
  await main();
}, 120000);
