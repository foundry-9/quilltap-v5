/**
 * Document-content helpers: markdown detection, YAML-frontmatter extraction for
 * the "Document Info" table, and the word-count for the status bar. Ported from
 * v4 `DocumentPane.tsx`.
 *
 * DIVERGENCE (recorded in the status log): v4 tries a real `YAML.parse` of the
 * frontmatter block first and only falls back to a loose line parser when that
 * throws. The SPA has no `yaml` dependency (new deps this round are limited to
 * the spike-gated Lexical packages), so we use v4's LOOSE line parser directly.
 * The table is display-only, and flat `key: value` (plus inline `[a, b]` arrays,
 * handled by {@link toChipValues}) — the shape real documents carry — renders
 * identically; nested/multiline YAML is the only case that would differ.
 *
 * @module documents/frontmatter
 */

const EM_DASH = '—';

/**
 * True for files we edit as Markdown. Everything else (JSON, YAML, plain text,
 * source code) renders in a plain textarea so its bytes are never round-tripped
 * through a Markdown serializer (v4 `isMarkdownFile`).
 */
export function isMarkdownFile(filePath: string): boolean {
  const dot = filePath.lastIndexOf('.');
  if (dot < 0) return false;
  const ext = filePath.slice(dot).toLowerCase();
  return ext === '.md' || ext === '.markdown';
}

/** Word count for the status bar (v4 `countWords`): non-empty whitespace runs. */
export function countWords(text: string): number {
  const trimmed = text.trim();
  if (!trimmed) return 0;
  return trimmed.split(/\s+/).length;
}

export interface ExtractedFrontmatter {
  rawBlock: string | null;
  body: string;
  data: Record<string, unknown>;
}

/** v4 `parseLooseFrontmatterData`: one `key: value` per line, `#` comments skipped. */
function parseLooseFrontmatterData(yamlContent: string): Record<string, unknown> {
  const data: Record<string, unknown> = {};
  for (const line of yamlContent.split('\n')) {
    const trimmed = line.trim();
    if (!trimmed || trimmed.startsWith('#')) continue;
    const colonIndex = trimmed.indexOf(':');
    if (colonIndex === -1) continue;

    const key = trimmed.slice(0, colonIndex).trim();
    const value = trimmed.slice(colonIndex + 1).trim();
    if (!key) continue;
    data[key] = value || EM_DASH;
  }
  return data;
}

/**
 * Split leading `---\n…\n---` YAML frontmatter from the body (v4
 * `extractFrontmatter`, loose-parser branch). No frontmatter → the whole string
 * is the body and `data` is empty.
 */
export function extractFrontmatter(content: string): ExtractedFrontmatter {
  const match = content.match(/^---\n([\s\S]*?)\n---(?:\n|$)/);
  if (!match) {
    return { rawBlock: null, body: content, data: {} };
  }

  const rawBlock = match[0];
  const yamlContent = match[1];
  const body = content.slice(rawBlock.length);

  return { rawBlock, body, data: parseLooseFrontmatterData(yamlContent) };
}

/** v4 `frontmatterValueToText`: primitives verbatim, objects as JSON, null → em dash. */
export function frontmatterValueToText(value: unknown): string {
  if (value === null || value === undefined) return EM_DASH;
  if (typeof value === 'string') return value;
  if (typeof value === 'number' || typeof value === 'boolean') return String(value);
  try {
    return JSON.stringify(value);
  } catch {
    return String(value);
  }
}

/**
 * v4 `toChipValues`: an array, or an inline `[a, b, c]` string, becomes a list
 * of tag chips; anything else returns null (rendered as plain text).
 */
export function toChipValues(value: unknown): string[] | null {
  if (Array.isArray(value)) {
    return value.map((item) => frontmatterValueToText(item).trim()).filter(Boolean);
  }

  if (typeof value !== 'string') return null;
  const trimmed = value.trim();
  if (!trimmed.startsWith('[') || !trimmed.endsWith(']')) return null;

  const inlineItems = trimmed
    .slice(1, -1)
    .split(',')
    .map((item) => item.trim().replace(/^['"]|['"]$/g, ''))
    .filter(Boolean);

  return inlineItems.length > 0 ? inlineItems : null;
}

/** One rendered frontmatter row (key + either a chip list or a text value). */
export interface FrontmatterRow {
  key: string;
  chips: string[] | null;
  text: string;
}

/** Project the frontmatter data into the rows the table renders. */
export function frontmatterRows(data: Record<string, unknown>): FrontmatterRow[] {
  return Object.entries(data).map(([key, value]) => {
    const chips = toChipValues(value);
    return { key, chips, text: frontmatterValueToText(value) };
  });
}
