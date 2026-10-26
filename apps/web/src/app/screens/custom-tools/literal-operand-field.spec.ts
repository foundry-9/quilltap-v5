import { ComponentFixture, TestBed } from '@angular/core/testing';
import { afterEach, describe, expect, it } from 'vitest';

import type { LiteralOperand } from '../../pascal/tool-draft';
import { LiteralOperandField, literalOfKind } from './literal-operand-field';

/**
 * LiteralOperandField — v4 `LiteralOperandField.tsx`: the literal right-hand
 * side both the gate and the outcome table render (extracted from v5's two
 * inlined copies — P4.D334 R-4).
 */

function render(
  operand: LiteralOperand,
  extra: Record<string, unknown> = {},
): { fixture: ComponentFixture<LiteralOperandField>; emitted: LiteralOperand[] } {
  TestBed.resetTestingModule();
  TestBed.configureTestingModule({ imports: [LiteralOperandField] });
  const fixture = TestBed.createComponent(LiteralOperandField);
  fixture.componentRef.setInput('operand', operand);
  for (const [k, v] of Object.entries(extra)) fixture.componentRef.setInput(k, v);
  const emitted: LiteralOperand[] = [];
  fixture.componentInstance.operandChange.subscribe((o) => emitted.push(o));
  fixture.detectChanges();
  return { fixture, emitted };
}

const el = (f: ComponentFixture<unknown>) => f.nativeElement as HTMLElement;

describe('LiteralOperandField (v4 LiteralOperandField.tsx)', () => {
  afterEach(() => TestBed.resetTestingModule());

  it('renders the three literal kinds with the markup the two chips used', () => {
    const num = render({ kind: 'number', text: '3' }).fixture;
    const input = el(num).querySelector<HTMLInputElement>('input[aria-label="Operand number"]')!;
    expect(input.getAttribute('class')).toBe('qt-input w-24 text-sm');
    expect(input.getAttribute('type')).toBe('number');
    expect(input.value).toBe('3');

    const str = render({ kind: 'string', text: 'hi' }).fixture;
    const text = el(str).querySelector<HTMLInputElement>('input[aria-label="Operand text"]')!;
    expect(text.getAttribute('class')).toBe('qt-input w-28 text-sm');
    expect(text.value).toBe('hi');

    const bool = render({ kind: 'boolean', value: false }).fixture;
    const select = el(bool).querySelector<HTMLSelectElement>('select[aria-label="Operand value"]')!;
    expect(select.getAttribute('class')).toBe('qt-select qt-select-sm w-20');
    expect(select.value).toBe('false');
  });

  it('sizes the text input by textClassName and the picker by pickerClassName', () => {
    const { fixture } = render(
      { kind: 'string', text: '' },
      { showTypePicker: true, pickerClassName: 'w-24 shrink-0', textClassName: 'w-32' },
    );
    expect(el(fixture).querySelector('input[aria-label="Operand text"]')!.getAttribute('class')).toBe('qt-input w-32 text-sm');
    expect(el(fixture).querySelector('select[aria-label="Literal type"]')!.getAttribute('class')).toBe(
      'qt-select qt-select-sm w-24 shrink-0',
    );
  });

  it('shows the picker only when asked, and a choice keeps typed text between number and text', () => {
    expect(el(render({ kind: 'number', text: '1' }).fixture).querySelector('select[aria-label="Literal type"]')).toBeNull();
    const { fixture, emitted } = render({ kind: 'number', text: '42' }, { showTypePicker: true });
    const picker = el(fixture).querySelector<HTMLSelectElement>('select[aria-label="Literal type"]')!;
    expect([...picker.options].map((o) => o.textContent?.trim())).toEqual(['number', 'text', 'true/false']);
    picker.value = 'string';
    picker.dispatchEvent(new Event('change'));
    picker.value = 'boolean';
    picker.dispatchEvent(new Event('change'));
    expect(emitted).toEqual([
      { kind: 'string', text: '42' },
      { kind: 'boolean', value: true },
    ]);
  });

  it('literalOfKind: a boolean starts at true and the other kinds carry text across', () => {
    expect(literalOfKind('number', { kind: 'string', text: '7' })).toEqual({ kind: 'number', text: '7' });
    expect(literalOfKind('number', { kind: 'boolean', value: true })).toEqual({ kind: 'number', text: '' });
    expect(literalOfKind('string', { kind: 'number', text: '7' })).toEqual({ kind: 'string', text: '7' });
    expect(literalOfKind('boolean', { kind: 'string', text: 'x' })).toEqual({ kind: 'boolean', value: true });
  });

  it('typing emits the operand of the kind on screen', () => {
    const { fixture, emitted } = render({ kind: 'string', text: '' });
    const input = el(fixture).querySelector<HTMLInputElement>('input[aria-label="Operand text"]')!;
    input.value = 'abc';
    input.dispatchEvent(new Event('input'));
    expect(emitted).toEqual([{ kind: 'string', text: 'abc' }]);
  });

  describe('stateOptions', () => {
    const STATES = ['equipped', 'carried', 'reachable', 'absent'];

    it('renders only a select of the states, with no picker and no leading dash while the text is a state', () => {
      const { fixture } = render({ kind: 'string', text: 'carried' }, { stateOptions: STATES, showTypePicker: true });
      expect(el(fixture).querySelectorAll('select')).toHaveLength(1);
      const select = el(fixture).querySelector<HTMLSelectElement>('select[aria-label="Inventory state"]')!;
      expect(select.getAttribute('class')).toBe('qt-select qt-select-sm w-28');
      expect([...select.options].map((o) => o.value)).toEqual(STATES);
      expect(select.value).toBe('carried');
    });

    it('offers a — option (value empty) only while the text is not a state', () => {
      const { fixture } = render({ kind: 'string', text: 'worn' }, { stateOptions: STATES });
      const select = el(fixture).querySelector<HTMLSelectElement>('select[aria-label="Inventory state"]')!;
      expect(select.options[0].value).toBe('');
      expect(select.options[0].textContent?.trim()).toBe('—');
      expect(select.options).toHaveLength(5);
    });

    it('treats a non-string operand as the empty text', () => {
      const { fixture } = render({ kind: 'number', text: '1' }, { stateOptions: STATES });
      expect(el(fixture).querySelector<HTMLSelectElement>('select[aria-label="Inventory state"]')!.options[0].value).toBe('');
    });

    it('emits { kind: string, text: value }', () => {
      const { fixture, emitted } = render({ kind: 'string', text: 'equipped' }, { stateOptions: STATES });
      const select = el(fixture).querySelector<HTMLSelectElement>('select[aria-label="Inventory state"]')!;
      select.value = 'absent';
      select.dispatchEvent(new Event('change'));
      expect(emitted).toEqual([{ kind: 'string', text: 'absent' }]);
    });
  });
});
