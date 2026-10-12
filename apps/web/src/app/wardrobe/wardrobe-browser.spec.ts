import { ChangeDetectionStrategy, Component, computed, signal } from '@angular/core';
import { ComponentFixture, TestBed } from '@angular/core/testing';
import { afterEach, describe, expect, it, vi } from 'vitest';

import { CoreClient } from '../core/core-client';
import type { CoreRequest, WardrobeItemDto } from '../core/core-contract';
import { ToastService } from '../ui/toast.service';
import { WardrobeBrowser } from './wardrobe-browser';
import type { WardrobeContainer } from './wardrobe-container';
import { createWardrobeItemActions, type WardrobeItemActions } from './wardrobe-item-actions';
import type { WardrobeListData } from './wardrobe-list-data';

/**
 * P4.D285 — v4 `__tests__/unit/components/wardrobe/wardrobe-browser.folders.
 * test.tsx` at `97f0d8cb8` (the nine cases, bug 229's included), transcribed
 * case for case against v4's fixtures and strings, plus v5 rows for the
 * browser's own controls (new folder, rename, delete-folder, the move toasts,
 * keyed view state).
 *
 * The harness mounts the browser inside a host wired the way the wardrobe
 * dialog wires it (v4's: a `BaseModal` with `closeOnClickOutside={!actions.
 * overlayOpen}`; v5's: a backdrop click and a document Escape gated by the
 * actions' `overlayOpen`), with the REAL actions store (R-O).
 *
 * **Gated: `P4D286_ROW_LANDED`.** Four of v4's cases drive affordances of the
 * ROW, which P4.D286 owns (C3 §5): the select checkbox (`Select <title>`), the
 * search crumb (`wardrobe-folder-crumb`) and the kebab's **Move to folder…**.
 * The browser binds them by the unifier's HANDOFF (status-log, P4.D285 unit
 * 6); the unifier applies it and flips this constant. Each gated case has an
 * ungated twin pinning the browser's own half today.
 */
const P4D286_ROW_LANDED = false;

afterEach(() => vi.restoreAllMocks());

const CHAR = 'char-1';
const OWN_ORIGIN = { scope: 'character' as const, id: CHAR, name: 'Alice' };
const PROJECT_ORIGIN = { scope: 'project' as const, id: 'proj-1', name: 'Blackwood' };

function item(
  id: string,
  title: string,
  folder: string,
  extra: Partial<WardrobeItemDto> = {},
): WardrobeItemDto {
  return {
    id,
    title,
    folder,
    types: ['top'],
    componentItemIds: [],
    isDefault: false,
    replace: false,
    characterId: CHAR,
    archivedAt: null,
    createdAt: '2026-01-01T00:00:00.000Z',
    updatedAt: '2026-01-01T00:00:00.000Z',
    origin: OWN_ORIGIN,
    ...extra,
  };
}

const COAT = item('coat', 'Greatcoat', '');
const GOWN = item('gown', 'Evening Gown', 'Formal/Evening');
const GLOVES = item('gloves', 'Opera Gloves', 'Formal');
const BORROWED = item('cape', 'Project Cape', 'Capes', { characterId: null, origin: PROJECT_ORIGIN });

const TREE = [
  { path: 'Formal', name: 'Formal', itemCount: 1 },
  { path: 'Formal/Evening', name: 'Evening', itemCount: 1 },
  { path: 'Rainy days', name: 'Rainy days', itemCount: 0 },
];

function makeData(
  items: WardrobeItemDto[],
  manage: (i: WardrobeItemDto) => boolean,
): WardrobeListData {
  const list = signal(items);
  return {
    listItems: list,
    resolutionPool: list,
    characterItems: signal([]),
    loading: signal(false),
    projectId: signal(null),
    canManage: manage,
    reload: async () => undefined,
  };
}

