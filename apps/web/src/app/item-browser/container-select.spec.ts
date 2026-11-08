import { Component, signal } from '@angular/core';
import { TestBed } from '@angular/core/testing';
import { afterEach, describe, expect, it } from 'vitest';

import type { CatalogueEntry } from './container-catalogue';
import { ContainerSelect } from './container-select';

/**
 * v4 `components/item-browser/ContainerSelect.tsx` at the pin `624d0d25c`
 * (NEW at `b2b783b47`). v4 has no jest file for it (its transfer-dialog tests
 * drive it); these rows pin the optgroup order, `exclude`, the dropped empty
 * group and the placeholder only while nothing is chosen.
 */

@Component({
  imports: [ContainerSelect],
  template: `
    <qt-container-select
      id="xfer"
      [value]="value()"
      (valueChange)="value.set($event); changes.push($event)"
      [catalogue]="catalogue()"
      generalName="Quilltap General"
      placeholder="Select a wardrobe"
      [exclude]="exclude()"
      [disabled]="disabled()"
      [labelledBy]="labelledBy()"
    />
  `,
})
class Host {
  readonly value = signal('');
  readonly catalogue = signal<{ characters: CatalogueEntry[]; projects: CatalogueEntry[]; groups: CatalogueEntry[] }>({
    characters: [
      { id: 'c1', name: 'Ada' },
      { id: 'c2', name: 'Bea' },
    ],
    projects: [{ id: 'p1', name: 'Opus' }],
    groups: [],
  });
  readonly exclude = signal<readonly string[]>([]);
  readonly disabled = signal(false);
  readonly labelledBy = signal<string | undefined>(undefined);
  readonly changes: string[] = [];
}

function render(setup: (h: Host) => void = () => {}) {
  TestBed.configureTestingModule({ imports: [Host] });
  const fixture = TestBed.createComponent(Host);
  setup(fixture.componentInstance);
  fixture.detectChanges();
  const el = fixture.nativeElement as HTMLElement;
  const select = () => el.querySelector('select') as HTMLSelectElement;
  const shape = () =>
    [...select().children].map((c) =>
      c.tagName === 'OPTGROUP'
        ? [(c as HTMLOptGroupElement).label, [...c.children].map((o) => `${(o as HTMLOptionElement).value}=${o.textContent!.trim()}`)]
        : ['(option)', `${(c as HTMLOptionElement).value}=${c.textContent!.trim()}`, (c as HTMLOptionElement).disabled],
    );
  return { fixture, el, host: fixture.componentInstance, select, shape };
}

describe('ContainerSelect', () => {
  afterEach(() => TestBed.resetTestingModule());

  it('Characters, General, Projects, Groups — each an optgroup of encoded keys; an empty group is dropped', () => {
    const { select, shape } = render();
    expect(select().id).toBe('xfer');
    expect(select().getAttribute('class')).toBe('qt-select w-full');
    expect(shape()).toEqual([
      ['(option)', '=Select a wardrobe', true],
      ['Characters', ['character:c1=Ada', 'character:c2=Bea']],
      ['General', ['general:=Quilltap General']],
      ['Projects', ['project:p1=Opus']],
    ]);
  });

  it('the placeholder shows only while nothing is chosen', () => {
    const { fixture, host, select, shape } = render((h) => h.value.set('project:p1'));
    expect(shape()[0][0]).toBe('Characters');
    expect(select().value).toBe('project:p1');
    host.value.set('');
    fixture.detectChanges();
    expect(shape()[0]).toEqual(['(option)', '=Select a wardrobe', true]);
  });

  it('exclude drops keys — General included — and a group left empty goes', () => {
    const { shape } = render((h) => h.exclude.set(['general:', 'project:p1', 'character:c2']));
    expect(shape()).toEqual([
      ['(option)', '=Select a wardrobe', true],
      ['Characters', ['character:c1=Ada']],
    ]);
  });

  it('emits the encoded key; disabled disables', () => {
    const { fixture, host, select } = render();
    select().value = 'general:';
    select().dispatchEvent(new Event('change'));
    expect(host.changes).toEqual(['general:']);
    host.disabled.set(true);
    fixture.detectChanges();
    expect(select().disabled).toBe(true);
  });

  it('names itself by the caller’s label id only when given one (no aria-labelledby otherwise — v4’s DOM)', () => {
    const { fixture, host, select } = render();
    expect(select().hasAttribute('aria-labelledby')).toBe(false);
    host.labelledBy.set('xfer-label');
    fixture.detectChanges();
    expect(select().getAttribute('aria-labelledby')).toBe('xfer-label');
  });

  it('the id lands on the select only — never on the host', () => {
    const { el } = render();
    expect(el.querySelector('qt-container-select')!.hasAttribute('id')).toBe(false);
    expect(el.querySelectorAll('#xfer')).toHaveLength(1);
  });
});
