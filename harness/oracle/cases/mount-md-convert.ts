/**
 * P4.6y MARKDOWN-CONVERTER ORACLE (tier-1 exact): drives v4's REAL
 * `convertMarkdownToText` (`lib/mount-index/converters/markdown-converter.ts`)
 * over a fixed corpus of syntax-edge inputs (each written to a temp .md file —
 * the converter reads from disk), emitting `{id, input, out}` NDJSON so the
 * Rust `services::mount_index::converters::markdown_to_text` port diffs
 * byte-for-byte. The corpus deliberately hammers the two backreference regexes
 * the Rust side hand-rolls (code fences, bold/italic) plus the JS-`\s`
 * class edges.
 *
 * Run (Node 24, from the v4 checkout):
 *   N=~/.nvm/versions/node/v24.13.1/bin
 *   W=<worktree>
 *   cd ~/source/quilltap-server
 *   $N/node --import tsx $W/harness/oracle/cases/mount-md-convert.ts \
 *     > /tmp/oracle-mount-md-convert.ndjson
 */

import { mkdtempSync, writeFileSync, rmSync } from 'node:fs';
import { join } from 'node:path';
import { tmpdir } from 'node:os';

const CORPUS: Array<[string, string]> = [
  ['plain', 'Just a plain paragraph.\nSecond line.\n'],
  ['frontmatter', '---\ntitle: X\nembed: false\n---\nBody after frontmatter.\n'],
  ['frontmatter-unclosed', '---\ntitle: X\nnever closed\n'],
  ['heading', '# One\n\n## Two\n\n###### Six\n\n####### Seven hashes\n'],
  ['fence-basic', 'before\n```rust\nlet x = 1;\n```\nafter\n'],
  ['fence-tilde', '~~~\ncode here\n~~~\ntail\n'],
  ['fence-longer-close', '````\ninner ``` fence\n````\nend\n'],
  ['fence-unclosed', '```\nnever closes\n'],
  ['fence-trailing-ws', '```js\ncode\n```   \nnext\n'],
  ['fence-blank-after', '```\nc\n```\n\nafter blank\n'],
  ['emphasis-mix', '**bold**, *it*, ***both***, __under__, _one_, ****x****\n'],
  ['emphasis-edge', 'a * b, *, ** **, *a*b*, * leading space*\n'],
  ['emphasis-nested-syms', '*a_b*, _a*b_, **a _b_ c**\n'],
  ['links', '[text](http://x) ![alt](img.png) [ref][r] [r]: http://y\n'],
  ['ref-def-line', '[r1]: https://example.com/one\nbody\n'],
  ['html', 'keep <b>bold html</b> and <div class="x">content</div>\n'],
  ['inline-code', 'use `let x` here, and ``double? no—single only`\n'],
  ['hrule', 'above\n---\nbelow\n***\nend\n___\n'],
  ['blockquote', '> quoted line\n>another\n> \n'],
  ['lists', '- a\n* b\n+ c\n  - indented\n1. one\n12. twelve\n'],
  ['blank-collapse', 'a\n\n\n\nb\n'],
  ['ws-edges', ' 　padded 　\n'],
  ['bom', '﻿# BOM heading\ncontent\n'],
  ['unicode', '# Émoji 🎉 héading\n\n**gras** avec *accents* — тест 🚀 done\n'],
  ['empty', ''],
  ['whitespace-only', '   \n\t\n'],
  ['crlf-ish', 'line one\nline two with trailing \nend\n'],
  ['fence-info-string', '```python title="x"\nprint(1)\n```\ndone\n'],
];

async function main(): Promise<void> {
  const { convertMarkdownToText } = await import('@/lib/mount-index/converters');
  const dir = mkdtempSync(join(tmpdir(), 'qt-md-convert-'));
  for (const [id, input] of CORPUS) {
    const p = join(dir, `${id}.md`);
    writeFileSync(p, input, 'utf8');
    const out = await convertMarkdownToText(p);
    process.stdout.write(JSON.stringify({ id, input, out }) + '\n');
  }
  rmSync(dir, { recursive: true, force: true });
  process.exit(0);
}

main().catch((err) => {
  process.stderr.write(`mount-md-convert oracle failed: ${err?.stack ?? err}\n`);
  process.exit(1);
});
