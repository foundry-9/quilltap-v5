import { ComponentFixture, TestBed } from '@angular/core/testing';
import { afterEach, describe, expect, it, vi } from 'vitest';

import { CoreClient } from '../../core/core-client';
import type { CoreRequest } from '../../core/core-contract';
import type { WardrobeContainer } from '../wardrobe-container';
import { WardrobeFolderField } from './wardrobe-folder-field';

/**
 * P4.D286 — v4 `components/wardrobe/wardrobe-item-editor/WardrobeFolderField.tsx`
 * (NEW in `4a38222e6`, read at that pin). No v4 jsdom test names the
 * component; these pin its strings and rules: the top level + every folder of
 * the DESTINATION (crumbs ` › `) + **New folder…**; a value the tree doesn't
 * list still offered and matched case-insensitively; New folder… typing /
 * Escape (stopped) / Pick; the help sentence; and an absent or failed read
 * leaving the top level and New folder… alone.
 */
type AnyRequest = CoreRequest & Record<string, unknown>;

const FOLDERS = [
  { path: 'Hats', name: 'Hats', itemCount: 2 },
  { path: 'Formal/Evening', name: 'Evening', itemCount: 1 },
  { path: 'Formal', name: 'Formal', itemCount: 0 },
];

interface Handle {
  fixture: ComponentFixture<WardrobeFolderField>;
  el: HTMLElement;
  seen: AnyRequest[];
  emitted: string[];
}

async function settle(fixture: ComponentFixture<unknown>): Promise<void> {
  for (let i = 0; i < 4; i++) {
    await Promise.resolve();
    fixture.detectChanges();
  }
}

async function render(
  container: WardrobeContainer | null,
  value: string,
  answer: (req: AnyRequest) => Record<string, unknown> | Error = () => ({ folders: FOLDERS }),
): Promise<Handle> {
  const seen: AnyRequest[] = [];
  const core = {
    dispatchData: vi.fn(async (req: CoreRequest) => {
      seen.push(req as AnyRequest);
      const out = answer(req as AnyRequest);
      if (out instanceof Error) throw out;
      return out;
    }),
  } as unknown as CoreClient;
  TestBed.resetTestingModule();
  TestBed.configureTestingModule({
    imports: [WardrobeFolderField],
    providers: [{ provide: CoreClient, useValue: core }],
  });
  const fixture = TestBed.createComponent(WardrobeFolderField);
  fixture.componentRef.setInput('container', container);
  fixture.componentRef.setInput('value', value);
  const emitted: string[] = [];
  fixture.componentInstance.valueChange.subscribe((v) => emitted.push(v));
  fixture.detectChanges();
  await settle(fixture);
  return { fixture, el: fixture.nativeElement as HTMLElement, seen, emitted };
}

const CHARACTER: WardrobeContainer = { scope: 'character', id: 'char-1' };

function optionTexts(el: HTMLElement): string[] {
  return [...el.querySelectorAll('select option')].map((o) => o.textContent!.trim());
}

