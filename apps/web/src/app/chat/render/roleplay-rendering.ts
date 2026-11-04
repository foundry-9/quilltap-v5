/**
 * Shared roleplay-rendering core — a faithful TS port of v4
 * `lib/chat/roleplay-rendering.ts` (HEAD a7b1398d; the Novel template's
 * nesting, unmarked scope and `strip` / `literal` pre-parse from `778b97b2b`
 * at 236c04e35, P4.D352). Framework-agnostic pure
 * string work; the SPA markdown renderer composes it exactly as v4's server
 * renderer does, so the two stay byte-for-byte aligned (proven by the captured
 * fixtures in `markdown-renderer.spec.ts`).
 *
 * NOTE: the dialogue pattern / detection chars below are copied byte-for-byte
 * from v4 — including the `\u` escape form of the curly pair, which is the
 * whole lesson of v4's bug 62 (`c7892132`): the entry claimed curly support for
 * its whole life while holding four copies of ASCII 0x22, because a literal
 * U+201C in source is indistinguishable from a straight quote at a glance. Do
 * not "tidy" them in either direction; the captured fixtures enforce parity.
 */

/** A stored rendering pattern (v4 `RenderingPattern`). */
export interface RenderingPattern {
  pattern: string;
  className: string;
  flags?: string;
  /**
   * `inline` / `line` style matches; `unmarked` styles the text no inline rule
   * claims; `literal` / `strip` act before parsing, in
   * {@link escapeMarkdownInBrackets} (v4 `RenderingPatternSchema`, `778b97b2b`).
   */
  scope?: 'inline' | 'line' | 'unmarked' | 'literal' | 'strip';
  hideDelimiters?: boolean;
}

/** The five scopes v4's `RenderingPatternSchema` knows. */
const KNOWN_SCOPES: ReadonlySet<string> = new Set(['inline', 'line', 'unmarked', 'literal', 'strip']);

/** Paragraph-level dialogue detection config (v4 `DialogueDetection`). */
export interface DialogueDetection {
  openingChars: string[];
  closingChars: string[];
  className: string;
}

/**
 * A rendering pattern as it arrives on the wire. `core-contract.ts` types
 * `scope` as a bare `string` where v4's `RenderingPatternSchema` constrains it
 * to five values; the contract file is frozen, so the narrowing happens here,
 * at the one boundary where a fetched template enters the renderer.
 */
export interface WireRenderingPattern {
  pattern: string;
  className: string;
  flags?: string;
  scope?: string;
  hideDelimiters?: boolean;
}

/**
 * Narrow a fetched template's patterns to the renderer's type. The five scopes
 * v4 knows pass through; an absent or unknown one becomes `'inline'` — which is
 * what {@link compileRenderingPatterns} (and v4's, verbatim) does with anything
 * else.
 *
 * (Until P4.D352 this coerced every non-`'line'` scope to `'inline'`. That was
 * v4's own compile at `322a5488e` and stopped being so at `778b97b2b`: a Novel
 * template's `[\s\S]` unmarked pattern became an inline rule claiming every
 * character, and its `literal` / `strip` patterns became inline rules too.)
 */
export function narrowRenderingPatterns(
  patterns: readonly WireRenderingPattern[],
): RenderingPattern[] {
  return patterns.map((p) => ({
    pattern: p.pattern,
    className: p.className,
    ...(p.flags === undefined ? {} : { flags: p.flags }),
    scope:
      p.scope !== undefined && KNOWN_SCOPES.has(p.scope)
        ? (p.scope as NonNullable<RenderingPattern['scope']>)
        : ('inline' as const),
    ...(p.hideDelimiters === undefined ? {} : { hideDelimiters: p.hideDelimiters }),
  }));
}

