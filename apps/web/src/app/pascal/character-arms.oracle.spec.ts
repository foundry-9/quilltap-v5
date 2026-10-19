/**
 * The corpus differential for Pascal's `{{character}}` placeholder and
 * `announceAsCharacter` key, on the browser side (`a05c6f755` — P4.D320).
 *
 * The SHARED corpus (`pascal-custom-tool-definition.oracle.ndjson`) is the
 * Pascal server lane's to grow; these are the rows that did not exist before,
 * recorded by this lane's own `apps/web/oracle/pascal-character.recorder.ts`
 * from v4's REAL `QtapCustomToolSchema` / `formatDefinitionIssues` /
 * `collectUnknownKeys` / `classifyPlaceholder` at the pin `e8e17b2d9`, so the
 * twins' new arms are pinned without editing another lane's generator (the
 * `progress-arms.oracle.spec.ts` precedent). The definition row ids are the
 * ones the shared generator will carry; at the union a same-id row whose bytes
 * differ between the two files is a finding.
 *
 * Two row kinds, both byte-compared:
 *
 *   1. `definition` — the verdict, the parsed data's `JSON.stringify` (key
 *      ORDER pinned), the unknown-key report, and the FULL issue sentence;
 *   2. `placeholder` — `classifyPlaceholder`'s serialized ref.
 */

import { describe, expect, it } from 'vitest';

import corpusText from '../../testing/fixtures/pascal-character.oracle.ndjson';
import { collectUnknownKeys, formatDefinitionIssues, safeParse } from './custom-tool-types';
import { classifyPlaceholder } from './placeholders';

interface DefinitionRow {
  kind: 'definition';
  id: string;
  inputJson: string;
  success: boolean;
  reason: string | null;
  data: string | null;
  unknownKeys: string[];
}
interface PlaceholderRow {
  kind: 'placeholder';
  id: string;
  key: string;
  out: string;
}
type Row = DefinitionRow | PlaceholderRow;

const ROWS: Row[] = (corpusText as unknown as string)
  .split('\n')
  .filter((line) => line.trim() !== '')
  .map((line) => JSON.parse(line) as Row);

const definitions = ROWS.filter((r): r is DefinitionRow => r.kind === 'definition');
const placeholders = ROWS.filter((r): r is PlaceholderRow => r.kind === 'placeholder');

describe("Pascal's character arms agree with v4 row for row", () => {
  it('carries the whole recorded corpus', () => {
    expect(ROWS).toHaveLength(18);
    expect(definitions).toHaveLength(12);
    expect(placeholders).toHaveLength(6);
  });

  it.each(definitions)('definition — $id', (row) => {
    const doc = JSON.parse(row.inputJson);
    const result = safeParse(doc);
    expect(result.success).toBe(row.success);
    if (result.success) {
      expect(JSON.stringify(result.data)).toBe(row.data);
    } else {
      expect(formatDefinitionIssues(result.issues)).toBe(row.reason);
    }
    expect(collectUnknownKeys(doc)).toEqual(row.unknownKeys);
  });

  it.each(placeholders)('placeholder — $id', (row) => {
    expect(JSON.stringify(classifyPlaceholder(row.key))).toBe(row.out);
  });
});
