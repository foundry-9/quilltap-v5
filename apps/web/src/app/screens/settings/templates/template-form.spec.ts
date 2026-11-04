import { describe, expect, it } from 'vitest';

import type { RoleplayTemplateDto, TemplateDelimiter } from '../../../core/core-contract';
import {
  DEFAULT_DELIMITER_STYLE,
  EMPTY_ADD_ONS,
  delimiterToFormEntry,
  emptyDelimiterEntry,
  emptyTemplateForm,
  formEntriesToDelimiters,
  formEntryToDelimiter,
  formToNarration,
  formToTemplateBody,
  isTemplateFormValid,
  narrationToForm,
  templateToForm,
  unmarkedFromForm,
} from './template-form';

describe('narration flatten', () => {
  it('maps a bare string to single mode with open = close', () => {
    expect(narrationToForm('*')).toEqual({ mode: 'single', open: '*', close: '*' });
  });

  it('defaults an empty/undefined string to `*`', () => {
    expect(narrationToForm(undefined)).toEqual({ mode: 'single', open: '*', close: '*' });
    expect(narrationToForm('')).toEqual({ mode: 'single', open: '*', close: '*' });
  });

  it('maps a tuple to pair mode', () => {
    expect(narrationToForm(['[', ']'])).toEqual({ mode: 'pair', open: '[', close: ']' });
  });

  it('serializes single mode to a string and pair mode to a tuple', () => {
    expect(formToNarration({ mode: 'single', open: '*', close: '*' })).toBe('*');
    expect(formToNarration({ mode: 'pair', open: '[', close: ']' })).toEqual(['[', ']']);
  });
});

describe('delimiter round-trip', () => {
  it('wrap single: DTO → form → DTO', () => {
    const d: TemplateDelimiter = {
      kind: 'wrap',
      name: 'Narration',
      buttonName: 'Nar',
      style: 'qt-chat-narration',
      delimiters: '*',
    };
    const entry = delimiterToFormEntry(d);
    expect(entry.delimiterMode).toBe('single');
    expect(entry.delimiterOpen).toBe('*');
    expect(formEntryToDelimiter(entry)).toEqual(d);
  });

  it('wrap pair: DTO → form → DTO', () => {
    const d: TemplateDelimiter = {
      kind: 'wrap',
      name: 'Action',
      buttonName: 'Act',
      style: 'qt-chat-dialogue',
      delimiters: ['[', ']'],
    };
    const entry = delimiterToFormEntry(d);
    expect(entry.delimiterMode).toBe('pair');
    expect(formEntryToDelimiter(entry)).toEqual(d);
  });

  it('linePrefix: DTO → form → DTO', () => {
    const d: TemplateDelimiter = {
      kind: 'linePrefix',
      name: 'OOC',
      buttonName: 'OOC',
      style: 'qt-chat-ooc',
      marker: '// ',
    };
    expect(formEntryToDelimiter(delimiterToFormEntry(d))).toEqual(d);
  });

  it('tagPrefix: DTO → form → DTO (tokenPattern preserved)', () => {
    const d: TemplateDelimiter = {
      kind: 'tagPrefix',
      name: 'Speaker',
      buttonName: 'Spk',
      style: 'qt-roleplay-1',
      open: '[',
      close: ']',
      tokenPattern: '[A-Z]+',
    };
    expect(formEntryToDelimiter(delimiterToFormEntry(d))).toEqual(d);
  });

  it('carries hideDelimiter + addOns through the round-trip', () => {
    const d: TemplateDelimiter = {
      kind: 'wrap',
      name: 'Bold span',
      buttonName: 'B',
      style: 'qt-roleplay-2',
      delimiters: '**',
      hideDelimiter: true,
      addOns: {
        bold: true,
        italic: false,
        reverse: false,
        underline: 'single',
        border: 'none',
        font: 'serif',
      },
    };
    const back = formEntryToDelimiter(delimiterToFormEntry(d));
    expect(back).toEqual(d);
  });
});