/** Default rendering patterns used when a template doesn't specify any. */
export const DEFAULT_RENDERING_PATTERNS: RenderingPattern[] = [
  // OOC: ((comments)) - double parentheses
  { pattern: '\\(\\([^)]+\\)\\)', className: 'qt-chat-ooc' },
  // OOC: // comment - line prefix style
  { pattern: '^// .+$', className: 'qt-chat-ooc', flags: 'm' },
  // Dialogue: "speech" - straight AND curly double quotes. The curly ones are
  // written as \u escapes on purpose (as the seeded Standard Roleplay template
  // does): this entry spent its whole life claiming curly support while holding
  // four copies of ASCII 0x22, because a literal U+201C in source is
  // indistinguishable from a straight quote at a glance (bug 62). Single quotes
  // are deliberately excluded - an apostrophe is a single quote far more often
  // than a quotation mark is (don't, writers', '80s).
  { pattern: '["\u201c][^"\u201d]+["\u201d]', className: 'qt-chat-dialogue' },
  // Narration: *actions* - single asterisks (not bold **)
  { pattern: '(?<!\\*)\\*[^*]+\\*(?!\\*)', className: 'qt-chat-narration' },
  // Narration: [actions] - square brackets (not links)
  { pattern: '\\[[^\\]]+\\](?!\\()', className: 'qt-chat-narration' },
  // Internal monologue: {thoughts} - excludes {{template}} variables
  { pattern: '(?<!\\{)\\{[^{}]+\\}(?!\\})', className: 'qt-chat-inner-monologue' },
];

/**
 * Default dialogue detection for paragraph-level styling.
 *
 * Straight and curly DOUBLE quotes only, the curly pair written as \u escapes
 * for the reason given on the dialogue pattern above (bug 62). Keep these in
 * step with that pattern — a paragraph the inline rule styles but this one
 * doesn't (or vice versa) is a visible split.
 */
export const DEFAULT_DIALOGUE_DETECTION: DialogueDetection = {
  openingChars: ['"', '\u201c'],
  closingChars: ['"', '\u201d'],
  className: 'qt-chat-dialogue',
};

/**
 * A neutral run of text, optionally tagged with a CSS class. A styled segment
 * may carry `children` — the other inline rules applied inside it (emphasis
 * inside dialogue) — in which case adapters render those instead of `text`.
 */
export interface Segment {
  text: string;
  className?: string;
  children?: Segment[];
}

/** A compiled rendering rule. */
export interface CompiledRule {
  regex: RegExp;
  className: string;
  /**
   * `unmarked` rules match nothing: their class styles the text no inline rule
   * claims (see {@link unmarkedClassFor}). The pre-parse `literal`/`strip`
   * scopes never become rules — they act in {@link escapeMarkdownInBrackets}.
   */
  scope: 'inline' | 'line' | 'unmarked';
  hideDelimiters: boolean;
}

/**
 * Compile stored patterns into rules. Compiled WITHOUT the global flag (the
 * tokenizer walks manually), matching v4 exactly. `scope` falls back to
 * `inline` for legacy patterns that predate the field.
 */
export function compileRenderingPatterns(patterns: RenderingPattern[]): CompiledRule[] {
  return patterns
    .filter((p) => p.scope !== 'literal' && p.scope !== 'strip')
    .map((p) => ({
      regex: new RegExp(p.pattern, (p.flags || '').replace('g', '')),
      className: p.className,
      scope: p.scope === 'line' ? 'line' : p.scope === 'unmarked' ? 'unmarked' : 'inline',
      hideDelimiters: p.hideDelimiters ?? false,
    }));
}

/**
 * The class a template gives to text no inline rule claims, or `undefined`.
 * Adapters pass it to {@link tokenizeInline} only for text that sits directly in
 * an unclaimed block — never inside a span or a block a line/wrap rule owns.
 */
export function unmarkedClassFor(rules: CompiledRule[]): string | undefined {
  return rules.find((r) => r.scope === 'unmarked')?.className || undefined;
}

/** How deep inline rules nest inside one another (dialogue › emphasis › …). */
const MAX_INLINE_NESTING = 3;

export interface TokenizeOptions {
  /**
   * Class for the runs no rule claims (see {@link unmarkedClassFor}). Omitted
   * ⇒ those runs stay plain. Never applied inside a matched span.
   */
  unmarkedClassName?: string;
}