/** The dialog's close plumbing around the browser (`wardrobe-control-dialog.ts` onBackdrop / onEscape). */
@Component({
  selector: 'qt-browser-host',
  changeDetection: ChangeDetectionStrategy.OnPush,
  imports: [WardrobeBrowser],
  host: { '(document:keydown.escape)': 'onEscape()' },
  template: `
    <div class="qt-dialog-overlay" data-testid="dialog-backdrop" (click)="onBackdrop()">
      <div class="qt-dialog" role="dialog" (click)="$event.stopPropagation()">
        <qt-wardrobe-browser
          [container]="container()"
          [data]="data()"
          [actions]="actions()"
          [showArchived]="showArchived()"
          (showArchivedChange)="showArchived.set($event)"
        />
      </div>
    </div>
  `,
})
class BrowserHost {
  readonly container = signal<WardrobeContainer | null>(null);
  readonly data = signal<WardrobeListData>(makeData([], () => true));
  readonly actions = signal<WardrobeItemActions>(null as unknown as WardrobeItemActions);
  readonly showArchived = signal(false);
  readonly dialogClosed = vi.fn();
  readonly overlayOpen = computed(() => this.actions().overlayOpen());

  onBackdrop(): void {
    if (this.overlayOpen()) return;
    this.dialogClosed();
  }

  onEscape(): void {
    if (this.overlayOpen()) return;
    this.dialogClosed();
  }
}

type AnyRequest = CoreRequest & Record<string, unknown>;

async function settle(fixture: ComponentFixture<unknown>, ticks = 6): Promise<void> {
  for (let i = 0; i < ticks; i++) {
    await Promise.resolve();
    fixture.detectChanges();
  }
  await fixture.whenStable();
  fixture.detectChanges();
}

async function renderBrowser(
  container: WardrobeContainer,
  data: WardrobeListData,
  opts: {
    route?: (req: AnyRequest) => Record<string, unknown> | Error;
    confirm?: (message: string) => boolean;
  } = {},
) {
  const seen: AnyRequest[] = [];
  const route = opts.route ?? (() => ({ folders: TREE }));
  const core = {
    dispatchData: vi.fn(async (req: CoreRequest) => {
      const r = req as AnyRequest;
      seen.push(r);
      const out = r['type'] === 'wardrobeFoldersList' && !opts.route ? { folders: TREE } : route(r);
      if (out instanceof Error) throw out;
      return out;
    }),
  } as unknown as CoreClient;
  TestBed.resetTestingModule();
  TestBed.configureTestingModule({
    imports: [BrowserHost],
    providers: [{ provide: CoreClient, useValue: core }],
  });
  const toasts = TestBed.inject(ToastService);
  const showSuccess = vi.spyOn(toasts, 'showSuccess').mockImplementation(() => '');
  const showError = vi.spyOn(toasts, 'showError').mockImplementation(() => '');
  const confirm = vi.fn(opts.confirm ?? (() => true));
  const reloadLists = vi.fn(async () => undefined);
  const actions = createWardrobeItemActions({
    core,
    toasts,
    container: () => container,
    listItems: () => data.listItems(),
    confirm,
    reloadLists,
  });
  const fixture = TestBed.createComponent(BrowserHost);
  fixture.componentInstance.container.set(container);
  fixture.componentInstance.data.set(data);
  fixture.componentInstance.actions.set(actions);
  fixture.detectChanges();
  await settle(fixture);
  const el = fixture.nativeElement as HTMLElement;
  return { fixture, el, seen, actions, confirm, reloadLists, showSuccess, showError };
}

const characterContainer: WardrobeContainer = { scope: 'character', id: CHAR };
const projectContainer: WardrobeContainer = { scope: 'project', id: 'proj-1' };
const ownOnly = (i: WardrobeItemDto) => Boolean(i.characterId);

const folderRows = (el: HTMLElement) =>
  Array.from(el.querySelectorAll<HTMLElement>('[data-testid="wardrobe-folder-row"]'));
const tierRowsShown = (el: HTMLElement) =>
  Array.from(el.querySelectorAll<HTMLElement>('[data-testid="wardrobe-tier-row"]'));
const byTitle = (el: HTMLElement, title: string) => el.querySelector(`[title="${title}"]`);
const buttonNamed = (root: Element, name: string) =>
  Array.from(root.querySelectorAll<HTMLButtonElement>('button')).find(
    (b) => b.textContent?.trim() === name,
  );
