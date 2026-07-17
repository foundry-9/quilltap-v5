import { ComponentFixture, TestBed } from '@angular/core/testing';
import { afterEach, describe, expect, it } from 'vitest';

import type { CustomToolParameterSpec } from '../core/core-contract';
import {
  CustomToolParamsForm,
  coerceParamValues,
  initialParamValues,
  type ParameterFormValues,
} from './custom-tool-params-form';

const PARAMS: Record<string, CustomToolParameterSpec> = {
  scale: { type: 'integer', default: 3, min: 1, max: 6, description: 'how hard' },
  weight: { type: 'number', default: 1.5 },
  label: { type: 'string', default: 'lock' },
  loud: { type: 'boolean', default: true },
};

describe('custom-tool-params-form — coercion (v4 CustomToolParamsForm)', () => {
  it('seeds the form from the declared defaults, booleans as booleans', () => {
    expect(initialParamValues(PARAMS)).toEqual({
      scale: '3',
      weight: '1.5',
      label: 'lock',
      loud: true,
    });
  });

  it('coerces text values back to the declared types', () => {
    const values: ParameterFormValues = { scale: '5', weight: '2.25', label: 'vault', loud: false };
    expect(coerceParamValues(PARAMS, values)).toEqual({
      scale: 5,
      weight: 2.25,
      label: 'vault',
      loud: false,
    });
  });

  it('falls back to the declared default for a blank or unparseable number (never NaN)', () => {
    const values: ParameterFormValues = { scale: '', weight: 'abc', label: '', loud: true };
    const out = coerceParamValues(PARAMS, values);
    expect(out['scale']).toBe(3); // blank integer → default
    expect(out['weight']).toBe(1.5); // unparseable float → default
    expect(out['label']).toBe(''); // an empty string stays an empty string
    expect(out['loud']).toBe(true);
  });

  it('parseInt truncates a float typed into an integer field', () => {
    expect(coerceParamValues(PARAMS, { ...initialParamValues(PARAMS), scale: '4.9' })['scale']).toBe(4);
  });
});

describe('CustomToolParamsForm — rendering', () => {
  afterEach(() => TestBed.resetTestingModule());

  function render(): ComponentFixture<CustomToolParamsForm> {
    TestBed.configureTestingModule({ imports: [CustomToolParamsForm] });
    const fixture = TestBed.createComponent(CustomToolParamsForm);
    fixture.componentRef.setInput('parameters', PARAMS);
    fixture.componentRef.setInput('values', initialParamValues(PARAMS));
    fixture.componentRef.setInput('idPrefix', 'pfx');
    fixture.detectChanges();
    return fixture;
  }

  it('renders a checkbox for a boolean and number/text inputs otherwise, with unique ids', () => {
    const el = render().nativeElement as HTMLElement;
    const scale = el.querySelector('#pfx-scale') as HTMLInputElement;
    const label = el.querySelector('#pfx-label') as HTMLInputElement;
    const loud = el.querySelector('#pfx-loud') as HTMLInputElement;
    expect(scale.type).toBe('number');
    expect(scale.getAttribute('min')).toBe('1');
    expect(scale.getAttribute('max')).toBe('6');
    expect(label.type).toBe('text');
    expect(label.hasAttribute('min')).toBe(false);
    expect(loud.type).toBe('checkbox');
    expect(loud.checked).toBe(true);
  });

  it('emits a ParamChange when a field changes', () => {
    const fixture = render();
    const seen: { param: string; value: string | boolean }[] = [];
    fixture.componentInstance.change.subscribe((c) => seen.push(c));
    const label = fixture.nativeElement.querySelector('#pfx-label') as HTMLInputElement;
    label.value = 'vault';
    label.dispatchEvent(new Event('input'));
    expect(seen).toEqual([{ param: 'label', value: 'vault' }]);
  });
});
