/**
 * The roleplay-template form model + the delimiter/narration ↔ DTO mapping
 * helpers, transcribed from v4 `components/settings/roleplay-templates/index.tsx`
 * (`delimiterToFormEntry` / `formEntriesToDelimiters` / the narration flatten)
 * and `types.ts`. Pure functions — exercised by `template-form.spec.ts`.
 *
 * P4.D352 adds the Novel template's SPA half (v4 `778b97b2b` at 236c04e35,
 * `hooks/useRoleplayTemplates.ts`): the `none` narration mode (stored as
 * `null` — unmarked prose), the unmarked-text style, the Markdown-asterisks
 * switch, and the two `wrap` flags.
 */

import type {
  NarrationDelimiters,
  RoleplayTemplateDto,
  TemplateDelimiter,
  TemplateDelimiterAddOns,
  UnmarkedStyle,
} from '../../../core/core-contract';

/** v4 `DEFAULT_TAG_TOKEN_PATTERN` — uppercase / non-cased, never lowercase. */
export const DEFAULT_TAG_TOKEN_PATTERN = '[^\\p{Ll}]+';

/** Falls back to when a user leaves the style field blank (v4 save). */
export const DEFAULT_DELIMITER_STYLE = 'qt-chat-narration';

/** A labelled `<option>` (the style datalists, the font select). */
export interface StyleOption {
  value: string;
  label: string;
}

/**
 * v4 `STYLE_OPTIONS` (`index.tsx:17-33`) — the built-in theme classes offered
 * in the style datalists (each delimiter's and the unmarked-text one).
 *
 * `qt-roleplay-plain` ships NO CSS rule, in v4 or here, by design (ruling R-6):
 * it is the chip-less base a writer dresses with flourishes. Do not "fix" it by
 * adding one.
 */
export const STYLE_OPTIONS: readonly StyleOption[] = [
  { value: 'qt-roleplay-plain', label: 'Plain (no chip — dress it with flourishes)' },
  { value: 'qt-chat-narration', label: 'Narration' },
  { value: 'qt-chat-dialogue', label: 'Dialogue' },
  { value: 'qt-chat-ooc', label: 'Out of Character' },
  { value: 'qt-chat-inner-monologue', label: 'Inner Monologue' },
  { value: 'qt-roleplay-1', label: 'Style 1 (high contrast)' },
  { value: 'qt-roleplay-2', label: 'Style 2 (high contrast)' },
  { value: 'qt-roleplay-3', label: 'Style 3 (high contrast)' },
  { value: 'qt-roleplay-4', label: 'Style 4 (high contrast)' },
  { value: 'qt-roleplay-danger', label: 'Danger' },
  { value: 'qt-roleplay-warning', label: 'Warning' },
  { value: 'qt-roleplay-success', label: 'Success' },
  { value: 'qt-roleplay-info', label: 'Info' },
  { value: 'qt-roleplay-muted', label: 'Muted' },
  { value: 'qt-roleplay-code', label: 'Code' },
];

/** v4 `FONT_OPTIONS` — the flourish font choices. */
export const FONT_OPTIONS: readonly StyleOption[] = [
  { value: '', label: 'Default' },
  { value: 'sans', label: 'Sans' },
  { value: 'serif', label: 'Serif' },
  { value: 'mono', label: 'Mono' },
  { value: 'display', label: 'Display' },
  { value: 'script', label: 'Script' },
];

/** The unmarked-text class a blank style field falls back to (v4 `unmarkedFromForm`). */
export const DEFAULT_UNMARKED_STYLE = 'qt-roleplay-plain';

/** v4 `EMPTY_ADD_ONS` — every flourish at its schema default. */
export type FullAddOns = Required<TemplateDelimiterAddOns>;

export const EMPTY_ADD_ONS: FullAddOns = {
  bold: false,
  italic: false,
  reverse: false,
  underline: 'none',
  border: 'none',
  font: '',
};

/** The three narration/delimiter kinds a toolbar entry may take. */
export type DelimiterKind = 'wrap' | 'linePrefix' | 'tagPrefix';

/** One editable delimiter row (v4's flattened form entry). */
export interface DelimiterFormEntry {
  kind: DelimiterKind;
  name: string;
  buttonName: string;
  style: string;
  // wrap
  delimiterMode: 'single' | 'pair';
  delimiterOpen: string;
  delimiterClose: string;
  // linePrefix
  marker: string;
  // tagPrefix
  tagOpen: string;
  tagClose: string;
  tokenPattern: string;
  /** wrap only: straight quotes also match curly ones (v4 `778b97b2b`). */
  matchCurlyQuotes: boolean;
  /** wrap only: may run unclosed to the end of a paragraph. */
  closeOptional: boolean;
  // decorations (all kinds)
  hideDelimiter: boolean;
  addOns: FullAddOns;
}