const nav = (el: HTMLElement) => el.querySelector('nav[aria-label="Wardrobe folders"]') as HTMLElement;
const search = (el: HTMLElement) =>
  el.querySelector<HTMLInputElement>('input[aria-label="Search wardrobe by title"]')!;
function type(input: HTMLInputElement, value: string): void {
  input.value = value;
  input.dispatchEvent(new Event('input'));
}
const listbox = (el: HTMLElement) => el.querySelector<HTMLElement>('[role="listbox"][aria-label="Folders"]');
const option = (el: HTMLElement, re: RegExp) =>
  Array.from(listbox(el)!.querySelectorAll<HTMLButtonElement>('[role="option"]')).find((o) =>
    re.test(o.textContent ?? ''),
  )!;

describe('WardrobeBrowser — folders (v4 `wardrobe-browser.folders.test.tsx`)', () => {
  it('lists the root’s subfolders (empty ones included) first, then the root’s items only', async () => {
    const { el } = await renderBrowser(characterContainer, makeData([COAT, GOWN, GLOVES], ownOnly));
    expect(folderRows(el)).toHaveLength(2);
    expect(folderRows(el)[0].textContent).toContain('Formal');
    expect(folderRows(el)[1].textContent).toContain('Rainy days');
    expect(folderRows(el)[0].textContent).toContain('1 item');
    expect(byTitle(el, 'Greatcoat')).not.toBeNull();
    expect(byTitle(el, 'Evening Gown')).toBeNull();
    expect(byTitle(el, 'Opera Gloves')).toBeNull();
    // Folder rows come first.
    const list = folderRows(el)[0].parentElement!.parentElement!;
    const order = Array.from(list.children).map((c) => c.tagName.toLowerCase());
    expect(order.indexOf('qt-wardrobe-folder-row')).toBeLessThan(order.indexOf('qt-wardrobe-item-row'));
  });

  it('entering a folder lists its subfolders and its own items; the breadcrumb goes back', async () => {
    const { el, fixture } = await renderBrowser(
      characterContainer,
      makeData([COAT, GOWN, GLOVES], ownOnly),
    );
    buttonNamed(folderRows(el)[0], 'Formal')!.click();
    await settle(fixture);
    expect(byTitle(el, 'Opera Gloves')).not.toBeNull();
    expect(byTitle(el, 'Greatcoat')).toBeNull();
    expect(byTitle(el, 'Evening Gown')).toBeNull();
    expect(folderRows(el).map((r) => r.textContent)).toEqual([expect.stringContaining('Evening')]);
    // The crumb trail: Wardrobe › Formal, the last one current.
    const crumbs = Array.from(nav(el).querySelectorAll('button'));
    expect(crumbs.map((b) => b.textContent?.trim())).toEqual(['Wardrobe', 'Formal']);
    expect(crumbs[1].getAttribute('aria-current')).toBe('page');
    expect(crumbs[0].getAttribute('aria-current')).toBeNull();

    buttonNamed(nav(el), 'Wardrobe')!.click();
    await settle(fixture);
    expect(byTitle(el, 'Greatcoat')).not.toBeNull();
  });

  it('an empty folder says so', async () => {
    const { el, fixture } = await renderBrowser(characterContainer, makeData([COAT], ownOnly));
    expect(folderRows(el).length).toBeGreaterThan(0);
    buttonNamed(el, 'Rainy days')!.click();
    await settle(fixture);
    expect(el.textContent).toContain('Nothing hangs here yet.');
  });

  it.skipIf(!P4D286_ROW_LANDED)('a search ignores folders and shows each hit’s folder', async () => {
    const { el, fixture } = await renderBrowser(
      characterContainer,
      makeData([COAT, GOWN, GLOVES], ownOnly),
    );
    type(search(el), 'o');
    await settle(fixture);
    expect(byTitle(el, 'Evening Gown')).not.toBeNull();
    expect(byTitle(el, 'Opera Gloves')).not.toBeNull();
    expect(byTitle(el, 'Greatcoat')).not.toBeNull();
    expect(folderRows(el)).toHaveLength(0);
    expect(el.textContent).toContain('Searching everywhere');
    expect(
      Array.from(el.querySelectorAll('[data-testid="wardrobe-folder-crumb"]'))
        .map((c) => c.textContent?.trim())
        .sort(),
    ).toEqual(['Formal', 'Formal › Evening']);
  });

  it('(ungated twin) a search ignores folders and says it is searching everywhere', async () => {
    const { el, fixture } = await renderBrowser(
      characterContainer,
      makeData([COAT, GOWN, GLOVES], ownOnly),
    );
    type(search(el), 'o');
    await settle(fixture);
    expect(byTitle(el, 'Evening Gown')).not.toBeNull();
    expect(byTitle(el, 'Opera Gloves')).not.toBeNull();
    expect(byTitle(el, 'Greatcoat')).not.toBeNull();
    expect(folderRows(el)).toHaveLength(0);
    expect(nav(el).textContent?.trim()).toBe('Searching everywhere');
    // No New folder while searching; Select stays.
    expect(buttonNamed(el, 'New folder')).toBeUndefined();
    expect(buttonNamed(el, 'Select')).toBeDefined();
  });

  it('shows tier rows in a character’s container, never in a shared one', async () => {
    const { el, fixture } = await renderBrowser(
      characterContainer,
      makeData([COAT, BORROWED], ownOnly),
    );
    expect(tierRowsShown(el)).toHaveLength(1);
    expect(tierRowsShown(el)[0].textContent).toContain('Project: Blackwood');
    // The borrowed item is in its tier, not at the closet's root.
    expect(byTitle(el, 'Project Cape')).toBeNull();

    tierRowsShown(el)[0].click();
    await settle(fixture);
    // Inside the tier: its own folders, no filing controls.
    expect(folderRows(el)).toHaveLength(1);
    expect(folderRows(el)[0].textContent).toContain('Capes');
    expect(buttonNamed(el, 'New folder')).toBeUndefined();
    expect(buttonNamed(el, 'Select')).toBeUndefined();
  });

  it('a shared container lists no tier rows', async () => {
    const shared = { ...BORROWED, folder: '' };
    const { el } = await renderBrowser(projectContainer, makeData([shared], () => true));
    expect(byTitle(el, 'Project Cape')).not.toBeNull();
    expect(tierRowsShown(el)).toHaveLength(0);
  });

  it.skipIf(!P4D286_ROW_LANDED)(
    'select mode puts checkboxes only on manageable rows and shows the bar',
    async () => {
      const borrowedAtRoot = item('hat', 'Borrowed Hat', '', {
        characterId: null,
        origin: PROJECT_ORIGIN,
      });
      // A view where a borrowed row sits beside an own one (a search).
      const { el, fixture } = await renderBrowser(
        characterContainer,
        makeData([COAT, borrowedAtRoot], ownOnly),
      );
      buttonNamed(el, 'Select')!.click();
      type(search(el), 'a');
      await settle(fixture);
      expect(byTitle(el, 'Borrowed Hat')).not.toBeNull();
      const greatcoat = el.querySelector<HTMLInputElement>('[aria-label="Select Greatcoat"]');
      expect(greatcoat).not.toBeNull();
      expect(el.querySelector('[aria-label="Select Borrowed Hat"]')).toBeNull();
      greatcoat!.click();
      await settle(fixture);
      expect(el.querySelector('[data-testid="wardrobe-select-bar"]')?.textContent).toContain(
        '1 selected',
      );
    },
  );

  it('(ungated twin) Select toggles the sticky bar; leaving the view drops the selection', async () => {
    const { el, fixture } = await renderBrowser(characterContainer, makeData([COAT, GLOVES], ownOnly));
    const select = buttonNamed(el, 'Select')!;
    expect(select.getAttribute('aria-pressed')).toBe('false');
    select.click();
    await settle(fixture);
    expect(select.getAttribute('aria-pressed')).toBe('true');
    expect(select.classList.contains('qt-button-secondary')).toBe(true);
    const bar = el.querySelector<HTMLElement>('[data-testid="wardrobe-select-bar"]')!;
    expect(bar.textContent).toContain('0 selected');
    expect(bar.classList.contains('sticky')).toBe(true);
    expect(buttonNamed(bar, 'Move to folder…')!.disabled).toBe(true);
    buttonNamed(bar, 'Cancel')!.click();
    await settle(fixture);
    expect(el.querySelector('[data-testid="wardrobe-select-bar"]')).toBeNull();
  });

  it.skipIf(!P4D286_ROW_LANDED)('the picker disables the folder the item is already in', async () => {
    const { el, fixture } = await renderBrowser(characterContainer, makeData([COAT, GLOVES], ownOnly));
    buttonNamed(folderRows(el)[0], 'Formal')!.click();
    await settle(fixture);
    expect(byTitle(el, 'Opera Gloves')).not.toBeNull();

    el.querySelector<HTMLButtonElement>('button[aria-label="More actions"]')!.click();
    await settle(fixture);
    Array.from(el.querySelectorAll<HTMLButtonElement>('[role="menuitem"]'))
      .find((m) => m.textContent?.trim() === 'Move to folder…')!
      .click();
    await settle(fixture);

    expect(listbox(el)).not.toBeNull();
    expect(option(el, /Formal/).disabled).toBe(true);
    expect(option(el, /top level/).disabled).toBe(false);
    expect(buttonNamed(el, 'Move')!.disabled).toBe(true);
  });

  it('(ungated twin) a picker opened for one item disables its folder and names it', async () => {
    const { el, fixture, actions } = await renderBrowser(
      characterContainer,
      makeData([COAT, GLOVES], ownOnly),
    );
    actions.openFolderPicker([GLOVES]);
    await settle(fixture);
    expect(el.querySelector('.qt-dialog-title')?.textContent?.trim()).toBe(
      'Move "Opera Gloves" to a folder',
    );
    expect(option(el, /Formal/).disabled).toBe(true);
    expect(option(el, /top level/).disabled).toBe(false);
    expect(buttonNamed(el, 'Move')!.disabled).toBe(true);
  });

  it.skipIf(!P4D286_ROW_LANDED)(
    'a click inside the picker does not close the dialog around the browser (bug 229)',
    async () => {
      const { el, fixture } = await renderBrowser(characterContainer, makeData([COAT, GLOVES], ownOnly));
      buttonNamed(folderRows(el)[0], 'Formal')!.click();
      await settle(fixture);
      el.querySelector<HTMLButtonElement>('button[aria-label="More actions"]')!.click();
      await settle(fixture);
      Array.from(el.querySelectorAll<HTMLButtonElement>('[role="menuitem"]'))
        .find((m) => m.textContent?.trim() === 'Move to folder…')!
        .click();
      await settle(fixture);
      const topLevel = option(el, /top level/);
      topLevel.dispatchEvent(new MouseEvent('mousedown', { bubbles: true }));
      topLevel.click();
      await settle(fixture);
      expect(fixture.componentInstance.dialogClosed).not.toHaveBeenCalled();
      expect(listbox(el)).not.toBeNull();
      expect(buttonNamed(el, 'Move')!.disabled).toBe(false);
    },
  );

  it('(ungated twin, bug 229) a click and an Escape inside the picker leave the dialog open', async () => {
    const { el, fixture, actions } = await renderBrowser(
      characterContainer,
      makeData([COAT, GLOVES], ownOnly),
    );
    actions.openFolderPicker([GLOVES]);
    await settle(fixture);
    expect(fixture.componentInstance.overlayOpen()).toBe(true);
    const topLevel = option(el, /top level/);
    topLevel.dispatchEvent(new MouseEvent('mousedown', { bubbles: true }));
    topLevel.click();
    await settle(fixture);
    expect(fixture.componentInstance.dialogClosed).not.toHaveBeenCalled();
    expect(buttonNamed(el, 'Move')!.disabled).toBe(false);

    // ONE Escape closes the picker, and only the picker.
    document.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape' }));
    await settle(fixture);
    expect(listbox(el)).toBeNull();
    expect(actions.folderPicker()).toBeNull();
    expect(fixture.componentInstance.dialogClosed).not.toHaveBeenCalled();
    // With the picker gone, the next Escape is the dialog's.
    document.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape' }));
    expect(fixture.componentInstance.dialogClosed).toHaveBeenCalledTimes(1);
  });
});

