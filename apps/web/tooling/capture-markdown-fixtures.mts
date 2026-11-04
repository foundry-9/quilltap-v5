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
 * The `math-*` fixtures couple to the KaTeX version the same way, which is why
 * v5 pins `katex` at v4's exact resolved version — **0.18.4** since v4's
 * `d339bad8` dependency refresh (see apps/web/package.json; re-read v4's
 * `package-lock.json`, not its `^`-ranged `package.json`, whenever this moves).
 * The math bytes themselves come from `rehype-katex`'s own nested `katex`
 * (0.16.47 in both trees at `48396682`) — the top-level pin is what keeps the
 * two trees resolving alike, so bump it in step and recapture. remark-math/
 * rehype-katex were added at v4 b8b12695; the single-dollar promotion
 * (`math-single-*`, `math-bare-token-*`) landed at v4 5915b04e — regenerate
 * this file from a v4 worktree at 7e6d13e5 or later.
 *
 * Last captured (P4.D352) from a pin worktree at `236c04e35` after the pin's
 * own `npm ci --offline`: the pin's ROOT `katex` is 0.19.1 (v5 still pins
 * 0.18.4), but `rehype-katex`'s nested `katex` is 0.16.47 as before, and every
 * `math-*` row came back byte-stable — ruling R-7 needed no bump. The
 * `template-novel-*` rows take their options from v4's own
 * `lib/chat/novel-template.ts` `NOVEL_TEMPLATE` — the object v4's seeder
 * inserts, never a hand-derived copy (its `renderingPatterns` JSON md5 at the
 * pin: `NOVEL_PATTERNS_MD5` below, logged on every run). The pre-existing rows
 * moved by exactly one at that pin: `escapes-in-brackets`, whose `*softly*` v4's
 * `778b97b2b` nesting now styles inside the `[…]` narration span.
 */

import { createHash } from 'node:crypto';
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

// v4's built-in Novel template, loaded from the same tree (v4 `778b97b2b`).
const { NOVEL_TEMPLATE } = (await import(
  pathToFileURL(resolve(V4_ROOT, 'lib/chat/novel-template.ts')).href
)) as {
  NOVEL_TEMPLATE: {
    renderingPatterns: NonNullable<RenderOptions['renderingPatterns']>;
    dialogueDetection: NonNullable<RenderOptions['dialogueDetection']>;
  };
};

/** md5 of the Novel patterns as JSON — the H-1 byte-compare handle. */
const NOVEL_PATTERNS_MD5 = createHash('md5')
  .update(JSON.stringify(NOVEL_TEMPLATE.renderingPatterns))
  .digest('hex');

/** Every `template-novel-*` row renders with exactly what v4 seeds. */
const NOVEL: RenderOptions = {
  renderingPatterns: NOVEL_TEMPLATE.renderingPatterns,
  dialogueDetection: NOVEL_TEMPLATE.dialogueDetection,
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
    scope?: 'inline' | 'line' | 'unmarked' | 'literal' | 'strip';
    hideDelimiters?: boolean;
  }[];
  dialogueDetection?: {
    openingChars: string[];
    closingChars: string[];
    className: string;
  } | null;
  /**
   * `smartTypographySettings.displayQuotes` (v4 Layer 1.6 Part A, `2d31810f`).
   * Absent is v4's own `displayQuotes = false` default — which is what every
   * pre-typography vector captured, so they stay the neutrality baseline.
   */
  displayQuotes?: boolean;
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

/** One message exercising every Novel arm at once. */
const NOVEL_SAMPLE = [
  'The rain had followed her all the way from the station. She paused at the door.',
  '',
  '"You came," Theo said. “I wasn\'t sure you would.”',
  '',
  '"Neither was I." She hung up her coat. "Your note said it was *urgent*.',
  '',
  '"And I meant it." A **loud** claim, and `a*b` stays code. ((brb, dinner))',
  '',
  '* * *',
  '',
  '- He wondered whether she had seen the letter.',
  '',
  '> *So he had read it.*',
].join('\n');