describe('formEntryToDelimiter omissions + drops', () => {
  it('drops an entry missing name or buttonName', () => {
    const entry = emptyDelimiterEntry();
    entry.name = 'Only a name';
    expect(formEntryToDelimiter(entry)).toBeNull();
  });

  it('falls back to the default style when blank', () => {
    const entry = emptyDelimiterEntry();
    entry.name = 'N';
    entry.buttonName = 'N';
    entry.delimiterOpen = '*';
    const out = formEntryToDelimiter(entry);
    expect(out).not.toBeNull();
    expect(out!.style).toBe(DEFAULT_DELIMITER_STYLE);
  });

  it('omits hideDelimiter + addOns at their defaults (keeps the DTO lean)', () => {
    const entry = emptyDelimiterEntry();
    entry.name = 'N';
    entry.buttonName = 'N';
    entry.style = 'qt-chat-narration';
    entry.delimiterOpen = '*';
    const out = formEntryToDelimiter(entry)!;
    expect('hideDelimiter' in out).toBe(false);
    expect('addOns' in out).toBe(false);
  });

  it('formEntriesToDelimiters filters the blank rows', () => {
    const good = emptyDelimiterEntry();
    good.name = 'N';
    good.buttonName = 'N';
    good.delimiterOpen = '*';
    const blank = emptyDelimiterEntry();
    expect(formEntriesToDelimiters([good, blank])).toHaveLength(1);
  });

  it('omits an empty tagPrefix tokenPattern', () => {
    const entry = emptyDelimiterEntry();
    entry.kind = 'tagPrefix';
    entry.name = 'T';
    entry.buttonName = 'T';
    entry.style = 'qt-roleplay-1';
    entry.tagOpen = '[';
    entry.tagClose = ']';
    entry.tokenPattern = '';
    const out = formEntryToDelimiter(entry)!;
    expect('tokenPattern' in out).toBe(false);
  });
});

// ---------------------------------------------------------------------------
// P4.D352 — the Novel template's SPA half (v4 778b97b2b at 236c04e35,
// `hooks/useRoleplayTemplates.ts:69-70, 96-97, 151-152, 158-190, 288-291`).
// ---------------------------------------------------------------------------

/** The Novel bag, from v4 `lib/chat/novel-template.ts:26-52` + `:94-119`. */
const NOVEL_DTO: RoleplayTemplateDto = {
  id: '00000000-0000-4000-8000-000000000003',
  userId: null,
  name: 'Novel',
  description:
    'Prose as a novel prints it: plain narration, "dialogue" in quotes, *emphasis* in single asterisks, and ((out of character)) in double parentheses.',
  systemPrompt: '[SYSTEM INSTRUCTION: NOVEL FORMATTING]',
  isBuiltIn: true,
  tags: [],
  delimiters: [
    {
      kind: 'wrap',
      name: 'Dialogue',
      buttonName: 'Dia',
      delimiters: '"',
      style: 'qt-chat-dialogue',
      matchCurlyQuotes: true,
      closeOptional: true,
    },
    {
      kind: 'wrap',
      name: 'Emphasis',
      buttonName: 'Emph',
      delimiters: '*',
      style: 'qt-roleplay-plain',
      hideDelimiter: true,
      addOns: { bold: true, italic: true, reverse: false, underline: 'none', border: 'none', font: '' },
    },
    { kind: 'wrap', name: 'Out of Character', buttonName: 'OOC', delimiters: ['((', '))'], style: 'qt-chat-ooc' },
  ],
  renderingPatterns: [],
  dialogueDetection: { openingChars: ['"', '“'], closingChars: ['"', '”'], className: 'qt-chat-dialogue' },
  narrationDelimiters: null,
  unmarkedStyle: {
    style: 'qt-roleplay-plain',
    addOns: { bold: false, italic: true, reverse: false, underline: 'none', border: 'none', font: '' },
  },
  markdownAsterisks: false,
  createdAt: '2026-10-22T00:00:00.000Z',
  updatedAt: '2026-10-22T00:00:00.000Z',
};

describe('narration none ↔ null (P4.D352)', () => {
  it('reads a null narration as `none`, the inputs falling back to `*`', () => {
    expect(narrationToForm(null)).toEqual({ mode: 'none', open: '*', close: '*' });
  });

  it('writes `none` as null', () => {
    expect(formToNarration({ mode: 'none', open: '*', close: '*' })).toBeNull();
  });
});

describe('the two wrap flags (P4.D352)', () => {
  it('reads them from a wrap, defaulting false', () => {
    const entry = delimiterToFormEntry(NOVEL_DTO.delimiters[0]);
    expect(entry.matchCurlyQuotes).toBe(true);
    expect(entry.closeOptional).toBe(true);
    const plain = delimiterToFormEntry(NOVEL_DTO.delimiters[2]);
    expect(plain.matchCurlyQuotes).toBe(false);
    expect(plain.closeOptional).toBe(false);
    expect(emptyDelimiterEntry().matchCurlyQuotes).toBe(false);
    expect(emptyDelimiterEntry().closeOptional).toBe(false);
  });

  it('emits each key only when ticked', () => {
    const ticked = formEntryToDelimiter(delimiterToFormEntry(NOVEL_DTO.delimiters[0]))!;
    expect(ticked).toEqual(NOVEL_DTO.delimiters[0]);
    const unticked = formEntryToDelimiter({
      ...delimiterToFormEntry(NOVEL_DTO.delimiters[0]),
      matchCurlyQuotes: false,
      closeOptional: false,
    })!;
    expect('matchCurlyQuotes' in unticked).toBe(false);
    expect('closeOptional' in unticked).toBe(false);
  });

  it('never emits them on a linePrefix entry', () => {
    const line = formEntryToDelimiter({
      ...emptyDelimiterEntry(),
      kind: 'linePrefix',
      name: 'Aside',
      buttonName: 'As',
      marker: '//',
      matchCurlyQuotes: true,
      closeOptional: true,
    })!;
    expect('matchCurlyQuotes' in line).toBe(false);
    expect('closeOptional' in line).toBe(false);
  });
});

