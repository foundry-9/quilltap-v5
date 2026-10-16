import { Component, signal } from '@angular/core';
import { TestBed } from '@angular/core/testing';
import { describe, expect, it } from 'vitest';

import {
  type ComponentMode,
  WardrobeComponentModeFieldset,
  defaultComponentMode,
} from './wardrobe-component-mode-fieldset';

/**
 * P4.D309 — v4 `components/wardrobe/WardrobeComponentModeFieldset.tsx`
 * (`bbbcd2388`): the shared radio group the transfer dialog and the folder
 * picker both mount. Its labels per mode, the starting choice, both footnotes
 * verbatim, the `name` grouping and `disabled`.
 */

@Component({
  imports: [WardrobeComponentModeFieldset],
  template: `
    <qt-wardrobe-component-mode-fieldset
      [mode]="mode()"
      [value]="value()"
      (valueChange)="value.set($event)"
      legend="This outfit bundles 2 components"
      [destination]="destination()"
      name="probe-components"
      [disabled]="disabled()"
    />
  `,
})
class Host {
  readonly mode = signal<'move' | 'copy'>('move');
  readonly value = signal<ComponentMode>('move');
  readonly destination = signal<'wardrobe' | 'folder'>('folder');
  readonly disabled = signal(false);
}

function render(
  init: Partial<{
    mode: 'move' | 'copy';
    value: ComponentMode;
    destination: 'wardrobe' | 'folder';
    disabled: boolean;
  }> = {},
) {
  TestBed.resetTestingModule();
  TestBed.configureTestingModule({ imports: [Host] });
  const fixture = TestBed.createComponent(Host);
  const host = fixture.componentInstance;
  if (init.mode) host.mode.set(init.mode);
  if (init.value) host.value.set(init.value);
  if (init.destination) host.destination.set(init.destination);
  if (init.disabled !== undefined) host.disabled.set(init.disabled);
  fixture.detectChanges();
  const el = fixture.nativeElement as HTMLElement;
  const radios = () => Array.from(el.querySelectorAll<HTMLInputElement>('input[type="radio"]'));
  return { fixture, host, el, radios };
}

describe('WardrobeComponentModeFieldset (v4 `WardrobeComponentModeFieldset.tsx`)', () => {
  it('defaultComponentMode: a move moves the components, a copy copies them', () => {
    expect(defaultComponentMode('move')).toBe('move');
    expect(defaultComponentMode('copy')).toBe('copy');
  });

  it('a move offers move / copy / leave', () => {
    const { el, radios } = render();
    expect(el.querySelector('legend')?.textContent?.trim()).toBe(
      'This outfit bundles 2 components',
    );
    expect(radios().map((r) => r.parentElement?.textContent?.trim())).toEqual([
      'Move the components along with it',
      'Copy the components (originals stay behind)',
      'Leave the components behind',
    ]);
    expect(radios().map((r) => r.checked)).toEqual([true, false, false]);
  });

  it('a copy offers copy / alone (it cannot move the pieces the original still wears)', () => {
    const { radios } = render({ mode: 'copy', value: 'copy' });
    expect(radios().map((r) => r.parentElement?.textContent?.trim())).toEqual([
      'Copy the components along with it',
      'Copy the outfit alone',
    ]);
    expect(radios().map((r) => r.checked)).toEqual([true, false]);
  });

  it('both footnotes, verbatim', () => {
    const folder = render({ destination: 'folder' });
    expect(folder.el.querySelector('p')?.textContent).toBe(
      'All or nothing — the choice covers every component, nested pieces included. Only components filed in this wardrobe travel; pieces borrowed from a shared tier stay put. Copied components take a "(copy)" title, and the outfit is rewired to point at them.',
    );
    const wardrobe = render({ destination: 'wardrobe' });
    expect(wardrobe.el.querySelector('p')?.textContent).toBe(
      'All or nothing — the choice covers every component, nested pieces included. Only components living in the same wardrobe travel; pieces from a shared tier stay put. The transferred outfit is rewired to point at whichever components arrive with it.',
    );
  });

  it('the radios share the given name and report the picked value', () => {
    const { fixture, host, radios } = render();
    expect(radios().every((r) => r.name === 'probe-components')).toBe(true);
    expect(radios().every((r) => r.classList.contains('qt-radio'))).toBe(true);
    radios()[2].click();
    fixture.detectChanges();
    expect(host.value()).toBe('none');
    expect(radios().map((r) => r.checked)).toEqual([false, false, true]);
  });

  it('disabled disables every radio', () => {
    const { radios } = render({ disabled: true });
    expect(radios().every((r) => r.disabled)).toBe(true);
  });
});
