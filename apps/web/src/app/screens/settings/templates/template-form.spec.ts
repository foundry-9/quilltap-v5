import { describe, expect, it } from 'vitest';

import type { TemplateDelimiter } from '../../../core/core-contract';
import {
  DEFAULT_DELIMITER_STYLE,
  delimiterToFormEntry,
  emptyDelimiterEntry,
  formEntriesToDelimiters,
  formEntryToDelimiter,
  formToNarration,
  narrationToForm,
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
