/**
 * Capture v4 → v5 markdown-render parity fixtures.
 *
 * Drives v4's REAL `renderMarkdownToHtml`
 * (`lib/services/markdown-renderer.service.ts`) over a fixed corpus and writes
 * `{ input, html }` pairs, which `markdown-renderer.spec.ts` byte-compares against
 * the v5 port. This is the tier-4/5 "capture from v4" step.
 *
 * Regenerate — run tsx FROM the v4 checkout (its cwd is what resolves the `@/`
 * tsconfig aliases the renderer's own imports use), and point `QT_V4_ROOT` at
 * the SAME tree so the capture reads v4's real renderer:
 *   cd ~/source/quilltap-server && QT_V4_ROOT=$PWD ./node_modules/.bin/tsx \
 *     <v5>/apps/web/tooling/capture-markdown-fixtures.mts
 *
 * On drift (v4 HEAD past the round's baseline, or a dirty tree) make a detached
 * worktree at the baseline and use it for BOTH — `cd <pin> && QT_V4_ROOT=$PWD …`
 * (the `oracle-regen-pinned-v4-worktree` recipe). `QT_V4_ROOT` defaults to
 * `~/source/quilltap-server`. This used to be a hard-coded `/private/tmp/qt-v4-
 * pin-7e6d13e5` import, which no longer exists — a pinned /tmp worktree never
 * survives a round, so the path is a parameter now.
 *
 * The corpus deliberately avoids language-LABELED code fences: rehype-highlight's
 * output depends on the transitive highlight.js version, so labeled blocks would
 * couple the fixtures to a highlighter version. Unlabeled fences (detect:false)
 * are plain <pre><code> and safe to pin.
 *
 * The `math-*` fixtures couple to the KaTeX version the same way — KaTeX 0.18's
 * markup is what these bytes pin — which is why v5 pins `katex` at v4's exact
 * resolved 0.18.1 (see apps/web/package.json). remark-math/rehype-katex were
 * added at v4 b8b12695; the single-dollar promotion (`math-single-*`,
 * `math-bare-token-*`) landed at v4 5915b04e — regenerate this file from a v4
 * worktree at 7e6d13e5 or later.
 */

import { writeFileSync, mkdirSync } from 'node:fs';
import { homedir } from 'node:os';
import { dirname, resolve } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';

/** The v4 checkout (or pinned baseline worktree) whose renderer is the oracle. */
const V4_ROOT = process.env['QT_V4_ROOT'] ?? resolve(homedir(), 'source/quilltap-server');

// v4's real server renderer, loaded from that tree. Dynamic so the path is a
// parameter — see the module note.
const { renderMarkdownToHtml } = (await import(
  pathToFileURL(resolve(V4_ROOT, 'lib/services/markdown-renderer.service.ts')).href
)) as {
  renderMarkdownToHtml: (content: string, options?: RenderOptions) => Promise<string>;
};

/**
 * The render options a corpus entry may pin. Mirrors v4's
 * `MarkdownRenderOptions` — the two template-driven inputs P4.30 threads from
 * the chat's roleplay template into every rendered row. `undefined` (the key
 * absent) is the "no template / reset" arm; `[]` and `null` are v4's two
 * fallback arms, which are deliberately NOT the same shape (an empty patterns
 * array falls back to the defaults, an explicit-null dialogueDetection does
 * too — `MessageContent.tsx:333-338`).
 */
interface RenderOptions {
  renderingPatterns?: {
    pattern: string;
    className: string;
    flags?: string;
    scope?: 'inline' | 'line';
    hideDelimiters?: boolean;
  }[];
  dialogueDetection?: {
    openingChars: string[];
    closingChars: string[];
    className: string;
  } | null;
}

/**
 * A custom pattern set that shares NO delimiter with the defaults, so a vector
 * rendered with it and the same vector rendered with none (the reset arm) are
 * byte-distinguishable. `@@`/`::` are deliberately markdown- and HTML-inert:
 * the roleplay pass runs over rendered HTML, so a delimiter containing `<`/`>`
 * would already be entity-escaped and could never match.
 */
const CUSTOM_PATTERNS: RenderOptions['renderingPatterns'] = [
  { pattern: '@@[^@]+@@', className: 'qt-chat-emote' },
  { pattern: '^:: .+$', className: 'qt-chat-aside', flags: 'm', scope: 'line' },
];

