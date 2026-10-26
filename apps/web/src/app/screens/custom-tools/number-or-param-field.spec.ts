import { ComponentFixture, TestBed } from '@angular/core/testing';
import { afterEach, describe, expect, it } from 'vitest';

import type { NumberOrParamValue } from '../../pascal/tool-draft';
import { NumberOrParamField } from './number-or-param-field';

/**
 * NumberOrParamField — v4 `NumberOrParamField.tsx` at the pin: a literal, a
 * parameter pick, a carried `$state` pill and (`e1872c94f`) a carried
 * `$inventory` pill. v4 has no jsdom test of its own for it; these rows pin the
 * round's additions and the markup they sit beside.
 */

function render(value: NumberOrParamValue, extra: Record<string, unknown> = {}) {
  TestBed.resetTestingModule();
  TestBed.configureTestingModule({ imports: [NumberOrParamField] });
  const fixture: ComponentFixture<NumberOrParamField> = TestBed.createComponent(NumberOrParamField);
  fixture.componentRef.setInput('value', value);
  fixture.componentRef.setInput('paramNames', ['layers']);
  fixture.componentRef.setInput('label', 'min');
  for (const [k, v] of Object.entries(extra)) fixture.componentRef.setInput(k, v);
  const emitted: NumberOrParamValue[] = [];
  fixture.componentInstance.valueChange.subscribe((v) => emitted.push(v));
  fixture.detectChanges();
  return { fixture, emitted };
}

const el = (f: ComponentFixture<unknown>) => f.nativeElement as HTMLElement;
const pill = (f: ComponentFixture<unknown>) => el(f).querySelector<HTMLElement>('span.font-mono')!;
const toggle = (f: ComponentFixture<unknown>) =>
  el(f).querySelector<HTMLButtonElement>('button[aria-label^="Toggle"]')!;

describe('NumberOrParamField — the carried references', () => {
  afterEach(() => TestBed.resetTestingModule());

  it('shows a $state reference as a read-only pill with its title', () => {
    const { fixture } = render({ kind: 'state', path: 'luck.total', fallback: '1' });
    expect(pill(fixture).textContent?.trim()).toBe('$state: luck.total → 1');
    expect(pill(fixture).getAttribute('title')).toBe(
      'This field draws from persistent state. Edit $state references in the raw JSON, or swap to a literal.',
    );
  });

  it('shows a $inventory reference as a read-only pill, classes as the $state pill’s', () => {
    const { fixture } = render({ kind: 'inventory', path: 'sword.metadata.reach', fallback: '2' });
    expect(pill(fixture).textContent?.trim()).toBe('$inventory: sword.metadata.reach → 2');
    expect(pill(fixture).getAttribute('class')).toBe('qt-text-xs qt-text-secondary font-mono rounded qt-bg-muted px-2 py-1');
    expect(pill(fixture).getAttribute('title')).toBe(
      'This field draws from an object the rolling character carries. Edit $inventory references in the raw JSON, or swap to a literal.',
    );
  });

  it('adds the error class to the pill while the field has an error', () => {
    const { fixture } = render({ kind: 'inventory', path: 'sword.metadata.reach', fallback: 'x' }, { hasError: true });
    expect(pill(fixture).classList.contains('qt-input-error')).toBe(true);
  });

  it.each([
    ['state', { kind: 'state', path: 'a.b', fallback: '1' }, 'Replace this $state reference with a literal number'],
    ['inventory', { kind: 'inventory', path: 'a.metadata.b', fallback: '1' }, 'Replace this $inventory reference with a literal number'],
  ] as Array<[string, NumberOrParamValue, string]>)('the %s toggle’s title, and the swap to a literal', (_name, value, title) => {
    const { fixture, emitted } = render(value);
    expect(toggle(fixture).getAttribute('title')).toBe(title);
    toggle(fixture).click();
    expect(emitted).toEqual([{ kind: 'literal', text: '' }]);
  });

  it('keeps the literal and parameter toggle titles', () => {
    expect(toggle(render({ kind: 'literal', text: '1' }).fixture).getAttribute('title')).toBe('Use a parameter instead of a number');
    expect(toggle(render({ kind: 'literal', text: '1' }, { paramNames: [] }).fixture).getAttribute('title')).toBe(
      'Declare a numeric parameter first',
    );
    expect(toggle(render({ kind: 'param', name: 'layers' }).fixture).getAttribute('title')).toBe('Use a literal number instead');
  });
});