describe('unmarkedFromForm (v4 :164-169)', () => {
  const base = { ...emptyTemplateForm(), name: 'x', systemPrompt: 'y' };

  it('answers null while unmarked text is not styled', () => {
    expect(unmarkedFromForm({ ...base, unmarkedEnabled: false })).toBeNull();
  });

  it('falls back to qt-roleplay-plain and omits addOns at their defaults', () => {
    expect(unmarkedFromForm({ ...base, unmarkedEnabled: true, unmarkedStyle: '  ' })).toEqual({
      style: 'qt-roleplay-plain',
    });
  });

  it('carries addOns once any flourish is set', () => {
    expect(
      unmarkedFromForm({
        ...base,
        unmarkedEnabled: true,
        unmarkedStyle: 'qt-roleplay-plain',
        unmarkedAddOns: { ...EMPTY_ADD_ONS, italic: true },
      }),
    ).toEqual({ style: 'qt-roleplay-plain', addOns: { ...EMPTY_ADD_ONS, italic: true } });
  });
});

describe('the Novel template through the form (P4.D352)', () => {
  it('seeds every Novel control from the DTO — edit and copy-as-new alike', () => {
    for (const form of [templateToForm(NOVEL_DTO), templateToForm(NOVEL_DTO, { copy: true })]) {
      expect(form.narration.mode).toBe('none');
      expect(form.unmarkedEnabled).toBe(true);
      expect(form.unmarkedStyle).toBe('qt-roleplay-plain');
      expect(form.unmarkedAddOns).toEqual({ ...EMPTY_ADD_ONS, italic: true });
      expect(form.markdownAsterisks).toBe(false);
    }
    expect(templateToForm(NOVEL_DTO, { copy: true }).name).toBe('Novel (Copy)');
  });

  it('reads an absent markdownAsterisks / unmarkedStyle as v4 does', () => {
    const { markdownAsterisks: _m, unmarkedStyle: _u, ...older } = NOVEL_DTO;
    const form = templateToForm({ ...older, narrationDelimiters: '*' });
    expect(form.markdownAsterisks).toBe(true);
    expect(form.unmarkedEnabled).toBe(false);
    expect(form.unmarkedStyle).toBe('qt-roleplay-plain');
  });

  it('round-trips to v4’s save payload', () => {
    const body = formToTemplateBody(templateToForm(NOVEL_DTO));
    expect(body.narrationDelimiters).toBeNull();
    expect(body.unmarkedStyle).toEqual(NOVEL_DTO.unmarkedStyle);
    expect(body.markdownAsterisks).toBe(false);
    expect(body.delimiters).toEqual(NOVEL_DTO.delimiters);
  });

  it('a blank form defaults to v4’s INITIAL_FORM_DATA', () => {
    const blank = emptyTemplateForm();
    expect(blank.unmarkedEnabled).toBe(false);
    expect(blank.unmarkedStyle).toBe('qt-roleplay-plain');
    expect(blank.unmarkedAddOns).toEqual(EMPTY_ADD_ONS);
    expect(blank.markdownAsterisks).toBe(true);
    expect(formToTemplateBody({ ...blank, name: 'n', systemPrompt: 'p' })).toMatchObject({
      narrationDelimiters: '*',
      unmarkedStyle: null,
      markdownAsterisks: true,
    });
  });

  it('is valid in none mode with empty narration inputs (v4 :868)', () => {
    const form = { ...emptyTemplateForm(), name: 'n', systemPrompt: 'p' };
    expect(isTemplateFormValid({ ...form, narration: { mode: 'none', open: '', close: '' } })).toBe(true);
    expect(isTemplateFormValid({ ...form, narration: { mode: 'single', open: ' ', close: '' } })).toBe(false);
    expect(isTemplateFormValid({ ...form, narration: { mode: 'pair', open: '[', close: '' } })).toBe(false);
  });
});
