/**
 * A pure TS port of v4's `components/characters/TemplateHighlighter.tsx` +
 * `components/characters/apply-character-field-updates.ts` — the counting /
 * transform engine behind the Details tab's `{{char}}`/`{{user}}` template
 * replace-and-reverse buttons, plus the render-time highlighter used by the
 * Details and System Prompts tabs.
 *
 * `collectTemplateFields` is the SINGLE source of truth for which character
 * fields participate in templating (the six prose scalars, every scenario +
 * system prompt, and the physical-description prompt/prose fields — NOT
 * `title` or the physical-description `name`). Both the count and the
 * transform walk this list so they can never drift apart (the v4 bug this
 * replaced).
 */

/** A local escape (v4 `lib/utils/regex.ts` `escapeRegex` — a one-line pure
 *  helper duplicated here rather than importing across the chat/ boundary). */
function escapeRegex(input: string): string {
  return input.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');
}

/** The minimal character shape the collector/transform/counter need. */
export interface TemplatableCharacter {
  name: string;
  identity?: string | null;
  manifesto?: string | null;
  description?: string | null;
  personality?: string | null;
  firstMessage?: string | null;
  exampleDialogues?: string | null;
  scenarios?: ReadonlyArray<{ id: string; content: string }> | null;
  systemPrompts?: ReadonlyArray<{ id: string; content: string }> | null;
  physicalDescription?: {
    id?: string;
    name?: string | null;
    fullDescription?: string | null;
    shortPrompt?: string | null;
    mediumPrompt?: string | null;
    longPrompt?: string | null;
    completePrompt?: string | null;
  } | null;
}

type TemplatableScalarField =
  | 'identity'
  | 'manifesto'
  | 'description'
  | 'personality'
  | 'firstMessage'
  | 'exampleDialogues';

type PhysicalSubField =
  | 'fullDescription'
  | 'shortPrompt'
  | 'mediumPrompt'
  | 'longPrompt'
  | 'completePrompt';

/** One templatable text slice on a character, tagged for write-back routing. */
export type TemplateFieldDescriptor =
  | { key: string; kind: 'scalar'; field: TemplatableScalarField; value: string | null | undefined }
  | { key: string; kind: 'scenario'; id: string; value: string }
  | { key: string; kind: 'systemPrompt'; id: string; value: string }
  | { key: string; kind: 'physical'; id: string; sub: PhysicalSubField; value: string | null | undefined };

const SCALAR_FIELDS: TemplatableScalarField[] = [
  'identity',
  'manifesto',
  'description',
  'personality',
  'firstMessage',
  'exampleDialogues',
];

const PHYSICAL_SUBS: PhysicalSubField[] = [
  'fullDescription',
  'shortPrompt',
  'mediumPrompt',
  'longPrompt',
  'completePrompt',
];

export function collectTemplateFields(character: TemplatableCharacter): TemplateFieldDescriptor[] {
  const descriptors: TemplateFieldDescriptor[] = [];

  for (const field of SCALAR_FIELDS) {
    descriptors.push({ key: field, kind: 'scalar', field, value: character[field] });
  }

  for (const scenario of character.scenarios ?? []) {
    descriptors.push({ key: `scenario:${scenario.id}`, kind: 'scenario', id: scenario.id, value: scenario.content });
  }

  for (const prompt of character.systemPrompts ?? []) {
    descriptors.push({
      key: `systemPrompt:${prompt.id}`,
      kind: 'systemPrompt',
      id: prompt.id,
      value: prompt.content,
    });
  }

  const pd = character.physicalDescription;
  if (pd) {
    const pdId = pd.id ?? 'physical';
    for (const sub of PHYSICAL_SUBS) {
      descriptors.push({ key: `physicalDescription:${pdId}:${sub}`, kind: 'physical', id: pdId, sub, value: pd[sub] });
    }
  }

  return descriptors;
}

/** Counts occurrences of the character/user names in a set of named fields. */
export function countTemplateReplacements(
  fields: Record<string, string | null | undefined>,
  characterName: string,
  userCharacterName?: string | null,
): { charCount: number; userCount: number; fieldCounts: Record<string, { char: number; user: number }> } {
  let charCount = 0;
  let userCount = 0;
  const fieldCounts: Record<string, { char: number; user: number }> = {};

  for (const [field, content] of Object.entries(fields)) {
    if (!content) {
      fieldCounts[field] = { char: 0, user: 0 };
      continue;
    }
    let fieldCharCount = 0;
    let fieldUserCount = 0;
    if (characterName && characterName.length > 0) {
      const charRegex = new RegExp(escapeRegex(characterName), 'gi');
      fieldCharCount = content.match(charRegex)?.length ?? 0;
    }
    if (userCharacterName && userCharacterName.length > 0) {
      const userRegex = new RegExp(escapeRegex(userCharacterName), 'gi');
      fieldUserCount = content.match(userRegex)?.length ?? 0;
    }
    fieldCounts[field] = { char: fieldCharCount, user: fieldUserCount };
    charCount += fieldCharCount;
    userCount += fieldUserCount;
  }

  return { charCount, userCount, fieldCounts };
}

/** Counts `{{char}}`/`{{user}}` template literals across a set of strings. */
export function countTemplateLiterals(
  values: Array<string | null | undefined>,
): { charCount: number; userCount: number } {
  let charCount = 0;
  let userCount = 0;
  for (const value of values) {
    if (!value) continue;
    charCount += value.match(/\{\{char\}\}/gi)?.length ?? 0;
    userCount += value.match(/\{\{user\}\}/gi)?.length ?? 0;
  }
  return { charCount, userCount };
}

