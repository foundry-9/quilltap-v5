/**
 * Shared roleplay-rendering core — a faithful TS port of v4
 * `lib/chat/roleplay-rendering.ts` (HEAD a7b1398d). Framework-agnostic pure
 * string work; the SPA markdown renderer composes it exactly as v4's server
 * renderer does, so the two stay byte-for-byte aligned (proven by the captured
 * fixtures in `markdown-renderer.spec.ts`).
 *
 * NOTE: the dialogue pattern / detection chars below are copied byte-for-byte
 * from v4 (straight quotes only, despite the "straight and curly" comment) — do
 * not "fix" them; the captured fixtures enforce parity.
 */

/** A stored rendering pattern (v4 `RenderingPattern`). */
export interface RenderingPattern {
  pattern: string;
  className: string;
  flags?: string;
  scope?: 'inline' | 'line';
  hideDelimiters?: boolean;
}

/** Paragraph-level dialogue detection config (v4 `DialogueDetection`). */
export interface DialogueDetection {
  openingChars: string[];
  closingChars: string[];
  className: string;
}

/** Default rendering patterns used when a template doesn't specify any. */
export const DEFAULT_RENDERING_PATTERNS: RenderingPattern[] = [
  // OOC: ((comments)) - double parentheses
  { pattern: '\\(\\([^)]+\\)\\)', className: 'qt-chat-ooc' },
  // OOC: // comment - line prefix style
  { pattern: '^// .+$', className: 'qt-chat-ooc', flags: 'm' },
  // Dialogue: "speech" - straight and curly quotes
  // (v4's stored pattern is straight-quote-only — copied byte-for-byte.)
  { pattern: '[""][^""]+[""]', className: 'qt-chat-dialogue' },
  // Narration: *actions* - single asterisks (not bold **)
  { pattern: '(?<!\\*)\\*[^*]+\\*(?!\\*)', className: 'qt-chat-narration' },
  // Narration: [actions] - square brackets (not links)
  { pattern: '\\[[^\\]]+\\](?!\\()', className: 'qt-chat-narration' },
  // Internal monologue: {thoughts} - excludes {{template}} variables
  { pattern: '(?<!\\{)\\{[^{}]+\\}(?!\\})', className: 'qt-chat-inner-monologue' },
];

/** Default dialogue detection for paragraph-level styling. */
export const DEFAULT_DIALOGUE_DETECTION: DialogueDetection = {
  openingChars: ['"', '"'],
  closingChars: ['"', '"'],
  className: 'qt-chat-dialogue',
};

/** A neutral run of text, optionally tagged with a CSS class. */
export interface Segment {
  text: string;
  className?: string;
}

/** A compiled rendering rule. */
export interface CompiledRule {
  regex: RegExp;
  className: string;
  scope: 'inline' | 'line';
  hideDelimiters: boolean;
}

/**
 * Compile stored patterns into rules. Compiled WITHOUT the global flag (the
 * tokenizer walks manually), matching v4 exactly.
 */
export function compileRenderingPatterns(patterns: RenderingPattern[]): CompiledRule[] {
  return patterns.map((p) => ({
    regex: new RegExp(p.pattern, (p.flags || '').replace('g', '')),
    className: p.className,
    scope: p.scope === 'line' ? 'line' : 'inline',
    hideDelimiters: p.hideDelimiters ?? false,
  }));
}

/**
 * Walk `text` once, finding the earliest match among the INLINE rules, and emit
 * neutral segments. Line-scoped rules are ignored here.
 */
export function tokenizeInline(text: string, rules: CompiledRule[]): Segment[] {
  const inlineRules = rules.filter((r) => r.scope === 'inline');
  const segments: Segment[] = [];
  let remaining = text;

  while (remaining.length > 0) {
    let earliest: { index: number; advance: number; className: string; text: string } | null = null;

    for (const rule of inlineRules) {
      const match = remaining.match(rule.regex);
      if (match && match.index !== undefined) {
        if (!earliest || match.index < earliest.index) {
          const display =
            rule.hideDelimiters && match.groups?.['rpBody'] !== undefined
              ? match.groups['rpBody']
              : match[0];
          earliest = {
            index: match.index,
            advance: match[0].length,
            className: rule.className,
            text: display,
          };
        }
      }
    }

    if (earliest) {
      if (earliest.index > 0) {
        segments.push({ text: remaining.substring(0, earliest.index) });
      }
      segments.push({ text: earliest.text, className: earliest.className });
      remaining = remaining.substring(earliest.index + earliest.advance);
    } else {
      segments.push({ text: remaining });
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

/** Serialize segments to an HTML string; matched runs get `<span class="…">`. */
export function segmentsToHtml(segments: Segment[]): string {
  let out = '';
  for (const seg of segments) {
    out += seg.className ? `<span class="${seg.className}">${seg.text}</span>` : seg.text;
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
 */
export function escapeMarkdownInBrackets(content: string, patterns: RenderingPattern[]): string {
  const inlinePatterns = patterns.filter((p) => p.scope !== 'line');

  const escapableRpBody = inlinePatterns.filter((p) => hasRpBodyGroup(p) && !p.hideDelimiters);

  const legacy = inlinePatterns.filter((p) => !hasRpBodyGroup(p));
  const hasBracketNarration = legacy.some((p) => p.pattern.includes('\\['));
  const hasBraceMonologue = legacy.some((p) => p.pattern.includes('\\{'));
  const hasAsteriskNarration = legacy.some(
    (p) => p.pattern.includes('\\*') && p.className.split(/\s+/).includes('qt-chat-narration'),
  );

  if (
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

    let result = part;

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

    return result;
  });

  return processedParts.join('');
}
