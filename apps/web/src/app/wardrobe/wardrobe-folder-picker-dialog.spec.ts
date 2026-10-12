import { ComponentFixture, TestBed } from '@angular/core/testing';
import { describe, expect, it, vi } from 'vitest';

import { CoreClient } from '../core/core-client';
import type { CoreRequest } from '../core/core-contract';
import { ToastService } from '../ui/toast.service';
import { WardrobeFolderPickerDialog } from './wardrobe-folder-picker-dialog';

/**
 * P4.D285 — v4 `components/wardrobe/WardrobeFolderPickerDialog.tsx`
 * (`4a38222e6`, unchanged by `97f0d8cb8`). v4 tests the picker through the
 * browser (`wardrobe-browser.folders.test.tsx` cases 8 and 9 — transcribed in
 * `wardrobe-browser.spec.ts`); these rows pin the component's own strings,
 * states and wire on their own: the title, the intro, the top-level-first
 * indented listbox, `(here now)` / `Already here`, Move / Moving… / Cancel,
 * New folder… under the highlighted row, and the Escape gate by `moving`.
 */

type AnyRequest = CoreRequest & Record<string, unknown>;

const TREE = [
  { path: 'Formal', name: 'Formal', itemCount: 1 },
  { path: 'Formal/Evening', name: 'Evening', itemCount: 1 },
  { path: 'Rainy days', name: 'Rainy days', itemCount: 0 },
];

async function settle(fixture: ComponentFixture<unknown>, ticks = 6): Promise<void> {
  for (let i = 0; i < ticks; i++) {
    await Promise.resolve();
    fixture.detectChanges();
  }
  await fixture.whenStable();
  fixture.detectChanges();
}

async function render(
  opts: {
    currentFolder?: string | null;
    moving?: boolean;
    subject?: string;
    route?: (req: AnyRequest) => Record<string, unknown> | Error | Promise<Record<string, unknown>>;
  } = {},
) {
  const seen: AnyRequest[] = [];
  const route = opts.route ?? (() => ({ folders: TREE }));
  const core = {
    dispatchData: vi.fn(async (req: CoreRequest) => {
      const r = req as AnyRequest;
      seen.push(r);
      const out = await route(r);
      if (out instanceof Error) throw out;
      return out;
    }),
  } as unknown as CoreClient;
  TestBed.resetTestingModule();
  TestBed.configureTestingModule({
    imports: [WardrobeFolderPickerDialog],
    providers: [{ provide: CoreClient, useValue: core }],
  });
  const toasts = TestBed.inject(ToastService);
  const showError = vi.spyOn(toasts, 'showError').mockImplementation(() => '');
  const fixture = TestBed.createComponent(WardrobeFolderPickerDialog);
  fixture.componentRef.setInput('container', { scope: 'character', id: 'alice' });
  fixture.componentRef.setInput('subject', opts.subject ?? '"Greatcoat"');
  fixture.componentRef.setInput(
    'currentFolder',
    opts.currentFolder === undefined ? '' : opts.currentFolder,
  );
  fixture.componentRef.setInput('moving', opts.moving ?? false);
  const closed = vi.fn();
  const moved = vi.fn();
  const changed = vi.fn();
  fixture.componentInstance.closed.subscribe(closed);
  fixture.componentInstance.move.subscribe(moved);
  fixture.componentInstance.foldersChanged.subscribe(changed);
  fixture.detectChanges();
  await settle(fixture);
  const el = fixture.nativeElement as HTMLElement;
  return { fixture, el, seen, closed, moved, changed, showError };
}

const options = (el: HTMLElement) =>
  Array.from(el.querySelectorAll<HTMLButtonElement>('[role="option"]'));
const byText = (el: HTMLElement, text: string) =>
  Array.from(el.querySelectorAll<HTMLButtonElement>('button')).find(
    (b) => b.textContent?.trim() === text,
  );