/**
 * Walk `text` once, finding the earliest match among the INLINE rules, and emit
 * neutral segments. Unmatched runs become plain `{ text }` segments (or carry
 * the unmarked class); matched runs become `{ text, className }`. Line-scoped
 * rules are ignored here.
 *
 * Matching the earliest span and advancing past it (rather than running each
 * pattern's `replace` independently) is what prevents a later pattern from
 * matching text already claimed by an earlier one — e.g. dialogue `"…"` matching
 * the quoted `class="…"` attribute of a span an earlier pattern inserted.
 */
export function tokenizeInline(
  text: string,
  rules: CompiledRule[],
  options: TokenizeOptions = {},
): Segment[] {
  return tokenizeWith(
    text,
    rules.filter((r) => r.scope === 'inline'),
    options.unmarkedClassName,
    0,
  );
}

/**
 * The walk itself. After a rule claims a span, the OTHER inline rules are run
 * over the span's displayed text, so `"I said *stop*"` styles the emphasis
 * inside the dialogue; the claiming rule is left out so it can't re-match its
 * own text. Nested results ride on `children` only when something inside was
 * actually styled.
 */
function tokenizeWith(
  text: string,
  inlineRules: CompiledRule[],
  unmarkedClassName: string | undefined,
  depth: number,
): Segment[] {
  const segments: Segment[] = [];
  let remaining = text;
  const plain = (t: string): Segment =>
    unmarkedClassName ? { text: t, className: unmarkedClassName } : { text: t };

  while (remaining.length > 0) {
    // `advance` is always the FULL match length (delimiters consumed); `text` is
    // what we display — the inner `rpBody` group when the rule hides delimiters,
    // else the full match.
    let earliest: { index: number; advance: number; rule: CompiledRule; text: string } | null =
      null;

    for (const rule of inlineRules) {
      const match = remaining.match(rule.regex);
      if (match && match.index !== undefined) {
        // A zero-length match (an open-ended pattern at the very end) claims
        // nothing and would loop forever; ignore it.
        if (match[0].length === 0) continue;
        if (!earliest || match.index < earliest.index) {
          const display =
            rule.hideDelimiters && match.groups?.['rpBody'] !== undefined
              ? match.groups['rpBody']
              : match[0];
          earliest = {
            index: match.index,
            advance: match[0].length,
            rule,
            text: display,
          };
        }
      }
    }

    if (earliest) {
      if (earliest.index > 0) {
        segments.push(plain(remaining.substring(0, earliest.index)));
      }
      const claimed: Segment = { text: earliest.text, className: earliest.rule.className };
      if (depth < MAX_INLINE_NESTING) {
        const claimant = earliest.rule;
        const others = inlineRules.filter((r) => r !== claimant);
        const inner =
          others.length > 0 ? tokenizeWith(earliest.text, others, undefined, depth + 1) : [];
        if (inner.some((seg) => seg.className)) claimed.children = inner;
      }
      segments.push(claimed);
      remaining = remaining.substring(earliest.index + earliest.advance);
    } else {
      segments.push(plain(remaining));
      break;
    }
  }

  return segments;
}

/** A whole-block match against a LINE-scoped rule. */
export interface LineMatch {
  className: string;
  hideDelimiters: boolean;
  prefix: string;
  body: string;
}

/** A whole-block match against an inline WRAP rule that hides its delimiters. */
export interface WrapBlockMatch {
  className: string;
  prefix: string;
  suffix: string;
}

/**
 * If the entire block of `text` is a single inline WRAP span whose rule hides its
 * delimiters, return the span class plus the opening/closing delimiters to strip.
 */
export function wrapBlockMatchFor(text: string, rules: CompiledRule[]): WrapBlockMatch | undefined {
  const trimmed = text.trim();
  if (!trimmed) return undefined;

  for (const rule of rules) {
    if (rule.scope !== 'inline' || !rule.hideDelimiters) continue;
    const flags = rule.regex.flags.replace('g', '').replace('m', '');
    const anchored = new RegExp(rule.regex.source, flags);
    const m = trimmed.match(anchored);
    if (!m || m.index !== 0 || m[0].length !== trimmed.length) continue;

    const body = m.groups?.['rpBody'];
    if (typeof body !== 'string' || body.length === 0) continue;

    const bodyStart = m[0].indexOf(body);
    if (bodyStart <= 0) continue;
    const prefix = m[0].slice(0, bodyStart);
    const suffix = m[0].slice(bodyStart + body.length);
    if (!prefix || !suffix) continue;

    return { className: rule.className, prefix, suffix };
  }
  return undefined;
}