/** v4 `EMPTY_DELIMITER` — a fresh `wrap` entry. */
export function emptyDelimiterEntry(): DelimiterFormEntry {
  return {
    kind: 'wrap',
    name: '',
    buttonName: '',
    style: '',
    delimiterMode: 'single',
    delimiterOpen: '',
    delimiterClose: '',
    marker: '',
    tagOpen: '',
    tagClose: '',
    tokenPattern: '',
    matchCurlyQuotes: false,
    closeOptional: false,
    hideDelimiter: false,
    addOns: { ...EMPTY_ADD_ONS },
  };
}

/** The narration delimiters, flattened for the mode radios + inputs. */
export interface NarrationForm {
  /** `none` = unmarked prose (v4 `778b97b2b`), stored as `null`. */
  mode: 'single' | 'pair' | 'none';
  open: string;
  close: string;
}

/**
 * v4 open-for-edit (`templateToFormData :171-190`): `null` → `none` (the inputs
 * still fall back to `*`); pair → [open, close]; else both = the string
 * (default `*`).
 */
export function narrationToForm(nd: NarrationDelimiters | null | undefined): NarrationForm {
  if (nd === null) {
    return { mode: 'none', open: '*', close: '*' };
  }
  if (Array.isArray(nd)) {
    return { mode: 'pair', open: nd[0] ?? '', close: nd[1] ?? '' };
  }
  const value = typeof nd === 'string' && nd.length > 0 ? nd : '*';
  return { mode: 'single', open: value, close: value };
}

/** v4 `narrationFromForm` (`:158-162`): `none` → `null`; pair → the tuple; else the string. */
export function formToNarration(form: NarrationForm): NarrationDelimiters | null {
  if (form.mode === 'none') return null;
  return form.mode === 'pair' ? [form.open, form.close] : form.open;
}

/** Merge a partial addOns over the defaults (v4 loads over `EMPTY_ADD_ONS`). */
function mergeAddOns(addOns: TemplateDelimiterAddOns | undefined): FullAddOns {
  return { ...EMPTY_ADD_ONS, ...(addOns ?? {}) };
}

/** v4 `delimiterToFormEntry` — a stored delimiter → the editable form entry. */
export function delimiterToFormEntry(d: TemplateDelimiter): DelimiterFormEntry {
  const base = emptyDelimiterEntry();
  base.kind = d.kind;
  base.name = d.name ?? '';
  base.buttonName = d.buttonName ?? '';
  base.style = d.style ?? '';
  base.hideDelimiter = d.hideDelimiter ?? false;
  base.addOns = mergeAddOns(d.addOns);

  if (d.kind === 'wrap') {
    if (Array.isArray(d.delimiters)) {
      base.delimiterMode = 'pair';
      base.delimiterOpen = d.delimiters[0] ?? '';
      base.delimiterClose = d.delimiters[1] ?? '';
    } else {
      base.delimiterMode = 'single';
      base.delimiterOpen = d.delimiters ?? '';
      base.delimiterClose = d.delimiters ?? '';
    }
    // v4 `:96-97` — wrap only; absent reads false.
    base.matchCurlyQuotes = d.matchCurlyQuotes ?? false;
    base.closeOptional = d.closeOptional ?? false;
  } else if (d.kind === 'linePrefix') {
    base.marker = d.marker ?? '';
  } else {
    base.tagOpen = d.open ?? '';
    base.tagClose = d.close ?? '';
    base.tokenPattern = d.tokenPattern ?? '';
  }
  return base;
}

/** True when any flourish deviates from the defaults (v4 `hasAnyAddOn`). */
export function hasAnyAddOn(addOns: FullAddOns): boolean {
  return (
    addOns.bold ||
    addOns.italic ||
    addOns.reverse ||
    addOns.underline !== 'none' ||
    addOns.border !== 'none' ||
    addOns.font !== ''
  );
}

/**
 * v4 `formEntryToDelimiter` — one form entry → a `TemplateDelimiter`, or `null`
 * when it lacks a `name` or `buttonName` (both required; blanks are dropped).
 * Decorations are OMITTED at their defaults to keep simple delimiters lean.
 */
export function formEntryToDelimiter(entry: DelimiterFormEntry): TemplateDelimiter | null {
  const name = entry.name.trim();
  const buttonName = entry.buttonName.trim();
  if (!name || !buttonName) {
    return null;
  }
  const style = entry.style.trim() || DEFAULT_DELIMITER_STYLE;

  const common: { name: string; buttonName: string; style: string } = { name, buttonName, style };
  const decorations: { hideDelimiter?: boolean; addOns?: FullAddOns } = {};
  if (entry.hideDelimiter) {
    decorations.hideDelimiter = true;
  }
  if (hasAnyAddOn(entry.addOns)) {
    decorations.addOns = { ...entry.addOns };
  }

  if (entry.kind === 'wrap') {
    const delimiters: string | [string, string] =
      entry.delimiterMode === 'pair'
        ? [entry.delimiterOpen, entry.delimiterClose]
        : entry.delimiterOpen;
    // v4 `:151-152`: each flag is emitted ONLY when ticked — an unticked box
    // sends no key at all.
    return {
      kind: 'wrap',
      ...common,
      delimiters,
      ...decorations,
      ...(entry.matchCurlyQuotes ? { matchCurlyQuotes: true as const } : {}),
      ...(entry.closeOptional ? { closeOptional: true as const } : {}),
    };
  }
  if (entry.kind === 'linePrefix') {
    return { kind: 'linePrefix', ...common, marker: entry.marker, ...decorations };
  }
  const tokenPattern = entry.tokenPattern.trim();
  return {
    kind: 'tagPrefix',
    ...common,
    open: entry.tagOpen,
    close: entry.tagClose,
    ...(tokenPattern ? { tokenPattern } : {}),
    ...decorations,
  };
}