/** Novel rows, each paired with a `-reset` twin over the same input and no options. */
const NOVEL_ROWS: { name: string; input: string; options?: RenderOptions }[] = [
  ['template-novel-sample', NOVEL_SAMPLE],
  ['template-novel-dialogue-straight', '"You\'re late," he said, not looking up.'],
  ['template-novel-dialogue-curly', '“Well met, traveller,” she said.'],
  ['template-novel-dialogue-mixed', '“Hello," she said.'],
  ['template-novel-dialogue-unclosed-paragraph', '"I went home that night.\n\n"Then I slept until noon."'],
  ['template-novel-emphasis-inside-dialogue', '"I said *stop*!" she cried.'],
  ['template-novel-double-asterisks-stripped', 'A **loud** claim, and ***both*** at once.'],
  ['template-novel-single-asterisk-literal', 'It was *urgent*, she said.'],
  ['template-novel-scene-break-kept', 'Before the break.\n\n* * *\n\nAfter the break.'],
  ['template-novel-bullet-kept', '* an item *with* stress\n* another item'],
  ['template-novel-inline-code-untouched', 'Use `a*b` here, and *this*.'],
  ['template-novel-math-untouched', 'Inline $$a*b$$ math and *this*.'],
  ['template-novel-ooc', 'She waited. ((brb, dinner))'],
  ['template-novel-unmarked-narration-italic', 'She set the cup down. "You came," she said.'],
  ['template-novel-unmarked-in-li-and-blockquote', '- She ran.\n- "Wait!" he cried.\n\n> She paused at the door.\n\n## The *letter*'],
  ['template-novel-wrap-block-hidden-under-unmarked', '*Damn.*'],
].flatMap(([name, input]) => [
  { name, input, options: NOVEL },
  { name: `${name}-reset`, input, options: {} },
]);

/** Novel's patterns plus one hidden-prefix LINE rule (`stripUnmarkedSpans`). */
const NOVEL_PLUS_LINE: RenderOptions = {
  renderingPatterns: [
    ...NOVEL_TEMPLATE.renderingPatterns,
    { pattern: '^// (?<rpBody>.+)$', className: 'qt-chat-ooc', flags: 'm', scope: 'line', hideDelimiters: true },
  ],
  dialogueDetection: NOVEL_TEMPLATE.dialogueDetection,
};

/**
 * Novel's patterns plus three more inline wraps, so five rules can nest in one
 * another and the depth cap (`MAX_INLINE_NESTING = 3`) is reached: dialogue ›
 * OOC › `@@` › `%%` is the deepest styled chain, and `^^` inside it stays plain.
 */