/** Replaces every occurrence of `name` with `template` (case-insensitive). */
export function replaceWithTemplate(content: string, name: string, template: string): string {
  if (!content || !name) return content;
  const regex = new RegExp(escapeRegex(name), 'gi');
  return content.replace(regex, template);
}

/** Replaces every `{{char}}` (or `{{user}}`) literal with a concrete name. */
export function replaceTemplateWithName(content: string, which: 'char' | 'user', name: string): string {
  if (!content) return content;
  const regex = which === 'char' ? /\{\{char\}\}/gi : /\{\{user\}\}/gi;
  return content.replace(regex, () => name);
}

/**
 * Applies a text transform across every templatable field, routing the
 * results into the two persistence paths the character PUT/prompt endpoints
 * require (v4 `applyTemplateTransform`).
 */
export function applyTemplateTransform(
  character: TemplatableCharacter,
  transform: (text: string) => string,
): { mainUpdates: Record<string, unknown>; changedSystemPrompts: Array<{ id: string; content: string }> } {
  const mainUpdates: Record<string, unknown> = {};
  const changedSystemPrompts: Array<{ id: string; content: string }> = [];
  const scenarioChanges = new Map<string, string>();
  const physicalChanges = new Map<PhysicalSubField, string>();

  for (const descriptor of collectTemplateFields(character)) {
    if (!descriptor.value) continue;
    const next = transform(descriptor.value);
    if (next === descriptor.value) continue;

    switch (descriptor.kind) {
      case 'scalar':
        mainUpdates[descriptor.field] = next;
        break;
      case 'systemPrompt':
        changedSystemPrompts.push({ id: descriptor.id, content: next });
        break;
      case 'scenario':
        scenarioChanges.set(descriptor.id, next);
        break;
      case 'physical':
        physicalChanges.set(descriptor.sub, next);
        break;
    }
  }

  if (scenarioChanges.size > 0 && character.scenarios) {
    mainUpdates['scenarios'] = character.scenarios.map((scenario) =>
      scenarioChanges.has(scenario.id) ? { ...scenario, content: scenarioChanges.get(scenario.id)! } : scenario,
    );
  }

  if (physicalChanges.size > 0 && character.physicalDescription) {
    const pd = character.physicalDescription;
    const nextPhysical: Record<string, unknown> = { ...pd };
    for (const [sub, content] of physicalChanges) {
      nextPhysical[sub] = content;
    }
    nextPhysical['name'] = typeof pd.name === 'string' && pd.name.trim() ? pd.name : 'Appearance';
    mainUpdates['physicalDescription'] = nextPhysical;
  }

  return { mainUpdates, changedSystemPrompts };
}

/** One rendered segment of highlighted text (v4 `TemplateDisplay`). */
export interface HighlightSegment {
  text: string;
  kind: 'plain' | 'char-template' | 'user-template' | 'char-hardcoded' | 'user-hardcoded';
  title?: string;
}

interface ContentMatch {
  kind: Exclude<HighlightSegment['kind'], 'plain'>;
  start: number;
  end: number;
  text: string;
}

/**
 * Builds the highlighted-segment list for one field's display (v4
 * `TemplateDisplay`): `{{char}}`/`{{user}}` template literals are resolved to
 * the concrete name and highlighted (solid); hard-coded occurrences of the
 * character/user names are flagged as "could be templated" (dashed warning).
 */
export function highlightSegments(
  content: string,
  characterName: string,
  userCharacterName: string | null | undefined,
): HighlightSegment[] {
  if (!content) return [];

  const userName = userCharacterName || 'USER';
  const matches: ContentMatch[] = [];

  const push = (regex: RegExp, kind: ContentMatch['kind'], text: (m: RegExpExecArray) => string): void => {
    let m: RegExpExecArray | null;
    while ((m = regex.exec(content)) !== null) {
      const start = m.index;
      const end = start + m[0].length;
      const overlaps = matches.some((existing) => start < existing.end && end > existing.start);
      if (!overlaps) {
        matches.push({ kind, start, end, text: text(m) });
      }
    }
  };

  push(/\{\{char\}\}/gi, 'char-template', () => characterName);
  push(/\{\{user\}\}/gi, 'user-template', () => userName);
  if (characterName) {
    push(new RegExp(escapeRegex(characterName), 'gi'), 'char-hardcoded', (m) => m[0]);
  }
  if (userCharacterName) {
    push(new RegExp(escapeRegex(userCharacterName), 'gi'), 'user-hardcoded', (m) => m[0]);
  }

  if (matches.length === 0) {
    return [{ text: content, kind: 'plain' }];
  }

  matches.sort((a, b) => a.start - b.start);

  const segments: HighlightSegment[] = [];
  let lastEnd = 0;
  const titleFor: Record<ContentMatch['kind'], string> = {
    'char-template': 'Character name (from {{char}})',
    'user-template': userCharacterName ? 'User character name (from {{user}})' : 'User (no default user character set)',
    'char-hardcoded': 'Hard-coded character name — consider replacing with {{char}}',
    'user-hardcoded': 'Hard-coded user character name — consider replacing with {{user}}',
  };
  for (const match of matches) {
    if (match.start > lastEnd) {
      segments.push({ text: content.slice(lastEnd, match.start), kind: 'plain' });
    }
    segments.push({ text: match.text, kind: match.kind, title: titleFor[match.kind] });
    lastEnd = match.end;
  }
  if (lastEnd < content.length) {
    segments.push({ text: content.slice(lastEnd), kind: 'plain' });
  }
  return segments;
}