/** v4 `formEntriesToDelimiters` — map + drop the null (blank) entries. */
export function formEntriesToDelimiters(entries: DelimiterFormEntry[]): TemplateDelimiter[] {
  return entries
    .map(formEntryToDelimiter)
    .filter((d): d is TemplateDelimiter => d !== null);
}

/** The whole editable form (the four scalars + narration + the delimiter list). */
export interface TemplateFormData {
  name: string;
  description: string;
  systemPrompt: string;
  narration: NarrationForm;
  delimiters: DelimiterFormEntry[];
  /** Style the text no delimiter claims (v4 `778b97b2b`). */
  unmarkedEnabled: boolean;
  unmarkedStyle: string;
  unmarkedAddOns: FullAddOns;
  /** Markdown reads asterisks as bold / italic (`false` ⇒ Novel's pre-parse). */
  markdownAsterisks: boolean;
}

/** A blank create form (v4 `INITIAL_FORM_DATA`, `types.ts:101-117`). */
export function emptyTemplateForm(): TemplateFormData {
  return {
    name: '',
    description: '',
    systemPrompt: '',
    narration: { mode: 'single', open: '*', close: '*' },
    delimiters: [],
    unmarkedEnabled: false,
    unmarkedStyle: DEFAULT_UNMARKED_STYLE,
    unmarkedAddOns: { ...EMPTY_ADD_ONS },
    markdownAsterisks: true,
  };
}

/**
 * Load a template into the form (edit) or with a `(Copy)` name (copy-as-new) —
 * v4 `templateToFormData` (`:171-190`), one function for both.
 */
export function templateToForm(
  template: RoleplayTemplateDto,
  opts?: { copy?: boolean },
): TemplateFormData {
  const unmarked = template.unmarkedStyle ?? null;
  return {
    name: opts?.copy ? `${template.name} (Copy)` : template.name,
    description: template.description ?? '',
    systemPrompt: template.systemPrompt ?? '',
    narration: narrationToForm(template.narrationDelimiters),
    delimiters: (template.delimiters ?? []).map(delimiterToFormEntry),
    unmarkedEnabled: unmarked !== null,
    unmarkedStyle: unmarked?.style ?? DEFAULT_UNMARKED_STYLE,
    unmarkedAddOns: mergeAddOns(unmarked?.addOns),
    markdownAsterisks: template.markdownAsterisks ?? true,
  };
}

/**
 * v4 `unmarkedFromForm` (`:164-169`): `null` while unmarked text is not styled;
 * else the style (blank → `qt-roleplay-plain`) with `addOns` only when a
 * flourish is set.
 */
export function unmarkedFromForm(form: TemplateFormData): UnmarkedStyle | null {
  if (!form.unmarkedEnabled) return null;
  const style = form.unmarkedStyle.trim() || DEFAULT_UNMARKED_STYLE;
  return hasAnyAddOn(form.unmarkedAddOns) ? { style, addOns: { ...form.unmarkedAddOns } } : { style };
}

/**
 * Whether the form may be submitted — v4's Save predicate (`index.tsx:868`):
 * a name and a prompt, and narration inputs only when narration is marked.
 */
export function isTemplateFormValid(form: TemplateFormData): boolean {
  return (
    form.name.trim().length > 0 &&
    form.systemPrompt.trim().length > 0 &&
    !(
      form.narration.mode !== 'none' &&
      (!form.narration.open.trim() ||
        (form.narration.mode === 'pair' && !form.narration.close.trim()))
    )
  );
}

/**
 * Build the create/update body (the client omits `renderingPatterns`). v4's
 * save payload (`:288-291`) always sends `narrationDelimiters` (possibly
 * `null`), `unmarkedStyle` (possibly `null`) and `markdownAsterisks`.
 */
export function formToTemplateBody(form: TemplateFormData): {
  name: string;
  description: string | null;
  systemPrompt: string;
  narrationDelimiters: NarrationDelimiters | null;
  delimiters: TemplateDelimiter[];
  unmarkedStyle: UnmarkedStyle | null;
  markdownAsterisks: boolean;
} {
  return {
    name: form.name.trim(),
    description: form.description.trim() || null,
    systemPrompt: form.systemPrompt,
    narrationDelimiters: formToNarration(form.narration),
    delimiters: formEntriesToDelimiters(form.delimiters),
    unmarkedStyle: unmarkedFromForm(form),
    markdownAsterisks: form.markdownAsterisks,
  };
}
