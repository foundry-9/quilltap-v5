/**
 * Oracle case — the Scriptorium conversation renderer (P4.6BM, tier-1 EXACT).
 *
 * Drives v4's REAL `renderConversationMarkdown`
 * (`lib/scriptorium/markdown-renderer.ts`) over the committed corpus
 * (`harness/oracle/fixtures/conversation-markdown.json`) and emits, per case,
 * the full `{ markdown, interchanges }` return. The Rust side compares
 * byte-for-byte — no normalization at all.
 *
 * The renderer reads the wall clock exactly once (`new Date().toISOString()`
 * for the header's `Current time:` line), so `globalThis.Date` is frozen to the
 * corpus `nowIso` BEFORE the module is imported. Everything else in the module
 * is a pure function of the corpus.
 *
 * `TZ=UTC` is load-bearing: v4 formats through `toLocaleDateString('en-US')` /
 * `toLocaleTimeString('en-US')` with no explicit timezone, so the rendered
 * timestamps follow the host zone. The Rust port computes in UTC.
 *
 * Run (Node 24, from the v4 checkout):
 *   N=~/.nvm/versions/node/v24.13.1/bin
 *   cd ~/source/quilltap-server
 *   TZ=UTC $N/npx tsx \
 *     ~/source/quilltap-v5/harness/oracle/cases/conversation-markdown.ts \
 *     > /tmp/oracle-conversation-markdown.ndjson
 */

import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import { readFileSync } from 'node:fs';

interface MessageSeed {
  id: string;
  type: string;
  role: string;
  content: string;
  participantId: string | null;
  createdAt: string;
}
interface CaseSeed {
  name: string;
  characterNames: Array<[string, string]>;
  metadata: {
    conversationId: string;
    title: string;
    createdAt: string;
    lastUpdatedAt: string;
  } | null;
  messages: MessageSeed[];
}
interface Spec {
  nowIso: string;
  cases: CaseSeed[];
}

async function main(): Promise<void> {
  const here = dirname(fileURLToPath(import.meta.url));
  const spec = JSON.parse(
    readFileSync(join(here, '..', 'fixtures', 'conversation-markdown.json'), 'utf8')
  ) as Spec;

  // Freeze the clock before importing the renderer: its only impure read is
  // `new Date().toISOString()` for the `Current time:` header line.
  const FIXED_MS = Date.parse(spec.nowIso);
  const RealDate = Date;
  class FrozenDate extends RealDate {
    constructor(...args: unknown[]) {
      if (args.length === 0) {
        super(FIXED_MS);
      } else {
        // @ts-expect-error — forwarding the real Date overloads verbatim.
        super(...args);
      }
    }
    static now(): number {
      return FIXED_MS;
    }
  }
  (globalThis as unknown as { Date: unknown }).Date = FrozenDate;

  const { renderConversationMarkdown } = await import('@/lib/scriptorium/markdown-renderer');

  const lines: string[] = [];
  for (const c of spec.cases) {
    const names = new Map<string, string>(c.characterNames);
    const result = renderConversationMarkdown(
      c.messages as never,
      [] as never,
      names,
      (c.metadata ?? undefined) as never
    );
    lines.push(
      JSON.stringify({
        name: c.name,
        markdown: result.markdown,
        interchanges: result.interchanges,
      })
    );
  }

  process.stdout.write(lines.join('\n') + '\n');
}

main().catch((err) => {
  process.stderr.write(`conversation-markdown oracle failed: ${err?.stack ?? err}\n`);
  process.exit(1);
});
