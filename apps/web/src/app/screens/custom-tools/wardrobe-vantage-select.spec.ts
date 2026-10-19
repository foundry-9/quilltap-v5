import { Component, signal } from '@angular/core';
import { ComponentFixture, TestBed } from '@angular/core/testing';
import { afterEach, describe, expect, it } from 'vitest';

import type { CustomToolDestinations } from '../../core/core-contract';
import type { WardrobeVantage } from '../../pascal/tool-draft';
import {
  WardrobeVantageSelect,
  decodeWardrobeVantage,
  encodeWardrobeVantage,
  wardrobeVantageName,
  wardrobeVantageOrigin,
} from './wardrobe-vantage-select';

/**
 * The Workbench's whose-wardrobe select (v4 `5cb0e0190`
 * `components/custom-tools/WardrobeVantageSelect.tsx`). v4 ships no test for
 * it; these pin the encoding, the four names, and every label / hint byte
 * against `git show e8e17b2d9:components/custom-tools/WardrobeVantageSelect.tsx:119-174`
 * (JSX `{' '}` joins → single spaces, `&rsquo;` → U+2019).
 */

const store = (mountPointId: string) => ({ mountPointId, mountName: mountPointId, existingToolNames: [] });
const DESTINATIONS: CustomToolDestinations = {
  general: store('mp-general'),
  characters: [{ characterId: 'char-imogen', characterName: 'Imogen', ...store('mp-imogen') }],
  groups: [{ groupId: 'g-1', groupName: 'The Regulars', stores: [{ ...store('mp-g'), official: true }] }],
  projects: [{ projectId: 'p-1', projectName: 'Thornfield', stores: [store('mp-p')] }],
  other: [],
};

describe('encodeWardrobeVantage / decodeWardrobeVantage', () => {
  it('round-trips every vantage', () => {
    for (const v of [
      { kind: 'character', characterId: 'char-imogen' },
      { kind: 'container', container: { scope: 'general', id: null } },
      { kind: 'container', container: { scope: 'project', id: 'p-1' } },
      { kind: 'container', container: { scope: 'group', id: 'g-1' } },
      { kind: 'none' },
    ] as WardrobeVantage[]) {
      expect(decodeWardrobeVantage(encodeWardrobeVantage(v))).toEqual(v);
    }
    expect(encodeWardrobeVantage({ kind: 'character', characterId: 'x' })).toBe('char:x');
    expect(encodeWardrobeVantage({ kind: 'none' })).toBe('none');
  });

  it('reads an empty character id or a mangled value as none', () => {
    expect(decodeWardrobeVantage('char:')).toEqual({ kind: 'none' });
    expect(decodeWardrobeVantage('wardrobe:x')).toEqual({ kind: 'none' });
    expect(decodeWardrobeVantage('project:')).toEqual({ kind: 'none' });
  });
});

describe('wardrobeVantageOrigin / wardrobeVantageName', () => {
  it('names each vantage, with v4 fallbacks for an id the destinations do not list', () => {
    expect(wardrobeVantageOrigin({ kind: 'none' }, DESTINATIONS)).toBeNull();
    expect(
      wardrobeVantageOrigin({ kind: 'character', characterId: 'char-imogen' }, DESTINATIONS),
    ).toEqual({ scope: 'character', id: 'char-imogen', name: 'Imogen' });
    expect(wardrobeVantageOrigin({ kind: 'character', characterId: 'gone' }, DESTINATIONS)).toEqual({
      scope: 'character',
      id: 'gone',
      name: '',
    });
    expect(
      wardrobeVantageOrigin({ kind: 'container', container: { scope: 'general', id: null } }, undefined),
    ).toEqual({ scope: 'general', id: null, name: 'Quilltap General' });
    expect(
      wardrobeVantageName({ kind: 'container', container: { scope: 'project', id: 'zz' } }, DESTINATIONS),
    ).toBe('this project');
    expect(
      wardrobeVantageName({ kind: 'container', container: { scope: 'group', id: 'zz' } }, DESTINATIONS),
    ).toBe('this group');
    expect(
      wardrobeVantageOrigin({ kind: 'container', container: { scope: 'character', id: 'c' } }, DESTINATIONS),
    ).toEqual({ scope: 'character', id: 'c', name: '' });
    expect(wardrobeVantageName({ kind: 'character', characterId: 'gone' }, DESTINATIONS)).toBe(
      'this character',
    );
    expect(wardrobeVantageName({ kind: 'none' }, DESTINATIONS)).toBeNull();
  });
});

