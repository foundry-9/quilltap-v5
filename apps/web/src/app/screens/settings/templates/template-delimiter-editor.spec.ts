import { Component, signal } from '@angular/core';
import { TestBed } from '@angular/core/testing';
import { describe, expect, it } from 'vitest';

import { TemplateDelimiterEditor } from './template-delimiter-editor';
import { emptyDelimiterEntry, type DelimiterFormEntry } from './template-form';

/** The delimiter editor's P4.D352 surface (v4 778b97b2b + eed72d60c at 236c04e35). */
@Component({
  imports: [TemplateDelimiterEditor],
  template: `<qt-template-delimiter-editor [(entries)]="entries" />`,
})
class Host {
  readonly entries = signal<DelimiterFormEntry[]>([]);
}

function mount(entries: DelimiterFormEntry[]) {
  TestBed.resetTestingModule();
  TestBed.configureTestingModule({ imports: [Host] });
  const fixture = TestBed.createComponent(Host);
  fixture.componentInstance.entries.set(entries);
  fixture.detectChanges();
  return fixture;
}

function checkboxByText(root: HTMLElement, text: string): HTMLInputElement | null {
  const label = Array.from(root.querySelectorAll('label')).find((l) =>
    (l.textContent ?? '').replace(/\s+/g, ' ').trim().startsWith(text),
  );
  return (label?.querySelector('input[type="checkbox"]') as HTMLInputElement) ?? null;
}

describe('TemplateDelimiterEditor (P4.D352)', () => {
  it('lists Plain first among the styles', () => {
    const fixture = mount([emptyDelimiterEntry()]);
    const options = (fixture.nativeElement as HTMLElement).querySelectorAll(
      '#qt-delimiter-styles option',
    );
    expect(options[0].getAttribute('value')).toBe('qt-roleplay-plain');
    expect(options[0].textContent).toBe('Plain (no chip — dress it with flourishes)');
    expect(options[1].getAttribute('value')).toBe('qt-chat-narration');
  });

  it('gives a wrap entry the two checkboxes, bound to its flags', () => {
    const fixture = mount([emptyDelimiterEntry()]);
    const root = fixture.nativeElement as HTMLElement;
    const curly = checkboxByText(root, 'Straight quotes also match curly ones (" and “…” alike)')!;
    const unclosed = checkboxByText(
      root,
      'May run unclosed to the end of a paragraph (speech carried over several)',
    )!;
    expect(curly.checked).toBe(false);
    curly.click();
    unclosed.click();
    fixture.detectChanges();
    const [entry] = fixture.componentInstance.entries();
    expect(entry.matchCurlyQuotes).toBe(true);
    expect(entry.closeOptional).toBe(true);
  });

  it('shows no such checkboxes on a linePrefix entry', () => {
    const fixture = mount([{ ...emptyDelimiterEntry(), kind: 'linePrefix', marker: '// ' }]);
    const root = fixture.nativeElement as HTMLElement;
    expect(checkboxByText(root, 'Straight quotes also match')).toBeNull();
    expect(checkboxByText(root, 'May run unclosed')).toBeNull();
  });

  it('names every control v4 names (R4 pairs, R6 aria-labels — eed72d60c)', () => {
    const fixture = mount([
      { ...emptyDelimiterEntry(), delimiterMode: 'pair' },
      { ...emptyDelimiterEntry(), kind: 'tagPrefix' },
      emptyDelimiterEntry(),
    ]);
    const root = fixture.nativeElement as HTMLElement;
    const labelled = (text: string) =>
      Array.from(root.querySelectorAll('label[for]')).filter(
        (l) => l.textContent!.trim() === text && root.querySelector(`#${l.getAttribute('for')}`),
      ).length;
    expect(labelled('Kind')).toBe(3);
    expect(labelled('Name')).toBe(3);
    expect(labelled('Button Label')).toBe(3);
    expect(labelled('Style (CSS class)')).toBe(3);
    expect(labelled('Brackets:')).toBe(1);
    expect(labelled('Token pattern (regex, Unicode)')).toBe(1);
    const aria = Array.from(root.querySelectorAll('input[aria-label]')).map((i) =>
      i.getAttribute('aria-label'),
    );
    expect(aria).toEqual(['Opening delimiter', 'Closing delimiter', 'Closing bracket', 'Delimiter']);
  });
});
