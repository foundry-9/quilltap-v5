import { ComponentFixture, TestBed } from '@angular/core/testing';
import { afterEach, describe, expect, it } from 'vitest';

import { CustomToolWardrobeLines, wardrobeReferenceLines } from './custom-tool-wardrobe-lines';
import type { CustomToolReferences } from './custom-tools.api';

/**
 * The run dialog's wardrobe sentences, proven alone — v4
 * `CustomToolRunDialog.tsx:926-936` + `:1001-1014` (`f0db8cbb8`, pinned at
 * `25b8fab6a`). The popup wire is the unifier's (three edits); this is the
 * pure function and the component it will mount.
 */

const BASE: CustomToolReferences = {
  value: false,
  roll: false,
  dice: false,
  llm: false,
  params: [],
  metadata: [],
  state: [],
};

async function render(
  references: CustomToolReferences | undefined,
): Promise<ComponentFixture<CustomToolWardrobeLines>> {
  TestBed.resetTestingModule();
  TestBed.configureTestingModule({ imports: [CustomToolWardrobeLines] });
  const fixture = TestBed.createComponent(CustomToolWardrobeLines);
  fixture.componentRef.setInput('references', references);
  fixture.detectChanges();
  return fixture;
}

const paragraphs = (f: ComponentFixture<unknown>) =>
  [...(f.nativeElement as HTMLElement).querySelectorAll('p')];
const said = (p: HTMLElement) => p.textContent?.replace(/\s+/g, ' ').trim();

describe('wardrobeReferenceLines (v4 CustomToolRunDialog.tsx:926-932)', () => {
  it('leads each promise in v4’s words, in the order wears → takesOff → clearsSlots', () => {
    expect(
      wardrobeReferenceLines({ ...BASE, clearsSlots: ['hair'], takesOff: ['Sundress'], wears: ['Cloak of Night'] }),
    ).toEqual([
      { key: 'wears', lead: 'It may dress the running character in', values: ['Cloak of Night'], suffix: '' },
      { key: 'takesOff', lead: 'It may undress them of', values: ['Sundress'], suffix: '' },
      { key: 'clearsSlots', lead: 'It may clear their', values: ['hair'], suffix: ' slot' },
    ]);
  });

  it('says slots for more than one cleared slot, slot for one', () => {
    expect(wardrobeReferenceLines({ ...BASE, clearsSlots: ['hair', 'top'] })[0].suffix).toBe(' slots');
    expect(wardrobeReferenceLines({ ...BASE, clearsSlots: ['hair'] })[0].suffix).toBe(' slot');
  });

  it('drops a line with no values, and answers nothing for an older server', () => {
    expect(wardrobeReferenceLines({ ...BASE, wears: [], takesOff: ['Hat'] }).map((l) => l.key)).toEqual(['takesOff']);
    expect(wardrobeReferenceLines(BASE)).toEqual([]);
    expect(wardrobeReferenceLines(undefined)).toEqual([]);
  });
});

describe('CustomToolWardrobeLines (v4 CustomToolRunDialog.tsx:1001-1014)', () => {
  afterEach(() => TestBed.resetTestingModule());

  it('renders one paragraph per line in the writes-paragraph idiom, the tail sentence after each', async () => {
    const fixture = await render({
      ...BASE,
      wears: ['Cloak of Night', 'Boots'],
      takesOff: ['Sundress'],
      clearsSlots: ['hair', 'top'],
    });
    const ps = paragraphs(fixture);
    expect(ps).toHaveLength(3);
    for (const p of ps) expect(p.className).toBe('text-xs qt-text-secondary mt-4 leading-relaxed');
    expect(ps.map(said)).toEqual([
      'It may dress the running character in Cloak of Night, Boots. Only when you run it as a character — a run made as nobody dresses nobody.',
      'It may undress them of Sundress. Only when you run it as a character — a run made as nobody dresses nobody.',
      'It may clear their hair, top slots. Only when you run it as a character — a run made as nobody dresses nobody.',
    ]);
    const codes = [...ps[0].querySelectorAll('code')];
    expect(codes.map((c) => c.textContent)).toEqual(['Cloak of Night', 'Boots']);
    for (const c of codes) expect(c.className).toBe('font-mono qt-text');
  });

  it('joins the values with ", " and nothing else — no space before the full stop or the suffix', async () => {
    const fixture = await render({ ...BASE, clearsSlots: ['hair'] });
    expect(paragraphs(fixture)[0].textContent?.replace(/\s+/g, ' ').trim()).toBe(
      'It may clear their hair slot. Only when you run it as a character — a run made as nobody dresses nobody.',
    );
    const two = await render({ ...BASE, wears: ['A', 'B'] });
    expect(paragraphs(two)[0].textContent).toContain('A, B.');
  });

  it('renders nothing when every list is empty, or references are undefined', async () => {
    expect(paragraphs(await render({ ...BASE, wears: [], takesOff: [], clearsSlots: [] }))).toHaveLength(0);
    expect(paragraphs(await render(undefined))).toHaveLength(0);
    expect(paragraphs(await render(BASE))).toHaveLength(0);
  });
});