/** A custom paragraph-level dialogue detection using guillemets. */
const CUSTOM_DIALOGUE: RenderOptions['dialogueDetection'] = {
  openingChars: ['«'],
  closingChars: ['»'],
  className: 'qt-chat-custom-dialogue',
};

const CORPUS: { name: string; input: string; options?: RenderOptions }[] = [
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
  // --- single-dollar promotion (5915b04e): promoteSingleDollarMath ---
  { name: 'math-single-dollar-command', input: 'where $\\mathcal{P}$ is the invariant' },
  { name: 'math-single-dollar-scripts', input: 'the term $T_{CMB}$ and $x^2$' },
  { name: 'math-single-dollar-currency-mix', input: 'The $50 fee scales as $\\pi r^2$ per unit.' },
  { name: 'math-bare-token-companion', input: 'where $K$ is the scalar and $\\mathcal{P}$ the invariant' },
  { name: 'math-bare-token-alone', input: 'where $K$ is the Kretschmann scalar' },
  { name: 'math-single-dollar-in-code', input: 'Run `echo $\\HOME{}` please.' },
  // --- P4.30: the chat's roleplay template threaded into the render ---------
  // Each custom vector is paired with a `-reset` twin over the SAME input and
  // NO options, so the pair is the proof that the template reached the render
  // (v4's three reset arms all land on this second shape).
  {
    name: 'template-custom-patterns-inline',
    input: 'She paused. @@leans in@@ and whispered.',
    options: { renderingPatterns: CUSTOM_PATTERNS },
  },
  {
    name: 'template-custom-patterns-inline-reset',
    input: 'She paused. @@leans in@@ and whispered.',
    options: {},
  },
  {
    // The custom set REPLACES the defaults: `*looks around*` must come back a
    // bare <em>, with no qt-chat-narration span.
    name: 'template-custom-patterns-supersede-defaults',
    input: 'She *looks around* and @@leans in@@.',
    options: { renderingPatterns: CUSTOM_PATTERNS },
  },
  {
    name: 'template-custom-patterns-line-scope',
    input: ':: a stage direction on its own line',
    options: { renderingPatterns: CUSTOM_PATTERNS },
  },
  {
    name: 'template-custom-patterns-line-scope-reset',
    input: ':: a stage direction on its own line',
    options: {},
  },
  {
    // The empty-array fallback arm: identical bytes to `bracket-narration`.
    name: 'template-empty-patterns-falls-back',
    input: 'A hush falls. [the door creaks open]',
    options: { renderingPatterns: [] },
  },
  {
    // hideDelimiters + the named `rpBody` group: the whole block is one wrap
    // span and the `%%` delimiters are dropped from the output.
    name: 'template-custom-patterns-hide-delimiters',
    input: '%%he turns to the window%%',
    options: {
      renderingPatterns: [
        { pattern: '%%(?<rpBody>[^%]+)%%', className: 'qt-chat-narration', hideDelimiters: true },
      ],
    },
  },
  {
    // The same named group WITHOUT hideDelimiters drives the rpBody-interior
    // markdown escape instead (`escapeMarkdownInBrackets`).
    name: 'template-custom-patterns-rpbody-escape',
    input: '%%she said *softly* now%% and left.',
    options: {
      renderingPatterns: [{ pattern: '%%(?<rpBody>[^%]+)%%', className: 'qt-chat-narration' }],
    },
  },
  {
    name: 'template-custom-dialogue-detection',
    input: '«Bonjour, mon ami»',
    options: { dialogueDetection: CUSTOM_DIALOGUE },
  },
  {
    name: 'template-custom-dialogue-detection-reset',
    input: '«Bonjour, mon ami»',
    options: {},
  },
  {
    // The `||` fallback arm: an explicit null is the default detection.
    name: 'template-null-dialogue-detection-falls-back',
    input: '"Hello there," she said warmly.',
    options: { dialogueDetection: null },
  },
];

async function main() {
  const out: { name: string; input: string; options?: RenderOptions; html: string }[] = [];
  for (const c of CORPUS) {
    // `undefined` options exercise v4's own default (`options = {}`), which is
    // exactly what the pre-P4.30 vectors captured — so they come back
    // byte-identical and stay the baseline-neutrality proof.
    const html = await renderMarkdownToHtml(c.input, c.options);
    out.push(c.options ? { name: c.name, input: c.input, options: c.options, html } : { name: c.name, input: c.input, html });
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