describe('WardrobeBrowser — filing (v4 `WardrobeBrowser.tsx:249-381`)', () => {
  it('New folder makes the folder inside the one being browsed and says it is ready', async () => {
    const { el, fixture, seen, showSuccess, reloadLists } = await renderBrowser(
      characterContainer,
      makeData([COAT, GLOVES], ownOnly),
      {
        route: (req) =>
          req['type'] === 'wardrobeFolderCreate'
            ? { folder: { path: 'Formal/Gala', name: 'Gala', itemCount: 0 } }
            : { folders: TREE },
      },
    );
    buttonNamed(folderRows(el)[0], 'Formal')!.click();
    await settle(fixture);
    buttonNamed(el, 'New folder')!.click();
    await settle(fixture);
    expect(buttonNamed(el, 'New folder')!.disabled).toBe(true);
    const input = el.querySelector<HTMLInputElement>('input[aria-label="New folder name"]')!;
    expect(input.placeholder).toBe('Folder name — Enter to make it, Esc to think better of it');
    type(input, ' Gala ');
    input.dispatchEvent(new KeyboardEvent('keydown', { key: 'Enter' }));
    await settle(fixture);
    expect(seen.filter((r) => r['type'] === 'wardrobeFolderCreate')).toEqual([
      { type: 'wardrobeFolderCreate', scope: 'character', containerId: CHAR, path: 'Formal/Gala' },
    ]);
    expect(showSuccess).toHaveBeenCalledWith('The folder "Gala" is ready for hanging things in');
    expect(el.querySelector('input[aria-label="New folder name"]')).toBeNull();
    expect(reloadLists).toHaveBeenCalled();
  });

  it('a refused folder name toasts the server’s sentence and keeps the input; Escape closes it', async () => {
    const { el, fixture, showError, fixture: f } = await renderBrowser(
      characterContainer,
      makeData([COAT], ownOnly),
      {
        route: (req) =>
          req['type'] === 'wardrobeFolderCreate'
            ? new Error('"images" is reserved for item pictures and cannot be a wardrobe folder')
            : { folders: TREE },
      },
    );
    buttonNamed(el, 'New folder')!.click();
    await settle(fixture);
    const input = el.querySelector<HTMLInputElement>('input[aria-label="New folder name"]')!;
    type(input, 'images');
    input.dispatchEvent(new KeyboardEvent('keydown', { key: 'Enter' }));
    await settle(fixture);
    expect(showError).toHaveBeenCalledWith(
      '"images" is reserved for item pictures and cannot be a wardrobe folder',
    );
    expect(el.querySelector('input[aria-label="New folder name"]')).not.toBeNull();
    input.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape', bubbles: true }));
    await settle(f);
    expect(el.querySelector('input[aria-label="New folder name"]')).toBeNull();
    expect(f.componentInstance.dialogClosed).not.toHaveBeenCalled();
  });

  it('a folder row renames inline and toasts the new name', async () => {
    const { el, fixture, seen, showSuccess } = await renderBrowser(
      characterContainer,
      makeData([COAT, GLOVES], ownOnly),
      {
        route: (req) =>
          req['type'] === 'wardrobeFolderRename'
            ? { folder: { path: 'Dress', name: 'Dress', itemCount: 1 } }
            : { folders: TREE },
      },
    );
    const row = folderRows(el)[0];
    row.querySelector<HTMLButtonElement>('button[aria-label="Folder actions for Formal"]')!.click();
    await settle(fixture);
    buttonNamed(row, 'Rename')!.click();
    await settle(fixture);
    const input = row.querySelector<HTMLInputElement>('input[aria-label="Rename folder Formal"]')!;
    expect(input.value).toBe('Formal');
    type(input, 'Dress');
    input.dispatchEvent(new KeyboardEvent('keydown', { key: 'Enter' }));
    await settle(fixture);
    input.dispatchEvent(new Event('blur'));
    await settle(fixture);
    expect(seen.filter((r) => r['type'] === 'wardrobeFolderRename')).toEqual([
      { type: 'wardrobeFolderRename', scope: 'character', containerId: CHAR, path: 'Formal', name: 'Dress' },
    ]);
    expect(showSuccess).toHaveBeenCalledWith('Renamed to "Dress"');
  });

  it('delete-folder asks with v4’s sentence and reports the items moved up a peg', async () => {
    const { el, fixture, confirm, showSuccess, seen } = await renderBrowser(
      characterContainer,
      makeData([COAT, GOWN, GLOVES], ownOnly),
      {
        route: (req) =>
          req['type'] === 'wardrobeFolderDelete'
            ? { movedItems: 2, movedFolders: 1 }
            : { folders: TREE },
      },
    );
    const row = folderRows(el)[0];
    row.querySelector<HTMLButtonElement>('button[aria-label="Folder actions for Formal"]')!.click();
    await settle(fixture);
    buttonNamed(row, 'Delete')!.click();
    await settle(fixture);
    expect(confirm).toHaveBeenCalledWith(
      'Send its 2 items (and any archived pieces filed there) up to the top of the wardrobe? Only the folder itself goes.',
    );
    expect(seen.filter((r) => r['type'] === 'wardrobeFolderDelete')).toEqual([
      { type: 'wardrobeFolderDelete', scope: 'character', containerId: CHAR, path: 'Formal' },
    ]);
    expect(showSuccess).toHaveBeenCalledWith('Folder removed; 2 items moved up a peg');
  });

  it('delete-folder of an empty folder asks the other two ways, and a refusal deletes nothing', async () => {
    const { el, fixture, confirm, seen } = await renderBrowser(
      characterContainer,
      makeData([COAT], ownOnly),
      { confirm: () => false },
    );
    const row = folderRows(el).find((r) => r.textContent?.includes('Rainy days'))!;
    row.querySelector<HTMLButtonElement>('button[aria-label="Folder actions for Rainy days"]')!.click();
    await settle(fixture);
    buttonNamed(row, 'Delete')!.click();
    await settle(fixture);
    expect(confirm).toHaveBeenCalledWith(
      'Remove the folder "Rainy days"? Anything archived in it goes up to the top of the wardrobe.',
    );
    expect(seen.some((r) => r['type'] === 'wardrobeFolderDelete')).toBe(false);
  });

  it('moving one item names it; the picker closes and the toast says where it hangs', async () => {
    const { el, fixture, actions, showSuccess, seen } = await renderBrowser(
      characterContainer,
      makeData([COAT, GLOVES], ownOnly),
      {
        route: (req) =>
          req['type'] === 'wardrobeItemsMove'
            ? { items: [{ ...COAT, folder: 'Formal' }] }
            : { folders: TREE },
      },
    );
    actions.openFolderPicker([COAT]);
    await settle(fixture);
    option(el, /Formal/).click();
    await settle(fixture);
    buttonNamed(el, 'Move')!.click();
    await settle(fixture);
    expect(seen.filter((r) => r['type'] === 'wardrobeItemsMove')).toEqual([
      { type: 'wardrobeItemsMove', scope: 'character', containerId: CHAR, itemIds: ['coat'], folder: 'Formal' },
    ]);
    expect(showSuccess).toHaveBeenCalledWith('"Greatcoat" now hangs in "Formal"');
    expect(actions.folderPicker()).toBeNull();
  });

  it('moving several to the top level counts them; a refusal keeps the picker and toasts', async () => {
    const ok = await renderBrowser(characterContainer, makeData([COAT, GOWN, GLOVES], ownOnly), {
      route: (req) => (req['type'] === 'wardrobeItemsMove' ? { items: [GOWN, GLOVES] } : { folders: TREE }),
    });
    ok.actions.openFolderPicker([GOWN, GLOVES]);
    await settle(ok.fixture);
    expect(ok.el.querySelector('.qt-dialog-title')?.textContent?.trim()).toBe(
      'Move 2 items to a folder',
    );
    // From two different folders: no row is "here now".
    expect(ok.el.textContent).not.toContain('(here now)');
    option(ok.el, /top level/).click();
    await settle(ok.fixture);
    buttonNamed(ok.el, 'Move')!.click();
    await settle(ok.fixture);
    expect(ok.showSuccess).toHaveBeenCalledWith('2 items now hang in the top of the wardrobe');

    const bad = await renderBrowser(characterContainer, makeData([COAT, BORROWED], ownOnly), {
      route: (req) =>
        req['type'] === 'wardrobeItemsMove'
          ? new Error('Not in this wardrobe (shared items are filed where they live): cape')
          : { folders: TREE },
    });
    bad.actions.openFolderPicker([COAT]);
    await settle(bad.fixture);
    option(bad.el, /Formal/).click();
    await settle(bad.fixture);
    buttonNamed(bad.el, 'Move')!.click();
    await settle(bad.fixture);
    expect(bad.showError).toHaveBeenCalledWith(
      'Not in this wardrobe (shared items are filed where they live): cape',
    );
    expect(bad.actions.folderPicker()).not.toBeNull();
  });

  it('+ New Item files into the folder being browsed (the root inside a borrowed tier)', async () => {
    const { el, fixture, actions } = await renderBrowser(
      characterContainer,
      makeData([COAT, GLOVES, BORROWED], ownOnly),
    );
    buttonNamed(folderRows(el)[0], 'Formal')!.click();
    await settle(fixture);
    buttonNamed(el, '+ New Item')!.click();
    expect(actions.editor()).toEqual({
      kind: 'create',
      mode: 'single',
      componentItemIds: [],
      folder: 'Formal',
    });
    actions.closeEditor();
    buttonNamed(nav(el), 'Wardrobe')!.click();
    await settle(fixture);
    tierRowsShown(el)[0].click();
    await settle(fixture);
    buttonNamed(el, '+ New Item')!.click();
    expect(actions.editor()).toMatchObject({ kind: 'create', folder: '' });
  });

  it('view state remembers the container it was set in (v4 `:123-134`, no reset effect)', async () => {
    const { el, fixture } = await renderBrowser(characterContainer, makeData([COAT, GLOVES], ownOnly));
    const crumbs = () =>
      Array.from(nav(el).querySelectorAll('button')).map((b) => b.textContent?.trim());
    buttonNamed(folderRows(el)[0], 'Formal')!.click();
    await settle(fixture);
    expect(crumbs()).toEqual(['Wardrobe', 'Formal']);
    // Another container reads the view as its starting value: its root.
    fixture.componentInstance.container.set({ scope: 'character', id: 'other' });
    await settle(fixture);
    expect(crumbs()).toEqual(['Wardrobe']);
    // Nothing navigated there, so the first container's view is still its own.
    fixture.componentInstance.container.set(characterContainer);
    await settle(fixture);
    expect(crumbs()).toEqual(['Wardrobe', 'Formal']);
    // Navigating in another container takes the one view with it.
    fixture.componentInstance.container.set({ scope: 'character', id: 'other' });
    await settle(fixture);
    buttonNamed(nav(el), 'Wardrobe')!.click();
    fixture.componentInstance.container.set(characterContainer);
    await settle(fixture);
    expect(crumbs()).toEqual(['Wardrobe']);
  });

  it('a tier row carries its chip and live count; the tier crumb names it', async () => {
    const archivedBorrowed = item('veil', 'Old Veil', '', {
      characterId: null,
      origin: PROJECT_ORIGIN,
      archivedAt: '2026-01-01T00:00:00.000Z',
    });
    const { el, fixture } = await renderBrowser(
      characterContainer,
      makeData([COAT, BORROWED, archivedBorrowed], ownOnly),
    );
    const row = tierRowsShown(el)[0];
    expect(row.querySelector('.qt-badge')?.textContent?.trim()).toBe('Project');
    expect(row.textContent).toContain('1 item');
    row.click();
    await settle(fixture);
    expect(Array.from(nav(el).querySelectorAll('button')).map((b) => b.textContent?.trim())).toEqual([
      'Wardrobe',
      'Project: Blackwood',
    ]);
  });
});
