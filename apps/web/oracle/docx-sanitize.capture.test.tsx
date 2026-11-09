/**
 * Tier-1 oracle — v4's `.docx` reading-copy sanitizer (P4.D387 item 4).
 *
 * v4 `c34986421` shows a `.docx` read-only: the server renders it to HTML with
 * mammoth (`?action=render-html`) and the client sanitises that HTML AGAIN
 * with DOMPurify (`components/files/FilePreview/FilePreviewDocx.tsx`
 * `sanitizeDocxHtml`), since there is no DOM on the server. DOMPurify's output
 * depends on the DOM it runs in, so this capture runs v4's REAL
 * `sanitizeDocxHtml` under v4's own jest jsdom environment and records each
 * input's output; `src/app/document-browser/sanitize-docx.spec.ts` compares
 * v5's private-instance twin byte for byte under vitest's jsdom.
 *
 * ## Regenerating
 *
 * jest ignores paths outside the checkout, so this file is COPIED into a v4
 * worktree pinned at `624d0d25c` (drift-ledger §5.1; the pin takes its own
 * `npm ci`, never a live `node_modules` symlink):
 *
 * ```bash
 * V5W=${V5W:-$HOME/source/quilltap-v5}
 * PIN=/tmp/qt-v4-pin-p4.d387-624d0d25c
 * cp "$V5W/apps/web/oracle/docx-sanitize.capture.test.tsx" \
 *    "$PIN/__tests__/unit/zz-p4d387-docx-sanitize.capture.test.tsx"
 * OUT="$V5W/apps/web/src/app/document-browser/__fixtures__/docx-sanitize.v4.json"
 * rm -f "$OUT"
 * (cd "$PIN" && PATH=$HOME/.nvm/versions/node/v24.13.1/bin:$PATH TZ=UTC \
 *    P4D387_DOCX_OUT="$OUT" npx jest --watchman=false --testTimeout=180000 \
 *    --runTestsByPath __tests__/unit/zz-p4d387-docx-sanitize.capture.test.tsx)
 * rm -f "$PIN/__tests__/unit/zz-p4d387-docx-sanitize.capture.test.tsx"
 * node -p "require('$OUT').length"     # expect 24
 * ```
 *
 * Record the pin's `node_modules/dompurify/package.json` version beside the
 * run (3.4.17 at the 2026-11-08 capture, equal to v5's). Then `npm test` in
 * `apps/web`. Fix the PORT, never the recorded JSON; a byte red from a
 * DOMPurify version skew is recorded as `DOMPURIFY_VERSION_SKEW`.
 *
 * @jest-environment jsdom
 */
import fs from 'fs'
import path from 'path'

import { sanitizeDocxHtml } from '@/components/files/FilePreview/FilePreviewDocx'

const OUT = process.env.P4D387_DOCX_OUT || '/tmp/p4d387-docx-sanitize.v4.json'

const CORPUS: Array<[string, string]> = [
  ['script', '<p>before</p><script>alert(1)</script><p>after</p>'],
  ['onclick', '<p onclick="alert(1)">click</p>'],
  ['style element', '<style>body{display:none}</style><p>styled</p>'],
  ['style attribute', '<p style="color:red">red</p>'],
  ['form controls', '<form action="/x"><input name="a" value="b"><button>Go</button></form><p>tail</p>'],
  ['iframe', '<iframe src="https://example.com"></iframe><p>x</p>'],
  ['object', '<object data="x.pdf"></object><p>x</p>'],
  ['embed', '<embed src="x.swf"><p>x</p>'],
  ['javascript href', '<a href="javascript:alert(1)">js</a>'],
  ['data html href', '<a href="data:text/html;base64,PHNjcmlwdD4=">data</a>'],
  ['data image src', '<img src="data:image/png;base64,iVBORw0KGgo=" alt="dot">'],
  ['mailto', '<a href="mailto:someone@example.com">mail</a>'],
  ['fragment', '<a href="#section-2">jump</a>'],
  ['https', '<a href="https://example.com/a?b=c">site</a>'],
  ['http', '<a href="http://example.com">plain</a>'],
  ['relative href', '<a href="notes/other.docx">relative</a>'],
  ['relative img', '<img src="images/a.png" alt="a">'],
  [
    'mammoth typical',
    '<h1>Title</h1><h2>Sub</h2><h3>Third</h3><p><strong>Bold</strong> and <em>italic</em>.</p><ul><li>one</li><li>two</li></ul><ol><li>first</li></ol><table><tr><td>a</td><td>b</td></tr></table><p><img src="data:image/jpeg;base64,/9j/4AAQ" /></p>',
  ],
  ['svg onload', '<svg onload="alert(1)"><circle r="2"></circle></svg><p>after</p>'],
  ['encoded javascript', '<a href="&#106;avascript:alert(1)">enc</a>'],
  ['amp-encoded javascript', '<a href="javascript&amp;colon;alert(1)">amp</a>'],
  ['entity colon javascript', '<a href="javascript&colon;alert(1)">colon</a>'],
  ['empty', ''],
  ['plain text', 'just words & an ampersand'],
]

describe('docx sanitize capture (v4 624d0d25c)', () => {
  it('records sanitizeDocxHtml over the corpus', () => {
    const rows = CORPUS.map(([id, input]) => ({ id, input, output: sanitizeDocxHtml(input) }))
    fs.mkdirSync(path.dirname(OUT), { recursive: true })
    fs.writeFileSync(OUT, JSON.stringify(rows, null, 2) + '\n')
    expect(rows).toHaveLength(24)
  })
})
