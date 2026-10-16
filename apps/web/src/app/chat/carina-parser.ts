/**
 * Carina Parser — inline LLM query detection (the CLIENT twin).
 *
 * Carina (the "reference desk") lets users and LLM characters pose a quick
 * question to a designated answerer character with a compact `@Name:` markup.
 * This module extracts that markup from raw message text.
 *
 * Forms (spec: `docs/v4/developer/features/carina.md`):
 *   `@CharName: question`          → public answer
 *   `@CharName? question`          → whispered answer (asker only)
 *   `@CharName: "quoted question"` → quoted (consumes up to the matching close)
 *   `@CharName? 'quoted question'` → straight or smart quotes both work
 *
 * Rules:
 *   - Detection is per-line; the `@` must begin the line.
 *   - Only the FIRST line that yields a real (non-empty) question fires; any
 *     later `@Name` lines are ignored (one query per message).
 *   - Quoted questions never span multiple lines (we operate per-line).
 *
 * **Why a client twin at all.** The server owns the authoritative parser
 * (`quilltap-core::carina_parser`), and nothing in the SPA needed to know what a
 * Carina address looked like until In Their Own Words: its gate must NOT rehearse
 * a `@Name:` line, because the address is machinery that has to survive verbatim
 * (v4 `useImpersonationVoice.ts:66`). v4's client imports `lib/chat/carina-parser`
 * directly — one module serving both sides — which v5 cannot do across the
 * Rust/TypeScript boundary, so this is a character-for-character transcription of
 * v4's `lib/chat/carina-parser.ts` at `f4ad2c8d1`, pinned row-for-row by
 * `carina-parser.oracle.spec.ts` against a corpus recorded from v4's REAL module
 * (re-recorded at `b0b6656b5` with the `isCarinaInvocableName` rows, P4.D224,
 * and at `25b8fab6a` with the `stripCarinaQueryLines` rows, P4.D307 — the strip
 * Pascal's trigger-phrase scan reads, `pascal/trigger-phrases.ts`).
 *
 * Pure and import-free, exactly as v4's is.
 *
 * @module chat/carina-parser
 */

export interface CarinaQuery {
  /** Answerer character name (word chars + interior spaces; trimmed). */
  characterName: string;
  /** `?` separator → whisper to the asker only; `:` separator → public. */
  whisper: boolean;
  /** The question text (quotes stripped when a matching pair was present). */
  question: string;
}

/**
 * Opening-quote → closing-quote map. Straight quotes pair with themselves;
 * smart quotes pair with their counterparts. The spec's single-regex form uses a
 * `\3` backref, which only works for straight quotes (open === close); v4 pairs
 * quotes explicitly so smart-quote spans close correctly.
 */
const QUOTE_PAIRS: Readonly<Record<string, string>> = {
  '"': '"',
  "'": "'",
  '“': '”', // “ … ”
  '‘': '’', // ‘ … ’
};

/**
 * Matches a single line: `@` + name (starts and ends with a word char, may
 * contain interior spaces) + separator (`:` or `?`) + optional whitespace + the
 * remainder of the line. Quote handling for the remainder happens in
 * `extractQuestion` so smart quotes pair correctly.
 *
 * `\w` is ASCII-only in JavaScript, so an accented or non-Latin name does NOT
 * match — a real v4 property, pinned by the corpus rather than "fixed".
 */
const NAME_SOURCE = '[\\w][\\w ]*\\w';
export const CARINA_LINE_RE = new RegExp(`^@(${NAME_SOURCE})([?:])\\s*(.*)$`);
const NAME_RE = new RegExp(`^${NAME_SOURCE}$`);

/**
 * Whether `name` can be addressed as `@name:` / `@name?` — the same name
 * grammar `parseCarinaQuery` applies, shared so the composer's `@` typeahead
 * never keeps an `@` in front of a name no query could ever fire for.
 *
 * v4 `3376b3dfa` rebuilt `LINE_RE` from `NAME_SOURCE` to export this; the
 * pattern is byte-identical to the literal it replaced (`.source` and `.flags`
 * pinned in `carina-parser.oracle.spec.ts`). The Rust parser needs no change.
 */
export function isCarinaInvocableName(name: string): boolean {
  return NAME_RE.test(name);
}

/**
 * Extract the question from the post-separator remainder. When the first
 * character is an opening quote and a matching close quote exists later on the
 * line, return the text between them; otherwise return the whole remainder (the
 * unquoted form, which mirrors the spec regex falling through to `(.*)`).
 */
function extractQuestion(rest: string): string {
  if (rest.length === 0) {
    return '';
  }
  const open = rest[0];
  const close = QUOTE_PAIRS[open];
  if (close) {
    const closeIdx = rest.indexOf(close, 1);
    if (closeIdx > 0) {
      return rest.slice(1, closeIdx).trim();
    }
    // No matching close quote — fall through to the unquoted form (keeps the
    // leading quote, exactly as the spec's `(.*)` alternative would).
  }
  return rest.trim();
}

/** Parse one line as a Carina query, or null when it is not one. */
function parseQueryLine(line: string): CarinaQuery | null {
  const m = CARINA_LINE_RE.exec(line);
  if (!m) {
    return null;
  }

  const characterName = m[1].trim();
  if (!characterName) {
    return null;
  }

  const question = extractQuestion(m[3]);
  if (!question) {
    // An `@Name:` with no question text isn't a usable query.
    return null;
  }

  return {
    characterName,
    whisper: m[2] === '?',
    question,
  };
}

/**
 * Parse the first Carina query from a message's raw content.
 * Returns `null` when no line yields a valid (non-empty) query — an empty
 * `@Name:` line is skipped in case a later line carries a real one.
 */
export function parseCarinaQuery(content: string): CarinaQuery | null {
  if (!content) {
    return null;
  }

  for (const line of content.split(/\r?\n/)) {
    const query = parseQueryLine(line);
    if (query) {
      return query;
    }
  }

  return null;
}

/**
 * Remove EVERY line that parses as a Carina query — not just the first, which
 * is all `parseCarinaQuery` fires. A query line is addressed to the reference
 * desk, not spoken in the scene, so a reader of "what was said" (Pascal's
 * trigger-phrase scan) subtracts them all. Built on the same line grammar, so
 * the two never disagree about what a query line is.
 *
 * v4 `362426c8f` (pinned at `25b8fab6a`) extracted `parseQueryLine` from
 * `parseCarinaQuery`'s loop to share it; the parse itself is unchanged (the 83
 * older corpus rows re-record byte-identical), and the strip is pinned by its
 * own `kind: 'strip'` rows in `carina-parser.oracle.spec.ts`.
 */
export function stripCarinaQueryLines(text: string): string {
  if (!text) {
    return text;
  }
  return text
    .split(/\r?\n/)
    .filter((line) => parseQueryLine(line) === null)
    .join('\n');
}