const NOVEL_PLUS_THREE: RenderOptions = {
  renderingPatterns: [
    ...NOVEL_TEMPLATE.renderingPatterns,
    { pattern: '@@[^@]+@@', className: 'qt-chat-emote' },
    { pattern: '%%[^%]+%%', className: 'qt-chat-aside' },
    { pattern: '\\^\\^[^^]+\\^\\^', className: 'qt-chat-whisper' },
  ],
  dialogueDetection: NOVEL_TEMPLATE.dialogueDetection,
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
  // --- Layer 1.6 Part A (2d31810f): render-time quote curling ---------------
  // Each `quotes-on-*` vector is paired with an `-off` twin over the SAME input
  // with `displayQuotes: false`, so the pair proves the setting reached the
  // pipeline rather than the bytes merely looking plausible.
  {
    name: 'quotes-on-dialogue',
    input: '"Hello there," she said warmly.',
    options: { displayQuotes: true },
  },
  {
    name: 'quotes-off-dialogue',
    input: '"Hello there," she said warmly.',
    options: { displayQuotes: false },
  },
  {
    // smartypants' famous apostrophe guesses, captured rather than argued
    // about: this is precisely the cosmetic-wrong-guess class the feature
    // accepts because nothing is written down.
    name: 'quotes-on-apostrophes',
    input: "It's the '80s, isn't it? 'Tis so.",
    options: { displayQuotes: true },
  },
  {
    name: 'quotes-off-apostrophes',
    input: "It's the '80s, isn't it? 'Tis so.",
    options: { displayQuotes: false },
  },
  {
    // The insertion POINT, pinned: the typographer runs after remark-breaks, so
    // it sees the text already split around a `break` node. Quote pairing
    // across that split is what would move if the plugin were reordered.
    name: 'quotes-on-soft-break',
    input: '"Hello\nthere," she said.',
    options: { displayQuotes: true },
  },
  {
    // `dashes: false`, permanently — `run it with --verbose` must survive being
    // written in prose. Part B handles dashes at the keystroke instead.
    name: 'quotes-on-dashes-untouched',
    input: 'Run it with --verbose --- honestly, "yes".',
    options: { displayQuotes: true },
  },
  {
    // `ellipses: false` — Part B owns `...`, where it becomes a real character
    // in the writer's text rather than a display substitution.
    name: 'quotes-on-ellipses-untouched',
    input: 'Wait... "really"?',
    options: { displayQuotes: true },
  },
  {
    // Structural skip 1: an `inlineCode` node is not a text node.
    name: 'quotes-on-inline-code',
    input: 'Run `git commit -m "wip"` and then say "done".',
    options: { displayQuotes: true },
  },
  {
    // Structural skip 2: a fenced `code` node is not a text node either.
    name: 'quotes-on-code-fence',
    input: '```\nprint("hello")\n```\n\nAnd "afterwards".',
    options: { displayQuotes: true },
  },
  {
    // Structural skip 3: math is its own node type, and the raw LaTeX survives
    // into KaTeX's <annotation> — a string transform here would have eaten it.
    name: 'quotes-on-math',
    input: 'Behold \\(\\text{"x"}\\) and then "prose".',
    options: { displayQuotes: true },
  },
  {
    // Structural skip 4: a link target is a `url` PROPERTY, not a text node.
    name: 'quotes-on-link-url',
    input: 'Visit [the site](https://example.com/a\'b) and say "hello".',
    options: { displayQuotes: true },
  },
  {
    // Suppression rule 1: a wrap pattern with an `rpBody` group whose delimiter
    // is a straight quote. Curling would desynchronize its escape pass from its
    // match pass, so the typographer stands down for the WHOLE render.
    name: 'quotes-suppressed-by-rpbody-quote-delimiter',
    input: '"she said softly" and then "again".',
    options: {
      displayQuotes: true,
      renderingPatterns: [{ pattern: '"(?<rpBody>[^"]+)"', className: 'qt-chat-narration' }],
    },
  },
  {
    // The negative control for rule 1: an `rpBody` wrap that claims `%%`, not a
    // quote, must NOT suppress.
    name: 'quotes-not-suppressed-by-safe-rpbody',
    input: '%%she said softly%% and then "again".',
    options: {
      displayQuotes: true,
      renderingPatterns: [{ pattern: '%%(?<rpBody>[^%]+)%%', className: 'qt-chat-narration' }],
    },
  },
  {
    // Suppression rule 2: a dialogue detection naming the straight quote
    // WITHOUT its curly counterpart — the bug-62 shape arriving via a custom
    // template instead of via the shipped fallback.
    name: 'quotes-suppressed-by-straight-only-detection',
    input: '"Hello there," she said warmly.',
    options: {
      displayQuotes: true,
      dialogueDetection: {
        openingChars: ['"'],
        closingChars: ['"'],
        className: 'qt-chat-dialogue',
      },
    },
  },
  {
    // The negative control for rule 2: a detection that declares the curly
    // forms alongside the straight one is safe, so curling proceeds.
    name: 'quotes-not-suppressed-by-curly-aware-detection',
    input: '"Hello there," she said warmly.',
    options: {
      displayQuotes: true,
      dialogueDetection: {
        openingChars: ['"', '“'],
        closingChars: ['"', '”'],
        className: 'qt-chat-dialogue',
      },
    },
  },
  {
    // A detection naming NO quote at all is not quote-sensitive: guillemets
    // cannot stop matching because of curling.
    name: 'quotes-not-suppressed-by-guillemet-detection',
    input: '«Bonjour» and "hello".',
    options: { displayQuotes: true, dialogueDetection: CUSTOM_DIALOGUE },
  },
  // --- P4.D352: the Novel template (v4 778b97b2b) ----------------------------
  ...NOVEL_ROWS,
  {
    name: 'template-novel-line-rule-strips-unmarked',
    input: '// an aside\n\nShe ran.',
    options: NOVEL_PLUS_LINE,
  },
  { name: 'template-novel-line-rule-strips-unmarked-reset', input: '// an aside\n\nShe ran.', options: {} },
  {
    name: 'template-novel-nesting-depth-cap',
    input: '"a ((b @@c %%d ^^e^^ d%% c@@ b)) a"',
    options: NOVEL_PLUS_THREE,
  },
  { name: 'template-novel-nesting-depth-cap-reset', input: '"a ((b @@c %%d ^^e^^ d%% c@@ b)) a"', options: {} },
  {
    // Captured as v4 renders it: a `*` inside TWO shown-delimiter wraps
    // (dialogue › OOC) is escaped by each wrap's interior pass, so the second
    // backslash survives as text and Markdown makes the run an `<em>`.
    name: 'template-novel-emphasis-in-ooc-in-dialogue',
    input: '"a ((b *c* b)) a"',
    options: NOVEL,
  },
  { name: 'template-novel-emphasis-in-ooc-in-dialogue-reset', input: '"a ((b *c* b)) a"', options: {} },
  {
    // `mentionsGlyph`: Novel's dialogue wrap names the curly forms, so curling
    // proceeds under it — and its pattern still matches the curled quotes.
    name: 'quotes-on-novel-dialogue',
    input: '"You came," she said. "I wasn\'t sure."',
    options: { ...NOVEL, displayQuotes: true },
  },
  { name: 'quotes-on-novel-dialogue-reset', input: '"You came," she said. "I wasn\'t sure."', options: {} },
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
  console.log(`Novel renderingPatterns md5 ${NOVEL_PATTERNS_MD5}`);
}

void main();
