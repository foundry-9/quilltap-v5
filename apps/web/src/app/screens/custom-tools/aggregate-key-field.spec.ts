import { ComponentFixture, TestBed } from '@angular/core/testing';
import { afterEach, describe, expect, it } from 'vitest';

import { AGGREGATE_KEY_TITLE, AGGREGATE_SKELETONS, AggregateKeyField } from './aggregate-key-field';

/**
 * AggregateKeyField — v4 `AggregateKeyField.tsx` (`a53024d3c`): the key box of
 * an `inventoryAggregate` chip and its Insert-key helper. The statement lives in
 * v4's `workbench-inventory.test.tsx` (`fills the key box from the Insert-key
 * helper, keeping a typed metadata key`); the 24 skeletons and the title are
 * pinned here.
 */

function render(value: string, disabled = false) {
  TestBed.resetTestingModule();
  TestBed.configureTestingModule({ imports: [AggregateKeyField] });
  const fixture: ComponentFixture<AggregateKeyField> = TestBed.createComponent(AggregateKeyField);
  fixture.componentRef.setInput('value', value);
  fixture.componentRef.setInput('disabled', disabled);
  const emitted: string[] = [];
  fixture.componentInstance.valueChange.subscribe((v) => emitted.push(v));
  fixture.detectChanges();
  return { fixture, emitted };
}

const el = (f: ComponentFixture<unknown>) => f.nativeElement as HTMLElement;
const insert = (f: ComponentFixture<unknown>) =>
  el(f).querySelector<HTMLSelectElement>('select[aria-label="Insert an aggregate key"]')!;
const choose = (select: HTMLSelectElement, value: string) => {
  select.value = value;
  select.dispatchEvent(new Event('change'));
};

describe('AggregateKeyField (v4 AggregateKeyField.tsx)', () => {
  afterEach(() => TestBed.resetTestingModule());

  it('offers 24 skeletons in v4’s order after the Insert key… prompt', () => {
    const { fixture } = render('');
    const options = [...insert(fixture).options];
    expect(options[0].value).toBe('');
    expect(options[0].textContent?.trim()).toBe('Insert key…');
    expect(options.slice(1).map((o) => o.value)).toEqual(AGGREGATE_SKELETONS);
    expect(AGGREGATE_SKELETONS).toHaveLength(24);
    expect(AGGREGATE_SKELETONS.slice(0, 8)).toEqual([
      'equipped.count',
      'equipped.names',
      'equipped.titles',
      'equipped.metadata.',
      'equipped.metadata.<key>.min',
      'equipped.metadata.<key>.max',
      'equipped.metadata.<key>.sum',
      'equipped.metadata.<key>.count',
    ]);
    expect(AGGREGATE_SKELETONS[8]).toBe('owned.count');
    expect(AGGREGATE_SKELETONS[16]).toBe('reachable.count');
  });

  it('keeps a typed metadata key: equipped.metadata.toolVersion + owned.metadata.<key>.max → owned.metadata.toolVersion.max', () => {
    const { fixture, emitted } = render('equipped.metadata.toolVersion');
    choose(insert(fixture), 'owned.metadata.<key>.max');
    expect(emitted).toEqual(['owned.metadata.toolVersion.max']);
  });

  it('keeps the key of a folded key too', () => {
    const { fixture, emitted } = render('equipped.metadata.toolVersion.min');
    choose(insert(fixture), 'reachable.metadata.<key>.sum');
    expect(emitted).toEqual(['reachable.metadata.toolVersion.sum']);
  });

  it('a blank box and a fold skeleton → …metadata.key.max', () => {
    const { fixture, emitted } = render('');
    choose(insert(fixture), 'equipped.metadata.<key>.max');
    expect(emitted).toEqual(['equipped.metadata.key.max']);
  });

  it('equipped.names + a fold skeleton has no typed metadata key → …metadata.key.max', () => {
    const { fixture, emitted } = render('equipped.names');
    choose(insert(fixture), 'equipped.metadata.<key>.max');
    expect(emitted).toEqual(['equipped.metadata.key.max']);
  });

  it('a plain skeleton replaces the box as it is', () => {
    const { fixture, emitted } = render('whatever');
    choose(insert(fixture), 'owned.titles');
    expect(emitted).toEqual(['owned.titles']);
  });

  it('puts the select back on its prompt after every choice (the helper is stateless)', () => {
    const { fixture } = render('');
    const select = insert(fixture);
    choose(select, 'owned.names');
    expect(select.value).toBe('');
  });

  it('types into the box', () => {
    const { fixture, emitted } = render('');
    const input = el(fixture).querySelector<HTMLInputElement>('input[aria-label="Inventory aggregate key"]')!;
    input.value = 'equipped.count';
    input.dispatchEvent(new Event('input'));
    expect(emitted).toEqual(['equipped.count']);
  });

  it('carries the error class while blank, the placeholder, the width and v4’s title byte for byte', () => {
    const blank = el(render('  ').fixture).querySelector('input')!;
    expect(blank.getAttribute('class')).toBe('qt-input w-56 text-sm qt-input-error');
    const filled = el(render('equipped.count').fixture).querySelector('input')!;
    expect(filled.getAttribute('class')).toBe('qt-input w-56 text-sm ');
    expect(filled.getAttribute('placeholder')).toBe('equipped.metadata.toolVersion.max');
    expect(filled.getAttribute('title')).toBe(AGGREGATE_KEY_TITLE);
    expect(AGGREGATE_KEY_TITLE).toBe(
      'One attribute across everything in a scope. Scopes: equipped (in this chat), owned (in their own vault, equipped or not), reachable (everything they can see). Fields: count, names, titles, metadata.<key> (a list of the objects’ values), or metadata.<key>.min / .max / .sum / .count (a number). A list matches = when any entry does; ordering needs a fold.',
    );
  });

  it('the select’s class, aria-label and title', () => {
    const { fixture } = render('');
    expect(insert(fixture).getAttribute('class')).toBe('qt-select qt-select-sm w-24 shrink-0');
    expect(insert(fixture).getAttribute('title')).toBe('Fill the box with a scope and a field; type the metadata key yourself.');
  });

  it('disables both controls', () => {
    const { fixture } = render('', true);
    expect(el(fixture).querySelector('input')!.disabled).toBe(true);
    expect(insert(fixture).disabled).toBe(true);
  });
});
