/**
 * Capture v4 → v5 markdown-render parity fixtures.
 *
 * Drives v4's REAL `renderMarkdownToHtml`
 * (`lib/services/markdown-renderer.service.ts`) over a fixed corpus and writes
 * `{ input, html }` pairs, which `markdown-renderer.spec.ts` byte-compares against
 * the v5 port. This is the tier-4/5 "capture from v4" step.
 *
 * Regenerate (from the v4 checkout so its tsconfig `@/` aliases resolve):
 *   cd ~/source/quilltap-server && npx tsx \
 *     ~/source/quilltap-v5/apps/web/tooling/capture-markdown-fixtures.mts
 *
 * The corpus deliberately avoids language-LABELED code fences: rehype-highlight's
 * output depends on the transitive highlight.js version, so labeled blocks would
 * couple the fixtures to a highlighter version. Unlabeled fences (detect:false)
 * are plain <pre><code> and safe to pin.
 *
 * The `math-*` fixtures couple to the KaTeX version the same way — KaTeX 0.18's
 * markup is what these bytes pin — which is why v5 pins `katex` at v4's exact
 * resolved 0.18.0 (see apps/web/package.json). remark-math/rehype-katex were
 * added at v4 b8b12695; regenerate this file from a v4 checkout at that baseline.
 */

import { writeFileSync, mkdirSync } from 'node:fs';
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

// v4's real server renderer (absolute path into the reference checkout).
import { renderMarkdownToHtml } from '/Users/csebold/source/quilltap-server/lib/services/markdown-renderer.service.ts';

const CORPUS: { name: string; input: string }[] = [
  { name: 'plain', input: 'Just a plain sentence.' },
  { name: 'bold', input: 'This is **bold** text.' },
  { name: 'italic-narration', input: 'She *looks around* nervously.' },
  { name: 'bracket-narration', input: 'A hush falls. [the door creaks open]' },
  { name: 'ooc-parens', input: 'Right then. ((this is just me talking))' },
  { name: 'ooc-line', input: '// a meta note on its own line' },
  { name: 'monologue', input: 'He paused. {What do I even say now?}' },
  { name: 'dialogue-straight', input: '"Hello there," she said warmly.' },
  { name: 'dialogue-curly', input: '“Well met, traveller.”' },
  { name: 'mixed-line', input: 'She smiled. "Come in," *stepping aside* to let him pass.' },
  { name: 'qtap-bare', input: 'See qtap://project/notes/plan.md for the outline.' },
  { name: 'qtap-in-code', input: 'Inline `qtap://x/y` should stay literal.' },
  { name: 'md-link', input: 'Visit [the site](https://example.com) sometime.' },
  { name: 'heading', input: '# A Heading\n\nAnd a paragraph.' },
  { name: 'ul', input: '- one\n- two\n- three' },
  { name: 'ol', input: '1. first\n2. second' },
  { name: 'blockquote', input: '> a quiet aside' },
  { name: 'strikethrough', input: 'This is ~~struck~~ through.' },
  { name: 'table', input: '| A | B |\n| - | - |\n| 1 | 2 |' },
  { name: 'code-fence-plain', input: '```\nplain code block\nno language\n```' },
  { name: 'soft-break', input: 'line one\nline two' },
  { name: 'inline-code', input: 'Use the `render()` function.' },
  { name: 'escapes-in-brackets', input: '[she said *softly* with _weight_]' },
  // --- math (b8b12695): remark-math + rehype-katex, normalizeMathDelimiters ---
  { name: 'math-inline-paren', input: 'Euler said \\(e^{i\\pi} + 1 = 0\\) once.' },
  { name: 'math-display-bracket', input: 'Behold the identity \\[x = 2\\] in a paragraph.' },
  { name: 'math-dollar-inline', input: 'Inline $$a^2 + b^2$$ math.' },
  { name: 'math-dollar-block', input: '$$\n\\int_0^1 x\\,dx\n$$' },
  { name: 'currency-prose', input: 'He slid $50 across the table, then another $20.' },
  { name: 'math-in-code-span', input: 'Use `\\(escaped\\)` literally.' },
  { name: 'math-in-fence', input: '```\n\\(not math\\)\n```' },
  {
    name: 'math-matrix-rowspacing',
    input: '$$\n\\begin{pmatrix} a \\\\[3pt] b \\end{pmatrix}\n$$',
  },
  { name: 'math-invalid-tex', input: 'Broken math: \\(\\frac{1}\\) here.' },
  { name: 'math-with-narration', input: '*She writes* \\(x = 1\\) on the board.' },
  { name: 'math-in-dialogue', input: '"The answer is \\(x^2\\)," she said.' },
];

async function main() {
  const out: { name: string; input: string; html: string }[] = [];
  for (const c of CORPUS) {
    const html = await renderMarkdownToHtml(c.input);
    out.push({ name: c.name, input: c.input, html });
  }

  const here = dirname(fileURLToPath(import.meta.url));
  const target = resolve(
    here,
    '../src/app/chat/render/__fixtures__/markdown-fixtures.json',
  );
  mkdirSync(dirname(target), { recursive: true });
  writeFileSync(target, JSON.stringify(out, null, 2) + '\n', 'utf8');
  console.log(`Wrote ${out.length} fixtures to ${target}`);
}

void main();
