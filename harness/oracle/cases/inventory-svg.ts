/**
 * Oracle case (P4.D324, the Inventory keystone — v4 `4fbdad186`): the
 * allow-list SVG sanitiser `lib/inventory/svg.ts`, pure.
 *
 * Drives v4's REAL `sanitizeInventorySvg` and `inventorySvgDataUri` over the
 * eight `svg.test.ts` shapes plus a fuzz list (nested quotes, `<svg/>`, stray
 * closes, a DOCTYPE with `[`, an unterminated comment / CDATA / PI, mixed-case
 * tags, `xlink:href="#a"`, `style="fill:url(#g)"`, `style="background:url(x)"`,
 * `onClick`, a glyph one byte past 16 KiB, an unknown element closing an
 * allowed one, an entity-like `&amp;` vs a bare `&`, a `data:` href with
 * embedded whitespace, JS-only whitespace in attributes, non-ASCII text).
 * Each row: `{ok, value}` or `{ok: false, name, message}`, bytes exact.
 *
 * Run (Node 24, from the v4 checkout):
 *   N=~/.nvm/versions/node/v24.13.1/bin
 *   V5W=${V5W:-$HOME/source/quilltap-v5}
 *   cd ~/source/quilltap-server
 *   $N/node --import tsx $V5W/harness/oracle/cases/inventory-svg.ts \
 *     > /tmp/oracle-inv-svg.ndjson
 */

import { inventorySvgDataUri, sanitizeInventorySvg } from '@/lib/inventory/svg';
import { INVENTORY_SVG_MAX_BYTES } from '@/lib/schemas/inventory.types';

const BASELINE = 'p4.d324-inventory-svg';

function emit(row: Record<string, unknown>): void {
  process.stdout.write(`${JSON.stringify(row)}\n`);
}