/**
 * If the whole block of `text` is a single line matching a LINE-scoped rule,
 * return the match details.
 */
export function lineMatchFor(text: string, rules: CompiledRule[]): LineMatch | undefined {
  const trimmed = text.trim();
  if (!trimmed) return undefined;

  for (const rule of rules) {
    if (rule.scope !== 'line') continue;
    const flags = rule.regex.flags.replace('m', '').replace('g', '');
    const anchored = new RegExp(rule.regex.source, flags);
    const m = trimmed.match(anchored);
    if (m && m.index === 0 && m[0].length === trimmed.length) {
      const body = m.groups?.['rpBody'] ?? trimmed;
      const prefix = m[0].length >= body.length ? m[0].slice(0, m[0].length - body.length) : '';
      return { className: rule.className, hideDelimiters: rule.hideDelimiters, prefix, body };
    }
  }
  return undefined;
}

/**
 * Check whether text content represents dialogue: it starts with an opening
 * quote char and ends with a closing quote char.
 */
export function isDialogueParagraph(text: string, detection: DialogueDetection): boolean {
  const trimmed = text.trim();
  if (trimmed.length < 2) return false;

  const firstChar = trimmed[0];
  const lastChar = trimmed[trimmed.length - 1];

  return detection.openingChars.includes(firstChar) && detection.closingChars.includes(lastChar);
}

/**
 * Serialize segments to an HTML string; matched runs get `<span class="…">`,
 * and a segment with `children` renders those instead of its `text`.
 */
export function segmentsToHtml(segments: Segment[]): string {
  let out = '';
  for (const seg of segments) {
    const body = seg.children ? segmentsToHtml(seg.children) : seg.text;
    out += seg.className ? `<span class="${seg.className}">${body}</span>` : body;
  }
  return out;
}

