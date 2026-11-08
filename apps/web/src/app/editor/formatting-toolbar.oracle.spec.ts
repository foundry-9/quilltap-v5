import { describe, expect, it } from 'vitest';

import corpusText from '../../testing/fixtures/markdown-button-name.oracle.ndjson';
import { MARKDOWN_BUTTON_TABLE, markdownButtonName } from './formatting-toolbar';

/**
 * The corpus differential for v4's `markdownButtonName`
 * (`components/chat/FormattingToolbar.tsx:128-139`, `df8d4d70a`) — the
 * Markdown buttons' accessible names. v4 does not export it, so
 * `apps/web/oracle/markdown-button-name.recorder.ts` slices the pin's OWN bytes
 * of the declaration, transpiles and evaluates them (ruling PL-11), and also
 * records v4's `MARKDOWN_FORMATS` so v5's button table is pinned to it.
 */

interface FormatsRow {
  id: 'formats';
  kind: 'formats';
  declaration: string;
  formats: Array<{ label: string; tooltip: string; type: string }>;
}
interface NameRow {
  id: string;
  kind: 'name';
  label: string;
  tooltip: string;
  out: string;
}

const ROWS: Array<FormatsRow | NameRow> = (corpusText as unknown as string)
  .split('\n')
  .filter((line) => line.trim() !== '')
  .map((line) => JSON.parse(line) as FormatsRow | NameRow);
const NAMES = ROWS.filter((r): r is NameRow => r.kind === 'name');
const FORMATS = ROWS.find((r): r is FormatsRow => r.kind === 'formats')!;

describe("markdownButtonName agrees with v4's real bytes row for row", () => {
  it('carries the whole recorded corpus', () => {
    expect(ROWS).toHaveLength(31);
    expect(NAMES).toHaveLength(30);
    expect(FORMATS.formats).toHaveLength(11);
  });

  it("v5's button table is v4's MARKDOWN_FORMATS, in order", () => {
    expect(JSON.stringify(MARKDOWN_BUTTON_TABLE.map((b) => ({ label: b.label, tooltip: b.tooltip, type: b.type })))).toBe(
      JSON.stringify(FORMATS.formats),
    );
  });

  it.each(NAMES)('$id', (row) => {
    expect(markdownButtonName({ label: row.label, tooltip: row.tooltip })).toBe(row.out);
  });
});