describe('WardrobeFolderPickerDialog (v4 `WardrobeFolderPickerDialog.tsx`)', () => {
  it('titles the subject, reads the tree for the container, and lists the top level first, indented', async () => {
    const { el, seen } = await render();
    expect(el.querySelector('.qt-dialog-title')?.textContent?.trim()).toBe(
      'Move "Greatcoat" to a folder',
    );
    expect(el.textContent).toContain(
      'Folders are for your browsing; what anyone can wear stays exactly as it was.',
    );
    expect(seen).toEqual([{ type: 'wardrobeFoldersList', scope: 'character', containerId: 'alice' }]);
    expect(el.querySelector('[role="listbox"]')?.getAttribute('aria-label')).toBe('Folders');
    const rows = options(el);
    expect(rows.map((r) => r.textContent?.replace(/\s+/g, ' ').trim())).toEqual([
      'Wardrobe (top level) (here now)',
      'Formal',
      'Evening',
      'Rainy days',
    ]);
    expect(rows.map((r) => r.style.paddingLeft)).toEqual(['8px', '24px', '40px', '24px']);
    // The folder icon marks folders, never the root row.
    expect(rows[0].querySelector('qt-icon')).toBeNull();
    expect(rows[1].querySelector('qt-icon')).not.toBeNull();
  });

  it('the folder the items share reads (here now), is disabled, and titled Already here', async () => {
    const { el } = await render({ currentFolder: 'formal' });
    const formal = options(el)[1];
    expect(formal.disabled).toBe(true);
    expect(formal.getAttribute('aria-disabled')).toBe('true');
    expect(formal.getAttribute('title')).toBe('Already here');
    expect(formal.textContent).toContain('(here now)');
    expect(options(el)[0].disabled).toBe(false);
    expect(options(el)[0].getAttribute('title')).toBeNull();
  });

  it('items from different folders (currentFolder null) leave every row choosable', async () => {
    const { el } = await render({ currentFolder: null });
    expect(options(el).every((o) => !o.disabled)).toBe(true);
    expect(el.textContent).not.toContain('(here now)');
  });

  it('Move waits for a highlighted, non-current row, then files there', async () => {
    const { el, fixture, moved } = await render({ currentFolder: 'Formal' });
    const move = byText(el, 'Move')!;
    expect(move.disabled).toBe(true);
    options(el)[3].click();
    fixture.detectChanges();
    expect(options(el)[3].getAttribute('aria-selected')).toBe('true');
    expect(options(el)[3].classList.contains('qt-bg-muted')).toBe(true);
    expect(move.disabled).toBe(false);
    move.click();
    expect(moved).toHaveBeenCalledWith('Rainy days');
  });

  it('moving: Moving…, Move and Cancel disabled, Escape and the backdrop ignored', async () => {
    const { el, fixture, closed } = await render({ moving: true });
    expect(byText(el, 'Moving…')?.disabled).toBe(true);
    expect(byText(el, 'Cancel')?.disabled).toBe(true);
    document.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape' }));
    (el.querySelector('.qt-dialog-overlay') as HTMLElement).click();
    fixture.detectChanges();
    expect(closed).not.toHaveBeenCalled();
  });

  it('Escape and Cancel close the picker when idle', async () => {
    const { el, closed } = await render();
    document.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape' }));
    expect(closed).toHaveBeenCalledTimes(1);
    byText(el, 'Cancel')!.click();
    expect(closed).toHaveBeenCalledTimes(2);
  });

  it('New folder… opens an input under the highlighted row and creates the folder there', async () => {
    const { el, fixture, seen, changed } = await render({
      route: (req) =>
        req['type'] === 'wardrobeFolderCreate'
          ? { folder: { path: 'Formal/Gala', name: 'Gala', itemCount: 0 } }
          : { folders: TREE },
    });
    options(el)[1].click();
    fixture.detectChanges();
    byText(el, 'New folder…')!.click();
    fixture.detectChanges();
    expect(byText(el, 'New folder…')!.disabled).toBe(true);
    const input = el.querySelector<HTMLInputElement>('input[aria-label="New folder name"]')!;
    expect(input.placeholder).toBe('Folder name');
    // Indented one level under the highlighted folder.
    expect((input.parentElement as HTMLElement).style.paddingLeft).toBe('40px');
    expect(options(el)[1].parentElement?.contains(input)).toBe(true);
    expect(byText(el, 'Create')!.disabled).toBe(true);
    input.value = 'Gala';
    input.dispatchEvent(new Event('input'));
    fixture.detectChanges();
    expect(byText(el, 'Create')!.disabled).toBe(false);
    input.dispatchEvent(new KeyboardEvent('keydown', { key: 'Enter' }));
    await settle(fixture);
    expect(seen.filter((r) => r['type'] === 'wardrobeFolderCreate')).toEqual([
      { type: 'wardrobeFolderCreate', scope: 'character', containerId: 'alice', path: 'Formal/Gala' },
    ]);
    expect(el.querySelector('input[aria-label="New folder name"]')).toBeNull();
    expect(changed).toHaveBeenCalledTimes(1);
    // The tree re-reads (v4 onSettled) — two reads in all.
    expect(seen.filter((r) => r['type'] === 'wardrobeFoldersList')).toHaveLength(2);
  });

  it('with nothing highlighted the input sits after the list; a refusal toasts the server’s sentence', async () => {
    const { el, fixture, showError, changed } = await render({
      route: (req) =>
        req['type'] === 'wardrobeFolderCreate'
          ? new Error('"images" is reserved for item pictures and cannot be a wardrobe folder')
          : { folders: TREE },
    });
    byText(el, 'New folder…')!.click();
    fixture.detectChanges();
    const input = el.querySelector<HTMLInputElement>('input[aria-label="New folder name"]')!;
    expect(input.closest('li')).toBeNull();
    expect((input.parentElement as HTMLElement).style.paddingLeft).toBe('24px');
    input.value = 'images';
    input.dispatchEvent(new Event('input'));
    fixture.detectChanges();
    byText(el, 'Create')!.click();
    await settle(fixture);
    expect(showError).toHaveBeenCalledWith(
      '"images" is reserved for item pictures and cannot be a wardrobe folder',
    );
    // A failure settles too: the host still refreshes.
    expect(changed).toHaveBeenCalledTimes(1);
    expect(el.querySelector('input[aria-label="New folder name"]')).not.toBeNull();
  });

  it('Escape inside the New folder input closes only the input', async () => {
    const { el, fixture, closed } = await render();
    byText(el, 'New folder…')!.click();
    fixture.detectChanges();
    const input = el.querySelector<HTMLInputElement>('input[aria-label="New folder name"]')!;
    input.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape', bubbles: true }));
    fixture.detectChanges();
    expect(el.querySelector('input[aria-label="New folder name"]')).toBeNull();
    expect(closed).not.toHaveBeenCalled();
  });

  it('shows Loading folders… until the first read lands', async () => {
    let release!: () => void;
    const gate = new Promise<void>((r) => (release = r));
    const { el, fixture } = await render({
      route: async () => {
        await gate;
        return { folders: TREE };
      },
    });
    expect(el.textContent).toContain('Loading folders…');
    expect(el.querySelector('[role="listbox"]')).toBeNull();
    release();
    await settle(fixture);
    expect(el.querySelector('[role="listbox"]')).not.toBeNull();
  });
});
