import { describe, expect, it } from 'vitest';

import { normalizeMathDelimiters } from './math';
import { applyRoleplayPatterns } from './markdown-renderer';
import { DEFAULT_RENDERING_PATTERNS, compileRenderingPatterns } from './roleplay-rendering';

/**
 * Case-for-case port of v4's `__tests__/unit/lib/markdown/math.test.ts` (HEAD
 * `b8b12695`, 14 cases):
 * - normalizeMathDelimiters (`./math`) — the `\(...\)`/`\[...\]` → `$$` rewrite
 *   step 2.5 runs before parsing.
 * - applyRoleplayPatterns (`./markdown-renderer`) — its KaTeX-subtree skip,
 *   which keeps roleplay patterns from rewriting rendered math markup. v4 keeps
 *   this helper in `lib/services/markdown-postprocess.ts` (split out so Jest can
 *   import it past the service's ESM-only unified deps); v5 keeps it inline in
 *   the renderer (its spec runner is vitest, which loads ESM fine).
 *
 * The full unified pipeline (remark-math/rehype-katex) is exercised by the
 * captured-v4-bytes fixtures in `markdown-renderer.spec.ts`; these tests pin the
 * import-safe pieces.
 */

describe('normalizeMathDelimiters', () => {
  it('returns input unchanged when no backslash delimiters are present', () => {
    const input = 'Plain prose with $50 and even $$x^2$$ math.';
    expect(normalizeMathDelimiters(input)).toBe(input);
  });

  it('converts \\(...\\) to inline $$ math', () => {
    expect(normalizeMathDelimiters('Euler said \\(e^{i\\pi} + 1 = 0\\) once.')).toBe(
      'Euler said $$e^{i\\pi} + 1 = 0$$ once.',
    );
  });

  it('converts \\[...\\] to block $$ math on its own lines', () => {
    expect(normalizeMathDelimiters('Behold: \\[x = 2\\] indeed.')).toBe(
      'Behold: \n$$\nx = 2\n$$\n indeed.',
    );
  });

  it('trims interior whitespace from multi-line display math', () => {
    expect(normalizeMathDelimiters('\\[\n\\sum_{n=1}^\\infty n\n\\]')).toBe(
      '\n$$\n\\sum_{n=1}^\\infty n\n$$\n',
    );
  });

  it('leaves delimiters inside inline code spans alone', () => {
    const input = 'Use `\\(escaped\\)` in code.';
    expect(normalizeMathDelimiters(input)).toBe(input);
  });

  it('leaves delimiters inside fenced code blocks alone', () => {
    const input = '```\n\\(not math\\)\n```';
    expect(normalizeMathDelimiters(input)).toBe(input);
  });

  it('leaves an unterminated fence (streaming) alone', () => {
    const input = 'Look:\n```\n\\(still streaming\\)';
    expect(normalizeMathDelimiters(input)).toBe(input);
  });

  it('leaves an unterminated $$ region (streaming) alone', () => {
    // Not one of v4's 14 cases — an explicit streaming-tolerance pin (tier 2.8):
    // the `$$…$$|$` skip arm swallows the rest of the string, so `\(…\)` still
    // being received inside an open display block is never half-transformed.
    const input = 'Start $$\n\\(still math\\)';
    expect(normalizeMathDelimiters(input)).toBe(input);
  });

  it('does not rewrite \\[-lookalikes inside existing $$ math', () => {
    // `\\[3pt]` is LaTeX row spacing; a naive rewrite would mangle the matrix.
    const input = '$$\n\\begin{pmatrix} a \\\\[3pt] b \\end{pmatrix}\n$$ then \\(x\\)';
    expect(normalizeMathDelimiters(input)).toBe(
      '$$\n\\begin{pmatrix} a \\\\[3pt] b \\end{pmatrix}\n$$ then $$x$$',
    );
  });

  it('converts multiple delimiters in one string', () => {
    expect(normalizeMathDelimiters('\\(a\\) and \\(b\\)')).toBe('$$a$$ and $$b$$');
  });
});

describe('applyRoleplayPatterns KaTeX skip', () => {
  const compiledRules = compileRenderingPatterns(DEFAULT_RENDERING_PATTERNS);

  // Trimmed-down but structurally faithful KaTeX output: MathML with the raw
  // LaTeX in <annotation>, plus HTML glyph spans. The LaTeX contains `{a}` and
  // `*b*` runs that the default inner-monologue/narration patterns would match.
  const katexInline =
    '<span class="katex">' +
    '<span class="katex-mathml"><math><semantics><mrow><mi>a</mi></mrow>' +
    '<annotation encoding="application/x-tex">\\frac{a}{b} + *b* + [c]</annotation>' +
    '</semantics></math></span>' +
    '<span class="katex-html" aria-hidden="true"><span class="base">' +
    '<span class="strut" style="height:0.6833em;"></span>' +
    '<span class="mord">{a} and "b"</span></span></span>' +
    '</span>';

  it('never rewrites text inside a KaTeX subtree', () => {
    const html = `<p>Before ${katexInline} after.</p>`;
    expect(applyRoleplayPatterns(html, compiledRules)).toBe(html);
  });

  it('still applies patterns outside the KaTeX subtree', () => {
    const html = `<p>*waves* ${katexInline} {thinks}</p>`;
    const out = applyRoleplayPatterns(html, compiledRules);
    expect(out).toContain('<span class="qt-chat-narration">*waves*</span>');
    expect(out).toContain('<span class="qt-chat-inner-monologue">{thinks}</span>');
    // The math markup itself is byte-identical.
    expect(out).toContain(katexInline);
  });

  it('skips katex-display and katex-error subtrees too', () => {
    const display =
      '<span class="katex-display"><span class="katex">' +
      '<span class="mord">{x}</span></span></span>';
    const error =
      '<span class="katex-error" title="ParseError" style="color:#cc0000">\\frac{</span>';
    const html = `<p>${display}</p><p>${error}</p>`;
    expect(applyRoleplayPatterns(html, compiledRules)).toBe(html);
  });

  it('still processes code-block skipping independently of math', () => {
    const html = `<pre><code>*not narration*</code></pre><p>*is narration*</p>`;
    const out = applyRoleplayPatterns(html, compiledRules);
    expect(out).toContain('<code>*not narration*</code>');
    expect(out).toContain('<span class="qt-chat-narration">*is narration*</span>');
  });
});
