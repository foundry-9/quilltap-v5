/**
 * The roleplay-template form model + the delimiter/narration ↔ DTO mapping
 * helpers, transcribed from v4 `components/settings/roleplay-templates/index.tsx`
 * (`delimiterToFormEntry` / `formEntriesToDelimiters` / the narration flatten)
 * and `types.ts`. Pure functions — exercised by `template-form.spec.ts`.
 */

import type {
  RoleplayTemplateDto,
  TemplateDelimiter,
  TemplateDelimiterAddOns,
} from '../../../core/core-contract';

/** v4 `DEFAULT_TAG_TOKEN_PATTERN` — uppercase / non-cased, never lowercase. */
export const DEFAULT_TAG_TOKEN_PATTERN = '[^\\p{Ll}]+';

/** Falls back to when a user leaves the style field blank (v4 save). */
export const DEFAULT_DELIMITER_STYLE = 'qt-chat-narration';

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
    hideDelimiter: false,
    addOns: { ...EMPTY_ADD_ONS },
  };
}

/** The narration delimiters, flattened for the mode radios + inputs. */
export interface NarrationForm {
  mode: 'single' | 'pair';
  open: string;
  close: string;
}

/** v4 open-for-edit: pair → [open, close]; else both = the string (default `*`). */
export function narrationToForm(nd: string | [string, string] | undefined): NarrationForm {
  if (Array.isArray(nd)) {
    return { mode: 'pair', open: nd[0] ?? '', close: nd[1] ?? '' };
  }
  const value = typeof nd === 'string' && nd.length > 0 ? nd : '*';
  return { mode: 'single', open: value, close: value };
}

/** v4 save: `mode === 'pair' ? [open, close] : open`. */
export function formToNarration(form: NarrationForm): string | [string, string] {
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
    return { kind: 'wrap', ...common, delimiters, ...decorations };
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
}

/** A blank create form (v4 `INITIAL_FORM_DATA`). */
export function emptyTemplateForm(): TemplateFormData {
  return {
    name: '',
    description: '',
    systemPrompt: '',
    narration: { mode: 'single', open: '*', close: '*' },
    delimiters: [],
  };
}

/** Load a template into the form (edit) or with a `(Copy)` name (copy-as-new). */
export function templateToForm(
  template: RoleplayTemplateDto,
  opts?: { copy?: boolean },
): TemplateFormData {
  return {
    name: opts?.copy ? `${template.name} (Copy)` : template.name,
    description: template.description ?? '',
    systemPrompt: template.systemPrompt ?? '',
    narration: narrationToForm(template.narrationDelimiters),
    delimiters: (template.delimiters ?? []).map(delimiterToFormEntry),
  };
}

/** Whether the form may be submitted (v4 disabled-submit predicate). */
export function isTemplateFormValid(form: TemplateFormData): boolean {
  return (
    form.name.trim().length > 0 &&
    form.systemPrompt.trim().length > 0 &&
    form.narration.open.trim().length > 0 &&
    (form.narration.mode !== 'pair' || form.narration.close.trim().length > 0)
  );
}

/** Build the create/update body (the client omits `renderingPatterns`). */
export function formToTemplateBody(form: TemplateFormData): {
  name: string;
  description: string | null;
  systemPrompt: string;
  narrationDelimiters: string | [string, string];
  delimiters: TemplateDelimiter[];
} {
  return {
    name: form.name.trim(),
    description: form.description.trim() || null,
    systemPrompt: form.systemPrompt,
    narrationDelimiters: formToNarration(form.narration),
    delimiters: formEntriesToDelimiters(form.delimiters),
  };
}
