/**
 * P4.167 (P4.D317 Tier 3 #30) oracle: the `.qtap` NDJSON reader's log lines.
 *
 * Drives v4's REAL `readNdjsonLines` (`lib/import/ndjson-reader.ts`) and
 * `assembleExportFromStream` (`lib/import/quilltap-import-stream.ts`) — the
 * pair `loadQtapFromUpload` runs — over the committed HAND-BUILT bundle
 * `fixtures/qtap-ndjson-reader-lines.qtap`: a second envelope, five non-object
 * records (a number, `null`, a string, `true`, `false`), an ARRAY (an object
 * to JS — its missing `kind` is an unknown kind), a kept and an orphaned
 * `wardrobe_item`, an orphaned `character_plugin_data` and `chat_message`,
 * three unknown kinds (a name, a number, none) and a footer. Every logger line
 * the reader emits is captured through a `Logger.prototype` spy (every child
 * logger shares the prototype) as `{level, message, ...context}` with
 * `module` dropped and `undefined` values skipped, plus the reassembled
 * export's shape.
 *
 * Run (Node 24, from the v4 checkout):
 *   N=~/.nvm/versions/node/v24.13.1/bin
 *   cd ~/source/quilltap-server
 *   LOG_LEVEL=error $N/npx tsx ~/source/quilltap-v5/harness/oracle/cases/qtap-ndjson-reader-lines.ts \
 *     > /tmp/oracle-qtap-ndjson-reader-lines.ndjson
 * Then:
 *   QT_ORACLE_QTAP_NDJSON_READER_LINES=/tmp/oracle-qtap-ndjson-reader-lines.ndjson \
 *     cargo test -p quilltap-harness --test qtap_ndjson_reader_lines_equivalence -- --nocapture
 */

import { readFileSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';

async function main(): Promise<void> {
  const here = dirname(fileURLToPath(import.meta.url));
  const bytes = readFileSync(join(here, '..', 'fixtures', 'qtap-ndjson-reader-lines.qtap'));

  const { Logger } = await import('@/lib/logger');
  const lines: Array<Record<string, unknown>> = [];
  for (const level of ['error', 'warn', 'info', 'debug'] as const) {
    const original = Logger.prototype[level];
    Logger.prototype[level] = function (
      this: unknown,
      message: string,
      context?: Record<string, unknown>,
      ...rest: unknown[]
    ) {
      const rec: Record<string, unknown> = { level, message };
      for (const [k, v] of Object.entries(context ?? {})) {
        if (k === 'module' || v === undefined) continue;
        rec[k] = v;
      }
      lines.push(rec);
      return (original as (...a: unknown[]) => void).call(this, message, context, ...rest);
    } as never;
  }

  const { readNdjsonLines } = await import('@/lib/import/ndjson-reader');
  const { assembleExportFromStream } = await import('@/lib/import/quilltap-import-stream');
  const stream = new ReadableStream<Uint8Array>({
    start(controller) {
      controller.enqueue(new Uint8Array(bytes));
      controller.close();
    },
  });
  const out = await assembleExportFromStream(readNdjsonLines(stream));
  const data = out.data as unknown as Record<string, unknown>;
  const characters = (data.characters ?? []) as Array<Record<string, unknown>>;
  process.stdout.write(
    JSON.stringify({
      name: 'reader_lines',
      lines,
      exportType: (out.manifest as unknown as Record<string, unknown>).exportType,
      characters: characters.map((c) => ({
        id: c.id,
        wardrobeItems: ((c.wardrobeItems ?? []) as Array<Record<string, unknown>>).map((w) => w.id),
        pluginData: c.pluginData ?? null,
      })),
    }) + '\n',
  );
}

main().catch((err) => {
  process.stderr.write(`${err instanceof Error ? err.stack : String(err)}\n`);
  process.exit(1);
});