@Component({
  imports: [WardrobeVantageSelect],
  template: `<qt-wardrobe-vantage-select
    [vantage]="vantage()"
    [destinations]="destinations"
    [homeProjectName]="home()"
    (vantageChange)="changes.push($event)"
  />`,
})
class Host {
  readonly vantage = signal<WardrobeVantage>({ kind: 'none' });
  readonly home = signal<string | null>(null);
  readonly destinations = DESTINATIONS;
  readonly changes: WardrobeVantage[] = [];
}

describe('WardrobeVantageSelect', () => {
  afterEach(() => TestBed.resetTestingModule());

  function render(vantage: WardrobeVantage, home: string | null = null): ComponentFixture<Host> {
    TestBed.configureTestingModule({ imports: [Host] });
    const fixture = TestBed.createComponent(Host);
    fixture.componentInstance.vantage.set(vantage);
    fixture.componentInstance.home.set(home);
    fixture.detectChanges();
    return fixture;
  }
  const el = (f: ComponentFixture<Host>) => f.nativeElement as HTMLElement;
  const label = (f: ComponentFixture<Host>) => el(f).querySelector('label > span')!.textContent;
  const hint = (f: ComponentFixture<Host>) => el(f).querySelector('p')!.textContent;

  it('labels and explains a character vantage', () => {
    const f = render({ kind: 'character', characterId: 'char-imogen' });
    expect(label(f)).toBe('Browsing Imogen’s wardrobe');
    expect(el(f).querySelector('strong')!.textContent).toBe('Imogen’s');
    expect(hint(f)).toBe(
      'What Imogen can reach: Imogen’s own wardrobe, the wardrobes of Imogen’s groups, and Quilltap General. Another character rolling this tool sees their own.',
    );
    expect(el(f).querySelector('p')!.className).toBe('text-xs qt-text-secondary');
  });

  it('adds the home project to a character vantage', () => {
    const f = render({ kind: 'character', characterId: 'char-imogen' }, 'Thornfield');
    expect(hint(f)).toBe(
      'What Imogen can reach: Imogen’s own wardrobe, the wardrobes of Imogen’s groups, and Quilltap General. Another character rolling this tool sees their own. This tool lives in Thornfield’s store; garments from Thornfield’s wardrobe will also be in reach when it runs there.',
    );
  });

  it('labels and explains a shared wardrobe', () => {
    const f = render({ kind: 'container', container: { scope: 'group', id: 'g-1' } });
    expect(label(f)).toBe('Browsing The Regulars wardrobe');
    expect(hint(f)).toBe(
      'Every garment in this wardrobe, and who can reach it: members of The Regulars.',
    );
  });

  it('asks for a choice under the none vantage', () => {
    const f = render({ kind: 'none' });
    expect(label(f)).toBe('Browsing  wardrobe');
    expect(el(f).querySelector('strong')).toBeNull();
    expect(hint(f)).toBe('Choose whose wardrobe to browse before picking garments.');
  });

  it('offers nobody, the characters, then General, projects and groups, in that order', () => {
    const f = render({ kind: 'none' });
    const select = el(f).querySelector<HTMLSelectElement>('select[aria-label="Whose wardrobe to browse"]')!;
    expect([...select.querySelectorAll('optgroup')].map((g) => g.label)).toEqual([
      'Characters',
      'Shared wardrobes',
    ]);
    expect([...select.options].map((o) => [o.value, o.textContent])).toEqual([
      ['none', '— choose —'],
      ['char:char-imogen', 'Imogen'],
      ['general:', 'Quilltap General'],
      ['project:p-1', 'Project · Thornfield'],
      ['group:g-1', 'Group · The Regulars'],
    ]);
    expect(select.value).toBe('none');
  });

  it('emits the decoded vantage on a change', () => {
    const f = render({ kind: 'none' });
    const select = el(f).querySelector<HTMLSelectElement>('select')!;
    select.value = 'project:p-1';
    select.dispatchEvent(new Event('change'));
    expect(f.componentInstance.changes).toEqual([
      { kind: 'container', container: { scope: 'project', id: 'p-1' } },
    ]);
  });
});