const cases: Array<[string, string | null]> = [
  // svg.test.ts
  ['plain', '<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24"><path d="M2 2L22 22" stroke="black"/></svg>'],
  ['blank', '   '],
  ['null', null],
  ['script-foreign-style', '<svg><script>alert(1)</script><foreignObject><div>x</div></foreignObject><style>*{}</style><circle r="2"/></svg>'],
  ['handlers-hrefs', '<svg onload="alert(1)"><use href="https://evil.example/x.svg#a"/><use xlink:href="#ok"/><a href="javascript:alert(1)"><rect/></a></svg>'],
  ['style-url', '<svg><rect style="fill:url(https://x/y)" fill="url(http://x)"/><rect fill="url(#grad)"/></svg>'],
  ['comments-pi-doctype', '<?xml version="1.0"?><!DOCTYPE svg [<!ENTITY x "boom">]><!-- hi --><svg><g/></svg>'],
  ['not-svg', '<div>hi</div>'],
  ['over-cap-test', `<svg>${'<g></g>'.repeat(INVENTORY_SVG_MAX_BYTES / 4)}</svg>`],
  ['close-open', '<svg><g><circle r="1"/>'],
  // fuzz
  ['nested-quotes', `<svg><text title='say "hi"' data-x="it's">a > b</text></svg>`],
  ['self-closing-root', '<svg/>'],
  ['stray-closes', '</g></svg><svg></rect><g></g></svg></svg>'],
  ['doctype-brackets', '<!DOCTYPE svg [ <!ENTITY a "]>"> ]><svg/>'],
  ['unterminated-cdata', '<svg><g><![CDATA[ x'],
  ['unterminated-comment', '<svg><!-- never closed <rect/>'],
  ['unterminated-pi', '<svg><? never'],
  ['cdata-kept-out', '<svg><text><![CDATA[<b>x</b>]]>after</text></svg>'],
  ['mixed-case', '<SVG><LinearGradient id="g"><Stop offset="0"/></LinearGradient><Rect fill="url(#g)"/></SVG>'],
  ['xlink-fragment', '<svg><use xlink:href="#a"/><use xlink:href=" #b "/></svg>'],
  ['style-fragment-url', '<svg><rect style="fill:url(#g)"/></svg>'],
  ['style-background-url', '<svg><rect style="background:url(x)"/></svg>'],
  ['style-expression', '<svg><rect style="width:expression(alert(1))"/><rect style="@import x"/></svg>'],
  ['onclick-mixed-case', '<svg><rect onClick="x()" ONLOAD="y"/></svg>'],
  ['over-cap-by-one', `<svg><desc>${'x'.repeat(INVENTORY_SVG_MAX_BYTES - 17)}</desc></svg>`],
  ['at-cap', `<svg><desc>${'x'.repeat(INVENTORY_SVG_MAX_BYTES - 18)}</desc></svg>`],
  ['unknown-closes-allowed', '<svg><g><blink>x</g></blink><rect/></svg>'],
  ['unknown-nested-drop', '<svg><foo><g><rect/></g></foo><circle/></svg>'],
  ['entities', '<svg><text>&amp; & &lt; &#38; &#x26; &bogus; &;</text></svg>'],
  ['attr-entities', '<svg><g data-a="a&amp;b & c" data-b="<x>" data-c=\'q"q\'/></svg>'],
  ['data-href-whitespace', '<svg><use href="da ta:image/png;base64,AAAA"/><image href="#x"/></svg>'],
  ['javascript-tab', '<svg><a href="java\tscript:alert(1)"><rect/></a><g fill="JAVA&#x09;SCRIPT:x"/></svg>'],
  ['vbscript', '<svg><g fill="vbscript:x"/></svg>'],
  ['url-quoted-fragment', `<svg><rect fill="url('#g')" stroke='url("#h")' clip-path="url( #c)"/></svg>`],
  ['url-two', '<svg><rect fill="url(#a) url(http://x)"/></svg>'],
  ['bad-attr-names', '<svg><g 1bad="x" -x="y" good="z" a:b="c" .d="e"/></svg>'],
  ['unquoted-attrs', '<svg><g fill=red stroke=#000 width=10/></svg>'],
  ['bare-attr', '<svg><g hidden fill="red"/></svg>'],
  ['attr-equals-spacing', '<svg><g fill = "red"  stroke\n=\n"blue"/></svg>'],
  ['nbsp-attr-separator', '<svg><g fill="red"/></svg>'],
  ['bom-attr', '<svg><g﻿fill="red"/></svg>'],
  ['text-outside-root', 'hello <svg><g/></svg> trailing'],
  ['gt-in-text', '<svg><text>a > b</text></svg>'],
  ['tag-in-quoted-attr', '<svg><g data-x="<rect/>"/></svg>'],
  ['unterminated-tag', '<svg><g fill="red"'],
  ['non-ascii-text', '<svg><title>Été 🎩</title></svg>'],
  ['whitespace-around', '  \n<svg><g/></svg>\n  '],
  ['empty-tag-name', '<svg>< g/><>x</svg>'],
  ['close-with-space', '<svg><g></ g ></svg>'],
  ['upper-close', '<svg><g></G></svg>'],
  ['first-tag-not-svg-after-comment', '<!-- c --><g/><svg/>'],
  ['only-comment', '<!-- only -->'],
  ['only-text', 'just text'],
  ['script-with-tags', '<svg><script><![CDATA[ if (a < b) {} ]]></script><g/></svg>'],
  ['foreignobject-case', '<svg><foreignObject><svg><g/></svg></foreignObject><rect/></svg>'],
  ['marker-symbol-mask', '<svg><defs><marker id="m"/><symbol id="s"/><mask id="k"/><pattern id="p"/><clipPath id="c"/><radialGradient id="r"/></defs></svg>'],
  ['tspan-desc', '<svg><text><tspan>a</tspan></text><desc>d</desc></svg>'],
  ['image-dropped', '<svg><image href="#x"/><image/></svg>'],
];

emit({ kind: 'meta', baseline: BASELINE, maxBytes: INVENTORY_SVG_MAX_BYTES });
for (const [id, input] of cases) {
  let out: Record<string, unknown>;
  try {
    out = { ok: true, value: sanitizeInventorySvg(input) };
  } catch (e) {
    const err = e as Error;
    out = { ok: false, name: err.name, message: err.message };
  }
  emit({ kind: 'sanitize', id, input, out });
}
for (const [id, svg] of [
  ['plain', '<svg viewBox="0 0 1 1"><path d="M0 0"/></svg>'],
  ['specials', `<svg><text>a&b "q" 'p' (x) *!~_.-</text></svg>`],
  ['non-ascii', '<svg><title>Été 🎩</title></svg>'],
] as Array<[string, string]>) {
  emit({ kind: 'data_uri', id, svg, out: inventorySvgDataUri(svg) });
}