describe('WardrobeFolderField (v4 4a38222e6)', () => {
  afterEach(() => vi.restoreAllMocks());

  it('offers the top level, the destination’s folders as crumbs, and New folder…', async () => {
    const { el, seen } = await render(CHARACTER, '');
    expect(seen).toEqual([
      { type: 'wardrobeFoldersList', scope: 'character', containerId: 'char-1' },
    ]);
    expect(el.querySelector('label')!.textContent!.trim()).toBe('Folder');
    expect(el.querySelector('label')!.getAttribute('for')).toBe('wardrobe-folder');
    expect(el.querySelector('select')!.id).toBe('wardrobe-folder');
    expect(optionTexts(el)).toEqual([
      'Wardrobe (top level)',
      'Formal',
      'Formal › Evening',
      'Hats',
      'New folder…',
    ]);
    expect(el.querySelector('select')!.value).toBe('');
    expect(el.textContent).toContain(
      'Where it is filed for your browsing. Folders never change what anyone can wear.',
    );
  });

  it('General reads its folders without an id', async () => {
    const { seen } = await render({ scope: 'general', id: null }, '');
    expect(seen).toEqual([{ type: 'wardrobeFoldersList', scope: 'general' }]);
  });

  it('selects the value case-insensitively, and offers one the tree does not list', async () => {
    const known = await render(CHARACTER, 'formal/EVENING');
    expect(known.el.querySelector('select')!.value).toBe('Formal/Evening');
    expect(optionTexts(known.el)).toHaveLength(5);

    const fresh = await render(CHARACTER, 'Boaters');
    expect(optionTexts(fresh.el)).toEqual([
      'Wardrobe (top level)',
      'Boaters',
      'Formal',
      'Formal › Evening',
      'Hats',
      'New folder…',
    ]);
    expect(fresh.el.querySelector('select')!.value).toBe('Boaters');
  });

  it('emits the picked folder', async () => {
    const { el, fixture, emitted } = await render(CHARACTER, '');
    const select = el.querySelector('select')!;
    select.value = 'Hats';
    select.dispatchEvent(new Event('change'));
    fixture.detectChanges();
    expect(emitted).toEqual(['Hats']);
  });

  it('New folder… reveals the input without changing the value; typing emits; Pick returns', async () => {
    const { el, fixture, emitted } = await render(CHARACTER, '');
    const select = el.querySelector('select')!;
    select.value = '\u0000new';
    select.dispatchEvent(new Event('change'));
    fixture.detectChanges();
    expect(emitted).toEqual([]);
    const field = el.querySelector<HTMLInputElement>('input#wardrobe-folder')!;
    expect(field.placeholder).toBe('e.g., Formal/Evening');
    expect(document.activeElement).toBe(field);
    field.value = 'Formal/Opera';
    field.dispatchEvent(new Event('input'));
    expect(emitted).toEqual(['Formal/Opera']);
    const pick = [...el.querySelectorAll('button')].find((b) => b.textContent!.trim() === 'Pick')!;
    pick.click();
    fixture.detectChanges();
    expect(el.querySelector('select')).not.toBeNull();
  });

  it('Escape in the input returns to the select and does not reach the editor', async () => {
    const { el, fixture } = await render(CHARACTER, '');
    const select = el.querySelector('select')!;
    select.value = '\u0000new';
    select.dispatchEvent(new Event('change'));
    fixture.detectChanges();
    const outer = vi.fn();
    document.addEventListener('keydown', outer);
    el.querySelector('input')!.dispatchEvent(
      new KeyboardEvent('keydown', { key: 'Escape', bubbles: true }),
    );
    document.removeEventListener('keydown', outer);
    fixture.detectChanges();
    expect(outer).not.toHaveBeenCalled();
    expect(el.querySelector('select')).not.toBeNull();
  });

  it('a failed (or absent) read leaves the top level and New folder…', async () => {
    const warn = vi.spyOn(console, 'warn').mockImplementation(() => undefined);
    const failed = await render(CHARACTER, '', () => new Error('unknown request type'));
    expect(optionTexts(failed.el)).toEqual(['Wardrobe (top level)', 'New folder…']);
    expect(warn).toHaveBeenCalledWith(
      '[useWardrobeFolders] Failed to load wardrobe folders',
      expect.objectContaining({ tierKey: 'character:char-1' }),
    );

    const none = await render(null, '');
    expect(none.seen).toEqual([]);
    expect(optionTexts(none.el)).toEqual(['Wardrobe (top level)', 'New folder…']);
  });

  it('re-reads when the destination changes', async () => {
    const { fixture, seen } = await render(CHARACTER, '');
    fixture.componentRef.setInput('container', { scope: 'project', id: 'p1' });
    fixture.detectChanges();
    await settle(fixture);
    expect(seen.map((r) => [r['scope'], r['containerId']])).toEqual([
      ['character', 'char-1'],
      ['project', 'p1'],
    ]);
  });
});
