import { describe, expect, it } from 'vitest';

import capture from './__fixtures__/docx-sanitize.v4.json';
import { sanitizeDocxHtml } from './sanitize-docx';

/**
 * The tier-1 SPA differential for the `.docx` reading-copy sanitizer (P4.D387
 * item 4): v4's REAL `components/files/FilePreview/FilePreviewDocx.tsx`
 * `sanitizeDocxHtml`, run under v4's jest jsdom at the pin `624d0d25c` by
 * `apps/web/oracle/docx-sanitize.capture.test.tsx` (its header has the
 * recipe), compared byte for byte with v5's private-instance twin under
 * vitest's jsdom. DOMPurify is 3.4.17 on both sides at the capture; a byte red
 * from a version skew would be `DOMPURIFY_VERSION_SKEW`, recorded, never
 * patched around.
 */

interface Row {
  id: string;
  input: string;
  output: string;
}

const ROWS = capture as Row[];

describe('sanitizeDocxHtml (v4 624d0d25c FilePreviewDocx)', () => {
  it('carries the captured corpus whole', () => {
    expect(ROWS.length).toBe(24);
  });

  it.each(ROWS.map((r) => [r.id, r] as const))('%s', (_id, row) => {
    expect(sanitizeDocxHtml(row.input)).toBe(row.output);
  });

  it('never touches the shared DOMPurify (a private instance)', async () => {
    const shared = (await import('dompurify')).default;
    const before = shared.removed.length;
    sanitizeDocxHtml('<script>x</script>');
    expect(shared.removed.length).toBe(before);
  });
});
