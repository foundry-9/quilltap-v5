import { ComponentFixture, TestBed } from '@angular/core/testing';
import { afterEach, beforeEach, describe, expect, it } from 'vitest';

import emojiDataset from '../../../../public/emoji/emoji-index.v1.json';
import unicodeDataset from '../../../../public/unicode/unicode-index.v1.json';
import '../jsdom-range-shim';
import { CharPickerPanel } from './char-picker-panel';
import { stubFetch, type FetchStub } from './char-typeahead-harness';
import { EMOJI_PROFILE } from './profiles/emoji';
import { UNICODE_PROFILE } from './profiles/unicode';
import type { CharProfile } from './types';

/**
 * The shared picker panel (v4 `CharPickerPanel.tsx`): the search field over the
 * same Tier B engine, the Recents row, the grouped grid, the lazy fetch and its
 * three states, and the keyboard path through the grid.
 *
 * @module editor/char-insert/char-picker-panel.spec
 */

async function settle(fixture: ComponentFixture<unknown>): Promise<void> {
  for (let i = 0; i < 4; i++) {
    await new Promise((r) => setTimeout(r, 0));
    fixture.detectChanges();
  }
}

describe('CharPickerPanel', () => {
  let fixture: ComponentFixture<CharPickerPanel>;
  let fetchStub: FetchStub;
  const picks: string[] = [];
  let closes = 0;

  beforeEach(() => {
    localStorage.clear();
    picks.length = 0;
    closes = 0;
    EMOJI_PROFILE.loader.resetForTests();
    UNICODE_PROFILE.loader.resetForTests();
  });

  afterEach(() => {
    fetchStub?.restore();
    TestBed.resetTestingModule();
  });

  async function open(
    profile: CharProfile,
    { datasetFails = false } = {},
  ): Promise<ComponentFixture<CharPickerPanel>> {
    const [fragment, payload] =
      profile === EMOJI_PROFILE
        ? ['emoji-index.v1.json', emojiDataset]
        : ['unicode-index.v1.json', unicodeDataset];
    fetchStub = stubFetch(fragment, payload, { datasetFails });

    TestBed.configureTestingModule({ imports: [CharPickerPanel] });
    fixture = TestBed.createComponent(CharPickerPanel);
    fixture.componentRef.setInput('profile', profile);
    fixture.componentInstance.pick.subscribe((char) => picks.push(char));
    fixture.componentInstance.close.subscribe(() => (closes += 1));
    fixture.detectChanges();
    await settle(fixture);
    return fixture;
  }

  function root(): HTMLElement {
    return fixture.nativeElement as HTMLElement;
  }

  function el<T extends HTMLElement>(selector: string): T | null {
    return root().querySelector(selector) as T | null;
  }

  function cells(): HTMLButtonElement[] {
    return Array.from(root().querySelectorAll<HTMLButtonElement>('.qt-emoji-picker-cell'));
  }

  function headers(): string[] {
    return Array.from(
      root().querySelectorAll<HTMLElement>('.qt-emoji-picker-group-header'),
    ).map((node) => node.textContent!.trim());
  }

  function type(value: string): void {
    const search = el<HTMLInputElement>('.qt-emoji-picker-search')!;
    search.value = value;
    search.dispatchEvent(new Event('input'));
    fixture.detectChanges();
  }

  it('fetches the dataset on open and groups the grid by the dataset`s own categories', async () => {
    await open(EMOJI_PROFILE);

    expect(fetchStub.datasetFetches()).toBe(1);
    // v4's `groupLabels`, in the dataset's group order.
    expect(headers().slice(0, 3)).toEqual(['Smileys & Emotion', 'People & Body', 'Animals & Nature']);
    expect(cells().length).toBeGreaterThan(1_800);
  });

  it('labels every cell with the dataset`s name, for screen readers', async () => {
    await open(EMOJI_PROFILE);

    const first = cells()[0];
    expect(first.getAttribute('aria-label')).toBe('grinning face');
    expect(first.getAttribute('title')).toBe('grinning face');
    expect(first.textContent!.trim()).toBe('😀');
  });

  it('searches through the same engine and shows one Results section', async () => {
    await open(EMOJI_PROFILE);

    type('tada');

    expect(headers()).toEqual(['Results']);
    expect(cells()[0].textContent!.trim()).toBe('🎉');
  });

  it('resolves a code point in the search field (unicode only)', async () => {
    await open(UNICODE_PROFILE);

    type('u+2192');

    expect(headers()).toEqual(['Results']);
    expect(cells()[0].getAttribute('aria-label')).toBe('rightwards arrow');
  });

  it('shows the profile`s empty label when nothing matches', async () => {
    await open(EMOJI_PROFILE);

    type('definitelynotanemoji');

    expect(el('.qt-emoji-picker-message')!.textContent!.trim()).toBe('No emoji found');
  });

  it('shows the profile`s failure label when the dataset cannot be fetched', async () => {
    await open(EMOJI_PROFILE, { datasetFails: true });

    expect(el('.qt-emoji-picker-message')!.textContent!.trim()).toBe("Couldn't load emoji");
    expect(cells()).toHaveLength(0);
  });

  it('leads with a Recently used row, from this profile`s list only', async () => {
    localStorage.setItem(EMOJI_PROFILE.recentsStorageKey, JSON.stringify(['🎉']));
    localStorage.setItem(UNICODE_PROFILE.recentsStorageKey, JSON.stringify(['→']));

    await open(EMOJI_PROFILE);

    expect(headers()[0]).toBe('Recently used');
    expect(cells()[0].textContent!.trim()).toBe('🎉');
  });

  it('emits the pick and closes on a click, keeping the editor`s selection', async () => {
    await open(EMOJI_PROFILE);
    type('tada');

    const cell = cells()[0];
    const mousedown = new MouseEvent('mousedown', { bubbles: true, cancelable: true });
    cell.dispatchEvent(mousedown);
    cell.click();

    // mousedown is prevented so the host editor keeps its caret (v4).
    expect(mousedown.defaultPrevented).toBe(true);
    expect(picks).toEqual(['🎉']);
    expect(closes).toBe(1);
  });

  it('commits the first result on Enter in the search field', async () => {
    await open(EMOJI_PROFILE);
    type('tada');

    const search = el<HTMLInputElement>('.qt-emoji-picker-search')!;
    search.dispatchEvent(new KeyboardEvent('keydown', { key: 'Enter', bubbles: true }));

    expect(picks).toEqual(['🎉']);
  });

  it('walks the grid with the arrows and steps back up to the search field', async () => {
    await open(EMOJI_PROFILE);
    type('face');
    fixture.detectChanges();

    const search = el<HTMLInputElement>('.qt-emoji-picker-search')!;
    search.dispatchEvent(new KeyboardEvent('keydown', { key: 'ArrowDown', bubbles: true }));
    fixture.detectChanges();
    expect(document.activeElement).toBe(cells()[0]);

    cells()[0].dispatchEvent(new KeyboardEvent('keydown', { key: 'ArrowRight', bubbles: true }));
    fixture.detectChanges();
    expect(document.activeElement).toBe(cells()[1]);

    // 8 columns: ArrowDown from cell 1 lands on cell 9.
    cells()[1].dispatchEvent(new KeyboardEvent('keydown', { key: 'ArrowDown', bubbles: true }));
    fixture.detectChanges();
    expect(document.activeElement).toBe(cells()[9]);

    // Back up off the top row returns to the search field rather than trapping
    // the keyboard in the grid.
    cells()[1].dispatchEvent(new KeyboardEvent('keydown', { key: 'ArrowUp', bubbles: true }));
    fixture.detectChanges();
    expect(document.activeElement).toBe(search);
  });

  it('carries ONE tab stop for the whole grid (roving tabindex)', async () => {
    await open(EMOJI_PROFILE);
    type('face');

    const tabbable = cells().filter((cell) => cell.tabIndex === 0);
    expect(tabbable).toHaveLength(1);
    expect(tabbable[0]).toBe(cells()[0]);
  });

  it('closes on a mousedown outside itself', async () => {
    await open(EMOJI_PROFILE);

    document.body.dispatchEvent(new MouseEvent('mousedown', { bubbles: true }));

    expect(closes).toBe(1);
  });
});