/** Characters that trigger markdown parsing and must be escaped inside spans. */
const MARKDOWN_CHARS = /([*_~`])/g;

function hasRpBodyGroup(pattern: RenderingPattern): boolean {
  return pattern.pattern.includes('(?<rpBody>');
}

function escapeRpBodyInterior(part: string, pattern: RenderingPattern): string {
  const baseFlags = (pattern.flags || '').replace('g', '');
  let re: RegExp;
  try {
    re = new RegExp(pattern.pattern, `${baseFlags}g`);
  } catch {
    return part;
  }
  return part.replace(re, (match: string, ...rest: unknown[]) => {
    const groups = rest[rest.length - 1] as Record<string, string> | undefined;
    const body = groups?.['rpBody'];
    if (typeof body !== 'string' || body.length === 0) return match;
    const bodyStart = match.indexOf(body);
    if (bodyStart < 0) return match;
    const open = match.slice(0, bodyStart);
    const close = match.slice(bodyStart + body.length);
    return `${open}${body.replace(MARKDOWN_CHARS, '\\$1')}${close}`;
  });
}

/**
 * Escape markdown syntax characters inside roleplay delimiters so the markdown
 * parser doesn't break a span before it can be styled. Verbatim port of v4.
 *
 * Since `778b97b2b` it also runs a template's pre-parse patterns: `strip`
 * matches are removed FIRST and `literal` matches are backslash-escaped LAST
 * (see the two calls below).
 */
export function escapeMarkdownInBrackets(content: string, patterns: RenderingPattern[]): string {
  const stripPatterns = patterns.filter((p) => p.scope === 'strip');
  const literalPatterns = patterns.filter((p) => p.scope === 'literal');
  const inlinePatterns = patterns.filter((p) => !p.scope || p.scope === 'inline');

  const escapableRpBody = inlinePatterns.filter((p) => hasRpBodyGroup(p) && !p.hideDelimiters);

  const legacy = inlinePatterns.filter((p) => !hasRpBodyGroup(p));
  const hasBracketNarration = legacy.some((p) => p.pattern.includes('\\['));
  const hasBraceMonologue = legacy.some((p) => p.pattern.includes('\\{'));
  const hasAsteriskNarration = legacy.some(
    (p) => p.pattern.includes('\\*') && p.className.split(/\s+/).includes('qt-chat-narration'),
  );

  if (
    stripPatterns.length === 0 &&
    literalPatterns.length === 0 &&
    escapableRpBody.length === 0 &&
    !hasBracketNarration &&
    !hasBraceMonologue &&
    !hasAsteriskNarration
  ) {
    return content;
  }

  const codeBlockRegex = /(```[\s\S]*?```)/g;
  const parts = content.split(codeBlockRegex);

  const processedParts = parts.map((part, index) => {
    if (index % 2 === 1) {
      return part;
    }

    // Strip first: a doubled `**` must vanish before the wrap-interior escape
    // below can turn it into `\\*\\*`, which nothing would then strip.
    let result = applyPreParse(part, stripPatterns, () => '');

    for (const pattern of escapableRpBody) {
      result = escapeRpBodyInterior(result, pattern);
    }

    if (hasBracketNarration) {
      result = result.replace(/\[([^\]]+)\](?!\()/g, (_match, inner) => {
        const escaped = inner.replace(MARKDOWN_CHARS, '\\$1');
        return `[${escaped}]`;
      });
    }

    if (hasBraceMonologue) {
      result = result.replace(/(?<!\{)\{([^{}]+)\}(?!\})/g, (_match, inner) => {
        const escaped = inner.replace(MARKDOWN_CHARS, '\\$1');
        return `{${escaped}}`;
      });
    }

    if (hasAsteriskNarration) {
      result = result.replace(/(?<!\*)\*([^*]+)\*(?!\*)/g, (_match, inner) => {
        const escaped = inner.replace(/([_~`])/g, '\\$1');
        return `*${escaped}*`;
      });
    }

    // Literal last: whatever the steps above left unescaped is made plain text
    // for the template's own delimiters to claim.
    result = applyPreParse(result, literalPatterns, (m) => m.replace(MARKDOWN_PUNCT, '\\$1'));

    return result;
  });

  return processedParts.join('');
}

/** Every ASCII punctuation character Markdown lets a backslash escape. */
const MARKDOWN_PUNCT = /([!-/:-@[-`{-~])/g;

/** A Markdown thematic break (`***`, `* * *`, `---`): a scene break, left alone. */
const THEMATIC_BREAK = /^ {0,3}([*_-])(?:[ \t]*\1){2,}[ \t]*$/;

/** A list bullet at line start (`* item`), whose marker is Markdown's own. */
const LEADING_BULLET = /^([ \t]*[*+-][ \t]+)/;

/** Inline code and `$$…$$` math, whose contents must never be rewritten. */
const PROTECTED_INLINE = /(`[^`\n]*`|\$\$[\s\S]*?\$\$)/g;

/**
 * Apply the pre-parse `strip` / `literal` patterns to a fenced-code-free part:
 * each match is replaced by `replace(match)`, except inside inline code or
 * math, on a thematic-break line, in a leading list bullet, or where the match
 * is already backslash-escaped.
 */
function applyPreParse(
  part: string,
  patterns: RenderingPattern[],
  replace: (match: string) => string,
): string {
  if (patterns.length === 0) return part;
  const regexes: RegExp[] = [];
  for (const p of patterns) {
    try {
      regexes.push(new RegExp(`(?<!\\\\)(?:${p.pattern})`, `${(p.flags || '').replace('g', '')}g`));
    } catch {
      // A malformed stored pattern must never break rendering.
    }
  }
  if (regexes.length === 0) return part;

  return part
    .split(PROTECTED_INLINE)
    .map((chunk, i) => {
      if (i % 2 === 1) return chunk; // inline code / math
      return chunk
        .split('\n')
        .map((line) => {
          if (THEMATIC_BREAK.test(line)) return line;
          const bullet = line.match(LEADING_BULLET)?.[1] ?? '';
          let body = line.slice(bullet.length);
          for (const re of regexes) body = body.replace(re, replace);
          return bullet + body;
        })
        .join('\n');
    })
    .join('');
}
